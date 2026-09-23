//! 한 egui 프레임 안에서 **host popup 이 plugin popup 보다 먼저 그려지는지** 못 박는다.
//!
//! ## 계약
//!
//! `GpuState::run_egui_frame` 의 클로저 안에서 `ui::draw_popups` 가
//! `draw_plugin_popups` 보다 **먼저** 와야 한다. 이유는 페인트가 아니라 **한 프레임
//! 안의 읽기 방향**이다:
//!
//! - host 쪽이 이번 프레임 히트테스트 rect 를 적재하고 plugin 쪽이 **같은 프레임에**
//!   읽는다. 뒤집히면 지난 프레임 rect 를 읽는다.
//! - 반대 방향(`plugin_popup_hittest`)은 **1 프레임 stale 인 것이 정상**이다.
//!   뒤집으면 그 1 이 조용히 0 이 되고 두 판정이 서로를 같은 프레임에 물게 된다.
//!
//! 어기면 화면은 멀쩡하다 — 히트테스트가 한 프레임 어긋날 뿐이라 컴파일도 되고
//! 시험도 통과한다. 실측(변이): 두 호출을 맞바꿔도 죽는 시험이 **하나도 없었다.**
//!
//! ## 이 계약을 진술하던 자리가 셋이었다
//!
//! `adapters/ui/popup/draw.rs`(stale 이 아닌 이유) · `state.rs`(1 프레임 stale 인 이유) ·
//! 그리고 순서를 **정하는** `gfx/gpu/egui_bridge.rs`. 앞의 둘은 결과를 말하고 배치를
//! 정하지 않으므로 집이 아니다. 지금은 집이 `egui_bridge.rs` 하나고 나머지 둘은 그리로
//! 가리킨다.
//!
//! **하필 그 집에 오해를 부르는 문장이 있었다** — 바로 아래 z-order 주석의 "위 두 draw
//! 호출의 순서는 최종 페인트 순서에 영향이 없다". 페인트에 한정하면 참인데, 그대로
//! 읽으면 "순서가 아무래도 좋다" 가 된다. 그 문장의 범위를 좁혀 두었다.
//!
//! ## 왜 코드 옆 인라인이 아니라 여기인가
//!
//! `core/attach.rs` 에 같은 형태(호출 순서를 소스에서 읽는 가드)가 인라인
//! `#[cfg(test)]` 로 산다. 그 배치는 코드 옆이라 읽기는 좋은데 **찾기가 나쁘다** —
//! 실측: 가드 디렉토리 셋만 훑는 역방향 조사가 그런 가드 6 파일 / 8 단언을 통째로
//! 놓쳤고, 그 결과 이미 덮인 계약이 "산문뿐" 으로 세어졌다. 덮임을 과소평가하면 없는
//! 회귀에 가드를 더 얹게 된다. 그래서 찾을 수 있는 자리에 둔다.

use super::{fn_body, repo_root, strip_comments};

/// 계약이 집행되는 파일과 함수.
const HOME: &str = "src/gfx/gpu/egui_bridge.rs";
const FRAME_FN: &str = "fn run_egui_frame";
/// 먼저 와야 하는 호출 — 이번 프레임 host popup 히트테스트 rect 를 적재한다.
const HOST_DRAW: &str = "ui::draw_popups(";
/// 뒤에 와야 하는 호출 — 그 rect 를 같은 프레임에 읽는다.
const PLUGIN_DRAW: &str = "popup_render::draw_plugin_popups(";

fn frame_body() -> String {
    let path = repo_root().join(HOME);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, FRAME_FN).unwrap_or_else(|| {
        panic!(
            "`{FRAME_FN}` 을 {HOME} 에서 자르지 못했다 — 이름이 바뀌었거나 자리를 옮겼다. \
             못 자른 상태의 초록은 통과가 아니라 미측정이니 이 가드의 상수를 함께 고쳐라"
        )
    });
    strip_comments(&body)
}

#[test]
fn the_host_popups_are_drawn_before_the_plugin_popups() {
    let body = frame_body();
    let host = body.find(HOST_DRAW).unwrap_or_else(|| {
        panic!("`{FRAME_FN}` 안에 `{HOST_DRAW}` 가 없다 — host popup draw 가 사라졌다")
    });
    let plugin = body.find(PLUGIN_DRAW).unwrap_or_else(|| {
        panic!("`{FRAME_FN}` 안에 `{PLUGIN_DRAW}` 가 없다 — plugin popup draw 가 사라졌다")
    });
    assert!(
        host < plugin,
        "프레임 draw 순서 계약이 깨졌다: `{FRAME_FN}` 안에서 `{PLUGIN_DRAW}` 가 \
         `{HOST_DRAW}` 보다 먼저 온다. 그러면 plugin 쪽 히트테스트가 지난 프레임 rect 를 \
         읽고, host 쪽이 기대하는 1 프레임 stale 이 0 이 된다 — 화면은 멀쩡하고 어떤 \
         시험에도 안 걸린다. 계약 본문은 {HOME} 의 그 두 줄 위에 있다"
    );
}

#[test]
fn the_guard_reads_the_frame_closure_and_not_some_other_function() {
    // 공허 방지 — `fn_body` 가 엉뚱한 함수를 잘라도 두 호출이 우연히 들어 있을 수 있다.
    // 이 프레임 클로저에만 있는 이웃 호출로 잘라 온 것을 확인한다.
    let body = frame_body();
    for neighbour in ["enforce_foreground_z_order(", "draw_plugin_banners("] {
        assert!(
            body.contains(neighbour),
            "잘라 온 본문이 `{FRAME_FN}` 이 아니다 — `{neighbour}` 가 안 보인다"
        );
    }
}

#[test]
fn child_scope_is_resolved_before_the_first_host_paint() {
    let body = frame_body();
    let inherit = body
        .find("popup_scope::inherit_file_picker_scope(")
        .expect("child scope resolution");
    let host = body.find(HOST_DRAW).expect("host draw");
    assert!(
        inherit < host,
        "child scope must be applied before host paint and hit testing"
    );
}
