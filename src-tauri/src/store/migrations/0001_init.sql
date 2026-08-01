-- Schema local do queryboard. Nenhuma senha é armazenada aqui — ver
-- secrets.rs (keyring do SO). CLAUDE.md §7.

CREATE TABLE connection (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('oracle', 'postgres', 'mysql')),
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    database TEXT,
    service_name TEXT,
    username TEXT NOT NULL,
    options_json TEXT NOT NULL DEFAULT '{}',
    max_rows INTEGER NOT NULL DEFAULT 1000,
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE query (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    connection_slug TEXT NOT NULL REFERENCES connection (slug) ON UPDATE CASCADE,
    sql TEXT NOT NULL,
    params_json TEXT NOT NULL DEFAULT '[]',
    columns_cache_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX query_by_connection ON query (connection_slug);

CREATE TABLE app_setting (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
