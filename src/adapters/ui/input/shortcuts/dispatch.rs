//! 단축키와 명령 팔레트의 액션 실행.

use crate::runtime::engine_read::EngineRead;
use winit::keyboard::{Key, ModifiersState};

use crate::intent::{Intent, OpenPopupMode, UiIntent};
use crate::view::main::MainView;

use super::copy_paste::ExplorerAction;
use super::keybinding::PresetApplyScope;
use super::surface_actions;
use super::zoom::ZoomAction;

/// `&mut self` 가 필요해 match 뒤로 미루는 팔레트 액션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeferredPaletteAction {
    Copy,
    Cut,
    Paste,
    Explorer(ExplorerAction),
}

impl DeferredPaletteAction {
    /// 플러그인 입력칸에 보낼 수 있는 편집 명령이면 그 편집.
    fn edit(self) -> Option<crate::adapters::ui::input::edit_key::EditKey> {
        use crate::adapters::ui::input::edit_key::EditKey;
        match self {
            Self::Copy => Some(EditKey::Copy),
            Self::Cut => Some(EditKey::Cut),
            Self::Paste => Some(EditKey::Paste),
            Self::Explorer(_) => None,
        }
    }
}

use super::keybinding::CellGeometry;
use super::matches_any_binding;
use super::{focused_explorer_surface_id, focused_workspace_category, send_app_event};

impl MainView {
    /// KeybindingSettings의 field_id로 액션을 실행한다. 알 수 없는 ID는 false다.
    /// 현재 명령 팔레트가 호출하며 에이전트 IPC에는 노출하지 않는다.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: action_id 문자열→액션 평면 match 디스패치 — 단축키와 1:1, arm 나열
    pub(crate) fn dispatch_action_by_id(
        &mut self,
        engine: &EngineRead<'_>,
        action_id: &str,
    ) -> bool {
        use crate::model::SplitDirection;

        // WebView가 포커스면 편집 액션은 그 페이지의 포커스 대상에만 실행한다.
        if let Some(action) = crate::webview::WebViewEditAction::from_field_id(action_id) {
            if self.run_webview_edit(engine, action) {
                return true;
            }
            // 페이지 실행 취소·다시 실행은 WebView 밖에 대상이 없다. 알 수 없는 ID 경고로 남기지 않는다.
            use crate::webview::WebViewEditAction::{Redo, Undo};
            if matches!(action, Undo | Redo) {
                return false;
            }
        }

        let terminal_rect = self.compute_terminal_rect();
        let cell_w = self.base.gpu.cell_width();
        let cell_h = self.base.gpu.cell_height();
        let scale_factor = self.base.gpu.scale_factor();
        let proxy = self.proxy.clone();
        let proxy = &proxy;
        let mut pending_copy_text: Option<String> = None;
        let mut pending_copy_scope: Option<u32> = None;
        // state/engine 차용이 끝난 뒤 clipboard/selection 액션을 실행하도록 모아 둔다.
        let mut deferred: Option<DeferredPaletteAction> = None;
        let state = &mut self.state;

        match action_id {
            "new_workspace" => {
                let category = focused_workspace_category(state, engine);
                state.dispatch_intent(
                    Intent::NewWorkspace {
                        kind: None,
                        params: serde_json::Value::Null,
                        category,
                    }
                    .from_user_shortcut("new_workspace"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "new_tab" => {
                let pane_id = state.focused_pane_id(engine);
                let cwd = state.resolve_inherit_cwd(engine);
                state.dispatch_intent(
                    crate::app::command::DomainIntent::CreateTab {
                        pane_id,
                        cwd,
                        kind: "terminal".into(),
                        name: None,
                        surface_params: serde_json::json!({}),
                        activate: true,
                    }
                    .from_user_shortcut("new_tab"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_pane_vertical" => {
                state.dispatch_intent(
                    Intent::SplitPane {
                        direction: SplitDirection::Vertical,
                    }
                    .from_user_shortcut("split_pane_vertical"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_pane_horizontal" => {
                state.dispatch_intent(
                    Intent::SplitPane {
                        direction: SplitDirection::Horizontal,
                    }
                    .from_user_shortcut("split_pane_horizontal"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_surface_vertical" => {
                state.dispatch_intent(
                    Intent::SplitSurface {
                        direction: SplitDirection::Vertical,
                    }
                    .from_user_shortcut("split_surface_vertical"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_surface_horizontal" => {
                state.dispatch_intent(
                    Intent::SplitSurface {
                        direction: SplitDirection::Horizontal,
                    }
                    .from_user_shortcut("split_surface_horizontal"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "toggle_settings" => {
                send_app_event(proxy, crate::AppEvent::OpenSettings);
            }
            "toggle_notifications" => {
                state.dispatch_intent(
                    UiIntent::TogglePopup {
                        id: "notifications",
                        mode: OpenPopupMode::Default(crate::model::popup_kind::PopupScope::Window),
                    }
                    .from_user_shortcut("toggle_notifications"),
                );
            }
            "fullscreen_stage_exit" => {
                // 키 경로는 앞단에서 닫지만 팔레트도 같은 액션 ID로 종료할 수 있어야 한다.
                state.close_fullscreen_stage();
            }
            "toggle_sidebar" => {
                state.sidebar_visible = !state.sidebar_visible;
            }
            "toggle_sidebar_collapse" => {
                state.sidebar_collapsed = !state.sidebar_collapsed;
            }
            "toggle_categories_collapsed" => {
                if engine.settings.general.workspace_categories_enabled {
                    state.dispatch_intent(
                        crate::intent::UiIntent::ToggleAllCategoriesCollapsed
                            .from_user_shortcut("toggle_categories_collapsed"),
                    );
                }
            }
            "close_workspace" => {
                state.close_active_workspace(engine);
                if engine.workspaces().is_empty() {
                    self.request_close();
                } else {
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                return true;
            }
            "close_pane" => {
                super::close_pane_action(state, engine);
                if engine.workspaces().is_empty() {
                    self.request_close();
                } else {
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                return true;
            }
            "close_surface" => {
                let closed = state.close_active_surface(engine);
                if !closed && !state.close_active_pane(engine) {
                    state.close_active_workspace(engine);
                }
                if engine.workspaces().is_empty() {
                    self.request_close();
                } else {
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                return true;
            }
            "close_active" => {
                super::close_active_action(state, engine);
                if engine.workspaces().is_empty() {
                    self.request_close();
                } else {
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                return true;
            }
            "focus_pane_next" => state.move_pane_focus_forward(engine),
            "focus_pane_prev" => state.move_pane_focus_backward(engine),
            "focus_surface_next" => state.move_surface_focus_forward(engine),
            "focus_surface_prev" => state.move_surface_focus_backward(engine),
            "next_tab" => state.next_tab_in_pane(engine),
            "prev_tab" => state.prev_tab_in_pane(engine),
            "restore_closed" => {
                state.dispatch_intent(
                    crate::intent::Intent::RestoreClosedItem.from_user_shortcut("restore_closed"),
                );
                state.resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "quit" => send_app_event(proxy, crate::AppEvent::QuitRequested),
            "quit_immediate" => send_app_event(proxy, crate::AppEvent::Shutdown),
            "quit_minimize" => send_app_event(proxy, crate::AppEvent::Minimize),
            "new_window" => send_app_event(
                proxy,
                crate::AppEvent::CreateWindow(crate::app::event::WindowRequestOrigin::User, None),
            ),
            "find" => {
                // 터미널 검색만 처리한다. 다른 종류의 자체 검색을 빈 터미널 검색창으로 가리지 않는다.
                // 탐색기는 자기 Find 바를 연다.
                if let Some(sid) = state.focused_surface_id(engine) {
                    surface_actions::find_on(state, engine, sid, "find");
                }
            }
            "open_markdown" => {
                state.enqueue_convert_input_popup(&engine.as_ref(), "markdown", None);
            }
            "open_explorer" => Self::open_explorer_tab(state),
            "convert_surface" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    surface_actions::open_convert_popup(state, sid, "convert_surface");
                }
            }
            "convert_to_markdown" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    state.enqueue_convert_input_popup(&engine.as_ref(), "markdown", Some(sid));
                }
            }
            "convert_to_explorer" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    surface_actions::convert_to_explorer(state, sid, "convert_to_explorer");
                }
            }
            "rename_tab" => {
                if let Some((target, current_name)) = state.active_tab_rename(engine) {
                    crate::adapters::ui::rename_popup::open(
                        state,
                        engine,
                        target,
                        current_name,
                        |intent| intent.from_user_shortcut("rename_tab"),
                    );
                }
            }
            "rename_workspace" => {
                let ws_idx = state.active_workspace_index(engine);
                if let Some(ws) = engine.workspace_at(ws_idx) {
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
            }
            "rename_workspace_subtitle" => {
                let ws_idx = state.active_workspace_index(engine);
                if let Some(ws) = engine.workspace_at(ws_idx) {
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
            }
            "select_all" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                    && let Some(view) = state.explorer_views.get_mut(sid)
                {
                    view.select_all();
                }
            }
            "enter_copy_mode" => {
                state.dialogs.pending_enter_copy_mode = true;
            }
            "copy_path" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                {
                    pending_copy_text = state
                        .explorer_views
                        .get(sid)
                        .and_then(|v| v.selected_paths_text());
                    if pending_copy_text.is_some() {
                        pending_copy_scope = Some(sid);
                    }
                }
            }
            "refresh" => {
                super::request_focused_surface_refresh(
                    state,
                    engine,
                    crate::intent::RefreshTrigger::Palette,
                );
            }
            "explorer_refresh" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                {
                    crate::adapters::ui::egui_panels::apply_explorer_action(
                        state,
                        engine,
                        sid,
                        crate::explorer_ui::ExplorerAction::Refresh,
                    );
                }
            }
            "explorer_go_up" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                {
                    crate::adapters::ui::egui_panels::apply_explorer_action(
                        state,
                        engine,
                        sid,
                        crate::explorer_ui::ExplorerAction::GoUp,
                    );
                }
            }
            "explorer_back" | "explorer_forward" | "explorer_focus_address" => {
                let html = action_id == "explorer_focus_address"
                    && super::html_focus_address_shortcut(state, engine);
                if !html && state.focused_surface_type(engine).is_kind("explorer") {
                    super::explorer_nav_shortcut(state, engine, action_id);
                }
            }
            "explorer_toggle_preview" | "explorer_toggle_hidden" | "explorer_properties" => {
                if state.focused_surface_type(engine).is_kind("explorer") {
                    super::explorer_view_shortcut(state, engine, action_id);
                }
            }
            "explorer_rename" => {
                deferred = Some(DeferredPaletteAction::Explorer(ExplorerAction::Rename))
            }
            "explorer_trash" => {
                deferred = Some(DeferredPaletteAction::Explorer(ExplorerAction::Trash))
            }
            "explorer_new_folder" | "explorer_new_file" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                {
                    state.start_explorer_create(
                        engine,
                        sid,
                        None,
                        action_id == "explorer_new_folder",
                    );
                }
            }
            "explorer_undo" | "explorer_redo" => {
                if state.focused_surface_type(engine).is_kind("explorer") {
                    super::explorer_history_shortcut(state, engine, action_id);
                }
            }
            "toggle_dag_list" => Self::toggle_dag_list_popup(state, engine),
            "open_port_scanner" => Self::open_tool_popup(
                state,
                crate::adapters::ui::popup::port_scanner::PORT_SCANNER_POPUP_ID,
                "open_port_scanner",
            ),
            "open_remote_tool" => Self::open_tool_popup(
                state,
                crate::adapters::ui::popup::remote_tool::REMOTE_TOOL_POPUP_ID,
                "open_remote_tool",
            ),
            "open_tutorial" => Self::open_tool_popup(
                state,
                crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID,
                "open_tutorial",
            ),
            "open_preset_window" => Self::open_preset_window(state),
            "open_file_picker" => Self::open_file_picker_tool(state, engine),
            "screenshot_to_clipboard" => Self::queue_screenshot_to_clipboard(state, engine),
            "apply_workspace_preset" => {
                Self::open_preset_apply_popup(state, engine, PresetApplyScope::Workspace);
            }
            "apply_tab_preset" => {
                Self::open_preset_apply_popup(state, engine, PresetApplyScope::Tab)
            }
            "apply_pane_preset" => {
                Self::open_preset_apply_popup(state, engine, PresetApplyScope::Pane)
            }
            "zoom_in" => {
                Self::apply_zoom(state, engine, ZoomAction::In);
            }
            "zoom_out" => {
                Self::apply_zoom(state, engine, ZoomAction::Out);
            }
            "zoom_reset" => {
                Self::apply_zoom(state, engine, ZoomAction::Reset);
            }
            "copy" => deferred = Some(DeferredPaletteAction::Copy),
            "cut" => deferred = Some(DeferredPaletteAction::Cut),
            "paste" => deferred = Some(DeferredPaletteAction::Paste),
            "minimize_window" => {
                self.base.winit.set_minimized(true);
            }
            "maximize_window" => {
                let maximized = self.base.winit.is_maximized();
                self.base.winit.set_maximized(!maximized);
            }
            "close_window" => {
                send_app_event(proxy, crate::AppEvent::CloseWindow(self.base.winit.id()));
            }
            other => {
                tracing::warn!("dispatch_action_by_id: unknown action '{other}'");
                return false;
            }
        }
        // 회신을 약속한 플러그인 surface 의 편집은 그 플러그인에 요청으로 보내고 아래 경로를 타지 않는다.
        let deferred = deferred.filter(|action| {
            !action
                .edit()
                .is_some_and(|edit| self.ask_plugin_surface_palette_edit(engine, edit))
        });
        // 키 경로와 같은 순서: 복사는 선택 텍스트→탐색기 파일, 붙여넣기는 탐색기→터미널이다.
        match deferred {
            Some(DeferredPaletteAction::Copy) => {
                if !self.run_copy(engine) {
                    self.run_explorer_action(engine, ExplorerAction::CopyFiles);
                }
            }
            Some(DeferredPaletteAction::Cut) => {
                self.run_explorer_action(engine, ExplorerAction::CutFiles);
            }
            Some(DeferredPaletteAction::Paste) => {
                let pasted = self.run_explorer_action(engine, ExplorerAction::PasteFiles);
                if !pasted {
                    self.run_paste(engine);
                }
            }
            Some(DeferredPaletteAction::Explorer(action)) => {
                self.run_explorer_action(engine, action);
            }
            None => {}
        }
        // 키 경로의 after_shortcut_consumed와 같이 진입 요청을 이 실행 안에서 소비한다.
        self.try_enter_vi_copy_mode(engine);
        if let Some(text) = pending_copy_text {
            let scope = pending_copy_scope
                .map(crate::adapters::ui::ToastScope::Surface)
                .unwrap_or(crate::adapters::ui::ToastScope::Window);
            self.copy_text(&text, false, "toast.copied_path", Vec::new(), scope);
        }
        self.base.state.dirty = true;
        true
    }

    /// 창 제어는 공용 액션으로 실행한다. macOS의 첫 조합은 Window 메뉴가 실행한다.
    fn handle_window_control_shortcuts(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
        kb: &crate::settings::KeybindingSettings,
    ) -> bool {
        super::window_control::window_control_action(
            kb,
            key,
            mods,
            super::window_control::OS_MENU_OWNS_FIRST,
        )
        .is_some_and(|action_id| self.dispatch_action_by_id(engine, action_id))
    }

    /// 사용자 스크립트를 읽어 App의 Lua 워커에 실행을 요청한다.
    /// 등록이 없거나 파일을 못 읽어도 매칭된 키는 소비해 다른 액션으로 넘어가지 않게 한다.
    fn try_dispatch_script_shortcut(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let kb = &engine.settings.keybindings;
        let Some(script_id) = kb
            .script_bindings
            .iter()
            .find(|b| matches_any_binding(std::slice::from_ref(&b.combo), key, mods))
            .map(|b| b.script_id.clone())
        else {
            return false;
        };
        let Some(entry) = engine.settings.scripts.get(&script_id) else {
            tracing::warn!(
                target: "tasty_lua",
                "script shortcut matched but script '{script_id}' not registered — ignoring"
            );
            self.toast_script_failure(
                &script_id,
                &crate::hooks::lua_runs::ShortcutFailure::NotRegistered,
            );
            return true;
        };
        let path = entry.path.clone();
        let name = if entry.name.is_empty() {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| script_id.clone())
        } else {
            entry.name.clone()
        };
        let stored_hash = entry.sha256.clone();
        if self.state.script_reads.len() < 8 {
            self.state
                .script_reads
                .push_back(crate::app::local_reads::PendingScript {
                    script_id,
                    name,
                    stored_hash,
                    query: crate::app::local_reads::script(path),
                });
        } else {
            tracing::warn!("script read queue is full");
            self.toast_script_failure(
                &name,
                &crate::hooks::lua_runs::ShortcutFailure::ReadQueueFull,
            );
        }
        true
    }

    fn toast_script_failure(
        &mut self,
        name: &str,
        failure: &crate::hooks::lua_runs::ShortcutFailure<'_>,
    ) {
        self.state.toasts.push(
            crate::hooks::lua_runs::shortcut_failure_text(name, failure),
            crate::adapters::ui::ToastKind::Error,
            crate::adapters::ui::ToastScope::Window,
        );
        self.base.state.dirty = true;
    }

    /// 전역 단축키(`handle_keybinding_shortcuts`)만 처리한다. 대상은 모델 포커스다. 마지막 workspace 를
    /// 닫았으면 창 닫기를 요청한다.
    pub(crate) fn run_keybinding_shortcuts(
        &mut self,
        engine: &EngineRead<'_>,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let cells = CellGeometry {
            w: crate::model::PhysicalPx(self.base.gpu.cell_width()),
            h: crate::model::PhysicalPx(self.base.gpu.cell_height()),
            scale_factor: self.base.gpu.scale_factor(),
        };
        let terminal_rect = self.compute_terminal_rect();
        if !Self::handle_keybinding_shortcuts(
            &mut self.state,
            engine,
            kb,
            key,
            mods,
            terminal_rect,
            cells,
            &self.proxy,
        ) {
            return false;
        }
        if engine.workspaces().is_empty() {
            self.request_close();
        }
        self.base.state.dirty = true;
        true
    }

    pub(crate) fn handle_shortcut(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        self.handle_shortcut_after_edit_judgment(engine, key, mods, false)
    }

    /// `edit_judged`이면 이 키의 편집(복사·잘라내기·붙여넣기) 판정이 이미 끝나 넘긴 키다. 편집 바인딩은
    /// 다시 고르지 않고 그 밖의 바인딩만 판정한다.
    pub(crate) fn handle_shortcut_after_edit_judgment(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
        edit_judged: bool,
    ) -> bool {
        let ctrl = mods.control_key();
        let shift = mods.shift_key();
        // alt는 macOS의 Command, 다른 OS의 Alt다. option은 macOS의 Option, 다른 OS의 Win·Super다.
        let (alt, option) = tasty_key_match::token_axes(mods);

        if !edit_judged && self.handle_webview_edit_shortcut(engine, key, mods) {
            return true;
        }

        if !edit_judged && self.handle_copy_shortcut(engine, key, mods) {
            return true;
        }

        if matches_any_binding(&engine.settings.keybindings.copy_link, key, mods)
            && self.run_copy_link(engine)
        {
            return true;
        }

        if self.handle_explorer_shortcut(engine, key, mods, edit_judged) {
            self.base.state.dirty = true;
            return true;
        }

        let kb = engine.settings.keybindings.clone();

        if self.handle_window_control_shortcuts(engine, key, mods, &kb) {
            return true;
        }

        if self.run_keybinding_shortcuts(engine, &kb, key, mods) {
            return true;
        }

        if self.try_dispatch_script_shortcut(engine, key, mods) {
            self.base.state.dirty = true;
            return true;
        }

        if Self::handle_numeric_switch_shortcuts(
            &mut self.state,
            engine,
            &kb,
            key,
            mods,
            ctrl,
            shift,
            alt,
            option,
        ) {
            if engine.workspaces().is_empty() {
                self.request_close();
            }
            self.base.state.dirty = true;
            return true;
        }

        if !edit_judged && self.handle_paste_shortcut(engine, key, mods) {
            return true;
        }

        if Self::handle_zoom_shortcut(&mut self.state, engine, key, mods) {
            self.base.state.dirty = true;
            return true;
        }

        false
    }
}
