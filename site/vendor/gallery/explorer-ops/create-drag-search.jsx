import React from "react";
import * as XDS from "../../components/index.js";
import * as XK from "../parts/explorer-kit.jsx";
import { Section, Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer file operations (2026-10-09, batch 9), part 1:
// shared cell frame + Create / Drag & drop / Search sections.
// Reuses the explorer kit from plugins/index.jsx via parts/explorer-kit.jsx.


const { IconButton: XIconButton, Button: XButton, Input: XInput, MenuItem: XMenuItem, Kbd: XKbd, Spinner: XSpinner, Checkbox: XCheckbox, Icon: XIcon } = XDS;


const xCap = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };
function XLbl({ children }) { return <div style={xCap}>{children}</div>; }
function XCol({ label, children, w }) {
  return <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: w, maxWidth: "100%", minWidth: 0 }}><XLbl>{label}</XLbl>{children}</div>;
}
function XThemes({ render, gap = "var(--tasty-space-lg)" }) {
  return (
    <div style={{ display: "flex", gap, flexWrap: "wrap", alignItems: "flex-start" }}>
      {[["Mocha", "mocha"], ["Latte", "latte"]].map(([l, t]) => (
        <div key={l} data-theme={t || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0 }}>
          <XLbl>{l}</XLbl>{render(l)}
        </div>
      ))}
    </div>
  );
}
const xtb = (label, glyph, o = {}) => <XIconButton key={o.key || label} size="sm" aria-label={label} title={label} disabled={o.disabled} active={o.active}>{glyph}</XIconButton>;
function XSep() { return <span style={{ width: "var(--tasty-border-width)", height: "var(--tasty-icon-size-md)", background: "var(--tasty-separator)", margin: "0 var(--tasty-space-xs)" }} />; }

// Toolbar with the 2026-10-09 command groups: nav · PathField · [create] | [find · preview] · SegToggle.
// narrow (cell < explorer-toolbar-compact-below 440): create + view groups fold into one More menu.
function XToolbar({ narrow, remote, denied, preview, search, path = "~/Downloads", view = "detail" }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none", height: 44, padding: "0 var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 1 }}>{xtb("Back", XK.ic.back)}{xtb("Forward", XK.ic.fwd, { disabled: true })}{xtb("Up", XK.ic.up)}{xtb("Refresh", XK.ic.refresh)}</div>
      <XK.PathField icon={XK.ic.folderOpen} path={path} />
      {narrow ? xtb("More", <XIcon name="more" />) : (
        <div style={{ display: "flex", alignItems: "center", gap: 1 }}>
          {!remote && <>{xtb(denied ? "Can't write to this folder" : "New folder", <XIcon name="folderPlus" />, { disabled: denied, key: "nf" })}{xtb(denied ? "Can't write to this folder" : "New file", <XIcon name="filePlus" />, { disabled: denied, key: "nfile" })}<XSep /></>}
          {xtb("Find", <XIcon name="search" />, { active: search })}
          {xtb("Preview panel", <XIcon name="columns" />, { active: preview })}
        </div>
      )}
      <XK.SegToggle value={view} />
    </div>
  );
}
function XStatus({ children, right }) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none", height: "var(--tasty-status-bar-height)", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-bg-sidebar)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
      <span style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", overflow: "hidden", whiteSpace: "nowrap" }}>{children || "5 items"}</span>
      {right}
    </div>
  );
}
function XCell({ w = 640, h = 300, toolbar, bar, status, statusRight, children, overlay }) {
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: w, height: h, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
      {toolbar || <XToolbar />}
      {bar}
      <div style={{ flex: 1, minHeight: 0, display: "flex", position: "relative" }}>{children}</div>
      <XStatus right={statusRight}>{status}</XStatus>
      {overlay}
    </div>
  );
}
const XROWS = [
  { glyph: XK.ic.folder, name: "Documents", size: "—", date: "2026-10-02 10:14", type: "Folder" },
  { glyph: XK.ic.folder, name: "mockup-exports", size: "—", date: "2026-06-20 14:30", type: "Folder" },
  { glyph: XK.ic.file, name: "report.pdf", size: "2.4 MB", date: "2026-06-24 09:12", type: "PDF" },
  { glyph: XK.ic.image, name: "diagram.png", size: "488 KB", date: "2026-06-26 18:05", type: "PNG", glyphColor: "var(--tasty-accent-info-raised)" },
  { glyph: XK.ic.file, name: "notes.md", size: "12 KB", date: "2026-06-27 11:40", type: "Markdown" },
];
function XDetail({ rows = XROWS, states = {}, before, ring = {} }) {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)" }}>
      <XK.DetailHeader />
      <div style={{ flex: 1, overflow: "hidden" }}>
        {before}
        {rows.map((r, i) => ring[i] ? <XRing key={i}><XK.DetailRow {...r} /></XRing> : <XK.DetailRow key={i} {...r} state={states[i]} />)}
      </div>
    </div>
  );
}
// Drop target — 1px inset accent ring + tint fill. Never moves layout.
function XRing({ children, radius = "var(--tasty-radius-sm)", style }) {
  return <div style={{ background: "var(--tasty-explorer-drop-target-bg)", boxShadow: "inset 0 0 0 var(--tasty-border-width) var(--tasty-explorer-drop-target-border)", borderRadius: radius, ...style }}>{children}</div>;
}

// ── Create ────────────────────────────────────────────────
function XNameError({ text, left = 34 }) {
  return (
    <div role="alert" style={{ position: "absolute", top: "100%", left, zIndex: 2, marginTop: 2, maxWidth: "var(--tasty-explorer-name-error-max-width)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-explorer-name-error-fg)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-popover)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", lineHeight: "var(--tasty-line-height-ui)" }}>{text}</div>
  );
}
function XDetailEdit({ glyph = XK.ic.folder, value = "New folder", error }) {
  return (
    <div style={{ position: "relative" }}>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 80px 132px 92px", alignItems: "center", height: "var(--tasty-table-cell-height)", padding: "0 10px", background: "var(--tasty-surface-active)" }}>
        <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
          <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{glyph}</span>
          <span style={{ flex: 1, minWidth: 0 }}><XInput block defaultValue={value} invalid={!!error} /></span>
        </span>
        <span /><span /><span />
      </div>
      {error && <XNameError text={error} />}
    </div>
  );
}
function XListEdit({ glyph = XK.ic.folder, value = "New folder", error }) {
  return (
    <div style={{ position: "relative", display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-table-cell-height)", padding: "0 8px", borderRadius: "var(--tasty-radius-sm)", background: "var(--tasty-surface-active)" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{glyph}</span>
      <span style={{ flex: 1, minWidth: 0 }}><XInput block defaultValue={value} invalid={!!error} /></span>
      {error && <XNameError text={error} left={32} />}
    </div>
  );
}
function XListRow({ glyph, name, state }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-tree-row-height)", padding: "0 8px", borderRadius: "var(--tasty-radius-sm)", fontSize: 13, color: state === "selected" ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)", background: state === "selected" ? "var(--tasty-surface-active)" : "transparent" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{glyph}</span>{name}
    </div>
  );
}
function XGridEdit({ glyph = XK.ic.file, value = "untitled.txt", error }) {
  return (
    <div style={{ position: "relative", width: 80, display: "flex", flexDirection: "column", alignItems: "center", gap: 4, padding: "8px 4px", borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-active)" }}>
      <span style={{ display: "inline-flex", height: "var(--tasty-explorer-grid-thumb-size)", alignItems: "center", color: "var(--tasty-text-muted)" }}>{glyph}</span>
      <span style={{ width: "var(--tasty-size-160)", alignSelf: "center", zIndex: 1 }}><XInput block defaultValue={value} invalid={!!error} /></span>
      <span style={{ height: 14 }} />
      {error && <XNameError text={error} left={-36} />}
    </div>
  );
}

function CreateSection() {
  return (
    <Section id="create" title="New folder · new file · where file commands live">
      <Spec id="toolbar-create-group-and-view-group-wide-narrow-remote-read-only" title="Toolbar — create group and view group (wide · narrow · remote · read-only)"
        when={<>Two small icon groups sit between the <b>PathField</b> and the <b>SegToggle</b>: <b>create</b> (New folder · New file) and, after a 1px separator, <b>view</b> (Find · Preview panel, both toggles with the IconButton <code>active</code> state). Icon-only, sm, tooltip = the command name. Delete, Rename and the clipboard stay in the context menu and on keys; the toolbar holds only what has no selection target. When the <b>cell</b> is narrower than <span className="tok">--tasty-explorer-toolbar-compact-below</span> (440) both groups fold into one <b>More</b> IconButton whose menu lists the same four rows; the PathField keeps flex 1 and the nav group never folds. A <b>remote (mirror)</b> explorer hides the create group (Find and Preview stay). A folder the user can't write to keeps the buttons <b>disabled</b> with the tooltip “Can't write to this folder”.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14, alignItems: "stretch" }}>
          <XCol label="wide · local"><div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", maxWidth: 640 }}><XToolbar /></div></XCol>
          <XCol label="wide · preview on · find on"><div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", maxWidth: 640 }}><XToolbar preview search /></div></XCol>
          <XCol label="remote (mirror) — create group hidden"><div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", maxWidth: 640 }}><XToolbar remote path="build-eu:~/logs" /></div></XCol>
          <XCol label="permission denied — create disabled"><div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden", maxWidth: 640 }}><XToolbar denied path="/usr/share" /></div></XCol>
          <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <XCol label="narrow cell (< 440) — More" w={400}><div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><XToolbar narrow /></div></XCol>
            <XK.CtxMenu title="More menu">
              <XMenuItem label="New folder" icon={<XIcon name="folderPlus" />} />
              <XMenuItem label="New file" icon={<XIcon name="filePlus" />} />
              <XMenuItem separator />
              <XMenuItem label="Find" icon={<XIcon name="search" />} />
              <XMenuItem label="Preview panel" icon={<XIcon name="columns" />} selected />
            </XK.CtxMenu>
          </div>
        </Stage>
        <Meta
          specs={[["order", "nav · PathField (flex 1) · create · | · view · SegToggle"], ["create", "folderPlus · filePlus · IconButton sm, icon-only"], ["view", "search · columns — toggles (active state)"], ["separator", "1px × 16 · separator · 4 each side"], ["narrow", <>cell &lt; <span className="tok">--tasty-explorer-toolbar-compact-below</span> 440 → one More (more glyph) · menu = same rows, toggles show the check</>], ["remote", "create hidden · view kept"], ["no write access", "create disabled · tooltip “Can't write to this folder”"], ["not in toolbar", "Delete · Rename · Copy / Cut / Paste (menu + keys)"]]}
          tokens={[{ tok: "--tasty-explorer-toolbar-compact-below", use: "→ size-440" }, { tok: "--tasty-separator", use: "group separator", color: "var(--tasty-separator)" }, { tok: "--tasty-state-disabled-fg", use: "disabled create", color: "var(--tasty-state-disabled-fg)" }]} />
        <Note>New glyphs: <code>icons/folderPlus.svg</code>, <code>icons/filePlus.svg</code> (and <code>info</code> for Properties). Keybinding actions the app may add: <code>explorer.new_folder</code>, <code>explorer.new_file</code>, <code>explorer.find</code>, <code>explorer.toggle_preview</code>, <code>explorer.properties</code> — the design names actions only, never keys.</Note>
      </Spec>

      <Spec id="context-menu-create-rows-empty-area-folder-and-properties" title="Context menu — create rows (empty area · folder) and Properties"
        when={<>The <b>empty-area</b> shape (target = current folder) opens with a new first group: <b>New folder</b> · <b>New file</b>. A <b>folder</b> row (list or sidebar tree) gets the same two rows inside its file-ops group; they create inside that folder. Every shape ends with <b>Properties</b> (info glyph) in the system group. Remote explorers hide the create rows like the other write rows. The <code>F2</code> / <code>Del</code> hints stay representative; the app shows a hint only when a binding exists.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <div style={{ display: "flex", gap: 18, flexWrap: "wrap", justifyContent: "center", alignItems: "flex-start" }}>
            <XK.CtxMenu title="Empty area → current folder">
              <XMenuItem label="New folder" icon={<XIcon name="folderPlus" />} />
              <XMenuItem label="New file" icon={<XIcon name="filePlus" />} />
              <XMenuItem separator />
              <XMenuItem label="Copy path" icon={XK.ic.link} />
              <XMenuItem label="Add to favorites" icon={XK.ic.star} />
              <XMenuItem label="Paste" icon={XK.ic.paste} shortcut={<XKbd keys="Ctrl+V" />} />
              <XMenuItem separator />
              <XMenuItem label="Properties" icon={<XIcon name="info" />} />
            </XK.CtxMenu>
            <XK.CtxMenu title="Single folder">
              <XMenuItem label="Copy path" icon={XK.ic.link} />
              <XMenuItem label="Add to favorites" icon={XK.ic.star} />
              <XMenuItem label="Copy" icon={XK.ic.copy} shortcut={<XKbd keys="Ctrl+C" />} />
              <XMenuItem label="Cut" icon={XK.ic.scissors} shortcut={<XKbd keys="Ctrl+X" />} />
              <XMenuItem label="Paste into" icon={XK.ic.paste} />
              <XMenuItem separator />
              <XMenuItem label="New folder" icon={<XIcon name="folderPlus" />} />
              <XMenuItem label="New file" icon={<XIcon name="filePlus" />} />
              <XMenuItem label="Rename" icon={XK.ic.rename} />
              <XMenuItem label="Move to Trash" icon={XK.ic.trash} danger />
              <XMenuItem separator />
              <XMenuItem label="Open in system" icon={XK.ic.external} />
              <XMenuItem label="Properties" icon={<XIcon name="info" />} />
            </XK.CtxMenu>
          </div>
        </Stage>
        <Meta
          specs={[["empty area", "create group first · then clipboard · then Properties"], ["folder", "create rows join the file-ops group · target = that folder"], ["file / multi", "no create rows · Properties last"], ["remote", "create rows hidden"], ["target folder", "fixed when the command starts — navigating away still creates there"]]}
          tokens={[{ tok: "--tasty-menu-bg", use: "menu", color: "var(--tasty-menu-bg)" }, { tok: "--tasty-menu-item-fg", use: "rows", color: "var(--tasty-menu-item-fg)" }]} />
      </Spec>

      <Spec id="name-input-inline-at-the-top-of-the-list-detail-list-grid" title="Name input — inline, at the top of the list (Detail · List · Grid)"
        when={<>A new item is named <b>inline</b>, not in a dialog: an editing row appears at the <b>top</b> of the current listing (not at its sorted place, so it never jumps while typing), selected, with the shared <b>Input</b> in the name slot. This is the same component as the inline rename the grid spec describes; the Rename dialog stays until the app builds it. Defaults: folder <b>“New folder”</b> fully selected, file <b>“untitled.txt”</b> with the <b>stem</b> selected; if the name is taken the default becomes “New folder 2”. <span className="ic">↵</span> confirms, <span className="ic">Esc</span> cancels and nothing is written. <b>Losing focus confirms</b> when the name is valid and <b>cancels</b> when it is empty or invalid. After a confirm the item moves to its sorted place and stays selected — only if the user is still on that folder in that explorer; focus never moves to another surface.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="Detail — new folder" w={420}>
            <div style={{ height: 172, display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><XDetail rows={XROWS.slice(1)} before={<XDetailEdit />} /></div>
          </XCol>
          <XCol label="List — new file" w={220}>
            <div style={{ height: 172, padding: "6px 4px", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <XListEdit glyph={XK.ic.file} value="untitled.txt" />
              {XROWS.slice(0, 4).map((r) => <XListRow key={r.name} glyph={r.glyph} name={r.name} />)}
            </div>
          </XCol>
          <XCol label="Grid — new file (editor spans 160, centred)" w={300}>
            <div style={{ height: 172, padding: "8px 8px 8px 48px", display: "flex", gap: 4, alignItems: "flex-start", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              <XGridEdit />
              <XK.GridCell glyph={XK.ic.folder} name="Documents" />
            </div>
          </XCol>
        </Stage>
        <Meta
          specs={[["placement", "top of the listing while typing → sorted place after ↵"], ["editor", "Input (control-height 28) in the name slot · Detail row stays 28 · List row grows 22 → 28 while editing · Grid: 160 wide, centred under the glyph"], ["defaults", "“New folder” all selected · “untitled.txt” stem selected · taken → “… 2”"], ["↵ / Esc", "confirm / cancel (nothing on disk)"], ["blur", "valid → confirm · empty or invalid → cancel"], ["after", "sorted place, selected — only if the user still views that folder"], ["write fails", "error Toast + reload (unchanged)"]]}
          tokens={[{ tok: "--tasty-surface-active", use: "editing row", color: "var(--tasty-surface-active)" }, { tok: "--tasty-input-border-focus", use: "editor focus", color: "var(--tasty-input-border-focus)" }, { tok: "--tasty-table-cell-height", use: "row 28" }]} />
      </Spec>

      <Spec id="name-errors-empty-invalid-character-already-exists" title="Name errors — empty · invalid character · already exists"
        when={<>Errors show <b>at the input</b>, never as a toast, and the input stays open: the Input takes its <code>invalid</code> border and a small message box hangs <b>under the field</b>, over the next rows (the list does not shift). The message box is the menu surface with a 1px <span className="tok">--tasty-explorer-name-error-fg</span> edge and text-primary caption copy, so it reads at 4.5:1 in both themes. Empty and invalid are checked as the user types; “already exists” is checked on <span className="ic">↵</span> (the app never overwrites).</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: 44, width: 400 }}>
              <div style={{ position: "relative", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}><XDetailEdit value="" error="Enter a name." /></div>
              <div style={{ position: "relative", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}><XDetailEdit value="drafts/2026" error="A name can't contain “/”." /></div>
              <div style={{ position: "relative", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", marginBottom: 32 }}><XDetailEdit value="Documents" error="“Documents” already exists in this folder." /></div>
            </div>
          )} />
        </Stage>
        <Meta
          specs={[["input", "Input invalid (danger border)"], ["message", "under the field · menu-bg · 1px explorer-name-error-fg · caption 11 · text-primary · max explorer-name-error-max-width 240 · overlaps rows below"], ["empty", "“Enter a name.”"], ["invalid", "“A name can't contain “{char}”.” · also reserved names on Windows: “This name is reserved by the system.”"], ["exists", "“{name}” already exists in this folder. — checked on ↵"], ["after an error", "input stays open, text kept"]]}
          tokens={[{ tok: "--tasty-explorer-name-error-fg", use: "→ accent-danger", color: "var(--tasty-explorer-name-error-fg)" }, { tok: "--tasty-menu-bg", use: "message box", color: "var(--tasty-menu-bg)" }, { tok: "--tasty-shadow-popover", use: "message lift" }]} />
        <Note>i18n: <code>explorer.new.folder_default</code> “New folder” · <code>explorer.new.file_default</code> “untitled.txt” · <code>explorer.name.empty</code> · <code>explorer.name.invalid_char</code> · <code>explorer.name.reserved</code> · <code>explorer.name.exists</code>. The default names are translated; the extension of the file default is not.</Note>
      </Spec>
    </Section>
  );
}

// ── Drag & drop ───────────────────────────────────────────
const XOPS = {
  move: { fg: "var(--tasty-explorer-drag-move-fg)", icon: "move", word: "Move to" },
  copy: { fg: "var(--tasty-explorer-drag-copy-fg)", icon: "plus", word: "Copy to" },
  refused: { fg: "var(--tasty-explorer-drag-refused-fg)", icon: "close", word: "Can't drop" },
};
function XDragChip({ glyph = XK.ic.file, label, op = "move", target, reason, style }) {
  const o = XOPS[op];
  return (
    <div style={{ display: "inline-flex", flexDirection: "column", gap: 2, maxWidth: "var(--tasty-explorer-drag-chip-max-width)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-popover)", pointerEvents: "none", ...style }}>
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0, fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>
        <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{glyph}</span>
        <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label}</span>
      </span>
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: o.fg }}>
        <XIcon name={o.icon} size="var(--tasty-icon-size-xs)" />
        <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{o.word} {op === "refused" ? <span style={{ color: "var(--tasty-text-secondary)" }}>— {reason}</span> : <b style={{ fontWeight: "var(--tasty-font-weight-semibold)" }}>{target}</b>}</span>
      </span>
    </div>
  );
}

function DragSection() {
  const box = { position: "relative", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "visible" };
  return (
    <Section id="drag" title="Drag to move or copy · drop indication">
      <Spec id="drag-chip-one-item-many-items-move-copy-refused" title="Drag chip — one item · many items · move / copy / refused"
        when={<>While dragging, a <b>chip</b> follows the pointer (12 right, 12 below). Line 1 is what is dragged: the item's glyph and name, or a stack glyph and “3 items”. Line 2 says <b>what the drop will do</b> before release, in the op colour: <b>Move to {"{folder}"}</b> (accent-primary), <b>Copy to {"{folder}"}</b> (accent-success) or <b>Can't drop — {"{reason}"}</b> (accent-danger). Default: <b>move on the same disk, copy across disks</b>; holding the modifier from Keybindings flips it and the line updates at once. The dragged rows themselves keep their selection look (no dim; dim stays the meaning of Cut).</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: 10, alignItems: "flex-start" }}>
              <XDragChip label="report.pdf" op="move" target="Documents" />
              <XDragChip glyph={<XIcon name="layers" />} label="3 items" op="copy" target="USB-backup" />
              <XDragChip glyph={<XIcon name="layers" />} label="3 items" op="refused" reason="a folder can't go inside itself" />
              <XDragChip glyph={XK.ic.folder} label="mockup-exports" op="refused" reason="remote folders are read-only" />
            </div>
          )} />
        </Stage>
        <Meta
          specs={[["chip", "surface-raised · 1px border-strong · radius · shadow-popover · max 240 · pointer +12 / +12"], ["line 1", "glyph + name (body 13, ellipsis) · many → layers glyph + “{n} items”"], ["line 2", "caption 11 · op glyph 12 · op colour · target folder semibold"], ["default op", "same disk → move · other disk → copy · modifier flips (key from Keybindings)"], ["refused", "onto itself / own subfolder · remote explorer · no write access"], ["Esc", "cancels the drag, nothing changes"]]}
          tokens={[{ tok: "--tasty-explorer-drag-move-fg", use: "→ accent-primary", color: "var(--tasty-explorer-drag-move-fg)" }, { tok: "--tasty-explorer-drag-copy-fg", use: "→ accent-success", color: "var(--tasty-explorer-drag-copy-fg)" }, { tok: "--tasty-explorer-drag-refused-fg", use: "→ accent-danger", color: "var(--tasty-explorer-drag-refused-fg)" }, { tok: "--tasty-explorer-drag-chip-max-width", use: "→ size-240" }]} />
      </Spec>

      <Spec id="drop-targets-folder-row-grid-cell-tree-node-favorite-current-fol" title="Drop targets — folder row · grid cell · tree node · favorite · current folder"
        when={<>Every target takes the same mark: a <b>1px inset accent ring</b> plus the accent tint fill (<span className="tok">--tasty-explorer-drop-target-bg</span> = accent-primary at tint-fill-alpha). Nothing moves or grows. A <b>folder</b> row or cell is a target; a <b>file</b> row is not — over a file, the target falls back to the <b>folder being shown</b>, and the whole list body takes the ring. Sidebar tree nodes and favorite rows are targets too. Hovering a <b>closed tree folder</b> for 800 ms expands it. A refused target gets <b>no ring</b>; the chip says why.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="Detail — onto a folder row" w={420}>
            <div style={{ ...box, height: 172, display: "flex" }}>
              <XDetail ring={{ 0: true }} states={{ 2: "selected" }} />
              <XDragChip label="report.pdf" op="move" target="Documents" style={{ position: "absolute", left: 120, top: 52 }} />
            </div>
          </XCol>
          <XCol label="List body — onto the current folder (over a file / empty space)" w={300}>
            <div style={{ ...box, height: 172 }}>
              <XRing radius="var(--tasty-radius)" style={{ height: "100%", padding: "6px 4px", boxSizing: "border-box" }}>
                {XROWS.map((r) => <XListRow key={r.name} glyph={r.glyph} name={r.name} />)}
              </XRing>
              <XDragChip glyph={<XIcon name="layers" />} label="2 items" op="copy" target="Downloads" style={{ position: "absolute", left: 110, top: 96 }} />
            </div>
          </XCol>
          <XCol label="Grid — onto a folder cell" w={260}>
            <div style={{ ...box, height: 132, padding: 8, display: "flex", gap: 4, background: "var(--tasty-bg-panel)" }}>
              <XRing radius="var(--tasty-radius)"><XK.GridCell glyph={XK.ic.folder} name="Documents" /></XRing>
              <XK.GridCell glyph={XK.ic.file} name="report.pdf" state="selected" />
              <XK.GridCell glyph={XK.ic.file} name="notes.md" />
            </div>
          </XCol>
          <XCol label="Sidebar — tree node · favorite" w={196}>
            <div style={{ ...box, background: "var(--tasty-bg-sidebar)", paddingBottom: 6 }}>
              <XK.SideHead>Files</XK.SideHead>
              <div style={{ padding: "0 6px", display: "flex", flexDirection: "column", gap: 1 }}>
                <XK.TreeNode label="Home" open />
                <XK.TreeNode label="Downloads" depth={1} open active />
                <XRing><XK.TreeNode label="Projects" depth={1} /></XRing>
              </div>
              <XK.SideHead>Favorites</XK.SideHead>
              <div style={{ padding: "0 6px", display: "flex", flexDirection: "column", gap: 1 }}>
                <XRing><XK.FavRow name="tasty" /></XRing>
                <XK.FavRow name="screenshots" />
              </div>
            </div>
          </XCol>
        </Stage>
        <Meta
          specs={[["mark", "inset 1px ring + tint fill · radius of the row / cell · no layout change"], ["folder row / cell", "target"], ["file row / empty space", "target = folder being shown → ring on the whole list body"], ["tree node · favorite", "target"], ["closed tree folder", "expands after 800 ms hover"], ["refused", "no ring · chip line 2 in refused colour"], ["drop", "same operation as Paste → progress, conflicts and results below"]]}
          tokens={[{ tok: "--tasty-explorer-drop-target-border", use: "→ accent-primary", color: "var(--tasty-explorer-drop-target-border)" }, { tok: "--tasty-explorer-drop-target-bg", use: "accent-primary × tint-fill-alpha", color: "var(--tasty-explorer-drop-target-bg)" }, { tok: "--tasty-tint-fill-alpha", use: "0.12" }]} />
      </Spec>

      <Spec id="files-from-the-os-copy-into-the-explorer-not-open" title="Files from the OS — copy into the explorer, not open"
        when={<>Files dragged in from the OS and dropped <b>on an explorer cell</b> are now <b>copied into</b> the target folder (folder row, or the folder being shown). Over an explorer cell the window drop overlay is <b>not drawn</b>; the explorer shows its own ring and a chip that always reads <b>Copy to</b> (an OS drag is never a move). Everywhere else in the window the existing overlay and “open with handler” stay as they are. A remote explorer refuses the OS drop and says so in the chip. Dragging items <b>out</b> of the explorer (for example into a terminal to paste a path) is left for a later pass.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="OS files over a local explorer" w={420}>
            <div style={{ position: "relative" }}>
              <XCell w={420} h={240} status="5 items">
                <XRing radius="0" style={{ flex: 1, display: "flex" }}><XDetail /></XRing>
              </XCell>
              <XDragChip glyph={<XIcon name="layers" />} label="2 items" op="copy" target="Downloads" style={{ position: "absolute", left: 140, top: 150 }} />
            </div>
          </XCol>
          <XCol label="OS files over a remote explorer" w={420}>
            <div style={{ position: "relative" }}>
              <XCell w={420} h={240} toolbar={<XToolbar remote path="build-eu:~/logs" />} status="5 items"><XDetail /></XCell>
              <XDragChip glyph={<XIcon name="layers" />} label="2 items" op="refused" reason="remote folders are read-only" style={{ position: "absolute", left: 120, top: 150 }} />
            </div>
          </XCol>
          <XCol label="elsewhere — window file-drop overlay (tokens b10)" w={420}>
            <XThemes gap="var(--tasty-space-sm)" render={() => (
              <div style={{ position: "relative", width: 420, maxWidth: "100%", height: 160, background: "var(--tasty-surface-terminal-focused-bg)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", padding: "var(--tasty-space-sm)", boxSizing: "border-box" }}>
                ~/work/tasty ❯
                <div style={{ position: "absolute", inset: "var(--tasty-space-sm)", display: "flex", alignItems: "center", justifyContent: "center", gap: "var(--tasty-space-sm)", background: "var(--tasty-file-drop-overlay-bg)", border: "var(--tasty-border-width) solid var(--tasty-file-drop-overlay-border)", borderRadius: "var(--tasty-radius)", color: "var(--tasty-file-drop-overlay-fg)", fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-body)" }}>
                  <XIcon name="download" size="var(--tasty-icon-size-md)" />Drop to open
                </div>
              </div>
            )} />
          </XCol>
        </Stage>
        <Meta
          specs={[["over an explorer", "copy into · window drop overlay suppressed for that cell"], ["elsewhere", "window overlay + open with handler (unchanged)"], ["op", "always Copy to (no move from the OS)"], ["remote", "refused · “remote folders are read-only”"], ["drag out", "not in this pass"]]}
          tokens={[{ tok: "--tasty-explorer-drop-target-border", use: "body ring", color: "var(--tasty-explorer-drop-target-border)" }, { tok: "--tasty-explorer-drag-copy-fg", use: "Copy to", color: "var(--tasty-explorer-drag-copy-fg)" }, { tok: "--tasty-file-drop-overlay-bg", use: "window overlay fill · 0.12 (was alpha 31)", color: "var(--tasty-file-drop-overlay-bg)" }, { tok: "--tasty-file-drop-overlay-border", use: "window overlay edge · 0.36 (was alpha 153)", color: "var(--tasty-file-drop-overlay-border)" }, { tok: "--tasty-file-drop-overlay-fg", use: "glyph + label · full ink", color: "var(--tasty-file-drop-overlay-fg)" }]} />
        <Note>i18n: <code>explorer.drag.items</code> “{"{n}"} items” · <code>explorer.drag.move_to</code> “Move to {"{folder}"}” · <code>explorer.drag.copy_to</code> “Copy to {"{folder}"}” · <code>explorer.drag.refused</code> “Can't drop — {"{reason}"}” · reasons: <code>into_itself</code> “a folder can't go inside itself” · <code>same_folder</code> “already in this folder” · <code>remote</code> “remote folders are read-only” · <code>no_write</code> “no write access”.</Note>
      </Spec>
    </Section>
  );
}

// ── Search & filter ───────────────────────────────────────
function XHi({ text, q }) {
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (!q || i < 0) return text;
  return <>{text.slice(0, i)}<span style={{ color: "var(--tasty-explorer-match-fg)" }}>{text.slice(i, i + q.length)}</span>{text.slice(i + q.length)}</>;
}
function XFindBar({ value = "", deep = false, right, remote }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none", height: "var(--tasty-explorer-search-bar-height)", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <span style={{ flex: 1, minWidth: 0 }}><XInput block icon={<XIcon name="search" />} defaultValue={value} placeholder={deep ? "Search in Downloads and subfolders" : "Filter this folder"} /></span>
      {!remote && <XCheckbox label="Subfolders" defaultChecked={deep} />}
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", whiteSpace: "nowrap" }}>{right}</span>
      <XIconButton size="sm" aria-label="Close" title="Close">{XK.ic.x}</XIconButton>
    </div>
  );
}
const XHITS = [
  ["report.pdf", "", "2.4 MB", "06-24 09:12"],
  ["report-final.pdf", "Documents", "2.6 MB", "10-02 10:14"],
  ["q3-report.xlsx", "Documents/finance", "88 KB", "09-30 17:02"],
  ["report.md", "mockup-exports/notes", "4 KB", "07-01 08:40"],
];
function XHitRow({ name, folder, size, date, q, state }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr var(--tasty-explorer-search-folder-col-width) 72px 92px", alignItems: "center", height: "var(--tasty-table-cell-height)", padding: "0 10px", fontSize: 13, background: state === "selected" ? "var(--tasty-surface-active)" : "transparent", color: state === "selected" ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)" }}>
      <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}><span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{XK.ic.file}</span><span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}><XHi text={name} q={q} /></span></span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", paddingRight: 8 }}>{folder || "."}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)", textAlign: "right", paddingRight: 8 }}>{size}</span>
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{date}</span>
    </div>
  );
}
function XHitHeader() {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr var(--tasty-explorer-search-folder-col-width) 72px 92px", alignItems: "center", height: "var(--tasty-table-cell-height)", padding: "0 10px", background: "var(--tasty-table-header-bg)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", fontSize: "var(--tasty-table-header-font-size)", fontWeight: "var(--tasty-table-header-font-weight)", textTransform: "uppercase", letterSpacing: "var(--tasty-table-header-tracking)", color: "var(--tasty-table-header-fg)" }}>
      <span>Name</span><span>Folder</span><span style={{ textAlign: "right", paddingRight: 8 }}>Size</span><span>Date</span>
    </div>
  );
}

function SearchSection() {
  return (
    <Section id="search" title="Filter this folder · search subfolders">
      <Spec id="find-bar-filter-the-current-folder" title="Find bar — filter the current folder"
        when={<>The <b>Find</b> action (toolbar search toggle, or the app's find action while the explorer has focus) opens a <b>bar docked under the toolbar</b>, the full width of the cell, 36 high. Typing <b>filters</b> the folder being shown at once: rows that don't match are hidden, the matched part of each name takes <span className="tok">--tasty-explorer-match-fg</span>, and the bar shows <b>“{"{shown}"} of {"{total}"}”</b>. Type-ahead is off while the field has focus; on the list it jumps among the visible rows. <span className="ic">Esc</span> in the field clears the text, a second <span className="ic">Esc</span> closes the bar; × closes it; Back / Up / the address bar close it too. Remote explorers get the filter.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <XCell w={520} h={210} toolbar={<XToolbar search />} bar={<XFindBar value="re" right="2 of 5" />} status="2 of 5 items">
              <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
                <XK.DetailHeader />
                <XK.DetailRow glyph={XK.ic.file} name={<XHi text="report.pdf" q="re" />} size="2.4 MB" date="2026-06-24 09:12" type="PDF" state="selected" />
                <XK.DetailRow glyph={XK.ic.folder} name={<XHi text="mockup-exports" q="re" />} size="—" date="2026-06-20 14:30" type="Folder" />
              </div>
            </XCell>
          )} />
        </Stage>
        <Meta
          specs={[["placement", "bar under the toolbar · full cell width · explorer-search-bar-height 36"], ["field", "Input + search icon · placeholder “Filter this folder”"], ["match", "substring, case-insensitive · matched part in explorer-match-fg"], ["count", "mono caption · “{shown} of {total}”"], ["type-ahead", "off in the field · on the list it cycles visible rows"], ["Esc", "clear → close"], ["closes on", "× · Back · Up · address bar · folder change"], ["remote", "filter yes"]]}
          tokens={[{ tok: "--tasty-explorer-search-bar-height", use: "→ size-36" }, { tok: "--tasty-explorer-match-fg", use: "→ autocomplete-match-fg (accent-primary)", color: "var(--tasty-explorer-match-fg)" }, { tok: "--tasty-input-bg", use: "field", color: "var(--tasty-input-bg)" }]} />
      </Spec>

      <Spec id="subfolders-recursive-search-searching-no-results-errors-stopped" title="Subfolders — recursive search · searching · no results · errors · stopped"
        when={<>Ticking <b>Subfolders</b> turns the same bar into a <b>recursive search</b> from the folder being shown. It runs in the background and fills the list as it goes: <b>Searching… {"{n}"} found</b> with a Spinner and a <b>Stop</b> button. Results replace the listing; <b>Detail</b> adds a <b>Folder</b> column (path relative to the start folder, mono, “.” for the start folder itself) and drops Type. <b>List</b> and <b>Grid</b> keep their look and show the folder in the tooltip and on the status line for the selected result. Every command on a result (open, copy, cut, rename, trash) uses its <b>real path</b>. Folders that can't be read are skipped and counted in accent-warning; nothing found is the centred empty state. Remote explorers don't offer Subfolders.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="searching — partial results" w={520}>
            <XCell w={520} h={230} toolbar={<XToolbar search />} bar={<XFindBar value="report" deep right={<><XSpinner size="var(--tasty-icon-size-sm)" />Searching… 4 found<XButton variant="ghost" size="sm">Stop</XButton></>} />} status={<>Documents/finance/q3-report.xlsx</>}>
              <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
                <XHitHeader />
                {XHITS.map(([n, f, s, d], i) => <XHitRow key={n} name={n} folder={f} size={s} date={d} q="report" state={i === 2 ? "selected" : null} />)}
              </div>
            </XCell>
          </XCol>
          <XCol label="done — 2 folders skipped" w={520}>
            <XCell w={520} h={140} toolbar={<XToolbar search />} bar={<XFindBar value="report" deep right={<>4 found · <span style={{ color: "var(--tasty-accent-warning-raised)" }} title="~/Downloads/private, ~/Downloads/.Trash">2 folders skipped</span></>} />}>
              <div style={{ flex: 1, display: "flex", flexDirection: "column" }}><XHitHeader />{XHITS.slice(0, 1).map(([n, f, s, d]) => <XHitRow key={n} name={n} folder={f} size={s} date={d} q="report" />)}</div>
            </XCell>
          </XCol>
          <XCol label="stopped" w={520}>
            <XCell w={520} h={140} toolbar={<XToolbar search />} bar={<XFindBar value="report" deep right={<>Stopped · 4 found</>} />}>
              <div style={{ flex: 1, display: "flex", flexDirection: "column" }}><XHitHeader />{XHITS.slice(0, 1).map(([n, f, s, d]) => <XHitRow key={n} name={n} folder={f} size={s} date={d} q="report" />)}</div>
            </XCell>
          </XCol>
          <XCol label="no results" w={520}>
            <XCell w={520} h={220} toolbar={<XToolbar search />} bar={<XFindBar value="invoice-2019" deep right="0 found" />}>
              <XK.ExpState glyph={<XIcon name="search" />} title="No matches in Downloads" sub="Subfolders included." />
            </XCell>
          </XCol>
          <XCol label="error — the start folder can't be read" w={520}>
            <XCell w={520} h={220} toolbar={<XToolbar search />} bar={<XFindBar value="report" deep right="—" />}>
              <XK.ExpState glyph={<XIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-explorer-error-fg)" title="Search failed" reason="Permission denied (os error 13)" actions={<XButton variant="secondary" size="sm">Retry</XButton>} />
            </XCell>
          </XCol>
        </Stage>
        <Meta
          specs={[["switch", "Subfolders checkbox in the bar (hidden on remote)"], ["running", "Spinner 14 · “Searching… {n} found” · Stop (ghost sm) · results stream in"], ["Detail", "Name · Folder (explorer-search-folder-col-width 160, mono, relative) · Size · Date"], ["List / Grid", "unchanged rows · folder in tooltip + status line"], ["skipped", "“{n} folders skipped” · accent-warning · tooltip lists them"], ["no results", "state screen · search glyph · “No matches in {folder}”"], ["error", "state screen · error tone · Retry"], ["stopped", "“Stopped · {n} found” · results kept"], ["leave", "untick → filter · Esc / × / navigation → folder"]]}
          tokens={[{ tok: "--tasty-explorer-search-folder-col-width", use: "→ size-160" }, { tok: "--tasty-spinner-indicator", use: "searching", color: "var(--tasty-spinner-indicator)" }, { tok: "--tasty-accent-warning", use: "skipped count", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-explorer-error-fg", use: "search failed", color: "var(--tasty-explorer-error-fg)" }]} />
        <Note>i18n: <code>explorer.find.filter_placeholder</code> · <code>explorer.find.search_placeholder</code> “Search in {"{folder}"} and subfolders” · <code>explorer.find.subfolders</code> · <code>explorer.find.count</code> “{"{shown}"} of {"{total}"}” · <code>explorer.find.searching</code> · <code>explorer.find.stop</code> · <code>explorer.find.stopped</code> · <code>explorer.find.skipped</code> · <code>explorer.find.none</code> “No matches in {"{folder}"}” · <code>explorer.find.failed</code>.</Note>
      </Spec>
    </Section>
  );
}

export { XCol, XThemes, XToolbar, XCell, XDetail, XROWS, XListRow, XHi, XFindBar, XListEdit, XDragChip, CreateSection, DragSection, SearchSection };
