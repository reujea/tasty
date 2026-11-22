//! 편집 키(복사·잘라내기·붙여넣기) 판정이 egui-winit보다 먼저 한 번 실행되는지 소스에서 확인한다.
//! 판정 결과는 `gfx::gpu::edit_keys` 시험이 상태 기계로 확인하지만, 그 시험은 창의 이벤트 경로를 거치지
//! 않아 판정을 egui-winit 뒤로 옮겨도 통과한다. 이 검사는 호출 위치의 순서를 본다. 조건 분기의 실제 실행
//! 여부와 이 목록 밖의 창은 판단하지 않는다. 계약: `docs/design/policies/key-mapping.md`의 "편집 키 판정".

use super::{fn_body, repo_root, strip_comments};

/// egui-winit에 이벤트를 넘기는 호출. 판정은 이보다 앞에 있어야 한다.
const EGUI_WINIT: &str = "handle_egui_event(";

fn body(rel: &str, signature: &str) -> String {
    let path = repo_root().join(rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, signature).unwrap_or_else(|| {
        panic!("{rel}에서 `{signature}` 본문을 읽지 못했다. 이름·이동을 확인한다.")
    });
    strip_comments(&body)
}

/// `body`에서 `first`의 첫 위치가 `then`의 첫 위치보다 앞인지. 둘 다 있어야 한다.
fn assert_before(body: &str, first: &str, then: &str, what: &str) {
    let at = |needle: &str| {
        body.find(needle).unwrap_or_else(|| {
            panic!("{what}: `{needle}` 호출이 없다. 구조가 바뀌었으면 이 검사도 갱신한다.")
        })
    };
    assert!(
        at(first) < at(then),
        "{what}: `{first}`가 `{then}`보다 뒤에 있다. 편집 키 판정은 egui-winit보다 먼저 해야 한다."
    );
}

/// egui 전용 창의 두 입구는 판정을 먼저 하고 가져가지 않은 이벤트만 egui-winit에 넘긴다.
#[test]
fn egui_only_windows_judge_before_egui_winit() {
    const GPU: &str = "src/gfx/gpu/edit_keys.rs";
    for signature in [
        "fn handle_egui_event_with_edit_keys(",
        "fn handle_egui_event_for_window(",
    ] {
        let body = body(GPU, signature);
        assert_before(&body, "judge_egui_key(", EGUI_WINIT, signature);
    }
}

/// 메인 창은 이벤트 경로의 맨 앞에서 판정하고, 가져간 이벤트는 그 자리에서 돌려보낸다.
#[test]
fn the_main_window_judges_before_egui_winit() {
    let body = body("src/view/main.rs", "fn handle_engine_event(");
    let gate = "let Some(edit_judged) = self.edit_key_gate(engine, &event) else {\n            return ViewAction::None;";
    assert!(
        body.contains(gate),
        "메인 창이 판정이 가져간 이벤트를 바로 돌려보내지 않는다."
    );
    for then in [
        EGUI_WINIT,
        "consume_explorer_ime_key(",
        "route_tutorial_input(",
    ] {
        assert_before(&body, "self.edit_key_gate(", then, "메인 창 이벤트 경로");
    }
}

/// 자체 키 처리를 가진 보조 창은 공통 흐름(`crate::view::judged_window_event`)으로만 egui에 넘긴다. 판정이
/// 가져간 이벤트가 창 자체 처리에 가지 않는 분기는 그 흐름의 시험(`view::judged_event::tests`)이 동작으로
/// 확인한다. 여기서는 창이 그 흐름을 우회하지 않는지와, 흐름이 판정 결과를 바꾸지 않고 그대로 쓰는지를 본다.
#[test]
fn auxiliary_windows_go_through_the_judged_event_flow() {
    for rel in [
        "src/view/settings.rs",
        "src/view/quit.rs",
        "src/view/preset.rs",
        "src/view/plugins.rs",
    ] {
        let src = std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"));
        assert!(
            !strip_comments(&src).contains("handle_egui_event"),
            "{rel}: 공통 흐름을 거치지 않고 egui에 넘긴다."
        );
        let body = body(rel, "fn handle_judged_event(");
        assert!(
            body.contains("crate::view::judged_window_event("),
            "{rel}: 창 이벤트가 공통 흐름을 거치지 않는다."
        );
    }
    // 줄바꿈·들여쓰기는 rustfmt가 정하므로 공백을 빼고 대조한다.
    let flow: String = body("src/view/judged_event.rs", "fn judged_window_event<")
        .split_whitespace()
        .collect();
    let call = "base.gpu.handle_egui_event_for_window(&base.winit,event,judged)";
    let after = flow
        .split(call)
        .nth(1)
        .unwrap_or_else(|| panic!("공통 흐름에 `{call}` 가 없다."));
    assert!(
        after.starts_with('}'),
        "공통 흐름이 판정 결과를 바꿔 넘긴다: `{call}` 뒤에 `{}`",
        after.chars().take(40).collect::<String>()
    );
}

/// 공통 판정이 넘긴 키라는 표시가 HTML 주소 띠의 바인딩 판정 두 곳에 간다. 빠지면 넘긴 복사 키가 같은 키의
/// 새로고침을 다시 편집으로 골라 아무것도 하지 않는다.
#[test]
fn the_address_band_hears_that_the_edit_key_passed() {
    let read = |rel: &str| {
        std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n")
    };
    for (rel, call) in [
        (
            "src/view/main/keyboard.rs",
            "self.try_consume_address_refresh_key(engine, event, edit_judged.is_some())",
        ),
        (
            "src/view/main.rs",
            "self.feed_egui_keyboard(engine, &event, edit_judged.is_some())",
        ),
        (
            "src/view/main.rs",
            "self.remap_html_address_keys(engine, event, from, edit_judged)",
        ),
    ] {
        assert!(
            strip_comments(&read(rel)).contains(call),
            "{rel}: `{call}` 가 없다. 넘긴 편집 키 표시가 주소 띠 판정에 가지 않는다."
        );
    }
}

/// 별도 창 Esc 표지(팝업 층 Esc 판정): 판정을 지난 사건을 egui-winit 에 넘기기 전에 입력 큐 길이를 재고, 넘긴 뒤 새로
/// 붙은 범위의 Esc 누름에만 표지를 쌓는다. 순서만 본다. 조건 분기와 범위 값은 `body_keys` 순수 함수 시험이 잰다.
#[test]
fn window_escape_marks_follow_the_egui_load() {
    let signature = "fn handle_egui_event_for_window(";
    let body = body("src/gfx/gpu/edit_keys.rs", signature);
    let order = [
        "judge_egui_key(",
        "egui_input().events.len()",
        EGUI_WINIT,
        "escape_marks_in(",
    ];
    for pair in order.windows(2) {
        assert_before(&body, pair[0], pair[1], signature);
    }
}

/// 별도 창 키 값(W1 주인과 Esc 표지)은 관리자를 그리는 함수가 팝업을 그리기 전에 패스마다 넣는다. 창 `render` 는
/// `run_egui` 전에 표지를 꺼내기만 하고 넣지 않는다. 소스 자리와 순서만 본다. 그리기 함수 안에서 조건을 걸어 첫 패스에만
/// 넣는 잘못은 잡지 못한다.
#[test]
fn window_pass_keys_are_put_in_every_pass() {
    const PUT: &str = "put_window_pass_keys(";
    for (rel, signature) in [
        ("src/view/settings/ui.rs", "fn draw_settings_panel("),
        ("src/view/file_chooser/window.rs", "fn draw("),
    ] {
        let body = body(rel, signature);
        assert_before(&body, PUT, ".draw_view_local(", signature);
    }
    for rel in [
        "src/view/settings.rs",
        "src/view/preset.rs",
        "src/view/plugins.rs",
    ] {
        let body = body(rel, "fn render(");
        for put in [PUT, "decide_frame_keys_for("] {
            assert!(
                !body.contains(put),
                "{rel}: `render` 가 `{put}` 를 부른다. 넣기는 관리자를 그리는 함수가 패스마다 한다."
            );
        }
        assert_before(&body, "take_window_escape_marks(", "run_egui(", rel);
    }
}
