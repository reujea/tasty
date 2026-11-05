import React from "react";
import { Icon } from "../core/Icon";
import { IconButton } from "../core/IconButton";
import { Button } from "../core/Button";
import { NOTIFICATION_LEVELS } from "./NotificationBadge";

/**
 * Tasty NotificationItem — one row of the notifications list (popup and fullscreen), 2026-10-17.
 *
 *   [glyph 14] title (1 line, ellipsis) ........ time · unread dot
 *              body (clamp 2 lines popup / 3 fullscreen) · "More" when cut
 *              source  workspace › surface  [closed] ...... [✓] [Jump]
 *
 * Unread = semibold text-primary title + accent-primary dot. Read = regular text-secondary title,
 * muted body, no dot. The level glyph keeps its colour in both states (level is identity, not state).
 * Clicking the row (or "More") toggles `expanded`: the body shows in full and wraps; expanding is an
 * explicit look at THAT item, so the caller marks it read. "✓" = mark this one read (unread rows only).
 * Jump = move to the source workspace/surface; disabled with "Source closed" when the source is gone.
 * No action steals focus on arrival; new rows are inserted above without moving the selection.
 */
export function NotificationItem({
  level = "info", title, body, source, sourceClosed = false, time, unread = false,
  expanded = false, wide = false, selected = false, hover = false,
  onToggle, onMarkRead, onJump, style, ...rest
}) {
  const L = NOTIFICATION_LEVELS[level] || NOTIFICATION_LEVELS.info;
  const bodyRef = React.useRef(null);
  const [cut, setCut] = React.useState(false);
  const lines = wide ? "var(--tasty-notification-item-body-lines-wide)" : "var(--tasty-notification-item-body-lines)";
  React.useLayoutEffect(() => {
    const el = bodyRef.current; if (!el || expanded) return;
    const check = () => { const c = el.scrollHeight > el.clientHeight + 1; setCut((p) => (p === c ? p : c)); };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check); ro.observe(el);
    return () => ro.disconnect();
  }, [body, expanded, wide]);
  const selInk = selected && hover ? "var(--tasty-text-on-active-hover)" : "var(--tasty-notification-item-fg-selected)";
  const bg = selected ? "var(--tasty-notification-item-bg-selected)" : hover ? "var(--tasty-notification-item-bg-hover)" : "transparent";
  const meta = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: selected ? selInk : "var(--tasty-notification-item-meta-fg)", whiteSpace: "nowrap", flex: "none" };
  const stop = (fn) => (e) => { e.stopPropagation(); fn && fn(e); };
  return (
    <div role="listitem" aria-label={(unread ? "Unread " : "") + L.one + ": " + (title || "")} aria-expanded={expanded} {...rest}
      onClick={onToggle}
      style={{ display: "flex", gap: "var(--tasty-notification-item-glyph-gap)", padding: "var(--tasty-notification-item-pad-y) var(--tasty-notification-item-pad-x)",
        background: bg, cursor: "pointer", borderBottom: "var(--tasty-border-width) solid var(--tasty-notification-item-separator)", minWidth: 0, ...style }}>
      <span data-part="glyph" title={L.one} style={{ flex: "none", display: "inline-flex", alignItems: "center", height: "calc(var(--tasty-font-size-body) * var(--tasty-line-height-ui))",
        color: selected ? "var(--tasty-accent-" + ({ error: "danger", warning: "warning", info: "primary" }[level] || "primary") + "-on-active)" : "var(--tasty-notification-item-" + level + "-glyph)" }}>
        <Icon name={L.glyph} size="var(--tasty-notification-item-glyph-size)" aria-hidden="true" />
      </span>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-notification-item-line-gap)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span data-part="title" title={title} style={{ flex: 1, minWidth: 0, overflow: expanded ? "visible" : "hidden", textOverflow: "ellipsis", whiteSpace: expanded ? "normal" : "nowrap",
            fontSize: "var(--tasty-font-size-body)", lineHeight: "var(--tasty-line-height-ui)",
            fontWeight: unread ? "var(--tasty-notification-item-title-weight)" : "var(--tasty-font-weight-normal)",
            color: selected ? selInk : unread ? "var(--tasty-notification-item-title-fg)" : "var(--tasty-notification-item-title-fg-read)" }}>{title}</span>
          {time && <span data-part="meta" style={meta}>{time}</span>}
          <span data-part="unread" aria-hidden="true" style={{ flex: "none", width: "var(--tasty-notification-item-unread-dot-size)", height: "var(--tasty-notification-item-unread-dot-size)",
            borderRadius: "var(--tasty-radius-pill)", background: !unread ? "transparent" : selected ? "var(--tasty-notification-item-unread-dot-selected)" : "var(--tasty-notification-item-unread-dot)" }} />
        </div>
        {body && (
          <div data-part="body" ref={bodyRef} style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", textWrap: "pretty",
            color: selected ? selInk : hover ? "var(--tasty-text-secondary-raised)" : unread ? "var(--tasty-notification-item-body-fg)" : "var(--tasty-notification-item-body-fg-read)",
            whiteSpace: expanded ? "pre-wrap" : "normal", wordBreak: "break-word", userSelect: expanded ? "text" : "none",
            ...(expanded ? {} : { display: "-webkit-box", WebkitLineClamp: lines, WebkitBoxOrient: "vertical", overflow: "hidden" }) }}>{body}</div>
        )}
        {body && (cut || expanded) && (
          <span style={{ alignSelf: "flex-start", display: "flex" }}>
            <button data-part="more" type="button" onClick={stop(onToggle)} aria-expanded={expanded}
              style={{ appearance: "none", border: 0, background: "transparent", padding: 0, cursor: "pointer", font: "inherit",
                fontSize: "var(--tasty-font-size-caption)", color: selected ? selInk : "var(--tasty-notification-item-more-fg)" }}>{expanded ? "Less" : "More"}</button>
          </span>
        )}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", minWidth: 0, minHeight: "var(--tasty-icon-button-size-sm)" }}>
          <span title={source} style={{ ...meta, flex: "0 1 auto", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>{source}</span>
          {sourceClosed && <span style={{ ...meta, color: selected ? "var(--tasty-notification-item-fg-selected)" : "var(--tasty-notification-item-closed-fg)" }}>· closed</span>}
          <span style={{ flex: 1 }} />
          {unread && <span onClick={(e) => e.stopPropagation()} style={{ display: "flex" }}><IconButton size="sm" aria-label="Mark as read" title="Mark as read" onClick={onMarkRead} style={selected ? { color: "var(--tasty-notification-item-fg-selected)" } : undefined}><Icon name="check" /></IconButton></span>}
          <span onClick={(e) => e.stopPropagation()} title={sourceClosed ? "Source closed" : undefined} style={{ display: "flex" }}>
            <Button variant="ghost" size="sm" ground={selected ? "active" : "panel"} disabled={sourceClosed} onClick={onJump}>Jump</Button>
          </span>
        </div>
      </div>
    </div>
  );
}
