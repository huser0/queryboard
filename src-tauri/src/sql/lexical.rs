//! Estágio 1 do guard: checagens textuais, antes de qualquer tokenização.
//!
//! Homóglifos e caracteres invisíveis (ataque "Trojan Source") não são
//! responsabilidade do parser — precisam ser barrados no texto cru, em
//! qualquer posição, inclusive dentro de literais e comentários.

use thiserror::Error;

/// 256 KiB. Uma query legítima não chega perto disso; acima é sinal de
/// abuso ou de um bug do lado de quem chama.
pub const MAX_SQL_BYTES: usize = 256 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LexicalError {
    #[error("SQL excede o limite de {max} bytes (recebido: {actual})")]
    TooLarge { max: usize, actual: usize },

    #[error("caractere de controle não permitido na posição de byte {byte_pos}")]
    ForbiddenControlChar { byte_pos: usize },

    #[error(
        "caractere invisível ou de controle de direcionamento de texto (U+{codepoint:04X}) \
         não permitido na posição de byte {byte_pos}"
    )]
    ForbiddenInvisibleChar { byte_pos: usize, codepoint: u32 },
}

/// Estágio 1: valida o texto cru, byte a byte / char a char, antes de
/// qualquer tentativa de tokenizar ou parsear.
pub fn check_raw_text(sql: &str) -> Result<(), LexicalError> {
    if sql.len() > MAX_SQL_BYTES {
        return Err(LexicalError::TooLarge {
            max: MAX_SQL_BYTES,
            actual: sql.len(),
        });
    }

    for (byte_pos, c) in sql.char_indices() {
        if is_forbidden_invisible(c) {
            return Err(LexicalError::ForbiddenInvisibleChar {
                byte_pos,
                codepoint: c as u32,
            });
        }
        if c.is_control() && c != '\t' && c != '\n' && c != '\r' {
            return Err(LexicalError::ForbiddenControlChar { byte_pos });
        }
    }

    Ok(())
}

/// Caracteres invisíveis, de largura zero, ou de controle de
/// direcionamento bidirecional — o vetor clássico de "Trojan Source"
/// (CVE-2021-42574). Nenhum SQL legítimo precisa deles.
fn is_forbidden_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' // zero-width space/joiners, LRM/RLM
        | '\u{202A}'..='\u{202E}' // LRE, RLE, PDF, LRO, RLO
        | '\u{2066}'..='\u{2069}' // LRI, RLI, FSI, PDI
        | '\u{FEFF}' // BOM / zero-width no-break space
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_ascii_sql() {
        assert!(check_raw_text("SELECT 1 FROM dual").is_ok());
    }

    #[test]
    fn accepts_accented_portuguese_text() {
        assert!(check_raw_text("SELECT 'ação' FROM dual").is_ok());
    }

    #[test]
    fn accepts_tab_newline_cr() {
        assert!(check_raw_text("SELECT 1\nFROM\tdual\r\n").is_ok());
    }

    #[test]
    fn rejects_too_large() {
        let sql = "SELECT ".to_string() + &"1".repeat(MAX_SQL_BYTES);
        let err = check_raw_text(&sql).unwrap_err();
        assert!(matches!(err, LexicalError::TooLarge { .. }));
    }

    #[test]
    fn rejects_nul_byte() {
        let sql = "SELECT 1\0 FROM dual";
        let err = check_raw_text(sql).unwrap_err();
        assert!(matches!(err, LexicalError::ForbiddenControlChar { .. }));
    }

    #[test]
    fn rejects_zero_width_space_anywhere() {
        let sql = "SEL\u{200B}ECT 1 FROM dual";
        let err = check_raw_text(sql).unwrap_err();
        assert!(matches!(err, LexicalError::ForbiddenInvisibleChar { .. }));
    }

    #[test]
    fn rejects_rtl_override_inside_comment() {
        let sql = "SELECT 1 -- \u{202E} comentário malicioso\nFROM dual";
        let err = check_raw_text(sql).unwrap_err();
        assert!(matches!(err, LexicalError::ForbiddenInvisibleChar { .. }));
    }

    #[test]
    fn rejects_rtl_override_inside_string_literal() {
        let sql = "SELECT '\u{202E}foo' FROM dual";
        let err = check_raw_text(sql).unwrap_err();
        assert!(matches!(err, LexicalError::ForbiddenInvisibleChar { .. }));
    }

    #[test]
    fn rejects_bom() {
        let sql = "\u{FEFF}SELECT 1 FROM dual";
        let err = check_raw_text(sql).unwrap_err();
        assert!(matches!(err, LexicalError::ForbiddenInvisibleChar { .. }));
    }
}
