import React from "react";
import { AnomalySubtab as Kit_AnomalySubtab } from "../../ui_kits/terminal/overlays/settings_anomaly.jsx";
import { Section, Spec, Stage, Meta, Note, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Windows › Settings › Misc › Anomaly detection (2026-10-27,
//). Every frame is the live kit part
// ui_kits/terminal/overlays/settings_anomaly.jsx (TastyKit.AnomalySubtab) — no second copy.


const AK = () => ({ AnomalySubtab: Kit_AnomalySubtab });
const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
const lbl = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };

// a settings content column: bg-panel, content inset space-lg (as the real page)
function AnFrame({ theme, label, w = "var(--tasty-size-560)", children }) {
  return (
    <div data-theme={theme || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", minWidth: 0, maxWidth: "100%", background: "var(--tasty-bg-app)" }}>
      <span style={lbl}>{label}</span>
      <div style={{ width: w, maxWidth: "100%", boxSizing: "border-box", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)" }}>{children}</div>
    </div>
  );
}
const stage = (children) => <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>{children}</Stage>;
const ALL_ON = { slow_loop_notify: true };
const MIN = { rss_surge_samples: 3, rss_surge_min_mib: 0, slow_loop_window_secs: 30, slow_loop_threshold: 2, call_burst_window_secs: 1, call_burst_threshold: 10 };
const MAX = { rss_surge_samples: 20, rss_surge_min_mib: 65536, slow_loop_window_secs: 3600, slow_loop_threshold: 10000, call_burst_window_secs: 600, call_burst_threshold: 100000 };

function AnomalySection() {
  const { AnomalySubtab: A } = AK();
  if (!A) return <Note>Kit part not loaded (overlays/settings_anomaly.jsx).</Note>;
  return (
    <Section id="anomaly" title="Settings · Misc › Anomaly detection">
      <Spec id="settings-anomaly" title="Default — nine values, notifications on / off / on"
        when={<>A new Misc L2 subtab <b>Anomaly detection</b>, after <b>Task pipeline</b> and before <b>Tastyrc</b> (Windows). The page opens with one <b>info notice</b> (the Performance apply-note recipe): these are heuristic signals that normal work can trigger, turning notifications off hides them only, and saved values apply from the next check. Then three sections in fixed order — <b>Memory growth</b> · <b>Repeated calls</b> · <b>Call bursts</b>. Each is a mono micro heading, the <b>Show notifications</b> Switch row, then the two number rows in backend order, with a 1px separator between rows. The switch row's caption says what the detector flags in plain words and ends with its internal name in mono (<code>Detector: slow_loop.</code>). Each number row has its own caption ending in its default. By default only Repeated calls is off. The first two frames are live: type, then Enter or click away.</>}>
        {stage(THEMES.map(([l, t]) => <AnFrame key={l} theme={t} label={l + " · default (true / false / true) · live"}><A /></AnFrame>))}
        <Meta
          specs={[["L2 position", "Misc · Scripts → Task pipeline → Anomaly detection → Tastyrc (Windows)"], ["notice", "info glyph 14 (settings-apply-note-glyph) · caption text-secondary · measure-md · gap 8"], ["notice → section · section → section", "space-xl 24"], ["section", "heading mono micro uppercase muted · rows gap settings-row-gap 12 · 1px separator between rows"], ["row", "label column settings-label-width 150 (subtab's longest en label fits) · gap 16 · min-height 32"], ["switch row caption", "what it flags · “Detector: <internal name>” (mono, text-secondary) · caption 11 muted · measure-md · gap 4"], ["number row", "settings number field (min 90, grows to value) · unit after, mono caption text-secondary · caption ends “Default N.”"], ["order", "notify switch first, then the two numbers as in [anomaly_detection]"]]}
          tokens={[{ tok: "--tasty-settings-apply-note-fg", use: "notice text → text-secondary", color: "var(--tasty-settings-apply-note-fg)" }, { tok: "--tasty-settings-apply-note-glyph", use: "notice glyph → text-muted", color: "var(--tasty-settings-apply-note-glyph)" }, { tok: "--tasty-space-xl", use: "between notice and sections (24)" }, { tok: "--tasty-settings-row-gap", use: "rows within a section (12)" }, { tok: "--tasty-settings-row-caption-gap", use: "row → its caption (4)" }, { tok: "--tasty-settings-label-width", use: "label column 150" }, { tok: "--tasty-settings-label-gap", use: "label → control 16" }, { tok: "--tasty-number-field-min-width", use: "field floor 90" }, { tok: "--tasty-number-field-unit-fg", use: "unit → text-secondary", color: "var(--tasty-number-field-unit-fg)" }, { tok: "--tasty-separator", use: "between rows", color: "var(--tasty-separator)" }]} />
        <Note>No new tokens. Every value above is an existing settings / number-field / apply-note role.</Note>
      </Spec>

      <Spec id="settings-anomaly-off" title="All on · notifications off while editing a number"
        when={<>Turning <b>Show notifications</b> off changes the Switch only. The two number rows keep their normal ink and box, stay focusable and editable, and keep applying: the detector still runs and records, it just doesn't notify. That rule is stated once, in the page notice; the rows are not dimmed or disabled, so they don't read as switched off.</>}>
        {stage(<>
          {THEMES.map(([l, t]) => <AnFrame key={"on" + l} theme={t} label={l + " · all three on"}><A values={ALL_ON} /></AnFrame>)}
          {THEMES.map(([l, t]) => <AnFrame key={"off" + l} theme={t} label={l + " · Repeated calls off · editing its threshold (40, focused, uncommitted)"}><A texts={{ slow_loop_threshold: "40" }} focusKey="slow_loop_threshold" /></AnFrame>)}
        </>)}
        <Meta specs={[["notify off", "Switch off only · number rows unchanged (no dim, no disabled ink) · editable · values stay valid"], ["meaning", "notifications hidden · detection + recording continue · no history reset · no re-notify of past events (page notice)"]]} />
        <Dont><b>Don't</b> dim or disable the number rows of a section whose notifications are off, and don't describe off as “detection stopped”.</Dont>
      </Spec>

      <Spec id="settings-anomaly-range" title="Out of range · after commit · bounds"
        when={<>Typing never corrects the value. While the typed number is outside the range the field gets the <b>invalid</b> edge and one line under it names the range <b>and the value it will commit as</b>. <span className="ic">Enter</span> or leaving the field commits: the value is clamped and the line goes away. A non-number reverts to the last committed value; there is no error popup. Values at either bound (including <code>Minimum RSS 0</code>) are valid and show no line.</>}>
        {stage(<>
          {THEMES.map(([l, t]) => <AnFrame key={"bad" + l} theme={t} label={l + " · Repeats of one call = 1 (typed, focused)"}><A texts={{ slow_loop_threshold: "1" }} focusKey="slow_loop_threshold" /></AnFrame>)}
          {THEMES.map(([l, t]) => <AnFrame key={"ok" + l} theme={t} label={l + " · after Enter / blur → committed 2"}><A values={{ slow_loop_threshold: 2 }} /></AnFrame>)}
          <AnFrame label="Mocha · every lower bound (valid, no range line)"><A values={MIN} /></AnFrame>
          <AnFrame theme="latte" label="Latte · every upper bound (valid, no range line)"><A values={Object.assign({}, MAX, ALL_ON)} /></AnFrame>
        </>)}
        <Meta specs={[["while typing", "no clamp · out of range → invalid edge + range line"], ["range line", "under the field, in the control column · caption 11 · number-field-note-fg · “Between 2 and 10000. Commits as 2.”"], ["commit", "Enter or blur · clamp into range · line removed"], ["non-number", "reverts to last committed value · no popup"], ["bounds", "3–20 · 0–65536 MiB · 30–3600 s · 2–10000 · 1–600 s · 10–100000"]]}
          tokens={[{ tok: "--tasty-number-field-note-fg", use: "range line → text-secondary", color: "var(--tasty-number-field-note-fg)" }, { tok: "--tasty-accent-danger", use: "invalid field edge (Input invalid)", color: "var(--tasty-accent-danger-raised)" }]} />
      </Spec>

      <Spec id="settings-anomaly-narrow" title="Narrow · ko / ja"
        when={<>The page reuses <b>settings_stack_row</b> as is. Each row decides on its own: when its control does not fit beside the label column it stacks (label on its own line, control under it at the row start, gap 4) and returns side by side only with 16 px to spare. Headings, captions and the range line always wrap and are never cut; the field and its unit stay on one line. ko / ja keep the internal names untranslated.</>}>
        {stage(<>
          <AnFrame label="Mocha · en · content 288 · stacked" w="var(--tasty-size-320)"><A stacked texts={{ slow_loop_threshold: "1" }} /></AnFrame>
          <AnFrame theme="latte" label="Latte · ko · content 408" w="var(--tasty-size-440)"><A lang="ko" /></AnFrame>
          <AnFrame label="Mocha · ja · content 288 · stacked" w="var(--tasty-size-320)"><A lang="ja" stacked texts={{ call_burst_threshold: "5" }} /></AnFrame>
        </>)}
        <Meta specs={[["switch", "per Row · control natural width > content − label column − settings-label-gap → stacked"], ["back", "side by side only with settings-row-stack-hysteresis 16 spare"], ["stacked", "label full width · control at row start · gap settings-row-stack-gap 4 · caption + range line follow at the row start"], ["wrap", "heading · label · caption · range line wrap anywhere · field + unit nowrap"]]}
          tokens={[{ tok: "--tasty-settings-row-stack-gap", use: "→ space-xs 4" }, { tok: "--tasty-settings-row-stack-hysteresis", use: "→ space-lg 16" }]} />
        <Note>Copy keys (en source, ko / ja in the kit): <code>settings.anomaly.notice</code> · <code>…show_notifications</code> · <code>…detector</code> · <code>…range</code> ({"{min} {max} {value}"}) · <code>…rss_surge.title / .caption</code> · <code>…slow_loop.title / .caption</code> · <code>…call_burst.title / .caption</code> · per field <code>…&lt;key&gt;.label / .unit / .caption</code>. Detector names <code>rss_surge</code> · <code>slow_loop</code> · <code>call_burst</code> are literal, never translated.</Note>
        <Note><b>Decisions (§6).</b> 1 — Section titles are human names only; the internal name appears once, at the end of the switch row's caption, in mono, the same way in all three sections. 2 — The heuristic / false-positive notice is page-level, at the top (one place, applies to all three); the repeat-not-latency explanation for slow_loop is in its own switch caption. 3 — Number rows of a section with notifications off look and behave normally; the notice carries “keeps detecting, values still apply”. 4 — No restore-defaults button (no settings precedent, and Cancel already drops the draft); each number caption names its default instead. 5 — settings_stack_row per Row with its existing switch rule and hysteresis. 6 — No new tokens.</Note>
      </Spec>
    </Section>
  );
}

export { AnomalySection };
