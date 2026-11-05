import React from "react";
import { Input as PfInput, Switch as PfSwitch, HelpHint as PfHelp, Icon as PfIcon } from "../../components/index.js";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Windows › Settings › Terminal › Performance: when saved values apply (2026-10-21).
// Three fixed rows; only the notice copy, role and place change.




const PF = {
  en: { note: "Saved changes apply to terminals you open afterward. Terminals that are already open don't change right away.",
    poll: "Targeted PTY polling", pollHint: "Checks a terminal for output only when it signals new output, instead of checking every terminal.",
    swap: "Scrollback disk swap", swapHint: "Keeps scrollback past the in-memory limit in a temporary file instead of dropping the oldest lines.",
    lines: "Disk scrollback lines", linesHint: "Lines kept on disk per terminal while disk swap is on. -1 keeps every line. Separate from Scrollback lines.",
    unit: "lines", unlimited: "no limit", range: "Use -1 or a number from 0 to 100000000." },
  ko: { note: "저장한 값은 이후 새로 여는 터미널부터 적용됩니다. 이미 열린 터미널은 바로 바뀌지 않습니다.",
    poll: "선택적 PTY 폴링", pollHint: "모든 터미널을 확인하지 않고, 새 출력이 있다고 알린 터미널만 확인합니다.",
    swap: "스크롤백 디스크 스왑", swapHint: "메모리 보관량을 넘은 스크롤백을 버리지 않고 임시 파일에 보관합니다.",
    lines: "디스크 보관 줄 수", linesHint: "디스크 스왑이 켜져 있을 때 터미널마다 디스크에 보관하는 줄 수입니다. -1이면 모두 보관합니다. 스크롤백 줄 수와는 별도입니다.",
    unit: "줄", unlimited: "무제한", range: "-1 또는 0부터 100000000까지의 수를 입력하세요." },
  ja: { note: "保存した値は、このあと新しく開くターミナルから適用されます。すでに開いているターミナルはすぐには変わりません。",
    poll: "選択的 PTY ポーリング", pollHint: "すべてのターミナルを確認せず、新しい出力を知らせたターミナルだけを確認します。",
    swap: "スクロールバックのディスク退避", swapHint: "メモリの保持量を超えたスクロールバックを捨てずに一時ファイルへ保存します。",
    lines: "ディスク保持行数", linesHint: "ディスク退避がオンのとき、ターミナルごとにディスクへ保持する行数です。-1 ですべて保持します。スクロールバック行数とは別です。",
    unit: "行", unlimited: "無制限", range: "-1 または 0〜100000000 の数を入力してください。" },
};

// 2026-10-22 — settings number field: the unit moves OUT of the
// field (as the remote-transfer / pipeline number fields) and the field FITS ITS VALUE: width = max(number-field-min-width 90,
// value length + 1ch caret room + 2 × input-padding-x + 2 × border), capped at the row's available width. Widths are in ch of
// the field's own mono font, so any user font fits. The longest value the config can hold (i64, 20 chars with sign) needs ≈ 151
// with D2Coding 11px, so the cap is reached only in a page narrower than any settings page.
function PfNumField({ value, unit, unlimited, disabled, note, invalid }) {
  const n = Math.max(String(value).length, 1) + 1;
  const ink = disabled ? "var(--tasty-state-disabled-fg)" : "var(--tasty-number-field-unit-fg)";
  return (
    <span style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0, maxWidth: "100%" }}>
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-number-field-unit-gap)", minWidth: 0 }}>
        <span data-number-field style={{ display: "inline-flex", flex: "none", maxWidth: "100%", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", width: "max(var(--tasty-number-field-min-width), calc(" + n + "ch + 2 * var(--tasty-input-padding-x) + 2 * var(--tasty-border-width)))" }}>
          <PfInput block mono value={value} onChange={() => {}} disabled={disabled} invalid={invalid} />
        </span>
        <span data-ink-grade={disabled ? "none" : undefined} data-ink-why={disabled ? "disabled" : undefined} style={{ fontFamily: value === "-1" ? "var(--tasty-font-ui)" : "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: ink, whiteSpace: "nowrap" }}>{value === "-1" ? unlimited : unit}</span>
      </span>
      {note && <span style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-number-field-note-fg)", maxWidth: "var(--tasty-measure-md)" }}>{note}</span>}
    </span>
  );
}

// the page-level note: info glyph + secondary ink, above the three rows (replaces the warning-yellow restart line)
function PfApplyNote({ L }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-settings-apply-note-gap)", maxWidth: "var(--tasty-measure-md)" }}>
      <span style={{ display: "inline-flex", flex: "none", height: "calc(var(--tasty-font-size-caption) * var(--tasty-line-height-ui))", alignItems: "center", color: "var(--tasty-settings-apply-note-glyph)" }}><PfIcon name="info" size="var(--tasty-icon-size-sm)" /></span>
      <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-settings-apply-note-fg)", textWrap: "pretty" }}>{L.note}</p>
    </div>
  );
}
function PfRow({ label, hint, children }) {
  return (
    <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-xs) var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)" }}>
      <span style={{ width: "var(--tasty-settings-label-width)", flex: "none", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)", display: "inline-flex", alignItems: "center", gap: "var(--tasty-help-hint-gap)" }}>
        <span>{label}:</span><PfHelp label={hint} placement="bottom" />
      </span>
      <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>{children}</span>
    </div>
  );
}
function PfPage({ lang = "en", poll = true, swap = true, lines = "10000", w, range, legacy }) {
  const L = PF[lang];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", width: w || "var(--tasty-size-560)", maxWidth: "100%", boxSizing: "border-box" }}>
      <PfApplyNote L={L} />
      <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        <PfRow label={L.poll} hint={L.pollHint}><PfSwitch checked={poll} onChange={() => {}} /></PfRow>
        <PfRow label={L.swap} hint={L.swapHint}><PfSwitch checked={swap} onChange={() => {}} /></PfRow>
        <PfRow label={L.lines} hint={L.linesHint}>
          {legacy ? <span style={{ display: "inline-flex", width: "var(--tasty-field-width-color)" }}><PfInput block mono value={lines} onChange={() => {}} disabled={!swap} addon={L.unit} /></span>
            : <PfNumField value={lines} unit={L.unit} unlimited={L.unlimited} disabled={!swap} note={range ? L.range : null} invalid={!!range} />}
        </PfRow>
      </div>
    </div>
  );
}
const PfPair = ({ children }) => (
  <>{[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
    <div key={label} {...(t ? { "data-theme": t } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)", minWidth: 0, flex: "1 1 var(--tasty-size-560)", maxWidth: "100%" }}>
      <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
    </div>
  ))}</>
);
const PfCase = ({ label, children }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0 }}>
    <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
  </div>
);

function PerfSection() {
  const st = { padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" };
  return (
    <Section id="perfapply" title="Terminal › Performance — when saved values apply">
      <Spec id="apply-note-the-three-rows" title="Apply note + the three rows"
        when={<>The old top line said every change needs a restart, in small <code>accent-warning</code> text. Saved values actually reach the <b>terminals opened afterward</b>; terminals already open do not switch over at save time. That is a fact about timing, not a warning, so the line becomes a neutral <b>note</b>: info glyph + <code>text-secondary</code> caption, at the top of the page above the rows, one sentence pair for all three rows. It does not promise that open terminals never change (a terminal that later reconnects may pick the new polling value), and it names no internal mechanism. The rows keep their order, controls, defaults and the existing HelpHint pattern; their descriptions say what the setting does and do not repeat the timing. <b>Disk scrollback lines</b> keeps default <b>10000</b>, <b>-1 = no limit</b>, stays separate from Scrollback lines (Terminal › General), and is disabled while disk swap is off. A larger valid value from the config file is shown as is and is only written back when the user commits an edit.</>}>
        <Stage variant="solo" style={st}>
          <PfPair>
            <PfCase label="default · swap on · 10000"><PfPage /></PfCase>
            <PfCase label="swap off · lines disabled"><PfPage swap={false} /></PfCase>
            <PfCase label="-1 · no limit"><PfPage lines="-1" /></PfCase>
            <PfCase label="larger config value, shown as read"><PfPage lines="2500000" /></PfCase>
          </PfPair>
        </Stage>
        <Stage variant="solo" style={st}>
          <PfCase label="ko · 560"><PfPage lang="ko" /></PfCase>
          <PfCase label="ja · 560"><PfPage lang="ja" /></PfCase>
          <PfCase label="ja · narrow 360 — note wraps, rows stack label over control"><PfPage lang="ja" w="var(--tasty-size-360)" /></PfCase>
          <PfCase label="ko · narrow 360 · swap off"><PfPage lang="ko" w="var(--tasty-size-360)" swap={false} /></PfCase>
        </Stage>
        <Meta
          specs={[["page", "tab body padding space-lg · note → rows gap space-md · rows gap space-sm"], ["note", "top of the page, above the rows · info glyph icon-size-sm settings-apply-note-glyph (text-muted) · caption settings-apply-note-fg (text-secondary), line-height-ui · gap settings-apply-note-gap 8 · max-width measure-md · wraps, never ellipsises"], ["contrast", "text-secondary on bg-panel: Mocha ≈ 8.0 · Latte ≈ 5.1 (≥ 4.5) · glyph text-muted ≥ 3"], ["rows", "SettingsRow: label settings-label-width + HelpHint (?) · gap settings-label-gap 16 · min-height settings-row-min-height · wrap below the label when narrow"], ["Targeted PTY polling", "Switch · behaviour, default unchanged"], ["Scrollback disk swap", "Switch · behaviour, default unchanged"], ["Disk scrollback lines", "2026-10-22: field fits its value, unit after the field — see the next spec (was Input mono field-width-color 110 + unit addon) · default 10000 · -1 shows “no limit” after the field · disabled (state-disabled) while swap is off · large file values shown verbatim, not clamped on read"], ["not added", "no restart button · no per-terminal status · no popup · no new setting"]]}
          tokens={[{ tok: "--tasty-settings-apply-note-fg", use: "new · → text-secondary", color: "var(--tasty-settings-apply-note-fg)" }, { tok: "--tasty-settings-apply-note-glyph", use: "new · → text-muted", color: "var(--tasty-settings-apply-note-glyph)" }, { tok: "--tasty-settings-apply-note-gap", use: "new · → space-sm" }]} />
        <Note>i18n — retire <code>settings.performance.restart_notice</code>; add <code>settings.performance.apply_notice</code>: “Saved changes apply to terminals you open afterward. Terminals that are already open don't change right away.” · “저장한 값은 이후 새로 여는 터미널부터 적용됩니다. 이미 열린 터미널은 바로 바뀌지 않습니다.” · “保存した値は、このあと新しく開くターミナルから適用されます。すでに開いているターミナルはすぐには変わりません。” A new key, because the meaning changes. Row labels and HelpHint descriptions shown here are the specimen's reading of the three settings: keep the product's existing label / description strings, and drop any “requires restart” wording from them. “no limit” / “무제한” / “無制限” reuses the existing unlimited label if the product has one.</Note>
        <Do>Say when values apply, once, as a neutral note at the top of the page.</Do>
        <Dont>Say a restart is required, or that open terminals never change; don't repeat the timing on every row.</Dont>
      </Spec>
      <PfLinesSpec />
    </Section>
  );
}

function PfLinesSpec() {
  const st = { padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" };
  return (
    <Spec id="disk-scrollback-lines-the-field-fits-its-value-unit-after-the-fi" title="Disk scrollback lines — the field fits its value, unit after the field"
      when={<>At 110 with the unit inside, 9 digits (100000000) lose their last digit and read ten times smaller; neither front- nor end-anchored clipping is safe for a number. So a number is <b>never clipped</b>: the unit word moves <b>after</b> the field (the pattern the remote-transfer and pipeline number fields already use), and the field <b>fits its value</b> — floor <code>number-field-min-width</code> (90, room for 9 digits + caret), growing by one character per digit while the user types and for any value read from the config file. Width is counted in <code>ch</code> of the field's own mono font, so a user-chosen font fits as well. Rest, hover, focus, blur and disabled draw the <b>whole</b> value; editing never scrolls inside the field, so Home / End / arrows only move the caret and Ctrl+A selects all. No ellipsis, no tooltip, no new string. <b>-1</b> shows “no limit” <b>in place of</b> the unit (“-1 lines” is not a count). A value outside -1 … 100000000 is drawn as typed with the field's invalid edge and the range line under it until commit clamps it; a large config value is drawn verbatim and not clamped on read, as before. When the page is narrow the row wraps under its label and the field keeps the same width.</>}>
      <Stage variant="solo" style={st}>
        <PfCase label="before — 110 + unit inside: 9 digits fill the space; with other font metrics the last digit clips (request capture, Linux)"><PfPage lines="100000000" legacy /></PfCase>
      </Stage>
      <Stage variant="solo" style={st}>
        <PfPair>
          <PfCase label="10000 (default) — floor 90"><PfPage /></PfCase>
          <PfCase label="100000000 — max, fits the floor"><PfPage lines="100000000" /></PfCase>
          <PfCase label="4000000000 — 10-digit config value, shown as read"><PfPage lines="4000000000" /></PfCase>
          <PfCase label="-1 — “no limit” replaces the unit"><PfPage lines="-1" /></PfCase>
          <PfCase label="swap off — disabled, whole value"><PfPage lines="4000000000" swap={false} /></PfCase>
          <PfCase label="typed 250000000 — over range, before commit"><PfPage lines="250000000" range /></PfCase>
        </PfPair>
      </Stage>
      <Stage variant="solo" style={st}>
        <PfCase label="ko · 100000000"><PfPage lang="ko" lines="100000000" /></PfCase>
        <PfCase label="ja · 9223372036854775807 — the longest config value"><PfPage lang="ja" lines="9223372036854775807" /></PfCase>
        <PfCase label="ko · narrow 360 · 4000000000 — row stacks, same field"><PfPage lang="ko" lines="4000000000" w="var(--tasty-size-360)" /></PfCase>
        <PfCase label="ja · narrow 360 · over range"><PfPage lang="ja" lines="250000000" range w="var(--tasty-size-360)" /></PfCase>
        <PfCase label="en · narrow 360 · swap off · -1"><PfPage lines="-1" swap={false} w="var(--tasty-size-360)" /></PfCase>
      </Stage>
      <Meta
        specs={[["field", "Input mono, no addon · width = max(number-field-min-width 90, (chars + 1) ch + 2 × input-padding-x + 2 × border) · max-width 100% of the control column · flex none"], ["chars", "length of the text in the field — typed text while editing, the stored value otherwise (sign included)"], ["unit", "after the field, gap number-field-unit-gap 8 · mono caption number-field-unit-fg (text-secondary) · disabled → state-disabled-fg · en lines · ko 줄 · ja 行 · any length (never inside the field)"], ["-1", "“no limit” (UI font, caption) in the unit's place"], ["rest / hover / blur", "whole value, left-aligned · nothing clipped, so no tooltip"], ["focus / editing", "field grows per typed char, no inner scroll · Ctrl+A selects all · Home / End move the caret only"], ["disabled", "swap off · same width rule · state-disabled"], ["out of range", "before commit: invalid edge (accent-danger) + range line under the field, caption number-field-note-fg, max-width measure-md · commit clamps as today"], ["narrow", "row wraps under the label (SettingsRow); field width unchanged; the range line follows the field"], ["cap", "longest i64 value = 20 chars ≈ 151 (D2Coding 11px) — reaches the max-width only in a column narrower than any settings page"]]}
        tokens={[{ tok: "--tasty-number-field-min-width", use: "new · → field-width-xs (90)" }, { tok: "--tasty-number-field-unit-gap", use: "new · → space-sm" }, { tok: "--tasty-number-field-unit-fg", use: "new · → text-secondary", color: "var(--tasty-number-field-unit-fg)" }, { tok: "--tasty-number-field-note-fg", use: "new · → text-secondary", color: "var(--tasty-number-field-note-fg)" }]} />
      <Note>Shared rule: every <b>settings number field</b> uses it — Disk scrollback lines, the remote-transfer and task-pipeline number fields (already 90 + unit outside; they gain only the grow-to-value rule), and any other settings Input that holds an integer. The File Handlers priority field (<code>file-handler-priority-width</code>) keeps its fixed 90: its range is short. Range line copy: if the product already has a range string, keep it; otherwise <code>settings.performance.disk_lines_range</code> “Use -1 or a number from 0 to 100000000.” / “-1 또는 0부터 100000000까지의 수를 입력하세요.” / “-1 または 0〜100000000 の数を入力してください。” The common Input addon defect noted in the request is separate and does not affect this rule (no addon here).</Note>
      <Do>Let a number field fit the whole number; put the unit after it.</Do>
      <Dont>Clip a number at either end, or keep a unit word inside a fixed-width number field.</Dont>
    </Spec>
  );
}

export { PerfSection };
