import React from "react";
import { Icon } from "../core/Icon";

/**
 * Tasty MenuItem — a row in a context menu, command palette, or tools
 * menu. 28px tall, icon + label + optional trailing shortcut. Supports
 * a danger treatment and a separator variant.
 */

const CSS = `
.tasty-menuitem {
  display: flex;
  align-items: center;
  gap: var(--tasty-space-sm);
  height: var(--tasty-control-height);
  padding: 0 var(--tasty-space-md);
  border-radius: var(--tasty-radius-sm);
  color: var(--tasty-menu-item-fg); /* 2026-10-07 — token is canonical: resting text-secondary, hover/active text-primary */
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-menuitem:hover { background: var(--tasty-menu-item-bg-hover); color: var(--tasty-menu-item-fg-hover); }
.tasty-menuitem--active { background: var(--tasty-surface-active); color: var(--tasty-menu-item-fg-hover); }
.tasty-menuitem--danger:hover { color: var(--tasty-accent-danger-strong); }
.tasty-menuitem--danger { color: var(--tasty-accent-danger-raised); }
.tasty-menuitem--active .tasty-kbd__plus { color: var(--tasty-kbd-plus-fg-active); }
.tasty-menuitem__icon { flex: none; display: inline-flex; color: var(--tasty-text-secondary); }
.tasty-menuitem--danger .tasty-menuitem__icon { color: var(--tasty-accent-danger-strong); }
.tasty-menuitem__icon svg { width: var(--tasty-icon-size-md); height: var(--tasty-icon-size-md); display: block; }
.tasty-menuitem__label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
/* 2026-10-08 (batch 8) — selected option (the current value of a Select-style list): ink + trailing check, no fill */
.tasty-menuitem--selected { color: var(--tasty-menu-item-selected-fg); }
.tasty-menuitem__check { flex: none; display: inline-flex; color: var(--tasty-menu-item-check-fg); }
.tasty-menuitem__check svg { width: var(--tasty-menu-item-check-size); height: var(--tasty-menu-item-check-size); display: block; }
/* wrap — the label wraps instead of ellipsising (fixed copy wider than the menu's max width); row grows */
.tasty-menuitem--wrap { height: auto; min-height: var(--tasty-menu-item-height); padding-top: var(--tasty-menu-item-wrap-padding-y); padding-bottom: var(--tasty-menu-item-wrap-padding-y); white-space: normal; line-height: var(--tasty-line-height-ui); }
.tasty-menuitem--wrap .tasty-menuitem__label { overflow: visible; }
.tasty-menuitem__shortcut { flex: none; font-family: var(--tasty-font-mono); font-size: var(--tasty-menu-item-shortcut-font-size); color: var(--tasty-text-muted-raised); }
.tasty-menuitem:hover .tasty-menuitem__shortcut { color: var(--tasty-text-on-active); }
.tasty-menuitem:hover .tasty-menuitem__check { color: var(--tasty-accent-primary-raised); }
/* 2026-11-05 — a keyboard-active row: one ink for every part, danger included (the row's fill is surface-active) */
.tasty-menuitem--active .tasty-menuitem__icon, .tasty-menuitem--active .tasty-menuitem__shortcut, .tasty-menuitem--danger.tasty-menuitem--active, .tasty-menuitem--danger.tasty-menuitem--active .tasty-menuitem__icon { color: var(--tasty-menu-item-part-fg-active); }
.tasty-menuitem[data-disabled="true"] { pointer-events: none; }
.tasty-menuitem[data-disabled="true"], .tasty-menuitem[data-disabled="true"] * { color: var(--tasty-state-disabled-fg); }
.tasty-menu-sep { height: var(--tasty-border-width); margin: var(--tasty-space-xs) 0; background: var(--tasty-menu-separator); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-menuitem-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function MenuItem({
  label,
  icon = null,
  shortcut = null,
  danger = false,
  active = false,
  selected = false,
  wrap = false,
  disabled = false,
  separator = false,
  className = "",
  ...rest
}) {
  ensureCss();
  if (separator) return <div className="tasty-menu-sep" role="separator" />;
  const cls = [
    "tasty-menuitem",
    danger ? "tasty-menuitem--danger" : "",
    active ? "tasty-menuitem--active" : "",
    selected ? "tasty-menuitem--selected" : "",
    wrap ? "tasty-menuitem--wrap" : "",
    className,
  ].filter(Boolean).join(" ");
  return (
    <div className={cls} data-disabled={disabled} aria-disabled={disabled || undefined} role={selected ? "menuitemradio" : "menuitem"} aria-checked={selected || undefined} {...rest}>
      {icon && <span className="tasty-menuitem__icon">{icon}</span>}
      <span className="tasty-menuitem__label">{label}</span>
      {shortcut != null && <span className="tasty-menuitem__shortcut">{shortcut}</span>}
      {selected && <span className="tasty-menuitem__check"><Icon name="check" /></span>}
    </div>
  );
}
