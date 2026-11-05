import React from "react";
import { Section as WSection, Spec as WSpec, Stage as WStage, Meta as WMeta, Note as WNote } from "../parts/specimen.jsx";
import { wLbl, wRec, WKbd, WInput, WIcon } from "./settings-narrow.jsx";

const wOS = [["macOS", "⌃⌥K", "Option"], ["Windows", "Ctrl+Win+K", "Win"], ["Linux", "Super+Shift+1", "Super"]];
function OptionKeySection() {
  return (
    <WSection id="optionkey" title="Win · Super as option — names and notices">
      <WSpec id="key-names-os-reserved-caption-text-entry-hint" title="Key names · OS-reserved caption · text-entry hint"
        when={<>The <code>option</code> token now also means <b>Win</b> on Windows and <b>Super</b> on Linux. They are shown as <b>words</b>, like Ctrl and Alt — no logo glyph (the Windows logo is a trademark and Linux keyboards print different marks). Linux uses <b>Super</b> on every desktop, KDE included, so one config reads the same everywhere. The display-style dropdown stays macOS-only (there is nothing to choose between on the other two). A shortcut <b>typed as text</b> (config file, bundle import, plugin shortcut field) that matches a short known list of OS-owned combos gets a warning caption — the list is a hint, not a guarantee, and the copy says “may”. A text field that receives an OS key name (<code>cmd+k</code>, <code>super+k</code>, <code>win+k</code>, <code>meta+k</code>) gets a dedicated caption saying how to write it.</>}>
        <WStage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-panel)", gap: 24, flexWrap: "wrap", alignItems: "flex-start" }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            <span style={wLbl}>shortcut display per OS</span>
            {wOS.map(([os, combo, word]) => (
              <div key={os} style={{ display: "grid", gridTemplateColumns: "72px 140px 1fr", alignItems: "center", gap: 8 }}>
                <span style={{ fontSize: 12, color: "var(--tasty-text-secondary)" }}>{os}</span>
                <span style={wRec}>{combo}</span>
                <WKbd keys={word} />
              </div>
            ))}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4, width: 300 }}>
            <span style={wLbl}>plugin shortcut field — OS-reserved (Windows)</span>
            <WInput block mono defaultValue="option+l" />
            <span style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-kb-os-reserved-fg)" }}><span style={{ display: "inline-flex", flex: "none", marginTop: 1 }}><WIcon name="alertTriangle" size="var(--tasty-icon-size-xs)" /></span>Windows may use Win+L itself, so Tasty might not receive it.</span>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4, width: 300 }}>
            <span style={wLbl}>plugin shortcut field — OS key name typed</span>
            <WInput block mono defaultValue="cmd+k" invalid />
            <span style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-kb-plugin-error-fg)" }}>Write Win, Super and Option as <code style={{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", background: "var(--tasty-ui-code-bg)", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)" }}>option</code>, and Cmd as <code style={{ fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", background: "var(--tasty-ui-code-bg)", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)" }}>alt</code>.</span>
          </div>
        </WStage>
        <WMeta
          specs={[["names", "macOS Option / ⌥ (display setting) · Windows Win · Linux Super (all desktops) — words, no icon asset"], ["display dropdown", "macOS only"], ["reserved list (hint)", "Windows: Win alone, Win+L · D · E · R · I · Tab · Linux: Super alone"], ["reserved caption", "warning tone · under the field / one line in the import warning block · never blocks saving"], ["recorded", "no caption (the OS key never reaches the recorder)"], ["OS key name", "error caption replacing the generic “Unrecognized key” · code runs for the tokens"], ["hold helper", "no option binding on Win / Linux → holding Win / Super shows nothing (agreed)"]]}
          tokens={[{ tok: "--tasty-kb-os-reserved-fg", use: "→ accent-warning", color: "var(--tasty-kb-os-reserved-fg)" }, { tok: "--tasty-kb-plugin-error-fg", use: "OS key name", color: "var(--tasty-kb-plugin-error-fg)" }]} />
        <WNote>i18n — <code>keys.os_reserved.windows</code> “Windows may use {"{combo}"} itself, so Tasty might not receive it.” / “Windows가 {"{combo}"}를 먼저 쓸 수 있어 Tasty에 전달되지 않을 수 있습니다.” / “{"{combo}"} は Windows が使うことがあり、Tasty に届かない場合があります。” · <code>keys.os_reserved.linux</code> “Your desktop may use {"{combo}"} itself, so Tasty might not receive it.” / “데스크톱이 {"{combo}"}를 먼저 쓸 수 있어 Tasty에 전달되지 않을 수 있습니다.” / “{"{combo}"} はデスクトップが使うことがあり、Tasty に届かない場合があります。” · <code>keys.os_key_name</code> “Write Win, Super and Option as `option`, and Cmd as `alt`.” / “Win·Super·Option 은 `option`, Cmd 는 `alt` 로 적으세요.” / “Win・Super・Option は `option`、Cmd は `alt` と書いてください。” (backticks = code runs). The option-migration card and its gallery sample are removed (feature deleted).</WNote>
      </WSpec>
    </WSection>
  );
}

export { OptionKeySection };
