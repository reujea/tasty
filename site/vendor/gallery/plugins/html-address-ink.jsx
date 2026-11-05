import React from "react";
import { HtmlAddressBar as Kit_HtmlAddressBar } from "../../ui_kits/terminal/overlays/html_address_bar.jsx";
import { Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Plugins › HTML viewer › address field ink: idle + read-only real value (2026-10-21).
// Box, geometry and state contract unchanged; only the value ink role.

const HcPair = ({ children }) => (
  <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-lg)", alignItems: "flex-start" }}>
    {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
      <div key={label} {...(t ? { "data-theme": t } : {})} style={{ flex: "1 1 var(--tasty-size-440)", minWidth: 0, maxWidth: "100%", display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
      </div>
    ))}
  </div>
);
const HcCase = ({ label, children }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
    <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{label}</span>
    <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>{children}</div>
  </div>
);
// before / after ink on the real field ground (input-bg = surface-raised)
function HcInk({ tok, label }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0, flex: "1 1 var(--tasty-size-200)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{label}</span>
      <span style={{ display: "flex", alignItems: "center", height: "var(--tasty-input-height)", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-input-bg)", border: "var(--tasty-border-width) solid var(--tasty-input-border)", borderRadius: "var(--tasty-input-radius)",
        fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-input-font-size)", color: "var(" + tok + ")", overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" }}>~/work/tasty/target/report.html</span>
    </div>
  );
}

function HtmlAddressContrastSpec() {

  const Bar = Kit_HtmlAddressBar;
  const st = { padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", alignItems: "stretch", gap: "var(--tasty-space-md)" };
  return (
    <Spec id="address-field-ink-idle-and-read-only-real-values" title="Address field ink — idle and read-only real values"
      when={<>The idle address and the mirror's read-only address are the <b>real value</b> the user reads, selects and copies, so they need body-text contrast (4.5:1). Both were <code>text-secondary</code> on the field ground (<code>input-bg</code> / <code>input-readonly-bg</code> → surface-raised): 7.10 in Mocha but <b>4.05 in Latte</b>. They now use <code>text-secondary-raised</code>, the existing role for secondary ink on surface-raised (Mocha = text-secondary, Latte = neutral-1100). Mocha does not change. In Latte the idle and editing values share one ink; editing is still told apart by the focus edge, the ring and the caret. Placeholder, disabled, invalid edge and error row keep their own roles.</>}>
      <Stage variant="solo" style={st}>
        <HcPair>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-md)" }}>
            <HcInk tok="--tasty-text-secondary" label="before · text-secondary" />
            <HcInk tok="--tasty-html-address-value-fg" label="after · html-address-value-fg" />
          </div>
          <HcCase label="local · idle"><Bar kind="local" address="~/work/tasty/target/report.html" /></HcCase>
          <HcCase label="local · editing (unchanged: input-fg)"><Bar kind="local" address="~/work/tasty/target/report.html" state="editing" /></HcCase>
          <HcCase label="web · idle"><Bar kind="web" address="https://docs.example.com/guide/getting-started#install" canForward /></HcCase>
          <HcCase label="mirror · read-only, selectable (input-readonly-fg)"><Bar kind="remote" address="build-eu:~/work/site/index.html" /></HcCase>
          <HcCase label="empty · placeholder (unchanged)"><Bar kind="empty" canBack={false} /></HcCase>
          <HcCase label="loading (unchanged)"><Bar kind="web" address="https://docs.example.com/guide/getting-started#install" state="loading" /></HcCase>
          <HcCase label="input error (unchanged)"><Bar kind="local" address="/tmp/x/missing.html" state="error" error="File not found: /tmp/x/missing.html" /></HcCase>
          <HcCase label="mirror · disconnected (Reload disabled; value stays read-only ink)"><Bar kind="remote" address="build-eu:~/work/site/index.html" disconnected /></HcCase>
        </HcPair>
      </Stage>
      <Meta
        specs={[["idle value", "html-address-value-fg → text-secondary-raised · Mocha 7.10 · Latte 5.17 (was 4.05)"], ["read-only value (mirror)", "input-readonly-fg → text-secondary-raised · same numbers · no opacity, stays selectable / copyable"], ["editing value", "input-fg → text-primary · Mocha 8.69 · Latte 5.17 · unchanged"], ["placeholder", "input-placeholder → text-placeholder · unchanged"], ["disabled", "state-disabled-fg · unchanged (disabled is a different state from read-only)"], ["invalid / error row", "input-border-invalid edge · alertCircle html-address-error-glyph · caption text-primary · unchanged"], ["box / geometry", "band 40 · field 28 · min 96 · padding 8 · nav gap 4 · error row 24 · Forward hidden < 440 — all unchanged"]]}
        tokens={[{ tok: "--tasty-html-address-value-fg", use: "new · → text-secondary-raised", color: "var(--tasty-html-address-value-fg)" }, { tok: "--tasty-input-readonly-fg", use: "changed · text-secondary → text-secondary-raised", color: "var(--tasty-input-readonly-fg)" }, { tok: "--tasty-md-address-value-fg", use: "new · → html-address-value-fg (same box in the Markdown band)", color: "var(--tasty-md-address-value-fg)" }]} />
      <Note><b>Scope.</b> <code>input-readonly-fg</code> is changed for every read-only Input, because the read-only box is surface-raised everywhere and the same 4.05 applies to all of them. Mocha is identical, so only Latte moves (n1000 → n1100). Consumers: Input <code>readOnly</code> (Appearance colour rows at Default; Explorer remote rename while waiting), <code>fp-folder-path-fg</code> (file picker folder mode path readout), the HTML mirror address field. Input <code>ground="raised"</code> read-only sits on <code>state-disabled-fill-raised</code> (one step lower than surface-raised), where n1100 reads higher still. Disabled ink, placeholder and the editing ink are not touched. The Markdown band's idle path had the same text-secondary on the same box; it now uses <code>md-address-value-fg</code>. No new strings.</Note>
    </Spec>
  );
}

export { HtmlAddressContrastSpec };
