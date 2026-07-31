//! Pipeline de validação read-only. Núcleo crítico do repositório — ver
//! CLAUDE.md §2 e §9. Toda alteração aqui exige novos casos de teste,
//! incluindo tentativas de bypass.
//!
//! [`ValidatedSql`] só pode ser construído por [`validate`]. Nenhuma outra
//! função deste módulo, nem de nenhum outro, tem acesso ao construtor —
//! é impossível, em nível de tipo, chamar um driver com SQL não validada.

use std::ops::ControlFlow;

use sqlparser::ast::{
    Expr, ObjectName, ObjectNamePart, Query, Select, SetExpr, Statement, TableFactor, Visit,
    Visitor,
};
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, TokenizerError, Whitespace};
use thiserror::Error;

use crate::sql::{denylist, lexical, lexical::LexicalError, location_to_byte_offset, Dialect};

/// Limite de profundidade de aninhamento de parênteses/subconsultas. Acima
/// disso o parser retorna um erro controlado em vez de estourar a pilha.
const RECURSION_LIMIT: usize = 50;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GuardError {
    #[error(transparent)]
    Lexical(#[from] LexicalError),

    #[error("token não permitido fora de literal de string, comentário ou identificador entre aspas: {token}")]
    NonAsciiToken { token: String },

    #[error(
        "apenas um comando é permitido por consulta ({separators} separador(es) ';' encontrado(s))"
    )]
    MultipleStatements { separators: usize },

    #[error("SQL vazia ou contém apenas comentários")]
    EmptyStatement,

    #[error("erro de tokenização: {0}")]
    Tokenizer(String),

    #[error("erro de sintaxe SQL: {0}")]
    Parser(String),

    #[error("consulta excede o limite de profundidade de aninhamento permitido")]
    RecursionLimitExceeded,

    #[error("apenas comandos SELECT (ou WITH ... SELECT) são permitidos")]
    NotASelect,

    #[error(
        "bloco de escrita (INSERT/UPDATE/DELETE/MERGE) não é permitido, nem mesmo dentro de WITH"
    )]
    WriteInCte,

    #[error("cláusula de bloqueio de linhas (FOR UPDATE/FOR SHARE) não é permitida em uma conexão somente leitura")]
    LockingClauseNotAllowed,

    #[error("SELECT ... INTO não é permitido — cria uma tabela")]
    SelectIntoNotAllowed,

    #[error("função de tabela (table function) não é permitida")]
    TableFunctionNotAllowed,

    #[error("função não permitida: {name}")]
    ForbiddenFunction { name: String },

    #[error(
        "nome de função com parte dinâmica não pôde ser verificado contra a lista de permissões"
    )]
    DynamicFunctionName,
}

/// SQL que já passou pelo pipeline completo do guard: é garantidamente um
/// único `SELECT` (ou `WITH ... SELECT`) sem cláusulas de escrita, bloqueio
/// ou funções proibidas. O texto armazenado é o **original do usuário**
/// (menos um eventual `;` terminal) — nunca uma reserialização do AST, para
/// preservar hints (`/*+ ... */`) e formatação byte a byte.
#[derive(Debug, Clone)]
pub struct ValidatedSql {
    sql: String,
    dialect: Dialect,
}

impl ValidatedSql {
    /// Privado de propósito — só [`validate`] pode chamar isto.
    fn new(sql: String, dialect: Dialect) -> Self {
        Self { sql, dialect }
    }

    pub fn as_str(&self) -> &str {
        &self.sql
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Retokeniza o texto já validado, para módulos que precisam da mesma
    /// varredura léxica (ex.: `params`) sem reimplementar o parsing.
    /// Infalível: este texto já tokenizou com sucesso uma vez.
    pub(crate) fn retokenize(&self) -> Vec<TokenWithSpan> {
        let dialect = self.dialect.as_sqlparser();
        Tokenizer::new(dialect.as_ref(), &self.sql)
            .tokenize_with_location()
            .expect("ValidatedSql já foi tokenizado com sucesso uma vez")
    }
}

/// Ponto de entrada único do guard. Ver CLAUDE.md §2 para a lista de
/// garantias em camadas; esta função implementa a camada 1 (parser).
pub fn validate(sql: &str, dialect: Dialect) -> Result<ValidatedSql, GuardError> {
    // Estágio 1: texto cru, antes de qualquer tokenização.
    lexical::check_raw_text(sql)?;

    let sqlparser_dialect = dialect.as_sqlparser();

    // Estágio 2: tokenizar e exigir ASCII fora de literais/comentários.
    let tokens = Tokenizer::new(sqlparser_dialect.as_ref(), sql)
        .tokenize_with_location()
        .map_err(tokenizer_error)?;
    check_ascii_tokens(&tokens)?;

    // Estágio 3: no máximo um separador ';', e só se for terminal.
    let trimmed_sql = trim_single_trailing_semicolon(sql, &tokens)?;

    // Estágio 4: parse. Exige exatamente um statement.
    let statements = Parser::new(sqlparser_dialect.as_ref())
        .with_recursion_limit(RECURSION_LIMIT)
        .try_with_sql(&trimmed_sql)
        .map_err(parser_error)?
        .parse_statements()
        .map_err(parser_error)?;

    let statement = match statements.as_slice() {
        [] => return Err(GuardError::EmptyStatement),
        [single] => single,
        many => {
            return Err(GuardError::MultipleStatements {
                separators: many.len() - 1,
            })
        }
    };

    // Estágio 5: raiz deve ser um SELECT.
    if !matches!(statement, Statement::Query(_)) {
        return Err(GuardError::NotASelect);
    }

    // Estágio 6: walk completo via Visitor — nunca match manual, para que
    // uma variante nova de AST numa versão futura do sqlparser não abra
    // um buraco silencioso (ver docs/adr/0002-parser-sql.md).
    let mut visitor = GuardVisitor { dialect };
    if let ControlFlow::Break(err) = statement.visit(&mut visitor) {
        return Err(err);
    }

    // Estágio 7: produzir ValidatedSql com o texto ORIGINAL (validado),
    // nunca uma reserialização do AST.
    Ok(ValidatedSql::new(trimmed_sql, dialect))
}

struct GuardVisitor {
    dialect: Dialect,
}

impl Visitor for GuardVisitor {
    type Break = GuardError;

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<GuardError> {
        if !query.locks.is_empty() {
            return ControlFlow::Break(GuardError::LockingClauseNotAllowed);
        }
        if matches!(
            query.body.as_ref(),
            SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_)
        ) {
            return ControlFlow::Break(GuardError::WriteInCte);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<GuardError> {
        if select.into.is_some() {
            return ControlFlow::Break(GuardError::SelectIntoNotAllowed);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, table_factor: &TableFactor) -> ControlFlow<GuardError> {
        if matches!(
            table_factor,
            TableFactor::Function { .. } | TableFactor::TableFunction { .. }
        ) {
            return ControlFlow::Break(GuardError::TableFunctionNotAllowed);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<GuardError> {
        if let Expr::Function(function) = expr {
            match object_name_lower(&function.name) {
                Some(name_lower) => {
                    if denylist::is_forbidden(self.dialect, &name_lower) {
                        return ControlFlow::Break(GuardError::ForbiddenFunction {
                            name: name_lower,
                        });
                    }
                }
                None => return ControlFlow::Break(GuardError::DynamicFunctionName),
            }
        }
        ControlFlow::Continue(())
    }
}

/// `None` se qualquer parte do nome for dinâmica (dialect-specific,
/// `ObjectNamePart::Function`) — nesse caso não dá para verificar contra a
/// lista de permissões, e o chamador deve bloquear por precaução.
fn object_name_lower(name: &ObjectName) -> Option<String> {
    let mut parts = Vec::with_capacity(name.0.len());
    for part in &name.0 {
        match part {
            ObjectNamePart::Identifier(ident) => parts.push(ident.value.to_lowercase()),
            ObjectNamePart::Function(_) => return None,
        }
    }
    Some(parts.join("."))
}

/// Tokens isentos da exigência de ASCII: literais de string (podem conter
/// qualquer texto do usuário, ex. `'ação'`), identificadores entre aspas, e
/// comentários (documentação em português não é risco — o Trojan Source já
/// foi barrado no estágio 1, para qualquer posição, inclusive comentários).
fn is_exempt_from_ascii_check(token: &Token) -> bool {
    if let Token::Word(word) = token {
        return word.quote_style.is_some();
    }
    matches!(
        token,
        Token::SingleQuotedString(_)
            | Token::DoubleQuotedString(_)
            | Token::TripleSingleQuotedString(_)
            | Token::TripleDoubleQuotedString(_)
            | Token::DollarQuotedString(_)
            | Token::SingleQuotedByteStringLiteral(_)
            | Token::DoubleQuotedByteStringLiteral(_)
            | Token::TripleSingleQuotedByteStringLiteral(_)
            | Token::TripleDoubleQuotedByteStringLiteral(_)
            | Token::SingleQuotedRawStringLiteral(_)
            | Token::DoubleQuotedRawStringLiteral(_)
            | Token::TripleSingleQuotedRawStringLiteral(_)
            | Token::TripleDoubleQuotedRawStringLiteral(_)
            | Token::NationalStringLiteral(_)
            | Token::QuoteDelimitedStringLiteral(_)
            | Token::NationalQuoteDelimitedStringLiteral(_)
            | Token::EscapedStringLiteral(_)
            | Token::UnicodeStringLiteral(_)
            | Token::HexStringLiteral(_)
            | Token::Whitespace(Whitespace::SingleLineComment { .. })
            | Token::Whitespace(Whitespace::MultiLineComment(_))
    )
}

fn check_ascii_tokens(tokens: &[TokenWithSpan]) -> Result<(), GuardError> {
    for tws in tokens {
        if is_exempt_from_ascii_check(&tws.token) {
            continue;
        }
        let rendered = tws.token.to_string();
        if !rendered.is_ascii() {
            return Err(GuardError::NonAsciiToken { token: rendered });
        }
    }
    Ok(())
}

/// Aceita no máximo um `;`, e só se não houver mais nada além de
/// whitespace/comentário depois dele (Oracle thin rejeita o terminador,
/// então ele é removido do texto final — ver "questão em aberto Q1" no
/// PLAN.md).
fn trim_single_trailing_semicolon(
    sql: &str,
    tokens: &[TokenWithSpan],
) -> Result<String, GuardError> {
    let semicolon_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, tws)| matches!(tws.token, Token::SemiColon))
        .map(|(idx, _)| idx)
        .collect();

    match semicolon_positions.as_slice() {
        [] => Ok(sql.to_string()),
        [idx] => {
            let trailing_is_harmless = tokens[idx + 1..]
                .iter()
                .all(|tws| matches!(tws.token, Token::Whitespace(_) | Token::EOF));
            if !trailing_is_harmless {
                return Err(GuardError::MultipleStatements { separators: 1 });
            }
            let byte_start = location_to_byte_offset(sql, tokens[*idx].span.start);
            Ok(sql[..byte_start].trim_end().to_string())
        }
        many => Err(GuardError::MultipleStatements {
            separators: many.len(),
        }),
    }
}

fn tokenizer_error(err: TokenizerError) -> GuardError {
    GuardError::Tokenizer(err.to_string())
}

fn parser_error(err: ParserError) -> GuardError {
    match err {
        ParserError::RecursionLimitExceeded => GuardError::RecursionLimitExceeded,
        other => GuardError::Parser(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rejected(sql: &str, dialect: Dialect) -> GuardError {
        validate(sql, dialect).expect_err(&format!("deveria ter sido rejeitado: {sql}"))
    }

    fn assert_accepted(sql: &str, dialect: Dialect) -> ValidatedSql {
        validate(sql, dialect).unwrap_or_else(|e| panic!("deveria ter sido aceito: {sql}: {e}"))
    }

    // ---------- bypass: statements empilhados ----------

    #[test]
    fn rejects_stacked_statements() {
        let err = assert_rejected("SELECT 1; DROP TABLE t", Dialect::Postgres);
        assert!(matches!(err, GuardError::MultipleStatements { .. }));
    }

    #[test]
    fn rejects_stacked_statements_hidden_behind_comment() {
        let err = assert_rejected("SELECT 1; -- \n DROP TABLE t", Dialect::Postgres);
        assert!(matches!(err, GuardError::MultipleStatements { .. }));
    }

    #[test]
    fn rejects_stacked_statements_hidden_in_block_comment_with_real_semicolon_after() {
        let err = assert_rejected(
            "SELECT 1 /* ; DROP TABLE t */ FROM dual; DROP TABLE t",
            Dialect::Oracle,
        );
        assert!(matches!(err, GuardError::MultipleStatements { .. }));
    }

    #[test]
    fn rejects_semicolon_inside_string_literal_followed_by_second_statement() {
        // O ';' dentro da string não conta; o segundo statement de verdade
        // (sem ';' antes dele) faz o parser falhar ou detectar múltiplos.
        let err = assert_rejected("SELECT ';' FROM dual DROP TABLE t", Dialect::Oracle);
        assert!(matches!(
            err,
            GuardError::Parser(_) | GuardError::MultipleStatements { .. }
        ));
    }

    // ---------- bypass: DML dentro de CTE ----------

    #[test]
    fn rejects_insert_inside_cte() {
        let err = assert_rejected(
            "WITH x AS (INSERT INTO t VALUES (1) RETURNING *) SELECT * FROM x",
            Dialect::Postgres,
        );
        assert_eq!(err, GuardError::WriteInCte);
    }

    #[test]
    fn rejects_delete_inside_cte() {
        let err = assert_rejected(
            "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x",
            Dialect::Postgres,
        );
        assert_eq!(err, GuardError::WriteInCte);
    }

    #[test]
    fn rejects_update_inside_cte() {
        let err = assert_rejected(
            "WITH x AS (UPDATE t SET a=1 RETURNING *) SELECT * FROM x",
            Dialect::Postgres,
        );
        assert_eq!(err, GuardError::WriteInCte);
    }

    // ---------- bypass: bloqueio de linhas / escrita direta ----------

    #[test]
    fn rejects_for_update() {
        let err = assert_rejected("SELECT * FROM t FOR UPDATE", Dialect::Postgres);
        assert_eq!(err, GuardError::LockingClauseNotAllowed);
    }

    #[test]
    fn rejects_select_into() {
        let err = assert_rejected("SELECT * INTO nova FROM t", Dialect::Postgres);
        assert_eq!(err, GuardError::SelectIntoNotAllowed);
    }

    #[test]
    fn rejects_create_table_as() {
        let err = assert_rejected("CREATE TABLE x AS SELECT 1", Dialect::Postgres);
        assert_eq!(err, GuardError::NotASelect);
    }

    #[test]
    fn rejects_anonymous_plsql_block() {
        // "BEGIN NULL; END;" tem 2 separadores ';' — barrado já no estágio
        // 3 (múltiplos statements), antes mesmo de chegar à checagem de
        // raiz. Também seria rejeitado por NotASelect se tivesse só um.
        let err = assert_rejected("BEGIN NULL; END;", Dialect::Oracle);
        assert!(matches!(
            err,
            GuardError::NotASelect
                | GuardError::Parser(_)
                | GuardError::Tokenizer(_)
                | GuardError::MultipleStatements { .. }
        ));
    }

    #[test]
    fn rejects_declare_block() {
        let err = assert_rejected("DECLARE v NUMBER; BEGIN NULL; END;", Dialect::Oracle);
        assert!(matches!(
            err,
            GuardError::NotASelect
                | GuardError::Parser(_)
                | GuardError::Tokenizer(_)
                | GuardError::MultipleStatements { .. }
        ));
    }

    #[test]
    fn rejects_do_block() {
        let err = assert_rejected("DO $$ BEGIN END $$", Dialect::Postgres);
        assert!(matches!(
            err,
            GuardError::NotASelect | GuardError::Parser(_) | GuardError::Tokenizer(_)
        ));
    }

    #[test]
    fn rejects_call_procedure() {
        let err = assert_rejected("CALL p()", Dialect::Postgres);
        assert_eq!(err, GuardError::NotASelect);
    }

    #[test]
    fn rejects_exec_procedure() {
        let err = assert_rejected("EXEC p", Dialect::MySql);
        assert!(matches!(
            err,
            GuardError::NotASelect | GuardError::Parser(_) | GuardError::Tokenizer(_)
        ));
    }

    #[test]
    fn rejects_merge() {
        let err = assert_rejected(
            "MERGE INTO t USING u ON (t.id = u.id) WHEN MATCHED THEN UPDATE SET t.a = u.a",
            Dialect::Oracle,
        );
        assert_eq!(err, GuardError::NotASelect);
    }

    #[test]
    fn rejects_truncate() {
        let err = assert_rejected("TRUNCATE TABLE t", Dialect::Postgres);
        assert_eq!(err, GuardError::NotASelect);
    }

    #[test]
    fn rejects_grant() {
        let err = assert_rejected("GRANT SELECT ON t TO u", Dialect::Postgres);
        assert!(matches!(
            err,
            GuardError::NotASelect | GuardError::Parser(_) | GuardError::Tokenizer(_)
        ));
    }

    // ---------- bypass: homóglifos / unicode ----------

    #[test]
    fn rejects_cyrillic_lookalike_keyword() {
        // "Е" (U+0415, CYRILLIC CAPITAL LETTER IE) no lugar do "E" latino.
        let err = assert_rejected("S\u{0415}LECT 1 FROM dual", Dialect::Oracle);
        assert!(matches!(
            err,
            GuardError::NonAsciiToken { .. } | GuardError::Parser(_) | GuardError::Tokenizer(_)
        ));
    }

    #[test]
    fn rejects_zero_width_space_inside_identifier() {
        let err = assert_rejected("SELECT col\u{200B}umn FROM t", Dialect::Postgres);
        assert_eq!(
            err,
            GuardError::Lexical(LexicalError::ForbiddenInvisibleChar {
                byte_pos: 10,
                codepoint: 0x200B,
            })
        );
    }

    #[test]
    fn rejects_rtl_override_anywhere() {
        let err = assert_rejected("SELECT 1 FROM t \u{202E}", Dialect::Postgres);
        assert!(matches!(
            err,
            GuardError::Lexical(LexicalError::ForbiddenInvisibleChar { .. })
        ));
    }

    // ---------- bypass: funções proibidas ----------

    #[test]
    fn rejects_pg_read_file() {
        let err = assert_rejected("SELECT pg_read_file('/etc/passwd')", Dialect::Postgres);
        assert_eq!(
            err,
            GuardError::ForbiddenFunction {
                name: "pg_read_file".to_string()
            }
        );
    }

    #[test]
    fn rejects_oracle_dbms_lock_sleep() {
        let err = assert_rejected("SELECT dbms_lock.sleep(10) FROM dual", Dialect::Oracle);
        assert_eq!(
            err,
            GuardError::ForbiddenFunction {
                name: "dbms_lock.sleep".to_string()
            }
        );
    }

    #[test]
    fn rejects_mysql_load_file() {
        let err = assert_rejected("SELECT load_file('/etc/passwd')", Dialect::MySql);
        assert_eq!(
            err,
            GuardError::ForbiddenFunction {
                name: "load_file".to_string()
            }
        );
    }

    #[test]
    fn rejects_nextval() {
        let err = assert_rejected("SELECT nextval('s')", Dialect::Postgres);
        assert_eq!(
            err,
            GuardError::ForbiddenFunction {
                name: "nextval".to_string()
            }
        );
    }

    #[test]
    fn rejects_pg_sleep_hidden_in_where() {
        let err = assert_rejected(
            "SELECT * FROM t WHERE id = 1 AND pg_sleep(5) IS NOT NULL",
            Dialect::Postgres,
        );
        assert_eq!(
            err,
            GuardError::ForbiddenFunction {
                name: "pg_sleep".to_string()
            }
        );
    }

    // ---------- limites ----------

    #[test]
    fn rejects_deeply_nested_parens_without_stack_overflow() {
        let sql = format!(
            "SELECT 1 FROM t WHERE {}1=1{}",
            "(".repeat(1000),
            ")".repeat(1000)
        );
        let err = assert_rejected(&sql, Dialect::Postgres);
        assert_eq!(err, GuardError::RecursionLimitExceeded);
    }

    #[test]
    fn rejects_empty_sql() {
        let err = assert_rejected("", Dialect::Postgres);
        assert_eq!(err, GuardError::EmptyStatement);
    }

    #[test]
    fn rejects_comment_only_sql() {
        let err = assert_rejected("-- só um comentário\n", Dialect::Postgres);
        assert_eq!(err, GuardError::EmptyStatement);
    }

    // ---------- falsos positivos: devem ser aceitos ----------

    #[test]
    fn accepts_semicolon_inside_string_literal() {
        assert_accepted("SELECT 'a;b' FROM dual", Dialect::Oracle);
    }

    #[test]
    fn accepts_single_trailing_semicolon_and_strips_it() {
        let validated = assert_accepted("SELECT 1", Dialect::Postgres);
        assert_eq!(validated.as_str(), "SELECT 1");

        let validated = assert_accepted("SELECT 1;", Dialect::Postgres);
        assert_eq!(validated.as_str(), "SELECT 1");

        let validated = assert_accepted("SELECT 1;   \n", Dialect::Postgres);
        assert_eq!(validated.as_str(), "SELECT 1");
    }

    #[test]
    fn accepts_and_preserves_oracle_hint_byte_for_byte() {
        let sql = "SELECT /*+ FULL(t) */ * FROM t";
        let validated = assert_accepted(sql, Dialect::Oracle);
        assert_eq!(validated.as_str(), sql);
    }

    #[test]
    fn accepts_accented_portuguese_string_literal() {
        assert_accepted("SELECT 'ação' FROM dual", Dialect::Oracle);
    }

    #[test]
    fn accepts_accented_portuguese_comment() {
        assert_accepted(
            "-- consulta de ofertas em decurso\nSELECT 1 FROM dual",
            Dialect::Oracle,
        );
    }

    #[test]
    fn accepts_recursive_cte() {
        assert_accepted(
            "WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM t WHERE n < 10) \
             SELECT * FROM t",
            Dialect::Postgres,
        );
    }

    #[test]
    fn accepts_union_all() {
        assert_accepted("SELECT 1 AS x UNION ALL SELECT 2", Dialect::Postgres);
    }

    #[test]
    fn accepts_subquery_in_from() {
        assert_accepted("SELECT * FROM (SELECT 1 AS x) sub", Dialect::Postgres);
    }

    #[test]
    fn accepts_subquery_in_in_list() {
        assert_accepted(
            "SELECT * FROM t WHERE id IN (SELECT id FROM u)",
            Dialect::Postgres,
        );
    }

    #[test]
    fn accepts_subquery_in_exists() {
        assert_accepted(
            "SELECT * FROM t WHERE EXISTS (SELECT 1 FROM u WHERE u.id = t.id)",
            Dialect::Postgres,
        );
    }

    #[test]
    fn accepts_subquery_in_select_list() {
        assert_accepted(
            "SELECT (SELECT count(*) FROM u) AS total FROM t",
            Dialect::Postgres,
        );
    }

    #[test]
    fn accepts_quoted_identifier_with_space() {
        assert_accepted(r#"SELECT "Coluna Com Espaço" FROM t"#, Dialect::Postgres);
    }

    #[test]
    fn accepts_named_bind_parameter() {
        assert_accepted("SELECT * FROM t WHERE id = :id", Dialect::Oracle);
    }

    #[test]
    fn accepts_postgres_cast_operator_not_confused_with_bind() {
        assert_accepted("SELECT id::text FROM t", Dialect::Postgres);
    }
}
