import React from "react";
import { MenuItem, Button, Icon } from "../../components/index.js";
import { ConvertSurfacePopup as Kit_ConvertSurfacePopup } from "../../ui_kits/terminal/overlays/info_modal.jsx";
import { PaletteFrame, FilePickerFrame, FpCrumbMenu } from "../parts/overlay-frames.jsx";
import { Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Popups › Over a web page, opaque mode (2026-10-21)
//jsx).
// Each screen A–D is shown in three columns: transparent (unchanged) · opaque (this request) · current (page hidden → boundary tile).
// Dashed amber outline = a rect the host can paint over the page (popup box, dropdown box). Drawn 2px outside the box for legibility.




const ConvertSurfacePopup = (p) => <Kit_ConvertSurfacePopup {...p} />;

const ZOOM = 0.56;
const COLS = [
  { mode: "transparent", label: "Transparent — unchanged" },
  { mode: "opaque", label: "Opaque — this request" },
  { mode: "hidden", label: "Current — page hidden" },
];

function MdPage() {
  const cell = { padding: "4px 8px", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-strong)", textAlign: "left" };
  return (
    <div style={{ position: "absolute", inset: 0, overflow: "hidden", padding: "20px 24px", display: "flex", flexDirection: "column", gap: 10,
      background: "var(--tasty-surface-markdown-focused-bg)", color: "var(--tasty-surface-markdown-focused-fg)", fontFamily: "var(--tasty-font-ui)" }}>
      <div style={{ fontSize: 14, fontWeight: 600 }}>Release notes — 0.14</div>
      <div style={{ fontSize: 13, lineHeight: 1.5, color: "var(--tasty-text-secondary)", textWrap: "pretty" }}>Surface conversion keeps the running process and its scrollback. Only the renderer changes, so a log can become a document and back without a restart.</div>
      <div style={{ fontSize: 13, fontWeight: 600 }}>Changes</div>
      <table style={{ borderCollapse: "collapse", fontSize: 12, color: "var(--tasty-text-secondary)" }}>
        <thead><tr><th style={{ ...cell, color: "var(--tasty-text-primary)" }}>Area</th><th style={{ ...cell, color: "var(--tasty-text-primary)" }}>Change</th></tr></thead>
        <tbody>
          {[["Explorer", "Remote write queue"], ["Markdown", "Address bar browse"], ["File picker", "Folder mode"], ["Handlers", "User overrides"]].map(([a, b]) => (
            <tr key={a}><td style={cell}>{a}</td><td style={cell}>{b}</td></tr>
          ))}
        </tbody>
      </table>
      <div style={{ fontSize: 13, lineHeight: 1.5, color: "var(--tasty-text-secondary)" }}>Every popup width is a token multiplied by ui_scale.</div>
    </div>
  );
}
// current fallback: page hidden, boundary tile in its place
function Tile() {
  return <div style={{ position: "absolute", inset: 0, background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", boxSizing: "border-box" }} />;
}
function Term() {
  return (
    <div style={{ position: "absolute", inset: 0, padding: "12px 14px", display: "flex", flexDirection: "column", gap: 4,
      background: "var(--tasty-surface-terminal-focused-bg)", color: "var(--tasty-surface-terminal-focused-fg)", fontFamily: "var(--tasty-font-mono)", fontSize: 12 }}>
      <span><span style={{ color: "var(--tasty-accent-success-raised)" }}>❯</span> cargo build --release</span>
      <span>   Compiling tasty v0.14.0</span>
      <span>    Finished release [optimized] in 41.2s</span>
      <span><span style={{ color: "var(--tasty-accent-success-raised)" }}>❯</span> </span>
    </div>
  );
}
const Scrim = ({ style }) => <div style={{ position: "absolute", inset: 0, background: "var(--tasty-scrim-bg)", ...style }} />;
const Host = ({ on, children, style }) => (
  <div style={{ display: "flex", outline: on ? "var(--tasty-border-width) dashed var(--tasty-accent-warning)" : "none", outlineOffset: 2, ...style }}>{children}</div>
);
function Frame({ w = 560, h = 340, children }) {
  return (
    <div style={{ position: "relative", width: w, height: h, overflow: "hidden", flex: "none", background: "var(--tasty-bg-app)",
      border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>{children}</div>
  );
}
const ground = (mode) => (mode === "hidden" ? <Tile /> : <MdPage />);

// A — convert_surface over a full-pane markdown page (surface-scoped scrim = the page itself)
function SceneA({ mode }) {
  const op = mode === "opaque";
  return (
    <Frame>
      <div style={{ height: 28, display: "flex", alignItems: "center", gap: 6, padding: "0 10px", background: "var(--tasty-bg-sidebar)",
        borderBottom: "var(--tasty-border-width) solid var(--tasty-border-frame)", fontSize: 12, color: "var(--tasty-text-secondary)" }}>
        <Icon name="markdown" size="var(--tasty-icon-size-sm)" /><span>release-notes.md</span>
      </div>
      <div style={{ position: "absolute", top: 29, left: 0, right: 0, bottom: 0 }}>
        {ground(mode)}
        {!op && <Scrim />}
        <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Host on={op}><ConvertSurfacePopup title="Surface Type" opaque={op} /></Host>
        </div>
      </div>
    </Frame>
  );
}
// B — command palette (window scrim) across a markdown page | terminal split
function SceneB({ mode }) {
  const op = mode === "opaque" || mode === "blotch";
  return (
    <Frame>
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: 279 }}>{ground(mode)}</div>
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 279, width: 1, background: "var(--tasty-border-frame)" }} />
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 280, right: 0 }}><Term /></div>
      {(mode === "transparent" || mode === "hidden") && <Scrim />}
      {mode === "blotch" && <Scrim style={{ left: 279 }} />}
      <div style={{ position: "absolute", top: 36, left: 0, right: 0, display: "flex", justifyContent: "center" }}>
        <Host on={op}><PaletteFrame opaque={op} /></Host>
      </div>
    </Frame>
  );
}
// C — Tools menu (scrim-less, shadow-popover) straddling sidebar | page
const TOOLS = ["Command palette…", "Listening ports…", "Remote connections…", "Presets", "Tutorial…", "Task DAGs", "Open File…"];
function SceneC({ mode }) {
  const op = mode === "opaque";
  return (
    <Frame>
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 150, right: 0 }}>{ground(mode)}</div>
      <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: 150, boxSizing: "border-box", background: "var(--tasty-bg-sidebar)",
        borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", display: "flex", flexDirection: "column", justifyContent: "flex-end", padding: 8 }}>
        <Button variant="ghost" size="sm" block ground="active" leadingIcon={<Icon name="tools" />} style={{ justifyContent: "flex-start", background: "var(--tasty-surface-active)" }}>Tools</Button>
      </div>
      <Host on={op} style={{ position: "absolute", left: 12, bottom: 52 }}>
        <div role="menu" aria-label="Tools" style={{ width: "max-content", minWidth: "var(--tasty-tools-menu-min-width)", maxWidth: "var(--tasty-tools-menu-max-width)", boxSizing: "border-box", whiteSpace: "nowrap",
          background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", padding: "var(--tasty-popup-content-margin)",
          borderRadius: op ? "var(--tasty-webview-opaque-radius)" : "var(--tasty-menu-radius)", boxShadow: op ? "var(--tasty-webview-opaque-shadow)" : "var(--tasty-shadow-popover)" }}>
          {TOOLS.map((l) => <MenuItem key={l} label={l} />)}
          <MenuItem separator />
          <MenuItem label="Git" />
        </div>
      </Host>
    </Frame>
  );
}
// D — file picker over a page, path-bar crumb dropdown hanging below the picker box
function SceneD({ mode }) {
  const op = mode === "opaque";
  const PX = 60, PY = 24, PATH_BOTTOM = 77;
  return (
    <Frame h={360}>
      {ground(mode)}
      {!op && <Scrim />}
      <Host on={op} style={{ position: "absolute", left: PX, top: PY }}>
        <FilePickerFrame w={440} h={188} deep opaque={op} />
      </Host>
      <Host on={op} style={{ position: "absolute", left: PX + 44, top: PY + PATH_BOTTOM + 4 }}>
        <FpCrumbMenu items={["Users", "maya", "tasty", "config", "keybindings", "exports"]} opaque={op} style={{ position: "static", marginTop: 0 }} />
      </Host>
    </Frame>
  );
}

function Grid({ Scene, h = 340, extra }) {
  const cols = extra ? [...COLS, extra] : COLS;
  const row = (theme) => (
    <div data-theme={theme === "Latte" ? "latte" : undefined} style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
      {cols.map((c) => (
        <div key={c.mode} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
          <span style={{ fontSize: "var(--tasty-font-size-caption)", color: c.mode === "opaque" ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{theme} · {c.label}</span>
          <div style={{ zoom: ZOOM }}><Scene mode={c.mode} /></div>
        </div>
      ))}
    </div>
  );
  return (
    <Stage variant="solo" eager style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-md)", alignItems: "flex-start" }}>
      {row("Mocha")}{row("Latte")}
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
        <span style={{ width: 16, height: 10, outline: "var(--tasty-border-width) dashed var(--tasty-accent-warning)" }} />
        host-drawable rect over the page (popup box · dropdown box) — every pixel inside is opaque, nothing outside it can change
      </span>
    </Stage>
  );
}

const OPAQUE_TOKENS = [
  { tok: "--tasty-webview-opaque-radius", use: "→ radius-none → radius-0 (0)" },
  { tok: "--tasty-webview-opaque-shadow", use: "→ shadow-none (none)" },
  { tok: "--tasty-border-frame", use: "shell edge kept (convert, dialogs)", color: "var(--tasty-border-frame)" },
  { tok: "--tasty-menu-border", use: "menu / dropdown edge kept", color: "var(--tasty-menu-border)" },
  { tok: "--tasty-scrim-bg", use: "not drawn while a page is visible", color: "var(--tasty-scrim-bg)" },
];

function GalleryWebviewOpaque() {
  return (
    <>
      <Spec id="contract-popup-over-a-page-in-opaque-mode" title="Contract — popup over a page in opaque mode"
        when={<>A markdown or html page is a native WebView. Where the host cannot composite over it with transparency (e.g. Linux X11 with no compositing manager), the host paints only inside the <b>popup box rect</b> and each <b>dropdown box rect</b>; those pixels are fully opaque and the page outside them cannot change. Transparent mode is unchanged. The mode is decided by the environment; there is no user setting and no on-screen indicator. Toasts and passive popups keep the current rule (overlapped page hidden).</>}>
        <Meta
          specs={[
            ["1 · corner", "square — box reads webview-opaque-radius (0); nothing to fill outside an arc"],
            ["2 · shadow", "none — webview-opaque-shadow; the existing 1px border is the only silhouette, no extra edge"],
            ["3 · scrim", "not drawn at all while any opaque-mode page is visible in the scrim's scope; no blotch"],
            ["  scope", "same rule for surface scope (the page itself) and window scope"],
            ["4 · straddle", "box rect intersects a page anywhere → the WHOLE box goes square + no shadow, never per edge"],
            ["5 · dropdown", "same rule; a dropdown also follows its parent shell (shell opaque → dropdown opaque)"],
            ["6 · tokens", "webview-opaque-radius → radius-none → radius-0 · webview-opaque-shadow → shadow-none"],
            ["unchanged", "shell structure · padding · rows · hover/active · focus look · border tokens"],
          ]}
          tokens={OPAQUE_TOKENS} />
        <Note><b>Input routing (unchanged contract, annotated here):</b> while a target popup is visible, a click on the page body is that popup's <b>outside click</b> and never reaches the page — popups that close on outside click close (convert_surface; command palette, which is <code>sticky_focus=true</code>, closes too), others stay open and only lose focus (notification panel, search bar). Keys and IME go to the focused popup, never to the page. Closing the popup leaves the page exactly as before it opened.</Note>
        <Note><b>Why no scrim:</b> the scrim cannot darken the page, so drawing it only on terminal / tab bar / sidebar makes the page the brightest thing on screen while a modal is up (measured). Dropping it keeps the screen uniform; the modal is still marked by its frame border, its focus, and outside-click routing. When no opaque page is on screen (page in a background tab, or transparent mode), the scrim draws as today.</Note>
      </Spec>

      <Spec id="a-convert-surface-over-a-full-pane-markdown-page" title="A — convert_surface over a full-pane markdown page"
        when={<>Surface-scoped scrim, and the surface is the page. Transparent: scrim over the page, rounded shell with <span className="tok">--tasty-shadow-modal</span>. Opaque: no scrim, square shell, no shadow, <span className="tok">--tasty-border-frame</span> edge kept. Current: page hidden behind the boundary tile.</>}>
        <Grid Scene={SceneA} />
        <Note>The 240 × ui_scale width is unchanged; longer de / ja / ko titles take the same band and tooltip rule as today, the edge treatment does not depend on width.</Note>
      </Spec>

      <Spec id="b-command-palette-across-markdown-page-terminal" title="B — command palette across markdown page | terminal"
        when={<>Window scrim. Opaque mode drops the scrim on <b>both</b> panes, because the page pane cannot be dimmed. The fourth column is the rejected option: scrim kept outside the page.</>}>
        <Grid Scene={SceneB} extra={{ mode: "blotch", label: "Rejected — scrim outside page only" }} />
        <Dont><b>Don't</b> keep the scrim on the non-page areas only: the page stays at full brightness while the terminal, tab bar and sidebar go dark (the blotch seen in measurement).</Dont>
      </Spec>

      <Spec id="c-tools-menu-straddling-sidebar-page" title="C — Tools menu straddling sidebar | page"
        when={<>Scrim-less, <span className="tok">--tasty-shadow-popover</span> in transparent mode. Part of the menu sits over the sidebar, part over the page. Because the box rect intersects the page, the whole menu is square with no shadow, including the part over the sidebar where a shadow could be drawn.</>}>
        <Grid Scene={SceneC} />
        <Do><b>Do</b> decide the edge once per box from its rect. A half-rounded, half-shadowed menu reads as a rendering fault.</Do>
      </Spec>

      <Spec id="d-file-picker-with-the-path-bar-dropdown-hanging-over-the-page" title="D — file picker with the path-bar dropdown hanging over the page"
        when={<>Two host rects: the picker box and the crumb dropdown box, which extends past the picker's bottom edge onto the page. Both are square with no shadow; the dropdown keeps <span className="tok">--tasty-menu-border</span>. Select / SearchableSelect menus opened from a popup over a page follow the same rule.</>}>
        <Grid Scene={SceneD} />
        <Note>A dropdown opened from an opaque-mode shell is opaque-mode even if its own rect ends over a terminal, so the parent and child never disagree.</Note>
      </Spec>
    </>
  );
}

export { GalleryWebviewOpaque };
