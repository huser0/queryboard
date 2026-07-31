//! Trait `Driver`/`Session` — o único ponto de contato entre o resto da
//! aplicação e um banco real. Isola a decisão Rota A (crate Rust puro) vs
//! Rota B (sidecar) descrita em CLAUDE.md §3 e docs/adr/0001-camada-oracle.md:
//! trocar de rota deve custar um módulo novo atrás deste trait, nunca uma
//! refatoração do resto do código.
//!
//! Nenhum tipo de driver específico (`sqlx::PgRow`, etc.) atravessa esta
//! fronteira — só os tipos de `crate::db::value` e `crate::db::error`.

use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::db::error::DbError;
use crate::db::value::ResultSet;
use crate::sql::{Dialect, ValidatedSql};

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub dialect: Dialect,
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub service_name: Option<String>,
    pub username: String,
}

/// Referência opaca a uma credencial no keyring do SO — nunca carrega o
/// segredo em si (ver `secrets.rs`, item 3.5 do roadmap). O `Debug`
/// nunca imprime o identificador real, para que um `dbg!()` ou log
/// acidental não vaze nada útil.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretRef(String);

impl SecretRef {
    pub fn new(connection_id: impl Into<String>) -> Self {
        Self(connection_id.into())
    }

    pub fn connection_id(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretRef(<redacted>)")
    }
}

/// Tetos aplicados por execução. `max_rows`/`fetch_size` são sempre
/// aplicados via fetch limitado no cursor, nunca injetando `LIMIT` na SQL
/// do usuário (CLAUDE.md §6). `fetch_size` costuma ser menor para a grade
/// da UI e maior para um passo intermediário de flow.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_rows: usize,
    pub timeout: Duration,
    pub fetch_size: usize,
}

impl Limits {
    pub const DEFAULT_MAX_ROWS: usize = 1000;
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
    pub const DEFAULT_FETCH_SIZE: usize = 100;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: Self::DEFAULT_MAX_ROWS,
            timeout: Self::DEFAULT_TIMEOUT,
            fetch_size: Self::DEFAULT_FETCH_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Null,
    Bool(bool),
    Int(i64),
    Decimal(String),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn dialect(&self) -> Dialect;

    async fn connect(
        &self,
        cfg: &ConnectionConfig,
        secret: &SecretRef,
    ) -> Result<Box<dyn Session>, DbError>;
}

#[async_trait]
pub trait Session: Send {
    /// `SET TRANSACTION READ ONLY` (ou equivalente do dialeto) + timeout
    /// de sessão. Sempre chamado logo após `connect`, antes de qualquer
    /// `execute_select`.
    async fn begin_read_only(&mut self, limits: &Limits) -> Result<(), DbError>;

    /// `sql` só é construível por `sql::guard::validate` — é impossível,
    /// em nível de tipo, chamar isto com SQL não validada.
    async fn execute_select(
        &mut self,
        sql: &ValidatedSql,
        binds: &[Bind],
        limits: &Limits,
        cancel: CancellationToken,
    ) -> Result<ResultSet, DbError>;

    /// Sempre `ROLLBACK`, nunca `COMMIT` — mesmo numa sessão somente
    /// leitura, encerrar com commit é o tipo de detalhe que uma revisão
    /// de código pode deixar passar (CLAUDE.md §2.2).
    async fn rollback_and_close(self: Box<Self>) -> Result<(), DbError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_ref_debug_never_prints_the_connection_id() {
        let secret = SecretRef::new("super-sensitive-connection-uuid");
        let debug_output = format!("{secret:?}");
        assert!(!debug_output.contains("super-sensitive-connection-uuid"));
    }
}
