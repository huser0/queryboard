//! Driver Oracle. Implementa `db::driver::{Driver, Session}` sobre o crate
//! `oracle` (ODPI-C/OCI — Rota B, CLAUDE.md §3). Nenhum tipo do `oracle`
//! atravessa a fronteira do trait — só `CellValue`/`ResultSet`/`DbError`.
//!
//! Decisão de rota: a Rota A preferida originalmente (`oracledb`, crate
//! Rust puro thin-mode) foi descartada depois de um spike real — o crate
//! (renomeado para `oraclemcp-driver-cx` em pleno processo de transferência
//! de nome para a Oracle Corp) exige Rust **nightly** para compilar
//! (`asupersync` puxa `#![feature(try_trait_v2)]` no feature-set `default`,
//! sem opção de desligar do lado do consumidor). A Rota B (`oracle`, crate
//! ODPI-C com 2.3M+ downloads) foi validada de ponta a ponta contra um
//! Oracle Database Free real: conexão, `NUMBER` de alta precisão, `CLOB`,
//! `DATE`/`TIMESTAMP WITH TIME ZONE`, `SET TRANSACTION READ ONLY`
//! bloqueando escrita, bind nomeado e bind posicional com placeholder
//! repetido (`:id + :id` com um valor só). Ver CLAUDE.md §3,
//! docs/adr/0001-camada-oracle.md e docs/adr/0006-camada-oracle-rota-b-odpic.md.
//!
//! O crate `oracle` carrega `libclntsh` (Oracle Instant Client) via dlopen
//! em **runtime**, não em link-time — buildar este módulo não exige
//! Instant Client instalado; só conectar de verdade exige.
//!
//! Todo o crate é síncrono (bloqueante — o ODPI-C por baixo é uma API C
//! bloqueante). Cada chamada roda dentro de `spawn_blocking`; a
//! `Connection` é `Send + Sync` (o próprio crate garante isso via
//! `AssertSend`/`AssertSync`), então o `Arc<Connection>` pode ser
//! compartilhado com a tarefa de cancelamento sem precisar de uma conexão
//! auxiliar separada como Postgres/MySQL — `break_execution()` interrompe
//! a chamada bloqueante em andamento na mesma conexão, de outra thread.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use oracle::sql_type::{OracleType, ToSql};
use oracle::{Connection, Connector};
use tokio_util::sync::CancellationToken;

use crate::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use crate::db::error::DbError;
use crate::db::value::{CellValue, ColumnMeta, ResultSet};
use crate::sql::{Dialect, ValidatedSql};

/// Resolve uma [`SecretRef`] para a senha real — mesmo padrão dos outros
/// drivers (ver `db/postgres.rs`).
pub type SecretResolver = Box<dyn Fn(&SecretRef) -> Result<String, DbError> + Send + Sync>;

pub struct OracleDriver {
    resolve_secret: SecretResolver,
}

impl OracleDriver {
    pub fn new(
        resolve_secret: impl Fn(&SecretRef) -> Result<String, DbError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            resolve_secret: Box::new(resolve_secret),
        }
    }
}

/// EasyConnect (`host:porta/serviço`) — cobre o caso comum sem exigir
/// `tnsnames.ora`. Quando `service_name` não é informado, cai para
/// `database` (mesmo campo usado pelos outros dialetos no formulário de
/// connection); resolução via `tnsnames.ora`/wallet fica para quando o
/// usuário tiver um ambiente real com esse arquivo para validar (ver
/// conversa com o usuário — ele confirmou que o ambiente final tem
/// `tnsnames.ora`, mas não tinha um disponível para testar agora).
fn connect_descriptor(cfg: &ConnectionConfig) -> String {
    let service = cfg
        .service_name
        .as_deref()
        .or(cfg.database.as_deref())
        .unwrap_or_default();
    format!("{}:{}/{}", cfg.host, cfg.port, service)
}

#[async_trait]
impl Driver for OracleDriver {
    fn dialect(&self) -> Dialect {
        Dialect::Oracle
    }

    async fn connect(
        &self,
        cfg: &ConnectionConfig,
        secret: &SecretRef,
    ) -> Result<Box<dyn Session>, DbError> {
        let password = (self.resolve_secret)(secret)?;
        let username = cfg.username.clone();
        let descriptor = connect_descriptor(cfg);

        let conn = tokio::task::spawn_blocking(move || {
            Connector::new(&username, &password, &descriptor)
                .connect()
                .map_err(DbError::connection_failed)
        })
        .await
        .map_err(DbError::driver)??;

        Ok(Box::new(OracleSession {
            conn: Arc::new(conn),
        }))
    }
}

pub struct OracleSession {
    conn: Arc<Connection>,
}

#[async_trait]
impl Session for OracleSession {
    async fn begin_read_only(&mut self, limits: &Limits) -> Result<(), DbError> {
        let conn = Arc::clone(&self.conn);
        let timeout = limits.timeout;
        tokio::task::spawn_blocking(move || {
            conn.set_call_timeout(Some(timeout))
                .map_err(|e| DbError::query_failed(e, None))?;
            // Tem que ser a primeira instrução da transação (CLAUDE.md §2).
            // `autocommit` é `false` por padrão no crate `oracle`, então
            // nenhuma escrita anterior poderia ter aberto a transação sem
            // que este seja o primeiro statement de verdade.
            conn.execute("SET TRANSACTION READ ONLY", &[])
                .map_err(|e| DbError::query_failed(e, None))?;
            Ok(())
        })
        .await
        .map_err(DbError::driver)?
    }

    async fn execute_select(
        &mut self,
        sql: &ValidatedSql,
        binds: &[Bind],
        limits: &Limits,
        cancel: CancellationToken,
    ) -> Result<ResultSet, DbError> {
        let started_at = Instant::now();
        let conn = Arc::clone(&self.conn);
        let sql_text = sql.as_str().to_string();
        let owned_binds: Vec<OwnedBind> = binds.iter().map(OwnedBind::from).collect();
        let max_rows = limits.max_rows;

        let mut query_task = tokio::task::spawn_blocking(move || {
            let boxed: Vec<Box<dyn ToSql>> =
                owned_binds.iter().map(OwnedBind::to_sql_box).collect();
            let refs: Vec<&dyn ToSql> = boxed.iter().map(AsRef::as_ref).collect();

            let result_set = conn
                .query(&sql_text, &refs)
                .map_err(|e| DbError::query_failed(e, None))?;

            let columns: Vec<ColumnMeta> = result_set
                .column_info()
                .iter()
                .map(|c| {
                    ColumnMeta::new(
                        c.name(),
                        format!("{:?}", c.oracle_type()),
                        Some(c.nullable()),
                    )
                })
                .collect();

            let mut rows = Vec::new();
            let mut truncated = false;
            for row in result_set {
                let row = row.map_err(|e| DbError::query_failed(e, None))?;
                if rows.len() >= max_rows {
                    truncated = true;
                    break;
                }
                rows.push(decode_row(&row)?);
            }

            Ok::<_, DbError>((columns, rows, truncated))
        });

        let (columns, rows, truncated) = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let conn_for_break = Arc::clone(&self.conn);
                let _ = tokio::task::spawn_blocking(move || conn_for_break.break_execution()).await;
                let _ = (&mut query_task).await;
                return Err(DbError::Cancelled);
            }
            joined = &mut query_task => {
                joined.map_err(DbError::driver)??
            }
        };

        Ok(ResultSet {
            columns,
            rows,
            truncated,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
        })
    }

    async fn rollback_and_close(self: Box<Self>) -> Result<(), DbError> {
        let conn = self.conn;
        tokio::task::spawn_blocking(move || {
            conn.rollback()
                .map_err(|e| DbError::query_failed(e, None))?;
            conn.close().map_err(DbError::driver)?;
            Ok(())
        })
        .await
        .map_err(DbError::driver)?
    }
}

/// Versão "dona" (owned) de um [`Bind`], pronta para virar `Box<dyn ToSql>`
/// dentro do `spawn_blocking` (o `oracle::sql_type::ToSql` exige referência
/// com o mesmo tempo de vida do array passado para `query`).
///
/// `Bind::Bool` vira `i64` (0/1): o `ToSql` nativo de `bool` no crate
/// mapeia para `BOOLEAN` do PL/SQL, que não é um tipo de coluna de tabela
/// real em SQL do Oracle (mesmo no 23c isso é opt-in) — tabelas Oracle
/// tradicionalmente representam booleano como `NUMBER(1)`/`CHAR(1)`, então
/// bindar como inteiro é o caminho compatível com o schema real do
/// usuário. `Bind::Decimal` vira texto: o crate não tem integração com
/// `rust_decimal`, e o Oracle já converte implicitamente um bind
/// `VARCHAR2` contra uma coluna `NUMBER` (mesmo racional do `DECIMAL` no
/// MySQL — ver `db/mysql.rs`), sem passar por `f64` e sem estourar a
/// precisão do `rust_decimal`.
enum OwnedBind {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

impl From<&Bind> for OwnedBind {
    fn from(bind: &Bind) -> Self {
        match bind {
            Bind::Null => OwnedBind::Null,
            Bind::Bool(b) => OwnedBind::Int(i64::from(*b)),
            Bind::Int(i) => OwnedBind::Int(*i),
            Bind::Decimal(s) => OwnedBind::Text(s.clone()),
            Bind::Float(f) => OwnedBind::Float(*f),
            Bind::Text(s) => OwnedBind::Text(s.clone()),
            Bind::Bytes(b) => OwnedBind::Bytes(b.clone()),
        }
    }
}

impl OwnedBind {
    fn to_sql_box(&self) -> Box<dyn ToSql> {
        match self {
            OwnedBind::Null => Box::new(None::<String>),
            OwnedBind::Int(i) => Box::new(*i),
            OwnedBind::Float(f) => Box::new(*f),
            OwnedBind::Text(s) => Box::new(s.clone()),
            OwnedBind::Bytes(b) => Box::new(b.clone()),
        }
    }
}

fn decode_row(row: &oracle::Row) -> Result<Vec<CellValue>, DbError> {
    row.column_info()
        .iter()
        .enumerate()
        .map(|(idx, col)| decode_cell(row, idx, col.oracle_type()))
        .collect()
}

fn format_timestamp(ts: &oracle::sql_type::Timestamp) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}",
        ts.year(),
        ts.month(),
        ts.day(),
        ts.hour(),
        ts.minute(),
        ts.second(),
        ts.nanosecond()
    )
}

fn format_timestamp_tz(ts: &oracle::sql_type::Timestamp) -> String {
    let offset_minutes = ts.tz_hour_offset() * 60 + ts.tz_minute_offset();
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let abs = offset_minutes.abs();
    format!(
        "{}{}:{:02}:{:02}",
        format_timestamp(ts),
        sign,
        abs / 60,
        abs % 60
    )
}

fn decode_cell(
    row: &oracle::Row,
    idx: usize,
    oracle_type: &OracleType,
) -> Result<CellValue, DbError> {
    // `oracle::Row::get` devolve `Option<T>` implicitamente via
    // `Option<T>: FromSql` — usamos isso para NULL em vez de checar antes,
    // já que o crate não expõe um "is_null" separado por índice.
    let map_err = |e: oracle::Error| DbError::query_failed(e, None);

    macro_rules! get_opt {
        ($ty:ty) => {
            row.get::<usize, Option<$ty>>(idx).map_err(map_err)?
        };
    }

    let value = match oracle_type {
        OracleType::Varchar2(_)
        | OracleType::NVarchar2(_)
        | OracleType::Char(_)
        | OracleType::NChar(_)
        | OracleType::Rowid
        | OracleType::CLOB
        | OracleType::NCLOB
        | OracleType::Long
        | OracleType::Json
        | OracleType::Xml => match get_opt!(String) {
            Some(s) => CellValue::Text(s),
            None => CellValue::Null,
        },
        OracleType::Raw(_) | OracleType::BLOB | OracleType::LongRaw | OracleType::BFILE => {
            match get_opt!(Vec<u8>) {
                Some(b) => CellValue::Bytes(b),
                None => CellValue::Null,
            }
        }
        OracleType::BinaryFloat => match get_opt!(f32) {
            Some(f) => CellValue::Float(f as f64),
            None => CellValue::Null,
        },
        OracleType::BinaryDouble => match get_opt!(f64) {
            Some(f) => CellValue::Float(f),
            None => CellValue::Null,
        },
        // NUMBER/FLOAT do Oracle nunca passam por f64/rust_decimal — pega
        // como string decimal canônica direto do driver (o crate `oracle`
        // já devolve a representação textual exata da precisão, validado
        // contra NUMBER(38,10) no spike). Precisão acima de ~28-29 dígitos
        // (teto do `rust_decimal`) fica documentada como limitação
        // conhecida caso algum consumidor rio abaixo tente reparsear como
        // `rust_decimal::Decimal` em vez de tratar como string opaca.
        OracleType::Number(_, _) | OracleType::Float(_) => match get_opt!(String) {
            Some(s) => CellValue::Decimal(s),
            None => CellValue::Null,
        },
        OracleType::Boolean => match get_opt!(bool) {
            Some(b) => CellValue::Bool(b),
            None => CellValue::Null,
        },
        // DATE do Oracle inclui hora (não é só data) — mapeado como
        // Timestamp, nunca Date, para não perder a componente de hora.
        OracleType::Date | OracleType::Timestamp(_) => {
            match get_opt!(oracle::sql_type::Timestamp) {
                Some(ts) => CellValue::Timestamp(format_timestamp(&ts)),
                None => CellValue::Null,
            }
        }
        OracleType::TimestampTZ(_) | OracleType::TimestampLTZ(_) => {
            match get_opt!(oracle::sql_type::Timestamp) {
                Some(ts) => CellValue::TimestampTz(format_timestamp_tz(&ts)),
                None => CellValue::Null,
            }
        }
        // Tipo sem mapeamento dedicado (ex.: IntervalDS/IntervalYM — o
        // crate não expõe `String: FromSql` para eles, então cai no
        // fallback de texto abaixo —, Object, RefCursor, Vector):
        // tenta texto; se não for compatível, degrada para um marcador
        // legível em vez de falhar a consulta inteira por uma única
        // coluna exótica — mesmo padrão do Postgres/MySQL.
        _ => match row.get::<usize, Option<String>>(idx) {
            Ok(Some(s)) => CellValue::Text(s),
            Ok(None) => CellValue::Null,
            Err(_) => CellValue::Text(format!("<{oracle_type:?}: não decodificado>")),
        },
    };

    Ok(value)
}
