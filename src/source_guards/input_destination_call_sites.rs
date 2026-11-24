//! 창의 키 처리와 IME 처리가 egui 로 보내지 않은 사건의 갈래를 `MainViewState::input_destination` 하나로 고르는지
//! 확인한다. 판정의 동작은 장면 시험(`adapters::ui::popup::route::tests`)이 그 함수를 직접 불러 잰다. 창 호출부가 그
//! 함수를 거치지 않고 갈래를 고르면 장면 시험은 보지 못하므로 호출부의 모양을 여기서 고정한다.
//!
//! 터미널 갈래는 갈래가 고른 surface 를 PTY 전송 끝까지 쓴다(`forward_key_to_terminal`이 포커스 surface 를 다시 고르지
//! 않는다). 잡지 못하는 것: 갈래 안에서 보내는 함수의 쓰기 자체.

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

/// 키: 터미널 게이트(경로 키는 단계 4 수신자, 그 밖은 `window_holds_terminal_keys`)를 넘긴 판정으로 갈래를 고르고,
/// 터미널 갈래만 PTY 로 보낸다.
#[test]
fn the_window_key_path_picks_its_branch_through_the_input_destination() {
    let src = read("src/view/main/keyboard.rs");
    let body = squeeze(&fn_body(&src, "fn handle_keyboard_input(").expect("handle_keyboard_input"));
    assert_eq!(
        body.matches("letoverlay_open=self.state.window_key_holds_terminal(self.key_decision);")
            .count(),
        1,
        "터미널 게이트"
    );
    for arm in [
        "Some(KeyReceiver::Surface(sid)|KeyReceiver::Shortcut{end:KeyEnd::Surface(sid),},)=>(self.state.surface_input_destination(engine,sid),Some(sid)),",
        "Some(_)=>return,",
        "None=>(self.state.input_destination(engine,overlay_open),self.state.focused_surface_id(engine),),",
        "matchdestination{",
        "InputDestination::Overlay=>return,",
        "InputDestination::Terminal(sid)=>self.forward_key_to_terminal(engine,sid,event),",
        "InputDestination::EguiMesh(sid)=>self.forward_key_to_egui_mesh(sid,event),",
        "InputDestination::AttachMesh(sid)=>self.forward_key_to_attach_mesh(sid,event),",
    ] {
        assert_eq!(body.matches(arm).count(), 1, "키 갈래 `{arm}`");
    }
    assert_eq!(
        body.matches("forward_key_to_terminal(").count(),
        1,
        "PTY 로 보내는 자리는 터미널 갈래 하나다"
    );
    // 터미널 갈래의 끝은 받은 sid 다. 줄바꿈 설정·터미널 상태·전송 대상·스크롤을 그 sid 로 정하고 포커스 surface 를 다시
    // 고르지 않는다.
    let forward =
        squeeze(&fn_body(&src, "fn forward_key_to_terminal(").expect("forward_key_to_terminal"));
    for use_sid in [
        "shift_enter_newline_for(engine,Some(sid))",
        "engine.as_ref().visible_terminal(sid)",
        "DomainIntent::SendToSurface{surface_id:sid,payload,}",
        "self.apply_keyboard_scroll_action(engine,sid,outcome.scroll_action);",
    ] {
        assert_eq!(forward.matches(use_sid).count(), 1, "터미널 끝 `{use_sid}`");
    }
    let scroll = squeeze(
        &fn_body(&src, "fn apply_keyboard_scroll_action(").expect("apply_keyboard_scroll_action"),
    );
    for body in [&forward, &scroll] {
        assert!(
            !body.contains("focused_surface_id"),
            "터미널 끝이 포커스 surface 를 다시 고른다"
        );
    }
}

/// IME: 갈래 판정 함수(`ime::route`)가 키·IME 오버레이 게이트(`window_keys_to_overlay`)를 넘긴 판정으로 갈래를 고르고,
/// 창의 IME 처리(`handle_event`)는 그 함수의 값으로만 갈래를 고른다. 오버레이·mesh 갈래가 아니면 터미널 조합으로 간다.
#[test]
fn the_window_ime_path_picks_its_branch_through_the_input_destination() {
    let src = read("src/view/main/ime.rs");
    let route = squeeze(&fn_body(&src, "fn route(").expect("route"));
    assert_eq!(
        route
            .matches("ImeRoute::To(state.input_destination(engine,state.window_keys_to_overlay()))")
            .count(),
        1,
        "IME 판정"
    );
    let body = squeeze(&fn_body(&src, "fn handle_event(").expect("handle_event"));
    for arm in [
        "letdestination=matchroute(&w.state,&mutw.ime_request_target,engine,&event,egui_consumed,){",
        "ImeRoute::To(destination)=>destination,",
        "matchdestination{",
        "InputDestination::EguiMesh(sid)=>{forward_ime_to_egui_mesh(w,sid,event);",
        "InputDestination::AttachMesh(sid)=>{forward_ime_to_attach_mesh(w,sid,event);",
        "InputDestination::Terminal(_)|InputDestination::Other(_)=>{}",
    ] {
        assert_eq!(body.matches(arm).count(), 1, "IME 갈래 `{arm}`");
    }
    assert!(
        !body.contains("window_keys_to_overlay(")
            && !body.contains("input_destination(")
            && !body.contains("focused_egui_mesh_surface_id"),
        "IME 갈래를 판정 밖에서 고른다"
    );
}

/// 버튼: mesh에서 누른 버튼의 뗌은 메뉴 닫기·분할선·위 층 분기보다 먼저 기록한 leaf로 간다. 나머지 사건은 위 층
/// 판정 뒤 로컬·attach 공통 판정 하나로 mesh에 간다. 판정의 동작은 `state::mesh_frames` 시험이 단계 순서대로
/// 불러 잰다. 창 호출부가 그 순서를 지키지 않으면 그 시험은 보지 못하므로 호출부의 모양을 여기서 고정한다.
#[test]
fn the_window_button_path_sends_a_recorded_mesh_release_first() {
    let src = read("src/view/main/mouse.rs");
    let body = squeeze(&fn_body(&src, "fn handle_mouse_input(").expect("handle_mouse_input"));
    let find = |needle: &str| {
        assert_eq!(
            body.matches(needle).count(),
            1,
            "`{needle}` 이 하나가 아니다"
        );
        body.find(needle).unwrap_or_default()
    };
    let recorded =
        find("ifself.forward_recorded_mesh_release(engine,button,button_state){return;}");
    let order = [
        recorded,
        find("ifmenu_dismiss_swallow{"),
        find("self.try_press_divider_first("),
        find("button_stays_above("),
        find("ifself.try_forward_mesh_button(engine,button,button_state){return;}"),
    ];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "기록된 뗌 → 메뉴 닫기 → 분할선 → 위 층 → mesh 순서가 아니다: {order:?}"
    );
    for gone in [
        "try_forward_egui_mesh_button",
        "try_forward_attach_mesh_button",
    ] {
        assert!(
            !src.contains(gone),
            "mesh 버튼 전달은 한 판정으로 간다: {gone}"
        );
    }
}

/// 창의 두 mesh 버튼 메서드는 조건 없이 공용 전달로 넘기고, 공용 전달은 창 상태를 [`route_mesh_button`]에
/// 넘긴다. 판정·전달은 MainView 없이 그 함수를 거치는 시험(`view::main::egui_mesh::button_tests`)이 잰다.
/// 메서드 안에 분할선 드래그 등으로 건너뛰는 조건이 생기면 그 시험은 보지 못하므로 모양을 여기서 고정한다.
#[test]
fn the_window_mesh_button_methods_hand_off_without_conditions() {
    let src = read("src/view/main/egui_mesh.rs");
    for (name, pass) in [
        ("fn forward_recorded_mesh_release(", "Recorded"),
        ("fn try_forward_mesh_button(", "Below"),
    ] {
        let body = squeeze(&fn_body(&src, name).expect(name));
        let call = format!("self.forward_mesh_button(engine,button,button_state,MeshPass::{pass})");
        assert!(
            body.matches(call.as_str()).count() == 1
                && !body.contains("if")
                && !body.contains("return")
                && !body.contains("dragging"),
            "{name} 은 조건 없이 공용 전달로 넘긴다: {body}"
        );
    }
    let body = squeeze(&fn_body(&src, "fn forward_mesh_button(").expect("forward_mesh_button"));
    assert_eq!(
        body.matches("route_mesh_button(window,pass,button,is_pressed(button_state))")
            .count(),
        1
    );
    let ifs = body.matches("if").count() - body.matches("modifiers").count();
    assert!(
        !body.contains("return") && ifs == 1 && body.contains("iftaken{self.mark_dirty();}"),
        "공용 전달은 가져간 사건에 다시 그리기만 더한다: {body}"
    );
}
