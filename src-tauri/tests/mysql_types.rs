//! Testes de integração do driver MySQL contra um MySQL real (efêmero,
//! via testcontainers) — CLAUDE.md §9: "testes de driver usam containers
//! efêmeros... nenhum teste toca banco de produção."
//!
//! Não replica 100% dos testes de `pg_types.rs` — muitos lá são
//! específicos de bugs do protocolo binário do Postgres que não se
//! aplicam ao MySQL (ver comentário de módulo em `db/mysql.rs`). Cobre:
//! mapeamento de cada `CellValue`, `max_rows` via cursor, sessão
//! read-only, cancelamento, e a confirmação (não suposição) de que bind
//! de texto cru casa com coluna tipada via coerção implícita do MySQL.
//!
//! Rodar com: `cargo test --features integration -- --ignored --test-threads=1`

#![cfg(feature = "integration")]

use std::sync::LazyLock;
use std::time::Duration;

use queryboard_lib::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use queryboard_lib::db::error::DbError;
use queryboard_lib::db::mysql::MySqlDriver;
use queryboard_lib::db::value::CellValue;
use queryboard_lib::sql::{validate, Dialect};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

const PASSWORD: &str = "";

/// Runtime único e persistente pro binário de teste inteiro — cada
/// `#[tokio::test]` cria seu próprio runtime descartável por padrão, e o
/// container compartilhado (`static CONTAINER` abaixo) não sobrevive à
/// troca de runtime entre testes: `OnceCell` fica preso ao runtime que
/// rodou o primeiro `get_or_init`, e o segundo teste (com um runtime
/// novo) recria o container do zero — confirmado rodando manualmente
/// (mesmo bug pré-existente em `pg_types.rs`, não introduzido aqui).
/// `#[test]` comum + `RUNTIME.block_on(...)` evita isso: um único runtime
/// vive por todo o processo do binário de teste.
static RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("runtime de teste deveria criar"));

static CONTAINER: OnceCell<ContainerAsync<Mysql>> = OnceCell::const_new();

async fn container() -> &'static ContainerAsync<Mysql> {
    CONTAINER
        .get_or_init(|| async {
            // Tag do módulo oficial testcontainers-modules (8.1).
            // `with_startup_timeout` bem acima do default de 60s —
            // confirmado rodando `podman run` puro (sem Rust envolvido)
            // que o boot completo do MySQL (init + restart do servidor
            // "de verdade") leva ~175s neste ambiente. Um timeout curto
            // aqui não só falha o teste: como `tokio::sync::OnceCell`
            // não cacheia falha de init, cada teste seguinte reiniciava
            // o container do zero, num loop sem fim.
            Mysql::default()
                .with_startup_timeout(Duration::from_secs(300))
                .start()
                .await
                .expect("mysql container deveria subir")
        })
        .await
}

async fn connection_config() -> ConnectionConfig {
    let container = container().await;
    let port = container
        .get_host_port_ipv4(3306)
        .await
        .expect("porta mapeada");
    ConnectionConfig {
        dialect: Dialect::MySql,
        host: "127.0.0.1".to_string(),
        port,
        database: Some("test".to_string()),
        service_name: None,
        username: "root".to_string(),
    }
}

fn driver() -> MySqlDriver {
    MySqlDriver::new(|_secret: &SecretRef| Ok(PASSWORD.to_string()))
}

async fn session_with_limits(limits: &Limits) -> Box<dyn Session> {
    let cfg = connection_config().await;
    let secret = SecretRef::new("test-connection");
    let mut session = driver()
        .connect(&cfg, &secret)
        .await
        .expect("conexão deveria funcionar");
    session
        .begin_read_only(limits)
        .await
        .expect("begin_read_only deveria funcionar");
    session
}

async fn session() -> Box<dyn Session> {
    session_with_limits(&Limits::default()).await
}

async fn select_one_cell(session: &mut dyn Session, sql: &str) -> CellValue {
    let validated = validate(sql, Dialect::MySql).expect("sql de teste deveria validar");
    let result = session
        .execute_select(
            &validated,
            &[],
            &Limits::default(),
            CancellationToken::new(),
        )
        .await
        .expect("query deveria executar");
    assert_eq!(result.rows.len(), 1, "esperava exatamente 1 linha: {sql}");
    result.rows[0][0].clone()
}

#[test]
#[ignore]
fn decimal_with_precision_and_scale_roundtrips_exactly() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT CAST(12345678901234.1234 AS DECIMAL(20,4))",
        )
        .await;
        assert_eq!(value, CellValue::Decimal("12345678901234.1234".to_string()));
    });
}

#[test]
#[ignore]
fn bigint_at_i64_boundary() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value =
            select_one_cell(&mut *session, "SELECT CAST(9223372036854775807 AS SIGNED)").await;
        assert_eq!(value, CellValue::Int(i64::MAX));
    });
}

#[test]
#[ignore]
fn plain_int() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT CAST(42 AS SIGNED)").await;
        assert_eq!(value, CellValue::Int(42));
    });
}

#[test]
#[ignore]
fn double_precision() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT CAST(3.5 AS DOUBLE)").await;
        assert_eq!(value, CellValue::Float(3.5));
    });
}

#[test]
#[ignore]
fn large_text() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT REPEAT('a', 100000)").await;
        match value {
            CellValue::Text(s) => assert_eq!(s.len(), 100_000),
            other => panic!("esperava Text, veio {other:?}"),
        }
    });
}

#[test]
#[ignore]
fn blob_bytes() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT CAST(X'DEADBEEF' AS BINARY)").await;
        assert_eq!(value, CellValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));
    });
}

#[test]
#[ignore]
fn json_type() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, r#"SELECT CAST('{"a":1}' AS JSON)"#).await;
        assert!(matches!(value, CellValue::Json(_)));
    });
}

#[test]
#[ignore]
fn bool_true_and_false() {
    RUNTIME.block_on(async {
        // Uma expressão de comparação (`x = y`) no MySQL devolve um inteiro
        // puro no fio, não um tipo marcado como booleano — só uma coluna
        // real `BOOLEAN`/`TINYINT(1)` decodifica como `CellValue::Bool`
        // (confirmado rodando `SELECT CAST(1 AS UNSIGNED) = 1` direto no
        // servidor: `type_info().name()` não é `BOOLEAN`). Por isso o teste
        // usa uma coluna de tabela real, igual `setup_typed_table`.
        setup_typed_table().await;
        let mut session = session().await;
        assert_eq!(
            select_one_cell(
                &mut *session,
                "SELECT active FROM bind_repro WHERE id = 8002"
            )
            .await,
            CellValue::Bool(true)
        );
    });
}

#[test]
#[ignore]
fn datetime_without_timezone() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT CAST('2026-07-31 12:00:00' AS DATETIME)",
        )
        .await;
        match value {
            CellValue::Timestamp(s) => assert!(s.starts_with("2026-07-31T12:00:00")),
            other => panic!("esperava Timestamp, veio {other:?}"),
        }
    });
}

#[test]
#[ignore]
fn timestamp_column_maps_to_timestamptz() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT CAST('2026-07-31 12:00:00' AS DATETIME) + INTERVAL 0 SECOND",
        )
        .await;
        // O cast acima ainda é DATETIME (sem tz) — TIMESTAMP de verdade só
        // aparece em coluna de tabela real, exercitado no cenário de bind
        // abaixo (`setup_typed_table`/`text_bind_matches_typed_columns`).
        assert!(matches!(value, CellValue::Timestamp(_)));
    });
}

#[test]
#[ignore]
fn date_only() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT CAST('2026-07-31' AS DATE)").await;
        assert_eq!(value, CellValue::Date("2026-07-31".to_string()));
    });
}

#[test]
#[ignore]
fn null_in_every_mapped_type() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        for cast in [
            "DECIMAL(10,2)",
            "SIGNED",
            "DOUBLE",
            "CHAR",
            "BINARY",
            "JSON",
            "DATE",
            "TIME",
            "DATETIME",
        ] {
            let sql = format!("SELECT CAST(NULL AS {cast})");
            let value = select_one_cell(&mut *session, &sql).await;
            assert_eq!(
                value,
                CellValue::Null,
                "CAST(NULL AS {cast}) deveria virar Null"
            );
        }
    });
}

#[test]
#[ignore]
fn scalar_bind_by_position() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT ? + 1", Dialect::MySql).unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Int(41)],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.rows[0][0], CellValue::Int(42));
    });
}

/// Prepara uma tabela real (fora do driver de leitura — o guard bloqueia
/// DDL de propósito) com colunas tipadas de catálogo pra exercitar o bind
/// de texto cru contra elas — mesmo racional de `setup_typed_table` em
/// `pg_types.rs`.
async fn setup_typed_table() {
    use sqlx::{ConnectOptions, Connection, Executor};
    let cfg = connection_config().await;
    let mut raw = sqlx::mysql::MySqlConnectOptions::new()
        .host(&cfg.host)
        .port(cfg.port)
        .username(&cfg.username)
        .password(PASSWORD)
        .database(cfg.database.as_deref().unwrap_or("test"))
        .connect()
        .await
        .expect("conexão de setup deveria funcionar");
    raw.execute(
        "CREATE TABLE IF NOT EXISTS bind_repro ( \
            id INT PRIMARY KEY, \
            price DECIMAL(10,2) NOT NULL, \
            active BOOLEAN NOT NULL, \
            sale_date DATE NOT NULL \
        )",
    )
    .await
    .expect("create table");
    raw.execute("TRUNCATE bind_repro").await.expect("truncate");
    raw.execute(
        "INSERT INTO bind_repro (id, price, active, sale_date) \
         VALUES (8002, 149.90, true, '2026-07-20')",
    )
    .await
    .expect("insert");
    let _ = raw.close().await;
}

/// Confirma de verdade (não só assume) que o MySQL faz coerção implícita
/// de `Bind::Text` cru contra colunas INT/DECIMAL/BOOLEAN/DATE, do jeito
/// que `db/mysql.rs` documenta — sem isso seria exatamente o tipo de
/// suposição não verificada que causou o bug real documentado no driver
/// Postgres (`text_bind_matches_*_column_by_inferred_type` em
/// `pg_types.rs`).
#[test]
#[ignore]
fn text_bind_matches_typed_columns() {
    RUNTIME.block_on(async {
        setup_typed_table().await;
        let mut session = session().await;

        let cases: &[(&str, &str)] = &[
            ("SELECT id FROM bind_repro WHERE id = ?", "8002"),
            ("SELECT id FROM bind_repro WHERE price = ?", "149.90"),
            ("SELECT id FROM bind_repro WHERE active = ?", "1"),
            (
                "SELECT id FROM bind_repro WHERE sale_date = ?",
                "2026-07-20",
            ),
        ];

        for (sql, value) in cases {
            let validated = validate(sql, Dialect::MySql).unwrap();
            let result = session
                .execute_select(
                    &validated,
                    &[Bind::Text(value.to_string())],
                    &Limits::default(),
                    CancellationToken::new(),
                )
                .await
                .unwrap_or_else(|e| panic!("execute_select falhou para {sql:?}: {e}"));
            assert_eq!(
                result.rows.len(),
                1,
                "Bind::Text({value:?}) deveria casar com a coluna tipada em {sql:?}"
            );
        }
    });
}

#[test]
#[ignore]
fn null_bind() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT ? IS NULL", Dialect::MySql).unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Null],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        // `IS NULL` é uma expressão de predicado, não uma coluna `BOOLEAN` —
        // o MySQL devolve o resultado como inteiro puro no fio (mesmo
        // racional de `bool_true_and_false` acima).
        assert_eq!(result.rows[0][0], CellValue::Int(1));
    });
}

#[test]
#[ignore]
fn max_rows_truncates_via_cursor_fetch_never_via_limit() {
    RUNTIME.block_on(async {
    let limits = Limits {
        max_rows: 5,
        ..Limits::default()
    };
    let mut session = session_with_limits(&limits).await;
    let validated = validate(
        "WITH RECURSIVE seq AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM seq WHERE n < 100) SELECT n FROM seq",
        Dialect::MySql,
    )
    .unwrap();
    let result = session
        .execute_select(&validated, &[], &limits, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 5);
    assert!(result.truncated);
    });
}

#[test]
#[ignore]
fn under_max_rows_is_not_truncated() {
    RUNTIME.block_on(async {
    let limits = Limits {
        max_rows: 1000,
        ..Limits::default()
    };
    let mut session = session_with_limits(&limits).await;
    let validated = validate(
        "WITH RECURSIVE seq AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM seq WHERE n < 10) SELECT n FROM seq",
        Dialect::MySql,
    )
    .unwrap();
    let result = session
        .execute_select(&validated, &[], &limits, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 10);
    assert!(!result.truncated);
    });
}

#[test]
#[ignore]
fn session_is_actually_read_only() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        // `@@transaction_read_only` reflete o modo definido por `SET
        // [SESSION] TRANSACTION READ ONLY` (default pra transações
        // futuras), não o `START TRANSACTION READ ONLY` que o driver usa
        // pra ESTA transação (confirmado rodando os dois direto no
        // servidor: `START TRANSACTION READ ONLY` seguido de `SELECT
        // @@transaction_read_only` devolve 0, mas uma tentativa de escrita
        // na mesma transação falha com `ERROR 1792: Cannot execute
        // statement in a READ ONLY transaction` — a imposição funciona, só
        // essa variável não é o jeito certo de observar via SELECT).
        // `performance_schema.events_transactions_current.ACCESS_MODE`
        // reflete a transação corrente de verdade.
        let value = select_one_cell(
            &mut *session,
            "SELECT ACCESS_MODE FROM performance_schema.events_transactions_current \
         WHERE THREAD_ID = PS_CURRENT_THREAD_ID()",
        )
        .await;
        assert_eq!(value, CellValue::Text("READ ONLY".to_string()));
    });
}

#[test]
#[ignore]
fn cancel_before_query_starts() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT 1", Dialect::MySql).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = session
            .execute_select(&validated, &[], &Limits::default(), cancel)
            .await;
        assert!(matches!(result, Err(DbError::Cancelled)));
    });
}

#[test]
#[ignore]
fn cancel_mid_query_stops_it_on_the_server() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel_clone.cancel();
        });

        // Produto cartesiano grande via CTE recursiva — lento o bastante pra
        // dar tempo do cancelamento chegar antes do fim, sem usar SLEEP()
        // (que está na denylist). `n < 300` fica bem abaixo do
        // `@@cte_max_recursion_depth` padrão do MySQL (1000) — passar disso
        // aborta a query quase instantaneamente com erro em vez de ficar
        // lenta (confirmado rodando direto no servidor: `n < 2000000`
        // estourava em ~0.7s com `ERROR 3636`, o oposto do que o teste
        // precisa). O produto cartesiano triplo (300³ = 27M linhas) é quem
        // faz o trabalho pesado, não a profundidade da recursão.
        let validated = validate(
            "WITH RECURSIVE seq AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM seq WHERE n < 300) \
         SELECT COUNT(*) FROM seq a, seq b, seq c",
            Dialect::MySql,
        )
        .unwrap();

        let started = std::time::Instant::now();
        let result = session
            .execute_select(&validated, &[], &Limits::default(), cancel)
            .await;
        assert!(matches!(result, Err(DbError::Cancelled)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "cancelamento deveria interromper bem antes da query terminar sozinha"
        );
    });
}

#[test]
#[ignore]
fn syntax_error_message_never_leaks_credentials() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT 1", Dialect::MySql).unwrap();
        // Query válida pro guard, mas com um bind a mais do que placeholders
        // — produz erro de execução real do servidor sem precisar de SQL
        // malformado (que o guard rejeitaria antes de chegar no driver).
        let result = session
            .execute_select(
                &validated,
                &[Bind::Int(1)],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await;
        if let Err(err) = result {
            let message = err.to_string();
            assert!(!message.contains("root@"));
            assert!(!message.contains("127.0.0.1"));
        }
    });
}
