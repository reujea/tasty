import React from "react";
import { Spinner } from "./Spinner";

/**
 * Tasty PendingIndicator (2026-10-19) — the small "waiting for a response" chip shown
 * while an already-attached workspace waits on requests that create no new area
 * (close · move · restore closed · convert). View-only: it owns no timer, no request
 * state, no cancel/retry. The caller passes the count and the OLDEST pending request's
 * elapsed seconds; it hides the chip on completion and hands failure/disconnect to the
 * existing error / reconnect notices.
 *
 *   [spinner 14] Waiting for response · 12s          (count 1)
 *   [spinner 14] Waiting for 3 responses · 1m 04s    (count > 1, time = the longest wait)
 *   [spinner 14] 1m 04s                              (compact: narrow window)
 *
 * Lives in the window's top-right NOTICE COLUMN (see gallery Overlays · Popups #notices).
 * Reduced motion → still arc. Tooltip lists the pending request kinds (never internal ids).
 */
export function formatElapsed(sec, units) {
  const u = units || { s: "s", m: "m", h: "h", sep: " " };
  const s = Math.max(0, Math.floor(sec || 0));
  if (s < 60) return s + u.s;
  const pad = (n) => String(n).padStart(2, "0");
  if (s < 3600) return Math.floor(s / 60) + u.m + u.sep + pad(s % 60) + u.s;
  return Math.floor(s / 3600) + u.h + u.sep + pad(Math.floor((s % 3600) / 60)) + u.m;
}

export function pendingLabel(count) {
  return count > 1 ? "Waiting for " + count + " responses" : "Waiting for response";
}

export function PendingIndicator({ count = 1, elapsed = 0, label, time, compact = false, reducedMotion = false, tooltip, style, ...rest }) {
  if (!count || count <= 0) return null;
  const text = label != null ? label : pendingLabel(count);
  const t = time != null ? time : formatElapsed(elapsed);
  return (
    <span role="status" aria-live="polite" aria-label={text + " · " + t} title={tooltip || undefined} {...rest}
      style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-pending-indicator-gap)", flex: "none", boxSizing: "border-box", maxWidth: "100%",
        height: "var(--tasty-pending-indicator-height)", padding: "0 var(--tasty-pending-indicator-padding-x)",
        background: "var(--tasty-pending-indicator-bg)", border: "var(--tasty-border-width) solid var(--tasty-pending-indicator-border)",
        borderRadius: "var(--tasty-pending-indicator-radius)", color: "var(--tasty-pending-indicator-fg)",
        fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-caption)", lineHeight: 1, whiteSpace: "nowrap", userSelect: "none", ...style }}>
      <Spinner size="var(--tasty-pending-indicator-spinner-size)" label={text} paused={reducedMotion} reducedMotion="arc"
        style={{ color: "var(--tasty-loading-overlay-indicator)" }} />
      {!compact && <span data-part="label" style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>{text}</span>}
      {!compact && <span data-part="sep" aria-hidden="true" style={{ color: "var(--tasty-pending-indicator-sep-fg)" }}>·</span>}
      <span data-part="time" style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontVariantNumeric: "tabular-nums", color: "var(--tasty-pending-indicator-time-fg)" }}>{t}</span>
    </span>
  );
}
