import React from "react";
const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty UI kit — CSD titlebar · macOS
// ─────────────────────────────────────────────────────────────────────────
// ⚠ NON-PRODUCT / EXPLORATION. The shipping product uses the NATIVE titlebar on
//   every OS (WindowAttributes::default(); zero decoration overrides). This file
//   designs what Tasty would draw if it switched to Client-Side Decorations —
//   adopting it requires WindowAttributes::with_decorations(false) + custom
//   drag/controls in code. Do NOT treat this as an implemented surface.
//
// macOS conventions: traffic lights (close · minimize · zoom) inset on the LEFT;
// glyphs (× − +) appear on hover; lights go grey when the window is unfocused.
// All color/size from --tasty-* tokens (component → semantic → primitive).

const TB_MAC_CSS = `
.tb-mac { display:flex; align-items:center; height:var(--tasty-titlebar-height); flex:none;
  background:var(--tasty-titlebar-bg); border-bottom:var(--tasty-border-width) solid var(--tasty-titlebar-border);
  padding:0 var(--tasty-space-md); gap:var(--tasty-space-md); -webkit-user-select:none; user-select:none; }
.tb-mac[data-active="false"] { background:var(--tasty-titlebar-bg-inactive); }
.tb-mac__lights { display:flex; gap:var(--tasty-space-sm); flex:none; }
.tb-mac__light { width:var(--tasty-titlebar-traffic-size); height:var(--tasty-titlebar-traffic-size);
  border-radius:var(--tasty-radius-pill); display:grid; place-items:center; }
.tb-mac__light svg { width:var(--tasty-size-8); height:var(--tasty-size-8); opacity:0; color:rgba(0,0,0,.55); }
.tb-mac__lights:hover .tb-mac__light svg { opacity:1; }
.tb-mac[data-active="false"] .tb-mac__light { background:var(--tasty-titlebar-traffic-inactive) !important; }
/* center = OS window drag region (everything not interactive) */
.tb-mac__drag { -webkit-app-region:drag; }
`;

function TitlebarMacOS({ active = true, variant = "split", title = "agents-prod", tabs = ["build · cargo", "README.md", "scratch"] }) {
  const G = {
    close: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M6 6l12 12M18 6 6 18"/></svg>,
    min:   <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M5 12h14"/></svg>,
    zoom:  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg>,
  };
  const lights = (
    <div className="tb-mac__lights">
      <span className="tb-mac__light" style={{ background: "var(--tasty-titlebar-traffic-close)" }} title="Close">{G.close}</span>
      <span className="tb-mac__light" style={{ background: "var(--tasty-titlebar-traffic-min)" }} title="Minimize">{G.min}</span>
      <span className="tb-mac__light" style={{ background: "var(--tasty-titlebar-traffic-zoom)" }} title="Zoom">{G.zoom}</span>
    </div>
  );
  const fg = active ? "var(--tasty-titlebar-fg)" : "var(--tasty-titlebar-fg-inactive)";

  if (variant === "tabs") {
    return (
      <div className="tb-mac" data-active={active}>
        {lights}
        <div style={{ display: "flex", gap: "var(--tasty-space-xs)", alignItems: "stretch", height: "100%", flex: 1, minWidth: 0 }}>
          {tabs.map((t, i) => (
            <div key={t} className={i === 0 ? "" : "tb-mac__drag"} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)",
              padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-term-sm)", maxWidth: "var(--tasty-tab-width)", minWidth: 0,
              color: i === 0 ? "var(--tasty-titlebar-fg)" : "var(--tasty-text-muted)",
              background: i === 0 ? "var(--tasty-bg-panel)" : "transparent",
              borderRadius: "var(--tasty-radius) var(--tasty-radius) 0 0", alignSelf: "flex-end", height: "calc(100% - var(--tasty-space-xs))" }}>
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{t}</span>
            </div>
          ))}
          <div className="tb-mac__drag" style={{ flex: 1 }} />
        </div>
      </div>
    );
  }
  return (
    <div className="tb-mac" data-active={active}>
      {lights}
      <div className="tb-mac__drag" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", margin: "0 auto" }}>
        <img src={`${BASE}tasty-icon.png`} width="16" height="16" alt="" />
        <span style={{ fontSize: "var(--tasty-font-size-term-sm)", color: fg }}>{title}</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>— tasty</span>
      </div>
      <div style={{ width: 52, flex: "none" }} />
    </div>
  );
}

(function ensure() {
  if (typeof document !== "undefined" && !document.getElementById("tb-mac-css")) {
    const el = document.createElement("style"); el.id = "tb-mac-css"; el.textContent = TB_MAC_CSS; document.head.appendChild(el);
  }
})();

export { TitlebarMacOS };
