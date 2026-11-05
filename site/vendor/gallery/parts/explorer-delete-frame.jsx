import React from "react";
import { Button, Icon } from "../../components/index.js";

// Tasty Gallery — Explorer "Delete permanently" confirm (2026-10-12). Self-contained (DS only) so both
// the Explorer file-ops page and Overlays › Dialogs can render the same frame.



// en / ko / ja copy. Keys in the specimen Note. {n}=top-level count, {k}=folders among them.
const XD_COPY = {
  en: {
    title: "Delete permanently?",
    file: (n) => <>“{n}” will be deleted. It won't go to the Trash.</>,
    folder: (n) => <>“{n}” and everything inside it will be deleted. It won't go to the Trash.</>,
    link: (n) => <>The link “{n}” will be deleted. The item it points to is kept.</>,
    many: (n, k) => <>{n} items will be deleted{k ? <>, including everything inside {k === 1 ? "1 folder" : k + " folders"}</> : null}. They won't go to the Trash.</>,
    warn: "This can't be undone.",
    cancel: "Cancel", ok: "Delete permanently", folderTag: "folder · contents included", linkTag: "link", brokenTag: "broken link",
  },
  ko: {
    title: "영구 삭제할까요?",
    file: (n) => <>“{n}” 항목을 삭제합니다. 휴지통으로 가지 않습니다.</>,
    folder: (n) => <>“{n}” 폴더와 그 안의 모든 내용을 삭제합니다. 휴지통으로 가지 않습니다.</>,
    link: (n) => <>링크 “{n}”을(를) 삭제합니다. 링크가 가리키는 항목은 남습니다.</>,
    many: (n, k) => <>항목 {n}개를 삭제합니다{k ? <>. 폴더 {k}개의 안쪽 내용도 함께 삭제됩니다</> : null}. 휴지통으로 가지 않습니다.</>,
    warn: "되돌릴 수 없습니다.",
    cancel: "취소", ok: "영구 삭제", folderTag: "폴더 · 내용 포함", linkTag: "링크", brokenTag: "깨진 링크",
  },
  ja: {
    title: "完全に削除しますか?",
    file: (n) => <>「{n}」を削除します。ゴミ箱には移動しません。</>,
    folder: (n) => <>「{n}」とその中のすべてを削除します。ゴミ箱には移動しません。</>,
    link: (n) => <>リンク「{n}」を削除します。リンク先の項目は残ります。</>,
    many: (n, k) => <>{n} 個の項目を削除します{k ? <>。{k} 個のフォルダーの中身もすべて削除されます</> : null}。ゴミ箱には移動しません。</>,
    warn: "元に戻せません。",
    cancel: "キャンセル", ok: "完全に削除", folderTag: "フォルダー · 中身を含む", linkTag: "リンク", brokenTag: "壊れたリンク",
  },
};

const GLYPH = { file: "file", folder: "folder", link: "link", broken: "link" };
function XDRow({ kind, name, parent, lang }) {
  const c = XD_COPY[lang];
  const tag = kind === "folder" ? c.folderTag : kind === "link" ? c.linkTag : kind === "broken" ? c.brokenTag : null;
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xs) var(--tasty-space-sm)", minWidth: 0 }}>
      <span style={{ display: "inline-flex", flex: "none", height: "var(--tasty-size-20)", alignItems: "center", color: kind === "broken" ? "var(--tasty-explorer-link-broken-fg)" : "var(--tasty-text-muted)" }}><Icon name={GLYPH[kind]} size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <span style={{ display: "flex", alignItems: "baseline", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>{name}</span>
          {tag && <span style={{ flex: "none", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{tag}</span>}
        </span>
        <span title={parent} style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: "rtl", textAlign: "left" }}>{"\u200e" + parent}</span>
      </span>
    </div>
  );
}

// items: [{ kind: file|folder|link|broken, name, parent }]. focus: "cancel" | "ok" | null. okState "disabled" = after the first press (single fire).
// Hover / pressed are the DS Button's own danger states (button-danger-* tokens) — nothing new here.
function ExplorerDeleteFrame({ lang = "en", items, focus = "cancel", okState, width }) {
  const c = XD_COPY[lang];
  const one = items.length === 1 ? items[0] : null;
  const folders = items.filter((i) => i.kind === "folder").length;
  const summary = one ? (one.kind === "folder" ? c.folder(one.name) : one.kind === "file" ? c.file(one.name) : c.link(one.name)) : c.many(items.length, folders);
  const ring = (on, node) => <span style={{ display: "inline-flex", borderRadius: "var(--tasty-radius)", outline: on ? "var(--tasty-focus-ring-width) solid var(--tasty-border-focus)" : "none", outlineOffset: "var(--tasty-border-width)" }}>{node}</span>;
  return (
    <div role="alertdialog" aria-label={c.title} style={{ width: width || "var(--tasty-info-modal-width)", maxWidth: "100%", maxHeight: "var(--tasty-info-modal-max-height)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden", textAlign: "left" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14) 0", minHeight: 0, flex: "1 1 auto" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-accent-danger-raised)" }}><Icon name="trash" size="var(--tasty-icon-size-md)" /></span>
          <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{c.title}</span>
        </div>
        <p style={{ margin: 0, fontSize: "var(--tasty-font-size-body)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)", textWrap: "pretty" }}>{summary}</p>
        <div style={{ flex: "0 1 auto", minHeight: 0, maxHeight: "var(--tasty-explorer-delete-list-max-height)", overflowY: "auto", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", padding: "var(--tasty-space-xs) 0" }}>
          {items.map((it, i) => <XDRow key={i} lang={lang} {...it} />)}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-accent-danger-raised)" }}><Icon name="alertTriangle" size="var(--tasty-icon-size-sm)" /></span>{c.warn}
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end", flexWrap: "wrap", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)", flex: "none" }}>
        {ring(focus === "cancel", <Button variant="ghost" size="sm">{c.cancel}</Button>)}
        {ring(focus === "ok", <Button variant="danger" size="sm" disabled={okState === "disabled"}>{c.ok}</Button>)}
      </div>
    </div>
  );
}

const XD_ITEMS = {
  file: [{ kind: "file", name: "report.pdf", parent: "~/Downloads" }],
  folder: [{ kind: "folder", name: "mockup-exports", parent: "~/Downloads" }],
  link: [{ kind: "link", name: "current", parent: "~/deploy" }],
  many: [
    { kind: "file", name: "report.pdf", parent: "~/Downloads" },
    { kind: "file", name: "report.pdf", parent: "~/Downloads/Documents/finance/2026/q3" },
    { kind: "folder", name: "build", parent: "~/work/tasty/target" },
    { kind: "broken", name: "latest-log", parent: "~/work/tasty/logs" },
    { kind: "file", name: "a-very-long-generated-artifact-name-app-release-unsigned-arm64.apk", parent: "~/work/tasty/target/release/bundle/android/outputs/apk" },
    { kind: "folder", name: "cache", parent: "~/work/tasty/node_modules/.vite" },
  ],
};

export { ExplorerDeleteFrame, XD_ITEMS };
