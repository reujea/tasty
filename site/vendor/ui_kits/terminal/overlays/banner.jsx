import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { Tag } from "../../../components/core/Tag.jsx";
import { MenuItem } from "../../../components/navigation/MenuItem.jsx";
import { Icon } from "../chrome.jsx";

// Tasty UI kit — Banner: the 4th overlay family (Modal / Popup / Toast / Banner).
// A floating, top-anchored notice pinned to the top of a scope's content area
// (View / Workspace / Pane / Tab / Surface) — NEVER over the tab bar. Focus-less,
// consumes its own mouse, and unlike a Toast it can carry actions. Lifetime rule (2026-11-02):
// a banner with NO user action has a TTL and auto-dismisses; a banner WITH an action
// (button or ⋯ menu row) has no TTL. × is not an action. TTL shows a top-right countdown that flips to an X on hover (and pauses while hovered
// or backgrounded). Mouse-capture always has its ⋯ (2026-11-03 — also with an unknown program name, where
// the menu shows its rows disabled), so it has no TTL under the normal rule; no exception. One banner shown per scope; the rest wait in a queue (max 5).
// Mirrors the proposed zilhak/tasty Banner manager. Standalone preview: banner.html



const bic = {
  mouse: <Icon name="mouse" size={16} />,
  check: <Icon name="check" size={16} />,
  warn: <Icon name="alertTriangle" size={16} />,
  x: <Icon name="close" size={14} />,
  more: <Icon name="more" size={14} />,
  bell: <Icon name="bell" />,
};

// ── the shared shell (frame only — every banner composes this) ──────────
function BannerShell({ recessed, zIndex, children }) {
  return (
    <div style={{ width: "100%", position: "relative", zIndex,
      background: "var(--tasty-banner-bg)", color: "var(--tasty-banner-fg)",
      border: "var(--tasty-border-width) solid var(--tasty-banner-border)",
      borderRadius: "var(--tasty-banner-radius)", boxShadow: "var(--tasty-banner-shadow)",
      opacity: recessed ? "var(--tasty-banner-recessed-opacity)" : 1,
      transition: "opacity var(--tasty-banner-fade) var(--tasty-ease-ui)" }}>
      {children}
    </div>
  );
}

// top-right affordance — TTL countdown number ↔ X. The X reveals on banner
// hover for BOTH kinds; a TTL banner shows the seconds when not hovered.
function BannerDismiss({ ttl, bannerHover, backgrounded, onClose }) {
  const [n, setN] = React.useState(ttl || 0);
  React.useEffect(() => { setN(ttl || 0); }, [ttl]);
  const frozen = bannerHover || backgrounded;        // TTL stop conditions
  React.useEffect(() => {
    if (!ttl || frozen) return undefined;
    if (n <= 0) { const id = setTimeout(() => onClose && onClose(), 0); return () => clearTimeout(id); }
    const id = setTimeout(() => setN((x) => x - 1), 1000);
    return () => clearTimeout(id);
  }, [ttl, frozen, n]);
  return (
    <span style={{ width: "var(--tasty-size-24)", height: "var(--tasty-size-24)", flex: "none",
      display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
      {bannerHover
        ? <IconButton size="sm" aria-label="Dismiss" onClick={onClose}>{bic.x}</IconButton>
        : ttl
          ? <span style={{ fontFamily: "var(--tasty-banner-countdown-font)", fontSize: "var(--tasty-banner-countdown-font-size)",
              color: "var(--tasty-banner-countdown-fg)", fontVariantNumeric: "tabular-nums" }}>{n}</span>
          : null}
    </span>
  );
}

// ── the "more" (⋯) overflow menu — per-app opt-outs behind one trigger ──
// Lives in the affordance column, LEFT of the ×, same hover reveal rule; stays
// lit while open. Rows are permanent Settings writes, so the copy says off /
// disable (never "pause"). The label's fixed text never truncates; the
// interpolated program name (mono) shrinks and ellipsises, so the menu stays
// between --tasty-banner-more-menu-min-width and -max-width in every locale.
// 2026-11-02 — prefix / {app} / suffix (en: prefix only; ko/ja: suffix only).
function MoreLabel({ text, app, after }) {
  return (
    <span style={{ display: "flex", minWidth: 0, whiteSpace: "nowrap" }}>
      {text && <span style={{ flex: "none" }}>{text}</span>}
      <span title={app} style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis",
        fontFamily: "var(--tasty-banner-more-app-font)", color: "var(--tasty-banner-more-app-fg)" }}>{app}</span>
      {after && <span style={{ flex: "none" }}>{after}</span>}
    </span>
  );
}

function BannerMore({ app, open, setOpen, onAction }) {
  const wrap = React.useRef(null);
  React.useEffect(() => {
    if (!open) return undefined;
    const onDown = (e) => { if (wrap.current && !wrap.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onDown); document.removeEventListener("keydown", onKey); };
  }, [open]);
  const act = (which) => { setOpen(false); onAction && onAction(which); };
  // 2026-11-03 — name unknown (remote mirror, before first poll, lookup failed): the actions exist but can't run.
  // Same two rows, disabled (no hover, no click, menu stays open, nothing saved, keyboard skips them),
  // worded without a name, under a non-interactive reason line. Not updated live if the name arrives while open.
  const known = !!app;
  return (
    <span ref={wrap} style={{ position: "relative", width: "var(--tasty-size-24)", height: "var(--tasty-size-24)", flex: "none",
      display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
      <IconButton size="sm" aria-label="More options" aria-expanded={open} active={open}
        onClick={() => setOpen(!open)}>{bic.more}</IconButton>
      {open && (
        <div role="menu" aria-label="Banner options" style={{ position: "absolute",
          top: "calc(100% + var(--tasty-banner-more-menu-offset))", right: 0, zIndex: 3,
          minWidth: "var(--tasty-banner-more-menu-min-width)", maxWidth: "var(--tasty-banner-more-menu-max-width)",
          width: known ? "max-content" : "var(--tasty-banner-more-menu-max-width)", boxSizing: "border-box",
          background: "var(--tasty-banner-more-menu-bg)",
          border: "var(--tasty-border-width) solid var(--tasty-banner-more-menu-border)",
          borderRadius: "var(--tasty-banner-more-menu-radius)", padding: "var(--tasty-banner-more-menu-padding)",
          boxShadow: "var(--tasty-banner-more-menu-shadow)" }}>
          {known ? (<>
            <MenuItem icon={bic.bell} onClick={() => act("notice")}
              label={<MoreLabel text="Don't show again for " app={app} />} />
            <MenuItem icon={<Icon name="mouse" />} onClick={() => act("capture")}
              label={<MoreLabel text="Let Tasty handle clicks in " app={app} />} />
          </>) : (<>
            <div role="note" style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)",
              padding: "var(--tasty-space-xs) var(--tasty-space-md)", color: "var(--tasty-banner-more-reason-fg)",
              fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)" }}>
              <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-banner-more-reason-glyph-offset)" }}><Icon name="info" size={14} /></span>
              <span style={{ minWidth: 0 }}>Can't tell which program is running, so nothing can be saved.</span>
            </div>
            <MenuItem separator />
            <MenuItem disabled wrap aria-disabled="true" icon={bic.bell} label="Don't show again for this program" />
            <MenuItem disabled wrap aria-disabled="true" icon={<Icon name="mouse" />} label="Let Tasty handle clicks in this program" />
          </>)}
        </div>
      )}
    </span>
  );
}

function Banner({ data, recessed, zIndex, backgrounded, onClose }) {
  const { glyph, glyphColor, title, body, actions, ttl, app } = data;
  const more = !!data.more; // 2026-11-03 — drawn whether or not the program name is known (unknown → disabled menu form)
  const [hover, setHover] = React.useState(false);
  const [menuOpen, setMenuOpen] = React.useState(false);
  const onAction = (which) => { if (which === "notice") onClose && onClose(); };
  return (
    <BannerShell recessed={recessed} zIndex={zIndex}>
      <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
        style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-banner-gap)",
          padding: "var(--tasty-banner-padding-y) var(--tasty-banner-padding-x)" }}>
        <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-size-1)",
          color: glyphColor || "var(--tasty-banner-icon-fg)" }}>{glyph}</span>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: "var(--tasty-size-2)",
          paddingRight: "var(--tasty-banner-more-reserve)" }}>
          <div style={{ fontSize: "var(--tasty-banner-title-font-size)", fontWeight: "var(--tasty-font-weight-semibold)",
            color: "var(--tasty-text-primary)", lineHeight: "var(--tasty-line-height-ui)" }}>{title}</div>
          {body && <div style={{ fontSize: "var(--tasty-banner-body-font-size)", color: "var(--tasty-banner-body-fg)",
            lineHeight: "var(--tasty-line-height-ui)" }}>{body}</div>}
          {actions && <div style={{ display: "flex", gap: "var(--tasty-space-sm)", marginTop: "var(--tasty-space-xs)" }}>{actions}</div>}
        </div>
        {/* affordance column — the ⋯ trigger (when the banner has one) then the dismiss slot */}
        <span style={{ position: "absolute", top: "var(--tasty-banner-padding-y)", right: "var(--tasty-space-sm)",
          display: "flex", alignItems: "center", gap: "var(--tasty-banner-more-column-gap)",
          opacity: more && !(hover || menuOpen) ? 0 : 1,
          transition: "opacity var(--tasty-banner-fade) var(--tasty-ease-ui)" }}>
          {more && <BannerMore app={app} open={menuOpen} setOpen={setMenuOpen} onAction={onAction} />}
          <BannerDismiss ttl={ttl} bannerHover={hover || menuOpen} backgrounded={backgrounded} onClose={onClose} />
        </span>
      </div>
    </BannerShell>
  );
}

// ── sample banners ──────────────────────────────────────────────────────
// Built lazily (at render) — NOT at module load. These compose DS components
// (Kbd / Button), and inside the compiled bundle this file evaluates before
// the DS namespace finishes populating, so a module-level `const SAMPLES`
// would create elements from still-undefined components. makeSamples() runs
// from BannerHost's render, by which point the namespace is ready.
function makeSamples() {
  return {
  mouse: {
    id: "mouse-capture", glyph: bic.mouse, glyphColor: "var(--tasty-text-secondary)", more: true, app: "vim",
    title: "Mouse reporting captured your drag",
    body: (<><b style={{ color: "var(--tasty-text-secondary-raised)" }}>vim</b> has mouse tracking on, so drag-to-select is disabled. Hold <Kbd keys="Shift" /> to select anyway.</>),
  },
  mouseUnknown: {
    id: "mouse-capture", glyph: bic.mouse, glyphColor: "var(--tasty-text-secondary)", more: true, app: null,
    title: "Mouse reporting captured your drag",
    body: (<>The running program has mouse tracking on, so drag-to-select is disabled. Hold <Kbd keys="Shift" /> to select anyway.</>),
  },
  preset: {
    id: "preset-applied", glyph: bic.check, glyphColor: "var(--tasty-accent-success)", ttl: 6,
    title: (<>Preset <b style={{ color: "var(--tasty-text-primary)" }}>Dev split</b> applied to this tab.</>),
  },
  shell: {
    id: "shell-integration-missing", glyph: bic.warn, glyphColor: "var(--tasty-accent-warning)", ttl: 10,
    title: "Shell integration not detected",
    body: "Command-completion hooks (docker build, just run, etc.) need OSC 133 shell integration. See docs/features/hooks for setup.",
  },
  err: {
    id: "write-failed", glyph: bic.warn, glyphColor: "var(--tasty-accent-danger)",
    title: "Couldn’t write ~/.tastyrc",
    body: "Permission denied. Check the file owner, or open it to resolve the conflict.",
    actions: (<><Button variant="secondary" size="sm">Retry</Button><Button variant="ghost" ground="raised" size="sm">Open file</Button></>),
  },
  };
}

// ── interactive host — one shown per scope, the rest queue (max 5) ──────
function BannerHost() {
  const SAMPLES = React.useMemo(makeSamples, []);
  const [stack, setStack] = React.useState(() => [SAMPLES.mouse]); // [0] = shown
  const [bg, setBg] = React.useState(false);                 // scope backgrounded → TTL pauses

  const spawn = (key) => setStack((s) => {
    const sample = { ...SAMPLES[key] };
    if (s[0] && s[0].id === sample.id) return [{ ...sample, _k: Math.random() }, ...s.slice(1)]; // same id shown → reset
    if (s.some((b) => b.id === sample.id)) return s;          // dup already queued → ignore
    if (s.length >= 6) return s;                              // 1 shown + 5 queued = full → ignore
    return [...s, sample];
  });
  const close = () => setStack((s) => s.slice(1));

  const shown = stack[0];
  const queued = Math.max(0, stack.length - 1);
  const Ctrl = ({ children, onClick, on }) => (
    <Button variant={on ? "secondary" : "ghost"} size="sm" onClick={onClick}>{children}</Button>
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: "100%", maxWidth: 760 }}>
      {/* demo control bar — not part of the component */}
      <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)", flexWrap: "wrap",
        padding: "var(--tasty-space-sm)", border: "var(--tasty-border-width) solid var(--tasty-separator)",
        borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-sidebar)" }}>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-micro)", textTransform: "uppercase",
          letterSpacing: "var(--tasty-letter-spacing-caps)", color: "var(--tasty-text-muted)" }}>spawn</span>
        <Ctrl onClick={() => spawn("mouse")}>Mouse capture</Ctrl>
        <Ctrl onClick={() => spawn("mouseUnknown")}>Mouse capture (name unknown)</Ctrl>
        <Ctrl onClick={() => spawn("preset")}>Preset (TTL 6s)</Ctrl>
        <Ctrl onClick={() => spawn("shell")}>Shell integration (TTL 10s)</Ctrl>
        <Ctrl onClick={() => spawn("err")}>Write error</Ctrl>
        <span style={{ width: "var(--tasty-border-width)", alignSelf: "stretch", background: "var(--tasty-separator)", margin: "0 var(--tasty-space-xs)" }} />
        <Ctrl on={bg} onClick={() => setBg((b) => !b)}>{bg ? "Backgrounded ✓" : "Background scope"}</Ctrl>
        <span style={{ flex: 1 }} />
        {queued > 0 && <Tag variant="accent">+{queued} queued</Tag>}
      </div>

      {/* faux scope: tab bar (must stay clear) + terminal content + the banner zone */}
      <div style={{ position: "relative", width: "100%", height: 320, borderRadius: "var(--tasty-radius)", overflow: "hidden",
        border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-color-black)",
        opacity: bg ? 0.55 : 1, transition: "opacity var(--tasty-banner-fade) var(--tasty-ease-ui)" }}>
        <div style={{ display: "flex", alignItems: "stretch", height: 28, flex: "none",
          background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid var(--tasty-separator)" }}>
          {["server", "dev", "vim"].map((t, i) => (
            <div key={t} style={{ display: "flex", alignItems: "center", padding: "0 var(--tasty-space-md)", fontSize: "var(--tasty-font-size-body)",
              borderRight: "var(--tasty-border-width) solid var(--tasty-separator)",
              background: i === 2 ? "var(--tasty-bg-panel)" : "transparent",
              color: i === 2 ? "var(--tasty-text-primary)" : "var(--tasty-text-muted)",
              boxShadow: i === 2 ? "inset 0 -2px 0 var(--tasty-accent-primary)" : "none" }}>{t}</div>
          ))}
        </div>
        <div style={{ padding: "var(--tasty-space-md) var(--tasty-size-14)", fontFamily: "var(--tasty-font-mono)",
          fontSize: "var(--tasty-font-size-term-sm)", lineHeight: 1.6, color: "var(--tasty-color-neutral-1100)", opacity: 0.4 }}>
          <div><span data-audit-exempt="ansi-content" style={{ color: "var(--tasty-color-green)" }}>~/tasty</span> $ vim src/main.rs</div>
          <div>-- VISUAL --   1,1   Top</div>
        </div>
        {/* banner zone — 8px below the tab bar, 8px side margins, no bottom margin */}
        <div style={{ position: "absolute", top: "calc(28px + var(--tasty-banner-margin))",
          left: "var(--tasty-banner-margin)", right: "var(--tasty-banner-margin)" }}>
          {shown && <Banner key={shown.id + (shown._k || "")} data={shown} backgrounded={bg} onClose={close} />}
          {!shown && (
            <div style={{ textAlign: "center", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", paddingTop: "var(--tasty-space-md)" }}>
              No banner — spawn one above.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export { Banner, BannerShell, BannerHost };
