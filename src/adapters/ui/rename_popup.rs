//! 이름 바꾸기 입력 팝업을 여는 한 지점. 범위는 바꿀 대상이 보이는 자리에서 나온다(`RenameTarget::popup_scope`).
//! 대상을 찾지 못하면 창 범위로 바꾸지 않고 열지 않는다. 새 요청이 범위를 바꾸면 그 범위로 다시 열고,
//! 앞 요청의 원격 대기 상태는 넘기지 않는다.

use crate::intent::{DispatchedIntent, UiIntent};
use crate::state::{MainViewState, RenameTarget};

pub(crate) const RENAME_POPUP_ID: crate::adapters::ui::popup::PopupId = "rename";

/// 이름 바꾸기 인스턴스의 데이터.
pub(crate) struct RenamePayload {
    pub(crate) target: RenameTarget,
    /// 입력 칸의 글자.
    pub(crate) buffer: String,
    /// 직전 프레임에 그린 콘텐츠 높이. 오류·대기 줄이 생기고 사라지는 대로 sizer 가 팝업 높이를
    /// 맞춘다. 아직 그리지 않았으면 `None`.
    pub(crate) content_height: Option<tasty_type_geometry::length::LogicalPx>,
    /// 원격 이름 변경의 답을 기다린다. 기다리는 동안 팝업은 열린 채 입력과 버튼을 막는다.
    pub(crate) waiting: Option<crate::state::RenameWaiting>,
}

impl RenamePayload {
    pub(crate) fn new(target: RenameTarget, buffer: String) -> Self {
        Self {
            target,
            buffer,
            content_height: None,
            waiting: None,
        }
    }
}

pub(crate) fn payload_of(
    payload: &crate::adapters::ui::popup::PopupPayload,
) -> Option<&RenamePayload> {
    match payload {
        crate::adapters::ui::popup::PopupPayload::Rename(r) => Some(r),
        _ => None,
    }
}

/// 열린 이름 바꾸기 인스턴스들의 데이터.
#[cfg(test)]
pub(crate) fn instances(state: &MainViewState) -> impl Iterator<Item = &RenamePayload> + '_ {
    state
        .popups
        .instances_of(RENAME_POPUP_ID)
        .filter_map(|p| payload_of(&p.payload))
}

pub(crate) fn instances_mut(
    state: &mut MainViewState,
) -> impl Iterator<Item = &mut RenamePayload> + '_ {
    state
        .popups
        .entries_of_mut(RENAME_POPUP_ID)
        .filter_map(|p| match &mut p.payload {
            crate::adapters::ui::popup::PopupPayload::Rename(r) => Some(r.as_mut()),
            _ => None,
        })
}

/// `target`의 범위로 이름 바꾸기를 연다. 대상이 없으면 경고 로그만 남기고 false다.
/// 그 범위에 이미 열려 있으면 대상과 입력을 새 요청으로 바꾸고, 앞 요청의 원격 대기와 높이는 버린다.
pub(crate) fn open(
    state: &mut MainViewState,
    engine: &crate::core::CoreState,
    target: RenameTarget,
    current_value: String,
    origin: impl FnOnce(UiIntent) -> DispatchedIntent,
) -> bool {
    let Some(scope) = target.popup_scope(engine) else {
        tracing::warn!("rename popup not opened: its target {target:?} is gone");
        return false;
    };
    state.dispatch_intent(origin(UiIntent::ReopenPopup(
        crate::intent::OpenRequest::new(RENAME_POPUP_ID, scope)
            .centered()
            .focused()
            .with_payload(crate::adapters::ui::popup::PopupPayload::Rename(Box::new(
                RenamePayload::new(target, current_value),
            ))),
    )));
    true
}

/// 시험용: 맨 위 이름 바꾸기 인스턴스의 대상.
#[cfg(test)]
pub(crate) fn current_target(state: &MainViewState) -> Option<RenameTarget> {
    instances(state).last().map(|r| r.target.clone())
}

/// 시험용: 이름 바꾸기 인스턴스를 그 데이터로 바로 연다. 위치를 주면 그 점, 없으면 범위 가운데다.
#[cfg(test)]
pub(crate) fn open_for_test(
    state: &mut MainViewState,
    rename: RenamePayload,
    scope: crate::model::popup_kind::PopupScope,
    at: Option<egui::Pos2>,
) -> Option<crate::adapters::ui::popup::PopupInstanceId> {
    let request = crate::intent::OpenRequest::new(RENAME_POPUP_ID, scope).focused();
    let request = match at {
        Some(pos) => request.at(pos),
        None => request.centered(),
    };
    // intent-exempt: 시험이 그리기 전 상태를 만드는 직접 열기. 사용자 입력 경로가 아니다.
    state.popups.open_request(
        &request.with_payload(crate::adapters::ui::popup::PopupPayload::Rename(Box::new(
            rename,
        ))),
        true,
    )
}

/// 시험용: 열린 인스턴스의 데이터를 바꾼다. 다음 프레임부터 새 입력으로 그린다.
#[cfg(test)]
pub(crate) fn set_buffer_for_test(state: &mut MainViewState, buffer: &str) {
    for rename in instances_mut(state) {
        rename.buffer = buffer.to_string();
    }
}

/// 시험용: 닫힌 원본까지 포함해 이름 바꾸기 데이터가 남았는지.
#[cfg(test)]
pub(crate) fn data_left(state: &MainViewState) -> bool {
    state
        .popups
        .entries_of(RENAME_POPUP_ID)
        .any(|p| payload_of(&p.payload).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::popup_kind::PopupScope;

    fn opened_scope(state: &mut MainViewState) -> Option<PopupScope> {
        let intents = state.take_pending_intents();
        match &intents[..] {
            [] => None,
            [
                DispatchedIntent {
                    body:
                        crate::intent::Intent::Ui(UiIntent::ReopenPopup(crate::intent::OpenRequest {
                            id: RENAME_POPUP_ID,
                            scope,
                            placement: crate::intent::PopupPlacement::ScopeCenter,
                            focus: true,
                            ..
                        })),
                    ..
                },
            ] => Some(scope.clone()),
            other => panic!("unexpected intents {other:?}"),
        }
    }

    #[test]
    fn each_target_opens_in_its_own_scope() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        for (target, scope) in [
            (
                RenameTarget::WorkspaceName { workspace_id: 1 },
                PopupScope::Window,
            ),
            (
                RenameTarget::WorkspaceSubtitle { workspace_id: 1 },
                PopupScope::Window,
            ),
            (RenameTarget::TabName { tab_id: 102 }, PopupScope::Pane(10)),
            (RenameTarget::TabName { tab_id: 201 }, PopupScope::Pane(20)),
            (RenameTarget::NewCategory, PopupScope::Window),
        ] {
            assert!(open(
                &mut state,
                &read,
                target.clone(),
                String::new(),
                UiIntent::from_user_context_menu
            ));
            assert_eq!(opened_scope(&mut state), Some(scope), "{target:?}");
        }
    }

    /// workspace·탭 대상이 없으면 창 범위로 열지 않고 상태도 남기지 않는다.
    #[test]
    fn a_gone_target_does_not_open() {
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let read = session.read();
        for target in [
            RenameTarget::WorkspaceName { workspace_id: 99 },
            RenameTarget::WorkspaceSubtitle { workspace_id: 99 },
            RenameTarget::TabName { tab_id: 999 },
        ] {
            assert!(!open(
                &mut state,
                &read,
                target.clone(),
                String::new(),
                UiIntent::from_user_context_menu
            ));
            assert_eq!(opened_scope(&mut state), None, "{target:?}");
            assert!(current_target(&state).is_none(), "{target:?}");
        }
    }

    fn open_tab_rename(
        state: &mut crate::state::RequestContext,
        session: &mut crate::runtime::engine_session::EngineSession,
        tab_id: u32,
    ) {
        let read = session.read();
        assert!(open(
            state,
            &read,
            RenameTarget::TabName { tab_id },
            String::new(),
            UiIntent::from_user_context_menu
        ));
        drop(read);
        crate::intent::popup::apply_pending(state, session);
    }

    /// 열린 이름 바꾸기 인스턴스의 (범위, 대상)을 범위 순서로 모은다.
    fn opened(state: &MainViewState) -> Vec<(PopupScope, RenameTarget)> {
        let mut found: Vec<(PopupScope, RenameTarget)> = state
            .popups
            .instances_of(RENAME_POPUP_ID)
            .filter_map(|p| Some((p.scope.clone(), payload_of(&p.payload)?.target.clone())))
            .collect();
        found.sort_by_key(|(scope, _)| format!("{scope:?}"));
        found
    }

    /// 한 pane 에서 연 탭 이름 바꾸기에 다른 pane 탭의 요청이 오면 그 pane 에 따로 열린다. 앞의 것은 그대로다.
    #[test]
    fn a_second_request_in_another_pane_opens_its_own_popup() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        open_tab_rename(&mut state, &mut session, 101);
        open_tab_rename(&mut state, &mut session, 201);
        assert_eq!(
            opened(&state),
            vec![
                (PopupScope::Pane(10), RenameTarget::TabName { tab_id: 101 }),
                (PopupScope::Pane(20), RenameTarget::TabName { tab_id: 201 }),
            ]
        );
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 같은 pane 의 다른 탭 요청은 범위를 그대로 두고 대상만 바꾼다.
    #[test]
    fn a_second_request_for_a_tab_in_the_same_pane_retargets_the_popup() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        open_tab_rename(&mut state, &mut session, 101);
        open_tab_rename(&mut state, &mut session, 102);
        assert_eq!(
            opened(&state),
            vec![(PopupScope::Pane(10), RenameTarget::TabName { tab_id: 102 })]
        );
        assert!(state.popups.take_closed_ids().is_empty());
    }

    /// 원격 이름 바꾸기를 기다리는 동안 같은 자리에서 다른 대상의 이름 바꾸기를 열면 대기 표시가 없고,
    /// 앞 요청의 늦은 성공 응답이 새 팝업을 닫지 않는다.
    #[test]
    fn a_second_request_drops_the_remote_wait_of_the_first() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        open_tab_rename(&mut state, &mut session, 101);
        for rename in instances_mut(&mut state) {
            rename.waiting = Some(crate::state::RenameWaiting {
                request: 5,
                host: "host".into(),
                done: false,
            });
            rename.content_height = Some(tasty_type_geometry::length::LogicalPx(40.0));
        }
        open_tab_rename(&mut state, &mut session, 102);
        let rename = instances(&state).next().expect("open");
        assert!(rename.waiting.is_none());
        assert!(rename.content_height.is_none());
        assert!(!crate::app::explorer_files::mark_rename_done(&mut state, 5));
        assert!(instances(&state).all(|r| r.waiting.is_none()));
        assert_eq!(
            current_target(&state),
            Some(RenameTarget::TabName { tab_id: 102 })
        );
    }
}
