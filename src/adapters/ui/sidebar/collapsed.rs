//! 접힌 사이드바의 입력을 만들고 화면 동작을 처리한다.

use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;

use super::full::{build_category_sections, entry_view};
use super::view::{
    SidebarCollapsedAction, SidebarCollapsedProps, WorkspaceEntryView, draw_collapsed_sidebar_view,
};

pub struct CollapsedSidebarResult {
    pub expand_clicked: bool,
    pub plugins_clicked: bool,
    pub settings_clicked: bool,
    pub tools_rect: Option<egui::Rect>,
    pub switch_ws: Option<usize>,
    pub add_ws: bool,
}

pub fn draw_collapsed_sidebar(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    sidebar_width: f32,
    plugin_alert: usize,
) -> CollapsedSidebarResult {
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

    let tools_hover = t("sidebar.tools_button");

    let mut result = CollapsedSidebarResult {
        expand_clicked: false,
        plugins_clicked: false,
        settings_clicked: false,
        tools_rect: None,
        switch_ws: None,
        add_ws: false,
    };

    let mut deferred_actions: Vec<SidebarCollapsedAction> = Vec::new();
    let mut resize_priority_hovered = false;

    // 워크스페이스 전환 modifier를 누르면 문자 아이콘 대신 숫자 키캡을 표시한다.
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
        .show(ctx, |ui| {
            let props = SidebarCollapsedProps {
                notifications: Some(&notifications),
                theme: &th,
                kb: &engine.settings.keybindings,
                workspaces: &workspaces,
                categories: sections.as_deref(),
                tools_hover,
                plugin_alert,
                workspace_switch_held,
                category_switch_held,
            };
            let result = draw_collapsed_sidebar_view(ui, &props);
            deferred_actions = result.actions;
            resize_priority_hovered = result.resize_priority_hovered;
        });
    state.resize_edge_widget_hovered |= resize_priority_hovered;

    // 우측 경계선 (ui_kit border-right) — sidebar_width 기준 (panel rect 는 separator
    // 영역 때문에 더 커서 터미널을 침범한다).
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

    for action in deferred_actions {
        match action {
            SidebarCollapsedAction::Expand => result.expand_clicked = true,
            SidebarCollapsedAction::Plugins => result.plugins_clicked = true,
            SidebarCollapsedAction::Settings => result.settings_clicked = true,
            SidebarCollapsedAction::ToolsClicked(rect) => result.tools_rect = Some(rect),
            SidebarCollapsedAction::NotificationsClicked(rect) => {
                crate::adapters::ui::notification::toggle_from_sidebar(state, rect)
            }
            SidebarCollapsedAction::WorkspaceClicked(i) => result.switch_ws = Some(i),
            SidebarCollapsedAction::NewWorkspace => result.add_ws = true,
            SidebarCollapsedAction::NewWorkspaceContextMenu { x, y } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::NewWorkspaceButton { x, y });
            }
            SidebarCollapsedAction::RailCategoryClicked { cat_id, anchor } => {
                state.dialogs.rail_category_popup = Some(cat_id);
                let pos = egui::pos2(anchor.right() + 6.0, anchor.top() - 6.0);
                state.dispatch_intent(
                    crate::intent::UiIntent::OpenPopup {
                        id: crate::adapters::ui::popup::rail_category::RAIL_CATEGORY_POPUP_ID,
                        mode: crate::intent::OpenPopupMode::AtFocused(
                            crate::model::popup_kind::PopupScope::Window,
                            pos,
                        ),
                    }
                    .from_user_menu("rail_category_button"),
                );
            }
        }
    }

    result
}
