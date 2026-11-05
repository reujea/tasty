# Tasty Gallery

The gallery is the canon the app follows. Its source lives in this system and is copied to the site at release. This page is the brief for that move, followed by the catalog as it stands. Companions: [`coverage.md`](coverage.md) (contract and location against specimen), `site-form-mock.html` (one page in the new form), `state-grid.jsx` and `contract-audit.js` (reference code).

## The site form

Decided 2026-11-05. The move itself belongs to the implementing side.

### Page shell

- **Use the site's header, theme, language and footer.** The gallery has no top bar, brand block, breadcrumb, theme toggle or Specs toggle of its own. Remove `Shell`'s `g-brand`, `g-top`, `ThemeToggle`, `SpecsToggle` and the `tasty-gallery-theme` and `tasty-gallery-specs` keys. The page theme is the site theme. A specimen that compares themes side by side keeps its own `data-theme` wrapper.
- **Use the guide's grammar:** left sidebar, body, previous and next. Sidebar links are text only, so the `desc` strings go. The page's sections nest under the current page in the sidebar. There is no right-hand contents list, and "On this page" goes.
- **Main column.** Specimens need the width: body fills the space beside the sidebar. Prose inside it stays at the guide's measure.
- **Headings.** Page title, section and specimen are all 14px or less, set apart by weight. Drop `g-section`'s uppercase mono heading and the 26px page title. Dates leave specimen titles: `Tooltip (2026-10-08)` becomes `Tooltip`. History lives in the changelog.
- **390px.** No horizontal page scroll. A stage, state grid or spec table wider than the column scrolls inside its own box, with the first column of a state grid sticky. At the guide's narrow width the sidebar takes the guide's narrow form. Measured on the mock: no page scroll at 1440 and 390 in both themes, and 5 of 6 state grids scroll inside their box at 390.

### Page map

The 15 URLs do not change, so `/design/gallery/<key>/` keeps its current path. The groups do: seven groups held four single-page groups, so they fold to five. New sections go inside existing pages, and no page is split. Previous and next follow this order.

| Group | Label | Key | Holds |
|---|---|---|---|
| Foundations | Foundations | `foundations` (`index.html`) | Elevation, floating surfaces, text, accents, ANSI, type, spacing, shape, UI scale, role gaps, tint box |
| Foundations | Icons | `icons` | The icon system, modifier symbols, every glyph |
| Foundations | Rules | `rules` | Every ground by role, all 9 groups stacked with no tabs. Site pages. Colors told apart |
| Foundations | Location map | `rules-map` | Every location in Mocha and Latte |
| Components | Components | `components` | Buttons, Badge, Tag, Kbd, form controls, Tab, TreeRow, MenuItem, feedback, CenterState, HelpHint, Tooltip, hint text, Table, ListCtrl, Scrollbar |
| Overlays | Dialogs | `overlays-dialogs` | Scrim and frame, scrim over each ground, approval, convert, file handler, preset, markdown open, rename, category, transfer, info modal, script confirm, delete permanently |
| Overlays | Windows | `overlays-windows` | Palette, ports, remote, remote attach, file picker, settings, permissions, plugins window, keybinding import, scripts, git viewer, clipboard, notifications, move and resize, preset editor |
| Overlays | Popups and menus | `overlays-popups` | Tools menu, search bar, switch overlays, modifier hints, categories, notice column, opaque mode over a page |
| Overlays | Banners | `overlays-banners` | Banner family and its instances |
| Overlays | Tutorial | `overlays-tutorial` | Marker, callout, topic list, composite |
| Shell | Layouts | `layouts` | Sidebar and rail, tab strip, list idioms, tiers, divider, surface focus, attention, move source, loading, notification summary, status bar, title bar |
| Shell | Startup | `loading` | Boot, phases, Latte, first-run shell, shutdown, boot error |
| Surfaces | Task DAG | `dag` | Canvas, node, phases, edges, routes, chrome, runner, rows, detail, empty and cycle, hosts |
| Surfaces | Plugin surfaces | `plugins` | Explorer, Markdown, HTML, Image |
| Surfaces | Explorer file ops | `explorer-ops` | Create, drag, search, remote writes, delete, staging, properties, results |

### One specimen

Every specimen has a stable `id` on its `Spec`, because the Rust gallery and the check both point at it. Its parts come in this order, and each part appears only when it has content.

| Part | What | Interactive |
|---|---|---|
| Heading and when to use | Title is ours. The sentence is a prose slot | - |
| Demo | The live component. Pointer and keyboard work | Yes |
| States | The frozen grid, one per contract | No |
| Views | What the demo hides, drawn as frames | No |
| Specs | Dimensions and tokens, always shown | No |

### Fixed states

**`StateGrid` draws one contract: a row per host, a column per state, from the `components` rows of `tokens/ground-ink-rules.json`.** Column order is `rest`, `hover`, `press`, `focus`, `selected`, `selected+hover`, `selected+press`, `keyboard-active`, `disabled`, `invalid`, `readonly`. A state or host the contract does not have gets no column or row. An empty intersection stays blank. Each cell paints the host ground and carries `data-contract`, `data-state` and `data-host`, spelled as in the file.

**How a cell freezes.** `mirrorPseudoStates` copies each `:hover`, `:active`, `:focus`, `:focus-visible` and `:focus-within` rule right after its original, with the pseudo-class replaced by `[data-force~="hover"]`, `[data-force~="press"]` or `[data-force~="focus"]`. The specificity and the order stay, so a cell shows what the component's own CSS shows and nothing is restyled. `hover` is forced alone, `press` as `hover press` because a pressed control is also hovered, and `selected`, `disabled`, `invalid` and `readonly` come from the component's real props. Inside a cell, `pointer-events`, `transition` and `animation` are off, and the control takes `tabindex="-1"`.

**The mirror has three limits.** A rule on a pseudo-element, such as `::-webkit-scrollbar-thumb:hover`, takes no attribute, so draw that part plainly. A component that holds hover in JS state takes a prop instead: `SearchableSelect` does, with `hoverIndex`. A part with no component, such as a title bar or a DAG node, is written with the contract's tokens directly.

**Renderers.** `render(state, host)` for each contract lives in the owner page's folder. `coverage.md` names the owner of each contract.

### Hidden states

A demo may hide states behind a tab, a fold or an open menu. The check does not click, so each of these also appears as a frame under Views, with `data-view="<name>"`. Known cases to convert:

- Rules page tabs: stack all 9.
- Plugins window tabs, including Attention. Settings tabs. Remote tabs.
- Drill-down detail, DAG popup list and detail, category collapse.
- Explorer view modes.
- Any open menu or Select: use the `open` prop.
- Tooltip: use the `open` prop.

Rows below a scroll fold stay in the DOM and are measured, so a list needs no frame. Do not virtualize a specimen.

`Stage` mounts lazily. With `?audit=1` it mounts everything at once and changes nothing else.

### Specs

The toggle goes. Specs are always on, because they are what the app is built from.

- **Layout table:** property, token, resolved value. Read the value from the token at render, as the mock's `Resolved` does, so it cannot drift. A value with no token needs a design request.
- **Contract table:** part, token, states, Mocha, Latte, for specimens with a contract, generated from the file.
- **Tokens table** for the rest: the same columns as chips carry today, as a table so it holds at 390.
- **The grid and rail overlay** becomes opt-in per stage: a small control on the stages that have `grid` or `anno` parts, rendered only while on, so it never sits in the DOM for the check. Layouts is the only page using `anno`.

### The check

The rules are in `RULES.md` section 14. `contract-audit.js` is a reference for the frozen-cell part and covers the Button family's parts. It runs on the mock at 113 cells and 241 rows, with no mismatch in either theme against the sources. The ratios it computes match the file's to 0.03.

| What | Rule |
|---|---|
| Every row of `components` | Has a frozen cell on some page, or the check fails |
| A cell's part | Its ink equals the row's token within 1/255 per channel, because `color-mix` rounds |
| A cell's part with a grade | Meets the grade |
| Other text on the page | 4.5, composited through any opacity |
| Other glyphs | 3 |
| `data-audit-exempt="<id>"` | Skips its subtree. The id must be in `exceptions[].id` with `audit: "subtree"`. Each use is listed in the output |
| `data-ink-grade="none" data-ink-why="disabled\|ornament"` | Outside a cell, marks an ink with no grade. It is not an exception, as `RULES.md` section 12 says |
| Scope | `[data-gallery-page]`. Site header, sidebar and footer carry `data-site-chrome` and are not part of these rules |

Exempt ids: `ansi-content`, `terminal-attributes`, `html-page`, `plugin-webview`, `third-party-plugin`, `image-content`, `code-highlight`, `brand`, `os-window-controls`, `dimmed`, `under-scrim`, `decorative-grid`. Opacity alone never exempts anything.

The check walks ancestors for the ground, so a specimen paints its ground on an ancestor. A sibling laid over ink, as with a scrim, is not seen: mark what is under it `under-scrim`.

### Files

**Structure: one folder per page, one file per section where a section is large.** A page imports from `parts/` and never from another page. `parts/` is for what two or more pages share. `window.*` globals and `window.__EXPLORER_KIT_ONLY` go with the move to modules.

**Format: ES modules, no exceptions for "old" files.** Every `.jsx` under `gallery/`, `ui_kits/terminal/` and `components/` imports what it uses and exports what it offers. Nothing registers itself on `window`, and nothing is loaded by a script tag. `ui_kits/terminal/chrome.jsx` and `work.jsx` are modules like the rest, and `app.jsx` imports them by name: a window-global version of either breaks the page. None of these files is a local variant of something else. To change one, read the current published file and edit that. Never republish a working copy from an earlier session.

| Now | Becomes |
|---|---|
| `shell.jsx` | `parts/specimen.jsx` (Section, Spec, Stage, Cluster, Meta, Note, Do, Dont). Page shell goes to the site layout |
| `gallery.css` | `parts/specimen.css`, with the shell, top bar and nav rules removed |
| `state-grid.jsx` | `parts/state-grid.jsx` |
| `contract-audit.js` | The check. Not shipped |
| `foundations.jsx` | `foundations/index.jsx` |
| `icons.jsx` | `icons/index.jsx` |
| `rules.jsx` | `rules/index.jsx` |
| `rules-map.jsx` | `rules-map/index.jsx` |
| `components.jsx` | `components/index.jsx` |
| `components-searchselect.jsx` | `components/searchable-select.jsx` |
| `components-scrollbar.jsx` | `components/scrollbar.jsx` |
| `layouts.jsx` | `layouts/index.jsx` |
| `layouts-tabstrip.jsx` | `layouts/tab-strip.jsx` |
| `layouts-move.jsx` | `layouts/move-source.jsx` |
| `layouts-loading.jsx` | `layouts/loading-overlay.jsx` |
| `layouts-loading-prelayout.jsx` | `layouts/loading-prelayout.jsx` |
| `layouts-titlebar.jsx` | `layouts/title-bar.jsx` |
| `notifications.jsx` | `parts/notifications.jsx` (layouts and windows both use it) |
| `loading.jsx` | `loading/index.jsx` |
| `dag.jsx` | `dag/index.jsx` |
| `overlays-shared.jsx` | `parts/overlay-frames.jsx` |
| `overlays-dialogs.jsx` | `overlays-dialogs/index.jsx` |
| `overlays-scrim-grounds.jsx` | `overlays-dialogs/scrim-grounds.jsx` |
| `explorer-delete-frame.jsx` | `parts/explorer-delete-frame.jsx` (dialogs and explorer-ops) |
| `overlays-popups.jsx` | `overlays-popups/index.jsx` |
| `overlays-popups-notices.jsx` | `overlays-popups/notice-column.jsx` |
| `overlays-popups-webview.jsx` | `overlays-popups/opaque-over-page.jsx` |
| `overlays-banners.jsx` | `overlays-banners/index.jsx` |
| `overlays-tutorial.jsx` | `overlays-tutorial/index.jsx` |
| `overlays-windows.jsx` | `overlays-windows/index.jsx` |
| `overlays-windows-b12.jsx` | Split: `overlays-windows/settings-narrow.jsx` (`SettingsNarrowSection`), `overlays-windows/kb-plugin-records.jsx` (`KbPluginsRecordSpec`), `overlays-windows/option-key.jsx` (`OptionKeySection`) |
| `overlays-windows-anomaly.jsx` | `overlays-windows/settings-anomaly.jsx` |
| `overlays-windows-file-handlers.jsx` | `overlays-windows/file-handlers.jsx` |
| `overlays-windows-perf.jsx` | `overlays-windows/settings-performance.jsx` |
| `overlays-windows-plugin-trust.jsx` | `overlays-windows/plugin-trust.jsx` |
| `preset_editor.jsx` | `overlays-windows/preset-editor.jsx` |
| `plugins.jsx` | `plugins/index.jsx`, with the explorer kit it exposes moved to `parts/explorer-kit.jsx` |
| `plugins-md-address.jsx` | `plugins/markdown-address.jsx` |
| `plugins-html-address-contrast.jsx` | `plugins/html-address-ink.jsx` |
| `explorer-ops.jsx` | `explorer-ops/index.jsx` |
| `explorer-ops-parts.jsx` | `explorer-ops/create-drag-search.jsx` |
| `explorer-ops-b11.jsx` | Split: `explorer-ops/results-and-refusals.jsx` (`FollowupsB11Section`), `explorer-ops/remaining-states.jsx` (`RemainingSection`) |
| `explorer-ops-b12.jsx` | Split: `explorer-ops/address-and-toolbar.jsx` (`FollowupsB12Section`), `explorer-ops/properties-and-preview.jsx` (`RemainingB12Section`) |
| `explorer-ops-delete.jsx` | `explorer-ops/delete-permanently.jsx` |
| `explorer-ops-remote.jsx` | `explorer-ops/remote-writes.jsx` |
| `explorer-ops-staging.jsx` | `explorer-ops/staging-left.jsx` |
| `explorer-ops-props-actions.jsx` | `explorer-ops/properties-actions.jsx` |

The page `.html` files go, and `site-form-mock.*` goes once the first real page exists. `ui_kits/terminal/` keeps its paths: gallery pages import what they use from it.

### What stays in this system

| Item | Decision | Why |
|---|---|---|
| Gallery sources as `.jsx` | Stay here, published as text | They are the source the site copies |
| `gallery/*.html` | Removed after the move | Once the sources are modules they do not render on the Artifact page. The site is where they render |
| `components/<Name>/preview.html`, `README.md`, `.d.ts` | Stay | The Artifact's component tab reads them |
| `components/bundle.js`, `bundle.css` | Stay | The previews load them. The site does not: it imports `components/**/*.jsx` |
| `components/**/*.jsx` | Canon for both | `bundle.js` is a build of these and can lag them, as it does now |

### Prose slots, for Codex

The gallery's prose is not design's. Each slot carries `data-prose-slot` in the mock. The existing English stays as the draft until Codex replaces it.

| Slot | Where | Count today |
|---|---|---|
| Page intro | `head.intro` of each page | 15 |
| Group landing or gallery entry | `/design/` link into the gallery | 1 |
| When to use | `when` on each `Spec` and `Section` that has one | About 380 |
| Notes | `<Note>` | About 230 |
| Do and don't | `<Do>`, `<Dont>` | About 110 |

Counts are from a line search of the sources, so treat them as sizes rather than totals. New to write: when to use for title bar, scrollbar and scrim over each ground. Text inside a specimen, its layout table and its token table is ours. `HowTo` goes: the specimen order explains itself, and Codex may put the reading note in the gallery entry.

---

# The catalog today

The rest describes the pages as they stand until the move.

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
| `rules.html` | **Rules** | Every ground × role cell of `tokens/ground-ink-rules.json`, painted with the real tokens, with the measured contrast of the current theme. |
| `rules-map.html` | **Location map** | Each window location with its ground and the child grounds it may hold, in Mocha and Latte side by side. |
| `components.html` | **Components** | Live primitives: Button, IconButton, Badge, Tag, Kbd, Input, Select, MultiSelect, Checkbox, Switch, Tab, TreeRow, MenuItem, StatusDot, Spinner, Toast (+ Toast stack), Hint text, Table, ListCtrl, Scrollbar (`components-scrollbar.jsx`). Hover/focus respond for real. **Selected row** — meaning glyphs, match run, `+`, muted on bg-app (2026-11-05). Also the **dot family** (generic 8 vs compact 6, attached ring geometry, toast gap), **one keycap** (palette converges on Kbd) and **glyph vs font sizes**. **SearchableSelect** (2026-10-19, `components-searchselect.jsx`: trigger states, open list, filter / no match / loading / pinned, Appearance › General font rows). |
| `overlays-dialogs.html` | **Overlays · Dialogs** | Centered modal confirm/edit surfaces: scrim recipe, **scrim over each ground** (`overlays-scrim-grounds.jsx`), agent approval, convert, file-handler picker, apply preset, markdown open, rename, **remote transfer (progress + failed popups)**, **Explorer delete permanently** (loads `explorer-delete-frame.jsx`), and **scrim scope** — a surface-bound popup dims its own surface only (both split directions · clamped popup · parent + child picker with one scrim). |
| `overlays-windows.html` | **Overlays · Windows** | Large modal surfaces (own internal nav): command palette, listening ports, remote connections, **preset editor (PresetView window)**, two-tier settings, **git worktree viewer**, **clipboard viewer**, **popup move & resize** (drag handles · 8-way resize · cursors · constraints · z-order), two-tier settings incl. **General › Remote transfer** subtab, **file picker — open + save mode** (one footer confirm · overwrite · deep-path elision), **Keybindings · Import / Export** incl. the resolved open values (export failure · stacked bundle notices · parse-failure copy · multi-conflict summary · modifier placeholder), the **path-bar allocation** rule at 400×360, **From ssh config** hosts under the remote profiles, **segmented vs tab-strip active states**, the **Process column minimum**, the **git diff toolbar height**, the **one numeric-input shape** and **Hook Handlers** origin/remove rules. **Notifications popup + fullscreen** (2026-10-17). Plugins window **unverified warning + Add anyway / Register anyway** (2026-10-19, `overlays-windows-plugin-trust.jsx`). **Handler › File Detectors · File Handlers** (2026-10-20 — two-line rows, add card, pending add/remove, empty; `overlays-windows-file-handlers.jsx`). **Misc › Anomaly detection** (2026-10-27, `overlays-windows-anomaly.jsx` + kit `settings_anomaly.jsx`) · Plugins **trusted verdict contrast** (2026-10-27) · Notifications **warning glyph contrast** (2026-10-27). |
| `overlays-popups.html` | **Overlays · Popups & menus** | Anchored, scrim-less surfaces: tools menu, search bar, switch-number overlay (modifier-hold tab/workspace numbers). **Notice column** (2026-10-19, `overlays-popups-notices.jsx`): non-focus restore-failure / save-halt notice, response-wait chip placement. **Over a web page — opaque mode** (2026-10-21, `overlays-popups-webview.jsx`): screens A–D × transparent / opaque / current-hidden, Mocha + Latte, host-rect outlines. |
| `overlays-banners.html` | **Overlays · Banners** | The **Banner** family (4th overlay family) — floating top notice, TTL countdown + queue + cross-scope stacking; the **mouse-capture** banner instance (anatomy · hit-zone), its **⋯ more menu** (per-app opt-out rows) + its **capture blacklist** editor. · HTML mirror large-file / limit notices (2026-10-12). · Secondary, Kbd, Tag and state ink in both themes (2026-11-05). |
| `overlays-tutorial.html` | **Overlays · Tutorial** | The **Marker** family (6th overlay family) — a message-less floating geometric marker (ring / glow / pulse / corners / leader) over a target; its guidance **Callout** (title · body · progress · Back/Next/Skip · 4-way tail); and the **Topic-list popup** that launches the flow. Opened from Tools → Tutorial. |
| `layouts.html` | **Layouts** | Structural shells: **title bar** active and inactive (`layouts-titlebar.jsx`), full sidebar ↔ collapsed rail, pane tab strip (+ `layouts-tabstrip.jsx`: status cluster, scroll arrows, out-of-view move cue), 1-depth (list→detail), drill-down (content swap: list ⇄ detail), 2-depth (tabs→sections), multi-tier tabs, divider, surface focus states, **workspace status bar** (final items + collapse order), **loading overlay** (+ `layouts-loading.jsx`: spinner over surface · tab · LNB card · rail avatar, 2026-10-16). **Notifications summary** (LNB block + rail, level count badge — 2026-10-17, `notifications.jsx`). Loading overlay **pre-layout** storyboards + **PendingIndicator** response wait (2026-10-19, `layouts-loading-prelayout.jsx`). |
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
Each page is a folder of ES modules, `gallery/<page>/`, and the site renders it. `index.jsx` imports its section files, `parts/`, and the component and kit modules it shows, and the tokens come from `styles.css`.
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
