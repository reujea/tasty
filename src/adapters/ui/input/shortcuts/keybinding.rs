//! 설정 가능한 키바인딩 (사이드바/탭/팝업 등) 의 매칭/분기.
//!
//! 본 fn 은 `handle_shortcut` 의 메인 분기에서 호출된다. 키 + 모디파이어와
//! `KeybindingSettings` 의 각 액션 바인딩 목록을 순서대로 비교하여 첫 매칭에서
//! 액션을 수행한다.
//!
//! 분기 본체는 카테고리별 그룹 매처(`match_*_bindings`)로 나뉘어 있으나 매칭
//! 순서는 원본 나열 순서를 그대로 보존한다 — 그룹은 연속 블록(contiguous run)
//! 단위이며, 본체는 그룹을 원본 순서대로 순차 호출한다(먼저 매칭되는 binding 이
//! 이긴다는 우선순위 규칙 불변).

use winit::keyboard::{Key, ModifiersState};

use crate::intent::{Intent, OpenPopupMode, UiIntent};
use crate::model::SplitDirection;
use crate::view::main::MainView;

use super::{
    focused_explorer_surface_id, focused_workspace_category, matches_any_binding, send_app_event,
};

/// [`MainView::handle_keybinding_shortcuts`] 및 하위 `match_*_bindings` 가 공유하는
/// 셀 크기(physical px) — `terminal_rect` 와 같은 typed-length 계열.
///
/// `scale_factor` 를 함께 싣는 이유: 이 셋은 **같은 프레임의 같은 창**에서 나오고
/// 늘 함께 쓰인다(레이아웃 재계산은 셋 다 필요하다). 따로 흘리면 서로 다른
/// 프레임의 값이 섞일 자리가 생긴다.
#[derive(Clone, Copy)]
pub(super) struct CellGeometry {
    pub w: crate::model::PhysicalPx,
    pub h: crate::model::PhysicalPx,
    /// 논리↔물리 변환 배율. pane 보더가 논리라 레이아웃 계산에 필요하다.
    pub scale_factor: f32,
}

/// 레이아웃 프리셋 적용 picker 의 대상 스코프.
///
/// **단축키 프리셋(tasty/mac/…)과 다른 것이다** — 이쪽은 workspace/tab/pane 레이아웃을
/// 적용하는 picker 다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetApplyScope {
    Workspace,
    Tab,
    Pane,
}

impl MainView {
    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    pub(super) fn handle_keybinding_shortcuts(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        // 그룹 호출 순서 = 원본 블록 나열 순서. 순서 변경 금지(단축키 우선순위 영향).
        if Self::match_create_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_split_bindings(state, engine, kb, key, mods, terminal_rect, cells) {
            return true;
        }
        if Self::match_panel_bindings(state, engine, kb, key, mods, proxy) {
            return true;
        }
        if Self::match_close_bindings(state, engine, kb, key, mods, terminal_rect, cells) {
            return true;
        }
        if Self::match_focus_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_sidebar_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_restore_quit_bindings(
            state,
            engine,
            kb,
            key,
            mods,
            terminal_rect,
            cells,
            proxy,
        ) {
            return true;
        }
        if Self::match_convert_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_capture_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_window_tab_bindings(
            state,
            engine,
            kb,
            key,
            mods,
            terminal_rect,
            cells,
            proxy,
        ) {
            return true;
        }
        if Self::match_rename_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_explorer_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_preset_bindings(state, kb, key, mods) {
            return true;
        }
        if Self::match_tools_menu_bindings(state, engine, kb, key, mods) {
            return true;
        }
        if Self::match_copy_rename_bindings(state, engine, kb, key, mods) {
            return true;
        }
        false
    }

    /// 생성 계열: new_workspace / new_tab.
    ///
    /// `pub(super)` — `focused_workspace_category` 계승 여부를 실제 dispatch 경로로
    /// 검증하는 `shortcuts::tests` 단위 테스트가 직접 호출한다(zoom/numeric 과 동일한
    /// 테스트 가시성 패턴).
    pub(super) fn match_create_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.new_workspace, key, mods) {
            // 현재 활성 워크스페이스의 카테고리를 계승 — 마우스 경로(레일 팝업/카테고리
            // 메뉴)와 동일하게 카테고리 인지형 생성으로 맞춘다(항상 normal 로 고정되던
            // 결함 수정).
            let category = focused_workspace_category(state, engine);
            state.dispatch_intent(
                Intent::NewWorkspace {
                    kind: None,
                    params: serde_json::Value::Null,
                    category,
                }
                .from_user_shortcut("new_workspace"),
            );
            return true;
        }
        if matches_any_binding(&kb.new_tab, key, mods) {
            if let Err(e) = state.add_tab(engine) {
                tracing::warn!("add_tab failed: {e}");
            }
            return true;
        }
        false
    }

    /// 분할 계열: split_pane_{vertical,horizontal} / split_surface_{vertical,horizontal}.
    fn match_split_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
    ) -> bool {
        if matches_any_binding(&kb.split_pane_vertical, key, mods) {
            state.dispatch_intent(
                Intent::SplitPane {
                    direction: SplitDirection::Vertical,
                }
                .from_user_shortcut("split_pane_vertical"),
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
            state.dispatch_intent(
                Intent::SplitPane {
                    direction: SplitDirection::Horizontal,
                }
                .from_user_shortcut("split_pane_horizontal"),
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
            state.dispatch_intent(
                Intent::SplitSurface {
                    direction: SplitDirection::Vertical,
                }
                .from_user_shortcut("split_surface_vertical"),
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
            state.dispatch_intent(
                Intent::SplitSurface {
                    direction: SplitDirection::Horizontal,
                }
                .from_user_shortcut("split_surface_horizontal"),
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

    /// 패널/오버레이 토글 계열: toggle_settings / toggle_notifications / find.
    fn match_panel_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
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
            return true;
        }
        if matches_any_binding(&kb.toggle_dag_list, key, mods) {
            Self::toggle_dag_list_popup(state);
            return true;
        }
        if matches_any_binding(&kb.find, key, mods) {
            // 이 winit 경로는 검색창이 포커스되지 않은 상태(터미널 포커스)에서만 도달한다
            // — 검색창 포커스 상태의 find 는 overlay 게이트에 막혀 egui 경로(search_bar)
            //   가 처리한다. 따라서 여기서는 항상 "검색창으로 포커스 이동"이다.
            //
            // `search_bar` popup(run_search)은 `find_terminal_by_id`로만 동작해 터미널이
            // 아닌 focused surface(예: markdown webview)에서는 항상 0/0으로 뜨는 빈 오버레이가
            // 된다 — terminal-search 기획(`docs/features/terminal-search/index.md`)도 애초에
            // "터미널 포커스 + find"만 서술한다. 따라서 focused surface가 Terminal일 때만
            // 이 popup을 연다; 그 외 kind는 자기 자신의 find-in-page(있다면, 예: markdown
            // plugin의 트러스트 JS 문서-내 검색)로 넘긴다 — 여기서 소비하지 않고 false 반환.
            //
            // webview 키 포워딩(`docs/adr/0102-webview-key-forwarding.md`)이 생긴 뒤에도
            // 이 분기는 유지한다. 포워딩은 `find` 를 **페이지 예약 액션**으로 두어 애초에
            // 가져가지 않으므로 webview 포커스 시에는 여기까지 오지도 않지만, webview 가
            // 아닌 비-터미널 kind(explorer 등)가 포커스일 때의 빈 0/0 오버레이는 여전히
            // 이 게이트만 막는다 — 두 장치는 대상이 겹치지 않는다.
            if !matches!(
                state.focused_surface_type(engine),
                crate::state::FocusedSurfaceType::Terminal
            ) {
                return false;
            }
            if state.popups.is_open("search_bar") {
                // 이미 떠 있으면 닫지 않고 포커스만 검색창으로 옮긴다.
                state.popups.set_focused("search_bar", true);
            } else if let Some(sid) = state.focused_surface_id(engine) {
                state.search.surface_id = sid;
                state.dispatch_intent(
                    UiIntent::OpenPopup {
                        id: "search_bar",
                        mode: OpenPopupMode::AtTopOfScope(
                            crate::adapters::ui::popup::PopupScope::Surface(sid),
                        ),
                    }
                    .from_user_shortcut("find_open"),
                );
            }
            return true;
        }
        false
    }

    /// 닫기 계열: close_workspace / close_pane / close_surface.
    fn match_close_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
    ) -> bool {
        if matches_any_binding(&kb.close_workspace, key, mods) {
            state.close_active_workspace(engine);
            if !engine.workspaces.is_empty() {
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
            if !state.close_active_pane(engine) {
                state.close_active_workspace(engine);
            }
            if !engine.workspaces.is_empty() {
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
            let closed = state.close_active_surface(engine);
            if !closed && !state.close_active_pane(engine) {
                state.close_active_workspace(engine);
            }
            if !engine.workspaces.is_empty() {
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

    /// 포커스 이동 계열: focus_{pane,surface}_{next,prev}.
    fn match_focus_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.focus_pane_next, key, mods) {
            state.move_pane_focus_forward(engine);
            return true;
        }
        if matches_any_binding(&kb.focus_pane_prev, key, mods) {
            state.move_pane_focus_backward(engine);
            return true;
        }
        if matches_any_binding(&kb.focus_surface_next, key, mods) {
            state.move_surface_focus_forward(engine);
            return true;
        }
        if matches_any_binding(&kb.focus_surface_prev, key, mods) {
            state.move_surface_focus_backward(engine);
            return true;
        }
        false
    }

    /// 사이드바 계열: toggle_sidebar / toggle_sidebar_collapse /
    /// toggle_categories_collapsed.
    fn match_sidebar_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
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
        // 카테고리 토글이 켜져 있을 때만 매칭·consume — 꺼져 있으면 키를 다른 binding 으로
        // 흘려보낸다(표시=동작: 비활성 기능은 단축키도 비활성).
        if engine.settings.general.workspace_categories_enabled
            && matches_any_binding(&kb.toggle_categories_collapsed, key, mods)
        {
            engine.toggle_all_categories_collapsed();
            engine.mark_layout_dirty();
            return true;
        }
        false
    }

    /// 복구/종료 계열: restore_closed / quit_immediate / quit_minimize / quit.
    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_restore_quit_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        terminal_rect: crate::model::PhysicalRect,
        cells: CellGeometry,
        proxy: &winit::event_loop::EventLoopProxy<crate::AppEvent>,
    ) -> bool {
        if matches_any_binding(&kb.restore_closed, key, mods) {
            state.dispatch_intent(
                crate::intent::Intent::RestoreClosedItem.from_user_shortcut("restore_closed"),
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

    /// 변환 계열: open_markdown / open_explorer / convert_surface /
    /// convert_to_markdown / convert_to_explorer.
    fn match_convert_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.open_markdown, key, mods) {
            // 새 탭으로 markdown 파일 열기 — surface_id 없이 file-open 팝업을 연다
            // (plugin 이 file_handler.dispatch 로 새 탭). host 는 kind 이름을 몰라도
            // registry `convert_input_popup` 데이터로 그 kind plugin 팝업을 연다.
            state.enqueue_convert_input_popup(engine, "markdown", None);
            return true;
        }
        if matches_any_binding(&kb.open_explorer, key, mods) {
            Self::open_explorer_tab(state);
            return true;
        }
        if matches_any_binding(&kb.convert_surface, key, mods) {
            if let Some(sid) = state.focused_surface_id(engine) {
                state.dialogs.convert_popup = Some(sid);
                state.dialogs.convert_popup_selected = None;
                state.dispatch_intent(
                    UiIntent::OpenPopup {
                        id: "convert_surface",
                        mode: OpenPopupMode::WithScope(
                            crate::adapters::ui::popup::PopupScope::Surface(sid),
                        ),
                    }
                    .from_user_shortcut("convert_surface"),
                );
            }
            return true;
        }
        if matches_any_binding(&kb.convert_to_markdown, key, mods) {
            if let Some(sid) = state.focused_surface_id(engine) {
                // 포커스 surface 를 제자리 markdown 변환 — surface_id 를 실어 file-open
                // 팝업을 연다(plugin 이 markdown.navigate 로 제자리 변환).
                state.enqueue_convert_input_popup(engine, "markdown", Some(sid));
            }
            return true;
        }
        if matches_any_binding(&kb.convert_to_explorer, key, mods) {
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
            return true;
        }
        false
    }

    /// 스크린샷→클립보드: `screenshot_to_clipboard`. 포커스된 surface 기준으로
    /// 로컬/원격(mirror) 을 **여기서** 판별해 `engine.pending_screenshot_captures`
    /// 에 push 만 한다 — 실제 OS 캡처(블로킹)는 `App::poll_screenshot_captures` 가
    /// 백그라운드 스레드에서 수행(메인 루프 무블록). 판별을 트리거 시점에 끝내 두는
    /// 이유: 캡처가 끝나기 전에 포커스가 바뀌어도 판정이 흔들리지 않게.
    fn match_capture_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.screenshot_to_clipboard, key, mods) {
            Self::queue_screenshot_to_clipboard(state, engine);
            return true;
        }
        false
    }

    /// 윈도우/탭 계열: new_window / close_active / next_tab / prev_tab.
    #[allow(clippy::too_many_arguments)] // reason: keybinding dispatch context
    fn match_window_tab_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
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
            if !state.close_active_tab(engine) && !state.close_active_pane(engine) {
                state.close_active_workspace(engine);
            }
            if !engine.workspaces.is_empty() {
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
            state.next_tab_in_pane(engine);
            return true;
        }
        if matches_any_binding(&kb.prev_tab, key, mods) {
            state.prev_tab_in_pane(engine);
            return true;
        }
        false
    }

    /// 이름 변경 계열: rename_tab / rename_workspace.
    fn match_rename_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.rename_tab, key, mods) {
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
            return true;
        }
        if matches_any_binding(&kb.rename_workspace, key, mods) {
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
            return true;
        }
        false
    }

    /// 탐색기 계열(탐색기 포커스일 때만): explorer_refresh / explorer_go_up.
    fn match_explorer_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.explorer_refresh, key, mods)
            && state.focused_surface_type(engine).is_kind("explorer")
        {
            if let Some(sid) = focused_explorer_surface_id(state, engine) {
                crate::adapters::ui::egui_panels::apply_explorer_action(
                    state,
                    engine,
                    sid,
                    crate::explorer_ui::ExplorerAction::Refresh,
                );
            }
            return true;
        }
        if matches_any_binding(&kb.explorer_go_up, key, mods)
            && state.focused_surface_type(engine).is_kind("explorer")
        {
            if let Some(sid) = focused_explorer_surface_id(state, engine) {
                crate::adapters::ui::egui_panels::apply_explorer_action(
                    state,
                    engine,
                    sid,
                    crate::explorer_ui::ExplorerAction::GoUp,
                );
                // 경로 변경은 ExplorerView 가 다음 draw 에서 자동 감지해 reload 한다
                // (toolbar GoUp 버튼과 동일 경로).
            }
            return true;
        }
        false
    }

    /// 커맨드 팔레트/프리셋 계열: toggle_command_palette / apply_{workspace,tab,pane}_preset.
    fn match_preset_bindings(
        state: &mut crate::state::AppState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.toggle_command_palette, key, mods) {
            state.dispatch_intent(
                UiIntent::TogglePopup {
                    id: crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID,
                    mode: OpenPopupMode::CenteredFocused,
                }
                .from_user_shortcut("toggle_command_palette"),
            );
            return true;
        }
        if matches_any_binding(&kb.apply_workspace_preset, key, mods) {
            Self::open_preset_apply_popup(state, PresetApplyScope::Workspace);
            return true;
        }
        if matches_any_binding(&kb.apply_tab_preset, key, mods) {
            Self::open_preset_apply_popup(state, PresetApplyScope::Tab);
            return true;
        }
        if matches_any_binding(&kb.apply_pane_preset, key, mods) {
            Self::open_preset_apply_popup(state, PresetApplyScope::Pane);
            return true;
        }
        false
    }

    /// 복사 모드/워크스페이스 부제 이름변경 계열: enter_copy_mode /
    /// rename_workspace_subtitle.
    fn match_copy_rename_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if matches_any_binding(&kb.enter_copy_mode, key, mods) {
            state.dialogs.pending_enter_copy_mode = true;
            return true;
        }
        if matches_any_binding(&kb.rename_workspace_subtitle, key, mods) {
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
            return true;
        }
        false
    }

    /// 사이드바 "도구" 메뉴의 빌트인 항목 다섯.
    ///
    /// 네 프리셋 기본값이 전부 비어 있으므로 사용자가 지정하기 전에는 아무것도 안 걸린다
    /// — 그 상태에서 이 함수는 다섯 번의 빈 슬라이스 비교로 끝난다.
    fn match_tools_menu_bindings(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
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
            Self::open_file_picker_tool(state, engine);
            return true;
        }
        false
    }

    /// 도구 메뉴의 단순 popup 항목 열기 — 단축키·명령 팔레트가 공유한다.
    ///
    /// 메뉴 클릭 경로(`adapters/ui/tools_menu.rs`)와 **같은 mode** 를 쓴다
    /// (`CenteredFocused`). 모드가 갈리면 같은 항목이 메뉴로 열 때와 키로 열 때 다른
    /// 자리에 뜨고 포커스도 달라진다.
    pub(crate) fn open_tool_popup(
        state: &mut crate::state::AppState,
        popup_id: &'static str,
        action: &'static str,
    ) {
        state.dispatch_intent(
            UiIntent::OpenPopup {
                id: popup_id,
                mode: OpenPopupMode::CenteredFocused,
            }
            .from_user_shortcut(action),
        );
    }

    /// Preset 윈도우 열기 — popup 이 아니라 별도 winit 윈도우라 분기가 다르다.
    pub(crate) fn open_preset_window(state: &mut crate::state::AppState) {
        state.dialogs.pending_open_preset_window = true;
    }

    /// 파일 피커 열기 — 여는 *전* 활성 workspace 의 mirror 여부로 로컬/원격을 판별해
    /// `state.dialogs.file_picker` 를 채워야 해서 단순 popup 열기와 분기가 다르다.
    /// 그래서 메뉴와 같은 함수를 부른다(popup id 만 발화하면 동작이 갈린다).
    pub(crate) fn open_file_picker_tool(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
    ) {
        use crate::adapters::ui::popup::file_picker;
        let start =
            file_picker::FilePickerStart::from_surface(engine, state.focused_surface_id(engine));
        file_picker::open(state, engine, None, Vec::new(), start);
    }

    /// 새 탭으로 탐색기 열기 — 단발 키·명령 팔레트·double-tap 이 공유한다.
    ///
    /// `open_markdown` 이 file-open 팝업을 여는 것과 달리 팝업이 없다. markdown 은 파일
    /// 하나를 골라야 하지만 탐색기는 디렉토리를 **자기가** 정한다 — 이 kind 는
    /// `convert_input_popup` 이 없고(host builtin), 경로 미지정이면 홈에서 연다
    /// (`core/surface_registry/builtins.rs` 의 `resolve_root`). `Intent::NewTab` 은 CLI 의
    /// `new tab --type explorer` 와 같은 도메인 인텐트(`CreateTab`)를 쓰되 선택은 다르다 —
    /// 단축키는 새 탭을 선택하고, 에이전트(CLI/IPC)는 선택하지 않는다(ADR-0502).
    pub(crate) fn open_explorer_tab(state: &mut crate::state::AppState) {
        state.dispatch_intent(
            crate::intent::Intent::NewTab {
                kind: Some("explorer".to_string()),
                params: serde_json::json!({}),
            }
            .from_user_shortcut("open_explorer"),
        );
    }

    /// DAG 목록 popup 토글 — 키 경로와 명령 팔레트가 공유한다.
    ///
    /// 스코프는 정의가 아니라 **여는 시점**의 활성 workspace 로 정해진다 — 이 창은 그
    /// workspace 를 벗어나면 숨고 돌아오면 다시 뜬다. 팔레트는 팝업이 닫힌 뒤에 drain
    /// 되므로 그 프레임의 활성 workspace 가 사용자가 보고 있던 것이다.
    pub(crate) fn toggle_dag_list_popup(state: &mut crate::state::AppState) {
        state.dispatch_intent(
            UiIntent::TogglePopup {
                id: crate::adapters::ui::popup::dag_list::DAG_LIST_POPUP_ID,
                mode: OpenPopupMode::WithScope(crate::adapters::ui::popup::PopupScope::Workspace(
                    state.active_workspace,
                )),
            }
            .from_user_shortcut("toggle_dag_list"),
        );
    }

    /// 스크린샷 캡처 예약 — 키 경로와 명령 팔레트가 공유한다.
    pub(crate) fn queue_screenshot_to_clipboard(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
    ) {
        let mirror_ws_id = state.focused_surface_id(engine).and_then(|sid| {
            let (idx, _pane_id) = engine.find_workspace_index_for_surface(sid)?;
            let ws = engine.workspaces.get(idx)?;
            ws.mirror.then_some(ws.id)
        });
        engine.pending_screenshot_captures.push(mirror_ws_id);
    }

    /// 레이아웃 프리셋 적용 picker 열기 — 키 경로와 명령 팔레트가 공유한다.
    pub(crate) fn open_preset_apply_popup(
        state: &mut crate::state::AppState,
        scope: PresetApplyScope,
    ) {
        use crate::adapters::ui::popup::preset_apply;
        let (id, action) = match scope {
            PresetApplyScope::Workspace => (
                preset_apply::APPLY_WORKSPACE_POPUP_ID,
                "apply_workspace_preset",
            ),
            PresetApplyScope::Tab => (preset_apply::APPLY_TAB_POPUP_ID, "apply_tab_preset"),
            PresetApplyScope::Pane => (preset_apply::APPLY_PANE_POPUP_ID, "apply_pane_preset"),
        };
        state.dialogs.preset_picker_selected = None;
        state.dispatch_intent(
            UiIntent::OpenPopup {
                id,
                mode: OpenPopupMode::CenteredFocused,
            }
            .from_user_shortcut(action),
        );
    }
}
