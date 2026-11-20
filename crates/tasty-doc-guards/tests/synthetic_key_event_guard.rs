//! 포커스 전환 때 winit이 만드는 합성 키가 단축키·PTY·egui로 전달되지 않도록 처리 위치를 검사한다.
//! window_event 진입부에서 setup·종료·부팅·모달·플러그인 처리보다 먼저 차단해야 한다.
//! 일부 View는 KeyboardInput을 별도로 처리하지 않고 egui에 전달하므로 공통 진입부에서 막는다.
//!
//! 합성 release를 버린 뒤 이전 press가 남으면 한 번의 실제 입력이 double tap으로 판단될 수 있다.
//! 따라서 MainView와 SettingsView 각각의 detector를 포커스 변경 때 초기화해야 한다.
//! 이 가드는 실행 동작 대신 소스의 문자열 위치를 비교한다.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let p: PathBuf = tasty_doc_guards::repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// `needle` 이 정확히 한 번 나오는 바이트 오프셋.
fn only_at(hay: &str, needle: &str, what: &str) -> usize {
    let n = hay.matches(needle).count();
    assert_eq!(
        n, 1,
        "{what}: `{needle}` 이 {n} 번 나온다 — 이 가드는 유일 출현을 전제한다. \
         구조가 바뀌었으면 가드도 함께 갱신하라."
    );
    hay.find(needle).expect("checked above")
}

#[test]
fn synthetic_gate_precedes_every_mode_branch() {
    let src = read("src/app/event_handler.rs");
    let gate = only_at(
        &src,
        "if is_synthetic_key_event(&event) {",
        "합성 키 진입부 게이트",
    );

    for (needle, what) in [
        ("if self.state.shell_setup_mode {", "shell setup 모드 분기"),
        (
            "self.handle_shutdown_window_event(event_loop, id, event);",
            "종료 상태 머신 분기",
        ),
        (
            "self.handle_boot_window_event(event_loop, event);",
            "부팅 상태 머신 분기",
        ),
        (
            "self.handle_active_modal_window_event(id, event);",
            "활성 모달 분기",
        ),
        (
            "self.route_window_input(id, event);",
            "plugin 편집 보류·plugin 단축키·View 전달",
        ),
    ] {
        let at = only_at(&src, needle, what);
        assert!(
            gate < at,
            "합성 키 차단이 {what}보다 뒤에 있다. 해당 경로가 먼저 키를 처리할 수 있다."
        );
    }

    // window_event 에서 plugin 단축키 가로채기와 View 전달은 게이트 뒤 route_window_input 한 곳에서만 일어난다.
    // 이 파일에 둘을 직접 부르는 곳이 있으면 게이트 앞에 놓여도 위 비교가 잡지 못하므로 0 개를 요구한다.
    // dispatch_window_event_to_view 는 이 파일에 정의돼 있어 정의 줄만 뺀다.
    for (call, definition, what) in [
        (
            "dispatch_window_event_to_view(",
            Some("fn dispatch_window_event_to_view("),
            "View 직접 전달",
        ),
        ("try_plugin_shortcut(", None, "plugin 단축키 직접 처리"),
    ] {
        let calls = src.matches(call).count() - definition.map_or(0, |d| src.matches(d).count());
        assert_eq!(
            calls, 0,
            "event_handler.rs 에 {what} `{call}` 호출이 {calls} 개 있다. window_event 는 게이트 뒤 \
             route_window_input 으로만 보낸다."
        );
    }

    // window_event 의 마지막 단계는 route_window_input 이 맡는다. plugin 단축키 가로채기와 View 전달이 그
    // 안에 이 순서로 있어야 위 비교가 두 단계를 덮는다.
    let route_src = read("src/app/edit_key_replay.rs");
    let route = only_at(
        &route_src,
        "pub(super) fn route_window_input(",
        "창 입력 마지막 단계",
    );
    let body = &route_src[route..];
    let body = &body[..body.find("\n    }\n").expect("route_window_input body end")];
    let shortcut = only_at(
        body,
        "self.try_plugin_shortcut(id, ke)",
        "plugin 단축키 가로채기",
    );
    let view = only_at(
        body,
        "self.dispatch_key_to_view(id, event);",
        "View 이벤트 전달",
    );
    assert!(
        shortcut < view,
        "route_window_input 에서 plugin 단축키 가로채기가 View 전달보다 뒤에 있다."
    );
    // View 전달은 `dispatch_key_to_view` 가 맡는다(키·IME 면 그 뒤 포커스 팝업 즉시 적용).
    let to_view = only_at(&route_src, "fn dispatch_key_to_view(", "View 전달 함수");
    let to_view_body = &route_src[to_view..];
    let to_view_body = &to_view_body[..to_view_body
        .find("\n    }\n")
        .expect("dispatch_key_to_view body end")];
    only_at(
        to_view_body,
        "self.dispatch_window_event_to_view(id, event);",
        "View 전달 함수의 View 이벤트 전달",
    );
}

#[test]
fn double_tap_detectors_reset_on_focus_change() {
    for (rel, what, arm) in [
        (
            "src/view/main.rs",
            "MainView",
            "WindowEvent::Focused(focused) => {",
        ),
        (
            // 이 arm은 포커스를 잃는 경우뿐 아니라 되찾는 경우도 본다. 포커스가
            // 돌아올 때 권한 상태 스냅샷을 다시 측정하기 때문이다. 그래서 값을 묶는
            // `Focused(focused)` 형태이며, MainView와 모양이 같은 것도 둘 다 포커스
            // 방향을 읽어야 해서다.
            "src/view/settings.rs",
            "SettingsView",
            "WindowEvent::Focused(focused) => {",
        ),
    ] {
        let src = read(rel);
        let reset = only_at(&src, "self.double_tap.reset();", &format!("{what} reset"));
        let focused = only_at(&src, arm, &format!("{what} Focused arm"));
        assert!(
            focused < reset && reset - focused < 600,
            "{what}: reset 이 Focused arm 안에 있지 않다 — 포커스 전환에 걸리지 않는다."
        );
    }
}

/// 순서 검사만으로는 판정 함수가 비활성화된 것을 알 수 없어 합성 플래그를 읽는 문자열도 확인한다.
/// winit의 비공개 platform_specific 필드 때문에 여기서는 KeyEvent를 직접 구성하지 않는다.
#[test]
fn predicate_reads_the_synthetic_flag() {
    let src = read("src/adapters/ui/input/synthetic.rs");
    let body = src
        .split_once("pub fn is_synthetic_key_event")
        .expect("`is_synthetic_key_event` 정의가 사라졌다")
        .1;
    // 테스트 모듈의 같은 이름이 판정 함수 본문을 대신하지 않게 제외한다.
    let body = body
        .split("#[cfg(test)]")
        .next()
        .expect("split always yields one");

    for needle in ["WindowEvent::KeyboardInput", "is_synthetic: true"] {
        assert!(
            body.contains(needle),
            "is_synthetic_key_event 본문에서 {needle}을 찾지 못했다. 실제 합성 플래그를 검사하는지 확인한다."
        );
    }
}
