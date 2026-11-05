import React from "react";

/**
 * Tasty Input — single-line text field. 28px tall, 1px border,
 * focus ring in accent-primary. Optional leading icon + addon.
 *
 * disabled — the control is unavailable: neutral box + disabled ink, no pointer.
 * readOnly — not editable NOW, but the value is information (2026-09-29, e.g. an
 *   Appearance colour row at Default showing the base hex). Same neutral box as
 *   disabled, the value keeps --tasty-input-readonly-fg (text-secondary-raised). It stays
 *   focusable, selectable and copyable; focus shows the 1px focus edge, no ring.
 * ground="raised" — the field sits on surface-raised (menu, banner, popover). The box
 *   steps down to bg-panel + border-strong in every state (2026-10-14); ink is unchanged.
 *   Consumers on surface-raised (2026-10-27): command palette search field, terminal search bar
 *   query field — no exception; every Input drawn on a surface-raised shell passes ground="raised".
 * invalid — red edge in every state; focused adds the red ring (2026-10-27: focus no longer
 *   repaints the edge blue over an invalid field).
 * focused — paints the focused box without DOM focus. Static specimens only (gallery).
 */

const CSS = `
.tasty-input {
  display: inline-flex;
  align-items: center;
  gap: var(--tasty-space-sm);
  height: var(--tasty-control-height);
  padding: 0 var(--tasty-space-md);
  border: var(--tasty-border-width) solid var(--tasty-input-border);
  border-radius: var(--tasty-radius);
  background: var(--tasty-surface-field);
  color: var(--tasty-text-primary);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  transition: border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              box-shadow var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-input--block { display: flex; width: 100%; }
.tasty-input.tasty-input--mono input { font-family: var(--tasty-font-mono); font-size: var(--tasty-font-size-caption); }
.tasty-input:hover:not(.tasty-input--raised):not(.tasty-input--invalid):not([data-readonly="true"]):not([data-disabled="true"]) { border-color: var(--tasty-border-control-raised); }
.tasty-input:hover .tasty-input__icon { color: var(--tasty-text-secondary); }
.tasty-input:focus-within, .tasty-input[data-focus="true"] { border-color: var(--tasty-border-focus); box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-border-focus); }
.tasty-input--invalid { border-color: var(--tasty-accent-danger); }
.tasty-input--invalid:focus-within, .tasty-input--invalid[data-focus="true"] { border-color: var(--tasty-accent-danger); box-shadow: 0 0 0 var(--tasty-border-width) var(--tasty-accent-danger); }
.tasty-input input {
  flex: 1; min-width: 0;
  border: 0; outline: 0; padding: 0; margin: 0;
  background: transparent; color: inherit; font: inherit;
}
.tasty-input input::placeholder { color: var(--tasty-input-placeholder); }
.tasty-input:hover input::placeholder { color: var(--tasty-text-placeholder); }
.tasty-input input:disabled { cursor: not-allowed; }
.tasty-input[data-disabled="true"] { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); pointer-events: none; }
.tasty-input[data-disabled="true"] input, .tasty-input[data-disabled="true"] input::placeholder,
.tasty-input[data-disabled="true"] .tasty-input__icon, .tasty-input[data-disabled="true"] .tasty-input__addon { color: var(--tasty-state-disabled-fg); }
.tasty-input[data-readonly="true"] { background: var(--tasty-input-readonly-bg); border-color: var(--tasty-input-readonly-border); }
.tasty-input[data-readonly="true"] input { color: var(--tasty-input-readonly-fg); cursor: text; }
.tasty-input[data-readonly="true"]:focus-within { border-color: var(--tasty-border-focus); box-shadow: none; }
.tasty-input--raised { background: var(--tasty-input-raised-bg); border-color: var(--tasty-input-raised-border); }
.tasty-input--raised[data-disabled="true"] { background: var(--tasty-state-disabled-fill-raised); border-color: var(--tasty-state-disabled-border-raised); }
.tasty-input--raised[data-readonly="true"] { background: var(--tasty-input-readonly-raised-bg); border-color: var(--tasty-input-readonly-raised-border); }
.tasty-input--raised.tasty-input--invalid { border-color: var(--tasty-accent-danger); }
.tasty-input--raised:focus-within, .tasty-input--raised[data-focus="true"], .tasty-input--raised[data-readonly="true"]:focus-within { border-color: var(--tasty-border-focus); }
.tasty-input--raised.tasty-input--invalid:focus-within, .tasty-input--raised.tasty-input--invalid[data-focus="true"] { border-color: var(--tasty-accent-danger); }
.tasty-input__icon { display: inline-flex; color: var(--tasty-input-icon-fg); flex: none; }
.tasty-input__icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-input__addon { color: var(--tasty-text-muted); font-family: var(--tasty-font-mono); font-size: var(--tasty-font-size-caption); flex: none; }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-input-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Input({
  icon = null,
  addon = null,
  block = false,
  mono = false,
  invalid = false,
  disabled = false,
  readOnly = false,
  ground = "panel",
  focused = false,
  className = "",
  ...rest
}) {
  ensureCss();
  const cls = [
    "tasty-input",
    block ? "tasty-input--block" : "",
    mono ? "tasty-input--mono" : "",
    invalid ? "tasty-input--invalid" : "",
    ground === "raised" ? "tasty-input--raised" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <div className={cls} data-disabled={disabled} data-readonly={!disabled && readOnly} data-focus={(!disabled && focused) || undefined}>
      {icon && <span className="tasty-input__icon">{icon}</span>}
      <input disabled={disabled} readOnly={readOnly} {...rest} />
      {addon && <span className="tasty-input__addon">{addon}</span>}
    </div>
  );
}
