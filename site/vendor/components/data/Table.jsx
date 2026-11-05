import React from "react";
import { Icon } from "../core/Icon";

/**
 * Tasty Table — a dense, monospace-friendly data table for system data
 * (ports, processes, sessions, keybindings). Column-config driven, with
 * per-column alignment + mono, optional sortable headers, row hover, a
 * selected state, and a sticky header for scroll regions.
 *
 * Built on the same tokens as TreeRow/MenuItem: 4px grid, --tasty-bg-sidebar
 * header tone, --tasty-surface-active selection, --tasty-separator hairlines. Data
 * cells default to --tasty-font-mono so IDs/ports/addresses align by glyph.
 */

const CSS = `
.tasty-table { width: 100%; border-collapse: separate; border-spacing: 0; font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body); color: var(--tasty-text-secondary); }
.tasty-table__head th {
  position: sticky; top: 0; z-index: 1;
  height: var(--tasty-control-height);
  padding: 0 var(--tasty-space-md);
  background: var(--tasty-bg-sidebar);
  color: var(--tasty-text-muted);
  font-family: var(--tasty-font-ui); font-size: var(--tasty-table-header-font-size); /* 2026-10-06: UI font (was mono) — matches the app; caps tracking renders on it */ font-weight: var(--tasty-table-header-font-weight);
  text-transform: uppercase; letter-spacing: var(--tasty-table-header-tracking);
  text-align: left; white-space: nowrap; user-select: none;
  border-bottom: var(--tasty-border-width) solid var(--tasty-separator);
}
.tasty-table__head th.is-tight,
.tasty-table__row td.is-tight { padding: 0; text-align: center; }
.tasty-table__head th.is-right  { text-align: right; }
.tasty-table__head th.is-center { text-align: center; }
.tasty-table__head th.is-sortable { cursor: pointer; transition: color var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tasty-table__head th.is-sortable:hover { color: var(--tasty-text-secondary); }
.tasty-table__sort { display: inline-flex; vertical-align: middle; margin-left: var(--tasty-space-xs); width: var(--tasty-icon-size-xs); height: var(--tasty-icon-size-xs);
  color: var(--tasty-accent-primary); opacity: 0; transition: opacity var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tasty-table__head th.is-active .tasty-table__sort { opacity: 1; }
.tasty-table__head th.is-sortable:hover .tasty-table__sort { opacity: 0.5; }
.tasty-table__head th.is-active .tasty-table__sort { opacity: 1; }

.tasty-table__row { transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tasty-table__row td {
  height: var(--tasty-control-height);
  padding: 0 var(--tasty-space-md);
  border-bottom: var(--tasty-border-width) solid var(--tasty-separator);
  color: var(--tasty-text-secondary); white-space: nowrap;
  max-width: 0; overflow: hidden; text-overflow: ellipsis;
}
.tasty-table--dense .tasty-table__row td,
.tasty-table--dense .tasty-table__head th { height: var(--tasty-control-height-tree); }
.tasty-table__row td.is-right  { text-align: right; }
.tasty-table__row td.is-center { text-align: center; }
.tasty-table__row td.is-mono   { font-family: var(--tasty-font-mono); color: var(--tasty-text-primary); }
.tasty-table__row td.is-strong { color: var(--tasty-text-primary); }
.tasty-table--hover .tasty-table__row:hover { background: var(--tasty-overlay-hover); }
.tasty-table--hover .tasty-table__row:hover td { color: var(--tasty-text-secondary-raised); }
.tasty-table__row.is-selected { background: var(--tasty-surface-active); }
.tasty-table__row.is-selected td { color: var(--tasty-text-on-active); }
.tasty-table--hover .tasty-table__row.is-selected:hover { background: var(--tasty-surface-active); }
.tasty-table--hover .tasty-table__row.is-selected:hover td { color: var(--tasty-text-on-active-hover); }
.tasty-table__row.is-selected td .tasty-statusdot { color: var(--tasty-text-on-active); }
.tasty-table__row.is-clickable { cursor: pointer; }
.tasty-table__row:last-child td { border-bottom: 0; }

.tasty-table__empty { padding: var(--tasty-space-lg); text-align: center; color: var(--tasty-text-muted);
  font-size: var(--tasty-font-size-body); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-table-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

const SortIcon = ({ dir }) => (
  <span className="tasty-table__sort">
    <Icon name={dir === "asc" ? "chevronUp" : "chevronDown"} size="100%" />
  </span>
);

export function Table({
  columns = [],
  rows = [],
  rowKey,
  selectedKey = null,
  onRowClick,
  sort = null,
  onSort,
  dense = false,
  hover = true,
  empty = "No rows",
  className = "",
  ...rest
}) {
  ensureCss();
  const keyOf = (row, i) =>
    rowKey ? (typeof rowKey === "function" ? rowKey(row) : row[rowKey]) : i;
  const alignCls = (a) => (a === "right" ? " is-right" : a === "center" ? " is-center" : "");
  const cls = [
    "tasty-table",
    dense ? "tasty-table--dense" : "",
    hover ? "tasty-table--hover" : "",
    className,
  ].filter(Boolean).join(" ");

  return (
    <table className={cls} {...rest}>
      <thead className="tasty-table__head">
        <tr>
          {columns.map((c) => {
            const active = sort && sort.key === c.key;
            const thCls = [
              alignCls(c.align).trim(),
              c.tight ? "is-tight" : "",
              c.sortable ? "is-sortable" : "",
              active ? "is-active" : "",
            ].filter(Boolean).join(" ");
            return (
              <th
                key={c.key}
                className={thCls}
                style={c.width || c.minWidth ? { width: c.width, minWidth: c.minWidth ?? c.width } : undefined}
                onClick={c.sortable && onSort ? () => onSort(c.key) : undefined}
              >
                {c.header}
                {c.sortable && <SortIcon dir={active ? sort.dir : "desc"} />}
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td className="tasty-table__empty" colSpan={columns.length}>{empty}</td>
          </tr>
        )}
        {rows.map((row, i) => {
          const k = keyOf(row, i);
          const selected = selectedKey != null && k === selectedKey;
          const rowCls = [
            "tasty-table__row",
            selected ? "is-selected" : "",
            onRowClick ? "is-clickable" : "",
          ].filter(Boolean).join(" ");
          return (
            <tr key={k} className={rowCls} onClick={onRowClick ? () => onRowClick(row, k) : undefined}>
              {columns.map((c) => {
                const tdCls = [
                  alignCls(c.align).trim(),
                  c.tight ? "is-tight" : "",
                  c.mono ? "is-mono" : "",
                  c.strong ? "is-strong" : "",
                ].filter(Boolean).join(" ");
                const content = c.render ? c.render(row[c.key], row) : row[c.key];
                return (
                  <td key={c.key} className={tdCls} title={typeof content === "string" ? content : undefined}>
                    {content}
                  </td>
                );
              })}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
