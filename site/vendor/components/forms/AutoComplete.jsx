import React from "react";

/**
 * Tasty AutoComplete — a free-text trigger (Input language) with a floating
 * candidate dropdown (menu-container + MenuItem language). This is a proper
 * typeahead: typing narrows the candidate list; it is NOT a closed picker
 * (that is Select). Backs both address bars (Explorer / Markdown) as the
 * candidate dropdown behind the shared PathField.
 *
 * - `icon`        — leading icon slot on the trigger (per-surface: folderOpen / file).
 * - `rowIcon`     — icon slot on every candidate row (per-surface injection).
 * - `items`       — candidate list: strings or { value, label, icon }.
 * - `mono`        — monospace path variant (path candidates render middle-ellipsis).
 * - `withGo`      — trailing Go (arrow-right) button; the address-bar affordance.
 * - `match`       — filter mode: "substring" (default, path-friendly) | "prefix" | "none".
 * - `highlight`   — highlight the matched run in each row (default true).
 * - `maxDropdownHeight` — px cap; the list scrolls internally past it (shrink-to-fit below).
 * - `emptyLabel`  — muted row shown when there is no match / no candidates.
 *
 * Controlled hooks for specimens/tests: `open`, `activeIndex`, `query`,
 * `hoverIndex` (forces a row's pointer-hover wash). Uncontrolled otherwise:
 * focus opens, typing filters, ↑/↓ move the keyboard-active row, ↵ selects,
 * Esc closes/reverts.
 */

const GO_GLYPH = (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M5 12h14M13 6l6 6-6 6" />
  </svg>
);

const CSS = `
.tasty-ac { position: relative; display: inline-flex; align-items: center; gap: var(--tasty-space-sm); }
.tasty-ac--block { display: flex; width: 100%; }
.tasty-ac__field {
  flex: 1; min-width: 0;
  display: inline-flex; align-items: center; gap: var(--tasty-space-sm);
  height: var(--tasty-control-height);
  padding: 0 var(--tasty-space-md);
  border: var(--tasty-border-width) solid var(--tasty-input-border);
  border-radius: var(--tasty-input-radius);
  background: var(--tasty-input-bg);
  color: var(--tasty-text-primary);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-ac__field--open { border-color: var(--tasty-input-border-focus); box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-border-focus); }
.tasty-ac--mono .tasty-ac__field input { font-family: var(--tasty-font-mono); font-size: var(--tasty-font-size-caption); }
.tasty-ac__field input {
  flex: 1; min-width: 0; border: 0; outline: 0; padding: 0; margin: 0;
  background: transparent; color: inherit; font: inherit;
}
.tasty-ac__field input::placeholder { color: var(--tasty-input-placeholder); }
.tasty-ac__icon { display: inline-flex; flex: none; color: var(--tasty-input-icon-fg); }
.tasty-ac__icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-ac__go { flex: none; }

.tasty-ac__pop {
  position: absolute; left: 0; top: calc(100% + var(--tasty-space-xs)); z-index: 40;
  min-width: 100%; box-sizing: border-box;
  background: var(--tasty-autocomplete-menu-bg);
  border: var(--tasty-border-width) solid var(--tasty-autocomplete-menu-border);
  border-radius: var(--tasty-autocomplete-menu-radius);
  box-shadow: var(--tasty-autocomplete-menu-shadow);
  padding: var(--tasty-space-xs);
  overflow: hidden;
}
.tasty-ac__list {
  max-height: var(--tasty-autocomplete-max-height);
  overflow-y: auto; overflow-x: hidden;
  display: flex; flex-direction: column;
}
.tasty-ac__row {
  display: flex; align-items: center; gap: var(--tasty-space-sm);
  height: var(--tasty-autocomplete-row-height);
  padding: 0 var(--tasty-autocomplete-row-padding-x);
  border-radius: var(--tasty-menu-item-radius);
  color: var(--tasty-autocomplete-row-fg);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  cursor: pointer; user-select: none;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-ac--mono .tasty-ac__row { font-family: var(--tasty-font-mono); font-size: var(--tasty-font-size-caption); }
.tasty-ac__row:hover { background: var(--tasty-autocomplete-row-bg-hover); color: var(--tasty-autocomplete-row-fg-active); }
.tasty-ac__row--active,
.tasty-ac__row--active:hover { background: var(--tasty-autocomplete-row-bg-active); color: var(--tasty-autocomplete-row-fg-active); }
.tasty-ac__row-icon { display: inline-flex; flex: none; color: var(--tasty-text-secondary); }
.tasty-ac__row-icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-ac__row-label { flex: 1; min-width: 0; display: flex; align-items: center; overflow: hidden; }
.tasty-ac__row-head { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tasty-ac__row-tail { flex: none; white-space: nowrap; }
.tasty-ac__mark { color: var(--tasty-autocomplete-match-fg); font-weight: var(--tasty-font-weight-semibold); }
.tasty-ac__row--active .tasty-ac__row-icon, .tasty-ac__row--active .tasty-ac__mark { color: var(--tasty-autocomplete-part-fg-active); }
.tasty-ac__row--active .tasty-ac__mark { font-weight: inherit; text-decoration: underline; text-decoration-thickness: var(--tasty-autocomplete-match-underline-width); text-underline-offset: var(--tasty-autocomplete-match-underline-offset); }
.tasty-ac__empty {
  height: var(--tasty-autocomplete-row-height);
  display: flex; align-items: center; padding: 0 var(--tasty-autocomplete-row-padding-x);
  color: var(--tasty-autocomplete-empty-fg);
  font-family: var(--tasty-font-ui); font-size: var(--tasty-font-size-caption);
}
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-autocomplete-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

function norm(item) {
  return typeof item === "string" ? { value: item, label: item } : item;
}

function filterItems(items, query, match) {
  if (!query || match === "none") return items;
  const q = query.toLowerCase();
  return items.filter((it) => {
    const l = norm(it).label.toLowerCase();
    return match === "prefix" ? l.startsWith(q) : l.includes(q);
  });
}

// Highlight the first case-insensitive run of `query` inside `text`.
function highlightRun(text, query) {
  if (!query) return text;
  const i = text.toLowerCase().indexOf(query.toLowerCase());
  if (i < 0) return text;
  return [
    text.slice(0, i),
    <span key="m" className="tasty-ac__mark">{text.slice(i, i + query.length)}</span>,
    text.slice(i + query.length),
  ];
}

// Middle-ellipsis for paths: keep the filename tail, ellipsize the parent dirs.
function RowLabel({ label, query, mono, highlight }) {
  const marked = highlight ? (t) => highlightRun(t, query) : (t) => t;
  const slash = mono ? label.lastIndexOf("/") : -1;
  if (slash > 0 && slash < label.length - 1) {
    const head = label.slice(0, slash + 1);
    const tail = label.slice(slash + 1);
    return (
      <span className="tasty-ac__row-label">
        <span className="tasty-ac__row-head">{marked(head)}</span>
        <span className="tasty-ac__row-tail">{marked(tail)}</span>
      </span>
    );
  }
  return <span className="tasty-ac__row-label"><span className="tasty-ac__row-head">{marked(label)}</span></span>;
}

export function AutoComplete({
  icon = null,
  rowIcon = null,
  items = [],
  mono = false,
  block = false,
  withGo = false,
  match = "substring",
  highlight = true,
  maxDropdownHeight = null,
  emptyLabel = "No matches",
  placeholder = "",
  // controlled hooks (specimens/tests)
  open,
  activeIndex,
  query,
  hoverIndex = null,
  defaultValue = "",
  onSelect,
  onGo,
  className = "",
  ...rest
}) {
  ensureCss();
  const isOpenControlled = open !== undefined;
  const isActiveControlled = activeIndex !== undefined;
  const isQueryControlled = query !== undefined;

  const [text, setText] = React.useState(defaultValue);
  const [openS, setOpenS] = React.useState(false);
  const [activeS, setActiveS] = React.useState(0);

  const q = isQueryControlled ? query : text;
  const isOpen = isOpenControlled ? open : openS;
  const filtered = filterItems(items, q, match);
  const active = isActiveControlled ? activeIndex : Math.min(activeS, Math.max(0, filtered.length - 1));

  const commit = (it) => {
    const n = norm(it);
    if (!isQueryControlled) setText(n.value);
    if (!isOpenControlled) setOpenS(false);
    onSelect && onSelect(n.value);
  };

  const onKeyDown = (e) => {
    if (isOpenControlled || isActiveControlled) return;
    if (e.key === "ArrowDown") { e.preventDefault(); setOpenS(true); setActiveS((i) => Math.min(i + 1, filtered.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActiveS((i) => Math.max(i - 1, 0)); }
    else if (e.key === "Enter") { if (filtered[active]) commit(filtered[active]); onGo && onGo(text); }
    else if (e.key === "Escape") { setOpenS(false); }
  };

  const cls = ["tasty-ac", block ? "tasty-ac--block" : "", mono ? "tasty-ac--mono" : "", className].filter(Boolean).join(" ");
  const listStyle = maxDropdownHeight != null ? { maxHeight: typeof maxDropdownHeight === "number" ? maxDropdownHeight + "px" : maxDropdownHeight } : undefined;

  return (
    <div className={cls}>
      <div className={"tasty-ac__field" + (isOpen ? " tasty-ac__field--open" : "")}>
        {icon && <span className="tasty-ac__icon">{icon}</span>}
        <input
          value={isQueryControlled ? query : text}
          placeholder={placeholder}
          onChange={(e) => { if (!isQueryControlled) { setText(e.target.value); if (!isOpenControlled) setOpenS(true); } }}
          onFocus={() => { if (!isOpenControlled) setOpenS(true); }}
          onKeyDown={onKeyDown}
          readOnly={isQueryControlled}
          {...rest}
        />
      </div>
      {withGo && (
        <button type="button" className="tasty-ac__go" aria-label="Go"
          onClick={() => (onGo ? onGo(isQueryControlled ? query : text) : null)}
          style={{ all: "unset", display: "inline-flex", alignItems: "center", justifyContent: "center",
            width: "var(--tasty-control-height)", height: "var(--tasty-control-height)", flex: "none",
            borderRadius: "var(--tasty-radius)", cursor: "pointer", color: "var(--tasty-accent-primary)" }}>
          {GO_GLYPH}
        </button>
      )}
      {isOpen && (
        <div className="tasty-ac__pop" role="listbox">
          <div className="tasty-ac__list tasty-scroll" style={listStyle}>
            {filtered.length === 0 ? (
              <div className="tasty-ac__empty">{emptyLabel}</div>
            ) : (
              filtered.map((it, i) => {
                const n = norm(it);
                const rowCls = "tasty-ac__row" + (i === active ? " tasty-ac__row--active" : "");
                const forcedHover = i === hoverIndex && i !== active
                  ? { background: "var(--tasty-autocomplete-row-bg-hover)", color: "var(--tasty-autocomplete-row-fg-active)" } : undefined;
                return (
                  <div key={n.value + i} className={rowCls} role="option" aria-selected={i === active}
                    style={forcedHover} onMouseDown={(e) => { e.preventDefault(); commit(it); }}>
                    {(n.icon || rowIcon) && <span className="tasty-ac__row-icon">{n.icon || rowIcon}</span>}
                    <RowLabel label={n.label} query={q} mono={mono} highlight={highlight} />
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}
