import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { Checkbox } from "../../../components/forms/Checkbox.jsx";
import { Input } from "../../../components/forms/Input.jsx";
import { Select } from "../../../components/forms/Select.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim, Spinner } from "./shared.jsx";

// Tasty UI kit — Remote connections popup (Tools › Remote connections…).
// Redesign of the former single-SSH ssh_tool into a TYPE-AGNOSTIC remote-profile
// store + an Attach (tasty-attach) store + a separate credential (Passkey) store,
// behind three top tabs (Remote profiles · Attach · Passkeys).
// Mirrors (planned) zilhak/tasty → src/adapters/ui/popup/remote_tool.rs
//   Popup: id "remote_tool", headless (content-drawn header),
//   close_on_outside_click: false (×/Close/Esc only), sticky_focus: false.
//   Size = 520×460. Shown over the modal Scrim.
//   Profiles: ~/.tasty/remote-profiles.toml (no secrets — passkey referenced by name).
//   Passkeys: ~/.tasty/passkeys.toml (path|inline; value never shown unless Reveal).
// Each tab routes List · Form (add/edit) · ConfirmDelete off the common header.



// Types with a dedicated form. Everything else (incl. smb/http for now) uses the
// generic key-value editor. The list is OPEN — users may type any string.
const KNOWN_TYPES = ["ssh", "smb", "http"];   // suggested in the combobox
const DEDICATED_TYPES = ["ssh"];              // 1st-version dedicated form = ssh only
const SHELL_OPTIONS = ["auto", "bash", "zsh", "fish", "sh", "powershell"];
const KIND_OPTIONS = ["path", "inline"];
// Attach: how tasty discovers the remote instance's control port.
const PORT_MODES = ["auto", "subcommand", "file-unix", "file-windows"];
const ATTACH_MODES = ["ref", "inline"];   // reference an ssh profile, or hold ssh info inline

// ── seed data ───────────────────────────────────────────────────────────
const SEED_PROFILES = [
  { id: "p1", type: "ssh", name: "prod-web", label: "us-east", host: "10.0.4.12", user: "deploy", port: "22",
    remoteTasty: "/usr/local/bin/tasty", shell: "zsh", state: "ok", passkeyRef: "ed25519-main", fields: [] },
  { id: "p2", type: "ssh", name: "db-primary", label: "", host: "db.internal", user: "postgres", port: "2222",
    remoteTasty: "", shell: "bash", state: "ok", passkeyRef: "", fields: [] },
  { id: "p3", type: "ssh", name: "edge-cache", label: "staging", host: "edge.example.com", user: "root", port: "22",
    remoteTasty: "", shell: "auto", state: "detecting", passkeyRef: "edge-pem", fields: [] },
  { id: "p4", type: "ssh", name: "legacy-box", label: "", host: "192.168.1.40", user: "admin", port: "22",
    remoteTasty: "", shell: "sh", state: "failed", passkeyRef: "old-rsa", fields: [] },
  { id: "p5", type: "smb", name: "media-nas", label: "lab", passkeyRef: "nas-cred", state: "ok",
    fields: [{ key: "host", value: "nas.local" }, { key: "share", value: "media" }] },
  { id: "p6", type: "http", name: "grafana", label: "", passkeyRef: "", state: "ok",
    fields: [{ key: "url", value: "https://grafana.internal" }] },
  { id: "p7", type: "snb", name: "scratch", label: "", passkeyRef: "", state: "ok",
    fields: [{ key: "endpoint", value: "10.2.2.9" }] },
];
// Attach entries — the info tasty needs to attach to a REMOTE tasty instance.
// Either references an ssh profile (mode:"ref" + sshRef) or holds ssh info inline
// (mode:"inline" + host/user/port/shell/passkeyRef). remoteTasty + portMode +
// portFile describe how to launch/find the remote instance.
const SEED_ATTACHES = [
  { id: "a1", name: "gb10", label: "us-east", mode: "ref", sshRef: "prod-web",
    remoteTasty: "tasty", portMode: "auto", portFile: "", state: "ok" },
  { id: "a2", name: "edge-direct", label: "", mode: "inline", host: "edge.example.com", user: "root",
    port: "22", shell: "auto", passkeyRef: "edge-pem", remoteTasty: "/opt/tasty/bin/tasty",
    portMode: "file-unix", portFile: "/run/user/1000/tasty/port", state: "ok" },
  { id: "a3", name: "legacy-attach", label: "", mode: "ref", sshRef: "legacy-box",
    remoteTasty: "tasty", portMode: "subcommand", portFile: "", state: "inactive" },
];
const SEED_PASSKEYS = [
  { id: "k1", name: "ed25519-main", kind: "path", value: "~/.ssh/id_ed25519" },
  { id: "k2", name: "edge-pem", kind: "path", value: "~/.ssh/edge.pem" },
  { id: "k3", name: "nas-cred", kind: "inline", value: "smb://user:hunter2@nas.local" },
  { id: "k4", name: "deploy-token", kind: "inline", value: "ghp_8sd7f6as9d8f7a6sd9f8a7sdf" },
];

const blankProfile = () => ({ id: null, type: "ssh", name: "", label: "", host: "", user: "", port: "",
  shell: "auto", passkeyRef: "", fields: [] });
const blankPasskey = () => ({ id: null, name: "", kind: "path", value: "" });
const blankAttach = () => ({ id: null, name: "", label: "", mode: "ref", sshRef: "", host: "", user: "",
  port: "", shell: "auto", passkeyRef: "", remoteTasty: "tasty", portMode: "auto", portFile: "" });
const attachTarget = (a) => a.mode === "ref"
  ? `\u2192 ${a.sshRef || "?"}`
  : `${a.user ? a.user + "@" : ""}${a.host || "?"}${a.port && a.port !== "22" ? ":" + a.port : ""}`;

const isUnknownType = (t) => !!t && !KNOWN_TYPES.includes(t.trim());
const isSshType = (t) => DEDICATED_TYPES.includes((t || "").trim());
const sshTarget = (p) => `${p.user ? p.user + "@" : ""}${p.host || "?"}${p.port && p.port !== "22" ? ":" + p.port : ""}`;
const genericSummary = (p) => (p.fields || []).filter((f) => f.key || f.value).slice(0, 2)
  .map((f) => `${f.key || "?"}=${f.value || ""}`).join("  ") || "—";
const profileSummary = (p) => (isSshType(p.type) ? sshTarget(p) : genericSummary(p));
const MASK = "••••••••";

// shared text styles (uniquely named — this file is concatenated into the bundle)
const rtMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" };
const rtCaption = { fontSize: 11, color: "var(--tasty-text-muted)" };
const rtLabel = { fontSize: 13, color: "var(--tasty-text-muted)", textAlign: "right" };
const rtFormTitle = { fontSize: 13, fontWeight: 600, color: "var(--tasty-text-primary)", marginBottom: 12 };
const rtScrollPad = { flex: 1, minHeight: 0, overflow: "auto", padding: "var(--tasty-size-14) var(--tasty-space-lg)" };
const rtFooter = { display: "flex", justifyContent: "flex-end", gap: 8, padding: "var(--tasty-space-md) var(--tasty-space-lg)", flex: "none",
  borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" };
const rtLinkBtn = { appearance: "none", border: 0, background: "transparent", cursor: "pointer", padding: 0,
  fontFamily: "var(--tasty-font-ui)", fontSize: 11, color: "var(--tasty-accent-primary-raised)" };

// icons (lucide-style, drawn through TastyKit.Icon)
// glyph names from the canonical set (icons/*.svg via <Icon name>)
const RD = {
  refresh: "refresh",
  edit: "edit",
  trash: "trash",
  eye: "eye",
  eyeOff: "eyeOff",
  folder: "folder",
  x: "close",
  warn: "alertTriangle",
  funnel: "filter",
};

// ── small warning pill (type-unknown / dangling-ref / unknown-kind) ──────
function WarnBadge({ children, title }) {
  return (
    <span title={title} style={{ display: "inline-flex", alignItems: "center", gap: 4, height: 16,
      padding: "0 var(--tasty-space-sm)", borderRadius: "var(--tasty-radius-sm)", whiteSpace: "nowrap",
      fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", fontWeight: 500, lineHeight: 1,
      color: "var(--tasty-tag-warning-fg)",
      border: "var(--tasty-border-width) solid color-mix(in srgb, var(--tasty-accent-warning) 40%, transparent)",
      background: "var(--tasty-tag-bg)" }}>
      <span style={{ display: "inline-flex" }}><Icon name={RD.warn} size={12} /></span>{children}
    </span>
  );
}

// ════════════════════════════════════════════════════════════════════════
// TAB A — Remote profiles
// ════════════════════════════════════════════════════════════════════════
function ProfileRow({ p, passkeyNames, onRedetect, onEdit, onDelete }) {
  const ssh = isSshType(p.type);
  const unknown = isUnknownType(p.type);
  const disabled = p.state === "failed";
  const dangling = p.passkeyRef && !passkeyNames.includes(p.passkeyRef);
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        {/* row 1 — name + type badge */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ fontSize: 13, fontWeight: 600, lineHeight: "var(--tasty-line-height-ui)", flex: "0 1 auto",
            color: disabled ? "var(--tasty-text-disabled)" : "var(--tasty-text-primary)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {p.name}{p.label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({p.label})</span>}
          </span>
          {unknown
            ? <WarnBadge title="Unknown type — no core feature or plugin handles it.">{p.type}</WarnBadge>
            : <Tag>{p.type}</Tag>}
        </div>
        {/* row 2 — target summary */}
        <div style={{ ...rtMono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{profileSummary(p)}</div>
        {/* row 3 — passkey ref + (ssh) shell/detect state */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", marginTop: 1, minWidth: 0, flexWrap: "wrap" }}>
          {p.passkeyRef
            ? (dangling
                ? <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                    <span style={{ ...rtCaption, fontFamily: "var(--tasty-font-mono)" }}>passkey: {p.passkeyRef}</span>
                    <WarnBadge title="Referenced passkey not found.">passkey missing</WarnBadge>
                  </span>
                : <span style={{ ...rtCaption, fontFamily: "var(--tasty-font-mono)" }}>passkey: {p.passkeyRef}</span>)
            : <span style={rtCaption}>passkey: <span style={{ fontFamily: "var(--tasty-font-mono)" }}>—</span></span>}
          {ssh && (
            <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
              <span style={rtCaption}>shell: <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{p.shell}</span></span>
              {p.state === "detecting" && (
                <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-xs)", ...rtCaption }}>
                  <Spinner size={12} /> detecting…
                </span>
              )}
              {p.state === "failed" && (
                <span style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>detection failed (disabled)</span>
              )}
            </span>
          )}
        </div>
      </div>
      {/* actions */}
      <div style={{ display: "flex", alignItems: "center", gap: 1, flex: "none" }}>
        {ssh && (
          <IconButton size="sm" aria-label="Re-detect" title="Probe the remote and update the discovery mode"
            disabled={p.state === "detecting"} onClick={() => onRedetect(p)}>
            <Icon name={RD.refresh} size={16} />
          </IconButton>
        )}
        <IconButton size="sm" aria-label="Edit" title="Edit" onClick={() => onEdit(p)}>
          <Icon name={RD.edit} size={16} />
        </IconButton>
        <IconButton size="sm" aria-label="Delete" title="Delete" onClick={() => onDelete(p)}>
          <Icon name={RD.trash} size={16} />
        </IconButton>
      </div>
    </div>
  );
}

const SSH_FIELDS = [
  { key: "name", label: "Name", ph: "prod-web" },
  { key: "host", label: "Host", ph: "10.0.4.12", mono: true },
  { key: "user", label: "User", ph: "deploy" },
  { key: "port", label: "Port", ph: "22", mono: true },
  { key: "label", label: "Label", ph: "us-east" },
];

function PasskeySelect({ value, onChange, passkeys }) {
  return (
    <Select block value={value} onChange={onChange}>
      <option value="">(none)</option>
      {passkeys.map((k) => <option key={k.id} value={k.name}>{k.name}</option>)}
    </Select>
  );
}

function ProfileForm({ draft, isNew, error, passkeys, onField, onFields, onSave, onCancel }) {
  const ssh = isSshType(draft.type);
  const unknown = isUnknownType(draft.type);
  const setField = (i, k, v) => { const next = draft.fields.map((f, j) => (j === i ? { ...f, [k]: v } : f)); onFields(next); };
  const addField = () => onFields([...(draft.fields || []), { key: "", value: "" }]);
  const delField = (i) => onFields(draft.fields.filter((_, j) => j !== i));

  return (
    <>
      <div className="tasty-scroll" style={rtScrollPad}>
        <div style={rtFormTitle}>{isNew ? "New profile" : "Edit profile"}</div>

        {/* Type — editable combobox (datalist gives suggestions + free text) */}
        <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
          <label style={rtLabel}>Type</label>
          <div>
            <Input block mono list="rt-known-types" placeholder="ssh" value={draft.type}
              onChange={(e) => onField("type", e.target.value)} />
            <datalist id="rt-known-types">{KNOWN_TYPES.map((t) => <option key={t} value={t} />)}</datalist>
          </div>
        </div>
        {unknown && (
          <p style={{ margin: "var(--tasty-space-sm) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)",
            fontSize: 11, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-accent-warning-raised)", maxWidth: 320 }}>
            <span style={{ flex: "none", marginTop: 1 }}><Icon name={RD.warn} size={12} /></span>
            Unknown type — will be saved but not handled until a plugin or core supports it.
          </p>
        )}

        <div style={{ height: "var(--tasty-space-sm)" }} />

        {ssh ? (
          /* dedicated SSH form */
          <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
            {SSH_FIELDS.map((f) => (
              <React.Fragment key={f.key}>
                <label style={rtLabel}>{f.label}</label>
                <Input block mono={f.mono} placeholder={f.ph} value={draft[f.key]}
                  onChange={(e) => onField(f.key, e.target.value)} />
              </React.Fragment>
            ))}
            <label style={rtLabel}>Shell</label>
            <Select block options={SHELL_OPTIONS} value={draft.shell} onChange={(e) => onField("shell", e.target.value)} />
            <label style={rtLabel}>Passkey</label>
            <PasskeySelect value={draft.passkeyRef} passkeys={passkeys} onChange={(e) => onField("passkeyRef", e.target.value)} />
          </div>
        ) : (
          /* generic key-value form (smb / http / unknown) */
          <>
            <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
              <label style={rtLabel}>Name</label>
              <Input block placeholder="media-nas" value={draft.name} onChange={(e) => onField("name", e.target.value)} />
              <label style={rtLabel}>Label</label>
              <Input block placeholder="lab" value={draft.label} onChange={(e) => onField("label", e.target.value)} />
              <label style={rtLabel}>Passkey</label>
              <PasskeySelect value={draft.passkeyRef} passkeys={passkeys} onChange={(e) => onField("passkeyRef", e.target.value)} />
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", margin: "var(--tasty-size-14) 0 var(--tasty-space-sm)" }}>
              <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
                letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Fields</span>
              <Button variant="ghost" size="sm" leadingIcon={ic.plus} onClick={addField}>Add field</Button>
            </div>
            {(draft.fields || []).length === 0 ? (
              <p style={{ ...rtCaption, fontStyle: "italic", margin: "var(--tasty-size-2) 0 0" }}>No fields. Add a key-value pair.</p>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
                {draft.fields.map((f, i) => (
                  <div key={i} style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr var(--tasty-control-height)", gap: "var(--tasty-space-sm)", alignItems: "center" }}>
                    <Input block mono placeholder="key" value={f.key} onChange={(e) => setField(i, "key", e.target.value)} />
                    <Input block mono placeholder="value" value={f.value} onChange={(e) => setField(i, "value", e.target.value)} />
                    <IconButton size="sm" aria-label="Remove field" title="Remove field" onClick={() => delField(i)}>
                      <Icon name={RD.x} size={14} />
                    </IconButton>
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {ssh && draft.shell === "auto" && (
          <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", ...rtCaption, lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-sm)" }}>
            Auto-detects the remote shell once when saved (runs an SSH probe).
          </p>
        )}
        {error && <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>{error}</p>}
      </div>
      <div style={rtFooter}>
        <Button variant="ghost" onClick={onCancel}>Cancel</Button>
        <Button variant="primary" onClick={onSave}>Save</Button>
      </div>
    </>
  );
}

// ════════════════════════════════════════════════════════════════════════
// TAB B — Passkeys
// ════════════════════════════════════════════════════════════════════════
function PasskeyRow({ k, revealed, onReveal, onEdit, onDelete }) {
  const unknownKind = !KIND_OPTIONS.includes(k.kind);
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ fontSize: 13, fontWeight: 600, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-primary)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{k.name}</span>
          {unknownKind ? <WarnBadge title="Unknown kind.">{k.kind}</WarnBadge> : <Tag>{k.kind}</Tag>}
        </div>
        <div style={{ ...rtMono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {k.kind} · {revealed ? k.value : MASK}
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 1, flex: "none" }}>
        {/* 2026-10-06: revealed = IconButton active (accent glyph + overlay-active) AND eye → eyeOff. Row delete = trash on all three tabs (x is reserved for Close / Dismiss). */}
        <IconButton size="sm" active={revealed} aria-label="Reveal"
          title={revealed ? "Hide value" : "Show value (local only)"} onClick={() => onReveal(k)}>
          <Icon name={revealed ? RD.eyeOff : RD.eye} size={16} />
        </IconButton>
        <IconButton size="sm" aria-label="Edit" title="Edit" onClick={() => onEdit(k)}>
          <Icon name={RD.edit} size={16} />
        </IconButton>
        <IconButton size="sm" aria-label="Delete" title="Delete" onClick={() => onDelete(k)}>
          <Icon name={RD.trash} size={16} />
        </IconButton>
      </div>
    </div>
  );
}

function PasskeyForm({ draft, isNew, error, onField, onSave, onCancel, onBrowse }) {
  const [show, setShow] = React.useState(false);
  return (
    <>
      <div className="tasty-scroll" style={rtScrollPad}>
        <div style={rtFormTitle}>{isNew ? "New passkey" : "Edit passkey"}</div>
        <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
          <label style={rtLabel}>Name</label>
          <Input block placeholder="ed25519-main" value={draft.name} onChange={(e) => onField("name", e.target.value)} />

          <label style={rtLabel}>Kind</label>
          <div style={{ display: "inline-flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)",
            borderRadius: "var(--tasty-radius)", overflow: "hidden", width: "fit-content" }}>
            {KIND_OPTIONS.map((opt, i) => {
              const on = draft.kind === opt;
              return (
                <button key={opt} onClick={() => onField("kind", opt)} style={{ appearance: "none", cursor: "pointer",
                  border: 0, borderLeft: i ? "var(--tasty-border-width) solid var(--tasty-border-default)" : 0, padding: "0 var(--tasty-size-14)", height: 26,
                  fontFamily: "var(--tasty-font-mono)", fontSize: 12,
                  color: on ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)",
                  background: on ? "var(--tasty-accent-primary)" : "transparent" }}>{opt}</button>
              );
            })}
          </div>

          <label style={{ ...rtLabel, alignSelf: draft.kind === "inline" ? "start" : "center", marginTop: draft.kind === "inline" ? 6 : 0 }}>Value</label>
          {draft.kind === "path" ? (
            <div style={{ display: "flex", gap: "var(--tasty-space-sm)" }}>
              <Input block mono placeholder="~/.ssh/id_ed25519" value={draft.value}
                onChange={(e) => onField("value", e.target.value)} />
              <Button variant="secondary" size="sm" leadingIcon={ic.folder ? undefined : undefined} onClick={onBrowse}>
                <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--tasty-space-xs)" }}><Icon name={RD.folder} size={14} />Browse…</span>
              </Button>
            </div>
          ) : (
            <div>
              <div style={{ position: "relative" }}>
                <textarea value={draft.value} onChange={(e) => onField("value", e.target.value)}
                  placeholder="Paste secret / key contents" rows={4} spellCheck={false}
                  style={{ width: "100%", boxSizing: "border-box", resize: "vertical", minHeight: 64,
                    padding: "var(--tasty-space-sm) var(--tasty-size-32) var(--tasty-space-sm) var(--tasty-space-sm)", border: "var(--tasty-border-width) solid var(--tasty-border-default)",
                    borderRadius: "var(--tasty-radius)", background: "var(--tasty-surface-raised)",
                    color: "var(--tasty-text-primary)", fontFamily: "var(--tasty-font-mono)", fontSize: 11,
                    lineHeight: "var(--tasty-line-height-ui)", outline: "none",
                    WebkitTextSecurity: show ? "none" : "disc", textSecurity: show ? "none" : "disc" }} />
                <span style={{ position: "absolute", top: 6, right: 6 }}>
                  <IconButton size="sm" active={show} aria-label="Show value"
                    title={show ? "Hide value" : "Show value (local only)"} onClick={() => setShow((s) => !s)}>
                    <Icon name={show ? RD.eyeOff : RD.eye} size={14} />
                  </IconButton>
                </span>
              </div>
            </div>
          )}
        </div>
        <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", ...rtCaption, lineHeight: "var(--tasty-line-height-ui)", maxWidth: 320 }}>
          The value is local-only. Profiles reference this passkey by name; the secret is never shared.
        </p>
        {error && <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>{error}</p>}
      </div>
      <div style={rtFooter}>
        <Button variant="ghost" onClick={onCancel}>Cancel</Button>
        <Button variant="primary" onClick={onSave}>Save</Button>
      </div>
    </>
  );
}

// ════════════════════════════════════════════════════════════════════════
// TAB C — Attach (tasty-attach targets)
// ════════════════════════════════════════════════════════════════════════
function AttachRow({ a, sshProfileNames, onEdit, onDelete }) {
  const inactive = a.state === "inactive";
  const dangling = a.mode === "ref" && a.sshRef && !sshProfileNames.includes(a.sshRef);
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-xs)",
      borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        {/* row 1 — name + mode/state badge */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ fontSize: 13, fontWeight: 600, lineHeight: "var(--tasty-line-height-ui)", flex: "0 1 auto",
            color: inactive ? "var(--tasty-text-disabled)" : "var(--tasty-text-primary)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {a.name}{a.label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({a.label})</span>}
          </span>
          <Tag>{a.mode === "ref" ? "profile" : "inline"}</Tag>
          {inactive && <WarnBadge title="Inactive — the referenced ssh profile or inline shell isn't reachable.">inactive</WarnBadge>}
        </div>
        {/* row 2 — target summary (→ profile, or host) */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
          <span style={{ ...rtMono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{attachTarget(a)}</span>
          {dangling && <WarnBadge title="Referenced ssh profile not found.">profile missing</WarnBadge>}
        </div>
        {/* row 3 — remote tasty + port mode */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-md)", marginTop: 1, minWidth: 0, flexWrap: "wrap" }}>
          <span style={rtCaption}>tasty: <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{a.remoteTasty || "tasty"}</span></span>
          <span style={rtCaption}>port: <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{a.portMode}</span></span>
        </div>
      </div>
      {/* actions */}
      <div style={{ display: "flex", alignItems: "center", gap: 1, flex: "none" }}>
        <IconButton size="sm" aria-label="Edit" title="Edit" onClick={() => onEdit(a)}>
          <Icon name={RD.edit} size={16} />
        </IconButton>
        <IconButton size="sm" aria-label="Delete" title="Delete" onClick={() => onDelete(a)}>
          <Icon name={RD.trash} size={16} />
        </IconButton>
      </div>
    </div>
  );
}

function AttachForm({ draft, isNew, error, sshProfiles, passkeys, onField, onSave, onCancel }) {
  const ref = draft.mode === "ref";
  return (
    <>
      <div className="tasty-scroll" style={rtScrollPad}>
        <div style={rtFormTitle}>{isNew ? "New attach" : "Edit attach"}</div>

        <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
          <label style={rtLabel}>Name</label>
          <Input block placeholder="gb10" value={draft.name} onChange={(e) => onField("name", e.target.value)} />
          <label style={rtLabel}>Label</label>
          <Input block placeholder="us-east" value={draft.label} onChange={(e) => onField("label", e.target.value)} />

          {/* connection mode toggle */}
          <label style={rtLabel}>Connection</label>
          <div style={{ display: "inline-flex", border: "var(--tasty-border-width) solid var(--tasty-border-default)",
            borderRadius: "var(--tasty-radius)", overflow: "hidden", width: "fit-content" }}>
            {[["ref", "SSH profile"], ["inline", "Direct (inline)"]].map(([opt, lbl], i) => {
              const on = draft.mode === opt;
              return (
                <button key={opt} onClick={() => onField("mode", opt)} style={{ appearance: "none", cursor: "pointer",
                  border: 0, borderLeft: i ? "var(--tasty-border-width) solid var(--tasty-border-default)" : 0, padding: "0 var(--tasty-size-14)", height: 26,
                  fontFamily: "var(--tasty-font-ui)", fontSize: 12,
                  color: on ? "var(--tasty-text-on-accent)" : "var(--tasty-text-secondary)",
                  background: on ? "var(--tasty-accent-primary)" : "transparent" }}>{lbl}</button>
              );
            })}
          </div>
        </div>

        <div style={{ height: "var(--tasty-space-sm)" }} />

        {ref ? (
          /* reference an existing ssh profile */
          <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
            <label style={rtLabel}>SSH profile</label>
            <Select block value={draft.sshRef} onChange={(e) => onField("sshRef", e.target.value)}>
              <option value="">(select a profile)</option>
              {sshProfiles.map((p) => <option key={p.id} value={p.name}>{p.name}{p.label ? ` (${p.label})` : ""}</option>)}
            </Select>
          </div>
        ) : (
          /* inline ssh info — same fieldset as the ssh profile form */
          <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
            <label style={rtLabel}>Host</label>
            <Input block mono placeholder="10.0.4.12" value={draft.host} onChange={(e) => onField("host", e.target.value)} />
            <label style={rtLabel}>User</label>
            <Input block placeholder="deploy" value={draft.user} onChange={(e) => onField("user", e.target.value)} />
            <label style={rtLabel}>Port</label>
            <Input block mono placeholder="22" value={draft.port} onChange={(e) => onField("port", e.target.value)} />
            <label style={rtLabel}>Shell</label>
            <Select block options={SHELL_OPTIONS} value={draft.shell} onChange={(e) => onField("shell", e.target.value)} />
            <label style={rtLabel}>Passkey</label>
            <PasskeySelect value={draft.passkeyRef} passkeys={passkeys} onChange={(e) => onField("passkeyRef", e.target.value)} />
          </div>
        )}

        <div style={{ display: "flex", alignItems: "center", margin: "var(--tasty-size-14) 0 var(--tasty-space-sm)" }}>
          <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
            letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Remote tasty</span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "var(--tasty-remote-label-col) 1fr", columnGap: 12, rowGap: 8, alignItems: "center" }}>
          <label style={rtLabel}>Executable</label>
          <Input block mono placeholder="tasty" value={draft.remoteTasty} onChange={(e) => onField("remoteTasty", e.target.value)} />
          <label style={rtLabel}>Port mode</label>
          <Select block options={PORT_MODES} value={draft.portMode} onChange={(e) => onField("portMode", e.target.value)} />
          <label style={rtLabel}>Port file</label>
          <Input block mono placeholder="optional — overrides port mode" value={draft.portFile} onChange={(e) => onField("portFile", e.target.value)} />
        </div>
        <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", ...rtCaption, lineHeight: "var(--tasty-line-height-ui)", maxWidth: "var(--tasty-measure-sm)" }}>
          Point <span style={{ fontFamily: "var(--tasty-font-mono)" }}>Executable</span> at the remote <span style={{ fontFamily: "var(--tasty-font-mono)" }}>tasty</span> binary when it isn't on <span style={{ fontFamily: "var(--tasty-font-mono)" }}>PATH</span>. A <span style={{ fontFamily: "var(--tasty-font-mono)" }}>Port file</span> path takes precedence over the selected port mode.
        </p>
        {error && <p style={{ margin: "var(--tasty-space-md) 0 0 calc(var(--tasty-remote-label-col) + var(--tasty-space-md))", fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>{error}</p>}
      </div>
      <div style={rtFooter}>
        <Button variant="ghost" onClick={onCancel}>Cancel</Button>
        <Button variant="primary" onClick={onSave}>Save</Button>
      </div>
    </>
  );
}

// ── shared confirm-delete view ──────────────────────────────────────────
function ConfirmDelete({ noun, name, hint, onConfirm, onCancel }) {
  return (
    <>
      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", justifyContent: "center",
        gap: 8, padding: "var(--tasty-space-lg) var(--tasty-space-lg)" }}>
        <p style={{ margin: 0, fontSize: 13, lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-primary)" }}>
          Delete {noun} <span style={{ fontFamily: "var(--tasty-font-mono)" }}>“{name}”</span>?
        </p>
        {hint && <p style={{ margin: 0, ...rtCaption, lineHeight: "var(--tasty-line-height-ui)" }}>{hint}</p>}
      </div>
      <div style={rtFooter}>
        <Button variant="ghost" onClick={onCancel}>Cancel</Button>
        <Button variant="secondary" onClick={onConfirm} style={{ color: "var(--tasty-accent-danger-raised)" }}>Delete</Button>
      </div>
    </>
  );
}

// ── protocol filter — Profiles tab only, session-only / NON-PERSISTENT ────
// The protocol set is DYNAMIC: derived from the types present (the real app
// passes the runtime "displayable protocol set"). State is an EXCLUDED set —
// empty = no filter (default). Apply-on-confirm: edits stay in a draft until
// Apply. Never written to disk; tasty restarts with the full set selected.
function ProtocolFilter({ protocols, hidden, onApply }) {
  const [open, setOpen] = React.useState(false);
  const [draft, setDraft] = React.useState(() => new Set(hidden));
  const activeHidden = protocols.filter((p) => hidden.has(p));
  const filtered = activeHidden.length > 0;
  const selectedCount = protocols.length - activeHidden.length;

  const openPanel = () => { setDraft(new Set(hidden)); setOpen(true); };
  const apply = () => { onApply(new Set([...draft].filter((p) => protocols.includes(p)))); setOpen(false); };
  const toggle = (proto) => setDraft((d) => { const n = new Set(d); n.has(proto) ? n.delete(proto) : n.add(proto); return n; });

  React.useEffect(() => {
    if (!open) return;
    const h = (e) => { if (e.key === "Escape") { e.stopImmediatePropagation(); setOpen(false); } };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  }, [open]);

  return (
    <div style={{ position: "relative", flex: "none" }}>
      <Button variant={filtered ? "primary" : "secondary"} size="sm" leadingIcon={<Icon name={RD.funnel} size={14} />}
        onClick={() => (open ? setOpen(false) : openPanel())}>
        Filter{filtered ? ` · ${selectedCount}/${protocols.length}` : ""}
      </Button>
      {open && (
        <>
          <div onClick={() => setOpen(false)} style={{ position: "fixed", inset: 0, zIndex: 40 }} />
          <div role="dialog" aria-label="Filter by protocol" style={{ position: "absolute", top: "calc(100% + var(--tasty-space-xs))", right: 0, zIndex: 41,
            width: "var(--tasty-remote-filter-dropdown-width)", boxSizing: "border-box", background: "var(--tasty-menu-bg)", border: "var(--tasty-border-width) solid var(--tasty-menu-border)", /* 2026-10-06: every anchored popover = menu container tokens */
            borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-popover)", display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <div style={{ padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)",
              fontFamily: "var(--tasty-font-mono)", fontSize: 10, textTransform: "uppercase",
              letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>Filter by protocol</div>
            <div className="tasty-scroll" style={{ maxHeight: 168, overflow: "auto", padding: "var(--tasty-space-sm) var(--tasty-space-md)",
              display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
              {protocols.length === 0 ? (
                <span style={{ ...rtCaption, fontStyle: "italic" }}>No protocols.</span>
              ) : protocols.map((proto) => (
                <div key={proto} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                  <Checkbox checked={!draft.has(proto)} onChange={() => toggle(proto)}
                    label={proto} /* 2026-10-06: shared Checkbox label (body, UI font) */ />
                  {isUnknownType(proto) && <WarnBadge title="Unknown type — no core feature or plugin handles it.">unknown</WarnBadge>}
                </div>
              ))}
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-sm) var(--tasty-space-md)",
              borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <button onClick={() => setDraft(new Set())} style={rtLinkBtn}>Select all</button>
              <span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-separator)" }}>·</span>
              <button onClick={() => setDraft(new Set(protocols))} style={rtLinkBtn}>Deselect all</button>
            </div>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--tasty-space-sm)",
              padding: "var(--tasty-space-sm) var(--tasty-space-md)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
              <Button variant="ghost" size="sm" onClick={() => setDraft(new Set())}>Reset</Button>
              <Button variant="primary" size="sm" onClick={apply}>Apply</Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// ── "From ssh config" — hosts read from ~/.ssh/config under the profile list ──
// Kit definition (2026-09-29). gallery/overlays-shared.jsx LocalSshSection is the
// static specimen of THIS component (state only) and must match it.
//   state          "hosts" | "empty" | "missing" | "unreadable"
//                  empty      = read OK, 0 hosts
//                  missing    = no file at ~/.ssh/config
//                  unreadable = exists but can't be read: any open error (permission
//                               denied included), not a regular file (a directory),
//                               or not valid UTF-8. One cause-neutral line (2026-09-29).
//   profileTargets user@host:port of existing ssh profiles → that host shows
//                  a muted "in profiles" Tag instead of the action
//   onImport(host) the existing import action (one ghost button per row)
const SSH_CONFIG_HOSTS = [
  { alias: "gb10", target: "maya@gb10.local:22" },
  { alias: "prod-web", target: "deploy@10.0.4.12:22" },
  { alias: "eu-west-build-farm-bastion-01", target: "ci-runner@bastion-01.eu-west.build.example.com:2222" },
];
function LocalSshSection({ state = "hosts", hosts = SSH_CONFIG_HOSTS, profileTargets = [], onImport }) {
  const caption = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
  const line = { padding: "var(--tasty-size-2) var(--tasty-space-xs) var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" };
  return (
    <div style={{ marginTop: "var(--tasty-space-md)", paddingTop: "var(--tasty-space-sm)", borderTop: "var(--tasty-border-width) solid var(--tasty-border-frame)" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: "var(--tasty-space-sm)", padding: "var(--tasty-size-2) var(--tasty-space-xs) var(--tasty-space-xs)" }}>
        <span style={{ fontSize: "var(--tasty-font-size-caption)", letterSpacing: "var(--tasty-letter-spacing-caps)", textTransform: "uppercase", color: "var(--tasty-text-secondary)" }}>From ssh config</span>
        <span style={caption}>~/.ssh/config</span>
        <div style={{ flex: 1 }} />
        {state === "hosts" && <span style={caption}>{hosts.length}</span>}
      </div>
      {state === "empty" && <div style={line}>No hosts in ~/.ssh/config.</div>}
      {state === "missing" && <div style={line}>No ~/.ssh/config found.</div>}
      {state === "unreadable" && <div style={line}>Can't read ~/.ssh/config. Check the file and its permissions.</div>}
      {state === "hosts" && hosts.map((h) => {
        const have = profileTargets.includes(h.target);
        return (
          <div key={h.alias} style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xs)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
            <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-label-detail-gap)" }}>
              <div style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{h.alias}</div>
              <div style={{ ...caption, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{h.target}</div>
            </div>
            {have
              ? <Tag>in profiles</Tag>
              : <Button variant="ghost" size="sm" leadingIcon={<Icon name="plus" size="var(--tasty-icon-size-sm)" />} onClick={() => onImport && onImport(h)}>Add profile</Button>}
          </div>
        );
      })}
    </div>
  );
}

// ── empty / add-bar list shell ──────────────────────────────────────────
function ListShell({ addLabel, onAdd, empty, children, hasItems, rightSlot }) {
  return (
    <>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--tasty-space-sm)",
        padding: "var(--tasty-space-md) var(--tasty-size-14) var(--tasty-space-sm)", flex: "none" }}>
        <Button variant="secondary" size="sm" leadingIcon={ic.plus} onClick={onAdd}>{addLabel}</Button>
        {rightSlot}
      </div>
      <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "0 var(--tasty-size-14) var(--tasty-space-sm)" }}>
        {hasItems ? children : (
          <div style={{ height: "100%", display: "flex", alignItems: "center", justifyContent: "center",
            textAlign: "center", padding: "0 var(--tasty-space-xl)", fontSize: "var(--tasty-font-size-term-sm)", fontStyle: "italic", color: "var(--tasty-text-muted)" }}>
            {empty}
          </div>
        )}
      </div>
    </>
  );
}

// ════════════════════════════════════════════════════════════════════════
function RemoteTool({ onClose, onFlash }) {
  const [tab, setTab] = React.useState("profiles");
  const [profiles, setProfiles] = React.useState(SEED_PROFILES);
  const [hidden, setHidden] = React.useState(() => new Set()); // excluded protocols (session-only)
  const [passkeys, setPasskeys] = React.useState(SEED_PASSKEYS);
  const [attaches, setAttaches] = React.useState(SEED_ATTACHES);
  const [pView, setPView] = React.useState({ type: "list" });
  const [kView, setKView] = React.useState({ type: "list" });
  const [aView, setAView] = React.useState({ type: "list" });
  const [pDraft, setPDraft] = React.useState(blankProfile());
  const [kDraft, setKDraft] = React.useState(blankPasskey());
  const [aDraft, setADraft] = React.useState(blankAttach());
  const [pErr, setPErr] = React.useState("");
  const [kErr, setKErr] = React.useState("");
  const [aErr, setAErr] = React.useState("");
  const [revealed, setRevealed] = React.useState(() => new Set());
  const [sshCfg] = React.useState("hosts"); // ~/.ssh/config read result: "hosts" | "empty" | "unreadable"
  const timers = React.useRef({});
  const passkeyNames = passkeys.map((k) => k.name);
  const sshProfiles = profiles.filter((p) => isSshType(p.type));
  const sshProfileNames = sshProfiles.map((p) => p.name);
  // dynamic protocol set (known types first, then unknowns alpha) + filtered view
  const protocols = React.useMemo(() => {
    const seen = [];
    profiles.forEach((p) => { const t = (p.type || "").trim(); if (t && !seen.includes(t)) seen.push(t); });
    const known = KNOWN_TYPES.filter((t) => seen.includes(t));
    const extra = seen.filter((t) => !KNOWN_TYPES.includes(t)).sort();
    return [...known, ...extra];
  }, [profiles]);
  const shownProfiles = profiles.filter((p) => !hidden.has((p.type || "").trim()));

  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") onClose && onClose(); };
    window.addEventListener("keydown", h);
    return () => { window.removeEventListener("keydown", h); Object.values(timers.current).forEach(clearTimeout); };
  }, [onClose]);

  const switchTab = (t) => { setTab(t); setPView({ type: "list" }); setKView({ type: "list" }); setAView({ type: "list" }); setPErr(""); setKErr(""); setAErr(""); };

  // ── profile ops ──
  const pNew = () => { setPDraft(blankProfile()); setPErr(""); setPView({ type: "form", isNew: true }); };
  const pEdit = (p) => { setPDraft({ ...blankProfile(), ...p, fields: (p.fields || []).map((f) => ({ ...f })) }); setPErr(""); setPView({ type: "form", isNew: false }); };
  const pField = (k, v) => { setPDraft((d) => ({ ...d, [k]: v })); if (pErr) setPErr(""); };
  const pSetFields = (fields) => setPDraft((d) => ({ ...d, fields }));
  const pRedetect = (p) => {
    setProfiles((ps) => ps.map((x) => (x.id === p.id ? { ...x, state: "detecting" } : x)));
    onFlash && onFlash(`Re-detecting ${p.name}…`);
    clearTimeout(timers.current[p.id]);
    timers.current[p.id] = setTimeout(() => setProfiles((ps) => ps.map((x) => (x.id === p.id ? { ...x, state: "ok" } : x))), 1400);
  };
  const pSave = () => {
    const ssh = isSshType(pDraft.type);
    if (!pDraft.type.trim()) return setPErr("Type is required.");
    if (!pDraft.name.trim()) return setPErr("Name is required.");
    if (ssh) {
      if (!pDraft.host.trim()) return setPErr("Host is required.");
      if (pDraft.port && !(Number(pDraft.port) >= 1 && Number(pDraft.port) <= 65535)) return setPErr("Port must be between 1 and 65535.");
    }
    if (profiles.some((p) => p.name === pDraft.name.trim() && p.id !== pDraft.id)) return setPErr("A profile with this name already exists.");
    if (pDraft.id) {
      setProfiles((ps) => ps.map((x) => (x.id === pDraft.id ? { ...pDraft, type: pDraft.type.trim(), state: x.state === "failed" ? "ok" : x.state } : x)));
    } else {
      setProfiles((ps) => [...ps, { ...pDraft, type: pDraft.type.trim(), id: "p" + Date.now(), state: "ok" }]);
    }
    onFlash && onFlash(pDraft.id ? `Saved ${pDraft.name}` : `Added ${pDraft.name}`);
    setPView({ type: "list" });
  };
  const pDelete = (p) => { setProfiles((ps) => ps.filter((x) => x.id !== p.id)); onFlash && onFlash(`Deleted ${p.name}`); setPView({ type: "list" }); };

  // ── passkey ops ──
  const kNew = () => { setKDraft(blankPasskey()); setKErr(""); setKView({ type: "form", isNew: true }); };
  const kEdit = (k) => { setKDraft({ ...k }); setKErr(""); setKView({ type: "form", isNew: false }); };
  const kField = (key, v) => { setKDraft((d) => ({ ...d, [key]: v })); if (kErr) setKErr(""); };
  const kReveal = (k) => setRevealed((s) => { const n = new Set(s); n.has(k.id) ? n.delete(k.id) : n.add(k.id); return n; });
  const kSave = () => {
    if (!kDraft.name.trim()) return setKErr("Name is required.");
    if (!/^[A-Za-z0-9_-]+$/.test(kDraft.name.trim())) return setKErr("Name may contain only letters, numbers, - and _.");
    if (passkeys.some((k) => k.name === kDraft.name.trim() && k.id !== kDraft.id)) return setKErr("A passkey with this name already exists.");
    if (!kDraft.value.trim()) return setKErr("Value is required.");
    if (kDraft.id) setPasskeys((ks) => ks.map((x) => (x.id === kDraft.id ? { ...kDraft, name: kDraft.name.trim() } : x)));
    else setPasskeys((ks) => [...ks, { ...kDraft, name: kDraft.name.trim(), id: "k" + Date.now() }]);
    onFlash && onFlash(kDraft.id ? `Saved ${kDraft.name}` : `Added ${kDraft.name}`);
    setKView({ type: "list" });
  };
  const kDelete = (k) => { setPasskeys((ks) => ks.filter((x) => x.id !== k.id)); onFlash && onFlash(`Deleted ${k.name}`); setKView({ type: "list" }); };
  const kBrowse = () => { kField("value", "~/.ssh/id_ed25519"); onFlash && onFlash("Choose a key file…"); };

  // ── attach ops ──
  const aNew = () => { setADraft(blankAttach()); setAErr(""); setAView({ type: "form", isNew: true }); };
  const aEdit = (a) => { setADraft({ ...blankAttach(), ...a }); setAErr(""); setAView({ type: "form", isNew: false }); };
  const aField = (k, v) => { setADraft((d) => ({ ...d, [k]: v })); if (aErr) setAErr(""); };
  const aSave = () => {
    if (!aDraft.name.trim()) return setAErr("Name is required.");
    if (attaches.some((a) => a.name === aDraft.name.trim() && a.id !== aDraft.id)) return setAErr("An attach with this name already exists.");
    if (aDraft.mode === "ref") {
      if (!aDraft.sshRef) return setAErr("Choose an ssh profile to reference.");
    } else if (!aDraft.host.trim()) return setAErr("Host is required for a direct attach.");
    if (aDraft.port && !(Number(aDraft.port) >= 1 && Number(aDraft.port) <= 65535)) return setAErr("Port must be between 1 and 65535.");
    if (aDraft.id) setAttaches((as) => as.map((x) => (x.id === aDraft.id ? { ...aDraft, name: aDraft.name.trim(), state: x.state } : x)));
    else setAttaches((as) => [...as, { ...aDraft, name: aDraft.name.trim(), id: "a" + Date.now(), state: "ok" }]);
    onFlash && onFlash(aDraft.id ? `Saved ${aDraft.name}` : `Added ${aDraft.name}`);
    setAView({ type: "list" });
  };
  const aDelete = (a) => { setAttaches((as) => as.filter((x) => x.id !== a.id)); onFlash && onFlash(`Deleted ${a.name}`); setAView({ type: "list" }); };

  const TabBtn = ({ id, children }) => {
    const on = tab === id;
    return (
      <button onClick={() => switchTab(id)} style={{ border: 0, background: "transparent", cursor: "pointer",
        height: 35, padding: "0 var(--tasty-space-md)", fontFamily: "var(--tasty-font-ui)", fontSize: 13,
        color: on ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)", fontWeight: on ? 600 : 400,
        borderBottom: on ? "var(--tasty-focus-ring-width) solid var(--tasty-accent-primary)" : "var(--tasty-focus-ring-width) solid transparent" }}>{children}</button>
    );
  };

  return (
    <Scrim onClose={() => {}}>
      <div style={{ width: 520, height: 460, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
        border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden",
        boxShadow: "var(--tasty-shadow-modal)" }}>
        {/* common header */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md) var(--tasty-space-md) var(--tasty-space-md) var(--tasty-size-14)", flex: "none",
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}>
            <Icon name="remote" size={16} />
          </span>
          <span style={{ fontSize: 14, fontWeight: 600, color: "var(--tasty-text-primary)" }}>Remote connections</span>
          <div style={{ flex: 1 }} />
          <IconButton size="sm" aria-label="Close" title="Close" onClick={onClose}>
            <Icon name={RD.x} size={16} />
          </IconButton>
        </div>
        {/* tab bar */}
        <div style={{ display: "flex", alignItems: "center", gap: 2, padding: "0 var(--tasty-space-sm)", flex: "none",
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
          <TabBtn id="profiles">Remote profiles</TabBtn>
          <TabBtn id="attach">Attach</TabBtn>
          <TabBtn id="passkeys">Passkeys</TabBtn>
        </div>

        {/* ── tab content ── */}
        {tab === "profiles" ? (
          pView.type === "form" ? (
            <ProfileForm draft={pDraft} isNew={pView.isNew} error={pErr} passkeys={passkeys}
              onField={pField} onFields={pSetFields} onSave={pSave}
              onCancel={() => { setPErr(""); setPView({ type: "list" }); }} />
          ) : pView.type === "confirm" ? (
            <ConfirmDelete noun="remote profile" name={pView.profile.name}
              onConfirm={() => pDelete(pView.profile)} onCancel={() => setPView({ type: "list" })} />
          ) : (
            <ListShell addLabel="Add profile" onAdd={pNew} hasItems={shownProfiles.length > 0}
              rightSlot={protocols.length >= 2 ? <ProtocolFilter protocols={protocols} hidden={hidden} onApply={setHidden} /> : null}
              empty={profiles.length === 0
                ? 'No remote profiles yet. Click “Add profile” to create one.'
                : 'No profiles match the selected protocols. Adjust or reset the filter.'}>
              {shownProfiles.map((p) => (
                <ProfileRow key={p.id} p={p} passkeyNames={passkeyNames} onRedetect={pRedetect}
                  onEdit={pEdit} onDelete={(x) => setPView({ type: "confirm", profile: x })} />
              ))}
              <LocalSshSection state={sshCfg} profileTargets={profiles.filter((p) => isSshType(p.type)).map((p) => `${p.user}@${p.host}:${p.port}`)}
                onImport={(h) => onFlash && onFlash(`Added profile from ssh config: ${h.alias}`)} />
            </ListShell>
          )
        ) : tab === "attach" ? (
          aView.type === "form" ? (
            <AttachForm draft={aDraft} isNew={aView.isNew} error={aErr} sshProfiles={sshProfiles} passkeys={passkeys}
              onField={aField} onSave={aSave} onCancel={() => { setAErr(""); setAView({ type: "list" }); }} />
          ) : aView.type === "confirm" ? (
            <ConfirmDelete noun="attach" name={aView.attach.name}
              onConfirm={() => aDelete(aView.attach)} onCancel={() => setAView({ type: "list" })} />
          ) : (
            <ListShell addLabel="Add attach" onAdd={aNew} hasItems={attaches.length > 0}
              empty={'No attach targets yet. Click \u201CAdd attach\u201D to create one.'}>
              {attaches.map((a) => (
                <AttachRow key={a.id} a={a} sshProfileNames={sshProfileNames}
                  onEdit={aEdit} onDelete={(x) => setAView({ type: "confirm", attach: x })} />
              ))}
            </ListShell>
          )
        ) : (
          kView.type === "form" ? (
            <PasskeyForm draft={kDraft} isNew={kView.isNew} error={kErr} onField={kField}
              onSave={kSave} onBrowse={kBrowse} onCancel={() => { setKErr(""); setKView({ type: "list" }); }} />
          ) : kView.type === "confirm" ? (
            <ConfirmDelete noun="passkey" name={kView.passkey.name}
              hint={'Profiles referencing it will show “passkey missing”.'}
              onConfirm={() => kDelete(kView.passkey)} onCancel={() => setKView({ type: "list" })} />
          ) : (
            <ListShell addLabel="Add passkey" onAdd={kNew} hasItems={passkeys.length > 0}
              empty={'No passkeys yet. Click “Add passkey” to create one.'}>
              {passkeys.map((k) => (
                <PasskeyRow key={k.id} k={k} revealed={revealed.has(k.id)} onReveal={kReveal}
                  onEdit={kEdit} onDelete={(x) => setKView({ type: "confirm", passkey: x })} />
              ))}
            </ListShell>
          )
        )}
      </div>
    </Scrim>
  );
}

export { RemoteTool, LocalSshSection };
