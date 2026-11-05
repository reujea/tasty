import React from "react";
import * as YDS from "../../components/index.js";
import { FollowupsB11Section as ExplorerOpsB11_FollowupsB11Section } from "./results-and-refusals.jsx";
import { RemainingSection as ExplorerOpsB11_RemainingSection } from "./remaining-states.jsx";
import { FollowupsB12Section as ExplorerOpsB12_FollowupsB12Section } from "./address-and-toolbar.jsx";
import { RemainingB12Section as ExplorerOpsB12_RemainingB12Section } from "./properties-and-preview.jsx";
import { PermanentDeleteSection as ExplorerOpsDelete_PermanentDeleteSection } from "./delete-permanently.jsx";
import { XCol, XThemes, XToolbar, XCell, XDetail, XROWS, XListRow, XHi, XFindBar, XListEdit, CreateSection, DragSection, SearchSection } from "./create-drag-search.jsx";
import { PropsActionsSection as ExplorerOpsPropsActions_PropsActionsSection } from "./properties-actions.jsx";
import { RemoteWriteSection as ExplorerOpsRemote_RemoteWriteSection } from "./remote-writes.jsx";
import { StagingLeftSection as ExplorerOpsStaging_StagingLeftSection } from "./staging-left.jsx";
import * as YK from "../parts/explorer-kit.jsx";
import { Section as YSection, Spec as YSpec, Stage as YStage, Meta as YMeta, Note as YNote } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer file operations (2026-10-09, batch 9), part 2:
// Progress / conflicts / results · Properties / preview / thumbnails · page mount.


const { IconButton: YIconButton, Button: YButton, Spinner: YSpinner, Checkbox: YCheckbox, Icon: YIcon, Toast: YToast, Tag: YTag, MenuItem: YMenuItem } = YDS;



const NAV = [
  { id: "create", label: "Create · commands" },
  { id: "drag", label: "Drag & drop" },
  { id: "progress", label: "Progress · conflicts · results" },
  { id: "properties", label: "Properties · preview" },
  { id: "search", label: "Filter · search" },
  { id: "followups", label: "Follow-ups (b10)" },
  { id: "followups11", label: "Follow-ups (b11)" },
  { id: "remaining", label: "Remaining states (b11)" },
  { id: "followups12", label: "Follow-ups (b12)" },
  { id: "remaining12", label: "Remaining UI (b12)" },
  { id: "remotewrite", label: "Remote write" },
  { id: "permdelete", label: "Delete permanently" },
  { id: "stagingleft", label: "Temporary folder left" },
  { id: "propsactions", label: "Properties actions" },
];

// ── Progress ──────────────────────────────────────────────
function YProgressStatus({ pct = 30, text, bytes, queued, waiting }) {
  return (
    <>
      <span style={{ position: "absolute", left: 0, right: 0, top: "calc(-1 * var(--tasty-border-width))", height: "var(--tasty-explorer-progress-height)", background: "var(--tasty-explorer-progress-track)" }}>
        <span style={{ display: "block", width: pct + "%", height: "100%", background: waiting ? "var(--tasty-accent-warning)" : "var(--tasty-explorer-progress-fill)" }} />
      </span>
      <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", color: waiting ? "var(--tasty-accent-warning-raised)" : "var(--tasty-text-secondary)" }}>{text}</span>
    </>
  );
}
function YStatusRight({ bytes, queued, waiting }) {
  return (
    <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none" }}>
      {bytes && <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{bytes}</span>}
      {queued ? <YTag>+{queued} queued</YTag> : null}
      {waiting ? <YButton variant="ghost" size="sm">Show</YButton> : <YIconButton size="sm" aria-label="Cancel" title="Cancel">{YK.ic.x}</YIconButton>}
    </span>
  );
}
function YQueue() {
  const row = (title, sub, run) => (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minHeight: "var(--tasty-menu-item-height)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-text-muted)" }}>{run ? <YSpinner size="var(--tasty-icon-size-sm)" /> : <YIcon name="layers" size="var(--tasty-icon-size-sm)" />}</span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{title}</span>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{sub}</span>
      </span>
      <YIconButton size="sm" aria-label={run ? "Cancel" : "Remove from queue"} title={run ? "Cancel" : "Remove from queue"}>{YK.ic.x}</YIconButton>
    </div>
  );
  return (
    <div style={{ position: "absolute", right: "var(--tasty-space-sm)", bottom: "calc(var(--tasty-status-bar-height) + var(--tasty-space-xs))", width: "var(--tasty-size-288)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", boxShadow: "var(--tasty-shadow-popover)", padding: "var(--tasty-space-xs)" }}>
      {row("Copy 40 items to Documents", "12 of 40 · 1.2 / 3.4 GB", true)}
      {row("Move 3 items to Archive", "Queued", false)}
    </div>
  );
}
function YConflict({ many, folder }) {
  const meta = (k, v) => (
    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-size-64) 1fr", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)" }}>
      <span style={{ color: "var(--tasty-text-muted)" }}>{k}</span><span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{v}</span>
    </div>
  );
  return (
    <div style={{ width: "var(--tasty-explorer-conflict-width)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ padding: "var(--tasty-space-md) var(--tasty-size-14) var(--tasty-space-sm)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        <div style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{folder ? "A folder named “assets” already exists" : "“report.pdf” already exists"}</div>
        <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>in ~/Documents</div>
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {meta("Existing", folder ? "folder · 214 items" : "2.4 MB · 2026-06-24 09:12")}
          {meta("Incoming", folder ? "folder · 37 items" : "2.6 MB · 2026-10-09 10:02")}
        </div>
        {folder && <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Folders are not merged.</div>}
        {many && <YCheckbox label="Do this for the other 3 conflicts" />}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "0 var(--tasty-size-14) var(--tasty-space-md)" }}>
        <YButton variant="ghost" size="sm">Cancel the rest</YButton>
        <span style={{ flex: 1 }} />
        <YButton variant="secondary" size="sm">Skip</YButton>
        {!folder && <YButton variant="secondary" size="sm">Replace</YButton>}
        <YButton variant="primary" size="sm">Keep both</YButton>
      </div>
    </div>
  );
}
function YToastAt({ children }) {
  return <div style={{ position: "absolute", right: "var(--tasty-space-sm)", bottom: "calc(var(--tasty-status-bar-height) + var(--tasty-space-sm))", maxWidth: "calc(100% - 2 * var(--tasty-space-sm))" }}>{children}</div>;
}
function YResult({ variant, title, lines, more, actions }) {
  return (
    <YToast variant={variant}>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)" }}>
          <span style={{ flex: 1, minWidth: 0 }}>{title}</span>
          <YIconButton size="sm" aria-label="Dismiss" title="Dismiss">{YK.ic.x}</YIconButton>
        </div>
        {lines && <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {lines.map(([p, why]) => (
            <div key={p} style={{ display: "flex", flexDirection: "column", fontSize: "var(--tasty-font-size-caption)" }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: "rtl", textAlign: "left" }}>{p}</span>
              <span style={{ color: "var(--tasty-text-muted)" }}>{why}</span>
            </div>
          ))}
          {more ? <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>and {more} more</span> : null}
        </div>}
        {actions && <div style={{ display: "flex", gap: "var(--tasty-space-xs)" }}>{actions}</div>}
      </div>
    </YToast>
  );
}

function ProgressSection() {
  return (
    <YSection id="progress" title="Progress · conflicts · cancel · results · undo">
      <YSpec id="running-on-the-explorer-s-own-status-line-queue" title="Running — on the explorer's own status line · queue"
        when={<>Work shows in the <b>status line of the explorer that started it</b>, so two explorers in different cells each show only their own. A 2px bar runs along the top edge of the status line (determinate by bytes; no animation), the left side reads <b>“Copying 12 of 40 · {"{current file}"}”</b>, the right side has the byte count, a <b>“+n queued”</b> Tag when more work waits in this explorer, and a <b>Cancel</b> ×. Clicking the left side or the Tag opens the <b>queue</b> popover above the status line: the running job (Cancel) and queued jobs (remove). One job runs at a time per app; a job queued in another explorer shows only there. Nothing takes keyboard focus.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={(l) => (
            <XCell w={520} h={l === "Mocha" ? 280 : 200} status={<YProgressStatus pct={34} text="Copying 12 of 40 · report-final-v3-signed.pdf" />} statusRight={<YStatusRight bytes="1.2 / 3.4 GB" queued={1} />} overlay={l === "Mocha" ? <YQueue /> : null}>
              <XDetail />
            </XCell>
          )} />
        </YStage>
        <YMeta
          specs={[["where", "status line of the explorer that started the job (never a window-wide toast)"], ["bar", "explorer-progress-height 2 · top edge of the status line · track / fill · width = bytes done"], ["label", "“{Copying|Moving|Moving to Trash} {i} of {n} · {file}” · caption · ellipsis"], ["bytes", "mono · “1.2 / 3.4 GB” · unknown size → item count only, bar full-width track with no fill"], ["queued", "Tag “+n queued” · opens the queue popover"], ["queue", "menu surface · 288 · above the status line · Spinner row = running (×) · layers row = queued (×)"], ["cancel", "× on the status line · stops items not yet done"]]}
          tokens={[{ tok: "--tasty-explorer-progress-height", use: "→ size-2" }, { tok: "--tasty-explorer-progress-track", use: "→ surface-raised", color: "var(--tasty-explorer-progress-track)" }, { tok: "--tasty-explorer-progress-fill", use: "→ accent-primary", color: "var(--tasty-explorer-progress-fill)" }, { tok: "--tasty-status-bar-height", use: "24" }]} />
      </YSpec>

      <YSpec id="name-conflict-surface-scoped-prompt-apply-to-all-waiting" title="Name conflict — surface-scoped prompt · apply to all · waiting"
        when={<>A conflict <b>pauses</b> the job and asks with a <b>popup scoped to that explorer cell</b> (dims only the cell, like the markdown large-file confirm). It opens only while the explorer has focus; if the user is typing in another surface the status line turns warning-toned — <b>“Waiting for your answer · 1 name conflict”</b> with a <b>Show</b> button — and the popup opens when they come back. Answers: <b>Keep both</b> (primary, the default on <span className="ic">↵</span>; the app names the copy), <b>Skip</b>, <b>Replace</b> (files only, never the default) and <b>Cancel the rest</b> (ghost, left; <span className="ic">Esc</span>). With more conflicts left, a checkbox applies the answer to all of them. A folder colliding with a folder (or a file with a folder) offers only Skip / Keep both — no merge, no replace.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="file conflict · 4 left · apply to all" w={520}>
            <XCell w={520} h={300} status={<YProgressStatus pct={50} text="Copying 20 of 40 · paused" />} statusRight={<YStatusRight bytes="1.7 / 3.4 GB" />}
              overlay={<div style={{ position: "absolute", inset: "44px 0 0 0", background: "var(--tasty-scrim-bg)", display: "flex", alignItems: "center", justifyContent: "center", padding: "var(--tasty-space-md)" }}><YConflict many /></div>}>
              <XDetail />
            </XCell>
          </XCol>
          <XCol label="folder conflict — no Replace" w={420}>
            <YConflict folder />
          </XCol>
          <XCol label="waiting — the user is in another surface" w={520}>
            <XThemes render={() => (
              <XCell w={480} h={150} status={<YProgressStatus pct={50} waiting text="Waiting for your answer · 1 name conflict" />} statusRight={<YStatusRight waiting />}>
                <XDetail rows={XROWS.slice(0, 2)} />
              </XCell>
            )} />
          </XCol>
        </YStage>
        <YMeta
          specs={[["scope", "popup clamped to the explorer cell · scrim-bg over the body · shadow-modal"], ["width", "explorer-conflict-width 400"], ["title", "14 semibold · “{name}” already exists · caption “in {folder}”"], ["compare", "Existing / Incoming · size · modified · mono caption"], ["buttons", "Cancel the rest (ghost, left) · Skip · Replace (files only) · Keep both (primary, ↵)"], ["apply to all", "Checkbox, only when more conflicts remain"], ["focus", "opens only when the explorer has focus · otherwise the status line waits (accent-warning) + Show"], ["overwrite", "offered for files from v1 · always an explicit choice"]]}
          tokens={[{ tok: "--tasty-explorer-conflict-width", use: "→ size-400" }, { tok: "--tasty-scrim-bg", use: "cell dim" }, { tok: "--tasty-accent-warning", use: "waiting line + bar", color: "var(--tasty-accent-warning-raised)" }]} />
      </YSpec>

      <YSpec id="results-done-cancelled-partial-retry-failed-trash-unavailable-un" title="Results — done · cancelled · partial + retry · failed · trash unavailable · undo"
        when={<>Results appear as a <b>Toast anchored inside the explorer cell</b>, bottom-right, 8 above the status line (the cell's toasts stack upward). <b>Done</b> and <b>cancelled</b> leave after the standard toast time; anything with a failure <b>stays until dismissed</b>. A partial or failed result lists up to <b>3 failed paths</b> (path ellipsised at the front so the name stays, reason under it) and then “and n more”; <b>Copy paths</b> copies every failed path, <b>Retry</b> runs only the failed and skipped items. A move across disks whose source delete failed is its own line (“copied; original not removed”). Trash never falls back to permanent delete. <b>Undo</b> sits on the done toast for as long as it is visible; its own result reports what could not be undone.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="done — in place" w={480}>
            <XCell w={480} h={210} overlay={<YToastAt><YResult variant="success" title="Copied 40 items to Documents" actions={<YButton variant="ghost" ground="raised" size="sm">Undo</YButton>} /></YToastAt>}><XDetail /></XCell>
          </XCol>
          <XThemes gap="var(--tasty-space-md)" render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)" }}>
              <YResult variant="info" title="Copy cancelled · 12 of 40 done" />
              <YResult variant="warning" title="Moved 37 of 40 · 3 failed"
                lines={[["~/Downloads/build/artifacts/app-release-unsigned.apk", "Permission denied"], ["~/Downloads/archive.zip", "Copied; original not removed (in use)"], ["~/Downloads/raw/IMG_2031.heic", "No space left on device"]]}
                actions={<><YButton variant="secondary" size="sm">Retry 3</YButton><YButton variant="ghost" ground="raised" size="sm">Copy paths</YButton></>} />
              <YResult variant="danger" title="Couldn't copy 12 items" lines={[["~/Downloads/raw/IMG_2030.heic", "No space left on device"]]} more={11}
                actions={<><YButton variant="secondary" size="sm">Retry 12</YButton><YButton variant="ghost" ground="raised" size="sm">Copy paths</YButton></>} />
              <YResult variant="danger" title="Trash isn't available on this drive. Nothing was deleted." />
              <YResult variant="warning" title="Undid move · 2 items couldn't be put back" lines={[["~/Documents/report.pdf", "A newer file is there now"]]} more={1} />
            </div>
          )} />
        </YStage>
        <YMeta
          specs={[["placement", "Toast inside the cell · bottom-right · status-bar-height + 8 · stack upward"], ["done", "success · “{Copied|Moved} {n} items to {folder}” · Undo (ghost) · standard time"], ["cancelled", "info · “{Copy} cancelled · {i} of {n} done” · standard time · partial file removed by the app"], ["partial", "warning · stays · ≤ 3 paths + “and n more” · Retry n · Copy paths"], ["failed", "danger · stays · same list and actions"], ["path", "mono caption · ellipsis at the front · reason under it (muted)"], ["trash unavailable", "danger · “Trash isn't available on this drive. Nothing was deleted.” · no fallback"], ["undo", "on the done toast while it shows · result toast lists what could not be undone"], ["dismiss", "× on every result"]]}
          tokens={[{ tok: "--tasty-toast-accent-success", use: "done", color: "var(--tasty-toast-accent-success)" }, { tok: "--tasty-toast-accent-warning", use: "partial / undo partial", color: "var(--tasty-toast-accent-warning)" }, { tok: "--tasty-toast-accent-danger", use: "failed / trash", color: "var(--tasty-toast-accent-danger)" }, { tok: "--tasty-toast-max-width", use: "320" }]} />
        <YNote>i18n: <code>explorer.op.copying</code> / <code>moving</code> / <code>trashing</code> “{"{verb}"} {"{i}"} of {"{n}"} · {"{file}"}” · <code>explorer.op.queued</code> “+{"{n}"} queued” · <code>explorer.op.waiting</code> “Waiting for your answer · {"{n}"} name conflict(s)” · <code>explorer.conflict.*</code> (title, in, existing, incoming, no_merge, apply_all, keep_both, skip, replace, cancel_rest) · <code>explorer.result.*</code> (done, cancelled, partial, failed, source_not_removed, more, retry, copy_paths, undo, undo_partial) · <code>explorer.trash.unavailable</code>.</YNote>
      </YSpec>
    </YSection>
  );
}

// ── Properties · preview · thumbnails ─────────────────────
function YField({ k, v, mono, copy, spin }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-explorer-props-label-width) 1fr", gap: "var(--tasty-space-sm)", alignItems: "start", minHeight: "var(--tasty-explorer-props-row-min-height)", paddingTop: "var(--tasty-explorer-props-row-pad-top)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-explorer-props-row-line)" }}>{k}</span>
      <span style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
        {spin && <YSpinner size="var(--tasty-icon-size-sm)" />}
        <span style={{ flex: 1, minWidth: 0, fontFamily: mono ? "var(--tasty-font-mono)" : "inherit", fontSize: mono ? "var(--tasty-font-size-caption)" : "var(--tasty-font-size-body)", lineHeight: "var(--tasty-explorer-props-row-line)", color: "var(--tasty-text-secondary)", overflowWrap: "anywhere" }}>{v}</span>
        {copy && <YIconButton size="sm" aria-label="Copy" title="Copy"><YIcon name="copy" /></YIconButton>}
      </span>
    </div>
  );
}
function YProps({ glyph = YK.ic.file, glyphColor, name, children, note }) {
  return (
    <div style={{ width: "var(--tasty-explorer-props-width)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-explorer-props-padding-x) var(--tasty-space-sm)" }}>
        <span style={{ display: "inline-flex", flex: "none", color: glyphColor || "var(--tasty-text-muted)" }}>{glyph}</span>
        <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-normal)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
        <YIconButton size="sm" aria-label="Close" title="Close">{YK.ic.x}</YIconButton>
      </div>
      <div style={{ padding: "0 var(--tasty-explorer-props-padding-x) var(--tasty-space-md)", display: "flex", flexDirection: "column" }}>
        {children}
        {note && <div style={{ marginTop: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{note}</div>}
      </div>
    </div>
  );
}
function YPreview({ kind }) {
  const head = { multi: ["3 items", "2 files, 1 folder"], pixels: ["scan-poster.tif", "TIFF image · 20000 × 14000 · 61 MB"], text: ["notes.md", "Markdown · 12 KB"], image: ["diagram.png", "PNG image · 1280 × 720 · 488 KB"], none: ["archive.zip", "Archive · 64 MB"], loading: ["notes.md", "Markdown · 12 KB"], large: ["server.log", "Log · 38 MB"], error: ["private.key", "File · 3 KB"] }[kind];
  let body;
  if (kind === "text") body = (
    <div style={{ flex: 1, padding: "var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)", whiteSpace: "pre", overflow: "hidden", background: "var(--tasty-bg-sidebar)" }}>{"# Notes\n\n- split floor 180\n- favorites pin 240\n- drag: move same disk\n\n## Open\n- thumbnails in Grid\n- preview panel back"}</div>
  );
  else if (kind === "image") body = (
    <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-sidebar)" }}>
      <div style={{ width: "100%", aspectRatio: "16 / 9", display: "flex", alignItems: "center", justifyContent: "center", border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius-sm)", background: "var(--tasty-surface-raised)", color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)" }}>image · fit</div>
    </div>
  );
  else {
    const st = {
      none: [YK.ic.file, null, "No preview for this file type"],
      loading: [<YSpinner />, null, "Loading preview…"],
      large: [YK.ic.file, null, "Too large to preview", "Over 1 MiB."],
      pixels: [YK.ic.file, null, "Too large to preview", "Over 16384 px on a side, or needs more than 256 MiB to decode.", "20000 × 14000 px"],
      multi: [<YIcon name="layers" />, null, "3 items selected", "Select one file to preview it."],
      error: [<YIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />, "var(--tasty-explorer-error-fg)", "Can't read this file", null, "Permission denied (os error 13)"],
    }[kind];
    body = <div style={{ flex: 1, display: "flex", padding: "var(--tasty-space-sm)", background: "var(--tasty-bg-sidebar)" }}><YK.ExpState glyph={st[0]} glyphColor={st[1]} title={st[2]} sub={st[3]} reason={st[4]} bg="var(--tasty-bg-sidebar)" /></div>;
  }
  return (
    <div style={{ width: "var(--tasty-explorer-preview-width)", flex: "none", display: "flex", flexDirection: "column", borderLeft: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
      <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", height: "var(--tasty-explorer-preview-header-height)", flex: "none", padding: "0 var(--tasty-space-sm)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{head[0]}</span>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{head[1]}</span>
      </div>
      {body}
    </div>
  );
}
function YThumbCell({ name, kind, state }) {
  const slot = kind === "thumb"
    ? <span style={{ width: "var(--tasty-explorer-grid-thumb-size)", height: "var(--tasty-explorer-grid-thumb-size)", boxSizing: "border-box", border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius-sm)", background: "var(--tasty-surface-hover)", display: "flex", alignItems: "flex-end", justifyContent: "center", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>img</span>
    : <span style={{ display: "inline-flex", color: kind === "loading" ? "var(--tasty-accent-info-raised)" : "var(--tasty-text-muted)" }}>{kind === "folder" ? YK.ic.folder : kind === "loading" ? YK.ic.image : YK.ic.file}</span>;
  return (
    <div style={{ width: 80, display: "flex", flexDirection: "column", alignItems: "center", gap: 4, padding: "8px 4px", borderRadius: "var(--tasty-radius)", background: state === "selected" ? "var(--tasty-surface-active)" : "transparent" }}>
      <span style={{ height: "var(--tasty-explorer-grid-thumb-size)", display: "flex", alignItems: "center", justifyContent: "center" }}>{slot}</span>
      <span style={{ fontSize: 11, textAlign: "center", lineHeight: "14px", height: 42, color: "var(--tasty-text-secondary)", wordBreak: "break-word", overflow: "hidden" }}>{name}</span>
    </div>
  );
}

function PropertiesSection() {
  return (
    <YSection id="properties" title="Properties · preview panel · Grid thumbnails">
      <YSpec id="properties-popup-for-one-item-folder-several-remote" title="Properties — popup for one item · folder · several · remote"
        when={<>Opened from the <b>Properties</b> context-menu row or the app's properties action, as a popup scoped to the explorer cell (same family as Rename; no scrim needed, <span className="ic">Esc</span> or × closes). It shows the item it was <b>opened for</b> and does not follow later selection changes. Fields are a two-column list: caption label (96) and value; paths and permissions are mono and wrap anywhere, with a Copy button on Location and Link target. Sizes that take time (folders, several items) show a Spinner and fill in when the background count ends; the popup never blocks the list. A <b>remote</b> item shows only what the host lists (name, kind, size, modified, location) and says so.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XThemes render={() => (
            <YProps glyph={YK.ic.image} name="diagram.png">
              <YField k="Kind" v="PNG image" />
              <YField k="Size" v="488 KB (499,712 bytes)" />
              <YField k="Modified" v="2026-06-26 18:05" mono />
              <YField k="Created" v="2026-06-26 17:58" mono />
              <YField k="Location" v="~/Downloads" mono copy />
              <YField k="Permissions" v="rw-r--r-- · read-only: no" mono />
            </YProps>
          )} />
          <XCol label="folder — size still counting" w={360}>
            <YProps glyph={YK.ic.folder} name="mockup-exports">
              <YField k="Kind" v="Folder" />
              <YField k="Size" v="Counting… 1,204 items · 1.1 GB" spin />
              <YField k="Modified" v="2026-06-20 14:30" mono />
              <YField k="Location" v="~/Downloads" mono copy />
            </YProps>
          </XCol>
          <XCol label="symlink" w={360}>
            <YProps glyph={<YIcon name="link" />} name="current">
              <YField k="Kind" v="Symbolic link" />
              <YField k="Link target" v="~/work/tasty/target/release/tasty" mono copy />
              <YField k="Location" v="~/bin" mono copy />
            </YProps>
          </XCol>
          <XCol label="several items" w={360}>
            <YProps glyph={<YIcon name="layers" />} name="3 items">
              <YField k="Kinds" v="2 files, 1 folder" />
              <YField k="Total size" v="66.4 MB" />
              <YField k="Location" v="~/Downloads" mono copy />
            </YProps>
          </XCol>
          <XCol label="remote item" w={360}>
            <YProps name="build-0412.log" note="Remote — only what the remote host lists is shown.">
              <YField k="Kind" v="File" />
              <YField k="Size" v="2.1 MB" />
              <YField k="Modified" v="2026-10-09 08:12" mono />
              <YField k="Location" v="build-eu:~/logs" mono copy />
            </YProps>
          </XCol>
        </YStage>
        <YMeta
          specs={[["open", "context menu Properties (every shape) · keybinding action explorer.properties"], ["popup", "explorer-props-width 360 · scoped to the cell · shadow-modal · Esc / ×"], ["title", "glyph + name · 14 semibold · ellipsis"], ["fields", "label explorer-props-label-width 96 · caption muted · value body 13 (mono caption for paths / dates / mode) · wrap anywhere"], ["local file", "Kind · Size (with bytes) · Modified · Created · Location · Permissions"], ["folder", "Size counts in the background (Spinner) · item count"], ["symlink", "Link target + Copy"], ["several", "“{n} items” · Kinds · Total size · Location (common parent)"], ["remote", "Kind · Size · Modified · Location · muted note"], ["selection change", "popup keeps its item"]]}
          tokens={[{ tok: "--tasty-explorer-props-width", use: "→ size-360" }, { tok: "--tasty-explorer-props-label-width", use: "→ size-96" }, { tok: "--tasty-shadow-modal", use: "popup" }, { tok: "--tasty-spinner-indicator", use: "counting", color: "var(--tasty-spinner-indicator)" }]} />
      </YSpec>

      <YSpec id="preview-panel-text-image-other-loading-too-large-unreadable" title="Preview panel — text · image · other · loading · too large · unreadable"
        when={<>The preview panel <b>comes back as a toggle</b> (toolbar view group). It docks on the right of the listing at <span className="tok">--tasty-explorer-preview-width</span> (288), resizable by a splitter between 200 and 460; the width is remembered per explorer. A 40 header shows the name and “kind · size” (image: pixel size). <b>Text</b> files (UTF-8, by extension or content) show their first 64 KB in mono; <b>images</b> fit the panel; anything else, folders included, shows “No preview for this file type” and leaves the facts to the header. A new selection clears the old preview at once and shows <b>Loading preview…</b> until the read ends. Over 1 MB or unreadable shows a state, never content. Remote explorers preview text and images the host returns.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="text" w={640}>
            <XCell w={640} h={250} toolbar={<XToolbar preview />} status="5 items · 1 selected"><XDetail states={{ 4: "selected" }} /><YPreview kind="text" /></XCell>
          </XCol>
          <XCol label="image" w={640}>
            <XCell w={640} h={250} toolbar={<XToolbar preview />} status="5 items · 1 selected"><XDetail states={{ 3: "selected" }} /><YPreview kind="image" /></XCell>
          </XCol>
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
            {["none", "loading", "large", "error"].map((k) => (
              <XCol key={k} label={k === "none" ? "not supported" : k === "large" ? "too large" : k === "error" ? "unreadable" : "loading"}>
                <div style={{ height: 230, display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><YPreview kind={k} /></div>
              </XCol>
            ))}
          </div>
        </YStage>
        <YMeta
          specs={[["toggle", "toolbar view group · columns glyph · keybinding action explorer.toggle_preview"], ["panel", "right · explorer-preview-width 288 · splitter 200 … 460 · 1px separator · remembered per explorer"], ["header", "40 · name (body, ellipsis) · kind · size (caption muted)"], ["text", "first 64 KB · mono caption · line-height-ui · on bg-sidebar · no wrap"], ["image", "fit, never upscaled · on bg-sidebar"], ["other / folder", "“No preview for this file type”"], ["loading", "Spinner · “Loading preview…” — the old preview is cleared first"], ["too large", "> 1 MB (app limit) · “Too large to preview”"], ["unreadable", "error tone · OS reason (mono)"], ["narrow cell", "panel hides itself below preview-min 200 + explorer-list-min-width 200 = 400; toggle stays on"]]}
          tokens={[{ tok: "--tasty-explorer-preview-width", use: "→ size-288" }, { tok: "--tasty-explorer-preview-min-width", use: "→ size-200" }, { tok: "--tasty-explorer-preview-max-width", use: "→ size-460" }, { tok: "--tasty-explorer-list-min-width", use: "→ size-200 (b10)" }, { tok: "--tasty-explorer-preview-header-height", use: "→ size-40 (b10)" }, { tok: "--tasty-bg-sidebar", use: "preview bed", color: "var(--tasty-bg-sidebar)" }]} />
      </YSpec>

      <YSpec id="grid-thumbnails-40-slot-for-every-cell" title="Grid thumbnails — 40 slot for every cell"
        when={<>Thumbnails are <b>Grid only</b> (List and Detail keep their glyphs). Every Grid cell's icon slot becomes <span className="tok">--tasty-explorer-grid-thumb-size</span> (40), so rows stay uniform whether or not a folder holds images; folders and other files keep the 16 glyph centred in it. An image file shows its glyph in accent-info <b>while the thumbnail loads</b>, then the picture fit into 40 × 40 with a 1px separator edge and radius-sm. Thumbnails are made in the background, cached, and skipped over the app's size limit (glyph stays). Remote explorers keep glyphs. Cell width stays 80; the cell grows by 24 in height (16 → 40 slot).</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <div style={{ display: "flex", gap: 4, flexWrap: "wrap", width: 340, padding: 8, background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
              <YThumbCell name="mockup-exports" kind="folder" />
              <YThumbCell name="diagram.png" kind="thumb" state="selected" />
              <YThumbCell name="screenshot-2026-10-09.png" kind="thumb" />
              <YThumbCell name="IMG_2031.heic" kind="loading" />
              <YThumbCell name="notes.md" kind="file" />
            </div>
          )} />
        </YStage>
        <YMeta
          specs={[["scope", "Grid only · local only"], ["slot", "explorer-grid-thumb-size 40 · every cell"], ["thumbnail", "fit 40 × 40 · 1px separator · radius-sm"], ["loading / over limit", "image glyph 16 · accent-info"], ["cell", "80 wide (unchanged) · +24 tall"]]}
          tokens={[{ tok: "--tasty-explorer-grid-thumb-size", use: "→ size-40" }, { tok: "--tasty-accent-info", use: "image glyph fallback", color: "var(--tasty-accent-info-raised)" }, { tok: "--tasty-separator", use: "thumb edge", color: "var(--tasty-separator)" }]} />
        <YNote>“img” stands in for the decoded picture. Real side: <code>grid_cell()</code> reserves the 40 slot for every entry; the thumbnail cache is keyed by path + mtime.</YNote>
      </YSpec>
    </YSection>
  );
}

// ── Follow-ups (2026-10-09 batch 10) ──────────────────────
function YGridHit({ name, q, glyph = YK.ic.file }) {
  return (
    <div style={{ width: 80, display: "flex", flexDirection: "column", alignItems: "center", gap: 4, padding: "8px 4px", borderRadius: "var(--tasty-radius)" }}>
      <span style={{ height: "var(--tasty-explorer-grid-thumb-size)", display: "flex", alignItems: "center", color: "var(--tasty-text-muted)" }}>{glyph}</span>
      <span style={{ fontSize: 11, textAlign: "center", lineHeight: "14px", color: "var(--tasty-text-secondary)", wordBreak: "break-word" }}><XHi text={name} q={q} /></span>
    </div>
  );
}
function FollowupsSection() {
  const menuBox = { width: "var(--tasty-size-200)", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", borderRadius: "var(--tasty-menu-radius)", padding: "var(--tasty-space-xs)", boxShadow: "var(--tasty-shadow-popover)" };
  return (
    <YSection id="followups" title="Follow-ups — find · names · results · properties">
      <YSpec id="find-every-view-highlights-while-filtering-no-matches-skipped-ca" title="Find — every view highlights · `..` while filtering · no matches · skipped · cap · More rows"
        when={<>The matched part takes <span className="tok">--tasty-explorer-match-fg</span> in <b>all three views</b> (Detail, List, Grid), filter and search alike. While <b>filtering</b> the <code>..</code> row stays at the top: it is navigation, never matched, never counted in “{"{shown}"} of {"{total}"}”; recursive results drop it (they replace the listing). A filter with <b>no matches</b> keeps the bar (“0 of 6”) and shows the state screen with a filter line — <b>“No names match “{"{query}"}””</b>, no sub-line. One unreadable folder reads <b>“1 folder skipped”</b>. A recursive search <b>stops at 5,000</b> results: the bar reads “5,000+ found · stopped”, the results stay, and a muted line asks for a longer query. Native menus have no check mark, so the <b>More</b> rows say the action: <b>Find / Close find</b>, <b>Show preview / Hide preview</b> — and Preview folds into More with the rest of the view group under 440.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="List — filter “re” · .. kept" w={360}>
            <XCell w={360} h={190} toolbar={<XToolbar search view="list" />} bar={<XFindBar value="re" right="2 of 6" />} status="2 of 6 items">
              <div style={{ flex: 1, padding: 4, display: "flex", flexDirection: "column" }}>
                <XListRow glyph={YK.ic.folder} name=".." />
                <XListRow glyph={YK.ic.file} name={<XHi text="report.pdf" q="re" />} state="selected" />
                <XListRow glyph={YK.ic.folder} name={<XHi text="mockup-exports" q="re" />} />
              </div>
            </XCell>
          </XCol>
          <XCol label="Grid — same highlight" w={360}>
            <XCell w={360} h={190} toolbar={<XToolbar search view="grid" />} bar={<XFindBar value="re" right="2 of 6" />}>
              <div style={{ flex: 1, display: "flex", gap: 4, padding: 8 }}>
                <YGridHit name=".." q="" glyph={YK.ic.folder} /><YGridHit name="report.pdf" q="re" /><YGridHit name="mockup-exports" q="re" glyph={YK.ic.folder} />
              </div>
            </XCell>
          </XCol>
          <XCol label="filter — no matches" w={360}>
            <XCell w={360} h={190} toolbar={<XToolbar search />} bar={<XFindBar value="rezzz" right="0 of 6" />}>
              <YK.ExpState glyph={<YIcon name="search" />} title="No names match “rezzz”" />
            </XCell>
          </XCol>
          <XCol label="search — cap reached · 1 skipped" w={520}>
            <XCell w={520} h={120} toolbar={<XToolbar search />} bar={<XFindBar value=".rs" deep right={<>5,000+ found · stopped · <span style={{ color: "var(--tasty-accent-warning-raised)" }}>1 folder skipped</span></>} />} status={<span style={{ color: "var(--tasty-text-muted)" }}>Showing the first 5,000. Type more to narrow the search.</span>}>
              <div style={{ flex: 1 }} />
            </XCell>
          </XCol>
          <XCol label="More menu (narrow) — action words, no checks">
            <div style={menuBox}>
              <YMenuItem icon={<YIcon name="folderPlus" />} label="New folder" />
              <YMenuItem icon={<YIcon name="filePlus" />} label="New file" />
              <YMenuItem icon={<YIcon name="search" />} label="Close find" />
              <YMenuItem icon={<YIcon name="columns" />} label="Show preview" />
            </div>
          </XCol>
        </YStage>
        <YMeta
          specs={[["highlight", "Detail · List · Grid — filter and search"], [".. row", "kept while filtering (not counted) · dropped from recursive results"], ["filter 0", "bar “0 of {total}” · search glyph · “No names match “{query}”” · no sub-line"], ["skipped", "explorer.find.skipped_one “1 folder skipped” · skipped “{n} folders skipped”"], ["cap", "5,000 results · “{n}+ found · stopped” · status line hint · results kept"], ["More rows", "Find ↔ Close find · Show preview ↔ Hide preview · no check marks on any OS"], ["narrow", "< 440: New folder · New file · Find · Preview all fold into More"]]}
          tokens={[{ tok: "--tasty-explorer-match-fg", use: "all views", color: "var(--tasty-explorer-match-fg)" }, { tok: "--tasty-explorer-toolbar-compact-below", use: "→ 440" }, { tok: "--tasty-accent-warning", use: "skipped", color: "var(--tasty-accent-warning-raised)" }]} />
        <YNote>i18n (new): <code>explorer.find.none_filter</code> “No names match “{"{query}"}”” · <code>explorer.find.skipped_one</code> “1 folder skipped” · <code>explorer.find.capped</code> “{"{n}"}+ found · stopped” · <code>explorer.find.capped_hint</code> “Showing the first {"{n}"}. Type more to narrow the search.” · <code>explorer.more.find_close</code> “Close find” · <code>explorer.more.preview_show</code> “Show preview” · <code>explorer.more.preview_hide</code> “Hide preview”.</YNote>
      </YSpec>

      <YSpec id="names-leading-trailing-spaces-are-refused" title="Names — leading / trailing spaces are refused"
        when={<>A new or renamed name that starts or ends with a space is <b>refused at the field</b>, never trimmed silently (the user may have meant it, and a silent change breaks what they typed). Spaces only = the existing empty error. On Windows a trailing <b>period</b> keeps the reserved-name message, since the OS would change the name the same way. The message box is capped by the new <span className="tok">--tasty-explorer-name-error-max-width</span>.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={() => (
            <div style={{ width: 300, height: 120, padding: 4, background: "var(--tasty-bg-panel)", borderRadius: "var(--tasty-radius)" }}>
              <XListEdit value="report " error="Names can't start or end with a space." />
            </div>
          )} />
        </YStage>
        <YMeta
          specs={[["leading / trailing space", "refused · explorer.name.edge_space"], ["only spaces", "explorer.name.empty"], ["Windows trailing .", "explorer.name.reserved (same copy)"], ["box", "explorer-name-error-max-width 240"]]}
          tokens={[{ tok: "--tasty-explorer-name-error-max-width", use: "→ size-240 (new)" }, { tok: "--tasty-explorer-name-error-fg", use: "border + edge", color: "var(--tasty-explorer-name-error-fg)" }]} />
      </YSpec>

      <YSpec id="results-originals-left-after-a-cross-disk-move-undo-kept-a-chang" title="Results — originals left after a cross-disk move · Undo kept a changed copy"
        when={<>A cross-disk move is copy, then delete. When the delete stops partway the copy is complete but the original may be <b>whole or partly</b> left. That result counts as a failure: <b>warning</b> tone, stays until dismissed, <b>no Undo</b> (the copy may be the only whole one). Undo of a copy never trashes a copy that changed after the job; it keeps it and lists it with its own reason.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes gap="var(--tasty-space-md)" render={() => (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "var(--tasty-toast-max-width)" }}>
              <YResult variant="warning" title="Moved 40 of 40 · 2 originals not fully removed"
                lines={[["~/Volumes/usb/photos/2026-09", "Copied; the original is still there, in whole or in part (Permission denied)"], ["~/Volumes/usb/archive.zip", "Copied; the original is still there, in whole or in part (in use)"]]}
                actions={<><YButton variant="secondary" size="sm">Retry 2</YButton><YButton variant="ghost" ground="raised" size="sm">Copy paths</YButton></>} />
              <YResult variant="warning" title="Undid copy · 1 item kept" lines={[["~/Documents/notes.md", "Changed after the copy, so it was kept"]]} />
            </div>
          )} />
        </YStage>
        <YMeta
          specs={[["source left", "warning · stays · no Undo · Retry n deletes the originals again · Copy paths"], ["title", "explorer.result.source_left_move"], ["line", "explorer.result.source_not_removed · reason in ( )"], ["undo kept", "explorer.result.changed_kept on the undo result line"]]}
          tokens={[{ tok: "--tasty-toast-accent-warning", use: "both", color: "var(--tasty-toast-accent-warning)" }]} />
        <YNote>en / ko / ja — <code>source_left_move</code> “Moved {"{done}"} of {"{total}"} · {"{n}"} originals not fully removed” / “{"{total}"}개 중 {"{done}"}개 이동 · 원본 {"{n}"}개가 다 지워지지 않음” / “{"{total}"} 件中 {"{done}"} 件を移動 · 元の {"{n}"} 件を削除しきれませんでした” · <code>source_not_removed</code> “Copied; the original is still there, in whole or in part ({"{reason}"})” / “복사됨. 원본이 전부 또는 일부 남아 있음 ({"{reason}"})” / “コピー済み。元の項目の全部または一部が残っています ({"{reason}"})” · <code>changed_kept</code> “Changed after the copy, so it was kept” / “복사 뒤 바뀌어서 남겨 둠” / “コピー後に変更されたため残しました”.</YNote>
      </YSpec>

      <YSpec id="properties-preview-can-t-read-several-selected-pixel-limit-link" title="Properties · preview — can't read · several selected · pixel limit · link glyph"
        when={<>Properties that <b>can't be read</b> keep the popup frame: the item's own glyph and name in the title, then an error line (alertTriangle 16 in explorer-error-fg + “Can't read properties”), the OS reason in mono, and a Retry. The title is <b>regular weight</b> — the same rule as the plugin name: the theme carries no semibold UI face, so popups use regular at 14 and the size carries the title. The preview with <b>nothing</b> selected keeps “Select a file”; with <b>several</b> it says how many and asks for one. A picture over the <b>pixel limit</b> uses the same “Too large to preview” screen as the byte limit, with its own sub-line and the real size on the reason line. Symlinks take the new <b>link</b> glyph. Kind reads as a word: <b>“PNG image”</b>, “Folder”, “Symbolic link”, unknown type “{"{EXT}"} file”, no extension “File”.</>}>
        <YStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="Properties — can't read" w={360}>
            <YProps glyph={YK.ic.file} name="private.key">
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", paddingTop: "var(--tasty-space-xs)" }}>
                <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-explorer-error-fg)" }}><YIcon name="alertTriangle" size="var(--tasty-icon-size-md)" />Can't read properties</span>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Permission denied (os error 13)</span>
                <span style={{ display: "flex", paddingTop: "var(--tasty-space-xs)" }}><YButton variant="secondary" size="sm">Retry</YButton></span>
              </div>
            </YProps>
          </XCol>
          {["multi", "pixels"].map((k) => (
            <XCol key={k} label={k === "multi" ? "preview — several selected" : "preview — over the pixel limit"}>
              <div style={{ height: 230, display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}><YPreview kind={k} /></div>
            </XCol>
          ))}
        </YStage>
        <YMeta
          specs={[["can't read", "title = item glyph + name · alertTriangle + “Can't read properties” (explorer-error-fg) · reason mono · Retry (secondary sm)"], ["title weight", "regular (font-weight-normal) at 14 — rule for every popup title until a semibold UI face ships"], ["none selected", "“Select a file” (unchanged)"], ["several", "layers glyph · “{n} items selected” · “Select one file to preview it.”"], ["pixel limit", "same screen as > 1 MB · sub “Over {px} px on a side, or needs more than {mem} to decode.” · reason = real size"], ["units", "binary sizes say MiB (256 MiB here, 512 MiB in the image viewer)"], ["Kind", "“{TYPE} image” · Folder · Symbolic link · “{EXT} file” · File"], ["list min", "explorer-list-min-width 200"], ["tokens", "props-padding-x 14 · props-row-min-height 24 · props-row-line 20 · props-row-pad-top 2 · preview-header-height 40"]]}
          tokens={[{ tok: "--tasty-explorer-props-padding-x", use: "→ size-14" }, { tok: "--tasty-explorer-props-row-min-height", use: "→ size-24" }, { tok: "--tasty-explorer-props-row-line", use: "→ size-20" }, { tok: "--tasty-explorer-props-row-pad-top", use: "→ size-2" }, { tok: "--tasty-explorer-preview-header-height", use: "→ size-40" }, { tok: "--tasty-explorer-error-fg", use: "can't read", color: "var(--tasty-explorer-error-fg)" }]} />
        <YNote>i18n (new): <code>explorer.props.unreadable</code> “Can't read properties” · <code>explorer.preview.multi</code> “{"{n}"} items selected” · <code>explorer.preview.multi_sub</code> “Select one file to preview it.” · <code>explorer.preview.too_large_pixels_sub</code> “Over {"{px}"} px on a side, or needs more than {"{mem}"} to decode.” · <code>explorer.kind.image</code> “{"{type}"} image” · <code>explorer.kind.ext_file</code> “{"{ext}"} file”.</YNote>
      </YSpec>
    </YSection>
  );
}

function Page() {
  return (
    <>
      <CreateSection />
      <DragSection />
      <ProgressSection />
      <PropertiesSection />
      <SearchSection />
      <FollowupsSection />
      <ExplorerOpsB11_FollowupsB11Section />
      <ExplorerOpsB11_RemainingSection />
      <ExplorerOpsB12_FollowupsB12Section />
      <ExplorerOpsB12_RemainingB12Section />
      <ExplorerOpsRemote_RemoteWriteSection />
      <ExplorerOpsDelete_PermanentDeleteSection />
      <ExplorerOpsStaging_StagingLeftSection />
      <ExplorerOpsPropsActions_PropsActionsSection />
    </>
  );
}


export const page = {
  active: "explorer-ops",
  nav: NAV,
  head: {
    title: "Explorer — file operations",
    intro: "See how the Explorer creates, moves, and inspects files. Compare examples of search, remote writes, deletion, progress, name conflicts, properties, and operation results.",
    
  },
  content: <Page />,
};

export { YResult, YProgressStatus, YStatusRight, YToastAt, YConflict, YQueue, ProgressSection, YField, YProps, YPreview, YThumbCell, PropertiesSection, YGridHit, FollowupsSection, Page };

export default function GalleryRoute() {
  return page.content;
}
