import React from "react";
import { Spec, Stage, Meta } from "../parts/specimen.jsx";

// Tasty Gallery: Components, Scrollbar. The thumb is drawn as a plain part, because a native scrollbar pseudo-element
// can be neither frozen nor read back. One cell per row of the Scrollbar contract.



const HOSTS = [
  ["crust", "--tasty-bg-app", "--tasty-scrollbar-thumb"],
  ["mantle", "--tasty-bg-sidebar", "--tasty-scrollbar-thumb"],
  ["base", "--tasty-bg-panel", "--tasty-scrollbar-thumb"],
  ["raised", "--tasty-surface-raised", "--tasty-scrollbar-thumb-raised"],
  ["term", "--tasty-surface-terminal-focused-bg", "--tasty-scrollbar-thumb"],
];

function GalleryScrollbar() {
  return (
    <Spec id="scrollbar-hosts" title="Scrollbar: thumb on each host" when="Use a scrollbar to show the scroll position when content exceeds its region. Compare the thumb on app, sidebar, panel, raised, and terminal grounds to find the token for each host.">
      <Stage variant="solo" eager style={{ padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", gap: "var(--tasty-space-sm)" }}>
        {HOSTS.map(([host, ground, thumb]) => (
          <figure key={host} style={{ margin: 0 }}>
            <figcaption style={{ marginBottom: "var(--tasty-space-xs)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{host}</figcaption>
            <div data-contract="Scrollbar" data-state="rest" data-host={host} style={{ position: "relative", width: "var(--tasty-size-64)", height: "var(--tasty-size-96)", background: `var(${ground})`, border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
              <span data-part="thumb" style={{ position: "absolute", top: "var(--tasty-size-8)", right: "var(--tasty-size-2)", width: "var(--tasty-size-6)", height: "var(--tasty-size-48)", borderRadius: "var(--tasty-radius-pill)", background: `var(${thumb})` }} />
            </div>
          </figure>
        ))}
      </Stage>
      <Meta
        specs={[["track", "10 wide, no fill"], ["thumb", "6 wide, 2 from each edge, pill"]]}
        tokens={[
          { tok: "--tasty-scrollbar-thumb", use: "thumb on crust, mantle, base, term", color: "var(--tasty-scrollbar-thumb)" },
          { tok: "--tasty-scrollbar-thumb-raised", use: "thumb on raised", color: "var(--tasty-scrollbar-thumb-raised)" },
        ]} />
    </Spec>
  );
}

export { GalleryScrollbar };
