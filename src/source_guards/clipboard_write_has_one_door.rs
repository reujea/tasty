//! 호스트의 텍스트 복사는 `src/clipboard/copy.rs` 의 공통 함수 하나로 쓴다. 그 함수가 창의
//! `TerminalClipboard` 로 백엔드를 고르고 결과를 토스트로 알린다. 다른 자리에서 클립보드에 직접
//! 쓰면 실패가 사용자에게 보이지 않거나 성공 토스트와 쓰기 결과가 갈린다.
//!
//! src, crates 의 주석·문자열을 뺀 소스에서 줄 단위로 쓰기 호출 형태를 찾고, 형태마다 허용한 파일
//! 밖의 호출을 실패로 본다. 허용 파일은 공통 함수, 그 함수가 부르는 구현, 문서에 적은 예외(OSC 52,
//! IPC, 캡처 게시, 파일 클립보드, 갤러리)다. 호출을 여러 줄에 나눠 쓰거나(`.set()` 뒤 다음 줄의
//! `.text(`는 `.set()` 줄로 찾는다) 수신자 이름에 `clipboard` 가 없는 Core 포트 쓰기는 찾지 못한다.
//! 별칭으로 바꾼 메서드 이름도 찾지 못한다. WebView 와 markdown 페이지 스크립트의 복사는 Rust
//! 호출이 아니라 이 검사 범위 밖이다(`docs/features/clipboard/index.md` 의 예외 표).

use tasty_doc_guards::source_text::mask_non_code;

/// 쓰기 호출 형태 하나와 그 형태를 쓸 수 있는 파일(경로 또는 `/` 로 끝나는 접두어).
struct Rule {
    what: &'static str,
    calls: &'static [&'static str],
    /// 줄에 이 낱말도 있어야 그 형태로 센다. 이름이 흔한 메서드를 클립보드 쓰기로만 좁힌다.
    receiver: Option<&'static str>,
    allowed: &'static [&'static str],
}

const DOOR: &str = "src/clipboard/copy.rs";

const RULES: &[Rule] = &[
    Rule {
        what: "TerminalClipboard·arboard 텍스트 쓰기",
        calls: &[".set_text(", ".set_text_primary("],
        receiver: None,
        allowed: &[
            DOOR,
            // TerminalClipboard 와 그 arboard 소유자, Core 포트의 arboard 구현.
            "src/clipboard/terminal.rs",
            "src/clipboard/context.rs",
            "src/adapters/production/arboard_clip.rs",
            // 클립보드가 아니다: 글꼴 shaping 버퍼, claude plugin 시험의 화면 대역.
            "crates/tasty-font/",
            "crates/tasty-plugin-claude/src/error_scan.rs",
        ],
    },
    Rule {
        what: "arboard 형식 지정 쓰기와 이미지 쓰기",
        calls: &[".set()", ".set_image("],
        receiver: None,
        allowed: &["src/clipboard/context.rs", "src/clipboard/files/macos.rs"],
    },
    Rule {
        what: "Wayland 선택 쓰기",
        calls: &[".copy_checked(", ".copy_with_png("],
        receiver: None,
        allowed: &[
            "src/clipboard/terminal.rs",
            "src/adapters/production/arboard_clip.rs",
            "src/file/drag.rs",
            "src/file/drag/",
        ],
    },
    Rule {
        what: "egui-winit 출력 처리(복사를 직접 쓴다)",
        calls: &["handle_platform_output("],
        receiver: None,
        allowed: &["src/gfx/gpu/text_selection.rs", "crates/tasty-gallery/"],
    },
    Rule {
        what: "egui-winit 클립보드로 복사 넘기기",
        calls: &["copy_with_egui_winit("],
        receiver: None,
        allowed: &["src/gfx/gpu/text_selection.rs", "src/boot/home_in_use.rs"],
    },
    Rule {
        what: "Core 클립보드 포트 쓰기",
        calls: &[".write_text(", ".write_text_and_image("],
        receiver: Some("clipboard"),
        allowed: &[
            // OSC 52, IPC clipboard.set_text, 캡처 게시와 포트 자신.
            "src/app/services/impl_pty.rs",
            "src/app/services/surface.rs",
            "src/ports/clipboard.rs",
        ],
    },
    Rule {
        what: "텍스트와 이미지를 함께 올리는 플랫폼 쓰기",
        calls: &[
            "write_text_and_png(",
            "setString_forType(",
            "SetClipboardData(",
        ],
        receiver: None,
        allowed: &[
            "src/clipboard/files.rs",
            "src/clipboard/files/",
            "crates/tasty-platform/src/pasteboard.rs",
        ],
    },
];

/// 줄이 `call` 을 쓰는가. 앞 글자가 이름의 일부면(`my_handle_platform_output(`) 세지 않는다.
fn uses(line: &str, call: &str) -> bool {
    line.match_indices(call).any(|(at, _)| {
        call.starts_with('.')
            || !line[..at]
                .chars()
                .next_back()
                .is_some_and(|before| before.is_alphanumeric() || before == '_')
    })
}

fn matches_rule(rule: &Rule, line: &str) -> bool {
    rule.calls.iter().any(|call| uses(line, call))
        && rule
            .receiver
            .is_none_or(|word| line.to_ascii_lowercase().contains(word))
}

fn allowed(rule: &Rule, rel: &str) -> bool {
    rule.allowed.iter().any(|path| {
        if path.ends_with('/') {
            rel.starts_with(path)
        } else {
            rel == *path
        }
    })
}

fn rule(what: &str) -> &'static Rule {
    RULES
        .iter()
        .find(|r| r.what == what)
        .expect("rule name is listed")
}

#[test]
fn each_write_shape_is_found_and_other_calls_pass() {
    let text = rule("TerminalClipboard·arboard 텍스트 쓰기");
    for line in [
        "self.clipboard.set_text(text)",
        "clipboard.set_text_primary(text)",
        "cb.set_text(text)",
    ] {
        assert!(matches_rule(text, line), "{line}");
    }
    assert!(!matches_rule(text, "self.copy_text(&text, false)"));
    let set = rule("arboard 형식 지정 쓰기와 이미지 쓰기");
    assert!(matches_rule(set, "clipboard()?.set().text(t)"));
    assert!(matches_rule(set, "            .set()"));
    assert!(matches_rule(set, "cb.set_image(image)"));
    assert!(!matches_rule(set, "flag.set(true)"));
    let egui = rule("egui-winit 출력 처리(복사를 직접 쓴다)");
    assert!(matches_rule(
        egui,
        "self.egui_state.handle_platform_output(window, output);"
    ));
    assert!(!matches_rule(egui, "my_handle_platform_output(x)"));
    let port = rule("Core 클립보드 포트 쓰기");
    assert!(matches_rule(port, "core.clipboard_arc().write_text(text)"));
    assert!(matches_rule(
        port,
        "self.clipboard.write_text_and_image(text, image)"
    ));
    // PTY 에 글자를 보내는 같은 이름의 메서드는 클립보드 쓰기가 아니다.
    assert!(!matches_rule(port, "self.write_text(surface_id, &text)"));
    let platform = rule("텍스트와 이미지를 함께 올리는 플랫폼 쓰기");
    assert!(matches_rule(platform, "x11::write_text_and_png(text, png)"));
    assert!(matches_rule(
        platform,
        "unsafe { SetClipboardData(format, h) }"
    ));
}

#[test]
fn an_allowed_prefix_covers_only_its_folder() {
    let wayland = rule("Wayland 선택 쓰기");
    assert!(allowed(wayland, "src/file/drag/wayland/clip.rs"));
    assert!(allowed(wayland, "src/file/drag.rs"));
    assert!(!allowed(wayland, "src/file/dragged.rs"));
    assert!(!allowed(wayland, "src/gfx/gpu/text_selection.rs"));
}

#[test]
fn clipboard_text_is_only_written_through_its_one_door() {
    let mut door_calls = 0usize;
    let mut offenders = Vec::new();
    for (rel_path, raw) in &super::rust_sources() {
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        if rel == "src/source_guards/clipboard_write_has_one_door.rs" {
            continue;
        }
        for (i, line) in mask_non_code(raw).lines().enumerate() {
            for rule in RULES {
                if !matches_rule(rule, line) {
                    continue;
                }
                if rel == DOOR {
                    door_calls += 1;
                }
                if !allowed(rule, &rel) {
                    offenders.push(format!(
                        "  {rel}:{}  [{}]  {}",
                        i + 1,
                        rule.what,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "공통 복사 함수 밖에서 클립보드에 쓰는 곳이 {}개다:\n{}\n텍스트 복사는 `crate::clipboard::copy::copy_text`(토스트가 없는 화면은 `copy_text_unannounced`)로 쓴다. 새 예외가 필요하면 docs/features/clipboard/index.md 의 예외 표와 이 검사의 허용 목록을 함께 고친다.",
        offenders.len(),
        offenders.join("\n")
    );
    assert!(
        door_calls > 0,
        "`{DOOR}` 에서 쓰기 호출을 찾지 못했다. 파일을 옮겼으면 이 검사를 따라 고친다."
    );
}

/// egui 출력은 복사를 꺼낸 뒤에만 egui-winit 에 넘긴다. 꺼내기를 빼면 egui-winit 이 다시 직접 쓴다.
#[test]
fn egui_output_reaches_egui_winit_only_after_its_copies_are_taken() {
    let src = std::fs::read_to_string(super::repo_root().join("src/gfx/gpu/text_selection.rs"))
        .expect("text_selection.rs is readable");
    let body =
        super::fn_body(&src, "fn apply_egui_output(").expect("apply_egui_output is defined once");
    let body = mask_non_code(&body);
    assert!(
        hands_over_after_holding(&body),
        "apply_egui_output 가 hold_copied_text 를 거치지 않고 egui-winit 에 출력을 넘긴다:\n{body}"
    );
}

/// 본문이 `hold_copied_text` 를 먼저 부르고 그 뒤에만 `handle_platform_output` 을 부르는가.
fn hands_over_after_holding(body: &str) -> bool {
    match (
        body.find("hold_copied_text("),
        body.find("handle_platform_output("),
    ) {
        (Some(hold), Some(hand)) => hold < hand,
        _ => false,
    }
}

#[test]
fn a_hand_over_without_holding_is_found() {
    assert!(hands_over_after_holding(
        "let output = hold_copied_text(&mut self.egui_copy, output);\n\
         self.egui_state.handle_platform_output(window, output);"
    ));
    assert!(!hands_over_after_holding(
        "self.egui_state.handle_platform_output(window, output);"
    ));
    assert!(!hands_over_after_holding(
        "self.egui_state.handle_platform_output(window, output);\n\
         let output = hold_copied_text(&mut self.egui_copy, output);"
    ));
}
