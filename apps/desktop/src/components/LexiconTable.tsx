import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";
import { useMemo, useState } from "react";
import type { LexiconEntry } from "../contracts";

const ROW_HEIGHT = 54;
const VIEWPORT_HEIGHT = 520;
const OVERSCAN = 8;

export function LexiconTable({ entries }: { entries: LexiconEntry[] }) {
  const columns = useMemo<ColumnDef<LexiconEntry>[]>(
    () => [
      { accessorKey: "name", header: "SIGN", cell: (info) => <strong>{String(info.getValue())}</strong> },
      { accessorKey: "underlying_form", header: "UR", cell: (info) => info.getValue<string>() ?? "—" },
      { accessorKey: "gloss", header: "GLOSS", cell: (info) => info.getValue<string>() ?? "—" },
      {
        accessorKey: "categories",
        header: "CATEGORIES",
        cell: (info) => <div className="tag-list">{info.getValue<string[]>().map((item) => <span key={item}>{item}</span>)}</div>,
      },
    ],
    [],
  );
  const table = useReactTable({ data: entries, columns, getCoreRowModel: getCoreRowModel() });
  const rows = table.getRowModel().rows;
  const [scrollTop, setScrollTop] = useState(0);
  const virtualized = rows.length > 100;
  const start = virtualized
    ? Math.min(rows.length - 1, Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN))
    : 0;
  const end = virtualized
    ? Math.min(rows.length, Math.ceil((scrollTop + VIEWPORT_HEIGHT) / ROW_HEIGHT) + OVERSCAN)
    : rows.length;
  const visibleRows = rows.slice(start, end);

  return (
    <div className="table-scroll" onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}>
      <table className="data-table">
        <thead>{table.getHeaderGroups().map((group) => <tr key={group.id}>{group.headers.map((header) => <th key={header.id}>{flexRender(header.column.columnDef.header, header.getContext())}</th>)}</tr>)}</thead>
        <tbody>
          {virtualized && start > 0 && <tr className="virtual-spacer"><td colSpan={columns.length} style={{ height: start * ROW_HEIGHT }} /></tr>}
          {visibleRows.map((row) => <tr key={row.id} style={virtualized ? { height: ROW_HEIGHT } : undefined}>{row.getVisibleCells().map((cell) => <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>)}</tr>)}
          {virtualized && end < rows.length && <tr className="virtual-spacer"><td colSpan={columns.length} style={{ height: (rows.length - end) * ROW_HEIGHT }} /></tr>}
        </tbody>
      </table>
      {!entries.length && <div className="empty-state">No matching entries</div>}
    </div>
  );
}
