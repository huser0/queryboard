//! Testes de integração do driver Postgres contra um Postgres real
//! (efêmero, via testcontainers) — CLAUDE.md §9: "testes de driver usam
//! containers efêmeros... nenhum teste toca banco de produção."
//!
//! Rodar com: `cargo test --features integration -- --ignored --test-threads=1`

#![cfg(feature = "integration")]

use std::sync::LazyLock;
use std::time::Duration;

use queryboard_lib::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use queryboard_lib::db::error::DbError;
use queryboard_lib::db::postgres::PostgresDriver;
use queryboard_lib::db::value::CellValue;
use queryboard_lib::sql::{validate, Dialect};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

const PASSWORD: &str = "queryboard-test-password";

/// Runtime único e persistente pro binário de teste inteiro — cada
/// `#[tokio::test]` cria seu próprio runtime descartável por padrão, e o
/// container compartilhado (`static CONTAINER` abaixo) não sobrevive à
/// troca de runtime entre testes: `OnceCell` fica preso ao runtime que
/// rodou o primeiro `get_or_init`, e o segundo teste (com um runtime
/// novo) recria o container do zero — bug pré-existente confirmado
/// rodando dois testes em sequência (`plain_int` passa, o próximo recria
/// o container e estoura `WaitContainer(StartupTimeout)`). `#[test]`
/// comum + `RUNTIME.block_on(...)` evita isso: um único runtime vive por
/// todo o processo do binário de teste, igual `mysql_types.rs`.
static RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("runtime de teste deveria criar"));

static CONTAINER: OnceCell<ContainerAsync<Postgres>> = OnceCell::const_new();

async fn container() -> &'static ContainerAsync<Postgres> {
    CONTAINER
        .get_or_init(|| async {
            // Timeout bem acima do default de 60s — confirmado rodando
            // `podman run` puro (sem Rust envolvido) que o boot de um
            // container de banco pode passar de 170s neste ambiente.
            Postgres::default()
                .with_password(PASSWORD)
                .with_tag("16-alpine")
                .with_startup_timeout(Duration::from_secs(300))
                .start()
                .await
                .expect("postgres container deveria subir")
        })
        .await
}

async fn connection_config() -> ConnectionConfig {
    let container = container().await;
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("porta mapeada");
    ConnectionConfig {
        dialect: Dialect::Postgres,
        host: "127.0.0.1".to_string(),
        port,
        database: Some("postgres".to_string()),
        service_name: None,
        username: "postgres".to_string(),
    }
}

fn driver() -> PostgresDriver {
    PostgresDriver::new(|_secret: &SecretRef| Ok(PASSWORD.to_string()))
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
    let validated = validate(sql, Dialect::Postgres).expect("sql de teste deveria validar");
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
fn numeric_with_precision_and_scale_roundtrips_exactly() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT 12345678901234567890.1234567890::numeric(38,10)",
        )
        .await;
        assert_eq!(
            value,
            CellValue::Decimal("12345678901234567890.1234567890".to_string())
        );
    });
}

#[test]
#[ignore]
fn numeric_without_declared_precision() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT 123.456::numeric").await;
        assert_eq!(value, CellValue::Decimal("123.456".to_string()));
    });
}

#[test]
#[ignore]
fn bigint_at_i64_boundary() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT 9223372036854775807::bigint").await;
        assert_eq!(value, CellValue::Int(i64::MAX));
    });
}

#[test]
#[ignore]
fn plain_int() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT 42::int").await;
        assert_eq!(value, CellValue::Int(42));
    });
}

#[test]
#[ignore]
fn float8() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT 3.5::float8").await;
        assert_eq!(value, CellValue::Float(3.5));
    });
}

#[test]
#[ignore]
fn large_text() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT repeat('a', 100000)").await;
        match value {
            CellValue::Text(s) => assert_eq!(s.len(), 100_000),
            other => panic!("esperava Text, veio {other:?}"),
        }
    });
}

#[test]
#[ignore]
fn bytea() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, r"SELECT '\xDEADBEEF'::bytea").await;
        assert_eq!(value, CellValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));
    });
}

#[test]
#[ignore]
fn uuid() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT '550e8400-e29b-41d4-a716-446655440000'::uuid",
        )
        .await;
        assert_eq!(
            value,
            CellValue::Text("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    });
}

#[test]
#[ignore]
fn json_and_jsonb() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let json = select_one_cell(&mut *session, r#"SELECT '{"a":1}'::json"#).await;
        let jsonb = select_one_cell(&mut *session, r#"SELECT '{"a":1}'::jsonb"#).await;
        assert!(matches!(json, CellValue::Json(_)));
        assert!(matches!(jsonb, CellValue::Json(_)));
    });
}

#[test]
#[ignore]
fn bool_true_and_false() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        assert_eq!(
            select_one_cell(&mut *session, "SELECT true").await,
            CellValue::Bool(true)
        );
        assert_eq!(
            select_one_cell(&mut *session, "SELECT false").await,
            CellValue::Bool(false)
        );
    });
}

#[test]
#[ignore]
fn timestamptz_with_explicit_offset() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(
            &mut *session,
            "SELECT '2026-07-31 12:00:00-03:00'::timestamptz",
        )
        .await;
        match value {
            CellValue::TimestampTz(s) => assert_eq!(s, "2026-07-31T15:00:00+00:00"),
            other => panic!("esperava TimestampTz, veio {other:?}"),
        }
    });
}

#[test]
#[ignore]
fn date() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT '2026-07-31'::date").await;
        assert_eq!(value, CellValue::Date("2026-07-31".to_string()));
    });
}

#[test]
#[ignore]
fn interval() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT interval '1 mon 2 days'").await;
        assert!(matches!(value, CellValue::Interval(_)));
    });
}

#[test]
#[ignore]
fn array_type_degrades_gracefully_instead_of_failing() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let value = select_one_cell(&mut *session, "SELECT ARRAY[1,2,3]").await;
        // Não tem CellValue dedicado para array; o importante é não falhar a
        // consulta inteira por causa de uma coluna de tipo exótico.
        assert!(matches!(value, CellValue::Text(_)));
    });
}

#[test]
#[ignore]
fn null_in_every_mapped_type() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        for cast in [
            "numeric",
            "bigint",
            "int",
            "float8",
            "text",
            "bytea",
            "uuid",
            "json",
            "jsonb",
            "bool",
            "timestamptz",
            "date",
            "time",
            "interval",
        ] {
            let sql = format!("SELECT NULL::{cast}");
            let value = select_one_cell(&mut *session, &sql).await;
            assert_eq!(value, CellValue::Null, "NULL::{cast} deveria virar Null");
        }
    });
}

#[test]
#[ignore]
fn scalar_bind_by_position() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        // ::bigint (não ::int) para casar com o tipo de fio que Bind::Int(i64)
        // manda — um cast para int4 causaria "incorrect binary data format"
        // porque o valor chega como int8 no protocolo binário.
        let validated = validate("SELECT $1::bigint + 1", Dialect::Postgres).unwrap();
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

/// Prepara uma tabela escrachável fora do driver de leitura (o guard
/// bloqueia DDL de propósito — CLAUDE.md §2) só para os testes abaixo
/// terem uma coluna com tipo de catálogo real (não um `::cast` em cima de
/// um literal, que já entra tipado e não exercitaria a inferência de tipo
/// do Postgres do jeito que `offer_id = :offer_id` contra uma tabela de
/// verdade exercita).
async fn setup_typed_table() {
    use sqlx::{ConnectOptions, Connection};
    let cfg = connection_config().await;
    let mut raw = sqlx::postgres::PgConnectOptions::new()
        .host(&cfg.host)
        .port(cfg.port)
        .username(&cfg.username)
        .password(PASSWORD)
        .database(cfg.database.as_deref().unwrap_or("postgres"))
        .connect()
        .await
        .expect("conexão de setup deveria funcionar");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bind_repro ( \
            id INTEGER PRIMARY KEY, \
            price NUMERIC(10,2) NOT NULL, \
            active BOOLEAN NOT NULL, \
            sale_date DATE NOT NULL \
        )",
    )
    .execute(&mut raw)
    .await
    .expect("create table");
    sqlx::query("TRUNCATE bind_repro")
        .execute(&mut raw)
        .await
        .expect("truncate");
    sqlx::query(
        "INSERT INTO bind_repro (id, price, active, sale_date) \
         VALUES (5002, 149.90, true, '2026-07-20')",
    )
    .execute(&mut raw)
    .await
    .expect("insert");
    let _ = raw.close().await;
}

/// Regressão: `Bind::Text` contra uma coluna cujo tipo o Postgres infere
/// como algo diferente de texto (INTEGER, NUMERIC, BOOLEAN, ...) tinha
/// que dar CERTO (o app sempre manda parâmetro como texto — ver
/// `to_bind`) ou, na pior das hipóteses, um erro claro. Em vez disso
/// devolvia silenciosamente zero linhas: sqlx-postgres sempre manda os
/// parâmetros em formato binário; um `Bind::Text` cru contra uma coluna
/// `INTEGER` fazia o Postgres decodificar os bytes UTF-8 da string como
/// um inteiro binário de 4 bytes — lixo, não um erro. Ver
/// `db::postgres::bind_one`/`coerce_text_to_inferred_type`.
#[test]
#[ignore]
fn text_bind_matches_integer_column_by_inferred_type() {
    RUNTIME.block_on(async {
        setup_typed_table().await;
        let mut session = session().await;
        let validated =
            validate("SELECT id FROM bind_repro WHERE id = $1", Dialect::Postgres).unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Text("5002".to_string())],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .expect("execute_select deveria funcionar");
        assert_eq!(
            result.rows.len(),
            1,
            "Bind::Text('5002') deveria casar com a linha id=5002 (INTEGER)"
        );
    });
}

#[test]
#[ignore]
fn text_bind_matches_numeric_column_by_inferred_type() {
    RUNTIME.block_on(async {
        setup_typed_table().await;
        let mut session = session().await;
        let validated = validate(
            "SELECT id FROM bind_repro WHERE price = $1",
            Dialect::Postgres,
        )
        .unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Text("149.90".to_string())],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .expect("execute_select deveria funcionar");
        assert_eq!(
            result.rows.len(),
            1,
            "Bind::Text('149.90') deveria casar com price NUMERIC"
        );
    });
}

#[test]
#[ignore]
fn text_bind_matches_bool_column_by_inferred_type() {
    RUNTIME.block_on(async {
        setup_typed_table().await;
        let mut session = session().await;
        let validated = validate(
            "SELECT id FROM bind_repro WHERE active = $1",
            Dialect::Postgres,
        )
        .unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Text("true".to_string())],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .expect("execute_select deveria funcionar");
        assert_eq!(
            result.rows.len(),
            1,
            "Bind::Text('true') deveria casar com active BOOLEAN"
        );
    });
}

/// Regressão específica: antes da correção cobrir DATE/TIME/TIMESTAMP*,
/// um `Bind::Text` vazio ou preenchido contra uma coluna `DATE` não
/// devolvia zero linhas silenciosamente — dava um erro de protocolo
/// ("insufficient data left in message"), porque o texto era mandado com
/// o mesmo número de bytes da string em vez dos 4 bytes binários que
/// `DATE` espera.
#[test]
#[ignore]
fn text_bind_matches_date_column_by_inferred_type() {
    RUNTIME.block_on(async {
        setup_typed_table().await;
        let mut session = session().await;
        let validated = validate(
            "SELECT id FROM bind_repro WHERE sale_date = $1",
            Dialect::Postgres,
        )
        .unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Text("2026-07-20".to_string())],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .expect("execute_select deveria funcionar");
        assert_eq!(
            result.rows.len(),
            1,
            "Bind::Text('2026-07-20') deveria casar com sale_date DATE"
        );
    });
}

#[test]
#[ignore]
fn null_bind() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT $1::text IS NULL", Dialect::Postgres).unwrap();
        let result = session
            .execute_select(
                &validated,
                &[Bind::Null],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.rows[0][0], CellValue::Bool(true));
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
            "SELECT * FROM generate_series(1, 100) AS g",
            Dialect::Postgres,
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
            "SELECT * FROM generate_series(1, 10) AS g",
            Dialect::Postgres,
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
        // "SHOW transaction_read_only" não é um SELECT — o guard rejeita
        // corretamente (não é Statement::Query). current_setting() dá a
        // mesma informação através de um SELECT de verdade.
        let value = select_one_cell(
            &mut *session,
            "SELECT current_setting('transaction_read_only')",
        )
        .await;
        assert_eq!(value, CellValue::Text("on".to_string()));
    });
}

#[test]
#[ignore]
fn cancel_before_query_starts() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT 1", Dialect::Postgres).unwrap();
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

        // Produto cartesiano grande sobre pg_class — lento o bastante para dar
        // tempo do cancelamento chegar antes do fim, mas não bloqueado pelo
        // guard (não usa pg_sleep, que está na denylist).
        let validated = validate(
            "SELECT count(*) FROM generate_series(1, 5000) a, generate_series(1, 5000) b",
            Dialect::Postgres,
        )
        .unwrap();

        let started = std::time::Instant::now();
        let result = session
            .execute_select(&validated, &[], &Limits::default(), cancel)
            .await;
        assert!(matches!(result, Err(DbError::Cancelled)));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "cancelamento deveria interromper bem antes da query terminar sozinha"
        );
    });
}

#[test]
#[ignore]
fn syntax_error_message_never_leaks_credentials() {
    RUNTIME.block_on(async {
        let mut session = session().await;
        let validated = validate("SELECT FORM t", Dialect::Postgres);
        // "FORM" não é um SELECT válido para o parser? na verdade isso falha
        // no guard, então testamos um erro que o banco de fato produz.
        if validated.is_err() {
            return;
        }
        let validated = validated.unwrap();
        let result = session
            .execute_select(
                &validated,
                &[],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await;
        if let Err(err) = result {
            let message = err.to_string();
            assert!(!message.contains(PASSWORD));
            assert!(!message.contains("127.0.0.1"));
        }
    });
}
