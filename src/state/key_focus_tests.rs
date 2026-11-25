//! host 키 포커스 판정(`key_focus_surface`)과 프레임 값, 그 값을 읽는 강조 해제·soft 점유 정리·plugin 이벤트 감지.
//! 창(winit·GPU)을 띄우지 않고 창이 부르는 같은 함수를 지금 상태로 부른다.

use crate::adapters::ui::popup::{PopupInstanceId, PopupScope};
use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
use crate::core::state::attention::AttentionKind;
use crate::runtime::engine_session::EngineSession;
use crate::state::RequestContext;

/// 터미널 하나가 선택된 장면과 그 surface ID.
fn scene() -> (RequestContext, EngineSession, u32) {
    crate::i18n::init("en");
    let (state, mut session) = crate::state::tests::test_state();
    let sid = session
        .borrow_mut()
        .workspace_at(0)
        .expect("workspace")
        .all_surface_ids()[0];
    assert_eq!(
        state.focused_surface_id(&session.borrow_mut().read()),
        Some(sid),
        "the terminal is selected"
    );
    (state, session, sid)
}

/// 창 범위 팝업(알림 목록)을 키 포커스를 주며 연다.
fn open_window_popup(state: &mut RequestContext) -> PopupInstanceId {
    state
        .popups
        .open_centered_focused("notifications", PopupScope::Window)
        .expect("opens")
}

/// 선택 surface 범위의 검색 바를 키 포커스를 주며 연다.
fn open_search_bar(state: &mut RequestContext, session: &mut EngineSession, sid: u32) {
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(sid))
                .focused(),
            true,
        )
        .expect("the search bar opens");
    state.normalize_focus(&session.borrow_mut().read());
}

fn key_focus(state: &RequestContext, session: &mut EngineSession) -> Option<u32> {
    state.key_focus_surface(&session.borrow_mut().read())
}

/// 선택 surface의 본문에 키가 있으면 그 surface가 키 포커스다. 키를 가진 팝업이 열리면 범위와 무관하게 없고,
/// 닫히면 돌아온다.
#[test]
fn a_popup_that_holds_key_focus_leaves_no_surface_with_key_focus() {
    let (mut state, mut session, sid) = scene();
    assert_eq!(key_focus(&state, &mut session), Some(sid));

    open_window_popup(&mut state);
    assert_eq!(key_focus(&state, &mut session), None, "window scope");
    state.popups.close("notifications");
    assert_eq!(key_focus(&state, &mut session), Some(sid), "closed");

    open_search_bar(&mut state, &mut session, sid);
    assert_eq!(key_focus(&state, &mut session), None, "surface scope");
    assert_eq!(
        state.focused_surface_id(&session.borrow_mut().read()),
        Some(sid),
        "the selection stays"
    );
}

/// 주소 띠에 키가 있으면 그 surface가 선택일 때만 키 포커스다. 선택이 아닌 surface의 띠는 키 포커스를 만들지 않는다.
#[test]
fn an_address_band_gives_key_focus_only_to_the_selected_surface() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    assert_eq!(key_focus(&state, &mut session), Some(201), "the selection");
    state.html_address.set_focused_for_test(201);
    assert_eq!(key_focus(&state, &mut session), Some(201), "its own band");
    state.html_address.set_focused_for_test(101);
    assert_eq!(
        key_focus(&state, &mut session),
        None,
        "another surface's band"
    );
}

/// 복원 안내가 열리기만 하면 선택 surface가 키 포커스다. 안쪽 컨트롤이 키를 가지면 키 포커스 surface가 없고,
/// 칠하기·강조 해제·mesh focused가 함께 따른다.
#[test]
fn restore_notice_controls_with_keys_leave_no_surface_with_key_focus() {
    use crate::adapters::ui::restore_notice::{RestoreNoticeEntry, open_for_test};
    use crate::state::surface_focus::TerminalFocusPaint;
    let entry = || RestoreNoticeEntry {
        id: 1,
        title: "title".into(),
        cause: "cause".into(),
        label: "label".into(),
        path: "/a".into(),
        body: "body".into(),
    };
    let (mut state, mut session, sid) = scene();
    open_for_test(&mut state, entry(), false);
    assert_eq!(key_focus(&state, &mut session), Some(sid), "shown, no keys");

    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::Completion);
    open_for_test(&mut state, entry(), true);
    assert_eq!(key_focus(&state, &mut session), None, "keys inside");
    assert!(!observe(&mut state, &mut session, true), "no observation");
    assert_eq!(
        attention(&mut session, sid),
        Some(AttentionKind::Completion)
    );
    let occupancy = crate::core::attach::OccupancyRegistry::new();
    let paint = TerminalFocusPaint::of(state.frame_key_focus(), sid, &occupancy, || false);
    assert!(!paint.bed_focused);
    assert!(!paint.show_cursor);
    assert!(!super::told_focused(state.frame_key_focus(), sid));
}

/// 키 포커스를 주지 않고 연 팝업(agent가 연 passive 팝업)은 키 포커스를 바꾸지 않는다. surface에 있던 키도, 다른
/// 팝업에 있던 키도 그대로다.
#[test]
fn a_popup_opened_without_focus_keeps_key_focus_where_it_was() {
    let (mut state, mut session, sid) = scene();
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .expect("opens passive");
    state.normalize_focus(&session.borrow_mut().read());
    assert_eq!(key_focus(&state, &mut session), Some(sid), "on the surface");
    state.popups.close("notifications");

    open_search_bar(&mut state, &mut session, sid);
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .expect("opens passive");
    state.normalize_focus(&session.borrow_mut().read());
    assert_eq!(
        key_focus(&state, &mut session),
        None,
        "still on the search bar"
    );
}

/// 설정 창 열기 요청은 다른 OS 창으로 가는 중이라 선택 surface의 키 포커스를 그대로 둔다. 튜토리얼 말풍선이 키를
/// 가지면 키 포커스 surface가 없다.
#[test]
fn a_settings_request_keeps_and_the_tutorial_bubble_takes_key_focus() {
    let (mut state, mut session, sid) = scene();
    state.settings_open_requested = true;
    assert_eq!(
        key_focus(&state, &mut session),
        Some(sid),
        "settings request"
    );
    state.settings_open_requested = false;

    state.tutorial.request_start(0);
    state.tutorial.start_pending();
    assert!(state.tutorial.active.is_some(), "the tutorial runs");
    state.tutorial.keyboard_focus = true;
    assert_eq!(key_focus(&state, &mut session), None, "tutorial bubble");
    state.tutorial.keyboard_focus = false;
    assert_eq!(
        key_focus(&state, &mut session),
        Some(sid),
        "bubble without keys"
    );
}

/// 프레임 값은 프레임 첫머리에 정한 값이다. 그리기 중 키가 팝업으로 가면 프레임 값은 그대로이고 다음 프레임을
/// 그리게 한다.
#[test]
fn the_frame_value_holds_for_the_frame_and_asks_for_the_next_one_when_focus_moves() {
    let (mut state, mut session, sid) = scene();
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    assert_eq!(state.frame_key_focus(), Some(sid));
    assert!(!state.key_focus_moved_since_frame(&session.borrow_mut().read()));

    open_window_popup(&mut state);
    assert_eq!(
        state.frame_key_focus(),
        Some(sid),
        "this frame keeps its value"
    );
    assert!(state.key_focus_moved_since_frame(&session.borrow_mut().read()));
    state.finish_frame_key_focus(&session.borrow_mut().read(), true);

    state.refresh_frame_key_focus(&session.borrow_mut().read());
    assert_eq!(state.frame_key_focus(), None, "the next frame");
    assert!(!state.key_focus_moved_since_frame(&session.borrow_mut().read()));
}

/// 준비 단계를 건너뛴 그리기는 그 프레임에 아직 정하지 않았을 때만 값을 정한다.
#[test]
fn a_draw_without_preparation_fills_the_frame_value_once() {
    let (mut state, mut session, sid) = scene();
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    open_window_popup(&mut state);
    state.ensure_frame_key_focus(&session.borrow_mut().read());
    assert_eq!(state.frame_key_focus(), Some(sid), "already filled");
    state.finish_frame_key_focus(&session.borrow_mut().read(), true);
    state.ensure_frame_key_focus(&session.borrow_mut().read());
    assert_eq!(state.frame_key_focus(), None, "a new frame");
}

/// 바깥 소비자가 읽는 값은 화면에 낸 프레임의 것이다. 제출에 실패한 프레임은 값을 바꾸지 않는다. 바탕 값은
/// 내용을 숨기는 점유 중이면 없다.
#[test]
fn the_shown_bed_follows_only_frames_that_were_shown() {
    let (mut state, mut session, sid) = scene();
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.shown_bed_focus(), Some(sid));

    open_window_popup(&mut state);
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    state.finish_frame_key_focus(&session.borrow_mut().read(), false);
    assert_eq!(state.shown_bed_focus(), Some(sid), "not shown");
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.shown_bed_focus(), None, "shown under the popup");

    state.popups.close("notifications");
    session
        .borrow_mut()
        .live
        .occupancy
        .acquire(sid, 41)
        .expect("occupy");
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.shown_bed_focus(), None, "hard occupied");
}

/// plugin `surface.focused`는 화면에 낸 프레임에서 키 포커스가 surface로 들어갈 때마다 하나다. 팝업으로 가는 전이는
/// 이벤트가 없고, 돌아오면 앞 값 없이 다시 온다. 두 전이 사이에 감지가 없어도 놓치지 않는다.
#[test]
fn surface_focused_follows_shown_key_focus_entries() {
    let (mut state, mut session, sid) = scene();
    let focused = |state: &mut RequestContext| -> Vec<(u32, Option<u32>)> {
        state
            .detect_focus_change()
            .into_iter()
            .filter_map(|e| match e {
                crate::state::PendingHostEvent::SurfaceFocused {
                    surface_id,
                    prev_surface_id,
                } => Some((surface_id, prev_surface_id)),
                _ => None,
            })
            .collect()
    };
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(focused(&mut state), vec![(sid, None)]);

    open_window_popup(&mut state);
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert!(focused(&mut state).is_empty(), "no event for the popup");
    state.popups.close("notifications");
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(
        focused(&mut state),
        vec![(sid, None)],
        "back from the popup"
    );

    open_window_popup(&mut state);
    state.show_key_focus_frame(&session.borrow_mut().read());
    state.popups.close("notifications");
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(
        focused(&mut state),
        vec![(sid, None)],
        "two transitions between polls"
    );
}

/// 키 포커스가 다른 surface로 옮겨 가면 그 진입의 앞 값은 마지막으로 낸 프레임의 surface다.
#[test]
fn a_key_focus_entry_carries_the_surface_keys_left() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.take_key_focus_entries(), vec![(201, None)]);

    assert!(state.focus_surface_by_id(&session.borrow_mut().read(), 101));
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.take_key_focus_entries(), vec![(101, Some(201))]);
}

/// 탭 제목의 OSC 제목 갱신은 선택 변경을 따른다. 키를 가진 팝업 아래에서도 선택이 바뀌면 대상이 나온다.
#[test]
fn the_osc_title_refresh_follows_the_selection_not_key_focus() {
    let (mut state, mut session, sid) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(
        state.detect_selection_change(&read),
        Some(sid),
        "first poll"
    );
    assert_eq!(state.detect_selection_change(&read), None, "unchanged");
    open_window_popup(&mut state);
    assert_eq!(
        state.detect_selection_change(&read),
        None,
        "a popup is not a selection change"
    );
}

/// 이 프레임의 FocusObserved를 만들어 창의 Inputs 단계처럼 적용한다.
fn observe(state: &mut RequestContext, session: &mut EngineSession, window_focused: bool) -> bool {
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    let effect = crate::view::main::redraw::focus_observed_effect(
        state,
        &session.borrow_mut().read(),
        window_focused,
    );
    let Some(effect) = effect else {
        return false;
    };
    state.dispatch_intent(
        crate::intent::Intent::Frame(effect.into()).from_user_menu("render-focus"),
    );
    for intent in state.take_pending_intents() {
        if let crate::intent::Intent::Frame(effect) = &intent.body {
            effect.apply(&mut session.borrow_mut(), None, &intent.origin);
        }
    }
    true
}

fn attention(session: &mut EngineSession, sid: u32) -> Option<AttentionKind> {
    session.borrow_mut().attention_kind(sid)
}

fn has_soft(session: &mut EngineSession, sid: u32) -> bool {
    session
        .borrow_mut()
        .live
        .occupancy
        .occupancy_of(sid)
        .is_some()
}

/// 부모 surface가 트리에 없는 soft 점유를 둔다.
fn orphan_soft(session: &mut EngineSession, sid: u32) {
    session
        .borrow_mut()
        .occupy_soft(sid, 987_654u32, None)
        .expect("soft");
}

/// 키를 가진 팝업 아래에서는 강조와 고아 soft 점유가 남는다. 키가 돌아온 프레임에 둘 다 정리된다.
#[test]
fn highlights_stay_under_a_focused_popup_and_clear_when_key_focus_returns() {
    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::Completion);
    orphan_soft(&mut session, sid);
    open_window_popup(&mut state);
    assert!(!observe(&mut state, &mut session, true), "no observation");
    assert_eq!(
        attention(&mut session, sid),
        Some(AttentionKind::Completion)
    );
    assert!(has_soft(&mut session, sid));

    state.popups.close("notifications");
    assert!(observe(&mut state, &mut session, true));
    assert_eq!(attention(&mut session, sid), None, "cleared");
    assert!(!has_soft(&mut session, sid), "orphan dropped");
}

/// 키를 가져가지 않는 팝업 아래에서는 지금처럼 정리된다.
#[test]
fn a_popup_without_focus_does_not_hold_back_the_clearing() {
    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::NeedsInput);
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .expect("opens passive");
    assert!(observe(&mut state, &mut session, true));
    assert_eq!(attention(&mut session, sid), None);
}

/// OS 창이 비활성이면 강조는 남고 고아 soft 점유만 정리된다.
#[test]
fn an_inactive_os_window_drops_the_orphan_and_keeps_the_highlight() {
    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::Completion);
    orphan_soft(&mut session, sid);
    assert!(observe(&mut state, &mut session, false));
    assert_eq!(
        attention(&mut session, sid),
        Some(AttentionKind::Completion)
    );
    assert!(!has_soft(&mut session, sid));
}

/// hard 점유 중에는 고아 soft 정리가 미뤄지고, 점유가 풀린 첫 관측에서 지워진다.
#[test]
fn hard_occupancy_delays_the_orphan_drop_until_it_ends() {
    let (mut state, mut session, sid) = scene();
    orphan_soft(&mut session, sid);
    session
        .borrow_mut()
        .live
        .occupancy
        .acquire(sid, 41)
        .expect("hard");
    assert!(observe(&mut state, &mut session, true));
    assert!(
        session
            .borrow_mut()
            .live
            .occupancy
            .occupancy_of(sid)
            .is_some(),
        "hard shows"
    );
    session.borrow_mut().force_detach(sid).expect("released");
    assert!(has_soft(&mut session, sid), "the orphan waited");
    assert!(observe(&mut state, &mut session, true));
    assert!(
        !has_soft(&mut session, sid),
        "dropped after the hard occupancy"
    );
}

/// 점유 workspace의 멤버(surface lock 없음)는 칠하기와 링이 hard로 본다. 강조 해제와 soft 정리도 미룬다. 점유 전에는
/// 같은 장면에서 정리된다.
#[test]
fn an_occupied_workspace_member_holds_back_clearing_like_a_hard_lock() {
    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::Completion);
    orphan_soft(&mut session, sid);
    session
        .borrow_mut()
        .live
        .occupancy
        .acquire_workspace(7, &[], &[sid], 41)
        .expect("workspace");
    assert!(!session.borrow_mut().live.occupancy.is_hard_occupied(sid));
    assert!(observe(&mut state, &mut session, true));
    assert_eq!(
        attention(&mut session, sid),
        Some(AttentionKind::Completion),
        "held back"
    );
    assert!(has_soft(&mut session, sid), "held back");

    session.borrow_mut().force_detach(sid).expect("released");
    assert!(observe(&mut state, &mut session, true));
    assert_eq!(attention(&mut session, sid), None);
    assert!(!has_soft(&mut session, sid));
}

/// mesh에 알리는 값은 점유와 무관하게 키 포커스만 따르고, 값이 바뀔 때 다시 알린다.
#[test]
fn mesh_focus_is_told_from_key_focus_on_every_change() {
    use super::{focus_changed, told_focused};
    assert!(told_focused(Some(3), 3));
    assert!(!told_focused(None, 3), "a popup holds key focus");
    assert!(!told_focused(Some(4), 3));
    assert!(focus_changed(None, false), "first context");
    assert!(focus_changed(Some(true), false), "the popup took key focus");
    assert!(focus_changed(Some(false), true), "key focus came back");
    assert!(!focus_changed(Some(true), true));
}

/// 검색 바가 키를 가지면 밑 터미널은 unfocused 바탕·활성 일치 잉크이고 PTY 커서를 그리지 않는다.
#[test]
fn the_terminal_under_a_focused_search_bar_paints_unfocused() {
    use crate::state::surface_focus::TerminalFocusPaint;
    let (mut state, mut session, sid) = scene();
    open_search_bar(&mut state, &mut session, sid);
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    let occupancy = crate::core::attach::OccupancyRegistry::new();
    let paint = TerminalFocusPaint::of(state.frame_key_focus(), sid, &occupancy, || false);
    assert!(!paint.bed_focused);
    assert!(!paint.show_cursor);
    let theme = tasty_themes::mocha_fallback();
    let ink = crate::renderer::SearchHighlights::for_theme(
        &theme,
        paint.bed_focused,
        Default::default(),
        &[],
        0,
    )
    .active_fg;
    assert_eq!(ink, theme.search_match_active_unfocused_fg().to_gpu_rgba());

    state.popups.close(SEARCH_BAR_POPUP_ID);
    state.refresh_frame_key_focus(&session.borrow_mut().read());
    let back = TerminalFocusPaint::of(state.frame_key_focus(), sid, &occupancy, || false);
    assert!(back.bed_focused && back.show_cursor, "key focus came back");
    let suppressed = TerminalFocusPaint::of(state.frame_key_focus(), sid, &occupancy, || true);
    assert!(
        suppressed.cursor_suppressed && !suppressed.show_cursor,
        "output suppression stays"
    );
}

/// 같은 surface가 키 포커스를 유지한 채 프레임을 더 내도 진입은 새로 생기지 않는다(plugin `surface.focused`를 매
/// 프레임 내지 않는다).
#[test]
fn frames_that_keep_key_focus_add_no_entry() {
    let (mut state, mut session, sid) = scene();
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert_eq!(state.take_key_focus_entries(), vec![(sid, None)]);
    state.show_key_focus_frame(&session.borrow_mut().read());
    state.show_key_focus_frame(&session.borrow_mut().read());
    assert!(state.take_key_focus_entries().is_empty());
}

/// 제출하지 못한 프레임은 진입을 넣지 않고 낸 값도 바꾸지 않는다. 다음에 낸 프레임이 진입을 넣는다.
#[test]
fn a_frame_that_was_not_shown_adds_no_entry() {
    let (mut state, mut session, sid) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    state.refresh_frame_key_focus(&read);
    state.finish_frame_key_focus(&read, false);
    assert!(state.take_key_focus_entries().is_empty(), "not shown");
    assert_eq!(state.shown_bed_focus(), None, "nothing shown yet");
    state.show_key_focus_frame(&read);
    assert_eq!(state.take_key_focus_entries(), vec![(sid, None)]);
    assert_eq!(state.shown_bed_focus(), Some(sid));
}

/// 전체화면 무대가 열린 동안 키 포커스 surface가 없고 강조 해제 효과도 없다. 무대가 닫히면 돌아온다.
#[test]
fn the_fullscreen_stage_leaves_no_surface_with_key_focus() {
    let (mut state, mut session, sid) = scene();
    session
        .borrow_mut()
        .raise_attention(sid, AttentionKind::Completion);
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(key_focus(&state, &mut session), None);
    assert!(!observe(&mut state, &mut session, true), "no observation");
    assert_eq!(
        attention(&mut session, sid),
        Some(AttentionKind::Completion)
    );
    assert!(state.close_fullscreen_stage());
    assert_eq!(key_focus(&state, &mut session), Some(sid));
}

/// App이 창마다 도는 두 단계(`observe_window_focus`, 세션별 `drain_session_host_events`): 키를 가진 팝업 아래에서
/// 선택을 바꾸면 탭의 OSC 제목을 새 선택 surface 터미널의 실제 제목으로 다시 읽고, 팝업이 키를 돌려줄 때(선택 변경
/// 없음)는 drain까지 마친 뒤에도 다시 읽지 않는다. 키 포커스 진입은 `surface.focused`로 drain 결과에 나온다. 두 터미널은
/// 서로 다른 OSC 제목을 가지고, 캐시에 남긴 다른 값으로 다시 읽었는지 가린다.
#[test]
fn the_window_focus_pass_refreshes_titles_on_selection_and_queues_key_focus_entries() {
    use crate::app::dispatch::host_events::{drain_session_host_events, observe_window_focus};
    use crate::state::PendingHostEvent;
    crate::i18n::init("en");
    let services = crate::adapters::ipc::handler::cli_entry_tests::test_core();
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    for sid in [101, 201] {
        let mut engine = session.borrow_mut();
        let terminal = engine.find_terminal_by_id_mut(sid).expect("a terminal");
        terminal.feed_bytes(format!("\x1b]2;osc {sid}\x07").as_bytes());
        assert_eq!(terminal.current_title(), Some(format!("osc {sid}")));
    }
    let set_title = |session: &mut EngineSession, sid: u32| {
        session
            .borrow_mut()
            .live
            .surface_titles
            .entry(sid)
            .or_default()
            .osc_title = Some(format!("stale {sid}"));
    };
    let title = |session: &mut EngineSession, sid: u32| {
        session
            .borrow_mut()
            .live
            .surface_titles
            .get(&sid)
            .and_then(|t| t.osc_title.clone())
    };
    let focused = |session: &mut EngineSession| -> Vec<(u32, Option<u32>)> {
        drain_session_host_events(&services, &mut session.borrow_mut())
            .into_iter()
            .filter_map(|e| match e {
                PendingHostEvent::SurfaceFocused {
                    surface_id,
                    prev_surface_id,
                } => Some((surface_id, prev_surface_id)),
                _ => None,
            })
            .collect()
    };
    state.show_key_focus_frame(&session.borrow_mut().read());
    observe_window_focus(&mut state, &mut session.borrow_mut());
    assert_eq!(focused(&mut session), vec![(201, None)], "first entry");

    open_window_popup(&mut state);
    state.show_key_focus_frame(&session.borrow_mut().read());
    set_title(&mut session, 101);
    assert!(state.focus_surface_by_id(&session.borrow_mut().read(), 101));
    observe_window_focus(&mut state, &mut session.borrow_mut());
    assert_eq!(
        title(&mut session, 101),
        Some("osc 101".into()),
        "the new selection's own OSC title"
    );
    assert!(focused(&mut session).is_empty(), "no entry under the popup");

    set_title(&mut session, 101);
    state.popups.close("notifications");
    state.show_key_focus_frame(&session.borrow_mut().read());
    observe_window_focus(&mut state, &mut session.borrow_mut());
    assert_eq!(focused(&mut session), vec![(101, None)], "keys came back");
    assert_eq!(
        title(&mut session, 101),
        Some("stale 101".into()),
        "keys came back without a selection change, drain included"
    );
}

/// 대응표: 무대·키를 가진 팝업·튜토리얼은 없음, 설정 요청은 안내와 무관하게 선택, 안내가 키를 가지면 없음, 주소 띠는
/// 선택일 때만 그 surface, surface 노드는 그 값.
#[test]
fn the_key_focus_table_maps_each_focus_node() {
    use super::key_focus_of;
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    let sel = Some(7);
    for notice in [false, true] {
        assert_eq!(key_focus_of(FocusNode::Stage, notice, sel), None, "stage");
        assert_eq!(
            key_focus_of(FocusNode::Popup(PopupNode::Plugin(3)), notice, sel),
            None,
            "popup"
        );
        assert_eq!(
            key_focus_of(FocusNode::SettingsRequest, notice, sel),
            sel,
            "settings request"
        );
        assert_eq!(
            key_focus_of(FocusNode::Tutorial, notice, sel),
            None,
            "tutorial"
        );
    }
    assert_eq!(key_focus_of(FocusNode::HtmlAddress(7), false, sel), Some(7));
    assert_eq!(key_focus_of(FocusNode::HtmlAddress(8), false, sel), None);
    assert_eq!(key_focus_of(FocusNode::HtmlAddress(7), true, sel), None);
    assert_eq!(
        key_focus_of(FocusNode::Surface(Some(7)), false, sel),
        Some(7)
    );
    assert_eq!(key_focus_of(FocusNode::Surface(Some(7)), true, sel), None);
    assert_eq!(key_focus_of(FocusNode::Surface(None), false, None), None);
}

/// 키 포커스는 창의 포커스 노드를 읽는다. 키를 가진 팝업이 포커스 노드면 선택 surface가 그대로여도 키 포커스가 없다.
#[test]
fn key_focus_reads_the_window_focus_node_of_a_focused_popup() {
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session, sid) = scene();
    open_window_popup(&mut state);
    state.normalize_focus(&session.borrow_mut().read());
    assert!(matches!(
        state.focus_node(&session.borrow_mut().read()),
        FocusNode::Popup(_)
    ));
    assert_eq!(
        state.focused_surface_id(&session.borrow_mut().read()),
        Some(sid)
    );
    assert_eq!(key_focus(&state, &mut session), None);
}

/// 포커스 노드가 선택 surface면 키 포커스는 그 surface다.
#[test]
fn key_focus_of_a_surface_focus_node_is_that_surface() {
    use crate::state::focus_node::FocusNode;
    let (state, mut session, sid) = scene();
    assert_eq!(
        state.focus_node(&session.borrow_mut().read()),
        FocusNode::Surface(Some(sid))
    );
    assert_eq!(key_focus(&state, &mut session), Some(sid));
}
