import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as HaIcon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Spinner } from "../../../components/feedback/Spinner.jsx";
import { Tooltip } from "../../../components/feedback/Tooltip.jsx";
import { HsTab, HsPage, HtmlScriptBanner } from "./html_script_banner.jsx";

// HTML surface — ADDRESS BAR band + remote-mirror size banners + receiving state.
// Designed 2026-10-12.
//
// Box boundaries (top → bottom, every box is host egui; the WebView rect starts after the last one):
//   tab strip
//   BAND      html-address-bar-height 40 · bg html-address-bar-bg · 1px html-address-bar-border below
//             [nav cluster: Back · Forward · Reload (IconButton sm 24, nav-gap 4)] gap 8 [field flex 1]
//             padding-x 8. Field = the PathField box (input tokens, 28 tall, mono, leading kind glyph), NO Go button.
//   ERROR ROW (only while an address-input error stands) html-address-error-height 24, same bg, part of the band —
//             the band's 1px rule moves below it. Glyph alertCircle html-address-error-glyph + message text-primary.
//   BANNERS   inset stack: padding banner-margin (8) on top/left/right, banners banner-inset-gap (8) apart,
//             banner-inset-gap (8) below the last one. Order: size banner (mirror, blocks rendering) → remote-content
//             banner (2026-11-01, html_remote_banner.jsx) → script banner.
//   PAGE      native WebView — or a host state painted in its place (empty · loading · failed · receiving · notice).
//
// Field states: idle (address, html-address-value-fg → text-secondary-raised; 2026-10-21 Latte 4.05 → 5.17) · editing (focus edge + caret, text-primary) · placeholder (empty tab)
//   · loading (Reload slot shows a Spinner, not clickable; field shows the target address) · error (danger edge,
//   still editing, error row below). Error clears on the next keystroke, on Esc (reverts to the current address)
//   and on a successful open — never on a timer.
// Address format: local = filesystem path (~ for home); file:// input is accepted and shown as the path after open.
//   web = full URL. Leading glyph: file (local) · link (web) · html (empty) · remote (mirror).
// Mirror: read-only field (input-readonly tokens) + Reload only; Back/Forward are not drawn. Before the remote HTML
//   feature ships the mirror surface is a placeholder and the band is HIDDEN (nothing to reload).
// Narrow: surface width < banner-narrow-below (440) → Forward is hidden (Back + Reload stay; shortcuts still work).
//   Field floor html-address-field-min-width (96); below that the field ellipsises from the left, buttons never shrink.
// Tooltips on band buttons open TOP (never down into the native WebView).




const HA_COPY = {
  placeholder: "Enter a file path or URL",
  back: "Back", forward: "Forward", reload: "Reload",
  emptyTitle: "No page loaded",
  emptyHint: "Type a path or URL in the address bar",
  notFound: "File not found: /tmp/x/missing.html",
  noBase: "Can't resolve a relative path — no document or working folder",
  sizeTitle: "The remote wants to send a large file",
  allow: "Allow", decline: "Decline",
  limitTitle: "2.1 MB received from the remote. Continue?",
  limitBody: "Automatic receiving pauses after 2 MB per document.",
  cont: "Continue", stop: "Stop",
  receiving: "Receiving remote files…",
  paused: "Paused — answer the notice above",
  macos: "Remote web pages can't be shown on this macOS version.",
  macosSub: "Requires macOS 14 or later.",
  disconnected: "Disconnected from the remote",
  disconnectedSub: "Reconnect to load this page.",
  skipped: (n) => `${n} skipped`,
};

const navBtn = (label, name, { disabled, busy } = {}) => (
  <Tooltip placement="top" content={label}>
    <span style={{ display: "inline-flex" }}>
      {busy ? (
        <span aria-label={label} style={{ width: "var(--tasty-icon-button-size-sm)", height: "var(--tasty-icon-button-size-sm)", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
          <Spinner size="var(--tasty-icon-size-sm)" />
        </span>
      ) : (
        <IconButton size="sm" aria-label={label} disabled={disabled}><HaIcon name={name} /></IconButton>
      )}
    </span>
  </Tooltip>
);

// kind: empty | local | web | remote · state: idle | editing | loading | error
// trailing (2026-11-01): optional end-of-field marker slot — the remote-content marker (html_remote_banner.jsx). Order: skipped Tag · marker.
function HtmlAddressField({ kind = "local", address = "", state = "idle", skipped = 0, trailing = null }) {
  const readOnly = kind === "remote";
  const editing = state === "editing" || state === "error";
  const glyph = { empty: "html", local: "file", web: "link", remote: "remote" }[kind];
  const border = state === "error" ? "var(--tasty-input-border-invalid)" : editing ? "var(--tasty-input-border-focus)" : readOnly ? "var(--tasty-input-readonly-border)" : "var(--tasty-input-border)";
  const empty = !address;
  return (
    <div style={{ flex: 1, minWidth: "var(--tasty-html-address-field-min-width)", display: "flex", alignItems: "center", gap: "var(--tasty-input-gap)",
      height: "var(--tasty-input-height)", padding: "0 var(--tasty-space-sm)", borderRadius: "var(--tasty-input-radius)",
      background: readOnly ? "var(--tasty-input-readonly-bg)" : "var(--tasty-input-bg)", border: "var(--tasty-border-width) solid " + border,
      boxShadow: editing ? "0 0 0 var(--tasty-focus-ring-width) color-mix(in srgb, " + (state === "error" ? "var(--tasty-input-border-invalid)" : "var(--tasty-border-focus)") + " 35%, transparent)" : "none" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-input-icon-fg)" }}><HaIcon name={glyph} size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ flex: 1, minWidth: 0, fontFamily: empty ? "var(--tasty-font-ui)" : "var(--tasty-font-mono)", fontSize: "var(--tasty-input-font-size)",
        color: empty ? "var(--tasty-input-placeholder)" : editing ? "var(--tasty-input-fg)" : readOnly ? "var(--tasty-input-readonly-fg)" : "var(--tasty-html-address-value-fg)",
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: empty ? "ltr" : "rtl", textAlign: "left" }}>
        <bdi>{empty ? HA_COPY.placeholder : address}</bdi>
        {editing && <span style={{ display: "inline-block", width: "var(--tasty-border-width)", height: "var(--tasty-size-14)", background: "var(--tasty-accent-primary)", marginLeft: "var(--tasty-border-width)", verticalAlign: "text-bottom" }} />}
      </span>
      {skipped > 0 && (
        <Tooltip placement="top" content="Declined: hero-video.mp4, fonts/Inter.woff2">
          <span style={{ display: "inline-flex", flex: "none" }}><Tag>{HA_COPY.skipped(skipped)}</Tag></span>
        </Tooltip>
      )}
      {trailing}
    </div>
  );
}

// The band (+ error row). canBack/canForward drive disabled; narrow hides Forward; remote = read-only + Reload only.
function HtmlAddressBar({ kind = "local", address = "", state = "idle", canBack = true, canForward = false, narrow = false, error = null, skipped = 0, disconnected = false, trailing = null }) {
  const remote = kind === "remote";
  return (
    <div style={{ flex: "none", display: "flex", flexDirection: "column", background: "var(--tasty-html-address-bar-bg)", borderBottom: "var(--tasty-border-width) solid var(--tasty-html-address-bar-border)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-html-address-bar-gap)", height: "var(--tasty-html-address-bar-height)", padding: "0 var(--tasty-html-address-bar-padding-x)" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-html-address-bar-nav-gap)", flex: "none" }}>
          {!remote && navBtn(HA_COPY.back, "chevronLeft", { disabled: !canBack })}
          {!remote && !narrow && navBtn(HA_COPY.forward, "chevronRight", { disabled: !canForward })}
          {navBtn(disconnected ? HA_COPY.disconnected : HA_COPY.reload, "refresh", { busy: state === "loading", disabled: disconnected })}
        </div>
        <HtmlAddressField kind={kind} address={address} state={state} skipped={skipped} trailing={trailing} />
      </div>
      {error && (
        <div role="alert" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-html-address-error-height)", padding: "0 var(--tasty-html-address-bar-padding-x)", marginTop: "calc(-1 * var(--tasty-space-xs))" }}>
          <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-html-address-error-glyph)" }}><HaIcon name="alertCircle" size="var(--tasty-icon-size-sm)" /></span>
          <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{error}</span>
        </div>
      )}
    </div>
  );
}

// Remote size banners — same inset shell + button boxes as the script banner. No × : each is a decision.
// kind: file (one large file at a time; "+N more" waiting) · limit (cumulative 2 MB per entry document)
// The title wraps and never clamps, like every other banner. title overrides the copy for the long-title specimen.
function HtmlSizeBanner({ kind = "file", name = "assets/hero-video.mp4", size = "3.2 MB", more = 0, narrow = false, title }) {
  const file = kind === "file";
  return (
    <div data-surface="banner" style={{ background: "var(--tasty-banner-bg)", color: "var(--tasty-banner-fg)", border: "var(--tasty-border-width) solid var(--tasty-banner-border)",
      borderRadius: "var(--tasty-banner-radius)", boxShadow: "var(--tasty-banner-shadow)" }}>
      <div style={{ display: "flex", flexWrap: narrow ? "wrap" : "nowrap", alignItems: narrow ? "flex-start" : "center", gap: "var(--tasty-banner-gap)", padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
        <span style={{ display: "inline-flex", flex: "none", alignSelf: "flex-start", marginTop: "var(--tasty-banner-glyph-offset)", color: file ? "var(--tasty-html-size-banner-glyph)" : "var(--tasty-html-limit-banner-glyph)" }}>
          <HaIcon name={file ? "download" : "alertTriangle"} size="var(--tasty-icon-size-md)" />
        </span>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)" }}>
          <div style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)", lineHeight: "var(--tasty-line-height-ui)" }}>
            {title || (file ? HA_COPY.sizeTitle : HA_COPY.limitTitle)}
          </div>
          {file ? (
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
              <span style={{ flex: "0 1 auto", minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: "rtl", textAlign: "left" }}><bdi>{name}</bdi></span>
              <span style={{ display: "inline-flex", flex: "none" }}><Tag variant="warning">{size}</Tag></span>
              {more > 0 && <span style={{ flex: "none", fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)", whiteSpace: "nowrap" }}>+{more} more waiting</span>}
            </div>
          ) : (
            <div style={{ fontSize: "var(--tasty-banner-body-font-size)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-banner-body-fg)" }}>{HA_COPY.limitBody}</div>
          )}
        </div>
        <span style={{ flex: "none", display: "flex", gap: "var(--tasty-space-sm)", marginLeft: narrow ? "calc(var(--tasty-icon-size-md) + var(--tasty-banner-gap))" : 0, width: narrow ? "100%" : "auto" }}>
          <Button size="sm" variant="ghost" ground="raised" style={{ whiteSpace: "nowrap" }}>{file ? HA_COPY.decline : HA_COPY.stop}</Button>
          <Button size="sm" variant="secondary" style={{ whiteSpace: "nowrap" }}>{file ? HA_COPY.allow : HA_COPY.cont}</Button>
        </span>
      </div>
    </div>
  );
}

// Host states painted in the page rect. receiving: spinner while auto-receiving; paused (a size banner waits): static glyph.
function HaCenter({ glyph, glyphColor, spin, title, titleColor = "var(--tasty-html-receiving-fg)", sub, mono }) {
  return (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", textAlign: "center" }}>
      <span style={{ display: "inline-flex", color: glyphColor }}>{spin ? <Spinner /> : <HaIcon name={glyph} size="var(--tasty-icon-size-md)" />}</span>
      <span style={{ fontSize: "var(--tasty-font-size-body)", color: titleColor }}>{title}</span>
      {sub && <span style={{ fontFamily: mono ? "var(--tasty-font-mono)" : "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", maxWidth: "100%", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sub}</span>}
    </div>
  );
}
function HtmlPageState({ kind = "empty", progress = "1.4 MB · 12 files", url }) {
  switch (kind) {
    case "empty": return <HaCenter glyph="html" glyphColor="var(--tasty-text-disabled)" title={HA_COPY.emptyTitle} sub={<span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}>{HA_COPY.emptyHint} · <Kbd keys={["Alt", "L"]} /></span>} />;
    case "loading": return <HaCenter spin title="Loading…" />;
    case "failed": return <HaCenter glyph="alertCircle" glyphColor="var(--tasty-accent-danger-raised)" title="Failed to load" titleColor="var(--tasty-accent-danger)" sub={url} mono />;
    case "receiving": return <HaCenter spin title={HA_COPY.receiving} sub={progress} mono />;
    case "paused": return <HaCenter glyph="download" glyphColor="var(--tasty-html-receiving-glyph)" title={HA_COPY.receiving} sub={HA_COPY.paused} />;
    case "macos": return <HaCenter glyph="info" glyphColor="var(--tasty-html-receiving-glyph)" title={HA_COPY.macos} sub={HA_COPY.macosSub} />;
    case "disconnected": return <HaCenter glyph="remote" glyphColor="var(--tasty-html-receiving-glyph)" title={HA_COPY.disconnected} sub={HA_COPY.disconnectedSub} />;
    default: return null;
  }
}

// Whole surface: tab strip → band → inset banner stack → page. banners: ["size" | "size-more" | "limit" | "script"]
function HtmlAddressSurfaceG({ width, height = 300, label = "report.html", bar = {}, banners = [], page = "page", pageLabel, progress, url, narrow = false, noBand = false }) {
  
  return (
    <div style={{ width, height, flex: width ? "none" : 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", overflow: "hidden" }}>
      <div style={{ display: "flex", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <HsTab label={label} icon="html" active />
      </div>
      {!noBand && <HtmlAddressBar narrow={narrow} {...bar} />}
      {banners.length > 0 && (
        <div style={{ flex: "none", display: "flex", flexDirection: "column", gap: "var(--tasty-banner-inset-gap)", padding: "var(--tasty-banner-margin) var(--tasty-banner-margin) var(--tasty-banner-inset-gap)" }}>
          {banners.map((b, i) => b === "script"
            ? <HtmlScriptBanner key={i} remote={bar.kind === "remote"} narrow={narrow} />
            : <HtmlSizeBanner key={i} kind={b === "limit" ? "limit" : "file"} more={b === "size-more" ? 2 : 0} narrow={narrow} />)}
        </div>
      )}
      {page === "page" ? <HsPage /> : <HtmlPageState kind={page} progress={progress} url={url} />}
    </div>
  );
}

export { HtmlAddressBar, HtmlAddressField, HtmlSizeBanner, HtmlPageState, HtmlAddressSurfaceG, HA_COPY };
