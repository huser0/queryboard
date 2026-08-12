import { useState } from "react";
import type { QuerySummary } from "../../ipc/types";
import { ConfirmDialog } from "../../components/ConfirmDialog";

interface QuerySelectorProps {
  queries: QuerySummary[];
  selectedSlugs: string[];
  runningSlugs: string[];
  onChange: (slugs: string[]) => void;
  onDelete: (slug: string) => Promise<void>;
  onEdit: (query: QuerySummary) => void;
}

export function QuerySelector({
  queries,
  selectedSlugs,
  runningSlugs,
  onChange,
  onDelete,
  onEdit,
}: QuerySelectorProps) {
  const [deleteErrors, setDeleteErrors] = useState<Record<string, string>>({});
  const [confirmingSlug, setConfirmingSlug] = useState<string | null>(null);

  function toggle(slug: string) {
    if (selectedSlugs.includes(slug)) {
      onChange(selectedSlugs.filter((s) => s !== slug));
    } else {
      onChange([...selectedSlugs, slug]);
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

  if (queries.length === 0) {
    return (
      <p className="empty-state">
        Nenhuma query salva ainda — salve uma a partir de um bloco SQL ad-hoc.
      </p>
    );
  }

  const confirming = queries.find((q) => q.slug === confirmingSlug) ?? null;

  return (
    <div className="query-selector" role="group" aria-label="Queries do painel">
      <h3 className="section-label">Queries do painel</h3>
      {queries.map((query) => (
        <div key={query.slug}>
          <label>
            <input
              type="checkbox"
              checked={selectedSlugs.includes(query.slug)}
              disabled={runningSlugs.includes(query.slug)}
              onChange={() => { toggle(query.slug); }}
            />
            {query.name} ({query.connection_slug})
          </label>
          <button
            type="button"
            onClick={() => { onEdit(query); }}
            disabled={runningSlugs.includes(query.slug)}
          >
            Editar
          </button>
          <button
            type="button"
            className="action-subtle"
            onClick={() => { setConfirmingSlug(query.slug); }}
            disabled={runningSlugs.includes(query.slug)}
          >
            Remover
          </button>
          {deleteErrors[query.slug] !== undefined && (
            <span role="alert">{deleteErrors[query.slug]}</span>
          )}
        </div>
      ))}
      <ConfirmDialog
        open={confirming !== null}
        title={confirming !== null ? `Remover ${confirming.name}?` : ""}
        description={
          confirming !== null
            ? `A query "${confirming.slug}" será removida do painel. Essa ação não pode ser desfeita.`
            : ""
        }
        onConfirm={() => {
          if (confirming !== null) {
            void handleDelete(confirming.slug);
          }
        }}
        onCancel={() => { setConfirmingSlug(null); }}
      />
    </div>
  );
}
