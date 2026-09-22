//! 소스를 **텍스트로 읽는** 가드들이 공유하는 두 가지: 코드가 아닌 부분을 덮는 것과,
//! 스캔 루트 아래의 `.rs` 를 모으는 것.
//!
//! 여기 있는 이유는 소비자가 **다른 컴파일 단위**에 흩어져 있기 때문이다 — 본체의
//! `src/source_guards/`(단위 테스트)와 루트 `tests/`(통합 타깃)는 서로의 비공개
//! 항목을 못 본다. 각자 사본을 두면 같은 물음에 답이 둘이 되고, 갈린 쪽은 조용하다.

use std::path::PathBuf;

/// 주석·문자열·문자 리터럴을 공백으로 덮은 사본을 만든다. 줄바꿈은 그대로 두므로
/// 결과 문자열의 줄 번호는 원본과 같다. 라이프타임 틱(`'a`)은 문자 리터럴과 구분한다.
///
/// "여기 **코드**에 X 가 있나" 를 묻는 가드가 쓴다.
pub fn mask_non_code(src: &str) -> String {
    mask(src, Fate::Blank, Fate::Blank, Fate::Blank)
}

/// [`mask_non_code`] 와 같은 구간을 덮되 **바이트 위치까지** 원본과 맞춘 사본 — 덮은
/// 글자 하나를 그 글자의 UTF-8 바이트 수만큼의 공백으로 바꾼다.
///
/// 구조(괄호 짝 · 구분자)는 이 사본에서 찾고 **값은 같은 구간의 원본에서 읽는** 가드가
/// 쓴다([`crate::match_arms`]). [`mask_non_code`] 는 글자 수만 지키므로, 덮인 구간에
/// 한글 같은 여러 바이트 글자가 있으면 그 뒤의 바이트 위치가 원본과 어긋난다.
pub fn mask_non_code_aligned(src: &str) -> String {
    mask(src, Fate::BlankBytes, Fate::BlankBytes, Fate::BlankBytes)
}

/// 주석만 덮고 리터럴은 남기되 **바이트 위치까지** 원본과 맞춘 사본.
///
/// [`mask_non_code_aligned`] 의 짝이다 — 그 사본에서 공백인 자리가 이 사본에서도 공백이면
/// 공백이나 주석이고, 여기서 글자가 남으면 리터럴이다. 구조 주사가 "주석은 건너뛰되
/// 리터럴은 건너뛰지 않는다" 를 가를 때 쓴다.
pub fn mask_comments_aligned(src: &str) -> String {
    mask(src, Fate::BlankBytes, Fate::Keep, Fate::Keep)
}

/// 문자열·문자 리터럴만 덮고 **주석은 원문 그대로 남긴** 사본.
///
/// "여기 **주석**이 달려 있나" 를 묻는 가드가 쓴다. 두 물음은 서로의 답을 지우므로
/// 한 함수로 못 합친다 — 주석을 덮으면 주석의 유무를 못 묻고, 안 덮으면 주석 속 코드
/// 형태가 코드로 세어진다. 판정기를 하나로 모으는 것은 **원인**을 모으는 것이지 함수
/// 개수를 줄이는 것이 아니다.
///
/// 이 결과에 `//` 가 있으면 진짜 주석이다. 원문에는 있는데 여기 없으면 그 `//` 는
/// 문자열 안에 있었다는 뜻이다 — URL 이 대표적이다.
pub fn mask_literals(src: &str) -> String {
    mask(src, Fate::Keep, Fate::Blank, Fate::Blank)
}

/// 주석만 덮고 **문자열·문자 리터럴은 원문 그대로 남긴** 사본.
///
/// "여기 **코드**에 이 리터럴을 담은 형태가 있나" 를 묻는 가드가 쓴다 —
/// `#[cfg(feature = "gui")]` 처럼 찾는 형태 안에 문자열 리터럴이 들어 있으면
/// [`mask_non_code`] 는 그 리터럴까지 지워 형태를 못 찾고, 원문은 주석 속 언급까지 센다.
pub fn mask_comments(src: &str) -> String {
    mask(src, Fate::Blank, Fate::Keep, Fate::Keep)
}

/// 문자 리터럴 안의 `"` 만 안전한 글자로 바꾼 사본. **그 밖은 원문 그대로다** —
/// 주석도 문자열도 코드도 안 건드린다.
///
/// 줄 수를 세는 계측기(`tokei`)가 쓴다. 그 계측기는 `'"'` 의 따옴표를 **문자열의
/// 시작**으로 읽고, 그 뒤 파일 끝까지를 문자열 안으로 본다 — 문자열 안의 빈 줄은
/// code 로 세므로 그 파일의 code 가 실제보다 **크게** 나온다. 실측(tokei 14.0.0,
/// 2026-09-09): `src/core/attach_runtime.rs` 에 `'"'` 한 자리가 들어오자 출하 SLOC 이
/// 1240 → 3046 으로 뛰었는데 같은 구간의 원시 순증은 +364 였다. 오진은 조용하다 —
/// 계측기는 성공으로 끝나고 값만 틀리다.
///
/// 여기서 고치는 이유: 판정(무엇이 출하되는가)과 계측(몇 줄인가)을 가른 설계에서
/// **사본은 계측 전용 산출물**이다. 계측기가 읽을 수 있는 형태로 넘기는 것이 사본을
/// 만드는 쪽의 몫이고, 소스를 계측기에 맞춰 쓰라고 요구하는 것보다 좁다. 바꾸는 것은
/// 리터럴 **안의 한 글자**뿐이라 줄 수도 code 수도 안 움직인다.
///
/// **내용 동등을 묻는 소비자는 이걸 쓰면 안 된다** — `'"'` 와 `'x'` 가 같아 보인다.
pub fn neutralize_char_literal_quotes(src: &str) -> String {
    mask(src, Fate::Keep, Fate::Keep, Fate::QuoteSafe)
}

/// 렉서가 구간 하나를 만났을 때 그 글자를 어떻게 할지.
///
/// 셋을 한 렉서에 두는 이유는 구간을 **가르는 규칙**이 하나여야 하기 때문이다 —
/// raw string · 이스케이프 · 라이프타임 틱을 아는 사본이 여럿이면 갈린 쪽이 조용하다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// 공백으로 덮는다(줄바꿈은 남긴다).
    Blank,
    /// 공백으로 덮되 글자의 바이트 수만큼 채운다(줄바꿈은 남긴다).
    BlankBytes,
    /// 원문 그대로 둔다.
    Keep,
    /// 원문 그대로 두되 `"` 만 안전한 글자로 바꾼다.
    QuoteSafe,
}

fn mask(src: &str, comments: Fate, strings: Fate, char_literals: Fate) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < chars.len() {
        i = match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                mask_line_comment(&chars, i, &mut out, comments)
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                mask_block_comment(&chars, i, &mut out, comments)
            }
            'r' | 'b' if raw_string_hashes(&chars, i).is_some() => {
                mask_raw_string(&chars, i, &mut out, strings)
            }
            '"' => mask_quoted(&chars, i, '"', &mut out, strings),
            '\'' if is_char_literal(&chars, i) => {
                mask_quoted(&chars, i, '\'', &mut out, char_literals)
            }
            c => {
                out.push(c);
                i + 1
            }
        };
    }
    out
}

/// 코드가 아닌 한 글자를 그 구간의 운명대로 내보낸다 — `Blank` 는 공백으로 덮되
/// 줄바꿈만 그대로 둬서 줄 번호를 지킨다.
fn emit(out: &mut String, c: char, fate: Fate) {
    match fate {
        Fate::Blank => out.push(if c == '\n' { '\n' } else { ' ' }),
        Fate::BlankBytes if c == '\n' => out.push('\n'),
        Fate::BlankBytes => out.extend(std::iter::repeat_n(' ', c.len_utf8())),
        Fate::Keep => out.push(c),
        // `x` 인 이유는 폭이 같은 아무 글자면 되기 때문이다 — 계측기가 이 자리를
        // 문자열의 시작으로 안 읽기만 하면 된다.
        Fate::QuoteSafe => out.push(if c == '"' { 'x' } else { c }),
    }
}

fn mask_line_comment(chars: &[char], mut i: usize, out: &mut String, fate: Fate) -> usize {
    while i < chars.len() && chars[i] != '\n' {
        emit(out, chars[i], fate);
        i += 1;
    }
    i
}

fn mask_block_comment(chars: &[char], mut i: usize, out: &mut String, fate: Fate) -> usize {
    let mut depth = 0usize;
    while i < chars.len() {
        let opening = chars[i] == '/' && chars.get(i + 1) == Some(&'*');
        let closing = chars[i] == '*' && chars.get(i + 1) == Some(&'/');
        if opening || closing {
            depth = if opening { depth + 1 } else { depth - 1 };
            emit(out, chars[i], fate);
            emit(out, chars[i + 1], fate);
            i += 2;
            if closing && depth == 0 {
                break;
            }
        } else {
            emit(out, chars[i], fate);
            i += 1;
        }
    }
    i
}

fn mask_raw_string(chars: &[char], i: usize, out: &mut String, fate: Fate) -> usize {
    let (quote, hashes) = raw_string_hashes(chars, i).expect("호출 전에 확인했다");
    // 접두사(`r` / `br` / `#`)는 코드다 — 여는 따옴표부터 덮는다.
    for c in &chars[i..quote] {
        out.push(*c);
    }
    let mut i = quote;
    emit(out, chars[i], fate);
    i += 1;
    while i < chars.len() {
        if chars[i] == '"' && chars[i + 1..].iter().take(hashes).all(|c| *c == '#') {
            for _ in 0..=hashes {
                if i < chars.len() {
                    emit(out, chars[i], fate);
                    i += 1;
                }
            }
            break;
        }
        emit(out, chars[i], fate);
        i += 1;
    }
    i
}

/// `terminator` 로 닫히는 리터럴(문자열·문자)을 그 운명대로 내보낸다. 역슬래시
/// 이스케이프를 따른다 — 여닫는 따옴표도 리터럴의 일부라 같은 운명이다.
fn mask_quoted(
    chars: &[char],
    mut i: usize,
    terminator: char,
    out: &mut String,
    fate: Fate,
) -> usize {
    emit(out, chars[i], fate);
    i += 1;
    while i < chars.len() {
        if chars[i] == '\\' {
            emit(out, chars[i], fate);
            i += 1;
            if i < chars.len() {
                emit(out, chars[i], fate);
                i += 1;
            }
            continue;
        }
        let done = chars[i] == terminator;
        emit(out, chars[i], fate);
        i += 1;
        if done {
            break;
        }
    }
    i
}

/// `i` 가 raw string 접두사(`r"`, `r#"`, `br"`, `br#"` …)의 시작이면 여는 `"` 의
/// 인덱스와 `#` 개수를 돌려준다.
fn raw_string_hashes(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let hash_start = j;
    while chars.get(j) == Some(&'#') {
        j += 1;
    }
    if chars.get(j) == Some(&'"') {
        Some((j, j - hash_start))
    } else {
        None
    }
}

/// `'` 가 문자 리터럴의 시작인지(아니면 라이프타임 틱인지) 가른다.
/// `'\n'` 처럼 이스케이프로 시작하거나, 두 칸 뒤가 닫는 따옴표면 문자 리터럴이다.
fn is_char_literal(chars: &[char], i: usize) -> bool {
    chars.get(i + 1) == Some(&'\\') || chars.get(i + 2) == Some(&'\'')
}

/// 스캔 루트 아래의 모든 `.rs` 를 (레포 상대 경로, LF 정규화된 내용)으로 모은다.
/// 빌드 산출물(`target/`)은 루트 밑에 없지만, 크레이트별 `target/` 이 생길 수 있어
/// 이름으로 한 번 더 뺀다.
///
/// **읽기 실패는 panic 이다.** 스캔 가드에서 조용히 건너뛰면 모수가 줄고, 줄어든 모수는
/// 언제나 초록이다.
/// 레포 상대 경로를 **구분자까지 정규화**해 돌려준다 — 언제나 `/` 다.
///
/// 소비자 대부분은 이 경로를 `to_string_lossy()` 로 펴서 **소스에 박힌 `/` 리터럴**
/// (명부의 좌표, 접두사)과 문자열로 비교한다. 그런데 `strip_prefix` 가 돌려주는 것은
/// **그 플랫폼의 구분자**라, Windows 에서는 같은 파일이 `crates\\x\\y.rs` 로 펴져
/// 어떤 리터럴과도 안 맞는다. 그 어긋남은 예외가 아니라 **조용한 0** 이다 —
/// 명부 조회가 전부 빗나가고, 가드는 "명부에 없다" 고 보고한다.
///
/// 2026-09-06 실측: 갤러리 사본 판정이 Windows 에서만 11 건을 미등록으로 잡았다.
/// 같은 커밋이 Linux 에서는 초록이었다 — 판정의 입력이 트리뿐인데도 플랫폼이 답을 갈랐다.
///
/// `/` 를 담은 `PathBuf` 는 Windows 에서도 그대로 열린다(std 가 두 구분자를 다 받는다).
/// 그래서 소비자가 `repo_root().join(rel)` 로 다시 여는 경로도 안 깨진다.
/// **다른 생산자도 이걸 써라.** 레포 상대 경로를 문자열로 펴서 비교하는 자리는 이
/// 저장소에 여럿이고, 각자 손으로 `replace('\\', "/")` 를 붙이거나 안 붙인다. 안 붙인
/// 자리는 Windows 에서만 조용히 빗나가고, 그 빗나감은 예외가 아니라 0 이다.
/// 규칙을 한 벌만 두려고 공개한다.
pub fn repo_relative(rel: &std::path::Path) -> PathBuf {
    let joined: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    PathBuf::from(joined.join("/"))
}

pub fn rust_sources(root: &std::path::Path, scan_roots: &[&str]) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = scan_roots.iter().map(|r| root.join(r)).collect();
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("스캔 루트를 읽을 수 없다: {} — {e}", dir.display()));
        for entry in entries {
            let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
            let path = entry.path();
            let file_type = entry.file_type().expect("파일 종류를 알 수 없다");
            if file_type.is_dir() {
                if entry.file_name() == "target" {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let rel = repo_relative(
                    path.strip_prefix(root)
                        .expect("스캔 경로는 레포 안이어야 한다"),
                );
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("소스를 읽을 수 없다: {} — {e}", path.display()));
                out.push((rel, text.replace("\r\n", "\n")));
            }
        }
    }
    out
}

/// `code` 안에 `name!` 매크로 **호출**이 있는가. 이름 앞의 경계를 본다.
///
/// 경계를 안 보면 `eprintln!` 이 `println` 을 담아 **stderr 를 stdout 으로 센다.**
/// 실측 2026-09-08: `git grep 'println!' -- src/` 가 17 을 냈는데 그중 3 이 `eprintln!`
/// 이었고, 그 17 을 근거로 ADR-0101 의 "현재는 없음" 이 낡았다고 의심했다 — 실제 값은
/// 0 이었다. 부분문자열로 세면 방향이 한쪽으로만 틀린다(더 많이 잡는다).
///
/// 입력은 **주석·문자열이 지워진 코드**여야 한다 — 이 함수는 그것을 안 한다.
/// [`mask_non_code`] 가 그 일을 한다.
pub fn invokes_macro(code: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = code[from..].find(name) {
        let at = from + pos;
        let end = at + name.len();
        let prev = code[..at].chars().next_back();
        let boundary_before = !prev.is_some_and(|c| c.is_alphanumeric() || c == '_');
        if boundary_before && code[end..].starts_with('!') {
            return true;
        }
        from = end;
    }
    false
}

/// 값만으로 "로케일 무관 영어가 아니다" 가 확정되는 문자.
///
/// **이 술어가 답하는 것은 좁다** — 한글·가나·한자가 한 글자라도 있으면 그 문자열은
/// 어떤 로케일에서도 영어가 아니다. 반대는 성립하지 않는다: ASCII 라고 영어인 것도,
/// 번역문이 아닌 것도 아니다(스페인어·터키어는 전부 이 범위 밖이다). 그래서 이것은
/// **판정의 전부가 아니라 값만으로 끝나는 부분집합**이다.
///
/// 두 가드가 같은 물음을 물어서 여기 둔다 — 번들 plugin 프로덕션 코드의 로케일 고정
/// 문구와, 진단 로그에 실릴 문자열의 로케일 무관성. 각자 사본을 두면 한쪽에 범위를
/// 더해도 다른 쪽은 모른다.
pub fn is_locale_specific(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF   // Hangul Jamo
        | 0x3040..=0x30FF // Hiragana · Katakana
        | 0x3130..=0x318F // Hangul Compatibility Jamo
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7A3 // Hangul Syllables
    )
}

#[cfg(test)]
mod repo_relative_tests {
    /// **이 단정을 재는 채널은 `check-windows` 하나다** (`cargo test --workspace --lib`).
    ///
    /// Linux·macOS 에서는 `join` 이 이미 `/` 를 내므로 이 단정은 언제나 참이고, 거기서
    /// 나오는 초록은 정규화가 살아 있다는 증거가 **아니다**. 채널 이름을 안 적어 두면
    /// 다음 사람이 그 초록을 증거로 읽는다 — 실측으로, 구분자 결함은 세 게이트가 전부
    /// 초록인 채로 두 번 살아남았고 `check-windows` 하나만이 답했다.
    #[test]
    fn repo_relative_always_yields_forward_slashes() {
        let p = std::path::PathBuf::from("crates")
            .join("tasty-doc-guards")
            .join("src")
            .join("lib.rs");
        assert_eq!(
            super::repo_relative(&p).to_string_lossy(),
            "crates/tasty-doc-guards/src/lib.rs"
        );
    }
}

#[cfg(test)]
mod neutralize_tests {
    use super::{mask_literals, mask_non_code, neutralize_char_literal_quotes};

    /// 이 함수가 생긴 형태. 계측기(`tokei`)는 `'"'` 의 따옴표를 문자열의 시작으로 읽고
    /// 그 뒤 파일 끝까지를 문자열 안으로 본다 — 그 오독을 끊는 것이 전부다.
    #[test]
    fn a_quote_inside_a_char_literal_is_swapped() {
        assert_eq!(
            neutralize_char_literal_quotes("let c = '\"';"),
            "let c = 'x';"
        );
    }

    /// **밖은 원문 그대로다.** 여기서 문자열이나 주석까지 건드리면 이 사본을 세는
    /// 게이트가 원문과 다른 것을 재게 된다.
    #[test]
    fn nothing_outside_a_char_literal_moves() {
        for src in [
            "let s = \"따옴표 \\\" 를 담은 문자열\";",
            "// 주석 안의 '\"' 도 그대로다",
            "let r = r#\"raw \"안\" 따옴표\"#;",
            "fn f<'a>(x: &'a str) -> &'a str { x }",
            "let c = 'a'; let n = '\\n'; let q = '\\'';",
        ] {
            assert_eq!(
                neutralize_char_literal_quotes(src),
                src,
                "밖을 건드렸다: {src}"
            );
        }
    }

    /// 줄 수는 안 움직인다 — 계측기가 이 사본의 좌표를 원본으로 읽는다.
    #[test]
    fn the_line_count_is_unchanged() {
        let src = "fn a() {\n    let c = '\"';\n}\n\nfn b() {}\n";
        let got = neutralize_char_literal_quotes(src);
        assert_eq!(got.split('\n').count(), src.split('\n').count());
        assert!(got.contains("fn b() {}"), "코드가 사라졌다: {got}");
    }

    /// 세 모드는 서로의 자리를 안 밟는다 — 한 렉서를 나눠 쓰므로 함께 본다.
    #[test]
    fn the_three_modes_keep_their_own_scopes() {
        let src = "let c = '\"'; // 주석";
        assert_eq!(mask_non_code(src), "let c =    ;      ");
        assert_eq!(mask_literals(src), "let c =    ; // 주석");
        assert_eq!(neutralize_char_literal_quotes(src), "let c = 'x'; // 주석");
    }
}
