//! Modelos serde canônicos — a mesma struct serializa para JSON (colunas
//! do SQLite local, `store::`) e é o que futuramente vira YAML no export
//! (roadmap item 14). Espelhados em TypeScript via `ts-rs`
//! (`#[ts(export)]`) para `src/ipc/generated/` — CLAUDE.md §8: "todo
//! comando Tauri tem tipo espelhado no front... divergência é bug".
//!
//! **Nenhuma struct aqui tem campo de senha.** Credenciais vivem só no
//! keyring do SO (`secrets.rs`) — CLAUDE.md §7.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    Oracle,
    Postgres,
    Mysql,
}

impl ConnectionKind {
    pub fn as_dialect(self) -> crate::sql::Dialect {
        match self {
            ConnectionKind::Oracle => crate::sql::Dialect::Oracle,
            ConnectionKind::Postgres => crate::sql::Dialect::Postgres,
            ConnectionKind::Mysql => crate::sql::Dialect::MySql,
        }
    }
}

/// O que o front recebe ao listar/ler uma connection. Sem senha — nunca
/// existiu um campo de senha aqui para "esquecer" de tirar.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConnectionSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub kind: ConnectionKind,
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub service_name: Option<String>,
    pub username: String,
    // i32, não i64: o IPC do Tauri serializa como JSON (sem bigint de
    // verdade), e ts-rs mapeia i64 para `bigint` no TS — um tipo que
    // mentiria sobre o valor real recebido em runtime. i32 vira `number`,
    // o que bate com o que realmente chega. Nenhum dos dois campos chega
    // perto do limite de i32 na prática.
    pub max_rows: i32,
    pub timeout_ms: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// O que o front envia para criar uma connection. `password` é usado uma
/// única vez para gravar no keyring (`secrets::store`) e nunca chega a
/// ser persistido em nenhuma struct que sobrevive à chamada IPC.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewConnection {
    pub slug: String,
    pub name: String,
    pub kind: ConnectionKind,
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
    pub service_name: Option<String>,
    pub username: String,
    pub password: String,
    #[ts(optional)]
    pub max_rows: Option<i32>,
    #[ts(optional)]
    pub timeout_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuerySummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub connection_slug: String,
    pub sql: String,
    pub params: Vec<QueryParam>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewQuery {
    pub slug: String,
    pub name: String,
    pub connection_slug: String,
    pub sql: String,
    #[ts(optional)]
    pub params: Option<Vec<QueryParam>>,
}
