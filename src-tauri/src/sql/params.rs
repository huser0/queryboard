//! Extração e reescrita de parâmetros nomeados (`:nome`). Reusa a mesma
//! tokenização do guard (via [`ValidatedSql::retokenize`]) para nunca
//! divergir sobre onde um `:nome` começa e termina — ver CLAUDE.md §9.
//!
//! Nada aqui gera SQL por concatenação. A reescrita troca apenas o texto
//! do placeholder pelo formato de bind do dialeto (`$n` / `?` / `:nome`),
//! via splice de offsets de byte sobre o texto original — nunca
//! reserialização do AST. Depois de reescrever, o resultado passa pelo
//! guard de novo (ver [`rewrite_placeholders`]).

use std::collections::{HashMap, HashSet};

use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Token, TokenWithSpan};
use thiserror::Error;

use crate::sql::{guard, location_to_byte_offset, Dialect, GuardError, ValidatedSql};

/// Teto de tamanho de lista, para não montar uma consulta absurda por
/// engano (CLAUDE.md §6.4). Configurável pelo chamador.
pub const DEFAULT_MAX_LIST_SIZE: usize = 10_000;

/// Limite de elementos por `IN (...)` no Oracle (`ORA-01795` acima disso).
/// Usado por [`oracle_batch_sizes`] — a divisão em lotes em si é
/// responsabilidade do runner de flow (roadmap item 9), não deste módulo.
pub const ORACLE_IN_LIST_BATCH_SIZE: usize = 1000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParamsError {
    #[error("parâmetro :{name} aparece na consulta mas nenhum valor foi informado")]
    MissingBinding { name: String },

    #[error("parâmetro :{name} foi informado mas não é usado na consulta")]
    UnusedBinding { name: String },

    #[error("parâmetro de lista :{name} só pode ser usado no padrão IN (:{name})")]
    ListParamOutsideInClause { name: String },

    #[error("parâmetro de lista :{name} não pode ser usado mais de uma vez na mesma consulta")]
    ListParamUsedMultipleTimes { name: String },

    #[error(
        "lista vazia para :{name} — nunca gera IN () (SQL inválida); trate como passo 'empty'"
    )]
    EmptyList { name: String },

    #[error("lista de :{name} excede o teto de {max} elementos (recebidos: {actual})")]
    ListTooLarge {
        name: String,
        max: usize,
        actual: usize,
    },

    #[error("SQL reescrita foi rejeitada pelo guard: {0}")]
    RewrittenSqlRejected(#[from] GuardError),
}

/// Uma ocorrência de `:nome` no SQL original, com a posição exata em bytes
/// e se ela está no padrão `IN (:nome)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamOccurrence {
    pub name: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub in_list_context: bool,
}

/// Extrai todas as ocorrências de `:nome` do SQL já validado, na ordem em
/// que aparecem no texto. Ignora qualquer `:` dentro de literais de string
/// ou comentários (o tokenizer já trata esse conteúdo como um token único,
/// não emite `Colon`/`Word` para dentro dele) e nunca confunde `::cast`
/// (Postgres) com um bind — `::` é um token `DoubleColon` distinto.
pub fn extract_params(validated: &ValidatedSql) -> Vec<ParamOccurrence> {
    let sql = validated.as_str();
    let tokens = validated.retokenize();
    let mut occurrences = Vec::new();

    let mut idx = 0;
    while idx < tokens.len() {
        if matches!(tokens[idx].token, Token::Colon) {
            if let Some(next) = tokens.get(idx + 1) {
                if let Token::Word(word) = &next.token {
                    if word.quote_style.is_none() {
                        let byte_start = location_to_byte_offset(sql, tokens[idx].span.start);
                        let byte_end = location_to_byte_offset(sql, next.span.end);
                        occurrences.push(ParamOccurrence {
                            name: word.value.clone(),
                            byte_start,
                            byte_end,
                            in_list_context: is_in_list_context(&tokens, idx),
                        });
                        idx += 2;
                        continue;
                    }
                }
            }
        }
        idx += 1;
    }

    occurrences
}

fn prev_non_whitespace(tokens: &[TokenWithSpan], idx: usize) -> Option<usize> {
    (0..idx)
        .rev()
        .find(|&i| !matches!(tokens[i].token, Token::Whitespace(_)))
}

fn next_non_whitespace(tokens: &[TokenWithSpan], idx: usize) -> Option<usize> {
    (idx + 1..tokens.len()).find(|&i| !matches!(tokens[i].token, Token::Whitespace(_)))
}

/// `true` se o `:nome` em `tokens[colon_idx]` está sozinho dentro de
/// `IN ( ... )` — o único padrão em que um parâmetro de lista é aceito.
fn is_in_list_context(tokens: &[TokenWithSpan], colon_idx: usize) -> bool {
    let Some(lparen_idx) = prev_non_whitespace(tokens, colon_idx) else {
        return false;
    };
    if !matches!(tokens[lparen_idx].token, Token::LParen) {
        return false;
    }
    let Some(in_idx) = prev_non_whitespace(tokens, lparen_idx) else {
        return false;
    };
    if !matches!(&tokens[in_idx].token, Token::Word(w) if w.keyword == Keyword::IN) {
        return false;
    }
    let Some(rparen_idx) = next_non_whitespace(tokens, colon_idx + 1) else {
        return false;
    };
    matches!(tokens[rparen_idx].token, Token::RParen)
}

/// Cardinalidade resolvida de um parâmetro no momento do bind. `List`
/// carrega apenas a contagem — o valor real é responsabilidade do
/// chamador (runner), este módulo só precisa saber quantos placeholders
/// gerar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Scalar,
    List(usize),
}

#[derive(Debug, Clone)]
pub struct ParamBinding {
    pub name: String,
    pub cardinality: Cardinality,
}

/// Para qual valor de entrada cada placeholder gerado corresponde, na
/// ordem em que os placeholders aparecem na SQL reescrita. É o que permite
/// ao driver montar o array de binds na ordem certa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindSlot {
    Scalar { name: String },
    ListElement { name: String, index: usize },
}

#[derive(Debug, Clone)]
pub struct RewrittenSql {
    pub validated: ValidatedSql,
    pub bind_order: Vec<BindSlot>,
}

/// Reescreve `:nome` para o formato de bind do dialeto e revalida o
/// resultado pelo guard antes de devolvê-lo — nunca confie apenas na
/// reescrita, mesmo sendo puramente textual.
pub fn rewrite_placeholders(
    validated: &ValidatedSql,
    bindings: &[ParamBinding],
    max_list_size: usize,
) -> Result<RewrittenSql, ParamsError> {
    let sql = validated.as_str();
    let dialect = validated.dialect();
    let mut occurrences = extract_params(validated);
    occurrences.sort_by_key(|occ| occ.byte_start);

    let binding_by_name: HashMap<&str, &ParamBinding> =
        bindings.iter().map(|b| (b.name.as_str(), b)).collect();

    let mut used_names: HashSet<&str> = HashSet::new();
    for occ in &occurrences {
        used_names.insert(occ.name.as_str());
        let binding =
            binding_by_name
                .get(occ.name.as_str())
                .ok_or_else(|| ParamsError::MissingBinding {
                    name: occ.name.clone(),
                })?;
        if matches!(binding.cardinality, Cardinality::List(_)) && !occ.in_list_context {
            return Err(ParamsError::ListParamOutsideInClause {
                name: occ.name.clone(),
            });
        }
    }
    for binding in bindings {
        if !used_names.contains(binding.name.as_str()) {
            return Err(ParamsError::UnusedBinding {
                name: binding.name.clone(),
            });
        }
        if let Cardinality::List(count) = binding.cardinality {
            if count == 0 {
                return Err(ParamsError::EmptyList {
                    name: binding.name.clone(),
                });
            }
            if count > max_list_size {
                return Err(ParamsError::ListTooLarge {
                    name: binding.name.clone(),
                    max: max_list_size,
                    actual: count,
                });
            }
        }
    }

    let mut output = String::with_capacity(sql.len());
    let mut cursor = 0usize;
    let mut bind_order = Vec::new();
    let mut postgres_scalar_slots: HashMap<String, usize> = HashMap::new();
    let mut postgres_next_slot = 1usize;
    let mut oracle_list_counter = 0usize;
    let mut seen_list_names: HashSet<String> = HashSet::new();
    let mut seen_oracle_scalar_names: HashSet<String> = HashSet::new();

    for occ in &occurrences {
        output.push_str(&sql[cursor..occ.byte_start]);
        let binding = binding_by_name[occ.name.as_str()];

        let replacement = match (dialect, binding.cardinality) {
            (Dialect::Oracle, Cardinality::Scalar) => {
                // Bind nativo por nome — Oracle já reusa `:id` repetido
                // sozinho, então só registramos o slot na primeira vez.
                if seen_oracle_scalar_names.insert(occ.name.clone()) {
                    bind_order.push(BindSlot::Scalar {
                        name: occ.name.clone(),
                    });
                }
                format!(":{}", occ.name)
            }
            (Dialect::Oracle, Cardinality::List(count)) => {
                require_first_use(&mut seen_list_names, &occ.name)?;
                let parts: Vec<String> = (0..count)
                    .map(|i| {
                        bind_order.push(BindSlot::ListElement {
                            name: occ.name.clone(),
                            index: i,
                        });
                        let placeholder = format!(":p{oracle_list_counter}");
                        oracle_list_counter += 1;
                        placeholder
                    })
                    .collect();
                parts.join(", ")
            }
            (Dialect::Postgres, Cardinality::Scalar) => {
                let slot = *postgres_scalar_slots
                    .entry(occ.name.clone())
                    .or_insert_with(|| {
                        let slot = postgres_next_slot;
                        postgres_next_slot += 1;
                        bind_order.push(BindSlot::Scalar {
                            name: occ.name.clone(),
                        });
                        slot
                    });
                format!("${slot}")
            }
            (Dialect::Postgres, Cardinality::List(count)) => {
                require_first_use(&mut seen_list_names, &occ.name)?;
                let parts: Vec<String> = (0..count)
                    .map(|i| {
                        bind_order.push(BindSlot::ListElement {
                            name: occ.name.clone(),
                            index: i,
                        });
                        let placeholder = format!("${postgres_next_slot}");
                        postgres_next_slot += 1;
                        placeholder
                    })
                    .collect();
                parts.join(", ")
            }
            (Dialect::MySql, Cardinality::Scalar) => {
                bind_order.push(BindSlot::Scalar {
                    name: occ.name.clone(),
                });
                "?".to_string()
            }
            (Dialect::MySql, Cardinality::List(count)) => {
                require_first_use(&mut seen_list_names, &occ.name)?;
                let parts: Vec<&str> = (0..count)
                    .map(|i| {
                        bind_order.push(BindSlot::ListElement {
                            name: occ.name.clone(),
                            index: i,
                        });
                        "?"
                    })
                    .collect();
                parts.join(", ")
            }
        };

        output.push_str(&replacement);
        cursor = occ.byte_end;
    }
    output.push_str(&sql[cursor..]);

    // Segurança em camadas: mesmo sendo reescrita puramente textual sobre
    // um SQL já validado, o resultado passa pelo guard de novo.
    let revalidated = guard::validate(&output, dialect)?;

    Ok(RewrittenSql {
        validated: revalidated,
        bind_order,
    })
}

fn require_first_use(seen: &mut HashSet<String>, name: &str) -> Result<(), ParamsError> {
    if !seen.insert(name.to_string()) {
        return Err(ParamsError::ListParamUsedMultipleTimes {
            name: name.to_string(),
        });
    }
    Ok(())
}

/// Divide `count` elementos em lotes de no máximo `batch_size` — o Oracle
/// rejeita `IN` com mais de 1000 elementos (`ORA-01795`). Função pura; o
/// runner de flow (roadmap item 9) decide o que fazer com cada lote
/// (executar e concatenar resultados).
pub fn oracle_batch_sizes(count: usize, batch_size: usize) -> Vec<usize> {
    if count == 0 || batch_size == 0 {
        return Vec::new();
    }
    let mut sizes = Vec::new();
    let mut remaining = count;
    while remaining > 0 {
        let take = remaining.min(batch_size);
        sizes.push(take);
        remaining -= take;
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::validate;

    fn validated(sql: &str, dialect: Dialect) -> ValidatedSql {
        validate(sql, dialect).expect("sql de teste deveria ser válida")
    }

    #[test]
    fn extracts_single_scalar_param() {
        let v = validated("SELECT * FROM t WHERE id = :id", Dialect::Oracle);
        let params = extract_params(&v);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "id");
        assert!(!params[0].in_list_context);
    }

    #[test]
    fn extracts_param_repeated_twice() {
        let v = validated(
            "SELECT * FROM t WHERE created_by = :user_id OR updated_by = :user_id",
            Dialect::Oracle,
        );
        let params = extract_params(&v);
        assert_eq!(params.len(), 2);
        assert!(params.iter().all(|p| p.name == "user_id"));
    }

    #[test]
    fn detects_in_list_context() {
        let v = validated("SELECT * FROM t WHERE id IN (:ids)", Dialect::Oracle);
        let params = extract_params(&v);
        assert_eq!(params.len(), 1);
        assert!(params[0].in_list_context);
    }

    #[test]
    fn ignores_colon_inside_string_literal() {
        let v = validated("SELECT ':not_a_param' FROM dual", Dialect::Oracle);
        assert!(extract_params(&v).is_empty());
    }

    #[test]
    fn does_not_confuse_postgres_cast_with_param() {
        let v = validated("SELECT id::text FROM t WHERE x = :x", Dialect::Postgres);
        let params = extract_params(&v);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
    }

    #[test]
    fn oracle_scalar_param_is_left_untouched() {
        let v = validated("SELECT * FROM t WHERE id = :id", Dialect::Oracle);
        let bindings = vec![ParamBinding {
            name: "id".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(
            rewritten.validated.as_str(),
            "SELECT * FROM t WHERE id = :id"
        );
        assert_eq!(
            rewritten.bind_order,
            vec![BindSlot::Scalar {
                name: "id".to_string()
            }]
        );
    }

    #[test]
    fn postgres_scalar_param_becomes_dollar_n() {
        let v = validated("SELECT * FROM t WHERE id = :id", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "id".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(
            rewritten.validated.as_str(),
            "SELECT * FROM t WHERE id = $1"
        );
    }

    #[test]
    fn postgres_repeated_scalar_reuses_same_slot() {
        let v = validated("SELECT * FROM t WHERE a = :x OR b = :x", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "x".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(
            rewritten.validated.as_str(),
            "SELECT * FROM t WHERE a = $1 OR b = $1"
        );
        // Um único bind é suficiente — mesmo valor usado duas vezes no SQL.
        assert_eq!(rewritten.bind_order.len(), 1);
    }

    #[test]
    fn mysql_repeated_scalar_gets_two_placeholders() {
        let v = validated("SELECT * FROM t WHERE a = :x OR b = :x", Dialect::MySql);
        let bindings = vec![ParamBinding {
            name: "x".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(
            rewritten.validated.as_str(),
            "SELECT * FROM t WHERE a = ? OR b = ?"
        );
        // MySQL não reusa posição — o valor precisa ser passado duas vezes.
        assert_eq!(rewritten.bind_order.len(), 2);
    }

    #[test]
    fn rewrites_list_of_one_element_per_dialect() {
        for (dialect, expected) in [
            (Dialect::Oracle, "SELECT * FROM t WHERE id IN (:p0)"),
            (Dialect::Postgres, "SELECT * FROM t WHERE id IN ($1)"),
            (Dialect::MySql, "SELECT * FROM t WHERE id IN (?)"),
        ] {
            let v = validated("SELECT * FROM t WHERE id IN (:ids)", dialect);
            let bindings = vec![ParamBinding {
                name: "ids".to_string(),
                cardinality: Cardinality::List(1),
            }];
            let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
            assert_eq!(rewritten.validated.as_str(), expected);
            assert_eq!(rewritten.bind_order.len(), 1);
        }
    }

    #[test]
    fn rewrites_list_of_exactly_1000_elements() {
        let v = validated("SELECT * FROM t WHERE id IN (:ids)", Dialect::Oracle);
        let bindings = vec![ParamBinding {
            name: "ids".to_string(),
            cardinality: Cardinality::List(1000),
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(rewritten.bind_order.len(), 1000);
        assert!(rewritten.validated.as_str().contains(":p0"));
        assert!(rewritten.validated.as_str().contains(":p999"));
    }

    #[test]
    fn empty_list_is_rejected_never_generates_in_parens() {
        let v = validated("SELECT * FROM t WHERE id IN (:ids)", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "ids".to_string(),
            cardinality: Cardinality::List(0),
        }];
        let err = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap_err();
        assert_eq!(
            err,
            ParamsError::EmptyList {
                name: "ids".to_string()
            }
        );
    }

    #[test]
    fn list_over_default_cap_is_rejected() {
        let v = validated("SELECT * FROM t WHERE id IN (:ids)", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "ids".to_string(),
            cardinality: Cardinality::List(10_001),
        }];
        let err = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap_err();
        assert_eq!(
            err,
            ParamsError::ListTooLarge {
                name: "ids".to_string(),
                max: DEFAULT_MAX_LIST_SIZE,
                actual: 10_001,
            }
        );
    }

    #[test]
    fn list_param_used_outside_in_clause_is_rejected() {
        let v = validated("SELECT * FROM t WHERE id = :ids", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "ids".to_string(),
            cardinality: Cardinality::List(3),
        }];
        let err = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap_err();
        assert_eq!(
            err,
            ParamsError::ListParamOutsideInClause {
                name: "ids".to_string()
            }
        );
    }

    #[test]
    fn scalar_param_inside_in_clause_is_allowed() {
        // IN (:id) com :id declarado escalar é SQL válida (IN com 1 valor).
        let v = validated("SELECT * FROM t WHERE id IN (:id)", Dialect::Postgres);
        let bindings = vec![ParamBinding {
            name: "id".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let rewritten = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap();
        assert_eq!(
            rewritten.validated.as_str(),
            "SELECT * FROM t WHERE id IN ($1)"
        );
    }

    #[test]
    fn missing_binding_is_rejected() {
        let v = validated("SELECT * FROM t WHERE id = :id", Dialect::Postgres);
        let err = rewrite_placeholders(&v, &[], DEFAULT_MAX_LIST_SIZE).unwrap_err();
        assert_eq!(
            err,
            ParamsError::MissingBinding {
                name: "id".to_string()
            }
        );
    }

    #[test]
    fn unused_binding_is_rejected() {
        let v = validated("SELECT 1 FROM dual", Dialect::Oracle);
        let bindings = vec![ParamBinding {
            name: "id".to_string(),
            cardinality: Cardinality::Scalar,
        }];
        let err = rewrite_placeholders(&v, &bindings, DEFAULT_MAX_LIST_SIZE).unwrap_err();
        assert_eq!(
            err,
            ParamsError::UnusedBinding {
                name: "id".to_string()
            }
        );
    }

    #[test]
    fn oracle_batch_sizes_splits_1001_into_two_batches() {
        assert_eq!(
            oracle_batch_sizes(1001, ORACLE_IN_LIST_BATCH_SIZE),
            vec![1000, 1]
        );
    }

    #[test]
    fn oracle_batch_sizes_exact_multiple() {
        assert_eq!(
            oracle_batch_sizes(2000, ORACLE_IN_LIST_BATCH_SIZE),
            vec![1000, 1000]
        );
    }

    #[test]
    fn oracle_batch_sizes_under_limit_is_single_batch() {
        assert_eq!(
            oracle_batch_sizes(999, ORACLE_IN_LIST_BATCH_SIZE),
            vec![999]
        );
    }

    #[test]
    fn oracle_batch_sizes_zero_is_empty() {
        assert_eq!(
            oracle_batch_sizes(0, ORACLE_IN_LIST_BATCH_SIZE),
            Vec::<usize>::new()
        );
    }
}
