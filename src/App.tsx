import { useEffect, useState } from "react";
import "./App.css";
import { client } from "./ipc/client";
import type { ConnectionSummary, ConnectionUpdate, NewConnection, QuerySummary } from "./ipc/types";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { SchemaExplorer } from "./features/connections/SchemaExplorer";
import { ErrorPanel } from "./components/ErrorPanel";
import { Board } from "./features/board/Board";

function App() {
  const [connections, setConnections] = useState<ConnectionSummary[]>([]);
  const [queries, setQueries] = useState<QuerySummary[]>([]);
  const [activeConnectionSlug, setActiveConnectionSlug] = useState<string | null>(null);
  const [creatingConnection, setCreatingConnection] = useState(false);
  const [showConnectionForm, setShowConnectionForm] = useState(false);
  const [editingConnection, setEditingConnection] = useState<ConnectionSummary | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

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

  async function handleCreateConnection(input: NewConnection) {
    setCreatingConnection(true);
    try {
      await client.connectionCreate(input);
      await refresh();
      setShowConnectionForm(false);
    } finally {
      setCreatingConnection(false);
    }
  }

  async function handleUpdateConnection(slug: string, input: ConnectionUpdate) {
    setCreatingConnection(true);
    try {
      await client.connectionUpdate(slug, input);
      await refresh();
      setEditingConnection(null);
    } finally {
      setCreatingConnection(false);
    }
  }

  const activeConnection = connections.find((c) => c.slug === activeConnectionSlug) ?? null;

  return (
    <main className="container">
      <header className="app-header">
        <h1>queryboard</h1>
        <span
          className="active-connection-banner"
          role="status"
          data-connected={activeConnection !== null}
        >
          {activeConnection !== null
            ? `Conectado a: ${activeConnection.name} (${activeConnection.slug})`
            : "Nenhuma connection selecionada"}
        </span>
      </header>
      {loadError !== null && <ErrorPanel message={loadError} />}

      <div className="app-shell">
        <aside className="sidebar" aria-label="Connections">
          <h2>Connections</h2>
          <ConnectionList
            connections={connections}
            selectedSlug={activeConnectionSlug}
            onSelect={setActiveConnectionSlug}
            onTest={client.connectionTest}
            onDelete={async (slug) => {
              await client.connectionDelete(slug);
              await refresh();
              if (activeConnectionSlug === slug) {
                setActiveConnectionSlug(null);
              }
            }}
            onEdit={(connection) => {
              setEditingConnection(connection);
              setShowConnectionForm(false);
            }}
          />
          <button
            type="button"
            data-variant="primary"
            onClick={() => {
              setEditingConnection(null);
              setShowConnectionForm((v) => !v);
            }}
          >
            + Nova connection
          </button>
          {showConnectionForm && (
            <ConnectionForm onSubmit={handleCreateConnection} submitting={creatingConnection} />
          )}
          {editingConnection && (
            <ConnectionForm
              editing={editingConnection}
              onSubmit={handleCreateConnection}
              onUpdate={handleUpdateConnection}
              onCancel={() => { setEditingConnection(null); }}
              submitting={creatingConnection}
            />
          )}
          <SchemaExplorer connectionSlug={activeConnectionSlug} />
        </aside>

        <div className="main-panel">
          <Board
            queries={queries}
            connections={connections}
            activeConnectionSlug={activeConnectionSlug}
            onQuerySaved={() => void refresh()}
            onQueryDeleted={() => void refresh()}
          />
        </div>
      </div>
    </main>
  );
}

export default App;
