//! 펼친 사이드바의 입력을 만들고 화면 동작을 처리한다.

use crate::i18n::t;
use crate::intent::Intent;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;

use super::view::{
    CategorySectionView, DragSnapshot, SidebarFullAction, SidebarFullProps, WorkspaceEntryView,
    draw_full_sidebar_view,
};

/// 워크스페이스 1개를 `WorkspaceEntryView` snapshot 으로 변환. collapsed 레일도 공유.
pub(super) fn entry_view(
    notices: &crate::adapters::ui::attach_notice::AttachNotices,
    pending: Option<crate::state::PendingMove>,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    global_idx: usize,
    ws: &crate::model::Workspace,
    active_ws: usize,
) -> WorkspaceEntryView {
    let surface_ids = ws.all_surface_ids();
    WorkspaceEntryView {
        loading_since: None,
        name: ws.name.clone(),
        subtitle: ws.subtitle.clone(),
        description: ws.description.clone(),
        busy_count: engine.busy_count(&surface_ids),
        completion_count: engine
            .attention_count_of_kind(crate::core::AttentionKind::Completion, &surface_ids),
        needs_input_count: engine
            .attention_count_of_kind(crate::core::AttentionKind::NeedsInput, &surface_ids),
        attached: engine.live.occupancy.workspace_holder(ws.id).is_some(),
        is_mirror: ws.mirror,
        is_active: global_idx == active_ws,
        move_source: crate::adapters::ui::move_source::workspace_cue(engine, active_ws, pending)
            == Some(global_idx),
        attach_refusal: notices.tooltip(ws.id),
        row_state: None,
    }
    .with_remote_row(notices, engine, ws)
}

/// 카테고리 표시가 켜져 있으면 저장된 순서로 그룹을 만든다. 행은 전역 워크스페이스 인덱스를 유지한다.
pub(super) fn build_category_sections(
    notices: &crate::adapters::ui::attach_notice::AttachNotices,
    presentation: &dyn crate::model::StructurePresentation,
    pending: Option<crate::state::PendingMove>,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    active_ws: usize,
) -> Option<Vec<CategorySectionView>> {
    if !engine.settings.general.workspace_categories_enabled {
        return None;
    }
    let workspaces_heading = t("sidebar.workspaces_heading").to_string();
    Some(
        engine
            .categories()
            .iter()
            .map(|cat| {
                let entries = engine
                    .workspaces_in_category(cat.id)
                    .into_iter()
                    .map(|(gi, ws)| (gi, entry_view(notices, pending, engine, gi, ws, active_ws)))
                    .collect();
                CategorySectionView {
                    id: cat.id,
                    label: if cat.is_normal() {
                        workspaces_heading.clone()
                    } else {
                        cat.name.clone()
                    },
                    collapsed: presentation.category_collapsed(cat.id),
                    entries,
                }
            })
            .collect(),
    )
}

pub struct FullSidebarResult {
    pub collapse_clicked: bool,
    pub plugins_clicked: bool,
    pub settings_clicked: bool,
    pub tools_rect: Option<egui::Rect>,
}

pub fn draw_full_sidebar(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sidebar_width: f32,
    plugin_alert: usize,
) -> FullSidebarResult {
    let th = theme::theme();
    let active_ws = state.active_workspace_index(engine);
    let mut workspaces: Vec<WorkspaceEntryView> = engine
        .workspaces()
        .into_iter()
        .enumerate()
        .map(|(i, ws)| {
            entry_view(
                &state.attach_notices,
                state.pending_move,
                engine,
                i,
                ws,
                active_ws,
            )
        })
        .collect();

    let mut sections = build_category_sections(
        &state.attach_notices,
        &state.navigation,
        state.pending_move,
        engine,
        active_ws,
    );
    super::full::apply_attach_loading(state, engine, &mut workspaces, &mut sections);

    let drag = state.dialogs.ws_drag.as_ref().map(|d| DragSnapshot {
        ws_idx: d.ws_idx,
        current_y: d.current_y,
    });

    let tools_label = t("sidebar.tools_button");
    let collapse_label = t("sidebar.collapse_button").to_string();
    let plugins_label = t("button.plugins");
    let settings_label = t("button.settings");
    let new_workspace_label = t("button.new_workspace");
    let workspaces_heading = t("sidebar.workspaces_heading");
    let occupied_hover = t("attach.occupied_workspace");
    let mirror_hover = t("attach.mirror_workspace");
    let mirror_pill_label = t("attach.mirror_pill_label");

    let mut result = FullSidebarResult {
        collapse_clicked: false,
        plugins_clicked: false,
        settings_clicked: false,
        tools_rect: None,
    };

    let mut deferred_actions: Vec<SidebarFullAction> = Vec::new();
    let mut resize_priority_hovered = false;

    // 실제 modifier 입력을 switch_overlay로 판별해 숫자 키캡을 표시한다.
    let workspace_switch_held = {
        let mods = ctx.input(|i| i.modifiers);
        crate::adapters::ui::switch_overlay::workspace_switch_held(
            mods,
            crate::adapters::ui::input::shortcuts::super_held(ctx),
            &engine.settings.keybindings,
        )
    };
    let category_switch_held = engine.settings.general.workspace_categories_enabled && {
        let mods = ctx.input(|i| i.modifiers);
        crate::adapters::ui::switch_overlay::category_switch_held(
            mods,
            crate::adapters::ui::input::shortcuts::super_held(ctx),
            &engine.settings.keybindings,
        )
    };

    let notification_data = crate::adapters::ui::notification::summary_data(engine);
    let notifications = notification_data.view(state.popups.is_open("notifications"));
    let panel_resp = egui::SidePanel::left("workspace_sidebar")
        .exact_width(sidebar_width)
        .resizable(false)
        .show_separator_line(false)
        // 행 배경과 구분선을 가장자리까지 그리도록 패널 여백을 없애고 구역별 여백을 쓴다.
        .frame(egui::Frame::new().fill(th.bg_sidebar().to_egui()))
        .show(ctx, |ui| {
            let props = SidebarFullProps {
                notifications: Some(&notifications),
                theme: &th,
                kb: &engine.settings.keybindings,
                workspaces: &workspaces,
                categories: sections.as_deref(),
                drag,
                tools_label,
                collapse_label: &collapse_label,
                plugins_label,
                settings_label,
                new_workspace_label,
                workspaces_heading,
                occupied_hover,
                mirror_hover,
                mirror_pill_label,
                plugin_alert,
                workspace_switch_held,
                category_switch_held,
            };
            let result = draw_full_sidebar_view(ui, &props);
            deferred_actions = result.actions;
            resize_priority_hovered = result.resize_priority_hovered;
        });
    state.resize_edge_widget_hovered |= resize_priority_hovered;

    // 패널 rect에는 리사이즈 영역이 포함되므로 실제 sidebar_width로 경계선을 정한다.
    let panel_rect = panel_resp.response.rect;
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("sidebar_right_border"),
    ))
    .vline(
        sidebar_width - 0.5,
        panel_rect.y_range(),
        egui::Stroke::new(th.border_width.value(), th.border_default().to_egui()),
    );

    let ws_count = engine.workspaces().len();

    for action in deferred_actions {
        match action {
            SidebarFullAction::Collapse => result.collapse_clicked = true,
            SidebarFullAction::Plugins => result.plugins_clicked = true,
            SidebarFullAction::Settings => result.settings_clicked = true,
            SidebarFullAction::ToolsClicked(rect) => result.tools_rect = Some(rect),
            SidebarFullAction::NotificationsClicked(rect) => {
                crate::adapters::ui::notification::toggle_from_sidebar(state, rect)
            }
            SidebarFullAction::WorkspaceClicked(i) => {
                state.switch_workspace(engine, i);
            }
            SidebarFullAction::WorkspaceContextMenu { ws_idx, x, y } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::Workspace { ws_idx, x, y });
            }
            SidebarFullAction::DragStart { ws_idx, y } => {
                state.dialogs.ws_drag = Some(crate::state::WsDragState {
                    ws_idx,
                    current_y: y,
                });
            }
            SidebarFullAction::DragUpdate { y } => {
                if let Some(drag) = state.dialogs.ws_drag.as_mut() {
                    drag.current_y = y;
                }
            }
            SidebarFullAction::DragReleased {
                drop_target,
                target_category,
            } => {
                let from = state.dialogs.ws_drag.as_ref().map(|d| d.ws_idx);
                state.dialogs.ws_drag = None;
                if let Some(from) = from
                    && from < engine.workspaces().len()
                {
                    let src_cat = engine
                        .workspace_at(from)
                        .expect("workspace index is valid")
                        .category;
                    match target_category {
                        Some(target_cat) if target_cat != src_cat => {
                            state.dispatch_intent(
                                crate::app::command::DomainIntent::SetWorkspaceCategory {
                                    workspace_id: engine
                                        .workspace_at(from)
                                        .expect("workspace index is valid")
                                        .id,
                                    category: target_cat,
                                }
                                .from_user_menu("sidebar_drag_category"),
                            );
                        }
                        _ => {
                            if let Some(to) = drop_target
                                && to < ws_count
                            {
                                state.move_workspace(engine, from, to);
                            }
                        }
                    }
                }
            }
            SidebarFullAction::NewWorkspace => {
                state.dispatch_intent(
                    Intent::NewWorkspace {
                        kind: None,
                        params: serde_json::Value::Null,
                        category: None,
                    }
                    .from_user_menu("sidebar_add_workspace"),
                );
            }
            SidebarFullAction::NewWorkspaceContextMenu { x, y } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::NewWorkspaceButton { x, y });
            }
            SidebarFullAction::CategoryHeaderToggle(cat_id) => {
                // 카테고리 접힘 상태는 레이아웃에 저장하며 펼친 화면과 레일이 공유한다.
                state.dispatch_intent(
                    crate::intent::UiIntent::ToggleCategoryCollapsed { id: cat_id }
                        .from_user_menu("sidebar_category_header"),
                );
            }
            SidebarFullAction::CategoryHeaderContextMenu { cat_id, x, y } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::WorkspaceCategoryHeader { cat_id, x, y });
            }
            SidebarFullAction::BackgroundContextMenu { x, y } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::SidebarBackground { x, y });
            }
        }
    }

    result
}

pub(super) fn apply_attach_loading(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    workspaces: &mut [WorkspaceEntryView],
    sections: &mut Option<Vec<CategorySectionView>>,
) {
    let since = |index| {
        engine.workspace_at(index).and_then(|workspace| {
            state
                .attach_pending
                .entries
                .iter()
                .filter(|entry| {
                    entry.workspace == workspace.id
                        && !entry.receipt.finished()
                        && entry.epoch.is_active()
                })
                .map(|entry| entry.receipt.admitted)
                .chain(
                    state
                        .attach_waits
                        .get(&workspace.id)
                        .into_iter()
                        .flatten()
                        .map(|wait| wait.admitted),
                )
                .min()
        })
    };
    // 저장된 원격 행은 연결을 시도하는 동안에도 같은 로딩 표시(LNB 스피너·레일 링)를 쓴다.
    let since = |index| {
        since(index).or_else(|| {
            engine
                .workspace_at(index)
                .and_then(|ws| engine.remote_row_view(ws.id).trying_since)
        })
    };
    for (index, entry) in workspaces.iter_mut().enumerate() {
        entry.loading_since = since(index);
    }
    for section in sections.iter_mut().flatten() {
        for (index, entry) in &mut section.entries {
            entry.loading_since = since(*index);
        }
    }
}
