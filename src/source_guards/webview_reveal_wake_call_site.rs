//! 숨김 보류 로드의 완료를 다시 그리기로 잇는 회차 끝 호출을 확인한다. 깨움 판정(`take_settled`)과 기록
//! (`record_pending`)의 동작은 `app::webview_sync::reveal_wake` 의 시험이 잰다. 그 판정을 이벤트 루프가
//! 부르지 않거나 깨움이 소유 창에 다시 그리기를 요청하지 않으면 그 시험은 보지 못하므로 두 자리의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출을 두되 그 앞에서 돌아가는 새 분기, OS 콜백이 이 회차 뒤에 오는 이벤트 루프.

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

/// 이벤트 루프 회차 끝(`about_to_wait`)은 native webview 이벤트를 처리한 뒤, 미룬 다시 그리기를 실행하기 전에
/// 깨움을 부른다. 앞이면 이번 회차의 GTK 완료 신호를 놓치고, 뒤면 요청한 다시 그리기가 이번 회차에 예약되지 않는다.
#[test]
fn the_loop_end_wakes_settled_reveals_between_the_webview_pump_and_the_repaints() {
    let src = read("src/app/event_handler.rs");
    let body = squeeze(&fn_body(&src, "fn about_to_wait(").expect("about_to_wait"));
    let pump = at(
        &body,
        "self.pump_webview_key_events();",
        "webview 이벤트 처리",
    );
    // 조건 없는 문장으로 폴링 예약 바로 뒤에 둔다. 조건문 안으로 옮겨 사실상 끄는 것도 여기서 잡는다.
    at(
        &body,
        "std::time::Instant::now(),);self.redraw_settled_webview_reveals();self.poll_pending_native_menus();",
        "폴링 예약 뒤 숨김 보류 깨움",
    );
    let wake = at(
        &body,
        "self.redraw_settled_webview_reveals();",
        "숨김 보류 깨움",
    );
    let repaint = at(
        &body,
        "self.drive_deferred_repaints(event_loop);",
        "미룬 다시 그리기",
    );
    assert!(pump < wake && wake < repaint, "깨움 호출의 순서가 바뀌었다");
}

/// 깨움은 판정이 참인 창에 다시 그리기를 요청한다.
#[test]
fn a_settled_reveal_marks_its_own_window_dirty() {
    let src = read("src/app/webview_sync/reveal_wake.rs");
    let body = squeeze(&fn_body(&src, "fn redraw_settled_webview_reveals(").expect("wake"));
    let judge = at(
        &body,
        "ifwake_window(&mutmain.webview_reveal_pending,&main.webviews)",
        "깨움 판정",
    );
    let mark = at(&body, "{main.mark_dirty();}", "소유 창 다시 그리기");
    assert!(judge < mark, "다시 그리기 요청이 판정 결과에 묶이지 않았다");
}
