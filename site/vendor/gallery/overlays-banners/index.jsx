import React from "react";
import { PageStates, Views, View } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { IconButton, Kbd, Tag, Button as BButton, Icon as BIcon, Badge as BBadge, BadgeGroup as BBadgeGroup, Spinner as BSpinner } from "../../components/index.js";
import { HtmlSizeBanner, HtmlAddressBar } from "../../ui_kits/terminal/overlays/html_address_bar.jsx";
import { HtmlRemoteSurfaceG, HtmlRemoteBanner, HtmlRemoteMarker } from "../../ui_kits/terminal/overlays/html_remote_banner.jsx";
import { HtmlScriptBanner, HtmlSurfaceG, TermSurfaceG } from "../../ui_kits/terminal/overlays/html_script_banner.jsx";
import { BannerScope, MouseCaptureBannerG, MouseCaptureHitZone, BlacklistEditorG, BannerShellG, TtlBannerG, StackDemoG, BannerMoreMenuG, BannerMoreDemoG, ic } from "../parts/overlay-frames.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty Gallery — Overlays · Banners (the 4th overlay family)
// One of the five Overlays sub-pages (dialogs · windows · popups · banners · tutorial). Frame components are shared from
// parts/overlay-frames.jsx (parts/overlay-frames.jsx); this file holds only the
// page's specimens + nav. See the other overlays-*.jsx for the rest.







// 2026-10-07 — auto-attach mapping refused. Workspace-scope banner + a persistent sidebar row mark.
const REFUSAL = {
  self: { target: "127.0.0.1:7420", reason: "This mapping points at this Tasty, so it was not attached." },
  profile: { target: "prod-web", reason: "No SSH profile with this name." },
  resolve: { target: "build-eu:7420", reason: "The endpoint could not be resolved." },
};
// Narrow rule (2026-10-09 b10, banner-narrow-scope): under --tasty-banner-narrow-below (judged on the SCOPE content width) the
// action button wraps under the text, starting at the body's left edge; × stays in the top-right like the HTML banner.
function BannerActionsG({ narrow, action, children }) {
  const x = <IconButton size="sm" aria-label="Dismiss">{ic.x}</IconButton>;
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-banner-gap)", padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
      {children[0]}
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-banner-gap)" }}>
        {children[1]}
        {narrow && <span style={{ display: "flex" }}>{action}</span>}
      </div>
      {narrow ? <span style={{ flex: "none", display: "flex" }}>{x}</span> : <span style={{ flex: "none", display: "flex", gap: "var(--tasty-space-xs)", alignItems: "center" }}>{action}{x}</span>}
    </div>
  );
}
function AttachRefusalBannerG({ kind = "self", narrow }) {
  const r = REFUSAL[kind];
  return (
    <BannerShellG>
      <BannerActionsG narrow={narrow} action={<BButton variant="secondary" size="sm" style={{ background: "var(--tasty-banner-button-bg)", borderColor: "var(--tasty-banner-button-border)" }}>Remove mapping</BButton>}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-glyph-offset)", color: "var(--tasty-accent-warning-raised)" }}><BIcon name="alertTriangle" size="var(--tasty-icon-size-md)" /></span>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)" }}>
          <span style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)" }}>Remote not attached — <span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-font-weight-normal)" }}>{r.target}</span></span>
          <span style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)" }}>{r.reason} Change or remove the mapping for this workspace.</span>
        </div>
      </BannerActionsG>
    </BannerShellG>
  );
}
// 2026-10-09 — attach mirror terminal-size sync failed (after the one automatic retry). Same family as the refusal banner.
// Body = TWO lines (b10, attach-sync-banner-body-cut): names line (one line, names ellipsise) + fixed copy line (wraps, never cut).
// b11 (attach-sync-name-floor): the 40 floor applies only while shrinking — box = min(natural, max(floor, shrunk)).
// A name shorter than the floor keeps its own width, so ", " / " +n" follow right after it.
function SyncNameG({ n }) {
  const ref = React.useRef(null);
  const [floor, setFloor] = React.useState("var(--tasty-attach-sync-name-min-width)");
  React.useLayoutEffect(() => {
    const el = ref.current; if (!el) return;
    const fl = parseFloat(getComputedStyle(el).getPropertyValue("--tasty-attach-sync-name-min-width")) || 40;
    const prev = el.style.minWidth; el.style.minWidth = "0px";
    const nat = el.scrollWidth; el.style.minWidth = prev;
    if (nat < fl) setFloor(nat + "px");
  }, [n]);
  return <span ref={ref} style={{ flex: "0 1 auto", minWidth: floor, maxWidth: "var(--tasty-attach-sync-name-max-width)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--tasty-text-secondary-raised)" }}>{n}</span>;
}
// b11 (attach-sync-duplicate-names): equal display names collapse into one entry + a muted “×k” that never shrinks.
// “N surfaces” still counts surfaces; +n counts the remaining entries.
function AttachSizeSyncBannerG({ names = ["build"], retrying = false, narrow }) {

  const many = names.length > 1;
  const groups = [];
  names.forEach((n) => { const g = groups.find((x) => x.n === n); g ? g.k++ : groups.push({ n, k: 1 }); });
  const shown = groups.slice(0, 2);
  const rest = groups.length - shown.length;
  const body = { fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)" };
  return (
    <BannerShellG>
      <BannerActionsG narrow={narrow} action={<BButton variant="secondary" size="sm" disabled={retrying} leadingIcon={retrying ? <BSpinner size="var(--tasty-icon-size-sm)" /> : undefined}
          style={retrying ? undefined : { background: "var(--tasty-banner-button-bg)", borderColor: "var(--tasty-banner-button-border)" }}>{many ? "Retry all" : "Retry"}</BButton>}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-glyph-offset)", color: "var(--tasty-attach-sync-glyph)" }}><BIcon name="alertTriangle" size="var(--tasty-icon-size-md)" /></span>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)" }}>
          <span style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)" }}>Couldn't sync the terminal size with the remote</span>
          <span style={{ ...body, display: "flex", alignItems: "baseline", whiteSpace: "nowrap", minWidth: 0, overflow: "hidden" }}>
            {many ? <span style={{ flex: "none" }}>{names.length} surfaces —&nbsp;</span> : null}
            {shown.map(({ n, k }, i) => <React.Fragment key={n}>{i > 0 ? <span style={{ flex: "none" }}>,&nbsp;</span> : null}<SyncNameG n={n} />{k > 1 ? <span style={{ flex: "none", color: "var(--tasty-banner-body-fg)" }}>&nbsp;×{k}</span> : null}</React.Fragment>)}
            {rest > 0 ? <span style={{ flex: "none" }}>&nbsp;+{rest}</span> : null}
          </span>
          <span style={body}>The remote may still be using the old size.</span>
        </div>
      </BannerActionsG>
    </BannerShellG>
  );
}
function RefusalRowG({ name, active, refused, move, ni, done }) {
  return (
    <div title={refused ? "Remote not attached — 127.0.0.1:7420 points at this Tasty." : undefined} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-size-28)", padding: "0 var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)",
      background: active ? "var(--tasty-surface-active)" : "transparent", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", fontSize: "var(--tasty-font-size-body)" }}>
      <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-status-idle)", flex: "none" }} />
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
      {move && <span aria-label="Move source inside" style={{ display: "inline-flex", flex: "none", color: "var(--tasty-move-source-glyph)" }}><BIcon name="move" size="var(--tasty-move-source-glyph-size)" /></span>}
      {refused && <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-attach-refusal-glyph)" }}><BIcon name="alertTriangle" size="var(--tasty-icon-size-sm)" /></span>}
      {(ni != null || done != null) && BBadgeGroup && (
        <BBadgeGroup>
          {ni != null && <BBadge variant="warning">{ni}</BBadge>}
          {done != null && <BBadge variant="primary">{done}</BBadge>}
        </BBadgeGroup>
      )}
    </div>
  );
}

// Collapsed rail: the refusal chip takes the free top-left corner (TR attention dot · BR mirror chip · BL move chip).
function RefusalRailG({ ch, refused, dot, move }) {
  const chip = { position: "absolute", pointerEvents: "none", display: "inline-flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--tasty-radius-pill)", background: "var(--tasty-bg-sidebar)" };
  return (
    <div title={refused ? "Remote not attached — 127.0.0.1:7420 points at this Tasty." : undefined} style={{ position: "relative", width: "var(--tasty-size-28)", height: "var(--tasty-size-28)", display: "inline-flex", alignItems: "center", justifyContent: "center",
      borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-font-weight-bold)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>
      {ch}
      {refused && <span aria-label="Remote not attached" style={{ ...chip, top: "calc(-1 * var(--tasty-border-width))", left: "calc(-1 * var(--tasty-border-width))", width: "var(--tasty-attach-refusal-chip-size)", height: "var(--tasty-attach-refusal-chip-size)", color: "var(--tasty-attach-refusal-glyph)" }}><BIcon name="alertTriangle" size="var(--tasty-attach-refusal-chip-glyph-size)" /></span>}
      {dot && <span aria-hidden style={{ ...chip, top: "var(--tasty-size-1)", right: "var(--tasty-size-1)", width: "var(--tasty-status-dot-size-compact)", height: "var(--tasty-status-dot-size-compact)", background: `var(--tasty-status-dot-${dot})`, boxShadow: "0 0 0 var(--tasty-status-dot-ring-width) var(--tasty-status-dot-ring)" }} />}
      {move && <span aria-label="Move source inside" style={{ ...chip, bottom: "calc(-1 * var(--tasty-border-width))", left: "calc(-1 * var(--tasty-border-width))", width: "var(--tasty-move-source-chip-size)", height: "var(--tasty-move-source-chip-size)", color: "var(--tasty-move-source-glyph)" }}><BIcon name="move" size="var(--tasty-move-source-chip-glyph-size)" /></span>}
    </div>
  );
}

const NAV = [
  { id: "banner", label: "Banner" },
  { id: "shellintegration", label: "Shell integration missing" },
  { id: "mousecapture", label: "Mouse capture" },
  { id: "more", label: "Banner more menu" },
  { id: "blacklist", label: "Capture blacklist" },
  { id: "htmlscript", label: "HTML script notice" },
  { id: "attachrefusal", label: "Auto-attach refused" },
  { id: "attachsizesync", label: "Attach size sync failed" },
  { id: "htmlremotesize", label: "HTML mirror size notices" },
  { id: "htmlremote", label: "HTML remote content notice" },
  { id: "bodyink", label: "Body ink · Mocha / Latte" },
];

// 2026-11-01 — remote content blocked. §3 variants 1–10 of the request.
const RC_VARIANTS = [
  ["1 · remote CSS + images, no scripts — blocked", { remote: "blocked" }],
  ["2 · loading a new document — Allow disabled (tooltip top)", { remote: "loading" }],
  ["3 · after Allow — reloading", { remote: "reloading" }],
  ["4 · allowed — no banner, globe marker in the address field", { remoteMarker: "allowed", page: "styled" }],
  ["5 · × dismissed — still blocked, globeOff marker (click → banner)", { remoteMarker: "blocked" }],
  ["6a · scripts + remote — both notices, remote first", { remote: "blocked", script: "blocked" }],
  ["6b · scripts allowed only — remote notice stays", { remote: "blocked", scriptMarker: "allowed" }],
  ["6c · remote allowed only — script notice stays", { script: "blocked", remoteMarker: "allowed", page: "styled" }],
  ["7 · only network-src scripts — remote notice only (nothing runnable yet)", { remote: "blocked" }],
  ["7′ · …after remote Allow — scripts now runnable, script notice (plain body)", { script: "blocked", remoteMarker: "allowed", page: "styled" }],
  ["8a · missed by the scan, block OBSERVED (Windows) — marker only", { remoteMarker: "blocked" }],
  ["8b · missed by the scan, no OS signal (macOS) — nothing to show", {}],
  ["10 · global “Allow remote content” on — no notice, no marker", { page: "styled" }],
];
function RcThemeGrid({ label, attr }) {

  return (
    <div {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-md)", alignItems: "flex-start" }}>
        {RC_VARIANTS.map(([cap, p]) => (
          <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-size-480)" }}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
            <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlRemoteSurfaceG {...p} height={p.remote && p.script ? 400 : 300} /></div>
          </div>
        ))}
        {[["9a · narrow (360) — variant 1", { remote: "blocked" }, 340], ["9b · narrow (360) — variant 6a", { remote: "blocked", script: "blocked" }, 520]].map(([cap, p, h]) => (
          <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
            <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlRemoteSurfaceG {...p} narrow width="var(--tasty-size-360)" height={h} /></div>
          </div>
        ))}
      </div>
    </div>
  );
}

// 2026-11-05 — body ink on the banner shell, every banner kind in both themes.
function BodyInkGrid({ label, theme }) {

  const rows = [
    ["mouse capture", <MouseCaptureBannerG />],
    ["HTML script", <HtmlScriptBanner />],
    ["HTML script · reloading", <HtmlScriptBanner state="reloading" />],
    ["HTML remote content", <HtmlRemoteBanner />],
    ["HTML mirror · large file, more waiting", <HtmlSizeBanner more={2} />],
    ["HTML mirror · cumulative limit", <HtmlSizeBanner kind="limit" />],
    ["auto-attach refused", <AttachRefusalBannerG />],
    ["attach size sync · names line", <AttachSizeSyncBannerG names={["build", "build", "logs"]} />],
    ["shell integration missing · TTL", <TtlBannerG ttl={10} glyph={<BIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-accent-warning-raised)" title="Shell integration not detected" body="Command-completion hooks (docker build, just run, etc.) need OSC 133 shell integration. See docs/features/hooks for setup." />],
  ];
  return (
    <div data-theme={theme} style={{ flex: "1 1 var(--tasty-size-440)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      {rows.map(([cap, el]) => (
        <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
          <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
          {el}
        </div>
      ))}
    </div>
  );
}

function RemainingInkGrid({ label, theme }) {

  const rows = [
    ["TTL countdown", <TtlBannerG ttl={10} glyph={<BIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-accent-warning-raised)" title="Shell integration not detected" />],
    ["⋯ menu · program name unknown · reason line", <BannerMoreMenuG app={null} />],
    ["large file · ghost Decline", <HtmlSizeBanner />],
    ["cumulative limit · ghost Stop", <HtmlSizeBanner kind="limit" />],
    ["long title · wraps, no clamp", <HtmlSizeBanner title="Long title sample: a longer translation or a future string wraps to as many lines as it needs and is never cut off, whether it runs to two lines, to three, or to more than that at this card width" />],
    ["sample title ink", <TtlBannerG />],
  ];
  return (
    <div data-theme={theme} style={{ flex: "1 1 var(--tasty-size-440)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      {rows.map(([cap, el]) => (
        <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
          <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
          {el}
        </div>
      ))}
    </div>
  );
}

// Ink over its composited ground, token by token. Tag and Kbd rows are swatches because their live specimens come from
// components/bundle.js; the size notice beside them is live and reads the banner button token directly.
const CLOSED_INK = [
  ["Banner Secondary", "banner-button-bg", null, "text-primary", "Allow", "6.31", "7.06"],
  ["Kbd", "kbd-bg", null, "kbd-fg", "Ctrl", "7.10", "5.17"],
  ["Kbd +", "surface-raised", null, "kbd-plus-fg", "+", "5.65", "5.17"],
  ["Tag default", "tag-bg", null, "tag-fg", "terminal", "7.10", "5.17"],
  ["Tag success", "tag-bg", null, "tag-success-fg", "running", "8.46", "4.91"],
  ["Tag warning", "tag-bg", null, "tag-warning-fg", "3.2 MB", "9.89", "5.25"],
  ["Tag attention", "tag-bg", null, "tag-attention-fg", "attention", "7.10", "5.12"],
  ["Tag danger", "tag-bg", null, "tag-danger-fg", "danger", "5.43", "4.79"],
  ["Tag info", "tag-bg", null, "tag-info-fg", "built-in", "8.08", "4.87"],
  ["Menu row, hover", "surface-raised", "overlay-hover", "menu-item-fg-hover", "Don't ask for this app", "6.78", "6.36"],
  ["Ghost on raised, hover", "surface-raised", "overlay-hover", "button-ghost-raised-fg-hover", "Decline", "6.78", "6.36"],
  ["Selected row", "surface-active", null, "text-on-active", "report.html", "4.62", "5.39"],
  ["Selected row, hover", "surface-active", "overlay-hover", "text-on-active", "report.html", "3.79", "4.57"],
  ["Accent glyph on selected row", "surface-active", null, "accent-primary-on-active", "✓", "3.17", "3.83"],
  ["Secondary on raised, pressed (3:1 floor)", "surface-raised", "overlay-active", "text-primary", "Add", "5.97", "3.98"],
];
function ClosedInkTable({ label, theme }) {
  const col = theme === "latte" ? 6 : 5;
  return (
    <div data-theme={theme} style={{ flex: "1 1 var(--tasty-size-440)", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{label}</span>
      {CLOSED_INK.map((r) => (
        <div key={r[0]} data-ink-row={r[0]} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)" }}>{r[0]}</span>
          <span data-ink-sample="" style={{ flex: "none", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)", fontSize: "var(--tasty-font-size-caption)", whiteSpace: "nowrap",
            color: "var(--tasty-" + r[3] + ")", backgroundColor: "var(--tasty-" + r[1] + ")",
            backgroundImage: r[2] ? "linear-gradient(var(--tasty-" + r[2] + "), var(--tasty-" + r[2] + "))" : "none" }}>{inkSample(r[4])}</span>
          <span style={{ flex: "none", width: "var(--tasty-size-32)", textAlign: "right", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)" }}>{r[col]}</span>
        </div>
      ))}
    </div>
  );
}

function Page() {
  return (
    <>
<Section id="banner" title="Banner — the floating top notice">
        <Spec id="banner-shell-the-4th-overlay-family" title="Banner shell — the 4th overlay family"
          when={<>After Modal / Popup / Toast, <b>Banner</b> is the fourth overlay concept. It floats at the <b>top of a scope's content area</b> (View / Workspace / Pane / Tab / Surface) and <b>never covers the tab bar</b> — full width minus <b>8px</b> side margins, <b>8px</b> top margin, <b>no bottom margin</b>, <b>8px</b> radius, variable height. Unlike a Toast it <b>consumes its own mouse</b> and can carry <b>actions</b>; unlike a Popup it takes <b>no keyboard focus</b> and has no titlebar/drag. Fired only by <b>direct user action</b> — never from agent/IPC. The example below is the first use: a TUI mouse-capture notice.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <BannerScope height={250}><MouseCaptureBannerG /></BannerScope>
          </Stage>
          <Meta
            specs={[["family", "Modal · Popup · Toast · Banner"], ["position", "top of content, below the tab bar"], ["width", "100% − 16px (8px each side)"], ["margin", "8 top / 8 sides · 0 bottom"], ["radius", <>8px (<span className="tok">--tasty-banner-radius</span>)</>], ["height", "variable — per banner"], ["lifetime", "no action → TTL (auto-dismiss) · has action → until the action or ×"], ["fires on", "user action only (never IPC/agent)"]]}
            tokens={[{ tok: "--tasty-banner-bg", use: "surface0 fill", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-border", use: "1px edge", color: "var(--tasty-banner-border)" }, { tok: "--tasty-banner-shadow", use: "floating lift" }, { tok: "--tasty-banner-margin", use: "8px top/sides" }]} />
          <Do><b>Do</b> reach for a Banner when there's an <b>action</b> to attach (Popup = independent feature, Banner = info + action, Toast = info only). If the message is short and action-less, use a <b>Toast</b> instead.</Do>
          <Note>There is <b>no</b> Info/Warning/Error kind — each banner's <b>id is its kind</b>, and it styles its own leading glyph/severity. These tokens are only the shared shell.</Note>
        </Spec>
        <Spec id="lifetime-ttl-follows-whether-the-banner-has-an-action" title="Lifetime — TTL follows whether the banner has an action"
          when={<>A banner exists only while it is valid. While valid: a banner that offers <b>no user action</b> has a <b>TTL</b> and closes on its own; a banner that offers <b>an action</b> (an inline button or a ⋯ menu row) has <b>no TTL</b> and stays until the user picks the action or closes it. The <b>× is not an action</b> — every banner has one, so it never makes a banner persistent. A developer may make an exception for a specific banner id; the exception and its reason are written as a code comment at the banner's definition.</>}>
          <Meta
            specs={[["no action", "TTL — countdown, auto-dismiss at 0"], ["has action", "no TTL — until the action or ×"], ["×", "not an action"], ["exception", "developer-directed, code comment at the definition (none in use for mouse capture since 2026-11-03)"], ["plugin banners", "same rule as authoring guidance (host can't see plugin content)"]]}
            tokens={[{ tok: "--tasty-banner-countdown-fg", use: "TTL seconds", color: "var(--tasty-banner-countdown-fg)" }]} />
          <Note>Host banners today: <b>TTL</b> — preset applied (6 s), shell integration missing (10 s). <b>No TTL</b> — mouse capture (⋯ rows), auto-attach refused (Remove mapping), attach size sync (Retry), HTML script notice (Allow), HTML remote content (Allow), HTML mirror size / aggregate (Allow · Continue).</Note>
        </Spec>
        <Spec id="dismiss-ttl-countdown-top-right-one-slot" title="Dismiss & TTL countdown — top-right, one slot"
          when={<>The top-right corner is an <b>affordance column</b> holding <b>one</b> dismiss slot (a banner that carries a ⋯ overflow trigger adds it to the left of that slot — see <i>Banner more menu</i>). A banner <b>with an action</b> (no TTL) keeps its <b>×</b> <b>hidden until you hover</b> the banner. An <b>action-less</b> banner (TTL) shows a <b>seconds countdown</b> there instead, flipping to the × on hover; at <b>0</b> it auto-dismisses. The countdown <b>pauses</b> while the banner is hovered or its scope is backgrounded, then resumes. Appear/dismiss is a <b>120ms alpha fade</b> — the banner never moves.</>}>
          <Stage variant="solo center" style={{ flexDirection: "column", gap: 16, padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>has an action → no TTL · hover the banner to reveal ×</div>
              <BannerShellG><div style={{ display: "flex", alignItems: "center", gap: 12, padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
                <span style={{ flex: 1, fontSize: 13, color: "var(--tasty-banner-fg)" }}>Stays until the action or × (× on hover, top-right)</span>
                <BButton variant="secondary" size="sm" style={{ background: "var(--tasty-banner-button-bg)", borderColor: "var(--tasty-banner-button-border)" }}>Retry</BButton>
                <IconButton size="sm" aria-label="Dismiss">{ic.x}</IconButton></div></BannerShellG>
            </div>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>no action → TTL 6s · live countdown · hover to pause + show ×</div>
              <TtlBannerG />
            </div>
          </Stage>
          <Meta
            specs={[["slot", "top-right · one affordance"], ["has action", "no TTL · × hidden → reveal on hover"], ["no action", "TTL · seconds count → × on hover"], ["expiry", "0 → auto-dismiss"], ["pause", "hover · backgrounded scope"], ["motion", "120ms alpha fade, no move"]]}
            tokens={[{ tok: "--tasty-banner-countdown-fg", use: "seconds", color: "var(--tasty-banner-countdown-fg)" }, { tok: "--tasty-banner-countdown-font", use: "mono digits" }, { tok: "--tasty-banner-fade", use: "120ms appear/dismiss" }]} />
        </Spec>
        <Spec id="queue-stacking-one-per-scope-rest-wait" title="Queue & stacking — one per scope, rest wait"
          when={<>A scope shows <b>one</b> banner at a time; others <b>queue</b> (max <b>5</b>). Re-firing the <b>shown</b> id resets its countdown; a <b>queued</b> duplicate id is ignored; a full queue drops new banners. When banners from <b>different scopes</b> coexist, the higher scope (<b>View &gt; Workspace &gt; Pane &gt; Tab &gt; Surface</b>) sits in front at a higher z-index, and the lower one is <b>dimmed to ~40% opacity</b> behind it — only its overhang shows.</>}>
          <Stage variant="solo center" style={{ padding: 24, background: "var(--tasty-bg-app)" }}>
            <StackDemoG />
          </Stage>
          <Meta
            specs={[["per scope", "1 shown + up to 5 queued"], ["dup (shown)", "resets the countdown"], ["dup (queued)", "ignored"], ["queue full", "new banner dropped"], ["z-order", "View > Workspace > Pane > Tab > Surface"], ["recessed", "lower scope → 40% opacity"]]}
            tokens={[{ tok: "--tasty-banner-recessed-opacity", use: "dimmed lower banner", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-bg", use: "both shells", color: "var(--tasty-banner-bg)" }]} />
          <Note>The interactive manager (spawn / queue / background-pause / dismiss-pops-next) lives in the kit specimen <span className="ic">ui_kits/terminal/overlays/banner.html</span>.</Note>
        </Spec>
      </Section>

      <Section id="shellintegration" title="Shell integration missing — action-less, TTL 10 s">
        <Spec id="shell-integration-missing-surface-scope-countdown-in-the-dismiss" title="shell-integration-missing — surface scope, countdown in the dismiss slot"
          when={<>Raised on a terminal surface when command-completion hooks need OSC 133 and no prompt marks were seen. Title + body, <b>no buttons</b>, so under the lifetime rule it is a <b>TTL</b> banner: the top-right slot shows the seconds, flips to × on hover, and the card closes at 0. The countdown starts when the card is shown, pauses while it is hovered or its scope is backgrounded, resumes where it stopped; re-raising the same id while shown resets it to 10. Card, glyph, title and body are unchanged — only the top-right slot changes.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-md)", alignItems: "stretch" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · live countdown · hover to pause + show ×</span>
                <div style={{ width: "100%", maxWidth: "var(--tasty-size-480)" }}>
                  <TtlBannerG ttl={10} glyph={<BIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-accent-warning-raised)"
                    title="Shell integration not detected" body="Command-completion hooks (docker build, just run, etc.) need OSC 133 shell integration. See docs/features/hooks for setup." />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["scope", "surface"], ["actions", "none → TTL"], ["TTL", "10 s — fixed design constant, not a setting"], ["pause", "hover · scope backgrounded"], ["re-raise (shown)", "countdown resets to 10"], ["copy", "unchanged — banner.shell_integration_missing.* (en/ko/ja)"]]}
            tokens={[{ tok: "--tasty-banner-countdown-fg", use: "seconds", color: "var(--tasty-banner-countdown-fg)" }, { tok: "--tasty-banner-countdown-font", use: "mono digits" }]} />
          <Note>Why 10 s: the body is two sentences plus a docs path (~25 words in en, longer in ja) — about 7–8 s of reading at a normal pace, plus time to notice a card that appears over live output. The 2 s toast default is for one-line confirmations and does not apply. Hover pauses the count, so a user who needs longer already has it. No setting: a per-banner TTL setting would be the only one of its kind; if users ask, a single global banner TTL setting is the place for it.</Note>
          <Note>The glyph shown here is the specimen's; keep whatever the implemented card uses — this request changes only the top-right slot.</Note>
        </Spec>
      </Section>

      <Section id="mousecapture" title="Mouse capture — the first banner instance">
        <Spec id="anatomy-states" title="Anatomy & states"
          when={<>The canonical use: a TUI app turns on mouse tracking (DECSET 1000/1002/1003), so drag-to-select and right-click stop working. The banner explains <b>why</b> and the <b>bypass</b>. It is <b>persistent</b> (no TTL — it doesn't time out), fires <b>once per tracking session</b> and only on a <b>real user click</b> (never from agent/IPC). The top-right × is hidden until you hover; dismissing it suppresses the banner for that session. Leading glyph = a mouse (open decision resolved: keep it, in <span className="tok">--tasty-banner-icon-fg</span> tone). Card height is <b>variable</b> — body wraps to 1–3 lines depending on locale.</>}>
          <Stage variant="solo center" style={{ flexDirection: "column", gap: 16, padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>default · hover the card to reveal ×</div>
              <MouseCaptureBannerG />
            </div>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>one-line body (short locale)</div>
              <MouseCaptureBannerG body="Mouse captured — hold Shift to select." />
            </div>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>recessed — dimmed behind a higher-scope banner</div>
              <div data-audit-exempt="dimmed" style={{ opacity: "var(--tasty-banner-recessed-opacity)" }}><MouseCaptureBannerG /></div>
            </div>
          </Stage>
          <Meta
            specs={[["title", <>"Mouse input captured" · 13/600 <span className="tok">--tasty-banner-fg</span></>], ["body", <>caption · <span className="tok">--tasty-banner-body-fg</span> · 1–3 lines</>], ["glyph", <>mouse · <span className="tok">--tasty-banner-icon-fg</span></>], ["affordance", "⋯ + × on hover (no TTL / countdown)"], ["fires", "once per tracking session · user click only"], ["dismiss", "suppresses for the session"]]}
            tokens={[{ tok: "--tasty-banner-bg", use: "card fill", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-icon-fg", use: "mouse glyph", color: "var(--tasty-banner-icon-fg)" }, { tok: "--tasty-banner-recessed-opacity", use: "dimmed variant" }]} />
          <Note>The bypass keys (<span className="ic">Shift+drag</span> / <span className="ic">Shift+Right-click</span>) are the message — keep them in the body. <b>No inline action buttons</b>: the body stays text-only, and the two per-app opt-outs live behind the <b>⋯</b> trigger next to the × (see <i>Banner more menu</i> below).</Note>
        </Spec>
        <Spec id="position-hit-zone-card-consumes-body-passes-through" title="Position & hit-zone — card consumes, body passes through"
          when={<>The banner floats at the <b>top of the surface content area, below the tab bar</b> (never over it), full width minus <b>8px</b> side margins. Crucially, the <b>card rect</b> — not the whole scope — is the mouse-consume + hover zone. Clicks on the <b>surface body below the card pass through to the capturing app</b>, so mouse reporting keeps working everywhere except the card itself.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <MouseCaptureHitZone />
          </Stage>
          <Meta
            specs={[["anchor", "surface content top, under tab bar"], ["margins", "8 top / 8 sides · 0 bottom"], ["consume zone", "the drawn card rect only"], ["below card", "pass-through to the app"], ["focus", "never steals keyboard focus"], ["inactive surface", "first click only focuses the surface"]]}
            tokens={[{ tok: "--tasty-banner-margin", use: "8px gap", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-accent-info", use: "consume-zone label", color: "var(--tasty-accent-info-raised)" }]} />
        </Spec>
      </Section>

      <Section id="more" title="Banner more menu — per-app opt-outs behind ⋯">
        <Spec id="trigger-second-slot-in-the-affordance-column" title="⋯ trigger — second slot in the affordance column"
          when={<>The mouse-capture banner's two per-app opt-outs are <b>not</b> inline buttons — they sit behind a <b>⋯ trigger</b> in the banner's top-right affordance column, <b>left of the ×</b> with a <b>4px</b> gap. Both share the <b>same reveal rule</b>: hidden until the banner is hovered (this banner has no TTL, so no countdown competes for the slot), and the ⋯ additionally <b>stays lit while its menu is open</b>. The card's width and height are unchanged — only the body's right reserve grows from 28 to <b>56px</b>, so a 1-line body still fits on one line.</>}>
          <Stage variant="solo center" style={{ flexDirection: "column", gap: 16, padding: 20, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>idle — no affordance drawn</div>
              <MouseCaptureBannerG />
            </div>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>banner hovered — ⋯ then ×</div>
              <MouseCaptureBannerG forceHover />
            </div>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>menu open — trigger stays lit (active)</div>
              <MouseCaptureBannerG open />
            </div>
          </Stage>
          <Meta
            specs={[["trigger", <>IconButton <b>ghost / sm</b> · 24px · <span className="ic">more</span> glyph</>], ["placement", "affordance column, left of ×"], ["gap", "4px between ⋯ and ×"], ["reveal", "banner hover — same rule as ×"], ["open state", "active (accent) while menu is open"], ["body reserve", "28 → 56px right padding"], ["card size", "unchanged"]]}
            tokens={[{ tok: "--tasty-banner-more-column-gap", use: "4px ⋯ ↔ ×" }, { tok: "--tasty-banner-more-reserve", use: "56px body right padding" }, { tok: "--tasty-overlay-active", use: "open-trigger wash", color: "var(--tasty-overlay-active)" }, { tok: "--tasty-accent-primary", use: "open-trigger glyph", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>New glyph <span className="ic">more</span> (<code>icons/more.svg</code> — three dots, horizontal, same 2px round-cap language as <code>M12 17h.01</code> style glyphs). A unicode <b>⋯</b> was rejected: the system renders every affordance through the <span className="ic">Icon</span> component so it inherits <code>currentColor</code> and the 16px sm size.</Note>
          <Dont><b>Don't</b> keep the trigger permanently visible. A persistent banner with a permanently drawn ⋯ reads as a second dismiss control and adds chrome to a notice that is already at the top of live output — hover is where every other banner affordance lives.</Dont>
        </Spec>
        <Spec id="the-menu-2-rows-anchored-under-the-trigger" title="The menu — 2 rows, anchored under the trigger"
          when={<>Clicking ⋯ opens a headless menu <b>4px below the trigger</b>, <b>right-aligned</b> to it (the trigger sits at the card's right edge, so left-aligning would overflow), at a higher z-order than the banner. It <b>flips above</b> the trigger when the viewport can't fit it below. Same anchored-popup contract as the Tools menu: <b>no scrim</b>, outside click · <Kbd keys="Esc" /> dismiss, ↑↓ move, <Kbd keys="↵" /> select. Selecting a row <b>acts immediately and closes</b> the menu — to run the other one, reopen ⋯.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <BannerMoreDemoG />
          </Stage>
          <Views><View name="more-menu" label="more menu"><BannerMoreDemoG defaultOpen /></View></Views>
          <Meta
            specs={[["rows", "2 × 28px MenuItem, leading icon"], ["anchor", "4px below ⋯, right-aligned"], ["overflow", "flip above the trigger"], ["width", "min 200 · max 288 (elastic)"], ["scrim", "none"], ["dismiss", <>outside click · <span className="ic">Esc</span> · row select</>], ["keyboard", "↑↓ · ↵ · Esc"]]}
            tokens={[{ tok: "--tasty-banner-more-menu-bg", use: "menu fill", color: "var(--tasty-banner-more-menu-bg)" }, { tok: "--tasty-banner-more-menu-border", use: "edge", color: "var(--tasty-banner-more-menu-border)" }, { tok: "--tasty-banner-more-menu-shadow", use: "popover lift" }, { tok: "--tasty-banner-more-menu-offset", use: "4px below trigger" }, { tok: "--tasty-banner-more-menu-min-width", use: "200px floor" }, { tok: "--tasty-banner-more-menu-max-width", use: "288px ceiling" }]} />
          <Note>Both rows write a <b>permanent</b>, per-program Settings entry (copy: see <i>Row copy</i> below): <code>Don't show again for {"{app}"}</code> → <code>mouse_capture_banner_blacklist</code> (capture stays delegated, only the notice is suppressed) · <code>Let Tasty handle clicks in {"{app}"}</code> → <code>mouse_capture_blacklist</code> (tasty takes clicks and drags back; the wheel still goes to the program). “Don't show again” also <b>closes the banner it was opened from</b> — suppressing the notice and leaving it on screen would contradict itself. “Let Tasty handle clicks” leaves the banner up (capture is already released; the user dismisses when done reading). Icons: <span className="ic">bell</span> for the notice row, <span className="ic">mouse</span> for the capture row.</Note>
          <Note>While the menu is open the banner is treated as <b>hover-equivalent</b> — a TTL banner would keep its countdown paused, and the affordance column stays drawn even if the pointer has left the card.</Note>
        </Spec>
        <Spec id="elastic-width-the-interpolated-program-name" title="Elastic width — the interpolated program name"
          when={<>Every row interpolates a program name of arbitrary length, and word order differs by locale (en: name last; ko/ja: name first). So a row's label is <b>two parts</b>: the <b>fixed text</b> (never truncates) and the <b>program name</b> in <b>mono</b> (same row ink as the fixed text: text-secondary at rest, text-primary on hover — 2026-10-08), which is the part that <b>shrinks and ellipsises</b>. The menu grows with the content between <b>200px</b> and <b>288px</b> (a wider band than the Tools menu's 160–240, since every row carries a program name) and the full name is available as the row's tooltip.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, alignItems: "flex-start", flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>short name — content-sized (the vim row already passes the 200 floor)</div>
              <BannerMoreMenuG app="vim" />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>medium — grows to fit</div>
              <BannerMoreMenuG app="python3.11" hovered={0} />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>long — capped at 288px, name ellipsises</div>
              <BannerMoreMenuG app="some-very-long-tool-name" />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>ja — fixed copy alone exceeds 288: that row wraps, menu stays 288 (2026-10-08)</div>
              <BannerMoreMenuG app="vim" hovered={1} wrapRows={[1]} texts={[["", " の通知をオフにする"], ["", " について、このプログラムのマウスキャプチャを常に無効にする"]]} />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>rejected — danger tone on the capture row</div>
              <BannerMoreMenuG app="vim" danger />
            </div>
          </Stage>
          <Meta
            specs={[["label", "fixed text + app name (2 spans)"], ["truncates", "the app name only"], ["app name", <>mono · row ink (<span className="tok">--tasty-banner-more-app-fg</span> → menu-item-fg / -fg-hover)</>], ["width", "content-sized, 200 ≤ w ≤ 288 — never widens past 288"], ["wrap", <>only when the FIXED copy + 1 name glyph does not fit: whole label wraps, row grows (min 28, pad-y <span className="tok">--tasty-menu-item-wrap-padding-y</span>); the fixed copy never ellipsises</>], ["tooltip", "full program name on the row"], ["tone", "both rows neutral (no danger)"]]}
            tokens={[{ tok: "--tasty-banner-more-app-font", use: "mono program name" }, { tok: "--tasty-banner-more-app-fg", use: "program name = row ink", color: "var(--tasty-banner-more-app-fg)" }, { tok: "--tasty-banner-more-app-fg-hover", use: "hover", color: "var(--tasty-banner-more-app-fg-hover)" }, { tok: "--tasty-menu-item-height", use: "28px rows" }, { tok: "--tasty-accent-danger", use: "rejected variant only", color: "var(--tasty-accent-danger-raised)" }]} />
          <Do><b>Do</b> keep the program name as the mono part of the row — it is the one thing the user must verify before writing a permanent per-app setting, and mono marks it as a process name rather than prose.</Do>
          <Dont><b>Don't</b> paint the “Let Tasty handle clicks” row in <b>danger</b> tone (last card above, shown for the record). Nothing is destroyed and nothing is lost: the setting is reversible from Settings › Terminal, and it <i>returns</i> the mouse to tasty. Danger in this system means delete — spending it here would make the two rows look like different classes of action when they are the same class at two strengths.</Dont>
        </Spec>
        <Spec id="row-copy-en-ko-ja" title="Row copy — en / ko / ja"
          when={<>Each row names <b>the program</b> and <b>what changes for it</b>, in words a user already has: “show again” for the notice, “clicks” for capture. “Again” / “다시” / “今後” marks the first row as lasting; the second row reads as a standing rule (“in vim, Tasty handles clicks”). The old “~에 대해” and “disable mouse capture” (which read as turning off a feature of the app) are gone. Each row = prefix + <span className="ic">{"{app}"}</span> (mono) + suffix; en uses the prefix only, ko/ja the suffix only (leading space in ja before the particle). Width band 200–288 is unchanged; with this copy no row wraps for a short name in any locale (the old ja wrap is retired, the wrap rule stays for long names and future copy).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, alignItems: "flex-start", flexWrap: "wrap" }}>
            {[["en", "vim"], ["ko", "vim"], ["ja", "vim"], ["en · long name", "some-very-long-tool-name", "en"], ["ja · long name", "some-very-long-tool-name", "ja"]].map(([cap, app, l]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{cap}</div>
                <BannerMoreMenuG app={app} lang={l || cap} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["row 1 en", <>prefix <code>Don't show again for </code> · suffix —</>], ["row 1 ko", <>prefix — · suffix <code>에서 다시 보지 않기</code></>], ["row 1 ja", <>prefix — · suffix <code> では今後表示しない</code></>], ["row 2 en", <>prefix <code>Let Tasty handle clicks in </code> · suffix —</>], ["row 2 ko", <>prefix — · suffix <code>에서 클릭은 Tasty가 처리</code></>], ["row 2 ja", <>prefix — · suffix <code> ではクリックを Tasty で処理</code></>], ["keys", "[popup.mouse_capture_banner_menu] prefix / suffix"]]}
            tokens={[{ tok: "--tasty-banner-more-menu-max-width", use: "288 — unchanged" }]} />
          <Note>Settings › Terminal › Mouse Capture and the user guide take the same verbs: list 1 <code>Don't show the capture notice for these programs</code> / <code>캡처 안내를 다시 보지 않을 프로그램</code> / <code>キャプチャ通知を表示しないプログラム</code> · list 2 <code>Let Tasty handle clicks in these programs</code> / <code>클릭을 Tasty가 처리할 프로그램</code> / <code>クリックを Tasty で処理するプログラム</code>, with the existing description (clicks and drags go to selection and the menu; the wheel is still sent).</Note>
        </Spec>
        <Spec id="program-name-unknown-rows-shown-disabled" title="Program name unknown — rows shown disabled"
          when={<>When the program name is unknown (remote attach mirror — no local PTY; before the first 1 Hz poll; OS lookup failed) the actions still exist but can't run, so they are <b>shown disabled, not hidden</b>. The ⋯ trigger is drawn exactly as with a known name (same hover reveal, lit while open, same 56 body reserve). The menu opens with a non-interactive <b>reason line</b> on top, a separator, then the <b>same two rows disabled</b> — worded with “this program” in place of the name. No “(unknown)” placeholder: a mono “(unknown)” would read as a process called that. Clicking a disabled row does nothing: nothing is saved, the menu stays open, the banner stays. Esc / outside click close as usual. Keyboard arrow navigation skips disabled rows (with both disabled, focus stays on the menu). If the name arrives while the menu is open the menu does not change; the next open shows the normal rows. On a remote mirror the name is always unknown, so this is the menu there — a note, no separate example.</>}>
          <Stage variant="solo center" style={{ flexDirection: "column", gap: 16, padding: 20, paddingBottom: 150, background: "var(--tasty-bg-app)" }}>
            <div style={{ width: "100%", maxWidth: 480 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>name unknown — menu open (en)</div>
              <MouseCaptureBannerG app={null} open />
            </div>
          </Stage>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, alignItems: "flex-start", flexWrap: "wrap" }}>
            {["en", "ko", "ja"].map((l) => (
              <div key={l} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{l} · unknown</div>
                <BannerMoreMenuG app={null} lang={l} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["unknown when", "remote mirror · before first poll · lookup failed"], ["⋯ trigger", "always drawn — same as known name"], ["body reserve", "56 (unchanged)"], ["reason line", <>info glyph 14 (top <span className="tok">--tasty-banner-more-reason-glyph-offset</span> 1) · caption · <span className="tok">--tasty-banner-more-reason-fg</span> · wraps · not focusable, no hover</>], ["rows", <>MenuItem <b>disabled</b> as is (pointer-events none, text + icon <span className="tok">--tasty-state-disabled-fg</span>, no overlay) · <b>wrap</b> on</>], ["tooltip", "none — the reason is in the menu, so pointer-events none costs nothing"], ["click", "no-op · menu stays open · nothing saved"], ["keyboard", "arrows skip disabled rows"], ["live update", "no — next open"], ["lifetime", "has ⋯ → no TTL (normal rule, no exception)"], ["keys", <>[popup.mouse_capture_banner_menu] <code>unknown_reason</code> · <code>notice_unknown</code> · <code>capture_unknown</code> (whole strings, no prefix/suffix)</>]]}
            tokens={[{ tok: "--tasty-banner-more-reserve", use: "56 — always" }, { tok: "--tasty-state-disabled-fg", use: "disabled rows", color: "var(--tasty-state-disabled-fg)" }, { tok: "--tasty-banner-more-reason-fg", use: "reason line", color: "var(--tasty-banner-more-reason-fg)" }, { tok: "--tasty-font-size-caption", use: "reason line 11" }, { tok: "--tasty-banner-more-reason-glyph-offset", use: "reason glyph top 1" }, { tok: "--tasty-menu-separator", use: "reason ↔ rows" }]} />
          <Do><b>Do</b> keep the rows the user would see with a known name, disabled, so the menu teaches what exists; the reason line says why it can't run now, without a hover.</Do>
          <Dont><b>Don't</b> hide the ⋯ or the rows when the name is unknown (2026-11-02 option a, withdrawn). <b>Don't</b> carry the reason in a tooltip on a disabled row — pointer-events none on the row blocks it, and a tooltip is invisible to anyone who doesn't hover.</Dont>
        </Spec>
      </Section>

      <Section id="blacklist" title="Mouse-capture blacklist (Settings › Terminal)">
        <Spec id="per-program-capture-opt-out-list-editor" title="Per-program capture opt-out — list editor"
          when={<>The companion settings (labels aligned with the ⋯ rows 2026-11-02): <b>Don't show the capture notice for these programs</b> (<code>mouse_capture_banner_blacklist</code>) and <b>Let Tasty handle clicks in these programs</b> (<code>mouse_capture_blacklist</code>) — process-name patterns where mouse capture is <b>disabled</b>, so clicks/drags are handled locally (select / tasty menu) instead of being sent to the app. Promoted from the old newline textarea to a <b>list editor</b> (rows + Add) — resolved open decision. Grouped with the <b>Show mouse-capture hint</b> switch since both govern the same feature. Default is an <b>empty list</b>. Remove uses <span className="ic">×</span> (it un-lists a program, it doesn't delete anything).</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>filled · one row hovered</div>
              <BlacklistEditorG />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>empty (default) · Add disabled</div>
              <BlacklistEditorG empty />
            </div>
          </Stage>
          <Meta
            specs={[["lives in", "Settings › Terminal"], ["group", "hint switch + blacklist (1 section)"], ["row", "28px · pattern (mono) + × remove"], ["add", "Input + Add button (disabled when empty)"], ["match", "case-insensitive substring or * wildcard"], ["default", "empty list"]]}
            tokens={[{ tok: "--tasty-overlay-hover", use: "row hover (8%)" }, { tok: "--tasty-accent-warning", use: "match-rule notice", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-input-bg", use: "add field", color: "var(--tasty-input-bg)" }]} />
          <Note>The wheel is always forwarded to the program even for blacklisted apps — only clicks/drags are intercepted. A blacklisted foreground app also suppresses the capture banner on that surface.</Note>
        </Spec>
      </Section>

      <Section id="htmlscript" title="HTML script notice — the first inset banner">
        <Spec id="placement-inset-scoped-to-one-surface" title="Placement — inset, scoped to one surface"
          when={<>The HTML page is a native WebView layered over tasty's GPU frame, so a floating banner would be hidden under it. Approach <b>⒜</b>: tasty paints the banner and the surface's WebView rect <b>shrinks</b> by the banner's height. This is a formal <b>inset</b> placement in the banner system, for any scope whose content is a native layer: same shell, margin <b>8 on all four sides</b> (<span className="tok">--tasty-banner-inset-gap</span> is the new bottom one), and the page starts below it. Floating stays the default everywhere else. Scope is the <b>surface</b>: the terminal beside it is untouched. Showing or clearing the banner resizes the page once; there is no animation on the rect.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-lg)", alignItems: "stretch" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", background: "var(--tasty-bg-app)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · two surfaces side by side, banner on the HTML one only</span>
                <div style={{ display: "flex", gap: "var(--tasty-border-width)", background: "var(--tasty-border-frame)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
                  <HtmlSurfaceG />
                  <TermSurfaceG />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["approach", "⒜ egui banner + WebView rect inset"], ["placement", "inset (new banner variant) · floating unchanged"], ["margin", "8 top / sides / bottom"], ["scope", "the HTML surface only"], ["focus", "never takes it · clicks don't reach the page"], ["TTL", "none — stays until allow or ×"]]}
            tokens={[{ tok: "--tasty-banner-inset-gap", use: "8 — banner → page (new)" }, { tok: "--tasty-banner-margin", use: "8 top / sides" }, { tok: "--tasty-banner-bg", use: "shell", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-border", use: "edge", color: "var(--tasty-banner-border)" }]} />
          <Dont><b>Don't</b> inject the notice into the document DOM (⒞): the page could hide or forge it, and its button can't run with scripts off. <b>Don't</b> build three native overlays (⒝) for one notice.</Dont>
        </Spec>
        <Spec id="banner-states" title="Banner states"
          when={<>Informational, not a warning: blocking is the default working as intended. Leading glyph <b>lock</b> in <span className="tok">--tasty-html-script-banner-glyph</span> (→ accent-info). One action, <b>[Allow for this document]</b>, Secondary / Sm — a relaxation should not be the loudest thing on screen, and there is no second action (the global switch lives in Settings › Appearance › HTML). Title and body <b>wrap and never clamp</b> — the card grows (Tasty-owned copy, same rule as refusal / size sync / mouse capture, 2026-10-10 b12); the button never wraps. The scope ("this document", "until Tasty restarts") is in the body copy. After Allow the document reloads once: the action is replaced by a spinner + <i>Reloading with scripts allowed</i>, × is withheld, and the banner fades out (120ms) when the new load commits.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-md)", alignItems: "stretch" }}>
            {[["1 · blocked (default)", {}], ["2 · hover — × revealed", { hover: true }], ["3 · after Allow — reloading", { state: "reloading" }], ["4 · surface loading a new document — Allow disabled until commit (tooltip on hover)", { state: "loading" }], ["remote content also blocked — body branch", { remote: true }]].map(([cap, p]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", maxWidth: "var(--tasty-size-600)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <HtmlScriptBanner {...p} />
              </div>
            ))}
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>6 · narrow surface (360) — action wraps under the body</span>
              <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden", alignSelf: "flex-start" }}><HtmlSurfaceG width="var(--tasty-size-360)" narrow height={300} /></div>
            </div>
          </Stage>
          <Meta
            specs={[["title", "\"Scripts in this document are blocked\" · 13/600"], ["body", "caption · banner-body-fg · wraps, never clamps (b12 — was ≤ 2 lines)"], ["remote branch", "network scripts stay blocked — says so"], ["action", "Secondary / Sm · no wrap"], ["reloading", "spinner + label · no × · fade out on commit"], ["loading (still blocked)", "Allow disabled from load start to commit · no delay · tooltip (top) \"Available when the document finishes loading\" · × kept"], ["narrow", "surface width < 440 (--tasty-banner-narrow-below) → action on its own line, body-aligned"], ["glyph nudge", "1 · --tasty-banner-glyph-offset"], ["title ↔ body", "2 · --tasty-banner-text-gap (every banner)"]]}
            tokens={[{ tok: "--tasty-html-script-banner-glyph", use: "lock → accent-info", color: "var(--tasty-html-script-banner-glyph)" }, { tok: "--tasty-banner-title-font-size", use: "13" }, { tok: "--tasty-banner-body-font-size", use: "11" }]} />
        </Spec>
        <Spec id="a-button-on-the-banner-shell" title="A button on the banner shell"
          when={<>The shell is <span className="tok">--tasty-banner-bg</span> = surface-raised, which is also the Secondary button's fill, and its edge (border-default) is the same ramp step. On a banner the button would be text only. Rule for <b>every banner</b>: a Secondary inside a banner shell (<code>data-surface="banner"</code>) goes <b>one ramp step up</b>, fill <span className="tok">--tasty-banner-button-bg</span> (→ surface-hover) with edge <span className="tok">--tasty-banner-button-border</span> (→ border-frame). Hover is the normal Button overlay (8%) on top; the edge does not change on hover here. No new variant: the context decides. Disabled on a banner follows the disabled rule (neutral box, disabled ink). <b>Plugin banners too (2026-10-06):</b> the rule belongs to the banner, not to who paints it. The platform opens the banner context for the whole frame of a plugin banner's content, so a plugin author draws a plain Secondary and gets the banner box; the context never leaks to the same plugin's surfaces or popups.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-size-560)", maxWidth: "100%", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · blocked, hover the button</span>
                <HtmlScriptBanner />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · loading — disabled on the shell</span>
                <HtmlScriptBanner state="loading" />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["where", "Secondary inside any banner shell"], ["fill", "surface-on-raised: Mocha one step above banner-bg (n400), Latte n200 (2026-11-05)"], ["edge", "border-frame (one step above banner-border)"], ["hover", "overlay-hover · edge unchanged"], ["variant", "none new — context rule in Button"]]}
            tokens={[{ tok: "--tasty-banner-button-bg", use: "→ surface-on-raised", color: "var(--tasty-banner-button-bg)" }, { tok: "--tasty-banner-button-border", use: "→ border-frame", color: "var(--tasty-banner-button-border)" }, { tok: "--tasty-banner-bg", use: "shell", color: "var(--tasty-banner-bg)" }]} />
        </Spec>
        <Spec id="load-failed-no-banner-no-marker" title="Load failed — no banner, no marker"
          when={<>When the document never commits (the web process dies first, or the load errors), the host chrome's <b>Failed to load</b> state takes the body. The script banner and the tab's lock / script marker are <b>both removed</b> in that state: they describe the previous document, and Allow would allow nothing. Only the failure state shows. A reload that commits decides banner and marker again from the new document. Same on every OS; only Linux is measured today.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", flexWrap: "wrap", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[["current — stale banner over the failure (wrong)", { banner: "blocked", marker: "blocked" }], ["decided — failure state only", { banner: null, marker: null }]].map(([cap, p]) => (
                  <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                    <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · {cap}</span>
                    <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlSurfaceG {...p} failed width="var(--tasty-size-360)" height={220} /></div>
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["banner", "removed while failed"], ["tab marker", "lock / script glyph removed while failed"], ["failure state", "alertCircle md + Failed to load (accent-danger) · URL mono caption text-disabled"], ["reload commits", "banner + marker re-derived from the new document"]]}
            tokens={[{ tok: "--tasty-accent-danger", use: "glyph + title", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-text-disabled", use: "URL", color: "var(--tasty-text-disabled)" }]} />
        </Spec>
        <Spec id="after-and-after-allow-the-tab-strip-marker" title="After × and after Allow — the tab-strip marker"
          when={<>× hides the notice for this load; scripts stay blocked. A <b>lock</b> marker then sits in the surface's tab, first in the tab's right-hand status cluster (Layouts › Pane tab strip; <span className="tok">--tasty-html-script-marker-fg</span> → glyph-dim, 12px). Clicking it <b>shows the banner again</b> — it re-explains before offering Allow, and never allows on its own. Mouse only, no focus change. After Allow, the tab carries a <b>script</b> glyph in text-muted with a tooltip; it is not clickable. Navigating to another document clears both, and the new document starts over. <code>#fragment</code> moves keep them.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[["4a · dismissed — lock, click to re-show", "blocked"], ["4b · allowed this session", "allowed"]].map(([cap, m]) => (
                  <div key={m} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                    <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · {cap}</span>
                    <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlSurfaceG banner={null} marker={m} width="var(--tasty-size-240)" height={120} /></div>
                  </div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["dismissed", "lock · glyph-dim · click → banner again"], ["allowed", "scriptFile · text-muted · tooltip only"], ["size", "12 (icon-size-xs) in the 24 strip"], ["cleared by", "navigation to another document · app restart"], ["kept on", "#fragment moves"], ["no banner when", "no runnable script · already allowed · sandbox off"]]}
            tokens={[{ tok: "--tasty-html-script-marker-fg", use: "→ glyph-dim", color: "var(--tasty-html-script-marker-fg)" }, { tok: "--tasty-html-script-marker-allowed-fg", use: "→ text-muted", color: "var(--tasty-html-script-marker-allowed-fg)" }, { tok: "--tasty-html-script-marker-size", use: "→ icon-size-xs" }]} />
          <Note><b>Firing:</b> only when the user views the document (opened it, or selected the surface). Agent/IPC opens, session restore and background loads keep a "has scripts" flag and show the banner the first time the user looks at that surface.</Note>
        </Spec>
      </Section>
      <Section id="attachrefusal" title="Auto-attach refused — mapping notice">
        <Spec id="persistent-row-mark-workspace-banner-no-toast" title="Persistent row mark + workspace banner, no toast"
          when={<>A workspace mapped with <code>tasty set workspace --ssh / --ssh-profile</code> whose target <b>is this Tasty</b> is not attached (a self-mirror would lock local input). Two signals, both <b>persistent</b> and neither takes focus: a <b>warning glyph</b> on the workspace's <b>sidebar row</b> (visible whether or not it is active, tooltip = target + reason), and a <b>Workspace-scope banner</b> at the top of that workspace's content while it is active — target in mono, the reason, and what to do. <b>Remove mapping</b> (banner button) clears the mapping as <code>tasty set workspace --id … --clear-mapping</code> would; × hides the banner for this activation only — the row mark stays until the mapping changes, is removed, or a profile re-check succeeds. <b>No toast</b>: the condition outlives a toast and must be findable later. The same pair covers the other mapping errors (profile not found, endpoint not resolvable) with their own reason line.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} {...(th ? { "data-theme": th } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", alignItems: "flex-start" }}>
                <div style={{ width: "var(--tasty-size-200)", display: "flex", flexDirection: "column", gap: "var(--tasty-size-1)", padding: "var(--tasty-space-xs)", background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)" }}>
                  <RefusalRowG name="tasty-core" />
                  <RefusalRowG name="staging-mirror" active refused />
                  <RefusalRowG name="scratch" />
                </div>
                <div style={{ width: "var(--tasty-size-460)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                  <AttachRefusalBannerG kind="self" />
                  <div style={{ height: "var(--tasty-size-64)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-terminal-focused-bg)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", padding: "var(--tasty-space-sm)" }}>local terminal — still usable</div>
                </div>
              </div>
            ))}
          </Stage>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-sm)", alignItems: "stretch" }}>
            <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>other mapping errors — same banner, own reason</div>
            <div style={{ maxWidth: "var(--tasty-size-560)" }}><AttachRefusalBannerG kind="profile" /></div>
            <div style={{ maxWidth: "var(--tasty-size-560)" }}><AttachRefusalBannerG kind="resolve" /></div>
          </Stage>
          <Meta
            specs={[["row mark", "alertTriangle 14 · accent-warning · trailing slot of the workspace row · tooltip = target + reason"], ["banner scope", "Workspace · shown while that workspace is active"], ["glyph", "alertTriangle 16 · accent-warning"], ["title", "Remote not attached — {target} (target mono)"], ["body", "{reason} Change or remove the mapping for this workspace."], ["action", "Remove mapping · banner button (secondary sm on banner-button tokens)"], ["×", "hides for this activation; the row mark stays"], ["clears", "mapping changed / removed · profile re-check succeeds"], ["focus", "never moved — agent-made mappings included"], ["toast", "none"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "glyph + row mark", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-banner-bg", use: "shell", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-button-bg", use: "Remove mapping", color: "var(--tasty-banner-button-bg)" }, { tok: "--tasty-banner-body-font-size", use: "reason 11" }]} />
          <Note>Strings (en): <code>remote.refusal.title</code> “Remote not attached — {"{target}"}” · <code>remote.refusal.self</code> “This mapping points at this Tasty, so it was not attached.” · <code>remote.refusal.profile_missing</code> “No SSH profile with this name.” · <code>remote.refusal.unresolved</code> “The endpoint could not be resolved.” · <code>remote.refusal.hint</code> “Change or remove the mapping for this workspace.” · <code>remote.refusal.remove</code> “Remove mapping”. No new tokens.</Note>
        </Spec>
        <Spec id="collapsed-rail-beside-the-attention-badges" title="Collapsed rail · beside the attention badges"
          when={<><b>Rail:</b> the refusal takes the one free avatar corner, <b>top-left</b>, as a 12px chip (alertTriangle 8 on bg-sidebar) — top-right stays the attention dot, bottom-right the mirror chip, bottom-left the move chip; all four can show at once, so there is no priority rule. Same tooltip as the row. <b>Row:</b> order after the name is <b>move glyph · refusal mark · badge group</b>, gap = the row item spacing (space-sm). The badges stay rightmost so the counts line up down the list; the two persistent marks sit before them. The mark never replaces a badge.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} {...(th ? { "data-theme": th } : {})} style={{ display: "flex", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", alignItems: "flex-start" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm)", background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)" }}>
                  <RefusalRailG ch="T" />
                  <RefusalRailG ch="S" refused />
                  <RefusalRailG ch="D" refused dot="warning" move />
                </div>
                <div style={{ width: "var(--tasty-size-200)", display: "flex", flexDirection: "column", gap: "var(--tasty-size-1)", padding: "var(--tasty-space-xs)", background: "var(--tasty-bg-sidebar)", borderRadius: "var(--tasty-radius)" }}>
                  <RefusalRowG name="tasty-core" done={3} />
                  <RefusalRowG name="staging-mirror" active refused ni={1} done={5} />
                  <RefusalRowG name="data-etl" refused move ni={2} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["rail chip", "top-left · 12 · alertTriangle 8 · accent-warning on bg-sidebar"], ["rail corners", "TR attention dot · BR mirror · BL move · TL refusal — no priority"], ["row order", "name · move · refusal · badges"], ["row gap", "space-sm (row item spacing)"], ["tooltip", "same as the row · not clickable"]]}
            tokens={[{ tok: "--tasty-attach-refusal-glyph", use: "→ accent-warning", color: "var(--tasty-attach-refusal-glyph)" }, { tok: "--tasty-attach-refusal-chip-size", use: "→ move-source-chip-size 12" }, { tok: "--tasty-attach-refusal-chip-glyph-size", use: "→ move-source-chip-glyph-size 8" }]} />
        </Spec>
      </Section>
      <Section id="attachsizesync" title="Attach mirror — terminal size sync failed">
        <Spec id="workspace-banner-default-retrying-several-surfaces" title="Workspace banner · default · retrying · several surfaces"
          when={<>An attached mirror asks the remote to resize each terminal to the local pane. When the remote does not answer within 5 s (or refuses) Tasty retries <b>once</b> on its own; if that also fails this banner appears in the <b>mirror workspace</b> that owns the surface. Same family as the refusal banner: <b>alertTriangle</b> in accent-warning, a semibold title, one muted body line, one banner-button action and the <b>×</b> dismiss. <b>Retry</b> re-sends only the failed surfaces' size requests (the connection and other surfaces are untouched). While waiting the button is <b>disabled with a leading Spinner</b> and keeps its label, so the width does not jump between languages; success removes the banner, failure returns it to the default state. <b>Several surfaces</b> failing together share <b>one card</b>: “N surfaces — a, b +n”, and the action reads <b>Retry all</b>. The banner also clears by itself when a later resize succeeds. A reconnecting mirror shows the reconnect notice instead (it wins the slot). Queue rules are the normal Workspace-scope ones: one card per scope.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} {...(th ? { "data-theme": th } : {})} style={{ width: "var(--tasty-size-460)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} — default</span>
                <AttachSizeSyncBannerG names={["build"]} />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>retrying (≤ 5 s)</span>
                <AttachSizeSyncBannerG names={["build"]} retrying />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>3 surfaces · long tab title truncated</span>
                <AttachSizeSyncBannerG names={["release-pipeline-watch-logs-eu-west", "build", "tests"]} />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>split tab · same name twice (b11)</span>
                <AttachSizeSyncBannerG names={["Shell", "Shell"]} />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>duplicates + others (b11)</span>
                <AttachSizeSyncBannerG names={["Shell", "Shell", "build", "tests"]} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["boxes", "glyph 16 · text column (flex 1) · action group (Retry + ×, gap 4)"], ["glyph", "alertTriangle 16 · attach-sync-glyph → accent-warning (same as refusal)"], ["title", "banner-title 13 · semibold · never truncated"], ["body", "banner-body 11 · banner-body-fg · TWO lines (b10): ① names — one line ② “The remote may still be using the old size.” — wraps, never cut"], ["name", "text-secondary-raised · each name ≤ attach-sync-name-max-width 160, shrinks to fit the line down to attach-sync-name-min-width 40, then ellipsis · the floor applies only while shrinking: box = min(natural, max(40, shrunk)) — a short name keeps its own width (b11) · first two entries, then +n; separators, ×k and +n never shrink"], ["same name (b11)", "equal display names collapse into one entry + “ ×k” (text-muted, never shrinks) · “N surfaces” counts surfaces · +n counts remaining entries · Retry all retries every surface"], ["body lines (b11 · b12)", "Tasty-owned banner copy (refusal · size sync · mouse capture · HTML script) never clamps — the card grows · plugin-supplied body clamps at 3 lines, then ellipsis + full text in the tooltip"], ["plugin body rows (b12)", "rows = min(3, floor(content height ÷ body line height)) — a fixed size_hint that holds fewer lines clamps at what fits; the ellipsis sits on the last row that fits"], ["plugin tooltip (b12)", "drawn by the HOST, not inside the plugin mesh: the widget reports “truncated + full text” for the frame and the host shows the standard Tooltip BELOW the banner card (left = body x, offset tooltip-offset 4, max tooltip-max-width 240, delay tooltip-delay) — never over the pointer, never clipped by the content rect · interim (no channel yet): the widget opens its tooltip below the body inside the content rect, never above"], ["retrying", "Retry disabled + leading Spinner 14, label unchanged"], ["many", "one card · “N surfaces — a, b +n” · Retry all"], ["×", "IconButton sm · hides this card; a new failure shows it again"], ["clears", "Retry success · later resize success · surface closed"], ["priority", "reconnect notice wins the slot"]]}
            tokens={[{ tok: "--tasty-attach-sync-glyph", use: "→ accent-warning", color: "var(--tasty-attach-sync-glyph)" }, { tok: "--tasty-attach-sync-name-max-width", use: "→ size-160" }, { tok: "--tasty-attach-sync-name-min-width", use: "→ size-40 (new, b11)" }, { tok: "--tasty-banner-button-bg", use: "Retry", color: "var(--tasty-banner-button-bg)" }, { tok: "--tasty-spinner-indicator", use: "retrying", color: "var(--tasty-spinner-indicator)" }]} />
          <Note>Strings (en): <code>remote.size_sync.title</code> “Couldn't sync the terminal size with the remote” · <code>remote.size_sync.names</code> “{"{names}"}” (line 1) · <code>remote.size_sync.hint</code> “The remote may still be using the old size.” (line 2; replaces <code>remote.size_sync.body</code>) · <code>remote.size_sync.many</code> “{"{n}"} surfaces — {"{names}"}” · <code>remote.size_sync.retry</code> “Retry” · <code>remote.size_sync.retry_all</code> “Retry all”.</Note>
        </Spec>
        <Spec id="narrow-action-under-the-text-stays-top-right" title="Narrow — action under the text, × stays top-right"
          when={<>Every banner judges narrow on the <b>content width of its scope</b>, before the banner margins: a surface banner on that surface, a Pane / Tab banner on that pane / tab content, a <b>Workspace</b> banner on the union of panes under the tab bar (what the app does today). Under <span className="tok">--tasty-banner-narrow-below</span> (440) the <b>action button</b> moves to its own line under the text, starting at the body's left edge (glyph 16 + banner-gap), banner-gap above it. The <b>×</b> is a dismiss affordance, not an action: it <b>stays in the top-right</b> of the card, as on the HTML script banner. Retry all and the retrying state (disabled + Spinner, label kept) look the same on the wrapped line. Samples at 360.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, th]) => (
              <div key={label} {...(th ? { "data-theme": th } : {})} style={{ width: "var(--tasty-size-360)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} — refused · 360</span>
                <AttachRefusalBannerG kind="self" narrow />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>size sync · one surface</span>
                <AttachSizeSyncBannerG names={["build"]} narrow />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>size sync · 3 surfaces · Retry all</span>
                <AttachSizeSyncBannerG names={["release-pipeline-watch-logs-eu-west", "build", "tests"]} narrow />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>size sync · retrying</span>
                <AttachSizeSyncBannerG names={["release-pipeline-watch-logs-eu-west", "build", "tests"]} narrow retrying />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["judged on", "scope content width before margins · workspace = pane union under the tab bar"], ["threshold", "banner-narrow-below 440"], ["action", "own line · left = body edge (glyph 16 + banner-gap) · banner-gap above"], ["×", "stays top-right of the card (not part of the wrapped group)"], ["retrying / Retry all", "same on the wrapped line"], ["sample width", "360"]]}
            tokens={[{ tok: "--tasty-banner-narrow-below", use: "→ size-440" }, { tok: "--tasty-banner-gap", use: "glyph ↔ text · text ↔ action line" }]} />
        </Spec>
      </Section>
      <Section id="htmlremotesize" title="HTML mirror — large-file and limit notices">
        <Spec id="two-inset-banners-mocha-latte-wide-narrow" title="Two inset banners · Mocha / Latte · wide / narrow"
          when={<>The two remote-size questions of a mirrored HTML surface, as banners. Same inset shell and button boxes as the script notice; <b>no ×</b> (each is a decision) and no keyboard focus. When both a size notice and the script notice show, the size notice sits <b>first</b>. Full surface placement (address bar band, receiving state, stacking) lives in <a href={`${BASE}design/gallery/plugins/#html`}>Plugins › HTML viewer</a>.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([th, dt]) => (
              <div key={th} data-theme={dt || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)", width: "var(--tasty-size-560)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{th} · large file · one file</span>
                <HtmlSizeBanner kind="file" />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{th} · large file · more waiting, long path</span>
                <HtmlSizeBanner kind="file" name="assets/media/marketing/2026/launch/hero-background-loop-4k.mp4" size="12.4 MB" more={2} />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{th} · cumulative limit</span>
                <HtmlSizeBanner kind="limit" />
              </div>
            ))}
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)", width: "var(--tasty-size-360)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>narrow (&lt; 440) — actions wrap under the body</span>
              <HtmlSizeBanner kind="file" narrow />
              <HtmlSizeBanner kind="limit" narrow />
            </div>
          </Stage>
          <Note><b>2026-11-01:</b> the remote-content notice joins the same stack between the two — <b>size → remote content → script</b>. See <a href="#htmlremote">HTML remote content notice</a>.</Note>
          <Meta
            specs={[["shell", "script-notice inset shell · no ×"], ["title", "13/600 · wraps, never clamps"], ["large file", "download (info) · title · path left-ellipsis + size Tag warning [+N more waiting] · Decline / Allow"], ["limit", "alertTriangle (warning) · total received · limit sentence · Stop / Continue"], ["order", "size notice above script notice"], ["narrow", "banner-narrow-below 440 → actions wrap"]]}
            tokens={[{ tok: "--tasty-html-size-banner-glyph", use: "→ accent-info", color: "var(--tasty-html-size-banner-glyph)" }, { tok: "--tasty-html-limit-banner-glyph", use: "→ accent-warning", color: "var(--tasty-html-limit-banner-glyph)" }]} />
        </Spec>
      </Section>
      <Section id="htmlremote" title="HTML remote content notice">
        <Spec id="banner-states-2" title="Banner — states"
          when={<>When the global <b>Allow remote content</b> is off and the document references <code>http(s)</code> stylesheets, images, fonts, frames, media or scripts, the HTML surface shows a second inset notice. Same shell, life rules and state names as the script notice; it is told apart by its glyph, <b>globeOff</b> (new), in the same informational tone (<span className="tok">--tasty-html-remote-banner-glyph</span> → accent-info). The title states the policy (“…is blocked”), which is true whether the reference was found by the static scan or seen blocked by the OS, so the copy never has to tell those apart. The body names the <b>kinds</b>, not counts: a static count can differ from what the page actually loads. Title and body wrap and never clamp (Tasty-owned copy, b12). One action <b>[Allow remote content]</b>, Secondary / Sm — distinct from the script notice's “Allow for this document” so two stacked notices never show the same button label.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-size-560)", maxWidth: "100%", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[["blocked", {}], ["hover — × revealed", { hover: true }], ["loading — Allow disabled until commit", { state: "loading" }], ["reloading — spinner, no ×", { state: "reloading" }]].map(([cap, p]) => (
                  <React.Fragment key={cap}>
                    <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · {cap}</span>
                    <HtmlRemoteBanner {...p} />
                  </React.Fragment>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["glyph", "globeOff 16 · html-remote-banner-glyph → accent-info"], ["title", "“Remote content in this document is blocked” · 13/600 · wraps"], ["body", "kinds, no counts · scope and lifetime last · 11 · banner-body-fg · wraps"], ["action", "Allow remote content · Secondary / Sm · no wrap"], ["loading", "disabled from load start to commit · tooltip top “Available when the document finishes loading” · × kept"], ["reloading", "Spinner 14 + “Reloading with remote content allowed” · body opacity-dimmed · no × · fades out on commit"], ["×", "hover only · hides the notice for this document, block stays"], ["focus", "never taken · click only"]]}
            tokens={[{ tok: "--tasty-html-remote-banner-glyph", use: "→ accent-info", color: "var(--tasty-html-remote-banner-glyph)" }, { tok: "--tasty-banner-bg", use: "shell", color: "var(--tasty-banner-bg)" }, { tok: "--tasty-banner-button-bg", use: "Allow", color: "var(--tasty-banner-button-bg)" }]} />
          <Note>Strings (en): <code>banner.html_remote.title</code> “Remote content in this document is blocked” · <code>banner.html_remote.body</code> “Stylesheets, images, fonts, frames and scripts from the network don't load, so the page may look incomplete. Allowing applies to this document only, until Tasty restarts.” · <code>banner.html_remote.action</code> “Allow remote content” · <code>banner.html_remote.reloading</code> “Reloading with remote content allowed” · <code>banner.html_remote.action_loading</code> “Available when the document finishes loading” (same text as the script key) · <code>banner.html_remote.marker_blocked</code> “Remote content blocked. Click for details.” · <code>banner.html_remote.marker_allowed</code> “Remote content allowed for this document until Tasty restarts”.</Note>
        </Spec>
        <Spec id="the-address-field-marker-always-findable" title="The address-field marker — always findable"
          when={<>The persistent signal lives at the <b>end of the address field</b>, after the mirror's “n skipped” Tag: the band is where the document's address and its loading state already are, and the tab cell's right cluster is kept for the script marker. Shown whenever the notice is <b>not</b> on screen. <b>Blocked</b> (× dismissed, or a block the OS observed without a static reference) = <b>globeOff</b> in glyph-dim inside a 20 hit cell with the hover fill; a click <b>shows the notice</b>, it never allows on its own. <b>Allowed</b> = <b>globe</b> in text-muted, tooltip only. Tooltips open <b>top</b>. Agent IPC/CLI queries and allows never change focus, the active tab or the marker drawing beyond the reload result. Failed (uncommitted) documents: no notice, no marker.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, attr]) => (
              <div key={label} {...(attr ? { "data-theme": attr } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: "var(--tasty-size-460)", maxWidth: "100%", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                {[["blocked", { trailing: <HtmlRemoteMarker kind="blocked" /> }], ["blocked · hover", { trailing: <HtmlRemoteMarker kind="blocked" hover /> }], ["allowed", { trailing: <HtmlRemoteMarker kind="allowed" /> }], ["mirror · after a declined large file — Tag, then marker", { kind: "remote", address: "build-eu:~/reports/q3/report.html", skipped: 2, trailing: <HtmlRemoteMarker kind="blocked" /> }]].map(([cap, p]) => (
                  <React.Fragment key={cap}>
                    <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · {cap}</span>
                    <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlAddressBar kind="local" address="~/reports/q3/report.html" {...p} /></div>
                  </React.Fragment>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["slot", "address field end · after skipped Tag · field ellipsises the address first"], ["cell", "20 (html-remote-marker-hit) · glyph 14 · radius-sm"], ["blocked", "globeOff · glyph-dim · hover fill · click → notice"], ["allowed", "globe · text-muted · tooltip only"], ["shown when", "notice not on screen · never with the notice"], ["with script marker", "independent places: script = tab cluster, remote = address field · never merged"], ["cleared by", "another document committing · failed load · app restart"]]}
            tokens={[{ tok: "--tasty-html-remote-marker-fg", use: "→ glyph-dim", color: "var(--tasty-html-remote-marker-fg)" }, { tok: "--tasty-html-remote-marker-allowed-fg", use: "→ text-muted", color: "var(--tasty-html-remote-marker-allowed-fg)" }, { tok: "--tasty-html-remote-marker-size", use: "→ icon-size-sm 14" }, { tok: "--tasty-html-remote-marker-hit", use: "→ size-20" }, { tok: "--tasty-html-remote-marker-hover-bg", use: "→ overlay-hover", color: "var(--tasty-html-remote-marker-hover-bg)" }]} />
          <Dont><b>Don't</b> make the blocked marker allow directly. It re-shows the notice, which explains scope before offering Allow — the same rule as the script lock.</Dont>
        </Spec>
        <Spec id="surface-variants-1-10-stacking-with-the-script-and-size-notices" title="Surface variants 1–10 · stacking with the script and size notices"
          when={<>Stacking order under the address band, one inset stack (<span className="tok">--tasty-banner-margin</span> 8 above, <span className="tok">--tasty-banner-inset-gap</span> 8 between and below): <b>mirror size notice → remote content → script</b>. Size blocks rendering entirely, remote content decides how the page looks, scripts decide how it behaves — and allowing remote can change what the script notice says. The two notices stay <b>separate cards</b>: the allows are independent and either one can be left standing. <b>Remote-src scripts</b> need both allows, so the script notice is judged on <i>runnable</i> scripts: a document whose only scripts come from the network shows the remote notice alone (7); after remote Allow reloads, those scripts become runnable and the script notice appears with its plain body (7′). With inline and network scripts both present, the script notice uses the revised <code>bodyRemote</code>. <b>Detection:</b> a static reference fires the notice. A block the OS reports without a static reference (Windows; Linux pending) shows the marker only, no late notice (8a). With no OS signal and no reference (macOS) nothing is shown (8b); the global switch in Settings stays the path.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", gap: "var(--tasty-space-lg)", alignItems: "stretch" }}>
            <RcThemeGrid label="Mocha" />
            <RcThemeGrid label="Latte" attr="latte" />
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
              <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>mirror · all three notices — size → remote → script</span>
              <div style={{ alignSelf: "flex-start", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><HtmlRemoteSurfaceG mirror size="file" remote="blocked" script="blocked" width="var(--tasty-size-560)" height={460} /></div>
            </div>
          </Stage>
          <Meta
            specs={[["order", "address band → size (mirror) → remote → script → WebView"], ["gaps", "banner-margin 8 top/sides · banner-inset-gap 8 between + below"], ["merge", "never — two cards, two independent allows"], ["script notice", "judged on runnable scripts (network scripts need remote allowed)"], ["bodyRemote", "kept as a key, copy revised: shown only alongside the remote notice"], ["static ref", "notice (when the user views)"], ["observed block, no ref", "marker only (Windows · Linux TBD)"], ["no signal, no ref", "nothing (macOS)"], ["narrow", "< 440 → each card wraps its action, stack order unchanged"], ["global on", "no notice · no marker"]]}
            tokens={[{ tok: "--tasty-banner-inset-gap", use: "8 between / below" }, { tok: "--tasty-banner-margin", use: "8 top / sides" }, { tok: "--tasty-banner-narrow-below", use: "→ size-440" }]} />
          <Note>Script notice <code>banner.html_script.body_remote</code> (en, revised): “Buttons and menus that need JavaScript may not respond. Allowing applies to this document only, until Tasty restarts. Scripts inside it run; scripts from the network also need remote content allowed.”</Note>
        </Spec>
      </Section>
      <Section id="bodyink" title="Body ink — every banner, both themes">
        <Spec id="banner-body-fg-the-supporting-line-on-the-banner-shell" title="banner-body-fg — the supporting line on the banner shell"
          when={<>The banner shell is <span className="tok">--tasty-banner-bg</span> = surface-raised. In Latte <code>text-muted</code> reaches only 3.65:1 on it, at 11px. The shell stays put, because it is what separates the banner from the page; the ink moves. Every banner body takes <span className="tok">--tasty-banner-body-fg</span>.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <BodyInkGrid label="Mocha" theme="mocha" />
            <BodyInkGrid label="Latte" theme="latte" />
          </Stage>
          <Meta
            specs={[["body", "banner-body-fg → text-muted-raised · Mocha n900 5.65 (unchanged) · Latte n1100 5.17 (was 3.65)"], ["reloading status", "same token · Spinner + label in the action slot"], ["reloading", "no dim · title and body keep their ink · Spinner + label carry the state"], ["sync name", "text-secondary-raised · Mocha n1000 7.10 · Latte n1100 5.17 (was 4.05)"], ["hierarchy", "title 13/600 · body 11/400 — in Latte both use n1100, so size and weight carry the order"], ["countdown, reason line", "same role, next spec"]]}
            tokens={[{ tok: "--tasty-banner-body-fg", use: "new · → text-muted-raised", color: "var(--tasty-banner-body-fg)" }, { tok: "--tasty-text-muted-raised", use: "new · Mocha n900 · Latte n1100", color: "var(--tasty-text-muted-raised)" }, { tok: "--tasty-text-secondary-raised", use: "sync name", color: "var(--tasty-text-secondary-raised)" }, { tok: "--tasty-banner-bg", use: "ground · unchanged", color: "var(--tasty-banner-bg)" }]} />
          <Dont><b>Don't</b> darken <code>banner-bg</code> or swap it for bg-panel to gain contrast. Banners float over panels; the fill and the banner buttons are tuned for surface-raised.</Dont>
        </Spec>
        <Spec id="countdown-reason-line-ghost-buttons-sample-titles" title="Countdown, reason line, ghost buttons, sample titles"
          when={<>The rest of the ink that sits on surface-raised. Every item moves to an existing <code>-raised</code> role; the only new token is the one component part the reason line never had. The size notice title now wraps like every other banner.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <RemainingInkGrid label="Mocha" theme="mocha" />
            <RemainingInkGrid label="Latte" theme="latte" />
          </Stage>
          <Meta
            specs={[["countdown", "banner-countdown-fg → text-muted-raised · Mocha n900 5.65 (unchanged) · Latte n1100 5.17 (was 3.65)"], ["reason line + glyph", "banner-more-reason-fg (new) → text-muted-raised · Mocha 5.65 (unchanged) · Latte 5.17 (was 3.65)"], ["Decline · Stop · other ghost on a banner", "Button ground=raised → button-ghost-raised-fg → text-secondary-raised · Mocha n1000 7.10 (unchanged) · Latte n1100 5.17 (was 4.05)"], ["sample titles", "banner-fg → text-primary · Mocha 8.69 · Latte 5.17 (was text-secondary 4.05)"], ["size notice title", "13/600 · wraps, never clamps (was ≤ 2 lines in the sample)"], ["hierarchy", "in Latte title, body and countdown all use n1100, so size, weight and the mono face carry the order"], ["closed in the next spec", "Allow / Continue on banner-button-bg (Latte 7.06) · Kbd on a banner (Latte 5.17) · size Tag warning (Latte 5.25) · ghost hover overlay (Latte 6.36)"]]}
            tokens={[{ tok: "--tasty-banner-countdown-fg", use: "→ text-muted-raised", color: "var(--tasty-banner-countdown-fg)" }, { tok: "--tasty-banner-more-reason-fg", use: "new · → text-muted-raised", color: "var(--tasty-banner-more-reason-fg)" }, { tok: "--tasty-button-ghost-raised-fg", use: "existing · Decline, Stop", color: "var(--tasty-button-ghost-raised-fg)" }, { tok: "--tasty-banner-fg", use: "sample titles", color: "var(--tasty-banner-fg)" }]} />
        </Spec>
        <Spec id="secondary-kbd-tag-and-state-ink" title="Secondary, Kbd, Tag and state ink"
          when={<>The last pairs below 4.5:1, and the rule for states. A selected or keyboard-active row draws every ink in <code>text-on-active</code>. Hover and pressed stay transient with a 3:1 floor, except rows people read while hovering. <code>README.md</code> holds the rule.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <ClosedInkTable label="Mocha" theme="mocha" />
            <ClosedInkTable label="Latte" theme="latte" />
          </Stage>
          <Meta
            specs={[["banner Secondary", "banner-button-bg → surface-on-raised · Latte n200, lighter than the shell"], ["Kbd", "kbd-fg → text-secondary-raised · switch-overlay-fg follows"], ["Tag", "tag-<variant>-fg · the accent stays on the edge"], ["selected row", "every ink → text-on-active"], ["accent glyph on it", "accent-primary-on-active"], ["hover, pressed", "3:1 floor · menu row and ghost on raised hold 4.5"]]}
            tokens={[{ tok: "--tasty-surface-on-raised", use: "new · Mocha n400 · Latte n200", color: "var(--tasty-surface-on-raised)" }, { tok: "--tasty-text-on-active", use: "new · Mocha n1100 · Latte #35374a", color: "var(--tasty-text-on-active)" }, { tok: "--tasty-accent-primary-on-active", use: "new · Mocha blue · Latte #1547ac", color: "var(--tasty-accent-primary-on-active)" }, { tok: "--tasty-tag-warning-fg", use: "new · → accent-warning-raised", color: "var(--tasty-tag-warning-fg)" }, { tok: "--tasty-kbd-fg", use: "→ text-secondary-raised", color: "var(--tasty-kbd-fg)" }]} />
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "overlays-banners",
  nav: NAV,
  head: {
    title: "Banners",
    intro: "See how banners bring messages and actions into the work area. Compare their scopes and queues, and try mouse actions that leave keyboard focus with your work.",
    
  },
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}

// The tokens these rows sample are glyph ink, grade 3. A glyph sample is a shape, not a text character.
function inkSample(s) {
  const dot = { display: "inline-block", width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "var(--tasty-radius-pill)" };
  if (s === "●") return <span style={{ ...dot, background: "currentColor" }} />;
  if (s === "○") return <span style={{ ...dot, boxSizing: "border-box", border: "var(--tasty-border-width) solid currentColor" }} />;
  if (s === "★") return <BIcon name="starFill" size="var(--tasty-icon-size-sm)" />;
  if (s === "✓") return <BIcon name="check" size="var(--tasty-icon-size-sm)" />;
  return s;
}
