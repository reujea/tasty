import React from "react";
import { XCol, XThemes, XCell } from "./create-drag-search.jsx";
import * as QK from "../parts/explorer-kit.jsx";
import { Section as QSection, Spec as QSpec, Stage as QStage, Meta as QMeta, Note as QNote } from "../parts/specimen.jsx";
import { QIconButton, QIcon, QBody, QROWS } from "./address-and-toolbar.jsx";

// ── Properties (b12) — local copy of the YProps frame (explorer-ops/index.jsx mounts after this file) ──
function QProps({ glyph = QK.ic.file, glyphColor, name, children }) {
  return (
    <div style={{ width: "var(--tasty-explorer-props-width)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-explorer-props-padding-x) var(--tasty-space-sm)" }}>
        <span style={{ display: "inline-flex", flex: "none", color: glyphColor || "var(--tasty-text-muted)" }}>{glyph}</span>
        <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-normal)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
        <QIconButton size="sm" aria-label="Close" title="Close">{QK.ic.x}</QIconButton>
      </div>
      <div style={{ padding: "0 var(--tasty-explorer-props-padding-x) var(--tasty-space-md)", display: "flex", flexDirection: "column" }}>{children}</div>
    </div>
  );
}
function QField({ k, v, mono, copy, warn }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-explorer-props-label-width) 1fr", gap: "var(--tasty-space-sm)", alignItems: "start", minHeight: "var(--tasty-explorer-props-row-min-height)", paddingTop: "var(--tasty-explorer-props-row-pad-top)" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-explorer-props-row-line)" }}>{k}</span>
      <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
        <span style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
          <span style={{ flex: 1, minWidth: 0, fontFamily: mono ? "var(--tasty-font-mono)" : "inherit", fontSize: mono ? "var(--tasty-font-size-caption)" : "var(--tasty-font-size-body)", lineHeight: "var(--tasty-explorer-props-row-line)", color: "var(--tasty-text-secondary)", overflowWrap: "anywhere" }}>{v}</span>
          {copy && <QIconButton size="sm" aria-label="Copy" title="Copy"><QIcon name="copy" /></QIconButton>}
        </span>
        {warn && <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-explorer-props-row-line)", color: warn[1] }}><QIcon name="alertTriangle" size="var(--tasty-icon-size-xs)" />{warn[0]}</span>}
      </span>
    </div>
  );
}
function QPreview({ h, low }) {
  return (
    <div style={{ width: "var(--tasty-explorer-preview-width)", flex: "none", display: "flex", flexDirection: "column", borderLeft: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-panel)" }}>
      <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", height: "var(--tasty-explorer-preview-header-height)", flex: "none", padding: "0 var(--tasty-space-sm)", borderBottom: low ? "none" : "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>diagram.png</span>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>PNG image · 640 × 400 · 96 KB</span>
      </div>
      {!low && <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-sidebar)" }}>
        <div style={{ width: "100%", aspectRatio: "16 / 10", display: "flex", alignItems: "center", justifyContent: "center", border: "var(--tasty-border-width) solid var(--tasty-separator)", borderRadius: "var(--tasty-radius-sm)", background: "var(--tasty-surface-raised)", color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)" }}>image · fit</div>
      </div>}
    </div>
  );
}

function RemainingB12Section() {
  const partial = "var(--tasty-explorer-props-partial-fg)";
  const broken = "var(--tasty-explorer-link-broken-fg)";
  return (
    <QSection id="remaining12" title="Remaining UI — counting stage · Properties rows · low preview (b12)">
      <QSpec id="progress-counting-stage-zero-byte-work" title="Progress — counting stage · zero-byte work"
        when={<>A copy or move first counts what it will touch. That stage now has its <b>own words</b> — “Counting items to copy · {"{n}"} found” — and <b>no bar</b> (there is nothing to measure against yet); the count refreshes at most 4 times a second. Once counting ends, the line counts <b>files found</b>, not top-level items, and names the file in work: “Copying 1,204 of 200,000 · a/b/file.txt” shows only the name. The bar follows <b>bytes</b>; when the byte total is 0 or unknown it follows the <b>file count</b>, so a tree of empty files still moves.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
          <XThemes render={(l) => (
            <XCell w={520} h={110} status={l === "Mocha" ? "Counting items to copy · 48,210 found" : "Copying 1,204 of 200,000 · file-01204.txt"} statusRight={l === "Latte" ? <span style={{ position: "absolute", left: 0, top: 0, height: "var(--tasty-explorer-progress-height)", width: "100%", background: "var(--tasty-explorer-progress-track)" }}><span style={{ display: "block", height: "100%", width: "0.6%", minWidth: 2, background: "var(--tasty-explorer-progress-fill)" }}></span></span> : null}><div style={{ flex: 1 }} /></XCell>
          )} />
        </QStage>
        <QMeta
          specs={[["counting", "explorer.op.counting_copy / counting_move · no bar · ≤ 4 updates / s"], ["after", "“Copying {i} of {n} · {name}” · n = files found · name = current file name"], ["bar", "bytes; byte total 0 or unknown → file count"]]}
          tokens={[{ tok: "--tasty-explorer-progress-fill", use: "bar", color: "var(--tasty-explorer-progress-fill)" }, { tok: "--tasty-explorer-progress-track", use: "track", color: "var(--tasty-explorer-progress-track)" }]} />
        <QNote>i18n — <code>explorer.op.counting_copy</code> “Counting items to copy · {"{n}"} found” / “복사할 항목 세는 중 · {"{n}"}개 찾음” / “コピーする項目を数えています · {"{n}"} 件” · <code>explorer.op.counting_move</code> “Counting items to move · {"{n}"} found” / “이동할 항목 세는 중 · {"{n}"}개 찾음” / “移動する項目を数えています · {"{n}"} 件”. Numbers use the locale grouping.</QNote>
      </QSpec>

      <QSpec id="properties-own-path-folder-permissions-links-partial-size" title="Properties — own path · folder permissions · links · partial size"
        when={<>For <b>one item</b>, a <b>Path</b> row (the item's own absolute path, mono, Copy) replaces Location; several items keep <b>Location</b> (their common folder). <b>Folders</b> get the same Permissions row as files. A <b>link</b> shows what the user cares about — its target: Kind “Symbolic link”, Link target, then the target's kind, size and modified time with “Target” labels; the link's own lstat values are not shown. A <b>broken link</b> is Kind <b>“Broken link”</b> (the word the Type column uses) with the warning link glyph in the header and a “Target not found” line under Link target. When a folder size count <b>skipped</b> folders it could not read, a warning line under Size says so.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="folder — permissions · partial size" w={360}>
            <QProps glyph={QK.ic.folder} name="folder">
              <QField k="Kind" v="Folder" />
              <QField k="Size" v="2 B · 3 items" warn={["Some folders couldn't be read", partial]} />
              <QField k="Modified" v="2026-10-10 21:51" mono />
              <QField k="Path" v="~/work/sandbox/folder" mono copy />
              <QField k="Permissions" v="rwxrwxr-x · read-only: no" mono />
            </QProps>
          </XCol>
          <XCol label="link" w={360}>
            <QProps glyph={<QIcon name="link" />} name="notes-link.md">
              <QField k="Kind" v="Symbolic link" />
              <QField k="Link target" v="~/work/notes/notes.md" mono copy />
              <QField k="Target kind" v="Markdown" />
              <QField k="Target size" v="12 KB (12,288 bytes)" />
              <QField k="Target modified" v="2026-06-27 11:40" mono />
              <QField k="Path" v="~/work/sandbox/notes-link.md" mono copy />
            </QProps>
          </XCol>
          <XCol label="broken link" w={360}>
            <QProps glyph={<QIcon name="link" />} glyphColor={broken} name="old-config.toml">
              <QField k="Kind" v="Broken link" />
              <QField k="Link target" v="~/.config/tasty/old-config.toml" mono copy warn={["Target not found", broken]} />
              <QField k="Path" v="~/work/sandbox/old-config.toml" mono copy />
            </QProps>
          </XCol>
        </QStage>
        <QMeta
          specs={[["order", "Kind · Size · Modified · Created · Path · Permissions"], ["Path", "one item: own absolute path + Copy (replaces Location) · several: Location (common folder) + Copy"], ["folder", "Permissions row, same format as files"], ["link", "Kind · Link target · Target kind · Target size · Target modified · Path — no lstat values"], ["broken", "Kind “Broken link” · header glyph link in explorer-link-broken-fg · “Target not found” under Link target"], ["partial", "warning line under Size · alertTriangle 12 · explorer-props-partial-fg"]]}
          tokens={[{ tok: "--tasty-explorer-props-partial-fg", use: "→ accent-warning", color: "var(--tasty-explorer-props-partial-fg)" }, { tok: "--tasty-explorer-link-broken-fg", use: "broken link", color: "var(--tasty-explorer-link-broken-fg)" }]} />
        <QNote>i18n — <code>explorer.props.path</code> “Path” / “경로” / “パス” · <code>.target_kind</code> “Target kind” / “대상 종류” / “リンク先の種類” · <code>.target_size</code> “Target size” / “대상 크기” / “リンク先のサイズ” · <code>.target_modified</code> “Target modified” / “대상 수정 시각” / “リンク先の更新日時” · <code>.target_missing</code> “Target not found” / “대상을 찾을 수 없음” / “リンク先が見つかりません” · <code>.partial</code> “Some folders couldn't be read” / “일부 폴더를 읽지 못함” / “一部のフォルダーを読み取れませんでした” · Kind for a broken link reuses <code>explorer.kind.broken_link</code>.</QNote>
      </QSpec>

      <QSpec id="preview-panel-in-a-low-cell-header-only" title="Preview panel in a low cell — header only"
        when={<>When the body under the preview header would be shorter than <span className="tok">--tasty-explorer-preview-body-min-height</span> 120, the body is <b>hidden</b> and only the header stays (name + kind · size), without its bottom rule. A 30 px thumbnail says nothing; the header still tells the user what is selected. The panel keeps its width, and the width rules (hide under preview-min + list-min) are unchanged.</>}>
        <QStage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XCol label="tall cell — body shown" w={640}>
            <XCell w={640} h={300} status="5 items"><QBody>{QROWS.map((r, i) => <QK.DetailRow key={i} {...r} state={i === 3 ? "selected" : undefined} />)}</QBody><QPreview /></XCell>
          </XCol>
          <XCol label="180 cell — body < 120 → header only" w={640}>
            <XCell w={640} h={180} status="5 items"><QBody>{QROWS.slice(2).map((r, i) => <QK.DetailRow key={i} {...r} state={i === 1 ? "selected" : undefined} />)}</QBody><QPreview low /></XCell>
          </XCol>
        </QStage>
        <QMeta
          specs={[["threshold", "body height < explorer-preview-body-min-height 120 → body hidden"], ["kept", "header (name + kind · size), no bottom rule"], ["width", "unchanged rules"]]}
          tokens={[{ tok: "--tasty-explorer-preview-body-min-height", use: "→ size-120" }, { tok: "--tasty-explorer-preview-header-height", use: "40" }]} />
        <QNote><b>Conditional items B1–B7</b> (no design until decided).</QNote>
      </QSpec>
    </QSection>
  );
}

export { RemainingB12Section };
