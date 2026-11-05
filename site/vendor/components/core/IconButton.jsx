import React from "react";

/**
 * Tasty IconButton — a square, icon-only control (toolbar, tab close,
 * sidebar actions). Matches Button height; ghost by default.
 */

const CSS = `
.tasty-iconbtn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--tasty-control-height);
  height: var(--tasty-control-height);
  padding: 0;
  border: var(--tasty-border-width) solid transparent;
  border-radius: var(--tasty-radius);
  background: transparent;
  color: var(--tasty-icon-button-fg);
  cursor: pointer;
  transition: color var(--tasty-motion-ui-fast) var(--tasty-ease-ui),
              background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-iconbtn svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-iconbtn:hover { color: var(--tasty-icon-button-fg-hover); background: var(--tasty-overlay-hover); }
.tasty-iconbtn:active { background: var(--tasty-overlay-active); }
.tasty-iconbtn--sm { width: var(--tasty-control-height-tab); height: var(--tasty-control-height-tab); }
.tasty-iconbtn--sm svg { width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); }
.tasty-iconbtn--solid { background: var(--tasty-surface-raised); border-color: var(--tasty-border-default); color: var(--tasty-text-primary); }
.tasty-iconbtn--on-active, .tasty-iconbtn--on-active:hover, .tasty-iconbtn--on-active:active { color: var(--tasty-text-on-active); }
.tasty-iconbtn--on-active:focus-visible { outline-color: var(--tasty-accent-primary-raised); }
.tasty-iconbtn--active { color: var(--tasty-accent-primary-raised); background: var(--tasty-overlay-active); }
.tasty-iconbtn[disabled] { color: var(--tasty-state-disabled-fg); background: transparent; cursor: default; pointer-events: none; }
.tasty-iconbtn--solid[disabled] { background: var(--tasty-state-disabled-fill); border-color: var(--tasty-state-disabled-border); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-iconbtn-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function IconButton({
  size = "md",
  variant = "ghost",
  active = false,
  ground = "panel",
  className = "",
  children,
  ...rest
}) {
  ensureCss();
  const cls = [
    "tasty-iconbtn",
    size === "sm" ? "tasty-iconbtn--sm" : "",
    variant === "solid" ? "tasty-iconbtn--solid" : "",
    active ? "tasty-iconbtn--active" : "",
    ground === "active" ? "tasty-iconbtn--on-active" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <button className={cls} {...rest}>
      {children}
    </button>
  );
}
