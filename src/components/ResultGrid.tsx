import { useRef } from "react";
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  createColumnHelper,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { CellValue, ResultSet } from "../ipc/client";

const columnHelper = createColumnHelper<CellValue[]>();

function renderCell(cell: CellValue): string {
  if (cell.type === "Null") {
    return "";
  }
  if (cell.type === "Bool") {
    return cell.value ? "true" : "false";
  }
  if (cell.type === "Bytes") {
    return `<${cell.value.length.toString()} bytes>`;
  }
  return String(cell.value);
}

// Decimal nunca deve virar Number() — CLAUDE.md D6, string canônica.
function isNumericLike(cell: CellValue): boolean {
  return cell.type === "Int" || cell.type === "Decimal" || cell.type === "Float";
}

interface ResultGridProps {
  result: ResultSet;
}

export function ResultGrid({ result }: ResultGridProps) {
  const parentRef = useRef<HTMLDivElement>(null);

  const columns = result.columns.map((col, idx) =>
    columnHelper.accessor((row) => row[idx], {
      id: col.name_lower,
      header: col.name_lower,
    }),
  );

  const table = useReactTable({
    data: result.rows,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  const rows = table.getRowModel().rows;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
  });

  if (result.rows.length === 0) {
    return <p>Nenhuma linha retornada.</p>;
  }

  // Virtualização precisa de posicionamento absoluto por linha — isso não
  // funciona de forma confiável em `<tr>` (o modelo de caixa `table-row`
  // ignora/quebra `position: absolute` de formas inconsistentes entre
  // navegadores, o que fazia a linha "escapar" do fluxo da tabela). Por
  // isso a grade usa `div`s com `role` de tabela em vez de `<table>` real.
  const columnCount = table.getAllLeafColumns().length;
  const gridTemplateColumns = `repeat(${columnCount.toString()}, minmax(140px, 1fr))`;

  return (
    <div ref={parentRef} className="result-grid" role="table">
      <div className="result-grid__header" role="rowgroup">
        {table.getHeaderGroups().map((headerGroup) => (
          <div
            key={headerGroup.id}
            className="result-grid__row"
            role="row"
            style={{ gridTemplateColumns }}
          >
            {headerGroup.headers.map((header) => (
              <div key={header.id} className="result-grid__cell" role="columnheader">
                {flexRender(header.column.columnDef.header, header.getContext())}
              </div>
            ))}
          </div>
        ))}
      </div>
      <div
        className="result-grid__body"
        role="rowgroup"
        style={{ height: `${virtualizer.getTotalSize().toString()}px` }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index];
          if (row === undefined) {
            return null;
          }
          return (
            <div
              key={row.id}
              className="result-grid__row result-grid__row--body"
              role="row"
              style={{
                gridTemplateColumns,
                transform: `translateY(${virtualRow.start.toString()}px)`,
              }}
            >
              {row.getVisibleCells().map((cell) => {
                const value = cell.getValue<CellValue>();
                return (
                  <div
                    key={cell.id}
                    className="result-grid__cell"
                    role="cell"
                    data-numeric={isNumericLike(value) ? "true" : undefined}
                  >
                    {renderCell(value)}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
