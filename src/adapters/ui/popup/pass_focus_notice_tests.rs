//! 복원 안내 안쪽 컨트롤이 키보드 포커스를 가진 동안 친 글자는 탐색기 타입어헤드가 읽지 않는다. 판정은 글자를 분류한
//! 사건 시점의 안내 포커스로 한다. 그리기 때 안내 포커스를 다시 읽으면, 앞 몫의 바깥 누름이 안내 포커스를 푼 뒤 그리는
//! 뒤 몫의 글자를 타입어헤드가 읽는다.

use super::*;
use crate::adapters::ui::restore_notice::{
    RESTORE_NOTICE_POPUP_ID, RestoreNoticeEntry, has_keyboard_focus, push,
};

fn entry() -> RestoreNoticeEntry {
    RestoreNoticeEntry {
        id: 1,
        title: "title".into(),
        cause: "cause".into(),
        label: "label".into(),
        path: "/restored".into(),
        body: "body".into(),
    }
}

/// 안내 카드 안쪽(버튼이 아닌 본문 아래쪽) 한 점.
fn notice_point(s: &Scene) -> egui::Pos2 {
    let rect = s
        .state
        .popups
        .instances_of(RESTORE_NOTICE_POPUP_ID)
        .map(|p| p.popup_rect())
        .next()
        .expect("안내가 떠 있다");
    egui::pos2(rect.center().x, rect.bottom() - 4.0)
}

fn release_at(pos: egui::Pos2) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }
}

/// 장면이 연 검색창을 닫는다(안내 장면은 팝업 키 주인이 없어야 한다).
fn close_search(s: &mut Scene) {
    let search = s.search;
    s.state.popups.close_instance(
        search,
        crate::adapters::ui::popup::PopupCloseReason::Command,
    );
}

/// L 탐색기가 포커스 노드인 채로 안내를 띄우고 안내 안쪽을 눌러 안쪽 컨트롤에 키보드 포커스를 준다. 검색창은 닫는다.
fn notice_focused_scene() -> (Scene, Explorer, Explorer, WindowInput) {
    let mut s = open_scene(|_| {});
    close_search(&mut s);
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
        assert!(push(&mut s.state, entry()));
        for _ in 0..3 {
            pass(&mut s, &mut input, &mut bodies);
        }
        assert!(!has_keyboard_focus(&s.state.popups));
        let point = notice_point(&s);
        {
            let engine = s.session.borrow_mut();
            input.press(&mut s.state, &engine.read(), point);
        }
        input.queue_mut().push(release_at(point));
        drain(&mut s, &mut input, &mut bodies);
        pass(&mut s, &mut input, &mut bodies);
    }
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_L)),
            "안내는 포커스 칸을 쓰지 않는다"
        );
    }
    assert!(s.state.popups.bound_input_key_owner().is_none());
    assert!(
        has_keyboard_focus(&s.state.popups),
        "안내 안쪽이 포커스를 가졌다"
    );
    assert!(s.state.window_keys_to_overlay());
    (s, l, r, input)
}

/// Given 안내 안쪽 컨트롤이 키보드 포커스를 가졌을 때, When 단축키가 아닌 글자 b 를 치면, Then 탐색기 타입어헤드는
/// b 를 읽지 않아 banana 를 고르지 않는다.
#[test]
fn a_letter_typed_into_a_focused_notice_skips_the_explorer_type_ahead() {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    assert!(
        type_text(&mut s, &mut input, true, "b").is_none(),
        "안내가 읽지 않는 글자는 키 경로를 올라가 egui 큐에 실리지 않는다"
    );
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, _) = &bodies else {
        unreachable!()
    };
    assert!(!l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// Given 안내 안쪽 포커스, When 한 묶음에 [안내 밖 L 탐색기 빈 곳 누름, 글자 b]가 오면, Then 누름 몫 패스가 안내
/// 포커스를 풀어도 b 는 안내 포커스 때 분류했으므로 타입어헤드가 읽지 않는다.
#[test]
fn a_letter_after_a_press_that_unfocuses_the_notice_still_skips_the_type_ahead() {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    let rect = surface_rect_l();
    let empty = egui::pos2(rect.left() + 24.0, rect.bottom() - 24.0);
    let notice = s
        .state
        .popups
        .instances_of(RESTORE_NOTICE_POPUP_ID)
        .map(|p| p.popup_rect())
        .next()
        .expect("안내");
    assert!(!notice.contains(empty), "누름 자리는 안내 밖이다");
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), empty);
    }
    input.queue_mut().push(release_at(empty));
    assert!(
        type_text(&mut s, &mut input, true, "b").is_none(),
        "안내가 읽지 않는 글자는 키 경로를 올라가 egui 큐에 실리지 않는다"
    );
    pass(&mut s, &mut input, &mut bodies);
    assert!(
        !has_keyboard_focus(&s.state.popups),
        "바깥 누름이 안내 포커스를 풀었다"
    );
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, _) = &bodies else {
        unreachable!()
    };
    assert!(!l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// Given 안내 안쪽 포커스, When 한 묶음에 [글자 b, 그리기 없이 안내 닫기, 글자 c]가 오면, Then b 는 막히고 c 는 안내
/// 포커스가 없을 때 분류했으므로 타입어헤드가 읽는다(앞 몫의 첫 키 값으로 뒤 글자까지 막지 않는다).
#[test]
fn a_letter_after_the_notice_closes_reaches_the_type_ahead() {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let cherry = crate::explorer_ui::view::DirEntryInfo {
        path: PathBuf::from("/tasty-test-no-such-dir-1/cherry.txt"),
        name: "cherry.txt".into(),
        is_dir: false,
        size: 0,
        modified: None,
        ext: "txt".into(),
        ..Default::default()
    };
    l.view.entries = l
        .view
        .entries
        .iter()
        .cloned()
        .chain(std::iter::once(cherry))
        .collect();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    assert!(
        type_text(&mut s, &mut input, true, "b").is_none(),
        "안내가 읽지 않는 글자는 키 경로를 올라가 egui 큐에 실리지 않는다"
    );
    s.state.popups.close(RESTORE_NOTICE_POPUP_ID);
    assert!(!has_keyboard_focus(&s.state.popups));
    assert!(type_text(&mut s, &mut input, true, "c").is_some());
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, _) = &bodies else {
        unreachable!()
    };
    assert!(l.selected().contains("cherry"), "L: {:?}", l.selected());
}

/// 대조: 안내가 떠 있어도 안쪽 포커스가 없으면 같은 b 를 타입어헤드가 읽어 banana 를 고른다.
#[test]
fn a_letter_with_a_notice_shown_but_unfocused_reaches_the_type_ahead() {
    let mut s = open_scene(|_| {});
    close_search(&mut s);
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    let mut input = WindowInput::default();
    focus_left(&mut s, &mut input, &mut bodies);
    assert!(push(&mut s.state, entry()));
    for _ in 0..3 {
        pass(&mut s, &mut input, &mut bodies);
    }
    assert!(!has_keyboard_focus(&s.state.popups));
    assert!(type_text(&mut s, &mut input, true, "b").is_some());
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, _) = &bodies else {
        unreachable!()
    };
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// Given 안내 안쪽 포커스, When [글자 b, 안내 닫기, 글자 c]가 오면, Then 두 글자는 안내 값이 달라 다른 몫에 들고 각
/// 몫은 자기 키들의 사건 시점 값을 가진다(앞 몫 참, 뒤 몫 거짓). 글자가 없는 몫은 값이 없다.
#[test]
fn each_segment_keeps_the_notice_focus_of_its_own_keys() {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    assert_eq!(s.state.pass_notice_focus(), None, "글자가 없는 몫");
    assert!(
        type_text(&mut s, &mut input, true, "b").is_none(),
        "안내가 읽지 않는 글자는 키 경로를 올라가 egui 큐에 실리지 않는다"
    );
    s.state.popups.close(RESTORE_NOTICE_POPUP_ID);
    assert!(type_text(&mut s, &mut input, true, "c").is_some());
    assert_eq!(s.state.pass_notice_focus(), Some(true), "b 의 몫");
    pass(&mut s, &mut input, &mut bodies);
    assert_eq!(s.state.pass_notice_focus(), Some(false), "c 의 몫");
    pass(&mut s, &mut input, &mut bodies);
    assert!(input.is_empty(), "끊은 자리는 하나다");
    assert_eq!(s.state.pass_notice_focus(), None);
}

/// 안내 안쪽 포커스 장면에서 한 묶음에 [L 빈 곳 클릭(몫 A), 글자 a] 를 넣고(`tail` 이면 그 뒤에 [L 빈 곳 누름, 글자 b]
/// 를 더 넣는다) A 를 그리는 패스가 무대를 연다(그리기 중 전환의 봉인). 같은 노드의 누름은 그 몫에 키가 실렸을 때 끊고,
/// 누름 뒤 글자는 끊는다.
fn seal_after_a_notice_letter(tail: bool) -> Scene {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let rect = surface_rect_l();
    let empty = egui::pos2(rect.left() + 24.0, rect.bottom() - 24.0);
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), empty);
    }
    input.queue_mut().push(release_at(empty));
    assert!(
        type_text(&mut s, &mut input, true, "a").is_none(),
        "안내 포커스 때 친 글자는 egui 큐에 실리지 않는다"
    );
    if tail {
        {
            let engine = s.session.borrow_mut();
            input.press(&mut s.state, &engine.read(), empty);
        }
        assert!(
            type_text(&mut s, &mut input, true, "b").is_none(),
            "안내 포커스 때 친 글자는 egui 큐에 실리지 않는다"
        );
    }
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.open_fullscreen_stage("blank"));
    });
    assert!(!stage);
    s
}

/// Given 안내 안쪽 포커스, When [L 클릭, 글자 a] 의 클릭 패스가 무대를 열어 a 의 몫(안내 값 참)이 지금 몫으로 승계된 채
/// 봉인되면, Then 닫는 몫의 안내 값은 그대로 참이고 봉인이 연 새 몫의 값은 없다.
#[test]
fn a_seal_of_the_inherited_segment_keeps_its_notice_focus() {
    let s = seal_after_a_notice_letter(false);
    assert_eq!(s.state.pass_notice_focus(), Some(true));
    let slots = s.state.segment_slots();
    assert_eq!(slots.len(), 2, "{slots:?}");
    assert!(slots[0].marked && !slots[1].marked);
    assert_eq!(slots[1].notice, None, "봉인한 새 몫");
}

/// Given 안내 안쪽 포커스, When [L 클릭, 글자 a, L 누름, 글자 b] 의 클릭 패스가 무대를 열어 뒤 큐 꼬리(b 의 몫)가
/// 봉인되면, Then 그 몫의 안내 값은 그대로 참이고 봉인이 연 새 몫의 값은 없다. 안내가 읽지 않는 글자 a 는 egui 큐에
/// 실리지 않아(K9) 같은 노드의 두 번째 누름은 a 의 몫을 끊지 않고 그 몫에 든다.
#[test]
fn a_seal_of_the_queue_tail_keeps_its_notice_focus() {
    let s = seal_after_a_notice_letter(true);
    let slots = s.state.segment_slots();
    assert_eq!(slots.len(), 3, "{slots:?}");
    assert_eq!(slots[0].notice, Some(true), "a 와 두 번째 누름의 몫");
    assert_eq!(slots[1].notice, Some(true), "b 의 몫");
    assert!(slots[1].marked && !slots[2].marked);
    assert_eq!(slots[2].notice, None, "봉인한 새 몫");
}

/// 경로만 다른 안내 항목(같은 제목·원인이어도 경로가 다르면 따로 쌓인다).
fn entry_at(id: u64, path: &str) -> RestoreNoticeEntry {
    RestoreNoticeEntry {
        id,
        path: path.into(),
        ..entry()
    }
}

/// 항목 id 가 `id` 인 열린 안내의 인스턴스와 자리.
fn notice_of(
    s: &Scene,
    id: u64,
) -> Option<(crate::adapters::ui::popup::PopupInstanceId, egui::Rect)> {
    s.state
        .popups
        .instances_of(RESTORE_NOTICE_POPUP_ID)
        .find_map(|p| match &p.payload {
            crate::adapters::ui::popup::PopupPayload::RestoreNotice(n) if n.entry.id == id => {
                Some((p.instance, p.popup_rect()))
            }
            _ => None,
        })
}

/// 창이 받은 bare Esc 누름 하나.
fn notice_escape(s: &mut Scene, input: &mut WindowInput) {
    let engine = s.session.borrow_mut();
    let handled = input.escape(&s.ctx, &mut s.state, &engine.read(), true);
    assert!(
        !handled,
        "안내가 받는 Esc 는 창이 처리하지 않고 egui 에 둔다"
    );
}

/// M-E0b-4 F: Given 안내 A·B 가 떠 있고 B 안쪽이 포커스일 때, When bare Esc 가 온 뒤 그리기 전에 안쪽 포커스가 A 로
/// 옮겨지면, Then 그 Esc 는 사건 시점 출발점 B 의 것이라 B 가 닫히고 A 는 열린 채다(그리기 때 포커스로 다시 고르지
/// 않는다).
#[test]
fn escape_on_notice_b_reaches_b_after_focus_moves_to_a_before_drawing() {
    let mut s = open_scene(|_| {});
    close_search(&mut s);
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    focus_left(&mut s, &mut input, &mut bodies);
    assert!(push(&mut s.state, entry_at(1, "/restored-a")));
    assert!(push(&mut s.state, entry_at(2, "/restored-b")));
    for _ in 0..3 {
        pass(&mut s, &mut input, &mut bodies);
    }
    let (b, rect) = notice_of(&s, 2).expect("B 가 떠 있다");
    let point = egui::pos2(rect.center().x, rect.bottom() - 4.0);
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    input.queue_mut().push(release_at(point));
    drain(&mut s, &mut input, &mut bodies);
    pass(&mut s, &mut input, &mut bodies);
    assert_eq!(
        crate::adapters::ui::restore_notice::focused_notice(&s.state.popups),
        Some(b),
        "B 안쪽이 포커스다"
    );
    notice_escape(&mut s, &mut input);
    assert_eq!(s.state.pass_notice(), Some(b), "Esc 몫의 출발점은 B 다");
    crate::adapters::ui::restore_notice::move_inside_focus(&s.ctx, &mut s.state.popups, 1);
    drain(&mut s, &mut input, &mut bodies);
    assert!(notice_of(&s, 2).is_none(), "B 가 닫힌다");
    assert!(notice_of(&s, 1).is_some(), "A 는 열린 채다");
}

/// M-E0b-4 P: Given 안내 하나의 안쪽이 포커스일 때, When bare Esc 가 오면, Then 그 안내가 닫힌다.
#[test]
fn escape_on_a_single_notice_reaches_it() {
    let (mut s, mut l, mut r, mut input) = notice_focused_scene();
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    notice_escape(&mut s, &mut input);
    drain(&mut s, &mut input, &mut bodies);
    assert!(notice_of(&s, 1).is_none(), "안내가 닫힌다");
}
