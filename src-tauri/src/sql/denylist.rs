//! Funções proibidas por dialeto — mesmo dentro de um `SELECT` sintaticamente
//! válido, algumas funções têm efeito colateral (I/O, controle de sessão,
//! sequences) e não são leitura pura. Ver CLAUDE.md §2.
//!
//! A checagem é feita sobre o nome normalizado (minúsculo) do último
//! segmento do nome qualificado, e sobre prefixos de schema conhecidos por
//! concentrar funções administrativas (`dbms_*`, `utl_*`, `sys.*`, ...).

use crate::sql::Dialect;

/// Nomes completos (podem ter mais de uma parte, ex.: `dbms_lock.sleep`) e
/// prefixos de schema/pacote proibidos, em minúsculo.
pub fn forbidden_function_names(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Postgres => &[
            "pg_read_file",
            "pg_read_binary_file",
            "pg_ls_dir",
            "pg_ls_logdir",
            "pg_ls_waldir",
            "pg_stat_file",
            "lo_import",
            "lo_export",
            "dblink",
            "dblink_connect",
            "dblink_exec",
            "nextval",
            "setval",
            "pg_sleep",
            "pg_sleep_for",
            "pg_sleep_until",
            "pg_terminate_backend",
            "pg_cancel_backend",
            "pg_reload_conf",
            "query_to_xml",
            "xpath",
        ],
        Dialect::Oracle => &["httpuritype", "sleep", "execute_immediate"],
        Dialect::MySql => &[
            "load_file",
            "sleep",
            "benchmark",
            "get_lock",
            "release_lock",
            "release_all_locks",
            "is_free_lock",
            "is_used_lock",
            "master_pos_wait",
            "source_pos_wait",
        ],
    }
}

/// Prefixos de schema/pacote onde qualquer função é proibida — cobre
/// famílias inteiras (`dbms_lock`, `dbms_xmlgen`, `utl_http`, ...) sem
/// precisar listar cada uma.
pub fn forbidden_prefixes(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Oracle => &["dbms_", "utl_", "owa_", "sys."],
        Dialect::Postgres | Dialect::MySql => &[],
    }
}

/// `name` já deve vir normalizado em minúsculo (ver
/// [`crate::sql::guard::object_name_lower`]).
pub fn is_forbidden(dialect: Dialect, name_lower: &str) -> bool {
    if forbidden_function_names(dialect).contains(&name_lower) {
        return true;
    }
    forbidden_prefixes(dialect)
        .iter()
        .any(|prefix| name_lower.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_pg_sleep() {
        assert!(is_forbidden(Dialect::Postgres, "pg_sleep"));
    }

    #[test]
    fn blocks_oracle_dbms_prefix_regardless_of_function() {
        assert!(is_forbidden(Dialect::Oracle, "dbms_lock"));
        assert!(is_forbidden(Dialect::Oracle, "dbms_xmlgen"));
    }

    #[test]
    fn blocks_mysql_load_file() {
        assert!(is_forbidden(Dialect::MySql, "load_file"));
    }

    #[test]
    fn allows_ordinary_functions() {
        assert!(!is_forbidden(Dialect::Postgres, "upper"));
        assert!(!is_forbidden(Dialect::Oracle, "to_char"));
        assert!(!is_forbidden(Dialect::MySql, "concat"));
    }
}
