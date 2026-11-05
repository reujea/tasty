import React from "react";

/**
 * Tasty Select — native-backed dropdown styled to match Input.
 * 28px tall with a custom chevron.
 */

const CSS = `
.tasty-select { position: relative; display: inline-flex; align-items: center; }
.tasty-select--block { display: flex; width: 100%; }
.tasty-select select {
  appearance: none;
  height: var(--tasty-control-height);
  width: 100%;
  padding: 0 var(--tasty-select-chevron-room) 0 var(--tasty-space-md);
  border: var(--tasty-border-width) solid var(--tasty-select-border);
  border-radius: var(--tasty-radius);
  background: var(--tasty-surface-field);
  color: var(--tasty-text-primary);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  cursor: pointer;
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-select select:hover { border-color: var(--tasty-border-control); }
.tasty-select select:focus { outline: 0; border-color: var(--tasty-border-focus); box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-border-focus); }
.tasty-select select:disabled { color: var(--tasty-state-disabled-fg); background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); cursor: default; }
/* 2026-10-21 — no value: trigger ink = select-placeholder; disabled (with or without a value) = state-disabled-fg.
   .tasty-select--empty sits on the wrapper, so it is a compound with .tasty-select, never a descendant. */
.tasty-select--empty select { color: var(--tasty-select-placeholder); }
.tasty-select--empty select:disabled { color: var(--tasty-state-disabled-fg); }
.tasty-select select option { color: var(--tasty-text-primary); }
/* 2026-10-21 — ground="raised": border-default = surface-raised, so the normal box vanished on a raised card */
.tasty-select--raised select { background: var(--tasty-select-raised-bg); border-color: var(--tasty-select-raised-border); }
.tasty-select--raised select:hover { border-color: var(--tasty-border-control-raised); }
.tasty-select--raised select:focus { border-color: var(--tasty-border-focus); }
.tasty-select--raised select:disabled { background: var(--tasty-select-raised-disabled-bg); border-color: var(--tasty-select-raised-disabled-border); }
.tasty-select__chevron {
  position: absolute; right: var(--tasty-select-chevron-offset); top: 50%; transform: translateY(-50%);
  pointer-events: none; color: var(--tasty-text-muted);
}
.tasty-select:hover .tasty-select__chevron { color: var(--tasty-text-secondary); }
.tasty-select:has(select:disabled) .tasty-select__chevron { color: var(--tasty-state-disabled-fg); }
.tasty-select__chevron svg { width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); display: block; }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-select-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Select({ options = [], block = false, placeholder, ground = "panel", className = "", children, onChange, ...rest }) {
  ensureCss();
  // 2026-10-19 — placeholder: a hidden, disabled "" option. The trigger shows it in placeholder ink while the
  // value is ""; it is never a row in the open list (MenuItem no-value rule, 2026-10-09).
  const controlled = rest.value !== undefined;
  const [inner, setInner] = React.useState(rest.defaultValue !== undefined ? rest.defaultValue : "");
  const cur = controlled ? rest.value : inner;
  const empty = placeholder != null && (cur === "" || cur == null);
  const props = { ...rest };
  if (placeholder != null && !controlled && props.defaultValue === undefined) props.defaultValue = "";
  const cls = ["tasty-select", block ? "tasty-select--block" : "", empty ? "tasty-select--empty" : "", ground === "raised" ? "tasty-select--raised" : "", className].filter(Boolean).join(" ");
  return (
    <div className={cls} aria-disabled={rest.disabled || undefined}>
      <select {...props} onChange={(e) => { if (!controlled) setInner(e.target.value); if (onChange) onChange(e); }}>
        {placeholder != null && <option value="" disabled hidden>{placeholder}</option>}
        {children ||
          options.map((o) => {
            const opt = typeof o === "string" ? { value: o, label: o } : o;
            return (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            );
          })}
      </select>
      <span className="tasty-select__chevron">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="m6 9 6 6 6-6" />
        </svg>
      </span>
    </div>
  );
}
