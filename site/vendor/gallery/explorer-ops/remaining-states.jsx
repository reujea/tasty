import React from "react";
import { XCol, XThemes, XToolbar, XCell } from "./create-drag-search.jsx";
import * as ZK from "../parts/explorer-kit.jsx";
import { Section as ZSection, Spec as ZSpec, Stage as ZStage, Meta as ZMeta, Note as ZNote } from "../parts/specimen.jsx";
import { ZInput, ZKIND_ROWS, ZBody, ZName, ZIcon, ZMenuItem } from "./results-and-refusals.jsx";

const zMenu = { width: "var(--tasty-size-240)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-space-xs)", boxShadow: "var(--tasty-shadow-popover)" };
function ZCursor({ children, on = true }) {
  return <div style={{ boxShadow: on ? "inset 0 0 0 var(--tasty-explorer-cursor-ring-width) var(--tasty-explorer-cursor-ring)" : "none", borderRadius: "var(--tasty-radius-sm)" }}>{children}</div>;
}

function ZMarquee() {
  return <div style={{ position: "absolute", left: 120, top: 40, width: 260, height: 70, background: "var(--tasty-explorer-marquee-bg)", border: "var(--tasty-border-width) solid var(--tasty-explorer-marquee-border)", pointerEvents: "none" }} />;
}
function ZEditRow({ value }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, height: "var(--tasty-table-cell-height)", padding: "0 10px", paddingLeft: "calc(10px + var(--tasty-explorer-create-indent))", background: "var(--tasty-surface-active)" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{ZK.ic.file}</span>
      <span style={{ flex: 1, minWidth: 0, maxWidth: 260 }}><ZInput block defaultValue={value} /></span>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", whiteSpace: "nowrap" }}>in mockup-exports</span>
    </div>
  );
}

function RemainingSection() {
  const rows = ZKIND_ROWS;
  return (
    <ZSection id="remaining" title="Remaining states — links · keyboard item · drag-select · hidden files · show in folder · redo · create in folder (b11)">
      <ZSpec id="links-in-the-listing-broken-link" title="Links in the listing · broken link"
        when={<>A symlink keeps its <b>target's glyph</b> (folder, file, image) and gets a trailing <b>link</b> glyph 12 after the name in <span className="tok">--tasty-explorer-link-glyph</span>, in all three views (Grid: after the label's last line). Type reads “Link to {"{kind}"}”. A <b>broken</b> link has no target glyph to borrow: the link glyph takes the item slot in <span className="tok">--tasty-explorer-link-broken-fg</span> (warning), the name stays normal ink (no strike-through — nothing was deleted), Type reads “Broken link”, and the tooltip names the missing target.</>}>
        <ZStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <XCell w={560} h={170} status="4 items"><ZBody>
              <ZK.DetailRow glyph={ZK.ic.folder} name={<ZName text="projects" link />} size="—" date="2026-10-04 12:00" type="Link to Folder" />
              <ZK.DetailRow {...rows[5]} />
              <ZK.DetailRow glyph={<ZIcon name="link" size="var(--tasty-icon-size-md)" />} glyphColor="var(--tasty-explorer-link-broken-fg)" name="old-config.toml" size="—" date="2026-08-11 09:30" type="Broken link" />
              <ZK.DetailRow {...rows[2]} />
            </ZBody></XCell>
          )} />
        </ZStage>
        <ZMeta
          specs={[["link", "target glyph · trailing link 12 (explorer-link-glyph-size) · gap space-xs · explorer-link-glyph"], ["broken", "link glyph in the item slot · explorer-link-broken-fg → accent-warning · name normal · Type “Broken link” · tooltip “Target not found: {path}”"], ["views", "Detail · List · Grid alike"]]}
          tokens={[{ tok: "--tasty-explorer-link-glyph", use: "→ text-muted", color: "var(--tasty-explorer-link-glyph)" }, { tok: "--tasty-explorer-link-glyph-size", use: "→ icon-size-xs 12" }, { tok: "--tasty-explorer-link-broken-fg", use: "→ accent-warning", color: "var(--tasty-explorer-link-broken-fg)" }]} />
        <ZNote>i18n: <code>explorer.link.target_missing</code> “Target not found: {"{path}"}” / “대상을 찾을 수 없음: {"{path}"}” / “リンク先が見つかりません: {"{path}"}”.</ZNote>
      </ZSpec>

      <ZSpec id="keyboard-current-item-drag-select-rectangle" title="Keyboard current item · drag-select rectangle"
        when={<>Arrow keys, Home / End and PageUp / Down move a <b>current item</b> separate from the selection. It is a 1px <b>inset</b> ring in <span className="tok">--tasty-explorer-cursor-ring</span> (border-focus) and is drawn only after keyboard movement; a mouse click hides it until the next key. On a selected row it sits on top of the selection fill. <b>Drag-select</b> from empty space draws a rectangle with <span className="tok">--tasty-explorer-marquee-bg</span> and a 1px <span className="tok">--tasty-explorer-marquee-border</span> (the drop-target tint pair). Near the top or bottom edge, inside <span className="tok">--tasty-explorer-autoscroll-zone</span> 24, the list scrolls faster the closer the pointer gets; nothing extra is drawn.</>}>
        <ZStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={(l) => (
            <XCell w={560} h={200} status={l === "Mocha" ? "2 of 5 selected" : "3 of 5 selected"}><ZBody>
              {rows.slice(0, 5).map((r, i) => {
                const sel = l === "Mocha" ? i === 2 || i === 3 : i >= 1 && i <= 3;
                const row = <ZK.DetailRow {...r} state={sel ? "selected" : undefined} />;
                return l === "Mocha" ? <ZCursor key={i} on={i === 3}>{row}</ZCursor> : <div key={i}>{row}</div>;
              })}
              {l === "Latte" && <ZMarquee />}
            </ZBody></XCell>
          )} />
        </ZStage>
        <ZMeta
          specs={[["current item", "1px inset ring · explorer-cursor-ring → border-focus · keyboard only · over selection"], ["marquee", "fill explorer-marquee-bg (tint-fill 0.12) · 1px explorer-marquee-border (tint-border 0.36) · no radius"], ["auto-scroll", "explorer-autoscroll-zone 24 at top / bottom · speed grows toward the edge · no indicator"], ["samples", "Mocha = keyboard ring on a selected row · Latte = drag-select"]]}
          tokens={[{ tok: "--tasty-explorer-cursor-ring", use: "→ border-focus", color: "var(--tasty-explorer-cursor-ring)" }, { tok: "--tasty-explorer-cursor-ring-width", use: "→ border-width 1" }, { tok: "--tasty-explorer-marquee-bg", use: "fill", color: "var(--tasty-explorer-marquee-bg)" }, { tok: "--tasty-explorer-marquee-border", use: "edge", color: "var(--tasty-explorer-marquee-border)" }, { tok: "--tasty-explorer-autoscroll-zone", use: "→ size-24" }]} />
      </ZSpec>

      <ZSpec id="hidden-files-show-in-enclosing-folder-undo-redo" title="Hidden files · show in enclosing folder · undo / redo"
        when={<><b>Hidden files</b> toggle from a <b>More</b> row that says the action (“Show hidden files” / “Hide hidden files”, no check mark) and the shortcut Ctrl+Shift+. (macOS ⌘⇧.). Off by default. When shown, hidden items use <span className="tok">--tasty-explorer-hidden-fg</span> for name and glyph — muted ink, not opacity, so they never look like cut items. While they are hidden the status line counts them: “5 items · 3 hidden”. <b>Show in enclosing folder</b> (search results and links) opens the folder that really holds the item, selects it, puts the current item on it and scrolls it into view; no flash. <b>Redo</b> lives beside Undo in the empty-area context menu and on Ctrl+Shift+Z / Ctrl+Y (macOS ⌘⇧Z). Each explorer keeps up to <b>10</b> steps; no history list is shown. A step that can no longer be undone leaves the row disabled with the reason in its tooltip.</>}>
        <ZStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="hidden shown" w={400}>
            <XCell w={400} h={180} toolbar={<XToolbar narrow />} status="8 items"><ZBody>
              <ZK.DetailRow glyph={<span style={{ color: "var(--tasty-explorer-hidden-fg)", display: "inline-flex" }}>{ZK.ic.folder}</span>} name={<ZName text=".git" fg="var(--tasty-explorer-hidden-fg)" />} size="—" date="2026-10-09 18:12" type="Folder" />
              <ZK.DetailRow glyph={<span style={{ color: "var(--tasty-explorer-hidden-fg)", display: "inline-flex" }}>{ZK.ic.file}</span>} name={<ZName text=".env" fg="var(--tasty-explorer-hidden-fg)" />} size="210 B" date="2026-10-01 07:45" type="ENV file" />
              {rows.slice(0, 3).map((r, i) => <ZK.DetailRow key={i} {...r} />)}
            </ZBody></XCell>
          </XCol>
          <XCol label="hidden off — status count" w={400}>
            <XCell w={400} h={120} toolbar={<XToolbar narrow />} status="5 items · 3 hidden"><ZBody>{rows.slice(0, 2).map((r, i) => <ZK.DetailRow key={i} {...r} />)}</ZBody></XCell>
          </XCol>
          <XCol label="More — hidden row">
            <div style={zMenu}>
              <ZMenuItem icon={<ZIcon name="folderPlus" />} label="New folder" />
              <ZMenuItem icon={<ZIcon name="search" />} label="Find" />
              <ZMenuItem icon={<ZIcon name="eye" />} label="Show hidden files" shortcut="Ctrl+Shift+." />
            </div>
          </XCol>
          <XCol label="search result — context menu">
            <div style={zMenu}>
              <ZMenuItem label="Open" />
              <ZMenuItem icon={<ZIcon name="folderOpen" />} label="Show in enclosing folder" />
              <ZMenuItem separator />
              <ZMenuItem label="Copy path" />
            </div>
          </XCol>
          <XCol label="search result — native menu (Linux · macOS, 2026-10-14)">
            <div style={zMenu}>
              <ZMenuItem label="Open" />
              <ZMenuItem label="Show in enclosing folder" />
              <ZMenuItem separator />
              <ZMenuItem label="Copy path" />
              <ZMenuItem label="Copy" shortcut="Ctrl+C" />
              <ZMenuItem label="Cut" shortcut="Ctrl+X" />
              <ZMenuItem separator />
              <ZMenuItem label="Rename" shortcut="F2" />
              <ZMenuItem label="Move to Trash" shortcut="Del" />
            </div>
          </XCol>
          <XCol label="empty area — undo / redo">
            <div style={zMenu}>
              <ZMenuItem label="Undo move 3 items" shortcut="Ctrl+Z" />
              <ZMenuItem label="Redo rename" shortcut="Ctrl+Shift+Z" />
              <ZMenuItem separator />
              <ZMenuItem label="Undo trash 2 items" disabled title="Can't undo: the items were emptied from the trash" />
            </div>
          </XCol>
        </ZStage>
        <ZMeta
          specs={[["hidden toggle", "More row · action words · Ctrl+Shift+. / ⌘⇧. · default off · per explorer, remembered"], ["hidden shown", "name + glyph explorer-hidden-fg (→ text-muted) · no opacity"], ["hidden off", "status “{n} items · {h} hidden”"], ["show in folder", "opens the real parent · selects + current item · scrolls into view · no flash"], ["search result · native", "Open · Show in enclosing folder · separator · the item's usual file menu from Copy path on · no glyphs (OS menu) · Open = file → default handler, folder → navigate into it"], ["redo", "context menu beside Undo · Ctrl+Shift+Z · Ctrl+Y · ⌘⇧Z"], ["history", "10 steps per explorer · no list · stale step = disabled row + tooltip reason"], ["Undo card", "unchanged — one step, no Redo button"]]}
          tokens={[{ tok: "--tasty-explorer-hidden-fg", use: "→ text-muted", color: "var(--tasty-explorer-hidden-fg)" }]} />
        <ZNote>i18n: <code>explorer.more.hidden_show</code> “Show hidden files” / “숨김 파일 보기” / “隠しファイルを表示” · <code>.hidden_hide</code> “Hide hidden files” / “숨김 파일 숨기기” / “隠しファイルを非表示” · <code>explorer.status.hidden</code> “{"{n}"} hidden” / “숨김 {"{n}"}개” / “非表示 {"{n}"}” · <code>explorer.menu.show_in_folder</code> “Show in enclosing folder” / “들어 있는 폴더에서 보기” / “含まれているフォルダで表示” · <code>explorer.menu.redo</code> “Redo {"{op}"}” / “다시 실행: {"{op}"}” / “やり直す: {"{op}"}” · <code>explorer.menu.undo_stale</code> “Can't undo: {"{reason}"}” / “되돌릴 수 없음: {"{reason}"}” / “元に戻せません: {"{reason}"}”.</ZNote>
      </ZSpec>

      <ZSpec id="create-inside-a-folder-input-under-the-target-row" title="Create inside a folder — input under the target row"
        when={<>“New file / New folder” from a <b>folder row's</b> menu puts the input row <b>directly under that folder</b>, indented by <span className="tok">--tasty-explorer-create-indent</span> 16, with a muted “in {"{folder}"}” after the field. The target folder takes the <b>drop-target ring</b> while the field is open, so the destination is visible in every view. In <b>Grid</b> the new cell appears right after the folder cell. Enter creates the item inside the folder (the listing does not navigate); Esc cancels.</>}>
        <ZStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <XCell w={560} h={180} status="5 items"><ZBody>
              <ZK.DetailRow {...rows[0]} />
              <div style={{ background: "var(--tasty-explorer-drop-target-bg)", boxShadow: "inset 0 0 0 var(--tasty-border-width) var(--tasty-explorer-drop-target-border)", borderRadius: "var(--tasty-radius-sm)" }}><ZK.DetailRow glyph={ZK.ic.folder} name="mockup-exports" size="—" date="2026-06-20 14:30" type="Folder" /></div>
              <ZEditRow value="untitled.txt" />
              <ZK.DetailRow {...rows[2]} />
            </ZBody></XCell>
          )} />
        </ZStage>
        <ZMeta
          specs={[["input", "under the target row · indent explorer-create-indent 16 · caption “in {folder}” muted"], ["target", "drop-target ring + fill while the field is open"], ["Grid", "new cell right after the folder cell"], ["keys", "Enter creates inside · Esc cancels · listing stays"]]}
          tokens={[{ tok: "--tasty-explorer-create-indent", use: "→ space-lg 16" }, { tok: "--tasty-explorer-drop-target-border", use: "target", color: "var(--tasty-explorer-drop-target-border)" }]} />
        <ZNote>i18n: <code>explorer.create.in_folder</code> “in {"{folder}"}” / “{"{folder}"} 안에” / “{"{folder}"} 内”.</ZNote>
      </ZSpec>
    </ZSection>
  );
}

export { RemainingSection };
