import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Board } from "../Board";
import { client } from "../../../ipc/client";
import type { ConnectionSummary, QuerySummary } from "../../../ipc/types";

vi.mock("../../../ipc/client", () => ({
  client: {
    queryRun: vi.fn(),
    queryRunAdhoc: vi.fn(),
    queryExtractParams: vi.fn().mockResolvedValue([]),
    queryCreate: vi.fn(),
    queryUpdate: vi.fn(),
    queryDelete: vi.fn(),
    queryCancel: vi.fn(),
  },
}));

// CodeMirror não digita de forma confiável no jsdom (getClientRects
// ausente) — trocado por um stand-in leve nos testes que precisam
// preencher o editor via `user.type`. `AdhocQueryBlock.test.tsx` já
// cobre a integração real com o CodeMirror sem depender de digitação.
vi.mock("../../queries/QueryEditor", () => ({
  QueryEditor: ({
    value,
    onChange,
  }: {
    value: string;
    onChange: (value: string) => void;
  }) => (
    <textarea
      aria-label="Editor de SQL"
      value={value}
      onChange={(e) => { onChange(e.currentTarget.value); }}
    />
  ),
}));

const connQueryboardLocal: ConnectionSummary = {
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

const queryA: QuerySummary = {
  id: "a",
  slug: "consulta_oferta",
  name: "Consulta oferta",
  connection_slug: "conn",
  sql: "SELECT 1",
  params: [{ name: "offer_id", type: "number", required: true }],
  created_at: "",
  updated_at: "",
};

const queryB: QuerySummary = {
  id: "b",
  slug: "consulta_relacao",
  name: "Consulta relação",
  connection_slug: "conn",
  sql: "SELECT 1",
  params: [
    { name: "offer_id", type: "number", required: true },
    { name: "product_id", type: "number", required: true },
  ],
  created_at: "",
  updated_at: "",
};

function emptyResult() {
  return { columns: [], rows: [], truncated: false, elapsed_ms: 1 };
}

describe("Board", () => {
  // Sem isso, chamadas de mock (ex. `queryCancel`) se acumulam entre
  // testes deste arquivo — cada teste deve começar com uma folha em
  // branco de call-count, e `queryExtractParams` deve voltar ao default
  // (`[]`) já que testes individuais sobrescrevem sua resolução.
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(client.queryExtractParams).mockResolvedValue([]);
  });

  it("shows no result cards and disables run-all with zero queries selected", () => {
    render(<Board queries={[queryA, queryB]} connections={[]} onQuerySaved={() => {}} onQueryDeleted={() => {}} activeConnectionSlug={null} />);
    expect(screen.queryByRole("region", { name: /painel de/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /executar tudo/i })).toBeDisabled();
  });

  it("merges shared param names into a single input and runs each selected query with its own subset", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryRun).mockResolvedValue(emptyResult());
    render(<Board queries={[queryA, queryB]} connections={[]} onQuerySaved={() => {}} onQueryDeleted={() => {}} activeConnectionSlug={null} />);

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));
    await user.click(screen.getByRole("checkbox", { name: /consulta relação/i }));

    // offer_id aparece uma única vez (compartilhado), product_id só existe em B.
    expect(screen.getAllByLabelText("offer_id")).toHaveLength(1);
    expect(screen.getByLabelText("product_id")).toBeInTheDocument();

    await user.type(screen.getByLabelText("offer_id"), "5002");
    await user.type(screen.getByLabelText("product_id"), "101");
    await user.click(screen.getByRole("button", { name: /executar tudo/i }));

    await waitFor(() => {
      expect(client.queryRun).toHaveBeenCalledTimes(2);
    });
    expect(client.queryRun).toHaveBeenCalledWith(
      expect.any(String),
      "consulta_oferta",
      { offer_id: "5002" },
    );
    expect(client.queryRun).toHaveBeenCalledWith(
      expect.any(String),
      "consulta_relacao",
      { offer_id: "5002", product_id: "101" },
    );
  });

  it("shows one query's error without blocking the other's result", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryRun).mockImplementation((_id, slug) => {
      if (slug === "consulta_oferta") {
        return Promise.reject(new Error("conexão indisponível"));
      }
      return Promise.resolve(emptyResult());
    });
    render(<Board queries={[queryA, queryB]} connections={[]} onQuerySaved={() => {}} onQueryDeleted={() => {}} activeConnectionSlug={null} />);

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));
    await user.click(screen.getByRole("checkbox", { name: /consulta relação/i }));
    await user.type(screen.getByLabelText("offer_id"), "5001");
    await user.type(screen.getByLabelText("product_id"), "1");
    await user.click(screen.getByRole("button", { name: /executar tudo/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("conexão indisponível");
    expect(await screen.findByText(/nenhuma linha retornada/i)).toBeInTheDocument();
  });

  it("cancels only the running query when cancel-all is clicked", async () => {
    const user = userEvent.setup();
    let resolveA: (() => void) | undefined;
    vi.mocked(client.queryRun).mockImplementation((_id, slug) => {
      if (slug === "consulta_oferta") {
        return new Promise((resolve) => {
          resolveA = () => { resolve(emptyResult()); };
        });
      }
      return Promise.resolve(emptyResult());
    });
    render(<Board queries={[queryA, queryB]} connections={[]} onQuerySaved={() => {}} onQueryDeleted={() => {}} activeConnectionSlug={null} />);

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));
    await user.click(screen.getByRole("checkbox", { name: /consulta relação/i }));
    await user.type(screen.getByLabelText("offer_id"), "5001");
    await user.type(screen.getByLabelText("product_id"), "1");
    await user.click(screen.getByRole("button", { name: /executar tudo/i }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /cancelar tudo/i })).toBeEnabled();
    });
    await user.click(screen.getByRole("button", { name: /cancelar tudo/i }));

    expect(client.queryCancel).toHaveBeenCalledTimes(1);
    await act(() => {
      resolveA?.();
      return Promise.resolve();
    });
  });

  it("merges a saved query's params with an ad-hoc block's and runs both, saved via queryRun and ad-hoc via queryRunAdhoc", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryRun).mockResolvedValue(emptyResult());
    vi.mocked(client.queryRunAdhoc).mockResolvedValue(emptyResult());
    vi.mocked(client.queryExtractParams).mockResolvedValue(["offer_id"]);

    render(
      <Board
        queries={[queryA]}
        connections={[connQueryboardLocal]}
        onQuerySaved={() => {}}
        onQueryDeleted={() => {}}
        activeConnectionSlug={null}
      />,
    );

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));

    await user.click(screen.getByRole("button", { name: /adicionar sql ad-hoc/i }));
    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");
    await user.type(
      screen.getByLabelText("Editor de SQL"),
      "SELECT * FROM t WHERE id = :offer_id",
    );

    // Espera a extração debounced do bloco ad-hoc terminar de verdade
    // (não só a contagem de labels merged, que já bateria 1 só com o
    // offer_id da query salva) antes de preencher e rodar.
    await waitFor(() => {
      expect(client.queryExtractParams).toHaveBeenCalledWith(
        "SELECT * FROM t WHERE id = :offer_id",
        "queryboard_local",
      );
    });
    // offer_id é compartilhado entre a query salva e o bloco ad-hoc —
    // deve aparecer uma única vez no formulário mesclado.
    await waitFor(() => {
      expect(screen.getAllByLabelText("offer_id")).toHaveLength(1);
    });
    await user.type(screen.getByLabelText("offer_id"), "5002");
    await user.click(screen.getByRole("button", { name: /^executar tudo$/i }));

    await waitFor(() => {
      expect(client.queryRun).toHaveBeenCalledWith(
        expect.any(String),
        "consulta_oferta",
        { offer_id: "5002" },
      );
      expect(client.queryRunAdhoc).toHaveBeenCalledWith(
        expect.any(String),
        "queryboard_local",
        "SELECT * FROM t WHERE id = :offer_id",
        { offer_id: "5002" },
      );
    });
  });

  it("isolates an ad-hoc block's error from a saved query's result", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryRun).mockResolvedValue(emptyResult());
    vi.mocked(client.queryRunAdhoc).mockRejectedValue(new Error("SQL inválida"));

    render(
      <Board
        queries={[queryA]}
        connections={[connQueryboardLocal]}
        onQuerySaved={() => {}}
        onQueryDeleted={() => {}}
        activeConnectionSlug={null}
      />,
    );

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));
    await user.click(screen.getByRole("button", { name: /adicionar sql ad-hoc/i }));
    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");
    await user.type(screen.getByLabelText("Editor de SQL"), "SELECT 1");
    await user.type(screen.getByLabelText("offer_id"), "5001");
    await user.click(screen.getByRole("button", { name: /^executar tudo$/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("SQL inválida");
    expect(await screen.findByText(/nenhuma linha retornada/i)).toBeInTheDocument();
  });

  it("cancels an ad-hoc block using its own execution_id when cancel-all is clicked", async () => {
    const user = userEvent.setup();
    let resolveAdhoc: (() => void) | undefined;
    vi.mocked(client.queryRunAdhoc).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveAdhoc = () => { resolve(emptyResult()); };
        }),
    );

    render(<Board queries={[]} connections={[connQueryboardLocal]} onQuerySaved={() => {}} onQueryDeleted={() => {}} activeConnectionSlug={null} />);

    await user.click(screen.getByRole("button", { name: /adicionar sql ad-hoc/i }));
    await user.selectOptions(screen.getByLabelText("Connection"), "queryboard_local");
    await user.type(screen.getByLabelText("Editor de SQL"), "SELECT 1");
    await user.click(screen.getByRole("button", { name: /^executar tudo$/i }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /cancelar tudo/i })).toBeEnabled();
    });
    await user.click(screen.getByRole("button", { name: /cancelar tudo/i }));

    expect(client.queryCancel).toHaveBeenCalledTimes(1);
    // O cancelamento usa o execution_id (uuid gerado por runById), nunca
    // o id estável do slot — o mock não expõe o uuid diretamente, mas a
    // asserção de "chamado uma vez" já prova que o dispatcher achou o
    // slot ad-hoc certo (não há query salva selecionada neste teste).
    await act(() => {
      resolveAdhoc?.();
      return Promise.resolve();
    });
  });

  it("defaults a new ad-hoc block's connection to activeConnectionSlug", async () => {
    const user = userEvent.setup();
    render(
      <Board
        queries={[]}
        connections={[connQueryboardLocal]}
        activeConnectionSlug="queryboard_local"
        onQuerySaved={() => {}}
        onQueryDeleted={() => {}}
      />,
    );

    await user.click(screen.getByRole("button", { name: /adicionar sql ad-hoc/i }));

    expect(screen.getByLabelText("Connection")).toHaveValue("queryboard_local");
  });

  it("deletes a saved query, drops it from selection/results, and notifies the parent", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryDelete).mockResolvedValue(undefined);
    const onQueryDeleted = vi.fn();

    render(
      <Board
        queries={[queryA, queryB]}
        connections={[]}
        activeConnectionSlug={null}
        onQuerySaved={() => {}}
        onQueryDeleted={onQueryDeleted}
      />,
    );

    await user.click(screen.getByRole("checkbox", { name: /consulta oferta/i }));
    const removeButton = screen.getAllByRole("button", { name: /^remover$/i })[0];
    if (removeButton === undefined) {
      throw new Error("expected a Remover button for consulta_oferta");
    }
    await user.click(removeButton);

    await waitFor(() => {
      expect(client.queryDelete).toHaveBeenCalledWith("consulta_oferta");
    });
    expect(onQueryDeleted).toHaveBeenCalled();
    // A query removida some da seleção — o painel de resultados não
    // deveria mais mostrar um card pra ela (só a fieldset de seleção
    // continua listando o que o pai (App) mandar via `queries` prop,
    // que num app real já teria sido atualizado por `onQueryDeleted`).
    expect(screen.queryByRole("checkbox", { name: /consulta oferta/i })).not.toBeChecked();
  });

  it("editing a saved query opens a pre-filled ad-hoc block whose Salvar calls queryUpdate, not queryCreate", async () => {
    const user = userEvent.setup();
    vi.mocked(client.queryUpdate).mockResolvedValue({ ...queryA, name: "Consulta oferta" });
    // Mesmo valor que o slot já nasce com (a extração debounced roda de
    // qualquer forma e não pode divergir do que foi pré-preenchido, ou
    // o teste fica dependente de timing).
    vi.mocked(client.queryExtractParams).mockResolvedValue(["offer_id"]);

    render(
      <Board
        queries={[queryA]}
        connections={[connQueryboardLocal]}
        activeConnectionSlug={null}
        onQuerySaved={() => {}}
        onQueryDeleted={() => {}}
      />,
    );

    await user.click(screen.getByRole("button", { name: /^editar$/i }));

    // O bloco ad-hoc novo já nasce com o SQL da query salva e o slug
    // travado (não é renomeável nesta versão).
    const slugInput = screen.getByLabelText("Slug para salvar");
    expect(slugInput).toHaveValue("consulta_oferta");
    expect(slugInput).toBeDisabled();
    expect(screen.getByRole("button", { name: /^atualizar$/i })).toBeInTheDocument();

    await waitFor(() => {
      expect(client.queryExtractParams).toHaveBeenCalledWith("SELECT 1", "conn");
    });
    await user.click(screen.getByRole("button", { name: /^atualizar$/i }));

    await waitFor(() => {
      expect(client.queryUpdate).toHaveBeenCalledWith("consulta_oferta", {
        slug: "consulta_oferta",
        name: "consulta_oferta",
        connection_slug: "conn",
        sql: "SELECT 1",
        params: [{ name: "offer_id", type: "string", required: true }],
      });
    });
    expect(client.queryCreate).not.toHaveBeenCalled();
  });
});
