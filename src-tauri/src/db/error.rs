//! Erros de banco sanitizados. CLAUDE.md §7: "Erros de banco são
//! repassados ao usuário (ele precisa deles para depurar SQL), mas passam
//! por um sanitizador que remove host, usuário e senha." Nenhum erro deste
//! módulo pode chegar à UI sem passar por [`sanitize`].

use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DbError {
    #[error("falha ao conectar: {reason}")]
    ConnectionFailed { reason: String },

    #[error("erro de execução: {reason}")]
    QueryFailed {
        reason: String,
        code: Option<String>,
    },

    #[error("tempo limite excedido ({timeout_ms} ms)")]
    Timeout { timeout_ms: u64 },

    #[error("consulta cancelada")]
    Cancelled,

    #[error("erro interno do driver: {reason}")]
    Driver { reason: String },
}

impl DbError {
    pub fn connection_failed(reason: impl std::fmt::Display) -> Self {
        Self::ConnectionFailed {
            reason: sanitize(&reason.to_string()),
        }
    }

    pub fn query_failed(reason: impl std::fmt::Display, code: Option<String>) -> Self {
        Self::QueryFailed {
            reason: sanitize(&reason.to_string()),
            code,
        }
    }

    pub fn driver(reason: impl std::fmt::Display) -> Self {
        Self::Driver {
            reason: sanitize(&reason.to_string()),
        }
    }
}

/// Casa qualquer token no formato `esquema://resto-sem-espaço`, ex.:
/// `postgres://user:senha@host:5432/db`. Redige o token inteiro — nunca
/// tenta separar cirurgicamente usuário/senha de host, porque uma extração
/// errada é exatamente o tipo de bug que vaza credencial.
static URI_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z0-9+.\-]*://\S+").expect("regex válida"));

/// Casa pares `chave=valor` (estilo DSN libpq: `host=... user=... password=...`),
/// case-insensitive, e redige só o valor.
static KEY_VALUE_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(host|hostaddr|user|username|uid|password|pwd)\s*=\s*\S+")
        .expect("regex válida")
});

pub fn sanitize(raw: &str) -> String {
    let step1 = URI_TOKEN.replace_all(raw, "[dsn redigida]");
    let step2 = KEY_VALUE_TOKEN.replace_all(&step1, "$1=***");
    step2.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_postgres_dsn_uri() {
        let raw = "connection to postgres://appuser:s3cr3t@db.internal:5432/erp failed";
        let clean = sanitize(raw);
        assert!(!clean.contains("s3cr3t"));
        assert!(!clean.contains("db.internal"));
        assert!(!clean.contains("appuser"));
        assert!(clean.contains("connection to"));
        assert!(clean.contains("failed"));
    }

    #[test]
    fn redacts_libpq_style_key_value_dsn() {
        let raw = "could not connect: host=db.internal user=appuser password=s3cr3t dbname=erp";
        let clean = sanitize(raw);
        assert!(!clean.contains("s3cr3t"));
        assert!(!clean.contains("db.internal"));
        assert!(!clean.contains("appuser"));
        assert!(clean.contains("could not connect"));
    }

    #[test]
    fn preserves_sql_syntax_error_message() {
        let raw = r#"syntax error at or near "FORM""#;
        assert_eq!(sanitize(raw), raw);
    }

    #[test]
    fn full_error_message_never_contains_the_original_dsn() {
        let secret_dsn = "postgres://prod_user:hunter2@prod-db.example.internal:5432/erp";
        let raw = format!("failed to connect using {secret_dsn}: connection refused");
        let clean = sanitize(&raw);
        assert!(!clean.contains(secret_dsn));
        assert!(!clean.contains("hunter2"));
    }
}
