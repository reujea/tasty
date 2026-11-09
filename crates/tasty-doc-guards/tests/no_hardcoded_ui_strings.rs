//! 사용자에게 보이는 하드코딩 문자열을 찾는다. 번역 정책과 예외는 docs/dev-guide/i18n.md에 있다.
//! 자동 실행 경로는 docs/dev-guide/ci-gates.md에 있다.
//!
//! 호출 형태와 문자 패턴을 검사하므로 모든 사용자 문구를 찾는 것은 아니다.
//! - W: egui 위젯 인자에서 연속 영문자 두 개 또는 CJK를 찾는다.
//! - H: hint_text는 예시 식별자를 허용하고 문장 형태(공백과 네 글자 이상 영단어)·CJK를 찾는다.
//! - N/F/E: 네이티브 메뉴, unwrap_or 폴백, 출력 매크로에서 문장·대문자 시작 영단어·CJK를 찾는다.
//!   소문자 한 단어는 식별자일 수 있어 허용하지만 실제 화면에 표시한다면 번역해야 한다.
//! - P: PushNotification의 title 리터럴을 찾는다.
//! - C: tasty-cli의 clap 도움말 주석과 about/help 속성에서 CJK를 찾는다. 영어 도움말은 허용한다.
//!
//! 키 이름·언어 이름·폰트 예시·단위·제품명 등은 허용 토큰으로 둔다.
//! 구조 출력·갤러리·데모·시뮬레이터·debug 경로 등은 아래 경로 목록으로 제외한다.
//! tracing 로그는 대상 호출에 없으며 테스트 아이템과 주석 줄도 제외한다.
//! 알려진 미수정 문구는 PENDING_FIX_LITERALS에 고칠 방법과 함께 기록하고, 사라지면 항목을 지운다.

use tasty_doc_guards::cfg_predicate as cfg_span;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 스캔에서 통째로 제외하는 경로 접두 — (접두, 이유).
const ALLOWLIST_PATH_PREFIXES: &[(&str, &str)] = &[
    (
        "crates/tasty-gallery/",
        "갤러리는 영어 예시를 표시하는 카탈로그다(i18n.md 공용 위젯 절).",
    ),
    (
        "crates/tasty-plugin-mesh-demo/",
        "사용자에게 배포하지 않는 데모 플러그인이다.",
    ),
    ("crates/tasty-tui-simulator/", "테스트용 TUI 시뮬레이터"),
    (
        "src/source_guards/",
        "src/lib.rs에서 cfg(test)로 선언한 테스트 전용 모듈이다. 이 검사는 다른 파일의 모듈 선언을 따라가지 못하므로 경로로 제외한다.",
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
        "src/boot/single_instance/launch_log.rs",
        "launch.log 진단 줄과 그 stderr 사본이다. 내부 진단 로그와 같은 자리이며 사용자 표면이 아니다(i18n.md 예외). release stderr tracing은 warn 이상만 보여 eprint를 쓰므로 .githooks/pre-commit C.11도 같은 파일을 면제한다.",
    ),
    (
        "crates/tasty-doc-guards/src/bin/",
        "개발자용 검사 도구의 진단이다. 의존성이 없는 크레이트이므로 번역 테이블을 추가하지 않는다(docs/dev-guide/guard-verification.md). 바이너리 경로만 제외한다.",
    ),
];

/// 리터럴 그대로 허용하는 토큰 — 번역하면 의미가 변하는 고유명사·식별자(i18n.md).
const LITERAL_TOKEN_ALLOWLIST: &[&str] = &[
    // EmptySurface::type_name의 기존 IPC type 식별자. 번역 대상 표시 문구가 아니다.
    "Empty",
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
    "简体中文",
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
    // Windows 클립보드 형식 이름. RegisterClipboardFormatW 에 넘기는 식별자다(Explorer 와 같아야 한다).
    "Preferred DropEffect",
];

/// 알려진 잔존 위반 — (파일, 리터럴, 고칠 방법). 여기 있는 동안은 실패로 치지 않되,
/// 사라지면(=고쳐지면) 항목을 지우라고 fail 한다. 새 위반을 여기 넣어 덮지 않는다.
const PENDING_FIX_LITERALS: &[(&str, &str, &str)] = &[
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

/// CLI 도움말의 /// 줄을 루트별로 센다. 한 루트의 수집 실패가 다른 결과에 가려지지 않아야 한다.
/// 2026-09-07 cfg 제외 전 측정: commands/ 1456줄, commands.rs 0줄, lib.rs 81줄.
/// None은 줄 하한이 없다는 뜻이며, 모든 루트에는 별도로 파일 존재 검사를 적용한다.
/// 실제 도움말이 줄었다면 cfg 제외 전후를 다시 측정해 해당 하한을 검토한다.
const CLAP_DOC_ROOTS: &[(&str, Option<usize>)] = &[
    ("crates/tasty-cli/src/commands", Some(800)),
    ("crates/tasty-cli/src/commands.rs", None),
    ("crates/tasty-cli/src/lib.rs", Some(40)),
];

const PRUNE_DIRS: &[&str] = &["target", "dist", ".worktree", ".git", "node_modules"];

/// 로컬 경로를 문서 인용으로 오인하지 않도록 순회용 이름을 조립한다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

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

/// pre-commit C.11이 검사 패턴을 실제 호출로 오인하지 않도록 조립한다.
const PRINT_CALLS: &[&str] = &[
    concat!("println", "!("),
    concat!("eprintln", "!("),
    concat!("print", "!("),
    concat!("eprint", "!("),
];

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
        return false; // 사용자 입력 재현 등 debug 전용 표면 (docs/principles/control-and-data.md)
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
            // 이름이 다른 CARGO_TARGET_DIR도 캐시 표식으로 제외한다.
            if is_pruned(name) || tasty_doc_guards::is_build_cache_dir(&p) {
                continue;
            }
        }
        gather(&p, out);
    }
}

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

/// 테스트 아이템 본문만 건너뛰어 테스트 모듈 뒤의 제품 코드도 검사한다.
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

/// 일반 문자열·중괄호 문자 리터럴·줄 주석 밖의 중괄호를 센다.
/// raw string을 별도로 해석하지 않아 테스트 범위를 잘못 판단할 수 있다.
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
         lang/{{en,ko,ja,zh-Hans}}.toml, or if it is a fixed identifier register it in \
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
        // 도움말 줄이 원래 없는 루트도 파일 수집 실패는 확인해야 한다.
        assert!(
            !files.is_empty(),
            "clap 도움말 루트 `{rel}`에서 Rust 파일을 찾지 못했다. CLAP_DOC_ROOTS의 경로와 수집을 확인한다."
        );

        let mut scanned_doc_lines = 0usize;
        for file in &files {
            let Ok(contents) = std::fs::read_to_string(file) else {
                continue;
            };
            let file_rel = rel_of(file);
            let src: Vec<&str> = contents.lines().collect();
            // doc 주석은 다음 아이템에 속하므로 test 속성 앞의 주석도 제외한다.
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
                "`{rel}`의 clap 도움말 후보가 {scanned_doc_lines}줄뿐이다(하한 {min}). cfg 제외 전의 /// 줄 수와 비교해 수집·제외 범위를 확인한다. 도움말이 실제로 줄었다면 해당 루트의 하한과 측정 근거를 함께 갱신한다."
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

/// 예외 경로의 존재만 확인한다. 해당 예외가 아직 필요한지는 별도 검토해야 한다.
#[test]
fn allowlist_path_prefixes_point_at_paths_that_exist() {
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
