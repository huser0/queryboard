//! Camada de segurança somente-leitura sobre SQL de usuário.
//!
//! `guard` é o único lugar do repositório autorizado a produzir um
//! [`ValidatedSql`]. Nenhum outro módulo deve receber SQL cru do usuário —
//! ver CLAUDE.md §2.

pub mod denylist;
pub mod guard;
pub mod lexical;
pub mod params;

pub use guard::{validate, GuardError, ValidatedSql};

/// Dialeto SQL de uma connection. Determina o parser usado pelo guard, a
/// forma de bind usada por `params`, e o driver de banco (`db::Driver`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    Oracle,
    Postgres,
    MySql,
}

impl Dialect {
    pub(crate) fn as_sqlparser(self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            Dialect::Oracle => Box::new(sqlparser::dialect::OracleDialect {}),
            Dialect::Postgres => Box::new(sqlparser::dialect::PostgreSqlDialect {}),
            Dialect::MySql => Box::new(sqlparser::dialect::MySqlDialect {}),
        }
    }
}

/// Converte uma `Location` (linha/coluna, 1-indexado, contagem por `char`)
/// do sqlparser para um offset de byte em `sql`. Usado por `guard` e
/// `params` para fazer splice textual sem nunca reserializar o AST — a
/// única forma de preservar hints (`/*+ ... */`) e formatação originais.
pub(crate) fn location_to_byte_offset(sql: &str, loc: sqlparser::tokenizer::Location) -> usize {
    let mut line = 1u64;
    let mut column = 1u64;
    for (byte_pos, ch) in sql.char_indices() {
        if line == loc.line && column == loc.column {
            return byte_pos;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    sql.len()
}
