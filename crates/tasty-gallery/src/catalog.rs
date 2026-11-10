//! 카탈로그는 페이지 → 구역 → 예제로 구성한다.
//! 각 페이지는 Category 하나에 대응한다. host_shell은 페이지 전체를 스크롤하며
//! 구역 목록으로 왼쪽 탐색 메뉴의 앵커를 만든다.

pub mod chrome_loading;
pub mod components;
pub mod components_settled;
pub mod foundations_disabled_ink;
pub mod foundations_rolegaps;
pub mod foundations_settled;
pub mod foundations_shape;
pub mod foundations_uiscale;
pub mod icons;
pub mod icons_keys;
pub mod layouts_attention;
pub mod layouts_settled;
pub mod overlays_resize;
pub mod overlays_settled;
pub mod plugins_settled;
pub mod popup_frame;
pub mod spacing;
pub mod spec;
pub mod surface_focus;
pub mod theme;
pub mod toast_card;
pub mod typography;
pub mod widgets;

use std::sync::OnceLock;

use tasty_settings::{GeneralSettings, KeybindingSettings};
use tasty_type_appearance::theme::Theme;

/// 기본 키바인딩을 본체 설정 UI와 같은 규칙으로 표시한다.
pub(crate) fn modifier_label(combo: &str) -> String {
    KeybindingSettings::format_display(combo, &GeneralSettings::default())
}

/// 탭·워크스페이스·카테고리 전환 modifier 를 기본 키바인딩으로 표시한 캡션(이 순서).
fn switch_captions() -> &'static [String; 3] {
    static CAPTIONS: OnceLock<[String; 3]> = OnceLock::new();
    CAPTIONS.get_or_init(|| {
        let k = KeybindingSettings::default();
        let held = |m: &str, rest: &str| format!("{} held · {rest}", modifier_label(m));
        [
            held(
                &k.tab_switch_modifier,
                "number keycap replaces each tab icon, in place",
            ),
            held(
                &k.workspace_switch_modifier,
                "keycap replaces status dot / letter avatar",
            ),
            held(
                &k.category_switch_modifier,
                "keycap right-aligned on headers / centered on rail --- \
                 (mutually exclusive with the workspace axis)",
            ),
        ]
    })
}

/// 카탈로그 1차 분류 = 문서 페이지 하나. 상단 crumb + 좌측 nav Catalog 그룹에 사용.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// 토큰·기초 (색/타입/간격/형태/스케일).
    Foundations,
    /// 위젯·컴포넌트 (단일 UI primitive).
    Components,
    /// 공통 아이콘.
    Icons,
    /// 모달·팝업 레이어.
    Overlays,
    /// 구조 셸 (사이드바/탭바/분할/하이라이트).
    Layouts,
    /// 플러그인 유래 컴포넌트 (네이티브와 분리된 플러그인 전용 섹션).
    Plugins,
    /// 부팅 로딩 등 앱 화면 전체를 보여주는 예제.
    Chrome,
}

impl Category {
    /// 페이지 라벨 (crumb / nav 링크 lbl).
    pub fn label(self) -> &'static str {
        match self {
            Category::Foundations => "Foundations",
            Category::Components => "Components",
            Category::Icons => "Icons",
            Category::Overlays => "Overlays",
            Category::Layouts => "Layouts",
            Category::Plugins => "Plugins",
            Category::Chrome => "Chrome",
        }
    }

    /// 탐색 링크 옆에 표시할 짧은 설명.
    pub fn desc(self) -> &'static str {
        match self {
            Category::Foundations => "tokens",
            Category::Components => "primitives",
            Category::Icons => "glyphs",
            Category::Overlays => "modals",
            Category::Layouts => "shells",
            Category::Plugins => "plugins",
            Category::Chrome => "app chrome",
        }
    }

    /// 페이지 헤더 intro 산문 (pagehead `p`).
    pub fn intro(self) -> &'static str {
        match self {
            Category::Foundations => {
                "The token layer — surface ramp, text hierarchy, accent roles, type, \
                 the 4px spacing grid, shape and motion. Everything else is built from these."
            }
            Category::Components => {
                "Single-purpose primitives — buttons, chips, form controls, navigation rows, \
                 status feedback. Each maps to one Theme-driven widget."
            }
            Category::Icons => {
                "The canonical glyph set — 24×24, 2px stroke, round caps, no fill, currentColor. \
                 One family across every surface."
            }
            Category::Overlays => {
                "Modal and popup layers — palettes, dialogs, pickers, agent approval. \
                 Scrim, frame, and lift composed from semantic tokens."
            }
            Category::Layouts => {
                "Structural shells — sidebars, tab strips, list→detail, dividers, surface focus. \
                 How panes and workspaces are framed."
            }
            Category::Plugins => {
                "UI contributed by plugins, kept apart from the native surfaces — clipboard and \
                 git viewers, plus the markdown / image / html content surfaces. Each ships with \
                 its plugin and is themed by the host."
            }
            Category::Chrome => {
                "Fully assembled app-chrome screens, not single widgets — the loading screen \
                 the host shows before the first real frame and again while it shuts down, \
                 with its window-size, phase-text and theme variants."
            }
        }
    }

    /// pagehead 의 HowTo 3컬럼 배너 노출 여부 (디자인은 Foundations 만 `howto:true`).
    pub fn howto(self) -> bool {
        matches!(self, Category::Foundations)
    }

    pub fn all() -> &'static [Category] {
        &[
            Category::Foundations,
            Category::Components,
            Category::Icons,
            Category::Overlays,
            Category::Layouts,
            Category::Plugins,
            Category::Chrome,
        ]
    }
}

/// 카탈로그 한 항목 — 한 specimen 의 헤딩 메타 + 라이브 데모 draw.
pub struct Spec {
    /// 앵커/스크롤 식별자.
    pub id: &'static str,
    /// 항목 제목 (h3).
    pub title: &'static str,
    /// "언제 쓰나" 한 줄 설명 (선택).
    pub when: Option<&'static str>,
    /// 라이브 데모 draw — `Theme` 만 받아 egui 위젯을 그린다.
    pub draw: fn(&mut egui::Ui, &Theme),
}

/// 페이지 내 한 구역 — nav "On this page" 앵커의 단위.
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub specs: Vec<Spec>,
}

/// 문서 페이지 하나 = 한 `Category`.
pub struct Page {
    pub category: Category,
    pub sections: Vec<Section>,
}

pub(crate) fn spec(
    id: &'static str,
    title: &'static str,
    when: Option<&'static str>,
    draw: fn(&mut egui::Ui, &Theme),
) -> Spec {
    Spec {
        id,
        title,
        when,
        draw,
    }
}

pub(crate) fn section(id: &'static str, title: &'static str, specs: Vec<Spec>) -> Section {
    Section { id, title, specs }
}

/// 예제가 하나이고 구역 id와 예제 id가 같은 구역.
fn single(
    id: &'static str,
    section_title: &'static str,
    title: &'static str,
    when: Option<&'static str>,
    draw: fn(&mut egui::Ui, &Theme),
) -> Section {
    section(id, section_title, vec![spec(id, title, when, draw)])
}

/// 페이지별 구역과 예제 목록.
pub fn pages() -> Vec<Page> {
    vec![
        // ── Foundations ──────────────────────────────────────────────
        Page {
            category: Category::Foundations,
            sections: vec![
                single(
                    "elevation",
                    "Color — elevation (surface ramp)",
                    "Depth reads through surface tint; shadow is for floating surfaces only",
                    Some(
                        "bg-app → sidebar → panel → surface-raised, one tint step apart — \
                         lift is popover / modal / none, by surface shape",
                    ),
                    theme::elevation,
                ),
                single(
                    "floating",
                    "Elevation — floating surfaces (the two shadows)",
                    "Two shadows, and the rule that picks one",
                    Some(
                        "anchored + scrim-less → shadow-popover · centered + scrim-backed → shadow-modal",
                    ),
                    foundations_settled::draw_shadows,
                ),
                section(
                    "text",
                    "Color — text",
                    vec![
                        spec(
                            "text",
                            "Hierarchy by text color, on any surface",
                            Some("primary → secondary → muted → disabled → placeholder"),
                            theme::text,
                        ),
                        spec(
                            "disabled-ink",
                            "Disabled ink — no contrast target, Latte one step up",
                            Some(
                                "Order placeholder < disabled < muted, Latte remaps to n800 · disabled controls take this ink with no opacity",
                            ),
                            foundations_disabled_ink::draw,
                        ),
                    ],
                ),
                single(
                    "accents",
                    "Color — accent roles",
                    "Accents map to roles, not decoration",
                    Some("primary · info · success · warning · danger · agent"),
                    theme::accents,
                ),
                single(
                    "terminal",
                    "Color — terminal / ANSI palette",
                    "The colors a terminal cell paints with — not UI chrome",
                    Some("ANSI 16 (SGR 30–37 / 90–97) + selection · vi cursor · search fills"),
                    theme::terminal,
                ),
                single(
                    "type",
                    "Type",
                    "Two families, hard 14px cap, hierarchy by weight",
                    Some("heading 13/600 · body 13 · caption 11 · mono 14"),
                    typography::draw,
                ),
                single(
                    "spacing",
                    "Spacing — the 4px grid, in use",
                    "Five steps, each with a job",
                    Some("xs chip · sm pair · md card · lg column · xl region"),
                    spacing::draw,
                ),
                single(
                    "shape",
                    "Radius · border · motion",
                    "Crisp and rectilinear — it's a terminal",
                    Some("radius 4/2 · 1px border · UI 90–120ms · terminal 0ms"),
                    foundations_shape::draw,
                ),
                single(
                    "uiscale",
                    "UI scale — sidebar zoom",
                    "One multiplier scales the sidebar; everything else stays fixed",
                    Some("stops 0.8 / 1.0 / 1.2 — sidebar root zoom only"),
                    foundations_uiscale::draw,
                ),
                section(
                    "rolegaps",
                    "Role gaps — settled",
                    vec![
                        spec(
                            "role-colors",
                            "Colors that had no role of their own",
                            Some(
                                "C1–C7 · four new roles, three moves to the role they should have used",
                            ),
                            foundations_rolegaps::draw_role_colors,
                        ),
                        spec(
                            "c1-correction",
                            "C1 correction — the popup frame does move, and should (settled)",
                            Some(
                                "popup frame border-strong → border-frame · pane divider unchanged",
                            ),
                            foundations_rolegaps::draw_c1_correction,
                        ),
                        spec(
                            "half-pixel-type",
                            "Half-pixel type sizes snap to the scale",
                            Some("read text snaps up · numeric micro-labels snap down"),
                            foundations_settled::draw_half_pixel,
                        ),
                        spec(
                            "tint-recipe",
                            "One tinted-box recipe: 12% fill, 36% edge",
                            Some(
                                "tint-fill-alpha 0.12 · tint-border-alpha 0.36 · fill-only and border-only uses",
                            ),
                            foundations_settled::draw_tint,
                        ),
                        spec(
                            "structural-dimensions",
                            "Structural dimensions — the leftovers",
                            Some(
                                "icon-size-lg 24 · toolbar-height 32 · popup default sizes stay outside tokens",
                            ),
                            foundations_settled::draw_structural,
                        ),
                    ],
                ),
            ],
        },
        // ── Components ───────────────────────────────────────────────
        Page {
            category: Category::Components,
            sections: vec![
                section(
                    "buttons",
                    "Buttons",
                    vec![
                        spec(
                            "button",
                            "Button",
                            Some(
                                "Primary action and its variants — primary, secondary, ghost, danger, agent",
                            ),
                            components::prim_button::draw,
                        ),
                        spec(
                            "button-disabled",
                            "Disabled — ink, never opacity",
                            Some(
                                "Every variant draws the same neutral box with the disabled ink; accent fills drop out",
                            ),
                            components::prim_button::draw_disabled,
                        ),
                        spec(
                            "icon-button",
                            "IconButton",
                            Some("Square icon-only control for toolbars and row affordances"),
                            components::prim_icon_button::draw,
                        ),
                    ],
                ),
                section(
                    "chips",
                    "Badge · Tag · Kbd",
                    vec![
                        spec(
                            "badge",
                            "Badge",
                            Some("Count or short status pill"),
                            components::prim_chips::draw_badge,
                        ),
                        spec(
                            "tag",
                            "Tag",
                            Some("Outlined label for surface kind or state"),
                            components::prim_chips::draw_tag,
                        ),
                        spec(
                            "kbd",
                            "Kbd",
                            Some("Keyboard shortcut keycaps"),
                            components::prim_chips::draw_kbd,
                        ),
                        spec(
                            "selected-row",
                            "Selected row",
                            Some("Meaning glyphs, chips, match run and Kbd + on surface-active, Mocha and Latte"),
                            components::selected_row::draw,
                        ),
                        spec(
                            "kbd-one-keycap",
                            "One keycap, no palette variant",
                            Some("the palette converges on Kbd — 16 / 4 / 4 / 10"),
                            components_settled::draw_one_keycap,
                        ),
                        spec(
                            "glyph-sizes",
                            "Glyph sizes are icons, not type",
                            Some("12 → icon-size-xs · 16 → icon-size-md · clipboard 30 → 28"),
                            components_settled::draw_glyph_sizes,
                        ),
                    ],
                ),
                section(
                    "forms",
                    "Form controls",
                    vec![
                        spec(
                            "input",
                            "Input",
                            Some("Single-line text field — icon, addon, mono, invalid, block"),
                            components::prim_input::draw,
                        ),
                        spec(
                            "codearea",
                            "CodeArea",
                            Some(
                                "Multi-line mono field with a line-number gutter — error line, empty, disabled",
                            ),
                            components::prim_code_area::draw,
                        ),
                        spec(
                            "forms",
                            "Select · Checkbox · Switch",
                            Some(
                                "Select = native dropdown styled like Input · Checkbox = standalone boolean (the MultiSelect option row) · Switch = boolean that applies immediately",
                            ),
                            components::prim_forms::draw,
                        ),
                        spec(
                            "multiselect",
                            "MultiSelect — many values in one control",
                            Some(
                                "Select sibling for more than one value — closed it is a Select, open a Checkbox menu that stays open; 3-branch plain-text summary",
                            ),
                            components::prim_multiselect::draw,
                        ),
                        spec(
                            "searchable-select",
                            "SearchableSelect — Select with a search field in its open list",
                            Some(
                                "Select variant for one value out of a long list — closed it is a Select, open a menu popover with a search field, pinned rows, filtered MenuItem rows and no-match / loading status rows",
                            ),
                            components::prim_searchable_select::draw,
                        ),
                        spec(
                            "autocomplete",
                            "AutoComplete",
                            Some(
                                "Free-text trigger + candidate dropdown (typeahead) — substring filter, match highlight, max-height scroll",
                            ),
                            components::prim_autocomplete::draw,
                        ),
                        spec(
                            "path-field",
                            "PathField",
                            Some(
                                "Shared address-bar path field — AutoComplete trigger + Go button, edit/navigate/revert",
                            ),
                            components::prim_path_field::draw,
                        ),
                        spec(
                            "text-caret",
                            "Text caret — steady",
                            Some(
                                "The caret in every UI text field is steady: drawn while the field and the window are focused, never blinking — matches the terminal cursor and keeps an idle focused field from waking the frame loop",
                            ),
                            components::prim_text_caret::draw,
                        ),
                    ],
                ),
                section(
                    "plugin-settings",
                    "Plugin settings page",
                    vec![
                        spec(
                            "plugin-settings",
                            "Plugin-contributed settings rows",
                            Some("label · control rows — toggle / select / number"),
                            components::plugin_settings::draw,
                        ),
                        spec(
                            "settings-number",
                            "Numbers in settings — one shape",
                            Some(
                                "mono Input + static suffix, clamp on commit — default / out of range / disabled",
                            ),
                            components::settings_number::draw,
                        ),
                    ],
                ),
                section(
                    "nav",
                    "Tab · TreeRow · MenuItem · DrillDown",
                    vec![
                        spec(
                            "tab",
                            "Tab",
                            Some("A surface tab and its status — active, idle, notification"),
                            components::prim_tab::draw,
                        ),
                        spec(
                            "tree-row",
                            "TreeRow",
                            Some("Disclosure row in the sidebar tree"),
                            components::prim_nav::draw_tree_row,
                        ),
                        spec(
                            "menu-item",
                            "MenuItem",
                            Some("Row in a context or command menu"),
                            components::prim_nav::draw_menu_item,
                        ),
                        spec(
                            "menu-item-selected",
                            "MenuItem — selected option",
                            Some(
                                "The current value inside an open Select-style list (settings \
                                 dropdowns, egui ComboBox): text-primary ink and a trailing accent \
                                 check, no fill. Fills keep their two jobs — pointer hover and \
                                 keyboard-active — so a selected row under the pointer shows both.",
                            ),
                            components::prim_nav::draw_menu_item_selected,
                        ),
                        spec(
                            "drilldown",
                            "One area, swapped — list → detail → back",
                            Some(
                                "When the model is \"one full-width list, pick an item, see its \
                                 detail, go back\" — and a side-by-side split would starve both — \
                                 the content area swaps in place. The list view (a full-width \
                                 ListCtrl) becomes a detail view the moment you select a row; a \
                                 pinned back bar (← + title) returns you. This is a generic \
                                 DrillDown layout — its first home is Settings › Keybindings › \
                                 Preset (preset list → diff preview), replacing the old cramped \
                                 120px-list + right-preview split. Select a preset below, then \
                                 press ←.",
                            ),
                            components::prim_drilldown::draw,
                        ),
                    ],
                ),
                section(
                    "feedback",
                    "StatusDot · Status resolution · Spinner · Toast · Toast stack · Toast view · HelpHint",
                    vec![
                        spec(
                            "status-dot",
                            "StatusDot",
                            Some("One dot for a surface's live state"),
                            components::prim_status_dot::draw,
                        ),
                        spec(
                            "status-resolution",
                            "Status resolution",
                            Some("How owner and activity collapse to a single dot"),
                            components::prim_status_resolution::draw,
                        ),
                        spec(
                            "dot-family",
                            "The dot family — 8 generic, 6 in dense chrome, and the attached ring",
                            Some("status-dot-size 8 · status-dot-size-compact 6 · ring 2 / 2"),
                            components_settled::draw_dot_family,
                        ),
                        spec(
                            "attach-loading",
                            "Attach loading overlay and response wait",
                            Some("Paint-only slots; compact response count and oldest elapsed time"),
                            components::attach_loading::draw,
                        ),
                        spec(
                            "attach-prelayout",
                            "Attach pre-layout — normal tabs and both split axes",
                            Some("Next frame layout; spinner on/off leaves geometry unchanged"),
                            components::attach_loading::draw_prelayout,
                        ),
                        spec(
                            "spinner",
                            "Spinner",
                            Some("Indeterminate progress — sizes and reduced-motion fallback"),
                            components::prim_spinner::draw,
                        ),
                        spec(
                            "toast",
                            "Toast",
                            Some("Transient notification card"),
                            widgets::toast::draw,
                        ),
                        spec(
                            "toast-stack",
                            "Toast stack",
                            Some("Bottom-right stack, newest at the bottom, oldest dropped past 5"),
                            widgets::toast::draw_stack,
                        ),
                        spec(
                            "toast-view",
                            "Toast view",
                            Some("draw_toast_scopes — scope stack, body wrap, alpha fade"),
                            components::toast::draw,
                        ),
                        spec(
                            "help-hint",
                            "HelpHint · Tooltip",
                            Some("Inline (?) glyph + hover tooltip bubble"),
                            components::prim_help_hint::draw,
                        ),
                    ],
                ),
                section(
                    "centerstate",
                    "CenterState — empty · loading · error",
                    vec![
                        spec(
                            "center-state",
                            "One centred block for every empty list",
                            Some(
                                "loading · empty · error × file picker / Settings Scripts — glyph 24, sub slot always reserved",
                            ),
                            components::prim_center_state::draw,
                        ),
                        spec(
                            "center-state-action",
                            "Error glyph and the action slot",
                            Some(
                                "alertTriangle is part-owned · the action hangs 12 below the sub slot, outside the centring",
                            ),
                            components::prim_center_state::draw_action_slot,
                        ),
                        spec(
                            "center-state-unsized",
                            "Unsized host + action — symmetric natural height",
                            Some(
                                "no host height · with an action the part reserves 48 above and below, the action ends 12 above the edge",
                            ),
                            components::prim_center_state::draw_unsized,
                        ),
                    ],
                ),
                section(
                    "text",
                    "Hint text",
                    vec![spec(
                        "hint",
                        "Hint text",
                        Some("Helper text below a field"),
                        widgets::hint_text::draw,
                    )],
                ),
                single(
                    "warning-callout",
                    "Warning callout",
                    "Warning callout",
                    Some("Bordered warning tint box — icon + caption under a risky toggle"),
                    widgets::warning_callout::draw,
                ),
                section(
                    "data",
                    "Table · ListCtrl",
                    vec![
                        spec(
                            "table",
                            "Table",
                            Some("Sticky-header data grid — the shared Table widget"),
                            components::prim_table::draw,
                        ),
                        spec(
                            "listctrl",
                            "ListCtrl",
                            Some(
                                "Row-selectable navigation list — pick one to drill into (pair with DrillDown)",
                            ),
                            components::prim_listctrl::draw,
                        ),
                        spec(
                            "listctrl-disabled-trailing",
                            "Disabled row with a trailing marker — the ink rule",
                            Some(
                                "Ink, never opacity — the chevron goes, the trailing Tag / Badge stays in its disabled variant",
                            ),
                            components::prim_listctrl::draw_disabled_trailing,
                        ),
                        spec(
                            "keyboard-active",
                            "Keyboard-active row",
                            Some(
                                "Selected fill and inks without the bar. A disabled row has no chevron",
                            ),
                            components::prim_listctrl::draw_keyboard_active,
                        ),
                    ],
                ),
                single(
                    "segmented",
                    "Segmented control",
                    "Segmented",
                    Some("Mutually-exclusive toggle — explorer's grid/list/detail switch"),
                    components::segmented::draw,
                ),
                single(
                    "explorer-cells",
                    "Explorer view cells",
                    "Explorer view cells",
                    Some("grid cell (new) · list row (tree_row) · detail (Table + sort header)"),
                    components::explorer_view_cells::draw,
                ),
                single(
                    "explorer-toolbar",
                    "Explorer toolbar",
                    "Address bar + view-mode toggle",
                    Some(
                        "surface-raised address box (clipped crumbs) + grid/list/detail icon toggle",
                    ),
                    components::explorer_toolbar::draw,
                ),
                components::explorer_ops::create_section(),
                components::explorer_ops::search_section(),
                section(
                    "explorer-sidebar",
                    "Explorer sidebar",
                    vec![
                        spec(
                            "explorer-sidebar-favorites",
                            "Sidebar Favorites — populated vs. empty state",
                            Some("caption always shown · faint star + caption + hint when empty"),
                            components::explorer_sidebar::draw_favorites,
                        ),
                        spec(
                            "explorer-sidebar",
                            "Sidebar layout — Favorites PINNED to the bottom (2-region split)",
                            Some(
                                "Files scrolls on top · Favorites pinned at a computed height · fixed 1px boundary",
                            ),
                            components::explorer_sidebar::draw,
                        ),
                        spec(
                            "explorer-sidebar-short-cell",
                            "Short cell — Favorites drops below 240, the cell stops at 180",
                            Some(
                                "Files only below a 240 body · split drag floor 180 (explorer-min-height) · compact state row below 120",
                            ),
                            components::explorer_sidebar::draw_short_cell,
                        ),
                    ],
                ),
                section(
                    "explorer-file-info",
                    "Explorer properties and preview",
                    vec![
                        spec(
                            "explorer-properties",
                            "Properties — popup for one item · folder · several · remote",
                            Some(
                                "360 popup scoped to the cell · keeps its item · 96 caption labels",
                            ),
                            components::explorer_file_info::draw_properties,
                        ),
                        spec(
                            "explorer-properties-actions",
                            "Properties actions — time format · Open · permissions",
                            Some(
                                "time fields per format · Open footer as Enter · permission editor under the row",
                            ),
                            components::explorer_props_actions::draw,
                        ),
                        spec(
                            "explorer-preview",
                            "Preview panel — text · image · other · loading · too large · unreadable",
                            Some("toggle · right side 288 (splitter 200 … 460) · 40 header"),
                            components::explorer_file_info::draw_preview,
                        ),
                        spec(
                            "explorer-thumbnails",
                            "Grid thumbnails — 40 slot for every cell",
                            Some("Grid only · local only · glyph in accent-info while loading"),
                            components::explorer_file_info::draw_thumbnails,
                        ),
                    ],
                ),
                single(
                    "layout-shell",
                    "Layout shell widgets",
                    "two-depth panel · overflow tab bar · content frame",
                    Some("tasty-ui-widgets 공용 함수를 직접 호출한다 (복제 아님 — demo=main)"),
                    components::prim_layout_shell::draw,
                ),
            ],
        },
        // ── Icons ────────────────────────────────────────────────────
        Page {
            category: Category::Icons,
            sections: vec![
                single(
                    "system-rules",
                    "The icon system",
                    "One geometry, recolored by context",
                    Some(
                        "24×24 viewBox · 2px stroke round · no fill · currentColor — sized via prop (26/20/16/14/12)",
                    ),
                    icons::draw_system_rules,
                ),
                single(
                    "keys-in-use",
                    "Modifier symbols in use",
                    "Keycap chip · settings display style",
                    Some(
                        "Two places consume these glyphs. In a keycap chip (modifier-hint \
                         header, switch overlays) the glyph replaces the key's text inside \
                         the same Kbd cap — 14px, so its optical weight matches the 12px mono \
                         label it stands in for. In the Settings › modifier display style \
                         dropdown, the closed trigger shows the glyph alone (it is a preview \
                         of the keycap), while each open option row pairs glyph + text label \
                         so the choice is never ambiguous.",
                    ),
                    icons_keys::draw,
                ),
                single(
                    "actions",
                    "Actions",
                    "The verbs",
                    Some(
                        "What an IconButton wraps in a toolbar or row — close/refresh in every overlay header, edit/trash/copy in list rows",
                    ),
                    icons::draw_actions,
                ),
                single(
                    "nav",
                    "Navigation & disclosure",
                    "Movement and open/closed state",
                    Some(
                        "Single chevrons = tree-row disclosure; doubled = collapse/expand the sidebar rail",
                    ),
                    icons::draw_nav,
                ),
                single(
                    "surfaces",
                    "Surfaces & workspace",
                    "The nouns of the workspace",
                    Some(
                        "What a tab, tree row, or new-surface button shows — terminal/markdown are the two core surface kinds",
                    ),
                    icons::draw_surfaces,
                ),
                single(
                    "view",
                    "View modes & favorites",
                    "Explorer view switch & bookmark marker",
                    Some(
                        "grid / list / detail toggle glyphs + star — the explorer toolbar and favorites sidebar",
                    ),
                    icons::draw_view,
                ),
                single(
                    "visibility",
                    "Visibility",
                    "Reveal toggle on secret values",
                    Some(
                        "Passkeys, env — eye when hidden, eyeOff when shown; swap in place on the same IconButton",
                    ),
                    icons::draw_visibility,
                ),
                single(
                    "status",
                    "Status & alerts",
                    "Inline meaning markers",
                    Some(
                        "Tinted by the line they sit in (warning amber, success green, danger red) via currentColor — not state dots",
                    ),
                    icons::draw_status,
                ),
                single(
                    "system",
                    "Tools & system",
                    "Sidebar footer & global tools",
                    Some("Each anchors a menu or window — tools, settings, plug, rocket"),
                    icons::draw_system,
                ),
                single(
                    "keys",
                    "Modifier keys (macOS)",
                    "Command / Option / Shift symbols",
                    Some(
                        "Vector replacements for ⌘/⌥/⇧ — the settings display-style dropdowns and the modifier-hint keycap chip",
                    ),
                    icons::draw_keys,
                ),
            ],
        },
        // ── Overlays ─────────────────────────────────────────────────
        Page {
            category: Category::Overlays,
            sections: vec![
                section(
                    "scrim",
                    "Scrim & frame",
                    vec![
                        spec(
                            "scrim",
                            "Dismiss on scrim or Esc",
                            Some(
                                "The shared recipe — bg-panel frame, 1px border-strong, modal shadow, scrim + blur",
                            ),
                            widgets::dialog::draw,
                        ),
                        spec(
                            "drag-handles",
                            "Drag handles — TitleBar · Region · None",
                            Some("how a popup moves · the cursor is the only affordance"),
                            overlays_settled::draw_drag_handles,
                        ),
                        spec(
                            "resize-map",
                            "8-direction resize — handle map & cursors",
                            Some(
                                "resizable popups · a band inside the border · corners win · cursor only",
                            ),
                            overlays_resize::draw_resize_map,
                        ),
                        spec(
                            "resize-constraints",
                            "Constraints & z-order — quiet limits, top-most wins",
                            Some(
                                "min size · scope rect is the cap · only the front popup responds",
                            ),
                            overlays_resize::draw_constraints,
                        ),
                    ],
                ),
                components::webview_opaque::section(),
                section(
                    "scrim-scope",
                    "Scrim scope — window vs surface",
                    vec![
                        spec(
                            "scrim-scope",
                            "The scrim covers the popup's scope, not always the window",
                            Some(
                                "Surface-bound popups dim only their own surface — border in, neighbours out",
                            ),
                            components::scrim_scope::draw_scope,
                        ),
                        spec(
                            "scrim-scope-child",
                            "Parent and child share one scrim",
                            Some(
                                "A child picker inherits the scope — the scrim is painted once, never stacked",
                            ),
                            components::scrim_scope::draw_child,
                        ),
                    ],
                ),
                section(
                    "fullscreen-stage",
                    "Fullscreen stage — the window-wide surface",
                    vec![
                        spec(
                            "fullscreen-stage",
                            "Shell: scrim, title, exit button",
                            Some(
                                "Separate content in the popup's shape — the original popup stays open behind it",
                            ),
                            components::fullscreen_stage::draw,
                        ),
                        spec(
                            "fullscreen-stage-titlebar",
                            "Entry point — the popup title bar button",
                            Some(
                                "Only a popup that declares a stage gets it · 24px fit button left of the X",
                            ),
                            components::fullscreen_stage::draw_titlebar,
                        ),
                    ],
                ),
                section(
                    "banner",
                    "Banner — the floating top notice",
                    vec![
                        spec(
                            "banner-shell",
                            "Banner shell — the 4th overlay family",
                            Some(
                                "Floats at content-top below the tab bar · 100% − 16px · 8px radius · user-action only",
                            ),
                            widgets::banner::draw,
                        ),
                        spec(
                            "banner-dismiss",
                            "Dismiss & TTL countdown",
                            Some(
                                "Top-right one slot — × hidden until hover · TTL seconds → × on hover",
                            ),
                            widgets::banner::draw_dismiss,
                        ),
                        spec(
                            "banner-lifetime",
                            "Lifetime — TTL follows whether the banner has an action",
                            Some(
                                "No action → TTL, auto-dismiss · has action → no TTL · × is not an action",
                            ),
                            widgets::banner::draw_lifetime,
                        ),
                        spec(
                            "banner-shell-integration",
                            "Shell integration missing — action-less, TTL 10 s",
                            Some(
                                "Surface scope · countdown in the dismiss slot · hover pauses and shows ×",
                            ),
                            widgets::banner::draw_shell_integration,
                        ),
                        spec(
                            "banner-stack",
                            "Queue & stacking",
                            Some(
                                "One per scope, rest queue (max 5) · lower scope dimmed to 40% behind",
                            ),
                            widgets::banner::draw_stack,
                        ),
                        spec(
                            "banner-anatomy",
                            "Anatomy & states",
                            Some(
                                "The canonical use: a TUI app turns on mouse tracking (DECSET \
                                 1000/1002/1003), so drag-to-select and right-click stop working. \
                                 The banner explains why and the bypass. It is persistent (no TTL \
                                 — it doesn't time out), fires once per tracking session and only \
                                 on a real user click (never from agent/IPC). The top-right × is \
                                 hidden until you hover; dismissing it suppresses the banner for \
                                 that session. Leading glyph = a mouse (open decision resolved: \
                                 keep it, in banner-icon-fg tone). Card height is variable — body \
                                 wraps to 1–3 lines depending on locale.",
                            ),
                            widgets::banner_mouse_capture::draw_anatomy,
                        ),
                        spec(
                            "banner-hit-zone",
                            "Position & hit-zone",
                            Some(
                                "Card rect consumes the mouse · the surface body below passes clicks through",
                            ),
                            widgets::banner::draw_hit_zone,
                        ),
                        spec(
                            "banner-blacklist",
                            "Capture blacklist (Settings › Terminal)",
                            Some(
                                "List editor — rows (pattern + ×) + Add field · empty state is neutral",
                            ),
                            widgets::banner::draw_blacklist,
                        ),
                        spec(
                            "banner-more-menu",
                            "\"More\" (⋯) context menu",
                            Some(
                                "⋯ left of × · hover-revealed, stays + active while open · suppress banner / disable capture",
                            ),
                            widgets::banner::draw_more_menu,
                        ),
                        spec(
                            "banner-more-elastic",
                            "Elastic width — the interpolated program name",
                            Some(
                                "Every row interpolates a program name of arbitrary length, and \
                                 word order differs by locale (en: name last; ko/ja: name first; \
                                 zh-Hans: name in the middle). \
                                 So a row's label is two parts: the fixed text (never truncates) \
                                 and the program name in mono (same row ink as the fixed text: \
                                 menu-item-fg at rest, menu-item-fg-hover on hover), which is the \
                                 part that shrinks and ellipsises. The menu grows with the content \
                                 between 200px and 288px (a wider band than the Tools menu's \
                                 160–240, since every row carries a program name) and the full \
                                 name is available as the row's tooltip. When the fixed text alone \
                                 does not fit at 288 (ja), that row wraps instead and grows.",
                            ),
                            widgets::banner_mouse_capture::draw_elastic,
                        ),
                        spec(
                            "banner-more-row-copy",
                            "Row copy — en / ko / ja / zh-Hans",
                            Some(
                                "Each row names the program and what changes for it: \u{201c}show \
                                 again\u{201d} for the notice, \u{201c}clicks\u{201d} for capture. \
                                 Each row = prefix + {app} (mono) + suffix; en uses the prefix \
                                 only, ko/ja the suffix only, zh-Hans both. Copy sits on one side \
                                 or both, never empty on both. Width band 200–288 is unchanged; \
                                 with this copy no row wraps for a short name in any locale.",
                            ),
                            widgets::banner_mouse_capture::draw_row_copy,
                        ),
                        spec(
                            "banner-more-unknown-name",
                            "Program name unknown — rows shown disabled",
                            Some(
                                "When the program name is unknown (before the first 1 Hz poll or, \
                                 on a remote attach mirror, before the server's first name; OS lookup \
                                 failed; a mirror of an older server that sends no name) the actions \
                                 still exist but can't run, so they are shown disabled, not hidden. \
                                 The ⋯ trigger is drawn exactly as with a known name. The menu opens \
                                 with a non-interactive reason line, a separator, then the same two \
                                 rows disabled, worded with \u{201c}this program\u{201d}. Clicking a \
                                 disabled row does nothing; Esc / outside click close as usual. If \
                                 the name arrives while the menu is open the menu does not change; \
                                 the next open shows the normal rows. A remote mirror uses the \
                                 program name its server sends.",
                            ),
                            widgets::banner_mouse_capture::draw_unknown_name,
                        ),
                    ],
                ),
                section(
                    "htmlscript",
                    "HTML script notice — the first inset banner",
                    vec![
                        spec(
                            "html-script-placement",
                            "Placement — inset, scoped to one surface",
                            Some(
                                "Same shell, margin 8 on all four sides · the WebView rect starts below it · the terminal beside it is untouched",
                            ),
                            widgets::html_script_banner::draw_placement,
                        ),
                        spec(
                            "html-script-states",
                            "Banner states",
                            Some(
                                "lock in accent-info · one Secondary/Sm action · reloading swaps the action for a spinner and hides ×",
                            ),
                            widgets::html_script_banner::draw_states,
                        ),
                        spec(
                            "html-script-banner-button",
                            "A button on the banner shell",
                            Some(
                                "Secondary inside any banner shell goes one ramp step up · fill surface-hover · edge border-frame · plugin banners too: the platform opens the context for the whole frame",
                            ),
                            widgets::html_script_banner::draw_banner_button,
                        ),
                        spec(
                            "html-script-markers",
                            "After × and after Allow — the tab-strip marker",
                            Some(
                                "lock (glyph-dim) re-shows the banner on click · script (text-muted) is tooltip only",
                            ),
                            widgets::html_script_banner::draw_markers,
                        ),
                        spec(
                            "html-script-load-failed",
                            "Load failed — no banner, no marker",
                            Some(
                                "the host chrome's Failed to load state takes the body · banner and tab marker are removed until a reload commits",
                            ),
                            widgets::html_script_banner::draw_load_failed,
                        ),
                    ],
                ),
                section(
                    "htmlremote",
                    "HTML remote content notice",
                    vec![
                        spec(
                            "html-remote-states",
                            "Banner states",
                            Some(
                                "globeOff in accent-info · one Secondary/Sm action \"Allow remote content\" · same states as the script notice",
                            ),
                            widgets::html_remote_banner::draw_states,
                        ),
                        spec(
                            "html-remote-markers",
                            "The address-field marker — always findable",
                            Some(
                                "end of the address field, after the skipped Tag · globeOff re-shows the notice on click · globe is tooltip only",
                            ),
                            widgets::html_remote_banner::draw_markers,
                        ),
                        spec(
                            "html-remote-variants",
                            "Surface variants 1–10 · stacking with the script and size notices",
                            Some(
                                "address band → size → remote → script → WebView · two separate cards, never merged",
                            ),
                            widgets::html_remote_banner::draw_variants,
                        ),
                    ],
                ),
                single(
                    "attachrefusal",
                    "Auto-attach refused",
                    "Persistent row mark + workspace banner, no toast",
                    Some(
                        "alertTriangle on the sidebar row and the rail avatar · Workspace banner with Remove mapping and ×",
                    ),
                    widgets::attach_refusal::draw,
                ),
                single(
                    "attachsizesync",
                    "Attach size sync failed",
                    "Workspace banner · default · retrying · several surfaces",
                    Some(
                        "alertTriangle · title · two muted body lines (names · fixed copy) · Retry (Retry all for several surfaces) and ×",
                    ),
                    widgets::attach_size_sync::draw,
                ),
                single(
                    "palette",
                    "Command palette",
                    "Top-anchored, fuzzy, keyboard-first",
                    Some("540px · surface-raised · spawns under the title bar"),
                    components::command_palette::draw,
                ),
                single(
                    "tools",
                    "Tools menu",
                    "Tools menu — anchored, no scrim",
                    Some(
                        "content-width popover (160–240) · built-in tools, then plugin tools · no icons",
                    ),
                    components::tools_menu::draw,
                ),
                section(
                    "ports",
                    "Listening ports",
                    vec![
                        spec(
                            "ports",
                            "Live listeners, copy address",
                            Some("660×520 · 7-column table · sticky header"),
                            components::port_scanner::draw,
                        ),
                        spec(
                            "ports-process-column",
                            "Process column — a minimum, not a fixed width (on Table)",
                            Some(
                                "Drawn on the shared Table with the popup's own columns · Process has a 200 floor and takes all the spare width · Address is fixed at its 140 floor · fixed columns never shrink · below the column budget the body scrolls sideways · no column hides · Process text ellipsises, the PID Tag stays",
                            ),
                            components::port_scanner::draw_process_column,
                        ),
                    ],
                ),
                section(
                    "remote",
                    "Remote connections",
                    vec![
                        spec(
                            "remote",
                            "Profiles & passkeys",
                            Some("520×460 · three tabs · SSH targets, identity at the boundary"),
                            components::remote::draw,
                        ),
                        spec(
                            "remote-filter",
                            "Protocol filter — button & dropdown",
                            Some("add-bar funnel · checkbox list · apply-on-confirm"),
                            components::remote::draw_filter,
                        ),
                        spec(
                            "remote-passkeys",
                            "Passkeys tab — rows, reveal, unknown kind",
                            Some(
                                "right tab · hidden / revealed (active + eyeOff) / long revealed path / unknown kind · Mocha · Latte",
                            ),
                            components::remote::draw_passkeys,
                        ),
                        spec(
                            "remote-attach",
                            "Attach tab — tasty-attach targets",
                            Some("middle tab · ref/inline targets · remote tasty + port discovery"),
                            components::remote::draw_attach,
                        ),
                        spec(
                            "remote-attach-form",
                            "Attach form — reference vs. inline",
                            Some(
                                "Connection toggle · ssh_ref dropdown / inline fieldset · Remote tasty group",
                            ),
                            components::remote::draw_attach_form,
                        ),
                        spec(
                            "remote-profile-form",
                            "Profile form — SSH (the [112px · 1fr] row grid)",
                            Some(
                                "add route inside the window · fixed 112 label column · pinned footer",
                            ),
                            components::remote::draw_profile_form,
                        ),
                        spec(
                            "remote-generic-passkey-forms",
                            "Generic & Passkey forms · badges",
                            Some("key-value fields · Unknown type badge · passkey path / inline"),
                            components::remote::draw_generic_passkey_forms,
                        ),
                        spec(
                            "remote-segment-rule",
                            "Segmented active is an accent fill — tab strips keep the underline",
                            Some(
                                "tab strip = 2px accent underline (view) · segmented = accent-primary fill + text-on-accent (value) · surface-active stays row selection",
                            ),
                            components::remote::draw_segment_rule,
                        ),
                    ],
                ),
                section(
                    "remote-workspace-attach",
                    "Add remote workspace",
                    vec![
                        spec(
                            "remote-workspace-attach",
                            "Two-pane picker — loaded",
                            Some(
                                "680×460 · attach profiles → remote workspace list · '+ New workspace' first · mirror on Connect",
                            ),
                            components::remote_attach::draw,
                        ),
                        spec(
                            "remote-workspace-attach-new-row",
                            "'+ New workspace' row — rest / hover / selected / creating / failed",
                            Some(
                                "first row of the loaded list · 34px · create on the remote and mirror that",
                            ),
                            components::remote_attach::draw_new_row,
                        ),
                        spec(
                            "remote-workspace-attach-states",
                            "Right-pane states — initial / connecting / error / empty",
                            Some(
                                "centered states off the left selection · empty stays on the list path",
                            ),
                            components::remote_attach::draw_states,
                        ),
                        spec(
                            "remote-workspace-attach-empty-plans",
                            "Empty remote — plan A (center-state + CTA) vs plan B (list path)",
                            Some(
                                "§6-1 · plan B keeps the list path: caps header + one pre-selected new row + a muted line",
                            ),
                            components::remote_attach::draw_empty_plans,
                        ),
                        spec(
                            "remote-workspace-attach-decisions",
                            "Decisions — the eight open questions, resolved",
                            Some("What was chosen and why. This is the implementation spec."),
                            components::remote_attach::draw_decisions,
                        ),
                    ],
                ),
                section("filepicker", "File picker", file_picker_specs()),
                section(
                    "transfer",
                    "Remote file transfer",
                    vec![
                        spec(
                            "transfer-progress",
                            "Progress — determinate bar (system's first)",
                            Some(
                                "400px · scrim-centered · recessed 4px track + accent fill, 0ms · row-repeat for multiple files",
                            ),
                            components::transfer::draw,
                        ),
                        spec(
                            "transfer-failed",
                            "Failed — rejected (Dismiss) vs. mid-transfer (Retry)",
                            Some(
                                "danger glyph · mono reason well (command-well) · no danger-fill button",
                            ),
                            components::transfer::draw_error,
                        ),
                    ],
                ),
                single(
                    "search",
                    "Search bar",
                    "Headless, sticky, top-right",
                    Some("360px body width · 28px Input + padding and border · no scrim"),
                    components::search_bar::draw,
                ),
                section(
                    "workspace-categories",
                    "Workspace categories",
                    vec![
                        spec(
                            "sidebar-context-menu",
                            "Sidebar context menu — target resolves under the cursor",
                            Some(
                                "background · category header · reserved · workspace row · 176px min",
                            ),
                            components::sidebar_context_menu::draw,
                        ),
                        spec(
                            "rail-category",
                            "Collapsed rail — `---` category button + anchored popup",
                            Some(
                                "52px rail · each boundary is a --- button · popup to its right · name header + actions",
                            ),
                            components::category_dialogs::draw_rail,
                        ),
                        spec(
                            "workspace-categories",
                            "Sidebar folders — dialogs & rail popup",
                            Some(
                                "Create/rename (360px + inline validation) · delete confirm (380px danger) · rail popup (176px)",
                            ),
                            components::category_dialogs::draw,
                        ),
                    ],
                ),
                section(
                    "switch",
                    "Switch-number overlay",
                    vec![
                        spec(
                            "switch-tab",
                            "Tab switch — modifier held",
                            Some(switch_captions()[0].as_str()),
                            components::switch_overlay::draw_tab,
                        ),
                        spec(
                            "switch-ws",
                            "Workspace switch — modifier held",
                            Some(switch_captions()[1].as_str()),
                            components::switch_overlay::draw_workspace,
                        ),
                        spec(
                            "switch-cat",
                            "Category switch — modifier held",
                            Some(switch_captions()[2].as_str()),
                            components::switch_overlay::draw_category,
                        ),
                    ],
                ),
                section(
                    "modhint",
                    "Modifier hints",
                    vec![
                        spec(
                            "modhint-hold",
                            "Modifier hint panel — hold to reveal",
                            Some(
                                "floating, focus-less · 500ms hold → fade in · bottom-left default · release dismisses instantly",
                            ),
                            components::modifier_hint::draw_hold,
                        ),
                        spec(
                            "modhint-anatomy",
                            "Anatomy & chord ordering",
                            Some(
                                "chords containing the held modifier · size then Ctrl → Cmd/Alt → Option → Shift · keycap, role and plugin rows",
                            ),
                            components::modifier_hint::draw_anatomy,
                        ),
                        spec(
                            "modhint",
                            "Held-modifier shortcut panel",
                            Some(
                                "220×400 · hold 500ms → fade in · focus-less, mouse-interactive · release vanishes",
                            ),
                            components::modifier_hint::draw,
                        ),
                    ],
                ),
                single(
                    "approval",
                    "Agent approval",
                    "Review the command before it runs",
                    Some("440px · the command and its grants, verbatim"),
                    components::approval::draw,
                ),
                section(
                    "convert",
                    "Convert surface",
                    vec![
                        spec(
                            "convert",
                            "Convert popup — pick the kind to become",
                            Some(
                                "240 × ui_scale · shared title bar (× only) · one MenuItem per kind",
                            ),
                            components::convert::draw,
                        ),
                        spec(
                            "convert-shortcut",
                            "Shortcut chip — each row's letter as a trailing keycap",
                            Some(
                                "Kbd in the MenuItem slot · rest = hover = active · collision drops the chip · en · ko · ja",
                            ),
                            components::convert::draw_shortcut,
                        ),
                        spec(
                            "convert-narrow",
                            "Narrow popup, long title — width scales",
                            Some(
                                "ja · 1.2 before (literal 200) and after (240 × 1.2) · Mocha · Latte",
                            ),
                            components::convert::draw_narrow,
                        ),
                    ],
                ),
                section(
                    "filehandler",
                    "File handler picker",
                    vec![
                        spec(
                            "filehandler",
                            "Open with… — pick a handler",
                            Some("420px · built-in + plugin handlers, one list"),
                            components::file_handler_picker::draw,
                        ),
                        spec(
                            "filehandler-format",
                            "Detected format — one Tag in the header",
                            Some("accent Tag · “format unknown” when detection fails"),
                            components::file_handler_picker::draw_format,
                        ),
                        spec(
                            "filehandler-recent",
                            "Recent — a second group in the same list",
                            Some("Suggested → rule → Recent, one selection across both"),
                            components::file_handler_picker::draw_recent,
                        ),
                        spec(
                            "filehandler-when",
                            "Relative time — six words and then a date",
                            Some("just now → {n}d ago → YYYY-MM-DD · column reserved"),
                            components::file_handler_picker::draw_when,
                        ),
                        spec(
                            "filehandler-path-cut",
                            "Header path — cut whole segments, measured not counted",
                            Some("390px line box · …/ prefix · 65 only as a fallback"),
                            components::file_handler_picker::draw_path_cut,
                        ),
                        spec(
                            "file-handler-remote-path",
                            "Remote path — the host leads the path line",
                            Some(
                                "remote glyph + host (attach profile name) before the path · \
                             host capped at 160 · path front-elided in what the mark leaves",
                            ),
                            components::file_handler_picker::draw_remote_path,
                        ),
                        spec(
                            "filehandler-fallback",
                            "No suggestions — the whole catalog, one time only",
                            Some("All handlers in the attention tone + one-time strip"),
                            components::file_handler_picker::draw_fallback,
                        ),
                        spec(
                            "filehandler-empty",
                            "Empty — nothing to pick from",
                            Some("centered block · exit to Settings › Handlers"),
                            components::file_handler_picker::draw_empty,
                        ),
                        spec(
                            "filehandler-long",
                            "Long list — cap the height, show the cut",
                            Some("264px scroll area + 20px fade, chrome never scrolls"),
                            components::file_handler_picker::draw_long,
                        ),
                        spec(
                            "filehandler-headless",
                            "One header, not two",
                            Some("headless frame · the rejected titlebar pairing"),
                            components::file_handler_picker::draw_headless,
                        ),
                        spec(
                            "filehandler-footer",
                            "Footer — settled: Cancel / Open, nothing else",
                            Some("pure dispatcher — one-time open, nothing stored"),
                            components::file_handler_picker::draw_footer,
                        ),
                        spec(
                            "filehandler-rows",
                            "Rows — the icon and the name are derived, not stored",
                            Some("kind → glyph · id segment → name · origin + full id"),
                            components::file_handler_picker::draw_rows,
                        ),
                        spec(
                            "filehandler-default",
                            "Default Tag, and what the picker means now",
                            Some("ambiguous / explicit only · interaction contract"),
                            components::file_handler_picker::draw_default_tag,
                        ),
                    ],
                ),
                single(
                    "preset",
                    "Apply preset",
                    "Apply a saved layout",
                    Some("440px · Workspace / Tab / Pane scope"),
                    components::apply_preset::draw,
                ),
                section(
                    "preseteditor",
                    "Preset editor",
                    vec![
                        spec(
                            "presetview",
                            "PresetView — list → toolbar + live preview",
                            Some(
                                "The modeless window behind Tools › Presets. L1 tabs pick the scope \
                                 (Workspace / Tab / Pane); a left list of saved presets for that \
                                 scope feeds a right detail = a toolbar (rename · duplicate · \
                                 delete · Edit) over a live demo-layout preview. The selected row \
                                 uses the surface-fill + 2px accent left-bar (the file-handler / \
                                 sidebar idiom). Click a preset, switch its mini tabs, then hit \
                                 Edit — structure edits happen on the preview, a leaf's gear opens \
                                 its settings screen in this same column.",
                            ),
                            components::preset_view::draw,
                        ),
                        spec(
                            "preseteditor",
                            "Demo-layout preview — view and edit",
                            Some(
                                "Workspace / Tab / Pane · pane card + tab strip + surface hairline",
                            ),
                            components::preset_editor::draw,
                        ),
                        spec(
                            "preseteditor-settings",
                            "Surface settings screen — parameters as a draft",
                            Some("header 44 · scrolling form · fixed 52 footer (Cancel · OK)"),
                            components::preset_surface_settings::draw,
                        ),
                    ],
                ),
                section(
                    "markdown",
                    "Markdown open",
                    vec![
                        spec(
                            "markdown",
                            "Edit or preview",
                            Some("420px · two choice cards"),
                            components::markdown_open::draw,
                        ),
                        spec(
                            "markdown-large-file",
                            "Confirm large file",
                            Some("360px · size tag + Cancel/Open"),
                            components::md_large_file::draw,
                        ),
                    ],
                ),
                single(
                    "rename",
                    "Rename popup",
                    "One field, autofocused",
                    Some("360px · workspace / subtitle / tab / category — one shared view"),
                    components::rename_popup::draw,
                ),
                single(
                    "explorer-context",
                    "Explorer context menu",
                    "Right-click — four targets",
                    Some("empty · file · folder · multi-select — menu_item reuse"),
                    components::explorer_context_menu::draw,
                ),
                single(
                    "explorer-favorite",
                    "Add to favorites popup",
                    "Name a global favorite",
                    Some("360px · shared rename view · explorer metrics"),
                    components::explorer_favorite_popup::draw,
                ),
                single(
                    "explorer-rename",
                    "Rename popup (explorer)",
                    "Rename a file or folder",
                    Some("360px · shared rename view · explorer metrics"),
                    components::explorer_rename_popup::draw,
                ),
                section(
                    "settings",
                    "Settings window",
                    vec![
                        spec(
                            "settings",
                            "Settings — two-tier navigation",
                            Some(
                                "the largest dialog · L1 top tabs · L2 filterable sidebar · shown at 620×380",
                            ),
                            components::settings::draw,
                        ),
                        spec(
                            "settings-controls",
                            "Settings content — the control vocabulary",
                            Some(
                                "rows · switch · colour override · language select · gallery-only",
                            ),
                            components::settings::draw_controls,
                        ),
                        spec(
                            "settings-appearance-colour-rows",
                            "Appearance › colour rows — the Default hex is read-only, not disabled",
                            Some(
                                "Default hex = Input readOnly (neutral box, text-secondary value, focusable, copyable) · override = normal Input",
                            ),
                            components::settings_appearance_colors::draw,
                        ),
                        spec(
                            "settings-appearance-colors-header",
                            "Appearance › Colors header — Reset all",
                            Some(
                                "intro text + Reset all (ghost sm) · disabled with no override, count when overridden",
                            ),
                            components::settings_appearance_colors::draw_header,
                        ),
                        spec(
                            "settings-appearance-terminal-rows",
                            "Appearance › Colors › Terminal-specific — 5 rows",
                            Some(
                                "search_match_active_fg right after search_match_active_bg · same plain swatch · Reset all includes it · 6 groups / 47 rows",
                            ),
                            components::settings_appearance_colors::draw_terminal_rows,
                        ),
                        spec(
                            "settings-appearance-font-override",
                            "Appearance › Font override — rows + preview below",
                            Some(
                                "label settings-label-width · control at its field width · trailing Use default · preview below the grid",
                            ),
                            components::settings_font_override::draw,
                        ),
                        components::settings_font_rejected::file_spec(),
                        components::settings_font_rejected::family_spec(),
                        components::settings_font_rejected::toast_spec(),
                        spec(
                            "settings-appearance-general",
                            "Appearance › General — font rows, preview below",
                            Some(
                                "single column · settings row grid · preview below the rows at full content width · combo field-width-lg",
                            ),
                            components::settings_font_override::draw_general,
                        ),
                        spec(
                            "settings-file-extension-mapping",
                            "Handler › File Extension Mapping",
                            Some(
                                "Input + Add · per extension an ordered detector list · chevron IconButtons disabled, not hidden",
                            ),
                            components::settings_handler::draw_extension_mapping,
                        ),
                        spec(
                            "settings-file-detectors",
                            "Handler › File Detectors",
                            Some(
                                "two-line rows · origin Tag per source · user-only Remove · add card with patch / validation · Save-pending Undo",
                            ),
                            components::settings_file_handlers::draw_detectors,
                        ),
                        spec(
                            "settings-file-handlers",
                            "Handler › File Handlers",
                            Some(
                                "two-line rows · prio · <detector> → <action> · add card with action kinds · Save-pending Undo",
                            ),
                            components::settings_file_handlers::draw_file_handlers,
                        ),
                        spec(
                            "settings-hook-handlers",
                            "Handler › Hook Handlers",
                            Some(
                                "registry rows — id · origin Tag · prio · Switch · shell cmd Input · add draft",
                            ),
                            components::settings_handler::draw_hook_handlers,
                        ),
                        spec(
                            "settings-hook-add-card-raised",
                            "Hook Handlers — add card on a raised ground",
                            Some(
                                "labels text-secondary-raised · Inputs and Cancel on the raised ground (Mocha + Latte)",
                            ),
                            components::settings_handler::draw_hook_add_card_raised,
                        ),
                        spec(
                            "settings-hook-edited-default",
                            "Hook Handlers — edited default: mark + Revert",
                            Some(
                                "host / plugin row with a user patch · edited Tag · Revert → Undo + reverts on save (Mocha + Latte)",
                            ),
                            components::settings_hook_override::draw,
                        ),
                        spec(
                            "settings-hook-seq-editor",
                            "Hook Handlers — IpcSequence text editor",
                            Some(
                                "inline CodeArea editor — normal · first error with gutter mark · empty note (Mocha + Latte)",
                            ),
                            components::settings_hook_seq::draw,
                        ),
                        spec(
                            "settings-general-row-grid",
                            "General › General — row grid, row captions, webhook row",
                            Some(
                                "label column = longest label clamped 150 … 240, gap 16 · wheel and language captions under their rows · webhook warning callout under its row",
                            ),
                            components::settings_general_grid::draw,
                        ),
                        spec(
                            "settings-keybinding-rows",
                            "Keybindings — binding rows on the shared label column",
                            Some(
                                "label column = longest label clamped 150 … 240 · gap 16 · hint after the last word · long labels wrap",
                            ),
                            components::settings_keybinding_rows::draw,
                        ),
                        spec(
                            "settings-control-metrics",
                            "Settings · control metrics — shortcut rows · drag flip modifier",
                            Some(
                                "record button kb-record 140 × 24 · add 32 · rows kb-row-gap 8 · drag flip Select md after the General entries · font combo · open Select (Mocha + Latte)",
                            ),
                            components::settings_control_metrics::draw,
                        ),
                        spec(
                            "settings-macos-permissions",
                            "General › Permissions (macOS)",
                            Some(
                                "3-column table · glyph + word states · FDA row action · HelpHints · debug Tag · requesting spinner · stale grant caption (A–F, Mocha + Latte)",
                            ),
                            components::settings_macos_permissions::draw,
                        ),
                        spec(
                            "settings-overlay-toast-duration",
                            "General › Overlay — toast duration",
                            Some(
                                "one row · mono Input 90 + static s · 1.0–10.0 step 0.5, clamp on commit",
                            ),
                            components::settings_number::draw_toast_duration,
                        ),
                        spec(
                            "settings-remote-transfer",
                            "General › Remote transfer",
                            Some(
                                "Save folder (mono Input + Browse…) · Maximum size (numeric + MiB) · settings-row grid",
                            ),
                            components::settings_remote_transfer::draw,
                        ),
                        components::settings_remote_attach::entry(),
                        spec(
                            "settings-task-pipeline",
                            "Misc › Task pipeline",
                            Some(
                                "Note size limit · Attempt report limit (numeric + B) · settings-row grid · range line only when out of range",
                            ),
                            components::settings_task_pipeline::draw,
                        ),
                        spec(
                            "settings-anomaly",
                            "Misc › Anomaly detection",
                            Some("Memory growth · Repeated calls · Call bursts · nine fields · notify off keeps numbers editable"),
                            components::settings_anomaly_detection::draw,
                        ),
                        spec(
                            "settings-anomaly-off", "Anomaly detection · notification switches",
                            Some("All on · notification off while editing a number · numbers stay normal and editable"),
                            components::settings_anomaly_detection::draw_off,
                        ),
                        spec(
                            "settings-anomaly-range",
                            "Anomaly detection · range and narrow",
                            Some("Typed 1 commits as 2 · minimum RSS 0 · narrow responsive rows"),
                            components::settings_anomaly_detection::draw_range,
                        ),
                        spec(
                            "settings-anomaly-narrow", "Anomaly detection · narrow ko / ja",
                            Some("Existing settings_stack_row natural-width decision and hysteresis · captions wrap"),
                            components::settings_anomaly_detection::draw_narrow,
                        ),
                        spec(
                            "settings-performance",
                            "Terminal › Performance",
                            Some(
                                "apply note on top · two switch rows · Lines kept on disk (numeric, unit after the field, -1 = no limit) · disabled while disk swap is off",
                            ),
                            components::settings_performance::draw,
                        ),
                        spec(
                            "settings-performance-number",
                            "Terminal › Performance — the number field fits its value",
                            Some(
                                "field grows to its value · unit after the field · -1 = no limit in the unit's place · over range = invalid edge + range line · 9 / 10 / 19 digits · ko / ja · narrow 360",
                            ),
                            components::settings_performance::draw_number_field,
                        ),
                    ],
                ),
                single(
                    "kbplugins",
                    "Keybindings · Plugins",
                    "Plugin picker + per-command mode",
                    Some(
                        "settings Row grid 150 + 16 · one line per command: mode 160 · slot · ghost Reset · Select and Reset 28 · Custom = keybinding record slots kb-plugin-record-height 28 (one per key + add, None slot when empty) · Reset disabled without an override · draft dot · parse error caption · recording · empty",
                    ),
                    components::kb_plugins::draw,
                ),
                section(
                    "kbimportexport",
                    "Keybindings · Import / Export",
                    vec![
                        spec(
                            "kbimportexport-entry",
                            "L2 placement & entry screen",
                            Some(
                                "L2 separator above the last row · 2 action rows (Export secondary · Import primary) · export toast",
                            ),
                            components::kb_import_export::draw_entry,
                        ),
                        spec(
                            "kbimportexport-preview",
                            "Import preview — group headers, select column, long-table handling",
                            Some(
                                "32px select · 1.6fr action · 1fr current · 1fr imported · group header row · changed-only toggle",
                            ),
                            components::kb_import_export::draw_preview,
                        ),
                        spec(
                            "kbimportexport-open-values",
                            "Open values — failure, notices, tokens",
                            Some(
                                "export failure inline in its row · one warning block, 3 lines then fold · parse copy without a line",
                            ),
                            components::kb_import_export::draw_open_values,
                        ),
                        spec(
                            "kbimportexport-remaining-values",
                            "Open values — unknown failure reason, counts of one",
                            Some(
                                "catch-all clause \"the write didn't finish.\" · OS text on its own muted mono line, ellipsised, full text in the tooltip · singular header and line at one notice, with no fold link",
                            ),
                            components::kb_import_export::draw_remaining_values,
                        ),
                    ],
                ),
                section(
                    "scripts",
                    "Misc · Scripts (Lua script manager)",
                    vec![spec(
                        "scripts-list",
                        "Scripts subsection — list, states & empty",
                        Some(
                            "name · path (middle-elided) · Kbd/Unbound · changed badge · auto-run trigger chips + Add trigger… · bind·rename·remove · empty state",
                        ),
                        components::script_manager::draw,
                    )],
                ),
                section(
                    "tutorial",
                    "Tutorial — marker · callout · topic popup",
                    vec![
                        spec(
                            "tutorial-marker",
                            "Marker overlay — the 6th overlay family",
                            Some(
                                "Floating geometric ring at a target rect · top z · pointer-events:none · glow + spotlight default",
                            ),
                            widgets::tutorial::draw_marker,
                        ),
                        spec(
                            "tutorial-callout",
                            "Callout bubble — guidance pinned to a marker",
                            Some(
                                "244px fixed · step/total + dot rail · Skip·Back·Next · 4-way tail · edge-avoidance",
                            ),
                            widgets::tutorial::draw_callout,
                        ),
                        spec(
                            "tutorial-topics",
                            "Topic-list popup — the entry surface",
                            Some(
                                "360px CenteredFocused + scrim · scrollable list · selected / done states · 진행",
                            ),
                            widgets::tutorial::draw_topics,
                        ),
                        spec(
                            "tutorial-composite",
                            "Composite — a tutorial step in place",
                            Some(
                                "Step 1/4 워크스페이스 — marker + spotlight dim + callout, popup closed",
                            ),
                            widgets::tutorial::draw_composite,
                        ),
                    ],
                ),
                single(
                    "notifications",
                    "Notification panel",
                    "Level counts · expand / acknowledge · closed source · empty",
                    Some("352×400 · 전체화면 무대를 선언한 유일한 popup (fit + X)"),
                    components::notification_panel::draw,
                ),
                section(
                    "info-modal",
                    "Info modal shell — notice queue",
                    vec![
                        spec(
                            "info-modal",
                            "One shell for every queued message",
                            Some(
                                "440 × 140..360 · body text-secondary · dismiss = Primary, rightmost · DB error = Quit",
                            ),
                            components::info_modal::draw,
                        ),
                        spec(
                            "popup-title-bar",
                            "Popup title bar — one or two buttons, title centred on the strip",
                            Some(
                                "reserve 32 (× only) · 60 (fit + ×) per side · IconButton sm 24 · long title ellipsises in the band",
                            ),
                            components::info_modal::draw_title_bar,
                        ),
                        spec(
                            "info-modal-permissions",
                            "Permissions notice (macOS) — the long case",
                            Some(
                                "scroll top / mid / end · paths · lead-ins · command chip · scroll edge above buttons · FDA paragraph branches (never / stale / revoked)",
                            ),
                            components::info_modal::draw_permissions,
                        ),
                        spec(
                            "info-modal-permission-branches",
                            "Permissions notice — FDA branches · signing aside",
                            Some(
                                "never / stale / stale Latte / revoked · scrolled to the end · stale steps as a numbered two-step list · signing paragraph last in every notice as secondary text",
                            ),
                            components::info_modal::draw_permission_branches,
                        ),
                    ],
                ),
                section(
                    "restore-notice",
                    "Restore failure / save halt — non-focus notice, top right",
                    vec![
                        spec(
                            "restore-notice",
                            "Restore failure — one flow",
                            Some(
                                "notice column · inset 8 · width 360 · no scrim · no focus · × = Continue",
                            ),
                            components::restore_notice::draw,
                        ),
                        spec(
                            "restore-notice-variants",
                            "Narrow window · long path · save halt",
                            Some("width = work area − 16 · path wraps anywhere · halt cause write"),
                            components::restore_notice::draw_variants,
                        ),
                    ],
                ),
                single(
                    "script-confirm",
                    "Script changed confirm",
                    "TOFU gate — run the changed script?",
                    Some("360px · mono 경로 truncate · changed 태그 · Run anyway / Cancel"),
                    components::script_confirm::draw,
                ),
                single(
                    "quit-modal",
                    "Quit confirmation",
                    "Quit or minimize to background",
                    Some("400×200 독립 창 · close_behavior = \"ask\" 경로"),
                    components::quit_modal::draw,
                ),
                section(
                    "plugins-window",
                    "Plugins manager window",
                    vec![
                        spec(
                            "plugins-window",
                            "Installed / Attention / Add plugin",
                            Some(
                                "상태 10 · 헤더 48 + 목록 + 상세 전량 · builtin 점 · health danger dot",
                            ),
                            components::plugins_window::draw,
                        ),
                        spec(
                            "plugin-add-hint-slot",
                            "Add plugin — dashed empty hint · manifest read error",
                            Some(
                                "Before Verify a dashed 1px border-default box · a read failure puts a solid accent-danger box in the same slot",
                            ),
                            components::plugins_window::draw_hint_slot,
                        ),
                        spec(
                            "plugins-install-path",
                            "Installed detail — install path · log path · Open folder",
                            Some(
                                "Open folder on the caption row, right · paths mono caption muted, wrap at any character, selectable · 380 ≈ 720 window, 540 ≈ 880 window",
                            ),
                            components::plugins_window::draw_install_paths,
                        ),
                        spec(
                            "plugin-identity-mark",
                            "Plugin identity mark — one component, two sizes",
                            Some(
                                "sm 32 on list rows · lg 46 on detail and preview · fixed tint bed",
                            ),
                            overlays_settled::draw_plugin_identity,
                        ),
                    ],
                ),
                single(
                    "drop-overlay",
                    "Drag & drop overlay",
                    "Drop to open — hover feedback on the terminal",
                    Some("inset 상자(fill 0.12 + 1px 보더 0.36) 안 download 글리프 + 강조색 라벨"),
                    components::drop_overlay::draw,
                ),
            ],
        },
        // ── Layouts ──────────────────────────────────────────────────
        Page {
            category: Category::Layouts,
            sections: vec![
                section(
                    "sidebar",
                    "Sidebar & rail",
                    vec![
                        spec(
                            "sidebar",
                            "Sidebar (Full / Collapsed)",
                            Some("Full 212 expands names; collapsed 52 rail keeps icon slots"),
                            components::sidebar::draw,
                        ),
                        spec("workspace-row-running-count", "Workspace row — running count", Some("Bare numeral between the dot and the name; running terminal surfaces, hidden tabs in, plugin surfaces out; 0 draws no numeral"), components::sidebar_running_count::draw),
                        spec("notification-summary", "Notification summary", Some("Unread error, warning and info · expanded and rail"), components::notification_panel::draw_summary),
                        spec(
                            "sidebar-attached-ring",
                            "Attached ring in the workspace row",
                            Some(
                                "Every row reserves the derived dot slot (16 at scale 1); the name follows the running count column",
                            ),
                            components::sidebar::draw_attached_ring,
                        ),
                        spec(
                            "rail-marks",
                            "Rail marks — category line and mirror chip",
                            Some("2px category line in avatar ink and the sky-edged mirror chip, Mocha and Latte"),
                            components::rail_marks::draw,
                        ),
                    ],
                ),
                section(
                    "tabs",
                    "Tab strips",
                    vec![
                        spec(
                            "tabbar",
                            "Pane tab strip",
                            Some(
                                "24×150 tabs on bg-sidebar; active lifts to bg-panel + accent bar",
                            ),
                            components::tab_bar::draw,
                        ),
                        spec(
                            "tab-strip-tooltips",
                            "Tooltips in the strip open upward — native content below",
                            Some(
                                "top → bottom → inside the strip (1px border-width tolerance), first clear of every WebView rect · else inside the strip anyway",
                            ),
                            widgets::html_script_banner::draw_strip_tooltips,
                        ),
                        spec(
                            "tab-scroll-arrows",
                            "Tab strip scroll arrows — disabled ink",
                            Some(
                                "left chevron at the scroll start takes tab-scroll-arrow-fg-disabled · right chevron takes tab-scroll-arrow-fg",
                            ),
                            components::tab_bar::draw_scroll_arrows,
                        ),
                        spec(
                            "tab-status-cluster",
                            "Tab cell — the right-hand status cluster",
                            Some(
                                "marker · move · busy · close in one fixed cluster · the label ellipsises first",
                            ),
                            components::tab_bar::draw_status_cluster,
                        ),
                        spec(
                            "tab-scroll-arrow-shape",
                            "Scroll arrows — chevron icon, square cell, no own fill",
                            Some(
                                "chevron 12 · 24×24 cell on the strip ground · hover on the enabled side only",
                            ),
                            components::tab_bar::draw_scroll_shape,
                        ),
                        spec(
                            "tab-move-cue",
                            "Move source scrolled out of view — the arrow on that side turns pink",
                            Some(
                                "tab-scroll-arrow-move-fg until the target cell is fully inside the viewport",
                            ),
                            components::tab_bar::draw_move_cue,
                        ),
                        spec(
                            "multitab",
                            "Multi-tier tabs",
                            Some("Workspace tier + pane tier, two levels max"),
                            widgets::multi_tab_layout::draw,
                        ),
                        spec(
                            "explorer-tabs",
                            "Explorer internal tab strip",
                            Some(
                                "Surface-local · 24px · bottom accent underline (vs pane top bar)",
                            ),
                            components::explorer_tab_bar::draw,
                        ),
                    ],
                ),
                section(
                    "statusbar",
                    "Status bar",
                    vec![
                        spec(
                            "statusbar",
                            "Workspace status bar",
                            Some(
                                "24px read-only summary + one keyboard reminder · collapses grid → \
                             shell → surface id → palette cap → branch text (calls the real \
                             `tasty_ui_widgets` view)",
                            ),
                            components::status_bar::draw,
                        ),
                        spec(
                            "statusbar-remote-item",
                            "Remote item — a mirror surface has focus",
                            Some(
                                "remote glyph + host (attach profile name) leads the left cluster \
                             in the remote hue · reconnecting drops to the bar's neutral ink + a \
                             status word · host capped at 160, drops last",
                            ),
                            components::status_bar_remote::draw,
                        ),
                        spec(
                            "statusbar-theme-cell",
                            "Theme cell — one glyph box, both themes (settled)",
                            Some(
                                "sun (Latte) and theme (Mocha) at one size — statusbar-glyph-size = icon-size-xs (12); the overshoot is fixed in the sun asset, not with a per-theme size",
                            ),
                            components::status_bar_theme_cell::draw,
                        ),
                    ],
                ),
                section(
                    "depth",
                    "List → detail",
                    vec![
                        spec(
                            "onedepth",
                            "1-depth (general shell)",
                            Some("Fixed list selects, detail fills the rest"),
                            widgets::layout_1depth::draw,
                        ),
                        spec(
                            "twodepth",
                            "2-depth (general shell)",
                            Some("L1 tabs (underline) + L2 sections (surface-active)"),
                            widgets::layout_2depth::draw,
                        ),
                    ],
                ),
                surface_focus::section(),
                section(
                    "surfaces",
                    "Dividers & surfaces",
                    vec![
                        spec(
                            "divider",
                            "Pane divider",
                            Some("1px line, ~7px hit-band, accent on hover, both axes"),
                            widgets::divider::draw,
                        ),
                        spec(
                            "surface",
                            "Surface focus states",
                            Some("Focused and unfocused beds, agent dot"),
                            components::surface_highlights::draw,
                        ),
                        spec(
                            "mixed-split",
                            "Mixed split — terminal + markdown in one pane group",
                            Some(
                                "each surface follows focus on one pair of beds · 36px address bar kept in split",
                            ),
                            layouts_settled::draw_mixed_split,
                        ),
                        spec(
                            "occupancy",
                            "Occupancy & attention borders",
                            Some(
                                "needs-input yellow 2px · soft green 1px · hard peach 1px · completed blue 2px",
                            ),
                            components::occupancy_borders::draw,
                        ),
                    ],
                ),
                layouts_attention::section(),
                single(
                    "movesource",
                    "Move source highlight",
                    "Move source — dashed ring and off-screen glyph",
                    Some(
                        "2px pink dashed ring inside the target · move glyph on the nearest visible container",
                    ),
                    components::move_source::draw,
                ),
                section(
                    "dag-graph",
                    "Task DAG · canvas & nodes",
                    vec![
                        spec(
                            "dag-canvas",
                            "Task DAG canvas — read-only observation",
                            Some("레이어 배치 + 직교 엣지. 포트도 드래그 연결도 없다 — 관찰 전용"),
                            components::dag::canvas::draw,
                        ),
                        spec(
                            "dag-node",
                            "Task node — every execution state",
                            Some("바 색 · 글리프 · 철자 라벨 세 채널로 상태를 동시에 표기"),
                            components::dag::node::draw_states,
                        ),
                        spec(
                            "dag-kinds",
                            "Task kinds",
                            Some("run / custom / reduce / wait_barrier — 선행 글리프로 구분"),
                            components::dag::node::draw_kinds,
                        ),
                        spec(
                            "dag-lod",
                            "Level of detail · selection · overflow",
                            Some("full ≥ 0.7 · compact ≥ 0.4 · block < 0.4, 박스 크기는 불변"),
                            components::dag::node::draw_lod,
                        ),
                        spec(
                            "dag-edges",
                            "Dependency edges",
                            Some(
                                "depends_on 실선 · fallback 6 3 · reduce 2 3 · binding 8 2 2 2 · transition 10 4, 화살촉은 의존하는 쪽",
                            ),
                            components::dag::edges::draw,
                        ),
                        spec(
                            "dag-routes",
                            "Transitions · not selected",
                            Some(
                                "전이 선택 4 상태는 굵기·잉크만 바뀐다 · 미선택 노드는 skipped 카드에 라벨·툴팁만 다르다",
                            ),
                            components::dag::routes::draw,
                        ),
                    ],
                ),
                section(
                    "dag-phase",
                    "Task DAG · running phase & unknown reason",
                    vec![
                        spec(
                            "dag-running-phase",
                            "Running phase — only awaiting_input gets its own look",
                            Some(
                                "입력 대기만 needs-input 노랑 전체 · 후처리·재시도 대기는 running 톤에 라벨과 실행 번호",
                            ),
                            components::dag::phase::draw_phases,
                        ),
                        spec(
                            "dag-why",
                            "Hover — the why-line for skipped, unknown and awaiting",
                            Some(
                                "name — label 다음 이유 한 줄 · 상세 패널의 입력 대기 알림과 Why unknown",
                            ),
                            components::dag::phase::draw_why,
                        ),
                    ],
                ),
                section(
                    "dag-shell",
                    "Task DAG · chrome, detail & surface",
                    vec![
                        spec(
                            "dag-chrome",
                            "Zoom cluster + minimap",
                            Some(
                                "캔버스 우하단 8px 안쪽 · 미니맵 560 미만에서 제거, 판독창 400 미만에서 제거",
                            ),
                            components::dag::chrome::draw,
                        ),
                        spec(
                            "dag-runner",
                            "Host runner state",
                            Some("멈춘 러너 + 남은 ready 만 경고 톤 — 끝난 그래프의 정지는 muted"),
                            components::dag::runner::draw,
                        ),
                        spec(
                            "dag-detail",
                            "Selected task — side panel · bottom sheet",
                            Some("288 패널 / 220 시트, 에러 tail 은 경계가 정해진 스크롤 블록"),
                            components::dag::detail::draw,
                        ),
                        spec(
                            "dag-states",
                            "Empty states and the cycle warning",
                            Some("워크스페이스 빈 상태 · 검색 무매치 · 사이클 배너"),
                            components::dag::states::draw,
                        ),
                        spec(
                            "dag-surface",
                            "Full-tab surface — wide and narrow",
                            Some("640 아래에서 헤더 2행 · 미니맵 제거 · 상세는 하단 시트"),
                            components::dag::surface::draw,
                        ),
                    ],
                ),
                section(
                    "dag-list",
                    "Task DAG · list rows & workspace popup",
                    vec![
                        spec(
                            "dag-rows",
                            "DAG list rows",
                            Some("출처 태그 · rollup 상태 · mono done/total — 진행 막대는 없다"),
                            components::dag::rows::draw,
                        ),
                        spec(
                            "dag-window",
                            "Workspace popup — list ⇄ single DAG",
                            Some(
                                "560 × 460, DrillDown 전면 교체. 상세는 640 아래라 항상 하단 시트",
                            ),
                            components::dag::window::draw,
                        ),
                        spec(
                            "dag-window-detail",
                            "Detail view — the header keeps the runner badge only",
                            Some(
                                "No second header — the back bar carries the compact zoom cluster and the runner badge",
                            ),
                            components::dag::window::draw_detail,
                        ),
                    ],
                ),
                single(
                    "titlebar",
                    "Window titlebar (CSD)",
                    "Active · inactive · close hover",
                    Some("titlebar-height(36) · window-button-size(24) · 하단 1px"),
                    components::titlebar::draw,
                ),
                single(
                    "empty-surface",
                    "Empty surface",
                    "One button, nothing else",
                    Some("포커스 바탕 전면 · 세로 중앙 · convert popup 을 연다"),
                    components::empty_surface::draw,
                ),
            ],
        },
        // ── Plugins ──────────────────────────────────────────────────
        Page {
            category: Category::Plugins,
            sections: vec![
                single(
                    "clipboard-viewer",
                    "Clipboard viewer popup",
                    "Clipboard viewer — read-only snapshot popup",
                    Some("480×360 · single column · type bar → code well · empty / read-failed"),
                    components::clipboard_viewer::draw,
                ),
                section(
                    "git-viewer",
                    "Git worktree viewer popup",
                    vec![
                        spec(
                            "git-viewer",
                            "Worktree rail + status / log / diff",
                            Some(
                                "≈960 · splitter H 0.25 · splitter V 0.5 · rail → status/log/diff",
                            ),
                            components::git_viewer::draw,
                        ),
                        spec(
                            "git-diff-toolbar",
                            "Diff toolbar — a container height, not a button height",
                            Some("git-toolbar-height 32 holds 28px controls"),
                            overlays_settled::draw_diff_toolbar,
                        ),
                    ],
                ),
                section(
                    "explorer",
                    "Explorer — the file-manager surface",
                    vec![
                        spec(
                            "explorer-surface",
                            "Explorer surface — full layout (Detail view)",
                            Some(
                                "internal tabs · toolbar (nav · path field · view toggle) · 196px sidebar · Detail content",
                            ),
                            components::explorer_surface::draw,
                        ),
                        spec(
                            "explorer-view-toggle",
                            "View toggle (SegToggle) — a segment, so it fills (settled)",
                            Some(
                                "grid / list / detail picks a value · accent fill + on-accent ink · Mocha and Latte",
                            ),
                            components::explorer_view_toggle::draw,
                        ),
                        spec(
                            "explorer-states",
                            "Empty / permission / loading / read error · favorite + rename popups",
                            Some(
                                "Status screens fill the content area; the two small editors reuse the Popup/rename visual language. A read failure that is not a permission denial gets its own state with Retry and Go up instead of the empty folder line.",
                            ),
                            components::explorer_states::draw,
                        ),
                    ].into_iter().chain(components::explorer_jobs::specs()).collect(),
                ),
                section(
                    "markdown-viewer",
                    "Markdown surface",
                    vec![
                        spec(
                            "markdown-viewer",
                            "Markdown surface",
                            Some(
                                "6-level prose hierarchy · library-owned body leading · element catalog · load/empty states",
                            ),
                            components::markdown_viewer::draw,
                        ),
                        plugins_settled::doc_background_spec(),
                        components::markdown_viewer::quote_find::spec(),
                        spec(
                            "markdown-address-states",
                            "Address bar states · large-file confirm",
                            Some("idle · editing · over 1 MiB confirm dims only the tile"),
                            plugins_settled::draw_address_states,
                        ),
                        spec(
                            "markdown-address-bar",
                            "Markdown — address band · local & remote mirror",
                            Some("Folder · (Reload) · editable field · Go · stale · disconnected · navigate failed"),
                            components::md_address::draw,
                        ),
                        spec(
                            "markdown-address-folder",
                            "Markdown — Folder button (Browse)",
                            Some("IconButton ghost sm · folderOpen · normal / hover / active / disabled"),
                            components::md_address::draw_folder_button,
                        ),
                        spec(
                            "markdown-address-failures",
                            "Markdown — remote navigate failed by kind · still waiting",
                            Some("one error row, a sentence per kind · remote reason verbatim · status row after 30 s"),
                            components::md_address::draw_failures,
                        ),
                        spec(
                            "markdown-address-narrow",
                            "Markdown — address band at narrow widths",
                            Some("Go hides when the field would drop below 160 · field floor 96"),
                            components::md_address::draw_narrow,
                        ),
                        components::markdown_viewer::callout_kinds_spec(),
                        components::markdown_viewer::content_colour_spec(),
                        components::markdown_viewer::heading_hierarchy_spec(),
                    ],
                ),
                components::image_viewer::section(),
                section("html-address", "HTML address band", vec![
                    spec("html-address-states", "Address band — states", Some("Shared host chrome; no document DOM injection"), components::html_address::draw_states),
                    spec("html-address-placement", "Address band — placement", Some("Narrow / mirror / size and script banner insets"), components::html_address::draw_placement),
                    spec("html-address-live", "Address band — live input", Some("Draft / Enter / Esc and pure URL resolution"), components::html_address::draw_live),
                ]),
                components::html_chrome::section(),
            ],
        },
        // ── Chrome ───────────────────────────────────────────────────
        Page {
            category: Category::Chrome,
            sections: vec![
                section(
                    "boot-loading",
                    "Boot loading screen",
                    vec![
                        spec(
                            "boot-loading-default",
                            "Default — 1280×720",
                            Some("Wordmark → spinner → phase text, centered stack"),
                            chrome_loading::draw_default,
                        ),
                        spec(
                            "boot-loading-min",
                            "Minimum window — 640×480",
                            Some("Same stack, size-invariant — no responsive scaling"),
                            chrome_loading::draw_min,
                        ),
                        spec(
                            "boot-loading-phases",
                            "Phase text — three variants",
                            Some("GpuInit / WaitingPlugins / RestoringLayout, side by side"),
                            chrome_loading::draw_phases,
                        ),
                        spec(
                            "boot-loading-no-text",
                            "No phase text",
                            Some("Slot stays reserved and empty — comparison variant"),
                            chrome_loading::draw_no_text,
                        ),
                        spec(
                            "boot-loading-spinner",
                            "Spinner reused — size 16 → 32 for boot hero",
                            Some("the shared Spinner one step larger · reduced-motion fallback"),
                            chrome_loading::draw_spinner_hero,
                        ),
                        spec(
                            "boot-loading-latte",
                            "Latte theme",
                            Some(
                                "GPU clear color follows the resolved theme, not a hardcoded dark",
                            ),
                            chrome_loading::draw_latte,
                        ),
                    ],
                ),
                section(
                    "shell-setup",
                    "First-run shell setup",
                    vec![spec(
                        "shell-setup-form",
                        "Shell path form — Quit / Use this shell",
                        Some(
                            "Lockup + 360 form · one validation line per host verdict · Git Bash notice on Windows only",
                        ),
                        chrome_loading::draw_shell_setup,
                    )],
                ),
                section(
                    "shutdown-loading",
                    "Shutdown screen",
                    vec![spec(
                        "shutdown-loading",
                        "Same surface as boot, shutdown phases",
                        Some(
                            "Shown only while quitting has work to wait for — values identical to boot",
                        ),
                        chrome_loading::draw_shutdown,
                    )],
                ),
            ],
        },
    ]
}

fn file_picker_specs() -> Vec<Spec> {
    use components::file_picker as picker;
    let entries = [
        (
            "filepicker",
            "Native file picker — local & remote, one component",
            "640×480 · PopupDef · differs only in header host badge + breadcrumb root",
            picker::draw as fn(&mut egui::Ui, &Theme),
        ),
        (
            "filepicker-remote-indicator",
            "Remote indicator — three candidates",
            "§6.1 · badge (recommended) · glyph + host · frame border — one accent-info axis",
            picker::draw_remote_indicator,
        ),
        (
            "filepicker-states",
            "States — loading · empty · permission · connection lost · multi-select",
            "body swaps list ↔ status without changing the frame",
            picker::draw_states,
        ),
        (
            "filepicker-folder-mode",
            "Folder mode — current or selected folder",
            "shared footer · inert files · 640 / 480×360 / 320 · en / ko / ja",
            picker::draw_folder_mode,
        ),
        (
            "filepicker-save-mode",
            "Save mode — one confirm, in the footer",
            "list pick fills the name · Overwrite inline · deep paths elide in the middle",
            picker::draw_save_mode,
        ),
        (
            "filepicker-gesture-table",
            "The gesture table — a folder selected in either mode",
            "single click selects · double click descends · a folder is never a save target · Open enters it",
            picker::draw_gesture_table,
        ),
        (
            "filepicker-path-bar",
            "Path bar — what gives way when the folded path still doesn't fit",
            "current folder → parent → root → the … menu · five steps, each at its floor",
            picker::draw_path_fit,
        ),
        (
            "filepicker-filter-chip",
            "File-type filter chip — a read-only readout",
            "caller's extensions in mono · hidden without a filter · capped at fp-filter-max-width, then ellipsis + tooltip",
            picker::draw_filter_chip,
        ),
    ];
    entries
        .into_iter()
        .map(|(id, title, when, draw)| spec(id, title, Some(when), draw))
        .collect()
}
