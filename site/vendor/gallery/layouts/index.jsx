import React from "react";
import { PageStates, AnnoTag, Views, View } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Kbd, Icon, IconButton, Button, Tab, TreeRow, StatusDot, Tag, Badge, BadgeGroup, Input, Switch, ListCtrl, DrillDown } from "../../components/index.js";
import { GalleryLoadingPrelayout } from "./loading-prelayout.jsx";
import { GalleryLoadingOverlay } from "./loading-overlay.jsx";
import { GalleryMoveSource } from "./move-source.jsx";
import { GalleryTabStrip } from "./tab-strip.jsx";
import { GalleryTitleBar } from "./title-bar.jsx";
import { Lnb as GalleryNotifications_Lnb } from "../parts/notifications.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont, GIcon } from "../parts/specimen.jsx";

const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty Gallery — Layouts. The structural shells: sidebar (full /
// collapsed rail), the pane tab strip, the 1-depth (list→detail) and
// 2-depth (tabs→sections→content) content idioms, and surface focus
// states. Turn on Specs to see width/height rails on each region.



const NAV = [
  { id: "sidebar", label: "Sidebar & rail" },
  { id: "tabbar", label: "Pane tab strip" },
  { id: "onedepth", label: "1-depth (list→detail)" },
  { id: "drilldown", label: "Drill-down (content swap)" },
  { id: "twodepth", label: "2-depth (sections)" },
  { id: "multitab", label: "Multi-tier tabs" },
  { id: "divider", label: "Divider" },
  { id: "surfaces", label: "Surface focus states" },
  { id: "attention", label: "Attention kinds" },
  { id: "movesource", label: "Move source" },
  { id: "loading", label: "Loading overlay" },
  { id: "notifsummary", label: "Notifications summary" },
  { id: "statusbar", label: "Workspace status bar" },
  { id: "titlebar", label: "Title bar" },
];

const ic = {
  folder: <GIcon d={<path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-7l-2-2H4a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1z" />} />,
  file: <GIcon d={<><path d="M14 3v4a1 1 0 0 0 1 1h4" /><path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z" /></>} />,
  term: <GIcon d={<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="m7 9 3 3-3 3M13 15h4" /></>} />,
  md: <GIcon d={<><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 15V9l2.5 3L12 9v6M16 9v4m0 0 2-2m-2 2-2-2" /></>} />,
  plus: <GIcon d={<path d="M12 5v14M5 12h14" />} />,
  branch: <GIcon d={<><circle cx="6" cy="6" r="2.5" /><circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="8" r="2.5" /><path d="M6 8.5v7M8.5 7H14a2 2 0 0 1 2 2v0" /></>} size={12} />,
  sun: <GIcon d={<><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M4.2 4.2l1.4 1.4m12.8 12.8 1.4 1.4M2 12h2m16 0h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4" /></>} size={12} />,
  settings: <GIcon d={<><circle cx="12" cy="12" r="3" /><path d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></>} />,
  plug: <GIcon d={<path d="M9 2v6M15 2v6M7 8h10v3a5 5 0 0 1-10 0V8zM12 16v6" />} />,
  tools: <GIcon d={<path d="M14.7 6.3a4 4 0 0 1-5.4 5.4L4 17v3h3l5.3-5.3a4 4 0 0 1 5.4-5.4l-2.7 2.7-2-2 2.7-2.7z" />} />,
  chevrons: <GIcon d={<path d="m11 17-5-5 5-5M18 17l-5-5 5-5" />} />,
  chevR: <GIcon d={<path d="m13 17 5-5-5-5M6 17l5-5-5-5" />} />,
  split: <GIcon d={<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M12 4v16" /></>} />,
  search: <GIcon d={<><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" /></>} />,
  lock: <GIcon d={<><rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></>} />,
  close: <GIcon d={<path d="M18 6 6 18M6 6l12 12" />} />,
};

// floating dimension badge (only visible in Specs mode via parent .anno)
function Dim({ children, style }) {
  return <AnnoTag style={style}>{children}</AnnoTag>;
}

const railHead = (txt) => (
  <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tasty-sidebar-section-heading-tracking)",
    color: "var(--tasty-text-muted)", padding: "12px 10px 6px" }}>{txt}</div>
);

// ── Sidebar (full) ──
function FullSidebar() {
  // Remote-mirror indicator = the info Tag (label + glyph), matching chrome.jsx WorkspaceRow.
  // Its own fill keeps the ink readable on every row state.
  const MirrorMark = () => (
    <Tag variant="info" caps title="Mirror of a remote workspace" style={{ flex: "none", color: "var(--tasty-tag-info-fg)" }}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round"
        style={{ width: "var(--tasty-workspace-mirror-icon-size)", height: "var(--tasty-workspace-mirror-icon-size)", display: "block" }}>
        <path d="M4 17l6-6-6-6" /><path d="M12 19h8" />
      </svg>
      remote
    </Tag>
  );
  return (
    <div className="anno" style={{ width: 212, flex: "none", display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", height: 360 }}>
      <Dim style={{ top: -9, left: "50%", transform: "translateX(-50%)" }}>212px</Dim>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "12px 12px 4px" }}>
        <img src={`${BASE}tasty-icon.png`} width="22" height="22" alt="" />
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: "var(--tasty-sidebar-wordmark-font-size)", letterSpacing: "var(--tasty-sidebar-wordmark-tracking)" }}>tasty<span data-audit-exempt="brand" style={{ color: "var(--tasty-brand-melon-flesh)" }}>.</span></span>
        <span style={{ marginLeft: "auto" }}><IconButton size="sm" aria-label="Collapse">{ic.chevrons}</IconButton></span>
      </div>
      {railHead("Workspaces")}
      <div style={{ padding: "0 6px", display: "flex", flexDirection: "column", gap: 1 }}>
        {[["tasty-core", "agent", true, 0, false], ["docs-site", "running", false, 3, false], ["data-etl", "running", false, 1, true], ["scratch", "idle", false, 0, false]].map(([n, st, a, notif, mir]) => (
          <div key={n} style={{ display: "flex", alignItems: "flex-start", gap: 9, padding: "6px 9px", borderRadius: "var(--tasty-radius-sm)",
            background: a ? "var(--tasty-surface-active)" : "transparent",
            boxShadow: a ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
            <span style={{ flex: "none", display: "inline-flex", alignItems: "center", height: "calc(13px * var(--tasty-line-height-ui))" }}>
              <StatusDot status={st === "agent" ? "running" : st} />
            </span>
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: "block", minWidth: 0, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: a ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>{n}</span>
              {mir && <span style={{ display: "block", marginTop: "var(--tasty-size-2)" }}><MirrorMark /></span>}
            </span>
            {notif > 0 && <Badge>{notif}</Badge>}
          </div>
        ))}
      </div>
      <div style={{ marginTop: "auto", padding: 10, borderTop: "1px solid var(--tasty-separator)", display: "flex", flexDirection: "column", gap: 2 }}>
        <Button variant="ghost" size="sm" block leadingIcon={ic.tools} style={{ justifyContent: "flex-start" }}>Tools</Button>
        <Button variant="ghost" size="sm" block leadingIcon={ic.plug} style={{ justifyContent: "flex-start" }}>Plugins</Button>
        <Button variant="ghost" size="sm" block leadingIcon={ic.settings} style={{ justifyContent: "flex-start" }}>Settings</Button>
      </div>
    </div>
  );
}

// 2026-10-12 — rail mirror chip: 12 circle, 1px workspace-mirror-chip-border (sky, full strength),
// bed workspace-mirror-chip-bg (bg-sidebar), 2px bg-sidebar halo, 8px remote glyph.
function MirrorChip() {
  return (
    <span title="Mirror of a remote workspace" style={{ position: "absolute", bottom: -1, right: -1, pointerEvents: "none", boxSizing: "border-box",
      display: "inline-flex", alignItems: "center", justifyContent: "center", width: "var(--tasty-workspace-mirror-chip-size)", height: "var(--tasty-workspace-mirror-chip-size)", borderRadius: "var(--tasty-radius-pill)",
      border: "var(--tasty-border-width) solid var(--tasty-workspace-mirror-chip-border)",
      background: "var(--tasty-workspace-mirror-chip-bg)", color: "var(--tasty-workspace-mirror-fg)", boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" style={{ width: "var(--tasty-size-8)", height: "var(--tasty-size-8)", display: "block" }}><path d="M4 17l6-6-6-6" /><path d="M12 19h8" /></svg>
    </span>
  );
}
// Static rail avatar with a forced state, for the 2026-10-12 rail-marks specimen.
function RailAv({ ch, state = "idle", notif, ring, mirror = true }) {
  const bg = state === "active" ? "var(--tasty-overlay-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent";
  return (
    <div style={{ position: "relative", display: "inline-flex" }}>
      <span style={{ width: "var(--tasty-size-36)", height: "var(--tasty-size-36)", display: "inline-flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--tasty-radius)", background: bg,
        fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-font-weight-bold)", fontSize: "var(--tasty-font-size-body)", color: state === "active" ? "var(--tasty-accent-primary-raised)" : "var(--tasty-text-muted)" }}>{ch}</span>
      {ring && <span style={{ position: "absolute", inset: 1, borderRadius: "var(--tasty-radius)", outline: "var(--tasty-status-dot-attached-ring-width) solid var(--tasty-accent-attached)", outlineOffset: "var(--tasty-status-dot-attached-ring-offset)", pointerEvents: "none" }} />}
      {notif && <span style={{ position: "absolute", top: -1, right: -1, pointerEvents: "none" }}><Badge dot variant="primary" style={{ boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }} /></span>}
      {mirror && <MirrorChip />}
    </div>
  );
}
function RailCatLine({ state = "idle", cap }) {
  return (
    <span style={{ width: "var(--tasty-size-36)", height: "var(--tasty-size-16)", display: "flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--tasty-radius-sm)",
      background: state === "pressed" ? "var(--tasty-overlay-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent" }}>
      {cap ? <SwitchCap n={cap} /> : <span style={{ width: "var(--tasty-rail-category-line-length)", height: "var(--tasty-rail-category-line-width)", borderRadius: "var(--tasty-radius-pill)", background: state === "idle" ? "var(--tasty-rail-category-line)" : "var(--tasty-rail-category-line-hover)" }} />}
    </span>
  );
}
function RailMarksBoard({ theme }) {
  const col = (cap, node) => (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: "var(--tasty-size-64)" }}>
      <div style={{ height: "var(--tasty-size-36)", display: "flex", alignItems: "center" }}>{node}</div>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)", textAlign: "center" }}>{cap}</span>
    </div>
  );
  const rail = (
    <div style={{ width: 52, flex: "none", display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-size-2)", padding: "var(--tasty-space-sm) 0", background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <RailCatLine /><RailAv ch="T" state="active" mirror={false} /><RailAv ch="S" mirror={false} />
      <RailCatLine state="hover" /><RailAv ch="E" /><RailAv ch="D" mirror={false} notif />
      <RailCatLine /><RailCatLine />
    </div>
  );
  return (
    <div {...(theme ? { "data-theme": theme } : {})} style={{ display: "flex", gap: "var(--tasty-space-lg)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-sidebar)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", flexWrap: "wrap", alignItems: "flex-start" }}>
      {rail}
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-lg)", minWidth: 0 }}>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{theme ? "Latte" : "Mocha"} — category line</span>
        <div style={{ display: "flex", gap: "var(--tasty-space-md)", flexWrap: "wrap" }}>
          {col("idle", <RailCatLine />)}{col("hover", <RailCatLine state="hover" />)}{col("pressed", <RailCatLine state="pressed" />)}{col("modifier held", <RailCatLine cap="2" />)}
        </div>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{theme ? "Latte" : "Mocha"} — mirror chip</span>
        <div style={{ display: "flex", gap: "var(--tasty-space-md)", flexWrap: "wrap" }}>
          {col("idle", <RailAv ch="E" />)}{col("active", <RailAv ch="T" state="active" />)}{col("hover", <RailAv ch="E" state="hover" />)}{col("+ notif", <RailAv ch="E" notif />)}{col("+ attached ring", <RailAv ch="E" ring />)}
        </div>
      </div>
    </div>
  );
}

// ── Sidebar (collapsed rail) ──
function CollapsedRail() {
  return (
    <div className="anno" style={{ width: 52, flex: "none", display: "flex", flexDirection: "column", alignItems: "center",
      background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", padding: "10px 0", gap: 4, height: 360 }}>
      <Dim style={{ top: -9, left: "50%", transform: "translateX(-50%)" }}>52px</Dim>
      <img src={`${BASE}tasty-icon.png`} width="24" height="24" alt="" style={{ marginBottom: 4 }} />
      <IconButton size="sm" aria-label="Expand">{ic.chevR}</IconButton>
      <div style={{ width: "var(--tasty-rail-flat-divider-length)", height: "var(--tasty-rail-category-line-width)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-rail-category-line)", margin: "4px 0" }} />
      <IconButton active aria-label="tasty-core"><span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13 }}>T</span></IconButton>
      <div style={{ position: "relative", display: "inline-flex" }}>
        <IconButton aria-label="docs-site"><span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13 }}>D</span></IconButton>
        <Badge dot variant="danger" style={{ position: "absolute", top: 1, right: 1, pointerEvents: "none" }} />
      </div>
      <div style={{ position: "relative", display: "inline-flex" }}>
        <IconButton aria-label="data-etl"><span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13 }}>E</span></IconButton>
        {/* mirror = local mirror of a remote instance — sky corner chip, bottom-right */}
        <MirrorChip />
      </div>
      <IconButton aria-label="New">{ic.plus}</IconButton>
      <div style={{ marginTop: "auto", display: "flex", flexDirection: "column", gap: 2 }}>
        <IconButton aria-label="Tools">{ic.tools}</IconButton>
        <IconButton aria-label="Plugins">{ic.plug}</IconButton>
        <IconButton aria-label="Settings">{ic.settings}</IconButton>
      </div>
    </div>
  );
}

// ── Sidebar with workspace categories (folders) ──
// Transient keycap for the Ctrl+Shift category quick-switch overlay — mirrors
// overlays-shared NumCap (same --tasty-switch-overlay-* tokens); this page
// doesn't load overlays-shared, so it's re-declared locally.
function SwitchCap({ n, active }) {
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
function catHeader(label, collapsed, cap, count) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", gap: 4,
      padding: "var(--tasty-sidebar-category-header-pad-y) var(--tasty-sidebar-category-header-pad-x)", paddingRight: 4, marginTop: 12, cursor: "pointer",
      background: "var(--tasty-sidebar-category-header-bg)",
      borderTop: "1px solid var(--tasty-sidebar-category-header-border)",
      borderBottom: "1px solid var(--tasty-sidebar-category-header-border)" }}>
      <span style={{ display: "inline-flex", width: 12, justifyContent: "center", color: "var(--tasty-sidebar-category-header-fg)",
        transform: collapsed ? "none" : "rotate(90deg)" }}>{ic.chevR}</span>
      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tasty-sidebar-section-heading-tracking)",
        fontWeight: "var(--tasty-sidebar-category-header-weight)", color: "var(--tasty-sidebar-category-header-fg)" }}>{label}</span>
      {cap && <SwitchCap n={cap.n} active={cap.active} />}
      {!cap && typeof count === "number" && (
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-sidebar-category-header-count-font-size)",
          color: "var(--tasty-sidebar-category-header-count-fg)", paddingRight: 6 }}>{count}</span>
      )}
      {!cap && typeof count !== "number" && <IconButton size="sm" aria-label={"New workspace in " + label} title={"New workspace in " + label}>{ic.plus}</IconButton>}
    </div>
  );
}
function catRows(rows) {
  return (
    <div>
      {rows.map(([n, st, sub, a, notif], i) => (
        <React.Fragment key={n}>
          {i > 0 && <div style={{ height: 1, background: "var(--tasty-separator)", margin: "0 0 0 32px" }} />}
          <div style={{ display: "flex", alignItems: "flex-start", gap: 9, padding: "7px 9px",
            background: a ? "var(--tasty-surface-active)" : "transparent", boxShadow: a ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
            <span style={{ height: 18, display: "inline-flex", alignItems: "center", flex: "none" }}><StatusDot status={st === "agent" ? "running" : st} /></span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 13, color: a ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>{n}</div>
              {sub && <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginTop: 1 }}>{sub}</div>}
            </div>
            {notif > 0 && <Badge>{notif}</Badge>}
          </div>
        </React.Fragment>
      ))}
    </div>
  );
}
function CategoryFullSidebar({ held }) {
  return (
    <div className="anno" style={{ width: 212, flex: "none", display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", height: 360 }}>
      <Dim style={{ top: -9, left: "50%", transform: "translateX(-50%)" }}>{held ? "Ctrl+Shift held" : "categories on"}</Dim>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "12px 12px 4px" }}>
        <img src={`${BASE}tasty-icon.png`} width="22" height="22" alt="" />
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: "var(--tasty-sidebar-wordmark-font-size)", letterSpacing: "var(--tasty-sidebar-wordmark-tracking)" }}>tasty<span data-audit-exempt="brand" style={{ color: "var(--tasty-brand-melon-flesh)" }}>.</span></span>
        <span style={{ marginLeft: "auto" }}><IconButton size="sm" aria-label="Collapse">{ic.chevrons}</IconButton></span>
      </div>
      {catHeader("Workspaces", false, held && { n: "1", active: true }, 2)}
      {catRows([["agents-prod", "running", null, true, 0], ["scratch", "idle", null, false, 0]])}
      {catHeader("Services", false, held && { n: "2" }, 3)}
      {catRows([["infra", "idle", "terraform + k8s", false, 0], ["api-gateway", "agent", "agent", false, 2], ["data-pipeline", "running", "spark · 4 nodes", false, 1]])}
      {catHeader("Archived", true, held && { n: "3" }, 0)}
    </div>
  );
}
function CategoryRail({ held }) {
  const line = (hover) => <div style={{ width: "var(--tasty-rail-category-line-length)", height: "var(--tasty-rail-category-line-width)", borderRadius: "var(--tasty-radius-pill)", background: hover ? "var(--tasty-rail-category-line-hover)" : "var(--tasty-rail-category-line)", margin: "3px 0" }} />;
  const boundary = (title, hover, cap) => (
    <button type="button" title={title} style={{ appearance: "none", border: 0, background: hover && !held ? "var(--tasty-overlay-hover)" : "transparent", cursor: "pointer", padding: "2px 0",
      borderRadius: "var(--tasty-radius-sm)", display: "flex", alignItems: "center", justifyContent: "center", minHeight: 22 }}>
      {held ? <SwitchCap n={cap.n} active={cap.active} /> : line(hover)}
    </button>
  );
  const av = (ch, active, dot) => (
    <div style={{ position: "relative", display: "inline-flex" }}>
      <IconButton active={active} aria-label={ch}><span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13 }}>{ch}</span></IconButton>
      {dot && <Badge dot variant="primary" style={{ position: "absolute", top: 1, right: 1, pointerEvents: "none" }} />}
    </div>
  );
  return (
    <div className="anno" style={{ width: 52, flex: "none", display: "flex", flexDirection: "column", alignItems: "center",
      background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", padding: "10px 0", gap: 4, height: 360 }}>
      <Dim style={{ top: -9, left: "50%", transform: "translateX(-50%)" }}>{held ? "Ctrl+Shift held" : "--- = category"}</Dim>
      <img src={`${BASE}tasty-icon.png`} width="24" height="24" alt="" style={{ marginBottom: 4 }} />
      <IconButton size="sm" aria-label="Expand">{ic.chevR}</IconButton>
      {boundary("Workspaces", false, { n: "1", active: true })}
      {av("A", true)}{av("S")}
      {boundary("Services", true, { n: "2" })}
      {av("I")}{av("A", false, true)}{av("D", false, true)}
      {boundary("Archived (empty)", false, { n: "3" })}
      <IconButton aria-label="New">{ic.plus}</IconButton>
      <div style={{ marginTop: "auto", display: "flex", flexDirection: "column", gap: 2 }}>
        <IconButton aria-label="Tools">{ic.tools}</IconButton>
        <IconButton aria-label="Settings">{ic.settings}</IconButton>
      </div>
    </div>
  );
}

function fakePane(label, focused, agent) {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column",
      background: focused ? "#000" : "var(--tasty-surface-terminal-unfocused-bg)",
      border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 10px", borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
        {agent ? <StatusDot status="agent" pulse label={label} /> : <StatusDot status={focused ? "running" : "idle"} label={label} />}
        <span style={{ marginLeft: "auto" }}><Tag>{focused ? "s_01HX" : "s_02KQ"}</Tag></span>
      </div>
      <div style={{ padding: "10px 12px", fontFamily: "var(--tasty-font-mono)", fontSize: 12.5,
        color: focused ? "var(--tasty-color-neutral-1100)" : "var(--tasty-surface-terminal-unfocused-fg)", lineHeight: 1.5 }}>
        <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> <span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-blue)" }}>main</span></div>
        <div style={{ display: "flex", alignItems: "center", marginTop: 2 }}>
          <span style={{ color: agent ? "var(--tasty-accent-agent-raised)" : "var(--tasty-color-mauve)" }} data-audit-exempt="ansi-content">❯&nbsp;</span>
          {focused && <span style={{ width: 8, height: 15, background: "var(--tasty-color-neutral-1100)", display: "inline-block", animation: "tasty-blink 1.1s step-end infinite" }} />}
        </div>
      </div>
    </div>
  );
}

// Markdown surface stand-in — address bar + prose, for the mixed-split specimen.
function fakeDocPane() {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column",
      background: "var(--tasty-surface-markdown-unfocused-bg)", border: "1px solid var(--tasty-separator)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, height: 36, flex: "none", padding: "0 8px",
        background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
        <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 4, height: 28, padding: "0 8px",
          background: "var(--tasty-input-bg)", border: "1px solid var(--tasty-input-border)", borderRadius: "var(--tasty-radius)",
          fontFamily: "var(--tasty-font-mono)", fontSize: 11.5, color: "var(--tasty-text-secondary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>~/tasty/notes/split-layout.md</div>
        <Tag>s_05MD</Tag>
      </div>
      <div style={{ padding: "14px 18px", lineHeight: 1.6 }}>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 17, color: "var(--tasty-text-primary)", marginBottom: 4 }}>Split layout</div>
        <div style={{ fontSize: 12.5, color: "var(--tasty-text-secondary)" }}>
          A pane group holds surfaces of different kinds. Only one of them has focus.</div>
      </div>
    </div>
  );
}

// ── Occupancy / completion border panes — one surface border channel, color-only ──
function occPane(kind) {
  const cfg = {
    soft: { color: "var(--tasty-surface-occupied-soft-border)", width: "var(--tasty-surface-occupied-border-width)",
            label: "occupied · soft", sub: "agent holds — writable", who: "agent" },
    hard: { color: "var(--tasty-surface-occupied-hard-border)", width: "var(--tasty-surface-occupied-border-width)",
            label: "occupied · hard", sub: "readonly · mirror-observe", who: "remote", readonly: true },
    done: { color: "var(--tasty-surface-highlight-done-border)", width: "var(--tasty-surface-highlight-done-width)",
            label: "completed", sub: "clears on focus", who: "done" },
  }[kind];
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-surface-terminal-focused-bg)",
      border: `${cfg.width} solid ${cfg.color}`, borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "6px 10px",
        borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
        {cfg.readonly && <span style={{ display: "inline-flex", color: cfg.color, width: 14, height: 14 }}>{ic.lock}</span>}
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, fontWeight: 600, color: "var(--tasty-text-primary)" }}>{cfg.label}</span>
        <span style={{ fontSize: 10, color: "var(--tasty-text-muted)" }}>· {cfg.sub}</span>
        {cfg.readonly && <span style={{ marginLeft: "auto" }}><IconButton size="sm" aria-label="Force detach">{ic.close}</IconButton></span>}
      </div>
      <div style={{ padding: "10px 12px", fontFamily: "var(--tasty-font-mono)", fontSize: 12.5,
        color: cfg.readonly ? "var(--tasty-surface-terminal-unfocused-fg)" : "var(--tasty-color-neutral-1100)", lineHeight: 1.5 }}>
        <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> <span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-blue)" }}>main</span></div>
        <div style={{ display: "flex", alignItems: "center", marginTop: 2 }}>
          <span style={{ color: cfg.who === "agent" ? "var(--tasty-accent-agent)" : "var(--tasty-color-mauve)" }} data-audit-exempt="ansi-content">❯&nbsp;</span>
          {cfg.who === "agent" && <span style={{ color: "var(--tasty-text-secondary)" }}>running tests…</span>}
          {cfg.who === "done" && <span style={{ color: "var(--tasty-text-secondary)" }}>build passed ✓</span>}
        </div>
      </div>
    </div>
  );
}

// ── Attention kinds — scale, badges, rail dot, tab/border priority ──
const ATT_KINDS = [
  { id: "needs-input", name: "NeedsInput", rank: 30, tok: "--tasty-attention-needs-input", role: "accent-warning", what: "blocked — waiting on the human", live: true },
  { id: "approval", name: "Approval", rank: 20, tok: "(reserved)", role: "accent-agent", what: "an agent asks permission", live: false },
  { id: "completion", name: "Completion", rank: 10, tok: "--tasty-attention-completion", role: "accent-primary", what: "task finished — clears on focus", live: true },
];
function AttentionScale() {
  const swatch = (k) => k.live ? `var(${k.tok})` : "transparent";
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
      <div style={{ display: "grid", gridTemplateColumns: "44px 120px 1fr 150px 60px", gap: 10, padding: "0 8px 6px",
        fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tasty-sidebar-section-heading-tracking)", color: "var(--tasty-text-muted)" }}>
        <span>rank</span><span>kind</span><span>meaning</span><span>role borrowed</span><span>fill</span>
      </div>
      {[{ id: "error", name: "Error", rank: 40, tok: "(reserved)", role: "accent-danger", what: "failed and unrecovered", live: false }].concat(ATT_KINDS).map((k) => (
        <div key={k.id} style={{ display: "grid", gridTemplateColumns: "44px 120px 1fr 150px 60px", gap: 10, alignItems: "center",
          padding: "7px 8px", background: k.live ? "var(--tasty-surface-raised)" : "transparent",
          border: "var(--tasty-border-width) solid " + (k.live ? "transparent" : "var(--tasty-separator)"),
          borderRadius: "var(--tasty-radius-sm)" }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>{k.rank}</span>
          <span style={{ fontSize: 13, color: "var(--tasty-text-primary)" }}>{k.name}{!k.live && <span style={{ fontSize: 10, color: "var(--tasty-text-muted)" }}> · reserved</span>}</span>
          <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)" }}>{k.what}</span>
          <span className="tok" style={{ fontSize: 11 }}>--tasty-{k.role}</span>
          <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ width: 18, height: 18, borderRadius: "var(--tasty-radius-sm)", background: swatch(k),
              border: k.live ? "none" : "1px dashed var(--tasty-border-strong)" }} />
            {k.live && <Badge variant={k.id === "needs-input" ? "warning" : "primary"}>3</Badge>}
          </span>
        </div>
      ))}
    </div>
  );
}

function AttRow({ name, status, ni, done, active }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 9, padding: "6px 9px", borderRadius: "var(--tasty-radius-sm)",
      background: active ? "var(--tasty-surface-active)" : "transparent", boxShadow: active ? "inset var(--tasty-selection-edge-width) 0 0 var(--tasty-accent-primary)" : "none" }}>
      <StatusDot status={status} />
      <span style={{ flex: 1, minWidth: 0, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)" }}>{name}</span>
      <BadgeGroup>
        {ni != null && <Badge variant="warning">{ni}</Badge>}
        {done != null && <Badge variant="primary">{done}</Badge>}
      </BadgeGroup>
    </div>
  );
}
function AttentionRows() {
  const cases = [
    ["Completion only", { name: "docs-site", status: "running", done: 3 }],
    ["NeedsInput only — same slot", { name: "data-etl", status: "running", ni: 1 }],
    ["Both — needs-input leads", { name: "tasty-core", status: "idle", ni: 2, done: 5 }],
    ["Overflow — 99+ on both", { name: "monorepo", status: "running", ni: "99+", done: "99+" }],
    ["Quiet", { name: "scratch", status: "idle" }],
  ];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, padding: 12, background: "var(--tasty-bg-sidebar)" }}>
      {cases.map(([cap, props]) => (
        <div key={cap} style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <span style={{ width: 190, flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>{cap}</span>
          <div style={{ width: 212, flex: "none" }}><AttRow {...props} /></div>
        </div>
      ))}
    </div>
  );
}

function RailAvatar({ ch, dot, busy }) {
  return (
    <div style={{ position: "relative", display: "inline-flex" }}>
      <IconButton aria-label={ch}><span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 13 }}>{ch}</span></IconButton>
      {(dot || busy) && (
        <span aria-hidden style={{ position: "absolute", top: 1, right: 1, pointerEvents: "none",
          width: "var(--tasty-status-dot-size-compact)", height: "var(--tasty-status-dot-size-compact)", borderRadius: "var(--tasty-radius-pill)",
          background: dot ? `var(--tasty-status-dot-${dot})` : "var(--tasty-status-dot-success)",
          boxShadow: "0 0 0 var(--tasty-status-dot-ring-width) var(--tasty-status-dot-ring)" }} />
      )}
    </div>
  );
}
function AttentionRail() {
  const cases = [
    ["busy only", "B", null, true, "running — green"],
    ["completion", "C", "completion", false, "finished — blue"],
    ["needs-input", "N", "needs-input", false, "blocked — yellow"],
    ["completion + busy", "CB", "completion", true, "blue wins"],
    ["needs-input + completion", "NC", "needs-input", false, "yellow wins"],
    ["all three", "A", "needs-input", true, "yellow wins"],
  ];
  return (
    <div style={{ display: "flex", gap: 0, padding: "14px 12px", background: "var(--tasty-bg-sidebar)" }}>
      {cases.map(([cap, ch, dot, busy, res]) => (
        <div key={cap} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", gap: 8,
          padding: "0 6px", borderRight: "1px solid var(--tasty-separator)" }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)", textAlign: "center", minHeight: 24 }}>{cap}</span>
          <RailAvatar ch={ch.slice(0, 1)} dot={dot} busy={busy} />
          <span style={{ fontSize: 10, color: "var(--tasty-text-secondary)", textAlign: "center" }}>{res}</span>
        </div>
      ))}
    </div>
  );
}

function AttTab({ label, kind, active }) {
  const fg = kind === "needs-input" ? "var(--tasty-tab-fg-needs-input)"
    : kind === "completion" ? "var(--tasty-tab-fg-completion)"
    : active ? "var(--tasty-tab-fg-active)" : "var(--tasty-tab-fg)";
  return (
    <div style={{ position: "relative", width: "var(--tasty-tab-width)", height: "var(--tasty-tab-height)", flex: "none",
      display: "flex", alignItems: "center", gap: 6, padding: "0 var(--tasty-space-sm)",
      background: active ? "var(--tasty-tab-bg-active)" : "var(--tasty-tab-bg)",
      borderRight: "1px solid var(--tasty-tab-separator)" }}>
      {active && <span style={{ position: "absolute", top: 0, left: 0, right: 0, height: "var(--tasty-tab-indicator-width)", background: "var(--tasty-tab-indicator)" }} />}
      <span style={{ display: "inline-flex", width: 13, height: 13, color: fg }}>{ic.term}</span>
      <span style={{ flex: 1, minWidth: 0, fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: fg }}>{label}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>
        {kind === "needs-input" ? "blocked" : kind === "completion" ? "done" : active ? "active" : "rest"}
      </span>
    </div>
  );
}

function attPane(kind) {
  const cfg = {
    "needs-input": { color: "var(--tasty-surface-highlight-input-border)", width: "var(--tasty-surface-highlight-input-width)",
      label: "needs input", sub: "blocked — rank 30", line: "Overwrite build/? [y/N]" },
    occupied: { color: "var(--tasty-surface-occupied-soft-border)", width: "var(--tasty-surface-occupied-border-width)",
      label: "occupied · soft", sub: "held — below needs-input", line: "running tests…" },
    completion: { color: "var(--tasty-surface-highlight-done-border)", width: "var(--tasty-surface-highlight-done-width)",
      label: "completed", sub: "rank 10 — clears on focus", line: "build passed ✓" },
  }[kind];
  return (
    <div key={kind} style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-surface-terminal-focused-bg)",
      border: `${cfg.width} solid ${cfg.color}`, borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "6px 10px",
        borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, fontWeight: 600, color: "var(--tasty-text-primary)" }}>{cfg.label}</span>
        <span style={{ fontSize: 10, color: "var(--tasty-text-muted)" }}>· {cfg.sub}</span>
      </div>
      <div style={{ padding: "10px 12px", fontFamily: "var(--tasty-font-mono)", fontSize: 12.5, color: "var(--tasty-color-neutral-1100)", lineHeight: 1.5 }}>
        <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> <span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-blue)" }}>main</span></div>
        <div style={{ marginTop: 2 }}><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-mauve)" }}>❯&nbsp;</span><span style={{ color: "var(--tasty-text-secondary)" }}>{cfg.line}</span></div>
      </div>
    </div>
  );
}

// ── Drill-down (content-swap) demo — mirrors Settings › Keybindings › Preset ──
const DD_PRESETS = [
  { id: "default", name: "Default", desc: "Tasty stock bindings", rows: [
    ["Copy", "Ctrl+Shift+C", "Ctrl+Shift+C"], ["Paste", "Ctrl+Shift+V", "Ctrl+Shift+V"],
    ["New tab", "Ctrl+T", "Ctrl+T"], ["Find", "Ctrl+F", "Ctrl+F"] ] },
  { id: "mac", name: "Mac", desc: "⌘-based, native-app muscle memory", rows: [
    ["Copy", "Ctrl+Shift+C", "⌘C"], ["Paste", "Ctrl+Shift+V", "⌘V"],
    ["New tab", "Ctrl+T", "⌘T"], ["Find", "Ctrl+F", "⌘F"] ] },
  { id: "vim", name: "Vim", desc: "modal, hjkl pane motions", rows: [
    ["Copy", "Ctrl+Shift+C", "y"], ["Paste", "Ctrl+Shift+V", "p"],
    ["New tab", "Ctrl+T", ":tabnew"], ["Find", "Ctrl+F", "/"] ] },
];
function DrillDownDemo({ initial = null }) {
  const [active, setActive] = React.useState("default");
  const [view, setView] = React.useState(initial ? "detail" : "list");
  const [selId, setSelId] = React.useState(initial);
  const sel = DD_PRESETS.find((p) => p.id === selId) || null;
  const isActiveSel = sel && sel.id === active;
  const items = DD_PRESETS.map((p) => ({ id: p.id, label: p.name, description: p.desc,
    trailing: p.id === active ? <Tag variant="success" dot>Active</Tag> : null }));
  const head = { fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
    letterSpacing: "var(--tasty-sidebar-section-heading-tracking)", color: "var(--tasty-text-muted)", padding: "0 12px 6px", borderBottom: "1px solid var(--tasty-separator)" };
  const cell = { padding: "6px 12px", borderBottom: "1px solid var(--tasty-separator)", fontSize: 13, display: "flex", alignItems: "center" };
  return (
    <div style={{ width: 560, height: 320, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
      border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ flex: 1, minHeight: 0 }}>
        <DrillDown
          view={view}
          title={sel ? sel.name + " preset" : ""}
          onBack={() => setView("list")}
          actions={<Button variant="primary" size="sm" disabled={isActiveSel} onClick={() => sel && setActive(sel.id)}>{isActiveSel ? "Applied" : "Apply"}</Button>}
          detail={sel && (
            <div style={{ padding: 16 }}>
              <div style={{ display: "grid", gridTemplateColumns: "minmax(0,1.6fr) 1fr 1fr" }}>
                <div style={{ ...head, textAlign: "left" }}>Action</div>
                <div style={head}>Current</div>
                <div style={head}>{sel.name}</div>
                {sel.rows.map(([a, cur, next]) => {
                  const ch = cur !== next;
                  return (
                    <React.Fragment key={a}>
                      <div style={{ ...cell, color: "var(--tasty-text-secondary)" }}>{a}</div>
                      <div style={{ ...cell, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>{cur}</div>
                      <div style={{ ...cell, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: ch ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: ch ? 600 : 400 }}>{next}</div>
                    </React.Fragment>
                  );
                })}
              </div>
            </div>
          )}
        >
          <div style={{ padding: "12px 16px" }}>
            <ListCtrl items={items} selectedId={active} onSelect={(id) => { setSelId(id); setView("detail"); }} />
          </div>
        </DrillDown>
      </div>
      {/* the surrounding modal footer — Apply lives up in the back bar, clear of this */}
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, padding: "8px 12px", borderTop: "1px solid var(--tasty-separator)" }}>
        <Button variant="ghost" size="sm">Cancel</Button><Button variant="primary" size="sm">Save</Button>
      </div>
    </div>
  );
}

function Layouts() {
  return (
    <>
      {/* SIDEBAR */}
      <Section id="sidebar" title="Sidebar & rail">
        <Spec id="full-sidebar-collapsed-rail" title="Full sidebar ↔ collapsed rail"
          when={<>The window's left navigation has two states. <b>Full (212px)</b> shows the wordmark, workspace list, and a footer of Tools / Plugins / Settings. <b>Collapsed (52px)</b> drops to an icon rail — workspaces become single-letter IconButtons, footer actions stack at the bottom. Same actions, same order, both states. Each workspace row is just <b>status dot + name + notification</b>: every row is a workspace, so a per-row folder icon carried no information, and a tab count is low-signal noise — the dot (idle / running / agent) and an unread badge are what actually vary. A workspace that is a <b>local mirror of a remote instance</b> gets a small <b>sky remote-link glyph before its name</b> (a corner chip on the rail) — a separate axis from the dot, so the dot stays execution-only.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex" }}>
              <FullSidebar />
              <CollapsedRail />
              <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", minWidth: 120 }} />
            </div>
          </Stage>
          <Meta
            specs={[["full width", "212px"], ["rail width", "52px"], ["row anatomy", "dot · name / [remote pill] · notification"], ["row height", <>~30px, <span className="tok">--tasty-radius-sm</span></>], ["active row", <><span className="tok">--tasty-surface-active</span> + 2px accent bar</>], ["mirror", "sky remote glyph before name / rail corner chip"]]}
            tokens={[{ tok: "--tasty-accent-success", use: "running dot", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-agent", use: "agent dot", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-accent-danger", use: "notification badge", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-workspace-mirror-fg", use: "mirror glyph (→ accent-remote / sky)", color: "var(--tasty-accent-remote)" }, { tok: "--tasty-surface-active", use: "active row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "active left bar", color: "var(--tasty-accent-primary-raised)" }]} />
          <Dont><b>Don't</b> give every row the same leading icon — when one kind fills the list, the icon is decoration, not information. Lead with the <b>status dot</b>, which varies per row, and let an unread <b>badge</b> earn the trailing slot instead of a static count.</Dont>
          <Note>Active selection now reads through <b>surface fill + a 2px accent left bar</b> (the folder icon used to carry that accent). Footer order is fixed: <b>Tools → Plugins → Settings</b>, bottom-aligned; in the rail they become 28px IconButtons in the same order. <b>Mirror</b> (remote-linked) moved <b>off the status dot</b> onto its own leading glyph — the dot no longer carries a sky "remote" color, only execution (running / idle / agent); the glyph never collides with the attached <b>lavender ring</b> or the notification badge.</Note>
        </Spec>

        <Spec id="workspace-categories-sidebar-folders" title="Workspace categories (sidebar folders)"
          when={<>An opt-in grouping (<b>Settings › General › Workspace categories (folders)</b>, default off). The flat list splits into <b>collapsible category sections</b>: a disclosure header (chevron) over that category's workspace rows. The reserved <b>normal</b> category is always first and keeps the plain <b>"Workspaces"</b> label — it can't be renamed, deleted, or reordered. <b>Empty categories</b> (e.g. Archived) show the header only. In the <b>rail</b>, categories aren't labelled: each boundary becomes a clickable <b>horizontal <code>---</code> button</b>, with that category's avatars below it. Collapse state is per-category and shared across both. All category management is via <b>right-click</b> (no dedicated buttons) — see Overlays › Popups.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex" }}>
              <CategoryFullSidebar />
              <CategoryRail />
              <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", minWidth: 120 }} />
            </div>
          </Stage>
          <Meta
            specs={[["header", <>band — <code>bg-app</code> face + hairline top/bottom, label <code>text-secondary</code> bold</>], ["count", "trailing workspace count; hidden on hover (+ takes the gutter)"], ["normal", "reserved · first · label “Workspaces”"], ["empty category", "header only, no rows"], ["rail boundary", <><code>---</code> full-width button</>], ["collapse", "per-category, shared full ↔ rail"], ["reorder", "drag row to another category"], ["gate", "Settings toggle (default off)"]]}
            tokens={[{ tok: "--tasty-sidebar-category-header-bg", use: "header band face (one tier below the sidebar)", color: "var(--tasty-sidebar-category-header-bg)" }, { tok: "--tasty-sidebar-category-header-fg", use: "label + chevron", color: "var(--tasty-sidebar-category-header-fg)" }, { tok: "--tasty-sidebar-category-header-count-fg", use: "trailing count", color: "var(--tasty-sidebar-category-header-count-fg)" }, { tok: "--tasty-separator", use: <>band hairlines + list rules + <code>---</code> line</>, color: "var(--tasty-separator)" }, { tok: "--tasty-surface-active", use: "active row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "active left bar", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>Backend invariants honoured: <code>normal</code> reserved &amp; pinned top; category order = section order; <code>collapsed</code> persisted in <code>layout.json</code>; empty categories allowed (no auto-delete). Toggle off → the flat single "Workspaces" list above, unchanged.</Note>
        </Spec>

        <Spec id="rail-marks-category-line-and-mirror-chip" title="Rail marks — category line and mirror chip"
          when={<>Two rail marks were too faint on <code>bg-sidebar</code>. <b>Category <code>---</code> line</b>: drawn in the idle avatar glyph ink (<span className="tok">--tasty-rail-category-line</span> → text-muted) at <b>2px</b> (<span className="tok">--tasty-rail-category-line-width</span> → focus-ring-width), 24 long with round ends. One colour for every category, including the one holding the active workspace: the active avatar already carries the accent, and the line marks a boundary, not a selection. Hover and pressed lift the line to text-secondary on the usual overlay bed. The modifier keycap that replaces it is unchanged. The flat-mode top divider uses the same line (28 long) so both rail modes look the same. <b>Mirror chip</b>: same 12 circle and 8 glyph, plus a full-strength <b>1px sky edge</b> (<span className="tok">--tasty-workspace-mirror-chip-border</span> → workspace-mirror-fg). The bed stays <code>bg-sidebar</code> so the glyph keeps its contrast, and the 2px halo still cuts it out of an active fill.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <RailMarksBoard /><RailMarksBoard theme="latte" />
          </Stage>
          <Meta
            specs={[["line", "24 × 2 · radius-pill · centred in the 36 × 16 button"], ["line idle", "rail-category-line → text-muted"], ["line hover / pressed", "rail-category-line-hover → text-secondary · bed overlay-hover / overlay-active"], ["active category", "same colour (no accent)"], ["flat divider", "28 × 2 · rail-category-line"], ["chip", "12 circle · 1px edge · 8 glyph · 2px bg-sidebar halo"], ["chip edge", "workspace-mirror-chip-border → workspace-mirror-fg (accent-remote)"], ["contrast", "line ≥ 3:1 non-text (Mocha ≈ 8:1, Latte ≈ 4.9:1 on bg-sidebar)"]]}
            tokens={[{ tok: "--tasty-rail-category-line-length", use: "line 24 (2026-10-14)" }, { tok: "--tasty-rail-flat-divider-length", use: "flat top divider 28" }, { tok: "--tasty-workspace-mirror-chip-size", use: "mirror chip 12" }, { tok: "--tasty-rail-category-line", use: "idle line", color: "var(--tasty-rail-category-line)" }, { tok: "--tasty-rail-category-line-hover", use: "hover / pressed line", color: "var(--tasty-rail-category-line-hover)" }, { tok: "--tasty-rail-category-line-width", use: "2 — exception to the 1px rule" }, { tok: "--tasty-workspace-mirror-chip-border", use: "chip edge", color: "var(--tasty-workspace-mirror-chip-border)" }, { tok: "--tasty-workspace-mirror-chip-bg", use: "chip bed", color: "var(--tasty-workspace-mirror-chip-bg)" }]} />
          <Note>The 2px line is a mark at avatar-glyph weight, not a box edge, so the 1px border rule still holds for borders. Move and attach-refusal chips keep their current look: they are short-lived states with their own coloured glyph, so no edge is added for now.</Note>
        </Spec>

        <Spec id="category-quick-switch-ctrl-shift-held" title="Category quick-switch — Ctrl+Shift held"
          when={<>A keyboard axis over the folders: holding <b>Ctrl+Shift</b> (rebindable) previews <b>Ctrl+Shift+1–9 / 0</b> to jump <b>between categories</b>, the way <b>Alt</b> jumps between workspaces and <b>Ctrl</b> between tabs. Each category gets a switch keycap: in the <b>full</b> sidebar it's <b>right-aligned on the header</b> (the chevron is kept — it carries collapse state and its auto-expand rotation); in the <b>rail</b> it sits on the <code>---</code> boundary. The reserved <b>normal</b> category is <b>1</b>. This and the workspace overlay are <b>modifier-exclusive</b>, so rows keep their status dots while headers show keycaps. See Overlays › Popups → Switch-number overlay for the full spec.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex" }}>
              <CategoryFullSidebar held />
              <CategoryRail held />
              <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", minWidth: 120 }} />
            </div>
          </Stage>
          <Meta
            specs={[["trigger", <>Ctrl+Shift held (rebindable)</>], ["full placement", "trailing keycap on header — chevron kept"], ["rail placement", <>keycap on the <code>---</code> boundary</>], ["range", "1–9 + 0; 11th category on: none"], ["reserved", "normal (“Workspaces”) = 1"], ["auto-expand", "collapsed target rotates open on switch"], ["last-active", "lands on category's last-focused ws"]]}
            tokens={[{ tok: "--tasty-switch-overlay-bg", use: "keycap fill", color: "var(--tasty-switch-overlay-bg)" }, { tok: "--tasty-switch-overlay-active-bg", use: "active category", color: "var(--tasty-switch-overlay-active-bg)" }, { tok: "--tasty-switch-overlay-border", use: "keycap edge", color: "var(--tasty-switch-overlay-border)" }, { tok: "--tasty-surface-active", use: "landed ws row", color: "var(--tasty-surface-active)" }]} />
          <Note>Reuses the switch-overlay keycap and all <span className="tok">--tasty-switch-overlay-*</span> tokens — no new tokens. <b>Auto-expand</b>: switching to a collapsed category opens it (chevron rotates, rows reveal) and persists <code>collapsed:false</code>. <b>Last-active</b>: the landed workspace uses the ordinary active treatment (<span className="tok">--tasty-surface-active</span> + 2px accent bar) — no separate cue.</Note>
        </Spec>
      </Section>

      {/* TAB BAR */}
      <Section id="tabbar" title="Pane tab strip">
        <Spec id="tab-strip-fixed-24px-row" title="Tab strip — fixed 24px row"
          when={<>Sits at the top of a pane group on <b>--tasty-bg-sidebar</b>. Tabs are <b>24px × 150px</b>; the active tab adopts the panel color with an accent top bar. A <code>+</code> IconButton follows the tabs; pane actions (split, search) pin to the right.</>}>
          <Stage variant="tight" grid>
            <div className="anno" style={{ display: "flex", alignItems: "stretch", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
              <Dim style={{ top: 28, left: 0 }}>24px tall</Dim>
              <div style={{ display: "flex" }}>
                <Tab label="build.sh" icon={ic.term} active />
                <Tab label="README.md" icon={ic.md} status="busy" />
                <Tab label="server.log" icon={ic.term} />
              </div>
              <div style={{ display: "flex", alignItems: "center", padding: "0 6px" }}><IconButton size="sm" aria-label="New tab">{ic.plus}</IconButton></div>
              <div style={{ flex: 1 }} />
              <div style={{ display: "flex", alignItems: "center", padding: "0 8px", gap: 2 }}>
                <IconButton size="sm" aria-label="Split">{ic.split}</IconButton>
                <IconButton size="sm" aria-label="Search">{ic.search}</IconButton>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["tab height", <>24px <span className="tok">--tasty-control-height-tab</span></>], ["tab width", <>150px <span className="tok">--tasty-tab-width</span></>], ["strip fill", <span className="tok">--tasty-bg-sidebar</span>], ["active", "panel fill + accent top bar"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "strip", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-bg-panel", use: "active tab", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-accent-primary", use: "active bar", color: "var(--tasty-accent-primary-raised)" }]} />
        </Spec>
        {GalleryTabStrip ? <GalleryTabStrip /> : null}
      </Section>

      {/* 1-DEPTH */}
      <Section id="onedepth" title="1-depth — list → detail (Plugins / Tools idiom)">
        <Spec id="one-sidebar-one-detail-pane" title="One sidebar, one detail pane"
          when={<>The simplest content layout: a <b>fixed-width list</b> (a TreeRow list of items) feeding a <b>flexible detail pane</b>. Use it for plugins, tools, port lists, file handlers — anything that's "pick one from a list, see its detail." No top tabs.</>}>
          <Stage variant="tight" grid>
            <div className="anno" style={{ display: "flex", height: 320, background: "var(--tasty-bg-panel)" }}>
              <Dim style={{ top: 6, left: 86 }}>~200px list</Dim>
              <div style={{ width: 200, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", padding: 6 }}>
                <div style={{ padding: 4, marginBottom: 4 }}><Input block icon={ic.search} placeholder="Filter plugins…" /></div>
                {[["git-helper", true], ["ai-review", false], ["docker", false], ["k8s-lens", false]].map(([n, a]) => (
                  <div key={n} style={{ display: "flex", alignItems: "center", gap: 7, padding: "6px 8px", borderRadius: "var(--tasty-radius-sm)",
                    fontSize: 13, color: a ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", background: a ? "var(--tasty-surface-active)" : "transparent" }}>
                    <span style={{ width: 6, height: 6, borderRadius: "50%", background: "var(--tasty-accent-agent)" }} />{n}
                  </div>
                ))}
              </div>
              <div style={{ flex: 1, padding: 18, display: "flex", flexDirection: "column", gap: 12, minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontSize: 15, fontWeight: 600 }}>git-helper</span>
                  <Tag variant="agent">plugin</Tag><Tag variant="success" dot>running</Tag>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
                  <span style={{ width: 130, fontSize: 13, color: "var(--tasty-text-secondary)" }}>Enabled</span><Switch defaultChecked />
                </div>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)" }}>Permissions</div>
                <div style={{ display: "flex", gap: 6 }}><Tag>clipboard</Tag><Tag>fs:read</Tag><Tag>ipc:git.*</Tag></div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["list width", "~200px fixed"], ["detail", "flex, 18px pad"], ["list fill", <span className="tok">--tasty-bg-sidebar</span>], ["selected", <span className="tok">--tasty-surface-active</span>]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "list", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-bg-panel", use: "detail", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-surface-active", use: "selected", color: "var(--tasty-surface-active)" }, { tok: "--tasty-space-lg", use: "detail pad" }]} />
        </Spec>
      </Section>

      {/* DRILL-DOWN */}
      <Section id="drilldown" title="Drill-down — content swap (list ⇄ detail)">
        <Spec id="one-area-swapped-list-detail-back" title="One area, swapped — list → detail → back"
          when={<>When the model is "one <b>full-width list</b>, pick an item, see its <b>detail</b>, go back" — and a side-by-side split would starve both — the content area <b>swaps in place</b>. The <b>list view</b> (a full-width <span className="ic">ListCtrl</span>) becomes a <b>detail view</b> the moment you select a row; a pinned <b>back bar</b> (← + title) returns you. This is a generic <span className="ic">DrillDown</span> layout — its first home is <b>Settings › Keybindings › Preset</b> (preset list → diff preview), replacing the old cramped 120px-list + right-preview split. Select a preset below, then press <b>←</b>.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", justifyContent: "center", padding: 12 }}><DrillDownDemo /></div>
          </Stage>
          <Views><View name="detail" label="detail"><DrillDownDemo initial={DD_PRESETS[1].id} /></View></Views>
          <Meta
            specs={[["views", "list (full width) ⇄ detail (full width)"], ["back bar", <>36px, <span className="tok">--tasty-drilldown-backbar-height</span> · bottom hairline</>], ["back button", <>ghost <span className="ic">IconButton</span> · <span className="ic">chevronLeft</span></>], ["title", "detail subject, next to ←"], ["actions slot", "right of the back bar — holds Apply"], ["transition", "instant (reduced-motion aware opt-in fade)"], ["scroll", "detail body scrolls; back bar pinned"]]}
            tokens={[
              { tok: "--tasty-drilldown-backbar-border", use: "back-bar bottom rule", color: "var(--tasty-separator)" },
              { tok: "--tasty-drilldown-title-fg", use: "detail title", color: "var(--tasty-text-primary)" },
              { tok: "--tasty-surface-active", use: "selected list row", color: "var(--tasty-surface-active)" },
              { tok: "--tasty-accent-primary", use: "Apply / active bar", color: "var(--tasty-accent-primary-raised)" },
            ]} />
          <Do><b>Do</b> put the detail's own action (<b>Apply</b>) in the <b>back-bar actions slot</b> — it stays clear of a surrounding modal footer's <b>Cancel / Save</b>, so the two button rows never collide. <b>Apply</b> is <b>disabled</b> for the already-active item (nothing to change).</Do>
          <Dont><b>Don't</b> reach for drill-down when items are few and detail is small — a plain <b>1-depth list→detail</b> split shows both at once. Swap only when the detail (or the list) genuinely wants the whole width.</Dont>
        </Spec>
      </Section>

      {/* 2-DEPTH */}
      <Section id="twodepth" title="2-depth — tabs → sections → content (Settings idiom)">
        <Spec id="top-tabs-over-a-filterable-section-list" title="Top tabs over a filterable section list"
          when={<>When one list isn't enough, add a <b>top tab row (L1)</b> above the list. L1 is a <b>small fixed set</b>; the left list (L2) is the <b>growable, filterable</b> dimension. This is the Settings window's structure — reuse it for any surface whose content tree outgrows a flat list.</>}>
          <Stage variant="tight" grid>
            <div className="anno" style={{ display: "flex", flexDirection: "column", height: 340, background: "var(--tasty-bg-panel)" }}>
              <Dim style={{ top: 6, right: 8 }}>L1 = fixed · L2 = grows</Dim>
              <div style={{ display: "flex", alignItems: "center", height: 40, flex: "none", padding: "0 12px", gap: 2, borderBottom: "1px solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
                {["General", "Appearance", "Keybindings", "Plugins"].map((t, i) => (
                  <span key={t} style={{ height: 39, padding: "0 12px", display: "inline-flex", alignItems: "center", fontSize: 13,
                    color: i === 1 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: i === 1 ? 600 : 400,
                    borderBottom: i === 1 ? "2px solid var(--tasty-accent-primary)" : "2px solid transparent" }}>{t}</span>
                ))}
              </div>
              <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
                <div style={{ width: 168, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "1px solid var(--tasty-separator)", padding: 6 }}>
                  <div style={{ padding: 4, marginBottom: 4 }}><Input block icon={ic.search} placeholder="Filter…" /></div>
                  {["Theme", "General", "Terminal"].map((s, i) => (
                    <div key={s} style={{ padding: "5px 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                      color: i === 0 ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", background: i === 0 ? "var(--tasty-surface-active)" : "transparent" }}>{s}</div>
                  ))}
                </div>
                <div style={{ flex: 1, padding: 18, minWidth: 0 }}>
                  <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".06em", color: "var(--tasty-text-muted)", marginBottom: 10 }}>Theme preset</div>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10, maxWidth: 320 }}>
                    {[["Mocha", true], ["Latte", false]].map(([n, on]) => (
                      <div key={n} style={{ border: on ? "1px solid var(--tasty-accent-primary)" : "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                        <div style={{ display: "flex", height: 30 }}>{(on ? ["#11111b", "#1e1e2e", "#89b4fa"] : ["#dce0e8", "#eff1f5", "#1e66f5"]).map((c, i) => <div key={i} style={{ flex: 1, background: c }} />)}</div>
                        <div style={{ padding: "5px 8px", fontSize: 12, background: "var(--tasty-bg-panel)" }}>{n}</div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </Stage>
          <Meta
            specs={[["L1 bar", "40–44px, accent underline"], ["L2 list", "168px, filter header"], ["content", "flex, 18px pad"], ["rule", "L1 fixed · L2 grows"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "L1 + L2", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-accent-primary", use: "active tab/preset", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-active", use: "active section", color: "var(--tasty-surface-active)" }]} />
          <Do><b>Do</b> give plugins their own L1 tab. <b>Don't</b> nest a growing plugin list inside another group's L2.</Do>
        </Spec>
      </Section>

      {/* MULTI-TIER TABS */}
      <Section id="multitab" title="Multi-tier tab layout">
        <Spec id="stacked-tab-tiers-workspace-over-panes" title="Stacked tab tiers — workspace over panes"
          when={<>When a window holds more than one level of switchable context, tabs <b>stack into tiers</b>: a top <b>workspace tier</b> (which project) over the per-pane <b>tab strip</b> (which surface). Each tier is a full tab row in its own band — the upper tier sits on <b>--tasty-bg-app</b>, the pane strip on <b>--tasty-bg-sidebar</b>. Only the active path down the tiers is highlighted, so the hierarchy reads at a glance.</>}>
          <Stage variant="tight" grid>
            <div className="anno" style={{ display: "flex", flexDirection: "column", height: 300, background: "var(--tasty-bg-panel)" }}>
              <Dim style={{ top: 6, right: 8 }}>tier 1 = workspace</Dim>
              {/* tier 1 — workspace tabs */}
              <div style={{ display: "flex", alignItems: "center", height: 32, flex: "none", padding: "0 8px", gap: 2,
                background: "var(--tasty-bg-app)", borderBottom: "1px solid var(--tasty-separator)" }}>
                {[["tasty-core", true], ["docs-site", false], ["scratch", false]].map(([n, a]) => (
                  <span key={n} style={{ display: "inline-flex", alignItems: "center", gap: 7, height: 24, padding: "0 12px",
                    borderRadius: "var(--tasty-radius-sm)", fontSize: 13,
                    background: a ? "var(--tasty-surface-active)" : "transparent",
                    color: a ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", fontWeight: a ? 600 : 400 }}>
                    <StatusDot status={a ? "agent" : "idle"} pulse={a} />{n}
                  </span>
                ))}
                <span style={{ marginLeft: 4 }}><IconButton size="sm" aria-label="New workspace">{ic.plus}</IconButton></span>
              </div>
              {/* tier 2 — pane tab strip */}
              <div style={{ display: "flex", alignItems: "stretch", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
                <Tab label="build.sh" icon={ic.term} active />
                <Tab label="README.md" icon={ic.md} status="busy" />
                <Tab label="server.log" icon={ic.term} />
                <div style={{ display: "flex", alignItems: "center", padding: "0 6px" }}><IconButton size="sm" aria-label="New tab">{ic.plus}</IconButton></div>
              </div>
              {/* content */}
              <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", margin: 8, borderRadius: "var(--tasty-radius)" }} />
            </div>
          </Stage>
          <Meta
            specs={[["tier 1", <>32px, <span className="tok">--tasty-bg-app</span></>], ["tier 2", <>24px Tab strip, <span className="tok">--tasty-bg-sidebar</span></>], ["active path", "one highlight per tier"], ["order", "broadest context on top"]]}
            tokens={[{ tok: "--tasty-bg-app", use: "tier 1 band", color: "var(--tasty-bg-app)" }, { tok: "--tasty-bg-sidebar", use: "tier 2 band", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-surface-active", use: "active workspace", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "active pane bar", color: "var(--tasty-accent-primary-raised)" }]} />
          <Dont><b>Don't</b> stack more than two tiers — beyond that, move the broadest dimension into the sidebar. Tiers are for <i>switchable</i> context, not deep trees.</Dont>
        </Spec>
      </Section>

      {/* DIVIDER */}
      <Section id="divider" title="Divider">
        <Spec id="pane-divider-the-resize-boundary" title="Pane divider — the resize boundary"
          when={<>The <b>1px separator</b> between split panes, doubling as the <b>drag-to-resize</b> handle. At rest it's a hairline (<b>--tasty-separator</b>); on hover the hit-band (a ~6px transparent zone around the line) lights to the accent and the cursor becomes a resize arrow. Works on both axes — a vertical divider splits left/right, a horizontal one splits top/bottom.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", height: 240, padding: 12, gap: 0, background: "var(--tasty-bg-panel)" }}>
              {/* left pane */}
              <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)", borderRadius: "var(--tasty-radius) 0 0 var(--tasty-radius)", display: "flex", alignItems: "center", justifyContent: "center", fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>pane A</div>
              {/* vertical divider (hover = accent) */}
              <div className="g-divider" title="Drag to resize" style={{ position: "relative", width: 7, cursor: "col-resize", display: "flex", alignItems: "center", justifyContent: "center" }}>
                <span className="g-divider-line" style={{ width: 1, height: "100%", background: "var(--tasty-separator)", display: "block" }} />
              </div>
              {/* right group with a horizontal divider */}
              <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
                <div style={{ flex: 1, background: "var(--tasty-surface-terminal-unfocused-bg)", borderRadius: "0 var(--tasty-radius) 0 0", display: "flex", alignItems: "center", justifyContent: "center", fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>pane B</div>
                <div className="g-divider" title="Drag to resize" style={{ position: "relative", height: 7, cursor: "row-resize", display: "flex", alignItems: "center", justifyContent: "center" }}>
                  <span className="g-divider-line" style={{ height: 1, width: "100%", background: "var(--tasty-separator)", display: "block" }} />
                </div>
                <div style={{ flex: 1, background: "var(--tasty-surface-terminal-unfocused-bg)", borderRadius: "0 0 var(--tasty-radius) 0", display: "flex", alignItems: "center", justifyContent: "center", fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-muted)" }}>pane C</div>
              </div>
            </div>
          </Stage>
          <style>{`.g-divider:hover .g-divider-line { background: var(--tasty-accent-primary); }`}</style>
          <Meta
            specs={[["line", <>1px <span className="tok">--tasty-separator</span></>], ["hit-band", "~6–8px transparent"], ["hover", <>line \u2192 <span className="tok">--tasty-accent-primary</span></>], ["cursor", "col-resize / row-resize"], ["axes", "vertical & horizontal"]]}
            tokens={[{ tok: "--tasty-separator", use: "rest line" }, { tok: "--tasty-accent-primary", use: "hover / dragging", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>Hover either divider above — the hairline turns accent. The <b>hit-band is wider than the visible line</b> so the 1px boundary stays easy to grab without thickening the resting visual.</Note>
        </Spec>
      </Section>

      {/* SURFACES */}
      <Section id="surfaces" title="Surface focus states">
        <Spec id="focused-unfocused-agent" title="Focused · unfocused · agent"
          when={<>Split panes make focus explicit through color. The <b>focused</b> terminal surface goes <b>true black (#000)</b> with full-strength text; <b>unfocused</b> surfaces dim to <code>--surface-terminal-unfocused</code> at 92% opacity. An <b>agent</b> surface carries the mauve pulsing dot — you always know who's driving.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 8, padding: 12, height: 220, background: "var(--tasty-bg-panel)" }}>
              {fakePane("agent · ai-review", false, true)}
              {fakePane("you · zsh", true, false)}
            </div>
          </Stage>
          <Meta
            specs={[["focused bg", <>#000 <span className="tok">--tasty-surface-terminal-focused-bg</span></>], ["unfocused bg", <span className="tok">--tasty-surface-terminal-unfocused-bg</span>], ["unfocused opacity", "0.92"], ["agent marker", <>pulsing <span className="tok">--tasty-accent-agent</span></>]]}
            tokens={[{ tok: "--tasty-surface-terminal-focused-bg", use: "#000 focused", color: "#000" }, { tok: "--tasty-surface-terminal-unfocused-bg", use: "dimmed", color: "var(--tasty-surface-terminal-unfocused-bg)" }, { tok: "--tasty-accent-agent", use: "agent dot", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-accent-success", use: "your running dot", color: "var(--tasty-accent-success-raised)" }]} />
          <Note>This is the soul of the product made visual: <b>user vs. agent</b> and <b>focused vs. not</b> are never ambiguous. Carry these colors into any surface mock.</Note>
        </Spec>

        <Spec id="mixed-split-terminal-markdown-in-one-pane-group" title="Mixed split — terminal + markdown in one pane group"
          when={<>A pane group is <b>kind-agnostic</b>: a <b>terminal</b> and a <b>markdown</b> surface can sit side by side in the same tab — the common "run it left, read the doc right" layout. Each surface keeps its <b>own</b> focus bed: the terminal goes <b>#000</b> when focused, the markdown surface uses the <b>document bed</b> (<span className="tok">--tasty-surface-markdown-focused-bg</span>) focused and the <b>sidebar bed</b> unfocused — never black. The markdown surface keeps its 36px <b>address bar</b> inside the split, and the status bar reports the <b>focused surface</b>, not the tab.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 8, padding: 12, height: 240, background: "var(--tasty-bg-panel)" }}>
              {fakePane("you · zsh", true, false)}
              {fakeDocPane()}
            </div>
          </Stage>
          <Meta
            specs={[["group", "one tab, surfaces of different kinds"], ["terminal focused", "#000 · opacity 1"], ["markdown unfocused", <span className="tok">--tasty-surface-markdown-unfocused-bg</span>], ["markdown chrome", "36px address bar, kept in split"], ["prose measure", "max 620px"], ["status bar", "focused surface id"]]}
            tokens={[{ tok: "--tasty-surface-markdown-focused-bg", use: "document bed (focused)", color: "var(--tasty-surface-markdown-focused-bg)" }, { tok: "--tasty-surface-markdown-unfocused-bg", use: "doc surface, unfocused", color: "var(--tasty-surface-markdown-unfocused-bg)" }, { tok: "--tasty-surface-terminal-focused-bg", use: "#000 terminal, focused", color: "#000" }, { tok: "--tasty-separator", use: "surface border" }]} />
          <Dont><b>Don't</b> paint the markdown pane black to "match" its terminal neighbour — black is a raw-TTY convention. Prose stays on the Catppuccin document bed, which is how the two kinds stay tellable apart at a glance.</Dont>
        </Spec>

        <Spec id="occupancy-completion-borders" title="Occupancy & completion borders"
          when={<>Three surface states ride <b>one visual channel</b> — the terminal surface's <b>border</b> — and are told apart by <b>color alone</b>, so the palette must stay mutually unmistakable at a thin edge. <b>Occupied · soft (green, 1px)</b>: a subject (remote user or AI agent) holds the surface — a cooperative heads-up, <b>no write restriction</b>; it may close or change at any time. <b>Occupied · hard (peach, 1px)</b>: the same notice plus <b>readonly</b> (input blocked, mirror-observe) — it carries a <b>force-detach</b> affordance top-right and absorbs the old remote-attach border. <b>Completed (blue, 2px)</b>: a <b>surface-highlight</b> drawing attention to a finished task; it <b>clears on focus</b>. Occupancy outranks the completion highlight when both apply (a source-side priority rule, not a token).</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 8, padding: 12, height: 220, background: "var(--tasty-bg-panel)" }}>
              {occPane("soft")}
              {occPane("hard")}
              {occPane("done")}
            </div>
          </Stage>
          <Meta
            specs={[["channel", "surface border — color-only"], ["soft", <>green, 1px — held, writable</>], ["hard", <>peach, 1px — readonly + force-detach</>], ["completed", <>blue, 2px — clears on focus</>], ["priority", "occupancy > completion (source rule)"]]}
            tokens={[{ tok: "--tasty-surface-occupied-soft-border", use: "soft edge (→ green)", color: "var(--tasty-accent-occupied-soft)" }, { tok: "--tasty-surface-occupied-hard-border", use: "hard edge (→ peach)", color: "var(--tasty-accent-occupied-hard)" }, { tok: "--tasty-surface-highlight-done-border", use: "completion edge (→ blue)", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-occupied-border-width", use: "occupancy edge = 1px" }, { tok: "--tasty-surface-highlight-done-width", use: "completion edge = 2px" }]} />
          <Note><b>Why blue, not sky, for completion.</b> The ADR describes the completion border as “sky”, but the live source draws it <b>blue</b> (<span className="tok">--tasty-accent-primary</span>) and the design confirms blue as canonical: against the green (soft) and peach (hard) neighbours on the same channel, blue is the most discriminable — sky sits too close to green at a 1px edge. <b>Hard</b> reuses the peach primitive but through its own role (<span className="tok">--tasty-accent-occupied-hard</span>), kept distinct from <span className="tok">--tasty-accent-attention</span> (plugin needs-attention) so the two intents can diverge. Completion is one <b>attention kind</b>; the second kind (<b>NeedsInput</b>, yellow) and the full cross-channel priority live in <b>Attention kinds</b> below.</Note>
        </Spec>
      </Section>

      {/* ATTENTION */}
      <Section id="attention" title="Attention kinds">
        <Spec id="the-scale-kind-color-rank" title="The scale — kind → color → rank"
          when={<>A background subject (agent, task, remote session) raises <b>attention</b> on a target. Attention is a <b>scale, not a pair of colors</b>: each <b>kind</b> takes one semantic slot (<span className="tok">--tasty-attention-&lt;kind&gt;</span>) that borrows an existing accent role, plus a <b>rank</b>. Every channel that renders attention — workspace badge, collapsed rail dot, tab title, surface border — reads the same slot, so adding a kind is one token line per channel and no new logic.</>}>
          <Stage variant="tight" grid><div style={{ padding: 14, background: "var(--tasty-bg-panel)" }}><AttentionScale /></div></Stage>
          <Meta
            specs={[["kinds (live)", "needs-input · completion"], ["reserved", "error (rank 40) · approval (rank 20)"], ["rank spacing", "10 — a future kind slots between without renumbering"], ["collision rule", "highest rank takes the single visual slot"], ["contrast", "fill × on-accent fg — both live kinds clear 4.5:1"]]}
            tokens={[{ tok: "--tasty-attention-needs-input", use: "blocked, waiting on the human (→ yellow)", color: "var(--tasty-attention-needs-input)" }, { tok: "--tasty-attention-completion", use: "task finished, FYI (→ blue)", color: "var(--tasty-attention-completion)" }, { tok: "--tasty-attention-needs-input-fg", use: "glyph atop the yellow fill", color: "var(--tasty-attention-needs-input-fg)" }, { tok: "--tasty-attention-completion-fg", use: "glyph atop the blue fill", color: "var(--tasty-attention-completion-fg)" }, { tok: "--tasty-attention-rank-needs-input", use: "30" }, { tok: "--tasty-attention-rank-completion", use: "10" }]} />
          <Note><b>NeedsInput outranks everything</b> because it is the only kind that is a <i>request</i> — work has stopped until the human answers. Completion is an <i>FYI</i> and clears on focus. No attention kind mints a new hue; a kind that can't honestly borrow an existing accent role is a sign the role list is missing something.</Note>
        </Spec>

        <Spec id="workspace-row-one-or-two-count-badges" title="Workspace row — one or two count badges"
          when={<>The expanded sidebar row can carry a badge per kind. <b>The trailing slot is the badge slot</b>: with one kind present it sits there regardless of which kind it is, so a row never shifts its badge position by kind. With both, <b>NeedsInput leads and Completion trails</b> — higher rank reads first in a left-to-right scan — separated by <span className="tok">--tasty-badge-group-gap</span>. Counts over two digits collapse to <code>99+</code> for both kinds.</>}>
          <Stage variant="tight" grid><AttentionRows /></Stage>
          <Meta
            specs={[["slot", "row trailing edge — kind-independent"], ["order (both)", "needs-input → completion"], ["gap", <span className="tok">--tasty-badge-group-gap</span>], ["overflow", "99+ — both kinds"], ["height", "16px badge, micro 10px numerals"]]}
            tokens={[{ tok: "--tasty-badge-warning-bg", use: "NeedsInput badge fill", color: "var(--tasty-badge-warning-bg)" }, { tok: "--tasty-badge-warning-fg", use: "NeedsInput numeral", color: "var(--tasty-badge-warning-fg)" }, { tok: "--tasty-badge-primary-bg", use: "Completion badge fill", color: "var(--tasty-badge-primary-bg)" }, { tok: "--tasty-badge-primary-fg", use: "Completion numeral", color: "var(--tasty-badge-primary-fg)" }, { tok: "--tasty-badge-group-gap", use: "4px between the two" }]} />
          <Dont><b>Don't</b> reserve an empty slot for the missing kind. A fixed two-slot row makes every quiet workspace look like it has holes in it; the group is inline and collapses to the count that exists.</Dont>
        </Spec>

        <Spec id="collapsed-rail-one-dot-highest-rank-wins" title="Collapsed rail — one dot, highest rank wins"
          when={<>The rail has one dot slot per workspace (the <b>compact 6px</b> dot, <span className="tok">--tasty-status-dot-size-compact</span> — the rail is 24px-class chrome) and no room for a number, so the dot answers <b>“what is the most urgent thing here”</b>, not “how many”. It is <b>always a single dot</b>, colored by the highest-ranked live state: <b>NeedsInput › Completion › busy/running</b>. Two dots were considered and rejected — at 6px on the rail a second dot reads as noise, still carries no count, and breaks the one-slot alignment the rail depends on. The count is one click away in the expanded sidebar.</>}>
          <Stage variant="tight" grid><AttentionRail /></Stage>
          <Meta
            specs={[["slot", "1 dot, top-right of the avatar"], ["size", <>6 · <span className="tok">--tasty-status-dot-size-compact</span></>], ["ring", <>1.5px <span className="tok">--tasty-status-dot-ring</span> (→ bg-sidebar) on <b>every</b> dot, busy included — keeps the dot legible over any avatar</>], ["order", "needs-input › completion › busy"], ["count", "not shown — expand the sidebar"]]}
            tokens={[{ tok: "--tasty-status-dot-needs-input", use: "rail dot — NeedsInput", color: "var(--tasty-status-dot-needs-input)" }, { tok: "--tasty-status-dot-completion", use: "rail dot — Completion", color: "var(--tasty-status-dot-completion)" }, { tok: "--tasty-status-dot-success", use: "rail dot — busy/running", color: "var(--tasty-status-dot-success)" }, { tok: "--tasty-status-dot-size-compact", use: "6px" }, { tok: "--tasty-status-dot-ring", use: "cut-out ring (→ bg-sidebar)", color: "var(--tasty-status-dot-ring)" }, { tok: "--tasty-status-dot-ring-width", use: "1.5px — OFF-SCALE hairline" }]} />
          <Note><b>Attention beats activity.</b> Busy-green says “something is happening”, which is the normal state of this product and the least actionable thing on the rail; both attention kinds are events that want a human. The previous code already preferred highlight over busy — this makes it a rule.</Note>
        </Spec>

        <Spec id="tab-title-surface-border-the-priority-ladder" title="Tab title & surface border — the priority ladder"
          when={<>The pane tab strip tints the <b>title color</b> and the surface tints its <b>border</b>. Both are single channels shared with non-attention states, so both resolve by rank. Tab title: <b>NeedsInput → Completion → active → rest</b>. Surface border: <b>NeedsInput (2px yellow) → occupancy (1px green/peach) → Completion (2px blue)</b> — NeedsInput steps <i>above</i> occupancy, the one change to the earlier occupancy-over-completion order.</>}>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", background: "var(--tasty-bg-sidebar)", borderBottom: "1px solid var(--tasty-separator)" }}>
              <AttTab label="build.log" kind="needs-input" />
              <AttTab label="deploy.sh" kind="completion" />
              <AttTab label="zsh" active />
              <AttTab label="notes.md" />
            </div>
          </Stage>
          <Stage variant="tight" grid>
            <div style={{ display: "flex", gap: 8, padding: 12, height: 200, background: "var(--tasty-bg-panel)" }}>
              {attPane("needs-input")}
              {attPane("occupied")}
              {attPane("completion")}
            </div>
          </Stage>
          <Meta
            specs={[["tab — needs-input", <span className="tok">--tasty-tab-fg-needs-input</span>], ["tab — completion", <span className="tok">--tasty-tab-fg-completion</span>], ["tab — active", <span className="tok">--tasty-tab-fg-active</span>], ["tab — rest", <span className="tok">--tasty-tab-fg</span>], ["border — needs-input", "2px, inside"], ["border priority", "needs-input > occupancy > completion"]]}
            tokens={[{ tok: "--tasty-tab-fg-needs-input", use: "blocked tab title", color: "var(--tasty-tab-fg-needs-input)" }, { tok: "--tasty-tab-fg-completion", use: "finished tab title", color: "var(--tasty-tab-fg-completion)" }, { tok: "--tasty-surface-highlight-input-border", use: "blocked surface edge", color: "var(--tasty-surface-highlight-input-border)" }, { tok: "--tasty-surface-highlight-input-width", use: "2px — matches completion" }]} />
          <Note><b>Why NeedsInput outranks occupancy.</b> Occupancy reads as “held, working, as expected” — which is exactly the state a blocked prompt would hide behind. A session that has stopped to ask you something must not look like a session that is busy. Completion stays below occupancy, as before. Attention clears only when the surface <b>actually receives focus</b> (focused pane, current workspace, window focused). The <b>visible focused tab</b> therefore never shows a tint; an <i>active</i> tab in an unfocused pane or another workspace can hold attention and keeps its tint by the ladder above (needs-input / completion over active).</Note>
          <Dont><b>Don't</b> stack the two edges (a 1px occupancy line inside a 2px NeedsInput line). One channel, one color — stacked edges read as a rendering bug at these widths.</Dont>
        </Spec>
      </Section>

      <Section id="movesource" title="Move source highlight">
        {GalleryMoveSource ? <GalleryMoveSource /> : null}
      </Section>

      <Section id="loading" title="Loading overlay">
        {GalleryLoadingOverlay ? <GalleryLoadingOverlay /> : null}
        {GalleryLoadingPrelayout ? <GalleryLoadingPrelayout /> : null}
      </Section>

      <Section id="notifsummary" title="Notifications summary (LNB · rail)">
        {true ? <GalleryNotifications_Lnb /> : null}
      </Section>

      <Section id="statusbar" title="Workspace status bar">
        <Spec id="what-the-24px-bar-carries-and-what-goes-first-when-it-can-t" title="What the 24px bar carries, and what goes first when it can't"
          when={<>The bar is a <b>read-only summary of the focused surface</b> plus one keyboard reminder. Left cluster, in this order: <b>git branch</b> (glyph + name), <b>surface id</b> (mono <code>s3·p1</code>), <b>shell</b>, <b>grid</b> (<code>120×32</code>). Right cluster: the <b>palette shortcut</b> and the <b>theme</b> glyph. Nothing here is a destination — clicking an item never moves focus, and the data is read from the focused surface at the existing 1Hz cache. Items that have no value <b>disappear</b> rather than showing a dash: outside a repo the branch item is absent, not empty.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "flex-start" }}>
            {[["wide — everything", 720, { branch: "feat/dag-detail" }],
              ["narrower — grid then shell drop", 470, { branch: "feat/dag-detail", drop: 2 }],
              ["narrow — surface id drops, branch truncates", 340, { branch: "feat/dag-detail-and-runner-badge", drop: 3 }],
              ["floor — branch glyph + palette + theme", 210, { branch: "feat/dag-detail", drop: 5 }],
              ["not a repo — the branch item is absent", 470, { branch: null, drop: 2 }],
              ["detached HEAD — short sha in the branch slot", 470, { branch: "@ 4f9c1ab", drop: 2 }]].map(([label, w, o]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ width: w, height: "var(--tasty-status-bar-height)", display: "flex", alignItems: "center", gap: 10, padding: "0 10px",
                  background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius-sm)",
                  fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden" }}>
                  {o.branch && (
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 4, flex: "none", maxWidth: 160, overflow: "hidden" }}>
                      <span style={{ display: "inline-flex", color: "var(--tasty-statusbar-glyph)" }}>{ic.branch}</span>
                      {(o.drop || 0) < 5 && <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{o.branch}</span>}
                    </span>
                  )}
                  {(o.drop || 0) < 3 && <span style={{ fontFamily: "var(--tasty-font-mono)", flex: "none" }}>s3·p1</span>}
                  {(o.drop || 0) < 2 && <span style={{ flex: "none" }}>zsh</span>}
                  {(o.drop || 0) < 1 && <span style={{ fontFamily: "var(--tasty-font-mono)", flex: "none" }}>120×32</span>}
                  <div style={{ flex: 1 }} />
                  {(o.drop || 0) < 4 && <span style={{ flex: "none" }}><Kbd keys="Ctrl+K" /></span>}
                  <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-statusbar-theme-glyph)" }}>{ic.sun}</span>
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["height", <>24 — <span className="tok">--tasty-status-bar-height</span>, outside the UI scale</>], ["left", "branch · surface id · shell · grid"], ["right", "palette shortcut · theme glyph"], ["glyphs", <span className="tok">--tasty-statusbar-glyph</span>], ["theme", <>a <code>sun</code> / <code>theme</code> glyph, <b>not</b> a coloured dot</>], ["drop order", "1 grid → 2 shell → 3 surface id → 4 palette cap → 5 branch text (glyph stays)"], ["never drops", "theme glyph"], ["no value", "the item is absent — never a dash"], ["detached HEAD", "short sha in the branch slot"], ["interaction", "read-only; no item moves focus"], ["refresh", "1Hz cache — unchanged"]]}
            tokens={[{ tok: "--tasty-status-bar-height", use: "24px bar" }, { tok: "--tasty-statusbar-glyph", use: "item glyphs", color: "var(--tasty-statusbar-glyph)" }, { tok: "--tasty-statusbar-theme-glyph", use: "theme indicator", color: "var(--tasty-statusbar-theme-glyph)" }, { tok: "--tasty-statusbar-dot-size", use: "6 — when a surface dot is shown" }]} />
          <Note>The theme indicator used to borrow <b>accent-warning</b> for light and <b>accent-agent</b> for dark. Light-vs-dark is not a warning and not an agent, so it becomes a <b>glyph in dim ink</b> — no colour role is minted for it.</Note>
          <Dont><b>Don't</b> add counts, clocks, CPU meters or notification badges. Anything that changes faster than the bar's 1Hz read, or that is not a property of the focused surface, belongs elsewhere.</Dont>
        </Spec>

        <Spec id="theme-cell-one-glyph-box-both-themes-settled" title="Theme cell — one glyph box, both themes (settled)"
          when={<>The theme cell draws <code>sun</code> in Latte and <code>theme</code> in Mocha at <b>one size</b>: <span className="tok">--tasty-statusbar-glyph-size</span> = <span className="tok">--tasty-icon-size-xs</span> (12), the same box every other inline glyph in the bar uses. A dimension must not depend on which theme is active, so the two glyphs are <b>not</b> given different sizes. They used to read differently anyway — <code>sun</code>'s rays ran to the edge of the 24 box (ink 92%) while <code>theme</code>'s ring stops short (ink 83%), so Latte's glyph measured <b>+2px taller</b> than the caption beside it at every UI scale. That is a drawing fault, not a sizing one, so it is fixed <b>in the asset</b>: <code>icons/sun.svg</code> ray extent pulled to the same 83% ink box. Both glyphs now land at 10px ink inside the 12 box, matching the 11px caption's ascender.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 16, alignItems: "stretch" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <div {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: 14, flexWrap: "wrap", alignItems: "flex-end",
                  background: "var(--tasty-bg-app)", padding: 12, borderRadius: "var(--tasty-radius)", border: "1px solid var(--tasty-border-default)" }}>
                  {[0.85, 1, 1.2].map((s) => (
                    <div key={s} style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                      <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, color: "var(--tasty-text-muted)" }}>UI scale ×{s}</div>
                      <div style={{ height: "var(--tasty-status-bar-height)", display: "flex", alignItems: "center", gap: 8, padding: "0 10px",
                        background: "var(--tasty-bg-sidebar)", border: "1px solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius-sm)",
                        fontSize: Math.round(11 * s), color: "var(--tasty-text-muted)" }}>
                        <span style={{ width: "var(--tasty-statusbar-dot-size)", height: "var(--tasty-statusbar-dot-size)", flex: "none",
                          borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-status-idle)" }} />
                        <span>main</span>
                        <span style={{ display: "inline-flex", color: "var(--tasty-statusbar-theme-glyph)" }}><Icon name="sun" size={Math.round(12 * s)} /></span>
                        <span>Latte</span>
                        <span style={{ display: "inline-flex", color: "var(--tasty-statusbar-theme-glyph)" }}><Icon name="theme" size={Math.round(12 * s)} /></span>
                        <span>Mocha</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["glyph box", <>12 — <span className="tok">--tasty-statusbar-glyph-size</span> → <span className="tok">--tasty-icon-size-xs</span></>], ["per theme", "the same value — Mocha and Latte never differ"], ["per glyph", "the same value — sun and theme never differ"], ["UI scale", <>rides the scale with the rest: round(12×s) = 10 / 12 / 14</>], ["bar height", "24, outside the UI scale — unchanged"], ["ink ratio", "83% of the box for both glyphs (10px ink at box 12)"], ["optical fix", <>asset — <code>icons/sun.svg</code> rays 2→3 / 22→21, diagonals to ±6.4</>], ["ink before", "sun 92% · theme 83% → Latte read +2px large"], ["color", <span className="tok">--tasty-statusbar-theme-glyph</span>]]}
            tokens={[{ tok: "--tasty-statusbar-glyph-size", use: "every inline glyph in the bar" }, { tok: "--tasty-icon-size-xs", use: "the semantic it aliases" }, { tok: "--tasty-statusbar-theme-glyph", use: "theme glyph ink", color: "var(--tasty-statusbar-theme-glyph)" }, { tok: "--tasty-statusbar-dot-size", use: "6 — the dot on the same line" }]} />
          <Note>The two glyphs sit side by side here <b>only to compare them</b>. In the product the cell carries exactly one: <code>sun</code> in Latte, <code>theme</code> in Mocha, with the theme name beside it.</Note>
          <Dont><b>Don't</b> give <code>sun</code> its own size token to cancel the overshoot. A size that changes with theme state turns a dimension into a state, and the same overshoot would follow <code>sun</code> into every other place it is drawn. Optical parity belongs in the glyph.</Dont>
        </Spec>
      </Section>

      <Section id="titlebar" title="Title bar">
        {GalleryTitleBar && <GalleryTitleBar />}
      </Section>

    </>
  );
}

export const page = {
  active: "layouts",
  nav: NAV,
  head: {
    title: "Layouts",
    intro: "Explore how sidebars, tabs, split regions, status bars, and title bars form the app shell. Compare states and views, then check dimensions and tokens in the always-visible Specs.",
    
  },
  content: <Layouts />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
