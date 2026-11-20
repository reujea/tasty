//! 팝업 Intent 처리. 범위·위치·포커스는 열기 요청(`OpenRequest`)을 따른다. 사용자 조작이 아닌
//! 요청으로 연 팝업은 키보드 포커스를 갖지 않는 passive 상태로 연다.
//! 정책: docs/design/flows/action-dispatch.md, docs/design/systems/popup.md.

#[cfg(feature = "gui")]
use super::DispatchedIntent;
#[cfg(feature = "gui")]
use super::{Intent, OpenRequest, UiIntent};
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

            UiIntent::OpenPopup(request) => {
                open(state, request, intent.origin.is_user());
            }
            UiIntent::ReopenPopup(request) => reopen(state, request, intent.origin.is_user()),
            UiIntent::ClosePopup { id } => state.popups.close(id),
            UiIntent::CloseInstance { instance, reason } => {
                state.popups.close_instance(*instance, *reason)
            }
            UiIntent::FocusPassivePopup { id, scope } => {
                if intent.origin.is_user() {
                    match scope {
                        Some(scope) => state.popups.focus_passive_in(id, scope),
                        None => state.popups.focus_passive(id),
                    };
                }
            }
            UiIntent::TogglePopup(request) => {
                match opened_for(state, request).map(|(instance, _)| instance) {
                    Some(instance) => state.popups.close_instance(
                        instance,
                        crate::adapters::ui::popup::PopupCloseReason::Command,
                    ),
                    None => {
                        open(state, request, intent.origin.is_user());
                    }
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

/// 이 요청에 쓸, 이미 열린 인스턴스와 그 인스턴스가 요청 범위에 있는지. 열기·다시 열기·토글 처리와 즉시 적용
/// 판정([`gives_user_focus`])이 이 값 하나를 읽는다.
#[cfg(feature = "gui")]
fn opened_for(
    state: &RequestContext,
    request: &OpenRequest,
) -> Option<(crate::adapters::ui::popup::PopupInstanceId, bool)> {
    let scope = state.popups.request_scope(request);
    let instance = state.popups.instance_for(request.id, &scope)?;
    let same_scope = state
        .popups
        .instance(instance)
        .is_some_and(|p| p.scope == scope);
    Some((instance, same_scope))
}

/// 이 intent 를 지금 처리하면 사용자 입력으로 포커스를 주며 팝업을 여는지. 처리기([`handle`])와 같은 값
/// ([`opened_for`])을 읽는다. 열린 인스턴스에 잇는 열기, 같은 범위에 열린 인스턴스의 내용만 바꾸는 다시 열기,
/// 열린 인스턴스를 닫는 토글은 포커스를 주지 않는다. 대기열의 앞 intent 가 바꿀 상태는 보지 않고 지금 상태로 판정한다.
#[cfg(feature = "gui")]
pub(crate) fn gives_user_focus(state: &RequestContext, intent: &DispatchedIntent) -> bool {
    if !intent.origin.is_user() {
        return false;
    }
    match &intent.body {
        Intent::Ui(UiIntent::OpenPopup(r) | UiIntent::TogglePopup(r)) => {
            r.focus && opened_for(state, r).is_none()
        }
        Intent::Ui(UiIntent::ReopenPopup(r)) => {
            r.focus && !opened_for(state, r).is_some_and(|(_, same_scope)| same_scope)
        }
        _ => false,
    }
}

/// 요청을 연다. 인스턴스 정책이 이 요청에 쓸 인스턴스가 이미 열려 있으면 위치·포커스를 바꾸지 않는다
/// (사용자가 준 포커스를 빼앗지 않는다). 큐 팝업이면 요청의 항목을 그 인스턴스의 차례 뒤에 잇는다.
/// 열었으면 새 인스턴스 id다.
#[cfg(feature = "gui")]
fn open(
    state: &mut RequestContext,
    request: &OpenRequest,
    by_user: bool,
) -> Option<crate::adapters::ui::popup::PopupInstanceId> {
    if let Some((instance, _)) = opened_for(state, request) {
        state.popups.append_queued(instance, request);
        return None;
    }
    if let Some(on_open) =
        crate::adapters::ui::popup::defs::find(request.id).and_then(|def| def.on_open)
    {
        on_open(state);
    }
    // 에이전트·플러그인·시스템 요청이 사용자의 키 입력을 가로채지 않게 한다.
    state.popups.open_request(request, by_user)
}

/// 새 요청이 내용과 대상을 바꾼 팝업을 다시 연다. 이 요청에 쓸 인스턴스가 다른 범위에 열려 있으면 닫기 훅 없이
/// 닫고 새 범위로 연다. 같은 범위에 열려 있으면 위치·포커스는 그대로 두고 인스턴스 데이터만 바꾼다.
#[cfg(feature = "gui")]
fn reopen(state: &mut RequestContext, request: &OpenRequest, by_user: bool) {
    if let Some((instance, same_scope)) = opened_for(state, request) {
        if same_scope {
            state
                .popups
                .replace_payload(instance, request.payload.take());
            return;
        }
        state.popups.close_for_reopen(request.id);
    }
    open(state, request, by_user);
}

/// 시험용: 엔진 없이 열기 요청 하나를 반영한다.
#[cfg(all(test, feature = "gui"))]
pub(crate) fn open_for_test(state: &mut RequestContext, request: &OpenRequest, by_user: bool) {
    open(state, request, by_user);
}

/// 시험용: 엔진 없이 다시 열기 요청 하나를 사용자 요청으로 반영한다.
#[cfg(all(test, feature = "gui"))]
pub(crate) fn reopen_for_test(state: &mut RequestContext, request: &OpenRequest) {
    reopen(state, request, true);
}

/// 시험용: 쌓인 intent 를 팝업 처리기로 차례로 반영한다(팝업 열기·닫기만 바뀐다).
#[cfg(all(test, feature = "gui"))]
pub(crate) fn apply_pending(
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
) {
    let mut engine = session.borrow_mut();
    for intent in state.take_pending_intents() {
        handle(state, &mut engine, &intent);
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
            crate::model::popup_kind::PopupScope::Window,
        ));
        (state, engine_session)
    }

    /// 시험이 고르는 열기 위치·포커스 조합.
    enum OpenPopupMode {
        Default(PopupScope),
        CenteredFocused(PopupScope),
        AtTopOfScope(PopupScope),
        AtFocused(PopupScope, egui::Pos2),
    }

    impl OpenPopupMode {
        fn request(self, id: &'static str) -> OpenRequest {
            match self {
                Self::Default(scope) => OpenRequest::new(id, scope),
                Self::CenteredFocused(scope) => OpenRequest::new(id, scope).centered().focused(),
                Self::AtTopOfScope(scope) => OpenRequest::new(id, scope).at_top().focused(),
                Self::AtFocused(scope, pos) => OpenRequest::new(id, scope).at(pos).focused(),
            }
        }
    }

    fn dispatched_open(id: &'static str, mode: OpenPopupMode) -> DispatchedIntent {
        UiIntent::OpenPopup(mode.request(id)).from_user_shortcut("test")
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
            &dispatched_open("test_popup", OpenPopupMode::Default(PopupScope::Window)),
        );
        assert!(state.popups.is_open("test_popup"));
        handle(
            &mut state,
            &mut engine,
            &dispatched_open(
                "test_popup",
                OpenPopupMode::CenteredFocused(PopupScope::Window),
            ),
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
            &dispatched_open("test_popup", OpenPopupMode::Default(PopupScope::Window)),
        );
        assert!(state.popups.is_open("test_popup"));
        handle(&mut state, &mut engine, &dispatched_close("test_popup"));
        assert!(!state.popups.is_open("test_popup"));
    }

    #[test]
    fn toggle_opens_when_closed_closes_when_open() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let toggle = UiIntent::TogglePopup(crate::intent::OpenRequest::new(
            "test_popup",
            PopupScope::Window,
        ))
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
            &dispatched_open("test_popup", OpenPopupMode::Default(PopupScope::Window)),
        );
        handle(&mut state, &mut engine, &dispatched_close("test_popup"));
        assert_eq!(state.popups.take_closed_ids(), vec!["test_popup"]);
    }

    fn dispatched_reopen(id: &'static str, scope: PopupScope) -> DispatchedIntent {
        UiIntent::ReopenPopup(OpenPopupMode::CenteredFocused(scope).request(id))
            .from_user_shortcut("test")
    }

    /// 다른 범위의 다시 열기 요청은 닫고 새 범위로 연다. 내용은 새 요청이 채웠으므로 on_close 를 부르지 않는다.
    #[test]
    fn reopen_in_another_scope_moves_the_popup_without_a_close_hook() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_reopen("test_popup", PopupScope::Surface(1)),
        );
        assert_eq!(
            state.popups.open_scope("test_popup"),
            Some(&PopupScope::Surface(1))
        );
        handle(
            &mut state,
            &mut engine,
            &dispatched_reopen("test_popup", PopupScope::Surface(2)),
        );
        assert_eq!(
            state.popups.open_scope("test_popup"),
            Some(&PopupScope::Surface(2))
        );
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 같은 범위의 다시 열기 요청은 열린 팝업을 그대로 둔다.
    #[test]
    fn reopen_in_the_same_scope_keeps_the_popup() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_reopen("test_popup", PopupScope::Surface(1)),
        );
        let z = state.popups.max_open_z_seq();
        handle(
            &mut state,
            &mut engine,
            &dispatched_reopen("test_popup", PopupScope::Surface(1)),
        );
        assert_eq!(state.popups.max_open_z_seq(), z);
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 열기 요청은 이미 열린 팝업의 범위를 바꾸지 않는다(다시 열기만 바꾼다).
    #[test]
    fn a_plain_open_keeps_the_scope_of_an_open_popup() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::Default(PopupScope::Surface(1))),
        );
        handle(
            &mut state,
            &mut engine,
            &dispatched_open("test_popup", OpenPopupMode::Default(PopupScope::Surface(2))),
        );
        assert_eq!(
            state.popups.open_scope("test_popup"),
            Some(&PopupScope::Surface(1))
        );
    }

    #[test]
    fn toggle_close_branch_pushes_to_closed_queue() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let toggle = UiIntent::TogglePopup(crate::intent::OpenRequest::new(
            "test_popup",
            PopupScope::Window,
        ))
        .from_user_shortcut("test");
        handle(&mut state, &mut engine, &toggle);
        assert!(state.popups.take_closed_ids().is_empty());
        handle(&mut state, &mut engine, &toggle);
        assert_eq!(state.popups.take_closed_ids(), vec!["test_popup"]);
    }

    #[test]
    fn agent_open_leaves_keyboard_with_the_user() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        handle(
            &mut state,
            &mut engine,
            &UiIntent::OpenPopup(
                crate::intent::OpenRequest::new("test_popup", PopupScope::Window)
                    .centered()
                    .focused(),
            )
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
            &dispatched_open(
                "test_popup",
                OpenPopupMode::CenteredFocused(PopupScope::Window),
            ),
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
            &dispatched_open(
                "test_popup",
                OpenPopupMode::CenteredFocused(PopupScope::Window),
            ),
        );
        handle(
            &mut state,
            &mut engine,
            &UiIntent::OpenPopup(
                crate::intent::OpenRequest::new("test_popup", PopupScope::Window)
                    .centered()
                    .focused(),
            )
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
            &UiIntent::OpenPopup(
                crate::intent::OpenRequest::new("test_popup", PopupScope::Window)
                    .centered()
                    .focused(),
            )
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
            crate::model::popup_kind::PopupScope::Window,
        ));
        let mut engine = engine_session.borrow_mut();
        open_passive(&mut state, &mut engine);
        handle(
            &mut state,
            &mut engine,
            &dispatched_open(
                "other_popup",
                OpenPopupMode::CenteredFocused(PopupScope::Window),
            ),
        );
        assert!(state.popups.is_focused("other_popup"));
        handle(
            &mut state,
            &mut engine,
            &UiIntent::FocusPassivePopup {
                id: "test_popup",
                scope: None,
            }
            .from_user_shortcut("test"),
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
            &UiIntent::FocusPassivePopup {
                id: "test_popup",
                scope: None,
            }
            .from_agent_ipc(),
        );
        assert!(state.popups.is_passive("test_popup"));
        assert!(!state.popups.has_focused());
    }

    #[test]
    fn a_focus_intent_needs_an_open_passive_popup() {
        let (mut state, mut engine_session) = make_state();
        let mut engine = engine_session.borrow_mut();
        let focus = UiIntent::FocusPassivePopup {
            id: "test_popup",
            scope: None,
        }
        .from_user_shortcut("test");
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
            &dispatched_open(
                "test_popup",
                OpenPopupMode::CenteredFocused(PopupScope::Window),
            ),
        );
        assert!(state.popups.is_open("test_popup"));
    }

    /// 범위는 열 때마다 정한다. 닫은 뒤 다른 범위로 다시 열면 직전 범위가 남지 않는다.
    #[test]
    fn every_mode_sets_the_scope_and_a_reopen_does_not_keep_the_last_one() {
        let modes = |scope: PopupScope| {
            [
                OpenPopupMode::Default(scope.clone()),
                OpenPopupMode::CenteredFocused(scope.clone()),
                OpenPopupMode::AtTopOfScope(scope.clone()),
                OpenPopupMode::AtFocused(scope, egui::pos2(10.0, 10.0)),
            ]
        };
        for (first, second) in modes(PopupScope::Surface(7))
            .into_iter()
            .zip(modes(PopupScope::Window))
        {
            let (mut state, mut engine_session) = make_state();
            let mut engine = engine_session.borrow_mut();
            handle(
                &mut state,
                &mut engine,
                &dispatched_open("test_popup", first),
            );
            assert_eq!(
                state.popups.open_scope("test_popup"),
                Some(&PopupScope::Surface(7))
            );
            handle(&mut state, &mut engine, &dispatched_close("test_popup"));
            handle(
                &mut state,
                &mut engine,
                &dispatched_open("test_popup", second),
            );
            assert_eq!(
                state.popups.open_scope("test_popup"),
                Some(&PopupScope::Window)
            );
        }
    }

    /// 부모 범위를 따르는 child 선언 하나와, 서로 다른 surface에 연 부모 둘.
    fn parent_child_state() -> (
        RequestContext,
        crate::runtime::engine_session::EngineSession,
        crate::adapters::ui::popup::PopupInstanceId,
        crate::adapters::ui::popup::PopupInstanceId,
    ) {
        use crate::adapters::ui::popup::PopupInstances;
        let (mut state, engine_session) = make_state();
        for id in ["parent_popup", "child_popup"] {
            state.popups.register(
                PopupState::new(
                    id,
                    "T".to_string(),
                    egui::vec2(200.0, 100.0),
                    PopupScope::Window,
                )
                .with_instances(PopupInstances::PerScopeTarget),
            );
        }
        let left = state
            .popups
            .open("parent_popup", PopupScope::Surface(1))
            .unwrap();
        let right = state
            .popups
            .open("parent_popup", PopupScope::Surface(2))
            .unwrap();
        (state, engine_session, left, right)
    }

    fn child_request(
        scope: PopupScope,
        parent: crate::adapters::ui::popup::PopupInstanceId,
        surface_id: u32,
    ) -> OpenRequest {
        let mut request = OpenRequest::new("child_popup", scope).with_payload(
            crate::adapters::ui::popup::PopupPayload::ExplorerConflict { surface_id },
        );
        request.parent = Some(parent);
        request
    }

    fn child_targets(state: &RequestContext) -> Vec<(PopupScope, Option<u32>)> {
        state
            .popups
            .instances_of("child_popup")
            .map(|p| {
                let target = match p.payload {
                    crate::adapters::ui::popup::PopupPayload::ExplorerConflict { surface_id } => {
                        Some(surface_id)
                    }
                    _ => None,
                };
                (p.scope.clone(), target)
            })
            .collect()
    }

    /// Given child가 L 부모 범위에 열려 있을 때, When 요청 범위는 L이지만 R 부모를 지정한 열기 intent가 오면,
    /// Then 요청을 버리지 않고 R 범위에 새 child를 연다.
    /// When 요청 범위는 비어 있는 surface지만 L 부모를 지정한 열기·다시 열기·토글 요청이 오면,
    /// Then 이미 열린 L child를 찾는다. 열기는 그 데이터를 덮어쓰지 않고, 다시 열기는 그 자리에서 데이터만
    /// 바꾸며, 토글은 그 child를 닫는다.
    #[test]
    fn open_intents_resolve_the_parent_scope_before_looking_for_an_instance() {
        let (mut state, mut engine_session, left, right) = parent_child_state();
        let mut engine = engine_session.borrow_mut();
        let open = |request: OpenRequest| UiIntent::OpenPopup(request).from_user_shortcut("test");
        handle(
            &mut state,
            &mut engine,
            &open(child_request(PopupScope::Surface(1), left, 10)),
        );
        assert_eq!(child_targets(&state), [(PopupScope::Surface(1), Some(10))]);

        handle(
            &mut state,
            &mut engine,
            &open(child_request(PopupScope::Surface(1), right, 20)),
        );
        assert_eq!(
            child_targets(&state),
            [
                (PopupScope::Surface(1), Some(10)),
                (PopupScope::Surface(2), Some(20))
            ],
            "a request for the R parent opens its own child"
        );

        handle(
            &mut state,
            &mut engine,
            &open(child_request(PopupScope::Surface(9), left, 30)),
        );
        assert_eq!(
            child_targets(&state),
            [
                (PopupScope::Surface(1), Some(10)),
                (PopupScope::Surface(2), Some(20))
            ],
            "the open L child keeps its target"
        );

        reopen(
            &mut state,
            &child_request(PopupScope::Surface(9), left, 50),
            true,
        );
        assert_eq!(
            child_targets(&state),
            [
                (PopupScope::Surface(1), Some(50)),
                (PopupScope::Surface(2), Some(20))
            ],
            "a reopen for the L parent updates the L child in place"
        );

        let toggle = UiIntent::TogglePopup(child_request(PopupScope::Surface(9), left, 40))
            .from_user_shortcut("test");
        handle(&mut state, &mut engine, &toggle);
        assert_eq!(child_targets(&state), [(PopupScope::Surface(2), Some(20))]);
    }
}
