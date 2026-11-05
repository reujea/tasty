import React from "react";
import * as DS_ns from "../../components/index.js";

// Tasty UI kit — LNB notification summary (2026-10-17).
// A small block directly ABOVE the Tools · Plugins · Settings block, in both sidebar modes.
// The whole block is ONE button: click (or the existing notifications shortcut) toggles the popup.
// It is a separate axis from surface attention — workspace rows / rail avatars get no new badge.
//
//   expanded (212):  [bell] Notifications ............ [⊘ 2][△ 1][ⓘ 14]     line 1, 24 high
//                    [glyph] latest unread title (1 line, ellipsis) ... 2m   line 2, only if unread > 0
//   rail (44 box):   bell, then the level badges stacked (error › warning › info), zeros skipped
//
// Counts = unread per level of THIS window's list. 0 → no pill; > 99 → "99+"; all 0 → label in
// muted ink, no line 2 (expanded) / bell only (rail). Arrival only updates counts + line 2:
// the popup never opens by itself and focus/selection/scroll stay where they are.
// Tooltip (title stand-in): "Notifications — 2 errors · 1 warning · 14 info" / "Notifications — no unread".

const DS = () => DS_ns;
const LEVEL_GLYPH = { error: "alertCircle", warning: "alertTriangle", info: "info" };

function summaryLabel({ error = 0, warning = 0, info = 0 } = {}) {
  const { notificationCountLabel } = DS();
  const parts = [["error", error], ["warning", warning], ["info", info]].filter(([, n]) => n > 0)
    .map(([l, n]) => (notificationCountLabel ? notificationCountLabel(l, n) : n + " " + l));
  return "Notifications — " + (parts.length ? parts.join(" · ") : "no unread");
}

function useHover(force) {
  const [h, setH] = React.useState(false);
  return [force != null ? force : h, { onMouseEnter: () => setH(true), onMouseLeave: () => setH(false) }];
}

function NotificationSummary({ counts = {}, latest = null, open = false, hover: forceHover, onOpen }) {
  const { Icon, NotificationBadgeGroup } = DS();
  const [hover, hov] = useHover(forceHover);
  const { error = 0, warning = 0, info = 0 } = counts;
  const any = error + warning + info > 0;
  const bg = open ? "var(--tasty-notification-summary-bg-open)" : hover ? "var(--tasty-notification-summary-bg-hover)" : "transparent";
  const lit = hover || open;
  const label = lit ? "var(--tasty-text-secondary-raised)" : "var(--tasty-notification-summary-label-fg)";
  const meta = lit ? "var(--tasty-text-muted-raised)" : "var(--tasty-notification-summary-meta-fg)";
  return (
    <div style={{ padding: "var(--tasty-notification-summary-pad)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <button type="button" {...hov} onClick={(e) => onOpen && onOpen(e.currentTarget.getBoundingClientRect())}
        title={summaryLabel(counts)} aria-label={summaryLabel(counts)} aria-haspopup="dialog" aria-expanded={open}
        style={{ appearance: "none", border: 0, font: "inherit", textAlign: "left", cursor: "pointer", width: "100%", display: "flex", flexDirection: "column",
          padding: any && latest ? "0 var(--tasty-notification-summary-pad-x) var(--tasty-space-xs)" : "0 var(--tasty-notification-summary-pad-x)",
          borderRadius: "var(--tasty-radius)", background: bg, minWidth: 0 }}>
        <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-notification-summary-icon-gap)", height: "var(--tasty-notification-summary-row-height)", minWidth: 0 }}>
          <span style={{ display: "inline-flex", flex: "none", color: any ? label : "var(--tasty-notification-summary-label-fg-empty)" }}><Icon name="bell" size={16} /></span>
          <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: "var(--tasty-font-size-body)",
            color: any ? label : "var(--tasty-notification-summary-label-fg-empty)" }} data-part="label">Notifications</span>
          {NotificationBadgeGroup && <NotificationBadgeGroup error={error} warning={warning} info={info} />}
        </span>
        {any && latest && (
          <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", minWidth: 0, paddingLeft: "calc(var(--tasty-size-16) + var(--tasty-notification-summary-icon-gap))" }}>
            <span data-part="meta" style={{ display: "inline-flex", flex: "none", color: meta }}><Icon name={LEVEL_GLYPH[latest.level] || "info"} size={12} /></span>
            <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              fontSize: "var(--tasty-notification-summary-preview-font-size)", lineHeight: "var(--tasty-line-height-ui)", color: label }}>{latest.title}</span>
            {latest.time && <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: meta }}>{latest.time}</span>}
          </span>
        )}
      </button>
    </div>
  );
}

function RailNotificationSummary({ counts = {}, open = false, hover: forceHover, onOpen }) {
  const { Icon, NotificationBadgeGroup } = DS();
  const [hover, hov] = useHover(forceHover);
  const { error = 0, warning = 0, info = 0 } = counts;
  const any = error + warning + info > 0;
  const bg = open ? "var(--tasty-notification-summary-bg-open)" : hover ? "var(--tasty-notification-summary-bg-hover)" : "transparent";
  return (
    <button type="button" {...hov} onClick={(e) => onOpen && onOpen(e.currentTarget.getBoundingClientRect())}
      title={summaryLabel(counts)} aria-label={summaryLabel(counts)} aria-haspopup="dialog" aria-expanded={open}
      style={{ appearance: "none", border: 0, font: "inherit", cursor: "pointer", width: "var(--tasty-notification-rail-width)", minHeight: "var(--tasty-control-height)",
        display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-xs)",
        padding: any ? "var(--tasty-space-xs) 0" : 0, borderRadius: "var(--tasty-radius)", background: bg, marginBottom: "var(--tasty-space-xs)" }}>
      <span style={{ display: "inline-flex", color: any ? "var(--tasty-notification-summary-label-fg)" : "var(--tasty-notification-summary-label-fg-empty)" }}><Icon name="bell" size={16} /></span>
      {NotificationBadgeGroup && <NotificationBadgeGroup direction="column" error={error} warning={warning} info={info} />}
    </button>
  );
}

const notificationSummaryLabel = summaryLabel;

export { NotificationSummary, RailNotificationSummary, notificationSummaryLabel };
