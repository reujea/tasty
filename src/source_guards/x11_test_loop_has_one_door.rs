//! 시험이 X11 winit `EventLoop` 를 만들 때는 `src/x11_test_loop.rs` 의 도우미를 지난다.
//! 도우미는 연결 실패 때 DISPLAY·인증 파일·소켓·lock·fd 상태를 싣는다. 직접 `.with_x11()` 로 만들고
//! `unwrap()`·`expect()` 하면 간헐적 `XOpenDisplayFailed` 의 원인을 가를 정보가 남지 않는다.
//!
//! `src`·`crates` 소스에서 주석·리터럴을 가린 뒤 `with_x11(` 호출 형태를 찾는다. `unwrap` 이 아니라
//! builder 호출 자체를 본다. 다른 이름으로 다시 내보낸 trait 메서드나 매크로 안의 조립은 추적하지 않는다.

use tasty_doc_guards::source_text::mask_non_code;

const DOOR: &str = "src/x11_test_loop.rs";

const CALL: &str = "with_x11(";

/// 시험이 아닌 곳의 예외 (파일, 사유).
const EXCEPTIONS: &[(&str, &str)] = &[(
    "src/boot/display_backend.rs",
    "제품 부팅 경로가 사용자가 고른 X11 백엔드로 EventLoop 를 만든다 — 시험 도우미의 대상이 아니다",
)];

/// 도우미와 예외 밖에서 `with_x11(` 를 부르는 (파일, 줄 번호, 줄).
fn offenders<'a>(sources: impl Iterator<Item = (String, &'a str)>) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, raw) in sources {
        if rel == DOOR || EXCEPTIONS.iter().any(|(f, _)| *f == rel) {
            continue;
        }
        for (i, line) in mask_non_code(raw).lines().enumerate() {
            if line.contains(CALL) {
                out.push(format!("  {rel}:{}  {}", i + 1, line.trim()));
            }
        }
    }
    out
}

#[test]
fn x11_event_loops_in_tests_go_through_the_helper() {
    let sources = super::rust_sources();
    let bad = offenders(
        sources
            .iter()
            .map(|(p, raw)| (p.to_string_lossy().replace('\\', "/"), raw.as_str())),
    );
    assert!(
        bad.is_empty(),
        "`{DOOR}` 밖에서 X11 EventLoop 를 직접 만드는 곳이 {}개다:\n{}\n시험은 crate::x11_test_loop::x11_event_loop() 를 쓴다. 제품 코드라면 EXCEPTIONS 에 사유를 적는다.",
        bad.len(),
        bad.join("\n")
    );

    let door = sources
        .iter()
        .find(|(p, _)| p.to_string_lossy().replace('\\', "/") == DOOR)
        .unwrap_or_else(|| panic!("`{DOOR}` 를 수집하지 못했다 — 옮겼으면 DOOR 를 따라 고친다"));
    assert!(
        mask_non_code(&door.1).contains(CALL),
        "`{DOOR}` 에 `{CALL}` 가 없다 — 도우미가 바뀌었으면 이 검사를 함께 고친다"
    );
    for (rel, reason) in EXCEPTIONS {
        let raw = std::fs::read_to_string(super::repo_root().join(rel)).unwrap_or_default();
        assert!(
            mask_non_code(&raw).contains(CALL),
            "예외 `{rel}`(\"{reason}\")에 `{CALL}` 가 없다. 옮겼으면 명부를 따라 고치고, 필요 없어졌으면 지운다."
        );
    }
}

#[test]
fn a_direct_builder_in_a_test_file_is_reported() {
    let planted = "fn event_loop() {\n    EventLoop::builder()\n        .with_x11()\n        .build()\n        .expect(\"x\");\n}\n";
    let commented = "// .with_x11() is not called here\nconst S: &str = \".with_x11()\";\n";
    let bad = offenders(
        [
            ("src/app/new_tests.rs".to_string(), planted),
            ("src/app/doc_only.rs".to_string(), commented),
            (DOOR.to_string(), planted),
            ("src/boot/display_backend.rs".to_string(), planted),
        ]
        .into_iter(),
    );
    assert_eq!(
        bad,
        vec!["  src/app/new_tests.rs:3  .with_x11()".to_string()]
    );
}
