//! CRUD de queries salvas no SQLite local.

use chrono::Utc;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::model::{NewQuery, QueryParam, QuerySummary};

#[derive(Debug, Error)]
pub enum QueriesError {
    #[error("já existe uma query com o slug '{slug}'")]
    DuplicateSlug { slug: String },
    #[error("query '{slug}' não encontrada")]
    NotFound { slug: String },
    #[error("connection '{connection_slug}' não encontrada")]
    UnknownConnection { connection_slug: String },
    #[error("erro de banco local")]
    Db(#[from] sqlx::Error),
    #[error("params_json corrompido no banco local")]
    CorruptParams,
}

#[derive(sqlx::FromRow)]
struct QueryRow {
    id: String,
    slug: String,
    name: String,
    connection_slug: String,
    sql: String,
    params_json: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<QueryRow> for QuerySummary {
    type Error = QueriesError;

    fn try_from(row: QueryRow) -> Result<Self, Self::Error> {
        let params: Vec<QueryParam> =
            serde_json::from_str(&row.params_json).map_err(|_| QueriesError::CorruptParams)?;
        Ok(QuerySummary {
            id: row.id,
            slug: row.slug,
            name: row.name,
            connection_slug: row.connection_slug,
            sql: row.sql,
            params,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.is_unique_violation())
}

fn is_foreign_key_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.is_foreign_key_violation())
}

pub async fn create(pool: &SqlitePool, input: &NewQuery) -> Result<QuerySummary, QueriesError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let params = input.params.clone().unwrap_or_default();
    let params_json = serde_json::to_string(&params).expect("Vec<QueryParam> sempre serializa");

    sqlx::query(
        "INSERT INTO query (id, slug, name, connection_slug, sql, params_json, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.slug)
    .bind(&input.name)
    .bind(&input.connection_slug)
    .bind(&input.sql)
    .bind(&params_json)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            QueriesError::DuplicateSlug {
                slug: input.slug.clone(),
            }
        } else if is_foreign_key_violation(&e) {
            QueriesError::UnknownConnection {
                connection_slug: input.connection_slug.clone(),
            }
        } else {
            QueriesError::Db(e)
        }
    })?;

    Ok(QuerySummary {
        id,
        slug: input.slug.clone(),
        name: input.name.clone(),
        connection_slug: input.connection_slug.clone(),
        sql: input.sql.clone(),
        params,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<QuerySummary>, QueriesError> {
    let rows: Vec<QueryRow> = sqlx::query_as(
        "SELECT id, slug, name, connection_slug, sql, params_json, created_at, updated_at \
         FROM query ORDER BY slug",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn get_by_slug(pool: &SqlitePool, slug: &str) -> Result<QuerySummary, QueriesError> {
    let row: Option<QueryRow> = sqlx::query_as(
        "SELECT id, slug, name, connection_slug, sql, params_json, created_at, updated_at \
         FROM query WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    row.ok_or_else(|| QueriesError::NotFound {
        slug: slug.to_string(),
    })?
    .try_into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionKind, NewConnection};

    async fn test_pool() -> SqlitePool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("app.db");
        crate::store::open(&path).await.unwrap()
    }

    async fn seed_connection(pool: &SqlitePool, slug: &str) {
        crate::store::connections::create(
            pool,
            &NewConnection {
                slug: slug.to_string(),
                name: "ERP".to_string(),
                kind: ConnectionKind::Postgres,
                host: "db.internal".to_string(),
                port: 5432,
                database: Some("erp".to_string()),
                service_name: None,
                username: "app_readonly".to_string(),
                password: "irrelevante-aqui".to_string(),
                max_rows: None,
                timeout_ms: None,
            },
        )
        .await
        .unwrap();
    }

    fn sample_input(slug: &str, connection_slug: &str) -> NewQuery {
        NewQuery {
            slug: slug.to_string(),
            name: "Consulta oferta".to_string(),
            connection_slug: connection_slug.to_string(),
            sql: "SELECT * FROM tb_offer WHERE offer_id = :offer_id".to_string(),
            params: Some(vec![QueryParam {
                name: "offer_id".to_string(),
                param_type: "number".to_string(),
                required: true,
            }]),
        }
    }

    #[tokio::test]
    async fn create_then_list_roundtrips_params() {
        let pool = test_pool().await;
        seed_connection(&pool, "erp_prod").await;

        create(&pool, &sample_input("consulta_oferta", "erp_prod"))
            .await
            .unwrap();

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].params.len(), 1);
        assert_eq!(listed[0].params[0].name, "offer_id");
    }

    #[tokio::test]
    async fn rejects_query_pointing_at_unknown_connection() {
        let pool = test_pool().await;
        let err = create(&pool, &sample_input("consulta_oferta", "não_existe"))
            .await
            .unwrap_err();
        assert!(matches!(err, QueriesError::UnknownConnection { .. }));
    }

    #[tokio::test]
    async fn duplicate_slug_is_rejected() {
        let pool = test_pool().await;
        seed_connection(&pool, "erp_prod").await;
        create(&pool, &sample_input("consulta_oferta", "erp_prod"))
            .await
            .unwrap();
        let err = create(&pool, &sample_input("consulta_oferta", "erp_prod"))
            .await
            .unwrap_err();
        assert!(matches!(err, QueriesError::DuplicateSlug { .. }));
    }
}
