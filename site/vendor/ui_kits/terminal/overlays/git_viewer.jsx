import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim, Spinner } from "./shared.jsx";

// Tasty UI kit — Git Viewer popup (Tools › Git, contributed by com.tasty.git-viewer).
// NEW visual design (no prior design existed — replaces the raw splitter+label mock
// reverse-transcribed from plugin structure). Brought up to the same visual language
// as remote_tool / port_scanner. Mirrors (planned)
//   zilhak/tasty → crates/tasty-plugin-git-viewer/src/view.rs
//   host paint  → src/plugin_bridge/ui_tree_render.rs
//   Popup: id "com.tasty.git-viewer/viewer", read-only, single-instance, 960×640.
//
// Layout: header + context strip; body = worktree rail (232px) | right column.
// Right column splits 50/50: Changes (status) over Commits (log) ↔ Diff.
// READ-ONLY: the only interactions are Refresh, select worktree (re-binds panes),
// select a changed file (→ diff), and the diff Back button. No staging/commit/checkout.
// All widgets map to host primitives: splitter · button · label(body/caption/heading/
// mono) · selectable_row · tag · scroll · vbox/hbox · spacer · center.



// glyph names from the canonical set (icons/*.svg via <Icon name>)
const GV = {
  branch: "gitBranch",
  refresh: "refresh",
  back: "chevronLeft",
  x: "close",
  tree: "gitTree",
  warn: "alertTriangle",
};

// ── token-colored pill — mirrors the Tag visual exactly, with an `info` tone the
//    Tag variants don't ship (main / oid / refs / hunk all read sky = accent-info) ──
const GV_TONE = {
  info: "var(--tasty-accent-info)", success: "var(--tasty-accent-success)",
  warning: "var(--tasty-accent-warning)", danger: "var(--tasty-accent-danger)",
  primary: "var(--tasty-accent-primary)",
};
const GV_TONE_FG = {
  info: "var(--tasty-tag-info-fg)", success: "var(--tasty-tag-success-fg)",
  warning: "var(--tasty-tag-warning-fg)", danger: "var(--tasty-tag-danger-fg)",
  primary: "var(--tasty-accent-primary-raised)",
};
function GBadge({ tone, dot, width, title, children }) {
  const c = GV_TONE[tone];
  const base = { display: "inline-flex", alignItems: "center", justifyContent: width ? "center" : "flex-start",
    gap: 4, height: 16, padding: width ? 0 : "0 var(--tasty-space-sm)", width, flex: "none",
    borderRadius: "var(--tasty-radius-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)",
    fontWeight: 500, lineHeight: 1, whiteSpace: "nowrap", borderStyle: "solid", borderWidth: "var(--tasty-border-width)" };
  if (!c) return <span title={title} style={{ ...base, color: "var(--tasty-tag-fg)",
    borderColor: "var(--tasty-border-default)", background: "var(--tasty-tag-bg)" }}>{children}</span>;
  return (
    <span title={title} style={{ ...base, color: GV_TONE_FG[tone], borderColor: `color-mix(in srgb, ${c} 40%, transparent)`,
      background: "var(--tasty-tag-bg)" }}>
      {dot && <span style={{ width: 5, height: 5, borderRadius: "var(--tasty-radius-pill)", background: "currentColor", flex: "none" }} />}
      {children}
    </span>
  );
}

const gvHeadStrip = { display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 28, flex: "none",
  padding: "0 var(--tasty-space-md)", background: "var(--tasty-bg-sidebar)",
  borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
  fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
  letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };
const gvMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 12 };

function PaneHead({ children, right }) {
  return <div style={gvHeadStrip}><span>{children}</span><div style={{ flex: 1 }} />{right}</div>;
}
function EmptyLine({ children }) {
  return <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", padding: "var(--tasty-space-lg)",
    fontSize: "var(--tasty-font-size-term-sm)", fontStyle: "italic", color: "var(--tasty-text-muted)" }}>{children}</div>;
}

// ── worktree rail row — 2 lines so name + oid + badges never overflow 232px ──
function WtRow({ wt, selected, onSelect }) {
  const invalid = wt.state === "invalid";
  const nameColor = invalid ? "var(--tasty-text-disabled)" : selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)";
  return (
    <button className="gv-row" onClick={() => onSelect(wt)} aria-pressed={selected}
      style={{ appearance: "none", textAlign: "left", cursor: "pointer", width: "100%", display: "flex", flexDirection: "column", gap: 3,
        padding: "var(--tasty-space-sm) var(--tasty-space-md)", border: 0,
        borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
        background: selected ? "var(--tasty-surface-active)" : "transparent",
        boxShadow: selected ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none", opacity: invalid ? 0.7 : 1 }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
        <span style={{ ...gvMono, color: nameColor, fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1 }}>{wt.name}</span>
        {wt.type === "main" ? <GBadge tone="info">main</GBadge> : <GBadge>linked</GBadge>}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
        {wt.oid && <span style={{ ...gvMono, fontSize: 11, color: selected ? "var(--tasty-text-on-active)" : "var(--tasty-accent-info-raised)" }}>{wt.oid}</span>}
        <div style={{ flex: 1 }} />
        {wt.state === "current" && <GBadge tone="success" dot>current</GBadge>}
        {wt.state === "locked" && <GBadge tone="warning" dot title={wt.reason}>locked</GBadge>}
        {wt.state === "invalid" && <GBadge tone="danger" dot title={wt.reason}>invalid</GBadge>}
      </div>
    </button>
  );
}

const ST = {
  M: { tone: "warning" }, A: { tone: "success" }, D: { tone: "danger" },
  R: { tone: "primary" }, "?": { tone: null }, U: { tone: "danger" },
};
function ChRow({ ch, selected, onSelect }) {
  const m = ch.path.match(/^(.*\/)?([^/]+)$/);
  const dir = (m && m[1]) || "";
  const file = (m && m[2]) || ch.path;
  return (
    <button className="gv-row" onClick={() => onSelect(ch)} aria-pressed={selected}
      style={{ appearance: "none", textAlign: "left", cursor: "pointer", width: "100%", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)",
        height: 26, padding: "0 var(--tasty-space-md)", border: 0,
        background: selected ? "var(--tasty-surface-active)" : "transparent",
        boxShadow: selected ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none" }}>
      <GBadge tone={ST[ch.p] ? ST[ch.p].tone : null} width={18}>{ch.p}</GBadge>
      <span style={{ ...gvMono, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        <span style={{ color: "var(--tasty-text-muted)" }}>{dir}</span>
        <span style={{ color: "var(--tasty-text-primary)" }}>{file}</span>
      </span>
    </button>
  );
}

function CmRow({ c }) {
  return (
    <div className="gv-row" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 28, padding: "0 var(--tasty-space-md)", whiteSpace: "nowrap", overflow: "hidden" }}>
      <span style={{ ...gvMono, fontSize: 11, color: "var(--tasty-accent-info-raised)", flex: "none" }}>{c.oid}</span>
      {c.refs && c.refs.length > 0 && c.refs.map((r) => <GBadge key={r} tone="info">{r}</GBadge>)}
      <span style={{ fontSize: 13, color: "var(--tasty-text-primary)", flex: 1, minWidth: 40, overflow: "hidden", textOverflow: "ellipsis" }}>{c.summary}</span>
      <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", flex: "none" }}>{c.author}</span>
      <span style={{ ...gvMono, fontSize: 11, color: "var(--tasty-text-muted)", flex: "none" }}>{c.time}</span>
    </div>
  );
}

// ── diff code well — recessed surface, old/new line gutter, ± line tints ──
function DiffLine({ d }) {
  const hunk = d.kind === "@";
  const add = d.kind === "+";
  const del = d.kind === "-";
  const fg = hunk ? "var(--tasty-accent-info-raised)" : add ? "var(--tasty-accent-success-raised)" : del ? "var(--tasty-accent-danger-raised)" : "var(--tasty-text-primary)";
  const bg = hunk ? "color-mix(in srgb, var(--tasty-accent-info) 9%, transparent)"
    : add ? "color-mix(in srgb, var(--tasty-accent-success) 10%, transparent)"
    : del ? "color-mix(in srgb, var(--tasty-accent-danger) 10%, transparent)" : "transparent";
  return (
    <div style={{ display: "flex", fontFamily: "var(--tasty-font-mono)", fontSize: 11, lineHeight: 1.65, background: bg, color: fg }}>
      <span style={{ width: 34, textAlign: "right", flex: "none", color: "var(--tasty-text-disabled)", paddingRight: 6, userSelect: "none" }}>{hunk ? "" : d.old}</span>
      <span style={{ width: 34, textAlign: "right", flex: "none", color: "var(--tasty-text-disabled)", paddingRight: 8, userSelect: "none" }}>{hunk ? "" : d.new}</span>
      <span style={{ width: 14, flex: "none", textAlign: "center", userSelect: "none", opacity: 0.8 }}>{hunk ? "" : d.kind === " " ? "" : d.kind}</span>
      <span style={{ whiteSpace: "pre", paddingRight: 12 }}>{d.text}</span>
    </div>
  );
}

const gvFrame = { width: 960, height: 640, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
  border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)",
  overflow: "hidden", boxShadow: "var(--tasty-shadow-modal)" };

function GitViewer({ onClose, onFlash, repo }) {
  const r = repo || { status: "ok", worktrees: [], changes: [], commits: [], diffs: {} };
  const [selWt, setSelWt] = React.useState(() => {
    const cur = (r.worktrees || []).find((w) => w.state === "current");
    return cur ? cur.id : (r.worktrees && r.worktrees[0] ? r.worktrees[0].id : null);
  });
  const [diffFile, setDiffFile] = React.useState(null);
  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") { diffFile ? setDiffFile(null) : onClose && onClose(); } };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose, diffFile]);

  const worktrees = r.worktrees || [];
  const changes = r.changes || [];
  const commits = r.commits || [];
  const curWt = worktrees.find((w) => w.id === selWt);
  const ctxName = curWt ? curWt.name : "—";
  const ctxBranch = (r.branch || (curWt && curWt.branch)) || "detached";
  const ctxOid = curWt ? curWt.oid : "";
  const diffLines = diffFile ? (r.diffs && r.diffs[diffFile]) || [] : null;

  const Header = (
    <>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)", flex: "none",
        borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name={GV.branch} size={16} /></span>
        <span style={{ fontSize: 14, fontWeight: 600 }}>Git</span>
        <div style={{ flex: 1 }} />
        <Button variant="secondary" size="sm" leadingIcon={<Icon name={GV.refresh} size={14} />}
          onClick={() => onFlash && onFlash("Refreshed")}>Refresh</Button>
        <IconButton size="sm" aria-label="Close" title="Close (Esc)" onClick={onClose}><Icon name={GV.x} size={16} /></IconButton>
      </div>
      {/* context strip — current worktree · branch · oid · repo path */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 30, flex: "none", padding: "0 var(--tasty-size-14)",
        background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name={GV.tree} size={13} /></span>
        <span style={{ ...gvMono, color: "var(--tasty-text-primary)" }}>{ctxName}</span>
        <span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-text-disabled)" }}>·</span>
        <span style={{ ...gvMono, color: "var(--tasty-text-secondary)" }}>{ctxBranch}</span>
        {ctxOid && <GBadge tone="info">{ctxOid}</GBadge>}
        <div style={{ flex: 1 }} />
        <span style={{ ...gvMono, fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 320 }}>{r.path}</span>
      </div>
    </>
  );

  let body;
  if (r.status === "nonrepo") {
    body = <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)", textAlign: "center", padding: "var(--tasty-space-xl)" }}>
      <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)", opacity: 0.5 }}><Icon name={GV.branch} size={30} /></span>
      <span style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-secondary)" }}>No git repository found</span>
      <span style={{ fontSize: 12, color: "var(--tasty-text-muted)", maxWidth: 320, lineHeight: 1.5 }}>The current working directory isn’t inside a git repository. Open a repo folder and reopen.</span>
    </div>;
  } else if (r.status === "busy") {
    body = <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)", textAlign: "center", padding: "var(--tasty-space-xl)" }}>
      <span style={{ fontSize: 13, fontWeight: 600, color: "var(--tasty-text-secondary)" }}>Git viewer is already open</span>
      <span style={{ fontSize: 12, color: "var(--tasty-text-muted)" }}>The existing window was brought to the front.</span>
    </div>;
  } else {
    body = (
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        {/* worktree rail */}
        <div style={{ width: 232, flex: "none", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", display: "flex", flexDirection: "column", minHeight: 0 }}>
          <PaneHead>Worktrees ({worktrees.length})</PaneHead>
          {worktrees.length === 0 ? <EmptyLine>No worktrees.</EmptyLine> : (
            <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
              {worktrees.map((wt) => <WtRow key={wt.id} wt={wt} selected={wt.id === selWt}
                onSelect={(w) => { if (w.state !== "invalid") { setSelWt(w.id); setDiffFile(null); } }} />)}
            </div>
          )}
        </div>
        {/* right column */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          {/* Changes (top half) */}
          <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
            <PaneHead>Changes ({changes.length})</PaneHead>
            {changes.length === 0 ? <EmptyLine>No changes.</EmptyLine> : (
              <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "var(--tasty-space-xs) 0" }}>
                {changes.map((ch) => <ChRow key={ch.path} ch={ch} selected={diffFile === ch.path}
                  onSelect={(c) => setDiffFile(c.path)} />)}
              </div>
            )}
          </div>
          {/* Commits ↔ Diff (bottom half) */}
          <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
            {diffFile ? (
              <>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 32, flex: "none", padding: "0 var(--tasty-space-sm) 0 var(--tasty-space-md)",
                  background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                  <Button variant="ghost" size="sm" leadingIcon={<Icon name={GV.back} size={14} />} onClick={() => setDiffFile(null)}>Back</Button>
                  <span style={{ ...gvMono, fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{diffFile}</span>
                </div>
                {diffLines && diffLines.length > 0 ? (
                  <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", background: "var(--tasty-bg-app)", padding: "var(--tasty-space-xs) 0" }}>
                    {diffLines.map((d, i) => <DiffLine key={i} d={d} />)}
                  </div>
                ) : <EmptyLine>No changes.</EmptyLine>}
              </>
            ) : (
              <>
                <PaneHead>Commits ({commits.length})</PaneHead>
                {commits.length === 0 ? <EmptyLine>No commits.</EmptyLine> : (
                  <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "var(--tasty-space-xs) 0" }}>
                    {commits.map((c) => <CmRow key={c.oid} c={c} />)}
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <Scrim onClose={onClose}>
      <div style={gvFrame}>
        {Header}
        {r.error && (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-size-14)", flex: "none",
            background: "color-mix(in srgb, var(--tasty-accent-danger) 10%, transparent)",
            borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", fontSize: 12, color: "var(--tasty-accent-danger-raised)" }}>
            <Icon name={GV.warn} size={14} />{r.error}
          </div>
        )}
        {body}
        <style>{`.gv-row:hover{ background: var(--tasty-overlay-hover); } .gv-row[aria-pressed="true"]:hover{ background: var(--tasty-surface-active); }`}</style>
      </div>
    </Scrim>
  );
}

export { GitViewer };
