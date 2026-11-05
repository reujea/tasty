import React from "react";
const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty UI kit — CSD titlebar · Linux
// ─────────────────────────────────────────────────────────────────────────
// ⚠ NON-PRODUCT / EXPLORATION. Shipping Tasty uses the NATIVE Linux titlebar.
//   This designs the Client-Side-Decoration alternative (the common case on
//   Wayland, where the client draws its own decorations + resize edges).
//   Adopting it needs WindowAttributes::with_decorations(false), custom drag,
//   and Wayland CSD resize-edge handling.
//
// Linux has NO single convention — the button set, order, and side vary by
// desktop environment and user setting (GNOME: Close only, right; KDE/Breeze:
// min·max·close, movable to either side). So the layout is DATA-DRIVEN: pass
// `buttons` (subset/order of min·max·close) and `side` (left|right). On Wayland
// the window draws rounded top corners + a 1px border + a soft shadow whose
// margin holds the resize edges. All color/size from --tasty-* tokens.

const TB_LIN_CSS = `
.tb-lin { display:flex; align-items:center; height:var(--tasty-titlebar-height); flex:none;
  background:var(--tasty-titlebar-bg); border-bottom:var(--tasty-border-width) solid var(--tasty-titlebar-border);
  padding:0 var(--tasty-space-sm); gap:var(--tasty-space-sm); -webkit-user-select:none; user-select:none;
  border-top-left-radius:var(--tasty-titlebar-csd-radius); border-top-right-radius:var(--tasty-titlebar-csd-radius); }
.tb-lin[data-active="false"] { background:var(--tasty-titlebar-bg-inactive); }
.tb-lin__drag { -webkit-app-region:drag; }
.tb-lin__ctrls { display:flex; gap:var(--tasty-space-xs); flex:none; }
.tb-lin__btn { width:var(--tasty-titlebar-window-button-size); height:var(--tasty-titlebar-window-button-size);
  border:0; border-radius:var(--tasty-radius-pill); display:grid; place-items:center;
  background:var(--tasty-titlebar-button-hover-bg); color:var(--tasty-titlebar-button-fg); cursor:pointer;
  transition:background var(--tasty-motion-ui-fast) var(--tasty-ease-ui), color var(--tasty-motion-ui-fast) var(--tasty-ease-ui); }
.tb-lin__btn svg { width:var(--tasty-icon-size-xs); height:var(--tasty-icon-size-xs); }
.tb-lin__btn:hover { color:var(--tasty-titlebar-button-fg-hover); background:var(--tasty-surface-hover); }
.tb-lin__btn--close:hover { background:var(--tasty-titlebar-close-hover-bg); color:var(--tasty-titlebar-close-hover-fg); }
.tb-lin[data-active="false"] .tb-lin__btn { color:var(--tasty-titlebar-fg-inactive); }
`;

function TitlebarLinux({ active = true, variant = "split", title = "agents-prod",
                         buttons = ["min", "max", "close"], side = "right",
                         tabs = ["build · cargo", "README.md", "scratch"] }) {
  const G = {
    min:   <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M6 12h12"/></svg>,
    max:   <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="6" y="6" width="12" height="12" rx="1"/></svg>,
    close: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M7 7l10 10M17 7 7 17"/></svg>,
  };
  const ctrls = (
    <div className="tb-lin__ctrls">
      {buttons.map((b) => (
        <button key={b} className={"tb-lin__btn" + (b === "close" ? " tb-lin__btn--close" : "")}
          title={b[0].toUpperCase() + b.slice(1)}>{G[b]}</button>
      ))}
    </div>
  );
  const fg = active ? "var(--tasty-titlebar-fg)" : "var(--tasty-titlebar-fg-inactive)";
  const left = side === "left";

  const center = variant === "tabs" ? (
    <div style={{ display: "flex", gap: "var(--tasty-space-xs)", alignItems: "flex-end", flex: 1, minWidth: 0, height: "100%" }}>
      {tabs.map((t, i) => (
        <div key={t} style={{ display: "flex", alignItems: "center", padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-term-sm)", maxWidth: "var(--tasty-tab-width)",
          color: i === 0 ? "var(--tasty-titlebar-fg)" : "var(--tasty-text-muted)",
          background: i === 0 ? "var(--tasty-bg-panel)" : "transparent",
          borderRadius: "var(--tasty-radius) var(--tasty-radius) 0 0", height: "calc(100% - var(--tasty-space-xs))" }}>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{t}</span>
        </div>
      ))}
      <div className="tb-lin__drag" style={{ flex: 1 }} />
    </div>
  ) : (
    <div className="tb-lin__drag" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: 1, minWidth: 0,
      justifyContent: "center" }}>
      <img src={`${BASE}tasty-icon.png`} width="16" height="16" alt="" />
      <span style={{ fontSize: "var(--tasty-font-size-term-sm)", color: fg }}>{title}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>— tasty</span>
    </div>
  );

  return (
    <div className="tb-lin" data-active={active}>
      {left && ctrls}
      {center}
      {!left && ctrls}
    </div>
  );
}

(function ensure() {
  if (typeof document !== "undefined" && !document.getElementById("tb-lin-css")) {
    const el = document.createElement("style"); el.id = "tb-lin-css"; el.textContent = TB_LIN_CSS; document.head.appendChild(el);
  }
})();

export { TitlebarLinux };
