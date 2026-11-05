import React from "react";

/**
 * Tasty Kbd — renders a keyboard shortcut. Keybindings are a
 * first-class concept in Tasty (everything is rebindable), so this
 * appears throughout settings, menus, and the command palette.
 */

const CSS = `
.tasty-kbd { display: inline-flex; align-items: center; gap: var(--tasty-kbd-gap); }
.tasty-kbd kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: var(--tasty-kbd-size);
  height: var(--tasty-kbd-size);
  padding: 0 var(--tasty-kbd-padding-x);
  border: var(--tasty-border-width) solid var(--tasty-border-strong);
  border-bottom-width: var(--tasty-kbd-shadow-depth);
  border-radius: var(--tasty-radius-sm);
  background: var(--tasty-surface-raised);
  font-family: var(--tasty-font-mono);
  font-size: var(--tasty-kbd-font-size);
  font-weight: var(--tasty-font-weight-medium);
  line-height: var(--tasty-line-height-tight);
  color: var(--tasty-kbd-fg);
}
.tasty-kbd__plus { color: var(--tasty-kbd-plus-fg); font-size: var(--tasty-font-size-micro); font-family: var(--tasty-font-mono); }
.tasty-kbd[data-ground="active"] .tasty-kbd__plus { color: var(--tasty-kbd-plus-fg-active); }
`;

let injected = false;
function ensureCss() {
  if (injected || typeof document === "undefined") return;
  const el = document.createElement("style");
  el.id = "tasty-kbd-css";
  el.textContent = CSS;
  document.head.appendChild(el);
  injected = true;
}

export function Kbd({ keys = [], ground, className = "", ...rest }) {
  ensureCss();
  const list = Array.isArray(keys) ? keys : String(keys).split("+").map((s) => s.trim());
  return (
    <span className={["tasty-kbd", className].filter(Boolean).join(" ")} data-ground={ground} {...rest}>
      {list.map((k, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span className="tasty-kbd__plus">+</span>}
          <kbd>{k}</kbd>
        </React.Fragment>
      ))}
    </span>
  );
}
