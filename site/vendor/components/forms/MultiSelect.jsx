import React from "react";
import { Checkbox } from "./Checkbox.jsx";

/**
 * Tasty MultiSelect — the Select sibling that holds MANY values at once.
 *
 * Closed, it is a Select: same --tasty-control-height, border, radius, font,
 * chevron — so the two read as one family side by side in a form. Open, it is
 * NOT a native list: it is a menu container (MenuItem language) of Checkbox
 * rows, and clicking a row toggles that row ONLY — the menu stays open so you
 * can flip three states in a row. Outside click / Esc closes.
 *
 * The trigger never enumerates the selection; it shows a caller-injected
 * summary (0 / N / all), because the widget owns no translations.
 *
 * - `options`      — strings or { value, label, disabled }.
 * - `value`        — controlled array of selected values (`defaultValue` otherwise).
 * - `summary`      — (count, total) => string; the injected i18n hook.
 * - `placeholder`  — 0-selected label (placeholder tone).
 * - `allToggle`    — top "Select all / Clear all" action row + separator (off by default).
 * - `maxMenuHeight`— px cap; the list scrolls internally past it.
 *
 * Controlled hooks for specimens: `open`, `activeIndex` (keyboard row),
 * `hoverIndex` (forces a pointer-hover wash).
 */

const CSS = `
.tasty-mselect { position: relative; display: inline-flex; flex-direction: column; }
.tasty-mselect--block { display: flex; width: 100%; }
.tasty-mselect__trigger {
  all: unset; box-sizing: border-box;
  display: flex; align-items: center;
  width: 100%;
  height: var(--tasty-multiselect-height);
  padding: 0 var(--tasty-multiselect-chevron-room) 0 var(--tasty-multiselect-padding-x);
  border: var(--tasty-border-width) solid var(--tasty-multiselect-border);
  border-radius: var(--tasty-multiselect-radius);
  background: var(--tasty-multiselect-bg);
  color: var(--tasty-multiselect-fg);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-multiselect-font-size);
  cursor: pointer;
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-mselect__trigger:hover { border-color: var(--tasty-multiselect-border-hover); }
.tasty-mselect__trigger:focus-visible,
.tasty-mselect__trigger[data-open="true"] {
  border-color: var(--tasty-multiselect-border-focus);
  box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-multiselect-border-focus);
}
.tasty-mselect__trigger[data-disabled="true"] { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); cursor: default; }
.tasty-mselect__trigger[data-disabled="true"], .tasty-mselect__trigger[data-disabled="true"] * { color: var(--tasty-state-disabled-fg); }
.tasty-mselect__summary { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
.tasty-mselect__summary[data-empty="true"] { color: var(--tasty-multiselect-summary-fg-empty); }
.tasty-mselect__chevron {
  position: absolute; right: var(--tasty-multiselect-chevron-offset); top: calc(var(--tasty-multiselect-height) / 2);
  transform: translateY(-50%); pointer-events: none;
  color: var(--tasty-multiselect-chevron-fg);
  transition: transform var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-mselect__chevron[data-open="true"] { transform: translateY(-50%) rotate(180deg); }
.tasty-mselect__chevron svg { width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); display: block; }

.tasty-mselect__menu {
  position: absolute; left: 0; top: calc(100% + var(--tasty-multiselect-menu-gap)); z-index: 40;
  min-width: 100%; max-width: var(--tasty-multiselect-menu-max-width); box-sizing: border-box;
  background: var(--tasty-multiselect-menu-bg);
  border: var(--tasty-border-width) solid var(--tasty-multiselect-menu-border);
  border-radius: var(--tasty-multiselect-menu-radius);
  box-shadow: var(--tasty-multiselect-menu-shadow);
  padding: var(--tasty-multiselect-menu-padding);
  overflow: hidden;
}
.tasty-mselect__list {
  max-height: var(--tasty-multiselect-menu-max-height);
  overflow-y: auto; overflow-x: hidden;
  display: flex; flex-direction: column;
}
.tasty-mselect__row {
  display: flex; align-items: center;
  height: var(--tasty-multiselect-row-height);
  padding: 0 var(--tasty-multiselect-row-padding-x);
  gap: var(--tasty-multiselect-row-gap);
  border-radius: var(--tasty-menu-item-radius);
  color: var(--tasty-multiselect-row-fg);
  font-size: var(--tasty-multiselect-font-size);
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-mselect__row:hover, .tasty-mselect__row[data-hover="true"] { background: var(--tasty-multiselect-row-bg-hover); }
.tasty-mselect__row:hover .tasty-check__label, .tasty-mselect__row[data-hover="true"] .tasty-check__label { color: var(--tasty-text-on-active); }
.tasty-mselect__row[data-active="true"] { background: var(--tasty-multiselect-row-bg-active); color: var(--tasty-multiselect-row-fg-active); }
.tasty-mselect__row .tasty-check { flex: 1; min-width: 0; height: 100%; }
.tasty-mselect__row .tasty-check__label {
  flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  color: var(--tasty-multiselect-row-fg);
}
.tasty-mselect__row[data-active="true"] .tasty-check__label { color: var(--tasty-multiselect-row-fg-active); }
.tasty-mselect__all {
  all: unset; box-sizing: border-box;
  display: flex; align-items: center;
  height: var(--tasty-multiselect-row-height);
  padding: 0 var(--tasty-multiselect-row-padding-x);
  border-radius: var(--tasty-menu-item-radius);
  color: var(--tasty-multiselect-all-fg);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-multiselect-font-size);
  cursor: pointer;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-mselect__all:hover { background: var(--tasty-multiselect-row-bg-hover); }
.tasty-mselect__sep { height: var(--tasty-border-width); margin: var(--tasty-space-xs) 0; background: var(--tasty-multiselect-separator); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-multiselect-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

const CHEVRON = (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="m6 9 6 6 6-6" />
  </svg>
);

function norm(o) {
  return typeof o === "string" ? { value: o, label: o, disabled: false } : { disabled: false, ...o };
}

// Default (English) summary. Consumers inject their own via `summary`.
function defaultSummary(count, total) {
  if (count === 0) return null;
  if (count === total) return "All";
  return count + " selected";
}

export function MultiSelect({
  options = [],
  value,
  defaultValue = [],
  onChange,
  summary = defaultSummary,
  placeholder = "None",
  allToggle = false,
  allLabel = "Select all",
  clearLabel = "Clear all",
  block = false,
  disabled = false,
  maxMenuHeight = null,
  // controlled hooks (specimens/tests)
  open,
  activeIndex,
  hoverIndex = null,
  className = "",
  ...rest
}) {
  ensureCss();
  const items = options.map(norm);
  const isValueControlled = value !== undefined;
  const isOpenControlled = open !== undefined;

  const [valueS, setValueS] = React.useState(defaultValue);
  const [openS, setOpenS] = React.useState(false);
  const [activeS, setActiveS] = React.useState(0);
  const rootRef = React.useRef(null);

  const selected = isValueControlled ? value : valueS;
  const isOpen = isOpenControlled ? open : openS;
  const active = activeIndex !== undefined ? activeIndex : activeS;

  React.useEffect(() => {
    if (!isOpen || isOpenControlled) return;
    const onDoc = (e) => { if (rootRef.current && !rootRef.current.contains(e.target)) setOpenS(false); };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [isOpen, isOpenControlled]);

  const emit = (next) => {
    if (!isValueControlled) setValueS(next);
    onChange && onChange(next);
  };
  const toggle = (v) => {
    const next = selected.includes(v) ? selected.filter((x) => x !== v) : [...selected, v];
    emit(next);
  };
  const allOn = items.length > 0 && items.every((o) => selected.includes(o.value));

  const onKeyDown = (e) => {
    if (isOpenControlled) return;
    if (e.key === "Escape") { setOpenS(false); return; }
    if (!isOpen && (e.key === "Enter" || e.key === " " || e.key === "ArrowDown")) { e.preventDefault(); setOpenS(true); return; }
    if (!isOpen) return;
    if (e.key === "ArrowDown") { e.preventDefault(); setActiveS((i) => Math.min(i + 1, items.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActiveS((i) => Math.max(i - 1, 0)); }
    else if (e.key === "Home") { e.preventDefault(); setActiveS(0); }
    else if (e.key === "End") { e.preventDefault(); setActiveS(items.length - 1); }
    else if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      const o = items[active];
      if (o && !o.disabled) toggle(o.value);
    }
  };

  const count = selected.length;
  const label = summary(count, items.length) ?? placeholder;
  const cls = ["tasty-mselect", block ? "tasty-mselect--block" : "", className].filter(Boolean).join(" ");
  const listStyle = maxMenuHeight != null
    ? { maxHeight: typeof maxMenuHeight === "number" ? maxMenuHeight + "px" : maxMenuHeight } : undefined;

  return (
    <div className={cls} ref={rootRef} {...rest}>
      <button
        type="button"
        className="tasty-mselect__trigger"
        data-open={isOpen}
        data-disabled={disabled}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        onClick={() => { if (!isOpenControlled) setOpenS((o) => !o); }}
        onKeyDown={onKeyDown}
      >
        <span className="tasty-mselect__summary" data-empty={count === 0} title={label}>{label}</span>
      </button>
      <span className="tasty-mselect__chevron" data-open={isOpen}>{CHEVRON}</span>
      {isOpen && (
        <div className="tasty-mselect__menu" role="listbox" aria-multiselectable="true">
          {allToggle && (
            <>
              <button type="button" className="tasty-mselect__all"
                onClick={() => emit(allOn ? [] : items.filter((o) => !o.disabled).map((o) => o.value))}>
                {allOn ? clearLabel : allLabel}
              </button>
              <div className="tasty-mselect__sep" role="separator" />
            </>
          )}
          <div className="tasty-mselect__list tasty-scroll" style={listStyle}>
            {items.map((o, i) => (
              <div
                key={o.value}
                className="tasty-mselect__row"
                role="option"
                aria-selected={selected.includes(o.value)}
                data-active={i === active ? "true" : undefined}
                data-hover={i === hoverIndex ? "true" : undefined}
              >
                <Checkbox
                  label={<span title={o.label}>{o.label}</span>}
                  checked={selected.includes(o.value)}
                  disabled={o.disabled}
                  onChange={() => toggle(o.value)}
                />
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
