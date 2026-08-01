// Reexporta os tipos gerados por ts-rs a partir das structs Rust em
// src-tauri/src/model/mod.rs (`cargo test export_bindings`). Este arquivo
// em si não é gerado — só a pasta `generated/` é. Mantenha esta lista em
// sincronia manualmente quando um novo `#[ts(export)]` for adicionado; o
// job `ipc-types` do CI falha se `generated/` divergir do que está
// commitado, o que pega esquecimentos aqui também (o import quebraria).

export type { ConnectionKind } from "./generated/ConnectionKind";
export type { ConnectionSummary } from "./generated/ConnectionSummary";
export type { NewConnection } from "./generated/NewConnection";
export type { QueryParam } from "./generated/QueryParam";
export type { QuerySummary } from "./generated/QuerySummary";
export type { NewQuery } from "./generated/NewQuery";
