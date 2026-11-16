//! 범위 대상이나 작용 대상이 사라진 팝업을 정리 단계가 닫는지 검사한다.
//! 두 pane fixture: pane 10 에 탭 101(선택)·102, pane 20 에 탭 201. 활성 workspace 는 1.
//! 프레임은 pane 사각형 없이 그린다. 그래서 Pane 범위 팝업은 보이지 않고 그리기 단계의 대상 검사도 돌지 않는다.

use crate::adapters::ui::popup::preset_apply::{
    APPLY_PANE_POPUP_ID, APPLY_TAB_POPUP_ID, PresetApplyTarget, open_for_pane,
};
use crate::adapters::ui::rename_popup::RENAME_POPUP_ID;
use crate::state::RenameTarget;
use tasty_core::DomainEvent as E;
use tasty_presets::PresetKind;
use tasty_type_geometry::length::PhysicalPx;
use tasty_type_geometry::rect::PhysicalRect;

fn frame(
    state: &mut crate::state::RequestContext,
    session: &crate::runtime::engine_session::EngineSession,
) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1920.0),
        height: PhysicalPx(1080.0),
    };
    drop(egui::Context::default().run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, &session.read(), &[], rect, 1.0);
    }));
}

/// fixture 기록 위에 닫기 사건을 적용한다.
fn close_in_core(session: &mut crate::runtime::engine_session::EngineSession, events: Vec<E>) {
    let before = crate::state::tests::test_model(crate::state::tests::two_pane_events());
    let first = before.applied.revision.unwrap_or(0) + 1;
    let events = events
        .into_iter()
        .zip(first..)
        .map(|(event, revision)| tasty_core::RecordedEvent { revision, event })
        .collect();
    let mut retired = Vec::new();
    tasty_core::projection::apply(
        session.borrow_mut().core,
        &before,
        &tasty_core::DomainBatch {
            batch_id: 2,
            events,
        },
        &mut retired,
    )
    .expect("apply");
}

fn open_preset(
    state: &mut crate::state::RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    kind: PresetKind,
    pane: u32,
) {
    assert!(open_for_pane(
        state,
        &session.read(),
        kind,
        pane,
        crate::intent::UiIntent::from_user_context_menu,
    ));
    crate::intent::popup::apply_pending(state, session);
}

/// 탭 이름 바꾸기를 연 뒤 그 탭의 pane 이 닫히면 팝업이 닫히고 입력 게이트가 풀린다.
#[test]
fn a_tab_rename_closes_when_its_pane_closes() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    assert!(crate::adapters::ui::rename_popup::open(
        &mut state,
        &session.read(),
        RenameTarget::TabName { tab_id: 201 },
        String::new(),
        crate::intent::UiIntent::from_user_context_menu,
    ));
    crate::intent::popup::apply_pending(&mut state, &mut session);
    frame(&mut state, &session);
    assert!(state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.has_input_dialog_open());

    close_in_core(&mut session, vec![E::PaneClosed { id: 20 }]);
    frame(&mut state, &session);
    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.dialogs.rename.is_none());
    assert!(!state.has_input_dialog_open());
}

/// 탭 프리셋을 연 뒤 대상 탭만 닫히면(pane 은 남음) 팝업이 닫히고 대상이 비워진다.
#[test]
fn a_tab_preset_closes_when_its_tab_closes_and_the_pane_stays() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    open_preset(&mut state, &mut session, PresetKind::Tab, 10);
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Tab),
        Some(PresetApplyTarget::Tab(101))
    );
    frame(&mut state, &session);
    assert!(state.popups.is_open(APPLY_TAB_POPUP_ID));

    close_in_core(&mut session, vec![E::TabClosed { id: 101 }]);
    assert!(session.read().has_pane(10));
    frame(&mut state, &session);
    assert!(!state.popups.is_open(APPLY_TAB_POPUP_ID));
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Tab),
        None
    );
}

/// pane 프리셋을 연 뒤 대상 pane 이 닫히면(workspace 는 남음) 팝업이 닫히고 대상이 비워진다.
#[test]
fn a_pane_preset_closes_when_its_pane_closes() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    open_preset(&mut state, &mut session, PresetKind::Pane, 20);
    frame(&mut state, &session);
    assert!(state.popups.is_open(APPLY_PANE_POPUP_ID));

    close_in_core(&mut session, vec![E::PaneClosed { id: 20 }]);
    frame(&mut state, &session);
    assert!(!state.popups.is_open(APPLY_PANE_POPUP_ID));
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Pane),
        None
    );
}

/// Pane 범위 팝업은 그 pane 이 사라지면 대상 판정이 없어도 닫힌다.
#[test]
fn a_pane_scoped_popup_closes_when_the_pane_closes() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    open_preset(&mut state, &mut session, PresetKind::Tab, 20);
    assert_eq!(
        state.popups.open_scope(APPLY_TAB_POPUP_ID),
        Some(&crate::model::popup_kind::PopupScope::Pane(20))
    );
    close_in_core(&mut session, vec![E::PaneClosed { id: 20 }]);
    state
        .popups
        .close_scope_orphans(|scope| super::scope_target_exists(scope, &session.read()));
    assert!(!state.popups.is_open(APPLY_TAB_POPUP_ID));
}

/// 탭 이름 바꾸기를 연 뒤 대상 탭만 닫히면(pane 은 남아 범위는 살아 있음) 팝업이 닫힌다.
#[test]
fn a_tab_rename_closes_when_its_tab_closes_and_the_pane_stays() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    assert!(crate::adapters::ui::rename_popup::open(
        &mut state,
        &session.read(),
        RenameTarget::TabName { tab_id: 101 },
        String::new(),
        crate::intent::UiIntent::from_user_context_menu,
    ));
    crate::intent::popup::apply_pending(&mut state, &mut session);
    close_in_core(&mut session, vec![E::TabClosed { id: 101 }]);
    assert!(session.read().has_pane(10));
    frame(&mut state, &session);
    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.dialogs.rename.is_none());
    assert!(!state.has_input_dialog_open());
}
