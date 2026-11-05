# Tasty Gallery

A restructured, agent-facing reference for the Tasty UI — a redesign of the
egui `tasty-gallery` crate (Storybook-style component catalog) rebuilt as
linked HTML pages. Where the original showed tokens in isolation (e.g. bare
spacing bars), every specimen here is tied to **usage**: what it's for, the
exact dimensions, and the tokens it consumes — and why.

## Pages
| File | Catalog | Covers |
|------|---------|--------|
| `index.html` | **Foundations** | Color (elevation ramp, text, accent roles), **Elevation — floating surfaces (the two shadows + the scope rule)**, Type, Spacing, Radius/border/motion — each token shown inside the thing it builds, plus **Role gaps — settled** (borrowed color roles, half-pixel type snapping, the one tinted-box opacity pair). |
| `icons.html` | **Icons** | The canonical glyph set (`icons/<name>.svg`, mirrored in `icons.json` + `Icon.jsx`), rendered by name through `<Icon>`. |
| `components.html` | **Components** | Live primitives: Button, IconButton, Badge, Tag, Kbd, Input, Select, MultiSelect, Checkbox, Switch, Tab, TreeRow, MenuItem, StatusDot, Spinner, Toast (+ Toast stack), Hint text, Table, ListCtrl. Hover/focus respond for real. Also the **dot family** (generic 8 vs compact 6, attached ring geometry, toast gap), **one keycap** (palette converges on Kbd) and **glyph vs font sizes**. **SearchableSelect** (2026-10-19, `components-searchselect.jsx`: trigger states, open list, filter / no match / loading / pinned, Appearance › General font rows). |
| `overlays-dialogs.html` | **Overlays · Dialogs** | Centered modal confirm/edit surfaces: scrim recipe, agent approval, convert, file-handler picker, apply preset, markdown open, rename, **remote transfer (progress + failed popups)**, **Explorer delete permanently** (loads `explorer-delete-frame.jsx`), and **scrim scope** — a surface-bound popup dims its own surface only (both split directions · clamped popup · parent + child picker with one scrim). |
| `overlays-windows.html` | **Overlays · Windows** | Large modal surfaces (own internal nav): command palette, listening ports, remote connections, **preset editor (PresetView window)**, two-tier settings, **git worktree viewer**, **clipboard viewer**, **popup move & resize** (drag handles · 8-way resize · cursors · constraints · z-order), two-tier settings incl. **General › Remote transfer** subtab, **file picker — open + save mode** (one footer confirm · overwrite · deep-path elision), **Keybindings · Import / Export** incl. the resolved open values (export failure · stacked bundle notices · parse-failure copy · multi-conflict summary · modifier placeholder), the **path-bar allocation** rule at 400×360, **From ssh config** hosts under the remote profiles, **segmented vs tab-strip active states**, the **Process column minimum**, the **git diff toolbar height**, the **one numeric-input shape** and **Hook Handlers** origin/remove rules. **Notifications popup + fullscreen** (2026-10-17). Plugins window **unverified warning + Add anyway / Register anyway** (2026-10-19, `overlays-windows-plugin-trust.jsx`). **Handler › File Detectors · File Handlers** (2026-10-20 — two-line rows, add card, pending add/remove, empty; `overlays-windows-file-handlers.jsx`). **Misc › Anomaly detection** (2026-10-27, `overlays-windows-anomaly.jsx` + kit `settings_anomaly.jsx`) · Plugins **trusted verdict contrast** (2026-10-27) · Notifications **warning glyph contrast** (2026-10-27). |
| `overlays-popups.html` | **Overlays · Popups & menus** | Anchored, scrim-less surfaces: tools menu, search bar, switch-number overlay (modifier-hold tab/workspace numbers). **Notice column** (2026-10-19, `overlays-popups-notices.jsx`): non-focus restore-failure / save-halt notice, response-wait chip placement. **Over a web page — opaque mode** (2026-10-21, `overlays-popups-webview.jsx`): screens A–D × transparent / opaque / current-hidden, Mocha + Latte, host-rect outlines. |
| `overlays-banners.html` | **Overlays · Banners** | The **Banner** family (4th overlay family) — floating top notice, TTL countdown + queue + cross-scope stacking; the **mouse-capture** banner instance (anatomy · hit-zone), its **⋯ more menu** (per-app opt-out rows) + its **capture blacklist** editor. · HTML mirror large-file / limit notices (2026-10-12). |
| `overlays-tutorial.html` | **Overlays · Tutorial** | The **Marker** family (6th overlay family) — a message-less floating geometric marker (ring / glow / pulse / corners / leader) over a target; its guidance **Callout** (title · body · progress · Back/Next/Skip · 4-way tail); and the **Topic-list popup** that launches the flow. Opened from Tools → Tutorial. |
| `layouts.html` | **Layouts** | Structural shells: full sidebar ↔ collapsed rail, pane tab strip (+ `layouts-tabstrip.jsx`: status cluster, scroll arrows, out-of-view move cue), 1-depth (list→detail), drill-down (content swap: list ⇄ detail), 2-depth (tabs→sections), multi-tier tabs, divider, surface focus states, **workspace status bar** (final items + collapse order), **loading overlay** (+ `layouts-loading.jsx`: spinner over surface · tab · LNB card · rail avatar, 2026-10-16). **Notifications summary** (LNB block + rail, level count badge — 2026-10-17, `notifications.jsx`). Loading overlay **pre-layout** storyboards + **PendingIndicator** response wait (2026-10-19, `layouts-loading-prelayout.jsx`). |
| `dag.html` | **Surfaces · Task DAG** | The read-only **Task DAG** view: graph canvas (layered top-down, orthogonal edges, dot grid, LOD tiers, minimap + zoom chrome), node card in all **8 execution states** and 4 task kinds, the 3 edge relations, runner badge (incl. stopped-with-ready-work), DAG list row, node detail (side panel / bottom sheet with error tail), empty + cycle states, and both hosts — tab surface (wide + 320) and workspace popup, incl. the **detail view's back-bar chrome** (zoom cluster + runner badge, no second header). |
| `loading.html` | **Chrome · Startup** | The boot / startup loading screen, first-run shell setup, shutdown screen and the **boot error screen** (2026-10-10). |
| `plugins.html` | **Plugins** | **Plugin surfaces** — surface kinds contributed by bundled plugins, kept off the general Layouts page because they grow without bound: Explorer (file manager), Markdown viewer, HTML (webview) viewer, Image viewer/paint. · HTML address bar band + mirror size banners / receiving state (2026-10-12). |
| `explorer-ops.html` | **Plugins · Explorer file ops** | Explorer file-operation design (2026-10-09): toolbar create / view groups, inline new-item naming and name errors, drag & drop (chip, drop targets, OS files), progress on the status line, name-conflict prompt, results / retry / undo toasts, Properties popup, preview panel, Grid thumbnails, filter and recursive search; batch 11 kind words, refused requests, Retry of originals, undo titles and remaining states (links, keyboard item, drag-select, hidden files, show in folder, redo, create in folder) in `explorer-ops-b11.jsx`. Loads `plugins.jsx` in kit-only mode (`window.__EXPLORER_KIT_ONLY`) and reuses `window.ExplorerKit`.; **remote explorer writes** (disabled rows + reasons, waiting row, connection lost, host on progress, drag reasons) and **Delete permanently** (menu row, confirm, cancel/invalid, progress, results) (2026-10-12). · temporary folder left behind (2026-10-12, `explorer-ops-staging.jsx`) · Properties actions — time format setting, Open footer, permission editor, OS menu rows (2026-10-21, `explorer-ops-props-actions.jsx`); permission box grid / stack / list in a narrow cell (2026-10-22). |

## How to read a specimen
Every `<Spec>` carries three things, in this order:
1. **When to use** — a plain statement of the role (not just "what it looks like").
2. **The live demo** — built from the real DS components (`components/bundle.js`).
3. **Layout spec + Tokens used** — fixed dimensions and the exact tokens it
   consumes, each annotated with its job.

## Controls (top bar)
- **Theme** — Mocha (dark, default) / Latte (light). Flips `data-theme`, persists.
- **Specs** — overlays the 4px grid and width/height rails on demos that opt in.

Both persist in `localStorage` (`tasty-gallery-theme`, `tasty-gallery-specs`)
and are shared across all pages.

The left nav is **grouped** (Foundations / Components / Overlays / Layouts /
Surfaces / Chrome / Plugins) in
`shell.jsx` `PAGE_GROUPS`; a page sits in one group. Live specimen demos
**lazy-mount** as they scroll near the viewport (`Stage` in `shell.jsx`), so a
page commits only a few demos at a time.

## Build
Each page loads `../styles.css` + `../components/bundle.js`, then `shell.jsx` (shared
chrome + specimen primitives, exposed on `window.Gallery`) and one content
script (`foundations.jsx` / `icons.jsx` / `components.jsx` / `layouts.jsx` /
`dag.jsx` / `loading.jsx` / `plugins.jsx` (+ `plugins-md-address.jsx`, the Markdown address bar, and `plugins-html-address-contrast.jsx`, HTML address field ink, 2026-10-21) — `explorer-ops` loads `plugins.jsx` + `explorer-ops-parts.jsx` + `explorer-ops-b11.jsx` + `explorer-ops-b12.jsx` + `explorer-delete-frame.jsx` + `explorer-ops-remote.jsx` + `explorer-ops-delete.jsx` + `explorer-ops-staging.jsx` + `explorer-ops.jsx` —, or an `overlays-*.jsx` page preceded by
`overlays-shared.jsx`; `overlays-windows` additionally loads `preset_editor.jsx`, `overlays-windows-perf.jsx` (Performance apply note, 2026-10-21; number field fits its value, 2026-10-22), `overlays-windows-b12.jsx` and `notifications.jsx` (+ kit `overlays/info_modal.jsx`, `overlays/notifications.jsx`); `layouts` also loads `notifications.jsx` + kit `notification_summary.jsx` (narrow settings rows · Plugins record slots · Win / Super as option)).
No tokens or component visuals are redefined here — specimens reference the
design system only.

## Completeness (no cuts)
The gallery is a **mirror of the whole app**: every UI component that exists in
the body — modal / popup / shared widget / layout idiom — is exposed here as a
specimen. Nothing is cut from the catalog. Earlier this page folded the egui
gallery's five categories (Appearance / Widget / Popup / Component / Layout)
into four (Foundations / Components / Overlays / Layouts) and merged redundant
splits (Widget+Component → Components; Surface Highlights → Layouts) — that
**restructuring stays**, but no component is dropped. Specimens previously
omitted (Spinner, Port Scanner, Search Bar, Tools Menu, Remote tool, Convert,
File Handler Picker, Apply Preset, Markdown Open, Toast stack, Divider,
multi-tier tabs, Hint text) are restored. The abstract token bars from the egui
gallery remain replaced by usage-anchored Foundations — anchoring is a
presentation change, not a cut.
