import React from "react";

/**
 * Tasty Toast — a transient, bottom-anchored notification ("Copied",
 * "Path copied", "Force detach"). Surface-raised card, accent rail,
 * optional icon. Coalesces in the real app; here it's a static cell.
 *
 * Host contract (2026-10-06):
 *   variant "agent" — catalog only; the host does NOT emit it (toast trigger policy: agent CLI/IPC
 *     results are logged, never toasted). Kept so agent-hosted plugins can reuse the card.
 *   hint — the shortcut of the action that produced the notice, shown only when that action came
 *     from a non-keyboard path (menu / mouse) and the binding is non-empty. Empty binding → no hint.
 *   icon — catalog-only optional slot; the host card draws rail + body only.
 */

const CSS = `
.tasty-toast {
  display: inline-flex;
  align-items: center;
  gap: var(--tasty-space-sm);
  min-height: var(--tasty-control-height);
  padding: var(--tasty-space-sm) var(--tasty-space-md);
  border: var(--tasty-border-width) solid var(--tasty-toast-border);
  border-left-width: var(--tasty-toast-accent-width);
  border-left-color: var(--tasty-accent-primary);
  border-radius: var(--tasty-radius);
  background: var(--tasty-surface-raised);
  color: var(--tasty-text-primary);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  max-width: var(--tasty-toast-max-width);
}
.tasty-toast--success { border-left-color: var(--tasty-accent-success); }
.tasty-toast--warning { border-left-color: var(--tasty-accent-warning); }
.tasty-toast--danger  { border-left-color: var(--tasty-accent-danger); }
.tasty-toast--agent   { border-left-color: var(--tasty-accent-agent); }
.tasty-toast__icon { flex: none; display: inline-flex; }
.tasty-toast__icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-toast--success .tasty-toast__icon { color: var(--tasty-toast-accent-success); }
.tasty-toast--warning .tasty-toast__icon { color: var(--tasty-toast-accent-warning); }
.tasty-toast--danger  .tasty-toast__icon { color: var(--tasty-toast-accent-danger); }
.tasty-toast--agent   .tasty-toast__icon { color: var(--tasty-toast-accent-agent); }
.tasty-toast--info    .tasty-toast__icon { color: var(--tasty-toast-accent-info); }
.tasty-toast__msg { flex: 1; min-width: 0; line-height: var(--tasty-line-height-ui); }
/* hint (2026-10-06): display-only, never truncates — the body wraps first. Pinned to the FIRST line
   of the body (align-self start + one body line box), so a two-line notice keeps the key beside its verb. */
.tasty-toast__hint { flex: none; align-self: flex-start; display: inline-flex; align-items: center; white-space: nowrap;
  min-height: calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui));
  font-family: var(--tasty-font-mono); font-size: var(--tasty-toast-hint-font-size); color: var(--tasty-text-muted); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-toast-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Toast({ variant = "info", icon = null, hint = null, className = "", children, ...rest }) {
  ensureCss();
  const cls = ["tasty-toast", `tasty-toast--${variant}`, className].filter(Boolean).join(" ");
  return (
    <div className={cls} role="status" {...rest}>
      {icon && <span className="tasty-toast__icon">{icon}</span>}
      <span className="tasty-toast__msg">{children}</span>
      {hint != null && <span className="tasty-toast__hint">{hint}</span>}
    </div>
  );
}
