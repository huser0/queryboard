import { useState } from "react";
import type { AdhocSlot, QueryExecutionState } from "./types";

interface AdhocTabsProps {
  slots: AdhocSlot[];
  activeId: string | null;
  executions: Record<string, QueryExecutionState>;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onReorder: (draggedId: string, targetId: string) => void;
  onAddNew: () => void;
}

function titleFor(slot: AdhocSlot): string {
  const firstLine = slot.sql.trim().split("\n")[0]?.slice(0, 28);
  return firstLine !== undefined && firstLine.length > 0 ? firstLine : "SQL ad-hoc";
}

/** Uma aba por consulta ad-hoc aberta — antes cada bloco ficava
 * permanentemente empilhado na tela; com várias abertas ao mesmo tempo
 * virava uma rolagem longa sem nenhum jeito rápido de fechar uma
 * específica ou mudar a ordem. Fechar/reordenar mora na aba; a execução
 * em si continua rodando em segundo plano mesmo pra uma aba que não está
 * em foco (o estado vive em `executions`, no `Board`, não aqui) — trocar
 * de aba não cancela nem esconde o progresso de outra. */
export function AdhocTabs({
  slots,
  activeId,
  executions,
  onSelect,
  onClose,
  onReorder,
  onAddNew,
}: AdhocTabsProps) {
  const [draggedId, setDraggedId] = useState<string | null>(null);

  return (
    <div className="adhoc-tabs" role="tablist" aria-label="Consultas ad-hoc abertas">
      {slots.map((slot) => {
        const status = executions[slot.id]?.status ?? "idle";
        const title = titleFor(slot);
        return (
          <div
            key={slot.id}
            className="adhoc-tabs__tab"
            data-dragging={slot.id === draggedId}
            draggable
            onDragStart={() => { setDraggedId(slot.id); }}
            onDragOver={(e) => { e.preventDefault(); }}
            onDrop={() => {
              if (draggedId !== null && draggedId !== slot.id) {
                onReorder(draggedId, slot.id);
              }
              setDraggedId(null);
            }}
            onDragEnd={() => { setDraggedId(null); }}
          >
            <button
              type="button"
              role="tab"
              aria-selected={slot.id === activeId}
              className="adhoc-tabs__select"
              onClick={() => { onSelect(slot.id); }}
            >
              <span className="status-dot" data-status={status} aria-hidden="true" />
              {title}
              {slot.savedAsSlug !== null && <span className="adhoc-tabs__saved-badge">salvo</span>}
            </button>
            <button
              type="button"
              className="adhoc-tabs__close"
              aria-label={`Fechar ${title}`}
              onClick={() => { onClose(slot.id); }}
            >
              ×
            </button>
          </div>
        );
      })}
      <button
        type="button"
        className="adhoc-tabs__add"
        title="Adicionar SQL ad-hoc"
        aria-label="Adicionar SQL ad-hoc"
        onClick={onAddNew}
      >
        +
      </button>
    </div>
  );
}
