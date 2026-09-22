//! 시각 토큰 드리프트 가드 — **UI 폰트 토큰 값을 복사한 이름 상수**를 막는다.
//!
//! # 왜 `tests/` 가 아니라 여기인가 (관례를 깬 이유)
//!
//! 소스 스캔 가드의 관례 자리는 `tests/*.rs` 이고 자매 가드
//! (`crates/tasty-doc-guards/tests/design_token_adherence.rs`)도 거기 있다. 이 파일만 본체 crate 안에 있는
//! 이유는 **자동으로 실행되는 잡의 수가 자리마다 다르기** 때문이다. 직접 재서 얻은
//! 값만 적는다(채널 표 전체는 여기 옮겨 적지 않는다 — 아래 정본 참조):
//!
//! ```text
//! 이 자리(본체 crate 의 #[cfg(test)] 모듈)   자동 실행 잡 2 개
//! tests/*.rs (자매 가드의 자리)              자동 실행 잡 1 개
//! ```
//!
//! 차이는 **한 잡**이다. 워크스페이스 전체를 도는 잡은 통합 테스트도 실행하므로 양쪽을
//! 다 보지만, `--lib --bins` 로 좁혀 도는 잡은 `tests/*.rs` 를 **타깃으로 만들지도
//! 않는다.** 그래서 이 파일을 `tests/` 로 "관례에 맞춰" 옮기면 **자동 실행이 사라지는
//! 것이 아니라 두 잡 중 하나로 준다.** 소스 스캔 가드는 실행되지 않으면 존재하지 않는
//! 것과 같아서(본체가 스캔이라 컴파일만으로는 아무것도 검사되지 않는다) 그 한 잡의
//! 차이를 기꺼이 산다 — **관례를 깨는 근거는 "0 대 1" 이 아니라 "1 대 2" 다.**
//!
//! > **이 문단의 만료 조건**(사람이 읽는 조건이 아니라 판정 가능한 형태로 적는다):
//! > `.github/workflows/crossplatform-check.yml` 의 단위 테스트 스텝이 `--lib --bins`
//! > 를 **포함하고** 헤드리스 테스트 스텝이 **포함하지 않을** 때만 위 "1 대 2" 가 참이다.
//! > 두 스텝이 같은 범위로 수렴하면 차이가 0 이 되고 이 파일이 여기 있을 이유도 사라진다.
//! > 한때 반대 방향으로 틀렸던 적이 있다 — 헤드리스가 `--lib --bins` 로 좁혀져 있던
//! > 시절의 서술("`tests/` 는 자동 실행 채널이 없다")을 그 스텝이 넓어진 뒤에도 들고
//! > 있었다. **근거를 적는 것이 근거의 유효성을 지켜 주지 않는다.**
//!
//! 그리고 그 잡들이 **초록인지는 별개로 확인해야 한다** — 채널이 있다는 것은 채널이
//! 건강하다는 뜻이 아니다. 한 층 더 있다: **잡이 애초에 발화하는가** 도 따로다(트리거가
//! 이 레포에서 일어나지 않는 이벤트면 그 잡은 active 로 보이면서 실행 이력이 0 이다).
//! 트리거·러너를 포함한 채널 정본은
//! [`docs/dev-guide/ci-gates.md`](../docs/dev-guide/ci-gates.md) 하나다.
//!
//! (`--lib --bins` 로 좁혀 도는 잡은 Windows 러너라 CRLF 체크아웃을 계산에 넣었다 —
//! [`discriminate::crlf_checkout_reads_the_same`].)
//!
//! # 무엇을 잡나
//!
//! `const BODY_FONT_SIZE: f32 = 13.0;` 처럼 **UI 폰트 스케일과 값이 같은 이름 상수**가
//! 폰트 자리(`.size(` · `FontId::proportional/monospace/new(`)에 오는 것을 잡는다.
//!
//! 자매 가드는 인라인 리터럴(`.size(13.0)`)만 막고 **명명 const 경유는 설계상 허용**
//! 한다 — 그게 스케일 **밖** 값(9.5 · 10.5 · 12.5 …)의 권장 해결책이기 때문이다
//! ([ADR-0126](../docs/adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md)).
//! 그런데 그 허용은 **값이 스케일 밖일 때만** 정당하다. 값이 토큰과 같으면 그 const 는
//! 토큰의 복사본이고, 복사본은 `ui_zoom` 을 타지 않아 zoom≠1 에서 조용히 갈라진다.
//! ADR-0126 이 그 세 자리를 우연히 감싸고 있었고, 이 가드가 그 틈을 닫는다.
//!
//! # 한계 (자매 가드의 "가드가 막지 못하는 것" 과 같은 성격)
//!
//! - 값 해석은 **`const NAME: f32 = <숫자>;` / `= LogicalPx(<숫자>)` 리터럴만** 따라간다.
//!   계산식(`13.0 * 1.0`)·다른 const 참조·`let` 변수는 못 따라간다.
//! - const 표는 **스캔 루트 안에서만** 모은다. 루트 밖에 정의된 이름은 판정되지 않는다.
//! - 폰트/위젯 판별은 수신자 look-back 이다 — `Spinner::new()` 가 같은 문(`;` 이전)에
//!   있으면 지름으로 보고 건너뛴다. 그 밖의 `.size(` 는 폰트로 본다.

use std::path::{Path, PathBuf};

use tasty_type_geometry::length::LogicalPx;

/// 스캔 대상 (repo-relative). 자매 가드 `crates/tasty-doc-guards/tests/design_token_adherence.rs` 의
/// `SCAN_ROOTS` 와 같은 집합이다 — 같은 축을 보므로 갈라지면 안 된다.
///
/// **그 "갈라지면 안 된다" 는 오래 주석으로만 있었고 실제로 갈라졌다.** 자매 쪽이
/// `src/gfx/gpu` 를 디렉토리로 넓히는 동안 이쪽은 `shell_setup.rs` 한 파일로 남았다.
/// 지금은 [`the_two_sister_guards_scan_the_same_roots`] 가 그 불변식을 판정한다.
///
/// 개별 `.rs` 파일을 루트로 등재하지 않는다 — 그 디렉토리에 나중에 생기는 파일이 기본
/// 제외가 되고, 그 누락은 아무 신호도 내지 않는다
/// (`docs/adr/0133-guard-scan-population-is-pinned-not-enumerated.md`).
const SCAN_ROOTS: &[&str] = &[
    "src/view",
    "src/adapters/ui",
    "src/gfx/gpu",
    "crates/tasty-gallery/src",
    "crates/tasty-ui-widgets/src",
    "crates/tasty-egui-theme/src",
];

/// 자매 가드의 소스에서 `const SCAN_ROOTS` 블록의 문자열 리터럴을 뽑는다.
///
/// 두 가드는 각각 lib 유닛과 통합 타깃이라 상수를 **공유할 수 없다**(통합 테스트의
/// 아이템은 본체 crate 에서 안 보인다). 그래서 값을 나누는 대신 소스를 읽어 대조한다 —
/// 이 파일은 이미 같은 방식으로 소스를 스캔하므로 새 의존이 아니다.
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
                // 주석 — 줄의 나머지는 코드가 아니다. **줄 앞이든 뒤든 자른다.**
                // 자매 파일의 이 블록에는 실제로 자유 주석이 있고, 거기에 경로를
                // 따옴표로 인용하면(`// "src/gfx" 는 제외`) 그것이 루트로 뽑혀
                // `assert_eq!` 가 **거짓 빨강**을 낸다. 방향은 안전하지만 메시지가
                // "두 자매 가드의 스캔 루트가 갈라졌다" 라 원인을 잘못 가리킨다.
                break;
            }
            i += 1;
        }
    }
    out
}

/// UI 폰트 스케일의 값 — `font_size_micro`(10) · `caption`(11) · `body`(13) ·
/// `heading`(13) · `max`(14). 이 다섯이 UI 스케일 전부다
/// (`docs/design/systems/theme.md` "UI 폰트 스케일").
///
/// 콘텐츠 폰트(`font_size_term_sm`(12) · `term`(14) · `term_lg`(16) · `prose_h1`(20))는
/// 스케일이 다르지만 `term` 은 값이 14 라 `max` 와 겹친다. 겹쳐도 판정은 옳다 —
/// 어느 쪽 의도든 **토큰을 쓰라**가 답이고, 어느 토큰인지는 고치는 사람이 정한다.
const UI_FONT_TOKEN_VALUES: &[f32] = &[10.0, 11.0, 13.0, 14.0];

/// **UI semantic 이 배정되지 않은 DTCG primitive 폰트 값.** ADR-0126 은 이 자리를
/// 명명 const 로 두는 것을 허용하되 **이름에 primitive 임을 남기라**고 요구한다 —
/// 호출 자리에서 "토큰인가 미배정 primitive 인가" 가 이름만으로 갈리게 하려는 것이다.
///
/// 값은 ADR 본문이 명시한 둘(12 · 16)만 본다. 다른 primitive(17 · 20)까지 넓히지 않은
/// 이유는 그것들이 semantic 을 갖고 있어(brand-wordmark · prose) 같은 처지가 아니기
/// 때문이다 — 규칙이 쓰여 있는 범위만 강제한다.
const UNMAPPED_PRIMITIVE_FONT_VALUES: &[f32] = &[12.0, 16.0];

/// 폰트 크기를 받는 호출 형태. 접두 뒤 첫 인자가 크기다.
const FONT_CALLS: &[&str] = &[
    ".size(",
    "FontId::proportional(",
    "FontId::monospace(",
    "FontId::new(",
];

/// **UI 반경 토큰의 값** — `SIZING.corner_radius_sm`(2) · `corner_radius`(4) ·
/// `corner_radius_lg`(8). 주석이 아니라 `theme.rs:410-412` 의 정의에서 온 값이다.
///
/// 이 축이 폰트 축보다 나쁜 이유: `corner_radius*` 는 `Theme` 에서 `zoomed()` 를
/// **타고**(`theme.rs` 의 `corner_radius: zoomed(SIZING.corner_radius)`) 명명 const 는
/// 안 탄다. 그래서 반경 사본은 배율 0.85 / 1.2 에서 토큰과 **다른 픽셀로 그려진다** —
/// 폰트 사본과 달리 시각적 회귀가 실재한다.
const UI_RADIUS_TOKEN_VALUES: &[f32] = &[2.0, 4.0, 8.0];

/// 반경을 받는 호출 형태. 자매 가드(`crates/tasty-doc-guards/tests/design_token_adherence.rs`)의
/// `FORBIDDEN_PREFIXES` 와 같은 둘이다 — 그쪽은 **리터럴**을, 여기는 **토큰 값을 복사한
/// 명명 const** 를 막는다. 두 판정이 합쳐져야 이 축의 우회로가 닫힌다.
const RADIUS_CALLS: &[&str] = &[".corner_radius(", "CornerRadius::same("];

/// `const NAME: f32 = 13.0;` / `const NAME: LogicalPx = LogicalPx(13.0);` 를 모은다.
///
/// 이름이 파일을 넘어 참조되므로(예 `CENTER_GLYPH_SIZE` 는 `tasty-ui-widgets` 에
/// 있고 host popup 이 쓴다) 표는 스캔 루트 **전체에서 하나로** 만든다.
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

/// 숫자 리터럴인가 — `13` · `13.0` · `13.0f32`.
fn numeric_literal(tok: &str) -> Option<f32> {
    let t = tok.trim().trim_end_matches("f32").trim_end_matches("f64");
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<f32>().ok()
}

/// 대문자 스네이크 이름인가 — 상수 참조의 구조적 표지.
fn is_screaming_snake(tok: &str) -> bool {
    tok.len() >= 3
        && tok.starts_with(|c: char| c.is_ascii_uppercase())
        && tok
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `.size(` 의 수신자가 스피너인가 — **문 단위** 판정이다. 호출 앞 텍스트를 뒤로
/// 이어붙여 **마지막 `;` 이후**만 본다: 앞 문이 스피너였다는 이유로 뒤 문이 면제되면
/// 안 된다. 스피너의 `.size()` 는 폰트가 아니라 위젯 지름이라 이 축의 대상이 아니다.
fn spinner_receiver(lines: &[&str], at: usize, before_call: &str) -> bool {
    // 한 문이 12줄을 넘게 이어지는 형태는 레포에 없다 — 비용을 여기서 끊는다.
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

/// 한 줄에서 **폰트 자리에 온 명명 상수**를 전부 뽑는다 — `(이름, 호출 시작 위치)`.
///
/// 파일 순회·경로 처리와 분리한 **순수 함수**라, 판정을 합성 문자열로 찌를 수 있다.
/// 두 판정기(`const_font_violations` · `primitive_name_violations`)가 이것을 공유하므로
/// "어디까지가 폰트 자리인가" 의 정의가 한 곳에만 있다.
///
/// 한 줄에 폰트 호출이 둘 이상 올 수 있다 — 첫 개만 보면 뒤 호출이 앞 호출의 판정
/// (특히 스피너 면제)에 묻힌다. 변이로 확인한 실제 누락이라 커서로 전부 훑는다.
fn font_call_args(line: &str) -> Vec<(String, usize)> {
    call_args(line, FONT_CALLS)
}

/// 인자 추출이 **`LogicalPx` 형태를 본다** — 이 축의 가드가 한 번 그것을 못 봤다.
///
/// `call_args` 는 호출 뒤 첫 `,`/`)` 까지를 인자로 끊는데, `.size(NAME.value())` 에서
/// 첫 `)` 는 `.value()` 의 것이다. 그래서 인자가 `NAME.value(` 로 잘려 이름 검사에서
/// 떨어졌고, **가드는 CLAUDE.md 가 금지하는 bare `f32` 만 보고 요구하는 `LogicalPx` 는
/// 못 보고 있었다.** 이름과 값이 같고 형태만 다른 변이 둘로 갈랐다: bare 는 빨강,
/// `.value()` 는 초록이었다.
///
/// 아래는 그 회귀를 형태로 고정한다 — 양극을 함께 둬서 "둘 다 못 본다" 로 통과하지
/// 않게 한다.
#[test]
fn the_font_arg_scan_sees_the_logical_px_form() {
    let wrapped = font_call_args(".size(ADD_PREVIEW_NAME_PRIMITIVE_16.value())");
    assert_eq!(
        wrapped.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["ADD_PREVIEW_NAME_PRIMITIVE_16"],
        "`LogicalPx` const 를 `.value()` 로 넘긴 자리를 못 봤다 — 이 레포의 길이 상수는 \
         대부분 이 형태다(CLAUDE.md 길이 타입 필수)"
    );

    let bare = font_call_args(".size(SEGMENT_BADGE_SIZE)");
    assert_eq!(
        bare.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["SEGMENT_BADGE_SIZE"],
        "bare 형태를 못 봤다 — 벗기기를 넣으면서 원래 보던 것을 잃었다"
    );

    // 비영 대조 — 상수가 아닌 인자는 안 잡아야 한다(위 둘이 "다 잡는다" 로 통과하는 것을 막는다).
    assert!(
        font_call_args(".size(th.font_size_body.value())").is_empty(),
        "토큰 접근자를 명명 상수로 잡았다 — 이 가드의 대상이 아니다"
    );
}

/// 주어진 호출 형태들의 **첫 인자가 명명 상수인** 자리를 모은다.
///
/// 경로 수식을 벗긴다 — `tasty_ui_widgets::tokens::BOOT_CARD_CORNER_RADIUS` 는
/// `BOOT_CARD_CORNER_RADIUS` 로 본다. 벗기지 않으면 **크레이트를 넘어 온 const 를
/// 통째로 놓친다**: 반경 호출자리의 명명 const 는 다수가 이 형태다(실측). 상수 표
/// (`collect_numeric_consts`)의 키가 벌거벗은 이름이라 마디를 맞춰야 조인된다.
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
            // `LogicalPx` const 는 `.value()` 로 벗겨서 넘긴다 — 그 형태가 이 레포의
            // **필수** 형태다(CLAUDE.md 길이 타입). 벗기지 않으면 인자가 `NAME.value(`
            // 로 잘려 이름 검사에서 떨어지고, 가드는 금지된 bare f32 만 보게 된다.
            let arg = arg.strip_suffix(".value(").unwrap_or(arg).trim();
            if is_screaming_snake(arg) {
                hits.push((arg.to_string(), at));
            }
        }
    }
    hits
}

/// 한 파일의 위반. `consts` 는 스캔 루트 전체에서 모은 이름→값 표다.
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

/// 미배정 primitive 값을 가진 폰트 const 가 **이름에 그 사실을 담고 있는가**.
///
/// 이 축만은 판별이 **이름**이다 — 규칙 자체가 이름에 대한 것이기 때문이다(ADR-0126).
/// 다른 곳에서 "면제는 이름이 아니라 구조로" 를 지키는 것과 모순이 아니다: 저긴 *무엇을
/// 빼줄지*를 이름으로 정하지 말라는 것이고, 여긴 *이름이 규칙의 대상*이다.
///
/// 면제는 없다. 위반 자리를 먼저 전부 이름에 맞춘 뒤 이 가드를 넣었기 때문이다 —
/// 순서가 반대면 allowlist 를 부풀리게 된다.
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
    let entries = std::fs::read_dir(path).unwrap_or_else(|e| {
        panic!(
            "스캔 대상 디렉토리를 읽을 수 없다: {} — {e}. 조용히 건너뛰면 가드가 \
             아무것도 검사하지 않은 채 통과한다.",
            path.display()
        )
    });
    for entry in entries.flatten() {
        gather_rs_files(&entry.path(), out);
    }
}

/// 스캔 하한 — 이 아래로 떨어지면 경로가 틀렸거나 읽기에 실패한 것이다. 하한이
/// 없으면 스캔 대상 0개인 가드가 **초록으로** 통과한다.
const MIN_SCANNED_FILES: usize = 200;

/// 스캔 루트를 읽어 `(파일 목록, 이름→값 표)` 를 만든다. 두 테스트가 공유한다.
///
/// 이름은 파일을 넘어 참조되므로 표는 **루트 전체에서 하나로** 모은 뒤에야 판정할 수
/// 있다 — 그래서 읽기와 판정이 두 단계다.
fn scan_sources() -> (Vec<(String, String)>, Vec<(String, f32)>) {
    // `CARGO_MANIFEST_DIR` 은 루트 패키지의 것이라 레포 루트다 — 크레이트 밖으로
    // 올라가는 상대경로(`../../..`)가 필요 없다.
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
        "스캔한 파일이 {}개뿐이다(하한 {MIN_SCANNED_FILES}) — 경로가 바뀌었을 것이다",
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

/// 반경 자리에 온 명명 상수의 값이 반경 토큰 값이면 위반이다.
///
/// 폰트 쪽 `const_font_violations` 와 같은 축(**값 × 자리**)이고, 다른 점은 예외가
/// 없다는 것이다 — 폰트에는 스피너 수신자 예외가 있지만 반경에는 그런 자리가 없다.
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

/// 이 축의 스캔 모수는 `SCAN_ROOTS` 중 **갤러리를 뺀 전부**다.
///
/// 갤러리는 `ctx.set_zoom_factor(ui_scale)` 로 egui 전역에 배율을 걸어 생성 const 값도
/// 함께 커지므로 거기서는 결함이 아니다(`docs/adr/0135-…`). 넣으면 결함 아닌 자리가
/// 위반이 되어 allowlist 만 불어난다.
///
/// **제외를 빼는 쪽으로 적는 이유**: 한때 이 자리는 `"src/"` 로 **남기는 쪽**을 적었는데,
/// 사유는 갤러리 하나인데 술어는 셋을 뺐다 — `crates/tasty-ui-widgets/src` 와
/// `crates/tasty-egui-theme/src` 가 경로 모양 때문에 함께 빠졌다. 둘은 갤러리가 아니라
/// **본체 UI** 다(본체가 widgets 를 쓰는 파일이 수십 개다). 거기서 생성 const 를 직접 읽으면
/// 본체에서는 배율을 안 타고 갤러리에서는 타는, 정확히 이 축의 결함이 된다.
///
/// 오늘 그 자리가 비어 있는 것은 가드 덕이 아니다 — 두 크레이트가 `tasty-design-tokens`
/// 에 **의존하지 않아서** 컴파일이 막는다. 의존이 하루 생기면 그 보호는 신호 없이
/// 사라지므로, 사유가 하나면 제외도 하나로 적는다.
const ZOOM_CONST_EXCLUDED_ROOT: &str = "crates/tasty-gallery/";

/// 아래 셋은 **거짓 초록 하한**이다. 표가 비거나 코퍼스가 비면 위반이 0 이 되어 가드가
/// 조용히 통과한다 — "위반 0" 은 모수가 비영일 때만 뜻이 있다.
const MIN_GENERATED_LOGICAL_CONSTS: usize = 200;
const MIN_GENERATED_OTHER_CONSTS: usize = 40;
/// 갤러리를 뺀 코퍼스의 하한. 갤러리 **한 쪽만** 남는 뒤집힌 필터는 이 하한 아래로
/// 떨어지므로 여기서 걸린다 — 하한이 "코퍼스가 비었나" 와 "필터가 뒤집혔나" 를 함께 본다.
///
/// 다만 하한은 **어디를 보는지**는 못 본다. 예전 술어(`src/` 만)는 이 하한을 넉넉히
/// 넘기면서 본체 UI 크레이트 둘을 통째로 놓쳤다 — 그 형태는 아래 루트 도달 단언이 잡는다.
const MIN_ZOOM_CONST_SCANNED_FILES: usize = 150;

/// 생성 DTCG 상수를 **타입으로** 가른다 — `(LogicalPx 인 것, 아닌 것)`.
///
/// 타입이 축을 가른다: `LogicalPx` 는 길이라 `ui_scale` 배율 축이고, `f32`(불투명도 ·
/// 가중치 · 지속시간)는 무차원이라 배율과 무관하다. 그래서 전자만 위반이다.
fn generated_const_types() -> (Vec<String>, Vec<String>) {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/tasty-design-tokens/src/generated");
    let (mut logical, mut other) = (Vec::new(), Vec::new());
    for file in ["primitive.rs", "semantic.rs", "component.rs"] {
        let text = std::fs::read_to_string(dir.join(file)).unwrap_or_else(|e| {
            panic!(
                "생성 토큰 파일을 읽을 수 없다: {file} — {e}. 조용히 건너뛰면 이름 표가 \
                 비고, 빈 표로는 위반이 0 이 되어 가드가 통과한다."
            )
        });
        for line in text.lines() {
            let rest = line.trim_start();
            // `pub` 뒤에 공백을 바로 요구하면 `pub(crate) const` 를 통째로 놓친다 —
            // `primitive.rs` 전량이 그 형태라 표에서 한 층이 조용히 빠진다. 지금은
            // 못 쓰는 층이지만(외부 크레이트가 참조하면 컴파일 에러) 표에 넣는 쪽이
            // 안전하다: 가시성이 넓어지는 날 가드가 이미 덮고 있다.
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

/// 생성 토큰 모듈(`generated`) 참조에서 **`generated::` 뒤의 경로**를 뽑는다
/// (`component::autocomplete::MAX_HEIGHT` · `semantic::SPACE_XS`). 분류는 마지막 조각
/// (상수 이름)으로 하고, 처방은 경로 전체로 찾는다([`theme_path_table`]) — 마지막 조각만
/// 으로는 `HEIGHT` 가 어느 컴포넌트의 것인지 모른다.
///
/// **이름이 아니라 경로로 잡는 이유**: 생성 모듈에는 `HEIGHT` · `SIZE` · `MAX_WIDTH`
/// 같은 일반적인 이름이 있어서, 이름만 대조하면 무관한 로컬 상수를 대량 오검출한다.
/// 경로가 있는 줄(= `use` 문)만 보면 소비 선언 하나당 정확히 한 번 잡힌다.
///
/// **묶음 import(`component::fp::{ROW_HEIGHT, GAP}`)는 항목마다 펼친다.** 펼치지 않으면
/// 경로가 `component::fp::` 에서 끊겨 마지막 조각이 빈 문자열이 되고, 실패문이 "모듈째
/// import 했다" 고 **이름으로 import 한 자리를** 잘못 진단한다. 중첩 묶음은 펼치지 않고
/// 끊긴 경로 그대로 둔다 — 그 자리는 "경로가 이어지지 않는다" 로 떨어져 조용히 빠지지 않는다.
fn generated_const_refs(lines: &[&str]) -> Vec<(String, usize)> {
    // **바늘을 쪼갠다.** 통짜로 두면 이 파일 자신이 그 패턴을 담게 되고,
    // 지금은 스캔 루트 밖이라 안 걸리지만 그건 **우연**이다 — 루트가 넓어지는 날
    // 가드가 자기를 물고, 그때의 처방은 보통 "자기 파일 제외" 라는 면제다.
    // 면제를 두지 않으려면 애초에 바늘을 담지 않으면 된다.
    // 쪼갬이 살아 있는지는 [`the_guard_does_not_carry_its_own_needle`] 가 본다.
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
            // 묶음이 줄을 넘기면(rustfmt 가 긴 묶음을 그렇게 접는다) 닫는 `}` 까지 잇는다.
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

/// `prefix` 가 `::` 로 끝나고 `after` 가 한 겹짜리 묶음(`{A, B as C}`)이면 항목마다
/// `prefix + 이름` 을 돌려준다. 묶음이 아니거나, 같은 줄에서 닫히지 않거나, 중첩이면
/// `None` — 호출부가 끊긴 경로를 그대로 남긴다.
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

/// 생성 길이 상수의 경로 → (그 토큰, 그 토큰의 `&Theme` 경로).
///
/// 이 가드의 처방은 "`&Theme` 을 경유해라" 다. 그 말이 따를 수 있는 처방이 되려면
/// **그 토큰의** 경로를 이름으로 대야 한다. 예전 실패문은 "같은 값의 `Theme`
/// 필드/접근자" 를 가리켰는데, 그것은 두 방향으로 틀린다 — component tier 에는 토큰마다
/// 제 이름의 접근자가 있으므로 값으로 찾을 이유가 없고, 경로가 없는 토큰에서는 값이
/// 같은 **다른 토큰**을 쓰게 만든다(픽셀은 같고 결합이 틀린다 — 그 토큰이 움직이면
/// 무관한 자리가 따라 움직인다. `source_guards::on_scale_length_literal` 이 손으로
/// 읽어 13 중 10 이 그런 자리였다고 적은 형태다).
///
/// 경로는 손으로 적지 않는다. 두 자리에서 읽는다:
///
/// - `SEMANTIC_DIM_TO_THEME_FIELD`(생성기가 쓰는 토큰 ↔ `Theme` 필드 표) — **먼저 나오는
///   항목이 이긴다**(그 표의 doc 이 정한 규칙이다). 필드가 `Theme` 에 실재하는지까지 본다.
/// - component 토큰은 표에 없으면 제 이름의 접근자(`component.menu-radius` →
///   `menu_radius()`)가 `generated_component.rs` 나 `theme.rs`(수기 — `modhint_*`)에
///   실재하는지 본다.
///
/// 둘 다 없으면 `None` 이다. 그 목록은 [`PATHLESS_LENGTH_TOKENS`] 가 이름으로 든다.
fn theme_path_table() -> Vec<(String, String, Option<String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| {
            panic!(
                "{rel} 를 읽을 수 없다 — {e}. 조용히 건너뛰면 처방 표가 비고, 빈 표는 \
                 모든 자리에 '경로 없음' 을 처방한다."
            )
        })
    };
    let theme = read("crates/tasty-type-appearance/src/theme.rs");
    let accessors = format!(
        "{theme}\n{}",
        read("crates/tasty-type-appearance/src/generated_component.rs")
    );
    let theme_struct = theme_struct_body(&theme);
    assert!(
        theme_struct.contains("pub spacing_xs: LogicalPx"),
        "`pub struct Theme {{` 본문을 못 찾았다 — 필드 판정이 전부 '없음' 으로 쏠린다"
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
                let accessor = tok
                    .strip_prefix("component.")
                    .map(|n| n.replace('-', "_"))
                    .filter(|f| accessors.contains(&format!("pub fn {f}(&self) -> LogicalPx")));
                let via = match (field, accessor) {
                    (Some(f), _) => Some(format!("th.{f}")),
                    (None, Some(f)) => Some(format!("th.{f}()")),
                    (None, None) => None,
                };
                out.push((path, tok, via));
            }
        }
    }
    out
}

/// `pub struct Theme {` 의 본문. 필드는 여기서만 찾는다 — `ThemeSizing` 에만 있는
/// 이름은 `th.<필드>` 로 못 부른다.
fn theme_struct_body(theme: &str) -> &str {
    theme
        .split_once("pub struct Theme {")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map_or("", |(body, _)| body)
}

/// 토큰 ↔ 필드 표에서 그 토큰의 `Theme` 필드. 먼저 나오는 항목이 이기고, 그 필드가
/// `Theme` 본문에 `LogicalPx` 로 실재할 때만 돌려준다 — 표 줄이 `ThemeSizing` 에만 있는
/// 필드를 가리키면 `None` 이다(그 이름은 `th.` 로 못 부른다).
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

/// **`&Theme` 경로가 없는 생성 길이 토큰** — (토큰, 사유).
///
/// 이 명부는 [`theme_path_table`] 과 **집합 동등**으로 대조된다
/// ([`every_generated_length_token_has_a_theme_path_or_is_listed`]). 늘어도 줄어도 실패한다 —
/// 경로가 생겼는데 줄이 남으면 그 토큰의 자리에 "경로 없음" 이 처방되고, 경로가 없는데
/// 줄이 없으면 실패문이 없는 경로를 가리킨다.
///
/// 여기 오른 토큰을 UI 가 쓰려면 **경로를 먼저 만든다** — `SEMANTIC_DIM_TO_THEME_FIELD` 에
/// (토큰, 필드) 를 더하고 `ThemeSizing` · `Theme` 필드와 `zoomed()` 배선을 둔다. 값은
/// `crates/tasty-design-tokens/tests/sizing_parity.rs` 가 토큰에 묶으므로 새 값을 정하는
/// 일이 아니다. 값이 같은 다른 `Theme` 이름으로 우회하는 것은 처방이 아니다.
const PATHLESS_LENGTH_TOKENS: &[(&str, &str)] = &[
    (
        "semantic.field-width-range",
        "소비처가 없어 표에 오른 적이 없다",
    ),
    (
        "semantic.font-size-brand-wordmark",
        "component `sidebar-wordmark-font-size` 의 alias 로만 쓰인다 — 그 접근자는 역할 \
         이름이라 이 토큰의 경로가 아니다",
    ),
    (
        "semantic.letter-spacing-ui",
        "자간은 egui `extra_letter_spacing` 에 f32 로 넘어가고 이 토큰의 소비처가 없다",
    ),
    (
        "semantic.radius-pill",
        "component `switch-radius` 의 alias 로만 쓰인다 — 그 접근자는 역할 이름이라 이 \
         토큰의 경로가 아니다",
    ),
];

/// 한 위반 자리의 처방. 경로가 있으면 그 이름을, 없으면 "없다" 와 만드는 법을 말한다.
fn length_const_prescription(
    rel: &str,
    at: usize,
    path: &str,
    table: &[(String, String, Option<String>)],
) -> String {
    match table.iter().find(|(p, ..)| p == path) {
        Some((_, tok, Some(via))) => format!(
            "  {rel}:{at} — `{path}`(`{tok}`)은 LogicalPx(배율 축)다. 같은 토큰의 `&Theme` \
             경로 `{via}` 를 써라"
        ),
        Some((_, tok, None)) => format!(
            "  {rel}:{at} — `{path}`(`{tok}`)은 LogicalPx(배율 축)인데 이 토큰에는 `&Theme` \
             경로가 없다(`PATHLESS_LENGTH_TOKENS`). 값이 같은 다른 `Theme` 이름으로 \
             우회하지 마라 — 경로를 먼저 만든다: `SEMANTIC_DIM_TO_THEME_FIELD` 에 (토큰, \
             필드) 추가 · `ThemeSizing`/`Theme` 필드와 `zoomed()` 배선 · `sizing_parity` 의 \
             arm · 생성물 재생성(`cargo run -p tasty-design-tokens --bin generate`), 그리고 \
             명부에서 그 줄을 지운다"
        ),
        // 표에 없는 경로 — 상수 이름까지 이어지지 않고 끊긴 자리다. 마지막 조각이 빈
        // 문자열(glob · 중첩 묶음)이거나 모듈 이름(모듈째 import)이다.
        None => format!(
            "  {rel}:{at} — `{path}` 는 상수 이름까지 경로가 이어지지 않는다(모듈째 import · \
             glob `*` · 중첩 묶음). 그래서 어느 토큰인지, 길이인지 무차원인지를 가를 수 \
             없다 — 상수를 하나씩 전체 경로로 import 해라(`…::component::<모듈>::<이름>`). \
             그러면 이 가드가 그 토큰의 `&Theme` 경로를 이름으로 댄다"
        ),
    }
}

/// UI 계층은 생성 DTCG 상수 중 **길이(`LogicalPx`)** 를 직접 소비하지 않는다.
///
/// 규칙 자체는 새것이 아니다 — `crates/tasty-design-tokens/src/lib.rs` 의
/// "zoom 우회 금지 (필수)" 가 "런타임 소비는 반드시 `&Theme` 필드/접근자를 경유한다" 고
/// 못박고 `generated/semantic.rs` doc 도 같은 말을 반복한다. **없던 것은 집행이다.**
///
/// 생성 const 는 컴파일 타임 상수라 `with_colors_and_zoom` 의 `zoomed()` 밖이다. 어느
/// 필드에 배율을 걸고 어느 필드에서 뺄지 — **zoom 적용·제외 정책은 `Theme` 가 소유한다**
/// (`border_width` · `status_bar_height` 처럼 일부러 배율을 안 타는 필드도 있다). const 를
/// 직접 읽으면 그 정책을 건너뛴다. 배율을 타는 토큰이면 zoom 1 에서만 같고 0.85 / 1.2 에서
/// 갈라진다 — **토큰을 썼으니 됐다고 믿게 만들기 때문에 리터럴보다 나쁘다.**
///
/// 무차원 상수(`EDGE_DIM_OPACITY` 등 `f32`)는 배율 축이 아니라 대상이 아니다.
#[test]
fn ui_does_not_consume_generated_length_consts_directly() {
    let (logical, other) = generated_const_types();
    assert!(
        logical.len() >= MIN_GENERATED_LOGICAL_CONSTS,
        "생성 LogicalPx 상수가 {}개뿐이다(하한 {MIN_GENERATED_LOGICAL_CONSTS}) — 표가 \
         비면 위반이 0 이 되어 가드가 조용히 통과한다. 무차원 표는 {}개",
        logical.len(),
        other.len()
    );
    assert!(
        other.len() >= MIN_GENERATED_OTHER_CONSTS,
        "생성 무차원 상수가 {}개뿐이다(하한 {MIN_GENERATED_OTHER_CONSTS}) — 두 갈래 중 \
         하나가 비면 타입 분류가 한쪽으로 쏠린다. LogicalPx 표는 {}개",
        other.len(),
        logical.len()
    );
    // **한 이름이 두 갈래에 다 있으면 이름 분류가 모호해진다** — 그 순간 이 가드의
    // 판정은 어느 쪽을 먼저 보느냐에 달린 값이 된다. 편입 시점 겹침 0 이고, 생기면
    // 이름이 아니라 전체 경로로 분류하도록 바꿔야 한다.
    let collisions: Vec<&String> = logical.iter().filter(|n| other.contains(n)).collect();
    assert!(
        collisions.is_empty(),
        "생성 상수 이름 {}개가 LogicalPx 와 무차원 양쪽에 있다 — 이름만으로는 축을 \
         가를 수 없다(경로 분류로 바꿔라): {:?}",
        collisions.len(),
        collisions
    );

    // 표가 실제로 두 갈래로 갈리는지 실소스로 확인한다 — 개수 하한은 "몇 개" 만 보고
    // "무엇" 은 안 본다. 두 이름은 이 축의 양극이다(길이 하나 · 무차원 하나).
    assert!(
        logical.iter().any(|n| n == "ICON_SIZE_SM"),
        "`ICON_SIZE_SM`(LogicalPx)이 길이 표에 없다 — 타입 파서가 죽었다"
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
        "`{ZOOM_CONST_EXCLUDED_ROOT}` 를 뺀 스캔 파일이 {}개뿐이다(하한 \
         {MIN_ZOOM_CONST_SCANNED_FILES}) — 코퍼스가 비면 위반도 0 이다. 전체 스캔 {}개",
        scanned.len(),
        sources.len()
    );

    // **코퍼스가 본체 UI 크레이트에 닿는지 이름으로 확인한다.** 개수 하한은 "몇 개" 만
    // 보고 "어디" 는 안 본다 — 예전 술어는 파일 152 개로 하한을 넉넉히 넘기면서도
    // 아래 두 루트를 통째로 못 보고 있었다. 그 형태는 개수로는 원리적으로 안 잡힌다.
    for root in ["crates/tasty-ui-widgets/src", "crates/tasty-egui-theme/src"] {
        assert!(
            scanned.iter().any(|(rel, _)| rel.starts_with(root)),
            "`{root}` 아래 파일이 스캔 코퍼스에 하나도 없다 — 본체 UI 크레이트다(갤러리가 \
             아니다). 배율을 안 타는 생성 const 를 거기 두면 본체에서만 조용히 깨진다"
        );
    }

    let table = theme_path_table();
    let mut violations = Vec::new();
    let mut dimensionless = 0usize;
    for (rel, contents) in &scanned {
        let lines: Vec<&str> = contents.lines().collect();
        for (path, at) in generated_const_refs(&lines) {
            let name = path.rsplit("::").next().unwrap_or("").to_string();
            // `else` 는 무차원이 아닌 이름 전부를 덮는다 — 셋이 함께 든다. ⒜ 처방 표에
            // 있는 LogicalPx 이름(`&Theme` 경로를 받는 정상 위반), ⒝ LogicalPx 인데 처방
            // 표에 없는 이름, ⒞ 어느 이름 표에도 없는 이름(경로가 상수까지 안 이어진 자리).
            // ⒝ 는 primitive 의 LogicalPx 다 — 처방 표가 semantic·component 만 읽으므로
            // 헬퍼의 끊긴 경로 처방으로 떨어진다. 검출은 덮지만 처방은 틀린다(이미 전체
            // 경로로 쓴 자리에 "전체 경로로 import 해라" 를 준다). 그 상수가 `pub(crate)`
            // 라 오늘은 도달 불가다 — 가시성이 넓어지면 첫 위반이 틀린 처방을 받는다.
            // 표에 있는지 없는지는 헬퍼가 가르므로, 여기서 다시 가르면 헬퍼를 우회할 두
            // 번째 자리만 생긴다.
            if other.contains(&name) {
                dimensionless += 1;
            } else {
                violations.push(length_const_prescription(rel, at, &path, &table));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "UI 계층이 생성 DTCG 길이 상수를 직접 소비한다 — const 는 `zoomed()` 밖이라 \
         zoom 적용·제외 정책(`Theme` 가 소유한다)을 건너뛴다. 배율을 타는 토큰이면 zoom 1 \
         에서만 같다:\n{}\n\
         (같은 스캔에서 무차원 상수 소비 {} 건은 정상으로 통과했다 — 판정기가 두 갈래를 \
         실제로 가르고 있다는 뜻이다)",
        violations.join("\n"),
        dimensionless
    );
}

/// 이 가드가 자기가 찾는 패턴을 자기 소스에 담지 않는다.
///
/// 담아도 오늘은 안 걸린다(이 파일은 스캔 루트 밖이다). 그건 우연이고, 루트가 넓어지는
/// 날 가드가 자기를 문다 — 그때 흔한 처방이 "자기 파일 제외" 라는 면제인데, 면제는
/// 그 파일이 **다른 위반**을 들여도 조용해진다. 바늘을 쪼개면 면제가 필요 없다.
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
        "가드 소스가 자기 바늘을 통짜로 담고 있다 — 쪼개진 형태로 되돌려라"
    );
    // 비영 대조 — 위 0 이 "파일을 못 읽어서 0" 이 아님을 같은 줄에서 보인다.
    assert!(
        me.matches("generated").count() > 0,
        "자기 소스에서 `generated` 를 하나도 못 찾았다 — 파일을 못 읽은 것이다"
    );
}

/// 변이 대상을 이름으로 박지 않고 **실측으로 최악을 고른다.**
///
/// 이 축의 최악은 `ICON_SIZE_SM` 처럼 눈에 띄는 이름이 아니라, **일반적인 이름을 깊은
/// 모듈 경로로 도달**하는 형태다: `component::autocomplete::MAX_HEIGHT`. `MAX_HEIGHT` 는
/// 생성 모듈 여러 곳에 있고 UI 소스에도 흔한 이름이라, 이름만 대조하는 판정기는 여기서
/// 오검출로 무너지고, `semantic::` 한 모듈만 보는 판정기는 **조용히 통과한다.**
///
/// 그 약한 형태가 실제로 초록임을 같은 테스트에서 단정한다 — 그래야 "내 판정기가 더
/// 강하다" 가 주장이 아니라 대조가 된다.
#[test]
fn the_path_parser_beats_the_weak_forms_on_the_deepest_path() {
    // fixture 도 쪼갠다 — 통짜로 쓰면 이 파일이 바늘을 담게 되고
    // [`the_guard_does_not_carry_its_own_needle`] 가 운다. (실제로 울었다.)
    let line = concat!(
        "use tasty_design_tokens",
        "::generated::component::autocomplete::MAX_HEIGHT;"
    );
    let refs = generated_const_refs(&[line]);
    assert_eq!(
        refs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["component::autocomplete::MAX_HEIGHT"],
        "경로를 끝까지 못 뽑았다 — 중간 모듈이 하나 더 끼면 무너진다"
    );

    // 약한 형태 ①: `semantic::` 한 모듈만 보는 판정기 → 이 줄에 **초록**이다.
    assert!(
        !line.contains("generated::semantic::"),
        "약한 형태(semantic 전용)가 이 줄을 잡아 버리면 대조가 성립하지 않는다"
    );
    // 약한 형태 ②: 경로의 **첫** 조각을 상수 이름으로 읽는 판정기 → `component` 를
    // 얻어 상수 표에서 못 찾고, 표에 없으면 무시하도록 짠 판정기는 초록이 된다.
    let first = line
        .split("::generated::")
        .nth(1)
        .and_then(|t| t.split("::").next())
        .unwrap_or("");
    assert_eq!(first, "component");
    let (logical, other) = generated_const_types();
    assert!(
        !logical.iter().any(|n| n == first) && !other.iter().any(|n| n == first),
        "첫 조각이 상수 표에 있으면 약한 형태 ②가 우연히 잡는다 — 대조가 무너진다"
    );

    // 그리고 최악의 이름이 실제로 길이 축이라는 것 — 이게 아니면 위 대조가 헛것이다.
    assert!(
        logical.iter().any(|n| n == "MAX_HEIGHT"),
        "`MAX_HEIGHT` 가 LogicalPx 표에 없다 — 최악 후보를 잘못 골랐다"
    );
}

/// 이 가드의 처방("그 토큰의 `&Theme` 경로를 써라")이 **실재하는 경로**를 가리킨다.
///
/// 처방 표가 무엇을 들고 있는지를 세 겹으로 잰다:
///
/// 1. **모수** — 표의 줄 수가 생성 파일의 `LogicalPx` 상수 수와 같다. doc 주석 파싱이
///    한 줄이라도 놓치면 그 상수의 자리에는 "경로가 이어지지 않는다" 가 처방된다.
/// 2. **component tier 는 전부 경로가 있다.** 토큰마다 제 이름의 접근자가 생성되고,
///    생성기가 건너뛴 것(`modhint_*`)은 `theme.rs` 에 수기로 있다. 이 단정이 깨지면
///    생성기가 새 토큰을 건너뛴 것이다 — 스킵 사유는 생성기가 찍는다.
/// 3. **경로가 없는 토큰은 [`PATHLESS_LENGTH_TOKENS`] 와 집합 동등이다.**
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
            "{file} 의 LogicalPx 상수 {declared} 개 중 {parsed} 개만 처방 표에 올랐다 — \
             doc 주석(`/// `{tier}.<이름>``)과 상수의 짝 읽기가 어긋났다"
        );
    }
    let component = table
        .iter()
        .filter(|(p, ..)| p.starts_with("component::"))
        .count();
    assert!(
        component >= 200,
        "component LogicalPx 상수가 {component} 개뿐이다 — 표가 비면 아래 단정이 전부 공허하다"
    );

    // 양성 대조 — 세 갈래(semantic 필드 · 생성 접근자 · 수기 접근자)가 각각 실제로 풀린다.
    // `CONTROL_HEIGHT_TAB` 은 표에 필드가 둘인 토큰이다(`item_height_tab` · 배율을 안
    // 타는 `tab_bar_height`) — "먼저 나오는 항목이 이긴다" 를 이 줄이 고정한다. 필드가
    // 둘인 토큰은 셋이라 셋 다 적는다 — 하나만 적으면 나머지 둘의 표 순서가 뒤집혀도 모른다.
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
        "`&Theme` 경로가 없는 길이 토큰이 명부와 다르다. 명부에만 있으면 경로가 생겼으니 \
         줄을 지워라. 실측에만 있으면 경로가 사라졌거나 새 토큰이다 — 사유와 함께 올려라"
    );
}

/// 토큰 ↔ 필드 표의 줄이 `Theme` 에 없는 필드를 가리키면 그 줄은 경로가 아니다.
///
/// 오늘 표가 가리키는 필드는 전부 `Theme` 에 있어서 실제 표로는 이 거름이 아무것도 안
/// 거른다 — 그래서 가짜 표 줄과 가짜 소스로 잰다. 가짜 소스는 `ThemeSizing` 에만 있는
/// 필드를 하나 둬, 본문을 `pub struct Theme {` 로 좁히는 것까지 함께 잰다.
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
    // 양성 대조 — 위 `None` 이 "아무것도 못 찾아서" 가 아님을 같은 입력에서 보인다.
    assert_eq!(
        semantic_theme_field("semantic.fake-b", &table, body),
        Some("spacing_xs")
    );
}

/// 처방 문구가 경로의 세 갈래를 제대로 가른다 — 있으면 **그 이름**을 대고 "같은 값" 을
/// 말하지 않으며, 없으면 "없다" 고 말하고 이름을 지어내지 않는다.
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
    // 끊긴 경로는 가드 루프가 실제로 보내는 입력으로 잰다 — 중첩 묶음을 파서에 넣어
    // 나온 경로다. "모듈째 import 했다" 로만 진단하면 이름으로 import 한 자리를 오진한다.
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
            "경로가 끊긴 자리를 다른 갈래로 보냈다: {broken}"
        );
    }
}

/// 묶음 import 를 항목마다 펼친다. 펼치지 않으면 이름으로 import 한 자리가 "모듈째
/// import 했다" 로 잘못 진단된다. rustfmt 가 여러 줄로 접은 묶음도 펼친다.
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
    // rustfmt 가 접은 여러 줄 묶음. 자리는 여는 줄이다.
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
    // 중첩 묶음은 펼치지 않는다 — 끊긴 경로가 그대로 남아 "경로가 이어지지 않는다" 로 떨어진다.
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

/// 반경 축의 **명명 const 우회로**를 닫는다. 자매 가드는 리터럴만 막으므로,
/// `const FOO: f32 = 8.0;` 를 만들어 `.corner_radius(FOO)` 로 쓰면 둘 다 통과했다.
///
/// ADR-0126 이 이 자리를 "자동 채널이 없는 유일한 트리거" 로 적어 뒀고, 그 트리거가
/// 사람 리뷰에서 실제로 발화해 이 판정이 생겼다.
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
        "UI 반경 토큰 값을 복사한 이름 상수가 반경 자리에 있다 — 토큰을 직접 쓸 것\n\
         (`th.corner_radius_sm/corner_radius/corner_radius_lg`):\n\
         · const 는 `ui_zoom` 을 타지 않는데 `corner_radius*` 토큰은 **탄다**. \
         배율 0.85 / 1.2 에서 사본만 고정돼 다른 픽셀로 그려진다\n\
         · 스케일 **밖** 값(3 · 6 · 12 …)의 명명 const 는 그대로 허용된다 — \
         금지되는 것은 토큰 값(2 · 4 · 8)의 **복사본**뿐이다(ADR-0126)\n{}",
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
        "UI 폰트 토큰 값을 복사한 이름 상수가 폰트 자리에 있다 — 토큰을 직접 쓸 것\n\
         (`th.font_size_micro/caption/body/heading/max` 또는 역할 접근자 \
         `badge_font_size()` 등):\n\
         · const 는 `ui_zoom` 을 타지 않는다. 토큰은 탄다. zoom≠1 에서 갈라진다\n\
         · 스케일 **밖** 값(9.5 · 10.5 · 12.5 …)의 명명 const 는 그대로 허용된다 — \
         금지되는 것은 토큰 값의 **복사본**뿐이다(ADR-0126)\n{}",
        violations.join("\n")
    );
}

/// **두 자매 가드가 같은 루트를 본다** — 이 파일의 `SCAN_ROOTS` doc 이 오래 주석으로만
/// 주장하던 불변식이다. 실제로 한 번 갈라졌고(자매가 `src/gfx/gpu` 를 디렉토리로 넓히는
/// 동안 이쪽은 파일 하나로 남았다), 갈라진 동안 **양쪽 다 초록이었다.**
///
/// 두 가드가 같은 축(디자인 토큰 리터럴/사본)을 보므로 모수가 다르면 한쪽만 보는 사각이
/// 생긴다. 상수를 공유할 수 없어(통합 테스트 아이템은 본체 crate 에서 안 보인다) 소스를
/// 읽어 대조한다.
///
/// 자매 파일이 없거나 형태가 바뀌어 파싱이 0 을 내면 **조용히 통과하지 않는다** — 빈
/// 목록도 실패로 본다.
#[test]
fn the_two_sister_guards_scan_the_same_roots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sister = root.join("crates/tasty-doc-guards/tests/design_token_adherence.rs");
    let src = std::fs::read_to_string(&sister)
        .unwrap_or_else(|e| panic!("자매 가드를 읽지 못했다 ({}): {e}", sister.display()));

    let theirs = sister_scan_roots(&src);
    assert!(
        !theirs.is_empty(),
        "자매 가드에서 `const SCAN_ROOTS` 를 못 뽑았다 — 형태가 바뀌었으면 \
         `sister_scan_roots` 도 함께 고칠 것. 0 개를 통과로 세지 않는다"
    );

    let mine: Vec<String> = SCAN_ROOTS.iter().map(|s| (*s).to_string()).collect();
    let mut a = mine.clone();
    let mut b = theirs.clone();
    a.sort();
    b.sort();
    assert_eq!(
        a, b,
        "두 자매 가드의 스캔 루트가 갈라졌다 — 같은 축을 보므로 한쪽만 넓히면 \
         나머지 한쪽에 사각이 생긴다"
    );

    // 그리고 그 루트는 디렉토리여야 한다 — 파일 열거는 새 파일을 기본 제외로 만든다.
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
        "semantic 이 없는 DTCG primitive 폰트 값을 쓰는 const 가 이름에 그 사실을 담고 \
         있지 않다 — ADR-0126 이 요구하는 형태다:\n\
         · 호출 자리에서 `.size(FOO_SIZE)` 만 보고는 그게 토큰인지 미배정 primitive 인지 \
         알 수 없다. 이름이 그걸 말해야 한다\n\
         · 선례: `ATTN_PRIMITIVE_12`\n\
         · semantic 이 생기면 const 가 통째로 사라지고 이 규칙도 그 자리에서 끝난다\n{}",
        violations.join("\n")
    );
}

#[cfg(test)]
mod discriminate {
    use super::*;

    /// `SCAN_ROOTS` 블록 파서가 **주석 안의 따옴표를 루트로 읽지 않는다**.
    ///
    /// 이 블록에는 자유 주석이 들어간다(자매 파일에 실제로 있다). 거기에 경로를
    /// 인용하면 파서가 그것을 루트로 뽑아 자매 대조가 **거짓 빨강**을 낸다 —
    /// 방향은 안전하지만 메시지가 원인을 잘못 가리키므로 진단이 헛돈다.
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

    /// 반경 축의 **전제 검사**. 폰트 축과 같은 축(값 × 자리)인지, 그리고 경로 수식을
    /// 벗기는 것이 실제로 동작하는지를 함께 잰다 — 반경 호출자리의 명명 const 는 다수가
    /// `크레이트::모듈::NAME` 형태라, 안 벗기면 **가드가 있어도 하나도 안 잡힌다.**
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

        // ① 벌거벗은 이름 — 잡힌다
        assert_eq!(check(&["    .corner_radius(PILL_RADIUS)"]).len(), 1);
        // ② 경로 수식 — 벗기지 않으면 놓친다. 이 단언이 그 벗기기를 못박는다.
        assert_eq!(
            check(&["    .corner_radius(tasty_ui_widgets::tokens::PILL_RADIUS)"]).len(),
            1,
            "경로 수식된 const 를 놓쳤다 — 실제 호출자리의 지배적 형태다"
        );
        // ③ 다른 생성자 형태
        assert_eq!(check(&["    CornerRadius::same(SMALL_R)"]).len(), 1);
        // ④ 스케일 밖 값은 허용 — ADR-0126 이 명시한 형태다
        assert_eq!(
            check(&["    .corner_radius(BOOT_CARD_CORNER_RADIUS)"]).len(),
            0
        );
        // ⑤ 자리가 다르면 이 판정의 대상이 아니다(값은 같아도)
        assert_eq!(check(&["    .size(PILL_RADIUS)"]).len(), 0);
        // ⑥ 주석은 코드가 아니다
        assert_eq!(check(&["    // .corner_radius(PILL_RADIUS)"]).len(), 0);
        // ⑦ 표에 없는 이름은 값을 모르므로 판정하지 않는다(고발 기본값 금지)
        assert_eq!(check(&["    .corner_radius(UNKNOWN_RADIUS)"]).len(), 0);
    }

    /// 판정의 **전제 검사** — 이 가드는 "이름에 FONT 가 들어가는가" 가 아니라
    /// "**폰트 자리에 온 상수의 값이 토큰 값인가**" 로 가른다. 축이 이름으로
    /// 미끄러지면 여기서 죽는다.
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

        // 값이 토큰과 같다 → 이름이 무엇이든 잡힌다.
        assert_eq!(check(&["    .size(BODY_FONT_SIZE),"]).len(), 1);
        assert_eq!(check(&["    .size(SOMETHING_ELSE),"]).len(), 1);
        // 값이 스케일 밖이다 → 이름에 FONT 가 있어도 잡지 않는다(ADR-0126 이 허용).
        assert_eq!(check(&["    .size(OFF_SCALE_FONT_SIZE),"]).len(), 0);
        // 폰트 자리가 아니다(스피너 지름) → 값이 토큰과 같아도 축 밖이다.
        assert_eq!(
            check(&[
                "    Spinner::new()",
                "        .size(LOADING_SPINNER_SIZE)",
                "        .color(c),",
            ])
            .len(),
            0
        );
        // 한 줄에 폰트 호출이 둘이어도 뒤엣것을 놓치지 않는다.
        assert_eq!(
            check(&[
                "    Spinner::new().size(LOADING_SPINNER_SIZE).show(ui, th); ui.label(RichText::new(x).size(BODY_FONT_SIZE));"
            ])
            .len(),
            1
        );
        // 같은 look-back 이 `;` 을 넘지 않는다 — 앞 문의 스피너가 뒤 문을 면제하지 못한다.
        assert_eq!(
            check(&[
                "    Spinner::new().size(LOADING_SPINNER_SIZE).show(ui, th);",
                "    ui.label(RichText::new(x).size(BODY_FONT_SIZE));",
            ])
            .len(),
            1
        );
        // `FontId::` 세 형태도 같은 자리다.
        assert_eq!(
            check(&["    egui::FontId::proportional(BODY_FONT_SIZE),"]).len(),
            1
        );
        assert_eq!(
            check(&["    egui::FontId::monospace(BODY_FONT_SIZE),"]).len(),
            1
        );
        // 주석 줄은 세지 않는다.
        assert_eq!(check(&["    // .size(BODY_FONT_SIZE)"]).len(), 0);
        // 표에 없는 이름은 값을 모르므로 판정하지 않는다(한계 — 모듈 doc 에 적었다).
        assert_eq!(check(&["    .size(UNKNOWN_CONST),"]).len(), 0);
    }

    /// **주석 스킵을 겨냥한 변이.** 판정기는 `//` 로 *시작하는* 줄만 건너뛴다 —
    /// 창을 "주석을 포함한 줄" 로 넓히면 뒤따르는 주석 한 조각이 진짜 위반을 가린다
    /// (706 이 `ci_channel_claims_match_workflows` 에서 정확히 그 형태로 뚫렸다).
    #[test]
    fn the_comment_skip_is_line_start_only() {
        let consts = vec![("BODY_FONT_SIZE".to_string(), 13.0_f32)];
        let check = |l: &str| {
            let mut out = Vec::new();
            const_font_violations("f.rs", &[l], &consts, &mut out);
            out.len()
        };
        // 주석 줄 — 안 센다(의도).
        assert_eq!(check("    // .size(BODY_FONT_SIZE)"), 0);
        // 코드 뒤에 주석이 붙은 줄 — **센다.** 주석이 사면권이 되면 안 된다.
        assert_eq!(check("    .size(BODY_FONT_SIZE), // 임시"), 1);
        assert_eq!(check("    .size(BODY_FONT_SIZE), /* 임시 */"), 1);
    }

    /// **의도된 false negative 를 고정한다.** 아래는 못 잡는 것이 설계다 — 나중에 누가
    /// 판정기를 넓히면 그 결정이 여기서 실패로 드러나고, 의도된 한계와 버그가 섞이지
    /// 않는다.
    #[test]
    fn the_intended_false_negatives_stay_false_negative() {
        let consts = vec![("BODY_FONT_SIZE".to_string(), 13.0_f32)];
        let check = |l: &str| {
            let mut out = Vec::new();
            const_font_violations("f.rs", &[l], &consts, &mut out);
            out.len()
        };
        // 소문자 지역 변수 — 값의 출처를 소스 스캔으로 따라갈 수 없다.
        assert_eq!(check("    .size(body_font_size)"), 0);
        // 인라인 리터럴 — 이 가드가 아니라 자매 가드(`design_token_adherence`)의 축이다.
        assert_eq!(check("    .size(13.0)"), 0);
        // 계산식 경유 — const 표가 리터럴 정의만 따라간다.
        assert_eq!(check("    .size(BODY_FONT_SIZE * 1.0)"), 0);
    }

    /// primitive 이름 규칙 판정기도 같은 축(값 + 위치)에서 돈다. 이름 검사는 **규칙의
    /// 대상이 이름이기 때문**이지, 판별을 이름으로 대신하는 것이 아니다.
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
        // 스케일 밖 값과 UI 토큰 값은 이 축이 아니다.
        assert_eq!(check(&["    .size(OFF_SCALE)"]), 0);
        assert_eq!(check(&["    .size(UI_TOKEN)"]), 0);
        // 폰트 자리가 아니면(스피너 지름) 값이 12·16 이어도 축 밖이다.
        assert_eq!(check(&["    Spinner::new().size(SPIN).show(ui, th);"]), 0);
        // 이름이 맞아도 값이 다른 primitive 면 잡힌다 — 이름만 보지 않는다.
        assert_eq!(
            check(&["    .size(QUX_PRIMITIVE_16)", "    .size(FOO_SIZE)"]),
            1
        );
    }

    /// CRLF 로 체크아웃된 트리(Windows 잡)에서도 같은 판정이어야 한다 —
    /// `str::lines()` 가 `\r` 을 떼지만, 인자 파싱이 `\r` 을 물면 이름이 어긋난다.
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

/// **이 파일에는 원래 면제 목록이 없었다.** 아래 `OVER_CAP_*` 둘이 그 부류를 만든
/// 것이고, 여기에 줄을 더하는 것은 항목 추가가 아니라 **부류를 넓히는 일**이다.
///
/// 만든 이유는 상한에 **문서로 승인된 예외가 실재하기 때문**이다(브랜드 락업). 그것을
/// 적을 자리가 없으면 가드 자체를 만들 수 없고, 파일 단위 제외는 다른 위반까지 덮으므로
/// 더 나쁘다. 그래도 대가는 남는다 — **다음 사람에게 "고치는 대신 목록에 넣는" 길이
/// 열려 있다.** 그 길을 좁히는 것이 아래 래칫과 역방향 검사다.
///
/// 새 줄을 더하기 전에 **먼저 그 자리를 고칠 수 있는지** 보고, 못 고치면 그 이유를
/// 적어라. 이 판단은 커밋 안에서 혼자 내릴 것이 아니다.
///
/// ── UI 폰트 상한(14px)을 넘는 명명 const 의 **정책 면제** — 범위 밖이라 남는다.
///
/// 사유의 형태가 "이 자리는 규칙 범위 밖이다" 라, 덮을 것이 지금 없어도 지우지 않는다
/// (ADR-0150). 아래 [`OVER_CAP_PENDING`] 과 **성격이 달라 합치지 않는다.**
const OVER_CAP_SANCTIONED: &[(&str, &str)] = &[(
    // 브랜드 락업 — `docs/architecture/boot-sequence.md` 가 "14px UI 폰트 상한의
    // sanctioned 예외" 라고 명시한다. 워드마크는 브랜드 자산의 verbatim 전사라
    // UI 텍스트 스케일의 대상이 아니다.
    "src/gfx/gpu/shell_setup.rs",
    "SETUP_BRAND_TITLE_SIZE",
)];

/// 상한을 넘는데 **승인이 없는** 자리 — **면제가 아니라 "아직" 이다.**
///
/// 셋째 칸에 **무슨 결정을 기다리는지** 적는다. 그 칸이 비면 이 목록은 면제 목록과
/// 구별되지 않는다 — 기다리는 것이 없는 항목은 부채가 아니라 면제다.
///
/// 사유의 형태가 한 가지라(= "이 자리가 지금 상한을 넘는다") 사유가 사라지면 항목도
/// 사라져야 한다. 그 강제는 아래 역방향 검사가 한다. 목록 자체의 증감은
/// [`OVER_CAP_PENDING_BUDGET`] 래칫이 본다.
const OVER_CAP_PENDING: &[(&str, &str, &str)] = &[(
    // primitive 16 을 쓰는 자리. const 의 doc 은 tier 를 설명하지만 **상한을 넘는다는
    // 말은 없다** — 상한이 `theme.md` 표에 한 줄로만 있고 채널이 없어서, 그 자리를
    // 만든 사람도 위반이라고 인지하지 못했다.
    "src/view/plugins/ui/add.rs",
    "ADD_PREVIEW_NAME_PRIMITIVE_16",
    "16 을 14 로 내릴지(픽셀이 바뀐다) 승인으로 올릴지 — 어느 semantic 에 묶을지가 \
     정해져야 답이 난다. 디자인 판정 대기",
)];

/// 한시 목록의 건수 래칫. **늘어도 실패하고 줄어도 실패한다** — 줄었으면 이 수도 같이
/// 내리라는 뜻이다. 남는 여유가 곧 안 보는 구간이다.
///
/// 하한(`<=`)으로 두면 항목이 조용히 늘고, 안 두면 이 목록이 그냥 면제 목록이 된다.
const OVER_CAP_PENDING_BUDGET: usize = 1;

/// UI 폰트 상한. `docs/design/systems/theme.md` "UI 폰트 최대" 행이 정본이다.
///
/// **비교 전용 문턱이다 — 렌더되는 자리가 없다.** 쓰이는 곳은 비교 하나와 실패 메시지
/// 둘뿐이라, 이 값을 그리는 코드를 찾지 마라.
///
/// **토큰을 링크한다 — 값을 손으로 옮겨 적지 않는다.** theme.md 의 그 행이 상한을
/// `font_size_max` 라고 이름으로 못박고 있으므로 정본은 그 토큰이고, 여기 숫자를 다시
/// 적으면 두 벌이 되어 **갈라져도 아무도 모른다.** 실측: 링크 전에는 토큰만 14 → 16 으로
/// 바꿔도 이 가드가 초록이었다(가드는 계속 14 를 강제하고, 토큰은 16 이라고 말한다).
///
/// 그래서 `LogicalPx` 인 것도 저절로 따라온다 — `SIZING.font_size_max` 가 그 타입이다.
/// (`f32` 로 두면 `src/source_guards/length_constant_frontier` 가 잡는다. 그 가드에는
/// 면제 목록이 없고 `FRONTIER` 영역 한 줄뿐이라, 빠져나가려면 전선을 면제 목록으로
/// 바꿔야 한다 — 그 대가가 `.value()` 세 번보다 크다.)
const UI_FONT_SIZE_CAP: LogicalPx = tasty_type_appearance::theme::SIZING.font_size_max;

/// 상수 표 하한 — 표가 비면 "상한 초과 0" 이 조용히 참이 된다.
const MIN_SCANNED_CONSTS: usize = 100;

/// 폰트 자리의 명명 const 가 **UI 폰트 상한을 넘지 않는다.**
///
/// 상한은 `CLAUDE.md` 와 `docs/design/systems/theme.md` 에 규칙으로 있었지만 **그것을
/// 보는 판정기가 없었다.** 값의 크기는 기계가 볼 수 있는데도 리터럴 재유입(자매 가드)과
/// 토큰 복사본만 보고 있었고, 크기는 아무도 안 봤다.
///
/// 면제를 **사유별로 두 목록으로** 나눈다. 섞으면 역방향 검사를 못 건다 — 정책 면제는
/// 덮을 것이 없어도 남아야 하고(ADR-0150) 한시 부채는 사라지면 지워져야 하는데, 한
/// 목록에서는 두 규칙이 동시에 성립할 수 없다. `crates/tasty-doc-guards/tests/layering.rs` 가 같은 이유로
/// `ALLOWED_PATHS` 와 `BASELINE_FILES` 를 갈라 둔다.
#[test]
fn no_named_font_const_exceeds_the_ui_font_size_cap() {
    // 래칫 — 한시 목록은 늘어도 줄어도 실패한다. 줄었으면 상한도 같이 내려라.
    assert_eq!(
        OVER_CAP_PENDING.len(),
        OVER_CAP_PENDING_BUDGET,
        "한시 목록의 건수가 상한({OVER_CAP_PENDING_BUDGET})과 다르다. 줄였으면 \
         `OVER_CAP_PENDING_BUDGET` 도 같이 내려라 — 남는 여유가 곧 안 보는 구간이다. \
         늘리는 것은 이 파일에 없던 부류를 넓히는 일이라 혼자 정할 것이 아니다"
    );
    // 기다리는 것이 없는 항목은 부채가 아니라 면제다 — 그러면 두 목록을 가른 근거가 사라진다.
    for (rel, name, awaiting) in OVER_CAP_PENDING {
        assert!(
            !awaiting.trim().is_empty(),
            "`{name}`({rel}) 이 무슨 결정을 기다리는지 안 적혀 있다 — 그 칸이 비면 \
             이 목록은 면제 목록과 구별되지 않는다"
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

    // 코퍼스가 비면 위반도 0 이다 — 상수 표가 살아 있는지 먼저 단정한다.
    assert!(
        consts.len() >= MIN_SCANNED_CONSTS,
        "상수 표가 {}개뿐이다(하한 {MIN_SCANNED_CONSTS}) — 표가 비면 상한 검사가 \
         조용히 통과한다",
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
        "UI 폰트 상한({}px)을 넘는 명명 const 가 폰트 자리에 있다 — \
         `docs/design/systems/theme.md` \"UI 폰트 최대\" 행이 정본이다:\n{}\n\
         승인된 예외라면 사유를 문서에 적고 `OVER_CAP_SANCTIONED` 에, 디자인 판정을 \
         기다리는 자리라면 `OVER_CAP_PENDING` 에 올려라 — 두 목록은 성격이 달라 섞지 않는다",
        UI_FONT_SIZE_CAP.value(),
        unlisted.join("\n")
    );

    // 역방향 — 한시 목록은 줄어들기만 한다. 정책 목록(`OVER_CAP_SANCTIONED`)에는 이
    // 검사를 걸지 않는다: 그쪽 사유는 "범위 밖" 이라 덮을 것이 없어도 유효하다(ADR-0150).
    let gone: Vec<String> = OVER_CAP_PENDING
        .iter()
        .filter(|(r, n, _)| !pending_seen.iter().any(|(sr, sn)| sr == r && sn == n))
        .map(|(r, n, awaiting)| format!("  {r} — `{n}` (기다리던 것: {awaiting})"))
        .collect();
    assert!(
        gone.is_empty(),
        "한시 목록이 가리키는 자리가 이제 상한을 안 넘는다 — 고쳤으면 목록에서도 \
         지워라. 남겨 두면 \"여기는 원래 부채\" 라는 신호가 아무것도 안 덮은 채 \
         살아남는다:\n{}",
        gone.join("\n")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 색 파생 계수 — 익명 리터럴 금지
// ─────────────────────────────────────────────────────────────────────────────

/// 색을 약하게 만드는 두 호출. 값 공간은 다르지만(배율 0~1 · 알파 0~255) 의도가 같고,
/// 레포 안에 그 둘이 **같은 비율의 두 표현**임을 적은 자리가 이미 있다
/// (`tasty-plugin-markdown` 의 diff 배경: `with_alpha(31)` ≈ `gamma_multiply(0.12)`).
/// 그래서 한 축으로 본다.
const COLOR_COEFF_CALLS: &[&str] = &[".gamma_multiply(", ".with_alpha("];

/// 이 축의 스캔 인구는 **레포 전체**다 — 위 `SCAN_ROOTS`(UI 여섯 곳)보다 넓다.
/// 계수를 쓰는 자리가 번들 plugin 크레이트에도 있어서, UI 루트로 좁히면 그쪽이 조용히
/// 열린다. 좁힘은 조용하고 넓힘은 시끄럽다 — 이 축은 넓은 쪽을 고른다.
const COLOR_COEFF_SCAN_ROOTS: &[&str] = &["src", "crates"];

/// 스캔이 실제로 레포를 훑었는지 고정한다. 경로가 틀리면 0 건이 초록으로 보인다.
const MIN_COLOR_COEFF_SCANNED_FILES: usize = 1000;

/// 줄에서 **숫자 리터럴을 인자로 받은** 색 계수 호출을 뽑는다.
///
/// 주석은 판정 대상이 아니다. 줄 전체 주석(`//`·`///`·`//!`)은 버리고, 뒤에 달린
/// 주석은 `"// "`(뒤 공백 포함)에서 자른다 — 공백을 요구해야 `https://` 가 안 잘린다.
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

/// 색 파생 계수(`gamma_multiply` 배율 · `with_alpha` 알파)는 익명 리터럴로 두지 않는다.
///
/// **대응 토큰이 없는 축이다.** DTCG 에 있는 opacity 는 `disabled`(0.5) ·
/// `recessed`(0.4) · `dimmed`(0.75) 셋뿐이고, 실제 쓰이는 값은 0.09~0.92 로 훨씬 넓다.
/// 그래서 이 가드는 "토큰을 써라" 가 아니라 **"값에 이름과 사유를 붙여라"** 를 요구한다 —
/// [ADR-0126](../docs/adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) 이
/// 폰트·반경 축에서 내린 것과 같은 결정이고, 그 ADR 본문이 결정을 **축 중립**이라고
/// 적는다.
///
/// **면제 목록이 없다.** 오늘 위반 0 이라 필요가 없고, 첫 예외를 넣는 것은 항목 추가가
/// 아니라 **부류의 창설**이다(같은 판단을 이 파일의 `OVER_CAP_SANCTIONED` 가 이미
/// 기록하고 있다). 넣기 전에 그 자리를 고칠 수 있는지 먼저 보고, 못 고치면 올려라.
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
        "스캔한 .rs 가 {} 개뿐이다 — 하한 {MIN_COLOR_COEFF_SCANNED_FILES}. 경로가 \
         틀렸으면 위반 0 이 측정이 아니라 침묵이다",
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
        "색 파생 계수가 익명 리터럴이다 ({} 자리). 사유를 적은 명명 const 로 올려라 — \
         값을 바꾸지 말고 이름만 붙인다. 값을 모으는 것(수렴)은 픽셀이 바뀌므로 별개의 \
         디자인 결정이다:\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// 위 0 이 **깨진 계측기의 0** 이 아님을 보인다.
///
/// 바늘을 소스에 통짜로 남기면 이 파일 자신이 위 가드에 걸린다 — 그래서 합성 줄은
/// 런타임에 조립한다.
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

    // 명명 const 로 올린 형태는 통과해야 한다 — 통과 못 하면 처방이 가드에 막힌다.
    let good = format!("        .fill(c{gamma}WARN_BADGE_FILL_OPACITY))");
    assert!(
        color_coeff_literals(&good).is_empty(),
        "명명 const 형태가 위반으로 잡혔다 — 이 가드가 요구하는 형태 자체가 막힌다"
    );

    // 주석은 판정 대상이 아니다. 줄 주석과 뒤에 달린 주석 둘 다.
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

// ─────────────────────────────────────────────────────────────────────────────
// 점 치수 축 — 익명 반지름 금지 · 토큰 값 사본 금지
// ─────────────────────────────────────────────────────────────────────────────

/// 점을 그리는 세 표기. 오늘 소스에 있는 것은 앞의 둘뿐인데 셋째를 함께 막는 이유는
/// `FontId::new(` 과 같다 — 둘만 막으면 셋째 표기로 그대로 재유입된다.
const DOT_RADIUS_CALLS: &[&str] = &["circle_filled(", "circle_stroke(", "circle("];

/// 점 축의 스캔 인구는 `SCAN_ROOTS` 에서 **갤러리를 뺀 것**이다.
///
/// 목록을 따로 적지 않고 걸러서 만든다 — 따로 적으면 두 목록이 조용히 갈린다
/// (자매 둘이 실제로 그렇게 갈렸고 갈린 동안 양쪽 다 초록이었다).
///
/// 갤러리를 빼는 사유는 하나다: 갤러리는 egui 전역 zoom 을 쓰므로 리터럴도 함께 커진다
/// ([ADR-0135](../docs/adr/0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md)).
/// 점 지름은 **길이**라 그 예외가 그대로 걸린다 — 같은 파일의 색 계수 축이 갤러리를
/// 모수에 넣는 것과 반대이고, 가르는 것은 경로가 아니라 **무차원이냐 길이냐**다.
fn dot_scan_roots() -> Vec<&'static str> {
    SCAN_ROOTS
        .iter()
        .copied()
        .filter(|r| !r.starts_with(ZOOM_CONST_EXCLUDED_ROOT))
        .collect()
}

const MIN_DOT_SCANNED_FILES: usize = 150;

/// `status_dot_size` 토큰 값. `zoomed()` 를 타므로 이 값의 명명 const 는 사본이다.
///
/// **비교 전용이다 — 이 값이 렌더되는 자리는 없다.** 그래도 `f32` 가 아니라
/// `LogicalPx` 로 선언한다: `src/source_guards/length_constant_frontier.rs` 가 전선 밖
/// `f32` 길이 상수를 막고, 이 축의 값은 길이(점 지름)라 그 술어에 정확히 걸린다.
/// 같은 형태를 이 파일의 `UI_FONT_SIZE_CAP` 이 이미 쓴다.
const DOT_SIZE_TOKEN_VALUE: LogicalPx = LogicalPx(8.0);

/// 이름이 점을 뜻하는지 판별하는 표지.
const DOT_CONST_NAME_MARK: &str = "DOT";

/// 한 줄에서 점 호출의 **두 번째 인자**(반지름)가 숫자 리터럴이면 돌려준다.
///
/// 괄호 균형을 세는 이유는 반지름이 보통 식이기 때문이다
/// (`theme.status_dot_size.value() * 0.5`). 균형을 안 세면 그 안의 `,` 에서 잘린다.
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

/// 점 반지름을 숫자 리터럴로 적지 않는다 — 이 축의 **① 자매**다.
///
/// 폰트·반경·색 계수 축에는 이 형태가 있었는데 점 축에만 없었다. 오늘 본체 루트의
/// 위반은 0 이라 면제 목록 없이 선다.
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
         그것을, 스케일 밖 값이면 사유를 적은 명명 const 를 써라(ADR-0126):\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// 점 축의 **② 자매** — 토큰 값 8 을 복사한 점 이름 const 를 막는다.
///
/// **판별 축이 자매 둘과 다르다. 이름이다.** 폰트·반경 축은 `값 × 자리`로 재는데
/// (`.size(` · `.corner_radius(` 라는 문법적 자리가 있다) 점 축에는 **그 자리가 없다** —
/// 점 const 는 `.value() * 0.5` 나 `vec2(` 로 소비되는 일반 산술이라, 소비 형태만 보고
/// "점 자리" 를 가릴 수 없다. 값만으로 재면 점이 아닌 `LogicalPx(8.0)` 상수를
/// 잡는다(실측: `file_handler_picker.rs` 의 `VERTICAL_PADDING` · `HORIZONTAL_MARGIN`).
///
/// **그래서 이 가드는 사거리가 좁다.** 이름에 `DOT` 이 없는 상수를 점에 쓰면 통과한다 —
/// 자매 둘이 이름 축을 거부한 바로 그 이유이고, 여기서는 그 대안이 없다. 이 한계는
/// 감추지 않는다: **부류 밖에 대상이 없다고 단정하지 않는다.**
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

    // 비영 대조 — 점 이름 상수를 하나도 못 찾았으면 위 0 은 측정이 아니다.
    assert!(
        dot_consts > 0,
        "점 이름 상수를 하나도 못 찾았다 — 이름 표지(`{DOT_CONST_NAME_MARK}`)나 상수 \
         파서가 깨졌다. 그 상태의 위반 0 은 침묵이다"
    );
    assert!(
        violations.is_empty(),
        "`status_dot_size`({}) 값을 복사한 점 이름 상수가 있다 — \
         토큰을 직접 써라(`th.status_dot_size`):\n\
         · const 는 `ui_zoom` 을 안 타는데 `status_dot_size` 는 **탄다**(0.85/1.0/1.2 에서 \
         7/8/10). 사본만 고정돼 다른 픽셀로 그려진다\n\
         · 스케일 **밖** 값(4 · 5 · 6 · 7)의 명명 const 는 그대로 허용된다(ADR-0126)\n{}",
        DOT_SIZE_TOKEN_VALUE.value(),
        violations.join("\n")
    );
}
