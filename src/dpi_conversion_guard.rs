//! DPI 배율 식별자와 곱셈·나눗셈이 붙은 수동 변환을 찾아 타입 API 사용을 요구한다.
//! PhysicalPx/LogicalPx는 단위 혼합을 막지만 변환 누락 자체는 막지 못한다.
//! 처음부터 타입 없는 길이 상수는 length_constant_frontier에서 따로 검사한다.
//! 자동 실행 경로는 docs/dev-guide/ci-gates.md에 있다.
//!
//! src와 crates를 순회하고, 주석·리터럴을 마스킹한 뒤 줄별로 판정한다.
//! 연산자와 배율 식별자가 다른 줄에 있거나 매크로가 문자열에서 코드를 만드는 경우는 놓칠 수 있다.
//! 허용한 산술과 아직 옮기지 않은 산술은 별도 목록으로 관리하며 파일별 건수까지 대조한다.
//! 같은 파일에서 하나를 지우고 다른 산술을 추가해 건수가 같으면 찾지 못한다.
//! 경로는 플랫폼과 무관한 상대 경로로 비교하고 CRLF도 같은 결과여야 한다.

use std::path::{Path, PathBuf};

/// 2026-10-04의 src 801개, crates 982개를 각각 하한 700/800과 대조한다.
/// 파일 분리·정리에 여유를 두되 한 루트 누락을 다른 루트의 증가가 가리지 못하게 한다.
const ROOT_FLOORS: &[(&str, usize)] = &[("src/", 700), ("crates/", 800)];

fn coverage_complaints(paths: &[String]) -> Vec<String> {
    ROOT_FLOORS
        .iter()
        .filter_map(|(prefix, floor)| {
            let count = paths.iter().filter(|path| path.starts_with(prefix)).count();
            (count < *floor)
                .then(|| format!("{prefix} Rust 파일 {count}개가 하한 {floor}에 못 미친다"))
        })
        .collect()
}

/// 경로·건수·사유로 허용 산술을 기록한다. 실제 건수가 늘거나 줄면 목록도 검토한다.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "crates/tasty-type-geometry/src/length.rs",
        2,
        "변환 API 본체 — `to_logical`/`to_physical` 이 실제로 나누고 곱하는 자리",
    ),
    (
        "src/host_api/webview.rs",
        8,
        "WebViewBounds의 논리·물리 좌표 변환을 담당하는 공용 구현이다.",
    ),
    (
        "crates/tasty-settings/src/appearance.rs",
        1,
        "EffectiveFont의 auto 모드는 설정한 폰트 크기에 배율을 적용한다. 좌표 타입의 변환과 별개인 폰트 설정 계산으로 허용한다.",
    ),
];

/// 아직 타입 API로 옮기지 않은 항목. 먼저 이식할 수 있는지 검토하고 해결되면 제거한다.
const PENDING_PORT: &[(&str, usize)] = &[];

/// 배율로 인식할 이름. pixels_per_point의 인자 없는 메서드 호출도 포함한다.
const CONVERSION_IDENTS: &[&str] = &["ppp", "scale_factor", "sf", "pixels_per_point"];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 주석·문자열·문자 리터럴을 공백으로 바꾸고 줄 구조를 보존한다.
/// 중첩 블록 주석과 raw string, 문자와 다른 라이프타임 표기를 구별해야 뒤의 코드가 빠지지 않는다.
fn mask_non_code(source: &str) -> String {
    let src: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;

    let blank = |out: &mut String, src: &[char], from: usize, to: usize| {
        for &c in &src[from..to] {
            out.push(if c == '\n' { '\n' } else { ' ' });
        }
    };

    while i < src.len() {
        let c = src[i];

        if c == '/' && src.get(i + 1) == Some(&'/') {
            let mut j = i;
            while j < src.len() && src[j] != '\n' {
                j += 1;
            }
            blank(&mut out, &src, i, j);
            i = j;
            continue;
        }

        // Rust 블록 주석은 중첩될 수 있다.
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

        // raw 식별자와 문자열을 구별하려고 해시 뒤의 따옴표를 확인한다.
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

        // 라이프타임은 코드에 남겨야 한다.
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

/// 배율 이름 바로 앞뒤의 곱셈·나눗셈을 센다. 타입 변환 함수의 인자로 전달한 경우는 세지 않는다.
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

        // 인자 없는 메서드 호출 앞의 연산자를 찾도록 수신자 경로를 건너뛴다.
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
        // 인자 없는 메서드는 괄호 뒤의 연산자를 확인한다.
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

/// 마스킹된 코드의 1기반 줄 번호. 한 줄에 여러 산술이 있으면 건수만큼 반복한다.
fn violations(masked: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, line) in masked.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let hits: usize = CONVERSION_IDENTS
            .iter()
            .map(|ident| conversion_hits(line, ident))
            .sum();
        out.extend(std::iter::repeat_n(idx + 1, hits));
    }
    out
}

/// 수집 결과와 예외 목록을 받아 파일·건수 차이를 확인한다. 합성 입력도 같은 판정을 사용한다.
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
            "수동 DPI 산술이 있다. LogicalPx::to_physical 또는 PhysicalPx::to_logical을 사용한다. 필요한 산술은 ALLOWED에 근거를 기록한다(docs/concepts/typed-length.md). 파일과 줄 번호: {offenders:#?}"
        ));
    }

    let count_of = |path: &str| {
        scanned
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, lines)| lines.len())
    };

    for (path, expected, reason) in allowed {
        match count_of(path) {
            None => complaints.push(format!(
                "ALLOWED 에 `{path}` 가 있는데 스캔 대상에 없다 — 파일이 옮겨졌거나 \
                 지워졌다. 등재를 지워라. (사유: {reason})"
            )),
            Some(actual) if actual > *expected => complaints.push(format!(
                "`{path}` 의 수동 산술이 {actual}건으로 ALLOWED 상한 {expected}건을 넘었다. \
                 그 증가가 정말 사유({reason})에 해당하는지 확인한다. 줄어든 것은 실패가 아니다."
            )),
            Some(_) => {}
        }
    }

    for (path, expected) in pending {
        match count_of(path) {
            None => complaints.push(format!(
                "PENDING_PORT 에 `{path}` 가 있는데 스캔 대상에 없다 — 파일이 \
                 옮겨졌거나 지워졌다. 등재를 지워라."
            )),
            Some(0) => complaints.push(format!(
                "{path}의 수동 산술이 없어졌다. PENDING_PORT에서 해결된 항목을 제거한다."
            )),
            Some(actual) if actual > *expected => complaints.push(format!(
                "`{path}` 의 수동 산술이 {actual}건으로 PENDING_PORT 상한 {expected}건을 \
                 넘었다. 이식 대기 파일에 새 수동 산술을 더하지 않는다."
            )),
            Some(_) => {}
        }
    }

    complaints
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("스캔 디렉터리 {}를 열지 못했다: {e}", dir.display()));
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

/// Windows에서도 예외 경로와 일치하도록 구분자를 /로 맞춘다.
fn relative_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn scan(root: &Path) -> Vec<(String, Vec<usize>)> {
    let mut files = Vec::new();
    collect_rs(&root.join("src"), &mut files);
    collect_rs(&root.join("crates"), &mut files);
    files.sort();
    files
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("스캔 파일 {}를 읽지 못했다: {e}", path.display()));
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

    #[test]
    fn either_missing_root_is_rejected_even_when_the_other_root_grows() {
        let src: Vec<_> = (0..2000).map(|i| format!("src/{i}.rs")).collect();
        let crates: Vec<_> = (0..2000)
            .map(|i| format!("crates/example/{i}.rs"))
            .collect();
        assert_eq!(coverage_complaints(&src).len(), 1);
        assert!(coverage_complaints(&src)[0].starts_with("crates/"));
        assert_eq!(coverage_complaints(&crates).len(), 1);
        assert!(coverage_complaints(&crates)[0].starts_with("src/"));
        let both: Vec<_> = src.into_iter().chain(crates).collect();
        assert!(coverage_complaints(&both).is_empty());
    }

    fn count_in_source(source: &str) -> usize {
        violations(&mask_non_code(source)).len()
    }

    /// 변환 함수 정의처럼 코드로 확인할 수 있는 예외 근거를 검사한다.
    /// appearance의 폰트 배율 예외는 용도에 대한 판단이므로 이 검사로 확인하지 않는다.
    #[test]
    fn the_checkable_allowed_reasons_still_hold() {
        let root = repo_root();
        let read = |rel: &str| {
            std::fs::read_to_string(root.join(rel))
                .unwrap_or_else(|e| panic!("{rel} 을 못 읽었다: {e}"))
        };

        let length = read("crates/tasty-type-geometry/src/length.rs");
        for f in ["fn to_physical", "fn to_logical"] {
            assert!(
                length.contains(f),
                "length.rs에 {f} 정의가 없다. 변환 구현이 이동했다면 ALLOWED도 갱신한다."
            );
        }

        let webview = read("src/host_api/webview.rs");
        for f in ["fn to_physical", "fn from_physical"] {
            assert!(
                webview.contains(f),
                "webview.rs에 {f} 정의가 없다. 좌표 변환 구현 예외를 확인한다."
            );
        }
    }

    /// macOS에서 변환 함수를 남긴 근거인 왕복 테스트의 존재와 양방향 호출을 확인한다.
    /// 테스트 모듈에 target_os 조건이 추가되면 플랫폼 공통이라는 근거를 다시 검토해야 한다.
    #[test]
    fn the_macos_dead_code_exemption_still_has_its_reason() {
        let src = std::fs::read_to_string(repo_root().join("src/host_api/webview.rs"))
            .expect("webview.rs 를 못 읽었다");

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

        assert!(
            !tests.contains("target_os"),
            "왕복 테스트 모듈에 target_os가 있다. 플랫폼 공통 테스트라는 예외 근거를 확인하고 조건이나 예외를 다시 검토한다."
        );

        assert!(
            src[..start].contains("target_os"),
            "webview.rs 앞부분에서 OS 조건을 찾지 못했다. 파일 위치와 해당 예외의 필요성을 확인한다."
        );
    }

    #[test]
    fn manual_arithmetic_is_counted_but_passing_the_factor_is_not() {
        assert_eq!(count_in_source("x: PhysicalPx(rect.min.x * ppp),"), 1);
        assert_eq!(count_in_source("let a = r.x.value() / scale_factor;"), 1);
        assert_eq!(count_in_source("(p.x / ppp, p.y / ppp)"), 2);
        assert_eq!(
            count_in_source("let w = px as f32 / ctx.pixels_per_point();"),
            1
        );

        assert_eq!(
            count_in_source("let l = physical.to_logical(scale_factor);"),
            0
        );
        assert_eq!(count_in_source("PhysicalPx(x).to_logical(sf).value()"), 0);
        assert_eq!(
            count_in_source("RawInput { pixels_per_point: ppp, ..d }"),
            0
        );

        assert_eq!(count_in_source("let v = happ * 2.0;"), 0);
        assert_eq!(count_in_source("let v = scale_factor_hint * 2.0;"), 0);
    }

    #[test]
    fn violations_report_the_line_numbers() {
        let src = "let a = 1;\nlet b = x * ppp;\n// 주석\nlet c = y / sf; let d = z / ppp;\n";
        assert_eq!(violations(&mask_non_code(src)), vec![2, 4, 4]);
    }

    /// 줄 앞의 별표를 주석으로 오인하면 실제 역참조 대입이 빠지므로 두 경우를 함께 확인한다.
    #[test]
    fn a_deref_assignment_is_code_not_a_continued_block_comment() {
        assert_eq!(count_in_source("*out = x * ppp;"), 1);
        assert_eq!(count_in_source("    *self.w -= v / scale_factor;"), 1);

        assert_eq!(count_in_source("/* 설명\n * x * ppp 는 금지\n */\n"), 0);
        assert_eq!(count_in_source("/* 설명 */ let a = x * ppp;"), 1);
        assert_eq!(count_in_source("/* a /* b */ x * ppp */ let v = 1;"), 0);
        assert_eq!(count_in_source("// x * ppp\nlet a = y * ppp;"), 1);
    }

    /// 리터럴 끝을 잘못 읽어 뒤의 코드까지 지우지 않는지 확인한다.
    #[test]
    fn string_literals_are_data_but_the_code_around_them_is_not() {
        assert_eq!(count_in_source(r#"let s = "x * ppp";"#), 0);
        assert_eq!(count_in_source(r#"let s = format!("{}", x * ppp);"#), 1);
        assert_eq!(
            count_in_source(r##"let s = r#"x * ppp"#; let a = y * ppp;"##),
            1
        );
        assert_eq!(count_in_source(r#"let s = "a\" * ppp b"; let v = 1;"#), 0);
        assert_eq!(count_in_source("let c = '\"'; let a = x * ppp;"), 1);
        assert_eq!(
            count_in_source("fn f<'a>(x: &'a f32) -> f32 { x * ppp }"),
            1
        );
    }

    /// 문자열로 생성한 코드와 줄이 갈린 연산자는 검사하지 못한다는 한계를 유지한다.
    #[test]
    fn known_false_negatives_are_pinned() {
        assert_eq!(count_in_source(r#"let doc = "쓰지 마라: x * ppp";"#), 0);
        assert_eq!(count_in_source(r#"paste::paste! { "x * ppp" }"#), 0);
        assert_eq!(count_in_source("let a = x *\n    ppp;"), 0);
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

    /// 예외 파일도 새 산술이 추가되면 건수 비교에서 검출해야 한다.
    #[test]
    fn a_listed_file_that_gains_a_violation_is_still_caught() {
        let allowed = &[("a.rs", 2, "사유")][..];
        let listed = |n: usize| vec![("a.rs".to_string(), vec![1; n])];

        assert!(verdict(&listed(2), allowed, &[]).is_empty());
        assert!(!verdict(&listed(3), allowed, &[]).is_empty());
        assert!(verdict(&listed(1), allowed, &[]).is_empty());
        assert!(!verdict(&[], allowed, &[]).is_empty());
        assert!(!verdict(&[("b.rs".to_string(), vec![7])], allowed, &[]).is_empty());
    }

    /// 같은 파일 안에서 산술 위치가 바뀌어도 건수가 같으면 허용되는 한계다.
    #[test]
    fn a_same_count_swap_inside_a_listed_file_is_a_known_blind_spot() {
        let allowed = &[("a.rs", 2, "사유")][..];
        assert!(verdict(&[("a.rs".to_string(), vec![10, 20])], allowed, &[]).is_empty());
        assert!(verdict(&[("a.rs".to_string(), vec![33, 44])], allowed, &[]).is_empty());
    }

    #[test]
    fn the_pending_list_must_converge() {
        let pending = &[("a.rs", 2)][..];
        assert!(verdict(&[("a.rs".to_string(), vec![1, 2])], &[], pending).is_empty());
        assert!(!verdict(&[("a.rs".to_string(), vec![])], &[], pending).is_empty());
        assert!(verdict(&[("a.rs".to_string(), vec![1])], &[], pending).is_empty());
        assert!(!verdict(&[("a.rs".to_string(), vec![1, 2, 3])], &[], pending).is_empty());
    }

    #[test]
    fn dpi_conversion_goes_through_the_typed_api() {
        let root = repo_root();
        let scanned = scan(&root);

        let paths: Vec<_> = scanned.iter().map(|(path, _)| path.clone()).collect();
        let coverage = coverage_complaints(&paths);
        assert!(coverage.is_empty(), "{}", coverage.join("\n"));

        let complaints = verdict(&scanned, ALLOWED, PENDING_PORT);
        assert!(complaints.is_empty(), "{}", complaints.join("\n\n---\n\n"));
    }
}
