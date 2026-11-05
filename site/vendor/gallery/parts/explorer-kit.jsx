import React from "react";
import { IconButton, AutoComplete } from "../../components/index.js";
import { GIcon } from "./specimen.jsx";

const ic = {
  folder: <GIcon d={<path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-7l-2-2H4a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1z" />} />,
  file: <GIcon d={<><path d="M14 3v4a1 1 0 0 0 1 1h4" /><path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z" /></>} />,
  x: <GIcon d={<path d="M18 6 6 18M6 6l12 12" />} size={12} />,
  plus: <GIcon d={<path d="M12 5v14M5 12h14" />} />,
  back: <GIcon d={<path d="m15 18-6-6 6-6" />} />,
  fwd: <GIcon d={<path d="m9 18 6-6-6-6" />} />,
  up: <GIcon d={<path d="M12 19V5M5 12l7-7 7 7" />} />,
  refresh: <GIcon d={<path d="M21 12a9 9 0 1 1-2.6-6.4M21 3v6h-6" />} />,
  grid: <GIcon d={<><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>} />,
  list: <GIcon d={<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />} />,
  detail: <GIcon d={<path d="M3 5h18M3 12h18M3 19h18M3 5v14" />} />,
  star: <GIcon d={<path d="m12 3 2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 18l-5.8 3 1.1-6.5L2.6 9.8l6.5-.9z" />} />,
  starFill: <GIcon fill="currentColor" d={<path d="m12 3 2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 18l-5.8 3 1.1-6.5L2.6 9.8l6.5-.9z" />} />,
  image: <GIcon d={<><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-5-5L5 21" /></>} />,
  chevD: <GIcon d={<path d="m6 9 6 6 6-6" />} />,
  chevRsm: <GIcon d={<path d="m9 18 6-6-6-6" />} />,
  copy: <GIcon d={<><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V5a2 2 0 0 1 2-2h8" /></>} />,
  scissors: <GIcon d={<><circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M20 4 8.1 15.9M14.5 12.5 20 20M8.1 8.1 12 12" /></>} />,
  paste: <GIcon d={<><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" /><rect x="8" y="2" width="8" height="4" rx="1" /></>} />,
  trash: <GIcon d={<path d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />} />,
  rename: <GIcon d={<path d="M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z" />} />,
  external: <GIcon d={<path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />} />,
  link: <GIcon d={<path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1.5 1.5M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1.5-1.5" />} />,
  lock: <GIcon d={<><rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></>} />,
  folderOpen: <GIcon d={<path d="M3 8a1 1 0 0 1 1-1h5l2 2h7a1 1 0 0 1 1 1v1H3z M3 11h18l-1.5 8a1 1 0 0 1-1 1H5.5a1 1 0 0 1-1-1z" />} />,
  globe: <GIcon d={<><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a15 15 0 0 1 0 18M12 3a15 15 0 0 0 0 18" /></>} />,
  alert: <GIcon d={<><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16h.01" /></>} />,
};

// ════════════════════════════════════════════════════════════
//  SHARED — editable path field (Explorer + Markdown address bars)
// ════════════════════════════════════════════════════════════
// One component both surfaces share. Display: mono path (ellipsis, secondary)
// behind a per-surface leading icon, trailing Go (arrow-right). Click → edit
// (focus ring + caret, text-primary); ↵/Go navigate, Esc reverts. Field tokens
// align to forms/Input (--tasty-input-*). The ONLY per-surface parameter is the
// leading `icon` (Explorer=folderOpen, Markdown=file); the outer bar is not part
// of this component. Real-side home: crates/tasty-ui-widgets (Input + IconPainter).
const goGlyph = <GIcon d={<path d="M5 12h14M13 6l6 6-6 6" />} />;
function PathField({ icon, path, editing = false, candidates = null, open = false, activeIndex = 0, query, maxDropdownHeight = 220 }) {
  // Editing WITH candidates → the shared AutoComplete dropdown (typeahead of
  // recent paths). Same trigger tokens as the idle field below; Explorer feeds
  // recent directories, Markdown recent files. The component emits the path.
  if (editing && candidates) {
    return (
      <AutoComplete block mono withGo icon={icon} rowIcon={icon}
        open={open} activeIndex={activeIndex} query={query != null ? query : path}
        items={candidates} maxDropdownHeight={maxDropdownHeight} emptyLabel="No matching path" />
    );
  }
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 6 }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 6, height: 28, padding: "0 8px",
        background: "var(--tasty-input-bg)", borderRadius: "var(--tasty-radius)",
        border: "1px solid " + (editing ? "var(--tasty-input-border-focus)" : "var(--tasty-input-border)"),
        boxShadow: editing ? "0 0 0 2px color-mix(in srgb, var(--tasty-border-focus) 35%, transparent)" : "none" }}>
        <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-input-icon-fg)" }}>{icon}</span>
        <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: 12,
          color: editing ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {path}
          {editing && <span style={{ display: "inline-block", width: 1, height: 13, background: "var(--tasty-accent-primary)", marginLeft: 1, verticalAlign: "text-bottom", animation: "tasty-blink 1.1s step-end infinite" }} />}
        </span>
      </div>
      <IconButton size="sm" aria-label="Go">{goGlyph}</IconButton>
    </div>
  );
}

// Explorer view toggle — a SEGMENT (picks a value: how the same listing is drawn), so it
// takes the R1 segment recipe: accent fill + on-accent ink. Never surface-active (row-selection only).
function SegToggle({ value = "detail", hover = null }) {
  const items = [["grid", ic.grid], ["list", ic.list], ["detail", ic.detail]];
  return (
    <div style={{ display: "flex", alignItems: "center", flex: "none", height: 28, padding: "var(--tasty-space-xs)", gap: "var(--tasty-space-xs)", boxSizing: "border-box",
      background: "var(--tasty-surface-raised)", border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
      {items.map(([k, glyph]) => {
        const active = k === value;
        const hov = k === hover && !active;
        return (
          <span key={k} aria-label={k} title={k} style={{ width: "var(--tasty-size-24)", height: "var(--tasty-size-20)", display: "inline-flex", alignItems: "center", justifyContent: "center",
            borderRadius: "var(--tasty-radius-sm)", color: active ? "var(--tasty-segtoggle-on-fg)" : "var(--tasty-text-muted)",
            background: active ? "var(--tasty-segtoggle-on-bg)" : hov ? "var(--tasty-overlay-hover)" : "transparent" }}>{glyph}</span>
        );
      })}
    </div>
  );
}

function SideHead({ children, trailing }) {
  return (
    <div style={{ display: "flex", alignItems: "center", padding: "10px 10px 4px" }}>
      <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: ".07em", color: "var(--tasty-text-muted)" }}>{children}</span>
      {trailing}
    </div>
  );
}

function TreeNode({ label, depth = 0, open, leaf, active }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 4, height: 22, paddingRight: 8, paddingLeft: 6 + depth * 12,
      borderRadius: "var(--tasty-radius-sm)", fontSize: 13, cursor: "default",
      color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)",
      background: active ? "var(--tasty-surface-active)" : "transparent" }}>
      <span style={{ width: 12, display: "inline-flex", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", flex: "none" }}>{leaf ? null : (open ? ic.chevD : ic.chevRsm)}</span>
      <span style={{ display: "inline-flex", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", flex: "none" }}>{ic.folder}</span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
    </div>
  );
}

function FavRow({ name, active }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, height: 22, flex: "none", padding: "0 6px", borderRadius: "var(--tasty-radius-sm)",
      fontSize: 13, color: active ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)", background: active ? "var(--tasty-surface-active)" : "transparent" }}>
      <span style={{ display: "inline-flex", color: active ? "var(--tasty-text-on-active)" : "var(--tasty-accent-warning-raised)", flex: "none" }}>{ic.starFill}</span>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
    </div>
  );
}

function DetailRow({ glyph, name, size, date, type, state, glyphColor }) {
  const bg = state === "selected" ? "var(--tasty-surface-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent";
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 80px 132px 92px", alignItems: "center", height: "var(--tasty-table-cell-height)", padding: "0 10px",
      fontSize: 13, background: bg, cursor: "default",
      color: state === "selected" ? "var(--tasty-text-on-active)" : state === "cut" ? "var(--tasty-cut-pending-fg)" : "var(--tasty-text-secondary)" }}>
      <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
        <span style={{ display: "inline-flex", flex: "none", color: state === "selected" ? "var(--tasty-text-on-active)" : glyphColor || "var(--tasty-text-muted)" }}>{glyph}</span>
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontStyle: state === "cut" ? "var(--tasty-cut-pending-font-style)" : undefined }}>{name}</span>
      </span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: state === "selected" ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)", textAlign: "right", paddingRight: 8 }}>{size}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: state === "selected" ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)" }}>{date}</span>
      <span style={{ fontSize: 12, color: state === "selected" ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)" }}>{type}</span>
    </div>
  );
}

function DetailHeader() {
  const cols = [["Name", true], ["Size", false], ["Date modified", false], ["Type", false]];
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 80px 132px 92px", alignItems: "center", height: "var(--tasty-table-cell-height)", padding: "0 10px",
      background: "var(--tasty-table-header-bg)", borderBottom: "1px solid var(--tasty-separator)",
      fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-table-header-font-size)", fontWeight: "var(--tasty-table-header-font-weight)", textTransform: "uppercase", letterSpacing: "var(--tasty-table-header-tracking)", color: "var(--tasty-table-header-fg)" }}>
      {cols.map(([c, sorted], i) => (
        <span key={c} style={{ display: "flex", alignItems: "center", gap: 3, justifyContent: i === 1 ? "flex-end" : "flex-start",
          paddingRight: i === 1 ? 8 : 0, color: sorted ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)" }}>
          {c}{sorted && <span style={{ fontSize: 9 }}>▲</span>}
        </span>
      ))}
    </div>
  );
}

// GridCell — icon-view file cell. Design SoT for the Rust grid_cell().
//   icon   : icon_glyph_size_md (16), down from item_height_interactive (28)
//   label  : font_size_caption (11), 3-line wrap-to-width + '…' overflow (LayoutJob
//            wrap.max_rows=3 · overflow_character='…' on the real side)
//   width  : CELL_W = 80 (unchanged)
//   height : FIXED 3-line — the label reserves 3 lines always, so short names keep
//            grid rows uniform (stable horizontal_wrapped alignment). Label is
//            top-aligned within the reserved block.
//   gaps   : spacing_sm (8) top/bottom · spacing_xs (4) icon→label
const GRID_CELL_W = 80;
const GRID_LABEL_LINES = 3;
const GRID_LABEL_LINE_H = 14; // round(font_size_caption 11 × 1.3)
function GridCell({ glyph, name, state, glyphColor }) {
  const bg = state === "selected" ? "var(--tasty-surface-active)" : state === "hover" ? "var(--tasty-overlay-hover)" : "transparent";
  return (
    <div style={{ width: GRID_CELL_W, display: "flex", flexDirection: "column", alignItems: "center", gap: 4, padding: "8px 4px",
      borderRadius: "var(--tasty-radius)", background: bg, cursor: "default" }}>
      <span style={{ display: "inline-flex", color: state === "selected" ? "var(--tasty-text-on-active)" : glyphColor || "var(--tasty-text-muted)", height: 16, alignItems: "center" }}>{glyph}</span>
      <span style={{ fontSize: 11, textAlign: "center", lineHeight: `${GRID_LABEL_LINE_H}px`, height: GRID_LABEL_LINES * GRID_LABEL_LINE_H,
        color: state === "selected" ? "var(--tasty-text-on-active)" : state === "cut" ? "var(--tasty-cut-pending-fg)" : "var(--tasty-text-secondary)", fontStyle: state === "cut" ? "var(--tasty-cut-pending-font-style)" : undefined, wordBreak: "break-word",
        display: "-webkit-box", WebkitLineClamp: GRID_LABEL_LINES, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{name}</span>
    </div>
  );
}

function CtxMenu({ title, children }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{title}</div>
      <div style={{ width: 220, background: "var(--tasty-menu-bg)", border: "1px solid var(--tasty-menu-border)",
        borderRadius: "var(--tasty-menu-radius)", padding: 6, boxShadow: "var(--tasty-shadow-popover)" }}>
        {children}
      </div>
    </div>
  );
}

// 2026-10-07 batch 4 — compact: content body under --tasty-explorer-state-compact-below (120) → one row:
// glyph (unscaled 16) · title (ellipsis; reason / sub move to its tooltip) · actions. Retry / Go up stay.
function ExpState({ glyph, glyphColor, title, sub, reason, actions, compact, bodyHeight, bg }) {
  // b11 (image-too-large-title-tone): refused-by-limit states colour only the glyph; the title is text-primary.
  const titleFg = glyphColor === "var(--tasty-image-too-large-fg)" ? "var(--tasty-text-primary)" : glyphColor === "var(--tasty-accent-warning-raised)" || glyphColor === "var(--tasty-explorer-error-fg)" || glyphColor === "var(--tasty-image-error-fg)" ? glyphColor : "var(--tasty-text-secondary)";
  if (compact) return (
    <div style={{ flex: 1, minWidth: 0, height: bodyHeight || 62, display: "flex", alignItems: "center", justifyContent: "center", boxSizing: "border-box",
      background: bg || "var(--tasty-bg-panel)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", padding: "0 var(--tasty-space-sm)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0, maxWidth: "100%" }}>
        <span style={{ display: "inline-flex", flex: "none", color: glyphColor || "var(--tasty-text-muted)" }}>{glyph}</span>
        <span title={reason || sub} style={{ minWidth: 0, fontSize: 13, color: titleFg, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{title}</span>
        {actions && <div style={{ display: "flex", flex: "none", gap: "var(--tasty-space-xs)" }}>{actions}</div>}
      </div>
    </div>
  );
  return (
    <div style={{ flex: 1, minWidth: 0, height: 180, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 8,
      background: bg || "var(--tasty-bg-panel)", border: "1px solid var(--tasty-separator)", borderRadius: "var(--tasty-radius)", textAlign: "center", padding: 16 }}>
      <span style={{ display: "inline-flex", color: glyphColor || "var(--tasty-text-muted)", transform: "scale(1.6)" }}>{glyph}</span>
      <div style={{ fontSize: 13, color: titleFg, textWrap: "balance" }}>{title}</div>
      {sub && <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", maxWidth: 200, textWrap: "balance" }}>{sub}</div>}
      {reason && <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", maxWidth: 200, overflowWrap: "anywhere" }}>{reason}</div>}
      {actions && <div style={{ display: "flex", gap: "var(--tasty-space-sm)", marginTop: "var(--tasty-space-xs)" }}>{actions}</div>}
    </div>
  );
}

export { DetailHeader, ic, DetailRow, SegToggle, CtxMenu, PathField, GridCell, SideHead, TreeNode, FavRow, ExpState };
