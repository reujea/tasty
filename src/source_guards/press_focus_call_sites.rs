//! 누름이 포커스 노드를 정하는 자리(J6)가 창의 누름 경로와 debug 포인터 주입에서 같은 함수를 거치는지 확인한다.
//! 포커스 노드를 옮기는 판정(`MainViewState::take_press_snapshot`)과 입력 끊기(`MainViewState::cut_key_frame`)의 동작은
//! 장면 시험(`adapters::ui::popup::route::tests::repro`)이 그 함수를 직접 불러 잰다. 창 호출부가 그 함수를 부르지
//! 않으면 장면 시험은 보지 못하므로 호출부의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출부가 함수를 부르되 결과를 버리는 다른 길을 더하는 것, 창 밖 별도 창의 누름 경로.

use super::{fn_body, repo_root, strip_comments};

fn read(rel: &str) -> String {
    strip_comments(
        &std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n"),
    )
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn at(body: &str, needle: &str, what: &str) -> usize {
    assert_eq!(
        body.matches(needle).count(),
        1,
        "{what}: `{needle}` 이 하나가 아니다"
    );
    body.find(needle).unwrap_or_default()
}

/// 창의 누름: egui 로 보내기 전에 입력을 끊고, 누름 처리가 누름 스냅샷 함수로 포커스 노드를 옮겨 그 프레임에 남긴다.
#[test]
fn a_window_press_moves_the_focus_node_through_the_press_snapshot() {
    let main = squeeze(&read("src/view/main.rs"));
    let cut = squeeze(
        &fn_body(&read("src/view/main.rs"), "fn cut_key_frame_before_press(")
            .expect("cut_key_frame_before_press"),
    );
    at(
        &cut,
        "ifself.state.cut_key_frame(self.base.gpu.egui_input_events_mut()){self.base.gpu.cut_egui_input_here();}",
        "입력 끊기",
    );
    let feed = squeeze(
        &fn_body(&read("src/view/main.rs"), "fn feed_egui_pointer_or_other(")
            .expect("feed_egui_pointer_or_other"),
    );
    at(
        &feed,
        "self.cut_key_frame_before_press();}self.base.gpu.handle_egui_event(&self.base.winit,event)",
        "누름을 egui 에 넣기 전 끊기",
    );
    at(
        &main,
        "self.feed_egui_pointer_or_other(&event)",
        "키가 아닌 사건의 egui 전달",
    );
    let mouse = read("src/view/main/mouse.rs");
    let handle = squeeze(&fn_body(&mouse, "fn handle_mouse_input(").expect("handle_mouse_input"));
    at(
        &handle,
        "ifbutton_state==ElementState::Pressed&&letSome(snapshot)=self.press_snapshot_now(engine,button){self.state.keep_press_snapshot(snapshot);}",
        "창 누름의 스냅샷",
    );
    let now = squeeze(&fn_body(&mouse, "fn press_snapshot_now(").expect("press_snapshot_now"));
    at(
        &now,
        "Some(self.state.take_press_snapshot(engine,pos,button))",
        "누름 스냅샷 함수",
    );
}

/// debug 포인터 주입: 누름이면 `press_focus`(끊기 → 누름 스냅샷 → 남기기)를 거친 뒤 egui 입력에 넣는다. `press_focus` 의
/// 동작은 `view::main::debug_input` 의 시험이 그 함수를 직접 불러 잰다.
#[test]
fn a_debug_pointer_press_moves_the_focus_node_through_the_press_snapshot() {
    let src = read("src/view/main/debug_input.rs");
    let body = squeeze(
        &fn_body(&src, "fn debug_inject_egui_pointer(").expect("debug_inject_egui_pointer"),
    );
    let focus = at(
        &body,
        "press_focus(&mutself.state,engine,&mutself.base.gpu,pos,map_egui_button(*button),);",
        "누름의 포커스 처리",
    );
    let push = at(
        &body,
        "push_egui_pointer_at(&mutself.base.gpu,pos,action,modifiers);",
        "egui 입력에 넣기",
    );
    assert!(focus < push, "포커스 처리 → egui 입력 순서가 아니다");
    let press = squeeze(&fn_body(&src, "fn press_focus(").expect("press_focus"));
    let cut = at(
        &press,
        "ifstate.cut_key_frame(sink.queued_events()){sink.cut_queue_here();}",
        "입력 끊기",
    );
    let take = at(
        &press,
        ".take_press_snapshot(core,pos,button);",
        "누름 스냅샷 함수",
    );
    let keep = at(
        &press,
        "state.keep_press_snapshot(snapshot);",
        "스냅샷 남기기",
    );
    assert!(
        cut < take && take < keep,
        "끊기 → 스냅샷 → 남기기 순서가 아니다"
    );
}
