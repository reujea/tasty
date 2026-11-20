//! 설정된 키바인딩을 그룹 순서대로 비교해 처음 매칭된 액션을 실행한다.
//! 그룹 순서도 충돌 우선순위에 영향을 준다.

use crate::runtime::engine_read::EngineRead;
use winit::keyboard::{Key, ModifiersState};

use crate::intent::{Intent, UiIntent};
use crate::model::SplitDirection;
use crate::view::main::MainView;

use super::{matches_any_binding, send_app_event, target_explorer_surface_id};
use crate::state::command_target::CommandTarget;

/// 같은 창·프레임에서 얻은 셀 크기와 배율을 함께 전달한다.
#[derive(Clone, Copy)]
pub(crate) struct CellGeometry {
    pub w: crate::model::PhysicalPx,
    pub h: crate::model::PhysicalPx,
    /// 논리↔물리 변환 배율. pane 보더가 논리라 레이아웃 계산에 필요하다.
    pub scale_factor: f32,
}

/// 키바인딩 프리셋과 구별되는 workspace/tab/pane 레이아웃 적용 범위.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetApplyScope {
    Workspace,
    Tab,
    Pane,
}

impl MainView {
    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    pub(crate) fn handle_keybinding_shortcuts(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        target: &CommandTarget,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        let t = target;
        if Self::match_create_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_split_bindings(state, engine, t, kb, key, mods, terminal_rect, cells) {
            return true;
        }
        if Self::match_panel_bindings(state, engine, t, kb, key, mods, proxy) {
            return true;
        }
        if Self::match_close_bindings(state, engine, t, kb, key, mods, terminal_rect, cells) {
            return true;
        }
        if Self::match_focus_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_sidebar_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_restore_quit_bindings(
            state,
            engine,
            t,
            kb,
            key,
            mods,
            terminal_rect,
            cells,
            proxy,
        ) {
            return true;
        }
        if Self::match_convert_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_capture_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_window_tab_bindings(
            state,
            engine,
            t,
            kb,
            key,
            mods,
            terminal_rect,
            cells,
            proxy,
        ) {
            return true;
        }
        if Self::match_rename_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if matches_any_binding(&kb.refresh, key, mods)
            && (super::surface_actions::held_for_draft(state, engine, t, "refresh")
                || super::request_target_surface_refresh(
                    state,
                    engine,
                    t,
                    crate::intent::RefreshTrigger::Shortcut,
                ))
        {
            return true;
        }
        if Self::match_explorer_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_preset_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_tools_menu_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        if Self::match_copy_rename_bindings(state, engine, t, kb, key, mods) {
            return true;
        }
        false
    }

    pub(super) fn match_create_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.new_workspace, key, mods) {
            let category = t.find_workspace(engine).map(|ws| ws.category);
            state.dispatch_intent(
                Intent::NewWorkspace {
                    kind: None,
                    params: serde_json::Value::Null,
                    category,
                    cwd_source: t.cwd_source(),
                }
                .from_user_shortcut("new_workspace"),
            );
            return true;
        }
        if matches_any_binding(&kb.new_tab, key, mods) {
            super::structure::new_terminal_tab(state, engine, t, "new_tab");
            return true;
        }
        false
    }

    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_split_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
    ) -> bool {
        if matches_any_binding(&kb.split_pane_vertical, key, mods) {
            super::structure::split_pane(
                state,
                engine,
                t,
                SplitDirection::Vertical,
                "split_pane_vertical",
            );
            state.resize_all(
                engine,
                terminal_rect,
                cells.w.value(),
                cells.h.value(),
                cells.scale_factor,
            );
            return true;
        }
        if matches_any_binding(&kb.split_pane_horizontal, key, mods) {
            super::structure::split_pane(
                state,
                engine,
                t,
                SplitDirection::Horizontal,
                "split_pane_horizontal",
            );
            state.resize_all(
                engine,
                terminal_rect,
                cells.w.value(),
                cells.h.value(),
                cells.scale_factor,
            );
            return true;
        }
        if matches_any_binding(&kb.split_surface_vertical, key, mods) {
            super::structure::split_surface(
                state,
                engine,
                t,
                SplitDirection::Vertical,
                "split_surface_vertical",
            );
            state.resize_all(
                engine,
                terminal_rect,
                cells.w.value(),
                cells.h.value(),
                cells.scale_factor,
            );
            return true;
        }
        if matches_any_binding(&kb.split_surface_horizontal, key, mods) {
            super::structure::split_surface(
                state,
                engine,
                t,
                SplitDirection::Horizontal,
                "split_surface_horizontal",
            );
            state.resize_all(
                engine,
                terminal_rect,
                cells.w.value(),
                cells.h.value(),
                cells.scale_factor,
            );
            return true;
        }
        false
    }

    fn match_panel_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        if matches_any_binding(&kb.toggle_settings, key, mods) {
            send_app_event(proxy, crate::AppEvent::OpenSettings);
            return true;
        }
        if matches_any_binding(&kb.toggle_notifications, key, mods) {
            state.dispatch_intent(
                UiIntent::TogglePopup(crate::intent::OpenRequest::new(
                    "notifications",
                    crate::model::popup_kind::PopupScope::Window,
                ))
                .from_user_shortcut("toggle_notifications"),
            );
            return true;
        }
        if matches_any_binding(&kb.toggle_dag_list, key, mods) {
            Self::toggle_dag_list_popup(state, t);
            return true;
        }
        if matches_any_binding(&kb.find, key, mods) {
            if super::surface_actions::held_for_draft(state, engine, t, "find") {
                return true;
            }
            // 터미널에서만 검색 팝업을 연다. 검색창이 이미 포커스를 받았으면 egui가 처리한다.
            // webview의 자체 find는 호스트로 전달하지 않으며 다른 비터미널도 여기서는 소비하지 않는다.
            // 탐색기는 자기 Find 바를 연다.
            return t.surface_id().is_some_and(|sid| {
                super::surface_actions::find_on(state, engine, sid, "find_open")
            });
        }
        false
    }

    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_close_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
    ) -> bool {
        if matches_any_binding(&kb.close_workspace, key, mods) {
            super::close_target_workspace(state, engine, t);
            if !engine.workspaces().is_empty() {
                state.resize_all(
                    engine,
                    terminal_rect,
                    cells.w.value(),
                    cells.h.value(),
                    cells.scale_factor,
                );
            }
            return true;
        }
        if matches_any_binding(&kb.close_pane, key, mods) {
            super::close_pane_action(state, engine, t);
            if !engine.workspaces().is_empty() {
                state.resize_all(
                    engine,
                    terminal_rect,
                    cells.w.value(),
                    cells.h.value(),
                    cells.scale_factor,
                );
            }
            return true;
        }
        if matches_any_binding(&kb.close_surface, key, mods) {
            super::close_surface_action(state, engine, t);
            if !engine.workspaces().is_empty() {
                state.resize_all(
                    engine,
                    terminal_rect,
                    cells.w.value(),
                    cells.h.value(),
                    cells.scale_factor,
                );
            }
            return true;
        }
        false
    }

    fn match_focus_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.focus_pane_next, key, mods) {
            state.move_pane_focus_in(engine, t, true);
            return true;
        }
        if matches_any_binding(&kb.focus_pane_prev, key, mods) {
            state.move_pane_focus_in(engine, t, false);
            return true;
        }
        if matches_any_binding(&kb.focus_surface_next, key, mods) {
            state.move_surface_focus_in(engine, t, false);
            return true;
        }
        if matches_any_binding(&kb.focus_surface_prev, key, mods) {
            state.move_surface_focus_in(engine, t, true);
            return true;
        }
        false
    }

    fn match_sidebar_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.toggle_sidebar, key, mods) {
            state.sidebar_visible = !state.sidebar_visible;
            return true;
        }
        if matches_any_binding(&kb.toggle_sidebar_collapse, key, mods) {
            state.sidebar_collapsed = !state.sidebar_collapsed;
            return true;
        }
        if engine.settings.general.workspace_categories_enabled
            && matches_any_binding(&kb.toggle_categories_collapsed, key, mods)
        {
            state.dispatch_intent(
                crate::intent::UiIntent::ToggleAllCategoriesCollapsed
                    .from_user_shortcut("toggle_categories_collapsed"),
            );
            return true;
        }
        false
    }

    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_restore_quit_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        if matches_any_binding(&kb.restore_closed, key, mods) {
            super::structure::restore_closed(state, engine, t, "restore_closed");
            state.resize_all(
                engine,
                terminal_rect,
                cells.w.value(),
                cells.h.value(),
                cells.scale_factor,
            );
            return true;
        }
        if matches_any_binding(&kb.quit_immediate, key, mods) {
            send_app_event(proxy, crate::AppEvent::Shutdown);
            return true;
        }
        if matches_any_binding(&kb.quit_minimize, key, mods) {
            send_app_event(proxy, crate::AppEvent::Minimize);
            return true;
        }
        if matches_any_binding(&kb.quit, key, mods) {
            send_app_event(proxy, crate::AppEvent::QuitRequested);
            return true;
        }
        false
    }

    fn match_convert_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.open_markdown, key, mods) {
            state.enqueue_convert_input_popup(&engine.as_ref(), "markdown", None);
            return true;
        }
        if matches_any_binding(&kb.open_explorer, key, mods) {
            Self::open_explorer_tab(state, engine, t);
            return true;
        }
        if matches_any_binding(&kb.convert_surface, key, mods) {
            if super::surface_actions::held_for_draft(state, engine, t, "convert_surface") {
                return true;
            }
            if let Some(sid) = t.surface_id() {
                super::surface_actions::open_convert_popup(state, sid, "convert_surface");
            }
            return true;
        }
        if matches_any_binding(&kb.convert_to_markdown, key, mods) {
            if super::surface_actions::held_for_draft(state, engine, t, "convert_to_markdown") {
                return true;
            }
            if let Some(sid) = t.surface_id() {
                state.enqueue_convert_input_popup(&engine.as_ref(), "markdown", Some(sid));
            }
            return true;
        }
        if matches_any_binding(&kb.convert_to_explorer, key, mods) {
            if super::surface_actions::held_for_draft(state, engine, t, "convert_to_explorer") {
                return true;
            }
            if let Some(sid) = t.surface_id() {
                super::surface_actions::convert_to_explorer(state, sid, "convert_to_explorer");
            }
            return true;
        }
        false
    }

    /// 캡처가 끝나기 전 포커스가 바뀌어도 대상이 바뀌지 않도록 여기서 로컬/원격을 정해 큐에 넣는다.
    /// 실제 캡처는 App의 백그라운드 작업이 수행한다.
    fn match_capture_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.screenshot_to_clipboard, key, mods) {
            Self::queue_screenshot_to_clipboard(state, engine, t);
            return true;
        }
        false
    }

    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_window_tab_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        if matches_any_binding(&kb.new_window, key, mods) {
            send_app_event(
                proxy,
                crate::AppEvent::CreateWindow(crate::app::event::WindowRequestOrigin::User, None),
            );
            return true;
        }
        if matches_any_binding(&kb.close_active, key, mods) {
            super::close_active_action(state, engine, t);
            if !engine.workspaces().is_empty() {
                state.resize_all(
                    engine,
                    terminal_rect,
                    cells.w.value(),
                    cells.h.value(),
                    cells.scale_factor,
                );
            }
            return true;
        }
        if matches_any_binding(&kb.next_tab, key, mods) {
            state.step_visible_tab_in(engine, t, true);
            return true;
        }
        if matches_any_binding(&kb.prev_tab, key, mods) {
            state.step_visible_tab_in(engine, t, false);
            return true;
        }
        false
    }

    pub(super) fn match_rename_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.rename_tab, key, mods) {
            super::open_tab_rename(state, engine, t);
            return true;
        }
        if matches_any_binding(&kb.rename_workspace, key, mods) {
            if let Some(ws) = t.find_workspace(engine) {
                let target = crate::state::RenameTarget::WorkspaceName {
                    workspace_id: ws.id,
                };
                crate::adapters::ui::rename_popup::open(
                    state,
                    engine,
                    target,
                    ws.name.clone(),
                    |intent| intent.from_user_shortcut("rename_workspace"),
                );
            }
            return true;
        }
        false
    }

    pub(super) fn match_explorer_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if super::focused_explorer_typing(state, engine) {
            return false;
        }
        if matches_any_binding(&kb.explorer_refresh, key, mods)
            && super::surface_actions::held_for_draft(state, engine, t, "explorer_refresh")
        {
            return true;
        }
        if matches_any_binding(&kb.explorer_refresh, key, mods)
            && target_explorer_surface_id(engine, t).is_some()
        {
            if let Some(sid) = target_explorer_surface_id(engine, t) {
                crate::adapters::ui::egui_panels::apply_explorer_action(
                    state,
                    engine,
                    sid,
                    crate::explorer_ui::ExplorerAction::Refresh,
                );
            }
            return true;
        }
        for (binding, action) in [
            (&kb.explorer_toggle_preview, "explorer_toggle_preview"),
            (&kb.explorer_toggle_hidden, "explorer_toggle_hidden"),
            (&kb.explorer_properties, "explorer_properties"),
        ] {
            if matches_any_binding(binding, key, mods)
                && (super::surface_actions::held_for_draft(state, engine, t, action)
                    || target_explorer_surface_id(engine, t).is_some())
            {
                super::explorer_view_shortcut(state, engine, t, action);
                return true;
            }
        }
        if matches_any_binding(&kb.explorer_go_up, key, mods)
            && super::surface_actions::held_for_draft(state, engine, t, "explorer_go_up")
        {
            return true;
        }
        if matches_any_binding(&kb.explorer_go_up, key, mods)
            && target_explorer_surface_id(engine, t).is_some()
        {
            if let Some(sid) = target_explorer_surface_id(engine, t) {
                crate::adapters::ui::egui_panels::apply_explorer_action(
                    state,
                    engine,
                    sid,
                    crate::explorer_ui::ExplorerAction::GoUp,
                );
            }
            return true;
        }
        if matches_any_binding(&kb.explorer_focus_address, key, mods)
            && (super::surface_actions::held_for_draft(state, engine, t, "explorer_focus_address")
                || super::html_focus_address_shortcut(state, engine, t))
        {
            return true;
        }
        for (binding, action) in [
            (&kb.explorer_back, "explorer_back"),
            (&kb.explorer_forward, "explorer_forward"),
            (&kb.explorer_new_folder, "explorer_new_folder"),
            (&kb.explorer_new_file, "explorer_new_file"),
            (&kb.explorer_undo, "explorer_undo"),
            (&kb.explorer_redo, "explorer_redo"),
        ] {
            if matches_any_binding(binding, key, mods)
                && super::surface_actions::held_for_draft(state, engine, t, action)
            {
                return true;
            }
        }
        for (binding, action) in [
            (&kb.explorer_back, "explorer_back"),
            (&kb.explorer_forward, "explorer_forward"),
            (&kb.explorer_focus_address, "explorer_focus_address"),
        ] {
            if matches_any_binding(binding, key, mods)
                && target_explorer_surface_id(engine, t).is_some()
            {
                super::explorer_nav_shortcut(state, engine, t, action);
                return true;
            }
        }
        for (binding, folder) in [
            (&kb.explorer_new_folder, true),
            (&kb.explorer_new_file, false),
        ] {
            if matches_any_binding(binding, key, mods)
                && target_explorer_surface_id(engine, t).is_some()
            {
                if let Some(sid) = target_explorer_surface_id(engine, t) {
                    state.start_explorer_create(engine, sid, None, folder);
                }
                return true;
            }
        }
        for (binding, action) in [
            (&kb.explorer_undo, "explorer_undo"),
            (&kb.explorer_redo, "explorer_redo"),
        ] {
            if matches_any_binding(binding, key, mods)
                && target_explorer_surface_id(engine, t).is_some()
            {
                super::explorer_history_shortcut(state, engine, t, action);
                return true;
            }
        }
        false
    }

    fn match_preset_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.toggle_command_palette, key, mods) {
            state.dispatch_intent(
                UiIntent::TogglePopup(
                    crate::intent::OpenRequest::new(
                        crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID,
                        crate::model::popup_kind::PopupScope::Window,
                    )
                    .centered()
                    .focused(),
                )
                .from_user_shortcut("toggle_command_palette"),
            );
            return true;
        }
        if matches_any_binding(&kb.apply_workspace_preset, key, mods) {
            Self::open_preset_apply_popup(state, engine, PresetApplyScope::Workspace, t);
            return true;
        }
        if matches_any_binding(&kb.apply_tab_preset, key, mods) {
            Self::open_preset_apply_popup(state, engine, PresetApplyScope::Tab, t);
            return true;
        }
        if matches_any_binding(&kb.apply_pane_preset, key, mods) {
            Self::open_preset_apply_popup(state, engine, PresetApplyScope::Pane, t);
            return true;
        }
        false
    }

    fn match_copy_rename_bindings(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.enter_copy_mode, key, mods) {
            if super::surface_actions::held_for_draft(state, engine, t, "enter_copy_mode") {
                return true;
            }
            state.dialogs.pending_enter_copy_mode = true;
            return true;
        }
        if matches_any_binding(&kb.rename_workspace_subtitle, key, mods) {
            if let Some(ws) = t.find_workspace(engine) {
                let target = crate::state::RenameTarget::WorkspaceSubtitle {
                    workspace_id: ws.id,
                };
                crate::adapters::ui::rename_popup::open(
                    state,
                    engine,
                    target,
                    ws.subtitle.clone(),
                    |intent| intent.from_user_shortcut("rename_workspace_subtitle"),
                );
            }
            return true;
        }
        false
    }

    fn match_tools_menu_bindings(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        use crate::adapters::ui::popup;
        if matches_any_binding(&kb.open_port_scanner, key, mods) {
            Self::open_tool_popup(
                state,
                popup::port_scanner::PORT_SCANNER_POPUP_ID,
                "open_port_scanner",
            );
            return true;
        }
        if matches_any_binding(&kb.open_remote_tool, key, mods) {
            Self::open_tool_popup(
                state,
                popup::remote_tool::REMOTE_TOOL_POPUP_ID,
                "open_remote_tool",
            );
            return true;
        }
        if matches_any_binding(&kb.open_preset_window, key, mods) {
            Self::open_preset_window(state);
            return true;
        }
        if matches_any_binding(&kb.open_tutorial, key, mods) {
            Self::open_tool_popup(
                state,
                crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID,
                "open_tutorial",
            );
            return true;
        }
        if matches_any_binding(&kb.open_file_picker, key, mods) {
            Self::open_file_picker_tool(state, engine, t);
            return true;
        }
        false
    }

    /// 메뉴와 단축키·팔레트가 같은 위치와 포커스 규칙으로 도구 팝업을 연다.
    pub(crate) fn open_tool_popup(
        state: &mut crate::state::MainViewState,
        popup_id: &'static str,
        action: &'static str,
    ) {
        state.dispatch_intent(
            UiIntent::OpenPopup(
                crate::intent::OpenRequest::new(
                    popup_id,
                    crate::model::popup_kind::PopupScope::Window,
                )
                .centered()
                .focused(),
            )
            .from_user_shortcut(action),
        );
    }

    pub(crate) fn open_preset_window(state: &mut crate::state::MainViewState) {
        state.dialogs.pending_open_preset_window = true;
    }

    /// 로컬/원격 대상 정보를 준비해야 하므로 팝업 ID만 보내지 않고 메뉴와 같은 열기 함수를 쓴다.
    pub(crate) fn open_file_picker_tool(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
    ) {
        let Some(workspace) = t.workspace_index(engine) else {
            return;
        };
        crate::adapters::ui::file_open_tool::open_at(
            state,
            engine,
            t.surface_id(),
            t.pane,
            workspace,
        );
    }

    /// 탐색기는 파일 선택 팝업 없이 새 탭을 연다. 경로가 없으면 홈을 사용한다.
    /// 사용자 단축키는 새 탭을 선택하며 에이전트의 탭 생성과 구별한다(docs/design/policies/focus.md).
    pub(crate) fn open_explorer_tab(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        t: &CommandTarget,
    ) {
        super::structure::new_tab(
            state,
            engine,
            t,
            Some("explorer".to_string()),
            serde_json::json!({}),
            "open_explorer",
        );
    }

    /// 명령 대상 workspace에 연결한다. 다른 workspace로 가면 숨고 돌아오면 다시 보인다.
    pub(crate) fn toggle_dag_list_popup(
        state: &mut crate::state::MainViewState,
        t: &CommandTarget,
    ) {
        let Some(workspace_id) = t.workspace else {
            return;
        };
        state.dispatch_intent(
            UiIntent::TogglePopup(
                crate::intent::OpenRequest::new(
                    crate::adapters::ui::popup::dag_list::DAG_LIST_POPUP_ID,
                    crate::adapters::ui::popup::PopupScope::Workspace(workspace_id),
                )
                .centered()
                .focused(),
            )
            .from_user_shortcut("toggle_dag_list"),
        );
    }

    pub(crate) fn queue_screenshot_to_clipboard(
        state: &mut crate::state::MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        t: &CommandTarget,
    ) {
        // 초안에는 아직 surface 가 없다. 생성 뒤 찍으면 사용자가 보지 않은 화면이 된다.
        if let Some(draft) = t.draft {
            tracing::info!(
                "screenshot_to_clipboard ignored: pending attach draft {} is selected",
                draft.0
            );
            return;
        }
        let mirror_ws_id = t.surface_id().and_then(|sid| {
            let (idx, _pane_id) = engine.find_workspace_index_for_surface(sid)?;
            let ws = engine.workspace_at(idx)?;
            ws.mirror.then_some(ws.id)
        });
        let target = t.surface_id().and_then(|surface| {
            crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface)
        });
        state.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::Screenshot {
                target,
                mirror_workspace: mirror_ws_id,
                view: state.webview_identity.clone(),
            })
            .from_user_shortcut("screenshot"),
        );
    }

    /// 프리셋 적용 팝업을 연다. 워크스페이스 프리셋은 창 범위, 탭·pane 프리셋은 명령 대상 탭·pane 의 범위다.
    pub(crate) fn open_preset_apply_popup(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        scope: PresetApplyScope,
        t: &CommandTarget,
    ) {
        use crate::adapters::ui::popup::preset_apply;
        use tasty_presets::PresetKind;
        let (kind, action) = match scope {
            PresetApplyScope::Workspace => {
                state
                    .dialogs
                    .preset_picker_selection
                    .clear(tasty_presets::PresetKind::Workspace);
                state.dispatch_intent(
                    UiIntent::OpenPopup(
                        crate::intent::OpenRequest::new(
                            preset_apply::APPLY_WORKSPACE_POPUP_ID,
                            crate::model::popup_kind::PopupScope::Window,
                        )
                        .centered()
                        .focused(),
                    )
                    .from_user_shortcut("apply_workspace_preset"),
                );
                return;
            }
            PresetApplyScope::Tab => (PresetKind::Tab, "apply_tab_preset"),
            PresetApplyScope::Pane => (PresetKind::Pane, "apply_pane_preset"),
        };
        if let (PresetKind::Tab, Some(draft)) = (kind, t.draft) {
            // 초안 뒤의 기존 탭은 사용자가 보고 있는 탭이 아니다.
            tracing::info!(
                "apply_tab_preset ignored: pending attach draft {} is selected",
                draft.0
            );
            return;
        }
        let target = match kind {
            PresetKind::Tab => t.tab.map(preset_apply::PresetApplyTarget::Tab),
            _ => t.pane.map(preset_apply::PresetApplyTarget::Pane),
        };
        let Some(target) = target else {
            tracing::warn!("{kind:?} preset popup not opened: the command has no target");
            return;
        };
        preset_apply::open_for_target(state, engine, target, |intent| {
            intent.from_user_shortcut(action)
        });
    }
}
