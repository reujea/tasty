import React from "react";
import { Icon, StatusDot, Tooltip } from "../../components/index.js";
import { Spec, Stage, Meta, Note, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Layouts › Pane tab strip: tab-cell status cluster, scroll arrows,
// and the move-source cue for a target scrolled out of view (2026-09-29).
// Mounted by layouts/index.jsx as <GalleryTabStrip />.
// Requests: html-script-marker-tab-slot · tab-scroll-arrow-shape · move-source-hidden-target.




const cap = (t) => <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)", marginBottom: "var(--tasty-space-xs)" }}>{t}</div>;
const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
const Themed = ({ attr, children, style }) => (
  <div {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)",
    background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", ...style }}>{children}</div>
);

// ── tab cell: [icon][label…] · right cluster [html marker][move][busy][close] ──
function Marker({ kind, hover }) {
  const lock = kind === "blocked";
  return (
    <span title={lock ? "Scripts blocked. Click to show the notice again." : "Scripts allowed for this document until Tasty restarts"}
      style={{ flex: "none", width: "var(--tasty-html-script-marker-hit)", height: "var(--tasty-html-script-marker-hit)", display: "inline-flex", alignItems: "center", justifyContent: "center",
        borderRadius: "var(--tasty-radius-sm)", cursor: lock ? "pointer" : "default",
        background: lock && hover ? "var(--tasty-html-script-marker-hover-bg)" : "transparent",
        color: lock ? (hover ? "var(--tasty-html-script-marker-fg-hover)" : "var(--tasty-html-script-marker-fg)") : "var(--tasty-html-script-marker-allowed-fg)" }}>
      <Icon name={lock ? "lock" : "scriptFile"} size="var(--tasty-html-script-marker-size)" />
    </span>
  );
}
function TabCellS({ label = "report.html", active, hover, marker, markerHover, markerTip, move, busy, width = "var(--tasty-tab-width)" }) {
  const showX = active || hover;
  const fg = active ? "var(--tasty-tab-fg-active)" : hover ? "var(--tasty-tab-fg-hover)" : "var(--tasty-tab-fg)";
  return (
    <div style={{ position: "relative", width, height: "var(--tasty-tab-height)", flex: "none", display: "flex", alignItems: "center", gap: "var(--tasty-tab-gap)",
      padding: "0 var(--tasty-space-xs) 0 var(--tasty-tab-padding-x)", color: fg,
      background: active ? "var(--tasty-tab-bg-active)" : hover ? "var(--tasty-surface-hover)" : "var(--tasty-tab-bg)",
      borderRight: "var(--tasty-border-width) solid var(--tasty-tab-separator)" }}>
      {active && <span style={{ position: "absolute", top: 0, left: 0, right: 0, height: "var(--tasty-tab-indicator-width)", background: "var(--tasty-tab-indicator)" }} />}
      <Icon name="html" size="var(--tasty-tab-icon-size)" />
      <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      {/* right cluster — fixed slots, never shrink; the label gives way first */}
      <span style={{ flex: "none", display: "inline-flex", alignItems: "center", gap: "var(--tasty-tab-status-gap)" }}>
        {marker && (markerTip
          ? <Tooltip open placement="top" content={marker === "blocked" ? "Scripts blocked. Click to show the notice again." : "Scripts allowed for this document until Tasty restarts"}><Marker kind={marker} hover={markerHover} /></Tooltip>
          : <Marker kind={marker} hover={markerHover} />)}
        {move && <span style={{ display: "inline-flex", color: "var(--tasty-move-source-glyph)" }}><Icon name="move" size="var(--tasty-move-source-glyph-size)" /></span>}
        {busy && <span style={{ display: "inline-flex", ["--tasty-status-dot-size"]: "var(--tasty-tab-dot-size)" }}><StatusDot status="running" /></span>}
        <span style={{ width: "var(--tasty-tab-close-size)", height: "var(--tasty-tab-close-size)", display: "inline-flex", alignItems: "center", justifyContent: "center",
          borderRadius: "var(--tasty-tab-close-radius)", color: "var(--tasty-text-muted)", visibility: showX ? "visible" : "hidden" }}><Icon name="close" size="var(--tasty-icon-size-xs)" /></span>
      </span>
    </div>
  );
}

// ── scroll arrow cell ──
function Arrow({ dir, disabled, hover, move }) {
  const color = disabled ? "var(--tasty-tab-scroll-arrow-fg-disabled)" : move ? "var(--tasty-tab-scroll-arrow-move-fg)" : "var(--tasty-tab-scroll-arrow-fg)";
  return (
    <span aria-disabled={disabled || undefined} title={move && !disabled ? "Move source is further " + dir : undefined}
      style={{ flex: "none", width: "var(--tasty-tab-scroll-arrow-width)", height: "var(--tasty-tab-height)", display: "inline-flex", alignItems: "center", justifyContent: "center",
        background: hover && !disabled ? "var(--tasty-tab-scroll-arrow-hover-bg)" : "transparent", color, cursor: disabled ? "default" : "pointer" }}>
      <Icon name={dir === "left" ? "chevronLeft" : "chevronRight"} size="var(--tasty-tab-scroll-arrow-glyph-size)" />
    </span>
  );
}
function Strip({ focused = true, at = "start", hoverSide, moveSide, tabs }) {
  return (
    <div style={{ display: "flex", alignItems: "stretch", width: "var(--tasty-size-560)", maxWidth: "100%", overflow: "hidden",
      background: focused ? "var(--tasty-surface-raised)" : "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <Arrow dir="left" disabled={at === "start"} hover={hoverSide === "left"} move={moveSide === "left"} />
      <div style={{ flex: 1, minWidth: 0, display: "flex", overflow: "hidden" }}>
        {(tabs || ["server", "dev", "vim", "logs", "tests"]).map((t, i) => <TabCellS key={t} label={t} active={i === 1} />)}
      </div>
      <Arrow dir="right" disabled={at === "end"} hover={hoverSide === "right"} move={moveSide === "right"} />
    </div>
  );
}

function GalleryTabStrip() {
  return (
    <>
      <Spec id="tab-cell-the-right-hand-status-cluster" title="Tab cell — the right-hand status cluster"
        when={<>Everything a tab reports sits in <b>one fixed cluster at the right end</b> of the cell, never glued to the label text. Order, left → right: <b>HTML script marker</b> · <b>move glyph</b> · <b>busy dot</b> · <b>close</b>. Items are <span className="tok">--tasty-tab-status-gap</span> (4) apart; the label keeps <span className="tok">--tasty-tab-gap</span> (8) to the cluster. The cluster never shrinks: the <b>label ellipsises first</b>, so markers stay visible in a narrow tab. Close keeps its slot (hidden until active / hover) so nothing shifts on hover. The <b>lock</b> marker has a 16 × 16 hit cell (<span className="tok">--tasty-html-script-marker-hit</span>) with its own hover fill; hovering it is still hovering the tab, so close shows too. The <b>scriptFile</b> marker is tooltip-only, no hover fill.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              {[["marker only · inactive", { marker: "blocked" }], ["marker only · active", { marker: "blocked", active: true }], ["allowed · active", { marker: "allowed", active: true }],
                ["marker + busy", { marker: "blocked", busy: true }], ["marker + move", { marker: "blocked", move: true }], ["marker + move + busy · hover", { marker: "blocked", move: true, busy: true, hover: true }],
                ["hover on the lock", { marker: "blocked", hover: true, markerHover: true }], ["narrow tab — label ellipsises", { marker: "blocked", move: true, busy: true, active: true, label: "quarterly-report-final.html", width: "var(--tasty-size-120)" }]].map(([c, p]) => (
                <div key={c}>{cap(c)}<div style={{ display: "flex", background: "var(--tasty-bg-sidebar)" }}><TabCellS {...p} /></div></div>
              ))}
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["order", "label … | marker · move · busy · close"], ["cluster gap", "4 · tab-status-gap"], ["label → cluster", "8 · tab-gap"], ["squeeze", "label ellipsises; cluster fixed"], ["lock hit", "16 × 16, hover = overlay-hover"], ["lock click", "re-shows the banner only (no tab switch, no focus change) · mouse only"], ["scriptFile", "tooltip only"], ["tooltip placement", "top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip (see next spec)"]]}
          tokens={[{ tok: "--tasty-tab-status-gap", use: "→ space-xs 4" }, { tok: "--tasty-html-script-marker-hit", use: "→ size-16" }, { tok: "--tasty-html-script-marker-hover-bg", use: "→ overlay-hover" }, { tok: "--tasty-html-script-marker-fg", use: "lock", color: "var(--tasty-html-script-marker-fg)" }]} />
      </Spec>

      <Spec id="tooltips-in-the-strip-open-upward-native-content-below" title="Tooltips in the strip open upward — native content below"
        when={<>Under an HTML tab the pane body is a <b>native WebView</b>, and the OS always draws it above Tasty's own layer. A tooltip opened <b>down</b> from the strip lands inside it and is never seen. So every tooltip anchored in a <b>pane tab strip or pane head</b> (script marker, close, scroll arrows, head buttons) opens <b>top</b>, over the title area or the pane above, whatever the surface kind, so the strip behaves the same everywhere. Candidates in order: <b>top</b>, then <b>bottom</b>; the first that fits the window and clears every native content rect wins. <b>Revised 2026-10-06</b> — if neither clears (an HTML pane stacked above another HTML pane), a third candidate is tried: <b>inside the strip</b> — the bubble is centred vertically on the strip row (Tasty paints the strip, so no WebView covers it) and placed beside the anchor cell, right first, then left, <span className="tok">--tasty-tooltip-offset</span> from the cell. It may cover neighbouring tabs; that is the price of being seen. <b>Revised 2026-10-06 b2</b> — the in-strip candidate tolerates 1px (border-width) per edge, and when every candidate fails the bubble is still placed <b>inside the strip</b>, the one place Tasty paints over. Top clamped (<span className="tok">--tasty-tooltip-offset</span> as the vertical margin, never flush at y = 0) remains only for tooltips with <b>no anchor cell</b>, i.e. outside a strip. Horizontally the bubble clamps to the window edges with <span className="tok">--tasty-tooltip-offset</span>. Hiding the WebView while a tooltip is up is rejected: the page would flash on every hover. Hover delay, copy and click are unchanged.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", paddingRight: "var(--tasty-size-120)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexDirection: "column", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · html tab active, lock hovered</span>
              <div style={{ width: "var(--tasty-size-320)", display: "flex", flexDirection: "column", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ height: "var(--tasty-titlebar-height)", flex: "none", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-bg-app)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>title area</div>
                <div style={{ display: "flex", background: "var(--tasty-surface-raised)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                  <TabCellS marker="blocked" active markerHover markerTip />
                  <TabCellS label="shell" />
                </div>
                <div style={{ height: "var(--tasty-size-96)", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--tasty-bg-panel)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>WebView — native, drawn above Tasty</div>
              </div>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · html pane under an html pane — fallback inside the strip</span>
              <div style={{ width: "var(--tasty-size-320)", display: "flex", flexDirection: "column", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
                <div style={{ height: "var(--tasty-size-64)", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--tasty-bg-panel)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>upper pane WebView</div>
                <div style={{ position: "relative", display: "flex", background: "var(--tasty-surface-raised)" /* app geometry: own WebView starts flush under the strip, gap 0 */ }}>
                  <TabCellS marker="blocked" active markerHover />
                  <TabCellS label="shell" />
                  <span role="tooltip" style={{ position: "absolute", top: "50%", left: "calc(var(--tasty-tab-width) + var(--tasty-tooltip-offset))", transform: "translateY(-50%)", whiteSpace: "nowrap",
                    padding: "var(--tasty-space-xs) var(--tasty-space-sm)", background: "var(--tasty-tooltip-bg)", color: "var(--tasty-tooltip-fg)", border: "var(--tasty-border-width) solid var(--tasty-tooltip-border)", borderRadius: "var(--tasty-tooltip-radius)", fontSize: "var(--tasty-font-size-caption)", boxShadow: "var(--tasty-shadow-popover)" }}>Scripts blocked. Click to show the notice again.</span>
                </div>
                <div style={{ height: "var(--tasty-size-64)", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--tasty-bg-panel)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>own WebView</div>
              </div>
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["scope", "every tooltip anchored in a pane tab strip or pane head"], ["placement", "top → bottom → inside the strip; first that clears all native rects"], ["inside the strip", "centred on the strip row · beside the anchor cell (right, then left) · tooltip-offset from the cell · may cover neighbour tabs"], ["overlap tolerance (b2)", "the in-strip candidate is judged against native rects with a tolerance of border-width (1px) per edge — a one-line bubble (25) on the 24 strip passes; nothing else changes size"], ["last resort (b2)", "if every candidate fails, the in-strip placement is used anyway (it is the one Tasty paints over); clamped top is no longer the final fallback"], ["specimen (b2)", "themes stacked vertically; real copy; the bubble may run past the example window inside the stage padding — the app clamps to the app window, not to a pane"], ["none clears", "superseded 2026-10-06 b2 → in-strip placement (see last resort)"], ["horizontal", "clamped to window edges · tooltip-offset 4"], ["WebView hiding", "not used for tooltips"], ["delay · copy · click", "unchanged"]]}
          tokens={[{ tok: "--tasty-tooltip-offset", use: "→ space-xs 4 · anchor gap + edge clamp" }, { tok: "--tasty-tooltip-bg", use: "bubble", color: "var(--tasty-tooltip-bg)" }]} />
      </Spec>

      <Spec id="scroll-arrows-chevron-icon-square-cell-no-own-fill" title="Scroll arrows — chevron icon, square cell, no own fill"
        when={<>The strip's scroll arrows are <b>icons</b>: <b>chevronLeft / chevronRight</b> at <span className="tok">--tasty-tab-scroll-arrow-glyph-size</span> (<b>14</b> since 2026-10-06 — at 12 the 1px stroke lands on a pixel seam and rasters at half alpha). The <code>&lt;</code> <code>&gt;</code> characters go: a text character is not an icon. The cell is square, <span className="tok">--tasty-tab-scroll-arrow-width</span> = strip height 24 (was 20; the tab viewport loses 4 per side, accepted). The cell has <b>no fill of its own</b>; it sits on the strip ground (focused pane surface-raised, unfocused bg-sidebar). Hover lays <span className="tok">--tasty-tab-scroll-arrow-hover-bg</span> on the enabled side only. The same shape and roles apply to <b>every</b> horizontal tab bar with arrows, including <code>horizontal_tab_bar_with_arrows</code> (its 0.4 tint retires; the end it has reached is disabled). Contrast is judged on <b>flat colour</b> like every other token ratio; rasteriser AA is not part of the target.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              {[["focused · left end", { at: "start" }], ["focused · middle · hover right", { at: "mid", hoverSide: "right" }], ["focused · right end", { at: "end" }], ["unfocused · middle", { focused: false, at: "mid" }], ["unfocused · left end · hover left (no fill: disabled)", { focused: false, at: "start", hoverSide: "left" }]].map(([c, p]) => (
                <div key={c}>{cap(c)}<Strip {...p} /></div>
              ))}
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["glyph", "chevronLeft / chevronRight · 14"], ["cell", "24 × 24, no fill"], ["hover", "overlay-hover, enabled side only"], ["disabled", "the end reached · no hover · no response"], ["scope", "pane strip + horizontal_tab_bar_with_arrows"], ["3:1 basis", "flat colour (Mocha 5.65 · Latte 3.65)"]]}
          tokens={[{ tok: "--tasty-tab-scroll-arrow-width", use: "→ control-height-tab 24" }, { tok: "--tasty-tab-scroll-arrow-glyph-size", use: "→ icon-size-sm 14" }, { tok: "--tasty-tab-scroll-arrow-fg", use: "→ text-muted", color: "var(--tasty-tab-scroll-arrow-fg)" }, { tok: "--tasty-tab-scroll-arrow-fg-disabled", use: "→ text-disabled", color: "var(--tasty-tab-scroll-arrow-fg-disabled)" }]} />
        <Dont><b>Don't</b> draw arrows with a font glyph. A proportional <code>&lt;</code> at 11px renders thinner than the icon stroke and depends on the UI font.</Dont>
      </Spec>

      <Spec id="move-source-scrolled-out-of-view-the-arrow-on-that-side-turns-pi" title="Move source scrolled out of view — the arrow on that side turns pink"
        when={<>When the pending move target (a tab, or a surface inside an <b>inactive</b> tab) is in the <b>active workspace</b> but its tab cell is scrolled out of the strip, the <b>scroll arrow on that side</b> takes <span className="tok">--tasty-tab-scroll-arrow-move-fg</span> (= move glyph pink). Tab targets and surface targets use the same cue. <b>Exception (2026-10-06):</b> a surface inside the <b>active</b> tab gets <b>no arrow cue</b> even when that tab's cell is scrolled away — its ring is drawn in the content area, so the target is on screen; the arrow only ever points at a target you cannot see. A cell counts as visible only when it is <b>fully</b> inside the viewport; until then the arrow carries the cue. The cue is static and behaves like the arrow: hover is the arrow's hover, a click is the arrow's normal one-step scroll (no jump). It ends when the cell comes fully into view; the ring or glyph in the cell takes over. That side can never be disabled while the target is beyond it. A rect too small for the ring (under 2 × ring width) gets <b>no substitute</b>: it cannot happen at the minimum pane size. Pane maximise does not exist yet; request it separately when it does.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              {[["target out on the right", { at: "start", moveSide: "right" }], ["target out on the left", { at: "end", moveSide: "left" }], ["target out on the right · hover", { at: "mid", moveSide: "right", hoverSide: "right" }], ["unfocused pane · target out on the left", { focused: false, at: "mid", moveSide: "left" }]].map(([c, p]) => (
                <div key={c}>{cap(c)}<Strip {...p} /></div>
              ))}
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["where", "the scroll arrow on the target's side"], ["colour", "accent-move (same as the glyph)"], ["tab · surface target", "same cue"], ["surface in the ACTIVE tab", "no cue — the content-area ring already shows it (2026-10-06)"], ["visible", "cell fully inside the viewport"], ["input", "none of its own — the arrow's normal step scroll"], ["too-small rect", "no substitute"], ["maximise", "out of scope — new request later"]]}
          tokens={[{ tok: "--tasty-tab-scroll-arrow-move-fg", use: "→ move-source-glyph", color: "var(--tasty-tab-scroll-arrow-move-fg)" }]} />
        <Note>Nothing new appears in the strip. The arrow already means "more tabs this way"; the pink says the move source is one of them.</Note>
      </Spec>
    </>
  );
}

export { GalleryTabStrip };
