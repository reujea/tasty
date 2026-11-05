import React from "react";
import { Spec, Stage, Meta } from "../parts/specimen.jsx";

// Tasty Gallery: Dialogs, the scrim over each ground of the location map. What sits under a scrim is inert, so it
// carries data-audit-exempt="under-scrim". The frame above it is measured like any other popup.



const GROUNDS = [
  ["scrim@base", "--tasty-bg-panel"],
  ["scrim@crust", "--tasty-bg-app"],
  ["scrim@mantle", "--tasty-bg-sidebar"],
  ["scrim@raised", "--tasty-surface-raised"],
  ["scrim@active", "--tasty-surface-active"],
  ["scrim@term", "--tasty-surface-terminal-focused-bg"],
];

function GalleryScrimGrounds() {
  return (
    <Spec id="scrim-grounds" title="Scrim over each ground" when="Use a scrim to distinguish the inactive region behind a modal or popup. Compare the same scrim over each ground, with the frame above it on the panel ground.">
      <Stage variant="solo" eager style={{ padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", gap: "var(--tasty-space-sm)" }}>
        {GROUNDS.map(([id, ground]) => (
          <figure key={id} style={{ margin: 0 }}>
            <figcaption style={{ marginBottom: "var(--tasty-space-xs)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{id}</figcaption>
            <div style={{ position: "relative", width: "var(--tasty-size-160)", height: "var(--tasty-size-96)", background: `var(${ground})`, borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <span data-audit-exempt="under-scrim" style={{ position: "absolute", top: "var(--tasty-space-xs)", left: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>Under scrim</span>
              <span style={{ position: "absolute", inset: 0, background: "var(--tasty-scrim-bg)" }} />
              <span style={{ position: "absolute", left: "50%", top: "50%", transform: "translate(-50%, -50%)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", whiteSpace: "nowrap" }}>Frame on base</span>
            </div>
          </figure>
        ))}
      </Stage>
      <Meta
        specs={[["child", "base only, as a modal or popup frame"], ["under the scrim", "inert, no target"]]}
        tokens={[
          { tok: "--tasty-scrim-bg", use: "the one scrim, every ground", color: "var(--tasty-scrim-bg)" },
          { tok: "--tasty-border-frame", use: "frame edge", color: "var(--tasty-border-frame)" },
        ]} />
    </Spec>
  );
}

export { GalleryScrimGrounds };
