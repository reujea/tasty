//! host popup의 draw가 plugin popup보다 앞에 있는지 소스 순서를 확인한다.
//! plugin은 같은 프레임에서 host의 hit-test rect를 읽고, 반대 방향은 이전 프레임 값을 사용한다.
//! 최종 페인트 순서와 별개로 이 데이터의 갱신 순서를 지켜야 한다.
//! 텍스트의 호출 위치를 비교하며 조건 분기의 실제 실행 여부는 판단하지 않는다.

use super::{fn_body, repo_root, strip_comments};

const HOME: &str = "src/gfx/gpu/egui_bridge.rs";
const FRAME_FN: &str = "fn run_egui_frame";
const HOST_DRAW: &str = "ui::draw_popups(";
const PLUGIN_DRAW: &str = "popup_render::draw_plugin_popups(";

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
        "`{FRAME_FN}`에서 `{PLUGIN_DRAW}`가 `{HOST_DRAW}`보다 앞에 있다. plugin은 이전 프레임의 host rect를 읽고 host는 예상보다 이른 plugin rect를 읽을 수 있다. {HOME}의 두 호출 순서를 확인한다."
    );
}

#[test]
fn the_guard_reads_the_frame_closure_and_not_some_other_function() {
    // 본문 추출이 다른 함수를 반환하지 않았는지 이웃 호출도 확인한다.
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
        .find("popup_scope::sync_plugin_popup_scopes(")
        .expect("child scope resolution");
    let host = body.find(HOST_DRAW).expect("host draw");
    assert!(
        inherit < host,
        "child scope must be applied before host paint and hit testing"
    );
}
