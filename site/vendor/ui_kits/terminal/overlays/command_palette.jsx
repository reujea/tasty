import React from "react";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { MenuItem } from "../../../components/navigation/MenuItem.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim } from "./shared.jsx";

// Tasty UI kit — command palette popup (⌘K / Ctrl+K, also Tools › Command palette…).
// Mirrors zilhak/tasty → src/adapters/ui/popup/command_palette.rs
//   Popup: id "command_palette", CenteredFocused, sticky_focus.
//   Size = --tasty-palette-width (540) wide; list CONTENT cap = --tasty-palette-list-max-height (320, on-scale);
//   the list box adds space-sm top + bottom, so its border-box maxHeight = 320 + 2 × space-sm (2026-10-06).
//   List padding space-sm on all four sides — no separate list↔footer gap (the host's 6 is retired). Search padding space-md is canonical. Source now matches:
//   defs.rs:136 = 540×360 and command_palette.rs:154 list ScrollArea
//   max_height 320 — reconcile complete.
//
// ICON SCOPE (design canonical): commands are generated dynamically from ~47
// binding fields in the real app. Only the 6 commands that map to a real
// binding field carry a SPECIFIC icon (new workspace / new terminal / new
// markdown / split pane / clipboard history / settings); every other dynamic
// command falls back to `ic.command`. The 2 rows here without a binding field
// (Toggle Theme / Listening ports…) are ILLUSTRATIVE only
// — they won't appear in the live palette; they show intended iconography.



// fallback glyph for any command without a specific icon (the ~41 other fields)
const icCommand = <Icon name="command" />;

function CommandPalette({ onClose, onRun }) {
  const [q, setQ] = React.useState("");
  const all = [
    { label: "New Workspace", sc: "Ctrl+Shift+N", icon: ic.plus, bound: true },
    { label: "New Terminal", sc: "Ctrl+T", icon: ic.term, bound: true },
    { label: "New Markdown…", sc: "Ctrl+Shift+M", icon: ic.md, bound: true },
    { label: "Split Pane Vertical", sc: "Ctrl+D", icon: ic.split, bound: true },
    { label: "Clipboard History", sc: "Ctrl+Shift+V", icon: <Icon name="copy" />, bound: true },
    { label: "Settings", sc: "Ctrl+,", icon: ic.settings, bound: true },
    // illustrative-only (no binding field — not in the live palette):
    { label: "Toggle Theme (Mocha / Latte)", sc: "", icon: <Icon name="theme" />, bound: false },
    { label: "Listening ports…", sc: "", icon: <Icon name="port" />, bound: false },
    // example of a dynamic command with no specific icon → fallback glyph:
    { label: "Toggle Clipboard Viewer", sc: "", icon: icCommand, bound: true },
  ];
  const items = all.filter((i) => i.label.toLowerCase().includes(q.toLowerCase()));
  return (
    <Scrim onClose={onClose} align="top">
      <div style={{ width: "var(--tasty-palette-width)", background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
        borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
        <div style={{ padding: "var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <Input block ground="raised" autoFocus icon={ic.search} placeholder="Type to search commands…"
            value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div className="tasty-scroll" style={{ padding: "var(--tasty-space-sm)", maxHeight: "calc(var(--tasty-palette-list-max-height) + 2 * var(--tasty-space-sm))", overflow: "auto" }}>
          {items.length === 0 && <div style={{ padding: 14, fontSize: 13, color: "var(--tasty-text-muted)" }}>No matching commands</div>}
          {items.map((i, n) => (
            <MenuItem key={i.label} label={i.label} icon={i.icon}
              shortcut={i.sc ? <Kbd keys={i.sc} /> : null} active={n === 0 && q !== ""}
              onClick={() => onRun(i.label)} />
          ))}
        </div>
        <div style={{ display: "flex", gap: 14, padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)",
          fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted-raised)" }}>
          <span>↑↓ navigate</span><span>↵ run</span><span>esc close</span>
        </div>
      </div>
    </Scrim>
  );
}

export { CommandPalette };
