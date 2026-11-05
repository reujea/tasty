import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as RcIcon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Spinner } from "../../../components/feedback/Spinner.jsx";
import { Tooltip } from "../../../components/feedback/Tooltip.jsx";
import { HtmlAddressBar, HtmlSizeBanner } from "./html_address_bar.jsx";
import { HsTab, HsPage, HtmlScriptBanner } from "./html_script_banner.jsx";

// HTML surface — REMOTE CONTENT blocked notice (inset banner) + address-field marker.
// Designed 2026-11-01.
//
// Same life rules as the script notice (html_script_banner.jsx): fires only when the USER views the document,
// allow binds to the committed document (URL + content fingerprint), Allow reloads once, disabled while loading,
// no banner/marker on a failed (uncommitted) document, none while the global "Allow remote content" is on.
// Remote allow and script allow are independent.
//
// Box boundaries: tab strip → address band [nav | field [kind glyph | address | skipped Tag | REMOTE MARKER]]
//   → inset banner stack (size → REMOTE → script, banner-inset-gap apart) → WebView rect.
// Banner = BannerShell > row [glyph globeOff | body (title + body, wrap, never clamp) | action] + dismiss slot (top-right,
//   hover). Narrow (surface < banner-narrow-below 440): action wraps under the body, body-aligned.
// Marker = 20 cell (html-remote-marker-hit) at the END of the address field. Shown when the banner is NOT:
//   blocked (dismissed, or blocked-observed with no static reference) = globeOff glyph-dim, click → banner shows
//   (never allows on its own); allowed = globe text-muted, tooltip only. Tooltip TOP.




const RC_COPY = {
  title: "Remote content in this document is blocked",
  body: "Stylesheets, images, fonts, frames and scripts from the network don't load, so the page may look incomplete. Allowing applies to this document only, until Tasty restarts.",
  action: "Allow remote content",
  reloading: "Reloading with remote content allowed",
  actionLoading: "Available when the document finishes loading",
  markerBlocked: "Remote content blocked. Click for details.",
  markerAllowed: "Remote content allowed for this document until Tasty restarts",
};

// state: "blocked" | "loading" | "reloading" — identical visual rules to HtmlScriptBanner.
function HtmlRemoteBanner({ state = "blocked", hover = false, narrow = false }) {
  const [h, setH] = React.useState(false);
  const showX = (hover || h) && state !== "reloading";
  const reloading = state === "reloading";
  const action = reloading ? (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-button-height-sm)", fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)", whiteSpace: "nowrap" }}>
      <Spinner size="var(--tasty-icon-size-sm)" />{RC_COPY.reloading}
    </span>
  ) : state === "loading" ? (
    <Tooltip placement="top" content={RC_COPY.actionLoading}>
      <span style={{ display: "inline-flex" }}><Button size="sm" variant="secondary" disabled style={{ whiteSpace: "nowrap" }}>{RC_COPY.action}</Button></span>
    </Tooltip>
  ) : (
    <Button size="sm" variant="secondary" style={{ whiteSpace: "nowrap" }}>{RC_COPY.action}</Button>
  );
  return (
    <div data-surface="banner" onMouseEnter={() => setH(true)} onMouseLeave={() => setH(false)} style={{ position: "relative",
      background: "var(--tasty-banner-bg)", color: "var(--tasty-banner-fg)", border: "var(--tasty-border-width) solid var(--tasty-banner-border)",
      borderRadius: "var(--tasty-banner-radius)", boxShadow: "var(--tasty-banner-shadow)" }}>
      <div style={{ display: "flex", flexWrap: narrow ? "wrap" : "nowrap", alignItems: narrow ? "flex-start" : "center", gap: "var(--tasty-banner-gap)",
        padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)",
        paddingRight: "calc(var(--tasty-icon-button-size-sm) + var(--tasty-space-sm) + var(--tasty-space-xs))" }}>
        <span style={{ display: "inline-flex", flex: "none", alignSelf: "flex-start", marginTop: "var(--tasty-banner-glyph-offset)", color: "var(--tasty-html-remote-banner-glyph)" }}>
          <RcIcon name="globeOff" size="var(--tasty-icon-size-md)" />
        </span>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-banner-text-gap)" }}>
          <div style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)", lineHeight: "var(--tasty-line-height-ui)", textWrap: "pretty" }}>{RC_COPY.title}</div>
          <div style={{ fontSize: "var(--tasty-banner-body-font-size)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-banner-body-fg)", textWrap: "pretty" }}>{RC_COPY.body}</div>
        </div>
        <span style={{ flex: "none", display: "flex", marginLeft: narrow ? "calc(var(--tasty-icon-size-md) + var(--tasty-banner-gap))" : 0, width: narrow ? "100%" : "auto" }}>{action}</span>
      </div>
      <span style={{ position: "absolute", top: "var(--tasty-banner-padding-y)", right: "var(--tasty-space-sm)", display: "flex",
        opacity: showX ? 1 : 0, transition: "opacity var(--tasty-banner-fade) var(--tasty-ease-ui)" }}>
        <IconButton size="sm" aria-label="Dismiss"><RcIcon name="close" /></IconButton>
      </span>
    </div>
  );
}

// kind: "blocked" | "allowed"
function HtmlRemoteMarker({ kind = "blocked", hover = false }) {
  const allowed = kind === "allowed";
  const [h, setH] = React.useState(false);
  const label = allowed ? RC_COPY.markerAllowed : RC_COPY.markerBlocked;
  return (
    <Tooltip placement="top" content={label}>
      <span role={allowed ? "img" : "button"} aria-label={label} onMouseEnter={() => setH(true)} onMouseLeave={() => setH(false)}
        style={{ display: "inline-flex", flex: "none", alignItems: "center", justifyContent: "center", marginRight: "calc(-1 * var(--tasty-space-xs))",
          width: "var(--tasty-html-remote-marker-hit)", height: "var(--tasty-html-remote-marker-hit)", borderRadius: "var(--tasty-radius-sm)",
          cursor: allowed ? "default" : "pointer", background: !allowed && (hover || h) ? "var(--tasty-html-remote-marker-hover-bg)" : "transparent",
          color: allowed ? "var(--tasty-html-remote-marker-allowed-fg)" : hover || h ? "var(--tasty-html-remote-marker-fg-hover)" : "var(--tasty-html-remote-marker-fg)" }}>
        <RcIcon name={allowed ? "globe" : "globeOff"} size="var(--tasty-html-remote-marker-size)" />
      </span>
    </Tooltip>
  );
}

// stand-in for a page whose remote stylesheet/images did not load (not tasty chrome)
function RcUnstyledPage() {
  const line = (w) => <span style={{ height: "var(--tasty-space-sm)", width: w, background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius-sm)" }} />;
  const broken = (w) => (
    <span style={{ width: w, height: "var(--tasty-size-40)", flex: "none", border: "var(--tasty-border-width) dashed var(--tasty-border-default)", borderRadius: "var(--tasty-radius-sm)",
      display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--tasty-text-disabled)" }}><RcIcon name="image" size="var(--tasty-icon-size-sm)" /></span>
  );
  return (
    <div style={{ flex: 1, minHeight: 0, background: "var(--tasty-surface-terminal-focused-bg)", padding: "var(--tasty-space-md)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-placeholder)" }}>WebView — report.html · remote CSS + images not loaded</span>
      {line("40%")}{line("92%")}
      <span style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>{broken("var(--tasty-size-64)")}{broken("var(--tasty-size-64)")}</span>
      {line("75%")}
    </div>
  );
}

// Whole surface for the comparison grid.
// remote / script: banner state or null · remoteMarker: "blocked" | "allowed" | null · scriptMarker: tab marker kind or null
// size: mirror size banner on top ("file" | "limit" | null) · page: "unstyled" | "styled"
function HtmlRemoteSurfaceG({ width, height = 300, narrow = false, remote = null, script = null, remoteMarker = null, scriptMarker = null, size = null, page = "unstyled", mirror = false }) {
  
  const stack = [
    size && <HtmlSizeBanner key="size" kind={size} narrow={narrow} />,
    remote && <HtmlRemoteBanner key="remote" state={remote} narrow={narrow} />,
    script && <HtmlScriptBanner key="script" state={script} remote={!!remote} narrow={narrow} />,
  ].filter(Boolean);
  return (
    <div style={{ width, height, flex: width ? "none" : 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", overflow: "hidden" }}>
      <div style={{ display: "flex", flex: "none", background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <HsTab label="report.html" icon="html" active marker={scriptMarker} />
      </div>
      <HtmlAddressBar kind={mirror ? "remote" : "local"} address={mirror ? "build-eu:~/reports/q3/report.html" : "~/reports/q3/report.html"} narrow={narrow}
        trailing={remoteMarker ? <HtmlRemoteMarker kind={remoteMarker} /> : null} />
      {stack.length > 0 && (
        <div style={{ flex: "none", display: "flex", flexDirection: "column", gap: "var(--tasty-banner-inset-gap)", padding: "var(--tasty-banner-margin) var(--tasty-banner-margin) var(--tasty-banner-inset-gap)" }}>{stack}</div>
      )}
      {page === "styled" ? <HsPage /> : <RcUnstyledPage />}
    </div>
  );
}

export { HtmlRemoteBanner, HtmlRemoteMarker, HtmlRemoteSurfaceG, RC_COPY };
