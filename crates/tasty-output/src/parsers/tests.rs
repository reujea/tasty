//! `parsers` 단위 테스트.

use crate::{ParsedItem, parse_buffer};

fn first<'a>(items: &'a [ParsedItem], kind: &str) -> &'a ParsedItem {
    items.iter().find(|i| i.kind == kind).expect("no item")
}

// ----- path -----

#[test]
fn path_basic_with_line_col() {
    let items = parse_buffer("error at src/main.rs:42:7 ok", ["path"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["path"], "src/main.rs");
    assert_eq!(items[0].data["line"], 42);
    assert_eq!(items[0].data["col"], 7);
}

#[test]
fn path_line_only() {
    let items = parse_buffer("see src/main.rs:42 ok", ["path"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["path"], "src/main.rs");
    assert_eq!(items[0].data["line"], 42);
    assert!(items[0].data["col"].is_null());
}

#[test]
fn path_no_suffix() {
    let items = parse_buffer("touch README.md ok", ["path"]).unwrap();
    let p = first(&items, "path");
    assert_eq!(p.data["path"], "README.md");
    assert!(p.data["line"].is_null());
}

#[test]
fn path_skips_short_or_non_path() {
    let items = parse_buffer("hello world test", ["path"]).unwrap();
    // "hello"/"world"/"test" 는 separator/확장자 없음 → skip.
    assert!(
        items.is_empty(),
        "expected no paths, got: {:?}",
        items.iter().map(|i| &i.data).collect::<Vec<_>>()
    );
}

// ----- url -----

#[test]
fn url_https() {
    let items = parse_buffer("see https://example.com/path?x=1", ["url"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["url"], "https://example.com/path?x=1");
    assert_eq!(items[0].data["scheme"], "https");
}

#[test]
fn url_strips_trailing_punct() {
    let items = parse_buffer("visit https://example.com.", ["url"]).unwrap();
    assert_eq!(items[0].data["url"], "https://example.com");
}

#[test]
fn url_ssh_scheme() {
    let items = parse_buffer("clone ssh://git@host/repo.git", ["url"]).unwrap();
    assert_eq!(items[0].data["scheme"], "ssh");
}

// ----- prompt_boundary -----

#[test]
fn prompt_boundary_a_b_c_d() {
    let text = "\x1b]133;A\x07prompt\x1b]133;B\x07cmd\x1b]133;C\x07out\x1b]133;D;0\x07";
    let items = parse_buffer(text, ["prompt_boundary"]).unwrap();
    let phases: Vec<&str> = items
        .iter()
        .map(|i| i.data["phase"].as_str().unwrap())
        .collect();
    assert_eq!(phases, ["A", "B", "C", "D"]);
}

#[test]
fn prompt_boundary_st_terminator() {
    let text = "\x1b]133;A\x1b\\";
    let items = parse_buffer(text, ["prompt_boundary"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["phase"], "A");
}

// ----- exit_code -----

#[test]
fn exit_code_zero() {
    let items = parse_buffer("\x1b]133;D;0\x07", ["exit_code"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["code"], 0);
}

#[test]
fn exit_code_nonzero_with_extra_token() {
    let items = parse_buffer("\x1b]133;D;127;some-extra\x07", ["exit_code"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["code"], 127);
}

// ----- ordering & multi-parser -----

#[test]
fn multiple_parsers_in_single_buffer() {
    let text = "error in src/main.rs:42:7\nsee https://example.com/docs\n";
    let items = parse_buffer(text, ["path", "url"]).unwrap();
    assert!(items.iter().any(|i| i.kind == "path"));
    assert!(items.iter().any(|i| i.kind == "url"));
}

#[test]
fn line_numbers_reflect_input_order() {
    let text = "a\nb\nsrc/main.rs\n";
    let items = parse_buffer(text, ["path"]).unwrap();
    let p = first(&items, "path");
    assert_eq!(p.line, 2);
}

// ----- osc_link -----

#[test]
fn osc_link_basic() {
    let text = "click \x1b]8;;https://example.com\x07here\x1b]8;;\x07 ok";
    let items = parse_buffer(text, ["osc_link"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["url"], "https://example.com");
    assert_eq!(items[0].data["text"], "here");
}

#[test]
fn osc_link_st_terminator() {
    let text = "\x1b]8;;file:///tmp/x\x1b\\label\x1b]8;;\x1b\\";
    let items = parse_buffer(text, ["osc_link"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["url"], "file:///tmp/x");
    assert_eq!(items[0].data["text"], "label");
}

// ----- osc_notification -----

#[test]
fn osc_notification_9() {
    let items = parse_buffer("\x1b]9;Build finished\x07", ["osc_notification"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["osc"], 9);
    assert_eq!(items[0].data["message"], "Build finished");
}

#[test]
fn osc_notification_777_with_title() {
    let items = parse_buffer("\x1b]777;notify;Compile;done\x07", ["osc_notification"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["osc"], 777);
    assert_eq!(items[0].data["action"], "notify");
    assert_eq!(items[0].data["title"], "Compile");
    assert_eq!(items[0].data["message"], "done");
}

// ----- progress -----

#[test]
fn progress_bar_with_percent() {
    let items = parse_buffer("[====>     ] 45%", ["progress"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["style"], "bar");
    assert_eq!(items[0].data["percent"], 45.0);
}

#[test]
fn progress_size_with_total() {
    let items = parse_buffer("Downloading 50 MB / 200 MB", ["progress"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["style"], "size");
    assert_eq!(items[0].data["current"], 50.0);
    assert_eq!(items[0].data["total"], 200.0);
    assert_eq!(items[0].data["percent"], 25.0);
}

#[test]
fn progress_plain_percent() {
    let items = parse_buffer("Progress: 78%", ["progress"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["style"], "percent");
    assert_eq!(items[0].data["percent"], 78.0);
}

#[test]
fn progress_ignores_percent_in_prose() {
    // 백분율 뒤에 긴 토큰이 있으면 progress 가 아님.
    let items = parse_buffer(
        "We saw 50% improvement in test coverage and many bugs fixed.",
        ["progress"],
    )
    .unwrap();
    assert!(items.is_empty(), "got: {items:?}");
}

// ----- compile_error -----

#[test]
fn compile_error_rustc_with_location() {
    let text = "error[E0308]: mismatched types\n --> src/main.rs:10:5\n";
    let items = parse_buffer(text, ["compile_error"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["tool"], "rustc");
    assert_eq!(items[0].data["severity"], "error");
    assert_eq!(items[0].data["code"], "E0308");
    assert_eq!(items[0].data["path"], "src/main.rs");
    assert_eq!(items[0].data["line_number"], 10);
    assert_eq!(items[0].data["column"], 5);
}

#[test]
fn compile_error_gcc() {
    let items = parse_buffer("src/foo.c:42:10: error: 'x' undeclared", ["compile_error"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["tool"], "gcc");
    assert_eq!(items[0].data["path"], "src/foo.c");
    assert_eq!(items[0].data["line_number"], 42);
    assert_eq!(items[0].data["column"], 10);
    assert_eq!(items[0].data["message"], "'x' undeclared");
}

#[test]
fn compile_error_tsc() {
    let items = parse_buffer(
        "src/foo.ts(10,5): error TS2345: Type 'string' is not assignable",
        ["compile_error"],
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["tool"], "tsc");
    assert_eq!(items[0].data["code"], "TS2345");
    assert_eq!(items[0].data["line_number"], 10);
    assert_eq!(items[0].data["column"], 5);
}

// ----- stack_trace -----

#[test]
fn stack_trace_python() {
    let text = "\
Traceback (most recent call last):
  File \"app.py\", line 10, in main
    raise ValueError('bad')
ValueError: bad
";
    let items = parse_buffer(text, ["stack_trace"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["language"], "python");
    assert_eq!(items[0].data["exception"], "ValueError");
    assert_eq!(items[0].data["message"], "bad");
    let frames = items[0].data["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["path"], "app.py");
    assert_eq!(frames[0]["line"], 10);
    assert_eq!(frames[0]["func"], "main");
}

#[test]
fn stack_trace_rust_panic() {
    let text = "thread 'main' panicked at 'bad', src/main.rs:42:7\n";
    let items = parse_buffer(text, ["stack_trace"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["language"], "rust");
    assert_eq!(items[0].data["thread"], "main");
    assert_eq!(items[0].data["message"], "bad");
    assert_eq!(items[0].data["path"], "src/main.rs");
    assert_eq!(items[0].data["line_number"], 42);
}

#[test]
fn stack_trace_node() {
    let text = "\
    at Object.<anonymous> (/app/index.js:10:13)
    at Module._compile (node:internal/modules/cjs/loader:1234:14)
";
    let items = parse_buffer(text, ["stack_trace"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["language"], "node");
    let frames = items[0].data["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
}

#[test]
fn stack_trace_java() {
    let text = "\
    at com.example.Foo.bar(Foo.java:42)
    at com.example.Main.main(Main.java:10)
";
    let items = parse_buffer(text, ["stack_trace"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["language"], "java");
    let frames = items[0].data["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["func"], "com.example.Foo.bar");
    assert_eq!(frames[0]["line"], 42);
}

// ----- test_result -----

#[test]
fn test_result_cargo() {
    let line = "test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.12s";
    let items = parse_buffer(line, ["test_result"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["framework"], "cargo");
    assert_eq!(items[0].data["status"], "passed");
    assert_eq!(items[0].data["passed"], 5);
    assert_eq!(items[0].data["failed"], 0);
    assert_eq!(items[0].data["ignored"], 1);
    assert_eq!(items[0].data["duration_seconds"], 0.12);
}

#[test]
fn test_result_cargo_failed() {
    let line = "test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out";
    let items = parse_buffer(line, ["test_result"]).unwrap();
    assert_eq!(items[0].data["status"], "failed");
}

#[test]
fn test_result_pytest() {
    let line = "============== 5 passed, 1 failed, 2 skipped in 0.34s ==============";
    let items = parse_buffer(line, ["test_result"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["framework"], "pytest");
    assert_eq!(items[0].data["status"], "failed");
    assert_eq!(items[0].data["counts"]["passed"], 5);
    assert_eq!(items[0].data["counts"]["failed"], 1);
    assert_eq!(items[0].data["counts"]["skipped"], 2);
    assert_eq!(items[0].data["duration_seconds"], 0.34);
}

#[test]
fn test_result_jest() {
    let line = "Tests:       1 failed, 2 passed, 3 total";
    let items = parse_buffer(line, ["test_result"]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data["framework"], "jest");
    assert_eq!(items[0].data["status"], "failed");
    assert_eq!(items[0].data["counts"]["passed"], 2);
    assert_eq!(items[0].data["counts"]["failed"], 1);
    assert_eq!(items[0].data["counts"]["total"], 3);
}
