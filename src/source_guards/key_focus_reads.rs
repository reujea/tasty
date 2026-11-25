//! 칠하기·강조 해제·mesh 통지·WebView 바탕이 선택(`focused_surface_id`)이 아니라 프레임마다 정한 host 키 포커스
//! (`state::key_focus`)를 읽는지 소스에서 확인한다. 자리마다 (파일, 감싼 `fn`, 그 안에 있어야 할 줄들)이다. 줄은
//! 공백을 지워 비교한다. 줄을 선택 값으로 되돌리거나 지우면 실패한다. mesh의 IME 위치는 입력용이라 선택을 읽는 줄도
//! 함께 고정한다. 잡지 못하는 것: 같은 이름의 변수에 다른 값을 넣는 자리(그 값을 만드는 줄도 고정해 줄인다).

use super::{fn_body, repo_root, strip_comments};

/// (파일, 함수 서명 앞부분, 본문에 있어야 할 줄들).
const SITES: &[(&str, &str, &[&str])] = &[
    (
        "src/gfx/gpu.rs",
        "fn prepare_layout(",
        &[
            "let key_focus = state.frame_key_focus();",
            "(pane_rects, dividers, key_focus)",
        ],
    ),
    (
        "src/gfx/gpu/render_pass.rs",
        "fn render_terminals(",
        &[
            "crate::state::surface_focus::TerminalFocusPaint::of(key_focus,*surface_id,&engine.live.occupancy,",
            "let is_focused = paint.bed_focused;",
            "let bg = if is_focused {term_surface.focused_bg",
            "let fg = if is_focused {term_surface.focused_fg",
            "SearchHighlights::for_theme(&theme,is_focused,",
            "let show_cursor = paint.show_cursor;",
        ],
    ),
    (
        "src/adapters/ui/egui_panels.rs",
        "fn draw_egui_panels(",
        &[
            "let key_focus = state.frame_key_focus();",
            "crate::state::surface_focus::bed_focused(key_focus,sid,&engine.live.occupancy);",
        ],
    ),
    (
        "src/adapters/ui/surface_focus_edge.rs",
        "fn collect(",
        &[
            "let focused = state.frame_key_focus();",
            "let is_focused = focused == Some(r.id);",
        ],
    ),
    (
        "src/app/webview_sync/bed_focus.rs",
        "fn wanted(",
        &["let focused = view.state.shown_bed_focus() == Some(sid);"],
    ),
    (
        "src/view/main/redraw.rs",
        "fn focus_observed_effect(",
        &["let sid = state.frame_key_focus()?;"],
    ),
    (
        "src/view/main/egui_mesh.rs",
        "fn forward_egui_mesh_context(",
        &[
            "let focused = self.state.focused_surface_id(engine);",
            "let key_focus = self.state.frame_key_focus();",
            "let told = crate::state::key_focus::told_focused(key_focus, sid);",
            "let focus_changed = crate::state::key_focus::focus_changed(st.last_focused, told);",
            "st.last_focused = Some(told);",
            "focused: told,",
        ],
    ),
    (
        "src/view/main/attach_mesh_input.rs",
        "fn forward_attach_mesh_context(",
        &[
            "let key_focus = self.state.frame_key_focus();",
            "let is_focused = crate::state::key_focus::told_focused(key_focus, sid);",
            "let focus_changed = crate::state::key_focus::focus_changed(st.last_focused, is_focused);",
            "if geom_changed || theme_changed || focus_changed {",
            "focused: is_focused,",
        ],
    ),
];

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn body(path: &str, signature: &str) -> String {
    let src = std::fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|e| panic!("{path} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, signature)
        .unwrap_or_else(|| panic!("{path}에서 `{signature}` 본문을 읽지 못했다"));
    squash(&strip_comments(&body))
}

#[test]
fn paint_and_focus_behaviors_read_the_frame_key_focus() {
    let mut missing = Vec::new();
    for (path, signature, lines) in SITES {
        let body = body(path, signature);
        for line in *lines {
            if !body.contains(&squash(line)) {
                missing.push(format!("{path} `{signature}`: {line}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "키 포커스를 읽어야 할 자리가 바뀌었다. 칠하기·강조 해제·mesh 통지는 `frame_key_focus`, WebView 바탕은 \
         `shown_bed_focus`를 읽는다(`state::key_focus`):\n{}",
        missing.join("\n")
    );
}

/// 프레임 값은 그 프레임의 첫 소비자(FocusObserved)보다 먼저, 그리기는 준비를 건너뛰었어도 그리기 전에 정한다.
#[test]
fn the_frame_value_is_filled_before_its_first_reader() {
    let prepare = body("src/view/main/redraw.rs", "fn prepare_render_inputs(");
    let fill = prepare
        .find(&squash("self.state.refresh_frame_key_focus(engine);"))
        .expect("prepare_render_inputs fills the frame value");
    let observe = prepare
        .find(&squash(
            "focus_observed_effect(&self.state, engine, window_focused)",
        ))
        .expect("the observation");
    let mesh = prepare
        .find(&squash("self.forward_egui_mesh_context(engine, mgr);"))
        .expect("the mesh context");
    assert!(fill < observe && fill < mesh, "filled first");

    let render = body("src/view/main/redraw.rs", "fn render_if_dirty(");
    let ensure = render
        .find(&squash("self.state.ensure_frame_key_focus(engine);"))
        .expect("render_if_dirty fills a frame that skipped preparation");
    let submit = render
        .find(&squash("self.submit_gpu_frame("))
        .expect("submit");
    let finish = render
        .find(&squash("self.state.finish_frame_key_focus(engine, shown);"))
        .expect("the shown frame is recorded");
    assert!(ensure < submit && submit < finish);

    let done = body("src/view/main/redraw.rs", "fn finish_redraw(");
    let moved = done
        .find(&squash(
            "if self.state.key_focus_moved_since_frame(engine) {",
        ))
        .expect("a move during the frame asks for the next one");
    let request = done
        .find(&squash("self.base.winit.request_redraw();"))
        .expect("request");
    assert!(moved < request);
}
