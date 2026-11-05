import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as DsIcon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { ListCtrl } from "../../../components/data/ListCtrl.jsx";
import { HelpHint } from "../../../components/feedback/HelpHint.jsx";
import { Checkbox } from "../../../components/forms/Checkbox.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Select } from "../../../components/forms/Select.jsx";
import { Switch } from "../../../components/forms/Switch.jsx";
import { DrillDown } from "../../../components/navigation/DrillDown.jsx";
import { ic, Icon } from "../chrome.jsx";
import { FileDetectorsPane, FileHandlersPane } from "./file_handlers.jsx";
import { KbImportExportSubtab } from "./kb_import_export.jsx";
import { KbPluginsSubtab } from "./kb_plugins_subtab.jsx";
import { MacPermissionsPane } from "./macos_permissions.jsx";
import { AnomalySubtab } from "./settings_anomaly.jsx";
import { Scrim } from "./shared.jsx";

// Tasty UI kit — Settings window (L1 top tabs + L2 sidebar + content).
// Mirrors zilhak/tasty → src/view/settings/ui.rs (+ settings/ui/tabs/*, keybindings_tab.rs).



function ThemeSwatch({ id, label, colors, active, onClick }) {
  return (
    <button onClick={onClick} style={{ textAlign: "left", padding: 0, border: active ? "var(--tasty-border-width) solid var(--tasty-accent-primary)" : "var(--tasty-border-width) solid var(--tasty-border-default)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", cursor: "pointer", background: "transparent",
      boxShadow: active ? "0 0 0 var(--tasty-border-width) var(--tasty-accent-primary)" : "none" }}>
      <div style={{ display: "flex", height: 38 }}>
        {colors.map((c, i) => <div key={i} style={{ flex: 1, background: c }} />)}
      </div>
      <div style={{ padding: "var(--tasty-space-sm) var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", fontSize: 12, color: "var(--tasty-text-primary)" }}>{label}</div>
    </button>
  );
}

function KeyRow({ action, keys }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)",
      /* b10: no divider — rows sit kb-row-gap (8) apart in the parent */ }}>
      <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-text-secondary)" }}>{action}</span>
      <Kbd keys={keys} />
    </div>
  );
}

// ── Theme-color override picker ────────────────────────────────────────
// Edits AppearanceSettings.theme_overrides (a flat PartialColors). Every row
// maps 1:1 to a PartialColors field: checked "Default" = field None (follows the
// preset base); unchecked = Some(hex). Switching presets clears all of these.
// base hexes below are Catppuccin Mocha — the swatch shows the base (dimmed)
// while a row is on Default, the override value once edited.
const PALETTE_GROUPS = [
  { name: "Surfaces", open: true, note: "Elevation ramp", colors: [
    ["crust", "#11111b"], ["mantle", "#181825"], ["base", "#1e1e2e"],
    ["surface0", "#313244"], ["surface1", "#45475a"], ["surface2", "#585b70"]] },
  { name: "Overlays", open: false, colors: [
    ["overlay0", "#6c7086"], ["overlay1", "#7f849c"], ["overlay2", "#9399b2"]] },
  { name: "Text", open: true, colors: [
    ["text", "#cdd6f4"], ["subtext1", "#bac2de"], ["subtext0", "#a6adc8"], ["placeholder", "#6c7086"]] },
  { name: "Accents", open: true, colors: [
    ["blue", "#89b4fa"], ["green", "#a6e3a1"], ["red", "#f38ba8"], ["yellow", "#f9e2af"],
    ["peach", "#fab387"], ["mauve", "#cba6f7"], ["teal", "#94e2d5"], ["sky", "#89dceb"],
    ["lavender", "#b4befe"], ["flamingo", "#f2cdcd"], ["pink", "#f5c2e7"], ["maroon", "#eba0ac"],
    ["rosewater", "#f5e0dc"]] },
  { name: "Terminal-specific", open: false, colors: [
    ["selection_bg", "#585b70"], ["vi_cursor_bg", "#f5e0dc"],
    ["search_match_bg", "#fab387"], ["search_match_active_bg", "#a6e3a1"],
    // 2026-10-12: 5th row, right after the fill it sits on. Same swatch as every row (it is one colour value); Reset all clears it too.
    ["search_match_active_fg", "#1e1e2e"]] },
  { name: "ANSI 16", open: false, colors: [
    ["ansi_black", "#45475a"], ["ansi_red", "#f38ba8"], ["ansi_green", "#a6e3a1"], ["ansi_yellow", "#f9e2af"],
    ["ansi_blue", "#89b4fa"], ["ansi_magenta", "#f5c2e7"], ["ansi_cyan", "#94e2d5"], ["ansi_white", "#bac2de"],
    ["ansi_bright_black", "#585b70"], ["ansi_bright_red", "#f38ba8"], ["ansi_bright_green", "#a6e3a1"], ["ansi_bright_yellow", "#f9e2af"],
    ["ansi_bright_blue", "#89b4fa"], ["ansi_bright_magenta", "#f5c2e7"], ["ansi_bright_cyan", "#94e2d5"], ["ansi_bright_white", "#a6adc8"]] },
];

function ColorOverridePicker() {
  // overrides: { field: hex }. Seeded with a few to show the overridden state.
  const [overrides, setOverrides] = React.useState({ base: "#181926", surface0: "#363a4f", blue: "#74c7ec" });
  const [collapsed, setCollapsed] = React.useState(() => {
    const c = {}; PALETTE_GROUPS.forEach((g) => { c[g.name] = !g.open; }); return c;
  });
  const count = Object.keys(overrides).length;
  const setField = (f, v) => setOverrides((o) => ({ ...o, [f]: v }));
  const toggle = (f, base) => setOverrides((o) => {
    const n = { ...o }; if (f in n) delete n[f]; else n[f] = base; return n;
  });
  const resetGroup = (g) => setOverrides((o) => {
    const n = { ...o }; g.colors.forEach(([f]) => delete n[f]); return n;
  });
  const headStyle = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
    letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12, marginBottom: 4 }}>
        <p style={{ flex: 1, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: 0, lineHeight: "var(--tasty-line-height-ui)" }}>
          Override individual colors of the current preset. Unchecking <b style={{ color: "var(--tasty-text-secondary)" }}>Default</b> on a row lets you edit it; switching presets clears every override.
        </p>
        <span style={{ flex: "none" }}><Button variant="ghost" size="sm" disabled={count === 0} onClick={() => setOverrides({})}>{count ? "Reset all (" + count + ")" : "Reset all"}</Button></span>
      </div>
      {PALETTE_GROUPS.map((g) => {
        const open = !collapsed[g.name];
        const ovN = g.colors.filter(([f]) => f in overrides).length;
        return (
          <div key={g.name} style={{ borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", paddingTop: 6 }}>
            <div onClick={() => setCollapsed((c) => ({ ...c, [g.name]: !c[g.name] }))}
              style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", cursor: "pointer", minHeight: 22 }}>
              <Icon name={open ? "chevronDown" : "chevronRight"} size={14} />
              <span style={headStyle}>{g.name}</span>
              {g.note && <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>· {g.note}</span>}
              {ovN > 0 && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-accent-primary-raised)" }}>{ovN} changed</span>}
              <span style={{ marginLeft: "auto" }}>
                {ovN > 0 && <Button variant="ghost" size="sm" onClick={(e) => { e.stopPropagation(); resetGroup(g); }}>Reset</Button>}
              </span>
            </div>
            {open && (
              <div style={{ display: "flex", flexDirection: "column", gap: 2, padding: "var(--tasty-space-xs) 0 var(--tasty-space-sm)" }}>
                {g.colors.map(([field, base]) => {
                  const ov = field in overrides;
                  const val = ov ? overrides[field] : base;
                  return (
                    <div key={field} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: 28 }}>
                      <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%", flex: "none",
                        background: ov ? "var(--tasty-accent-primary)" : "transparent" }} />
                      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 12,
                        color: ov ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{field}</span>
                      {/* Default (no override) = readOnly, not disabled: the base hex is information (2026-09-29) */}
                      <Input mono value={val} readOnly={!ov} onChange={(e) => setField(field, e.target.value)} style={{ width: "var(--tasty-field-width-xs)" }} />
                      <span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", flex: "none", background: val,
                        border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} />
                      <Checkbox label="Default" checked={!ov} onChange={() => toggle(field, base)} />
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── Scripts (Misc › Scripts) ───────────────────────────────────────────
// Lua scripts registered for shortcut-triggered execution (docs/features/lua-hooks/index.md). Each row:
// display name, absolute path (middle-elided), bound shortcut (or Unbound), a
// "Changed" badge when the on-disk file no longer matches the SHA recorded at
// registration (TOFU — a re-confirm is required on next run), and an Auto-run
// trigger row: chips for the host-lifecycle events the script is bound to fire on,
// plus an add control. Shortcut binding is owned by the Keybindings tab; auto-run
// bindings are edited inline here.
// glyph names from the canonical set (icons/*.svg via <Icon name>)
const SD = {
  edit: "edit",
  trash: "trash",
  kbd: "keyboard",
  folder: "folder",
  warn: "alertTriangle",
  script: "scriptFile",
  x: "close",
  caret: "chevronDown",
};

// Host-lifecycle event whitelist a script can auto-run on (docs/features/lua-hooks/index.md). Named
// `<noun>.<action>.<phase>`; `.pre` fires before the action, `.post` after.
// The add control only offers events not already bound to that script.
const LIFECYCLE_EVENTS = [
  "app.ready.post",
  "window.create.post", "window.close.pre",
  "tab.create.post", "tab.close.pre",
  "pane.create.post", "pane.split.post", "pane.close.pre",
  "surface.focus.post",
  "session.start.post", "session.end.pre",
  "clipboard.copy.post", "clipboard.paste.pre",
];

const SEED_SCRIPTS = [
  { id: "s1", name: "Reformat JSON", path: "~/.tasty/scripts/reformat-json.lua", shortcut: "Ctrl+Shift+J", changed: false, triggers: ["clipboard.copy.post"] },
  { id: "s2", name: "Tail & highlight errors", path: "~/.tasty/scripts/tail-errors.lua", shortcut: "", changed: false, triggers: ["pane.create.post", "session.start.post"] },
  { id: "s3", name: "Deploy staging", path: "~/work/ops/tasty/deploy-staging.lua", shortcut: "Ctrl+Alt+D", changed: true, triggers: [] },
];

function ScriptChangedBadge() {
  return (
    <span title="File changed since registration — you'll be asked to confirm on next run."
      style={{ display: "inline-flex", alignItems: "center", gap: 4, height: 16, padding: "0 var(--tasty-space-sm)",
        borderRadius: "var(--tasty-radius-sm)", whiteSpace: "nowrap", fontFamily: "var(--tasty-font-mono)",
        fontSize: "var(--tasty-font-size-micro)", fontWeight: 500, lineHeight: 1, color: "var(--tasty-accent-warning-raised)",
        border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
        background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>
      <span style={{ display: "inline-flex" }}><Icon name={SD.warn} size={12} /></span>changed
    </span>
  );
}

// Auto-run trigger chip — same 16px geometry as the changed badge. The whole
// chip is the remove affordance (mono event name + a close glyph); hover tints
// it with the 12% foreground overlay and lifts the glyph to text-secondary.
function TriggerChip({ event, onRemove }) {
  const [hover, setHover] = React.useState(false);
  return (
    <button type="button" onClick={onRemove} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      title={"Remove auto-run trigger — " + event}
      style={{ appearance: "none", cursor: "pointer", flex: "none", display: "inline-flex", alignItems: "center", gap: 4,
        height: 16, padding: "0 var(--tasty-space-xs)", borderRadius: "var(--tasty-radius-sm)",
        borderStyle: "solid", borderWidth: "var(--tasty-border-width)", borderColor: "var(--tasty-border-default)",
        background: hover ? "var(--tasty-overlay-active)" : "transparent", whiteSpace: "nowrap",
        fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", lineHeight: 1, color: "var(--tasty-text-secondary)" }}>
      <span>{event}</span>
      <span style={{ display: "inline-flex", color: hover ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)" }}><Icon name={SD.x} size={12} /></span>
    </button>
  );
}

// The 4th row of a ScriptRow's centre column: an "Auto-run:" caption, the bound
// trigger chips, and a dashed add control opening a menu of the still-available
// events. Empty `triggers` still shows the caption + add control so the affordance
// is discoverable. 2026-10-07: dashed edge = border-dash / border-dash-gap (4 / 4,
// corners solid); when every event is bound the control stays, DISABLED (not hidden).
function TriggerRow({ triggers, onAdd, onRemove }) {
  const [open, setOpen] = React.useState(false);
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (!open) return;
    const onDoc = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);
  const available = LIFECYCLE_EVENTS.filter((e) => !triggers.includes(e));
  const none = available.length === 0;
  return (
    <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-xs)", marginTop: 2 }}>
      <span style={{ flex: "none", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Auto-run:</span>
      {triggers.map((e) => <TriggerChip key={e} event={e} onRemove={() => onRemove(e)} />)}
      <div ref={ref} style={{ position: "relative", flex: "none" }}>
        <button type="button" disabled={none} onClick={() => setOpen((v) => !v)}
          title={none ? "All events already bound" : "Add an auto-run trigger"}
          style={{ appearance: "none", cursor: none ? "default" : "pointer", display: "inline-flex", alignItems: "center", gap: 4,
            height: 16, padding: "0 var(--tasty-space-xs)", borderRadius: "var(--tasty-radius-sm)",
            borderStyle: "dashed", borderWidth: "var(--tasty-border-width)", borderColor: "var(--tasty-border-default)",
            background: open ? "var(--tasty-overlay-active)" : "transparent", whiteSpace: "nowrap",
            fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", lineHeight: 1, color: none ? "var(--tasty-state-disabled-fg)" : "var(--tasty-text-muted)" }}>
          <span>Add trigger…</span><span style={{ display: "inline-flex" }}><Icon name={SD.caret} size={12} /></span>
        </button>
        {open && !none && (
          <div role="menu" className="tasty-scroll"
            style={{ position: "absolute", top: "calc(100% + var(--tasty-space-xs))", left: 0, zIndex: 40, minWidth: "var(--tasty-trigger-menu-min-width)", maxHeight: "var(--tasty-trigger-menu-max-height)", overflowY: "auto",
              background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
              borderRadius: "var(--tasty-menu-radius)", boxShadow: "var(--tasty-shadow-popover)", padding: "var(--tasty-popup-content-margin)" }}>
            {available.map((e) => (
              <button key={e} type="button" role="menuitem" onClick={() => { onAdd(e); setOpen(false); }}
                onMouseEnter={(ev) => { ev.currentTarget.style.background = "var(--tasty-overlay-hover)"; ev.currentTarget.style.color = "var(--tasty-text-primary)"; }}
                onMouseLeave={(ev) => { ev.currentTarget.style.background = "transparent"; ev.currentTarget.style.color = "var(--tasty-text-secondary)"; }}
                style={{ appearance: "none", cursor: "pointer", display: "block", width: "100%", textAlign: "left", border: 0,
                  background: "transparent", padding: "0 var(--tasty-space-sm)", height: "var(--tasty-menu-item-height)", lineHeight: "var(--tasty-menu-item-height)",
                  borderRadius: "var(--tasty-radius-sm)", whiteSpace: "nowrap",
                  fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-secondary)" }}>{e}</button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// path shown head + full filename, elided in the middle (dir tail truncates first)
function ScriptPath({ path }) {
  const i = path.lastIndexOf("/");
  const dir = i >= 0 ? path.slice(0, i + 1) : "";
  const file = i >= 0 ? path.slice(i + 1) : path;
  return (
    <span style={{ display: "flex", minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
      <span style={{ flex: "0 1 auto", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }}>{dir}</span>
      <span style={{ flex: "none", color: "var(--tasty-text-secondary)" }}>{file}</span>
    </span>
  );
}

function ScriptRow({ s, renaming, confirming, onStartRename, onCommitRename, onCancelRename, onDelete, onConfirmDelete, onCancelDelete, onAddTrigger, onRemoveTrigger }) {
  const [name, setName] = React.useState(s.name);
  React.useEffect(() => { if (renaming) setName(s.name); }, [renaming, s.name]);
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-sm) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 2, color: "var(--tasty-text-muted)" }}><Icon name={SD.script} size={16} /></span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        {/* row 1 — name (or inline rename) + changed badge */}
        {renaming ? (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
            <Input block autoFocus value={name} onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") onCommitRename(name); if (e.key === "Escape") onCancelRename(); }} />
            <Button variant="primary" size="sm" onClick={() => onCommitRename(name)}>Save</Button>
            <Button variant="ghost" size="sm" onClick={onCancelRename}>Cancel</Button>
          </div>
        ) : (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
            <span style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.name}</span>
            {s.changed && <ScriptChangedBadge />}
          </div>
        )}
        {/* row 2 — path */}
        {!renaming && <ScriptPath path={s.path} />}
        {/* row 3 — changed help text */}
        {!renaming && s.changed && (
          <span style={{ fontSize: 11, color: "var(--tasty-accent-warning-raised)", lineHeight: "var(--tasty-line-height-ui)" }}>
            File changed since registration — you'll be asked to confirm on next run.
          </span>
        )}
        {/* row 4 — auto-run triggers (host-lifecycle event bindings) */}
        {!renaming && <TriggerRow triggers={s.triggers || []} onAdd={onAddTrigger} onRemove={onRemoveTrigger} />}
      </div>
      {/* right — shortcut + actions (or delete confirm) */}
      {!renaming && (confirming ? (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none" }}>
          <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)" }}>Remove?</span>
          <Button variant="ghost" size="sm" onClick={onCancelDelete}>Cancel</Button>
          <Button variant="secondary" size="sm" style={{ color: "var(--tasty-accent-danger-raised)" }} onClick={onConfirmDelete}>Remove</Button>
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none" }}>
          {s.shortcut
            ? <Kbd keys={s.shortcut} />
            : <span style={{ fontSize: 12, color: "var(--tasty-text-disabled)", fontStyle: "italic" }}>Unbound</span>}
          <IconButton size="sm" aria-label="Bind shortcut" title="Bind shortcut (Keybindings)"><Icon name={SD.kbd} size={16} /></IconButton>
          <IconButton size="sm" aria-label="Rename" title="Rename" onClick={onStartRename}><Icon name={SD.edit} size={16} /></IconButton>
          <IconButton size="sm" aria-label="Remove" title="Remove" onClick={onDelete}><Icon name={SD.trash} size={16} /></IconButton>
        </div>
      ))}
    </div>
  );
}

function ScriptManager() {
  const [scripts, setScripts] = React.useState(SEED_SCRIPTS);
  const [adding, setAdding] = React.useState(false);
  const [renameId, setRenameId] = React.useState(null);
  const [confirmId, setConfirmId] = React.useState(null);
  const [draftPath, setDraftPath] = React.useState("");
  const [draftName, setDraftName] = React.useState("");

  const startAdd = () => { setDraftPath(""); setDraftName(""); setAdding(true); setRenameId(null); setConfirmId(null); };
  const browse = () => { setDraftPath("~/.tasty/scripts/new-script.lua"); if (!draftName) setDraftName("New script"); };
  const commitAdd = () => {
    if (!draftPath.trim()) return;
    setScripts((xs) => [...xs, { id: "s" + Date.now(), name: draftName.trim() || draftPath.split("/").pop(),
      path: draftPath.trim(), shortcut: "", changed: false, triggers: [] }]);
    setAdding(false);
  };
  const commitRename = (id, name) => { setScripts((xs) => xs.map((x) => (x.id === id ? { ...x, name: name.trim() || x.name } : x))); setRenameId(null); };
  const remove = (id) => { setScripts((xs) => xs.filter((x) => x.id !== id)); setConfirmId(null); };
  const addTrigger = (id, ev) => setScripts((xs) => xs.map((x) => (x.id === id ? { ...x, triggers: [...(x.triggers || []), ev] } : x)));
  const removeTrigger = (id, ev) => setScripts((xs) => xs.map((x) => (x.id === id ? { ...x, triggers: (x.triggers || []).filter((t) => t !== ev) } : x)));

  const headStyle = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
    letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };

  return (
    <>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>Scripts</div>
          <p style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: "2px 0 0", maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>
            Register and manage Lua scripts you can run with a shortcut. Binding a trigger is done in <b style={{ color: "var(--tasty-text-secondary)" }}>Keybindings</b>; each script is verified against the SHA recorded when it was added.
          </p>
        </div>
        <span style={{ flex: "none" }}><Button variant="secondary" size="sm" leadingIcon={ic.plus} onClick={startAdd}>Add script</Button></span>
      </div>

      {/* add form */}
      {adding && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)",
          border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)" }}>
          <span style={headStyle}>New script</span>
          <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)" }}>
            <span style={{ width: 100, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)" }}>File:</span>
            <Input block mono placeholder="~/.tasty/scripts/my-script.lua" value={draftPath} onChange={(e) => setDraftPath(e.target.value)} />
            <Button variant="secondary" size="sm" onClick={browse}><span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}><Icon name={SD.folder} size={14} />Browse…</span></Button>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)" }}>
            <span style={{ width: 100, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)" }}>Display name:</span>
            <Input block placeholder="optional — defaults to file name" value={draftName} onChange={(e) => setDraftName(e.target.value)} />
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <Button variant="ghost" size="sm" onClick={() => setAdding(false)}>Cancel</Button>
            <Button variant="primary" size="sm" onClick={commitAdd}>Add script</Button>
          </div>
        </div>
      )}

      {/* list / empty */}
      {scripts.length === 0 && !adding ? (
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)",
          textAlign: "center", padding: "var(--tasty-space-xl) 0", color: "var(--tasty-text-muted)" }}>
          <Icon name={SD.script} size={26} />
          <div style={{ fontSize: 14, color: "var(--tasty-text-secondary)" }}>No scripts registered</div>
          <p style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: 0, maxWidth: "var(--tasty-measure-sm)", lineHeight: "var(--tasty-line-height-ui)" }}>
            Click <b style={{ color: "var(--tasty-text-secondary)" }}>Add script</b> to register a Lua script and bind it to a shortcut.
          </p>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column" }}>
          {scripts.map((s) => (
            <ScriptRow key={s.id} s={s}
              renaming={renameId === s.id} confirming={confirmId === s.id}
              onStartRename={() => { setRenameId(s.id); setConfirmId(null); }}
              onCommitRename={(name) => commitRename(s.id, name)}
              onCancelRename={() => setRenameId(null)}
              onDelete={() => { setConfirmId(s.id); setRenameId(null); }}
              onConfirmDelete={() => remove(s.id)}
              onCancelDelete={() => setConfirmId(null)}
              onAddTrigger={(ev) => addTrigger(s.id, ev)}
              onRemoveTrigger={(ev) => removeTrigger(s.id, ev)} />
          ))}
        </div>
      )}
    </>
  );
}

// ── Mouse-capture blacklist (Settings › Terminal › Mouse Capture) ──────
// Process-name patterns where mouse capture is disabled, so clicks/drags are
// handled locally instead of sent to the capturing TUI. Mirrors the gallery
// BlacklistEditorG specimen (Overlays › Banners › Capture blacklist).
function CaptureBlacklist() {
  const [items, setItems] = React.useState(["htop", "vim", "ht*"]);
  const [draft, setDraft] = React.useState("");
  const [hover, setHover] = React.useState(null);
  const add = () => { const v = draft.trim(); if (!v) return; setItems((xs) => (xs.includes(v) ? xs : [...xs, v])); setDraft(""); };
  const remove = (v) => setItems((xs) => xs.filter((x) => x !== v));
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
      {items.length === 0 ? (
        <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", padding: "var(--tasty-space-xs) 0" }}>
          No programs excluded — clicks are sent to capturing apps.
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          {items.map((p) => (
            <div key={p} onMouseEnter={() => setHover(p)} onMouseLeave={() => setHover(null)}
              style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 28, padding: "0 var(--tasty-space-xs) 0 var(--tasty-space-sm)",
                borderRadius: "var(--tasty-radius-sm)", background: hover === p ? "var(--tasty-overlay-hover)" : "transparent" }}>
              <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)" }}>{p}</span>
              <IconButton size="sm" aria-label={"Remove " + p} onClick={() => remove(p)}><Icon name="close" size={16} /></IconButton>
            </div>
          ))}
        </div>
      )}
      <div style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>
        <Input block mono placeholder="process name or pattern, e.g. htop or ht*" value={draft}
          onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") add(); }} />
        <Button variant="secondary" size="sm" disabled={!draft.trim()} onClick={add}>Add</Button>
      </div>
      <p style={{ fontSize: 11, color: "var(--tasty-accent-warning-raised)", margin: 0, lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-md)" }}>
        Case-insensitive substring or <b>*</b> wildcard on the process name. When a listed program is foreground, clicks/drags are handled locally (select / tasty menu); the wheel is still sent.
      </p>
    </div>
  );
}

// ── Handler › Hook Handlers (Settings › Handler › Hook Handlers) ───────
// Registry of inbound-hook handlers fired by the webhook receiver (inbound
// hook server, not yet built). Mirrors the File Handlers subtab
// shape: id · origin (host / plugin / user) · priority · action · status.
// Default action is a shell command whose mapping is edited inline. Scope
// decision (open point): this subtab is the HANDLER REGISTRY only — the
// listener (webhook server bind/port/secret) is managed separately via CLI /
// its own surface, so it is not surfaced here.
const HOOK_ORIGIN = {
  host:   { label: "host",   variant: undefined },
  plugin: { label: "plugin", variant: "agent" },
  user:   { label: "user",   variant: undefined },
};
const SEED_HOOKS = [
  { id: "push.received",   origin: "host",   prio: 10, cmd: 'tasty notify "push → $TASTY_HOOK_REPO"', on: true },
  { id: "pr.opened",       origin: "plugin", prio: 20, cmd: "git-helper pr open --id $TASTY_HOOK_PR", on: true },
  { id: "deploy.finished", origin: "user",   prio: 30, cmd: "~/ops/on-deploy.sh $TASTY_HOOK_ENV", on: false },
  { id: "alert.fired",     origin: "host",   prio: 40, cmd: "tasty pane new --title Alert", on: true },
];

function HookRow({ h, onToggle, onCmd, onDelete }) {
  const o = HOOK_ORIGIN[h.origin] || HOOK_ORIGIN.user;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", /* a switched-off hook stays editable, so it is read: muted id, no opacity */ }}>
      {/* line 1 — id · origin · priority · status · remove */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
        <span style={{ flex: "0 1 auto", fontFamily: "var(--tasty-font-mono)", fontSize: 13, fontWeight: 600, color: h.on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{h.id}</span>
        <Tag variant={o.variant}>{o.label}</Tag>
        <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>prio {h.prio}</span>
        <span style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none" }}>
          <Switch checked={h.on} onChange={onToggle} />
          <IconButton size="sm" aria-label={"Remove " + h.id} title="Remove" onClick={onDelete}><Icon name="trash" size={16} /></IconButton>
        </span>
      </div>
      {/* line 2 — action: default shell command, edited inline */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
        <span style={{ flex: "none", width: 74, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Shell cmd:</span>
        <Input block mono value={h.cmd} disabled={!h.on} onChange={(e) => onCmd(e.target.value)} />
      </div>
    </div>
  );
}

function HookHandlers() {
  const [hooks, setHooks] = React.useState(SEED_HOOKS);
  const [adding, setAdding] = React.useState(false);
  const [draftId, setDraftId] = React.useState("");
  const [draftCmd, setDraftCmd] = React.useState("");
  const toggle = (id) => setHooks((xs) => xs.map((x) => (x.id === id ? { ...x, on: !x.on } : x)));
  const setCmd = (id, cmd) => setHooks((xs) => xs.map((x) => (x.id === id ? { ...x, cmd } : x)));
  const remove = (id) => setHooks((xs) => xs.filter((x) => x.id !== id));
  const startAdd = () => { setDraftId(""); setDraftCmd(""); setAdding(true); };
  const commitAdd = () => {
    const id = draftId.trim(); if (!id) return;
    const maxPrio = hooks.reduce((m, x) => Math.max(m, x.prio), 0);
    setHooks((xs) => [...xs, { id, origin: "user", prio: maxPrio + 10, cmd: draftCmd.trim(), on: true }]);
    setAdding(false);
  };
  const headStyle = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
    letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };

  return (
    <>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
        <p style={{ flex: 1, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: 0, maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>
          Handlers fired when the inbound-hook server receives a matching event. Includes core
          <b style={{ color: "var(--tasty-text-secondary)" }}> host</b> defaults,
          <b style={{ color: "var(--tasty-text-secondary)" }}> plugin</b> contributions, and your own
          <b style={{ color: "var(--tasty-text-secondary)" }}> user</b> mappings; lower priority values run first.
          The webhook <b style={{ color: "var(--tasty-text-secondary)" }}>listener</b> (bind / port / secret) is configured separately.
        </p>
        <span style={{ flex: "none" }}><Button variant="secondary" size="sm" leadingIcon={ic.plus} onClick={startAdd}>Add handler</Button></span>
      </div>

      {adding && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)",
          border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)" }}>
          {/* 2026-10-21 — raised card: labels text-secondary-raised, Inputs ground="raised", Cancel ghost ground="raised" (same fix as the File Handlers add card) */}
          <span style={headStyle}>New hook handler</span>
          <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)" }}>
            <span style={{ width: 100, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary-raised)" }}>Event id:</span>
            <Input block mono ground="raised" placeholder="e.g. pipeline-done" value={draftId} onChange={(e) => setDraftId(e.target.value)} />
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)" }}>
            <span style={{ width: 100, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary-raised)" }}>Shell command:</span>
            <Input block mono ground="raised" placeholder='tasty notify "$TASTY_HOOK_*"' value={draftCmd} onChange={(e) => setDraftCmd(e.target.value)} />
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <Button variant="ghost" size="sm" ground="raised" onClick={() => setAdding(false)}>Cancel</Button>
            <Button variant="primary" size="sm" disabled={!draftId.trim()} onClick={commitAdd}>Add handler</Button>
          </div>
        </div>
      )}

      <div style={headStyle}>Registered hook handlers</div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {hooks.map((h) => (
          <HookRow key={h.id} h={h} onToggle={() => toggle(h.id)} onCmd={(v) => setCmd(h.id, v)} onDelete={() => remove(h.id)} />
        ))}
      </div>
    </>
  );
}

// ── Keybindings › Preset (drill-down: ListCtrl of presets → diff preview) ──
// Was a cramped left-list (120px) + right preview split with an "Apply" button
// floating just above the modal footer. Reworked into a generic DrillDown: the
// preset ListCtrl owns the full content width (List view); selecting one swaps
// the whole area to a Detail view — a back bar (← + preset name) whose RIGHT
// slot holds Apply (kept clear of the footer's Cancel/Save), over the retained
// 기능/이전/이후 (Action / Current / Preset) diff table with changed rows'
// Preset column drawn in accent-primary (color-only emphasis — no bold, per the
// egui weight limit; matches the `{n} changed` accent convention on the
// Appearance override badge). "Apply" writes the selected preset into the draft;
// the already-active preset (no diff). Footer "Save" still commits the whole
// settings draft — a different scope, so the two never sit on the same row.
const KB_PRESETS = [
  { id: "default", name: "Default", desc: "Tasty stock bindings", rows: [
    { action: "Copy", cur: "Ctrl+Shift+C", next: "Ctrl+Shift+C" },
    { action: "Paste", cur: "Ctrl+Shift+V", next: "Ctrl+Shift+V" },
    { action: "New tab", cur: "Ctrl+T", next: "Ctrl+T" },
    { action: "Command palette", cur: "Ctrl+K", next: "Ctrl+K" },
    { action: "Split vertical", cur: "Ctrl+D", next: "Ctrl+D" },
    { action: "Find", cur: "Ctrl+F", next: "Ctrl+F" },
  ] },
  { id: "mac", name: "Mac", desc: "⌘-based, native-app muscle memory", rows: [
    { action: "Copy", cur: "Ctrl+Shift+C", next: "⌘C" },
    { action: "Paste", cur: "Ctrl+Shift+V", next: "⌘V" },
    { action: "New tab", cur: "Ctrl+T", next: "⌘T" },
    { action: "Command palette", cur: "Ctrl+K", next: "⌘K" },
    { action: "Split vertical", cur: "Ctrl+D", next: "⌘D" },
    { action: "Find", cur: "Ctrl+F", next: "⌘F" },
  ] },
  { id: "emacs", name: "Emacs", desc: "C-x / C-c prefix chords", rows: [
    { action: "Copy", cur: "Ctrl+Shift+C", next: "Alt+W" },
    { action: "Paste", cur: "Ctrl+Shift+V", next: "Ctrl+Y" },
    { action: "New tab", cur: "Ctrl+T", next: "Ctrl+X 2" },
    { action: "Command palette", cur: "Ctrl+K", next: "Alt+X" },
    { action: "Split vertical", cur: "Ctrl+D", next: "Ctrl+X 3" },
    { action: "Find", cur: "Ctrl+F", next: "Ctrl+S" },
  ] },
  { id: "vim", name: "Vim", desc: "modal, hjkl pane motions", rows: [
    { action: "Copy", cur: "Ctrl+Shift+C", next: "y" },
    { action: "Paste", cur: "Ctrl+Shift+V", next: "p" },
    { action: "New tab", cur: "Ctrl+T", next: ":tabnew" },
    { action: "Command palette", cur: "Ctrl+K", next: ":" },
    { action: "Split vertical", cur: "Ctrl+D", next: "Ctrl+W v" },
    { action: "Find", cur: "Ctrl+F", next: "/" },
  ] },
];

function PresetDiffTable({ preset, presetName }) {
  const head = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
    letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)",
    padding: "0 var(--tasty-space-md) var(--tasty-space-sm)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" };
  const cell = { padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
    fontSize: 13, display: "flex", alignItems: "center" };
  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(0,1.6fr) 1fr 1fr", alignItems: "stretch" }}>
      <div style={{ ...head, textAlign: "left" }}>Action</div>
      <div style={{ ...head }}>Current</div>
      <div style={{ ...head }}>{presetName}</div>
      {preset.rows.map((r) => {
        const changed = r.cur !== r.next;
        return (
          <React.Fragment key={r.action}>
            <div style={{ ...cell, color: "var(--tasty-text-secondary)" }}>{r.action}</div>
            <div style={{ ...cell, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{r.cur}</div>
            <div style={{ ...cell, fontFamily: "var(--tasty-font-mono)", fontSize: 12,
              color: changed ? "var(--tasty-accent-primary-raised)" : "var(--tasty-text-muted)" }}>{r.next}</div>
          </React.Fragment>
        );
      })}
    </div>
  );
}

function PresetSubtab() {
  const [activeId, setActiveId] = React.useState("default");
  const [view, setView] = React.useState("list");
  const [selId, setSelId] = React.useState(null);
  const sel = KB_PRESETS.find((p) => p.id === selId) || null;
  const isActiveSel = sel && sel.id === activeId;

  const items = KB_PRESETS.map((p) => ({
    id: p.id,
    label: p.name,
    description: p.desc,
    trailing: p.id === activeId ? <Tag variant="success" dot>Active</Tag> : null,
  }));

  const openPreset = (id) => { setSelId(id); setView("detail"); };
  const apply = () => { if (sel) setActiveId(sel.id); };
  const changed = sel ? sel.rows.filter((r) => r.cur !== r.next).length : 0;

  return (
    <DrillDown
      view={view}
      title={sel ? sel.name + " preset" : ""}
      onBack={() => setView("list")}
      actions={
        <Button variant="primary" size="sm" disabled={isActiveSel} onClick={apply}>
          {isActiveSel ? "Applied" : "Apply"}
        </Button>
      }
      detail={sel && (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)" }}>
          <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-md)" }}>
            {isActiveSel
              ? <>This preset is <b style={{ color: "var(--tasty-text-secondary)" }}>currently active</b> — every binding already matches, so there is nothing to apply.</>
              : <><b style={{ color: "var(--tasty-text-secondary)" }}>{changed}</b> of {sel.rows.length} bindings change. <b>Apply</b> writes them into the draft; nothing is saved until you press <b style={{ color: "var(--tasty-text-secondary)" }}>Save</b>.</>}
          </p>
          <PresetDiffTable preset={sel} presetName={sel.name} />
        </div>
      )}
    >
      <div style={{ padding: "var(--tasty-space-md) var(--tasty-space-lg)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-md)" }}>
          Pick a keybinding preset to preview its bindings against your current ones, then apply it. The <b style={{ color: "var(--tasty-text-secondary)" }}>Active</b> preset is the one in effect now.
        </p>
        <ListCtrl items={items} selectedId={activeId} onSelect={openPreset} />
      </div>
    </DrillDown>
  );
}

// ── Settings ───────────────────────────────────────────────────────────
// 7-tab L1 IA (settings-ia-restructure, 2026-06-17). Every category now has a
// home: Terminal behaviour + Performance, FileHandler, and Tastyrc are promoted
// out of the old over-stuffed General group into their own top-level tabs. The
// growable second level (incl. plugin-contributed sections) lives in the left
// sidebar with a filter. Plugins remain their OWN top-level tab.
const IS_WINDOWS = true; // preview assumes Windows. Misc has Tastyrc (Windows-only); empty elsewhere.
// Plugin-contributed Appearance pages are dynamic — present only when a plugin registers
// one, inserted after Terminal / before HTML (mirrors the source push order). Shown with
// the same agent-dot marker the Plugins tab uses.
const APPEARANCE_PLUGIN_PAGES = ["Diff colors"];
const L1_TABS = ["General", "Terminal", "Appearance", "Keybindings", "FileHandler", "Misc", "Plugins"];
// NEW AXIS on the L2 row (the Rust `L2Section` has only a label + is_plugin):
// a section can ask for a SEPARATOR above it. Used by exactly one row today —
// Keybindings › Import / Export, which moves a configuration rather than
// editing bindings, so it reads as a different kind of destination.
const KB_L2_SEPARATED = ["Import / Export"];
// L1 "File Handler" was generalized to "Handler" (inbound-hook server work): the
// same tab now hosts BOTH the file-routing subtabs (prefixed "File …") and the
// new Hook Handlers registry. Internal key stays `FileHandler` to avoid a rename
// migration; only the visible label changed.
const L1_LABEL = { General: "General", Terminal: "Terminal", Appearance: "Appearance",
  Keybindings: "Keybindings", FileHandler: "Handler", Misc: "Misc", Plugins: "Plugins" };
const L2 = {
  // Permissions exists in macOS builds only (last L2 of General). Shown here so the kit carries it.
  General: ["General", "Notifications", "Accessibility", "Permissions"],
  Terminal: ["General", "Mouse Capture", "TUI", "Performance"],
  Appearance: ["Theme", "Colors", "General", "Display", "Tasty", "Terminal", ...APPEARANCE_PLUGIN_PAGES, "HTML"],
  // Keybindings L2 — TRANSCRIBED from src/view/settings/ui.rs:906-950 (eleven
  // sections; the old design list carried a phantom "Image" and was missing
  // Explorer + Scripts). "Import / Export" is appended LAST, after Plugins, and
  // is the one SEPARATED row (see KB_L2_SEPARATED — a new axis on the L2 row).
  Keybindings: ["General", "Workspace", "Pane", "Tab", "Surface", "Clipboard", "Zoom", "Explorer", "Scripts", "Preset", "Plugins", "Import / Export"],
  FileHandler: ["File Extension Mapping", "File Detectors", "File Handlers", "Hook Handlers"],
  // 2026-10-27: Scripts → Task pipeline → Anomaly detection → Tastyrc (Windows only)
  Misc: IS_WINDOWS ? ["Scripts", "Task pipeline", "Anomaly detection", "Tastyrc"] : ["Scripts", "Task pipeline", "Anomaly detection"],
  Plugins: ["git-helper", "ai-review", "docker", "k8s-lens"],
};

// Settings › FileHandler › File Extension Mapping (2026-09-29).
// The product's structure is the design: an extension Input + Add, then per extension an ORDERED
// list of detectors (first match wins). Controls:
//   Add       Button secondary sm — disabled while the input is empty or no detector is installed
//   ▲ / ▼     IconButton sm chevronUp / chevronDown per row (the text glyphs go) — disabled, not
//             hidden, on the top row (▲), the bottom row (▼) and on a row that is not a candidate
//             (its detector is off / not installed: both). Disabled = the shared ink rule.
// Rows keep their slot so the arrows never shift between rows.
const EXT_SEED = [
  { ext: ".md", rows: [["Markdown viewer", true], ["Editor", true], ["html-preview", false]] },
  { ext: ".log", rows: [["Log viewer", true]] },
  { ext: ".ipynb", missing: true, rows: [] },
  { ext: ".html", rows: [["HTML viewer", true], ["Editor", true]] },
];
// 2026-10-06 — group header = .ext · [Tag disabled "not installed"] · spacer · [Reset | Remove] (ghost Button sm),
// min-height button-height-sm. Reset shows only once the order differs from install order (custom); Remove only on a
// not-installed group (no enabled detector supports it), which has no rows. Both edit the draft; no confirm.
function ExtensionMapping({ detectors = true }) {

  const [map, setMap] = React.useState(EXT_SEED);
  const [draft, setDraft] = React.useState("");
  const move = (gi, i, d) => setMap((m) => m.map((g, j) => {
    if (j !== gi) return g;
    const rows = g.rows.slice(); const t = rows[i]; rows[i] = rows[i + d]; rows[i + d] = t; return { ...g, rows, custom: true };
  }));
  const reset = (gi) => setMap((m) => m.map((g, j) => j === gi ? { ...g, custom: false, rows: (EXT_SEED.find((s) => s.ext === g.ext) || g).rows } : g));
  const remove = (gi) => setMap((m) => m.filter((_, j) => j !== gi));
  const add = () => { const e = draft.trim(); if (!e || !detectors) return; setMap((m) => [...m, { ext: e.startsWith(".") ? e : "." + e, rows: [["Editor", true]] }]); setDraft(""); };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
      <div style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>
        <Input block mono placeholder="extension, e.g. .log" value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") add(); }} />
        <Button variant="secondary" size="sm" disabled={!draft.trim() || !detectors} onClick={add}>Add</Button>
      </div>
      {map.map((g, gi) => {
        const cand = g.rows.filter(([, c]) => c).length;
        return (
          <div key={g.ext} style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-button-height-sm)", padding: "var(--tasty-space-xs) 0", borderBottom: g.missing ? "var(--tasty-border-width) solid var(--tasty-separator)" : undefined }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: g.missing ? "var(--tasty-text-disabled)" : "var(--tasty-text-secondary)" }}>{g.ext}</span>
              {g.missing && <Tag>not installed</Tag>}
              <span style={{ flex: 1 }} />
              {g.custom && <Button variant="ghost" size="sm" title="Remove the custom priority for this extension (revert to install order)." onClick={() => reset(gi)}>Reset</Button>}
              {g.missing && <Button variant="ghost" size="sm" onClick={() => remove(gi)}>Remove</Button>}
            </div>
            {g.rows.map(([name, c], i) => (
              <div key={name} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-settings-row-min-height)",
                borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                <span style={{ width: "var(--tasty-space-lg)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{i + 1}</span>
                <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-body)", color: c ? "var(--tasty-text-secondary)" : "var(--tasty-text-disabled)" }}>{name}</span>
                {!c && <Tag>off</Tag>}
                <IconButton size="sm" aria-label="Move up" disabled={!c || i === 0} onClick={() => move(gi, i, -1)}><DsIcon name="chevronUp" /></IconButton>
                <IconButton size="sm" aria-label="Move down" disabled={!c || i >= cand - 1} onClick={() => move(gi, i, 1)}><DsIcon name="chevronDown" /></IconButton>
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

function SettingsWindow({ theme, onTheme, uiScale, onUiScale, onClose }) {
  const [l1, setL1] = React.useState("Appearance");
  const [l2, setL2] = React.useState("Theme");
  const [filter, setFilter] = React.useState("");
  const [toast, setToast] = React.useState(null);
  React.useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2000);
    return () => clearTimeout(t);
  }, [toast]);
  const isPlugins = l1 === "Plugins";
  const isPluginSection = (s) => isPlugins || (l1 === "Appearance" && APPEARANCE_PLUGIN_PAGES.includes(s));
  // The Preset subtab is a self-contained DrillDown that owns its own padding
  // + internal scroll (list full-width, detail table scrolls). It renders
  // full-bleed — outside the standard padded/scrolling content wrapper.
  const fullBleed = l1 === "Keybindings" && (l2 === "Preset" || l2 === "Import / Export");
  const pickL1 = (t) => { setL1(t); setL2(L2[t][0] || null); setFilter(""); };
  const shown = L2[l1].filter((s) => s.toLowerCase().includes(filter.toLowerCase()));

  // 2026-10-08 (batch 8) — label column = the SUBTAB's longest label, clamped to
  // [--tasty-settings-label-width 150, --tasty-settings-label-max-width 240]; longer labels wrap.
  // Gap = --tasty-settings-label-gap 16. The app measures per subtab + locale; the kit hard-codes the
  // en result per subtab in LABEL_COL (General › General clamps to 240: the webhook label wraps).
  const LABEL_COL = { "General/General": "var(--tasty-settings-label-max-width)", "Terminal/TUI": "var(--tasty-settings-label-max-width)" };
  const labelCol = LABEL_COL[l1 + "/" + l2] || "var(--tasty-settings-label-width)";
  // caption / callout that belongs to ONE row sits directly under it (gap --tasty-settings-row-caption-gap 4),
  // at the row's left edge, width measure-md — same as every settings Note.
  const Row = ({ label, hint, caption, children }) => {
    const row = (
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)" }}>
        <span style={{ width: labelCol, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)",
          display: "inline-flex", alignItems: "center", gap: "var(--tasty-help-hint-gap)" }}>
          <span>{label}</span>{hint && <HelpHint label={hint} placement="bottom" />}
        </span>
        {children}
      </div>
    );
    if (!caption) return row;
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)" }}>
        {row}
        {caption}
      </div>
    );
  };
  const RowCaption = ({ children }) => (
    <p style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: 0, maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>{children}</p>
  );
  const WarnCallout = ({ children }) => (
    <div style={{ display: "flex", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderRadius: "var(--tasty-radius)", maxWidth: "var(--tasty-measure-md)",
      border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
      background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 1, color: "var(--tasty-accent-warning-raised)" }}>
        <Icon name="alertTriangle" size={16} />
      </span>
      <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>{children}</p>
    </div>
  );
  const Mono = ({ children }) => (
    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
      letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>{children}</div>
  );
  const Note = ({ children }) => (
    <p style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", margin: 0, maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>{children}</p>
  );

  function body() {
    // ── Appearance ──
    if (l1 === "Appearance") {
      if (l2 === "Theme")
        return (
          <>
            <Mono>Theme preset</Mono>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--tasty-space-sm)" }}>
              <ThemeSwatch label="Catppuccin Mocha" active={theme === "mocha"} onClick={() => onTheme("mocha")}
                colors={["#11111b", "#1e1e2e", "#89b4fa", "#cba6f7", "#a6e3a1"]} />
              <ThemeSwatch label="Catppuccin Latte" active={theme === "latte"} onClick={() => onTheme("latte")}
                colors={["#dce0e8", "#eff1f5", "#1e66f5", "#8839ef", "#40a02b"]} />
            </div>
            <Note>Selecting a preset resets all custom colors. Fine-tune individual colors in the <b style={{ color: "var(--tasty-text-secondary)" }}>Colors</b> section — switching presets clears those overrides.</Note>
          </>
        );
      if (l2 === "Colors")
        return <ColorOverridePicker />;
      if (l2 === "Display")
        return (
          <>
            <Mono>Sidebar scale</Mono>
            <Note>Scales the <b style={{ color: "var(--tasty-text-secondary)" }}>sidebar</b> only — its wordmark, workspace list, and footer. The title bar, tabs, panes, and dialogs are <b style={{ color: "var(--tasty-text-secondary)" }}>unaffected</b>; terminal glyph size is set separately under <span style={{ fontFamily: "var(--tasty-font-mono)" }}>Appearance › General › Font size</span>. UI scale has <b style={{ color: "var(--tasty-text-secondary)" }}>no keyboard shortcut</b> — change it here.</Note>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "var(--tasty-space-sm)", marginTop: 2 }}>
              {[["sm", "Small", "0.8×"], ["md", "Medium", "1.0×"], ["lg", "Large", "1.2×"]].map(([key, label, mult]) => {
                const on = uiScale === key;
                return (
                  <button key={key} onClick={() => onUiScale(key)} style={{
                    appearance: "none", cursor: "pointer", textAlign: "left",
                    border: on ? "var(--tasty-border-width) solid var(--tasty-accent-primary)" : "var(--tasty-border-width) solid var(--tasty-border-default)",
                    boxShadow: on ? "0 0 0 var(--tasty-border-width) var(--tasty-accent-primary)" : "none",
                    background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)", padding: "var(--tasty-space-md) var(--tasty-space-md)",
                    display: "flex", flexDirection: "column", gap: 8 }}>
                    {/* preview: 'Aa' sized by the stop so the difference is visible at md scale */}
                    <span style={{ fontWeight: 600, color: "var(--tasty-text-primary)",
                      fontSize: key === "sm" ? 13 : key === "md" ? 16 : 20, lineHeight: 1 }}>Aa</span>
                    <span style={{ display: "flex", alignItems: "baseline", gap: "var(--tasty-space-sm)" }}>
                      <span style={{ fontSize: 13, color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)" }}>{label}</span>
                      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{mult}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          </>
        );
      if (l2 === "General")
        return (
          <>
            <Row label="Font family:"><Select options={["D2Coding", "JetBrains Mono", "Cascadia Code"]} style={{ width: "var(--tasty-field-width-lg)" }} /></Row>
            <Row label="Font size:"><Input mono defaultValue="14" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>px</span></Row>
            <Row label="Line height:"><Input mono defaultValue="1.2" style={{ width: "var(--tasty-field-width-xs)" }} /></Row>
            <Row label="Ligatures:"><Switch defaultChecked /></Row>
            <Row label="Background opacity:"><input type="range" min="60" max="100" defaultValue="100" style={{ accentColor: "var(--tasty-accent-primary)", width: "var(--tasty-field-width-range)" }} /></Row>
          </>
        );
      if (l2 === "Tasty")
        return (
          <>
            <Mono>Tasty chrome</Mono>
            <Row label="Accent:"><Input mono defaultValue="#89b4fa" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "#89b4fa", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
            <Row label="Sidebar background:"><Input mono defaultValue="#181825" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "#181825", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
            <Row label="Active tab indicator:"><Select options={["Underline", "Fill", "Dot"]} defaultValue="Underline" style={{ width: "var(--tasty-field-width-md)" }} /></Row>
            <Row label="Use theme defaults:"><Switch defaultChecked /></Row>
            <Note>Overrides the Tasty app chrome (sidebar, tabs, title bar) independently of the terminal surface. These map to the same theme overrides as the full <b style={{ color: "var(--tasty-text-secondary)" }}>Colors</b> palette — this is just a curated shortcut. Turn on “Use theme defaults” to follow the selected preset.</Note>
          </>
        );
      if (l2 === "HTML")
        return (
          <>
            <Mono>HTML viewer</Mono>
            <Row label="Default zoom:"><Input mono defaultValue="100" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>%</span></Row>
            <Row label="Color scheme:"><Select options={["Follow theme", "Light", "Dark"]} defaultValue="Follow theme" style={{ width: "var(--tasty-field-width-md)" }} /></Row>
            <Row label="Allow remote content:"><Switch /></Row>
            <Row label="Sandbox scripts:"><Switch defaultChecked /></Row>
            <Note>Controls how the built-in HTML viewer renders previews opened from the terminal. Remote content is blocked by default.</Note>
          </>
        );
      if (APPEARANCE_PLUGIN_PAGES.includes(l2))
        return (
          <>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%", background: "var(--tasty-accent-agent)" }} />
              <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{l2}</span>
              <Tag variant="agent">git-helper</Tag>
            </div>
            <Note>Contributed by the <b style={{ color: "var(--tasty-text-secondary)" }}>git-helper</b> plugin — these controls appear only while the plugin is installed.</Note>
            <Mono>Diff palette</Mono>
            <Row label="Added lines:"><Input mono defaultValue="#a6e3a1" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "#a6e3a1", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
            <Row label="Removed lines:"><Input mono defaultValue="#f38ba8" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "#f38ba8", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
            <Row label="Context dim:"><input type="range" min="0" max="100" defaultValue="40" style={{ accentColor: "var(--tasty-accent-primary)", width: "var(--tasty-field-width-range)" }} /></Row>
          </>
        );
      // Terminal surface colors (fallback)
      return (
        <>
          <Mono>Surface colors — terminal</Mono>
          <Row label="Focused background:"><Input mono defaultValue="#000000" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "var(--tasty-surface-terminal-focused-bg)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
          <Row label="Unfocused background:"><Input mono defaultValue="#1e1e2e" style={{ width: "var(--tasty-field-width-color)" }} /><span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", background: "#1e1e2e", border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} /></Row>
          <Row label="Use default:"><Switch defaultChecked /></Row>
          <Note>Per-kind surface backgrounds. Like the Tasty section, these write to the same theme overrides as the full <b style={{ color: "var(--tasty-text-secondary)" }}>Colors</b> palette.</Note>
        </>
      );
    }

    // ── Terminal (behaviour + performance — promoted from the old General group) ──
    if (l1 === "Terminal") {
      if (l2 === "Performance")
        return (
          <>
            <Mono>Performance — all terminal subsystems</Mono>
            {/* 2026-10-21 (performance-settings-apply-timing): neutral apply note replaces the warning-yellow restart line; the three product rows */}
            <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-settings-apply-note-gap)", maxWidth: "var(--tasty-measure-md)" }}>
              <span style={{ display: "inline-flex", flex: "none", height: "calc(var(--tasty-font-size-caption) * var(--tasty-line-height-ui))", alignItems: "center", color: "var(--tasty-settings-apply-note-glyph)" }}><Icon name="info" size={14} /></span>
              <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-settings-apply-note-fg)" }}>Saved changes apply to terminals you open afterward. Terminals that are already open don't change right away.</p>
            </div>
            <Row label="Targeted PTY polling:" hint="Checks a terminal for output only when it signals new output, instead of checking every terminal."><Switch defaultChecked /></Row>
            <Row label="Scrollback disk swap:" hint="Keeps scrollback past the in-memory limit in a temporary file instead of dropping the oldest lines."><Switch defaultChecked /></Row>
            <Row label="Disk scrollback lines:" hint="Lines kept on disk per terminal while disk swap is on. -1 keeps every line. Separate from Scrollback lines."><span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-number-field-unit-gap)" }}><Input mono defaultValue="10000" style={{ width: "var(--tasty-number-field-min-width)" }} /><span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-number-field-unit-fg)" }}>lines</span></span></Row>
          </>
        );
      // Terminal › Mouse Capture — hint banner toggle + per-program capture opt-out
      if (l2 === "Mouse Capture")
        return (
          <>
            <Row label="Show mouse capture hint:"><Switch defaultChecked /></Row>
            <Note>When a program turns on mouse tracking (DECSET 1000/1002/1003), a banner explains why drag-to-select stopped and how to bypass it by holding <b style={{ color: "var(--tasty-text-secondary)" }}>Shift</b>. Turn this off to suppress that banner.</Note>
            <div style={{ height: "var(--tasty-border-width)", background: "var(--tasty-separator)", margin: "var(--tasty-space-xs) 0" }} />
            <Mono>Let Tasty handle clicks in these programs</Mono>
            <CaptureBlacklist />
          </>
        );
      // Terminal › TUI — OSC 52 clipboard-read gate (security-sensitive)
      if (l2 === "TUI")
        return (
          <>
            {/* 2026-10-08 (batch 8) — the OSC 52 row is ON the grid; its callout is the row's own (measure-md, gap 4). */}
            <Row label="Allow clipboard read (OSC 52):" caption={<WarnCallout>Turning this on lets programs running in the terminal <b style={{ color: "var(--tasty-text-primary)" }}>read your system clipboard</b> via OSC 52. Leave it off unless you trust everything that runs here.</WarnCallout>}><Switch /></Row>
          </>
        );
      // Terminal › General — behaviour
      return (
        <>
          <Row label="Shell:"><Input mono defaultValue="/usr/bin/zsh" style={{ width: "var(--tasty-field-width-lg)" }} /></Row>
          {IS_WINDOWS && <Row label="Shell mode:"><Select options={["Default (full rc)", "Tasty (Tasty rc)", "Custom"]} defaultValue="Tasty (Tasty rc)" style={{ width: "var(--tasty-field-width-lg)" }} /></Row>}
          <Row label="Startup command:"><Input block mono placeholder="run on new session — optional" /></Row>
          <Row label="Scrollback lines:"><Input mono defaultValue="10000" style={{ width: "var(--tasty-field-width-xs)" }} /></Row>
          <Row label="Confirm close if running:"><Switch defaultChecked /></Row>
          <Row label="Inherit working directory:"><Switch defaultChecked /></Row>
          <Row label="Link click modifier:"><Select options={["Ctrl", "Alt", "Shift", "None"]} defaultValue="Ctrl" style={{ width: "var(--tasty-field-width-md)" }} /></Row>
          {!IS_WINDOWS && <Row label="Option as Meta (macOS):"><Switch /></Row>}
        </>
      );
    }

    // ── Keybindings ──
    if (l1 === "Keybindings")
      return (
        <>
          {l2 === "General" && <><KeyRow action="New workspace" keys="Ctrl+Shift+N" /><KeyRow action="Toggle settings" keys="Ctrl+," /><KeyRow action="Command palette" keys="Ctrl+K" /></>}
          {l2 === "Workspace" && <><KeyRow action="Close workspace" keys="Ctrl+Shift+W" /><KeyRow action="Rename workspace" keys="F2" /><KeyRow action="Switch workspace" keys="Ctrl+1" /></>}
          {l2 === "Pane" && <><KeyRow action="Split vertical" keys="Ctrl+D" /><KeyRow action="Split horizontal" keys="Ctrl+Shift+D" /><KeyRow action="Focus next pane" keys="Ctrl+]" /></>}
          {l2 === "Tab" && <><KeyRow action="New tab" keys="Ctrl+T" /><KeyRow action="Next tab" keys="Ctrl+Tab" /><KeyRow action="Close tab" keys="Ctrl+W" /></>}
          {l2 === "Surface" && <><KeyRow action="Convert surface" keys="Ctrl+Shift+C" /><KeyRow action="Focus next surface" keys="Alt+]" /></>}
          {l2 === "Clipboard" && <><KeyRow action="Copy" keys="Ctrl+Shift+C" /><KeyRow action="Paste" keys="Ctrl+Shift+V" /><KeyRow action="Clipboard history" keys="Ctrl+Alt+V" /></>}
          {l2 === "Zoom" && <><KeyRow action="Terminal font: zoom in" keys="Ctrl++" /><KeyRow action="Terminal font: zoom out" keys="Ctrl+-" /><KeyRow action="Terminal font: reset" keys="Ctrl+0" /></>}
          {l2 === "Explorer" && <><KeyRow action="Explorer: open surface" keys="Ctrl+Shift+E" /><KeyRow action="Explorer: reveal current path" keys="Ctrl+Alt+E" /><KeyRow action="Explorer: toggle favorites" keys="None" /></>}
          {l2 === "Scripts" && <><KeyRow action="deploy-staging.lua" keys="Ctrl+Alt+1" /><KeyRow action="rotate-logs.lua" keys="Ctrl+Alt+2" /></>}
          {l2 === "Preset" && <PresetSubtab />}
          {l2 === "Import / Export" && KbImportExportSubtab &&
            <KbImportExportSubtab onFlash={setToast} />}
          {/* 2026-10-08 (batch 8) — real structure: plugin picker + per-command mode / slot / Reset (kb_plugins_subtab.jsx). */}
          {l2 === "Plugins" && KbPluginsSubtab && <KbPluginsSubtab />}
        </>
      );

    // ── Handler (generalized from "File Handler": file-routing subtabs, prefixed
    //    "File …", + the new Hook Handlers registry) ──
    if (l1 === "FileHandler") {
      if (l2 === "File Extension Mapping")
        return (
          <>
            <Mono>Extension → detectors (first match wins)</Mono>
            <ExtensionMapping />
          </>
        );
      // 2026-10-20 (file-handler-forms) — registered list + user add card + Save-pending add/remove: file_handlers.jsx
      if (l2 === "File Detectors") {

        return <FileDetectorsPane />;
      }
      if (l2 === "Hook Handlers")
        return <HookHandlers />;
      // File Handlers

      return <FileHandlersPane />;
    }

    // ── Misc (Scripts everywhere; Tastyrc is Windows-only) ──
    if (l1 === "Misc") {
      if (l2 === "Scripts")
        return <ScriptManager />;
      if (l2 === "Task pipeline")
        return (
          <>
            <Mono>Report limits</Mono>
            <Row label="Note size limit" caption={<RowCaption>One note longer than this is cut at a UTF-8 boundary and marked as truncated. Must be smaller than the attempt limit.</RowCaption>}><Input mono defaultValue="1024" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-number-field-unit-fg)" }}>B</span></Row>
            <Row label="Attempt report limit" caption={<RowCaption>Total note text kept for one task attempt. Notes past it are not stored, only counted.</RowCaption>}><Input mono defaultValue="16384" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-number-field-unit-fg)" }}>B</span></Row>
          </>
        );
      if (l2 === "Anomaly detection" && AnomalySubtab)
        return <AnomalySubtab />;
      if (l2 === "Tastyrc")
        return (
          <>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>Tastyrc</span>
              <Tag variant="warning">Windows only</Tag>
            </div>
            <Note>The Windows shell integration reads a generated <span style={{ fontFamily: "var(--tasty-font-mono)" }}>.tastyrc</span> to inject Tasty's prompt and key bindings into PowerShell / cmd. macOS and Linux use the rc files directly, so this tab is empty there.</Note>
            <Row label="Profile path:"><Input mono defaultValue="%USERPROFILE%\.tastyrc" style={{ width: 240 }} /></Row>
            <Row label="Auto-regenerate:"><Switch defaultChecked /></Row>
            <div style={{ display: "flex", gap: 8 }}>
              <Button variant="secondary">Open file</Button>
              <Button variant="ghost">Regenerate now</Button>
            </div>
          </>
        );
      // non-Windows: tab is visible everywhere but has no sections → empty state
      return (
        <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          gap: "var(--tasty-space-sm)", textAlign: "center", color: "var(--tasty-text-muted)" }}>
          <Icon name="sun" size={26} />
          <div style={{ fontSize: 14, color: "var(--tasty-text-secondary)" }}>No settings on this platform</div>
          <Note>The only Misc setting (<span style={{ fontFamily: "var(--tasty-font-mono)" }}>.tastyrc</span>) applies to the Windows shell integration. The tab stays visible on every platform for consistency.</Note>
        </div>
      );
    }

    // ── Plugins (each L2 is an installed plugin) ──
    if (l1 === "Plugins")
      return (
        <>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%", background: "var(--tasty-accent-agent)" }} />
            <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{l2}</span>
            <Tag variant="agent">plugin</Tag>
            <Tag variant="success" dot>running</Tag>
          </div>
          <Row label="Enabled:"><Switch defaultChecked /></Row>
          <Mono>Permissions</Mono>
          <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>
            <Tag>clipboard</Tag><Tag>fs:read</Tag><Tag>{"ipc:" + l2 + ".*"}</Tag>
          </div>
          <Mono>Keybindings</Mono>
          <KeyRow action={l2 + ": run"} keys="Ctrl+Alt+G" />
          <Row label="Binding mode:"><Select options={["Inherit (manifest)", "Custom", "Disabled"]} style={{ width: "var(--tasty-field-width-lg)" }} /></Row>
        </>
      );

    // General group (consolidated misc categories) ──
    if (l2 === "Notifications")
      return (
        <>
          <Row label="Enabled:"><Switch defaultChecked /></Row>
          <Row label="Sound:"><Switch /></Row>
          <Row label="Coalesce (ms):"><Input mono defaultValue="400" style={{ width: "var(--tasty-field-width-xs)" }} /></Row>
        </>
      );
    if (l2 === "Accessibility")
      return (
        <>
          <Row label="Reduced motion:"><Switch /></Row>
          <Note>Skip UI fade/slide animations. Terminal output animations are unaffected — they are always 0ms.</Note>
          <Row label="Show modifier key hints:"><Switch defaultChecked /></Row>
          <Note>Show a floating panel of the available shortcuts while you hold a modifier key (Ctrl / Alt / Shift — macOS adds Cmd / Option). It appears after a brief hold and dismisses the moment you release.</Note>
          <Row label="High contrast:"><Switch disabled /></Row>
          <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>High contrast (coming soon)</span>
        </>
      );
    if (l2 === "Permissions") {

      return <MacPermissionsPane scenario="none" />;
    }
    // General/General
    return (
      <>
        <Row label="Restore layout:"><Switch defaultChecked /></Row>
        <Row label="Close behavior:"><Select options={["Ask", "Minimize to background", "Quit"]} style={{ width: "var(--tasty-field-width-lg)" }} /></Row>
        {/* 2026-10-08 (batch 8) — the app's two description lines sit under THEIR rows (Row caption), not under the webhook callout. */}
        <Row label="Wheel scroll distance:" caption={<RowCaption>Lines scrolled per wheel notch in the terminal. Trackpads scroll by distance and ignore this.</RowCaption>}>
          <Input mono defaultValue="3" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>lines</span>
        </Row>
        <Row label="Language:" caption={<RowCaption>Changing the language takes effect after Tasty restarts.</RowCaption>}><Select options={["English", "한국어", "日本語"]} style={{ width: "var(--tasty-field-width-md)" }} /></Row>
        {/* 2026-10-21 — explorer Properties time format. Applies on Save. */}
        <Row label="Time format in Properties:" caption={<RowCaption>Used for Modified, Created and Target modified in Explorer Properties, in this computer's time zone. Example: <span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>2026-10-21 12:12</span></RowCaption>}>
          <Select options={[{ value: "minutes", label: "Minutes" }, { value: "seconds", label: "Seconds" }, { value: "seconds_offset", label: "Seconds and UTC offset" }]} defaultValue="minutes" style={{ width: "var(--tasty-field-width-lg)" }} />
        </Row>
        {/* 2026-10-07 — [webhook] allow_external. On the row grid; the warning callout is the row's own and is ALWAYS shown. Save/Cancel flow. */}
        <Row label="Accept webhook calls from other computers:" caption={<WarnCallout>When on, the webhook listener takes every network interface, so anyone who can reach its port can call your registered webhooks. When off, only programs on this computer can. Applies from the next start.</WarnCallout>}><Switch /></Row>
      </>
    );
  }

  return (
    <Scrim onClose={onClose}>
      <div style={{ width: "var(--tasty-settings-window-width)", height: "var(--tasty-settings-window-height)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
        border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden",
        boxShadow: "var(--tasty-shadow-modal)" }}>
        {/* L1 — small fixed set of top tabs */}
        <div style={{ display: "flex", alignItems: "center", height: 44, flex: "none", padding: "0 var(--tasty-space-md)", gap: 2,
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--tasty-text-primary)", letterSpacing: "var(--tasty-letter-spacing-ui)" }}>Settings</span>
          <span style={{ width: 1, height: 20, background: "var(--tasty-separator)", margin: "0 var(--tasty-size-14) 0 var(--tasty-space-sm)", flex: "none" }} />
          {L1_TABS.map((t) => (
            <button key={t} onClick={() => pickL1(t)} style={{ border: 0, background: "transparent", cursor: "pointer",
              height: 43, padding: "0 var(--tasty-space-md)", fontFamily: "var(--tasty-font-ui)", fontSize: 13,
              color: t === l1 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: t === l1 ? 600 : 400,
              borderBottom: t === l1 ? "var(--tasty-focus-ring-width) solid var(--tasty-accent-primary)" : "var(--tasty-focus-ring-width) solid transparent" }}>{L1_LABEL[t]}</button>
          ))}
        </div>

        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          {/* L2 — growable left sidebar with filter */}
          <div style={{ width: "var(--tasty-settings-sidebar-width)", flex: "none", background: "var(--tasty-bg-sidebar)",
            borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", display: "flex", flexDirection: "column" }}>
            <div style={{ padding: 8, borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <Input block icon={ic.search}
                placeholder={isPlugins ? "Filter plugins…" : "Filter sections…"}
                value={filter} onChange={(e) => setFilter(e.target.value)} />
            </div>
            <div className="tasty-scroll" style={{ flex: 1, overflow: "auto", padding: "var(--tasty-space-sm)" }}>
              {shown.map((s) => (
                <React.Fragment key={s}>
                  {KB_L2_SEPARATED.includes(s) && l1 === "Keybindings" && !filter && (
                    <div style={{ height: "var(--tasty-border-width)", background: "var(--tasty-separator)",
                      margin: "var(--tasty-space-sm) var(--tasty-space-sm)" }} />
                  )}
                  <div onClick={() => setL2(s)} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)",
                  padding: "var(--tasty-space-xs) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)", fontSize: 13, cursor: "pointer",
                  color: s === l2 ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)",
                  background: s === l2 ? "var(--tasty-surface-active)" : "transparent" }}>
                  {isPluginSection(s) && <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%",
                    background: "var(--tasty-accent-agent)", flex: "none" }} />}
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s}</span>
                </div>
                </React.Fragment>
              ))}
              {shown.length === 0 && <div style={{ padding: "var(--tasty-space-md)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{filter ? "No matches" : "No sections"}</div>}
            </div>
          </div>

          {/* content */}
          <div style={{ position: "relative", flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
            {fullBleed ? (
              <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
                {body()}
              </div>
            ) : (
              <div className="tasty-scroll" style={{ flex: 1, padding: "var(--tasty-space-lg)", overflow: "auto" }}>
                {/* The 620 cap lives HERE — on the settings content column — so every
                    non-full-bleed subtab inherits it and no block carries its own width.
                    Full-bleed subtabs replace this column with their own layout. */}
                <div style={{ maxWidth: "var(--tasty-settings-content-max-width)", display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-gap)" }}>
                  {body()}
                </div>
              </div>
            )}
            {/* The window runs its own ToastManager (settings.rs) — export
                confirmation lands here, not in the main window. */}
            {toast && (
              <div style={{ position: "absolute", left: "50%", bottom: 64, transform: "translateX(-50%)", zIndex: 4,
                display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", maxWidth: 520,
                padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderRadius: "var(--tasty-radius)",
                background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
                boxShadow: "var(--tasty-shadow-modal)", fontSize: 12, color: "var(--tasty-text-secondary)" }}>
                <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-success-raised)" }}><Icon name="check" size={14} /></span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{toast}</span>
              </div>
            )}
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "var(--tasty-space-md) var(--tasty-size-14)",
              borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <Button variant="ghost" onClick={onClose}>Cancel</Button>
              <Button variant="primary" onClick={onClose}>Save</Button>
            </div>
          </div>
        </div>
      </div>
    </Scrim>
  );
}

export { SettingsWindow };
