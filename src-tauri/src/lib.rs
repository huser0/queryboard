pub mod db;
pub mod model;
pub mod secrets;
pub mod sql;
pub mod store;

#[cfg(feature = "tauri-app")]
pub mod ipc;

#[cfg(feature = "tauri-app")]
fn app_db_path() -> std::path::PathBuf {
    let mut dir = dirs::data_dir().expect("diretório de dados do SO deveria existir");
    dir.push("queryboard");
    std::fs::create_dir_all(&dir).expect("falha ao criar o diretório de dados do queryboard");
    dir.push("app.db");
    dir
}

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pool = tauri::async_runtime::block_on(async {
        store::open(&app_db_path())
            .await
            .expect("falha ao abrir o banco local")
    });
    let state = ipc::AppState::new(pool);

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            ipc::connections::connection_list,
            ipc::connections::connection_create,
            ipc::connections::connection_delete,
            ipc::connections::connection_test,
            ipc::query::query_list,
            ipc::query::query_create,
            ipc::query::query_update,
            ipc::query::query_delete,
            ipc::query::query_extract_params,
            ipc::query::query_run,
            ipc::query::query_run_adhoc,
            ipc::query::connection_schema,
            ipc::query_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn cargo_test_pipeline_runs() {
        assert_eq!(env!("CARGO_PKG_NAME"), "queryboard");
    }
}
