import React from "react";
import * as QDS from "../../components/index.js";
import { XCol, XThemes, XCell } from "./create-drag-search.jsx";
import * as QK from "../parts/explorer-kit.jsx";
import { Section as QSection, Spec as QSpec, Stage as QStage, Meta as QMeta, Note as QNote } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer file operations, part 4 (2026-10-10, batch 12):
// address takes file paths + remote check · hidden toggle on wide toolbars · create field width · auto-scroll and drag
// timing · Undo / Redo menu copy · counting stage · Properties rows · low preview. Loaded before explorer-ops/index.jsx.


const { IconButton: QIconButton, Icon: QIcon, Toast: QToast, MenuItem: QMenuItem, Input: QInput, Spinner: QSpinner } = QDS;



const qMenu = { width: "var(--tasty-size-240)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-space-xs)", boxShadow: "var(--tasty-shadow-popover)" };
const qtb = (label, glyph, o = {}) => <QIconButton key={label} size="sm" aria-label={label} title={label} active={o.active}>{glyph}</QIconButton>;
const qSep = <span style={{ width: "var(--tasty-border-width)", height: "var(--tasty-icon-size-md)", background: "var(--tasty-separator)", margin: "0 var(--tasty-space-xs)" }} />;
const QROWS = [
  { glyph: QK.ic.folder, name: "Documents", size: "—", date: "2026-10-02 10:14", type: "Folder" },
  { glyph: QK.ic.folder, name: "mockup-exports", size: "—", date: "2026-06-20 14:30", type: "Folder" },
  { glyph: QK.ic.file, name: "report.pdf", size: "2.4 MB", date: "2026-06-24 09:12", type: "PDF document" },
  { glyph: QK.ic.image, name: "diagram.png", size: "488 KB", date: "2026-06-26 18:05", type: "PNG image", glyphColor: "var(--tasty-accent-info-raised)" },
  { glyph: QK.ic.file, name: "notes.md", size: "12 KB", date: "2026-06-27 11:40", type: "Markdown" },
];
function QBody({ children }) {
  return <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", position: "relative" }}><QK.DetailHeader /><div style={{ flex: 1, overflow: "hidden", position: "relative" }}>{children}</div></div>;
}
// Address field — idle / pending (remote check). Same box as PathField; trailing slot holds the spinner.
function QAddress({ path, pending, placeholder }) {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "center", gap: 6, height: "var(--tasty-control-height)", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-input-bg)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) solid " + (pending ? "var(--tasty-input-border-focus)" : "var(--tasty-input-border)") }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{QK.ic.folderOpen}</span>
      <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: path ? "var(--tasty-text-secondary)" : "var(--tasty-text-placeholder)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{path || placeholder}</span>
      {pending && <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-spinner-indicator)" }} title="Checking the remote path…"><QSpinner size="var(--tasty-explorer-address-pending-size)" /></span>}
    </div>
  );
}
function QToolbar({ address, hidden }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none", height: 44, padding: "0 var(--tasty-space-sm)", background: "var(--tasty-bg-panel)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 1 }}>{qtb("Back", QK.ic.back)}{qtb("Up", QK.ic.up)}{qtb("Refresh", QK.ic.refresh)}</div>
      {address || <QAddress path="~/Downloads" />}
      <div style={{ display: "flex", alignItems: "center", gap: 1 }}>
        {qtb("New folder", <QIcon name="folderPlus" />)}{qtb("New file", <QIcon name="filePlus" />)}{qSep}
        {qtb("Find", <QIcon name="search" />)}{qtb("Preview panel", <QIcon name="columns" />)}
        {qtb(hidden ? "Hide hidden files (Ctrl+Shift+.)" : "Show hidden files (Ctrl+Shift+.)", <QIcon name="eye" />, { active: hidden })}
      </div>
      <QK.SegToggle value="detail" />
    </div>
  );
}
function QHiddenRow({ glyph, name, size, date, type }) {
  const fg = "var(--tasty-explorer-hidden-fg)";
  return <QK.DetailRow glyph={<span style={{ color: fg, display: "inline-flex" }}>{glyph}</span>} name={<span style={{ color: fg }}>{name}</span>} size={size} date={date} type={type} />;
}
function QCreateRow({ value }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-table-cell-height)", padding: "0 10px", paddingLeft: "calc(10px + var(--tasty-explorer-create-indent))", background: "var(--tasty-surface-active)" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{QK.ic.file}</span>
      <span style={{ flex: "1 1 auto", minWidth: 0, maxWidth: "var(--tasty-explorer-create-field-max-width)" }}><QInput block defaultValue={value} /></span>
      <span style={{ flex: "none", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", whiteSpace: "nowrap" }}>in mockup-exports</span>
    </div>
  );
}
const qToastAt = (node) => <div style={{ position: "absolute", right: "var(--tasty-space-sm)", bottom: "calc(var(--tasty-status-bar-height) + var(--tasty-space-sm))", maxWidth: "calc(100% - 2 * var(--tasty-space-sm))" }}>{node}</div>;

function FollowupsB12Section() {
  return (
    <QSection id="followups12" title="Follow-ups — address · hidden toggle · create field · auto-scroll · Undo / Redo menu">
      <QSpec id="address-field-folders-and-files-remote-check-pending" title="Address field — folders and files · remote check pending"
        when={<>The address field now takes <b>a folder or a file</b>; a file opens its folder and selects it. The placeholder and the not-found toast say <b>path</b>, not folder. <b>Remote</b> explorers must ask the remote whether the path is a file, so on Enter the field <b>keeps the typed path</b> (it does not snap back to the current folder), stays in its focus border and, if the answer has not come after <span className="tok">--tasty-explorer-address-pending-delay</span> 200 ms, shows a <b>Spinner</b> in the trailing slot. The list and the status line do not change: nothing has moved yet. The answer navigates; after 8 s without one the explorer goes to the typed path, as today. <b>Esc</b> during the check cancels it and restores the current path. Local explorers answer at once and never show the pending state.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={(l) => (
            l === "Mocha"
              ? <XCell w={560} h={170} toolbar={<QToolbar address={<QAddress path="/srv/app/logs/2026-10-10.log" pending />} />} status="5 items"><QBody>{QROWS.slice(0, 3).map((r, i) => <QK.DetailRow key={i} {...r} />)}</QBody></XCell>
              : <XCell w={560} h={200} toolbar={<QToolbar address={<QAddress placeholder="Go to folder or file…" />} />} status="5 items" overlay={qToastAt(<QToast variant="danger">Path not found: ~/Downloads/report-final.pdf</QToast>)}><QBody>{QROWS.slice(0, 3).map((r, i) => <QK.DetailRow key={i} {...r} />)}</QBody></XCell>
          )} />
        </QStage>
        <QMeta
          specs={[["pending", "remote only · field keeps the typed path + focus border · Spinner 12 in the trailing slot after 200 ms · list + status unchanged · Esc cancels"], ["end", "answer → navigate (file: open folder + select) · 8 s → go to the typed path"], ["not found", "error toast, explorer scope · path = the confirmed absolute path"], ["placeholder", "“Go to folder or file…”"]]}
          tokens={[{ tok: "--tasty-explorer-address-pending-delay", use: "→ motion-ui-fade 200" }, { tok: "--tasty-explorer-address-pending-size", use: "→ icon-size-sm 12" }, { tok: "--tasty-spinner-indicator", use: "spinner", color: "var(--tasty-spinner-indicator)" }]} />
        <QNote>i18n — <code>explorer.address.not_found</code> “Path not found: {"{}"}” / “경로를 찾을 수 없습니다: {"{}"}” / “パスが見つかりません: {"{}"}” · <code>explorer.address.placeholder</code> “Go to folder or file…” / “폴더나 파일로 이동…” / “フォルダーまたはファイルへ移動…” · spinner tooltip <code>explorer.address.checking</code> “Checking the remote path…” / “원격 경로 확인 중…” / “リモートのパスを確認しています…”.</QNote>
      </QSpec>

      <QSpec id="hidden-files-on-a-wide-toolbar-create-field-width" title="Hidden files on a wide toolbar · create field width"
        when={<>A wide cell gets a <b>hidden files toggle</b> at the end of the view group (after Find and Preview); it folds into <b>More</b> with the rest of the group under <span className="tok">--tasty-explorer-toolbar-compact-below</span> 440. It is an IconButton with the <b>eye</b> glyph that shows <b>pressed</b> while hidden files are shown; its tooltip carries the action words and the shortcut, so the native More menu does not need a shortcut column. The <b>create-in-folder field</b> stops at <span className="tok">--tasty-explorer-create-field-max-width</span> 240 and the muted “in {"{folder}"}” follows it at space-sm, in Detail and List alike. <b>Grid</b> shows no caption (the target cell's drop-target ring already says where). The target ring uses the <b>drag-and-drop ring</b> — radius, not radius-sm — so the two never differ.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={(l) => (
            <XCell w={640} h={200} toolbar={<QToolbar hidden={l === "Mocha"} />} status={l === "Mocha" ? "8 items" : "5 items"}><QBody>
              {l === "Mocha" && <><QHiddenRow glyph={QK.ic.folder} name=".git" size="—" date="2026-10-09 18:12" type="Folder" /><QHiddenRow glyph={QK.ic.file} name=".env" size="210 B" date="2026-10-01 07:45" type="ENV file" /></>}
              {l === "Latte" && <><QK.DetailRow {...QROWS[0]} /><div style={{ background: "var(--tasty-explorer-drop-target-bg)", boxShadow: "inset 0 0 0 var(--tasty-border-width) var(--tasty-explorer-drop-target-border)", borderRadius: "var(--tasty-radius)" }}><QK.DetailRow {...QROWS[1]} /></div><QCreateRow value="untitled.txt" /></>}
              <QK.DetailRow {...QROWS[2]} />
            </QBody></XCell>
          )} />
        </QStage>
        <QMeta
          specs={[["toggle", "IconButton sm · eye · pressed (active) while shown · last in the view group · folds into More under 440"], ["tooltip", "“Show hidden files (Ctrl+Shift+.)” / “Hide hidden files (Ctrl+Shift+.)” · macOS (⌘⇧.)"], ["More row", "native menu, no shortcut column — fine"], ["create field", "max explorer-create-field-max-width 240 · caption at space-sm right after it"], ["Grid", "no caption"], ["target ring", "same as the drop ring · radius (4)"]]}
          tokens={[{ tok: "--tasty-explorer-create-field-max-width", use: "→ size-240" }, { tok: "--tasty-explorer-hidden-fg", use: "shown hidden items", color: "var(--tasty-explorer-hidden-fg)" }, { tok: "--tasty-explorer-drop-target-border", use: "target", color: "var(--tasty-explorer-drop-target-border)" }]} />
        <QNote>Tooltip strings reuse <code>explorer.more.hidden_show</code> / <code>.hidden_hide</code> with the bound shortcut appended in parentheses (the shortcut is formatted, not translated). The b11 kit sample with radius-sm is superseded.</QNote>
      </QSpec>

      <QSpec id="edge-auto-scroll-dragging-items-tree-expand" title="Edge auto-scroll · dragging items · tree expand"
        when={<>One rule for every drag that can reach a list edge: <b>drag-select</b>, <b>dragging items</b> in the listing, and dragging over the <b>sidebar tree</b> (each in its own scroll area). Inside the <span className="tok">--tasty-explorer-autoscroll-zone</span> 24 band the list scrolls at <b>speed = 20 rows/s × t²</b>, t = 0 at the band's inner edge and 1 at the list edge, in <b>rows of the current view</b> (Detail row, List row, Grid cell row) so every view moves the same number of items. Past the edge the pointer keeps the <b>full speed</b>; it does not grow with distance. Nothing is drawn for the band. Hovering a closed tree folder during an item drag expands it after <span className="tok">--tasty-explorer-drag-expand-delay</span> 800 ms; the drop-target ring is the waiting cue, no extra mark.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XCol label="spec annotation — the dashed bands are NOT drawn" w={560}>
            <XCell w={560} h={220} status="2 of 5 selected"><QBody>
              {QROWS.map((r, i) => <QK.DetailRow key={i} {...r} state={i >= 3 ? "selected" : undefined} />)}
              <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: "var(--tasty-explorer-autoscroll-zone)", border: "var(--tasty-border-width) dashed var(--tasty-text-muted)", pointerEvents: "none", display: "flex", alignItems: "center", justifyContent: "flex-end", padding: "0 var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>24 · 0 → 20 rows/s</div>
            </QBody></XCell>
          </XCol>
        </QStage>
        <QMeta
          specs={[["band", "explorer-autoscroll-zone 24, top + bottom · no indicator"], ["speed", "20 rows/s × t² (ease-in) · rows of the current view · rule constant"], ["outside", "keeps 20 rows/s · app drives the scroll offset itself"], ["applies to", "drag-select · item drag (listing) · item drag over the sidebar tree"], ["tree expand", "explorer-drag-expand-delay 800 ms · cue = drop-target ring"]]}
          tokens={[{ tok: "--tasty-explorer-autoscroll-zone", use: "→ size-24" }, { tok: "--tasty-explorer-drag-expand-delay", use: "→ duration-800 (new primitive)" }]} />
      </QSpec>

      <QSpec id="undo-redo-rows-copy-empty-history-windows-shortcuts" title="Undo / Redo rows — copy · empty history · Windows · shortcuts"
        when={<>The Undo / Redo group sits where it is now: in the empty-area menu between the create / paste group and Properties. It is <b>always there</b>: with no step a row is a disabled bare “Undo” / “Redo”. A row names the step with a gerund (“Undo copying 3 items”), with an en <code>_one</code> form. Rows bound to a command <b>show their first binding</b> in the native shortcut column — Undo and Redo as well as Rename and Trash; one rule for the whole menu. A step that can no longer be undone is disabled with the reason in the tooltip on Linux and macOS; Windows popup menus have no item tooltips, so there the <b>row label itself</b> becomes the reason sentence. That sentence is read, so it takes <span className="tok">--tasty-text-muted</span>, not the disabled ink. The reason is a full sentence (“A newer file is there now”), so the lead-in ends with a period, not a colon.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="steps on both sides">
            <div style={qMenu}><QMenuItem label="Paste" shortcut="Ctrl+V" /><QMenuItem separator /><QMenuItem label="Undo copying 3 items" shortcut="Ctrl+Z" /><QMenuItem label="Redo moving 1 item" shortcut="Ctrl+Shift+Z" /><QMenuItem separator /><QMenuItem label="Properties" /></div>
          </XCol>
          <XCol label="empty history">
            <div style={qMenu}><QMenuItem label="Paste" shortcut="Ctrl+V" disabled /><QMenuItem separator /><QMenuItem label="Undo" shortcut="Ctrl+Z" disabled /><QMenuItem label="Redo" shortcut="Ctrl+Shift+Z" disabled /><QMenuItem separator /><QMenuItem label="Properties" /></div>
          </XCol>
          <XCol label="stale — Linux / macOS (tooltip) · Windows (label)">
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <div style={qMenu}><QMenuItem label="Undo moving 3 items" shortcut="Ctrl+Z" disabled title="Can't undo. A newer file is there now" /></div>
              <div style={qMenu}><div role="note" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--tasty-space-lg)", minHeight: "var(--tasty-menu-item-height)", padding: "0 var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-muted)" }}><span>Can't undo. A newer file is there now</span><span style={{ fontSize: "var(--tasty-font-size-caption)" }}>Ctrl+Z</span></div></div>
            </div>
          </XCol>
        </QStage>
        <QMeta
          specs={[["position", "empty-area menu · create / paste | Undo · Redo | Properties (unchanged)"], ["empty", "always shown · disabled bare Undo / Redo"], ["shortcut column", "every row bound to a command shows its first binding — Undo / Redo included"], ["stale", "disabled · tooltip “Can't undo. {reason}” (GTK · macOS) · Windows: same sentence as the row label, text-muted, no hover"]]}
          tokens={[{ tok: "--tasty-state-disabled-fg", use: "disabled rows (kit)", color: "var(--tasty-state-disabled-fg)" }]} />
        <QNote>i18n — <code>explorer.menu.undo</code> “Undo {"{op}"}” / “실행 취소: {"{op}"}” / “元に戻す: {"{op}"}” · <code>explorer.menu.undo_none</code> “Undo” / “실행 취소” / “元に戻す” · <code>explorer.menu.redo_none</code> “Redo” / “다시 실행” / “やり直す” · <code>explorer.menu.op_copy</code> “copying {"{n}"} items” (<code>_one</code> “copying 1 item”) / “{"{n}"}개 복사” / “{"{n}"} 件のコピー” · <code>explorer.menu.op_move</code> “moving {"{n}"} items” (<code>_one</code> “moving 1 item”) / “{"{n}"}개 이동” / “{"{n}"} 件の移動” · <code>explorer.menu.undo_stale</code> changes to “Can't undo. {"{reason}"}” / “되돌릴 수 없습니다. {"{reason}"}” / “元に戻せません。{"{reason}"}” (reason strings stay as they are) · settings labels confirmed: <code>settings.keybindings.explorer_undo_label</code> “Undo:” / “실행 취소:” / “元に戻す:” · <code>explorer_redo_label</code> “Redo:” / “다시 실행:” / “やり直す:”.</QNote>
      </QSpec>
    </QSection>
  );
}

export { QIconButton, QIcon, QBody, QROWS, FollowupsB12Section };
