//! 전체화면 무대의 렌더 분기 순서와 캡처·화면 제출·격자 유지 경로를 소스에서 확인한다.
//! 서피스 캡처는 무대 분기 전에 처리하고, 원본 레이아웃 갱신은 무대 중 생략해야 한다.
//! 무대 화면의 window 캡처와 present는 유지하며 attach 중계가 있는 render_if_dirty를 중단하지 않는다.
//! 무대 상태는 레이아웃에 저장하지 않는다. 이 검사는 core/layout_persistence만 확인하고
//! app/persistence와 명시적인 프리셋 캡처는 검사하지 않는다.
//! 같은 redraw 함수의 순서에 의존하는 모달 열기 요청 처리도 함께 확인한다.
//! GPU 동작 자체가 아닌 호출·조건 문자열과 위치를 검사한다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::path::{Path, PathBuf};
use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, walk_with_floor};

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let p: PathBuf = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn only_at(hay: &str, needle: &str, what: &str) -> usize {
    let n = hay.matches(needle).count();
    assert_eq!(
        n, 1,
        "{what}: `{needle}` 이 {n} 번 나온다 — 이 가드는 유일 출현을 전제한다. \
         구조가 바뀌었으면 가드도 함께 갱신하라."
    );
    hay.find(needle).expect("checked above")
}

/// 주석·리터럴을 마스킹한 괄호 경계로 지정 함수만 읽는다.
fn fn_body<'a>(src: &'a str, header: &str) -> &'a str {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let start = only_at(&code, header, "함수 시그니처");
    let open = start + code[start..].find('{').expect("function opening brace");
    let close =
        tasty_doc_guards::match_arms::matching_close(&code, open).expect("function closing brace");
    &src[open + 1..close]
}

#[test]
fn stage_branch_sits_between_offscreen_capture_and_layout() {
    let src = read("src/gfx/gpu.rs");
    let screenshot = only_at(
        &src,
        "self.handle_pending_surface_screenshot(state, &engine.as_ref());",
        "offscreen 캡처",
    );
    // 무대 분기는 그리는 입력 몫의 포커스 노드로 고른다(`MainViewState::pass_draws_stage`).
    let branch = only_at(&src, "if state.pass_draws_stage(engine) {", "무대 분기");
    let resize = only_at(&src, "self.prepare_layout(", "배경 레이아웃 계산");
    assert!(
        screenshot < branch,
        "무대 분기가 offscreen surface 스크린샷보다 앞에 있다 — \
         `ui.screenshot --surface <id>` 가 무대 중 영구 대기하게 된다."
    );
    assert!(
        branch < resize,
        "무대 분기가 배경 레이아웃 계산 뒤로 밀렸다."
    );
}

#[test]
fn stage_frame_keeps_window_capture_and_present() {
    let src = read("src/gfx/gpu.rs");
    let body = fn_body(&src, "fn render_fullscreen_stage(");
    assert!(
        body.contains("self.pending_screenshot.take()"),
        "무대 프레임이 window 스크린샷 캡처를 건너뛴다 — `ui.screenshot` 요청이 \
         영구 대기하고, 무대 검증 수단이 사라진다."
    );
    assert!(
        body.contains("output.present()"),
        "무대 프레임이 present 를 건너뛴다 — 화면에 아무것도 올라가지 않는다."
    );
}

#[test]
fn render_if_dirty_has_no_stage_early_return() {
    let src = read("src/view/main/redraw.rs");
    let body = fn_body(&src, "fn render_if_dirty(");
    assert!(
        !body.contains("fullscreen_stage"),
        "render_if_dirty가 무대를 참조한다. attach 중계를 중단하지 않도록 무대 렌더 분기는 Gpu::render에 둔다."
    );
}

#[test]
fn terminal_resize_request_is_gated_in_prepare_redraw() {
    let src = read("src/view/main/redraw.rs");
    let body = fn_body(&src, "fn prepare_redraw(");
    assert!(
        body.contains("if !self.state.fullscreen_stage_active() {")
            && body
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
                .contains("self.state.resize_all("),
        "`prepare_redraw` 의 `state.resize_all` 가 무대 게이트를 잃었다 — 무대 중 \
         창 크기가 바뀌면 원본 grid 가 따라가 리플로우된다."
    );
}

/// 모달 열기 요청은 render_if_dirty 안의 egui 처리에서 설정된다.
/// 요청 처리가 render 뒤에 있으면 같은 프레임에서 설정·해제가 끝나 다른 입력 이벤트가 요청 상태를 볼 수 없다.
/// 여기서는 처리 순서만 확인하며 실제 지속 시간과 설정 함수의 호출 경로는 검증하지 않는다.
#[test]
fn the_modal_open_latch_is_cleared_before_the_pass_that_sets_it() {
    let src = read("src/view/main/redraw.rs");
    let body = fn_body(&src, "fn prepare_redraw(");

    body.find("self.dispatch_pending_modal_opens();").expect(
        "prepare_redraw에서 모달 열기 요청 처리를 찾지 못했다. 요청 상태가 남아 입력을 계속 차단하지 않는지 확인한다.",
    );
    let frame = read("src/app/view_frame.rs");
    let prepare = only_at(&frame, "view.prepare_redraw(", "입력/레이아웃 준비");
    let render = only_at(&frame, "view.render_if_dirty(", "렌더 호출");
    assert!(
        prepare < render,
        "모달 열기 요청 처리가 render_if_dirty 뒤에 있다. 같은 redraw에서 요청 설정과 해제가 끝나면 다른 입력 이벤트가 요청 상태를 볼 수 없다. 순서를 복원하거나 요청 상태를 읽는 경로와 계약을 함께 재검토한다."
    );
}

#[test]
fn window_resize_does_not_touch_the_grid_during_a_stage() {
    let src = read("src/view/main.rs");
    let arm_head = "WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {";
    let start = only_at(&src, arm_head, "resize 이벤트 arm");
    let rest = &src[start..];
    let end = rest
        .find("\n            WindowEvent::")
        .unwrap_or(rest.len());
    let arm = &rest[..end];

    let gate = arm
        .find("if self.state.fullscreen_stage_active() {")
        .expect(
            "resize 이벤트 arm 에 무대 게이트가 없다 — 무대 중 창 크기가 바뀌면 grid 가 \
             즉시 따라가 '진입 시점 값 유지' 계약이 깨진다.",
        );
    let gpu_resize = arm
        .find("self.base.gpu.resize(new_size);")
        .expect("resize 이벤트 arm 에 gpu.resize 가 없다");
    assert!(
        gpu_resize < gate,
        "gpu.resize 가 무대 게이트 안으로 들어갔다 — GPU 서페이스 크기는 무대 여부와 \
         무관하게 창을 따라가야 한다."
    );
    for after_gate in ["GeometryEffect::DefaultGrid {", ".resize_all("] {
        let at = arm
            .find(after_gate)
            .unwrap_or_else(|| panic!("resize 이벤트 arm 에 `{after_gate}` 가 없다"));
        assert!(
            at > gate,
            "`{after_gate}` 가 무대 게이트 밖에 있다 — 무대 중에도 grid 가 재계산된다."
        );
    }
}

/// 무대는 저장 대상이 아니다. 레이아웃 영속화의 모듈 파일과 하위 디렉터리를 모두 검사한다.
const PERSISTENCE_FLOOR: Floor = Floor {
    min: 4,
    measured: 6,
    measured_on: "2026-09-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::LaneTip("ee7a32349"),
    why_this_gap: "레이아웃 영속화 모듈의 Rust 파일은 2026-09-08의 ee7a32349에서 6개였다. 당시 형제 모듈 3272커밋의 한 변경에서 파일 수가 움직인 최대 단위 1을 기준으로 여유 2를 뒀다. 모듈을 하나의 파일로 합치는 변경은 이 범위를 넘으므로 실제 재구성과 수집 실패를 구별해 다시 측정해야 한다.",
};

/// 모듈 파일과 같은 이름의 디렉터리를 함께 수집하도록 상위 core에서 시작해 접두어로 좁힌다.
fn persistence_sources(root: &Path, floor: &Floor) -> Result<Vec<Walked>, String> {
    walk_with_floor(
        &root.join("src/core"),
        root,
        floor,
        Descend::SkipBuildCaches,
        &|found| found.rel.starts_with("src/core/layout_persistence") && found.rel.ends_with(".rs"),
    )
}

/// 무대 식별자 문자열의 참조를 찾는다. 실제 직렬화 여부를 분석하지는 않는다.
fn mentions_stage(text: &str) -> bool {
    text.contains("fullscreen_stage")
}

fn stage_referencing(files: &[Walked]) -> Vec<String> {
    let mut hits = Vec::new();
    for found in files {
        let text = std::fs::read_to_string(&found.path)
            .unwrap_or_else(|e| panic!("read {}: {e}", found.path.display()));
        if mentions_stage(&text) {
            hits.push(found.rel.clone());
        }
    }
    hits
}

#[test]
fn stage_state_is_not_persisted() {
    let root = repo_root();
    let files =
        persistence_sources(&root, &PERSISTENCE_FLOOR).unwrap_or_else(|why| panic!("{why}"));
    let hits = stage_referencing(&files);
    assert!(
        hits.is_empty(),
        "레이아웃 영속화에서 무대 참조를 찾았다: {hits:?}. 무대는 저장 대상이 아니다."
    );
}

#[test]
fn the_persistence_scan_reacts_to_a_planted_reference() {
    // 플랫폼마다 다른 시각 해상도에 의존하지 않도록 임시 경로는 PID와 단조 카운터로 구별한다.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let base = std::env::temp_dir().join(format!(
        "tasty-stage-persist-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let module_dir = base.join("src/core/layout_persistence");
    std::fs::create_dir_all(&module_dir).expect("픽스처 디렉토리");

    let plain = [
        (
            base.join("src/core/layout_persistence.rs"),
            "pub mod schema;\n",
        ),
        (module_dir.join("capture.rs"), "fn capture() {}\n"),
        (module_dir.join("restore.rs"), "fn restore() {}\n"),
        (module_dir.join("schema.rs"), "struct Snapshot;\n"),
    ];
    for (path, text) in &plain {
        std::fs::write(path, text).expect("픽스처 파일");
    }
    std::fs::write(
        base.join("src/core/state.rs"),
        "let x = fullscreen_stage_active();\n",
    )
    .expect("접두사 밖 파일");

    let probe = Floor {
        min: 2,
        measured: 4,
        measured_on: "2026-09-06",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "픽스처는 이 시험이 방금 만든 것이라 모수가 코드와 함께만 움직인다 — \
                       그래도 하한을 실측보다 낮게 두는 것은 이 자리의 물음이 파일 수가 \
                       아니라 순회가 살아 있는가이기 때문이다.",
    };

    let clean = persistence_sources(&base, &probe).expect("픽스처 순회가 하한에 걸렸다");
    assert_eq!(
        clean.len(),
        4,
        "픽스처에서 모은 수가 다르다 — 접두사가 모듈 밖까지 집었거나 못 미쳤다: {:?}",
        clean.iter().map(|f| f.rel.clone()).collect::<Vec<_>>()
    );
    assert!(
        stage_referencing(&clean).is_empty(),
        "무대를 안 쓰는 픽스처에서 위반이 나왔다 — 판정이 아무거나 집는다"
    );

    std::fs::write(
        module_dir.join("restore.rs"),
        "fn restore(s: &Snapshot) { s.fullscreen_stage; }\n",
    )
    .expect("참조 심기");
    let planted = persistence_sources(&base, &probe).expect("픽스처 순회가 하한에 걸렸다");
    assert_eq!(
        stage_referencing(&planted),
        vec!["src/core/layout_persistence/restore.rs".to_string()],
        "합성 입력에 추가한 무대 참조를 검출하지 못했다"
    );

    // 이유: 검사 후 임시 경로 정리 실패는 판정 결과에 영향을 주지 않는다.
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn body_reader_does_not_borrow_calls_from_the_next_method() {
    let source =
        "impl X { pub(crate) fn prepare() { let _ = \"}\"; } pub(crate) fn finish() { poll(); } }";
    assert!(!fn_body(source, "fn prepare(").contains("poll()"));
    assert!(fn_body(source, "fn finish(").contains("poll()"));
}
