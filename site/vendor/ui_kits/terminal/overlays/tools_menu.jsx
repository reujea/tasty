import React from "react";
import { MenuItem } from "../../../components/navigation/MenuItem.jsx";

// Tasty UI kit — tools menu popup (sidebar "Tools" button).
// Mirrors zilhak/tasty → src/adapters/ui/tools_menu.rs (+ sidebar/tools.rs anchor math).
//   PopupDef: id "tools_menu", 160px wide, headless, close_on_outside_click.


// ── Tools menu — mirrors src/adapters/ui/tools_menu.rs ──────────────────
// 160px popup anchored ABOVE the sidebar Tools button, left-aligned with it
// (pos = btn.min.x, btn.min.y - menu_height). 28px rows, no icons, no scrim.
// Built-in entries first, then a separator, then plugin-contributed tools
// (clipboard-viewer contributes "Clipboard Viewer"; git-viewer contributes "Git").
// Built-in order mirrors src/adapters/ui/tools_menu.rs:48-69 — Remote connections…
// is the 3rd entry, between Listening ports… and Presets. Seven built-ins (2026-10-07,
// = BUILTIN_TOOLS); every label that opens a window ends in the ellipsis character.
const TOOLS_BUILTIN = [
  { id: "palette", label: "Command palette…" },
  { id: "ports", label: "Listening ports…" },
  { id: "remote", label: "Remote connections…" },
  { id: "presets", label: "Presets" },
  { id: "tutorial", label: "Tutorial…" },
  { id: "dags", label: "Task DAGs" },
  { id: "open", label: "Open File…" },
];
const TOOLS_PLUGIN = [
  { id: "clipboard", label: "Clipboard Viewer" },
  { id: "git", label: "Git" },
];

function ToolsMenu({ anchor, onClose, onAction }) {
  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  const Row = ({ item }) => (
    <MenuItem label={item.label} onClick={() => onAction(item.id)}
      style={{ color: undefined }} className="tasty-toolsmenu-item" />
  );
  return (
    <div onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}
      style={{ position: "absolute", inset: 0, zIndex: 55 }}>
      <style>{`.tasty-toolsmenu-item{color:var(--tasty-menu-item-fg)}
.tasty-toolsmenu-item:hover{color:var(--tasty-menu-item-fg-hover)}`}</style>
      <div onClick={(e) => e.stopPropagation()} role="menu" aria-label="Tools"
        style={{ position: "absolute", left: anchor.left,
          bottom: window.innerHeight - anchor.top + 2, width: 160,
          background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
          borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-popup-content-margin)",
          boxShadow: "var(--tasty-shadow-popover)" }}>
        {TOOLS_BUILTIN.map((item) => <Row key={item.id} item={item} />)}
        <MenuItem separator />
        {TOOLS_PLUGIN.map((item) => <Row key={item.id} item={item} />)}
      </div>
    </div>
  );
}

export { ToolsMenu };
