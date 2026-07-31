pub mod db;
pub mod sql;

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
