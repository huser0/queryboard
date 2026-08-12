import { useEffect, useState } from "react";
import { client, type TableInfo } from "../../ipc/client";

interface SchemaExplorerProps {
  connectionSlug: string | null;
  /** Chamado com o identificador (nome de tabela ou coluna) clicado, pra
   * inserir no editor de SQL ad-hoc ativo — ver `Board.insertSchemaText`. */
  onInsert: (text: string) => void;
}

/** Árvore de tabelas/colunas da connection ativa, na sidebar — ajuda a
 * escrever SQL sem precisar decorar nome de tabela/coluna de cabeça ou
 * ficar trocando de janela pra conferir o schema. Introspecção passa
 * pelo mesmo `connection_schema` (backend) que só roda um SELECT comum
 * contra o catálogo, sujeito às mesmas garantias de somente-leitura de
 * qualquer outra query. */
export function SchemaExplorer({ connectionSlug, onInsert }: SchemaExplorerProps) {
  const [tables, setTables] = useState<TableInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  // Incrementado pelo botão de recarregar — só existe pra disparar o
  // efeito de novo sem duplicar a lógica de fetch aqui e ali.
  const [reloadToken, setReloadToken] = useState(0);

  useEffect(() => {
    if (connectionSlug === null) {
      setTables(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    client
      .connectionSchema(connectionSlug)
      .then((result) => {
        setTables(result);
      })
      .catch((err: unknown) => {
        setError(String(err));
        setTables(null);
      })
      .finally(() => {
        setLoading(false);
      });
  }, [connectionSlug, reloadToken]);

  if (connectionSlug === null) {
    return null;
  }

  function toggle(name: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  }

  return (
    <div className="schema-explorer">
      <div className="schema-explorer__heading">
        <h2>Tabelas</h2>
        <button
          type="button"
          className="schema-explorer__reload"
          aria-label="Recarregar schema"
          disabled={loading}
          onClick={() => { setReloadToken((n) => n + 1); }}
        >
          ↻
        </button>
      </div>

      {loading && <p className="schema-explorer__status">carregando…</p>}
      {error !== null && <span role="alert">{error}</span>}

      {tables !== null && tables.length === 0 && !loading && (
        <p className="empty-state">Nenhuma tabela encontrada.</p>
      )}

      {tables !== null && tables.length > 0 && (
        <ul className="schema-explorer__tables">
          {tables.map((table) => {
            const isOpen = expanded.has(table.name);
            return (
              <li key={table.name}>
                <button
                  type="button"
                  className="schema-explorer__table-toggle"
                  aria-expanded={isOpen}
                  title={`Expandir e inserir "${table.name}" no editor`}
                  onClick={() => {
                    toggle(table.name);
                    onInsert(table.name);
                  }}
                >
                  <span aria-hidden="true">{isOpen ? "▾" : "▸"}</span>
                  {table.name}
                </button>
                {isOpen && (
                  <ul className="schema-explorer__columns">
                    {table.columns.map((column) => (
                      <li key={column.name}>
                        <button
                          type="button"
                          className="schema-explorer__column"
                          title={`Inserir "${column.name}" no editor`}
                          onClick={() => { onInsert(column.name); }}
                        >
                          <span className="schema-explorer__column-name">{column.name}</span>
                          <span className="schema-explorer__column-type">{column.data_type}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
