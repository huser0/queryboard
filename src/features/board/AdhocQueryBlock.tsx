import { useEffect, useState } from "react";
import { client } from "../../ipc/client";
import type { ConnectionSummary, NewQuery, QuerySummary } from "../../ipc/types";
import { QueryEditor } from "../queries/QueryEditor";
import { RunBar } from "../queries/RunBar";
import { ResultGrid } from "../../components/ResultGrid";
import { ErrorPanel } from "../../components/ErrorPanel";
import type { AdhocSlot, QueryExecutionState } from "./types";

interface AdhocQueryBlockProps {
  slot: AdhocSlot;
  connections: ConnectionSummary[];
  disabled: boolean;
  // Patch, não o slot inteiro — se fosse `{ ...slot, ... }` fechando
  // sobre o `slot` de um render antigo, um `onChange` que resolve depois
  // (ex. o timer de debounce da extração de parâmetro) pode sobrescrever
  // um campo mudado nesse meio-tempo por outro caminho (ex. `savedAsSlug`
  // setado por um "Salvar" que terminou antes do timer dispar). O
  // chamador (Board) sempre mescla contra o estado mais recente.
  onChange: (patch: Partial<AdhocSlot>) => void;
  onSaved: (saved: QuerySummary) => void;
  // Execução e resultado ficam DENTRO do próprio card do bloco — antes
  // apareciam num card separado, longe do editor, e dava a impressão de
  // que a consulta "não trazia resultado" quando na verdade só estava
  // fora de vista.
  state: QueryExecutionState;
  onRun: () => void;
  onCancel: () => void;
}

export function AdhocQueryBlock({
  slot,
  connections,
  disabled,
  onChange,
  onSaved,
  state,
  onRun,
  onCancel,
}: AdhocQueryBlockProps) {
  const [saveSlug, setSaveSlug] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [extractError, setExtractError] = useState<string | null>(null);

  // Mesmo padrão de debounce de App.tsx: extração de parâmetro reusa o
  // backend (sql::params::extract_params) contra o SQL ainda não salvo.
  // Falha de extração (SQL rejeitada pelo guard) precisa aparecer na
  // tela — senão o campo de parâmetro só some sem explicar o motivo.
  useEffect(() => {
    if (slot.sql.trim().length === 0) {
      if (slot.paramNames.length > 0) {
        onChange({ paramNames: [] });
      }
      setExtractError(null);
      return;
    }
    if (slot.connectionSlug === null) {
      // O <select> logo acima já deixa óbvio o que falta — sem mensagem
      // duplicada aqui.
      if (slot.paramNames.length > 0) {
        onChange({ paramNames: [] });
      }
      return;
    }
    const timer = setTimeout(() => {
      client
        .queryExtractParams(slot.sql, slot.connectionSlug ?? "")
        .then((names) => {
          onChange({ paramNames: names });
          setExtractError(null);
        })
        .catch((err: unknown) => {
          onChange({ paramNames: [] });
          setExtractError(String(err));
        });
    }, 300);
    return () => { clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [slot.sql, slot.connectionSlug]);

  const isEditingSaved = slot.savedAsSlug !== null;

  async function handleSave() {
    if (slot.connectionSlug === null) {
      setSaveError("selecione uma connection primeiro");
      return;
    }
    const slug = slot.savedAsSlug ?? saveSlug;
    if (slug.trim().length === 0) {
      setSaveError("slug é obrigatório para salvar");
      return;
    }
    setSaving(true);
    setSaveError(null);
    try {
      const input: NewQuery = {
        slug,
        name: slug,
        connection_slug: slot.connectionSlug,
        sql: slot.sql,
        params: slot.paramNames.map((name) => ({ name, type: "string", required: true })),
      };
      // Se o bloco já nasceu apontando pra uma query salva (ver "Editar"
      // em QuerySelector/adhocSlotFromQuery), salvar de novo atualiza a
      // mesma linha em vez de criar uma nova.
      const saved = slot.savedAsSlug !== null
        ? await client.queryUpdate(slot.savedAsSlug, input)
        : await client.queryCreate(input);
      onChange({ savedAsSlug: saved.slug });
      onSaved(saved);
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="board-panel" aria-label="Bloco de SQL ad-hoc">
      {/* Fechar/reordenar essa consulta mora na aba (ver AdhocTabs), não
          aqui dentro — o card só cuida do conteúdo em si. */}
      <label>
        Connection
        <select
          value={slot.connectionSlug ?? ""}
          disabled={disabled}
          onChange={(e) => {
            const value = e.currentTarget.value;
            onChange({ connectionSlug: value.length > 0 ? value : null });
          }}
        >
          <option value="">selecione...</option>
          {connections.map((conn) => (
            <option key={conn.slug} value={conn.slug}>
              {conn.name} ({conn.slug})
            </option>
          ))}
        </select>
      </label>

      <QueryEditor
        value={slot.sql}
        onChange={(value) => { onChange({ sql: value }); }}
        error={extractError}
      />

      <RunBar
        status={state.status}
        onRun={onRun}
        onCancel={onCancel}
        rowCount={state.result?.rows.length ?? null}
        elapsedMs={state.result?.elapsed_ms ?? null}
        truncated={state.result?.truncated ?? false}
      />

      <div className="board-panel__save-row">
        <input
          aria-label="Slug para salvar"
          placeholder="slug para salvar (opcional)"
          value={slot.savedAsSlug ?? saveSlug}
          disabled={disabled || saving || isEditingSaved}
          onChange={(e) => { setSaveSlug(e.currentTarget.value); }}
        />
        <button
          type="button"
          data-variant="primary"
          onClick={() => void handleSave()}
          disabled={disabled || saving}
        >
          {saving ? "Salvando…" : isEditingSaved ? "Atualizar" : "Salvar"}
        </button>
        {slot.savedAsSlug !== null && <span role="status">salvo como {slot.savedAsSlug}</span>}
        {saveError !== null && <span role="alert">{saveError}</span>}
      </div>

      {state.error !== null && <ErrorPanel message={state.error} />}
      {state.result !== null && <ResultGrid result={state.result} />}
    </section>
  );
}
