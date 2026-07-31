//! Representação de resultados de banco, independente de driver e nunca
//! vazando um tipo específico de crate de driver para o resto da app —
//! ver CLAUDE.md §3 ("sem vazar tipos específicos de driver").
//!
//! `CellValue::Decimal` nunca passa por `f64`: `NUMBER`/`numeric`/`decimal`
//! de qualquer precisão viram string decimal canônica. É o que torna a
//! regra `row.preco != origin.preco` (CLAUDE.md §6.5) uma comparação
//! exata, e não uma comparação de ponto flutuante.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    /// String decimal canônica: sem zeros à esquerda supérfluos, sinal só
    /// quando negativo, escala preservada exatamente como veio do banco.
    Decimal(String),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    /// ISO 8601 (`AAAA-MM-DD`).
    Date(String),
    /// ISO 8601 (`HH:MM:SS[.ffffff]`).
    Time(String),
    /// ISO 8601 sem timezone.
    Timestamp(String),
    /// ISO 8601 com offset explícito.
    TimestampTz(String),
    Json(String),
    /// Representação textual de um `INTERVAL` (ex.: `1 mon 2 days 00:00:03`).
    Interval(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColumnMeta {
    /// Nome original devolvido pelo banco.
    pub name: String,
    /// Nome normalizado para minúsculo — é o que aparece no contexto
    /// `row.<coluna>` das regras (CLAUDE.md §5: Oracle devolve maiúsculo
    /// por padrão, e isso precisa ser transparente para quem escreve
    /// regras).
    pub name_lower: String,
    pub declared_type: String,
    pub nullable: Option<bool>,
}

impl ColumnMeta {
    pub fn new(
        name: impl Into<String>,
        declared_type: impl Into<String>,
        nullable: Option<bool>,
    ) -> Self {
        let name = name.into();
        let name_lower = name.to_lowercase();
        Self {
            name,
            name_lower,
            declared_type: declared_type.into(),
            nullable,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultSet {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<CellValue>>,
    /// `true` se o resultado foi cortado por `max_rows`. Sempre via fetch
    /// limitado no cursor — nunca por `LIMIT` injetado na SQL do usuário
    /// (CLAUDE.md §6).
    pub truncated: bool,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_meta_normalizes_oracle_uppercase_names() {
        let col = ColumnMeta::new("OFFER_STATUS", "NUMBER", Some(false));
        assert_eq!(col.name, "OFFER_STATUS");
        assert_eq!(col.name_lower, "offer_status");
    }
}
