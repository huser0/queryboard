//! Comandos de connection. Nenhum devolve senha, em nenhuma forma —
//! CLAUDE.md §7.

use crate::db::driver::{ConnectionConfig, Limits, SecretRef};
use crate::ipc::AppState;
use crate::model::{ConnectionSummary, ConnectionUpdate, NewConnection};
use crate::secrets;
use crate::store::connections;

#[tauri::command]
pub async fn connection_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConnectionSummary>, String> {
    connections::list(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn connection_create(
    state: tauri::State<'_, AppState>,
    input: NewConnection,
) -> Result<ConnectionSummary, String> {
    let created = connections::create(&state.pool, &input)
        .await
        .map_err(|e| e.to_string())?;
    // Senha só é gravada depois que a linha no SQLite existe — se o
    // keyring falhar, a connection fica inconsistente sem senha, o que é
    // preferível a uma senha órfã sem connection dona.
    secrets::store(&created.id, &input.password).map_err(|e| e.to_string())?;
    Ok(created)
}

/// `input.password` só sobrescreve o keyring quando vem preenchido — uma
/// edição que não mexe na senha não deve exigir que o usuário a redigite
/// (e ela nunca poderia vir pré-preenchida na UI, já que a senha nunca
/// atravessa o IPC de volta — CLAUDE.md §7).
#[tauri::command]
pub async fn connection_update(
    state: tauri::State<'_, AppState>,
    slug: String,
    input: ConnectionUpdate,
) -> Result<ConnectionSummary, String> {
    let password = input.password.clone();
    let updated = connections::update(&state.pool, &slug, &input)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        secrets::store(&updated.id, &password).map_err(|e| e.to_string())?;
    }
    Ok(updated)
}

#[tauri::command]
pub async fn connection_delete(
    state: tauri::State<'_, AppState>,
    slug: String,
) -> Result<(), String> {
    let id = connections::delete(&state.pool, &slug)
        .await
        .map_err(|e| e.to_string())?;
    secrets::delete(&id).map_err(|e| e.to_string())
}

/// Conecta, entra em modo somente-leitura e desfaz — nunca deixa a
/// sessão aberta. É o que a UI chama para o botão "testar conexão".
#[tauri::command]
pub async fn connection_test(
    state: tauri::State<'_, AppState>,
    slug: String,
) -> Result<(), String> {
    let summary = connections::get_by_slug(&state.pool, &slug)
        .await
        .map_err(|e| e.to_string())?;
    let driver = state.driver_for(summary.kind)?;

    let cfg = ConnectionConfig {
        dialect: summary.kind.as_dialect(),
        host: summary.host,
        port: summary.port,
        database: summary.database,
        service_name: summary.service_name,
        username: summary.username,
    };
    let secret = SecretRef::new(summary.id);

    let mut session = driver
        .connect(&cfg, &secret)
        .await
        .map_err(|e| e.to_string())?;
    session
        .begin_read_only(&Limits {
            timeout: std::time::Duration::from_secs(10),
            ..Limits::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    session
        .rollback_and_close()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
