import type { QuerySummary } from "../../ipc/types";
import { Info } from "lucide-react";
import { formatCell } from "../../components/cellFormat";
import { useEffect, useId, useRef, useState } from "react";
import { RunBar } from "../queries/RunBar";
import { ResultGrid } from "../../components/ResultGrid";
import { ErrorPanel } from "../../components/ErrorPanel";
import type { QueryExecutionState } from "./types";

interface QueryPanelProps {
  query: QuerySummary;
  state: QueryExecutionState;
  onRun: () => void;
  onCancel: () => void;
  onMove?: (target: string) => void;
  onMoveStep?: (direction: number) => void;
}

export function QueryPanel({ query, state, onRun, onCancel, onMove, onMoveStep }: QueryPanelProps) {
  const [moving, setMoving] = useState(false);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [mode, setMode] = useState("auto");
  const [infoEditing, setInfoEditing] = useState(false);
  const [infoError, setInfoError] = useState<string | null>(null);
  const infoKey = `queryboard.query-description.${query.slug}`;
  const [description, setDescription] = useState(() => {
    try { return localStorage.getItem(infoKey) ?? ""; } catch { return ""; }
  });
  const [descriptionDraft, setDescriptionDraft] = useState(description);
  const infoId = useId();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const detailsId = useId();
  const panelRef = useRef<HTMLElement>(null);
  const drag = useRef<{x: number; y: number; width: number; height: number} | null>(null);
  const sizeKey = `queryboard.card-size.${query.slug}`;
  const [size, setSize] = useState<{width: number; height: number} | null>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(sizeKey) ?? "null") as {width: number; height: number} | null;
      return saved && Number.isFinite(saved.width) && Number.isFinite(saved.height)
        ? { width: Math.max(200, saved.width), height: Math.max(140, saved.height) } : null;
    } catch { return null; }
  });
  useEffect(() => {
    const timer = window.setTimeout(() => {
    try {
      if (size) localStorage.setItem(sizeKey, JSON.stringify(size));
      else localStorage.removeItem(sizeKey);
    } catch { /* Resizing still works when device storage is unavailable. */ }
    }, 250);
    return () => { window.clearTimeout(timer); };
  }, [size, sizeKey]);
  const isMetric = mode !== "table" && state.result?.columns.length === 1 && state.result.rows.length <= 50;
  function resize(width: number, height: number) {
    const available = panelRef.current?.parentElement?.clientWidth ?? width;
    setSize({ width: Math.min(Math.max(200, width), available), height: Math.max(140, Math.min(1200, height)) });
  }
  return (
    <section ref={panelRef} className="board-panel" data-query-slug={query.slug} data-moving={moving} data-metric={isMetric} aria-label={`Painel de ${query.name}`}
      style={isMetric && size ? { width: size.width, height: size.height, flexGrow: 0 } : undefined}>
      <div className="query-info">
        <button type="button" className="query-info__button" aria-label={`Informações de ${query.name}`}
          aria-describedby={`${infoId}-tooltip`} aria-expanded={infoEditing} aria-controls={`${infoId}-editor`}
          onClick={() => { setDescriptionDraft(description); setInfoError(null); setInfoEditing(open => !open); }}><Info size={16} strokeWidth={1.75} aria-hidden="true" /></button>
        {!infoEditing && <span role="tooltip" id={`${infoId}-tooltip`} className="query-info__tooltip">
          {description || "Clique para adicionar uma descrição desta consulta."}
        </span>}
      </div>
      {infoEditing && <form id={`${infoId}-editor`} className="query-info__editor" onSubmit={event => {
        event.preventDefault();
        try {
          const text = descriptionDraft.trim();
          localStorage.setItem(infoKey, text);
          setDescription(text); setInfoEditing(false); setInfoError(null);
        } catch { setInfoError("Não foi possível salvar a descrição neste dispositivo."); }
      }}>
        <label htmlFor={`${infoId}-text`}>Descrição da consulta</label>
        <textarea autoFocus id={`${infoId}-text`} value={descriptionDraft} maxLength={2000} rows={4}
          placeholder="Ex.: Ofertas em andamento sem envio para a loja."
          onChange={event => { setDescriptionDraft(event.target.value); }} />
        {infoError && <p role="alert">{infoError}</p>}
        <div><button type="submit">Salvar descrição</button><button type="button" onClick={() => { setInfoEditing(false); }}>Cancelar</button></div>
      </form>}
      {onMove && <button type="button" className="card-move-handle"
        aria-label={`Mover ${query.name}`} title="Arraste sobre outro card para reposicionar. Use as setas para mudar a ordem."
        onPointerDown={event => {
          if (event.button !== 0) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          setMoving(true); setDropTarget(null);
        }}
        onPointerMove={event => {
          if (!moving) return;
          const target = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>("[data-query-slug]");
          setDropTarget(target?.parentElement === panelRef.current?.parentElement && target?.dataset.querySlug !== query.slug ? target?.dataset.querySlug ?? null : null);
        }}
        onPointerUp={() => {
          if (moving && dropTarget) onMove(dropTarget);
          setMoving(false); setDropTarget(null);
        }}
        onPointerCancel={() => { setMoving(false); setDropTarget(null); }}
        onLostPointerCapture={() => { setMoving(false); setDropTarget(null); }}
        onKeyDown={event => {
          if (["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(event.key)) {
            event.preventDefault(); onMoveStep?.(["ArrowLeft", "ArrowUp"].includes(event.key) ? -1 : 1);
          }
        }}
      >⠿</button>}
      {moving && <span className="card-move-status" role="status">{dropTarget ? "Solte para reposicionar" : "Arraste sobre outro card"}</span>}
      {isMetric && state.result !== null && (
        <div className="query-metric" aria-label="Resultado em card">
          <button className="query-metric__field" type="button" aria-expanded={detailsOpen} aria-controls={detailsId} onClick={() => { setDetailsOpen(open => !open); }}>
            {state.result.columns[0]?.name}
          </button>
          {state.result.rows.length === 0 ? <p>Nenhuma linha retornada.</p> : state.result.rows.map((row, index) => <strong key={index}>{row[0] === undefined ? "—" : formatCell(row[0]).text}</strong>)}
        </div>
      )}
      <div id={detailsId} hidden={isMetric && !detailsOpen}>
      <h3>{query.name}</h3>
      <label>Exibição<select value={mode} onChange={event => { setMode(event.target.value); }}><option value="auto">Automática</option><option value="table">Tabela</option><option value="card">Card (uma coluna)</option></select></label>
      <p className="board-panel__meta">
        slug: {query.slug} · conexão: {query.connection_slug}
      </p>
      {isMetric && <pre className="query-metric__sql">{query.sql}</pre>}
      {isMetric && <button type="button" onClick={() => { setSize(null); }}>Restaurar tamanho</button>}
      <RunBar
        status={state.status}
        onRun={onRun}
        onCancel={onCancel}
        rowCount={state.result?.rows.length ?? null}
        elapsedMs={state.result?.elapsed_ms ?? null}
        truncated={state.result?.truncated ?? false}
      />
      </div>
      {state.error !== null && <ErrorPanel message={state.error} />}
      {mode === "card" && state.result !== null && state.result.columns.length !== 1 && <p>Selecione apenas uma coluna no SQL para exibir um card. O resultado completo aparece abaixo.</p>}
      {state.result !== null && !isMetric && <ResultGrid result={state.result} />}
      {isMetric && (["right", "bottom", "corner"] as const).map(edge => <button
        key={edge} data-edge={edge}
        type="button" className="card-resize-handle" aria-label={`Redimensionar ${query.name}: ${edge === "right" ? "largura" : edge === "bottom" ? "altura" : "largura e altura"}`}
        title="Arraste para redimensionar. Use as setas do teclado para ajustar."
        onPointerDown={event => {
          if (event.button !== 0) return;
          const rect = panelRef.current?.getBoundingClientRect();
          if (!rect) return;
          drag.current = { x: event.clientX, y: event.clientY, width: rect.width, height: rect.height };
          event.currentTarget.setPointerCapture(event.pointerId);
          event.preventDefault();
        }}
        onPointerMove={event => {
          const start = drag.current;
          if (start) resize(start.width + (edge === "bottom" ? 0 : event.clientX - start.x), start.height + (edge === "right" ? 0 : event.clientY - start.y));
        }}
        onPointerUp={() => { drag.current = null; }}
        onPointerCancel={() => { drag.current = null; }}
        onLostPointerCapture={() => { drag.current = null; }}
        onKeyDown={event => {
          if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
          event.preventDefault();
          const rect = panelRef.current?.getBoundingClientRect();
          if (rect) resize(rect.width + (edge === "bottom" ? 0 : event.key === "ArrowRight" ? 20 : event.key === "ArrowLeft" ? -20 : 0), rect.height + (edge === "right" ? 0 : event.key === "ArrowDown" ? 20 : event.key === "ArrowUp" ? -20 : 0));
        }}
      />)}
    </section>
  );
}
