import { useEffect, useRef, useState } from "react";
import { Settings } from "lucide-react";
import "./App.css";
import { client } from "./ipc/client";
import type { ConnectionSummary, ConnectionUpdate, NewConnection, QuerySummary } from "./ipc/types";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { SchemaExplorer } from "./features/connections/SchemaExplorer";
import { ErrorPanel } from "./components/ErrorPanel";
import { Board, type BoardHandle } from "./features/board/Board";

function App() {
  const boardRef = useRef<BoardHandle>(null);
  const connectionsDialog = useRef<HTMLDialogElement>(null);
  const [pages, setPages] = useState<{id: string; name: string}[]>(() => {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem("queryboard.pages") ?? "null");
      if (Array.isArray(saved)) {
        const pages = (saved as unknown[]).filter((p): p is {id: string; name: string} => typeof p === "object" && p !== null && "id" in p && typeof p.id === "string" && "name" in p && typeof p.name === "string");
        if (pages.length) return pages;
      }
    } catch { /* Use the initial workspace when storage is unavailable. */ }
    return [{ id: "default", name: "Product" }];
  });
  const [activePage, setActivePage] = useState(pages[0]?.id ?? "default");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pageName, setPageName] = useState("");
  useEffect(() => {
    try { localStorage.setItem("queryboard.pages", JSON.stringify(pages)); }
    catch { setLoadError("Não foi possível salvar as páginas neste dispositivo."); }
  }, [pages]);
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
      {loadError !== null && <ErrorPanel message={loadError} />}

      <div className="app-shell">
        <aside className="sidebar" aria-label="Conexões">
          <div className="workspace-brand"><span aria-hidden="true">Q.</span><div><strong>Queryboard</strong><small>Workspace SQL</small></div></div>
          <button type="button" onClick={() => connectionsDialog.current?.showModal()}>Conexão</button>
          <nav className="workspace-pages" aria-label="Páginas de consultas">
            {pages.map(page => <div key={page.id} className="workspace-page-row" data-active={page.id === activePage}>
              <button className="workspace-page-link" type="button" aria-current={!settingsOpen && page.id === activePage ? "page" : undefined} onClick={() => { setActivePage(page.id); setSettingsOpen(false); }}>{page.name}</button>
              <button className="workspace-page-settings" type="button" aria-label={`Configurações de ${page.name}`} title={`Configurações de ${page.name}`} aria-current={settingsOpen && page.id === activePage ? "page" : undefined} onClick={() => { setActivePage(page.id); setSettingsOpen(true); }}><Settings size={16} strokeWidth={1.75} aria-hidden="true" /></button>
            </div>)}
          </nav>
          <form onSubmit={event => {
            event.preventDefault();
            if (!pageName.trim()) return;
            const page = { id: crypto.randomUUID(), name: pageName.trim() };
            setPages(previous => [...previous, page]); setActivePage(page.id); setSettingsOpen(true); setPageName("");
          }}>
            <label>Nome da página<input value={pageName} onChange={event => { setPageName(event.target.value); }} placeholder="Ex.: Product" required /></label>
            <button type="submit">+ Criar página</button>
          </form>
          <dialog ref={connectionsDialog} className="connections-modal" aria-labelledby="connections-title">
          <header className="connections-modal__header"><h2 id="connections-title">Conexões</h2><button type="button" onClick={() => connectionsDialog.current?.close()}>Fechar</button></header>
          <div className="sidebar__heading">
            <h2>Conexões</h2>
            <span
              className="active-connection-banner"
              role="status"
              data-connected={activeConnection !== null}
            >
              {activeConnection !== null
                ? `${activeConnection.name} (${activeConnection.slug})`
                : "nenhuma selecionada"}
            </span>
          </div>
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
            + Nova conexão
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
          </dialog>
          {settingsOpen && <SchemaExplorer
            connectionSlug={activeConnectionSlug}
            onInsert={(text) => { boardRef.current?.insertSchemaText(text); }}
          />}
        </aside>

        <div className="main-panel">
          <header className="workspace-header">
            <div><p>WORKSPACE / {settingsOpen ? "CONFIGURAÇÕES" : "CONSULTAS"}</p><h1>{settingsOpen ? "Configurações" : pages.find(page => page.id === activePage)?.name}</h1></div>
            <dl className="workspace-config" aria-label="Configuração do workspace">
              <div><dt>Conexões</dt><dd>{connections.length}</dd></div>
              <div><dt>Consultas salvas</dt><dd>{queries.length}</dd></div>
              <div><dt>Conexão selecionada</dt><dd>{activeConnection?.name ?? "Nenhuma"}</dd></div>
            </dl>
            <span className="workspace-badge">Somente leitura</span>
          </header>
          {settingsOpen && <div className="workspace-settings-nav"><label>Configurar workspace<select value={activePage} onChange={event => { setActivePage(event.target.value); }}>{pages.map(page => <option key={page.id} value={page.id}>{page.name}</option>)}</select></label><button type="button" onClick={() => { setSettingsOpen(false); }}>Ver workspace</button></div>}
          {pages.map(page => <div key={page.id} hidden={page.id !== activePage}><Board
            active={page.id === activePage}
            view={settingsOpen ? "settings" : "workspace"}
            onOpenSettings={() => { setSettingsOpen(true); }}
            storageKey={`queryboard.page.${page.id}`}
            ref={page.id === activePage ? boardRef : undefined}
            queries={queries}
            connections={connections}
            activeConnectionSlug={activeConnectionSlug}
            onQuerySaved={() => void refresh()}
            onQueryDeleted={() => void refresh()}
          /></div>)}
        </div>
      </div>
    </main>
  );
}

export default App;
