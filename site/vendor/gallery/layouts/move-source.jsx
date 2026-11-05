import React from "react";
import { Icon, StatusDot } from "../../components/index.js";
import { Spec, Stage, Meta, Note, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Layouts › Move source highlight.
// The pending "Move Surface / Tab / Pane" target. Mounted by layouts/index.jsx as
// <GalleryMoveSource />. Tokens: --tasty-move-source-* (components.css).




const RW = "var(--tasty-move-source-ring-width)";
// Ring = dashed stroke fully INSIDE the rect (stroke centred at rw/2). Painted last.
function MoveRing() {
  return (
    <svg aria-hidden style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none", zIndex: 3 }}>
      <rect x="1" y="1" style={{ x: `calc(${RW} / 2)`, y: `calc(${RW} / 2)`, width: `calc(100% - ${RW})`, height: `calc(100% - ${RW})`,
        fill: "none", stroke: "var(--tasty-move-source-ring)", strokeWidth: RW,
        strokeDasharray: "var(--tasty-move-source-dash) var(--tasty-move-source-dash-gap)" }} />
    </svg>
  );
}
const MoveGlyph = () => (
  <span aria-label="Move source inside" style={{ display: "inline-flex", flex: "none", color: "var(--tasty-move-source-glyph)" }}>
    <Icon name="move" size="var(--tasty-move-source-glyph-size)" />
  </span>
);
const cap = (t) => <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)", marginBottom: 6 }}>{t}</div>;

function TabCell({ label, active, hover, ring, cue }) {
  const fg = active ? "var(--tasty-tab-fg-active)" : hover ? "var(--tasty-tab-fg-hover)" : "var(--tasty-tab-fg)";
  return (
    <div style={{ position: "relative", width: "var(--tasty-tab-width)", height: "var(--tasty-tab-height)", flex: "none", display: "flex",
      alignItems: "center", gap: "var(--tasty-tab-gap)", padding: "0 var(--tasty-tab-padding-x)", color: fg,
      background: active ? "var(--tasty-tab-bg-active)" : hover ? "var(--tasty-surface-hover)" : "var(--tasty-tab-bg)",
      borderRight: "var(--tasty-border-width) solid var(--tasty-tab-separator)" }}>
      {active && <span style={{ position: "absolute", top: 0, left: 0, right: 0, height: "var(--tasty-tab-indicator-width)", background: "var(--tasty-tab-indicator)" }} />}
      <Icon name="terminal" size="var(--tasty-tab-icon-size)" />
      <span style={{ flex: 1, minWidth: 0, fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      {cue && <MoveGlyph />}
      {ring && <MoveRing />}
    </div>
  );
}

// edge: null | "needs-input" | "completion" | "hard" | "soft"
const EDGE = {
  "needs-input": ["var(--tasty-surface-highlight-input-border)", "var(--tasty-surface-highlight-input-width)"],
  completion: ["var(--tasty-surface-highlight-done-border)", "var(--tasty-surface-highlight-done-width)"],
  hard: ["var(--tasty-surface-occupied-hard-border)", "var(--tasty-surface-occupied-border-width)"],
  soft: ["var(--tasty-surface-occupied-soft-border)", "var(--tasty-surface-occupied-border-width)"],
};
function Surface({ focused, ring, edge, cmd = "cargo test", id }) {
  return (
    <div style={{ position: "relative", flex: 1, minWidth: 0, padding: "8px 10px", fontFamily: "var(--tasty-font-mono)", fontSize: 12, lineHeight: 1.5,
      background: focused ? "var(--tasty-surface-terminal-focused-bg)" : "var(--tasty-surface-terminal-unfocused-bg)",
      color: focused ? "var(--tasty-text-primary)" : "var(--tasty-surface-terminal-unfocused-fg)" }}>
      <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> <span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-blue)" }}>main</span></div>
      <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-mauve)" }}>❯ </span>{cmd}</div>
      {id && <div style={{ marginTop: 4, fontSize: 10, color: "var(--tasty-text-muted)" }}>{id}</div>}
      {edge && <span aria-hidden style={{ position: "absolute", inset: 0, pointerEvents: "none", zIndex: 2, border: `${EDGE[edge][1]} solid ${EDGE[edge][0]}` }} />}
      {ring && <MoveRing />}
    </div>
  );
}
function Pane({ tabs, ring, children }) {
  return (
    <div style={{ position: "relative", flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
      <div style={{ display: "flex", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", overflow: "hidden" }}>
        {tabs.map((t) => <TabCell key={t.label} {...t} />)}
      </div>
      <div style={{ flex: 1, minHeight: 0, display: "flex", gap: "var(--tasty-border-width)", background: "var(--tasty-separator)" }}>{children}</div>
      {ring && <MoveRing />}
    </div>
  );
}
function WsRow({ name, active, cue, status = "idle" }) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", gap: "var(--tasty-workspace-dot-gap)", height: 28,
      padding: "0 var(--tasty-workspace-row-padding-x)", fontSize: 13,
      background: active ? "var(--tasty-surface-active)" : "transparent",
      boxShadow: active ? "inset var(--tasty-workspace-row-active-bar-width) 0 0 var(--tasty-accent-primary)" : "none",
      color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>
      <span style={{ width: "var(--tasty-workspace-dot-slot)", flex: "none", display: "inline-flex", justifyContent: "center" }}><StatusDot status={status} /></span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
      {cue && <MoveGlyph />}
    </div>
  );
}
function Screen({ ws = [{ name: "tasty-core", active: true, status: "running" }, { name: "data-etl" }, { name: "docs-site" }], children }) {
  return (
    <div style={{ display: "flex", height: 196, background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-separator)", overflow: "hidden" }}>
      <div style={{ width: 160, flex: "none", display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)", paddingTop: 8,
        background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        {ws.map((w) => <WsRow key={w.name} {...w} />)}
      </div>
      <div style={{ flex: 1, minWidth: 0, display: "flex", gap: "var(--tasty-border-width)", background: "var(--tasty-separator)" }}>{children}</div>
    </div>
  );
}
function RailAvatar({ ch, cue }) {
  return (
    <div style={{ position: "relative", width: "var(--tasty-size-28)", height: "var(--tasty-size-28)", display: "inline-flex", alignItems: "center", justifyContent: "center",
      borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13, color: "var(--tasty-text-secondary)" }}>
      {ch}
      {cue && (
        <span aria-label="Move source inside" style={{ position: "absolute", bottom: -1, left: -1, pointerEvents: "none", display: "inline-flex", alignItems: "center", justifyContent: "center",
          width: "var(--tasty-move-source-chip-size)", height: "var(--tasty-move-source-chip-size)", borderRadius: "var(--tasty-radius-pill)",
          background: "var(--tasty-bg-sidebar)", color: "var(--tasty-move-source-glyph)" }}>
          <Icon name="move" size="var(--tasty-move-source-chip-glyph-size)" />
        </span>
      )}
    </div>
  );
}

const TABS_L = (o = {}) => [{ label: "zsh", active: true, ...o.a }, { label: "build.log", ...o.b }];
const TABS_R = (o = {}) => [{ label: "server.log", active: true, ...o.a }];
const stage = (children, theme) => <Stage variant="tight" grid><div {...(theme ? { "data-theme": theme } : {})} style={{ padding: 12, background: "var(--tasty-bg-panel)" }}>{children}</div></Stage>;

function GalleryMoveSource() {
  return (
    <>
      <Spec id="the-mark-one-dashed-ring-three-scopes" title="The mark — one dashed ring, three scopes"
        when={<>After <b>Move Surface / Move Tab / Move Pane</b>, the armed target carries a <b>dashed 2px ring</b> in <span className="tok">--tasty-accent-move</span> (pink) until the move runs, another target is armed, or the target closes. The ring is the same for all three kinds; only the <b>rect</b> changes — surface rect, tab cell rect, or the whole pane (tab strip + content). Dashed is the discriminator: every other edge in the product (notification, occupancy, focus, divider) is solid. Static, never animated, and it does not take input.</>}>
        {stage(
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div>{cap("1 · surface — right half of a split tab")}
              <Screen>
                <Pane tabs={TABS_L()}><Surface focused /><Surface ring cmd="tail -f app.log" /></Pane>
                <Pane tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
            <div>{cap("2a · tab — active tab")}
              <Screen>
                <Pane tabs={TABS_L({ a: { ring: true } })}><Surface focused /></Pane>
                <Pane tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
            <div>{cap("2b · tab — inactive tab")}
              <Screen>
                <Pane tabs={TABS_L({ b: { ring: true } })}><Surface focused /></Pane>
                <Pane tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
            <div>{cap("3 · pane — tab strip + content in one ring")}
              <Screen>
                <Pane tabs={TABS_L()}><Surface focused /></Pane>
                <Pane ring tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
          </div>
        )}
        {stage(
          <div>{cap("Latte · 1 surface + 3 pane")}
            <Screen>
              <Pane tabs={TABS_L()}><Surface focused /><Surface ring cmd="tail -f app.log" /></Pane>
              <Pane ring tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
            </Screen></div>, "latte")}
        <Meta
          specs={[["shape", "dashed ring, 4 on / 4 off"], ["weight", <>2px <span className="tok">--tasty-focus-ring-width</span></>], ["placement", "INSIDE the rect — never on the shared divider"], ["surface", "surface rect"], ["tab", "tab cell rect (24 × tab width)"], ["pane", "pane rect = tab strip + content"], ["motion", "none — static"], ["input", "pass-through"]]}
          tokens={[{ tok: "--tasty-move-source-ring", use: "ring (→ accent-move → pink)", color: "var(--tasty-move-source-ring)" }, { tok: "--tasty-move-source-ring-width", use: "2px" }, { tok: "--tasty-move-source-dash", use: "dash 4" }, { tok: "--tasty-move-source-dash-gap", use: "gap 4" }]} />
        <Note><b>Same language, different rect.</b> A pane ring encloses the tab strip, a surface ring does not — that alone tells the three apart, and the menu already names the kind in the toast. <b>Why pink.</b> Blue, yellow, green, peach, mauve, lavender and sky already mean completion, needs-input, occupancy, agent, attach and remote; pink is the one Catppuccin hue with no state attached, and it stays far from all of them at a 2px edge. <b>Inside, not outside:</b> split surfaces share a 1px divider, so an outside ring would paint over the neighbour.</Note>
      </Spec>

      <Spec id="tab-cell-ring-over-every-tab-state" title="Tab cell — ring over every tab state"
        when={<>The tab ring sits on top of the cell whatever its state. On the <b>active</b> tab the top edge crosses the 2px active indicator: pink dashes cover it, the blue shows through the gaps, so "active" stays readable from the panel fill and title color.</>}>
        {stage(
          <div style={{ display: "grid", gridTemplateColumns: "80px auto auto", gap: 8, alignItems: "center" }}>
            <span />{cap("rest")}{cap("move source")}
            {[["active", { active: true }], ["inactive", {}], ["hover", { hover: true }]].map(([k, p]) => (
              <React.Fragment key={k}>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{k}</span>
                <div style={{ display: "flex", background: "var(--tasty-bg-sidebar)" }}><TabCell label="build.log" {...p} /></div>
                <div style={{ display: "flex", background: "var(--tasty-bg-sidebar)" }}><TabCell label="build.log" ring {...p} /></div>
              </React.Fragment>
            ))}
          </div>
        )}
        <Meta specs={[["rect", "tab cell, incl. its right separator"], ["active indicator", "dashes over it, gaps show it"], ["title / icon", "unchanged — attention tint still applies"]]} />
      </Spec>

      <Spec id="target-not-visible-move-glyph-on-the-nearest-visible-container" title="Target not visible — move glyph on the nearest visible container"
        when={<>The target is often out of sight: the user arms a move, then goes to another workspace to pick the destination. The <b>nearest visible container</b> of the target shows the <b>move glyph</b> (12px, pink): the <b>workspace row</b> (and the <b>rail avatar</b> when collapsed) when the target lives in another workspace, or the <b>inactive tab cell</b> that holds a target surface in this workspace. <b>Ring = this is it; glyph = it is in here.</b> Only one cue is on screen at a time; once the target itself is visible, the glyph goes and the ring shows.</>}>
        {stage(
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div>{cap("4a · target (any kind) is in workspace data-etl")}
              <Screen ws={[{ name: "tasty-core", active: true, status: "running" }, { name: "data-etl", cue: true }, { name: "docs-site" }]}>
                <Pane tabs={TABS_L()}><Surface focused /></Pane>
                <Pane tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
            <div>{cap("4b · target surface sits in inactive tab build.log, this workspace")}
              <Screen>
                <Pane tabs={TABS_L({ b: { cue: true } })}><Surface focused /></Pane>
                <Pane tabs={TABS_R()}><Surface cmd="npm run dev" /></Pane>
              </Screen></div>
            <div>{cap("4c · collapsed rail — corner chip, bottom-left")}
              <div style={{ display: "flex", gap: 12, padding: 10, width: "max-content", background: "var(--tasty-bg-sidebar)" }}>
                <RailAvatar ch="T" /><RailAvatar ch="D" cue /><RailAvatar ch="S" />
              </div></div>
          </div>
        )}
        <Meta
          specs={[["row", "trailing, before the badge group"], ["tab cell", "after the title (title truncates)"], ["rail", "12px chip bottom-left — top-right = attention dot, bottom-right = mirror chip"], ["text", "none — glyph only, no i18n length"]]}
          tokens={[{ tok: "--tasty-move-source-glyph", use: "glyph (→ accent-move)", color: "var(--tasty-move-source-glyph)" }, { tok: "--tasty-move-source-glyph-size", use: "12 — row · tab" }, { tok: "--tasty-move-source-chip-size", use: "12 — rail chip" }, { tok: "--tasty-move-source-chip-glyph-size", use: "8 — glyph in chip" }]} />
        <Dont><b>Don't</b> ring a container. A ringed tab means "this tab moves"; a ringed workspace row would read as "this workspace moves", which is not an action.</Dont>
      </Spec>

      <Spec id="overlap-dashes-on-top-gaps-show-what-is-underneath" title="Overlap — dashes on top, gaps show what is underneath"
        when={<>The move ring is <b>painted last</b> on the target rect, above the notification border, occupancy border, occupancy overlay and focus bed. It never replaces them: the <b>dashes</b> are pink, the <b>gaps</b> show the edge beneath. A move source that also needs input reads as a pink/yellow dashed band — both facts at once, no stacked edges, no priority to resolve.</>}>
        {stage(
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 8 }}>
            {[["focused", { focused: true }], ["+ needs-input", { edge: "needs-input", cmd: "Overwrite? [y/N]" }], ["+ completion", { edge: "completion", cmd: "build passed" }], ["+ occupied · hard", { edge: "hard", cmd: "remote session" }]].map(([k, p]) => (
              <div key={k}>{cap(k)}<div style={{ display: "flex", height: 88 }}><Surface ring {...p} /></div></div>
            ))}
          </div>
        )}
        {stage(
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 8 }}>
            {[["Latte · focused", { focused: true }], ["Latte · + needs-input", { edge: "needs-input", cmd: "Overwrite? [y/N]" }], ["Latte · + completion", { edge: "completion", cmd: "build passed" }], ["Latte · + occupied · hard", { edge: "hard", cmd: "remote session" }]].map(([k, p]) => (
              <div key={k}>{cap(k)}<div style={{ display: "flex", height: 88 }}><Surface ring {...p} /></div></div>
            ))}
          </div>, "latte")}
        <Meta specs={[["z-order", "focus bed < occupancy overlay < notification / occupancy edge < move ring"], ["2px edge under", "gaps show it at full width"], ["1px edge under", "gaps show 1px edge + 1px bed"], ["clears", "move runs · another target armed · target closes"], ["kept on", "focus change · tab switch · workspace switch"]]} />
        <Note><b>Not decided, deliberately left out.</b> <b>Dimming</b> the source the way Explorer dims a cut file: rejected — the source stays fully usable while armed, and a dim collides with the unfocused-surface dim. <b>Destination candidates</b> while the menu is open: rejected — every other surface/tab/pane is a candidate, so marking them all carries no information, and the menu is native.</Note>
      </Spec>
    </>
  );
}

export { GalleryMoveSource };
