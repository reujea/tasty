import React from "react";

/**
 * Tasty Button — the primary interactive control.
 * 28px tall (item-height-interactive), 1px border, 4px radius,
 * 8%/12% hover/active overlays derived from the active theme.
 * Disabled: neutral box + disabled ink (no opacity), same for every variant.
 */

const CSS = `
.tasty-btn {
  --_h: var(--tasty-control-height);
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--tasty-space-sm);
  height: var(--_h);
  padding: 0 var(--tasty-space-md);
  border: var(--tasty-border-width) solid transparent;
  border-radius: var(--tasty-radius);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  font-weight: var(--tasty-font-weight-medium);
  line-height: 1;
  white-space: nowrap;
  cursor: pointer;
  user-select: none;
  background: transparent;
  color: var(--tasty-text-primary);
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              border-color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              filter var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-btn::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: transparent;
  pointer-events: none;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-btn:hover::after { background: var(--tasty-overlay-hover); }
.tasty-btn:active::after { background: var(--tasty-overlay-active); }

.tasty-btn--sm { --_h: var(--tasty-button-height-sm); padding: 0 var(--tasty-space-sm); font-size: var(--tasty-font-size-caption); }
.tasty-btn--lg { --_h: var(--tasty-button-height-lg); padding: 0 var(--tasty-space-lg); }

.tasty-btn--primary { background: var(--tasty-accent-primary); color: var(--tasty-text-on-accent); font-weight: var(--tasty-font-weight-semibold); }
.tasty-btn--agent   { background: var(--tasty-accent-agent);   color: var(--tasty-text-on-accent); font-weight: var(--tasty-font-weight-semibold); }
.tasty-btn--danger  { background: var(--tasty-accent-danger);  color: var(--tasty-text-on-accent); font-weight: var(--tasty-font-weight-semibold); }

.tasty-btn--secondary { background: var(--tasty-surface-raised); border-color: var(--tasty-border-default); }
.tasty-btn--secondary svg { color: var(--tasty-text-secondary); }
.tasty-btn--secondary:hover { border-color: var(--tasty-button-secondary-border-hover); color: var(--tasty-text-on-active); }
.tasty-btn--secondary:active { border-color: var(--tasty-border-default); color: var(--tasty-text-on-active); }

.tasty-btn--ghost { background: transparent; color: var(--tasty-button-ghost-fg); }
.tasty-btn--ghost svg { color: var(--tasty-text-secondary); }
.tasty-btn--ghost:hover, .tasty-btn--ghost:active { color: var(--tasty-button-ghost-fg-hover); }
/* 2026-10-21 — ghost on a raised ground (ground="raised"): Latte text-secondary on surface0 is 4.05:1 */
.tasty-btn--secondary.tasty-btn--on-raised { border-color: var(--tasty-banner-button-border); }
.tasty-btn--secondary.tasty-btn--on-raised:hover { border-color: var(--tasty-border-frame); color: var(--tasty-text-primary); }
.tasty-btn--secondary.tasty-btn--on-raised:active { border-color: var(--tasty-border-frame); color: var(--tasty-text-on-active-hover); }
.tasty-btn--ghost.tasty-btn--on-raised { color: var(--tasty-button-ghost-raised-fg); }
.tasty-btn--ghost.tasty-btn--on-raised:hover { color: var(--tasty-button-ghost-raised-fg-hover); }
.tasty-btn--ghost.tasty-btn--on-active { color: var(--tasty-button-ghost-active-fg); }
.tasty-btn--ghost.tasty-btn--on-active:hover, .tasty-btn--ghost.tasty-btn--on-active:active { color: var(--tasty-text-on-active-hover); }
.tasty-btn--ghost.tasty-btn--on-active svg { color: var(--tasty-text-on-active); }
.tasty-btn--ghost.tasty-btn--on-active:focus-visible { outline-color: var(--tasty-accent-primary-raised); }

/* disabled = ink, never opacity (2026-09-29): every variant draws the same neutral box,
   accent fills drop out, label + icons take the disabled ink, no hover / active overlay. */
.tasty-btn[disabled] { background: var(--tasty-button-disabled-bg); border-color: var(--tasty-button-disabled-border);
  color: var(--tasty-button-disabled-fg); cursor: default; pointer-events: none; }
.tasty-btn--ghost[disabled] { background: transparent; border-color: transparent; }
.tasty-btn[disabled] svg { color: var(--tasty-button-disabled-fg); }
.tasty-btn[disabled]::after { background: transparent; }
/* Secondary on a banner shell — one ramp step up from banner-bg so the box reads */
[data-surface="banner"] .tasty-btn--secondary:not([disabled]) { background: var(--tasty-banner-button-bg); border-color: var(--tasty-banner-button-border); }
.tasty-btn--block { display: flex; width: 100%; }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-btn-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Button({
  variant = "secondary",
  size = "md",
  block = false,
  leadingIcon = null,
  trailingIcon = null,
  ground = "panel",
  className = "",
  children,
  ...rest
}) {
  ensureCss();
  const cls = [
    "tasty-btn",
    `tasty-btn--${variant}`,
    size !== "md" ? `tasty-btn--${size}` : "",
    block ? "tasty-btn--block" : "",
    ground === "raised" ? "tasty-btn--on-raised" : "",
    ground === "active" ? "tasty-btn--on-active" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <button className={cls} {...rest}>
      {leadingIcon}
      {children != null && <span>{children}</span>}
      {trailingIcon}
    </button>
  );
}
