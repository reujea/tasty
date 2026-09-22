//! 카탈로그 모델 — 디자인(4) gallery 의 **page > section > spec** 문서 계층.
//!
//! 한 페이지(`Page`)는 1차 분류(`Category`) 하나에 대응하고, 여러 `Section` 을
//! 가진다. 각 `Section` 은 여러 `Spec` 을 묶는다. `Spec::draw` 는 선택된 한
//! specimen 의 라이브 데모(stage/cluster/meta)를 그린다.
//!
//! 셸(`host_shell`)은 활성 페이지의 전 Section/Spec 을 한 문서로 스크롤 렌더하고,
//! 좌측 nav 의 "On this page" 앵커는 이 Section 목록에서 도출한다.
//!
//! 이 단계(인프라)는 모델/셸/헬퍼의 토대만 만든다. 각 `Spec::draw` 는 기존
//! specimen 의 `draw` 함수를 그대로 연결해 컴파일/실행을 유지하며, 페이지별
//! specimen 콘텐츠의 디자인 정합 재작성은 Round 2 의 책임이다.

pub mod chrome_loading;
pub mod components;
pub mod foundations_shape;
pub mod foundations_uiscale;
pub mod icons;
pub mod popup_frame;
pub mod spacing;
pub mod spec;
pub mod theme;
pub mod toast_card;
pub mod typography;
pub mod widgets;

use std::sync::OnceLock;

use tasty_settings::{GeneralSettings, KeybindingSettings};
use tasty_type_appearance::theme::Theme;

/// quick-switch 축의 modifier 를 **설정 기본값**에서 표시 문자열로 뽑는다
/// (`"ctrl+shift"` → `"Ctrl+Shift"`). 표기 규칙은 본체 설정 UI 와 같은
/// `KeybindingSettings::format_display` 를 그대로 쓴다.
///
/// 조합을 specimen 라벨에 문자열로 박으면 기본값이 바뀔 때 라벨만 조용히 남는다 —
/// 실제로 카테고리 축의 기본값이 바뀐 뒤 컴포넌트 캡션만 갱신되고 이 카탈로그의
/// 제목·부제는 옛 조합을 가리킨 채로 있었다. 여기서 파생시키면 그 드리프트가 구조적으로
/// 불가능해진다(단축키를 코드에 하드코딩하지 않는다는 정책과 같은 취지).
fn modifier_label(combo: &str) -> String {
    KeybindingSettings::format_display(combo, &GeneralSettings::default())
}

/// 탭 축 quick-switch specimen 부제 — modifier 는 기본값에서 파생.
fn tab_switch_caption() -> &'static str {
    static CAPTION: OnceLock<String> = OnceLock::new();
    CAPTION
        .get_or_init(|| {
            format!(
                "{} held · number keycap replaces each tab icon, in place",
                modifier_label(&KeybindingSettings::default().tab_switch_modifier)
            )
        })
        .as_str()
}

/// 워크스페이스 축 quick-switch specimen 부제 — modifier 는 기본값에서 파생.
fn workspace_switch_caption() -> &'static str {
    static CAPTION: OnceLock<String> = OnceLock::new();
    CAPTION
        .get_or_init(|| {
            format!(
                "{} held · keycap replaces status dot / letter avatar",
                modifier_label(&KeybindingSettings::default().workspace_switch_modifier)
            )
        })
        .as_str()
}

/// 카테고리 축 quick-switch specimen 부제 — modifier 는 기본값에서 파생.
///
/// 끝의 배타성 문구는 특정 조합이 아니라 **축**을 가리킨다 — 워크스페이스 축과
/// 카테고리 축은 서로 다른 조합이라 두 오버레이가 동시에 그려지지 않는다는 뜻이며,
/// 어느 쪽 기본값이 바뀌어도 그대로 참이다.
fn category_switch_caption() -> &'static str {
    static CAPTION: OnceLock<String> = OnceLock::new();
    CAPTION
        .get_or_init(|| {
            format!(
                "{} held · keycap right-aligned on headers / centered on rail --- \
                 (mutually exclusive with the workspace axis)",
                modifier_label(&KeybindingSettings::default().category_switch_modifier)
            )
        })
        .as_str()
}

/// 카탈로그 1차 분류 = 문서 페이지 하나. 상단 crumb + 좌측 nav Catalog 그룹에 사용.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// 토큰·기초 (색/타입/간격/형태/스케일).
    Foundations,
    /// 위젯·컴포넌트 (단일 UI primitive).
    Components,
    /// canonical 글리프 세트.
    Icons,
    /// 모달·팝업 레이어.
    Overlays,
    /// 구조 셸 (사이드바/탭바/분할/하이라이트).
    Layouts,
    /// 플러그인 유래 컴포넌트 (네이티브와 분리된 플러그인 전용 섹션).
    Plugins,
    /// 앱 크롬 전용 완결 화면 (부팅 로딩 등) — 위젯이 아니라 조립된 화면 단위 specimen.
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

    /// nav 링크 우측 desc (research §1.2 의 페이지 메타).
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

fn spec(
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

fn section(id: &'static str, title: &'static str, specs: Vec<Spec>) -> Section {
    Section { id, title, specs }
}

/// 모든 페이지의 Section/Spec 트리.
///
/// 기존 specimen `draw` 들을 디자인 분류(research §3.1)에 따라 page/section 으로
/// 임시 매핑한다 — 34 개 기존 draw 전수 연결. 콘텐츠 재작성은 Round 2.
pub fn pages() -> Vec<Page> {
    vec![
        // ── Foundations ──────────────────────────────────────────────
        Page {
            category: Category::Foundations,
            sections: vec![
                section(
                    "elevation",
                    "Color — elevation (surface ramp)",
                    vec![spec(
                        "elevation",
                        "Depth reads through surface tint; shadow is for floating surfaces only",
                        Some(
                            "bg-app → sidebar → panel → surface-raised, one tint step apart — \
                             lift is popover / modal / none, by surface shape",
                        ),
                        theme::elevation,
                    )],
                ),
                section(
                    "text",
                    "Color — text",
                    vec![spec(
                        "text",
                        "Hierarchy by text color, on any surface",
                        Some("primary → secondary → muted → disabled → placeholder"),
                        theme::text,
                    )],
                ),
                section(
                    "accents",
                    "Color — accent roles",
                    vec![spec(
                        "accents",
                        "Accents map to roles, not decoration",
                        Some("primary · info · success · warning · danger · agent"),
                        theme::accents,
                    )],
                ),
                section(
                    "terminal",
                    "Color — terminal / ANSI palette",
                    vec![spec(
                        "terminal",
                        "The colors a terminal cell paints with — not UI chrome",
                        Some("ANSI 16 (SGR 30–37 / 90–97) + selection · vi cursor · search fills"),
                        theme::terminal,
                    )],
                ),
                section(
                    "type",
                    "Type",
                    vec![spec(
                        "type",
                        "Two families, hard 14px cap, hierarchy by weight",
                        Some("heading 13/600 · body 13 · caption 11 · mono 14"),
                        typography::draw,
                    )],
                ),
                section(
                    "spacing",
                    "Spacing — the 4px grid, in use",
                    vec![spec(
                        "spacing",
                        "Five steps, each with a job",
                        Some("xs chip · sm pair · md card · lg column · xl region"),
                        spacing::draw,
                    )],
                ),
                section(
                    "shape",
                    "Radius · border · motion",
                    vec![spec(
                        "shape",
                        "Crisp and rectilinear — it's a terminal",
                        Some("radius 4/2 · 1px border · UI 90–120ms · terminal 0ms"),
                        foundations_shape::draw,
                    )],
                ),
                section(
                    "uiscale",
                    "UI scale — sidebar zoom",
                    vec![spec(
                        "uiscale",
                        "One multiplier scales the sidebar; everything else stays fixed",
                        Some("stops 0.8 / 1.0 / 1.2 — sidebar root zoom only"),
                        foundations_uiscale::draw,
                    )],
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
                            "forms",
                            "Select · Multi-select · Checkbox · Switch",
                            Some("Choice and toggle controls (single + multi choice)"),
                            components::prim_forms::draw,
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
                            "drilldown",
                            "DrillDown",
                            Some(
                                "Master → detail content swap — pinned back bar (← + title + actions), instant switch",
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
                    "text",
                    "Hint text",
                    vec![spec(
                        "hint",
                        "Hint text",
                        Some("Helper text below a field"),
                        widgets::hint_text::draw,
                    )],
                ),
                section(
                    "warning-callout",
                    "Warning callout",
                    vec![spec(
                        "warning-callout",
                        "Warning callout",
                        Some("Bordered warning tint box — icon + caption under a risky toggle"),
                        widgets::warning_callout::draw,
                    )],
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
                    ],
                ),
                section(
                    "segmented",
                    "Segmented control",
                    vec![spec(
                        "segmented",
                        "Segmented",
                        Some("Mutually-exclusive toggle — explorer's grid/list/detail switch"),
                        components::segmented::draw,
                    )],
                ),
                section(
                    "explorer-cells",
                    "Explorer view cells",
                    vec![spec(
                        "explorer-cells",
                        "Explorer view cells",
                        Some(
                            "grid cell (new) · list row (tree_row) · detail (Table + sort header)",
                        ),
                        components::explorer_view_cells::draw,
                    )],
                ),
                section(
                    "explorer-toolbar",
                    "Explorer toolbar",
                    vec![spec(
                        "explorer-toolbar",
                        "Address bar + view-mode toggle",
                        Some(
                            "surface-raised address box (clipped crumbs) + grid/list/detail icon toggle",
                        ),
                        components::explorer_toolbar::draw,
                    )],
                ),
                section(
                    "explorer-sidebar",
                    "Explorer sidebar",
                    vec![spec(
                        "explorer-sidebar",
                        "Files tree + Favorites (populated / empty)",
                        Some(
                            "tree active highlight · section separator · filled star · empty state",
                        ),
                        components::explorer_sidebar::draw,
                    )],
                ),
                section(
                    "layout-shell",
                    "Layout shell widgets",
                    vec![spec(
                        "layout-shell",
                        "two-depth panel · overflow tab bar · content frame",
                        Some("tasty-ui-widgets 공용 함수를 직접 호출한다 (복제 아님 — demo=main)"),
                        components::prim_layout_shell::draw,
                    )],
                ),
            ],
        },
        // ── Icons ────────────────────────────────────────────────────
        // 디자인(4) §2.3 — system-rules Section 1개 + 8 job 그룹 Section.
        // 모든 글리프 24×24, 2px stroke, round, no fill, currentColor.
        Page {
            category: Category::Icons,
            sections: vec![
                section(
                    "system-rules",
                    "The icon system",
                    vec![spec(
                        "system-rules",
                        "One geometry, recolored by context",
                        Some(
                            "24×24 viewBox · 2px stroke round · no fill · currentColor — sized via prop (26/20/16/14/12)",
                        ),
                        icons::draw_system_rules,
                    )],
                ),
                section(
                    "actions",
                    "Actions",
                    vec![spec(
                        "actions",
                        "The verbs",
                        Some(
                            "What an IconButton wraps in a toolbar or row — close/refresh in every overlay header, edit/trash/copy in list rows",
                        ),
                        icons::draw_actions,
                    )],
                ),
                section(
                    "nav",
                    "Navigation & disclosure",
                    vec![spec(
                        "nav",
                        "Movement and open/closed state",
                        Some(
                            "Single chevrons = tree-row disclosure; doubled = collapse/expand the sidebar rail",
                        ),
                        icons::draw_nav,
                    )],
                ),
                section(
                    "surfaces",
                    "Surfaces & workspace",
                    vec![spec(
                        "surfaces",
                        "The nouns of the workspace",
                        Some(
                            "What a tab, tree row, or new-surface button shows — terminal/markdown are the two core surface kinds",
                        ),
                        icons::draw_surfaces,
                    )],
                ),
                section(
                    "view",
                    "View modes & favorites",
                    vec![spec(
                        "view",
                        "Explorer view switch & bookmark marker",
                        Some(
                            "grid / list / detail toggle glyphs + star — the explorer toolbar and favorites sidebar",
                        ),
                        icons::draw_view,
                    )],
                ),
                section(
                    "visibility",
                    "Visibility",
                    vec![spec(
                        "visibility",
                        "Reveal toggle on secret values",
                        Some(
                            "Passkeys, env — eye when hidden, eyeOff when shown; swap in place on the same IconButton",
                        ),
                        icons::draw_visibility,
                    )],
                ),
                section(
                    "status",
                    "Status & alerts",
                    vec![spec(
                        "status",
                        "Inline meaning markers",
                        Some(
                            "Tinted by the line they sit in (warning amber, success green, danger red) via currentColor — not state dots",
                        ),
                        icons::draw_status,
                    )],
                ),
                section(
                    "system",
                    "Tools & system",
                    vec![spec(
                        "system",
                        "Sidebar footer & global tools",
                        Some("Each anchors a menu or window — tools, settings, plug, rocket"),
                        icons::draw_system,
                    )],
                ),
                section(
                    "keys",
                    "Modifier keys (macOS)",
                    vec![spec(
                        "keys",
                        "Command / Option / Shift symbols",
                        Some(
                            "Vector replacements for ⌘/⌥/⇧ — the settings display-style dropdowns and the modifier-hint keycap chip",
                        ),
                        icons::draw_keys,
                    )],
                ),
            ],
        },
        // ── Overlays ─────────────────────────────────────────────────
        // 디자인(4) §2.4 의 14 Spec 을 1:1 Section 으로 — 모든 모달이 공유하는
        // scrim/frame 레시피(scrim)부터 2-tier settings 까지.
        Page {
            category: Category::Overlays,
            sections: vec![
                section(
                    "scrim",
                    "Scrim & frame",
                    vec![spec(
                        "scrim",
                        "Dismiss on scrim or Esc",
                        Some(
                            "The shared recipe — bg-panel frame, 1px border-strong, modal shadow, scrim + blur",
                        ),
                        widgets::dialog::draw,
                    )],
                ),
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
                                "Only a popup that declares a stage gets it · 20px square left of the X",
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
                            "banner-stack",
                            "Queue & stacking",
                            Some(
                                "One per scope, rest queue (max 5) · lower scope dimmed to 40% behind",
                            ),
                            widgets::banner::draw_stack,
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
                    ],
                ),
                section(
                    "palette",
                    "Command palette",
                    vec![spec(
                        "palette",
                        "Top-anchored, fuzzy, keyboard-first",
                        Some("480px · surface-raised · spawns under the title bar"),
                        components::command_palette::draw,
                    )],
                ),
                section(
                    "tools",
                    "Tools menu",
                    vec![spec(
                        "tools",
                        "Anchored to the sidebar, no scrim",
                        Some("160px popover · builtin actions + plugins"),
                        components::tools_menu::draw,
                    )],
                ),
                section(
                    "ports",
                    "Listening ports",
                    vec![spec(
                        "ports",
                        "Live listeners, copy address",
                        Some("660×520 · 7-column table · sticky header"),
                        components::port_scanner::draw,
                    )],
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
                    ],
                ),
                section(
                    "filepicker",
                    "File picker",
                    vec![
                        spec(
                            "filepicker",
                            "Native file picker — local & remote, one component",
                            Some(
                                "640×480 · PopupDef · differs only in header host badge + breadcrumb root",
                            ),
                            components::file_picker::draw,
                        ),
                        spec(
                            "filepicker-states",
                            "States — loading · empty · permission · connection lost · multi-select",
                            Some("body swaps list ↔ status without changing the frame"),
                            components::file_picker::draw_states,
                        ),
                        spec(
                            "filepicker-save-mode",
                            "Save mode — one confirm, in the footer",
                            Some(
                                "list pick fills the name · Overwrite inline · deep paths elide in the middle",
                            ),
                            components::file_picker::draw_save_mode,
                        ),
                        spec(
                            "filepicker-gesture-table",
                            "The gesture table — a folder selected in either mode",
                            Some(
                                "single click selects · double click descends · a folder is never a save target · Open enters it",
                            ),
                            components::file_picker::draw_gesture_table,
                        ),
                    ],
                ),
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
                section(
                    "search",
                    "Search bar",
                    vec![spec(
                        "search",
                        "Headless, sticky, top-right",
                        Some("360×28 · find bar on the focused surface, no scrim"),
                        components::search_bar::draw,
                    )],
                ),
                section(
                    "workspace-categories",
                    "Workspace categories",
                    vec![spec(
                        "workspace-categories",
                        "Sidebar folders — dialogs & rail popup",
                        Some(
                            "Create/rename (360px + inline validation) · delete confirm (380px danger) · rail popup (176px)",
                        ),
                        components::category_dialogs::draw,
                    )],
                ),
                section(
                    "switch",
                    "Switch-number overlay",
                    vec![
                        spec(
                            "switch-tab",
                            "Tab switch — modifier held",
                            Some(tab_switch_caption()),
                            components::switch_overlay::draw_tab,
                        ),
                        spec(
                            "switch-ws",
                            "Workspace switch — modifier held",
                            Some(workspace_switch_caption()),
                            components::switch_overlay::draw_workspace,
                        ),
                        spec(
                            "switch-cat",
                            "Category switch — modifier held",
                            Some(category_switch_caption()),
                            components::switch_overlay::draw_category,
                        ),
                    ],
                ),
                section(
                    "modhint",
                    "Modifier hints",
                    vec![spec(
                        "modhint",
                        "Held-modifier shortcut panel",
                        Some(
                            "220×400 · hold 500ms → fade in · focus-less, mouse-interactive · release vanishes",
                        ),
                        components::modifier_hint::draw,
                    )],
                ),
                section(
                    "approval",
                    "Agent approval",
                    vec![spec(
                        "approval",
                        "Review the command before it runs",
                        Some("440px · the command and its grants, verbatim"),
                        components::approval::draw,
                    )],
                ),
                section(
                    "convert",
                    "Convert surface",
                    vec![spec(
                        "convert",
                        "Swap a surface's type in place",
                        Some("400px · From → To, scrollback preserved"),
                        components::convert::draw,
                    )],
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
                section(
                    "preset",
                    "Apply preset",
                    vec![spec(
                        "preset",
                        "Apply a saved layout",
                        Some("440px · Workspace / Tab / Pane scope"),
                        components::apply_preset::draw,
                    )],
                ),
                section(
                    "preseteditor",
                    "Preset editor",
                    vec![
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
                section(
                    "rename",
                    "Rename popup",
                    vec![spec(
                        "rename",
                        "One field, autofocused",
                        Some("360px · workspace / subtitle / tab — one view"),
                        components::rename_popup::draw,
                    )],
                ),
                section(
                    "explorer-context",
                    "Explorer context menu",
                    vec![spec(
                        "explorer-context",
                        "Right-click — four targets",
                        Some("empty · file · folder · multi-select — menu_item reuse"),
                        components::explorer_context_menu::draw,
                    )],
                ),
                section(
                    "explorer-favorite",
                    "Add to favorites popup",
                    vec![spec(
                        "explorer-favorite",
                        "Name a global favorite",
                        Some("≈280px · path caption + seeded input · anchored popup"),
                        components::explorer_favorite_popup::draw,
                    )],
                ),
                section(
                    "explorer-rename",
                    "Rename popup (explorer)",
                    vec![spec(
                        "explorer-rename",
                        "Rename a file or folder",
                        Some("≈280px · same skeleton as Add to favorites"),
                        components::explorer_rename_popup::draw,
                    )],
                ),
                section(
                    "settings",
                    "Settings window",
                    vec![
                        spec(
                            "settings",
                            "Three-tier: tabs over sidebar over content",
                            Some("1100×700 · 7 L1 tabs · L2 sidebar · content · footer"),
                            components::settings::draw,
                        ),
                        spec(
                            "settings-file-extension-mapping",
                            "Handler › File Extension Mapping",
                            Some("ext cluster (mono) → handler Select · Add mapping"),
                            components::settings_handler::draw_extension_mapping,
                        ),
                        spec(
                            "settings-file-detectors",
                            "Handler › File Detectors",
                            Some("Detection passes — name + desc rows · Switch"),
                            components::settings_handler::draw_detectors,
                        ),
                        spec(
                            "settings-file-handlers",
                            "Handler › File Handlers",
                            Some("name · kind Tag · Switch rows"),
                            components::settings_handler::draw_file_handlers,
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
                            "settings-remote-transfer",
                            "General › Remote transfer",
                            Some(
                                "Save folder (mono Input + Browse…) · Maximum size (numeric + MiB) · settings-row grid",
                            ),
                            components::settings_remote_transfer::draw,
                        ),
                    ],
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
                            "kbimportexport-migration",
                            "Option migration — pending · resolved · conflict · unbound · not needed",
                            Some(
                                "tinted card gates Apply · record slot / modifier Select · dropped-plugin info line · inline parse failure",
                            ),
                            components::kb_import_export::draw_migration,
                        ),
                        spec(
                            "kbimportexport-open-values",
                            "The six open values — failure, notices, conflicts, placeholder, tokens",
                            Some(
                                "export failure inline in its row · one warning block, 3 lines then fold · parse copy without a line · conflict count from 2 · Select a modifier",
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
                    vec![
                        spec(
                            "scripts-list",
                            "Registered scripts — bound / unbound / changed",
                            Some(
                                "glyph · name+path (middle-elided) · Kbd/Unbound · bind·rename·remove",
                            ),
                            components::script_manager::draw,
                        ),
                        spec(
                            "scripts-empty",
                            "Empty state",
                            Some("Centered glyph + \"No scripts registered\" + Add-script prompt"),
                            components::script_manager::draw_empty,
                        ),
                    ],
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
                section(
                    "notifications",
                    "Notification panel",
                    vec![spec(
                        "notifications",
                        "Unread header · entry list · empty state",
                        Some("350×400 · 전체화면 무대를 선언한 유일한 popup (fit + X)"),
                        components::notification_panel::draw,
                    )],
                ),
                section(
                    "info-modal",
                    "Info modal",
                    vec![spec(
                        "info-modal",
                        "Boot notice queue — one message at a time",
                        Some("440px · height 140..360 · [OK] 가장 오른쪽 + 추가 액션 버튼"),
                        components::info_modal::draw,
                    )],
                ),
                section(
                    "script-confirm",
                    "Script changed confirm",
                    vec![spec(
                        "script-confirm",
                        "TOFU gate — run the changed script?",
                        Some("360px · mono 경로 truncate · changed 태그 · Run anyway / Cancel"),
                        components::script_confirm::draw,
                    )],
                ),
                section(
                    "quit-modal",
                    "Quit confirmation",
                    vec![spec(
                        "quit-modal",
                        "Quit or minimize to background",
                        Some("400×200 독립 창 · close_behavior = \"ask\" 경로"),
                        components::quit_modal::draw,
                    )],
                ),
                section(
                    "plugins-window",
                    "Plugins manager window",
                    vec![spec(
                        "plugins-window",
                        "Installed / Attention / Add plugin",
                        Some(
                            "상태 8 · 헤더 48 + 목록 + 상세 전량 · builtin 점 · health danger dot",
                        ),
                        components::plugins_window::draw,
                    )],
                ),
                section(
                    "drop-overlay",
                    "Drag & drop overlay",
                    vec![spec(
                        "drop-overlay",
                        "Drop to open — hover feedback on the terminal",
                        Some("accent-primary 12% fill + 60% 1px 보더 + 중앙 라벨"),
                        components::drop_overlay::draw,
                    )],
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
                    vec![spec(
                        "sidebar",
                        "Sidebar (Full / Collapsed)",
                        Some("Full 212 expands names; collapsed 52 rail keeps icon slots"),
                        components::sidebar::draw,
                    )],
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
                    vec![spec(
                        "statusbar",
                        "Workspace status bar",
                        Some(
                            "24px read-only summary + one keyboard reminder · collapses grid → \
                             shell → surface id → palette cap → branch text (calls the real \
                             `tasty_ui_widgets` view)",
                        ),
                        components::status_bar::draw,
                    )],
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
                            Some("Focused #000, unfocused 0.92, agent dot"),
                            components::surface_highlights::draw,
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
                // Task DAG — 디자인 `gallery/dag.jsx` 의 NAV 섹션 전부. 캔버스/노드,
                // 크롬/상세/서피스, 그리고 목록 행 + 워크스페이스 popup 세 묶음이다.
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
                                "depends_on 실선 · fallback 6 3 · reduce 2 3, 화살촉은 의존하는 쪽",
                            ),
                            components::dag::edges::draw,
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
                    ],
                ),
                section(
                    "titlebar",
                    "Window titlebar (CSD)",
                    vec![spec(
                        "titlebar",
                        "Active · inactive · close hover",
                        Some("titlebar-height(36) · window-button-size(24) · 하단 1px"),
                        components::titlebar::draw,
                    )],
                ),
                section(
                    "empty-surface",
                    "Empty surface",
                    vec![spec(
                        "empty-surface",
                        "One button, nothing else",
                        Some("bg-app 전면 · 세로 중앙 · convert popup 을 연다"),
                        components::empty_surface::draw,
                    )],
                ),
            ],
        },
        // ── Plugins ──────────────────────────────────────────────────
        // 플러그인 유래 specimen 을 네이티브와 분리한 전용 페이지. 각 플러그인을
        // 하나의 Section 으로 묶는다(clipboard / git / markdown / image / html).
        Page {
            category: Category::Plugins,
            sections: vec![
                section(
                    "clipboard-viewer",
                    "Clipboard viewer popup",
                    vec![spec(
                        "clipboard-viewer",
                        "Current clipboard, master-detail",
                        Some("480×360 · splitter 0.3 · type list → preview · empty / read-failed"),
                        components::clipboard_viewer::draw,
                    )],
                ),
                section(
                    "git-viewer",
                    "Git worktree viewer popup",
                    vec![spec(
                        "git-viewer",
                        "Worktree rail + status / log / diff",
                        Some("≈960 · splitter H 0.25 · splitter V 0.5 · rail → status/log/diff"),
                        components::git_viewer::draw,
                    )],
                ),
                section(
                    "markdown-viewer",
                    "Markdown surface",
                    vec![spec(
                        "markdown-viewer",
                        "Markdown surface",
                        Some(
                            "6-level prose hierarchy · library-owned body leading · element catalog · load/empty states",
                        ),
                        components::markdown_viewer::draw,
                    )],
                ),
                section(
                    "image-viewer",
                    "Image surface / canvas",
                    vec![spec(
                        "image-viewer",
                        "Image surface / canvas",
                        Some("Toolbar + zoom · canvas=bg-sidebar · loaded / no-image fallback"),
                        components::image_viewer::draw,
                    )],
                ),
                section(
                    "html-chrome",
                    "HTML (webview) chrome",
                    vec![spec(
                        "html-chrome",
                        "HTML (webview) chrome",
                        Some(
                            "Native overlay · thin chrome · boundary / placeholder / loading / error",
                        ),
                        components::html_chrome::draw,
                    )],
                ),
            ],
        },
        // ── Chrome ───────────────────────────────────────────────────
        // 완결 앱 크롬 화면 specimen — 위젯이 아니라 조립된 화면 단위(디자인
        // `gallery/shell.jsx` 의 신규 "Chrome" 그룹 1:1 전사).
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
                    "shutdown-loading",
                    "Shutdown loading screen",
                    vec![
                        spec(
                            "shutdown-loading-default",
                            "Default — 1280×720",
                            Some("Same lockup as boot — only the phase text differs"),
                            chrome_loading::draw_shutdown_default,
                        ),
                        spec(
                            "shutdown-loading-phases",
                            "Phase text — four variants",
                            Some(
                                "SavingLayout / ReclaimingBootWorker / ClosingSurfaces / StoppingPlugins",
                            ),
                            chrome_loading::draw_shutdown_phases,
                        ),
                    ],
                ),
            ],
        },
    ]
}
