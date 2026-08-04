//! Driver MySQL. Implementa `db::driver::{Driver, Session}` sobre `sqlx`.
//! Nenhum tipo do `sqlx` atravessa a fronteira do trait — só
//! `CellValue`/`ResultSet`/`DbError` (CLAUDE.md §3).
//!
//! Ao contrário do Postgres (`db/postgres.rs`), o protocolo binário do
//! MySQL não exige que o tipo Rust do bind bata exatamente com o tipo da
//! coluna alvo: cada parâmetro de `COM_STMT_EXECUTE` carrega sua própria
//! tag de tipo, e o servidor faz a mesma coerção implícita que faria para
//! um literal SQL comum (`WHERE id = '5'` funciona contra uma coluna
//! `INT` igual a `WHERE id = 5`). Por isso não existe aqui um equivalente
//! ao `coerce_text_to_inferred_type` do Postgres — bind direto pelo tipo
//! Rust nativo do `Bind`, validado contra um MySQL real em
//! `tests/mysql_types.rs` (`text_bind_matches_typed_columns`).

use std::time::Instant;

use async_trait::async_trait;
use futures_util::StreamExt;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlRow};
use sqlx::{
    AssertSqlSafe, Column, ConnectOptions, Connection, Executor, Row, SqlSafeStr, Statement,
    TypeInfo, ValueRef,
};
use tokio_util::sync::CancellationToken;

use crate::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use crate::db::error::DbError;
use crate::db::value::{CellValue, ColumnMeta, ResultSet};
use crate::sql::{Dialect, ValidatedSql};

/// Resolve uma [`SecretRef`] para a senha real — mesmo padrão do
/// `PostgresDriver` (ver `db/postgres.rs`).
pub type SecretResolver = Box<dyn Fn(&SecretRef) -> Result<String, DbError> + Send + Sync>;

pub struct MySqlDriver {
    resolve_secret: SecretResolver,
}

impl MySqlDriver {
    pub fn new(
        resolve_secret: impl Fn(&SecretRef) -> Result<String, DbError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            resolve_secret: Box::new(resolve_secret),
        }
    }
}

#[async_trait]
impl Driver for MySqlDriver {
    fn dialect(&self) -> Dialect {
        Dialect::MySql
    }

    async fn connect(
        &self,
        cfg: &ConnectionConfig,
        secret: &SecretRef,
    ) -> Result<Box<dyn Session>, DbError> {
        let password = (self.resolve_secret)(secret)?;

        let options = MySqlConnectOptions::new()
            .host(&cfg.host)
            .port(cfg.port)
            .username(&cfg.username)
            .password(&password);
        let options = match &cfg.database {
            Some(db) => options.database(db),
            None => options,
        };

        let conn = options
            .connect()
            .await
            .map_err(DbError::connection_failed)?;

        Ok(Box::new(MySqlSession {
            conn,
            cfg: cfg.clone(),
            password,
            connection_id: None,
        }))
    }
}

pub struct MySqlSession {
    conn: MySqlConnection,
    cfg: ConnectionConfig,
    // Guardada só para abrir a conexão auxiliar de cancelamento
    // (`KILL QUERY`) sem precisar resolver o segredo de novo.
    password: String,
    connection_id: Option<u64>,
}

#[async_trait]
impl Session for MySqlSession {
    async fn begin_read_only(&mut self, limits: &Limits) -> Result<(), DbError> {
        // MySQL aceita `START TRANSACTION READ ONLY` direto, sem precisar
        // de `BEGIN` + `SET TRANSACTION` separados como o Postgres.
        self.conn
            .execute(AssertSqlSafe("START TRANSACTION READ ONLY"))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;

        // MAX_EXECUTION_TIME só se aplica a SELECT (MySQL 5.7.8+/8.x) — o
        // container de dev usa mysql:8.4.
        let timeout_ms = limits.timeout.as_millis();
        let set_timeout = format!("SET SESSION MAX_EXECUTION_TIME = {timeout_ms}");
        self.conn
            .execute(AssertSqlSafe(set_timeout))
            .await
            .map_err(|e| DbError::query_failed(e, None))?;

        let connection_id: u64 = sqlx::query_scalar(AssertSqlSafe("SELECT CONNECTION_ID()"))
            .fetch_one(&mut self.conn)
            .await
            .map_err(|e| DbError::query_failed(e, None))?;
        self.connection_id = Some(connection_id);

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

        // Metadados de coluna vêm de um prepare separado — mesmo motivo do
        // Postgres: funciona mesmo com zero linhas de resultado.
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
                    self.cancel_connection().await;
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

impl MySqlSession {
    /// Cancela a query em andamento via uma conexão auxiliar chamando
    /// `KILL QUERY <connection_id>` — o MySQL permite que o próprio usuário
    /// mate a própria conexão sem privilégio `PROCESS`/`SUPER`. Silencioso
    /// em caso de falha: o chamador já vai reportar `DbError::Cancelled`.
    async fn cancel_connection(&self) {
        let Some(connection_id) = self.connection_id else {
            return;
        };

        let options = MySqlConnectOptions::new()
            .host(&self.cfg.host)
            .port(self.cfg.port)
            .username(&self.cfg.username)
            .password(&self.password);
        let options = match &self.cfg.database {
            Some(db) => options.database(db),
            None => options,
        };

        if let Ok(mut aux) = options.connect().await {
            let kill = format!("KILL QUERY {connection_id}");
            let _ = aux.execute(AssertSqlSafe(kill)).await;
            let _ = aux.close().await;
        }
    }
}

/// Bind direto pelo tipo Rust nativo do `Bind` — ver comentário de módulo
/// sobre por que o MySQL não precisa da coerção por tipo inferido que o
/// Postgres precisa.
fn bind_all<'q>(
    mut query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    binds: &'q [Bind],
) -> Result<sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>, DbError> {
    for bind in binds {
        query = bind_one(query, bind)?;
    }
    Ok(query)
}

fn bind_one<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    bind: &'q Bind,
) -> Result<sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>, DbError> {
    let query = match bind {
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
    Ok(query)
}

fn decode_row(row: &MySqlRow) -> Result<Vec<CellValue>, DbError> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(idx, column)| decode_cell(row, idx, column.type_info().name()))
        .collect()
}

fn decode_cell(row: &MySqlRow, idx: usize, type_name: &str) -> Result<CellValue, DbError> {
    let raw = row
        .try_get_raw(idx)
        .map_err(|e| DbError::query_failed(e, None))?;
    if raw.is_null() {
        return Ok(CellValue::Null);
    }

    let map_err = |e: sqlx::Error| DbError::query_failed(e, None);

    let value = match type_name {
        "BOOLEAN" | "TINYINT(1)" => CellValue::Bool(row.try_get::<bool, _>(idx).map_err(map_err)?),
        "TINYINT" | "TINYINT UNSIGNED" | "SMALLINT" | "SMALLINT UNSIGNED" | "MEDIUMINT"
        | "MEDIUMINT UNSIGNED" | "INT" | "INT UNSIGNED" | "YEAR" => {
            CellValue::Int(row.try_get::<i64, _>(idx).map_err(map_err)?)
        }
        "BIGINT" | "BIGINT UNSIGNED" => {
            // BIGINT UNSIGNED pode exceder i64::MAX; sqlx decodifica como
            // i64 e estouraria — cai pro caminho de texto se não couber.
            match row.try_get::<i64, _>(idx) {
                Ok(v) => CellValue::Int(v),
                Err(_) => CellValue::Text(row.try_get::<String, _>(idx).map_err(map_err)?),
            }
        }
        "FLOAT" => CellValue::Float(row.try_get::<f32, _>(idx).map_err(map_err)? as f64),
        "DOUBLE" => CellValue::Float(row.try_get::<f64, _>(idx).map_err(map_err)?),
        // DECIMAL nunca passa por f64 (CLAUDE.md — comparação exata). O
        // MySQL permite DECIMAL(65,30), acima do teto do rust_decimal
        // (~28-29 dígitos significativos) — limitação conhecida, ainda não
        // observada em dado real de teste.
        "DECIMAL" => CellValue::Decimal(
            row.try_get::<sqlx::types::Decimal, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" => {
            CellValue::Text(row.try_get::<String, _>(idx).map_err(map_err)?)
        }
        "VARBINARY" | "BINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => {
            CellValue::Bytes(row.try_get::<Vec<u8>, _>(idx).map_err(map_err)?)
        }
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
        // DATETIME não carrega timezone; TIMESTAMP do MySQL é sempre
        // armazenado/convertido em UTC internamente e já chega assim no
        // fio — mapeado como TimestampTz por já ser um instante absoluto,
        // ao contrário do DATETIME (hora local sem fuso associado).
        "DATETIME" => CellValue::Timestamp(
            row.try_get::<chrono::NaiveDateTime, _>(idx)
                .map_err(map_err)?
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string(),
        ),
        "TIMESTAMP" => CellValue::TimestampTz(
            row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx)
                .map_err(map_err)?
                .to_rfc3339(),
        ),
        "JSON" => CellValue::Json(
            row.try_get::<serde_json::Value, _>(idx)
                .map_err(map_err)?
                .to_string(),
        ),
        // Tipo sem mapeamento dedicado (ex.: SET, GEOMETRY, BIT): tenta
        // texto; se não for compatível, degrada para um marcador legível
        // em vez de falhar a consulta inteira por uma coluna exótica.
        _ => match row.try_get::<String, _>(idx) {
            Ok(text) => CellValue::Text(text),
            Err(_) => CellValue::Text(format!("<{type_name}: valor não decodificado>")),
        },
    };

    Ok(value)
}
