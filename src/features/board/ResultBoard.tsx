import type { QuerySummary } from "../../ipc/types";
import { QueryPanel } from "./QueryPanel";
import type { QueryExecutionState } from "./types";

interface ResultBoardProps {
  queries: QuerySummary[];
  executions: Record<string, QueryExecutionState>;
  onRunOne: (slug: string) => void;
  onCancelOne: (slug: string) => void;
}

export function ResultBoard({ queries, executions, onRunOne, onCancelOne }: ResultBoardProps) {
  if (queries.length === 0) {
    return null;
  }

  return (
    <div className="board-grid">
      {queries.map((query) => (
        <QueryPanel
          key={query.slug}
          query={query}
          state={executions[query.slug] ?? { status: "idle", executionId: null, result: null, error: null }}
          onRun={() => { onRunOne(query.slug); }}
          onCancel={() => { onCancelOne(query.slug); }}
        />
      ))}
    </div>
  );
}
