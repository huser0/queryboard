//! SQLite local — `$APPDATA/queryboard/app.db`. Guarda connections
//! (sem senha, ver `secrets.rs`), queries, rules e flows. CLAUDE.md §4.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

pub mod connections;
pub mod queries;

pub async fn open(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::migrate!("src/store/migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_on_a_fresh_database() {
        let dir = tempfile::tempdir().unwrap();
        let pool = open(&dir.path().join("app.db")).await.unwrap();
        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        let names: Vec<String> = tables.into_iter().map(|(n,)| n).collect();
        assert!(names.contains(&"connection".to_string()));
        assert!(names.contains(&"query".to_string()));
        assert!(names.contains(&"app_setting".to_string()));
    }

    #[tokio::test]
    async fn migrations_are_idempotent_on_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        open(&db_path).await.unwrap();
        // Reabrir o mesmo arquivo não deve falhar nem duplicar nada.
        let pool = open(&db_path).await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT count(*) FROM connection")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }
}
