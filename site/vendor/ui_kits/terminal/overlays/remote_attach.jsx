import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { CenterState } from "../../../components/feedback/CenterState.jsx";
import { StatusDot } from "../../../components/feedback/StatusDot.jsx";
import { ic, Icon } from "../chrome.jsx";
import { Scrim, Spinner } from "./shared.jsx";

// Tasty UI kit — Add remote workspace popup.
// NEW surface. Lets the user browse a remote tasty instance's workspaces and attach
// (mirror) one locally, instead of having to know the remote workspace id up front.
// Entry point (NOT designed here): sidebar workspaces-header / new-workspace(+) right
// click → context menu "Add remote workspace" → this popup.
//
// Two-pane picker + footer action bar, in the SAME visual language as remote_tool
// (Scrim · headless content-drawn header · --tasty-bg-panel frame · ghost/primary
// footer). It CONSUMES the tasty-attach profiles registered on remote_tool's Attach
// tab (~/.tasty/remote-profiles.toml, kind "tasty-attach") — it does not edit them.
//   Left  : the attach-profile list (single select).
//   Right : the selected profile's remote workspaces, across 4 states —
//           initial (nothing picked) · connecting · error (retry) · loaded (list).
//           The loaded list's FIRST row is "+ New workspace" (RaNewWsRow) — create one
//           on the remote with its defaults and mirror that, so an empty remote is never
//           a dead end. Select-then-confirm like any other row; footer says which.
//   Footer: Connect (enabled only when a selectable remote workspace is chosen) ·
//           Cancel (always enabled — closes, aborting any in-flight connect).



// ── icons (lucide-style, drawn through TastyKit.Icon) ──
// glyph names from the canonical set (icons/*.svg via <Icon name>)
const RA_IC = {
  remote: "remote",
  x: "close",
  warn: "alertTriangle",
  refresh: "refresh",
  panes: "split",
  empty: "paneEmpty",
  plus: "plus",
};

// sentinel ws id for the "+ New workspace" row — it is a LIST ROW, so it flows
// through the same single-select state as every real remote ws (§6-5).
const RA_NEW_WS = "__ra_new_ws__";
// demo: creating on this profile is rejected by the remote (§6-4 failure state)
const RA_CREATE_FAILS = "t3";
const RA_CREATE_ERROR = "Remote refused: workspace quota reached (8/8) on edge.example.com. Close a workspace there, or raise `limits.workspaces` in the remote's tasty.toml, then try again.";

// tasty-attach profiles (kind "tasty-attach") — mirrors the Attach tab of remote_tool.
// `state:"inactive"` = last detection failed → connecting to it errors (demo).
const RA_ATTACHES = [
  { id: "t1", name: "prod-web", label: "us-east", target: "deploy@10.0.4.12", via: "ref", state: "ok" },
  { id: "t2", name: "gb10", label: "", target: "→ prod-web", via: "ref", state: "ok" },
  { id: "t3", name: "edge-direct", label: "", target: "root@edge.example.com", via: "inline", state: "ok" },
  { id: "t4", name: "media-nas", label: "lab", target: "→ nas.local", via: "ref", state: "ok" },
  { id: "t5", name: "legacy-attach", label: "", target: "→ legacy-box", via: "ref", state: "inactive" },
];

// remote workspaces per attach profile. null = the connect fails (error state);
// [] = connects but the remote has no workspaces (loaded-empty). `attached` = a
// remote workspace already claimed by ANOTHER client → not selectable here.
const RA_WORKSPACES = {
  t1: [
    { id: "w1", name: "agents-prod", panes: 3, busy: true },
    { id: "w2", name: "api-gateway", panes: 2, busy: false, attached: true },
    { id: "w3", name: "scratch", panes: 1, busy: false },
  ],
  t2: [
    { id: "w4", name: "etl-nightly", panes: 4, busy: true },
    { id: "w5", name: "warehouse", panes: 2, busy: false },
    { id: "w6", name: "sandbox", panes: 1, busy: false, attached: true },
  ],
  t3: [
    { id: "w7", name: "edge-cache", panes: 1, busy: false },
  ],
  t4: [],
  t5: null,
};
const RA_ERRORS = {
  t5: "SSH authentication failed — passkey “old-rsa” was rejected by legacy-box.",
};

// shared text styles (uniquely named — concatenated into the app bundle alongside
// remote_tool.jsx, so NO generic identifiers here)
const raCaptionCaps = { fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase",
  letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" };
const raMono = { fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" };
const raCaption = { fontSize: 11, color: "var(--tasty-text-muted)" };

// ── lavender "in use" pill — a remote ws already claimed by ANOTHER client.
//    Uses accent-attached (the same axis as the sidebar attached ring), NOT danger. ──
function RaInUseBadge() {
  return (
    <Tag variant="attached" title="Already attached on another client — can't mirror it here."
      style={{ color: "var(--tasty-tag-attached-fg)", borderColor: "color-mix(in srgb, var(--tasty-accent-attached) 40%, transparent)" }}>in use</Tag>
  );
}

function RaInactiveBadge() {
  return (
    <Tag variant="warning" title="Inactive — last detection failed." style={{ color: "var(--tasty-tag-warning-fg)" }}>
      <span style={{ display: "inline-flex" }}><Icon name={RA_IC.warn} size={12} /></span>inactive
    </Tag>
  );
}

// ── LEFT — a single attach-profile row (single select) ──
function RaAttachProfileRow({ a, selected, onSelect }) {
  const [hover, setHover] = React.useState(false);
  const inactive = a.state === "inactive";
  return (
    <div role="option" aria-selected={selected} onClick={() => onSelect(a)}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ display: "flex", flexDirection: "column", gap: 2, cursor: "pointer", position: "relative",
        padding: "var(--tasty-space-sm) var(--tasty-space-md)",
        background: selected ? "var(--tasty-surface-active)" : hover ? "var(--tasty-overlay-hover)" : "transparent",
        boxShadow: selected ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", minWidth: 0 }}>
        <span style={{ fontSize: 13, fontWeight: 600, lineHeight: "var(--tasty-line-height-ui)", flex: "0 1 auto",
          color: selected ? "var(--tasty-text-on-active)" : "var(--tasty-text-secondary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {a.name}{a.label && <span style={{ fontWeight: 400, color: "var(--tasty-text-muted)" }}>  ({a.label})</span>}
        </span>
        {inactive && <RaInactiveBadge />}
      </div>
      <div style={{ ...raMono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{a.target}</div>
    </div>
  );
}

// ── RIGHT — "+ New workspace", the FIRST row of the loaded list ──
// Not a button, not a separate tab: a row in the same list, so it obeys the list's
// select-then-confirm contract (§6-5). Distinguished from real remote workspaces on
// THREE channels, never colour alone (§6-2): the `plus` glyph in the status-dot slot,
// accent text, and a 1px separator below it that closes the group.
// Same 34px box as RaRemoteWsRow — the glyph sits in a dot-width slot and overflows
// symmetrically, so the name column's left edge is pixel-identical to the rows below.
function RaNewWsRow({ selected, onSelect, phase = "rest", error, onRetry }) {
  const [hover, setHover] = React.useState(false);
  const creating = phase === "creating";
  const failed = phase === "failed";
  return (
    <div>
      <div role="option" aria-selected={selected} aria-busy={creating || undefined}
        title="Creates a workspace on the remote with its default name and cwd — you won't be asked for a name — then mirrors it here."
        onClick={() => !creating && onSelect()}
        onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", position: "relative",
          padding: "var(--tasty-space-sm) var(--tasty-space-md)", cursor: creating ? "default" : "pointer",
          background: selected ? "var(--tasty-surface-active)" : (!creating && hover) ? "var(--tasty-overlay-hover)" : "transparent",
          boxShadow: selected ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none" }}>
        {/* glyph occupies the status-dot slot → shared left alignment line */}
        <span style={{ flex: "none", width: "var(--tasty-status-dot-size)", display: "inline-flex", alignItems: "center", justifyContent: "center",
          color: creating ? "var(--tasty-text-muted)" : failed ? "var(--tasty-accent-danger)" : "var(--tasty-accent-primary)" }}>
          {creating ? <Spinner size={14} /> : <Icon name={failed ? RA_IC.warn : RA_IC.plus} size={14} />}
        </span>
        <span style={{ flex: "0 1 auto", minWidth: 0, fontSize: 13, fontWeight: 500, lineHeight: "var(--tasty-line-height-ui)",
          color: creating ? "var(--tasty-text-muted)" : selected ? "var(--tasty-text-primary)" : "var(--tasty-accent-primary)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {creating ? "Creating workspace…" : "New workspace"}
        </span>
        <div style={{ flex: 1 }} />
        {!creating && !failed && <span style={raCaption}>on remote</span>}
      </div>
      {failed && (
        <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)",
          padding: "var(--tasty-space-xs) var(--tasty-space-md) var(--tasty-space-sm)" }}>
          <span style={{ flex: "none", width: "var(--tasty-status-dot-size)" }} />
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", alignItems: "flex-start", gap: "var(--tasty-space-xs)" }}>
            <span title={error} style={{ ...raCaption, color: "var(--tasty-accent-danger-raised)", lineHeight: "var(--tasty-line-height-ui)",
              display: "-webkit-box", WebkitLineClamp: 3, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{error}</span>
            <Button variant="secondary" size="sm" leadingIcon={<Icon name={RA_IC.refresh} size={14} />} onClick={onRetry}>Try again</Button>
          </div>
        </div>
      )}
      <div style={{ height: "var(--tasty-border-width)", margin: "var(--tasty-space-xs) 0", background: "var(--tasty-separator)" }} />
    </div>
  );
}

// ── RIGHT — a single remote-workspace row (single select; attached = disabled) ──
function RaRemoteWsRow({ w, selected, onSelect }) {
  const [hover, setHover] = React.useState(false);
  const disabled = !!w.attached;
  return (
    <div role="option" aria-selected={selected} aria-disabled={disabled}
      onClick={() => !disabled && onSelect(w)}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", position: "relative",
        padding: "var(--tasty-space-sm) var(--tasty-space-md)", cursor: disabled ? "default" : "pointer",
        background: selected ? "var(--tasty-surface-active)" : (!disabled && hover) ? "var(--tasty-overlay-hover)" : "transparent",
        boxShadow: selected ? "inset var(--tasty-listctrl-selected-bar-width) 0 0 var(--tasty-listctrl-selected-bar)" : "none" }}>
      {/* execution dot — busy=running(green), else idle */}
      <span style={{ flex: "none", display: "inline-flex", alignItems: "center" }}>
        <StatusDot status={w.busy ? "running" : "idle"} pulse={w.busy} />
      </span>
      <span style={{ flex: "0 1 auto", minWidth: 0, fontSize: 13, fontWeight: 500, lineHeight: "var(--tasty-line-height-ui)",
        color: disabled ? "var(--tasty-text-muted)" : selected ? "var(--tasty-text-primary)" : "var(--tasty-text-secondary)",
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{w.name}</span>
      <span style={{ flex: "none", display: "inline-flex", alignItems: "center", gap: 4, ...raCaption }}>
        <Icon name={RA_IC.panes} size={12} />{w.panes}
      </span>
      <div style={{ flex: 1 }} />
      {w.busy && !disabled && <span style={raCaption}>busy</span>}
      {disabled && <RaInUseBadge />}
    </div>
  );
}

// ── RIGHT-pane centered state (initial / connecting / error / empty) ──
function RaCenterState({ children }) {
  return (
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
      textAlign: "center", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xl) var(--tasty-space-lg)" }}>{children}</div>
  );
}

// ════════════════════════════════════════════════════════════════════════
function RemoteAttach({ onClose, onFlash }) {
  const [attachSel, setAttachSel] = React.useState(null);   // selected attach profile id
  const [conn, setConn] = React.useState("initial");         // initial | connecting | error | loaded
  const [wsSel, setWsSel] = React.useState(null);            // selected remote ws id (or RA_NEW_WS)
  const [phase, setPhase] = React.useState("rest");          // rest | creating | failed  (new-ws row)
  const timer = React.useRef(null);
  const createTimer = React.useRef(null);

  React.useEffect(() => {
    const h = (e) => { if (e.key === "Escape") onClose && onClose(); };
    window.addEventListener("keydown", h);
    return () => { window.removeEventListener("keydown", h); clearTimeout(timer.current); clearTimeout(createTimer.current); };
  }, [onClose]);

  const connect = (a) => {
    clearTimeout(timer.current);
    clearTimeout(createTimer.current); setPhase("rest");
    setAttachSel(a.id); setWsSel(null); setConn("connecting");
    timer.current = setTimeout(() => {
      setConn(RA_WORKSPACES[a.id] == null ? "error" : "loaded");
    }, 1300);
  };
  const retry = () => { const a = RA_ATTACHES.find((x) => x.id === attachSel); if (a) connect(a); };

  const selAttach = RA_ATTACHES.find((a) => a.id === attachSel) || null;
  const wsList = attachSel ? RA_WORKSPACES[attachSel] : undefined;
  const newSel = wsSel === RA_NEW_WS;
  const canConnect = conn === "loaded" && !!wsSel && phase !== "creating";
  const createWs = () => {
    setPhase("creating");
    createTimer.current = setTimeout(() => {
      if (attachSel === RA_CREATE_FAILS) { setPhase("failed"); return; }
      onFlash && onFlash(`Created a workspace on ${selAttach.name} — attaching…`);
      onClose && onClose();
    }, 1600);
  };
  const doConnect = () => {
    if (newSel) { createWs(); return; }
    const w = (wsList || []).find((x) => x.id === wsSel);
    if (!w) return;
    onFlash && onFlash(`Attaching “${w.name}” from ${selAttach.name}…`);
    onClose && onClose();
  };
  const raNewRow = (
    <RaNewWsRow selected={newSel} phase={phase} error={RA_CREATE_ERROR}
      onSelect={() => { setWsSel(RA_NEW_WS); if (phase === "failed") setPhase("rest"); }}
      onRetry={createWs} />
  );

  return (
    <Scrim onClose={() => {}}>
      <div style={{ width: 680, height: 460, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)",
        border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", overflow: "hidden",
        boxShadow: "var(--tasty-shadow-modal)" }}>

        {/* header (headless / content-drawn — matches remote_tool) */}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flex: "none",
          padding: "var(--tasty-space-md) var(--tasty-space-md) var(--tasty-space-md) var(--tasty-size-14)",
          borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <span style={{ display: "inline-flex", color: "var(--tasty-text-muted)" }}><Icon name={RA_IC.remote} size={16} /></span>
          <span style={{ fontSize: 14, fontWeight: 600, color: "var(--tasty-text-primary)" }}>Add remote workspace</span>
          <div style={{ flex: 1 }} />
          <IconButton size="sm" aria-label="Close" title="Close" onClick={onClose}><Icon name={RA_IC.x} size={16} /></IconButton>
        </div>

        {/* body — two panes */}
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>

          {/* LEFT — attach profiles (240px) */}
          <div style={{ width: 240, flex: "none", minHeight: 0, display: "flex", flexDirection: "column",
            borderRight: "var(--tasty-border-width) solid var(--tasty-separator)", background: "var(--tasty-bg-sidebar)" }}>
            <div style={{ ...raCaptionCaps, padding: "var(--tasty-space-md) var(--tasty-space-md) var(--tasty-space-xs)" }}>Attach profiles</div>
            <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", paddingBottom: "var(--tasty-space-sm)" }}>
              {RA_ATTACHES.map((a) => (
                <RaAttachProfileRow key={a.id} a={a} selected={a.id === attachSel} onSelect={connect} />
              ))}
            </div>
          </div>

          {/* RIGHT — remote workspaces (4 states) */}
          <div style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", flexDirection: "column" }}>
            {/* centred states render the DS CenterState part (2026-09-29): part owns sizes, colours,
                type and the error glyph (alertTriangle); the Retry action hangs below, outside the centring. */}
            {conn === "initial" && (
              <CenterState variant="empty" glyph={RA_IC.remote} title="Select an attach profile"
                sub="Pick a profile on the left to connect and list the remote instance's workspaces." />
            )}

            {conn === "connecting" && (
              <CenterState variant="loading" title="Connecting…"
                sub={<>Establishing the SSH tunnel to <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{selAttach && selAttach.name}</span> and listing workspaces. This can take a few seconds.</>} />
            )}

            {conn === "error" && (
              <CenterState variant="error" title="Can't connect"
                sub={(selAttach && RA_ERRORS[selAttach.id]) || "The remote instance didn't respond."}
                action={<Button variant="secondary" size="sm" leadingIcon={<Icon name={RA_IC.refresh} size="var(--tasty-icon-size-sm)" />} onClick={retry}>Retry</Button>} />
            )}

            {/* loaded — ONE render path whether or not the remote has workspaces (§6-1, plan B):
                the caps header + "+ New workspace" row always render, so the pane is never a
                dead end; an empty remote just adds one muted line under the row. */}
            {conn === "loaded" && Array.isArray(wsList) && (
              <>
                <div style={{ ...raCaptionCaps, padding: "var(--tasty-space-md) var(--tasty-space-md) var(--tasty-space-xs)", display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
                  <span>Remote workspaces</span>
                  <span data-ink-grade="none" data-ink-why="ornament" style={{ color: "var(--tasty-text-disabled)" }}>·</span>
                  <span style={{ textTransform: "none", letterSpacing: 0, fontFamily: "var(--tasty-font-ui)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{selAttach && selAttach.name}</span>
                </div>
                <div className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", paddingBottom: "var(--tasty-space-sm)" }}>
                  {raNewRow}
                  {wsList.length > 0 ? wsList.map((w) => (
                    <RaRemoteWsRow key={w.id} w={w} selected={w.id === wsSel} onSelect={(x) => setWsSel(x.id)} />
                  )) : (
                    <div style={{ display: "flex", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-xs) var(--tasty-space-md)" }}>
                      <span style={{ flex: "none", width: "var(--tasty-status-dot-size)" }} />
                      <span style={{ ...raCaption, flex: 1, minWidth: 0, lineHeight: "var(--tasty-line-height-ui)" }}>
                        <span style={{ fontFamily: "var(--tasty-font-mono)" }}>{selAttach && selAttach.name}</span> is reachable but has no workspaces yet.
                      </span>
                    </div>
                  )}
                </div>
              </>
            )}
          </div>
        </div>

        {/* footer — Connect (conditional) · Cancel (always) */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", flex: "none",
          padding: "var(--tasty-space-md) var(--tasty-space-lg)", borderTop: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!canConnect} onClick={doConnect}>{newSel ? "Create & connect" : "Connect"}</Button>
        </div>
      </div>
    </Scrim>
  );
}

export { RemoteAttach };
