//! 전체화면 무대 상태와 닫기 후 처리를 검사한다.
//! GPU 렌더 순서는 여기서 실행하지 않으며 별도의 fullscreen_stage_render_gate가 소스를 검사한다.

use super::tests::test_state;
use crate::adapters::ui::draw_popups;
use crate::adapters::ui::fullscreen;

fn run_stage_frame(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let ctx = egui::Context::default();
    // 렌더 결과 대신 프레임 후 상태를 검사한다.
    drop(ctx.run(egui::RawInput::default(), |ctx| {
        crate::adapters::ui::draw_fullscreen_stage(ctx, state, engine);
    }));
}

fn run_normal_frame(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let ctx = egui::Context::default();
    // 렌더 결과 대신 프레임 후 상태를 검사한다.
    drop(ctx.run(egui::RawInput::default(), |ctx| {
        draw_popups(
            ctx,
            state,
            engine,
            &[],
            crate::model::PhysicalRect {
                x: crate::model::PhysicalPx(0.0),
                y: crate::model::PhysicalPx(0.0),
                width: crate::model::PhysicalPx(800.0),
                height: crate::model::PhysicalPx(600.0),
            },
            1.0,
        );
    }));
}

#[test]
fn fullscreen_stage_marks_overlay_open() {
    let (mut state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    assert!(!state.has_egui_overlay_open());
    assert!(state.open_fullscreen_stage("blank"));
    assert!(state.has_egui_overlay_open());
    assert!(state.close_fullscreen_stage());
    assert!(!state.has_egui_overlay_open());
}

#[test]
fn unknown_stage_id_is_rejected() {
    let (mut state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    assert!(!state.open_fullscreen_stage("no-such-stage"));
    assert!(!state.fullscreen_stage_active());
    assert!(state.stage_closed_queue.is_empty());
}

#[test]
fn only_one_stage_at_a_time() {
    use std::sync::atomic::Ordering;

    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    // cfg(test)의 두 번째 무대로 교체를 검사한다.
    let b = fullscreen::defs::TEST_STAGE_ID;
    assert!(state.open_fullscreen_stage(b));
    let closes_before = fullscreen::defs::TEST_STAGE_CLOSES.load(Ordering::Relaxed);

    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(state.fullscreen_stage_id(), Some("blank"));
    assert_eq!(state.stage_closed_queue, vec![b]);

    run_stage_frame(&mut state, &engine.read());
    assert!(state.stage_closed_queue.is_empty());
    assert_eq!(
        fullscreen::defs::TEST_STAGE_CLOSES.load(Ordering::Relaxed),
        closes_before + 1,
        "교체된 무대의 on_close가 실행되지 않았다"
    );
}

#[test]
fn reopening_the_same_stage_does_not_close_and_reopen() {
    let (mut state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    assert!(state.open_fullscreen_stage("blank"));
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(state.fullscreen_stage_id(), Some("blank"));
    assert!(state.stage_closed_queue.is_empty());
}

#[test]
fn close_pushes_exactly_one_hook_entry_and_is_idempotent() {
    let (mut state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    state.open_fullscreen_stage("blank");
    assert!(state.close_fullscreen_stage());
    assert!(!state.close_fullscreen_stage());
    assert_eq!(state.stage_closed_queue, vec!["blank"]);
}

#[test]
fn normal_frame_drains_the_close_hook_queue() {
    // 종료 후에는 일반 프레임이 닫기 큐를 처리해야 한다.
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    state.open_fullscreen_stage("blank");
    state.close_fullscreen_stage();
    assert!(!state.stage_closed_queue.is_empty());
    run_normal_frame(&mut state, &engine.read());
    assert!(state.stage_closed_queue.is_empty());
}

#[test]
fn stage_frame_drains_the_close_hook_queue() {
    // 무대 교체에는 일반 프레임이 끼지 않으므로 무대 프레임도 닫기 큐를 처리해야 한다.
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    state.open_fullscreen_stage("blank");
    state.stage_closed_queue.push("blank");
    run_stage_frame(&mut state, &engine.read());
    assert!(state.stage_closed_queue.is_empty());
}

#[test]
fn stage_frame_paints_only_when_a_stage_is_up() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    let painted = |state: &mut crate::state::RequestContext,
                   engine: &crate::runtime::engine_read::EngineRead<'_>| {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            crate::adapters::ui::draw_fullscreen_stage(ctx, state, engine);
        });
        out.shapes.len()
    };
    assert_eq!(painted(&mut state, &engine.read()), 0);
    state.open_fullscreen_stage("blank");
    assert!(painted(&mut state, &engine.read()) > 0);
}

fn run_normal_frame_with_input(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    raw: egui::RawInput,
) {
    let ctx = egui::Context::default();
    drop(ctx.run(raw, |ctx| {
        draw_popups(
            ctx,
            state,
            engine,
            &[],
            crate::model::PhysicalRect {
                x: crate::model::PhysicalPx(0.0),
                y: crate::model::PhysicalPx(0.0),
                width: crate::model::PhysicalPx(1920.0),
                height: crate::model::PhysicalPx(1080.0),
            },
            1.0,
        );
    }));
}

fn screen_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    }
}

/// 팝업의 공개 위치·크기로 타이틀바 버튼 좌표를 계산한다.
fn fullscreen_btn_center(pos: egui::Pos2, size: egui::Vec2) -> egui::Pos2 {
    let title_h = crate::adapters::ui::popup::title_bar_height().value();
    let btn = 20.0;
    let close_center_x = pos.x + size.x - btn * 0.5 - 4.0;
    egui::pos2(
        close_center_x - btn - crate::adapters::ui::popup::title_btn_gap(),
        pos.y + title_h * 0.5,
    )
}

#[test]
fn clicking_the_popup_fullscreen_button_opens_the_stage_and_keeps_the_popup() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    state.popups.open_at_focused(
        "notifications",
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(400.0, 300.0),
    ); // intent-exempt: 테스트 하네스.

    // 등록 시 적용한 배율이 포함된 크기를 읽는다.
    let (pos, size) = {
        let p = state.popups.get_mut("notifications").expect("등록된 popup");
        (p.pos, p.size)
    };

    let mut raw = screen_input();
    raw.events.push(egui::Event::PointerButton {
        pos: fullscreen_btn_center(pos, size),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_normal_frame_with_input(&mut state, &engine.read(), raw);

    assert_eq!(
        state.fullscreen_stage_id(),
        Some(fullscreen::notifications::NOTIFICATIONS_STAGE_ID)
    );
    assert!(
        state.popups.is_open("notifications"),
        "무대를 열어도 원본 팝업은 열린 상태로 남아야 한다"
    );
}

#[test]
fn the_same_click_on_a_popup_without_the_flag_does_nothing() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    state.popups.open_at_focused(
        "rename",
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(400.0, 300.0),
    ); // intent-exempt: 테스트 하네스.
    let (pos, size) = {
        let p = state.popups.get_mut("rename").expect("등록된 popup");
        (p.pos, p.size)
    };

    let mut raw = screen_input();
    raw.events.push(egui::Event::PointerButton {
        pos: fullscreen_btn_center(pos, size),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_normal_frame_with_input(&mut state, &engine.read(), raw);

    assert!(!state.fullscreen_stage_active());
}

// 같은 egui Context에서도 원본 팝업과 무대의 스크롤 상태를 구별해야 한다.
#[test]
fn the_stage_scroll_state_is_a_different_entry_from_the_popups() {
    type ScrollState = egui::containers::scroll_area::State;
    let scroll_entries = |ctx: &egui::Context| ctx.memory(|m| m.data.count::<ScrollState>());

    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    let ctx = egui::Context::default();

    state.popups.open_at_focused(
        "notifications",
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(400.0, 300.0),
    ); // intent-exempt: 테스트 하네스.
    drop(ctx.run(screen_input(), |ctx| {
        draw_popups(
            ctx,
            &mut state,
            &engine.read(),
            &[],
            crate::model::PhysicalRect {
                x: crate::model::PhysicalPx(0.0),
                y: crate::model::PhysicalPx(0.0),
                width: crate::model::PhysicalPx(1920.0),
                height: crate::model::PhysicalPx(1080.0),
            },
            1.0,
        );
    }));
    assert_eq!(
        scroll_entries(&ctx),
        1,
        "비교할 팝업 스크롤 상태가 만들어져야 한다"
    );

    assert!(state.open_fullscreen_stage(fullscreen::notifications::NOTIFICATIONS_STAGE_ID));
    drop(ctx.run(screen_input(), |ctx| {
        crate::adapters::ui::draw_fullscreen_stage(ctx, &mut state, &engine.read());
    }));
    let stage_scroll_id = fullscreen::notifications::recorded_scroll_id(&ctx)
        .expect("무대 콘텐츠가 자기 스크롤 상태 id 를 기록해야 한다");
    assert!(
        ctx.data_mut(|d| d.get_persisted::<ScrollState>(stage_scroll_id))
            .is_some()
    );
    assert_eq!(
        scroll_entries(&ctx),
        2,
        "무대와 원본 팝업이 같은 스크롤 상태를 사용한다"
    );
}

// 서로 다른 무대끼리도 상태가 분리돼야 하므로 무대 ID를 바꿔 검사한다.
#[test]
fn two_stages_with_the_same_content_do_not_share_scroll_state() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    let ctx = egui::Context::default();
    let draw_stage = |state: &mut crate::state::RequestContext,
                      engine: &crate::runtime::engine_read::EngineRead<'_>,
                      id: &'static str| {
        assert!(state.open_fullscreen_stage(id));
        drop(ctx.run(screen_input(), |ctx| {
            crate::adapters::ui::draw_fullscreen_stage(ctx, state, engine);
        }));
        fullscreen::notifications::recorded_scroll_id(&ctx).expect("무대가 id 를 기록해야 한다")
    };

    let a = draw_stage(
        &mut state,
        &engine.read(),
        fullscreen::notifications::NOTIFICATIONS_STAGE_ID,
    );
    let b = draw_stage(
        &mut state,
        &engine.read(),
        fullscreen::defs::TEST_TWIN_STAGE_ID,
    );
    assert_ne!(a, b, "서로 다른 무대가 같은 스크롤 상태를 사용한다");
}

#[test]
fn notifications_stage_clears_its_own_scroll_state_on_close() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    assert!(state.open_fullscreen_stage(fullscreen::notifications::NOTIFICATIONS_STAGE_ID));

    let ctx = egui::Context::default();
    drop(ctx.run(screen_input(), |ctx| {
        crate::adapters::ui::draw_fullscreen_stage(ctx, &mut state, &engine.read());
    }));
    let scroll_id = fullscreen::notifications::recorded_scroll_id(&ctx)
        .expect("무대 콘텐츠가 자기 스크롤 상태 id 를 기록해야 한다");
    assert!(
        ctx.data_mut(|d| d.get_persisted::<egui::containers::scroll_area::State>(scroll_id))
            .is_some(),
        "정리할 ScrollArea 상태가 만들어져야 한다"
    );

    assert!(state.close_fullscreen_stage());
    drop(ctx.run(screen_input(), |ctx| {
        crate::adapters::ui::draw_popups(
            ctx,
            &mut state,
            &engine.read(),
            &[],
            crate::model::PhysicalRect {
                x: crate::model::PhysicalPx(0.0),
                y: crate::model::PhysicalPx(0.0),
                width: crate::model::PhysicalPx(1920.0),
                height: crate::model::PhysicalPx(1080.0),
            },
            1.0,
        );
    }));
    assert!(fullscreen::notifications::recorded_scroll_id(&ctx).is_none());
    assert!(
        ctx.data_mut(|d| d.get_persisted::<egui::containers::scroll_area::State>(scroll_id))
            .is_none(),
        "무대 자체 상태가 종료 후에도 남아 있다"
    );
}
