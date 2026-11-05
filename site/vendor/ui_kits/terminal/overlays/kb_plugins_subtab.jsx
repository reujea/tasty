import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon } from "../../../components/core/Icon.jsx";
import { Select } from "../../../components/forms/Select.jsx";

// Settings › Keybindings › Plugins subtab (2026-10-08, batch 8).
// Box boundaries for the implementer (all component tokens, --tasty-kb-plugin-*):
//   picker row:  [title column 150 "Plugin:"] gap 16 [Select 200]
//   list gap 12
//   command:     padding-y 4 · 1px separator between commands (none after the last)
//                [title column 150, wraps] gap 16 [ control line (min-h 32, every control 28):
//                  mode Select 160 · gap 8 · slot (inherit-source Select 200 | record slots ≥ 176 | "(Unassigned)" 200) · gap 8 · Reset ]
//                  (b12: Custom = record slots 28 high, gap 4 between slots; the line wraps as one flow from the mode x)
//                                                [ caption (gap 4): "Inherited (Ctrl+C)" | parse error ]
// The title column is the settings Row label column (settings-label-width), so the picker Select and every
// mode Select start at the same x. Reset is a ghost md button: same 28 height as the line, disabled when
// there is no override. A draft (unsaved) change shows a 6px accent dot after the title. Save/Cancel = footer.



const KBP_SOURCES = [
  { value: "clipboard.copy", label: "clipboard.copy" },
  { value: "clipboard.paste", label: "clipboard.paste" },
  { value: "clipboard.cut", label: "clipboard.cut" },
  { value: "select_all", label: "select_all" },
];
// resolved host binding per inherit source (null = the host action has no key → "None")
const KBP_RESOLVED = { "clipboard.copy": "Ctrl+Shift+C", "clipboard.paste": "Ctrl+Shift+V", "clipboard.cut": null, "select_all": "Ctrl+Shift+A" };
const KBP_MODES = [{ value: "inherit", label: "Inherit" }, { value: "custom", label: "Custom" }, { value: "none", label: "None" }];

const KBP_PLUGINS = [
  { id: "clipboard-viewer", name: "Clipboard Viewer", commands: [
    { id: "open", title: "Open clipboard viewer", manifest: { mode: "custom", keys: "ctrl+shift+h" } },
    { id: "paste-plain", title: "Paste last entry as plain text", manifest: { mode: "inherit", source: "clipboard.paste" } },
    { id: "clear", title: "Clear history", manifest: { mode: "none" } },
  ] },
  { id: "git-helper", name: "Git Helper", commands: [
    { id: "stage", title: "Stage hunk", manifest: { mode: "custom", keys: "ctrl+alt+g" } },
    { id: "copy-sha", title: "Copy commit SHA", manifest: { mode: "inherit", source: "clipboard.copy" } },
  ] },
];

const KBP_MODS = ["ctrl", "shift", "alt", "cmd", "super", "option"];
// one chord = modifiers + one key; a list = chords joined by commas; spaces ignored
function kbpParse(text) {
  const parts = text.replace(/\s+/g, "").split(",").filter(Boolean);
  for (const p of parts) {
    const seg = p.toLowerCase().split("+");
    const key = seg.pop();
    if (!key || seg.some((m) => !KBP_MODS.includes(m)) || KBP_MODS.includes(key)) return p;
    if (!/^([a-z0-9]|f([1-9]|1[0-9]|2[0-4])|enter|tab|space|esc|escape|backspace|delete|home|end|pageup|pagedown|up|down|left|right|[-=,.;'/\[\]\\`])$/.test(key)) return p;
  }
  return null; // null = valid
}

const kbpS = {
  title: { width: "var(--tasty-kb-plugin-title-width)", flex: "none", minHeight: "var(--tasty-kb-plugin-row-min-height)",
    display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: 13, color: "var(--tasty-text-secondary)",
    lineHeight: "var(--tasty-line-height-ui)", overflowWrap: "anywhere" },
  // the line wraps as ONE flow (2026-10-10 b12): a second line starts at the mode Select x; Reset is the last item.
  line: { display: "flex", alignItems: "center", flexWrap: "wrap", columnGap: "var(--tasty-kb-plugin-control-gap)", rowGap: "var(--tasty-space-xs)", minHeight: "var(--tasty-kb-plugin-row-min-height)" },
  rec: { height: "var(--tasty-kb-plugin-record-height)", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box",
    border: "var(--tasty-border-width) solid var(--tasty-kb-record-border)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)",
    fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", padding: "0 var(--tasty-space-sm)" },
  caption: { fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)" },
};

// Custom = record slots (2026-10-10 b12, confirmed): one kb_record_slot per key (min kb-record-width 140), then the
// add (+) slot (kb-record-add-width 32); no key → one None slot. Every slot is kb-plugin-record-height 28 (the line's
// height). Slots are one group: space-xs between them; the group's ends keep the line's control gap 8.
const kbpTitle = (s) => s.split("+").map((p) => p.charAt(0).toUpperCase() + p.slice(1)).join("+");
function KbpRecordSlots({ keys = "", recording = false, err }) {
  const list = keys.split(",").map((s) => s.trim()).filter(Boolean);
  const R = kbpS.rec;
  return (
    <span style={{ display: "inline-flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
      {list.length === 0 && !recording && <span style={{ ...R, minWidth: "var(--tasty-kb-record-width)", color: "var(--tasty-kb-record-empty-fg)", fontFamily: "var(--tasty-font-ui)" }}>None</span>}
      {list.map((key) => <span key={key} title="Click, then press the shortcut" style={{ ...R, minWidth: "var(--tasty-kb-record-width)", ...(err === key ? { borderColor: "var(--tasty-kb-plugin-error-fg)" } : null) }}>{err === key ? key : kbpTitle(key)}</span>)}
      {recording
        ? <span style={{ ...R, borderColor: "var(--tasty-border-focus)", color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-ui)" }}>Press key combination...</span>
        : list.length > 0 && <span title="Add shortcut" style={{ ...R, width: "var(--tasty-kb-record-add-width)", padding: 0, background: "transparent", color: "var(--tasty-text-muted)" }}><Icon name="plus" size="var(--tasty-icon-size-sm)" /></span>}
    </span>
  );
}

function KbpCommandRow({ cmd, value, draft, last, onChange, onReset }) {
  const v = draft ?? cmd.manifest; // effective (draft override or manifest default)
  const overridden = draft != null;
  const dirty = JSON.stringify(draft) !== JSON.stringify(value);
  const err = v.mode === "custom" && v.keys ? kbpParse(v.keys) : null; // a hand-edited config value that doesn't parse
  const setMode = (mode) => {
    if (mode === "custom") onChange({ mode, keys: (draft && draft.keys) || (cmd.manifest.mode === "custom" ? cmd.manifest.keys : "") });
    else if (mode === "inherit") onChange({ mode, source: cmd.manifest.source || KBP_SOURCES[0].value });
    else onChange({ mode });
  };
  const resolved = v.mode === "inherit" ? KBP_RESOLVED[v.source] : null;
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-kb-plugin-title-gap)", padding: "var(--tasty-kb-plugin-row-padding-y) 0",
      borderBottom: last ? "none" : "var(--tasty-border-width) solid var(--tasty-kb-plugin-separator)" }}>
      <div style={kbpS.title}>
        <span>{cmd.title}</span>
        {dirty && <span title="Changed — not saved yet" style={{ flex: "none", width: "var(--tasty-kb-plugin-draft-dot-size)", height: "var(--tasty-kb-plugin-draft-dot-size)",
          borderRadius: "50%", background: "var(--tasty-kb-plugin-draft-dot)" }}></span>}
      </div>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-kb-plugin-caption-gap)" }}>
        <div style={kbpS.line}>
          <Select options={KBP_MODES} value={v.mode} onChange={(e) => setMode(e.target.value)} style={{ width: "var(--tasty-kb-plugin-mode-width)" }} />
          {v.mode === "inherit" && <Select options={KBP_SOURCES} value={v.source} onChange={(e) => onChange({ mode: "inherit", source: e.target.value })} style={{ width: "var(--tasty-kb-plugin-slot-width)" }} />}
          {v.mode === "custom" && <KbpRecordSlots keys={v.keys} recording={v.recording} err={err} />}
          {v.mode === "none" && <span style={{ width: "var(--tasty-kb-plugin-slot-width)", flex: "none", height: "var(--tasty-kb-plugin-control-height)", display: "flex", alignItems: "center",
            fontSize: 13, color: "var(--tasty-kb-plugin-none-fg)" }}>(Unassigned)</span>}
          <Button variant="ghost" disabled={!overridden} title="Clear the override and use the manifest default." onClick={onReset}>Reset</Button>
        </div>
        {v.mode === "inherit" && <span style={{ ...kbpS.caption, color: "var(--tasty-kb-plugin-caption-fg)" }}>{resolved ? `Inherited (${resolved})` : "None"}</span>}
        {err && <span style={{ ...kbpS.caption, color: "var(--tasty-kb-plugin-error-fg)" }}>Unrecognized key: <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{err}</span></span>}
      </div>
    </div>
  );
}

// saved: { [pluginId/cmdId]: override } (what Save last wrote) · drafts start equal to saved.
function KbPluginsSubtab({ plugins = KBP_PLUGINS, saved = { "clipboard-viewer/clear": { mode: "custom", keys: "ctrl+alt+x" } }, seedDrafts = null, initial = null }) {
  const [pid, setPid] = React.useState(initial || (plugins[0] && plugins[0].id));
  const [drafts, setDrafts] = React.useState(() => ({ ...saved, ...(seedDrafts || {}) }));
  if (!plugins.length)
    return <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>No plugins have registered shortcuts.</p>;
  const sorted = plugins.slice().sort((a, b) => a.name.localeCompare(b.name));
  const plugin = sorted.find((p) => p.id === pid) || sorted[0];
  const set = (k, val) => setDrafts((d) => { const n = { ...d }; if (val == null) delete n[k]; else n[k] = val; return n; });
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-kb-plugin-list-gap)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-kb-plugin-title-gap)", minHeight: "var(--tasty-kb-plugin-row-min-height)" }}>
        <span style={{ ...kbpS.title, minHeight: 0 }}>Plugin:</span>
        <Select options={sorted.map((p) => ({ value: p.id, label: p.name }))} value={plugin.id} onChange={(e) => setPid(e.target.value)} style={{ width: "var(--tasty-kb-plugin-picker-width)" }} />
      </div>
      <div>
        {plugin.commands.map((c, i) => {
          const k = plugin.id + "/" + c.id;
          return <KbpCommandRow key={k} cmd={c} value={saved[k] ?? null} draft={drafts[k] ?? null} last={i === plugin.commands.length - 1}
            onChange={(val) => set(k, val)} onReset={() => set(k, null)} />;
        })}
      </div>
    </div>
  );
}

export { KbPluginsSubtab, KBP_PLUGINS };
