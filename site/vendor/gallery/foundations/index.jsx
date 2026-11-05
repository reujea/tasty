import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Icon as FIcon, Button, Tag, StatusDot, Input, Badge } from "../../components/index.js";
import { Section, Spec, Stage, Cluster, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Foundations. Tokens shown ONLY in the context that
// consumes them: each swatch/value carries a "used for" statement, and
// the spacing section shows real gaps/padding on the 4px grid, never
// abstract bars.





const NAV = [
  { id: "elevation", label: "Color — elevation" },
  { id: "floating", label: "Elevation — floating surfaces" },
  { id: "text", label: "Color — text" },
  { id: "accents", label: "Color — accent roles" },
  { id: "terminal", label: "Color — terminal / ANSI" },
  { id: "type", label: "Type" },
  { id: "spacing", label: "Spacing" },
  { id: "shape", label: "Radius · border · motion" },
  { id: "uiscale", label: "UI scale" },
  { id: "rolegaps", label: "Role gaps — settled" },
];

// ── a labelled surface tile (token + what uses it) ──
function Tile({ bg, name, role, dark }) {
  return (
    <div style={{ background: bg, border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)",
      padding: "11px 13px", minWidth: 168 }}>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)" }}>{name}</div>
      <div style={{ fontSize: 11.5, color: "var(--tasty-text-muted)", marginTop: 3 }}>{role}</div>
    </div>
  );
}

// ── the two floating-surface shadows, shown on the surfaces that own them ──
function FloatMenu() {
  return (
    <div style={{ width: 180, background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", padding: 4, boxShadow: "var(--tasty-shadow-popover)" }}>
      {["Split pane", "New tab", "Rename…"].map((l, i) => (
        <div key={l} style={{ height: 28, display: "flex", alignItems: "center", padding: "0 8px", fontSize: 13,
          borderRadius: "var(--tasty-radius-sm)", color: i === 0 ? "var(--tasty-menu-item-fg-hover)" : "var(--tasty-menu-item-fg)",
          background: i === 0 ? "var(--tasty-menu-item-bg-hover)" : "transparent" }}>{l}</div>
      ))}
    </div>
  );
}

function FloatModal() {
  return (
    <div style={{ position: "relative", width: 300, height: 148, background: "var(--tasty-scrim-bg)",
      borderRadius: "var(--tasty-radius)", display: "flex", alignItems: "center", justifyContent: "center" }}>
      <div style={{ width: 220, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
        borderRadius: "var(--tasty-radius)", overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" }}>
        <div style={{ padding: "12px 14px", borderBottom: "1px solid var(--tasty-separator)", fontSize: 13, fontWeight: 600 }}>Delete workspace</div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "8px 14px" }}>
          <Button variant="ghost" size="sm">Cancel</Button>
          <Button variant="danger" size="sm">Delete</Button>
        </div>
      </div>
    </div>
  );
}

function ElevationStack() {
  // a true nested composition: chrome → panel → card → row
  return (
    <div style={{ background: "var(--tasty-bg-app)", border: "1px solid var(--tasty-border-default)",
      borderRadius: "var(--tasty-radius)", padding: 14, width: 360 }}>
      <Lbl tok="--tasty-bg-app">window chrome / deepest</Lbl>
      <div style={{ background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)",
        borderRadius: "var(--tasty-radius)", padding: 12, marginTop: 8 }}>
        <Lbl tok="--tasty-bg-sidebar">sidebars, side panels</Lbl>
        <div style={{ background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-separator)",
          borderRadius: "var(--tasty-radius)", padding: 12, marginTop: 8 }}>
          <Lbl tok="--tasty-bg-panel">primary content panels</Lbl>
          <div style={{ background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)",
            borderRadius: "var(--tasty-radius)", padding: 8, marginTop: 8 }}>
            <Lbl tok="--tasty-surface-raised" raised>cards, inputs, menus</Lbl>
            <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 2 }}>
              <div style={{ background: "var(--tasty-surface-hover)", borderRadius: "var(--tasty-radius-sm)", padding: "5px 8px",
                fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-on-active)" }}>--tasty-surface-hover · hovered row</div>
              <div style={{ background: "var(--tasty-surface-active)", borderRadius: "var(--tasty-radius-sm)", padding: "5px 8px",
                fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-on-active)" }}>--tasty-surface-active · selected row</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
function Lbl({ tok, raised, children }) {
  return (
    <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)" }}>{tok}</span>
      <span style={{ fontSize: 11, color: raised ? "var(--tasty-text-muted-raised)" : "var(--tasty-text-muted)" }}>{children}</span>
    </div>
  );
}

// ── accent role row: name + live usage ──
function AccentRow({ tok, color, role, demo }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "16px 150px 1fr", alignItems: "center", gap: 13,
      padding: "9px 0", borderBottom: "1px solid var(--tasty-separator)" }}>
      <span style={{ width: 16, height: 16, borderRadius: "var(--tasty-radius-sm)", background: color,
        border: "1px solid var(--tasty-border-strong)" }} />
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)" }}>{tok}</span>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <span style={{ fontSize: 12.5, color: "var(--tasty-text-secondary)", width: 168, flex: "none" }}>{role}</span>
        <span style={{ display: "inline-flex", gap: 8, alignItems: "center" }}>{demo}</span>
      </div>
    </div>
  );
}

// ── spacing demo: real usage on the 4px grid ──
function SpaceUse({ token, px, usedFor, children }) {
  return (
    <div style={{ borderBottom: "1px solid var(--tasty-separator)", padding: "16px 0" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, marginBottom: 10 }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12.5, color: "var(--tasty-text-primary)" }}>{token}</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-accent-info-raised)" }}>{px}</span>
        <span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>used for {usedFor}</span>
      </div>
      <div style={{ position: "relative" }}>
        <div className="gridover" style={{ borderRadius: 2 }} />
        {children}
      </div>
    </div>
  );
}

function typeScaleRow(size, tok, weight, role, sample) {
  return (
    <div key={tok} style={{ display: "grid", gridTemplateColumns: "150px 1fr", alignItems: "baseline", gap: 16,
      padding: "9px 0", borderBottom: "1px solid var(--tasty-separator)" }}>
      <div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)" }}>{tok}</div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10.5, color: "var(--tasty-text-muted)", marginTop: 2 }}>{size} · {weight}</div>
      </div>
      <div>
        <div style={{ fontSize: size, fontWeight: weight, color: "var(--tasty-text-primary)", lineHeight: 1.3 }}>{sample}</div>
        <div style={{ fontSize: 11.5, color: "var(--tasty-text-muted)", marginTop: 3 }}>{role}</div>
      </div>
    </div>
  );
}

// ── terminal / ANSI palette ──
// The 16 SGR colors a terminal cell paints with, plus the four state-fill
// tokens. These are TERMINAL CONTENT colors, distinct from the UI accent roles.
const ANSI_NORMAL = [
  ["--tasty-ansi-black", "30"], ["--tasty-ansi-red", "31"], ["--tasty-ansi-green", "32"], ["--tasty-ansi-yellow", "33"],
  ["--tasty-ansi-blue", "34"], ["--tasty-ansi-magenta", "35"], ["--tasty-ansi-cyan", "36"], ["--tasty-ansi-white", "37"],
];
const ANSI_BRIGHT = [
  ["--tasty-ansi-bright-black", "90"], ["--tasty-ansi-bright-red", "91"], ["--tasty-ansi-bright-green", "92"], ["--tasty-ansi-bright-yellow", "93"],
  ["--tasty-ansi-bright-blue", "94"], ["--tasty-ansi-bright-magenta", "95"], ["--tasty-ansi-bright-cyan", "96"], ["--tasty-ansi-bright-white", "97"],
];

function AnsiRow({ tok, sgr }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "16px 1fr auto", alignItems: "center", gap: 10,
      padding: "7px 2px", borderBottom: "1px solid var(--tasty-separator)" }}>
      <span style={{ width: 16, height: 16, borderRadius: "var(--tasty-radius-sm)", background: `var(${tok})`,
        border: "1px solid var(--tasty-border-strong)" }} />
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)",
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{tok.replace("--tasty-", "")}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10.5, color: "var(--tasty-text-muted)" }}>SGR {sgr}</span>
    </div>
  );
}
function AnsiCol({ heading, rows }) {
  return (
    <div>
      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
        letterSpacing: ".06em", color: "var(--tasty-text-muted)", marginBottom: 4 }}>{heading}</div>
      {rows.map(([tok, sgr]) => <AnsiRow key={tok} tok={tok} sgr={sgr} />)}
    </div>
  );
}
// state fill rendered onto a faux terminal line
function Hi({ tok, children }) {
  return <span style={{ background: `var(${tok})`, color: tok.includes("match-active") ? "var(--tasty-search-match-active-fg)" : tok.includes("selection") ? "var(--tasty-text-on-active)" : "var(--tasty-text-primary)", borderRadius: 2, padding: "0 1px" }}>{children}</span>;
}
function SemRow({ tok, role }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "16px 1fr", alignItems: "center", gap: 10, padding: "6px 0",
      borderBottom: "1px solid var(--tasty-separator)" }}>
      <span style={{ width: 16, height: 16, borderRadius: "var(--tasty-radius-sm)", background: `var(${tok})`,
        border: "1px solid var(--tasty-border-strong)" }} />
      <div style={{ display: "flex", alignItems: "baseline", gap: 10, flexWrap: "wrap" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)" }}>{tok.replace("--tasty-", "")}</span>
        <span style={{ fontSize: 11.5, color: "var(--tasty-text-muted)" }}>{role}</span>
      </div>
    </div>
  );
}

function Foundations() {
  return (
    <>
      {/* ELEVATION */}
      <Section id="elevation" title="Color — elevation (surface ramp)">
        <Spec id="depth-reads-through-surface-tint-never-shadow" title="Depth reads through surface tint, never shadow"
          when={<>Tasty has <b>no shadow system</b> (modals aside). Pick the next step up the ramp whenever you nest a container. A card on a panel is <b>--tasty-surface-raised</b> on <b>--tasty-bg-panel</b> — one step, not a drop shadow.</>}>
          <Stage variant="solo center">
            <ElevationStack />
          </Stage>
          <Meta
            specs={[["ramp order", <>crust→mantle→base→surface0→1→2</>], ["border", <>1px <span className="tok">--tasty-border-default</span></>], ["shadow", "none (UI); modals only"]]}
            tokens={[
              { tok: "--tasty-bg-app", use: "window", color: "var(--tasty-bg-app)" },
              { tok: "--tasty-bg-sidebar", use: "rails", color: "var(--tasty-bg-sidebar)" },
              { tok: "--tasty-bg-panel", use: "content", color: "var(--tasty-bg-panel)" },
              { tok: "--tasty-surface-raised", use: "cards", color: "var(--tasty-surface-raised)" },
              { tok: "--tasty-surface-hover", use: "hover row", color: "var(--tasty-surface-hover)" },
              { tok: "--tasty-surface-active", use: "selected", color: "var(--tasty-surface-active)" },
            ]} />
          <Do><b>Do</b> tint up exactly one ramp step each time you nest a container.</Do>
          <Dont><b>Don't</b> add a <code>box-shadow</code> to cards or panels — only floating surfaces use shadow (next section).</Dont>
        </Spec>
      </Section>

      {/* FLOATING SURFACES — the two-shadow exception */}
      <Section id="floating" title="Elevation — floating surfaces (the two shadows)">
        <Spec id="two-shadows-and-the-rule-that-picks-one" title="Two shadows, and the rule that picks one"
          when={<>In-page depth is tint. A surface that <b>leaves the page</b> gets a shadow, and there are exactly two — no third value, no per-component shadow. The rule is the surface's relationship to the ground: <b>anchored and scrim-less</b> (it floats over live content it doesn't own) takes <b>--tasty-shadow-popover</b>; <b>centered and scrim-backed</b> (it owns the viewport) takes <b>--tasty-shadow-modal</b> — <i>and</i> the scrim. The scrim dims the ground but draws no edge, so on a dark theme a dark modal on dimmed dark ground loses its silhouette; the shadow is what carries the edge.</>}>
          <Stage variant="solo center" style={{ gap: 28, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 8, alignItems: "center" }}>
              <FloatMenu />
              <Lbl tok="--tasty-shadow-popover">0 6px 18px · 40% — anchored, scrim-less</Lbl>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8, alignItems: "center" }}>
              <FloatModal />
              <Lbl tok="--tasty-shadow-modal">0 20px 60px · 55% — centered, scrim-backed</Lbl>
            </div>
          </Stage>
          <Meta
            specs={[["popover surfaces", "search bar · tools menu · context menu · tooltip · autocomplete · multiselect menu · banner + its more-menu · modifier-hint · tutorial callout · switch-number overlay"],
              ["modal surfaces", "command palette · ports · remote · settings · plugins · preset editor · git viewer · clipboard viewer · DAG popup · centered dialogs"],
              ["not listed", "no shadow"],
              ["layers", "single — never stacked"],
              ["color", "pure black alpha only — no tinted shadow"],
              ["geometry", "integer px, non-negative spread, off the 4px grid on purpose"],
              ["themes", "one value for mocha and latte"],
              ["transition", "none — geometry is fixed; banner / modifier-hint fade by opacity only"]]}
            tokens={[{ tok: "--tasty-shadow-popover", use: "anchored overlays" }, { tok: "--tasty-shadow-modal", use: "scrim-backed surfaces" }, { tok: "--tasty-scrim-bg", use: "the dim behind a modal", color: "var(--tasty-scrim-bg)" }]} />
          <Do><b>Do</b> reference the token. Every floating surface in this system resolves to one of these two values — a hand-written <code>box-shadow</code> is how a third shadow gets into the product.</Do>
          <Dont><b>Don't</b> put a shadow on an anchored surface that already sits inside a shadowed one (a menu opened inside a modal), and don't stack layers to make a "bigger" lift.</Dont>
          <Note>One sanctioned exception lives outside this pair: <code>--tasty-titlebar-csd-shadow</code> (<code>0 18px 50px -8px</code>) is OS window-frame chrome, and its negative spread is functional — it keeps the falloff inside the 8px band that carries the resize edges.</Note>
        </Spec>
      </Section>

      {/* TEXT */}
      <Section id="text" title="Color — text">
        <Spec id="hierarchy-by-text-color-on-any-surface" title="Hierarchy by text color, on any surface"
          when={<>Four text tints carry hierarchy. <b>Headings</b> aren't bigger — they're <b>--tasty-text-primary</b> at weight 600 (see Type). Meta, captions, and IDs drop to <b>--tasty-text-muted</b>.</>}>
          <Stage variant="solo column" style={{ gap: 10 }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)" }}>--tasty-text-primary — headings, active labels, terminal foreground</div>
            <div style={{ fontSize: 13, color: "var(--tasty-text-secondary)" }}>--tasty-text-secondary — body labels, descriptions, settings rows</div>
            <div style={{ fontSize: 13, color: "var(--tasty-text-muted)" }}>--tasty-text-muted — meta, captions, hints, monospace IDs</div>
            <div data-ink-grade="none" data-ink-why="disabled" style={{ fontSize: 13, color: "var(--tasty-text-disabled)" }}>--tasty-text-disabled — disabled controls and rows</div>
            <Input placeholder="--tasty-text-placeholder — empty field hint" style={{ width: 320 }} />
          </Stage>
          <Meta tokens={[
            { tok: "--tasty-text-primary", use: "headings, active", color: "var(--tasty-text-primary)" },
            { tok: "--tasty-text-secondary", use: "body, labels", color: "var(--tasty-text-secondary)" },
            { tok: "--tasty-text-muted", use: "meta, IDs", color: "var(--tasty-text-muted)" },
            { tok: "--tasty-text-disabled", use: "disabled", color: "var(--tasty-text-disabled)" },
            { tok: "--tasty-text-placeholder", use: "empty inputs", color: "var(--tasty-text-placeholder)" },
          ]} />
        </Spec>
      </Section>

      {/* ACCENTS */}
      <Section id="accents" title="Color — accent roles">
        <Spec id="accents-map-to-roles-not-decoration" title="Accents map to roles, not decoration"
          when={<>Each accent owns one meaning. The standout rule: <b>mauve = AI agent</b>. Anything agent-driven is tinted mauve so user-vs-agent stays visible at a glance. Don't reach for an accent for looks — reach for the role.</>}>
          <Stage variant="solo column" style={{ gap: 0, padding: "8px 26px" }}>
            <AccentRow tok="--tasty-accent-primary" color="var(--tasty-accent-primary-raised)" role="primary actions, focus ring"
              demo={<Button variant="primary" size="sm">Save</Button>} />
            <AccentRow tok="--tasty-accent-info" color="var(--tasty-accent-info-raised)" role="informational, neutral status"
              demo={<Tag variant="accent" dot>info</Tag>} />
            <AccentRow tok="--tasty-accent-success" color="var(--tasty-accent-success-raised)" role="running / ok / success"
              demo={<StatusDot status="running" pulse label="running" />} />
            <AccentRow tok="--tasty-accent-warning" color="var(--tasty-accent-warning-raised)" role="warnings, validation"
              demo={<Tag variant="warning" dot>warning</Tag>} />
            <AccentRow tok="--tasty-accent-attention" color="var(--tasty-accent-attention-raised)" role="needs-attention notice (plugins, occupied) — peach, NOT yellow"
              demo={<Tag variant="attention" dot>attention</Tag>} />
            <AccentRow tok="--tasty-accent-danger" color="var(--tasty-accent-danger-raised)" role="destructive, errors"
              demo={<Button variant="danger" size="sm">Force detach</Button>} />
            <AccentRow tok="--tasty-accent-agent" color="var(--tasty-accent-agent-raised)" role="AI-agent surfaces & actions"
              demo={<><StatusDot status="agent" pulse /><Tag variant="agent">agent</Tag></>} />
          </Stage>
          <Meta
            specs={[["fill text", <span className="tok">--tasty-text-on-accent</span>], ["focus ring", <>2px <span className="tok">--tasty-accent-primary</span></>], ["agent rule", "mauve = always agent"]]}
            tokens={[
              { tok: "--tasty-accent-primary", use: "blue", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-accent-info", use: "sky", color: "var(--tasty-accent-info-raised)" },
              { tok: "--tasty-accent-success", use: "green", color: "var(--tasty-accent-success-raised)" },
              { tok: "--tasty-accent-warning", use: "yellow", color: "var(--tasty-accent-warning-raised)" },
              { tok: "--tasty-accent-attention", use: "peach", color: "var(--tasty-accent-attention-raised)" },
              { tok: "--tasty-accent-danger", use: "red", color: "var(--tasty-accent-danger-raised)" },
              { tok: "--tasty-accent-agent", use: "mauve", color: "var(--tasty-accent-agent-raised)" },
            ]} />
        </Spec>
      </Section>

      {/* TERMINAL / ANSI */}
      <Section id="terminal" title="Color — terminal / ANSI palette">
        <Spec id="the-colors-a-terminal-cell-paints-with-not-ui-chrome" title="The colors a terminal cell paints with — not UI chrome"
          when={<>These are the <b>terminal content</b> colors the renderer writes into cells, kept deliberately separate from the accent roles above. An app's <span className="ic">SGR 32</span> green is <b>--tasty-ansi-green</b> — it is <b>not</b> <span className="tok">--tasty-accent-success</span>, even though both are green. Never wire terminal output to a UI accent, or a theme tweak to one will silently repaint the other. The 16 ANSI colors derive from the same Catppuccin palette; the four state fills are what Tasty itself paints onto cells (selection, vi cursor, search).</>}>
          <Stage variant="solo column" style={{ gap: 18, padding: "10px 26px" }}>
            {/* ANSI 16 — normal + bright, paired by hue */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--tasty-space-xl)" }}>
              <AnsiCol heading="normal · SGR 30–37" rows={ANSI_NORMAL} />
              <AnsiCol heading="bright · SGR 90–97" rows={ANSI_BRIGHT} />
            </div>

            {/* terminal state fills, shown on a faux terminal line */}
            <div>
              <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
                letterSpacing: ".06em", color: "var(--tasty-text-muted)", marginBottom: 8 }}>terminal state fills — painted onto cells</div>
              <div style={{ background: "var(--tasty-surface-terminal-focused-bg)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)",
                padding: "12px 14px", fontFamily: "var(--tasty-font-mono)", fontSize: 13, lineHeight: 1.8,
                color: "var(--tasty-color-neutral-1100, #cdd6f4)" }}>
                <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-ansi-green)" }}>~/tasty</span> $ vi notes.md</div>
                <div>
                  The <Hi tok="--tasty-selection-bg">selected words</Hi> wrap the <Hi tok="--tasty-search-match-bg">match</Hi> and
                  the <Hi tok="--tasty-search-match-active-bg">active match</Hi> sit&nbsp;
                  <span style={{ background: "var(--tasty-vi-cursor-bg)", color: "#000", borderRadius: 1 }}>h</span>ere
                </div>
              </div>
              <div style={{ marginTop: 10 }}>
                <SemRow tok="--tasty-selection-bg" role="mouse / keyboard selection region" />
                <SemRow tok="--tasty-vi-cursor-bg" role="vi-mode block cursor" />
                <SemRow tok="--tasty-search-match-bg" role="search matches in scrollback" />
                <SemRow tok="--tasty-search-match-active-bg" role="the current / active match" />
                <SemRow tok="--tasty-search-match-active-fg" role="cell ink inside the active match (2026-10-10)" />
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["set", "16 ANSI (8 normal + 8 bright)"], ["source", "derived from the Catppuccin palette"], ["scope", "terminal cells — not UI chrome"], ["state fills", "selection · vi cursor · search"]]}
            tokens={[
              { tok: "--tasty-ansi-red", use: "SGR 31 red", color: "var(--tasty-ansi-red)" },
              { tok: "--tasty-ansi-green", use: "SGR 32 green", color: "var(--tasty-ansi-green)" },
              { tok: "--tasty-ansi-bright-blue", use: "SGR 94 br.blue", color: "var(--tasty-ansi-bright-blue)" },
              { tok: "--tasty-selection-bg", use: "selection fill", color: "var(--tasty-selection-bg)" },
              { tok: "--tasty-search-match-active-bg", use: "active match", color: "var(--tasty-search-match-active-bg)" },
            ]} />
          <Note><b>Active match ink (2026-10-10 b12).</b> Glyphs inside the <b>active</b> match are drawn in <span className="tok">--tasty-search-match-active-fg</span>, not the cell's own foreground: Mocha's light text on yellow 70% was 1.61 : 1. Mocha ink → neutral-200 (base) <b>6.95 : 1</b>. Latte keeps text-primary ink and drops the active fill to <b>50%</b> → <b>4.68 : 1</b> (70% was 3.94). Ordinary matches (30%) keep the cell's own colours: Mocha 4.75 · Latte 5.50. Cell colours set by the app (SGR) are overridden only inside the active match.</Note>
          <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([lab, th]) => (
              <div key={lab} {...(th ? { "data-theme": th } : {})} style={{ padding: "8px 12px", background: "var(--tasty-surface-terminal-focused-bg)", borderRadius: "var(--tasty-radius)", fontFamily: "var(--tasty-font-mono)", fontSize: 13, color: "var(--tasty-surface-terminal-focused-fg)" }}>
                {lab}: a <span style={{ background: "var(--tasty-search-match-bg)" }}>match</span> and the <span style={{ background: "var(--tasty-search-match-active-bg)", color: "var(--tasty-search-match-active-fg)" }}>match</span> under the cursor
              </div>
            ))}
          </div>
          <Dont><b>Don't</b> use an ANSI color for UI chrome or an accent for terminal output — <span className="ic">ansi-green</span> and <span className="ic">accent-success</span> are different roles that only happen to share a hue.</Dont>
        </Spec>
      </Section>

      {/* TYPE */}
      <Section id="type" title="Type">
        <Spec id="two-families-hard-14px-cap-hierarchy-by-weight" title="Two families, hard 14px cap, hierarchy by weight"
          when={<>UI chrome uses the <b>native system sans</b>, capped at 14px — body is 13px, captions 11px. Code and terminal use <b>D2Coding</b> (the brand mono, with ligatures). Headings are body-size at weight 600; never scale UI text up for emphasis.</>}>
          <Stage variant="solo column" style={{ gap: 0 }}>
            {typeScaleRow(13, "--tasty-font-size-heading", 600, "section heading — same size as body, weight 600", "New workspace")}
            {typeScaleRow(13, "--tasty-font-size-body", 400, "default UI text, labels, menu rows", "Held by another client. Force detach to take over.")}
            {typeScaleRow(11, "--tasty-font-size-caption", 400, "meta, badges, status-bar, hints", "bash · 80×24 · s_01HX")}
            <div style={{ display: "grid", gridTemplateColumns: "150px 1fr", alignItems: "baseline", gap: 16, padding: "12px 0 0" }}>
              <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-primary)" }}>--tasty-font-mono</div>
              <div>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 14, color: "var(--tasty-text-primary)" }}>D2Coding — git diff --stat  →  !=  ==&gt;  </div>
                <div style={{ fontSize: 11.5, color: "var(--tasty-text-muted)", marginTop: 3 }}>terminal cells, code, IDs, shortcuts — ligatures on</div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["UI cap", <span className="tok">--tasty-font-size-max</span>], ["heading", "body size, weight 600"], ["mono", <span className="tok">--tasty-font-mono</span>]]}
            tokens={[
              { tok: "--tasty-font-ui", use: "chrome (system sans)" },
              { tok: "--tasty-font-mono", use: "D2Coding — code/term" },
              { tok: "--tasty-font-size-body", use: "13px default" },
              { tok: "--tasty-font-size-caption", use: "11px meta" },
              { tok: "--tasty-font-weight-semibold", use: "600 = heading" },
            ]} />
          <Note>Two <b>sanctioned exceptions</b> to the 14px cap, both flagged in the tokens: <span className="tok">--tasty-font-size-prose-h1</span> (20px — rendered markdown <i>content</i>, not chrome; since the egui_commonmark adoption this is the <b>Heading anchor</b> at the top of the library's interpolated heading ladder, not a per-level size) and <span className="tok">--tasty-font-size-brand-wordmark</span> (17px — the mono <b>“tasty.”</b> sidebar wordmark, a <i>branding</i> mark). Nothing else in UI chrome exceeds 14px. Markdown <b>content</b> typography is otherwise <b>library-driven</b> (size ladder + leading fixed by egui_commonmark) — see the Markdown viewer specimen.</Note>
        </Spec>
      </Section>

      {/* SPACING — the one that mattered */}
      <Section id="spacing" title="Spacing — the 4px grid, in use">
        <Spec id="five-steps-each-with-a-job" title="Five steps, each with a job"
          when={<>The whole system snaps to a <b>4px grid</b> — there are only five spacing tokens. Below, each one is shown <b>doing its job</b>, not as an abstract bar. Turn on <b>Specs</b> to see the 4px rhythm under each example.</>}>
          <Stage variant="solo column" style={{ gap: 0, padding: "4px 26px 14px" }}>
            <SpaceUse token="--tasty-space-xs" px="4px" usedFor="icon↔label gaps, chip insets, tight inner padding">
              <div style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-xs)", background: "var(--tasty-surface-raised)",
                border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)" }}>
                <span style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--tasty-accent-success)" }} />
                <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)" }}>running</span>
              </div>
            </SpaceUse>
            <SpaceUse token="--tasty-space-sm" px="8px" usedFor="gaps between sibling controls, list-row padding">
              <div style={{ display: "inline-flex", gap: "var(--tasty-space-sm)" }}>
                <Button size="sm" variant="secondary">Cancel</Button>
                <Button size="sm" variant="primary">Save</Button>
              </div>
            </SpaceUse>
            <SpaceUse token="--tasty-space-md" px="12px" usedFor="card / panel padding, toolbar insets">
              <div style={{ background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)",
                borderRadius: "var(--tasty-radius)", padding: "var(--tasty-space-md)", fontSize: 12.5, color: "var(--tasty-text-secondary)", display: "inline-block" }}>
                Card padded with --tasty-space-md on all sides
              </div>
            </SpaceUse>
            <SpaceUse token="--tasty-space-lg" px="16px" usedFor="section gaps inside dialogs, form row rhythm">
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-lg)" }}>
                <span style={{ fontSize: 12.5, color: "var(--tasty-text-secondary)" }}>Setting group A</span>
                <span style={{ fontSize: 12.5, color: "var(--tasty-text-secondary)" }}>Setting group B</span>
              </div>
            </SpaceUse>
            <SpaceUse token="--tasty-space-xl" px="24px" usedFor="major gaps between distinct regions / columns">
              <div style={{ display: "flex", gap: "var(--tasty-space-xl)" }}>
                <div style={{ background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)", padding: "6px 12px", fontSize: 12, color: "var(--tasty-text-muted)" }}>region</div>
                <div style={{ background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)", padding: "6px 12px", fontSize: 12, color: "var(--tasty-text-muted)" }}>region</div>
              </div>
            </SpaceUse>
          </Stage>
          <Meta
            specs={[["grid", "4 / 8 / 12 / 16 / 24"], ["rule", "multiples of 4px only"], ["heights", "snap to grid too"]]}
            tokens={[
              { tok: "--tasty-space-xs", use: "4 · icon gaps" },
              { tok: "--tasty-space-sm", use: "8 · control gaps" },
              { tok: "--tasty-space-md", use: "12 · card pad" },
              { tok: "--tasty-space-lg", use: "16 · sections" },
              { tok: "--tasty-space-xl", use: "24 · regions" },
            ]} />
          <Dont><b>Don't</b> use values off the grid (6px, 10px, 14px). If something needs in-between, you're missing a layout, not a token.</Dont>
        </Spec>
      </Section>

      {/* RADIUS / BORDER / MOTION */}
      <Section id="shape" title="Radius · border · motion">
        <Spec id="crisp-and-rectilinear-it-s-a-terminal" title="Crisp and rectilinear — it's a terminal"
          when={<>Radius is <b>always 4px</b> (2px for tiny inner chips). Borders are <b>always 1px</b>. Motion is short UI feedback only — and <b>terminal content never animates (0ms, always)</b>.</>}>
          <Stage variant="solo" style={{ gap: 26 }}>
            <Cluster label="--tasty-radius · 4px (default)">
              <div style={{ width: 88, height: 48, background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }} />
            </Cluster>
            <Cluster label="--tasty-radius-sm · 2px (inner)">
              <div style={{ width: 88, height: 48, background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius-sm)" }} />
            </Cluster>
            <Cluster label="--tasty-radius-pill · chips/dots">
              <Badge variant="danger">99+</Badge>
            </Cluster>
            <Cluster label="fixed heights">
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-muted)", lineHeight: 1.7 }}>
                tree 22 · control 28<br />tab 24 ×150
              </span>
            </Cluster>
          </Stage>
          <Meta
            specs={[["radius", <>4px (<span className="tok">--tasty-radius</span>)</>], ["border", <>1px (<span className="tok">--tasty-border-width</span>)</>], ["UI motion", <>90–120ms <span className="tok">--tasty-ease-ui</span></>], ["terminal motion", "0ms — always"]]}
            tokens={[
              { tok: "--tasty-radius", use: "4px everywhere" },
              { tok: "--tasty-radius-sm", use: "2px inner" },
              { tok: "--tasty-motion-ui", use: "120ms feedback" },
              { tok: "--tasty-motion-term", use: "0ms — terminal" },
              { tok: "--tasty-focus-ring-width", use: "2px ring" },
            ]} />
        </Spec>
      </Section>

      {/* UI SCALE */}
      <Section id="uiscale" title="UI scale — sidebar zoom">
        <Spec id="one-multiplier-scales-the-sidebar-everything-else-stays-fixed" title="One multiplier scales the sidebar; everything else stays fixed"
          when={<>Sidebar scale is a <b>single multiplier</b> exposed in <span className="ic">Settings › Appearance › Display</span>. Three named stops feed one active value — components read <b>only</b> <span className="tok">--tasty-ui-scale</span>, never the stops directly. It zooms the <b>sidebar</b> alone (wordmark, workspace list, footer); the <b>title bar, tabs, panes, and dialogs are untouched</b>, and the terminal keeps its own glyph size (a separate font-size axis). Bigger nav targets without disturbing the work area.</>}>
          <Stage variant="solo" grid style={{ gap: 22, alignItems: "flex-end" }}>
            {[["sm", "0.8×", "Small"], ["md", "1.0×", "Medium"], ["lg", "1.2×", "Large"]].map(([key, mult, label]) => (
              <Cluster key={key} label={`${label} · ${mult}`}>
                <div style={{ zoom: `var(--tasty-ui-scale-${key})`, display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)",
                  background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", padding: "6px 9px" }}>
                  <StatusDot status="idle" />
                  <span style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>agents-prod</span>
                  <Badge variant="primary">2</Badge>
                </div>
              </Cluster>
            ))}
          </Stage>
          <Meta
            specs={[["stops", <>0.8 / 1.0 / 1.2 (<span className="tok">--tasty-ui-scale-sm/md/lg</span>)</>], ["active value", <span className="tok">--tasty-ui-scale</span>], ["consumed by", "sidebar root zoom"], ["excluded", "title bar, tabs, panes, dialogs"], ["control", "Appearance › Display"]]}
            tokens={[
              { tok: "--tasty-ui-scale-sm", use: "0.8 · compact" },
              { tok: "--tasty-ui-scale-md", use: "1.0 · default" },
              { tok: "--tasty-ui-scale-lg", use: "1.2 · roomy" },
              { tok: "--tasty-ui-scale", use: "active — the only one read" },
            ]} />
          <Dont><b>Don't</b> read a named stop (<span className="ic">--tasty-ui-scale-lg</span>) in a component, and <b>don't</b> add per-component size tokens (no "sidebar = medium"). One multiplier, read in one place, the 4px grid intact.</Dont>
          <Note>Implementation: the sidebar root sets <span className="ic">zoom: var(--tasty-ui-scale)</span> with <span className="ic">height: 100%</span> so it fills top-to-bottom at any scale. Nothing else reads <span className="tok">--tasty-ui-scale</span> — the title bar, tab strip, panes, and modals render at fixed size, so terminal columns and tab hit-targets never shift.</Note>
        </Spec>
      </Section>

      <Section id="rolegaps" title="Role gaps — settled">
        <Spec id="colors-that-had-no-role-of-their-own" title="Colors that had no role of their own"
          when={<>Seven places were reading a role whose <b>value</b> matched but whose <b>meaning</b> didn't. Four get a name, three move to the role they should always have used. New roles preserve the pixel; moves change it, and each is called out.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 10, alignItems: "stretch" }}>
            {[["C1", "pane divider · GPU-inactive surface border · popup frame", "surface-active (a fill)", "--tasty-border-frame", "new role · popup frame MOVES", "var(--tasty-border-frame)"],
              ["C2", "sidebar dim chevron · dim icons", "text-placeholder (unentered text)", "--tasty-glyph-dim", "new role · same pixel", "var(--tasty-glyph-dim)"],
              ["C3", "disabled text in ports / convert / remote", "text-placeholder", "--tasty-text-disabled", "MOVES — disabled is its own ink", "var(--tasty-text-disabled)"],
              ["C4", "tab-strip scroll arrow, disabled", "border-strong (a border)", "--tasty-text-disabled", "MOVES — same rule as C3", "var(--tasty-text-disabled)"],
              ["C5", "status-bar theme indicator", "accent-warning / accent-agent", "--tasty-statusbar-theme-glyph", "MOVES — a glyph, no colour role", "var(--tasty-statusbar-theme-glyph)"],
              ["C6", "StatusDot idle", "text-muted", "--tasty-status-dot-idle", "MOVES — the canonical idle tone", "var(--tasty-status-dot-idle)"],
              ["C7", "Plugins window header glyph", "accent-attention (a state)", "--tasty-accent-decorative", "new role · same pixel", "var(--tasty-accent-decorative)"]].map(([id, place, was, now, verdict, color]) => (
              <div key={id} style={{ display: "flex", alignItems: "center", gap: 12, padding: "8px 10px", background: "var(--tasty-bg-panel)",
                border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ flex: "none", width: 26, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{id}</span>
                <span style={{ flex: 1, minWidth: 0, fontSize: 12, color: "var(--tasty-text-secondary)" }}>{place}</span>
                <span style={{ flex: "none", width: 190, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{was}</span>
                <span style={{ flex: "none", width: 20, height: 20, borderRadius: "var(--tasty-radius-sm)", background: color, border: "1px solid var(--tasty-border-default)" }} />
                <span style={{ flex: "none", width: 210, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-primary)" }}>{now}</span>
                <span style={{ flex: "none", width: 200, fontSize: 11, color: /MOVES/.test(verdict) ? "var(--tasty-accent-warning-raised)" : "var(--tasty-text-muted)" }}>{verdict}</span>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["new semantic roles", "border-frame · glyph-dim · accent-decorative"], ["new component role", "statusbar-theme-glyph (→ glyph-dim)"], ["moves (pixel changes)", "C3 · C4 · C5 · C6"], ["Latte", "all four new roles track the ramp — no per-theme remap"], ["contrast", "C3 / C4 are disabled ink — exempt from 4.5:1; C2 is chrome, not text"]]}
            tokens={[{ tok: "--tasty-border-frame", use: "divider / frame edge", color: "var(--tasty-border-frame)" }, { tok: "--tasty-glyph-dim", use: "dim chrome glyphs", color: "var(--tasty-glyph-dim)" }, { tok: "--tasty-accent-decorative", use: "header ornament", color: "var(--tasty-accent-decorative)" }, { tok: "--tasty-text-disabled", use: "every disabled label / glyph", color: "var(--tasty-text-disabled)" }]} />
          <Note>The three value-preserving holdouts elsewhere (tab hover fill, tab separator, toast border) are confirmed as <b>intended visual changes</b> — move them to canonical; they were never design questions.</Note>
        </Spec>

        <Spec id="c1-correction-the-popup-frame-does-move-and-should-settled" title="C1 correction — the popup frame does move, and should (settled)"
          when={<>C1 was transcribed as “pixel-unchanged”. That holds for two of its three places: the <b>pane divider</b> and the <b>GPU-inactive surface border</b> came from <span className="tok">--tasty-surface-active</span>, the same ramp step. The <b>popup frame</b> came from <span className="tok">--tasty-border-strong</span> one step below, so it moved — neutral-400 → neutral-500. <b>The move is the correct outcome, not a regression.</b> A frame is the edge between a floating surface and everything behind it and needs more separation than a line <i>inside</i> that surface; the popup's internal dividers stay on <span className="tok">--tasty-border-strong</span> exactly one step below, so frame and content now read in the right order. The step is chosen for contrast, not for lightness: in Mocha neutral-500 is brighter than the panel, in Latte it is darker. Both gain.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})}
                style={{ display: "flex", flexDirection: "column", gap: 8, padding: 14, background: "var(--tasty-bg-app)",
                  border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ display: "flex", gap: 14, alignItems: "flex-start" }}>
                  {[["settled — border-frame", "var(--tasty-border-frame)", true], ["rejected — border-strong", "var(--tasty-border-strong)", false]].map(([cap, edge, on]) => (
                    <div key={cap} style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                      <div style={{ fontSize: 11, color: on ? "var(--tasty-accent-success-raised)" : "var(--tasty-text-muted)" }}>{cap}</div>
                      <div style={{ width: 216, background: "var(--tasty-bg-panel)", border: "1px solid " + edge, borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                        <div style={{ height: 28, display: "flex", alignItems: "center", padding: "0 10px", fontSize: 12,
                          color: "var(--tasty-text-secondary)", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid " + edge }}>Listening ports</div>
                        <div style={{ padding: "10px 10px 0", fontSize: 12, color: "var(--tasty-text-primary)" }}>Show all (system-wide)</div>
                        <div style={{ height: 1, background: "var(--tasty-border-strong)", margin: "10px 0 0" }} />
                        <div style={{ padding: "8px 10px 12px", fontSize: 11, color: "var(--tasty-text-muted)" }}>Favorites</div>
                      </div>
                    </div>
                  ))}
                  <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                    <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>appearance cards — inactive → border-strong</div>
                    <div style={{ display: "flex", gap: 8, padding: 10, width: 150, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                      <div style={{ flex: 1, height: 64, borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", border: "2px solid var(--tasty-accent-primary)" }} />
                      <div style={{ flex: 1, height: 64, borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-strong)" }} />
                    </div>
                  </div>
                  <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                    <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>pane divider — unchanged</div>
                    <div style={{ display: "flex", height: 104, width: 150, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                      <div style={{ flex: 1, background: "var(--tasty-bg-app)" }} />
                      <div style={{ width: 1, background: "var(--tasty-border-frame)" }} />
                      <div style={{ flex: 1, background: "var(--tasty-bg-app)" }} />
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["popup frame", <span className="tok">--tasty-border-frame</span>], ["popup titlebar underline", <>the same — it is part of the frame</>], ["popup inner dividers", <>unchanged — <span className="tok">--tasty-border-strong</span>, one step below</>], ["pane divider", "border-frame — pixel unchanged"], ["GPU-inactive surface border", "border-frame — pixel unchanged"], ["Mocha frame vs panel", "1.80:1 → 2.46:1"], ["Latte frame vs panel", "1.60:1 → 1.91:1"], ["4.5:1", "not applicable — a 1px structural edge is not text; the move raises separation in both themes"], ["transcription fix", "C1 reads “new role; the popup frame moves one step up”, not “pixel-unchanged”"], ["fifth call site", <>Settings › Appearance <b>inactive card border</b> is NOT a frame — it is a line inside a panel → <span className="tok">--tasty-border-strong</span> (neutral-500 → 400, pixel changes). The frame list stays at <b>four</b>.</>]]}
            tokens={[{ tok: "--tasty-border-frame", use: "popup frame · titlebar underline · pane divider · GPU-inactive surface", color: "var(--tasty-border-frame)" }, { tok: "--tasty-border-strong", use: "dividers INSIDE a surface", color: "var(--tasty-border-strong)" }]} />
          <Dont><b>Don't</b> mint a popup-only border role to put the old value back. Three places, one meaning — the edge that bounds a frame — and a fourth role would only record that one of them used to be wrong.</Dont>
          <Note><b>Frame vs partition, stated once.</b> <span className="tok">--tasty-border-frame</span> bounds a surface against what is <i>behind</i> it: popup frame · popup titlebar underline · pane divider · GPU-inactive surface border. Anything drawn <i>inside</i> a panel that separates siblings — dividers, card outlines, section rules — is a partition and reads <span className="tok">--tasty-border-strong</span> (or <span className="tok">--tasty-border-default</span> when quieter). The appearance-tab card border arrived on <code>border_frame()</code> only by a value-preserving port from <code>surface_active()</code>; it moves down one step so the outer-edge &gt; inner-line order holds inside Settings too. The active card keeps its accent edge.</Note>
        </Spec>

        <Spec id="half-pixel-type-sizes-snap-to-the-scale" title="Half-pixel type sizes snap to the scale"
          when={<>A <code>.5</code> font size can never equal a token: UI sizes pass through <code>round()</code> at every zoom. All of them snap, by one rule — <b>text you read snaps up, numeric micro-labels snap down</b>.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 10, alignItems: "stretch" }}>
            {[["T1", "Plugins segment label / badge / count", "12.5 / 9.5 / 10.5", "12 / 10 / 10"],
              ["T2", "Plugins Attention title / reason / fingerprint / label", "13.5 / 12.5 / 11.5 / 10.5", "13 / 12 / 11 / 10"],
              ["T3", "sidebar notification badge number", "9.5", "10 (micro)"],
              ["T4", "command palette hint", "10.5", "11 (caption — it is read, not counted)"],
              ["T5", "first-run brand title / warning", "30 / 12.5", "30 kept as --tasty-font-size-brand-display / 12"],
              ["T6", "clipboard image glyph", "30", "28 — icon family, existing exception"]].map(([id, place, was, now]) => (
              <div key={id} style={{ display: "flex", alignItems: "baseline", gap: 12, padding: "8px 10px", background: "var(--tasty-bg-panel)",
                border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ flex: "none", width: 26, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{id}</span>
                <span style={{ flex: 1, minWidth: 0, fontSize: 12, color: "var(--tasty-text-secondary)" }}>{place}</span>
                <span style={{ flex: "none", width: 200, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{was}</span>
                <span style={{ flex: "none", width: 330, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-primary)" }}>{now}</span>
              </div>
            ))}
            <div style={{ display: "flex", gap: 24, paddingTop: 6, flexWrap: "wrap" }}>
              {[0.85, 1, 1.2].map((z) => (
                <div key={z} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>ui_scale {z}</span>
                  {[["13", 13], ["12", 12], ["11", 11], ["10", 10]].map(([label, s]) => (
                    <span key={label} style={{ fontSize: Math.round(s * z), color: "var(--tasty-text-secondary)" }}>Needs attention — {Math.round(s * z)}px</span>
                  ))}
                </div>
              ))}
            </div>
          </Stage>
          <Meta
            specs={[["rule", "read text → snap up · numeric micro-label → snap down"], ["scale used", "13 body · 12 term-sm · 11 caption · 10 micro"], ["new primitive", "font-size-30 (branding exception, named)"], ["glyphs", "judged on the icon family, not the type scale"], ["zoom", "integers at 0.85 / 1 / 1.2 — no .5 survives round()"]]}
            tokens={[{ tok: "--tasty-font-size-brand-display", use: "first-run brand title (30)" }, { tok: "--tasty-font-size-caption", use: "11 — hints" }, { tok: "--tasty-font-size-micro", use: "10 — badge counts" }]} />
        </Spec>

        <Spec id="one-tinted-box-recipe-12-fill-36-edge" title="One tinted-box recipe: 12% fill, 36% edge"
          when={<>The same “accent as a soft box” idiom had drifted into four coefficient pairs (0.14/0.45, 0.12/0.35, 0.11/0.36, 0.12/none). They converge on <b>one public pair</b> — <span className="tok">--tasty-tint-fill-alpha</span> 0.12 and <span className="tok">--tasty-tint-border-alpha</span> 0.36 — with two sanctioned partial uses: <b>fill only</b> (warning callout) and <b>border only</b> (the remote chip tag). Every accent uses the same two numbers, so a tinted box is recognisable across the app regardless of hue.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "flex-start" }}>
            {[["fill + border — the default", true, true], ["fill only — warning callout", true, false], ["border only — remote chip", false, true]].map(([label, fill, border]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</span>
                <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
                  {[["info", "var(--tasty-accent-info)"], ["warning", "var(--tasty-accent-warning)"], ["danger", "var(--tasty-accent-danger)"], ["success", "var(--tasty-accent-success)"]].map(([name, col]) => (
                    <span key={name} style={{ display: "inline-flex", alignItems: "center", height: 24, padding: "0 10px", borderRadius: "var(--tasty-radius)",
                      fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: col.replace(")", "-strong)"),
                      background: fill ? `color-mix(in srgb, ${col} calc(var(--tasty-tint-fill-alpha) * 100%), transparent)` : "transparent",
                      border: border ? `1px solid color-mix(in srgb, ${col} calc(var(--tasty-tint-border-alpha) * 100%), transparent)` : "1px solid transparent" }}>{name}</span>
                  ))}
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["fill", <>0.12 — <span className="tok">--tasty-tint-fill-alpha</span></>], ["edge", <>0.36 — <span className="tok">--tasty-tint-border-alpha</span></>], ["retired", "0.14/0.45 · 0.12/0.35 · 0.11/0.36"], ["partial uses", "fill-only · border-only (same coefficients)"], ["affected", "file-picker info badge · keybinding IE notice & migrate card · Plugins error box / Installed badge / Attention banner · warning callout · remote & script badges · chip remote tag"], ["clipboard inset (I1)", "14 → 12 (--tasty-space-md) — the 4px grid wins; no 14 semantic is opened"]]}
            tokens={[{ tok: "--tasty-tint-fill-alpha", use: "every tinted fill" }, { tok: "--tasty-tint-border-alpha", use: "every tinted edge" }, { tok: "--tasty-space-md", use: "clipboard header / type-bar / footer inset" }]} />
          <Note>Opacity coefficients are <b>primitives</b> (<span className="tok">--tasty-opacity-tint-fill</span> / <span className="tok">-border</span>) with semantic aliases, so a per-role override stays possible later without re-scattering literals.</Note>
        </Spec>
        <Spec id="disabled-ink-no-contrast-target-latte-one-step-up" title="Disabled ink — no contrast target, Latte one step up"
          when={<>WCAG 1.4.3 and 1.4.11 exempt disabled controls, and a 4.5:1 target would collapse the hierarchy: in Latte the first ramp step that clears 4.5:1 on the panel is neutral-900, one step under <span className="tok">--tasty-text-muted</span> (neutral-1000). So disabled gets <b>no contrast target</b>. Two rules replace it. <b>Order:</b> disabled sits strictly below <span className="tok">--tasty-text-muted</span> and above <span className="tok">--tasty-text-placeholder</span> on the ramp, and the ink step is the only thing that marks disabled (no extra opacity on the label). <b>Parity:</b> the Latte ramp is compressed at its light end, so <span className="tok">--tasty-text-disabled</span> remaps to <b>neutral-800</b> in Latte (Mocha stays neutral-700). Labels and glyphs still share the one ink. The <b>enabled</b> tab-strip arrow is an active control glyph and moves to <span className="tok">--tasty-text-muted</span>, which clears 3:1 in both themes. Ratios below are flat colour on the control's own ground, and flat colour is the basis of the 3:1 (2026-09-29); rendered AA glyphs measure lower and are not part of the target. Disabled controls take this ink with <b>no opacity</b> (Components › Buttons › Disabled). Arrow shape: Layouts › Pane tab strip.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha", { c3: "3.40", c4: "3.40", en: "5.65" }, { c3: "3.40 (n700, unchanged)", c4: "3.40", en: "4.45 (n800)" }], ["Latte", "latte", { c3: "2.56", c4: "2.56", en: "4.05" }, { c3: "2.07 (n700)", c4: "2.07", en: "2.56 (n800)" }]].map(([label, attr, now, was]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>C3 · port scanner footer (ink example; the footer stays Close + Copy address) — disabled {now.c3}:1 (was {was.c3})</span>
                  <div style={{ display: "flex", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
                    {[["Close", false, true], ["Copy address", true, false]].map(([l, dis, box]) => (
                      <span key={l} data-ink-grade={dis ? "none" : undefined} data-ink-why={dis ? "disabled" : undefined} style={{ height: "var(--tasty-control-height)", display: "inline-flex", alignItems: "center", padding: "0 var(--tasty-space-md)", borderRadius: "var(--tasty-radius)", background: box ? "var(--tasty-surface-raised)" : "transparent", border: "var(--tasty-border-width) solid " + (box ? "var(--tasty-border-default)" : "transparent"), fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-medium)", color: dis ? "var(--tasty-text-disabled)" : "var(--tasty-text-primary)" }}>{l}</span>
                    ))}
                  </div>
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                  <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>C4 · tab-strip scroll — disabled {now.c4}:1 · enabled {now.en}:1 (was {was.en})</span>
                  <div style={{ display: "flex", alignItems: "stretch", height: "var(--tasty-control-height-tab)", width: "var(--tasty-size-288)", background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius-sm)", overflow: "hidden" }}>
                    <span data-ink-grade="none" data-ink-why="disabled" style={{ width: "var(--tasty-tab-scroll-arrow-width)", display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-tab-scroll-arrow-fg-disabled)" }}><FIcon name="chevronLeft" size="var(--tasty-tab-scroll-arrow-glyph-size)" /></span>
                    <span style={{ flex: 1, display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>server · dev · vim · logs</span>
                    <span style={{ width: "var(--tasty-tab-scroll-arrow-width)", display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-tab-scroll-arrow-fg)" }}><FIcon name="chevronRight" size="var(--tasty-tab-scroll-arrow-glyph-size)" /></span>
                  </div>
                </div>
                <div style={{ display: "flex", gap: "var(--tasty-space-xs)", alignItems: "center" }}>
                  {[["placeholder", "var(--tasty-text-placeholder)"], ["disabled", "var(--tasty-text-disabled)"], ["muted", "var(--tasty-text-muted)"], ["secondary", "var(--tasty-text-secondary)"], ["primary", "var(--tasty-text-primary)"]].map(([n, v]) => (
                    <span key={n} data-ink-grade={n === "disabled" ? "none" : undefined} data-ink-why={n === "disabled" ? "disabled" : undefined} style={{ padding: "var(--tasty-space-xs) var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius-sm)", fontSize: "var(--tasty-font-size-caption)", color: v }}>{n}</span>
                  ))}
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["A · target", "none — WCAG exempts disabled"], ["rule 1 · order", "placeholder < disabled < muted"], ["rule 2 · parity", "Latte remap n700 → n800"], ["B · ground", "n/a (no target) — report on the control's own ground"], ["C · enabled arrow", "in scope → text-muted (3:1 non-text)"], ["one ink", "labels + glyphs, unchanged principle"], ["pixels", "Latte disabled everywhere · both themes' enabled arrow"]]}
            tokens={[{ tok: "--tasty-text-disabled", use: "Mocha n700 · Latte n800", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-tab-scroll-arrow-fg", use: "→ text-muted", color: "var(--tasty-tab-scroll-arrow-fg)" }, { tok: "--tasty-tab-scroll-arrow-fg-disabled", use: "→ text-disabled", color: "var(--tasty-tab-scroll-arrow-fg-disabled)" }]} />
          <Dont><b>Don't</b> lift disabled to 4.5:1. In Latte that lands one step under text-muted and disabled stops reading as disabled. The hierarchy is the requirement; contrast is reported, not targeted.</Dont>
        </Spec>
        <Spec id="structural-dimensions-the-leftovers" title="Structural dimensions — the leftovers"
          when={<>Four items with no design answer, settled. <b>A · B</b>: the centre glyph 22 and the scripts empty glyph 26 become one <b>CenterState</b> part on a new icon tier, <span className="tok">--tasty-icon-size-lg</span> = 24 (Components › CenterState). <b>C</b>: no nominal block height; the block centres in its list region. <b>D</b>: a 32px in-surface toolbar is a container role, so it gets <span className="tok">--tasty-toolbar-height</span> and <span className="tok">--tasty-git-toolbar-height</span> re-points to it (value unchanged). Scroll max-height 200 stays a <b>screen-only dimension, no DTCG token</b>: it is the <b>tutorial topic popup's topic-list cap</b> (Overlays › Tutorial). It bounds rows that scale, so it <b>scales with ui_scale</b>; a hand-written Theme accessor (not generated from DTCG) is the right holder (2026-09-29). Column min-width 200 is <b>withdrawn</b>: it was an example from the request, and the only such site (port scanner Process column) already has <span className="tok">--tasty-port-process-col-min-width</span> (D7). Borrowing <span className="tok">--tasty-field-width-lg</span> or <span className="tok">--tasty-settings-sidebar-width</span> stays a false coupling. <b>E</b>: popup default sizes stay <b>outside the token system</b>. They are per-popup screen values; a size tier would imply they are interchangeable. Values design owns are fixed below, snapped to the 4px grid.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }}>
            <table style={{ borderCollapse: "collapse", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>
              <thead><tr>{["popup", "code today", "canonical W × H", "note"].map((h) => <th key={h} style={{ textAlign: "left", padding: "var(--tasty-space-xs) var(--tasty-space-md)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-muted)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-strong)" }}>{h}</th>)}</tr></thead>
              <tbody>
                {[["notifications","350 × 400","352 × 400","W snapped to grid"],["script changed confirm","360 × 150","360 × 152","H snapped to grid"],["search bar","360 × 28","360 × 28",""],["info modal","440 × 160","440 × 140..360","named: --tasty-info-modal-width / -max-height"],["approval","480 × 240","480 × 240",""],["file picker","640 × 480","640 × 480",""],["port scanner","660 × 520","660 × 520",""],["command palette","540 × 412","540 × 412",""],["DAG list","560 × 460","560 × 460","already --tasty-dag-popup-width / -height"],["remote tool","520 × 460","520 × 460",""],["remote attach","680 × 460","680 × 460",""],["transfer progress","400 × 180","400 × 180",""],["transfer error","400 × 200","400 × 200",""],["preset apply ×3","360 × 320","360 × 320",""]].map(([n, a, b, note]) => (
                  <tr key={n}>
                    <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>{n}</td>
                    <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-md)", fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-muted)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>{a}</td>
                    <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-md)", fontFamily: "var(--tasty-font-mono)", color: a === b ? "var(--tasty-text-primary)" : "var(--tasty-accent-warning-raised)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>{b}</td>
                    <td style={{ padding: "var(--tasty-space-xs) var(--tasty-space-md)", color: "var(--tasty-text-muted)", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>{note}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Stage>
          <Meta
            specs={[["A centre glyph", "22 → 24 · --tasty-center-state-glyph-size"], ["B scripts glyph", "26 → 24 · merged with A"], ["C block height", "100 / 120 → none (centres in region)"], ["D toolbar 32", "→ --tasty-toolbar-height (new semantic)"], ["D scroll max 200", "tutorial topic list · no DTCG token · hand-written Theme accessor · scales"], ["D col min 200", "withdrawn — = port Process col (D7)"], ["E popup sizes", "outside tokens · 350 → 352 · 150 → 152"], ["ui_scale", "A–D scale (incl. the tutorial cap) · E popup sizes do not"]]}
            tokens={[{ tok: "--tasty-icon-size-lg", use: "→ size-24 (new tier)" }, { tok: "--tasty-toolbar-height", use: "→ size-32 (new)" }, { tok: "--tasty-git-toolbar-height", use: "→ toolbar-height" }]} />
          <Note>Moving A · B · D onto tokens puts them on ui_scale: at 0.85 / 1.2 the pixels change (glyph 20.4 / 28.8). That is intended — they are UI chrome, not screen frames.</Note>
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "foundations",
  nav: NAV,
  head: {
    title: "Foundations",
    intro: "See how Tasty's colours, spacing, and typography work in context. Each example shows where a token is used and what it controls.",
    
  },
  content: <Foundations />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
