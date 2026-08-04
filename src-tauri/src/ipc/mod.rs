//! Comandos Tauri expostos ao frontend. Único módulo que conhece
//! `tauri::` — CLAUDE.md §4. Todo comando devolve `Result<T, String>`
//! com mensagem já segura para mostrar ao usuário (erros de banco já
//! passaram pelo sanitizador de `db::error`).

pub mod connections;
pub mod query;

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::db::driver::{Driver, SecretRef};
use crate::db::error::DbError;
use crate::db::mysql::MySqlDriver;
use crate::db::oracle::OracleDriver;
use crate::db::postgres::PostgresDriver;
use crate::model::ConnectionKind;
use crate::secrets;

pub struct AppState {
    pub pool: SqlitePool,
    drivers: HashMap<ConnectionKind, Arc<dyn Driver>>,
    /// Execuções em andamento, por `execution_id` gerado pelo front — é o
    /// que permite `query_cancel` encontrar o `CancellationToken` certo.
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let mut drivers: HashMap<ConnectionKind, Arc<dyn Driver>> = HashMap::new();
        drivers.insert(
            ConnectionKind::Postgres,
            Arc::new(PostgresDriver::new(resolve_secret)),
        );
        drivers.insert(
            ConnectionKind::Mysql,
            Arc::new(MySqlDriver::new(resolve_secret)),
        );
        drivers.insert(
            ConnectionKind::Oracle,
            Arc::new(OracleDriver::new(resolve_secret)),
        );
        Self {
            pool,
            drivers,
            running: Mutex::new(HashMap::new()),
        }
    }

    fn driver_for(&self, kind: ConnectionKind) -> Result<Arc<dyn Driver>, String> {
        self.drivers
            .get(&kind)
            .cloned()
            .ok_or_else(|| format!("driver para {kind:?} ainda não implementado"))
    }

    async fn register_running(&self, execution_id: String, token: CancellationToken) {
        self.running.lock().await.insert(execution_id, token);
    }

    async fn unregister_running(&self, execution_id: &str) {
        self.running.lock().await.remove(execution_id);
    }
}

fn resolve_secret(secret: &SecretRef) -> Result<String, DbError> {
    secrets::resolve(secret).map_err(|e| DbError::driver(e.to_string()))
}

#[tauri::command]
pub async fn query_cancel(
    state: tauri::State<'_, AppState>,
    execution_id: String,
) -> Result<(), String> {
    if let Some(token) = state.running.lock().await.get(&execution_id) {
        token.cancel();
    }
    Ok(())
}
