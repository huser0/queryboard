import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ResultGrid } from "./ResultGrid";
import type { ResultSet } from "../ipc/client";

function makeResult(overrides: Partial<ResultSet> = {}): ResultSet {
  return {
    columns: [
      { name: "OFFER_ID", name_lower: "offer_id", declared_type: "NUMBER", nullable: false },
      { name: "PRECO", name_lower: "preco", declared_type: "NUMERIC", nullable: true },
    ],
    rows: [],
    truncated: false,
    elapsed_ms: 12,
    ...overrides,
  };
}

describe("ResultGrid", () => {
  it("renders decimal cells fully, never through Number() (formatted for pt-BR, no digit lost)", () => {
    const result = makeResult({
      rows: [
        [
          { type: "Int", value: 1 },
          // 30 dígitos — se isso passasse por Number(), viraria notação
          // científica e perderia precisão. Formatação (separador de
          // milhar, vírgula decimal) é por manipulação de string, então
          // todo dígito continua presente na tela, só reagrupado.
          { type: "Decimal", value: "123456789012345678901234567890.5" },
        ],
      ],
    });

    render(<ResultGrid result={result} />);

    expect(
      screen.getByText("123.456.789.012.345.678.901.234.567.890,5"),
    ).toBeInTheDocument();
  });

  it("preserves the original column header casing", () => {
    const result = makeResult({
      rows: [[{ type: "Int", value: 1 }, { type: "Null" }]],
    });

    render(<ResultGrid result={result} />);

    expect(screen.getByText("OFFER_ID")).toBeInTheDocument();
    expect(screen.getByText("PRECO")).toBeInTheDocument();
    expect(screen.queryByText("offer_id")).not.toBeInTheDocument();
  });

  it("renders null cells with a distinct marker, not empty and not the string 'null'", () => {
    const result = makeResult({
      rows: [[{ type: "Int", value: 1 }, { type: "Null" }]],
    });

    render(<ResultGrid result={result} />);

    expect(screen.queryByText("null")).not.toBeInTheDocument();
    expect(screen.getByLabelText("valor nulo")).toHaveTextContent("—");
  });

  it("shows a message instead of an empty table when there are no rows", () => {
    render(<ResultGrid result={makeResult({ rows: [] })} />);
    expect(screen.getByText("Nenhuma linha retornada.")).toBeInTheDocument();
  });

  it("renders a large row set without crashing (virtualization active)", () => {
    const rows = Array.from({ length: 1000 }, (_, i) => [
      { type: "Int" as const, value: i },
      { type: "Text" as const, value: `linha ${String(i)}` },
    ]);
    render(<ResultGrid result={makeResult({ rows })} />);
    // Com virtualização, nem todas as 1000 linhas são montadas no DOM —
    // só confirmamos que o componente renderiza sem estourar e mostra
    // pelo menos as primeiras linhas.
    expect(screen.getByText("linha 0")).toBeInTheDocument();
  });
});
