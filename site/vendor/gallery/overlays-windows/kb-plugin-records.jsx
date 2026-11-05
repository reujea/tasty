import React from "react";
import { KbPluginsSubtab as Kit_KbPluginsSubtab } from "../../ui_kits/terminal/overlays/kb_plugins_subtab.jsx";
import { Spec as WSpec, Stage as WStage, Meta as WMeta } from "../parts/specimen.jsx";
import { wLbl } from "./settings-narrow.jsx";

function KbPluginsRecordSpec() {
  const T = { KbPluginsSubtab: Kit_KbPluginsSubtab };
  return (
    <WSpec id="keybindings-plugins-custom-record-slots" title="Keybindings › Plugins — Custom = record slots"
      when={<>Custom now records, using the same record slot as the other subtabs (one slot per key, then +, or a None slot), with two local changes so the line keeps <b>one control height</b>: slots here are <span className="tok">--tasty-kb-plugin-record-height</span> 28, like the mode Select and Reset. Slots are one group (space-xs 4 between them); the group's ends keep the line's 8. The slot column is no longer a fixed 200 box: one key is 140 + 4 + 32 = 176, more keys push Reset right. When the line runs out of width it <b>wraps as one flow</b>: the next line starts at the <b>mode Select x</b> (the same control x where other subtabs wrap), and Reset stays the <b>last item</b>. Every key is shown; the single-button sample is retired. The Esc hint sits under the subtab as on the other subtabs.</>}>
      <WStage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-panel)", gap: "var(--tasty-space-xl)", flexDirection: "column", alignItems: "flex-start" }}>
        {T && T.KbPluginsSubtab && <>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 620 }}>
            <span style={wLbl}>recording — two keys + a third being recorded · a Custom command with no key shows the None slot</span>
            <T.KbPluginsSubtab saved={{}} seedDrafts={{ "clipboard-viewer/open": { mode: "custom", keys: "ctrl+shift+h,ctrl+alt+v", recording: true }, "clipboard-viewer/clear": { mode: "custom", keys: "" } }} />
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 460 }}>
            <span style={wLbl}>narrow (460) — three keys wrap from the mode x · Reset last</span>
            <T.KbPluginsSubtab saved={{}} seedDrafts={{ "clipboard-viewer/open": { mode: "custom", keys: "ctrl+shift+h,ctrl+alt+v,ctrl+f5" } }} />
          </div>
        </>}
      </WStage>
      <WMeta
        specs={[["slot", "kb_record_slot · min kb-record-width 140 × kb-plugin-record-height 28 · + kb-record-add-width 32 · None slot 140"], ["gaps", "space-xs 4 between slots · kb-plugin-control-gap 8 around the group"], ["width", "grows with keys · Reset pushed right"], ["wrap", "one flow · next line at the mode Select x · row gap 4 · Reset last"], ["recording", "“Press key combination...” takes the + slot's place, grows to its text"], ["unparsable config text", "slot shows the raw text with the danger border + the error caption"]]}
        tokens={[{ tok: "--tasty-kb-plugin-record-height", use: "→ kb-plugin-control-height 28" }, { tok: "--tasty-kb-record-width", use: "140" }, { tok: "--tasty-kb-record-add-width", use: "32" }, { tok: "--tasty-kb-record-border", use: "rest", color: "var(--tasty-kb-record-border)" }, { tok: "--tasty-kb-record-border-hover", use: "hover", color: "var(--tasty-kb-record-border-hover)" }]} />
    </WSpec>
  );
}

export { KbPluginsRecordSpec };
