import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Checkbox } from "../../../components/forms/Checkbox.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Select } from "../../../components/forms/Select.jsx";
import { DrillDown } from "../../../components/navigation/DrillDown.jsx";
import { Icon } from "../chrome.jsx";

// Tasty UI kit — Settings › Keybindings › Import / Export.
// Borrows the Preset
// drill-down skeleton (list-position entry → detail with a back bar whose RIGHT
// slot holds Apply, over a diff grid) and adds three things Preset does not have:
//   • GROUP HEADERS in the diff grid (4 groups: general combos / quick-switch /
//     script bindings / plugin overrides) — a NEW axis on that table.
//   • A per-row SELECT column (partial apply — user decision §6-2).
//   (The OPTION MIGRATION card was removed 2026-10-10 b12: Win / Super now record as `option`, so an
//    option binding matches on every OS and nothing needs migrating.)
// Apply writes the draft; the window footer's Save commits it (same 2-stage
// contract as Preset). Rendered full-bleed — it owns its own scroll.



const IE_FILE = "tasty-keybindings-2026-09-09.toml";

const IE_GROUPS = [
  { id: "general", label: "General bindings", rows: [
    { action: "Copy", cur: "Ctrl+Shift+C", next: "Ctrl+Shift+C" },
    { action: "Paste", cur: "Ctrl+Shift+V", next: "Ctrl+Shift+V" },
    { action: "Command palette", cur: "Ctrl+K", next: "Ctrl+Shift+P" },
    { action: "Split vertical", cur: "Ctrl+D", next: "Ctrl+Alt+D" },
    { action: "Screenshot to clipboard", cur: "None", next: "Ctrl+Shift+4" },
    { action: "Find", cur: "Ctrl+F", next: "Ctrl+F" },
  ] },
  // §6-5: ONE ROW PER AXIS, not per slot. Slots store raw keys; the combo is
  // composed at display time, so an axis row's value is the composed range.
  { id: "quickswitch", label: "Quick switch (axis summary)", rows: [
    { action: "Tab axis", cur: "Alt+1…0", next: "Ctrl+1…0", note: "10 slots follow this axis" },
    { action: "Workspace axis", cur: "Alt+Shift+1…9", next: "Alt+Shift+1…9", note: "9 slots" },
    { action: "Category axis", cur: "Option+1…0", next: "Ctrl+Option+1…0", note: "10 slots" },
  ] },
  { id: "scripts", label: "Script bindings", rows: [
    { action: "deploy-staging.lua", cur: "Ctrl+Alt+1", next: "Ctrl+Alt+1" },
    { action: "rotate-logs.lua", cur: "None", next: "Ctrl+Alt+2" },
  ] },
  { id: "plugins", label: "Plugin overrides", rows: [
    { action: "open panel", plugin: "git-helper", cur: "Ctrl+Alt+G", next: "Ctrl+Alt+G" },
    { action: "review staged", plugin: "ai-review", cur: "Ctrl+Alt+R", next: "Ctrl+Shift+R" },
  ] },
];

// Quick-switch modifier combos (non-macOS now includes option = Win / Super: 15 combos).
const MODIFIER_COMBOS = ["Ctrl", "Alt", "Shift", "Ctrl+Alt", "Ctrl+Shift", "Alt+Shift", "Ctrl+Alt+Shift"];

const IE_DISCARDED = ["k8s-lens", "s3-browser"];

const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12 };
const microCaps = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
  letterSpacing: "var(--tasty-letter-spacing-caps)" };

// ── Diff grid — 4 columns (select · action · current · imported) ─────────
function IeDiffTable({ groups, changedOnly, collapsed, onToggleGroup, sel, onSel }) {
  const head = { ...microCaps, color: "var(--tasty-text-muted)",
    padding: "0 var(--tasty-space-md) var(--tasty-space-sm)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" };
  const cell = { padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
    fontSize: 13, display: "flex", alignItems: "center" };
  return (
    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-kb-ie-select-column-width) minmax(0,1.6fr) 1fr 1fr", alignItems: "stretch" }}>
      <div style={{ ...head }} />
      <div style={{ ...head, textAlign: "left" }}>Action</div>
      <div style={{ ...head }}>Current</div>
      <div style={{ ...head }}>Imported</div>
      {groups.map((g) => {
        const rows = changedOnly ? g.rows.filter((r) => r.cur !== r.next) : g.rows;
        const isOpen = !collapsed[g.id];
        const on = rows.length > 0 && rows.every((r) => sel[g.id + r.action] !== false);
        return (
          <React.Fragment key={g.id}>
            {/* GROUP HEADER — a NEW axis on this table. One row spanning all four
                columns: select-all box, group name, counts, collapse chevron.
                It does NOT repeat the column headers. */}
            <div style={{ gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)",
              padding: "var(--tasty-space-sm) var(--tasty-space-md)", background: "var(--tasty-surface-raised)",
              borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <Checkbox checked={on} onChange={() => onSel(g, !on)} aria-label={"Select all in " + g.label} />
              <button type="button" onClick={() => onToggleGroup(g.id)} style={{ border: 0, background: "transparent", cursor: "pointer",
                display: "flex", alignItems: "center", gap: 6, padding: 0 }}>
                <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>
                  <Icon name={isOpen ? "chevronDown" : "chevronRight"} size={14} />
                </span>
                <span style={{ ...microCaps, color: "var(--tasty-text-secondary)" }}>{g.label}</span>
              </button>
              <span style={{ ...mono, fontSize: 11, color: "var(--tasty-text-muted)" }}>
                {g.rows.filter((r) => r.cur !== r.next).length} changed · {g.rows.length} total
              </span>
            </div>
            {isOpen && rows.map((r) => {
              const changed = r.cur !== r.next;
              const key = g.id + r.action;
              return (
                <React.Fragment key={key}>
                  <div style={{ ...cell, justifyContent: "center" }}>
                    <Checkbox checked={sel[key] !== false} onChange={() => onSel(g, sel[key] === false, r)}
                      aria-label={"Apply " + r.action} />
                  </div>
                  <div style={{ ...cell, color: "var(--tasty-text-secondary)", flexDirection: "column", alignItems: "flex-start", gap: 2 }}>
                    <span>{r.action}</span>
                    {r.plugin && (
                      <span style={{ ...mono, fontSize: 10, color: "var(--tasty-text-muted)", display: "flex", alignItems: "center", gap: 5 }}>
                        <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%",
                          background: "var(--tasty-accent-agent)", flex: "none" }} />{r.plugin}
                      </span>
                    )}
                    {r.note && <span style={{ fontSize: 10, color: "var(--tasty-text-muted)" }}>{r.note}</span>}
                  </div>
                  <div style={{ ...cell, ...mono, color: "var(--tasty-text-muted)" }}>{r.cur}</div>
                  <div style={{ ...cell, ...mono,
                    color: r.blocked ? "var(--tasty-accent-warning-raised)" : changed ? "var(--tasty-accent-primary)" : "var(--tasty-text-muted)" }}>{r.next}</div>
                </React.Fragment>
              );
            })}
          </React.Fragment>
        );
      })}
    </div>
  );
}

// ── Entry (list position) — two actions, not a list ──────────────────────
// `notice` — §1: an export failure is told INLINE in the row that started it
// (success is a toast; a failure carries a retry, so it must not auto-dismiss).
function IeActionRow({ glyph, title, desc, action, notice }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)",
      padding: "var(--tasty-kb-ie-notice-inset)", borderRadius: "var(--tasty-radius)",
      background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-md)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 2, color: "var(--tasty-text-muted)" }}><Icon name={glyph} size={16} /></span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>{title}</div>
        <p style={{ margin: "2px 0 0", fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)",
          maxWidth: "var(--tasty-measure-md)" }}>{desc}</p>
      </div>
      <span style={{ flex: "none" }}>{action}</span>
      </div>
      {notice}
    </div>
  );
}

// A file picker opens as a POPUP on the settings window's own PopupManager
// (it already runs one for the shortcut-conflict confirm) — not a DrillDown
// step, because the drill-down is already spoken for by the preview, and the
// same picker serves both Export (save target) and Import (open).
function IePickerPopup({ mode, onClose, onPick }) {
  return (
    <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center",
      background: "var(--tasty-scrim-bg)", zIndex: 3 }}>
      <div style={{ width: 420, background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)",
        border: "var(--tasty-border-width) solid var(--tasty-border-strong)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
        <div style={{ display: "flex", alignItems: "center", height: "var(--tasty-titlebar-height)", padding: "0 var(--tasty-space-sm) 0 var(--tasty-space-md)",
          background: "var(--tasty-bg-app)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <span style={{ fontSize: 12, color: "var(--tasty-titlebar-fg)" }}>{mode === "export" ? "Export to…" : "Open keybinding file"}</span>
          <span style={{ marginLeft: "auto" }}><IconButton size="sm" aria-label="Close" onClick={onClose}><Icon name="close" size={16} /></IconButton></span>
        </div>
        <div style={{ padding: "var(--tasty-space-md)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
          <p style={{ margin: 0, fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>
            The in-app file picker view mounts here — its own design is unchanged; only this placement is new.
          </p>
          <Input block value={"~/tasty/" + IE_FILE} readOnly />
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <Button variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
            <Button variant="primary" size="sm" onClick={onPick}>{mode === "export" ? "Save" : "Open"}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// `variant` drives the specimen states: "ready" (entry), "detail", "failed", "exportfailed".
function KbImportExportSubtab({ variant = "ready", onFlash, failLine = 1, failCause = "readonly", osError = "os error 28: No space left on device" }) {
  const [view, setView] = React.useState(variant === "ready" || variant === "exportfailed" ? "list" : "detail");
  const [picker, setPicker] = React.useState(null);
  const [changedOnly, setChangedOnly] = React.useState(true);
  const [collapsed, setCollapsed] = React.useState({});
  const [sel, setSel] = React.useState({});
  const [failed, setFailed] = React.useState(variant === "failed");
  const [exportError, setExportError] = React.useState(variant === "exportfailed" ? "~/tasty/" + IE_FILE : null);
  const onSel = (g, next, row) => setSel((s) => {
    const c = { ...s };
    (row ? [row] : g.rows).forEach((r) => { c[g.id + r.action] = next; });
    return c;
  });

  const total = IE_GROUPS.reduce((n, g) => n + g.rows.length, 0);
  const changed = IE_GROUPS.reduce((n, g) => n + g.rows.filter((r) => r.cur !== r.next).length, 0);
  const picked = IE_GROUPS.reduce((n, g) => n + g.rows.filter((r) => sel[g.id + r.action] !== false).length, 0);

  const detail = failed ? (
    // §6-6 — a broken file is a fact about the thing you were looking at, so it
    // is told INLINE in the detail area, not as a toast or a popup.
    <div style={{ padding: "var(--tasty-space-lg)" }}>
      <div style={{ borderRadius: "var(--tasty-radius)", padding: "var(--tasty-kb-ie-notice-inset)",
        background: "color-mix(in srgb, var(--tasty-accent-danger) 12%, transparent)",
        border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-danger) 35%, transparent)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", color: "var(--tasty-accent-danger-raised)", fontSize: 13, fontWeight: 600 }}>
          <Icon name="alertCircle" size={16} /><span>This file can't be read as keybindings</span>
        </div>
        <p style={{ margin: "4px 0 0", fontSize: 12, color: "var(--tasty-text-secondary)", maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>
          <span style={mono}>~/Downloads/settings.json</span> — expected a keybinding export (TOML, a <span style={mono}>[keybindings]</span> table);
          {/* §3 — no line number: the clause is REPLACED, not dropped. The middle
              sentence always says why; the outer sentences never change. */}
          {failLine ? <> parsing stopped at line {failLine}.</> : <> the file isn't TOML.</>} Nothing was changed.
        </p>
        <div style={{ marginTop: "var(--tasty-space-sm)" }}>
          <Button variant="secondary" size="sm" onClick={() => { setFailed(false); setPicker("import"); }}>Choose another file</Button>
        </div>
      </div>
    </div>
  ) : (
    <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "var(--tasty-space-lg)",
      display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
      <p style={{ margin: 0, fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-lg)" }}>
        <span style={mono}>{IE_FILE}</span> — <b style={{ color: "var(--tasty-text-secondary)" }}>{changed}</b> of {total} bindings change,
        {" "}<b style={{ color: "var(--tasty-text-secondary)" }}>{picked}</b> selected. <b>Apply</b> writes the selected rows into the draft;
        nothing is saved until you press <b style={{ color: "var(--tasty-text-secondary)" }}>Save</b>.
      </p>
      {/* Discarded plugin overrides — INFORMATION, not a warning: nothing is
          wrong and there is nothing to do. Above the table because it explains
          what the table does NOT contain. */}
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", fontSize: 12, color: "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><Icon name="helpCircle" size={14} /></span>
        <span>{IE_DISCARDED.length} plugin overrides were dropped — those plugins aren't installed here
          (<span style={mono}>{IE_DISCARDED.join(", ")}</span>).</span>
      </div>
      <IeDiffTable groups={IE_GROUPS} changedOnly={changedOnly} collapsed={collapsed}
        onToggleGroup={(id) => setCollapsed((c) => ({ ...c, [id]: !c[id] }))} sel={sel} onSel={onSel} />
    </div>
  );

  return (
    <div style={{ position: "relative", flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
      <DrillDown
        view={view}
        title="Import keybindings"
        onBack={() => setView("list")}
        actions={!failed && (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
            <Button variant="ghost" size="sm" onClick={() => setChangedOnly((v) => !v)}>
              {changedOnly ? "Show all " + total : "Changed only"}
            </Button>

            <Button variant="primary" size="sm" disabled={picked === 0}
              onClick={() => onFlash && onFlash("Applied to draft — press Save to commit")}>Apply</Button>
          </div>
        )}
        detail={detail}
      >
        <div style={{ padding: "var(--tasty-space-md) var(--tasty-space-lg)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
          <p style={{ margin: 0, fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-md)" }}>
            Move your whole keybinding configuration between machines. Importing never applies straight away —
            you see what changes first.
          </p>
          <IeActionRow glyph="download" title="Export"
            desc="Writes every binding — general, quick switch, script bindings and plugin overrides — to one file."
            action={<Button variant="secondary" size="sm" disabled={!!exportError} onClick={() => setPicker("export")}>Export…</Button>}
            notice={exportError && (
              <div style={{ borderRadius: "var(--tasty-radius)", padding: "var(--tasty-kb-ie-notice-inset)",
                background: "color-mix(in srgb, var(--tasty-accent-danger) 12%, transparent)",
                border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-danger) 35%, transparent)" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", color: "var(--tasty-accent-danger-raised)", fontSize: 13, fontWeight: 600 }}>
                  <Icon name="alertCircle" size={16} /><span>The export wasn't written</span>
                </div>
                <p style={{ margin: "4px 0 0", fontSize: 12, color: "var(--tasty-text-secondary)", maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>
                  {/* The middle clause comes from a FIXED set; an unknown cause falls
                      back to "the write didn't finish." and the OS string goes on its
                      own muted line below — never spliced into the sentence. */}
                  <span style={mono}>{exportError}</span> — {failCause === "readonly" ? "the folder is read-only."
                    : failCause === "denied" ? "you don't have permission to write there."
                      : failCause === "space" ? "the disk is full."
                        : "the write didn't finish."} Nothing was written.
                </p>
                {failCause === "other" && osError && (
                  <div style={{ ...mono, marginTop: "var(--tasty-space-xs)", fontSize: 11, color: "var(--tasty-text-muted)",
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: "var(--tasty-measure-md)" }} title={osError}>{osError}</div>
                )}
                <div style={{ marginTop: "var(--tasty-space-sm)", display: "flex", gap: "var(--tasty-space-sm)" }}>
                  <Button variant="secondary" size="sm" onClick={() => setExportError(null)}>Try again</Button>
                  <Button variant="ghost" size="sm" onClick={() => { setExportError(null); setPicker("export"); }}>Choose another location…</Button>
                </div>
              </div>
            )} />
          <IeActionRow glyph="file" title="Import"
            desc="Reads a keybinding file and shows the changes against your current bindings before anything is written."
            action={<Button variant="primary" size="sm" onClick={() => setPicker("import")}>Import…</Button>} />
        </div>
      </DrillDown>
      {picker && (
        <IePickerPopup mode={picker} onClose={() => setPicker(null)} onPick={() => {
          const m = picker; setPicker(null);
          // §6-7 — export feedback is the window's own TOAST carrying the
          // resolved path; there is no result screen to return to.
          if (m === "export") onFlash && onFlash("Exported to ~/tasty/" + IE_FILE);
          else { setFailed(false); setView("detail"); }
        }} />
      )}
    </div>
  );
}

export { KbImportExportSubtab, IeDiffTable, IeActionRow, IePickerPopup, IE_GROUPS, IE_FILE, IE_DISCARDED, MODIFIER_COMBOS };
