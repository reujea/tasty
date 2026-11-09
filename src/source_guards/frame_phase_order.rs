//! 그리는 창의 단계 순서를 소스에서 확인한다. grid → tap → 입력·mesh → draw → HTML gate → native sync →
//! lazy WebView 생성 통지(그 프레임에 만든 것을 알린다).
//! 각 drain은 바로 앞 단계까지의 효과를 다음 단계 전에 적용하며, 호출 위치와 넘기는 단계가 일치해야
//! 한다. resize가 입력 준비 뒤로 밀리면 입력 context가 옛 grid를 읽고, HTML 허용이 webview 동기화 뒤로 밀리면 허용 직후 reload가 한 프레임 늦는다.
//! 텍스트의 호출 위치를 앞에서부터 차례로 찾으며 조건 분기의 실제 실행 여부는 판단하지 않는다.

use super::{fn_body, repo_root, strip_comments};

const HOME: &str = "src/app/view_frame.rs";
const FRAME_FN: &str = "fn redraw_main_window(";
const DRAIN: &str = "apply_frame_effects(";

/// 순서대로 나타나야 하는 호출. 같은 문자열은 앞 단계 뒤에서 다시 찾는다.
const STEPS: &[&str] = &[
    "view.prepare_redraw(",
    DRAIN,
    "FramePhase::Geometry",
    "view.prepare_render_inputs(",
    DRAIN,
    "FramePhase::Inputs",
    "view_mesh::relay_subscribed_mesh(",
    "view.render_if_dirty(",
    DRAIN,
    "FramePhase::Draw",
    "view.finish_redraw(",
    "webview_sync::synchronize(",
    "webview_sync::announce_lazy_webviews(",
    "webview_sync::settle_overlay_host(",
];

fn frame_body() -> String {
    let path = repo_root().join(HOME);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, FRAME_FN).unwrap_or_else(|| {
        panic!("{HOME}에서 `{FRAME_FN}` 본문을 읽지 못했다. 이름·이동 여부와 함수 추출을 확인한다.")
    });
    strip_comments(&body)
}

/// 각 단계의 첫 위치를 앞 단계 뒤에서 찾는다. 빠진 단계가 있으면 그 이름을 돌려준다.
fn ordered_positions(body: &str, steps: &[&str]) -> Result<Vec<usize>, String> {
    let mut from = 0;
    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        let at = body[from..]
            .find(step)
            .map(|at| from + at)
            .ok_or_else(|| (*step).to_string())?;
        out.push(at);
        from = at + step.len();
    }
    Ok(out)
}

#[test]
fn frame_phases_drain_between_geometry_inputs_draw_and_native_sync() {
    let body = frame_body();
    if let Err(step) = ordered_positions(&body, STEPS) {
        panic!(
            "`{FRAME_FN}`에서 `{step}`가 앞 단계 뒤에 없다. 단계 순서는 {STEPS:?} 이다. \
             resize 효과는 입력 준비 전에, HTML gate는 finish_redraw·webview 동기화 전에 적용해야 한다."
        );
    }
}

#[test]
fn the_frame_has_exactly_three_drains() {
    let body = frame_body();
    assert_eq!(
        body.matches(DRAIN).count(),
        3,
        "`{FRAME_FN}`의 `{DRAIN}` 호출 수가 바뀌었다. 단계가 늘거나 줄면 STEPS와 단계 정의를 함께 고친다."
    );
}

/// 판정 함수가 뒤바뀐 순서를 실제로 검출하는지 합성 입력으로 확인한다.
#[test]
fn a_swapped_order_is_detected() {
    let good = "a(); d(); b(); d(); c();";
    assert!(ordered_positions(good, &["a(", "d(", "b(", "d(", "c("]).is_ok());
    // drain이 입력 준비 뒤로 밀리면 drain 뒤에서 입력 준비를 찾지 못한다.
    let resize_after_input = "a(); b(); d(); c();";
    assert_eq!(
        ordered_positions(resize_after_input, &["a(", "d(", "b(", "c("]),
        Err("b(".to_string())
    );
    // gate drain이 native 동기화 뒤로 밀리면 drain 뒤에서 동기화를 찾지 못한다.
    let gate_after_sync = "a(); c(); d();";
    assert_eq!(
        ordered_positions(gate_after_sync, &["a(", "d(", "c("]),
        Err("c(".to_string())
    );
}
