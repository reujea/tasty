import React from "react";
import { Icon as MIcon, IconButton, AutoComplete, Tooltip, Spinner } from "../../components/index.js";
import { Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Plugins › Markdown viewer › address bar: folder button + editable mirror bar (2026-10-21).
// Replaces the old MdAddressBar in plugins/index.jsx (which now delegates here).
//
// Band (same box as the HTML band, md-address-* → html-address-*): height 40 · bg-sidebar · 1px separator below ·
//   padding-x 8 · gap 8 between groups.
//   local : [Folder]            gap 8 [field: file glyph · path]   gap 8 [Go]
//   remote: [Folder · Reload]   gap 8 [field: remote glyph · path] gap 8 [Go]       (left cluster gap 4)
//   Buttons = shared IconButton ghost sm (24 box, 16 glyph icon-size-md): folderOpen · refresh · arrowRight.
//   Field  = the PathField box (input tokens, input-height 28, mono, leading kind glyph = the surface/mirror marker).
// Narrow: Go hides first when the field would drop below md-address-go-hide-below (160) — Enter still navigates.
//   Then the field shrinks to md-address-field-min-width (96) and ellipsises from the LEFT. Folder / Reload never hide.




const MA_COPY = {
  en: { open: "Open a file…", openRemote: "Open a remote file…", go: "Go", reload: "Reload", stale: "The remote file changed — reload",
    disconnected: "Disconnected from the remote", err: (p) => <>Can't open <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> on the remote: file not found.</>,
    rejected: (p, r) => <>Can't open <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> on the remote: {r}</>,
    lost: () => <>Connection lost before the remote answered. Reconnect and try again.</>,
    cancelled: (p) => <>Didn't open <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span>: the remote connection changed. Try again.</>,
    internal: (p) => <>Couldn't send the request to open <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span>. Try again.</>,
    waiting: "Still waiting for the remote…" },
  ko: { open: "파일 열기…", openRemote: "원격 파일 열기…", go: "이동", reload: "새로 고침", stale: "원격 파일이 바뀌었습니다 — 새로 고침",
    disconnected: "원격 연결이 끊겼습니다", err: (p) => <>원격에서 <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> 파일을 열 수 없습니다: 파일이 없습니다.</>,
    rejected: (p, r) => <>원격에서 <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> 파일을 열 수 없습니다: {r}</>,
    lost: () => <>원격이 응답하기 전에 연결이 끊겼습니다. 다시 연결한 뒤 시도하세요.</>,
    cancelled: (p) => <>원격 연결이 바뀌어 <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> 파일을 열지 않았습니다. 다시 시도하세요.</>,
    internal: (p) => <><span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> 파일을 여는 요청을 보내지 못했습니다. 다시 시도하세요.</>,
    waiting: "원격의 응답을 기다리는 중…" },
  ja: { open: "ファイルを開く…", openRemote: "リモートのファイルを開く…", go: "移動", reload: "再読み込み", stale: "リモートのファイルが変更されました — 再読み込み",
    disconnected: "リモートから切断されました", err: (p) => <>リモートで <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> を開けません: ファイルが見つかりません。</>,
    rejected: (p, r) => <>リモートで <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> を開けません: {r}</>,
    lost: () => <>リモートの応答前に接続が切れました。再接続してからもう一度お試しください。</>,
    cancelled: (p) => <>リモート接続が変わったため <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> を開きませんでした。もう一度お試しください。</>,
    internal: (p) => <><span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p}</span> を開く要求を送信できませんでした。もう一度お試しください。</>,
    waiting: "リモートの応答を待っています…" },
};
const MD_RECENT = ["~/work/tasty/README.md", "~/work/tasty/docs/architecture.md", "~/work/tasty/docs/design/systems/theme.md", "~/work/tasty/CHANGELOG.md"];

const tip = (label, node) => <Tooltip placement="top" content={label}><span style={{ display: "inline-flex", flex: "none" }}>{node}</span></Tooltip>;

// stale Reload: filled accent box, same 24 square + radius as IconButton sm; the only filled thing in the band
function StaleReload({ label }) {
  return tip(label, (
    <button aria-label={label} style={{ width: "var(--tasty-icon-button-size-sm)", height: "var(--tasty-icon-button-size-sm)", display: "inline-flex", alignItems: "center", justifyContent: "center", padding: 0,
      border: 0, borderRadius: "var(--tasty-icon-button-radius)", background: "var(--tasty-md-address-stale-bg)", color: "var(--tasty-md-address-stale-fg)", cursor: "pointer" }}>
      <MIcon name="refresh" />
    </button>
  ));
}

// field — kind glyph (file | remote) + mono path; editing = focus edge + caret; error = invalid edge; off = readonly box
function MdField({ remote, path, editing, error, off }) {
  const border = error ? "var(--tasty-input-border-invalid)" : editing ? "var(--tasty-input-border-focus)" : off ? "var(--tasty-input-readonly-border)" : "var(--tasty-input-border)";
  const ring = error || editing;
  return (
    <div style={{ flex: 1, minWidth: "var(--tasty-md-address-field-min-width)", display: "flex", alignItems: "center", gap: "var(--tasty-input-gap)", height: "var(--tasty-input-height)", boxSizing: "border-box",
      padding: "0 var(--tasty-space-sm)", borderRadius: "var(--tasty-input-radius)", background: off ? "var(--tasty-input-readonly-bg)" : "var(--tasty-input-bg)", border: "var(--tasty-border-width) solid " + border,
      boxShadow: ring ? "0 0 0 var(--tasty-focus-ring-width) color-mix(in srgb, " + (error ? "var(--tasty-input-border-invalid)" : "var(--tasty-border-focus)") + " 35%, transparent)" : "none" }}>
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-input-icon-fg)" }}><MIcon name={remote ? "remote" : "file"} size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ flex: 1, minWidth: 0, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-input-font-size)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", direction: "rtl", textAlign: "left",
        color: off ? "var(--tasty-text-muted-raised)" : editing || error ? "var(--tasty-input-fg)" : "var(--tasty-md-address-value-fg)" }}>
        <bdi>{path}</bdi>
        {(editing || error) && <span style={{ display: "inline-block", width: "var(--tasty-border-width)", height: "var(--tasty-size-14)", background: "var(--tasty-accent-primary)", marginLeft: "var(--tasty-border-width)", verticalAlign: "text-bottom" }} />}
      </span>
    </div>
  );
}

// state: idle | editing | stale | disconnected | error | waiting · noGo = narrow (Go hidden) · suggest = local editing with recent files
// fail (state error): err (file not found) | rejected (+ reason, the remote's English line) | lost | cancelled | internal
function MdAddressBar({ remote = false, state = "idle", path, lang = "en", noGo = false, suggest = false, fail = "err", reason = "" }) {
  const L = MA_COPY[lang] || MA_COPY.en;
  const p = path || (remote ? "/home/deploy/agents-prod/RUNBOOK.md" : "~/work/tasty/docs/architecture.md");
  const off = state === "disconnected";
  const editing = state === "editing" || state === "waiting";
  const kind = fail === "rejected" && !reason ? "internal" : fail;
  const msg = kind === "rejected" ? L.rejected(p, <span lang="en">{reason}</span>) : (L[kind] || L.err)(p);
  const full = kind === "rejected" ? p + ": " + reason : null;
  return (
    <div style={{ flex: "none", display: "flex", flexDirection: "column", background: "var(--tasty-md-address-bar-bg)", borderBottom: "var(--tasty-border-width) solid var(--tasty-md-address-bar-border)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-md-address-bar-gap)", height: "var(--tasty-md-address-bar-height)", padding: "0 var(--tasty-md-address-bar-padding-x)", position: "relative" }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-md-address-bar-nav-gap)", flex: "none" }}>
          {tip(off ? L.disconnected : remote ? L.openRemote : L.open, <IconButton size="sm" aria-label={remote ? L.openRemote : L.open} disabled={off}><MIcon name="folderOpen" /></IconButton>)}
          {remote && (state === "stale" ? <StaleReload label={L.stale} />
            : tip(off ? L.disconnected : L.reload, <IconButton size="sm" aria-label={L.reload} disabled={off}><MIcon name="refresh" /></IconButton>))}
        </div>
        {suggest && !remote
          ? <div style={{ flex: 1, minWidth: "var(--tasty-md-address-field-min-width)", display: "flex" }}><AutoComplete block mono icon={<MIcon name="file" size="var(--tasty-icon-size-sm)" />} rowIcon={<MIcon name="file" size="var(--tasty-icon-size-sm)" />} open activeIndex={1} query="~/work/tasty/docs/" items={MD_RECENT} maxDropdownHeight={160} emptyLabel="No matching path" /></div>
          : <MdField remote={remote} path={p} editing={editing} error={state === "error"} off={off} />}
        {!noGo && tip(L.go, <IconButton size="sm" aria-label={L.go} disabled={off}><MIcon name="arrowRight" /></IconButton>)}
      </div>
      {state === "error" && (
        <div role="alert" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-md-address-error-height)", padding: "0 var(--tasty-md-address-bar-padding-x)", marginTop: "calc(-1 * var(--tasty-space-xs))" }}>
          <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-md-address-error-glyph)" }}><MIcon name="alertCircle" size="var(--tasty-icon-size-sm)" /></span>
          <span title={full || undefined} style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{msg}</span>
        </div>
      )}
      {state === "waiting" && (
        <div role="status" style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: "var(--tasty-md-address-error-height)", padding: "0 var(--tasty-md-address-bar-padding-x)", marginTop: "calc(-1 * var(--tasty-space-xs))" }}>
          <span style={{ display: "inline-flex", flex: "none" }}><Spinner size="var(--tasty-icon-size-sm)" /></span>
          <span style={{ flex: 1, minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-md-address-pending-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{L.waiting}</span>
        </div>
      )}
    </div>
  );
}

// ── gallery section ────────────────────────────────────────────────
const MaPair = ({ children, cols }) => (
  <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-lg)", alignItems: "flex-start" }}>
    {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, t]) => (
      <div key={label} {...(t ? { "data-theme": t } : {})} style={{ flex: "1 1 var(--tasty-size-440)", minWidth: 0, maxWidth: "100%", display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label}</span>{children}
      </div>
    ))}
  </div>
);
const MaCase = ({ label, w, h, children }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", width: w || "100%", maxWidth: "100%" }}>
    <span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{label}</span>
    <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", overflow: "visible", background: "var(--tasty-md-doc-bg)", minHeight: h }}>{children}</div>
  </div>
);
// pinned IconButton states for the spec row (same box the component paints; hover/active = Theme overlays)
function PinnedBtn({ st }) {
  const dis = st === "disabled";
  return (
    <span style={{ width: "var(--tasty-icon-button-size-sm)", height: "var(--tasty-icon-button-size-sm)", display: "inline-flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--tasty-icon-button-radius)",
      background: st === "hover" ? "var(--tasty-icon-button-overlay-hover)" : st === "active" ? "var(--tasty-icon-button-bg-active)" : "transparent",
      color: dis ? "var(--tasty-state-disabled-fg)" : st === "normal" ? "var(--tasty-icon-button-fg)" : "var(--tasty-icon-button-fg-hover)",
      boxShadow: st === "focus" ? "0 0 0 var(--tasty-focus-ring-width) var(--tasty-border-focus)" : "none" }}><MIcon name="folderOpen" /></span>
  );
}
function TipBubble({ children }) {
  return <span style={{ display: "inline-flex", maxWidth: "var(--tasty-size-240)", padding: "var(--tasty-tooltip-padding-y) var(--tasty-tooltip-padding-x)", background: "var(--tasty-tooltip-bg)", border: "var(--tasty-border-width) solid var(--tasty-tooltip-border)",
    borderRadius: "var(--tasty-tooltip-radius)", color: "var(--tasty-tooltip-fg)", fontSize: "var(--tasty-tooltip-font-size)", lineHeight: "var(--tasty-tooltip-line-height)" }}>{children}</span>;
}

function MdAddressSpecs() {

  const st = { padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", flexDirection: "column", alignItems: "stretch", gap: "var(--tasty-space-md)" };
  const LONG = "~/work/tasty/docs/design/systems/very-long-folder-name-for-width-checks/theme-and-tokens.md";
  return (
    <>
      <Spec id="address-bar-folder-button-local-remote" title="Address bar — folder button · local · remote"
        when={<>A <b>folder button</b> (folderOpen) now leads the band; it opens the Tasty file picker at the current document's folder with the Markdown filter, and the picked file replaces this surface's document. It sits <b>outside</b> the field and the field keeps its leading <b>kind glyph</b> (file = local, remote = mirror): the glyph says what the field holds, the button is the action, and the remote glyph is the same mirror marker the HTML band uses. The Markdown mirror field is now <b>editable</b>: type a remote path, Enter or Go replaces the document, Esc reverts. Reload joins the folder button in the <b>left cluster</b>, where the HTML band also keeps it. Raw → and ↻ are retired: every button is the shared <b>IconButton ghost sm</b> with an <code>icons/</code> glyph.</>}>
        <Stage variant="solo" style={st}>
          <MaPair>
            <MaCase label="local · idle"><MdAddressBar /></MaCase>
            <MaCase label="local · editing, recent files open" h="var(--tasty-size-200)"><MdAddressBar suggest /></MaCase>
            <MaCase label="remote · idle"><MdAddressBar remote /></MaCase>
            <MaCase label="remote · editing"><MdAddressBar remote state="editing" path="/home/deploy/agents-prod/docs/ONCALL" /></MaCase>
            <MaCase label="remote · stale (Reload filled)"><MdAddressBar remote state="stale" /></MaCase>
            <MaCase label="remote · disconnected"><MdAddressBar remote state="disconnected" /></MaCase>
            <MaCase label="remote · navigate failed"><MdAddressBar remote state="error" path="/home/deploy/agents-prod/RUNBOK.md" /></MaCase>
          </MaPair>
        </Stage>
        <Meta specs={[["band", "md-address-bar-height 40 · md-address-bar-bg (bg-sidebar) · 1px md-address-bar-border below · padding-x 8 · group gap 8"],
          ["left cluster", "Folder (local) · Folder + Reload (remote), gap md-address-bar-nav-gap 4"], ["buttons", "IconButton ghost sm — 24 box, icon-size-md 16 glyph, radius; hover / active = Theme overlay 8% / 12%; disabled = state-disabled-fg, no overlay"],
          ["field", "PathField box — input-height 28 · pad-x space-sm · input-gap · leading kind glyph icon-size-sm (file | remote) · mono input-font-size · front ellipsis (tail = file name) · flex 1, floor md-address-field-min-width 96"],
          ["Go", "arrowRight (new glyph), trailing, both variants; Enter is the same action"], ["remote editing", "Enter / Go → same surface opens that remote path · Esc reverts · no recent list (local only)"],
          ["stale", "Reload = filled md-address-stale-bg (accent-primary) + md-address-stale-fg (text-on-accent); folder button stays ghost — the fill is the only loud thing in the band"],
          ["disconnected", "field = input-readonly box, path in state-disabled-fg, not editable · Folder · Reload · Go disabled · tooltip on each “Disconnected from the remote”"],
          ["navigate failed", "field keeps the typed path, input-border-invalid edge + ring, still editing · error row under the band (md-address-error-height 24, alertCircle in md-address-error-glyph, caption text-primary) — the HTML band's error row · clears on next keystroke, Esc, or a successful open"],
          ["tooltips", "open TOP, never down into the WebView"]]}
          tokens={[{ tok: "--tasty-md-address-bar-bg", use: "new · → html-address-bar-bg", color: "var(--tasty-md-address-bar-bg)" }, { tok: "--tasty-md-address-stale-bg", use: "new · → accent-primary", color: "var(--tasty-md-address-stale-bg)" }, { tok: "--tasty-md-address-stale-fg", use: "new · → text-on-accent", color: "var(--tasty-md-address-stale-fg)" }, { tok: "--tasty-md-address-error-glyph", use: "new · → html-address-error-glyph", color: "var(--tasty-md-address-error-glyph)" }, { tok: "--tasty-input-readonly-bg", use: "disconnected field", color: "var(--tasty-input-readonly-bg)" }, { tok: "--tasty-input-border-invalid", use: "failed navigate edge", color: "var(--tasty-input-border-invalid)" }]} />
        <Note><b>Markdown mirror vs HTML mirror.</b> Same band box, same left-cluster Reload, same remote glyph as the mirror marker, same error row. The one difference is the field: HTML mirror = read-only box (input-readonly tokens) with no Go; Markdown mirror = the normal editable box with Go. The field box is how a user tells “I can type here” apart.</Note>
      </Spec>

      <Spec id="folder-button-states-tooltip-strings" title="Folder button — states · tooltip strings"
        when={<>The shared IconButton; the only per-surface parts are the glyph and the tooltip. Hover / active use the Theme overlays; focus is the 2px focus ring; disabled (remote disconnected) has no overlay.</>}>
        <Stage variant="solo" style={st}>
          <MaPair>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-lg)", alignItems: "center" }}>
              {["normal", "hover", "active", "focus", "disabled"].map((s) => (
                <span key={s} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm)", background: "var(--tasty-md-address-bar-bg)", borderRadius: "var(--tasty-radius)" }}>
                  <PinnedBtn st={s} /><span style={{ fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" }}>{s}</span>
                </span>
              ))}
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-sm)" }}>
              {["en", "ko", "ja"].map((l) => <React.Fragment key={l}><TipBubble>{MA_COPY[l].open}</TipBubble><TipBubble>{MA_COPY[l].openRemote}</TipBubble><TipBubble>{MA_COPY[l].stale}</TipBubble></React.Fragment>)}
            </div>
          </MaPair>
        </Stage>
        <Note>Strings (tooltip = aria-label without the ellipsis) — <code>markdown.address.open</code>: Open a file… · 파일 열기… · ファイルを開く… — <code>.open_remote</code>: Open a remote file… · 원격 파일 열기… · リモートのファイルを開く… — <code>.go</code>: Go · 이동 · 移動 — <code>.reload</code>: Reload · 새로 고침 · 再読み込み — <code>.stale</code>: The remote file changed — reload · 원격 파일이 바뀌었습니다 — 새로 고침 · リモートのファイルが変更されました — 再読み込み — <code>.disconnected</code>: Disconnected from the remote · 원격 연결이 끊겼습니다 · リモートから切断されました — <code>.error.not_found</code>: Can't open {"{path}"} on the remote: file not found. · 원격에서 {"{path}"} 파일을 열 수 없습니다: 파일이 없습니다. · リモートで {"{path}"} を開けません: ファイルが見つかりません。 Tooltips wrap at the tooltip max-width (size-240); the ja stale string takes two lines, the rest one.</Note>
      </Spec>

      <Spec id="remote-navigate-failed-by-kind-still-waiting" title="Remote navigate failed — by kind · still waiting"
        when={<>The error row is one shape for every failure (alertCircle, caption <code>text-primary</code>, 24 row, field keeps the typed path with the invalid edge); only the sentence changes. Each kind the host can tell apart gets its own sentence, because the right next step differs: <b>file not found</b> (unchanged) · <b>remote refused</b> — the remote's own one-line reason is shown after the colon <b>as sent</b> (English, not translated; it is the only detail the user can act on) · <b>connection lost before the answer</b> — whether it applied is unknown, so the row says reconnect and try again and claims nothing about the document · <b>cancelled because the connection / mapping changed</b> before sending · <b>internal</b> (the host never got the request). No kind reuses “file not found”. After <b>30 s</b> without a result the band shows a <b>status row</b> (Spinner + “Still waiting for the remote…”, <code>md-address-pending-fg</code>) in the error row's place — it is not an error, the remote may still answer; the field keeps the typed path in the editing look. The row goes away when the result arrives (success replaces the document; failure swaps in the error row) and on the same triggers that clear an error (next keystroke, Esc). A late result still applies as today.</>}>
        <Stage variant="solo" style={st}>
          <MaPair>
            <MaCase label="not found (unchanged)"><MdAddressBar remote state="error" path="/home/deploy/agents-prod/RUNBOK.md" /></MaCase>
            <MaCase label="remote refused — reason as sent"><MdAddressBar remote state="error" fail="rejected" reason="permission denied" path="/root/secrets/NOTES.md" /></MaCase>
            <MaCase label="remote refused — long reason ellipsises, tooltip has all"><MdAddressBar remote state="error" fail="rejected" reason="not a markdown file (detected: application/octet-stream)" path="/home/deploy/agents-prod/build/artifact.md" /></MaCase>
            <MaCase label="connection lost before the answer"><MdAddressBar remote state="error" fail="lost" path="/home/deploy/agents-prod/ONCALL.md" /></MaCase>
            <MaCase label="cancelled — connection changed before sending"><MdAddressBar remote state="error" fail="cancelled" path="/home/deploy/agents-prod/ONCALL.md" /></MaCase>
            <MaCase label="internal — host didn't get the request"><MdAddressBar remote state="error" fail="internal" path="/home/deploy/agents-prod/ONCALL.md" /></MaCase>
            <MaCase label="no result after 30 s — status row"><MdAddressBar remote state="waiting" path="/home/deploy/agents-prod/ONCALL.md" /></MaCase>
          </MaPair>
        </Stage>
        <Stage variant="solo" style={st}>
          <MaPair>
            {["ko", "ja"].map((l) => (
              <React.Fragment key={l}>
                <MaCase label={l + " · refused"}><MdAddressBar lang={l} remote state="error" fail="rejected" reason="permission denied" path="/root/secrets/NOTES.md" /></MaCase>
                <MaCase label={l + " · lost"}><MdAddressBar lang={l} remote state="error" fail="lost" /></MaCase>
                <MaCase label={l + " · cancelled"}><MdAddressBar lang={l} remote state="error" fail="cancelled" /></MaCase>
                <MaCase label={l + " · internal"}><MdAddressBar lang={l} remote state="error" fail="internal" /></MaCase>
                <MaCase label={l + " · still waiting"}><MdAddressBar lang={l} remote state="waiting" /></MaCase>
              </React.Fragment>
            ))}
          </MaPair>
        </Stage>
        <Meta specs={[["row", "unchanged error row — md-address-error-height 24 · alertCircle icon-size-sm md-address-error-glyph · caption text-primary · ellipsis at the end, full sentence as tooltip"], ["path", "mono inside the sentence, as the not-found row"], ["reason", "rejected only · the remote's line verbatim after “: ” · not translated (lang=en) · empty reason → internal sentence"], ["waiting row", "same 24 row, role=status · Spinner icon-size-sm · caption md-address-pending-fg (text-secondary on bg-sidebar) · starts at 30 s, no countdown"], ["clears", "error and waiting rows: next keystroke · Esc · successful open (unchanged rule)"]]}
          tokens={[{ tok: "--tasty-md-address-pending-fg", use: "new · → text-secondary", color: "var(--tasty-md-address-pending-fg)" }, { tok: "--tasty-md-address-error-glyph", use: "unchanged", color: "var(--tasty-md-address-error-glyph)" }]} />
        <Note>i18n (new, <code>markdown.address.error.*</code>) — <code>.rejected</code>: Can't open {"{path}"} on the remote: {"{reason}"} · 원격에서 {"{path}"} 파일을 열 수 없습니다: {"{reason}"} · リモートで {"{path}"} を開けません: {"{reason}"} — <code>.connection_lost</code>: Connection lost before the remote answered. Reconnect and try again. · 원격이 응답하기 전에 연결이 끊겼습니다. 다시 연결한 뒤 시도하세요. · リモートの応答前に接続が切れました。再接続してからもう一度お試しください。 — <code>.cancelled</code>: Didn't open {"{path}"}: the remote connection changed. Try again. · 원격 연결이 바뀌어 {"{path}"} 파일을 열지 않았습니다. 다시 시도하세요. · リモート接続が変わったため {"{path}"} を開きませんでした。もう一度お試しください。 — <code>.internal</code>: Couldn't send the request to open {"{path}"}. Try again. · {"{path}"} 파일을 여는 요청을 보내지 못했습니다. 다시 시도하세요. · {"{path}"} を開く要求を送信できませんでした。もう一度お試しください。 — <code>markdown.address.waiting</code>: Still waiting for the remote… · 원격의 응답을 기다리는 중… · リモートの応答を待っています… — existing <code>.error.not_found</code> unchanged. Kind → key: NotFound → not_found · Rejected(reason) → rejected · Disconnected → connection_lost · Cancelled → cancelled · Internal → internal.</Note>
      </Spec>

      <Spec id="narrow-surface-long-paths" title="Narrow surface · long paths"
        when={<>Buttons never shrink. When the field would fall below <b>160</b> (<code>md-address-go-hide-below</code>), <b>Go hides first</b>; Enter still navigates. The field then shrinks to its <b>96</b> floor and ellipsises from the <b>left</b>, so the file name stays visible. Folder and Reload are never hidden: Folder is the only pointer route to the picker, and Reload carries the stale signal. The error row ellipsises at its end; the full message is its tooltip.</>}>
        <Stage variant="solo" style={st}>
          <MaPair>
            <MaCase label="560 · en · local, long path" w="var(--tasty-size-560)"><MdAddressBar path={LONG} /></MaCase>
            <MaCase label="288 · ko · local, Go hidden" w="var(--tasty-size-288)"><MdAddressBar lang="ko" noGo path={LONG} /></MaCase>
            <MaCase label="240 · ja · remote stale, Go hidden" w="var(--tasty-size-240)"><MdAddressBar lang="ja" remote state="stale" noGo /></MaCase>
            <MaCase label="320 · ja · remote navigate failed" w="var(--tasty-size-320)"><MdAddressBar lang="ja" remote state="error" noGo path="/home/deploy/agents-prod/RUNBOK.md" /></MaCase>
          </MaPair>
        </Stage>
        <Meta specs={[["hide order", "Go (field < 160) → nothing else hides"], ["floor", "field 96 = md-address-field-min-width; below that the surface clips the band at its end (no wrap)"], ["local min band", "8 + 24 + 8 + 96 + 8 = 144 without Go"], ["remote min band", "8 + 24 + 4 + 24 + 8 + 96 + 8 = 172 without Go"]]}
          tokens={[{ tok: "--tasty-md-address-go-hide-below", use: "new · → field-width-md 160" }, { tok: "--tasty-md-address-field-min-width", use: "new · → html-address-field-min-width 96" }]} />
        <Do>Keep the tail of the path (the file name) visible: ellipsise the field from the left.</Do>
        <Dont>Drop the folder button to make room; hide Go, which Enter replaces.</Dont>
      </Spec>
    </>
  );
}

export { MdAddressBar, MdAddressSpecs };
