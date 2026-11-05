import React from "react";
import * as DSA from "../../../components/index.js";

// Tasty UI kit — Settings › Misc › Anomaly detection (2026-10-27).
// Mirrors zilhak/tasty → crates/tasty-settings/src/anomaly_detection.rs ([anomaly_detection], 9 fields).
// L2 order under Misc: Scripts → Task pipeline → Anomaly detection → Tastyrc (Windows only).
// One page = info notice + three sections (Memory growth · Repeated calls · Call bursts); each section =
// mono micro heading · notify Switch row (+ caption naming the detector) · two number rows (+ caption).
// Notify off hides notifications only: number rows keep the normal look and stay editable.
// Number rows reuse the settings number field (number-field-*): no clamp while typing; Enter / blur commits,
// clamping into range; a non-number reverts to the last committed value. The range line shows only while
// the typed value is out of range and names the value it will commit as.



const ANOMALY_FIELDS = [
  { id: "rss_surge", rows: [
    { key: "rss_surge_notify", kind: "switch", def: true },
    { key: "rss_surge_samples", min: 3, max: 20, def: 5 },
    { key: "rss_surge_min_mib", min: 0, max: 65536, def: 256 }] },
  { id: "slow_loop", rows: [
    { key: "slow_loop_notify", kind: "switch", def: false },
    { key: "slow_loop_window_secs", min: 30, max: 3600, def: 300 },
    { key: "slow_loop_threshold", min: 2, max: 10000, def: 20 }] },
  { id: "call_burst", rows: [
    { key: "call_burst_notify", kind: "switch", def: true },
    { key: "call_burst_window_secs", min: 1, max: 600, def: 60 },
    { key: "call_burst_threshold", min: 10, max: 100000, def: 1000 }] },
];
const ANOMALY_DEFAULTS = Object.fromEntries(ANOMALY_FIELDS.flatMap((s) => s.rows.map((r) => [r.key, r.def])));

// en is the source copy; ko / ja carry the meaning for translators. Detector names (rss_surge …) are never translated.
const ANOMALY_COPY = {
  en: {
    notice: "These detectors look for patterns that often mean trouble, but normal work can trigger them too. Turning a detector's notifications off only hides them: it keeps detecting and recording, and its values below still apply. Saved changes apply from the next check.",
    show: "Show notifications:", detector: "Detector",
    range: (a, b, v) => "Between " + a + " and " + b + ". Commits as " + v + ".",
    sec: {
      rss_surge: { h: "Memory growth", c: "Flags a process whose memory (RSS) keeps rising across consecutive samples while above the minimum size." },
      slow_loop: { h: "Repeated calls", c: "Flags an agent that makes the same call (same method, same arguments) many times within the window. It counts repeats, not slow responses." },
      call_burst: { h: "Call bursts", c: "Flags an agent that makes a very large number of calls of any kind within the window." },
    },
    row: {
      rss_surge_samples: ["Rising samples in a row:", "samples", "How many consecutive rising samples count as growth. Default 5."],
      rss_surge_min_mib: ["Minimum RSS:", "MiB", "Processes below this size are not flagged. 0 checks every size. Default 256."],
      slow_loop_window_secs: ["Window:", "s", "The period repeats are counted over. Default 300."],
      slow_loop_threshold: ["Repeats of one call:", "times", "Repeats within the window that count as a loop. Default 20."],
      call_burst_window_secs: ["Window:", "s", "The period calls are counted over. Default 60."],
      call_burst_threshold: ["Calls in window:", "calls", "Calls within the window that count as a burst. Default 1000."],
    },
  },
  ko: {
    notice: "이 탐지기들은 문제일 가능성이 큰 패턴을 살피지만, 정상적인 작업에서도 걸릴 수 있습니다. 탐지기의 알림을 끄면 알림만 숨겨집니다. 탐지와 기록은 계속되고 아래 값도 그대로 적용됩니다. 저장한 변경은 다음 판정부터 적용됩니다.",
    show: "알림 표시:", detector: "탐지기",
    range: (a, b, v) => a + "~" + b + " 사이의 값입니다. " + v + "(으)로 확정됩니다.",
    sec: {
      rss_surge: { h: "메모리 증가", c: "연속된 샘플에서 메모리(RSS)가 계속 늘면서 최소 크기를 넘은 프로세스를 알립니다." },
      slow_loop: { h: "반복 호출", c: "에이전트가 판정 창 안에서 같은 호출(같은 메서드, 같은 인자)을 여러 번 반복하면 알립니다. 응답이 느린지가 아니라 반복 횟수를 셉니다." },
      call_burst: { h: "호출 폭주", c: "에이전트가 판정 창 안에서 종류와 관계없이 매우 많은 호출을 하면 알립니다." },
    },
    row: {
      rss_surge_samples: ["연속 증가 샘플:", "개", "몇 번 연속으로 늘어야 증가로 볼지 정합니다. 기본값 5."],
      rss_surge_min_mib: ["최소 RSS:", "MiB", "이 크기보다 작은 프로세스는 알리지 않습니다. 0이면 모든 크기를 봅니다. 기본값 256."],
      slow_loop_window_secs: ["판정 창:", "초", "반복을 세는 기간입니다. 기본값 300."],
      slow_loop_threshold: ["같은 호출 반복:", "회", "판정 창 안에서 이만큼 반복되면 반복 호출로 봅니다. 기본값 20."],
      call_burst_window_secs: ["판정 창:", "초", "호출을 세는 기간입니다. 기본값 60."],
      call_burst_threshold: ["호출 횟수:", "회", "판정 창 안에서 이만큼 호출하면 폭주로 봅니다. 기본값 1000."],
    },
  },
  ja: {
    notice: "これらの検出器は問題の兆候になりやすいパターンを監視しますが、通常の作業でも反応することがあります。検出器の通知をオフにしても通知が表示されなくなるだけで、検出と記録は続き、下の値もそのまま適用されます。保存した変更は次の判定から適用されます。",
    show: "通知を表示:", detector: "検出器",
    range: (a, b, v) => a + "〜" + b + " の範囲です。" + v + " として確定します。",
    sec: {
      rss_surge: { h: "メモリ増加", c: "連続したサンプルでメモリ (RSS) が増え続け、最小サイズを超えたプロセスを知らせます。" },
      slow_loop: { h: "繰り返し呼び出し", c: "エージェントが判定ウィンドウ内で同じ呼び出し (同じメソッド・同じ引数) を何度も繰り返すと知らせます。応答の遅さではなく、繰り返しの回数を数えます。" },
      call_burst: { h: "呼び出しの急増", c: "エージェントが判定ウィンドウ内で種類を問わず非常に多くの呼び出しを行うと知らせます。" },
    },
    row: {
      rss_surge_samples: ["連続増加サンプル:", "個", "何回連続で増えたら増加とみなすかを決めます。既定値 5。"],
      rss_surge_min_mib: ["最小 RSS:", "MiB", "このサイズ未満のプロセスは知らせません。0 ですべてのサイズが対象です。既定値 256。"],
      slow_loop_window_secs: ["判定ウィンドウ:", "秒", "繰り返しを数える期間です。既定値 300。"],
      slow_loop_threshold: ["同じ呼び出しの繰り返し:", "回", "判定ウィンドウ内でこの回数繰り返すと反復とみなします。既定値 20。"],
      call_burst_window_secs: ["判定ウィンドウ:", "秒", "呼び出しを数える期間です。既定値 60。"],
      call_burst_threshold: ["呼び出し回数:", "回", "判定ウィンドウ内でこの回数に達すると急増とみなします。既定値 1000。"],
    },
  },
};

const anCap = { margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-md)", textWrap: "pretty", overflowWrap: "anywhere" };

// one settings Row: side by side (label column · control) or stacked (settings_stack_row: label line, control under it)
function AnRow({ label, stacked, labelCol, caption, children }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-caption-gap)", minWidth: 0 }}>
      <div style={{ display: "flex", flexDirection: stacked ? "column" : "row", alignItems: stacked ? "stretch" : "center", gap: stacked ? "var(--tasty-settings-row-stack-gap)" : "var(--tasty-settings-label-gap)", minHeight: "var(--tasty-settings-row-min-height)", minWidth: 0 }}>
        <span style={{ flex: "none", width: stacked ? "auto" : labelCol, fontSize: "var(--tasty-font-size-body)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)", overflowWrap: "anywhere" }}>{label}</span>
        <span style={{ display: "flex", flexDirection: "column", alignItems: "flex-start", gap: "var(--tasty-space-xs)", minWidth: 0 }}>{children}</span>
      </div>
      {caption}
    </div>
  );
}

function AnNumField({ text, unit, invalid, note, focused, onChange, onCommit }) {
  const { Input } = DSA;
  const n = Math.max(String(text).length, 1) + 1;
  return (
    <>
      <span style={{ display: "flex", alignItems: "center", gap: "var(--tasty-number-field-unit-gap)", minWidth: 0 }}>
        <span style={{ display: "inline-flex", flex: "none", maxWidth: "100%", width: "max(var(--tasty-number-field-min-width), calc(" + n + "ch + 2 * var(--tasty-input-padding-x) + 2 * var(--tasty-border-width)))" }}>
          <Input block mono value={text} invalid={invalid} focused={focused} inputMode="numeric"
            onChange={(e) => onChange && onChange(e.target.value)} onBlur={() => onCommit && onCommit()}
            onKeyDown={(e) => { if (e.key === "Enter" && onCommit) onCommit(); }} style={{ textAlign: "right" }} />
        </span>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-number-field-unit-fg)", whiteSpace: "nowrap" }}>{unit}</span>
      </span>
      {note && <span role="status" style={{ fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-number-field-note-fg)", maxWidth: "var(--tasty-measure-md)", overflowWrap: "anywhere" }}>{note}</span>}
    </>
  );
}

// values: committed draft (ANOMALY_DEFAULTS by default). texts: per-key typed text (static specimens show
// an uncommitted entry this way). focusKey: paint that field focused without DOM focus.
function AnomalySubtab({ lang = "en", values, texts, focusKey, stacked = false, labelCol = "var(--tasty-settings-label-width)", width, onDraft }) {
  const { Switch, Icon } = DSA;
  const L = ANOMALY_COPY[lang] || ANOMALY_COPY.en;
  const [vals, setVals] = React.useState(() => Object.assign({}, ANOMALY_DEFAULTS, values));
  const [txt, setTxt] = React.useState(() => Object.assign({}, texts));
  const setVal = (k, v) => setVals((s) => { const n = Object.assign({}, s, { [k]: v }); onDraft && onDraft(n); return n; });
  const textOf = (k) => (txt[k] !== undefined ? txt[k] : String(vals[k]));
  const parse = (t) => (/^\s*-?\d+\s*$/.test(t) ? parseInt(t, 10) : null);
  const clamp = (r, v) => Math.min(r.max, Math.max(r.min, v));
  const commit = (r) => {
    const v = parse(textOf(r.key));
    if (v !== null) setVal(r.key, clamp(r, v));
    setTxt((s) => { const n = Object.assign({}, s); delete n[r.key]; return n; });
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xl)", width: width || "100%", maxWidth: "var(--tasty-settings-content-max-width)", minWidth: 0 }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-settings-apply-note-gap)", maxWidth: "var(--tasty-measure-md)" }}>
        <span style={{ display: "inline-flex", flex: "none", height: "calc(var(--tasty-font-size-caption) * var(--tasty-line-height-ui))", alignItems: "center", color: "var(--tasty-settings-apply-note-glyph)" }}><Icon name="info" size="var(--tasty-icon-size-sm)" /></span>
        <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-settings-apply-note-fg)", textWrap: "pretty" }}>{L.notice}</p>
      </div>
      {ANOMALY_FIELDS.map((s) => (
        <section key={s.id} aria-label={L.sec[s.id].h} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-settings-row-gap)", minWidth: 0 }}>
          <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase", letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>{L.sec[s.id].h}</div>
          {s.rows.map((r, i) => {
            const sep = i > 0 && <div style={{ height: "var(--tasty-border-width)", background: "var(--tasty-separator)" }} />;
            if (r.kind === "switch") return (
              <React.Fragment key={r.key}>{sep}
                <AnRow label={L.show} stacked={stacked} labelCol={labelCol}
                  caption={<p style={anCap}>{L.sec[s.id].c} {L.detector}: <code style={{ fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" }}>{s.id}</code>.</p>}>
                  <Switch checked={!!vals[r.key]} onChange={(e) => setVal(r.key, e && e.target ? e.target.checked : !vals[r.key])} aria-label={L.sec[s.id].h + " — " + L.show.replace(/:$/, "")} />
                </AnRow>
              </React.Fragment>
            );
            const [lab, unit, cap] = L.row[r.key];
            const t = textOf(r.key); const v = parse(t);
            const out = v !== null && (v < r.min || v > r.max);
            return (
              <React.Fragment key={r.key}>{sep}
                <AnRow label={lab} stacked={stacked} labelCol={labelCol} caption={<p style={anCap}>{cap}</p>}>
                  <AnNumField text={t} unit={unit} invalid={out} focused={focusKey === r.key} note={out ? L.range(r.min, r.max, clamp(r, v)) : null}
                    onChange={(x) => setTxt((st) => Object.assign({}, st, { [r.key]: x }))} onCommit={() => commit(r)} />
                </AnRow>
              </React.Fragment>
            );
          })}
        </section>
      ))}
    </div>
  );
}

export { AnomalySubtab, ANOMALY_FIELDS, ANOMALY_DEFAULTS, ANOMALY_COPY };
