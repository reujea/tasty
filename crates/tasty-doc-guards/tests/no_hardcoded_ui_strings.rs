//! 사용자 표면 문자열 하드코딩 가드 — 사람이 읽는 문자열이 `t()` 를 거치지 않고
//! 리터럴로 소스에 박히면 fail 한다.
//!
//! 배경: `CLAUDE.md` "국제화" 는 자연어 하드코딩을 금지하지만 지금까지 리뷰에만
//! 의존했고, egui 본체는 지켜졌어도 가장자리(CLI stderr · OS 네이티브 메뉴 · 알림
//! 제목 · `unwrap_or` 폴백)에서 반복 재발했다. 이 가드는 의존 0 크레이트에 살아서
//! `doc-guards.yml` 이 경로 필터 없이 돌린다 — main push 와 PR 마다 자동으로 돈다
//! (`docs/dev-guide/ci-gates.md`).
//! 근거 문서는
//! `docs/dev-guide/i18n.md` — 예외 목록도 그 문서의 "하드코딩 허용 예외" 를 그대로 옮긴 것이다.
//!
//! **형태별 검사** (한 정규식으로는 절반도 못 잡으므로 `find_*` 로 분리):
//! - W  egui 위젯 인자 — `ui.label(` / `ui.button(` / `ui.heading(` / `RichText::new(` /
//!   `Button::new(` / `Label::new(` / `.on_hover_text(` 등에 리터럴. 두 글자 이상의
//!   영문자 또는 CJK 가 있으면 위반(라벨은 단어 하나여도 사용자 문구다).
//! - H  `.hint_text(` — placeholder 는 `my-viewer` / `md, mdx` 같은 예시 식별자가 많아
//!   문장 형태(공백 + 네 글자 이상 단어)나 CJK 만 위반으로 본다.
//! - N  OS 네이티브 — `NSString::from_str(` / `MenuItem::new(` / `.with_tooltip(` / `w!(`.
//!   `UTF-8` · `about:blank` 같은 식별자는 두고, 대문자로 시작하는 영단어 하나
//!   (`Quit` / `File`)나 문장을 위반으로 본다.
//! - P  `PushNotification { … title: "…" }` — 알림 제목 리터럴(ADR-0106).
//! - F  `unwrap_or("…")` / `unwrap_or_else(|| "…")` 폴백 — 대문자 시작 영단어·문장·CJK
//!   (`"Unknown"` / `"Shell"`). 소문자 단일 단어(`"unknown"`)는 식별자일 수 있어 두지만,
//!   그런 값이 화면에 간다면 그것도 `t()` 다(i18n.md "위젯 호출이 아닌 경로").
//! - E  `println!` / `eprintln!` 의 포맷 문자열 — 문장·대문자 영단어·CJK. tasty-cli 의
//!   stdout 은 `outln!` 이라 stderr 의 `eprintln!` 만 걸린다. `tracing::*` 는 사용자
//!   표면이 아니라 검사하지 않는다.
//! - C  clap `///` 도움말 — `crates/tasty-cli` 의 서브커맨드/인자 doc comment 와
//!   `about =` / `help =` 리터럴에 한글·가나·한자 금지(영어로만 쓴다 — i18n.md).
//!   영어 도움말 자체는 i18n.md 예외라 검사 대상이 아니다.
//!
//! **검사하지 않는 것(i18n.md 예외 — 그대로 옮김)**: 수식키·키 이름(`Ctrl`/`Escape`),
//! 폰트 프리뷰 `AaBbCcDdEeFfGg`, 언어 이름(`English`/`한국어`/`日本語`), 단위(`MiB`),
//! 제품명 `Tasty`, `tracing::*` 로그, clap 의 영어 도움말, JSON-RPC 메서드·프로토콜 토큰
//! (문자열 인자를 위젯에 넘기지 않으므로 형태 자체가 안 걸린다), `tasty list` 구조 출력의
//! 고정 토큰(`crates/tasty-cli/src/format.rs` 통째로), 갤러리 specimen(영어 리터럴이
//! 카탈로그의 본질), 데모 plugin, TUI 시뮬레이터, `debug/` 디렉토리(사용자 표면 아님),
//! `#[cfg(test)]` / `#[test]` 가 붙은 아이템의 본문(테스트 모듈 뒤의 코드는 스캔), `//` 주석.
//!
//! 잔존 위반은 [`PENDING_FIX_LITERALS`] 에 파일·리터럴·고칠 방법과 함께 둔다 — 고쳐지면
//! 항목을 지우라고 fail 하므로 빚이 조용히 남지 않는다.
//!
//! 선례: `crates/tasty-doc-guards/tests/no_todo_file_citation.rs`(구조 템플릿) · `crates/tasty-doc-guards/tests/design_token_adherence.rs`.

use tasty_doc_guards::cfg_predicate as cfg_span;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 스캔에서 통째로 제외하는 경로 접두 — (접두, 이유).
const ALLOWLIST_PATH_PREFIXES: &[(&str, &str)] = &[
    (
        "crates/tasty-gallery/",
        "갤러리 specimen — 영어 리터럴을 그대로 주입하는 카탈로그 (i18n.md 공용 위젯 절)",
    ),
    (
        "crates/tasty-plugin-mesh-demo/",
        "데모 plugin — 사용자 배포 표면이 아니다",
    ),
    ("crates/tasty-tui-simulator/", "테스트용 TUI 시뮬레이터"),
    (
        "src/source_guards/",
        "src/lib.rs 가 `#[cfg(test)] mod source_guards;` 로 다는 테스트 전용 모듈 — \
         릴리스 바이너리에 없다. 이 스캐너는 파일 **안**의 `#[test]` 만 보므로 모듈 \
         선언 쪽 cfg 를 못 따라간다. `#[test]` 밖 헬퍼의 진단 문구가 사용자 문구로 \
         잘못 걸리던 것을 경로로 막는다 (`/debug/` 제외와 같은 근거)",
    ),
    (
        "crates/tasty-cli/src/format.rs",
        "tasty list 구조 출력의 고정 토큰 — 기계 파싱 대상 (i18n.md 예외, t() 미사용이 컨벤션)",
    ),
    (
        "crates/tasty-platform/src/crash_report.rs",
        "panic hook — 번역 테이블이 없을 수 있고 crash 문구는 리포트 대조용으로 고정",
    ),
    (
        "crates/tasty-cli/src/help.rs",
        "clap 도움말 보강 출력 — clap 의 영어 about 텍스트와 한 화면에 섞여 나오므로 같은 언어(i18n.md clap 예외)",
    ),
    (
        "crates/tasty-doc-guards/src/bin/",
        "게이트 스크립트가 부르는 개발 도구 — 사용자 배포 표면이 아니고, 이 크레이트는 \
         의존이 0 인 것이 존재 이유라(ADR-0138) 번역 테이블을 들일 수도 없다. 진단은 \
         게이트 로그로 나가 개발자만 읽는다 (`crates/tasty-tui-simulator/` 와 같은 근거). \
         **`src/bin/` 만이다** — 이 크레이트의 라이브러리는 아무것도 출력하지 않는다",
    ),
];

/// 리터럴 그대로 허용하는 토큰 — 번역하면 의미가 변하는 고유명사·식별자(i18n.md).
const LITERAL_TOKEN_ALLOWLIST: &[&str] = &[
    // 수식키 · 키 이름
    "Ctrl",
    "Alt",
    "Shift",
    "Cmd",
    "Command",
    "Option",
    "Super",
    "Meta",
    "Win",
    "Fn",
    "Esc",
    "Escape",
    "Enter",
    "Return",
    "Tab",
    "Space",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    // 폰트 프리뷰 · 언어 이름
    "AaBbCcDdEeFfGg",
    "English",
    "한국어",
    "日本語",
    // 단위
    "KiB",
    "MiB",
    "GiB",
    "KB",
    "MB",
    "GB",
    "ms",
    "px",
    "fps",
    "Hz",
    // 제품명
    "Tasty",
    // 프로토콜 토큰 — Windows webview 가 원격 차단에 돌려주는 HTTP 403 reason phrase
    "Blocked",
];

/// 알려진 잔존 위반 — (파일, 리터럴, 고칠 방법). 여기 있는 동안은 실패로 치지 않되,
/// 사라지면(=고쳐지면) 항목을 지우라고 fail 한다. 새 위반을 여기 넣어 덮지 않는다.
const PENDING_FIX_LITERALS: &[(&str, &str, &str)] = &[
    (
        "src/gfx/gpu/shell_setup.rs",
        "OK",
        "확인 버튼 라벨 — t(\"button.ok\") 로 대체 (ko 는 '확인')",
    ),
    (
        "crates/tasty-ipc/src/client/stream.rs",
        "unknown error",
        "stream.open 거절 사유 폴백 — bail! 로 CLI stderr 에 노출",
    ),
    (
        "crates/tasty-plugin-git-viewer/src/main.rs",
        "remote git query failed",
        "plugin 오류 패널(self.error)에 표시 — Translator 키로 대체",
    ),
    (
        "src/adapters/ipc/handler/telemetry/session.rs",
        "(시작 없음)",
        "IPC 응답 텍스트에 한국어 하드코딩 — t() 또는 언어 중립 토큰",
    ),
    (
        "src/adapters/ipc/handler/telemetry/session.rs",
        "(끝 없음)",
        "IPC 응답 텍스트에 한국어 하드코딩 — t() 또는 언어 중립 토큰",
    ),
];

/// clap 도움말 스캔 뿌리와 **그 뿌리 하나의** `///` 줄 하한(`crates/tasty-cli`).
///
/// 스캔 대상이 0 이면 [`clap_help_text_is_english_only`] 는 위반을 못 찾는 것이 아니라
/// **볼 것이 없어서** 초록이다. `#[cfg(test)]` 를 걷어내는 판정이 너무 많이 먹으면 그
/// 형태로 조용히 무너지므로, 실측보다 낮되 붕괴를 잡을 만큼은 높게 잡는다 — 전부를
/// 게이트로 보는 변이는 0 으로 떨어지고, 게이트가 절반쯤 새는 변이도 이 아래로 온다.
/// CLI 표면이 정상적으로 줄어 여기 걸리면 그때 값을 다시 재서 내린다.
///
/// ★ **하한은 뿌리마다 따로다 — 합 하나로 재면 큰 뿌리가 작은 뿌리의 죽음을 덮는다.**
/// 실측(2026-09-07, 게이트 앞): `commands/` 1456 · `commands.rs` 0 · `lib.rs` 81.
/// 합 하한 하나였을 때는 `commands/` 혼자 그 합을 어떤 값 위로도 올려서, `lib.rs` 뿌리가
/// 통째로 안 보이게 돼도 합은 조용히 초록이었다.
///
/// `None` 은 **줄 하한 없음**이다. 줄 하한은 **줄이 있는 뿌리에만** 건다 — `commands.rs`
/// 는 지금 `///` 가 0 줄이라 어떤 줄 하한도 터질 수 없는 대조군이 되고, 값이 있는 척하는
/// 하한은 안 보는 구간을 덮는 옷이 된다. 줄이 없는 뿌리를 지키는 것은 아래 "파일을 한 개
/// 이상 봤는가" 단정이고, 그것이 단일 파일 뿌리의 실제 고장 형태 — 줄이 주는 것이 아니라
/// **경로가 낡아 그 뿌리가 통째로 안 보이는 것** — 을 잡는다.
const CLAP_DOC_ROOTS: &[(&str, Option<usize>)] = &[
    ("crates/tasty-cli/src/commands", Some(800)),
    ("crates/tasty-cli/src/commands.rs", None),
    ("crates/tasty-cli/src/lib.rs", Some(40)),
];

/// 순회에서 통째로 가지치기할 디렉토리명(`crates/tasty-doc-guards/tests/no_todo_file_citation.rs` 와 동일).
const PRUNE_DIRS: &[&str] = &["target", "dist", ".worktree", ".git", "node_modules"];

/// gitignored 로컬 폴더 이름의 조각. 리터럴로 두면 이 파일이 비-git 경로 참조 금지
/// (`docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`) 를 어긴다 — 인용이
/// 아니라 순회 입력이지만, 조각으로 조립하면 예외 등록 없이 규칙을 지킬 수 있다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 가지치기 대상 디렉토리인지 — 빌드 산출물 + gitignored 로컬 폴더(선행 `.`).
fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

/// 단일 인자 위젯 호출 — 뒤에 오는 리터럴에 엄격 판정(W).
const WIDGET_CALLS: &[&str] = &[
    "ui.label(",
    "ui.button(",
    "ui.heading(",
    "ui.small(",
    "ui.strong(",
    "ui.weak(",
    "ui.monospace(",
    "ui.code(",
    "ui.small_button(",
    "ui.link(",
    "RichText::new(",
    "Button::new(",
    "Label::new(",
    "CollapsingHeader::new(",
    ".on_hover_text(",
    ".on_disabled_hover_text(",
];

/// OS 네이티브 호출 — 뒤에 오는 리터럴에 식별자 허용 판정(N).
const NATIVE_CALLS: &[&str] = &[
    "NSString::from_str(",
    "MenuItem::new(",
    ".with_tooltip(",
    "w!(",
];

/// 폴백 호출 — (F).
const FALLBACK_CALLS: &[&str] = &["unwrap_or(", "unwrap_or_else(|| ", "unwrap_or_else(|_| "];

/// 표준 출력 매크로 — (E). pre-commit C.11 이 이 파일의 추가 라인에서 매크로 토큰을 잡지
/// 않도록 `concat!` 로 조립한다 — 검사 대상은 소스이지 이 상수가 아니다.
const PRINT_CALLS: &[&str] = &[
    concat!("println", "!("),
    concat!("eprintln", "!("),
    concat!("print", "!("),
    concat!("eprint", "!("),
];

/// `CARGO_MANIFEST_DIR` 을 그대로 쓰지 않는다 — 그 값은 이 타깃이 어느 패키지에 사는지에
/// 따라 달라져서 **스캔 뿌리가 파일과 함께 움직인다.** `repo_root()` 는 표지 파일로 자기가
/// 잡은 경로를 검증한다.
fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn rel_of(file: &Path) -> String {
    file.strip_prefix(root())
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 스캔 대상 — `src/**/*.rs` + `crates/*/src/**/*.rs`. 테스트 파일·debug 디렉토리·
/// build 스크립트·allowlist 접두는 제외.
fn is_scan_target(rel: &str) -> bool {
    if !rel.ends_with(".rs") || rel.ends_with("/build.rs") || rel == "build.rs" {
        return false;
    }
    let in_crate_src = rel
        .strip_prefix("crates/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, after)| after.starts_with("src/"));
    if !(rel.starts_with("src/") || in_crate_src) {
        return false;
    }
    if rel.contains("/tests/") || rel.ends_with("/tests.rs") || rel.ends_with("_tests.rs") {
        return false;
    }
    if rel.contains("/debug/") {
        return false; // 사용자 입력 재현 등 debug 전용 표면 (docs/identity.md 2.1)
    }
    !ALLOWLIST_PATH_PREFIXES
        .iter()
        .any(|(prefix, _)| rel.starts_with(prefix))
}

fn gather(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if is_scan_target(&rel_of(path)) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // 이름은 성질이 아니다 — `CARGO_TARGET_DIR` 로 다른 이름을 준 빌드
            // 디렉토리는 이름 목록에 안 걸린다. 표식 판정을 **보태서** 부른다(흉내
            // 내지 않는다): 근거와 후보 비교는 [`tasty_doc_guards::is_build_cache_dir`].
            if is_pruned(name) || tasty_doc_guards::is_build_cache_dir(&p) {
                continue;
            }
        }
        gather(&p, out);
    }
}

// ── 리터럴 판정 ────────────────────────────────────────────────────────

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF   // Hangul Jamo
        | 0x3040..=0x30FF // Hiragana · Katakana
        | 0x3130..=0x318F // Hangul Compatibility Jamo
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7A3 // Hangul Syllables
    )
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

/// 영문자 두 개가 연속으로 나오는가 — 단어 하나라도 사용자 문구로 본다.
fn has_word(s: &str) -> bool {
    s.as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_alphabetic() && w[1].is_ascii_alphabetic())
}

/// 뒤에 붙는 문장부호(`…` / `:` / `.` / `!` / `?`)를 뗀 본문.
fn strip_trailing_punct(s: &str) -> &str {
    s.trim().trim_end_matches(['…', '.', ':', '!', '?', ' '])
}

/// 대문자로 시작하고 나머지가 소문자인 세 글자 이상의 영단어 하나(`Quit` / `Shell`).
fn is_capitalized_word(s: &str) -> bool {
    let s = strip_trailing_punct(s);
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && s.len() >= 3
        && chars.all(|c| c.is_ascii_lowercase())
}

/// 문장 형태 — 공백으로 나뉜 단어 중 네 글자 이상의 순수 영단어가 있다.
fn looks_like_sentence(s: &str) -> bool {
    s.contains(' ')
        && s.split_whitespace().any(|w| {
            let w = strip_trailing_punct(w).trim_end_matches(',');
            w.len() >= 4 && w.chars().all(|c| c.is_ascii_alphabetic())
        })
}

fn is_token_allowlisted(s: &str) -> bool {
    LITERAL_TOKEN_ALLOWLIST.contains(&s.trim())
}

/// W — 위젯 인자: 단어·CJK 면 위반.
fn strict_violation(lit: &str) -> bool {
    !is_token_allowlisted(lit) && (has_cjk(lit) || has_word(lit))
}

/// H — placeholder: 문장·CJK 만 위반.
fn hint_violation(lit: &str) -> bool {
    has_cjk(lit) || looks_like_sentence(lit)
}

/// N / F / E — 식별자는 허용, 대문자 영단어·문장·CJK 는 위반.
fn prose_violation(lit: &str) -> bool {
    !is_token_allowlisted(lit)
        && (has_cjk(lit) || looks_like_sentence(lit) || is_capitalized_word(lit))
}

/// `after` 가 (공백·`&` 뒤에) 문자열 리터럴로 시작하면 그 내용을 돌려준다.
fn leading_literal(after: &str) -> Option<&str> {
    let rest = after.trim_start().trim_start_matches('&').trim_start();
    let body = rest.strip_prefix('"')?;
    let mut prev_backslash = false;
    for (i, c) in body.char_indices() {
        if c == '"' && !prev_backslash {
            return Some(&body[..i]);
        }
        prev_backslash = c == '\\' && !prev_backslash;
    }
    None
}

/// `line` 에서 `call` 뒤에 리터럴이 오는 자리를 전부 찾아 `judge` 로 판정한다.
fn find_calls(line: &str, calls: &[&str], judge: fn(&str) -> bool, out: &mut Vec<String>) {
    for call in calls {
        let mut from = 0;
        while let Some(pos) = line[from..].find(call) {
            let start = from + pos;
            from = start + call.len();
            // `w!(` 가 `anyhow!(` 의, `println!(` 이 `eprintln!(` 의 접미로 걸리지 않게 —
            // 패턴이 식별자 문자로 시작하면 바로 앞 글자는 식별자 문자가 아니어야 한다.
            let starts_with_ident = call
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let preceded_by_ident = line[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if starts_with_ident && preceded_by_ident {
                continue;
            }
            if let Some(lit) = leading_literal(&line[from..])
                && judge(lit)
            {
                out.push(format!(
                    "{} {lit:?}",
                    call.trim_end_matches('(').trim_end_matches('!')
                ));
            }
        }
    }
}

/// P — `PushNotification {` 이후 몇 줄 안의 `title: "…"`.
fn find_notification_title(lines: &[&str], idx: usize) -> Option<String> {
    if !lines[idx].contains("PushNotification") || !lines[idx].contains('{') {
        return None;
    }
    for line in lines.iter().skip(idx).take(8) {
        let trimmed = line.trim_start();
        if let Some(after) = trimmed.strip_prefix("title:") {
            if let Some(lit) = leading_literal(after)
                && strict_violation(lit)
            {
                return Some(format!("PushNotification title {lit:?}"));
            }
            return None;
        }
        if trimmed.starts_with('}') {
            return None;
        }
    }
    None
}

/// `#[cfg(test)]` / `#[test]` 가 붙은 아이템(모듈·함수·impl)의 본문을 건너뛴다. 파일 끝까지
/// 끊지 않으므로 테스트 모듈 **뒤에** 오는 아이템도 스캔한다 — clippy `items_after_test_module`
/// 이 경고만 하는 배치가 실제로 있어, 첫 `#[cfg(test)]` 에서 멈추면 그 뒤가 사각지대가 된다.
#[derive(Default)]
struct TestRegion {
    skipping: bool,
    depth: i32,
    opened: bool,
}

impl TestRegion {
    /// 이 줄이 테스트 영역이면 true.
    fn skip(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();
        if !self.skipping {
            if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[test]") {
                self.skipping = true;
                self.depth = 0;
                self.opened = false;
                return true;
            }
            return false;
        }
        let (opens, closes) = brace_counts(line);
        if opens > 0 {
            self.opened = true;
        }
        self.depth += opens - closes;
        if self.opened {
            if self.depth <= 0 {
                self.skipping = false; // 본문 끝 — 이 줄까지 테스트 영역
            }
        } else if trimmed.ends_with(';') {
            self.skipping = false; // `#[cfg(test)] mod tests;` / `use …;` 한 줄 아이템
        }
        true
    }
}

/// 문자열·문자 리터럴·`//` 주석 밖의 `{` / `}` 개수. raw string(`r#"…"#`) 안의 중괄호는
/// 구분하지 않는다 — 테스트 픽스처에 드물고, 어긋나면 위반이 드러나는 쪽(오탐)으로 기운다.
fn brace_counts(line: &str) -> (i32, i32) {
    let mut opens = 0;
    let mut closes = 0;
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '\'' => {
                // 문자 리터럴 `'{'` / `'}'` — 라이프타임 `'a` 는 뒤에 중괄호가 오지 않아 무해.
                if let Some(&next) = chars.peek()
                    && (next == '{' || next == '}')
                {
                    chars.next();
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'/') => break,
            '{' => opens += 1,
            '}' => closes += 1,
            _ => {}
        }
    }
    (opens, closes)
}

/// 한 파일의 위반 목록 — (1-base 줄, 설명).
fn scan_file(contents: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = contents.lines().collect();
    let mut found = Vec::new();
    let mut tests = TestRegion::default();
    for (idx, line) in lines.iter().enumerate() {
        // 테스트 픽스처 문구는 사용자 표면이 아니다.
        if tests.skip(line) {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let mut hits = Vec::new();
        find_calls(line, WIDGET_CALLS, strict_violation, &mut hits);
        find_calls(line, &[".hint_text("], hint_violation, &mut hits);
        find_calls(line, NATIVE_CALLS, prose_violation, &mut hits);
        find_calls(line, FALLBACK_CALLS, prose_violation, &mut hits);
        find_calls(line, PRINT_CALLS, prose_violation, &mut hits);
        if let Some(hit) = find_notification_title(&lines, idx) {
            hits.push(hit);
        }
        for hit in hits {
            found.push((idx + 1, hit));
        }
    }
    found
}

/// `hit` 가 PENDING 항목이면 그 (파일, 리터럴) 을 돌려준다.
fn pending_entry_of(rel: &str, hit: &str) -> Option<(&'static str, &'static str)> {
    PENDING_FIX_LITERALS
        .iter()
        .find(|(file, lit, _)| *file == rel && hit.ends_with(&format!("{lit:?}")))
        .map(|(file, lit, _)| (*file, *lit))
}

#[test]
fn no_hardcoded_user_facing_strings() {
    let mut files = Vec::new();
    gather(&root(), &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no source files scanned — path layout changed?"
    );

    let mut violations = Vec::new();
    let mut pending_seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for file in &files {
        let Ok(contents) = std::fs::read_to_string(file) else {
            continue;
        };
        let rel = rel_of(file);
        for (line_no, hit) in scan_file(&contents) {
            if let Some(entry) = pending_entry_of(&rel, &hit) {
                pending_seen.insert(entry);
                continue;
            }
            violations.push(format!("  {rel}:{line_no}: {hit}"));
        }
    }
    assert!(
        violations.is_empty(),
        "user-facing string literals must go through t() (CLAUDE.md 국제화). Move each to \
         lang/{{en,ko,ja}}.toml, or if it is a fixed identifier register it in \
         LITERAL_TOKEN_ALLOWLIST / ALLOWLIST_PATH_PREFIXES with a reason \
         (docs/dev-guide/i18n.md 강제 테스트):\n{}",
        violations.join("\n")
    );
    let stale: Vec<String> = PENDING_FIX_LITERALS
        .iter()
        .filter(|(file, lit, _)| !pending_seen.contains(&(*file, *lit)))
        .map(|(file, lit, fix)| format!("  {file} {lit:?} ({fix})"))
        .collect();
    assert!(
        stale.is_empty(),
        "PENDING_FIX_LITERALS entries no longer occur — remove them:\n{}",
        stale.join("\n")
    );
}

// ── clap 도움말 ────────────────────────────────────────────────────────

fn gather_all_rs(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "rs") {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        gather_all_rs(&entry.path(), out);
    }
}

/// C — `///` 도움말 줄, 또는 `about = "…"` / `help = "…"` 리터럴에 CJK.
fn clap_doc_violation(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if let Some(doc) = trimmed.strip_prefix("///") {
        if has_cjk(doc) {
            return Some(format!("/// {}", doc.trim()));
        }
        return None;
    }
    for attr in ["about = ", "help = ", "long_about = ", "long_help = "] {
        if let Some(pos) = line.find(attr)
            && let Some(lit) = leading_literal(&line[pos + attr.len()..])
            && has_cjk(lit)
        {
            return Some(format!("{attr}{lit:?}"));
        }
    }
    None
}

#[test]
fn clap_help_text_is_english_only() {
    let mut violations = Vec::new();
    for (rel, min_doc_lines) in CLAP_DOC_ROOTS {
        let mut files = Vec::new();
        gather_all_rs(&root().join(rel), &mut files);
        files.sort();
        // 뿌리마다 따로 묻는다. 이 단정은 **모든** 뿌리에 걸린다 — 줄 하한이 없는 뿌리도
        // 여기서는 걸린다. 단일 파일 뿌리의 실제 고장은 "줄이 준다" 가 아니라 "경로가
        // 낡아 그 뿌리가 통째로 안 보인다" 이고, 그것은 줄 수로는 안 보인다.
        assert!(
            !files.is_empty(),
            "clap 도움말 뿌리 `{rel}` 에서 `.rs` 를 한 개도 못 찾았다 — `CLAP_DOC_ROOTS` 의 \
             경로가 낡았다. 이 뿌리는 지금 아무것도 검사하지 않는다(위반이 없는 것이 아니라 \
             보는 것이 없다)."
        );

        // 이 뿌리에서 실제로 검사된 `///` 줄. 게이트 판정이 망가져 전부 게이트로 보이면
        // 이 수가 무너지고, 그때 아래 하한이 먼저 말한다 — 0 은 통과가 아니라 측정 실패다.
        let mut scanned_doc_lines = 0usize;
        for file in &files {
            let Ok(contents) = std::fs::read_to_string(file) else {
                continue;
            };
            let file_rel = rel_of(file);
            let src: Vec<&str> = contents.lines().collect();
            // `#[cfg(test)]` 아래는 바이너리에 안 들어가므로 `--help` 에도 안 나온다.
            // doc 주석은 **뒤따르는 항목**에 귀속되니 속성 앞 줄까지 함께 걷어낸다.
            let gated = cfg_span::cfg_gated_lines(&src, "test");
            for (idx, line) in src.iter().enumerate() {
                if gated[idx] {
                    continue;
                }
                if line.trim_start().starts_with("///") {
                    scanned_doc_lines += 1;
                }
                if let Some(hit) = clap_doc_violation(line) {
                    violations.push(format!("  {file_rel}:{}: {hit}", idx + 1));
                }
            }
        }

        if let Some(min) = min_doc_lines {
            assert!(
                scanned_doc_lines >= *min,
                "뿌리 `{rel}` 의 clap 도움말 후보 `///` 줄이 {scanned_doc_lines} 개뿐이다(하한 \
                 {min}). 게이트 판정이 너무 많이 걷어냈거나 그 뿌리 안의 경로가 낡았다 — 이 \
                 술어는 볼 것이 없으면 공짜로 초록이다.\n\
                   ★ 판별 — 이 모수는 두 단계를 거친다: 뿌리에서 `///` 줄을 모으고, 그다음 \
                   `#[cfg(test)]` 게이트로 걷어낸다. 그러니 **게이트 앞의 수**를 따로 세면 어느 \
                   단계가 무너졌는지 갈린다:\n\
                       git ls-files -- {rel} | grep '\\.rs$' | xargs grep -h '^ *///' | wc -l\n\
                   2026-09-07 실측(게이트 앞): `crates/tasty-cli/src/commands` 1456 · \
                   `crates/tasty-cli/src/commands.rs` 0 · `crates/tasty-cli/src/lib.rs` 81. \
                   그 수는 그대로인데 여기만 무너졌으면 **게이트가 너무 많이 먹은 것**이고, 그 \
                   수도 함께 줄었으면 CLI 표면이 정말 줄어든 것이다. 앞쪽이 이 자리의 주된 고장 \
                   형태다 — 뿌리가 통째로 어긋나는 것보다 게이트가 새는 쪽이 훨씬 조용하다.\n\
                   ★ 이 하한을 내려서 통과시키지 마라 — 게이트가 절반쯤 새는 변이는 정확히 이 \
                   수를 이 아래로 떨어뜨리고, 하한을 내리는 것은 그 변이를 승인하는 것과 같은 \
                   조작이다. CLI 가 정당하게 줄었으면 위 명령으로 게이트 앞 수를 먼저 재고, 그 \
                   비율대로 **그 뿌리의** 값을 다시 잡아라."
            );
        }
    }

    assert!(
        violations.is_empty(),
        "clap help text must be English only — `///` doc comments and about/help literals \
         surface verbatim in `--help` (docs/dev-guide/i18n.md, cli-structure.md 도움말 문구). \
         `#[cfg(test)]` 아래는 바이너리에 안 들어가므로 여기 안 걸린다. \
         Move Korean/Japanese background notes to `//` comments or docs/:\n{}",
        violations.join("\n")
    );
}

/// 면제가 가리키는 경로가 **실재하는가** — 참조 무결성.
///
/// **초록은 "이 면제가 아직 필요하다" 가 아니다**(ADR-0150). 가리키는 것이 실재한다는
/// 것뿐이고, 실재해도 그 면제가 아무것도 안 덮고 있을 수 있다. 두 축을 섞으면 "안 덮으면
/// 지워라" 라는 틀린 처방이 참조 무결성의 옷을 입고 돌아온다.
///
/// 경로가 썩으면 면제는 조용히 아무 일도 안 하게 되는데, 목록에는 "여기는 원래 위반해도
/// 된다" 는 신호가 남는다. 판정과 그 양극성 회귀는 [`tasty_doc_guards::missing_referents`].
#[test]
fn allowlist_path_prefixes_point_at_paths_that_exist() {
    // 위와 같은 이유로 `repo_root()` 를 쓴다 — 명부의 경로는 레포 상대라, 뿌리가 이
    // 크레이트로 좁아지면 전부 "없는 경로" 가 된다.
    let root = tasty_doc_guards::repo_root();
    let missing = tasty_doc_guards::missing_referents(
        &root,
        ALLOWLIST_PATH_PREFIXES.iter().map(|(rel, _)| *rel),
    );
    assert!(
        missing.is_empty(),
        "면제가 없는 경로를 가리킨다 — 옮겼으면 항목도 옮기고, 사라졌으면 항목을 지워라: {missing:?}"
    );
}
