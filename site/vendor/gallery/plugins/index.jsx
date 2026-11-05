import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Icon as PIcon, IconButton, Button, Input, AutoComplete, MenuItem, Switch, Kbd, Spinner, Checkbox, Select, Tag } from "../../components/index.js";
import { HtmlAddressSurfaceG } from "../../ui_kits/terminal/overlays/html_address_bar.jsx";
import { HtmlAddressContrastSpec as PluginsHtmlContrast_HtmlAddressContrastSpec } from "./html-address-ink.jsx";
import { MdAddressBar as B, MdAddressSpecs as PluginsMdAddress_MdAddressSpecs } from "./markdown-address.jsx";
import { Section, Spec, Stage, Meta, Note, Dont, GIcon } from "../parts/specimen.jsx";
import { ic, PathField, SegToggle, SideHead, TreeNode, FavRow, DetailHeader, DetailRow, GridCell, ExpState, CtxMenu } from "../parts/explorer-kit.jsx";

const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty Gallery — Plugin surfaces. Surface kinds contributed by bundled plugins
// that fill a work-area tile: the Explorer file manager and the content viewers
// (Markdown / HTML / Image …). Kept on their OWN page because plugin surfaces will
// grow without bound — they must not crowd the general structural Layouts.




const NAV = [
  { id: "explorer", label: "Explorer (file manager)" },
  { id: "markdown", label: "Markdown viewer" },
  { id: "html", label: "HTML viewer" },
  { id: "image", label: "Image viewer" },
];

// Recent-path candidate lists for the shared PathField's AutoComplete dropdown.
const EXP_RECENT = ["~/Downloads", "~/work/tasty", "~/work/tasty-ui/src", "~/work/tasty/crates/tasty-ui-widgets", "~/.config/tasty"];
const MD_RECENT = ["~/work/tasty/README.md", "~/work/tasty/docs/architecture.md", "~/work/tasty/docs/design/systems/theme.md", "~/work/tasty/CHANGELOG.md"];

// ════════════════════════════════════════════════════════════
//  EXPLORER (file-manager surface)
// ════════════════════════════════════════════════════════════
function ExpTab({ label, active, closable = true }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, height: "100%", padding: "0 8px 0 10px",
      maxWidth: 140, fontSize: 12, cursor: "default",
      color: active ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
      background: active ? "var(--tasty-bg-panel)" : "transparent",
      borderRight: "1px solid var(--tasty-separator)",
      boxShadow: active ? "inset 0 2px 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{ic.folder}</span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      {closable && <span style={{ display: "inline-flex", flex: "none" }}>{ic.x}</span>}
    </div>
  );
}

function ExpInternalTabs() {
  return (
    <div style={{ display: "flex", alignItems: "stretch", height: 28, flex: "none",
      background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
      <ExpTab label="Downloads" active />
      <ExpTab label="src" />
      <div style={{ display: "flex", alignItems: "center", padding: "0 4px" }}><IconButton size="sm" aria-label="New tab">{ic.plus}</IconButton></div>
    </div>
  );
}

function ExpToolbar() {
  const navBtn = (label, glyph, disabled) => (
    <span style={{ display: "inline-flex" }}>
      <IconButton size="sm" aria-label={label} disabled={disabled}>{glyph}</IconButton>
    </span>
  );
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, flex: "none", height: 44, padding: "0 8px",
      background: "var(--tasty-bg-panel)", borderBottom: "1px solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 1 }}>
        {navBtn("Back", ic.back)}
        {navBtn("Forward", ic.fwd, true)}
        {navBtn("Up", ic.up)}
        {navBtn("Refresh", ic.refresh)}
      </div>
      <PathField icon={ic.folderOpen} path="~/Downloads" />
      <SegToggle />
    </div>
  );
}

// Favorites empty state — the section CAPTION stays even at 0 favorites, so the
// feature is always discoverable; a muted line + faint hint fill the slot. Tone
// matches the content-area "This folder is empty" state (text-muted). i18n key:
// explorer.sidebar.favorites_empty ("아직 즐겨찾기가 없습니다").
function FavoritesEmpty() {
  return (
    <div style={{ padding: "2px 10px 10px", display: "flex", flexDirection: "column", gap: 3 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--tasty-text-muted)" }}>
        <span style={{ display: "inline-flex", flex: "none" }}>{ic.star}</span>
        <span style={{ fontSize: 12 }}>No favorites yet</span>
      </div>
      <div style={{ fontSize: 11, lineHeight: 1.45, color: "var(--tasty-text-placeholder)" }}>
        Right-click a folder → <span style={{ color: "var(--tasty-text-muted)" }}>Add to favorites</span>.
      </div>
    </div>
  );
}

// ExpSidebar — 2-REGION PINNED layout (design SoT for explorer.rs sidebar()).
// NOT one scroll column: "Files" owns the top region with its OWN scroll, and
// "Favorites" is pinned to the bottom at a computed fixed height, so a long tree
// can never push the favorites list below the fold. The boundary between them is
// a fixed 1px --tasty-explorer-split-border at a screen coordinate — it does not
// travel with content (a short tree leaves empty bg above the line, it does not
// pull the line up). Both regions scroll independently.
//   pin height: --tasty-explorer-favorites-pin-height (240) when the sidebar BODY
//   height >= --tasty-explorer-favorites-pin-threshold (600); below that,
//   --tasty-explorer-favorites-pin-ratio (0.4) x body height, snapped to the 4px
//   grid and floored at --tasty-explorer-favorites-pin-min-height (120).
const FAV_PIN = { base: 240, threshold: 600, ratio: 0.4, min: 120 };
function favPinHeight(bodyH) {
  if (!bodyH || bodyH >= FAV_PIN.threshold) return FAV_PIN.base;
  return Math.max(FAV_PIN.min, Math.round((bodyH * FAV_PIN.ratio) / 4) * 4);
}

const TREE_SHORT = [
  { label: "Home", depth: 0, open: true },
  { label: "Downloads", depth: 1, open: true, active: true },
  { label: "mockup-exports", depth: 2, leaf: true },
  { label: "Documents", depth: 1 },
  { label: "Projects", depth: 1 },
];
const TREE_LONG = [
  { label: "Home", depth: 0, open: true },
  { label: "Downloads", depth: 1, open: true, active: true },
  { label: "mockup-exports", depth: 2, leaf: true },
  { label: "invoices", depth: 2, leaf: true },
  { label: "Documents", depth: 1, open: true },
  { label: "contracts", depth: 2, leaf: true },
  { label: "notes", depth: 2, leaf: true },
  { label: "Projects", depth: 1, open: true },
  { label: "tasty", depth: 2, open: true },
  { label: "crates", depth: 3, leaf: true },
  { label: "docs", depth: 3, leaf: true },
  { label: "src", depth: 3, leaf: true },
  { label: "target", depth: 3, leaf: true },
  { label: "design-system", depth: 2, leaf: true },
  { label: "playground", depth: 2, leaf: true },
  { label: "Pictures", depth: 1, open: true },
  { label: "screenshots", depth: 2, leaf: true },
  { label: "wallpapers", depth: 2, leaf: true },
  { label: "Music", depth: 1 },
  { label: "Videos", depth: 1 },
  { label: ".config", depth: 1 },
  { label: ".cache", depth: 1 },
];

const FAVS_DEFAULT = [["tasty", true], ["Documents", false], ["screenshots", false]];
const FAVS_MANY = [["tasty", true], ["crates", false], ["design-system", false], ["Documents", false],
  ["mockup-exports", false], ["screenshots", false], ["wallpapers", false], ["invoices", false],
  [".config", false], ["playground", false]];

// 2026-10-07 — short cell: below --tasty-explorer-favorites-hide-below (240 = fav floor 120 + Files 120) the
// Favorites region is dropped and Files takes the whole body. The explorer cell itself stops at
// --tasty-explorer-min-height (180 since 2026-10-08), so Detail always shows its header + 2 rows.
const FAV_HIDE_BELOW = 240;
function ExpSidebar({ favorites = FAVS_DEFAULT, tree = "short", height }) {
  const rows = tree === "long" ? TREE_LONG : TREE_SHORT;
  const pin = favPinHeight(height);
  const showFav = !height || height >= FAV_HIDE_BELOW;
  return (
    <div style={{ width: "var(--tasty-explorer-sidebar-width)", height: height || "100%", flex: "none", display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      {/* region A — Files: fills the remainder, scrolls on its own */}
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <SideHead>Files</SideHead>
        <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "0 6px 6px", display: "flex", flexDirection: "column", gap: 1 }}>
          {rows.map((r, i) => <TreeNode key={i} {...r} />)}
        </div>
      </div>
      {/* region B — Favorites: pinned to the bottom, fixed height, own scroll */}
      {showFav && <div style={{ height: pin, flex: "none", display: "flex", flexDirection: "column", overflow: "hidden",
        borderTop: "var(--tasty-border-width) solid var(--tasty-explorer-split-border)" }}>
        <SideHead>Favorites</SideHead>
        {favorites.length === 0 ? (
          <FavoritesEmpty />
        ) : (
          <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "0 6px 8px", display: "flex", flexDirection: "column", gap: 1 }}>
            {favorites.map(([n, a]) => <FavRow key={n} name={n} active={a} />)}
          </div>
        )}
      </div>}
    </div>
  );
}

function ExpDetail() {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)" }}>
      <DetailHeader />
      <div style={{ flex: 1, overflow: "hidden" }}>
        <DetailRow glyph={ic.folder} name="mockup-exports" size="—" date="2026-06-20 14:30" type="Folder" />
        <DetailRow glyph={ic.file} name="report.pdf" size="2.4 MB" date="2026-06-24 09:12" type="PDF" />
        <DetailRow glyph={ic.image} name="diagram.png" size="488 KB" date="2026-06-26 18:05" type="PNG" state="selected" glyphColor="var(--tasty-accent-info-raised)" />
        <DetailRow glyph={ic.file} name="notes.md" size="12 KB" date="2026-06-27 11:40" type="Markdown" state="hover" />
        <DetailRow glyph={ic.file} name="archive.zip" size="64 MB" date="2026-06-18 22:01" type="Archive" state="cut" />
      </div>
    </div>
  );
}

function ExplorerFrame() {
  return (
    <div style={{ width: "100%", maxWidth: 760, display: "flex", flexDirection: "column", height: 420,
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <ExpInternalTabs />
      <ExpToolbar />
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <ExpSidebar height={348} />
        <ExpDetail />
      </div>
    </div>
  );
}

function ExpGridMini() {
  return (
    <div style={{ flex: 1, background: "var(--tasty-bg-panel)", padding: 8, display: "flex", flexWrap: "wrap", gap: 4, alignContent: "flex-start" }}>
      <GridCell glyph={ic.folder} name="mockup-exports" />
      <GridCell glyph={ic.file} name="rust-toolchain.toml" />
      <GridCell glyph={ic.image} name="diagram.png" state="selected" glyphColor="var(--tasty-accent-info-raised)" />
      <GridCell glyph={ic.file} name="notes.md" state="hover" />
      <GridCell glyph={ic.file} name="THIRD_PARTY_LICENSES.md" />
      <GridCell glyph={ic.folder} name="node_modules" />
    </div>
  );
}

function ExpListMini() {
  const rows = [[ic.folder, "mockup-exports", null], [ic.file, "report.pdf", null], [ic.image, "diagram.png", "selected"], [ic.file, "notes.md", "hover"], [ic.file, "build.sh", null], [ic.file, "archive.zip", "cut"]];
  return (
    <div style={{ flex: 1, background: "var(--tasty-bg-panel)", padding: "6px 4px" }}>
      {rows.map(([g, n, st], i) => (
        <div key={i} style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-tree-row-height)", padding: "0 8px", borderRadius: "var(--tasty-radius-sm)",
          fontSize: 13, fontStyle: st === "cut" ? "var(--tasty-cut-pending-font-style)" : undefined,
          color: st === "selected" ? "var(--tasty-text-on-active)" : st === "cut" ? "var(--tasty-cut-pending-fg)" : "var(--tasty-text-secondary)",
          background: st === "selected" ? "var(--tasty-surface-active)" : st === "hover" ? "var(--tasty-overlay-hover)" : "transparent" }}>
          <span style={{ display: "inline-flex", flex: "none", color: st === "selected" ? "var(--tasty-text-on-active)" : g === ic.image ? "var(--tasty-accent-info)" : "var(--tasty-text-muted)" }}>{g}</span>{n}
        </div>
      ))}
    </div>
  );
}

function ViewModeColumn({ title, sub, children }) {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
      <div style={{ marginBottom: 6 }}>
        <div style={{ fontSize: 12, fontWeight: 600, color: "var(--tasty-text-secondary)" }}>{title}</div>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{sub}</div>
      </div>
      <div style={{ height: 200, display: "flex", flexDirection: "column", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>{children}</div>
    </div>
  );
}

// ════════════════════════════════════════════════════════════
//  CONTENT VIEWERS (markdown / html / image)
// ════════════════════════════════════════════════════════════
// 2026-10-07 batch 4 — matches render.rs heading_sizes_px(): 5-step interpolation prose-h1 (20) → body (13),
// i.e. 20 · 18.6 · 17.2 · 15.8 · 14.4 · 13, every level text-primary · 600.
const mdHSize = (n) => `calc(var(--tasty-font-size-prose-h1) - (var(--tasty-font-size-prose-h1) - var(--tasty-font-size-body)) * ${(n - 1) / 5})`;
const MD_H = {
  1: { fontSize: mdHSize(1), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "0 0 10px", lineHeight: 1.3 },
  2: { fontSize: mdHSize(2), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "20px 0 8px", lineHeight: 1.3 },
  3: { fontSize: mdHSize(3), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "16px 0 6px" },
  4: { fontSize: mdHSize(4), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "14px 0 4px" },
  5: { fontSize: mdHSize(5), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "12px 0 4px" },
  6: { fontSize: mdHSize(6), fontWeight: 600, color: "var(--tasty-text-primary)", margin: "12px 0 4px" },
};
function MdH({ level, children }) { return <div style={MD_H[level]}>{children}</div>; }
const mdBody = { fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)", lineHeight: 1.6, margin: "0 0 10px" };
function InlineCode({ children }) {
  return <code style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, background: "var(--tasty-md-code-bg)", color: "var(--tasty-text-primary)", padding: "1px 5px", borderRadius: "var(--tasty-radius-sm)" }}>{children}</code>;
}
function MdLink({ children }) { return <span style={{ color: "var(--tasty-accent-primary-raised)", textDecoration: "underline", textUnderlineOffset: 2, cursor: "pointer" }}>{children}</span>; }

// Markdown inline GFM table — grid + zebra, read-only (no hover/selection/sort;
// SEPARATE from the shared Table widget). Every color/size reads a --tasty-md-table-*
// token so gallery + real render.rs table() transcribe 1:1. Grid line (surface1) is
// the lightest element → always visible; header (surface0) is the distinct band;
// zebra (mantle) is one subtle step below the base row (base).
const MDT_B = "var(--tasty-border-width) solid var(--tasty-md-table-border)";
function MdTable({ cols, rows }) {
  const pad = "var(--tasty-md-table-cell-padding-y) var(--tasty-md-table-cell-padding-x)";
  return (
    <div style={{ border: MDT_B, borderRadius: "var(--tasty-radius)", overflow: "hidden", marginBottom: 12 }}>
      <table style={{ borderCollapse: "collapse", width: "100%", background: "var(--tasty-md-table-row-bg)",
        fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-md-table-cell-fg)", lineHeight: 1.6 }}>
        <thead>
          <tr>
            {cols.map((c, j) => (
              <th key={j} style={{ padding: pad, textAlign: c.num ? "right" : "left", verticalAlign: "top",
                width: c.w || "auto", background: "var(--tasty-md-table-header-bg)", color: "var(--tasty-md-table-header-fg)",
                fontWeight: "var(--tasty-font-weight-normal)", fontFamily: c.num ? "var(--tasty-font-mono)" : "inherit",
                borderRight: j < cols.length - 1 ? MDT_B : "none", borderBottom: MDT_B }}>{c.h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i}>
              {r.map((cell, j) => (
                <td key={j} style={{ padding: pad, textAlign: cols[j].num ? "right" : "left", verticalAlign: "top",
                  fontFamily: cols[j].num ? "var(--tasty-font-mono)" : "inherit",
                  background: i % 2 === 1 ? "var(--tasty-md-table-row-bg-zebra)" : "transparent",
                  borderRight: j < r.length - 1 ? MDT_B : "none",
                  borderBottom: i < rows.length - 1 ? MDT_B : "none" }}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TypeScaleSheet() {
  const rows = [["h1", 1], ["h2", 2], ["h3", 3], ["h4", 4], ["h5", 5], ["h6", 6]];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, width: 360 }}>
      {rows.map(([tag, lvl]) => (
        <div key={tag} style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
          <span style={{ width: 24, fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{tag}</span>
          <div style={{ ...MD_H[lvl], margin: 0, flex: 1 }}>The quick brown fox</div>
        </div>
      ))}
      <div style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
        <span style={{ width: 24, fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>p</span>
        <div style={{ ...mdBody, margin: 0, flex: 1 }}>Body — 13px, line-height 1.6, secondary.</div>
      </div>
      <div style={{ display: "flex", alignItems: "baseline", gap: 12 }}>
        <span style={{ width: 24, fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>small</span>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", flex: 1 }}>Caption — body × 0.85, muted.</div>
      </div>
    </div>
  );
}

// Markdown surface chrome — browser-style address bar (path display/edit) + Go
// button, above the body. `editing` = clicked-into edit mode (focus ring + caret).
// 2026-10-21 — delegates to gallery/plugins-md-address.jsx (folder button + editable mirror bar).
function MdAddressBar({ editing = false, path = "~/work/tasty/docs/architecture.md", ...rest }) {

  return <B state={editing ? "editing" : "idle"} path={path} {...rest} />;
}

// Large-file (>1MB) confirm — surface-scoped popup, same 360px shell as markdown_open.
function MdLargeFilePopup() {
  return (
    <div style={{ width: 360, maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
      <div style={{ padding: "14px 14px 0" }}>
        <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 3 }}>Open large file?</div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 10, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>~/work/tasty/docs/CHANGELOG-full.md</div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Tag variant="warning">3.2 MB</Tag>
          <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>Over 1 MB — rendering may be slow.</span>
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: 14 }}>
        <Button variant="ghost">Cancel</Button>
        <Button variant="primary">Open</Button>
      </div>
    </div>
  );
}

function MarkdownDoc({ editing = false }) {
  return (
    <div style={{ width: "100%", maxWidth: 640, height: 460, display: "flex", flexDirection: "column", background: "var(--tasty-md-doc-bg)",
      border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <MdAddressBar editing={editing} />
      <div style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "20px 24px" }}>
      <MdH level={1}>Markdown surface</MdH>
      <div style={mdBody}>A read-only viewer that reloads on file change. Text is selectable; links show a URL tooltip on hover. Body uses <InlineCode>--tasty-font-size-body</InlineCode> at the 14px cap, line-height <b style={{ color: "var(--tasty-text-secondary)" }}>1.6</b>.</div>
      <MdH level={2}>Headings &amp; emphasis</MdH>
      <div style={mdBody}>Inline runs: <b style={{ color: "var(--tasty-text-secondary)" }}>bold</b>, <i>italic</i>, <b><i>both</i></b>, <span style={{ textDecoration: "line-through", color: "var(--tasty-text-muted)" }}>strikethrough</span>, a <MdLink>link</MdLink>, and <InlineCode>inline code</InlineCode>.</div>
      <MdH level={3}>Lists</MdH>
      <div style={{ ...mdBody, margin: "0 0 6px" }}>
        <div style={{ display: "flex", gap: 8 }}><span style={{ color: "var(--tasty-text-muted)" }}>•</span><span>Bullet item with wrapped text running onto a second line for rhythm.</span></div>
        <div style={{ display: "flex", gap: 8, paddingLeft: 16 }}><span style={{ color: "var(--tasty-text-muted)" }}>◦</span><span>Nested bullet</span></div>
        <div style={{ display: "flex", gap: 8 }}><span style={{ color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)", fontSize: 12 }}>1.</span><span>Ordered item</span></div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}><Checkbox checked readOnly /><span>Task done</span></div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}><Checkbox readOnly /><span>Task to do</span></div>
      </div>
      <MdH level={3}>Code block</MdH>
      <pre style={{ margin: "0 0 12px", background: "var(--tasty-md-code-bg)", border: "1px solid var(--tasty-md-code-border)", borderRadius: "var(--tasty-radius)", padding: 12, overflow: "auto",
        fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-secondary)", lineHeight: 1.5 }}>
{`fn main() {
    println!("hi from tasty");
}`}
      </pre>
      <MdH level={3}>Table</MdH>
      {/* Markdown inline table — grid + zebra (read-only: no hover/selection/sort).
          Opaque base fill + border-strong grid + surface0 header + mantle zebra so it
          reads on both the focused surface (~#000) and this bg-panel card. Value ladder
          dark→light: mantle(zebra) < base(row) < surface0(header) < surface1(grid). */}
      <MdTable
        cols={[{ h: "Resource" }, { h: "Kind" }, { h: "Count", num: true, w: "80px" }]}
        rows={[["surface", "viewer", "12"], ["popup", "overlay", "8"], ["banner", "notice", "3"]]}
      />
      <MdH level={3}>Blockquote</MdH>
      <div style={{ borderLeft: "var(--tasty-md-quote-bar-width) solid var(--tasty-md-quote-bar)", paddingLeft: 12, margin: "0 0 12px", color: "var(--tasty-md-quote-fg)", fontSize: 13, lineHeight: 1.6 }}>
        Quoted text reads one tone down (muted) with a left bar.
        <div style={{ borderLeft: "var(--tasty-md-quote-bar-width) solid var(--tasty-md-quote-bar)", paddingLeft: 12, marginTop: 6 }}>Nested quote, one level deeper.</div>
      </div>
      <MdH level={3}>Callouts</MdH>
      {/* 2026-10-06: GitHub alerts / Obsidian callouts — both forms (blockquote and collapsible details) read md-quote-bar-width; colour per alert type */}
      {/* 2026-10-07: drawn as the renderer does — type colour at tint-fill-alpha, 16px type icon before the label, radius; note = accent-primary */}
      {[["Note", "var(--tasty-accent-primary)", "alertCircle", "Non-collapsible: > [!note]", false], ["Warning", "var(--tasty-accent-warning)", "alertTriangle", "Collapsible: > [!warning]- (closed until clicked)", true]].map(([t, c, glyph, body, fold]) => (
        <div key={t} style={{ borderLeft: "var(--tasty-md-quote-bar-width) solid " + c, borderRadius: "var(--tasty-radius)", background: "color-mix(in srgb, " + c + " calc(var(--tasty-tint-fill-alpha) * 100%), transparent)", padding: "var(--tasty-space-sm) var(--tasty-space-md)", margin: "0 0 12px", fontSize: 13, lineHeight: 1.6, color: "var(--tasty-md-quote-fg)" }}>
          <div style={{ color: c.replace(")", "-raised)"), fontWeight: 600, display: "flex", alignItems: "center", gap: "var(--tasty-md-callout-icon-gap)" }}>{fold ? <PIcon name="chevronRight" size="var(--tasty-md-callout-marker-size)" /> : null}<PIcon name={glyph} size="var(--tasty-icon-size-md)" />{t}</div>
          {!fold && <div style={{ color: "var(--tasty-text-primary)" }}>{body}</div>}
          {fold && <div style={{ fontSize: 11, color: "var(--tasty-text-primary)" }}>{body}</div>}
        </div>
      ))}
      <div style={{ height: 1, background: "var(--tasty-md-rule)", margin: "16px 0" }} />
      <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Horizontal rule above · trailing space below.</div>
      </div>
    </div>
  );
}

// ── Markdown content colour (WebView path, 2026-10-07 batch 4): render.rs theme_css()
// writes Tasty tokens into --md-* CSS variables; hljs_css() maps highlight.js classes
// to palette roles. Rendered as text+swatch so the mapping is the source of truth.
function Sw({ c }) {
  return <span style={{ display: "inline-block", width: 12, height: 12, flex: "none", borderRadius: 2, border: "1px solid var(--tasty-separator)", background: c, verticalAlign: "middle" }} />;
}
const MD_VAR_MAP = [
  ["--md-fg", "body text", "--tasty-text-secondary"],
  ["--md-strong", "headings · strong · table header", "--tasty-text-primary"],
  ["--md-link", "links", "--tasty-accent-primary"],
  ["--md-code-bg", "inline code · code-block fill", "--tasty-surface-raised"],
  ["--md-code-border", "code-block border", "--tasty-separator"],
  ["--md-quote-bar", "blockquote bar", "--tasty-border-strong"],
  ["--md-rule", "horizontal rule", "--tasty-separator"],
];
const HLJS_MAP = [
  ["hljs-keyword", "--tasty-color-mauve"],
  ["hljs-string", "--tasty-color-green"],
  ["hljs-title · function", "--tasty-color-blue"],
  ["hljs-number", "--tasty-color-peach"],
  ["hljs-type", "--tasty-color-yellow"],
  ["hljs-comment (italic)", "--tasty-text-muted"],
  ["hljs-tag", "--tasty-color-teal"],
  ["hljs-variable", "--tasty-color-lavender"],
  ["hljs-built_in", "--tasty-color-red"],
];
function MdInjectionMap() {
  const cell = { padding: "6px 10px", fontSize: 12, color: "var(--tasty-text-secondary)", borderBottom: "1px solid var(--tasty-separator)", textAlign: "left", verticalAlign: "middle" };
  const head = { ...cell, fontSize: 11, color: "var(--tasty-text-muted)", fontWeight: "var(--tasty-font-weight-normal)", textTransform: "uppercase", letterSpacing: ".05em" };
  const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: 11 };
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 24, alignItems: "flex-start" }}>
      <div style={{ flex: "1 1 380px", minWidth: 320 }}>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 6, textTransform: "uppercase", letterSpacing: ".05em" }}>theme_css() --md-* → Tasty token</div>
        <div style={{ border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", background: "var(--tasty-bg-panel)" }}>
          <table style={{ borderCollapse: "collapse", width: "100%" }}>
            <thead><tr><th style={head}>CSS variable</th><th style={head}>draws</th><th style={head}>token</th></tr></thead>
            <tbody>
              {MD_VAR_MAP.map(([f, d, t], i) => (
                <tr key={i}>
                  <td style={{ ...cell, ...mono, color: "var(--tasty-accent-agent-raised)" }}>{f}</td>
                  <td style={cell}>{d}</td>
                  <td style={{ ...cell, borderBottom: i === MD_VAR_MAP.length - 1 ? "none" : cell.borderBottom }}>
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}><Sw c={`var(${t})`} /><span style={mono}>{t}</span></span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div style={{ flex: "1 1 240px", minWidth: 220 }}>
        <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 6, textTransform: "uppercase", letterSpacing: ".05em" }}>hljs_css() highlight role → palette</div>
        <div style={{ border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", background: "var(--tasty-bg-panel)" }}>
          <table style={{ borderCollapse: "collapse", width: "100%" }}>
            <tbody>
              {HLJS_MAP.map(([role, t], i) => (
                <tr key={i}>
                  <td style={{ ...cell, fontFamily: "var(--tasty-font-mono)", fontSize: 11, borderBottom: i === HLJS_MAP.length - 1 ? "none" : cell.borderBottom }}>{role}</td>
                  <td style={{ ...cell, textAlign: "right", borderBottom: i === HLJS_MAP.length - 1 ? "none" : cell.borderBottom }}>
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 6, justifyContent: "flex-end" }}><span style={{ ...mono, color: "var(--tasty-text-muted)" }}>{t.replace("--tasty-", "")}</span><Sw c={`var(${t})`} /></span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function HtmlTile({ state }) {
  const center = (glyph, glyphColor, title, titleColor, sub) => (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8, textAlign: "center", padding: 16 }}>
      <span style={{ display: "inline-flex", color: glyphColor, transform: "scale(1.5)" }}>{glyph}</span>
      <div style={{ fontSize: 13, color: titleColor }}>{title}</div>
      {sub && <div style={{ fontSize: 11, fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-muted)", maxWidth: 220, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sub}</div>}
    </div>
  );
  const body = {
    boundary: <div style={{ flex: 1 }} />, /* 2026-10-06: plain tile while an egui overlay hides the loaded WebView — no glyph, no text */
    placeholder: center(ic.globe, "var(--tasty-text-disabled)", "No page loaded", "var(--tasty-text-muted)"),
    loading: center(<Spinner />, "var(--tasty-spinner-indicator)", "Loading…", "var(--tasty-text-muted)"),
    error: center(ic.alert, "var(--tasty-accent-danger-raised)", "Failed to load", "var(--tasty-accent-danger-raised)", "https://offline.example"),
  }[state];
  return (
    <div style={{ width: 220, height: 150, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>{body}</div>
  );
}

function HtmlSettings() {
  const Row = ({ label, children }) => (
    <div style={{ display: "flex", alignItems: "center", gap: 12, minHeight: 28 }}>
      <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-text-secondary)" }}>{label}</span>{children}
    </div>
  );
  return (
    <div style={{ width: 320, display: "flex", flexDirection: "column", gap: 8, padding: 14,
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>HTML viewer</div>
      <Row label="Default zoom"><div style={{ display: "flex", alignItems: "center", gap: 4 }}><Input style={{ width: 72 }} defaultValue="100" /><span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>%</span></div></Row>
      <Row label="Color scheme"><Select options={["follow", "light", "dark"]} style={{ width: 160 }} /></Row>
      <Row label="Allow remote content"><Switch /></Row>
      <Row label="Sandbox scripts"><Switch defaultChecked /></Row>
      <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", lineHeight: 1.5 }}>Page pixels are drawn by the OS WebView — these settings configure it; tasty doesn't theme the content.</div>
    </div>
  );
}

function ImgBtn({ glyph, label, disabled }) {
  return (
    <span style={{ display: "inline-flex" }}>
      <IconButton size="sm" aria-label={label} disabled={disabled}>{glyph}</IconButton>
    </span>
  );
}
function ZoomGroup({ pct = "100%" }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2, flex: "none" }}>
      <Button variant="secondary" size="sm">Fit</Button>
      <ImgBtn glyph={ic.plus} label="Zoom in" />
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-image-zoom-font-size)", color: "var(--tasty-text-muted)", minWidth: "var(--tasty-image-zoom-min-width)", textAlign: "center" }}>{pct}</span>
      <IconButton size="sm" aria-label="Zoom out"><GIcon d={<path d="M5 12h14" />} /></IconButton>
    </div>
  );
}
function ImgCanvas({ children, panned }) {
  return (
    <div style={{ flex: 1, minHeight: 0, position: "relative", background: "var(--tasty-bg-sidebar)", display: "flex", alignItems: "center", justifyContent: "center", overflow: "hidden" }}>
      <div style={{ position: "relative", width: panned ? 280 : 200, height: panned ? 200 : 132, transform: panned ? "translate(24px,-12px)" : "none",
        background: "linear-gradient(135deg, var(--tasty-surface-raised), var(--tasty-bg-panel))",
        border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius-sm)", display: "flex", alignItems: "center", justifyContent: "center",
        color: "var(--tasty-text-disabled)" }}>
        <span style={{ display: "inline-flex", transform: "scale(1.4)" }}>{ic.image}</span>
        {children}
      </div>
    </div>
  );
}
function FloatingSelection() {
  const H = "calc(var(--tasty-image-handle-size) / -2)", MID = "calc(50% - var(--tasty-image-handle-size) / 2)";
  const handle = (style) => <span style={{ position: "absolute", width: "var(--tasty-image-handle-size)", height: "var(--tasty-image-handle-size)", background: "var(--tasty-accent-primary)", ...style }} />;
  return (
    <div style={{ position: "absolute", top: "50%", left: "50%", transform: "translate(-50%,-50%)", width: 96, height: 64,
      border: "1px solid var(--tasty-accent-primary)", background: "var(--tasty-bg-panel)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-text-muted)" }}>
      <span style={{ fontSize: 10, fontFamily: "var(--tasty-font-mono)" }}>pasted</span>
      {handle({ top: H, left: H })}{handle({ top: H, left: MID })}{handle({ top: H, right: H })}
      {handle({ top: MID, left: H })}{handle({ top: MID, right: H })}
      {handle({ bottom: H, left: H })}{handle({ bottom: H, left: MID })}{handle({ bottom: H, right: H })}
    </div>
  );
}
function ImgBar({ children }) {
  return <div style={{ display: "flex", alignItems: "center", gap: 8, flex: "none", height: 40, padding: "0 8px", background: "var(--tasty-bg-panel)", borderBottom: "1px solid var(--tasty-separator)" }}>{children}</div>;
}
function ImgSurface({ mode, panned, floating }) {
  return (
    <div style={{ width: "100%", maxWidth: 460, height: 320, display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      {mode === "viewer" ? (
        <ImgBar>
          <div style={{ display: "flex", gap: 1 }}><ImgBtn glyph={ic.back} label="Previous" /><ImgBtn glyph={ic.fwd} label="Next" /><ImgBtn glyph={ic.refresh} label="Refresh" /><ImgBtn glyph={ic.rename} label="Edit" /><ImgBtn glyph={ic.plus} label="New image" /></div>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>diagram.png (2/5)</span>
          <span style={{ flex: 1 }} />
          <ZoomGroup pct={panned ? "250%" : "100%"} />
        </ImgBar>
      ) : (
        <ImgBar>
          <Button variant="secondary" size="sm">Save</Button>
          <Button variant="ghost" size="sm">Cancel</Button>
          <ImgBtn glyph={<GIcon d={<path d="M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-1" />} />} label="Undo" />
          <ImgBtn glyph={<GIcon d={<path d="m15 14 5-5-5-5M20 9H9a5 5 0 0 0 0 10h1" />} />} label="Redo" disabled />
          <span style={{ width: 1, height: 18, background: "var(--tasty-separator)" }} />
          <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Brush</span>
          <span style={{ width: 60, height: 4, borderRadius: 2, background: "var(--tasty-surface-active)", position: "relative" }}><span style={{ position: "absolute", left: "40%", top: -3, width: 10, height: 10, borderRadius: "50%", background: "var(--tasty-text-muted)" }} /></span>
          <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>Color</span>
          <span style={{ width: 16, height: 16, borderRadius: "var(--tasty-radius-sm)", background: "var(--tasty-accent-danger)", border: "1px solid var(--tasty-border-strong)" }} />
          <span style={{ flex: 1 }} />
          <ZoomGroup />
        </ImgBar>
      )}
      <ImgCanvas panned={panned}>{floating && <FloatingSelection />}</ImgCanvas>
    </div>
  );
}

function Page() {
  return (
    <>
      <Section id="explorer" title="Explorer — the file-manager surface">
        <Spec id="editable-path-field-shared-address-component-explorer-markdown" title="Editable path field — shared address component (Explorer + Markdown)"
          when={<>Both the Explorer toolbar and the Markdown chrome now share <b>one</b> editable path field — a single design SoT for the “path address” role. <b>Display</b>: one mono path line (ellipsis, <span className="tok">--tasty-text-secondary</span>) behind a leading icon, with a trailing <b>Go</b> (arrow-right) button. <b>Click to edit</b>: focus ring + blink caret, text goes <span className="tok">--tasty-text-primary</span>; <span className="ic">↵</span> or <b>Go</b> navigate to the typed path, <span className="ic">Esc</span> reverts. While editing, a <b><a href={`${BASE}design/gallery/components/#forms`}>AutoComplete</a> candidate dropdown</b> hangs below the field — a typeahead of recent paths (Explorer = recent directories, Markdown = recent files) that narrows as you type; <span className="ic">↑↓</span> move the keyboard-active row, <span className="ic">↵</span>/Go confirm. The only per-surface parameters are the <b>leading icon</b> slot (Explorer <span className="ic">folderOpen</span> / Markdown <span className="ic">file</span>) and the candidate list. Field tokens align to <code>forms/Input</code>: <span className="tok">--tasty-input-bg</span> / <span className="tok">--tasty-input-border</span> (idle) → <span className="tok">--tasty-input-border-focus</span> (edit) + focus ring.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 18 }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, 300px)", gap: 20, justifyContent: "center" }}>
              {[["Explorer context — folderOpen · recent directories", ic.folderOpen, "~/Downloads", EXP_RECENT], ["Markdown context — file · recent files", ic.file, "~/work/tasty/docs/architecture.md", MD_RECENT]].map(([label, icon, path, candidates]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span style={{ width: 54, flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>idle</span>
                    <PathField icon={icon} path={path} />
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span style={{ width: 54, flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>editing</span>
                    <PathField icon={icon} path={path} editing />
                  </div>
                  <div style={{ display: "flex", gap: 10, minHeight: 200 }}>
                    <span style={{ width: 54, flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)", paddingTop: 6 }}>editing<br/>+ list</span>
                    <div style={{ flex: 1, position: "relative" }}>
                      <PathField icon={icon} path={path} editing candidates={candidates} open activeIndex={1} query="tasty" />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["display", "mono path · ellipsis · text-secondary"], ["editing", "focus ring (2px) + caret · text-primary"], ["suggestions", <>editing → <a href={`${BASE}design/gallery/components/#forms`}>AutoComplete</a> dropdown (recent paths)</>], ["leading icon", "per-surface slot — folderOpen / file"], ["candidates", "per-surface — recent dirs / recent files"], ["Go", "arrow-right · IconButton sm · ↵ also confirms"], ["navigate", "↵ / Go → typed path · Esc reverts"], ["height / radius", <>28 · <span className="tok">--tasty-radius</span></>]]}
            tokens={[{ tok: "--tasty-input-bg", use: "field fill", color: "var(--tasty-input-bg)" }, { tok: "--tasty-input-border", use: "idle border", color: "var(--tasty-input-border)" }, { tok: "--tasty-input-border-focus", use: "editing border", color: "var(--tasty-input-border-focus)" }, { tok: "--tasty-border-focus", use: "focus ring (35%)", color: "var(--tasty-border-focus)" }, { tok: "--tasty-autocomplete-menu-bg", use: "dropdown fill", color: "var(--tasty-autocomplete-menu-bg)" }, { tok: "--tasty-autocomplete-row-bg-active", use: "keyboard-active row", color: "var(--tasty-autocomplete-row-bg-active)" }, { tok: "--tasty-input-icon-fg", use: "leading icon", color: "var(--tasty-input-icon-fg)" }, { tok: "--tasty-accent-primary", use: "caret + Go glyph", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>Open questions resolved (design SoT): <b>(1) leading icon</b> is a per-surface slot — Explorer <span className="ic">folderOpen</span>, Markdown <span className="ic">file</span> — mirroring <code>forms/Input</code>'s leading-icon slot. <b>(2)</b> Explorer's <b>breadcrumb</b> (<code>Crumb</code> + chevron) is <b>dropped</b> for the plain editable field; a hybrid crumb↔edit form was <b>rejected</b>. <b>(3) Go</b> button on both surfaces. <b>(4)</b> only the field + dropdown are shared; the outer bar stays per-surface. <b>(5 — new)</b> the edit-time candidate dropdown is the formal <b><a href={`${BASE}design/gallery/components/#forms`}>AutoComplete</a></b> component (full state matrix — filtered / overflow-scroll / empty / hover vs keyboard-active — lives on the Components page), replacing the source-only <code>Combobox</code>. Source home: extract <code>PathField</code> into <code>crates/tasty-ui-widgets</code> composing the renamed <code>AutoComplete</code> (was <code>Combobox</code>); recent-path candidate source + wiring/i18n tracked separately from this design pass.</Note>
        </Spec>
        <Spec id="explorer-surface-full-layout-detail-view" title="Explorer surface — full layout (Detail view)"
          when={<>A full file manager living in a work-area tile, like the terminal/markdown surfaces. Top→bottom: an <b>internal directory tab bar</b> (lighter than the pane tab strip — each tab is its own cwd), a <b>toolbar</b> (Back / Forward / Up / Refresh · editable path field · grid/list/detail view toggle), then a body of a <b>196px sidebar</b> (Files tree over a Favorites list) feeding the <b>content area</b>. Default view is <b>Detail</b>; the address is an <b>editable path field</b> shared with the Markdown surface (click to type a path, <span className="ic">↵</span>/Go to jump to that directory).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <ExplorerFrame />
          </Stage>
          <Meta
            specs={[["surface", "fills a work-area tile"], ["internal tabs", <>28px · per-cwd, with <span className="ic">×</span> + <span className="ic">＋</span></>], ["toolbar", "44px · nav · path field · view toggle"], ["sidebar", <>196px — Files tree + Favorites</>], ["splitter", <>1px <span className="tok">--tasty-separator</span>, drag to resize</>], ["row height", <>28 (Detail) · <span className="tok">--tasty-table-cell-height</span> — the shared Table</>], ["detail header", "shared Table header — UI font · table-header-font-size 11 · weight medium · caps"], ["min height", <>180 · <span className="tok">--tasty-explorer-min-height</span> — the split drag stops here (tabs 28 + toolbar 44 + header 28 + 2 rows + status 24)</>], ["selected row", <span className="tok">--tasty-surface-active</span>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "surface + content", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-bg-sidebar", use: "tabs + sidebar + header", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-surface-raised", use: "view toggle container", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-segtoggle-on-bg", use: "view toggle — selected segment", color: "var(--tasty-segtoggle-on-bg)" }, { tok: "--tasty-input-bg", use: "path field", color: "var(--tasty-input-bg)" }, { tok: "--tasty-surface-active", use: "selected row (rows only)", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-warning", use: "favorite star", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-accent-primary", use: "active tab bar", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>Defaults chosen (briefs left these open): <b>Detail</b> is the default view; the right-hand preview panel is <b>dropped</b> in favour of a wider content area (re-add later as a toggle if needed); no in-toolbar filter search in this pass; the tree is rooted at <b>Home</b>. Favorites are global across surfaces.</Note>
        </Spec>

        <Spec id="view-toggle-segtoggle-a-segment-so-it-fills-settled" title="View toggle (SegToggle) — a segment, so it fills (settled)"
          when={<>The 2026-09-17 rule reads “<b>underline switches a view, fill picks a value</b>”. The Explorer view toggle looked like it sat on both sides: it is drawn as a fill, but what it changes is called <code>ExplorerViewMode</code>. It is a <b>segment</b>. “View” in the rule means <i>different content</i> — a tab strip swaps what is shown. grid / list / detail show the <b>same listing</b> drawn three ways; that is a setting, a value. So the selected segment takes the segment recipe already settled for the remote Connection toggle: <span className="tok">--tasty-accent-primary</span> fill + <span className="tok">--tasty-text-on-accent</span> ink — icon-only segments included, the glyph inherits the ink via <code>currentColor</code>. <span className="tok">--tasty-surface-active</span> leaves this control for good: it is <b>row selection</b>, never a segment fill. Off and hover are unchanged.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})}
                style={{ display: "flex", flexDirection: "column", gap: 10, padding: 14, background: "var(--tasty-bg-app)",
                  border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                {[["on = detail · off = grid, list", { value: "detail" }], ["hover on an off segment (list)", { value: "detail", hover: "list" }], ["on = grid", { value: "grid" }]].map(([cap, props]) => (
                  <div key={cap} style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 8, height: 44, padding: "0 8px", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)", border: "1px solid var(--tasty-border-default)" }}>
                      <PathField icon={ic.folderOpen} path="~/Downloads" />
                      <SegToggle {...props} />
                    </div>
                    <span style={{ width: 190, fontSize: 11, color: "var(--tasty-text-muted)" }}>{cap}</span>
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["verdict", "a VALUE — segment recipe (fill), same as remote Connection"], ["on", <><span className="tok">--tasty-segtoggle-on-bg</span> → accent-primary · <span className="tok">--tasty-segtoggle-on-fg</span> → text-on-accent</>], ["off", <><span className="tok">--tasty-text-muted</span> glyph, no fill</>], ["hover (off)", <span className="tok">--tasty-overlay-hover</span>], ["container", <>surface-raised + border-default 1px · radius · pad 4 · gap 4 · segment 24 × 20 (2026-10-07: the product dims) — width = pad*2 + seg*3 + gap*2 + border*2</>], ["ink contrast", "Mocha crust on blue ≈ 9.4:1 · Latte white on blue ≈ 4.8:1 — AA in both"], ["motion", "0ms — click swaps immediately"], ["surface-active", "row selection only — not a segment fill, not a tab fill"]]}
            tokens={[{ tok: "--tasty-segtoggle-on-bg", use: "selected segment fill", color: "var(--tasty-segtoggle-on-bg)" }, { tok: "--tasty-segtoggle-on-fg", use: "selected segment glyph", color: "var(--tasty-segtoggle-on-fg)" }, { tok: "--tasty-text-muted", use: "off glyph", color: "var(--tasty-text-muted)" }, { tok: "--tasty-surface-raised", use: "container", color: "var(--tasty-surface-raised)" }]} />
          <Note>Rule, extended: <b>underline switches content (tab strip); fill picks a value (segment) — including a value that only changes how the same content is drawn.</b> <span className="tok">--tasty-surface-active</span> is row selection and belongs to neither. No third branch is needed; the word “view” in a variable name does not make a control a tab strip.</Note>
          <Dont><b>Don't</b> draw a 2px underline inside the toggle container — the segments sit on the container's own border and an underline would collide with it; that vocabulary belongs to tab strips at the top of a surface.</Dont>
        </Spec>

        <Spec id="sidebar-favorites-populated-vs-empty-state" title="Sidebar Favorites — populated vs. empty state"
          when={<>The sidebar <b>Favorites</b> section caption is <b>always shown</b>, even at zero favorites — so the feature never disappears and reads as unimplemented. With favorites it lists STAR rows (left); empty, the caption stays and an <b>empty state</b> fills the slot: a faint outline star + <b>"No favorites yet"</b> in <span className="tok">--tasty-text-muted</span>, then a dimmer one-line hint pointing at the add path. Same tone as the content-area "This folder is empty" state — left-aligned to fit the 196px column.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>with favorites</div>
              <div style={{ height: 300, display: "flex", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><ExpSidebar height={300} /></div>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty — caption persists</div>
              <div style={{ height: 300, display: "flex", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><ExpSidebar height={300} favorites={[]} /></div>
            </div>
          </Stage>
          <Meta
            specs={[["caption", "always shown — micro caps, text-muted"], ["empty line", <>outline star + "No favorites yet" · <span className="tok">--tasty-text-muted</span></>], ["hint", <>1 line · <span className="tok">--tasty-text-placeholder</span></>], ["align", "left, inside 196px column"], ["condition", "was: hide section at 0 → now: always show"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "caption + empty line", color: "var(--tasty-text-muted)" }, { tok: "--tasty-text-placeholder", use: "hint line", color: "var(--tasty-text-placeholder)" }, { tok: "--tasty-accent-warning", use: "filled star (populated)", color: "var(--tasty-accent-warning-raised)" }]} />
          <Note>New string <InlineCode>explorer.sidebar.favorites_empty</InlineCode> — en "No favorites yet" · ko "아직 즐겨찾기가 없습니다" · add to <InlineCode>lang/&#123;en,ko,ja&#125;.toml</InlineCode>. The hint reuses existing "Add to favorites" copy. Implementer: replace <InlineCode>if !favorites.is_empty()</InlineCode> in <InlineCode>explorer.rs sidebar()</InlineCode> with an always-rendered caption + empty branch.</Note>
        </Spec>

        <Spec id="sidebar-layout-favorites-pinned-to-the-bottom-2-region-split" title="Sidebar layout — Favorites PINNED to the bottom (2-region split)"
          when={<>The sidebar is <b>not one scroll column</b> any more. It splits into two independent regions: <b>Files</b> takes the remainder at the top and scrolls on its own, and <b>Favorites</b> is <b>pinned to the bottom</b> at a computed fixed height — so however deep the tree gets, the favorites list is always on screen. The boundary is a <b>fixed 1px <span className="tok">--tasty-explorer-split-border</span></b> at a screen coordinate: it does <b>not</b> flow with content, so a short tree leaves empty <span className="tok">--tasty-bg-sidebar</span> above the line rather than pulling the line up. Pin height = <b>240</b> (<span className="tok">--tasty-explorer-favorites-pin-height</span>) while the sidebar body is ≥ <b>600</b> (<span className="tok">--tasty-explorer-favorites-pin-threshold</span>); below that it becomes <b>40%</b> of the body height (<span className="tok">--tasty-explorer-favorites-pin-ratio</span>), snapped to the 4px grid and floored at <b>120</b> (<span className="tok">--tasty-explorer-favorites-pin-min-height</span>). Row, caption and empty-state visuals are unchanged — this is a layout change only.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 18 }}>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center" }}>
              {[["long tree — Files scrolls, pin holds", "tree overflows · favorites intact", { tree: "long", favorites: FAVS_DEFAULT }],
                ["short tree — empty bg above a fixed line", "boundary does not move up", { tree: "short", favorites: FAVS_DEFAULT }],
                ["Favorites empty — caption + hint persist", "pinned region keeps its height", { tree: "long", favorites: [] }],
                ["many favorites — region scrolls itself", "list never expands past the pin", { tree: "long", favorites: FAVS_MANY }]].map(([label, sub, props]) => (
                <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-secondary)" }}>{label}</div>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{sub}</div>
                  <div style={{ display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                    <ExpSidebar height={620} {...props} />
                  </div>
                  <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>body 620 → pin {favPinHeight(620)}</div>
                </div>
              ))}
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center", alignItems: "flex-start" }}>
              {[620, 560, 420, 300].map((h) => (
                <div key={h} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>body {h} → pin {favPinHeight(h)}{h >= 600 ? " (240 flat)" : h === 300 ? " (min clamp)" : " (40%)"}</div>
                  <div style={{ display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                    <ExpSidebar height={h} tree="long" />
                  </div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["structure", "2 regions · independent scroll state"], ["Files region", "flex 1 · own scroll · caption pinned at its top"], ["Favorites region", <>fixed height, bottom-pinned · <span className="tok">--tasty-explorer-favorites-pin-height</span></>], ["pin height", "body ≥ 600 → 240 · else 40% (4px-snapped) · min 120"], ["boundary", <>1px <span className="tok">--tasty-explorer-split-border</span> at a fixed coordinate</>], ["sidebar width", <>196 unchanged · <span className="tok">--tasty-explorer-sidebar-width</span></>], ["resize", "recomputed from the live body height — no drag handle"], ["row visuals", "unchanged (tree row · star row · empty state)"]]}
            tokens={[{ tok: "--tasty-explorer-sidebar-width", use: "196 column" }, { tok: "--tasty-explorer-favorites-pin-height", use: "pinned region height" }, { tok: "--tasty-explorer-favorites-pin-threshold", use: "small-surface switch" }, { tok: "--tasty-explorer-favorites-pin-min-height", use: "lower clamp" }, { tok: "--tasty-explorer-split-border", use: "fixed boundary line", color: "var(--tasty-separator)" }, { tok: "--tasty-bg-sidebar", use: "both regions' fill", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-accent-warning", use: "filled star", color: "var(--tasty-accent-warning-raised)" }]} />
          <Note>Open decisions resolved (design SoT): <b>(1) threshold basis</b> = the <b>sidebar body height</b> (the split container itself), not the whole explorer tab — the pin only competes with the tree, and this keeps the rule local to <InlineCode>sidebar()</InlineCode>. <b>(2) scrollbar</b> = the shared <InlineCode>.tasty-scroll</InlineCode> behaviour, hover-revealed like every other scroll area — the pinned region gets no special always-on bar. <b>(3) slack space</b> at 0–1 favorites stays <b>empty <span className="tok">--tasty-bg-sidebar</span></b>: no filler padding, no vertical centering, so rows always start directly under the caption. <b>(4) boundary weight</b> = the <b>same</b> 1px separator as before — no shadow, no tint; "pinned" is communicated by behaviour (the line never moves, each side scrolls alone), not by extra decoration. <b>(5) 240 ↔ 40% jump</b> is left as a <b>hard switch</b> (no interpolation) as the requester allows, with a <b>120 floor</b> added so a very short surface still shows caption + ~3 rows. Implementer: split the single <InlineCode>ScrollArea</InlineCode> in <InlineCode>explorer.rs sidebar()</InlineCode> into two — <InlineCode>ScrollArea::vertical().id_salt("exp_tree")</InlineCode> in the top allocation and <InlineCode>id_salt("exp_favs")</InlineCode> inside a bottom-up allocated strip of the computed height — and draw the separator as the strip's top edge instead of an inline <InlineCode>ui.separator()</InlineCode>.</Note>
        </Spec>

        <Spec id="short-cell-favorites-drops-below-240-the-cell-stops-at-180" title="Short cell — Favorites drops below 240, the cell stops at 180"
          when={<>When the explorer is split low, the sidebar body can be shorter than the Favorites floor. Below <b>240</b> (<span className="tok">--tasty-explorer-favorites-hide-below</span> = the 120 Favorites floor + 120 for Files) the <b>Favorites region is not drawn</b> and Files takes the whole body; it comes back as soon as the body reaches 240 again. The explorer cell itself has a floor: the split drag stops at <b>180</b> (<span className="tok">--tasty-explorer-min-height</span>). <b>2026-10-08</b>: the old 160 counted toolbar + status line + two rows but left out the internal tab bar and the Detail header, so the app showed only <code>..</code> at the floor. 180 is the full stack, top to bottom: internal tabs 28 · toolbar 44 · Detail header 28 · <b>two rows</b> 56 · status line 24. The explorer's internal layout does not change at the floor (no smaller header or status line); the floor follows the stack. The status line always has its place and Detail shows two rows. Nothing is cut mid-row. <b>Outside a drag (2026-10-09 wording)</b>: a <b>split</b> (<code>tasty split</code> or the split menu) that would leave an explorer cell under the floor places the new divider so the explorer keeps 180 and the sibling takes the rest. Only explorer cells have a floor; a sibling of another kind has none of its own, but a new split never leaves it under <b>56</b> (<span className="tok">--tasty-split-sibling-min-height</span> = pane tab strip 28 + one 28 row), so no sliver cell is made. The split is <b>refused</b> (no change) when that 56 can't be kept, when two explorer cells can't both keep 180, or when an explorer cell already under 180 would be split further. User split → info Toast “Not enough room to split this pane”; agent split → error <code>split refused: not enough room</code>. The check uses the largest view showing the workspace; with no window showing it, the floor is skipped. A <b>window resize</b> does not hold the floor — the cell may shrink below 180. <b>Short body</b>: when the content body is under <b>120</b> (<span className="tok">--tasty-explorer-state-compact-below</span>) every status screen switches to a <b>one-line compact row</b> — 16px glyph, title (ellipsis), and the state's buttons; the OS reason and the sub line move to the title tooltip. Retry and Go up stay on the row. Nothing is clipped.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[300, 240, 200, 84].map((h) => (
              <div key={h} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>body {h} → {h >= 240 ? "pin " + favPinHeight(h) : "Files only"}{h === 84 ? " (cell at its 180 floor)" : ""}</div>
                <div style={{ display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <ExpSidebar height={h} tree="long" />
                </div>
              </div>
            ))}
          </Stage>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 12, flexDirection: "column", alignItems: "stretch" }}>
            <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>content body 84 (cell at 180) → compact row</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 12, width: "100%", maxWidth: 440 }}>
              <ExpState compact glyph={<PIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-explorer-error-fg)" title="Can't read this folder" reason="No such file or directory (os error 2)"
                actions={<><Button variant="secondary" size="sm">Retry</Button><Button variant="ghost" size="sm">Go up</Button></>} />
              <ExpState compact glyph={ic.lock} glyphColor="var(--tasty-accent-warning-raised)" title="Permission denied" sub="You don't have access to read this folder." />
              <ExpState compact glyph={ic.folderOpen} title="This folder is empty" />
            </div>
          </Stage>
          <Meta
            specs={[["hide Favorites", <>body &lt; 240 · <span className="tok">--tasty-explorer-favorites-hide-below</span></>], ["Files only", "caption + tree, own scroll, full body"], ["return", "body ≥ 240 → pin ladder as before (120 floor)"], ["cell floor", <>180 · <span className="tok">--tasty-explorer-min-height</span> · split drag stops here · tabs 28 + toolbar 44 + header 28 + 2×28 + status 24</>], ["split", "explorer keeps 180, sibling takes the rest (≥ 56 · split-sibling-min-height); refused if 56 can't be kept, if two explorer cells can't both keep 180, or if an explorer under 180 would shrink further"], ["refusal", "user → info Toast “Not enough room to split this pane” · agent → split refused: not enough room"], ["window resize", "floor not held"], ["compact state", <>content body &lt; 120 · <span className="tok">--tasty-explorer-state-compact-below</span> · glyph · title · buttons on one row, reason in tooltip</>], ["scope", "floor = explorer cells only · other kinds have no floor (0) during drags and resizes; 56 guards only a new split"], ["sidebar", "never hidden as a whole"]]}
            tokens={[{ tok: "--tasty-explorer-favorites-hide-below", use: "→ size-240" }, { tok: "--tasty-explorer-min-height", use: "→ size-180 (was 160)" }, { tok: "--tasty-explorer-state-compact-below", use: "→ size-120" }, { tok: "--tasty-split-sibling-min-height", use: "→ size-56 (2026-10-09)" }, { tok: "--tasty-explorer-favorites-pin-min-height", use: "120 floor (unchanged)" }]} />
          <Note>The Favorites height rule already reads tokens (<span className="tok">--tasty-explorer-favorites-pin-height</span> 240, <span className="tok">-pin-threshold</span> 600, <span className="tok">-pin-ratio</span> 0.4, <span className="tok">-pin-min-height</span> 120). The three dimensions scale with the UI zoom like the other explorer tokens; the ratio is unitless and does not.</Note>
        </Spec>

        <Spec id="view-modes-grid-list-detail" title="View modes — Grid · List · Detail">
          <Stage variant="tight">
            <div style={{ display: "flex", gap: 16, padding: 14, background: "var(--tasty-bg-panel)" }}>
              <ViewModeColumn title="Grid (icons)" sub="icon 16 + 3-line name (…); image = thumbnail slot"><ExpGridMini /></ViewModeColumn>
              <ViewModeColumn title="List" sub="small icon + name, one dense column"><ExpListMini /></ViewModeColumn>
              <ViewModeColumn title="Detail" sub="sortable columns: Name · Size · Date · Type"><div style={{ display: "flex", flexDirection: "column", flex: 1 }}><DetailHeader /><div style={{ flex: 1, background: "var(--tasty-bg-panel)" }}><DetailRow glyph={ic.folder} name="mockup-exports" size="—" date="06-20 14:30" type="Folder" /><DetailRow glyph={ic.file} name="report.pdf" size="2.4 MB" date="06-24 09:12" type="PDF" /><DetailRow glyph={ic.image} name="diagram.png" size="488 KB" date="06-26 18:05" type="PNG" state="selected" glyphColor="var(--tasty-accent-info-raised)" /></div></div></ViewModeColumn>
            </div>
          </Stage>
          <Meta
            specs={[["grid cell", "80px · icon 16 + 3-line label wrap (…)"], ["list row", <>22 · <span className="tok">--tasty-tree-row-height</span> (same row as the file tree) · icon + name</>], ["detail row / header", <>28 · <span className="tok">--tasty-table-cell-height</span> (same as the shared Table) · 4 columns</>], ["cell states", "default · hover (8%) · selected (12%) · cut (50%) · renaming"], ["sort", "click a column header → ▲ / ▼"]]}
            tokens={[{ tok: "--tasty-tree-row-height", use: "List row (22)" }, { tok: "--tasty-table-cell-height", use: "Detail row + header (28)" }, { tok: "--tasty-overlay-hover", use: "row hover (8%)" }, { tok: "--tasty-surface-active", use: "selected (12%)", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-info", use: "image-file glyph", color: "var(--tasty-accent-info-raised)" }]} />
          <Note><b>Grid cell</b> (updated): icon shrinks to <span className="tok">icon_glyph_size_md</span> (16, was 28) and the label wraps up to <b>3 lines</b> at <span className="tok">font_size_caption</span> (11) with <b>… overflow</b> on the last line — replacing the old 12-char hard cut. Height is <b>fixed to 3 label lines</b> so grid rows stay uniform (short names reserve the space, label top-aligned). CELL_W stays 80. Real side: <InlineCode>LayoutJob</InlineCode> with <InlineCode>wrap.max_rows = 3</InlineCode> + <InlineCode>overflow_character = '…'</InlineCode>, drop <InlineCode>truncate(&amp;e.name, 12)</InlineCode>, recompute <InlineCode>cell_h</InlineCode>. <b>Cut</b> items read <span className="tok">--tasty-cut-pending-fg</span> with an italic name until pasted, no opacity. A <b>renaming</b> cell swaps its label for an inline Input (extension preserved). Image cells show a thumbnail in the icon slot at implementation time — here a tinted glyph stands in. <b>Row heights (2026-09-29)</b>: no Explorer-specific height. List reuses the tree row (<span className="tok">--tasty-tree-row-height</span> 22), Detail reuses the shared Table (<span className="tok">--tasty-table-cell-height</span> 28 for both header and body rows). The old 24 / 26 literals are retired. Implementer: the Table widget must read <InlineCode>table_cell_height</InlineCode> for <InlineCode>row_height</InlineCode> and <InlineCode>header_height</InlineCode> instead of its font-derived default (~35).</Note>
        </Spec>

        <Spec id="right-click-context-menu-target-resolves-to-4-shapes" title="Right-click context menu — target resolves to 4 shapes"
          when={<>Finder/VS Code rule: right-clicking an item <b>inside the selection</b> targets the whole selection; <b>outside</b> it, selection collapses to that one item; on the <b>empty background</b>, the target is the current directory (cwd). Menu items appear/disappear accordingly, grouped by divider into <b>clipboard · file ops · system</b>.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center" }}>
              <CtxMenu title="Background (empty area) → cwd">
                <MenuItem label="Copy path" icon={ic.link} />
                <MenuItem label="Add to favorites" icon={ic.star} />
                <MenuItem label="Paste" icon={ic.paste} shortcut={<Kbd keys="Ctrl+V" />} />
              </CtxMenu>
              <CtxMenu title="Single file">
                <MenuItem label="Copy path" icon={ic.link} />
                <MenuItem label="Copy" icon={ic.copy} shortcut={<Kbd keys="Ctrl+C" />} />
                <MenuItem label="Cut" icon={ic.scissors} shortcut={<Kbd keys="Ctrl+X" />} />
                <MenuItem separator />
                <MenuItem label="Rename" icon={ic.rename} shortcut={<Kbd keys="F2" />} />
                <MenuItem label="Move to Trash" icon={ic.trash} danger shortcut={<Kbd keys="Del" />} />
              </CtxMenu>
              <CtxMenu title="Single folder">
                <MenuItem label="Copy path" icon={ic.link} />
                <MenuItem label="Add to favorites" icon={ic.star} />
                <MenuItem label="Copy" icon={ic.copy} shortcut={<Kbd keys="Ctrl+C" />} />
                <MenuItem label="Cut" icon={ic.scissors} shortcut={<Kbd keys="Ctrl+X" />} />
                <MenuItem label="Paste into" icon={ic.paste} />
                <MenuItem separator />
                <MenuItem label="Rename" icon={ic.rename} shortcut={<Kbd keys="F2" />} />
                <MenuItem label="Move to Trash" icon={ic.trash} danger shortcut={<Kbd keys="Del" />} />
                <MenuItem separator />
                <MenuItem label="Open in system" icon={ic.external} />
              </CtxMenu>
              <CtxMenu title="Multiple selected (3)">
                <MenuItem label="Copy paths" icon={ic.link} />
                <MenuItem label="Copy" icon={ic.copy} shortcut={<Kbd keys="Ctrl+C" />} />
                <MenuItem label="Cut" icon={ic.scissors} shortcut={<Kbd keys="Ctrl+X" />} />
                <MenuItem separator />
                <MenuItem label="Move to Trash" icon={ic.trash} danger shortcut={<Kbd keys="Del" />} />
              </CtxMenu>
            </div>
          </Stage>
          <Meta
            specs={[["target", "in-selection → all · out → that one · empty → cwd"], ["favorites", "folder + cwd only"], ["paste", "shown when the clipboard holds files"], ["open in system", "single folder only"], ["groups", "clipboard · file ops · system (dividers)"]]}
            tokens={[{ tok: "--tasty-menu-bg", use: "menu fill", color: "var(--tasty-menu-bg)" }, { tok: "--tasty-menu-border", use: "1px edge" }, { tok: "--tasty-shadow-popover", use: "float" }, { tok: "--tasty-accent-danger", use: "Trash (danger item)", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note>Reuses the shared <span className="ic">MenuItem</span> / Popup visual language — no bespoke menu chrome. Shortcut hints are representative; real keys come from Keybindings settings.</Note>
        </Spec>

        <Spec id="empty-permission-loading-read-error-favorite-rename-popups" title="Empty / permission / loading / read error · favorite + rename popups"
          when={<>Status screens fill the content area; the two small editors reuse the Popup/rename visual language. <b>Read error (2026-10-07)</b>: any failure that is not a permission denial (path gone, not a directory, I/O error, remote read failure) gets its own state instead of falling through to “This folder is empty”: <b>alertTriangle</b> in <span className="tok">--tasty-explorer-error-fg</span> (→ accent-danger, as in the file picker error), title <b>“Can't read this folder”</b>, the <b>OS reason</b> as one mono caption line (text-muted, max 200, untranslated), and two buttons: <b>Retry</b> (secondary sm — re-reads the same path) and <b>Go up</b> (ghost sm — opens the parent; hidden at a root). Toolbar, tree and status line stay usable.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 16 }}>
            <div style={{ display: "flex", gap: 12, width: "100%", maxWidth: 700 }}>
              <ExpState glyph={ic.folderOpen} title="This folder is empty" />
              <ExpState glyph={ic.lock} glyphColor="var(--tasty-accent-warning-raised)" title="Permission denied" sub="You don't have access to read this folder." />
              <ExpState glyph={<Spinner />} title="Loading…" />
              <ExpState glyph={<PIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-explorer-error-fg)" title="Can't read this folder" reason="No such file or directory (os error 2)"
                actions={<><Button variant="secondary" size="sm">Retry</Button><Button variant="ghost" size="sm">Go up</Button></>} />
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center" }}>
              <div style={{ width: 300, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
                <div style={{ padding: "12px 14px 10px" }}>
                  <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 10 }}>Add to favorites</div>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 4 }}>Path</div>
                  <div style={{ fontSize: 12, fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)", marginBottom: 10 }}>~/Downloads</div>
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 4 }}>Name</div>
                  <Input block defaultValue="Downloads" />
                </div>
                <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "0 14px 12px" }}>
                  <Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">Add</Button>
                </div>
              </div>
              <div style={{ width: 300, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
                <div style={{ padding: "12px 14px 10px" }}>
                  <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 2 }}>Rename</div>
                  <div style={{ fontSize: 12, color: "var(--tasty-text-muted)", marginBottom: 10 }}>Press <Kbd keys="↵" /> to confirm, <Kbd keys="Esc" /> to cancel.</div>
                  <Input block autoFocus defaultValue="diagram.png" />
                  <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginTop: 6 }}>Extension <b style={{ color: "var(--tasty-text-secondary)" }}>.png</b> preserved.</div>
                </div>
                <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "0 14px 12px" }}>
                  <Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">Rename</Button>
                </div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["empty", "centered glyph + line"], ["permission", "warning (peach) tone"], ["loading", <>Spinner — busy-indicator policy</>], ["popups", "Popup language · ghost + primary footer"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "permission tone", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-spinner-indicator", use: "loading", color: "var(--tasty-spinner-indicator)" }, { tok: "--tasty-shadow-modal", use: "popup lift" }]} />
          <Note>Explorer font (family + size, 14px cap) gets an <b>Appearance › Explorer</b> sub-tab mirroring the Terminal/Markdown font sections — same control layout, deferred to the Settings specimen.</Note>
        </Spec>
      </Section>

      <Section id="markdown" title="Markdown viewer">
        <Spec id="markdown-read-only-viewer-address-bar-body" title="Markdown — read-only viewer (address bar + body)"
          when={<>The markdown surface is an <b>address-bar chrome</b> over a single vertical scroll body that fills the tile (body on <span className="tok">--tasty-md-doc-bg</span> = crust, reloads on file change). The chrome is native UI. The body is an HTML/CSS document in a <b>WebView</b>: colour, heading sizes, leading and block spacing all come from Tasty tokens through <code>theme_css()</code> (maps in <b>content colour</b> and <b>heading hierarchy</b> below). The drawing uses the same values.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <MarkdownDoc />
          </Stage>
          <Meta
            specs={[["chrome", <>40px address bar + Go, on <span className="tok">--tasty-bg-sidebar</span></>], ["body", <>13px · CSS line-height 1.6</>], ["headings", "h1 20 → h6 13, 5-step CSS interpolation (20 · 18.6 · 17.2 · 15.8 · 14.4 · 13)"], ["heading style", "600 · text-primary at every level"], ["h1", "prose-h1 — the only cap-exempt content size"], ["code", <>mono on <span className="tok">--tasty-surface-raised</span></>], ["link", <span className="tok">--tasty-accent-primary</span>], ["table", <>grid + zebra, read-only — <span className="tok">--tasty-md-table-*</span></>]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "chrome bar", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-md-doc-bg", use: "document bed (crust)", color: "var(--tasty-md-doc-bg)" }, { tok: "--tasty-input-bg", use: "address field", color: "var(--tasty-input-bg)" }, { tok: "--tasty-text-secondary", use: "body", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-primary", use: "headings · strong", color: "var(--tasty-text-primary)" }, { tok: "--tasty-surface-raised", use: "code bg + inline chip", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-accent-primary", use: "links + Go", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-md-table-border", use: "table grid (surface1)", color: "var(--tasty-md-table-border)" }, { tok: "--tasty-md-table-header-bg", use: "table header (surface0)", color: "var(--tasty-md-table-header-bg)" }, { tok: "--tasty-md-table-row-bg-zebra", use: "zebra row (mantle)", color: "var(--tasty-md-table-row-bg-zebra)" }, { tok: "--tasty-font-size-prose-h1", use: "h1 (cap-exempt)" }]} />
          <Note>The surface leads with a browser-style <b>address bar</b> (path display/edit) + <b>Go</b> button. The body follows the WebView/CSS path (2026-10-07); the earlier egui_commonmark renderer and its library-owned sizes are retired. Removed/redefined content tokens (<span className="tok">--tasty-font-size-prose-h2</span>, <span className="tok">--tasty-line-height-prose</span> removed; <span className="tok">--tasty-font-size-prose-h1</span> → Heading anchor) are covered in the heading-hierarchy specimen.</Note>
        </Spec>
        <Spec id="markdown-document-background-single-bed-webview-render-path" title="Markdown — document background (single bed, webview render path)"
          when={<>Under the webview render channel the markdown body is an HTML/CSS document drawn by the host WebView — there is <b>no focused/unfocused signal</b> on that path, so markdown has exactly <b>one</b> background (unlike the terminal, which swaps two). That bed is <span className="tok">--tasty-md-doc-bg</span> → <span className="tok">--tasty-surface-markdown-focused-bg</span> = <b>crust</b>. Pure black is a <i>terminal</i> convention (raw TTY) and sits outside the Catppuccin ramp; prose at the 14px cap on #000 halates. crust is the palette's deepest tone, so every content fill still steps up and stays discriminable: <b>crust</b> (page) &lt; <b>mantle</b> (table zebra) &lt; <b>base</b> &lt; <b>surface0</b> (code + table header) &lt; <b>surface1</b> (table grid). mantle would have swallowed the zebra stripe; base would have flattened the table's own fill against the page.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 12 }}>
            <div style={{ display: "flex", gap: 10, flexWrap: "wrap", justifyContent: "center" }}>
              {[["crust — chosen", "var(--tasty-color-neutral-0)", true], ["mantle — collides with zebra", "var(--tasty-color-neutral-100)", false], ["base — flattens table fill", "var(--tasty-color-neutral-200)", false], ["#000 — terminal only", "var(--tasty-color-black)", false]].map(([label, c, on]) => (
                <div key={label} style={{ width: 148, display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ height: 60, background: c, border: on ? "1px solid var(--tasty-accent-primary)" : "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", display: "flex", flexDirection: "column", justifyContent: "flex-end", padding: 6, gap: 3 }}>
                    <div style={{ height: 10, background: "var(--tasty-md-table-row-bg-zebra)" }} />
                    <div style={{ height: 10, background: "var(--tasty-md-code-bg)" }} />
                  </div>
                  <div style={{ fontSize: 11, color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{label}</div>
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["backgrounds", "one (no focus swap on the webview path)"], ["mocha", "crust #11111b"], ["latte", "crust #dce0e8 (tracks bg-app, no branch)"], ["terminal", "unchanged — pure black stays terminal-only"], ["unfocused_bg", "unused by the webview renderer"]]}
            tokens={[{ tok: "--tasty-md-doc-bg", use: "document bed → --md-bg", color: "var(--tasty-md-doc-bg)" }, { tok: "--tasty-md-code-bg", use: "code fill → --md-code-bg", color: "var(--tasty-md-code-bg)" }, { tok: "--tasty-md-table-row-bg-zebra", use: "zebra → --md-zebra", color: "var(--tasty-md-table-row-bg-zebra)" }, { tok: "--tasty-md-table-border", use: "grid → --md-border", color: "var(--tasty-md-table-border)" }, { tok: "--tasty-md-quote-bar", use: "blockquote bar", color: "var(--tasty-md-quote-bar)" }, { tok: "--tasty-md-rule", use: "horizontal rule", color: "var(--tasty-md-rule)" }]} />
          <Note>Resolves the copy-paste value in <code>[surfaces.markdown].focused_bg</code> (was <code>#000000</code>, identical to terminal). Implementing side: set mocha <code>focused_bg = crust</code>, drop the Latte white override, and keep <code>unfocused_bg</code> as-is — it is dead on the webview path.</Note>
        </Spec>
        <Spec id="markdown-content-colour-webview-css-2026-10-07" title="Markdown — content colour (WebView + CSS, 2026-10-07)"
          when={<>Markdown renders in a <b>WebView</b>. <code>render.rs theme_css()</code> writes Tasty tokens into CSS variables, and the stylesheet reads only those variables, so the page follows Mocha/Latte 1:1. Code highlighting is <b>highlight.js</b> (offline bundle): <code>hljs_css()</code> maps each <code>hljs-*</code> class to a palette role. Sizes, leading and block spacing are CSS too and belong to the next spec. The tables below are the source of truth (the old egui_commonmark / syntect path is retired).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: "100%", maxWidth: 720 }}><MdInjectionMap /></div>
          </Stage>
          <Meta
            specs={[["path", "WebView · theme_css() → CSS variables"], ["--md-fg", <><span className="tok">--tasty-text-secondary</span></>], ["--md-strong", <><span className="tok">--tasty-text-primary</span></>], ["--md-link", <><span className="tok">--tasty-accent-primary</span></>], ["--md-code-bg / -border", <><span className="tok">--tasty-surface-raised</span> / <span className="tok">--tasty-separator</span></>], ["--md-quote-bar · --md-rule", <><span className="tok">--tasty-border-strong</span> · <span className="tok">--tasty-separator</span></>], ["hljs", "keyword mauve · string green · title/function blue · number peach · type yellow · comment text-muted italic · tag teal · variable lavender · built_in red"], ["diff", "deletion / addition bg = accent-danger / accent-success at tint-fill-alpha"], ["callouts", "type colour bar (md-quote-bar-width) + type colour fill at tint-fill-alpha + radius + 16px type icon before the label · gaps 4 (md-callout-icon-gap) · collapsible: chevronRight at md-callout-marker-size (→ icon-size-sm) in the type colour, chevronDown when open, default <details> marker hidden · note accent-primary · tip accent-success · important accent-agent · warning accent-warning · caution accent-danger"]]}
            tokens={[{ tok: "--tasty-text-secondary", use: "body", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-primary", use: "headings / strong", color: "var(--tasty-text-primary)" }, { tok: "--tasty-accent-primary", use: "links", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-raised", use: "code fills", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-strong", use: "blockquote bar", color: "var(--tasty-border-strong)" }, { tok: "--tasty-separator", use: "code-block border", color: "var(--tasty-separator)" }]} />
          <Note>Both tables follow <code>theme_css()</code> / <code>hljs_css()</code> one row per variable or class; the two diff backgrounds are in the Meta. Both themes follow automatically because the palette roles re-point per theme.</Note>
        </Spec>
        <Spec id="address-bar-states-large-file-confirm" title="Address bar states · large-file confirm"
          when={<>The address bar is a browser-style path field. <b>Display</b> shows the current file path (mono, secondary); <b>click</b> to edit (focus ring + caret) and type a new path — <span className="ic">↵</span> or the <b>Go</b> button opens it in place, <span className="ic">Esc</span> reverts to the original. A non-<code>.md</code> extension still opens as markdown. Opening a file <b>over 1 MB</b> first raises a <b>surface-scoped</b> confirm — clamped to the tile (not a window scrim), same 360px shell as <code>markdown_open</code> — with Open / Cancel; Cancel opens nothing.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 16 }}>
            <div style={{ width: 440, maxWidth: "100%", display: "flex", flexDirection: "column", gap: 10 }}>
              <div><div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 6 }}>display (idle)</div>
                <div style={{ border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><MdAddressBar /></div></div>
              <div><div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 6 }}>editing (focus)</div>
                <div style={{ border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><MdAddressBar editing path="~/work/tasty/docs/design/systems/theme.md" /></div></div>
            </div>
            <div style={{ position: "relative", width: 440, maxWidth: "100%", height: 232, background: "var(--tasty-bg-panel)",
              border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <MdAddressBar path="~/work/tasty/docs/CHANGELOG-full.md" />
              <div data-audit-exempt="under-scrim" style={{ padding: "14px 18px" }}><div style={{ ...MD_H[1], margin: 0 }}>Changelog</div><div style={mdBody}>A very long history…</div></div>
              <div style={{ position: "absolute", inset: "40px 0 0 0", background: "rgba(0,0,0,.5)", display: "flex", alignItems: "center", justifyContent: "center", padding: 12 }}>
                <MdLargeFilePopup />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["field", "mono path · click to edit"], ["idle", <>secondary text, <span className="tok">--tasty-input-border</span></>], ["editing", <>focus ring + caret, <span className="tok">--tasty-input-border-focus</span></>], ["Go", "IconButton · ↵ also confirms"], ["popup scope", "surface (clamped to tile)"], ["popup", "360px · Open / Cancel"]]}
            tokens={[{ tok: "--tasty-input-bg", use: "field fill", color: "var(--tasty-input-bg)" }, { tok: "--tasty-input-border-focus", use: "edit border", color: "var(--tasty-input-border-focus)" }, { tok: "--tasty-border-focus", use: "focus ring", color: "var(--tasty-border-focus)" }, { tok: "--tasty-accent-warning", use: "size chip", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-shadow-modal", use: "popup lift" }, { tok: "--tasty-accent-primary", use: "Open (primary)", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>The address bar is the <b>shared editable path field</b> (see Explorer section) with a <span className="ic">file</span> leading icon — same field, same edit/navigate/revert contract as the Explorer toolbar, and the same edit-time <b><a href={`${BASE}design/gallery/components/#forms`}>AutoComplete</a></b> dropdown (candidates = recent files). Copy: title “Open large file?” · body “Over 1 MB — rendering may be slow.” · size chip formatted (e.g. “3.2 MB”). Surface-scoped (<code>PopupScope::Surface</code>) so it dims only this tile; Cancel replaces nothing. New i18n keys when wiring: <code>markdown.large_file.title / body</code>.</Note>
        </Spec>
        <Spec id="large-file-read-deferred-after-cancel" title="Large file — read deferred after Cancel"
          when={<>Cancelling the 1 MB confirm leaves the body on the neutral state block (<code>.tasty-state</code>, same place and colours as the empty-file state). The block now carries a way back: a <b>secondary sm Button “Open file…”</b> under the line, <span className="tok">--tasty-space-sm</span> below it. It <b>raises the same confirm again</b> (it does not read the file directly), so the 1 MB rule has one entry point. The ellipsis says a question follows. Hover / active are the Button's own derived states. Local and mirror documents both get it; the CLI <code>tasty markdown reload</code> stays as it is.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} {...(th ? { "data-theme": th } : {})} style={{ width: 320, height: 200, display: "flex", flexDirection: "column", background: "var(--tasty-md-doc-bg)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-lg)", textAlign: "center" }}>
                  <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-muted)" }}>This file is over 1 MB, so it hasn't been read yet.</span>
                  <Button variant="secondary" size="sm">Open file…</Button>
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["block", ".tasty-state — unchanged (centered, text-muted, body 13)"], ["action", "Button secondary sm · “Open file…” · under the line"], ["gap", <><span className="tok">--tasty-space-sm</span> line → button</>], ["click", "raises large-file-confirm again (Open / Cancel)"], ["Cancel again", "same block, button stays"]]}
            tokens={[{ tok: "--tasty-button-secondary-bg", use: "button fill", color: "var(--tasty-button-secondary-bg)" }, { tok: "--tasty-text-muted", use: "state line", color: "var(--tasty-text-muted)" }, { tok: "--tasty-space-sm", use: "line → button" }]} />
          <Note>WebView: render the button as a <code>&lt;button class="tasty-state-action"&gt;</code> styled from the injected Button variables; it posts the same IPC as the reload path. i18n: <code>markdown.state.large_deferred</code> (line) · <code>markdown.state.large_deferred_open</code> “Open file…”.</Note>
        </Spec>
        {<PluginsMdAddress_MdAddressSpecs />}
      </Section>

      <Section id="html" title="HTML viewer">
        <Spec id="markdown-heading-hierarchy-css-interpolated-2026-10-07" title="Markdown — heading hierarchy (CSS-interpolated, 2026-10-07)"
          when={<>Heading sizes are set by Tasty in CSS: <code>heading_sizes_px()</code> interpolates five steps from <b>h1</b> = <span className="tok">--tasty-font-size-prose-h1</span> (20, the only cap-exempt content size) down to <b>h6</b> = body (13) and writes them as <code>--md-h1</code>…<code>--md-h6</code>. Body line-height is <b>1.6</b> and block spacing is CSS as well. Weight and colour come from the same token map as the previous spec.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", padding: 24 }}>
              <TypeScaleSheet />
            </div>
          </Stage>
          <Meta
            specs={[["ladder", "CSS — 5-step interpolation 20 → 13 (--md-h1 … --md-h6)"], ["h1", "prose-h1 (20) · cap-exempt"], ["h2 … h5", "18.6 · 17.2 · 15.8 · 14.4"], ["h6", "body 13"], ["weight · colour", "600 · text-primary, every level"], ["leading", "CSS line-height 1.6"], ["block spacing", "CSS"], ["small", "11 · muted"]]}
            tokens={[{ tok: "--tasty-font-size-prose-h1", use: "Heading anchor (cap-exempt)" }, { tok: "--tasty-text-primary", use: "headings / strong", color: "var(--tasty-text-primary)" }, { tok: "--tasty-text-secondary", use: "body", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "de-emphasis", color: "var(--tasty-text-muted)" }]} />
          <Note>Load-fail / empty / loading states (proposed): a peach-toned <b>"Failed to load"</b> instead of raw <InlineCode>Error:</InlineCode> text, a centered <b>"This file is empty"</b>, and the Spinner for slow loads — all in the Explorer states pattern above. Markdown font gets its own <b>Appearance › Markdown</b> sub-tab. Size, leading and spacing are all Tasty's (CSS).</Note>
        </Spec>
      </Section>

      <Section id="html" title="HTML viewer">
        <Spec id="html-webview-chrome-4-states" title="HTML — webview chrome (4 states)"
          when={<>The html surface is <b>thin chrome + a native OS WebView overlay</b> — tasty does <b>not</b> theme the page pixels. What tasty paints is the tile skeleton before/instead of the overlay: <b>boundary</b> (region marker), <b>placeholder</b> (no URL), <b>loading</b> (Spinner), <b>error</b> (danger). Same tile (<span className="tok">--tasty-bg-panel</span> + 1px border), centered content. The address bar band above these states is specified below (2026-10-12).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 14, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>boundary</div><HtmlTile state="boundary" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>placeholder</div><HtmlTile state="placeholder" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>loading</div><HtmlTile state="loading" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>error</div><HtmlTile state="error" /></div>
          </Stage>
          <Meta
            specs={[["content", "native OS WebView (token-agnostic)"], ["tile", <><span className="tok">--tasty-bg-panel</span> + 1px <span className="tok">--tasty-border-default</span></>], ["boundary (2026-10-06)", "plain tile only (bg-panel + 1px border) — shown only while an egui overlay hides a loaded page; no glyph, label or URL / path"], ["placeholder", "GLOBE (disabled) + 'No page loaded'"], ["loading", "Spinner + 'Loading…'"], ["error", "ALERT (danger) + URL"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "placeholder / loading label", color: "var(--tasty-text-muted)" }, { tok: "--tasty-text-disabled", use: "placeholder / URL", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-spinner-indicator", use: "loading", color: "var(--tasty-spinner-indicator)" }, { tok: "--tasty-accent-danger", use: "error", color: "var(--tasty-accent-danger-raised)" }]} />
        </Spec>
        <Spec id="html-address-bar-band" title="HTML — address bar band"
          when={<>The page is a native WebView that tasty cannot paint over, so the address bar is a <b>host band above it</b> and the WebView rect starts below the band (same inset idea as the script banner). Composition = the Explorer toolbar recipe: <b>Back · Forward · Reload</b> (IconButton sm) then the <b>PathField box</b> — without the Go button (Enter opens; the band already carries three buttons). Height 40 and <span className="tok">--tasty-bg-sidebar</span> + 1px separator match the markdown address bar. Local files show as a <b>path</b> (<code>file://</code> input is accepted and shown as the path after open); web pages show the full URL. Loading swaps Reload for a Spinner in the same 24 cell. Input errors stand in a 24px row <b>inside the band</b> (danger edge on the field, glyph + message below); they clear on the next keystroke, on Esc, or on a successful open — never on a timer. Stack order under the band: <b>size banner → script banner → page</b>, banner-margin 8 above the first, banner-inset-gap 8 between and below.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[
              ["1 · empty tab — placeholder + hint", { kind: "empty", canBack: false }, "empty"],
              ["2 · local file — shown as a path", { kind: "local", address: "~/work/tasty/target/report.html", canBack: true }, "page"],
              ["3 · web page", { kind: "web", address: "https://docs.example.com/guide/getting-started#install", canBack: true, canForward: true }, "page"],
              ["4 · loading — Reload slot spins", { kind: "web", address: "https://docs.example.com/guide/", state: "loading", canBack: true }, "loading"],
              ["5 · load failed — band keeps the tried address", { kind: "web", address: "https://offline.example", canBack: true }, "failed"],
              ["6 · editing — focus edge + caret, keys stay in the field", { kind: "local", address: "~/work/tasty/target/rep", state: "editing" }, "page"],
              ["7a · input error — missing file, page unchanged", { kind: "local", address: "/tmp/x/missing.html", state: "error", error: "File not found: /tmp/x/missing.html" }, "page"],
              ["7b · input error — no base for a relative path", { kind: "empty", address: "docs/index.html", state: "error", canBack: false, error: "Can't resolve a relative path — no document or working folder" }, "empty"],
            ].map(([cap, bar, page]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <HtmlAddressSurfaceG width="var(--tasty-size-460)" height={240} bar={bar} page={page} url="https://offline.example" />
                </div>
              </div>
            ))}
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>8 · with the script banner — band, then the inset banner, then the page</span>
              <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                <HtmlAddressSurfaceG width="var(--tasty-size-560)" height={300} bar={{ kind: "local", address: "~/work/tasty/target/report.html" }} banners={["script"]} />
              </div>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>9 · mirror — read-only field, Reload only</span>
              <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                <HtmlAddressSurfaceG width="var(--tasty-size-460)" height={240} bar={{ kind: "remote", address: "build-eu:~/work/site/index.html" }} />
              </div>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>10 · narrow (360 &lt; 440) — Forward hidden</span>
              <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                <HtmlAddressSurfaceG width="var(--tasty-size-360)" height={300} narrow bar={{ kind: "web", address: "https://docs.example.com/guide/getting-started", canBack: true, canForward: true }} banners={["script"]} />
              </div>
            </div>
            {[["Latte · local + editing", { kind: "local", address: "~/work/tasty/target/rep", state: "editing" }, "page"], ["Latte · input error", { kind: "local", address: "/tmp/x/missing.html", state: "error", error: "File not found: /tmp/x/missing.html" }, "page"], ["Latte · empty tab", { kind: "empty", canBack: false }, "empty"]].map(([cap, bar, page]) => (
              <div key={cap} data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <HtmlAddressSurfaceG width="var(--tasty-size-460)" height={220} bar={bar} page={page} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["band", "40 · bg-sidebar · 1px separator below · padding-x 8"], ["nav", "Back · Forward · Reload — IconButton sm 24, gap 4 · disabled at history ends"], ["field", "PathField box (input tokens, 28, mono) · no Go · leading glyph file / link / html / remote"], ["address", "local = path (~ for home) · web = full URL · typed and shown the same"], ["loading", "Spinner in the Reload cell, not clickable"], ["error", "danger edge + 24 row in the band · clears on keystroke / Esc / successful open"], ["stack", "band → size banner → script banner → page · 8 / 8 / 8"], ["narrow", "< banner-narrow-below 440 → Forward hidden · field floor 96"], ["mirror", "read-only field + Reload only · band hidden while mirror HTML is a placeholder"], ["tooltips", "top — never down into the WebView"], ["empty", "keep glyph + 'No page loaded' · add hint + Alt+L / ⌘L"]]}
            tokens={[{ tok: "--tasty-html-address-bar-height", use: "→ size-40" }, { tok: "--tasty-html-address-bar-bg", use: "→ bg-sidebar", color: "var(--tasty-html-address-bar-bg)" }, { tok: "--tasty-html-address-bar-border", use: "→ separator", color: "var(--tasty-html-address-bar-border)" }, { tok: "--tasty-html-address-bar-padding-x", use: "→ space-sm" }, { tok: "--tasty-html-address-bar-gap", use: "→ space-sm" }, { tok: "--tasty-html-address-bar-nav-gap", use: "→ space-xs" }, { tok: "--tasty-html-address-field-min-width", use: "→ size-96" }, { tok: "--tasty-html-address-error-height", use: "→ size-24" }, { tok: "--tasty-html-address-error-glyph", use: "→ accent-danger", color: "var(--tasty-html-address-error-glyph)" }, { tok: "--tasty-input-border-invalid", use: "field edge on error", color: "var(--tasty-input-border-invalid)" }]} />
          <Note>An agent changing the address while the user edits does not touch the typed text or focus, and gets no extra marker — Esc reverts to whatever the current address is by then.</Note>
        </Spec>
        {<PluginsHtmlContrast_HtmlAddressContrastSpec />}
        <Spec id="html-mirror-remote-size-banners-and-the-receiving-state" title="HTML mirror — remote size banners and the receiving state"
          when={<>A mirror receives the entry document first, then each resource the page asks for. One file over 100 KB raises the <b>large-file banner</b> (name, size Tag, Decline / Allow); several waiting files are asked <b>one at a time</b>, with a “+N more waiting” count. Over 2 MB per entry document raises the <b>limit banner</b> (Stop / Continue) and auto receiving pauses. Both use the script banner's inset shell and button boxes, and sit <b>above</b> the script banner (they block rendering; the script banner is about the rendered document). Neither has × — each is a decision; neither takes keyboard focus. While a banner waits the page rect shows <b>Receiving remote files…</b> with a static glyph and “Paused”; while receiving runs it shows a Spinner with bytes · files received, so it reads differently from the local Loading state. Declined files leave a <b>“N skipped” Tag</b> in the read-only field (tooltip lists them) until Reload.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[
              ["1 · receiving the entry document", { banners: [], page: "receiving", progress: "18 KB · 1 file" }],
              ["2 · large file waiting (+2 more)", { banners: ["size-more"], page: "paused" }],
              ["3 · cumulative limit reached", { banners: ["limit"], page: "paused" }],
              ["4 · size banner + script banner", { banners: ["size", "script"], page: "paused", height: 360 }],
              ["5 · after declining — page shown, “2 skipped” in the field", { banners: [], page: "page", skipped: 2 }],
              ["6 · macOS 11–13, remote web address", { banners: [], page: "macos", address: "build-eu ↦ https://intranet.example/status" }],
              ["7 · disconnected before the page arrived", { banners: [], page: "disconnected", disconnected: true }],
            ].map(([cap, o]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <HtmlAddressSurfaceG width="var(--tasty-size-560)" height={o.height || 280} label={`${BASE}design/gallery/`} bar={{ kind: "remote", address: o.address || "build-eu:~/work/site/index.html", skipped: o.skipped, disconnected: o.disconnected }} banners={o.banners} page={o.page} progress={o.progress} />
                </div>
              </div>
            ))}
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>8 · narrow (360) — actions wrap under the body</span>
              <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                <HtmlAddressSurfaceG width="var(--tasty-size-360)" height={320} narrow label={`${BASE}design/gallery/`} bar={{ kind: "remote", address: "build-eu:~/work/site/index.html" }} banners={["size"]} page="paused" />
              </div>
            </div>
            {[["Latte · large file", ["size"]], ["Latte · limit", ["limit"]]].map(([cap, b]) => (
              <div key={cap} data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <HtmlAddressSurfaceG width="var(--tasty-size-560)" height={260} label={`${BASE}design/gallery/`} bar={{ kind: "remote", address: "build-eu:~/work/site/index.html" }} banners={b} page="paused" />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["placement", "inset, same as the script banner · above it when both show"], ["large file", "download glyph (info) · title · path (left-ellipsis) + size Tag warning · Decline ghost / Allow secondary"], ["several files", "one at a time · '+N more waiting'"], ["limit", "alertTriangle (warning) · received total · Stop ghost / Continue secondary"], ["dismiss", "no × · no keyboard focus · click only"], ["receiving", "Spinner + bytes · files (mono)"], ["paused", "static download glyph + 'Paused — answer the notice above'"], ["declined", "'N skipped' Tag in the read-only field, tooltip lists names · cleared by Reload"], ["macOS 11–13", "info glyph + copy in the page rect · band stays"], ["disconnected", "Reload disabled (tooltip says why) · page rect notice if nothing arrived; a received page stays"]]}
            tokens={[{ tok: "--tasty-html-size-banner-glyph", use: "→ accent-info", color: "var(--tasty-html-size-banner-glyph)" }, { tok: "--tasty-html-limit-banner-glyph", use: "→ accent-warning", color: "var(--tasty-html-limit-banner-glyph)" }, { tok: "--tasty-html-receiving-fg", use: "→ text-muted", color: "var(--tasty-html-receiving-fg)" }, { tok: "--tasty-html-receiving-glyph", use: "→ glyph-dim", color: "var(--tasty-html-receiving-glyph)" }, { tok: "--tasty-banner-inset-gap", use: "between banners + below the last" }]} />
        </Spec>
        <Spec id="html-settings-appearance-html-viewer" title="HTML — settings (Appearance › HTML viewer)">
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <HtmlSettings />
          </Stage>
          <Meta
            specs={[["pattern", "plugin settings page (label + control rows)"], ["default zoom", "number + % suffix"], ["color scheme", "select · follow / light / dark"], ["allow remote", "switch (off default)"], ["sandbox scripts", "switch (on default)"]]}
            tokens={[{ tok: "--tasty-select-bg", use: "scheme select", color: "var(--tasty-select-bg)" }, { tok: "--tasty-switch-track-bg-on", use: "enabled toggle", color: "var(--tasty-switch-track-bg-on)" }, { tok: "--tasty-input-bg", use: "zoom field", color: "var(--tasty-input-bg)" }]} />
          <Note>Reuses the existing plugin-settings row vocabulary (switch 28×16 · select · number) — no bespoke controls. Toolbar / address bar / find-in-page / retry stay <b>open decisions</b> (not in this pass).</Note>
        </Spec>
      </Section>

      <Section id="image" title="Image viewer">
        <Spec id="image-viewer-edit-paint-modes" title="Image — viewer & edit (paint) modes"
          when={<>The image surface is a single horizontal <b>control bar</b> over a <b>canvas</b>. Two modes toggle the bar: <b>viewer</b> (◀ ▶ ↻ ✎ ＋ · filename (i/n) · right-aligned zoom group) and <b>edit / paint</b> (Save · Cancel · undo/redo · brush size · color · zoom). Canvas fills with the <span className="tok">--tasty-bg-sidebar</span> tone so the image floats; ≤100% fits to window, &gt;100% pans on drag. ◀ ▶ show only when sibling images exist.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>viewer · fit (100%)</div><ImgSurface mode="viewer" /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>viewer · zoomed + panned (250%)</div><ImgSurface mode="viewer" panned /></div>
          </Stage>
          <Meta
            specs={[["layout", "control bar (40px) + canvas"], ["viewer bar", "◀ ▶ ↻ ✎ ＋ · name (i/n) · zoom"], ["canvas", <><span className="tok">--tasty-bg-sidebar</span> · fit ≤100% · pan &gt;100%</>], ["zoom group", "Fit · ＋ · % · −  (right)"], ["buttons", <>surface-raised + <span className="tok">--tasty-border-default</span></>]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "canvas (mantle)", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-surface-raised", use: "control buttons", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-text-muted", use: "filename · zoom %", color: "var(--tasty-text-muted)" }]} />
        </Spec>
        <Spec id="image-paint-bar-floating-selection-popups" title="Image — paint bar · floating selection · popups"
          when={<>Edit mode swaps in the paint bar (undo/redo with disabled state, brush-size slider, color swatch defaulting to <span className="tok">--tasty-accent-danger</span>). A pasted image becomes a <b>floating selection</b> — accent border + 8 resize handles — that you move/resize before committing. <b>New Image</b> and <b>Save As</b> are small popups.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}><div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>edit · floating selection</div><ImgSurface mode="edit" floating /></div>
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ width: "var(--tasty-image-popup-width)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
                <div style={{ padding: "var(--tasty-image-popup-pad-top) var(--tasty-image-popup-pad-x) var(--tasty-image-popup-gap)" }}>
                  <div style={{ fontSize: "var(--tasty-image-popup-title-font-size)", fontWeight: "var(--tasty-image-popup-title-weight)", marginBottom: "var(--tasty-image-popup-gap)" }}>New Image</div>
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-image-popup-btn-gap)", fontSize: 13, color: "var(--tasty-text-secondary)" }}>
                    <span>Width</span><Input style={{ width: "var(--tasty-image-size-input-width)", flex: "none" }} defaultValue="800" /><span style={{ color: "var(--tasty-text-muted)" }}>×</span><span>Height</span><Input style={{ width: "var(--tasty-image-size-input-width)", flex: "none" }} defaultValue="600" />
                  </div>
                </div>
                <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-image-popup-btn-gap)", padding: "0 var(--tasty-image-popup-pad-x) var(--tasty-image-popup-pad-top)" }}><Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">OK</Button></div>
              </div>
              <div style={{ width: "var(--tasty-image-popup-width)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
                <div style={{ padding: "var(--tasty-image-popup-pad-top) var(--tasty-image-popup-pad-x) var(--tasty-image-popup-gap)" }}>
                  <div style={{ fontSize: "var(--tasty-image-popup-title-font-size)", fontWeight: "var(--tasty-image-popup-title-weight)", marginBottom: "var(--tasty-image-popup-gap)" }}>Save As</div>
                  <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-image-path-row-gap)" }}>
                    <div style={{ flex: 1 }}><Input block placeholder="path/to/image.png" /></div>
                    <IconButton size="sm" aria-label="Browse">{ic.folderOpen}</IconButton>
                  </div>
                </div>
                <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-image-popup-btn-gap)", padding: "0 var(--tasty-image-popup-pad-x) var(--tasty-image-popup-pad-top)" }}><Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">Save</Button></div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["paint bar", "Save · Cancel · ↶ ↷ · brush · color · zoom"], ["undo/redo", "enabled / disabled (text-disabled)"], ["floating sel", <>accent border + 8 handles · 6 · <span className="tok">--tasty-image-handle-size</span> (on-scale), centred on the edge</>], ["popup card", <>300 · pad 12 / 14 / 10 · title 14 semibold, 10 below · buttons gap 8 · <span className="tok">--tasty-image-popup-*</span></>], ["size field", <>64 · <span className="tok">--tasty-image-size-input-width</span></>], ["path row gap", <>6 · <span className="tok">--tasty-image-path-row-gap</span></>], ["zoom %", <>mono 11 · min 40 · <span className="tok">--tasty-image-zoom-font-size</span></>], ["commit", "click outside = composite · Esc = cancel"], ["New Image", "Width × Height (1–8192)"], ["Save As", "path input + browse · PNG"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "floating selection + handles", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-danger", use: "default brush color", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-surface-active", use: "brush slider track", color: "var(--tasty-surface-active)" }, { tok: "--tasty-shadow-modal", use: "popups" }]} />
          <Note>Defaults (brief §6): metadata status-bar, filmstrip, async loading indicator, rotate/flip/crop tools, and transparency checkerboard are <b>out of scope</b> this pass — viewer + brush/paste paint only. The two canvas states (empty · load failed) are the next spec.</Note>
        </Spec>
        <Spec id="image-empty-canvas-vs-load-failed-save-as-name-clash" title="Image — empty canvas vs load failed · Save As name clash"
          when={<>The canvas reuses the <b>explorer state screen</b> (glyph · title · sub / OS reason · buttons, and its one-line <b>compact</b> form when the canvas is under 120) on the canvas tone <span className="tok">--tasty-bg-sidebar</span>. <b>Empty</b> (surface opened without a file) keeps the single muted line “No image loaded” and no buttons. <b>Load failed</b> is split by cause, each with its own title: <b>missing</b> (path gone or moved), <b>permission</b> (lock glyph, warning tone — the explorer's permission state), <b>decode</b> (damaged or unsupported; the decoder message goes on the mono reason line instead of only the log). Every failure has one button, <b>Retry</b> (secondary sm = the toolbar refresh). The file watcher still reloads by itself within 1 s, and ◀ ▶ in the bar keep working when the folder has other images. Save As opened because a <code>.png</code> of the same name exists gets a <b>caption above the field</b> in accent-warning and the field pre-filled with the next free name, stem selected.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 12 }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 12, width: "100%", maxWidth: 760 }}>
              <ExpState bg="var(--tasty-bg-sidebar)" glyph={ic.image} title="No image loaded" />
              <ExpState bg="var(--tasty-bg-sidebar)" glyph={<PIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-image-error-fg)" title="Image not found" sub="It may have been moved or deleted." reason="~/Pictures/diagram.png" actions={<Button variant="secondary" size="sm">Retry</Button>} />
              <ExpState bg="var(--tasty-bg-sidebar)" glyph={ic.lock} glyphColor="var(--tasty-accent-warning-raised)" title="Permission denied" sub="You don't have access to read this file." actions={<Button variant="secondary" size="sm">Retry</Button>} />
              <ExpState bg="var(--tasty-bg-sidebar)" glyph={<PIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-image-error-fg)" title="Can't open this image" sub="The file is damaged or in an unsupported format." reason="Format error decoding Png: invalid signature" actions={<Button variant="secondary" size="sm">Retry</Button>} />
              <ExpState bg="var(--tasty-bg-sidebar)" glyph={ic.image} glyphColor="var(--tasty-image-too-large-fg)" title="Image is too large to open" sub="Over 16384 px on a side, or needs more than 512 MiB to decode." reason="20000 × 300 px" />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%", maxWidth: 440 }}>
              <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>canvas under 120 → compact row</div>
              <ExpState compact bg="var(--tasty-bg-sidebar)" glyph={<PIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-image-error-fg)" title="Can't open this image" reason="Format error decoding Png: invalid signature" actions={<Button variant="secondary" size="sm">Retry</Button>} />
            </div>
            <div style={{ width: "var(--tasty-image-popup-width)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
              <div style={{ padding: "var(--tasty-image-popup-pad-top) var(--tasty-image-popup-pad-x) var(--tasty-image-popup-gap)" }}>
                <div style={{ fontSize: "var(--tasty-image-popup-title-font-size)", fontWeight: "var(--tasty-image-popup-title-weight)", marginBottom: "var(--tasty-image-popup-gap)" }}>Save As</div>
                <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-accent-warning-raised)", marginBottom: "var(--tasty-space-xs)" }}>diagram.png already exists. Save the PNG under another name.</div>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-image-path-row-gap)" }}>
                  <div style={{ flex: 1 }}><Input block defaultValue="~/Pictures/diagram-1.png" /></div>
                  <IconButton size="sm" aria-label="Browse">{ic.folderOpen}</IconButton>
                </div>
              </div>
              <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-image-popup-btn-gap)", padding: "0 var(--tasty-image-popup-pad-x) var(--tasty-image-popup-pad-top)" }}><Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">Save</Button></div>
            </div>
          </Stage>
          <Meta
            specs={[["states", "image-empty · image-load-failed-missing · -permission · -decode · -too-large (b10)"], ["gallery grid", "3 per row (b10) — never squeeze five into one row"], ["too large", "image glyph · image-too-large-fg (→ accent-warning; refused by limit, the file is fine) · title text-primary · sub with the limits · reason = real size “{w} × {h} px” (or “needs {mem}”) · NO Retry (same file, same result)"], ["units", "binary — MiB (also the explorer preview: 256 MiB, and its byte limit “Over 1 MiB.” b11)"], ["line breaks (b11)", "number + unit joined by NBSP in every language (“512 MiB”, “16384 px”) · state title + sub use balanced wrap (lines of near-equal length — no 1–2 character tail; CSS text-wrap: balance) · same rule for the explorer preview state"], ["layout", "explorer state screen on bg-sidebar · compact under explorer-state-compact-below 120"], ["empty", "image glyph · “No image loaded” · muted · no button"], ["missing / decode", <>alertTriangle · <span className="tok">--tasty-image-error-fg</span> (→ accent-danger) title</>], ["permission", "lock · accent-warning (same as explorer)"], ["reason", "mono caption · muted · path (missing) or decoder message (decode)"], ["button", "Retry · secondary sm · = refresh"], ["recovery", "watcher reload ≤ 1 s → viewer; ◀ ▶ still step the folder"], ["Save As clash", "caption 11 · accent-warning · above the field · field = next free name (name-1.png), stem selected"]]}
            tokens={[{ tok: "--tasty-image-error-fg", use: "→ accent-danger", color: "var(--tasty-image-error-fg)" }, { tok: "--tasty-image-too-large-fg", use: "→ accent-warning (b10)", color: "var(--tasty-image-too-large-fg)" }, { tok: "--tasty-accent-warning", use: "permission · name clash", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-bg-sidebar", use: "canvas", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-explorer-state-compact-below", use: "compact switch" }]} />
          <Note>Strings (en / ko / ja): <code>image.state.missing</code> “Image not found” / “이미지를 찾을 수 없습니다” / “画像が見つかりません” · sub “It may have been moved or deleted.” · <code>image.state.permission</code> “Permission denied” / “권한이 없습니다” / “アクセス権がありません” · <code>image.state.decode</code> “Can't open this image” / “이미지를 열 수 없습니다” / “この画像を開けません” · sub “The file is damaged or in an unsupported format.” · <code>image.save_as.exists</code> “{"{name}"} already exists. Save the PNG under another name.”</Note>
          <Note>ko / ja confirmed (b10): <code>missing_sub</code> “이동했거나 삭제되었을 수 있습니다.” / “移動または削除された可能性があります。” · <code>permission_sub</code> “이 파일을 읽을 권한이 없습니다.” / “このファイルを読み取る権限がありません。” · <code>decode_sub</code> “파일이 손상되었거나 지원하지 않는 형식입니다.” / “ファイルが破損しているか、対応していない形式です。” · Retry “다시 시도” / “再試行” · <code>save_as.exists</code> “{"{name}"} 파일이 이미 있습니다. 다른 이름으로 PNG를 저장하세요.” / “{"{name}"} は既に存在します。別の名前で PNG を保存してください。” · <code>too_large</code> “이미지가 너무 커서 열 수 없습니다” / “画像が大きすぎて開けません” · <code>too_large_sub</code> “한 변이 {"{px}"} px를 넘거나 디코딩에 {"{mem}"}보다 많은 메모리가 필요합니다.” / “一辺が {"{px}"} px を超えるか、デコードに {"{mem}"} を超えるメモリが必要です。” · markdown <code>large_deferred_open</code> “파일 열기…” / “ファイルを開く…” (kept). Gallery stays <b>en-only</b>; ko/ja live in these notes and the app's lang files.</Note>
        </Spec>
      </Section>
    </>
  );
}


export const page = {
  active: "plugins",
  nav: NAV,
  head: {
    title: "Plugin surfaces",
    intro: "Explore the Explorer and the Markdown, HTML, and image viewers. These tools sit alongside terminals in the work area, so you can read files and check results without leaving your workspace.",
    
  },
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
