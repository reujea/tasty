import React from "react";
import { IconButton, Input, Icon, Kbd } from "../../components/index.js";
import { Section, Spec, Stage, Meta, Note, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Icons. The one canonical glyph set. Every icon the
// product draws (toolbars, IconButtons, tree rows, inline status) lives
// as a file in icons/<name>.svg and is rendered by name through the DS
// <Icon/> component — so an overlay reaches for `<Icon name="close"/>`
// instead of redrawing the X path by hand. All glyphs: 24×24 viewBox,
// 2px stroke, round caps, no fill, currentColor — they inherit the text
// color of whatever wraps them (an IconButton, a muted header span, an
// accent warning line). This page renders straight from ICON_PATHS, so
// it is always an exact mirror of the icon files.



// ── Canonical glyph catalogue ───────────────────────────────────────────
// Grouped by job. Each entry is just a name (which file / registry key it
// is) + the role it plays — the geometry lives in icons/<name>.svg, never
// here. Adding a glyph: create the .svg, mirror it into Icon.jsx's
// ICON_PATHS, then list its name below.
const GROUPS = [
  {
    id: "actions",
    title: "Actions",
    when: <>The verbs. These are what an <b>IconButton</b> wraps in a toolbar or row — <code>close</code> and <code>refresh</code> sit in every overlay header; <code>edit</code> / <code>trash</code> / <code>copy</code> in list rows. Draw them from here, never re-inline the path.</>,
    icons: [
      { name: "plus", role: "create / add" },
      { name: "close", role: "dismiss overlay (header X)" },
      { name: "refresh", role: "re-scan / reload (ports, re-detect)" },
      { name: "edit", role: "edit a row / value" },
      { name: "trash", role: "delete / remove" },
      { name: "copy", role: "copy to clipboard" },
      { name: "check", role: "confirm / saved / done" },
      { name: "search", role: "filter / search affordance" },
      { name: "filter", role: "filter funnel" },
      { name: "swap", role: "swap / switch direction" },
      { name: "more", role: "more actions — overflow menu trigger (banner ⋯)" },
      { name: "download", role: "download / save to disk" },
      { name: "star", role: "favorite (outline)" },
      { name: "starFill", role: "favorite (active / filled)" },
    ],
  },
  {
    id: "nav",
    title: "Navigation & disclosure",
    when: <>Movement and open/closed state. Single chevrons are <b>tree-row disclosure</b> and menu affordances; the doubled chevrons <b>collapse / expand the sidebar rail</b>.</>,
    icons: [
      { name: "chevronRight", role: "collapsed row · forward" },
      { name: "arrowRight", role: "Go — navigate to the typed address (Markdown address bar)" },
      { name: "chevronDown", role: "expanded row" },
      { name: "chevronUp", role: "collapsed row · previous / up" },
      { name: "chevronLeft", role: "back" },
      { name: "chevronsLeft", role: "collapse sidebar" },
      { name: "chevronsRight", role: "expand sidebar rail" },
      { name: "move", role: "move / reposition (4-way)" },
      { name: "fit", role: "fullscreen stage (popup title bar)" },
    ],
  },
  {
    id: "surfaces",
    title: "Surfaces & workspace",
    when: <>The nouns of the workspace — what a <b>tab</b>, <b>tree row</b>, or new-surface button shows. <code>terminal</code> and <code>markdown</code> are the two core surface kinds.</>,
    icons: [
      { name: "terminal", role: "terminal surface / tab" },
      { name: "agent", role: "agent turn — DAG agent task (Claude / Codex)" },
      { name: "markdown", role: "markdown surface / tab" },
      { name: "html", role: "html / web surface (URL tab)" },
      { name: "split", role: "split a pane (vertical divider)" },
      { name: "splitH", role: "split a pane (horizontal divider)" },
      { name: "paneEmpty", role: "empty pane / no surface" },
      { name: "folder", role: "folder / workspace" },
      { name: "folderOpen", role: "folder / workspace (open)" },
      { name: "file", role: "file leaf" },
      { name: "folderPlus", role: "new folder (explorer toolbar / menu)" },
      { name: "filePlus", role: "new file (explorer toolbar / menu)" },
      { name: "info", role: "properties / information" },
      { name: "link", role: "symbolic link (explorer Properties / rows) — 2026-10-09 batch 10" },
      { name: "image", role: "image surface / preview" },
      { name: "list", role: "log surface / list lines" },
      { name: "layoutGrid", role: "gallery / grid view mode" },
      { name: "layoutDetail", role: "detail view mode" },
      { name: "listView", role: "list view mode" },
      { name: "layers", role: "layout presets / stacked layers" },
      { name: "columns", role: "column split layout" },
      { name: "clipboard", role: "clipboard viewer" },
      { name: "textLeft", role: "text content / paragraph" },
      { name: "scriptFile", role: "script / lua file" },
      { name: "remote", role: "remote connection" },
      { name: "port", role: "listening port / target" },
      { name: "gitBranch", role: "git branch" },
      { name: "gitTree", role: "git tree / lineage" },
    ],
  },
  {
    id: "visibility",
    title: "Visibility",
    when: <>The reveal toggle on secret values (passkeys, env). They swap in place on the same <b>IconButton</b> — <code>eye</code> when hidden, <code>eyeOff</code> when shown.</>,
    icons: [
      { name: "eye", role: "reveal value" },
      { name: "eyeOff", role: "hide value" },
      { name: "lock", role: "locked / secret held back" },
      { name: "globe", role: "remote content allowed (network)" },
      { name: "globeOff", role: "remote content blocked (network)" },
    ],
  },
  {
    id: "status",
    title: "Status & alerts",
    when: <>Inline meaning markers, tinted by the line they sit in (warning amber, success green, danger red) via <code>currentColor</code>. Not for state <i>dots</i> — that's <span className="ic">StatusDot</span>; these carry a message.</>,
    icons: [
      { name: "alertTriangle", role: "warning / unverified" },
      { name: "alertCircle", role: "error / failed" },
      { name: "helpCircle", role: "help hint · (?) tooltip trigger" },
      { name: "shieldCheck", role: "trusted / signed" },
      { name: "bell", role: "notification" },
    ],
  },
  {
    id: "system",
    title: "Tools & system",
    when: <>The sidebar footer and global tools — each anchors a menu or window. <code>tools</code> opens the Tools menu, <code>settings</code> the Settings window, <code>plug</code> the Plugins window.</>,
    icons: [
      { name: "tools", role: "Tools menu" },
      { name: "settings", role: "Settings window" },
      { name: "plug", role: "Plugins" },
      { name: "rocket", role: "getting started / launch" },
      { name: "command", role: "generic command / palette entry" },
      { name: "theme", role: "theme toggle (Mocha / Latte)" },
      { name: "keyboard", role: "keybinding / shortcut" },
      { name: "mouse", role: "mouse / pointer capture" },
      { name: "sun", role: "empty state / no settings" },
      { name: "hash", role: "number / tab-switch digits" },
    ],
  },
  {
    id: "keys",
    title: "Modifier keys (macOS)",
    when: <>The three macOS modifier <b>keycap glyphs</b>, as vectors. They exist because the app renders with its own bundled font and the unicode characters (⌘ U+2318, ⌥ U+2325, ⇧ U+21E7) fall back to tofu — so a modifier symbol is drawn as an <span className="ic">&lt;Icon/&gt;</span>, never typed as text. Note <code>cmdKey</code> is the real ⌘ keycap glyph; the older <code>command</code> in <b>Tools &amp; system</b> is the generic "a command" concept icon and is <b>not</b> a substitute.</>,
    icons: [
      { name: "cmdKey", role: "Command key symbol (⌘)" },
      { name: "optionKey", role: "Option key symbol (⌥)" },
      { name: "shiftKey", role: "Shift key symbol (⇧)" },
    ],
  },
];

const NAV = [{ id: "keys-in-use", label: "Modifier symbols in use" }, ...GROUPS.map((g) => ({ id: g.id, label: g.title }))];

function Tile({ name, role }) {
  return (
    <div className="icontile" title={role}>
      <span className="glyph"><Icon name={name} size={22} /></span>
      <span className="iname">{name}</span>
      <span className="irole">{role}</span>
    </div>
  );
}

function Icons() {
  return (
    <>
      {/* the system rules, once, up top */}
      <Section id="system-rules" title="The icon system">
        <Spec id="one-geometry-recolored-by-context" title="One geometry, recolored by context"
          when={<>Every Tasty glyph is a <b>24×24 viewBox, 2px stroke, round cap/join, no fill</b>, drawn in <code>currentColor</code>. An icon never carries its own color — it inherits from the control around it. Render one by name with <span className="ic">&lt;Icon name="…"/&gt;</span>; size it through the <code>size</code> prop, not by editing the path.</>}>
          <Stage style={{ gap: 30, alignItems: "center" }}>
            <div className="cluster">
              <span className="lbl">size scale — set via size prop</span>
              <div className="row" style={{ alignItems: "flex-end", gap: 22 }}>
                {[26, 20, 16, 14, 12].map((s) => (
                  <div key={s} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, color: "var(--tasty-text-secondary)" }}>
                    <Icon name="settings" size={s} />
                    <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{s}px</span>
                  </div>
                ))}
              </div>
            </div>
            <div className="cluster">
              <span className="lbl">inherits currentColor</span>
              <div className="row" style={{ gap: 18 }}>
                <span style={{ color: "var(--tasty-text-muted)", display: "inline-flex" }}><Icon name="refresh" size={18} /></span>
                <span style={{ color: "var(--tasty-accent-warning-raised)", display: "inline-flex" }}><Icon name="alertTriangle" size={18} /></span>
                <span style={{ color: "var(--tasty-accent-success-raised)", display: "inline-flex" }}><Icon name="shieldCheck" size={18} /></span>
                <span style={{ color: "var(--tasty-accent-danger-raised)", display: "inline-flex" }}><Icon name="alertCircle" size={18} /></span>
              </div>
            </div>
            <div className="cluster">
              <span className="lbl">in an IconButton — the common case</span>
              <div className="row">
                <IconButton aria-label="Refresh"><Icon name="refresh" /></IconButton>
                <IconButton aria-label="Close"><Icon name="close" /></IconButton>
                <Input icon={<Icon name="search" />} placeholder="Filter…" style={{ width: 170 }} />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[
              ["source", <>one file per glyph — <span className="tok">icons/&lt;name&gt;.svg</span></>],
              ["render", <><span className="tok">&lt;Icon name="close"/&gt;</span></>],
              ["viewBox", "0 0 24 24"],
              ["stroke", <>2px · round cap + join</>],
              ["fill", "none by default; per-glyph `fill` flag (icons.json) — only starFill is filled"],
              ["color", <>inherits <span className="tok">currentColor</span></>],
              ["sizes", "26 / 20 / 16 (default) / 14 / 12"],
            ]}
            tokens={[
              { tok: "--tasty-text-muted", use: "rest glyph", color: "var(--tasty-text-muted)" },
              { tok: "--tasty-accent-warning", use: "warn tint", color: "var(--tasty-accent-warning-raised)" },
              { tok: "--tasty-accent-success", use: "ok tint", color: "var(--tasty-accent-success-raised)" },
              { tok: "--tasty-accent-danger", use: "error tint", color: "var(--tasty-accent-danger-raised)" },
            ]} />
          <Note>Geometry lives in the <b>icons/*.svg</b> files (exported to <code>icons.json</code>); <span className="ic">&lt;Icon/&gt;</span> embeds the same registry so it renders synchronously and recolors from the wrapping control. Pull a glyph <b>by name</b> — never paste a new <span className="ic">&lt;path&gt;</span>, never <span className="ic">&lt;img&gt;</span> an .svg (that loses currentColor). Each entry also carries a machine-readable <code>fill</code> boolean so codegen (and <span className="ic">&lt;Icon/&gt;</span>) can tell a filled glyph from a stroke one — today only <code>starFill</code> is <code>fill:true</code> (the solid ★ for an active favorite); its outline twin <code>star</code> stays stroke-only.</Note>
          <Dont><b>Don't</b> bake a color into a glyph or mix stroke widths. Filled glyphs are the rare exception (flagged <code>fill:true</code> in icons.json, e.g. <code>starFill</code>) — for a filled <i>state dot</i>, reach for <span className="ic">StatusDot</span> / <span className="ic">Badge</span> instead, not a new icon.</Dont>
        </Spec>
      </Section>

      {/* where the modifier glyphs actually land */}
      <Section id="keys-in-use" title="Modifier symbols in use">
        <Spec id="keycap-chip-settings-display-style" title="Keycap chip · settings display style"
          when={<>Two places consume these glyphs. In a <b>keycap chip</b> (modifier-hint header, switch overlays) the glyph replaces the key's text inside the same <span className="ic">Kbd</span> cap — <b>14px</b>, so its optical weight matches the 12px mono label it stands in for. In the <b>Settings › modifier display style</b> dropdown, the closed trigger shows the <b>glyph alone</b> (it is a preview of the keycap), while each open option row pairs <b>glyph + text label</b> so the choice is never ambiguous.</>}>
          <Stage style={{ gap: 30 }}>
            <div className="cluster">
              <span className="lbl">keycap chip — text style vs. symbol style</span>
              <div className="row" style={{ gap: 20, alignItems: "center" }}>
                <Kbd keys={["Cmd", "Option"]} />
                <span style={{ color: "var(--tasty-text-muted)", fontSize: 11 }}>→</span>
                <Kbd keys={[<Icon name="cmdKey" size={14} />, <Icon name="optionKey" size={14} />]} />
                <Kbd keys={[<Icon name="shiftKey" size={14} />, "4"]} />
              </div>
            </div>
            <div className="cluster">
              <span className="lbl">settings dropdown — trigger (symbol style) + open option list</span>
              <div className="row" style={{ gap: 24, alignItems: "flex-start" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  {[["Cmd key display:", "cmdKey"], ["Option key display:", "optionKey"], ["Shift key display:", "shiftKey"]].map(([label, glyph]) => (
                    <div key={glyph} style={{ display: "flex", alignItems: "center", gap: 12, minHeight: "var(--tasty-settings-row-min-height)" }}>
                      <span style={{ width: 128, flex: "none", fontSize: 13, color: "var(--tasty-text-secondary)" }}>{label}</span>
                      <span style={{ display: "inline-flex", alignItems: "center", gap: 8, height: "var(--tasty-control-height)", padding: "0 var(--tasty-space-sm)",
                        border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)",
                        background: "var(--tasty-surface-raised)", color: "var(--tasty-text-primary)" }}>
                        <Icon name={glyph} size={16} />
                        <Icon name="chevronDown" size={14} />
                      </span>
                    </div>
                  ))}
                </div>
                <div style={{ width: 168, padding: "var(--tasty-space-xs)", border: "var(--tasty-border-width) solid var(--tasty-border-default)",
                  borderRadius: "var(--tasty-radius)", background: "var(--tasty-menu-bg, var(--tasty-surface-raised))", boxShadow: "var(--tasty-shadow-popover)",
                  display: "flex", flexDirection: "column", gap: 1 }}>
                  {[["Text — \u201cOption\u201d", null, false], ["Symbol", "optionKey", true]].map(([label, glyph, active]) => (
                    <div key={label} style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-menu-item-height)", padding: "0 var(--tasty-space-sm)",
                      borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                      background: "transparent",
                      color: active ? "var(--tasty-menu-item-selected-fg)" : "var(--tasty-menu-item-fg)" }}>
                      <span style={{ width: 16, display: "inline-flex", justifyContent: "center" }}>{glyph ? <Icon name={glyph} size={16} /> : null}</span>
                      <span style={{ flex: 1 }}>{label}</span>
                      {active && <span style={{ display: "inline-flex", color: "var(--tasty-menu-item-check-fg)" }}><Icon name="check" size={14} /></span>}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[
              ["names", <><span className="tok">cmdKey</span> · <span className="tok">optionKey</span> · <span className="tok">shiftKey</span></>],
              ["keycap chip size", "14px (matches the 12px mono keycap label)"],
              ["dropdown size", "16px — Icon md default"],
              ["closed trigger", "glyph only"],
              ["option row", "glyph + text label + check on the active row"],
              ["color", <>inherits <span className="tok">currentColor</span> from the cap / row (text-secondary at rest)</>],
            ]}
            tokens={[
              { tok: "--tasty-text-secondary", use: "keycap glyph", color: "var(--tasty-text-secondary)" },
              { tok: "--tasty-surface-raised", use: "cap / trigger fill", color: "var(--tasty-surface-raised)" },
              { tok: "--tasty-border-strong", use: "cap border", color: "var(--tasty-border-strong)" },
              { tok: "--tasty-overlay-hover", use: "active option row", color: "var(--tasty-overlay-hover)" },
            ]} />
          <Note>No new tokens — the glyphs drop into the existing <span className="ic">Kbd</span> cap and menu-row recipes untouched. The <code>+</code> joiner between caps stays <b>text</b>; only the key faces become vectors.</Note>
          <Dont><b>Don't</b> type ⌘ / ⌥ / ⇧ as unicode text anywhere in the product — that is the bug these glyphs exist to fix. And don't reach for <code>command</code> when you mean the ⌘ keycap.</Dont>
        </Spec>
      </Section>

      {/* the catalogue, grouped by job — rendered straight from the registry */}
      {GROUPS.map((g) => (
        <Section key={g.id} id={g.id} title={g.title}>
          <Spec when={g.when}>
            <Stage variant="solo" style={{ padding: 0, overflow: "hidden" }}>
              <div className="icongrid">
                {g.icons.map((ic) => <Tile key={ic.name} {...ic} />)}
              </div>
            </Stage>
          </Spec>
        </Section>
      ))}
    </>
  );
}

export const page = {
  active: "icons",
  nav: NAV,
  head: {
    title: "Icons",
    intro: "Find an icon by its name or purpose. Hover over a tile to learn its role, and use the shared Icon component to keep icons consistent across the app.",
    
  },
  content: <Icons />,
};

export default function GalleryRoute() {
  return page.content;
}
