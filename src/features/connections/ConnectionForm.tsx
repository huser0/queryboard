import { useState } from "react";
import type { SubmitEventHandler } from "react";
import type { ConnectionKind, NewConnection } from "../../ipc/types";
import { ReadOnlyWarning } from "./ReadOnlyWarning";

const KINDS: ConnectionKind[] = ["postgres", "oracle", "mysql"];
const DEFAULT_PORT: Record<ConnectionKind, number> = {
  postgres: 5432,
  oracle: 1521,
  mysql: 3306,
};

interface ConnectionFormProps {
  onSubmit: (input: NewConnection) => Promise<void>;
  submitting: boolean;
}

export function ConnectionForm({ onSubmit, submitting }: ConnectionFormProps) {
  const [slug, setSlug] = useState("");
  const [name, setName] = useState("");
  const [kind, setKind] = useState<ConnectionKind>("postgres");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(DEFAULT_PORT.postgres);
  const [database, setDatabase] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const handleSubmit: SubmitEventHandler<HTMLFormElement> = (e) => {
    e.preventDefault();
    void submit();
  };

  async function submit() {
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
    <form onSubmit={handleSubmit} aria-label="Nova connection">
      <ReadOnlyWarning />

      <label>
        Slug
        <input
          value={slug}
          onChange={(e) => { setSlug(e.currentTarget.value); }}
          placeholder="erp_prod"
          required
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
            setPort(DEFAULT_PORT[next]);
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
          required
        />
      </label>

      <button type="submit" data-variant="primary" disabled={submitting}>
        {submitting ? "Salvando…" : "Salvar connection"}
      </button>
    </form>
  );
}
