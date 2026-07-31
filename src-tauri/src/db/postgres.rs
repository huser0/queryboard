//! Driver Postgres. Implementa `db::driver::{Driver, Session}` sobre
//! `sqlx`. Nenhum tipo do `sqlx` atravessa a fronteira do trait — só
//! `CellValue`/`ResultSet`/`DbError` (CLAUDE.md §3).

use std::time::Instant;

use async_trait::async_trait;
use futures_util::StreamExt;
use sqlx::postgres::{PgConnectOptions, PgConnection, PgRow};
use sqlx::{
    AssertSqlSafe, Column, ConnectOptions, Connection, Executor, Row, SqlSafeStr, Statement,
    TypeInfo, ValueRef,
};
use tokio_util::sync::CancellationToken;

use crate::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use crate::db::error::DbError;
use crate::db::value::{CellValue, ColumnMeta, ResultSet};
use crate::sql::{Dialect, ValidatedSql};

/// Resolve uma [`SecretRef`] para a senha real. Injetado na construção do
/// driver — a implementação de verdade (keyring do SO) chega no item 3.5
/// do roadmap; isolar isto num closure evita que o driver precise saber
/// como segredos são guardados.
pub type SecretResolver = Box<dyn Fn(&SecretRef) -> Result<String, DbError> + Send + Sync>;

pub struct PostgresDriver {
    resolve_secret: SecretResolver,
}

impl PostgresDriver {
    pub fn new(
        resolve_secret: impl Fn(&SecretRef) -> Result<String, DbError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            resolve_secret: Box::new(resolve_secret),
        }
    }
}

#[async_trait]
impl Driver for PostgresDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }

    async fn connect(
        &self,
        cfg: &ConnectionConfig,
        secret: &SecretRef,
    ) -> Result<Box<dyn Session>, DbError> {
        let password = (self.resolve_secret)(secret)?;

        let options = PgConnectOptions::new()
            .host(&cfg.host)
            .port(cfg.port)
            .username(&cfg.username)
            .password(&password)
            .application_name("queryboard");
        let options = match &cfg.database {
            Some(db) => options.database(db),
            None => options,
        };

        let conn = options
            .connect()
            .await
            .map_err(DbError::connection_failed)?;

        Ok(Box::new(PostgresSession {
            conn,
            cfg: cfg.clone(),
            password,
            backend_pid: None,
        }))
    }
}

pub struct PostgresSession {
    conn: PgConnection,
    cfg: ConnectionConfig,
    // Guardada só para abrir a conexão auxiliar de cancelamento
    // (`pg_cancel_backend`) sem precisar resolver o segredo de novo.
    password: String,
    backend_pid: Option<i32>,
}

#[async_trait]
impl Session for PostgresSession {
    async fn begin_read_only(&mut self, limits: &Limits) -> Result<(), DbError> {
        self.conn
            .execute(AssertSqlSafe("BEGIN"))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;
        self.conn
            .execute(AssertSqlSafe("SET TRANSACTION READ ONLY"))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;

        let timeout_ms = limits.timeout.as_millis();
        let set_timeout = format!("SET LOCAL statement_timeout = {timeout_ms}");
        self.conn
            .execute(AssertSqlSafe(set_timeout))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;

        let pid: i32 = sqlx::query_scalar(AssertSqlSafe("SELECT pg_backend_pid()"))
            .fetch_one(&mut self.conn)
            .await
            .map_err(|e| DbError::query_failed(e, None))?;
        self.backend_pid = Some(pid);

        Ok(())
    }

    async fn execute_select(
        &mut self,
        sql: &ValidatedSql,
        binds: &[Bind],
        limits: &Limits,
        cancel: CancellationToken,
    ) -> Result<ResultSet, DbError> {
        let started_at = Instant::now();

        // Metadados de coluna vêm de um prepare separado — funciona mesmo
        // quando o resultado tem zero linhas, o que uma linha de exemplo
        // não garantiria.
        let statement = self
            .conn
            .prepare(AssertSqlSafe(sql.as_str()).into_sql_str())
            .await
            .map_err(|e| DbError::query_failed(e, None))?;
        let columns: Vec<ColumnMeta> = statement
            .columns()
            .iter()
            .map(|c| ColumnMeta::new(c.name(), c.type_info().name(), None))
            .collect();

        let query = bind_all(sqlx::query(AssertSqlSafe(sql.as_str())), binds)?;
        let mut stream = query.fetch(&mut self.conn);

        let mut rows = Vec::with_capacity(limits.max_rows.min(limits.fetch_size));
        let mut truncated = false;

        loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    drop(stream);
                    self.cancel_backend().await;
                    return Err(DbError::Cancelled);
                }
                next = stream.next() => next,
            };

            let Some(row) = next
                .transpose()
                .map_err(|e| DbError::query_failed(e, None))?
            else {
                break;
            };

            if rows.len() >= limits.max_rows {
                truncated = true;
                break;
            }
            rows.push(decode_row(&row)?);
        }
        drop(stream);

        Ok(ResultSet {
            columns,
            rows,
            truncated,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
        })
    }

    async fn rollback_and_close(mut self: Box<Self>) -> Result<(), DbError> {
        self.conn
            .execute(AssertSqlSafe("ROLLBACK"))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;
        self.conn.close().await.map_err(DbError::driver)?;
        Ok(())
    }
}

impl PostgresSession {
    /// Cancela a query em andamento via uma conexão auxiliar chamando
    /// `pg_cancel_backend(pid)` — a mesma técnica funciona com qualquer
    /// cliente Postgres, não depende de nenhuma API nativa do sqlx.
    /// Silencioso em caso de falha: o chamador já vai reportar
    /// `DbError::Cancelled` de qualquer forma.
    async fn cancel_backend(&self) {
        let Some(pid) = self.backend_pid else {
            return;
        };

        let options = PgConnectOptions::new()
            .host(&self.cfg.host)
            .port(self.cfg.port)
            .username(&self.cfg.username)
            .password(&self.password)
            .application_name("queryboard-cancel");
        let options = match &self.cfg.database {
            Some(db) => options.database(db),
            None => options,
        };

        if let Ok(mut aux) = options.connect().await {
            let _ = sqlx::query(AssertSqlSafe("SELECT pg_cancel_backend($1)"))
                .bind(pid)
                .execute(&mut aux)
                .await;
            let _ = aux.close().await;
        }
    }
}

fn bind_all<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    binds: &'q [Bind],
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>, DbError> {
    for bind in binds {
        query = match bind {
            // Bind nulo sem tipo declarado ainda não é suportado de forma
            // geral — depende dos metadados de parâmetro da Query (item
            // 3.5 do roadmap). Por ora, assume texto.
            Bind::Null => query.bind(None::<String>),
            Bind::Bool(b) => query.bind(*b),
            Bind::Int(i) => query.bind(*i),
            Bind::Decimal(s) => {
                let decimal = s
                    .parse::<sqlx::types::Decimal>()
                    .map_err(|e| DbError::driver(format!("decimal inválido para bind: {e}")))?;
                query.bind(decimal)
            }
            Bind::Float(f) => query.bind(*f),
            Bind::Text(s) => query.bind(s.clone()),
            Bind::Bytes(b) => query.bind(b.clone()),
        };
    }
    Ok(query)
}

fn decode_row(row: &PgRow) -> Result<Vec<CellValue>, DbError> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(idx, column)| decode_cell(row, idx, column.type_info().name()))
        .collect()
}

fn decode_cell(row: &PgRow, idx: usize, type_name: &str) -> Result<CellValue, DbError> {
    let raw = row
        .try_get_raw(idx)
        .map_err(|e| DbError::query_failed(e, None))?;
    if raw.is_null() {
        return Ok(CellValue::Null);
    }

    let map_err = |e: sqlx::Error| DbError::query_failed(e, None);

    let value = match type_name {
        "BOOL" => CellValue::Bool(row.try_get::<bool, _>(idx).map_err(map_err)?),
        "INT2" => CellValue::Int(row.try_get::<i16, _>(idx).map_err(map_err)? as i64),
        "INT4" => CellValue::Int(row.try_get::<i32, _>(idx).map_err(map_err)? as i64),
        "INT8" => CellValue::Int(row.try_get::<i64, _>(idx).map_err(map_err)?),
        "FLOAT4" => CellValue::Float(row.try_get::<f32, _>(idx).map_err(map_err)? as f64),
        "FLOAT8" => CellValue::Float(row.try_get::<f64, _>(idx).map_err(map_err)?),
        // NUMERIC nunca passa por f64/rust_decimal (que satura por volta de
        // 28-29 dígitos significativos, insuficiente para NUMBER(38,x)) —
        // decodifica o formato binário do protocolo direto. Ver PLAN.md D6.
        "NUMERIC" => {
            let bytes = raw.as_bytes().map_err(DbError::driver)?;
            CellValue::Decimal(decode_pg_numeric(bytes)?)
        }
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" => {
            CellValue::Text(row.try_get::<String, _>(idx).map_err(map_err)?)
        }
        "UUID" => CellValue::Text(
            row.try_get::<sqlx::types::Uuid, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        "BYTEA" => CellValue::Bytes(row.try_get::<Vec<u8>, _>(idx).map_err(map_err)?),
        "DATE" => CellValue::Date(
            row.try_get::<chrono::NaiveDate, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        "TIME" => CellValue::Time(
            row.try_get::<chrono::NaiveTime, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        "TIMESTAMP" => CellValue::Timestamp(
            row.try_get::<chrono::NaiveDateTime, _>(idx)
                .map_err(map_err)?
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string(),
        ),
        "TIMESTAMPTZ" => CellValue::TimestampTz(
            row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx)
                .map_err(map_err)?
                .to_rfc3339(),
        ),
        "JSON" | "JSONB" => CellValue::Json(
            row.try_get::<serde_json::Value, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        "INTERVAL" => {
            let interval = row
                .try_get::<sqlx::postgres::types::PgInterval, _>(idx)
                .map_err(map_err)?;
            CellValue::Interval(format!(
                "{} mon {} days {} us",
                interval.months, interval.days, interval.microseconds
            ))
        }
        // Tipo sem mapeamento dedicado (ex.: array, range, tipo de
        // domínio custom): tenta decodificar como texto; se o tipo não
        // for compatível com decode textual (a maioria dos arrays não
        // é), degrada para um marcador legível em vez de falhar a
        // consulta inteira por causa de uma única coluna exótica.
        _ => match row.try_get::<String, _>(idx) {
            Ok(text) => CellValue::Text(text),
            Err(_) => {
                let byte_len = raw.as_bytes().map(<[u8]>::len).unwrap_or(0);
                CellValue::Text(format!("<{type_name}: {byte_len} bytes não decodificados>"))
            }
        },
    };

    Ok(value)
}

/// Decodifica o formato binário de `NUMERIC` do protocolo Postgres para
/// uma string decimal canônica, sem intermediário de precisão limitada.
/// Formato: 2 bytes ndigits, 2 bytes weight (i16), 2 bytes sign, 2 bytes
/// dscale, seguidos de `ndigits` grupos base-10000 (i16 big-endian) —
/// documentado em `backend/utils/adt/numeric.c` do Postgres.
fn decode_pg_numeric(bytes: &[u8]) -> Result<String, DbError> {
    const SIGN_NEG: u16 = 0x4000;
    const SIGN_NAN: u16 = 0xC000;

    if bytes.len() < 8 {
        return Err(DbError::driver("NUMERIC binário truncado"));
    }
    let num_digits = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let weight = i16::from_be_bytes([bytes[2], bytes[3]]) as i32;
    let sign = u16::from_be_bytes([bytes[4], bytes[5]]);
    let scale = i16::from_be_bytes([bytes[6], bytes[7]]) as i32;

    if sign == SIGN_NAN {
        return Err(DbError::driver("NUMERIC NaN não é suportado"));
    }

    let mut digits = Vec::with_capacity(num_digits);
    let mut cursor = 8usize;
    for _ in 0..num_digits {
        if cursor + 2 > bytes.len() {
            return Err(DbError::driver("NUMERIC binário truncado"));
        }
        digits.push(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        cursor += 2;
    }

    let digit_at = |pos: i32| -> u16 {
        let idx = weight - pos;
        if idx >= 0 && (idx as usize) < digits.len() {
            digits[idx as usize]
        } else {
            0
        }
    };

    let mut int_part = String::new();
    if weight >= 0 {
        for pos in (0..=weight).rev() {
            let group = digit_at(pos);
            if int_part.is_empty() {
                int_part.push_str(&group.to_string());
            } else {
                int_part.push_str(&format!("{group:04}"));
            }
        }
    }
    if int_part.is_empty() {
        int_part.push('0');
    }

    let scale_nonneg = scale.max(0);
    let mut frac_full = String::new();
    if scale_nonneg > 0 {
        let frac_groups = (scale_nonneg + 3) / 4;
        for i in 1..=frac_groups {
            frac_full.push_str(&format!("{:04}", digit_at(-i)));
        }
    }
    let scale_usize = scale_nonneg as usize;
    while frac_full.len() < scale_usize {
        frac_full.push('0');
    }
    let frac_part = &frac_full[..scale_usize];

    let is_zero = int_part == "0" && frac_part.bytes().all(|b| b == b'0');
    let sign_str = if sign == SIGN_NEG && !is_zero {
        "-"
    } else {
        ""
    };

    if scale_usize == 0 {
        Ok(format!("{sign_str}{int_part}"))
    } else {
        Ok(format!("{sign_str}{int_part}.{frac_part}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric_bytes(
        num_digits: u16,
        weight: i16,
        sign: u16,
        scale: i16,
        digits: &[u16],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&num_digits.to_be_bytes());
        bytes.extend_from_slice(&weight.to_be_bytes());
        bytes.extend_from_slice(&sign.to_be_bytes());
        bytes.extend_from_slice(&scale.to_be_bytes());
        for d in digits {
            bytes.extend_from_slice(&d.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn decodes_zero() {
        let bytes = numeric_bytes(0, 0, 0x0000, 0, &[]);
        assert_eq!(decode_pg_numeric(&bytes).unwrap(), "0");
    }

    #[test]
    fn decodes_123_45() {
        // 123.45 -> grupos [123, 4500], weight=0, scale=2
        let bytes = numeric_bytes(2, 0, 0x0000, 2, &[123, 4500]);
        assert_eq!(decode_pg_numeric(&bytes).unwrap(), "123.45");
    }

    #[test]
    fn decodes_small_fraction_0_001() {
        // 0.001 -> grupo [10] na posição -1, weight=-1, scale=3
        let bytes = numeric_bytes(1, -1, 0x0000, 3, &[10]);
        assert_eq!(decode_pg_numeric(&bytes).unwrap(), "0.001");
    }

    #[test]
    fn decodes_negative_value() {
        let bytes = numeric_bytes(1, 0, 0x4000, 0, &[42]);
        assert_eq!(decode_pg_numeric(&bytes).unwrap(), "-42");
    }

    #[test]
    fn decodes_value_exceeding_rust_decimal_precision() {
        // 123456789012345678901234567890 — 30 dígitos inteiros, acima do
        // que rust_decimal (28-29 dígitos) conseguiria representar sem
        // perda. Agrupado em base-10000 a partir da direita: 12 | 3456 |
        // 7890 | 1234 | 5678 | 9012 | 3456 | 7890 (grupo líder tem só 2
        // dígitos, os demais são sempre 4).
        let digits: Vec<u16> = vec![12, 3456, 7890, 1234, 5678, 9012, 3456, 7890];
        let weight = (digits.len() as i16) - 1;
        let bytes = numeric_bytes(digits.len() as u16, weight, 0x0000, 0, &digits);
        let decoded = decode_pg_numeric(&bytes).unwrap();
        assert_eq!(decoded, "123456789012345678901234567890");
    }

    #[test]
    fn rejects_nan() {
        let bytes = numeric_bytes(0, 0, 0xC000, 0, &[]);
        assert!(decode_pg_numeric(&bytes).is_err());
    }
}
