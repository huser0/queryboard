import type { QuerySummary } from "../../ipc/types";
import { RunBar } from "../queries/RunBar";
import { ResultGrid } from "../../components/ResultGrid";
import { ErrorPanel } from "../../components/ErrorPanel";
import type { QueryExecutionState } from "./types";

interface QueryPanelProps {
  query: QuerySummary;
  state: QueryExecutionState;
  onRun: () => void;
  onCancel: () => void;
}

export function QueryPanel({ query, state, onRun, onCancel }: QueryPanelProps) {
  return (
    <section className="board-panel" aria-label={`Painel de ${query.name}`}>
      <h3>{query.name}</h3>
      <p className="board-panel__meta">
        slug: {query.slug} · connection: {query.connection_slug}
      </p>
      <RunBar
        status={state.status}
        onRun={onRun}
        onCancel={onCancel}
        rowCount={state.result?.rows.length ?? null}
        elapsedMs={state.result?.elapsed_ms ?? null}
        truncated={state.result?.truncated ?? false}
      />
      {state.error !== null && <ErrorPanel message={state.error} />}
      {state.result !== null && <ResultGrid result={state.result} />}
    </section>
  );
}
