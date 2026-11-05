import React from "react";
import { Button, IconButton, Icon, Checkbox, Spinner, Select, MenuItem } from "../../components/index.js";
import { XCol, XThemes } from "./create-drag-search.jsx";
import * as K from "../parts/explorer-kit.jsx";
import { Section, Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Explorer Properties: time format setting · Open footer · permission editor · OS menu rows / link note check
// (2026-10-21). Loaded before explorer-ops/index.jsx.






const PA = {
  en: { open: "Open", edit: "Edit", cancel: "Cancel", apply: "Apply", applying: "Applying…", read: "Read", write: "Write", exec: "Execute",
    owner: "Owner", group: "Group", others: "Others", folderOnly: "Changes this folder only, not the items in it.",
    special: "Special bits (setuid, setgid, sticky) are kept.", locked: "Only the owner can change permissions.",
    failed: "Couldn't change permissions", readOnly: "Read-only", winHint: "Windows file attribute. Other access rules aren't changed here.",
    tfLabel: "Time format in Properties", tfCaption: "Used for Modified, Created and Target modified in Explorer Properties, in this computer's time zone.", example: "Example:",
    tf: ["Minutes", "Seconds", "Seconds and UTC offset"], kind: "Kind", size: "Size", modified: "Modified", created: "Created", path: "Path", perms: "Permissions" },
  ko: { open: "열기", edit: "편집", cancel: "취소", apply: "적용", applying: "적용 중…", read: "읽기", write: "쓰기", exec: "실행",
    owner: "소유자", group: "그룹", others: "기타", folderOnly: "이 폴더만 바꾸며 안의 항목은 그대로입니다.",
    special: "특수 비트(setuid, setgid, sticky)는 그대로 둡니다.", locked: "소유자만 권한을 바꿀 수 있습니다.",
    failed: "권한을 바꾸지 못했습니다", readOnly: "읽기 전용", winHint: "Windows 파일 특성입니다. 다른 접근 규칙은 여기서 바꾸지 않습니다.",
    tfLabel: "속성 창 시각 형식", tfCaption: "탐색기 속성 창의 수정 시각·만든 시각·대상 수정 시각에 쓰며, 이 컴퓨터의 시간대 기준입니다.", example: "예:",
    tf: ["분까지", "초까지", "초 + UTC 오프셋"], kind: "종류", size: "크기", modified: "수정 시각", created: "만든 시각", path: "경로", perms: "권한" },
  ja: { open: "開く", edit: "編集", cancel: "キャンセル", apply: "適用", applying: "適用中…", read: "読み取り", write: "書き込み", exec: "実行",
    owner: "所有者", group: "グループ", others: "その他", folderOnly: "このフォルダーだけを変更し、中の項目は変わりません。",
    special: "特殊ビット（setuid・setgid・sticky）はそのまま残ります。", locked: "所有者だけが権限を変更できます。",
    failed: "権限を変更できませんでした", readOnly: "読み取り専用", winHint: "Windows のファイル属性です。ほかのアクセス規則はここでは変更しません。",
    tfLabel: "プロパティの日時形式", tfCaption: "エクスプローラーのプロパティの更新日時・作成日時・リンク先の更新日時に使います。このコンピューターのタイムゾーンで表示します。", example: "例:",
    tf: ["分まで", "秒まで", "秒＋UTCオフセット"], kind: "種類", size: "サイズ", modified: "更新日時", created: "作成日時", path: "パス", perms: "権限" },
};
const TF = [["minutes", "%Y-%m-%d %H:%M", "2026-10-21 12:12"], ["seconds", "%Y-%m-%d %H:%M:%S", "2026-10-21 12:12:05"], ["seconds_offset", "%Y-%m-%d %H:%M:%S %:z", "2026-10-21 12:12:05 +09:00"]];

const pLbl = { fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-explorer-props-row-line)" };
const pMono = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)" };

// Properties frame (same as YProps / QProps) + optional Open footer
function PProps({ glyph = K.ic.file, glyphColor, name, children, note, footer, w }) {
  return (
    <div style={{ width: w || "var(--tasty-explorer-props-width)", maxWidth: "100%", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-explorer-props-padding-x) var(--tasty-space-sm)" }}>
        <span style={{ display: "inline-flex", flex: "none", color: glyphColor || "var(--tasty-text-muted)" }}>{glyph}</span>
        <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-normal)", color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{name}</span>
        <IconButton size="sm" aria-label="Close" title="Close">{K.ic.x}</IconButton>
      </div>
      <div style={{ padding: "0 var(--tasty-explorer-props-padding-x) var(--tasty-space-md)", display: "flex", flexDirection: "column" }}>
        {children}
        {note && <div style={{ marginTop: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>{note}</div>}
      </div>
      {footer && <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-explorer-props-padding-x)", borderTop: "var(--tasty-border-width) solid var(--tasty-explorer-props-footer-border)" }}>{footer}</div>}
    </div>
  );
}
function PField({ k, v, mono, copy, warn, action, under, below, time }) {
  // time values wrap only between date · time · offset (each part is one unbreakable unit)
  const val = time ? v.split(" ").map((p, i) => <React.Fragment key={i}>{i > 0 && " "}<span style={{ whiteSpace: "nowrap" }}>{p}</span></React.Fragment>) : v;
  return (
    <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-explorer-props-label-width) minmax(0, 1fr)", gap: "var(--tasty-space-sm)", alignItems: "start", minHeight: "var(--tasty-explorer-props-row-min-height)", paddingTop: "var(--tasty-explorer-props-row-pad-top)" }}>
      <span style={pLbl}>{k}</span>
      <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
        <span style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
          <span style={{ flex: 1, minWidth: 0, ...(mono || time ? pMono : { fontSize: "var(--tasty-font-size-body)" }), lineHeight: "var(--tasty-explorer-props-row-line)", color: "var(--tasty-explorer-props-value-fg)", overflowWrap: time ? "normal" : "anywhere" }}>{val}</span>
          {copy && <IconButton size="sm" aria-label="Copy" title="Copy"><Icon name="copy" /></IconButton>}
          {action}
        </span>
        {warn && <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-explorer-props-row-line)", color: warn[1] }}><Icon name="alertTriangle" size="var(--tasty-icon-size-xs)" />{warn[0]}</span>}
        {under}
      </span>
      {below && <div style={{ gridColumn: "1 / -1", minWidth: 0 }}>{below}</div>}
    </div>
  );
}
const openBtn = (L, p = {}) => <Button variant="secondary" size="sm" {...p}>{L.open}</Button>;
const editBtn = (L) => <Button variant="ghost" size="sm">{L.edit}</Button>;

// permission editor — opens under the Permissions row and spans the CONTENT width (label column included; 2026-10-21):
// 360 − 2 × 14 − 2 × 1 = 330 ≥ 4 × 56 + 2 × 8 + 2 × 1 = 242.
// Column heads and role names wrap inside their 56 cell (never ellipsised, never squeezed).
const BITS = { "rw-r--r--": [[1, 1, 0], [1, 0, 0], [1, 0, 0]], "rw-r-----": [[1, 1, 0], [1, 0, 0], [0, 0, 0]], "rwxr-xr-x": [[1, 1, 1], [1, 0, 1], [1, 0, 1]], "rwxrwxr-x": [[1, 1, 1], [1, 1, 1], [1, 0, 1]] };
const octal = (b) => b.map((r) => r[0] * 4 + r[1] * 2 + r[2]).join("");
// 2026-10-22: the box never overflows — it is max-width 100% of the
// content and picks one of three layouts from its OWN border-box width, re-evaluated on every layout pass:
//   grid  (≥ explorer-props-perm-grid-min-width 242)  — batch 8, role column + R W X
//   stack (≥ explorer-props-perm-stack-min-width 186) — heads over 3 cells; each role name on its own line above its 3 checkboxes
//   list  (< 186)                                     — each role name, then three labelled checkboxes one per line (no lower bound)
// Cells stay 56 in grid and stack. DOM / Tab order is identical in all three (Owner R W X → Group → Others → Cancel → Apply).
function usePermLayout(ref, forced) {
  const [lay, setLay] = React.useState(forced || "grid");
  React.useLayoutEffect(() => {
    if (forced) { setLay(forced); return; }
    const el = ref.current; if (!el) return;
    const probe = (tok) => { const p = document.createElement("div"); p.style.cssText = "position:absolute;visibility:hidden;height:0;width:var(" + tok + ")"; el.appendChild(p); const w = p.offsetWidth; p.remove(); return w; };
    const run = () => { const g = probe("--tasty-explorer-props-perm-grid-min-width"), s = probe("--tasty-explorer-props-perm-stack-min-width"), w = el.offsetWidth; setLay(w >= g ? "grid" : w >= s ? "stack" : "list"); };
    run();
    const ro = new ResizeObserver(run); ro.observe(el); return () => ro.disconnect();
  }, [forced]);
  return lay;
}
function PermEditor({ L, mode, from, folder, state = "editing", special, reason, layout }) {
  const busy = state === "applying";
  const ref = React.useRef(null);
  const lay = usePermLayout(ref, layout);
  const box = { position: "relative", boxSizing: "border-box", maxWidth: "100%", minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", marginTop: "var(--tasty-space-xs)", marginBottom: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm)",
    border: "var(--tasty-border-width) solid var(--tasty-explorer-props-perm-border)", borderRadius: "var(--tasty-radius)" };
  const buttons = (changed) => (
    <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "flex-end", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
      {busy && <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}><Spinner size="var(--tasty-icon-size-sm)" />{L.applying}</span>}
      <Button variant="ghost" size="sm" disabled={busy}>{L.cancel}</Button>
      <Button variant="primary" size="sm" disabled={busy || !changed}>{L.apply}</Button>
    </div>
  );
  const err = state === "failed" && (
    <div role="alert" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-explorer-props-perm-error-fg)" }}><Icon name="alertCircle" size="var(--tasty-icon-size-sm)" />{L.failed}</span>
      <span style={{ ...pMono, color: "var(--tasty-text-muted)", overflowWrap: "anywhere" }}>{reason}</span>
    </div>
  );
  if (mode === "win") {
    return (
      <div ref={ref} style={box}>
        <Checkbox label={L.readOnly} defaultChecked={state !== "failed"} disabled={busy} />
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>{L.winHint}</span>
        {err}
        {buttons(state !== "failed")}
      </div>
    );
  }
  const target = state === "failed" ? BITS[from] : BITS[from].map((r, i) => (i === 2 ? [0, 0, 0] : r));
  const cell = { width: "var(--tasty-explorer-props-perm-cell-width)", flex: "none", display: "flex", justifyContent: "center" };
  const head = { ...cell, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", textAlign: "center", lineHeight: "var(--tasty-line-height-ui)", overflowWrap: "anywhere" };
  const roleTxt = { fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-explorer-props-value-fg)", overflowWrap: "anywhere" };
  const cols = [L.read, L.write, L.exec];
  const cb = (role, r, c, lbl) => <Checkbox aria-label={role + " " + cols[c]} label={lbl} checked={!!target[r][c]} disabled={busy} onChange={() => {}} />;
  const roles = [L.owner, L.group, L.others];
  return (
    <div ref={ref} style={box} data-perm-layout={lay}>
      {lay === "grid" && <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
        <div style={{ display: "flex", alignItems: "flex-end" }}><span style={{ width: "var(--tasty-explorer-props-perm-cell-width)", flex: "none" }}></span>{cols.map((h) => <span key={h} style={head}>{h}</span>)}</div>
        {roles.map((role, r) => (
          <div key={role} style={{ display: "flex", alignItems: "center", minHeight: "var(--tasty-explorer-props-row-min-height)" }}>
            <span style={{ ...roleTxt, width: "var(--tasty-explorer-props-perm-cell-width)", flex: "none" }}>{role}</span>
            {[0, 1, 2].map((c) => <span key={c} style={cell}>{cb(role, r, c)}</span>)}
          </div>
        ))}
      </div>}
      {lay === "stack" && <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
        <div style={{ display: "flex", alignItems: "flex-end" }}>{cols.map((h) => <span key={h} style={head}>{h}</span>)}</div>
        {roles.map((role, r) => (
          <div key={role} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-explorer-props-perm-role-gap)" }}>
            <span style={roleTxt}>{role}</span>
            <div style={{ display: "flex", alignItems: "center", minHeight: "var(--tasty-explorer-props-row-min-height)" }}>{[0, 1, 2].map((c) => <span key={c} style={cell}>{cb(role, r, c)}</span>)}</div>
          </div>
        ))}
      </div>}
      {lay === "list" && <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        {roles.map((role, r) => (
          <div key={role} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-explorer-props-perm-role-gap)", minWidth: 0 }}>
            <span style={roleTxt}>{role}</span>
            {[0, 1, 2].map((c) => <span key={c} style={{ display: "flex", minWidth: 0, minHeight: "var(--tasty-explorer-props-row-min-height)", alignItems: "center", overflowWrap: "anywhere" }}>{cb(role, r, c, cols[c])}</span>)}
          </div>
        ))}
      </div>}
      <span style={{ ...pMono, color: "var(--tasty-text-secondary)", display: "flex", flexWrap: "wrap", columnGap: "0.5ch" }}><span style={{ whiteSpace: "nowrap" }}>{target.map((r) => (r[0] ? "r" : "-") + (r[1] ? "w" : "-") + (r[2] ? "x" : "-")).join("")}</span><span style={{ whiteSpace: "nowrap" }}>· {octal(target)}</span></span>
      {(folder || special) && <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>{folder ? L.folderOnly : null}{folder && special ? " " : null}{special ? L.special : null}</span>}
      {err}
      {buttons(state !== "failed")}
    </div>
  );
}

function SettingsTimeRow({ lang = "en", pick = 0 }) {
  const L = PA[lang];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", width: "var(--tasty-size-560)", maxWidth: "100%", boxSizing: "border-box" }}>
      <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-xs) var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)" }}>
        <span style={{ width: "var(--tasty-settings-label-width)", flex: "none", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)" }}>{L.tfLabel}:</span>
        <span style={{ display: "flex", width: "var(--tasty-field-width-lg)" }}><Select block options={TF.map(([v], i) => ({ value: v, label: L.tf[i] }))} defaultValue={TF[pick][0]} /></span>
      </div>
      <p style={{ margin: 0, maxWidth: "var(--tasty-measure-md)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>
        {L.tfCaption} {L.example} <span style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{TF[pick][2]}</span>
      </p>
    </div>
  );
}

// popup width that gives the box exactly `tok` (− a step): box = popup − 2 × padding-x − 2 × border
const popW = (tok, minus) => "calc(var(" + tok + ") + 2 * var(--tasty-explorer-props-padding-x) + 2 * var(--tasty-border-width)" + (minus ? " - var(" + minus + ")" : "") + ")";
const W_GRID = popW("--tasty-explorer-props-perm-grid-min-width"), W_UNDER = popW("--tasty-explorer-props-perm-grid-min-width", "--tasty-space-xs");
const W_STACK = popW("--tasty-explorer-props-perm-stack-min-width"), W_LIST = popW("--tasty-explorer-props-perm-stack-min-width", "--tasty-space-xl");
function PermNarrowSpec() {
  const L = PA.en;
  const unix = (lab, w, p, ll = L, lang) => (
    <XCol label={lab} w={null}>
      <PProps glyph={p.folder ? K.ic.folder : K.ic.file} name={p.name || "deploy.env"} w={w} footer={openBtn(ll)}>
        <PField k={ll.perms} v={(p.from || "rw-r--r--") + " · read-only: no"} mono below={<PermEditor L={ll} mode="unix" from={p.from || "rw-r--r--"} {...p} />} />
      </PProps>
    </XCol>
  );
  const win = (lab, w, p = {}) => (
    <XCol label={lab} w={null}>
      <PProps glyph={K.ic.file} name="report.docx" w={w} footer={openBtn(L)}>
        <PField k="Permissions" v="read-only: yes" mono below={<PermEditor L={L} mode="win" {...p} />} />
      </PProps>
    </XCol>
  );
  const st = { padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" };
  return (
    <Spec id="permissions-narrow-explorer-cell-the-box-re-lays-itself-never-cl" title="Permissions — narrow Explorer cell: the box re-lays itself, never clips"
      when={<>The popup shrinks with its cell and the editor box now does too: it is <b>max-width 100%</b> of the content and chooses its layout from its <b>own width</b>. <b>Grid</b> (box ≥ <code>explorer-props-perm-grid-min-width</code> 242) is the batch 8 layout, unchanged. <b>Stack</b> (box ≥ <code>explorer-props-perm-stack-min-width</code> 186): the role column goes away; column heads sit over the three 56 cells and each role name is a line of its own above its three checkboxes, so the grid stays aligned and needs 3 × 56 + 18. <b>List</b> (box &lt; 186): no head row; each role name, then three <b>labelled</b> checkboxes (Read · Write · Execute) one per line. List has <b>no lower bound</b>: labels and role names wrap anywhere, nothing is cut, only the box grows taller (the popup grows with it, as today). Cell width 56 is never changed. In every layout the readout wraps only at “ · ”, Cancel / Apply wrap onto a second line (still right-aligned, Apply last) when they don't fit side by side, and the box's right edge, padding and the footer Open keep their inset. No horizontal scroll.</>}>
      <Stage variant="solo center" style={st}>
        {unix("wide — grid (box 330)", null, {})}
        {unix("box 242 — grid, at the boundary", W_GRID, {})}
        {unix("box 238 — stack (just under)", W_UNDER, {})}
        {unix("box 186 — stack, its lower bound", W_STACK, {})}
        {unix("box 162 — list", W_LIST, {})}
      </Stage>
      <Stage variant="solo center" style={st}>
        <XThemes render={() => (
          <PProps glyph={K.ic.file} name="deploy.env" w={W_UNDER} footer={openBtn(L)}>
            <PField k="Permissions" v="rw-r--r-- · read-only: no" mono below={<PermEditor L={L} mode="unix" from="rw-r--r--" />} />
          </PProps>
        )} />
        {unix("stack · applying", W_UNDER, { name: "shared", folder: true, special: true, from: "rwxrwxr-x", state: "applying" })}
        {unix("stack · failed", W_UNDER, { name: "mounted.iso", state: "failed", reason: "Read-only file system (os error 30)" })}
        {unix("list · folder + special bits", W_LIST, { name: "shared", folder: true, special: true, from: "rwxrwxr-x" })}
        {unix("list · failed", W_LIST, { name: "mounted.iso", state: "failed", reason: "Read-only file system (os error 30)" })}
      </Stage>
      <Stage variant="solo center" style={st}>
        {unix("ko · stack", W_UNDER, { name: "배포.env" }, PA.ko)}
        {unix("ja · stack · folder", W_UNDER, { name: "共有", folder: true, from: "rwxr-xr-x" }, PA.ja)}
        {unix("ja · list", W_LIST, { name: "共有", folder: true, from: "rwxr-xr-x" }, PA.ja)}
        {win("Windows · box 238", W_UNDER)}
        {win("Windows · box 162 · failed", W_LIST, { state: "failed", reason: "Access is denied. (os error 5)" })}
      </Stage>
      <Meta
        specs={[["box", "max-width 100% of the field row (label column included) · never wider than the popup content · border, radius, padding space-sm, gap space-sm as batch 8"], ["grid", "box ≥ explorer-props-perm-grid-min-width (4 × 56 + 2 × 8 + 2 × 1 = 242) — batch 8 as is"], ["stack", "186 ≤ box < 242 · head row = 3 cells of 56 (heads centred, bottom-aligned, wrap in the cell) · per role: name line (caption, value-fg, wraps anywhere) → gap explorer-props-perm-role-gap 4 → row of 3 cells, min-height row-min-height 24 · role groups gap space-xs"], ["list", "box < explorer-props-perm-stack-min-width (3 × 56 + 18 = 186) · per role: name line → role-gap → three Checkbox with label (Read / Write / Execute), one per 24 row · role groups gap space-sm · no head row · no lower bound"], ["switch", "by the box's own border-box width, on every layout pass — dragging the cell divider re-lays the open box at once (no animation, terminal motion 0); checked state, pending changes, focus and the applying / failed state are kept"], ["readout", "mode text and “· octal” are two unbreakable parts; wraps between them"], ["buttons", "flex-wrap, justify end, gap space-sm · Cancel then Apply; Apply drops to line 2 (right) when both don't fit"], ["Windows", "one labelled Checkbox + hint — no grid, so no layout switch; the box only caps at 100% and the hint wraps; buttons wrap as above"], ["keys", "DOM order = Tab order in all layouts: Owner R W X → Group R W X → Others R W X → Cancel → Apply · focus ring unchanged (Checkbox focus-visible)"], ["a11y", "grid / stack checkboxes keep aria-label “Role Column”; list draws the column word as the visible label"], ["copy", "no new strings — the list labels reuse explorer.props.perm.read / .write / .execute"]]}
        tokens={[{ tok: "--tasty-explorer-props-perm-grid-min-width", use: "new · calc(4 × perm-cell-width + 2 × space-sm + 2 × border-width) = 242" }, { tok: "--tasty-explorer-props-perm-stack-min-width", use: "new · calc(3 × perm-cell-width + 2 × space-sm + 2 × border-width) = 186" }, { tok: "--tasty-explorer-props-perm-role-gap", use: "new · → space-xs (role name → its checkboxes, stack / list)" }]} />
      <Note>Boundaries in cell width (popup = cell − 2 × space-sm; box = popup − 2 × 14 − 2): grid at cell ≥ 288, stack 232 – 287, list below 232. These follow from the tokens; the implementation compares the box width with the two tokens, never the cell width. Shared function <code>tasty_ui_widgets::perm_box</code> takes the available width and returns the layout.</Note>
    </Spec>
  );
}

function PropsActionsSection() {
  const L = PA.en, broken = "var(--tasty-explorer-link-broken-fg)", partial = "var(--tasty-explorer-props-partial-fg)";
  const ctxTail = (show) => (
    <>
      <MenuItem label="Move to Trash" icon={K.ic.trash} danger />
      {show !== false && <><MenuItem separator /><MenuItem label="Open in system" icon={K.ic.external} />{show && <MenuItem label={show} icon={<Icon name="folderOpen" />} />}</>}
      <MenuItem separator />
      <MenuItem label="Properties" icon={<Icon name="info" />} />
    </>
  );
  return (
    <Section id="propsactions" title="Properties — time format · Open · permissions">
      <Spec id="time-format-setting-settings-general-general" title="Time format setting — Settings › General › General"
        when={<>One Select row under <b>Language</b>, because it is a display preference like the language. Three choices, all in the computer's <b>local time zone</b>: <b>Minutes</b> (default, today's format), <b>Seconds</b>, <b>Seconds and UTC offset</b>. The offset is numeric (<code>+09:00</code>): zone abbreviations differ by OS and are ambiguous (CST), and a UTC choice would contradict “local time”. The caption under the row names the fields it changes and shows the chosen format with a fixed example date; the option labels stay short words. Saving applies to open Properties popups on their next draw. The Detail list date column is not affected.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XThemes render={() => <SettingsTimeRow pick={0} />} />
          <XCol label="ko · Seconds" w={560}><SettingsTimeRow lang="ko" pick={1} /></XCol>
          <XCol label="ja · Seconds and UTC offset" w={560}><SettingsTimeRow lang="ja" pick={2} /></XCol>
        </Stage>
        <Meta
          specs={[["place", "Settings › General › General, after Language · SettingsRow + Select field-width-lg 200 · RowCaption under the row"], ["minutes (default)", "%Y-%m-%d %H:%M → 2026-10-21 12:12"], ["seconds", "%Y-%m-%d %H:%M:%S → 2026-10-21 12:12:05"], ["seconds_offset", "%Y-%m-%d %H:%M:%S %:z → 2026-10-21 12:12:05 +09:00 (local offset, DST-aware per value)"], ["applies to", "Properties Modified · Created · Target modified only"], ["example", "fixed sample instant rendered in the chosen format, mono, text-secondary"]]}
          tokens={[{ tok: "--tasty-settings-label-width", use: "150" }, { tok: "--tasty-field-width-lg", use: "200" }, { tok: "--tasty-settings-row-caption-gap", use: "4" }]} />
        <Note>i18n (new): <code>settings.general.props_time_format</code> “Time format in Properties” / “속성 창 시각 형식” / “プロパティの日時形式” · <code>.props_time_format.caption</code> (above) · <code>.props_time_format.example</code> “Example:” / “예:” / “例:” · options <code>.minutes</code> “Minutes” / “분까지” / “分まで” · <code>.seconds</code> “Seconds” / “초까지” / “秒まで” · <code>.seconds_offset</code> “Seconds and UTC offset” / “초 + UTC 오프셋” / “秒＋UTCオフセット”. Config value: <code>minutes | seconds | seconds_offset</code>.</Note>
      </Spec>

      <Spec id="time-fields-in-each-format-wrapping-in-the-value-column" title="Time fields in each format — wrapping in the value column"
        when={<>Time values are mono caption in the ~264 value column. The longest format (26 characters) fits on one line at the 360 popup width. When the popup is narrower (cell smaller than 360), a time value wraps <b>only between its parts</b> — date · time · offset — so the offset moves whole to line 2 and digits are never split. Paths keep wrap-anywhere.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          {TF.map(([k, , ex]) => (
            <XCol key={k} label={k} w={360}>
              <PProps glyph={K.ic.image} name="diagram.png" footer={openBtn(L)}>
                <PField k="Modified" v={ex} time />
                <PField k="Created" v={ex.replace("12:12", "11:58")} time />
              </PProps>
            </XCol>
          ))}
          <XCol label="link · seconds + offset · cell 240 wide" w={240}>
            <PProps glyph={<Icon name="link" />} name="notes-link.md" w="var(--tasty-size-240)" footer={openBtn(L)}>
              <PField k="Kind" v="Symbolic link" />
              <PField k="Target modified" v="2026-06-27 11:40:09 +09:00" time />
              <PField k="Path" v="~/work/sandbox/very/long/folder/name/notes-link.md" mono copy />
            </PProps>
          </XCol>
        </Stage>
      </Spec>

      <Spec id="open-footer-button-shown-where-enter-would-open" title="Open — footer button, shown where Enter would open"
        when={<>A footer under the fields, right-aligned <b>Open</b> (secondary sm, no icon), above a separator rule. It does what <b>Enter / double-click</b> does in the list for the item the popup was opened for: a file follows the file-handler settings, a folder is entered in that Explorer, a link opens its target. The popup <b>stays open</b>. It is shown for <b>one item</b>, local or remote (remote follows the remote list's Enter; where that list's Enter does nothing, the footer is not drawn). A <b>broken link</b> shows Open <b>disabled</b> with the tooltip “Target not found”. <b>Several items</b> and <b>can't read</b> have no footer. Tab order: field buttons (Copy, Edit) then Open then ×.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XThemes render={() => (
            <PProps glyph={K.ic.image} name="diagram.png" footer={openBtn(L)}>
              <PField k="Kind" v="PNG image" />
              <PField k="Size" v="488 KB (499,712 bytes)" />
              <PField k="Modified" v="2026-06-26 18:05" time />
              <PField k="Path" v="~/Downloads/diagram.png" mono copy />
              <PField k="Permissions" v="rw-r--r-- · read-only: no" mono action={editBtn(L)} />
            </PProps>
          )} />
          <XCol label="folder" w={360}>
            <PProps glyph={K.ic.folder} name="mockup-exports" footer={openBtn(L)}>
              <PField k="Kind" v="Folder" />
              <PField k="Size" v="1.1 GB · 1,204 items" />
              <PField k="Path" v="~/Downloads/mockup-exports" mono copy />
              <PField k="Permissions" v="rwxr-xr-x · read-only: no" mono action={editBtn(L)} />
            </PProps>
          </XCol>
          <XCol label="link — opens the target" w={360}>
            <PProps glyph={<Icon name="link" />} name="notes-link.md" footer={openBtn(L)} note="Deleting or renaming this link doesn't change its target.">
              <PField k="Kind" v="Symbolic link" />
              <PField k="Link target" v="~/work/notes/notes.md" mono copy />
              <PField k="Target kind" v="Markdown" />
              <PField k="Path" v="~/work/sandbox/notes-link.md" mono copy />
            </PProps>
          </XCol>
          <XCol label="broken link — Open disabled" w={360}>
            <PProps glyph={<Icon name="link" />} glyphColor={broken} name="old-config.toml" footer={openBtn(L, { disabled: true, title: "Target not found" })} note="Deleting or renaming this link doesn't change its target.">
              <PField k="Kind" v="Broken link" />
              <PField k="Link target" v="~/.config/tasty/old-config.toml" mono copy warn={["Target not found", broken]} />
              <PField k="Path" v="~/work/sandbox/old-config.toml" mono copy />
            </PProps>
          </XCol>
          <XCol label="remote item" w={360}>
            <PProps name="build-0412.log" note="Remote — only what the remote host lists is shown." footer={openBtn(L)}>
              <PField k="Kind" v="File" />
              <PField k="Size" v="2.1 MB" />
              <PField k="Modified" v="2026-10-09 08:12" time />
              <PField k="Location" v="build-eu:~/logs" mono copy />
            </PProps>
          </XCol>
          <XCol label="several items — no footer" w={360}>
            <PProps glyph={<Icon name="layers" />} name="3 items">
              <PField k="Kinds" v="2 files, 1 folder" warn={["Some items couldn't be read", partial]} />
              <PField k="Total size" v="66.4 MB" />
              <PField k="Location" v="~/Downloads" mono copy />
            </PProps>
          </XCol>
          <XCol label="ko · ja" w={360}>
            <PProps glyph={K.ic.file} name="報告書.pdf" footer={<>{openBtn(PA.ja)}</>}>
              <PField k={PA.ja.kind} v="PDF" />
              <PField k={PA.ja.modified} v="2026-10-21 12:12" time />
              <PField k={PA.ja.perms} v="rw-r--r-- · read-only: no" mono action={editBtn(PA.ja)} />
            </PProps>
          </XCol>
        </Stage>
        <Meta
          specs={[["footer", "padding space-sm explorer-props-padding-x · top 1px explorer-props-footer-border · justify end"], ["button", "secondary sm “Open” · same action as Enter for the popup's own item · popup stays open"], ["shown", "one item: file · folder · link (target) · remote (if its list opens on Enter)"], ["disabled", "broken link · tooltip “Target not found”"], ["hidden", "several items · can't read (Retry stays the only button)"], ["keys", "no new shortcut · Tab reaches it · Space / Enter"]]}
          tokens={[{ tok: "--tasty-explorer-props-footer-border", use: "new · → separator", color: "var(--tasty-explorer-props-footer-border)" }, { tok: "--tasty-explorer-props-value-fg", use: "new · → text-secondary (names the existing value ink)", color: "var(--tasty-explorer-props-value-fg)" }]} />
        <Note>i18n (new): <code>explorer.props.open</code> “Open” / “열기” / “開く” · disabled tooltip reuses <code>explorer.props.target_missing</code>.</Note>
      </Spec>

      <Spec id="permissions-edit-opens-an-editor-under-the-row-apply-writes" title="Permissions — Edit opens an editor under the row, Apply writes"
        when={<>The Permissions row keeps its text value and gets an <b>Edit</b> button (ghost sm) at its end. Edit opens a box <b>under the row, spanning the content width</b> — from the label column's left edge to the right inset (330 at the 360 popup), not inside the 226 value column, which is narrower than the 242 the grid needs — with a <b>3 × 3 checkbox grid</b> — Owner · Group · Others × Read · Write · Execute — and the resulting mode as text + octal, so the change is readable before it runs. Nothing is written until <b>Apply</b> (primary sm, disabled while nothing changed); Cancel closes the box. Apply goes through the file-operation queue: the box shows <b>Applying…</b> with a Spinner and locks its controls; on success it closes and the row shows the new value; on failure it stays open with the reason (OS error) and the grid <b>re-reads the real mode</b>. Esc closes the popup and never undoes an applied change. Folders say the change is <b>not recursive</b>; setuid / setgid / sticky are kept and said so when present. When the user is neither the owner nor root, Edit is not drawn and a muted line says <b>only the owner can change permissions</b>. No Edit on links, several items, remote items or can't read.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap", alignItems: "flex-start" }}>
          <XThemes render={() => (
            <PProps glyph={K.ic.file} name="deploy.env" footer={openBtn(L)}>
              <PField k="Kind" v="File" />
              <PField k="Path" v="~/work/tasty/deploy.env" mono copy />
              <PField k="Permissions" v="rw-r--r-- · read-only: no" mono below={<PermEditor L={L} mode="unix" from="rw-r--r--" />} />
            </PProps>
          )} />
          <XCol label="folder — applying" w={360}>
            <PProps glyph={K.ic.folder} name="shared" footer={openBtn(L)}>
              <PField k="Kind" v="Folder" />
              <PField k="Permissions" v="rwxrwxr-x · read-only: no" mono below={<PermEditor L={L} mode="unix" from="rwxrwxr-x" folder special state="applying" />} />
            </PProps>
          </XCol>
          <XCol label="failed — real mode re-read" w={360}>
            <PProps glyph={K.ic.file} name="mounted.iso" footer={openBtn(L)}>
              <PField k="Kind" v="Disk image" />
              <PField k="Permissions" v="rw-r--r-- · read-only: no" mono below={<PermEditor L={L} mode="unix" from="rw-r--r--" state="failed" reason="Read-only file system (os error 30)" />} />
            </PProps>
          </XCol>
          <XCol label="not the owner — no Edit" w={360}>
            <PProps glyph={K.ic.file} name="hosts" footer={openBtn(L)}>
              <PField k="Kind" v="File" />
              <PField k="Path" v="/etc/hosts" mono copy />
              <PField k="Permissions" v="rw-r--r-- · read-only: no" mono under={<span style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-explorer-props-row-line)", color: "var(--tasty-explorer-props-perm-locked-fg)" }}>{L.locked}</span>} />
            </PProps>
          </XCol>
          <XCol label="Windows — read-only attribute only" w={360}>
            <PProps glyph={K.ic.file} name="report.docx" footer={openBtn(L)}>
              <PField k="Kind" v="Word document" />
              <PField k="Permissions" v="read-only: yes" mono below={<PermEditor L={L} mode="win" />} />
            </PProps>
          </XCol>
          <XCol label="Windows — failed" w={360}>
            <PProps glyph={K.ic.file} name="report.docx" footer={openBtn(L)}>
              <PField k="Permissions" v="read-only: no" mono below={<PermEditor L={L} mode="win" state="failed" reason="Access is denied. (os error 5)" />} />
            </PProps>
          </XCol>
          <XCol label="ko — editor" w={360}>
            <PProps glyph={K.ic.file} name="배포.env" footer={openBtn(PA.ko)}>
              <PField k={PA.ko.perms} v="rw-r--r-- · read-only: no" mono below={<PermEditor L={PA.ko} mode="unix" from="rw-r--r--" />} />
            </PProps>
          </XCol>
          <XCol label="ja — editor" w={360}>
            <PProps glyph={K.ic.folder} name="共有" footer={openBtn(PA.ja)}>
              <PField k={PA.ja.perms} v="rwxr-xr-x · read-only: no" mono below={<PermEditor L={PA.ja} mode="unix" from="rwxr-xr-x" folder />} />
            </PProps>
          </XCol>
        </Stage>
        <Meta
          specs={[["entry", "Edit ghost sm at the end of the Permissions value line · hidden when the box is open"], ["box", "2026-10-21: under the row, spans the CONTENT width (grid-column 1 / -1 of the field row — label column included; 330 at popup 360) · grid left-aligned in the box · Unix and Windows boxes alike · 1px explorer-props-perm-border · radius · padding space-sm · gap space-sm · bg stays bg-panel"], ["grid", "role column + 3 columns explorer-props-perm-cell-width 56 (unchanged) = 224 + padding 2 × 8 + border 2 = 242 ≤ 330 · rows row-min-height 24 · Checkbox per bit"], ["long words", "column heads centred and role names left-aligned inside their 56 cell; they wrap to a second line rather than ellipsise (ja 書き込み / 読み取り / グループ ≈ 48 fit on one line at caption size) · heads bottom-aligned so a wrapped head grows upward"], ["value line", "text value, Edit and the locked line stay in the value column; only the editor box spans"], ["readout", "mode text + octal, mono caption, text-secondary — updates as boxes change"], ["Apply", "primary sm, disabled until the mode differs · applying: Spinner + “Applying…”, grid + buttons disabled"], ["success", "box closes, row value = new mode (re-read)"], ["failure", "alertCircle + “Couldn't change permissions” explorer-props-perm-error-fg + OS error mono muted · grid re-read · box stays · queue result shows as today"], ["locked", "euid ≠ owner and ≠ root → no Edit, muted line under the value"], ["Windows", "one Checkbox “Read-only” + caption · same Apply / Cancel · never called ACL"], ["not editable", "links · several · remote · can't read → no Edit, no line"], ["kept", "setuid / setgid / sticky preserved, line shown only when present · never recursive"]]}
          tokens={[{ tok: "--tasty-explorer-props-perm-border", use: "new · → border-strong", color: "var(--tasty-explorer-props-perm-border)" }, { tok: "--tasty-explorer-props-perm-cell-width", use: "new · → size-56" }, { tok: "--tasty-explorer-props-perm-error-fg", use: "new · → accent-danger (bg-panel: Mocha 7.08 · Latte 4.80)", color: "var(--tasty-explorer-props-perm-error-fg)" }, { tok: "--tasty-explorer-props-perm-locked-fg", use: "new · → text-muted", color: "var(--tasty-explorer-props-perm-locked-fg)" }]} />
        <Note>i18n (new): <code>explorer.props.perm.edit</code> “Edit” / “편집” / “編集” · <code>.perm.read</code> / <code>.write</code> / <code>.execute</code> · <code>.perm.owner</code> / <code>.group</code> / <code>.others</code> · <code>.perm.apply</code> “Apply” / “적용” / “適用” · <code>.perm.applying</code> “Applying…” / “적용 중…” / “適用中…” · <code>.perm.folder_only</code> “Changes this folder only, not the items in it.” · <code>.perm.special_kept</code> “Special bits (setuid, setgid, sticky) are kept.” · <code>.perm.locked</code> “Only the owner can change permissions.” · <code>.perm.failed</code> “Couldn't change permissions” · <code>.perm.read_only</code> “Read-only” · <code>.perm.windows_hint</code> “Windows file attribute. Other access rules aren't changed here.” — ko / ja as in the PA table in <code>gallery/explorer-ops-props-actions.jsx</code>. Cancel reuses <code>common.cancel</code>.</Note>
      </Spec>

      <PermNarrowSpec />

      <Spec id="already-built-context-menu-os-rows-link-note-partial-read-check" title="Already built — context menu OS rows · link note · partial read (check)"
        when={<>Confirmed with two changes. <b>Order</b>: Move to Trash · separator · <b>Open in system</b> · <b>Show in Finder / File Explorer / File Manager</b> (one item only) · separator · Properties — the OS rows get their own group so Properties stays the last row on its own. <b>Label</b>: “Open in system” in sentence case, as the existing menu row already reads (“Show in Finder” keeps the product name). Icons: <code>external</code> for Open in system, <code>folderOpen</code> for Show in …. Remote cells hide both rows and the group's separator. The <b>link note</b> wording and place (muted note under the fields, same as the remote note) are confirmed. <b>Some items couldn't be read</b> under Kinds and <b>—</b> for a remote Total size with folders are confirmed as built.</>}>
        <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
          <K.CtxMenu title="One file · macOS">{ctxTail("Show in Finder")}</K.CtxMenu>
          <K.CtxMenu title="One folder · Windows">{ctxTail("Show in File Explorer")}</K.CtxMenu>
          <K.CtxMenu title="Several items · Linux">{ctxTail(null)}</K.CtxMenu>
          <K.CtxMenu title="Remote cell">{ctxTail(false)}</K.CtxMenu>
          <XCol label="remote · several with a folder" w={360}>
            <PProps glyph={<Icon name="layers" />} name="4 items" note="Remote — only what the remote host lists is shown.">
              <PField k="Kinds" v="3 files, 1 folder" />
              <PField k="Total size" v="—" />
              <PField k="Location" v="build-eu:~/logs" mono copy />
            </PProps>
          </XCol>
        </Stage>
        <Note>Strings unchanged except the label case: <code>explorer.menu.open_in_system</code> “Open in system” / “시스템에서 열기” / “システムで開く” · <code>.reveal_macos</code> “Show in Finder” / “Finder에서 보기” / “Finder で表示” · <code>.reveal_windows</code> “Show in File Explorer” / “파일 탐색기에서 보기” / “エクスプローラーで表示” · <code>.reveal_linux</code> “Show in File Manager” / “파일 관리자에서 보기” / “ファイルマネージャーで表示” · <code>explorer.props.link_note</code> as requested.</Note>
      </Spec>
    </Section>
  );
}

export { PropsActionsSection };
