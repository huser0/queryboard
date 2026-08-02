// Painel: roda N queries em paralelo, sem dependência entre elas (nenhuma
// usa resultado da outra) — tanto queries já salvas quanto SQL ad-hoc
// digitado na hora (salvar é opcional, nunca pré-requisito para rodar). É
// o subconjunto mínimo do conceito de Flow do CLAUDE.md §6 — sem
// propagação, sem `correlate_on`, sem `run_if`, sem `collect`/`first`/
// `fan_out`. Qualquer trabalho futuro de encadear dados entre queries
// pertence a um módulo `flow/` de verdade (roadmap itens 6/9/10/12), não
// deve crescer dentro deste componente.

import { useState } from "react";
import { client, type ResultSet } from "../../ipc/client";
import type { ConnectionSummary, QuerySummary } from "../../ipc/types";
import { ParamsPanel } from "../queries/ParamsPanel";
import { QuerySelector } from "./QuerySelector";
import { RunAllBar } from "./RunAllBar";
import { ResultBoard } from "./ResultBoard";
import { AdhocQueryBlock } from "./AdhocQueryBlock";
import { AdhocTabs } from "./AdhocTabs";
import { mergedParamNames, missingParamNames, paramsFor } from "./params";
import {
  adhocSlotFromQuery,
  newAdhocSlot,
  type AdhocSlot,
  type QueryExecutionState,
  IDLE_STATE,
} from "./types";

interface BoardProps {
  queries: QuerySummary[];
  connections: ConnectionSummary[];
  activeConnectionSlug: string | null;
  onQuerySaved: () => void;
  onQueryDeleted: () => void;
}

/** Representa um `AdhocSlot` executável no mesmo formato de `QuerySummary`
 * só para exibição/execução no Painel — é o que permite reusar
 * `ResultBoard`/`QueryPanel`/`mergedParamNames` sem nenhuma mudança neles.
 * Nunca é persistido; `id`/`created_at`/`updated_at` são só placeholders. */
function adhocAsQuerySummary(slot: AdhocSlot): QuerySummary {
  const firstLine = slot.sql.trim().split("\n")[0]?.slice(0, 60);
  return {
    id: slot.id,
    slug: slot.id,
    name: firstLine !== undefined && firstLine.length > 0 ? firstLine : "SQL ad-hoc",
    connection_slug: slot.connectionSlug ?? "",
    sql: slot.sql,
    params: slot.paramNames.map((name) => ({ name, type: "string", required: true })),
    created_at: "",
    updated_at: "",
  };
}

export function Board({
  queries,
  connections,
  activeConnectionSlug,
  onQuerySaved,
  onQueryDeleted,
}: BoardProps) {
  const [selectedSlugs, setSelectedSlugs] = useState<string[]>([]);
  const [adhocSlots, setAdhocSlots] = useState<AdhocSlot[]>([]);
  // Qual aba ad-hoc está visível — as outras continuam rodando em
  // segundo plano (ver `executions`), só não estão renderizadas.
  const [activeAdhocId, setActiveAdhocId] = useState<string | null>(null);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [executions, setExecutions] = useState<Record<string, QueryExecutionState>>({});

  const selectedQueries = queries.filter((q) => selectedSlugs.includes(q.slug));
  const activeAdhocSlot = adhocSlots.find((s) => s.id === activeAdhocId);
  const runnableAdhoc = adhocSlots.filter(
    (s) => s.connectionSlug !== null && s.sql.trim().length > 0,
  );
  const adhocSummaries = runnableAdhoc.map(adhocAsQuerySummary);
  const boardQueries = [...selectedQueries, ...adhocSummaries];

  const paramNames = mergedParamNames(boardQueries);
  const runningRunIds = Object.entries(executions)
    .filter(([, state]) => state.status === "running")
    .map(([runId]) => runId);
  const anyRunning = runningRunIds.length > 0;

  async function runById(runId: string, execute: (executionId: string) => Promise<ResultSet>) {
    const executionId = crypto.randomUUID();
    setExecutions((prev) => ({
      ...prev,
      [runId]: { status: "running", executionId, result: null, error: null },
    }));
    try {
      const res = await execute(executionId);
      setExecutions((prev) => ({
        ...prev,
        [runId]: { status: "ok", executionId: null, result: res, error: null },
      }));
    } catch (err) {
      const message = String(err);
      setExecutions((prev) => ({
        ...prev,
        [runId]: {
          status: message.includes("cancel") ? "cancelled" : "error",
          executionId: null,
          result: null,
          error: message,
        },
      }));
    }
  }

  // Roda sem sequer chamar o backend quando falta preencher parâmetro
  // obrigatório — vira um erro claro na hora, em vez de deixar o driver
  // rejeitar com algo tipo "valor '' não é um inteiro válido".
  function blockOnMissingParams(runId: string, missing: string[]): boolean {
    if (missing.length === 0) {
      return false;
    }
    setExecutions((prev) => ({
      ...prev,
      [runId]: {
        status: "error",
        executionId: null,
        result: null,
        error: `preencha o(s) parâmetro(s) obrigatório(s): ${missing.join(", ")}`,
      },
    }));
    return true;
  }

  async function runSavedQuery(query: QuerySummary) {
    if (blockOnMissingParams(query.slug, missingParamNames(query, paramValues))) {
      return;
    }
    await runById(query.slug, (executionId) =>
      client.queryRun(executionId, query.slug, paramsFor(query, paramValues)),
    );
  }

  async function runAdhocSlot(slot: AdhocSlot) {
    const connectionSlug = slot.connectionSlug;
    if (connectionSlug === null) {
      return;
    }
    const summary = adhocAsQuerySummary(slot);
    if (blockOnMissingParams(slot.id, missingParamNames(summary, paramValues))) {
      return;
    }
    await runById(slot.id, (executionId) =>
      client.queryRunAdhoc(executionId, connectionSlug, slot.sql, paramsFor(summary, paramValues)),
    );
  }

  async function handleRunAll() {
    await Promise.allSettled([
      ...selectedQueries.map((q) => runSavedQuery(q)),
      ...runnableAdhoc.map((s) => runAdhocSlot(s)),
    ]);
  }

  // Blocos ad-hoc chamam `runAdhocSlot` direto (RunBar mora dentro do
  // próprio card, ver AdhocQueryBlock) — este dispatcher só existe para
  // as queries salvas listadas no `ResultBoard`.
  function handleRunOne(slug: string) {
    const query = queries.find((q) => q.slug === slug);
    if (query !== undefined) {
      void runSavedQuery(query);
    }
  }

  function handleCancelOne(runId: string) {
    const id = executions[runId]?.executionId;
    if (id !== null && id !== undefined) {
      void client.queryCancel(id);
    }
  }

  function handleCancelAll() {
    for (const state of Object.values(executions)) {
      if (state.status === "running" && state.executionId !== null) {
        void client.queryCancel(state.executionId);
      }
    }
  }

  // Mescla contra o estado mais recente (via updater function), nunca
  // contra um `slot` fechado num render antigo — é o que evita uma
  // atualização atrasada (ex. extração de parâmetro com debounce)
  // sobrescrever um campo mudado nesse meio-tempo por outro caminho
  // (ex. `savedAsSlug` setado por um "Salvar" concluído antes do timer).
  function updateAdhocSlot(id: string, patch: Partial<AdhocSlot>) {
    setAdhocSlots((prev) => prev.map((s) => (s.id === id ? { ...s, ...patch } : s)));
  }

  // Também limpa `executions[id]` na hora, não só `adhocSlots` — senão um
  // card fechado enquanto ainda "rodando" continuava contando pra
  // `anyRunning`/`summary` (que somam TODAS as entradas de `executions`,
  // não só as dos slots ainda visíveis), deixando "Executar tudo"
  // desabilitado por uma aba que já nem existe mais.
  function removeAdhocSlot(id: string) {
    setAdhocSlots((prev) => prev.filter((s) => s.id !== id));
    setExecutions((prev) =>
      Object.fromEntries(Object.entries(prev).filter(([runId]) => runId !== id)),
    );
  }

  function addAdhocTab() {
    const id = crypto.randomUUID();
    setAdhocSlots((prev) => [...prev, newAdhocSlot(id, activeConnectionSlug)]);
    setActiveAdhocId(id);
  }

  // "Editar" abre uma cópia editável da query salva como bloco ad-hoc —
  // reusa o mecanismo de "Salvar" que já existe (ver AdhocQueryBlock),
  // que detecta `savedAsSlug` já preenchido e chama `queryUpdate` em vez
  // de `queryCreate`. A linha original só muda quando isso é confirmado.
  // Sempre foca a aba nova — senão "Editar" adicionava uma aba fora de
  // vista, sem nenhum indício visual de que algo abriu.
  function handleEditQuery(query: QuerySummary) {
    const id = crypto.randomUUID();
    setAdhocSlots((prev) => [...prev, adhocSlotFromQuery(id, query)]);
    setActiveAdhocId(id);
  }

  // Fechar cancela primeiro se estiver rodando (sem esperar a
  // confirmação do backend — a aba já some na hora) e escolhe a próxima
  // a focar do jeito que um navegador escolhe ao fechar uma aba: a
  // anterior, ou a primeira que sobrar.
  function closeAdhocTab(id: string) {
    if (executions[id]?.status === "running") {
      handleCancelOne(id);
    }
    removeAdhocSlot(id);
    if (activeAdhocId === id) {
      const idx = adhocSlots.findIndex((s) => s.id === id);
      const remaining = adhocSlots.filter((s) => s.id !== id);
      const next = remaining[Math.max(0, idx - 1)] ?? remaining[0];
      setActiveAdhocId(next?.id ?? null);
    }
  }

  function reorderAdhocSlots(draggedId: string, targetId: string) {
    setAdhocSlots((prev) => {
      const fromIdx = prev.findIndex((s) => s.id === draggedId);
      const toIdx = prev.findIndex((s) => s.id === targetId);
      if (fromIdx === -1 || toIdx === -1) {
        return prev;
      }
      const next = [...prev];
      const [moved] = next.splice(fromIdx, 1);
      if (moved === undefined) {
        return prev;
      }
      next.splice(toIdx, 0, moved);
      return next;
    });
  }

  async function handleDeleteQuery(slug: string) {
    await client.queryDelete(slug);
    setSelectedSlugs((prev) => prev.filter((s) => s !== slug));
    setExecutions((prev) =>
      Object.fromEntries(Object.entries(prev).filter(([runId]) => runId !== slug)),
    );
    onQueryDeleted();
  }

  const summary = {
    total: boardQueries.length,
    running: boardQueries.filter((q) => executions[q.slug]?.status === "running").length,
    ok: boardQueries.filter((q) => executions[q.slug]?.status === "ok").length,
    error: boardQueries.filter((q) => executions[q.slug]?.status === "error").length,
    cancelled: boardQueries.filter((q) => executions[q.slug]?.status === "cancelled").length,
  };

  return (
    <section aria-label="Painel">
      <h2>Painel</h2>
      <QuerySelector
        queries={queries}
        selectedSlugs={selectedSlugs}
        runningSlugs={runningRunIds}
        onChange={setSelectedSlugs}
        onDelete={handleDeleteQuery}
        onEdit={handleEditQuery}
      />

      {/* Parâmetros vem ANTES dos blocos (que já têm seu próprio botão
          "Executar") — senão o campo fica escondido depois do primeiro
          botão de rodar, e é fácil clicar "Executar" sem nunca ter visto
          o campo pra preencher. */}
      <ParamsPanel
        paramNames={paramNames}
        values={paramValues}
        onChange={(name, value) => { setParamValues((prev) => ({ ...prev, [name]: value })); }}
      />

      <div className="board-adhoc-zone">
        <AdhocTabs
          slots={adhocSlots}
          activeId={activeAdhocId}
          executions={executions}
          onSelect={setActiveAdhocId}
          onClose={closeAdhocTab}
          onReorder={reorderAdhocSlots}
          onAddNew={addAdhocTab}
        />
        {activeAdhocSlot !== undefined ? (
          <AdhocQueryBlock
            key={activeAdhocSlot.id}
            slot={activeAdhocSlot}
            connections={connections}
            disabled={runningRunIds.includes(activeAdhocSlot.id)}
            onChange={(patch) => { updateAdhocSlot(activeAdhocSlot.id, patch); }}
            onSaved={onQuerySaved}
            state={executions[activeAdhocSlot.id] ?? IDLE_STATE}
            onRun={() => void runAdhocSlot(activeAdhocSlot)}
            onCancel={() => { handleCancelOne(activeAdhocSlot.id); }}
          />
        ) : (
          <p className="empty-state">Nenhuma consulta ad-hoc aberta — clique em "+" para começar.</p>
        )}
      </div>

      <RunAllBar
        disabled={boardQueries.length === 0}
        anyRunning={anyRunning}
        onRunAll={() => void handleRunAll()}
        onCancelAll={handleCancelAll}
        summary={summary}
      />
      <ResultBoard
        queries={selectedQueries}
        executions={executions}
        onRunOne={handleRunOne}
        onCancelOne={handleCancelOne}
      />
    </section>
  );
}
