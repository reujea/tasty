import React from "react";

/**
 * Tasty Tag — a small monospace label for surface kinds, permissions,
 * plugin origins ("built-in"), and detector rule types.
 *
 * disabled (own prop, or any Tag inside a disabled ListCtrl row) follows the ink
 * rule: every variant draws the neutral box (tag-disabled-bg / -border) with the
 * disabled ink; accent fills and tint edges drop out, the dot takes the ink too.
 */

const CSS = `
.tasty-tag {
  display: inline-flex;
  align-items: center;
  gap: var(--tasty-tag-gap);
  height: var(--tasty-tag-size);
  padding: 0 var(--tasty-tag-padding-x);
  border-radius: var(--tasty-radius-sm);
  border: var(--tasty-border-width) solid var(--tasty-border-default);
  background: var(--tasty-surface-raised);
  font-family: var(--tasty-font-mono);
  font-size: var(--tasty-tag-font-size);
  font-weight: var(--tasty-font-weight-medium);
  line-height: var(--tasty-line-height-tight);
  color: var(--tasty-tag-fg);
  white-space: nowrap;
}
.tasty-tag--accent { border-color: transparent; color: var(--tasty-text-on-accent); background: var(--tasty-accent-primary); }
.tasty-tag--agent  { border-color: transparent; color: var(--tasty-text-on-accent); background: var(--tasty-accent-agent); }
.tasty-tag--success{ color: var(--tasty-tag-success-fg); border-color: color-mix(in srgb, var(--tasty-accent-success) 40%, transparent); }
.tasty-tag--warning{ color: var(--tasty-tag-warning-fg); border-color: color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent); }
.tasty-tag--attention{ color: var(--tasty-tag-attention-fg); border-color: color-mix(in srgb, var(--tasty-accent-attention) 40%, transparent); }
.tasty-tag--danger { color: var(--tasty-tag-danger-fg);  border-color: color-mix(in srgb, var(--tasty-accent-danger) 40%, transparent); }
.tasty-tag--info   { color: var(--tasty-tag-info-fg);    border-color: color-mix(in srgb, var(--tasty-accent-info) 40%, transparent); }
.tasty-tag--attached { color: var(--tasty-tag-attached-fg); border-color: color-mix(in srgb, var(--tasty-accent-attached) 40%, transparent); }
.tasty-tag--caps { text-transform: uppercase; letter-spacing: var(--tasty-letter-spacing-caps); }
.tasty-tag--disabled { background: var(--tasty-tag-disabled-bg); border-color: var(--tasty-tag-disabled-border); color: var(--tasty-tag-disabled-fg); }
.tasty-tag--disabled[data-ground="raised"] { border-color: var(--tasty-border-strong); }
.tasty-tag--disabled[data-ground="active"] { border-color: var(--tasty-border-frame); }
.tasty-tag__dot { width: var(--tasty-tag-dot-size); height: var(--tasty-tag-dot-size); border-radius: var(--tasty-radius-pill); background: currentColor; flex: none; }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-tag-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Tag({ variant = "default", dot = false, disabled = false, caps = false, ground, className = "", children, ...rest }) {
  ensureCss();
  const cls = [
    "tasty-tag",
    variant !== "default" ? `tasty-tag--${variant}` : "",
    disabled ? "tasty-tag--disabled" : "",
    caps ? "tasty-tag--caps" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <span className={cls} data-ground={ground} aria-disabled={disabled || undefined} {...rest}>
      {dot && <span className="tasty-tag__dot" />}
      {children}
    </span>
  );
}
