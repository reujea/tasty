import React from "react";

/**
 * Tasty StatusDot — a small state indicator for surfaces, panes, and
 * plugins (running / idle / attached-elsewhere / error). Optional pulse
 * for live/running state. Optional text label.
 *
 * `attached` (claimed by another client) is an ORTHOGONAL flag, not a status:
 * it draws a lavender ring AROUND the status dot (via outline + offset) so a
 * single dot carries both owner×activity (fill) and attached (ring) without
 * needing a second dot. Deliberately lavender, never danger-red.
 *
 * `needs-input` / `completion` are ATTENTION kinds, not execution states. They
 * share this one dot slot with execution and win it by rank:
 * needs-input > completion > running/agent/waiting > idle.
 */

const CSS = `
.tasty-statusdot { display: inline-flex; align-items: center; gap: var(--tasty-space-xs); font-family: var(--tasty-font-ui); font-size: var(--tasty-font-size-caption); color: var(--tasty-text-secondary); }
.tasty-statusdot__dot { position: relative; width: var(--tasty-status-dot-size); height: var(--tasty-status-dot-size); border-radius: var(--tasty-radius-pill); flex: none; background: var(--tasty-status-dot-idle); }
.tasty-statusdot--running .tasty-statusdot__dot { background: var(--tasty-status-dot-success); }
.tasty-statusdot--idle    .tasty-statusdot__dot { background: var(--tasty-status-dot-idle); }
.tasty-statusdot--agent   .tasty-statusdot__dot { background: var(--tasty-status-dot-agent); }
.tasty-statusdot--waiting .tasty-statusdot__dot { background: var(--tasty-accent-warning-glyph); }
.tasty-statusdot--error   .tasty-statusdot__dot { background: var(--tasty-status-dot-danger); }
.tasty-statusdot--needs-input .tasty-statusdot__dot { background: var(--tasty-status-dot-needs-input); }
.tasty-statusdot--completion  .tasty-statusdot__dot { background: var(--tasty-status-dot-completion); }
.tasty-statusdot--attached .tasty-statusdot__dot { outline: var(--tasty-status-dot-attached-ring-width) solid var(--tasty-status-dot-attached-ring); outline-offset: var(--tasty-status-dot-attached-ring-offset); }
.tasty-statusdot[data-ground="raised"].tasty-statusdot--idle .tasty-statusdot__dot { background: var(--tasty-glyph-dim-raised); }
.tasty-statusdot[data-ground="raised"].tasty-statusdot--waiting .tasty-statusdot__dot { background: var(--tasty-accent-warning-raised); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--idle .tasty-statusdot__dot { background: var(--tasty-status-dot-idle-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--running .tasty-statusdot__dot { background: var(--tasty-status-dot-success-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--waiting .tasty-statusdot__dot { background: var(--tasty-status-dot-warning-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--error .tasty-statusdot__dot { background: var(--tasty-status-dot-danger-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--agent .tasty-statusdot__dot { background: var(--tasty-status-dot-agent-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--needs-input .tasty-statusdot__dot { background: var(--tasty-status-dot-needs-input-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--completion .tasty-statusdot__dot { background: var(--tasty-status-dot-completion-active); }
.tasty-statusdot[data-ground="active"].tasty-statusdot--attached .tasty-statusdot__dot { outline-color: var(--tasty-status-dot-attached-ring-active); }
.tasty-statusdot--pulse .tasty-statusdot__dot::after {
  content: ""; position: absolute; inset: calc(-1 * var(--tasty-size-3)); border-radius: var(--tasty-radius-pill);
  background: inherit; opacity: 0.4; animation: tasty-pulse var(--tasty-status-dot-pulse-duration) ease-out infinite;
}
@keyframes tasty-pulse { 0% { transform: scale(0.6); opacity: 0.5; } 100% { transform: scale(1.8); opacity: 0; } }
@media (prefers-reduced-motion: reduce) { .tasty-statusdot--pulse .tasty-statusdot__dot::after { animation: none; } }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-statusdot-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function StatusDot({ status = "idle", pulse = false, attached = false, label = null, ground, className = "", ...rest }) {
  ensureCss();
  const cls = [
    "tasty-statusdot",
    `tasty-statusdot--${status}`,
    pulse ? "tasty-statusdot--pulse" : "",
    attached ? "tasty-statusdot--attached" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <span className={cls} data-ground={ground} {...rest}>
      <span className="tasty-statusdot__dot" />
      {label != null && <span>{label}</span>}
    </span>
  );
}
