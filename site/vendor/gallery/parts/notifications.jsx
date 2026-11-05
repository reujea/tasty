import React from "react";
import * as DS from "../../components/index.js";
import { NotificationSummary as Kit_NotificationSummary, RailNotificationSummary as Kit_RailNotificationSummary, notificationSummaryLabel as Kit_notificationSummaryLabel } from "../../ui_kits/terminal/notification_summary.jsx";
import { useNotifications as Kit_useNotifications, NotificationsPopup as Kit_NotificationsPopup, NOTIFICATION_SAMPLES as Kit_NOTIFICATION_SAMPLES } from "../../ui_kits/terminal/overlays/notifications.jsx";
import { Spec, Stage, Meta, Note, Dont } from "./specimen.jsx";

// Tasty Gallery — Notifications (2026-10-17).
// Two mounts: GalleryNotifications_Lnb (Layouts › "Notifications summary") and
// .Popup (Overlays · Windows › "Notifications"). Live kit parts only:
// ui_kits/terminal/notification_summary.jsx, overlays/notifications.jsx, overlays/info_modal.jsx.



const NB = DS.NotificationBadge || (() => null);
const NBG = DS.NotificationBadgeGroup || (() => null);
const K = () => ({ useNotifications: Kit_useNotifications, NotificationSummary: Kit_NotificationSummary, RailNotificationSummary: Kit_RailNotificationSummary, notificationSummaryLabel: Kit_notificationSummaryLabel, NotificationsPopup: Kit_NotificationsPopup, NOTIFICATION_SAMPLES: Kit_NOTIFICATION_SAMPLES });

const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };
const Cap = ({ children }) => <div style={{ ...mono, marginBottom: "var(--tasty-space-xs)" }}>{children}</div>;
const Themed = ({ attr, children, style }) => (
  <div {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)",
    background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0, ...style }}>{children}</div>
);
const stage = (children, style) => <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start", ...style }}>{children}</Stage>;

const LATEST = {
  err: { level: "error", title: "Build failed: cargo build exited with status 101", time: "2m" },
  warn: { level: "warning", title: "Disk usage above 85%", time: "9m" },
  long: { level: "error", title: "Remote attach lost: connection reset by peer while syncing the layout of prod-web", time: "1h" },
};
const LNB_STATES = [
  ["none", { counts: {} }],
  ["three levels", { counts: { error: 2, warning: 1, info: 5 }, latest: LATEST.err }],
  ["one level only", { counts: { warning: 3 }, latest: LATEST.warn }],
  ["long summary · ellipsis + tooltip", { counts: { error: 1, info: 2 }, latest: LATEST.long }],
  ["many · 99+", { counts: { error: 12, warning: 140, info: 1200 }, latest: LATEST.err }],
  ["hover", { counts: { error: 2, warning: 1, info: 5 }, latest: LATEST.err, hover: true }],
  ["popup open", { counts: { error: 2, warning: 1, info: 5 }, latest: LATEST.err, open: true }],
];

function LnbBed({ children }) {
  const { Button, Icon } = DS;
  return (
    <div style={{ width: 212, display: "flex", flexDirection: "column", background: "var(--tasty-bg-sidebar)", border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      <div style={{ height: "var(--tasty-size-16)" }} />
      {children}
      <div style={{ padding: "var(--tasty-space-sm)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)" }}>
        <Button variant="ghost" size="sm" block leadingIcon={<Icon name="tools" />} style={{ justifyContent: "flex-start" }}>Tools</Button>
        <Button variant="ghost" size="sm" block leadingIcon={<Icon name="plug" />} style={{ justifyContent: "flex-start" }}>Plugins</Button>
        <Button variant="ghost" size="sm" block leadingIcon={<Icon name="settings" />} style={{ justifyContent: "flex-start" }}>Settings</Button>
      </div>
    </div>
  );
}
function RailBed({ children }) {
  const { IconButton, Icon } = DS;
  return (
    <div style={{ width: 52, display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-size-2)", padding: "var(--tasty-space-sm) 0",
      background: "var(--tasty-bg-sidebar)", border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)" }}>
      {children}
      <IconButton aria-label="Tools"><Icon name="tools" /></IconButton>
      <IconButton aria-label="Plugins"><Icon name="plug" /></IconButton>
      <IconButton aria-label="Settings"><Icon name="settings" /></IconButton>
    </div>
  );
}

function Lnb() {
  const { NotificationSummary, RailNotificationSummary, notificationSummaryLabel } = K();
  if (!NotificationSummary) return <Note>Kit part not loaded (notification_summary.jsx).</Note>;
  return (
    <>
      <Spec id="lnb-summary-expanded-sidebar" title="LNB summary — expanded sidebar"
        when={<>A small block <b>directly above</b> the Tools · Plugins · Settings block, with its own top rule. The whole block is <b>one button</b>: click (or the existing notifications shortcut) toggles the popup. Line 1: bell · <b>Notifications</b> · one count pill per level with unread items, fixed order <b>error › warning › info</b>. Line 2 (only when something is unread): the <b>newest unread</b> title, one line, ellipsis, with its level glyph and elapsed time. With nothing unread the block is one muted line. Workspace rows get <b>no</b> new badge; their blue count stays the surface-attention count.</>}>
        {stage(THEMES.map(([label, attr]) => (
          <Themed key={label} attr={attr}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
              {LNB_STATES.map(([c, p]) => (
                <div key={c}><Cap>{c}</Cap><LnbBed><NotificationSummary {...p} /></LnbBed>
                  <div style={{ ...mono, maxWidth: 212, marginTop: "var(--tasty-space-xs)", whiteSpace: "normal" }}>tooltip: {notificationSummaryLabel(p.counts)}</div></div>
              ))}
            </div>
          </Themed>
        )))}
        <Meta
          specs={[["position", "above the Tools block · own 1px top rule · pad 8"], ["line 1", "24 · bell 16 · label 13 · badges right, gap 4"], ["line 2", "unread > 0 only · newest unread title 12 · glyph 12 muted · time mono 10"], ["empty", "one line · label + bell in text-muted · no pills"], ["click", "toggles the popup (= existing shortcut)"], ["hover / open", "overlay-hover / overlay-active bed"], ["arrival", "counts + line 2 update · popup never opens · focus kept"], ["l10n", "label ellipsises first; pills never shrink"]]}
          tokens={[{ tok: "--tasty-notification-summary-pad", use: "→ space-sm 8" }, { tok: "--tasty-notification-summary-row-height", use: "→ size-24" }, { tok: "--tasty-notification-summary-label-fg", use: "→ text-secondary", color: "var(--tasty-notification-summary-label-fg)" }, { tok: "--tasty-notification-summary-label-fg-empty", use: "→ text-muted", color: "var(--tasty-notification-summary-label-fg-empty)" }, { tok: "--tasty-notification-summary-preview-fg", use: "→ text-secondary", color: "var(--tasty-notification-summary-preview-fg)" }, { tok: "--tasty-notification-summary-bg-hover", use: "→ overlay-hover", color: "var(--tasty-notification-summary-bg-hover)" }, { tok: "--tasty-notification-summary-bg-open", use: "→ overlay-active", color: "var(--tasty-notification-summary-bg-open)" }]} />
        <Dont><b>Don't</b> add the notification counts to workspace rows or rail avatars, and don't sum them with the surface completion / needs-input counts.</Dont>
      </Spec>

      <Spec id="rail-bell-with-stacked-counts" title="Rail — bell with stacked counts"
        when={<>The collapsed rail keeps the same block above Tools, as a <b>44-wide</b> button (wider than the 36 avatar, so <code>99+</code> fits with its glyph). Bell on top, then the level pills <b>stacked</b> error › warning › info, zeros skipped. With nothing unread it is the bell alone at button height. No line 2; the tooltip carries the full counts.</>}>
        {stage(THEMES.map(([label, attr]) => (
          <Themed key={label} attr={attr}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-end" }}>
              {[["none", {}], ["three", { error: 2, warning: 1, info: 5 }], ["one", { info: 4 }], ["99+", { error: 3, warning: 140, info: 1200 }]].map(([c, counts]) => (
                <div key={c}><Cap>{c}</Cap><RailBed><RailNotificationSummary counts={counts} /></RailBed></div>
              ))}
              <div><Cap>hover</Cap><RailBed><RailNotificationSummary counts={{ error: 2, info: 5 }} hover /></RailBed></div>
              <div><Cap>open</Cap><RailBed><RailNotificationSummary counts={{ error: 2, info: 5 }} open /></RailBed></div>
            </div>
          </Themed>
        )))}
        <Meta
          specs={[["box", "44 wide · min 28 · pad 4 when counts show"], ["stack", "bell · pills column, gap 2 · error › warning › info"], ["0", "pill not drawn · all 0 = bell only"], ["cap", "99+ (fits 44 with glyph)"], ["tooltip", "Notifications — 2 errors · 1 warning · 5 info"]]}
          tokens={[{ tok: "--tasty-notification-rail-width", use: "→ size-44" }, { tok: "--tasty-notification-badge-stack-gap", use: "→ size-2" }]} />
      </Spec>

      <Spec id="level-count-badge-notificationbadge" title="Level count badge — NotificationBadge"
        when={<>A 16 pill with the <b>level glyph</b> and the count, so the level reads without colour: <b>error</b> red (critical failure) · <b>warning</b> yellow (non-critical failure) · <b>info</b> the completion blue. Each pill has an aria-label / tooltip ("3 errors"). <code>0</code> draws nothing; above 99 reads <code>99+</code>.</>}>
        {stage(THEMES.map(([label, attr]) => (
          <Themed key={label} attr={attr}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
            {["error", "warning", "info"].map((lv) => (
              <div key={lv} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-md)" }}>
                <span style={{ ...mono, width: 56 }}>{lv}</span>
                {[1, 9, 42, 99, 100].map((n) => <NB key={n} level={lv} count={n} />)}
                <span style={mono}>0 → (none)</span>
              </div>
            ))}
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-md)" }}><span style={{ ...mono, width: 56 }}>group</span><NBG error={2} warning={1} info={14} /><NBG warning={3} /></div>
          </Themed>
        )))}
        <Meta
          specs={[["size", "16 high · min 16 · pad-x 4 · pill radius"], ["glyph", "10 · alertCircle / alertTriangle / info · gap 2"], ["number", "mono 10 bold"], ["order", "error › warning › info"], ["0", "not drawn"], ["cap", "> 99 → 99+"]]}
          tokens={[{ tok: "--tasty-notification-badge-error-bg", use: "→ notify-error → accent-danger", color: "var(--tasty-notification-badge-error-bg)" }, { tok: "--tasty-notification-badge-warning-bg", use: "→ notify-warning-fill → Mocha notify-warning · Latte yellow 50% over white (2026-10-19)", color: "var(--tasty-notification-badge-warning-bg)" }, { tok: "--tasty-notification-badge-warning-fg", use: "→ notify-warning-fg → Mocha text-on-accent · Latte neutral-1100 — Latte 5.01:1 (was 2.62)", color: "var(--tasty-notification-badge-warning-fg)" }, { tok: "--tasty-notification-badge-info-bg", use: "→ notify-info → attention-completion", color: "var(--tasty-notification-badge-info-bg)" }, { tok: "--tasty-notification-badge-error/info-fg", use: "→ notify-*-fg → text-on-accent", color: "var(--tasty-notification-badge-info-fg)" }, { tok: "--tasty-notification-badge-glyph-size", use: "→ size-10" }, { tok: "--tasty-notification-badge-max", use: "99" }]} />
      </Spec>
    </>
  );
}

// ── Popup / fullscreen ─────────────────────────────────────────────
function Popup() {
  const { NotificationsPopup, NOTIFICATION_SAMPLES } = K();
  if (!NotificationsPopup) return <Note>Kit part not loaded (overlays/notifications.jsx).</Note>;
  const S = NOTIFICATION_SAMPLES;
  const Live = ({ wide }) => {
    const n = K().useNotifications();
    return <NotificationsPopup items={n.items} expandedId={n.expandedId} fullscreen={wide} onToggle={n.onToggle} onMarkRead={n.onMarkRead} onMarkAllRead={n.onMarkAllRead} />;
  };
  return (
    <>
      <Spec id="notifications-popup-352-400" title="Notifications popup — 352 × 400"
        when={<>The existing popup, extended. Shared title bar (centred title, <b>fullscreen</b> fit + <b>close</b>). A 28 toolbar shows the same unread level pills as the LNB and <b>Mark all read</b>. The list is newest first. Each item: level glyph · title · elapsed time · unread dot / body (2 lines) / source · <b>✓</b> mark read · <b>Jump</b>. Unread = semibold title + blue dot; read = regular title, muted body. A source that has closed keeps its record, gets a <b>· closed</b> tag and a disabled Jump ("Source closed"). The first frame is live: click a row, ✓ or Mark all read.</>}>
        {stage(THEMES.map(([label, attr]) => (
          <Themed key={label} attr={attr}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
              <div><Cap>live · unread + read mixed</Cap><Live /></div>
              <div><Cap>long body expanded · hover on next row</Cap><NotificationsPopup items={S.map((n, i) => (i === 0 ? Object.assign({}, n, { unread: false }) : n))} expandedId="n1" hoverId="n2" /></div>
              <div><Cap>empty</Cap><NotificationsPopup items={[]} /></div>
            </div>
          </Themed>
        )))}
        <Meta
          specs={[["frame", "352 × 400 · title 28 · toolbar 28 · list scrolls"], ["order", "newest first"], ["item", "pad 12 / 8 · glyph 14 · title 13 · body 12 · meta mono 10 · 1px rule"], ["unread", "title semibold text-primary · blue dot (badge-dot-size)"], ["read", "title text-secondary · body text-muted · no dot"], ["long body", "clamp 2 · More / Less (only when cut) · row click toggles · expanded = full, wraps, selectable"], ["source closed", "· closed tag · Jump disabled, tooltip Source closed"], ["empty", "No notifications. · No unread · Mark all read disabled"]]}
          tokens={[{ tok: "--tasty-notifications-popup-height", use: "→ size-400" }, { tok: "--tasty-notifications-toolbar-height", use: "→ control-height 28" }, { tok: "--tasty-notification-item-title-fg", use: "→ text-primary (unread)", color: "var(--tasty-notification-item-title-fg)" }, { tok: "--tasty-notification-item-title-fg-read", use: "→ text-secondary", color: "var(--tasty-notification-item-title-fg-read)" }, { tok: "--tasty-notification-item-body-fg-read", use: "→ text-muted", color: "var(--tasty-notification-item-body-fg-read)" }, { tok: "--tasty-notification-item-unread-dot", use: "→ notify-unread → accent-primary", color: "var(--tasty-notification-item-unread-dot)" }, { tok: "--tasty-notification-item-closed-fg", use: "→ text-placeholder", color: "var(--tasty-notification-item-closed-fg)" }, { tok: "--tasty-notification-item-more-fg", use: "→ accent-primary", color: "var(--tasty-notification-item-more-fg)" }, { tok: "--tasty-notification-item-body-lines", use: "2" }]} />
      </Spec>

      <Spec id="fullscreen-stage-same-list-wide" title="Fullscreen stage — same list, wide"
        when={<>The fit button moves the same list onto the fullscreen stage. Content, order, actions and read rules are identical; only the body clamp grows to <b>3 lines</b> and titles get the width.</>}>
        {stage(THEMES.map(([label, attr]) => (
          <Themed key={label} attr={attr} style={{ flex: "1 1 520px" }}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · fullscreen</span>
            <div style={{ height: 440, minWidth: 0 }}><NotificationsPopup items={S} fullscreen selectedId="n2" /></div>
          </Themed>
        )))}
        <Meta specs={[["stage", "fills the fullscreen stage · no shadow"], ["body clamp", "3 lines"], ["selection", "keyboard row = surface-active bed"]]}
          tokens={[{ tok: "--tasty-notification-item-body-lines-wide", use: "3" }, { tok: "--tasty-notification-item-bg-selected", use: "→ surface-active", color: "var(--tasty-notification-item-bg-selected)" }]} />
      </Spec>

      <Spec id="notification-warning-glyph" title="Warning level glyph — Latte contrast, rest and hover"
        when={<>The warning glyph is the only thing in a row that says “warning” (no level word in title, body or meta), so it must clear the <b>3:1</b> non-text floor on both the resting row (bg-panel) and the hover bed (bg-panel + overlay-hover). <span className="tok">--tasty-notification-item-warning-glyph</span> now points at the existing <span className="tok">--tasty-accent-warning-glyph</span> instead of notify-warning. Mocha resolves to the same yellow as before; Latte gets the darker warning-glyph ink. Read and unread use the same colour (level is identity, not state). Error and info glyphs, the count badge and accent-warning are unchanged. Popup and fullscreen render the same NotificationItem, so both lists follow.</>}>
        {stage(THEMES.map(([label, attr]) => {
          const NI = DS.NotificationItem;
          const W = { level: "warning", title: "Disk usage above 85% on /var/lib/warehouse", body: "The nightly ETL keeps running, but the next snapshot may not fit.", source: "data-pipeline › ingest", time: "9m" };
          const rows = [["warning · unread · rest", Object.assign({}, W, { unread: true })], ["warning · unread · hover", Object.assign({}, W, { unread: true, hover: true })], ["warning · read · rest", W], ["warning · read · hover", Object.assign({}, W, { hover: true })],
            ["error · unread (reference)", { level: "error", unread: true, title: "Build failed: cargo build exited with status 101", body: "error[E0308]: mismatched types", source: "api-gateway › build", time: "2m" }], ["info · read (reference)", { level: "info", title: "Deploy to staging finished", body: "All checks passed.", source: "agents-prod › deploy", time: "14m" }]];
          const list = (wide) => (
            <div style={{ width: wide ? "var(--tasty-size-560)" : "var(--tasty-notifications-popup-width)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              {rows.map(([c, p]) => <div key={c}><div style={{ ...mono, padding: "var(--tasty-space-xs) var(--tasty-notification-item-pad-x) 0" }}>{c}</div>{NI && <NI {...p} wide={wide} />}</div>)}
            </div>
          );
          return (
            <Themed key={label} attr={attr}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>
              <div style={{ display: "flex", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
                <div><Cap>popup list</Cap>{list(false)}</div>
                <div><Cap>fullscreen list</Cap>{list(true)}</div>
              </div>
            </Themed>
          );
        }))}
        <Meta specs={[["glyph", "14 · alertTriangle · same ink read / unread · rest / hover"], ["ground", "bg-panel · hover = + overlay-hover (Mocha white 8% · Latte black 8%)"], ["target", "3:1 non-text, specified colours (AA edge pixels not counted)"]]}
          tokens={[{ tok: "--tasty-notification-item-warning-glyph", use: "was notify-warning → now accent-warning-glyph (Mocha = accent-warning · Latte yellow 50% toward n1100 ≈ #966f43)", color: "var(--tasty-notification-item-warning-glyph)" }, { tok: "--tasty-notify-warning", use: "unchanged (accent-warning) — no longer read by the list glyph", color: "var(--tasty-notify-warning)" }, { tok: "--tasty-notification-badge-warning-bg", use: "unchanged (notify-warning-fill)", color: "var(--tasty-notification-badge-warning-bg)" }, { tok: "--tasty-notification-item-error-glyph / info-glyph", use: "unchanged", color: "var(--tasty-notification-item-error-glyph)" }]} />
        <Note><b>Contrast (WCAG relative luminance).</b> Latte, glyph #966f43 on base #eff1f5: <b>3.99:1</b> rest · hover <b>3.35:1</b> CSS (base × 0.92) / <b>3.35:1</b> egui bytes (premultiplied black (0,0,0,20): base × 235/255, rounded). Before (#df8e1d): 2.31 rest · 1.94 hover. Mocha, glyph #f9e2af (unchanged) on base #1e1e2e: 12.91:1 rest · hover 10.20:1 CSS (white 0.08) / 10.21:1 egui (premultiplied (20,20,20,20): 20 + base × 235/255).</Note>
        <Note><b>Why reuse accent-warning-glyph.</b> It already is “a warning glyph that must clear 3:1 on light grounds” (plugin trust box, notice column), and it clears the hover bed too. A notification-only role would hold the same value. The badge keeps its own fill role (notify-warning-fill), so it is unaffected.</Note>
      </Spec>

      <Spec id="read-rules-and-arrival" title="Read rules and arrival">
        {stage(<span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)", maxWidth: 720, textWrap: "pretty" }}>Opening or closing the popup marks nothing read. An item becomes read by its <b>✓</b> or by <b>expanding</b> it. <b>Mark all read</b> covers this window's list only. When the last unread item of a source surface is read, the existing attention-clear rule runs; hard occupancy protection is unchanged. A new notification updates the pills and the LNB line 2 only: it is inserted at the top of the list without moving selection, scroll offset (when scrolled down) or keyboard focus, and never opens the popup.</span>, { minHeight: 0 })}
        <Meta specs={[["open / close", "no read change"], ["per item", "✓ · expand"], ["Mark all read", "this window's list"], ["attention", "existing clear rule after the source's last unread is read"], ["Jump", "explicit only · disabled when source closed"], ["arrival", "no auto-open · no focus / selection / scroll change"]]} />
      </Spec>
    </>
  );
}

export { Lnb, Popup };
