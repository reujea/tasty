//! release IPC 표(`METHOD_TABLE`)에 **사용자 입력 재현** 메서드가 들어오면 실패한다.
//!
//! [`docs/identity.md`] 원칙 1 ②: 키/마우스 주입, popup 강제 조작, 메뉴 강제
//! invoke, 프로그래밍적 포커스 전환은 release IPC/CLI 표면에 존재하지 않고
//! `#[cfg(debug_assertions)]` 격리로만 제공된다. 정책·판단 기준은
//! [`docs/dev-guide/debug-ipc.md`], 이 가드가 생긴 결정은
//! [`docs/adr/0115-input-reproduction-ipc-debug-isolation.md`].
//!
//! # 판정 기준을 왜 이렇게 기계화했나
//!
//! "사용자 입력 재현인가" 는 의미 판단이라 완전 자동 판정이 불가능하다. 그래서
//! **자동으로 판정하지 않고, 이미 사람이 내려둔 판정과의 어긋남**을 잡는다 —
//! 어긋남은 기계적으로 판정 가능하고, 실제로 이 가드를 만든 결함이 정확히 그
//! 형태였다(`surface.raw_key`·`surface.switch_input_source`·`surface.ime_*` 는
//! CLI 에서는 debug 전용인데 IPC 표에서는 release 였다).
//!
//! - **가드 1 (CLI↔IPC 정합)** — debug CLI(`DebugCommands`, 모듈째
//!   `#![cfg(debug_assertions)]`)가 부르는 메서드는 IPC 에서도 debug 여야 한다.
//!   같은 기능의 두 표면이 서로 다른 빌드에 노출되는 것 자체가 결함이다.
//!   예외는 [`CLI_DEBUG_RELEASE_METHODS`] 에 근거와 함께 명시한다.
//! - **가드 2 (이름 규칙)** — 입력 재현 계열임이 이름에 드러나는 형태
//!   ([`INPUT_REPRODUCTION_PATTERNS`])는 release 표에 못 들어온다. 이름만으로
//!   판정되는 좁은 집합이라 오탐이 적고, 새 메서드에도 선제적으로 걸린다.
//! - **가드 3 (prefix 규칙)** — `surface.ime_*` 는 개별 등재가 아니라
//!   `PREFIX_RULES` 로 해소되므로 위 두 가드의 사각지대다. 그 규칙이
//!   `#[cfg(debug_assertions)]` 아래 있는지 소스에서 직접 확인한다.
//!
//! - **가드 4 (라우터 ↔ 표 정합)** — 위 셋은 전부 *등재 표* 를 지킨다. 그런데
//!   release 차단의 부하를 실제로 지는 곳은 **라우터의 cfg** 다: 권한 게이트는
//!   `CallerContext::Local` 을 무조건 통과시키고(`crates/tasty-ipc/src/caller.rs`),
//!   디스패치 경로는 `method_meta()` 를 아예 부르지 않는다. 즉 **표에서 빼도
//!   release 라우터에 팔이 남아 있으면 로컬 IPC 호출자에게 그대로 열린다.**
//!   그래서 release 라우터 함수의 팔이 release 표(`METHOD_TABLE`)에 없으면 실패한다.
//!
//!   기존 [`tests/ipc_router_table_parity.rs`] 는 이걸 못 잡는다 — 그 가드는
//!   "라우터에 팔이 있으면 어느 표에든 등재돼 있어야 한다" 를 보는데, 테스트는 항상
//!   debug 로 컴파일되므로 `DEBUG_METHODS` 가 채워져 있어 debug 표만으로 "등재됨" 이
//!   된다. **표는 debug, 팔은 release** 라는 정확히 이 결함 형태가 그 가드를 통과한다.
//!
//! 이 가드들이 **잡지 못하는 것**: 이름에 단서가 없고 debug CLI 진입점도 없는
//! 새 release 메서드의 의미 판단. 그건 사람이 리뷰에서 본다 — 자동화의 목표는
//! "이미 내려진 판정이 조용히 뒤집히는 것" 을 막는 데까지다.
//!
//! 가드 4 의 추출기는 **팔 바로 위의 `#[cfg]`** 와 **팔을 감싸는 블록의 `#[cfg]`** 를
//! 모두 읽고(`debug_gated_lines`), 팔의 모양은 `"m" =>` · `== "m"` · `starts_with("ns.")`
//! 셋을 본다. 이 둘은 각각 겪은 결함이다 — 블록 형태를 못 읽어 지워질 팔을 남았다고 한
//! **위양성**, `==` 만 봐서 같은 블록의 접두어 팔을 통째로 놓친 **거짓 음성**. 뒤엣것이
//! 더 나쁘다(가드가 초록인 채로 표면이 열린다). 두 방향 모두 [`extractor_mutations`]
//! 에서 변이로 고정한다.
//!
//! 남는 사각지대는 **라우터 함수 밖**이다: 팔을 헬퍼 함수로 옮기면 `RELEASE_ROUTERS` 의
//! 시그니처 본문에 안 잡힌다. 그때는 그 헬퍼를 `RELEASE_ROUTERS` 에 추가한다.
//!
//! release 빌드에서는 `DEBUG_METHODS` 가 빈 슬라이스라 대조가 성립하지 않으므로
//! `#![cfg(debug_assertions)]` 로 debug 에서만 돈다.
//!
//! **채널 — 컴파일은 두 조합 모두 자동, 실행은 한 조합만이다.** `tests/*.rs` 라 이 파일이
//! 컴파일되는지는 CI 가 두 조합에서 본다(`crossplatform-check.yml` 의 Windows
//! `clippy --all-targets` 와 headless `clippy --all-targets --no-default-features`).
//! 실행 쪽은 자동 실행은 **헤드리스 조합**(`check-headless` 의 전체 스위트)에서만 일어난다
//! (기본 조합 잡은 `--lib --bins` 라 통합 타깃을 못 본다 — `docs/dev-guide/ci-gates.md`).
//! `test.yml` 의 기본 조합 전체 스위트는 `workflow_dispatch` 전용이라 거기엔 채널이 없다.
//! (채널 정본은 [ci-gates](../docs/dev-guide/ci-gates.md)).
//! 따라서 이 가드를 근거로 로컬 검증을 건너뛰지 않는다.
#![cfg(debug_assertions)]

use tasty_doc_guards::cfg_predicate as cfg_span;
use tasty_doc_guards::match_arms::Source;

use std::path::Path;

use tasty_ipc::method_meta::METHOD_TABLE;

/// debug CLI 가 부르지만 **release IPC 로 두는 것이 맞는** 메서드.
///
/// 여기 추가하는 것은 "debug CLI 진입점이 있는데 IPC 는 release" 를 의도로
/// 선언하는 일이다. 근거를 한 줄로 남기지 않을 거면 추가하지 마라.
const CLI_DEBUG_RELEASE_METHODS: &[(&str, &str)] = &[
    // `tasty debug attach` 는 로컬 loopback self-attach 라 debug 전용이지만,
    // 그 안에서 쓰는 force-detach 자체는 원격 attach(release)와 공용 메커니즘이라
    // release JSON-RPC 다. docs/dev-guide/debug-ipc.md "tasty debug attach" 참조.
    (
        "attach.force_detach",
        "원격 attach 와 공용 — release 정식 표면",
    ),
    (
        "attach.force_detach_workspace",
        "원격 attach 와 공용 — release 정식 표면",
    ),
];

/// release 표에 있으면 안 되는 이름 형태. `(패턴, 무엇을 겨냥하나)`.
///
/// 이름만으로 입력 재현임이 드러나는 좁은 집합이다 — 넓히면 정상 에이전트
/// 메서드를 오탐한다. 대조: `surface.send_key` 는 대상 surface ID 를 받아 그
/// PTY 에 바이트를 쓰는 정상 에이전트 동작이라 어느 패턴에도 걸리지 않는다.
const INPUT_REPRODUCTION_PATTERNS: &[(&str, &str)] = &[
    ("inject", "키·마우스 이벤트 주입"),
    ("raw_key", "OS 이벤트 스트림 키 주입"),
    ("switch_input_source", "OS 전역 입력 소스 전환"),
    ("ime_", "입력기 조합 상태 강제 세팅"),
    ("simulate", "입력 시뮬레이션"),
];

/// 메서드 이름의 마지막 세그먼트가 이것이면 release 표에 있으면 안 된다.
/// (`window.focus` / `view.focus` — 프로그래밍적 포커스 전환.)
const FORBIDDEN_LAST_SEGMENTS: &[&str] = &["focus"];

fn manifest_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 가드 1 — debug CLI 가 부르는 메서드는 release 표에 없어야 한다.
#[test]
fn debug_cli_methods_are_not_in_the_release_table() {
    let path = manifest_root().join("crates/tasty-cli/src/request/debug.rs");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "debug CLI 요청 매핑을 읽을 수 없다: {}: {e}",
            path.display()
        )
    });

    // `"ns.method"` 형태의 리터럴만. i18n 키(`cli.debug.*`)는 표에 없어서
    // 자연히 걸러진다 — 별도 예외 목록이 필요 없다.
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

/// 가드 2 — 입력 재현임이 이름에 드러나는 메서드는 release 표에 없어야 한다.
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
         (docs/identity.md 원칙 1 ② — 입력 재현은 release 에 없다). \
         DEBUG_METHODS 로 옮겨라. 이름이 우연히 겹친 정상 에이전트 메서드라면 \
         이 파일의 패턴을 좁히고 그 근거를 남겨라:\n  {}",
        offenders.join("\n  ")
    );
}

/// 가드 3 — `surface.ime_*` prefix 규칙은 debug 빌드에만 존재해야 한다.
///
/// prefix 로 해소되는 메서드는 `METHOD_TABLE` 을 훑는 위 두 가드에 걸리지 않아
/// 사각지대다. `PREFIX_RULES` 정의에 cfg 격리가 붙어 있는지 소스에서 확인한다.
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
         (docs/identity.md 원칙 1 ②). 바로 앞 줄: {:?}",
        lines.get(idx.wrapping_sub(1)).copied().unwrap_or("<없음>")
    );
    assert!(
        src.contains("#[cfg(not(debug_assertions))]\npub const PREFIX_RULES"),
        "release 쪽 `PREFIX_RULES` (빈 슬라이스) 정의가 없다 — debug 쪽에만 cfg 를 \
         걸면 release 빌드가 컴파일되지 않는다."
    );
}

/// release 빌드에서도 컴파일되는 디스패치 함수 — `(파일, 함수 시그니처 시작, 최소 팔 수)`.
///
/// 여기 없는 라우터(`route_debug_handler` · `ipc_step_debug` ·
/// `ipc_step_window_required` · `handler/ime.rs`)는 함수·모듈째
/// `#[cfg(debug_assertions)]` 라 release 바이너리에 존재하지 않는다.
///
/// 세 번째 값은 그 함수에서 최소한 이만큼은 팔을 찾아야 한다는 하한이다 — 중괄호
/// 스캔이 깨져 본문을 짧게 읽으면 가드가 **아무것도 검사하지 않은 채 통과**하므로,
/// 실측값보다 조금 낮게 잡아 미스캔을 실패로 드러낸다.
const RELEASE_ROUTERS: &[(&str, &str, usize)] = &[
    (
        "src/adapters/ipc/handler.rs",
        "fn route_engine_handler(",
        200,
    ),
    // gui release 창 라우터 — 창 상태 자체가 대상인 핸들러(popup 을 여는 `file_picker.trigger`)가
    // 여기 산다. 입력 재현(popup 강제 open)이 들어오기 가장 쉬운 자리인데 한동안 명부에 없어
    // 이 가드가 훑지 않았다. 하한 1 은 오늘 팔 하나(`file_picker.trigger`)다.
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
    // 헤드리스 dispatch 는 **두 자리**다. `pump_ipc` 에는 이름으로 가르는 팔이 더 이상
    // 없고(하한 0 은 그 사실이다), 종단 응답이 전부 `intercept_app_layer` 로 갔다 —
    // 모듈 doc 이 적어 둔 사각("팔을 헬퍼 함수로 옮기면 안 잡힌다")이 실제로 발생한
    // 자리이고, 그 doc 이 정한 처방대로 헬퍼를 명부에 **더한다**. `pump_ipc` 를 빼지
    // 않는 이유는 팔이 거기 다시 생길 수 있어서고, 하한 0 이라 스캔 붕괴는 못 보지만
    // 그쪽은 아래 항목의 하한과 전역 `scanned > 200` 이 맡는다.
    (
        "src/boot/headless_dispatch.rs",
        "pub(crate) fn pump_ipc(",
        0,
    ),
    // `intercept_debug_app_layer` 는 **일부러 안 넣는다.** 그쪽 팔은 함수 시그니처에
    // 붙은 `#[cfg(debug_assertions)]` 로 gated 인데, `is_debug_gated` 는 스캔한 본문
    // 줄에서만 cfg 를 찾으므로 시그니처 밖의 그것을 못 본다 — 넣으면 debug 표면이
    // 통째로 "release 표에 없는 팔" 로 잡히는 위양성이 된다. 그 팔들은 예전에도
    // (블록 cfg 안에 있어서) gated 로 걸러졌으므로 이 제외로 잃는 판정은 없다.
    (
        "src/boot/headless_dispatch.rs",
        "fn intercept_app_layer(",
        5,
    ),
];

/// `sig` 가 가리키는 함수 정의 **전부**의 본문 줄을 돌려준다.
///
/// 본문의 경계는 공용 판정기(`tasty_doc_guards::match_arms`)가 주석·문자열·문자 리터럴을 덮은
/// 사본에서 찾는다 — 손으로 센 중괄호는 주석 속 `}` 나 `'}'` 에서 본문을 일찍 닫아 그 뒤의
/// 팔을 조용히 놓쳤다. 정의가 `#[cfg]` 로 둘 이상이면 둘 다 읽는다(첫 정의만 보면 둘째의
/// 팔은 어느 플랫폼에서도 대조를 안 받는다). 정의마다 줄 목록을 따로 돌려주어 한 정의의
/// 블록 cfg 가 다음 정의로 새지 않게 한다.
fn fn_body_lines(src: &str, sig: &str) -> Vec<Vec<String>> {
    let name = sig
        .split_once("fn ")
        .and_then(|(_, rest)| rest.strip_suffix('('))
        .unwrap_or_else(|| panic!("시그니처가 `… fn <이름>(` 모양이 아니다: {sig}"));
    let source = Source::new(src);
    let bodies = source.fn_bodies(name);
    assert!(!bodies.is_empty(), "함수 시그니처를 찾지 못했다: {sig}");
    bodies
        .iter()
        .map(|body| {
            let mut lines: Vec<String> = source.slice(body).lines().map(str::to_owned).collect();
            join_wrapped_arms(&mut lines);
            lines
        })
        .collect()
}

/// `=>` 나 `|` 가 다음 줄로 넘어간 match 팔을 **원래 줄로 끌어올린다.**
///
/// 팔 인식이 한 줄 단위라, 아래 같은 형태는 그대로면 팔로 보이지 않는다.
///
/// ```text
/// "surface.raw_key"
///     => handle(..),
/// ```
///
/// rustfmt 가 이 형태를 한 줄로 되돌리므로(`cargo fmt --check` 가 CI·pre-commit 에서
/// 강제한다) 실제로 레포에 들어올 수는 없다. 그래도 정규화해 둔다 — 가드의 회피
/// 난이도를 포매터 하나에만 의존시키지 않기 위함이다.
///
/// match guard 도 같다 — 패턴과 guard 를 합친 길이가 넘치면 rustfmt 는 **guard 를 다음
/// 줄로 내린다**(이쪽은 포매터가 되돌리지 않는 실제 형태다).
///
/// ```text
/// "surface.raw_key"
///     if matches!(caller, CallerContext::Local) => handle(..),
/// ```
///
/// `if` 로 시작하는 줄은 평범한 문장일 때가 훨씬 많으므로, 윗줄이 **닫는 따옴표로
/// 끝날 때만**(= 패턴의 끝) 끌어올린다.
///
/// 병합한 줄은 **빈 줄로 남긴다.** 줄 번호가 밀리면 `is_debug_gated` 가 팔 위의
/// `#[cfg]` 를 잘못 짚는다.
fn join_wrapped_arms(lines: &mut [String]) {
    for i in (0..lines.len()).rev() {
        let t = lines[i].trim_start();
        let guard = starts_with_guard(t);
        if !(t.starts_with("=>") || t.starts_with('|') || guard) {
            continue;
        }
        // 위로 올라가며 가장 가까운 비어있지 않은 줄에 붙인다.
        let Some(prev) = (0..i).rev().find(|&j| !lines[j].trim().is_empty()) else {
            continue;
        };
        if lines[prev].trim_start().starts_with("//") {
            continue;
        }
        if guard && !lines[prev].trim_end().ends_with('"') {
            continue;
        }
        let merged = format!("{} {}", lines[prev].trim_end(), t);
        lines[prev] = merged;
        lines[i] = String::new();
    }
}
/// 디스패치 위치에서 뽑은 메서드 이름 리터럴 하나.
struct Arm<'a> {
    name: &'a str,
    /// `starts_with("ns.")` 형태 — 이름 하나가 아니라 **접두어**를 가리킨다.
    is_prefix: bool,
}

/// 디스패치 위치의 메서드 이름 리터럴만 뽑는다 — `"ns.method" =>` (match 팔),
/// `method == "ns.method"` (if 형), `method.starts_with("ns.")` (접두어 형) 세 형태.
/// 응답 payload 의 문자열은 이 세 문법 위치가 아니라서 걸리지 않는다.
///
/// 접두어 형을 함께 보는 이유: 같은 라우터의 같은 블록 안에서도 **두 구현이 다른 모양의
/// 코드를 쓴다.** 실제로 헤드리스 pump 의 debug 갈래 셋 중 둘은 `==`, 하나는
/// `starts_with` 였고, `==` 만 보는 추출기는 셋 중 둘만 봤다. 그 상태에서 접두어 형으로
/// release 표면을 여는 팔은 **가드가 통째로 못 본다** — 위양성보다 나쁜 거짓 음성이다.
fn dispatch_methods(line: &str) -> Vec<Arm<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = line[from..].find('"') {
        let open = from + rel;
        let Some(close_rel) = line[open + 1..].find('"') else {
            break;
        };
        let close = open + 1 + close_rel;
        let name = &line[open + 1..close];
        let after = line[close + 1..].trim_start();
        let before = line[..open].trim_end();
        // `"a" | "b" => ..` 의 앞쪽 팔은 `=>` 가 아니라 `|` 가 뒤따른다. 같은 줄
        // 뒤쪽에 `=>` 가 있으면 그것도 팔로 센다. guard 가 붙은 팔(`"a" if .. =>`)은
        // 닫는 따옴표 뒤가 `if` 다 — 문자열 리터럴 바로 뒤의 `if` 낱말은 패턴 + guard
        // 자리에서만 문법적으로 성립하므로, guard 가 여러 줄로 넘어가 `=>` 가 같은 줄에
        // 없어도 팔로 센다. `"a" | "b" if ..` 의 앞쪽 이름도 같은 이유로 센다.
        let is_arm = after.starts_with("=>")
            || starts_with_guard(after)
            || (after.starts_with('|')
                && (after.contains("=>") || alternation_ends_in_guard(after)));
        let is_eq = before.ends_with("==");
        let is_prefix = before.ends_with("starts_with(") || before.ends_with("strip_prefix(");
        let shaped = name.contains('.')
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        if shaped && (is_arm || is_eq || is_prefix) {
            out.push(Arm { name, is_prefix });
        }
        from = close + 1;
    }
    out
}

/// `t` 가 match guard 의 `if` 낱말로 시작하는가(`if` 뒤가 공백이나 `(`).
fn starts_with_guard(t: &str) -> bool {
    t.strip_prefix("if")
        .is_some_and(|r| r.starts_with(|c: char| c.is_whitespace() || c == '('))
}

/// `| "b" | "c" if ..` 처럼 `|` 로 이은 따옴표 대안들 끝에 guard 가 오는가.
fn alternation_ends_in_guard(mut rest: &str) -> bool {
    while let Some(r) = rest.strip_prefix('|') {
        let r = r.trim_start();
        let Some(r) = r.strip_prefix('"') else {
            return false;
        };
        let Some(close) = r.find('"') else {
            return false;
        };
        rest = r[close + 1..].trim_start();
    }
    starts_with_guard(rest)
}

/// `lines[idx]` 바로 위의 연속 attribute/주석/빈 줄을 훑어 `debug_assertions` cfg 가
/// 붙어 있는지 본다. attribute·주석·빈 줄이 아닌 줄을 만나면 멈춘다.
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

/// 블록·다중행 항목에 붙은 cfg 를 그 항목이 덮는 줄 전체에 상속시킨다.
///
/// 구현은 [`cfg_span`] 에 있다 — 같은 판정이 필요한 통합 타깃이 둘이고,
/// 사본이 둘이면 갈리고 갈린 쪽은 조용하다.
fn debug_gated_lines(lines: &[String]) -> Vec<bool> {
    cfg_span::cfg_gated_lines(lines, "debug_assertions")
}

/// 스캔 결과 한 건 — 순수 함수 대조군에서 그대로 검사할 수 있게 값으로 돌려준다.
struct Scanned {
    name: String,
    is_prefix: bool,
    gated: bool,
}

/// 라우터 본문 줄들에서 디스패치 팔을 뽑고 각 팔이 debug 게이트 아래인지 표시한다.
fn scan_arms(lines: &[String]) -> Vec<Scanned> {
    let gated = debug_gated_lines(lines);
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for arm in dispatch_methods(line) {
            out.push(Scanned {
                name: arm.name.to_string(),
                is_prefix: arm.is_prefix,
                gated: gated[i] || is_debug_gated(lines, i),
            });
        }
    }
    out
}

/// release 표에 그 팔이 등재돼 있는가. 접두어 형은 그 접두어로 시작하는 항목이
/// 하나라도 있으면 등재된 것으로 본다.
fn registered_in_release_table(name: &str, is_prefix: bool) -> bool {
    if is_prefix {
        METHOD_TABLE.iter().any(|(m, _)| m.starts_with(name))
    } else {
        METHOD_TABLE.iter().any(|(m, _)| *m == name)
    }
}

/// 가드 4 — release 라우터의 팔은 release 표에 등재돼 있어야 한다.
#[test]
fn release_router_arms_are_registered_in_the_release_table() {
    let root = manifest_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for (rel, sig, min_arms) in RELEASE_ROUTERS {
        let path = root.join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("라우터 소스를 읽을 수 없다: {}: {e}", path.display()));
        let arms: Vec<Scanned> = fn_body_lines(&src, sig)
            .iter()
            .flat_map(|lines| scan_arms(lines))
            .collect();
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
            "`{rel}` 의 `{sig}` 에서 팔을 {here} 개밖에 못 찾았다(하한 {min_arms}) — \
             중괄호 스캔이나 팔 인식이 깨졌을 가능성이 크다. 조용한 미스캔은 \
             위양성보다 나쁘므로 여기서 실패시킨다."
        );
        scanned += here;
    }

    assert!(
        scanned > 200,
        "release 라우터 팔을 {scanned} 개밖에 못 찾았다 — 스캔 패턴이 깨졌을 가능성이 크다"
    );
    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "release 빌드에서도 컴파일되는 라우터에 팔이 있는데 release 표(METHOD_TABLE)에 \
         없는 메서드가 있다. **표에서 빼는 것만으로는 release 표면이 닫히지 않는다** — \
         권한 게이트가 로컬 호출자를 무조건 통과시키고 디스패치는 method_meta() 를 \
         부르지 않으므로, 팔이 남아 있으면 로컬 IPC 로 그대로 도달한다. 팔에 \
         `#[cfg(debug_assertions)]` 를 걸거나 debug 라우터로 옮겨라:\n  {}",
        offenders.join("\n  ")
    );
}

/// 가드 4 의 추출기가 **무엇을 보고 무엇을 못 보는지** 를 고정한다.
///
/// 이 가드는 소스를 읽어 컴파일러의 판정을 흉내 내는 자리다. 흉내가 어긋나는 방향이
/// 둘이라 둘 다 못 박는다 — 지워질 팔을 남았다고 하는 **위양성**, 남을 팔을 못 보는
/// **거짓 음성**. 뒤엣것이 더 나쁘다(가드가 초록인 채로 표면이 열린다).
mod extractor_mutations {
    use super::*;

    fn lines(src: &str) -> Vec<String> {
        src.lines().map(str::to_owned).collect()
    }

    /// 블록에 붙은 cfg 가 안쪽 팔들에 상속된다 — 이 가드가 고쳐진 자리.
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
        let arms = scan_arms(&lines(src));
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

    /// cfg 줄을 떼면 같은 팔이 다시 지목된다 — 상속이 **무조건 통과**가 아니다.
    #[test]
    fn removing_the_block_cfg_reopens_the_finding() {
        let src = "\
{
    let rpc_id = id.clone();
    if method == \"debug.lua.eval\" { run(); }
}
";
        let arms = scan_arms(&lines(src));
        assert!(
            arms.iter().any(|a| a.name == "debug.lua.eval" && !a.gated),
            "cfg 없는 블록의 팔을 gated 로 봤다 — 상속이 너무 넓다"
        );
    }

    /// 게이트된 블록이 닫힌 **뒤**의 팔은 여전히 release 다.
    #[test]
    fn an_arm_after_the_gated_block_is_still_release() {
        let src = "\
#[cfg(debug_assertions)]
{
    if method == \"debug.lua.eval\" { run(); }
}
if method == \"leaked.method\" { run(); }
";
        let arms = scan_arms(&lines(src));
        let leaked = arms
            .iter()
            .find(|a| a.name == "leaked.method")
            .expect("블록 뒤의 팔을 못 봤다");
        assert!(!leaked.gated, "블록의 게이트가 블록 밖으로 새어 나갔다");
    }

    /// 문자열 안의 중괄호가 블록을 일찍 닫지 않는다 — 닫히면 그 뒤가 release 로 보인다.
    #[test]
    fn braces_inside_strings_do_not_close_the_gated_block() {
        let src = "\
#[cfg(debug_assertions)]
{
    log(\"}\");
    if method == \"debug.lua.eval\" { run(); }
}
";
        let arms = scan_arms(&lines(src));
        assert!(
            arms.iter().any(|a| a.name == "debug.lua.eval" && a.gated),
            "문자열 안 `}}` 에 속아 블록이 일찍 닫혔다"
        );
    }

    /// guard 가 붙은 팔도 팔이다 — 닫는 따옴표 뒤 `=>` 만 보면 guard 한 줄로
    /// release 표 등재 없이 release 라우터에 팔이 들어온다.
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
        let names: Vec<String> = scan_arms(&lines(src)).into_iter().map(|a| a.name).collect();
        for want in [
            "leaked.guarded",
            "leaked.alt_a",
            "leaked.alt_b",
            "leaked.multiline",
        ] {
            assert!(
                names.iter().any(|n| n == want),
                "guard 가 붙은 팔 `{want}` 을 못 봤다: {names:?}"
            );
        }
    }

    /// rustfmt 가 guard 를 다음 줄로 내린 형태도 팔이다 — 이쪽은 포매터가 되돌리지
    /// 않는다. 병합은 본문 추출(`fn_body_lines`)에서 일어나므로 같은 순서로 부른다.
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
        let mut ls = lines(src);
        join_wrapped_arms(&mut ls);
        let names: Vec<String> = scan_arms(&ls).into_iter().map(|a| a.name).collect();
        for want in ["leaked.wrapped", "leaked.wrapped_a", "leaked.wrapped_b"] {
            assert!(
                names.iter().any(|n| n == want),
                "다음 줄로 내린 guard 의 팔 `{want}` 을 못 봤다: {names:?}"
            );
        }
    }

    /// 평범한 `if` 문은 윗줄에 붙지 않는다 — 윗줄이 닫는 따옴표로 끝날 때만 guard 다.
    #[test]
    fn a_plain_if_statement_is_not_joined_as_a_guard() {
        let src = "\
let x = run();
if method == \"kept.eq\" { run(); }
";
        let mut ls = lines(src);
        join_wrapped_arms(&mut ls);
        assert_eq!(ls[1].trim(), "if method == \"kept.eq\" { run(); }");
        assert_eq!(scan_arms(&ls).len(), 1);
    }

    /// 등재 판정이 접두어와 정확 이름을 구분한다.
    #[test]
    fn a_prefix_arm_is_registered_only_if_the_table_has_something_under_it() {
        assert!(
            !registered_in_release_table("no.such.namespace.", true),
            "아무것도 없는 접두어를 등재됐다고 봤다"
        );
        let (first, _) = METHOD_TABLE[0];
        assert!(
            registered_in_release_table(first, false),
            "표에 있는 이름을 미등재로 봤다 — 대조군이 죽었다"
        );
    }
}
