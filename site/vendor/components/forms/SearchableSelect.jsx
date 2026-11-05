import React from "react";
import { Icon } from "../core/Icon";
import { MenuItem } from "../navigation/MenuItem.jsx";
import { Spinner } from "../feedback/Spinner.jsx";

/**
 * Tasty SearchableSelect (2026-10-19) — a Select whose OPEN list carries a search
 * field. Closed it IS a Select: same 28 trigger, 12 left / 28 chevron room, borders,
 * chevron. Open: menu popover (menu-bg · menu-border · menu-radius) with
 *   search field (inset 4) · separator · [pinned rows · separator] · option rows (MenuItem)
 * Rows are the shared MenuItem option row (selected = ink + trailing check, no fill).
 *
 * Decisions:
 *  1. Opening focuses the search field. The field draws border-focus as its EDGE only
 *     (no ring) — the trigger already carries the open ring.
 *  2. Keyboard cursor: ↑/↓ move the active row over visible enabled rows, Enter commits
 *     and closes, Esc closes and returns focus to the trigger. Focus stays in the field.
 *  3. The query is cleared every time the list closes.
 *  4. Search stays IN the list (not a combobox trigger) so the trigger never changes
 *     shape and the current value is never overwritten by typing.
 *  5. `pinned` options (e.g. "Default (monospace)") are always shown on top, ignore the
 *     query, and sit above their own separator.
 *  Match = case-insensitive substring on the label.
 *
 * Specimen hooks: `open`, `query`, `activeIndex`, `hoverIndex`, `focusSearch`.
 */

const CSS = `
.tasty-sselect { position: relative; display: inline-flex; flex-direction: column; }
.tasty-sselect--block { display: flex; width: 100%; }
.tasty-sselect__trigger {
  all: unset; box-sizing: border-box; display: flex; align-items: center; width: 100%;
  height: var(--tasty-select-height);
  padding: 0 var(--tasty-select-chevron-room) 0 var(--tasty-select-padding-x);
  border: var(--tasty-border-width) solid var(--tasty-select-border);
  border-radius: var(--tasty-select-radius);
  background: var(--tasty-select-bg); color: var(--tasty-select-fg);
  font-family: var(--tasty-font-ui); font-size: var(--tasty-select-font-size);
  cursor: pointer;
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui), box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-sselect__trigger:hover, .tasty-sselect__trigger[data-hover="true"] { border-color: var(--tasty-select-search-trigger-border-hover); }
.tasty-sselect__trigger:focus-visible, .tasty-sselect__trigger[data-open="true"] {
  border-color: var(--tasty-select-border-focus);
  box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-select-border-focus);
}
.tasty-sselect__trigger[data-disabled="true"] { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); box-shadow: none; cursor: default; }
.tasty-sselect__trigger[data-disabled="true"], .tasty-sselect__trigger[data-disabled="true"] * { color: var(--tasty-state-disabled-fg); }
.tasty-sselect__value { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
.tasty-sselect__value[data-empty="true"] { color: var(--tasty-select-search-value-placeholder); }
.tasty-sselect__chevron {
  position: absolute; right: var(--tasty-select-chevron-offset); top: calc(var(--tasty-select-height) / 2);
  transform: translateY(-50%); pointer-events: none; display: inline-flex; color: var(--tasty-select-chevron-fg);
  transition: transform var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-sselect__chevron[data-open="true"] { transform: translateY(-50%) rotate(180deg); }
.tasty-sselect__chevron[data-disabled="true"] { color: var(--tasty-state-disabled-fg); }
.tasty-sselect__menu {
  position: absolute; left: 0; top: calc(100% + var(--tasty-select-search-menu-gap)); z-index: 40;
  min-width: 100%; max-width: var(--tasty-select-search-menu-max-width); box-sizing: border-box;
  max-height: var(--tasty-select-search-list-max-height);
  display: flex; flex-direction: column;
  background: var(--tasty-menu-bg); border: var(--tasty-border-width) solid var(--tasty-menu-border);
  border-radius: var(--tasty-menu-radius); box-shadow: var(--tasty-shadow-popover);
  padding: var(--tasty-select-search-inset); overflow: hidden;
}
.tasty-sselect__field {
  flex: none; display: flex; align-items: center; box-sizing: border-box;
  height: var(--tasty-select-search-field-height);
  padding: 0 var(--tasty-select-search-field-padding-x); gap: var(--tasty-select-search-field-gap);
  background: var(--tasty-select-search-field-bg);
  border: var(--tasty-border-width) solid var(--tasty-select-search-field-border);
  border-radius: var(--tasty-radius);
}
.tasty-sselect__field[data-focus="true"], .tasty-sselect__field:focus-within { border-color: var(--tasty-select-search-field-border-focus); }
.tasty-sselect__field-icon { flex: none; display: inline-flex; color: var(--tasty-select-search-icon-fg); }
.tasty-sselect__field input {
  all: unset; flex: 1; min-width: 0; height: 100%;
  font-family: var(--tasty-font-ui); font-size: var(--tasty-select-font-size); color: var(--tasty-select-search-field-fg);
}
.tasty-sselect__field input::placeholder { color: var(--tasty-select-search-placeholder); }
.tasty-sselect__sep { flex: none; height: var(--tasty-border-width); margin: var(--tasty-space-xs) 0; background: var(--tasty-menu-separator); }
.tasty-sselect__list { flex: 1 1 auto; min-height: 0; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; }
.tasty-sselect__list > .tasty-menuitem, .tasty-sselect__pinned > .tasty-menuitem { flex: none; }
.tasty-sselect__pinned { flex: none; display: flex; flex-direction: column; }
.tasty-sselect__status {
  flex: none; display: flex; align-items: center; gap: var(--tasty-space-sm);
  height: var(--tasty-menu-item-height); padding: 0 var(--tasty-menu-item-padding-x);
  font-family: var(--tasty-font-ui); font-size: var(--tasty-select-font-size);
  color: var(--tasty-select-search-status-fg); cursor: default; user-select: none; white-space: nowrap;
  overflow: hidden; text-overflow: ellipsis;
}
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-searchable-select-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

function norm(o) {
  return typeof o === "string" ? { value: o, label: o, disabled: false } : { disabled: false, ...o };
}

export function filterOptions(items, query) {
  const q = (query || "").trim().toLowerCase();
  if (!q) return items;
  return items.filter((o) => String(o.label).toLowerCase().includes(q));
}

export function SearchableSelect({
  options = [],
  pinned = [],
  value,
  defaultValue = null,
  onChange,
  placeholder = "Select…",
  searchPlaceholder = "Search",
  emptyLabel = "No matches",
  loading = false,
  loadingLabel = "Loading…",
  block = false,
  disabled = false,
  open,
  query,
  activeIndex,
  hoverIndex = null,
  focusSearch = false,
  hoverTrigger = false,
  className = "",
  style,
  ...rest
}) {
  ensureCss();
  const items = options.map(norm);
  const pins = pinned.map(norm);
  const isValueControlled = value !== undefined;
  const isOpenControlled = open !== undefined;
  const [valueS, setValueS] = React.useState(defaultValue);
  const [openS, setOpenS] = React.useState(false);
  const [queryS, setQueryS] = React.useState("");
  const [activeS, setActiveS] = React.useState(-1);
  const rootRef = React.useRef(null);
  const triggerRef = React.useRef(null);
  const inputRef = React.useRef(null);

  const current = isValueControlled ? value : valueS;
  const isOpen = isOpenControlled ? open : openS;
  const q = query !== undefined ? query : queryS;
  const shown = filterOptions(items, q);
  const rows = [...pins, ...shown];
  const enabledIdx = rows.map((o, i) => (o.disabled ? -1 : i)).filter((i) => i >= 0);
  const active = activeIndex !== undefined ? activeIndex : activeS;
  const selectedOpt = [...pins, ...items].find((o) => o.value === current);

  const close = (refocus) => {
    if (isOpenControlled) return;
    setOpenS(false); setQueryS(""); setActiveS(-1);
    if (refocus && triggerRef.current) triggerRef.current.focus();
  };
  const openList = () => {
    if (disabled || isOpenControlled) return;
    setQueryS("");
    const si = [...pins, ...items].findIndex((o) => o.value === current);
    setActiveS(si >= 0 ? si : (enabledIdx[0] ?? -1));
    setOpenS(true);
  };

  React.useEffect(() => {
    if (!isOpen || isOpenControlled) return;
    if (inputRef.current) inputRef.current.focus();
    const onDoc = (e) => { if (rootRef.current && !rootRef.current.contains(e.target)) close(false); };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [isOpen, isOpenControlled]);

  const commit = (o) => {
    if (!o || o.disabled) return;
    if (!isValueControlled) setValueS(o.value);
    onChange && onChange(o.value);
    close(true);
  };
  const onQuery = (e) => {
    setQueryS(e.target.value);
    const next = [...pins, ...filterOptions(items, e.target.value)];
    const first = next.findIndex((o) => !o.disabled && !pins.includes(o));
    setActiveS(first >= 0 ? first : next.findIndex((o) => !o.disabled));
  };
  const onFieldKey = (e) => {
    if (e.key === "Escape") { e.preventDefault(); close(true); return; }
    if (e.key === "Tab") { close(false); return; }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const pos = enabledIdx.indexOf(active);
      const nextPos = e.key === "ArrowDown" ? Math.min(pos + 1, enabledIdx.length - 1) : Math.max(pos - 1, 0);
      setActiveS(enabledIdx[nextPos] ?? -1);
    } else if (e.key === "Enter") { e.preventDefault(); commit(rows[active]); }
  };
  const onTriggerKey = (e) => {
    if (!isOpen && (e.key === "Enter" || e.key === " " || e.key === "ArrowDown")) { e.preventDefault(); openList(); }
  };

  const cls = ["tasty-sselect", block ? "tasty-sselect--block" : "", className].filter(Boolean).join(" ");
  const label = selectedOpt ? selectedOpt.label : placeholder;
  const row = (o, i) => (
    <MenuItem key={"r" + i + o.value} label={<span title={o.label}>{o.label}</span>}
      selected={o.value === current} active={i === active} disabled={o.disabled}
      role="option" aria-selected={o.value === current}
      style={i === hoverIndex ? { background: "var(--tasty-menu-item-bg-hover)", color: "var(--tasty-menu-item-fg-hover)" } : undefined}
      onMouseDown={(e) => e.preventDefault()}
      onMouseEnter={() => !isOpenControlled && setActiveS(i)}
      onClick={() => commit(o)} />
  );

  return (
    <div className={cls} ref={rootRef} style={style} {...rest}>
      <button type="button" ref={triggerRef} className="tasty-sselect__trigger"
        data-open={isOpen} data-disabled={disabled} data-hover={hoverTrigger || undefined} disabled={disabled}
        aria-haspopup="listbox" aria-expanded={isOpen}
        onClick={() => (isOpen ? close(false) : openList())} onKeyDown={onTriggerKey}>
        <span className="tasty-sselect__value" data-empty={!selectedOpt} title={label}>{label}</span>
      </button>
      <span className="tasty-sselect__chevron" data-open={isOpen} data-disabled={disabled}>
        <Icon name="chevronDown" size="var(--tasty-icon-size-sm)" />
      </span>
      {isOpen && (
        <div className="tasty-sselect__menu" role="listbox">
          <label className="tasty-sselect__field" data-focus={focusSearch || undefined}>
            <span className="tasty-sselect__field-icon"><Icon name="search" size="var(--tasty-icon-size-sm)" /></span>
            <input ref={inputRef} type="text" value={q} placeholder={searchPlaceholder} aria-label={searchPlaceholder}
              spellCheck={false} autoComplete="off" readOnly={query !== undefined}
              onChange={onQuery} onKeyDown={onFieldKey} />
          </label>
          <div className="tasty-sselect__sep" role="separator" />
          {pins.length > 0 && (
            <>
              <div className="tasty-sselect__pinned">{pins.map((o, i) => row(o, i))}</div>
              <div className="tasty-sselect__sep" role="separator" />
            </>
          )}
          <div className="tasty-sselect__list tasty-scroll">
            {shown.map((o, j) => row(o, pins.length + j))}
            {loading && (
              <div className="tasty-sselect__status">
                <Spinner size={14} label={loadingLabel} />
                <span>{loadingLabel}</span>
              </div>
            )}
            {!loading && shown.length === 0 && (
              <div className="tasty-sselect__status">{emptyLabel}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
