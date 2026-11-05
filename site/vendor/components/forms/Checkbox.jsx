import React from "react";

/**
 * Tasty Checkbox — square, 16px, accent fill when checked.
 * Pairs with an optional label.
 */

const CSS = `
.tasty-check { display: inline-flex; align-items: center; gap: var(--tasty-space-sm); cursor: pointer; user-select: none; }
.tasty-check input { position: absolute; opacity: 0; width: 0; height: 0; }
.tasty-check__box {
  width: var(--tasty-checkbox-size); height: var(--tasty-checkbox-size); flex: none;
  border: var(--tasty-border-width) solid var(--tasty-border-control);
  border-radius: var(--tasty-radius-sm);
  background: var(--tasty-surface-raised);
  display: inline-flex; align-items: center; justify-content: center;
  color: var(--tasty-text-on-accent);
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-check__box svg { width: var(--tasty-icon-size-xs); height: var(--tasty-icon-size-xs); opacity: 0; }
.tasty-check input:checked + .tasty-check__box { background: var(--tasty-accent-primary); border-color: var(--tasty-accent-primary); }
.tasty-check input:checked + .tasty-check__box svg { opacity: 1; }
.tasty-check input:focus-visible + .tasty-check__box { outline: var(--tasty-focus-ring-width) solid var(--tasty-border-focus); outline-offset: 1px; }
.tasty-check[data-ground="raised"] .tasty-check__box { border-color: var(--tasty-border-control-raised); }
.tasty-check[data-ground="active"] .tasty-check__box { border-color: var(--tasty-text-on-active); }
.tasty-check[data-ground="active"] input:focus-visible + .tasty-check__box { outline-color: var(--tasty-accent-primary-raised); }
.tasty-check__label { font-size: var(--tasty-font-size-body); color: var(--tasty-text-primary); }
.tasty-check[data-disabled="true"] { cursor: default; pointer-events: none; }
.tasty-check[data-disabled="true"] input + .tasty-check__box,
.tasty-check[data-disabled="true"] input:checked + .tasty-check__box { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); color: var(--tasty-state-disabled-fg); }
.tasty-check[data-disabled="true"] .tasty-check__label { color: var(--tasty-state-disabled-fg); }
.tasty-check[data-disabled="true"][data-ground="raised"] input + .tasty-check__box { border-color: var(--tasty-border-strong); }
.tasty-check[data-disabled="true"][data-ground="active"] input + .tasty-check__box { border-color: var(--tasty-border-frame); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-check-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Checkbox({ label, checked, defaultChecked, disabled = false, ground, className = "", ...rest }) {
  ensureCss();
  return (
    <label className={["tasty-check", className].filter(Boolean).join(" ")} data-disabled={disabled} data-ground={ground} aria-disabled={disabled || undefined}>
      <input type="checkbox" checked={checked} defaultChecked={defaultChecked} disabled={disabled} {...rest} />
      <span className="tasty-check__box">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
          <path d="M20 6 9 17l-5-5" />
        </svg>
      </span>
      {label != null && <span className="tasty-check__label">{label}</span>}
    </label>
  );
}
