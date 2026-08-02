import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionSummary,
  NewConnection,
  NewQuery,
  QuerySummary,
} from "./types";

// O macro `#[tauri::command]` espera os argumentos em camelCase por
// padrão (nenhum comando usa `rename_all = "snake_case"`), então os
// nomes de parâmetro aqui não batem 1:1 com os nomes em snake_case do
// lado Rust — é assim mesmo, não um erro de digitação.

export interface ResultSetColumn {
  name: string;
  name_lower: string;
  declared_type: string;
  nullable: boolean | null;
}

export type CellValue =
  | { type: "Null" }
  | { type: "Bool"; value: boolean }
  | { type: "Int"; value: number }
  | { type: "Decimal"; value: string }
  | { type: "Float"; value: number }
  | { type: "Text"; value: string }
  | { type: "Bytes"; value: number[] }
  | { type: "Date"; value: string }
  | { type: "Time"; value: string }
  | { type: "Timestamp"; value: string }
  | { type: "TimestampTz"; value: string }
  | { type: "Json"; value: string }
  | { type: "Interval"; value: string };

export interface ResultSet {
  columns: ResultSetColumn[];
  rows: CellValue[][];
  truncated: boolean;
  elapsed_ms: number;
}

export interface ColumnInfo {
  name: string;
  data_type: string;
}

export interface TableInfo {
  name: string;
  columns: ColumnInfo[];
}

export const client = {
  connectionList: (): Promise<ConnectionSummary[]> =>
    invoke("connection_list"),

  connectionCreate: (input: NewConnection): Promise<ConnectionSummary> =>
    invoke("connection_create", { input }),

  connectionDelete: (slug: string): Promise<void> =>
    invoke("connection_delete", { slug }),

  connectionTest: (slug: string): Promise<void> =>
    invoke("connection_test", { slug }),

  queryList: (): Promise<QuerySummary[]> => invoke("query_list"),

  queryCreate: (input: NewQuery): Promise<QuerySummary> =>
    invoke("query_create", { input }),

  queryUpdate: (slug: string, input: NewQuery): Promise<QuerySummary> =>
    invoke("query_update", { slug, input }),

  queryDelete: (slug: string): Promise<void> => invoke("query_delete", { slug }),

  queryExtractParams: (sql: string, connectionSlug: string): Promise<string[]> =>
    invoke("query_extract_params", { sql, connectionSlug }),

  queryRun: (
    executionId: string,
    slug: string,
    params: Record<string, string>,
  ): Promise<ResultSet> =>
    invoke("query_run", { executionId, slug, params }),

  queryRunAdhoc: (
    executionId: string,
    connectionSlug: string,
    sql: string,
    params: Record<string, string>,
  ): Promise<ResultSet> =>
    invoke("query_run_adhoc", { executionId, connectionSlug, sql, params }),

  queryCancel: (executionId: string): Promise<void> =>
    invoke("query_cancel", { executionId }),

  connectionSchema: (connectionSlug: string): Promise<TableInfo[]> =>
    invoke("connection_schema", { connectionSlug }),
};

export type QueryboardClient = typeof client;
