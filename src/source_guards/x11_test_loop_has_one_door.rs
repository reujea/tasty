//! 시험이 X11 winit `EventLoop` 를 만들 때는 `src/x11_test_loop.rs` 의 도우미를 지난다.
//! 도우미는 연결 실패 때 DISPLAY·인증 파일·소켓·lock·fd 상태를 싣는다. 직접 `.with_x11()` 로 만들고
//! `unwrap()`·`expect()` 하면 간헐적 `XOpenDisplayFailed` 의 원인을 가를 정보가 남지 않는다.
//!
//! `src`·`crates` 소스에서 주석·리터럴을 가린 뒤 식별자 `with_x11` 다음에 공백·줄바꿈(가린 주석 포함)만
//! 두고 `(` 가 오는 호출을 찾는다. `unwrap` 이 아니라 builder 호출 자체를 본다.
//! 예외 파일은 `#[cfg(...test...)]` 항목 밖의 호출 수만 허용 수만큼 인정하고, 시험 항목 안의 호출은 위반이다.
//! 시험 항목은 `test` 낱말을 담고 `not(test` 를 담지 않는 cfg 속성 뒤의 첫 블록으로 본다.
//! 다른 이름으로 다시 내보낸 trait 메서드, 매크로 안의 조립, cfg 속성 없이 다른 방식으로 시험에만
//! 컴파일되는 코드는 추적하지 않는다.

use tasty_doc_guards::source_text::mask_non_code;

const DOOR: &str = "src/x11_test_loop.rs";

const NAME: &str = "with_x11";

/// 시험이 아닌 곳의 예외 (파일, 시험 항목 밖에서 허용하는 호출 수, 사유).
const EXCEPTIONS: &[(&str, usize, &str)] = &[(
    "src/boot/display_backend.rs",
    1,
    "제품 부팅 경로 apply()가 사용자가 고른 X11 백엔드로 EventLoop 를 만든다 — 시험 도우미의 대상이 아니다",
)];

/// 마스킹한 코드에서 `with_x11` 호출의 바이트 위치. 이름 뒤 공백·줄바꿈을 건너뛴 첫 글자가 `(` 여야 한다.
fn call_positions(masked: &str) -> Vec<usize> {
    super::word_positions(masked, NAME)
        .into_iter()
        .filter(|&pos| {
            masked[pos + NAME.len()..]
                .chars()
                .find(|c| !c.is_whitespace())
                == Some('(')
        })
        .collect()
}

/// 시험 전용 cfg 속성 뒤 첫 블록의 바이트 범위.
fn test_item_spans(masked: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find("#[cfg(") {
        let open = from + rel + 1;
        let Some(close) = super::matching_delim(masked, open) else {
            break;
        };
        let attr = &masked[open..=close];
        let is_test = !super::word_positions(attr, "test").is_empty() && !attr.contains("not(test");
        if is_test && let Some(span) = super::block_after(masked, close + 1) {
            spans.push(span);
        }
        from = close + 1;
    }
    spans
}

/// 도우미 밖의 위반 줄. 예외 파일은 시험 항목 안 호출과, 시험 항목 밖 호출 수가 허용 수와 다른 경우가 위반이다.
fn offenders<'a>(sources: impl Iterator<Item = (String, &'a str)>) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, raw) in sources {
        if rel == DOOR {
            continue;
        }
        let masked = mask_non_code(raw);
        let calls = call_positions(&masked);
        let line = |pos: usize| {
            let n = super::line_of(&masked, pos);
            let text = masked.lines().nth(n - 1).unwrap_or("").trim().to_string();
            format!("  {rel}:{n}  {text}")
        };
        match EXCEPTIONS.iter().find(|(f, _, _)| *f == rel) {
            None => out.extend(calls.into_iter().map(line)),
            Some((_, allowed, _)) => {
                let spans = test_item_spans(&masked);
                let (in_tests, outside): (Vec<usize>, Vec<usize>) = calls
                    .into_iter()
                    .partition(|&p| spans.iter().any(|&(a, b)| a < p && p < b));
                out.extend(in_tests.into_iter().map(line));
                if outside.len() != *allowed {
                    out.push(format!(
                        "  {rel}: 시험 항목 밖 호출 {}개(허용 {allowed}개) — 늘었으면 도우미를 쓰고, 줄었으면 EXCEPTIONS 를 고친다",
                        outside.len()
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn x11_event_loops_in_tests_go_through_the_helper() {
    let sources = super::rust_sources();
    let rels: Vec<String> = sources
        .iter()
        .map(|(p, _)| p.to_string_lossy().replace('\\', "/"))
        .collect();
    let bad = offenders(
        rels.iter()
            .cloned()
            .zip(sources.iter().map(|(_, raw)| raw.as_str())),
    );
    assert!(
        bad.is_empty(),
        "`{DOOR}` 밖에서 X11 EventLoop 를 직접 만드는 곳이 {}개다:\n{}\n시험은 crate::x11_test_loop::x11_event_loop() 를 쓴다. 제품 코드라면 EXCEPTIONS 에 사유를 적는다.",
        bad.len(),
        bad.join("\n")
    );

    let door = rels
        .iter()
        .position(|r| r == DOOR)
        .unwrap_or_else(|| panic!("`{DOOR}` 를 수집하지 못했다 — 옮겼으면 DOOR 를 따라 고친다"));
    assert!(
        !call_positions(&mask_non_code(&sources[door].1)).is_empty(),
        "`{DOOR}` 에 `{NAME}(` 호출이 없다 — 도우미가 바뀌었으면 이 검사를 함께 고친다"
    );
    for (rel, _, reason) in EXCEPTIONS {
        assert!(
            rels.iter().any(|r| r == rel),
            "예외 `{rel}`(\"{reason}\")를 수집하지 못했다. 옮겼으면 명부를 따라 고치고, 필요 없어졌으면 지운다."
        );
    }
}

#[test]
fn direct_builders_are_reported_in_every_call_shape() {
    let planted = "fn event_loop() {\n    EventLoop::builder()\n        .with_x11()\n        .build()\n        .expect(\"x\");\n}\n";
    let spaced = "fn a() { b.with_x11 (); }\n";
    let split = "fn a() {\n    b.with_x11\n        ();\n}\n";
    let commented = "fn a() { b.with_x11 /* why */ (); }\n";
    let not_calls = "// .with_x11() is not called here\nconst S: &str = \".with_x11()\";\nuse x::with_x11_ext;\nfn f() { let with_x11 = 1; }\n";
    let bad = offenders(
        [
            ("src/app/new_tests.rs".to_string(), planted),
            ("src/app/spaced.rs".to_string(), spaced),
            ("src/app/split.rs".to_string(), split),
            ("src/app/commented.rs".to_string(), commented),
            ("src/app/doc_only.rs".to_string(), not_calls),
            (DOOR.to_string(), planted),
        ]
        .into_iter(),
    );
    assert_eq!(
        bad,
        vec![
            "  src/app/new_tests.rs:3  .with_x11()".to_string(),
            "  src/app/spaced.rs:1  fn a() { b.with_x11 (); }".to_string(),
            "  src/app/split.rs:2  b.with_x11".to_string(),
            "  src/app/commented.rs:1  fn a() { b.with_x11           (); }".to_string(),
        ]
    );
}

#[test]
fn the_exception_admits_only_its_product_call() {
    let rel = EXCEPTIONS[0].0.to_string();
    let product = "fn apply(b: &mut B) {\n    b.with_x11();\n}\n";
    let in_test_mod = "fn apply(b: &mut B) {\n    b.with_x11();\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        B::new().with_x11().build().unwrap();\n    }\n}\n";
    let in_nested_test_mod = "mod linux {\n    pub fn apply(b: &mut B) { b.with_x11(); }\n    #[cfg(all(test, target_os = \"linux\"))]\n    mod tests {\n        fn t() { B::new().with_x11(); }\n    }\n}\n";
    let not_test = "fn apply(b: &mut B) { b.with_x11(); }\n#[cfg(not(test))]\nfn other(b: &mut B) { b.with_x11(); }\n";
    let none = "fn apply() {}\n";

    assert!(offenders([(rel.clone(), product)].into_iter()).is_empty());
    assert_eq!(
        offenders([(rel.clone(), in_test_mod)].into_iter()),
        vec![format!("  {rel}:7  B::new().with_x11().build().unwrap();")]
    );
    assert_eq!(
        offenders([(rel.clone(), in_nested_test_mod)].into_iter()),
        vec![format!("  {rel}:5  fn t() {{ B::new().with_x11(); }}")]
    );
    let two = offenders([(rel.clone(), not_test)].into_iter());
    assert_eq!(two.len(), 1, "{two:?}");
    assert!(
        two[0].contains("시험 항목 밖 호출 2개(허용 1개)"),
        "{two:?}"
    );
    let zero = offenders([(rel.clone(), none)].into_iter());
    assert!(
        zero[0].contains("시험 항목 밖 호출 0개(허용 1개)"),
        "{zero:?}"
    );
}
