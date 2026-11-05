import React from "react";
import * as DS from "../../components/index.js";
import { NoticeColumn as Kit_NoticeColumn } from "../../ui_kits/terminal/overlays/restore_notice.jsx";
import * as P from "./loading-overlay.jsx";
import { Spec, Stage, Meta, Note, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Layouts › Loading overlay › Pre-layout + response wait (2026-10-19,
// client pre-layout and response wait). Storyboards built from the loading parts
// (layouts/loading-overlay.jsx) — a normal tab / split appears on the frame after the user's intent,
// the existing LoadingOverlay sits on it, then the same spot turns into the real content.



const { LTab, LRow, TermSurface, LoadingOverlay, THEMES, mono } = P;

const PendingIndicator = DS.PendingIndicator || (() => null);
const cap = { fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
const step = (t) => <span style={{ ...mono, display: "block", marginBottom: "var(--tasty-space-xs)", whiteSpace: "normal" }}>{t}</span>;
const Arrow = () => <span aria-hidden style={{ alignSelf: "center", ...mono, fontSize: "var(--tasty-font-size-body)" }}>→</span>;

function Empty({ loading, rm }) {
  return <div style={{ position: "relative", flex: 1, minWidth: 0, minHeight: 0, background: "var(--tasty-bg-panel)" }}>{LoadingOverlay && <LoadingOverlay kind="surface" visible={!!loading} reducedMotion={rm} label="Attaching…" />}</div>;
}
function Surf({ s, rm }) {
  if (s.type === "empty") return <Empty loading={s.loading} rm={rm} />;
  return <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", outline: s.selected ? "var(--tasty-border-width) solid var(--tasty-border-focus)" : "none", outlineOffset: "calc(-1 * var(--tasty-border-width))" }}><TermSurface w="100%" h="100%" loading={s.loading} rm={rm} /></div>;
}
function Pane({ pane, rm, first }) {
  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", borderLeft: first ? "none" : "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ display: "flex", background: "var(--tasty-surface-raised)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", overflow: "hidden", flex: "none" }}>
        {pane.tabs.map((t, i) => <LTab key={i} label={t.label} kind={t.kind || "terminal"} active={t.active} loading={t.loading} rm={rm} width={t.w || "var(--tasty-size-120)"} />)}
      </div>
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: pane.dir === "column" ? "column" : "row", gap: "var(--tasty-border-width)", background: "var(--tasty-separator)" }}>
        {pane.surfaces.map((s, i) => <Surf key={i} s={s} rm={rm} />)}
      </div>
    </div>
  );
}
// lnb: [{ name, active, loading, status }] · panes: [{ tabs, surfaces, dir }] · pending: { count, elapsed } | null
function Frame({ lnb, panes, pending, rm, w = 600, h = 200, lnbW = true }) {
  const NoticeColumn = (Kit_NoticeColumn) || null;
  return (
    <div style={{ width: w, maxWidth: "100%", height: h, display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden", background: "var(--tasty-bg-app)", flex: "none" }}>
      {lnbW && <div style={{ flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", overflow: "hidden" }}>
        {lnb.map((r, i) => <LRow key={i} name={r.name} active={r.active} loading={r.loading} status={r.status || (r.active ? "running" : "idle")} mirror={r.mirror} rm={rm} />)}
      </div>}
      <div style={{ position: "relative", flex: 1, minWidth: 0, display: "flex" }}>
        {panes.map((p, i) => <Pane key={i} pane={p} rm={rm} first={i === 0} />)}
        {pending && (NoticeColumn
          ? <NoticeColumn topOffset="var(--tasty-tab-height)"><PendingIndicator {...pending} reducedMotion={rm} /></NoticeColumn>
          : <div style={{ position: "absolute", top: "calc(var(--tasty-tab-height) + var(--tasty-space-sm))", right: "var(--tasty-space-sm)" }}><PendingIndicator {...pending} reducedMotion={rm} /></div>)}
      </div>
    </div>
  );
}
const T = (label, o = {}) => ({ label, ...o });
const TERM = (o = {}) => ({ type: "term", ...o });
const EMPTY = (o = {}) => ({ type: "empty", loading: true, ...o });
const WS = (loading, o = {}) => [{ name: "build-farm", active: true, mirror: true, loading, ...o }, { name: "local", status: "idle" }];
const Row = ({ children }) => <div style={{ display: "flex", gap: "var(--tasty-space-md)", alignItems: "flex-start", flexWrap: "wrap" }}>{children}</div>;
const Cell = ({ label, children }) => <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>{step(label)}{children}</div>;
const Themed = ({ attr, children }) => (
  <div data-theme={attr || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-lg)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0 }}>{children}</div>
);

function GalleryLoadingPrelayout() {
  if (!LTab) return null;
  const W = 560, H = 180;
  return (
    <>
      <Spec id="loading-prelayout" title="Pre-layout — the client draws a normal tab / split first, the overlay sits on it"
        when={<>Two different changes, shown apart. <b>(a) Overlay toggle</b> on a fixed target (the specimens above): geometry never changes. <b>(b) Pre-layout</b>: the user asks for a <b>new tab</b>, a <b>surface split</b> or a <b>pane split</b> on an attached workspace; on the <b>next frame</b> the client adds a <b>normal</b> tab / split — same strip, same split geometry, no loading layout, no new SurfaceKind — and puts the existing LoadingOverlay on the new tab's kind-icon slot, the new surface, and the workspace's LNB card. The spinner's 200 delay applies to the spinner only, never to the pre-layout. When the response arrives the same spot turns into the real content. The tab keeps its own title; "Loading…" is never written into it.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexDirection: "column", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={cap}>{label} · new tab</span>
              <Row>
                <Cell label="1 · before"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM()] }]} /></Cell>
                <Arrow />
                <Cell label="2 · intent accepted → normal tab + new surface · loading on tab / surface / LNB"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server"), T("logs"), T("New tab", { active: true, loading: true })], surfaces: [EMPTY()] }]} /></Cell>
                <Arrow />
                <Cell label="3 · response → same tab, real content · loading off"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server"), T("logs"), T("zsh", { active: true })], surfaces: [TERM()] }]} /></Cell>
              </Row>
            </Themed>
          ))}
          <Themed>
            <span style={cap}>Mocha · surface split (inside the tab) and pane split (the layout above it)</span>
            <Row>
              <Cell label="surface split · current tab splits, the new side carries the overlay · tab strip unchanged"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true, loading: true }), T("logs")], surfaces: [TERM(), EMPTY()] }]} /></Cell>
              <Arrow />
              <Cell label="confirmed"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM(), TERM()] }]} /></Cell>
            </Row>
            <Row>
              <Cell label="pane split · a new pane with its OWN tab strip (one tab, loading) and new content"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM()] }, { tabs: [T("New tab", { active: true, loading: true, w: "var(--tasty-size-120)" })], surfaces: [EMPTY()] }]} /></Cell>
              <Arrow />
              <Cell label="confirmed"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM()] }, { tabs: [T("htop", { active: true })], surfaces: [TERM()] }]} /></Cell>
            </Row>
          </Themed>
        </Stage>
        <Meta
          specs={[["timing", "pre-layout on the next frame after intent · spinner after loading-overlay-delay 200 · both off on response"], ["new tab", "normal tab appended after the current one · its own default title · overlay in the kind-icon slot · new surface overlay · LNB card dot slot"], ["surface split", "current tab's split geometry · overlay on the new side only · tab icon + LNB carry it too"], ["pane split", "layout split · new pane = independent tab strip + one tab + new surface"], ["input", "existing surfaces keep content, selection, input, scroll · the new area sends input nowhere until it is real · the user can move to any real area"], ["WebView kind", "tab + LNB only (unchanged rule); native overlay only when it really exists"], ["not", "no loading-only layout · no new SurfaceKind · no 'Loading…' title"]]}
          tokens={[{ tok: "--tasty-loading-overlay-delay", use: "spinner only — never delays the pre-layout" }]} />
      </Spec>

      <Spec id="loading-prelayout-edges" title="Pre-layout — selection moved · several adds · explicit failure · merged results"
        when={<><b>Selection moved</b>: while waiting, the user picks another real tab; when the response lands, the new tab turns real <b>in the background</b> and the user's selection is kept. <b>Several adds</b> to the same pane: each request gets its own tab with its own overlay. <b>Explicit failure</b>: only that tab / split and its overlay go; everything the user did since stays; the reason goes through the existing error notice. <b>Merged results</b> that cannot be matched to requests: the confirmed structure is shown, pre-layout areas that might duplicate it are folded, the LNB card keeps its wait for unfinished requests, and nothing is auto-focused. This is an exception path, never the default for a single add.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexDirection: "column", alignItems: "flex-start" }}>
          <Themed>
            <span style={cap}>Mocha</span>
            <Row>
              <Cell label="selection moved · user picked 'server' while 'New tab' waits"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true }), T("logs"), T("New tab", { loading: true })], surfaces: [TERM()] }]} /></Cell>
              <Arrow />
              <Cell label="response → 'zsh' real in the background · selection stays on 'server'"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs"), T("zsh")], surfaces: [TERM()] }]} /></Cell>
            </Row>
            <Row>
              <Cell label="several adds · two requests, two tabs, two overlays"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server"), T("New tab", { loading: true }), T("New tab", { active: true, loading: true })], surfaces: [EMPTY()] }]} /></Cell>
              <Arrow />
              <Cell label="first answered, second still waiting"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server"), T("zsh"), T("New tab", { active: true, loading: true })], surfaces: [EMPTY()] }]} /></Cell>
            </Row>
            <Row>
              <Cell label="explicit failure of the 2nd add · user had since split 'server'"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true }), T("zsh"), T("New tab", { loading: true })], surfaces: [TERM(), TERM()] }]} /></Cell>
              <Arrow />
              <Cell label="only the failed tab goes · split + selection kept · error via existing notice"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("zsh")], surfaces: [TERM(), TERM()] }]} /></Cell>
            </Row>
            <Row>
              <Cell label="merged results · 3 pre-layout tabs, host returns a combined layout"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true }), T("New tab", { loading: true }), T("New tab", { loading: true }), T("New tab", { loading: true })], surfaces: [TERM()] }]} /></Cell>
              <Arrow />
              <Cell label="confirmed tabs shown · duplicates folded · LNB keeps waiting for the unfinished one · no auto focus"><Frame w={W} h={H} lnb={WS(true)} panes={[{ tabs: [T("server", { active: true }), T("zsh"), T("htop")], surfaces: [TERM()] }]} /></Cell>
            </Row>
          </Themed>
        </Stage>
        <Meta specs={[["selection", "never taken from the user · real tab appears in place, unselected if the user moved"], ["several", "one pre-layout area per request, in request order"], ["failure", "remove that area + its overlay only · later user changes kept · existing error / notification"], ["merged", "show confirmed · fold possible duplicates · LNB wait stays for unfinished · no focus on an uncertain target"]]} />
      </Spec>

      <Spec id="loading-wait" title="Response wait on an attached workspace — PendingIndicator"
        when={<>Requests that make <b>no new area</b> — close, move, restore closed, convert — show a small <b>PendingIndicator</b> in the window's top-right <b>notice column</b> (under the top-right pane's tab strip, 8 in; see Overlays · Popups › Notice column for its neighbours). Text is neutral (“Waiting for response”) plus the <b>elapsed time</b>; with several requests it counts them and shows the <b>oldest</b> wait. It is view-only: no cancel, no retry, no resend. It disappears on completion; failure and disconnect hand over to the existing error notice and reconnect banner. Unrelated output and other completions do not reset the time. It belongs to its workspace: switch away and it is not shown elsewhere; come back and it shows the running time. It never starts because a shell is quiet (password prompt, long command), and on an old server only supported requests are tracked.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexDirection: "column", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={cap}>{label}</span>
              <Row>
                <Cell label="close tab sent · waiting 12s"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM()] }]} pending={{ count: 1, elapsed: 12 }} /></Cell>
                <Cell label="3 pending · oldest 1m 04s · busy + REMOTE + attention unchanged"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true }), T("logs")], surfaces: [TERM()] }]} pending={{ count: 3, elapsed: 64 }} /></Cell>
              </Row>
            </Themed>
          ))}
          <Themed>
            <span style={cap}>Mocha · chip states</span>
            <div style={{ display: "flex", gap: "var(--tasty-space-lg)", alignItems: "center", flexWrap: "wrap" }}>
              {[["1 · 12s", { count: 1, elapsed: 12 }], ["3 · 1m 04s", { count: 3, elapsed: 64 }], ["1 · 1h 02m", { count: 1, elapsed: 3720 }], ["compact", { count: 2, elapsed: 9, compact: true }], ["reduced motion", { count: 1, elapsed: 30, reducedMotion: true }],
                ["ko", { count: 3, elapsed: 64, label: "응답 3건 대기 중", time: "1분 04초" }], ["ja", { count: 1, elapsed: 12, label: "応答を待っています", time: "12秒" }]].map(([c, p]) => (
                <div key={c} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)" }}><span style={mono}>{c}</span><PendingIndicator {...p} /></div>
              ))}
            </div>
            <Row>
              <Cell label="completed → chip gone"><Frame w={W} h={H} lnb={WS(false)} panes={[{ tabs: [T("server", { active: true })], surfaces: [TERM()] }]} /></Cell>
              <Cell label="switched to 'local' → not shown here; back on build-farm → running time"><Frame w={W} h={H} lnb={[{ name: "build-farm", mirror: true, status: "idle" }, { name: "local", active: true }]} panes={[{ tabs: [T("notes", { active: true })], surfaces: [TERM()] }]} /></Cell>
            </Row>
            <Row>
              <Cell label="narrow window · compact chip"><Frame w={320} h={H} lnbW={false} lnb={[]} panes={[{ tabs: [T("server", { active: true })], surfaces: [TERM()] }]} pending={{ count: 2, elapsed: 9, compact: true }} /></Cell>
              <Cell label="reduced motion · still arcs everywhere"><Frame w={W} h={H} rm lnb={WS(true)} panes={[{ tabs: [T("server"), T("New tab", { active: true, loading: true })], surfaces: [EMPTY()] }]} pending={{ count: 1, elapsed: 20 }} /></Cell>
            </Row>
            <Row>
              <Cell label="long titles · ko / ja tabs ellipsise, overlay keeps its slot"><Frame w={W} h={H} lnb={[{ name: "원격 빌드 팜 — 유럽 리전 야간 작업", active: true, mirror: true, loading: true }, { name: "ローカル開発環境（メイン）", status: "idle" }]} panes={[{ tabs: [T("배포 서버 로그 모니터링", { w: "var(--tasty-size-140)" }), T("新しいタブ — 本番ログ監視", { active: true, loading: true, w: "var(--tasty-size-140)" })], surfaces: [EMPTY()] }]} /></Cell>
              <Cell label="UI scale ×1.25"><div style={{ zoom: 1.25 }}><Frame w={440} h={150} lnbW={false} lnb={[]} panes={[{ tabs: [T("server"), T("New tab", { active: true, loading: true })], surfaces: [EMPTY()] }]} pending={{ count: 1, elapsed: 7 }} /></div></Cell>
            </Row>
          </Themed>
        </Stage>
        <Meta
          specs={[["use for", "close · move · restore closed · convert (no new area) — never instead of a pre-layout"], ["box", "24 · pad-x 8 · gap 8 · surface-raised · 1px border-strong · radius 4"], ["content", "spinner 14 (loading-overlay-indicator) · caption 12 label · · · mono tabular time"], ["copy", "Waiting for response · 12s / Waiting for 3 responses · 1m 04s · ko 응답 대기 중 · 12초 / 응답 3건 대기 중 · 1분 04초 · ja 応答を待っています · 12秒 / 3件の応答を待っています · 1分04秒"], ["time", "<60 s → 12s · <1 h → 1m 04s · else 1h 02m · the OLDEST pending request"], ["compact", "column < 200 → spinner + time; label in aria-label / tooltip"], ["tooltip", "one line per pending kind with its own time · never internal ids"], ["show / hide", "after 200 (loading-overlay-delay) · gone on completion · failure → existing error · disconnect → reconnect banner"], ["scope", "per workspace · hidden while another workspace is shown"], ["never", "scrim · full-width bar · cancel / retry / resend · started by shell silence"]]}
          tokens={[{ tok: "--tasty-pending-indicator-height", use: "→ size-24" }, { tok: "--tasty-pending-indicator-padding-x", use: "→ space-sm 8" }, { tok: "--tasty-pending-indicator-gap", use: "→ space-sm 8" }, { tok: "--tasty-pending-indicator-bg", use: "→ surface-raised", color: "var(--tasty-pending-indicator-bg)" }, { tok: "--tasty-pending-indicator-border", use: "→ border-strong", color: "var(--tasty-pending-indicator-border)" }, { tok: "--tasty-pending-indicator-fg", use: "→ text-secondary-raised", color: "var(--tasty-pending-indicator-fg)" }, { tok: "--tasty-pending-indicator-time-fg", use: "→ text-primary", color: "var(--tasty-pending-indicator-time-fg)" }, { tok: "--tasty-pending-indicator-sep-fg", use: "→ text-muted", color: "var(--tasty-pending-indicator-sep-fg)" }, { tok: "--tasty-pending-indicator-spinner-size", use: "→ icon-size-sm 14" }, { tok: "--tasty-pending-indicator-compact-below", use: "→ size-200" }, { tok: "--tasty-pending-indicator-delay", use: "→ loading-overlay-delay 200" }]} />
        <Note><b>Contrast</b> on surface-raised: label text-secondary-raised Mocha ≈ 7.1:1 · Latte n1100 5.17:1 · time text-primary Mocha ≈ 9.5:1 · Latte 5.17:1 · separator text-muted is decorative · spinner arc accent-primary Mocha ≈ 6.9:1 · Latte ≈ 3.6:1 (≥ 3). Chip edge border-strong is the same floating edge as Toast / Banner.</Note>
        <Dont><b>Don't</b> start the wait because a terminal has been silent, and don't use the chip in place of the pre-layout for a new tab or split.</Dont>
      </Spec>
    </>
  );
}

export { GalleryLoadingPrelayout };
