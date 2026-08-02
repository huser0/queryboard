import type { RunStatus } from "../queries/RunBar";
import type { ResultSet } from "../../ipc/client";
import type { QuerySummary } from "../../ipc/types";

export interface QueryExecutionState {
  status: RunStatus;
  executionId: string | null;
  result: ResultSet | null;
  error: string | null;
}

export const IDLE_STATE: QueryExecutionState = {
  status: "idle",
  executionId: null,
  result: null,
  error: null,
};

/** SQL digitado direto no Painel, sem passar pela tabela `query` do
 * SQLite local — "Salvar" é opcional, nunca pré-requisito para rodar. */
export interface AdhocSlot {
  id: string;
  connectionSlug: string | null;
  sql: string;
  paramNames: string[];
  savedAsSlug: string | null;
}

export function newAdhocSlot(id: string, connectionSlug: string | null = null): AdhocSlot {
  return { id, connectionSlug, sql: "", paramNames: [], savedAsSlug: null };
}

/** "Editar" uma query salva abre uma cópia editável dela como bloco
 * ad-hoc, com `savedAsSlug` já preenchido — é isso que faz o "Salvar" do
 * bloco virar um `queryUpdate` na query original em vez de criar uma
 * nova (ver `AdhocQueryBlock::handleSave`). */
export function adhocSlotFromQuery(id: string, query: QuerySummary): AdhocSlot {
  return {
    id,
    connectionSlug: query.connection_slug,
    sql: query.sql,
    paramNames: query.params.map((p) => p.name),
    savedAsSlug: query.slug,
  };
}
