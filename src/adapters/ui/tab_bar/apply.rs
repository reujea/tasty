//! Tab bar actions → application and core state.

use super::{PaneTabBarView, TabBarAction, compute_drop_index};
use crate::model::LogicalPx;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use egui::emath::GuiRounding as _;

/// 탭바 동작을 처리한다. 직접 조작은 대상 pane으로 먼저 포커스를 옮긴다.
pub fn apply_tab_bar_actions(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    actions: Vec<TabBarAction>,
    panes: &[PaneTabBarView],
    tab_w: f32,
    scale_factor: f32,
) {
    let separator_w: f32 = 1.0;

    for action in actions {
        if let TabBarAction::SwitchTab { pane_id, tab_index } = &action
            && let Some(id) = panes
                .iter()
                .find(|pane| pane.pane_id == *pane_id && pane.draft_pane.is_none())
                .and_then(|pane| pane.draft_tabs.get(*tab_index))
                .copied()
                .flatten()
        {
            state.attach_pending.select(
                crate::state::attach_pending::PendingId(id),
                &state.navigation,
            );
            continue;
        }
        if matches!(
            action,
            TabBarAction::SwitchTab { .. } | TabBarAction::FocusPane { .. }
        ) {
            state.attach_pending.deselect();
        }
        if let Some(pane_id) = action.focus_target_pane() {
            state
                .navigation
                .select_pane(state.active_workspace(engine), pane_id);
        }
        match action {
            TabBarAction::SwitchTab { pane_id, tab_index } => {
                let before = state.tutorial_tab_snapshot(engine);
                if let Some(pane) = engine.find_pane_by_id(pane_id) {
                    state.navigation.goto_tab(pane, tab_index);
                }
                state.observe_tutorial_tab_switch(engine, before);
            }
            TabBarAction::CloseTab { pane_id, tab_index } => {
                state.close_tab(engine, pane_id, tab_index);
            }
            TabBarAction::AddTab { pane_id } => {
                let selected = engine
                    .find_pane_by_id(pane_id)
                    .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
                    .and_then(|tab| state.navigation.surface_id(tab));
                let cwd = selected
                    .and_then(|surface| state.resolve_inherit_cwd_from_surface(engine, surface));
                state.dispatch_intent(
                    crate::app::command::DomainIntent::CreateTab {
                        pane_id,
                        cwd,
                        kind: "terminal".into(),
                        name: None,
                        surface_params: serde_json::json!({}),
                        activate: true,
                    }
                    .from_user_menu("tab.create"),
                );
            }
            TabBarAction::RequestSplit { pane_id } => {
                // 누른 버튼의 pane 을 나눈다. cwd 원본은 그때 그 pane 에서 선택된 surface 다.
                let selected = engine
                    .find_pane_by_id(pane_id)
                    .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
                    .and_then(|tab| state.navigation.surface_id(tab));
                state.dispatch_intent(
                    crate::shortcuts::structure::split_pane_at(
                        pane_id,
                        selected,
                        crate::model::SplitDirection::Vertical,
                    )
                    .from_user_shortcut("split_pane_vertical"),
                );
            }
            TabBarAction::OpenSearch { pane_id: _ } => {
                open_search_for_focused_terminal(state, engine);
            }
            TabBarAction::ScrollLeft { pane_id } => {
                let offset = state.tab_bar_scroll.entry(pane_id).or_default();
                *offset = (*offset - LogicalPx(tab_w)).max(LogicalPx(0.0));
            }
            TabBarAction::ScrollRight { pane_id } => {
                *state.tab_bar_scroll.entry(pane_id).or_default() += LogicalPx(tab_w);
            }
            TabBarAction::AutoScrollToActiveTab { pane_id, offset } => {
                apply_auto_scroll(state, engine, pane_id, offset);
            }
            TabBarAction::FocusPane { pane_id: _ } => {}
            TabBarAction::OpenContextMenu {
                pane_id,
                tab_index,
                pos,
            } => {
                state.dialogs.pending_native_menu = Some(crate::state::PendingNativeMenu::Tab {
                    pane_id,
                    tab_index,
                    x: pos.x,
                    y: pos.y,
                });
            }
            TabBarAction::OpenPaneContextMenu { pane_id, pos } => {
                state.dialogs.pending_native_menu = Some(crate::state::PendingNativeMenu::Pane {
                    pane_id,
                    x: pos.x,
                    y: pos.y,
                });
            }
            TabBarAction::OpenNewTabButtonContextMenu { pane_id, pos } => {
                state.dialogs.pending_native_menu =
                    Some(crate::state::PendingNativeMenu::NewTabButton {
                        pane_id,
                        x: pos.x,
                        y: pos.y,
                    });
            }
            TabBarAction::DragStart { pane_id, tab_index } => {
                state.dialogs.tab_drag = Some(crate::state::TabDragState {
                    pane_id,
                    tab_index,
                    current_x: 0.0,
                });
            }
            TabBarAction::DragUpdate { pane_id, mouse_x } => {
                if let Some(ref mut drag) = state.dialogs.tab_drag
                    && drag.pane_id == pane_id
                {
                    drag.current_x = mouse_x;
                }
            }
            TabBarAction::ShowHtmlScriptBanner { surface_id } => {
                show_html_script_banner(state, engine, surface_id);
            }
            TabBarAction::DragEnd { pane_id } => {
                apply_drag_end(
                    state,
                    engine,
                    panes,
                    tab_w,
                    scale_factor,
                    separator_w,
                    pane_id,
                );
            }
        }
    }
}

/// 사용자가 탭의 lock 표지를 눌렀다. 표지를 누른 것 자체가 문서를 본 것이다.
fn show_html_script_banner(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) {
    let Some(snapshot) = engine.html_script(surface_id) else {
        return;
    };
    state.dispatch_intent(
        crate::intent::Intent::Frame(
            crate::app::frame_effect::DrawEffect::Html(
                snapshot.request(crate::runtime::html_script::HtmlActionKind::Reshow),
            )
            .into(),
        )
        .from_user_menu("html-script-marker"),
    );
}

/// 검색 버튼은 터미널에서만 동작한다. 검색창은 terminal 데이터만 읽는다.
fn open_search_for_focused_terminal(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if !matches!(
        state.focused_surface_type(engine),
        crate::state::FocusedSurfaceType::Terminal
    ) {
        return;
    }
    let focused = state.focused_surface_id(engine);
    crate::adapters::ui::search_bar::open_or_focus_for(state, focused, "find");
}

/// 화면에서 계산한 자동 스크롤 오프셋을 pane에 반영한다.
fn apply_auto_scroll(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pane_id: u32,
    offset: f32,
) {
    if state
        .active_workspace(engine)
        .pane_layout()
        .find_pane(pane_id)
        .is_some()
    {
        state.tab_bar_scroll.insert(pane_id, LogicalPx(offset));
    }
}

/// 드래그한 탭을 놓은 위치로 옮긴다.
fn apply_drag_end(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    panes: &[PaneTabBarView],
    tab_w: f32,
    scale_factor: f32,
    separator_w: f32,
    pane_id: u32,
) {
    if let Some(drag) = state.dialogs.tab_drag.take()
        && drag.pane_id == pane_id
        && let Some(pane_info) = panes.iter().find(|i| i.pane_id == pane_id)
    {
        let pane_rect = pane_info.rect;
        let pane_logical_x = pane_rect.x.to_logical(scale_factor).value().round_ui();
        let pane_logical_w = pane_rect.width.to_logical(scale_factor).value().round_ui();
        let target = compute_drop_index(
            drag.current_x,
            pane_logical_x,
            pane_info.scroll_offset,
            pane_info.tab_names.len(),
            tab_w,
            separator_w,
            pane_logical_w,
        );
        // mirror 워크스페이스는 로컬 탭 순서 변경 대신 MoveTab 을 원격으로
        // forward 한다(로컬 실행은 원격 트리와 어긋남).
        if target != drag.tab_index {
            let mirror_op = engine
                .find_pane_by_id(pane_id)
                .and_then(|p| p.tabs.get(state.navigation.tab_index(p)))
                .and_then(|t| state.navigation.surface_id(t))
                .map(|sid| crate::ipc::stream::StructuralOp::MoveTab {
                    anchor_surface_id: sid,
                    from_index: drag.tab_index,
                    to_index: target,
                });
            if !state.forward_mirror_structural(engine, mirror_op, Vec::new())
                && let Some(tab_id) = engine
                    .find_pane_by_id(pane_id)
                    .and_then(|pane| pane.tabs.get(drag.tab_index))
                    .map(|tab| tab.id)
            {
                state.dispatch_intent(
                    crate::app::command::DomainIntent::MoveTab {
                        pane_id,
                        tab_id,
                        to_index: target,
                    }
                    .from_user_menu("tab_drag"),
                );
            }
        }
    }
}
