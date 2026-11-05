import React from "react";
import * as WDS from "../../components/index.js";
import { Section as WSection, Spec as WSpec, Stage as WStage, Meta as WMeta } from "../parts/specimen.jsx";

// Tasty Gallery — Windows, batch 12 (2026-10-10): settings rows at narrow widths · Plugins Custom record slots ·
// Win / Super as option. Loaded before overlays-windows.jsx, which places these sections in its Page.


const { Select: WSelect, Input: WInput, Kbd: WKbd, Icon: WIcon } = WDS;

const wRec = { height: "var(--tasty-kb-record-height)", minWidth: "var(--tasty-kb-record-width)", display: "inline-flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box", border: "var(--tasty-border-width) solid var(--tasty-kb-record-border)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", padding: "0 var(--tasty-space-sm)" };
const wAdd = { ...wRec, minWidth: 0, width: "var(--tasty-kb-record-add-width)", padding: 0, background: "transparent", color: "var(--tasty-text-muted)" };
const wLbl = { fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
const wBox = "var(--tasty-border-width) dashed var(--tasty-border-strong)";

// One settings Row. stacked = label on its own line, control at the row start (box outlines = implementer boundaries).
function WRow({ label, stacked, boxes, children }) {
  const cell = boxes ? { outline: wBox, outlineOffset: -1 } : null;
  return (
    <div style={{ display: "flex", flexDirection: stacked ? "column" : "row", alignItems: stacked ? "stretch" : "center", gap: stacked ? "var(--tasty-settings-row-stack-gap)" : "var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)" }}>
      <span style={{ ...cell, flex: "none", width: stacked ? "auto" : "var(--tasty-settings-label-width)", minHeight: stacked ? 0 : "var(--tasty-settings-row-min-height)", display: "flex", alignItems: "center", fontSize: 13, color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)", overflowWrap: "anywhere" }}>{label}</span>
      <span style={{ ...cell, display: "flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-xs)", minWidth: 0 }}>{children}</span>
    </div>
  );
}
// A settings content column at a given window width (sidebar 200 fixed; content inset space-lg each side).
function WSettingsCol({ win, title, stacked, children }) {
  const w = win - 200 - 2 * 16;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
      <span style={wLbl}>{title} — window {win} · content {w}</span>
      <div style={{ width: w, display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-gap)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", boxSizing: "content-box" }}>
        {children(stacked)}
      </div>
    </div>
  );
}
const wRows = (st) => (
  <>
    <WRow label="Next workspace" stacked={st} boxes><span style={wRec}>Ctrl+Alt+Down</span><span style={wAdd}><WIcon name="plus" size="var(--tasty-icon-size-sm)" /></span></WRow>
    <WRow label="Quick switch modifier" stacked={st} boxes><WSelect options={[{ value: "a", label: "Ctrl+Alt" }]} value="a" style={{ width: "var(--tasty-field-width-md)" }} /></WRow>
    <WRow label="Run “format-buffer.lua”" stacked={st} boxes><span style={{ ...wRec, color: "var(--tasty-kb-record-empty-fg)", fontFamily: "var(--tasty-font-ui)" }}>None</span></WRow>
    <WRow label="Maximum size" stacked={st} boxes><span style={{ width: "var(--tasty-field-width-xs)", flex: "none" }}><WInput block defaultValue="512" style={{ fontFamily: "var(--tasty-font-mono)", textAlign: "right" }} /></span><span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>MB</span></WRow>
  </>
);

function SettingsNarrowSection() {
  return (
    <WSection id="settingsnarrow" title="Settings rows at narrow widths — stacked Row">
      <WSpec id="row-side-by-side-stacked-when-the-control-doesn-t-fit" title="Row — side by side · stacked when the control doesn't fit"
        when={<>Below the supported minimum (a tiling WM ignoring 1100 × 700) a one-piece control can be wider than what is left beside the label. Each <b>Row decides on its own</b>: when its control's natural width does not fit in <i>content − label column − gap</i>, the label takes the <b>full row width</b> and the control moves under it at the <b>row start x</b> (not the control column), <span className="tok">--tasty-settings-row-stack-gap</span> 4 below. Rows whose control fits stay side by side, so a subtab can mix both. To stop flicker while resizing, a stacked row goes back only when the control fits with <span className="tok">--tasty-settings-row-stack-hysteresis</span> 16 to spare. Inside a stacked control the existing wraps still apply (second shortcut buttons wrap from the control x, units drop under the field). Dashed outlines are the label / control boxes.</>}>
        <WStage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <WSettingsCol win={700} title="side by side (fits)" stacked={false}>{wRows}</WSettingsCol>
          <WSettingsCol win={520} title="stacked" stacked>{wRows}</WSettingsCol>
          <WSettingsCol win={420} title="stacked" stacked>{wRows}</WSettingsCol>
        </WStage>
        <WMeta
          specs={[["switch", "per Row · control natural width > content − label column − settings-label-gap"], ["stacked", "label full width · control at row start · gap settings-row-stack-gap 4"], ["back", "only when it fits with settings-row-stack-hysteresis 16 spare"], ["row height", "min settings-row-min-height 32 applies to each line"], ["captions", "unchanged — measure-md or the remaining width, whichever is smaller"], ["inside", "existing wraps (buttons from control x · unit under the field)"]]}
          tokens={[{ tok: "--tasty-settings-row-stack-gap", use: "→ space-xs 4" }, { tok: "--tasty-settings-row-stack-hysteresis", use: "→ space-lg 16" }, { tok: "--tasty-settings-label-width", use: "150 (side by side only)" }, { tok: "--tasty-settings-label-gap", use: "16" }]} />
      </WSpec>
    </WSection>
  );
}

export { wLbl, wRec, WKbd, WInput, WIcon, SettingsNarrowSection };
