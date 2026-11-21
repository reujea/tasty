//! 사용자 입력 재현을 debug 빌드에만 제공한다는 규칙을 CLI 이름·메서드 표·라우터 소스로 확인한다.
//! 이름이나 debug CLI에 단서가 없는 새 메서드의 의미는 사람이 검토해야 한다.
//! 로컬 호출자는 권한 표 검사에서 허용되므로 표에서 이름을 빼는 것만으로 라우터 접근이 차단되지는 않는다.
//! 등록된 함수 본문만 읽는다. 핸들러를 다른 함수로 옮기면 RELEASE_ROUTERS 목록도 확인한다.
//!
//! 이 검사는 DEBUG_METHODS와 비교하므로 debug 빌드에서만 실행한다.
//! 자동 실행은 헤드리스 조합에서만 일어난다(check-headless). 기본 조합의 --lib --bins 호출에는 포함되지 않는다.
//! Windows·헤드리스·Linux gui의 clippy --all-targets는 컴파일 검사이며 실행과 구별한다.
#![cfg(debug_assertions)]

use tasty_doc_guards::cfg_predicate as cfg_span;
use tasty_doc_guards::match_arms::Source;

use std::ops::Range;
use std::path::Path;

use tasty_ipc::method_meta::METHOD_TABLE;

/// debug CLI에서 사용하지만 release IPC에도 필요한 메서드와 근거다.
/// 로컬 attach의 강제 끊기도 debug 메서드(debug.attach.*)로 재현하므로 지금은 없다.
const CLI_DEBUG_RELEASE_METHODS: &[(&str, &str)] = &[];

/// 입력 재현을 나타내는 이름 패턴이다. 대상 PTY에 바이트를 쓰는 surface.send_key와 구별한다.
const INPUT_REPRODUCTION_PATTERNS: &[(&str, &str)] = &[
    ("inject", "키·마우스 이벤트 주입"),
    ("raw_key", "OS 이벤트 스트림 키 주입"),
    ("switch_input_source", "OS 전역 입력 소스 전환"),
    ("ime_", "입력기 조합 상태 강제 세팅"),
    ("simulate", "입력 시뮬레이션"),
];

const FORBIDDEN_LAST_SEGMENTS: &[&str] = &["focus"];

fn manifest_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn debug_cli_methods_are_not_in_the_release_table() {
    let path = manifest_root().join("crates/tasty-cli/src/request/debug.rs");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "debug CLI 요청 매핑을 읽을 수 없다: {}: {e}",
            path.display()
        )
    });

    let mut offenders: Vec<String> = Vec::new();
    let mut seen = 0usize;
    for raw in src.split('"').skip(1).step_by(2) {
        if !raw.contains('.')
            || !raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
        {
            continue;
        }
        seen += 1;
        if CLI_DEBUG_RELEASE_METHODS.iter().any(|(m, _)| *m == raw) {
            continue;
        }
        if METHOD_TABLE.iter().any(|(m, _)| *m == raw) {
            offenders.push(raw.to_string());
        }
    }

    assert!(
        seen > 30,
        "debug CLI 메서드 리터럴을 {seen} 개밖에 못 찾았다 — 스캔 패턴이 깨졌을 가능성이 크다"
    );
    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "debug 전용 CLI(`DebugCommands`)가 부르는데 release 표(METHOD_TABLE)에 \
         등재된 메서드가 있다. 같은 기능이 CLI 에서는 debug, IPC 에서는 release 로 \
         갈라진 상태라 IPC 표면만 열려 있다. DEBUG_METHODS 로 옮기거나, release 가 \
         맞다면 CLI_DEBUG_RELEASE_METHODS 에 근거와 함께 등재하라:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn release_table_has_no_input_reproduction_method_names() {
    let mut offenders: Vec<String> = Vec::new();
    for (method, _) in METHOD_TABLE {
        for (pat, why) in INPUT_REPRODUCTION_PATTERNS {
            if method.contains(pat) {
                offenders.push(format!("{method} — '{pat}' ({why})"));
            }
        }
        let last = method.rsplit('.').next().unwrap_or(method);
        if FORBIDDEN_LAST_SEGMENTS.contains(&last) {
            offenders.push(format!("{method} — 마지막 세그먼트 '{last}' (포커스 전환)"));
        }
    }

    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "release 표(METHOD_TABLE)에 사용자 입력 재현 계열 메서드가 있다 \
         (docs/principles/control-and-data.md — 입력 재현은 release 에 없다). \
         DEBUG_METHODS 로 옮겨라. 이름이 우연히 겹친 정상 에이전트 메서드라면 \
         이 파일의 패턴을 좁히고 그 근거를 남겨라:\n  {}",
        offenders.join("\n  ")
    );
}

/// prefix 규칙은 개별 METHOD_TABLE 항목이 아니므로 별도로 cfg를 확인한다.
#[test]
fn ime_prefix_rule_stays_debug_only() {
    let path = manifest_root().join("crates/tasty-ipc/src/method_meta.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("메서드 표 소스를 읽을 수 없다: {}: {e}", path.display()));
    let lines: Vec<&str> = src.lines().collect();

    let idx = lines
        .iter()
        .position(|l| l.contains("pub const PREFIX_RULES") && l.contains("surface.ime_"))
        .expect(
            "`surface.ime_` 를 담은 PREFIX_RULES 정의를 찾지 못했다 — 정의가 \
             여러 줄로 쪼개졌거나 규칙이 사라졌다. 사라졌다면 이 테스트도 함께 \
             정리하라(사각지대가 없어진 것이므로).",
        );

    assert!(
        idx > 0 && lines[idx - 1].trim() == "#[cfg(debug_assertions)]",
        "`surface.ime_` prefix 규칙이 `#[cfg(debug_assertions)]` 없이 정의돼 있다. \
         IME 조합 상태 강제 세팅은 사용자 입력 재현이라 release 표면에 두지 않는다 \
         (docs/principles/control-and-data.md). 바로 앞 줄: {:?}",
        lines.get(idx.wrapping_sub(1)).copied().unwrap_or("<없음>")
    );
    assert!(
        src.contains("#[cfg(not(debug_assertions))]\npub const PREFIX_RULES"),
        "release 쪽 `PREFIX_RULES` (빈 슬라이스) 정의가 없다 — debug 쪽에만 cfg 를 \
         걸면 release 빌드가 컴파일되지 않는다."
    );
}

/// release에서 남는 라우터의 파일·함수 시그니처·메서드 후보 수 하한이다.
/// 함수나 모듈 전체가 debug 전용인 라우터는 제외한다.
const RELEASE_ROUTERS: &[(&str, &str, usize)] = &[
    (
        "src/adapters/ipc/handler.rs",
        "fn route_engine_handler(",
        200,
    ),
    ("src/adapters/ipc/handler.rs", "fn route_window_handler(", 1),
    (
        "src/app/ipc/app_methods.rs",
        "pub(crate) fn ipc_step_app_methods(",
        12,
    ),
    (
        "src/app/dispatch/list_global.rs",
        "pub(crate) fn dispatch_list_global(",
        3,
    ),
    // pump_ipc에도 메서드 분기가 다시 생길 수 있어 남긴다. 현재 하한 0으로는 그 함수의 부분 누락을 찾지 못한다.
    (
        "src/boot/headless_dispatch.rs",
        "pub(crate) fn pump_ipc(",
        0,
    ),
    // 함수 선언 바깥의 cfg는 본문 검사에서 보이지 않으므로 debug 전용 intercept_debug_app_layer는 등록하지 않는다.
    (
        "src/boot/headless_dispatch.rs",
        "fn intercept_app_layer(",
        5,
    ),
];

/// 함수의 정의마다 메서드 후보를 읽는다. 같은 이름의 정의가 여럿이면(cfg 로 갈린 정의) 모두 읽는다.
fn router_candidates(text: &str, sig: &str) -> Vec<Scanned> {
    let name = sig
        .split_once("fn ")
        .and_then(|(_, rest)| rest.strip_suffix('('))
        .unwrap_or_else(|| panic!("시그니처가 `… fn <이름>(` 모양이 아니다: {sig}"));
    let source = Source::new(text);
    let bodies = source.fn_bodies(name);
    assert!(!bodies.is_empty(), "함수 시그니처를 찾지 못했다: {sig}");
    let mut out = Vec::new();
    for body in bodies {
        match scan_body(&source, body.clone()) {
            Ok(found) => out.extend(found),
            Err(e) => panic!(
                "{sig}({}행 정의)의 match 팔을 읽지 못했다 — {e}. 라우터를 판정기가 읽을 수 있는 \
                 모양으로 두어라. 못 읽는 팔은 release 표와 대조할 수 없다",
                source.line_of(body.start)
            ),
        }
    }
    out
}

struct Scanned {
    name: String,
    /// 메서드 하나가 아닌 접두사 비교로 추출한 항목이다.
    is_prefix: bool,
    gated: bool,
}

/// 팔 앞 속성 중 debug_assertions 를 함의하는 `#[cfg(…)]` 가 있는가.
fn attrs_imply_debug(attrs: &str) -> bool {
    attrs.split("#[").skip(1).any(|a| {
        a.trim_start()
            .strip_prefix("cfg(")
            .and_then(|r| r.trim_end().strip_suffix(")]"))
            .is_some_and(|pred| cfg_span::implies(pred, "debug_assertions"))
    })
}

/// 본문 하나에서 메서드 후보를 읽는다.
///
/// match 팔은 공용 판정기(`Source::match_arms`)로 떼어 팔마다 읽는다. debug 게이트도 팔 단위로
/// 본다 — 팔 머리 앞 속성, 그 팔을 감싼 팔의 속성, 그리고 match 를 감싼 블록·문장의 cfg.
/// 마지막 것은 팔 속성을 지운 사본에서 줄 단위로 계산한다. 팔 속성을 남겨 두면 한 줄에 놓인
/// 다음 팔까지 같은 줄이라 게이트로 칠해진다.
///
/// `==`·`starts_with`·`strip_prefix` 비교는 match 팔이 아니라 판정기 밖이다. 이 형태는 줄 단위로
/// 읽고 게이트도 줄 단위다 — `#[cfg]` 문장은 그 줄 전체가 같은 문장이라 팔과 같은 사각이 없다.
/// 한 줄에 cfg 문장과 다른 비교 문장을 함께 두면 둘째가 게이트로 칠해지지만 rustfmt 는 문장을
/// 한 줄에 하나로 편다. 이 한계는 `known_limit_a_cfg_statement_gates_its_whole_line` 이 고정한다.
fn scan_body(source: &Source, body: Range<usize>) -> Result<Vec<Scanned>, String> {
    let mut arms = Vec::new();
    for block in source.match_blocks(body.clone())? {
        arms.extend(source.match_arms(block)?);
    }

    // 팔 속성을 지운 본문 — 남은 cfg 는 블록·문장 단위다.
    let mut bytes = source.slice(&body).as_bytes().to_vec();
    for arm in &arms {
        for b in &mut bytes[arm.attrs.start - body.start..arm.attrs.end - body.start] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    let blanked = String::from_utf8(bytes).map_err(|e| format!("팔 속성 지우기: {e}"))?;
    let lines: Vec<String> = blanked.lines().map(str::to_owned).collect();
    let line_gate = debug_gated_lines(&lines);
    let gated_line =
        |i: usize| line_gate.get(i).copied().unwrap_or(false) || is_debug_gated(&lines, i);
    let line_index = |pos: usize| source.text[body.start..pos].matches('\n').count();

    let gated_spans: Vec<Range<usize>> = arms
        .iter()
        .filter(|a| attrs_imply_debug(source.slice(&a.attrs)))
        .map(|a| a.attrs.start..a.body.end)
        .collect();

    let mut out = Vec::new();
    for arm in &arms {
        for alt in source.alternatives(&arm.pattern) {
            let Some(name) = source.plain_string(&alt).filter(|n| method_shaped(n)) else {
                continue;
            };
            out.push(Scanned {
                name: name.to_string(),
                is_prefix: false,
                gated: gated_spans.iter().any(|s| s.contains(&alt.start))
                    || gated_line(line_index(alt.start)),
            });
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for c in comparison_methods(line) {
            out.push(Scanned {
                name: c.name.to_string(),
                is_prefix: c.is_prefix,
                gated: gated_line(i),
            });
        }
    }
    Ok(out)
}

fn method_shaped(name: &str) -> bool {
    name.contains('.')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

struct Comparison<'a> {
    name: &'a str,
    is_prefix: bool,
}

/// method 변수·필드의 == 비교와 starts_with·strip_prefix 문자열을 읽는다. match 팔은 읽지 않는다.
/// 수신자의 마지막 식별자를 확인하며 타입이나 별칭의 데이터 흐름까지 분석하지는 않는다.
fn comparison_methods(line: &str) -> Vec<Comparison<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = line[from..].find('"') {
        let open = from + rel;
        let Some(close_rel) = line[open + 1..].find('"') else {
            break;
        };
        let close = open + 1 + close_rel;
        let name = &line[open + 1..close];
        let before = line[..open].trim_end();
        let is_eq = before.strip_suffix("==").is_some_and(method_receiver);
        let is_prefix = [".starts_with(", ".strip_prefix("]
            .iter()
            .any(|suffix| before.strip_suffix(suffix).is_some_and(method_receiver));
        if method_shaped(name) && (is_eq || is_prefix) {
            out.push(Comparison { name, is_prefix });
        }
        from = close + 1;
    }
    out
}

fn method_receiver(before: &str) -> bool {
    before
        .trim_end()
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        == Some("method")
}

/// 앞의 연속 속성·주석·빈 줄에서 debug_assertions 문자열을 찾는다. 이 보조 검사는 조건식의 not·any 의미를 평가하지 않는다.
fn is_debug_gated(lines: &[String], idx: usize) -> bool {
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.starts_with("#[") {
            if t.contains("debug_assertions") {
                return true;
            }
            continue;
        }
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        return false;
    }
    false
}

fn debug_gated_lines(lines: &[String]) -> Vec<bool> {
    cfg_span::cfg_gated_lines(lines, "debug_assertions")
}

/// 접두사 항목은 같은 접두사로 시작하는 release 메서드가 하나만 있어도 등록된 것으로 본다.
fn registered_in_release_table(name: &str, is_prefix: bool) -> bool {
    if is_prefix {
        METHOD_TABLE.iter().any(|(m, _)| m.starts_with(name))
    } else {
        METHOD_TABLE.iter().any(|(m, _)| *m == name)
    }
}

#[test]
fn release_router_arms_are_registered_in_the_release_table() {
    let root = manifest_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for (rel, sig, min_arms) in RELEASE_ROUTERS {
        let path = root.join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("라우터 소스를 읽을 수 없다: {}: {e}", path.display()));
        let arms = router_candidates(&src, sig);
        let here = arms.len();
        for arm in arms {
            if arm.gated {
                continue;
            }
            if !registered_in_release_table(&arm.name, arm.is_prefix) {
                offenders.push(format!("{rel}: {}", arm.name));
            }
        }
        assert!(
            here >= *min_arms,
            "{rel}의 {sig}에서 메서드 후보를 {here} 개만 읽었다(하한 {min_arms}). 함수 경계와 판독 형식을 확인한다."
        );
        scanned += here;
    }

    assert!(
        scanned > 200,
        "release 라우터의 메서드 후보를 {scanned} 개만 읽었다. 수집 범위와 판독 형식을 확인한다."
    );
    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "release 라우터의 메서드 후보가 METHOD_TABLE에 없다. 표에서만 이름을 빼면 로컬 호출 접근을 막지 못한다. 입력 재현 기능이면 debug cfg를 적용하거나 debug 라우터로 옮긴다:\n  {}",
        offenders.join("\n  ")
    );
}

mod extractor_mutations {
    use super::*;

    /// 조각을 함수 본문으로 감싸 실제 검사와 같은 경로로 읽는다.
    fn scan(snippet: &str) -> Vec<Scanned> {
        let text = format!("fn f() {{\n{snippet}}}\n");
        let source = Source::new(&text);
        let body = source.fn_bodies("f").remove(0);
        scan_body(&source, body).unwrap_or_else(|e| panic!("조각을 읽지 못했다: {e}"))
    }

    fn find<'a>(arms: &'a [Scanned], name: &str) -> &'a Scanned {
        arms.iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("`{name}` 을 못 읽었다"))
    }

    #[test]
    fn a_block_level_cfg_gates_the_arms_inside_it() {
        let src = "\
#[cfg(debug_assertions)]
{
    let rpc_id = id.clone();
    if method == \"debug.lua.eval\" { run(); }
    if method.starts_with(\"debug.event_bus.\") { run(); }
}
";
        let arms = scan(src);
        assert_eq!(
            arms.len(),
            2,
            "세 형태 중 둘을 팔로 봐야 한다: {}",
            arms.len()
        );
        assert!(
            arms.iter().all(|a| a.gated),
            "블록 cfg 가 안쪽 팔에 상속되지 않았다 — 위양성이 그대로다"
        );
        assert!(
            arms.iter()
                .any(|a| a.is_prefix && a.name == "debug.event_bus."),
            "접두어 형을 못 봤다"
        );
    }

    #[test]
    fn removing_the_block_cfg_reopens_the_finding() {
        let src = "\
{
    let rpc_id = id.clone();
    if method == \"debug.lua.eval\" { run(); }
}
";
        assert!(
            !find(&scan(src), "debug.lua.eval").gated,
            "cfg 없는 블록의 팔을 gated 로 봤다 — 상속이 너무 넓다"
        );
    }

    #[test]
    fn an_arm_after_the_gated_block_is_still_release() {
        let src = "\
#[cfg(debug_assertions)]
{
    if method == \"debug.lua.eval\" { run(); }
}
if method == \"leaked.method\" { run(); }
";
        assert!(
            !find(&scan(src), "leaked.method").gated,
            "블록의 게이트가 블록 밖으로 새어 나갔다"
        );
    }

    #[test]
    fn braces_inside_strings_do_not_close_the_gated_block() {
        let src = "\
#[cfg(debug_assertions)]
{
    log(\"}\");
    if method == \"debug.lua.eval\" { run(); }
}
";
        assert!(
            find(&scan(src), "debug.lua.eval").gated,
            "문자열 안 `}}` 에 속아 블록이 일찍 닫혔다"
        );
    }

    #[test]
    fn a_guarded_arm_is_still_an_arm() {
        let src = "\
match method {
    \"leaked.guarded\" if matches!(caller, CallerContext::Local) => run(),
    \"leaked.alt_a\" | \"leaked.alt_b\" if ok => run(),
    \"leaked.multiline\" if matches!(
        caller,
        CallerContext::Local
    ) => run(),
    _ => {}
}
";
        let arms = scan(src);
        for want in [
            "leaked.guarded",
            "leaked.alt_a",
            "leaked.alt_b",
            "leaked.multiline",
        ] {
            assert!(!find(&arms, want).gated, "`{want}` 을 gated 로 봤다");
        }
    }

    #[test]
    fn a_guard_wrapped_to_the_next_line_is_still_an_arm() {
        let src = "\
match method {
    \"leaked.wrapped\"
        if matches!(caller, CallerContext::Local) =>
    {
        run()
    }
    \"leaked.wrapped_a\"
    | \"leaked.wrapped_b\"
        if ok => run(),
    _ => {}
}
";
        let arms = scan(src);
        for want in ["leaked.wrapped", "leaked.wrapped_a", "leaked.wrapped_b"] {
            find(&arms, want);
        }
    }

    #[test]
    fn a_plain_if_statement_is_read_once() {
        let src = "\
let x = run();
if method == \"kept.eq\" { run(); }
";
        assert_eq!(scan(src).len(), 1);
    }

    #[test]
    fn caller_identifiers_are_not_methods_but_unregistered_method_comparisons_are() {
        let arms = scan(
            r#"
if plugin_id == "com.tasty.markdown" { run(); }
if caller.plugin_id.starts_with("com.tasty.") { run(); }
if other_method == "not.a_method" { run(); }
if method == "leaked.direct" { run(); }
if cmd.request.method == "leaked.field" { run(); }
if method.starts_with("leaked.prefix.") { run(); }
if cmd.request.method.strip_prefix("leaked.stripped.").is_some() { run(); }
"#,
        );
        assert_eq!(arms.len(), 4, "호출자 식별자를 메서드로 읽으면 안 된다");
        for (name, prefix) in [
            ("leaked.direct", false),
            ("leaked.field", false),
            ("leaked.prefix.", true),
            ("leaked.stripped.", true),
        ] {
            let arm = find(&arms, name);
            assert_eq!(arm.is_prefix, prefix);
            assert!(!arm.gated);
            assert!(!registered_in_release_table(&arm.name, arm.is_prefix));
        }
    }

    /// 한 줄에 두 팔이면 둘 다 읽는다(줄 단위 판독은 둘째를 줄의 나머지로 읽었다).
    #[test]
    fn two_arms_on_one_line_are_both_read() {
        let src = "\
match method {
    \"system.info\" => run(), \"zz.not_in_table\" => run(),
    _ => {}
}
";
        let arms = scan(src);
        assert!(!find(&arms, "system.info").gated);
        assert!(!find(&arms, "zz.not_in_table").gated);
    }

    /// 첫 팔의 debug cfg 가 같은 줄의 다음 팔을 칠하지 않는다. 줄 단위 게이트는 둘째 팔까지
    /// gated 로 읽어, release 에서 컴파일되는 팔이 표 대조를 피했다.
    #[test]
    fn a_debug_cfg_on_an_arm_does_not_gate_the_next_arm_on_the_same_line() {
        let src = "\
match method {
    #[cfg(debug_assertions)]
    \"system.zz_debug\" => run(), \"zz.not_in_table\" => run(),
    _ => {}
}
";
        let arms = scan(src);
        assert!(find(&arms, "system.zz_debug").gated);
        assert!(
            !find(&arms, "zz.not_in_table").gated,
            "같은 줄의 둘째 팔이 첫 팔의 cfg 로 칠해졌다"
        );
    }

    #[test]
    fn a_gated_arm_gates_the_match_nested_in_its_body() {
        let src = "\
match method {
    #[cfg(debug_assertions)]
    \"debug.outer\" => match sub {
        \"debug.inner\" => run(),
        _ => {}
    },
    \"kept.outer\" => run(),
    _ => {}
}
";
        let arms = scan(src);
        assert!(
            find(&arms, "debug.inner").gated,
            "바깥 팔의 게이트가 안쪽 팔에 안 미쳤다"
        );
        assert!(
            !find(&arms, "kept.outer").gated,
            "게이트가 다음 팔로 새어 나갔다"
        );
    }

    #[test]
    fn a_not_debug_cfg_on_an_arm_is_release() {
        let src = "\
match method {
    #[cfg(not(debug_assertions))]
    \"kept.release_only\" => run(),
    _ => {}
}
";
        assert!(!find(&scan(src), "kept.release_only").gated);
    }

    /// 판정기 밖에 남긴 비교 형태의 한계를 고정한다. 한 줄에 cfg 문장과 다른 비교 문장을 함께
    /// 두면 둘째도 gated 로 읽는다. rustfmt 가 문장을 한 줄에 하나로 펴서 레포에는 들어오기
    /// 어렵다. 이 시험이 실패하면 한계가 닫힌 것이므로 scan_body 의 설명도 함께 고친다.
    #[test]
    fn known_limit_a_cfg_statement_gates_its_whole_line() {
        let src = "\
#[cfg(debug_assertions)]
if method == \"debug.x\" { run(); } if method == \"leaked.same_line\" { run(); }
";
        assert!(find(&scan(src), "leaked.same_line").gated);
    }

    #[test]
    fn an_unreadable_match_is_an_error_not_a_skip() {
        let text =
            "fn f() {\nmatch method { \"a.b\" => if x { 1 } else { 2 } \"c.d\" => 3, _ => 0 }\n}\n";
        let source = Source::new(text);
        let body = source.fn_bodies("f").remove(0);
        assert!(scan_body(&source, body).is_err());
    }

    #[test]
    fn a_prefix_arm_is_registered_only_if_the_table_has_something_under_it() {
        assert!(
            !registered_in_release_table("no.such.namespace.", true),
            "아무것도 없는 접두어를 등재됐다고 봤다"
        );
        let (first, _) = METHOD_TABLE[0];
        assert!(
            registered_in_release_table(first, false),
            "release 표에 있는 메서드를 미등록으로 판정했다"
        );
    }
}
