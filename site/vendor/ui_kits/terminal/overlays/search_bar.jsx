import React from "react";
import { Icon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Input } from "../../../components/forms/Input.jsx";

// Tasty UI kit — terminal search bar (Ctrl+F / ⌘F, top-right of the focused surface).
// Mirrors zilhak/tasty → src/adapters/ui/search_bar.rs
//   PopupDef: id "search_bar", 360×28, headless, sticky_focus, stays open on outside click.


// ── Search bar — mirrors src/adapters/ui/search_bar.rs (popup id "search_bar") ──
// Headless popup floating at the top-right of the FOCUSED terminal surface.
// One horizontal row, 4px gaps: input (flex — FILLS the bar, min 60px; DESIGN
// CANONICAL, see search_bar.rs note below) · status text
// (error > no-matches > {current}/{total}) · ▲▼ (only when matches exist) ·
// Aa / .* / ab option toggles · │ divider · ✕ close (Esc). Enter next, Shift+Enter prev, ↑↓ navigate,
// Esc closes. Does NOT close on outside click; input keeps focus.
// INPUT WIDTH (canonical): flex:1 — the input grows to fill the bar after the
// fixed-width counter (40px) and always-visible buttons are laid out. The Rust
// search_bar.rs:24-32 desired_width(200.0) is a fixed-width approximation to
// reconcile toward the flexible design, not a design constraint.
function SearchBar({ lines = [], onClose }) {
  const [q, setQ] = React.useState("");
  const [matchCase, setMatchCase] = React.useState(false);   // "Aa" — default off (case-insensitive)
  const [regex, setRegex] = React.useState(false);           // ".*"
  const [wholeWord, setWholeWord] = React.useState(false);   // "ab"
  const [cur, setCur] = React.useState(0);

  const { count, error } = React.useMemo(() => {
    if (!q) return { count: 0, error: false };
    let pattern;
    try {
      let src = regex ? q : q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      if (wholeWord) src = "\\b(?:" + src + ")\\b";
      pattern = new RegExp(src, matchCase ? "g" : "gi");
    } catch (e) { return { count: 0, error: true }; }
    let n = 0;
    for (const ln of lines) { const m = ln.match(pattern); if (m) n += m.length; }
    return { count: n, error: false };
  }, [q, matchCase, regex, wholeWord, lines]);

  React.useEffect(() => { setCur(0); }, [q, matchCase, regex, wholeWord]);

  const next = () => { if (count) setCur((c) => (c + 1) % count); };
  const prev = () => { if (count) setCur((c) => (c - 1 + count) % count); };
  const onKey = (e) => {
    if (e.key === "Enter") { e.preventDefault(); if (e.shiftKey) prev(); else next(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); prev(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); next(); }
    else if (e.key === "Escape") { e.stopPropagation(); onClose(); }
  };

  // status counter — always visible (fixed layout): "0/0" when empty/no
  // matches/invalid; red whenever a query has zero usable results.
  const hasQuery = q.length > 0;
  const counterText = count > 0 ? `${cur + 1}/${count}` : "0/0";
  const counterColor = hasQuery && (error || count === 0)
    ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-muted-raised)";

  const Toggle = ({ label, active, title, onClick }) => (
    <IconButton size="sm" active={active} aria-label={title} title={title} onClick={onClick}>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, fontWeight: 600, lineHeight: 1,
        color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted-raised)" }}>{label}</span>
    </IconButton>
  );

  return (
    <div role="search" style={{ position: "absolute", top: "var(--tasty-space-sm)", right: "var(--tasty-space-sm)", zIndex: 20,
      width: 360, maxWidth: "calc(100% - var(--tasty-space-md))",
      display: "flex", alignItems: "center", gap: 4, padding: 4,
      background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
      borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-popover)",
      fontFamily: "var(--tasty-font-ui)" }}>
      <span style={{ flex: 1, minWidth: 60, display: "flex" }}>
        <Input block ground="raised" invalid={hasQuery && error} autoFocus placeholder="Search..." value={q}
          onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} />
      </span>
      <span style={{ flex: "none", width: 40, textAlign: "center", fontSize: 12,
        color: counterColor, whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums" }}>{counterText}</span>
      <IconButton size="sm" aria-label="Previous match" title="Previous match (Shift+Enter)"
        disabled={count === 0} onClick={prev}>
        <Icon name="chevronUp" size={14} />
      </IconButton>
      <IconButton size="sm" aria-label="Next match" title="Next match (Enter)"
        disabled={count === 0} onClick={next}>
        <Icon name="chevronDown" size={14} />
      </IconButton>
      <Toggle label="Aa" active={matchCase} title="Match case" onClick={() => setMatchCase((v) => !v)} />
      <Toggle label=".*" active={regex} title="Regular expression" onClick={() => setRegex((v) => !v)} />
      <Toggle label="ab" active={wholeWord} title="Match whole word" onClick={() => setWholeWord((v) => !v)} />
      <span style={{ flex: "none", width: 1, alignSelf: "stretch", margin: "0 var(--tasty-size-2)",
        background: "var(--tasty-separator)" }}></span>
      <IconButton size="sm" aria-label="Close search" title="Close (Esc)" onClick={onClose}>
        <Icon name="close" size={14} />
      </IconButton>
    </div>
  );
}

export { SearchBar };
