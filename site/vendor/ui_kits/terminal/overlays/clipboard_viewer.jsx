import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Checkbox } from "../../../components/forms/Checkbox.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim, Spinner } from "./shared.jsx";

// Tasty UI kit — Clipboard Viewer popup (Tools › Clipboard Viewer / toggle shortcut).
// NEW visual design (no prior design existed — replaces the reverse-transcribed
// master-detail mock with a giant accent "Text" button). Mirrors (planned)
//   zilhak/tasty → crates/tasty-plugin-clipboard-viewer/src/view.rs
//   host paint  → src/plugin_bridge/ui_tree_render.rs
//   Popup: id "com.tasty.clipboard-viewer", read-only SNAPSHOT (not history),
//   single-instance guarded, close_on_outside_click. Size = 480×360 (canonical).
//
// Design principle: a single type (Text — the only real case today) is FIRST
// CLASS — header badge + metadata + one body column. Multiple types (Image /
// Files / HTML / Other) expand the same header into a segmented switch; no rail,
// ever. At ≥5 segments the switch compacts to icon-only (active keeps its label).
// HTML is shown as SOURCE only — never rendered — with a "Pretty print" checkbox
// (components/forms/Checkbox) sitting in the type bar's meta slot; the metadata
// then moves next to the mime in the footer. "Other" buckets every format that
// isn't text/image/files/html as name + textualized content rows.
// All widgets map to host primitives: label · tag · text_preview · button ·
// segmented(button group) · center · vbox/hbox/scroll · spacer.



// glyph names from the canonical set (icons/*.svg via <Icon name>)
const CB = {
  clip:  "clipboard",
  text:  "textLeft",
  image: "image",
  files: "file",
  html:  "html",
  other: "layers",
  warn:  "alertTriangle",
  x:     "close",
  lock:  "lock",
};
// "other" = the catch-all bucket → `layers` (many stacked formats). helpCircle is
// reserved for help affordances; paneEmpty reads as "nothing here", which is wrong
// for a bucket that HAS content.
const TYPE_ICON = { text: CB.text, image: CB.image, files: CB.files, html: CB.html, other: CB.other };
// ≥5 segments can't hold icon+label inside 480px once labels grow 20–40% in
// de/ko/ja, so the switch compacts: inactive segments go icon-only (tooltip keeps
// the name), the active one keeps its label. No horizontal scroll, no truncation.
const SEG_COMPACT_AT = 5;

// prettify = indentation/line breaks only — never a browser render (read-only popup)
const VOID_EL = new Set(["area","base","br","col","embed","hr","img","input","link","meta","source","track","wbr"]);
function prettyHtml(src) {
  const parts = String(src).replace(/>\s+</g, "><").split(/(<[^>]+>)/).filter((s) => s && s.trim());
  let depth = 0;
  return parts.map((p) => {
    if (/^<\//.test(p)) depth = Math.max(0, depth - 1);
    const line = "  ".repeat(depth) + p.trim();
    if (/^<[^/!]/.test(p) && !/\/>$/.test(p) && !VOID_EL.has((p.match(/^<([a-z0-9-]+)/i) || [])[1]?.toLowerCase())) depth++;
    return line;
  }).join("\n");
}

const cbWell = {
  flex: 1, minHeight: 0, overflow: "auto", margin: "var(--tasty-space-sm) var(--tasty-size-14) var(--tasty-size-14)",
  borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid var(--tasty-separator)",
  background: "var(--tasty-bg-app)",
};
const cbMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12, lineHeight: 1.7, color: "var(--tasty-text-primary)" };
const cbMetaMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" };

// ── centered state (empty / read-failed / already-open) ──────────────────
function CenterState({ glyph, title, sub, tone }) {
  const color = tone === "danger" ? "var(--tasty-accent-danger)" : "var(--tasty-text-muted)";
  return (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center",
      justifyContent: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xl)", textAlign: "center" }}>
      <span style={{ display: "inline-flex", color, opacity: tone === "danger" ? 0.9 : 0.5 }}><Icon d={glyph} size={28} /></span>
      <span style={{ fontSize: 13, fontWeight: 600, color: tone === "danger" ? "var(--tasty-accent-danger)" : "var(--tasty-text-secondary)" }}>{title}</span>
      {sub && <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", maxWidth: 280, lineHeight: 1.5 }}>{sub}</span>}
    </div>
  );
}

// ── type switch — single type = badge; ≥2 = segmented control ────────────
function TypeSwitch({ types, active, onPick }) {
  const compact = types.length >= SEG_COMPACT_AT;
  if (types.length <= 1) {
    const t = types[0];
    return (
      <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name={TYPE_ICON[t.id]} size={14} /></span>
        <Tag variant="accent">{t.label}</Tag>
      </span>
    );
  }
  return (
    <div style={{ display: "inline-flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)",
      borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      {types.map((t, i) => {
        const on = t.id === active;
        const label = !compact || on;
        return (
          <button key={t.id} onClick={() => onPick(t.id)} title={t.label} aria-label={t.label} style={{ appearance: "none", cursor: "pointer",
            display: "inline-flex", alignItems: "center", justifyContent: "center", gap: label ? "var(--tasty-space-xs)" : 0,
            height: 26, padding: label ? "0 10px" : "0 8px", minWidth: label ? 0 : 30,
            border: 0, borderLeft: i ? "var(--tasty-border-width) solid var(--tasty-border-default)" : 0,
            fontFamily: "var(--tasty-font-ui)", fontSize: 12, fontWeight: on ? 600 : 400,
            color: on ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)",
            background: on ? "var(--tasty-accent-primary)" : "transparent" }}>
            <Icon name={TYPE_ICON[t.id]} size={13} />{label ? t.label : null}
          </button>
        );
      })}
    </div>
  );
}

// ── per-type body ────────────────────────────────────────────────────────
function TypeBody({ t, pretty }) {
  if (t.id === "html") {
    // read-only source view — identical well/mono treatment as `text`; the toggle
    // only re-indents the same source (0ms, no transition).
    return (
      <div className="tasty-scroll" style={cbWell}>
        <pre style={{ ...cbMono, margin: 0, padding: "var(--tasty-space-md) var(--tasty-size-14)",
          whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{pretty ? prettyHtml(t.content) : t.content}</pre>
      </div>
    );
  }
  if (t.id === "other") {
    const formats = t.formats || [];
    return (
      <div className="tasty-scroll" style={cbWell}>
        <div style={{ display: "flex", flexDirection: "column" }}>
          {formats.map((f, i) => (
            <div key={i} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)",
              padding: "var(--tasty-space-md) var(--tasty-size-14)",
              borderBottom: i < formats.length - 1 ? "var(--tasty-border-width) solid var(--tasty-separator)" : "none" }}>
              <div style={{ display: "flex", alignItems: "baseline", gap: "var(--tasty-space-sm)" }}>
                <span style={{ ...cbMetaMono, color: "var(--tasty-text-secondary)", fontWeight: 600, letterSpacing: "0.02em" }}>{f.name}</span>
                {f.size && <span style={cbMetaMono}>{f.size}</span>}
              </div>
              <pre style={{ ...cbMono, margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-all" }}>{f.content}</pre>
              {f.moreLines ? <span style={{ ...cbMetaMono, fontStyle: "italic" }}>+{f.moreLines} more lines</span> : null}
            </div>
          ))}
        </div>
      </div>
    );
  }
  if (t.id === "files") {
    return (
      <div className="tasty-scroll" style={cbWell}>
        <div style={{ display: "flex", flexDirection: "column" }}>
          {t.files.map((f, i) => (
            <div key={i} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-md)",
              borderBottom: i < t.files.length - 1 ? "var(--tasty-border-width) solid var(--tasty-separator)" : "none" }}>
              <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)", flex: "none" }}><Icon name={CB.files} size={14} /></span>
              <span style={{ ...cbMono, fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f}</span>
            </div>
          ))}
        </div>
      </div>
    );
  }
  if (t.id === "image") {
    return (
      <div style={{ ...cbWell, display: "flex", alignItems: "center", justifyContent: "center" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-space-sm)", color: "var(--tasty-text-muted)" }}>
          <Icon name={CB.image} size={30} />
          <span style={{ ...cbMetaMono }}>{t.meta}</span>
          <span style={{ fontSize: 11, color: "var(--tasty-text-disabled)", fontStyle: "italic" }}>No inline image preview</span>
        </div>
      </div>
    );
  }
  // text
  return (
    <div className="tasty-scroll" style={cbWell}>
      <pre style={{ ...cbMono, margin: 0, padding: "var(--tasty-space-md) var(--tasty-size-14)",
        whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{t.content}</pre>
    </div>
  );
}

const cbFrame = { width: 480, height: 360, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
  border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)",
  overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" };

function ClipboardViewer({ onClose, snapshot }) {
  const snap = snapshot || { status: "ok", types: [] };
  const types = snap.types || [];
  const [active, setActive] = React.useState(types[0] ? types[0].id : null);
  // kept for the popup's lifetime — switching type away and back preserves it
  const [pretty, setPretty] = React.useState(false);
  React.useEffect(() => { setActive(types[0] ? types[0].id : null); }, [snap]);
  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") onClose && onClose(); };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  const cur = types.find((t) => t.id === active) || types[0] || null;
  const dataState = snap.status === "ok" && types.length > 0;

  return (
    <Scrim onClose={onClose}>
      <div style={cbFrame}>
        {/* header */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)", flex: "none",
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name={CB.clip} size={16} /></span>
          <span style={{ fontSize: 14, fontWeight: 600 }}>Clipboard</span>
          <Tag>snapshot</Tag>
          <div style={{ flex: 1 }} />
          <IconButton size="sm" aria-label="Close" title="Close (Esc)" onClick={onClose}><Icon name={CB.x} size={16} /></IconButton>
        </div>

        {dataState ? (
          <>
            {/* type bar — switch (or single badge) + live metadata */}
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-sm) var(--tasty-size-14)", flex: "none",
              borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
              <TypeSwitch types={types} active={active} onPick={setActive} />
              <div style={{ flex: 1 }} />
              {cur && cur.id === "html"
                ? <Checkbox label="Pretty print" checked={pretty} onChange={(e) => setPretty(e.target.checked)} />
                : <span style={cbMetaMono}>{cur && cur.meta}</span>}
            </div>
            {/* body */}
            {cur && <TypeBody t={cur} pretty={pretty} />}
            {/* footer */}
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-sm) var(--tasty-size-14)", flex: "none",
              borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <span style={cbMetaMono}>{cur && (cur.id === "html" ? `${cur.mime} · ${cur.meta}` : cur.mime)}</span>
              <div style={{ flex: 1 }} />
              <Button variant="secondary" size="sm" onClick={onClose}>Close</Button>
            </div>
          </>
        ) : snap.status === "failed" ? (
          <CenterState glyph={CB.warn} tone="danger" title="Couldn't read the clipboard"
            sub="The system clipboard handle could not be opened. Close another app that may be holding it and reopen." />
        ) : snap.status === "busy" ? (
          <CenterState glyph={CB.lock} title="Clipboard viewer is already open"
            sub="Only one snapshot window runs at a time — the existing one was brought to the front." />
        ) : (
          <CenterState glyph={CB.clip} title="Clipboard is empty"
            sub="Copy some text, an image, or files and reopen to see a snapshot here." />
        )}
      </div>
    </Scrim>
  );
}

export { ClipboardViewer };
