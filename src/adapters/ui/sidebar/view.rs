//! 앱 상태 없이 사이드바를 그려 사용자 동작을 반환한다. 호출부가 상태를 읽고 결과를 반영한다.

mod card_status;

use crate::adapters::ui::attach_loading as loading;
use crate::adapters::ui::{brand, icons};
use crate::theme::Theme;
use card_status::CardPaint;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::LoadingPlacement;
use tasty_ui_widgets::tokens::{STRUCT_GAP_1, STRUCT_GAP_2, STRUCT_GAP_3};
use tasty_ui_widgets::{
    ControlSize, IconButton, RowParent, RowState, TagVariant, hspace, tag_caps, vspace,
};

/// 드래그 중 표시되는 ghost workspace 이름. DTCG primitive `font-size-12` 는 있으나
/// semantic role 이 없어 `Theme` 필드가 없다 — docs/design/systems/theme.md 대로 **이름에 primitive 임을 남긴다**.
const GHOST_WS_NAME_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 사이드바의 워크스페이스 행 입력.
#[derive(Debug, Clone)]
pub struct WorkspaceEntryView {
    pub loading_since: Option<std::time::Instant>,
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
    /// 다른 클라이언트가 점유 중인지. 점 또는 아바타 둘레의 링으로 표시한다.
    pub attached: bool,
    /// 이 워크스페이스가 원격을 attach 한 client mirror 인지 (하늘색 인디케이터, 항상 켜짐).
    pub is_mirror: bool,
    pub is_active: bool,
    /// 이동 대기 대상이 이 워크스페이스 안에 있고 화면에 보이지 않는지. move 글리프로 표시한다.
    pub move_source: bool,
    /// 자동 attach 매핑을 연결하지 않았으면 행 끝 경고 표지의 툴팁(대상과 이유).
    pub attach_refusal: Option<String>,
}

/// 카테고리별 행과 전역 인덱스. 클릭·드래그 동작이 같은 워크스페이스를 가리키도록 전역 인덱스를 유지한다.
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
    pub notifications: Option<&'a tasty_ui_widgets::NotificationSummary<'a>>,
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
    /// 워크스페이스 전환 modifier가 눌렸는지. 상태 점 대신 설정된 슬롯 키캡을 표시한다.
    pub workspace_switch_held: bool,
    /// 카테고리 전환 modifier가 눌렸는지. 헤더에 해당 슬롯 키캡을 표시한다.
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
    pub notifications: Option<&'a tasty_ui_widgets::NotificationSummary<'a>>,
    pub tools_hover: &'a str,
    /// "확인 필요" plugin 개수. >0 이면 Plugins 레일 버튼에 danger 배지.
    pub plugin_alert: usize,
    /// 워크스페이스 전환 modifier가 눌렸는지. 문자 아이콘 대신 슬롯 키캡을 표시한다.
    pub workspace_switch_held: bool,
    /// 카테고리 전환 modifier가 눌렸는지. 카테고리 버튼 자리에 설정된 슬롯 키캡을 표시한다.
    pub category_switch_held: bool,
}

/// Full sidebar view 가 보고하는 사용자 의도. wrapper 가 state mutation 으로 변환.
#[derive(Debug, Clone)]
pub enum SidebarFullAction {
    Collapse,
    Plugins,
    Settings,
    ToolsClicked(egui::Rect),
    NotificationsClicked(egui::Rect),
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
    /// 마우스 떼짐 — drop_target은 이동 후 전역 인덱스이며 자기 앞뒤 슬롯은 None.
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
    NotificationsClicked(egui::Rect),
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
    /// 실제 버튼·행 위에서는 창 가장자리 리사이즈보다 위젯 조작을 우선한다.
    /// 빈 배경의 우클릭 영역은 제외해 리사이즈가 가능하게 한다.
    pub resize_priority_hovered: bool,
}

/// [`draw_collapsed_sidebar_view`] 의 반환값 — [`SidebarFullDrawResult`] 와 동형.
pub struct SidebarCollapsedDrawResult {
    pub actions: Vec<SidebarCollapsedAction>,
    /// [`SidebarFullDrawResult::resize_priority_hovered`] 참고 — collapsed 레일의
    /// 펼치기 버튼/Tools/Plugins/Settings/카테고리 `---` 버튼/워크스페이스 아바타 위인지.
    pub resize_priority_hovered: bool,
}

// 배율이 필요한 치수는 Theme에서 읽는다.
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

/// 연속된 카테고리별 드롭 영역. 앞 섹션의 end_y부터 시작하므로 헤더 위 간격도 포함된다.
struct SectionSpan {
    id: crate::model::WorkspaceCategoryId,
    /// 섹션이 끝나는 y (다음 섹션 시작 = 이 값).
    end_y: f32,
    /// 카테고리 헤더 rect — 행 rect 가 없는(빈/접힌) 섹션의 marker x·폭·y anchor.
    header_rect: egui::Rect,
    /// 이 섹션에 실제로 그린 카드의 범위. 빈/접힌 섹션은 빈 범위다.
    card_range: std::ops::Range<usize>,
}

/// 드롭 결과와 표시선을 같은 규칙으로 정한다. 위·아래 바깥은 첫·마지막 섹션에 속한다.
/// 평면 모드는 spans 가 비어 None.
fn resolve_drop_section(spans: &[SectionSpan], y: f32) -> Option<&SectionSpan> {
    spans.iter().find(|s| y < s.end_y).or_else(|| spans.last())
}

/// 가이드와 드롭이 공유하는, 이동 전 카드 사이의 전역 삽입 슬롯.
struct WorkspaceDrop {
    slot: Option<usize>,
    preceding: Option<usize>,
    following: Option<usize>,
    category: Option<crate::model::WorkspaceCategoryId>,
    marker: Option<egui::Rect>,
}

impl WorkspaceDrop {
    fn reorder_target(&self, from: usize) -> Option<usize> {
        let slot = self.slot?;
        if self.preceding == Some(from) || self.following == Some(from) {
            return None;
        }
        // remove(from)이 아래쪽 슬롯을 한 칸 당긴다. 자기 앞뒤 슬롯은 같은 결과다.
        let target = slot - usize::from(from < slot);
        (target != from).then_some(target)
    }
}

fn resolve_workspace_drop(
    card_rects: &[(usize, egui::Rect)],
    spans: &[SectionSpan],
    y: f32,
) -> WorkspaceDrop {
    let section = resolve_drop_section(spans, y);
    let cards = section.map_or(card_rects, |s| &card_rects[s.card_range.clone()]);
    let local_slot = cards
        .iter()
        .position(|(_, rect)| y < rect.center().y)
        .unwrap_or(cards.len());
    let next = cards.get(local_slot);
    let slot = next
        .map(|(index, _)| *index)
        .or_else(|| cards.last().map(|(index, _)| index + 1));
    let marker = if let Some((_, rect)) = next.or_else(|| cards.last()) {
        let marker_y = if next.is_some() {
            rect.min.y - 1.0
        } else {
            rect.max.y + 1.0
        };
        Some(egui::Rect::from_min_size(
            egui::pos2(rect.min.x, marker_y),
            egui::vec2(rect.width(), 2.0),
        ))
    } else {
        section.map(|s| {
            egui::Rect::from_min_size(
                egui::pos2(s.header_rect.min.x, s.header_rect.max.y + 1.0),
                egui::vec2(s.header_rect.width(), 2.0),
            )
        })
    };
    WorkspaceDrop {
        slot,
        preceding: local_slot.checked_sub(1).map(|i| cards[i].0),
        following: next.map(|(index, _)| *index),
        category: section.map(|s| s.id),
        marker,
    }
}

/// 최초 프레임은 제외하고 활성 인덱스가 바뀔 때만 자동 스크롤해 사용자 스크롤을 유지한다.
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

    egui::TopBottomPanel::top("workspace_sidebar_header")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            vspace(ui, th.spacing_md);
            let (collapsed, hovered) = draw_sidebar_header(ui, th, props.collapse_label);
            resize_priority_hovered |= hovered;
            if collapsed {
                actions.push(SidebarFullAction::Collapse);
            }
            vspace(ui, th.spacing_xs);
        });

    egui::TopBottomPanel::bottom("workspace_sidebar_bottom")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(summary) = props.notifications {
                let response = tasty_ui_widgets::notification_summary(ui, th, summary, false);
                resize_priority_hovered |= response.hovered();
                if response.clicked() {
                    actions.push(SidebarFullAction::NotificationsClicked(response.rect));
                }
            }
            ui.separator();
            vspace(ui, STRUCT_GAP_2);

            let tools_resp = draw_ghost_block_button(ui, th, Some(icons::TOOLS), props.tools_label);
            resize_priority_hovered |= tools_resp.hovered();
            if tools_resp.clicked() {
                actions.push(SidebarFullAction::ToolsClicked(tools_resp.rect));
            }
            vspace(ui, STRUCT_GAP_2);

            let plug_resp = draw_ghost_block_button(ui, th, Some(icons::PLUG), props.plugins_label);
            resize_priority_hovered |= plug_resp.hovered();
            if plug_resp.clicked() {
                actions.push(SidebarFullAction::Plugins);
            }
            if props.plugin_alert > 0 {
                paint_alert_badge(ui, th, plug_resp.rect, props.plugin_alert, true);
            }
            vspace(ui, STRUCT_GAP_2);

            let settings_resp =
                draw_ghost_block_button(ui, th, Some(icons::SETTINGS), props.settings_label);
            resize_priority_hovered |= settings_resp.hovered();
            if settings_resp.clicked() {
                actions.push(SidebarFullAction::Settings);
            }
            vspace(ui, th.spacing_sm);
        });

    // 전체 워크스페이스의 활성 인덱스를 기록해 실제로 바뀐 프레임에만 스크롤한다.
    let active_idx = props.workspaces.iter().position(|w| w.is_active);
    let active_scroll_track_id = egui::Id::new("sidebar_workspace_active_scroll_track");
    let prev_active_idx: Option<Option<usize>> = ui.data(|d| d.get_temp(active_scroll_track_id));
    let should_scroll_to_active = should_scroll_to_active_workspace(prev_active_idx, active_idx);
    ui.data_mut(|d| d.insert_temp(active_scroll_track_id, active_idx));

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            // 행 배경·구분선 사이가 벌어지지 않도록 행 간격은 없애고 섹션 간격만 별도로 준다.
            ui.spacing_mut().item_spacing.y = 0.0;
            vspace(ui, th.spacing_sm);
            let mut card_rects: Vec<(usize, egui::Rect)> = Vec::new();
            let mut section_spans: Vec<SectionSpan> = Vec::new();

            if let Some(sections) = props.categories {
                for (sec_i, section) in sections.iter().enumerate() {
                    let first_card = card_rects.len();
                    // 헤더 앞의 간격도 이 섹션 드롭 영역에 포함한다.
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
                    // 카테고리 키캡은 오른쪽에 표시하고 접힘 상태 chevron은 유지한다.
                    if props.category_switch_held
                        && let Some(digit) =
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
                        // 헤더가 아래쪽 경계선을 그리므로 첫 행에 중복 선을 두지 않는다.
                        // 활성 카테고리에서만 로컬 인덱스로 키캡을 표시해 실제 전환 순서와 맞춘다.
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
                    }
                    section_spans.push(SectionSpan {
                        id: section.id,
                        end_y: ui.cursor().min.y,
                        header_rect: header.rect,
                        card_range: first_card..card_rects.len(),
                    });
                }
            } else {
                draw_section_heading(ui, th, props.workspaces_heading);
                vspace(ui, th.spacing_xs);

                if !props.workspaces.is_empty() {
                    draw_list_separator(ui, th, 0.0);
                }

                for (i, ws) in props.workspaces.iter().enumerate() {
                    if i > 0 {
                        draw_list_separator(ui, th, 32.0);
                    }
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

                if !props.workspaces.is_empty() {
                    draw_list_separator(ui, th, 0.0);
                }
            }

            if let Some(drag) = props.drag {
                let drop = resolve_workspace_drop(&card_rects, &section_spans, drag.current_y);
                let released = !ui.input(|i| i.pointer.primary_down());
                if released {
                    actions.push(SidebarFullAction::DragReleased {
                        drop_target: drop.reorder_target(drag.ws_idx),
                        target_category: drop.category,
                    });
                } else {
                    if let Some(line) = drop.marker {
                        ui.painter().rect_filled(line, 0.0, th.accent_primary());
                    }

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
            // 카테고리를 사용하면 추가 버튼 대신 카테고리 메뉴에서 워크스페이스를 만든다.
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
                        egui::Stroke::new(th.focus_ring_width.value(), th.accent_success_raised()),
                        egui::StrokeKind::Inside,
                    );
                }
                vspace(ui, th.spacing_xs);
            }

            // 목록 아래 빈 배경은 새 카테고리·원격 추가 메뉴를 제공한다.
            // 실제 콘텐츠가 아니므로 리사이즈 우선권을 가로채지 않는다.
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

    egui::TopBottomPanel::top("workspace_sidebar_collapsed_header")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            vspace(ui, th.spacing_sm);
            ui.vertical_centered(|ui| {
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
            vspace(ui, th.spacing_sm);
        });

    egui::TopBottomPanel::bottom("workspace_sidebar_collapsed_bottom")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(summary) = props.notifications {
                    let response = tasty_ui_widgets::notification_summary(ui, th, summary, true);
                    resize_priority_hovered |= response.hovered();
                    if response.clicked() {
                        actions.push(SidebarCollapsedAction::NotificationsClicked(response.rect));
                    }
                    vspace(ui, th.spacing_xs);
                }
                ui.separator();
                vspace(ui, STRUCT_GAP_2);

                let (tools_btn_rect, tools_resp) =
                    ui.allocate_exact_size(collapsed_icon_size(th), egui::Sense::click());
                resize_priority_hovered |= tools_resp.hovered();
                paint_icon_button(ui, th, tools_btn_rect, &tools_resp, icons::TOOLS);
                let tools_resp = tools_resp.on_hover_text(props.tools_hover);
                if tools_resp.clicked() {
                    actions.push(SidebarCollapsedAction::ToolsClicked(tools_btn_rect));
                }
                vspace(ui, STRUCT_GAP_2);

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
            // 접힌·빈 카테고리도 버튼을 표시하며 해당 modifier를 누르면 슬롯 키캡으로 바꾼다.
            for (sec_i, section) in sections.iter().enumerate() {
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
            // 카테고리가 없으면 첫 아바타 위에 카테고리 선과 같은 색·굵기의 구분선을 둔다.
            let (divider, _) = ui.allocate_exact_size(
                tasty_ui_widgets::rail_flat_divider_size(th),
                egui::Sense::hover(),
            );
            tasty_ui_widgets::paint_rail_flat_divider(ui.painter(), th, divider);
            vspace(ui, th.spacing_xs);
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
                    egui::Stroke::new(th.focus_ring_width.value(), th.accent_success_raised()),
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
        hspace(ui, th.spacing_md);
        brand::draw_wordmark(
            ui,
            th,
            th.sidebar_logo_size,
            th.sidebar_wordmark_font_size,
            th.sidebar_wordmark_tracking(th.sidebar_wordmark_font_size),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            hspace(ui, th.spacing_md);
            // 시안 사이드바 머리의 접기 버튼은 IconButton sm(chevrons)이다.
            let resp = IconButton::new()
                .size(ControlSize::Sm)
                .show(ui, th, &|ui, rect, c| {
                    icons::CHEVRONS_LEFT
                        .image(rect.width(), c)
                        .paint_at(ui, rect);
                });
            hovered = resp.hovered();
            resp.clone().on_hover_text(collapse_hover);
            collapse = resp.clicked();
        });
    });
    (collapse, hovered)
}

/// ui_kit 섹션 헤딩 — font-label 대문자, muted, 좌측 패딩. 자간은 `sidebar_section_heading_tracking`(caps-wide 0.07em).
fn draw_section_heading(ui: &mut egui::Ui, th: &Theme, text: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::hover());
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: tasty_ui_widgets::label_font(th.sidebar_section_heading_font_size.value()),
            extra_letter_spacing: th
                .sidebar_section_heading_tracking(th.sidebar_section_heading_font_size)
                .value(),
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

/// 카테고리 이름·접힘 상태·개수. 클릭은 접기·펼치기, 우클릭은 메뉴를 연다.
/// 현재는 추가 버튼이 없어 개수를 항상 표시한다.
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
    let border = th.sidebar_category_header_border().to_egui_premultiplied();
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
    let text_x = chevron_rect.max.x + gap;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: tasty_ui_widgets::label_font(th.sidebar_section_heading_font_size.value()),
            extra_letter_spacing: th
                .sidebar_section_heading_tracking(th.sidebar_section_heading_font_size)
                .value(),
            color: fg.into(),
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let pos = egui::pos2(text_x, row_center_y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, fg.into());
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

/// 워크스페이스 행과 드래그 영역을 그린다. 액션 대상은 전역 인덱스다.
#[allow(clippy::too_many_arguments)] // reason: 표시 입력은 이미 props 로 묶었고 남은 셋은 호출부가 루프 밖에서 따로 쓰는 &mut 출력이다
fn draw_ws_row(
    ui: &mut egui::Ui,
    props: &SidebarFullProps<'_>,
    global_idx: usize,
    ws: &WorkspaceEntryView,
    // 호출부가 활성 카테고리의 로컬 슬롯 키를 결정한다. 없으면 상태 점을 유지한다.
    switch_digit: Option<&str>,
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
    // 행 채움은 내용보다 먼저 칠해야 하므로 카드 응답의 hover를 그리기 전에 읽는다. 카드를 처음 그리는
    // 프레임에는 응답이 없어 hover 가 아니다.
    let card_id = egui::Id::new(("ws_card", global_idx));
    let hovered = ui.ctx().read_response(card_id).is_some_and(|r| r.hovered());
    let card = draw_workspace_card(
        ui,
        th,
        ws,
        hovered,
        props.mirror_pill_label,
        switch_digit,
        fade,
    );
    let card_rect = card.rect;
    if ws.is_active && should_scroll_to_active {
        // align=None → 이미 뷰포트 안이면 스크롤 무변화, 밖이면 최소 이동으로만 보정.
        ui.scroll_to_rect(card_rect, None);
    }
    let card_response = ui.interact(card_rect, card_id, egui::Sense::click_and_drag());
    *resize_priority_hovered |= card_response.hovered();
    card_status::show_card_tooltips(
        ui,
        th,
        ws,
        &card,
        &card_response,
        props.occupied_hover,
        props.mirror_hover,
    );

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
            egui::Stroke::new(th.focus_ring_width.value(), th.accent_success_raised()),
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

/// 카테고리 레일 버튼. 클릭한 영역을 팝업 위치 계산에 넘긴다.
/// keycap이 있으면 선 대신 슬롯 키캡을 표시한다.
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
    // 시안 RailCategoryBtn은 버튼 바탕에 radius-sm을 쓴다(아바타는 radius).
    let radius = th.corner_radius_sm.value();
    let pressed = resp.is_pointer_button_down_on();
    if pressed {
        ui.painter()
            .rect_filled(rect, radius, th.active_overlay.to_egui_premultiplied());
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.hover_overlay.to_egui_premultiplied());
    }
    match keycap {
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
            tasty_ui_widgets::paint_rail_category_line(
                ui.painter(),
                th,
                rect.center(),
                pressed || resp.hovered(),
            );
        }
    }
    resp.clicked().then_some(rect)
}

/// 워크스페이스 행의 move 글리프. 이름 뒤, 배지 묶음 앞에 놓인다.
fn paint_move_source_row_glyph(ui: &mut egui::Ui, th: &Theme) {
    let size = tasty_ui_widgets::move_source_glyph_size(th);
    let (slot, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    tasty_ui_widgets::paint_move_source_glyph(ui, th, slot);
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
    // 빈 이름은 생략하고 첫 문자를 대문자로 표시한다.
    let label = ws
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let (rect, resp) = ui.allocate_exact_size(collapsed_ws_size(th), egui::Sense::click());
    // 선택 아바타는 mantle+press 채움이라 hue 글자는 raised 띠를 읽는다.
    let text_color: egui::Color32 = if ws.is_active {
        th.accent_primary_raised().into()
    } else if resp.hovered() {
        th.text_muted_raised().into()
    } else {
        th.text_muted().into()
    };
    *resize_priority_hovered |= resp.hovered();
    let radius = th.corner_radius.value();
    if ws.is_active {
        ui.painter()
            .rect_filled(rect, radius, th.overlay_active().to_egui_premultiplied());
    }
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.hover_overlay.to_egui_premultiplied());
    }
    // 키캡으로 바꿔도 모서리 상태 점은 유지한다.
    let fade = crate::adapters::ui::switch_overlay::appear_fade(
        ui.ctx(),
        th,
        ("ws_collapsed", global_idx),
        props.workspace_switch_held,
    );
    if switch_digit.is_none() && !label.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &label,
            egui::FontId::monospace(th.font_size_body.value()),
            text_color,
        );
    }
    let loading = loading::paint_sidebar_loading(
        ui,
        th,
        ws.loading_since,
        rect,
        LoadingPlacement::Rail,
        egui::Color32::TRANSPARENT,
    );
    // 전환 키캡은 이니셜 둘레 스피너 위에 그린다.
    if let Some(digit) = switch_digit {
        crate::adapters::ui::switch_overlay::paint_keycap(
            ui.painter(),
            th,
            rect.center(),
            digit,
            ws.is_active,
            fade,
        );
    }
    // 사이드바 로딩은 attach 응답 대기에서만 켜진다.
    let loading_line = loading.then(|| crate::i18n::t("loading.remote_attach"));
    // 상태는 오른쪽 위 점, mirror는 오른쪽 아래 표시, 다른 클라이언트 점유는 둘레 링이다.
    // 왼쪽 아래는 move 칩, 왼쪽 위는 attach 거절 칩이다.
    // 점 하나로 NeedsInput > Completion > running 순서의 상태를 표시한다.
    if let Some(dot) = tasty_ui_widgets::RailDot::resolve(
        ws.needs_input_count > 0,
        ws.completion_count > 0,
        ws.busy_count > 0,
    ) {
        tasty_ui_widgets::paint_rail_dot(ui.painter(), th, rect, dot);
    }
    if ws.attached {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                th.status_dot_attached_ring_width().value(),
                th.accent_attached_raised(),
            ),
            egui::StrokeKind::Inside,
        );
    }
    // 이동 대기 칩은 알림 점·mirror 칩과 겹치지 않도록 왼쪽 아래에 둔다.
    if ws.move_source {
        tasty_ui_widgets::paint_move_source_chip(ui, th, rect, th.bg_sidebar().into());
    }
    // 자동 attach 거절 칩은 남은 왼쪽 위 모서리에 둔다. 툴팁은 펼친 행 표지와 같다.
    if let Some(tooltip) = &ws.attach_refusal {
        tasty_ui_widgets::paint_attach_refusal_chip(ui, th, rect, th.bg_sidebar().into());
        tasty_ui_widgets::attach_refusal_avatar_tooltip(ui, th, &resp, tooltip, loading_line);
    } else if let Some(line) = loading_line {
        // 디자인 rail 아바타 툴팁은 워크스페이스 이름이다. 로딩 중에는 그 아래 둘째 줄을 더한다.
        if tasty_ui_widgets::tooltip_hover_delay_elapsed(ui.ctx(), th, resp.id, resp.hovered()) {
            tasty_ui_widgets::Tooltip::new(&ws.name)
                .second_line(line)
                .id_source(resp.id)
                .show(ui, th, resp.rect);
        }
    }
    if ws.is_mirror {
        tasty_ui_widgets::paint_workspace_mirror_chip(ui, th, rect);
    }
    if resp.clicked() {
        actions.push(SidebarCollapsedAction::WorkspaceClicked(global_idx));
    }
}

/// Full 사이드바의 workspace card 1 장 — Frame::show 로 직접 그리고 점유한 rect 반환.
#[allow(clippy::too_many_arguments)] // reason: props 뭉치 대신 말단 표시값만 받아 카드 그리기를 사이드바 상태에서 떼어 둔다 — 묶으면 그 분리가 사라진다
fn draw_workspace_card(
    ui: &mut egui::Ui,
    th: &Theme,
    ws: &WorkspaceEntryView,
    // 카드 응답의 hover. 비활성 카드의 행 채움(받침)을 정한다.
    hovered: bool,
    mirror_pill_label: &str,
    // Some(digit) 면 status dot 자리에 숫자 키캡(switch-number overlay)을 그린다.
    switch_digit: Option<&str>,
    // switch-number overlay 등장 페이드 계수(0..=1, motion-ui-fast 90ms).
    switch_fade: f32,
) -> CardPaint {
    // hover 채움은 받침에 들어가고 점·스피너·키캡·글자는 그 위에 그린다.
    let pressed = hovered && ui.input(|i| i.pointer.primary_down());
    let state = RowState::of(true, ws.is_active, hovered, pressed);
    let bg = state.fill(th);

    // 행 배경은 모서리와 바깥 여백 없이 사이드바 폭을 채운다.
    // 제목·REMOTE 줄·부제·설명은 모두 점 슬롯, 숫자 칸과 간격 뒤에서 시작한다.
    let body_indent = tasty_ui_widgets::workspace_row_body_offset(ui, th, ws.busy_count);
    let frame = egui::Frame::new().fill(bg).inner_margin(egui::Margin {
        left: th.workspace_row_padding_x().value() as i8,
        right: card_inner_margin_x(th),
        top: card_inner_margin_y(th),
        bottom: card_inner_margin_y(th),
    });

    let mut dot_slot = card_status::DotSlot::default();
    let mut remote_line = None;
    let response = frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            dot_slot = card_status::paint_dot_slot(ui, th, ws, state, switch_digit, switch_fade);
            let name_color = state.label(th, RowParent::Mantle, th.text_secondary());

            // 배지 폭을 먼저 확보하고 남은 폭에 이름을 줄여 표시한다.
            // 오른쪽부터 그리므로 Completion 뒤에 NeedsInput을 넣어 왼쪽에 배치한다.
            // 이름 뒤 순서는 move 글리프 · 거절 표지 · 배지 묶음이고 배지가 가장 오른쪽이다.
            // 끝 칸 항목 사이는 행 간격(space-sm)이다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let item_gap = (th.spacing_sm.value() - ui.spacing().item_spacing.x).max(0.0);
                let mut placed = tasty_ui_widgets::workspace_attention_badges(
                    ui,
                    th,
                    ws.needs_input_count,
                    ws.completion_count,
                );
                if let Some(tooltip) = &ws.attach_refusal {
                    if placed {
                        ui.add_space(item_gap);
                    }
                    tasty_ui_widgets::attach_refusal_mark(ui, th, tooltip);
                    placed = true;
                }
                if ws.move_source {
                    if placed {
                        ui.add_space(item_gap);
                    }
                    paint_move_source_row_glyph(ui, th);
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

        // mirror 표시는 부제 유무와 무관하게 이름 아래 별도 줄에 둔다.
        if ws.is_mirror {
            vspace(ui, STRUCT_GAP_1);
            let resp = ui.horizontal(|ui| {
                ui.add_space(body_indent);
                ui.spacing_mut().item_spacing.x = th.workspace_mirror_gap().value();
                let mirror_fg = match state {
                    RowState::Selected => th.workspace_mirror_fg_active(),
                    _ => state.hue(
                        th,
                        th.workspace_mirror_fg(),
                        th.accent_remote_raised(),
                        th.accent_remote_on_active(),
                    ),
                };
                ui.add(
                    icons::TERMINAL_PROMPT
                        .image(th.workspace_mirror_icon_size().value(), mirror_fg.into()),
                );
                tag_caps(ui, th, mirror_pill_label, TagVariant::Info, false);
            });
            remote_line = Some(resp.response.rect);
        }

        if !ws.subtitle.is_empty() {
            vspace(ui, STRUCT_GAP_1);
            ui.horizontal(|ui| {
                ui.add_space(body_indent);
                // 부제는 이름과 정렬하고 일반 UI 글꼴을 사용한다.
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&ws.subtitle)
                            .size(th.sidebar_button_label_font_size.value())
                            .color(state.meta(th, RowParent::Mantle)),
                    )
                    .truncate(),
                );
            });
        }

        if !ws.description.is_empty() {
            vspace(ui, STRUCT_GAP_3);
            ui.horizontal(|ui| {
                ui.add_space(body_indent);
                // 설명은 이름과 정렬하고 최대 두 줄로 줄인다.
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
                        color: if ws.is_active {
                            th.text_on_active()
                        } else {
                            th.text_placeholder()
                        }
                        .into(),
                        line_height: Some(size * th.line_height_ui),
                        ..Default::default()
                    },
                );
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.label(galley);
            });
        }
    });

    let card_rect = response.response.rect;

    // 활성 표시선은 행 안쪽에 그려 상태 점과 겹치지 않게 한다.
    if ws.is_active {
        let bar = egui::Rect::from_min_size(
            card_rect.min,
            egui::vec2(
                th.workspace_row_active_bar_width().value(),
                card_rect.height(),
            ),
        );
        ui.painter().rect_filled(bar, 0.0, th.accent_primary());
    }

    CardPaint {
        rect: card_rect,
        loading: dot_slot.loading,
        occupied_dot: dot_slot.occupied_dot,
        running_count: dot_slot.running_count,
        remote_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn mock_ws(name: &str, is_active: bool) -> WorkspaceEntryView {
        WorkspaceEntryView {
            loading_since: None,
            name: name.to_string(),
            subtitle: String::new(),
            description: String::new(),
            busy_count: 0,
            completion_count: 0,
            needs_input_count: 0,
            attached: false,
            is_mirror: false,
            move_source: false,
            attach_refusal: None,
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
            card_range: 0..usize::from(has_visible_rows),
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
        assert_eq!(resolve_drop_section(&spans, 50.0).map(|s| s.id), Some(0));
        assert_eq!(resolve_drop_section(&spans, 120.0).map(|s| s.id), Some(1));
        assert_eq!(resolve_drop_section(&spans, 160.0).map(|s| s.id), Some(2));
        assert_eq!(resolve_drop_section(&spans, -10.0).map(|s| s.id), Some(0));
        assert_eq!(resolve_drop_section(&spans, 999.0).map(|s| s.id), Some(2));
        // 경계값: 섹션 간 gap 은 end_y 연속으로 다음 섹션에 귀속 (y == 이전 end_y).
        assert_eq!(resolve_drop_section(&spans, 100.0).map(|s| s.id), Some(1));
        assert!(
            !resolve_drop_section(&spans, 50.0)
                .unwrap()
                .card_range
                .is_empty()
        );
        assert!(
            resolve_drop_section(&spans, 120.0)
                .unwrap()
                .card_range
                .is_empty()
        );
    }

    #[test]
    fn resolve_drop_section_empty_spans_yields_none() {
        assert!(resolve_drop_section(&[], 42.0).is_none());
    }

    fn mock_cards(indices: &[usize]) -> Vec<(usize, egui::Rect)> {
        indices
            .iter()
            .enumerate()
            .map(|(row, &index)| {
                (
                    index,
                    egui::Rect::from_min_size(
                        egui::pos2(0.0, row as f32 * 20.0),
                        egui::vec2(180.0, 20.0),
                    ),
                )
            })
            .collect()
    }

    #[test]
    fn workspace_drop_downward_matches_guide_slot() {
        let cards = mock_cards(&[0, 1, 2, 3]);
        let drop = resolve_workspace_drop(&cards, &[], 60.0);
        assert_eq!(drop.slot, Some(3));
        assert_eq!(drop.marker.unwrap().top(), cards[3].1.top() - 1.0);
        assert_eq!(drop.reorder_target(0), Some(2));
    }

    #[test]
    fn workspace_drop_own_adjacent_slots_are_noop() {
        let cards = mock_cards(&[0, 1, 2, 3]);
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 20.0).reorder_target(0),
            None
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 5.0).reorder_target(0),
            None
        );
    }

    #[test]
    fn workspace_drop_upward_first_and_last_controls() {
        let cards = mock_cards(&[0, 1, 2, 3]);
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 20.0).reorder_target(3),
            Some(1)
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 999.0).reorder_target(0),
            Some(3)
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], -5.0).reorder_target(2),
            Some(0)
        );
    }

    #[test]
    fn workspace_drop_every_guide_slot_matches_resulting_neighbors() {
        for count in 1..=8 {
            let ids: Vec<_> = (0..count).collect();
            let cards = mock_cards(&ids);
            for y in -5..=(count as i32 * 20 + 5) {
                let drop = resolve_workspace_drop(&cards, &[], y as f32);
                let slot = drop.slot.unwrap();
                let marker_y = if slot < count {
                    cards[slot].1.top() - 1.0
                } else {
                    cards[count - 1].1.bottom() + 1.0
                };
                assert_eq!(drop.marker.unwrap().top(), marker_y);
                for from in 0..count {
                    let mut actual = ids.clone();
                    if let Some(to) = drop.reorder_target(from) {
                        actual.remove(from);
                        actual.insert(to, from);
                    }
                    // Build the expected list by inserting at the guide's original gap,
                    // independently of the remove/insert final-index calculation.
                    let mut expected = Vec::new();
                    for gap in 0..=count {
                        if gap == slot {
                            expected.push(from);
                        }
                        if gap < count && gap != from {
                            expected.push(gap);
                        }
                    }
                    assert_eq!(actual, expected, "count={count} y={y} from={from}");
                }
            }
        }
    }

    #[test]
    fn workspace_drop_category_end_uses_only_its_section_cards() {
        let cards = mock_cards(&[0, 1, 2, 3]);
        let mut normal = mock_span(0, 50.0, true);
        normal.card_range = 0..2;
        let mut other = mock_span(1, 100.0, true);
        other.card_range = 2..4;
        let spans = [normal, other];
        let drop = resolve_workspace_drop(&cards, &spans, 45.0);
        assert_eq!(drop.category, Some(0));
        assert_eq!(drop.slot, Some(2));
        assert_eq!(drop.marker.unwrap().top(), cards[1].1.bottom() + 1.0);
        assert_eq!(drop.reorder_target(0), Some(1));
        assert!(drop.marker.unwrap().bottom() < spans[0].end_y);
    }

    #[test]
    fn workspace_drop_interleaved_global_indices_keep_local_adjacent_slots_noop() {
        let cards = mock_cards(&[0, 2, 4]);
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 20.0).reorder_target(0),
            None
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 40.0).reorder_target(2),
            None
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 60.0).reorder_target(0),
            Some(4)
        );
        assert_eq!(
            resolve_workspace_drop(&cards, &[], 0.0).reorder_target(4),
            Some(0)
        );
    }

    #[test]
    fn workspace_drop_empty_or_collapsed_section_anchors_to_its_header() {
        let cards = mock_cards(&[0, 1]);
        let mut visible = mock_span(0, 50.0, true);
        visible.card_range = 0..2;
        let mut empty = mock_span(1, 85.0, false);
        empty.card_range = 2..2;
        let spans = [visible, empty];
        let drop = resolve_workspace_drop(&cards, &spans, 60.0);
        assert_eq!(drop.category, Some(1));
        assert_eq!(drop.reorder_target(0), None);
        assert_eq!(
            drop.marker.unwrap().top(),
            spans[1].header_rect.bottom() + 1.0
        );
        let flat_empty = resolve_workspace_drop(&[], &[], 0.0);
        assert_eq!(flat_empty.reorder_target(0), None);
        assert!(flat_empty.marker.is_none());
    }

    #[test]
    fn should_scroll_to_active_workspace_skips_first_frame() {
        assert!(!should_scroll_to_active_workspace(None, Some(3)));
        assert!(!should_scroll_to_active_workspace(None, None));
    }

    #[test]
    fn should_scroll_to_active_workspace_skips_when_unchanged() {
        assert!(!should_scroll_to_active_workspace(Some(Some(3)), Some(3)));
        assert!(!should_scroll_to_active_workspace(Some(None), None));
    }

    #[test]
    fn should_scroll_to_active_workspace_triggers_on_change() {
        assert!(should_scroll_to_active_workspace(Some(Some(3)), Some(7)));
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
                    notifications: None,
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

    /// 로딩 중인 워크스페이스 "build-farm"을 둘째 행으로 그린 출력. 모션 감소로 키캡 페이드를 건너뛴다.
    fn loading_sidebar(
        collapsed: bool,
        switch_held: bool,
        active: bool,
    ) -> (egui::FullOutput, Theme) {
        let mut ws = mock_ws("build-farm", active);
        ws.loading_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        let workspaces = vec![mock_ws("main", !active), ws];
        let theme = loading_theme();
        let output = loading_sidebar_frame(
            &egui::Context::default(),
            &theme,
            &workspaces,
            collapsed,
            switch_held,
            egui::RawInput::default(),
        );
        (output, theme)
    }

    fn loading_theme() -> Theme {
        let mut theme = test_theme();
        theme.reduced_motion = true;
        theme
    }

    /// 같은 context 로 한 프레임을 그린다. 포인터를 실은 입력을 여러 번 넣어 hover 를 재현한다.
    fn loading_sidebar_frame(
        ctx: &egui::Context,
        theme: &Theme,
        workspaces: &[WorkspaceEntryView],
        collapsed: bool,
        switch_held: bool,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        let kb = crate::settings::KeybindingSettings::default();
        ctx.run(input, |ctx| {
            egui::SidePanel::left("test_loading_keycap").show(ctx, |ui| {
                if collapsed {
                    let props = SidebarCollapsedProps {
                        theme,
                        kb: &kb,
                        workspaces,
                        categories: None,
                        notifications: None,
                        tools_hover: "Tools menu",
                        plugin_alert: 0,
                        workspace_switch_held: switch_held,
                        category_switch_held: false,
                    };
                    let _view = draw_collapsed_sidebar_view(ui, &props);
                } else {
                    let props = SidebarFullProps {
                        theme,
                        kb: &kb,
                        workspaces,
                        categories: None,
                        drag: None,
                        notifications: None,
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
                    let _view = draw_full_sidebar_view(ui, &props);
                }
            });
        })
    }

    /// 시안 우선순위: 전환 키캡은 같은 칸의 스피너 위에 그린다(카드 점 칸·레일 아바타).
    #[test]
    fn switch_keycap_paints_above_the_loading_spinner() {
        for collapsed in [false, true] {
            let (output, theme) = loading_sidebar(collapsed, true, false);
            let (spinner, keycap) =
                crate::adapters::ui::attach_loading::paint_order::spinner_and_keycap(
                    &output, &theme,
                );
            let (spinner, keycap) = (spinner.expect("spinner"), keycap.expect("keycap"));
            assert!(
                keycap > spinner,
                "collapsed={collapsed} {keycap} <= {spinner}"
            );
        }
    }

    /// 시안 LNB 받침은 bg-sidebar 위에 현재 행 채움을 겹친 색이다. 활성 카드의 점 칸은 사이드바 배경을
    /// 깐 뒤 같은 칸을 surface-active로 다시 칠한다. 비활성 카드는 투명 채움이라 사이드바 배경만 남는다.
    #[test]
    fn lnb_loading_bed_layers_the_row_fill_over_the_sidebar_background() {
        let slot = test_theme().workspace_dot_slot().value();
        for active in [true, false] {
            let (output, theme) = loading_sidebar(false, false, active);
            let shapes = crate::adapters::ui::attach_loading::paint_order::flatten(&output);
            let beds: Vec<egui::Color32> = shapes
                .iter()
                .filter(|(_, r)| r.is_some_and(|r| (r.width() - slot).abs() < 0.01))
                .map(|(c, _)| *c)
                .collect();
            let sidebar = theme.bg_sidebar().to_egui();
            let row = theme.surface_active().to_egui();
            let at = beds
                .iter()
                .position(|c| *c == sidebar)
                .expect("sidebar bed");
            let next = beds.get(at + 1).copied();
            if active {
                assert_eq!(next, Some(row), "{beds:?}");
            } else {
                assert_eq!(next, Some(egui::Color32::TRANSPARENT), "{beds:?}");
            }
        }
    }

    /// 비활성 로딩 카드를 hover 하는 프레임. 첫 프레임에서 점 칸 받침 위치를 찾고, 그 가운데에 포인터를
    /// 둔 두 프레임을 더 그린다(egui 는 직전 프레임의 위젯 위치로 hover 를 정한다).
    fn hovered_loading_card(attached: bool) -> (egui::Context, egui::FullOutput, Theme) {
        let ctx = egui::Context::default();
        let theme = loading_theme();
        let mut ws = mock_ws("build-farm", false);
        ws.loading_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        ws.attached = attached;
        let workspaces = vec![mock_ws("main", true), ws];
        let first = loading_sidebar_frame(
            &ctx,
            &theme,
            &workspaces,
            false,
            false,
            egui::RawInput::default(),
        );
        let slot = theme.workspace_dot_slot().value();
        let sidebar = theme.bg_sidebar().to_egui();
        let bed = crate::adapters::ui::attach_loading::paint_order::flatten(&first)
            .into_iter()
            .find_map(|(c, r)| r.filter(|r| c == sidebar && (r.width() - slot).abs() < 0.01))
            .expect("loading bed");
        let mut output = first;
        for _ in 0..2 {
            let input = egui::RawInput {
                events: vec![egui::Event::PointerMoved(bed.center())],
                ..Default::default()
            };
            output = loading_sidebar_frame(&ctx, &theme, &workspaces, false, false, input);
        }
        (ctx, output, theme)
    }

    /// 시안 LNB 받침: hover 한 비활성 카드는 bg-sidebar 위에 overlay-hover 를 겹친 받침 위에 스피너를 그린다.
    /// hover 채움을 스피너 뒤에 카드 전체로 덮으면 스피너 호까지 흐려진다.
    #[test]
    fn hovered_inactive_card_puts_the_hover_fill_under_the_loading_spinner() {
        let (_ctx, output, theme) = hovered_loading_card(false);
        let shapes = crate::adapters::ui::attach_loading::paint_order::flatten(&output);
        let slot = theme.workspace_dot_slot().value();
        let sidebar = theme.bg_sidebar().to_egui();
        let hover = theme.hover_overlay.to_egui_premultiplied();
        let beds: Vec<egui::Color32> = shapes
            .iter()
            .filter(|(_, r)| r.is_some_and(|r| (r.width() - slot).abs() < 0.01))
            .map(|(c, _)| *c)
            .collect();
        let at = beds
            .iter()
            .position(|c| *c == sidebar)
            .expect("sidebar bed");
        assert_eq!(beds.get(at + 1).copied(), Some(hover), "{beds:?}");
        let (spinner, _) =
            crate::adapters::ui::attach_loading::paint_order::spinner_and_keycap(&output, &theme);
        let spinner = spinner.expect("spinner");
        let bed = shapes
            .iter()
            .find_map(|(c, r)| r.filter(|r| *c == sidebar && (r.width() - slot).abs() < 0.01))
            .expect("bed rect");
        // 같은 색의 1px 구분선 등은 제외하고 스피너 칸을 덮는 hover 채움만 본다.
        let over_slot = shapes
            .iter()
            .rposition(|(c, r)| *c == hover && r.is_some_and(|r| r.contains(bed.center())))
            .expect("hover fill");
        assert!(
            over_slot < spinner,
            "hover fill {over_slot} >= spinner {spinner}"
        );
    }

    /// 카드 전체가 클릭 위젯이므로 점유 점 툴팁 영역은 카드 응답 뒤에 등록해야 hover 된다.
    #[test]
    fn the_occupied_dot_tooltip_area_is_hovered_on_a_card() {
        let (ctx, _output, _theme) = hovered_loading_card(true);
        let card = egui::Id::new(("ws_card", 1usize));
        assert!(ctx.read_response(card).is_some_and(|r| r.hovered()));
        assert!(
            ctx.read_response(card.with("occupied_dot"))
                .is_some_and(|r| r.hovered())
        );
    }

    /// 그려진 글자와 왼쪽 x. 숫자와 이름의 위치를 비교한다.
    fn painted_texts(output: &egui::FullOutput) -> Vec<(String, f32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(text) => out.push((text.galley.text().to_owned(), text.pos.x)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    fn x_of(texts: &[(String, f32)], text: &str) -> f32 {
        texts
            .iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("{text} not painted: {texts:?}"))
            .1
    }

    fn counted_ws(name: &str, busy_count: usize) -> WorkspaceEntryView {
        WorkspaceEntryView {
            busy_count,
            subtitle: format!("{name}-sub"),
            description: format!("{name}-desc"),
            is_mirror: true,
            ..mock_ws(name, false)
        }
    }

    fn counted_frame(
        ctx: &egui::Context,
        workspaces: &[WorkspaceEntryView],
        collapsed: bool,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        loading_sidebar_frame(ctx, &test_theme(), workspaces, collapsed, false, input)
    }

    #[test]
    fn a_zero_row_paints_no_numeral_and_a_counted_row_paints_it_before_the_name() {
        let workspaces = [counted_ws("quiet", 0), counted_ws("busy", 3)];
        let ctx = egui::Context::default();
        let output = counted_frame(&ctx, &workspaces, false, egui::RawInput::default());
        let texts = painted_texts(&output);
        for quiet in ["0", "1"] {
            assert!(!texts.iter().any(|(t, _)| t == quiet), "{texts:?}");
        }
        let theme = test_theme();
        let card = ctx
            .read_response(egui::Id::new(("ws_card", 1usize)))
            .expect("card");
        let numeral = x_of(&texts, "3");
        assert_eq!(
            numeral,
            card.rect.min.x
                + theme.workspace_row_padding_x().value()
                + theme.workspace_dot_slot().value()
                + theme.workspace_running_count_gap().value()
        );
        assert!(numeral < x_of(&texts, "busy"));
    }

    #[test]
    fn body_lines_share_the_name_x_and_only_an_overflow_row_moves() {
        let workspaces = [
            counted_ws("zero", 0),
            counted_ws("one", 1),
            counted_ws("nine", 99),
            counted_ws("over", 100),
        ];
        let output = counted_frame(
            &egui::Context::default(),
            &workspaces,
            false,
            egui::RawInput::default(),
        );
        let texts = painted_texts(&output);
        for name in ["zero", "one", "nine", "over"] {
            let x = x_of(&texts, name);
            assert_eq!(x, x_of(&texts, &format!("{name}-sub")), "{name} subtitle");
            assert_eq!(
                x,
                x_of(&texts, &format!("{name}-desc")),
                "{name} description"
            );
        }
        assert_eq!(x_of(&texts, "zero"), x_of(&texts, "one"));
        assert_eq!(x_of(&texts, "zero"), x_of(&texts, "nine"));
        assert!(x_of(&texts, "over") > x_of(&texts, "zero"), "{texts:?}");
        assert_eq!(x_of(&texts, "99+"), x_of(&texts, "99"));
    }

    #[test]
    fn the_collapsed_rail_paints_no_numeral() {
        let workspaces = [counted_ws("busy", 3)];
        let output = counted_frame(
            &egui::Context::default(),
            &workspaces,
            true,
            egui::RawInput::default(),
        );
        let texts = painted_texts(&output);
        assert!(!texts.iter().any(|(t, _)| t == "3"), "{texts:?}");
    }

    /// 숫자 칸 위에 포인터를 두고 눌렀다 뗀다. 칸 위치는 첫 프레임의 글자 위치로 찾는다.
    fn on_the_numeral(
        workspaces: &[WorkspaceEntryView],
        press: bool,
    ) -> (egui::Context, Vec<SidebarFullAction>) {
        let ctx = egui::Context::default();
        let first = counted_frame(&ctx, workspaces, false, egui::RawInput::default());
        let texts = painted_texts(&first);
        let (_, x) = texts
            .iter()
            .find(|(t, _)| t == "3")
            .expect("numeral painted");
        let card = ctx
            .read_response(egui::Id::new(("ws_card", 0usize)))
            .expect("card");
        let at = egui::pos2(*x + 1.0, card.rect.min.y + 6.0);
        let mut events = vec![vec![egui::Event::PointerMoved(at)], vec![]];
        if press {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            events.push(vec![button(true)]);
            events.push(vec![button(false)]);
        }
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let mut actions = Vec::new();
        for events in events {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            drop(ctx.run(input, |ctx| {
                egui::SidePanel::left("test_loading_keycap").show(ctx, |ui| {
                    let props = counted_props(&theme, &kb, workspaces, None);
                    actions.extend(draw_full_sidebar_view(ui, &props).actions);
                });
            }));
        }
        (ctx, actions)
    }

    fn counted_props<'a>(
        theme: &'a Theme,
        kb: &'a crate::settings::KeybindingSettings,
        workspaces: &'a [WorkspaceEntryView],
        drag: Option<DragSnapshot>,
    ) -> SidebarFullProps<'a> {
        SidebarFullProps {
            theme,
            kb,
            workspaces,
            categories: None,
            drag,
            notifications: None,
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
            workspace_switch_held: false,
            category_switch_held: false,
        }
    }

    #[test]
    fn the_drag_ghost_paints_the_name_without_the_numeral() {
        let workspaces = [counted_ws("busy", 3)];
        let (theme, kb) = (test_theme(), crate::settings::KeybindingSettings::default());
        let ctx = egui::Context::default();
        let at = egui::pos2(40.0, 120.0);
        let mut output = None;
        let frames = [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        ];
        for events in frames {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            output = Some(ctx.run(input, |ctx| {
                egui::SidePanel::left("test_drag_ghost").show(ctx, |ui| {
                    let drag = DragSnapshot {
                        ws_idx: 0,
                        current_y: at.y,
                    };
                    let props = counted_props(&theme, &kb, &workspaces, Some(drag));
                    drop(draw_full_sidebar_view(ui, &props));
                });
            }));
        }
        let texts = painted_texts(&output.expect("frame ran"));
        let count = |text: &str| texts.iter().filter(|(t, _)| t == text).count();
        assert_eq!(count("busy"), 2, "card and ghost: {texts:?}");
        assert_eq!(count("3"), 1, "card only: {texts:?}");
    }

    #[test]
    fn the_numeral_ink_follows_the_row_state() {
        let theme = test_theme();
        let workspaces = [
            counted_ws("rest", 3),
            WorkspaceEntryView {
                is_active: true,
                ..counted_ws("selected", 4)
            },
        ];
        let output = counted_frame(
            &egui::Context::default(),
            &workspaces,
            false,
            egui::RawInput::default(),
        );
        assert_eq!(painted_color(&output, "3"), theme.text_muted().to_egui());
        assert_eq!(
            painted_color(&output, "4"),
            theme.text_on_active().to_egui()
        );
    }

    fn painted_color(output: &egui::FullOutput, wanted: &str) -> egui::Color32 {
        fn walk(shape: &egui::Shape, wanted: &str) -> Option<egui::Color32> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == wanted => {
                    Some(text.fallback_color)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| walk(s, wanted)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|c| walk(&c.shape, wanted))
            .unwrap_or_else(|| panic!("{wanted} not painted"))
    }

    #[test]
    fn the_tooltip_states_the_exact_count_past_the_overflow_notation() {
        let text = card_status::running_count_tooltip(128);
        assert!(text.contains("128"), "{text}");
        assert!(!text.contains("99+"), "{text}");
    }

    #[test]
    fn the_tooltip_zone_covers_the_numeral_and_leaves_the_row_click_alone() {
        let workspaces = [mock_ws_busy("busy", 3)];
        let (ctx, _) = on_the_numeral(&workspaces, false);
        let card = egui::Id::new(("ws_card", 0usize));
        assert!(ctx.read_response(card).is_some_and(|r| r.hovered()));
        assert!(
            ctx.read_response(card.with("running_count"))
                .is_some_and(|r| r.hovered())
        );
        let (_, actions) = on_the_numeral(&workspaces, true);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, SidebarFullAction::WorkspaceClicked(0))),
            "click on the numeral must still switch the workspace"
        );
    }

    #[test]
    fn a_zero_row_registers_no_tooltip_zone() {
        let ctx = egui::Context::default();
        let workspaces = [mock_ws_busy("quiet", 0)];
        drop(counted_frame(
            &ctx,
            &workspaces,
            false,
            egui::RawInput::default(),
        ));
        let card = egui::Id::new(("ws_card", 0usize));
        assert!(ctx.read_response(card).is_some());
        assert!(ctx.read_response(card.with("running_count")).is_none());
    }

    fn mock_ws_busy(name: &str, busy_count: usize) -> WorkspaceEntryView {
        WorkspaceEntryView {
            busy_count,
            ..mock_ws(name, false)
        }
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
                    notifications: None,
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
                    notifications: None,
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
        // mirror·점유·알림이 함께 있는 행도 렌더링한다.
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
        // 설정 슬롯을 넘는 워크스페이스까지 포함해 키캡·기본 표시를 함께 확인한다.
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
                loading_since: None,
                name: "A workspace name long enough to require single-line ellipsis".into(),
                subtitle: "a subtitle label that is also fairly long for ellipsis".into(),
                description: long_desc.into(),
                busy_count: 0,
                completion_count: 150,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                move_source: false,
                attach_refusal: None,
                is_active: true,
            },
            WorkspaceEntryView {
                loading_since: None,
                name: "title only".into(),
                subtitle: String::new(),
                description: String::new(),
                busy_count: 0,
                completion_count: 0,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                move_source: false,
                attach_refusal: None,
                is_active: false,
            },
            WorkspaceEntryView {
                loading_since: None,
                name: "title + description".into(),
                subtitle: String::new(),
                description: "short desc".into(),
                busy_count: 0,
                completion_count: 0,
                needs_input_count: 0,
                attached: false,
                is_mirror: false,
                move_source: false,
                attach_refusal: None,
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
                    notifications: None,
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
        // 활성 카테고리의 로컬 인덱스로만 키캡을 표시한다.
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
                entries: vec![(0, mock_ws("a", false)), (1, mock_ws("b", false))],
            },
            CategorySectionView {
                id: 1,
                label: "Services".into(),
                collapsed: false,
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
            loading_since: None,
            name: "active".into(),
            subtitle: String::new(),
            description: String::new(),
            busy_count: 3,
            completion_count: 2,
            needs_input_count: 0,
            attached: true,
            is_mirror: false,
            move_source: false,
            attach_refusal: None,
            is_active: true,
        }];
        let actions = run_collapsed(ws, false);
        assert!(actions.is_empty());
    }
}
