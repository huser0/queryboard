import { Fragment, useState } from "react";
import type { ConnectionSummary } from "../../ipc/types";
import { ConfirmDialog } from "../../components/ConfirmDialog";

interface ConnectionListProps {
  connections: ConnectionSummary[];
  selectedSlug: string | null;
  onSelect: (slug: string) => void;
  onTest: (slug: string) => Promise<void>;
  onDelete: (slug: string) => Promise<void>;
  onEdit: (connection: ConnectionSummary) => void;
}

type TestState = { status: "idle" } | { status: "ok" } | { status: "error"; message: string };

export function ConnectionList({
  connections,
  selectedSlug,
  onSelect,
  onTest,
  onDelete,
  onEdit,
}: ConnectionListProps) {
  const [testStates, setTestStates] = useState<Record<string, TestState>>({});
  const [deleteErrors, setDeleteErrors] = useState<Record<string, string>>({});
  const [confirmingSlug, setConfirmingSlug] = useState<string | null>(null);

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
    setConfirmingSlug(null);
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
    return <p className="empty-state">Nenhuma conexão cadastrada ainda.</p>;
  }

  const confirming = connections.find((c) => c.slug === confirmingSlug) ?? null;

  return (
    <Fragment>
      <ul aria-label="Conexões cadastradas">
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
              <button type="button" onClick={() => { onEdit(conn); }}>
                Editar
              </button>
              <button
                type="button"
                className="action-subtle"
                onClick={() => { setConfirmingSlug(conn.slug); }}
              >
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
      <ConfirmDialog
        open={confirming !== null}
        title={confirming !== null ? `Remover ${confirming.name}?` : ""}
        description={
          confirming !== null
            ? `A connection "${confirming.slug}" será removida. Essa ação não pode ser desfeita.`
            : ""
        }
        onConfirm={() => {
          if (confirming !== null) {
            void handleDelete(confirming.slug);
          }
        }}
        onCancel={() => { setConfirmingSlug(null); }}
      />
    </Fragment>
  );
}
