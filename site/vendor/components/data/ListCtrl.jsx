import React from "react";
import { Icon } from "../core/Icon";

/**
 * Tasty ListCtrl — a full-width, row-selectable NAVIGATION list. Unlike
 * Table (a multi-column, sortable data grid), ListCtrl is a "pick one to
 * drill into" list: each row is a single item with a primary label, an
 * optional secondary description, an optional leading icon + trailing slot
 * (a Tag/Badge — e.g. an "Active" marker), and — by default — a trailing
 * chevron affordance signalling that clicking enters a detail view. Pair it
 * with DrillDown for the list → detail content swap.
 *
 * States: default / hover (--tasty-overlay-hover) / selected
 * (--tasty-surface-active) / disabled. Disabled = the ink rule (2026-09-29): every
 * glyph and line takes --tasty-state-disabled-fg, no chevron, no hover, not
 * selectable, no opacity; a trailing Tag / Badge drops to its disabled variant
 * (neutral box + the same ink — an accent fill never carries disabled ink).
 * Rows are divided by a hairline
 * (--tasty-separator); a per-row leading rail marks the selected row with
 * an accent bar (matching the sidebar/list idiom).
 */

const CSS = `
.tasty-listctrl { display: flex; flex-direction: column; width: 100%;
  font-family: var(--tasty-font-ui); font-size: var(--tasty-listctrl-font-size); }
.tasty-listctrl__row {
  display: flex; align-items: center; gap: var(--tasty-listctrl-row-gap);
  width: 100%; text-align: left; appearance: none; border: 0; background: transparent;
  min-height: var(--tasty-listctrl-row-min-height);
  padding: var(--tasty-listctrl-row-padding-y) var(--tasty-listctrl-row-padding-x);
  border-radius: var(--tasty-listctrl-radius);
  cursor: pointer; user-select: none; position: relative;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-listctrl--divided .tasty-listctrl__row:not(:last-child) {
  border-bottom: var(--tasty-border-width) solid var(--tasty-listctrl-divider); border-radius: 0; }
.tasty-listctrl__row:hover { background: var(--tasty-listctrl-row-bg-hover); }
.tasty-listctrl__row.is-selected { background: var(--tasty-listctrl-row-bg-selected);
  box-shadow: inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar); }
.tasty-listctrl__row.is-disabled { pointer-events: none; }
.tasty-listctrl__row.is-disabled, .tasty-listctrl__row.is-disabled * { color: var(--tasty-state-disabled-fg); }
.tasty-listctrl__row.is-disabled .tasty-tag { background: var(--tasty-tag-disabled-bg); border-color: var(--tasty-tag-disabled-border); color: var(--tasty-tag-disabled-fg); }
.tasty-listctrl__row.is-disabled .tasty-badge { background: var(--tasty-badge-disabled-bg); color: var(--tasty-badge-disabled-fg); }
.tasty-listctrl__icon { flex: none; display: inline-flex; color: var(--tasty-listctrl-icon-fg); }
.tasty-listctrl__icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-listctrl__text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.tasty-listctrl__label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  color: var(--tasty-listctrl-label-fg); }
.tasty-listctrl__row:hover .tasty-listctrl__label,
.tasty-listctrl__row.is-selected .tasty-listctrl__label { color: var(--tasty-listctrl-label-fg-active); }
.tasty-listctrl__desc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  font-size: var(--tasty-listctrl-desc-font-size); color: var(--tasty-listctrl-desc-fg); }
.tasty-listctrl__row.is-selected .tasty-listctrl__desc, .tasty-listctrl__row.is-selected .tasty-listctrl__icon, .tasty-listctrl__row.is-selected .tasty-listctrl__chev { color: var(--tasty-listctrl-part-fg-selected); }
.tasty-listctrl__trailing { flex: none; display: inline-flex; align-items: center; gap: var(--tasty-space-sm); }
.tasty-listctrl__chev { flex: none; display: inline-flex; width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); color: var(--tasty-listctrl-chevron-fg); }
.tasty-listctrl__chev svg { width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); display: block; }
.tasty-listctrl__row:hover .tasty-listctrl__icon, .tasty-listctrl__row:hover .tasty-listctrl__chev { color: var(--tasty-text-secondary); }
.tasty-listctrl__row:hover .tasty-listctrl__desc { color: var(--tasty-text-muted-raised); }
.tasty-listctrl__row:active .tasty-listctrl__label { color: var(--tasty-text-secondary-raised); }
.tasty-listctrl__row:active .tasty-listctrl__desc { color: var(--tasty-text-muted-raised); }
.tasty-listctrl__row:focus-visible .tasty-listctrl__label, .tasty-listctrl__row:focus-visible .tasty-listctrl__desc, .tasty-listctrl__row:focus-visible .tasty-listctrl__icon, .tasty-listctrl__row:focus-visible .tasty-listctrl__chev { color: var(--tasty-text-on-active); }
.tasty-listctrl__row.is-selected:hover .tasty-listctrl__label, .tasty-listctrl__row.is-selected:hover .tasty-listctrl__desc { color: var(--tasty-text-on-active-hover); }
.tasty-listctrl__row.is-selected:hover .tasty-listctrl__icon, .tasty-listctrl__row.is-selected:hover .tasty-listctrl__chev { color: var(--tasty-text-on-active); }
.tasty-listctrl__row.is-active { background: var(--tasty-listctrl-row-bg-selected); }
.tasty-listctrl__row.is-active .tasty-listctrl__label, .tasty-listctrl__row.is-active .tasty-listctrl__desc, .tasty-listctrl__row.is-active .tasty-listctrl__icon, .tasty-listctrl__row.is-active .tasty-listctrl__chev { color: var(--tasty-text-on-active); }
.tasty-listctrl__empty { padding: var(--tasty-space-lg); text-align: center;
  color: var(--tasty-text-muted); font-size: var(--tasty-font-size-body); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-listctrl-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function ListCtrl({
  items = [],
  selectedId = null,
  activeId = null,
  onSelect,
  chevron = true,
  divided = true,
  empty = "No items",
  className = "",
  ...rest
}) {
  ensureCss();
  const cls = ["tasty-listctrl", divided ? "tasty-listctrl--divided" : "", className]
    .filter(Boolean).join(" ");

  if (items.length === 0) {
    return (
      <div className={cls} {...rest}>
        <div className="tasty-listctrl__empty">{empty}</div>
      </div>
    );
  }

  return (
    <div className={cls} role="listbox" {...rest}>
      {items.map((it) => {
        const selected = selectedId != null && it.id === selectedId;
        const rowCls = [
          "tasty-listctrl__row",
          selected ? "is-selected" : "",
          activeId != null && it.id === activeId ? "is-active" : "",
          it.disabled ? "is-disabled" : "",
        ].filter(Boolean).join(" ");
        return (
          <button
            key={it.id}
            type="button"
            role="option"
            aria-selected={selected}
            aria-disabled={it.disabled || undefined}
            className={rowCls}
            onClick={it.disabled || !onSelect ? undefined : () => onSelect(it.id, it)}
          >
            {it.icon && <span className="tasty-listctrl__icon">{it.icon}</span>}
            <span className="tasty-listctrl__text">
              <span className="tasty-listctrl__label" title={typeof it.label === "string" ? it.label : undefined}>{it.label}</span>
              {it.description && <span className="tasty-listctrl__desc">{it.description}</span>}
            </span>
            {(it.trailing || (chevron && !it.disabled)) && (
              <span className="tasty-listctrl__trailing">
                {it.trailing}
                {chevron && !it.disabled && <span className="tasty-listctrl__chev"><Icon name="chevronRight" size="100%" /></span>}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
