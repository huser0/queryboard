import { useState } from "react";
import type { ConnectionSummary } from "../../ipc/types";

interface ConnectionListProps {
  connections: ConnectionSummary[];
  selectedSlug: string | null;
  onSelect: (slug: string) => void;
  onTest: (slug: string) => Promise<void>;
  onDelete: (slug: string) => Promise<void>;
}

type TestState = { status: "idle" } | { status: "ok" } | { status: "error"; message: string };

export function ConnectionList({
  connections,
  selectedSlug,
  onSelect,
  onTest,
  onDelete,
}: ConnectionListProps) {
  const [testStates, setTestStates] = useState<Record<string, TestState>>({});
  const [deleteErrors, setDeleteErrors] = useState<Record<string, string>>({});

  async function handleTest(slug: string) {
    setTestStates((prev) => ({ ...prev, [slug]: { status: "idle" } }));
    try {
      await onTest(slug);
      setTestStates((prev) => ({ ...prev, [slug]: { status: "ok" } }));
    } catch (err) {
      setTestStates((prev) => ({
        ...prev,
        [slug]: { status: "error", message: String(err) },
      }));
    }
  }

  async function handleDelete(slug: string) {
    try {
      await onDelete(slug);
      setDeleteErrors((prev) =>
        Object.fromEntries(Object.entries(prev).filter(([s]) => s !== slug)),
      );
    } catch (err) {
      setDeleteErrors((prev) => ({ ...prev, [slug]: String(err) }));
    }
  }

  if (connections.length === 0) {
    return <p className="empty-state">Nenhuma connection cadastrada ainda.</p>;
  }

  return (
    <ul aria-label="Connections cadastradas">
      {connections.map((conn) => {
        const testState = testStates[conn.slug] ?? { status: "idle" as const };
        return (
          <li key={conn.id}>
            <button
              type="button"
              aria-current={conn.slug === selectedSlug}
              onClick={() => { onSelect(conn.slug); }}
            >
              {conn.name} ({conn.kind}) — {conn.slug}
            </button>
            <button type="button" onClick={() => void handleTest(conn.slug)}>
              Testar
            </button>
            <button type="button" data-variant="danger" onClick={() => void handleDelete(conn.slug)}>
              Remover
            </button>
            {testState.status === "ok" && <span role="status">conexão ok</span>}
            {testState.status === "error" && (
              <span role="alert">falha: {testState.message}</span>
            )}
            {deleteErrors[conn.slug] !== undefined && (
              <span role="alert">{deleteErrors[conn.slug]}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
