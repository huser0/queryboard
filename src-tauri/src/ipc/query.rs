//! Comandos de query: salvar e executar uma única query parametrizada
//! (roadmap item 3/4 — flow multi-query é item 6, ainda não existe).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::db::driver::{Bind, ConnectionConfig, Driver, Limits, SecretRef};
use crate::db::error::DbError;
use crate::db::value::{CellValue, ResultSet};
use crate::ipc::AppState;
use crate::model::{ConnectionSummary, NewQuery, QuerySummary};
use crate::sql::params::{
    extract_params, rewrite_placeholders, Cardinality, ParamBinding, DEFAULT_MAX_LIST_SIZE,
};
use crate::sql::{validate, Dialect, GuardError, ValidatedSql};
use crate::store::{connections, queries};

/// Extrai os nomes de parâmetro de um SQL ainda não salvo — é o que
/// alimenta o painel de parâmetros do editor em tempo real, reusando a
/// mesma tokenização do guard (`sql::params::extract_params`), nunca uma
/// heurística à parte no frontend. Reusado por `query_run_adhoc` para
/// decidir quais binds montar.
fn ordered_param_names(validated: &ValidatedSql) -> Vec<String> {
    let mut names = Vec::new();
    for occurrence in extract_params(validated) {
        if !names.contains(&occurrence.name) {
            names.push(occurrence.name);
        }
    }
    names
}

#[tauri::command]
pub async fn query_extract_params(
    state: tauri::State<'_, AppState>,
    sql: String,
    connection_slug: String,
) -> Result<Vec<String>, String> {
    let connection = connections::get_by_slug(&state.pool, &connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    let validated = validate(&sql, connection.kind.as_dialect()).map_err(guard_error_to_string)?;
    Ok(ordered_param_names(&validated))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Lista tabelas/colunas da connection pro explorador de schema da
/// sidebar. Nunca introspecciona via API privilegiada do driver — monta
/// um SELECT comum contra o catálogo (`information_schema`/equivalente)
/// e reusa o MESMO pipeline (`validate` + `execute_read_only`) de
/// qualquer outra query, então fica sujeito às mesmas garantias de
/// somente-leitura do CLAUDE.md §2 sem abrir um caminho novo.
#[tauri::command]
pub async fn connection_schema(
    state: tauri::State<'_, AppState>,
    connection_slug: String,
) -> Result<Vec<TableInfo>, String> {
    let connection = connections::get_by_slug(&state.pool, &connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    let driver = state.driver_for(connection.kind)?;
    let dialect = connection.kind.as_dialect();

    let sql = schema_query_for(dialect);
    let validated = validate(sql, dialect).map_err(guard_error_to_string)?;
    let execution_id = uuid::Uuid::new_v4().to_string();

    let result =
        execute_read_only(&state, driver, connection, execution_id, &validated, &[]).await?;
    Ok(tables_from_result(&result))
}

fn schema_query_for(dialect: Dialect) -> &'static str {
    match dialect {
        // Só schema `public` por enquanto — cobre o caso comum sem
        // introduzir navegação multi-schema, que é um passo à parte.
        Dialect::Postgres => {
            "select table_name, column_name, data_type, ordinal_position \
             from information_schema.columns \
             where table_schema = 'public' \
             order by table_name, ordinal_position"
        }
        Dialect::MySql => {
            "select table_name, column_name, data_type, ordinal_position \
             from information_schema.columns \
             where table_schema = database() \
             order by table_name, ordinal_position"
        }
        Dialect::Oracle => {
            "select table_name, column_name, data_type, column_id as ordinal_position \
             from user_tab_columns \
             order by table_name, column_id"
        }
    }
}

fn cell_as_text(cell: Option<&CellValue>) -> Option<String> {
    match cell {
        Some(CellValue::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

// Depende das linhas já virem ordenadas por `table_name` (ver
// `schema_query_for`) — agrupa por adjacência em vez de um HashMap pra
// preservar a ordem alfabética que o próprio SQL já garantiu.
fn tables_from_result(result: &ResultSet) -> Vec<TableInfo> {
    let mut tables: Vec<TableInfo> = Vec::new();
    for row in &result.rows {
        let Some(table_name) = cell_as_text(row.first()) else {
            continue;
        };
        let Some(column_name) = cell_as_text(row.get(1)) else {
            continue;
        };
        let data_type = cell_as_text(row.get(2)).unwrap_or_default();
        let column = ColumnInfo {
            name: column_name,
            data_type,
        };

        match tables.last_mut() {
            Some(last) if last.name == table_name => last.columns.push(column),
            _ => tables.push(TableInfo {
                name: table_name,
                columns: vec![column],
            }),
        }
    }
    tables
}

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
pub async fn query_update(
    state: tauri::State<'_, AppState>,
    slug: String,
    input: NewQuery,
) -> Result<QuerySummary, String> {
    let connection = connections::get_by_slug(&state.pool, &input.connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    // Mesma defesa em camadas de query_create (§2) — o SQL editado passa
    // pelo guard aqui, e de novo em execute_select na hora de rodar.
    validate(&input.sql, connection.kind.as_dialect()).map_err(guard_error_to_string)?;
    queries::update(&state.pool, &slug, &input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn query_delete(state: tauri::State<'_, AppState>, slug: String) -> Result<(), String> {
    queries::delete(&state.pool, &slug)
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

    execute_read_only(
        &state,
        driver,
        connection,
        execution_id,
        &rewritten.validated,
        &bind_values,
    )
    .await
}

/// Executa SQL ad-hoc direto contra uma connection — nunca toca a tabela
/// `query` do SQLite local (CLAUDE.md: salvar é opcional, nunca
/// pré-requisito para rodar). Passa pelo **mesmo** `validate()` que
/// `query_run` usa para SQL salva — não existe caminho mais permissivo
/// para SQL não salva (§2: "na dúvida, bloqueie").
///
/// Sem `param_type` guardado (não há linha salva), todo parâmetro vira
/// `Bind::Text` — o que já é o comportamento efetivo de toda query salva
/// hoje, já que o front sempre grava `type: "string"` (ver `to_bind`).
#[tauri::command]
pub async fn query_run_adhoc(
    state: tauri::State<'_, AppState>,
    execution_id: String,
    connection_slug: String,
    sql: String,
    params: HashMap<String, String>,
) -> Result<ResultSet, String> {
    let connection = connections::get_by_slug(&state.pool, &connection_slug)
        .await
        .map_err(|e| e.to_string())?;
    let driver = state.driver_for(connection.kind)?;

    let dialect = connection.kind.as_dialect();
    let validated = validate(&sql, dialect).map_err(guard_error_to_string)?;

    let names = ordered_param_names(&validated);
    let mut bindings = Vec::with_capacity(names.len());
    let mut bind_values = Vec::with_capacity(names.len());
    for name in &names {
        let raw = params
            .get(name)
            .ok_or_else(|| format!("parâmetro obrigatório não informado: {name}"))?;
        bindings.push(ParamBinding {
            name: name.clone(),
            cardinality: Cardinality::Scalar,
        });
        bind_values.push(Bind::Text(raw.clone()));
    }

    let rewritten = rewrite_placeholders(&validated, &bindings, DEFAULT_MAX_LIST_SIZE)
        .map_err(|e| e.to_string())?;

    execute_read_only(
        &state,
        driver,
        connection,
        execution_id,
        &rewritten.validated,
        &bind_values,
    )
    .await
}

/// Sequência compartilhada por `query_run`/`query_run_adhoc`: conecta,
/// entra em modo somente-leitura, registra o `execution_id` para
/// cancelamento, executa, desregistra e sempre faz `rollback` (nunca
/// `commit` — CLAUDE.md §2.2).
async fn execute_read_only(
    state: &AppState,
    driver: Arc<dyn Driver>,
    connection: ConnectionSummary,
    execution_id: String,
    validated: &ValidatedSql,
    bind_values: &[Bind],
) -> Result<ResultSet, String> {
    let cfg = ConnectionConfig {
        dialect: connection.kind.as_dialect(),
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

    // Registrado ANTES de conectar, não depois de já ter voltado do
    // handshake + `BEGIN`/`SET TRANSACTION READ ONLY` — esse round-trip
    // sozinho pode levar dezenas a centenas de ms (sobretudo numa
    // conexão fria, já que cada execução abre uma nova). Um cancelamento
    // que chegasse nessa janela, com o registro só acontecendo depois de
    // `begin_read_only`, não encontrava nada em `running` (`query_cancel`
    // simplesmente não fazia nada) e a query rodava inteira mesmo com o
    // usuário já tendo clicado "Cancelar" — é exatamente o "botão
    // cancelar não responde" quando o clique vem logo depois de
    // "Executar".
    let cancel = CancellationToken::new();
    state
        .register_running(execution_id.clone(), cancel.clone())
        .await;

    let result = run_read_only(driver, &cfg, &secret, &limits, validated, bind_values, cancel).await;

    state.unregister_running(&execution_id).await;
    result.map_err(|e| e.to_string())
}

/// Conecta e entra em modo somente-leitura verificando `cancel` entre
/// cada etapa — sem isso, um cancelamento que chega durante o handshake
/// ou o `BEGIN` só seria observado depois, dentro de `execute_select`,
/// deixando a query rodar por mais tempo do que o necessário (ou nem
/// chegar a observar o cancelamento, se a sessão nunca chegasse lá).
async fn run_read_only(
    driver: Arc<dyn Driver>,
    cfg: &ConnectionConfig,
    secret: &SecretRef,
    limits: &Limits,
    validated: &ValidatedSql,
    bind_values: &[Bind],
    cancel: CancellationToken,
) -> Result<ResultSet, DbError> {
    let mut session = driver.connect(cfg, secret).await?;
    if cancel.is_cancelled() {
        let _ = session.rollback_and_close().await;
        return Err(DbError::Cancelled);
    }

    session.begin_read_only(limits).await?;
    if cancel.is_cancelled() {
        let _ = session.rollback_and_close().await;
        return Err(DbError::Cancelled);
    }

    let result = session
        .execute_select(validated, bind_values, limits, cancel)
        .await;
    let _ = session.rollback_and_close().await;
    result
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
