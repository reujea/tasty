import React from "react";
import { Icon } from "../../components/index.js";
import { Spec, Stage, Meta } from "../parts/specimen.jsx";

// Tasty Gallery: Layouts, Title bar. Each cell is one row of the Title bar, Title bar (inactive) and Window close
// button contracts, painted with the tokens the contract names.




const CELL = { display: "grid", placeItems: "center", width: "var(--tasty-titlebar-caption-width)", height: "var(--tasty-titlebar-height)", flex: "none" };
const STATE_BG = { rest: "transparent", hover: "var(--tasty-titlebar-button-hover-bg)", press: "var(--tasty-titlebar-button-active-bg)" };
const STATE_FG = { rest: "var(--tasty-titlebar-button-fg)", hover: "var(--tasty-titlebar-button-fg-hover)", press: "var(--tasty-titlebar-button-fg-hover)" };

const WINDOWS = [
  { label: "Active", contract: "Title bar", host: "crust", bg: "var(--tasty-titlebar-bg)", fg: "var(--tasty-titlebar-fg)" },
  { label: "Inactive", contract: "Title bar (inactive)", host: "mantle", bg: "var(--tasty-titlebar-bg-inactive)", fg: "var(--tasty-titlebar-fg-inactive)" },
];

function Band({ w, children }) {
  return (
    <div style={{ display: "flex", alignItems: "stretch", width: "100%", minWidth: "var(--tasty-size-320)", height: "var(--tasty-titlebar-height)", background: w.bg }}>
      <span data-part="title" style={{ flex: 1, minWidth: 0, alignSelf: "center", padding: "0 var(--tasty-space-md)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: "var(--tasty-font-size-body)", color: w.fg }}>agents-prod</span>
      {children}
    </div>
  );
}

function CaptionCell({ w, state }) {
  return (
    <span style={{ ...CELL, background: STATE_BG[state], color: STATE_FG[state] }}>
      <Icon name="chevronDown" size="var(--tasty-icon-size-xs)" data-part="button" />
    </span>
  );
}

function CloseCell({ w }) {
  return (
    <span style={{ ...CELL, background: "var(--tasty-titlebar-close-hover-bg)", color: "var(--tasty-titlebar-close-hover-fg)" }}>
      <Icon name="close" size="var(--tasty-icon-size-xs)" data-part="glyph" />
    </span>
  );
}

const STAGE = { padding: "var(--tasty-space-md)", background: "var(--tasty-bg-panel)", flexDirection: "column", gap: "var(--tasty-space-sm)", overflowX: "auto" };

function GalleryTitleBar() {
  return (
    <Spec id="titlebar-states" title="Title bar: active and inactive, rest, hover, press, close" when="Use a title bar to show whether a window is active. Compare the title and buttons in active and inactive windows, including rest, hover, and press states and the close button's hover treatment.">
      <Stage variant="solo" eager style={STAGE}>
        {WINDOWS.map((w) => (
          <div key={w.label} style={{ width: "100%", minWidth: 0 }}>
            <div style={{ marginBottom: "var(--tasty-space-xs)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{w.label}: rest, hover, press, close</div>
            <Band w={w}>
              {["rest", "hover", "press"].map((s) => <CaptionCell key={s} w={w} state={s} />)}
              <CloseCell w={w} />
            </Band>
          </div>
        ))}
      </Stage>
      <Meta
        specs={[["height", "titlebar-height 36"], ["caption cell", "titlebar-caption-width by titlebar-height"], ["glyph", "icon-size-xs 12"], ["title", "body, ellipsis at the right end"]]}
        tokens={[
          { tok: "--tasty-titlebar-bg", use: "active ground, crust", color: "var(--tasty-titlebar-bg)" },
          { tok: "--tasty-titlebar-bg-inactive", use: "inactive ground, mantle", color: "var(--tasty-titlebar-bg-inactive)" },
          { tok: "--tasty-titlebar-fg", use: "active title", color: "var(--tasty-titlebar-fg)" },
          { tok: "--tasty-titlebar-fg-inactive", use: "inactive title", color: "var(--tasty-titlebar-fg-inactive)" },
          { tok: "--tasty-titlebar-button-fg", use: "button at rest", color: "var(--tasty-titlebar-button-fg)" },
          { tok: "--tasty-titlebar-button-fg-hover", use: "button under hover and press", color: "var(--tasty-titlebar-button-fg-hover)" },
          { tok: "--tasty-titlebar-close-hover-bg", use: "close fill", color: "var(--tasty-titlebar-close-hover-bg)" },
          { tok: "--tasty-titlebar-close-hover-fg", use: "close glyph", color: "var(--tasty-titlebar-close-hover-fg)" },
        ]} />
    </Spec>
  );
}

export { GalleryTitleBar };
