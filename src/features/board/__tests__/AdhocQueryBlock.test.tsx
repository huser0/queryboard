import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdhocQueryBlock } from "../AdhocQueryBlock";
import { client } from "../../../ipc/client";
import { IDLE_STATE, type AdhocSlot } from "../types";
import type { ConnectionSummary } from "../../../ipc/types";

vi.mock("../../../ipc/client", () => ({
  client: {
    queryExtractParams: vi.fn().mockResolvedValue([]),
    queryCreate: vi.fn(),
  },
}));

const conn: ConnectionSummary = {
  id: "c1",
  slug: "queryboard_local",
  name: "Postgres Local",
  kind: "postgres",
  host: "localhost",
  port: 55432,
  database: "queryboard",
  service_name: null,
  username: "queryboard",
  max_rows: 1000,
  timeout_ms: 30000,
  created_at: "",
  updated_at: "",
};

/** `AdhocQueryBlock` é controlado (o `slot` vive no pai) — este wrapper
 * segura o estado localmente, do jeito que `Board` faria de verdade, e
 * expõe os `paramNames`/`savedAsSlug` atuais para os testes afirmarem
 * sobre eles depois de uma atualização via `onChange`.
 *
 * Digitar no editor CodeMirror via `user.type` não funciona de forma
 * confiável no jsdom (mede layout com `getClientRects`, ausente no
 * jsdom) — por isso os testes partem de um `initialSql` já preenchido
 * em vez de simular digitação no editor. */
function ControlledBlock({
  connections,
  initialSql = "",
  onSaved = () => { /* noop */ },
}: {
  connections: ConnectionSummary[];
  initialSql?: string;
  onSaved?: (saved: { slug: string }) => void;
}) {
  const [slot, setSlot] = useState<AdhocSlot>({
    id: "slot-1",
    connectionSlug: null,
    sql: initialSql,
    paramNames: [],
    savedAsSlug: null,
  });
  return (
    <div>
      <AdhocQueryBlock
        slot={slot}
        connections={connections}
        disabled={false}
        onChange={(patch) => { setSlot((prev) => ({ ...prev, ...patch })); }}
        onSaved={onSaved}
        state={IDLE_STATE}
        onRun={() => { /* noop */ }}
        onCancel={() => { /* noop */ }}
      />
      <div data-testid="extracted-params">{slot.paramNames.join(",")}</div>
    </div>
  );
}

describe("AdhocQueryBlock", () => {
  // Sem isso, `mockResolvedValue` de um teste vaza pro próximo (mesma
  // spy compartilhada pelo módulo inteiro) — foi o que causou o segundo
  // teste herdar `["offer_id"]` do primeiro em vez do `[]` default.
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(client.queryExtractParams).mockResolvedValue([]);
  });

  it("extracts params once a connection is picked for already-typed SQL, debounced", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryExtractParams).mockResolvedValue(["offer_id"]);

    render(
      <ControlledBlock
        connections={[conn]}
        initialSql="SELECT * FROM t WHERE id = :offer_id"
      />,
    );

    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");

    await waitFor(() => {
      expect(client.queryExtractParams).toHaveBeenCalledWith(
        "SELECT * FROM t WHERE id = :offer_id",
        "queryboard_local",
      );
    });
    await waitFor(() => {
      expect(screen.getByTestId("extracted-params")).toHaveTextContent("offer_id");
    });
  });

  it("calls queryCreate with the expected shape when Salvar is clicked, and shows a saved badge", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryCreate).mockResolvedValue({
      id: "q1",
      slug: "meu_slug",
      name: "meu_slug",
      connection_slug: "queryboard_local",
      sql: "SELECT 1",
      params: [],
      created_at: "",
      updated_at: "",
    });
    const onSaved = vi.fn();

    render(<ControlledBlock connections={[conn]} initialSql="SELECT 1" onSaved={onSaved} />);
    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");

    // Espera a extração debounced terminar (e a atualização de estado
    // que ela dispara) antes de salvar — senão o timer pode resolver
    // *depois* do "Salvar" e sobrescrever `savedAsSlug` com um `slot`
    // capturado antes do save (closure velha do efeito de debounce).
    await waitFor(() => {
      expect(client.queryExtractParams).toHaveBeenCalledWith("SELECT 1", "queryboard_local");
    });

    await user.type(screen.getByLabelText("Slug para salvar"), "meu_slug");
    await user.click(screen.getByRole("button", { name: /^salvar$/i }));

    await waitFor(() => {
      expect(client.queryCreate).toHaveBeenCalledWith({
        slug: "meu_slug",
        name: "meu_slug",
        connection_slug: "queryboard_local",
        sql: "SELECT 1",
        params: [],
      });
    });
    expect(onSaved).toHaveBeenCalled();
    expect(await screen.findByText(/salvo como meu_slug/i)).toBeInTheDocument();
  });

  it("shows an inline error when queryCreate rejects", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryCreate).mockRejectedValue(
      new Error("já existe uma query com o slug 'x'"),
    );

    render(<ControlledBlock connections={[conn]} initialSql="SELECT 1" />);
    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");

    await user.type(screen.getByLabelText("Slug para salvar"), "x");
    await user.click(screen.getByRole("button", { name: /^salvar$/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "já existe uma query com o slug 'x'",
    );
  });

  it("renders its own run bar and result grid inline, not in a separate panel", async () => {
    const user = userEvent.setup();
    const onRun = vi.fn();
    const slot: AdhocSlot = {
      id: "slot-1",
      connectionSlug: "queryboard_local",
      sql: "SELECT 1",
      paramNames: [],
      savedAsSlug: null,
    };

    render(
      <AdhocQueryBlock
        slot={slot}
        connections={[conn]}
        disabled={false}
        onChange={() => { /* noop */ }}
        onSaved={() => { /* noop */ }}
        state={{
          status: "ok",
          executionId: null,
          result: {
            columns: [{ name: "offer_id", name_lower: "offer_id", declared_type: "INT4", nullable: false }],
            rows: [[{ type: "Int", value: 5002 }]],
            truncated: false,
            elapsed_ms: 3,
          },
          error: null,
        }}
        onRun={onRun}
        onCancel={() => { /* noop */ }}
      />,
    );

    // A grade e a barra de execução aparecem dentro do mesmo <section>
    // do editor — não num card separado em outro lugar da página.
    expect(screen.getByRole("region", { name: "Bloco de SQL ad-hoc" })).toBeInTheDocument();
    expect(screen.getByText("5002")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^executar$/i }));
    expect(onRun).toHaveBeenCalled();
  });
});
