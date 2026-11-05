import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as MpIcon } from "../../../components/core/Icon.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { HelpHint } from "../../../components/feedback/HelpHint.jsx";
import { Spinner } from "../../../components/feedback/Spinner.jsx";
import { InfoModalShell as Shell } from "./info_modal.jsx";

// macOS permissions — Settings › General › Permissions (L2, macOS builds only)
// and the boot "Some permissions are not granted" info modal.
// Designed 2026-09-28.
//
// Box boundaries for the implementer:
//   pane:  content column (settings 620 cap) > status table [label | status | row action]
//          > detection note > button row > request note (↔ requesting line)
//   modal: InfoModalShell (info_modal.jsx) — title bar > body scroll area > button row




// four states, one glyph each — Unknown and Not observable share the ink, never the glyph
const PERM_STATES = {
  granted:      { word: "Granted",                    glyph: "check",       fg: "var(--tasty-perm-granted-fg)" },
  missing:      { word: "Not granted",                glyph: "alertCircle", fg: "var(--tasty-perm-missing-fg)" },
  unknown:      { word: "Unknown",                    glyph: "helpCircle",  fg: "var(--tasty-perm-unknown-fg)" },
  unobservable: { word: "Cannot check automatically", glyph: "eyeOff",      fg: "var(--tasty-perm-unobservable-fg)" },
};

function PermStatus({ state }) {
  const s = PERM_STATES[state];
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-perm-status-gap)", color: s.fg,
      fontSize: "var(--tasty-font-size-body)", whiteSpace: "nowrap" }}>
      <span style={{ display: "inline-flex", flex: "none" }}><MpIcon name={s.glyph} size="var(--tasty-icon-size-sm)" /></span>
      <span style={{ color: state === "granted" || state === "missing" ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)" }}>{s.word}</span>
    </span>
  );
}

const PERM_SCENARIOS = {
  none:       { fda: "missing", screen: "missing", ax: "missing" },
  all:        { fda: "granted", screen: "granted", ax: "granted" },
  fdaUnknown: { fda: "unknown", screen: "granted", ax: "granted" },
  requesting: { fda: "missing", screen: "missing", ax: "missing" },
  // 2026-10-06 — granted once, then the app binary changed (update / rebuild): macOS keeps an entry that no
  // longer matches. Still state "missing" (no fifth state) — the row gains a one-line remedy under the label.
  fdaStale:   { fda: "missing", screen: "granted", ax: "granted", fdaSub: "Tasty changed since access was granted. Its entry in System Settings may still look on, but it no longer applies. Remove Tasty from the list, then add it again." },
  // 2026-10-08 — same hash, turned off outside Tasty: also "missing" + one caption line with its remedy.
  // 2026-10-08 (batch 8) — the re-add sentence joins the SAME caption paragraph (end of line, wraps); no second line.
  fdaRevoked: { fda: "missing", screen: "granted", ax: "granted", fdaSub: "Access was turned off outside Tasty. Turn it back on for Tasty in the list. If Tasty is no longer in the list (for example after tccutil reset), add it again with +." },
};

const mpNote = { margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-muted)",
  maxWidth: "var(--tasty-measure-xl)", textWrap: "pretty" };

function PermRow({ label, hint, sub, tag, state, action }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(0,1fr) auto", alignItems: "center", columnGap: "var(--tasty-space-lg)",
      minHeight: "var(--tasty-perm-row-height)", padding: "var(--tasty-space-xs) 0", borderBottom: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>
      <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)" }}>
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-help-hint-gap)", flexWrap: "wrap", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>
          {label}{hint && <HelpHint label={hint} placement="bottom" />}{tag && <Tag>{tag}</Tag>}
        </span>
        {sub && <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{sub}</span>}
      </div>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: "var(--tasty-space-md)", flexWrap: "wrap" }}>
        <PermStatus state={state} />
        {action}
      </div>
    </div>
  );
}

function MacPermissionsPane({ scenario = "none", debug = false }) {
  const s = PERM_SCENARIOS[scenario];
  const requesting = scenario === "requesting";
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
      <div style={{ display: "flex", flexDirection: "column", borderTop: "var(--tasty-border-width) solid var(--tasty-border-default)" }}>
        <PermRow label="Full Disk Access" state={s.fda} sub={s.fdaSub}
          hint="macOS has no API to report Full Disk Access, so this state is inferred and can be wrong."
          action={<Button size="sm" variant="secondary">Open System Settings</Button>} />
        <PermRow label="Screen recording" state={s.screen} />
        <PermRow label="Folder access" sub="Downloads · Documents · Desktop · volumes" state="unobservable"
          hint="Checking a folder would itself open a permission prompt, so Tasty does not check." />
        {debug && <PermRow label="Accessibility (key injection)" tag="debug" state={s.ax} />}
      </div>
      <p style={mpNote}>These states are a snapshot from startup, from opening this page, and from this window regaining focus. Nothing in Tasty is blocked by them; they only decide whether the startup notice appears.</p>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-sm)" }}>
        <Button variant="primary" disabled={requesting}>Request all permissions</Button>
      </div>
      {requesting ? (
        <p style={{ ...mpNote, display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", color: "var(--tasty-text-secondary)" }}>
          <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-size-1)" }}><Spinner size="var(--tasty-icon-size-sm)" /></span>
          <span>Requesting permissions. Answer each prompt as it appears; the next one shows after you answer.</span>
        </p>
      ) : (
        <p style={mpNote}>Asks for folder access, then screen recording, one prompt at a time. Items you already allowed or denied are not asked again, and a denied item can only be restored in System Settings. Full Disk Access cannot be requested by an app: use Open System Settings in its row and add Tasty yourself.</p>
      )}
    </div>
  );
}

// ── boot info modal ──────────────────────────────────────────────────────
const mpPath = { color: "var(--tasty-text-primary)", fontWeight: "var(--tasty-font-weight-medium)" };
const mpLead = { color: "var(--tasty-text-primary)", fontWeight: "var(--tasty-font-weight-semibold)" };
// emphasis (2026-09-29): colour carries it — text-primary vs the text-secondary body. The weights
// below are the web rendering only; the product may draw them at regular weight (egui has no bold
// UI family) and that is accepted. Command chip = mono + surface-raised run background ONLY:
// no radius, no side padding, wraps anywhere (one plain background run per wrapped line).
// 2026-10-08 — signing paragraph = secondary text; stale steps = numbered list, same body size, list gap = space-xs.
const mpAside = { margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
const mpSteps = { margin: 0, paddingLeft: "var(--tasty-space-xl)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", color: "var(--tasty-text-primary)" };
const mpCmd = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-primary)",
  background: "var(--tasty-surface-raised)", overflowWrap: "anywhere" };

// fda branch (2026-10-06): "never" = no grant on record · "stale" = granted before, binary hash differs ·
// "revoked" = granted before, same hash (turned off outside Tasty). Only the FDA paragraph changes; the
// title, buttons and show/hide rule (current state at every boot) do not. selfBuilt = ad-hoc / locally
// signed build. 2026-10-08: the app cannot tell a release from a self-built app, so the signing paragraph is
// shown in EVERY notice, last, as secondary text (mpAside: text-muted · caption). The stale paragraph carries
// the remove-and-add step as a numbered two-step list (mpSteps).
function PermNoticeBody({ fda = "never", selfBuilt = true } = {}) {
  const fdaPane = <span style={mpPath}>System Settings &gt; Privacy &amp; Security &gt; Full Disk Access</span>;
  return (
    <>
      <p>Commands you run in Tasty read and write files on your behalf, and macOS attributes that access to Tasty. Without permission, a prompt may interrupt your work or a feature may not work.</p>
      <p>Some permissions currently look like they are not granted. <span style={mpPath}>Settings &gt; General &gt; Permissions</span> shows which ones and what state they are in.</p>
      {fda === "stale" && <p><span style={mpLead}>Full Disk Access:</span> Tasty had this permission, but Tasty has changed since (an update or a rebuild) and macOS ties the permission to the exact app. In {fdaPane} the entry for Tasty may still look switched on, or may be missing.</p>}
      {fda === "stale" && <ol style={mpSteps}><li>Select Tasty in the list and remove it with −.</li><li>Add Tasty again with + and switch it on.</li></ol>}
      {fda === "stale" && <p>If access still fails afterwards, quit and reopen Tasty.</p>}
      {fda === "revoked" && <p><span style={mpLead}>Full Disk Access:</span> Tasty had this permission, and it was turned off outside Tasty. Turn it back on for Tasty in {fdaPane}. If Tasty is no longer in the list (for example after <span style={mpCmd}>tccutil reset</span>), add it again with + and switch it on.</p>}
      <p><span style={mpLead}>{fda === "never" ? "Full Disk Access:" : "What it covers:"}</span> {fda === "never" ? <>macOS has no way for an app to ask for this, so add Tasty yourself in {fdaPane}. </> : null}Granting it removes the file-access prompts (other apps' data, Downloads, Documents, Desktop and mounted volumes). It does not cover controlling other apps (Automation / Apple Events, e.g. osascript), screen recording, or accessibility: those are separate permissions and will still prompt.</p>
      <p><span style={mpLead}>Screen recording:</span> this is used by the screenshot feature. Turn it on in <span style={mpPath}>System Settings &gt; Privacy &amp; Security &gt; Screen &amp; System Audio Recording</span>. Once you deny it, the app cannot ask again and only System Settings can turn it back on.</p>
      <p>Once you grant them, this notice stops appearing. There is no setting to turn it off, because recording that you dismissed it would leave no way to tell you when a permission is reset later.</p>
      {selfBuilt && <p style={mpAside}>Downloaded releases and builds you make yourself are both ad-hoc signed, so after every update or rebuild macOS sees a different app and the earlier permission no longer applies. If you build Tasty yourself, sign it with the "Tasty Dev" certificate first (<span style={mpCmd}>./scripts/macos-codesign-identity.sh --create</span>).</p>}
    </>
  );
}

// One message in the shared InfoModalShell. The ONLY queue message with authored
// emphasis (paths · lead-ins · command chip) and a second button.
function PermissionNoticeModal({ scroll = "top", fda = "never", selfBuilt = true }) {

  return (
    <Shell id="perm-notice" title="Some permissions are not granted" scroll={scroll}
      actions={<><Button variant="secondary">Open permission settings</Button><Button variant="primary">OK</Button></>}>
      {PermNoticeBody({ fda, selfBuilt }).props.children}
    </Shell>
  );
}

export { MacPermissionsPane, PermissionNoticeModal, PermStatus, PERM_STATES };
