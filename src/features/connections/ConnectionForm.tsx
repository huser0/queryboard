import { useState } from "react";
import type { SubmitEventHandler } from "react";
import type { ConnectionKind, ConnectionSummary, ConnectionUpdate, NewConnection } from "../../ipc/types";
import { ReadOnlyWarning } from "./ReadOnlyWarning";

const KINDS: ConnectionKind[] = ["postgres", "oracle", "mysql"];
const DEFAULT_PORT: Record<ConnectionKind, number> = {
  postgres: 5432,
  oracle: 1521,
  mysql: 3306,
};

interface ConnectionFormProps {
  submitting: boolean;
  /** Ausente (modo criação): connection nova, senha obrigatória. */
  editing?: ConnectionSummary;
  onSubmit: (input: NewConnection) => Promise<void>;
  onUpdate?: (slug: string, input: ConnectionUpdate) => Promise<void>;
  onCancel?: () => void;
}

export function ConnectionForm({
  submitting,
  editing,
  onSubmit,
  onUpdate,
  onCancel,
}: ConnectionFormProps) {
  const [slug, setSlug] = useState(editing?.slug ?? "");
  const [name, setName] = useState(editing?.name ?? "");
  const [kind, setKind] = useState<ConnectionKind>(editing?.kind ?? "postgres");
  const [host, setHost] = useState(editing?.host ?? "");
  const [port, setPort] = useState(editing?.port ?? DEFAULT_PORT.postgres);
  const [database, setDatabase] = useState(editing?.database ?? "");
  const [username, setUsername] = useState(editing?.username ?? "");
  const [password, setPassword] = useState("");

  const handleSubmit: SubmitEventHandler<HTMLFormElement> = (e) => {
    e.preventDefault();
    void submit();
  };

  async function submit() {
    if (editing) {
      await onUpdate?.(editing.slug, {
        name,
        kind,
        host,
        port,
        database: database.length > 0 ? database : null,
        service_name: null,
        username,
        password: password.length > 0 ? password : undefined,
        max_rows: undefined,
        timeout_ms: undefined,
      });
      return;
    }
    await onSubmit({
      slug,
      name,
      kind,
      host,
      port,
      database: database.length > 0 ? database : null,
      service_name: null,
      username,
      password,
      max_rows: undefined,
      timeout_ms: undefined,
    });
  }

  return (
    <form onSubmit={handleSubmit} aria-label={editing ? "Editar conexão" : "Nova conexão"}>
      <ReadOnlyWarning />

      <label>
        Slug
        <input
          value={slug}
          onChange={(e) => { setSlug(e.currentTarget.value); }}
          placeholder="erp_prod"
          required
          disabled={editing !== undefined}
        />
      </label>

      <label>
        Nome
        <input
          value={name}
          onChange={(e) => { setName(e.currentTarget.value); }}
          placeholder="ERP produção"
          required
        />
      </label>

      <label>
        Tipo
        <select
          value={kind}
          onChange={(e) => {
            const next = e.currentTarget.value as ConnectionKind;
            setKind(next);
            if (editing === undefined) {
              setPort(DEFAULT_PORT[next]);
            }
          }}
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
      </label>

      <label>
        Host
        <input
          value={host}
          onChange={(e) => { setHost(e.currentTarget.value); }}
          required
        />
      </label>

      <label>
        Porta
        <input
          type="number"
          value={port}
          onChange={(e) => { setPort(Number(e.currentTarget.value)); }}
          required
        />
      </label>

      <label>
        Banco
        <input
          value={database}
          onChange={(e) => { setDatabase(e.currentTarget.value); }}
        />
      </label>

      <label>
        Usuário
        <input
          value={username}
          onChange={(e) => { setUsername(e.currentTarget.value); }}
          required
        />
      </label>

      <label>
        Senha
        <input
          type="password"
          value={password}
          onChange={(e) => { setPassword(e.currentTarget.value); }}
          placeholder={editing ? "deixe em branco para manter a senha atual" : undefined}
          required={editing === undefined}
        />
      </label>

      <button type="submit" data-variant="primary" disabled={submitting}>
        {submitting ? "Salvando…" : editing ? "Salvar alterações" : "Salvar conexão"}
      </button>
      {editing && onCancel && (
        <button type="button" onClick={onCancel} disabled={submitting}>
          Cancelar
        </button>
      )}
    </form>
  );
}
