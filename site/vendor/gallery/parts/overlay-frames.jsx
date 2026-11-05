import React from "react";
import { MenuItem, Input, Select, Switch, Checkbox, Button, IconButton, Tag, Kbd, Table, StatusDot, Spinner, Icon, CenterState } from "../../components/index.js";
import { Stage } from "./specimen.jsx";

// Tasty Gallery — Overlays SHARED frames. Loaded by every overlays-*.html page
// (Dialogs / Windows / Popups & menus / Banners). Defines the scrim Backdrop, the
// icon set, and all live overlay frame components, then exposes them on
// parts/overlay-frames.jsx. No page content here — see overlays-<family>.jsx.

// Tasty Gallery — Overlays. The modal / popup layer: scrim recipe,
// command palette, the two-tier settings window, agent approval, and
// rename. These are the patterns an agent should copy for any new
// dialog — exact frame dimensions and the scrim contract are spec'd.



const NAV = [
  { id: "scrim", label: "Scrim & frame" },
  { id: "banner", label: "Banner" },
  { id: "palette", label: "Command palette" },
  { id: "tools", label: "Tools menu" },
  { id: "ports", label: "Listening ports" },
  { id: "remote", label: "Remote connections" },
  { id: "search", label: "Search bar" },
  { id: "switch", label: "Switch-number overlay" },
  { id: "approval", label: "Agent approval" },
  { id: "convert", label: "Convert surface" },
  { id: "filehandler", label: "File handler picker" },
  { id: "preset", label: "Apply preset" },
  { id: "preseteditor", label: "Preset editor" },
  { id: "markdown", label: "Markdown open" },
  { id: "rename", label: "Rename popup" },
  { id: "settings", label: "Settings window" },
];

// glyphs by name from the canonical set (icons/*.svg via <Icon name>)
const ic = {
  search: <Icon name="search" />,
  plus: <Icon name="plus" />,
  term: <Icon name="terminal" />,
  md: <Icon name="markdown" />,
  split: <Icon name="split" />,
  settings: <Icon name="settings" />,
  rocket: <Icon name="rocket" />,
  x: <Icon name="close" />,
  refresh: <Icon name="refresh" />,
  back: <Icon name="chevronLeft" />,
  tools: <Icon name="tools" />,
  port: <Icon name="port" />,
  remote: <Icon name="remote" />,
  swap: <Icon name="swap" />,
  file: <Icon name="file" />,
  edit: <Icon name="edit" />,
  layers: <Icon name="layers" />,
  download: <Icon name="download" />,
  funnel: <Icon name="filter" />,
  mouse: <Icon name="mouse" />,
  check: <Icon name="check" />,
  warn: <Icon name="alertTriangle" />,
  folder: <Icon name="folder" />,
};

// a faux app backdrop so the scrim reads correctly
function Backdrop({ height = 360, blur = false, children }) {
  return (
    <div style={{ position: "relative", width: "100%", height, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "1px solid var(--tasty-border-default)", background: "var(--tasty-bg-app)" }}>
      {/* hint of app chrome behind */}
      <div style={{ position: "absolute", inset: 0, padding: 14, opacity: 0.5 }}>
        <div style={{ height: 28, background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)", marginBottom: 10 }} />
        <div style={{ display: "flex", gap: 10, height: "calc(100% - 38px)" }}>
          <div style={{ width: 120, background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)" }} />
          <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", borderRadius: "var(--tasty-radius)" }} />
        </div>
      </div>
      {/* scrim — flat fill by default; the 1px blur (documented recipe) is opt-in
          so only the canonical Scrim spec pays the per-frame compositing cost */}
      <div style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,.5)", backdropFilter: blur ? "blur(1px)" : undefined,
        display: "flex", alignItems: children.props.align === "top" ? "flex-start" : "center", justifyContent: "center",
        paddingTop: children.props.align === "top" ? 40 : 0 }}>
        {children}
      </div>
    </div>
  );
}

// opaque: popup over an opaque-mode WebView page (2026-10-21) — square corner, no shadow, border kept
// ground: the search field is ground="raised" (2026-10-27). focused/value drive static state specimens.
function PaletteFrame({ opaque = false, focused = false, value = "", rows = 4 } = {}) {
  const items = [
    { label: "New Terminal", sc: "Ctrl+T", icon: ic.term, active: true },
    { label: "Split Pane Vertical", sc: "Ctrl+D", icon: ic.split },
    { label: "Toggle Theme (Mocha / Latte)", sc: "", icon: <Icon name="theme" /> },
    { label: "Settings", sc: "Ctrl+,", icon: ic.settings },
  ];
  return (
    <div align="top" style={{ width: 480, background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: opaque ? "var(--tasty-webview-opaque-radius)" : "var(--tasty-radius)", overflow: "hidden", boxShadow: opaque ? "var(--tasty-webview-opaque-shadow)" : "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: 10, borderBottom: "1px solid var(--tasty-separator)" }}>
        <Input block ground="raised" focused={focused} icon={ic.search} placeholder="Type to search commands…" defaultValue={value} />
      </div>
      <div style={{ padding: 6 }}>
        {items.slice(0, rows).map((i) => (
          <MenuItem key={i.label} label={i.label} icon={i.icon} active={i.active}
            shortcut={i.sc ? <Kbd keys={i.sc} /> : null} />
        ))}
      </div>
      <div style={{ display: "flex", gap: 14, padding: "8px 12px", borderTop: "1px solid var(--tasty-separator)",
        fontFamily: "var(--tasty-font-mono)", fontSize: 10.5, color: "var(--tasty-text-muted-raised)" }}>
        <span>↑↓ navigate</span><span>↵ run</span><span>esc close</span>
      </div>
    </div>
  );
}

function ApprovalFrame() {
  return (
    <div style={{ width: 440, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-header-pad-y) var(--tasty-transfer-pad-x)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--tasty-accent-agent)" }} />
        <span style={{ fontSize: 14, fontWeight: 600 }}>Approve agent action</span>
        <Tag variant="agent" style={{ marginLeft: "auto" }}>agent</Tag>
      </div>
      <div style={{ padding: "var(--tasty-transfer-pad-x)", display: "flex", flexDirection: "column", gap: "var(--tasty-transfer-body-gap)" }}>
        <p style={{ margin: 0, fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>
          The agent <b style={{ color: "var(--tasty-text-primary)" }}>ai-review</b> wants to run a command in <span className="ic">s_01HXK9</span>:
        </p>
        <pre style={{ margin: 0, background: "var(--tasty-surface-terminal-focused-bg)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)",
          padding: "10px 12px", fontFamily: "var(--tasty-font-mono)", fontSize: 12.5, color: "var(--tasty-text-primary)", overflow: "auto" }}>
git push --force origin main</pre>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          <Tag variant="danger" dot>destructive</Tag><Tag>fs:write</Tag><Tag>net</Tag>
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost">Deny</Button>
        <Button variant="secondary">Allow once</Button>
        <Button variant="agent">Always allow</Button>
      </div>
    </div>
  );
}

function RenameFrame() {
  return (
    <div style={{ width: 360, background: "var(--tasty-dialog-bg)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: "14px 14px 0" }}>
        <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 2 }}>Rename workspace</div>
        <div style={{ fontSize: 12, color: "var(--tasty-text-muted)", marginBottom: 12 }}>Press <Kbd keys="↵" /> to confirm, <Kbd keys="Esc" /> to cancel.</div>
        <Input block autoFocus defaultValue="tasty-core" />
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14 }}>
        <Button variant="ghost">Cancel</Button>
        <Button variant="primary">Rename</Button>
      </div>
    </div>
  );
}

function SettingsFrame() {
  const L1 = ["General", "Appearance", "Keybindings", "Plugins"];
  const L2 = ["Theme", "General", "Terminal"];
  return (
    <div style={{ width: 620, height: 380, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      {/* L1 top tabs */}
      <div style={{ display: "flex", alignItems: "center", height: 44, flex: "none", padding: "0 12px", gap: 2,
        borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
        <span style={{ fontSize: 14, fontWeight: 700 }}>Settings</span>
        <span style={{ width: 1, height: 20, background: "var(--tasty-separator)", margin: "0 14px 0 6px" }} />
        {L1.map((t, i) => (
          <span key={t} style={{ height: 43, padding: "0 13px", display: "inline-flex", alignItems: "center", fontSize: 13,
            color: i === 1 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: i === 1 ? 600 : 400,
            borderBottom: i === 1 ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{t}</span>
        ))}
      </div>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        {/* L2 sidebar */}
        <div style={{ width: 168, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)" }}>
          <div style={{ padding: 8, borderBottom: "1px solid var(--tasty-separator)" }}><Input block icon={ic.search} placeholder="Filter…" /></div>
          <div style={{ padding: 6 }}>
            {L2.map((s, i) => (
              <div key={s} style={{ padding: "5px 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                color: i === 0 ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", background: i === 0 ? "var(--tasty-surface-active)" : "transparent" }}>{s}</div>
            ))}
          </div>
        </div>
        {/* content */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
          <div style={{ flex: 1, padding: 18, display: "flex", flexDirection: "column", gap: 12 }}>
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Theme preset</div>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
              {[["Catppuccin Mocha", ["#11111b", "#1e1e2e", "#89b4fa", "#cba6f7", "#a6e3a1"], true],
                ["Catppuccin Latte", ["#dce0e8", "#eff1f5", "#1e66f5", "#8839ef", "#40a02b"], false]].map(([label, cols, on]) => (
                <div key={label} style={{ border: on ? "1px solid var(--tasty-accent-primary)" : "1px solid var(--tasty-border-default)",
                  borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: on ? "0 0 0 1px var(--tasty-accent-primary)" : "none" }}>
                  <div style={{ display: "flex", height: 34 }}>{cols.map((c, i) => <div key={i} style={{ flex: 1, background: c }} />)}</div>
                  <div style={{ padding: "6px 9px", background: "var(--tasty-bg-panel)", fontSize: 12 }}>{label}</div>
                </div>
              ))}
            </div>
            <p style={{ fontSize: 12, color: "var(--tasty-text-muted)", margin: 0, lineHeight: 1.5 }}>Selecting a preset resets all surface colors.</p>
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
            <Button variant="ghost">Cancel</Button><Button variant="primary">Save</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Settings › General › Overlay — toast duration: the settings number shape
// (mono Input · static "s" suffix · clamp on commit). DragValue retired 2026-10-07.
// Name kept (ToastDragValue) so existing imports keep resolving.
function ToastDragValue({ state = "rest", v = "2.0" }) {
  const bad = state === "invalid";
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 10 }}>
      <span style={{ flex: "none", width: "var(--tasty-field-width-xs)" }}>
        <Input block defaultValue={bad ? "14" : v} aria-invalid={bad || undefined}
          style={{ fontFamily: "var(--tasty-font-mono)", textAlign: "right", ...(bad ? { borderColor: "var(--tasty-accent-danger)" } : state === "editing" ? { borderColor: "var(--tasty-border-focus)" } : null) }} />
      </span>
      <span style={{ flex: "none", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>s</span>
    </span>
  );
}
const ToastDurationField = ToastDragValue;

function SettingsGeneralOverlayFrame() {
  const L1 = ["General", "Appearance", "Keybindings", "Plugins"];
  const L2 = ["General", "Notifications", "Accessibility", "Overlay"];
  const active = 3;
  return (
    <div style={{ width: 620, height: 380, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", height: 44, flex: "none", padding: "0 12px", gap: 2,
        borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
        <span style={{ fontSize: 14, fontWeight: 700 }}>Settings</span>
        <span style={{ width: 1, height: 20, background: "var(--tasty-separator)", margin: "0 14px 0 6px" }} />
        {L1.map((t, i) => (
          <span key={t} style={{ height: 43, padding: "0 13px", display: "inline-flex", alignItems: "center", fontSize: 13,
            color: i === 0 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: i === 0 ? 600 : 400,
            borderBottom: i === 0 ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{t}</span>
        ))}
      </div>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <div style={{ width: 168, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)" }}>
          <div style={{ padding: 8, borderBottom: "1px solid var(--tasty-separator)" }}><Input block icon={ic.search} placeholder="Filter…" /></div>
          <div style={{ padding: 6 }}>
            {L2.map((s, i) => (
              <div key={s} style={{ padding: "5px 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                color: i === active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", background: i === active ? "var(--tasty-surface-active)" : "transparent" }}>{s}</div>
            ))}
          </div>
        </div>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
          <div style={{ flex: 1, padding: 18, display: "flex", flexDirection: "column", gap: 12 }}>
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Toast</div>
            <div style={{ display: "grid", gridTemplateColumns: "180px 1fr", alignItems: "center" }}>
              <span style={{ fontSize: 13 }}>Toast duration</span>
              <span><ToastDragValue /></span>
            </div>
            <p style={{ fontSize: 12, color: "var(--tasty-text-muted)", margin: 0, lineHeight: 1.5 }}>How long a toast stays on screen before it auto-dismisses. 1.0–10.0 seconds, in steps of 0.5.</p>
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
            <Button variant="ghost">Cancel</Button><Button variant="primary">Save</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Settings › General › Remote transfer — 5th L2 subtab (dir + max_mb) ──
function SettingsRemoteTransferFrame() {
  const L1 = ["General", "Appearance", "Keybindings", "Plugins"];
  const L2 = ["General", "Notifications", "Accessibility", "Overlay", "Remote transfer"];
  const active = 4;
  return (
    <div style={{ width: 620, height: 380, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", height: 44, flex: "none", padding: "0 12px", gap: 2,
        borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
        <span style={{ fontSize: 14, fontWeight: 700 }}>Settings</span>
        <span style={{ width: 1, height: 20, background: "var(--tasty-separator)", margin: "0 14px 0 6px" }} />
        {L1.map((t, i) => (
          <span key={t} style={{ height: 43, padding: "0 13px", display: "inline-flex", alignItems: "center", fontSize: 13,
            color: i === 0 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: i === 0 ? 600 : 400,
            borderBottom: i === 0 ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{t}</span>
        ))}
      </div>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <div style={{ width: 168, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)" }}>
          <div style={{ padding: 8, borderBottom: "1px solid var(--tasty-separator)" }}><Input block icon={ic.search} placeholder="Filter…" /></div>
          <div style={{ padding: 6 }}>
            {L2.map((s, i) => (
              <div key={s} style={{ padding: "5px 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                color: i === active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", background: i === active ? "var(--tasty-surface-active)" : "transparent" }}>{s}</div>
            ))}
          </div>
        </div>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
          <div style={{ flex: 1, padding: 18, display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Received files</div>
            <div style={{ display: "grid", gridTemplateColumns: "150px 1fr", alignItems: "center", gap: 12, minHeight: "var(--tasty-settings-row-min-height)" }}>
              <span style={{ fontSize: 13 }}>Save folder</span>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Input block mono defaultValue="~/.tasty/transfers/" style={{ flex: 1, minWidth: 0 }} />
                <Button variant="secondary" size="sm" leadingIcon={ic.folder} style={{ flex: "none" }}>Browse…</Button>
              </div>
            </div>
            <p style={{ fontSize: 12, color: "var(--tasty-text-muted)", margin: 0, lineHeight: 1.5 }}>Where files received from a remote workspace are saved.</p>
            <div style={{ borderTop: "1px solid var(--tasty-separator)" }} />
            <div style={{ display: "grid", gridTemplateColumns: "150px 1fr", alignItems: "center", gap: 12, minHeight: "var(--tasty-settings-row-min-height)" }}>
              <span style={{ fontSize: 13 }}>Maximum size</span>
              <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                <Input mono defaultValue="500" style={{ width: 88 }} />
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>MiB</span>
              </span>
            </div>
            <p style={{ fontSize: 12, color: "var(--tasty-text-muted)", margin: 0, lineHeight: 1.5 }}>Total the folder may hold. A transfer that would push it past this limit is rejected before it starts.</p>
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
            <Button variant="ghost">Cancel</Button><Button variant="primary">Save</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Remote transfer — progress popup (PopupDef, auto-closes on completion) ──
function TransferProgressFrame({ name = "sprint-42-demo.mp4", pct = 27, done = "34.6 MiB", total = "128.0 MiB", rate = "2.1 MiB/s" }) {
  return (
    <div style={{ width: "var(--tasty-transfer-popup-width)", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-header-pad-y) var(--tasty-transfer-pad-x)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.download}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Receiving file</span>
        <span style={{ marginLeft: "auto", fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>{pct}%</span>
      </div>
      <div style={{ padding: "var(--tasty-transfer-pad-x)", display: "flex", flexDirection: "column", gap: "var(--tasty-transfer-body-gap)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
          <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{ic.file}</span>
          <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            fontFamily: "var(--tasty-font-mono)", fontSize: 13, color: "var(--tasty-text-primary)" }}>{name}</span>
        </div>
        <div style={{ height: "var(--tasty-progress-height)", background: "var(--tasty-progress-track-bg)", borderRadius: "var(--tasty-progress-radius)", overflow: "hidden" }}>
          <div style={{ width: pct + "%", height: "100%", background: "var(--tasty-progress-fill-bg)" }} />
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-muted)" }}>
          <span>{done} / {total}</span><span>{rate}</span>
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost">Cancel</Button>
      </div>
    </div>
  );
}

// ── Remote transfer — error / rejected popup ──
function TransferErrorFrame({ retry = false, name = "sprint-42-demo.mp4", reason = "capacity exceeded — transfers folder is at its 500 MiB limit" }) {
  return (
    <div style={{ width: "var(--tasty-transfer-popup-width)", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-header-pad-y) var(--tasty-transfer-pad-x)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-accent-danger-raised)" }}>{ic.warn}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Transfer failed</span>
      </div>
      <div style={{ padding: "var(--tasty-transfer-pad-x)", display: "flex", flexDirection: "column", gap: "var(--tasty-transfer-body-gap)" }}>
        <p style={{ margin: 0, fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>
          <b style={{ color: "var(--tasty-text-primary)", fontFamily: "var(--tasty-font-mono)", fontWeight: 600 }}>{name}</b> could not be received.
        </p>
        <div style={{ background: "var(--tasty-bg-app)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)",
          padding: "var(--tasty-transfer-well-pad-y) var(--tasty-transfer-well-pad-x)", fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-accent-danger-raised)" }}>{reason}</div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-transfer-footer-pad-y) var(--tasty-transfer-pad-x)", borderTop: "1px solid var(--tasty-separator)" }}>
        {retry ? <Button variant="ghost">Dismiss</Button> : null}
        {retry ? <Button variant="secondary">Retry</Button> : <Button variant="secondary">Dismiss</Button>}
      </div>
    </div>
  );
}

// ── Tools menu — 160px, anchored above the sidebar Tools button, no scrim ──
function ToolsMenuFrame() {
  const builtin = ["Command palette…", "Listening ports…", "Remote connections…", "Presets", "Tutorial…", "Task DAGs", "Open File…"];
  const plugin = ["Clipboard Viewer", "Git"];
  return (
    <div style={{ position: "relative", width: "100%", height: 400, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "1px solid var(--tasty-border-default)", background: "var(--tasty-bg-app)" }}>
      {/* faux sidebar with a Tools button at the bottom */}
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: 150, background: "var(--tasty-bg-sidebar)",
        borderRight: "1px solid var(--tasty-separator)", display: "flex", flexDirection: "column", justifyContent: "flex-end", padding: 8 }}>
        <Button variant="ghost" size="sm" block ground="active" leadingIcon={ic.tools} style={{ justifyContent: "flex-start", background: "var(--tasty-surface-active)" }}>Tools</Button>
      </div>
      {/* the menu, anchored above the button */}
      <div role="menu" aria-label="Tools" style={{ position: "absolute", left: 12, bottom: 52, width: "max-content", minWidth: "var(--tasty-tools-menu-min-width)", maxWidth: "var(--tasty-tools-menu-max-width)", boxSizing: "border-box", whiteSpace: "nowrap",
        background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
        borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-popup-content-margin)", boxShadow: "var(--tasty-shadow-popover)" }}>
        {builtin.map((l) => <MenuItem key={l} label={l} />)}
        <MenuItem separator />
        {plugin.map((l) => <MenuItem key={l} label={l} />)}
      </div>
    </div>
  );
}

// ── Listening ports — favorites section (design SoT: the popup's only WRITING
// control). Star = accent-warning gold when registered, muted outline when not;
// identity is (addr, port). The section is bounded — caption 22 + list capped at
// --tasty-port-favorites-max-height (112 = 5 rows) — and ignores the search box,
// the scope checkbox and the sort: it always reports the FULL system scan.
function PortStarG({ on }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", flex: "none",
      width: "var(--tasty-control-height-tree)", height: "var(--tasty-control-height-tree)", borderRadius: "var(--tasty-radius-sm)",
      color: on ? "var(--tasty-port-star-on)" : "var(--tasty-port-star-off)" }}>
      <Icon name={on ? "starFill" : "star"} size="var(--tasty-icon-size-sm)" />
    </span>
  );
}

function PortsFavoritesG({ favorites = "mixed" }) {
  const sets = {
    mixed: [["127.0.0.1:5173", "vite · 48990 · Project A", "LISTEN"],
            ["0.0.0.0:8080", "tasty-agent · 50321 · Project B", "LISTEN"],
            ["127.0.0.1:5432", "postgres · 1192", "LISTEN"],
            ["127.0.0.1:9999", "not running", "NONE"]],
    scrolling: [["127.0.0.1:3000", "node · 48213 · Project A", "LISTEN"],
            ["127.0.0.1:5173", "vite · 48990 · Project A", "LISTEN"],
            ["127.0.0.1:5432", "postgres · 1192", "LISTEN"],
            ["127.0.0.1:6379", "redis-server · 1456", "LISTEN"],
            ["0.0.0.0:8080", "tasty-agent · 50321 · Project B", "LISTEN"],
            ["127.0.0.1:9229", "node · 48213 · Project A", "CLOSE_WAIT"],
            ["127.0.0.1:9999", "not running", "NONE"]],
    empty: [],
  };
  const rows = sets[favorites] || sets.mixed;
  const cap = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };
  return (
    <div style={{ flex: "none", background: "var(--tasty-port-favorites-bg)",
      borderBottom: "1px solid var(--tasty-port-favorites-border)" }}>
      <div style={{ display: "flex", alignItems: "center", height: "var(--tasty-control-height-tree)", padding: "0 14px" }}>
        <span style={{ ...cap, textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)" }}>
          Favorites{rows.length > 0 && ` · ${rows.length}`}
        </span>
        <div style={{ flex: 1 }} />
        <span style={{ ...cap, color: "var(--tasty-text-placeholder)" }}>system-wide</span>
      </div>
      {rows.length === 0 ? (
        <div style={{ display: "flex", alignItems: "center", gap: 4, height: "var(--tasty-control-height-tree)", padding: "0 14px", color: "var(--tasty-text-muted)" }}>
          <span style={{ display: "inline-flex", flex: "none" }}><Icon name="star" size="var(--tasty-icon-size-xs)" /></span>
          <span style={{ fontSize: 12 }}>No favorites yet</span>
          <span style={{ fontSize: 12, color: "var(--tasty-text-placeholder)" }}>— click a star in the list below to pin a port.</span>
        </div>
      ) : (
        <div className="tasty-scroll" style={{ maxHeight: "var(--tasty-port-favorites-max-height)", overflowY: "auto" }}>
          {rows.map(([addr, meta, state]) => (
            <div key={addr} style={{ display: "flex", alignItems: "center", height: "var(--tasty-port-favorites-row-height)", padding: "0 14px 0 0" }}>
              <span style={{ width: "var(--tasty-port-star-col-width)", display: "inline-flex", justifyContent: "center", flex: "none" }}>
                <PortStarG on />
              </span>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)", flex: "none" }}>{addr}</span>
              <span style={{ flex: 1, minWidth: 12, paddingLeft: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12, color: "var(--tasty-text-muted)" }}>{meta}</span>
              <span style={{ flex: "none", display: "inline-flex", justifyContent: "flex-end", minWidth: 112 }}>
                {state === "NONE"
                  ? <StatusDot status="idle" label="NONE" />
                  : <StatusDot status={state === "LISTEN" ? "running" : "waiting"} pulse={state === "LISTEN"} label={state} />}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Listening ports — 660×520 table popup built on the Table component ──
// Shared with the "Process column" specimen (overlays-windows.jsx) so both draw the same columns.
const PortsDash = () => <span style={{ color: "var(--tasty-text-muted)" }}>—</span>;
const PORTS_COLUMNS = [
  { key: "fav", header: "", tight: true, width: "var(--tasty-port-star-col-width)",
    render: (_v, row) => <PortStarG on={row.port === 5173 || row.port === 8080} /> },
  { key: "port", header: "Port", align: "right", mono: true, sortable: true, width: 72 },
  { key: "proto", header: "Proto", mono: true, width: 64 },
  // 2026-10-08 (batch 8) — Address is pinned at its 140 floor; Process (width 100%) takes ALL spare width.
  { key: "addr", header: "Address", mono: true, sortable: true, width: "var(--tasty-port-addr-col-min-width)" },
  { key: "proc", header: "Process", strong: true, sortable: true, width: "100%", minWidth: "var(--tasty-port-process-col-min-width)",
    render: (v, row) => (<span style={{ display: "inline-flex", alignItems: "center", gap: 8, minWidth: 0 }}><span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{v}</span><Tag>{row.pid}</Tag></span>) },
  { key: "ws", header: "Workspace", width: 104, render: (v) => v || <PortsDash /> },
  { key: "state", header: "State", width: 132,
    render: (v) => <StatusDot status={v === "LISTEN" ? "running" : "waiting"} pulse={v === "LISTEN"} label={v} /> },
];
function PortsFrame({ favorites = "mixed" }) {
  const rows = [
    { port: 3000, proto: "tcp", addr: "127.0.0.1", proc: "node", pid: 48213, ws: "Project A", tab: "server", state: "LISTEN" },
    { port: 5173, proto: "tcp", addr: "127.0.0.1", proc: "vite", pid: 48990, ws: "Project A", tab: "dev", state: "LISTEN" },
    { port: 8080, proto: "tcp", addr: "0.0.0.0", proc: "tasty-agent", pid: 50321, ws: "Project B", tab: "agent", state: "LISTEN" },
    { port: 8443, proto: "tcp6", addr: "::", proc: "tasty-agent", pid: 50321, ws: "Project B", tab: "agent", state: "LISTEN" },
    { port: 9229, proto: "tcp", addr: "127.0.0.1", proc: "node", pid: 48213, ws: "Project A", tab: "server", state: "CLOSE_WAIT" },
  ];
  const columns = PORTS_COLUMNS;
  return (
    <div style={{ width: "100%", maxWidth: 640, height: 460, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 14px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.port}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Listening ports</span>
        <Tag variant="accent">5 listening</Tag>
        <div style={{ flex: 1 }} />
        <Input icon={ic.search} placeholder="Filter…" style={{ width: 160 }} />
        <IconButton aria-label="Refresh">{ic.refresh}</IconButton>
        <IconButton aria-label="Close">{ic.x}</IconButton>
      </div>
      <div style={{ display: "flex", alignItems: "center", padding: "8px 14px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <Checkbox label="Show all (system-wide)" />
      </div>
      <PortsFavoritesG favorites={favorites} />
      <div style={{ overflow: "auto", flex: 1, minHeight: 0 }}>
        <Table columns={columns} rows={rows} rowKey="port" selectedKey={8080} />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 14px", flex: "none", borderTop: "1px solid var(--tasty-separator)" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>5 of 5 ports</span>
        <div style={{ flex: 1 }} />
        <Button variant="ghost">Copy address</Button>
        <Button variant="secondary">Close</Button>
      </div>
    </div>
  );
}

// ── Remote connections — 520×460, three top tabs, list view ──
function RemoteFrame({ tab = "profiles", ssh = "hosts" }) {
  const Caption = ({ children }) => <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)" }}>{children}</span>;
  const ProfileRow = ({ name, label, type, target, passkey, detecting }) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "10px 4px", borderBottom: "1px solid var(--tasty-separator)" }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600 }}>{name}{label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({label})</span>}</span>
          <Tag>{type}</Tag>
        </div>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{target}</span>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <Caption>passkey: {passkey || "—"}</Caption>
          {detecting && <span style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 11, color: "var(--tasty-text-muted)" }}><Spinner size={12} /> detecting…</span>}
        </div>
      </div>
      <div style={{ display: "flex", gap: 1, flex: "none" }}>
        <IconButton size="sm" aria-label="Re-detect">{ic.refresh}</IconButton>
        <IconButton size="sm" aria-label="Edit">{ic.edit}</IconButton>
        <IconButton size="sm" aria-label="Delete"><Icon name="trash" /></IconButton>
      </div>
    </div>
  );
  const AttachRow = ({ name, label, mode, target, tasty, port, inactive, missing }) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "10px 4px", borderBottom: "1px solid var(--tasty-separator)" }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600, color: inactive ? "var(--tasty-text-muted)" : "var(--tasty-text-primary)" }}>{name}{label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({label})</span>}</span>
          <Tag>{mode}</Tag>
          {inactive && <span style={{ display: "inline-flex", alignItems: "center", gap: 3, height: 16, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
            fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, color: "var(--tasty-accent-warning-raised)",
            border: "1px solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
            background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>inactive</span>}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{target}</span>
          {missing && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-accent-warning-raised)" }}>profile missing</span>}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <Caption>tasty: {tasty}</Caption><Caption>port: {port}</Caption>
        </div>
      </div>
      <div style={{ display: "flex", gap: 1, flex: "none" }}>
        <IconButton size="sm" aria-label="Edit">{ic.edit}</IconButton>
        <IconButton size="sm" aria-label="Delete"><Icon name="trash" /></IconButton>
      </div>
    </div>
  );
  const TabBtn = ({ children, on }) => (
    <span style={{ height: 35, padding: "0 12px", display: "inline-flex", alignItems: "center", fontSize: 13,
      color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: on ? 600 : 400,
      borderBottom: on ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{children}</span>
  );
  return (
    <div style={{ width: 520, height: 440, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 14px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.remote}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Remote connections</span>
        <div style={{ flex: 1 }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      <div style={{ display: "flex", gap: 2, padding: "0 8px", flex: "none", borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
        <TabBtn on={tab === "profiles"}>Remote profiles</TabBtn><TabBtn on={tab === "attach"}>Attach</TabBtn><TabBtn on={tab === "passkeys"}>Passkeys</TabBtn>
      </div>
      {tab === "passkeys" ? (
        <>
          <div style={{ display: "flex", alignItems: "center", padding: "10px 14px 6px", flex: "none" }}>
            <Button variant="secondary" size="sm" leadingIcon={ic.plus}>Add passkey</Button>
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 14px 8px" }}>
            {[{ name: "ed25519-main", kind: "file", value: "~/.ssh/id_ed25519" },
              { name: "deploy-key-with-a-very-long-name", kind: "file", value: "/Users/hyunjun/Library/Application Support/tasty/keys/deploy/ci-runner/id_ed25519_deploy", revealed: true },
              { name: "vault-token", kind: "secret", value: "••••••••" },
              { name: "legacy", kind: "agent-x", value: "••••••••", unknown: true }].map((k) => (
              <div key={k.name} style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-xs)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
                    <span style={{ fontSize: 13, fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{k.name}</span>
                    {k.unknown
                      ? <span title="Unknown kind." style={{ flex: "none", display: "inline-flex", alignItems: "center", gap: 3, height: 16, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, color: "var(--tasty-accent-warning-raised)", border: "1px solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)", background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>{k.kind}</span>
                      : <span style={{ flex: "none", display: "inline-flex" }}><Tag>{k.kind}</Tag></span>}
                  </div>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{k.kind} · {k.revealed ? k.value : "••••••••"}</span>
                </div>
                <div style={{ display: "flex", gap: 1, flex: "none" }}>
                  <IconButton size="sm" active={!!k.revealed} aria-label={k.revealed ? "Hide value" : "Show value (local only)"}><Icon name={k.revealed ? "eyeOff" : "eye"} /></IconButton>
                  <IconButton size="sm" aria-label="Edit">{ic.edit}</IconButton>
                  <IconButton size="sm" aria-label="Delete"><Icon name="trash" /></IconButton>
                </div>
              </div>
            ))}
          </div>
        </>
      ) : tab === "attach" ? (
        <>
          <div style={{ display: "flex", alignItems: "center", padding: "10px 14px 6px", flex: "none" }}>
            <Button variant="secondary" size="sm" leadingIcon={ic.plus}>Add attach</Button>
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 14px 8px" }}>
            <AttachRow name="gb10" label="us-east" mode="profile" target="→ prod-web" tasty="tasty" port="auto" />
            <AttachRow name="edge-direct" mode="inline" target="root@edge.example.com" tasty="/opt/tasty/bin/tasty" port="file-unix" />
            <AttachRow name="legacy-attach" mode="profile" target="→ legacy-box" tasty="tasty" port="subcommand" inactive />
          </div>
        </>
      ) : (
      <>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "10px 14px 6px", flex: "none" }}>
        <Button variant="secondary" size="sm" leadingIcon={ic.plus}>Add profile</Button>
        <div style={{ position: "relative" }}>
          <Button variant="primary" size="sm" leadingIcon={ic.funnel}>Filter · 3/4</Button>
          <div role="dialog" aria-label="Filter by protocol" style={{ position: "absolute", top: "calc(100% + 4px)", right: 0, zIndex: 5, width: "var(--tasty-remote-filter-menu-width)",
            background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)",
            boxShadow: "var(--tasty-shadow-popover)", overflow: "hidden" }}>
            <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--tasty-separator)", fontFamily: "var(--tasty-font-mono)",
              fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Filter by protocol</div>
            <div style={{ padding: "8px 12px", display: "flex", flexDirection: "column", gap: 8 }}>
              {[["ssh", true, false], ["smb", true, false], ["http", true, false], ["snb", false, true]].map(([p, on, unknown]) => (
                <div key={p} style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Checkbox checked={on} readOnly label={p} />
                  {unknown && <span style={{ display: "inline-flex", alignItems: "center", gap: 3, height: 16, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
                    fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, color: "var(--tasty-accent-warning-raised)",
                    border: "1px solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
                    background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>unknown</span>}
                </div>
              ))}
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "8px 12px", borderTop: "1px solid var(--tasty-separator)" }}>
              <span style={{ fontSize: 11, color: "var(--tasty-accent-primary-raised)" }}>Select all</span>
              <span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-separator)" }}>·</span>
              <span style={{ fontSize: 11, color: "var(--tasty-accent-primary-raised)" }}>Deselect all</span>
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 12px", borderTop: "1px solid var(--tasty-separator)" }}>
              <Button variant="ghost" ground="raised" size="sm">Reset</Button>
              <Button variant="primary" size="sm">Apply</Button>
            </div>
          </div>
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 14px 8px" }}>
        <ProfileRow name="prod-web" label="us-east" type="ssh" target="deploy@10.0.4.12" passkey="ed25519-main" />
        <ProfileRow name="db-primary" type="ssh" target="postgres@db.internal:2222" passkey="" />
        <ProfileRow name="edge-cache" label="staging" type="ssh" target="root@edge.example.com" passkey="edge-pem" detecting />
        <ProfileRow name="media-nas" label="lab" type="smb" target="host=nas.local  share=media" passkey="nas-cred" />
        {ssh !== "none" && <LocalSshSection state={ssh} />}
      </div>
      </>
      )}
    </div>
  );
}

// ── R2 — local ~/.ssh/config hosts: a SECONDARY list under the profiles ──
// Settled shape: same scroll as the profile list (it is the same question —
// "which machine" — and a separate tab would hide it), but visually one tier
// down, so it can never be mistaken for a stored profile:
//   · a section header (11px uppercase label + mono source path + count), not a card
//   · two lines per row instead of three: alias, then user@host:port in mono
//   · no per-row icon buttons; ONE ghost "Add profile" action, right-aligned,
//     which is the existing import action (no new behaviour)
//   · a host already imported shows a muted "in profiles" Tag and no action
// Empty / failure are one muted line each — a missing ssh config is not an error.
// Static specimen of the kit component ui_kits/terminal/overlays/remote_tool.jsx › LocalSshSection (the definition).
function LocalSshSection({ state = "hosts" }) {
  const hosts = [
    { alias: "gb10", target: "maya@gb10.local:22" },
    { alias: "prod-web", target: "deploy@10.0.4.12:22", have: true },
    { alias: "eu-west-build-farm-bastion-01", target: "ci-runner@bastion-01.eu-west.build.example.com:2222" },
  ];
  return (
    <div style={{ marginTop: 10, paddingTop: 8, borderTop: "1px solid var(--tasty-border-frame)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6, padding: "2px 4px 6px" }}>
        <span style={{ fontSize: 11, letterSpacing: "0.06em", textTransform: "uppercase", color: "var(--tasty-text-secondary)" }}>From ssh config</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>~/.ssh/config</span>
        <div style={{ flex: 1 }} />
        {state === "hosts" && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{hosts.length}</span>}
      </div>
      {state === "empty" && <div style={{ padding: "2px 4px 6px", fontSize: 12, color: "var(--tasty-text-muted)" }}>No hosts in ~/.ssh/config.</div>}
      {state === "missing" && <div style={{ padding: "2px 4px 6px", fontSize: 12, color: "var(--tasty-text-muted)" }}>No ~/.ssh/config found.</div>}
      {state === "unreadable" && <div style={{ padding: "2px 4px 6px", fontSize: 12, color: "var(--tasty-text-muted)" }}>Can't read ~/.ssh/config. Check the file and its permissions.</div>}
      {state === "hosts" && hosts.map((h) => (
        <div key={h.alias} style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 4px", borderBottom: "1px solid var(--tasty-separator)" }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{h.alias}</div>
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{h.target}</div>
          </div>
          {h.have
            ? <Tag>in profiles</Tag>
            : <Button variant="ghost" size="sm" leadingIcon={ic.plus}>Add profile</Button>}
        </div>
      ))}
    </div>
  );
}

// ── ShellMock — the whole window at 1/1 scale, small: CSD titlebar · sidebar ·
// pane tab strip · two surfaces · status bar. Exists so a SCOPED scrim can be
// judged against everything it does and does not cover.
//   scope: "none" | "window" | "surface"   which rect the scrim dims
//   split: "v" (side by side) | "h" (stacked)
//   child: true → a Browse picker opened from the popup (two shells, ONE scrim)
function ShellMock({ scope = "none", split = "v", child = false, narrow = false, clamp = false }) {
  const W = narrow ? 560 : 760, H = 380;
  const surfaceScrim = scope === "surface";
  const Term = ({ lines, focused, children, label }) => (
    <div style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, overflow: "hidden",
      background: focused ? "var(--tasty-surface-terminal-focused-bg)" : "var(--tasty-surface-terminal-unfocused-bg)" }}>
      <div style={{ padding: "8px 10px", fontFamily: "var(--tasty-font-mono)", fontSize: 11, lineHeight: 1.6,
        color: focused ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)", whiteSpace: "pre" }}>{lines}</div>
      {children}
    </div>
  );
  const popup = (
    <div style={{ position: "relative", width: clamp ? 220 : 300, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-frame)",
      borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: child ? "visible" : "hidden" }}>
      <div style={{ padding: "10px 12px 8px", display: "flex", flexDirection: "column", gap: 6 }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>Open markdown file</div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>docs/architecture.md</div>
      </div>
      <div style={{ display: "flex", gap: 6, padding: "0 12px 10px" }}>
        <Button variant="secondary" size="sm">Browse…</Button>
        <div style={{ flex: 1 }} />
        <Button variant="ghost" size="sm">Cancel</Button>
        <Button variant="primary" size="sm">Open</Button>
      </div>
      {child && (
        <div style={{ position: "absolute", left: "50%", top: "50%", transform: "translate(-50%,-50%)", zIndex: 3,
          width: clamp ? 200 : 260, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-frame)",
          borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
          <div style={{ padding: "8px 10px", fontSize: 12, fontWeight: 600, borderBottom: "1px solid var(--tasty-separator)" }}>Open file</div>
          <div style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4 }}>
            {["configs", "docs", "README.md"].map((n, i) => (
              <div key={n} style={{ display: "flex", alignItems: "center", gap: 6, height: 20, fontSize: 12,
                color: i === 2 ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)" }}>
                <Icon name={i === 2 ? "file" : "folder"} size={12} />{n}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
  return (
    <div style={{ width: W, height: H, display: "flex", flexDirection: "column", overflow: "hidden",
      border: "1px solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-app)", position: "relative" }}>
      {/* titlebar */}
      <div style={{ flex: "none", height: 28, display: "flex", alignItems: "center", gap: 6, padding: "0 10px",
        background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>tasty — agents-prod</span>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* sidebar */}
        <div style={{ flex: "none", width: narrow ? 120 : 148, background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-border-frame)", padding: 8,
          display: "flex", flexDirection: "column", gap: 6 }}>
          {["agents-prod", "build", "notes"].map((n, i) => (
            <div key={n} style={{ display: "flex", alignItems: "center", gap: 6, height: 22, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
              background: i === 0 ? "var(--tasty-surface-active)" : "transparent", fontSize: 12,
              color: i === 0 ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>
              <span style={{ width: "var(--tasty-statusbar-dot-size)", height: "var(--tasty-statusbar-dot-size)", borderRadius: "var(--tasty-radius-pill)",
                background: i === 0 ? "var(--tasty-status-dot-success)" : "var(--tasty-status-dot-idle)" }} />{n}
            </div>
          ))}
        </div>
        {/* work area */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ flex: "none", height: 24, display: "flex", alignItems: "stretch", background: "var(--tasty-bg-panel)", borderBottom: "1px solid var(--tasty-separator)" }}>
            {["zsh", "cargo watch"].map((t, i) => (
              <span key={t} style={{ display: "inline-flex", alignItems: "center", gap: 5, padding: "0 10px", fontSize: 11,
                color: i === 0 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
                borderBottom: i === 0 ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent",
                borderRight: "1px solid var(--tasty-separator)" }}>
                {t}
                {i === 1 && <span style={{ width: "var(--tasty-tab-dot-size)", height: "var(--tasty-tab-dot-size)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-status-dot-success)" }} />}
              </span>
            ))}
          </div>
          <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: split === "v" ? "row" : "column" }}>
            <div style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, display: "flex" }}>
              <Term focused lines={"$ tail -f deploy.log\n[12:04] applied 3 manifests\n[12:05] waiting for rollout…"} />
              {/* the scrim — surface scope: this rect, nothing else */}
              {surfaceScrim && <div style={{ position: "absolute", inset: 0, background: "var(--tasty-scrim-bg)", zIndex: 1 }} />}
              {scope === "surface" && (
                <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", zIndex: 2, padding: 8 }}>{popup}</div>
              )}
            </div>
            <div style={{ flex: "none", ...(split === "v" ? { width: 1 } : { height: 1 }), background: "var(--tasty-border-frame)" }} />
            <Term lines={"$ git status\nOn branch feat/dag-detail\nnothing to commit"} />
          </div>
        </div>
      </div>
      {/* status bar */}
      <div style={{ flex: "none", height: "var(--tasty-status-bar-height)", display: "flex", alignItems: "center", gap: 10, padding: "0 10px",
        background: "var(--tasty-bg-sidebar)", borderTop: "1px solid var(--tasty-separator)", fontSize: 11, color: "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}><Icon name="gitBranch" size={12} />feat/dag-detail</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)" }}>s3·p1</span>
        <div style={{ flex: 1 }} />
        <Kbd keys="Cmd K" />
        <span style={{ display: "inline-flex", color: "var(--tasty-statusbar-theme-glyph)" }}><Icon name="sun" size={12} /></span>
      </div>
      {/* window-scope scrim covers chrome too — drawn last, above everything */}
      {scope === "window" && (
        <>
          <div style={{ position: "absolute", inset: 0, background: "var(--tasty-scrim-bg)", zIndex: 4 }} />
          <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", zIndex: 5 }}>{popup}</div>
        </>
      )}
    </div>
  );
}

// ── Search bar — 360px headless popup floating top-right of the focused surface ──
// ground: the query field is ground="raised" (2026-10-27). Props drive the state specimens:
//   focused (static focus paint) · invalid (regex error, counter 0/0 red, .* on) · height of the faux terminal.
function SearchBarFrame({ value = "tasty", focused = false, invalid = false, height = 240 } = {}) {
  const Toggle = ({ label, active }) => (
    <IconButton size="sm" active={active} aria-label={label}>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, fontWeight: 600,
        color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted-raised)" }}>{label}</span>
    </IconButton>
  );
  return (
    <div style={{ position: "relative", width: "100%", height, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "1px solid var(--tasty-border-default)", background: "var(--tasty-surface-terminal-focused-bg)" }}>
      {/* faux terminal text */}
      <div style={{ padding: "12px 14px", fontFamily: "var(--tasty-font-mono)", fontSize: 12.5, lineHeight: 1.7, color: "var(--tasty-surface-terminal-focused-fg)" }}>
        <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> $ cargo build --release</div>
        <div>   Compiling tasty-core v0.5.0</div>
        <div>   Compiling <mark style={{ background: "var(--tasty-accent-warning)", color: "#000" }}>tasty</mark>-ui-widgets v0.5.0</div>
        <div>   Compiling <mark style={{ background: "var(--tasty-search-match-active-bg)", color: "var(--tasty-search-match-active-fg)" }}>tasty</mark>-gallery v0.5.0</div>
        <div>    Finished release [optimized] target(s)</div>
      </div>
      {/* the search bar */}
      <div role="search" style={{ position: "absolute", top: 8, right: 8, width: 360, maxWidth: "calc(100% - 16px)",
        display: "flex", alignItems: "center", gap: 4, padding: 4, background: "var(--tasty-surface-raised)",
        border: "var(--tasty-border-width) solid var(--tasty-search-bar-border)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-popover)" }}>
        <span style={{ flex: 1, minWidth: 60, display: "flex" }}><Input block ground="raised" focused={focused} invalid={invalid} defaultValue={value} /></span>
        <span style={{ flex: "none", width: 40, textAlign: "center", fontSize: 12, color: invalid ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-muted-raised)", fontVariantNumeric: "tabular-nums" }}>{invalid ? "0/0" : "2/3"}</span>
        <IconButton size="sm" aria-label="Previous"><Icon name="chevronUp" size={14} /></IconButton>
        <IconButton size="sm" aria-label="Next"><Icon name="chevronDown" size={14} /></IconButton>
        <Toggle label="Aa" /><Toggle label=".*" active={invalid} /><Toggle label="ab" active={!invalid} />
        <span style={{ width: 1, alignSelf: "stretch", margin: "0 2px", background: "var(--tasty-separator)" }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
    </div>
  );
}

// ── Mocha · Latte pair for ground specimens (2026-10-27) — Mocha inherits the page, Latte is scoped ──
function GroundThemes({ render }) {
  return (
    <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
      {[["Mocha", "mocha"], ["Latte", "latte"]].map(([l, t]) => (
        <div key={l} data-theme={t} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0 }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{l}</span>
          {render(l)}
        </div>
      ))}
    </div>
  );
}
function GroundCap({ children }) {
  return <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{children}</span>;
}

// ── File handler picker — "Open with…" list ──
// One frame, five states (default · recent · fallback · empty · long) driven by
// props so the implementing side can transcribe a single component.
//   state:   "default" | "recent" | "fallback" | "empty" | "long"
//   headless: true  = the frame draws its OWN header (recommended, path once)
//             false = common popup titlebar above it (path drawn twice)
//   footer:  "actions" (settled — Cancel / Open). "checkbox" / "disabled" exist
//            only for the F1 decision specimen; nothing ships them.
// F2/F3 — a row is rendered from the handler model ONLY:
//   { id, owner: "host" | "user" | "plugin", kind, name?, icon?, when? }
// There is no icon field and no display-name field in the model, so both are
// DERIVED, and a plugin may override either through its manifest:
//   icon — from the surface KIND the handler's action opens (FH_KIND_ICON);
//          an unknown / undeclared kind falls back to "file". A plugin-declared
//          icon must be a name in icons.json; anything else falls back the same way.
//   name — the id's LOCAL segment (after the last "/"), verbatim. A declared
//          name wins; an id with no "/" is shown whole.
// The second line always carries origin + the full id, so the id is visible
// exactly once per row and the name line never has to hold it.
const FH_KIND_ICON = { markdown: "markdown", html: "html", image: "image", directory: "folder",
  editor: "edit", pager: "terminal", terminal: "terminal", log: "listView", table: "columns", binary: "layers" };
const fhIcon = (h) => <Icon name={(h.icon && FH_KIND_ICON[h.kind] !== undefined ? h.icon : null) || FH_KIND_ICON[h.kind] || "file"} />;
const fhName = (h) => h.name || (h.id.includes("/") ? h.id.slice(h.id.lastIndexOf("/") + 1) : h.id);
const FH_ROWS = {
  suggested: [
    { id: "com.tasty.markdown/preview", owner: "host", kind: "markdown", name: "Markdown preview", dflt: true },
    { id: "com.tasty.text/editor", owner: "host", kind: "editor", name: "Text editor" },
    { id: "com.tasty.pager/less", owner: "host", kind: "pager", name: "Terminal (less)" },
    { id: "dev.git-helper.diff/viewer", owner: "plugin", kind: "markdown" },
  ],
  // T1–T3 — the relative-time vocabulary, one row per bucket. The `when` string
  // is the WHOLE vocabulary: six buckets, and past a week it stops counting and
  // shows the date. An ISO-style date is locale-neutral (no ko/ja string, no
  // plural rule), bounded at 10 chars, and sorts visually in a mono column.
  recent: [
    { id: "com.tasty.text/editor", owner: "host", kind: "editor", name: "Text editor", when: "just now" },
    { id: "com.tasty.log/viewer", owner: "host", kind: "log", name: "Log viewer", when: "12m ago" },
    { id: "dev.tabular.csv/table", owner: "plugin", kind: "table", name: "CSV table", when: "2h ago" },
    { id: "io.binview.hex/viewer", owner: "plugin", kind: "binary", when: "yesterday" },
    { id: "com.tasty.image/viewer", owner: "host", kind: "image", name: "Image viewer", when: "4d ago" },
    { id: "com.tasty.shell/editor-env", owner: "host", kind: "editor", name: "Open in $EDITOR", when: "2026-08-30" },
  ],
  all: [
    { id: "com.tasty.text/editor", owner: "host", kind: "editor", name: "Text editor" },
    { id: "com.tasty.pager/less", owner: "host", kind: "pager", name: "Terminal (less)" },
    { id: "com.tasty.markdown/preview", owner: "host", kind: "markdown", name: "Markdown preview" },
    { id: "com.tasty.log/viewer", owner: "host", kind: "log", name: "Log viewer" },
    { id: "io.binview.hex/viewer", owner: "plugin", kind: "binary" },
  ],
  // one list mixing the three F2/F3 cases: declared name + icon (plugin),
  // nothing declared (host), and an id long enough to need elision
  mixed: [
    { id: "com.tasty.image/viewer", owner: "host", kind: "image" },
    { id: "dev.imgview.raster/preview", owner: "plugin", kind: "image", name: "Raster preview", icon: "image", dflt: true },
    { id: "com.tasty.text/editor", owner: "user", kind: "editor", name: "Text editor" },
    { id: "net.example.enterprise.documents.attachments/inline-preview-handler", owner: "plugin", kind: "html" },
  ],
};
const FH_LONG = [
  ...FH_ROWS.suggested,
  { id: "com.tasty.log/viewer", owner: "host", kind: "log", name: "Log viewer" },
  { id: "io.binview.hex/viewer", owner: "plugin", kind: "binary" },
  { id: "dev.imgview.raster/preview", owner: "plugin", kind: "image", name: "Image preview" },
  { id: "dev.tabular.csv/table", owner: "plugin", kind: "table", name: "CSV table" },
  { id: "com.tasty.shell/editor-env", owner: "host", kind: "editor", name: "Open in $EDITOR" },
  { id: "com.tasty.tree/reveal", owner: "host", kind: "directory", name: "Reveal in file tree" },
];

const FH_OWNER_WORD = { host: "built-in", user: "you", plugin: "plugin" };
function FhRow({ h, sel, dim }) {
  const plugin = h.owner === "plugin";
  // the id line elides at the FRONT: a reverse-DNS id's tail is what identifies
  // the handler, the vendor prefix is the repeated part (same rule as the path
  // in the header). Elided in the model, rendered LTR.
  const idText = h.id.length > 34 ? "…" + h.id.slice(-33) : h.id;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 10px", borderRadius: "var(--tasty-radius-sm)",
      background: sel ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: sel ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ display: "inline-flex", color: plugin ? "var(--tasty-accent-agent-raised)" : "var(--tasty-text-muted)", opacity: dim && !plugin ? 0.8 : 1 }}>{fhIcon(h)}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, color: "var(--tasty-text-primary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          fontFamily: h.name ? "var(--tasty-font-ui)" : "var(--tasty-font-mono)" }}>{fhName(h)}</div>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", whiteSpace: "nowrap", overflow: "hidden", display: "flex", gap: 5, minWidth: 0 }}>
          <span style={{ flex: "none", color: plugin ? "var(--tasty-accent-agent-raised)" : "var(--tasty-text-muted)" }}>{FH_OWNER_WORD[h.owner]}</span>
          <span data-ink-grade="none" data-ink-why="ornament" style={{ flex: "none", color: "var(--tasty-text-disabled)" }}>·</span>
          <span style={{ fontFamily: "var(--tasty-font-mono)", overflow: "hidden", textOverflow: "ellipsis" }}>{idText}</span>
          {h.when && <><span data-ink-grade="none" data-ink-why="ornament" style={{ flex: "none", color: "var(--tasty-text-disabled)" }}>·</span><span style={{ flex: "none", width: "var(--tasty-fh-when-width)" }}>{h.when}</span></>}
        </div>
      </div>
      {h.dflt && <Tag variant="accent">default</Tag>}
    </div>
  );
}

// group label — the one device that separates "suggested" from "recent" / "all"
function FhGroup({ label, count, caption, tone }) {
  return (
    <div style={{ padding: "8px 10px 4px" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
        <span style={{ fontSize: 11, letterSpacing: "0.06em", textTransform: "uppercase",
          color: tone === "attention" ? "var(--tasty-accent-attention-raised)" : "var(--tasty-text-secondary)" }}>{label}</span>
        {count != null && <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)" }}>{count}</span>}
      </div>
      {caption && <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginTop: 2 }}>{caption}</div>}
    </div>
  );
}

function FhHeader({ state, path }) {
  const fmt = state === "fallback" ? "unknown" : state === "empty" ? "markdown" : "markdown";
  return (
    <div style={{ padding: "14px 14px 10px", borderBottom: "1px solid var(--tasty-separator)", display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 14, fontWeight: 600, flex: 1 }}>Open file with…</span>
        {state === "fallback"
          ? <Tag>format unknown</Tag>
          : <Tag variant="accent">{fmt}</Tag>}
      </div>
      {/* long paths are truncated at the FRONT before render — the filename is the
          tail and matters most. Done in the model layer (already-elided literal),
          NOT with direction:rtl, which reorders the run and clips the wrong end.
          T5 — the cut is driven by the MEASURED line box (390 inside the 420
          frame: 420 − 1px border × 2 − 14px inset × 2), at a segment boundary:
          drop whole leading segments and prefix "…/" until the rest fits.
          65 mono chars at 11px is the derived cap where measuring is not
          available: 390 / 6 — D2Coding 11px is 5.56px nominal but each glyph ADVANCES
          a whole 6px once rounded (2026-10-06; was 70 / 5.5, which overran by ~30px). A single segment
          longer than the line is the one case that cuts mid-segment. */}
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)",
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "clip", minWidth: 0 }}>
        {path || (state === "long" || state === "fallback" ? "…/federation/screens.tsx" : "docs/architecture.md")}
      </span>
    </div>
  );
}

function FhTitlebar() {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, height: 28, padding: "0 6px 0 10px",
      background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
      <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)", flex: 1, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
        Open with — …/screens.tsx
      </span>
      <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.x}</span>
    </div>
  );
}

// F1 — SETTLED (b): the picker is a pure dispatcher. Every pick is one-time, so
// the footer is Cancel / Open and nothing else. The two rejected readings are
// kept renderable ONLY for the decision specimen (variant "checkbox" / "disabled");
// no shipping state uses them, and no frame passes them any more.
function FhFooter({ variant = "actions", canOpen = true }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "8px 14px", borderTop: "1px solid var(--tasty-separator)" }}>
      {variant === "checkbox" && <Checkbox label="Always open .md with this" defaultChecked />}
      {variant === "disabled" && (
        <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
          <Checkbox label="Always open .md with this" disabled />
          <Tag>soon</Tag>
        </span>
      )}
      <div style={{ flex: 1 }} />
      <Button variant="ghost">Cancel</Button>
      <Button variant="primary" disabled={!canOpen}>Open</Button>
    </div>
  );
}

function FileHandlerFrame({ state = "default", headless = true, footer = "actions", path }) {
  const long = state === "long";
  const rowsOf = (key) => (state === "mixed" ? FH_ROWS.mixed : key);
  const listStyle = { padding: 6, ...(long ? { maxHeight: 264, overflow: "auto" } : null) };
  return (
    <div style={{ width: 420, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      {!headless && <FhTitlebar />}
      <FhHeader state={state} path={path} />

      {state === "fallback" && (
        <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "8px 14px",
          background: "var(--tasty-surface-raised)", borderBottom: "1px solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-accent-attention-raised)", flex: "0 0 auto" }}>{ic.warn}</span>
          <span style={{ fontSize: 11, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>
            One-time choice — nothing is registered for this format. This screen shows again next time.
          </span>
        </div>
      )}

      {state === "empty" ? (
        <div style={{ padding: "32px 14px", display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-text-disabled)" }}>{ic.file}</span>
          <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)" }}>No handlers registered.</div>
          <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", textAlign: "center" }}>Register one in Settings › Handlers to open this file.</div>
          <Button variant="secondary" size="sm" style={{ marginTop: 4 }}>Register a handler in Settings</Button>
        </div>
      ) : (
        <div style={{ position: "relative" }}>
          <div style={listStyle}>
            {state === "fallback" ? (
              <>
                <FhGroup label="All handlers" count={FH_ROWS.all.length} caption="No handler matches this format." tone="attention" />
                {FH_ROWS.all.map((h, i) => <FhRow key={h.id} h={h} sel={i === 0} />)}
              </>
            ) : (
              <>
                <FhGroup label="Suggested" count={(long ? FH_LONG : rowsOf(FH_ROWS.suggested)).length} />
                {(long ? FH_LONG : rowsOf(FH_ROWS.suggested)).map((h, i) => <FhRow key={h.id} h={h} sel={i === 0} />)}
                {state === "recent" && (
                  <>
                    <div style={{ height: 1, background: "var(--tasty-separator)", margin: "8px 10px 0" }} />
                    <FhGroup label="Recent" count={FH_ROWS.recent.length} caption="Recently used — not matched to this format." />
                    {FH_ROWS.recent.map((h) => <FhRow key={h.id} h={h} dim />)}
                  </>
                )}
              </>
            )}
          </div>
          {long && (
            <div style={{ position: "absolute", left: 1, right: 1, bottom: 0, height: 20, pointerEvents: "none",
              background: "linear-gradient(to bottom, transparent, var(--tasty-bg-panel))" }} />
          )}
        </div>
      )}

      <FhFooter variant={footer} canOpen={state !== "empty"} />
    </div>
  );
}

// ── Apply preset — scope + preset list ──
function PresetFrame() {
  const Seg = ({ children, on }) => (
    <button style={{ appearance: "none", cursor: "pointer", border: 0, padding: "0 14px", height: 26,
      borderLeft: on ? 0 : "1px solid var(--tasty-border-default)", fontSize: 12,
      color: on ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)",
      background: on ? "var(--tasty-accent-primary)" : "transparent" }}>{children}</button>
  );
  const presets = [
    { name: "Dev split", meta: "2 panes · editor + shell", sel: true },
    { name: "Logs grid", meta: "4 panes · tail -f" },
    { name: "Single shell", meta: "1 pane · zsh" },
  ];
  return (
    <div style={{ width: 440, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: "14px 14px 12px", borderBottom: "1px solid var(--tasty-separator)", display: "flex", alignItems: "center", gap: 12 }}>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Apply preset</span>
        <div style={{ flex: 1 }} />
        <div style={{ display: "inline-flex", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
          <Seg on>Workspace</Seg><Seg>Tab</Seg><Seg>Pane</Seg>
        </div>
      </div>
      <div style={{ padding: 6 }}>
        {presets.map((p) => (
          <div key={p.name} style={{ display: "flex", alignItems: "center", gap: 10, padding: "9px 10px", borderRadius: "var(--tasty-radius-sm)",
            background: p.sel ? "var(--tasty-surface-active)" : "transparent", boxShadow: p.sel ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
            <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.layers}</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>{p.name}</div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)" }}>{p.meta}</div>
            </div>
          </div>
        ))}
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14, borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost">Cancel</Button><Button variant="primary">Apply to workspace</Button>
      </div>
    </div>
  );
}

// ── Markdown open — confirm preview vs raw ──
function MarkdownOpenFrame() {
  const Choice = ({ icon, title, sub, on }) => (
    <button style={{ appearance: "none", cursor: "pointer", textAlign: "left", flex: 1, display: "flex", flexDirection: "column", gap: 6,
      padding: 12, borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)",
      border: on ? "1px solid var(--tasty-accent-primary)" : "1px solid var(--tasty-border-default)",
      boxShadow: on ? "0 0 0 1px var(--tasty-accent-primary)" : "none" }}>
      <span style={{ display: "inline-flex", color: on ? "var(--tasty-accent-primary-raised)" : "var(--tasty-text-muted)" }}>{icon}</span>
      <span style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)" }}>{title}</span>
      <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>{sub}</span>
    </button>
  );
  return (
    <div style={{ width: 420, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: "14px 14px 0" }}>
        <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 2 }}>Open markdown file</div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 12 }}>README.md</div>
        <div style={{ display: "flex", gap: 10 }}>
          <Choice icon={ic.md} title="Rendered preview" sub="Formatted view with headings and links." on />
          <Choice icon={<Icon name="scriptFile" />} title="Raw text" sub="Edit the source in the editor surface." />
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14 }}>
        <Button variant="ghost">Cancel</Button><Button variant="primary">Open preview</Button>
      </div>
    </div>
  );
}

// ── Switch-number overlay — transient keycaps over tabs / workspaces ──
// Shown while the (rebindable) tab/workspace switch modifier is held; the
// keycap REPLACES each item's leading indicator. Reuses the Kbd keycap; the
// current item gets an accent-filled keycap. No scrim, no reflow.
function NumCap({ n, active }) {
  return (
    <span aria-hidden style={{ display: "inline-flex", alignItems: "center", justifyContent: "center",
      width: "var(--tasty-switch-overlay-size)", height: "var(--tasty-switch-overlay-size)", flex: "none",
      borderRadius: "var(--tasty-radius-sm)", borderStyle: "solid", borderWidth: "var(--tasty-border-width)",
      borderBottomWidth: "var(--tasty-switch-overlay-shadow-depth)",
      borderColor: active ? "var(--tasty-switch-overlay-active-bg)" : "var(--tasty-switch-overlay-border)",
      background: active ? "var(--tasty-switch-overlay-active-bg)" : "var(--tasty-switch-overlay-bg)",
      color: active ? "var(--tasty-switch-overlay-active-fg)" : "var(--tasty-switch-overlay-fg)",
      fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-kbd-font-size)",
      fontWeight: "var(--tasty-font-weight-medium)", lineHeight: 1 }}>{n}</span>
  );
}

function HeldLabel({ keys, children }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8, fontSize: 11, color: "var(--tasty-text-muted)" }}>
      <Kbd keys={keys} /><span>held — {children}</span>
    </div>
  );
}

function TabStripMock({ held }) {
  const tabs = [
    { n: "1", label: "server", icon: ic.term },
    { n: "2", label: "dev", icon: ic.term },
    { n: "3", label: "agent", icon: ic.term, active: true },
    { n: "4", label: "README.md", icon: ic.file },
  ];
  return (
    <div style={{ display: "flex", alignItems: "stretch", background: "var(--tasty-bg-sidebar)", width: "fit-content",
      border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      {tabs.map((t) => (
        <div key={t.n} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 28, padding: "0 12px",
          borderRight: "1px solid var(--tasty-separator)", whiteSpace: "nowrap", fontSize: 13,
          background: t.active ? "var(--tasty-bg-panel)" : "transparent",
          color: t.active ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
          boxShadow: t.active ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>
          <span style={{ display: "inline-flex", width: 16, height: 16, alignItems: "center", justifyContent: "center" }}>
            {held ? <NumCap n={t.n} active={t.active} /> : t.icon}
          </span>
          <span>{t.label}</span>
        </div>
      ))}
    </div>
  );
}

function WsRowMock({ n, name, sub, status, active, held }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm)",
      background: active ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ flex: "none", height: 18, display: "inline-flex", alignItems: "center" }}>
        {held ? <NumCap n={n} active={active} /> : <StatusDot status={status} pulse={status === "running" || status === "agent"} />}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 500, color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</div>
        {sub && <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginTop: 1 }}>{sub}</div>}
      </div>
    </div>
  );
}

function SidebarMock({ held }) {
  const rows = [
    { n: "1", name: "tasty-core", sub: "main · 2 tabs", status: "running" },
    { n: "2", name: "ai-review", sub: "agent", status: "agent", active: true },
    { n: "3", name: "infra", sub: "idle", status: "idle" },
    { n: "4", name: "docs-site", sub: "idle", status: "idle" },
  ];
  return (
    <div style={{ width: 188, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em",
        color: "var(--tasty-text-muted)", padding: "10px 8px 4px" }}>Workspaces</div>
      {rows.map((r, i) => (
        <React.Fragment key={r.n}>
          {i > 0 && <div style={{ height: 1, background: "var(--tasty-separator)", margin: "0 0 0 32px" }} />}
          <WsRowMock {...r} held={held} />
        </React.Fragment>
      ))}
    </div>
  );
}

function RailMock() {
  const items = [
    { n: "1", letter: "T", status: "running" },
    { n: "2", letter: "A", status: "agent", active: true },
    { n: "3", letter: "I", status: "idle" },
    { letter: "D", status: "idle", noNum: true },
  ];
  return (
    <div style={{ width: 44, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", padding: "8px 0", display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
      {items.map((it, i) => (
        <div key={i} style={{ width: 28, height: 28, borderRadius: "var(--tasty-radius)", display: "flex", alignItems: "center", justifyContent: "center",
          color: "var(--tasty-text-secondary)",
          background: it.active ? "var(--tasty-surface-active)" : "transparent",
          boxShadow: it.active ? "inset 0 0 0 1px var(--tasty-accent-primary)" : "none" }}>
          {it.noNum
            ? <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 13, fontWeight: 700 }}>{it.letter}</span>
            : <NumCap n={it.n} active={it.active} />}
        </div>
      ))}
    </div>
  );
}

// ── Category quick-switch — Ctrl+Shift held paints a keycap per CATEGORY ──
// A second switch AXIS layered on the folders feature. The two switch
// overlays are modifier-EXCLUSIVE, so they never paint at once:
//   Alt        = workspaces  (row status-dot → keycap)   [SidebarMock/RailMock]
//   Ctrl+Shift  = categories  (this overlay)
// Full sidebar: the keycap is RIGHT-aligned on the category HEADER row — the
// chevron is NOT replaced, because it is load-bearing for the auto-expand
// transition (a collapsed target rotates open on switch). Reserved `normal`
// (“Workspaces”) is category 1 and gets a slot like any other. Digits run
// 1–9 then 0 (10th); an 11th category onward gets no keycap. Workspace rows
// keep their status dots throughout — only headers change. Switching lands on
// that category's LAST-ACTIVE workspace (falls back to its first), shown with
// the ordinary active treatment (surface-active + 2px accent bar).
function CatSwitchSidebarMock({ held }) {
  const chev = (collapsed) => (
    <span style={{ display: "inline-flex", width: 12, justifyContent: "center", color: "var(--tasty-text-muted)" }}>
      <Icon name={collapsed ? "chevronRight" : "chevronDown"} size={13} />
    </span>
  );
  const Head = ({ n, label, collapsed, active }) => (
    <div style={{ display: "flex", alignItems: "center", gap: 4, padding: "4px 8px", marginTop: 8 }}>
      {chev(collapsed)}
      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".07em", color: "var(--tasty-text-muted)" }}>{label}</span>
      {held && <NumCap n={n} active={active} />}
    </div>
  );
  const Row = ({ name, status, active }) => (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "7px 9px",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ flex: "none", height: 18, display: "inline-flex", alignItems: "center" }}><StatusDot status={status} pulse={status === "running" || status === "agent"} /></span>
      <span style={{ flex: 1, minWidth: 0, fontSize: 13, color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
    </div>
  );
  const rule = <div style={{ height: 1, background: "var(--tasty-separator)", margin: "0 0 0 32px" }} />;
  return (
    <div style={{ width: 188, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", paddingBottom: 8 }}>
      <Head n="1" label="Workspaces" active />
      <Row name="tasty-core" status="running" active />
      {rule}
      <Row name="scratch" status="idle" />
      <Head n="2" label="Services" />
      <Row name="api-gateway" status="agent" />
      {rule}
      <Row name="data-pipeline" status="running" />
      <Head n="3" label="Archived" collapsed />
    </div>
  );
}

// Collapsed rail: categories are `---` boundary buttons. Ctrl+Shift held puts
// the keycap centered ON the `---` slot (a collapsed/empty category still
// shows its `---`, so it still gets a keycap).
function CatSwitchRailMock({ held }) {
  const boundary = (n, active) => (
    <div style={{ height: 22, display: "flex", alignItems: "center", justifyContent: "center" }}>
      {held ? <NumCap n={n} active={active} /> : <div style={{ width: 24, height: 1, background: "var(--tasty-separator)" }} />}
    </div>
  );
  const av = (ch, active) => (
    <div style={{ width: 28, height: 28, borderRadius: "var(--tasty-radius)", display: "flex", alignItems: "center", justifyContent: "center",
      fontFamily: "var(--tasty-font-mono)", fontSize: 13, fontWeight: 700, color: "var(--tasty-text-secondary)",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset 0 0 0 1px var(--tasty-accent-primary)" : "none" }}>{ch}</div>
  );
  return (
    <div style={{ width: 44, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", padding: "8px 0", display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
      {boundary("1", true)}
      {av("T", true)}{av("S")}
      {boundary("2")}
      {av("A")}{av("D")}
      {boundary("3")}
    </div>
  );
}

// ── Banner — the 4th overlay family (floating, top-anchored, focus-less) ──
// Shared shell; sits at the top of a scope's content area, never over the tab bar.
function BannerShellG({ recessed, zIndex, children }) {
  return (
    <div data-surface="banner" data-audit-exempt={recessed ? "dimmed" : undefined} style={{ width: "100%", position: "relative", zIndex,
      background: "var(--tasty-banner-bg)", color: "var(--tasty-banner-fg)",
      border: "1px solid var(--tasty-banner-border)", borderRadius: "var(--tasty-banner-radius)",
      boxShadow: "var(--tasty-banner-shadow)", opacity: recessed ? "var(--tasty-banner-recessed-opacity)" : 1 }}>
      {children}
    </div>
  );
}

// a faux scope: tab strip on top (must stay clear) + content; banner floats at content top
function BannerScope({ height = 240, tab = "vim", children }) {
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: 600, height, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "1px solid var(--tasty-border-default)", background: "var(--tasty-surface-terminal-focused-bg)" }}>
      <div style={{ display: "flex", alignItems: "stretch", height: 28, background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        {["server", "dev", tab].map((t, i) => (
          <div key={t} style={{ display: "flex", alignItems: "center", padding: "0 12px", fontSize: 13, borderRight: "1px solid var(--tasty-separator)",
            background: i === 2 ? "var(--tasty-bg-panel)" : "transparent", color: i === 2 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
            boxShadow: i === 2 ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>{t}</div>
        ))}
      </div>
      <div style={{ padding: "12px 14px", fontFamily: "var(--tasty-font-mono)", fontSize: 12, lineHeight: 1.7, color: "var(--tasty-surface-terminal-focused-fg)" }}>
        <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> $ vim src/main.rs</div>
        <div>-- VISUAL --   1,1   Top</div>
      </div>
      <div style={{ position: "absolute", top: "calc(28px + var(--tasty-banner-margin))",
        left: "var(--tasty-banner-margin)", right: "var(--tasty-banner-margin)" }}>{children}</div>
    </div>
  );
}

// ── Banner "more" (⋯) menu — per-app opt-outs behind one trigger ──────────
// A row's label is fixed text + the interpolated program name; the fixed part
// never truncates, the app name (mono) shrinks first and ellipsises, so the menu
// stays inside --tasty-banner-more-menu-max-width in every locale.
// 2026-10-08 (batch 8) — the app name takes the ROW ink (banner-more-app-fg → menu-item-fg, hover → -fg-hover),
// i.e. it inherits; only the mono face sets it apart. wrap: the fixed copy alone is wider than the menu's
// max width (ja) → the whole label wraps as text, the menu stays at 288; the name never breaks mid-word and
// ellipsises only if it alone exceeds a full line.
function MoreLabel({ text, app, after, wrap = false }) {
  if (wrap) return (
    <span style={{ display: "block", whiteSpace: "normal" }}>
      {text}<span style={{ display: "inline-block", maxWidth: "100%", verticalAlign: "bottom", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        fontFamily: "var(--tasty-banner-more-app-font)", color: "inherit" }} title={app}>{app}</span>{after}
    </span>
  );
  return (
    <span style={{ display: "flex", minWidth: 0, whiteSpace: "nowrap" }}>
      <span style={{ flex: "none" }}>{text}</span>
      <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis",
        fontFamily: "var(--tasty-banner-more-app-font)", color: "inherit" }} title={app}>{app}</span>
      {after && <span style={{ flex: "none" }}>{after}</span>}
    </span>
  );
}

// the menu itself — 2 rows, no scrim, anchored under the ⋯ trigger.
// 2026-11-02 (mouse-capture-menu-wording) — rows are [prefix, suffix] around {app}; en = prefix only, ko/ja = suffix only.
const MORE_COPY = {
  en: [["Don't show again for ", ""], ["Let Tasty handle clicks in ", ""]],
  ko: [["", "에서 다시 보지 않기"], ["", "에서 클릭은 Tasty가 처리"]],
  ja: [["", " では今後表示しない"], ["", " ではクリックを Tasty で処理"]],
};
// 2026-11-03 (mouse-capture-menu-unknown-name) — program name unknown → same two rows, DISABLED, worded
// without a name ("this program"), under a non-interactive reason line. No "(unknown)" placeholder.
const MORE_COPY_UNKNOWN = {
  en: { reason: "Can't tell which program is running, so nothing can be saved.", rows: ["Don't show again for this program", "Let Tasty handle clicks in this program"] },
  ko: { reason: "실행 중인 프로그램을 확인할 수 없어 저장할 수 없습니다.", rows: ["이 프로그램에서 다시 보지 않기", "이 프로그램에서 클릭은 Tasty가 처리"] },
  ja: { reason: "実行中のプログラムを特定できないため、保存できません。", rows: ["このプログラムでは今後表示しない", "このプログラムではクリックを Tasty で処理"] },
};
function BannerMoreMenuUnknownG({ lang = "en", style }) {
  const c = MORE_COPY_UNKNOWN[lang];
  return (
    <div role="menu" aria-label="Banner options" style={{
      width: "var(--tasty-banner-more-menu-max-width)", boxSizing: "border-box",
      background: "var(--tasty-banner-more-menu-bg)", border: "1px solid var(--tasty-banner-more-menu-border)",
      borderRadius: "var(--tasty-banner-more-menu-radius)", padding: "var(--tasty-banner-more-menu-padding)",
      boxShadow: "var(--tasty-banner-more-menu-shadow)", ...style }}>
      <div role="note" style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)",
        padding: "var(--tasty-space-xs) var(--tasty-space-md)", color: "var(--tasty-banner-more-reason-fg)",
        fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-more-reason-glyph-offset)" }}><Icon name="info" size={14} /></span>
        <span style={{ minWidth: 0 }}>{c.reason}</span>
      </div>
      <MenuItem separator />
      <MenuItem disabled wrap aria-disabled="true" icon={<Icon name="bell" />} label={c.rows[0]} />
      <MenuItem disabled wrap aria-disabled="true" icon={<Icon name="mouse" />} label={c.rows[1]} />
    </div>
  );
}
function BannerMoreMenuG({ app = "vim", hovered = -1, danger = false, lang = "en", texts = null, wrapRows = [], style }) {
  if (!app) return <BannerMoreMenuUnknownG lang={lang} style={style} />;
  const copy = texts || MORE_COPY[lang];
  const rows = [
    { icon: <Icon name="bell" />, danger: false },
    { icon: <Icon name="mouse" />, danger },
  ].map((r, i) => ({ ...r, text: copy[i][0], after: copy[i][1] }));
  return (
    <div role="menu" aria-label="Banner options" style={{
      minWidth: "var(--tasty-banner-more-menu-min-width)", maxWidth: "var(--tasty-banner-more-menu-max-width)",
      background: "var(--tasty-banner-more-menu-bg)", border: "1px solid var(--tasty-banner-more-menu-border)",
      borderRadius: "var(--tasty-banner-more-menu-radius)", padding: "var(--tasty-banner-more-menu-padding)",
      boxShadow: "var(--tasty-banner-more-menu-shadow)", ...style }}>
      {rows.map((r, i) => (
        <MenuItem key={i} icon={r.icon} danger={r.danger} active={hovered === i} wrap={wrapRows.includes(i)}
          label={<MoreLabel text={r.text} app={app} after={r.after} wrap={wrapRows.includes(i)} />} />
      ))}
    </div>
  );
}

// the canonical mouse-capture banner — persistent (no TTL); ⋯ and × on hover.
// Fires once per tracking session, on a real user click while an app holds the
// mouse. Content = title + shift-bypass hint; the two per-app opt-outs live
// behind the ⋯ trigger, so the card body is never crowded by action buttons.
// 2026-11-03 — ⋯ is ALWAYS drawn (also when app is null = name unknown → disabled menu form).
function MouseCaptureBannerG({ glyph = true, body, lines, app = "vim", lang = "en", open = false, hovered = -1, danger = false, forceHover = false }) {
  const more = true;
  const [hover, setHover] = React.useState(false);
  const shown = hover || forceHover || open;
  const defBody = (
    <>This app is capturing the mouse. Hold <Kbd keys="Shift" />+drag to select text, <Kbd keys="Shift" />+Right-click for the tasty menu.</>
  );
  return (
    <BannerShellG>
      <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-banner-gap)",
          padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
        {glyph && <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-glyph-offset)", color: "var(--tasty-banner-icon-fg)" }}>{ic.mouse}</span>}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)",
          paddingRight: "var(--tasty-banner-more-reserve)" }}>
          <div style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: 600, color: "var(--tasty-banner-fg)", lineHeight: 1.4 }}>Mouse input captured</div>
          <div style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)", lineHeight: 1.45 }}>{body || defBody}</div>
        </div>
        <span style={{ position: "absolute", top: "var(--tasty-banner-padding-y)", right: "var(--tasty-space-sm)",
          display: "flex", alignItems: "center", gap: "var(--tasty-banner-more-column-gap)",
          opacity: shown ? 1 : 0, transition: "opacity var(--tasty-banner-fade) var(--tasty-ease-ui)" }}>
          {more && (
            <span style={{ position: "relative", display: "inline-flex" }}>
              <IconButton size="sm" aria-label="More options" aria-expanded={open} active={open}><Icon name="more" /></IconButton>
              {open && (
                <div style={{ position: "absolute", top: "calc(100% + var(--tasty-banner-more-menu-offset))", right: 0, zIndex: 2 }}>
                  <BannerMoreMenuG app={app} hovered={hovered} danger={danger} lang={lang} />
                </div>
              )}
            </span>
          )}
          <IconButton size="sm" aria-label="Dismiss">{ic.x}</IconButton>
        </span>
      </div>
    </BannerShellG>
  );
}

// interactive: click ⋯ to open, outside-click / Esc to close, row click acts.
function BannerMoreDemoG({ defaultOpen = false }) {
  const [open, setOpen] = React.useState(defaultOpen);
  const [log, setLog] = React.useState(null);
  const wrap = React.useRef(null);
  React.useEffect(() => {
    if (!open) return undefined;
    const onDown = (e) => { if (wrap.current && !wrap.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onDown); document.removeEventListener("keydown", onKey); };
  }, [open]);
  const act = (which) => { setOpen(false); setLog(which); };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%", maxWidth: 600 }}>
      <div ref={wrap}>
        <BannerScope height={230}>
          {log !== "notice" ? (
            <BannerShellG>
              <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-banner-gap)",
                padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
                <span style={{ display: "inline-flex", flex: "none", marginTop: 1, color: "var(--tasty-banner-icon-fg)" }}>{ic.mouse}</span>
                <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2, paddingRight: "var(--tasty-banner-more-reserve)" }}>
                  <div style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: 600, color: "var(--tasty-banner-fg)", lineHeight: 1.4 }}>Mouse input captured</div>
                  <div style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)", lineHeight: 1.45 }}>
                    This app is capturing the mouse. Hold <Kbd keys="Shift" />+drag to select text, <Kbd keys="Shift" />+Right-click for the tasty menu.
                  </div>
                </div>
                <span style={{ position: "absolute", top: "var(--tasty-banner-padding-y)", right: "var(--tasty-space-sm)",
                  display: "flex", alignItems: "center", gap: "var(--tasty-banner-more-column-gap)" }}>
                  <span style={{ position: "relative", display: "inline-flex" }}>
                    <IconButton size="sm" aria-label="More options" aria-expanded={open} active={open}
                      onClick={() => setOpen((o) => !o)}><Icon name="more" /></IconButton>
                    {open && (
                      <div style={{ position: "absolute", top: "calc(100% + var(--tasty-banner-more-menu-offset))", right: 0, zIndex: 2 }}>
                        <div role="menu" aria-label="Banner options" style={{
                          minWidth: "var(--tasty-banner-more-menu-min-width)", maxWidth: "var(--tasty-banner-more-menu-max-width)",
                          background: "var(--tasty-banner-more-menu-bg)", border: "1px solid var(--tasty-banner-more-menu-border)",
                          borderRadius: "var(--tasty-banner-more-menu-radius)", padding: "var(--tasty-banner-more-menu-padding)",
                          boxShadow: "var(--tasty-banner-more-menu-shadow)" }}>
                          <MenuItem icon={<Icon name="bell" />} onClick={() => act("notice")}
                            label={<MoreLabel text="Don't show again for " app="vim" />} />
                          <MenuItem icon={<Icon name="mouse" />} onClick={() => act("capture")}
                            label={<MoreLabel text="Let Tasty handle clicks in " app="vim" />} />
                        </div>
                      </div>
                    )}
                  </span>
                  <IconButton size="sm" aria-label="Dismiss" onClick={() => setLog("dismissed")}>{ic.x}</IconButton>
                </span>
              </div>
            </BannerShellG>
          ) : null}
        </BannerScope>
      </div>
      <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", minHeight: 16 }}>
        {log === "notice" && <>Added <code>vim</code> to <code>mouse_capture_banner_blacklist</code> — banner closed with the action.</>}
        {log === "capture" && <>Added <code>vim</code> to <code>mouse_capture_blacklist</code> — capture released; the banner stays until dismissed.</>}
        {log === "dismissed" && <>Dismissed for this tracking session — nothing written to Settings.</>}
        {!log && <>Click <b>⋯</b> to open · outside click or <span className="ic">Esc</span> closes.</>}
      </div>
    </div>
  );
}

// B4 — position context + hit-zone. The card consumes its own mouse; the surface
// body below it is pass-through (clicks reach the capturing app).
function MouseCaptureHitZone() {
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: 560, height: 250, borderRadius: "var(--tasty-radius)", overflow: "hidden",
      border: "1px solid var(--tasty-border-default)", background: "var(--tasty-surface-terminal-focused-bg)" }}>
      {/* tab bar — banner must never cover this */}
      <div style={{ display: "flex", alignItems: "stretch", height: 28, background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        {["server", "dev", "htop"].map((t, i) => (
          <div key={t} style={{ display: "flex", alignItems: "center", padding: "0 12px", fontSize: 13, borderRight: "1px solid var(--tasty-separator)",
            background: i === 2 ? "var(--tasty-bg-panel)" : "transparent", color: i === 2 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
            boxShadow: i === 2 ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>{t}</div>
        ))}
      </div>
      {/* surface body — pass-through */}
      <div style={{ position: "absolute", inset: "28px 0 0 0", display: "flex", alignItems: "flex-end", padding: 12 }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>↑ surface body — pass-through · clicks/drag reach the app</span>
      </div>
      {/* the banner card, with an explicit consume-zone outline */}
      <div style={{ position: "absolute", top: "calc(28px + var(--tasty-banner-margin))", left: "var(--tasty-banner-margin)", right: "var(--tasty-banner-margin)" }}>
        <div style={{ position: "relative" }}>
          <MouseCaptureBannerG />
          <span style={{ position: "absolute", top: -7, left: 8, fontFamily: "var(--tasty-font-mono)", fontSize: 9, letterSpacing: ".04em",
            background: "var(--tasty-bg-app)", color: "var(--tasty-accent-info-raised)", padding: "0 4px", borderRadius: 2 }}>CARD RECT = mouse-consume + hover zone</span>
        </div>
      </div>
    </div>
  );
}

// blacklist editor (Settings › Terminal) — list rows + add input + empty state
function BlacklistRow({ pattern, hover }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, height: 28, padding: "0 4px 0 8px", borderRadius: "var(--tasty-radius-sm)",
      background: hover ? "var(--tasty-overlay-hover)" : "transparent" }}>
      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)" }}>{pattern}</span>
      <IconButton size="sm" aria-label={"Remove " + pattern}>{ic.x}</IconButton>
    </div>
  );
}

function BlacklistEditorG({ empty }) {
  return (
    <div style={{ width: 320, display: "flex", flexDirection: "column", gap: 10, padding: 14,
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Mouse capture</div>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-text-secondary)" }}>Show mouse-capture hint</span>
        <Switch defaultChecked />
      </div>
      <div style={{ height: 1, background: "var(--tasty-separator)" }} />
      <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)" }}>Don't show the capture notice for these programs</div>
      {empty ? (
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5, padding: "4px 0" }}>No programs listed — the notice shows for every capturing program.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          <BlacklistRow pattern="less" />
        </div>
      )}
      <div style={{ height: 1, background: "var(--tasty-separator)" }} />
      <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)" }}>Let Tasty handle clicks in these programs</div>
      {empty ? (
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5, padding: "4px 0" }}>No programs listed — clicks are sent to capturing programs.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          <BlacklistRow pattern="htop" />
          <BlacklistRow pattern="vim" hover />
          <BlacklistRow pattern="ht*" />
        </div>
      )}
      <div style={{ display: "flex", gap: 6 }}>
        <Input block placeholder="process name or pattern, e.g. htop or ht*" />
        <Button variant="secondary" size="sm" disabled={empty}>Add</Button>
      </div>
      <div style={{ fontSize: 11, color: "var(--tasty-accent-warning-raised)", lineHeight: 1.5 }}>
        Case-insensitive substring or <b>*</b> wildcard on the process name. When a listed program is foreground, clicks/drags are handled locally; the wheel is still sent.
      </div>
    </div>
  );
}

// a TTL banner — live countdown that flips to X on hover (and pauses while hovered).
// 2026-11-02: no user action → TTL (the rule). Loops back to `ttl` at 0 so the specimen keeps running.
function TtlBannerG({ ttl = 6, glyph, glyphColor, title, body }) {
  const [hover, setHover] = React.useState(false);
  const [n, setN] = React.useState(ttl);
  React.useEffect(() => {
    if (hover) return undefined;                       // pause while hovered
    const id = setTimeout(() => setN((x) => (x <= 1 ? ttl : x - 1)), 1000);
    return () => clearTimeout(id);
  }, [hover, n, ttl]);
  if (title) return (
    <BannerShellG>
      <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-banner-gap)", padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-glyph-offset)", color: glyphColor || "var(--tasty-banner-icon-fg)" }}>{glyph}</span>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)" }}>
          <span style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-banner-fg)" }}>{title}</span>
          {body && <span style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)" }}>{body}</span>}
        </div>
        <span style={{ width: "var(--tasty-size-24)", height: "var(--tasty-size-24)", flex: "none", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
          {hover
            ? <IconButton size="sm" aria-label="Dismiss">{ic.x}</IconButton>
            : <span style={{ fontFamily: "var(--tasty-banner-countdown-font)", fontSize: "var(--tasty-banner-countdown-font-size)",
                color: "var(--tasty-banner-countdown-fg)", fontVariantNumeric: "tabular-nums" }}>{n}</span>}
        </span>
      </div>
    </BannerShellG>
  );
  return (
    <BannerShellG>
      <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ display: "flex", alignItems: "center", gap: "var(--tasty-banner-gap)",
          padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
        <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-success-raised)" }}>{ic.check}</span>
        <div style={{ flex: 1, fontSize: "var(--tasty-banner-title-font-size)", color: "var(--tasty-banner-fg)" }}>
          Preset <b style={{ color: "var(--tasty-text-primary)" }}>Dev split</b> applied to this tab.
        </div>
        <span style={{ width: 24, height: 24, flex: "none", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
          {hover
            ? <IconButton size="sm" aria-label="Dismiss">{ic.x}</IconButton>
            : <span style={{ fontFamily: "var(--tasty-banner-countdown-font)", fontSize: "var(--tasty-banner-countdown-font-size)",
                color: "var(--tasty-banner-countdown-fg)", fontVariantNumeric: "tabular-nums" }}>{n}</span>}
        </span>
      </div>
    </BannerShellG>
  );
}

// cross-scope stacking — a higher-scope banner sits in front; the lower one is dimmed behind
function StackDemoG() {
  const line = (txt) => (<span style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)" }}>{txt}</span>);
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: 520, height: 116 }}>
      {/* lower-tier (Pane) banner — taller, recessed behind */}
      <div style={{ position: "absolute", top: 14, left: 0, right: 0 }}>
        <BannerShellG recessed zIndex={1}>
          <div style={{ padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)", height: 84, display: "flex", alignItems: "flex-end" }}>
            {line("Pane banner — recessed to 40% opacity behind the higher banner; only its overhang shows")}
          </div>
        </BannerShellG>
      </div>
      {/* higher-tier (Workspace) banner — on top, full opacity, higher z */}
      <div style={{ position: "absolute", top: 0, left: 0, right: 0 }}>
        <BannerShellG zIndex={2}>
          <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
            <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-warning-raised)" }}>{ic.warn}</span>
            <span style={{ flex: 1, fontSize: "var(--tasty-banner-title-font-size)", color: "var(--tasty-banner-fg)" }}>Workspace banner — in front, full opacity, higher z-index</span>
          </div>
        </BannerShellG>
      </div>
    </div>
  );
}

// ── Git worktree viewer — 960×640 read-only popup (status/log + diff) ──
// token-colored pill — mirrors the Tag visual (outlined, color-mix tint); `info`
// is the tone Tag variants don't ship (main / oid / refs / hunk = sky).
const GV_TONE = { info: "var(--tasty-accent-info)", success: "var(--tasty-accent-success)",
  warning: "var(--tasty-accent-warning)", danger: "var(--tasty-accent-danger)", primary: "var(--tasty-accent-primary)" };
function gvPill(txt, tone, dot, width) {
  const c = GV_TONE[tone];
  const ink = c && c.replace(")", "-raised)");
  const base = { display: "inline-flex", alignItems: "center", justifyContent: width ? "center" : "flex-start", gap: 4,
    height: 16, padding: width ? 0 : "0 6px", width, flex: "none", borderRadius: "var(--tasty-radius-sm)",
    fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, lineHeight: 1, whiteSpace: "nowrap", border: "1px solid" };
  const dotEl = dot ? <span style={{ width: 5, height: 5, borderRadius: "var(--tasty-radius-pill)", background: "currentColor" }} /> : null;
  if (!c) return <span key={txt} style={{ ...base, color: "var(--tasty-text-secondary)", borderColor: "var(--tasty-border-default)", background: "var(--tasty-surface-raised)" }}>{txt}</span>;
  return <span key={txt} style={{ ...base, color: ink, borderColor: `color-mix(in srgb, ${c} 40%, transparent)`, background: `color-mix(in srgb, ${c} 12%, var(--sg-chip-base, transparent))` }}>{dotEl}{txt}</span>;
}
const gvStrip = { display: "flex", alignItems: "center", gap: 8, height: 26, flex: "none", padding: "0 12px",
  background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)",
  fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" };

// ── Git worktree viewer — 960×640 read-only popup (rail · status/log · diff) ──
function GitViewerFrame({ diff }) {
  const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12 };
  const WtRow = ({ name, oid, type, state, reason, active, dim }) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 3, padding: "7px 10px", borderBottom: "1px solid var(--tasty-separator)",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ ...mono, fontWeight: 600, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          color: dim ? "var(--tasty-text-muted)" : active ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)" }}>{name}</span>
        {type === "main" ? gvPill("main", "info") : gvPill("linked")}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        {oid && <span style={{ ...mono, fontSize: 11, color: active ? "var(--tasty-text-on-active)" : "var(--tasty-accent-info-raised)" }}>{oid}</span>}
        <span style={{ flex: 1 }} />
        {state && gvPill(state, state === "current" ? "success" : state === "locked" ? "warning" : "danger", true)}
      </div>
    </div>
  );
  const StRow = ({ p, tone, dir, file, active }) => (
    <div style={{ display: "flex", alignItems: "center", gap: 8, height: 26, padding: "0 12px",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      {gvPill(p, tone, false, 18)}
      <span style={{ ...mono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        <span style={{ color: "var(--tasty-text-muted)" }}>{dir}</span><span style={{ color: "var(--tasty-text-primary)" }}>{file}</span>
      </span>
    </div>
  );
  const Head = ({ children }) => <div style={gvStrip}>{children}</div>;
  const dl = (oldn, newn, kind, content) => {
    const hunk = kind === "@", add = kind === "+", del = kind === "-";
    const fg = hunk ? "var(--tasty-accent-info-raised)" : add ? "var(--tasty-accent-success-raised)" : del ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-primary)";
    const bg = hunk ? "color-mix(in srgb, var(--tasty-accent-info) 9%, transparent)" : add ? "color-mix(in srgb, var(--tasty-accent-success) 10%, transparent)" : del ? "color-mix(in srgb, var(--tasty-accent-danger) 10%, transparent)" : "transparent";
    return (
      <div style={{ display: "flex", ...mono, fontSize: 11, lineHeight: 1.65, background: bg, color: fg }}>
        <span style={{ width: 30, textAlign: "right", color: "var(--tasty-text-muted-raised)", paddingRight: 6 }}>{hunk ? "" : oldn}</span>
        <span style={{ width: 30, textAlign: "right", color: "var(--tasty-text-muted-raised)", paddingRight: 8 }}>{hunk ? "" : newn}</span>
        <span style={{ width: 12, textAlign: "center" }}>{hunk || kind === " " ? "" : kind}</span>
        <span style={{ whiteSpace: "pre" }}>{content}</span>
      </div>
    );
  };
  return (
    <div style={{ width: "100%", maxWidth: 640, height: 412, display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      {/* header */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "8px 12px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name="gitBranch" size={16} /></span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Git</span>
        <span style={{ flex: 1 }} />
        <Button variant="secondary" size="sm" leadingIcon={ic.refresh}>Refresh</Button>
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      {/* context strip */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, height: 28, flex: "none", padding: "0 12px", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ ...mono, color: "var(--tasty-text-primary)" }}>tasty</span>
        <span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-text-disabled)" }}>·</span>
        <span style={{ ...mono, color: "var(--tasty-text-secondary)" }}>main</span>
        {gvPill("a1b2c3d", "info")}
        <span style={{ flex: 1 }} />
        <span style={{ ...mono, fontSize: 11, color: "var(--tasty-text-muted)" }}>~/work/tasty</span>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* rail */}
        <div style={{ width: 200, flex: "none", borderRight: "1px solid var(--tasty-separator)", display: "flex", flexDirection: "column", overflow: "hidden" }}>
          <Head>Worktrees (4)</Head>
          <div style={{ flex: 1, overflow: "auto" }}>
            <WtRow name="tasty" oid="a1b2c3d" type="main" state="current" active />
            <WtRow name="feature-ui" oid="9f8e7d6" type="linked" state="locked" />
            <WtRow name="release-0.6" oid="3c4d5e6" type="linked" />
            <WtRow name="stale-wt" type="linked" state="invalid" dim />
          </div>
        </div>
        {/* right column */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ flex: 1, minHeight: 0, borderBottom: "1px solid var(--tasty-separator)", display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <Head>Changes (4)</Head>
            <div style={{ flex: 1, overflow: "auto", padding: "2px 0" }}>
              <StRow p="M" tone="warning" dir="crates/tasty-plugin-git-viewer/src/" file="view.rs" active={diff} />
              <StRow p="A" tone="success" dir="ui_kits/terminal/overlays/" file="git_viewer.jsx" />
              <StRow p="D" tone="danger" dir="crates/tasty-gallery/src/catalog/" file="legacy_git.rs" />
              <StRow p="?" tone={null} dir="scraps/" file="git-notes.txt" />
            </div>
          </div>
          <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            {diff ? (
              <>
                <div style={{ display: "flex", alignItems: "center", gap: 8, height: 30, flex: "none", padding: "0 8px 0 12px", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
                  <Button variant="ghost" size="sm" leadingIcon={ic.back}>Back</Button>
                  <span style={{ ...mono, fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>crates/tasty-plugin-git-viewer/src/view.rs</span>
                </div>
                <div style={{ flex: 1, overflow: "auto", background: "var(--tasty-bg-app)", padding: "2px 0" }}>
                  {dl("", "", "@", "@@ -38,9 +38,12 @@ fn draw_worktree_rail()")}
                  {dl("38", "38", " ", "    for wt in worktrees {")}
                  {dl("39", "", "-", "        ui.label(wt.name);")}
                  {dl("40", "", "-", "        ui.colored_label(BLUE, wt.oid);")}
                  {dl("", "39", "+", "        let row = SelectableRow::new(wt.id);")}
                  {dl("", "40", "+", "        row.badge(wt.state_badge());")}
                  {dl("41", "41", " ", "    }")}
                </div>
              </>
            ) : (
              <>
                <Head>Commits (5)</Head>
                <div style={{ flex: 1, overflow: "auto", padding: "2px 0" }}>
                  {[["a1b2c3d", ["main", "origin/main"], "feat(git-viewer): worktree rail + diff well", "2h"],
                    ["9f8e7d6", [], "fix: align diff gutter line numbers", "5h"],
                    ["3c4d5e6", ["release/0.6"], "docs: read-only viewer note", "1d"]].map(([o, refs, s, t]) => (
                    <div key={o} style={{ display: "flex", alignItems: "center", gap: 8, height: 28, padding: "0 12px", whiteSpace: "nowrap", overflow: "hidden" }}>
                      <span style={{ ...mono, fontSize: 11, color: "var(--tasty-accent-info-raised)" }}>{o}</span>
                      {refs.map((r) => gvPill(r, "info"))}
                      <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis" }}>{s}</span>
                      <span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>zilhak</span>
                      <span style={{ ...mono, fontSize: 11, color: "var(--tasty-text-muted)" }}>{t} ago</span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Clipboard viewer — 480×360 read-only SNAPSHOT popup (single-column) ──
function ClipboardFrame({ state = "data" }) {
  const TEXT = `cargo build -p tasty-gallery --release
git switch wt-5/T8-code-review
tasty read screen --surface 3 --json
~/work/tasty/crates/tasty-gallery/src/catalog`;
  const center = (glyph, title, sub, danger) => (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8, padding: 24, textAlign: "center" }}>
      <span style={{ display: "inline-flex", color: danger ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-muted)" }}>{glyph}</span>
      <span style={{ fontSize: 13, fontWeight: 600, color: danger ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-secondary)" }}>{title}</span>
      {sub && <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", maxWidth: 280, lineHeight: 1.5 }}>{sub}</span>}
    </div>
  );
  const clipGlyph = <Icon name="clipboard" size={28} />;
  const Frame = ({ children }) => (
    <div style={{ width: "100%", maxWidth: 460, height: 320, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 14px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name="clipboard" size={16} /></span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Clipboard</span>
        <Tag>snapshot</Tag>
        <span style={{ flex: 1 }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      {children}
    </div>
  );
  if (state === "empty") return <Frame>{center(clipGlyph, "Clipboard is empty", "Copy some text, an image, or files and reopen to see a snapshot here.")}</Frame>;
  if (state === "failed") return <Frame>{center(<Icon name="alertTriangle" size={28} />, "Couldn't read the clipboard", "The system clipboard handle could not be opened.", true)}</Frame>;
  const seg = (id, active, first, compact) => {
    const [label, glyph] = TYPE_DEF[id];
    const bare = compact && !active;
    return (
      <span key={id} title={label} style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", gap: bare ? 0 : 4, height: 26,
        padding: bare ? "0 8px" : "0 10px", minWidth: bare ? 30 : 0,
        borderLeft: first ? "none" : "1px solid var(--tasty-border-default)", fontSize: 12, fontWeight: active ? 600 : 400,
        color: active ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)", background: active ? "var(--tasty-accent-primary)" : "transparent" }}>
        <Icon name={glyph} size={13} />{bare ? null : label}</span>
    );
  };
  const ids = CB_SETS[state] || CB_SETS.data;
  const cur = state === "html" || state === "htmlPretty" ? "html" : state === "other" ? "other" : "text";
  const compact = ids.length >= 5;
  const meta = cur === "html" ? "312 chars · 1 line" : cur === "other" ? "4 formats" : "284 chars · 6 lines · UTF-8";
  const mime = cur === "html" ? "text/html · 312 chars · 1 line" : cur === "other" ? "4 unrecognized formats" : "text/plain";
  const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12, lineHeight: 1.7, color: "var(--tasty-text-primary)" };
  const metaMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" };
  const well = { flex: 1, minHeight: 0, overflow: "auto", margin: "8px 14px 14px", borderRadius: "var(--tasty-radius)",
    border: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-app)" };
  const body = cur === "other" ? (
    <div style={well}>
      <div style={{ display: "flex", flexDirection: "column" }}>
        {CB_FORMATS.map((f, i) => (
          <div key={f.name} style={{ display: "flex", flexDirection: "column", gap: 4, padding: "10px 14px",
            borderBottom: i < CB_FORMATS.length - 1 ? "1px solid var(--tasty-separator)" : "none" }}>
            <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
              <span style={{ ...metaMono, color: "var(--tasty-text-secondary)", fontWeight: 600 }}>{f.name}</span>
              <span style={metaMono}>{f.size}</span>
            </div>
            <pre style={{ ...mono, margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-all" }}>{f.content}</pre>
            {f.moreLines ? <span style={{ ...metaMono, fontStyle: "italic" }}>+{f.moreLines} more lines</span> : null}
          </div>
        ))}
      </div>
    </div>
  ) : (
    <div style={well}>
      <pre style={{ ...mono, margin: 0, padding: "10px 14px", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
        {cur === "html" ? (state === "htmlPretty" ? CB_HTML_PRETTY : CB_HTML_RAW) : TEXT}</pre>
    </div>
  );
  return (
    <Frame>
      {/* type bar */}
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 14px", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        {ids.length > 1 ? (
          <div style={{ display: "inline-flex", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
            {ids.map((id, i) => seg(id, id === cur, i === 0, compact))}
          </div>
        ) : (
          <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
            <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name="textLeft" size={13} /></span>
            <Tag variant="accent">Text</Tag>
          </span>
        )}
        <span style={{ flex: 1 }} />
        {cur === "html"
          ? <Checkbox label="Pretty print" checked={state === "htmlPretty"} readOnly />
          : <span style={metaMono}>{meta}</span>}
      </div>
      {body}
      {/* footer */}
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 14px", flex: "none", borderTop: "1px solid var(--tasty-separator)" }}>
        <span style={metaMono}>{mime}</span>
        <span style={{ flex: 1 }} />
        <Button variant="secondary" size="sm">Close</Button>
      </div>
    </Frame>
  );
}
const TYPE_DEF = { text: ["Text", "textLeft"], image: ["Image", "image"], files: ["Files", "file"], html: ["HTML", "html"], other: ["Other", "layers"] };
const CB_SETS = { data: ["text"], multi: ["text", "image", "files"], html: ["text", "html"], htmlPretty: ["text", "html"],
  other: ["text", "other"], five: ["text", "image", "files", "html", "other"] };
const CB_HTML_RAW = `<div class="release"><h3>tasty 0.9.4</h3><ul><li><a href="https://github.com/zilhak/tasty/pull/812">#812</a> clipboard viewer: HTML type</li><li>#815 port scanner: refresh throttle</li></ul></div>`;
const CB_HTML_PRETTY = `<div class="release">
  <h3>
    tasty 0.9.4
  </h3>
  <ul>
    <li>
      <a href="https://github.com/zilhak/tasty/pull/812">
        #812
      </a>
      clipboard viewer: HTML type
    </li>
    <li>
      #815 port scanner: refresh throttle
    </li>
  </ul>
</div>`;
const CB_FORMATS = [
  { name: "RTF", size: "1.4 KB", content: "{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 D2Coding;}}\n\\f0\\fs20 cargo build -p tasty-gallery --release\\par", moreLines: 22 },
  { name: "Custom App Data", size: "860 B", content: "com.tasty.pane-ref v1\nsurface=3 pane=0 range=112:0-118:64" },
  { name: "text/csv", size: "204 B", content: "port,pid,process\n5173,48211,node", moreLines: 4 },
  { name: "application/x-vnd.oasis.opendocument.text", size: "12.8 KB", content: "PK\\x03\\x04\\x14\\x00 mimetypeapplication/vnd.oasis…" },
];

// ── Remote profile / passkey form — a route inside the 520×460 remote_tool popup ──
function RemoteFormFrame({ variant = "ssh", error }) {
  const passkey = variant.startsWith("passkey");
  const attach = variant.startsWith("attach");
  const attachInline = variant === "attach-inline";
  const FRow = ({ label, children, hint }) => (
    <>
      <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, alignItems: "center" }}>
        <span style={{ textAlign: "right", fontSize: 13, color: "var(--tasty-text-muted)" }}>{label}</span>
        <div style={{ minWidth: 0 }}>{children}</div>
      </div>
      {hint && <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12 }}><span /><span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.4 }}>{hint}</span></div>}
    </>
  );
  const peach = (txt) => (
    <span style={{ display: "inline-flex", alignItems: "center", height: 16, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
      fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, color: "var(--tasty-accent-warning-raised)", border: "1px solid var(--tasty-accent-warning)" }}>{txt}</span>
  );
  const seg = (label, active) => (
    <div style={{ display: "flex", alignItems: "center", height: "var(--tasty-control-height)", padding: "0 12px", borderRadius: "var(--tasty-radius)", fontSize: 13, cursor: "pointer",
      /* R1 — a SEGMENTED control's active segment is an accent FILL (same as
         Apply-preset scope and the clipboard type segment). surface-active is the
         row-selection fill and is not used here. Tab STRIPS keep the underline. */
      background: active ? "var(--tasty-accent-primary)" : "var(--tasty-surface-raised)", color: active ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)",
      border: "1px solid " + (active ? "var(--tasty-border-strong)" : "var(--tasty-border-default)") }}>{label}</div>
  );
  return (
    <div style={{ width: "100%", maxWidth: 460, height: 408, display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      {/* header */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 14px", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.remote}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Remote connections</span>
        <span style={{ flex: 1 }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      {/* tabs */}
      <div style={{ display: "flex", gap: 4, padding: "6px 10px 0", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        {[["Remote profiles", !passkey && !attach], ["Attach", attach], ["Passkeys", passkey]].map(([t, a]) => (
          <div key={t} style={{ padding: "6px 12px", fontSize: 13, cursor: "pointer", color: a ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
            boxShadow: a ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>{t}</div>
        ))}
      </div>
      {/* body */}
      <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "12px 16px", display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 2 }}>{passkey ? "New passkey" : attach ? "New attach" : "New profile"}</div>
        {!passkey && !attach && (
          <>
            <FRow label="Type"><div style={{ display: "flex", gap: 4 }}><Input block defaultValue={variant === "generic" ? "smb" : "ssh"} /><Select options={["ssh", "smb", "http"]} style={{ width: 64 }} /></div></FRow>
            {variant === "generic" && error !== "type" && <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12 }}><span /><span>{peach("Unknown type")}</span></div>}
            <FRow label="Name"><Input block defaultValue={variant === "generic" ? "fileshare" : "prod-web"} /></FRow>
            {variant === "ssh" ? (
              <>
                <FRow label="Host"><Input block defaultValue="10.0.0.4" /></FRow>
                <FRow label="User"><Input block defaultValue="deploy" /></FRow>
                <FRow label="Port"><Input style={{ width: 96 }} defaultValue="22" /></FRow>
                <FRow label="Label"><Input block placeholder="optional" /></FRow>
                <FRow label="Shell" hint="Auto-detects the remote shell once when saved (runs an SSH probe)."><Select options={["auto", "bash", "zsh", "fish"]} block /></FRow>
                <FRow label="Passkey"><Select options={["(none)", "id_ed25519", "deploy-key"]} block /></FRow>
              </>
            ) : (
              <>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)", marginTop: 4 }}>Fields</div>
                {[["host", "fs.local"], ["share", "team"]].map(([k, v]) => (
                  <div key={k} style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr 28px", columnGap: 12, alignItems: "center" }}>
                    <Input defaultValue={k} style={{ width: "100%" }} /><Input defaultValue={v} block /><IconButton size="sm" aria-label="Remove field">{ic.x}</IconButton>
                  </div>
                ))}
                <div><Button variant="ghost" size="sm" leadingIcon={ic.plus}>Add field</Button></div>
                <FRow label="Passkey"><Select options={["(none)", "smb-cred"]} block /></FRow>
              </>
            )}
          </>
        )}
        {attach && (
          <>
            <FRow label="Name"><Input block defaultValue="gb10" /></FRow>
            <FRow label="Label"><Input block placeholder="optional" defaultValue={attachInline ? "" : "us-east"} /></FRow>
            <FRow label="Connection"><div style={{ display: "flex", gap: 6 }}>{seg("SSH profile", !attachInline)}{seg("Direct (inline)", attachInline)}</div></FRow>
            {attachInline ? (
              <>
                <FRow label="Host"><Input block defaultValue="edge.example.com" /></FRow>
                <FRow label="User"><Input block defaultValue="root" /></FRow>
                <FRow label="Port"><Input style={{ width: 96 }} defaultValue="22" /></FRow>
                <FRow label="Shell"><Select options={["auto", "bash", "zsh", "fish"]} block /></FRow>
                <FRow label="Passkey"><Select options={["(none)", "edge-pem"]} block /></FRow>
              </>
            ) : (
              <FRow label="SSH profile"><Select options={["(select a profile)", "prod-web (us-east)", "db-primary", "legacy-box"]} block /></FRow>
            )}
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)", marginTop: 4 }}>Remote tasty</div>
            <FRow label="Executable"><Input block defaultValue={attachInline ? "/opt/tasty/bin/tasty" : "tasty"} /></FRow>
            <FRow label="Port mode"><Select options={["auto", "subcommand", "file-unix", "file-windows"]} block /></FRow>
            <FRow label="Port file" hint="Optional — an explicit path takes precedence over the port mode."><Input block placeholder="optional path" defaultValue={attachInline ? "/run/user/1000/tasty/port" : ""} /></FRow>
          </>
        )}
        {passkey && (
          <>
            <FRow label="Name"><Input block defaultValue="id_ed25519" /></FRow>
            <FRow label="Kind"><div style={{ display: "flex", gap: 6 }}>{seg("path", variant === "passkey-path")}{seg("inline", variant === "passkey-inline")}</div></FRow>
            {variant === "passkey-path"
              ? <FRow label="Value" hint="References a key file you own."><Input block defaultValue="~/.ssh/id_ed25519" /></FRow>
              : <FRow label="Value" hint="Pasted secret is materialized to a 0600-managed file."><textarea rows={3} placeholder="Paste secret / key contents" style={{ width: "100%", resize: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 12, padding: 8, background: "var(--tasty-input-bg)", color: "var(--tasty-input-fg)", border: "1px solid var(--tasty-input-border)", borderRadius: "var(--tasty-radius)" }} /></FRow>}
            <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12 }}><span /><span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.4 }}>Local-only. Profiles reference this passkey by name; the secret is never shared.</span></div>
          </>
        )}
        {error && <div style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)", marginTop: 4 }}>{error === "type" ? "Type is required." : error === "host" ? "Host is required." : "Port must be 1–65535."}</div>}
      </div>
      {/* footer — full-width separator, buttons inset */}
      <div style={{ flex: "none", borderTop: "1px solid var(--tasty-separator)", padding: "12px 16px", display: "flex", justifyContent: "flex-end", gap: 8 }}>
        <Button variant="ghost" size="sm">Cancel</Button>
        <Button variant="primary" size="sm">Save</Button>
      </div>
    </div>
  );
}

// ── Scripts subsection (Settings › Misc › Scripts) — Lua script manager ──
function ScriptManagerFrame({ empty }) {
  const sIcon = <Icon name="scriptFile" size={16} />;
  const kbdIcon = <Icon name="keyboard" size={16} />;
  const changedBadge = (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 3, height: 16, padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
      fontFamily: "var(--tasty-font-mono)", fontSize: 10, fontWeight: 500, color: "var(--tasty-accent-warning-raised)",
      border: "1px solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
      background: "color-mix(in srgb, var(--tasty-accent-warning) 12%, transparent)" }}>changed</span>
  );
  const xIcon = <Icon name="close" size={12} />;
  const caretIcon = <Icon name="chevronDown" size={12} />;
  // Auto-run trigger chip (static mirror — whole chip is the remove affordance)
  const TriggerChip = ({ event }) => (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 4, height: 16, padding: "0 4px", flex: "none",
      borderRadius: "var(--tasty-radius-sm)", border: "1px solid var(--tasty-border-default)", whiteSpace: "nowrap",
      fontFamily: "var(--tasty-font-mono)", fontSize: 10, lineHeight: 1, color: "var(--tasty-text-secondary)" }}>
      <span>{event}</span><span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{xIcon}</span>
    </span>
  );
  // Auto-run row: caption + chips + dashed add control (menu of remaining events)
  const TriggerRow = ({ triggers }) => (
    <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: 4, marginTop: 2 }}>
      <span style={{ flex: "none", fontSize: 11, color: "var(--tasty-text-muted)" }}>Auto-run:</span>
      {triggers.map((e) => <TriggerChip key={e} event={e} />)}
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4, height: 16, padding: "0 4px", flex: "none",
        borderRadius: "var(--tasty-radius-sm)", border: "1px dashed var(--tasty-border-default)", whiteSpace: "nowrap",
        fontFamily: "var(--tasty-font-mono)", fontSize: 10, lineHeight: 1, color: "var(--tasty-text-muted)" }}>
        <span>Add trigger…</span><span style={{ display: "inline-flex" }}>{caretIcon}</span>
      </span>
    </div>
  );
  const Path = ({ dir, file }) => (
    <span style={{ display: "flex", minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: 12 }}>
      <span style={{ flex: "0 1 auto", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0, color: "var(--tasty-text-muted)" }}>{dir}</span>
      <span style={{ flex: "none", color: "var(--tasty-text-secondary)" }}>{file}</span>
    </span>
  );
  const Row = ({ name, dir, file, shortcut, changed, triggers }) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 12, padding: "8px 4px", borderBottom: "1px solid var(--tasty-separator)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 2, color: "var(--tasty-text-muted)" }}>{sIcon}</span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)" }}>{name}</span>
          {changed && changedBadge}
        </div>
        <Path dir={dir} file={file} />
        {changed && <span style={{ fontSize: 11, color: "var(--tasty-accent-warning-raised)", lineHeight: 1.4 }}>File changed since registration — you'll be asked to confirm on next run.</span>}
        <TriggerRow triggers={triggers || []} />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 6, flex: "none" }}>
        {shortcut ? <Kbd keys={shortcut} /> : <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", fontStyle: "italic" }}>Unbound</span>}
        <IconButton size="sm" aria-label="Bind shortcut">{kbdIcon}</IconButton>
        <IconButton size="sm" aria-label="Rename">{ic.edit}</IconButton>
        <IconButton size="sm" aria-label="Remove">{ic.x}</IconButton>
      </div>
    </div>
  );
  return (
    <div style={{ width: "100%", maxWidth: 560, display: "flex", flexDirection: "column", gap: 14, padding: 18,
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 16, fontWeight: 600, color: "var(--tasty-text-primary)" }}>Scripts</div>
          <p style={{ fontSize: 12, color: "var(--tasty-text-muted)", margin: "2px 0 0", lineHeight: 1.5 }}>Register and manage Lua scripts you can run with a shortcut. Binding a trigger is done in <b style={{ color: "var(--tasty-text-secondary)" }}>Keybindings</b>; each script is verified against the SHA recorded when it was added.</p>
        </div>
        <Button variant="secondary" size="sm" leadingIcon={ic.plus}>Add script</Button>
      </div>
      {empty ? (
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8, textAlign: "center", padding: "32px 0", color: "var(--tasty-text-muted)" }}>
          <Icon name="scriptFile" size={26} />
          <div style={{ fontSize: 14, color: "var(--tasty-text-secondary)" }}>No scripts registered</div>
          <p style={{ fontSize: 12, margin: 0, maxWidth: 300, lineHeight: 1.5 }}>Click <b style={{ color: "var(--tasty-text-secondary)" }}>Add script</b> to register a Lua script and bind it to a shortcut.</p>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <Row name="Reformat JSON" dir="~/.tasty/scripts/" file="reformat-json.lua" shortcut="Ctrl+Shift+J" triggers={["clipboard.copy.post"]} />
          <Row name="Tail & highlight errors" dir="~/.tasty/scripts/" file="tail-errors.lua" shortcut="" triggers={["pane.create.post", "session.start.post"]} />
          <Row name="Deploy staging" dir="~/work/ops/tasty/" file="deploy-staging.lua" shortcut="Ctrl+Alt+D" changed triggers={[]} />
        </div>
      )}
    </div>
  );
}

// ════════════════════════════════════════════════════════════════════════
//  Workspace categories (sidebar folders) — context menu, rail popup, dialogs
//  Static mirrors of ui_kits/terminal/{chrome.jsx, overlays/sidebar_context_menu.jsx}.
// ════════════════════════════════════════════════════════════════════════
const catIc = {
  plus: <Icon name="plus" />,
  edit: <Icon name="edit" />,
  trash: <Icon name="trash" />,
  folder: <Icon name="folder" />,
  move: <Icon name="move" />,
  chevD: <Icon name="chevronDown" size={14} />,
  chevR: <Icon name="chevronRight" size={13} />,
};
const catMenuPanel = { background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
  borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-popup-content-margin)", boxShadow: "var(--tasty-shadow-popover)", minWidth: 176 };

// small labelled menu column (for the "target resolves to N shapes" layout)
function CatMenu({ caption, children }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{caption}</span>
      <div role="menu" style={catMenuPanel}>{children}</div>
    </div>
  );
}

// Faux full sidebar with two category groups + one empty/collapsed category.
function CategorySidebarFrame({ collapsedRail }) {
  const Head = ({ label, collapsed, reserved }) => (
    <div style={{ display: "flex", alignItems: "center", gap: 4, padding: "4px 8px", marginTop: 8, cursor: "pointer" }}>
      <span style={{ display: "inline-flex", width: 12, justifyContent: "center", color: "var(--tasty-text-muted)",
        transform: collapsed ? "none" : "rotate(0deg)" }}>{collapsed ? catIc.chevR : catIc.chevD}</span>
      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".07em", color: "var(--tasty-text-muted)" }}>{label}</span>
    </div>
  );
  const Row = ({ name, sub, status, active }) => (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "8px", cursor: "pointer",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ height: 18, display: "inline-flex", alignItems: "center", flex: "none" }}>
        <StatusDot status={status} pulse={status === "running" || status === "agent"} /></span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 500, color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</div>
        {sub && <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginTop: 1 }}>{sub}</div>}
      </div>
    </div>
  );
  const List = ({ children }) => <div style={{ borderTop: "1px solid var(--tasty-separator)" }}>{children}</div>;
  return (
    <div style={{ width: 212, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", height: 380, display: "flex", flexDirection: "column" }}>
      <Head label="Workspaces" reserved /><List><Row name="agents-prod" status="running" active /><Row name="scratch" status="idle" /></List>
      <Head label="Services" /><List><Row name="infra" sub="terraform + k8s" status="idle" /><Row name="api-gateway" sub="agent" status="agent" /><Row name="data-pipeline" sub="spark · 4 nodes" status="running" /></List>
      <Head label="Archived" collapsed />
      <div style={{ marginTop: "auto", padding: 8, borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost" size="sm" block leadingIcon={catIc.plus} style={{ justifyContent: "flex-start" }}>New Workspace</Button>
      </div>
    </div>
  );
}

// Faux 52px rail: `---` category buttons + letter avatars, with the anchored popup.
function RailCategoryFrame() {
  const Sep = () => <div style={{ width: 24, height: 1, background: "var(--tasty-separator)", margin: "3px 0" }} />;
  const Av = ({ ch, active, dot }) => (
    <div style={{ position: "relative", width: 28, height: 28, borderRadius: "var(--tasty-radius)", display: "flex", alignItems: "center", justifyContent: "center",
      color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", background: active ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: active ? "inset 0 0 0 1px var(--tasty-accent-primary)" : "none", fontFamily: "var(--tasty-font-mono)", fontSize: 13, fontWeight: 700 }}>
      {ch}{dot && <span style={{ position: "absolute", top: -1, right: -1, width: 6, height: 6, borderRadius: "50%", background: "var(--tasty-accent-primary)", boxShadow: "0 0 0 2px var(--tasty-bg-sidebar)" }} />}</div>
  );
  return (
    <div style={{ position: "relative", height: 380, display: "flex" }}>
      <div style={{ width: 52, flex: "none", background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)",
        padding: "10px 0", display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
        <div style={{ width: 24, height: 24, borderRadius: 6, background: "var(--tasty-brand-melon-flesh)", marginBottom: 4 }} />
        <Sep /><Av ch="A" active /><Av ch="S" />
        <div title="Services" style={{ padding: "2px 0", cursor: "pointer" }}><div style={{ width: 24, height: 1, background: "var(--tasty-text-muted)" }} /></div>
        <Av ch="I" /><Av ch="A" dot /><Av ch="D" dot />
        <Sep />{/* Archived — empty, header-only */}
      </div>
      {/* anchored popup to the right of the Services `---` button */}
      <div role="menu" aria-label="Services" style={{ position: "absolute", left: 60, top: 118, ...catMenuPanel }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: 8, padding: "4px 8px 8px", borderBottom: "1px solid var(--tasty-separator)", marginBottom: 4 }}>
          <span style={{ flex: 1, fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)" }}>Services</span>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>3</span>
        </div>
        <MenuItem label="Add workspace" icon={catIc.plus} />
        <MenuItem label="Collapse" icon={catIc.chevD} />
        <MenuItem separator />
        <MenuItem label="Rename category" icon={catIc.edit} />
        <MenuItem label="Delete category" icon={catIc.trash} danger />
      </div>
    </div>
  );
}

// Create / rename category — 360px single-field dialog (Rename pattern) + validation.
function CategoryEditFrame({ mode = "new", error }) {
  const errText = error === "empty" ? "Enter a category name." : error === "reserved" ? "\u201Cnormal\u201D is reserved." : error === "duplicate" ? "A category named \u201CServices\u201D already exists." : null;
  return (
    <div style={{ width: 360, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: "14px 14px 0" }}>
        <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 2 }}>{mode === "new" ? "New category" : "Rename category"}</div>
        <div style={{ fontSize: 12, color: "var(--tasty-text-muted)", marginBottom: 12 }}>Press <Kbd keys="↵" /> to confirm, <Kbd keys="Esc" /> to cancel.</div>
        <Input block autoFocus defaultValue={mode === "new" ? "" : "Services"} placeholder="Category name"
          style={errText ? { borderColor: "var(--tasty-accent-danger)" } : undefined} />
        {errText && <div style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)", marginTop: 6 }}>{errText}</div>}
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14 }}>
        <Button variant="ghost">Cancel</Button>
        <Button variant="primary" disabled={!!error}>{mode === "new" ? "Create" : "Rename"}</Button>
      </div>
    </div>
  );
}

// Delete category — destructive confirm; the category's workspaces move to normal.
function CategoryDeleteFrame() {
  return (
    <div style={{ width: 380, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "14px 14px 0" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-accent-danger-raised)" }}>{catIc.trash}</span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Delete category?</span>
      </div>
      <p style={{ margin: "10px 14px 0", fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>
        Delete <b style={{ color: "var(--tasty-text-primary)" }}>Services</b>? Its <b>3 workspaces</b> aren't deleted — they move back to <b>Workspaces</b>.
      </p>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14, marginTop: 4 }}>
        <Button variant="ghost">Cancel</Button>
        <Button variant="danger">Delete category</Button>
      </div>
    </div>
  );
}

// ── Modifier hint panel — floating, focus-less discoverability overlay ──
// Painted while a modifier is HELD; lists every chord that contains it,
// grouped by chord (size-ascending, then Ctrl→Cmd/Alt→Option→Shift), plus
// any special role the chord carries. Draggable + edge-resizable, an X for a
// this-session dismiss. NEVER takes focus — the muted drag strip (sidebar
// fill, not a titlebar) is the visual promise of that. Content below is the
// "Ctrl held" instance (cross-platform: the same on Win/Linux and macOS).
const mhIc = {
  hash: <Icon name="hash" />,
  mouse: <Icon name="mouse" />,
  folder: <Icon name="folder" />,
};

// chord shown as keycaps + "+" joiners — reuses the Kbd vocabulary as the header
function ChordHead({ keys }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 2px 6px",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <Kbd keys={keys} />
    </div>
  );
}

// one keycap row inside a chord section — action label (elides) + its Kbd
function HintRow({ action, keys, plugin }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, minHeight: "var(--tasty-modhint-row-min-height)" }}>
      {plugin && <span aria-hidden style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)",
        borderRadius: "50%", flex: "none", background: "var(--tasty-accent-agent)" }} />}
      <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-modhint-row-font-size)", color: "var(--tasty-text-secondary)",
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{action}</span>
      <span style={{ flex: "none" }}><Kbd keys={keys} /></span>
    </div>
  );
}

// special-role row — a full-width wash, leading glyph, no keycap. This is the
// "what does holding it DO" line (tab-switch numbers, mouse-capture bypass).
function RoleRow({ icon, children }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "var(--tasty-modhint-row-gap) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)",
      background: "var(--tasty-modhint-role-bg)" }}>
      <span style={{ display: "inline-flex", flex: "none", marginTop: 1, color: "var(--tasty-modhint-role-fg)" }}>{icon}</span>
      <span style={{ flex: 1, fontSize: "var(--tasty-modhint-row-font-size)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-modhint-role-text-fg)" }}>{children}</span>
    </div>
  );
}

// empty-chord placeholder — the row for a chord that EXISTS but carries no
// binding and no role. Deliberately the QUIETEST row in the panel: muted text
// (--tasty-modhint-empty-fg → text-muted, one step below a keycap row), no
// keycap, no wash, no leading glyph. It is an ABSENCE signal, not a role-row,
// so it must never borrow the role-row's wash/glyph emphasis. Static & non-
// interactive (no hover, no focus, no click) — like every row in this overlay.
function EmptyRow({ label = "No shortcuts bound" }) {
  return (
    <div style={{ display: "flex", alignItems: "center", minHeight: "var(--tasty-modhint-empty-row-min-height)", padding: "0 2px" }}>
      <span style={{ fontSize: "var(--tasty-modhint-row-font-size)", color: "var(--tasty-modhint-empty-fg)" }}>{label}</span>
    </div>
  );
}

// the panel body — the "Ctrl held" chord list. A chord with no binding and no
// role is NOT omitted: its ChordHead is shown with a single EmptyRow placeholder
// beneath, so holding an all-empty combo (e.g. Ctrl+Alt+Shift) still shows the
// panel with a "No shortcuts bound" line rather than nothing at all.
// (Reverses the 2026-07-02 decision "empty chord → omitted entirely, never 'none'".)
const MH_SECTIONS = [
  { keys: "Ctrl",
    role: <>Preview <b style={{ color: "var(--tasty-text-primary)" }}>tab-switch numbers</b> — <span style={{ fontFamily: "var(--tasty-font-mono)" }}>1</span>–<span style={{ fontFamily: "var(--tasty-font-mono)" }}>9</span>, <span style={{ fontFamily: "var(--tasty-font-mono)" }}>0</span> over each tab.</>, roleIcon: mhIc.hash,
    rows: [
      { action: "Command palette", keys: "Ctrl+K" },
      { action: "New tab", keys: "Ctrl+T" },
      { action: "Close tab", keys: "Ctrl+W" },
      { action: "Split vertical", keys: "Ctrl+D" },
      { action: "Settings", keys: "Ctrl+," },
    ] },
  { keys: "Ctrl+Alt",
    rows: [
      { action: "Apply workspace preset", keys: "Ctrl+Alt+1" },
      { action: "git-helper: Stage hunk", keys: "Ctrl+Alt+G", plugin: true },
      { action: "docker: Attach shell", keys: "Ctrl+Alt+D", plugin: true },
    ] },
  { keys: "Ctrl+Shift",
    rows: [
      { action: "New workspace", keys: "Ctrl+Shift+N" },
      { action: "Split horizontal", keys: "Ctrl+Shift+D" },
      { action: "Copy", keys: "Ctrl+Shift+C" },
      { action: "Paste", keys: "Ctrl+Shift+V" },
    ] },
];

// Ctrl+Shift held — the category quick-switch chord. Its own function (a numeric
// switch that jumps categories) rides in the special-role row, exactly like
// Ctrl = tab-switch numbers. Only present when the folders feature is on.
const MH_CAT_SECTIONS = [
  { keys: "Ctrl+Shift",
    role: <>Switch <b style={{ color: "var(--tasty-text-primary)" }}>category</b> — <span style={{ fontFamily: "var(--tasty-font-mono)" }}>1</span>–<span style={{ fontFamily: "var(--tasty-font-mono)" }}>9</span>, <span style={{ fontFamily: "var(--tasty-font-mono)" }}>0</span> over each category header; a collapsed target auto-expands.</>, roleIcon: mhIc.folder,
    rows: [
      { action: "New workspace", keys: "Ctrl+Shift+N" },
      { action: "Split horizontal", keys: "Ctrl+Shift+D" },
      { action: "Copy", keys: "Ctrl+Shift+C" },
      { action: "Paste", keys: "Ctrl+Shift+V" },
    ] },
];

// Ctrl held, mixed — filled chords next to empty ones. Ctrl (role + rows) and
// Ctrl+Shift (rows) are bound; Ctrl+Alt and Ctrl+Alt+Shift are empty and draw
// the EmptyRow placeholder. Shows how a filled section and an absence section
// contrast within one list.
const MH_MIXED_SECTIONS = [
  { keys: "Ctrl",
    role: <>Preview <b style={{ color: "var(--tasty-text-primary)" }}>tab-switch numbers</b> — <span style={{ fontFamily: "var(--tasty-font-mono)" }}>1</span>–<span style={{ fontFamily: "var(--tasty-font-mono)" }}>9</span>, <span style={{ fontFamily: "var(--tasty-font-mono)" }}>0</span> over each tab.</>, roleIcon: mhIc.hash,
    rows: [
      { action: "Command palette", keys: "Ctrl+K" },
      { action: "New tab", keys: "Ctrl+T" },
      { action: "Close tab", keys: "Ctrl+W" },
    ] },
  { keys: "Ctrl+Alt", rows: [] },
  { keys: "Ctrl+Shift",
    rows: [
      { action: "New workspace", keys: "Ctrl+Shift+N" },
      { action: "Split horizontal", keys: "Ctrl+Shift+D" },
      { action: "git-helper: Stage hunk", keys: "Ctrl+Shift+G", plugin: true },
    ] },
  { keys: "Ctrl+Alt+Shift", rows: [] },
];

// Ctrl+Alt+Shift held — the all-empty case that motivated the empty-state: no
// superset chord carries a binding, so the panel is nothing but ChordHead +
// placeholder (previously it never appeared at all).
const MH_EMPTY_SECTIONS = [
  { keys: "Ctrl+Alt+Shift", rows: [] },
];

function ModifierHintPanelG({ style, held = "Ctrl", sections = MH_SECTIONS, showGrip = true }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", flex: "none",
      width: "var(--tasty-modhint-width)", height: "var(--tasty-modhint-height)",
      background: "var(--tasty-modhint-bg)", border: "var(--tasty-border-width) solid var(--tasty-modhint-border)",
      borderRadius: "var(--tasty-modhint-radius)", boxShadow: "var(--tasty-modhint-shadow)", overflow: "hidden",
      position: "relative", ...style }}>
      {/* drag strip — muted (sidebar) fill, NOT a focus titlebar */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, flex: "none", height: "var(--tasty-modhint-header-height)",
        padding: "0 4px 0 10px", cursor: "move", userSelect: "none",
        background: "var(--tasty-modhint-header-bg)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <Kbd keys={held} />
        <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>held</span>
        <span style={{ marginLeft: "auto" }}>
          <IconButton size="sm" aria-label="Hide for this hold">{ic.x}</IconButton>
        </span>
      </div>
      {/* scrolling chord list */}
      <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "10px",
        display: "flex", flexDirection: "column", gap: "var(--tasty-modhint-section-gap)" }}>
        {sections.map((s) => {
          const isEmpty = !s.role && (!s.rows || s.rows.length === 0);
          return (
          <div key={s.keys} style={{ display: "flex", flexDirection: "column", gap: isEmpty ? 3 : 6 }}>
            <ChordHead keys={s.keys} />
            {s.role && <RoleRow icon={s.roleIcon}>{s.role}</RoleRow>}
            {(s.rows || []).map((r) => <HintRow key={r.keys} {...r}
              keys={r.keys.startsWith(s.keys + "+") ? r.keys.slice(s.keys.length + 1) : r.keys} />)}
            {isEmpty && <EmptyRow />}
          </div>
          );
        })}
      </div>
      {/* bottom-right resize grip */}
      {showGrip && (
        <span aria-hidden style={{ position: "absolute", right: "var(--tasty-modhint-grip-inset)", bottom: "var(--tasty-modhint-grip-inset)", width: "var(--tasty-modhint-grip-size)", height: "var(--tasty-modhint-grip-size)", cursor: "nwse-resize" }}>
          <svg viewBox="0 0 12 12" width="100%" height="100%" fill="none"
            stroke="var(--tasty-modhint-grip-fg)" strokeWidth="1" strokeLinecap="round">
            <path d="M11 5 5 11M11 9 9 11" />
          </svg>
        </span>
      )}
    </div>
  );
}

// ── Add remote workspace — 680×460 two-pane remote-workspace picker (NEW) ──
// Consumes the tasty-attach profiles (left) → lists a remote instance's workspaces
// (right) across 4 states. Same shell language as RemoteFrame (remote_tool).
function RaBadge({ tone, children, title }) {
  // The Tag recipe: its own fill keeps the ink readable on a selected row.
  const attached = tone === "attached";
  const style = attached
    ? { color: "var(--tasty-tag-attached-fg)", borderColor: "color-mix(in srgb, var(--tasty-accent-attached) 40%, transparent)" }
    : { color: "var(--tasty-tag-warning-fg)" };
  return (
    <Tag variant={attached ? "attached" : "warning"} title={title} style={style}>
      {!attached && <span style={{ display: "inline-flex" }}>{ic.warn}</span>}{children}
    </Tag>
  );
}
function RaProfile({ name, label, target, selected, inactive }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2, padding: "var(--tasty-space-sm) var(--tasty-space-md)", position: "relative",
      background: selected ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: selected ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>
          {name}{label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({label})</span>}
        </span>
        {inactive && <RaBadge title="Inactive">inactive</RaBadge>}
      </div>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{target}</div>
    </div>
  );
}
function RaWs({ name, panes, busy, selected, attached, muted }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "var(--tasty-space-sm) var(--tasty-space-md)", position: "relative",
      background: selected ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: selected ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ display: "inline-flex" }}><StatusDot status={busy ? "running" : "idle"} pulse={busy} /></span>
      <span style={{ fontSize: 13, fontWeight: 500, color: attached || muted ? "var(--tasty-text-muted)" : selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>{name}</span>
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 11, color: "var(--tasty-text-muted)" }}>{ic.split}{panes}</span>
      <div style={{ flex: 1 }} />
      {busy && !attached && <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>busy</span>}
      {attached && <RaBadge tone="attached" title="Already attached on another client">in use</RaBadge>}
    </div>
  );
}
function RaNewRow({ selected, hover, phase = "rest", error, sep = true }) {
  const creating = phase === "creating";
  const failed = phase === "failed";
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "var(--tasty-space-sm) var(--tasty-space-md)", position: "relative",
        background: selected ? "var(--tasty-surface-active)" : hover ? "var(--tasty-overlay-hover)" : "transparent",
        boxShadow: selected ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
        <span style={{ flex: "none", width: "var(--tasty-status-dot-size)", display: "inline-flex", alignItems: "center", justifyContent: "center",
          color: creating ? "var(--tasty-text-muted)" : failed ? "var(--tasty-accent-danger-raised)" : "var(--tasty-accent-primary-raised)" }}>
          {creating ? <Spinner size={14} /> : <Icon name={failed ? "alertTriangle" : "plus"} size={14} />}
        </span>
        <span style={{ fontSize: 13, fontWeight: 500, whiteSpace: "nowrap",
          color: creating ? "var(--tasty-text-muted)" : selected ? "var(--tasty-text-primary)" : "var(--tasty-accent-primary-raised)" }}>
          {creating ? "Creating workspace…" : "New workspace"}
        </span>
        <div style={{ flex: 1 }} />
        {!creating && !failed && <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>on remote</span>}
      </div>
      {failed && (
        <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "var(--tasty-space-xs) var(--tasty-space-md) var(--tasty-space-sm)" }}>
          <span style={{ flex: "none", width: "var(--tasty-status-dot-size)" }} />
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 4 }}>
            <span style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)", lineHeight: 1.5,
              display: "-webkit-box", WebkitLineClamp: 3, WebkitBoxOrient: "vertical", overflow: "hidden" }}>
              {error || "Remote refused: workspace quota reached (8/8) on edge.example.com. Close a workspace there, or raise `limits.workspaces` in the remote's tasty.toml, then try again."}
            </span>
            <Button variant="secondary" size="sm" leadingIcon={ic.refresh}>Try again</Button>
          </div>
        </div>
      )}
      {sep && <div style={{ height: 1, margin: "var(--tasty-space-xs) 0", background: "var(--tasty-separator)" }} />}
    </div>
  );
}
// one real remote-ws row, for the new-workspace row-state specimens (alignment check)
function RaWsPeek({ muted }) {
  return <RaWs name="agents-prod" panes={3} busy muted={muted} />;
}
function RemoteAttachFrame({ state = "loaded", newPhase = "rest", newSelected, emptyPlan = "B" }) {
  const caps = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" };
  const center = (kids) => (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      textAlign: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xl) var(--tasty-space-lg)" }}>{kids}</div>
  );
  const errSel = state === "error";
  return (
    <div style={{ width: 680, height: 460, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      {/* header */}
      {/* section insets (2026-09-29): every section starts on fp-inset-start (12) — header glyph, first crumb, Name, row icon and footer label share one column */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-fp-section-gap)", padding: "var(--tasty-fp-header-pad-y) var(--tasty-fp-inset-end) var(--tasty-fp-header-pad-y) var(--tasty-fp-inset-start)", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.remote}</span>
        <span style={{ fontSize: 14, fontWeight: 600, color: "var(--tasty-text-primary)" }}>Add remote workspace</span>
        <div style={{ flex: 1 }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      {/* body */}
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* left */}
        <div style={{ width: 240, flex: "none", display: "flex", flexDirection: "column", borderRight: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
          <div style={{ ...caps, padding: "10px 12px 4px" }}>Attach profiles</div>
          <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
            <RaProfile name="prod-web" label="us-east" target="deploy@10.0.4.12" selected={state === "loaded"} />
            <RaProfile name="gb10" target="→ prod-web" selected={state === "loading"} />
            <RaProfile name="edge-direct" target="root@edge.example.com" />
            <RaProfile name="media-nas" label="lab" target="→ nas.local" selected={state === "empty"} />
            <RaProfile name="legacy-attach" target="→ legacy-box" inactive selected={errSel} />
          </div>
        </div>
        {/* right */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          {state === "initial" && <CenterState variant="empty" glyph="remote" title="Select an attach profile" sub="Pick a profile on the left to connect and list the remote instance's workspaces." />}
          {state === "loading" && <CenterState variant="loading" title="Connecting…" sub={<>Establishing the SSH tunnel to <span style={{ fontFamily: "var(--tasty-font-mono)" }}>gb10</span> and listing workspaces. This can take a few seconds.</>} />}
          {state === "error" && <CenterState variant="error" title="Can't connect" sub="SSH authentication failed — passkey “old-rsa” was rejected by legacy-box." action={<Button variant="secondary" size="sm" leadingIcon={<Icon name="refresh" size="var(--tasty-icon-size-sm)" />}>Retry</Button>} />}
          {state === "loaded" && (<>
            <div style={{ ...caps, padding: "10px 12px 4px", display: "flex", alignItems: "center", gap: 8 }}>
              <span>Remote workspaces</span><span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-text-disabled)" }}>·</span>
              <span style={{ textTransform: "none", letterSpacing: 0, fontFamily: "var(--tasty-font-ui)", fontSize: 11 }}>prod-web</span>
            </div>
            <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
              <RaNewRow phase={newPhase} selected={newSelected || newPhase !== "rest"} />
              <RaWs name="agents-prod" panes={3} busy selected={!newSelected && newPhase === "rest"} />
              <RaWs name="api-gateway" panes={2} attached />
              <RaWs name="scratch" panes={1} />
            </div>
          </>)}
          {state === "empty" && emptyPlan === "B" && (<>
            <div style={{ ...caps, padding: "10px 12px 4px", display: "flex", alignItems: "center", gap: 8 }}>
              <span>Remote workspaces</span><span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-text-disabled)" }}>·</span>
              <span style={{ textTransform: "none", letterSpacing: 0, fontFamily: "var(--tasty-font-ui)", fontSize: 11 }}>media-nas</span>
            </div>
            <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
              <RaNewRow selected />
              <div style={{ display: "flex", gap: 8, padding: "var(--tasty-space-xs) var(--tasty-space-md)" }}>
                <span style={{ flex: "none", width: "var(--tasty-status-dot-size)" }} />
                <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)" }}>media-nas</span> is reachable but has no workspaces yet.
                </span>
              </div>
            </div>
          </>)}
          {state === "empty" && emptyPlan === "A" && center(<>
            <span style={{ display: "inline-flex", color: "var(--tasty-text-placeholder)", transform: "scale(1.4)" }}><Icon name="paneEmpty" size={16} /></span>
            <span style={{ fontSize: 13, color: "var(--tasty-text-muted)" }}>No workspaces on this remote yet</span>
            <span style={{ fontSize: 11, color: "var(--tasty-text-muted)", maxWidth: 300, lineHeight: 1.5 }}><span style={{ fontFamily: "var(--tasty-font-mono)" }}>media-nas</span> is reachable. Create one there and mirror it here.</span>
            <span style={{ marginTop: 4 }}><Button variant="secondary" size="sm" leadingIcon={<Icon name="plus" size={14} />}>New workspace</Button></span>
          </>)}
        </div>
      </div>
      {/* footer */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8, flex: "none", padding: "10px 20px", borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost">Cancel</Button>
        <Button variant="primary" disabled={newPhase === "creating" || (state !== "loaded" && state !== "empty")}>
          {(newSelected || newPhase !== "rest" || state === "empty") ? "Create & connect" : "Connect"}
        </Button>
      </div>
    </div>
  );
}

// ── Native file picker — 640×480 modal "Open file" dialog ──────────────
// A select-and-confirm dialog (NOT the Explorer surface): browse a dir,
// pick a path, hand it back, close. One component, two modes — local vs.
// a remote attach host — differing only in the header host indicator and
// the breadcrumb root. Popup language: Scrim · content-drawn header ·
// --tasty-bg-panel frame, same as remote_tool / Add remote workspace.
// inert (2026-10-21, folder mode): a file row that is listed for orientation but can't be selected —
// ink state-disabled-fg (never opacity), no hover overlay, skipped by ↑↓, tooltip "Only folders can be selected."
function FpRow({ kind, name, size, mod, selected, focus, multi, checked, dim, inert, tip }) {
  const isFolder = kind === "folder";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-fp-row-pad-y) var(--tasty-fp-inset-start)", position: "relative", cursor: "default",
      background: selected ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: selected ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none",
      outline: focus ? "1px solid var(--tasty-accent-primary)" : "none", outlineOffset: -1,
      opacity: dim ? 0.5 : 1 }}>
      {multi && <span style={{ display: "inline-flex", flex: "none" }}><Checkbox checked={!!checked} /></span>}
      <span style={{ flex: "none", display: "inline-flex", color: inert ? "var(--tasty-fp-folder-file-fg)" : isFolder ? "var(--tasty-accent-primary-raised)" : "var(--tasty-text-muted)" }}>
        <Icon name={isFolder ? "folder" : "file"} size={16} />
      </span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        fontSize: 13, fontWeight: isFolder ? 500 : 400, color: inert ? "var(--tasty-fp-folder-file-fg)" : selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>{name}</span>
      <span style={{ flex: "none", width: 68, textAlign: "right", fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: inert ? "var(--tasty-fp-folder-file-fg)" : selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)" }}>{size}</span>
      <span style={{ flex: "none", width: 108, textAlign: "right", fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: inert ? "var(--tasty-fp-folder-file-fg)" : selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)" }}>{mod}</span>
      {tip && (
        <span role="tooltip" style={{ position: "absolute", left: "calc(var(--tasty-fp-inset-start) + var(--tasty-size-96))", top: "100%", marginTop: "var(--tasty-space-xs)", zIndex: 4, whiteSpace: "nowrap",
          padding: "var(--tasty-tooltip-padding-y) var(--tasty-tooltip-padding-x)", background: "var(--tasty-tooltip-bg)", border: "var(--tasty-border-width) solid var(--tasty-tooltip-border)",
          borderRadius: "var(--tasty-tooltip-radius)", boxShadow: "var(--tasty-tooltip-shadow)", color: "var(--tasty-tooltip-fg)", fontSize: "var(--tasty-tooltip-font-size)", lineHeight: "var(--tasty-tooltip-line-height)" }}>{tip}</span>
      )}
    </div>
  );
}
// Overflow rule: the path bar NEVER pushes the footer or the refresh button.
// Deep paths elide in the MIDDLE — root + … + the last two segments (the
// current folder and its parent are the two that orient you) — and the … is
// a menu that lists the hidden ancestors.
// Allocation (settled 2026-09-17): the bar measures the width LEFT OVER after
// the trailing buttons and their gap — never the popup width. Priority, highest
// first: current folder → parent → root → the … menu. So the squeeze runs in
// this order, each step only when the previous one has hit its floor:
//   1 ancestors fold into the … menu, one per step (already approved)
//   2 the parent crumb shrinks 180 → 64 (--tasty-fp-crumb-min-width)
//   3 the current folder shrinks 180 → 96 (--tasty-fp-crumb-current-min-width)
//   4 the parent folds into the … menu too, leaving root › … › current
//   5 root folds in as well — SINGLE-CRUMB mode: … › current, the floor of the
//     design. The current folder is never clipped without a visible ellipsis.
// Growing back needs 8px more than the floor (--tasty-fp-bar-hysteresis) so a
// drag-resize cannot flap. The current folder elides at the FRONT ("…-bbbb"):
// its tail is what tells two sibling folders apart. Ancestors elide at the tail.
function FpCrumbs({ items, elide = false, single = false }) {
  const shown = single
    ? [{ label: "…", hidden: items.length - 1 }, items[items.length - 1]]
    : elide && items.length > 3
      ? [items[0], { label: "…", hidden: items.length - 3 }, ...items.slice(-2)]
      : items;
  const alloc = (it, i, n) => {
    if (it.hidden) return { flex: "none" };                       // the … menu never shrinks
    if (it.current) return { flex: "0 1 auto", minWidth: "var(--tasty-fp-crumb-current-min-width)", maxWidth: "var(--tasty-fp-crumb-max-width)" };
    if (it.root) return { flex: "0 1 auto", minWidth: 0, maxWidth: "var(--tasty-fp-crumb-max-width)" };
    return { flex: "0 1 auto", minWidth: "var(--tasty-fp-crumb-min-width)", maxWidth: "var(--tasty-fp-crumb-max-width)" };
  };
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2, minWidth: 0, flex: 1, flexWrap: "nowrap", overflow: "hidden" }}>
      {shown.map((it, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span style={{ flex: "none", display: "inline-flex", color: "var(--tasty-text-disabled)" }}><Icon name="chevronRight" size={13} /></span>}
          <span title={it.hidden ? "Show " + it.hidden + (it.hidden === 1 ? " hidden folder" : " hidden folders") : undefined} style={{ ...alloc(it, i, shown.length), overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            fontSize: "var(--tasty-font-size-caption)", fontFamily: it.root ? "var(--tasty-font-mono)" : "var(--tasty-font-ui)",
            cursor: it.current ? "default" : "pointer",
            color: it.current ? "var(--tasty-text-primary)" : "var(--tasty-accent-primary-raised)" }}>{it.label}</span>
        </React.Fragment>
      ))}
    </div>
  );
}
// The … menu: the hidden ancestors, in path order, one row each. Width is
// content-measured inside a band (min = one crumb's measure, max = the popover
// ceiling) so a short path doesn't get a wide menu and a long one truncates.
function FpCrumbMenu({ items, opaque = false, style }) {
  return (
    <div style={{ position: "absolute", top: "100%", left: 44, marginTop: 4, zIndex: 5,
      minWidth: "var(--tasty-fp-crumb-menu-min-width)", maxWidth: "var(--tasty-fp-crumb-menu-max-width)",
      boxSizing: "border-box", padding: "var(--tasty-popup-content-margin)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)",
      borderRadius: opaque ? "var(--tasty-webview-opaque-radius)" : "var(--tasty-menu-radius)", boxShadow: opaque ? "var(--tasty-webview-opaque-shadow)" : "var(--tasty-shadow-popover)", ...style }}>
      {items.map((l, i) => (
        <div key={l} style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-menu-item-height)", padding: "0 var(--tasty-menu-item-padding-x)", borderRadius: "var(--tasty-menu-item-radius)",
          fontSize: "var(--tasty-font-size-body)", background: i === 0 ? "var(--tasty-menu-item-bg-hover)" : "transparent", color: i === 0 ? "var(--tasty-menu-item-fg-hover)" : "var(--tasty-menu-item-fg)" }}>
          <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-primary-raised)" }}><Icon name="folder" size="var(--tasty-icon-size-md)" /></span>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{l}</span>
        </div>
      ))}
    </div>
  );
}
// host indicator chip shown in the header on remote mode (indicator="badge")
function FpHostBadge({ host }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5, height: 22, padding: "0 8px", borderRadius: "var(--tasty-radius)",
      background: "color-mix(in srgb, var(--tasty-accent-info) 14%, transparent)", border: "1px solid color-mix(in srgb, var(--tasty-accent-info) 45%, transparent)",
      color: "var(--tasty-accent-info-raised)", fontFamily: "var(--tasty-font-mono)", fontSize: 11, whiteSpace: "nowrap" }}>
      <span style={{ display: "inline-flex" }}><Icon name="remote" size={13} /></span>{host}
    </span>
  );
}
// 2026-10-21 — folder mode: mode="folder" returns a FOLDER path.
// Target = the selected folder row, else the current folder. Files are listed but inert. Filter chip hidden.
// Enter = Select folder (confirms the target) · double-click / → = enter the folder · Alt+↑ / Up = parent.
// Copy keys picker.folder.* (en / ko / ja); the gallery passes lang to check widths.
const FP_FOLDER_COPY = {
  en: { title: "Select folder", label: "Folder", confirm: "Select folder", cancel: "Cancel", hintCur: "Using the current folder. Select a subfolder to use it instead.",
    hintSel: (n) => <>Double-click <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{n}</span> to open it.</>, fileTip: "Only folders can be selected.", empty: "No subfolders here" },
  ko: { title: "폴더 선택", label: "폴더", confirm: "폴더 선택", cancel: "취소", hintCur: "현재 폴더를 사용합니다. 하위 폴더를 고르면 그 폴더를 사용합니다.",
    hintSel: (n) => <><span style={{ fontFamily: "var(--tasty-font-mono)" }}>{n}</span> 폴더를 열려면 더블클릭하세요.</>, fileTip: "폴더만 선택할 수 있습니다.", empty: "하위 폴더가 없습니다" },
  ja: { title: "フォルダーを選択", label: "フォルダー", confirm: "フォルダーを選択", cancel: "キャンセル", hintCur: "現在のフォルダーを使用します。サブフォルダーを選ぶとそのフォルダーを使用します。",
    hintSel: (n) => <><span style={{ fontFamily: "var(--tasty-font-mono)" }}>{n}</span> を開くにはダブルクリックします。</>, fileTip: "選択できるのはフォルダーだけです。", empty: "サブフォルダーはありません" },
};
function FilePickerFrame({ state = "loaded", remote = false, indicator = "badge", multi = false, filters = [], mode = "open", save = "new", deep = false, crumbMenu = false, folderSel = false, w = 640, h = 480, pathKind, single = false, elideStage,
  folderPick = "none", fileHover = false, lang = "en", opaque = false }) {
  const host = "deploy@10.0.4.12";
  const saving = mode === "save";
  const folderMode = mode === "folder";
  const FC = FP_FOLDER_COPY[lang] || FP_FOLDER_COPY.en;
  const deepTail = [{ label: "tasty" }, { label: "config" }, { label: "keybindings" }, { label: "exports" }, { label: "2026-09", current: true }];
  const LONG_PARENT = "long-folder-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const LONG_CURRENT = "current-folder-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
  const crumbs = pathKind === "longtwo"
    ? [{ label: "/", root: true }, { label: "Users" }, { label: "maya" }, { label: LONG_PARENT }, { label: LONG_CURRENT, current: true }]
    : pathKind === "longroot"
      ? [{ label: "\\\\build-farm-eu-west\\releases$", root: true }, { label: "2026" }, { label: "09" }, { label: "nightly", current: true }]
      : remote
    ? [{ label: host, root: true }, { label: "home" }, { label: "deploy" }, { label: "agents-prod", current: true }]
    : deep
      ? [{ label: "/", root: true }, { label: "Users" }, { label: "maya" }, ...deepTail]
      : [{ label: "/", root: true }, { label: "Users" }, { label: "maya" }, { label: "projects", current: true }];
  const borderMode = remote && indicator === "border";
  const files = [
    { kind: "folder", name: "configs", size: "—", mod: "Jul 12 09:14" },
    { kind: "folder", name: "logs", size: "—", mod: "Jul 14 22:03" },
    { kind: "folder", name: "node_modules", size: "—", mod: "Jul 02 11:40" },
    { kind: "file", name: "README.md", size: "4.2 KB", mod: "Jul 15 08:21", pick: true },
    { kind: "file", name: "package.json", size: "1.1 KB", mod: "Jul 15 08:21", pick: true },
    { kind: "file", name: "pipeline.yaml", size: "3.8 KB", mod: "Jul 14 17:55", pick: true },
    { kind: "file", name: "deploy.sh", size: "902 B", mod: "Jul 11 14:02" },
    { kind: "file", name: ".env", size: "218 B", mod: "Jul 09 10:30" },
  ];
  const fileName = multi ? "README.md, package.json, pipeline.yaml" : "README.md";
  // save mode: the INPUT is the only target. Picking a list row just writes its
  // name into the input — and since every listed name exists, a pick IS the
  // overwrite state (save="picked"). Editing the name away from the picked row
  // clears the selection and the confirm goes back to Save (save="edited").
  const saveName = save === "picked" ? "pipeline.yaml" : save === "edited" ? "pipeline-v2.yaml" : "keybindings-2026-09-14.toml";
  const saveSelected = save === "picked" ? "pipeline.yaml" : null;
  const overwrite = saving && save === "picked";
  const canOpen = (state === "loaded" || state === "empty" ? state === "loaded" : false);
  const center = (kids) => (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      textAlign: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xl) var(--tasty-space-lg)" }}>{kids}</div>
  );
  const errText = state === "error-conn"
    ? { title: "Remote connection lost", body: <>The SSH tunnel to <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{host}</span> dropped. Reconnect to resume browsing from the last folder.</> }
    : { title: "Permission denied", body: <>You don't have permission to read <span style={{ fontFamily: "var(--tasty-font-mono)" }}>/home/deploy/agents-prod</span>. Try a different folder or check access.</> };
  return (
    <div style={{ width: w, height: h, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: borderMode ? "1px solid var(--tasty-accent-info)" : "1px solid var(--tasty-border-strong)",
      borderRadius: opaque ? "var(--tasty-webview-opaque-radius)" : "var(--tasty-radius)", overflow: "hidden", boxShadow: opaque ? "var(--tasty-webview-opaque-shadow)" : "var(--tasty-shadow-modal)" }}>
      {borderMode && <div style={{ height: 2, flex: "none", background: "var(--tasty-accent-info)" }} />}
      {/* header */}
      {/* section insets (2026-09-29): every section starts on fp-inset-start (12) — header glyph, first crumb, Name, row icon and footer label share one column */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-fp-section-gap)", padding: "var(--tasty-fp-header-pad-y) var(--tasty-fp-inset-end) var(--tasty-fp-header-pad-y) var(--tasty-fp-inset-start)", flex: "none", borderBottom: "1px solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: remote && indicator === "glyph" ? "var(--tasty-accent-info-raised)" : "var(--tasty-text-muted)" }}>
          {remote && (indicator === "glyph" || indicator === "border") ? <Icon name="remote" /> : <Icon name={folderMode ? "folder" : "file"} />}
        </span>
        <span style={{ fontSize: 14, fontWeight: 600, color: "var(--tasty-text-primary)" }}>{folderMode ? FC.title : saving ? "Save file" : "Open file"}</span>
        {remote && indicator === "badge" && <FpHostBadge host={host} />}
        {remote && indicator === "glyph" && <span style={{ fontSize: 12, fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-accent-info-raised)" }}>{host}</span>}
        <div style={{ flex: 1 }} />
        <IconButton size="sm" aria-label="Close">{ic.x}</IconButton>
      </div>
      {/* path bar */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-fp-section-gap)", padding: "var(--tasty-fp-path-pad-y) var(--tasty-fp-inset-end) var(--tasty-fp-path-pad-y) var(--tasty-fp-inset-start)", flex: "none", position: "relative", borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
        <FpCrumbs items={crumbs} elide={elideStage ?? (deep || pathKind === "longtwo")} single={single} />
        <IconButton size="sm" aria-label="Up one folder"><Icon name="chevronUp" size="var(--tasty-icon-size-sm)" /></IconButton>
        <IconButton size="sm" aria-label="Refresh">{ic.refresh}</IconButton>
        {crumbMenu && <FpCrumbMenu items={crumbs.slice(1, -2).map((c) => c.label)} />}
      </div>
      {/* list header */}
      {state === "loaded" && (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-fp-list-head-pad-y) var(--tasty-fp-inset-start)", flex: "none", borderBottom: "1px solid var(--tasty-separator)",
          fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>
          {multi && <span style={{ width: 16, flex: "none" }} />}
          <span style={{ width: 16, flex: "none" }} /><span style={{ flex: 1 }}>Name</span>
          <span style={{ width: 68, textAlign: "right", flex: "none" }}>Size</span>
          <span style={{ width: 108, textAlign: "right", flex: "none" }}>Modified</span>
        </div>
      )}
      {/* body */}
      <div style={{ flex: 1, minHeight: 0, overflow: "hidden", display: "flex", flexDirection: "column" }}>
        {state === "loaded" && (
          <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
            {files.map((f, i) => (
              <FpRow key={f.name} {...f}
                inert={folderMode && f.kind === "file"}
                tip={folderMode && fileHover && f.name === "README.md" ? FC.fileTip : null}
                multi={multi}
                checked={multi && f.pick && ["README.md", "package.json", "pipeline.yaml"].includes(f.name)}
                selected={folderMode ? folderPick === "row" && f.name === "configs" : folderSel ? f.name === "configs" : saving ? f.name === saveSelected : !multi && f.name === "README.md"}
                focus={!folderMode && !saving && !multi && !folderSel && f.name === "pipeline.yaml"} />
            ))}
          </div>
        )}
        {state === "loading" && <CenterState variant="loading" title="Loading folder…" sub={remote ? <>Reading <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{host}</span> over SSH.</> : "Reading the directory contents."} />}
        {(state === "error-perm" || state === "error-conn") && <CenterState variant="error" title={errText.title} sub={errText.body}
          action={<Button variant="secondary" size="sm" leadingIcon={<Icon name="refresh" size="var(--tasty-icon-size-sm)" />}>{state === "error-conn" ? "Reconnect" : "Retry"}</Button>} />}
        {state === "empty" && <CenterState variant="empty" glyph="folderOpen" title={folderMode ? FC.empty : "This folder is empty"} />}
      </div>
      {/* footer */}
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-fp-section-gap)", flex: "none", padding: "var(--tasty-fp-footer-pad-y) var(--tasty-fp-inset-start)", borderTop: "1px solid var(--tasty-separator)" }}>
        {folderMode && (() => {
          const cur = "/" + crumbs.slice(1).map((c) => c.label).join("/");
          const target = folderPick === "row" ? cur + "/configs" : cur;
          const live = state === "loaded" || state === "empty";
          return (<>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-fp-section-gap)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", flex: "none", width: "var(--tasty-fp-footer-label-width)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{FC.label}</span>
              {/* read-only path readout: Input readonly box, mono, FRONT ellipsis (the tail names the folder), full path as tooltip */}
              <span title={target} style={{ flex: 1, minWidth: 0, height: "var(--tasty-control-height)", display: "flex", alignItems: "center", boxSizing: "border-box", padding: "0 var(--tasty-space-md)",
                background: "var(--tasty-input-readonly-bg)", border: "var(--tasty-border-width) solid var(--tasty-input-readonly-border)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: "rtl", textAlign: "left",
                  fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: live ? "var(--tasty-fp-folder-path-fg)" : "var(--tasty-text-muted)" }}>
                  <span style={{ direction: "ltr", unicodeBidi: "isolate" }}>{target}</span>
                </span>
              </span>
            </div>
            {live && (
              <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-micro)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-fp-folder-hint-fg)" }}>
                <span style={{ display: "inline-flex", flex: "none", height: "calc(var(--tasty-font-size-micro) * var(--tasty-line-height-ui))", alignItems: "center" }}><Icon name="folder" size="var(--tasty-icon-size-sm)" /></span>
                <span style={{ minWidth: 0, overflowWrap: "anywhere" }}>{folderPick === "row" ? FC.hintSel("configs") : FC.hintCur}</span>
              </div>
            )}
          </>);
        })()}
        {!folderMode && <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-fp-section-gap)" }}>
          <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", flex: "none", width: "var(--tasty-fp-footer-label-width)" }}>File name</span>
          <span style={{ flex: 1, minWidth: 0, display: "flex" }}>
            <Input block defaultValue={saving ? saveName : folderSel ? "" : state === "loaded" ? fileName : ""} placeholder={saving ? "Type a file name" : "No file selected"} />
          </span>
          {filters.length > 0 && (() => {
            /* 2026-10-06 — READ-ONLY readout of the caller's filter: no chevron, no hover, not focusable.
               Hidden when the caller passes no filter. Content width up to fp-filter-max-width,
               then end-ellipsis; the tooltip carries the full list. */
            const label = filters.map((e) => "*." + e).join(", ");
            return (
              <span title={"Showing " + label} style={{ flex: "none", display: "inline-flex", alignItems: "center", height: "var(--tasty-fp-filter-height)", maxWidth: "var(--tasty-fp-filter-max-width)", padding: "0 var(--tasty-space-sm)", boxSizing: "border-box",
                border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", cursor: "default" }}>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }}>{label}</span>
              </span>
            );
          })()}
        </div>}
        {folderSel && (
          <div style={{ display: "flex", alignItems: "flex-start", gap: 6, fontSize: 11, lineHeight: 1.5, color: "var(--tasty-text-muted)" }}>
            <span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><Icon name="folder" size={13} /></span>
            <span>{saving
              ? <>Folders aren't save targets — double-click <span style={{ fontFamily: "var(--tasty-font-mono)" }}>configs</span> to open it.</>
              : <><span style={{ fontFamily: "var(--tasty-font-mono)" }}>configs</span> is a folder — <b style={{ fontWeight: 600 }}>Open</b> enters it.</>}</span>
          </div>
        )}
        {overwrite && (
          <div style={{ display: "flex", alignItems: "flex-start", gap: 6, fontSize: 11, lineHeight: 1.5, color: "var(--tasty-accent-warning-raised)" }}>
            <span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><Icon name="alertTriangle" size={13} /></span>
            <span><span style={{ fontFamily: "var(--tasty-font-mono)" }}>{saveName}</span> already exists in this folder. Saving replaces it.</span>
          </div>
        )}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {multi && <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>3 selected</span>}
          <div style={{ flex: 1, minWidth: 0 }} />
          <span style={{ flex: "none", display: "flex" }}><Button variant="ghost">{folderMode ? FC.cancel : "Cancel"}</Button></span>
          <span style={{ flex: "none", display: "flex" }}>
            <Button variant="primary" disabled={folderMode ? !(state === "loaded" || state === "empty") : !saving && !canOpen}>{folderMode ? FC.confirm : saving ? (overwrite ? "Overwrite" : "Save") : "Open"}</Button>
          </span>
        </div>
      </div>
    </div>
  );
}

export { ic, Backdrop, GroundThemes, GroundCap, PaletteFrame, ApprovalFrame, RenameFrame, SettingsFrame, SettingsGeneralOverlayFrame, SettingsRemoteTransferFrame, TransferProgressFrame, TransferErrorFrame, ToastDragValue, ToolsMenuFrame, PortsFrame, PORTS_COLUMNS, PortsFavoritesG, PortStarG, RemoteFrame, LocalSshSection, SearchBarFrame, FileHandlerFrame, FhFooter, PresetFrame, MarkdownOpenFrame, HeldLabel, TabStripMock, SidebarMock, RailMock, CatSwitchSidebarMock, CatSwitchRailMock, BannerShellG, BannerScope, MouseCaptureBannerG, MouseCaptureHitZone, BlacklistEditorG, TtlBannerG, StackDemoG, BannerMoreMenuG, BannerMoreDemoG, GitViewerFrame, ClipboardFrame, RemoteFormFrame, ScriptManagerFrame, ShellMock, RemoteAttachFrame, RaNewRow, RaWsPeek, FilePickerFrame, FpCrumbMenu, ModifierHintPanelG, MH_CAT_SECTIONS, MH_MIXED_SECTIONS, MH_EMPTY_SECTIONS, catIc, CatMenu, RailCategoryFrame, CategoryEditFrame, CategoryDeleteFrame };
