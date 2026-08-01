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

  return (
    <div ref={parentRef} style={{ overflow: "auto", maxHeight: "480px" }}>
      <table>
        <thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <th key={header.id}>
                  {flexRender(header.column.columnDef.header, header.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody
          style={{ height: `${virtualizer.getTotalSize().toString()}px`, position: "relative" }}
        >
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const row = rows[virtualRow.index];
            if (row === undefined) {
              return null;
            }
            return (
              <tr
                key={row.id}
                style={{
                  position: "absolute",
                  top: 0,
                  transform: `translateY(${virtualRow.start.toString()}px)`,
                }}
              >
                {row.getVisibleCells().map((cell) => {
                  const value = cell.getValue<CellValue>();
                  return (
                    <td
                      key={cell.id}
                      style={
                        isNumericLike(value)
                          ? { textAlign: "right", fontFamily: "monospace" }
                          : undefined
                      }
                    >
                      {renderCell(value)}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
