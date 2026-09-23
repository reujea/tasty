//! 단축키 디스패치 — `handle_shortcut` 의 키 입력 → 액션 분기 + `dispatch_action_by_id`
//! 의 액션 ID 직접 호출 (Command Palette / 자동화 진입점).

use winit::keyboard::{Key, ModifiersState};

use crate::intent::{Intent, OpenPopupMode, UiIntent};
use crate::view::main::MainView;

use super::copy_paste::ExplorerAction;
use super::keybinding::PresetApplyScope;
use super::zoom::ZoomAction;

/// `&mut self` 가 필요해 match 뒤로 미루는 팔레트 액션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeferredPaletteAction {
    Copy,
    Cut,
    Paste,
}

use super::keybinding::CellGeometry;
use super::matches_any_binding;
use super::{focused_explorer_surface_id, focused_workspace_category, send_app_event};

impl MainView {
    /// Dispatch a keybinding action by its stable `field_id` (예: `"new_workspace"`).
    /// 단축키와 정확히 같은 효과를 낸다.
    ///
    /// ## 왜 평평한 문인가
    ///
    /// action id 를 **단일 진입점**으로 모으는 것이 이 함수의 설계다. 같은 액션을 부르는
    /// 자리가 여럿 생겨도 효과가 갈리지 않게 하려는 것이고, id 가
    /// `KeybindingSettings` 의 필드 이름 그대로라 설정·단축키·이 문이 한 어휘를 쓴다.
    ///
    /// ## 오늘 누가 들어오는가 — Command Palette 뿐이다
    ///
    /// **에이전트/IPC 경로는 없다.** 이 구분을 적어 두는 이유는, "단일 진입점이 있다" 와
    /// "그 진입점이 에이전트에게 열려 있다" 가 다른 물음인데 앞엣것만 보고 뒤엣것을
    /// 읽기 쉽기 때문이다. release 에서 에이전트는 팝업을 강제로 못 열므로(원칙 1),
    /// 팔레트를 통한 도달도 성립하지 않는다.
    ///
    /// 호출자 집합은 `crates/tasty-doc-guards/tests/`(액션 문 가드)가 붙든다 — 새 문이
    /// 생기면 그 가드가 먼저 반응하고, 그때 원칙 1·3 과 대조하는 것이 사람의 몫이다.
    ///
    /// Returns true if the action was recognized and dispatched. Unknown action_id는 false.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: action_id 문자열→액션 평면 match 디스패치 — 단축키와 1:1, arm 나열
    pub(crate) fn dispatch_action_by_id(&mut self, action_id: &str) -> bool {
        use crate::adapters::ui::popup::PopupScope;
        use crate::model::SplitDirection;

        let terminal_rect = self.compute_terminal_rect();
        let cell_w = self.base.gpu.cell_width();
        let cell_h = self.base.gpu.cell_height();
        // `state`/`engine` 을 가변 차용하기 **전에** 잡는다 — 아래 match 안에서는
        // `self.base` 를 다시 못 읽는다.
        let scale_factor = self.base.gpu.scale_factor();
        let proxy = self.proxy.clone();
        let proxy = &proxy;
        // copy_path 등 clipboard 쓰기는 self.clipboard 차용이 필요해 match(=state/engine
        // 차용) 종료 후 처리하도록 텍스트만 모아둔다. 토스트 스코프(sid)도 함께 모아둔다.
        let mut pending_copy_text: Option<String> = None;
        let mut pending_copy_scope: Option<u32> = None;
        // clipboard/selection 을 만지는 액션은 `&mut self` 메서드라 아래 match 안에서
        // 못 부른다(state/engine 을 이미 가변 차용했다). 무엇을 할지만 정해 두고 match
        // 뒤에서 실행한다 — `pending_copy_text` 와 같은 이유, 같은 형태다.
        let mut deferred: Option<DeferredPaletteAction> = None;
        let state = &mut self.state;
        let engine = &mut self.core_state;

        match action_id {
            "new_workspace" => {
                // 현재 활성 워크스페이스의 카테고리를 계승 (keybinding.rs
                // match_create_bindings 와 동일 정책 — Command Palette/자동화 진입점도
                // 마우스 경로와 동일하게 카테고리 인지형 생성이어야 한다).
                let category = focused_workspace_category(state, engine);
                state.dispatch_intent(
                    Intent::NewWorkspace {
                        kind: None,
                        params: serde_json::Value::Null,
                        category,
                    }
                    .from_user_shortcut("new_workspace"),
                );
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "new_tab" => {
                if let Err(e) = state.add_tab(engine) {
                    tracing::warn!("add_tab failed: {e}");
                }
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "split_pane_vertical" => {
                state.dispatch_intent(
                    Intent::SplitPane {
                        direction: SplitDirection::Vertical,
                    }
                    .from_user_shortcut("split_pane_vertical"),
                );
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "split_pane_horizontal" => {
                state.dispatch_intent(
                    Intent::SplitPane {
                        direction: SplitDirection::Horizontal,
                    }
                    .from_user_shortcut("split_pane_horizontal"),
                );
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "split_surface_vertical" => {
                state.dispatch_intent(
                    Intent::SplitSurface {
                        direction: SplitDirection::Vertical,
                    }
                    .from_user_shortcut("split_surface_vertical"),
                );
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "split_surface_horizontal" => {
                state.dispatch_intent(
                    Intent::SplitSurface {
                        direction: SplitDirection::Horizontal,
                    }
                    .from_user_shortcut("split_surface_horizontal"),
                );
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "toggle_settings" => {
                send_app_event(proxy, crate::AppEvent::OpenSettings);
            }
            "toggle_notifications" => {
                let will_open = !state.popups.is_open("notifications");
                state.dispatch_intent(
                    UiIntent::TogglePopup {
                        id: "notifications",
                        mode: OpenPopupMode::Default,
                    }
                    .from_user_shortcut("toggle_notifications"),
                );
                if will_open {
                    state.dispatch_intent(
                        crate::core::intent::DomainIntent::MarkAllNotificationsRead
                            .from_user_shortcut("toggle_notifications"),
                    );
                }
            }
            "fullscreen_stage_exit" => {
                // 키 경로는 0단계 무대 게이트(`view::main::keyboard`)가 직접 처리한다.
                // 여기 arm 이 있는 이유는 `dispatch_action_by_id` 가 field_id 로 액션을
                // 부르는 **일반 진입점**이기 때문 — 메뉴/CSD 등 다른 호출자가 생겨도
                // unknown action 경고로 떨어지지 않고 같은 수렴점을 탄다. 무대가 없으면
                // `close_fullscreen_stage` 가 false 를 반환하는 무해한 no-op 이다.
                state.close_fullscreen_stage();
            }
            "toggle_sidebar" => {
                state.sidebar_visible = !state.sidebar_visible;
            }
            "toggle_sidebar_collapse" => {
                state.sidebar_collapsed = !state.sidebar_collapsed;
            }
            "toggle_categories_collapsed" => {
                // Command Palette 경로. 카테고리 토글이 꺼져 있으면 무해한 no-op.
                if engine.settings.general.workspace_categories_enabled {
                    engine.toggle_all_categories_collapsed();
                    engine.mark_layout_dirty();
                }
            }
            "close_workspace" => {
                state.close_active_workspace(engine);
                if engine.workspaces.is_empty() {
                    self.request_close();
                } else {
                    crate::core::Core::resize_all_terminals(
                        self.state.tab_bar_height,
                        engine,
                        terminal_rect,
                        cell_w,
                        cell_h,
                        scale_factor,
                    );
                }
                return true;
            }
            "close_pane" => {
                if !state.close_active_pane(engine) {
                    state.close_active_workspace(engine);
                }
                if engine.workspaces.is_empty() {
                    self.request_close();
                } else {
                    crate::core::Core::resize_all_terminals(
                        self.state.tab_bar_height,
                        engine,
                        terminal_rect,
                        cell_w,
                        cell_h,
                        scale_factor,
                    );
                }
                return true;
            }
            "close_surface" => {
                let closed = state.close_active_surface(engine);
                if !closed && !state.close_active_pane(engine) {
                    state.close_active_workspace(engine);
                }
                if engine.workspaces.is_empty() {
                    self.request_close();
                } else {
                    crate::core::Core::resize_all_terminals(
                        self.state.tab_bar_height,
                        engine,
                        terminal_rect,
                        cell_w,
                        cell_h,
                        scale_factor,
                    );
                }
                return true;
            }
            "close_active" => {
                if !state.close_active_tab(engine) && !state.close_active_pane(engine) {
                    state.close_active_workspace(engine);
                }
                if engine.workspaces.is_empty() {
                    self.request_close();
                } else {
                    crate::core::Core::resize_all_terminals(
                        self.state.tab_bar_height,
                        engine,
                        terminal_rect,
                        cell_w,
                        cell_h,
                        scale_factor,
                    );
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
                crate::core::Core::resize_all_terminals(
                    state.tab_bar_height,
                    engine,
                    terminal_rect,
                    cell_w,
                    cell_h,
                    scale_factor,
                );
            }
            "quit" => send_app_event(proxy, crate::AppEvent::QuitRequested),
            "quit_immediate" => send_app_event(proxy, crate::AppEvent::Shutdown),
            "quit_minimize" => send_app_event(proxy, crate::AppEvent::Minimize),
            "new_window" => send_app_event(
                proxy,
                crate::AppEvent::CreateWindow(crate::app::event::WindowRequestOrigin::User, None),
            ),
            "find" => {
                // winit 경로는 검색창 비포커스(터미널 포커스) 상태에서만 도달한다.
                // 검색창 포커스 상태의 find 는 egui 경로(search_bar)가 처리한다.
                // 여기서는 항상 "검색창으로 포커스 이동".
                //
                // `keybinding.rs`의 kb.find 분기와 동일한 이유로 Terminal 포커스만 처리한다
                // — search_bar의 run_search는 find_terminal_by_id로만 동작해 다른 kind에서는
                // 항상 빈 0/0 오버레이가 된다. Command Palette/자동화로 이 액션 ID를 직접
                // 호출하는 이 경로도 원시 키 경로와 동일하게 가드해야 같은 버그가 재발하지
                // 않는다.
                if matches!(
                    state.focused_surface_type(engine),
                    crate::state::FocusedSurfaceType::Terminal
                ) {
                    if state.popups.is_open("search_bar") {
                        state.popups.set_focused("search_bar", true);
                    } else if let Some(sid) = state.focused_surface_id(engine) {
                        state.search.surface_id = sid;
                        state.dispatch_intent(
                            UiIntent::OpenPopup {
                                id: "search_bar",
                                mode: OpenPopupMode::AtTopOfScope(PopupScope::Surface(sid)),
                            }
                            .from_user_shortcut("find"),
                        );
                    }
                }
            }
            "open_markdown" => {
                // 새 탭 markdown 열기: surface_id 없이 file-open 팝업(plugin 이 새 탭 dispatch).
                state.enqueue_convert_input_popup(engine, "markdown", None);
            }
            "open_explorer" => Self::open_explorer_tab(state),
            "convert_surface" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    state.dialogs.convert_popup = Some(sid);
                    state.dialogs.convert_popup_selected = None;
                    state.dispatch_intent(
                        UiIntent::OpenPopup {
                            id: "convert_surface",
                            mode: OpenPopupMode::WithScope(PopupScope::Surface(sid)),
                        }
                        .from_user_shortcut("convert_surface"),
                    );
                }
            }
            "convert_to_markdown" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    // 제자리 markdown 변환: surface_id 를 실어 file-open 팝업(plugin navigate).
                    state.enqueue_convert_input_popup(engine, "markdown", Some(sid));
                }
            }
            "convert_to_explorer" => {
                if let Some(sid) = state.focused_surface_id(engine) {
                    state.dispatch_intent(
                        crate::intent::Intent::ConvertSurface {
                            surface_id: sid,
                            target: crate::intent::ConvertTarget::Kind {
                                cwd: None,
                                kind: "explorer".to_string(),
                                params: serde_json::json!({}),
                            },
                        }
                        .from_user_shortcut("convert_to_explorer"),
                    );
                }
            }
            "rename_tab" => {
                let pane_id = state.active_workspace(engine).focused_pane;
                if let Some(pane) = state
                    .active_workspace(engine)
                    .pane_layout()
                    .find_pane(pane_id)
                {
                    let tab_index = pane.active_tab;
                    if let Some(tab) = pane.tabs.get(tab_index) {
                        let current_name = tab.display_name();
                        let target = crate::state::RenameTarget::TabName { pane_id, tab_index };
                        let scope = target.popup_scope();
                        state.dialogs.rename = Some((target, current_name));
                        state.dispatch_intent(
                            UiIntent::OpenPopup {
                                id: "rename",
                                mode: OpenPopupMode::WithScope(scope),
                            }
                            .from_user_shortcut("rename_tab"),
                        );
                    }
                }
            }
            "rename_workspace" => {
                let ws_idx = state.active_workspace;
                if let Some(ws) = engine.workspaces.get(ws_idx) {
                    let target = crate::state::RenameTarget::WorkspaceName { ws_idx };
                    let scope = target.popup_scope();
                    state.dialogs.rename = Some((target, ws.name.clone()));
                    state.dispatch_intent(
                        UiIntent::OpenPopup {
                            id: "rename",
                            mode: OpenPopupMode::WithScope(scope),
                        }
                        .from_user_shortcut("rename_workspace"),
                    );
                }
            }
            "rename_workspace_subtitle" => {
                let ws_idx = state.active_workspace;
                if let Some(ws) = engine.workspaces.get(ws_idx) {
                    let target = crate::state::RenameTarget::WorkspaceSubtitle { ws_idx };
                    let scope = target.popup_scope();
                    state.dialogs.rename = Some((target, ws.subtitle.clone()));
                    state.dispatch_intent(
                        UiIntent::OpenPopup {
                            id: "rename",
                            mode: OpenPopupMode::WithScope(scope),
                        }
                        .from_user_shortcut("rename_workspace_subtitle"),
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
                // 단발 키 경로(`keybinding.rs` 의 `match_copy_rename_bindings`)와 **같은
                // 신호**를 쓴다 — 여기서 직접 모드에 들어가지 않는 이유는 진입 판정이
                // 포커스된 surface 가 터미널인지 등 다음 프레임의 상태를 보기 때문이다
                // (`view/main/vi_copy.rs` 의 `try_enter_vi_copy_mode`).
                // 팔레트는 팝업이 닫힌 뒤 drain 되므로 그 프레임이 곧 사용자가 기대하는
                // 포커스 상태다.
                state.dialogs.pending_enter_copy_mode = true;
            }
            "copy_path" => {
                if state.focused_surface_type(engine).is_kind("explorer")
                    && let Some(sid) = focused_explorer_surface_id(state, engine)
                {
                    // clipboard 쓰기는 self.clipboard 가 필요해 match 밖에서 처리.
                    pending_copy_text = state
                        .explorer_views
                        .get(sid)
                        .and_then(|v| v.selected_paths_text());
                    if pending_copy_text.is_some() {
                        pending_copy_scope = Some(sid);
                    }
                }
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
            // 윈도우 컨트롤 — CSD 캡션 버튼(P5)/Linux DE 버튼(P6)/macOS 네이티브
            // 신호등과 동일한 winit window 조작을 그대로 수행한다(단일 동작 경로).
            "toggle_dag_list" => Self::toggle_dag_list_popup(state),
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
                Self::open_preset_apply_popup(state, PresetApplyScope::Workspace);
            }
            "apply_tab_preset" => Self::open_preset_apply_popup(state, PresetApplyScope::Tab),
            "apply_pane_preset" => Self::open_preset_apply_popup(state, PresetApplyScope::Pane),
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
                // CSD close 버튼과 동일 라이프사이클(quit/close 라우팅)로 보낸다.
                send_app_event(proxy, crate::AppEvent::CloseWindow(self.base.winit.id()));
            }
            other => {
                tracing::warn!("dispatch_action_by_id: unknown action '{other}'");
                return false;
            }
        }
        // 키 경로의 **순서를 그대로** 따른다(`handle_shortcut`): 복사는 선택 텍스트가
        // 먼저이고 안 되면 explorer 파일 복사, 붙여넣기는 explorer 가 먼저이고 안 되면
        // 터미널 붙여넣기다. 순서를 바꾸면 같은 이름의 명령이 키로 누를 때와 팔레트에서
        // 고를 때 다르게 동작한다.
        match deferred {
            Some(DeferredPaletteAction::Copy) => {
                if !self.run_copy() {
                    self.run_explorer_action(ExplorerAction::CopyFiles);
                }
            }
            Some(DeferredPaletteAction::Cut) => {
                self.run_explorer_action(ExplorerAction::CutFiles);
            }
            Some(DeferredPaletteAction::Paste) => {
                if !self.run_explorer_action(ExplorerAction::PasteFiles) {
                    self.run_paste();
                }
            }
            None => {}
        }
        if let Some(text) = pending_copy_text
            && let Some(cb) = self.clipboard.as_mut()
        {
            cb.set_text(&text);
            if let Some(sid) = pending_copy_scope {
                self.state.toasts.push_info(
                    crate::i18n::t("toast.copied_path"),
                    crate::adapters::ui::ToastScope::Surface(sid),
                );
            }
        }
        self.base.dirty = true;
        true
    }

    /// 윈도우 컨트롤 단축키(minimize/maximize/close)를 현재 window 에 적용한다.
    /// 매칭 시 [`dispatch_action_by_id`](Self::dispatch_action_by_id) 로 위임해 CSD 버튼과
    /// 동일 경로를 탄다. macOS 는 NSMenu 가 처리하므로 비활성(`cfg(not(macos))`).
    #[cfg(not(target_os = "macos"))]
    fn handle_window_control_shortcuts(
        &mut self,
        key: &Key,
        mods: ModifiersState,
        kb: &crate::settings::KeybindingSettings,
    ) -> bool {
        for (binding, action_id) in [
            (&kb.minimize_window, "minimize_window"),
            (&kb.maximize_window, "maximize_window"),
            (&kb.close_window, "close_window"),
        ] {
            if matches_any_binding(binding, key, mods) {
                return self.dispatch_action_by_id(action_id);
            }
        }
        false
    }

    /// 사용자 스크립트 단축키 매칭 → Lua 워커 실행 요청 (ADR-0031).
    ///
    /// combo 가 매칭되면 등록 스크립트를 조회해 소스를 읽고 `AppEvent::RunLuaScript` 로
    /// App(lua_engine 소유)에 넘긴다. 매칭됐으나 스크립트/파일이 없으면 이벤트는 소비하되
    /// 실행하지 않는다(다른 핸들러로 새지 않게). release 는 사용자 키 입력에서만 이 경로를 탄다.
    fn try_dispatch_script_shortcut(&mut self, key: &Key, mods: ModifiersState) -> bool {
        let kb = &self.core_state.settings.keybindings;
        let Some(script_id) = kb
            .script_bindings
            .iter()
            .find(|b| matches_any_binding(std::slice::from_ref(&b.combo), key, mods))
            .map(|b| b.script_id.clone())
        else {
            return false;
        };
        let Some(entry) = self.core_state.settings.scripts.get(&script_id) else {
            tracing::warn!(
                target: "tasty_lua",
                "script shortcut matched but script '{script_id}' not registered — ignoring"
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
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "tasty_lua", "script read failed {}: {e}", path.display());
                return true;
            }
        };
        // TOFU 게이트(ADR-0031): 등록 해시와 현재 파일 해시 비교. 같으면 조용히 실행,
        // 다르면 실행 보류 + 변경 확인 팝업(수동 발화 = popup).
        let current_hash = tasty_settings::hash_bytes(source.as_bytes());
        if current_hash == stored_hash {
            send_app_event(&self.proxy, crate::AppEvent::RunLuaScript { source, name });
        } else {
            self.state.dialogs.pending_script_confirm = Some(crate::state::PendingScriptConfirm {
                script_id,
                name,
                source,
                new_hash: current_hash,
                result: None,
            });
            self.state.dispatch_intent(
                crate::intent::UiIntent::OpenPopup {
                    id: "script_changed_confirm",
                    mode: crate::intent::OpenPopupMode::CenteredFocused,
                }
                .from_user_menu("script_tofu_gate"),
            );
        }
        true
    }

    /// Handle keyboard shortcuts. Returns true if the event was consumed by a shortcut.
    pub(crate) fn handle_shortcut(&mut self, key: &Key, mods: ModifiersState) -> bool {
        let ctrl = mods.control_key();
        let shift = mods.shift_key();
        // `alt` = "alt" 토큰(macOS 물리 ⌘=super, 그 외 Alt). `option` = "option" 토큰
        // (macOS 물리 ⌥, 그 외 항상 false). switch_target_for 의 정규화 규약과 동일.
        #[cfg(target_os = "macos")]
        let (alt, option) = (mods.super_key(), mods.alt_key());
        #[cfg(not(target_os = "macos"))]
        let (alt, option) = (mods.alt_key(), false);

        let terminal_rect = self.compute_terminal_rect();
        let cell_w = self.base.gpu.cell_width();
        let cell_h = self.base.gpu.cell_height();

        // Clipboard copy (needs &self before state borrow)
        if self.handle_copy_shortcut(key, mods) {
            return true;
        }

        // Explorer 선택/경로복사 (clipboard 차용 필요 → keybinding free-fn 이전에 처리)
        if self.handle_explorer_shortcut(key, mods) {
            self.base.dirty = true;
            return true;
        }

        let kb = self.core_state.settings.keybindings.clone();

        // 윈도우 컨트롤(minimize/maximize/close)은 현재 winit window 를 직접 조작한다.
        // macOS 는 이 액션을 NSMenu(performMiniaturize:/performZoom:/performClose:) 의
        // key equivalent 로 처리하므로(AppKit 가 키를 소비) winit 경로를 끈다 — 이중 처리 방지.
        #[cfg(not(target_os = "macos"))]
        if self.handle_window_control_shortcuts(key, mods, &kb) {
            return true;
        }

        // Configurable keybinding shortcuts
        let cells = CellGeometry {
            w: crate::model::PhysicalPx(cell_w),
            h: crate::model::PhysicalPx(cell_h),
            scale_factor: self.base.gpu.scale_factor(),
        };
        if Self::handle_keybinding_shortcuts(
            &mut self.state,
            &mut self.core_state,
            &kb,
            key,
            mods,
            terminal_rect,
            cells,
            &self.proxy,
        ) {
            if self.core_state.workspaces.is_empty() {
                self.request_close();
            }
            self.base.dirty = true;
            return true;
        }

        // 사용자 스크립트 단축키 (ADR-0031) — 사용자 키 입력 경로에서만 발화.
        if self.try_dispatch_script_shortcut(key, mods) {
            self.base.dirty = true;
            return true;
        }

        // Numeric tab/workspace switching (Ctrl+1..9 / Alt+1..9)
        if Self::handle_numeric_switch_shortcuts(
            &mut self.state,
            &mut self.core_state,
            &kb,
            key,
            mods,
            ctrl,
            shift,
            alt,
            option,
        ) {
            if self.core_state.workspaces.is_empty() {
                self.request_close();
            }
            self.base.dirty = true;
            return true;
        }

        // Clipboard paste
        if self.handle_paste_shortcut(key, mods) {
            return true;
        }

        // Zoom
        if Self::handle_zoom_shortcut(&mut self.state, &mut self.core_state, key, mods) {
            self.base.dirty = true;
            return true;
        }

        false
    }
}
