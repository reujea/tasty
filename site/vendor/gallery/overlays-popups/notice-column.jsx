import React from "react";
import * as DS from "../../components/index.js";
import { NoticeColumn as Kit_NoticeColumn, RestoreNotice as Kit_RestoreNotice } from "../../ui_kits/terminal/overlays/restore_notice.jsx";
import { Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Popups › Notice column: restore-failure / save-halt non-focus notice +
// response-wait chip placement (2026-10-19). Kit: ui_kits/terminal/overlays/restore_notice.jsx.



const { Icon, NotificationItem, NotificationBadgeGroup } = DS;
const PendingIndicator = DS.PendingIndicator || (() => null); // until the bundle recompiles
const K = () => ({ NoticeColumn: Kit_NoticeColumn, RestoreNotice: Kit_RestoreNotice });

const THEMES = [["Mocha", "mocha"], ["Latte", "latte"]];
const cap = { fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
const mono = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: "var(--tasty-text-muted)" };
const LINES = ["~ $ ls", "Cargo.toml  crates  docs  src", "~ $ cargo build", "   Compiling tasty v0.9.4", "~ $ ▌"];
const ANNOT = { outline: "var(--tasty-border-width) dashed var(--tasty-accent-move)", outlineOffset: "var(--tasty-size-2)" };

function Term({ lines = LINES, search }) {
  return (
    <div style={{ position: "relative", flex: 1, minWidth: 0, padding: "var(--tasty-space-sm)", background: "var(--tasty-surface-terminal-focused-bg, var(--tasty-bg-panel))", fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", lineHeight: 1.5, color: "var(--tasty-text-secondary)", overflow: "hidden" }}>
      {lines.map((l, i) => <div key={i} style={{ whiteSpace: "nowrap" }}>{l}</div>)}
      {search && <div style={{ position: "absolute", top: "var(--tasty-space-sm)", right: "var(--tasty-space-sm)", width: "var(--tasty-size-240)", maxWidth: "calc(100% - var(--tasty-space-md))", height: "var(--tasty-size-32)", boxSizing: "border-box", display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", padding: "0 var(--tasty-space-sm)", background: "var(--tasty-surface-raised)", border: "var(--tasty-border-width) solid var(--tasty-search-bar-border)", borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-muted)", fontSize: "var(--tasty-font-size-caption)", fontFamily: "var(--tasty-font-ui)" }}><Icon name="search" size="var(--tasty-icon-size-xs)" /><span style={{ color: "var(--tasty-text-primary)" }}>cargo</span><span style={{ marginLeft: "auto" }}>1/2</span></div>}
    </div>
  );
}
function Strip({ tabs = ["main"] }) {
  return (
    <div style={{ flex: "none", height: "var(--tasty-tab-height)", display: "flex", background: "var(--tasty-surface-raised)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      {tabs.map((t, i) => <span key={t} style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-tab-gap)", padding: "0 var(--tasty-tab-padding-x)", fontSize: "var(--tasty-font-size-caption)", color: i === 0 ? "var(--tasty-tab-fg-active)" : "var(--tasty-tab-fg)", background: i === 0 ? "var(--tasty-tab-bg-active)" : "transparent", borderRight: "var(--tasty-border-width) solid var(--tasty-tab-separator)" }}><Icon name="terminal" size="var(--tasty-tab-icon-size)" />{t}</span>)}
    </div>
  );
}
function Banner({ text }) {
  return (
    <div style={{ position: "absolute", top: "calc(var(--tasty-tab-height) + var(--tasty-banner-margin))", left: "var(--tasty-banner-margin)", right: "var(--tasty-banner-margin)", height: "var(--tasty-size-32)", zIndex: 31, display: "flex", alignItems: "center", gap: "var(--tasty-banner-gap)", padding: "0 var(--tasty-banner-padding-x)", background: "var(--tasty-banner-bg)", border: "var(--tasty-border-width) solid var(--tasty-banner-border)", borderRadius: "var(--tasty-banner-radius)", boxShadow: "var(--tasty-banner-shadow)", fontSize: "var(--tasty-banner-title-font-size)", color: "var(--tasty-banner-fg)" }}>
      <span style={{ display: "inline-flex", color: "var(--tasty-accent-warning-glyph)" }}><Icon name="alertTriangle" size="var(--tasty-icon-size-sm)" /></span>{text}
    </div>
  );
}
// Window: LNB strip (summary dot) · work area with panes. The notice column is relative to the WORK AREA.
function Win({ w = 640, h = 300, split, search, banner, notifErr, children, annotate, colTop = "var(--tasty-tab-height)" }) {
  const NoticeColumn = K().NoticeColumn || (({ children }) => <div>{children}</div>);
  return (
    <div style={{ width: w, maxWidth: "100%", height: h, display: "flex", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden", background: "var(--tasty-bg-app)" }}>
      <div style={{ width: 120, flex: "none", display: "flex", flexDirection: "column", background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
        <div style={{ padding: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)", background: "var(--tasty-surface-active)" }}>Default</div>
        <div style={{ flex: 1 }} />
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-sm)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", fontSize: "var(--tasty-font-size-caption)", color: notifErr ? "var(--tasty-text-secondary)" : "var(--tasty-text-muted)" }}>
          <Icon name="bell" size="var(--tasty-icon-size-sm)" />{notifErr && NotificationBadgeGroup ? <NotificationBadgeGroup error={1} /> : null}
        </div>
      </div>
      <div style={{ position: "relative", flex: 1, minWidth: 0, display: "flex" }}>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", ...(annotate ? ANNOT : {}) }}><Strip tabs={["main"]} /><Term /></div>
        {split && <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", borderLeft: "var(--tasty-border-width) solid var(--tasty-separator)" }}><Strip tabs={["logs"]} /><Term lines={["$ tail -f app.log", "INFO listening on :8080", "INFO GET /health 200"]} search={search} /></div>}
        {banner && <Banner text={banner} />}
        {children && <NoticeColumn topOffset={colTop}>{children}</NoticeColumn>}
      </div>
    </div>
  );
}
const Themed = ({ attr, children }) => (
  <div data-theme={attr || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)", minWidth: 0 }}>{children}</div>
);
const RN = (p) => { const R = K().RestoreNotice; return R ? <R {...p} /> : null; };
const LONG = "/Users/hyunjoon.kim/Library/Application Support/dev.zilhak.tasty/profiles/work-laptop-2026/session/layout.journal";

function GalleryNoticeColumn() {
  return (
    <>
      <Spec id="restore-notice" title="Restore failure — non-focus notice, top right"
        when={<>When the saved layout <b>cannot be read at all</b> at boot, Tasty does not stop: it opens one <b>default workspace</b> in its initial state and shows a small notice in the window's <b>top-right notice column</b>. No scrim, no centred modal, no focus move: the terminal keeps the keyboard, and keys, selection and scroll outside the notice behave as if it were not there. The notice is the shared popup frame (PopupTitleBar with ×, popup shadow): cause line with an error glyph, the <b>saved-layout path</b> (selectable, wraps anywhere, copy button), what happened and that the original is untouched, then <b>Continue</b>. <b>×</b> and <b>Continue</b> are the same action. The same error is recorded in <b>Notifications</b>, so dismissing loses nothing. Not for: a home locked by another instance, the first shell failing to start, an explicit webhook port failure.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={cap}>{label} · one flow</span>
              <span style={mono}>1 · boot, store unreadable → default workspace opens (nothing restored)</span>
              <Win />
              <span style={mono}>2 · notice shown — terminal keeps focus (dashed = input passes through)</span>
              <Win annotate notifErr><RN cause="denied" /></Win>
              <span style={mono}>3 · Continue / × → notice gone · terminal unchanged · error stays in Notifications</span>
              <Win notifErr />
            </Themed>
          ))}
        </Stage>
        <Meta
          specs={[["place", "notice column · work-area top-right, under the top-right pane's tab strip · inset 8 · width 360 (max = work area − 16)"], ["frame", "PopupTitleBar (28 · title centred · ×) · bg-panel · 1px border-frame · radius 4 · shadow-popover · no scrim"], ["body", "pad 12 · gap 8 · glyph alertCircle 16 notify-error + cause 13 text-primary · label mono 10 caps · path box · explanation term-sm text-secondary · Continue (secondary sm) right"], ["path", "mono 12 · bg-sidebar box, 1px border-default · selectable · overflow-wrap anywhere · copy IconButton sm"], ["focus", "never on appear · Enter / Esc NOT captured · role=status aria-live=polite"], ["explicit controls", "after a click inside: × / copy / Continue are normal buttons · Esc inside = Continue → focus back to the surface"], ["dismiss", "= continue in the initial state · never quits · never deletes / moves / overwrites the saved layout"], ["not shown", "internal method / class names · retry / reset / recovery buttons · any promise that changes are saved"]]}
          tokens={[{ tok: "--tasty-restore-notice-width", use: "→ notice-column-width → size-360" }, { tok: "--tasty-restore-notice-glyph", use: "→ notify-error → accent-danger", color: "var(--tasty-restore-notice-glyph)" }, { tok: "--tasty-restore-notice-padding", use: "→ space-md 12" }, { tok: "--tasty-restore-notice-gap", use: "→ space-sm 8" }, { tok: "--tasty-restore-notice-body-fg", use: "→ text-secondary", color: "var(--tasty-restore-notice-body-fg)" }, { tok: "--tasty-restore-notice-label-fg", use: "→ text-muted", color: "var(--tasty-restore-notice-label-fg)" }, { tok: "--tasty-restore-notice-path-bg", use: "→ bg-sidebar", color: "var(--tasty-restore-notice-path-bg)" }, { tok: "--tasty-restore-notice-path-border", use: "→ border-default", color: "var(--tasty-restore-notice-path-border)" }, { tok: "--tasty-restore-notice-path-fg", use: "→ text-primary", color: "var(--tasty-restore-notice-path-fg)" }]} />
        <Note><b>Contrast</b> (on bg-panel): cause text-primary Mocha ≈ 11:1 · Latte ≈ 7.1:1 · explanation text-secondary Mocha ≈ 8:1 · Latte n1000 ≈ 5.9:1 · label text-muted Mocha ≈ 5.4:1 · Latte n900 ≈ 4.6:1 · path text-primary on bg-sidebar Mocha ≈ 11.6:1 · Latte ≈ 6.5:1 · glyph accent-danger Mocha ≈ 6.5:1 · Latte ≈ 4.6:1 (≥ 3).</Note>
      </Spec>

      <Spec id="restore-notice-variants" title="Narrow · long path · split + search bar · save halt · ko / ja · the notification record"
        when={<>Narrow window: the notice takes the work area minus 8 on each side; the path wraps, the button never leaves the frame. With a <b>split</b>, the column still belongs to the window's top-right; if the top-right pane has its <b>search bar</b> open, the column starts under it (the bar is where the user is typing and keeps its place). <b>Save halt</b> (running, saving stopped) uses the same notice: current output and input go on, and it says layout changes from now on may not be restored. The <b>Notifications</b> record is an ordinary error item with the same title, cause and path.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={cap}>{label}</span>
              <span style={mono}>narrow window (work area ≈ 260) · long path · corrupt</span>
              <Win w={380} h={420}><RN cause="corrupt" path={LONG} /></Win>
              <span style={mono}>split · search bar open in the top-right pane → column below it</span>
              <Win w={720} h={340} split search colTop="calc(var(--tasty-tab-height) + var(--tasty-space-sm) + var(--tasty-size-32))"><RN cause="denied" /></Win>
              <span style={mono}>running · save halt variant</span>
              <Win w={640} h={330} split><RN variant="halt" cause="write" /></Win>
            </Themed>
          ))}
          <Themed>
            <span style={cap}>Mocha · ko / ja length</span>
            <RN lang="ko" cause="corrupt" path={LONG} />
            <RN lang="ja" variant="halt" cause="write" />
            <span style={cap}>Notifications › the error record (existing NotificationItem)</span>
            <div style={{ width: "var(--tasty-notifications-popup-width)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)", borderRadius: "var(--tasty-radius)", overflow: "hidden" }}>
              {NotificationItem && <NotificationItem level="error" title="Couldn't restore your last session" body={"Tasty doesn't have permission to read the saved layout.\n" + LONG} source="Tasty" time="1m" unread expanded />}
            </div>
          </Themed>
        </Stage>
        <Meta specs={[["narrow", "width = work area − 2 × 8 · path wraps · Continue stays inside"], ["split", "column = window top-right, not the focused pane"], ["search bar", "column top = tab strip + 8 + search bar + 8"], ["halt", "same frame · cause write · says output/input continue · no save promise"], ["record", "NotificationItem error · title + cause + path in body · source Tasty"], ["i18n", "notice.restore_failed_title / _body · notice.save_halted_title / _body · notice.cause_read_denied / _corrupt / _write_failed · notice.saved_layout_label · notice.copy_path · notice.continue"]]} />
      </Spec>

      <Spec id="notice-column" title="Notice column — what sits where, top right"
        when={<>One column, the window's work-area top-right, inset 8. Order top → bottom: <b>restore / save-halt notice</b>, then the <b>response-wait chip</b> (PendingIndicator), 8 apart, right-aligned. The column starts <b>below a window-scope banner</b> (resize failure, reconnect) and <b>below an open search bar</b> in the top-right pane; neither is moved by it. Pane- and surface-scope banners stay inside their own scope and do not move the column. While a <b>reconnect</b> banner is up the chip is gone (the request ended in the disconnect path). The chip never becomes a scrim or a full-width bar.</>}>
        <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
          {THEMES.map(([label, attr]) => (
            <Themed key={label} attr={attr}>
              <span style={cap}>{label}</span>
              <span style={mono}>notice + waiting chip</span>
              <Win w={640} h={360} notifErr><RN cause="denied" /><PendingIndicator count={1} elapsed={12} tooltip="Close tab · 12s" /></Win>
              <span style={mono}>window banner (resize failed) → column under it · 3 pending, oldest 1m 04s</span>
              <Win w={640} h={200} banner="Couldn't resize the remote terminal." colTop="calc(var(--tasty-tab-height) + var(--tasty-size-32) + var(--tasty-banner-margin))"><PendingIndicator count={3} elapsed={64} tooltip={"Move tab · 1m 04s\nClose tab · 20s\nRestore closed tab · 3s"} /></Win>
              <span style={mono}>narrow → compact chip (spinner + time)</span>
              <Win w={300} h={140}><PendingIndicator count={2} elapsed={9} compact /></Win>
            </Themed>
          ))}
        </Stage>
        <Meta specs={[["column", "work area top-right · inset 8 · width 360 · gap 8 · items right-aligned"], ["order", "restore / halt notice › PendingIndicator"], ["moves below", "window-scope banner · top-right pane's open search bar"], ["never covers", "the tab strip: the column starts under the top-right pane's tab strip (tab-height + 8), so close / scroll arrows stay reachable"], ["input", "column itself pointer-events none; only the notice / chip boxes take the pointer"]]}
          tokens={[{ tok: "--tasty-notice-column-inset", use: "→ space-sm 8" }, { tok: "--tasty-notice-column-gap", use: "→ space-sm 8" }, { tok: "--tasty-notice-column-width", use: "→ size-360" }]} />
      </Spec>
    </>
  );
}

export { GalleryNoticeColumn };
