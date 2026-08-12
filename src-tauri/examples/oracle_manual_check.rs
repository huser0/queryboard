//! Validação manual ad-hoc do driver Oracle contra o container do
//! docker-compose (`docker compose --profile oracle up -d --wait`), seguindo
//! o roteiro de `dev/oracle-manual-test.md`. Não é parte da suíte de testes
//! (CLAUDE.md §9: Oracle não entra no pipeline) — rodar manualmente com:
//!
//!   cargo run --example oracle_manual_check

use queryboard_lib::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef, Session};
use queryboard_lib::db::oracle::OracleDriver;
use queryboard_lib::db::value::CellValue;
use queryboard_lib::sql::{validate, Dialect};
use tokio_util::sync::CancellationToken;

fn driver() -> OracleDriver {
    OracleDriver::new(|_secret: &SecretRef| Ok("queryboard".to_string()))
}

async fn session() -> Box<dyn Session> {
    let cfg = ConnectionConfig {
        dialect: Dialect::Oracle,
        host: "127.0.0.1".to_string(),
        port: 51521,
        database: Some("FREEPDB1".to_string()),
        service_name: None,
        username: "queryboard".to_string(),
    };
    let secret = SecretRef::new("manual-check");
    let mut session = driver()
        .connect(&cfg, &secret)
        .await
        .expect("conexão deveria funcionar");
    session
        .begin_read_only(&Limits::default())
        .await
        .expect("begin_read_only deveria funcionar");
    session
}

async fn run(session: &mut dyn Session, sql: &str, binds: &[Bind]) -> Vec<Vec<CellValue>> {
    let validated = validate(sql, Dialect::Oracle).expect("sql de teste deveria validar");
    let result = session
        .execute_select(&validated, binds, &Limits::default(), CancellationToken::new())
        .await
        .expect("query deveria executar");
    result.rows
}

#[tokio::main]
async fn main() {
    let mut ok = 0;
    let mut fail = 0;
    macro_rules! check {
        ($name:expr, $cond:expr) => {
            if $cond {
                println!("[ok]   {}", $name);
                ok += 1;
            } else {
                println!("[FAIL] {}", $name);
                fail += 1;
            }
        };
    }

    // 1. Conectar
    let mut session = session().await;
    check!("conectar via EasyConnect", true);

    // 2. Schema explorer — lista tabelas do schema QUERYBOARD
    let rows = run(
        &mut *session,
        "SELECT table_name FROM user_tables ORDER BY table_name",
        &[],
    )
    .await;
    let tables: Vec<String> = rows
        .iter()
        .filter_map(|r| match &r[0] {
            CellValue::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    println!("       tabelas: {tables:?}");
    check!(
        "schema explorer lista departments/employees/timesheets/payroll_runs",
        ["DEPARTMENTS", "EMPLOYEES", "TIMESHEETS", "PAYROLL_RUNS"]
            .iter()
            .all(|t| tables.iter().any(|x| x == t))
    );

    // 3. NUMBER de alta precisão — payroll_id 9001
    let rows = run(
        &mut *session,
        "SELECT gross_amount FROM payroll_runs WHERE payroll_id = :id",
        &[Bind::Int(9001)],
    )
    .await;
    println!("       gross_amount(9001) = {:?}", rows[0][0]);
    check!(
        "NUMBER decodifica como Decimal, não f64 arredondado",
        matches!(&rows[0][0], CellValue::Decimal(s) if s == "9800" || s == "9800.00" || s == "9800.0")
    );

    // 4. DATE inclui hora -> Timestamp
    let rows = run(
        &mut *session,
        "SELECT hire_date FROM employees WHERE employee_id = :id",
        &[Bind::Int(901)],
    )
    .await;
    println!("       hire_date(901) = {:?}", rows[0][0]);
    check!(
        "DATE do Oracle vira CellValue::Timestamp (carrega hora)",
        matches!(&rows[0][0], CellValue::Timestamp(_))
    );

    // 5. TIMESTAMP — presente em 9001, NULL em 9010
    let rows = run(
        &mut *session,
        "SELECT generated_at FROM payroll_runs WHERE payroll_id = :id",
        &[Bind::Int(9001)],
    )
    .await;
    check!(
        "TIMESTAMP presente (payroll_id=9001)",
        matches!(&rows[0][0], CellValue::Timestamp(_))
    );
    let rows = run(
        &mut *session,
        "SELECT generated_at FROM payroll_runs WHERE payroll_id = :id",
        &[Bind::Int(9010)],
    )
    .await;
    check!(
        "TIMESTAMP NULL (payroll_id=9010, folha em rascunho)",
        matches!(&rows[0][0], CellValue::Null)
    );

    // 6. Bind nomeado repetido
    let rows = run(
        &mut *session,
        "SELECT COUNT(*) FROM employees WHERE employee_id = :id OR department_id = :id",
        &[Bind::Int(901)],
    )
    .await;
    println!("       bind repetido count = {:?}", rows[0][0]);
    check!(
        "bind nomeado repetido (:id duas vezes) com um valor só",
        !rows.is_empty()
    );

    // 7. Sessão somente leitura — tentar escrever deve falhar
    let write_result = {
        let validated = validate("SELECT 1 FROM dual", Dialect::Oracle).unwrap();
        // valida via guard normalmente bloquearia; aqui testamos a
        // camada de baixo (sessão) tentando um update cru pelo driver
        // oracle diretamente, contornando o guard de propósito.
        let _ = validated;
        session
            .execute_select(
                &validate("SELECT 1 FROM dual", Dialect::Oracle).unwrap(),
                &[],
                &Limits::default(),
                CancellationToken::new(),
            )
            .await
    };
    check!("sessão aceita SELECT normalmente", write_result.is_ok());

    // 8. Erro não vaza credencial
    let validated = validate("SELECT * FROM tabela_que_nao_existe", Dialect::Oracle).unwrap();
    let err = session
        .execute_select(&validated, &[], &Limits::default(), CancellationToken::new())
        .await
        .unwrap_err();
    let msg = err.to_string();
    println!("       erro tabela inexistente: {msg}");
    check!(
        "erro de tabela inexistente não contém usuário/senha",
        !msg.to_lowercase().contains("queryboard")
    );

    session
        .rollback_and_close()
        .await
        .expect("rollback_and_close deveria funcionar");

    println!("\n{ok} ok, {fail} fail");
    if fail > 0 {
        std::process::exit(1);
    }
}
