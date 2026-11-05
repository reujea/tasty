import React from "react";
import { Button as LButton, Input as LInput, Icon as LIcon, Spinner } from "../../components/index.js";
import { Section, Spec, Stage, Meta, Note } from "../parts/specimen.jsx";

const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty Gallery — Chrome · Startup (boot loading screen)
// The pre-app full-window loading surface (egui render_loading, mirrors
// render_shell_setup): app-bg fill + centered brand lockup → spinner → phase
// slot. App chrome, NOT terminal content, so the spinner may spin (reduced-
// motion → static). Reuses components/feedback/Spinner.jsx (size bumped to 32)
// and the components/BrandLogo/preview.html lockup verbatim.






const NAV = [
  { id: "default", label: "Default 1280×720" },
  { id: "minimal", label: "Minimal 640×480" },
  { id: "phases", label: "Phase variants" },
  { id: "spinner", label: "Spinner spec" },
  { id: "latte", label: "Latte variant" },
  { id: "shellsetup", label: "First-run shell setup" },
  { id: "shutdown", label: "Shutdown screen" },
  { id: "booterror", label: "Boot error screen" },
];

// Brand lockup — components/BrandLogo/preview.html (branding exception to the 14px UI cap,
// like --tasty-font-size-brand-wordmark). Mark → text gap = space-sm (8), the same gap
// as the sidebar wordmark; the old −10px overlap is retired (2026-10-07).
function Lockup() {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-sm)" }}>
      <img src={`${BASE}tasty-icon.png`} alt="tasty"
        style={{ width: 64, height: 64, imageRendering: "-webkit-optimize-contrast" }} />
      <span style={{ fontFamily: "var(--tasty-font-mono)", fontWeight: 700, fontSize: 38, letterSpacing: "var(--tasty-loading-lockup-tracking)", color: "var(--tasty-text-primary)" }}>
        tasty<span data-audit-exempt="brand" style={{ color: "var(--tasty-brand-melon-flesh)" }}>.</span>
      </span>
    </div>
  );
}

// A scaled true-size boot viewport. w/h are logical px; z the display scale.
function BootFrame({ w, h, z = 1, phase, showPhase = true, theme }) {
  const inner = {
    width: w, height: h, transform: z === 1 ? undefined : `scale(${z})`, transformOrigin: "top left",
  };
  const boot = {
    position: "absolute", inset: 0, background: "var(--tasty-bg-app)",
    display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
  };
  const wrap = {
    display: "inline-block", border: "var(--tasty-border-width) solid var(--tasty-border-strong)",
    borderRadius: "var(--tasty-radius-8)", overflow: "hidden",
  };
  const vp = { position: "relative", overflow: "hidden", width: w * z, height: h * z };
  const content = (
    <div style={inner}>
      <div style={boot}>
        <Lockup />
        <span style={{ marginTop: "var(--tasty-space-xl)", color: "var(--tasty-accent-primary-raised)" }}>
          <Spinner size={32} stroke={3} label="Starting tasty" />
        </span>
        <div style={{ marginTop: "var(--tasty-space-lg)", height: "var(--tasty-size-16)", lineHeight: "var(--tasty-size-16)",
          fontFamily: "var(--tasty-font-ui)", fontSize: "var(--tasty-font-size-body)",
          color: showPhase ? "var(--tasty-text-muted)" : "transparent" }}>
          {showPhase ? phase : "·"}
        </div>
      </div>
    </div>
  );
  return (
    <div style={wrap}>
      <div style={vp} {...(theme ? { "data-theme": theme } : {})}>{content}</div>
    </div>
  );
}

// First-run shell setup (render_shell_setup) — 2026-09-29, revised 2026-10-06.
// Same boot surface + lockup, then a form: title · sub · [Windows: Git Bash notice] · path Input ·
// validation line (height ALWAYS reserved = one caption line) · Quit / Use this shell.
// check: "empty" | "missing" | "notShell" | "valid"  — one line per host verdict (path empty / no file /
// file exists but name has no bash|zsh / valid).
const SHELL_CHECK = {
  empty:    null,
  missing:  { glyph: "alertCircle", fg: "var(--tasty-accent-danger-raised)",  text: "No file at this path" },
  notShell: { glyph: "alertCircle", fg: "var(--tasty-accent-danger-raised)",  text: "Not a bash or zsh executable" },
  valid:    { glyph: "check",       fg: "var(--tasty-accent-success-raised)", text: "Shell found" },
};
function ShellSetupFrame({ check = "valid", os = "mac", theme }) {
  const c = SHELL_CHECK[check];
  const win = os === "win";
  const value = { empty: "", missing: win ? "C:/Program Files/Git/bin/bash.exe" : "/usr/local/bin/zsh", notShell: win ? "C:/Windows/System32/cmd.exe" : "/usr/bin/fish", valid: win ? "D:/Tools/Git/bin/bash.exe" : "/bin/zsh" }[check];
  return (
    <div style={{ display: "inline-block", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius-8)", overflow: "hidden" }}>
      <div {...(theme ? { "data-theme": theme } : {})} style={{ width: 640, height: 480, background: "var(--tasty-bg-app)", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <Lockup />
        <div style={{ marginTop: "var(--tasty-space-xl)", width: "var(--tasty-size-360)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
          <div style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>Choose a shell</div>
          <div style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)" }}>New terminals start this shell. You can change it later in Settings.</div>
          {win && (
            <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-xs)", fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-accent-warning-raised)" }}>
              <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-size-1)" }}><LIcon name="alertTriangle" size="var(--tasty-icon-size-sm)" /></span>
              <span>Git Bash was not found. Install Git for Windows, or enter the path to bash.exe.</span>
            </div>
          )}
          <LInput block mono defaultValue={value} placeholder={win ? "C:/Program Files/Git/bin/bash.exe" : "/bin/zsh"} />
          <div style={{ display: "flex", alignItems: "center", gap: "var(--tasty-space-xs)", minHeight: "calc(var(--tasty-font-size-caption) * var(--tasty-line-height-ui))", fontSize: "var(--tasty-font-size-caption)", color: c ? c.fg : undefined }}>
            {c && <><LIcon name={c.glyph} size="var(--tasty-icon-size-sm)" />{c.text}</>}
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", marginTop: "var(--tasty-space-sm)" }}>
            <LButton variant="secondary">Quit</LButton>
            <LButton variant="primary" disabled={check !== "valid"}>Use this shell</LButton>
          </div>
        </div>
      </div>
    </div>
  );
}


// Boot error screen (boot_error_screen) — 2026-10-10 b12. The shell-setup structure, not a card:
// lockup → space-xl → 360 form: [alertCircle danger + title] · body · guidance · Quit (secondary, right).
const lCode = { fontFamily: "var(--tasty-ui-code-font)", fontSize: "1em", background: "var(--tasty-ui-code-bg)", color: "var(--tasty-ui-code-fg)", padding: "0 var(--tasty-ui-code-padding-x)", borderRadius: "var(--tasty-ui-code-radius)", whiteSpace: "nowrap" };
const BOOT_ERRORS = {
  engine: { title: "Could not start the terminal", body: <>Tasty could not start a terminal. Details: No such file or directory (os error 2)</>, hint: <>Check <code style={lCode}>general.shell</code> in your config.toml and make sure the shell exists, then start Tasty again.</> },
  datadir: { title: "Data folder already in use", body: <>Another Tasty is already using this data folder: /Users/hana/Library/Application Support/tasty-workspaces/personal/profile-default</>, hint: <>Use the Tasty that is already running, or start this one with a different <code style={lCode}>TASTY_HOME</code>.</> },
  webhook: { title: "Webhook port in use", body: <>The webhook port 7420 could not be opened: Address already in use (os error 48)</>, hint: <>Stop the app that is using port 7420, or start Tasty with <code style={lCode}>--webhook-port 7421</code>. <code style={lCode}>tasty --help</code> lists the other start options.</> },
};
function BootErrorFrame({ kind = "engine", theme, h = 480 }) {
  const e = BOOT_ERRORS[kind];
  return (
    <div style={{ display: "inline-block", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius-8)", overflow: "hidden" }}>
      <div {...(theme ? { "data-theme": theme } : {})} style={{ width: 640, height: h, background: "var(--tasty-bg-app)", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center" }}>
        <Lockup />
        <div style={{ marginTop: "var(--tasty-space-xl)", width: "var(--tasty-boot-form-width)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", minHeight: 0 }}>
          <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--tasty-space-sm)" }}>
            <span style={{ display: "inline-flex", flex: "none", marginTop: "var(--tasty-size-1)", color: "var(--tasty-boot-error-glyph)" }}><LIcon name="alertCircle" size="var(--tasty-icon-size-md)" /></span>
            <span style={{ fontSize: "var(--tasty-font-size-max)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)", lineHeight: "var(--tasty-line-height-ui)" }}>{e.title}</span>
          </div>
          <div style={{ fontSize: "var(--tasty-font-size-body)", color: "var(--tasty-text-secondary)", lineHeight: "var(--tasty-line-height-ui)", overflowWrap: "anywhere", textWrap: "pretty" }}>{e.body}</div>
          <div style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", lineHeight: "var(--tasty-line-height-ui)", textWrap: "pretty" }}>{e.hint}</div>
          <div style={{ display: "flex", justifyContent: "flex-end", marginTop: "var(--tasty-space-sm)" }}>
            <LButton variant="secondary">Quit</LButton>
          </div>
        </div>
      </div>
    </div>
  );
}

function Page() {
  return (
    <>
      <Section id="default" title="Default — 1280 × 720 (logical)">
        <Spec id="boot-screen-full-client-area-centered-stack" title="Boot screen — full client area, centered stack"
          when={<>Shown from the very first presented GPU frame until <b>Ready</b> (GpuInit → WaitingPlugins → RestoringLayout). App-background fill, no chrome (sidebar/tabs/status don't exist yet), non-interactive, short + variable, frame-drop tolerant. Frame shown at 60%; token dimensions are real values.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={1280} h={720} z={0.6} phase="Loading plugins…" />
          </Stage>
          <Meta
            specs={[
              ["surface", "client area = --tasty-bg-app solid (= GPU clear color)"],
              ["stack", "lockup → (space-xl 24) → spinner → (space-lg 16) → phase slot"],
              ["lockup", "64px mark · space-sm 8 · tasty. mono 38px · melon dot (no overlap — 2026-10-07)"],
              ["spinner", "32px · stroke 3 · accent-primary · track 0.22 · 900ms linear"],
              ["phase slot", "fixed --tasty-size-16 · font-size-body · text-muted"],
            ]}
            tokens={[
              { tok: "--tasty-bg-app", use: "full surface", color: "var(--tasty-bg-app)" },
              { tok: "--tasty-accent-primary", use: "spinner arc", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-brand-melon-flesh", use: "wordmark dot", color: "var(--tasty-brand-melon-flesh)" },
              { tok: "--tasty-spinner-duration", use: "900ms rotation" },
              { tok: "--tasty-space-xl / -lg", use: "stack gaps" },
            ]} />
          <Note><b>Non-interactive · frame-drop tolerant:</b> single constant-velocity arc, angle = f(time), so a 50–300ms atomic boot task (GPU init) that stalls the frame loop won't strobe or jump.</Note>
        </Spec>
      </Section>

      <Section id="minimal" title="Minimal window — 640 × 480 (true size)">
        <Spec id="same-stack-no-responsive-shrink" title="Same stack, no responsive shrink"
          when={<>Minimum window size. Stack elements, gaps, and spinner size are <b>identical</b> to the default — centered placement still has ample margin, so implementation needs one centered layout regardless of window size.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={640} h={480} z={1} phase="Restoring layout…" />
          </Stage>
        </Spec>
      </Section>

      <Section id="phases" title="Phase variants (3) + no-text">
        <Spec id="fixed-height-phase-slot-swap-omit-never-shifts-the-stack" title="Fixed-height phase slot — swap/omit never shifts the stack"
          when={<>Phase strings map 1:1 to the boot state machine and are confirmed in English → i18n keys verbatim (en/ko/ja). The slot always occupies --tasty-size-16 whether filled or empty, so the lockup + spinner never move; phases not all appearing (e.g. fresh install with no layout restore) is fine.</>}>
          <Stage variant="solo" style={{ display: "grid", gridTemplateColumns: "repeat(2, auto)", gap: "var(--tasty-space-lg)", justifyContent: "start", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={640} h={480} z={0.5} phase="Initializing graphics…" />
            <BootFrame w={640} h={480} z={0.5} phase="Loading plugins…" />
            <BootFrame w={640} h={480} z={0.5} phase="Restoring layout…" />
            <BootFrame w={640} h={480} z={0.5} showPhase={false} />
          </Stage>
          <Meta
            specs={[
              ["1 · GpuInit", "Initializing graphics…"],
              ["2 · WaitingPlugins", "Loading plugins…"],
              ["3 · RestoringLayout", "Restoring layout…"],
              ["no-text (§2-4)", "wordmark + spinner only; slot reserved, empty"],
            ]}
            tokens={[{ tok: "--tasty-size-16", use: "fixed phase-slot height" }, { tok: "--tasty-text-muted", use: "phase color", color: "var(--tasty-text-muted)" }]} />
        </Spec>
      </Section>

      <Section id="spinner" title="Spinner spec">
        <Spec id="spinner-jsx-reused-size-16-32-for-boot-hero" title="Spinner.jsx reused — size 16 → 32 for boot hero"
          when={<>Same visual vocabulary as <code>components/feedback/Spinner.jsx</code> (thin arc on a faint track). Only the size is bumped one step to a boot-hero 32px; no new spinner invented. reduced-motion freezes the arc (Spinner's built-in static fallback).</>}>
          <Stage variant="solo" style={{ display: "flex", gap: "var(--tasty-space-xl)", padding: "var(--tasty-space-xl)", background: "var(--tasty-bg-app)", alignItems: "center" }} eager>
            <span style={{ color: "var(--tasty-accent-primary-raised)" }}><Spinner size={32} stroke={3} label="boot spinner" /></span>
            <span style={{ color: "var(--tasty-accent-primary-raised)", display: "inline-flex" }}>
              <span className="tasty-spinner--dots" style={{ ["--tasty-spinner-size"]: "32px", width: 32, height: 32, display: "inline-block" }}><svg /></span>
            </span>
            <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", fontFamily: "var(--tasty-font-mono)" }}>animated · reduced-motion (3-dot fallback)</span>
          </Stage>
          <Meta
            specs={[
              ["size", "32px (catalog 16 → hero) · viewBox 24"],
              ["stroke", "3px · arc round cap"],
              ["arc / track", "90° accent-primary / full-circle currentColor @0.22"],
              ["rotation", "--tasty-spinner-duration 900ms linear ∞ (constant velocity)"],
              ["reduced-motion", "arc frozen / Spinner 3-dot fallback"],
            ]}
            tokens={[
              { tok: "--tasty-spinner-indicator", use: "= accent-primary", color: "var(--tasty-accent-primary-raised)" },
              { tok: "--tasty-spinner-track", use: "0.22 track" },
              { tok: "--tasty-spinner-duration", use: "900ms" },
            ]} />
        </Spec>
      </Section>

      <Section id="latte" title="Latte variant (theme-following)">
        <Spec id="follows-the-saved-theme-no-dark-flash-for-light-users" title="Follows the saved theme — no dark flash for light users"
          when={<>Settings are loaded by boot time, so the loading screen adopts the user's theme. Mocha is the default (above); this Latte frame is the same tokens resolved in the light theme — a light user never sees a dark flash.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={640} h={480} z={0.7} phase="Loading plugins…" theme="latte" />
          </Stage>
          <Note>Same token names, light values. The <code>--tasty-bg-app</code> fill doubles as the GPU clear color, so it must be read from the resolved theme (not hard-coded dark) on the implementing side.</Note>
        </Spec>
      </Section>
      <Section id="shellsetup" title="First-run shell setup">
        <Spec id="shell-path-form-quit-use-this-shell" title="Shell path form — Quit / Use this shell"
          when={<>Shown on the boot surface when no usable shell is configured (<code>render_shell_setup</code>). Same fill and lockup as the loading screen, then a 360-wide form: title, a sub that may wrap to two lines (body, line-height-ui; never truncated), mono path <b>Input</b>, a validation line (glyph + word), and a right-aligned button row. The confirm moves to the shared <b>Button primary</b> (md) — the hand-painted accent-success fill goes, there is no success variant — and is labelled by what it does, <b>Use this shell</b> (the untranslated "OK" goes; i18n key). <b>Quit</b> (was Cancel — it exits the app) is Button secondary. The validation line has one message per host verdict, and on <b>Windows</b> a warning caption names the missing Git Bash (2026-10-06). While the path is not an executable, the primary is <b>disabled</b> with the shared ink rule; the validation line says why.</>}>
          <Stage variant="solo" style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-lg)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <ShellSetupFrame os="win" check="empty" />
            <ShellSetupFrame os="win" check="missing" />
            <ShellSetupFrame os="win" check="notShell" />
            <ShellSetupFrame os="win" check="valid" />
            <ShellSetupFrame os="mac" check="notShell" />
            <ShellSetupFrame os="mac" check="valid" />
            <ShellSetupFrame os="win" check="missing" theme="latte" />
            <ShellSetupFrame os="mac" check="valid" theme="latte" />
          </Stage>
          <Meta
            specs={[["form width", "360 · --tasty-size-360"], ["stack", "lockup → (space-xl) → title · sub · input · validation · buttons (space-sm)"], ["title", "14 / 600 · text-primary"], ["validation (2026-10-06)", "empty → blank line, height reserved · No file at this path · Not a bash or zsh executable (danger) · Shell found (success)"], ["Git Bash notice", "Windows only · caption line alertTriangle + warning ink · between sub and Input · no box"], ["confirm", "Button primary md · Use this shell · disabled unless valid · Enter = confirm when valid"], ["cancel → Quit", "Button secondary md · labelled Quit because it exits the app"], ["no card", "the host's 440 card, 12px literal and 32 input go: control-height Input, caption type, size-360 form"]]}
            tokens={[{ tok: "--tasty-accent-success", use: "valid line", color: "var(--tasty-accent-success-raised)" }, { tok: "--tasty-accent-danger", use: "invalid line", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-state-disabled-fg", use: "disabled confirm ink", color: "var(--tasty-state-disabled-fg)" }]} />
        </Spec>
      </Section>
      <Section id="shutdown" title="Shutdown screen">
        <Spec id="same-surface-as-boot-shutdown-phases" title="Same surface as boot, shutdown phases"
          when={<>When quitting has work to wait for, the window shows the <b>boot surface again</b> — same fill, lockup, spinner and fixed phase slot, no chrome, non-interactive — with the shutdown phase in the slot. Values are identical to the boot screen; only the copy differs. If there is nothing to wait for, no frame is drawn and the window just closes.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={1280} h={720} z={0.6} phase="Saving layout…" />
          </Stage>
          <Stage variant="solo" style={{ display: "grid", gridTemplateColumns: "repeat(2, auto)", gap: "var(--tasty-space-lg)", justifyContent: "start", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootFrame w={640} h={480} z={0.5} phase="Saving layout…" />
            <BootFrame w={640} h={480} z={0.5} phase="Stopping background worker…" />
            <BootFrame w={640} h={480} z={0.5} phase="Closing surfaces…" />
            <BootFrame w={640} h={480} z={0.5} phase="Stopping plugins…" />
          </Stage>
          <Meta
            specs={[["surface · stack", "identical to boot (lockup → space-xl → spinner → space-lg → phase slot)"], ["1 · SavingLayout", "Saving layout…"], ["2 · ReclaimingBootWorker", "Stopping background worker…"], ["3 · ClosingSurfaces", "Closing surfaces…"], ["4 · StoppingPlugins", "Stopping plugins…"], ["nothing to wait for", "no frame — the window closes"], ["theme", "follows the saved theme, like boot"]]}
            tokens={[{ tok: "--tasty-bg-app", use: "full surface", color: "var(--tasty-bg-app)" }, { tok: "--tasty-accent-primary", use: "spinner arc", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-muted", use: "phase", color: "var(--tasty-text-muted)" }]} />
          <Note>No new tokens. Phase copy is new i18n (<code>shutdown.saving_layout</code> · <code>shutdown.reclaiming_boot_worker</code> · <code>shutdown.closing_surfaces</code> · <code>shutdown.stopping_plugins</code>); keep the trailing ellipsis character.</Note>
        </Spec>
      </Section>
      <Section id="booterror" title="Boot error screen — 2026-10-10 (b12)">
        <Spec id="could-not-start-engine-data-folder-in-use-webhook-port" title="Could not start — engine · data folder in use · webhook port"
          when={<>Shown instead of the app when Tasty cannot start. It is the <b>same boot stage</b> as the loading and first-run shell screens, so it takes the <b>same structure</b>: app-bg fill, the lockup, then a <b>360 form</b> (<span className="tok">--tasty-boot-form-width</span>). The diagnostic card goes (its 460 width, 120 × 34 Quit and 6px corner had no token). The title row leads with <b>alertCircle</b> in <span className="tok">--tasty-boot-error-glyph</span> (danger); the title itself is <b>text-primary</b> 14 / 600 like every other title — the glyph carries the tone, so danger is not repeated on the title and the button. Body (13, text-secondary) holds the sentence and the OS detail; guidance (caption, muted) says what to do, with CLI pieces as code runs. <b>Quit</b> is the shared <b>Button secondary</b> md, right-aligned, as on shell setup — there is no danger-fill variant and quitting is the only choice, not a destructive one. Esc, Enter and the window close all quit with code 1.</>}>
          <Stage variant="solo" style={{ display: "flex", flexWrap: "wrap", gap: "var(--tasty-space-lg)", padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)" }} eager>
            <BootErrorFrame kind="engine" />
            <BootErrorFrame kind="datadir" />
            <BootErrorFrame kind="webhook" />
            <BootErrorFrame kind="datadir" theme="latte" />
          </Stage>
          <Meta
            specs={[["structure", "lockup → (space-xl) → form 360 · gaps space-sm · no card, no shadow"], ["title row", "alertCircle 16 · boot-error-glyph → accent-danger · gap space-sm · title 14 / 600 text-primary, wraps"], ["body", "font-size-body · text-secondary · line-height-ui · wraps; paths and OS errors break anywhere (overflow-wrap: anywhere)"], ["guidance", "caption · text-muted · CLI pieces = code runs (ui-code-*)"], ["Quit", "Button secondary md · right · Enter / Esc / close = Quit (exit 1)"], ["long content", "never truncated · when the stack is taller than the window − 2 × space-xl, the lockup is dropped first; past that the form scrolls (standard scrollbar), Quit stays pinned under it"], ["theme", "follows the saved theme when it could be read; else Mocha"]]}
            tokens={[{ tok: "--tasty-boot-form-width", use: "→ size-360" }, { tok: "--tasty-boot-error-glyph", use: "→ accent-danger", color: "var(--tasty-boot-error-glyph)" }, { tok: "--tasty-text-secondary", use: "body", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "guidance", color: "var(--tasty-text-muted)" }, { tok: "--tasty-ui-code-bg", use: "CLI runs", color: "var(--tasty-ui-code-bg)" }]} />
          <Note>Copy is unchanged from the app (<code>boot_error.*</code>); the OS error and path in the samples are illustrative. The webhook guidance shown here is a sample of the longest case (three runs) — use the app's string.</Note>
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "loading",
  nav: NAV,
  head: {
  title: "Chrome · Startup",
  intro: "Explore startup screens through to readiness, first-run shell setup, shutdown, and boot errors. Compare the guidance and progress indicators at each stage.",
  
},
  content: <Page />,
};

export default function GalleryRoute() {
  return page.content;
}
