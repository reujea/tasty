//! Pure view 함수 + props/action — Full / Collapsed sidebar 의 시각 / 입력 처리.
//!
//! 본 모듈은 AppState / CoreState / 글로벌 `theme::theme()` 에 접근하지 않는다.
//! 호출처 wrapper (`full.rs::draw_full_sidebar`, `collapsed.rs::draw_collapsed_sidebar`)
//! 가 props 추출 + action 매핑을 담당한다. gallery 는 같은 view 를 mock props
//! 로 호출해 시각 검증한다 — props 분리 패턴(`docs/dev-guide/gallery-first.md`).

use crate::adapters::ui::{brand, icons};
use crate::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::{STRUCT_GAP_1, STRUCT_GAP_2, STRUCT_GAP_3};
use tasty_ui_widgets::{TagVariant, hspace, tag, vspace};

// ── 디자인 스케일 밖 폰트 크기 ──────────────────────────────────────────────
//
// **`.5` 로 끝나는 값은 애초에 토큰이 될 수 없다** — 토큰 폰트 크기는 `zoomed()` 의
// `.round()` 를 거쳐 어떤 `ui_scale` 에서도 정수다. semantic 이 없는 primitive(12)도
// 같은 이유로 이름만 붙인다. 규칙 전문은 `docs/design/systems/theme.md`
// "스케일 밖 폰트 값".

/// 드래그 중 표시되는 ghost workspace 이름. DTCG primitive `font-size-12` 는 있으나
/// semantic role 이 없어 `Theme` 필드가 없다 — ADR-0126 대로 **이름에 primitive 임을 남긴다**.
const GHOST_WS_NAME_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// Full / Collapsed 공통 — 사이드바 한 행 (workspace card / square) 에 들어가는
/// 데이터. AppState / CoreState 모두 비의존인 owned/snapshot 값.
#[derive(Debug, Clone)]
pub struct WorkspaceEntryView {
    pub name: String,
    pub subtitle: String,
    pub description: String,
    pub busy_count: usize,
    /// 이 워크스페이스에서 `Completion` kind 로 attention 중인 surface 개수. full 은
    /// 우측(기존 자리) 파란 숫자 배지, collapsed 는 dot 으로 표현(`> 0` 조건, 단
    /// `needs_input_count` 가 dot 색 우선순위에서 이긴다).
    pub completion_count: usize,
    /// 이 워크스페이스에서 `NeedsInput` kind 로 attention 중인 surface 개수. full 은
    /// 좌측(Completion 배지가 있으면 그 왼쪽, 없으면 단독으로 우측) 노란 숫자 배지.
    /// `Completion` 보다 우선순위가 높다 — collapsed dot 은 이 값이 0 초과면 항상
    /// 노랑을 택한다.
    pub needs_input_count: usize,
    /// 다른 client 가 해당 workspace 를 attach 한 상태 (빨간 인디케이터).
    pub attached: bool,
    /// 이 워크스페이스가 원격을 attach 한 client mirror 인지 (하늘색 인디케이터, 항상 켜짐).
    pub is_mirror: bool,
    pub is_active: bool,
}

/// 워크스페이스 카테고리 섹션 1개(사이드바 폴더). `entries` 는 그 카테고리에 속한
/// 워크스페이스를 **전역 인덱스 동반**으로 담는다 — 사이드바 action(클릭/드래그)이
/// `engine.workspaces` 의 전역 인덱스로 동작하므로 그룹 렌더에서도 전역 인덱스를 보존한다.
#[derive(Debug, Clone)]
pub struct CategorySectionView {
    pub id: crate::model::WorkspaceCategoryId,
    /// 표시 라벨 — normal(예약) 은 "워크스페이스" heading, 그 외 카테고리 이름.
    pub label: String,
    pub collapsed: bool,
    /// (전역 인덱스, 행 뷰) 목록.
    pub entries: Vec<(usize, WorkspaceEntryView)>,
}

/// Full sidebar 의 view 입력. labels 는 사전 번역.
pub struct SidebarFullProps<'a> {
    pub theme: &'a Theme,
    /// 워크스페이스 slot 키캡 문자를 읽을 키바인딩 설정 (switch-number overlay).
    pub kb: &'a crate::settings::KeybindingSettings,
    pub workspaces: &'a [WorkspaceEntryView],
    /// `Some` 이면 카테고리 섹션으로 그룹 렌더(토글 on), `None` 이면 기존 평면 렌더.
    pub categories: Option<&'a [CategorySectionView]>,
    pub drag: Option<DragSnapshot>,
    pub tools_label: &'a str,
    pub collapse_label: &'a str,
    pub plugins_label: &'a str,
    pub settings_label: &'a str,
    pub new_workspace_label: &'a str,
    pub workspaces_heading: &'a str,
    pub occupied_hover: &'a str,
    /// mirror(원격 워크스페이스 로컬 mirror) pill 의 hover tooltip.
    pub mirror_hover: &'a str,
    /// mirror pill 라벨 텍스트(예: "Remote") — 표시 시 `.to_uppercase()` 적용.
    pub mirror_pill_label: &'a str,
    /// "확인 필요" plugin 개수. >0 이면 Plugins 버튼에 danger 배지를 그린다.
    pub plugin_alert: usize,
    /// switch-number overlay — 사용자가 `workspace_switch_modifier` 를 누르고 있는 동안 true.
    /// 각 워크스페이스의 leading status dot 을 숫자 키캡(`Alt+1`…`9`)으로 in-place 교체.
    pub workspace_switch_held: bool,
    /// 카테고리 quick-switch overlay — `workspace_switch_modifier`+Shift(기본 Alt+Shift) 홀드
    /// 시 true(folders 기능 on 전제). 카테고리 헤더 우측에 섹션 번호 키캡을 표시. Workspace
    /// 와 상호 배타(Shift 유무) — 동시 true 아님.
    pub category_switch_held: bool,
}

/// 진행 중인 workspace drag-and-drop 의 스냅샷. 호출처가 매 프레임 view 에 전달.
#[derive(Debug, Clone, Copy)]
pub struct DragSnapshot {
    pub ws_idx: usize,
    pub current_y: f32,
}

/// Collapsed sidebar 의 view 입력.
pub struct SidebarCollapsedProps<'a> {
    pub theme: &'a Theme,
    /// 워크스페이스 slot 키캡 문자를 읽을 키바인딩 설정 (switch-number overlay).
    pub kb: &'a crate::settings::KeybindingSettings,
    pub workspaces: &'a [WorkspaceEntryView],
    /// `Some` 이면 카테고리 그룹으로 렌더(토글 on) — `---` 버튼 + 소속 아바타.
    /// `None` 이면 기존 평면 아바타 나열.
    pub categories: Option<&'a [CategorySectionView]>,
    pub tools_hover: &'a str,
    /// "확인 필요" plugin 개수. >0 이면 Plugins 레일 버튼에 danger 배지.
    pub plugin_alert: usize,
    /// switch-number overlay — 사용자가 `workspace_switch_modifier` 를 누르고 있는 동안 true.
    /// 각 워크스페이스의 leading letter avatar 를 숫자 키캡(`Alt+1`…`9`)으로 in-place 교체.
    pub workspace_switch_held: bool,
    /// 카테고리 quick-switch overlay — `workspace_switch_modifier`+Shift 홀드 시 true. 각
    /// 카테고리 경계 `---` 슬롯 중앙에 섹션 번호 키캡을 표시(디자인 C). folders 기능 on 전제.
    pub category_switch_held: bool,
}

/// Full sidebar view 가 보고하는 사용자 의도. wrapper 가 state mutation 으로 변환.
#[derive(Debug, Clone)]
pub enum SidebarFullAction {
    Collapse,
    Plugins,
    Settings,
    ToolsClicked(egui::Rect),
    WorkspaceClicked(usize),
    WorkspaceContextMenu {
        ws_idx: usize,
        x: f32,
        y: f32,
    },
    DragStart {
        ws_idx: usize,
        y: f32,
    },
    DragUpdate {
        y: f32,
    },
    /// 마우스 떼짐 — drop_target=None 이면 drop 위치가 from 과 동일 (순서 변경 없음).
    /// `target_category` 는 그룹 모드에서 드롭 위치가 속한 카테고리 id(평면 모드는 None).
    /// 소속이 다르면 카테고리 이동, 같으면 순서 변경.
    DragReleased {
        drop_target: Option<usize>,
        target_category: Option<crate::model::WorkspaceCategoryId>,
    },
    NewWorkspace,
    /// "New workspace" 버튼 우클릭 — 프리셋으로 새 워크스페이스 생성 진입점.
    NewWorkspaceContextMenu {
        x: f32,
        y: f32,
    },
    /// 카테고리 헤더 클릭 — 접힘/펼침 토글.
    CategoryHeaderToggle(crate::model::WorkspaceCategoryId),
    /// 카테고리 헤더 우클릭 — 카테고리 컨텍스트 메뉴(이름변경/삭제/새 카테고리).
    CategoryHeaderContextMenu {
        cat_id: crate::model::WorkspaceCategoryId,
        x: f32,
        y: f32,
    },
    /// 사이드바 빈 배경 우클릭 — 새 카테고리 · 원격 워크스페이스 추가.
    BackgroundContextMenu {
        x: f32,
        y: f32,
    },
}

/// Collapsed sidebar view 가 보고하는 사용자 의도.
#[derive(Debug, Clone)]
pub enum SidebarCollapsedAction {
    Expand,
    Plugins,
    Settings,
    ToolsClicked(egui::Rect),
    WorkspaceClicked(usize),
    NewWorkspace,
    /// "+" 아이콘 우클릭 — 프리셋으로 새 워크스페이스 생성 진입점.
    NewWorkspaceContextMenu {
        x: f32,
        y: f32,
    },
    /// 레일 `---` 카테고리 버튼 클릭 — 우측 앵커드 팝업 열기. `anchor` 는 버튼 rect.
    RailCategoryClicked {
        cat_id: crate::model::WorkspaceCategoryId,
        anchor: egui::Rect,
    },
}

/// [`draw_full_sidebar_view`] 의 반환값 — 사용자 액션 + 가장자리 리사이즈 우선권 판정.
pub struct SidebarFullDrawResult {
    pub actions: Vec<SidebarFullAction>,
    /// 마우스가 사이드바의 실제 클릭 가능 위젯(헤더 접기, Tools/Plugins/Settings,
    /// 카테고리 헤더, 워크스페이스 카드, New workspace) 위인지. `AppState.resize_edge_widget_hovered`
    /// 에 OR 로 합성된다 — 서쪽 가장자리 리사이즈 마진이 사이드바 폭 안에 있을 때
    /// (사이드바가 보이는 상태) 위젯 단위로만 리사이즈를 양보하기 위함. 목록 아래
    /// 빈 배경의 우클릭 캐처(`Sense::click`, 컨텍스트 메뉴 전용)는 실제 콘텐츠가
    /// 아니라 의도적으로 제외한다(타이틀바 드래그 rect 와 동일 이유 — 빈 여백은
    /// 항상 리사이즈 우선).
    pub resize_priority_hovered: bool,
}

/// [`draw_collapsed_sidebar_view`] 의 반환값 — [`SidebarFullDrawResult`] 와 동형.
pub struct SidebarCollapsedDrawResult {
    pub actions: Vec<SidebarCollapsedAction>,
    /// [`SidebarFullDrawResult::resize_priority_hovered`] 참고 — collapsed 레일의
    /// 펼치기 버튼/Tools/Plugins/Settings/카테고리 `---` 버튼/워크스페이스 아바타 위인지.
    pub resize_priority_hovered: bool,
}

// Sidebar 의 모든 zoom-sensitive 길이는 Theme 토큰에서 가져온다 (Z-1/Z-2 에서
// host UI zoom 곱셈이 토큰 자체에 박힘). 아래는 토큰에서 도출하는 헬퍼.
fn btn_height(th: &Theme) -> f32 {
    th.item_height_tab.value()
}
fn collapsed_icon_size(th: &Theme) -> egui::Vec2 {
    egui::vec2(
        th.sidebar_collapsed_slot_width.value(),
        th.sidebar_collapsed_icon_height.value(),
    )
}

/// Plugins 버튼의 "확인 필요" danger 배지 (개수 표기 pill). `inline_right=true`
/// (확장 사이드바)는 버튼 우측 가장자리 세로 중앙에, false(축소 레일)는 아이콘
/// 우상단 모서리에 그린다.
fn paint_alert_badge(
    ui: &egui::Ui,
    th: &Theme,
    btn_rect: egui::Rect,
    count: usize,
    inline_right: bool,
) {
    let h = 15.0;
    let galley = ui.painter().layout_no_wrap(
        count.to_string(),
        egui::FontId::proportional(th.badge_font_size().value()),
        egui::Color32::from(th.text_on_accent()),
    );
    let pad = 4.0;
    let w = (galley.size().x + pad * 2.0).max(h);
    let center = if inline_right {
        egui::pos2(btn_rect.right() - w / 2.0 - 10.0, btn_rect.center().y)
    } else {
        egui::pos2(
            btn_rect.right() - w / 2.0 - 1.0,
            btn_rect.top() + h / 2.0 + 1.0,
        )
    };
    let badge_rect = egui::Rect::from_center_size(center, egui::vec2(w, h));
    ui.painter()
        .rect_filled(badge_rect, h / 2.0, egui::Color32::from(th.accent_danger()));
    let gp = egui::pos2(
        badge_rect.center().x - galley.size().x / 2.0,
        badge_rect.center().y - galley.size().y / 2.0,
    );
    ui.painter()
        .galley(gp, galley, egui::Color32::from(th.text_on_accent()));
}
/// 워크스페이스 행 개수 배지의 색 variant — 디자인 Badge variant="primary"(파랑,
/// Completion)/"warning"(노랑, NeedsInput). 둘 다 전경은 `text-on-accent` 로 동일.
#[derive(Clone, Copy)]
enum BadgeVariant {
    Primary,
    Warning,
}

impl BadgeVariant {
    fn fill(self, th: &Theme) -> egui::Color32 {
        match self {
            BadgeVariant::Primary => th.accent_primary().into(),
            BadgeVariant::Warning => th.accent_warning().into(),
        }
    }
}

/// 워크스페이스 행 개수 배지 — 디자인 Badge variant="primary"/"warning"
/// (accent 채움 pill + count, 99 초과 시 "99+"). Badge specimen 토큰 전사:
/// min-width/height=badge-size, padding-x=badge-padding-x, font=mono badge-font-size,
/// pill(반경=높이/2), 전경 text-on-accent. `right_to_left` 레이아웃 안에서 크기를
/// allocate 하고 그 자리에 painter 로 그린다.
fn paint_workspace_count_badge(ui: &mut egui::Ui, th: &Theme, count: usize, variant: BadgeVariant) {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let galley = ui.painter().layout_no_wrap(
        label,
        egui::FontId::monospace(th.badge_font_size().value()),
        egui::Color32::from(th.text_on_accent()),
    );
    let size = th.badge_size().value();
    let pad_x = th.badge_padding_x().value();
    let w = (galley.size().x + pad_x * 2.0).max(size);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, size), egui::Sense::hover());
    // pill = 완전 라운드(반경 = 높이/2).
    ui.painter().rect_filled(rect, size / 2.0, variant.fill(th));
    let gp = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter()
        .galley(gp, galley, egui::Color32::from(th.text_on_accent()));
}
fn collapsed_ws_size(th: &Theme) -> egui::Vec2 {
    egui::vec2(
        th.sidebar_collapsed_slot_width.value(),
        th.sidebar_collapsed_workspace_height.value(),
    )
}
fn card_inner_margin_x(th: &Theme) -> i8 {
    th.spacing_sm.value() as i8
}
fn card_inner_margin_y(th: &Theme) -> i8 {
    th.spacing_xs.value() as i8
}

/// 그룹 모드 드롭존 1개 — 섹션(헤더+행, 빈/접힌 카테고리는 헤더만)의 판정·표시 정보.
/// spans 는 렌더 순서대로 연속이라(이전 섹션의 end_y 가 곧 다음 섹션의 시작) 시작 y 를
/// 따로 들지 않는다 — 비-첫 섹션 헤더 위 8px gap 도 그 섹션의 드롭존에 포함된다.
struct SectionSpan {
    id: crate::model::WorkspaceCategoryId,
    /// 섹션이 끝나는 y (다음 섹션 시작 = 이 값).
    end_y: f32,
    /// 카테고리 헤더 rect — 행 rect 가 없는(빈/접힌) 섹션의 marker x·폭·y anchor.
    header_rect: egui::Rect,
    /// 행이 실제로 렌더됐는가 (`!collapsed && !entries.is_empty()`).
    has_visible_rows: bool,
}

/// 드래그 커서 y → 대상 섹션. release(드롭) 판정과 insert marker(가이드)가 **같은
/// 규칙을 공유**해 "가이드가 가리키는 곳 = 놓았을 때 실제 결과" 불변식을 지킨다.
/// 규칙: y 가 끝나기 전인 첫 섹션(위로 벗어나면 첫 섹션), 아래로 벗어나면 마지막 섹션.
/// 평면 모드는 spans 가 비어 None.
fn resolve_drop_section(spans: &[SectionSpan], y: f32) -> Option<&SectionSpan> {
    spans.iter().find(|s| y < s.end_y).or_else(|| spans.last())
}

/// 활성 워크스페이스 자동 스크롤 트리거 판정. `prev` 는 직전 프레임에 저장해 둔 active
/// 전역 인덱스(`None` 이면 아직 한 번도 기록된 적 없는 최초 프레임), `current` 은 이번
/// 프레임의 active 전역 인덱스. 최초 프레임에 트리거하지 않아야 시작 시 불필요한 점프가
/// 없고, 값이 그대로면 사용자가 수동으로 스크롤해 둔 상태를 덮어쓰지 않는다.
fn should_scroll_to_active_workspace(prev: Option<Option<usize>>, current: Option<usize>) -> bool {
    matches!(prev, Some(prev) if prev != current)
}

/// Pure view: full sidebar 내부 (SidePanel 안쪽 ui) 를 그리고 action 리스트
/// 를 반환. 호출처는 SidePanel 을 직접 연다.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: egui 즉시모드 draw — ScrollArea show 클로저 내부 위젯 나열이 구조적(clippy 가 클로저를 과대계상)
pub fn draw_full_sidebar_view(
    ui: &mut egui::Ui,
    props: &SidebarFullProps<'_>,
) -> SidebarFullDrawResult {
    let mut actions: Vec<SidebarFullAction> = Vec::new();
    let mut resize_priority_hovered = false;
    let th = props.theme;

    // 헤더 — 워드마크 `tasty.` + 접기 (ui_kit Sidebar 상단).
    egui::TopBottomPanel::top("workspace_sidebar_header")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            // 디자인 chrome.jsx Sidebar 헤더 padding-top: space-md (10→12 스냅).
            vspace(ui, th.spacing_md);
            let (collapsed, hovered) = draw_sidebar_header(ui, th, props.collapse_label);
            resize_priority_hovered |= hovered;
            if collapsed {
                actions.push(SidebarFullAction::Collapse);
            }
            // 디자인 chrome.jsx Sidebar 헤더 padding-bottom: space-xs (6→4 스냅 — parity-notes 잔차 해소).
            vspace(ui, th.spacing_xs);
        });

    // 바닥 고정 섹션 (Tools / Plugins / Settings). 접기는 헤더로 이동.
    egui::TopBottomPanel::bottom("workspace_sidebar_bottom")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.separator();
            vspace(ui, STRUCT_GAP_2);

            // Tools
            let tools_resp = draw_ghost_block_button(ui, th, Some(icons::TOOLS), props.tools_label);
            resize_priority_hovered |= tools_resp.hovered();
            if tools_resp.clicked() {
                actions.push(SidebarFullAction::ToolsClicked(tools_resp.rect));
            }
            vspace(ui, STRUCT_GAP_2);

            // Plugins (확인 필요 plugin 있으면 우측에 danger 배지)
            let plug_resp = draw_ghost_block_button(ui, th, Some(icons::PLUG), props.plugins_label);
            resize_priority_hovered |= plug_resp.hovered();
            if plug_resp.clicked() {
                actions.push(SidebarFullAction::Plugins);
            }
            if props.plugin_alert > 0 {
                paint_alert_badge(ui, th, plug_resp.rect, props.plugin_alert, true);
            }
            vspace(ui, STRUCT_GAP_2);

            // Settings
            let settings_resp =
                draw_ghost_block_button(ui, th, Some(icons::SETTINGS), props.settings_label);
            resize_priority_hovered |= settings_resp.hovered();
            if settings_resp.clicked() {
                actions.push(SidebarFullAction::Settings);
            }
            vspace(ui, th.spacing_sm);
        });

    // 활성 워크스페이스로 자동 스크롤 — `props.workspaces` 는 그룹/평면 모드 공통으로
    // 전체 목록을 담으므로 여기서 active 전역 인덱스를 한 번만 구하면 모든 전환 경로
    // (quick-switch, 카테고리 경계 이동, 클릭)를 커버한다. egui 메모리에 직전 프레임의
    // active 인덱스를 저장해 두고, 판정 자체는 순수 함수 `should_scroll_to_active_workspace`
    // 로 분리(단위 테스트 대상) — 매 프레임 강제 스크롤하면 사용자의 수동 스크롤을
    // 덮어쓰게 되므로 "실제로 바뀐 프레임"에만 트리거한다.
    let active_idx = props.workspaces.iter().position(|w| w.is_active);
    let active_scroll_track_id = egui::Id::new("sidebar_workspace_active_scroll_track");
    let prev_active_idx: Option<Option<usize>> = ui.data(|d| d.get_temp(active_scroll_track_id));
    let should_scroll_to_active = should_scroll_to_active_workspace(prev_active_idx, active_idx);
    ui.data_mut(|d| d.insert_temp(active_scroll_track_id, active_idx));

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            // 디자인 chrome.jsx: 목록 블록은 행/구분선이 세로 gap 없이 맞붙는다
            // (WorkspaceRow 들이 flex column, 사이 margin 0). egui 기본 item_spacing.y
            // (=spacing_xs≈4) 가 선택 행 배경과 상/하 구분선 사이에 틈을 만들어 0 으로 둔다.
            // 섹션 간 간격은 아래 add_space 들이 명시적으로 준다.
            ui.spacing_mut().item_spacing.y = 0.0;
            vspace(ui, th.spacing_sm);
            let mut card_rects: Vec<(usize, egui::Rect)> = Vec::new();
            // 그룹 모드 드롭존 판정용 — 각 섹션(헤더+행, 빈 카테고리는 헤더만).
            let mut section_spans: Vec<SectionSpan> = Vec::new();

            if let Some(sections) = props.categories {
                // 그룹 렌더(토글 on) — 카테고리별 헤더(chevron) + 소속 행. 접힘/빈
                // 카테고리는 헤더만. normal 은 항상 맨 위(sections 순서 = 표시 순서).
                for (sec_i, section) in sections.iter().enumerate() {
                    // 섹션 간 간격 (디자인 비-첫 섹션 marginTop: space-md — 헤더가 밴드로
                    // 승격되면서 space-sm(8)보다 한 단 넓혀졌다). 헤더 앞에 두어 gap 이 이
                    // 섹션의 드롭존에 포함된다 (spans 는 end_y 연속 — 이전 섹션 end_y 부터가
                    // 이 섹션이므로 gap 도 이쪽에 귀속).
                    if sec_i > 0 {
                        ui.add_space(th.spacing_md.value());
                    }
                    let header = draw_category_header(
                        ui,
                        th,
                        &section.label,
                        section.collapsed,
                        section.entries.len(),
                    );
                    resize_priority_hovered |= header.hovered;
                    // 디자인 B: Alt+Shift 홀드 시 헤더 행 **우측 정렬** 키캡(섹션 번호). chevron
                    // 대체 아님 — chevron 은 접힘상태·auto-expand 회전 담당(load-bearing). 11번째+
                    // 카테고리(슬롯 밖)는 키캡 없음. active = 이 섹션이 현재 워크스페이스 소유.
                    if props.category_switch_held {
                        if let Some(digit) =
                            crate::adapters::ui::switch_overlay::category_digit(props.kb, sec_i)
                        {
                            let active_sec = section.entries.iter().any(|(_, ws)| ws.is_active);
                            let fade = crate::adapters::ui::switch_overlay::appear_fade(
                                ui.ctx(),
                                th,
                                ("cat_header", u64::from(section.id)),
                                props.category_switch_held,
                            );
                            let pad = th.spacing_sm.value();
                            let half = crate::adapters::ui::switch_overlay::keycap_size(th) / 2.0;
                            let center =
                                egui::pos2(header.rect.max.x - pad - half, header.rect.center().y);
                            crate::adapters::ui::switch_overlay::paint_keycap(
                                ui.painter(),
                                th,
                                center,
                                digit,
                                active_sec,
                                fade,
                            );
                        }
                    }
                    if header.toggled {
                        actions.push(SidebarFullAction::CategoryHeaderToggle(section.id));
                    }
                    if let Some(pos) = header.context {
                        actions.push(SidebarFullAction::CategoryHeaderContextMenu {
                            cat_id: section.id,
                            x: pos.x,
                            y: pos.y,
                        });
                    }
                    if !section.collapsed && !section.entries.is_empty() {
                        // 목록 블록 상단 보더는 그리지 않는다 — 헤더 밴드의 bottom hairline이
                        // 이미 그 경계를 그린다(이중선 방지, 디자인 "헤더 밑 첫 행의 top border
                        // 는 그리지 말 것" 규칙).
                        // 키캡은 **active 카테고리**에서만, 그 카테고리 내 **로컬 인덱스**
                        // 로 표시(전역 인덱스 아님). 비활성 카테고리 행은 키캡 미표시 —
                        // 슬롯 단축키가 active 카테고리 로컬 순서로 전환하기 때문(표시=동작).
                        let active_sec = section.entries.iter().any(|(_, ws)| ws.is_active);
                        for (row_i, (global_idx, ws)) in section.entries.iter().enumerate() {
                            if row_i > 0 {
                                draw_list_separator(ui, th, 32.0);
                            }
                            let switch_digit = if props.workspace_switch_held && active_sec {
                                crate::adapters::ui::switch_overlay::workspace_digit(
                                    props.kb, row_i,
                                )
                            } else {
                                None
                            };
                            draw_ws_row(
                                ui,
                                props,
                                *global_idx,
                                ws,
                                switch_digit,
                                should_scroll_to_active,
                                &mut actions,
                                &mut card_rects,
                                &mut resize_priority_hovered,
                            );
                        }
                        // 하단 보더 없음 — 그룹 경계는 헤더 아래 상단 보더 1줄만
                        // (디자인 rowList bottomBorder=false, 2026-07-02 고아 구분선 제거).
                    }
                    // 빈/접힌 카테고리도 헤더 영역이 드롭존(그 카테고리로 편입).
                    section_spans.push(SectionSpan {
                        id: section.id,
                        end_y: ui.cursor().min.y,
                        header_rect: header.rect,
                        has_visible_rows: !section.collapsed && !section.entries.is_empty(),
                    });
                }
            } else {
                // 평면 렌더(토글 off) — 단일 "워크스페이스" heading + 전체 행.
                draw_section_heading(ui, th, props.workspaces_heading);
                vspace(ui, th.spacing_xs);

                // 디자인 chrome.jsx:141-149 — 목록 블록 상단 보더 (separator).
                if !props.workspaces.is_empty() {
                    draw_list_separator(ui, th, 0.0);
                }

                for (i, ws) in props.workspaces.iter().enumerate() {
                    // 행 사이 1px 구분선, 좌측 32px 들여쓰기 (디자인 margin-left:32px).
                    if i > 0 {
                        draw_list_separator(ui, th, 32.0);
                    }
                    // 평면 모드(카테고리 off): 전역=로컬 → 전역 인덱스가 곧 슬롯 인덱스.
                    let switch_digit = if props.workspace_switch_held {
                        crate::adapters::ui::switch_overlay::workspace_digit(props.kb, i)
                    } else {
                        None
                    };
                    draw_ws_row(
                        ui,
                        props,
                        i,
                        ws,
                        switch_digit,
                        should_scroll_to_active,
                        &mut actions,
                        &mut card_rects,
                        &mut resize_priority_hovered,
                    );
                }

                // 디자인 chrome.jsx:141-149 — 목록 블록 하단 보더 (separator).
                if !props.workspaces.is_empty() {
                    draw_list_separator(ui, th, 0.0);
                }
            }

            // Drag release / drop marker / ghost preview. card_rects 는 (전역 인덱스,
            // rect) 를 담으므로 position → 전역 인덱스 매핑으로 그룹/평면 모두 정확.
            if let Some(drag) = props.drag {
                let released = !ui.input(|i| i.pointer.primary_down());
                if released {
                    let pos = card_rects
                        .iter()
                        .position(|(_, rect)| drag.current_y < rect.center().y)
                        .unwrap_or(card_rects.len().saturating_sub(1));
                    let target = card_rects
                        .get(pos)
                        .map(|(gi, _)| *gi)
                        .unwrap_or(drag.ws_idx);
                    let drop = (target != drag.ws_idx).then_some(target);
                    // 드롭 위치가 속한 카테고리(그룹 모드) — marker 와 같은 규칙
                    // (resolve_drop_section). 평면 모드는 spans 가 비어 None.
                    let target_category =
                        resolve_drop_section(&section_spans, drag.current_y).map(|s| s.id);
                    actions.push(SidebarFullAction::DragReleased {
                        drop_target: drop,
                        target_category,
                    });
                } else {
                    // Insert marker — release 와 같은 규칙(resolve_drop_section)으로 대상
                    // 섹션을 판정. 빈/접힌 카테고리(행 rect 없음)는 헤더 바로 아래에
                    // 그린다 (놓으면 그 카테고리로 편입 — 가이드 = 드롭 결과).
                    if let Some(sec) = resolve_drop_section(&section_spans, drag.current_y)
                        .filter(|s| !s.has_visible_rows)
                    {
                        let line = egui::Rect::from_min_size(
                            egui::pos2(sec.header_rect.min.x, sec.header_rect.max.y + 1.0),
                            egui::vec2(sec.header_rect.width(), 2.0),
                        );
                        ui.painter().rect_filled(line, 0.0, th.accent_primary());
                    } else {
                        // 행 경계(reorder 가이드) — 기존 card_rects 규칙.
                        let insert_idx = card_rects
                            .iter()
                            .position(|(_, rect)| drag.current_y < rect.center().y)
                            .unwrap_or(card_rects.len());
                        if let Some(marker_rect) = if insert_idx < card_rects.len() {
                            Some(card_rects[insert_idx].1)
                        } else {
                            card_rects.last().map(|(_, r)| *r)
                        } {
                            let marker_y = if insert_idx < card_rects.len() {
                                marker_rect.min.y - 1.0
                            } else {
                                marker_rect.max.y + 1.0
                            };
                            let line = egui::Rect::from_min_size(
                                egui::pos2(marker_rect.min.x, marker_y),
                                egui::vec2(marker_rect.width(), 2.0),
                            );
                            ui.painter().rect_filled(line, 0.0, th.accent_primary());
                        }
                    }

                    // Ghost card.
                    if let Some(ws) = props.workspaces.get(drag.ws_idx)
                        && let Some((_, first_rect)) = card_rects.first()
                    {
                        let ghost_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                first_rect.min.x,
                                drag.current_y - first_rect.height() / 2.0,
                            ),
                            first_rect.size(),
                        );
                        // 드래그 중 따라다니는 고스트는 반투명이다. 대응 토큰 없음.
                        const DRAG_GHOST_ALPHA: u8 = 180;
                        let ghost_bg = th.surface_raised().with_alpha(DRAG_GHOST_ALPHA).to_egui();
                        let ghost_fg = th.text_primary().with_alpha(DRAG_GHOST_ALPHA).to_egui();
                        ui.painter().rect_filled(ghost_rect, 4.0, ghost_bg);
                        ui.painter().text(
                            ghost_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &ws.name,
                            egui::FontId::proportional(GHOST_WS_NAME_PRIMITIVE_12.value()),
                            ghost_fg,
                        );
                    }
                }
            }

            vspace(ui, th.spacing_xs);
            // 카테고리 기능 ON 이면 + 버튼을 그리지 않는다 — 생성은 카테고리 헤더 메뉴 /
            // 레일 `---` 팝업의 Add workspace 로 이동했기 때문. 배경 우클릭 메뉴는 새 카테고리 ·
            // 원격 워크스페이스 추가뿐이다(`handle_sidebar_background_native_menu` 참고).
            if props.categories.is_none() {
                let new_ws_resp =
                    draw_ghost_block_button(ui, th, Some(icons::PLUS), props.new_workspace_label);
                resize_priority_hovered |= new_ws_resp.hovered();
                if new_ws_resp.clicked() {
                    actions.push(SidebarFullAction::NewWorkspace);
                }
                if new_ws_resp.secondary_clicked() {
                    let pos = new_ws_resp.interact_pointer_pos().unwrap_or_default();
                    actions.push(SidebarFullAction::NewWorkspaceContextMenu { x: pos.x, y: pos.y });
                    ui.painter().rect_stroke(
                        new_ws_resp.rect,
                        4.0,
                        egui::Stroke::new(th.focus_ring_width.value(), th.accent_success()),
                        egui::StrokeKind::Inside,
                    );
                }
                vspace(ui, th.spacing_xs);
            }

            // 목록 아래 빈 배경 우클릭 → 배경 컨텍스트 메뉴(새 카테고리 / 원격
            // 워크스페이스 추가). 그룹·평면 모드 공통(이전엔 그룹 모드 한정이었으나
            // 평면 모드 배경 우클릭에도 원격 추가를 노출하도록 대칭화,
            // `docs/features/workspace-category/index.md` 참고).
            // 남은 스크롤 영역 전체를 우클릭 감지 영역으로. 이 캐처는 빈 배경이라
            // resize_priority_hovered 에 넣지 않는다 — 타이틀바 드래그 rect 와 동일 이유
            // (SidebarFullDrawResult::resize_priority_hovered 문서 참고).
            let remaining = ui.available_size_before_wrap();
            if remaining.y > 1.0 {
                let (_bg_rect, bg_resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), remaining.y),
                    egui::Sense::click(),
                );
                if bg_resp.secondary_clicked() {
                    let pos = bg_resp.interact_pointer_pos().unwrap_or_default();
                    actions.push(SidebarFullAction::BackgroundContextMenu { x: pos.x, y: pos.y });
                }
            }
        });

    SidebarFullDrawResult {
        actions,
        resize_priority_hovered,
    }
}

/// Pure view: collapsed sidebar 내부.
pub fn draw_collapsed_sidebar_view(
    ui: &mut egui::Ui,
    props: &SidebarCollapsedProps<'_>,
) -> SidebarCollapsedDrawResult {
    let mut actions: Vec<SidebarCollapsedAction> = Vec::new();
    let mut resize_priority_hovered = false;
    let th = props.theme;

    // 헤더 — 로고 + 펼치기(») 버튼 (ui_kit CollapsedSidebar 상단).
    egui::TopBottomPanel::top("workspace_sidebar_collapsed_header")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            // 디자인 chrome.jsx CollapsedSidebar padding-top: space-sm (10→8 스냅).
            vspace(ui, th.spacing_sm);
            ui.vertical_centered(|ui| {
                // 로고 (collapsed) — 상단, expand 버튼 위.
                let logo_size = th.sidebar_logo_collapsed_size.value();
                let logo_vec = egui::vec2(logo_size, logo_size);
                let (logo_rect, _) = ui.allocate_exact_size(logo_vec, egui::Sense::hover());
                egui::Image::from_bytes(brand::LOGO_URI, brand::LOGO_PNG)
                    .fit_to_exact_size(logo_vec)
                    .paint_at(ui, logo_rect);
                vspace(ui, th.spacing_xs);
                let (rect, resp) =
                    ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
                resize_priority_hovered |= resp.hovered();
                if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, 4.0, th.hover_overlay.to_egui_premultiplied());
                }
                let color: egui::Color32 = if resp.hovered() {
                    th.text_secondary().into()
                } else {
                    // 물러나는 chrome glyph — `glyph-dim`(disabled 아님).
                    th.glyph_dim().into()
                };
                let sz = th.icon_glyph_size_md.value();
                icons::CHEVRONS_RIGHT.image(sz, color).paint_at(
                    ui,
                    egui::Rect::from_center_size(rect.center(), egui::vec2(sz, sz)),
                );
                if resp.clicked() {
                    actions.push(SidebarCollapsedAction::Expand);
                }
            });
            // 디자인 chrome.jsx rail expand(«) marginBottom: space-sm (6→8 스냅).
            vspace(ui, th.spacing_sm);
        });

    egui::TopBottomPanel::bottom("workspace_sidebar_collapsed_bottom")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.separator();
                vspace(ui, STRUCT_GAP_2);

                // Tools
                let (tools_btn_rect, tools_resp) =
                    ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
                resize_priority_hovered |= tools_resp.hovered();
                paint_icon_button(ui, th, tools_btn_rect, &tools_resp, icons::TOOLS);
                let tools_resp = tools_resp.on_hover_text(props.tools_hover);
                if tools_resp.clicked() {
                    actions.push(SidebarCollapsedAction::ToolsClicked(tools_btn_rect));
                }
                vspace(ui, STRUCT_GAP_2);

                // Plugins (확인 필요 plugin 있으면 우상단에 danger 배지)
                let (rect, resp) =
                    ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
                resize_priority_hovered |= resp.hovered();
                paint_icon_button(ui, th, rect, &resp, icons::PLUG);
                if resp.clicked() {
                    actions.push(SidebarCollapsedAction::Plugins);
                }
                if props.plugin_alert > 0 {
                    paint_alert_badge(ui, th, rect, props.plugin_alert, false);
                }
                vspace(ui, STRUCT_GAP_2);

                // Settings
                let (rect, resp) =
                    ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
                resize_priority_hovered |= resp.hovered();
                paint_icon_button(ui, th, rect, &resp, icons::SETTINGS);
                if resp.clicked() {
                    actions.push(SidebarCollapsedAction::Settings);
                }
                vspace(ui, th.spacing_md);
            });
        });

    ui.vertical_centered(|ui| {
        vspace(ui, th.spacing_xs);
        if let Some(sections) = props.categories {
            // 그룹 렌더(토글 on) — 카테고리마다 `---` 버튼 + (접힘 아니면) 소속 아바타.
            // 접힌/빈 카테고리는 `---` 버튼만. normal 항상 맨 위(sections 순서).
            for (sec_i, section) in sections.iter().enumerate() {
                // 디자인 C: Alt+Shift 홀드 시 `---` 슬롯 중앙에 섹션 번호 키캡. 11번째+ 없음.
                // active = 이 섹션이 현재 워크스페이스 소유. (접힘/빈 카테고리도 `---` 존재 → 키캡.)
                let cat_keycap = if props.category_switch_held {
                    crate::adapters::ui::switch_overlay::category_digit(props.kb, sec_i).map(|d| {
                        let active = section.entries.iter().any(|(_, ws)| ws.is_active);
                        let fade = crate::adapters::ui::switch_overlay::appear_fade(
                            ui.ctx(),
                            th,
                            ("cat_rail", u64::from(section.id)),
                            props.category_switch_held,
                        );
                        (d, active, fade)
                    })
                } else {
                    None
                };
                if let Some(anchor) =
                    draw_rail_category_button(ui, th, cat_keycap, &mut resize_priority_hovered)
                {
                    actions.push(SidebarCollapsedAction::RailCategoryClicked {
                        cat_id: section.id,
                        anchor,
                    });
                }
                if !section.collapsed {
                    // active 카테고리에서만, 로컬 인덱스로 키캡(full 사이드바와 동일).
                    let active_sec = section.entries.iter().any(|(_, ws)| ws.is_active);
                    for (row_i, (global_idx, ws)) in section.entries.iter().enumerate() {
                        let switch_digit = if props.workspace_switch_held && active_sec {
                            crate::adapters::ui::switch_overlay::workspace_digit(props.kb, row_i)
                        } else {
                            None
                        };
                        draw_collapsed_avatar(
                            ui,
                            props,
                            *global_idx,
                            ws,
                            switch_digit,
                            &mut actions,
                            &mut resize_priority_hovered,
                        );
                    }
                }
            }
        } else {
            // 평면 렌더(토글 off) — 전체 아바타 나열.
            for (i, ws) in props.workspaces.iter().enumerate() {
                let switch_digit = if props.workspace_switch_held {
                    crate::adapters::ui::switch_overlay::workspace_digit(props.kb, i)
                } else {
                    None
                };
                draw_collapsed_avatar(
                    ui,
                    props,
                    i,
                    ws,
                    switch_digit,
                    &mut actions,
                    &mut resize_priority_hovered,
                );
            }
        }

        // 카테고리 기능 ON 이면 + 버튼을 그리지 않는다 — 생성은 rail 카테고리
        // 팝업 Add workspace 로 이동했기 때문.
        if props.categories.is_none() {
            vspace(ui, STRUCT_GAP_2);
            let (rect, resp) =
                ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
            resize_priority_hovered |= resp.hovered();
            paint_icon_button(ui, th, rect, &resp, icons::PLUS);
            if resp.clicked() {
                actions.push(SidebarCollapsedAction::NewWorkspace);
            }
            if resp.secondary_clicked() {
                let pos = resp.interact_pointer_pos().unwrap_or_default();
                actions
                    .push(SidebarCollapsedAction::NewWorkspaceContextMenu { x: pos.x, y: pos.y });
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(th.focus_ring_width.value(), th.accent_success()),
                    egui::StrokeKind::Inside,
                );
            }
        }
    });

    SidebarCollapsedDrawResult {
        actions,
        resize_priority_hovered,
    }
}

/// 디자인의 ghost variant block button — 사이드바 좌측 정렬 버튼 공통 (Full
/// New Workspace / Tools / Plugins / Settings). 평소 subtext1 (text-secondary),
/// hover 시 text (text-primary) + overlay_hover 배경, pressed 시 overlay_active.
fn draw_ghost_block_button(
    ui: &mut egui::Ui,
    th: &Theme,
    leading_icon: Option<icons::Icon>,
    label: &str,
) -> egui::Response {
    let full_width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(full_width, btn_height(th)), egui::Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    if pressed {
        ui.painter()
            .rect_filled(rect, 4.0, th.active_overlay.to_egui_premultiplied());
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, th.hover_overlay.to_egui_premultiplied());
    }
    let color: egui::Color32 = if resp.hovered() || pressed {
        th.text_primary().into()
    } else {
        th.text_secondary().into()
    };
    let mut text_x = rect.min.x + 10.0;
    if let Some(icon) = leading_icon {
        let icon_size = 16.0;
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(text_x, rect.center().y - icon_size / 2.0),
            egui::vec2(icon_size, icon_size),
        );
        icon.image(icon_size, color).paint_at(ui, icon_rect);
        text_x = icon_rect.max.x + 8.0;
    }
    ui.painter().text(
        egui::pos2(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(th.sidebar_button_label_font_size.value()),
        color,
    );
    resp
}

/// ui_kit 사이드바 헤더 — 워드마크 `tasty.` (`.` = 브랜드색) + 접기(«).
/// collapse 클릭 여부 반환.
/// 헤더의 접기(«) 버튼 hover 여부를 두 번째 값으로 함께 보고한다 — 호출부가
/// `resize_priority_hovered`(서쪽 가장자리 리사이즈 우선권)에 합성한다.
fn draw_sidebar_header(ui: &mut egui::Ui, th: &Theme, collapse_hover: &str) -> (bool, bool) {
    let mut collapse = false;
    let mut hovered = false;
    ui.horizontal(|ui| {
        // 디자인 chrome.jsx Sidebar 헤더 padding-left 12 (패널 좌우 margin 0).
        hspace(ui, th.spacing_md);
        // 로고(수박 PNG) + 워드마크 `tasty.` 락업 — 부팅 로딩 화면과 공유하는
        // 단일 소스 (`brand::draw_wordmark`).
        brand::draw_wordmark(ui, th, th.sidebar_logo_size, th.sidebar_wordmark_font_size);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 디자인 chrome.jsx Sidebar 헤더 padding-right 12 (패널 좌우 margin 0).
            hspace(ui, th.spacing_md);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
            hovered = resp.hovered();
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 4.0, th.hover_overlay.to_egui_premultiplied());
            }
            // 평소: subtext1 (--text-secondary), hover: text (--text-primary). 톤 한 단계 상향.
            let color: egui::Color32 = if resp.hovered() {
                th.text_primary().into()
            } else {
                th.text_secondary().into()
            };
            let sz = th.icon_glyph_size_md.value();
            icons::CHEVRONS_LEFT.image(sz, color).paint_at(
                ui,
                egui::Rect::from_center_size(rect.center(), egui::vec2(sz, sz)),
            );
            resp.clone().on_hover_text(collapse_hover);
            collapse = resp.clicked();
        });
    });
    (collapse, hovered)
}

/// ui_kit 섹션 헤딩 — 모노 대문자, muted, 좌측 패딩. 트래킹 0.07em (=0.7px @ 10px).
fn draw_section_heading(ui: &mut egui::Ui, th: &Theme, text: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::hover());
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(th.sidebar_section_heading_font_size.value()),
            extra_letter_spacing: 0.7,
            color: th.text_muted().into(),
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let pos = egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, th.text_muted().into());
}

/// 카테고리 헤더 상호작용 결과 — 좌클릭(접힘 토글) / 우클릭(컨텍스트 메뉴 좌표).
/// `rect` 는 헤더가 차지한 영역 — 빈/접힌 섹션의 드롭 marker anchor(`SectionSpan`).
struct HeaderInteraction {
    toggled: bool,
    context: Option<egui::Pos2>,
    rect: egui::Rect,
    /// 리사이즈 우선권용 hover — `resize_priority_hovered` 참고.
    hovered: bool,
}

/// ui_kit 카테고리 헤더 (chrome.jsx `CategoryHeader` 전사) — 밴드(bg-app 면 + 상/하
/// hairline) + chevron + 대문자 캡스 라벨 + 우측 워크스페이스 카운트. 접힘 시 chevron
/// 우향(▶), 펼침 시 하향(▼). hover 시 overlay-hover 배경이 밴드 위에 얹힌다(밴드를
/// 대체하지 않음). 좌클릭=접힘 토글, 우클릭=컨텍스트 메뉴 좌표. 라벨은 muted 에서
/// secondary 로 승격됐다(행보다 아래로 읽히던 문제 수정) — egui UI 폰트에 합성 bold 가
/// 없어(`Theme::sidebar_category_header_fg` 참고) weight 신호는 이 색 승격만으로 낸다.
/// 디자인 padding: 상하=space-sm 대칭(기존 space-xs 에서 확대), 좌우=space-sm.
///
/// `count`(카테고리 소속 워크스페이스 수, 접힘 여부 무관 — 필터링 안 함)는 우측에 상시
/// 노출한다. 디자인은 이 자리를 hover-reveal `+`(카테고리에 워크스페이스 추가) 버튼과
/// 공유하고 hover 시 카운트가 페이드아웃하도록 규정하지만, 그 `+` 버튼 자체가 아직 이
/// 코드베이스에 없어(grep 확인) 페이드아웃할 대상이 없다 — `+` 버튼이 실제로 추가될 때
/// 이 자리에 hover 교대 로직을 함께 넣는다.
fn draw_category_header(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    collapsed: bool,
    count: usize,
) -> HeaderInteraction {
    let pad_top = th.sidebar_category_header_pad_y().value();
    let pad_bottom = th.sidebar_category_header_pad_y().value();
    let pad_left = th.sidebar_category_header_pad_x().value();
    let pad_right = th.sidebar_category_header_pad_x().value();
    let gap = th.spacing_xs.value();
    let label_h = 18.0; // draw_section_heading 헤딩 행 높이와 동일.
    let total_h = pad_top + label_h + pad_bottom;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), total_h),
        egui::Sense::click(),
    );
    let border_w = th.border_width.value();
    ui.painter()
        .rect_filled(rect, 0.0, th.sidebar_category_header_bg().to_egui());
    let border = th.sidebar_category_header_border().to_egui();
    ui.painter().hline(
        rect.x_range(),
        rect.min.y,
        egui::Stroke::new(border_w, border),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.max.y,
        egui::Stroke::new(border_w, border),
    );
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, th.hover_overlay.to_egui_premultiplied());
    }
    let row_center_y = rect.min.y + pad_top + label_h / 2.0;
    // chevron 12px. 접힘=우향, 펼침=하향 (디자인 rotate(90deg) 를 아이콘 교체로).
    let chevron_size = 12.0;
    let chevron_rect = egui::Rect::from_center_size(
        egui::pos2(rect.min.x + pad_left + chevron_size / 2.0, row_center_y),
        egui::vec2(chevron_size, chevron_size),
    );
    let icon = if collapsed {
        icons::CHEVRON_RIGHT
    } else {
        icons::CHEVRON_DOWN
    };
    let fg = th.sidebar_category_header_fg();
    icon.image(chevron_size, fg.into())
        .paint_at(ui, chevron_rect);
    // 라벨 — 디자인 textTransform:uppercase (카테고리명도 대문자). 모노 캡스.
    let text_x = chevron_rect.max.x + gap;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(th.sidebar_section_heading_font_size.value()),
            extra_letter_spacing: 0.7,
            color: fg.into(),
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let pos = egui::pos2(text_x, row_center_y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, fg.into());
    // 우측 워크스페이스 카운트 — 10px mono, text-disabled, 우측 gutter에 pad_right 만큼
    // 여백을 두고 정렬.
    let mut count_job = egui::text::LayoutJob::default();
    count_job.append(
        &count.to_string(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(th.sidebar_category_header_count_font_size().value()),
            color: th.sidebar_category_header_count_fg().into(),
            ..Default::default()
        },
    );
    let count_galley = ui.painter().layout_job(count_job);
    let count_pos = egui::pos2(
        rect.max.x - pad_right - count_galley.size().x,
        row_center_y - count_galley.size().y / 2.0,
    );
    ui.painter().galley(
        count_pos,
        count_galley,
        th.sidebar_category_header_count_fg().into(),
    );
    let context = resp
        .secondary_clicked()
        .then(|| resp.interact_pointer_pos().unwrap_or_default());
    HeaderInteraction {
        toggled: resp.clicked(),
        context,
        rect,
        hovered: resp.hovered(),
    }
}

/// Full 사이드바 워크스페이스 행 1개 — card 렌더 + 클릭/우클릭/드래그 action 을
/// `actions` 로 보고하고 (전역 인덱스, rect) 를 `card_rects` 에 누적한다. 그룹/평면
/// 렌더가 공유한다. `global_idx` 는 반드시 `engine.workspaces` 의 전역 인덱스여야
/// action(WorkspaceClicked/DragStart 등)이 올바른 대상을 가리킨다.
fn draw_ws_row(
    ui: &mut egui::Ui,
    props: &SidebarFullProps<'_>,
    global_idx: usize,
    ws: &WorkspaceEntryView,
    // switch-number overlay: workspace_switch_modifier 홀드 시 leading status dot 을
    // 대체할 키캡 문자. 호출부(섹션 루프)가 로컬 인덱스·active 카테고리 여부를 판단해
    // 넘긴다. None 이면 원래 status dot 유지.
    switch_digit: Option<&str>,
    // 활성 인덱스가 이번 프레임에 바뀌었을 때만 true — 이 행이 active 면 뷰포트 안으로
    // 스크롤을 보정한다(호출부에서 프레임당 한 번 계산, 매 프레임 강제 스크롤 방지).
    should_scroll_to_active: bool,
    actions: &mut Vec<SidebarFullAction>,
    card_rects: &mut Vec<(usize, egui::Rect)>,
    resize_priority_hovered: &mut bool,
) {
    let th = props.theme;
    let fade = crate::adapters::ui::switch_overlay::appear_fade(
        ui.ctx(),
        th,
        ("ws_full", global_idx),
        props.workspace_switch_held,
    );
    let card_rect = draw_workspace_card(
        ui,
        th,
        ws,
        props.occupied_hover,
        props.mirror_hover,
        props.mirror_pill_label,
        switch_digit,
        fade,
    );
    if ws.is_active && should_scroll_to_active {
        // align=None → 이미 뷰포트 안이면 스크롤 무변화, 밖이면 최소 이동으로만 보정.
        ui.scroll_to_rect(card_rect, None);
    }
    let card_response = ui.interact(
        card_rect,
        egui::Id::new(("ws_card", global_idx)),
        egui::Sense::click_and_drag(),
    );
    *resize_priority_hovered |= card_response.hovered();

    if card_response.clicked() {
        actions.push(SidebarFullAction::WorkspaceClicked(global_idx));
    }

    if card_response.secondary_clicked() {
        let pos = card_response.interact_pointer_pos().unwrap_or_default();
        actions.push(SidebarFullAction::WorkspaceContextMenu {
            ws_idx: global_idx,
            x: pos.x,
            y: pos.y,
        });
        ui.painter().rect_stroke(
            card_rect,
            4.0,
            egui::Stroke::new(th.focus_ring_width.value(), th.accent_success()),
            egui::StrokeKind::Inside,
        );
    }

    if card_response.drag_started_by(egui::PointerButton::Primary) {
        let y = card_response
            .interact_pointer_pos()
            .map(|p| p.y)
            .unwrap_or(0.0);
        actions.push(SidebarFullAction::DragStart {
            ws_idx: global_idx,
            y,
        });
    }

    if card_response.dragged_by(egui::PointerButton::Primary)
        && let Some(drag) = props.drag
        && drag.ws_idx == global_idx
        && let Some(pos) = card_response.interact_pointer_pos()
    {
        actions.push(SidebarFullAction::DragUpdate { y: pos.y });
    }

    card_rects.push((global_idx, card_rect));
}

/// 워크스페이스 목록의 1px 수평 구분선 (디자인 `separator` 토큰).
/// 블록 상하 보더는 `left_inset=0`, 행 사이 구분선은 `left_inset=32`(디자인
/// `margin-left:32px`). `separator` 는 premultiplied 반투명 바이트로 저장돼 있어
/// `to_egui_premultiplied()` 로 변환한다.
fn draw_list_separator(ui: &mut egui::Ui, th: &Theme, left_inset: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), th.border_width.value()),
        egui::Sense::hover(),
    );
    let line = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + left_inset, rect.min.y),
        egui::vec2(
            (rect.width() - left_inset).max(0.0),
            th.border_width.value(),
        ),
    );
    ui.painter()
        .rect_filled(line, 0.0, th.separator.to_egui_premultiplied());
}

/// Collapsed 측 IconButton — hover 배경 + SVG icon 그리기 helper.
fn paint_icon_button(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    resp: &egui::Response,
    icon: icons::Icon,
) {
    // pressed (마우스 누른 채 위) > hover > idle. pressed 가 우선, 배경만 강화.
    let radius = th.corner_radius.value();
    let pressed = resp.is_pointer_button_down_on();
    if pressed {
        ui.painter()
            .rect_filled(rect, radius, th.active_overlay.to_egui_premultiplied());
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.hover_overlay.to_egui_premultiplied());
    }
    let color: egui::Color32 = if resp.hovered() || pressed {
        th.text_secondary().into()
    } else {
        // 물러나는 chrome glyph — `glyph-dim`.
        th.glyph_dim().into()
    };
    let icon_size = th.icon_glyph_size_md.value();
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(icon_size, icon_size));
    icon.image(icon_size, color).paint_at(ui, icon_rect);
}

/// 레일 카테고리 경계 `---` 버튼 (chrome.jsx `RailCategoryBtn` 전사). 클릭 시 버튼
/// rect 를 반환(우측 앵커 팝업 위치 계산용). 폭=slot_width(디자인 size-36 자리, 아바타
/// 열 정렬), 높이=spacing_lg(=size-16), 내부 선 폭=slot_width-spacing_sm(=size-24)·
/// 높이=border_width, idle=separator / hover=text-muted.
/// `keycap` = `Some((digit, active, fade))` 이면 디자인 C 대로 `---` 라인 대신 그 슬롯
/// 중앙에 카테고리 번호 키캡을 그린다(Alt+Shift 홀드). `None` 이면 기존 `---` 라인.
fn draw_rail_category_button(
    ui: &mut egui::Ui,
    th: &Theme,
    keycap: Option<(&str, bool, f32)>,
    resize_priority_hovered: &mut bool,
) -> Option<egui::Rect> {
    let w = th.sidebar_collapsed_slot_width.value();
    let h = th.spacing_lg.value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    *resize_priority_hovered |= resp.hovered();
    let radius = th.corner_radius.value();
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.hover_overlay.to_egui_premultiplied());
    }
    match keycap {
        // 디자인 C: `---` 슬롯 자리가 그대로 키캡이 된다(라인 대체).
        Some((digit, active, fade)) => {
            crate::adapters::ui::switch_overlay::paint_keycap(
                ui.painter(),
                th,
                rect.center(),
                digit,
                active,
                fade,
            );
        }
        None => {
            let line_w = (w - th.spacing_sm.value()).max(0.0);
            let line_h = th.border_width.value();
            let line_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(line_w, line_h));
            let line_color: egui::Color32 = if resp.hovered() {
                th.text_muted().into()
            } else {
                th.separator.to_egui_premultiplied()
            };
            ui.painter().rect_filled(line_rect, 0.0, line_color);
        }
    }
    resp.clicked().then_some(rect)
}

/// Collapsed 레일 워크스페이스 아바타 1개 — 머리글자 사각 + 상태 dot/링 + 클릭 action.
/// 그룹/평면 렌더가 공유한다. `global_idx` 는 반드시 전역 인덱스여야 클릭/switch overlay
/// 가 올바른 워크스페이스를 가리킨다.
fn draw_collapsed_avatar(
    ui: &mut egui::Ui,
    props: &SidebarCollapsedProps<'_>,
    global_idx: usize,
    ws: &WorkspaceEntryView,
    // switch-number overlay 키캡 문자(호출부 판단). None 이면 letter avatar 유지.
    switch_digit: Option<&str>,
    actions: &mut Vec<SidebarCollapsedAction>,
    resize_priority_hovered: &mut bool,
) {
    let th = props.theme;
    // 디자인 (chrome.jsx CollapsedSidebar): 워크스페이스 이름 첫 글자 대문자,
    // mono 13 bold. 빈 이름이면 라벨 생략 (한글/이모지도 안전하게 chars().next()).
    let label = ws
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    // G3: 디자인 IconButton.active — active = bg overlay-active + 글자색
    // accent-primary(blue), 테두리 없음. inactive = bg 없음 + 글자색 text-muted.
    // G4: notif 의 글자색(yellow) 표현은 제거 — notif 는 우상단 dot 으로만.
    let text_color: egui::Color32 = if ws.is_active {
        th.accent_primary().into()
    } else {
        th.text_muted().into()
    };

    let (rect, resp) = ui.allocate_exact_size(collapsed_ws_size(th), egui::Sense::click());
    *resize_priority_hovered |= resp.hovered();
    if ws.is_active {
        ui.painter()
            .rect_filled(rect, 4.0, th.overlay_active().to_egui_premultiplied());
    }
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, th.hover_overlay.to_egui_premultiplied());
    }
    // switch-number overlay: workspace_switch_modifier 홀드 시 letter avatar 자리에
    // 숫자 키캡을 in-place 그린다(코너 상태 dot 은 유지). 키캡 문자는 호출부가 판단.
    // 등장 페이드(90ms, motion-ui-fast) — held 여부로 매 프레임 구동.
    let fade = crate::adapters::ui::switch_overlay::appear_fade(
        ui.ctx(),
        th,
        ("ws_collapsed", global_idx),
        props.workspace_switch_held,
    );
    if let Some(digit) = switch_digit {
        crate::adapters::ui::switch_overlay::paint_keycap(
            ui.painter(),
            th,
            rect.center(),
            digit,
            ws.is_active,
            fade,
        );
    } else if !label.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &label,
            egui::FontId::monospace(th.font_size_body.value()),
            text_color,
        );
    }
    // 우상단 dot — notif(blue+링) > running(초록). attached 는 아바타 둘레 lavender ring,
    // mirror 는 우하단 corner chip(아래) 로 분리 — dot 은 실행상태 전용
    // (디자인 2026-07-02 workspace-mirror-indicator: sky "remote" fill 제거).
    // 접힌 rail 은 24px 크롬 계열 — 점 가족 규칙상 compact 6.
    let dot_radius = th.status_dot_size_compact().value() * 0.5;
    let dot_pad = 4.0;
    let dot_center = egui::pos2(
        rect.max.x - dot_pad - dot_radius,
        rect.min.y + dot_pad + dot_radius,
    );
    // 우선순위(디자인 확정): NeedsInput(노랑) > Completion(파랑) > running(초록).
    // 52px 레일에서 dot 2개는 기각됐다 — 최고 랭크 kind 1개만 색으로 표시(카운트는
    // 확장 사이드바 배지가 담당).
    if ws.needs_input_count > 0 {
        ui.painter()
            .circle_filled(dot_center, dot_radius + 1.5, th.bg_sidebar());
        ui.painter()
            .circle_filled(dot_center, dot_radius, th.accent_warning());
    } else if ws.completion_count > 0 {
        // G4: notif → blue dot + bg-sidebar 링 (디자인 Badge dot variant, boxShadow 0 0 0 1.5px).
        ui.painter()
            .circle_filled(dot_center, dot_radius + 1.5, th.bg_sidebar());
        ui.painter()
            .circle_filled(dot_center, dot_radius, th.accent_primary());
    } else if ws.busy_count > 0 {
        ui.painter()
            .circle_filled(dot_center, dot_radius, th.accent_success());
    }
    // attached(다른 client 점유) → 아바타 둘레 lavender ring. 굵기는
    // `status-dot-attached-ring-width`(2). red(error) 재사용 분리.
    if ws.attached {
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(
                th.status_dot_attached_ring_width().value(),
                th.border_attached(),
            ),
            egui::StrokeKind::Inside,
        );
    }
    // 디자인 CollapsedSidebar mirror chip: 아바타 우하단 sky corner chip. pill(size-12) +
    // boxShadow spread(size-2) 는 둘 다 bg-sidebar → 반경 spacing_sm(=size-8) 의
    // bg-sidebar halo 로 합성 렌더(아바타/이웃과 시각 분리). glyph=spacing_sm(=size-8, `>_→`
    // TERMINAL_PROMPT), tint=workspace_mirror_fg. 채널 분리: notif=우상단 / mirror=우하단 /
    // attached=둘레 ring — 셋이 겹치지 않는다.
    if ws.is_mirror {
        let halo_r = th.spacing_sm.value();
        let glyph = th.spacing_sm.value();
        let inset = th.spacing_xs.value();
        let chip_center = egui::pos2(rect.max.x - inset, rect.max.y - inset);
        ui.painter()
            .circle_filled(chip_center, halo_r, th.bg_sidebar());
        // paint_at: layout 을 건드리지 않는 순수 페인트(아바타 rect 는 위에서 이미 할당됨).
        // collapsed 의 다른 인디케이터(notif/attached)와 동일하게 per-chip tooltip 은 없다.
        let glyph_rect = egui::Rect::from_center_size(chip_center, egui::vec2(glyph, glyph));
        icons::TERMINAL_PROMPT
            .image(glyph, th.workspace_mirror_fg().into())
            .paint_at(ui, glyph_rect);
    }
    if resp.clicked() {
        actions.push(SidebarCollapsedAction::WorkspaceClicked(global_idx));
    }
}

/// Full 사이드바의 workspace card 1 장 — Frame::show 로 직접 그리고 점유한 rect 반환.
fn draw_workspace_card(
    ui: &mut egui::Ui,
    th: &Theme,
    ws: &WorkspaceEntryView,
    occupied_hover: &str,
    mirror_hover: &str,
    mirror_pill_label: &str,
    // Some(digit) 면 status dot 자리에 숫자 키캡(switch-number overlay)을 그린다.
    switch_digit: Option<&str>,
    // switch-number overlay 등장 페이드 계수(0..=1, motion-ui-fast 90ms).
    switch_fade: f32,
) -> egui::Rect {
    // ui_kit WorkspaceRow — 테두리 없는 플랫 행. active 만 배경 채움 (`--surface-active`
    // = catppuccin surface2).
    let bg = if ws.is_active {
        th.surface_active().to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };

    // 디자인 chrome.jsx WorkspaceRow: 행은 좌우 margin 없이 사이드바 폭을 꽉 채우고
    // (active bg full-bleed), 모서리는 사각(border-radius 없음). 좌우 padding 10 은
    // inner_margin 이 갖는다. 과거 outer_margin(6,0) + corner_radius(2) 는 배경을
    // 가장자리에서 6px 떼어 좌측 accent bar(아래)와 우측 보더 사이에 틈을 만들었다.
    let frame = egui::Frame::new()
        .fill(bg)
        // 좌측은 상태 dot 여백을 줄이기 위해 spacing_xs(4), 우측은 "!" highlight
        // 배지 위치 유지를 위해 spacing_sm(8) 으로 비대칭 적용.
        .inner_margin(egui::Margin {
            left: th.spacing_xs.value() as i8,
            right: card_inner_margin_x(th),
            top: card_inner_margin_y(th),
            bottom: card_inner_margin_y(th),
        });

    let response = frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
            // 좌측 상태 dot — 디자인 StatusDot (running/idle/agent/waiting/error)
            // 중 ws-level 데이터로 결정 가능한 case 만 표시. dot 은 항상 렌더하고
            // 색만 상태별로 분기한다 (디자인 StatusDot 은 idle 에도 점을 그림).
            // fill: running(busy_count>0) → accent-success
            //       > idle → `status-dot-idle` (StatusDot Idle 과 같은 role).
            //       mirror(원격 origin)는 fill 이 아니라 아래 "REMOTE" pill 로 표시한다.
            // attached(다른 client 점유)는 fill 이 아니라 dot 을 감싸는 ring
            //   (디자인 StatusDot attached prop — 굵기·offset 은
            //   `status-dot-attached-ring-width` · `-offset` 토큰). red 는
            //   error 전용으로 보존 — attached 에 red 재사용 시 error 와 충돌하므로 분리.
            // 디자인의 agent / waiting case 는 ws-level 데이터 부재로 보류.
            // 슬롯 폭 = spacing_sm(현재 점 지름 `badge-dot-size` 와 같은 값, 좌우 내부
            // 패딩 0) — dot 유무와 무관하게 항상 점유되어 라벨 시작 x 가 흔들리지
            // 않는다. 높이는 행 높이 안정을 위해 16px 유지.
            let dot_slot = egui::vec2(th.spacing_sm.value(), 16.0);
            let (dot_rect, dot_resp) = ui.allocate_exact_size(dot_slot, egui::Sense::hover());
            if let Some(digit) = switch_digit {
                // switch-number overlay: dot 슬롯(8px) 중앙에 16px 키캡을 그린다. 슬롯
                // 할당은 그대로라 라벨 시작 x 불변 → 리플로 없음. 키캡 좌/우 edge 는
                // 카드 좌측 inner edge ~ 라벨 시작 사이에 정확히 들어간다(겹침 0).
                crate::adapters::ui::switch_overlay::paint_keycap(
                    ui.painter(),
                    th,
                    dot_rect.center(),
                    digit,
                    ws.is_active,
                    switch_fade,
                );
            } else {
                // 디자인 StatusDot: 활성/비활성 무관하게 같은 색 (alpha 조정 없음).
                // dot 은 실행상태 전용 — mirror(원격 origin)는 이름과 subtitle 사이 별도
                // 줄의 "REMOTE" pill 로 분리. sky "remote" fill 제거.
                let dot_color: egui::Color32 = if ws.busy_count > 0 {
                    th.accent_success().into()
                } else {
                    // idle 상태 점 — StatusDot Idle 과 같은 role(`status-dot-idle`).
                    th.status_dot_idle().into()
                };
                // 지름은 `badge-dot-size` 에서 온다 — 여기 4 를 박으면 같은 슬롯에
                // 겹쳐 그려지는 키캡만 `ui_zoom` 을 타서 배율에서 둘이 갈린다.
                // `tasty_ui_widgets::paint_badge_dot` 을 부르지 못하는 이유는 색뿐이다:
                // idle 색 `status_dot_idle` 에 대응하는 `BadgeVariant` 가 없다.
                let dot_r = th.badge_dot_size().value() * 0.5;
                ui.painter()
                    .circle_filled(dot_rect.center(), dot_r, dot_color);
                // attached → dot 을 감싸는 lavender ring. offset 은 **점 바깥 edge →
                // ring 안쪽 edge** 로 재므로 반지름은 dot 반지름 + offset + 굵기 절반.
                // 총 bbox = dot + 2×(offset + 굵기) 로 24px 바 안에 남는다.
                if ws.attached {
                    let ring_w = th.status_dot_attached_ring_width().value();
                    ui.painter().circle_stroke(
                        dot_rect.center(),
                        dot_r + th.status_dot_attached_ring_offset().value() + ring_w * 0.5,
                        egui::Stroke::new(ring_w, th.border_attached()),
                    );
                }
                if ws.attached && ws.busy_count == 0 {
                    dot_resp.on_hover_text(occupied_hover);
                }
            }
            // G5/J4: active 이름 text-primary, inactive 이름 text-secondary (한 단계
            // 어두움). 강조는 색으로만 — 디자인엔 굵기 차이가 없어 .strong() 미사용.
            let name_color = if ws.is_active {
                th.text_primary()
            } else {
                th.text_secondary()
            };

            // 디자인 위계: title 13px(font_size_body), 단일 줄 ellipsis. badge 를 먼저
            // 우측에 점유시킨 뒤 남은 좌측 폭을 title 이 채우며 길면 말줄임한다
            // (truncate 가 가용폭을 모두 먹어 badge 를 밀어내지 않도록 reserve-first).
            //
            // 디자인 확정: 배지 자리(우측)는 kind 와 무관하게 유지 — kind 1개면 그
            // 자리 단독 차지, 2개면 NeedsInput(노랑)이 앞(좌측)·Completion(파랑)이
            // 뒤(우측, 기존 자리), 사이 간격은 badge-group-gap(=spacing_xs). Completion
            // 을 먼저 그려야 right_to_left 레이아웃에서 가장 오른쪽에 앉는다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ws.completion_count > 0 {
                    paint_workspace_count_badge(ui, th, ws.completion_count, BadgeVariant::Primary);
                }
                if ws.needs_input_count > 0 {
                    if ws.completion_count > 0 {
                        ui.add_space(th.spacing_xs.value());
                    }
                    paint_workspace_count_badge(
                        ui,
                        th,
                        ws.needs_input_count,
                        BadgeVariant::Warning,
                    );
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&ws.name)
                                .size(th.font_size_body.value())
                                .color(name_color),
                        )
                        .truncate(),
                    );
                });
            });
        });

        // mirror(원격 워크스페이스 로컬 mirror)는 타이틀 행의 leading glyph 가
        // 아니라 이름과 subtitle 사이 별도 줄의 sky "REMOTE" pill 로 표시한다
        // (가시성 강화). subtitle 유무와 독립 — subtitle 이 없어도 그린다. collapsed rail 의
        // corner chip(이 함수 밖)은 이 변경과 무관, 그대로 유지.
        if ws.is_mirror {
            vspace(ui, STRUCT_GAP_1);
            let resp = ui.horizontal(|ui| {
                // 타이틀 시작 x 정렬: subtitle/description 과 동일 인셋
                // (dot 슬롯 spacing_sm=8 + item_spacing spacing_xs=4).
                ui.add_space(th.spacing_sm.value() + th.spacing_xs.value());
                ui.spacing_mut().item_spacing.x = th.workspace_mirror_gap().value();
                ui.add(icons::TERMINAL_PROMPT.image(
                    th.workspace_mirror_icon_size().value(),
                    th.workspace_mirror_fg().into(),
                ));
                tag(
                    ui,
                    th,
                    &mirror_pill_label.to_uppercase(),
                    TagVariant::Remote,
                    false,
                );
            });
            resp.response.on_hover_text(mirror_hover);
        }

        if !ws.subtitle.is_empty() {
            // 디자인 margin-top 1px (title 과의 위계 간격).
            vspace(ui, STRUCT_GAP_1);
            ui.horizontal(|ui| {
                // 타이틀 시작 x 정렬: dot 슬롯(spacing_sm=8) + item_spacing(spacing_xs=4).
                ui.add_space(th.spacing_sm.value() + th.spacing_xs.value());
                // 디자인 WorkspaceRow subtitle: sidebar_button_label_font_size (11px —
                // 2026-07-02 디자인 판정: 12 는 터미널 전용, font-size-caption 스냅)
                // 본문 sans, text-muted, 단일 줄 ellipsis. "짧은 라벨"로 읽히도록
                // 코드체(mono) 가 아니라 일반 UI 폰트로 그린다 (위계: description 보다
                // 한 단계 크고 진함).
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&ws.subtitle)
                            .size(th.sidebar_button_label_font_size.value())
                            .color(th.text_muted()),
                    )
                    .truncate(),
                );
            });
        }

        if !ws.description.is_empty() {
            // 디자인 margin-top 3px (subtitle 보다 한 단계 넓은 위계 간격).
            vspace(ui, STRUCT_GAP_3);
            ui.horizontal(|ui| {
                // 서브타이틀과 동일하게 타이틀 시작 x 정렬: 슬롯(spacing_sm=8)+spacing(spacing_xs=4).
                ui.add_space(th.spacing_sm.value() + th.spacing_xs.value());
                // 디자인 description: 11px(font_size_caption), text-placeholder(가장 흐린
                // 톤), line-height 1.35, 긴 설명문은 최대 2줄 후 말줄임(2-line clamp).
                let size = th.font_size_caption.value();
                let mut job = egui::text::LayoutJob {
                    wrap: egui::text::TextWrapping {
                        max_width: ui.available_width(),
                        max_rows: 2,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                job.append(
                    &ws.description,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(size),
                        color: th.text_placeholder().into(),
                        line_height: Some(size * 1.35),
                        ..Default::default()
                    },
                );
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.label(galley);
            });
        }
    });

    let card_rect = response.response.rect;

    if !ws.is_active && response.response.hovered() {
        // full-bleed 행 — bg 와 동일하게 사각(상/하 구분선과 틈 없이 맞붙도록).
        ui.painter()
            .rect_filled(card_rect, 0.0, th.hover_overlay.to_egui_premultiplied());
    }

    // Active 좌측 2px inset accent bar (디자인 `boxShadow: inset 2px 0 0 var(--accent-primary)`).
    // 카드 좌측 가장 안쪽 모서리(x 0~2). dot 슬롯은 좌측 inner_margin(spacing_xs=4)
    // 부터 시작 → bar(0~2)와 dot(4~12)가 2px 간격으로 겹치지 않는다.
    if ws.is_active {
        let bar = egui::Rect::from_min_size(card_rect.min, egui::vec2(2.0, card_rect.height()));
        ui.painter().rect_filled(bar, 0.0, th.accent_primary());
    }

    card_rect
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn mock_ws(name: &str, is_active: bool) -> WorkspaceEntryView {
        WorkspaceEntryView {
            name: name.to_string(),
            subtitle: String::new(),
            description: String::new(),
            busy_count: 0,
            completion_count: 0,
            needs_input_count: 0,
            attached: false,
            is_mirror: false,
            is_active,
        }
    }

    fn mock_span(id: u32, end_y: f32, has_visible_rows: bool) -> SectionSpan {
        SectionSpan {
            id,
            end_y,
            header_rect: egui::Rect::from_min_size(
                egui::pos2(0.0, end_y - 26.0),
                egui::vec2(180.0, 26.0),
            ),
            has_visible_rows,
        }
    }

    #[test]
    fn resolve_drop_section_maps_cursor_to_sections() {
        // normal(행 있음, ~100) / Services(빈, ~150) / Archived(접힘, ~200).
        let spans = vec![
            mock_span(0, 100.0, true),
            mock_span(1, 150.0, false),
            mock_span(2, 200.0, false),
        ];
        // 섹션 내부.
        assert_eq!(resolve_drop_section(&spans, 50.0).map(|s| s.id), Some(0));
        assert_eq!(resolve_drop_section(&spans, 120.0).map(|s| s.id), Some(1));
        assert_eq!(resolve_drop_section(&spans, 160.0).map(|s| s.id), Some(2));
        // 위로 벗어남 → 첫 섹션.
        assert_eq!(resolve_drop_section(&spans, -10.0).map(|s| s.id), Some(0));
        // 아래로 벗어남 → 마지막 섹션.
        assert_eq!(resolve_drop_section(&spans, 999.0).map(|s| s.id), Some(2));
        // 경계값: 섹션 간 gap 은 end_y 연속으로 다음 섹션에 귀속 (y == 이전 end_y).
        assert_eq!(resolve_drop_section(&spans, 100.0).map(|s| s.id), Some(1));
        // marker 분기 조건(has_visible_rows) — 빈/접힌 섹션만 헤더 marker.
        assert!(resolve_drop_section(&spans, 50.0).unwrap().has_visible_rows);
        assert!(
            !resolve_drop_section(&spans, 120.0)
                .unwrap()
                .has_visible_rows
        );
    }

    #[test]
    fn resolve_drop_section_empty_spans_yields_none() {
        // 평면 모드(토글 off): spans 비어 있음 → None (기존 reorder 경로 유지).
        assert!(resolve_drop_section(&[], 42.0).is_none());
    }

    #[test]
    fn should_scroll_to_active_workspace_skips_first_frame() {
        // 최초 프레임(직전 기록 없음) — 시작 시 불필요한 점프 방지.
        assert!(!should_scroll_to_active_workspace(None, Some(3)));
        assert!(!should_scroll_to_active_workspace(None, None));
    }

    #[test]
    fn should_scroll_to_active_workspace_skips_when_unchanged() {
        // 활성 인덱스가 그대로면 사용자가 수동 스크롤해 둔 상태를 덮어쓰지 않는다.
        assert!(!should_scroll_to_active_workspace(Some(Some(3)), Some(3)));
        assert!(!should_scroll_to_active_workspace(Some(None), None));
    }

    #[test]
    fn should_scroll_to_active_workspace_triggers_on_change() {
        // quick-switch/카테고리 경계 이동 등으로 active 전역 인덱스가 바뀐 프레임.
        assert!(should_scroll_to_active_workspace(Some(Some(3)), Some(7)));
        // 워크스페이스가 전부 닫혀 active 가 없어진 경우도 "바뀜"으로 취급.
        assert!(should_scroll_to_active_workspace(Some(Some(3)), None));
        assert!(should_scroll_to_active_workspace(Some(None), Some(0)));
    }

    fn run_full(workspaces: Vec<WorkspaceEntryView>, switch_held: bool) -> Vec<SidebarFullAction> {
        let ctx = egui::Context::default();
        let mut out: Vec<SidebarFullAction> = Vec::new();
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("test_full").show(ctx, |ui| {
                let props = SidebarFullProps {
                    theme: &theme,
                    kb: &kb,
                    workspaces: &workspaces,
                    categories: None,
                    drag: None,
                    tools_label: "Tools",
                    collapse_label: "Collapse",
                    plugins_label: "Plugins",
                    settings_label: "Settings",
                    new_workspace_label: "New Workspace",
                    workspaces_heading: "WORKSPACES",
                    occupied_hover: "Held by another client",
                    mirror_hover: "Mirror of a remote workspace",
                    mirror_pill_label: "REMOTE",
                    plugin_alert: 0,
                    workspace_switch_held: switch_held,
                    category_switch_held: false,
                };
                out = draw_full_sidebar_view(ui, &props).actions;
            });
        }));
        out
    }

    fn run_collapsed(
        workspaces: Vec<WorkspaceEntryView>,
        switch_held: bool,
    ) -> Vec<SidebarCollapsedAction> {
        let ctx = egui::Context::default();
        let mut out: Vec<SidebarCollapsedAction> = Vec::new();
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("test_collapsed").show(ctx, |ui| {
                let props = SidebarCollapsedProps {
                    theme: &theme,
                    kb: &kb,
                    workspaces: &workspaces,
                    categories: None,
                    tools_hover: "Tools menu",
                    plugin_alert: 0,
                    workspace_switch_held: switch_held,
                    category_switch_held: false,
                };
                out = draw_collapsed_sidebar_view(ui, &props).actions;
            });
        }));
        out
    }

    fn run_collapsed_grouped(
        sections: Vec<CategorySectionView>,
        workspaces: Vec<WorkspaceEntryView>,
        switch_held: bool,
    ) -> Vec<SidebarCollapsedAction> {
        let ctx = egui::Context::default();
        let mut out: Vec<SidebarCollapsedAction> = Vec::new();
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("test_collapsed_grouped").show(ctx, |ui| {
                let props = SidebarCollapsedProps {
                    theme: &theme,
                    kb: &kb,
                    workspaces: &workspaces,
                    categories: Some(&sections),
                    tools_hover: "Tools menu",
                    plugin_alert: 0,
                    workspace_switch_held: switch_held,
                    category_switch_held: false,
                };
                out = draw_collapsed_sidebar_view(ui, &props).actions;
            });
        }));
        out
    }

    #[test]
    fn collapsed_view_grouped_renders_rail_without_panic() {
        // normal(펼침, 2 아바타) + Services(접힘) + Archived(빈) — `---` 버튼/아바타/
        // 접힘/빈 경로 전부 패닉 없이 layout 되는지.
        let workspaces = vec![mock_ws("a", true), mock_ws("b", false), mock_ws("c", false)];
        let sections = vec![
            CategorySectionView {
                id: 0,
                label: "WORKSPACES".into(),
                collapsed: false,
                entries: vec![(0, mock_ws("a", true)), (1, mock_ws("b", false))],
            },
            CategorySectionView {
                id: 1,
                label: "Services".into(),
                collapsed: true,
                entries: vec![(2, mock_ws("c", false))],
            },
            CategorySectionView {
                id: 2,
                label: "Archived".into(),
                collapsed: false,
                entries: vec![],
            },
        ];
        let actions = run_collapsed_grouped(sections, workspaces, false);
        assert!(actions.is_empty(), "expected no actions, got {actions:?}");
    }

    #[test]
    fn full_view_no_input_yields_no_actions() {
        let ws = vec![mock_ws("Default", true)];
        let actions = run_full(ws, false);
        assert!(actions.is_empty(), "expected no actions, got {actions:?}");
    }

    #[test]
    fn full_view_renders_many_without_panic() {
        let ws: Vec<_> = (0..10)
            .map(|i| mock_ws(&format!("ws-{i}"), i == 0))
            .collect();
        let actions = run_full(ws, false);
        assert!(actions.is_empty());
    }

    #[test]
    fn mirror_indicator_renders_full_and_collapsed_without_panic() {
        // is_mirror=true 워크스페이스: full 은 이름과 subtitle 사이 별도 줄의 "REMOTE"
        // pill, collapsed 는 아바타 우하단 corner chip 을 그린다. busy+
        // attached+notif 와 공존하는 mirror 행도 섞어 채널 분리 렌더 경로를 no-panic 검증.
        let mut mirror = mock_ws("infra", false);
        mirror.is_mirror = true;
        let mut mirror_busy = mock_ws("data-pipeline", true);
        mirror_busy.is_mirror = true;
        mirror_busy.busy_count = 2;
        mirror_busy.completion_count = 3;
        mirror_busy.needs_input_count = 1;
        mirror_busy.attached = true;
        let ws = vec![mock_ws("main", false), mirror, mirror_busy];
        assert!(run_full(ws.clone(), false).is_empty());
        assert!(run_collapsed(ws, false).is_empty());
    }

    #[test]
    fn full_view_switch_overlay_held_renders_keycaps_without_panic() {
        // switch-number overlay 활성(workspace_switch_held=true): 11개 ws — 1~9 는 키캡,
        // 10번째+(index ≥ 9)는 status dot 유지. keycap draw 경로가 패닉 없이 layout 되는지.
        let ws: Vec<_> = (0..11)
            .map(|i| mock_ws(&format!("ws-{i}"), i == 1))
            .collect();
        let actions = run_full(ws, true);
        assert!(actions.is_empty());
    }

    #[test]
    fn collapsed_view_switch_overlay_held_renders_keycaps_without_panic() {
        let ws: Vec<_> = (0..11)
            .map(|i| mock_ws(&format!("ws-{i}"), i == 2))
            .collect();
        let actions = run_collapsed(ws, true);
        assert!(actions.is_empty());
    }

    #[test]
    fn full_view_renders_subtitle_and_description_hierarchy_without_panic() {
        // title + subtitle + description (long, to exercise the 2-line clamp +
        // single-line ellipsis paths) must lay out without panicking.
        let long_desc = "This is a deliberately long workspace description that \
            should wrap onto multiple lines and then be clamped to at most two \
            rows with a trailing ellipsis by the description renderer.";
        let ws = vec![
            WorkspaceEntryView {
                name: "A workspace name long enough to require single-line ellipsis".into(),
                subtitle: "a subtitle label that is also fairly long for ellipsis".into(),
                description: long_desc.into(),
                busy_count: 0,
                // 150 → "99+" cap 경로도 함께 no-panic 검증.
                completion_count: 150,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                is_active: true,
            },
            // title-only and title+subtitle and title+description combinations.
            WorkspaceEntryView {
                name: "title only".into(),
                subtitle: String::new(),
                description: String::new(),
                busy_count: 0,
                completion_count: 0,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                is_active: false,
            },
            WorkspaceEntryView {
                name: "title + description".into(),
                subtitle: String::new(),
                description: "short desc".into(),
                busy_count: 0,
                completion_count: 0,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                is_active: false,
            },
        ];
        let actions = run_full(ws, false);
        assert!(actions.is_empty());
    }

    fn run_full_grouped(
        sections: Vec<CategorySectionView>,
        workspaces: Vec<WorkspaceEntryView>,
        switch_held: bool,
    ) -> Vec<SidebarFullAction> {
        let ctx = egui::Context::default();
        let mut out: Vec<SidebarFullAction> = Vec::new();
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("test_full_grouped").show(ctx, |ui| {
                let props = SidebarFullProps {
                    theme: &theme,
                    kb: &kb,
                    workspaces: &workspaces,
                    categories: Some(&sections),
                    drag: None,
                    tools_label: "Tools",
                    collapse_label: "Collapse",
                    plugins_label: "Plugins",
                    settings_label: "Settings",
                    new_workspace_label: "New Workspace",
                    workspaces_heading: "WORKSPACES",
                    occupied_hover: "Held by another client",
                    mirror_hover: "Mirror of a remote workspace",
                    mirror_pill_label: "REMOTE",
                    plugin_alert: 0,
                    workspace_switch_held: switch_held,
                    category_switch_held: false,
                };
                out = draw_full_sidebar_view(ui, &props).actions;
            });
        }));
        out
    }

    #[test]
    fn full_view_grouped_renders_sections_without_panic() {
        // normal(펼침, 2행) + Services(접힘, 1행) + Archived(빈) — 헤더/행/접힘/빈 경로 전부.
        let workspaces = vec![mock_ws("a", true), mock_ws("b", false), mock_ws("c", false)];
        let sections = vec![
            CategorySectionView {
                id: 0,
                label: "WORKSPACES".into(),
                collapsed: false,
                entries: vec![(0, mock_ws("a", true)), (1, mock_ws("b", false))],
            },
            CategorySectionView {
                id: 1,
                label: "Services".into(),
                collapsed: true,
                entries: vec![(2, mock_ws("c", false))],
            },
            CategorySectionView {
                id: 2,
                label: "Archived".into(),
                collapsed: false,
                entries: vec![],
            },
        ];
        let actions = run_full_grouped(sections, workspaces, false);
        assert!(actions.is_empty(), "expected no actions, got {actions:?}");
    }

    #[test]
    fn grouped_switch_overlay_local_index_paths_do_not_panic() {
        // 카테고리 그룹 + workspace_switch_held. active 워크스페이스(전역 3)가
        // 두 번째 카테고리에 속하고, 그 카테고리 로컬 인덱스는 [0,1] — 첫 카테고리
        // (비활성)는 키캡 미표시(None), 활성 카테고리는 로컬 인덱스 기준 키캡. 두 경로
        // (active/비active 카테고리, 로컬 인덱스 산출) 가 패닉 없이 layout 되는지.
        let workspaces = vec![
            mock_ws("a", false),
            mock_ws("b", false),
            mock_ws("c", false),
            mock_ws("d", true),
        ];
        let sections = vec![
            CategorySectionView {
                id: 0,
                label: "WORKSPACES".into(),
                collapsed: false,
                // 전역 [0,1] — 비활성 카테고리 → 키캡 미표시.
                entries: vec![(0, mock_ws("a", false)), (1, mock_ws("b", false))],
            },
            CategorySectionView {
                id: 1,
                label: "Services".into(),
                collapsed: false,
                // 전역 [2,3] — active(d) 포함 → 로컬 인덱스 0,1 로 키캡.
                entries: vec![(2, mock_ws("c", false)), (3, mock_ws("d", true))],
            },
        ];
        let full = run_full_grouped(sections.clone(), workspaces.clone(), true);
        assert!(full.is_empty(), "expected no actions, got {full:?}");
        let collapsed = run_collapsed_grouped(sections, workspaces, true);
        assert!(
            collapsed.is_empty(),
            "expected no actions, got {collapsed:?}"
        );
    }

    #[test]
    fn collapsed_view_no_input_yields_no_actions() {
        let ws = vec![mock_ws("Default", true), mock_ws("Other", false)];
        let actions = run_collapsed(ws, false);
        assert!(actions.is_empty());
    }

    #[test]
    fn collapsed_view_renders_busy_and_attached_without_panic() {
        let ws = vec![WorkspaceEntryView {
            name: "active".into(),
            subtitle: String::new(),
            description: String::new(),
            busy_count: 3,
            completion_count: 2,
            needs_input_count: 0,
            attached: true,
            is_mirror: false,
            is_active: true,
        }];
        let actions = run_collapsed(ws, false);
        assert!(actions.is_empty());
    }
}
