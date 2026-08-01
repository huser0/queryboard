//! Comandos de query: salvar e executar uma única query parametrizada
//! (roadmap item 3/4 — flow multi-query é item 6, ainda não existe).

use std::collections::HashMap;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::db::driver::{Bind, ConnectionConfig, Limits, SecretRef};
use crate::db::value::ResultSet;
use crate::ipc::AppState;
use crate::model::{NewQuery, QuerySummary};
use crate::sql::params::{rewrite_placeholders, Cardinality, ParamBinding, DEFAULT_MAX_LIST_SIZE};
use crate::sql::{validate, GuardError};
use crate::store::{connections, queries};

#[tauri::command]
pub async fn query_list(state: tauri::State<'_, AppState>) -> Result<Vec<QuerySummary>, String> {
    queries::list(&state.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn query_create(
    state: tauri::State<'_, AppState>,
    input: NewQuery,
) -> Result<QuerySummary, String> {
    let connection = connections::get_by_slug(&state.pool, &input.connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    // Validado aqui (save-time) para dar erro cedo na UI; execute_select
    // também valida nunca confiando só nisto (defesa em camadas, §2).
    validate(&input.sql, connection.kind.as_dialect()).map_err(guard_error_to_string)?;
    queries::create(&state.pool, &input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn query_run(
    state: tauri::State<'_, AppState>,
    execution_id: String,
    slug: String,
    params: HashMap<String, String>,
) -> Result<ResultSet, String> {
    let query = queries::get_by_slug(&state.pool, &slug)
        .await
        .map_err(|e| e.to_string())?;
    let connection = connections::get_by_slug(&state.pool, &query.connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    let driver = state.driver_for(connection.kind)?;

    let dialect = connection.kind.as_dialect();
    let validated = validate(&query.sql, dialect).map_err(guard_error_to_string)?;

    let mut bindings = Vec::with_capacity(query.params.len());
    let mut bind_values = Vec::with_capacity(query.params.len());
    for declared in &query.params {
        let raw = params
            .get(&declared.name)
            .ok_or_else(|| format!("parâmetro obrigatório não informado: {}", declared.name))?;
        bindings.push(ParamBinding {
            name: declared.name.clone(),
            cardinality: Cardinality::Scalar,
        });
        bind_values.push(to_bind(&declared.param_type, raw));
    }

    let rewritten = rewrite_placeholders(&validated, &bindings, DEFAULT_MAX_LIST_SIZE)
        .map_err(|e| e.to_string())?;

    let cfg = ConnectionConfig {
        dialect,
        host: connection.host,
        port: connection.port,
        database: connection.database,
        service_name: connection.service_name,
        username: connection.username,
    };
    let secret = SecretRef::new(connection.id);
    let limits = Limits {
        max_rows: connection.max_rows as usize,
        timeout: Duration::from_millis(connection.timeout_ms as u64),
        ..Limits::default()
    };

    let mut session = driver
        .connect(&cfg, &secret)
        .await
        .map_err(|e| e.to_string())?;
    session
        .begin_read_only(&limits)
        .await
        .map_err(|e| e.to_string())?;

    let cancel = CancellationToken::new();
    state
        .register_running(execution_id.clone(), cancel.clone())
        .await;

    let result = session
        .execute_select(&rewritten.validated, &bind_values, &limits, cancel)
        .await;

    state.unregister_running(&execution_id).await;
    let _ = session.rollback_and_close().await;

    result.map_err(|e| e.to_string())
}

fn to_bind(param_type: &str, raw: &str) -> Bind {
    match param_type {
        "number" => {
            if let Ok(i) = raw.parse::<i64>() {
                Bind::Int(i)
            } else {
                Bind::Decimal(raw.to_string())
            }
        }
        _ => Bind::Text(raw.to_string()),
    }
}

fn guard_error_to_string(err: GuardError) -> String {
    err.to_string()
}
