import React from "react";

/**
 * Tasty TreeRow — a row in the sidebar / file explorer tree. 22px tall
 * (item-height-tree), supports indent levels, an optional disclosure
 * chevron, a leading icon, and a selected state.
 *
 * No disabled state (2026-09-29). A tree row names something that exists; if it
 * can't be opened (no permission, remote dropped) the row stays a normal row and
 * the open attempt reports why. Dimmed items (pending cut) use state-dim-opacity.
 */

const CSS = `
.tasty-treerow {
  display: flex;
  align-items: center;
  gap: var(--tasty-tree-row-gap);
  height: var(--tasty-control-height-tree);
  padding: 0 var(--tasty-space-sm) 0 var(--tasty-space-xs);
  border-radius: var(--tasty-radius-sm);
  color: var(--tasty-text-secondary);
  font-family: var(--tasty-font-ui);
  font-size: var(--tasty-font-size-body);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition: background var(--tasty-motion-ui-fast) var(--tasty-ease-ui), color var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-treerow:hover { background: var(--tasty-overlay-hover); color: var(--tasty-text-secondary-raised); }
.tasty-treerow:hover .tasty-treerow__meta { color: var(--tasty-text-muted-raised); }
.tasty-treerow--selected { background: var(--tasty-tree-row-bg-active); color: var(--tasty-tree-row-fg-active); }
.tasty-treerow--selected:hover { background: var(--tasty-tree-row-bg-active); color: var(--tasty-text-on-active-hover); }
.tasty-treerow--selected:hover .tasty-treerow__meta { color: var(--tasty-text-on-active-hover); }
.tasty-treerow--selected .tasty-treerow__chevron, .tasty-treerow--selected .tasty-treerow__meta { color: var(--tasty-tree-row-part-fg-active); }
.tasty-treerow__chevron {
  flex: none; width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); display: inline-flex; align-items: center; justify-content: center;
  color: var(--tasty-text-secondary); transition: transform var(--tasty-motion-ui-fast) var(--tasty-ease-ui);
}
.tasty-treerow__chevron svg { width: var(--tasty-icon-size-xs); height: var(--tasty-icon-size-xs); }
.tasty-treerow__chevron--open { transform: rotate(90deg); }
.tasty-treerow__chevron--leaf { visibility: hidden; }
.tasty-treerow__icon { flex: none; display: inline-flex; color: var(--tasty-text-secondary); }
.tasty-treerow__icon svg { width: var(--tasty-icon-size-sm); height: var(--tasty-icon-size-sm); display: block; }
.tasty-treerow--selected .tasty-treerow__icon { color: var(--tasty-tree-row-icon-fg-active); }
.tasty-treerow__label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.tasty-treerow__meta { flex: none; font-family: var(--tasty-font-mono); font-size: var(--tasty-tree-row-meta-font-size); color: var(--tasty-text-muted); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-treerow-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function TreeRow({
  label,
  icon = null,
  level = 0,
  expandable = false,
  open = false,
  selected = false,
  meta = null,
  className = "",
  ...rest
}) {
  ensureCss();
  const cls = ["tasty-treerow", selected ? "tasty-treerow--selected" : "", className].filter(Boolean).join(" ");
  const chevCls = [
    "tasty-treerow__chevron",
    !expandable ? "tasty-treerow__chevron--leaf" : "",
    open ? "tasty-treerow__chevron--open" : "",
  ].filter(Boolean).join(" ");
  return (
    <div className={cls} style={{ paddingLeft: `calc(var(--tasty-space-xs) + ${level} * var(--tasty-tree-row-indent))` }} {...rest}>
      <span className={chevCls}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
          <path d="m9 6 6 6-6 6" />
        </svg>
      </span>
      {icon && <span className="tasty-treerow__icon">{icon}</span>}
      <span className="tasty-treerow__label">{label}</span>
      {meta != null && <span className="tasty-treerow__meta">{meta}</span>}
    </div>
  );
}
