//! 팝업 Intent 처리. 위치는 호출자가 고른 OpenPopupMode를 따른다. 사용자 조작이 아닌
//! 요청으로 연 팝업은 키보드 포커스를 갖지 않는 passive 상태로 연다.
//! 정책: docs/design/flows/action-dispatch.md, docs/design/systems/popup.md.

#[cfg(feature = "gui")]
use super::DispatchedIntent;
#[cfg(feature = "gui")]
use super::{Intent, OpenPopupMode, UiIntent};
#[cfg(feature = "gui")]
use crate::state::RequestContext;

/// 헤드리스에서는 팝업을 표시할 수 없어 요청을 무시한다.
#[cfg(feature = "gui")]
pub fn handle(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    intent: &DispatchedIntent,
) {
    #[cfg(feature = "gui")]
    {
        let Intent::Ui(ui) = &intent.body else {
            return;
        };
        match ui {
            UiIntent::SetCategoryCollapsed { id, collapsed } => {
                if intent.origin.is_user() && engine.categories().iter().any(|c| c.id == *id) {
                    if *collapsed {
                        state.navigation.collapsed_categories.insert(*id);
                    } else {
                        state.navigation.collapsed_categories.remove(id);
                    }
                    engine.mark_layout_dirty();
                }
            }
            UiIntent::ToggleCategoryCollapsed { id } => {
                if intent.origin.is_user() && engine.categories().iter().any(|c| c.id == *id) {
                    if !state.navigation.collapsed_categories.remove(id) {
                        state.navigation.collapsed_categories.insert(*id);
                    }
                    engine.mark_layout_dirty();
                }
            }
            UiIntent::ToggleAllCategoriesCollapsed => {
                if intent.origin.is_user() {
                    let collapse = engine
                        .categories()
                        .iter()
                        .any(|c| !state.navigation.collapsed_categories.contains(&c.id));
                    state.navigation.collapsed_categories.clear();
                    if collapse {
                        state
                            .navigation
                            .collapsed_categories
                            .extend(engine.categories().iter().map(|c| c.id));
                    }
                    engine.mark_layout_dirty();
                }
            }

            UiIntent::OpenPopup { id, mode } => open(state, id, mode, intent.origin.is_user()),
            UiIntent::ClosePopup { id } => state.popups.close(id),
            UiIntent::FocusPassivePopup { id } => {
                if intent.origin.is_user() {
                    state.popups.focus_passive(id);
                }
            }
            UiIntent::TogglePopup { id, mode } => {
                if state.popups.is_open(id) {
                    state.popups.close(id);
                } else {
                    open(state, id, mode, intent.origin.is_user());
                }
            }
            UiIntent::RequestSurfaceRefresh {
                target,
                kind,
                trigger,
            } => {
                if intent.origin.is_user() {
                    enqueue_refresh(engine, target, kind, *trigger);
                }
            }
        }
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = (state, engine, intent); // reason: 헤드리스에서는 팝업 요청을 처리하지 않는다.
    }
}

/// 키를 누른 뒤 대상이 닫혔거나 다른 kind 로 바뀌었거나 다시 만들어졌으면 신호를 내지 않는다.
#[cfg(feature = "gui")]
fn enqueue_refresh(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    target: &crate::runtime::surface_binding::SurfaceBinding,
    kind: &str,
    trigger: super::RefreshTrigger,
) {
    if !crate::core::host_event::refresh_target_current(&engine.as_ref(), target, kind) {
        tracing::debug!(
            "refresh request for surface {} dropped: the target changed",
            target.surface_id()
        );
        return;
    }
    tracing::debug!(
        target: "tasty::refresh",
        "refresh request for surface {} ({kind}) accepted",
        target.surface_id()
    );
    engine.enqueue_host_event(crate::state::PendingHostEvent::SurfaceRefreshRequested {
        target: target.clone(),
        kind: kind.to_string(),
        trigger: match trigger {
            super::RefreshTrigger::Shortcut => "shortcut",
            super::RefreshTrigger::Palette => "palette",
        },
    });
}

#[cfg(feature = "gui")]
fn open(state: &mut RequestContext, id: &'static str, mode: &OpenPopupMode, by_user: bool) {
    if state.popups.is_open(id) {
        return;
    }
    if id == crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID {
        crate::adapters::ui::tutorial::open_catalog(state);
    }
    if id == crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID {
        state
            .tutorial
            .observe(crate::adapters::ui::tutorial::PracticeEvent::OpenPalette);
    }
    match mode {
        OpenPopupMode::Default => state.popups.open(id),
        OpenPopupMode::CenteredFocused => state.popups.open_centered_focused(id),
        OpenPopupMode::WithScope(scope) => state.popups.open_with_scope(id, scope.clone()),
        OpenPopupMode::AtTopOfScope(scope) => state.popups.open_at_top_of_scope(id, scope.clone()),
        OpenPopupMode::AtFocused(pos) => state.popups.open_at_focused(id, *pos),
    }
    // 에이전트·플러그인·시스템 요청이 사용자의 키 입력을 가로채지 않게 한다.
    if !by_user {
        state.popups.mark_passive(id);
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;
    use crate::adapters::ui::popup::{PopupScope, PopupState};

    fn make_state() -> (
        RequestContext,
        crate::runtime::engine_session::EngineSession,
    ) {
        let (mut state, engine_session) = crate::state::tests::test_state();
        state.popups.register(PopupState::new(
            "test_popup",
            "Test".to_string(),
            egui::vec2(200.0, 100.0),
        ));
        (state, engine_session)
    }

    fn dispatched_open(id: &'static str, mode: OpenPopupMode) -> DispatchedIntent {
        UiIntent::OpenPopup { id, mode }.from_user_shortcut("test")
    }

    fn dispatched_close(id: &'static str) -> DispatchedIntent {
        UiIntent::ClosePopup { id }.from_user_shortcut("test")
    }

    #[test]
    fn second_open_intent_for_same_id_is_deduped() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::Default),
        );
        assert!(state.popups.is_open("test_popup"));
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::CenteredFocused),
        );
        assert!(state.popups.is_open("test_popup"));
    }

    #[test]
    fn close_intent_closes_popup() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::Default),
        );
        assert!(state.popups.is_open("test_popup"));
        handle(&mut state, &mut engine, &dispatched_close("test_popup"));
        assert!(!state.popups.is_open("test_popup"));
    }

    #[test]
    fn toggle_opens_when_closed_closes_when_open() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let toggle = UiIntent::TogglePopup {
            id: "test_popup",
            mode: OpenPopupMode::Default,
        }
        .from_user_shortcut("test");
        handle(&mut state, &mut engine, &toggle);
        assert!(state.popups.is_open("test_popup"));
        handle(&mut state, &mut engine, &toggle);
        assert!(!state.popups.is_open("test_popup"));
    }

    // 닫기 큐에 기록해야 후속 프레임의 정리가 실행된다.
    #[test]
    fn close_intent_pushes_to_closed_queue() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::Default),
        );
        handle(&mut state, &mut engine, &dispatched_close("test_popup"));
        assert_eq!(state.popups.take_closed_queue(), vec!["test_popup"]);
    }

    #[test]
    fn toggle_close_branch_pushes_to_closed_queue() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let toggle = UiIntent::TogglePopup {
            id: "test_popup",
            mode: OpenPopupMode::Default,
        }
        .from_user_shortcut("test");
        handle(&mut state, &mut engine, &toggle);
        assert!(state.popups.take_closed_queue().is_empty());
        handle(&mut state, &mut engine, &toggle);
        assert_eq!(state.popups.take_closed_queue(), vec!["test_popup"]);
    }

    #[test]
    fn agent_open_leaves_keyboard_with_the_user() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &UiIntent::OpenPopup {
                id: "test_popup",
                mode: OpenPopupMode::WithScope(PopupScope::Window),
            }
            .from_agent_ipc(),
        );
        assert!(state.popups.is_open("test_popup"));
        assert!(!state.popups.is_focused("test_popup"));
        assert!(state.popups.is_passive("test_popup"));
        assert!(!state.popups.has_focused());
        assert!(!state.has_egui_overlay_open());
    }

    #[test]
    fn user_open_takes_focus_and_is_not_passive() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::WithScope(PopupScope::Window)),
        );
        assert!(state.popups.is_focused("test_popup"));
        assert!(!state.popups.is_passive("test_popup"));
        assert!(state.popups.has_focused());
        assert!(state.has_egui_overlay_open());
    }

    #[test]
    fn agent_open_of_an_already_open_popup_keeps_user_focus() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::WithScope(PopupScope::Window)),
        );
        handle(
            &mut state,
            &mut engine,
            &UiIntent::OpenPopup {
                id: "test_popup",
                mode: OpenPopupMode::WithScope(PopupScope::Window),
            }
            .from_agent_ipc(),
        );
        assert!(state.popups.is_focused("test_popup"));
        assert!(!state.popups.is_passive("test_popup"));
    }

    fn open_passive(
        state: &mut RequestContext,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    ) {
        handle(
            state,
            engine,
            &UiIntent::OpenPopup {
                id: "test_popup",
                mode: OpenPopupMode::WithScope(PopupScope::Window),
            }
            .from_agent_ipc(),
        );
        assert!(state.popups.is_passive("test_popup"));
    }

    #[test]
    fn a_user_focus_intent_lifts_a_passive_popup_like_a_press_inside_it() {
        let (mut state, mut engine_session) = make_state();
        state.popups.register(PopupState::new(
            "other_popup",
            "Other".to_string(),
            egui::vec2(200.0, 100.0),
        ));
        let mut engine = engine_session.borrow_mut();
        open_passive(&mut state, &mut engine);
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("other_popup", OpenPopupMode::WithScope(PopupScope::Window)),
        );
        assert!(state.popups.is_focused("other_popup"));
        handle(
            &mut state,
            &mut engine,
            &UiIntent::FocusPassivePopup { id: "test_popup" }.from_user_shortcut("test"),
        );
        assert!(state.popups.is_focused("test_popup"));
        assert!(!state.popups.is_passive("test_popup"));
        assert!(
            !state.popups.is_focused("other_popup"),
            "다른 팝업의 포커스가 풀린다"
        );
        let (top, _) = state.popups.open_geometry("test_popup").expect("open");
        assert_eq!(state.popups.max_open_z_seq(), Some(top), "맨 앞에 온다");
    }

    #[test]
    fn a_non_user_focus_intent_leaves_the_popup_passive() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        open_passive(&mut state, &mut engine);
        handle(
            &mut state,
            &mut engine,
            &UiIntent::FocusPassivePopup { id: "test_popup" }.from_agent_ipc(),
        );
        assert!(state.popups.is_passive("test_popup"));
        assert!(!state.popups.has_focused());
    }

    #[test]
    fn a_focus_intent_needs_an_open_passive_popup() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let focus = UiIntent::FocusPassivePopup { id: "test_popup" }.from_user_shortcut("test");
        handle(&mut state, &mut engine, &focus);
        assert!(!state.popups.has_focused(), "닫힌 팝업");
        assert!(!state.popups.is_open("test_popup"));

        open_passive(&mut state, &mut engine);
        handle(&mut state, &mut engine, &dispatched_close("test_popup"));
        handle(&mut state, &mut engine, &focus);
        assert!(!state.popups.has_focused(), "그 사이 닫힌 팝업");
    }

    #[test]
    fn open_with_scope_uses_requested_scope() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::WithScope(PopupScope::Window)),
        );
        assert!(state.popups.is_open("test_popup"));
    }
}
