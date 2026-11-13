//! 호스트의 텍스트 복사는 `src/clipboard/copy.rs` 의 공통 함수 하나로 쓴다. 그 함수가 창의
//! `TerminalClipboard` 로 백엔드를 고르고 결과를 토스트로 알린다. 다른 자리에서 클립보드에 직접
//! 쓰면 실패가 사용자에게 보이지 않거나 성공 토스트와 쓰기 결과가 갈린다.
//!
//! src, crates 의 주석·문자열을 뺀 소스에서 줄 단위로 쓰기 호출 형태를 찾는다. 형태는 상위 API
//! (arboard, `TerminalClipboard`, Wayland 선택 객체, egui-winit 출력 처리, Core 포트)와 그 아래
//! 프로토콜 쓰기(Wayland `set_selection`, X11 `set_selection_owner`, macOS pasteboard, Windows
//! `SetClipboardData`), egui 의 이미지 복사(`OutputCommand::CopyImage`, `copy_image`)다. 허용은
//! 형태마다 파일 안의 구현 함수 이름으로 정한다. 같은 파일이라도 다른 함수에 새로 쓰면 실패한다.
//! 파일 전체를 허용하는 곳은 클립보드가 아닌 같은 이름의 메서드를 쓰는 글꼴 크레이트와 따로 도는
//! 갤러리뿐이다. 이미지 복사는 허용한 곳이 없다. egui 출력에서 꺼내지 않고 egui-winit 이 직접 쓴다.
//!
//! 찾지 못하는 것: 호출을 여러 줄에 나눠 쓴 것(`.set()` 뒤 다음 줄의 `.text(` 는 `.set()` 줄로
//! 찾는다), 수신자 이름에 `clipboard` 가 없는 Core 포트 쓰기, 별칭으로 바꾼 메서드 이름, 허용한
//! 함수와 이름이 같은 새 함수, 허용한 함수 안에 더한 쓰기. WebView 와 markdown 페이지 스크립트의
//! 복사는 Rust 호출이 아니라 이 검사 범위 밖이다(`docs/features/clipboard/index.md` 의 예외 표).

use tasty_doc_guards::source_text::mask_non_code;

/// 쓰기 호출 형태 하나와 그 형태를 쓸 수 있는 자리.
struct Rule {
    what: &'static str,
    calls: &'static [&'static str],
    /// 줄에 이 낱말도 있어야 그 형태로 센다. 이름이 흔한 메서드를 클립보드 쓰기로만 좁힌다.
    receiver: Option<&'static str>,
    allowed: &'static [Place],
}

/// 허용한 자리: 파일 경로(또는 `/` 로 끝나는 폴더)와 그 안의 함수 이름. 이름이 없으면 파일 전체다.
struct Place {
    path: &'static str,
    fns: &'static [&'static str],
}

const fn whole(path: &'static str) -> Place {
    Place { path, fns: &[] }
}

const fn within(path: &'static str, fns: &'static [&'static str]) -> Place {
    Place { path, fns }
}

const DOOR: &str = "src/clipboard/copy.rs";

const RULES: &[Rule] = &[
    Rule {
        what: "TerminalClipboard·arboard 텍스트 쓰기",
        calls: &[".set_text(", ".set_text_primary("],
        receiver: None,
        allowed: &[
            within(DOOR, &["write"]),
            // TerminalClipboard 와 그 arboard 소유자, Core 포트의 arboard 구현, 그 시험.
            within(
                "src/clipboard/terminal.rs",
                &[
                    "set_text",
                    "set_text_primary",
                    "a_window_without_a_wayland_selection_keeps_the_host_clipboard",
                ],
            ),
            within(
                "src/clipboard/context.rs",
                &[
                    "set_text",
                    "set_text_primary",
                    "initialization_failure_survives_every_operation",
                    "the_os_clipboard_round_trips_text",
                ],
            ),
            within("src/adapters/production/arboard_clip.rs", &["write_text"]),
            // 클립보드가 아니다: 글꼴 shaping 버퍼, claude plugin 시험의 화면 대역.
            whole("crates/tasty-font/"),
            within(
                "crates/tasty-plugin-claude/src/error_scan.rs",
                &[
                    "transient_error_that_keeps_producing_output_never_stalls",
                    "an_old_error_followed_by_more_output_uses_the_normal_threshold",
                    "an_error_followed_by_a_short_redraw_still_uses_the_error_threshold",
                    "status_line_redraws_of_an_idle_target_never_fire_a_stall",
                    "new_output_starts_a_new_quiet_stretch_for_the_state_check",
                    "output_resumption_rearms_and_does_not_double_fire",
                    "a_screen_notice_before_a_restart_is_not_repeated_while_the_output_stays",
                    "a_background_wait_is_noticed_once",
                    "an_error_split_across_two_polls_is_still_detected",
                ],
            ),
        ],
    },
    Rule {
        what: "arboard 형식 지정 쓰기와 이미지 쓰기",
        calls: &[".set()", ".set_image("],
        receiver: None,
        allowed: &[
            within("src/clipboard/context.rs", &["set_text_primary"]),
            within("src/clipboard/files/macos.rs", &["write"]),
        ],
    },
    Rule {
        what: "Wayland 선택 쓰기",
        calls: &[".copy_checked(", ".copy_with_png("],
        receiver: None,
        allowed: &[
            within("src/clipboard/terminal.rs", &["set_text"]),
            within(
                "src/adapters/production/arboard_clip.rs",
                &["write_text", "write_text_and_image"],
            ),
            within("src/file/drag.rs", &["copy_with_png"]),
            within("src/file/drag/wayland/clip.rs", &["copy"]),
        ],
    },
    Rule {
        what: "Wayland 프로토콜 선택 지정",
        calls: &["set_selection("],
        receiver: None,
        allowed: &[within("src/file/drag/wayland/clip.rs", &["copy_with_png"])],
    },
    Rule {
        what: "X11 프로토콜 선택 소유",
        calls: &["set_selection_owner("],
        receiver: None,
        allowed: &[
            // 파일·캡처 클립보드의 소유와 해제, INCR 전송 시험의 가짜 소유자.
            within(
                "src/clipboard/files/x11.rs",
                &["own", "clear", "slow_incr_owner"],
            ),
            // 클립보드가 아니다: 끌어서 놓기의 XdndSelection 소유.
            within("src/file/drag/x11.rs", &["start"]),
        ],
    },
    Rule {
        what: "macOS pasteboard 직접 쓰기",
        calls: &["setString_forType(", "setData_forType(", "writeObjects("],
        receiver: None,
        allowed: &[
            within(
                "crates/tasty-platform/src/pasteboard.rs",
                &["write_text_and_png"],
            ),
            within("src/clipboard/files/macos.rs", &["write"]),
        ],
    },
    Rule {
        what: "텍스트와 이미지를 함께 올리는 플랫폼 쓰기",
        calls: &["write_text_and_png(", "SetClipboardData("],
        receiver: None,
        allowed: &[
            within("src/clipboard/files.rs", &["write_text_and_image"]),
            within("src/clipboard/files/windows.rs", &["set"]),
        ],
    },
    Rule {
        what: "egui-winit 출력 처리(복사를 직접 쓴다)",
        calls: &["handle_platform_output("],
        receiver: None,
        allowed: &[
            within(
                "src/gfx/gpu/text_selection.rs",
                &["apply_egui_output", "copy_with_egui_winit"],
            ),
            whole("crates/tasty-gallery/"),
        ],
    },
    Rule {
        what: "egui-winit 클립보드로 복사 넘기기",
        calls: &["copy_with_egui_winit("],
        receiver: None,
        allowed: &[within("src/boot/home_in_use.rs", &["render"])],
    },
    Rule {
        what: "egui 이미지 복사(egui-winit 이 직접 쓴다)",
        calls: &["CopyImage(", ".copy_image("],
        receiver: None,
        allowed: &[],
    },
    Rule {
        what: "Core 클립보드 포트 쓰기",
        calls: &[".write_text(", ".write_text_and_image("],
        receiver: Some("clipboard"),
        allowed: &[
            // OSC 52, IPC clipboard.set_text, 캡처 게시.
            within("src/app/services/impl_pty.rs", &["drain_terminal_events"]),
            within("src/app/services/surface.rs", &["clipboard_set_text"]),
            within("src/ports/clipboard.rs", &["publish_text_with_image"]),
        ],
    },
];

/// 줄에서 `call` 을 쓰는 첫 자리. 앞 글자가 이름의 일부면(`my_handle_platform_output(`) 세지 않고,
/// `fn` 바로 뒤의 정의도 세지 않는다.
fn use_at(line: &str, call: &str) -> Option<usize> {
    line.match_indices(call).map(|(at, _)| at).find(|&at| {
        if call.starts_with('.') {
            return true;
        }
        let before = &line[..at];
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let defines = before
            .trim_end()
            .strip_suffix("fn")
            .is_some_and(|rest| !rest.chars().next_back().is_some_and(ident));
        !defines && !before.chars().next_back().is_some_and(ident)
    })
}

fn rule_use_at(rule: &Rule, line: &str) -> Option<usize> {
    if rule
        .receiver
        .is_some_and(|word| !line.to_ascii_lowercase().contains(word))
    {
        return None;
    }
    rule.calls
        .iter()
        .filter_map(|call| use_at(line, call))
        .min()
}

/// `rel` 파일의 `function` 안(함수 밖이면 `None`)에서 이 형태를 쓸 수 있는가.
fn allowed(rule: &Rule, rel: &str, function: Option<&str>) -> bool {
    rule.allowed.iter().any(|place| {
        let in_path = if place.path.ends_with('/') {
            rel.starts_with(place.path)
        } else {
            rel == place.path
        };
        in_path && (place.fns.is_empty() || function.is_some_and(|f| place.fns.contains(&f)))
    })
}

/// 파일 하나의 허용 밖 쓰기와 공통 함수 안의 쓰기 수.
fn offenders_in(rel: &str, raw: &str) -> (Vec<String>, usize) {
    let masked = mask_non_code(raw);
    let spans = super::fn_spans(&masked);
    let mut offenders = Vec::new();
    let mut door_calls = 0usize;
    let mut start = 0usize;
    for (i, line) in masked.split_inclusive('\n').enumerate() {
        for rule in RULES {
            let Some(at) = rule_use_at(rule, line) else {
                continue;
            };
            if rel == DOOR {
                door_calls += 1;
            }
            let function = super::enclosing_fn(&spans, start + at).map(|span| span.0.as_str());
            if !allowed(rule, rel, function) {
                offenders.push(format!(
                    "  {rel}:{} (fn {})  [{}]  {}",
                    i + 1,
                    function.unwrap_or("-"),
                    rule.what,
                    line.trim()
                ));
            }
        }
        start += line.len();
    }
    (offenders, door_calls)
}

fn rule(what: &str) -> &'static Rule {
    RULES
        .iter()
        .find(|r| r.what == what)
        .expect("rule name is listed")
}

#[test]
fn each_write_shape_is_found_and_other_calls_pass() {
    const TEXT: &str = "TerminalClipboard·arboard 텍스트 쓰기";
    const SET: &str = "arboard 형식 지정 쓰기와 이미지 쓰기";
    const EGUI: &str = "egui-winit 출력 처리(복사를 직접 쓴다)";
    const IMAGE: &str = "egui 이미지 복사(egui-winit 이 직접 쓴다)";
    const PORT: &str = "Core 클립보드 포트 쓰기";
    const WAYLAND: &str = "Wayland 프로토콜 선택 지정";
    const X11: &str = "X11 프로토콜 선택 소유";
    const MAC: &str = "macOS pasteboard 직접 쓰기";
    const PLATFORM: &str = "텍스트와 이미지를 함께 올리는 플랫폼 쓰기";
    let cases = [
        (TEXT, "self.clipboard.set_text(text)", true),
        (TEXT, "clipboard.set_text_primary(text)", true),
        (TEXT, "cb.set_text(text)", true),
        (TEXT, "self.copy_text(&text, false)", false),
        (SET, "clipboard()?.set().text(t)", true),
        (SET, "            .set()", true),
        (SET, "cb.set_image(image)", true),
        (SET, "flag.set(true)", false),
        (
            EGUI,
            "self.egui_state.handle_platform_output(window, output);",
            true,
        ),
        (EGUI, "my_handle_platform_output(x)", false),
        (
            IMAGE,
            "o.commands.push(egui::OutputCommand::CopyImage(img));",
            true,
        ),
        (IMAGE, "ui.ctx().copy_image(img);", true),
        (PORT, "core.clipboard_arc().write_text(text)", true),
        (
            PORT,
            "self.clipboard.write_text_and_image(text, image)",
            true,
        ),
        // PTY 에 글자를 보내는 같은 이름의 메서드는 클립보드 쓰기가 아니다.
        (PORT, "self.write_text(surface_id, &text)", false),
        (
            WAYLAND,
            "device.set_selection(Some(&source), serial);",
            true,
        ),
        (WAYLAND, "conn.set_selection_owner(w, a, t)", false),
        (X11, "conn.set_selection_owner(w, a, t)?", true),
        (MAC, "pb.writeObjects(&objects)", true),
        (MAC, "pb.setData_forType(Some(&d), t)", true),
        (PLATFORM, "x11::write_text_and_png(text, png)", true),
        (PLATFORM, "unsafe { SetClipboardData(format, h) }", true),
        // 정의는 쓰기가 아니다.
        (
            PLATFORM,
            "pub(crate) fn write_text_and_png(text: &str) {",
            false,
        ),
    ];
    for (what, line, expected) in cases {
        assert_eq!(
            rule_use_at(rule(what), line).is_some(),
            expected,
            "{what}: {line}"
        );
    }
}

#[test]
fn an_allowed_prefix_covers_only_its_folder() {
    let gallery = rule("egui-winit 출력 처리(복사를 직접 쓴다)");
    assert!(allowed(gallery, "crates/tasty-gallery/src/main.rs", None));
    assert!(!allowed(
        gallery,
        "crates/tasty-gallery-x/src/main.rs",
        None
    ));
    let wayland = rule("Wayland 선택 쓰기");
    assert!(allowed(wayland, "src/file/drag.rs", Some("copy_with_png")));
    assert!(!allowed(
        wayland,
        "src/file/dragged.rs",
        Some("copy_with_png")
    ));
}

/// Given 허용한 파일, When 허용한 구현 함수 밖의 새 함수가 직접 쓰면, Then 그 함수만 걸린다.
#[test]
fn a_write_in_another_function_of_an_allowed_file_is_found() {
    let cases = [
        (
            "src/clipboard/terminal.rs",
            "fn set_text(&mut self) { self.host.set_text(t) }\n\
             fn copy_now(&mut self) { self.host.set_text(t) }\n",
        ),
        (
            "src/clipboard/context.rs",
            "fn set_text(&mut self) { cb.set_text(t) }\n\
             fn copy_now(&mut self) { cb.set_text(t) }\n",
        ),
        (
            "src/adapters/production/arboard_clip.rs",
            "fn write_text(&self) { cb.set_text(t) }\n\
             fn copy_now(&self) { cb.set_text(t) }\n",
        ),
        (
            "src/gfx/gpu/text_selection.rs",
            "fn apply_egui_output(&mut self) { self.egui_state.handle_platform_output(w, o); }\n\
             fn copy_now(&mut self) { self.egui_state.handle_platform_output(w, o); }\n",
        ),
        (
            "crates/tasty-plugin-claude/src/error_scan.rs",
            "fn a_background_wait_is_noticed_once() { host.set_text(t); }\n\
             fn copy_now() { cb.set_text(t); }\n",
        ),
        (
            "src/file/drag/wayland/clip.rs",
            "fn copy_with_png(&mut self) { device.set_selection(Some(&s), serial); }\n\
             fn copy_now(&mut self) { device.set_selection(Some(&s), serial); }\n",
        ),
        (
            "src/clipboard/files/x11.rs",
            "fn own(s: Session) { conn.set_selection_owner(w, a, t); }\n\
             fn copy_now() { conn.set_selection_owner(w, a, t); }\n",
        ),
        (
            "crates/tasty-platform/src/pasteboard.rs",
            "fn write_text_and_png() { pb.setData_forType(d, t); }\n\
             fn copy_now() { pb.writeObjects(&o); }\n",
        ),
    ];
    for (rel, src) in cases {
        let (offenders, _) = offenders_in(rel, src);
        assert_eq!(offenders.len(), 1, "{rel}: {offenders:?}");
        assert!(offenders[0].contains("fn copy_now"), "{rel}: {offenders:?}");
    }
}

/// Given 허용 목록 밖의 파일, When 프로토콜 쓰기나 이미지 복사를 하면, Then 모두 걸린다.
#[test]
fn protocol_writes_and_image_copies_elsewhere_are_found() {
    let src = "fn f(ctx: &egui::Context) {\n\
               ctx.output_mut(|o| o.commands.push(egui::OutputCommand::CopyImage(img)));\n\
               ctx.copy_image(img);\n\
               device.set_selection(Some(&s), serial);\n\
               conn.set_selection_owner(w, a, t);\n\
               pb.writeObjects(&o);\n\
               }\n";
    for rel in [
        "src/view/main/clipboard.rs",
        "src/file/drag/wayland/owner.rs",
        "src/clipboard/files/other.rs",
    ] {
        let (offenders, _) = offenders_in(rel, src);
        assert_eq!(offenders.len(), 5, "{rel}: {offenders:?}");
    }
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
        let (found, calls) = offenders_in(&rel, raw);
        offenders.extend(found);
        door_calls += calls;
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

/// 본문의 `handle_platform_output` 호출마다 그 앞(직전 호출 뒤)에 `hold_copied_text` 가 있는가.
/// 호출이 하나도 없으면 거짓이다.
fn hands_over_after_holding(body: &str) -> bool {
    let mut calls: Vec<(usize, bool)> = body
        .match_indices("hold_copied_text(")
        .map(|(at, _)| (at, true))
        .chain(
            body.match_indices("handle_platform_output(")
                .map(|(at, _)| (at, false)),
        )
        .collect();
    calls.sort_unstable();
    let mut held = false;
    let mut handed = false;
    for (_, hold) in calls {
        if hold {
            held = true;
        } else if held {
            held = false;
            handed = true;
        } else {
            return false;
        }
    }
    handed
}

#[test]
fn a_hand_over_without_holding_is_found() {
    let hold = "let output = hold_copied_text(&mut self.egui_copy, output);\n";
    let hand = "self.egui_state.handle_platform_output(window, output);\n";
    assert!(hands_over_after_holding(&format!("{hold}{hand}")));
    assert!(!hands_over_after_holding(hand));
    assert!(!hands_over_after_holding(&format!("{hand}{hold}")));
    assert!(!hands_over_after_holding(hold));
    // 정상 전달 뒤에 꺼내지 않은 출력을 한 번 더 넘긴다.
    assert!(!hands_over_after_holding(&format!("{hold}{hand}{hand}")));
}
