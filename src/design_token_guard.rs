//! UI 폰트·반경 토큰 값을 복사한 명명 상수, 생성 길이 상수의 직접 사용과 익명 색·점 치수를 검사한다.
//! 배율 적용은 Theme가 맡으므로 토큰 값과 같은 const라도 직접 쓰면 설정을 반영하지 못할 수 있다.
//! 토큰 없는 값의 이름·사유 규칙은 docs/design/systems/theme.md를 따른다. 자동 실행 범위는 docs/dev-guide/ci-gates.md에 있다.
//!
//! 숫자나 LogicalPx 숫자로 정의한 const만 읽으며 계산식·다른 상수·지역 변수는 추적하지 않는다.
//! 상수는 스캔 루트 전체에서 이름으로 찾으므로 루트 밖 정의와 같은 이름의 다른 모듈을 정확히 구별하지 못한다.
//! 폰트 검사는 size 호출을 기본 대상으로 보고, 최근 12줄의 같은 문장에 Spinner::new가 있으면 지름으로 제외한다.
//! 주석으로 시작하는 줄은 제외하지만 파일 전체를 Rust 문법으로 해석하는 검사는 아니다.

use std::path::{Path, PathBuf};

use tasty_type_geometry::length::LogicalPx;

/// 인라인 리터럴 검사와 같은 디렉터리를 대상으로 한다. 두 목록은 별도 시험에서 대조한다.
/// 개별 파일만 등록하면 나중에 추가한 파일이 빠질 수 있어 디렉터리를 등록한다.
const SCAN_ROOTS: &[&str] = &[
    "src/view",
    "src/adapters/ui",
    "src/gfx/gpu",
    "crates/tasty-gallery/src",
    "crates/tasty-ui-widgets/src",
    "crates/tasty-egui-theme/src",
];

/// 별도 테스트 타깃의 상수를 직접 공유할 수 없어 소스의 SCAN_ROOTS를 읽는다.
fn sister_scan_roots(src: &str) -> Vec<String> {
    let Some(start) = src.find("const SCAN_ROOTS: &[&str] = &[") else {
        return Vec::new();
    };
    let rest = &src[start..];
    let Some(end) = rest.find("];") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in rest[..end].lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut in_string = false;
        let mut buf = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if in_string {
                if c == '"' {
                    out.push(std::mem::take(&mut buf));
                    in_string = false;
                } else {
                    buf.push(c);
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '/' && chars.get(i + 1) == Some(&'/') {
                // 주석에 적힌 따옴표 경로를 스캔 루트로 오인하지 않도록 줄 뒤쪽 주석도 제외한다.
                break;
            }
            i += 1;
        }
    }
    out
}

/// UI 폰트 토큰의 비교값. 콘텐츠 폰트와 값이 같아도 토큰 사용 대상이다.
/// 어떤 역할의 토큰인지까지는 이 검사로 결정하지 않는다.
const UI_FONT_TOKEN_VALUES: &[f32] = &[10.0, 11.0, 13.0, 14.0];

/// UI semantic이 없는 primitive 폰트 값은 상수 이름에 primitive와 값을 표시한다(docs/design/systems/theme.md).
/// 여기서는 아래 등록된 12·16만 확인하며 brand-wordmark/prose의 semantic 값은 제외한다.
const UNMAPPED_PRIMITIVE_FONT_VALUES: &[f32] = &[12.0, 16.0];

const FONT_CALLS: &[&str] = &[
    ".size(",
    "FontId::proportional(",
    "FontId::monospace(",
    "FontId::new(",
];

/// UI 반경 토큰의 비교값. Theme의 배율 적용을 건너뛰는 명명 상수 사본을 찾는다.
const UI_RADIUS_TOKEN_VALUES: &[f32] = &[2.0, 4.0, 8.0];

/// 인라인 리터럴 검사와 같은 호출에서 명명 상수 사용을 찾는다.
const RADIUS_CALLS: &[&str] = &[".corner_radius(", "CornerRadius::same("];

/// 숫자 또는 LogicalPx 숫자로 정의한 상수를 루트 전체에서 모은다.
fn collect_numeric_consts(lines: &[&str], out: &mut Vec<(String, f32)>) {
    for line in lines {
        let t = line.trim_start();
        let t = t.strip_prefix("pub(crate) ").unwrap_or(t);
        let t = t.strip_prefix("pub ").unwrap_or(t);
        let Some(rest) = t.strip_prefix("const ") else {
            continue;
        };
        let Some((name, rhs)) = rest.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            continue;
        }
        let Some((_, value)) = rhs.split_once('=') else {
            continue;
        };
        let v = value.trim().trim_end_matches(';').trim();
        let v = v
            .strip_prefix("LogicalPx(")
            .map(|s| s.trim_end_matches(')'))
            .unwrap_or(v);
        if let Some(n) = numeric_literal(v.trim()) {
            out.push((name.to_string(), n));
        }
    }
}

fn numeric_literal(tok: &str) -> Option<f32> {
    let t = tok.trim().trim_end_matches("f32").trim_end_matches("f64");
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<f32>().ok()
}

fn is_screaming_snake(tok: &str) -> bool {
    tok.len() >= 3
        && tok.starts_with(|c: char| c.is_ascii_uppercase())
        && tok
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// 최근 코드의 마지막 세미콜론 뒤에 Spinner::new가 있는지 본다. 스피너 size는 폰트가 아니라 지름이다.
fn spinner_receiver(lines: &[&str], at: usize, before_call: &str) -> bool {
    // 검사 비용을 제한하려고 앞 12줄까지만 본다.
    let from = at.saturating_sub(12);
    let mut text = String::new();
    for prev in &lines[from..at] {
        text.push_str(prev);
        text.push('\n');
    }
    text.push_str(before_call);
    let stmt = match text.rfind(';') {
        Some(i) => &text[i + 1..],
        None => &text[..],
    };
    stmt.contains("Spinner::new()")
}

/// 한 줄의 모든 폰트 호출에서 상수 이름과 호출 위치를 모아 스피너 예외를 각각 적용한다.
fn font_call_args(line: &str) -> Vec<(String, usize)> {
    call_args(line, FONT_CALLS)
}

#[test]
fn the_font_arg_scan_sees_the_logical_px_form() {
    let wrapped = font_call_args(".size(ADD_PREVIEW_NAME_PRIMITIVE_16.value())");
    assert_eq!(
        wrapped.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["ADD_PREVIEW_NAME_PRIMITIVE_16"],
        "LogicalPx 상수를 value()로 전달한 호출을 찾지 못했다"
    );

    let bare = font_call_args(".size(SEGMENT_BADGE_SIZE)");
    assert_eq!(
        bare.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["SEGMENT_BADGE_SIZE"],
        "일반 상수 인자를 찾지 못했다"
    );

    assert!(
        font_call_args(".size(th.font_size_body.value())").is_empty(),
        "토큰 접근자를 명명 상수로 잡았다 — 이 가드의 대상이 아니다"
    );
}

/// 첫 인자가 명명 상수인 호출을 찾는다. 경로 접두를 벗겨 수집한 상수 이름과 비교한다.
fn call_args(line: &str, calls: &[&str]) -> Vec<(String, usize)> {
    let mut hits = Vec::new();
    for call in calls {
        let mut cursor = 0usize;
        while let Some(rel_at) = line[cursor..].find(call) {
            let at = cursor + rel_at;
            cursor = at + call.len();
            let rest = &line[cursor..];
            let Some(end) = rest.find([',', ')']) else {
                continue;
            };
            let arg = rest[..end].trim();
            let arg = arg.rsplit("::").next().unwrap_or(arg).trim();
            // LogicalPx 상수를 value()로 전달한 경우에도 상수 이름을 찾는다.
            let arg = arg.strip_suffix(".value(").unwrap_or(arg).trim();
            if is_screaming_snake(arg) {
                hits.push((arg.to_string(), at));
            }
        }
    }
    hits
}

fn const_font_violations(
    rel: &str,
    lines: &[&str],
    consts: &[(String, f32)],
    out: &mut Vec<String>,
) {
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (arg, at) in font_call_args(line) {
            if spinner_receiver(lines, i, &line[..at]) {
                continue;
            }
            let Some((_, value)) = consts.iter().find(|(n, _)| *n == arg) else {
                continue;
            };
            if UI_FONT_TOKEN_VALUES.contains(value) {
                out.push(format!(
                    "  {}:{} — `{}` = {} 는 UI 폰트 토큰과 같은 값이다",
                    rel,
                    i + 1,
                    arg,
                    value
                ));
            }
        }
    }
}

/// 등록한 primitive 값을 쓰는 폰트 상수에 PRIMITIVE_값 이름이 있는지 확인한다.
fn primitive_name_violations(
    rel: &str,
    lines: &[&str],
    consts: &[(String, f32)],
    out: &mut Vec<String>,
) {
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (arg, at) in font_call_args(line) {
            if spinner_receiver(lines, i, &line[..at]) {
                continue;
            }
            let Some((_, value)) = consts.iter().find(|(n, _)| *n == arg) else {
                continue;
            };
            if !UNMAPPED_PRIMITIVE_FONT_VALUES.contains(value) {
                continue;
            }
            let want = format!("PRIMITIVE_{}", *value as i64);
            if !arg.contains(&want) {
                out.push(format!(
                    "  {}:{} — `{}` = {} 는 semantic 이 없는 primitive 다. \
                     이름에 `{}` 를 담을 것",
                    rel,
                    i + 1,
                    arg,
                    value,
                    want
                ));
            }
        }
    }
}

fn gather_rs_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("스캔 디렉터리를 읽지 못했다: {} — {e}", path.display()));
    for entry in entries.flatten() {
        gather_rs_files(&entry.path(), out);
    }
}

/// 빈 수집 결과가 위반 0개로 통과하지 않게 하는 하한이다.
const MIN_SCANNED_FILES: usize = 200;

/// 스캔한 소스 파일 하나 — 저장소 기준 상대 경로와 전체 내용.
type ScannedSource = (String, String);
/// 소스에서 거둔 수치 상수 하나 — 이름과 값.
type NumericConst = (String, f32);

/// 파일을 넘는 상수 참조도 비교하도록 전체 상수 표를 모은 뒤 판정한다.
fn scan_sources() -> (Vec<ScannedSource>, Vec<NumericConst>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut files = Vec::new();
    for target in SCAN_ROOTS {
        let path = root.join(target);
        let before = files.len();
        gather_rs_files(&path, &mut files);
        assert!(
            files.len() > before,
            "스캔 루트 `{target}` 에서 .rs 파일을 하나도 찾지 못했다"
        );
    }
    assert!(
        files.len() >= MIN_SCANNED_FILES,
        "파일을 {}개만 읽었다(하한 {MIN_SCANNED_FILES}). 경로와 수집 범위를 확인한다.",
        files.len()
    );

    let mut sources = Vec::new();
    let mut consts = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let contents = std::fs::read_to_string(file).expect("소스 파일 read 실패");
        sources.push((rel, contents));
    }
    for (_, contents) in &sources {
        let lines: Vec<&str> = contents.lines().collect();
        collect_numeric_consts(&lines, &mut consts);
    }
    (sources, consts)
}

/// 반경 호출에는 폰트 검사의 스피너 예외가 없다.
fn const_radius_violations(
    rel: &str,
    lines: &[&str],
    consts: &[(String, f32)],
    out: &mut Vec<String>,
) {
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (arg, _) in call_args(line, RADIUS_CALLS) {
            let Some((_, value)) = consts.iter().find(|(n, _)| *n == arg) else {
                continue;
            };
            if UI_RADIUS_TOKEN_VALUES.contains(value) {
                out.push(format!(
                    "  {}:{} — `{}` = {} 는 UI 반경 토큰과 같은 값이다",
                    rel,
                    i + 1,
                    arg,
                    value
                ));
            }
        }
    }
}

/// 갤러리는 egui 전역 배율을 적용하므로 생성 길이 상수도 함께 커진다(docs/concepts/typed-length.md).
/// 이 예외만 제외하며 본체가 사용하는 UI 크레이트는 계속 검사한다.
const ZOOM_CONST_EXCLUDED_ROOT: &str = "crates/tasty-gallery/";

/// 상수 표와 검사 대상 파일이 비어 통과하지 않도록 각각 하한을 둔다.
const MIN_GENERATED_LOGICAL_CONSTS: usize = 200;
const MIN_GENERATED_OTHER_CONSTS: usize = 40;
/// 파일 수 하한만으로 특정 디렉터리 누락은 알 수 없어 루트 도달 여부도 확인한다.
const MIN_ZOOM_CONST_SCANNED_FILES: usize = 150;

/// LogicalPx 생성 상수와 나머지 상수를 나눈다. 길이의 배율 적용 우회만 이 검사에서 금지한다.
fn generated_const_types() -> (Vec<String>, Vec<String>) {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/tasty-design-tokens/src/generated");
    let (mut logical, mut other) = (Vec::new(), Vec::new());
    for file in ["primitive.rs", "semantic.rs", "component.rs"] {
        let text = std::fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("생성 토큰 파일을 읽지 못했다: {file} — {e}"));
        for line in text.lines() {
            let rest = line.trim_start();
            // 가시성이 넓어져도 놓치지 않도록 pub(crate) 상수도 포함한다.
            let Some(rest) = rest.strip_prefix("pub") else {
                continue;
            };
            let rest = rest.strip_prefix("(crate)").unwrap_or(rest);
            let Some(rest) = rest.trim_start().strip_prefix("const ") else {
                continue;
            };
            let Some((name, ty)) = rest.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if !is_screaming_snake(name) {
                continue;
            }
            let ty = ty.split('=').next().unwrap_or("").trim();
            if ty == "LogicalPx" {
                logical.push(name.to_string());
            } else {
                other.push(name.to_string());
            }
        }
    }
    (logical, other)
}

/// 생성 모듈 참조에서 상수까지의 경로를 읽는다. 이름만 보면 HEIGHT 같은 일반 상수를 오인할 수 있다.
/// 한 겹의 묶음 import는 펼치고 중첩 묶음은 끊긴 경로로 남겨 해석할 수 없다고 보고한다.
fn generated_const_refs(lines: &[&str]) -> Vec<(String, usize)> {
    // 검사 소스 자체가 대상 패턴을 갖지 않도록 조립한다.
    const NEEDLE: &str = concat!("tasty_design_tokens", "::generated::");
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut rest = *line;
        while let Some(at) = rest.find(NEEDLE) {
            let tail = &rest[at + NEEDLE.len()..];
            let path: String = tail
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            // rustfmt가 여러 줄로 나눈 묶음은 닫는 중괄호까지 연결한다.
            let mut after = tail[path.len()..].to_string();
            if after.starts_with('{') && !after.contains('}') {
                for next in lines.iter().skip(i + 1).take(64) {
                    after.push(' ');
                    after.push_str(next.trim());
                    if next.contains('}') {
                        break;
                    }
                }
            }
            match expand_group(&path, &after) {
                Some(items) => out.extend(items.into_iter().map(|p| (p, i + 1))),
                None => out.push((path, i + 1)),
            }
            rest = &rest[at + NEEDLE.len()..];
        }
    }
    out
}

/// 한 겹 묶음은 이름별로 펼친다. 중첩되거나 닫히지 않은 묶음은 None으로 반환한다.
fn expand_group(prefix: &str, after: &str) -> Option<Vec<String>> {
    if !prefix.ends_with("::") {
        return None;
    }
    let body = after.strip_prefix('{')?;
    let body = &body[..body.find('}')?];
    if body.contains('{') {
        return None;
    }
    let items: Vec<String> = body
        .split(',')
        .map(|item| item.split_whitespace().next().unwrap_or(""))
        .filter(|name| !name.is_empty())
        .map(|name| {
            if name == "self" {
                prefix.trim_end_matches("::").to_string()
            } else {
                format!("{prefix}{name}")
            }
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

/// 생성 길이 토큰을 해당 Theme 필드·접근자에 연결한다. 값이 같은 다른 토큰으로 대체하지 않는다.
/// SEMANTIC_DIM_TO_THEME_FIELD의 첫 항목을 우선하며 Theme에 실제 필드가 있어야 한다.
/// component 토큰은 자기 이름의 생성·수기 접근자도 확인한다. 둘 다 없으면 None이다.
fn theme_path_table() -> Vec<(String, String, Option<String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("Theme 경로를 확인할 파일을 읽지 못했다: {rel} — {e}"))
    };
    let theme = read("crates/tasty-type-appearance/src/theme.rs");
    let accessors = format!(
        "{theme}\n{}",
        read("crates/tasty-type-appearance/src/generated_component.rs")
    );
    // 자간 토큰은 글자 크기를 받는 생성 접근자가 Theme 경로다.
    let tracking = read("crates/tasty-type-appearance/src/generated_tracking.rs");
    let theme_struct = theme_struct_body(&theme);
    assert!(
        theme_struct.contains("pub spacing_xs: LogicalPx"),
        "Theme 본문에서 기준 필드를 찾지 못했다. 구조체 파싱을 확인한다."
    );

    let mut out = Vec::new();
    for (tier, rel) in [
        (
            "semantic",
            "crates/tasty-design-tokens/src/generated/semantic.rs",
        ),
        (
            "component",
            "crates/tasty-design-tokens/src/generated/component.rs",
        ),
    ] {
        let text = read(rel);
        let (mut module, mut token) = (String::new(), None::<String>);
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(m) = t.strip_prefix("pub mod ") {
                module = m.trim_end_matches(" {").trim().to_string();
            } else if let Some(doc) = t.strip_prefix(&format!("/// `{tier}.")) {
                token = doc.split('`').next().map(|n| format!("{tier}.{n}"));
            } else if let Some(decl) = t.strip_prefix("pub const ") {
                let Some((name, ty)) = decl.split_once(':') else {
                    continue;
                };
                let (Some(tok), true) = (token.take(), ty.trim_start().starts_with("LogicalPx"))
                else {
                    continue;
                };
                let path = if tier == "component" {
                    format!("component::{module}::{}", name.trim())
                } else {
                    format!("semantic::{}", name.trim())
                };
                let field = semantic_theme_field(
                    &tok,
                    tasty_design_tokens::dtcg::SEMANTIC_DIM_TO_THEME_FIELD,
                    theme_struct,
                );
                let component_fn = tok.strip_prefix("component.").map(|n| n.replace('-', "_"));
                let accessor = component_fn
                    .as_ref()
                    .filter(|f| accessors.contains(&format!("pub fn {f}(&self) -> LogicalPx")));
                let tracking_accessor = component_fn.as_ref().filter(|f| {
                    tracking.contains(&format!(
                        "pub fn {f}(&self, font_size: LogicalPx) -> LogicalPx"
                    ))
                });
                let via = match (field, accessor, tracking_accessor) {
                    (Some(f), ..) => Some(format!("th.{f}")),
                    (None, Some(f), _) => Some(format!("th.{f}()")),
                    (None, None, Some(f)) => Some(format!("th.{f}(font_size)")),
                    (None, None, None) => None,
                };
                out.push((path, tok, via));
            }
        }
    }
    out
}

/// ThemeSizing에만 있는 필드는 th에서 읽을 수 없으므로 Theme 본문만 찾는다.
fn theme_struct_body(theme: &str) -> &str {
    theme
        .split_once("pub struct Theme {")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map_or("", |(body, _)| body)
}

/// 토큰 매핑의 첫 항목을 쓰되 Theme의 LogicalPx 필드로 존재할 때만 반환한다.
fn semantic_theme_field<'a>(
    tok: &str,
    table: &[(&str, &'a str)],
    theme_struct: &str,
) -> Option<&'a str> {
    table
        .iter()
        .find(|(p, _)| *p == tok)
        .map(|(_, f)| *f)
        .filter(|f| theme_struct.contains(&format!("pub {f}: LogicalPx")))
}

/// Theme 경로가 없는 길이 토큰. 실제 결과와 집합이 같아야 한다.
/// 사용하려면 매핑과 ThemeSizing/Theme 필드·배율 적용·값 대조를 먼저 추가한다.
/// 값이 같은 다른 Theme 토큰으로 우회하지 않는다.
const PATHLESS_LENGTH_TOKENS: &[(&str, &str)] = &[
    (
        "semantic.field-width-range",
        "소비처가 없어 표에 오른 적이 없다",
    ),
    (
        "semantic.font-size-brand-lockup",
        "component `loading-lockup-font-size` 의 alias 로만 쓰인다 — 그 접근자는 역할 \
         이름이라 이 토큰의 경로가 아니다",
    ),
    (
        "semantic.font-size-brand-wordmark",
        "component `sidebar-wordmark-font-size` 의 alias 로만 쓰인다 — 그 접근자는 역할 \
         이름이라 이 토큰의 경로가 아니다",
    ),
    (
        "semantic.letter-spacing-ui",
        "자간은 글자 크기에 비례해 길이 상수 경로를 두지 않는다 — 생성 접근자 \
         `letter_spacing_ui(font_size)` 로 읽는다",
    ),
    (
        "semantic.radius-none",
        "component `webview-opaque-radius` 의 alias 로만 쓰인다 — 그 접근자는 역할 이름이라 이 \
         토큰의 경로가 아니다",
    ),
    (
        "semantic.radius-pill",
        "component `switch-radius` 의 alias 로만 쓰인다 — 그 접근자는 역할 이름이라 이 \
         토큰의 경로가 아니다",
    ),
];

fn length_const_prescription(
    rel: &str,
    at: usize,
    path: &str,
    table: &[(String, String, Option<String>)],
) -> String {
    match table.iter().find(|(p, ..)| p == path) {
        Some((_, tok, Some(via))) => format!(
            "  {rel}:{at}: 길이 상수 {path}({tok}) 대신 같은 토큰의 Theme 경로 `{via}`를 사용한다."
        ),
        Some((_, tok, None)) => format!(
            "  {rel}:{at}: {path}({tok})에는 Theme 경로가 없다(PATHLESS_LENGTH_TOKENS). 값이 같은 다른 토큰으로 우회하지 않는다. SEMANTIC_DIM_TO_THEME_FIELD, ThemeSizing/Theme 필드와 zoomed 적용, sizing_parity를 추가하고 생성물을 갱신한 뒤 목록에서 제거한다."
        ),
        // 상수 이름까지 읽지 못한 모듈·glob·중첩 import를 안내한다.
        None => format!(
            "  {rel}:{at}: {path}는 상수까지 경로가 이어지지 않는다(모듈 import·glob·중첩 묶음). 상수를 하나씩 전체 경로로 import해 해당 토큰의 Theme 경로를 확인한다."
        ),
    }
}

/// 생성 길이 상수를 직접 읽으면 Theme가 정한 배율 적용·제외 정책을 건너뛴다.
/// 따라서 UI는 같은 토큰의 Theme 경로로 읽는다. 길이가 아닌 상수는 이 검사에서 제외한다.
#[test]
fn ui_does_not_consume_generated_length_consts_directly() {
    let (logical, other) = generated_const_types();
    assert!(
        logical.len() >= MIN_GENERATED_LOGICAL_CONSTS,
        "생성 LogicalPx 상수를 {}개만 읽었다(하한 {MIN_GENERATED_LOGICAL_CONSTS}). 파싱을 확인한다. 나머지 타입은 {}개다.",
        logical.len(),
        other.len()
    );
    assert!(
        other.len() >= MIN_GENERATED_OTHER_CONSTS,
        "생성한 나머지 타입 상수를 {}개만 읽었다(하한 {MIN_GENERATED_OTHER_CONSTS}). 타입 분류를 확인한다. LogicalPx는 {}개다.",
        other.len(),
        logical.len()
    );
    // 같은 이름이 길이·나머지 분류에 모두 있으면 이름만으로 분류할 수 없어 전체 경로 판정이 필요하다.
    let collisions: Vec<&String> = logical.iter().filter(|n| other.contains(n)).collect();
    assert!(
        collisions.is_empty(),
        "상수 이름 {}개가 길이와 나머지 타입에 모두 있다. 이름만으로 구분하지 말고 전체 경로로 분류해야 한다: {:?}",
        collisions.len(),
        collisions
    );

    // 각 타입의 대표 상수도 확인해 개수만 맞는 잘못된 파싱을 찾는다.
    assert!(
        logical.iter().any(|n| n == "ICON_SIZE_SM"),
        "`ICON_SIZE_SM`(LogicalPx)을 길이 상수로 수집하지 못했다. 타입 파서를 확인한다."
    );
    assert!(
        other.iter().any(|n| n == "EDGE_DIM_OPACITY"),
        "`EDGE_DIM_OPACITY`(f32)가 무차원 표에 없다 — 타입 파서가 한쪽으로 쏠렸다"
    );

    let (sources, _) = scan_sources();
    let scanned: Vec<_> = sources
        .iter()
        .filter(|(rel, _)| !rel.starts_with(ZOOM_CONST_EXCLUDED_ROOT))
        .collect();
    assert!(
        scanned.len() >= MIN_ZOOM_CONST_SCANNED_FILES,
        "갤러리를 제외한 파일을 {}개만 읽었다(하한 {MIN_ZOOM_CONST_SCANNED_FILES}, 전체 {}개). 수집과 제외 범위를 확인한다.",
        scanned.len(),
        sources.len()
    );

    // 전체 파일 수가 충분해도 본체 UI 크레이트가 빠졌을 수 있어 각각 확인한다.
    for root in ["crates/tasty-ui-widgets/src", "crates/tasty-egui-theme/src"] {
        assert!(
            scanned.iter().any(|(rel, _)| rel.starts_with(root)),
            "본체 UI 크레이트 {root}에서 파일을 읽지 못했다. 갤러리와 달리 배율 적용 우회를 검사해야 한다."
        );
    }

    let table = theme_path_table();
    let mut violations = Vec::new();
    let mut dimensionless = 0usize;
    for (rel, contents) in &scanned {
        let lines: Vec<&str> = contents.lines().collect();
        for (path, at) in generated_const_refs(&lines) {
            let name = path.rsplit("::").next().unwrap_or("").to_string();
            // primitive LogicalPx도 검출하지만 Theme 안내 표는 semantic/component만 읽는다.
            // primitive가 공개되면 전체 경로가 있어도 import를 고치라는 부정확한 안내가 나올 수 있다.
            if other.contains(&name) {
                dimensionless += 1;
            } else {
                violations.push(length_const_prescription(rel, at, &path, &table));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "UI에서 생성 길이 상수를 직접 읽어 Theme의 배율 적용·제외 정책을 건너뛴다:\n{}\n같은 검사에서 길이가 아닌 상수 사용 {}건은 허용했다.",
        violations.join("\n"),
        dimensionless
    );
}

/// 스캔 범위가 넓어져도 검사 소스 자체가 금지 패턴을 담지 않게 한다.
#[test]
fn the_guard_does_not_carry_its_own_needle() {
    let me = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/design_token_guard.rs"),
    )
    .expect("자기 소스 read 실패");
    let whole = concat!("tasty_design_tokens", "::generated::");
    assert_eq!(
        me.matches(whole).count(),
        0,
        "검사 소스에 대상 패턴이 한 문자열로 남았다. 조립하는 형태를 유지한다."
    );
    assert!(
        me.matches("generated").count() > 0,
        "검사 소스에서 generated를 찾지 못했다. 읽은 파일을 확인한다."
    );
}

/// 깊은 경로의 일반적인 상수 이름을 써서 모듈 하나만 보거나 경로 첫 조각만 읽는 오검출을 확인한다.
#[test]
fn the_path_parser_beats_the_weak_forms_on_the_deepest_path() {
    // 합성 입력도 검사 패턴을 한 문자열로 소스에 남기지 않도록 조립한다.
    let line = concat!(
        "use tasty_design_tokens",
        "::generated::component::autocomplete::MAX_HEIGHT;"
    );
    let refs = generated_const_refs(&[line]);
    assert_eq!(
        refs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["component::autocomplete::MAX_HEIGHT"],
        "중간 모듈을 포함한 상수 경로를 읽지 못했다"
    );

    assert!(
        !line.contains("generated::semantic::"),
        "약한 형태(semantic 전용)가 이 줄을 잡아 버리면 대조가 성립하지 않는다"
    );
    // 첫 경로 조각을 상수명으로 쓰는 판정은 component를 얻어 실제 상수를 놓친다.
    let first = line
        .split("::generated::")
        .nth(1)
        .and_then(|t| t.split("::").next())
        .unwrap_or("");
    assert_eq!(first, "component");
    let (logical, other) = generated_const_types();
    assert!(
        !logical.iter().any(|n| n == first) && !other.iter().any(|n| n == first),
        "경로의 첫 조각이 상수 표에 있어 첫 이름만 읽는 방식과 비교할 수 없다"
    );

    assert!(
        logical.iter().any(|n| n == "MAX_HEIGHT"),
        "`MAX_HEIGHT`가 LogicalPx 표에 없어 경로의 마지막 상수를 읽는지 확인할 수 없다"
    );
}

/// 생성 LogicalPx 개수와 안내 표가 맞는지, component 토큰에 접근자가 있는지 확인한다.
/// Theme 경로가 없는 토큰은 PATHLESS_LENGTH_TOKENS와 대조한다.
#[test]
fn every_generated_length_token_has_a_theme_path_or_is_listed() {
    let table = theme_path_table();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (tier, file) in [("semantic", "semantic.rs"), ("component", "component.rs")] {
        let text = std::fs::read_to_string(
            root.join("crates/tasty-design-tokens/src/generated")
                .join(file),
        )
        .expect("생성 토큰 파일 read 실패");
        let declared = text
            .lines()
            .filter(|l| {
                l.trim_start().starts_with("pub const ")
                    && l.split_once(':')
                        .is_some_and(|(_, ty)| ty.trim_start().starts_with("LogicalPx"))
            })
            .count();
        let parsed = table
            .iter()
            .filter(|(p, ..)| p.starts_with(&format!("{tier}::")))
            .count();
        assert_eq!(
            parsed, declared,
            "{file}의 LogicalPx 상수{declared}개 중 {parsed}개만 안내 표에 있다. 토큰 문서 주석과 선언을 함께 읽는지 확인한다."
        );
    }
    let component = table
        .iter()
        .filter(|(p, ..)| p.starts_with("component::"))
        .count();
    assert!(
        component >= 200,
        "component 길이 상수를 {component}개만 읽었다. 안내 표 수집을 확인한다."
    );

    // 필드 매핑이 여럿인 토큰은 첫 항목이 우선이므로 해당 토큰을 각각 확인한다.
    for (path, want) in [
        ("semantic::SPACE_XS", "th.spacing_xs"),
        ("semantic::CONTROL_HEIGHT_TAB", "th.item_height_tab"),
        ("semantic::FONT_SIZE_BODY", "th.font_size_body"),
        ("semantic::FONT_SIZE_CAPTION", "th.font_size_caption"),
        (
            "component::autocomplete::MAX_HEIGHT",
            "th.autocomplete_max_height()",
        ),
        ("component::modhint::WIDTH", "th.modhint_width()"),
    ] {
        let got = table.iter().find(|(p, ..)| p == path).map(|(.., v)| v);
        assert_eq!(
            got,
            Some(&Some(want.to_string())),
            "`{path}` 의 처방이 `{want}` 가 아니다"
        );
    }

    let orphans: Vec<&str> = table
        .iter()
        .filter(|(p, _, via)| p.starts_with("component::") && via.is_none())
        .map(|(_, tok, _)| tok.as_str())
        .collect();
    assert!(
        orphans.is_empty(),
        "component 토큰 {} 개에 `&Theme` 접근자가 없다 — 생성기가 건너뛰었다(`cargo run -p \
         tasty-design-tokens --bin generate` 가 스킵 사유를 찍는다): {orphans:?}",
        orphans.len()
    );

    let mut pathless: Vec<&str> = table
        .iter()
        .filter(|(.., via)| via.is_none())
        .map(|(_, tok, _)| tok.as_str())
        .collect();
    pathless.sort_unstable();
    let mut listed: Vec<&str> = PATHLESS_LENGTH_TOKENS.iter().map(|(t, _)| *t).collect();
    listed.sort_unstable();
    assert_eq!(
        pathless, listed,
        "Theme 경로 없는 토큰 목록이 실제와 다르다. 경로가 생긴 항목은 제거하고 새로 경로가 없는 항목은 이유와 함께 등록한다."
    );
}

/// ThemeSizing에만 있는 필드를 Theme 경로로 잘못 안내하지 않는지 합성 입력으로 확인한다.
#[test]
fn a_table_field_missing_from_theme_is_not_a_path() {
    let fake = "pub struct ThemeSizing {\n    pub only_sizing: LogicalPx,\n    \
                pub spacing_xs: LogicalPx,\n}\n\npub struct Theme {\n    \
                pub spacing_xs: LogicalPx,\n}\n";
    let body = theme_struct_body(fake);
    let table = [
        ("semantic.fake-a", "only_sizing"),
        ("semantic.fake-b", "spacing_xs"),
    ];
    assert_eq!(
        semantic_theme_field("semantic.fake-a", &table, body),
        None,
        "`ThemeSizing` 에만 있는 필드를 `th.` 경로로 처방했다"
    );
    assert_eq!(
        semantic_theme_field("semantic.fake-b", &table, body),
        Some("spacing_xs")
    );
}

/// 경로 있음·경로 없음·import 해석 실패에 각각 올바른 조치를 안내해야 한다.
#[test]
fn the_prescription_names_the_token_path_not_a_same_value_one() {
    let table = theme_path_table();
    let found = length_const_prescription("x.rs", 1, "component::autocomplete::MAX_HEIGHT", &table);
    assert!(
        found.contains("`th.autocomplete_max_height()`") && !found.contains("같은 값의"),
        "경로가 있는 자리에 그 이름을 대지 않았다: {found}"
    );
    let pathless = length_const_prescription("x.rs", 1, "semantic::FIELD_WIDTH_RANGE", &table);
    assert!(
        pathless.contains("경로가 없다") && !pathless.contains("`th."),
        "경로가 없는 토큰에 경로를 지어냈다: {pathless}"
    );
    // 중첩 import를 실제 파서로 읽은 결과를 넘겨 이름으로 가져온 항목을 모듈 import로 오인하지 않는지 확인한다.
    let nested = concat!(
        "use tasty_design_tokens",
        "::generated::component::{fp::{GAP}, dag::EDGE};"
    );
    for (path, _) in generated_const_refs(&[nested]) {
        let broken = length_const_prescription("x.rs", 1, &path, &table);
        assert!(
            broken.contains("경로가 이어지지 않는다")
                && broken.contains("중첩 묶음")
                && broken.contains("전체 경로로 import"),
            "해소되지 않는 경로를 다른 결과로 분류했다: {broken}"
        );
    }
}

/// 한 겹의 묶음과 여러 줄 묶음은 이름별로 펼쳐야 한다.
#[test]
fn group_imports_expand_to_one_path_per_item() {
    let line = concat!(
        "use tasty_design_tokens",
        "::generated::component::fp::{ROW_HEIGHT, GAP as G, self};"
    );
    let refs = generated_const_refs(&[line]);
    assert_eq!(
        refs.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        vec![
            "component::fp::ROW_HEIGHT",
            "component::fp::GAP",
            "component::fp"
        ],
    );
    let folded = [
        concat!(
            "    use tasty_design_tokens",
            "::generated::component::fp::{"
        ),
        "        CRUMB_MAX_WIDTH, CRUMB_MIN_WIDTH,",
        "    };",
    ];
    assert_eq!(
        generated_const_refs(&folded),
        vec![
            ("component::fp::CRUMB_MAX_WIDTH".to_string(), 1),
            ("component::fp::CRUMB_MIN_WIDTH".to_string(), 1),
        ],
    );
    let nested = concat!(
        "use tasty_design_tokens",
        "::generated::component::{fp::{GAP}, dag::EDGE};"
    );
    assert_eq!(
        generated_const_refs(&[nested])
            .iter()
            .map(|(p, _)| p.as_str())
            .collect::<Vec<_>>(),
        vec!["component::"],
    );
}

#[test]
fn no_named_const_copies_a_radius_token() {
    let (sources, consts) = scan_sources();
    let mut violations = Vec::new();
    for (rel, contents) in &sources {
        let lines: Vec<&str> = contents.lines().collect();
        const_radius_violations(rel, &lines, &consts, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "반경 토큰 값을 복사한 명명 상수를 반경 인자에 사용했다. 같은 역할의 Theme 토큰을 쓴다. 토큰 범위 밖 값은 이름·사유를 둔 상수를 허용한다(docs/design/systems/theme.md):\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_named_const_copies_a_ui_font_token() {
    let (sources, consts) = scan_sources();
    let mut violations = Vec::new();
    for (rel, contents) in &sources {
        let lines: Vec<&str> = contents.lines().collect();
        const_font_violations(rel, &lines, &consts, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "폰트 토큰 값을 복사한 명명 상수를 폰트 인자에 사용했다. font_size_* 또는 역할별 Theme 접근자를 쓴다. 토큰 범위 밖 값은 이름·사유를 둔 상수를 허용한다(docs/design/systems/theme.md):\n{}",
        violations.join("\n")
    );
}

/// 인라인 리터럴 검사와 같은 루트를 읽는지 확인한다. 빈 파싱 결과도 실패시킨다.
#[test]
fn the_two_sister_guards_scan_the_same_roots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sister = root.join("crates/tasty-doc-guards/tests/design_token_adherence.rs");
    let src = std::fs::read_to_string(&sister)
        .unwrap_or_else(|e| panic!("자매 가드를 읽지 못했다 ({}): {e}", sister.display()));

    let theirs = sister_scan_roots(&src);
    assert!(
        !theirs.is_empty(),
        "인라인 리터럴 검사에서 SCAN_ROOTS를 읽지 못했다. 선언 형식과 sister_scan_roots를 확인한다."
    );

    let mine: Vec<String> = SCAN_ROOTS.iter().map(|s| (*s).to_string()).collect();
    let mut a = mine.clone();
    let mut b = theirs.clone();
    a.sort();
    b.sort();
    assert_eq!(
        a, b,
        "인라인 리터럴 검사와 명명 상수 검사의 스캔 루트가 다르다"
    );

    // 파일 단위 목록은 새 파일을 놓치므로 루트는 디렉터리여야 한다.
    for r in SCAN_ROOTS {
        assert!(
            !r.ends_with(".rs"),
            "스캔 루트에 개별 파일이 있다: {r}. 디렉토리로 쓸 것"
        );
    }
}

#[test]
fn unmapped_primitive_font_consts_say_so_in_their_name() {
    let (sources, consts) = scan_sources();
    let mut violations = Vec::new();
    for (rel, contents) in &sources {
        let lines: Vec<&str> = contents.lines().collect();
        primitive_name_violations(rel, &lines, &consts, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "UI semantic이 없는 primitive 폰트 상수 이름에 primitive와 값을 표시해야 한다. 예: ATTN_PRIMITIVE_12. semantic 토큰이 생기면 해당 토큰으로 옮긴다(docs/design/systems/theme.md):\n{}",
        violations.join("\n")
    );
}

#[cfg(test)]
mod discriminate {
    use super::*;

    #[test]
    fn the_scan_root_parser_ignores_quotes_inside_comments() {
        let src = "\
const SCAN_ROOTS: &[&str] = &[
    \"src\",
    // \"src/gfx\" 는 제외한다 — 이 따옴표는 루트가 아니다
    \"crates\", // \"tests\" 는 여기 없다
];
";
        assert_eq!(
            sister_scan_roots(src),
            vec!["src".to_string(), "crates".to_string()],
            "주석 안의 따옴표가 루트로 새어 들어왔다"
        );
    }

    #[test]
    fn the_radius_axis_strips_paths_and_checks_value_and_position() {
        let consts = vec![
            ("PILL_RADIUS".to_string(), 8.0_f32), // 토큰 값 = corner_radius_lg
            ("BOOT_CARD_CORNER_RADIUS".to_string(), 12.0), // 스케일 밖 — 허용
            ("SMALL_R".to_string(), 2.0),         // 토큰 값 = corner_radius_sm
        ];
        let check = |lines: &[&str]| {
            let mut out = Vec::new();
            const_radius_violations("f.rs", lines, &consts, &mut out);
            out
        };

        assert_eq!(check(&["    .corner_radius(PILL_RADIUS)"]).len(), 1);
        assert_eq!(
            check(&["    .corner_radius(tasty_ui_widgets::tokens::PILL_RADIUS)"]).len(),
            1,
            "모듈 경로가 붙은 상수를 찾지 못했다"
        );
        assert_eq!(check(&["    CornerRadius::same(SMALL_R)"]).len(), 1);
        assert_eq!(
            check(&["    .corner_radius(BOOT_CARD_CORNER_RADIUS)"]).len(),
            0
        );
        assert_eq!(check(&["    .size(PILL_RADIUS)"]).len(), 0);
        assert_eq!(check(&["    // .corner_radius(PILL_RADIUS)"]).len(), 0);
        assert_eq!(check(&["    .corner_radius(UNKNOWN_RADIUS)"]).len(), 0);
    }

    #[test]
    fn the_axis_is_value_and_position_not_name() {
        let consts = vec![
            ("BODY_FONT_SIZE".to_string(), 13.0_f32),
            ("OFF_SCALE_FONT_SIZE".to_string(), 10.5),
            ("SOMETHING_ELSE".to_string(), 13.0),
            ("LOADING_SPINNER_SIZE".to_string(), 14.0),
        ];
        let check = |lines: &[&str]| {
            let mut out = Vec::new();
            const_font_violations("f.rs", lines, &consts, &mut out);
            out
        };

        assert_eq!(check(&["    .size(BODY_FONT_SIZE),"]).len(), 1);
        assert_eq!(check(&["    .size(SOMETHING_ELSE),"]).len(), 1);
        assert_eq!(check(&["    .size(OFF_SCALE_FONT_SIZE),"]).len(), 0);
        assert_eq!(
            check(&[
                "    Spinner::new()",
                "        .size(LOADING_SPINNER_SIZE)",
                "        .color(c),",
            ])
            .len(),
            0
        );
        assert_eq!(
            check(&[
                "    Spinner::new().size(LOADING_SPINNER_SIZE).show(ui, th); ui.label(RichText::new(x).size(BODY_FONT_SIZE));"
            ])
            .len(),
            1
        );
        assert_eq!(
            check(&[
                "    Spinner::new().size(LOADING_SPINNER_SIZE).show(ui, th);",
                "    ui.label(RichText::new(x).size(BODY_FONT_SIZE));",
            ])
            .len(),
            1
        );
        assert_eq!(
            check(&["    egui::FontId::proportional(BODY_FONT_SIZE),"]).len(),
            1
        );
        assert_eq!(
            check(&["    egui::FontId::monospace(BODY_FONT_SIZE),"]).len(),
            1
        );
        assert_eq!(check(&["    // .size(BODY_FONT_SIZE)"]).len(), 0);
        assert_eq!(check(&["    .size(UNKNOWN_CONST),"]).len(), 0);
    }

    /// 뒤에 주석이 붙어 있어도 그 앞의 실제 호출은 검사해야 한다.
    #[test]
    fn the_comment_skip_is_line_start_only() {
        let consts = vec![("BODY_FONT_SIZE".to_string(), 13.0_f32)];
        let check = |l: &str| {
            let mut out = Vec::new();
            const_font_violations("f.rs", &[l], &consts, &mut out);
            out.len()
        };
        assert_eq!(check("    // .size(BODY_FONT_SIZE)"), 0);
        assert_eq!(check("    .size(BODY_FONT_SIZE), // 임시"), 1);
        assert_eq!(check("    .size(BODY_FONT_SIZE), /* 임시 */"), 1);
    }

    /// 지역 변수·인라인 리터럴·계산식은 이 검사 대상이 아니라는 한계를 확인한다.
    #[test]
    fn the_intended_false_negatives_stay_false_negative() {
        let consts = vec![("BODY_FONT_SIZE".to_string(), 13.0_f32)];
        let check = |l: &str| {
            let mut out = Vec::new();
            const_font_violations("f.rs", &[l], &consts, &mut out);
            out.len()
        };
        assert_eq!(check("    .size(body_font_size)"), 0);
        assert_eq!(check("    .size(13.0)"), 0);
        assert_eq!(check("    .size(BODY_FONT_SIZE * 1.0)"), 0);
    }

    #[test]
    fn the_primitive_name_rule_checks_value_first() {
        let consts = vec![
            ("FOO_SIZE".to_string(), 12.0_f32),
            ("BAR_PRIMITIVE_12".to_string(), 12.0),
            ("BAZ_SIZE".to_string(), 16.0),
            ("QUX_PRIMITIVE_16".to_string(), 16.0),
            ("OFF_SCALE".to_string(), 12.5),
            ("UI_TOKEN".to_string(), 13.0),
            ("SPIN".to_string(), 16.0),
        ];
        let check = |lines: &[&str]| {
            let mut out = Vec::new();
            primitive_name_violations("f.rs", lines, &consts, &mut out);
            out.len()
        };
        assert_eq!(check(&["    .size(FOO_SIZE)"]), 1);
        assert_eq!(check(&["    .size(BAR_PRIMITIVE_12)"]), 0);
        assert_eq!(check(&["    .size(BAZ_SIZE)"]), 1);
        assert_eq!(check(&["    .size(QUX_PRIMITIVE_16)"]), 0);
        assert_eq!(check(&["    .size(OFF_SCALE)"]), 0);
        assert_eq!(check(&["    .size(UI_TOKEN)"]), 0);
        assert_eq!(check(&["    Spinner::new().size(SPIN).show(ui, th);"]), 0);
        assert_eq!(
            check(&["    .size(QUX_PRIMITIVE_16)", "    .size(FOO_SIZE)"]),
            1
        );
    }

    /// CRLF에서도 같은 인자 이름을 읽는지 확인한다.
    #[test]
    fn crlf_checkout_reads_the_same() {
        let consts = vec![("BODY_FONT_SIZE".to_string(), 13.0_f32)];
        let src = "    .size(BODY_FONT_SIZE),\r\n    let x = 1;\r\n";
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        const_font_violations("f.rs", &lines, &consts, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn const_table_reads_both_shapes() {
        let mut out = Vec::new();
        collect_numeric_consts(
            &[
                "const A: f32 = 13.0;",
                "pub const B: LogicalPx = LogicalPx(11.0);",
                "pub(crate) const C: f32 = 9.5;",
                "const D: f32 = SOMETHING * 2.0;",
                "let e = 13.0;",
            ],
            &mut out,
        );
        assert_eq!(
            out,
            vec![
                ("A".to_string(), 13.0),
                ("B".to_string(), 11.0),
                ("C".to_string(), 9.5)
            ]
        );
    }
}

/// UI 폰트 상한의 승인된 정책 예외. 브랜드 자산처럼 규칙 범위 밖인 항목을 기록한다.
/// 현재 검출 결과가 없어도 범위 밖이라는 근거가 유효하면 유지한다.
/// 새 예외는 먼저 규칙에 맞게 고칠 수 있는지 검토하고 문서에 승인 근거를 남긴다.
const OVER_CAP_SANCTIONED: &[(&str, &str)] = &[];

/// 상한을 넘지만 디자인 결정을 기다리는 항목. 기다리는 결정을 기록한다.
/// 위반이 없어지면 항목과 예산을 함께 줄이며 승인된 정책 예외와 구별한다.
const OVER_CAP_PENDING: &[(&str, &str, &str)] = &[];

/// 대기 목록이 늘거나 줄면 예산도 함께 검토하도록 개수를 일치시킨다.
const OVER_CAP_PENDING_BUDGET: usize = 0;

/// UI 폰트 상한은 SIZING.font_size_max에서 가져온다. 렌더링이 아닌 비교 전용 길이값이다.
const UI_FONT_SIZE_CAP: LogicalPx = tasty_type_appearance::theme::SIZING.font_size_max;

/// 상수 표가 비어 상한 초과 0개로 통과하지 않게 한다.
const MIN_SCANNED_CONSTS: usize = 100;

/// 승인된 정책 예외는 범위 밖 근거로 유지하고, 해결된 대기 항목은 제거한다.
#[test]
fn no_named_font_const_exceeds_the_ui_font_size_cap() {
    assert_eq!(
        OVER_CAP_PENDING.len(),
        OVER_CAP_PENDING_BUDGET,
        "대기 항목 수가 예산 {OVER_CAP_PENDING_BUDGET}과 다르다. 해결된 항목은 예산도 함께 줄인다. 새 항목은 디자인 판단이 필요한 이유를 검토해 등록한다."
    );
    for (rel, name, awaiting) in OVER_CAP_PENDING {
        assert!(
            !awaiting.trim().is_empty(),
            "{name}({rel})이 기다리는 디자인 결정이 적혀 있지 않다"
        );
    }

    let (sources, consts) = scan_sources();
    let sanctioned = |rel: &str, name: &str| {
        OVER_CAP_SANCTIONED
            .iter()
            .any(|(r, n)| *r == rel && *n == name)
    };
    let mut over_cap: Vec<(String, String, f32)> = Vec::new();
    for (rel, contents) in &sources {
        let lines: Vec<&str> = contents.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for (arg, at) in font_call_args(line) {
                if spinner_receiver(&lines, i, &line[..at]) {
                    continue;
                }
                let Some((_, value)) = consts.iter().find(|(n, _)| *n == arg) else {
                    continue;
                };
                if *value > UI_FONT_SIZE_CAP.value() {
                    over_cap.push((rel.clone(), arg, *value));
                }
            }
        }
    }

    assert!(
        consts.len() >= MIN_SCANNED_CONSTS,
        "상수를 {}개만 읽었다(하한 {MIN_SCANNED_CONSTS}). 빈 표로 검사하지 않도록 수집을 확인한다.",
        consts.len()
    );

    let mut unlisted: Vec<String> = Vec::new();
    let mut pending_seen: Vec<(String, String)> = Vec::new();
    for (rel, name, value) in &over_cap {
        if sanctioned(rel, name) {
            continue;
        }
        if OVER_CAP_PENDING
            .iter()
            .any(|(r, n, _)| *r == rel && *n == name)
        {
            pending_seen.push((rel.clone(), name.clone()));
            continue;
        }
        unlisted.push(format!(
            "  {rel} — `{name}` = {value} > {}",
            UI_FONT_SIZE_CAP.value()
        ));
    }
    unlisted.sort();
    unlisted.dedup();
    assert!(
        unlisted.is_empty(),
        "UI 폰트 상한 {}px를 넘는 상수다:\n{}\n승인된 예외는 근거를 문서화해 OVER_CAP_SANCTIONED에, 디자인 결정을 기다리면 그 내용을 OVER_CAP_PENDING에 기록한다.",
        UI_FONT_SIZE_CAP.value(),
        unlisted.join("\n")
    );

    // 정책 예외와 달리 대기 항목은 더 이상 상한을 넘지 않으면 제거한다.
    let gone: Vec<String> = OVER_CAP_PENDING
        .iter()
        .filter(|(r, n, _)| !pending_seen.iter().any(|(sr, sn)| sr == r && sn == n))
        .map(|(r, n, awaiting)| format!("  {r} — `{n}` (기다리던 것: {awaiting})"))
        .collect();
    assert!(
        gone.is_empty(),
        "대기 항목이 더 이상 폰트 상한을 넘지 않는다. 해결된 항목을 목록에서 제거한다:\n{}",
        gone.join("\n")
    );
}

/// 색 배율과 알파의 익명 숫자 인자를 검사한다.
const COLOR_COEFF_CALLS: &[&str] = &[".gamma_multiply(", ".with_alpha("];

/// 플러그인의 색 처리도 포함하도록 src와 crates를 모두 검사한다.
const COLOR_COEFF_SCAN_ROOTS: &[&str] = &["src", "crates"];

const MIN_COLOR_COEFF_SCANNED_FILES: usize = 1000;

/// 숫자 인자를 받은 색 호출을 찾는다. 줄 주석과 // 뒤에 공백이 있는 후행 주석만 제외한다.
fn color_coeff_literals(line: &str) -> Vec<(&'static str, String)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return Vec::new();
    }
    let code = match line.find("// ") {
        Some(at) => &line[..at],
        None => line,
    };
    let mut hits = Vec::new();
    for call in COLOR_COEFF_CALLS {
        let mut cursor = 0usize;
        while let Some(rel) = code[cursor..].find(call) {
            cursor += rel + call.len();
            let rest = &code[cursor..];
            let Some(end) = rest.find([',', ')']) else {
                continue;
            };
            let arg = rest[..end].trim();
            if !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit() || c == '.') {
                hits.push((*call, arg.to_string()));
            }
        }
    }
    hits
}

/// 모든 색 계수에 대응하는 토큰이 있는 것은 아니므로 이름과 사유를 요구한다(docs/design/systems/theme.md).
/// 값을 통합하거나 바꾸는 것은 별도의 디자인 결정이다.
#[test]
fn no_color_derivation_coefficient_is_an_anonymous_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for target in COLOR_COEFF_SCAN_ROOTS {
        let path = root.join(target);
        let before = files.len();
        gather_rs_files(&path, &mut files);
        assert!(
            files.len() > before,
            "스캔 루트 `{target}` 에서 .rs 파일을 하나도 찾지 못했다"
        );
    }
    assert!(
        files.len() >= MIN_COLOR_COEFF_SCANNED_FILES,
        "Rust 파일을 {}개만 읽었다(하한 {MIN_COLOR_COEFF_SCANNED_FILES}). 경로와 수집 범위를 확인한다.",
        files.len()
    );

    let mut violations = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for (no, line) in text.lines().enumerate() {
            for (call, arg) in color_coeff_literals(line) {
                violations.push(format!("{rel}:{} {call}{arg})", no + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "색 계수의 익명 숫자가 {}곳 있다. 값은 그대로 두고 사유와 이름을 붙인다. 값을 통합하거나 바꾸려면 별도의 디자인 결정이 필요하다:\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// 합성 호출도 금지 패턴 자체를 소스에 담지 않도록 조립한다.
#[test]
fn the_color_coeff_scan_sees_both_forms_and_ignores_the_named_one() {
    let gamma = COLOR_COEFF_CALLS[0];
    let alpha = COLOR_COEFF_CALLS[1];

    let bad_gamma = format!("        .fill(c{gamma}0.12))");
    assert_eq!(
        color_coeff_literals(&bad_gamma),
        vec![(COLOR_COEFF_CALLS[0], "0.12".to_string())],
        "배율 리터럴을 못 봤다"
    );

    let bad_alpha = format!("    let x = c{alpha}180).to_egui();");
    assert_eq!(
        color_coeff_literals(&bad_alpha),
        vec![(COLOR_COEFF_CALLS[1], "180".to_string())],
        "알파 리터럴을 못 봤다"
    );

    let good = format!("        .fill(c{gamma}WARN_BADGE_FILL_OPACITY))");
    assert!(
        color_coeff_literals(&good).is_empty(),
        "허용한 명명 상수를 위반으로 분류했다"
    );

    let line_comment = format!("    // 예전에는 c{gamma}0.4) 였다");
    assert!(
        color_coeff_literals(&line_comment).is_empty(),
        "줄 주석을 판정했다"
    );
    let trailing = format!("    let x = c{alpha}NAMED); // c{gamma}0.4) 였다");
    assert!(
        color_coeff_literals(&trailing).is_empty(),
        "뒤에 달린 주석을 판정했다"
    );
}

/// 점을 그리는 세 호출의 반지름을 검사한다.
const DOT_RADIUS_CALLS: &[&str] = &["circle_filled(", "circle_stroke(", "circle("];

/// 점 치수는 길이이므로 전역 zoom을 쓰는 갤러리를 제외한다(docs/concepts/typed-length.md).
/// 무차원 색 계수 검사는 이 예외를 적용하지 않는다.
fn dot_scan_roots() -> Vec<&'static str> {
    SCAN_ROOTS
        .iter()
        .copied()
        .filter(|r| !r.starts_with(ZOOM_CONST_EXCLUDED_ROOT))
        .collect()
}

const MIN_DOT_SCANNED_FILES: usize = 150;

/// 점 지름 토큰과 비교할 기준값.
const DOT_SIZE_TOKEN_VALUE: LogicalPx = LogicalPx(8.0);

const DOT_CONST_NAME_MARK: &str = "DOT";

/// 점 호출의 두 번째 인자를 읽는다. 중첩 식의 쉼표에서 잘리지 않도록 괄호 깊이를 센다.
fn dot_radius_literal(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for call in DOT_RADIUS_CALLS {
        let mut cursor = 0usize;
        while let Some(rel) = line[cursor..].find(call) {
            cursor += rel + call.len();
            let mut depth = 0i32;
            let mut args: Vec<String> = Vec::new();
            let mut cur = String::new();
            for ch in line[cursor..].chars() {
                match ch {
                    '(' | '[' => depth += 1,
                    ')' | ']' if depth == 0 => {
                        args.push(cur.clone());
                        break;
                    }
                    ')' | ']' => depth -= 1,
                    ',' if depth == 0 => {
                        args.push(cur.clone());
                        cur.clear();
                        continue;
                    }
                    _ => {}
                }
                cur.push(ch);
            }
            if let Some(radius) = args.get(1) {
                let r = radius.trim();
                if numeric_literal(r).is_some() {
                    hits.push(r.to_string());
                }
            }
        }
    }
    hits
}

#[test]
fn no_dot_radius_is_an_anonymous_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for target in dot_scan_roots() {
        let path = root.join(target);
        let before = files.len();
        gather_rs_files(&path, &mut files);
        assert!(
            files.len() > before,
            "스캔 루트 `{target}` 에서 .rs 파일을 하나도 찾지 못했다"
        );
    }
    assert!(
        files.len() >= MIN_DOT_SCANNED_FILES,
        "스캔한 .rs 가 {} 개뿐이다 — 하한 {MIN_DOT_SCANNED_FILES}",
        files.len()
    );

    let mut violations = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for (no, line) in text.lines().enumerate() {
            for r in dot_radius_literal(line) {
                violations.push(format!("{rel}:{} 반지름 {r}", no + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "점 반지름이 익명 리터럴이다 ({} 자리). 토큰(`th.status_dot_size`)이 맞으면 \
         그것을, 스케일 밖 값이면 사유를 적은 명명 const 를 써라(docs/design/systems/theme.md):\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// 점 토큰 값을 복사한 DOT 이름 상수를 찾는다. 점은 일반 산술로도 소비돼 호출만으로 구분하기 어렵다.
/// 다른 이름의 상수를 점에 쓰면 놓칠 수 있다.
#[test]
fn no_dot_named_const_copies_the_status_dot_token() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for target in dot_scan_roots() {
        gather_rs_files(&root.join(target), &mut files);
    }
    assert!(files.len() >= MIN_DOT_SCANNED_FILES, "모수 미달");

    let mut violations = Vec::new();
    let mut dot_consts = 0usize;
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let mut consts = Vec::new();
        collect_numeric_consts(&lines, &mut consts);
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for (name, value) in consts {
            if !name.contains(DOT_CONST_NAME_MARK) {
                continue;
            }
            dot_consts += 1;
            if (value - DOT_SIZE_TOKEN_VALUE.value()).abs() < f32::EPSILON {
                violations.push(format!("{rel}: {name} = {value}"));
            }
        }
    }

    assert!(
        dot_consts > 0,
        "DOT 이름 상수를 찾지 못했다. {DOT_CONST_NAME_MARK} 표지와 상수 파싱을 확인한다."
    );
    assert!(
        violations.is_empty(),
        "점 토큰 값{}을 복사한 DOT 이름 상수다. 같은 역할의 th.status_dot_size를 사용한다. 토큰 범위 밖 값은 이름·사유를 둔 상수를 허용한다(docs/design/systems/theme.md):\n{}",
        DOT_SIZE_TOKEN_VALUE.value(),
        violations.join("\n")
    );
}
