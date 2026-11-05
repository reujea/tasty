import React from "react";
import { Button } from "../../../components/core/Button.jsx";
import { Icon } from "../../../components/core/Icon.jsx";
import { IconButton } from "../../../components/core/IconButton.jsx";
import { Kbd } from "../../../components/core/Kbd.jsx";
import { MenuItem } from "../../../components/navigation/MenuItem.jsx";

// Info modal SHELL — the boot / runtime notice queue (one message at a time).
// Repo: src/adapters/ui/info_modal.rs (shell) + popup/defs.rs (title in the popup title bar).
// Designed 2026-09-28.
//
// Shell rules (apply to EVERY queued message, not just the permissions notice):
//   frame   440 wide × 140..360 tall; body scrolls, title bar + button row never do
//   title   the SHARED popup title bar (popup/defs.rs): bg-sidebar strip, control-height 28,
//           title centred (14/600, one line, ellipsis — never wraps), close × on the right,
//           1px info-modal-title-edge rule below, always drawn. × = the dismiss action
//           (same as Enter / Esc and the Primary button); outside click never closes.
//   body    13 · text-secondary · paragraph gap 12; plain text unless the message
//           itself authors emphasis (only the permissions notice does)
//   edge    1px rule above the button row ONLY while content is hidden below
//           (the title bar rule already bounds the top)
//   buttons DS Button widget, right-aligned; the dismiss button is Primary, rightmost
//   keys    Enter / Esc = the dismiss button
//
// Title bar buttons (2026-09-29): the title is centred
// on the STRIP, whatever sits on the right. Both sides reserve
//   reserve(N) = popup-title-edge-inset + N × popup-title-btn-size + (N − 1) × popup-title-btn-gap + popup-title-text-gap
//   N = 1 (×): 32 · N = 2 (fullscreen + ×): 60.
// Narrow popups (2026-09-29):
//   · every popup width is a token and scales with ui_scale like every other size. Scaling alone
//     keeps the title/band ratio, so a width must be sized for its longest locale title at 1.0:
//     convert popup 200 → convert-popup-width 240 (band 176; ja title ≈ 139).
//   · a title that still doesn't fit ellipsises in the band (rule unchanged) and, ONLY when it is
//     cut, shows the full title in a Tooltip on hover (shared placement: top, then bottom).
// Buttons are IconButton sm (24, popup-title-btn-size) — the product's 20 goes. Fullscreen (glyph fit)
// sits left of ×, popup-title-btn-gap (4) apart, and only on popups that declare a fullscreen stage.



const TITLE_RESERVE = (n) => `calc(var(--tasty-popup-title-edge-inset) + ${n} * var(--tasty-popup-title-btn-size) + ${n - 1} * var(--tasty-popup-title-btn-gap) + var(--tasty-popup-title-text-gap))`;

// The shared popup title bar. fullscreen = the popup declares a fullscreen stage (adds the fit button).
function PopupTitleBar({ title, id, fullscreen = false, edge = "var(--tasty-info-modal-title-edge)", onClose, closeLabel = "Close" }) {
  const n = fullscreen ? 2 : 1;
  const tRef = React.useRef(null);
  const [cut, setCut] = React.useState(false);
  React.useLayoutEffect(() => {
    const el = tRef.current; if (!el) return;
    const check = () => { const c = el.scrollWidth > el.clientWidth; setCut((p) => (p === c ? p : c)); };
    check();
    const ro = new ResizeObserver(check); ro.observe(el);
    return () => ro.disconnect();
  }, [title]);
  // Same DOM in both states (no re-parenting); the full title surfaces only when cut.
  // Kit stand-in for the hover Tooltip: the native title attribute.
  const text = <span ref={tRef} id={id} title={cut ? title : undefined} style={{ display: "block", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{title}</span>;
  return (
    <div style={{ flex: "none", position: "relative", height: "var(--tasty-control-height)", display: "flex", alignItems: "center", justifyContent: "center",
      padding: "0 " + TITLE_RESERVE(n), background: "var(--tasty-bg-sidebar)", borderBottom: "var(--tasty-border-width) solid " + edge }}>
      <span style={{ display: "flex", minWidth: 0 }}>{text}</span>
      <span style={{ position: "absolute", right: "var(--tasty-popup-title-edge-inset)", top: 0, bottom: 0, display: "flex", alignItems: "center", gap: "var(--tasty-popup-title-btn-gap)" }}>
        {fullscreen && <IconButton size="sm" aria-label="Fullscreen" title="Fullscreen"><Icon name="fit" /></IconButton>}
        <IconButton size="sm" aria-label={closeLabel} title={closeLabel} onClick={onClose}><Icon name="close" /></IconButton>
      </span>
    </div>
  );
}

// scroll: "top" | "mid" | "bottom" | "fits" (no overflow)
function InfoModalShell({ title, children, actions, scroll = "fits", id = "info-modal" }) {
  const ref = React.useRef(null);
  React.useEffect(() => {
    const el = ref.current; if (!el) return;
    const max = el.scrollHeight - el.clientHeight;
    el.scrollTop = scroll === "mid" ? Math.round(max / 2) : scroll === "bottom" ? max : 0;
  }, [scroll]);
  const edgeBottom = scroll === "top" || scroll === "mid";
  return (
    <div role="dialog" aria-modal="true" aria-labelledby={id + "-title"} style={{ width: "var(--tasty-info-modal-width)",
      minHeight: "var(--tasty-info-modal-min-height)", maxHeight: "var(--tasty-info-modal-max-height)",
      display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)",
      borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <PopupTitleBar title={title} id={id + "-title"} />
      <div ref={ref} className="tasty-scroll" style={{ flex: 1, minHeight: 0, overflow: "auto", padding: "var(--tasty-space-md) var(--tasty-space-lg)",
        display: "flex", flexDirection: "column", gap: "var(--tasty-info-modal-para-gap)",
        fontSize: "var(--tasty-font-size-body)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)" }}>
        {React.Children.map(children, (p) => React.isValidElement(p) ? React.cloneElement(p, { style: { margin: 0, textWrap: "pretty", ...(p.props.style || {}) } }) : p)}
      </div>
      <div style={{ flex: "none", display: "flex", justifyContent: "flex-end", flexWrap: "wrap", gap: "var(--tasty-space-sm)",
        padding: "var(--tasty-space-md) var(--tasty-space-lg)",
        borderTop: "var(--tasty-border-width) solid " + (edgeBottom ? "var(--tasty-info-modal-scroll-edge)" : "transparent") }}>
        {actions || <Button variant="primary">OK</Button>}
      </div>
    </div>
  );
}

// Two of the five plain queue messages, drawn in the shell.
function ThemeNotFoundModal() {
  return (
    <InfoModalShell id="theme-nf" title="Theme not found">
      <p>The theme "gruvbox-hard" set in settings.toml was not found, so the default theme is used.</p>
    </InfoModalShell>
  );
}
function DbInitErrorModal() {
  return (
    <InfoModalShell id="db-err" title="Database initialization error" actions={<Button variant="primary">Quit</Button>}>
      <p>The database file is locked by another Tasty process. Close the other instance and start Tasty again.</p>
    </InfoModalShell>
  );
}

// Notifications popup head (352 wide) — the one popup that declares a fullscreen stage today.
function NotificationsPopupHead({ title = "Notifications", w = "var(--tasty-notifications-popup-width)" }) {
  return (
    <div style={{ width: w, display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)",
      borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <PopupTitleBar title={title} fullscreen />
      <div style={{ padding: "var(--tasty-space-md) var(--tasty-space-lg)", fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-muted)" }}>No notifications.</div>
    </div>
  );
}

// Convert surface (2026-09-29): the product's small list popup is the target shape (the 400 dialog
// is retired). Title bar + one MenuItem per kind the surface can become; click converts, Esc / × close.
// Width = convert-popup-width (240 × ui_scale); height = title + rows.
// Shortcut chip (2026-10-12): each row ends in a DS Kbd
// keycap with its letter (kind id's first latin letter, upper-cased, locale-independent) through the
// MenuItem `shortcut` slot. Display only — no click target. Same keycap in every row state (rest /
// hover / active). A row whose letter collides with an earlier row has no shortcut → no chip and no
// reserved slot; the label runs to the row's end. Label ellipsises, the chip never shrinks.
// kinds: [icon, label, key | null]
const CONVERT_KINDS = [["markdown", "Markdown", "M"], ["image", "Image", "I"], ["gitTree", "DAG", "D"], ["html", "HTML", "H"], ["folder", "Explorer", "E"]];
// opaque (2026-10-21): shell over an opaque-mode WebView page — square corner, no shadow, border-frame kept
function ConvertSurfacePopup({ title = "Surface Type", kinds = CONVERT_KINDS, active = 0, hover = -1, opaque = false }) {
  return (
    <div style={{ width: "var(--tasty-convert-popup-width)", display: "flex", flexDirection: "column", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-frame)",
      borderRadius: opaque ? "var(--tasty-webview-opaque-radius)" : "var(--tasty-radius)", boxShadow: opaque ? "var(--tasty-webview-opaque-shadow)" : "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <PopupTitleBar title={title} />
      <div style={{ display: "flex", flexDirection: "column", padding: "var(--tasty-space-xs)" }}>
        {kinds.map(([ic, label, key], i) => <MenuItem key={ic + i} icon={<Icon name={ic} />} label={label} active={i === active}
          shortcut={key ? <Kbd keys={[key]} aria-hidden="true" /> : null}
          style={i === hover && i !== active ? { background: "var(--tasty-menu-item-bg-hover)", color: "var(--tasty-menu-item-fg-hover)" } : undefined} />)}
      </div>
    </div>
  );
}

export { ConvertSurfacePopup, InfoModalShell, ThemeNotFoundModal, DbInitErrorModal, PopupTitleBar, NotificationsPopupHead };
