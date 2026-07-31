#![no_main]

use libfuzzer_sys::fuzz_target;
use queryboard_lib::sql::{validate, Dialect};

/// O guard nunca deve entrar em pânico, travar ou estourar a pilha,
/// não importa o input — aceitar ou rejeitar é a única saída permitida.
/// Ver CLAUDE.md §9 e docs/adr/0002-parser-sql.md.
fuzz_target!(|data: &[u8]| {
    let Ok(sql) = std::str::from_utf8(data) else {
        return;
    };

    for dialect in [Dialect::Oracle, Dialect::Postgres, Dialect::MySql] {
        let _ = validate(sql, dialect);
    }
});
