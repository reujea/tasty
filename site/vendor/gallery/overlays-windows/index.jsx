import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Icon as WIcon, Checkbox as WCheckbox, Select as WSelect, Tag as WTag, Table as WTable, Input as WInput, Switch as WSwitch, CodeArea as WCodeArea, Kbd, IconButton, Input, Button } from "../../components/index.js";
import { KbPluginsSubtab as Kit_KbPluginsSubtab, KbPluginsSubtab } from "../../ui_kits/terminal/overlays/kb_plugins_subtab.jsx";
import { MacPermissionsPane } from "../../ui_kits/terminal/overlays/macos_permissions.jsx";
import { Popup as GalleryNotifications_Popup } from "../parts/notifications.jsx";
import { GroundThemes as WGroundThemes, GroundCap as WGroundCap, Backdrop, PaletteFrame, LocalSshSection, PortsFrame, PORTS_COLUMNS, PortsFavoritesG, PortStarG, RemoteFrame, SettingsFrame, SettingsGeneralOverlayFrame, SettingsRemoteTransferFrame, ToastDragValue, GitViewerFrame, ClipboardFrame, RemoteFormFrame, RemoteAttachFrame, RaNewRow, RaWsPeek, FilePickerFrame, ScriptManagerFrame, ic } from "../parts/overlay-frames.jsx";
import { AnomalySection as OverlaysWindowsAnomaly_AnomalySection } from "./settings-anomaly.jsx";
import { KbPluginsRecordSpec as OverlaysWindowsB12_KbPluginsRecordSpec } from "./kb-plugin-records.jsx";
import { SettingsNarrowSection as OverlaysWindowsB12_SettingsNarrowSection } from "./settings-narrow.jsx";
import { OptionKeySection as OverlaysWindowsB12_OptionKeySection } from "./option-key.jsx";
import { FileHandlerSection as OverlaysWindowsFileHandlers_FileHandlerSection } from "./file-handlers.jsx";
import { PerfSection as OverlaysWindowsPerf_PerfSection } from "./settings-performance.jsx";
import { TrustBox2 as OverlaysWindowsPluginTrust_TrustBox2, PluginTrustSpecs as OverlaysWindowsPluginTrust_PluginTrustSpecs } from "./plugin-trust.jsx";
import { Section as PresetEditor_Section } from "./preset-editor.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Windows (large modal surfaces)
// One of the five Overlays sub-pages (dialogs · windows · popups · banners · tutorial). Frame components are shared from
// parts/overlay-frames.jsx (parts/overlay-frames.jsx); this file holds only the
// page's specimens + nav. See the other overlays-*.jsx for the rest.


// Distinct names — the sibling gallery scripts already bind Icon/Checkbox/
// Select/Tag at top level, and a second binding of the same name resolves to
// undefined here (invalid element type).









// 2026-09-29 disabled sites without a screen — static specimens of the kit
// (plugins_window.jsx FingerprintLine / Add bar, settings_window.jsx ExtensionMapping).
function FpLineG({ value }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
      <span style={{ color: "var(--tasty-text-secondary)" }}>fingerprint</span><span>{value}</span>
      <IconButton size="sm" aria-label="Copy fingerprint"><WIcon name="copy" /></IconButton>
    </div>
  );
}
function AddBarG({ blocked, trusted = true, perms = 3 }) {
  // 2026-10-06: "unsigned-no-key" → "missing-pubkey" (the state IS signed; only the .pub is missing / unreadable)
  const why = { installed: "Already installed", "missing-pubkey": "Signed, but the publisher's public key file is missing", "signature-error": "Signature check failed" }[blocked];
  const grants = perms === 0 ? "No permissions" : perms === 1 ? "Grants 1 permission" : "Grants " + perms + " permissions";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{why || grants}</span>
      <div style={{ flex: 1 }} />
      <Button variant="ghost">Cancel</Button>
      {blocked ? <Button variant="primary" disabled>Add plugin</Button> : trusted ? <Button variant="primary">Add plugin</Button> : <Button variant="primary">Trust &amp; add</Button>}
    </div>
  );
}
// 2026-10-06 static specimen of plugins_window.jsx TrustBox (same copy + recipe)
const TRUST_G = {
  "unknown-key":         ["warning", "alertTriangle", "Unverified publisher", "This plugin isn't signed by a key in your trust store. It runs with the permissions above on every launch — review them, and only add plugins from sources you trust. Adding it also trusts this key."],
  "permissions-changed": ["warning", "alertTriangle", "Permissions changed", "This publisher is trusted, but this version asks for permissions the trusted version did not have. Review the list above; adding it trusts the new set."],
  "missing-pubkey":      ["danger", "alertCircle", "Public key file missing", "The manifest is signed by a key that isn't in your trust store, and tasty-plugin.toml.pub is missing or unreadable, so the key can't be added. Ask the publisher for this public key file."],
  "signature-error":     ["danger", "alertCircle", "Signature check failed", "The signature could not be verified. The plugin can't be added until the publisher ships a valid signature."],
};
function TrustBoxG({ kind }) {
  // 2026-10-19: the trust box moved to the unverified-warning recipe — one copy in overlays-windows-plugin-trust.jsx
  if (true) return <OverlaysWindowsPluginTrust_TrustBox2 kind={kind} />;
  const [tone, glyph, title, body] = kind === "trusted" ? ["success"] : TRUST_G[kind];
  const acc = "var(--tasty-accent-" + tone + ")";
  const box = { display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)", borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-secondary)",
    background: "color-mix(in srgb, " + acc + " calc(var(--tasty-tint-fill-alpha) * 100%), transparent)", border: "var(--tasty-border-width) solid color-mix(in srgb, " + acc + " calc(var(--tasty-tint-border-alpha) * 100%), transparent)" };
  if (kind === "trusted") return (
    <div style={{ ...box, flexDirection: "row", alignItems: "center", color: acc, fontSize: "var(--tasty-font-size-term-sm)" }}><WIcon name="shieldCheck" size={16} /><span>Signed by a <b>trusted publisher</b> — its key is in your trust store.</span></div>
  );
  return (
    <div style={box}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", color: acc, fontSize: 13, fontWeight: 600 }}><WIcon name={glyph} size={16} /><span>{title}</span></div>
      <p style={{ margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)" }}>{body}</p>
      {kind !== "signature-error" && <FpLineG value="9f2c 4ad1 b770 e3a6  ·  ed25519" />}
    </div>
  );
}
// 2026-10-06 — Settings › Appearance › <surface> › Font override. Same idiom as the colour-override rows:
// label (settings-label-width) · control (its field-width) · trailing Checkbox "Use default". Preview is a
// separate block BELOW the grid at every window width — no side column to collide with.
function FontOverrideG({ long = false }) {
  const ud = long ? "既定値を使用" : "Use default";
  const rows = [
    ["Font family", <span style={{ display: "flex", width: "var(--tasty-field-width-lg)" }}><Input block defaultValue="D2Coding" icon={<WIcon name="search" />} /></span>, false],
    ["Custom font file", <span style={{ display: "flex", width: "var(--tasty-field-width-lg)" }}><Input block mono readOnly /></span>, true],
    ["Font size", <span style={{ display: "flex", width: "var(--tasty-field-width-xs)" }}><Input block mono defaultValue="14" addon="px" /></span>, false],
    ["Line height", <span style={{ display: "flex", width: "var(--tasty-field-width-xs)" }}><Input block mono readOnly defaultValue="1.2" /></span>, true],
    ["Font DPI scaling", <span style={{ display: "flex", width: "var(--tasty-field-width-md)" }}><Input block readOnly defaultValue="Follow display" /></span>, true],
  ];
  // 2026-10-06 (b2) — preview content: the surface's EFFECTIVE background + effective font, four sample lines
  // (latin · hangul · digits · kana), inside the kit box edge. bg-app / bg-panel are stand-ins for the runtime
  // surface colours (Terminal default: Focused #000000, Unfocused = base).
  const pv = (focused) => (
    <div style={{ flex: "1 1 var(--tasty-font-preview-min-width)", minWidth: "var(--tasty-font-preview-min-width)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{focused ? "Focused" : "Unfocused"}</span>
      <div style={{ display: "flex", flexDirection: "column", padding: "var(--tasty-font-preview-padding-y) var(--tasty-font-preview-padding-x)", background: focused ? "var(--tasty-bg-app)" : "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid " + (focused ? "var(--tasty-border-strong)" : "var(--tasty-separator)"), borderRadius: "var(--tasty-radius)",
        fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term)", lineHeight: "var(--tasty-font-preview-line-height)", color: "var(--tasty-text-primary)", whiteSpace: "nowrap", overflow: "hidden" }}>
        <span>AaBbCcDdEeFfGg</span><span>가나다라마바사</span><span>1234567890</span><span>アカサタナハマラヤワ</span>
      </div>
    </div>
  );
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-lg)", maxWidth: "var(--tasty-settings-content-max-width)" }}>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {rows.map(([label, ctl, def]) => (
          <div key={label} style={{ display: "flex", alignItems: "center", flexWrap: "wrap", columnGap: "var(--tasty-space-lg)", rowGap: "var(--tasty-space-xs)", minHeight: "var(--tasty-settings-row-min-height)", padding: "var(--tasty-space-xs) 0" }}>
            <span style={{ flex: "none", width: "var(--tasty-settings-label-width)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>{label}</span>
            <span style={{ flex: "none", display: "flex" }}>{ctl}</span>
            <WCheckbox label={ud} checked={def} />
          </div>
        ))}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Preview</span>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-md)" }}>{pv(true)}{pv(false)}</div>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Font: D2Coding / 14.0px</span>
      </div>
    </div>
  );
}
function ExtMapG({ draft = "", detectors = true, custom = false, missing = false, long = false, pendingRemove = false, pendingReset = false }) {
  // 2026-10-06 (b2) — pending (draft) state: the button that changed the draft becomes Undo in the same slot,
  // and a disabled Tag says what Save will do. Remove-pending also strikes the .ext label.
  const P = long ? { undo: "Rückgängig", removed: "保存時に削除", reset: "Se restablece al guardar" } : { undo: "Undo", removed: "removed on save", reset: "reset on save" };
  // 2026-10-06 — Reset (custom order) and Remove (not installed) live at the RIGHT END of the group header,
  // ghost Button sm; header min-height = button-height-sm so the row does not jump when Reset appears.
  const L = long ? { reset: "Restablecer orden", remove: "Entfernen", ni: "インストールされていません" } : { reset: "Reset", remove: "Remove", ni: "not installed" };
  const groups = [{ ext: ".md", custom, rows: [["Markdown viewer", true], ["Editor", true], ["html-preview", false]] }, ...(missing ? [{ ext: ".ipynb", missing: true, rows: [] }] : []), { ext: ".log", rows: [["Log viewer", true]] }];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", width: "var(--tasty-size-360)", maxWidth: "100%" }}>
      <div style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>
        <Input block mono placeholder="extension, e.g. .log" defaultValue={draft} />
        <Button variant="secondary" size="sm" disabled={!draft || !detectors}>Add</Button>
      </div>
      {groups.map((g) => {
        const cand = g.rows.filter(([, c]) => c).length;
        return (
          <div key={g.ext} style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-button-height-sm)", padding: "var(--tasty-space-xs) 0", borderBottom: g.missing ? "var(--tasty-border-width) solid var(--tasty-separator)" : undefined }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: g.missing ? "var(--tasty-text-muted)" : "var(--tasty-text-secondary)", textDecoration: g.missing && pendingRemove ? "line-through" : undefined }}>{g.ext}</span>
              {g.missing && <WTag>{pendingRemove ? P.removed : L.ni}</WTag>}
              {g.custom && pendingReset && <WTag>{P.reset}</WTag>}
              <span style={{ flex: 1 }} />
              {g.custom && (pendingReset ? <Button variant="ghost" size="sm">{P.undo}</Button> : <span title="Remove the custom priority for this extension (revert to install order)."><Button variant="ghost" size="sm">{L.reset}</Button></span>)}
              {g.missing && <Button variant="ghost" size="sm">{pendingRemove ? P.undo : L.remove}</Button>}
            </div>
            {g.rows.map(([name, c], i) => (
              <div key={name} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-settings-row-min-height)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                <span style={{ width: "var(--tasty-space-lg)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{i + 1}</span>
                <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-body)", color: c ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)" }}>{name}</span>
                {!c && <WTag>off</WTag>}
                <IconButton size="sm" aria-label="Move up" disabled={!c || i === 0}><WIcon name="chevronUp" /></IconButton>
                <IconButton size="sm" aria-label="Move down" disabled={!c || i >= cand - 1}><WIcon name="chevronDown" /></IconButton>
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

// 2026-10-07 — host / plugin hook row carrying a user patch: "edited" mark + Revert, and the pending-revert state.

function HookOverrideG() {
  const rows = [
    { ev: "on_open", act: "ipc: focus → open_markdown_preview", def: "open_markdown_preview", origin: "host", edited: true, on: true },
    { ev: "on_paste", act: "imgview.stash", origin: "dev.imgview", edited: true, on: false },
    { ev: "on_open", act: "ipc: focus → open_markdown_preview", def: "open_markdown_preview", origin: "host", edited: true, on: true, pending: true },
    { ev: "on_exit", act: "ipc: focus → save → close", origin: "host", on: true },
  ];
  return (
    <div style={{ width: "var(--tasty-size-560)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      {rows.map((r, i) => {
        const plugin = r.origin !== "host";
        const shownOn = r.pending ? true : r.on;
        return (
          <div key={i} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{r.ev}</span>
              <span style={{ color: plugin ? "var(--tasty-accent-agent-raised)" : undefined, display: "inline-flex" }}><WTag>{r.origin}</WTag></span>
              {r.edited && !r.pending && <span title="Changed in your settings. Updates to the default no longer apply." style={{ display: "inline-flex" }}><WTag>edited</WTag></span>}
              {r.pending && <WTag>reverts on save</WTag>}
              <span style={{ flex: 1 }} />
              <span title={r.pending ? "Reverts on save. Undo to change it." : undefined} style={{ display: "inline-flex" }}><WSwitch checked={shownOn} disabled={r.pending} onChange={() => {}} aria-label="Enabled" /></span>
              <span title={"Provided by " + r.origin + " — can't be removed"} style={{ display: "inline-flex", color: "var(--tasty-glyph-dim)" }}><WIcon name="lock" size="var(--tasty-icon-size-sm)" /></span>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
              <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.pending ? r.def : r.act}</span>
              {r.edited && (r.pending ? <Button variant="ghost" size="sm">Undo</Button> : <span title={"Go back to the default from " + r.origin + "."}><Button variant="ghost" size="sm">Revert</Button></span>)}
              {r.act.startsWith("ipc:") && <Button variant="ghost" size="sm" disabled={r.pending}>Edit</Button>}
            </div>
          </div>
        );
      })}
    </div>
  );
}
const ThemePair = ({ children }) => (
  <>{[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
    <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)", minWidth: 0 }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
    </div>
  ))}</>
);
// 2026-10-06 — Hook Handlers IpcSequence inline editor (normal · error · empty)
function HookSeqEditorG({ state = "normal" }) {

  const text = { normal: 'system.info\nnotification.send {"body":"${body.branch}","title":"Build ${body.status}"}\nworkspace.create {"name":"ci-${body.run}"}', error: '# notify\nsystem.info\nnotification.send {"title": "Build", body: 1}', empty: "" }[state];
  const cap = { fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)" };
  return (
    <div style={{ width: "var(--tasty-size-460)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-md) 0" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-secondary)" }}>on_webhook</span>
        <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-primary)" }}>ci-notify</span>
        <WTag>you</WTag>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm) var(--tasty-space-md) var(--tasty-space-md)" }}>
        {WCodeArea && <WCodeArea minRows={4} defaultValue={text} errorLine={state === "error" ? 3 : null} placeholder="system.info" />}
        {state === "error"
          ? <div style={{ ...cap, display: "flex", alignItems: "baseline", gap: "var(--tasty-space-xs)", flexWrap: "wrap" }}>
              <span style={{ display: "inline-flex", alignSelf: "center", color: "var(--tasty-accent-danger-raised)" }}><WIcon name="alertCircle" size={12} /></span>
              <span style={{ color: "var(--tasty-accent-danger-raised)" }}>Line 3, column 38: invalid params JSON.</span>
              <span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-muted)" }}>key must be a string</span>
            </div>
          : state === "empty"
            ? <span style={{ ...cap, color: "var(--tasty-text-muted)" }}>No calls. The handler does nothing.</span>
            : null}
        <span style={{ ...cap, color: "var(--tasty-text-muted)" }}>One call per line: method, then optional JSON params. Lines starting with # are skipped and are not kept.</span>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", paddingTop: "var(--tasty-space-xs)" }}>
          <Button variant="ghost" size="sm">Cancel</Button>
          <Button variant="secondary" size="sm" disabled={state === "error"}>Apply</Button>
        </div>
      </div>
    </div>
  );
}

const NAV = [
  { id: "palette", label: "Command palette" },
  { id: "ports", label: "Listening ports" },
  { id: "remote", label: "Remote connections" },
  { id: "remoteattach", label: "Add remote workspace" },
  { id: "filepicker", label: "File picker" },
  { id: "preseteditor", label: "Preset editor" },
  { id: "settings", label: "Settings window" },
  { id: "settingsnarrow", label: "Settings rows — narrow (b12)" },
  { id: "filehandlers", label: "Handler › File Detectors · Handlers" },
  { id: "perfapply", label: "Terminal › Performance — apply note" },
  { id: "permissions", label: "General › Permissions (macOS)" },
  { id: "kbimportexport", label: "Keybindings · Import / Export" },
  { id: "optionkey", label: "Win · Super as option (b12)" },
  { id: "pluginswindow", label: "Plugins window · avatar · disabled sites" },
  { id: "scripts", label: "Misc · Scripts" },
  { id: "anomaly", label: "Misc · Anomaly detection" },
  { id: "gitviewer", label: "Git viewer" },
  { id: "clipboard", label: "Clipboard viewer" },
  { id: "notifications", label: "Notifications" },
  { id: "moveresize", label: "Move & resize" },
];

// ── Plugin avatar — the canonical identity mark (Plugins window) ──────
// ONE component, TWO sizes. Values are component tokens (--tasty-plugin-avatar-*);
// this specimen is the design SoT the real widget is transcribed from.
function PluginAvatarG({ initial, size = "sm" }) {
  const lg = size === "lg";
  const s = lg ? "lg" : "sm";
  return (
    <span style={{ width: `var(--tasty-plugin-avatar-size-${s})`, height: `var(--tasty-plugin-avatar-size-${s})`,
      flex: "none", borderRadius: "var(--tasty-plugin-avatar-radius)", display: "inline-flex",
      alignItems: "center", justifyContent: "center", background: "var(--tasty-plugin-avatar-bg)",
      border: "var(--tasty-plugin-avatar-border-width) solid var(--tasty-plugin-avatar-border)",
      fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-plugin-avatar-initial-weight)",
      color: "var(--tasty-plugin-avatar-fg)", lineHeight: 1,
      fontSize: `var(--tasty-plugin-avatar-initial-font-size-${s})` }}>{initial}</span>
  );
}

// A Plugins-window list row at each row state — the mark does not change.
function PluginRowG({ name, meta, state = "rest", disabled }) {
  const bg = state === "selected" ? "var(--tasty-surface-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", width: 260,
      padding: "var(--tasty-space-sm)", borderRadius: "var(--tasty-radius)", background: bg,
      boxShadow: state === "selected" ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" /* disabled PLUGIN stays selectable, so its name is read: muted ink, never opacity */ }}>
      <PluginAvatarG initial={name.charAt(0).toUpperCase()} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: "var(--tasty-font-size-body)", color: disabled ? "var(--tasty-text-muted)" : "var(--tasty-text-primary)" }}>{name}</div>
        <div style={{ marginTop: 2, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{meta}</div>
      </div>
    </div>
  );
}

// ── Keybindings › Import / Export — specimen parts ────────────────
const ieMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12 };
const ieCaps = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
  letterSpacing: "var(--tasty-letter-spacing-caps)" };

// L2 tail with the SEPARATED row (new axis on the settings L2 row model).
// Import/Export specimens render at the product width: Keybindings › Import/Export is FULL-BLEED, so at the default
// 1100 window the content column is 868 (settings-window-width − L2 sidebar − paddings). The 620 settings cap does not
// apply here (2026-10-07). Column widths come from the kb-ie tokens (288 · 120).
const IE_W = 868;
function IeL2Tail() {
  const row = (label, active) => (
    <div key={label} style={{ display: "flex", alignItems: "center", padding: "4px 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
      color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)",
      background: active ? "var(--tasty-surface-active)" : "transparent" }}>{label}</div>
  );
  return (
    <div style={{ width: "var(--tasty-settings-sidebar-width)", padding: 8, background: "var(--tasty-bg-sidebar)",
      border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)" }}>
      {["Explorer", "Scripts", "Preset", "Plugins"].map((l) => row(l, false))}
      <div style={{ height: 1, background: "var(--tasty-separator)", margin: "8px" }} />
      {row("Import / Export", true)}
    </div>
  );
}

function IeEntry() {
  const card = (glyph, title, desc, btn) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 12, padding: "12px 14px", borderRadius: "var(--tasty-radius)",
      background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 2, color: "var(--tasty-text-muted)" }}><WIcon name={glyph} size={16} /></span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>{title}</div>
        <p style={{ margin: "2px 0 0", fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>{desc}</p>
      </div>
      <span style={{ flex: "none" }}>{btn}</span>
    </div>
  );
  return (
    <div style={{ width: "100%", maxWidth: IE_W, display: "flex", flexDirection: "column", gap: 12 }}>
      {card("download", "Export", "Writes every binding — general, quick switch, script bindings and plugin overrides — to one file.",
        <Button variant="secondary" size="sm">Export…</Button>)}
      {card("file", "Import", "Reads a keybinding file and shows the changes against your current bindings before anything is written.",
        <Button variant="primary" size="sm">Import…</Button>)}
    </div>
  );
}

// Diff grid — select column + group header axis.
function IeGrid() {
  const head = { ...ieCaps, color: "var(--tasty-text-muted)", padding: "0 12px 8px", borderBottom: "1px solid var(--tasty-separator)" };
  const cell = { padding: "8px 12px", borderBottom: "1px solid var(--tasty-separator)", fontSize: 13, display: "flex", alignItems: "center" };
  const group = (label, counts, open = true) => (
    <div style={{ gridColumn: "1 / -1", display: "flex", alignItems: "center", gap: 8, padding: "8px 12px",
      background: "var(--tasty-surface-raised)", borderBottom: "1px solid var(--tasty-separator)" }}>
      <WCheckbox defaultChecked={open} aria-label={"Select all in " + label} />
      <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><WIcon name={open ? "chevronDown" : "chevronRight"} size={14} /></span>
      <span style={{ ...ieCaps, color: "var(--tasty-text-secondary)" }}>{label}</span>
      <span style={{ ...ieMono, fontSize: 11, color: "var(--tasty-text-muted)" }}>{counts}</span>
    </div>
  );
  const row = (action, cur, next, extra) => (
    <React.Fragment key={action}>
      <div style={{ ...cell, justifyContent: "center" }}><WCheckbox defaultChecked aria-label={"Apply " + action} /></div>
      <div style={{ ...cell, flexDirection: "column", alignItems: "flex-start", gap: 2, color: "var(--tasty-text-secondary)" }}>
        <span>{action}</span>{extra}
      </div>
      <div style={{ ...cell, ...ieMono, color: "var(--tasty-text-muted)" }}>{cur}</div>
      <div style={{ ...cell, ...ieMono, color: cur === next ? "var(--tasty-text-muted)" : "var(--tasty-accent-primary-raised)" }}>{next}</div>
    </React.Fragment>
  );
  return (
    <div style={{ width: "100%", maxWidth: IE_W, display: "grid", gridTemplateColumns: "32px minmax(0,1.6fr) 1fr 1fr", alignItems: "stretch" }}>
      <div style={head} /><div style={{ ...head, textAlign: "left" }}>Action</div><div style={head}>Current</div><div style={head}>Imported</div>
      {group("General bindings", "3 changed · 61 total")}
      {row("Command palette", "Ctrl+K", "Ctrl+Shift+P")}
      {row("Split vertical", "Ctrl+D", "Ctrl+Alt+D")}
      {group("Quick switch (axis summary)", "2 changed · 3 axes")}
      {row("Tab axis", "Alt+1…0", "Ctrl+1…0", <span style={{ fontSize: 10, color: "var(--tasty-text-muted)" }}>10 slots follow this axis</span>)}
      {group("Plugin overrides", "1 changed · 2 total")}
      {row("review staged", "Ctrl+Alt+R", "Ctrl+Shift+R",
        <span style={{ ...ieMono, fontSize: 10, color: "var(--tasty-text-muted)", display: "flex", alignItems: "center", gap: 5 }}>
          <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%",
            background: "var(--tasty-accent-agent)" }} />git-helper</span>)}
      {group("Script bindings", "1 changed · 2 total", false)}
    </div>
  );
}

function IeBackBarG({ unresolved }) {
  return (
    <div style={{ width: "100%", maxWidth: IE_W, display: "flex", alignItems: "center", gap: 8, height: 40, padding: "0 12px",
      background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)" }}>
      <IconButton size="sm" aria-label="Back">{ic.back}</IconButton>
      <span style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>Import keybindings</span>
      <span style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 8 }}>
        <Button variant="ghost" size="sm">{unresolved ? "Show all 73" : "Changed only"}</Button>
        {unresolved > 0 && <span style={{ ...ieMono, fontSize: 11, color: "var(--tasty-accent-warning-raised)" }}>{unresolved} unresolved</span>}
        <Button variant="primary" size="sm" disabled={unresolved > 0}>Apply</Button>
      </span>
    </div>
  );
}

function IeNotices() {
  return (
    <div style={{ width: "100%", maxWidth: IE_W, display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 12, color: "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><WIcon name="helpCircle" size={14} /></span>
        <span>2 plugin overrides were dropped — those plugins aren't installed here (<span style={ieMono}>k8s-lens, s3-browser</span>).</span>
      </div>
      <div style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>
        <span style={ieMono}>tasty-keybindings-2026-09-09.toml</span> — <b style={{ color: "var(--tasty-text-secondary)" }}>7</b> of 73 bindings change.
        {" "}No <span style={ieMono}>option</span> bindings to migrate.
      </div>
      <div style={{ borderRadius: "var(--tasty-radius)", padding: "12px 14px",
        background: "color-mix(in srgb, var(--tasty-accent-danger) 12%, transparent)",
        border: "1px solid color-mix(in srgb, var(--tasty-accent-danger) 35%, transparent)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, color: "var(--tasty-accent-danger-raised)", fontSize: 13, fontWeight: 600 }}>
          <WIcon name="alertCircle" size={16} /><span>This file can't be read as keybindings</span>
        </div>
        <p style={{ margin: "4px 0 8px", fontSize: 12, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>
          <span style={ieMono}>~/Downloads/settings.json</span> — expected a keybinding export (TOML, a <span style={ieMono}>[keybindings]</span> table);
          parsing stopped at line 1. Nothing was changed.
        </p>
        <Button variant="secondary" size="sm">Choose another file</Button>
      </div>
    </div>
  );
}

// ── Import / Export — the six values the spec left open ───────────
// Shared notice block recipe: tone + glyph + title + lines. Same geometry as
// the parse-failure block, so failure / warning / info differ only in tone.
function IeBlockG({ tone, glyph, title, count, children, action }) {
  return (
    <div style={{ width: "100%", maxWidth: "var(--tasty-settings-content-max-width)", borderRadius: "var(--tasty-radius)",
      padding: "var(--tasty-kb-ie-notice-inset)", background: "color-mix(in srgb, " + tone + " 12%, transparent)",
      border: "1px solid color-mix(in srgb, " + tone + " 35%, transparent)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, color: tone, fontSize: 13, fontWeight: 600 }}>
        <WIcon name={glyph} size={16} /><span>{title}</span>
        {count && <span style={{ marginLeft: "auto", ...ieMono, fontSize: 11, color: tone }}>{count}</span>}
      </div>
      <div style={{ margin: "4px 0 0", fontSize: 12, color: "var(--tasty-text-on-active)", lineHeight: "var(--tasty-line-height-ui)" }}>{children}</div>
      {action && <div style={{ marginTop: 8, display: "flex", gap: 8 }}>{action}</div>}
    </div>
  );
}

// §1 — export failure. Success is a toast (nothing to do); failure replaces the
// Export row's description with an inline block, because the retry lives there.
function IeExportFailG({ reason = "readonly" }) {
  // §7 of the open-values round: the middle clause is a FIXED set, and the OS
  // string is never spliced into the sentence — it gets its own muted mono line.
  const clause = reason === "readonly" ? "the folder is read-only."
    : reason === "denied" ? "you don't have permission to write there."
    : reason === "space" ? "the disk is full."
    : "the write didn't finish.";
  return (
    <div style={{ width: "100%", maxWidth: "var(--tasty-settings-content-max-width)", display: "flex", flexDirection: "column", gap: 8,
      padding: "var(--tasty-kb-ie-notice-inset)", borderRadius: "var(--tasty-radius)",
      background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)" }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: 2, color: "var(--tasty-text-muted)" }}><WIcon name="download" size={16} /></span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>Export</div>
          <p style={{ margin: "2px 0 0", fontSize: 12, color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>
            Writes every binding — general, quick switch, script bindings and plugin overrides — to one file.
          </p>
        </div>
        <span style={{ flex: "none" }}><Button variant="secondary" size="sm" disabled>Export…</Button></span>
      </div>
      <IeBlockG tone="var(--tasty-accent-danger-strong)" glyph="alertCircle" title="The export wasn't written"
        action={<><Button variant="secondary" size="sm">Try again</Button><Button variant="ghost" ground="active" size="sm">Choose another location…</Button></>}>
        <span style={ieMono}>~/tasty/tasty-keybindings-2026-09-14.toml</span> — {clause} Nothing was written.
        {reason === "other" && (
          <div style={{ ...ieMono, marginTop: 4, fontSize: 11, color: "var(--tasty-text-on-active)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
            title="os error 28: No space left on device (os reported)">os error 28: No space left on device</div>
        )}
      </IeBlockG>
    </div>
  );
}

// §2 — bundle warnings other than "plugin not installed". One warning block,
// one line per notice, count in the header. The dropped-override info line
// stays separate (muted, no tone) — it is a fact, not a warning.
function IeBundleNoticesG({ one = false }) {
  const line = (txt) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 6, marginTop: 4 }}>
      <span style={{ flex: "none", color: "var(--tasty-text-muted)" }}>·</span><span>{txt}</span>
    </div>
  );
  return (
    <div style={{ width: "100%", maxWidth: "var(--tasty-settings-content-max-width)", display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 12, color: "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><WIcon name="helpCircle" size={14} /></span>
        <span>2 plugin overrides were dropped — those plugins aren't installed here (<span style={ieMono}>k8s-lens, s3-browser</span>).</span>
      </div>
      <IeBlockG tone="var(--tasty-accent-warning-raised)" glyph="alertTriangle" title="Read with warnings" count={one ? "1 notice" : "4 notices"}
        action={one ? null : <Button variant="ghost" size="sm">Show 1 more</Button>}>
        {one
          ? line(<>1 unknown action was skipped (<span style={ieMono}>tab.pin</span>).</>)
          : <>
            {line(<>Written by a newer schema (<span style={ieMono}>v3</span>, this build reads <span style={ieMono}>v2</span>) — unreadable parts were skipped.</>)}
            {line(<>2 unknown actions were skipped (<span style={ieMono}>pane.zoom_cycle, tab.pin</span>).</>)}
            {line(<>The group <span style={ieMono}>[keybindings.image]</span> is empty — nothing to import from it.</>)}
          </>}
      </IeBlockG>
    </div>
  );
}

// §3 — parsing failure with and without a line number.
function IeParseFailG({ line = true }) {
  return (
    <IeBlockG tone="var(--tasty-accent-danger-strong)" glyph="alertCircle" title="This file can't be read as keybindings"
      action={<Button variant="secondary" size="sm">Choose another file</Button>}>
      <span style={ieMono}>~/Downloads/settings.json</span> — expected a keybinding export (TOML, a <span style={ieMono}>[keybindings]</span> table);
      {line ? <> parsing stopped at line 1.</> : <> the file isn't TOML.</>} Nothing was changed.
    </IeBlockG>
  );
}

// §4 — several conflicts. Count first, in the card; the rows keep their own
// inline reason, so the summary never repeats the list.
function IeConflictSummaryG() {
  const row = (action, from, to, reason) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 4, padding: "8px 0", borderTop: "1px solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 28, flexWrap: "wrap" }}>
        <span style={{ width: "var(--tasty-kb-ie-action-column-width)", flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{action}</span>
        <span style={{ ...ieMono, width: "var(--tasty-kb-ie-from-column-width)", flex: "none", color: "var(--tasty-text-muted)" }}>{from}</span>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><WIcon name="chevronRight" size={14} /></span>
        <span style={{ minWidth: "var(--tasty-kb-ie-slot-min-width)", height: "var(--tasty-kb-ie-slot-height)", display: "inline-flex", alignItems: "center",
          padding: "0 8px", ...ieMono, background: "var(--tasty-surface-raised)", color: "var(--tasty-text-primary)",
          border: "1px solid var(--tasty-accent-danger)", borderRadius: "var(--tasty-radius)" }}>{to}</span>
      </div>
      <div style={{ paddingLeft: "var(--tasty-kb-ie-action-column-width)", display: "flex", alignItems: "center", gap: 6, fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>
        <WIcon name="alertTriangle" size={14} /><span>{reason}</span>
      </div>
    </div>
  );
  return (
    <div style={{ width: "100%", maxWidth: "var(--tasty-settings-content-max-width)", borderRadius: "var(--tasty-radius)", padding: "var(--tasty-kb-ie-notice-inset)",
      background: "color-mix(in srgb, var(--tasty-accent-warning) 11%, transparent)",
      border: "1px solid color-mix(in srgb, var(--tasty-accent-warning) 36%, transparent)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, color: "var(--tasty-accent-warning-raised)", fontSize: 13, fontWeight: 600 }}>
        <WIcon name="alertTriangle" size={16} /><span>Option bindings need a replacement</span>
        <span style={{ marginLeft: "auto", ...ieMono, fontSize: 11 }}>3 of 4 unresolved</span>
      </div>
      <p style={{ margin: "4px 0 0", fontSize: 12, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>
        <span style={{ color: "var(--tasty-accent-danger-raised)", fontWeight: 600 }}>3 conflicts</span> — those shortcuts are already bound.
        The shortcut-conflict popup opens on Apply.
      </p>
      <div style={{ marginTop: 8 }}>
        {row("Toggle vi mode", "Option+V", "Ctrl+Shift+C", "Also bound to Copy")}
        {row("Jump to error", "Option+E", "Ctrl+Shift+K", "Also bound to Clear scrollback")}
        {row("Screenshot to clipboard", "Option+Shift+4", "Ctrl+Shift+P", "Also bound to Command palette")}
      </div>
    </div>
  );
}

// §5 — the axis-modifier Select before anything is chosen.
function IeModifierSelectG({ chosen = false }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 28 }}>
      <span style={{ width: "var(--tasty-kb-ie-from-column-width)", flex: "none", ...ieMono, color: "var(--tasty-text-muted)" }}>Option</span>
      <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><WIcon name="chevronRight" size={14} /></span>
      <WSelect options={chosen ? ["Ctrl+Alt"] : ["Select a modifier"]} style={{ width: "var(--tasty-field-width-md)",
        color: chosen ? "var(--tasty-text-primary)" : "var(--tasty-text-placeholder)" }} />
      {chosen
        ? <span style={{ display: "inline-flex", color: "var(--tasty-accent-success-raised)" }}><WIcon name="check" size={14} /></span>
        : <span style={{ fontSize: 11, color: "var(--tasty-accent-warning-raised)" }}>Not set</span>}
    </div>
  );
}

// ── Move & resize — interaction spec helpers ──────────────────
// A faux popup: mantle titlebar (drag handle) over a surface0 body.
function FauxPopup({ title = "Listening ports", w = 240, h = 150, titlebar = true, region = false, grab = true, children, style }) {
  return (
    <div style={{ width: w, background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden", ...style }}>
      {titlebar && (
        <div style={{ position: "relative", display: "flex", alignItems: "center", justifyContent: "center", height: "var(--tasty-titlebar-height)",
          background: "var(--tasty-bg-app)", borderBottom: "1px solid var(--tasty-separator)", cursor: grab ? "grab" : "default" }}>
          <span style={{ fontSize: 12, color: "var(--tasty-titlebar-fg)" }}>{title}</span>
          <span style={{ position: "absolute", right: 4, top: "50%", transform: "translateY(-50%)", cursor: "default" }}>
            <IconButton size="sm" aria-label="Close" className="faux-close">{ic.x}</IconButton>
          </span>
        </div>
      )}
      {region && (
        <div style={{ display: "flex", alignItems: "center", gap: 6, height: "var(--tasty-titlebar-height)", padding: "0 6px",
          background: "var(--tasty-bg-app)", borderBottom: "1px solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 6, height: "100%", paddingRight: 8, cursor: "grab", color: "var(--tasty-text-secondary)" }}>
            <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.port}</span><span style={{ fontSize: 12 }}>Ports</span>
          </span>
          <div style={{ flex: 1, cursor: "text" }}><Input block icon={ic.search} placeholder="filter…" /></div>
          <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
        </div>
      )}
      <div style={{ height: h, padding: 10, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.7 }}>{children}</div>
    </div>
  );
}

// the 8-direction resize band overlaid on a popup. Real CSS cursors, no visual grip.
function ResizeMap() {
  const band = 12, corner = 16;
  const zone = (style, cursor, label) => (
    <div className="rz-zone" style={{ position: "absolute", cursor, ...style }} title={label}>
      {label && <span style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center",
        fontSize: 9, fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-accent-info-raised)" }}>{label}</span>}
    </div>
  );
  return (
    <div style={{ position: "relative", width: 280, height: 180 }}>
      <FauxPopup w={280} h={104} title="remote_tool — resizable" style={{ position: "absolute", inset: 0 }}>
        <div>min 420×320 · drag any edge or corner</div>
        <div style={{ color: "var(--tasty-text-muted)" }}>no visual grip — cursor only</div>
      </FauxPopup>
      {/* edges */}
      {zone({ top: 0, left: corner, right: corner, height: band }, "ns-resize", "N")}
      {zone({ bottom: 0, left: corner, right: corner, height: band }, "ns-resize", "S")}
      {zone({ left: 0, top: corner, bottom: corner, width: band }, "ew-resize", "W")}
      {zone({ right: 0, top: corner, bottom: corner, width: band }, "ew-resize", "E")}
      {/* corners (take priority over edges) */}
      {zone({ top: 0, left: 0, width: corner, height: corner }, "nwse-resize", "NW")}
      {zone({ top: 0, right: 0, width: corner, height: corner }, "nesw-resize", "NE")}
      {zone({ bottom: 0, left: 0, width: corner, height: corner }, "nesw-resize", "SW")}
      {zone({ bottom: 0, right: 0, width: corner, height: corner }, "nwse-resize", "SE")}
    </div>
  );
}

function CursorRow({ area, cursor, glyph }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "5px 8px", borderRadius: "var(--tasty-radius-sm)",
      fontSize: 12, color: "var(--tasty-text-secondary)", cursor }}>
      <span style={{ width: 26, textAlign: "center", fontSize: 13 }}>{glyph}</span>
      <span style={{ flex: 1 }}>{area}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{cursor}</span>
    </div>
  );
}

function CursorMatrix() {
  return (
    <div style={{ width: 320, display: "flex", flexDirection: "column", gap: 1, padding: 8,
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <CursorRow area="Content (inside, not a handle)" cursor="default" glyph="↖" />
      <CursorRow area="Drag handle — hover" cursor="grab" glyph="✋" />
      <CursorRow area="Dragging" cursor="grabbing" glyph="✊" />
      <CursorRow area="Corner NW / SE" cursor="nwse-resize" glyph="⤡" />
      <CursorRow area="Corner NE / SW" cursor="nesw-resize" glyph="⤢" />
      <CursorRow area="Edge E / W" cursor="ew-resize" glyph="↔" />
      <CursorRow area="Edge N / S" cursor="ns-resize" glyph="↕" />
      <CursorRow area="Close button — hover" cursor="pointer" glyph="✕" />
    </div>
  );
}

// scope clamp — popup pinned to a scope rect, dragged into the corner & stopped
function ClampDemo() {
  return (
    <div style={{ position: "relative", width: 300, height: 200, background: "var(--tasty-bg-app)",
      border: "1px dashed var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <span style={{ position: "absolute", top: 4, left: 6, fontFamily: "var(--tasty-font-mono)", fontSize: 9, color: "var(--tasty-text-muted)" }}>scope rect</span>
      <div style={{ position: "absolute", right: 0, bottom: 0 }}>
        <FauxPopup w={188} h={70} title="popup" grab>
          <div>clamped — no part leaves the scope</div>
        </FauxPopup>
      </div>
    </div>
  );
}

// z-order — two overlapping popups; only the top one's edges respond
function ZOrderDemo() {
  return (
    <div style={{ position: "relative", width: 320, height: 200 }}>
      <div style={{ position: "absolute", left: 0, top: 0 }}>
        <FauxPopup w={190} h={66} title="behind" grab={false}>
          <div style={{ color: "var(--tasty-text-muted)" }}>handles don't respond</div>
        </FauxPopup>
      </div>
      <div style={{ position: "absolute", right: 0, bottom: 0 }}>
        <FauxPopup w={200} h={70} title="front — active" grab>
          <div>top popup owns the cursor; click brings to front</div>
        </FauxPopup>
      </div>
    </div>
  );
}

function Page() {
  return (
    <>
<Section id="palette" title="Command palette">
        <Spec id="command-palette-k" title="Command palette — ⌘K"
          when={<>Top-anchored launcher. A search Input header, a scrollable MenuItem list with the first row pre-highlighted, and a monospace hint footer. The fastest path to any command — every action should be reachable here.</>}>
          <Stage variant="solo center"><Backdrop height={400}><PaletteFrame /></Backdrop></Stage>
          <Meta
            specs={[["width", "480–540px"], ["anchor", "top, ~40–90px"], ["header", <>Input + <span className="tok">--tasty-separator</span></>], ["rows", <>28px MenuItem · first active</>], ["footer", "mono hints ↑↓ ↵ esc"]]}
            tokens={[{ tok: "--tasty-surface-raised", use: "frame", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-surface-active", use: "highlight", color: "var(--tasty-surface-active)" }, { tok: "--tasty-font-mono", use: "hints" }]} />
        </Spec>
        <Spec id="search-field-ground-normal-focused" title="Search field ground — normal · focused"
          when={<>The palette shell is <span className="tok">--tasty-surface-raised</span>, so its search field is an <b>Input ground="raised"</b> (the 10-14 rule, no exception). The box steps down to <code>bg-panel</code> with a <code>border-strong</code> edge, so it reads against the shell before it takes focus. The field opens focused (autoFocus); <b>normal</b> is the box after focus moves away. Width, icon, placeholder, list and footer are unchanged; no copy changes.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <WGroundThemes render={() => (
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
                <WGroundCap>normal — focus elsewhere</WGroundCap><PaletteFrame rows={2} />
                <WGroundCap>focused</WGroundCap><PaletteFrame rows={2} focused value="term" />
              </div>
            )} />
          </Stage>
          <Meta
            specs={[["field ground", <>ground="raised" · no exception</>], ["normal", <>input-raised-bg → bg-panel · input-raised-border → border-strong</>], ["focused", <>bg unchanged · border-focus edge + 1px border-focus ring</>], ["Mocha", "bg-panel = base · border-strong = surface1 · shell surface-raised = surface0"], ["Latte", "bg-panel = base · border-strong = surface1 · shell surface-raised = surface0"], ["ink", "text-primary · placeholder text-placeholder · icon text-muted (unchanged)"]]}
            tokens={[{ tok: "--tasty-input-raised-bg", use: "field fill", color: "var(--tasty-input-raised-bg)" }, { tok: "--tasty-input-raised-border", use: "field edge", color: "var(--tasty-input-raised-border)" }, { tok: "--tasty-border-focus", use: "focused edge + ring", color: "var(--tasty-border-focus)" }, { tok: "--tasty-surface-raised", use: "shell", color: "var(--tasty-surface-raised)" }]} />
        </Spec>
      </Section>

<Section id="ports" title="Listening ports">
        <Spec id="listening-ports-the-table-popup" title="Listening ports — the Table popup"
          when={<>Tools › Listening ports. A <b>660×520</b> modal built directly on the <b>Table</b> component — the canonical example of a data-table surface. Header carries a live filter + refresh; a <b>Show all</b> checkbox toggles between Tasty-owned and system-wide ports; the scan state shows a <b>Spinner</b>. Rows are sortable; the State column renders a StatusDot.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><PortsFrame /></Stage>
          <Meta
            specs={[["frame", "660 × 520 (canonical)"], ["built on", <span className="ic">Table</span>], ["header", "filter Input + refresh"], ["scan", <>Spinner \u2192 \u201cCollecting\u2026\u201d</>], ["footer", "count · Copy address · Close"]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-surface-active", use: "selected row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-success", use: "LISTEN dot", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-font-mono", use: "port/addr/pid" }]} />
          <Note>The 660px width is <b>design-canonical</b> — the 7-column table (Port · Proto · Address · Process · Workspace · Tab · State) needs it, plus a <b>tight 28px leading star column</b>. Don't narrow the frame; let columns truncate instead.</Note>
        </Spec>
        <Spec id="favorites-star-toggle-pinned-section-none-badge" title="Favorites — star toggle, pinned section, NONE badge"
          when={<>Port favorites are keyed by <b>(addr, port)</b> — never the PID, which changes on every restart. A <b>tight 28px leading column</b> in the table carries the star (outline = unregistered, <b>gold starFill</b> = registered, same pair the Explorer sidebar uses); clicking it registers or unregisters <b>immediately</b>, with no confirm step. This is the only writing control in an otherwise read-only popup. A <b>Favorites</b> section sits between the filter row and the table and lists every favorite <b>regardless of the search box, the scope checkbox and the sort</b> — and it always judges LISTEN/NONE against the <b>full system scan</b>, so a favorite that Tasty doesn't own still reads LISTEN. A favorite whose port is not in the scan gets the <b>NONE</b> badge: StatusDot <span className="ic">idle</span>, muted, no pulse — apart from LISTEN (green + pulse) and other states (yellow). Stars can be toggled from <b>either</b> region.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 18 }}>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center", alignItems: "flex-start" }}>
              {[["mixed — LISTEN + NONE", "4 favorites, one stopped", "mixed"],
                ["empty — caption persists", "one muted how-to line", "empty"],
                ["7 favorites — section scrolls", "list capped at 112px (5 rows)", "scrolling"]].map(([label, sub, v]) => (
                <div key={v} style={{ display: "flex", flexDirection: "column", gap: 6, width: 300 }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                    <PortsFavoritesG favorites={v} />
                  </div>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-placeholder)" }}>{sub}</div>
                </div>
              ))}
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
              <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>star states</span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}><PortStarG /><span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>unregistered</span></span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}><PortStarG on /><span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>registered</span></span>
            </div>
          </Stage>
          <Meta
            specs={[["identity", <span className="ic">(addr, port)</span>], ["star", "22×22 hit · tight 28px column"], ["section", "caption 22 + list ≤ 112 (5 rows)"], ["row height", "22 (tree density)"], ["scope", "always system-wide"], ["frame", "unchanged — 660 × 520"]]}
            tokens={[{ tok: "--tasty-port-star-on", use: "registered star", color: "var(--tasty-port-star-on)" }, { tok: "--tasty-port-star-off", use: "outline star", color: "var(--tasty-port-star-off)" }, { tok: "--tasty-port-favorites-bg", use: "section tone", color: "var(--tasty-port-favorites-bg)" }, { tok: "--tasty-port-state-none-dot", use: "NONE dot", color: "var(--tasty-port-state-none-dot)" }, { tok: "--tasty-port-favorites-max-height", use: "scroll cap" }]} />
          <Do><b>Do</b> keep the favorites rows as <b>summary</b> rows (addr:port · process · state), not the 7-column grid — a stopped port has no process/workspace/tab data to show, and the summary row never needs the table's horizontal scroll. The star column width is shared, so stars still line up across both regions.</Do>
          <Note>New strings: <span className="ic">ports.favorites</span> ("Favorites") · <span className="ic">ports.favorites_empty</span> ("No favorites yet") · <span className="ic">ports.favorites_empty_hint</span> · <span className="ic">ports.state_none</span> ("NONE") · <span className="ic">ports.favorites_scope</span> ("system-wide"). Leave 20–40% growth room for ko/ja/de — the caption row and the empty line are single-line by design.</Note>
        </Spec>
        <Spec id="process-column-a-minimum-not-a-fixed-width-on-table-2026-10-08" title="Process column — a minimum, not a fixed width (on Table, 2026-10-08)"
          when={<>Drawn on the shared <b>Table</b> with the popup's own columns (star · Port · Proto · Address · Process · Workspace · State), so header, row height, fonts and padding are the popup's. The <b>Process</b> column has a <b>floor</b>, <span className="tok">--tasty-port-process-col-min-width</span> (200), passed as the Table column's <code>minWidth</code>: it takes spare width and never shrinks below the floor. A column's <code>width</code> is also its floor, so the fixed columns never shrink. <b>Address</b> is fixed at its floor <span className="tok">--tasty-port-addr-col-min-width</span> (140, fits <span className="ic">255.255.255.255</span> and <span className="ic">::</span> forms; IPv6 ellipsises). <b>Process takes all the spare width</b>; Address never grows (2026-10-08). The sum is the popup's <b>column budget</b>. At the popup's own 660 the budget is a little wider, so the table body <b>scrolls horizontally</b>, the existing scroll policy of the popup. <b>No column shrinks away or hides</b>. Process text ellipsises inside its cell; the PID Tag stays visible.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-lg)", alignItems: "flex-start" }}>
            {[["860 — wider than the column budget: Process takes ALL the spare width, Address stays at 140", 860], ["660 — popup width, under the budget (836): the body scrolls sideways; nothing shrinks or hides", 660]].map(([label, tw]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</div>
                <div className="tasty-scroll" style={{ width: tw, overflowX: "auto", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)" }}>
                  <WTable columns={PORTS_COLUMNS} rowKey="port" rows={[
                    { port: 3000, proto: "tcp", addr: "127.0.0.1", proc: "node /usr/local/bin/vite --host --strictPort", pid: 41822, ws: "Project A", state: "LISTEN" },
                    { port: 5432, proto: "tcp", addr: "127.0.0.1", proc: "postgres: checkpointer", pid: 913, ws: null, state: "LISTEN" },
                    { port: 8080, proto: "tcp", addr: "0.0.0.0", proc: "tasty-agent", pid: 50321, ws: "Project B", state: "LISTEN" },
                  ]} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["component", "Table — the popup's column defs, shared"], ["Process", <>strong · <code>minWidth</code> = <span className="tok">--tasty-port-process-col-min-width</span> · ellipsis</>], ["value", "200 — unchanged"], ["Address", <>fixed · <span className="tok">--tasty-port-addr-col-min-width</span> 140 (floor = width)</>], ["spare width", "100% to Process"], ["stage gaps", <>gallery Column stage <span className="tok">--tasty-space-lg</span> between tables · cluster caption <span className="tok">--tasty-space-sm</span></>], ["overflow", "body scrolls horizontally below the column budget (also at 660); no column hides"], ["fixed columns", "width = floor (star · Port · Proto · Workspace · State)"], ["metrics", "Table's own — row/header table-cell-height 28 · cell pad 12 · caps header"], ["zoom", "scales with the UI scale, like every width token"]]}
            tokens={[{ tok: "--tasty-port-process-col-min-width", use: "Process floor" }, { tok: "--tasty-port-addr-col-min-width", use: "Address floor + width" }, { tok: "--tasty-port-star-col-width", use: "leading star column" }, { tok: "--tasty-table-cell-height", use: "header + rows" }]} />
          <Note>Replaces the hand-built schematic (header 24 · rows 26 · mono 11 · PID column). There are no specimen-only numbers left to name: every metric comes from Table or the popup's column widths. New Table column option <code>minWidth</code> (additive).</Note>
        </Spec>
      </Section>

<Section id="remote" title="Remote connections">
        <Spec id="remote-connections-profiles-passkeys" title="Remote connections — profiles + passkeys"
          when={<>Tools › Remote connections. A <b>520×460</b> modal with <b>three top tabs</b>: a type-agnostic <b>remote-profile</b> store (ssh / smb / http / anything), an <b>Attach</b> store (tasty-attach targets), and a separate <b>Passkey</b> credential store. Each tab routes list → form → confirm-delete off a shared header. SSH profiles get a dedicated form; everything else uses a generic key-value editor. The Profiles add-bar carries a <b>protocol filter</b> (right) — a dropdown of the protocols present, checkbox-toggled with Select all / Deselect all / Reset, committed on <b>Apply</b>. Secrets live only in passkeys, referenced by name.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><RemoteFrame /></Stage>
          <Meta
            specs={[["frame", "520 × 460"], ["tabs", "Remote profiles · Attach · Passkeys"], ["routes", "list → form → confirm"], ["filter", <>protocol dropdown (Profiles only) · menu <span className="tok">--tasty-remote-filter-menu-width</span> = 240, measured as the <b>border box</b></>], ["dismiss", <>×/Close/<span className="ic">Esc</span> only</>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-bg-sidebar", use: "tab bar", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-accent-primary", use: "active tab / filter on", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-warning", use: "unknown-type / dangling-ref", color: "var(--tasty-accent-warning-raised)" }]} />
          <Do><b>Do</b> keep secrets in the Passkey store and reference them by name. A profile never holds a secret inline.</Do>
          <Note>The protocol filter is <b>Profiles-only</b> (Passkeys has no filter) and <b>session-only</b> — never persisted; Tasty restarts with every protocol selected. The button reads <span className="ic">Filter</span> when off and turns accent with a <span className="ic">selected/total</span> count when a filter is applied.</Note>
        </Spec>
        <Spec id="passkeys-tab-rows-reveal-unknown-kind" title="Passkeys tab — rows, reveal, unknown kind"
          when={<>The <b>third tab</b>, same 520-wide frame. Each row: <b>name</b> (600) + kind <b>Tag</b> — or the warning badge when the stored kind is unknown — then a mono line <span className="ic">kind · value</span>, masked until revealed. Actions on the right: <b>reveal</b>, <b>edit</b>, <b>delete</b> (<code>trash</code>, as on the other two tabs), IconButton sm. Revealed = IconButton <b>active</b> and the glyph swaps <code>eye</code> → <code>eyeOff</code>. The value line <b>ellipsizes, revealed or not</b>; the action cluster is <code>flex: none</code> and is never pushed out of the row.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 20, flexWrap: "wrap" }}>
            <RemoteFrame tab="passkeys" />
            <div data-theme="latte"><RemoteFrame tab="passkeys" /></div>
          </Stage>
          <Meta
            specs={[["row", "name 600 + Tag / warn badge · mono caption kind · value"], ["mask", "fixed 8-dot mask while hidden"], ["revealed", "IconButton active + eyeOff"], ["long value", "ellipsis at the end · actions flex none, never pushed out"], ["actions", "reveal · edit · trash · IconButton sm, gap 1"], ["add-bar", "Add passkey (Button secondary sm) · no filter"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "unknown kind", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-text-muted", use: "value line", color: "var(--tasty-text-muted)" }]} />
        </Spec>
        <Spec id="attach-tab-tasty-attach-targets" title="Attach tab — tasty-attach targets"
          when={<>The <b>middle tab</b>. An <b>Attach</b> holds everything needed to attach to a <b>remote tasty instance</b> — it either <b>references</b> an ssh profile (<span className="ic">→ prod-web</span>) or carries <b>inline</b> ssh info, plus a <b>remote tasty</b> executable path and a <b>port discovery</b> mode. Splitting this off the ssh profile keeps ssh profiles pure connection info. Rows show name + (label), the reference/host summary, remote-tasty + port-mode captions, and an <b>inactive</b> badge when the referenced profile or inline shell isn't reachable.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><RemoteFrame tab="attach" /></Stage>
          <Meta
            specs={[["rows", "name+label · target · tasty/port"], ["mode", "profile (→ ref) · inline"], ["target", <><span className="ic">→ profile</span> or <span className="ic">user@host</span></>], ["inactive", "peach badge (unreachable)"], ["add-bar", "Add attach (no protocol filter)"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "inactive / profile-missing", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-text-disabled", use: "inactive name", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-separator", use: "row dividers" }]} />
          <Do><b>Do</b> keep <span className="ic">remote_tasty</span> and port discovery on the Attach, not the ssh profile — an ssh profile is reusable connection info; how to find the remote tasty binary is attach-specific.</Do>
        </Spec>
        <Spec id="attach-form-reference-vs-inline" title="Attach form — reference vs. inline"
          when={<>The add/edit route on the Attach tab, on the shared [<span className="tok">--tasty-remote-label-col</span> · 1fr] grid. A <b>Connection</b> segmented toggle switches between <b>SSH profile</b> (an <span className="ic">ssh_ref</span> dropdown of existing ssh profiles) and <b>Direct (inline)</b> (the ssh profile fieldset — host / user / port / shell / passkey). Both share a <b>Remote tasty</b> group: <b>Executable</b> (default <span className="ic">tasty</span>), <b>Port mode</b> (<span className="ic">auto / subcommand / file-unix / file-windows</span>), and an optional <b>Port file</b> that overrides the mode.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>reference an ssh profile</div><RemoteFormFrame variant="attach-ref" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>inline ssh info</div><RemoteFormFrame variant="attach-inline" /></div>
          </Stage>
          <Meta
            specs={[["toggle", "SSH profile ↔ Direct (inline)"], ["ref", "ssh_ref dropdown of ssh profiles"], ["inline", "host · user · port · shell · passkey"], ["remote tasty", "Executable (def. tasty)"], ["port", "auto / subcommand / file-unix / file-windows"], ["port file", "optional — overrides port mode"]]}
            tokens={[{ tok: "--tasty-remote-label-col", use: "shared 112 label column" }, { tok: "--tasty-accent-primary", use: "active tab / segment", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-muted", use: "labels / hints", color: "var(--tasty-text-muted)" }]} />
        </Spec>
        <Spec id="profile-form-ssh-the-112px-1fr-row-grid" title="Profile form — SSH (the [112px · 1fr] row grid)"
          when={<>The add/edit route inside the popup. Every row is the same grid: a <b>fixed 112px right-aligned label</b> + a <b>1fr</b> control (columnGap 12, rowGap 8) so labels never collapse/truncate and all controls share one left edge. SSH gets the dedicated fields (Type · Name · Host · User · Port · Label · Shell + hint · Passkey) — <b>remote_tasty moved to the Attach tab</b>. Body scrolls; the footer is pinned with a <b>full-width</b> separator and right-aligned <b>Cancel (ghost) / Save (primary)</b>. Right: the same form with a <b>validation error</b>.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>SSH · add mode</div><RemoteFormFrame variant="ssh" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>validation error</div><RemoteFormFrame variant="ssh" error="host" /></div>
          </Stage>
          <Meta
            specs={[["frame", "520 × 460 · resizable · headless"], ["row", <>[<span className="tok">--tasty-remote-label-col</span> 112 · 1fr] · gap 12/8</>], ["label", "right-aligned · text-muted · 13"], ["body/footer", "flex:1 scroll · pinned footer"], ["footer sep", "full-width 1px · buttons inset 16/12"], ["error", "accent-danger caption (11)"]]}
            tokens={[{ tok: "--tasty-remote-label-col", use: "112px label column" }, { tok: "--tasty-text-muted", use: "labels / hints", color: "var(--tasty-text-muted)" }, { tok: "--tasty-separator", use: "footer + tab divider" }, { tok: "--tasty-accent-primary", use: "active tab · Save", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-danger", use: "validation error", color: "var(--tasty-accent-danger-raised)" }]} />
          <Do><b>Do</b> keep the label column fixed at 112px (not 1fr-negotiated) — egui's auto grid collapsed it and truncated labels, so the form uses a manual two-column row.</Do>
        </Spec>
        <Spec id="generic-passkey-forms-badges" title="Generic & Passkey forms · badges"
          when={<>A non-ssh Type swaps the SSH grid for a <b>generic key-value editor</b> (Type · Name · FIELDS rows `[112 · 1fr · 28]` + Add field · Passkey), with a peach <b>Unknown type</b> badge when the type isn't registered (still saves). The <b>Passkey</b> tab's form has Name · Kind (<span className="ic">path</span> / <span className="ic">inline</span> segmented) · Value — singleline path vs. 3-row multiline secret — plus the local-only note.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>generic · unknown type</div><RemoteFormFrame variant="generic" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>passkey · kind = path</div><RemoteFormFrame variant="passkey-path" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>passkey · kind = inline</div><RemoteFormFrame variant="passkey-inline" /></div>
          </Stage>
          <Meta
            specs={[["generic", "Type · Name · FIELDS · Passkey"], ["field row", "[112 key · 1fr value · 28 ✕]"], ["unknown type", "peach badge (saves anyway)"], ["passkey kind", "path = singleline · inline = 3-row"], ["secret", "name-reference only · note always shown"], ["footer", "ghost Cancel / primary Save (both forms)"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "unknown-type / dangling badge", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-primary", use: "selected kind segment (accent fill)", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-input-bg", use: "inline secret field", color: "var(--tasty-input-bg)" }, { tok: "--tasty-remote-label-col", use: "shared 112 column" }]} />
          <Note>Other states (brief §5/§ST): <b>dangling passkey</b> reuses the same peach badge ("passkey missing"); <b>detecting</b> shows "detecting…" / "detection failed (disabled)" + Re-detect after a shell=auto save. Secrets live only in passkeys — a profile holds a <b>name reference</b>, never an inline secret.</Note>
        </Spec>
        <Spec id="segmented-active-is-an-accent-fill-tab-strips-keep-the-underline" title="Segmented active is an accent fill — tab strips keep the underline"
          when={<>Two components were drifting into one another. A <b>tab strip</b> (the window's Profiles / Attach / Passkeys bar) marks the current tab with a <b>2px accent underline</b> and a weight change, no fill. A <b>segmented control</b> (the attach form's <code>profile</code> / <code>inline</code> switch, the clipboard type segment, the apply-preset scope) marks the active segment with an <b>accent-primary fill</b> and <span className="tok">--tasty-text-on-accent</span> ink. <span className="tok">--tasty-surface-active</span> is the <b>row-selection</b> fill and is not used by either — that was the gallery's bug, now fixed.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>tab strip — underline</div>
              <RemoteFrame tab="attach" ssh="none" />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>segmented — accent fill</div>
              <RemoteFormFrame variant="attach" />
            </div>
          </Stage>
          <Meta
            specs={[["tab strip", <>2px <span className="tok">--tasty-accent-primary</span> underline · weight 600</>], ["segmented", <>fill <span className="tok">--tasty-accent-primary</span> · ink <span className="tok">--tasty-text-on-accent</span></>], ["inactive segment", <span className="tok">--tasty-surface-raised</span>], ["scope", "every remote segment + clipboard type + preset scope"], ["not used", <><span className="tok">--tasty-surface-active</span> (row selection)</>]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "active segment / underline", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-on-accent", use: "active segment ink", color: "var(--tasty-text-on-accent)" }, { tok: "--tasty-surface-raised", use: "inactive segment", color: "var(--tasty-surface-raised)" }]} />
          <Note>Rule of thumb: an <b>underline</b> switches a <b>view</b>, a <b>fill</b> switches a <b>value</b>. The remote tab bar navigates; the attach switch sets a field.</Note>
        </Spec>

        <Spec id="from-ssh-config-a-second-subordinate-list" title="From ssh config — a second, subordinate list"
          when={<>Hosts parsed out of <code>~/.ssh/config</code> sit <b>below the profiles, in the same scroll</b> — it is the same question (“which machine?”), and a separate tab would hide the answer. Everything about the section says <b>one tier down</b>: a section header instead of a card, the <b>source path in mono</b> so the origin is unambiguous, <b>two lines</b> per row instead of the profile row's three, secondary ink on the alias, and <b>no per-row icon buttons</b> — one ghost <b>Add profile</b>, which is the import action that already exists. An already-imported host shows a muted <b>in profiles</b> Tag and no action, so importing twice is not offered.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>3 hosts · one already a profile · long alias</div>
              <RemoteFrame />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {[["no hosts", "empty"], ["no file", "missing"], ["unreadable config (any open error · directory · not UTF-8)", "unreadable"]].map(([label, st]) => (
                <div key={st} style={{ display: "flex", flexDirection: "column", gap: 6, width: 300 }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ padding: "8px 12px", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)" }}>
                    <LocalSshSection state={st} />
                  </div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["place", "below the profile list, same scroll"], ["header", <>11px uppercase label · <span className="tok">--tasty-font-mono</span> source path · count</>], ["separator", <>1px <span className="tok">--tasty-border-frame</span> above the section</>], ["row", "alias (13, secondary) + user@host:port (mono 11, muted)"], ["action", "ghost Add profile — the existing import, no new behaviour"], ["already imported", <>Tag <b>in profiles</b>, no action</>], ["empty / failure", "one muted line each, no error tone"], ["long alias", "ellipsises; the target line never wraps"]]}
            tokens={[{ tok: "--tasty-border-frame", use: "section rule", color: "var(--tasty-border-frame)" }, { tok: "--tasty-text-secondary", use: "alias + section label", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "target · source path · states", color: "var(--tasty-text-muted)" }]} />
          <Dont><b>Don't</b> give these rows the profile row's edit / delete / re-detect buttons. The file is the user's own; tasty reads it and never writes it.</Dont>
          <Note>A missing or unreadable <code>~/.ssh/config</code> is <b>not an error</b> — one muted line, no warning tone, section header stays so the origin is still explained. <b>States (2026-09-29)</b>: <b>empty</b> = read, 0 hosts → “No hosts in ~/.ssh/config.” · <b>missing</b> = no file → “No ~/.ssh/config found.” · <b>unreadable</b> = the file exists but can't be read — permission denied, any other open error, a directory at that path, or invalid UTF-8 — one cause-neutral line “Can't read ~/.ssh/config. Check the file and its permissions.” The UI does not split causes; an undecodable file must not fall through to the empty line.</Note>
        </Spec>
      </Section>

      <Section id="remoteattach" title="Add remote workspace">
        <Spec id="two-pane-remote-workspace-picker-4-states" title="Two-pane remote-workspace picker — 4 states"
          when={<>Sidebar workspaces-header / new-workspace(<span className="ic">+</span>) right click → <b>Add remote workspace</b>. A <b>680×460</b> modal in the <b>remote_tool</b> language (Scrim · content-drawn header · <span className="tok">--tasty-bg-panel</span> frame). It <b>consumes</b> the <b>tasty-attach</b> profiles registered on remote_tool's Attach tab — it never edits them. <b>Left (240px)</b>: the attach-profile list, single select, with an <b>inactive</b> badge on profiles whose last detection failed. <b>Right</b>: the selected profile's remote workspaces. <b>Footer</b>: <b>Connect</b> (primary, enabled <i>only</i> when a selectable remote workspace is chosen) · <b>Cancel</b> (ghost, always enabled — closes, aborting any in-flight connect).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><RemoteAttachFrame state="loaded" /></Stage>
          <Meta
            specs={[["frame", "680 × 460"], ["left", "240px attach-profile list (single select)"], ["right", "remote workspaces — 4 states"], ["ws row", "dot · name · panes · busy / in-use"], ["connect", "enabled only with a selectable ws"], ["cancel", "always enabled · aborts connect"], ["dismiss", <>×/Cancel/<span className="ic">Esc</span></>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-bg-sidebar", use: "left pane", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-surface-active", use: "selected row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "selected bar · Connect", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-attached", use: "in-use (claimed elsewhere)", color: "var(--tasty-accent-attached)" }, { tok: "--tasty-accent-success", use: "busy dot", color: "var(--tasty-accent-success-raised)" }]} />
          <Do><b>Do</b> block selection of a remote workspace already <b>attached on another client</b> (lavender <span className="ic">in use</span> badge, dot dimmed) — mirror it there, not twice. This is the same <span className="tok">--tasty-accent-attached</span> axis as the sidebar attached ring.</Do>
          <Note>The right pane runs four states off the left selection: <b>initial</b> (nothing picked — placeholder), <b>connecting</b> (Spinner — SSH tunnel + list can take seconds), <b>error</b> (danger glyph + reason + <b>Retry</b>), and <b>loaded</b> (the workspace list, or a muted “no workspaces” empty when the remote is reachable but idle).</Note>
        </Spec>
        <Spec id="right-pane-states-initial-connecting-error-empty" title="Right-pane states — initial · connecting · error · empty"
          when={<>The three non-list states, plus the <b>empty</b> remote (plan B: the list path with the pre-selected “+ New workspace” row), driven by the left selection. Each is a centered column (glyph → title → one muted line), so the pane never looks broken while a connect is pending or failed.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>initial — nothing picked</div><RemoteAttachFrame state="initial" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>connecting</div><RemoteAttachFrame state="loading" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>error — retry</div><RemoteAttachFrame state="error" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty — reachable, no workspaces (list path)</div><RemoteAttachFrame state="empty" emptyPlan="B" /></div>
          </Stage>
          <Meta
            specs={[["initial", "remote glyph + prompt · Connect disabled"], ["connecting", "Spinner + “Connecting…”"], ["error", "danger glyph + reason + Retry"], ["empty", "caps header + pre-selected “+ New workspace” row + one muted line · Create & connect enabled"]]}
            tokens={[{ tok: "--tasty-text-placeholder", use: "initial glyph", color: "var(--tasty-text-placeholder)" }, { tok: "--tasty-accent-danger", use: "error glyph", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-spinner-track", use: "connecting spinner" }]} />
        </Spec>
        <Spec id="new-workspace-row-the-escape-from-a-dead-end-remote" title="“+ New workspace” row — the escape from a dead-end remote"
          when={<>The <b>first row</b> of the loaded list opens the other path: instead of mirroring a workspace the remote already has, <b>create one there</b> and mirror that. It is deliberately a <b>row in the list</b>, not a button or a second tab — so it inherits the list's <b>select-then-confirm</b> contract: clicking selects, the footer confirms (§6-5). No name or cwd is ever asked for; the remote's defaults are used, which the row's tooltip states outright.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><RemoteAttachFrame state="loaded" newSelected /></Stage>
          <Meta
            specs={[["position", "first row of the scroll area, under the caps header"], ["height", "34 — identical to a ws row (8/12 padding, 13px/500)"], ["glyph", <><span className="ic">plus</span> 14px inside the <b>status-dot slot</b> → same name-column left edge</>], ["right slot", "muted “on remote” instead of panes/busy"], ["separator", <>1px <span className="tok">--tasty-separator</span>, 4 above / 4 below</>], ["sticky", "no — scrolls with the list"], ["footer", "label becomes Create & connect while selected"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "glyph + label + select bar", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-overlay-hover", use: "hover", color: "var(--tasty-overlay-hover)" }, { tok: "--tasty-surface-active", use: "selected", color: "var(--tasty-surface-active)" }, { tok: "--tasty-separator", use: "group divider", color: "var(--tasty-separator)" }, { tok: "--tasty-accent-danger", use: "create failed", color: "var(--tasty-accent-danger-raised)" }]} />
          <Do><b>Do</b> separate it from the real workspaces on <b>three channels at once</b> — glyph, accent label, and the divider. Colour alone would fail both colour-blind users and the 4.5:1 rule; the <span className="ic">plus</span> glyph and the divider carry the meaning without it. When the row is <b>selected</b> the label drops the accent for <span className="tok">--tasty-text-primary</span> (accent over <span className="tok">--tasty-surface-active</span> measures 3.17:1) — the glyph and the 2px bar keep the accent, so nothing is lost.</Do>
          <Dont><b>Don't</b> give it a status dot, pane count, or a busy/in-use badge — none of them exist for a workspace that hasn't been created. The dot slot is <b>reused</b> (not padded out) so the name column still lines up.</Dont>
        </Spec>

        <Spec id="new-workspace-row-rest-hover-selected-creating-failed" title="New-workspace row — rest · hover · selected · creating · failed"
          when={<>All five row states at pane width (440px), each with the first real ws row beneath so the shared alignment line is visible. <b>Creating</b> swaps the glyph for a 14px <b>Spinner</b> and the label for “Creating workspace…” (§6-3: inline, not a full-pane takeover — the user keeps the list they were reading, and the roundtrip is seconds, not minutes). <b>Failed</b> keeps the row and hangs the remote's own message under it with a <b>Try again</b> button (§6-4), clamped to 3 lines with the full text in <span className="tok">title</span>.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["rest", {}], ["hover", { hover: true }], ["selected", { selected: true }], ["creating (list muted, Connect disabled)", { selected: true, phase: "creating" }], ["create failed — inline, list kept", { selected: true, phase: "failed" }]].map(([label, p]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6, width: 440 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <RaNewRow {...p} />
                  <RaWsPeek muted={p.phase === "creating"} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["rest", "transparent · accent glyph + label"], ["hover", <span className="tok">--tasty-overlay-hover</span>], ["selected", <>+ 2px inset accent bar · label switches to <span className="tok">--tasty-text-primary</span> (accent on <span className="tok">--tasty-surface-active</span> is only 3.17:1) — identical to a ws row</>], ["creating", "Spinner 14 in the glyph slot · label muted · list names in text-muted, not replaced, no opacity"], ["failed", "danger glyph · message clamp 3 lines · Try again (secondary sm)"], ["motion", "state swaps are instant (0ms) — the Spinner is the one allowed exception"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "rest / selected label", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-muted", use: "creating label", color: "var(--tasty-text-muted)" }, { tok: "--tasty-accent-danger", use: "failed glyph + message", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note><b>i18n.</b> “New workspace” grows to “Neuer Arbeitsbereich” / “새 워킬스페이스”. The label is <span className="tok">flex: 0 1 auto; min-width: 0</span> with single-line ellipsis and the trailing “on remote” caption is the first thing to be pushed out — the row never wraps and never grows past 34px.</Note>
        </Spec>

        <Spec id="empty-remote-plan-a-center-state-cta-vs-plan-b-list-path" title="Empty remote — plan A (center-state + CTA) vs plan B (list path)"
          when={<>§6-1, the decision that sets the implementation's render branch. Before this change a reachable-but-empty remote ended at a center-state with nothing but <b>Cancel</b>. <b>Plan A</b> keeps that center-state and adds a CTA button. <b>Plan B</b> takes the <b>list</b> path even when empty: caps header + the one “+ New workspace” row, with a muted line explaining why nothing follows it.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>plan A — center-state + CTA</div><RemoteAttachFrame state="empty" emptyPlan="A" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-accent-primary-raised)" }}>plan B — list path (recommended)</div><RemoteAttachFrame state="empty" emptyPlan="B" /></div>
          </Stage>
          <Meta
            specs={[["recommendation", "plan B"], ["why", "one render path, one affordance, one confirm route"], ["plan B copy", "“‹host› is reachable but has no workspaces yet.”"], ["footer", "Create & connect, enabled (the row is pre-selected)"], ["plan A cost", "a second way to trigger the same action, confirmed differently"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "explanatory line", color: "var(--tasty-text-muted)" }, { tok: "--tasty-accent-primary", use: "row label", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note><b>Recommended: plan B.</b> Plan A's CTA is a <i>button</i>, so it fires immediately — the same action would then be confirmed two different ways depending on whether the remote happened to be empty, and the implementation would carry two render branches plus two handlers. Plan B has one list, one row, one confirm route, and the empty case degrades to “the list has exactly one row.” In plan B the row is <b>pre-selected</b> on an empty remote, so <b>Create &amp; connect</b> is live the moment the pane loads — the dead end is gone without adding a control.</Note>
        </Spec>

        <Spec id="decisions-the-eight-open-questions-resolved" title="Decisions — the eight open questions, resolved"
          when={<>What was chosen and why. This is the implementation spec.</>}>
          <Note>
            <b>1 · empty state</b> — <b>plan B</b> (list path, one pre-selected row). One render branch, one confirm route.<br />
            <b>2 · distinction</b> — <span className="ic">plus</span> glyph in the dot slot <b>+</b> accent label <b>+</b> 1px separator below (4/4 margins). Weight stays 500, size stays 13 — the row must read as a peer of the rows below it, not as a header.<br />
            <b>3 · creating</b> — <b>inline in the row</b> (glyph → Spinner, label → “Creating workspace…”), list below stays readable, names in text-muted, and inert. A full-pane <i>connecting</i> takeover would throw away the list for a 1–3s roundtrip.<br />
            <b>4 · failure</b> — <b>inline under the row</b>, message clamped to 3 lines (full string in <span className="tok">title</span>) + <b>Try again</b>. The connect-error center-state is right for “we never got a list”; here we <i>have</i> the list and the user's next move is usually to pick an existing workspace instead — don't hide it.<br />
            <b>5 · confirm (core)</b> — <b>select, then footer</b>. The user chose a placement <i>inside a list</i>; a single row that fires on click while its neighbours only select is the inconsistency, and it also loses the reversible “I clicked it, now what?” moment before a remote-mutating action. Cost accepted: the row carries a selected state.<br />
            <b>6 · height &amp; sticky</b> — <b>34px, not sticky</b>. Same box as a ws row; sticky would stack a second frozen band under the caps header for a list that rarely exceeds ~8 rows.<br />
            <b>7 · footer label</b> — <b>“Create &amp; connect”</b> while the new row is selected, “Connect” otherwise. Because §6-5 chose footer confirmation, the button must say which of the two things it will do.<br />
            <b>8 · tooltip</b> — <b>yes</b>, on the row: “Creates a workspace on the remote with its default name and cwd — you won't be asked for a name — then mirrors it here.” Not asking for a name is the surprising part, so it gets said where the click happens.
          </Note>
          <Note><b>Interaction contract.</b> Arrow keys traverse the new row as row 1; <span className="ic">Enter</span> confirms the selection (same as the footer). Changing the left-hand profile resets the selection and the row's phase to rest. During creation the left profile list and the right list are inert; <b>Cancel</b> stays live — it closes the popup and abandons the in-flight request, and a workspace already created on the remote is <b>not</b> rolled back (a flash message on close says so; no extra confirm). On success the popup closes straight into attach — no interstitial “created” step. The caps header keeps its wording: <span className="ic">REMOTE WORKSPACES · ‹profile›</span> still describes the group, and the new row's own label says it is a creation.</Note>
          <Note><b>New icons: none.</b> <span className="ic">plus</span>, <span className="ic">alertTriangle</span> and <span className="ic">refresh</span> already exist in <span className="tok">icons/</span>. <b>New tokens: none</b> — the row is built from <span className="tok">--tasty-accent-primary</span>, <span className="tok">--tasty-overlay-hover</span>, <span className="tok">--tasty-surface-active</span>, <span className="tok">--tasty-separator</span>, <span className="tok">--tasty-accent-danger</span> and the existing spacing steps.</Note>
        </Spec>
      </Section>

      <Section id="filepicker" title="File picker">
        <Spec id="native-file-picker-local-remote-one-component" title="Native file picker — local & remote, one component"
          when={<>Tasty's own <b>Open file</b> dialog, replacing the OS-native picker so it can browse a <b>remote attach host</b> over the <b>same SSH mechanism</b> as attach — the OS picker only ever sees the local disk. It's a <b>select-and-confirm</b> dialog (pick a path, hand it back, close), <b>not</b> the Explorer surface (that's a free-roam tab). A <b>640×480</b> modal in the <b>remote_tool</b> popup language: Scrim · content-drawn header · <span className="tok">--tasty-bg-panel</span> frame. <b>Local and remote are the same component</b> — they differ only in the header host indicator and the breadcrumb root. Rows reuse the <span className="ic">folder</span>/<span className="ic">file</span> list language; folder <b>double-click</b> descends, file <b>click</b> fills the name field, file <b>double-click</b> = Open.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>local</div><FilePickerFrame /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>remote (host badge — recommended)</div><FilePickerFrame remote indicator="badge" /></div>
          </Stage>
          <Meta
            specs={[["frame", "640 × 480 · PopupDef"], ["header", "glyph · title · host indicator · ✕"], ["path bar", "breadcrumb + refresh · bg-sidebar"], ["row", "checkbox? · icon · name · size · modified"], ["footer", "name field · type filter · Cancel / Open"], ["open", "enabled only with a selection"], ["dismiss", <>×/Cancel/<span className="ic">Esc</span> · Open = <span className="ic">Enter</span></>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-bg-sidebar", use: "path / list-header bar", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-surface-active", use: "selected row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "selected bar · folder glyph · Open · crumb link", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-info", use: "remote host indicator", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-accent-danger", use: "error glyph", color: "var(--tasty-accent-danger-raised)" }]} />
          <Do><b>Do</b> keep local and remote a single component with a single set of states — the target only changes the <b>host indicator</b> and the <b>breadcrumb root</b> (<span className="tok">--tasty-font-mono</span> <span className="ic">user@host</span> vs. <span className="ic">/</span>), never the layout.</Do>
          <Note>Out of scope this pass (deferred, not designed): a favorites/recent-locations rail and per-host history. Multi-select is spec'd below but ships behind single-select by default.</Note>
        </Spec>

        <Spec id="remote-indicator-three-candidates" title="Remote indicator — three candidates"
          when={<>§6.1 open decision: how to signal "this dialog is looking at a remote host, not your local disk," clearly but not loudly. Three candidates on the same <span className="tok">--tasty-accent-info</span> axis. <b>Badge</b> (recommended): a mono <span className="ic">user@host</span> chip beside the title — explicit about <i>which</i> host, reuses the attach/remote visual language. <b>Glyph + host</b>: swap the header icon to <span className="ic">remote</span> and print the host inline — lighter, no chip. <b>Border</b>: tint the whole frame edge + a 2px top strip — unmissable but the heaviest; use only if remote/local confusion is a real hazard.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>A · host badge</div><FilePickerFrame remote indicator="badge" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>B · glyph + host</div><FilePickerFrame remote indicator="glyph" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>C · frame border</div><FilePickerFrame remote indicator="border" /></div>
          </Stage>
          <Meta
            specs={[["A badge", "mono host chip · info-tinted · explicit"], ["B glyph", "remote icon + inline host · lightest"], ["C border", "info edge + 2px top strip · loudest"], ["shared axis", "--tasty-accent-info (never danger)"], ["local", "no indicator · file glyph · / root"]]}
            tokens={[{ tok: "--tasty-accent-info", use: "all three indicators", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-font-mono", use: "host string", color: "var(--tasty-text-secondary)" }]} />
          <Dont><b>Don't</b> use a danger/warning tone for the remote indicator — remote is a normal, expected mode, not an error. Reserve peach/red for the connection-lost state below.</Dont>
        </Spec>

        <Spec id="states-loading-empty-permission-connection-lost-multi-select" title="States — loading · empty · permission · connection lost · multi-select"
          when={<>The body region swaps between list and status states without changing the frame. <b>Loading</b>: Spinner while the directory reads (remote adds "over SSH"). <b>Empty</b>: muted <span className="ic">folderOpen</span> + "This folder is empty." <b>Permission denied</b> (local) and <b>Remote connection lost</b> share the layout — danger glyph · title · one reason line · action — but differ in copy and action (<b>Retry</b> vs. <b>Reconnect</b>, which resumes from the last folder). <b>Multi-select</b> (§6.2): a checkbox column, a comma-joined name field, and an <span className="ic">N selected</span> count — spec'd for the future, single-select is the default.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>loading (remote)</div><FilePickerFrame remote state="loading" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty folder</div><FilePickerFrame state="empty" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>permission denied (local)</div><FilePickerFrame state="error-perm" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>connection lost (remote)</div><FilePickerFrame remote state="error-conn" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>multi-select (remote)</div><FilePickerFrame remote multi /></div>
          </Stage>
          <Meta
            specs={[["loading", "Spinner · reads dir (remote: over SSH)"], ["empty", "folderOpen glyph + muted line"], ["error", "danger glyph · title · reason · action"], ["perm vs. conn", "Retry vs. Reconnect (resumes)"], ["multi", "checkbox col · joined names · N selected"], ["Open", "disabled while loading / error / empty"]]}
            tokens={[{ tok: "--tasty-accent-danger", use: "error glyph", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-placeholder", use: "empty glyph", color: "var(--tasty-text-placeholder)" }, { tok: "--tasty-spinner-track", use: "loading spinner" }]} />
          <Note>Row focus ring (keyboard nav) is shown on <span className="ic">pipeline.yaml</span> in the loaded frames — 1px <span className="tok">--tasty-accent-primary</span> outline, distinct from the filled selection background so focus and selection never merge visually.</Note>
        </Spec>

        <Spec id="save-mode-one-confirm-in-the-footer" title="Save mode — one confirm, in the footer"
          when={<>The same picker, asked to <b>produce</b> a path instead of read one (Settings → Export, and every future save target — <code>rfd</code> blocks the calling thread, so no OS dialog). <b>There is exactly one confirm control, and it is the footer's primary button</b> — the <b>File name</b> row the picker already owns becomes editable and the button reads <b>Save</b>. No second row is appended below the view, because a second button would point at a second target and the screen could hold two answers at once.<br /><br /><b>One target.</b> Selecting a list row does not confirm anything — it <b>writes that row's name into the input</b>, and the input is the only thing the button reads. Typing after a pick keeps the row selected only while the name still matches; edit a character and the selection clears. So "the picked file" and "the typed name" are never two different paths.<br /><br /><b>Overwrite is inline, not a second dialog.</b> When the typed name already exists in this folder, a warning line appears above the buttons and the primary <b>relabels to Overwrite</b> — the decision stays on the button the user is already reaching for. A modal confirm on top of a modal picker would be a second scrim for a reversible write.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>save — typed name (new file)</div><FilePickerFrame mode="save" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>save — existing file picked → Overwrite</div><FilePickerFrame mode="save" save="picked" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>save — name edited after the pick → selection cleared</div><FilePickerFrame mode="save" save="edited" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>deep path — fits at 640, nothing folds</div><FilePickerFrame mode="save" deep elideStage={false} /></div>
          </Stage>
          <Meta
            specs={[["title", "Save file (open mode: Open file)"], ["confirm", "footer primary only — one control"], ["labels", "Save · Overwrite when the name exists"], ["input", "editable · placeholder “Type a file name”"], ["list pick", "writes the name into the input → Overwrite"], ["selection", "clears as soon as the name diverges → Save"], ["overwrite", "11px warning line above the buttons"], ["disabled", "Save disabled while the name is empty"], ["breadcrumb", "folds only when the measured row doesn't fit — see Path bar"], ["footer", "never shrinks — the input absorbs it"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "overwrite line", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-input-bg", use: "name field", color: "var(--tasty-input-bg)" }, { tok: "--tasty-text-placeholder", use: "empty name", color: "var(--tasty-text-placeholder)" }, { tok: "--tasty-separator", use: "footer rule", color: "var(--tasty-separator)" }]} />
          <Note><b>Long-path shrink rule (fixes a defect in open mode too).</b> Overflow is absorbed <b>in the path bar</b>, never by the footer: the breadcrumb is the only flexible child (<code>flex:1; min-width:0</code>) and elides in the <b>middle</b> — root + <span className="ic">…</span> + the last two segments, since the current folder and its parent are what orient you; the <span className="ic">…</span> lists the hidden ancestors on click. Footer label, filter chip and both buttons are <code>flex:none</code>; only the name input shrinks. No horizontal scroll, and Cancel / Open / Save can never be clipped.</Note>
          <Dont><b>Don't</b> append a save row under the list. Two confirm controls in one dialog means two targets — pick <span className="ic">package.json</span>, then type a different name, and the screen holds two answers with no rule for which wins.</Dont>
        </Spec>

        <Spec id="gestures-folder-targets-the-six-open-branches" title="Gestures &amp; folder targets — the six open branches"
          when={<>The save-mode decision left six branches undecided, and because <b>save and open are the same view</b>, each answer had to hold in both modes. One gesture table now covers them: <b>single click selects, double click descends</b> — in both modes, for both kinds. A <b>folder is never a save target</b>: the footer button still reads the input only, so a selected folder can't change what gets written; a muted line says so where the reading happens. In save mode a <b>file's double-click stops at select</b> — it writes the name, which <i>is</i> the overwrite state, and the warning line has to be readable before the write. Only open mode confirms on a file's double-click.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>save — folder selected · not a save target</div><FilePickerFrame mode="save" folderSel /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>open — folder selected · Open enters it</div><FilePickerFrame folderSel /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>… menu open — at the 320 picker floor, where the deep path really folds</div><FilePickerFrame mode="save" deep crumbMenu w={320} h={420} /></div>
          </Stage>
          <Meta
            specs={[["single click", "selects the row — file or folder, both modes"], ["double click folder", "descends — both modes"], ["double click file", "open: confirms · save: selects only"], ["folder + Save", "never a target — muted footer line, button unchanged"], ["folder + Open", "enters it (Open is the keyboard route to descend)"], ["elision", "overflow-driven — one ancestor at a time, no depth threshold"], ["… tooltip", "Show 3 hidden folders (singular: 1 hidden folder)"], ["… menu", "content-measured, 180–320 band = BORDER-BOX outer width, path order"], ["… menu rows", "shared MenuItem — 28 · pad-x 12 · folder 16 · text-primary"], ["specimen", <>320 (<span className="tok">--tasty-fp-popup-min-width</span>) — the width where it folds</>]]}
            tokens={[{ tok: "--tasty-fp-crumb-max-width", use: "180 — one crumb's cap (NEW)" }, { tok: "--tasty-fp-crumb-menu-min-width", use: "180 — … menu floor (NEW)" }, { tok: "--tasty-fp-crumb-menu-max-width", use: "320 — … menu ceiling (NEW)" }, { tok: "--tasty-text-muted", use: "folder-target line", color: "var(--tasty-text-muted)" }, { tok: "--tasty-menu-bg", use: "… menu fill", color: "var(--tasty-menu-bg)" }, { tok: "--tasty-menu-border", use: "… menu edge", color: "var(--tasty-menu-border)" }, { tok: "--tasty-menu-item-padding-x", use: "row pad-x 12" }]} />
          <Note><b>Elision is measured, not counted.</b> The path bar elides only when the crumb row doesn't fit, and it drops <b>one ancestor at a time</b> from the middle — a six-segment path that fits stays whole. The floor is root + <span className="ic">…</span> + the current folder; the parent is the first tail segment to go. Separately, a single crumb longer than <span className="tok">--tasty-fp-crumb-max-width</span> ellipsises inside itself — a different axis from the path's middle elision, which is why it has its own token.</Note>
          <Note><b>Open mode changes too</b> (same view, same table): folder single-click now <i>selects</i> instead of doing nothing, and <b>Open</b> with a folder selected descends — which is also what <span className="ic">Enter</span> does, so the keyboard route to "go into this folder" exists without a second control. File behaviour in open mode is unchanged.</Note>
        </Spec>

        <Spec id="path-bar-what-gives-way-when-the-folded-path-still-doesn-t-fit" title="Path bar — what gives way when the folded path still doesn't fit"
          when={<>Folding the middle (<code>root › … › parent › current</code>) is not always enough: at <b>400px</b> with 53-character folder names the current folder used to be <b>clipped without an ellipsis</b>. The bar now allocates the width it actually has — <b>the path bar minus the Up and Refresh buttons and their gaps</b>, never the popup width — in a fixed priority order: <b>current folder → parent → root → the … menu</b>. Five steps, each taken only when the one before it has hit its floor.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "flex-start" }}>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>stage diagram (forced) — step 2 · parent at its 64 floor</div>
                <FilePickerFrame pathKind="longtwo" />
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>stage diagram (forced) — step 5 · … › current</div>
                <FilePickerFrame pathKind="longtwo" w={400} h={360} single />
              </div>
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>440×300 · long UNC root — capped at 180, nothing folds</div>
                <FilePickerFrame pathKind="longroot" w={440} h={300} />
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>stage diagram (forced) — save / overwrite · footer untouched</div>
                <FilePickerFrame pathKind="longtwo" mode="save" save="picked" w={400} h={360} single />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["measure", "path-bar width − Up − Refresh − gaps (not the popup width)"], ["specimens", "cards marked “stage diagram” force the fold to show the step; unmarked cards are measured"], ["product gallery", "measured only — a width ladder: longtwo at 640 · 520 · 440 · 360 · 320, longroot at 320, each labelled with the step crumb_alloc::plan returns"], ["crumb type", <>11 · <span className="tok">--tasty-font-size-caption</span> · current = text-primary, no extra weight</>], ["1 fold", "ancestors → … menu, one per step (approved 09-14)"], ["2 parent", <>180 → 64 (<span className="tok">--tasty-fp-crumb-min-width</span>)</>], ["3 current", <>180 → 96 (<span className="tok">--tasty-fp-crumb-current-min-width</span>)</>], ["4 parent folds", "root › … › current"], ["5 root folds", "… › current — the design floor"], ["grow back", <>floor + 8 (<span className="tok">--tasty-fp-bar-hysteresis</span>)</>], ["current ellipsis", "at the FRONT (…-bbbb) — the tail names the folder"], ["ancestor ellipsis", "at the tail"], ["picker floor", <><span className="tok">--tasty-fp-popup-min-width</span> 320 — the owning surface always wins</>]]}
            tokens={[{ tok: "--tasty-fp-crumb-max-width", use: "cap (approved, unchanged)" }, { tok: "--tasty-fp-crumb-min-width", use: "ancestor floor" }, { tok: "--tasty-fp-crumb-current-min-width", use: "current-folder floor" }, { tok: "--tasty-fp-bar-hysteresis", use: "grow-back margin" }, { tok: "--tasty-fp-popup-min-width", use: "popup floor" }]} />
          <Do><b>Do</b> keep the <b>… menu</b> in place through every step: hidden ancestors enter it in <b>path order</b>, so step 4 and 5 just prepend the parent and the root. Its hit area never exceeds its painted box.</Do>
          <Dont><b>Don't</b> buy width from the footer, the file-name input, the Refresh button, or the font size. The path bar takes what is left over after those, and folds.</Dont>
          <Note>Same table at <b>ui_scale 0.85 / 1 / 1.2</b>: floors and caps are width tokens, so they scale with everything else and the order is unchanged. Windows drives, UNC roots and remote <code>user@host</code> roots are ordinary root crumbs — long ones fold at step 5, they get no exemption.</Note>
        </Spec>
        <Spec id="file-type-filter-chip-a-read-only-readout" title="File-type filter chip — a read-only readout"
          when={<>The chip shows the filter <b>the caller</b> passed and nothing else: there is no menu and no way to change it, so it is drawn as a <b>readout</b>, not a control — no chevron, no fill, no hover, not focusable. Text is the extension list in mono (<code>*.toml, *.json</code>), lowercase, in the caller's order. <b>No filter → no chip</b>; the name field takes the width. Width is the content, capped at <span className="tok">--tasty-fp-filter-max-width</span>; past that the list ends in an ellipsis and the tooltip ("Showing …") carries all of it. Both consumers (Tools menu picker, the picker inside Settings) and both modes show the same chip. In save mode the filter does <b>not</b> touch the name field — no extension is appended.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>no filter — chip hidden</div><FilePickerFrame w={480} h={300} /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>one extension</div><FilePickerFrame w={480} h={300} filters={["toml"]} /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>two extensions — save mode</div><FilePickerFrame w={480} h={300} mode="save" filters={["toml", "json"]} /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>many — capped at 160, ellipsis + tooltip</div><FilePickerFrame w={480} h={300} filters={["png", "jpg", "jpeg", "gif", "webp", "svg"]} /></div>
          </Stage>
          <Meta
            specs={[["role", "read-only readout — not a button, no menu, not in tab order"], ["label", "*.ext list, mono caption, caller order, joined by \", \""], ["no filter", "chip absent"], ["width", "content, ≤ fp-filter-max-width (160) · then end ellipsis"], ["tooltip", "Showing *.png, *.jpg, … (full list)"], ["height", "fp-filter-height = the Input beside it"], ["box", "1px separator · radius · no fill · pad-x space-sm"], ["save mode", "no extension auto-append"], ["i18n", "only the tooltip prefix is translated; the list is literal"]]}
            tokens={[{ tok: "--tasty-fp-filter-height", use: "28 — matches the Input" }, { tok: "--tasty-fp-filter-max-width", use: "160 cap (new)" }, { tok: "--tasty-separator", use: "chip edge", color: "var(--tasty-separator)" }, { tok: "--tasty-text-muted", use: "list", color: "var(--tasty-text-muted)" }]} />
          <Dont><b>Don't</b> draw it with a chevron or a hover fill while nothing opens — the earlier static "All files ▾" chip promised a menu that does not exist.</Dont>
        </Spec>

        <Spec id="folder-mode-mode-folder-returns-a-folder" title={"Folder mode — mode=\"folder\" returns a folder"}
          when={<>The third mode of the same frame, for the three callers that used the OS folder dialog (Settings › Remote transfer › Save folder, Plugins › Add › Find folder…, Preset editor <code>dir</code>). Local only. The <b>target</b> is the selected folder row, or the <b>current folder</b> when nothing is selected, so <b>Select folder</b> is enabled whenever the folder reads (also when it is empty) and disabled while loading or on an error. The footer swaps the File name input for a <b>read-only Folder readout</b> of the full target path (front ellipsis, tooltip = full path) and one muted hint line. <b>Files stay listed</b> so the folder is recognisable. They are not selectable but they are read: <span className="tok">--tasty-fp-folder-file-fg</span> (text-muted), no hover, skipped by ↑↓, tooltip "Only folders can be selected." The filter chip is hidden: filters don't apply to folders. Not the same thing as <code>folderSel</code> above, which is a folder row in open / save mode.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>no row selected — target = current folder</div><FilePickerFrame mode="folder" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>subfolder selected — target = configs</div><FilePickerFrame mode="folder" folderPick="row" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>pointer on a file — inert, tooltip only</div><FilePickerFrame mode="folder" folderPick="row" fileHover /></div>
          </Stage>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Settings chooser size 480×360 · empty folder — still selectable</div><FilePickerFrame mode="folder" state="empty" w={480} h={360} /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>480×360 · read error — Select folder disabled, path in disabled ink</div><FilePickerFrame mode="folder" state="error-perm" w={480} h={360} /></div>
            <div data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: 6, padding: 8, background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Latte · 480×360 · subfolder selected</div><FilePickerFrame mode="folder" folderPick="row" w={480} h={360} /></div>
          </Stage>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>en · 640 · long folder names</div><FilePickerFrame mode="folder" pathKind="longtwo" folderPick="row" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>ko · 480×360 · long names</div><FilePickerFrame mode="folder" lang="ko" pathKind="longtwo" w={480} h={360} /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>ja · 320 floor · long names, selected</div><FilePickerFrame mode="folder" lang="ja" pathKind="longtwo" folderPick="row" single w={320} h={420} /></div>
          </Stage>
          <Meta
            specs={[["target", "selected folder row, else current folder — one path returned, picker closes"], ["header", "folder glyph + “Select folder” (14 semibold) — same header box"], ["footer row 1", "label fp-footer-label-width 64 (caption, ellipsis) · gap fp-section-gap 8 · readout flex 1, control-height 28, pad-x space-md, input-readonly-bg / -border, mono caption, fp-folder-path-fg, FRONT ellipsis, tooltip full path, not editable"],
              ["footer row 2", "hint: folder glyph icon-size-sm + micro, fp-folder-hint-fg, wraps; hidden on loading / error"], ["footer row 3", "spacer · Cancel ghost · Select folder primary, gap space-sm; never shrinks — rows 1–2 give way"],
              ["Select folder", "enabled: loaded or empty · disabled: loading, error (path readout in state-disabled-fg)"], ["file rows", "not selectable, read for orientation: glyph + name + size + date in fp-folder-file-fg (text-muted, 4.5), no hover / selection overlay, ↑↓ skip, tooltip"], ["filter chip", "hidden — the caller's filter is ignored in this mode"],
              ["single click folder", "selects (target = that folder)"], ["click selected row again / empty area", "clears → target = current folder"], ["double click folder · →", "enters it (selection cleared)"], ["Enter", "Select folder — confirms the target"], ["Up · Alt+↑ · breadcrumb", "unchanged"]]}
            tokens={[{ tok: "--tasty-fp-folder-file-fg", use: "→ text-muted", color: "var(--tasty-fp-folder-file-fg)" }, { tok: "--tasty-fp-folder-path-fg", use: "new · → input-readonly-fg", color: "var(--tasty-fp-folder-path-fg)" }, { tok: "--tasty-fp-folder-hint-fg", use: "new · → text-muted", color: "var(--tasty-fp-folder-hint-fg)" }, { tok: "--tasty-input-readonly-bg", use: "readout box", color: "var(--tasty-input-readonly-bg)" }, { tok: "--tasty-fp-footer-label-width", use: "64 label column" }]} />
          <Note>Strings (<code>picker.folder.*</code>) — <b>title / confirm</b>: Select folder · 폴더 선택 · フォルダーを選択 — <b>label</b>: Folder · 폴더 · フォルダー — <b>hint.current</b>: Using the current folder. Select a subfolder to use it instead. · 현재 폴더를 사용합니다. 하위 폴더를 고르면 그 폴더를 사용합니다. · 現在のフォルダーを使用します。サブフォルダーを選ぶとそのフォルダーを使用します。 — <b>hint.selected</b>: Double-click {"{name}"} to open it. · {"{name}"} 폴더를 열려면 더블클릭하세요. · {"{name}"} を開くにはダブルクリックします。 — <b>file.tooltip</b>: Only folders can be selected. · 폴더만 선택할 수 있습니다. · 選択できるのはフォルダーだけです。 — <b>empty</b>: No subfolders here · 하위 폴더가 없습니다 · サブフォルダーはありません. Width check: the longest label (フォルダー) fits 64 at caption; the longest button (フォルダーを選択) fits beside Cancel at the 320 floor; hints wrap to two lines at 320, never clip.</Note>
          <Dont><b>Don't</b> make Enter descend in this mode: Enter presses the primary button in every mode (in open mode Open happens to descend into a selected folder); here the primary button is Select folder, so Enter confirms. Descending is double-click or →.</Dont>
        </Spec>
      </Section>
      {<PresetEditor_Section />}

<Section id="settings" title="Settings window">
        <Spec id="settings-two-tier-navigation" title="Settings — two-tier navigation"
          when={<>The largest dialog. <b>L1</b> = a small, fixed set of top tabs (General / Appearance / Keybindings / Plugins). <b>L2</b> = a growable, filterable left sidebar of sections (plugins contribute here). This split keeps top-level navigation stable while the content tree scales. Reuse it for any settings-like surface.</>}>
          <Stage variant="solo center"><Backdrop height={420}><SettingsFrame /></Backdrop></Stage>
          <Meta
            specs={[["frame", "1100 × 700 (canonical)"], ["L1 tabs", "44px bar, accent underline"], ["L2 sidebar", "200px, filter + list"], ["footer", "Cancel / Save, right"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "L1 + L2", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-bg-panel", use: "content", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-accent-primary", use: "active tab", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-active", use: "active section", color: "var(--tasty-surface-active)" }]} />
          <Note>L1 stays small forever; growth happens only in L2. Plugins are their <b>own</b> L1 tab — never crammed into another group's sidebar.</Note>
        </Spec>

        <Spec id="general-general-row-grid-row-captions-webhook-row" title="General › General — row grid, row captions, webhook row"
          when={<>Every settings subtab uses one <b>label column · control</b> grid. <b>Label column</b> = the subtab's longest label, clamped to <span className="tok">--tasty-settings-label-width</span> (150) … <span className="tok">--tasty-settings-label-max-width</span> (240); a label longer than 240 wraps. <b>Gap</b> 16 (<span className="tok">--tasty-settings-label-gap</span>). The app's own fit-to-longest measure is canonical with this clamp; the old 12 gap moves to 16. A description that belongs to <b>one row</b> sits directly under that row (<span className="tok">--tasty-settings-row-caption-gap</span> 4), at the row's left edge, measure-md: that is where the wheel-scroll and language-restart lines go. The webhook row keeps its always-shown warning callout the same way. Terminal › TUI's OSC 52 row follows the same rule (on the grid, callout measure-md).</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-panel)" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 14, width: 620 }}>
              {[["Restore layout:", <WSwitch key="s" defaultChecked />], ["Close behavior:", <WSelect key="c" options={["Ask", "Minimize to background", "Quit"]} style={{ width: "var(--tasty-field-width-lg)" }} />],
                ["Wheel scroll distance:", <><WInput key="w" mono defaultValue="3" style={{ width: "var(--tasty-field-width-xs)" }} /><span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>lines</span></>, "Lines scrolled per wheel notch in the terminal. Trackpads scroll by distance and ignore this."],
                ["Language:", <WSelect key="l" options={["English", "한국어", "日本語"]} style={{ width: "var(--tasty-field-width-md)" }} />, "Changing the language takes effect after Tasty restarts."],
                ["Accept webhook calls from other computers:", <WSwitch key="h" />, null, true]].map(([label, ctl, cap, warn]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)" }}>
                    <span style={{ width: "var(--tasty-settings-label-max-width)", flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>{label}</span>{ctl}
                  </div>
                  {cap && <p style={{ margin: 0, fontSize: 12, color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>{cap}</p>}
                  {warn && <div style={{ display: "flex", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderRadius: "var(--tasty-radius)", maxWidth: "var(--tasty-measure-md)",
                    border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)", background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>
                    <span style={{ display: "inline-flex", flex: "none", marginTop: 1, color: "var(--tasty-accent-warning-raised)" }}><WIcon name="alertTriangle" size={16} /></span>
                    <p style={{ margin: 0, fontSize: 12, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>When on, the webhook listener takes every network interface, so anyone who can reach its port can call your registered webhooks. When off, only programs on this computer can. Applies from the next start.</p>
                  </div>}
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["label column", "longest label of the subtab, clamp 150 … 240, wraps past 240 (en: webhook label → 240, 2 lines)"], ["gap", "16 label → control"], ["row caption", "directly under its row · gap 4 · left edge · measure-md · caption 12 muted"], ["callout", "same slot as a caption · warning · alertTriangle 16 · always visible · measure-md"], ["placement", "webhook row last (after Language)"], ["setting", "[webhook] allow_external · default off · Save / Cancel · from next start"]]}
            tokens={[{ tok: "--tasty-settings-label-width", use: "label column floor 150" }, { tok: "--tasty-settings-label-max-width", use: "label column cap 240" }, { tok: "--tasty-settings-label-gap", use: "16" }, { tok: "--tasty-settings-row-caption-gap", use: "row → caption 4" }, { tok: "--tasty-accent-warning", use: "callout edge + icon", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-measure-md", use: "caption / callout width" }]} />
          <Note>Copy is confirmed as shipped: label “Accept webhook calls from other computers:”, callout as above (<code>settings.general.webhook_allow_external_*</code>). Wheel / language caption copy here is placeholder for the app's existing strings — keep the app's text, move only the position.</Note>
        </Spec>

        <Spec id="keybindings-plugins-plugin-picker-per-command-mode" title="Keybindings › Plugins — plugin picker + per-command mode" badges={<span className="ic">interactive</span>}
          when={<>Pick a plugin, then edit each of its commands on <b>one control line</b>: <b>mode</b> Select (Inherit / Custom / None) · <b>slot</b> (inherit-source Select, key Input, or “(Unassigned)”) · <b>Reset</b>. Rides the settings Row grid (title column 150 + gap 16, so the picker and every mode Select start at the same x), not the 288 Import/Export column: 288 + the line overflows the 620 cap. <b>Every control on a line is 28</b> (md) — Select, Input and the ghost Reset alike. Titles wrap inside the column, never ellipsise. Inherit adds a caption under the line with the resolved key; a Custom value that fails to parse turns the Input invalid and adds an error caption. An unsaved change shows a 6px accent dot after the title. Reset is <b>disabled</b> when there is no override. Save / Cancel stay in the window footer.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-panel)", gap: "var(--tasty-space-xl)", flexDirection: "column", alignItems: "flex-start" }}>
            {Kit_KbPluginsSubtab && <>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 620 }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>default — row 1 Custom (record slot, b12) · row 2 Inherit + caption · row 3 overridden (draft = saved, Reset enabled)</span>
                <KbPluginsSubtab />
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 620 }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>draft + invalid — row 1 holds unparsable config text (danger slot + error) · row 2 switched to None (dot)</span>
                <KbPluginsSubtab saved={{}} seedDrafts={{ "clipboard-viewer/open": { mode: "custom", keys: "ctrl+shft+h" }, "clipboard-viewer/paste-plain": { mode: "none" } }} />
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 620 }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>empty</span>
                <KbPluginsSubtab plugins={[]} />
              </div>
            </>}
          </Stage>
          <Meta
            specs={[["picker", "\"Plugin:\" in the 150 title column · Select 200 · plugins by name"], ["command", "padding-y 4 · 1px separator between commands, none after the last"], ["line", "min-h 32 · mode 160 · gap 8 · slot 200 · gap 8 · Reset (ghost md)"], ["height", "28 for every control on the line"], ["caption", "gap 4 · caption 12 · Inherit: muted \"Inherited (Ctrl+C)\" / \"None\" · parse error: danger"], ["draft", "6px accent-primary dot after the title"], ["Reset", "disabled when no override · tooltip \"Clear the override and use the manifest default.\""], ["list gap", "12 picker → first command"]]}
            tokens={[{ tok: "--tasty-kb-plugin-title-width", use: "150 title column" }, { tok: "--tasty-kb-plugin-mode-width", use: "160" }, { tok: "--tasty-kb-plugin-slot-width", use: "200" }, { tok: "--tasty-kb-plugin-picker-width", use: "200" }, { tok: "--tasty-kb-plugin-control-height", use: "28" }, { tok: "--tasty-kb-plugin-draft-dot", use: "draft", color: "var(--tasty-kb-plugin-draft-dot)" }, { tok: "--tasty-kb-plugin-error-fg", use: "parse error", color: "var(--tasty-kb-plugin-error-fg)" }, { tok: "--tasty-kb-plugin-separator", use: "between commands", color: "var(--tasty-kb-plugin-separator)" }]} />
          <Note>Behaviour unchanged from the app: switching to Custom fills the previous Custom value or the manifest key; switching to Inherit fills the manifest source or the first of the four; typing writes the draft at once (commas = several keys, spaces ignored); Reset clears the override in the draft. The record-button alternative is a behaviour change and is shown for the user's decision only.</Note>
        </Spec>
        {<OverlaysWindowsB12_KbPluginsRecordSpec />}

        <Spec id="general-overlay-toast-duration" title="General › Overlay — toast duration"
          when={<>The <b>General</b> L1 tab gains a fourth L2 section, <b>Overlay</b> (after General / Notifications / Accessibility) — the umbrella term for Toast / Banner / Modifier-hint / Marker overlays. It ships with <b>one row</b>: <b>Toast duration</b>, a mono number field (the <b>Numbers in settings</b> shape — DragValue retired 2026-10-07) that controls how long a toast stays before auto-dismissing (today hardcoded at 2000ms). Exposed in <b>seconds</b> (matches the user's mental model), stored as ms. Same Grid (label + control) and hint-text pattern as the other General sections — no new interaction invented.</>}>
          <Stage variant="solo center" style={{ gap: 24, flexWrap: "wrap" }}>
            <Backdrop height={420}><SettingsGeneralOverlayFrame /></Backdrop>
            <div style={{ display: "flex", flexDirection: "column", gap: 10, alignSelf: "center" }}>
              {[["rest", "rest"], ["editing", "editing — focus border"], ["invalid", "out of range — commits as 10.0"]].map(([s, l]) => (
                <div key={s} style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <ToastDragValue state={s} />
                  <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{l}</span>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["L2 position", "4th — after Accessibility"], ["label", "Toast duration"], ["unit", "seconds — mono “2.0 s”"], ["range / step", "1.0–10.0 s, step 0.5"], ["default", "2.0 s (= DEFAULT_LIFETIME 2000ms)"], ["control", "mono Input (90) + static “s” — the Numbers in settings shape"], ["commit", "blur / ↵ · clamps to 1.0–10.0 and snaps to 0.5"], ["out of range", "danger border + “Between 1.0 and 10.0.” line"], ["hint", "12px muted line below the grid"]]}
            tokens={[{ tok: "--tasty-surface-active", use: "active L2 row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-field-width-xs", use: "field width 90" }, { tok: "--tasty-border-focus", use: "editing border", color: "var(--tasty-border-focus)" }, { tok: "--tasty-accent-danger", use: "out of range", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-font-mono", use: "value text" }]} />
          <Note>Scope is <b>this one row only</b> — the tab name is the umbrella so future overlay settings (banner, marker…) can land here without inventing a new section. Existing General / Notifications / Accessibility content is untouched. Store the value in ms internally; only the display is in seconds.</Note>
        </Spec>
        <Spec id="settings-general-remote-transfer-5th-l2-subtab" title="Settings · General › Remote transfer — 5th L2 subtab"
          when={<>The <b>General</b> L1 tab gains a fifth L2 section, <b>Remote transfer</b> (after Overlay) — the umbrella for the remote (mirror) file-transfer channel, following the Overlay-subtab precedent: a topical L2 with room to grow rather than rows crammed into General. Two rows edit <code>RemoteTransferSettings</code>: <b>Save folder</b> — a mono path Input + a <b>Browse…</b> secondary button (opens the native folder picker, same pairing as the Scripts file row) — and <b>Maximum size</b> — a numeric Input with a mono <b>MiB</b> unit suffix. Each row keeps the settings-row grid (150px label · control), a muted description line beneath, and a separator between rows.</>}>
          <Stage variant="solo center"><Backdrop height={420}><SettingsRemoteTransferFrame /></Backdrop></Stage>
          <Meta
            specs={[["L2 position", "5th — after Overlay"], ["rows", "Save folder · Maximum size"], ["folder row", "mono path Input + Browse… (secondary, folder icon)"], ["size row", "numeric Input · 88px + mono “MiB” suffix"], ["defaults", "~/.tasty/transfers/ · 500 MiB"], ["row grid", "150px label · control · desc below"]]}
            tokens={[{ tok: "--tasty-settings-row-min-height", use: "row height" }, { tok: "--tasty-surface-active", use: "active L2 row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-separator", use: "row separator", color: "var(--tasty-separator)" }, { tok: "--tasty-text-muted", use: "descriptions + unit", color: "var(--tasty-text-muted)" }]} />
          <Note>The unit is a static mono <b>MiB</b> suffix outside the field — not typed, not a Tag — mirroring how Toast duration carries its “s” unit. Exceeding <b>Maximum size</b> rejects new transfers before they start; the rejection surfaces as the <b>Transfer failed</b> popup (Overlays › Dialogs › Remote transfer).</Note>
        </Spec>
        <Spec id="settings-misc-task-pipeline-report-limits" title="Settings · Misc › Task pipeline — Report limits"
          when={<>A new Misc L2 subtab <b>Task pipeline</b> (after Scripts, before Tastyrc on Windows). The content column is one <b>mono micro uppercase</b> heading, <b>REPORT LIMITS</b>, then two settings rows in the row grid (label column = longest label clamped 150 … 240, gap 16), each a <b>mono numeric Input</b> (90) with the static unit <b>B</b>, its description caption directly under it (gap 4, measure-md), and a 1px separator between the rows. The two values are tied (note limit &lt; attempt limit): each field's range narrows to the other's current value, so a crossed pair can't be committed. The relation stays in the description copy; the range line appears <b>only when a value is out of range</b>, like every numeric field. Values are raw bytes with no grouping (they are typed, and match the config file); the unit stays <b>B</b>.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha", false], ["Latte · out of range", "latte", true]].map(([label, th, bad]) => (
              <div key={label} data-theme={th || undefined} style={{ width: 460, display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-gap)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Report limits</div>
                {[["Note size limit", bad ? "70000" : "1024", "One note longer than this is cut at a UTF-8 boundary and marked as truncated. Must be smaller than the attempt limit.", bad ? "Between 64 and 16383. Commits as 16383." : null],
                  ["Attempt report limit", "16384", "Total note text kept for one task attempt. Notes past it are not stored, only counted.", null]].map(([lab, val, desc, err], i) => (
                  <React.Fragment key={lab}>
                    {i > 0 && <div style={{ height: 1, background: "var(--tasty-separator)" }} />}
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)" }}>
                      <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-settings-label-width) auto auto", columnGap: "var(--tasty-settings-label-gap)", alignItems: "center", justifyContent: "start", minHeight: "var(--tasty-settings-row-min-height)" }}>
                        <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>{lab}</span>
                        <span style={{ width: "var(--tasty-field-width-xs)" }}><Input block defaultValue={val} aria-invalid={err ? true : undefined} style={{ fontFamily: "var(--tasty-font-mono)", textAlign: "right", ...(err ? { borderColor: "var(--tasty-accent-danger)" } : null) }} /></span>
                        <span style={{ marginLeft: "calc(var(--tasty-space-sm) - var(--tasty-settings-label-gap))", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>B</span>
                      </div>
                      {err && <div style={{ marginLeft: "calc(var(--tasty-settings-label-width) + var(--tasty-settings-label-gap))", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-accent-danger-raised)" }}>{err}</div>}
                      <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-md)", lineHeight: "var(--tasty-line-height-ui)" }}>{desc}</p>
                    </div>
                  </React.Fragment>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["L2 position", "Misc · after Scripts · before Tastyrc (Windows)"], ["heading", "REPORT LIMITS · mono micro uppercase · muted"], ["row grid", "label [150 … 240] · gap 16 (settings-label-gap — the request's 12 is superseded)"], ["field", "mono Input · field-width-xs 90 · right-aligned · unit B (static, muted, 8 after the field)"], ["caption", "under its row · gap 4 · caption 11 · muted · measure-md"], ["between rows", "1px separator · settings-row-gap 12"], ["ranges", "note 64 … min(65536, attempt − 1) · attempt max(128, note + 1) … 131072"], ["range line", "only when out of range (danger) · clamp on commit · directly UNDER THE INPUT, in the control column (label width + label gap) — b10, every numeric field"], ["numbers", "raw bytes, no grouping, no KiB"]]}
            tokens={[{ tok: "--tasty-settings-row-gap", use: "→ space-md 12 (2026-10-09)" }, { tok: "--tasty-settings-row-caption-gap", use: "row → caption 4" }, { tok: "--tasty-field-width-xs", use: "90" }, { tok: "--tasty-accent-danger", use: "out of range", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note>i18n (given): en “Note size limit” / “Attempt report limit” · ko “기록 한 건 상한” / “회차 report 상한” · ja “記録 1 件の上限” / “試行 report 上限” · heading “Report limits”. Gallery spec id: <code>settings-task-pipeline</code>.</Note>
        </Spec>
        <Spec id="numbers-in-settings-one-shape-mono-input-a-static-suffix" title="Numbers in settings — one shape: mono Input + a static suffix"
          when={<>Three different numeric controls had appeared: a mono text Input with a static unit (remote transfer <b>Maximum size</b> · MiB), a <b>drag</b> number (plugin <b>Default zoom</b> · %), and a proposed stepper. The settled shape is the <b>first</b>, everywhere: a <b>mono text Input</b>, keyboard entry only, a <b>static muted suffix</b> outside the field, and <b>clamp on commit</b> (blur / <span className="ic">↵</span>) — not while typing, so you can type <code>150</code> in a 25–200 field without the second keystroke fighting you. Out of range shows the danger border + one inline line naming the range. A drag surface inside a scrolling settings pane steals the scroll and hides its own range; a stepper needs two more hit targets for a field people set once.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 16, alignItems: "flex-start" }}>
            {[["default", "150", false], ["out of range — clamp on commit", "420", true], ["disabled", "100", false, true]].map(([label, val, bad, dis]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6, width: 420 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span style={{ flex: 1, fontSize: 13, color: dis ? "var(--tasty-text-disabled)" : "var(--tasty-text-secondary)" }} aria-disabled={dis || undefined}>Default zoom</span>
                  <span style={{ flex: "none", width: "var(--tasty-field-width-xs)" }}>
                    <Input block defaultValue={val} disabled={dis} aria-invalid={bad || undefined}
                      style={{ fontFamily: "var(--tasty-font-mono)", textAlign: "right", ...(bad ? { borderColor: "var(--tasty-accent-danger)" } : null) }} />
                  </span>
                  <span style={{ flex: "none", width: 28, fontSize: 12, color: "var(--tasty-text-muted)" }}>%</span>
                </div>
                {bad && <div style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)", alignSelf: "flex-end", width: "calc(var(--tasty-field-width-xs) + 38px)" }}>Between 25 and 200. Commits as 200.</div>}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["control", "existing Input — no new component"], ["font", <span className="tok">--tasty-font-mono</span>], ["align", "right — digits line up down a settings column"], ["width", <>90 (<span className="tok">--tasty-field-width-xs</span>) — <b>88 is dropped</b></>], ["suffix", "static text outside the field, muted"], ["clamp", "on commit (blur / ↵), never mid-typing"], ["out of range", "danger border + one inline line with the range, under the input (control column), above the description caption (b10)"], ["scope", "every numeric field in Settings and in plugin settings"]]}
            tokens={[{ tok: "--tasty-field-width-xs", use: "numeric field width", }, { tok: "--tasty-accent-danger", use: "out-of-range edge + line", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-muted", use: "unit suffix", color: "var(--tasty-text-muted)" }]} />
          <Note><b>Maximum size keeps 90.</b> The 88 in the earlier mock was a drawing accident, not a role — the field-width set stays 90 / 110 / 160 / 180 / 200 with no new member. Two-pixel gain, one more name to maintain: not worth it.</Note>
          <Dont><b>Don't</b> put a drag-to-change number inside a scrollable settings pane. The gesture collides with the scroll and the range stays invisible until you overshoot it.</Dont>
        </Spec>

        <Spec id="settings-control-metrics-shortcut-rows-drag-flip-modifier-font-c" title="Settings · control metrics — shortcut rows · drag flip modifier · font combo · open Select"
          when={<>Values that were literals in the app become tokens. <b>Shortcut rows</b> (Keybindings General … Scripts): the record button is the Import / Export slot size — <span className="tok">--tasty-kb-record-width</span> 140 × <span className="tok">--tasty-kb-record-height</span> 24, mono, surface-raised — and the add button is <span className="tok">--tasty-kb-record-add-width</span> 32 × 24; buttons in a row sit 4 apart; rows sit <span className="tok">--tasty-kb-row-gap</span> <b>8</b> apart with no divider (a long, scanned list — denser than the 12 of settings rows). The <b>Explorer drag flip modifier</b> is the <b>last row of Keybindings › General</b>, alone after the shortcut rows behind space-sm · separator · space-xs (b11 — the app's placement; the category switch modifier stays in Workspace and carries <b>no caption</b>, nor do Tab / Workspace switch modifiers). Shape: label + modifier Select (field-width-md) + caption. A combination from config.toml that is not in the list is appended at the end of the list as written. Record button, add and None slot share <span className="tok">--tasty-kb-record-border</span> → <span className="tok">--tasty-kb-record-border-hover</span> on hover (fill unchanged; Mocha “Ctrl+W” shows hover). A row with <b>no binding</b> shows one <b>None</b> slot (record width, border only, muted) that starts recording on click — no + beside it. The font search combo is the shared <b>SearchableSelect</b> (2026-10-19 — Components › Form controls <code>#searchable-select</code>); the Tab / Workspace / Category switch modifier combos are a plain <b>Select</b>. The <b>font search combo</b> list caps at <span className="tok">--tasty-font-combo-list-max-height</span> 300 and its search field is the list width minus <span className="tok">--tasty-font-combo-search-inset</span> on each side. An <b>open Select</b> trigger takes the existing <span className="tok">--tasty-select-border-focus</span> (→ border-focus), as in the components “no value” panel. Appearance › General keeps the app's order: font rows, the preview right after them (space-lg before, caption heading “Preview”), then a separator and Ligatures / Opacity.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} data-theme={th || undefined} style={{ width: 480, display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · Keybindings › General</div>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-kb-row-gap)" }}>
                  {[["New tab", ["Ctrl+T"]], ["Close tab", ["Ctrl+W", "Ctrl+F4"], 1], ["Split right", []]].map(([act, keys, hov]) => (
                    <div key={act} style={{ display: "grid", gridTemplateColumns: "var(--tasty-settings-label-width) 1fr", columnGap: "var(--tasty-settings-label-gap)", alignItems: "center" }}>
                      <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>{act}</span>
                      <span style={{ display: "flex", gap: "var(--tasty-space-xs)", flexWrap: "wrap" }}>
                        {keys.length === 0 && <span style={{ width: "var(--tasty-kb-record-width)", height: "var(--tasty-kb-record-height)", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box", border: "var(--tasty-border-width) solid var(--tasty-kb-record-border)", borderRadius: "var(--tasty-radius)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-kb-record-empty-fg)" }}>None</span>}
                        {keys.map((k, ki) => <span key={k} style={{ width: "var(--tasty-kb-record-width)", height: "var(--tasty-kb-record-height)", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box", background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid " + (hov && ki === 0 && !th ? "var(--tasty-kb-record-border-hover)" : "var(--tasty-kb-record-border)"), borderRadius: "var(--tasty-radius)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)" }}>{k}</span>)}
                        {keys.length > 0 && <span style={{ width: "var(--tasty-kb-record-add-width)", height: "var(--tasty-kb-record-height)", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box", border: "var(--tasty-border-width) solid var(--tasty-kb-record-border)", borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-muted)" }}><WIcon name="plus" size="var(--tasty-icon-size-sm)" /></span>}
                      </span>
                    </div>
                  ))}
                </div>
                <div style={{ display: "flex", flexDirection: "column", paddingTop: "var(--tasty-space-sm)", paddingBottom: "var(--tasty-space-xs)" }}><div style={{ height: "var(--tasty-border-width)", background: "var(--tasty-separator)" }} /></div>
                {[["Explorer drag flip modifier:", th ? "Option" : "Ctrl", "Hold while dragging in the Explorer to copy instead of move, or move instead of copy."]].map(([lab, val, cap]) => (
                  <div key={lab} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)" }}>
                    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-settings-label-width) auto", columnGap: "var(--tasty-settings-label-gap)", alignItems: "center", justifyContent: "start", minHeight: "var(--tasty-settings-row-min-height)" }}>
                      <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>{lab}</span>
                      <span style={{ width: "var(--tasty-field-width-md)", height: "var(--tasty-control-height)", display: "flex", alignItems: "center", justifyContent: "space-between", boxSizing: "border-box", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-select-bg)", border: "var(--tasty-border-width) solid " + (lab.startsWith("Explorer") && !th ? "var(--tasty-select-border-focus)" : "var(--tasty-select-border)"), borderRadius: "var(--tasty-radius)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>{val}<WIcon name="chevronDown" size="var(--tasty-icon-size-sm)" /></span>
                    </div>
                    <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-md)" }}>{cap}</p>
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["record button", "kb-record-width 140 × kb-record-height 24 · mono caption · surface-raised · 1px border-default"], ["add (+)", "kb-record-add-width 32 × 24 · border only (kb-record-border) · plus icon sm in text-muted (b11 — replaces the filled “+” glyph)"], ["hover (b11)", "record · add · None: border → kb-record-border-hover (border-strong) · fill unchanged"], ["no binding (b11)", "one None slot · kb-record-width × 24 · border only · kb-record-empty-fg · click = record the first binding · no +"], ["in-row gap", "space-xs 4"], ["row gap", "kb-row-gap 8 · no divider (kit KeyRow divider + paddingBottom 6 retired)"], ["drag flip row", "Keybindings › General · LAST row, alone · space-sm · separator · space-xs above (b11) · Select field-width-md · config combos not in the list appended at the end · options macOS Option / Command / Control / Shift (default Option), others Ctrl / Alt / Shift (default Ctrl)"], ["font combo", "2026-10-19: = SearchableSelect (components #searchable-select) · trigger field-width-lg 200 · popup max 300 · search field = popup − 2 × 4 · font-combo-* alias select-search-* · modifier combos = plain Select"], ["captions (b11)", "Category / Tab / Workspace switch modifier rows: no caption · drag flip keeps its caption"], ["open Select", "trigger border select-border-open → border-focus (Mocha sample shows it open)"], ["Appearance › General", "font rows → space-lg → caption “Preview” → preview (content width) → separator → Ligatures · Opacity (app order confirmed)"]]}
            tokens={[{ tok: "--tasty-kb-record-width", use: "→ kb-ie-slot-min-width 140" }, { tok: "--tasty-kb-record-height", use: "→ kb-ie-slot-height 24" }, { tok: "--tasty-kb-record-add-width", use: "→ size-32" }, { tok: "--tasty-kb-record-border", use: "→ border-default (b11)", color: "var(--tasty-kb-record-border)" }, { tok: "--tasty-kb-record-border-hover", use: "→ border-strong (b11)", color: "var(--tasty-kb-record-border-hover)" }, { tok: "--tasty-kb-record-empty-fg", use: "→ text-muted (b11)", color: "var(--tasty-kb-record-empty-fg)" }, { tok: "--tasty-kb-row-gap", use: "→ space-sm 8" }, { tok: "--tasty-font-combo-list-max-height", use: "→ size-300" }, { tok: "--tasty-font-combo-search-inset", use: "→ space-xs 4" }, { tok: "--tasty-select-border-focus", use: "existing → border-focus · now also the OPEN state", color: "var(--tasty-select-border-focus)" }]} />
          <Note>i18n: <code>settings.keybindings.explorer_drag_flip_modifier_label</code> “Explorer drag flip modifier:” / “탐색기 드래그 전환 키:” / “エクスプローラーのドラッグ切替キー:” · caption <code>…_hint</code> “Hold while dragging in the Explorer to copy instead of move, or move instead of copy.” / “탐색기에서 끌 때 누르고 있으면 이동 대신 복사, 복사 대신 이동합니다.” / “エクスプローラーでドラッグ中に押すと、移動とコピーが入れ替わります。” The same label goes in the import table and the option-migration card. Gallery id: <code>settings-control-metrics</code>.</Note>
        </Spec>

        <Spec id="hook-handlers-origin-decides-whether-a-row-can-be-removed" title="Hook Handlers — origin decides whether a row can be removed"
          when={<>The registry <b>re-seeds host and plugin defaults on every start</b>, so a remove button on those rows promises something the system undoes. Rows now carry their <b>origin</b> as a Tag — <b>host</b> / <b>you</b> / the <b>plugin</b> in mauve — and only <b>user</b> rows get the trash affordance. Host and plugin rows show a <b>lock glyph</b> in the same slot (not a disabled button: nothing is pending) with the tooltip “Provided by host — can't be removed”. <b>IpcSequence</b> stays a <b>one-line mono summary</b> with an <b>Edit</b> button that opens the sequence editor; editing a multi-step sequence inline inside a settings row has nowhere to put the steps.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: 520, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--tasty-separator)", fontSize: 11, letterSpacing: ".06em", textTransform: "uppercase", color: "var(--tasty-text-secondary)" }}>Hook handlers</div>
              {[{ ev: "on_open", act: "open_markdown_preview", origin: "host" },
                { ev: "on_open", act: "run: code -g {path}:{line}", origin: "you" },
                { ev: "on_paste", act: "imgview.stash", origin: "dev.imgview" },
                { ev: "on_exit", act: "ipc: focus → save → close", origin: "you", seq: true },
                { ev: "on_webhook", act: "ipc: system.info → … (2 steps)", origin: "you", seq: true, cli: true }].map((r, i) => {
                const user = r.origin === "you";
                const plugin = r.origin !== "host" && !user;
                return (
                  <div key={i} style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 12px", borderBottom: "1px solid var(--tasty-separator)" }}>
                    <span style={{ flex: "none", width: 88, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-secondary)" }}>{r.ev}</span>
                    <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.act}</span>
                    <span style={{ flex: "none", color: plugin ? "var(--tasty-accent-agent-raised)" : undefined }}><WTag>{r.origin}</WTag></span>
                    {r.seq && !r.cli && <Button variant="ghost" size="sm">Edit</Button>}
                    {r.cli && <span title="tasty hook-handler get --id ci-notify" style={{ flex: "none", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Edit with CLI</span>}{r.cli && <IconButton size="sm" aria-label="Copy edit command"><WIcon name="copy" size={13} /></IconButton>}
                    <span style={{ flex: "none", width: 24, display: "inline-flex", justifyContent: "center" }}>
                      {user
                        ? <IconButton size="sm" aria-label="Remove"><WIcon name="trash" size={13} /></IconButton>
                        : <span title="Provided by host — can't be removed" style={{ display: "inline-flex", color: "var(--tasty-glyph-dim)" }}><WIcon name="lock" size={13} /></span>}
                    </span>
                  </div>
                );
              })}
            </div>
          </Stage>
          <Meta
            specs={[["origin", <>Tag: <b>host</b> · <b>you</b> · plugin id (<span className="tok">--tasty-accent-agent</span>)</>], ["remove", "user rows only"], ["not removable", <>lock glyph, <span className="tok">--tasty-glyph-dim</span>, with tooltip</>], ["not a disabled button", "nothing is pending — an affordance would lie"], ["IpcSequence", "mono one-line summary, steps joined by →"], ["sequence editing (2026-10-06 b2)", "Edit (ghost sm) on EVERY IpcSequence row (host / plugin edits save as a user override, like ShellCommand) → opens the inline text editor below"], ["not text-representable", "a stored method name the line format cannot carry → the row keeps caption \"Edit with CLI\" + copy; copies tasty hook-handler get --id <id>"], ["registry", "unchanged — defaults re-seed on start"]]}
            tokens={[{ tok: "--tasty-glyph-dim", use: "lock glyph", color: "var(--tasty-glyph-dim)" }, { tok: "--tasty-accent-agent", use: "plugin origin", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-font-mono", use: "event · action · sequence" }]} />
          <Note>The design's earlier “remove on every row” is dropped: the registry policy wins, and the lock is the honest reading of it.</Note>
        </Spec>
        <Spec id="hook-handlers-ipcsequence-text-editor" title="Hook Handlers — IpcSequence text editor"
          when={<>Edit expands the row <b>inline</b>: the one-line summary is replaced by a <b>CodeArea</b> (mono, line-number gutter) holding the sequence as text, one IPC call per line — <code>method</code>, a space, then optional one-line JSON params. A help line under the field states the format. The text is parsed <b>on every change</b>; the <b>first</b> error shows on its own line under the field (glyph + translated sentence with line / column, then the untranslated parser reason in mono muted), the line is marked in the gutter, and <b>Apply</b> is disabled while an error stands. Apply writes the <b>tab draft</b>; Settings <b>Save</b> commits it, Cancel in the footer reverts it, like every other row. An empty sequence is allowed with a muted note. <b>Mod+Enter</b> = Apply, <b>Esc</b> = Cancel; Enter is a newline. Adding a new IpcSequence handler from the Add card is out of scope this round.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><HookSeqEditorG state="normal" /></ThemePair>
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><HookSeqEditorG state="error" /></ThemePair>
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><HookSeqEditorG state="empty" /></ThemePair>
          </Stage>
          <Meta
            specs={[["placement", "inline — the row's second line becomes the editor; one row open at a time"], ["field", "CodeArea · minRows 4 · grows to codearea-max-height 200, then scrolls · no wrap"], ["help", "text-muted caption: One call per line: method, then optional JSON params. Lines starting with # are skipped and are not kept."], ["parse", "every change · first error only"], ["error line", "alertCircle + sentence in accent-danger caption · parser reason after it, mono caption text-muted (untranslated) · gutter number danger + tinted band"], ["error copy", "Line {line}: a method name is required before the params. · Line {line}: the method name contains a control character. · Line {line}, column {column}: invalid params JSON."], ["empty", "allowed · note: No calls. The handler does nothing."], ["buttons", "right-aligned · Cancel ghost sm · Apply secondary sm (disabled while an error stands)"], ["save flow", "Apply → tab draft → Settings Save"], ["keys", "Mod+Enter Apply · Esc Cancel · Enter newline"], ["reopen", "comments / blank lines are gone, JSON compact with sorted keys — the help line says so; no extra notice"]]}
            tokens={[{ tok: "--tasty-codearea-gutter-bg", use: "→ bg-sidebar", color: "var(--tasty-codearea-gutter-bg)" }, { tok: "--tasty-codearea-gutter-fg", use: "→ text-muted", color: "var(--tasty-codearea-gutter-fg)" }, { tok: "--tasty-codearea-error-fg", use: "→ accent-danger", color: "var(--tasty-codearea-error-fg)" }, { tok: "--tasty-codearea-max-height", use: "→ size-200" }]} />
        </Spec>
        <Spec id="hook-handlers-edited-default-mark-revert" title="Hook Handlers — edited default: mark + Revert"
          when={<>Editing (or switching off) a handler that <b>host</b> or a <b>plugin</b> registered saves a <b>user patch</b> under the same id; the row keeps its origin Tag and padlock. The row now says so: an <b>edited</b> Tag right after the origin Tag, and a <b>Revert</b> ghost button on the action line beside Edit. Revert follows the extension-mapping pending pattern: the summary goes back to the default, the button becomes <b>Undo</b> in the same slot, and the Tag reads <b>reverts on save</b>. Settings <b>Save</b> drops the patch (same effect as <code>tasty hook-handler remove --id</code>); Cancel restores it. Any patch counts — a changed sequence, a Switch turned off, or both — and one Revert clears all of it.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><HookOverrideG /></ThemePair>
          </Stage>
          <Meta
            specs={[["mark", "Tag (neutral) “edited” after the origin Tag — host / plugin rows with a user patch only"], ["mark tooltip", "Changed in your settings. Updates to the default no longer apply."], ["action", "Revert · ghost Button sm · action line, left of Edit"], ["Revert tooltip", "Go back to the default from {origin}."], ["pending", "button → Undo (same slot) · Tag “reverts on save” (disabled) · summary shows the default · Switch (at the default) and Edit disabled until Undo or Save — tooltip “Reverts on save. Undo to change it.” (batch 5)"], ["summary line", "mono term-sm · text-secondary — the Handler › Hook Handlers HookRow type on every row (batch 5)"], ["scope", "sequence edit · Switch off · both — any user patch"], ["Save / Cancel", "Save removes the patch · Cancel restores it"], ["user rows", "never marked — they are the user's own"]]}
            tokens={[{ tok: "--tasty-tag-disabled-bg", use: "pending Tag", color: "var(--tasty-tag-disabled-bg)" }, { tok: "--tasty-glyph-dim", use: "padlock", color: "var(--tasty-glyph-dim)" }, { tok: "--tasty-accent-agent", use: "plugin origin", color: "var(--tasty-accent-agent-raised)" }]} />
          <Note>No new tokens: the mark is the shared Tag, the pending state reuses the extension-mapping Undo + disabled Tag pair. Strings: <code>hook_handler.edited</code> “edited” · <code>hook_handler.edited_tip</code> · <code>hook_handler.revert</code> “Revert” · <code>hook_handler.revert_tip</code> · <code>hook_handler.reverts_on_save</code> “reverts on save” · <code>hook_handler.pending_locked_tip</code> “Reverts on save. Undo to change it.” · <code>common.undo</code>.</Note>
        </Spec>
        <Spec id="appearance-colour-rows-the-default-hex-is-read-only-not-disabled" title="Appearance › colour rows — the Default hex is read-only, not disabled"
          when={<>With <b>Default</b> checked, a colour row has no override and its hex field cannot be edited. The field still carries the <b>base value in use</b>, the only text value on the row, so it is <b>read-only</b>, not disabled: the same neutral box as a disabled Input, with the value in <span className="tok">--tasty-input-readonly-fg</span> (text-secondary) instead of the disabled ink. The value can be selected and copied; the field takes focus (1px focus edge, no ring). Unchecking Default starts the override and the field becomes a normal Input. Applies to the Tasty colour rows, the terminal surface background row and the Colors group.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, theme]) => (
              <div key={label} {...(theme ? { "data-theme": theme } : {})} style={{ width: "var(--tasty-size-360)", display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", marginBottom: "var(--tasty-space-xs)" }}>{label}</span>
                {[["accent", "#89b4fa", null], ["surface_bg", "#1e1e2e", "#181825"], ["selection", "#45475a", null]].map(([field, base, ov]) => (
                  <div key={field} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-control-height)" }}>
                    <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "var(--tasty-radius-pill)", flex: "none", background: ov ? "var(--tasty-accent-primary)" : "transparent" }} />
                    <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: ov ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{field}</span>
                    <span style={{ flex: "none", display: "flex", width: "var(--tasty-field-width-xs)" }}><Input block mono readOnly={!ov} defaultValue={ov || base} /></span>
                    <span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", flex: "none", background: ov || base, border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} />
                    <WCheckbox label="Default" checked={!ov} />
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["Default hex", "Input readOnly"], ["box", "state-disabled fill + border (same as disabled)"], ["value ink", "input-readonly-fg → text-secondary-raised (2026-10-21; Latte 5.17)"], ["select · copy", "allowed; focusable"], ["override", "normal Input"], ["disabled", "reserved for an unavailable control"]]}
            tokens={[{ tok: "--tasty-input-readonly-bg", use: "→ state-disabled-fill", color: "var(--tasty-input-readonly-bg)" }, { tok: "--tasty-input-readonly-border", use: "→ state-disabled-border", color: "var(--tasty-input-readonly-border)" }, { tok: "--tasty-input-readonly-fg", use: "→ text-secondary-raised", color: "var(--tasty-input-readonly-fg)" }]} />
        </Spec>
        <Spec id="appearance-colors-terminal-specific-5-rows-active-match-ink-afte" title="Appearance › Colors › Terminal-specific — 5 rows, active match ink after its fill"
          when={<>The theme key <code>search_match_active_fg</code> gets a row like every other palette field, so it can be changed without editing TOML. It goes <b>right after</b> <code>search_match_active_bg</code>: the ink sits on that fill, and reading the pair together is how a user judges it. The swatch stays the <b>same plain swatch</b> as every row. A text-on-fill sample would be the only row of its kind and would need the other row's value to draw; the live terminal is the real preview. <b>Reset all</b> clears it like any other override. The list is now 6 groups / <b>47 rows</b>; Terminal-specific has <b>5</b>.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha", [["selection_bg", "#585b70"], ["vi_cursor_bg", "#f5e0dc"], ["search_match_bg", "#fab387"], ["search_match_active_bg", "#a6e3a1"], ["search_match_active_fg", "#1e1e2e"]]], ["Latte", "latte", [["selection_bg", "#acb0be"], ["vi_cursor_bg", "#dc8a78"], ["search_match_bg", "#fe640b"], ["search_match_active_bg", "#40a02b"], ["search_match_active_fg", "#4c4f69"]]]].map(([label, theme, rows]) => (
              <div key={label} {...(theme ? { "data-theme": theme } : {})} style={{ width: "var(--tasty-size-360)", display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", marginBottom: "var(--tasty-space-xs)" }}>{label} · Terminal-specific (5)</span>
                {rows.map(([field, base]) => (
                  <div key={field} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-control-height)" }}>
                    <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "var(--tasty-radius-pill)", flex: "none" }} />
                    <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: field === "search_match_active_fg" ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{field}</span>
                    <span style={{ flex: "none", display: "flex", width: "var(--tasty-field-width-xs)" }}><Input block mono readOnly defaultValue={base} /></span>
                    <span style={{ width: "var(--tasty-swatch-size)", height: "var(--tasty-swatch-size)", borderRadius: "var(--tasty-swatch-radius)", flex: "none", background: base, border: "var(--tasty-border-width) solid var(--tasty-border-strong)" }} />
                    <WCheckbox label="Default" checked />
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["group", "Terminal-specific · 4 → 5 rows"], ["order", "selection_bg · vi_cursor_bg · search_match_bg · search_match_active_bg · search_match_active_fg"], ["label", "the key name, like every row (no translation)"], ["swatch", "unchanged plain swatch"], ["Default base", "Mocha #1e1e2e (neutral-200) · Latte #4c4f69 (neutral-1100)"], ["Reset all", "includes this row"], ["total", "6 groups / 47 rows"]]}
            tokens={[{ tok: "--tasty-search-match-active-fg", use: "the value this row edits", color: "var(--tasty-search-match-active-fg)" }, { tok: "--tasty-swatch-size", use: "swatch" }]} />
          <Note>The highlighted key name is only to point the new row out in this specimen; in the window every Default row uses the same muted ink.</Note>
        </Spec>
        <Spec id="appearance-font-override-rows-preview-below" title="Appearance › Font override — rows + preview below"
          when={<>The override grid uses the <b>colour-override idiom</b> already in Settings: label in <span className="tok">--tasty-settings-label-width</span> (150), the control at its own field width, then a trailing <b>Checkbox "Use default"</b> with its label always shown. Trailing means a long ko / ja label grows into free space instead of pushing the control. The <b>Preview</b> leaves the side column and sits <b>below</b> the grid at every window width, Focused and Unfocused side by side (they wrap under each other when narrow), then the one-line font summary. Nothing overlaps at 1100 or at the minimum width; the two-column egui split goes. A defaulted row keeps its control visible at the shared disabled look, filled with the default.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><div style={{ width: "var(--tasty-settings-content-max-width)", maxWidth: "100%" }}><FontOverrideG /></div></ThemePair>
            <ThemePair><div style={{ width: "var(--tasty-size-360)" }}><FontOverrideG long /></div></ThemePair>
          </Stage>
          <Meta
            specs={[["row", "label 150 · control · Use default — gap space-lg, min-h settings-row 32 (same as Settings Row)"], ["on default", "control is a read-only box with the value in effect, no opacity · empty when no value is in effect, no placeholder"], ["Font family", "searchable combo · field-width-lg 200"], ["Custom font file", "path Input mono · field-width-lg 200"], ["Font size · Line height", "number Input · field-width-xs 90"], ["DPI scaling", "Select · field-width-md 160"], ["Use default", "Checkbox + label, trailing, never truncated"], ["narrow", "row wraps: the checkbox drops under the control (row-gap space-xs)"], ["preview", "below the grid · space-lg above · Focused / Unfocused flex 1 each"], ["preview box (b2)", "the surface's effective bg (runtime colour, not a token) + effective font & size · 4 lines: latin · hangul · digits · kana · ink = surface focused fg · edge Focused border-strong / Unfocused separator · radius"], ["padding", "font-preview-padding-y space-sm · -x space-md"], ["line height", "font-preview-line-height → line-height-ui 1.4 × effective size; box height follows (no height token)"], ["stack", "a box narrower than font-preview-min-width (→ field-width-lg 200) wraps under the other — content width < 2 × 200 + space-md"], ["summary", "mono caption · text-muted"]]}
            tokens={[{ tok: "--tasty-settings-label-width", use: "150 label column (new name)" }, { tok: "--tasty-field-width-lg", use: "family · file" }, { tok: "--tasty-field-width-md", use: "DPI" }, { tok: "--tasty-field-width-xs", use: "numbers" }, { tok: "--tasty-settings-row-min-height", use: "row" }]} />
        </Spec>
        <Spec id="filehandler-file-extension-mapping-order-add" title="FileHandler › File Extension Mapping — order + Add"
          when={<>The product's structure is the design (it replaces the earlier per-extension Select). An extension <b>Input + Add</b> (Button secondary sm, the same pair as the capture blacklist), then per extension an <b>ordered detector list</b> — first match wins. Reorder with <b>IconButton sm</b> <code>chevronUp</code> / <code>chevronDown</code> (the ▲ ▼ text glyphs go). Arrows are <b>disabled, not hidden</b>: ▲ on the top row, ▼ on the last candidate, both on a row whose detector is off (muted name + disabled Tag), so rows keep one slot layout. Add is disabled while the input is empty or no detector is installed.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><ExtMapG /></ThemePair>
            <ThemePair><ExtMapG draft=".toml" /></ThemePair>
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><ExtMapG custom missing /></ThemePair>
            <ThemePair><ExtMapG custom missing long /></ThemePair>
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair><ExtMapG custom missing pendingRemove pendingReset /></ThemePair>
            <ThemePair><ExtMapG custom missing pendingRemove pendingReset long /></ThemePair>
          </Stage>
          <Meta
            specs={[["add", "Button secondary sm · disabled: empty input or no detector"], ["order", "IconButton sm chevronUp / chevronDown"], ["top / last row", "▲ / ▼ disabled"], ["non-candidate row", "both disabled · name text-disabled · Tag disabled \"off\""], ["hide instead?", "no — slots stay put"], ["Reset (2026-10-06)", "ghost Button sm · header right end · only when the extension has a custom order · tooltip kept · draft only"], ["not installed", "the group header alone: .ext text-disabled · Tag disabled · Remove ghost Button sm; no detector rows; separator under it"], ["header", "min-height button-height-sm — Reset appearing never moves the rows"], ["long copy", "Tag and button never truncate; .ext label is the shrinking item"], ["confirm", "none — both edit the draft, Cancel reverts"], ["pending (2026-10-06 b2)", "after Remove / Reset, before Save: the header stays; the pressed button becomes Undo (ghost sm, same slot); a Tag disabled says what Save does — \"removed on save\" / \"reset on save\""], ["pending remove", ".ext label line-through (text-disabled kept)"], ["pending reset", "rows already show install order; Reset → Undo"], ["Undo", "drops that one draft change; pressing it again is not a toggle back — Remove / Reset return"], ["Save / Cancel", "Save applies (removed group disappears, Reset button disappears); Cancel reverts every pending header"]]}
            tokens={[{ tok: "--tasty-state-disabled-fg", use: "disabled ink", color: "var(--tasty-state-disabled-fg)" }, { tok: "--tasty-settings-row-min-height", use: "row" }]} />
        </Spec>
      </Section>

      {<OverlaysWindowsB12_SettingsNarrowSection />}
      {<OverlaysWindowsFileHandlers_FileHandlerSection />}
      {<OverlaysWindowsPerf_PerfSection />}
      {<OverlaysWindowsAnomaly_AnomalySection />}
      <Section id="permissions" title="Settings › General › Permissions (macOS)">
        <Spec id="status-table-one-action-per-row-one-request-button" title="Status table, one action per row, one request button"
          when={<>macOS builds only, the last L2 under General. A three-column table: <b>permission</b> · <b>status</b> · <b>row action</b>. The four states are told apart by <b>glyph + word</b>, with colour as a third channel: <b>Granted</b> check / success, <b>Not granted</b> alertCircle / warning, <b>Unknown</b> helpCircle / muted, <b>Cannot check automatically</b> eyeOff / muted. Unknown (inference failed) and Cannot check (deliberately not looked at) share the muted ink but never the glyph, and neither can be misread as granted. The Full Disk Access shortcut moves <b>into its own row</b> as Secondary / Sm <b>[Open System Settings]</b>. What needs explaining per row now lives in HelpHints, so the two notes under the table are short. <b>[Request all permissions]</b> stays Primary / Md under the table. While requesting it is disabled and the line below becomes a spinner + <i>requesting</i> copy; the button carries no spinner. The debug-only Accessibility row carries a <b>debug</b> Tag.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["A · nothing granted — Mocha", "none", false, null], ["A · nothing granted — Latte", "none", false, "latte"], ["B · all granted", "all", false, null], ["C · FDA unknown, screen granted", "fdaUnknown", false, null], ["F · FDA granted before an update — Mocha", "fdaStale", false, null], ["F · FDA granted before an update — Latte", "fdaStale", false, "latte"], ["G · FDA turned off outside Tasty (revoked, 2026-10-08)", "fdaRevoked", false, null], ["D · requesting", "requesting", false, null], ["E · debug build (4 rows)", "none", true, null], ["E · debug build — Latte", "all", true, "latte"]].map(([cap, sc, dbg, theme]) => (
              <div key={cap} {...(theme ? { "data-theme": theme } : {})} style={{ width: "var(--tasty-size-560)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", background: "var(--tasty-bg-app)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
                  <MacPermissionsPane scenario={sc} debug={dbg} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["columns", "label (1fr) · status · row action"], ["row", "min 32 (settings row) · 1px border-default rule"], ["status", "glyph 14 + word · gap 4"], ["FDA action", "Secondary / Sm, in the FDA row"], ["primary", "Request all permissions · Primary / Md"], ["requesting", "button disabled · note line → spinner + copy"], ["notes", "caption 12 · text-muted · wrap at measure-xl"], ["narrow", "status + action wrap under each other, right-aligned; label never truncates"], ["debug row", "Tag \"debug\""], ["stale grant (2026-10-06)", "no 5th state — Not granted + a caption line under the label with the remedy (remove, then add again)"], ["revoked (2026-10-08)", "same pattern — Not granted + caption line \"turn it back on\"; no chip, no new status for either branch"]]}
            tokens={[{ tok: "--tasty-perm-granted-fg", use: "check", color: "var(--tasty-perm-granted-fg)" }, { tok: "--tasty-perm-missing-fg", use: "alertCircle", color: "var(--tasty-perm-missing-fg)" }, { tok: "--tasty-perm-unknown-fg", use: "helpCircle", color: "var(--tasty-perm-unknown-fg)" }, { tok: "--tasty-perm-unobservable-fg", use: "eyeOff", color: "var(--tasty-perm-unobservable-fg)" }, { tok: "--tasty-perm-row-height", use: "→ settings row 32" }]} />
          <Note><b>Copy changed</b> (en final, in the specimen): FDA button → "Open System Settings"; <i>detection_note</i> and <i>request_note</i> shortened, their per-row parts moved to the FDA and Folder access HelpHints; <i>requesting</i> shortened to two clauses. Status words unchanged.</Note>
          <Dont><b>Don't</b> paint Unknown or Cannot check as a blank or a dash. An empty status cell reads as "fine".</Dont>
        </Spec>
      </Section>

      <Section id="pluginswindow" title="Plugins window · plugin avatar">
        <Spec id="plugin-identity-mark-one-component-two-sizes" title="Plugin identity mark — one component, two sizes"
          when={<>A plugin manifest carries <b>no image</b>, so its identity in the Plugins window is a <b>square initial mark</b>: the first letter of the plugin name, mono, bold, in a tinted square at <span className="tok">--tasty-radius</span>. There are exactly <b>two sizes</b> — <b>sm (32)</b> on Installed / Attention list rows and <b>lg (46)</b> on a detail identity block and the <b>Add plugin</b> manifest preview (the old one-off 42px preview copy is gone; it was the same thing at different numbers). The tint is mixed into <span className="tok">--tasty-surface-raised</span>, a <b>fixed bed</b> — not the row background — so the mark is pixel-identical on a rest, hover and selected row. The initial's size is a <b>token per size</b>, not a ratio of the box: <b>13 at sm, 14 at lg</b> — inside the 14px UI cap, with no exception (2026-10-07; the earlier 16 is withdrawn). The two sizes differ by the box (32 / 46) and one type step. Weight is <b>regular</b>: no bold mono face ships, so colour and size carry the mark.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 32, alignItems: "flex-start", flexWrap: "wrap", padding: 14, background: "var(--tasty-bg-panel)" }}>
              <div style={{ display: "flex", gap: 18, alignItems: "flex-end" }}>
                {[["sm", "32 · list row"], ["lg", "46 · detail / preview"]].map(([s, l]) => (
                  <div key={s} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
                    <PluginAvatarG initial="G" size={s} />
                    <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{l}</span>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <PluginRowG name="git-helper" meta="tasty-labs · v1.4.2" state="rest" />
                <PluginRowG name="ai-review" meta="tasty-labs · v0.9.0" state="hover" />
                <PluginRowG name="docker" meta="community · v2.1.0" state="selected" />
                <PluginRowG name="vim-mode" meta="ophen · v3.2.1 · disabled" state="rest" disabled />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["sizes", "sm 32 · lg 46 — the whole roster"], ["content", "name initial, uppercase, mono regular"], ["initial size", "sm 13 · lg 14 — no UI-cap exception"], ["tint bed", "surface-raised — fixed, not the row bg"], ["mix", "bg 18% · border 38% of accent-primary"], ["row states", "mark unchanged — only the row bg moves"], ["disabled row", "whole row at state-disabled-opacity"]]}
            tokens={[{ tok: "--tasty-plugin-avatar-size-sm", use: "32 — list row" }, { tok: "--tasty-plugin-avatar-size-lg", use: "46 — detail / manifest preview" }, { tok: "--tasty-plugin-avatar-bg", use: "tinted square", color: "var(--tasty-plugin-avatar-bg)" }, { tok: "--tasty-plugin-avatar-border", use: "1px edge", color: "var(--tasty-plugin-avatar-border)" }, { tok: "--tasty-plugin-avatar-fg", use: "the initial", color: "var(--tasty-plugin-avatar-fg)" }, { tok: "--tasty-plugin-avatar-initial-font-size-sm", use: "→ font-size-body 13" }, { tok: "--tasty-plugin-avatar-initial-font-size-lg", use: "→ font-size-max 14" }, { tok: "--tasty-plugin-avatar-initial-weight", use: "→ font-weight-normal" }, { tok: "--tasty-plugin-avatar-border-width", use: "= --tasty-border-width" }]} />
          <Note><b>The mark carries identity, not classification.</b> An earlier draft coloured it by a plugin <i>category</i>, but a manifest has no category field — every real install fell back to one colour, so the hue said nothing while implying a taxonomy. Colour is now fixed at <span className="tok">--tasty-plugin-avatar-fg</span> for every plugin; state (running / error / needs attention) is already carried by the row's status dot and callouts, which is where a reader looks for it. If a classification axis is ever wanted here, it needs a manifest field first — and then it is a new decision, not this token.</Note>
          <Dont><b>Don't</b> mix the tint into the row background to "blend" on a selected row — the mark would then shift colour with row state and stop being a stable identity. And don't re-derive the initial's size from the box (<code>round(size × 0.42)</code>): that produced 19px at lg, off the type scale and over the UI cap with no decision behind it.</Dont>
        </Spec>
        <Spec id="copy-fingerprint-add-plugin-that-can-t-be-added" title="Copy fingerprint · Add plugin that can't be added"
          when={<><b>Copy fingerprint</b> moves out of the action bar: an <b>IconButton sm</b> <code>copy</code> right after the mono fingerprint, in Attention and in the Add plugin manifest card alike. No fingerprint → no line and no button (nothing to copy, so no disabled state). Attention's action bar has <b>no Details</b> for signature reasons (2026-10-06): the reason panel already shows the blurb and the fingerprint. <b>Add plugin</b> on a verified manifest that can't be added (already installed · signed but the public key file is missing · signature error) stays in its slot <b>disabled</b> — the variant it would have had, drawn with the shared disabled ink — and the reason replaces "Grants N permissions" on the left of the bar.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair>
              <FpLineG value="a13c 4e7f 2b08 9d51  ·  ed25519" />
              <div style={{ width: "var(--tasty-size-560)", maxWidth: "100%", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                <AddBarG />
                <AddBarG trusted={false} />
                <AddBarG blocked="installed" />
                <AddBarG perms={1} />
                <AddBarG perms={0} />
              </div>
            </ThemePair>
          </Stage>
          <Meta
            specs={[["copy fingerprint", "IconButton sm copy · after the value · absent without a fingerprint"], ["attention bar", "no Details (2026-10-06) — the reason panel already shows the blurb + fingerprint"], ["add — blocked", "disabled Add plugin · reason on the left"], ["reasons", "Already installed (2026-10-19: missing .pub / signature error are addable now — Add anyway)"], ["untrusted + .pub", "Trust & add — Primary (agent is for AI-agent surfaces only)"], ["grants", "No permissions · Grants 1 permission · Grants N permissions"]]}
            tokens={[{ tok: "--tasty-state-disabled-fg", use: "disabled ink", color: "var(--tasty-state-disabled-fg)" }, { tok: "--tasty-text-muted", use: "reason", color: "var(--tasty-text-muted)" }]} />
        </Spec>
        <Spec id="add-plugin-trust-judgment-six-kinds" title="Add plugin — trust judgment, six kinds"
          when={<>One box under the manifest card, toned by <b>what Add will do</b>: <b>success</b> — adds as is; <b>warning</b> — adds, and adding trusts the key (unknown key) or the new permission set (permissions changed); <b>danger</b> — unverified (2026-10-19: no longer blocked; explicit <b>Add anyway</b>, see the next spec). The fingerprint line follows the body whenever a fingerprint exists (signature errors have none). Untrusted-but-addable uses <b>Trust &amp; add</b> in <b>Primary</b>: the agent variant is reserved for AI-agent surfaces.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair>
              <div style={{ width: "var(--tasty-size-560)", maxWidth: "100%", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                {["trusted", "permissions-changed", "unknown-key", "missing-pubkey", "unsigned", "signature-error"].map((k) => <TrustBoxG key={k} kind={k} />)}
              </div>
            </ThemePair>
          </Stage>
          <Meta
            specs={[["tone", "success add · warning add + trust · danger blocked"], ["box", "tint-fill + tint-border of the tone · pad space-md / 14 · radius"], ["title", "glyph 16 + 13/600 in the tone"], ["body", "term-sm · text-secondary"], ["fingerprint", "after the body · absent for signature-error"], ["installed", "trust box as judged · bar reason 'Already installed' only (no second notice)"], ["Attention › signature invalid", "no fingerprint line (the signature it would identify is the broken part)"], ["homepage", "mono caption row 'Homepage' under Source · link text, opens the default browser"], ["homepage link (b2)", "text-secondary · 1px underline always · hover text-primary · focus = focus ring · pointer cursor (no accent: the row sits on surface-raised and text-secondary already clears 4.5:1)"], ["authors", "id · first author · +N (tooltip lists all)"], ["empty lists", "Permissions / Surface kinds: 'None' in text-muted, no Tag · font-size-caption (same as the mono caption rows it replaces)"], ["long fingerprint (b2)", "colon-hex over 16 bytes → first 8 + ' … ' + last 8 bytes, one line; tooltip + copy = full value"], ["Signature invalid (b2)", "header kept · fixed note 'The signature does not match this plugin's files.' · cause (key missing / mismatch …) as a mono caption text-muted line under it"], ["action-bar left text (b2)", "font-size-caption — the kit's 12 is corrected"], ["flow (b2)", "kit structure: path input, preview card directly under it, no 'Plugin information' title / second step"]]}
            tokens={[{ tok: "--tasty-tint-fill-alpha", use: "box fill" }, { tok: "--tasty-tint-border-alpha", use: "box edge" }, { tok: "--tasty-accent-warning", use: "add + trust", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-danger", use: "blocked", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-accent-success", use: "trusted", color: "var(--tasty-accent-success-raised)" }]} />
        </Spec>
        {<OverlaysWindowsPluginTrust_PluginTrustSpecs />}
        <Spec id="add-plugin-dashed-empty-hint-manifest-read-error" title="Add plugin — dashed empty hint · manifest read error"
          when={<>Before Verify the slot under the path field holds the <b>empty hint</b>: a <b>dashed</b> 1px border-default box. Renderers without CSS dashes draw it with <span className="tok">--tasty-border-dash</span> / <span className="tok">--tasty-border-dash-gap</span> (4 / 4) on the straight edges, centred so each edge starts and ends on a dash; the <b>corners stay solid</b> arcs. When Verify cannot read <code>tasty-plugin.toml</code> (missing, parse error) the <b>same box</b> takes the slot with a <b>solid accent-danger edge</b>: alertTriangle + “Can't read tasty-plugin.toml”, then the reader's reason as one mono caption line (untranslated). No fill, no action — the path field above is how to fix it.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ThemePair>
              <div style={{ width: "var(--tasty-size-400)", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-size-14) var(--tasty-space-lg)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) dashed var(--tasty-border-default)", color: "var(--tasty-text-muted)", fontSize: "var(--tasty-font-size-body)" }}>
                <WIcon name="folder" size="var(--tasty-icon-size-md)" /><span>Choose a folder and press <b style={{ color: "var(--tasty-text-secondary)" }}>Verify</b> to read its manifest.</span>
              </div>
              <div style={{ width: "var(--tasty-size-400)", display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-size-14) var(--tasty-space-lg)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid var(--tasty-accent-danger)" }}>
                <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-danger-raised)" }}><WIcon name="alertTriangle" size="var(--tasty-icon-size-md)" /></span>
                <span style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)", minWidth: 0 }}>
                  <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-accent-danger-raised)" }}>Can't read tasty-plugin.toml</span>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", overflowWrap: "anywhere" }}>TOML parse error at line 4, column 9: expected `=`</span>
                </span>
              </div>
              <div style={{ width: "var(--tasty-size-400)", display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-size-14) var(--tasty-space-lg)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid var(--tasty-accent-danger)" }}>
                <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-danger-raised)" }}><WIcon name="alertTriangle" size="var(--tasty-icon-size-md)" /></span>
                <span style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)", minWidth: 0 }}>
                  <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-accent-danger-raised)" }}>tasty-plugin.toml is not valid</span>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", overflowWrap: "anywhere" }}>binary "bin/imgview" not found in the plugin folder</span>
                </span>
              </div>
            </ThemePair>
          </Stage>
          <Meta
            specs={[["empty hint", "dashed 1px border-default · radius · pad 14 / 16"], ["dash", <>4 on / 4 off · <span className="tok">--tasty-border-dash</span> · <span className="tok">--tasty-border-dash-gap</span> · OFF-SCALE</>], ["corners", "solid arc; dashes on straight edges only, centred"], ["read error", "same box · solid accent-danger edge · no fill"], ["title", "Can't read tasty-plugin.toml · body · accent-danger"], ["reason", "mono caption · text-muted · untranslated"], ["invalid", "read but fails validation (binary path · extras) — same box · title “tasty-plugin.toml is not valid” · reason = validation message (batch 5)"], ["slot width", "measure-xl (560) capped by the column — unchanged (batch 5)"], ["action", "none — fix the path above and Verify again"]]}
            tokens={[{ tok: "--tasty-border-dash", use: "→ size-4" }, { tok: "--tasty-border-dash-gap", use: "→ size-4" }, { tok: "--tasty-border-default", use: "hint edge", color: "var(--tasty-border-default)" }, { tok: "--tasty-accent-danger", use: "read error", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note>The same dash pair draws the Scripts <b>Add trigger…</b> control (Misc › Scripts). Strings: <code>plugins.add.read_error</code> “Can't read tasty-plugin.toml” · <code>plugins.add.invalid</code> “tasty-plugin.toml is not valid”.</Note>
        </Spec>
        <Spec id="installed-detail-install-path-log-path-open-folder" title="Installed detail — install path · log path · Open folder"
          when={<>Last section of the installed detail, after Command (identity · description · error · Permissions · Command · <b>Install path</b>). <b>Open folder</b> moves to the <b>caption row</b>, right-aligned, so the path never competes with it for width: at the 720 minimum and the 880 default the button is always whole. The install path and the log path are mono caption muted, <b>wrap at any character</b> and stay selectable, so nothing is cut and no tooltip is needed.</>}>
          <Stage variant="grid" style={{ display: "flex", flexWrap: "wrap", gap: 16, padding: 20, alignItems: "flex-start" }}>
            <div style={{ width: 380, boxSizing: "border-box", padding: 16, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)",
                border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-bg-panel)" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Install path</span>
                  <div style={{ flex: 1 }} />
                  <Button variant="secondary" size="sm" leadingIcon={<WIcon name="folder" />}>Open folder</Button>
                </div>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", wordBreak: "break-all" }}>/home/tasty/.local/share/tasty/plugins/com.example.image-viewer-with-a-long-plugin-identifier</span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", wordBreak: "break-all" }}>Log: /home/tasty/.local/state/tasty/plugins/com.example.image-viewer-with-a-long-plugin-identifier/plugin.log</span>
              </div>
            <div style={{ width: 540, boxSizing: "border-box", padding: 16, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)",
                border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-bg-panel)" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Install path</span>
                  <div style={{ flex: 1 }} />
                  <Button variant="secondary" size="sm" leadingIcon={<WIcon name="folder" />}>Open folder</Button>
                </div>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", wordBreak: "break-all" }}>/home/tasty/.local/share/tasty/plugins/com.example.image-viewer-with-a-long-plugin-identifier</span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", wordBreak: "break-all" }}>Log: /home/tasty/.local/state/tasty/plugins/com.example.image-viewer-with-a-long-plugin-identifier/plugin.log</span>
              </div>
          </Stage>
          <Meta
            specs={[["order", "… Permissions · Command · Install path"], ["caption row", "INSTALL PATH (mono 10 caps) · flex · Open folder"], ["Open folder", "Button secondary sm · folder icon · opens the OS file manager"], ["path", "mono caption 11 · text-muted · break-all · selectable"], ["log", "same style, “Log: ” prefix, own line"], ["widths", "left 380 ≈ 720 window · right 540 ≈ 880 window"], ["shown", "installed plugins only"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "path text", color: "var(--tasty-text-muted)" }, { tok: "--tasty-font-size-caption", use: "11 path" }, { tok: "--tasty-space-sm", use: "8 row gap" }]} />
        </Spec>
        <Spec id="installed-detail-command-keycaps-per-platform" title="Installed detail — command keycaps per platform"
          when={<>A command keycap is the manifest chord split on “+”. On <b>Windows / Linux</b> each part is a word keycap. On <b>macOS</b> the chord first goes through the keybinding mapping (alt → Command, option → Option, ctrl → Control), the modifiers sort in Apple order <b>Ctrl · Option · Shift · Command</b> (⌃ ⌥ ⇧ ⌘), and each one follows the user's <b>modifier display style</b> from Settings › General — the same as the Settings keybinding rows and the modifier hint on that machine. <b>Ctrl stays the word “Ctrl”</b>: it has no display style and no glyph, so no Control icon is added. Manifest <code>ctrl + shift + h</code> and <code>alt + k</code> shown below.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-panel)", gap: "var(--tasty-space-md)", flexDirection: "column", alignItems: "flex-start" }}>
            {[["Windows / Linux", ["Ctrl", "Shift", "H"], ["Alt", "K"]], ["macOS · style text", ["Ctrl", "Shift", "H"], ["Cmd", "K"]], ["macOS · style symbol", ["Ctrl", "⇧", "H"], ["⌘", "K"]]].map(([lab, a, b]) => (
              <div key={lab} style={{ display: "grid", gridTemplateColumns: "var(--tasty-size-160) auto auto", columnGap: "var(--tasty-space-lg)", alignItems: "center" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{lab}</span>
                <Kbd keys={a} /><Kbd keys={b} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["order (macOS)", "Ctrl · Option · Shift · Command, then the key (⌃ ⌥ ⇧ ⌘) — kit comment fixed"], ["style", "follows Settings › General modifier display style (alt · option · shift) — never forced symbols"], ["Ctrl", "always the word “Ctrl” · no icon"], ["other OS", "word keycaps, manifest order"]]}
            tokens={[{ tok: "--tasty-kbd-size", use: "keycap" }]} />
          <Note>Attention, invalid signature — <code>plugins.attn_desc_hidden</code> (the design key now matches the app's flat <code>attn_*</code> keys; <code>plugins.attention.desc_hidden</code> retired): en “Description hidden — the signature is invalid.” · ko “서명이 유효하지 않아 설명을 숨겼습니다.” · ja “署名が無効なため説明を非表示にしています。”</Note>
        </Spec>
      </Section>

      <Section id="kbimportexport" title="Keybindings · Import / Export">
        <Spec id="l2-placement-entry-screen" title="L2 placement & entry screen"
          when={<>A <b>single</b> L2 section named <b>Import / Export</b>, pinned <b>last</b> under Keybindings (after Plugins). It moves a configuration rather than editing bindings, so it gets a <b>separator above it</b> — a <b>NEW axis on the settings L2 row model</b> (today a row carries only a label + <code>is_plugin</code>); the separator is suppressed while the sidebar filter is active, since filtering breaks the adjacency it describes. The section's list position is <b>not a list</b>: two action rows, each with a one-line description and its own trailing button. <b>Import is primary, Export secondary</b> — import is the path with consequences and the one people come here for; export is a single fire-and-forget write.<br /><br />While we were here we also corrected the <b>Keybindings L2 roster</b> to the eleven real sections (<b>General · Workspace · Pane · Tab · Surface · Clipboard · Zoom · Explorer · Scripts · Preset · Plugins</b>) plus the new twelfth: the design carried a phantom <b>Image</b> section and was missing <b>Explorer</b> and <b>Scripts</b>.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 24, alignItems: "flex-start", flexWrap: "wrap", padding: 14, background: "var(--tasty-bg-panel)" }}>
              <IeL2Tail />
              <IeEntry />
            </div>
          </Stage>
          <Meta
            specs={[["L2 items", "one — “Import / Export”"], ["position", "last, after Plugins"], ["separator", "1px above the row (new axis)"], ["filtering", "separator hidden while filtering"], ["entry", "2 action rows, not a ListCtrl"], ["weights", "Import primary · Export secondary"], ["export feedback", "the window's own toast, carrying the path"], ["file picker", "popup on the window's PopupManager"]]}
            tokens={[{ tok: "--tasty-separator", use: "L2 separator + row rules" }, { tok: "--tasty-surface-raised", use: "action row bed", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-default", use: "action row edge", color: "var(--tasty-border-default)" }, { tok: "--tasty-settings-sidebar-width", use: "200 — L2 column" }]} />
          <Note><b>Why a popup for the file picker</b> and not another drill-down step: the drill-down is already spoken for by the preview (list ⇄ detail), and the same picker serves both directions — Export needs a save target, Import an open target. The settings window already runs a PopupManager for the shortcut-conflict confirm, so this adds a case, not a mechanism.</Note>
          <Dont><b>Don't</b> give export its own result screen. There is nothing to do after a write, so the confirmation is a <b>toast with the resolved path</b> and the user stays where they were.</Dont>
        </Spec>

        <Spec id="import-preview-group-headers-select-column-long-table-handling" title="Import preview — group headers, select column, long-table handling"
          when={<>The detail view keeps the Preset skeleton (back bar with <b>Apply</b> in the right slot, over the diff grid) and grows two axes. <b>Group headers</b> (NEW on this table): one row spanning all columns, carrying a select-all box, the group name in mono micro caps, a <b>changed / total</b> count and a collapse chevron — it does <b>not</b> repeat Action / Current / Imported. Four groups: general combos, quick switch, script bindings, plugin overrides. A <b>select column</b> (32px, leading) makes apply <b>per row</b>; the group box is the group's select-all. The third column header is the fixed word <b>Imported</b> — the file name lives in the intro line, where it can be long without breaking the grid.<br /><br />Length is handled by <b>both</b> levers: the table opens <b>changed-only</b> (a <b>Show all 73</b> toggle in the back bar) and every group collapses. At minimum 73 rows — 100+ once slots are expanded — neither alone is enough. <b>Quick switch is summarised one row per axis</b>, not per slot: slots store raw keys and the combo is composed for display, so the axis row's value is the composed range (<span className="tok">Ctrl+1…0</span>) with the slot count as a sub-line. That keeps 29 slot rows out of the table and matches where the edit actually happens.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", flexDirection: "column", gap: 12, alignItems: "flex-start", padding: 14, background: "var(--tasty-bg-panel)" }}>
              <IeBackBarG unresolved={0} />
              <IeGrid />
            </div>
          </Stage>
          <Meta
            specs={[["columns", "32px select · 1.6fr action · 1fr current · 1fr imported"], ["group header", "spans all columns, on surface-raised"], ["group content", "select-all · name (mono 10 caps) · counts · chevron"], ["groups", "general · quick switch · scripts · plugin overrides"], ["default view", "changed only"], ["toggle", "“Show all {n}” in the back bar"], ["quick switch", "1 row per axis (3), slot count as sub-line"], ["plugin row", "command name + agent-dot plugin name, two lines"], ["changed cell", "accent-primary, colour only (no bold)"]]}
            tokens={[{ tok: "--tasty-surface-raised", use: "group header bed", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-accent-primary", use: "changed imported value", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-agent", use: "plugin dot", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-separator", use: "cell rules" }, { tok: "--tasty-letter-spacing-caps", use: "header + group name tracking" }]} />
          <Note><b>Apply writes the draft, Save commits it</b> — the same two-stage contract as Preset, and the intro line says so. The two never share a row: Apply sits in the back bar, Save in the window footer.</Note>
        </Spec>

        {/* "Option migration" Spec removed 2026-10-10 (b12): the app deleted option migration (key-position policy — Win / Super record as option). */}

        <Spec id="the-six-open-values-failure-notices-conflicts-placeholder-tokens" title="The six open values — failure, notices, conflicts, placeholder, tokens (4 · 5 belong to the removed option migration — retired 2026-10-10)"
          when={<>Six things the first pass left blank. The rule behind all of them: <b>a result with nothing to do is a toast; a result with something to do is inline, where the thing to do lives.</b><br /><br /><b>1 · Export failure</b> — success stays a toast carrying the resolved path. Failure is an inline danger block <b>inside the Export row</b> (the row that started it), with <b>Try again</b> and <b>Choose another location…</b>; the row's button goes disabled while the block is up, so there is one live retry, not two.<br /><br /><b>2 · Other bundle warnings</b> — the dropped-override <b>info line</b> stays exactly as it is (muted, no tone, no glyph weight): nothing is wrong and there is nothing to do. Everything that <i>is</i> a warning — newer schema tag, unknown actions, empty groups — collects in <b>one warning block</b>, one line per notice, count in the header, in that fixed order. Three lines show; the rest fold behind <b>Show {"{n}"} more</b>. Never a block per notice.<br /><br /><b>3 · Parsing failure with no line number</b> — the line clause is <b>replaced, not dropped</b>: the middle sentence always says why the file failed. With a position: “parsing stopped at line 1.” Without: “the file isn't TOML.” The first and last sentences never change, so the two read as one message.<br /><br /><b>4 · Several conflicts</b> — <b>count first</b>, in the card intro: “<b>3 conflicts</b> — those shortcuts are already bound.” The per-row inline reason stays on every row (it names <i>which</i> binding), so the summary never repeats the list and nothing needs collapsing. The summary line appears from <b>2</b> up; at 1 the row line alone carries it.<br /><br /><b>5 · Modifier Select placeholder</b> — <b>Select a modifier</b>, sentence case, UI font (not mono — it is not a key), <span className="tok">--tasty-text-placeholder</span>. It is a sentinel first option that leaves the list once a real combo is chosen, and the row keeps its warning-toned <b>Not set</b> trailer. Not an em-dashed pseudo-value like “— pick a modifier —”, which reads as a choice.<br /><br /><b>6 · Tokens — opened.</b> The raw <span className="tok">--tasty-size-*</span> reads in the spec jsx were a tier violation, so they now have names (below). The off-grid <b>14px</b> card inset was a slip: it snaps to <span className="tok">--tasty-space-md</span> (12). Drop the quoted constants and the raw-dimension ratchet and read the tokens.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", flexDirection: "column", gap: 14, alignItems: "flex-start", padding: 14, background: "var(--tasty-bg-panel)" }}>
              <IeExportFailG />
              <IeBundleNoticesG />
              <IeParseFailG />
              <IeParseFailG line={false} />
              <IeConflictSummaryG />
              <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: "var(--tasty-kb-ie-notice-inset)", borderRadius: "var(--tasty-radius)",
                background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)" }}>
                <IeModifierSelectG />
                <IeModifierSelectG chosen />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["export success", "toast with the resolved path"], ["export failure", "inline danger block in the Export row"], ["failure actions", "Try again · Choose another location…"], ["info line", "dropped overrides — muted, unchanged"], ["warning block", "one block, 1 line per notice, count in header"], ["notice order", "schema → unknown actions → empty groups"], ["fold", "3 lines, then “Show {n} more”"], ["parse copy", "line clause replaced by “the file isn't TOML.”"], ["conflicts", "count-first line from 2 up; row lines always"], ["placeholder", "“Select a modifier” · text-placeholder"]]}
            tokens={[{ tok: "--tasty-settings-content-max-width", use: "620 — content column cap (NEW)" }, { tok: "--tasty-kb-ie-select-column-width", use: "32 — diff select column (NEW)" }, { tok: "--tasty-kb-ie-action-column-width", use: "288 — action label column + sub-line indent (NEW)" }, { tok: "--tasty-kb-ie-from-column-width", use: "120 — original shortcut column (NEW)" }, { tok: "--tasty-kb-ie-slot-height", use: "24 — replacement slot chip (NEW)" }, { tok: "--tasty-kb-ie-slot-min-width", use: "140 — slot chip min width (NEW)" }, { tok: "--tasty-kb-ie-notice-inset", use: "12 — notice / card inset, both axes (NEW, was off-grid 14)" }, { tok: "--tasty-accent-warning", use: "warning block + Not set", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-danger", use: "failure blocks + conflict count", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note><b>Why the notices are one block and the info line is not in it.</b> Tone is the sorting key, not topic: a reader scans for "is anything wrong". Merging an unactionable fact into a warning-toned block makes the whole block unactionable-looking; splitting the four warnings into four blocks makes a wall where one thing is needed — read it, then go look at the table.</Note>
          <Dont><b>Don't</b> toast an export failure. A toast auto-dismisses and carries no retry, so the one case where the user must act is the one case that disappears on its own.</Dont>
        </Spec>

        <Spec id="open-values-unknown-failure-reason-counts-of-one-and-the-620-cap" title="Open values — unknown failure reason, counts of one, and the 620 cap"
          when={<>Three blanks left by the first round. <b>An export failure with no known cause</b> keeps the block exactly as decided — only the middle clause changes, from a fixed set (<span className="ic">read-only</span> · <span className="ic">permission</span> · <span className="ic">disk full</span> · catch-all <b>the write didn't finish.</b>), and the OS string is never spliced into the sentence: it gets its own muted mono line below, one line, ellipsised, full text in <span className="tok">title</span>. <b>Counts of one</b> get singular forms (<span className="ic">1 notice</span>, <span className="ic">1 unknown action was skipped</span>) — English only; ko/ja don't inflect.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", flexDirection: "column", gap: 14, alignItems: "flex-start", padding: 14, background: "var(--tasty-bg-panel)" }}>
              <IeExportFailG reason="other" />
              <IeBundleNoticesG one />
            </div>
          </Stage>
          <Meta
            specs={[["clause set", "read-only · permission · disk full · the write didn't finish."], ["OS text", "own line, mono 11, muted — never inside the sentence"], ["first / last sentence", "never change (same as parse failure)"], ["singular", "1 notice · 1 unknown action was skipped (…)"], ["fold link", "absent at 1 notice — nothing to fold"], ["620 cap", "the settings scroll column, every non-full-bleed subtab"], ["prose", "keeps --tasty-measure-md (reading measure, narrower)"]]}
            tokens={[{ tok: "--tasty-settings-content-max-width", use: "620 — now set once on the settings content column" }, { tok: "--tasty-text-muted", use: "OS reason line", color: "var(--tasty-text-muted)" }, { tok: "--tasty-accent-danger", use: "failure block", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note><b>Where the 620 cap lives.</b> On the <b>settings content column itself</b> — the scrolling pane inside the Settings window — so every non-full-bleed L2 subtab inherits it, UI-kit subtabs included, and no block carries its own width. <b>Full-bleed subtabs are exempt</b>: they replace the column with their own layout (diff tables, editors). Body prose keeps <span className="tok">--tasty-measure-md</span> — a line-length measure, a different axis from the column cap and narrower than it.</Note>
          <Dont><b>Don't</b> build the OS message into the sentence (“… — os error 28: No space left on device.”). It breaks the sentence's grammar in three languages and buries the two parts the user can act on: which path, and that nothing was written.</Dont>
        </Spec>
      </Section>

      {<OverlaysWindowsB12_OptionKeySection />}
      <Section id="scripts" title="Misc · Scripts (Lua script manager)">
        <Spec id="scripts-subsection-list-states-empty" title="Scripts subsection — list, states & empty"
          when={<>Settings › <b>Misc</b> › <b>Scripts</b>. A subsection (not a separate popup) that manages user <b>Lua scripts</b> run by a shortcut. Each <b>ScriptRow</b>: the display name, the absolute path (<b>middle-elided</b> — dir tail truncates, filename always shown), a bound-shortcut <span className="ic">Kbd</span> badge (or italic <b>Unbound</b>), a peach <b>changed</b> badge + help line when the file's SHA no longer matches the one recorded at registration (TOFU re-confirm on next run), and an <b>Auto-run</b> row: mono <b>trigger chips</b> for the host-lifecycle events the script fires on (click a chip to remove it) plus a dashed <b>Add trigger…</b> control offering the remaining events. Row actions: bind shortcut (→ Keybindings), rename (inline), remove (inline confirm).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>registered scripts</div><ScriptManagerFrame /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty state</div><ScriptManagerFrame empty /></div>
          </Stage>
          <Meta
            specs={[["home", "Settings › Misc › Scripts (subsection)"], ["row", "name · path · shortcut · actions · auto-run"], ["shortcut", <><span className="ic">Kbd</span> badge or italic Unbound</>], ["changed", "peach badge + help line (SHA mismatch)"], ["auto-run", "trigger chips + Add trigger… (13 lifecycle events)"], ["Add trigger… edge", <>dashed · <span className="tok">--tasty-border-dash</span> / <span className="tok">--tasty-border-dash-gap</span> 4 / 4 · corners solid (2026-10-07)</>], ["trigger menu", <>min 200 · max 220 then scrolls · <span className="tok">--tasty-trigger-menu-min-width</span> / <span className="tok">-max-height</span> · menu-* container</>], ["all bound", "control stays, DISABLED (disabled ink) · tooltip All events already bound"], ["chip", "mono event · click removes · hover 12% overlay"], ["actions", "bind · rename · remove"], ["empty", "glyph + Add script prompt"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "changed badge + help", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-border-default", use: "chip + add-control border", color: "var(--tasty-border-default)" }, { tok: "--tasty-overlay-active", use: "chip hover / menu open", color: "var(--tasty-overlay-active)" }, { tok: "--tasty-text-disabled", use: "Unbound", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-font-mono", use: "path · trigger chips" }]} />
          <Note>Two independent run paths: a <b>manual shortcut</b> (bound in the Keybindings tab, shown as the <span className="ic">Kbd</span> badge) and <b>auto-run triggers</b> (edited inline here — the script fires when the host emits a bound lifecycle event). A script can have either, both, or neither. The <span className="ic">changed</span> state is informational — the script still runs, but re-confirms once (TOFU) because the on-disk file drifted from the registered hash.</Note>
        </Spec>
      </Section>

      <Section id="gitviewer" title="Git viewer">
        <Spec id="git-worktree-viewer-read-only-popup" title="Git worktree viewer — read-only popup"
          when={<>Tools → Git. A <b>960×640</b> read-only popup that surveys every worktree. A header (<span className="ic">Git</span> + Refresh) sits over a <b>context strip</b> (current worktree · branch · HEAD oid · repo path). A left <b>worktree rail</b> (fixed 232px) lists main + linked worktrees as <b>two-line rows</b> — name + type badge, then oid + state badge — so nothing overflows the rail; the right column splits 50/50 into <b>Changes</b> (status) over <b>Commits</b> (log). Every badge is a <b>token-tinted pill</b> (the Tag visual), not colored text. <b>No writes</b> — only Refresh, selecting a worktree (re-binds the panes), selecting a changed file, and the diff Back button.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><GitViewerFrame /></Stage>
          <Meta
            specs={[["frame", "960 × 640 · dismiss on outside click"], ["header", "Git + Refresh · context strip (wt · branch · oid · path)"], ["rail", "232px — 2-line rows: name+type / oid+state"], ["right", "50/50 split — Changes over Commits"], ["badges", "Tag-style pills: main · linked · current · locked · invalid"], ["status", "M·A·D·R·?·U fixed-width pill prefix"], ["read-only", "Refresh · select · Back only"]]}
            tokens={[{ tok: "--tasty-accent-info", use: "oid · main · refs · R", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-accent-success", use: "current · A", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-warning", use: "locked · M", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-danger", use: "invalid · D · U", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-surface-active", use: "selected row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-bg-sidebar", use: "section / context strips", color: "var(--tasty-bg-sidebar)" }]} />
          <Do><b>Do</b> distinguish <b>"current"</b> worktree (green badge — where cwd lives) from the <b>selected</b> row (surface-active fill — what you're viewing). They can differ: you can inspect a non-current worktree without switching anything.</Do>
        </Spec>
        <Spec id="diff-variant-file-row-unified-diff" title="Diff variant — file row → unified diff"
          when={<>Clicking a file in Changes swaps the bottom pane (log → diff): a <b>Back</b> toolbar + file path, then unified hunks with an old/new line-number gutter. Hunk header in <span className="tok">--tasty-accent-info</span>, <code>+</code> in success, <code>-</code> in danger, context primary. Back returns to the log.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}><GitViewerFrame diff /></Stage>
          <Meta
            specs={[["trigger", "select a Changes row → diff"], ["toolbar", "Back + file path (muted/mono)"], ["gutter", "old / new line numbers (mono)"], ["hunk header", <span className="tok">--tasty-accent-info</span>], ["empty diff", "Back + 'No changes.'"], ["states", "non-repo · error · loading · already-open · 0 worktrees"]]}
            tokens={[{ tok: "--tasty-accent-info", use: "hunk header", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-accent-success", use: "added line", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-danger", use: "deleted line", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-disabled", use: "line-number gutter", color: "var(--tasty-text-disabled)" }]} />
          <Note>Empty/edge states reuse <span className="ic">--tasty-text-muted</span> one-liners: "No git repository…", "No changes.", "No commits.", "No worktrees.", "Git viewer is already open." Errors are a single <span className="ic">--tasty-accent-danger</span> line above the (still shown) panes.</Note>
        </Spec>
        <Spec id="diff-toolbar-a-container-height-not-a-button-height" title="Diff toolbar — a container height, not a button height"
          when={<>The diff toolbar is <b>32px</b> tall and holds 28px controls with 2px of air above and below. It now has its own name — <span className="tok">--tasty-git-toolbar-height</span> — rather than borrowing <span className="tok">--tasty-control-height</span>: the two are different roles and, as it happens, different numbers (32 vs 28), so aliasing them would have been wrong in value as well as in meaning. The value does not change.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: 460, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <div style={{ height: "var(--tasty-git-toolbar-height)", display: "flex", alignItems: "center", gap: 6, padding: "0 8px",
                background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
                <Button variant="ghost" size="sm">Unified</Button>
                <Button variant="ghost" size="sm">Split</Button>
                <div style={{ flex: 1 }} />
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>+128 −44</span>
              </div>
              <div style={{ padding: "10px 12px", fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.7 }}>
                <div>@@ -12,7 +12,9 @@ fn draw_path_bar()</div>
                <div style={{ color: "var(--tasty-accent-success-raised)" }}>+    let avail = bar_w - buttons_w - gap;</div>
                <div style={{ color: "var(--tasty-accent-danger-raised)" }}>-    let avail = popup_w;</div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["height", "32 — unchanged"], ["token", <span className="tok">--tasty-git-toolbar-height</span>], ["holds", "28px controls + 2px air"], ["not", <><span className="tok">--tasty-control-height</span> (28, a control's own box)</>]]}
            tokens={[{ tok: "--tasty-git-toolbar-height", use: "toolbar container" }, { tok: "--tasty-control-height", use: "the buttons inside it" }]} />
        </Spec>
      </Section>

      <Section id="clipboard" title="Clipboard viewer">
        <Spec id="clipboard-viewer-read-only-snapshot-popup" title="Clipboard viewer — read-only snapshot popup"
          when={<>Tools → Clipboard Viewer (or a toggle shortcut). A <b>480×360</b> screen-centered popup that shows what's on the system clipboard <b>right now</b> — a one-time snapshot, <b>not</b> history. <b>Single column</b>: a header (clipboard icon + <span className="ic">snapshot</span> tag), a <b>type bar</b>, then a recessed mono <b>code well</b>. The type bar treats a <b>single type as first-class</b> — just a <span className="ic">Text</span> badge + live metadata (<span className="ic">284 chars · 6 lines · UTF-8</span>); two-plus types expand it into a <b>segmented switch</b> (Text / Image / Files / HTML / Other). <b>No rail.</b> At <b>5 segments</b> the switch compacts to <b>icon-only</b> (the active segment keeps its label; tooltips carry the names) so it survives 20–40% longer translations inside 480px. <b>HTML</b> is shown as <b>source text only — never rendered</b>; a <span className="ic">Pretty print</span> checkbox takes over the type bar's meta slot (metadata moves beside the mime in the footer) and re-indents the same source instantly (0ms). <b>Other</b> buckets every format that isn't text/image/files/html — one row per format: mono name + size, then its textualized content, separated by 1px rules, all of it scrolling in the same well (long content clamps with a muted <span className="ic">+N more lines</span>). <b>Read-only</b> — no copy/paste/edit/delete.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>single type (Text)</div><ClipboardFrame /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>multi-type · segmented</div><ClipboardFrame state="multi" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>HTML · source (Pretty print off)</div><ClipboardFrame state="html" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>HTML · Pretty print on</div><ClipboardFrame state="htmlPretty" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Other · format buckets</div><ClipboardFrame state="other" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>5 types · compact icon-only segments</div><ClipboardFrame state="five" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty</div><ClipboardFrame state="empty" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>read failed</div><ClipboardFrame state="failed" /></div>
          </Stage>
          <Meta
            specs={[["frame", "480 × 360 · screen-center · dismiss outside"], ["layout", "single column — no rail"], ["type bar", "1 type = badge + meta · 2–4 = segmented · ≥5 = icon-only"], ["segment", "h 26 · pad 0 10 (icon-only 0 8, min-w 30)"], ["metadata", "chars · lines · encoding (mono 11, right) — moves to footer on HTML"], ["HTML body", "source only, never rendered · Pretty print = re-indent, 0ms"], ["Other body", "per format: mono 11 name + size / mono 12 content · 1px rules"], ["body", "recessed code well — bg-app · mono 12 · scroll"], ["empty / fail", "centered glyph + muted / danger one-liner"]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-bg-app", use: "code well", color: "var(--tasty-bg-app)" }, { tok: "--tasty-bg-sidebar", use: "type bar", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-accent-primary", use: "selected type segment", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-danger", use: "read-failed", color: "var(--tasty-accent-danger-raised)" }]} />
          <Dont><b>Don't</b> add any mutate action (copy / re-copy / clear) or a master-detail type rail — this is a one-column snapshot inspector. Snapshot is taken once at open; no live polling, no history. <b>Don't</b> render the HTML payload (no live preview, no styled output) and <b>don't</b> let the type bar scroll horizontally or truncate labels with ellipsis — compact to icon-only instead.</Dont>
        </Spec>
      </Section>

      <Section id="notifications" title="Notifications — popup · fullscreen">
        {true ? <GalleryNotifications_Popup /> : null}
      </Section>

      <Section id="moveresize" title="Move & resize — popup interaction spec">
        <Spec id="drag-handles-titlebar-region-none" title="Drag handles — TitleBar · Region · None"
          when={<>How a popup moves. A <b>titlebar</b> popup drags from the <b>whole bar</b> (the close button is excluded). A <b>headless</b> popup can declare a <b>Region</b> handle — a header band (currently the left half: icon + title) — while the header's own widgets (search, close) keep priority. <b>None</b> means the popup can't be moved. Affordance is the <b>cursor</b> only (grab → grabbing); no painted grip. Hover the bars below to feel the cursors.</>}>
          <Stage variant="solo center" style={{ padding: 24, background: "var(--tasty-bg-app)", gap: 20, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>TitleBar — whole bar drags</div>
              <FauxPopup title="Listening ports"><div>title centered · close excluded from the handle</div></FauxPopup>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Region — header band drags, widgets win</div>
              <FauxPopup titlebar={false} region w={260}><div>left band = grab · search/close = their own cursor</div></FauxPopup>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>None — not movable</div>
              <FauxPopup title="apply preset" grab={false}><div>no handle · grab cursor never appears</div></FauxPopup>
            </div>
          </Stage>
          <Meta
            specs={[["TitleBar", "whole bar drags, close excluded"], ["Region", "header left band (headless popups)"], ["None", "immovable"], ["affordance", "cursor only — no painted grip"], ["priority", "widget > close > resize edge > drag handle > content"], ["start gate", "after content render — widget pointer wins"]]}
            tokens={[{ tok: "--tasty-bg-app", use: "titlebar fill (mantle)", color: "var(--tasty-bg-app)" }, { tok: "--tasty-surface-raised", use: "popup body", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-titlebar-height", use: "bar / band height" }]} />
          <Dont><b>Don't</b> tint or outline the drag band over a widget — the widget always wins the press, so a visual grip there only invites mis-clicks. Let the cursor carry the affordance.</Dont>
        </Spec>
        <Spec id="8-direction-resize-handle-map-cursors" title="8-direction resize — handle map & cursors"
          when={<>Opt-in (<span className="ic">resizable</span> popups — today: <b>Listening ports</b> & <b>Remote connections</b>). A <b>band just inside the border</b> holds 8 hit zones: 4 edges + 4 corners (corners take priority). There's <b>no visual grip</b> — the cursor is the only affordance. Dragging an edge moves <b>only that edge</b> (the opposite side stays fixed); a corner moves two. Hover the outlined zones to see each cursor.</>}>
          <Stage variant="solo center" style={{ padding: 28, background: "var(--tasty-bg-app)", gap: 28, flexWrap: "wrap", alignItems: "center" }}>
            <ResizeMap />
            <CursorMatrix />
          </Stage>
          <Meta
            specs={[["zones", "N · S · E · W + NW · NE · SW · SE"], ["band", "inside the border (token-width)"], ["corner", "wins over edge at the crossing"], ["edge drag", "that edge moves, opposite fixed"], ["grip", "none — cursor only"], ["persist", "user size sticks until the popup closes"]]}
            tokens={[{ tok: "--tasty-accent-info", use: "zone labels (demo only)", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-border-strong", use: "popup edge", color: "var(--tasty-border-strong)" }]} />
          <Note>Once you resize, the size <b>sticks</b> (the sizer won't overwrite it); closing the popup resets it to its default size on next open.</Note>
        </Spec>
        <Spec id="constraints-z-order-quiet-limits-top-most-wins" title="Constraints & z-order — quiet limits, top-most wins"
          when={<>Limits are enforced <b>silently</b>. A popup can't shrink past its <b>min size</b> (or its default size if none is set), and <b>no part</b> may leave the scope rect (Window / Workspace / Pane / Tab / Surface) — drag or resize just stops at the boundary, and a shrinking scope <b>auto-repositions</b> the popup back inside. The scope edge is the effective max (there's no separate max size). When popups overlap, only the <b>top-most</b> one's handles/cursor respond; pressing a popup brings it to front.</>}>
          <Stage variant="solo center" style={{ padding: 24, background: "var(--tasty-bg-app)", gap: 28, flexWrap: "wrap", alignItems: "center" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>scope clamp — stops at the boundary</div>
              <ClampDemo />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>z-order — only the front popup responds</div>
              <ZOrderDemo />
            </div>
          </Stage>
          <Meta
            specs={[["min size", "min_size, else default_size"], ["max", "scope rect is the cap (no max_size)"], ["clamp", "no part leaves the scope"], ["reposition", "shrinking scope pulls it back in"], ["limit signal", "none — quiet stop (no warning color)"], ["z-order", "top-most only · press = bring to front"]]}
            tokens={[{ tok: "--tasty-border-strong", use: "scope rect (demo)", color: "var(--tasty-border-strong)" }, { tok: "--tasty-shadow-modal", use: "popup lift" }]} />
          <Dont><b>Don't</b> flash a warning color (peach/red) when a popup hits min size or the scope edge — the limit is communicated by simply <b>not moving</b>. Reserve danger tones for actual errors.</Dont>
        </Spec>
      </Section>
      <style>{`.rz-zone:hover{background:color-mix(in srgb,var(--tasty-accent-info) 14%,transparent);} .faux-close:hover{color:var(--tasty-titlebar-close-hover-fg);background:var(--tasty-titlebar-close-hover-bg);}`}</style>
    </>
  );
}

export const page = {
  active: "overlays-windows",
  nav: NAV,
  head: {
    title: "Windows",
    intro: "Browse the larger windows used for settings, launchers, and data tables. Compare their navigation, dimensions, and tab views to choose a layout for your content.",
    
  },
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
