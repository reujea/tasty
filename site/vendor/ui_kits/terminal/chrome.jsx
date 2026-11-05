import React from "react";
const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;
import { Badge } from "../../components/core/Badge.jsx";
import { Button } from "../../components/core/Button.jsx";
import { Icon as TastyDesignSystem_41fd3f_Icon } from "../../components/core/Icon.jsx";
import { IconButton } from "../../components/core/IconButton.jsx";
import { Kbd } from "../../components/core/Kbd.jsx";
import { Tag } from "../../components/core/Tag.jsx";
import { StatusDot } from "../../components/feedback/StatusDot.jsx";
import { Tab } from "../../components/navigation/Tab.jsx";
import { TreeRow } from "../../components/navigation/TreeRow.jsx";
import { RailNotificationSummary, NotificationSummary } from "./notification_summary.jsx";
import { RailCategoryPopup, SidebarContextMenu } from "./overlays/sidebar_context_menu.jsx";

// Tasty UI kit — screen chrome (title bar, sidebar, tab strip, terminal, status bar).
// Composes the DS component library (components/index.js).

// Resolved at render, not at load: inside components/bundle.js this file runs before the DS
// namespace is filled, so an eager `Icon` read is undefined and the module-level `ic`
// map below would hold elements with an undefined type.
const DSIcon = (p) => React.createElement(TastyDesignSystem_41fd3f_Icon, p);

// ── icons — canonical set, by name from icons/*.svg via the DS <Icon/> ──
// `Icon` is a thin compatibility shim: pass `name` (preferred — renders the
// canonical glyph) or a legacy `d` fragment (still supported for any not-yet-
// migrated call site). New code should always use name.
const Icon = ({ name, d, fill, size = 16, sw = 2, ...rest }) =>
  name != null
    ? <DSIcon name={name} size={size} {...rest} />
    : (
      <svg width={size} height={size} viewBox="0 0 24 24" fill={fill || "none"} stroke={fill ? "none" : "currentColor"}
           strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block" }}>{d}</svg>
    );
const ic = {
  term: <DSIcon name="terminal" />,
  md: <DSIcon name="markdown" />,
  folder: <DSIcon name="folder" />,
  file: <DSIcon name="file" />,
  plus: <DSIcon name="plus" />,
  settings: <DSIcon name="settings" />,
  plug: <DSIcon name="plug" />,
  tools: <DSIcon name="tools" />,
  search: <DSIcon name="search" />,
  bell: <DSIcon name="bell" />,
  chevrons: <DSIcon name="chevronsLeft" />,
  split: <DSIcon name="split" />,
  rocket: <DSIcon name="rocket" />,
};

// ⚠ NON-PRODUCT MOCK. Tasty ships the NATIVE titlebar on every OS (this just
// paints macOS traffic lights for the kit's frame). For the Client-Side-
// Decoration exploration (macOS/Windows/Linux, tokenized) see
// titlebar/titlebar_{macos,windows,linux}.{jsx,html} and FEATURES.md.
function TitleBar({ workspace }) {
  return (
    <div style={{ display: "flex", alignItems: "center", height: "var(--tasty-titlebar-height)", flex: "none",
      background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
      padding: "0 var(--tasty-space-md)", gap: "var(--tasty-space-md)", WebkitUserSelect: "none" }}>
      <div style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>
        <span style={{ width: "var(--tasty-titlebar-traffic-size)", height: "var(--tasty-titlebar-traffic-size)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-titlebar-traffic-close)" }} />
        <span style={{ width: "var(--tasty-titlebar-traffic-size)", height: "var(--tasty-titlebar-traffic-size)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-titlebar-traffic-min)" }} />
        <span style={{ width: "var(--tasty-titlebar-traffic-size)", height: "var(--tasty-titlebar-traffic-size)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-titlebar-traffic-zoom)" }} />
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", margin: "0 auto" }}>
        <img src={`${BASE}tasty-icon.png`} width="16" height="16" alt="" />
        <span style={{ fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-secondary)" }}>{workspace}</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>— tasty</span>
      </div>
      <div style={{ width: 52 }} />
    </div>
  );
}

function WorkspaceRow({ ws, active, onClick, onContextMenu }) {
  
  const [hover, setHover] = React.useState(false);
  return (
    <div onClick={onClick} onContextMenu={onContextMenu} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-workspace-dot-gap)",
        padding: "var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-space-sm) var(--tasty-workspace-row-padding-x)",
        cursor: "pointer", position: "relative",
        background: active ? "var(--tasty-surface-active)" : hover ? "var(--tasty-overlay-hover)" : "transparent",
        boxShadow: active ? "inset var(--tasty-workspace-row-active-bar-width) 0 0 var(--tasty-accent-primary)" : "none",
        ...(active ? {
          "--tasty-status-dot-idle": "var(--tasty-status-dot-idle-active)", "--tasty-status-dot-success": "var(--tasty-status-dot-success-active)",
          "--tasty-status-dot-warning": "var(--tasty-status-dot-warning-active)", "--tasty-status-dot-danger": "var(--tasty-status-dot-danger-active)",
          "--tasty-status-dot-agent": "var(--tasty-status-dot-agent-active)", "--tasty-status-dot-needs-input": "var(--tasty-status-dot-needs-input-active)",
          "--tasty-status-dot-completion": "var(--tasty-status-dot-completion-active)", "--tasty-status-dot-attached-ring": "var(--tasty-status-dot-attached-ring-active)",
        } : null) }}>
      {/* dot slot = the attached mark's full bbox (16), reserved on EVERY row so the
          title x (8 + 16 + 4 = 28) never shifts; the ring can't reach the card edge,
          the 2px active accent bar, or the label. See --tasty-workspace-dot-slot. */}
      <span style={{ flex: "none", width: "var(--tasty-workspace-dot-slot)", height: "calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui))", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
        <StatusDot status={ws.status} attached={ws.attached}
          pulse={ws.status === "agent" || ws.status === "running"}
          title={ws.attached ? "Attached on another client" : undefined} />
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        {/* title = workspace name only (full width — the remote pill sits on its
            own line below, so the title never loses room) */}
        <div style={{ display: "flex", alignItems: "center", minWidth: 0 }}>
          <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-medium)", lineHeight: "var(--tasty-line-height-ui)",
            color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", minWidth: 0,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{ws.name}</span>
        </div>
        {/* remote pill = local mirror of a remote instance. Own line between title
            and subtitle (label + glyph) — same remote axis (--tasty-accent-remote),
            high visibility, and doesn't steal width from the title. Mirrors the
            remote_attach "in use" pill vocabulary. Separate from the status dot. */}
        {ws.mirror && (
          <div style={{ marginTop: "var(--tasty-size-2)" }}>
            <Tag variant="info" caps title={ws.mirrorTarget ? `Mirror of remote → ${ws.mirrorTarget}` : "Mirror of a remote workspace"}
              aria-label="Remote mirror" style={{ color: "var(--tasty-tag-info-fg)" }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round"
                style={{ width: "var(--tasty-workspace-mirror-icon-size)", height: "var(--tasty-workspace-mirror-icon-size)", display: "block" }}>
                <path d="M4 17l6-6-6-6" /><path d="M12 19h8" />
              </svg>
              remote
            </Tag>
          </div>
        )}
        {/* subtitle = short secondary label, one notch below the title */}
        {ws.subtitle && (
          <div style={{ fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)", marginTop: "var(--tasty-size-1)", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{ws.subtitle}</div>
        )}
        {/* description = longer explanatory prose, dimmest tone, wraps up to 2 lines */}
        {ws.description && (
          <div style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", marginTop: "var(--tasty-size-3)", color: "var(--tasty-text-placeholder)",
            display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{ws.description}</div>
        )}
      </div>
      {ws.notif > 0 && (
        <span style={{ flex: "none", marginLeft: "var(--tasty-space-xs)", height: "calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui))", display: "inline-flex", alignItems: "center" }}>
          <Badge variant="primary">{ws.notif}</Badge>
        </span>
      )}
    </div>
  );
}

function RailBtn({ title, active, onClick, children }) {
  return (
    <div title={title} style={{ display: "flex", justifyContent: "center" }}>
      <IconButton active={active} onClick={onClick} aria-label={title}>{children}</IconButton>
    </div>
  );
}

// ── Workspace categories (sidebar folders) — opt-in via the `categories` prop. ──
// Backend invariants the design honours: the reserved `normal` category is always
// first (its header shows the existing "Workspaces" label), can't be renamed /
// deleted / reordered, and holds category-less workspaces. collapse state is
// per-category and SHARED between the full sidebar and the collapsed rail. Empty
// categories render the header (or rail `---` button) with no rows.
// The trailing `+` creates a workspace INTO this category (replaces the removed
// global "New Workspace" button when categories are on). Applies to EVERY header
// including the reserved `normal` (its `+` = a category-less workspace, same intent
// as the old global button). It is HOVER-REVEAL (header already tints on hover) but
// stays keyboard-reachable — focus-visible reveals it. Absolutely positioned in a
// reserved right gutter so it never shifts the label or grows the header height.
// The header is a BAND (bg one tier down + hairlines) carrying a workspace COUNT,
// so the section boundary reads as elevation and the 10px caps line out-ranks the
// 13px rows under it. The count yields to the hover-reveal `+` (same gutter).
function CategoryHeader({ label, collapsed, count, onToggle, onAdd, onContextMenu }) {
  const [hover, setHover] = React.useState(false);
  const [addFocus, setAddFocus] = React.useState(false);
  const show = hover || addFocus;
  return (
    <div role="button" aria-expanded={!collapsed} onClick={onToggle} onContextMenu={onContextMenu}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ position: "relative", display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", cursor: "pointer",
        padding: "var(--tasty-sidebar-category-header-pad-y) var(--tasty-sidebar-category-header-pad-x)",
        paddingRight: "calc(var(--tasty-icon-button-size-sm) + var(--tasty-space-sm))",
        background: hover ? "var(--tasty-overlay-hover)" : "var(--tasty-sidebar-category-header-bg)",
        borderTop: "var(--tasty-border-width) solid var(--tasty-sidebar-category-header-border)",
        borderBottom: "var(--tasty-border-width) solid var(--tasty-sidebar-category-header-border)" }}>
      <span style={{ display: "inline-flex", flex: "none", width: 12, justifyContent: "center", color: "var(--tasty-sidebar-category-header-fg)",
        transform: collapsed ? "none" : "rotate(90deg)", transition: "transform var(--tasty-motion-ui-fast) var(--tasty-ease-ui)" }}>
        <Icon d={<path d="m9 6 6 6-6 6" />} size={12} sw={2.4} />
      </span>
      <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-sidebar-section-heading-font-size)", textTransform: "uppercase",
        letterSpacing: "var(--tasty-sidebar-section-heading-tracking)", color: "var(--tasty-sidebar-category-header-fg)",
        fontWeight: "var(--tasty-sidebar-category-header-weight)",
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      {typeof count === "number" && (
        <span aria-hidden style={{ position: "absolute", top: "50%", right: "var(--tasty-space-sm)", transform: "translateY(-50%)",
          fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-sidebar-category-header-count-font-size)",
          color: "var(--tasty-sidebar-category-header-count-fg)",
          opacity: show ? 0 : 1, transition: "opacity var(--tasty-motion-ui-fast) var(--tasty-ease-ui)" }}>{count}</span>
      )}
      <span style={{ position: "absolute", top: "50%", right: "var(--tasty-space-xs)", transform: "translateY(-50%)",
        opacity: show ? 1 : 0, transition: "opacity var(--tasty-motion-ui-fast) var(--tasty-ease-ui)" }}>
        <IconButton size="sm" title={"New workspace in " + label} aria-label={"New workspace in " + label}
          onFocus={() => setAddFocus(true)} onBlur={() => setAddFocus(false)}
          onClick={(e) => { e.stopPropagation(); onAdd && onAdd(e); }}>{ic.plus}</IconButton>
      </span>
    </div>
  );
}

// Rail category boundary — the thin separator becomes a full-width `---` button
// that opens an anchored popup to its right (same anchored-popup pattern as Tools).
// 2026-10-12: line = avatar ink (rail-category-line → text-muted), 2px (rail-category-line-width);
// hover/pressed lift it to text-secondary on the overlay bed. Same colour for every category.
function RailCategoryBtn({ label, onOpen, forceState }) {
  const [hover, setHover] = React.useState(false);
  const [press, setPress] = React.useState(false);
  const st = forceState || (press ? "pressed" : hover ? "hover" : "idle");
  return (
    <button type="button" title={label} aria-label={label}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => { setHover(false); setPress(false); }}
      onMouseDown={() => setPress(true)} onMouseUp={() => setPress(false)}
      onClick={(e) => onOpen && onOpen(e.currentTarget.getBoundingClientRect())}
      style={{ appearance: "none", border: 0, cursor: "pointer", padding: 0,
        width: "var(--tasty-size-36)", height: "var(--tasty-size-16)", display: "flex", alignItems: "center", justifyContent: "center",
        borderRadius: "var(--tasty-radius-sm)", background: st === "pressed" ? "var(--tasty-overlay-active)" : st === "hover" ? "var(--tasty-overlay-hover)" : "transparent" }}>
      <span style={{ width: "var(--tasty-rail-category-line-length)", height: "var(--tasty-rail-category-line-width)", borderRadius: "var(--tasty-radius-pill)",
        background: st === "idle" ? "var(--tasty-rail-category-line)" : "var(--tasty-rail-category-line-hover)" }} />
    </button>
  );
}

function useCategoryCollapse(categories) {
  const [collapsedIds, setCollapsedIds] = React.useState(() =>
    (categories || []).reduce((m, c) => (c.collapsed ? Object.assign(m, { [c.id]: true }) : m), {}));
  const isCollapsed = (c) => !!collapsedIds[c.id];
  const toggle = (c) => setCollapsedIds((m) => Object.assign({}, m, { [c.id]: !m[c.id] }));
  return { isCollapsed, toggle };
}

// notifications = { counts: {error, warning, info}, latest, open } — LNB summary block above Tools
// (notification_summary.jsx, 2026-10-17). Always drawn; all-zero = muted label / bell only.
const NO_NOTIFS = { counts: { error: 0, warning: 0, info: 0 }, latest: null, open: false };
function CollapsedSidebar({ workspaces, activeWs, onWs, onSettings, onPlugins, onTools, onToggle, pluginAlert = 0, categories = null, workspacesHeading = "Workspaces", notifications = NO_NOTIFS, onNotifications }) {
  const RailNotif = RailNotificationSummary;
  const { isCollapsed, toggle } = useCategoryCollapse(categories);
  const [popup, setPopup] = React.useState(null); // { anchor, cat }
  const Popup = RailCategoryPopup;
  const Avatar = (w) => (
    <div key={w.id} style={{ position: "relative", display: "flex", justifyContent: "center" }}>
      <RailBtn title={w.name} active={w.id === activeWs} onClick={() => onWs(w.id)}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-bold)", lineHeight: "var(--tasty-line-height-tight)" }}>
          {w.name.charAt(0).toUpperCase()}
        </span>
      </RailBtn>
      {w.notif > 0 && (
        <span style={{ position: "absolute", top: -1, right: -1, pointerEvents: "none" }}>
          <Badge dot variant="primary" style={{ boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }} />
        </span>
      )}
      {/* attached = claimed by another client — lavender ring around the avatar (B7-J2) */}
      {w.attached && (
        <span title="Attached on another client" style={{ position: "absolute", inset: 1,
          borderRadius: "var(--tasty-radius)", outline: "var(--tasty-status-dot-attached-ring-width) solid var(--tasty-accent-attached)",
          outlineOffset: "var(--tasty-status-dot-attached-ring-offset)", pointerEvents: "none" }} />
      )}
      {/* mirror = local mirror of a remote instance — sky corner chip at BOTTOM-right
          (notif owns top-right, attached owns the ring) so the three never collide */}
      {w.mirror && (
        <span title={w.mirrorTarget ? `Mirror of remote → ${w.mirrorTarget}` : "Mirror of a remote workspace"}
          aria-label="Remote mirror" style={{ position: "absolute", bottom: -1, right: -1, pointerEvents: "none",
          display: "inline-flex", alignItems: "center", justifyContent: "center",
          width: "var(--tasty-workspace-mirror-chip-size)", height: "var(--tasty-workspace-mirror-chip-size)", borderRadius: "var(--tasty-radius-pill)",
          boxSizing: "border-box", border: "var(--tasty-border-width) solid var(--tasty-workspace-mirror-chip-border)",
          background: "var(--tasty-workspace-mirror-chip-bg)", color: "var(--tasty-workspace-mirror-fg)",
          boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round"
            style={{ width: "var(--tasty-size-8)", height: "var(--tasty-size-8)", display: "block" }}>
            <path d="M4 17l6-6-6-6" /><path d="M12 19h8" />
          </svg>
        </span>
      )}
    </div>
  );
  return (
    <div style={{ width: 52, flex: "none", height: "100%", zoom: "var(--tasty-ui-scale)",
      display: "flex", flexDirection: "column", alignItems: "center",
      background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", padding: "var(--tasty-space-sm) 0", gap: "var(--tasty-size-2)" }}>
      <img src={`${BASE}tasty-icon.png`} width="24" height="24" alt="Tasty" style={{ marginBottom: "var(--tasty-space-xs)" }} />
      <div title="Expand sidebar" style={{ marginBottom: "var(--tasty-space-sm)" }}>
        <IconButton size="sm" aria-label="Expand" onClick={onToggle}>
          <Icon d={<path d="m13 17 5-5-5-5M6 17l5-5-5-5" />} />
        </IconButton>
      </div>
      {categories ? (
        categories.map((cat) => (
          <React.Fragment key={cat.id}>
            <RailCategoryBtn label={cat.reserved ? workspacesHeading : cat.name}
              onOpen={(anchor) => setPopup({ anchor, cat })} />
            {!isCollapsed(cat) && cat.workspaces.map((w) => Avatar(w))}
          </React.Fragment>
        ))
      ) : (
        <>
          <div style={{ width: "var(--tasty-rail-flat-divider-length)", height: "var(--tasty-rail-category-line-width)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-rail-category-line)", marginBottom: "var(--tasty-space-xs)" }} />
          {workspaces.map((w) => Avatar(w))}
        </>
      )}
      {!categories && <RailBtn title="New Workspace" onClick={() => {}}>{ic.plus}</RailBtn>}
      <div style={{ marginTop: "auto", display: "flex", flexDirection: "column", gap: 2, alignItems: "center" }}>
        {RailNotif && <RailNotif counts={notifications.counts} open={notifications.open} onOpen={onNotifications} />}
        <RailBtn title="Tools" onClick={(e) => onTools(e.currentTarget.getBoundingClientRect())}>{ic.tools}</RailBtn>
        <div style={{ position: "relative", display: "flex", justifyContent: "center" }}>
          <RailBtn title={pluginAlert > 0 ? `Plugins — ${pluginAlert} need attention` : "Plugins"} onClick={onPlugins}>{ic.plug}</RailBtn>
          {pluginAlert > 0 && (
            <span style={{ position: "absolute", top: -1, right: 4, pointerEvents: "none" }}>
              <Badge variant="danger" style={{ boxShadow: "0 0 0 var(--tasty-size-2) var(--tasty-bg-sidebar)" }}>{pluginAlert}</Badge>
            </span>
          )}
        </div>
        <RailBtn title="Settings" onClick={onSettings}>{ic.settings}</RailBtn>
      </div>
      {popup && Popup && (
        <Popup anchor={popup.anchor} reserved={popup.cat.reserved}
          label={popup.cat.reserved ? workspacesHeading : popup.cat.name}
          collapsed={isCollapsed(popup.cat)} count={popup.cat.workspaces.length}
          onToggleCollapse={() => { toggle(popup.cat); setPopup(null); }}
          onClose={() => setPopup(null)} />
      )}
    </div>
  );
}

function Sidebar({ workspaces, activeWs, onWs, onSettings, onPlugins, onTools, collapsed, onToggle, pluginAlert = 0, categories = null, workspacesHeading = "Workspaces", notifications = NO_NOTIFS, onNotifications }) {
  const NotifSummary = NotificationSummary;
  const Heading = ({ children }) => (
    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase",
      letterSpacing: "var(--tasty-sidebar-section-heading-tracking)", color: "var(--tasty-text-muted)", padding: "var(--tasty-space-md) var(--tasty-space-sm) var(--tasty-space-xs)" }}>{children}</div>
  );
  const { isCollapsed, toggle } = useCategoryCollapse(categories);
  const [menu, setMenu] = React.useState(null); // { x, y, target }
  const Menu = SidebarContextMenu;
  const openMenu = (e, target) => { e.preventDefault(); e.stopPropagation(); setMenu({ x: e.clientX, y: e.clientY, target }); };
  const rowList = (items, cat, bottomBorder = true, topBorder = true) => (
    <div style={{ display: "flex", flexDirection: "column",
      borderTop: topBorder ? "var(--tasty-border-width) solid var(--tasty-separator)" : "none",
      borderBottom: bottomBorder ? "var(--tasty-border-width) solid var(--tasty-separator)" : "none" }}>
      {items.map((w, i) => (
        <React.Fragment key={w.id}>
          {i > 0 && <div style={{ height: "var(--tasty-border-width)", background: "var(--tasty-separator)", margin: "0 0 0 var(--tasty-size-32)" }} />}
          <WorkspaceRow ws={w} active={w.id === activeWs} onClick={() => onWs(w.id)}
            onContextMenu={categories ? (e) => openMenu(e, { kind: "workspace", ws: w, cat }) : undefined} />
        </React.Fragment>
      ))}
    </div>
  );
  if (collapsed) return <CollapsedSidebar workspaces={workspaces} activeWs={activeWs} onWs={onWs}
    onSettings={onSettings} onPlugins={onPlugins} onTools={onTools} onToggle={onToggle} pluginAlert={pluginAlert}
    categories={categories} workspacesHeading={workspacesHeading} notifications={notifications} onNotifications={onNotifications} />;
  return (
    <div style={{ width: 212, flex: "none", height: "100%", zoom: "var(--tasty-ui-scale)",
      display: "flex", flexDirection: "column",
      background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-md) var(--tasty-space-xs)" }}>
        <img src={`${BASE}tasty-icon.png`} width="22" height="22" alt="Tasty" />
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-font-weight-bold)", fontSize: "var(--tasty-sidebar-wordmark-font-size)", color: "var(--tasty-text-primary)",
          letterSpacing: "var(--tasty-sidebar-wordmark-tracking)" }}>tasty<span data-audit-exempt="brand" style={{ color: "var(--tasty-brand-melon-flesh)" }}>.</span></span>
        <span style={{ marginLeft: "auto" }}>
          <IconButton size="sm" aria-label="Collapse" onClick={onToggle}>{ic.chevrons}</IconButton>
        </span>
      </div>

      {categories ? (
        <div onContextMenu={(e) => { if (e.target === e.currentTarget) openMenu(e, { kind: "background" }); }}
          style={{ display: "flex", flexDirection: "column", paddingTop: "var(--tasty-space-sm)", paddingBottom: "var(--tasty-space-xs)" }}>
          {categories.map((cat, i) => (
            <div key={cat.id} style={i === 0 ? undefined : { marginTop: "var(--tasty-space-md)" }}>
              <CategoryHeader label={cat.reserved ? workspacesHeading : cat.name} collapsed={isCollapsed(cat)}
                count={cat.workspaces.length}
                onToggle={() => toggle(cat)} onAdd={() => {}} onContextMenu={(e) => openMenu(e, { kind: "category", cat })} />
              {!isCollapsed(cat) && cat.workspaces.length > 0 && rowList(cat.workspaces, cat, false, false)}
            </div>
          ))}
        </div>
      ) : (
        <>
          <Heading>{workspacesHeading}</Heading>
          {rowList(workspaces, null)}
        </>
      )}
      {!categories && (
        <div style={{ padding: "var(--tasty-space-sm) var(--tasty-space-sm) 0" }}>
          <Button variant="ghost" size="sm" block leadingIcon={ic.plus}
            style={{ justifyContent: "flex-start" }}>New Workspace</Button>
        </div>
      )}

      <div style={{ marginTop: "auto" }}>
        {NotifSummary && <NotifSummary counts={notifications.counts} latest={notifications.latest} open={notifications.open} onOpen={onNotifications} />}
      </div>
      <div style={{ padding: "var(--tasty-space-sm)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)",
        display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)" }}>
        <Button variant="ghost" size="sm" block leadingIcon={ic.tools}
          onClick={(e) => onTools(e.currentTarget.getBoundingClientRect())}
          style={{ justifyContent: "flex-start" }}>Tools</Button>
        <Button variant="ghost" size="sm" block leadingIcon={ic.plug} onClick={onPlugins}
          trailingIcon={pluginAlert > 0 ? <Badge variant="danger"
            style={{ marginLeft: "auto" }}>{pluginAlert}</Badge> : null}
          style={{ justifyContent: "flex-start" }}>Plugins</Button>
        <Button variant="ghost" size="sm" block leadingIcon={ic.settings} onClick={onSettings}
          style={{ justifyContent: "flex-start" }}>Settings</Button>
      </div>
      {menu && Menu && <Menu x={menu.x} y={menu.y} target={menu.target} onClose={() => setMenu(null)} />}
    </div>
  );
}

export { Icon, ic, TitleBar, Sidebar };
