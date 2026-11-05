import React from "react";
import * as DS_ns from "../../../components/index.js";
import { PopupTitleBar } from "./info_modal.jsx";

// Notifications popup + fullscreen stage (2026-10-17).
// Extends the existing popup (src/adapters/ui/notification.rs · fullscreen/notifications.rs):
// same shared PopupTitleBar (title centred, fullscreen fit + close ×), 352 × 400 default.
//
//   title bar   "Notifications" · [fit] [×]
//   toolbar 28  unread level badges (error › warning › info) ........ [Mark all read]
//   list        NotificationItem rows, newest first; scrolls; title bar + toolbar never do
//   empty       "No notifications." (toolbar shows "No unread", Mark all read disabled)
//
// Read rules: opening / closing the popup marks NOTHING read. Per item: ✓ or expanding the row.
// "Mark all read" = every item of THIS window's list only. Reading the last unread item of a source
// surface lets the existing attention-clear rule run (hard occupancy protection unchanged).
// New items are inserted at the top; selection, scroll offset and keyboard focus are kept
// (when scrolled away from the top, the list does not jump).
// Fullscreen: same list and actions on the wide stage; body clamp 3 lines instead of 2.

const DS = () => DS_ns;

const NOTIFICATION_SAMPLES = [
  { id: "n1", level: "error", unread: true, time: "2m", source: "api-gateway › build", title: "Build failed: cargo build exited with status 101",
    body: "error[E0308]: mismatched types\n  --> src/routes/auth.rs:42:17\n   |\n42 |     let token: Token = header.value();\n   |                ^^^^^ expected `Token`, found `&str`\n\nThe agent stopped after the failed build. 3 more errors were reported in src/routes/session.rs; run cargo build in the surface to see the full output." },
  { id: "n2", level: "warning", unread: true, time: "9m", source: "data-pipeline › ingest", title: "Disk usage above 85% on /var/lib/warehouse",
    body: "The nightly ETL keeps running, but the next snapshot may not fit. Free space or move old partitions." },
  { id: "n3", level: "info", unread: true, time: "14m", source: "agents-prod › deploy", title: "Deploy to staging finished",
    body: "4 services updated in 2m 31s." },
  { id: "n4", level: "info", unread: false, time: "1h", source: "scratch › surface 2", sourceClosed: true, title: "Script run-tests.lua completed",
    body: "212 passed, 0 failed." },
  { id: "n5", level: "error", unread: false, time: "3h", source: "infra › ssh prod-web", sourceClosed: true, title: "Remote attach lost: connection reset by peer",
    body: "The mirror of prod-web stopped receiving updates. The workspace was closed afterwards." },
];

function unreadCounts(items) {
  return items.reduce((m, n) => (n.unread ? Object.assign(m, { [n.level]: (m[n.level] || 0) + 1 }) : m), { error: 0, warning: 0, info: 0 });
}

function NotificationsPopup({ items = NOTIFICATION_SAMPLES, fullscreen = false, expandedId = null, selectedId = null, hoverId = null,
  onToggle, onMarkRead, onMarkAllRead, onJump, width, height, style }) {
  const { Button, NotificationBadgeGroup, NotificationItem } = DS();
  const TitleBar = PopupTitleBar;
  const c = unreadCounts(items);
  const any = c.error + c.warning + c.info > 0;
  return (
    <div role="dialog" aria-label="Notifications" style={{ width: fullscreen ? "100%" : (width || "var(--tasty-notifications-popup-width)"),
      height: fullscreen ? "100%" : (height || "var(--tasty-notifications-popup-height)"), boxSizing: "border-box",
      display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)",
      borderRadius: "var(--tasty-radius)", boxShadow: fullscreen ? "none" : "var(--tasty-shadow-modal)", overflow: "hidden", ...style }}>
      {TitleBar && <TitleBar title="Notifications" fullscreen />}
      <div style={{ flex: "none", height: "var(--tasty-notifications-toolbar-height)", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)",
        padding: "0 var(--tasty-space-xs) 0 var(--tasty-notification-item-pad-x)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        {any && NotificationBadgeGroup ? <NotificationBadgeGroup error={c.error} warning={c.warning} info={c.info} />
          : <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>No unread</span>}
        <span style={{ flex: 1 }} />
        <Button variant="ghost" size="sm" disabled={!any} onClick={onMarkAllRead}>Mark all read</Button>
      </div>
      {items.length === 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-muted)" }}>No notifications.</div>
      ) : (
        <div role="list" className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
          {items.map((n) => NotificationItem && (
            <NotificationItem key={n.id} level={n.level} title={n.title} body={n.body} source={n.source} sourceClosed={n.sourceClosed}
              time={n.time} unread={n.unread} wide={fullscreen} expanded={expandedId === n.id} selected={selectedId === n.id} hover={hoverId === n.id}
              onToggle={() => onToggle && onToggle(n.id)} onMarkRead={() => onMarkRead && onMarkRead(n.id)} onJump={() => onJump && onJump(n)} />
          ))}
        </div>
      )}
    </div>
  );
}

// Stateful wrapper for the kit app: expand marks that item read; ✓ marks one; Mark all read marks the list.
function useNotifications(initial = NOTIFICATION_SAMPLES) {
  const [items, setItems] = React.useState(initial);
  const [expandedId, setExpandedId] = React.useState(null);
  const read = (id) => setItems((xs) => xs.map((n) => (n.id === id ? Object.assign({}, n, { unread: false }) : n)));
  return {
    items, expandedId, counts: unreadCounts(items),
    latest: items.find((n) => n.unread) || null,
    onToggle: (id) => { setExpandedId((e) => (e === id ? null : id)); read(id); },
    onMarkRead: read,
    onMarkAllRead: () => setItems((xs) => xs.map((n) => Object.assign({}, n, { unread: false }))),
  };
}

const notificationUnreadCounts = unreadCounts;

export { NotificationsPopup, useNotifications, notificationUnreadCounts, NOTIFICATION_SAMPLES };
