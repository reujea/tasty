//! 그리기 중 전환의 봉인(`MainViewState::seal_after_drawing_transition`)이 몫 칸에 적는 값. 봉인은 세 저장 위치(GPU 끊은
//! 자리, 팝업 관리자의 뒤 몫, 뒤 큐)에 한 칸씩 넣고 새 `at` 은 남은 원시 길이다. 닫는 몫의 노드는 채우지 않고 새 몫에는
//! 표시가 없다. 사건·노드·결속이 없는 열린 몫은 봉인하지 않고 그 표시를 지운다. 열린 화면 칸은 끊기·패스 닫기·봉인
//! 세 자리에서 적힌다. 그리기 중 전환은 패스 콜백에서 무대를 여닫아 모사한다(`stage_or_window_pass_with`).

use super::*;
use crate::state::focus_node::SegmentSlots;

const BLANK: Option<&str> = Some("blank");

/// L 탐색기가 포커스 노드이고 모든 버튼이 올라간 장면.
fn scene() -> (Scene, Explorer, Explorer, WindowInput) {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
        input
            .queue_mut()
            .push(release_at(surface_rect_l().center()));
        drain(&mut s, &mut input, &mut bodies);
    }
    assert!(!s.ctx.input(|i| i.pointer.any_down()));
    (s, l, r, input)
}

fn press(s: &mut Scene, input: &mut WindowInput, pos: egui::Pos2) {
    let engine = s.session.borrow_mut();
    input.press(&mut s.state, &engine.read(), pos);
}

fn key(s: &mut Scene, input: &mut WindowInput, k: egui::Key) {
    let engine = s.session.borrow_mut();
    assert!(
        input
            .key(&mut s.state, &engine.read(), false, key_press(k))
            .is_some()
    );
}

fn slot(
    screen: Option<&'static str>,
    marked: bool,
    focus: Option<FocusNode>,
    press: bool,
) -> (Option<&'static str>, bool, Option<FocusNode>, bool) {
    (screen, marked, focus, press)
}

fn shape(slots: &[SegmentSlots]) -> Vec<(Option<&'static str>, bool, Option<FocusNode>, bool)> {
    slots
        .iter()
        .map(|s| (s.screen, s.marked, s.focus, s.press))
        .collect()
}

/// 무대를 연 장면.
fn staged() -> (Scene, Explorer, Explorer, WindowInput) {
    let (mut s, mut l, mut r, mut input) = scene();
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    (s, l, r, input)
}

/// Given 무대, When [무대 누름·뗌(몫 A), 키 K(몫 B), 누름 P(몫 C)] 의 A 를 그리며 무대를 닫으면, Then 봉인은 뒤 큐 꼬리
/// C 를 남은 원시 길이(3)에서 닫는다. 세 저장 위치에 한 칸씩 들고 두 좌표 목록이 같다. B·C 는 전환 앞 표시를 갖고
/// 끊기 때 적은 열린 화면(무대)을 그대로 둔다. 누름만 든 C 의 노드는 채우지 않는다. 새 몫은 지금 화면(일반)에서 열리고
/// 표시·노드·안내 값이 없다.
#[test]
fn a_seal_of_the_queue_tail_pushes_one_slot_at_the_raw_left() {
    let (mut s, mut l, mut r, mut input) = staged();
    let point = screen().center();
    press(&mut s, &mut input, point);
    input.queue_mut().push(release_at(point));
    key(&mut s, &mut input, egui::Key::F9);
    press(&mut s, &mut input, empty_l());
    assert_eq!(input.queue_mut().len(), 6, "A·B·C 의 원시 길이 3·1·2");
    let left_before = s.state.popups.cut_segments_left();
    let cuts_before = input.cut_count();
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage);
    assert_eq!(input.queue_mut().len(), 3, "남은 원시 길이");
    // 패스가 한 칸씩 꺼내고 봉인이 한 칸씩 넣는다.
    assert_eq!(s.state.popups.cut_segments_left(), left_before);
    assert_eq!(input.cut_count(), cuts_before);
    let ats = s.state.popups.cut_ats();
    assert_eq!(
        ats,
        [1, 3],
        "C 는 B 뒤(1), 봉인한 새 몫은 남은 원시 길이(3)"
    );
    assert_eq!(input.cuts_text(), format!("InputCuts({ats:?})"));
    let slots = s.state.segment_slots();
    assert_eq!(
        shape(&slots),
        [
            slot(BLANK, true, Some(FocusNode::Stage), false),
            slot(BLANK, true, None, true),
            slot(None, false, None, false),
        ]
    );
    assert_eq!(slots[2].notice, None, "새 몫에는 안내 값이 없다");
}

/// Given 무대, When [무대 클릭 A, 키 K] 의 A 를 그리며 무대를 닫으면(뒤 큐가 빈다), Then 봉인은 승계된 지금 몫 K 를
/// 남은 원시 길이(1)에서 닫고 새 몫을 뒤 큐에 둔다. When 그 뒤 K 를 그리는 패스가 다시 무대를 열면, Then 사건·노드·
/// 결속이 없는 열린 몫(새 몫)은 봉인하지 않고 표시를 지워 열린 화면을 무대로 바꾼다. 끊은 자리는 늘지 않는다. When 그
/// 몫을 그려 닫으면, Then 다음 지금 몫은 패스 닫기가 지금 화면(무대)으로 연다.
#[test]
fn a_seal_of_the_inherited_segment_then_a_bare_open_segment_loses_its_mark() {
    let (mut s, mut l, mut r, mut input) = staged();
    let point = screen().center();
    press(&mut s, &mut input, point);
    input.queue_mut().push(release_at(point));
    key(&mut s, &mut input, egui::Key::F9);
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage);
    assert_eq!(input.queue_mut().len(), 1, "K 의 원시 길이");
    assert_eq!(s.state.popups.cut_ats(), [1]);
    assert_eq!(input.cuts_text(), "InputCuts([1])");
    assert_eq!(
        shape(&s.state.segment_slots()),
        [
            slot(BLANK, true, Some(FocusNode::Stage), false),
            slot(None, false, None, false),
        ]
    );

    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.open_fullscreen_stage("blank"));
    });
    assert!(!stage, "K 몫은 일반 화면으로 그린다");
    assert!(s.state.popups.cut_ats().is_empty());
    assert_eq!(input.cut_count(), 0, "빈 열린 몫은 봉인하지 않는다");
    assert_eq!(
        shape(&s.state.segment_slots()),
        [slot(BLANK, false, None, false)]
    );

    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    assert_eq!(
        shape(&s.state.segment_slots()),
        [slot(BLANK, false, None, false)],
        "패스 닫기가 지금 화면으로 연다"
    );
}

/// Given 무대, When 한 묶음에 [무대 누름(몫 A), 키 K, 같은 버튼 뗌(L 의 banana 줄 자리)] 이 오고 A 를 그리며 무대를
/// 닫으면(R2-20 의 단일 버튼), Then K 몫을 일반 화면으로 그릴 때 그 뗌은 egui 의 눌림만 풀고 위젯 클릭을 만들지 않는다.
/// 다른 버튼이 눌린 채인 다중 버튼의 클릭 집계(`pointer.button_clicked`·전역 클릭 시각)는 egui 공개 API 로 막을 수 없는
/// 한계라 이 시험의 범위 밖이고 단정하지 않는다.
#[test]
fn a_single_button_release_after_the_stage_closed_ends_the_press_without_a_click() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    l.actions.clear();
    press(&mut s, &mut input, banana);
    key(&mut s, &mut input, egui::Key::F9);
    input.queue_mut().push(release_at(banana));
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage);
    assert!(
        s.ctx.input(|i| i.pointer.any_down()),
        "A 의 누름을 무대가 쥐었다"
    );
    let mut rest = Vec::new();
    while !input.is_empty() {
        rest.push(stage_or_window_pass_with(
            &mut s,
            &mut input,
            &mut l,
            &mut r,
            &mut |_| {},
        ));
    }
    assert!(rest.iter().all(|(stage, _)| !stage));
    assert!(
        rest.iter().any(|(_, e)| e
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: false, .. }))),
        "뗌은 남아 눌림을 푼다: {rest:?}"
    );
    assert!(!s.ctx.input(|i| i.pointer.any_down()), "눌림이 풀렸다");
    assert!(!l.selected().contains("banana"), "L: {:?}", l.selected());
    assert!(
        !l.actions.iter().any(|a| a.contains("OpenFile")),
        "위젯 클릭: {:?}",
        l.actions
    );
}

/// L 의 빈 곳(항목 줄 아래).
fn empty_l() -> egui::Pos2 {
    let rect = surface_rect_l();
    egui::pos2(rect.left() + 24.0, rect.bottom() - 24.0)
}
