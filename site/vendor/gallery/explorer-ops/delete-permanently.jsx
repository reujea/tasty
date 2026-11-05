import React from "react";
import { MenuItem, Kbd, Icon, Button } from "../../components/index.js";
import { ExplorerDeleteFrame, XD_ITEMS } from "../parts/explorer-delete-frame.jsx";
import { XCol, XThemes, XCell, XDetail } from "./create-drag-search.jsx";
import * as ExplorerOpsY_ns from "./index.jsx";
import * as K from "../parts/explorer-kit.jsx";
import { Section, Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer file operations: Delete permanently (2026-10-12, request explorer-permanent-delete).
// Menu row · confirm (shared frame from parts/explorer-delete-frame.jsx) · cancel / invalid · progress · results.






const Y = () => ExplorerOpsY_ns;

function Scrimmed({ children, h = 340 }) {
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: 640, height: h, borderRadius: "var(--tasty-radius)", overflow: "hidden", border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-bg-app)" }}>
      <div style={{ position: "absolute", inset: 0, display: "flex" }}><XDetail /></div>
      <div style={{ position: "absolute", inset: 0, background: "var(--tasty-scrim-bg)", display: "flex", alignItems: "center", justifyContent: "center", padding: "var(--tasty-space-md)" }}>{children}</div>
    </div>
  );
}

function PermanentDeleteSection() {
  const { YResult, YProgressStatus, YStatusRight, YToastAt } = Y();
  const cap = (t) => <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-secondary)" }}>{t}</span>;
  return (
    <Section id="permdelete" title="Delete permanently — confirm and results">
      <Spec id="context-menu-delete-permanently-right-after-move-to-trash" title="Context menu — “Delete permanently…” right after Move to Trash"
        when={<>One new row, <b>directly under Move to Trash</b> in the same file-ops group, so the two ways to delete sit together and the safe one comes first. Danger tone like Trash, label ends in “…” because it always opens a confirm. The shortcut column shows the <b>first configured binding</b> (defaults: Shift+Delete on Windows / Linux, ⌘⌥⌫ on macOS); with no binding it shows nothing. The row is <b>hidden</b> where there is nothing to delete (empty-area menu) and on <b>remote</b> explorers (no remote delete in this scope). The key on a remote shows an info toast; with no selection it does nothing. No toolbar button.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          {[["Windows / Linux", "Shift+Delete"], ["macOS", "⌘⌥⌫"]].map(([os, key]) => (
            <K.CtxMenu key={os} title={"File · " + os}>
              <MenuItem label="Copy path" icon={K.ic.link} />
              <MenuItem label="Copy" icon={K.ic.copy} shortcut={<Kbd keys={os === "macOS" ? "⌘C" : "Ctrl+C"} />} />
              <MenuItem label="Cut" icon={K.ic.scissors} shortcut={<Kbd keys={os === "macOS" ? "⌘X" : "Ctrl+X"} />} />
              <MenuItem separator />
              <MenuItem label="Rename" icon={K.ic.rename} />
              <MenuItem label="Move to Trash" icon={K.ic.trash} danger shortcut={<Kbd keys={os === "macOS" ? "⌘⌫" : "Delete"} />} />
              <MenuItem label="Delete permanently…" icon={K.ic.trash} danger shortcut={<Kbd keys={key} />} />
              <MenuItem separator />
              <MenuItem label="Properties" icon={<Icon name="info" />} />
            </K.CtxMenu>
          ))}
          <XCol label="remote explorer — key pressed" w={320}>
            <YResult variant="info" title="Permanent delete isn't available for remote folders." />
          </XCol>
        </Stage>
        <Meta
          specs={[["label", "“Delete permanently…” · danger · trash glyph"], ["position", "right after Move to Trash, same group"], ["shortcut", "first configured binding · none → blank"], ["empty area / no selection", "row hidden · key does nothing"], ["remote", "row hidden · key → info toast"], ["toolbar", "no button"]]}
          tokens={[{ tok: "--tasty-accent-danger", use: "danger row (MenuItem danger)", color: "var(--tasty-accent-danger-raised)" }]} />
      </Spec>

      <Spec id="confirm-window-scope-centred-dialog-file-folder-link-many-long-p" title="Confirm — window-scope centred dialog · file · folder · link · many · long paths"
        when={<>The confirm uses the existing <b>Window-scope centred confirm</b> (scrim over the window, the same as the category and script confirms): the act is destructive and the dialog must hold the keyboard. It does <b>not</b> borrow the 380 category width; it reuses the <b>info-modal frame</b> (<span className="tok">--tasty-info-modal-width</span> 440, max <span className="tok">--tasty-info-modal-max-height</span> 360) because it carries a list. Structure: title row (danger trash glyph, “Delete permanently?”) · a <b>summary sentence</b> that names one item or counts the top-level items · the <b>target list</b> (glyph, name, parent path in mono, ellipsised at the front so the end of the path stays) that scrolls past <span className="tok">--tasty-explorer-delete-list-max-height</span> · the line <b>“This can't be undone.”</b> · footer <b>Cancel</b> (ghost) and <b>Delete permanently</b> (danger). A folder row says “contents included”; a link says the link is deleted and its target is kept. Same-named files from different folders are told apart by their parent path.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="one file · in place (Mocha)" w={640}><Scrimmed><ExplorerDeleteFrame items={XD_ITEMS.file} /></Scrimmed></XCol>
          <XCol label="one folder" w={440}><ExplorerDeleteFrame items={XD_ITEMS.folder} /></XCol>
          <XCol label="one link" w={440}><ExplorerDeleteFrame items={XD_ITEMS.link} /></XCol>
          <XCol label="6 items from search results · different folders · long names · list scrolls" w={440}><ExplorerDeleteFrame items={XD_ITEMS.many} /></XCol>
        </Stage>
        <Meta
          specs={[["scope", "Window scrim (existing centred-confirm policy) · bound to the source explorer"], ["frame", "info-modal-width 440 · max info-modal-max-height 360 · bg-panel · 1px border-strong · shadow-modal"], ["padding", "space-md top · size-14 sides (dialog shell)"], ["title", "trash glyph icon-size-md accent-danger · font-size-max semibold"], ["summary", "body · text-secondary · names the one item or counts top-level items + folders"], ["list", "bg-app well · 1px border-default · rows: glyph 14 · name body · tag caption · parent mono caption, front ellipsis · scroll past explorer-delete-list-max-height 160"], ["warning", "alertTriangle accent-danger + “This can't be undone.” text-primary semibold"], ["footer", "Cancel ghost sm · Delete permanently danger sm · right-aligned · wraps on narrow"], ["link", "link glyph · “The item it points to is kept.” · broken link glyph in explorer-link-broken-fg"]]}
          tokens={[{ tok: "--tasty-info-modal-width", use: "frame width", color: null }, { tok: "--tasty-info-modal-max-height", use: "frame max" }, { tok: "--tasty-explorer-delete-list-max-height", use: "new · → size-160" }, { tok: "--tasty-accent-danger", use: "glyphs", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-scrim-bg", use: "window dim" }]} />
      </Spec>

      <Spec id="confirm-latte-ko-ja-narrow-window-keyboard-focus" title="Confirm — Latte · ko · ja · narrow window · keyboard focus"
        when={<>Default focus is <b>Cancel</b>. <span className="ic">Tab</span> / <span className="ic">Shift+Tab</span> move between Cancel and Delete permanently (the list is not a stop; it scrolls with the wheel). <span className="ic">↵</span> and <span className="ic">Space</span> press the <b>focused</b> button, so an Enter left over from the menu or the key that opened the dialog presses Cancel. Auto-repeat of the opening key is ignored. <span className="ic">Esc</span>, the close action and a click on the scrim all <b>cancel</b>: nothing is deleted. The danger button fires once and is disabled right after the press, so a double click or repeat can't queue two jobs. On a window narrower than the frame, the frame takes the window width minus the scrim margin and the footer wraps.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          <div data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
            {cap("Latte · ko · focus on Cancel (default)")}
            <ExplorerDeleteFrame lang="ko" items={XD_ITEMS.many.slice(0, 3)} focus="cancel" />
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
            {cap("Mocha · ja · Tab → focus on Delete permanently")}
            <ExplorerDeleteFrame lang="ja" items={XD_ITEMS.folder} focus="ok" />
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
            {cap("narrow window (300) · en · after the press: disabled")}
            <ExplorerDeleteFrame items={XD_ITEMS.many.slice(3, 5)} focus={null} okState="disabled" width="var(--tasty-size-300)" />
          </div>
        </Stage>
        <Meta
          specs={[["default focus", "Cancel"], ["Tab order", "Cancel → Delete permanently → (wraps)"], ["↵ / Space", "press the focused button"], ["opening key", "auto-repeat ignored · a held Enter hits Cancel"], ["Esc · close · scrim click", "cancel · 0 deletes"], ["single fire", "danger button disabled after the first press"], ["focus ring", "focus-ring-width · border-focus · 1px offset"], ["narrow", "frame = window width − scrim margin · footer wraps"]]}
          tokens={[{ tok: "--tasty-border-focus", use: "focus ring", color: "var(--tasty-border-focus)" }, { tok: "--tasty-focus-ring-width", use: "2" }]} />
        <Note>i18n (new): <code>explorer.delete.menu</code> “Delete permanently…” / “영구 삭제…” / “完全に削除…” · <code>explorer.delete.title</code> “Delete permanently?” / “영구 삭제할까요?” / “完全に削除しますか?” · <code>explorer.delete.one_file</code> · <code>explorer.delete.one_folder</code> · <code>explorer.delete.one_link</code> · <code>explorer.delete.many</code> + <code>explorer.delete.many_one</code> (“1 item will be deleted…”) · <code>explorer.delete.many_folders</code> + <code>_one</code> (“including everything inside {"{k}"} folders” / “1 folder”) · <code>explorer.delete.irreversible</code> “This can't be undone.” / “되돌릴 수 없습니다.” / “元に戻せません。” · <code>explorer.delete.confirm</code> “Delete permanently” / “영구 삭제” / “完全に削除” · <code>explorer.delete.tag_folder</code> / <code>tag_link</code> / <code>tag_broken_link</code> · <code>explorer.delete.remote_unavailable</code>. Full strings in the frame source <code>gallery/explorer-delete-frame.jsx</code> XD_COPY.</Note>
      </Spec>

      <Spec id="cancel-and-invalid-targets-nothing-is-deleted" title="Cancel and invalid targets — nothing is deleted"
        when={<>The confirm keeps a <b>snapshot</b> of the paths and of the explorer that opened it. Changing the selection or folder behind the scrim does not change the targets. If that <b>explorer goes away</b> (pane closed, surface replaced) the confirm closes and an info toast in the window says nothing was deleted. If a target <b>disappeared or was replaced</b> between the confirm and the job reaching it, that item is skipped, never the new file; it is reported in the result as “changed since you confirmed”.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)", maxWidth: "100%" }}>
              <YResult variant="info" title="The explorer closed before you confirmed. Nothing was deleted." />
              <YResult variant="warning" title="Deleted 5 of 6 · 1 skipped" lines={[["~/Downloads/report.pdf", "Changed since you confirmed — skipped"]]} />
            </div>
          )} />
        </Stage>
        <Meta
          specs={[["snapshot", "paths + source explorer, taken when the confirm opens"], ["source gone", "confirm closes · info toast · 0 deletes"], ["target gone / replaced", "skipped · result line “Changed since you confirmed — skipped”"], ["cancel", "no toast (nothing happened)"]]}
          tokens={[{ tok: "--tasty-toast-accent-info", use: "source gone", color: "var(--tasty-toast-accent-info)" }]} />
      </Spec>

      <Spec id="progress-deleting-items-only-stop-at-the-next-item" title="Progress — “Deleting”, items only, stop at the next item"
        when={<>After the press the job goes into the existing bounded queue and shows on the explorer's status line. The verb is <b>Deleting</b> (never “Moving to Trash” or “Copying”). Total bytes are unknown, so the line shows <b>top-level items only</b> and the bar is the <b>track with no fill</b> (the existing unknown-size rule); no percent. Cancel × reads <b>“Stop after this item”</b>: stopping happens at item boundaries; a folder that is being deleted finishes or fails on its own. Queue rows read “Delete {"{n}"} items permanently”.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <XCell w={520} h={170} status={<YProgressStatus pct={0} text="Deleting 3 of 12 · mockup-exports" />} statusRight={<YStatusRight queued={1} />}><XDetail rows={undefined} /></XCell>
          )} />
        </Stage>
        <Meta
          specs={[["label", "“Deleting {i} of {n} · {name}” · top-level items"], ["bar", "track only · no fill · no percent"], ["bytes", "not shown"], ["cancel ×", "tooltip “Stop after this item” · stops at the next item boundary"], ["queue row", "“Delete {n} items permanently” · _one"]]}
          tokens={[{ tok: "--tasty-explorer-progress-track", use: "track only", color: "var(--tasty-explorer-progress-track)" }]} />
        <Note>i18n: <code>explorer.op.deleting</code> “Deleting {"{i}"} of {"{n}"} · {"{file}"}” / “삭제 중 {"{i}"}/{"{n}"} · {"{file}"}” / “削除中 {"{i}"}/{"{n}"} · {"{file}"}” · <code>explorer.op.stop_after_item</code> “Stop after this item” / “이 항목까지만 하고 멈춤” / “この項目の後で停止” · <code>explorer.queue.delete</code> “Delete {"{n}"} items permanently” + <code>_one</code>.</Note>
      </Spec>

      <Spec id="results-done-stopped-partial-folder-partly-deleted-failed-retry" title="Results — done · stopped · partial · folder partly deleted · failed · retry"
        when={<>The existing result card, with <b>no Undo anywhere</b> and nothing added to the Undo / Redo history. A partial result says what <b>was</b> deleted and lists what is <b>left</b> (≤ 3 paths + “and n more”, Copy paths). A folder that failed part-way is listed as left, with the reason “Folder kept; some of its contents were deleted”, so the card never says nothing was deleted when something was. <b>Retry</b> appears when items are left; it opens the <b>same confirm</b> again for those items only, with the same warning. Stopped tells how many are gone and that they can't be brought back.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="done — in place" w={480}>
            <XCell w={480} h={210} overlay={<YToastAt><YResult variant="success" title="Deleted 12 items permanently" /></YToastAt>}><XDetail /></XCell>
          </XCol>
          <XThemes gap="var(--tasty-space-md)" render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)", maxWidth: "100%" }}>
              <YResult variant="success" title="Deleted “report.pdf” permanently" />
              <YResult variant="info" title="Stopped · 5 of 12 deleted. Deleted items can't be restored." />
              <YResult variant="warning" title="Deleted 9 of 12 · 3 left"
                lines={[["~/work/tasty/target/build", "Folder kept; some of its contents were deleted"], ["~/Downloads/locked.db", "In use by another app"], ["/usr/share/doc/readme", "Permission denied"]]}
                actions={<><Button variant="secondary" size="sm">Retry 3…</Button><Button variant="ghost" ground="raised" size="sm">Copy paths</Button></>} />
              <YResult variant="danger" title="Couldn't delete 1 item"
                lines={[["~/work/tasty/node_modules", "Folder kept; some of its contents were deleted"]]}
                actions={<><Button variant="secondary" size="sm">Retry…</Button><Button variant="ghost" ground="raised" size="sm">Copy paths</Button></>} />
              <YResult variant="danger" title="Couldn't delete 2 items · nothing was deleted" lines={[["~/Downloads/a.iso", "Permission denied"], ["~/Downloads/b.iso", "Permission denied"]]}
                actions={<><Button variant="secondary" size="sm">Retry 2…</Button><Button variant="ghost" ground="raised" size="sm">Copy paths</Button></>} />
            </div>
          )} />
        </Stage>
        <Meta
          specs={[["done", "success · “Deleted {n} items permanently” / _one “Deleted “{name}” permanently” · standard time · no Undo"], ["stopped", "info · “Stopped · {i} of {n} deleted. Deleted items can't be restored.”"], ["partial", "warning · stays · “Deleted {i} of {n} · {k} left” · list of what is left · Retry {k}… · Copy paths"], ["folder part-way", "listed as left · “Folder kept; some of its contents were deleted”"], ["failed", "danger · “Couldn't delete {n} items” · “· nothing was deleted” only when the job touched nothing"], ["Retry", "label ends in “…” · reopens the confirm for the left items"], ["history", "no Undo / Redo entry for any outcome"]]}
          tokens={[{ tok: "--tasty-toast-accent-success", use: "done", color: "var(--tasty-toast-accent-success)" }, { tok: "--tasty-toast-accent-warning", use: "partial", color: "var(--tasty-toast-accent-warning)" }, { tok: "--tasty-toast-accent-danger", use: "failed", color: "var(--tasty-toast-accent-danger)" }]} />
        <Note>i18n: <code>explorer.result.deleted</code> “Deleted {"{n}"} items permanently” / “항목 {"{n}"}개를 영구 삭제함” / “{"{n}"} 個の項目を完全に削除しました” + <code>_one</code> · <code>explorer.result.delete_stopped</code> “Stopped · {"{i}"} of {"{n}"} deleted. Deleted items can't be restored.” / “멈춤 · {"{n}"}개 중 {"{i}"}개 삭제됨. 삭제된 항목은 되돌릴 수 없습니다.” / “停止しました · {"{n}"} 個中 {"{i}"} 個を削除。削除した項目は元に戻せません。” · <code>explorer.result.delete_partial</code> “Deleted {"{i}"} of {"{n}"} · {"{k}"} left” / “{"{n}"}개 중 {"{i}"}개 삭제 · {"{k}"}개 남음” / “{"{n}"} 個中 {"{i}"} 個を削除 · {"{k}"} 個が残っています” · <code>explorer.result.delete_failed</code> “Couldn't delete {"{n}"} items” + <code>_one</code> / “항목 {"{n}"}개를 삭제하지 못함” / “{"{n}"} 個の項目を削除できませんでした” · <code>explorer.result.nothing_deleted</code> · <code>explorer.result.folder_partly_deleted</code> “Folder kept; some of its contents were deleted” / “폴더는 남았지만 안쪽 일부는 삭제됨” / “フォルダーは残りましたが、中身の一部は削除されました” · <code>explorer.result.changed_skipped</code> “Changed since you confirmed — skipped” / “확인한 뒤 바뀌어 건너뜀” / “確認後に変更されたためスキップ” · <code>explorer.result.source_closed</code>.</Note>
      </Spec>
    </Section>
  );
}

export { PermanentDeleteSection };
