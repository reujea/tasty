import React from "react";
const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty UI kit — CSD titlebar · Windows
// ─────────────────────────────────────────────────────────────────────────
// ⚠ NON-PRODUCT / EXPLORATION. Shipping Tasty uses the NATIVE Windows titlebar.
//   This designs the Client-Side-Decoration alternative. Adopting it needs
//   WindowAttributes::with_decorations(false) + custom drag + WM_NCHITTEST
//   handling (HTMAXBUTTON on the maximize rect for Snap Layouts).
//
// Windows conventions: caption buttons (minimize · maximize/restore · close) on
// the RIGHT, each ~46px wide and full titlebar height; close-hover paints the
// system red (#c42b1c via --tasty-accent-window-close) with a white glyph.
// The maximize button must expose its hit-rect to the OS so the Snap-Layouts
// flyout appears on hover. All color/size from --tasty-* tokens.

const TB_WIN_CSS = `
.tb-win { display:flex; align-items:stretch; height:var(--tasty-titlebar-height); flex:none;
  background:var(--tasty-titlebar-bg); border-bottom:var(--tasty-border-width) solid var(--tasty-titlebar-border);
  -webkit-user-select:none; user-select:none; }
.tb-win[data-active="false"] { background:var(--tasty-titlebar-bg-inactive); }
.tb-win__title { display:flex; align-items:center; gap:var(--tasty-space-sm); padding:0 var(--tasty-space-md); -webkit-app-region:drag; }
.tb-win__caption { display:flex; align-items:stretch; flex:none; }
.tb-win__btn { width:var(--tasty-titlebar-caption-width); display:grid; place-items:center;
  border:0; background:transparent; color:var(--tasty-titlebar-button-fg); cursor:pointer;
  transition:background var(--tasty-motion-ui-fast) var(--tasty-ease-ui), color var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tb-win__btn svg { width:var(--tasty-icon-size-xs); height:var(--tasty-icon-size-xs); }
.tb-win__btn:hover { background:var(--tasty-titlebar-button-hover-bg); color:var(--tasty-titlebar-button-fg-hover); }
.tb-win__btn:active { background:var(--tasty-titlebar-button-active-bg); }
.tb-win__btn--close:hover { background:var(--tasty-titlebar-close-hover-bg); color:var(--tasty-titlebar-close-hover-fg); }
.tb-win[data-active="false"] .tb-win__btn { color:var(--tasty-titlebar-fg-inactive); }
`;

function TitlebarWindows({ active = true, variant = "split", title = "agents-prod", maximized = false, tabs = ["build · cargo", "README.md", "scratch"] }) {
  const G = {
    min:     <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M5 12h14"/></svg>,
    max:     <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="5" y="5" width="14" height="14" rx="1"/></svg>,
    restore: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="7" y="7" width="11" height="11" rx="1"/><path d="M7 5h10a2 2 0 0 1 2 2v10"/></svg>,
    close:   <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M6 6l12 12M18 6 6 18"/></svg>,
  };
  const caption = (
    <div className="tb-win__caption">
      <button className="tb-win__btn" title="Minimize">{G.min}</button>
      {/* Snap Layouts: this rect is reported to the OS via WM_NCHITTEST → HTMAXBUTTON */}
      <button className="tb-win__btn" data-snap-target="true" title="Maximize">{maximized ? G.restore : G.max}</button>
      <button className="tb-win__btn tb-win__btn--close" title="Close">{G.close}</button>
    </div>
  );
  const fg = active ? "var(--tasty-titlebar-fg)" : "var(--tasty-titlebar-fg-inactive)";

  if (variant === "tabs") {
    return (
      <div className="tb-win" data-active={active}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "0 var(--tasty-space-sm) 0 var(--tasty-space-md)", flex: "none", WebkitAppRegion: "drag" }}>
          <img src={`${BASE}tasty-icon.png`} width="16" height="16" alt="" />
        </div>
        <div style={{ display: "flex", gap: "var(--tasty-space-xs)", alignItems: "flex-end", flex: 1, minWidth: 0 }}>
          {tabs.map((t, i) => (
            <div key={t} style={{ display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-term-sm)", maxWidth: "var(--tasty-tab-width)",
              color: i === 0 ? "var(--tasty-titlebar-fg)" : "var(--tasty-text-muted)",
              background: i === 0 ? "var(--tasty-bg-panel)" : "transparent",
              borderRadius: "var(--tasty-radius) var(--tasty-radius) 0 0", height: "calc(100% - var(--tasty-space-xs))" }}>
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{t}</span>
            </div>
          ))}
          <div style={{ flex: 1, WebkitAppRegion: "drag" }} />
        </div>
        {caption}
      </div>
    );
  }
  return (
    <div className="tb-win" data-active={active}>
      <div className="tb-win__title" style={{ flex: 1, minWidth: 0 }}>
        <img src={`${BASE}tasty-icon.png`} width="16" height="16" alt="" />
        <span style={{ fontSize: "var(--tasty-font-size-term-sm)", color: fg }}>{title}</span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>— tasty</span>
      </div>
      {caption}
    </div>
  );
}

(function ensure() {
  if (typeof document !== "undefined" && !document.getElementById("tb-win-css")) {
    const el = document.createElement("style"); el.id = "tb-win-css"; el.textContent = TB_WIN_CSS; document.head.appendChild(el);
  }
})();

export { TitlebarWindows };
