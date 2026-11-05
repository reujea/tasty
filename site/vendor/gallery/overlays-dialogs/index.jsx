import React from "react";
import { PageStates } from "../parts/specimen.jsx";
import { STATES } from "./states.jsx";
import { Kbd, Tag as DTag, Button as DButton } from "../../components/index.js";
import { ConvertSurfacePopup, ThemeNotFoundModal, DbInitErrorModal, NotificationsPopupHead } from "../../ui_kits/terminal/overlays/info_modal.jsx";
import { PermissionNoticeModal } from "../../ui_kits/terminal/overlays/macos_permissions.jsx";
import { ExplorerDeleteFrame as ExplorerDeleteFrame_ExplorerDeleteFrame, XD_ITEMS as ExplorerDeleteFrame_XD_ITEMS } from "../parts/explorer-delete-frame.jsx";
import { GalleryScrimGrounds } from "./scrim-grounds.jsx";
import { Backdrop, ApprovalFrame, FileHandlerFrame, FhFooter, PresetFrame, MarkdownOpenFrame, RenameFrame, CategoryEditFrame, CategoryDeleteFrame, TransferProgressFrame, TransferErrorFrame, ShellMock } from "../parts/overlay-frames.jsx";
import { Section, Spec, Stage, Meta, Note, Do, Dont } from "../parts/specimen.jsx";

const BASE = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;

// Tasty Gallery — Overlays · Dialogs (modal confirm/edit surfaces)
// One of the five Overlays sub-pages (dialogs · windows · popups · banners · tutorial). Frame components are shared from
// parts/overlay-frames.jsx (parts/overlay-frames.jsx); this file holds only the
// page's specimens + nav. See the other overlays-*.jsx for the rest.



// 2026-10-09 — "Script changed since registration" confirm (host popup script_changed_confirm).
const SCRIPT_CONFIRM_COPY = {
  en: ["Script changed since registration", "changed", "This script's contents changed after it was registered. Running it runs the new contents.", "Run anyway", "Cancel"],
  ko: ["등록 후 스크립트가 바뀌었습니다", "변경됨", "등록 뒤 스크립트 내용이 바뀌었습니다. 실행하면 바뀐 내용이 실행됩니다.", "그래도 실행", "취소"],
  ja: ["登録後にスクリプトが変更されました", "変更あり", "登録後にこのスクリプトの内容が変更されました。実行すると変更後の内容が実行されます。", "このまま実行", "キャンセル"],
};
function ScriptConfirmG({ lang = "en" }) {
  const [title, tag, body, run, cancel] = SCRIPT_CONFIRM_COPY[lang];
  return (
    <div style={{ width: "var(--tasty-size-360)", background: "var(--tasty-bg-panel)", border: "var(--tasty-border-width) solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", boxShadow: "var(--tasty-shadow-modal)", overflow: "hidden" }}>
      <div style={{ padding: "var(--tasty-space-md) var(--tasty-size-14)", display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)" }}>
        <div style={{ fontSize: "var(--tasty-font-size-body)", fontWeight: "var(--tasty-font-weight-semibold)", color: "var(--tasty-text-primary)" }}>{title}</div>
        <div style={{ fontFamily: "var(--tasty-font-mono)", fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>~/.config/tasty/scripts/deploy-preview-with-long-name.sh</div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-start", gap: "var(--tasty-space-xs)" }}>
          <DTag variant="warning">{tag}</DTag>
          <p style={{ margin: 0, fontSize: "var(--tasty-font-size-caption)", lineHeight: "var(--tasty-line-height-ui)", color: "var(--tasty-text-secondary)" }}>{body}</p>
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--tasty-space-sm)", marginTop: "var(--tasty-space-xs)" }}>
          <DButton variant="ghost" size="sm">{cancel}</DButton><DButton variant="primary" size="sm">{run}</DButton>
        </div>
      </div>
    </div>
  );
}


const NAV = [
  { id: "scrim", label: "Scrim & frame" },
  { id: "scrimgrounds", label: "Scrim over each ground" },
  { id: "approval", label: "Agent approval" },
  { id: "convert", label: "Convert surface" },
  { id: "filehandler", label: "File handler picker" },
  { id: "preset", label: "Apply preset" },
  { id: "markdown", label: "Markdown open" },
  { id: "rename", label: "Rename popup" },
  { id: "category", label: "Workspace category" },
  { id: "transfer", label: "Remote transfer" },
  { id: "permnotice", label: "Info modal shell" },
  { id: "scriptconfirm", label: "Script changed confirm" },
  { id: "permdelete", label: "Explorer delete permanently" },
];

function Page() {
  return (
    <>
<Section id="scrim" title="Scrim & frame — the shared recipe">
        <Spec id="every-overlay-sits-on-the-same-scrim" title="Every overlay sits on the same scrim"
          when={<>Modals are the <b>one place</b> the system uses shadow. The recipe: a <b>50% black scrim</b> (+1px blur) over the app, a frame of <b>--tasty-bg-panel</b> (or <b>--tasty-surface-raised</b> for the palette) with a <b>1px --tasty-border-strong</b> edge and a soft drop shadow. Click-scrim or <Kbd keys="Esc" /> dismisses.</>}>
          <Stage variant="solo center">
            <Backdrop height={300} blur>
              <div align="center" style={{ width: 320, background: "var(--tasty-bg-panel)", border: "1px solid var(--tasty-border-strong)",
                borderRadius: "var(--tasty-radius)", padding: 18, boxShadow: "var(--tasty-shadow-modal)", textAlign: "center" }}>
                <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 6 }}>This is the frame</div>
                <div style={{ fontSize: 12.5, color: "var(--tasty-text-muted)" }}>--tasty-bg-panel · 1px --tasty-border-strong · soft shadow · sits on a 50% scrim</div>
              </div>
            </Backdrop>
          </Stage>
          <Meta
            specs={[["scrim", "rgba(0,0,0,.5) + blur(1px)"], ["frame fill", <span className="tok">--tasty-bg-panel</span>], ["border", <>1px <span className="tok">--tasty-border-strong</span></>], ["shadow", "0 20px 60px / .55 (modals only)"], ["dismiss", <>scrim click · <span className="ic">Esc</span></>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-border-strong", use: "edge", color: "var(--tasty-border-strong)" }, { tok: "--tasty-radius", use: "4px corners" }]} />
          <Note>The two anchor positions: <b>centered</b> (dialogs, settings) and <b>top-aligned</b> (command palette, ~40px from top). Nothing else.</Note>
        </Spec>

        <Spec id="scope-a-surface-bound-popup-dims-its-surface-not-the-window" title="Scope — a surface-bound popup dims its surface, not the window"
          when={<>A popup that declares <b>surface scope</b> is centered inside <b>one surface</b>, so its scrim covers <b>that surface's rect and nothing else</b>. Dimming the whole window says “the application is blocked” when only one pane is; the sidebar, titlebar, status bar, the divider and the adjacent surface stay at full ink and keep reading as live. <b>Window-scope</b> popups (command palette, settings, plugins, every centered confirm) keep the full-window scrim — the choice follows the popup's declared scope, not its shape.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["settled — surface scope", "surface", "var(--tasty-accent-success-raised)"],
              ["rejected — window scrim for a surface popup", "window", "var(--tasty-accent-danger-raised)"],
              ["rejected — no scrim at all", "none", "var(--tasty-accent-danger-raised)"]].map(([label, scope, c]) => (
              <div key={scope} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: c }}>{label}</div>
                <ShellMock scope={scope} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["scrim rect", "the surface's OUTER rect (its own border included)"], ["excluded", "divider · pane tab strip · sidebar · titlebar · status bar · adjacent surfaces"], ["radius", "follows the surface's own radius (0 in the shell today)"], ["clip", "scrim AND the popup's shadow are clipped to the rect"], ["popup inset", "8px, so the shadow has room inside the clip"], ["fill", <span className="tok">--tasty-scrim-bg</span>], ["window scope", "unchanged — full-window scrim"], ["input blocking", "unchanged — dim ≠ block"]]}
            tokens={[{ tok: "--tasty-scrim-bg", use: "50% black, both scopes", color: "var(--tasty-scrim-bg)" }, { tok: "--tasty-shadow-modal", use: "the popup keeps its modal shadow" }, { tok: "--tasty-border-frame", use: "divider / surface edge, undimmed", color: "var(--tasty-border-frame)" }]} />
          <Note>The alpha does <b>not</b> change: one scrim value, two scopes. And the surface is still a <b>centered, scrim-backed</b> surface, so it keeps <b>shadow-modal</b> — narrowing the scrim does not make it a popover.</Note>
          <Dont><b>Don't</b> read this as an input rule. The scrim marks <b>what the popup belongs to</b>; which input it swallows is the host's existing contract and is unchanged here.</Dont>
        </Spec>

        <Spec id="both-split-directions-a-clamped-popup-and-a-child-picker" title="Both split directions, a clamped popup, and a child picker"
          when={<>The boundary has to read in every arrangement. Side-by-side and stacked splits show the divider staying lit between a dimmed and an undimmed surface. In a <b>narrow</b> surface the popup clamps to the rect (it never grows past its surface, and focus is never moved to win space). When a <b>child</b> picker opens from the popup, the scrim is drawn <b>once</b> — the parent shell stays fully painted under the child, which sits above it with its own shadow. Two shells, one scrim, no accumulated darkness.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["horizontal split", { scope: "surface", split: "h" }],
              ["narrow surface — popup clamped", { scope: "surface", narrow: true, clamp: true }],
              ["parent + child picker — one scrim", { scope: "surface", child: true }]].map(([label, props]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <ShellMock {...props} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["scrim count", "one per scope — never composited twice"], ["depth", "scrim z0 · parent shell z1 · child shell z2"], ["parent under child", "full ink, not dimmed further"], ["clamp", "popup shrinks to the surface rect, min 320 → surface width wins"], ["surface hidden", "popup shell AND scrim hide with it; both return on re-show"], ["host popups", "same rule when they declare a surface scope"]]}
            tokens={[{ tok: "--tasty-fp-popup-min-width", use: "picker floor, surface overrides it" }, { tok: "--tasty-scrim-bg", use: "drawn once", color: "var(--tasty-scrim-bg)" }, { tok: "--tasty-shadow-modal", use: "both shells" }]} />
          <Note><b>convert_surface</b> and any other host popup bound to a surface gain this scrim (they have none today); host popups with no target binding stay on the window scrim. Child-lifetime and draft policy are the host's, unchanged by this visual rule.</Note>
        </Spec>
      </Section>

<Section id="scrimgrounds" title="Scrim over each ground">
        {GalleryScrimGrounds && <GalleryScrimGrounds />}
      </Section>

      <Section id="approval" title="Agent approval">
        <Spec id="approval-popup-the-mauve-gate" title="Approval popup — the mauve gate"
          when={<>Shown when an agent requests a privileged or destructive action. The <b>mauve agent accent</b> in the header and tag makes it unmistakable. Show the <b>exact command</b>, the requesting plugin, the target surface ID, and the permission tags. Three responses: deny / allow once / always.</>}>
          <Stage variant="solo center"><Backdrop height={420}><ApprovalFrame /></Backdrop></Stage>
          <Meta
            specs={[["width", "440px"], ["header dot", <span className="tok">--tasty-accent-agent</span>], ["command", "mono on #000"], ["actions", "ghost / secondary / agent"]]}
            tokens={[{ tok: "--tasty-accent-agent", use: "agent gate", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-accent-danger", use: "destructive tag", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }]} />
          <Do><b>Do</b> name the actor and show the literal command. The user is granting authority — make the stakes legible.</Do>
        </Spec>
        <Spec id="danger-choice-tinted-fill-with-readable-ink" title="Danger choice — tinted fill with readable ink"
          when={<>An approval request can offer a <b>danger choice</b> (the option that does the destructive thing). It was painted as accent-danger × 0.18 in code, which fell under 4.5:1 (Mocha 3.18 today, Latte 3.92 once mixed in sRGB). It now has tokens: an <b>opaque</b> fill <span className="tok">--tasty-approval-danger-bg</span> = danger at the tint-fill 12% mixed into <b>bg-panel</b> (not into whatever is behind), a 1px <span className="tok">--tasty-approval-danger-border</span> in danger so the tone survives without colour-only ink, and <b>text-primary</b> label ink (<span className="tok">--tasty-approval-danger-fg</span>). The ink stays text-primary rather than danger red: red on the tint is 3.94 in Latte.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 16, flexWrap: "wrap" }}>
            {[["Mocha", "mocha", "9.18 : 1"], ["Latte", "latte", "5.80 : 1"]].map(([lab, th, cr]) => (
              <div key={lab} {...(th ? { "data-theme": th } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-surface-raised)", borderRadius: "var(--tasty-radius)", width: 300 }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{lab} · popup surface-raised · label {cr}</span>
                {[["Allow once", false], ["Always allow in this workspace", false], ["Allow and skip future checks", true]].map(([t, dz]) => (
                  <div key={t} style={{ height: "var(--tasty-control-height)", display: "flex", alignItems: "center", padding: "0 var(--tasty-space-md)", borderRadius: "var(--tasty-radius)", fontSize: "var(--tasty-font-size-body)",
                    background: dz ? "var(--tasty-approval-danger-bg)" : "var(--tasty-bg-panel)", color: dz ? "var(--tasty-approval-danger-fg)" : "var(--tasty-text-primary)",
                    border: "var(--tasty-border-width) solid " + (dz ? "var(--tasty-approval-danger-border)" : "var(--tasty-border-default)") }}>{t}</div>
                ))}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["fill", "color-mix(in srgb, accent-danger 12%, bg-panel) — opaque · Mocha (56,43,61) · Latte (236,214,222)"], ["label", "text-primary · Mocha 9.18 · Latte 5.80"], ["edge", "1px accent-danger · vs surface-raised Mocha 5.43 · Latte 3.52 (≥ 3 non-text)"], ["retired", "linear_multiply(0.18) in code"], ["hover", "the normal button hover overlay on top"]]}
            tokens={[{ tok: "--tasty-approval-danger-bg", use: "fill", color: "var(--tasty-approval-danger-bg)" }, { tok: "--tasty-approval-danger-border", use: "→ accent-danger", color: "var(--tasty-approval-danger-border)" }, { tok: "--tasty-approval-danger-fg", use: "→ text-primary", color: "var(--tasty-approval-danger-fg)" }]} />
          <Note>Contrast is WCAG 2.x relative luminance on the computed sRGB values. Choice labels in the sample are illustrative.</Note>
        </Spec>
      </Section>

<Section id="convert" title="Convert surface">
        <Spec id="convert-popup-pick-the-kind-to-become" title="Convert popup — pick the kind to become"
          when={<>A small list popup that converts a surface to another kind <b>without losing the running process or scrollback</b> — only the renderer changes. The shared popup title bar, then one <b>MenuItem</b> per kind the surface can become; click (or ↵ on the highlighted row) converts, × / Esc close. This is the product's shape and the target — the earlier 400-wide From / To dialog is retired. Width is <span className="tok">--tasty-convert-popup-width</span> (<b>240</b>, was a literal 200) and <b>scales with ui_scale</b> like every popup width; height = title bar + rows.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Mocha · en · ui_scale 1.0</span>
                <div style={{ zoom: 1 }}><ConvertSurfacePopup title="Surface Type" /></div>
              </div>
            <div data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Latte · en · ui_scale 1.0</span>
                <div style={{ zoom: 1 }}><ConvertSurfacePopup title="Surface Type" /></div>
              </div>
          </Stage>
          <Meta
            specs={[["width", "240 × ui_scale · --tasty-convert-popup-width (was literal 200)"], ["title", "shared PopupTitleBar (× only, reserve 32 per side)"], ["rows", "MenuItem 28 · icon + kind label · one per convertible kind"], ["actions", "click / ↵ converts · × / Esc close"]]}
            tokens={[{ tok: "--tasty-convert-popup-width", use: "→ size-240, × ui_scale" }, { tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-border-frame", use: "edge", color: "var(--tasty-border-frame)" }]} />
        </Spec>
        <Spec id="shortcut-chip-each-row-s-letter-as-a-trailing-keycap" title="Shortcut chip — each row's letter as a trailing keycap"
          when={<>While the popup is open, one bare letter converts to that row. The letter is the kind id's first latin letter, upper-cased (terminal T · markdown M · image I · html H · explorer E · dag D), so it never follows the translated label — the row shows it. It rides the existing MenuItem <b>shortcut</b> slot as the DS <b>Kbd</b> keycap, the same one the command palette uses: no new tokens. Display only; it takes no clicks. The keycap is the <b>same in every row state</b> — it already sits on its own raised fill, so it reads on rest, hover and active alike. A row whose letter collides with an earlier row has <b>no shortcut</b>: no chip, no reserved slot, the label runs to the end. The label ellipsises; the chip never shrinks. Width stays <span className="tok">--tasty-convert-popup-width</span> 240.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha", "1.0", "rest · hover (Image) · active (Markdown) · collision · long"], ["Latte", "latte", "1.0", "same states"]].map(([th, dt, sc, cap]) => (
              <div key={th} data-theme={dt || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{th} · en · {sc} — {cap}</span>
                <ConvertSurfacePopup title="Surface Type" active={0} hover={1}
                  kinds={[["markdown", "Markdown", "M"], ["image", "Image", "I"], ["html", "HTML", "H"], ["file", "Mermaid (plugin)", null], ["file", "Notebook viewer with outputs (plugin)", "N"]]} />
              </div>
            ))}
            {[["Mocha", "mocha", "ko", "서피스 타입 전환", [["terminal", "터미널", "T"], ["image", "이미지", "I"], ["gitTree", "DAG", "D"], ["html", "HTML", "H"], ["folder", "탐색기", "E"]]], ["Mocha", "mocha", "ja", "サーフェスタイプ切替", [["terminal", "ターミナル", "T"], ["markdown", "マークダウン", "M"], ["image", "画像", "I"], ["html", "HTML", "H"], ["folder", "エクスプローラー", "E"]]], ["Latte", "latte", "ja", "サーフェスタイプ切替", [["terminal", "ターミナル", "T"], ["markdown", "マークダウン", "M"], ["image", "画像", "I"], ["html", "HTML", "H"], ["folder", "エクスプローラー", "E"]]]].map(([th, dt, lang, t, k], n) => (
              <div key={n} data-theme={dt || undefined} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{th} · {lang} · {lang === "ko" ? "1.0" : "1.2 (240 × 1.2, fits)"}</span>
                <div style={{ zoom: lang === "ko" ? 1 : 1.2 }}><ConvertSurfacePopup title={t} kinds={k} active={0} /></div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["chip", "DS Kbd keycap in MenuItem shortcut slot · one latin capital"], ["letter", "kind id first letter, upper-case · never the label"], ["states", "rest = hover = active · keycap unchanged"], ["no shortcut", "collision → no chip, no reserved slot, label to row end"], ["overflow", "label ellipsis · chip flex none"], ["width", "240 × ui_scale — unchanged"], ["input", "display only · click / ↑↓ ↵ / letter / Esc unchanged"]]}
            tokens={[{ tok: "--tasty-kbd-size", use: "keycap height / min width" }, { tok: "--tasty-kbd-font-size", use: "letter" }, { tok: "--tasty-surface-raised", use: "keycap fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-strong", use: "keycap edge", color: "var(--tasty-border-strong)" }, { tok: "--tasty-text-secondary", use: "letter ink", color: "var(--tasty-text-secondary)" }]} />
          <Note>Shared rule for any "pick by one letter" list popup: show the letter as a trailing Kbd keycap in the MenuItem shortcut slot; rows without a letter drop the chip. Scope here is the convert popup only.</Note>
        </Spec>
        <Spec id="narrow-popup-long-title-width-scales-cut-titles-get-a-tooltip" title="Narrow popup, long title — width scales, cut titles get a tooltip"
          when={<>Every popup width is a token and is multiplied by ui_scale, the same as the title and the reserve. Scaling alone keeps the ratio, so the base width must fit the longest locale title at 1.0: the ja title <i>サーフェスタイプ切替</i> is ≈ 139, and 200 leaves a band of 200 − 2 × 32 = 136 — it cuts at every scale. The convert popup's base therefore goes to <b>240</b> (band 176), and at 1.2 everything scales together (288 / 211 / ≈ 167). A title that still overflows ellipsises inside the symmetric band (unchanged), and <b>only then</b> shows the full title in a Tooltip on hover (shared placement: top, then bottom). No content-driven widening: the width stays a token.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Mocha · ja · 1.2 — before: literal 200 (cuts)</span>
                <div style={{ zoom: 1.2, ["--tasty-convert-popup-width"]: "calc(200px / 1.2)" }}><ConvertSurfacePopup title="サーフェスタイプ切替" /></div>
              </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Mocha · ja · 1.2 — convert-popup-width 240 × 1.2 (fits)</span>
                <div style={{ zoom: 1.2 }}><ConvertSurfacePopup title="サーフェスタイプ切替" /></div>
              </div>
            <div data-theme="latte" style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Latte · ja · 1.2 — convert-popup-width 240 × 1.2 (fits)</span>
                <div style={{ zoom: 1.2 }}><ConvertSurfacePopup title="サーフェスタイプ切替" /></div>
              </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>Mocha · still too long — ellipsis + hover tooltip</span>
                <div style={{ zoom: 1 }}><ConvertSurfacePopup title="Convert this surface to another type" /></div>
              </div>
          </Stage>
          <Meta
            specs={[["width", "token × ui_scale — never a literal · base sized for the longest locale title at 1.0"], ["band", "width − 2 × reserve(N) · convert 240 → 176"], ["overflow", "ellipsis inside the band"], ["tooltip", "only when the title is cut · full title · top → bottom"], ["widen to fit", "no"]]}
            tokens={[{ tok: "--tasty-convert-popup-width", use: "240 × ui_scale" }, { tok: "--tasty-popup-title-btn-size", use: "reserve part" }]} />
          <Dont><b>Don't</b> keep a popup width as a literal outside the scale, and don't widen a popup to its title — the band follows the token width.</Dont>
        </Spec>
      </Section>

<Section id="filehandler" title="File handler picker">
        <Spec id="open-with-pick-a-handler" title="Open with… — pick a handler"
          when={<>Shown when a file can be opened more than one way. A <b>list of handlers</b> — built-in (preview / editor / pager) and <b>plugin-contributed</b> — each with an icon, name, and origin. The header carries the <b>path</b> and the <b>detected format</b>; the current default is tagged. Selected row uses the accent left-bar pattern.</>}>
          <Stage variant="solo center"><Backdrop height={380}><div align="center"><FileHandlerFrame /></div></Backdrop></Stage>
          <Meta
            specs={[["width", "420px"], ["header", "title + format Tag · path (mono 11)"], ["rows", "icon · name · origin"], ["default", "Tag accent"], ["selected", <>2px accent left bar</>], ["footer", "Cancel / Open — settled F1 (b), see “Footer — settled”"]]}
            tokens={[{ tok: "--tasty-surface-active", use: "selected", color: "var(--tasty-surface-active)" }, { tok: "--tasty-accent-primary", use: "select bar", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-agent", use: "plugin handler", color: "var(--tasty-accent-agent-raised)" }]} />
          <Note>Plugin handlers are marked with the mauve agent accent — you always know a third party is handling the open.</Note>
        </Spec>

        <Spec id="detected-format-one-tag-in-the-header" title="Detected format — one Tag in the header"
          when={<>The picker always says <b>what it thinks the file is</b>. The detector value (<code>markdown</code> · <code>html</code> · <code>$directory</code>) sits as an <b>accent Tag</b> on the title row, opposite the title; the path sits under it in mono. When detection fails the Tag reads <b>“format unknown”</b> in the neutral outline variant — an unknown format is a fact, not an error.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>detected</div>
              <FileHandlerFrame />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>not detected → fallback</div>
              <FileHandlerFrame state="fallback" />
            </div>
          </Stage>
          <Meta
            specs={[["place", "title row, right-aligned"], ["detected", <>Tag <span className="tok">accent</span></>], ["unknown", "Tag default (outline)"], ["path", <>mono 11 · <span className="tok">--tasty-text-muted</span></>], ["long path", "elide at the FRONT in the model, then render LTR"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "format Tag", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-font-mono", use: "path" }, { tok: "--tasty-text-muted", use: "path ink", color: "var(--tasty-text-muted)" }]} />
          <Do><b>Do</b> cut long paths at the <b>front</b> — the filename is the tail and the tail identifies the file. Elide in the <b>model</b> (drop leading segments, prefix <code>…/</code>) and render the result left-to-right. <b>Don't</b> reach for <code>direction: rtl</code> + <code>text-overflow</code>: it reorders the run and clips the informative end.</Do>
        </Spec>

        <Spec id="recent-a-second-group-in-the-same-list" title="Recent — a second group in the same list"
          when={<>The picker shows two kinds of handler: <b>Suggested</b> (matched to this format) and <b>Recent</b> (the user's last picks, all formats, LRU cap 10). They live in <b>one 420px list</b> separated by a rule and a group label, not in two columns — at 420px a column can't hold icon + name + origin. The Recent group carries a one-line caption saying it is <b>not matched to this format</b>, and its rows show <b>when</b> they were used instead of a bare origin. Selection is single across both groups.</>}>
          <Stage variant="solo center"><Backdrop height={760}><div align="center"><FileHandlerFrame state="recent" /></div></Backdrop></Stage>
          <Meta
            specs={[["groups", "Suggested → rule → Recent"], ["group label", "11px uppercase, +0.06em, text-secondary"], ["count", "mono, next to the label"], ["caption", "11px muted, one line"], ["recent meta", "origin · id · when"], ["when column", <>reserved <span className="tok">--tasty-fh-when-width</span> (56) — never shrinks, never drops</>], ["selection", "one row across BOTH groups"]]}
            tokens={[{ tok: "--tasty-separator", use: "group rule", color: "var(--tasty-separator)" }, { tok: "--tasty-text-secondary", use: "group label", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-text-muted", use: "caption / time", color: "var(--tasty-text-muted)" }, { tok: "--tasty-fh-when-width", use: "when column reserve" }]} />
          <Dont><b>Don't</b> split the frame into two side-by-side lists. Two 200px columns lose the origin line, and the eye has to compare across a gutter to pick one thing.</Dont>
        </Spec>

        <Spec id="recent-the-relative-time-vocabulary-settled-t1-t4" title="Recent — the relative-time vocabulary (settled T1–T4)"
          when={<>The <code>when</code> fragment is <b>six buckets and nothing else</b>. Under a minute it says so; under a day it counts minutes then hours; the second day is a word; up to a week it counts days; <b>past a week it stops counting and shows the date</b>. A <code>23d ago</code> does not help anyone pick a handler, and the ISO-style date needs no ko/ja string and no plural rule. The column is <b>reserved at the widest member</b> (the date, 10 mono chars) so the popup — which recomputes the bucket every frame — never reflows the id when a row crosses a boundary while open. When the line is short of room the <b>id yields</b>: it already elides at the front; a partially clipped time would read as a different time.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 6, alignItems: "stretch" }}>
            {[["< 60 s", "just now", "—"],
              ["< 60 min", "{n}m ago", "12m ago"],
              ["< 24 h", "{n}h ago", "2h ago"],
              ["24–48 h", "yesterday", "—"],
              ["2–7 d", "{n}d ago", "4d ago"],
              ["≥ 7 d", "YYYY-MM-DD", "2026-08-30"]].map(([span, en, sample]) => (
              <div key={span} style={{ display: "flex", alignItems: "center", gap: 12, padding: "6px 10px", background: "var(--tasty-bg-panel)",
                border: "1px solid var(--tasty-border-default)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ flex: "none", width: 80, fontFamily: "var(--tasty-font-mono)", fontSize: 11, color: "var(--tasty-text-muted)" }}>{span}</span>
                <span style={{ flex: "none", width: 120, fontFamily: "var(--tasty-font-mono)", fontSize: 12, color: "var(--tasty-text-primary)" }}>{en}</span>
                <span style={{ flex: "none", width: "var(--tasty-fh-when-width)", fontSize: 11, color: "var(--tasty-text-muted)", background: "var(--tasty-surface-raised)", padding: "2px 0", textAlign: "left", paddingLeft: 4, boxSizing: "border-box" }}>{sample === "—" ? en : sample}</span>
                <span style={{ flex: 1, fontSize: 11, color: "var(--tasty-text-muted)" }}>{span === "≥ 7 d" ? "absolute date — locale-neutral, 10 chars, the column's reserve" : span === "< 60 s" ? "the only non-numeric bucket besides yesterday" : ""}</span>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["T1 buckets", "6 — the proposed 5 plus a 7-day ceiling"], ["T2 en", "approved as proposed for the first five"], ["T3 ceiling", "≥ 7 d → YYYY-MM-DD, never {n}w / {n}mo"], ["T4 priority", "when never shrinks or drops; the id elides first"], ["reserve", <><span className="tok">--tasty-fh-when-width</span> = 56 — 10 D2Coding chars @ 11px (55) up to the grid</>], ["live update", "allowed — the reserve makes a bucket change reflow-free"], ["ko / ja", "translate the five words / patterns; the date bucket has no string"], ["n", "integer floor; 1m / 1h / 1d singular forms are the same pattern"]]}
            tokens={[{ tok: "--tasty-fh-when-width", use: "when column" }, { tok: "--tasty-text-muted", use: "when ink", color: "var(--tasty-text-muted)" }]} />
          <Dont><b>Don't</b> freeze the value at open time to avoid churn. The reserve already makes churn invisible to layout, and a picker left open across midnight should say <code>yesterday</code>.</Dont>
        </Spec>

        <Spec id="header-path-where-the-front-cut-lands-settled-t5" title="Header path — where the front cut lands (settled T5)"
          when={<>The header's path line is one mono-11 row inside the 420 frame: <b>390px</b> after the 1px frame borders and the 14px insets (420 − 2 − 28), which is <b>65 characters</b> at this font: D2Coding 11px is 5.56px nominal, but each glyph advances a whole <b>6px</b> once rounded, so 390 / 6 = 65. The cut is a <b>measure, not a count</b>: drop whole leading segments and prefix <code>…/</code> until the remainder fits the line box. The 65-char figure is the derived cap for a path that cannot be measured. The one time a cut falls mid-segment is a single segment longer than the whole line — then the filename itself elides at the front, by characters. The 48-char provisional cap was cutting paths that fit by a wide margin.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["65 chars — fits exactly, no cut", "packages/design-system/src/components/navigation/federated/Bar.ts"],
              ["93 → 57 chars — leading segments dropped at a boundary", "…/navigation/federation/SidebarCategoryHeaderContrast.tsx"],
              ["one 72-char segment → 65 — mid-segment, front", "…on-federation-sidebar-category-header-contrast-exploration-v2.md"]].map(([label, p]) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>{label}</div>
                <FileHandlerFrame path={p} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["line box", "390 = 420 − 1 × 2 (frame border) − 14 × 2"], ["cap", "65 mono chars @ 11px (390 / 6 — rounded advance, D2Coding)"], ["was", "70 / 5.5 nominal — overran the line by ~30px (2026-10-06)"], ["rule", "measure first; the count is the fallback"], ["boundary", "segment — drop whole leading segments, prefix …/"], ["mid-segment", "only when one segment exceeds the line; then front-elide by characters, prefix …"], ["row id", "unchanged — 34 chars, front, the id line is narrower"], ["was", "48 chars provisional — retired"]]}
            tokens={[{ tok: "--tasty-font-mono", use: "path" }, { tok: "--tasty-text-muted", use: "path ink", color: "var(--tasty-text-muted)" }]} />
        </Spec>

        <Spec id="no-suggestions-the-whole-catalog-one-time-only" title="No suggestions — the whole catalog, one time only"
          when={<>When nothing matches, the list falls back to <b>All handlers</b> — same rows, different promise. Three signals carry the difference: the group label reads <b>All handlers</b> in the <b>attention</b> tone with the caption “No handler matches this format.”, the header Tag reads <b>format unknown</b>, and a strip under the header states the consequence — the pick is <b>one-time</b> and this screen returns next time. Distinct from the empty state: here there is something to choose.</>}>
          <Stage variant="solo center"><Backdrop height={470}><div align="center"><FileHandlerFrame state="fallback" /></div></Backdrop></Stage>
          <Meta
            specs={[["label", <>All handlers · <span className="tok">--tasty-accent-attention</span></>], ["strip", <>8/14 on <span className="tok">--tasty-surface-raised</span></>], ["strip icon", "alertTriangle, attention"], ["promise", "one-time dispatch, no registration"]]}
            tokens={[{ tok: "--tasty-accent-attention", use: "fallback tone", color: "var(--tasty-accent-attention-raised)" }, { tok: "--tasty-surface-raised", use: "notice strip", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-text-secondary", use: "strip copy", color: "var(--tasty-text-secondary)" }]} />
          <Note>Attention-peach, not warning-yellow: nothing is wrong, the system just has no opinion.</Note>
        </Spec>

        <Spec id="empty-nothing-to-pick-from" title="Empty — nothing to pick from"
          when={<>No handler is registered anywhere in the system. The list area is replaced by a centered block — muted file glyph, <b>“No handlers registered.”</b>, one line of instruction, and a <b>secondary button into Settings › Handlers</b>. <b>Open</b> stays disabled; Cancel still works. This is the only state where the frame offers an <b>exit to somewhere else</b>.</>}>
          <Stage variant="solo center"><Backdrop height={340}><div align="center"><FileHandlerFrame state="empty" /></div></Backdrop></Stage>
          <Meta
            specs={[["block", "32/14 padding, centered, 8px gap"], ["glyph", <><span className="tok">--tasty-text-disabled</span></>], ["action", "Button secondary sm → Settings › Handlers"], ["Open", "disabled"]]}
            tokens={[{ tok: "--tasty-text-disabled", use: "empty glyph", color: "var(--tasty-text-disabled)" }, { tok: "--tasty-text-muted", use: "instruction", color: "var(--tasty-text-muted)" }]} />
          <Do><b>Do</b> keep the frame the same width and the footer intact — the empty state is the same dialog, not a different one.</Do>
        </Spec>

        <Spec id="long-list-cap-the-height-show-the-cut" title="Long list — cap the height, show the cut"
          when={<>Past ten rows the list <b>scrolls inside a 264px area</b> (≈8 rows) so the frame never outgrows a short window: header + 264 + footer ≈ 380px. Two affordances mark it — the <b>count next to the group label</b> tells you how many there are, and a <b>20px fade</b> to the panel fill at the bottom edge shows the list continues. No scroll shadow, no custom scrollbar.</>}>
          <Stage variant="solo center"><Backdrop height={430}><div align="center"><FileHandlerFrame state="long" /></div></Backdrop></Stage>
          <Meta
            specs={[["list max-height", "264px (~8 rows)"], ["frame max", "≈380px total"], ["overflow", "auto, list area only"], ["fade", <>20px → <span className="tok">--tasty-bg-panel</span></>], ["count", "mono, in the group label"]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "fade target", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-separator", use: "header / footer rules", color: "var(--tasty-separator)" }]} />
          <Note>Header and footer never scroll — the path, the format, and the actions stay put.</Note>
        </Spec>

        <Spec id="one-header-not-two" title="One header, not two"
          when={<>The picker is <b>headless</b> (no common popup titlebar): the frame draws its own header so the path appears <b>once</b>, in mono, truncated from the front. Using the shared titlebar puts the path in two places at two truncations — the reading below. Same precedent as Transfer progress.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-accent-success-raised)" }}>headless — one header</div>
              <FileHandlerFrame />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-accent-danger-raised)" }}>titlebar + header — path twice</div>
              <FileHandlerFrame headless={false} state="long" />
            </div>
          </Stage>
          <Meta
            specs={[["mode", <>headless: <span className="tok">true</span></>], ["header owner", "the frame"], ["dismiss", <>Esc · Cancel · titlebar X — <b>not</b> scrim click</>], ["precedent", "Transfer progress"]]}
            tokens={[{ tok: "--tasty-bg-sidebar", use: "titlebar fill (the rejected option)", color: "var(--tasty-bg-sidebar)" }, { tok: "--tasty-separator", use: "header rule", color: "var(--tasty-separator)" }]} />
          <Dont><b>Don't</b> pair the common titlebar with an in-frame header. The path gets cut two different ways in one dialog.</Dont>
        </Spec>

        <Spec id="footer-settled-cancel-open-nothing-else" title="Footer — settled: Cancel / Open, nothing else"
          when={<>The footer's “Always open .md with this” checkbox is <b>dropped</b> (reading <b>b</b>). The picker is a <b>pure dispatcher</b>: it opens one file, one time, and stores nothing. A checkbox that writes a persistent format→handler binding needs a place to <b>see and undo</b> that binding — Settings › Handlers has no such row today — and a checkbox that writes nothing is a lie. Every specimen on this page now carries the two-button footer.</>}>
          <Stage variant="solo" style={{ padding: 20, background: "var(--tasty-bg-app)", flexDirection: "column", gap: 14 }}>
            {[["settled — Cancel / Open, every pick is one-time", "actions"],
              ["rejected (a) — persists a binding with nowhere to undo it", "checkbox"],
              ["rejected (c) — an affordance that does nothing", "disabled"]].map(([label, v], i) => (
              <div key={v} style={{ display: "flex", flexDirection: "column", gap: 6, width: 420 }}>
                <div style={{ fontSize: 11, color: i === 0 ? "var(--tasty-accent-success-raised)" : "var(--tasty-text-muted)" }}>{label}</div>
                <div style={{ border: "1px solid var(--tasty-border-strong)", borderRadius: "var(--tasty-radius)", background: "var(--tasty-bg-panel)", overflow: "hidden" }}>
                  <FhFooter variant={v} />
                </div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["footer", "Cancel (ghost) · Open (primary)"], ["footer height", "44px (8/14 padding, 28px controls)"], ["persistence", "none — dispatch is one-time"], ["default binding", "lives in Settings › Handlers, set there"], ["Open", "disabled until a row is selected"]]}
            tokens={[{ tok: "--tasty-separator", use: "footer rule", color: "var(--tasty-separator)" }, { tok: "--tasty-accent-primary", use: "Open", color: "var(--tasty-accent-primary-raised)" }]} />
          <Note>If a persistent “always open with” ever ships, it belongs in <b>Settings › Handlers</b> as a visible, removable row — not as a checkbox on a transient dialog.</Note>
        </Spec>

        <Spec id="rows-the-icon-and-the-name-are-derived-not-stored" title="Rows — the icon and the name are derived, not stored"
          when={<>The handler model is <code>{"{ id, detector, priority, owner, action, disabled }"}</code> — <b>no icon field and no display-name field</b>. Both are derived, and both stay legible when a plugin declares neither. <b>Icon</b>: the glyph of the <b>surface kind the action opens</b> (markdown → <code>markdown</code>, editor → <code>edit</code>, pager → <code>terminal</code>, directory → <code>folder</code>, binary → <code>layers</code>, table → <code>columns</code>), falling back to <code>file</code> for an unknown kind — a plugin-declared icon wins only if it names a glyph in <code>icons.json</code>. <b>Name</b>: the id's <b>local segment</b> after the last <code>/</code>, rendered in mono when it is the raw id and in the UI font when a name was declared, so you can see which you are reading. The second line always carries <b>origin + the full id</b>, front-elided — the id appears exactly once per row.</>}>
          <Stage variant="solo center"><Backdrop height={380}><div align="center"><FileHandlerFrame state="mixed" /></div></Backdrop></Stage>
          <Meta
            specs={[["icon", "from action surface kind → icons.json"], ["icon fallback", <code>file</code>], ["name", "id segment after the last “/”"], ["name (raw id)", "mono — signals “no declared name”"], ["id line", "origin · id (mono 11), front-elided at 34 chars"], ["origin words", "built-in · you · plugin"], ["plugin", <>mauve <span className="tok">--tasty-accent-agent</span> on the origin word + glyph</>]]}
            tokens={[{ tok: "--tasty-accent-agent", use: "plugin origin", color: "var(--tasty-accent-agent-raised)" }, { tok: "--tasty-font-mono", use: "id + undeclared names" }, { tok: "--tasty-text-muted", use: "id line", color: "var(--tasty-text-muted)" }]} />
          <Do><b>Do</b> elide a long id at the <b>front</b>. <code>net.example.enterprise.documents…</code> repeats across a vendor's handlers; the tail is what tells them apart — the same rule the header path uses.</Do>
          <Note>The mauve stays on the <b>origin word and the glyph</b>, not on the name — a plugin's handler is still named after what it does.</Note>
        </Spec>
        <Spec id="default-tag-and-what-the-picker-means-now" title="Default Tag, and what the picker means now"
          when={<>The picker no longer appears when one handler clearly wins — the implementation dispatches the top match directly. So it opens in three cases only: <b>ambiguous candidates</b>, <b>no match</b>, or an <b>explicit “Open with…”</b>. The <b>default</b> Tag is kept for the first and third: in an ambiguous list it names the handler that would have run, and on an explicit open it tells the user what they are overriding. In the <b>fallback</b> state nothing is default, so no Tag appears. Interaction contract below.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>ambiguous / explicit — default Tag shown</div>
              <FileHandlerFrame />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>no match — no default</div>
              <FileHandlerFrame state="fallback" />
            </div>
          </Stage>
          <Meta
            specs={[["single click", "select, frame stays"], ["double click", "select + open"], ["no selection", "Open disabled"], ["selection scope", "one row across all groups"], ["dismiss", "Esc · Cancel · titlebar X"], ["scrim click", "does NOT close"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "default Tag fill", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-text-on-accent", use: "default Tag ink", color: "var(--tasty-text-on-accent)" }]} />
          <Note>Keep the Tag. It answers “what happens if I just press Enter” — the question the picker exists to ask.</Note>
        </Spec>
      </Section>

      <Section id="preset" title="Apply preset">
        <Spec id="apply-preset-scope-then-pick" title="Apply preset — scope then pick"
          when={<>Applies a saved layout preset at one of three scopes — <b>Workspace / Tab / Pane</b> — chosen with a segmented control in the header. Below it, a selectable list of presets with a mono summary of what each lays out. The confirm button names the active scope (“Apply to workspace”).</>}>
          <Stage variant="solo center"><Backdrop height={340}><div align="center"><PresetFrame /></div></Backdrop></Stage>
          <Meta
            specs={[["width", "440px"], ["scope", "Workspace · Tab · Pane (segmented)"], ["rows", "name + mono summary"], ["confirm", "names the scope"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "active scope / select", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-active", use: "chosen preset", color: "var(--tasty-surface-active)" }, { tok: "--tasty-font-mono", use: "layout summary" }]} />
        </Spec>
      </Section>

<Section id="markdown" title="Markdown open">
        <Spec id="markdown-open-preview-vs-raw" title="Markdown open — preview vs raw"
          when={<>A two-choice confirm shown when opening a <code>.md</code> file: <b>rendered preview</b> or <b>raw text</b> in the editor. Two equal card-buttons with the default pre-selected (accent ring), filename in mono, cancel + confirm. The pattern for any “open as A or B” fork.</>}>
          <Stage variant="solo center"><Backdrop height={300}><div align="center"><MarkdownOpenFrame /></div></Backdrop></Stage>
          <Meta
            specs={[["width", "420px"], ["choices", "2 equal card-buttons"], ["default", "accent ring + fill"], ["confirm", "names the choice"]]}
            tokens={[{ tok: "--tasty-accent-primary", use: "selected choice", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-surface-raised", use: "choice fill", color: "var(--tasty-surface-raised)" }, { tok: "--tasty-border-default", use: "unselected edge" }]} />
        </Spec>
      </Section>

<Section id="rename" title="Rename popup">
        <Spec id="rename-the-smallest-dialog" title="Rename — the smallest dialog"
          when={<>A focused single-field dialog for workspace / tab rename. Title, a one-line keyboard hint, an autofocused Input, cancel/confirm. The template for any "edit one value" modal.</>}>
          <Stage variant="solo center"><Backdrop height={260}><RenameFrame /></Backdrop></Stage>
          <Meta
            specs={[["width", "360px"], ["field", "autofocus, full-width"], ["confirm", <span className="ic">↵</span>], ["padding", <span className="tok">--tasty-space-md</span>]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-accent-primary", use: "confirm", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-border-focus", use: "field ring", color: "var(--tasty-border-focus)" }]} />
        </Spec>
      </Section>

<Section id="category" title="Workspace category">
        <Spec id="new-rename-category-the-rename-dialog-reused" title="New / rename category — the Rename dialog, reused"
          when={<>Creating and renaming a workspace category reuses the <b>360px single-field Rename dialog</b> verbatim — title, keyboard hint, one autofocused Input, cancel/confirm. Name validation runs on confirm and shows an inline error under the field (the confirm button disables): <b>empty</b> after trim, the reserved word <b>“normal”</b>, or a <b>case-insensitive duplicate</b>.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>new — empty field</div>
              <CategoryEditFrame mode="new" />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>validation — duplicate name</div>
              <CategoryEditFrame mode="new" error="duplicate" />
            </div>
          </Stage>
          <Meta
            specs={[["width", "360px (Rename dialog)"], ["field", "autofocus, full-width"], ["confirm", <span className="ic">↵</span>], ["errors", "empty · reserved ‘normal’ · duplicate"], ["invalid", "inline msg + confirm disabled"]]}
            tokens={[{ tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-accent-primary", use: "confirm", color: "var(--tasty-accent-primary-raised)" }, { tok: "--tasty-accent-danger", use: "error field + text", color: "var(--tasty-accent-danger-raised)" }]} />
          <Note>Error copy maps to the existing keys <code>workspace_category.error.empty / reserved / duplicate</code>. No new dialog chrome — this is the Rename template with a validation line.</Note>
        </Spec>
        <Spec id="delete-category-destructive-confirm" title="Delete category — destructive confirm"
          when={<>“Delete category” never deletes immediately — a small centered confirm on the scrim gates it. The action is <b>destructive</b> (danger button + danger glyph). The body states the safe outcome: the category's <b>workspaces are not deleted</b>; they move back to <b>normal</b> (“Workspaces”). Small-modal tone, matching the Rename dialog footprint.</>}>
          <Stage variant="solo center"><Backdrop height={260}><div align="center"><CategoryDeleteFrame /></div></Backdrop></Stage>
          <Meta
            specs={[["width", "380px"], ["trigger", "Delete category (menu/popup)"], ["tone", <>destructive — <span className="tok">--tasty-accent-danger</span></>], ["body", "workspaces move to normal (not deleted)"], ["actions", "Cancel / Delete category"]]}
            tokens={[{ tok: "--tasty-accent-danger", use: "glyph + confirm button", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }, { tok: "--tasty-text-on-accent", use: "button label", color: "var(--tasty-text-on-accent)" }]} />
          <Do><b>Do</b> reassure in the body: reversible-feeling copy (“workspaces move back to Workspaces”) lowers the stakes of an irreversible-sounding verb.</Do>
        </Spec>
      </Section>

      <Section id="transfer" title="Remote transfer">
        <Spec id="transfer-progress-auto-closing-status-popup" title="Transfer progress — auto-closing status popup"
          when={<>Shown while a file is being received from an attached remote workspace over the bulk channel (begin/chunk/commit). A <b>PopupDef</b> frame on the shared scrim, headless header: download glyph · <b>Receiving file</b> · a right-aligned mono <b>percent</b>. Body: the <b>filename</b> (mono, middle of the frame, ellipsized), the system's first <b>determinate progress bar</b> — a recessed 4px track with an accent fill, <b>no animation</b> (the fill tracks bytes; motion stays 0ms) — and a mono stats line: <b>transferred / total</b> left, rate right. It <b>auto-closes on completion</b>; the single ghost <b>Cancel</b> aborts the transfer. It does <b>not</b> close on outside click — dismissing progress by mis-click would read as an abort.</>}>
          <Stage variant="solo center"><Backdrop height={300}><div align="center"><TransferProgressFrame /></div></Backdrop></Stage>
          <Meta
            specs={[["width", <span className="tok">--tasty-transfer-popup-width</span>], ["header", "download glyph · title · mono %"], ["filename", "mono 13 · ellipsized"], ["bar", "4px track + accent fill · no animation"], ["stats", "done / total · rate (mono, muted)"], ["close", "auto on completion · Cancel aborts"], ["outside click", "does not dismiss"]]}
            tokens={[{ tok: "--tasty-progress-track-bg", use: "recessed track", color: "var(--tasty-progress-track-bg)" }, { tok: "--tasty-progress-fill-bg", use: "determinate fill", color: "var(--tasty-progress-fill-bg)" }, { tok: "--tasty-progress-height", use: "4px thickness" }, { tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }]} />
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["ui_scale 0.85", 0.85], ["ui_scale 1", 1], ["ui_scale 1.2", 1.2]].map(([cap, z]) => (
              <div key={cap} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap} — width and insets scale together</span>
                <div style={{ zoom: z }}><TransferProgressFrame /></div>
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["scale (2026-10-06)", "ON-SCALE: width AND every inset below multiply by ui_scale"], ["header", "pad-y transfer-header-pad-y 12 · pad-x transfer-pad-x 14 · gap space-sm"], ["body", "padding transfer-pad-x 14 · block gap transfer-body-gap 10"], ["footer", "pad-y transfer-footer-pad-y 10 · pad-x 14"], ["reason well", "transfer-well-pad-y 8 · transfer-well-pad-x 10"], ["line heights", "not tokens — each line = its font-size × line-height-ui (header row = max(glyph md, title line))"]]}
            tokens={[{ tok: "--tasty-transfer-pad-x", use: "14 inset" }, { tok: "--tasty-transfer-header-pad-y", use: "→ space-md 12" }, { tok: "--tasty-transfer-body-gap", use: "→ size-10" }, { tok: "--tasty-transfer-footer-pad-y", use: "→ size-10" }, { tok: "--tasty-transfer-well-pad-y", use: "→ space-sm 8" }, { tok: "--tasty-transfer-well-pad-x", use: "→ size-10" }]} />
          <Note>Rate / time-remaining is <b>optional</b> — filename + progress are the contract; drop the right stat when the channel can't estimate it. Multi-file transfers reuse this frame with one filename+bar row per file, newest last — same header, one Cancel for the batch.</Note>
          <Do><b>Do</b> keep the bar still — the fill moves only when bytes land. A shimmering or eased bar would be the only animated chrome in a 0ms-motion terminal.</Do>
        </Spec>
        <Spec id="transfer-failed-rejected-mid-transfer-error" title="Transfer failed — rejected & mid-transfer error"
          when={<>Shown when a transfer is <b>rejected before it starts</b> (capacity exceeded) or <b>fails mid-transfer</b> (channel dropped). Danger glyph + <b>Transfer failed</b> title; the body names the <b>file</b> (mono) and quotes the <b>reason</b> verbatim in a mono well — the same command-well pattern as agent approval, so the machine-reported cause is legible and copyable. Rejected → a single <b>Dismiss</b>; mid-transfer failure → <b>Retry</b> (secondary) beside a ghost Dismiss — retry is only offered when re-sending can succeed.</>}>
          <Stage variant="solo center" style={{ padding: 20, background: "var(--tasty-bg-app)", gap: 18, flexWrap: "wrap", alignItems: "flex-start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>rejected — capacity exceeded, no retry</div>
              <TransferErrorFrame />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div style={{ fontSize: 11, color: "var(--tasty-text-muted)" }}>failed mid-transfer — retry offered</div>
              <TransferErrorFrame retry reason="connection lost — remote workspace detached at 34.6 MiB" />
            </div>
          </Stage>
          <Meta
            specs={[["width", <span className="tok">--tasty-transfer-popup-width</span>], ["header", "danger glyph · Transfer failed"], ["reason", "verbatim, mono well on bg-app"], ["rejected", "Dismiss only"], ["failed", "Dismiss (ghost) · Retry (secondary)"], ["dismiss", "button · Esc · scrim click"]]}
            tokens={[{ tok: "--tasty-accent-danger", use: "glyph + reason text", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-bg-app", use: "reason well", color: "var(--tasty-bg-app)" }, { tok: "--tasty-bg-panel", use: "frame", color: "var(--tasty-bg-panel)" }]} />
          <Dont><b>Don't</b> use a danger-filled button here — danger fill is reserved for confirming a destructive act (delete). This popup only acknowledges one; Dismiss stays secondary/ghost.</Dont>
          <Note>Capacity rejection points at its own cure: the reason names the configured limit, which lives in <b>Settings › General › Remote transfer</b> (Overlays › Windows).</Note>
        </Spec>
      </Section>

      <Section id="permnotice" title="Info modal shell — notice queue">
        <Spec id="one-shell-for-every-queued-message" title="One shell for every queued message"
          when={<>Boot and runtime notices share one queue and one <b>shell</b>, shown one at a time. The shell rules apply to <b>every</b> message: <b>440</b> wide, <b>140–360</b> tall; the <b>title sits in the popup title bar</b> (fixed strip, 14 / 600, 1px rule below, always drawn); body <b>13 · text-secondary</b>, paragraph gap 12, scrolls when it overflows; a <b>1px scroll edge</b> above the button row only while content is hidden below; buttons are the DS <b>Button</b>, right-aligned, dismiss = <b>Primary, rightmost</b>. Bodies are <b>plain text</b> — emphasis (paths, lead-ins, command chip) is authored by the permissions notice only. Below: a one-sentence message and the one message that ends the app.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Theme not found — one sentence, min height", "theme", null], ["Database initialization error — quits", "db", null], ["Theme not found — Latte", "theme", "latte"], ["Database initialization error — Latte", "db", "latte"]].map(([cap, k, theme]) => (
              <div key={cap} {...(theme ? { "data-theme": theme } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                {k === "theme" ? <ThemeNotFoundModal /> : <DbInitErrorModal />}
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["size", "440 × 140..360"], ["title", "popup title bar · 14 / 600 · rule below"], ["body", "13 · text-secondary · para gap 12 · plain"], ["scroll edge", "above buttons, only while content is hidden below"], ["dismiss", "Primary, rightmost · Enter / Esc"], ["app-ending", "same Primary, label Quit"]]}
            tokens={[{ tok: "--tasty-info-modal-min-height", use: "→ size-140" }, { tok: "--tasty-info-modal-title-edge", use: "→ border-default", color: "var(--tasty-info-modal-title-edge)" }, { tok: "--tasty-info-modal-scroll-edge", use: "→ border-default", color: "var(--tasty-info-modal-scroll-edge)" }]} />
          <Note><b>App-ending message.</b> The database error keeps the single <b>Primary</b> button — it is the only way forward, not a destructive choice, so Danger would suggest an alternative that does not exist. What changes is the <b>label</b>: <b>Quit</b> (ko 종료 · ja 終了) instead of OK, so the button says what it does. Enter / Esc still trigger it. The body text shown is a sample of the seven cause strings.</Note>
        </Spec>
        <Spec id="popup-title-bar-one-or-two-buttons-title-centred-on-the-strip" title="Popup title bar — one or two buttons, title centred on the strip"
          when={<>The shared title bar keeps the title <b>centred on the strip</b> however many buttons sit on the right. Both sides reserve the same width: <b>edge inset + N × button + (N − 1) × gap + text gap</b> — <b>32</b> for × alone, <b>60</b> for fullscreen + ×. A long title ellipsises inside that symmetric band; it never slides left to use the free space. Buttons are <b>IconButton sm</b> (<span className="tok">--tasty-popup-title-btn-size</span> 24; the product's 20 goes), <span className="tok">--tasty-popup-title-btn-gap</span> 4 apart, the rightmost <span className="tok">--tasty-popup-title-edge-inset</span> 4 from the edge. <b>Fullscreen</b> (glyph <code>fit</code>) sits left of × and appears only on a popup that declares a fullscreen stage — today the notifications popup (352 × 400). Hover = overlay + tooltip; click widens the popup to its stage.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["Mocha", "mocha"], ["Latte", "latte"]].map(([label, theme]) => (
              <div key={label} {...(theme ? { "data-theme": theme } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-sm)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · two buttons · short title</span>
                <NotificationsPopupHead />
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{label} · two buttons · long title ellipsises in the symmetric band</span>
                <NotificationsPopupHead title="Notifications from every workspace and remote host" />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["centre", "strip centre, always"], ["reserve", "4 + N × 24 + (N − 1) × 4 + 4 → 32 · 60 per side"], ["buttons", "IconButton sm 24 (was 20 in the product)"], ["order", "fullscreen (fit) · close"], ["fullscreen", "only when the popup declares a stage"], ["long title", "ellipsis inside the band"]]}
            tokens={[{ tok: "--tasty-popup-title-btn-size", use: "→ icon-button-size-sm 24" }, { tok: "--tasty-popup-title-btn-gap", use: "→ space-xs 4" }, { tok: "--tasty-popup-title-edge-inset", use: "→ space-xs 4" }, { tok: "--tasty-popup-title-text-gap", use: "→ space-xs 4" }, { tok: "--tasty-notifications-popup-width", use: "→ size-352" }]} />
          <Dont><b>Don't</b> centre the title in the space left of the buttons. With two buttons the title then sits 28 px left of centre, and two popups side by side no longer line up.</Dont>
        </Spec>
        <Spec id="permissions-notice-macos-the-long-case" title="Permissions notice (macOS) — the long case"
          when={<>The same shell with the longest body. The six paragraphs are kept (they are the only place the signing and Automation caveats appear) and the <b>body scrolls</b>; title bar and button row never do. The scroll edge (<span className="tok">--tasty-info-modal-scroll-edge</span>) sits above the button row at the top and mid-way, and goes away at the end; the title bar rule bounds the top in every state. Paragraph gap 12. Settings paths are set in text-primary / medium; the shell command is mono on a raised chip; the two lead-ins are semibold. Buttons: <b>[Open permission settings]</b> Secondary, <b>[OK]</b> Primary on the far right. Open keeps the modal up (settings is its own window). OK · Enter · Esc close. There is no "don't show again".</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["scrolled to top", "top", null], ["mid-way", "mid", null], ["at the end", "bottom", null], ["top — Latte", "top", "latte"]].map(([cap, sc, theme]) => (
              <div key={cap} {...(theme ? { "data-theme": theme } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <PermissionNoticeModal scroll={sc} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["width", "440 (unchanged)"], ["height", "140..360 · body scrolls"], ["title", "popup title bar · 14 / 600"], ["body", "13 · text-secondary · para gap 12"], ["paths", "text-primary · 500"], ["command", "mono 11 · surface-raised chip"], ["scroll edge", "1px border-default above buttons, only while content is hidden below"], ["buttons", "Open permission settings (Secondary) · OK (Primary, rightmost)"], ["keys", "Enter / Esc = OK"]]}
            tokens={[{ tok: "--tasty-info-modal-width", use: "→ size-440" }, { tok: "--tasty-info-modal-max-height", use: "→ size-360" }, { tok: "--tasty-info-modal-para-gap", use: "→ space-md 12" }, { tok: "--tasty-info-modal-scroll-edge", use: "→ border-default", color: "var(--tasty-info-modal-scroll-edge)" }]} />
          <Note>ko / ja bodies are longer and only scroll further. The shell does not grow for them.</Note>
        </Spec>
        <Spec id="permissions-notice-fda-branches-signing-aside" title="Permissions notice — FDA branches · signing aside"
          when={<>Only the Full Disk Access paragraph changes between <b>never</b> / <b>stale</b> / <b>revoked</b>; title, buttons and the show rule do not. <b>Stale</b> carries the remove-and-add step as a <b>numbered two-step list</b> (text-primary, body size), followed by the quit-and-reopen line. <b>Revoked</b> stays one paragraph. The <b>signing paragraph</b> now shows in <b>every</b> notice (release and self-built are both ad-hoc signed) as <b>secondary text</b>: last in the body, caption size, text-muted. Shown scrolled to the end, where the branch and the aside sit.</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["never — no grant on record", "never", null], ["stale — granted before, app changed", "stale", null], ["stale — Latte", "stale", "latte"], ["revoked — turned off outside Tasty", "revoked", null]].map(([cap, fda, theme]) => (
              <div key={cap} {...(theme ? { "data-theme": theme } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{cap}</span>
                <PermissionNoticeModal scroll="bottom" fda={fda} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["branches", "never · stale · revoked — FDA paragraph only"], ["stale steps", "ordered list 1–2 · body 13 · text-primary · indent space-xl · gap space-xs"], ["signing aside", "every notice · last · caption 12 · text-muted · command chip unchanged"], ["new strings", "stale step 1 · stale step 2 · stale retry line (split from the old stale paragraph)"]]}
            tokens={[{ tok: "--tasty-text-muted", use: "signing aside", color: "var(--tasty-text-muted)" }, { tok: "--tasty-font-size-caption", use: "signing aside" }, { tok: "--tasty-space-xl", use: "step list indent" }]} />
        </Spec>
      </Section>
      <Section id="scriptconfirm" title="Script changed confirm">
        <Spec id="wrapped-body-height-fits-the-content" title="Wrapped body · height fits the content"
          when={<>The confirm stays <b>360</b> wide in every language. The <b>changed</b> Tag sits on <b>its own line</b> and the explanation is a <b>full-width paragraph</b> under it, so the text never starts beside the tag and wraps back under it (no <code>horizontal_wrapped</code>). Height <b>fits the content</b> — no 152 floor, so ko leaves no empty band under the buttons and ja grows by one line. Body → buttons is <span className="tok">--tasty-space-md</span> (8 row gap + 4 extra). Buttons stay inside the 360 (right-aligned, Run anyway primary, Cancel ghost).</>}>
          <Stage variant="solo" style={{ padding: "var(--tasty-space-lg)", background: "var(--tasty-bg-app)", gap: "var(--tasty-space-lg)", flexWrap: "wrap", alignItems: "flex-start" }}>
            {[["en", null], ["ko", null], ["ja", null], ["ja · Latte", "latte"]].map(([lang, th]) => (
              <div key={lang} {...(th ? { "data-theme": th } : {})} style={{ display: "flex", flexDirection: "column", gap: "var(--tasty-space-xs)", padding: "var(--tasty-space-md)", background: "var(--tasty-bg-app)", borderRadius: "var(--tasty-radius)" }}>
                <span style={{ fontSize: "var(--tasty-font-size-caption)", color: "var(--tasty-text-muted)" }}>{lang}</span>
                <ScriptConfirmG lang={lang.slice(0, 2)} />
              </div>
            ))}
          </Stage>
          <Meta
            specs={[["width", "360 × UI scale · all languages"], ["rows", "title (body 13 semibold) · path (mono caption, ellipsis) · Tag warning “changed” · paragraph (caption 11, text-secondary, line-height-ui, full width) · buttons"], ["row gap", "space-sm 8 · Tag → paragraph space-xs 4"], ["body → buttons", "space-md 12"], ["height", "fits content (title bar + padding + rows) · no minimum"], ["padding", "12 top/bottom · 14 sides"], ["buttons", "right-aligned · Cancel ghost sm · Run anyway primary sm · inside 360"]]}
            tokens={[{ tok: "--tasty-accent-warning", use: "changed Tag", color: "var(--tasty-accent-warning-raised)" }, { tok: "--tasty-text-secondary", use: "paragraph", color: "var(--tasty-text-secondary)" }, { tok: "--tasty-space-md", use: "body → buttons" }, { tok: "--tasty-shadow-modal", use: "popup" }]} />
        </Spec>
      </Section>

      <Section id="permdelete" title="Explorer — Delete permanently">
        <Spec id="destructive-confirm-with-a-target-list" title="Destructive confirm with a target list"
          when={<>Window-scope centred confirm on the shared scrim, opened by the Explorer's <b>Delete permanently…</b>. Reuses the info-modal frame size (440, max 360) because it carries a scrolling target list. Default focus <b>Cancel</b>; Esc, close and scrim click cancel. The full state set (file, folder, link, many, focus, ko/ja, progress, results) is on <a href={`${BASE}design/gallery/explorer-ops/#permdelete`}>Explorer — file operations › Delete permanently</a>.</>}>
          <Stage variant="solo center"><Backdrop height={380}><div align="center">{<ExplorerDeleteFrame_ExplorerDeleteFrame items={ExplorerDeleteFrame_XD_ITEMS.many.slice(0, 4)} />}</div></Backdrop></Stage>
          <Meta
            specs={[["width", "info-modal-width 440"], ["max height", "info-modal-max-height 360 · list scrolls past explorer-delete-list-max-height 160"], ["buttons", "Cancel ghost (default focus) · Delete permanently danger"], ["dismiss", "Esc · close · scrim click = cancel"]]}
            tokens={[{ tok: "--tasty-accent-danger", use: "glyph + button", color: "var(--tasty-accent-danger-raised)" }, { tok: "--tasty-explorer-delete-list-max-height", use: "list" }]} />
        </Spec>
      </Section>
    </>
  );
}

export const page = {
  active: "overlays-dialogs",
  nav: NAV,
  head: {
    title: "Dialogs",
    intro: "Explore dialogs for confirming an action or editing a value. Check their messages, actions, and dismissal, then compare the scrim and frame over different grounds.",
    
  },
  content: <Page />,
};

export default function GalleryRoute() {
  return <PageStates value={STATES}>{page.content}</PageStates>;
}
