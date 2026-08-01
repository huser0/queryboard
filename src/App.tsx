import { useEffect, useRef, useState } from "react";
import "./App.css";
import { client, type ResultSet } from "./ipc/client";
import type { ConnectionSummary, NewConnection, QuerySummary } from "./ipc/types";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { QueryEditor } from "./features/queries/QueryEditor";
import { ParamsPanel } from "./features/queries/ParamsPanel";
import { RunBar, type RunStatus } from "./features/queries/RunBar";
import { ResultGrid } from "./components/ResultGrid";
import { ErrorPanel } from "./components/ErrorPanel";

function App() {
  const [connections, setConnections] = useState<ConnectionSummary[]>([]);
  const [queries, setQueries] = useState<QuerySummary[]>([]);
  const [selectedConnectionSlug, setSelectedConnectionSlug] = useState<string | null>(null);
  const [creatingConnection, setCreatingConnection] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [querySlug, setQuerySlug] = useState("");
  const [sql, setSql] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [paramNames, setParamNames] = useState<string[]>([]);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});

  const [runStatus, setRunStatus] = useState<RunStatus>("idle");
  const [result, setResult] = useState<ResultSet | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const executionIdRef = useRef<string | null>(null);

  async function refresh() {
    try {
      const [conns, qs] = await Promise.all([client.connectionList(), client.queryList()]);
      setConnections(conns);
      setQueries(qs);
      setLoadError(null);
    } catch (err) {
      setLoadError(String(err));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  // Extração de parâmetros reusa o backend (sql::params::extract_params),
  // não uma heurística de regex no front — CLAUDE.md §8.
  useEffect(() => {
    if (selectedConnectionSlug === null || sql.trim().length === 0) {
      setParamNames([]);
      return;
    }
    const timer = setTimeout(() => {
      client
        .queryExtractParams(sql, selectedConnectionSlug)
        .then(setParamNames)
        .catch(() => { setParamNames([]); });
    }, 300);
    return () => { clearTimeout(timer); };
  }, [sql, selectedConnectionSlug]);

  async function handleCreateConnection(input: NewConnection) {
    setCreatingConnection(true);
    try {
      await client.connectionCreate(input);
      await refresh();
    } finally {
      setCreatingConnection(false);
    }
  }

  async function handleSaveQuery() {
    if (selectedConnectionSlug === null) {
      setSaveError("selecione uma connection primeiro");
      return;
    }
    setSaveError(null);
    try {
      await client.queryCreate({
        slug: querySlug,
        name: querySlug,
        connection_slug: selectedConnectionSlug,
        sql,
        params: paramNames.map((name) => ({ name, type: "string", required: true })),
      });
      await refresh();
    } catch (err) {
      setSaveError(String(err));
    }
  }

  async function handleRun() {
    if (queries.every((q) => q.slug !== querySlug)) {
      setRunError("salve a query antes de executar");
      return;
    }
    const executionId = crypto.randomUUID();
    executionIdRef.current = executionId;
    setRunStatus("running");
    setRunError(null);
    setResult(null);
    try {
      const res = await client.queryRun(executionId, querySlug, paramValues);
      setResult(res);
      setRunStatus("ok");
    } catch (err) {
      const message = String(err);
      setRunStatus(message.includes("cancel") ? "cancelled" : "error");
      setRunError(message);
    } finally {
      executionIdRef.current = null;
    }
  }

  function handleCancel() {
    if (executionIdRef.current !== null) {
      void client.queryCancel(executionIdRef.current);
    }
  }

  return (
    <main className="container">
      <h1>queryboard</h1>
      {loadError !== null && <ErrorPanel message={loadError} />}

      <section aria-label="Connections">
        <h2>Connections</h2>
        <ConnectionList
          connections={connections}
          selectedSlug={selectedConnectionSlug}
          onSelect={setSelectedConnectionSlug}
          onTest={client.connectionTest}
          onDelete={async (slug) => {
            await client.connectionDelete(slug);
            await refresh();
          }}
        />
        <ConnectionForm onSubmit={handleCreateConnection} submitting={creatingConnection} />
      </section>

      <section aria-label="Query">
        <h2>Query</h2>
        <label>
          Slug da query
          <input value={querySlug} onChange={(e) => { setQuerySlug(e.currentTarget.value); }} />
        </label>
        <QueryEditor value={sql} onChange={setSql} error={saveError} />
        <ParamsPanel
          paramNames={paramNames}
          values={paramValues}
          onChange={(name, value) => { setParamValues((prev) => ({ ...prev, [name]: value })); }}
        />
        <button type="button" onClick={() => void handleSaveQuery()}>
          Salvar query
        </button>

        <RunBar
          status={runStatus}
          onRun={() => void handleRun()}
          onCancel={handleCancel}
          rowCount={result?.rows.length ?? null}
          elapsedMs={result?.elapsed_ms ?? null}
          truncated={result?.truncated ?? false}
        />
        {runError !== null && <ErrorPanel message={runError} />}
        {result !== null && <ResultGrid result={result} />}
      </section>
    </main>
  );
}

export default App;
