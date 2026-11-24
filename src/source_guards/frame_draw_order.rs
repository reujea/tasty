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

/// host 팝업이 위일 때 포커스 띠는 egui 패스 안에서 합성될 plugin 팝업 본문의 clip으로 구멍을 내고, 합성은
/// egui 패스 뒤에 한다. 둘이 같은 프레임의 같은 clip을 쓰려면 본문 디코드가 egui 패스보다 앞이어야 한다.
/// 디코드가 합성 단계에만 있으면 첫 프레임과 clip이 바뀌는 프레임에 띠 구멍이 한 프레임 늦는다. 합성 단계가
/// 다시 디코드하면 그 사이 plugin이 쓴 새 프레임으로 띠 구멍과 다른 본문을 합성한다.
#[test]
fn the_popup_bodies_are_decoded_before_the_edge_reads_them() {
    let body = frame_body();
    let prepare = body
        .find("self.prepare_mesh_popups(")
        .expect("`run_egui_frame` 이 egui 패스 전에 팝업 본문을 디코드해야 한다");
    let bodies = body
        .find("popup_scope::bodies_to_decode(")
        .expect("사전 디코드 대상은 직전 프레임 합성 영역에서 골라야 한다(접힌 팝업 제외)");
    assert!(bodies < prepare, "디코드 대상을 고른 뒤 디코드한다");
    let composed = body
        .find("self.composed_popups()")
        .expect("띠가 읽는 합성 본문");
    let pass = body.find("self.run_main_pass(").expect("egui 패스");
    let redraw = body
        .find("popup_scope::redraw_for_undecoded_bodies(")
        .expect("본문 없이 합성 영역을 받은 팝업은 다음 프레임을 요청해야 한다");
    assert!(
        pass < redraw,
        "다음 프레임 요청은 이번 패스의 합성 영역을 본 뒤다"
    );
    assert!(
        prepare < composed && composed < pass,
        "팝업 본문 디코드 → 합성 본문 읽기 → egui 패스 순서가 아니다"
    );
    let path = repo_root().join("src/gfx/gpu/egui_mesh_prepare.rs");
    let src = strip_comments(
        &std::fs::read_to_string(&path)
            .expect("egui_mesh_prepare.rs")
            .replace("\r\n", "\n"),
    );
    // 합성 함수의 시그니처부터 다음 함수 정의 앞까지를 본문으로 읽는다.
    let start = src
        .find("fn render_egui_mesh_popups(")
        .expect("egui_mesh_prepare.rs 에 팝업 합성 함수가 있어야 한다");
    let rest = &src[start + 1..];
    let end = rest
        .find("\n    pub(super) fn ")
        .or_else(|| rest.find("\n    fn "));
    let composite = &rest[..end.unwrap_or(rest.len())];
    assert!(
        composite.contains("composite_mesh_target(")
            && !composite.contains("decode_mesh_popup(")
            && !composite.contains("decode_and_track("),
        "팝업 합성은 egui 패스 전에 디코드한 본문을 그대로 합성하고 다시 디코드하지 않는다"
    );
}
