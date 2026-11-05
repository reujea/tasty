import React from "react";
import { Badge } from "../../../components/core/Badge.jsx";
import { Button } from "../../../components/core/Button.jsx";
import { Icon as DsIcon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Spinner } from "../../../components/feedback/Spinner.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Switch } from "../../../components/forms/Switch.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim, Spinner as TastyKit_Spinner } from "./shared.jsx";

// Tasty UI kit — Plugins window (lifecycle: install / enable / uninstall).
// Mirrors zilhak/tasty → src/view/plugins.rs + src/view/plugins/ui/{list,add}.rs

const PwSpinner = Spinner || TastyKit_Spinner;


// ── Plugins — installed list + add/install new plugins ──────────────────
// Opened from the sidebar "Plugins" entry. The Settings › Plugins tab is for
// per-plugin *configuration*; this window is for lifecycle: enable / disable
// installed plugins, and add new ones from a LOCAL FOLDER via the Add plugin tab.
// There is no online catalog/marketplace (deferred — docs/dev-guide/plugin-packaging.md): you install from a path or a cloned repo, Tasty verifies the
// folder's tasty-plugin.toml + signature, and copies it into ~/.tasty/plugins.
const PLUGIN_LIST = [
  { id: "git-helper", name: "git-helper", author: "tasty-labs", version: "1.4.2", cat: "Source control",
    installed: true, status: "running", agent: false, installs: "126k", rating: "4.9", builtin: true,
    homepage: "https://github.com/zilhak/tasty-git-helper", kinds: ["git-blame", "git-branches"],
    desc: "Inline git status, blame, and one-key staging inside any terminal surface. Adds a gutter ribbon and a compact branch switcher to the tab strip.",
    perms: ["fs:read", "clipboard", "ipc:git-helper.*"], cmd: "git-helper: open panel", key: "Ctrl+Alt+G" },
  { id: "ai-review", name: "ai-review", author: "tasty-labs", version: "0.9.0", cat: "AI",
    installed: true, status: "agent", agent: true, installs: "84k", rating: "4.7",
    desc: "Agent-driven review of staged diffs. Streams suggested patches into a side surface and lets you apply them hunk by hunk.",
    perms: ["fs:read", "fs:write", "net", "ipc:ai-review.*"], cmd: "ai-review: review staged", key: "Ctrl+Alt+R" },
  { id: "docker", name: "docker", author: "community", version: "2.1.0", cat: "DevOps",
    installed: true, status: "idle", agent: false, installs: "203k", rating: "4.6",
    desc: "A surface for container logs, exec sessions, and compose control. Attaches to the local Docker socket and mirrors `docker ps` live.",
    perms: ["ipc:docker.*", "net"], cmd: "docker: containers", key: "Ctrl+Alt+D" },
  { id: "k8s-lens", name: "k8s-lens", author: "community", version: "0.6.3", cat: "DevOps",
    installed: true, status: "error", agent: false, installs: "57k", rating: "4.1",
    desc: "Kubernetes context switcher and pod log streamer. Currently failing to reach the configured cluster — check kubeconfig.",
    perms: ["net", "fs:read", "ipc:k8s-lens.*"], cmd: "k8s-lens: switch context", key: "Ctrl+Alt+K" },
  { id: "vim-mode", name: "vim-mode", author: "ophen", version: "3.2.1", cat: "Editing",
    installed: false, status: "idle", agent: false, installs: "418k", rating: "4.8",
    desc: "Modal editing keybindings for every text surface — normal/insert/visual modes, registers, and the operators you actually use.",
    perms: ["clipboard", "ipc:vim-mode.*"], cmd: "vim-mode: toggle", key: "Ctrl+Alt+V" },
  { id: "theme-studio", name: "theme-studio", author: "tasty-labs", version: "1.0.4", cat: "Appearance",
    installed: false, status: "idle", agent: false, installs: "92k", rating: "4.9",
    desc: "Live-edit surface colors and export a Catppuccin-compatible theme. Hot-reloads every open pane as you drag.",
    perms: ["fs:read", "fs:write"], cmd: "theme-studio: open", key: "Ctrl+Alt+T" },
  { id: "s3-browser", name: "s3-browser", author: "community", version: "0.4.0", cat: "Cloud",
    installed: false, status: "idle", agent: false, installs: "31k", rating: "4.3",
    desc: "Browse, preview, and stream from S3 buckets as a first-class surface. Drag objects between buckets and your local tree.",
    perms: ["net", "fs:read", "fs:write"], cmd: "s3-browser: open bucket", key: "" },
  { id: "pomodoro", name: "pomodoro", author: "kettle", version: "2.0.0", cat: "Productivity",
    installed: false, status: "idle", agent: false, installs: "19k", rating: "4.5",
    desc: "A focus timer that lives in the status bar, logs sessions per workspace, and dims inactive panes while you're in a sprint.",
    perms: ["ipc:pomodoro.*"], cmd: "pomodoro: start", key: "" },
];

// ── Needs-attention cases ───────────────────────────────────────────────
// Plugins that are installed/bundled but were REJECTED at registration
// (signature/trust) or are ENABLED-but-failing (health error). Mirrors
// tasty-host-plugin/src/bundle_sig.rs TrustDecision + builtin.rs Skipped
// reasons. NOT a marketplace — only local/bundled plugins (docs/dev-guide/plugin-packaging.md).
const ATTN = {
  // 2026-10-19 (plugin-install-warning-and-choice): signature reasons are NOT a dead end any more —
  // the item stays unregistered until the user explicitly picks "Register anyway".
  "unknown-key":        { sev: "danger",  label: "Signature not trusted", unverified: "unknown-key",
    blurb: "Signed by a key that isn't in your trust store. Not registered — you can register it anyway." },
  "signature-invalid":  { sev: "danger",  label: "Signature invalid", unverified: "signature-error",
    blurb: "Signature missing or failed verification. Not registered — you can register it anyway." },
  "permissions-changed":{ sev: "warning", label: "Permissions changed",
    blurb: "Manifest permissions changed since you trusted it — re-approval required. Permissions limit what the plugin can ask Tasty to do; they don't isolate the plugin program." },
  "health-error":       { sev: "warning", label: "Runtime error",
    blurb: "Enabled, but failing at runtime." },
};
const SEV_COLOR = { danger: "var(--tasty-accent-danger)", warning: "var(--tasty-accent-warning)" };

// Rejected / unregistered plugins — these never made it into the installed list.
const ATTENTION_LIST = [
  { id: "fleet-sync", name: "fleet-sync", author: "tasty-labs", version: "1.2.0", cat: "DevOps",
    reason: "unknown-key", builtin: true, homepage: "https://github.com/zilhak/tasty-fleet-sync",
    desc: "Bundled cluster fleet sync. Its publisher key was rotated and the signature no longer matches a trusted key.",
    detail: { fingerprint: "a13c 4e7f 2b08 9d51  ·  ed25519",
      note: "The built-in publisher key rotated this release; the bundled signature was made with a key not yet in your trust store. Update Tasty or import the new key to restore it." } },
  { id: "secrets-vault", name: "secrets-vault", author: "community", version: "0.8.4", cat: "Cloud",
    reason: "permissions-changed",
    desc: "Reads and injects secrets into surfaces. You trusted v0.7 — v0.8.4 requests a different permission set.",
    detail: { added: ["fs:write", "net"], removed: ["clipboard"] } },
  { id: "remote-shell", name: "remote-shell", author: "community", version: "1.0.0", cat: "DevOps",
    reason: "signature-invalid", homepage: "https://example.invalid/remote-shell",
    desc: "Opens remote SSH surfaces. The bundle's signature file is missing or corrupt.",
    detail: { note: "tasty-plugin.sig is absent or does not match the manifest hash. Re-download the plugin from its source." } },
];

// The plugin identity mark — a square with the name's initial (plugin manifests
// carry no image). CANONICAL, single copy: every avatar in this window goes
// through here. Two sizes only — "sm" (32) on list rows, "lg" (46) on a detail
// or manifest-preview identity block. The tint mixes into surface-raised, a
// FIXED bed, so the mark reads the same on default / hover / selected rows, and
// the colour never varies by plugin: a manifest has no category field, so the
// mark carries identity (the initial), not classification.
// 2026-10-09 — keycap rule: split the manifest chord on "+", trim, Title-case each part
// (single letters upper-case). 2026-10-10 (b11, plugins-macos-keycaps): on macOS the chord first goes
// through the keybinding mapping (alt → Command, option → Option, ctrl → Control), modifiers sort
// Ctrl ⌥ ⇧ ⌘ (Apple order ⌃ ⌥ ⇧ ⌘), and each follows the user's modifier display style exactly like
// the Settings keybinding rows and the modifier hint. Ctrl stays the word "Ctrl" (no style, no glyph).
function kbdKeys(chord) {
  return String(chord).split("+").map((k) => k.trim()).filter(Boolean)
    .map((k) => k.length === 1 ? k.toUpperCase() : k.charAt(0).toUpperCase() + k.slice(1).toLowerCase());
}
function PluginAvatar({ plugin, size = "sm" }) {
  const lg = size === "lg";
  return (
    <span style={{ width: `var(--tasty-plugin-avatar-size-${lg ? "lg" : "sm"})`, height: `var(--tasty-plugin-avatar-size-${lg ? "lg" : "sm"})`,
      flex: "none", borderRadius: "var(--tasty-plugin-avatar-radius)",
      display: "inline-flex", alignItems: "center", justifyContent: "center",
      background: "var(--tasty-plugin-avatar-bg)",
      border: "var(--tasty-plugin-avatar-border-width) solid var(--tasty-plugin-avatar-border)",
      fontFamily: "var(--tasty-font-mono)", fontWeight: "var(--tasty-plugin-avatar-initial-weight)",
      color: "var(--tasty-plugin-avatar-fg)",
      fontSize: `var(--tasty-plugin-avatar-initial-font-size-${lg ? "lg" : "sm"})`, lineHeight: 1 }}>
      {plugin.name.charAt(0).toUpperCase()}
    </span>
  );
}

// ── Add plugin — local folder install (no catalog) ─────────────────────
// Mirrors src/view/plugins/ui/add.rs: pick a local folder (or clone), read its
// tasty-plugin.toml manifest, judge trust by signature, then Install /
// TrustAndInstall. A sample manifest stands in for a real folder read.
const SAMPLE_MANIFEST = {
  id: "com.aurelia.logwatch", name: "logwatch", version: "0.3.1", author: "aurelia",
  trusted: false, fingerprint: "9f2c 4ad1 b770 e3a6  ·  ed25519",
  desc: "Tails and highlights structured log files as a dedicated surface — severity filters, a jump-to-error gutter, and live follow on the active workspace.",
  perms: ["fs:read", "fs:watch", "ipc:logwatch.*"],
  surfaces: ["logwatch.viewer"],
  path: "~/dev/tasty-logwatch",
};

// Fingerprint line (2026-09-29): mono text + IconButton sm `copy` right after it — the copy
// action lives with the value, not in the action bar. No fingerprint → no line, no button.
// 2026-10-06: a long colon-hex fingerprint (> 16 bytes, e.g. SHA-256 = 95 chars) shows the first 8 and last 8
// bytes joined by " … "; tooltip and copy carry the full value. Short forms show as given.
function shortFingerprint(v) {
  const parts = String(v).split(":");
  return parts.length > 16 ? parts.slice(0, 8).join(":") + " … " + parts.slice(-8).join(":") : v;
}
function FingerprintLine({ value, onCopy }) {

  if (!value) return null;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontFamily: "var(--tasty-font-mono)",
      fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
      <span style={{ color: "var(--tasty-text-secondary)" }}>fingerprint</span>
      <span title={value} style={{ whiteSpace: "nowrap" }}>{shortFingerprint(value)}</span>
      <IconButton size="sm" aria-label="Copy fingerprint" title="Copy fingerprint" onClick={onCopy}><DsIcon name="copy" /></IconButton>
    </div>
  );
}

// Add plugin — a verified manifest that can't be added (2026-09-29): the button stays in its slot,
// DISABLED (shared ink rule), and the reason replaces "Grants N permissions" on the left of the bar.
//   manifest.blocked: "installed" | "missing-pubkey" | "signature-error"
//   2026-10-06: "unsigned-no-key" → "missing-pubkey". The host state (UntrustedNoPubkey) IS signed — by a key
//   not in the trust store — and the .pub that would let us trust it is missing or unreadable.
// 2026-10-19: missing-pubkey / signature-error are no longer blocked — they take the unverified
// warning + an explicit "Add anyway". Only "installed" still disables Add.
const ADD_BLOCKED = {
  "installed": "Already installed",
};
const grantsLabel = (n) => n === 0 ? "No permissions" : n === 1 ? "Grants 1 permission" : "Grants " + n + " permissions";

// Trust judgment box (2026-10-06) — one box, five kinds. Tone: success = can add as is · warning = can add,
// adding trusts the key · danger = blocked. Box = tint-fill + tint-border of the tone, pad space-md / 14,
// title row (glyph md + 13/600 in the tone) > body (term-sm, text-secondary) > fingerprint line (absent
// when there is no fingerprint — signature-error never has one).
//   manifest.trust: "trusted" | "unknown-key" | "permissions-changed" | "missing-pubkey" | "signature-error"
//   2026-10-19 (plugin-install-warning-and-choice): the four UNVERIFIED kinds (unknown-key · missing-pubkey ·
//   unsigned · signature-error) share ONE danger box: same title, same risk paragraph; only the cause line
//   differs. All four are addable through an explicit "Add anyway" (danger button, never the initial focus).
//   Title ink is text-primary (accent titles failed 4.5:1 in Latte); only the glyph carries the tone.
const UNVERIFIED_TITLE = "Unverified plugin";
const UNVERIFIED_RISK = "Tasty can't confirm who made this plugin or that its files are unchanged. A plugin runs as a program with your account's access: it can read and change your files, use the network, and start other programs. The permissions above only limit what it can ask Tasty to do. Add it only if you trust where it came from.";
const UNVERIFIED_CAUSE = {
  "unknown-key":     "It is signed by a key that isn't in your trust store.",
  "missing-pubkey":  "It is signed, but the publisher's public key file (tasty-plugin.toml.pub) is missing or unreadable.",
  "unsigned":        "It has no signature.",
  "signature-error": "Its signature could not be verified against these files.",
};
const TRUST_KIND = {
  "trusted":             { tone: "success", glyph: "shieldCheck",  title: null },
  "permissions-changed": { tone: "warning", glyph: "alertTriangle", title: "Permissions changed",
    body: "This publisher is trusted, but this version asks for permissions the trusted version did not have. Review the list above; adding it trusts the new set. These permissions limit what the plugin can ask Tasty to do; they don't isolate the plugin program from your files, network or other programs." },
  "unknown-key":         { tone: "danger", glyph: "alertCircle", title: UNVERIFIED_TITLE, unverified: true, after: "Adding it also trusts this key." },
  "missing-pubkey":      { tone: "danger", glyph: "alertCircle", title: UNVERIFIED_TITLE, unverified: true },
  "unsigned":            { tone: "danger", glyph: "alertCircle", title: UNVERIFIED_TITLE, unverified: true, noFingerprint: true },
  "signature-error":     { tone: "danger", glyph: "alertCircle", title: UNVERIFIED_TITLE, unverified: true, noFingerprint: true },
};
const trustKindOf = (m) => m.trust || (m.trusted ? "trusted" : "unknown-key");
function TrustBox({ manifest }) {
  const kind = trustKindOf(manifest);
  const k = TRUST_KIND[kind];
  const acc = "var(--tasty-accent-" + k.tone + ")";
  const box = { display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)",
    borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-secondary)",
    background: "color-mix(in srgb, " + acc + " calc(var(--tasty-tint-fill-alpha) * 100%), transparent)",
    border: "var(--tasty-border-width) solid color-mix(in srgb, " + acc + " calc(var(--tasty-tint-border-alpha) * 100%), transparent)" };
  if (kind === "trusted") return (
    <div style={{ ...box, flexDirection: "row", alignItems: "center", fontSize: "var(--tasty-font-size-term-sm)" }}>
      {/* 2026-10-27: inks split from the tone — glyph + body → accent-success-ink (Latte ≥ 5.5:1 on mantle/base+tint); fill/border keep accent-success */}
      <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-plugin-trust-glyph-success)" }}><Icon name="shieldCheck" size={16} /></span>
      <span style={{ color: "var(--tasty-plugin-trust-body-trusted-fg)" }}>Signed by a <b>trusted publisher</b> — its key is in your trust store.</span>
    </div>
  );
  const p = { margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-plugin-trust-body-fg)", overflowWrap: "anywhere" };
  return (
    <div style={box} role={k.unverified ? "alert" : undefined}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", color: "var(--tasty-plugin-trust-title-fg)", fontSize: 13, fontWeight: 600 }}>
        <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-plugin-trust-glyph-" + k.tone + ")" }}><Icon name={k.glyph} size={16} /></span><span>{k.title}</span>
      </div>
      {k.unverified ? <>
        <p style={p}>{UNVERIFIED_CAUSE[kind]}</p>
        <p style={p}>{UNVERIFIED_RISK}{k.after ? " " + k.after : ""}</p>
      </> : <p style={p}>{k.body}</p>}
      {!k.noFingerprint && <FingerprintLine value={manifest.fingerprint} />}
    </div>
  );
}
// Primary action for a verdict. Unverified = danger "Add anyway" ("Trust key & add anyway" when the .pub is
// present, because adding also trusts the key). Never autoFocus: Cancel holds the initial focus.
function addActionLabel(m) {
  const kind = trustKindOf(m);
  if (kind === "trusted") return "Add plugin";
  if (kind === "permissions-changed") return "Trust & add";
  return kind === "unknown-key" ? "Trust key & add anyway" : "Add anyway";
}
function AddPluginForm({ onAdded, onCancel }) {
  const [path, setPath] = React.useState("");
  const [manifest, setManifest] = React.useState(null);
  const [busy, setBusy] = React.useState(false);
  const setPathReset = (v) => { setPath(v); setManifest(null); }; // re-verify after any edit
  const verify = () => {
    const p = path.trim() || SAMPLE_MANIFEST.path;
    setPath(p);
    // demo: a path under ~/.tasty/plugins stands in for "already installed" (disabled Add);
    // "unsigned" / "nopub" / "badsig" in the path pick the other unverified verdicts
    const trust = /unsigned/.test(p) ? "unsigned" : /nopub/.test(p) ? "missing-pubkey" : /badsig/.test(p) ? "signature-error" : undefined;
    setManifest({ ...SAMPLE_MANIFEST, path: p, trust, fingerprint: trust === "unsigned" || trust === "signature-error" ? null : SAMPLE_MANIFEST.fingerprint,
      blocked: p.startsWith("~/.tasty/plugins") ? "installed" : undefined });
  };
  // explicit choice → busy (no double add) → the existing result path (onAdded / toast)
  const add = () => { if (busy) return; setBusy(true); setTimeout(() => { setBusy(false); onAdded(manifest); }, 900); };
  const onBarKey = (e) => { if (e.key === "Escape" && !busy) { e.stopPropagation(); onCancel && onCancel(); } };
  const pick = () => setPathReset("~/dev/tasty-logwatch"); // stands in for the native rfd folder picker

  const Mono = ({ children }) => (
    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
      letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>{children}</div>
  );
  const code = { fontFamily: "var(--tasty-font-mono)", color: "var(--tasty-text-secondary)" };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
      <div className="tasty-scroll" style={{ flex: 1, overflow: "auto", padding: "var(--tasty-size-22) var(--tasty-space-xl)",
        display: "flex", flexDirection: "column", gap: "var(--tasty-space-lg)" }}>
        {/* path picker */}
        <div style={{ maxWidth: "var(--tasty-measure-xl)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
          <Mono>Plugin folder</Mono>
          <div style={{ display: "flex", gap: 8 }}>
            <Input mono icon={ic.folder} placeholder="~/dev/my-plugin"
              value={path} onChange={(e) => setPathReset(e.target.value)} style={{ flex: 1 }} />
            <Button variant="secondary" leadingIcon={ic.folder} onClick={pick}>Find folder…</Button>
            <Button variant="primary" disabled={!path.trim() && !manifest} onClick={verify}>Verify</Button>
          </div>
          <p style={{ margin: 0, fontSize: 12, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-muted)" }}>
            Point Tasty at a local folder containing a <code style={code}>tasty-plugin.toml</code>. Verifying reads its
            manifest and checks the signature; adding copies it into <code style={code}>~/.tasty/plugins</code>.
          </p>
        </div>

        {/* 2026-10-07 — empty hint keeps the dashed edge (border-dash / border-dash-gap 4 / 4, corners solid).
            A manifest that can't be read takes the same box, solid accent-danger edge — see gallery Plugins › Add plugin read error. */}
        {!manifest ? (
          <div style={{ maxWidth: "var(--tasty-measure-xl)", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)",
            padding: "var(--tasty-size-14) var(--tasty-space-lg)", borderRadius: "var(--tasty-radius)", border: "var(--tasty-border-width) dashed var(--tasty-border-default)",
            color: "var(--tasty-text-muted)", fontSize: "var(--tasty-font-size-term-sm)" }}>
            <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>{ic.folder}</span>
            <span>Choose a folder and press <b style={{ color: "var(--tasty-text-secondary)" }}>Verify</b> to read its manifest.</span>
          </div>
        ) : (
          <div style={{ maxWidth: "var(--tasty-measure-xl)", display: "flex", flexDirection: "column", gap: 14 }}>
            {/* manifest preview */}
            <div style={{ border: "var(--tasty-border-width) solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)",
              background: "var(--tasty-surface-raised)", padding: 16, display: "flex", flexDirection: "column", gap: "var(--tasty-space-md)" }}>
              <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                <PluginAvatar plugin={manifest} size="lg" />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                    <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{manifest.name}</span>
                    <Tag>{"v" + manifest.version}</Tag>
                  </div>
                  <div style={{ marginTop: 3, fontFamily: "var(--tasty-font-mono)", fontSize: 11,
                    color: "var(--tasty-text-muted)" }}>{manifest.id} · {manifest.author}</div>
                </div>
              </div>
              <p style={{ margin: 0, fontSize: 13, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)" }}>{manifest.desc}</p>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                <Mono>Permissions</Mono>
                <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>{manifest.perms.map((p) => <Tag key={p}>{p}</Tag>)}</div>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                <Mono>Surface kinds</Mono>
                <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>{manifest.surfaces.map((s) => <Tag key={s}>{s}</Tag>)}</div>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <Mono>Source</Mono>
                <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-secondary)",
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{manifest.path}</span>
              </div>
            </div>

            {/* trust judgment */}
            <TrustBox manifest={manifest} />
          </div>
        )}
      </div>

      {/* action bar */}
      {/* 2026-10-19: wraps on narrow windows (buttons drop under the reason, right-aligned). Unverified verdict:
          initial focus = Cancel (Enter on it cancels), Esc = Cancel, Tab → "Add anyway". No implicit default button. */}
      <div onKeyDown={onBarKey} style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)",
        borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", flex: "none" }}>
        {manifest && (
          <span style={{ flex: "1 1 auto", minWidth: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
            {manifest.blocked ? ADD_BLOCKED[manifest.blocked] : grantsLabel(manifest.perms.length)}
          </span>
        )}
        {!manifest && <div style={{ flex: 1 }} />}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", marginLeft: "auto" }}>
          {busy && <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}><PwSpinner size={14} label="Adding" />Adding…</span>}
          <Button key={manifest ? manifest.path + trustKindOf(manifest) : "none"} variant="ghost" autoFocus={!!manifest && TRUST_KIND[trustKindOf(manifest)].unverified} disabled={busy} onClick={onCancel}>Cancel</Button>
          {manifest && (manifest.blocked
            ? <Button variant="primary" disabled>Add plugin</Button>
            : <Button variant={TRUST_KIND[trustKindOf(manifest)].unverified ? "danger" : "primary"} disabled={busy} aria-busy={busy || undefined} onClick={add}>{addActionLabel(manifest)}</Button>)}
        </div>
      </div>
    </div>
  );
}

// ── Attention tab — rejected / failing plugins with reason + action ─────
function RegisterAnyway({ name, onFlash }) {
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => setBusy(false), [name]);
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
      {busy && <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}><PwSpinner size={14} label="Registering" />Registering…</span>}
      <Button variant="danger" disabled={busy} aria-busy={busy || undefined}
        onClick={() => { setBusy(true); setTimeout(() => { setBusy(false); onFlash && onFlash("Registered " + name); }, 900); }}>
        Register anyway
      </Button>
    </span>
  );
}
function AttentionPanel({ items, onFlash, onConfigure }) {
  const [selId, setSelId] = React.useState(items[0] ? items[0].id : null);
  const sel = items.find((p) => p.id === selId) || items[0] || null;

  const Mono = ({ children }) => (
    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
      letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>{children}</div>
  );

  if (items.length === 0)
    return (
      <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center",
        justifyContent: "center", gap: 12, color: "var(--tasty-text-muted)", padding: 24 }}>
        <span style={{ display: "inline-flex", width: 46, height: 46, borderRadius: "var(--tasty-radius)",
          alignItems: "center", justifyContent: "center", color: "var(--tasty-accent-success-raised)",
          background: "color-mix(in srgb, var(--tasty-accent-success) 12%, transparent)",
          border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-success) 30%, transparent)" }}>
          <Icon name="shieldCheck" size={20} />
        </span>
        <div style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)" }}>No plugins need attention</div>
        <div style={{ fontSize: 12, maxWidth: "var(--tasty-measure-sm)", textAlign: "center", lineHeight: "var(--tasty-line-height-ui)" }}>
          Rejected or failing plugins show up here with the reason and what to do next.
        </div>
      </div>
    );

  const meta = sel && ATTN[sel.reason];
  const c = sel && SEV_COLOR[meta.sev];
  return (
    <React.Fragment>
      {/* list */}
      <div className="tasty-scroll" style={{ width: "var(--tasty-plugins-list-width)", flex: "none", overflow: "auto", padding: 8,
        background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)",
        display: "flex", flexDirection: "column", gap: 3 }}>
        {items.map((p) => {
          const on = p.id === sel.id;
          const pc = SEV_COLOR[ATTN[p.reason].sev];
          return (
            <div key={p.id} onClick={() => setSelId(p.id)} style={{ display: "flex", alignItems: "center",
              gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius)", cursor: "pointer",
              background: on ? "var(--tasty-surface-active)" : "transparent",
              boxShadow: on ? `inset var(--tasty-selection-edge-width) 0 0 ${pc}` : "none" }}>
              <PluginAvatar plugin={p} size="sm" />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, color: on ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)",
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.name}</div>
                <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: on ? "var(--tasty-text-on-active)" : pc,
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{ATTN[p.reason].label}</div>
              </div>
              <span style={{ flex: "none", width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%", background: pc }} />
            </div>
          );
        })}
      </div>

      {/* detail */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
        <div className="tasty-scroll" style={{ flex: 1, overflow: "auto", padding: "var(--tasty-space-lg)",
          display: "flex", flexDirection: "column", gap: 16 }}>
          {/* identity */}
          <div style={{ display: "flex", gap: "var(--tasty-space-md)", alignItems: "flex-start" }}>
            <PluginAvatar plugin={sel} size="lg" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-normal)", color: "var(--tasty-text-primary)" }}>{sel.name}</span>
                <Tag>{"v" + sel.version}</Tag>
                {sel.builtin && <Tag>built-in</Tag>}
              </div>
              <div style={{ marginTop: 4, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)",
                display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
{/* 2026-10-09 b10 — each separator travels WITH the item after it (one nowrap span), so a wrapped item never leaves a dangling · */}
                {[sel.author, sel.id, sel.reason !== "signature-invalid" && sel.homepage && /^https?:\/\//.test(sel.homepage) && <a href={sel.homepage} target="_blank" rel="noreferrer" style={{ color: "var(--tasty-accent-primary-raised)", textDecoration: "underline", textUnderlineOffset: 2 }}>{sel.homepage.replace(/^https?:\/\//, "")}</a>].filter(Boolean).map((part, i) => (
                  <span key={i} style={{ display: "inline-flex", gap: 8, whiteSpace: "nowrap" }}>{i > 0 && <span>·</span>}{part}</span>
                ))}
              </div>
            </div>
          </div>

          {/* 2026-10-09 b10 — an invalid signature means the manifest text is unverified: no description, no homepage (untrusted-key items still show both) */}
          {sel.reason === "signature-invalid"
            ? <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Description hidden — the signature is invalid.</p>
            : sel.desc && <p style={{ margin: 0, fontSize: 13, lineHeight: "var(--tasty-line-height-ui)",
            color: "var(--tasty-text-secondary)", maxWidth: "var(--tasty-measure-lg)" }}>{sel.desc}</p>}

          {/* reason banner — 2026-10-19: signature reasons take the shared unverified box (same risk copy as Add) */}
          {meta.unverified ? <TrustBox manifest={{ trust: meta.unverified, fingerprint: null }} /> :
          <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: "var(--tasty-space-md) var(--tasty-size-14)",
            borderRadius: "var(--tasty-radius)", color: "var(--tasty-text-secondary)",
            background: `color-mix(in srgb, ${c} 11%, transparent)`,
            border: `1px solid color-mix(in srgb, ${c} 36%, transparent)` }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", color: c, fontSize: 13, fontWeight: 600 }}>
              <Icon name={meta.sev === "danger" ? "alertCircle" : "alertTriangle"} size={16} />
              <span>{meta.label}</span>
            </div>
            <p style={{ margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)" }}>{meta.blurb}</p>
          </div>}

          {/* reason-specific detail */}
          {sel.reason === "permissions-changed" && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              <Mono>Permission changes</Mono>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                {(sel.detail.added || []).map((p) => (
                  <div key={p} style={{ display: "flex", alignItems: "center", gap: 8,
                    fontFamily: "var(--tasty-font-mono)", fontSize: 12 }}>
                    <span style={{ color: "var(--tasty-accent-success-raised)", fontWeight: 700, width: 10 }}>+</span>
                    <span style={{ color: "var(--tasty-text-secondary)" }}>{p}</span>
                    <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>newly requested</span>
                  </div>
                ))}
                {(sel.detail.removed || []).map((p) => (
                  <div key={p} style={{ display: "flex", alignItems: "center", gap: 8,
                    fontFamily: "var(--tasty-font-mono)", fontSize: 12 }}>
                    <span style={{ color: "var(--tasty-text-muted)", fontWeight: 700, width: 10 }}>−</span>
                    <span style={{ color: "var(--tasty-text-muted)", textDecoration: "line-through" }}>{p}</span>
                    <span style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>no longer used</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {(sel.reason === "unknown-key" || sel.reason === "signature-invalid") && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              <Mono>Signature</Mono>
              <FingerprintLine value={sel.detail.fingerprint} onCopy={() => onFlash && onFlash("Fingerprint copied")} />
              {!sel.detail.note && sel.reason === "signature-invalid" && <p style={{ margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)",
                color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-lg)" }}>The signature does not match this plugin's files.</p>}
              {sel.detail.cause && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{sel.detail.cause}</span>}
              {sel.detail.note && <p style={{ margin: 0, fontSize: "var(--tasty-font-size-term-sm)", lineHeight: "var(--tasty-line-height-ui)",
                color: "var(--tasty-text-muted)", maxWidth: "var(--tasty-measure-lg)" }}>{sel.detail.note}</p>}
            </div>
          )}

          {sel.reason === "health-error" && sel.detail.error && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              <Mono>Error</Mono>
              <pre style={{ margin: 0, background: "var(--tasty-bg-app)", border: "var(--tasty-border-width) solid var(--tasty-separator)",
                borderRadius: "var(--tasty-radius)", padding: "var(--tasty-space-md) var(--tasty-space-md)", fontFamily: "var(--tasty-font-mono)",
                fontSize: 12, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-accent-danger-raised)", whiteSpace: "pre-wrap" }}>{sel.detail.error}</pre>
            </div>
          )}
        </div>

        {/* action bar */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)",
          borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", flex: "none" }}>
          <span style={{ fontSize: 12, color: c, display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
            <span style={{ width: "var(--tasty-status-dot-size)", height: "var(--tasty-status-dot-size)", borderRadius: "50%", background: c }} />
            {meta.sev === "danger" ? "Not registered" : "Needs review"}
          </span>
          <div style={{ flex: 1 }} />
          {sel.reason === "permissions-changed" && (
            <Button variant="primary" onClick={() => onFlash && onFlash("Re-approve " + sel.name)}>Re-approve</Button>
          )}
          {sel.reason === "health-error" && (
            <Button variant="ghost" leadingIcon={ic.settings}
              onClick={() => onConfigure && onConfigure()}>Configure</Button>
          )}
          {/* unknown-key · signature-invalid (2026-10-19): explicit "Register anyway" (danger). Not focused on selection;
              not acting = stays unregistered. Busy → button disabled, Spinner + "Registering…" beside it. */}
          {meta.unverified && <RegisterAnyway name={sel.name} onFlash={onFlash} />}
        </div>
      </div>
    </React.Fragment>
  );
}

function PluginsWindow({ onClose, onFlash, onConfigure }) {
  const [tab, setTab] = React.useState("installed"); // installed | attention | add
  const [q, setQ] = React.useState("");
  const [installed, setInstalled] = React.useState(() =>
    Object.fromEntries(PLUGIN_LIST.map((p) => [p.id, p.installed])));
  const [enabled, setEnabled] = React.useState(() =>
    Object.fromEntries(PLUGIN_LIST.map((p) => [p.id, p.installed && p.status !== "idle"])));
  const [selId, setSelId] = React.useState("git-helper");
  const [confirmId, setConfirmId] = React.useState(null); // uninstall confirm is bound to one plugin id
  // 2026-10-09 b10 — changing the selection cancels the confirm; Esc cancels it; focus lands on Cancel when it opens.
  const pick = (id) => { setSelId(id); setConfirmId(null); };
  React.useEffect(() => {
    if (!confirmId) return;
    const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); setConfirmId(null); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [confirmId]);

  const matches = (p) => {
    const n = q.trim().toLowerCase();
    return !n || `${p.name} ${p.author} ${p.cat} ${p.desc}`.toLowerCase().includes(n);
  };
  // The list is INSTALLED-ONLY. There is no catalog/marketplace browse — adding a
  // plugin means pointing Tasty at a LOCAL folder (the “Add plugin” tab). Online
  // catalog browse + install-by-id are deferred (docs/dev-guide/plugin-packaging.md).
  const list = PLUGIN_LIST.filter((p) => installed[p.id] && matches(p));
  const sel = PLUGIN_LIST.find((p) => p.id === selId);
  const selVisible = sel && installed[sel.id] && matches(sel);

  const uninstall = (p) => {
    setInstalled((m) => ({ ...m, [p.id]: false }));
    setEnabled((m) => ({ ...m, [p.id]: false }));
    onFlash && onFlash(`Removed ${p.name}`);
  };
  const toggleEnabled = (p) => setEnabled((m) => ({ ...m, [p.id]: !m[p.id] }));

  const installedCount = PLUGIN_LIST.filter((p) => installed[p.id]).length;

  // Attention = rejected/unregistered (signature/trust) + enabled-but-failing
  // (health error, pulled live from the installed list).
  const healthErrors = PLUGIN_LIST
    .filter((p) => installed[p.id] && enabled[p.id] && p.status === "error")
    .map((p) => ({ id: p.id, name: p.name, author: p.author, version: p.version, cat: p.cat,
      reason: "health-error", desc: p.desc,
      detail: { error: "Failed to reach the configured cluster at https://10.0.4.11:6443\nkubeconfig context \"prod\" — connection refused" } }));
  const attention = [...ATTENTION_LIST, ...healthErrors];
  const attentionCount = attention.length;

  const Seg = ({ value, label, count, danger }) => {
    const on = tab === value;
    return (
      <button onClick={() => setTab(value)} style={{ appearance: "none", cursor: "pointer", border: 0,
        height: 26, padding: "0 var(--tasty-space-md)", borderRadius: "var(--tasty-radius-sm)", display: "inline-flex",
        alignItems: "center", gap: "var(--tasty-space-sm)", fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-term-sm)",
        fontWeight: on ? 600 : 400, color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-on-active)",
        background: on ? "var(--tasty-surface-raised)" : "transparent",
        boxShadow: on ? "inset 0 0 0 var(--tasty-border-width) var(--tasty-border-default)" : "none" }}>
        {label}
        {count != null && !danger && <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)",
          color: on ? "var(--tasty-text-secondary-raised)" : "var(--tasty-text-on-active)" }}>{count}</span>}
        {danger && count > 0 && <Badge variant="danger">{count}</Badge>}
      </button>
    );
  };

  const Mono = ({ children }) => (
    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
      letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>{children}</div>
  );

  function detail() {
    if (!sel || !selVisible)
      return (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 13, color: "var(--tasty-text-muted)" }}>
          {list.length ? "Select a plugin" : "No plugins to show"}
        </div>
      );
    const isInstalled = installed[sel.id];
    const isOn = enabled[sel.id];
    return (
      <>
        <div className="tasty-scroll" style={{ flex: 1, overflow: "auto", padding: "var(--tasty-space-lg)",
          display: "flex", flexDirection: "column", gap: 16 }}>
          {/* identity */}
          <div style={{ display: "flex", gap: "var(--tasty-space-md)", alignItems: "flex-start" }}>
            <PluginAvatar plugin={sel} size="lg" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-normal)", color: "var(--tasty-text-primary)" }}>{sel.name}</span>
                <Tag>{"v" + sel.version}</Tag>
                {sel.builtin && <Tag>built-in</Tag>}
                {sel.agent && <Tag variant="agent">agent</Tag>}
              </div>
              {/* 2026-10-09 — meta: author(s) · plugin id · homepage link (each optional; id always present) */}
              <div style={{ marginTop: 4, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)",
                display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
{/* 2026-10-09 b10 — each separator travels WITH the item after it (one nowrap span), so a wrapped item never leaves a dangling · */}
                {[sel.author, sel.id, sel.homepage && <a href={sel.homepage} target="_blank" rel="noreferrer" style={{ color: "var(--tasty-accent-primary-raised)", textDecoration: "underline", textUnderlineOffset: 2 }}>{sel.homepage.replace(/^https?:\/\//, "")}</a>].filter(Boolean).map((part, i) => (
                  <span key={i} style={{ display: "inline-flex", gap: 8, whiteSpace: "nowrap" }}>{i > 0 && <span>·</span>}{part}</span>
                ))}
              </div>
            </div>

          </div>

          <p style={{ margin: 0, fontSize: 13, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)", maxWidth: "var(--tasty-measure-lg)" }}>{sel.desc}</p>

          {sel.status === "error" && isInstalled && isOn && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "var(--tasty-space-sm) var(--tasty-space-md)",
              borderRadius: "var(--tasty-radius)", fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-accent-danger-raised)",
              background: "color-mix(in srgb, var(--tasty-accent-danger) 12%, transparent)",
              border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-danger) 35%, transparent)" }}>
              <Icon name="alertCircle" size={16} />
              <span>Failed to connect. Check the plugin's configuration in Settings.</span>
            </div>
          )}

          <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
            <Mono>Permissions</Mono>
            <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>
              {sel.perms.map((p) => <Tag key={p}>{p}</Tag>)}
            </div>
          </div>

          {sel.cmd && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              <Mono>Command</Mono>
              <div style={{ display: "flex", alignItems: "center", gap: 16, minHeight: "var(--tasty-settings-row-min-height)",
                borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
                <span style={{ flex: 1, fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-secondary)" }}>{sel.cmd}</span>
                {sel.key && <Kbd keys={kbdKeys(sel.key)} />}
              </div>
            </div>
          )}

          {sel.kinds && sel.kinds.length > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              <Mono>Surface kinds</Mono>
              <div style={{ display: "flex", gap: "var(--tasty-space-sm)", flexWrap: "wrap" }}>
                {sel.kinds.map((k) => <Tag key={k}>{k}</Tag>)}
              </div>
            </div>
          )}

          {/* 2026-10-07 — install path + log path. Open folder sits on the caption row (right), never on the
              path row, so a long path can't push it out at 720 or 880. Paths wrap at any character
              (break-all), stay selectable — nothing is truncated, so no tooltip is needed. */}
          {isInstalled && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
              <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                <Mono>Install path</Mono>
                <div style={{ flex: 1 }} />
                <Button variant="secondary" size="sm" leadingIcon={<Icon name="folder" />}>Open folder</Button>
              </div>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)",
                wordBreak: "break-all", userSelect: "text" }}>{`/home/tasty/.local/share/tasty/plugins/${sel.id}`}</span>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)",
                wordBreak: "break-all", userSelect: "text" }}>{`Log: /home/tasty/.local/state/tasty/plugins/${sel.id}/plugin.log`}</span>
            </div>
          )}
        </div>

        {/* action bar */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-size-14)",
          borderTop: "var(--tasty-border-width) solid var(--tasty-separator)", flex: "none" }}>
          {isInstalled && confirmId === sel.id && (
            <>
              {/* 2026-10-09 — uninstall confirm REPLACES the bar contents in place. b10: the bar grows with its text (max(text, control 28) + space-md × 2); the note may wrap. */}
              <span style={{ display: "inline-flex", flex: "none", color: "var(--tasty-accent-attention-raised)" }}><Icon name="alertTriangle" size={16} /></span>
              <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
                <span style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-primary)" }}>Uninstall {sel.name}?</span>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>
                  {sel.builtin ? "Built-in — it won't be installed again on the next launch." : "Its files are removed. Settings stay until you delete them."}
                </span>
              </div>
              <Button variant="ghost" autoFocus onClick={() => setConfirmId(null)}>Cancel</Button>
              <Button variant="danger" onClick={() => { setConfirmId(null); uninstall(sel); }}>Uninstall</Button>
            </>
          )}
          {isInstalled && confirmId !== sel.id && (
            <>
              <label style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer" }}>
                <Switch checked={isOn} onChange={() => toggleEnabled(sel)} />
                <span style={{ fontSize: 13, color: "var(--tasty-text-secondary)" }}>{isOn ? "Enabled" : "Disabled"}</span>
              </label>
              <div style={{ flex: 1 }} />
              <Button variant="ghost" leadingIcon={ic.settings}
                onClick={() => onConfigure && onConfigure()}>Configure</Button>
              <Button variant="secondary" onClick={() => setConfirmId(sel.id)}
                style={{ color: "var(--tasty-accent-danger-raised)" }}>Uninstall</Button>
            </>
          )}
        </div>
      </>
    );
  }

  return (
    <Scrim onClose={onClose}>
      <div style={{ width: 820, height: 540, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
        border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden",
        boxShadow: "var(--tasty-shadow-modal)" }}>
        {/* header */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", height: 48, flex: "none", padding: "0 var(--tasty-size-14)",
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-brand-melon-flesh)" }}>
            <Icon name="plug" size={16} />
          </span>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--tasty-text-primary)", letterSpacing: "var(--tasty-letter-spacing-ui)" }}>Plugins</span>
          <span style={{ width: 1, height: 20, background: "var(--tasty-separator)", margin: "0 var(--tasty-space-sm)" }} />
          <div style={{ display: "flex", gap: 2, padding: 2, borderRadius: "var(--tasty-radius)",
            background: "var(--tasty-surface-active)" }}>
            <Seg value="installed" label="Installed" count={installedCount} />
            <Seg value="attention" label="Attention" count={attentionCount} danger />
            <Seg value="add" label="Add plugin" />
          </div>
          <div style={{ flex: 1 }} />
          {tab === "installed" && (
            <Input icon={ic.search} placeholder="Filter installed…"
              value={q} onChange={(e) => setQ(e.target.value)} style={{ width: "var(--tasty-field-width-lg)" }} />
          )}
          <IconButton aria-label="Close" onClick={onClose}>
            <Icon name="close" />
          </IconButton>
        </div>

        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          {tab === "add" ? (
            <AddPluginForm
              onAdded={(m) => { onFlash && onFlash("Added " + m.name); setTab("installed"); }}
              onCancel={() => setTab("installed")} />
          ) : tab === "attention" ? (
            <AttentionPanel items={attention} onFlash={onFlash} onConfigure={onConfigure} />
          ) : (
          <React.Fragment>
          {/* list */}
          <div className="tasty-scroll" style={{ width: "var(--tasty-plugins-list-width)", flex: "none", overflow: "auto", padding: 8,
            background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)",
            display: "flex", flexDirection: "column", gap: 3 }}>
            {list.map((p) => {
              const on = p.id === selId;
              const isOn = installed[p.id] && enabled[p.id];
              return (
                <div key={p.id} onClick={() => pick(p.id)} style={{ display: "flex", alignItems: "center",
                  gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-sm)", borderRadius: "var(--tasty-radius)", cursor: "pointer",
                  background: on ? "var(--tasty-surface-active)" : "transparent",
                  boxShadow: on ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none" }}>
                  <PluginAvatar plugin={p} size="sm" />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                      <span style={{ fontSize: 13, color: on ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)",
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.name}</span>
                    </div>
                    <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", color: on ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)",
                      overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {p.author} · v{p.version}
                    </div>
                  </div>
                  {installed[p.id] ? (
                    !isOn && <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10,
                      color: on ? "var(--tasty-text-on-active)" : "var(--tasty-text-muted)" }}>off</span>
                  ) : (
                    <span style={{ flex: "none", fontFamily: "var(--tasty-font-mono)", fontSize: 10,
                      color: "var(--tasty-text-muted)" }}>{p.cat}</span>
                  )}
                </div>
              );
            })}
            {list.length === 0 && (
              <div style={{ padding: 14, fontSize: "var(--tasty-font-size-term-sm)", color: "var(--tasty-text-muted)", textAlign: "center" }}>
                {q ? "No matches" : tab === "installed" ? "Nothing installed yet" : "All caught up"}
              </div>
            )}
          </div>

          {/* detail */}
          <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
            {detail()}
          </div>
          </React.Fragment>
          )}
        </div>
      </div>
    </Scrim>
  );
}

const pluginAttentionCount = ATTENTION_LIST.length + PLUGIN_LIST.filter((p) => p.installed && p.status === "error").length;

export { PluginsWindow, TrustBox, ADD_BLOCKED, SAMPLE_MANIFEST, pluginAttentionCount };
