import React from "react";
import { Icon, StatusDot, Badge, Tag, LoadingOverlay as TastyDesignSystem_41fd3f_LoadingOverlay } from "../../components/index.js";
import { Spec, Stage, Meta, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Layouts › Loading overlay (2026-10-16, request loading-spinner-overlay).
// A Spinner laid OVER existing content on four placements: surface · tab · LNB card · rail avatar.
// Every specimen is an off / on pair at identical geometry. Mounted by layouts/index.jsx as <GalleryLoadingOverlay />.



const LoadingOverlay = TastyDesignSystem_41fd3f_LoadingOverlay || (() => null); // until the bundle recompiles

const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };
const cap = (t) => <div style={{ ...mono, marginBottom: "var(--tasty-space-xs)" }}>{t}</div>;
const BOX = { outline: "var(--tasty-border-width) dashed var(--tasty-accent-move)", outlineOffset: 0 };
const Themed = ({ attr, children, style }) => (
  <div {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)",
    background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0, ...style }}>{children}</div>
);
const Pair = ({ label, off, on }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
    {cap(label)}
    <div style={{ display: "flex", gap: "var(--tasty-space-md)", alignItems: "flex-start", flexWrap: "wrap" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)" }}><span style={mono}>off</span>{off}</div>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)" }}><span style={mono}>on</span>{on}</div>
    </div>
  </div>
);
const SwitchCap = ({ n, active }) => (
  <span aria-hidden style={{ position: "absolute", inset: 0, display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box",
    borderRadius: "var(--tasty-radius-sm)", borderStyle: "solid", borderWidth: "var(--tasty-border-width)", borderBottomWidth: "var(--tasty-switch-overlay-shadow-depth)",
    borderColor: active ? "var(--tasty-switch-overlay-active-bg)" : "var(--tasty-switch-overlay-border)",
    background: active ? "var(--tasty-switch-overlay-active-bg)" : "var(--tasty-switch-overlay-bg)", color: active ? "var(--tasty-switch-overlay-active-fg)" : "var(--tasty-switch-overlay-fg)",
    fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-kbd-font-size)", fontWeight: "var(--tasty-font-weight-medium)", lineHeight: 1 }}>{n}</span>
);

// ── tab cell: [kind icon ← overlay][label…] · [marker][move][busy][close] ──
function LTab({ label = "build-server", kind = "terminal", active, hover, att, busy, move, marker, width = "var(--tasty-tab-width)", loading, keycap, boxes, rm }) {
  const showX = active || hover;
  const fg = att === "needs-input" ? "var(--tasty-tab-fg-needs-input)" : att === "completion" ? "var(--tasty-tab-fg-completion)"
    : active ? "var(--tasty-tab-fg-active)" : hover ? "var(--tasty-tab-fg-hover)" : "var(--tasty-tab-fg)";
  const fill = active ? "var(--tasty-tab-bg-active)" : hover ? "var(--tasty-surface-hover)" : "var(--tasty-tab-bg)";
  return (
    <div style={{ position: "relative", width, height: "var(--tasty-tab-height)", flex: "none", display: "flex", alignItems: "center", gap: "var(--tasty-tab-gap)", boxSizing: "border-box",
      padding: "0 var(--tasty-space-xs) 0 var(--tasty-tab-padding-x)", color: fg, background: fill, borderRight: "var(--tasty-border-width) solid var(--tasty-tab-separator)" }}>
      {active && <span style={{ position: "absolute", top: 0, left: 0, right: 0, height: "var(--tasty-tab-indicator-width)", background: "var(--tasty-tab-indicator)" }} />}
      <span style={{ position: "relative", flex: "none", display: "inline-flex", width: "var(--tasty-tab-icon-size)", height: "var(--tasty-tab-icon-size)" }}>
        <Icon name={kind} size="var(--tasty-tab-icon-size)" />
        <LoadingOverlay kind="tab" visible={!!loading} bed={fill} reducedMotion={rm} label="Loading" style={boxes ? BOX : undefined} />
        {keycap && <SwitchCap n={keycap} active={active} />}
      </span>
      <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      <span style={{ flex: "none", display: "inline-flex", alignItems: "center", gap: "var(--tasty-tab-status-gap)" }}>
        {marker && <span style={{ width: "var(--tasty-html-script-marker-hit)", height: "var(--tasty-html-script-marker-hit)", display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-html-script-marker-fg)" }}><Icon name="lock" size="var(--tasty-html-script-marker-size)" /></span>}
        {move && <span style={{ display: "inline-flex", color: "var(--tasty-move-source-glyph)" }}><Icon name="move" size="var(--tasty-move-source-glyph-size)" /></span>}
        {busy && <span style={{ display: "inline-flex", ["--tasty-status-dot-size"]: "var(--tasty-tab-dot-size)" }}><StatusDot status="running" /></span>}
        <span style={{ width: "var(--tasty-tab-close-size)", height: "var(--tasty-tab-close-size)", display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-text-muted)", visibility: showX ? "visible" : "hidden" }}><Icon name="close" size="var(--tasty-icon-size-xs)" /></span>
      </span>
    </div>
  );
}
const TabBed = ({ children }) => <div style={{ display: "flex", background: "var(--tasty-bg-sidebar)" }}>{children}</div>;
function Arrow({ dir, disabled }) {
  return (
    <span style={{ flex: "none", width: "var(--tasty-tab-scroll-arrow-width)", height: "var(--tasty-tab-height)", display: "inline-flex", alignItems: "center", justifyContent: "center",
      color: disabled ? "var(--tasty-tab-scroll-arrow-fg-disabled)" : "var(--tasty-tab-scroll-arrow-fg)" }} data-ink-grade={disabled ? "none" : undefined} data-ink-why={disabled ? "disabled" : undefined}><Icon name={dir === "left" ? "chevronLeft" : "chevronRight"} size="var(--tasty-tab-scroll-arrow-glyph-size)" /></span>
  );
}
function OverflowStrip({ loadingIdx = [], focused = true }) {
  const tabs = ["server", "build-server", "vim", "logs", "tests", "db"];
  return (
    <div style={{ display: "flex", width: "var(--tasty-size-460)", maxWidth: "100%", overflow: "hidden", background: focused ? "var(--tasty-surface-raised)" : "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <Arrow dir="left" disabled />
      <div style={{ flex: 1, minWidth: 0, display: "flex", overflow: "hidden" }}>
        {tabs.map((t, i) => <LTab key={t} label={t} active={i === 1} loading={loadingIdx.includes(i)} />)}
      </div>
      <Arrow dir="right" />
    </div>
  );
}

// ── LNB workspace card: [dot slot ← overlay][title / REMOTE / subtitle / description][badge] ──
function LRow({ name = "build-farm", status = "running", active, hover, attached, mirror, subtitle, description, notif, loading, keycap, boxes, rm }) {
  const fill = active ? "var(--tasty-surface-active)" : hover ? "var(--tasty-overlay-hover)" : "transparent";
  const bed = `linear-gradient(${fill}, ${fill}), var(--tasty-bg-sidebar)`;
  const line = "calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui))";
  return (
    <div style={{ width: "var(--tasty-size-200)", display: "flex", alignItems: "flex-start", gap: "var(--tasty-workspace-dot-gap)", position: "relative", boxSizing: "border-box",
      padding: "var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-workspace-row-padding-x)", background: fill,
      boxShadow: active ? "inset var(--tasty-workspace-row-active-bar-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <span style={{ position: "relative", flex: "none", width: "var(--tasty-workspace-dot-slot)", height: line, display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
        <StatusDot status={status} attached={attached} />
        <LoadingOverlay kind="lnb" visible={!!loading} bed={bed} reducedMotion={rm} style={boxes ? BOX : undefined} />
        {keycap && <span style={{ position: "absolute", top: "50%", left: 0, width: "var(--tasty-switch-overlay-size)", height: "var(--tasty-switch-overlay-size)", transform: "translateY(-50%)" }}><SwitchCap n={keycap} active={active} /></span>}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-medium)", lineHeight: "var(--tasty-line-height-ui)", color: active ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</div>
        {mirror && (
          <div style={{ marginTop: "var(--tasty-size-2)" }}>
            <Tag variant="info" caps aria-label="Remote mirror" style={{ color: "var(--tasty-tag-info-fg)" }}>
              <Icon name="remote" size="var(--tasty-workspace-mirror-icon-size)" />remote
            </Tag>
          </div>
        )}
        {subtitle && <div style={{ fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)", marginTop: "var(--tasty-size-1)", color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{subtitle}</div>}
        {description && <div style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", marginTop: "var(--tasty-size-3)", color: "var(--tasty-text-placeholder)", display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{description}</div>}
      </div>
      {notif > 0 && <span style={{ flex: "none", marginLeft: "var(--tasty-space-xs)", height: line, display: "inline-flex", alignItems: "center" }}><Badge variant="primary">{notif}</Badge></span>}
    </div>
  );
}
const LnbBed = ({ children }) => <div style={{ background: "var(--tasty-bg-sidebar)", padding: "var(--tasty-space-xs) 0" }}>{children}</div>;

// ── rail avatar: 36 box · initial · corner chips · attached ring · overlay ring ──
function LAv({ ch = "B", state = "idle", notif, ring, mirror, move, refused, loading, keycap, boxes, rm }) {
  const bg = state === "active" ? "var(--tasty-overlay-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent";
  const chip = { position: "absolute", pointerEvents: "none", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-bg-sidebar)", boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" };
  return (
    <div style={{ position: "relative", display: "inline-flex", flex: "none" }}>
      <span style={{ width: "var(--tasty-size-36)", height: "var(--tasty-size-36)", display: "inline-flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--tasty-radius)", background: bg,
        fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-font-weight-bold)", fontSize: "var(--tasty-font-size-body)", color: state === "active" ? "var(--tasty-accent-primary-raised)" : "var(--tasty-text-muted)" }}>{ch}</span>
      <LoadingOverlay kind="rail" visible={!!loading} reducedMotion={rm} style={boxes ? BOX : undefined} />
      {ring && <span style={{ position: "absolute", inset: 1, borderRadius: "var(--tasty-radius)", outline: "var(--tasty-status-dot-attached-ring-width) solid var(--tasty-accent-attached)", outlineOffset: "var(--tasty-status-dot-attached-ring-offset)", pointerEvents: "none" }} />}
      {notif && <span style={{ position: "absolute", top: -1, right: -1, pointerEvents: "none" }}><Badge dot variant="primary" style={{ boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }} /></span>}
      {mirror && <span style={{ ...chip, bottom: -1, right: -1, width: "var(--tasty-workspace-mirror-chip-size)", height: "var(--tasty-workspace-mirror-chip-size)", border: "var(--tasty-border-width) solid var(--tasty-workspace-mirror-chip-border)", background: "var(--tasty-workspace-mirror-chip-bg)", color: "var(--tasty-workspace-mirror-fg)" }}><Icon name="remote" size="var(--tasty-size-8)" /></span>}
      {move && <span style={{ ...chip, bottom: -1, left: -1, width: "var(--tasty-move-source-chip-size)", height: "var(--tasty-move-source-chip-size)", color: "var(--tasty-move-source-glyph)" }}><Icon name="move" size="var(--tasty-move-source-chip-glyph-size)" /></span>}
      {refused && <span style={{ ...chip, top: -1, left: -1, width: "var(--tasty-attach-refusal-chip-size)", height: "var(--tasty-attach-refusal-chip-size)", color: "var(--tasty-attach-refusal-glyph)" }}><Icon name="alertTriangle" size="var(--tasty-attach-refusal-chip-glyph-size)" /></span>}
      {keycap && <span style={{ position: "absolute", top: "50%", left: "50%", width: "var(--tasty-switch-overlay-size)", height: "var(--tasty-switch-overlay-size)", transform: "translate(-50%,-50%)" }}><SwitchCap n={keycap} active={state === "active"} /></span>}
    </div>
  );
}
const RailBed = ({ children }) => <div style={{ display: "inline-flex", padding: "var(--tasty-space-sm)", background: "var(--tasty-bg-sidebar)" }}>{children}</div>;

// ── surfaces ──
const TERM = ["$ tasty remote attach build-01", "connecting to build-01:7420 …", "handshake ok · tasty 0.9.4", "requesting workspace layout", "$ ▌"];
function TermSurface({ loading, search, moveRing, boxes, rm, w = "var(--tasty-size-288)", h = "var(--tasty-size-140)" }) {
  return (
    <div style={{ position: "relative", width: w, height: h, boxSizing: "border-box", padding: "var(--tasty-space-sm)", overflow: "hidden", background: "var(--tasty-bg-panel)",
      border: "var(--tasty-border-width) solid var(--tasty-border-default)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", lineHeight: 1.5, color: "var(--tasty-text-secondary)" }}>
      {TERM.map((l, i) => <div key={i} style={{ whiteSpace: "nowrap" }}>{l}</div>)}
      {moveRing && <span style={{ position: "absolute", inset: 0, boxShadow: "inset 0 0 0 var(--tasty-move-source-ring-width) var(--tasty-move-source-ring)", pointerEvents: "none" }} />}
      <LoadingOverlay kind="surface" visible={!!loading} reducedMotion={rm} style={boxes ? BOX : undefined} />
      {search && (
        <div style={{ position: "absolute", top: "var(--tasty-space-sm)", right: "var(--tasty-space-sm)", display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", height: "var(--tasty-size-24)", padding: "0 var(--tasty-space-sm)",
          background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-search-bar-border)", borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-muted)", fontSize: "var(--tasty-font-size-caption)" }}>
          <Icon name="search" size="var(--tasty-icon-size-xs)" /><span style={{ color: "var(--tasty-text-primary)" }}>handshake</span><span>1/1</span>
        </div>
      )}
    </div>
  );
}
function DocSurface({ loading, boxes, rm }) {
  return (
    <div style={{ position: "relative", width: "var(--tasty-size-288)", height: "var(--tasty-size-140)", boxSizing: "border-box", padding: "var(--tasty-space-md)", overflow: "hidden", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>
      <div style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-bold)", color: "var(--tasty-text-primary)", marginBottom: "var(--tasty-space-xs)" }}>Remote setup</div>
      <div style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: 1.5, color: "var(--tasty-text-secondary)" }}>Attach mirrors every workspace of the host into this window. Layout arrives first, scrollback follows per surface.</div>
      <LoadingOverlay kind="surface" visible={!!loading} reducedMotion={rm} style={boxes ? BOX : undefined} />
    </div>
  );
}
function WebViewSurface() {
  return (
    <div style={{ width: "var(--tasty-size-288)", height: "var(--tasty-size-140)", boxSizing: "border-box", display: "flex", alignItems: "center", justifyContent: "center", textAlign: "center", padding: "var(--tasty-space-md)",
      background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) dashed var(--tasty-border-strong)", ...mono }}>WebView (native, drawn above Tasty) · no in-surface spinner · the tab and the LNB card carry it</div>
  );
}

// ── all three at once ──
function MiniWindow({ loading, rm }) {
  return (
    <div style={{ display: "flex", width: "var(--tasty-size-560)", maxWidth: "100%", height: "var(--tasty-size-200)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ width: 52, flex: "none", display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-size-2)", padding: "var(--tasty-space-sm) 0", background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <LAv ch="T" /><LAv ch="B" state="active" mirror loading={loading} rm={rm} /><LAv ch="S" notif />
      </div>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <div style={{ display: "flex", background: "var(--tasty-surface-raised)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", overflow: "hidden" }}>
          <LTab label="build-server" active loading={loading} rm={rm} /><LTab label="logs" busy />
        </div>
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}><TermSurface loading={loading} rm={rm} w="100%" h="100%" /></div>
      </div>
    </div>
  );
}

function GalleryLoadingOverlay() {
  return (
    <>
      <Spec id="loading-overlay-four-placements-over-existing-content" title="Loading overlay — four placements over existing content"
        when={<>While a slow request runs (first: <b>remote attach</b> adding its structure) a <b>LoadingOverlay</b> is drawn <b>on top of what is already there</b>. The <b>part itself</b> adds no tab, pane, slot or "Loading…" title and never touches the stored name. (2026-10-19) This is the part's responsibility only: when the user asks for a new tab or split, the <b>client first lays out a normal tab / split</b> and the part is drawn on it — see <i>Pre-layout</i> below. Each placement has a fixed box (dashed below = the overlay rect): <b>surface</b> = the surface rect, spinner 16 on a 24 bed bottom-right · <b>tab</b> = the 14 kind-icon slot · <b>LNB card</b> = the 16 dot slot · <b>rail</b> = the 36 avatar, a 28 ring around the initial. Ink is one token everywhere. The overlay has <b>no hit-test and no focus</b>: click, drag, hover, close, select and keys behave as if it were not there. The caller passes the rect and <code>visible</code>; the part owns no timers.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · overlay boxes shown</span>
              <Pair label="surface" off={<TermSurface />} on={<TermSurface loading boxes />} />
              <Pair label="tab" off={<TabBed><LTab active /></TabBed>} on={<TabBed><LTab active loading boxes /></TabBed>} />
              <Pair label="LNB card" off={<LnbBed><LRow active /></LnbBed>} on={<LnbBed><LRow active loading boxes /></LnbBed>} />
              <Pair label="rail avatar" off={<RailBed><LAv state="active" /></RailBed>} on={<RailBed><LAv state="active" loading boxes /></RailBed>} />
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["surface", "rect = surface · bed 24 bottom-right, inset 8 · spinner 16"], ["tab", "rect = kind-icon slot 14 · bed = cell fill · spinner 14"], ["LNB card", "rect = dot slot 16 × title line · bed = row fill · spinner 14"], ["rail", "rect = avatar 36 · no bed · ring 28, 2px"], ["mask", "none — no dim, no scrim, nothing outside the box"], ["input", "pointer-events none · never focused"], ["lifetime", "caller: visible + rect; no network/journal/timer state"], ["geometry", "unchanged on/off — tab count, widths, title, surface rect, PTY grid, LNB rows"]]}
          tokens={[{ tok: "--tasty-loading-overlay-indicator", use: "→ spinner-indicator → accent-primary", color: "var(--tasty-loading-overlay-indicator)" }, { tok: "--tasty-loading-overlay-surface-bed-bg", use: "→ surface-raised", color: "var(--tasty-loading-overlay-surface-bed-bg)" }, { tok: "--tasty-loading-overlay-surface-bed-border", use: "→ border-default", color: "var(--tasty-loading-overlay-surface-bed-border)" }, { tok: "--tasty-loading-overlay-tab-size", use: "→ tab-icon-size 14" }, { tok: "--tasty-loading-overlay-lnb-size", use: "→ icon-size-sm 14" }, { tok: "--tasty-loading-overlay-rail-size", use: "→ size-28" }, { tok: "--tasty-loading-overlay-surface-inset", use: "→ space-sm 8" }]} />
      </Spec>

      <Spec id="tab-the-spinner-takes-the-kind-icon-slot-nothing-else" title="Tab — the spinner takes the kind-icon slot, nothing else"
        when={<>The spinner covers <b>only the 14 kind icon</b> at the start of the cell, on a bed of the cell's current fill (inactive / hover / active). The label, its attention colour (needs-input, completion), the active indicator and the whole right cluster (<b>HTML marker · move · busy · close</b>) stay as they are; close still appears on hover. Loading (accent arc, left) and busy (green dot, right) can show together and do not read alike. The <b>Ctrl switch keycap</b> fills the same slot and draws <b>above</b> the spinner while the modifier is held. In a narrow tab the label still ellipsises first. A tab scrolled out of the strip is not drawn, so its spinner needs no repaint.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              {[["inactive", {}], ["active", { active: true }], ["hover · close shows", { hover: true }], ["attention · needs-input", { att: "needs-input" }], ["attention · completion", { att: "completion" }],
                ["busy + loading", { busy: true, active: true }], ["move + HTML marker", { kind: "html", label: "report.html", move: true, marker: true }], ["narrow 120 · long title", { active: true, busy: true, move: true, label: "remote-attach-build-farm-eu", width: "var(--tasty-size-120)" }],
                ["Ctrl held · keycap above", { active: true, keycap: "2" }]].map(([c, p]) => (
                <Pair key={c} label={c} off={<TabBed><LTab {...p} /></TabBed>} on={<TabBed><LTab {...p} loading /></TabBed>} />
              ))}
              <div>{cap("overflow strip · off")}<OverflowStrip /></div>
              <div>{cap("overflow strip · on (2 tabs)")}<OverflowStrip loadingIdx={[1, 4]} /></div>
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["covers", "kind icon only"], ["bed", "the cell's resolved fill (tab-bg / surface-hover / tab-bg-active)"], ["kept", "label · attention colour · indicator · marker · move · busy · close"], ["busy + loading", "both visible — arc left, green dot right"], ["Ctrl keycap", "above the spinner (same slot)"], ["off-screen tab", "not drawn · no repaint"]]}
          tokens={[{ tok: "--tasty-loading-overlay-tab-size", use: "→ tab-icon-size 14" }]} />
      </Spec>

      <Spec id="lnb-card-the-spinner-takes-the-dot-slot" title="LNB card — the spinner takes the dot slot"
        when={<>On the expanded sidebar the spinner sits in the card's <b>16 dot slot</b> (title-line height) over the status dot and, if present, the attached ring, on a bed of the row's current fill. Title, <b>REMOTE</b> pill, subtitle, description, notification badge, active accent bar and hover are untouched; the ring and dot come back as they are when loading ends. Only the loading card changes; no other card and no part of the LNB is covered. The <b>Alt switch keycap</b> draws above it. Works the same with categories on or off. A card inside a <b>collapsed category</b> is not drawn, and its category header (and rail <code>---</code> button) shows <b>nothing</b>: it follows the existing collapse rule.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              {[["inactive", { status: "idle" }], ["active", { active: true }], ["hover", { hover: true, status: "idle" }], ["attached ring", { attached: true }],
                ["REMOTE · subtitle · description · notif", { mirror: true, subtitle: "build-01 · 4 surfaces", description: "Nightly build farm, EU region. Mirrors the remote host's layout.", notif: 3 }],
                ["Alt held · keycap above", { active: true, keycap: "2" }]].map(([c, p]) => (
                <Pair key={c} label={c} off={<LnbBed><LRow {...p} /></LnbBed>} on={<LnbBed><LRow {...p} loading /></LnbBed>} />
              ))}
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["covers", "status dot + attached ring (dot slot 16)"], ["bed", "bg-sidebar + the row fill (surface-active / overlay-hover)"], ["kept", "title · REMOTE · subtitle · description · badge · accent bar · hover"], ["Alt keycap", "above the spinner"], ["categories", "same on/off"], ["collapsed category", "card not drawn · header and rail --- show nothing"]]}
          tokens={[{ tok: "--tasty-loading-overlay-lnb-size", use: "→ icon-size-sm 14" }, { tok: "--tasty-workspace-dot-slot", use: "→ size-16 (rect)" }]} />
      </Spec>

      <Spec id="rail-avatar-a-ring-around-the-initial" title="Rail avatar — a ring around the initial"
        when={<>The four avatar corners are taken (TL refusal · TR attention dot · BL move · BR mirror) and the initial is the avatar's identity, so the spinner is a <b>28 ring around the initial</b> with no bed. The initial stays readable inside it. Corner chips keep their 2px <code>bg-sidebar</code> halo and draw <b>above</b> the ring, so they cut it cleanly; the attached ring (square, lavender) sits outside it. Active, hover and the Ctrl+Alt switch keycap behave as today; the keycap draws above.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap" }}>
                {[["idle", {}], ["active", { state: "active" }], ["hover", { state: "hover" }], ["notif", { notif: true }], ["attached ring", { ring: true }], ["mirror", { mirror: true }], ["move", { move: true }], ["refused", { refused: true }],
                  ["all marks", { notif: true, ring: true, mirror: true, move: true, refused: true, state: "active" }], ["keycap above", { keycap: "2" }]].map(([c, p]) => (
                  <Pair key={c} label={c} off={<RailBed><LAv {...p} /></RailBed>} on={<RailBed><LAv {...p} loading /></RailBed>} />
                ))}
              </div>
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["ring", "28 · 2px · centred on the 36 avatar"], ["initial", "kept, inside the ring"], ["corner chips", "above the ring (halo cuts it)"], ["attached ring", "unchanged, outside"], ["collapsed category", "avatar not drawn · nothing on ---"]]}
          tokens={[{ tok: "--tasty-loading-overlay-rail-size", use: "→ size-28" }]} />
      </Spec>

      <Spec id="surface-corner-bed-content-stays-live" title="Surface — corner bed, content stays live"
        when={<>Inside a surface the spinner sits on a <b>24 bed bottom-right</b>, 8 in from the rect, never outside it: other splits, panes, the tab strip and the LNB are not covered. There is <b>no dim</b>: terminal cells, cursor and selection keep drawing at 0ms and are never faded or moved; output keeps arriving under it. Bottom-right keeps clear of the search bar (top-right) and of a prompt at the bottom-left; if a long line runs under the bed, the bed wins for 24 × 24 only. The move-target ring stays on the rect edge outside the bed, and in-surface overlays (search bar, popups) draw <b>above</b> the spinner. On a <b>native WebView</b> surface nothing is drawn inside: the OS paints the page above Tasty and hiding the WebView for a non-blocking spinner is not acceptable, so the <b>tab and LNB card carry the loading</b>. If the native overlay layer lands later, the same bed applies there.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              <Pair label="terminal" off={<TermSurface />} on={<TermSurface loading />} />
              <Pair label="terminal · search bar + move ring" off={<TermSurface search moveRing />} on={<TermSurface search moveRing loading />} />
              <Pair label="egui surface (markdown)" off={<DocSurface />} on={<DocSurface loading />} />
              <div>{cap("HTML · native WebView (intent)")}<WebViewSurface /></div>
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["place", "bottom-right · inset 8 · inside the rect"], ["bed", "24 · surface-raised · 1px border-default · radius 4"], ["terminal", "no dim, no motion on cells · PTY grid unchanged"], ["z-order", "content < move ring < spinner < search bar / popups"], ["WebView", "nothing in-surface; tab + LNB show it; never hide the WebView"]]}
          tokens={[{ tok: "--tasty-loading-overlay-surface-size", use: "→ spinner-size 16" }, { tok: "--tasty-loading-overlay-surface-bed-size", use: "→ size-24" }]} />
      </Spec>

      <Spec id="all-three-at-once-ui-scale-reduced-motion-timing" title="All three at once · UI scale · reduced motion · timing"
        when={<>One request can light the rail avatar (or LNB card), the tab and the surface together; each is switched independently by its caller. The overlay appears only if the request is still running after <span className="tok">--tasty-loading-overlay-delay</span> (200), so fast round trips never flash; it appears and disappears <b>without a fade</b> (the arc is the motion) and goes the instant loading ends, showing the content as it is at that moment. Rotation is Spinner's 900ms period. With <b>reduced motion</b> it is a <b>still arc</b> on its track, not three dots: dots read as a typing or ellipsis mark and do not fit in a 14 slot or around an initial. Sizes are tokens, so they follow <code>--tasty-ui-scale</code>; stroke stays 2px at 1×.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexDirection: "column", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              <Pair label="rail + tab + surface" off={<MiniWindow />} on={<MiniWindow loading />} />
              <div>{cap("reduced motion · still arc")}<MiniWindow loading rm /></div>
            </Themed>
          ))}
          <Themed>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>UI scale (zoom) · Mocha</span>
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", alignItems: "flex-start", flexWrap: "wrap" }}>
              {["1", "1.25", "1.5"].map((z) => (
                <div key={z}>{cap("×" + z)}
                  <div style={{ zoom: z, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                    <TabBed><LTab active loading busy width="var(--tasty-size-140)" /></TabBed>
                    <LnbBed><LRow active loading /></LnbBed>
                    <RailBed><LAv state="active" mirror notif loading /></RailBed>
                  </div>
                </div>
              ))}
            </div>
          </Themed>
        </Stage>
        <Meta
          specs={[["switching", "per placement, by its caller"], ["delay", "200 · loading-overlay-delay (same role as explorer-address-pending-delay)"], ["in / out", "no fade · gone immediately on finish or failure"], ["rotation", "spinner-duration 900"], ["reduced motion", "still arc + track (Spinner reducedMotion=\"arc\")"], ["scale", "token sizes × ui-scale; stroke 2px at 1×"], ["hidden placements", "off-screen tab / collapsed card not drawn → no repaint"]]}
          tokens={[{ tok: "--tasty-loading-overlay-delay", use: "→ motion-ui-fade 200" }, { tok: "--tasty-spinner-duration", use: "→ duration-900" }]} />
      </Spec>

      <Spec id="labels-and-tooltips" title="Labels and tooltips">
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", alignItems: "flex-start", gap: "var(--tasty-space-sm)", minHeight: 0 }}>
          <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>The overlay has no tooltip of its own (no hit-test). The existing tab / avatar / card tooltip gains a second line from the caller's key; screen readers get the same string as the overlay's label.</span>
        </Stage>
        <Meta
          specs={[["loading.generic", "en Loading… · ko 불러오는 중… · ja 読み込み中…"], ["loading.remote_attach", "en Attaching… · ko 연결하는 중… · ja 接続中…"], ["tooltip", "existing tooltip + second line (muted)"], ["a11y", "role=status on the overlay, label = same key"]]} />
        <Dont><b>Don't</b> write "Loading…" into the tab title or workspace name, and don't add a slot beside the title for it.</Dont>
      </Spec>
    </>
  );
}

export { GalleryLoadingOverlay, LTab, LRow, LAv, TermSurface, TabBed, LnbBed, RailBed, Themed, THEMES, mono, cap, LoadingOverlay, Arrow, OverflowStrip, DocSurface, WebViewSurface, MiniWindow };
