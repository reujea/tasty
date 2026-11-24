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

/// 메인 창은 이벤트 경로의 맨 앞에서 판정하고, 가져간 이벤트는 그 자리에서 돌려보낸다. 단계 순서는 E0 함수
/// (`run_key_event`)가 정하고, 단계마다 메인 창이 부르는 함수는 `MainKeySteps`에 있다.
#[test]
fn the_main_window_judges_before_egui_winit() {
    let handle = body("src/view/main.rs", "fn handle_engine_event(");
    assert!(
        handle.contains("crate::view::key_event::run_key_event(&mut steps, event)"),
        "메인 창이 E0 함수로 사건을 넘기지 않는다."
    );
    let order = body("src/view/key_event.rs", "fn run_key_event<");
    let gate =
        "let Some(judged) = steps.judge_edit_key(&event) else {\n        return ViewAction::None;";
    assert!(
        order.contains(gate),
        "E0 함수가 판정이 가져간 이벤트를 바로 돌려보내지 않는다."
    );
    for then in ["steps.intercepts_after_edit_judgment(", "steps.feed_egui("] {
        assert_before(&order, "steps.judge_edit_key(", then, "E0 함수");
    }
    const STEPS: &str = "src/view/main/key_event_steps.rs";
    let judge = body(STEPS, "fn judge_edit_key(");
    assert!(
        judge.contains("self.view.edit_key_gate(self.engine, event)"),
        "메인 창의 단계 6이 편집 판정을 부르지 않는다."
    );
    let intercept = body(STEPS, "fn intercepts_after_edit_judgment(");
    for call in ["consume_explorer_ime_key(", "route_tutorial_input("] {
        assert!(
            intercept.contains(call),
            "메인 창의 단계 7에 `{call}` 가 없다."
        );
    }
    assert!(
        body(STEPS, "fn feed_egui(").contains(EGUI_WINIT),
        "메인 창의 egui-winit 호출이 단계 8 밖에 있다."
    );
    for (rel, fun) in [
        ("src/view/main.rs", "fn handle_engine_event("),
        (STEPS, "fn ends_before_edit_judgment("),
        (STEPS, "fn settle_key_node("),
        (STEPS, "fn judge_edit_key("),
        (STEPS, "fn intercepts_after_edit_judgment("),
    ] {
        assert!(
            !body(rel, fun).contains(EGUI_WINIT),
            "{rel} `{fun}`: 편집 판정 앞 단계가 egui-winit에 넘긴다."
        );
    }
}

/// 계약 E0: 메인 창과 자체 키 처리를 가진 별도 창(`judged_window_event`)이 같은 E0 함수로 사건을 넘긴다. 단계
/// 순서는 그 함수 한 곳에만 있다.
#[test]
fn every_window_runs_a_key_event_through_one_order_function() {
    for (rel, signature, call) in [
        (
            "src/view/main.rs",
            "fn handle_engine_event(",
            "crate::view::key_event::run_key_event(&mut steps, event)",
        ),
        (
            "src/view/judged_event.rs",
            "fn judged_window_event<",
            "run_key_event(&mut steps, event)",
        ),
    ] {
        assert!(
            body(rel, signature).contains(call),
            "{rel} `{signature}`: E0 함수 `{call}` 를 부르지 않는다."
        );
    }
    let flow = body("src/view/judged_event.rs", "fn judged_window_event<");
    assert!(
        flow.contains("window_steps("),
        "별도 창 입구가 별도 창 단계(`window_steps`)로 E0 함수를 부르지 않는다."
    );
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
            "src/view/main/key_event_steps.rs",
            ".feed_egui_keyboard(self.engine, event, judged.is_some())",
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

/// 별도 창 bare Esc(단계 8 뒤 단계 11): egui 에 실은 사건의 몫 결속(`bind_window_input`)이 bare Esc 면 그 몫을 끝내고
/// (뒤 경계) 사건 시점 수신자로 적용한다. 공용 순서 함수가 egui 적재 뒤에 그 결속을 부르는지는
/// `every_window_runs_a_key_event_through_one_order_function` 이 본다. 순서만 본다. 적용 값은 `window_input` 시험이 잰다.
#[test]
fn a_window_escape_is_applied_after_its_segment_ends() {
    let signature = "fn bind_window_input(";
    let bind: String = body("src/view/window_input.rs", signature)
        .split_whitespace()
        .collect();
    assert_before(
        &bind,
        "Incoming::Escape=>{popups.note_escape_loaded();",
        "popups.apply_window_escape(ctx,pending,escape_held);",
        signature,
    );
    let feed = body(
        "src/gfx/gpu/edit_keys.rs",
        "fn handle_egui_event_for_window(",
    );
    assert!(
        !feed.contains("escape_marks"),
        "별도 창 egui 적재에 Esc 표지가 남았다. bare Esc 는 사건 입구의 적용이 넘긴다."
    );
}

/// 별도 창 패스 키 주인(그 몫의 결속, 없으면 `run` 전 노드)은 관리자를 그리는 함수가 팝업을 그리기 전에 패스마다 정한다.
/// 창 `render` 는 공용 패스(`run_window_pass`)로만 패스를 열고 입력을 직접 꺼내지 않는다. 공용 패스 주체가 `run`
/// 전에 결속 없는 몫의 키 주인을 관리자에 둔다. 소스 자리와 순서만 본다. 그리기 함수 안에서 조건을 걸어 첫 패스에만 넣는 잘못은 잡지 못한다.
#[test]
fn window_pass_keys_are_put_in_every_pass() {
    const PUT: &str = "window_widget_takes_keys(";
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
        // preset 창은 공용 패스가 넘긴 그리기 클로저 안에서 정한다(창 위젯을 선택 창보다 먼저 그린다).
        if let Some(at) = body.find(PUT) {
            assert!(
                body.find("run_window_pass(").is_some_and(|run| run < at),
                "{rel}: `render` 가 공용 패스 밖에서 `{PUT}` 를 부른다."
            );
        }
        for put in ["take_egui_input(", "run_egui("] {
            assert!(
                !body.contains(put),
                "{rel}: `render` 가 `{put}` 를 부른다. 패스는 공용 패스가 열고 넣기는 관리자를 그리는 함수가 한다."
            );
        }
        assert!(
            body.contains("run_window_pass("),
            "{rel}: `render` 가 공용 패스로 패스를 열지 않는다."
        );
    }
    let host = "src/gfx/gpu/window_pass.rs";
    let run = body(host, "fn run(");
    assert_before(&run, "begin_run(", ".run(raw_input", host);
}

/// M-N1-2 F: 설정 열기 요청 래치 노드가 받은 키는 단계 6 편집 판정에 가지 않는다. 판정 입구(`judge_edit_key`)가 단계 4
/// 수신자를 plugin·창 판정보다 먼저 읽는다. 단계 6 은 GPU 가 있는 `MainView` 라 시험 대역이 없어 순서를 본다.
#[test]
fn an_edit_key_during_the_settings_latch_is_not_judged() {
    let judge = body("src/view/main/edit_keys.rs", "fn judge_edit_key(");
    let latch = "settings_latch_receives(self.key_decision)";
    assert!(
        judge.contains("return EguiKey::Free;"),
        "래치 노드가 받은 키를 판정하지 않고 넘기는 갈래가 없다."
    );
    for then in [
        "judge_plugin_popup_edit_key(",
        "judge_plugin_surface_edit_key(",
        "judge_egui_key(",
        "judge_unsettled_egui_key(",
    ] {
        assert_before(&judge, latch, then, "fn judge_edit_key(");
    }
}
