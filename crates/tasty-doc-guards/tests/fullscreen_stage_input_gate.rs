//! 전체화면 무대에서 뒤쪽 UI로 입력이 전달되지 않도록 처리 위치와 호출을 검사한다.
//! 키보드 판정의 값은 stage_key_decision 단위 테스트가 확인하고, 이 검사는 소스의 순서·문자열을 확인한다.
//! 네이티브 메뉴·파일 드래그·IME·플러그인 단축키는 일반 키보드 처리 밖에 있어 각각 확인해야 한다.

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
fn keyboard_stage_gate_precedes_double_tap_and_escape() {
    let src = read("src/view/main/keyboard.rs");
    let gate = only_at(
        &src,
        "if self.try_consume_fullscreen_stage_key(engine, event) {",
        "0단계 무대 게이트 호출",
    );
    let double_tap = only_at(
        &src,
        "if self.try_consume_double_tap_key(engine) {",
        "1~3단계 double-tap",
    );
    let escape = only_at(&src, "if self.try_consume_escape_key(event) {", "4단계 ESC");
    assert!(
        gate < double_tap,
        "무대 입력 처리가 double-tap 뒤에 있어 일반 단축키가 실행될 수 있다."
    );
    assert!(
        gate < escape,
        "무대 입력 처리가 일반 Escape 처리 뒤에 있다. 무대의 Escape가 뒤쪽 UI에도 전달될 수 있다."
    );
}

/// 무대가 입력을 처리했으면 뒤의 일반 입력 단계로 진행하면 안 된다.
#[test]
fn keyboard_stage_gate_returns_immediately() {
    let src = read("src/view/main/keyboard.rs");
    let call =
        "if self.try_consume_fullscreen_stage_key(engine, event) {\n            return;\n        }";
    assert!(
        src.contains(call),
        "0단계 게이트가 소비 즉시 return 하지 않는다 — 무대 중 키가 뒤 단계로 샌다."
    );
}

/// 종료 바인딩을 무대 밖에서 조회·적용하면 기존 Escape 처리나 터미널 전달을 가로챌 수 있다.
#[test]
fn stage_exit_key_has_a_single_decision_site() {
    let src = read("src/view/main/keyboard.rs");
    only_at(&src, "fn stage_exit_key_matches(", "무대 종료 키 판정 함수");
    assert!(
        src.contains("    if stage_exit_key_matches(exit_bindings, key, mods) {"),
        "무대 종료 판정이 `stage_key_decision` 안에서 이 함수를 거치지 않는다."
    );
    only_at(
        &src,
        "keybindings.fullscreen_stage_exit",
        "무대 종료 바인딩 조회",
    );
    let lookup = only_at(
        &src,
        "&engine.settings.keybindings.fullscreen_stage_exit,",
        "게이트의 바인딩 조회",
    );
    let gate_fn = only_at(
        &src,
        "fn try_consume_fullscreen_stage_key(\n        &mut self,\n        engine: &crate::runtime::engine_read::EngineRead<'_>,\n        event: &winit::event::KeyEvent,\n    ) -> bool {",
        "0단계 게이트 함수",
    );
    let stage_active_guard = only_at(
        &src,
        "    if !stage_active {\n        return StageKeyDecision::PassThrough;\n    }",
        "무대 비활성 조기 반환",
    );
    assert!(
        gate_fn < lookup,
        "무대 종료 바인딩 조회가 무대 처리 함수 앞에 있다. 무대 밖에서 기존 Escape 처리를 가로채지 않는지 확인한다."
    );
    let call_site = only_at(
        &src,
        "    if stage_exit_key_matches(exit_bindings, key, mods) {",
        "판정 호출부",
    );
    assert!(
        stage_active_guard < call_site,
        "`stage_key_decision` 의 무대 비활성 조기 반환이 판정 호출 뒤로 밀렸다 — 무대가 \
         없을 때도 종료 바인딩이 매칭된다."
    );
}

/// 마우스 경로들이 동일한 오버레이 조건을 사용해야 입력 차단 범위가 일치한다.
#[test]
fn mouse_layers_share_one_stage_aware_gate() {
    let src = read("src/view/main/mouse.rs");
    only_at(
        &src,
        "fn mouse_overlay_open(&self) -> bool {\n        self.state.settings_open_requested || self.state.fullscreen_stage_active()",
        "통합 판정 정의",
    );
    let calls = src.matches("self.mouse_overlay_open()").count();
    assert_eq!(
        calls, 5,
        "`mouse_overlay_open()` 호출이 {calls} 회다 — handle_cursor_moved / \
         handle_mouse_input(click-to-activate 포함) / handle_mouse_wheel / \
         try_begin_os_resize / update_hovered_link 다섯 지점 전부가 같은 판정을 봐야 한다."
    );
    // click-to-activate는 일반 차단 검사보다 먼저 실행되므로 같은 조건을 인자로 받아야 한다.
    assert!(
        src.contains("self.try_click_to_activate(engine, button, button_state, overlay_open)"),
        "click-to-activate가 공통 오버레이 조건을 받지 않는다. 무대 뒤 서피스로 포커스가 이동할 수 있다."
    );
    assert!(
        !src.contains("let overlay_open = self.state.settings_open_requested;"),
        "`overlay_open = settings_open_requested` 정의가 되살아났다 — 그 경로는 무대를 모른다."
    );
}

#[test]
fn cursor_icon_bails_out_during_a_stage() {
    let src = read("src/state/mouse.rs");
    let bail = only_at(
        &src,
        "if self.fullscreen_stage_active() {",
        "커서 판정 무대 조기 반환",
    );
    let contains = only_at(
        &src,
        "if !terminal_rect.contains(PhysicalPx(x), PhysicalPx(y)) {",
        "terminal_rect 판정",
    );
    assert!(
        bail < contains,
        "무대 조기 반환이 좌표 검사 뒤에 있어 뒤쪽 UI의 커서가 표시될 수 있다."
    );
}

/// OS 메뉴·드래그는 렌더링한 오버레이만으로 막을 수 없어 별도로 억제한다.
#[test]
fn os_level_ui_is_suppressed_during_a_stage() {
    let src = read("src/view/main/redraw.rs");
    only_at(
        &src,
        "self.sync_fullscreen_stage_transition(engine);",
        "무대 진입 정리 훅 호출",
    );
    let menu_guard = "if self.state.fullscreen_stage_active() {\n            self.state.dialogs.pending_native_menu = None;\n            return;\n        }";
    assert!(
        src.contains(menu_guard),
        "무대 중 네이티브 컨텍스트 메뉴 억제가 사라졌다 — OS 팝업은 wgpu 표면 위에 떠서 \
         무대가 덮지 못한다."
    );
    let drag = read("src/view/main/file_drag.rs");
    let poll = drag
        .split("pub(crate) fn poll_outbound_drag(")
        .nth(1)
        .and_then(|rest| rest.split("\n    }\n").next())
        .expect("poll_outbound_drag 본문을 찾지 못했다");
    assert!(
        poll.contains("if self.state.window_keys_to_overlay()")
            && poll.contains("self.cancel_outbound_drag();\n            return;"),
        "무대 중 네이티브 파일 드래그 억제가 사라졌다."
    );
    // 닫힌 네이티브 메뉴의 결과를 회수하려면 폴링은 계속해야 한다.
    only_at(
        &src,
        "self.poll_pending_native_menu(engine);",
        "네이티브 메뉴 폴링",
    );
}

/// 메뉴 폴링을 렌더 뒤에서 수행하는 순서가 바뀌면 결과 회수 경로도 재검토해야 한다.
#[test]
fn native_menu_polling_stays_after_render() {
    let src = read("src/view/main/redraw.rs");
    let poll = only_at(
        &src,
        "self.poll_pending_native_menu(engine);",
        "네이티브 메뉴 폴링",
    );
    assert!(poll > only_at(&src, "fn finish_redraw(", "렌더 후처리"));
    let frame = read("src/app/view_frame.rs");
    let render = only_at(&frame, "view.render_if_dirty(", "렌더 호출");
    let finish = only_at(&frame, "view.finish_redraw(", "후처리 호출");
    assert!(
        render < finish,
        "네이티브 메뉴 폴링 위치가 바뀌었다 — 이 가드는 렌더 뒤 폴링을 전제한다."
    );
}

/// 무대 진입 시 이전 UI의 입력 상태를 정리한다. 항목별 이유는 docs/design/systems/fullscreen-stage.md의 진입 시 정리 절을 따른다.
#[test]
fn stage_entry_discards_every_in_flight_gesture() {
    let src = read("src/view/main/redraw.rs");
    let body = fn_body(&src, "fn sync_fullscreen_stage_transition(");
    for (line, why) in [
        (
            "self.clear_ime_preedit(engine);",
            "조합 중 IME 가 뒤 PTY 로 확정된다",
        ),
        (
            "self.finish_divider_drag(engine);",
            "마지막 유효 비율을 확정하며 분할선 제스처를 끝내지 않는다",
        ),
        (
            "self.left_mouse_down = false;",
            "좌클릭 선택 게이트가 눌린 채로 남는다",
        ),
        (
            "self.left_select_bypass = false;",
            "Shift+클릭 우회 플래그가 남아 다음 클릭 경로가 어긋난다",
        ),
        (
            "self.state.popups.cancel_pointer_interactions();",
            "팝업 이동·크기 조절 상태가 해제되지 않는다",
        ),
        (
            "self.hovered_link = None;",
            "뒤쪽 UI 좌표로 계산한 링크 hover가 남는다",
        ),
        (
            "self.state.pending_resize_cursor = None;",
            "뒤쪽 UI의 크기 조절 커서가 남는다",
        ),
        (
            "self.dismiss_pending_native_menu();",
            "떠 있던 네이티브 메뉴가 무대 위에 남는다",
        ),
        (
            "self.state.dialogs.pending_native_menu = None;",
            "진입 직전 큐잉된 메뉴 요청이 무대를 나온 뒤 엉뚱한 자리에 뜬다",
        ),
        (
            "self.cancel_outbound_drag();",
            "아무도 누르고 있지 않은 파일 드래그가 시작된다",
        ),
    ] {
        assert!(
            body.contains(line),
            "무대 진입 정리에서 `{line}` 이 사라졌다 — {why}."
        );
    }
}

fn fn_body<'a>(src: &'a str, signature: &str) -> &'a str {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let start = only_at(&code, signature, "함수 시그니처");
    let open = start + code[start..].find('{').expect("function opening brace");
    let close =
        tasty_doc_guards::match_arms::matching_close(&code, open).expect("function closing brace");
    &src[open + 1..close]
}

/// IME 경로는 무대를 아는 정면 함수(`window_keys_to_overlay`)를 거쳐야 한다.
#[test]
fn ime_overlay_gate_knows_the_stage() {
    let src = read("src/view/main/ime.rs");
    let expected =
        "    ImeRoute::To(state.input_destination(engine, state.window_keys_to_overlay()))";
    assert!(
        src.contains(expected),
        "IME 의 `overlay_open` 이 무대를 보지 않는다 — 무대 중 IME Preedit/Commit 이 \
         뒤 터미널 PTY 로 샌다."
    );
}

/// 플러그인 단축키는 일반 키보드 처리 전에 실행되므로 따로 막는다. winit 경로 키는 포커스 노드(무대 중에는 무대
/// 노드)에서 시작한 경로로 판정하고(`plugin_key_gate_open`, 무대가 쓰는 키와 종료 키는 닫힘), 나머지 키는 무대를 아는
/// 정면 함수(`window_keys_to_overlay`)로 막는다.
#[test]
fn plugin_shortcut_gate_knows_the_stage() {
    let src = read("src/app/plugin_glue/shortcut.rs");
    for expected in [
        "        _ => !state.window_keys_to_overlay(),",
        "(None, PluginKeyGate::Winit(value)) => {\n            crate::adapters::ui::input::key_nodes::plugin_key_gate_open(",
    ] {
        assert!(
            src.contains(expected),
            "플러그인 단축키 경로에 무대 차단 조건이 없다(`{expected}`). 일반 키보드 처리보다 먼저 실행되므로 여기서 \
             별도로 막아야 한다."
        );
    }
    let nodes = read("src/adapters/ui/input/key_nodes.rs");
    assert!(
        nodes.contains("FocusNode::Stage => vec![KeyNode::Stage {"),
        "경로 키 게이트의 무대 출발 노드가 없다"
    );
}

/// 창 키 경로는 정면 함수 `window_keys_to_overlay`(본문이 무대를 함께 본다) 하나를 거친다. `keyboard_overlay_open`
/// 을 직접 부르는 곳은 그 정면 함수와, 앞선 0단계 무대 게이트가 있는 창의 터미널 게이트(`window_holds_terminal_keys`)
/// 둘뿐이어야 한다. mouse_overlay_open과 has_egui_overlay_open은 정의에 무대 조건이 있어야 한다.
#[test]
fn every_overlay_open_composite_is_stage_aware() {
    const FRONT: &str = "fn window_keys_to_overlay(";
    const TERMINAL_GATE: &str = "fn window_holds_terminal_keys(";
    let mut files = Vec::new();
    collect_rs(&tasty_doc_guards::repo_root().join("src"), &mut files);

    let state = read("src/state.rs");
    let front = fn_body(&state, FRONT);
    assert!(
        front.contains("self.keyboard_overlay_open() || self.fullscreen_stage_active()"),
        "정면 함수가 무대를 함께 보지 않는다:\n{front}"
    );

    let mut direct = Vec::new();
    let mut callers = Vec::new();
    // 시험과 소스 가드는 호출부가 아니다. 장면 시험은 같은 함수를 불러 재고, 가드는 그 이름을 문자열로 담는다.
    let production = |rel: &str| {
        let file = rel.rsplit('/').next().unwrap_or(rel);
        !rel.starts_with("src/source_guards/")
            && !rel.contains("/tests/")
            && !file.ends_with("_tests.rs")
            && file != "tests.rs"
    };
    for rel in files.iter().filter(|rel| production(rel)) {
        let src = read(rel);
        for (i, _) in src.match_indices(".keyboard_overlay_open()") {
            // 이 호출을 담은 함수.
            let within = src[..i].rfind("fn ").map_or("", |f| &src[f..i]);
            if within.starts_with(FRONT) {
                direct.push(format!("{rel} front"));
            } else if within.starts_with(TERMINAL_GATE) {
                let keyboard = read("src/view/main/keyboard.rs");
                assert!(
                    keyboard.contains("self.try_consume_fullscreen_stage_key(engine, event)")
                        && keyboard.contains(
                            "let overlay_open = self.state.window_key_holds_terminal(self.key_decision);"
                        )
                        && read("src/state/popup_key_owner.rs")
                            .contains("KeyDecision::Current => self.window_holds_terminal_keys(),"),
                    "{rel}: 무대 항 없이 예외로 허용되던 근거(keyboard.rs 의 0단계 무대 게이트와 그 뒤 터미널 게이트)가 사라졌다."
                );
                direct.push(format!("{rel} terminal"));
            } else {
                direct.push(format!("{rel} other"));
            }
        }
        if src.contains(".window_keys_to_overlay()") {
            callers.push(rel.clone());
        }
    }
    direct.sort();
    assert_eq!(
        direct,
        vec![
            "src/state.rs front".to_string(),
            "src/state/popup_key_owner.rs terminal".to_string(),
        ],
        "`keyboard_overlay_open()` 을 정면 함수 밖에서 부른다. 무대를 아는 `window_keys_to_overlay()` 를 쓴다."
    );
    callers.sort();
    assert_eq!(
        callers,
        vec![
            "src/adapters/ipc/handler/debug_state.rs".to_string(),
            "src/adapters/ui/input/explorer_ime.rs".to_string(),
            "src/adapters/ui/input/key_nodes.rs".to_string(),
            "src/app/plugin_glue/shortcut.rs".to_string(),
            "src/app/webview_sync.rs".to_string(),
            "src/state/popup_key_owner.rs".to_string(),
            "src/view/main/edit_keys.rs".to_string(),
            "src/view/main/file_drag.rs".to_string(),
            "src/view/main/ime.rs".to_string(),
            "src/view/main/plugin_surface_keys.rs".to_string(),
        ],
        "`window_keys_to_overlay()` 호출부 집합이 바뀌었다. 새 지점이 생겼다면 \
         docs/architecture/input-layer.md 의 열거도 함께 갱신하라."
    );

    let mouse = read("src/view/main/mouse.rs");
    assert!(
        mouse
            .contains("self.state.settings_open_requested || self.state.fullscreen_stage_active()"),
        "`mouse_overlay_open()` 정의에서 무대가 빠졌다 — 마우스 전 경로가 무대를 모르게 된다."
    );
    let webview = fn_body(&state, "pub fn has_egui_overlay_open(&self) -> bool {");
    assert!(
        webview.contains("self.fullscreen_stage.is_some()"),
        "`has_egui_overlay_open()` 에서 무대가 빠졌다 — WebView 가 무대를 뚫고 나온다."
    );
}

/// 수집 경로는 저장소 루트 기준이어야 read가 같은 파일을 연다.
fn collect_rs(dir: &std::path::Path, out: &mut Vec<String>) {
    let root = tasty_doc_guards::repo_root();
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path.strip_prefix(&root).expect("under manifest dir");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// 오버레이 조건의 매개변수마다 ui.state의 원인 필드가 있는지 대조한다. 무대 조건은 함수 밖에서 적용하므로 따로 확인한다.
#[test]
fn the_gate_report_has_a_slot_for_every_predicate_parameter() {
    let state = read("src/state.rs");
    let sig_at = only_at(
        &state,
        "fn keyboard_overlay_open(\n",
        "게이트 술어의 자유함수 서명",
    );
    let open = state[sig_at..].find('(').unwrap() + sig_at;
    let close = state[open..].find(')').expect("서명이 안 닫힌다") + open;
    let params: Vec<String> = state[open + 1..close]
        .split(',')
        .filter_map(|p| p.split(':').next())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    assert!(
        params.len() >= 4,
        "매개변수를 못 읽었다 — 서명 파싱이 깨졌다: {params:?}"
    );

    let reporter = read("src/adapters/ipc/handler/debug_state.rs");
    for p in &params {
        assert!(
            reporter.contains(&format!("\"gate_{p}\":")),
            "오버레이 조건 {p}의 ui.state 보고 필드 gate_{p}가 없다. 새 조건을 추가했다면 원인 보고도 갱신한다."
        );
    }
    assert!(
        reporter.contains("\"gate_fullscreen_stage_active\":"),
        "무대 조건의 보고 필드가 없다. 일반 오버레이 함수의 매개변수에 없는 별도 조건이므로 직접 추가한다."
    );
}

#[test]
fn body_reader_does_not_borrow_calls_from_the_next_method() {
    let source =
        "impl X { pub(crate) fn prepare() { let _ = \"}\"; } pub(crate) fn finish() { poll(); } }";
    assert!(!fn_body(source, "fn prepare(").contains("poll()"));
    assert!(fn_body(source, "fn finish(").contains("poll()"));
}
