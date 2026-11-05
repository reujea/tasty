import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Kbd, MenuItem } from "../../components/index.js";
import { GalleryNoticeColumn } from "./notice-column.jsx";
import { GalleryWebviewOpaque } from "./opaque-over-page.jsx";
import { GroundThemes as PGroundThemes, GroundCap as PGroundCap, ToolsMenuFrame, SearchBarFrame, HeldLabel, TabStripMock, SidebarMock, RailMock, CatSwitchSidebarMock, CatSwitchRailMock, ic, CatMenu, RailCategoryFrame, catIc, ModifierHintPanelG, MH_CAT_SECTIONS, MH_MIXED_SECTIONS, MH_EMPTY_SECTIONS } from "../parts/overlay-frames.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

// Tasty Gallery — Overlays · Popups & menus (anchored, no scrim)
// One of the five Overlays sub-pages (dialogs · windows · popups · banners · tutorial). Frame components are shared from
// parts/overlay-frames.jsx (parts/overlay-frames.jsx); this file holds only the
// page's specimens + nav. See the other overlays-*.jsx for the rest.





const NAV = [
  { id: "tools", label: "Tools menu" },
  { id: "search", label: "Search bar" },
  { id: "switch", label: "Switch-number overlay" },
  { id: "modhint", label: "Modifier hints" },
  { id: "categories", label: "Workspace categories" },
  { id: "notices", label: "Notice column · restore notice" },
  { id: "webview", label: "Over a web page (opaque)" },
];

// Interactive hold-to-reveal demo: press & hold the button; after a 500ms dwell
// the panel fades in (opacity 0.2→1 over 200ms). Release dismisses instantly.
// Reduced-motion shows it immediately with no fade.
function ModHintHoldDemo() {
  const reduce = typeof window !== "undefined" && window.matchMedia
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const [phase, setPhase] = React.useState("idle"); // idle | pending | shown
  const [op, setOp] = React.useState(reduce ? 1 : 0.2);
  const timer = React.useRef(null);
  const start = () => {
    if (phase !== "idle") return;
    setPhase("pending");
    timer.current = setTimeout(() => setPhase("shown"), 500);
  };
  const stop = () => { clearTimeout(timer.current); setPhase("idle"); };
  React.useEffect(() => () => clearTimeout(timer.current), []);
  React.useEffect(() => {
    if (phase !== "shown") { setOp(reduce ? 1 : 0.2); return undefined; }
    if (reduce) { setOp(1); return undefined; }
    setOp(0.2);
    const id = requestAnimationFrame(() => requestAnimationFrame(() => setOp(1)));
    return () => cancelAnimationFrame(id);
  }, [phase, reduce]);
  const status = phase === "idle" ? "press & hold" : phase === "pending" ? "holding… (500ms)" : "showing";
  return (
    <div style={{ position: "relative", width: "100%", maxWidth: 560, height: 460, borderRadius: "var(--tasty-radius)",
      overflow: "hidden", border: "var(--tasty-border-width) solid var(--tasty-border-default)", background: "var(--tasty-bg-app)" }}>
      {/* faux app: sidebar + terminal surface */}
      <div style={{ position: "absolute", inset: 0, display: "flex" }}>
        <div style={{ width: 180, flex: "none", background: "var(--tasty-bg-sidebar)", borderRight: "var(--tasty-border-width) solid var(--tasty-separator)" }} />
        <div style={{ flex: 1, background: "var(--tasty-surface-terminal-focused-bg)" }} />
      </div>
      {/* the hold control, centred on the surface */}
      <div style={{ position: "absolute", top: 20, left: 200, display: "flex", alignItems: "center", gap: 10 }}>
        <button type="button"
          onMouseDown={start} onMouseUp={stop} onMouseLeave={stop}
          onTouchStart={(e) => { e.preventDefault(); start(); }} onTouchEnd={stop}
          style={{ appearance: "none", cursor: "pointer", userSelect: "none", display: "inline-flex", alignItems: "center", gap: 8,
            height: "var(--tasty-control-height)", padding: "0 var(--tasty-space-md)", borderRadius: "var(--tasty-radius)",
            border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
            background: phase === "idle" ? "var(--tasty-surface-raised)" : "var(--tasty-surface-active)",
            color: "var(--tasty-text-secondary)", fontFamily: "var(--tasty-font-ui)", fontSize: 13 }}>
          <Kbd keys="Ctrl" /><span>Hold</span>
        </button>
        <span style={{ fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{status}</span>
      </div>
      {/* the panel — default position: bottom-left, above the sidebar footer */}
      {phase === "shown" && (
        <div style={{ position: "absolute", left: 12, bottom: 12, opacity: op,
          transition: reduce ? "none" : "opacity var(--tasty-modhint-fade) var(--tasty-ease-ui)" }}>
          <ModifierHintPanelG />
        </div>
      )}
    </div>
  );
}

function Page() {
  return (
    <>
<Section id="tools" title="Tools menu">
        <Spec id="tools-menu-anchored-no-scrim" title="Tools menu — anchored, no scrim"
          when={<>A popup anchored <b>above</b> the sidebar Tools button (left-aligned to it). Unlike the modal layer this is a lightweight menu — <b>no scrim</b>, dismisses on outside click or <Kbd keys="Esc" />. Built-in entries first, in this fixed order — <b>Command palette… · Listening ports… · Remote connections… · Presets · Tutorial… · Task DAGs · Open File…</b> — then a separator, then <b>plugin-contributed</b> tools in manifest <code>order</code> (Clipboard Viewer 100 · Git 130). Every label that opens a further window ends in the single ellipsis character <code>…</code>, never three periods. <b>Width (2026-10-07)</b>: the menu is as wide as its longest row (label + <span className="tok">--tasty-menu-item-padding-x</span> both sides + the 4 ring + border), never narrower than <b>160</b> and never wider than <b>240</b>; a label past 240 ellipsizes at its <b>end</b>. This covers translated labels without a per-locale width.</>}>
          <Stage variant="solo center"><ToolsMenuFrame /></Stage>
          <Meta
            specs={[["width", <>fit content · min 160 <span className="tok">--tasty-tools-menu-min-width</span> · max 240 <span className="tok">--tasty-tools-menu-max-width</span></>], ["overflow", "end ellipsis past max"], ["anchor", "above button, left-aligned"], ["rows", "28px MenuItem, no icons"], ["built-in", "7 — fixed order"], ["inner ring", <>4 · <span className="tok">--tasty-popup-content-margin</span></>], ["scrim", "none"], ["dismiss", <>outside click · <span className="ic">Esc</span></>]]}
            tokens={[{ tok: "--tasty-menu-bg", use: "menu fill", color: "var(--tasty-menu-bg)" }, { tok: "--tasty-menu-border", use: "edge → border-strong", color: "var(--tasty-menu-border)" }, { tok: "--tasty-popup-content-margin", use: "→ space-xs 4" }, { tok: "--tasty-tools-menu-min-width", use: "→ size-160" }, { tok: "--tasty-tools-menu-max-width", use: "→ size-240" }, { tok: "--tasty-shadow-popover", use: "lift" }]} />
          <Note>Plugins extend the list <b>below</b> the separator only — built-in order is fixed. This is a popover, not a dialog: it never dims the app.</Note>
        </Spec>
      </Section>

<Section id="search" title="Search bar">
        <Spec id="search-bar-ctrl-f-headless" title="Search bar — Ctrl/⌘F, headless"
          when={<>A <b>360px</b> headless popup floating at the <b>top-right of the focused surface</b> — not on a scrim, and it <b>stays open</b> on outside click. One row: a flex Input, a <code>cur/total</code> counter (red on zero matches), ▲▼ steppers, <code>Aa</code> / <code>.*</code> / <code>ab</code> option toggles, a divider, and close. <Kbd keys="↵" /> next, <Kbd keys="Esc" /> closes.</>}>
          <Stage variant="solo center"><SearchBarFrame /></Stage>
          <Meta
            specs={[["frame", "360 × 28 row"], ["anchor", "top-right of focused surface"], ["counter", "cur/total · red on 0"], ["toggles", "Aa · .* · ab"], ["dismiss", <span className="ic">Esc</span>]]}
            tokens={[{ tok: "--tasty-surface-raised", use: "bar fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-search-bar-border", use: "edge → menu-border", color: "var(--tasty-search-bar-border)" }, { tok: "--tasty-accent-danger", use: "no-match counter", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-shadow-popover", use: "lift" }]} />
          <Note>Sticky-focus popover: the input keeps focus and the bar persists through clicks in the terminal — it closes only on <span className="ic">Esc</span> or its × button.</Note>
        </Spec>
        <Spec id="query-field-ground-normal-focused-invalid" title="Query field ground — normal · focused · invalid"
          when={<>The bar is <span className="tok">--tasty-surface-raised</span>, so the query field is an <b>Input ground="raised"</b> (the 10-14 rule, no exception): <code>bg-panel</code> fill, <code>border-strong</code> edge. <b>normal</b> is the box after a shortcut hands focus back to the terminal; the bar stays open. <b>invalid</b> is a regex error with <code>.*</code> on: the edge is <code>accent-danger</code> in both focus states, and focused adds the danger ring. Behaviour (Enter, Shift+Enter, Esc, filtering) and copy are unchanged.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <PGroundThemes render={() => (
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", width: 400, maxWidth: "100%" }}>
                <PGroundCap>normal — focus back in the terminal</PGroundCap><SearchBarFrame height={48} />
                <PGroundCap>focused</PGroundCap><SearchBarFrame height={48} focused />
                <PGroundCap>invalid · focused</PGroundCap><SearchBarFrame height={48} focused invalid value="(tas" />
                <PGroundCap>invalid · focus in the terminal</PGroundCap><SearchBarFrame height={48} invalid value="(tas" />
              </div>
            )} />
          </Stage>
          <Meta
            specs={[["field ground", <>ground="raised" · no exception</>], ["normal", <>input-raised-bg → bg-panel · input-raised-border → border-strong</>], ["focused", <>border-focus edge + 1px border-focus ring</>], ["invalid", <>accent-danger edge · focused adds 1px accent-danger ring · counter 0/0 accent-danger</>], ["Mocha / Latte", "bg-panel = base · border-strong = surface1 · bar surface-raised = surface0"]]}
            tokens={[{ tok: "--tasty-input-raised-bg", use: "field fill", color: "var(--tasty-input-raised-bg)" }, { tok: "--tasty-input-raised-border", use: "field edge", color: "var(--tasty-input-raised-border)" }, { tok: "--tasty-border-focus", use: "focused", color: "var(--tasty-border-focus)" }, { tok: "--tasty-accent-danger", use: "invalid", color: "var(--tasty-accent-danger-raised)" }]} />
        </Spec>
      </Section>

<Section id="switch" title="Switch-number overlay">
        <Spec id="tab-switch-overlay-modifier-held" title="Tab switch overlay — modifier held"
          when={<>Holding the <b>tab switch modifier</b> (<Kbd keys="Ctrl" /> by default — rebindable) paints a number keycap over <b>every tab</b>, previewing the <Kbd keys="Ctrl+1" />…<Kbd keys="Ctrl+9" /> / <Kbd keys="Ctrl+0" /> switch shortcuts. The keycap <b>replaces the tab's leading icon in place</b> — no width change, no scrim; the icon returns on release. The <b>active</b> tab's keycap is <b>accent-filled</b> so "where am I" stays legible. Workspaces show nothing — only the modifier bound to tabs triggers this.</>}>
          <Stage variant="solo center" style={{ flexDirection: "column", gap: 16, padding: 20, background: "var(--tasty-bg-app)" }}>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>released — each tab shows its surface icon</div>
              <TabStripMock held={false} />
            </div>
            <div>
              <HeldLabel keys="Ctrl">icon slot becomes the number keycap</HeldLabel>
              <TabStripMock held={true} />
            </div>
          </Stage>
          <Meta
            specs={[["widget", <><span className="ic">Kbd</span> keycap · 16px</>], ["content", "digit only · 0 = 10th tab"], ["placement", "replaces leading icon, in place"], ["range", "1–9 + 0; 11th tab onward: none"], ["active tab", "accent-filled keycap"], ["scrim", "none"], ["appear", "90ms fade · release 0ms"]]}
            tokens={[{ tok: "--tasty-switch-overlay-bg", use: "keycap fill", color: "var(--tasty-switch-overlay-bg)" }, { tok: "--tasty-switch-overlay-active-bg", use: "active-tab keycap", color: "var(--tasty-switch-overlay-active-bg)" }, { tok: "--tasty-switch-overlay-border", use: "keycap edge", color: "var(--tasty-switch-overlay-border)" }, { tok: "--tasty-switch-overlay-fade", use: "90ms appear" }]} />
          <Note>The digit equals the <b>key you press</b>, so the 10th tab reads <span className="ic">0</span> (not "10") to match <Kbd keys="Ctrl+0" />. Tabs past the 10th carry no shortcut, so they get <b>no keycap</b> — never paint a number a key won't trigger.</Note>
        </Spec>
        <Spec id="workspace-switch-overlay-modifier-held" title="Workspace switch overlay — modifier held"
          when={<>Holding the <b>workspace switch modifier</b> (<Kbd keys="Alt" /> by default — rebindable) paints a keycap over <b>each sidebar workspace</b>, previewing <Kbd keys="Alt+1" />…<Kbd keys="Alt+9" />. In the <b>full</b> sidebar the keycap replaces the leading <b>status dot</b>; in the <b>collapsed</b> rail it replaces the <b>letter avatar</b>. Same in-place, no-scrim rule as tabs; tabs show nothing here. Releasing dismisses instantly.</>}>
          <Stage variant="solo center" style={{ alignItems: "flex-start", gap: 24, padding: 20, background: "var(--tasty-bg-app)", flexWrap: "wrap" }}>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>released — full sidebar</div>
              <SidebarMock held={false} />
            </div>
            <div>
              <HeldLabel keys="Alt">status dot becomes the keycap</HeldLabel>
              <SidebarMock held={true} />
            </div>
            <div>
              <HeldLabel keys="Alt">collapsed rail · letter becomes the keycap</HeldLabel>
              <RailMock />
            </div>
          </Stage>
          <Meta
            specs={[["widget", <><span className="ic">Kbd</span> keycap · 16px</>], ["content", "digit only (1–9)"], ["full placement", "replaces status dot"], ["collapsed", "replaces letter avatar"], ["range", "1–9 only; 10th onward: none"], ["active ws", "accent-filled keycap"], ["scrim", "none"]]}
            tokens={[{ tok: "--tasty-switch-overlay-bg", use: "keycap fill", color: "var(--tasty-switch-overlay-bg)" }, { tok: "--tasty-switch-overlay-active-bg", use: "active-ws keycap", color: "var(--tasty-switch-overlay-active-bg)" }, { tok: "--tasty-switch-overlay-active-fg", use: "digit on accent", color: "var(--tasty-switch-overlay-active-fg)" }, { tok: "--tasty-switch-overlay-size", use: "16px footprint" }]} />
          <Note>Workspace shortcuts only go <b>1–9</b> (no <span className="ic">0</span>), so the 10th workspace and beyond get <b>no keycap</b> — in the collapsed rail they keep their letter avatar (last item above). The bound modifier is read from the same keybindings the shortcut uses: rebind it and the overlay follows.</Note>
          <Do><b>Do</b> replace the leading indicator rather than overlapping the label — the keycap's own surface-raised fill + border clears 4.5:1 on both tab and sidebar backgrounds without a scrim.</Do>
        </Spec>
        <Spec id="category-switch-overlay-ctrl-shift-held" title="Category switch overlay — Ctrl+Shift held"
          when={<>A third switch axis, present only when <b>Workspace categories (folders)</b> is on. Holding the <b>category switch modifier</b> (<Kbd keys="Ctrl+Shift" /> by default — rebindable) paints a keycap over <b>each category</b>, previewing <Kbd keys="Ctrl+Shift+1" />…<Kbd keys="Ctrl+Shift+9" /> / <Kbd keys="Ctrl+Shift+0" />. In the <b>full</b> sidebar the keycap is <b>right-aligned on the category header</b> — it does <b>not</b> replace the chevron, which stays to carry collapse state (and its auto-expand rotation). In the <b>rail</b> it sits <b>centered on the <code>---</code> boundary</b>. The reserved <b>normal</b> category (“Workspaces”) is <b>1</b>. This overlay and the workspace overlay are <b>modifier-exclusive</b> — <Kbd keys="Alt" /> paints workspaces, <Kbd keys="Ctrl+Shift" /> paints categories, never both at once, so workspace rows keep their status dots here. The chord is the product default <code>category_switch_modifier = ctrl+shift</code> (every preset) — the gallery and the app read it from there.</>}>
          <Stage variant="solo center" style={{ alignItems: "flex-start", gap: 24, padding: 20, background: "var(--tasty-bg-app)", flexWrap: "wrap" }}>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>released — full sidebar with categories</div>
              <CatSwitchSidebarMock held={false} />
            </div>
            <div>
              <HeldLabel keys="Ctrl+Shift">category header gets a trailing keycap</HeldLabel>
              <CatSwitchSidebarMock held={true} />
            </div>
            <div>
              <HeldLabel keys="Ctrl+Shift">collapsed rail · <code>---</code> becomes the keycap</HeldLabel>
              <CatSwitchRailMock held={true} />
            </div>
          </Stage>
          <Meta
            specs={[["widget", <><span className="ic">Kbd</span> keycap · 16px</>], ["content", "digit only · 0 = 10th category"], ["full placement", "trailing on header — chevron kept"], ["rail", <>centered on the <code>---</code> boundary</>], ["range", "1–9 + 0; 11th category onward: none"], ["reserved", "normal (“Workspaces”) = 1"], ["active cat", "accent-filled keycap"], ["exclusivity", "Ctrl = tabs · Alt = ws · Ctrl+Shift = category"], ["default", "category_switch_modifier = ctrl+shift"]]}
            tokens={[{ tok: "--tasty-switch-overlay-bg", use: "keycap fill", color: "var(--tasty-switch-overlay-bg)" }, { tok: "--tasty-switch-overlay-active-bg", use: "active-category keycap", color: "var(--tasty-switch-overlay-active-bg)" }, { tok: "--tasty-switch-overlay-active-fg", use: "digit on accent", color: "var(--tasty-switch-overlay-active-fg)" }, { tok: "--tasty-surface-active", use: "landed workspace row", color: "var(--tasty-surface-active)" }]} />
          <Note><b>Auto-expand:</b> switching to a <b>collapsed</b> category rotates its chevron open and reveals its rows (the chevron is why it isn't replaced by the keycap) — the new collapse state persists in <code>layout.json</code>. <b>Last-active:</b> the switch lands on that category's most-recently-focused workspace (its first if never visited), shown with the ordinary <b>surface-active + 2px accent bar</b> — no separate affordance. Reuses the <span className="ic">NumCap</span> keycap and every <span className="tok">--tasty-switch-overlay-*</span> token; no new tokens.</Note>
        </Spec>
      </Section>

<Section id="modhint" title="Modifier hints">
        <Spec id="modifier-hint-panel-hold-to-reveal" title="Modifier hint panel — hold to reveal"
          when={<>A floating, <b>focus-less</b> discoverability panel painted while a modifier is <b>held</b> — like macOS's hold-⌘ shortcut sheet. Press &amp; hold the <Kbd keys="Ctrl" /> control: after a <b>500ms</b> dwell the panel <b>fades in</b> (opacity 0.2→1 over 200ms); release dismisses it <b>instantly</b>. It defaults to the <b>bottom-left</b>, just above the sidebar footer, and is <b>draggable</b> + edge-<b>resizable</b>. It never dims the app and never takes keyboard focus.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <ModHintHoldDemo />
          </Stage>
          <Meta
            specs={[["trigger", <>modifier held <b>500ms</b></>], ["default size", <>180 × 400 · <span className="tok">--tasty-modhint-width</span> × <span className="tok">--tasty-modhint-height</span></>], ["default pos", "bottom-left, above sidebar footer"], ["appear", "fade opacity 0.2→1 · 200ms"], ["release", "0ms — instant dismiss"], ["focus", "never — not a focus window"], ["scrim", "none"]]}
            tokens={[{ tok: "--tasty-modhint-bg", use: "panel fill", color: "var(--tasty-modhint-bg)" }, { tok: "--tasty-modhint-shadow", use: "floating lift" }, { tok: "--tasty-modhint-fade", use: "200ms fade-in" }, { tok: "--tasty-modhint-hold-delay", use: "500ms hold" }]} />
          <Note>Reduced motion skips the fade — the panel appears at full opacity the instant the 500ms hold completes. The <b>bound</b> modifier set is read from the same keybindings source the shortcuts use (Win/Linux: Ctrl / Alt / Shift; macOS adds Cmd / Option).</Note>
        </Spec>

        <Spec id="anatomy-chord-ordering" title="Anatomy & chord ordering"
          when={<>Sections are the shortcut <b>chords that contain the held modifier</b>, ordered by <b>chord size</b> (1→2→3→4) then by modifier priority <b>Ctrl → Cmd/Alt → Option → Shift</b>. Each section header is the chord as <span className="ic">Kbd</span> keycaps; under it sit the bound <b>keycap rows</b> (action + <span className="ic">Kbd</span>), an optional <b>special-role row</b> (what the chord itself does — e.g. Ctrl = tab-switch numbers, Shift = TUI mouse-capture bypass), and <b>plugin</b> rows carrying the agent dot. A chord with <b>no</b> bindings and no role is <b>omitted</b> entirely.</>}>
          <Stage variant="solo center" style={{ padding: 24, background: "var(--tasty-bg-app)", gap: 40, alignItems: "flex-start", flexWrap: "wrap" }}>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>default — Ctrl held</div>
              <ModifierHintPanelG />
            </div>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>resized taller (drag any edge) — 480</div>
              <ModifierHintPanelG style={{ height: 480 }} />
            </div>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>Ctrl+Shift held — categories on</div>
              <ModifierHintPanelG held="Ctrl+Shift" sections={MH_CAT_SECTIONS} style={{ height: 300 }} />
            </div>
          </Stage>
          <Meta
            specs={[["drag strip", "muted sidebar fill — NOT a titlebar"], ["X button", "dismiss for this hold only"], ["section order", "size ↑, then Ctrl→Cmd/Alt→Option→Shift"], ["keycap row", "action + Kbd"], ["role row", "washed, leading glyph, no keycap"], ["plugin row", "agent dot, context-dependent"], ["row text", <>11 · <span className="tok">--tasty-modhint-row-font-size</span> (chord · role · empty rows)</>], ["chord row", <>min 24 · <span className="tok">--tasty-modhint-row-min-height</span></>], ["role row pad", <>6 / 8 · <span className="tok">--tasty-modhint-row-gap</span> / <span className="tok">--tasty-space-sm</span></>], ["grip", <>12 · inset 2 from right/bottom · <span className="tok">--tasty-modhint-grip-size</span> · <span className="tok">--tasty-modhint-grip-inset</span> (on-scale)</>], ["min size", "180 × 240"]]}
            tokens={[{ tok: "--tasty-modhint-header-bg", use: "drag strip", color: "var(--tasty-modhint-header-bg)" }, { tok: "--tasty-modhint-role-bg", use: "role-row wash", color: "var(--tasty-modhint-role-bg)" }, { tok: "--tasty-modhint-role-fg", use: "role glyph", color: "var(--tasty-modhint-role-fg)" }, { tok: "--tasty-accent-agent", use: "plugin dot", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-modhint-grip-fg", use: "resize grip", color: "var(--tasty-modhint-grip-fg)" }]} />
          <Do><b>Do</b> keep it opaque and low-chrome — it floats over live terminal output, and the muted drag strip (not a titlebar) is the promise that it will never steal focus.</Do>
          <Dont><b>Don't</b> hide a chord that a key combo <i>could</i> reach — a bound-empty chord shows its ChordHead with a muted <b>“No shortcuts bound”</b> placeholder (see below), never nothing, so holding an all-empty combo still surfaces the panel.</Dont>
          <Note>When <b>Workspace categories (folders)</b> is on, the <Kbd keys="Ctrl+Shift" /> chord carries a <b>“Switch category”</b> role row (folder glyph) — the discoverability entry for the category quick-switch (see Switch-number overlay). A chord's role row is how a numeric switch announces itself, exactly like <Kbd keys="Ctrl" /> = tab-switch numbers.</Note>
        </Spec>

        <Spec id="empty-chord-no-shortcuts-bound-placeholder" title="Empty chord — “No shortcuts bound” placeholder"
          when={<>A chord that <b>exists but carries no binding and no role</b> is <b>not</b> omitted — its <span className="ic">ChordHead</span> shows with a single muted <b>placeholder row</b> beneath it. This <b>reverses</b> the original decision (“empty chord → omitted entirely, never ‘none’”): holding an all-empty combo like <Kbd keys="Ctrl+Alt+Shift" /> now surfaces the panel with a <b>“No shortcuts bound”</b> line instead of showing nothing. The placeholder is the <b>quietest</b> row in the panel — <span className="ic">text-muted</span>, <b>no keycap</b>, <b>no wash</b>, <b>no leading glyph</b> — an <i>absence</i> signal, deliberately not a role-row. It is fully static and non-interactive.</>}>
          <Stage variant="solo center" style={{ padding: 24, background: "var(--tasty-bg-app)", gap: 40, alignItems: "flex-start", flexWrap: "wrap" }}>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>all-empty — Ctrl+Alt+Shift held</div>
              <ModifierHintPanelG held="Ctrl+Alt+Shift" sections={MH_EMPTY_SECTIONS} />
            </div>
            <div>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)", marginBottom: 8 }}>mixed — Ctrl held, filled + empty sections</div>
              <ModifierHintPanelG held="Ctrl" sections={MH_MIXED_SECTIONS} />
            </div>
          </Stage>
          <Meta
            specs={[["row", "placeholder text only — no keycap"], ["tone", "muted (below a keycap row)"], ["wash", "none — not a role-row"], ["glyph", "none"], ["min height", "20px (tighter than a 24px keycap row)"], ["section gap", "3px inside (empty sits tight)"], ["copy", "“No shortcuts bound”"], ["interaction", "none — static, no hover/focus"]]}
            tokens={[{ tok: "--tasty-modhint-empty-fg", use: "placeholder text", color: "var(--tasty-modhint-empty-fg)" }, { tok: "--tasty-text-muted", use: "→ semantic", color: "var(--tasty-text-muted)" }, { tok: "--tasty-modhint-section-gap", use: "between sections (unchanged)" }]} />
          <Do><b>Do</b> keep the placeholder muted and bare — it announces an <i>absence</i>, so it must read quieter than a bound keycap row and never borrow the role-row's wash or glyph.</Do>
          <Dont><b>Don't</b> give it a keycap, a hover highlight, or a washed background — a keycap implies a binding that isn't there, and a wash makes it look like a role-row that <i>does</i> something.</Dont>
          <Note>i18n key <code>modifier_hint.empty</code> — en <code>“No shortcuts bound”</code> · ko <code>“지정된 단축키 없음”</code> · ja <code>“割り当てなし”</code>. Fits one line in the 180px panel across all three. New token <code>--tasty-modhint-empty-fg</code> → <code>text-muted</code> (a pointer, no new primitive), so the implementing side flips <code>build_hint_sections()</code>' empty-<code>retain</code> and paints one <code>draw_row</code> variant.</Note>
        </Spec>
      </Section>

      <Section id="categories" title="Workspace categories">
        <Spec id="sidebar-context-menu-target-resolves-under-the-cursor" title="Sidebar context menu — target resolves under the cursor"
          when={<>Right-clicking anywhere in the sidebar workspace area opens this menu (no dedicated buttons) — only when <b>Settings › General › Workspace categories (folders)</b> is on. Items resolve to the <b>target under the cursor</b>: empty <b>background</b>, a <b>category header</b>, or a <b>workspace row</b>. The reserved <b>normal</b> category (“Workspaces”) is additive-only — no rename/delete. Same <span className="ic">MenuItem</span> + anchored-popup language as the Tools menu.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 22, flexWrap: "wrap", alignItems: "flex-start" }}>
            <CatMenu caption="target: background (cwd)">
              <MenuItem label="New category" icon={catIc.plus} />
            </CatMenu>
            <CatMenu caption="target: category header">
              <MenuItem label="Add workspace" icon={catIc.plus} />
              <MenuItem separator />
              <MenuItem label="Rename category" icon={catIc.edit} />
              <MenuItem label="Delete category" icon={catIc.trash} danger />
              <MenuItem separator />
              <MenuItem label="New category" icon={catIc.plus} />
            </CatMenu>
            <CatMenu caption="target: reserved (normal)">
              <MenuItem label="Add workspace" icon={catIc.plus} />
              <MenuItem separator />
              <MenuItem label="Rename category" icon={catIc.edit} disabled />
              <MenuItem label="Delete category" icon={catIc.trash} disabled />
              <MenuItem separator />
              <MenuItem label="New category" icon={catIc.plus} />
            </CatMenu>
            <CatMenu caption="target: workspace row">
              <MenuItem label="Move to category" icon={catIc.move} disabled />
              <MenuItem label="Workspaces" />
              <MenuItem label="Services" />
              <MenuItem separator />
              <MenuItem label="New category" icon={catIc.plus} />
            </CatMenu>
          </Stage>
          <Meta
            specs={[["trigger", "right-click, no dedicated button"], ["width", "176px min"], ["background", "New category"], ["category", "Add ws · Rename · Delete · New"], ["reserved", "Add ws · New (additive-only)"], ["workspace", "Move to category (inert header) · targets · New"], ["targets", "every category except the current one · normal = “Workspaces”"]]}
            tokens={[{ tok: "--tasty-surface-raised", use: "menu fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-strong", use: "edge", color: "var(--tasty-border-strong)" }, { tok: "--tasty-accent-danger", use: "delete row", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-shadow-popover", use: "lift" }]} />
          <Note>“Move to category” is an <b>inert header</b> with the target categories listed <b>flat</b> beneath it (current category omitted) — no submenu, because the product draws this menu with the OS-native context menu (2026-10-07). Drag-and-drop reorder stays the primary path; this is the menu fallback. New strings: <code>workspace_category.add_workspace</code> · <code>collapse</code> · <code>expand</code>. Existing: <code>new_category</code> · <code>rename_category</code> · <code>delete_category</code> · <code>move_to_category</code>.</Note>
        </Spec>
        <Spec id="collapsed-rail-category-button-anchored-popup" title="Collapsed rail — `---` category button + anchored popup"
          when={<>In the 52px rail, categories aren't labelled — each boundary is a clickable <b>horizontal <code>---</code> button</b> (the old thin separator, now a button). Clicking it opens a popup to its <b>right</b> (like the rail Tools button). The popup's <b>top line is the category name</b>, a <b>non-clickable header</b>; below are the actions. A category's avatars sit <b>below</b> its <code>---</code>; a <b>collapsed or empty</b> category shows only the <code>---</code> button.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)" }}>
            <RailCategoryFrame />
          </Stage>
          <Meta
            specs={[["boundary", <><code>---</code> full-width button</>], ["anchor", "popup to the right of the button"], ["header", "category name — non-clickable"], ["items", "Add workspace · Collapse · Rename · Delete"], ["collapsed/empty", <><code>---</code> only, no avatars</>], ["reserved", "no Rename/Delete"]]}
            tokens={[{ tok: "--tasty-separator", use: <><code>---</code> line (idle)</>, color: "var(--tasty-separator)" }, { tok: "--tasty-text-muted", use: <><code>---</code> line (hover)</>, color: "var(--tasty-text-muted)" }, { tok: "--tasty-surface-raised", use: "popup fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-menu-border", use: "edge", color: "var(--tasty-menu-border)" }, { tok: "--tasty-popup-content-margin", use: "inner ring 4 (all sides)" }, { tok: "--tasty-accent-danger", use: "delete row", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note>Collapse state is per-category and <b>shared</b> with the full sidebar (both seed from the persisted <code>collapsed</code> flag in <code>layout.json</code>). Toggling here or in the full header updates the same state.</Note>
        </Spec>
      </Section>
      <Section id="notices" title="Notice column — restore failure · save halt · response wait">
        {GalleryNoticeColumn ? <GalleryNoticeColumn /> : null}
      </Section>
      <Section id="webview" title="Over a web page — opaque mode">
        {GalleryWebviewOpaque ? <GalleryWebviewOpaque /> : null}
      </Section>
    </>
  );
}

export const page = {
  active: "overlays-popups",
  nav: NAV,
  head: {
    title: "Popups & menus",
    intro: "Explore menus, search bars, switch overlays, and modifier hints. Check their placement and dismissal, and compare opaque overlays above a web page.",
    
  },
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
