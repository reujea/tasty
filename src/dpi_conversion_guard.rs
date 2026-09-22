//! DPI 변환이 `PhysicalPx`/`LogicalPx` 의 변환 API 밖에서 일어나는 것을 막는 드리프트 가드.
//!
//! `docs/concepts/typed-length.md` 는 두 좌표계를 섞는 것을 타입이 막는다고 규정한다.
//! 타입이 실제로 막는 것은 **혼합**(`PhysicalPx + LogicalPx`)이고, **변환 누락**은 막지
//! 못한다 — `PhysicalPx(x * ppp)` 에서 `* ppp` 를 빠뜨려도 그대로 컴파일된다. 그래서
//! 변환은 `to_physical(sf)` / `to_logical(sf)` 로만 하고, 그 밖의 수동 산술을 이 가드가
//! 잡는다.
//!
//! # 분담은 셋이다 — 이 가드는 그중 하나다
//!
//! 1. **두 좌표계를 섞는 것** (`PhysicalPx + LogicalPx`) — 컴파일러.
//! 2. **변환을 빠뜨리는 것** (`.value()` 뒤의 수동 scale factor 산술) — 이 가드.
//! 3. **애초에 타입을 안 쓰고 선언하는 것** (`const W: f32 = 96.0;`) —
//!    `src/source_guards/length_constant_frontier.rs`.
//!
//! 셋째가 따로 있는 이유는 앞의 둘이 **이미 타입이 붙은 값**에만 걸리기 때문이다.
//! 처음부터 `f32` 로 선언된 길이는 섞일 두 타입도, 벗길 `.value()` 도 없어 둘 다
//! 조용히 통과한다.
//!
//! # 왜 `tests/` 가 아니라 여기인가 — 되돌리지 마라
//!
//! 레포의 소스 스캔 가드는 관례상 `tests/*.rs` 에 모여 있다. 이 가드만 크레이트 안의
//! 유닛 테스트인 것은 관례를 몰라서가 아니라 **실행 채널이 다르기 때문**이다.
//!
//! 통합 테스트(`tests/*.rs`)와 크레이트 안 유닛 테스트는 **컴파일 축과 실행 축이 다르게**
//! 자동화돼 있다. 어느 축이 어디서 도는지는 [`docs/dev-guide/ci-gates.md`] 가 정본이다 —
//! 여기에 그 표를 복제하지 않는다(복제하면 정본이 바뀔 때 이 주석만 낡는다).
//!
//! 이 가드에 걸리는 것은 **두 축의 차이**다. 통합 테스트는 컴파일 축이 자동이지만 실행 축은
//! 그렇지 않고, 크레이트 안 유닛 테스트는 실행 축까지 자동이다. 일반 테스트에게 "컴파일은
//! 자동으로 본다" 는 부분적 안전망이 되지만, **소스를 런타임에 읽어 스캔하는 가드에게는
//! 0** 이다 — 스캔 로직은 컴파일돼도 실행되지 않으면 아무것도 안 본다. 드리프트 가드의
//! 값어치는 "지금 위반 0건" 이 아니라 "나중에 다시 새면 잡힌다" 이므로, 사람이 기억해야
//! 도는 채널에 두면 규칙이 문서에 관례로만 적혀 있는 상태와 실효가 같다.
//!
//! 그래서 여기에 있다. `tests/` 로 옮기면 **조용히 실행 축을 잃는다** — 옮긴 쪽은 초록으로
//! 보이고, 아무도 그것이 실행되지 않는다는 사실을 모른다.
//!
//! 소재를 루트 패키지로 잡은 이유는 둘이다. ① 스캔 범위가 워크스페이스 전역이라, 리프
//! 타입 크레이트가 자기 소비자들을 검사하면 의존 방향이 뒤집힌다. ② 루트 패키지의
//! `CARGO_MANIFEST_DIR` 이 곧 레포 루트라 상향 경로(`../..`)가 필요 없다 — 상향 경로가
//! 틀리면 스캔 대상이 0개가 되고 가드가 조용히 초록이 되는데, 그 위험 자체를 없앤다.
//!
//! # 판정은 순수 함수 세 층이다
//!
//! 파일 순회·읽기(부수효과)와 **판정**을 나눈다. 판정은 [`mask_non_code`] → [`violations`]
//! → [`verdict`] 세 순수 함수뿐이고, **면제도 전부 이 셋 안에** 있다. 그래서 면제를 겨냥한
//! 변이를 레포 트리를 실제로 고치지 않고 **합성 문자열**로 먹여 커밋되는 테스트로 붙박을 수
//! 있다. 면제를 하나 늘리면 그것을 겨냥한 변이도 같이 늘려라 — 면제 로직이 검출 로직보다
//! 넓어지는 순간 정밀화가 곧 무력화가 된다.
//!
//! 면제는 셋이다. ① 주석, ② 문자열 리터럴, ③ [`ALLOWED`]/[`PENDING_PORT`] 표.
//!
//! **①②를 줄 앞머리가 아니라 문자 단위 상태 기계로 판정하는 이유**: `*` 로 시작하는 줄을
//! 블록 주석 이어짐으로 보면 역참조 대입(`*out = x * ppp;`)이 통째로 빠진다. 이 레포에 `*`
//! 로 시작하는 **코드** 줄이 수백 개다. 주석 안인지 여부는 파일을 순회하며 `/*`…`*/` 상태를
//! 들고 가야만 갈린다.
//!
//! **②(문자열 리터럴)는 검출을 깎지 않는다** — 리터럴 안의 산술은 실행되지 않는다. 대신
//! 이 파일이 **자기 자신을 면제할 필요를 없앤다**: 탐지기 테스트의 예제(`"x * ppp"`)가
//! 리터럴이라 코드로 세지 않는다. 그래서 파일 통째 제외(self-exemption)는 **없다**.
//!
//! # Windows 에서도 돈다
//!
//! 유닛 테스트를 도는 잡 중 하나는 Windows 러너다. 경로는 `Path` API 로 다루고 구분자를
//! `/` 로 정규화한다. 줄 끝의 `\r`(CRLF)은 판정 전에 떼어낸다. 이걸 놓치면 Windows
//! 잡만 빨개지고, 그 잡은 아무도 자기 변경 것으로 보지 않는다.
//!
//! [`docs/dev-guide/ci-gates.md`]: ../docs/dev-guide/ci-gates.md

use std::path::{Path, PathBuf};

/// 스캔이 최소한 이만큼은 파일을 봐야 한다. 경로가 틀어져 대상이 줄면 위반이 0건이 되어
/// 가드가 **조용히 초록**이 된다 — 그 실패를 잡으려고 설계한 장치가 이 하한이다.
///
/// 파일 수를 여기 적지 않는다. 적었던 판은 공용 모수가 움직여도 안 따라와 여러 회차
/// 낡아 있었다. 대신 성질을 적는다 — 시험이 재는 성질에는 그 시험 이름을, 시험이 없는
/// 성질에는 재는 법을 붙인다:
///
/// - `src` 한쪽만 훑는 사고는 이 하한에 걸린다 — `src/` 의 `.rs` 수가 하한 아래라는
///   것을 `a_src_only_scan_falls_below_the_floor` 가 **지금 트리를 세어** 고정한다.
/// - ★ `crates` 한쪽만 훑는 사고는 이 하한이 **못 잡는다** — `crates/` 의 `.rs` 수가
///   이미 하한을 넘었다. 재는 법: `find -L crates -name '*.rs' -type f | wc -l` 을 이
///   하한과 견준다(순회가 심볼릭 링크를 따라가므로 `-L`). 그래도 가드 전체로는 지금
///   잡힌다: [`ALLOWED`]·[`PENDING_PORT`] 에 `src/` 행이 남아 있는 동안은 그 행의
///   **존재 검사**("등재가 있는데 스캔 대상에 없다")가 실패시킨다. 설계된 채널이 아니라
///   부수 효과이고, 시험으로 고정하지 않았으며, 두 표의 `src/` 행이 0 이 되면 사라진다.
///   재는 법: `scan()` 의 `src` 순회 줄을 주석 처리하고
///   `dpi_conversion_goes_through_the_typed_api` 를 돌려, 하한 단정이 아니라 `verdict`
///   단정이 그 존재 검사 문구로 실패하는지 본다.
const MIN_SCANNED_FILES: usize = 800;

/// 수동 DPI 산술이 남아도 되는 자리 — `(경로, 건수, 사유)`.
///
/// 건수까지 고정하는 이유: 파일 단위로만 면제하면 그 파일이 새 위반을 들여도 안 잡힌다.
/// 실제 건수와 다르면 실패하므로, 이식으로 줄었을 때도 이 표를 갱신하게 된다.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "crates/tasty-type-geometry/src/length.rs",
        2,
        "변환 API 본체 — `to_logical`/`to_physical` 이 실제로 나누고 곱하는 자리",
    ),
    (
        "src/host_api/webview.rs",
        8,
        "`WebViewBounds` 짝 타입의 변환 본체. 세 파일에 흩어져 있던 산술을 여기로 모은 \
         초크포인트라, 여기 산술이 있는 것이 정상이다",
    ),
    (
        "crates/tasty-settings/src/appearance.rs",
        1,
        "폰트 크기 스케일 — 길이가 아니라 글자 크기라 두 좌표계 어느 쪽도 아니다",
    ),
    (
        "crates/tasty-plugin-sdk/src/egui_surface.rs",
        2,
        "plugin SDK 는 `tasty-type-geometry` 에 의존하지 않는다(별도 프로세스의 공개 \
         API 표면). 타입 API 를 쓸 수 없어 산술이 유일한 수단이다",
    ),
];

/// 아직 타입 API 로 옮기지 못한 자리 — `(경로, 건수)`. **지금은 비어 있다.**
///
/// 비었다는 것이 이 목록의 정상 상태다. 여기에 항목이 생겼다면 새 변환 경로가 타입 API
/// 밖에 만들어졌다는 뜻이므로, 등재하기 전에 먼저 옮길 수 있는지 본다.
///
/// [`ALLOWED`] 와 분리해 둔 이유는 성격이 반대이기 때문이다. `ALLOWED` 는 "여기 산술이
/// 있는 것이 정상" 이라 영구적이고, 이쪽은 "아직 못 옮겼다" 라 없어져야 한다. 한 표에
/// 섞으면 다음 사람이 둘을 구분하지 못해 미이식분이 영구 면제로 굳는다.
const PENDING_PORT: &[(&str, usize)] = &[];

/// 수동 변환으로 보는 식별자. `pixels_per_point` 는 메서드 호출 형태(`ppp()` )로도 쓰인다.
const CONVERSION_IDENTS: &[&str] = &["ppp", "scale_factor", "sf", "pixels_per_point"];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 주석과 문자열 리터럴을 같은 자리수의 공백으로 덮는다 — **줄 구조는 보존**한다(줄 번호가
/// 어긋나면 실패 메시지가 엉뚱한 줄을 가리킨다).
///
/// 줄 앞머리 휴리스틱이 아니라 문자 단위 상태 기계인 이유는 모듈 doc 참조. 러스트의 다음
/// 형태를 모두 가른다: 줄 주석, **중첩되는** 블록 주석, 문자열(이스케이프 포함), raw
/// 문자열(`r#"…"#`), 문자 리터럴, 그리고 **문자 리터럴처럼 생긴 라이프타임**(`&'a`).
/// 마지막 것을 놓치면 `'a` 부터 다음 `'` 까지가 통째로 먹혀 그 사이 코드가 안 보인다.
fn mask_non_code(source: &str) -> String {
    let src: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;

    // 원문의 `i..end` 를 공백으로 덮되 줄바꿈만 남긴다.
    let blank = |out: &mut String, src: &[char], from: usize, to: usize| {
        for &c in &src[from..to] {
            out.push(if c == '\n' { '\n' } else { ' ' });
        }
    };

    while i < src.len() {
        let c = src[i];

        // 줄 주석 — 줄 끝까지.
        if c == '/' && src.get(i + 1) == Some(&'/') {
            let mut j = i;
            while j < src.len() && src[j] != '\n' {
                j += 1;
            }
            blank(&mut out, &src, i, j);
            i = j;
            continue;
        }

        // 블록 주석 — 러스트는 중첩을 허용하므로 깊이를 센다.
        if c == '/' && src.get(i + 1) == Some(&'*') {
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < src.len() && depth > 0 {
                if src[j] == '/' && src.get(j + 1) == Some(&'*') {
                    depth += 1;
                    j += 2;
                } else if src[j] == '*' && src.get(j + 1) == Some(&'/') {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut out, &src, i, j);
            i = j;
            continue;
        }

        // raw 문자열 — `r"…"` / `r#"…"#` / `br##"…"##`. `#` 뒤가 `"` 가 아니면
        // raw 식별자(`r#type`)라 문자열이 아니다.
        if c == 'r'
            && !src[..i]
                .last()
                .is_some_and(|&p| is_ident_char(p) && p != 'b')
        {
            let mut hashes = 0;
            while src.get(i + 1 + hashes) == Some(&'#') {
                hashes += 1;
            }
            if src.get(i + 1 + hashes) == Some(&'"') {
                let mut j = i + 2 + hashes;
                loop {
                    if j >= src.len() {
                        break;
                    }
                    if src[j] == '"' && (1..=hashes).all(|k| src.get(j + k) == Some(&'#')) {
                        j += 1 + hashes;
                        break;
                    }
                    j += 1;
                }
                blank(&mut out, &src, i, j);
                i = j;
                continue;
            }
        }

        // 일반 문자열 — 여러 줄에 걸칠 수 있다.
        if c == '"' {
            let mut j = i + 1;
            while j < src.len() && src[j] != '"' {
                j += if src[j] == '\\' { 2 } else { 1 };
            }
            j = (j + 1).min(src.len());
            blank(&mut out, &src, i, j);
            i = j;
            continue;
        }

        // 문자 리터럴 대 라이프타임. `'a>` 는 라이프타임이라 그냥 코드로 흘린다.
        if c == '\'' {
            let escaped = src.get(i + 1) == Some(&'\\');
            let plain = src.get(i + 2) == Some(&'\'');
            if escaped || plain {
                let mut j = i + 1;
                while j < src.len() && src[j] != '\'' {
                    j += if src[j] == '\\' { 2 } else { 1 };
                }
                j = (j + 1).min(src.len());
                blank(&mut out, &src, i, j);
                i = j;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }
    out
}

/// `line` 안에서 `ident` 가 **낱말로** 나타나고 그 바로 앞이나 뒤에 `*` 또는 `/` 가 붙은
/// 횟수. `to_logical(scale_factor)` 처럼 인자로 넘기는 형태는 세지 않는다.
fn conversion_hits(line: &str, ident: &str) -> usize {
    let mut hits = 0;
    let mut from = 0;
    while let Some(rel) = line[from..].find(ident) {
        let start = from + rel;
        let end = start + ident.len();
        from = end;

        let before_is_ident = line[..start].chars().next_back().is_some_and(is_ident_char);
        let after_is_ident = line[end..].chars().next().is_some_and(is_ident_char);
        if before_is_ident || after_is_ident {
            continue; // `appp` / `scale_factor_x` 같은 다른 식별자의 일부
        }

        // 앞쪽: 공백을 건너뛴 첫 글자가 연산자인가. `x / ctx.pixels_per_point()` 처럼
        // 메서드 호출이면 수신자 경로(`ctx.`)를 먼저 건너뛰어야 연산자에 닿는다.
        let mut before = &line[..start];
        if before.ends_with('.') {
            before = before.trim_end_matches('.');
            before = before.trim_end_matches(|c| is_ident_char(c) || c == '.');
        }
        let prev_op = before
            .chars()
            .rev()
            .find(|c| !c.is_whitespace())
            .is_some_and(|c| c == '*' || c == '/');
        // 뒤쪽: 메서드 호출이면 `()` 를 건너뛴 뒤의 첫 글자를 본다.
        let mut rest = &line[end..];
        if rest.starts_with("()") {
            rest = &rest[2..];
        }
        let next_op = rest
            .chars()
            .find(|c| !c.is_whitespace())
            .is_some_and(|c| c == '*' || c == '/');

        if prev_op || next_op {
            hits += 1;
        }
    }
    hits
}

/// 마스킹된 소스에서 수동 변환이 있는 **1-based 줄 번호**. 한 줄에 여러 건이면 그 줄이
/// 건수만큼 들어가므로 `len()` 이 곧 건수다.
///
/// 입력은 반드시 [`mask_non_code`] 를 거친 것이어야 한다 — 이 함수는 주석/리터럴을 모른다.
fn violations(masked: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, line) in masked.lines().enumerate() {
        // CRLF 체크아웃(Windows 러너)에서 줄 끝 `\r` 이 판정에 섞이지 않게 떼어낸다.
        let line = line.trim_end_matches('\r');
        let hits: usize = CONVERSION_IDENTS
            .iter()
            .map(|ident| conversion_hits(line, ident))
            .sum();
        out.extend(std::iter::repeat_n(idx + 1, hits));
    }
    out
}

/// 스캔 결과를 면제표와 대조해 **불평 목록**을 낸다. 비어 있으면 통과.
///
/// 표를 인자로 받는 이유: 실제 레포 트리를 고치지 않고 합성 입력으로 면제 로직 자체에
/// 변이를 먹일 수 있게 하기 위해서다(모듈 doc 참조).
fn verdict(
    scanned: &[(String, Vec<usize>)],
    allowed: &[(&str, usize, &str)],
    pending: &[(&str, usize)],
) -> Vec<String> {
    let mut complaints = Vec::new();

    let mut offenders: Vec<(&str, &Vec<usize>)> = scanned
        .iter()
        .filter(|(rel, lines)| {
            !lines.is_empty()
                && !allowed.iter().any(|(p, _, _)| *p == rel.as_str())
                && !pending.iter().any(|(p, _)| *p == rel.as_str())
        })
        .map(|(rel, lines)| (rel.as_str(), lines))
        .collect();
    offenders.sort();
    if !offenders.is_empty() {
        complaints.push(format!(
            "DPI 변환을 수동 산술로 하는 자리가 있다. `LogicalPx::to_physical(sf)` / \
             `PhysicalPx::to_logical(sf)` 를 거쳐라 — 산술이 정당한 자리면 사유와 함께 \
             이 파일의 ALLOWED 에 등재한다(docs/concepts/typed-length.md).\n\
             위반(파일: 줄 번호): {offenders:#?}"
        ));
    }

    let count_of = |path: &str| {
        scanned
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, lines)| lines.len())
    };

    // 역방향 ① — 영구 면제. 건수가 다르면(줄었든 늘었든) 표가 낡은 것이다.
    for (path, expected, reason) in allowed {
        match count_of(path) {
            None => complaints.push(format!(
                "ALLOWED 에 `{path}` 가 있는데 스캔 대상에 없다 — 파일이 옮겨졌거나 \
                 지워졌다. 등재를 지워라. (사유: {reason})"
            )),
            Some(actual) if actual != *expected => complaints.push(format!(
                "`{path}` 의 수동 산술이 {actual}건인데 ALLOWED 는 {expected}건으로 \
                 적혀 있다. 늘었으면 그 증가가 정말 사유({reason})에 해당하는지 \
                 확인하고, 줄었으면 숫자를 낮춰라."
            )),
            Some(_) => {}
        }
    }

    // 역방향 ② — 미이식 목록. 0 이 되면 행을 지워야 목록이 실제로 수렴한다.
    for (path, expected) in pending {
        match count_of(path) {
            None => complaints.push(format!(
                "PENDING_PORT 에 `{path}` 가 있는데 스캔 대상에 없다 — 파일이 \
                 옮겨졌거나 지워졌다. 등재를 지워라."
            )),
            Some(0) => complaints.push(format!(
                "`{path}` 의 수동 산술이 0건이다 — 이식이 끝났으니 PENDING_PORT \
                 에서 그 행을 지워라. 남겨두면 이 파일이 나중에 다시 새도 안 잡힌다."
            )),
            Some(actual) if actual != *expected => complaints.push(format!(
                "`{path}` 의 수동 산술이 {actual}건인데 PENDING_PORT 는 {expected}건 \
                 으로 적혀 있다. 이식으로 줄었으면 숫자를 낮춰라."
            )),
            Some(_) => {}
        }
    }

    complaints
}

/// 디렉토리 순회. **실패를 삼키지 않는다** — 못 읽은 디렉토리는 위반이 없는 디렉토리와
/// 구분되지 않으므로, 조용히 건너뛰면 가드가 초록인 채로 눈이 먼다.
/// 매니페스트가 `name` 을 **의존으로 선언**하는가. 주석이나 산문 언급은 세지 않는다 —
/// 단순 문자열 포함으로 보면 "이 크레이트에 의존하지 않는다" 는 주석 한 줄이 판정을
/// 뒤집는다.
fn declares_dependency(manifest: &str, name: &str) -> bool {
    manifest.lines().any(|l| {
        let l = l.trim_start();
        l.strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "스캔 디렉토리 `{}` 를 열지 못했다: {e}. 건너뛰면 그 하위의 위반이 \
             통째로 안 보인 채 가드가 초록이 된다.",
            dir.display()
        )
    });
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|e| panic!("`{}` 의 항목을 읽지 못했다: {e}.", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 레포 루트 기준 상대경로를 `/` 구분자로 정규화한다 — Windows 러너에서 `\` 로 나오는
/// 것을 그대로 비교하면 allowlist 가 전부 빗나간다.
fn relative_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// 파일 순회 + 읽기(부수효과 층). 판정은 하지 않고 `(상대경로, 위반 줄 번호)` 만 낸다.
fn scan(root: &Path) -> Vec<(String, Vec<usize>)> {
    let mut files = Vec::new();
    collect_rs(&root.join("src"), &mut files);
    collect_rs(&root.join("crates"), &mut files);
    files.sort();
    files
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
                panic!(
                    "스캔 대상 `{}` 를 읽지 못했다: {e}. 건너뛰면 이 파일의 위반이 \
                     '위반 없음' 과 구분되지 않는다.",
                    path.display()
                )
            });
            (
                relative_slash(root, path),
                violations(&mask_non_code(&source)),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// 하한 문장의 `src` 쪽 성질을 **지금 트리에서 세어** 고정한다. 값을 문장에 베끼면
    /// 트리가 움직여도 안 따라온다. 좌변은 가드 본체와 같은 순회(`collect_rs`)로 센 수다 —
    /// 선언된 공용 모수 `SRC_RS` 를 좌변으로 쓰면 선언이 낡은 동안 성질이 거짓인 트리에서도
    /// 초록이 된다. 선언이 트리와 맞는가는 이 시험의 물음이 아니다
    /// (`scripts/check-population-freshness.sh` 의 물음이다). 선언 값은 진단용으로만 찍는다.
    #[test]
    fn a_src_only_scan_falls_below_the_floor() {
        let mut files = Vec::new();
        collect_rs(&repo_root().join("src"), &mut files);
        let declared = tasty_doc_guards::floored_walk::populations::SRC_RS;
        assert!(
            files.len() < MIN_SCANNED_FILES,
            "`src/` 의 `.rs` 가 지금 트리에서 {}개라 하한 {MIN_SCANNED_FILES} 이상이다(진단용: \
             선언 모수 `SRC_RS` = {}, {} 실측) — `src` 한쪽만 훑는 사고를 이 하한이 더는 못 \
             잡는다. 스캔 루트별 하한을 두거나, 합계 스캔 수 아래에서 하한을 올려라. 이 단정을 \
             지우거나 완화하지 마라 — 그러면 하한 문장의 `src` 항목이 거짓인 채 남는다.",
            files.len(),
            declared.measured,
            declared.measured_on,
        );
    }

    fn count_in_source(source: &str) -> usize {
        violations(&mask_non_code(source)).len()
    }

    /// `ALLOWED` 의 사유 중 **기계로 확인할 수 있는 것**을 확인한다.
    ///
    /// 면제의 근거가 산문이면, 그 근거가 거짓이 되어도 표는 그대로 남아 면제만
    /// 살아남는다 — 이름·경로로 면제하는 표의 공통 약점이다. 여기 걸린 셋은 사유가
    /// 사실 진술이라 검사할 수 있다. 넷째(`appearance.rs`, "길이가 아니라 글자 크기")는
    /// 의미 판단이라 검사 대상이 아니고, **검사 못 한다는 것을 여기 적어 둔다.**
    #[test]
    fn the_checkable_allowed_reasons_still_hold() {
        let root = repo_root();
        let read = |rel: &str| {
            std::fs::read_to_string(root.join(rel))
                .unwrap_or_else(|e| panic!("{rel} 을 못 읽었다: {e}"))
        };

        // ① "변환 API 본체" — 정말 그 두 함수를 여기서 정의하는가.
        let length = read("crates/tasty-type-geometry/src/length.rs");
        for f in ["fn to_physical", "fn to_logical"] {
            assert!(
                length.contains(f),
                "length.rs 가 `{f}` 를 정의하지 않는다 — ALLOWED 의 '변환 API 본체'                  사유가 거짓이 됐다. 함수가 옮겨졌으면 면제도 따라가야 한다."
            );
        }

        // ② "짝 타입의 변환 본체" — 왕복 양방향이 여기 있는가.
        let webview = read("src/host_api/webview.rs");
        for f in ["fn to_physical", "fn from_physical"] {
            assert!(
                webview.contains(f),
                "webview.rs 가 `{f}` 를 정의하지 않는다 — 초크포인트 사유가 거짓이 됐다."
            );
        }

        // ③ "plugin SDK 는 tasty-type-geometry 에 의존하지 않는다" — 사실 진술이다.
        //    의존이 생기면 타입 API 를 쓸 수 있게 되므로 면제의 근거가 사라진다.
        let sdk = read("crates/tasty-plugin-sdk/Cargo.toml");
        assert!(
            !declares_dependency(&sdk, "tasty-type-geometry"),
            "plugin SDK 가 이제 tasty-type-geometry 에 의존한다 — ALLOWED 의 사유가              거짓이 됐다. 타입 API 를 쓸 수 있으므로 그 2건을 이식하고 표에서 빼라."
        );
        // 비영 대조 — 파일을 실제로 읽었고 파서가 산다. 의존 0 과 못 읽음을 가른다.
        let declared = sdk
            .lines()
            .filter(|l| {
                let l = l.trim_start();
                l.starts_with("tasty-") && l.contains('=')
            })
            .count();
        assert!(
            declared > 0,
            "SDK 의 Cargo.toml 에서 tasty- 의존을 하나도 못 읽었다 — 파서나 경로가 틀렸다"
        );
    }

    /// `WebViewBounds::to_physical` 의 macOS `allow(dead_code)` 면제가 든 **근거**를
    /// 검사한다.
    ///
    /// 그 면제는 "지우거나 `#[cfg]` 로 빼지 않는 이유는 왕복 테스트가 세 OS 모두에서
    /// 이 함수를 `from_physical` 의 역으로 고정하기 때문" 이라고 적는다. 함수의 존재만
    /// 보면 그 근거가 거짓이 되는 것을 못 본다 — 왕복 테스트에 OS 게이트가 붙는 순간
    /// macOS 에서 고정이 사라지고, 그때 면제는 "안 쓰는 함수를 남겨 둔 것" 이 된다.
    #[test]
    fn the_macos_dead_code_exemption_still_has_its_reason() {
        let src = std::fs::read_to_string(repo_root().join("src/host_api/webview.rs"))
            .expect("webview.rs 를 못 읽었다");

        // ① 근거가 지목하는 왕복 테스트가 실재하고 두 방향을 다 부른다.
        let start = src
            .find("mod bounds_tests {")
            .expect("왕복 테스트 모듈이 없다 — 면제의 근거가 사라졌다");
        let tests = &src[start..];
        for needle in [
            "fn the_physical_round_trip_returns_the_original_rect",
            "from_physical(",
            "to_physical(",
        ] {
            assert!(
                tests.contains(needle),
                "왕복 테스트에 `{needle}` 이 없다 — 면제의 근거가 거짓이 됐다"
            );
        }

        // ② 그 테스트가 **세 OS 모두에서** 도는가. OS 게이트가 붙으면 근거가 무너진다.
        assert!(
            !tests.contains("target_os"),
            "왕복 테스트 모듈에 OS 게이트가 생겼다 — 면제의 근거('세 OS 모두에서 \
             고정한다')가 거짓이 됐다. 게이트를 빼거나 면제를 다시 판단해라."
        );

        // ③ 비영 대조 — 파일에는 OS 게이트가 실제로 있다(면제 자신이 그것이다).
        //    ②의 0 이 "게이트가 없다" 인지 "파일을 못 읽었다" 인지를 가른다.
        assert!(
            src[..start].contains("target_os"),
            "webview.rs 앞부분에 OS 게이트가 하나도 없다 — 면제 자체가 사라졌거나 \
             파일을 잘못 읽었다. 그렇다면 이 테스트의 전제부터 다시 봐라."
        );
    }

    /// 위 판정기가 살아 있는가 — 주석 언급과 진짜 선언을 가르는지 fixture 로 본다.
    /// (모수 단언과 서로를 대체하지 않는다: 이쪽은 "판정기가 죽었는가", 위는 "볼 것이
    /// 주어졌는가" 를 본다.)
    #[test]
    fn a_commented_mention_is_not_a_dependency_declaration() {
        let name = "tasty-type-geometry";
        assert!(declares_dependency(
            "tasty-type-geometry = { path = \"../tasty-type-geometry\" }",
            name
        ));
        assert!(declares_dependency(
            "  tasty-type-geometry  = \"0.1\"",
            name
        ));
        // 주석 언급은 선언이 아니다 — 이것이 문자열 포함 판정과 갈리는 자리다.
        assert!(!declares_dependency(
            "# tasty-type-geometry 에 의존하지 않는다",
            name
        ));
        assert!(!declares_dependency(
            "# tasty-type-geometry = \"0.1\"",
            name
        ));
        // 접두사가 같은 다른 크레이트를 삼키지 않는다.
        assert!(!declares_dependency(
            "tasty-type-geometry-extra = \"0.1\"",
            name
        ));
    }

    #[test]
    fn manual_arithmetic_is_counted_but_passing_the_factor_is_not() {
        // 수동 변환 — 잡아야 한다.
        assert_eq!(count_in_source("x: PhysicalPx(rect.min.x * ppp),"), 1);
        assert_eq!(count_in_source("let a = r.x.value() / scale_factor;"), 1);
        assert_eq!(count_in_source("(p.x / ppp, p.y / ppp)"), 2);
        assert_eq!(
            count_in_source("let w = px as f32 / ctx.pixels_per_point();"),
            1
        );

        // scale factor 를 **인자로 넘기는** 것은 변환 API 를 쓰는 정상 형태다.
        assert_eq!(
            count_in_source("let l = physical.to_logical(scale_factor);"),
            0
        );
        assert_eq!(count_in_source("PhysicalPx(x).to_logical(sf).value()"), 0);
        assert_eq!(
            count_in_source("RawInput { pixels_per_point: ppp, ..d }"),
            0
        );

        // 다른 식별자의 일부를 낱말로 오인하지 않는다.
        assert_eq!(count_in_source("let v = happ * 2.0;"), 0);
        assert_eq!(count_in_source("let v = scale_factor_hint * 2.0;"), 0);
    }

    #[test]
    fn violations_report_the_line_numbers() {
        let src = "let a = 1;\nlet b = x * ppp;\n// 주석\nlet c = y / sf; let d = z / ppp;\n";
        assert_eq!(violations(&mask_non_code(src)), vec![2, 4, 4]);
    }

    /// 면제 ① 을 겨냥한 변이 — 주석 면제가 코드를 먹지 않는가.
    ///
    /// 줄 앞머리로 `*` 를 주석으로 보던 판정이 잡아내지 못하던 형태다. 이 레포에 `*` 로
    /// 시작하는 코드 줄이 수백 개라, 그 판정에서는 역참조 대입이 전부 사각지대였다.
    #[test]
    fn a_deref_assignment_is_code_not_a_continued_block_comment() {
        assert_eq!(count_in_source("*out = x * ppp;"), 1);
        assert_eq!(count_in_source("    *self.w -= v / scale_factor;"), 1);

        // 진짜 블록 주석 이어짐은 여전히 주석이다.
        assert_eq!(count_in_source("/* 설명\n * x * ppp 는 금지\n */\n"), 0);
        // 블록 주석이 닫힌 **뒤**의 코드는 다시 센다.
        assert_eq!(count_in_source("/* 설명 */ let a = x * ppp;"), 1);
        // 중첩 블록 주석에서 안쪽 `*/` 로 일찍 빠져나오면 뒤가 코드로 오인된다.
        assert_eq!(count_in_source("/* a /* b */ x * ppp */ let v = 1;"), 0);
        // 줄 주석은 줄 끝까지만. 다음 줄은 코드다.
        assert_eq!(count_in_source("// x * ppp\nlet a = y * ppp;"), 1);
    }

    /// 면제 ② 를 겨냥한 변이 — 리터럴 면제가 코드를 먹지 않는가.
    ///
    /// 이 면제가 있어서 가드 파일이 자기 자신을 통째로 제외할 필요가 없다. 대신 마스커가
    /// 리터럴의 끝을 못 찾으면 그 뒤 코드가 통째로 사라지므로, 경계마다 변이를 둔다.
    #[test]
    fn string_literals_are_data_but_the_code_around_them_is_not() {
        assert_eq!(count_in_source(r#"let s = "x * ppp";"#), 0);
        // 리터럴 **밖**의 산술은 그대로 잡는다.
        assert_eq!(count_in_source(r#"let s = format!("{}", x * ppp);"#), 1);
        assert_eq!(
            count_in_source(r##"let s = r#"x * ppp"#; let a = y * ppp;"##),
            1
        );
        // 이스케이프된 따옴표에서 리터럴이 일찍 끝난 것으로 보면 뒤가 새어 나온다.
        assert_eq!(count_in_source(r#"let s = "a\" * ppp b"; let v = 1;"#), 0);
        // 문자 리터럴 안의 따옴표.
        assert_eq!(count_in_source("let c = '\"'; let a = x * ppp;"), 1);
        // **라이프타임을 문자 리터럴로 오인하면** `'a` 부터 다음 `'` 까지가 먹힌다.
        assert_eq!(
            count_in_source("fn f<'a>(x: &'a f32) -> f32 { x * ppp }"),
            1
        );
    }

    /// 의도된 false negative — 붙박아 둔다.
    ///
    /// 나중에 누가 판정기를 여기까지 넓히면 이 테스트가 **먼저 깨져서** 그 결정이 드러난다.
    #[test]
    fn known_false_negatives_are_pinned() {
        // ① 문자열 안의 산술은 실행되지 않으므로 세지 않는다.
        assert_eq!(count_in_source(r#"let doc = "쓰지 마라: x * ppp";"#), 0);
        // ② 매크로가 문자열에서 코드를 만들어 내는 형태는 못 본다. 이 레포엔 없다.
        assert_eq!(count_in_source(r#"paste::paste! { "x * ppp" }"#), 0);
        // ③ 판정 단위가 줄이라, 연산자와 식별자가 **다른 줄로** 갈리면 못 본다.
        assert_eq!(count_in_source("let a = x *\n    ppp;"), 0);
        // 다만 연산자가 식별자 쪽 줄에 남으면 줄바꿈이 있어도 잡는다 — 이쪽이 흔한 형태다.
        assert_eq!(count_in_source("let a = x\n    * ppp;"), 1);
    }

    #[test]
    fn crlf_does_not_change_the_verdict() {
        let lf = "let a = x * ppp;\nlet b = y / scale_factor;\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(count_in_source(lf), 2);
        assert_eq!(count_in_source(&crlf), count_in_source(lf));
        assert_eq!(count_in_source("// x * ppp\r\n"), 0);
    }

    /// 면제 ③ 을 겨냥한 변이 — 등재된 파일이 **새 위반**을 들이면 잡히는가.
    #[test]
    fn a_listed_file_that_gains_a_violation_is_still_caught() {
        let allowed = &[("a.rs", 2, "사유")][..];
        let listed = |n: usize| vec![("a.rs".to_string(), vec![1; n])];

        // 건수가 맞으면 조용하다.
        assert!(verdict(&listed(2), allowed, &[]).is_empty());
        // 늘면 잡는다 — 파일 단위 면제였다면 여기서 새어 나간다.
        assert!(!verdict(&listed(3), allowed, &[]).is_empty());
        // 줄어도 잡는다 — 표가 낡으면 그만큼 사각지대가 생긴다.
        assert!(!verdict(&listed(1), allowed, &[]).is_empty());
        // 등재 파일이 사라지면 잡는다.
        assert!(!verdict(&[], allowed, &[]).is_empty());
        // 등재되지 않은 파일의 위반은 그대로 잡는다.
        assert!(!verdict(&[("b.rs".to_string(), vec![7])], allowed, &[]).is_empty());
    }

    /// 면제 ③ 의 **의도된 사각지대** — 붙박아 둔다.
    #[test]
    fn a_same_count_swap_inside_a_listed_file_is_a_known_blind_spot() {
        // 등재 파일 안에서 한 건을 지우고 다른 자리에 한 건을 들이면 건수가 같아 안 잡힌다.
        // 줄 번호까지 고정하면 잡히지만, 위쪽에 한 줄만 끼어도 전부 어긋나 무관한 변경마다
        // 빨개진다 — 그 소음은 결국 이 표를 통째로 넓히는 압력이 된다. 좁히지 않기로 한
        // 결정이고, 넓히려면 이 테스트를 먼저 고쳐야 한다.
        let allowed = &[("a.rs", 2, "사유")][..];
        assert!(verdict(&[("a.rs".to_string(), vec![10, 20])], allowed, &[]).is_empty());
        assert!(verdict(&[("a.rs".to_string(), vec![33, 44])], allowed, &[]).is_empty());
    }

    #[test]
    fn the_pending_list_must_converge() {
        let pending = &[("a.rs", 2)][..];
        assert!(verdict(&[("a.rs".to_string(), vec![1, 2])], &[], pending).is_empty());
        // 0 이 되면 행을 지우라고 한다.
        assert!(!verdict(&[("a.rs".to_string(), vec![])], &[], pending).is_empty());
        // 줄었으면 숫자를 낮추라고 한다.
        assert!(!verdict(&[("a.rs".to_string(), vec![1])], &[], pending).is_empty());
    }

    #[test]
    fn dpi_conversion_goes_through_the_typed_api() {
        let root = repo_root();
        let scanned = scan(&root);

        assert!(
            scanned.len() >= MIN_SCANNED_FILES,
            "스캔한 .rs 파일이 {}개뿐이다(하한 {MIN_SCANNED_FILES}). 스캔 루트가 \
             어긋났을 때 위반 0건으로 조용히 통과하는 것을 막는 하한이다 — 위반이 \
             정말 없는 것이 아니라 아무것도 안 본 것이다.",
            scanned.len(),
        );

        let complaints = verdict(&scanned, ALLOWED, PENDING_PORT);
        assert!(complaints.is_empty(), "{}", complaints.join("\n\n---\n\n"));
    }
}
