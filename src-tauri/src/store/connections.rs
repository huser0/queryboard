//! CRUD de connections no SQLite local. Nunca lê nem escreve senha —
//! não há coluna para isso na tabela `connection` (ver `secrets.rs`).

use chrono::Utc;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::model::{ConnectionKind, ConnectionSummary, NewConnection};

#[derive(Debug, Error)]
pub enum ConnectionsError {
    #[error("já existe uma connection com o slug '{slug}'")]
    DuplicateSlug { slug: String },
    #[error("connection '{slug}' não encontrada")]
    NotFound { slug: String },
    #[error("erro de banco local")]
    Db(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct ConnectionRow {
    id: String,
    slug: String,
    name: String,
    kind: String,
    host: String,
    port: i64,
    database: Option<String>,
    service_name: Option<String>,
    username: String,
    max_rows: i64,
    timeout_ms: i64,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ConnectionRow> for ConnectionSummary {
    type Error = ConnectionsError;

    fn try_from(row: ConnectionRow) -> Result<Self, Self::Error> {
        let kind = match row.kind.as_str() {
            "oracle" => ConnectionKind::Oracle,
            "postgres" => ConnectionKind::Postgres,
            "mysql" => ConnectionKind::Mysql,
            other => {
                return Err(ConnectionsError::Db(sqlx::Error::Decode(
                    format!("kind de connection desconhecido no banco local: {other}").into(),
                )))
            }
        };
        Ok(ConnectionSummary {
            id: row.id,
            slug: row.slug,
            name: row.name,
            kind,
            host: row.host,
            port: row.port as u16,
            database: row.database,
            service_name: row.service_name,
            username: row.username,
            max_rows: row.max_rows as i32,
            timeout_ms: row.timeout_ms as i32,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn kind_str(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Oracle => "oracle",
        ConnectionKind::Postgres => "postgres",
        ConnectionKind::Mysql => "mysql",
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.is_unique_violation())
}

pub async fn create(
    pool: &SqlitePool,
    input: &NewConnection,
) -> Result<ConnectionSummary, ConnectionsError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let max_rows = input.max_rows.unwrap_or(1000);
    let timeout_ms = input.timeout_ms.unwrap_or(30_000);

    sqlx::query(
        "INSERT INTO connection \
         (id, slug, name, kind, host, port, database, service_name, username, \
          options_json, max_rows, timeout_ms, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, '{}', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.slug)
    .bind(&input.name)
    .bind(kind_str(input.kind))
    .bind(&input.host)
    .bind(input.port as i64)
    .bind(&input.database)
    .bind(&input.service_name)
    .bind(&input.username)
    .bind(max_rows)
    .bind(timeout_ms)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            ConnectionsError::DuplicateSlug {
                slug: input.slug.clone(),
            }
        } else {
            ConnectionsError::Db(e)
        }
    })?;

    Ok(ConnectionSummary {
        id,
        slug: input.slug.clone(),
        name: input.name.clone(),
        kind: input.kind,
        host: input.host.clone(),
        port: input.port,
        database: input.database.clone(),
        service_name: input.service_name.clone(),
        username: input.username.clone(),
        max_rows,
        timeout_ms,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ConnectionSummary>, ConnectionsError> {
    let rows: Vec<ConnectionRow> = sqlx::query_as(
        "SELECT id, slug, name, kind, host, port, database, service_name, username, \
                max_rows, timeout_ms, created_at, updated_at \
         FROM connection ORDER BY slug",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn get_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> Result<ConnectionSummary, ConnectionsError> {
    let row: Option<ConnectionRow> = sqlx::query_as(
        "SELECT id, slug, name, kind, host, port, database, service_name, username, \
                max_rows, timeout_ms, created_at, updated_at \
         FROM connection WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| ConnectionsError::NotFound {
        slug: slug.to_string(),
    })?
    .try_into()
}

/// Devolve o `id` interno (chave do keyring) da connection removida, para
/// que o chamador (IPC) possa apagar o segredo correspondente — este
/// módulo não sabe nada sobre keyring, de propósito.
pub async fn delete(pool: &SqlitePool, slug: &str) -> Result<String, ConnectionsError> {
    let existing = get_by_slug(pool, slug).await?;
    sqlx::query("DELETE FROM connection WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(existing.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let dir = tempfile::tempdir().unwrap();
        // `tempdir` é dropado no fim da função que a criou; para manter o
        // arquivo vivo pelo tempo do teste, usamos `into_path` (o SO limpa
        // /tmp de qualquer forma).
        let path = dir.keep().join("app.db");
        crate::store::open(&path).await.unwrap()
    }

    fn sample_input(slug: &str) -> NewConnection {
        NewConnection {
            slug: slug.to_string(),
            name: "ERP produção".to_string(),
            kind: ConnectionKind::Postgres,
            host: "db.internal".to_string(),
            port: 5432,
            database: Some("erp".to_string()),
            service_name: None,
            username: "app_readonly".to_string(),
            password: "não-deveria-ir-para-lugar-nenhum".to_string(),
            max_rows: None,
            timeout_ms: None,
        }
    }

    #[tokio::test]
    async fn create_then_list_roundtrips() {
        let pool = test_pool().await;
        let created = create(&pool, &sample_input("erp_prod")).await.unwrap();
        assert_eq!(created.slug, "erp_prod");
        assert_eq!(created.max_rows, 1000);

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug, "erp_prod");
    }

    #[tokio::test]
    async fn duplicate_slug_is_rejected() {
        let pool = test_pool().await;
        create(&pool, &sample_input("erp_prod")).await.unwrap();
        let err = create(&pool, &sample_input("erp_prod")).await.unwrap_err();
        assert!(matches!(err, ConnectionsError::DuplicateSlug { .. }));
    }

    #[tokio::test]
    async fn get_by_slug_not_found() {
        let pool = test_pool().await;
        let err = get_by_slug(&pool, "não-existe").await.unwrap_err();
        assert!(matches!(err, ConnectionsError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_removes_the_row_and_returns_its_id() {
        let pool = test_pool().await;
        let created = create(&pool, &sample_input("erp_prod")).await.unwrap();
        let deleted_id = delete(&pool, "erp_prod").await.unwrap();
        assert_eq!(deleted_id, created.id);
        assert!(list(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn stored_database_file_never_contains_the_password_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        let pool = crate::store::open(&db_path).await.unwrap();
        create(&pool, &sample_input("erp_prod")).await.unwrap();
        pool.close().await;

        let bytes = std::fs::read(&db_path).unwrap();
        let password = "não-deveria-ir-para-lugar-nenhum".as_bytes();
        let contains = bytes.windows(password.len()).any(|w| w == password);
        assert!(!contains, "a senha não deveria estar no app.db");
    }
}
