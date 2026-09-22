//! e2e 단일 인스턴스 원칙 가드 — 테스트가 tasty GUI 프로세스를 필요 이상으로 띄우면 fail 한다.
//!
//! 원칙(ADR-0090, `docs/dev-guide/e2e-tests.md` §1): **격리 단위는 프로세스가 아니라
//! workspace 다.** tasty 인스턴스는 test binary 당 1 개를 `common::shared()` 로 공유하고,
//! 테스트별 격리는 `create_workspace()` 로 확보한다. GUI 창은 뜰 때마다 OS 포커스를
//! 훔치고 기동 비용도 크므로, 인스턴스 수는 그 자체로 관리 대상이다.
//!
//! 이 가드는 두 축을 본다 — 어느 한 축만으로는 회귀를 다 못 잡는다.
//!
//! | 축 | 잡는 회귀 | 상수 |
//! |---|---|---|
//! | 파일당 전용 spawn 호출 수 | `#[test]` 마다 인스턴스를 띄우는 형태 | [`ALLOWLIST_FILES`] (미등록 = 0 회) |
//! | 인스턴스를 띄우는 test 파일 개수 | 새 binary 가 늘어 총량이 다시 증가하는 형태 | [`EXPECTED_INSTANCE_TESTS`] |
//! | 어느 바이너리를 띄우는가 | 하네스가 각자 다른 바이너리를 고르는 형태 | [`BIN_SELECTION_ALLOWLIST`] |
//!
//! 세 번째 축이 여기 있는 이유는 앞의 둘과 **같은 목록을 근거로 삼기 때문**이다 —
//! "인스턴스를 띄우는 파일이 무엇인가" 를 아는 자리가 둘이 되면 그 둘이 갈린다.
//!
//! **동적 가드(실행 중 tasty PID 개수 세기)는 일부러 채택하지 않았다** — 가드가 공유
//! 인스턴스 위에서 돌면 자기 프로세스를 포함해 세고, cargo 는 test 타겟을 순차 실행하므로
//! "binary 당 1 개" 는 설계상 허용값이라 PID 수만으로 위반과 정상을 가릴 수 없으며,
//! 개발자 머신의 실사용 tasty 가 그대로 오탐이 된다. 동적 관찰은 수동 검증 절차로 남긴다.
//!
//! 선례: `crates/tasty-doc-guards/tests/no_emoji_in_source.rs`(`ALLOWLIST_FILES` 정적 스캔),
//! `tests/cli_naming_count_drift.rs`(카운트 스냅샷 고정).

use std::collections::BTreeSet;
use std::path::Path;

/// 문서 경로 — 위반 메시지에 실어 다음 작업자가 원칙을 찾아갈 수 있게 한다.
const DOC: &str = "docs/dev-guide/e2e-tests.md (원칙 근거: docs/adr/0090-test-isolation-by-workspace-not-process.md)";

/// 이 가드 자신 — 아래 마커 문자열을 상수로 담고 있어 스캔하면 자기 자신을 잡는다.
const SELF_FILE: &str = "e2e_single_instance_guard.rs";

/// **전용** 인스턴스를 띄우는 호출의 마커. 공유 진입점(`common::shared()`)은 여기 없다 —
/// 그것이 기본 경로이고 몇 번 부르든 프로세스는 하나이기 때문이다.
const DEDICATED_SPAWN_MARKERS: &[&str] = &[
    // tests/common 하네스의 전용 인스턴스 생성자 두 개.
    "TastyInstance::spawn(",
    "TastyInstance::spawn_with_inherit_cwd(",
    // tests/webhook_common 하네스. 같은 바이너리를 띄우므로 창 비용이 같다.
    // 이쪽은 공유 진입점이 없어 builder 호출 하나가 곧 인스턴스 하나다. 재시작 전용
    // 진입점도 같은 비용이라 함께 센다 — 빠뜨리면 인스턴스가 가드 밖에서 늘어난다.
    "WebhookInstance::builder(",
    "WebhookInstance::builder_for_restart(",
];

/// 인스턴스를 띄우는 파일인지 판정하는 마커 — 전용/공유를 가리지 않는다.
/// **호출부가 아니라 하네스 모듈 선언을 본다**: 진입점은 `use gui_common::shared;` 처럼
/// import 해 쓸 수 있어 `gui_common::shared(` 같은 호출부 문자열이 아예 안 나타날 수 있는
/// 반면, 모듈 선언은 tasty 프로세스를 띄우는 하네스를 끌어다 쓴다는 사실 그 자체다.
/// (`attach_common` 은 프로토콜 frame 헬퍼일 뿐 프로세스를 띄우지 않아 여기 없다.)
const INSTANCE_HARNESS_MARKERS: &[&str] =
    &["mod common;", "mod gui_common;", "mod webhook_common;"];

/// 전용 인스턴스가 정당한 파일과 그 상한. **미등록 파일은 0 회** — 즉 새 test binary 는
/// `common::shared()` 를 쓰고 workspace 로 격리해야 한다. 등록은 "프로세스 경계 자체가
/// 검증 대상" 일 때만 정당하다.
///
/// - `tests/soak_memory.rs` (1): 프로세스 트리 RSS 를 **외부에서** 측정한다(`pid()`).
///   다른 테스트의 활동이 섞이면 측정 자체가 무의미해진다. 전수 `#[ignore]`.
/// - `tests/attach_convert_cwd_loopback.rs` (1): 검증 대상 동작이 `inherit_cwd` 설정에
///   게이트되는데, 그 값은 격리 HOME 의 `config.toml` 에 미리 쓰는 *기동 시점* 설정이라
///   이미 떠 있는 인스턴스에 런타임으로 바꿔 끼울 수 없다. 같은 파일의 나머지 테스트는
///   공유 인스턴스를 쓴다.
/// - `tests/hook_env_integration.rs` (1): 웹훅 하네스는 `TASTY_HOME`/`webhooks.toml` 을
///   인스턴스별로 시딩해야 해 공유 진입점이 없다.
/// - `tests/webhook_integration.rs` (2): 위와 같은 이유 + **재시작 시나리오**라 같은 HOME 을
///   물려받는 두 번째 인스턴스가 검증 대상 그 자체다(영속성 확인).
const ALLOWLIST_FILES: &[(&str, usize)] = &[
    ("tests/soak_memory.rs", 1),
    ("tests/attach_convert_cwd_loopback.rs", 1),
    ("tests/hook_env_integration.rs", 1),
    ("tests/webhook_integration.rs", 2),
];

/// tasty 프로세스를 띄우는 test 파일의 **전체 목록**. 파일당 spawn 수를 아무리 조여도
/// binary 가 늘면 총량은 다시 증가하므로(각 binary 가 1 개씩만 띄워도 마찬가지) 목록 자체를
/// 고정한다. 새 e2e binary 가 정말 필요하면 여기 추가하면서 그 필요를 한 번 되짚게 된다.
const EXPECTED_INSTANCE_TESTS: &[&str] = &[
    "tests/attach_attention_loopback.rs",
    "tests/attach_convert_cwd_loopback.rs",
    "tests/attach_git_query_loopback.rs",
    "tests/attach_list_dir_loopback.rs",
    "tests/attach_local_creation_tap.rs",
    // ADR-0255 의 markdown 원문 채널. 앞의 두 채널(list_dir · git_query)과 같은 가족의
    // 세 번째 타깃이라 같은 자리에 둔다. **이 스위트만 번들 plugin 스테이징을 켠다**
    // (`spawn_diag::SUITES_THAT_CALL_BUNDLED_PLUGINS`) — 검증 대상이 실제 markdown
    // plugin 이 만든 surface 라 빈 번들 루트로는 `unknown surface kind: markdown` 이
    // 된다. 그 대가로 격리 홈이 커진다.
    "tests/attach_markdown_content_loopback.rs",
    "tests/attach_silent_disconnect.rs",
    // forward 가 아닌 원인의 구조 변경 역반영(ADR-0481)과 사라진 anchor 의 회신(ADR-0482).
    // 검증 대상이 서버와 holder 사이의 **실제 attach 스트림**이라 루프백 attach 로 두
    // 끝(서버 인스턴스 + 붙는 client)이 필요하다 — 위 attach 가족과 같은 이유로 별도 binary 다.
    "tests/attach_structure_sync_loopback.rs",
    "tests/e2e_tests.rs",
    "tests/gui_tests.rs",
    "tests/hook_env_integration.rs",
    "tests/hooks_detection_e2e.rs",
    "tests/shared_instance_harness.rs",
    "tests/soak_memory.rs",
    "tests/webhook_integration.rs",
];

/// `CARGO_BIN_EXE_tasty` 를 **직접** 써도 되는 자리와 그 이유.
///
/// 인스턴스를 띄우는 하네스는 `spawn_diag::instance_bin()` 하나를 거쳐야 한다 —
/// 그래야 "무엇을 띄우는가" 를 한 곳에서 바꿀 수 있고, 하네스마다 다른 바이너리를
/// 고르는 상태로 갈리지 않는다(근거: `docs/adr/0127-e2e-harness-binary-selection.md`).
/// 아래 둘은 그 규칙 밖이다.
///
/// - `tests/gui_common/mod.rs`: **GUI 바이너리 자체가 검증 대상**이다(실제 데스크톱
///   입력 주입). override 를 따라 headless 를 띄우면 검증이 성립하지 않는다.
/// - `tests/cli_stdout_broken_pipe.rs`: 인스턴스를 띄우지 않는다 — cargo 가 방금 빌드한
///   바이너리의 **CLI stdout 동작**이 검증 대상이라 그 바이너리여야 한다.
///
/// **면제는 파일 통째가 아니라 횟수까지 묶는다.** 파일 단위로 면제하면 그 파일 안에
/// 새 spawn 이 하나 더 생겨도 가드가 침묵한다 — 면제가 검출보다 넓어지는 형태다.
/// 여기 적힌 수를 넘으면 그 파일도 위반이 된다.
const BIN_SELECTION_ALLOWLIST: &[(&str, usize)] = &[
    ("tests/gui_common/mod.rs", 1),
    ("tests/cli_stdout_broken_pipe.rs", 1),
];

/// 바이너리 선택의 유일한 자리 — 이 파일 안에서는 `CARGO_BIN_EXE_tasty` 가 정상이다.
/// chokepoint 도 횟수를 묶는다(면제를 좁게 두는 것은 여기도 같다).
const BIN_SELECTION_CHOKEPOINT: (&str, usize) = ("tests/spawn_diag/mod.rs", 1);

/// [`BIN_SELECTION_MARKER`] 가 최소 이만큼은 나와야 스캔이 살아 있는 것으로 본다.
/// chokepoint 1 + allowlist 2 = 3 이 현재 하한이다. 경로 계산이 틀려 스캔 대상이
/// 0 개가 되면 **가드가 조용히 초록이 된다** — 그것을 잡는 유일한 장치다.
const BIN_SELECTION_MIN_HITS: usize = 3;

/// 스캔이 찾는 문자열 — **호출 형태**만 본다. 산문(doc 주석)이 이 이름을 언급하는
/// 것은 위반이 아니고, `env!(` 이 붙은 자리가 곧 "이 파일이 바이너리를 직접 고른다"
/// 이다.
///
/// **`concat!` 로 쪼개 둔 것이 핵심이다.** 한 리터럴로 적으면 이 가드 파일 자신이
/// 자기 패턴에 걸려 "가드 파일 통째 제외" 라는 면제가 하나 더 필요해진다. 쪼개면
/// 소스에 그 문자열이 이어진 형태로 존재하지 않아 **면제 없이** 자기 자신까지
/// 스캔 대상으로 둘 수 있다. 면제는 적을수록 좋다.
const BIN_SELECTION_MARKER: &str = concat!("env!(\"CARGO_BIN_EXE_", "tasty\")");

/// 한 파일이 바이너리를 **직접** 고르는 자리의 줄 번호(1-기준).
///
/// 파일 순회·경로 처리와 분리한 **순수 함수**다 — 면제를 겨냥한 변이를 레포에 진짜
/// 위반을 심어서가 아니라 **합성 입력**으로 찌를 수 있어야 하기 때문이다(아래
/// `detects_*` / `does_not_flag_*` 테스트가 그 변이를 영구히 붙박는다).
fn direct_binary_pick_lines(contents: &str) -> Vec<usize> {
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(BIN_SELECTION_MARKER))
        .map(|(i, _)| i + 1)
        .collect()
}

/// 그 파일에 허용된 직접 선택 횟수. 미등록 파일은 0.
fn direct_pick_allowance(rel: &str) -> usize {
    if rel == BIN_SELECTION_CHOKEPOINT.0 {
        return BIN_SELECTION_CHOKEPOINT.1;
    }
    BIN_SELECTION_ALLOWLIST
        .iter()
        .find(|(f, _)| *f == rel)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

/// `tests/` 아래 **모든** `.rs` 를 모은다 — 하네스 모듈(`tests/<name>/mod.rs`)이
/// 실제로 spawn 하는 자리라 깊이 1 만 봐서는 이 축을 못 본다.
fn all_test_sources(tests_dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![tests_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("tests/ 하위 read 실패")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(tests_dir.parent().expect("tests/ 의 부모"))
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let contents = std::fs::read_to_string(&path).expect("test 파일 read 실패");
            out.push((rel, contents));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn only_one_place_decides_which_binary_the_harness_spawns() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = 0usize;
    let mut violations = Vec::new();

    // 이 파일 자신도 스캔한다 — `BIN_SELECTION_MARKER` 를 `concat!` 로 쪼개 둔 덕에
    // 자기 패턴에 걸리지 않아 자기 제외 면제가 필요 없다.
    for (rel, contents) in all_test_sources(&root.join("tests")) {
        let lines = direct_binary_pick_lines(&contents);
        if lines.is_empty() {
            continue;
        }
        hits += lines.len();
        let allowed = direct_pick_allowance(&rel);
        if lines.len() > allowed {
            violations.push(format!(
                "  {rel} — {} 회 (허용 {allowed} 회, 줄 {lines:?})",
                lines.len()
            ));
        }
    }

    assert!(
        hits >= BIN_SELECTION_MIN_HITS,
        "스캔이 {hits} 건만 찾았다(하한 {BIN_SELECTION_MIN_HITS}) — 경로 계산이 틀려 \
         대상이 비었을 가능성이 크다. 가드가 아무것도 안 보면서 초록이 되는 형태다.\n\
           ★ 판별 — 이 하한은 센 값이 아니라 **명부에서 도출된 값**이다: chokepoint 1 + \
           [`BIN_SELECTION_ALLOWLIST`] 의 허용 횟수 합. 그래서 갈래가 딱 둘이다. 명부에 적힌 자리들이 \
           소스에 그대로 있으면 순회가 그 파일들에 안 닿은 것이고, 명부의 자리가 소스에서 사라졌으면 \
           모수가 정말 줄어든 것이다. 어느 쪽인지는 명부의 경로를 `git ls-files` 로 확인하면 갈린다 — \
           셋뿐이라 눈으로 다 본다.\n\
           ★ 이 수를 손으로 내려서 통과시키지 마라. 뒤쪽(자리가 정말 사라진 것)이면 고칠 곳은 하한이 \
           아니라 **명부**다 — 명부에서 그 줄을 지우면 하한은 따라 내려온다. 명부는 그대로 두고 하한만 \
           내리면 면제가 실재보다 넓어진 채로 남는다."
    );

    assert!(
        violations.is_empty(),
        "하네스가 띄울 바이너리는 `spawn_diag::instance_bin()` 한 곳에서 정한다 — \
         `CARGO_BIN_EXE_tasty` 를 직접 부르면 그 자리는 override 를 따르지 않아, \
         같은 완주 안에서 하네스마다 다른 바이너리를 띄우게 된다. 정당한 예외면 \
         BIN_SELECTION_ALLOWLIST 에 이유와 함께 등록할 것. 근거: {DOC}\n{}",
        violations.join("\n")
    );
}

// ── 판정기 단위 테스트 ────────────────────────────────────────────────────
// 아래 넷은 이 가드에 걸었던 변이를 **합성 입력으로 붙박은 것**이다. 레포에 진짜
// 위반을 심어 돌리는 일회성 변이와 달리, 판정기를 넓히거나 좁히면 여기서 깨진다.

#[test]
fn detects_a_file_that_picks_the_binary_itself() {
    let src = "mod common;\nlet c = Command::new(env!(\"CARGO_BIN_EXE_tasty\"));\n";
    assert_eq!(direct_binary_pick_lines(src), vec![2]);
}

#[test]
fn detects_an_extra_pick_inside_an_exempted_file() {
    // 면제를 겨냥한 변이 — 면제된 파일이라도 **허용 횟수를 넘으면** 걸려야 한다.
    // 면제를 파일 통째로 두면 이 입력이 조용히 통과한다.
    let src = "let a = env!(\"CARGO_BIN_EXE_tasty\");\nlet b = env!(\"CARGO_BIN_EXE_tasty\");\n";
    let lines = direct_binary_pick_lines(src);
    assert_eq!(lines.len(), 2);
    assert!(lines.len() > direct_pick_allowance("tests/gui_common/mod.rs"));
}

#[test]
fn does_not_flag_prose_that_merely_names_the_variable() {
    // doc 주석이 이름을 언급하는 것은 위반이 아니다 — 호출 형태(`env!(`)만 본다.
    let src = "//! 하네스는 CARGO_BIN_EXE_tasty 를 띄우곤 했다.\n/// CARGO_BIN_EXE_tasty\n";
    assert!(direct_binary_pick_lines(src).is_empty());
}

#[test]
fn intentionally_does_not_see_a_runtime_lookup() {
    // **의도된 false negative.** 런타임에 경로를 만드는 형태는 이 가드가 못 본다 —
    // 컴파일 시점 매크로만 보기 때문이다. 판정기를 나중에 넓히면 이 테스트가 깨지고,
    // 그때 그것이 의도된 확장인지 사람이 판단하게 된다(한계와 버그의 구분).
    let src = "let bin = std::env::var(\"TASTY_E2E_BIN\").unwrap();\n";
    assert!(direct_binary_pick_lines(src).is_empty());
}

/// `tests/*.rs` (깊이 1) 만 모은다 — test binary 의 진입 파일이다. 하네스 모듈은
/// `tests/<name>/mod.rs` 에 있어 자연히 제외되며, 거기 있는 `pub fn spawn` 은 호출이
/// 아니라 정의라 세면 안 된다.
fn test_entry_files(tests_dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(tests_dir).expect("tests/ 디렉토리 read 실패");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if name == SELF_FILE {
            continue;
        }
        let contents = std::fs::read_to_string(&path).expect("test 파일 read 실패");
        out.push((format!("tests/{name}"), contents));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn count_markers(contents: &str, markers: &[&str]) -> usize {
    markers.iter().map(|m| contents.matches(m).count()).sum()
}

#[test]
fn dedicated_instance_spawns_stay_within_allowlist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for (rel, contents) in test_entry_files(&root.join("tests")) {
        let found = count_markers(&contents, DEDICATED_SPAWN_MARKERS);
        let allowed = ALLOWLIST_FILES
            .iter()
            .find(|(f, _)| *f == rel)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        if found > allowed {
            violations.push(format!(
                "  {rel} — 전용 인스턴스 spawn {found} 회 (허용 {allowed} 회)"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "테스트가 tasty 인스턴스를 필요 이상으로 띄운다. GUI 창은 뜰 때마다 OS 포커스를 \
         훔치므로 인스턴스는 test binary 당 1 개가 기본이다 — `common::shared()` 를 쓰고 \
         격리는 `create_workspace()` 의 **workspace 단위**로 하라. 프로세스 경계 자체가 \
         검증 대상이라 전용 인스턴스가 꼭 필요하면 ALLOWLIST_FILES 에 이유와 함께 등록할 것. \
         원칙: {DOC}\n{}",
        violations.join("\n")
    );
}

#[test]
fn instance_spawning_test_files_match_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let actual: BTreeSet<String> = test_entry_files(&root.join("tests"))
        .into_iter()
        .filter(|(_, contents)| count_markers(contents, INSTANCE_HARNESS_MARKERS) > 0)
        .map(|(rel, _)| rel)
        .collect();
    let expected: BTreeSet<String> = EXPECTED_INSTANCE_TESTS
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let added: Vec<&String> = actual.difference(&expected).collect();
    let removed: Vec<&String> = expected.difference(&actual).collect();

    assert!(
        added.is_empty() && removed.is_empty(),
        "tasty 인스턴스를 띄우는 test 파일 목록이 스냅샷과 다르다. 파일당 spawn 수를 조여도 \
         binary 가 늘면 총량은 다시 증가하므로 목록 자체를 고정한다 — 새 파일이 정말 별도 \
         binary 여야 하는지(기존 e2e 파일에 시나리오를 추가하면 되는 것은 아닌지) 먼저 \
         확인하고, 맞다면 EXPECTED_INSTANCE_TESTS 에 추가하라. 원칙: {DOC}\n\
         \x20 추가됨: {added:?}\n\x20 사라짐: {removed:?}"
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
///
/// 이 파일의 겹은 **둘**이고 가리키는 것이 같은 갈래(테스트 파일 경로)라 한 자리에서 본다.
#[test]
fn both_allowlists_point_at_test_files_that_exist() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let cited = ALLOWLIST_FILES
        .iter()
        .chain(BIN_SELECTION_ALLOWLIST.iter())
        .map(|(rel, _)| *rel);
    let missing = tasty_doc_guards::missing_referents(root, cited);
    assert!(
        missing.is_empty(),
        "면제가 없는 테스트 파일을 가리킨다 — 옮겼으면 항목도 옮기고, 사라졌으면 지워라: {missing:?}"
    );
}

/// 네 번째 축 — **스위트별 데몬 판정의 명부가 이 파일의 명부와 갈리지 않는가.**
///
/// `tests/spawn_diag/mod.rs` 의 `HEADLESS_OK_SUITES` 는 "인스턴스를 띄우는 파일이
/// 무엇인가" 를 아는 **두 번째 자리**다. 이 파일 모듈 doc 이 축 ③을 여기 둔 이유가
/// 그대로 적용된다 — 그 자리가 둘이 되면 둘이 갈리고, 갈린 쪽은 조용하다.
/// 오타 난 이름은 명부에 없는 것으로 취급돼 `SameCombo` 로 떨어지므로 **아무 에러 없이
/// 최적화만 사라진다.**
///
/// 그래서 **양방향으로 본다**: 명부에 없는 인스턴스 스위트가 있으면(=분류를 안 했다)
/// 실패하고, 명부에만 있고 실재하지 않는 이름이 있어도(=오타·이름 변경) 실패한다.
/// 한쪽만 보면 늘어나는 것과 썩는 것 중 하나를 놓친다.
#[test]
fn daemon_kind_roster_matches_instance_test_roster() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("tests/spawn_diag/mod.rs"))
        .expect("spawn_diag 를 읽을 수 있어야 한다");
    let body = src
        .split_once("const HEADLESS_OK_SUITES: &[&str] = &[")
        .and_then(|(_, rest)| rest.split_once("];"))
        .map(|(body, _)| body)
        .expect("HEADLESS_OK_SUITES 명부를 못 찾았다 — 이름이 바뀌었으면 이 가드도 함께 고쳐라");
    let headless_ok: BTreeSet<String> = body
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();

    // 자기 조합의 데몬이 필요한 것들. `gui_tests` 는 이 경로를 아예 안 쓰지만
    // (BIN_SELECTION_ALLOWLIST) 데몬이 gui 여야 하는 것은 같다.
    // `attach_structure_sync_loopback` 은 테스트 쪽 `cfg(feature` 가 0 이지만 **데몬 쪽**이
    // 조합마다 다른 호출측을 잰다 — forward 회신을 gui 데몬은 `apply_forwarded_structural_op`,
    // 헤드리스 데몬은 `apply_structural_ops` 가 만든다(ADR-0482). 헤드리스 데몬을 받으면 gui
    // 완주가 헤드리스 호출측을 한 번 더 잴 뿐이다. 실측 2026-09-23: gui 호출측을 끊는 변이가
    // SameCombo 에서는 빨갛고, HeadlessOk + override 에서는 초록이었다(ADR-0170 보강).
    let same_combo: BTreeSet<String> = ["e2e_tests", "gui_tests", "attach_structure_sync_loopback"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let instance_suites: BTreeSet<String> = EXPECTED_INSTANCE_TESTS
        .iter()
        .map(|p| {
            Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    let classified: BTreeSet<String> = headless_ok.union(&same_combo).cloned().collect();
    let unclassified: Vec<&String> = instance_suites.difference(&classified).collect();
    let stale: Vec<&String> = classified.difference(&instance_suites).collect();

    assert!(
        unclassified.is_empty() && stale.is_empty(),
        "스위트별 데몬 판정 명부가 EXPECTED_INSTANCE_TESTS 와 갈렸다. 새 인스턴스 스위트는 \
         `spawn_diag::HEADLESS_OK_SUITES` 에 넣거나(IPC/attach 만 쓴다) 조합 의존 단언이 \
         있거나 단언의 대상이 데몬 조합마다 다른 코드 경로면 이 테스트의 same_combo 에 넣어라 — 분류를 안 하면 안전한 쪽(SameCombo)으로 \
         떨어져 GPU 부팅을 계속 탄다. 원칙: {DOC}\n\
         \x20 분류 안 된 스위트: {unclassified:?}\n\x20 명부에만 있는 이름: {stale:?}"
    );
}

// ---------------------------------------------------------------------------
// 격리 홈 축 — 하네스가 무엇을 띄우는가와 **어디에 띄우는가**는 다른 물음이다
// ---------------------------------------------------------------------------

/// 인스턴스를 띄우는 호출 형태. `concat!` 로 쪼갠 이유는 [`BIN_SELECTION_MARKER`] 와
/// 같다 — 이 가드 파일 자신이 자기 패턴에 걸리면 "가드 파일 제외" 라는 면제가 하나 더
/// 필요해진다.
const SPAWN_FORMS: &[&str] = &[
    concat!("Command::new(spawn_diag::", "instance_bin())"),
    concat!("Command::new(env!(\"CARGO_BIN_EXE_", "tasty\"))"),
];

/// 그 spawn 이 전용 tasty 루트를 세우는가.
const ISOLATED_HOME_MARKER: &str = concat!(".env(\"TASTY_", "HOME\"");

/// 스캔이 살아 있는지 보는 하한. 실측 2026-09-07: **5**(하한 5, 여유 0) —
/// `gui_common` 1 · `common` 1 · `webhook_common` 2 · `cli_stdout_broken_pipe` 1.
/// 여유 0 은 여기서 의도한 값이다: 자리가 하나 줄면 빨개지고, 그때 아래 문장대로
/// 이 수를 함께 내린다. 줄어든 것이 삭제인지 스캔 고장인지는 사람이 가른다.
/// 경로 계산이 틀려 대상이 0 개가 되면 **가드가 조용히 초록**이 되는데, 그것을 잡는
/// 유일한 장치다. 자리를 지웠으면 이 수도 함께 내려라.
const ISOLATED_HOME_MIN_SPAWNS: usize = 5;

/// 한 소스의 (spawn 자리 수, 전용 홈 지정 수).
fn spawn_and_home_counts(src: &str) -> (usize, usize) {
    let spawns = SPAWN_FORMS.iter().map(|f| src.matches(f).count()).sum();
    (spawns, src.matches(ISOLATED_HOME_MARKER).count())
}

/// 하네스가 띄우는 tasty 는 **전용 tasty 루트**를 받아야 한다.
///
/// 안 받으면 자식은 이 머신에서 tasty 를 실제로 쓰는 사람의 홈을 쓴다 — 번들 plugin 을
/// 거기 설치하고, 거기 저장된 레이아웃을 **복원한다.** 뒤쪽이 조용한 쪽이다: 복원된
/// workspace 에는 surface 가 여럿인데 화면에 자리(rect)를 가진 것은 활성 탭 하나뿐이라,
/// 그 자리를 안 겨눈 마우스 주입은 **아무 일도 안 하고 성공처럼 보인다.** 그러면 시험은
/// 빈 출력을 "보고가 없다" 로 읽고, 음성 단정(`!contains(..)`)은 **그 빈 출력 때문에
/// 오히려 통과한다.** 실측(같은 커밋·같은 Xvfb, 이 변수 하나만 바꿈): 실제 홈에서는
/// 주입 6 회가 전부 무효였고 격리 홈에서는 6 회가 전부 유효했다.
///
/// 이 축이 오래 안 걸린 이유는 소비 스위트(`tests/gui_tests.rs`)가 전수 `#[ignore]` 라
/// **자동 채널에서 한 번도 안 돌기 때문**이다. 사람이 손으로 돌릴 때만 드러난다.
///
/// **이 가드가 답하지 않는 것**: 판정 단위가 **파일**이지 호출 자리가 아니다. 한 파일에
/// spawn 이 둘이고 전용 홈 지정도 둘인데 그중 한 짝이 어긋나 있으면 통과한다. 자리 단위로
/// 묶으려면 구문 분석이 필요하고, 그 비용은 지금 이 축이 잡으려는 결함(하네스가 통째로
/// 안 세우는 것)에 비해 크다. 파일 안에서 짝이 어긋나는 형태가 실제로 나오면 그때 좁혀라.
#[test]
fn every_harness_spawn_gets_its_own_tasty_home() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = root.join("tests");
    let mut total_spawns = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for (rel, src) in all_test_sources(&tests_dir) {
        let (spawns, homes) = spawn_and_home_counts(&src);
        if spawns == 0 {
            continue;
        }
        total_spawns += spawns;
        if homes < spawns {
            offenders.push(format!("{rel} — spawn {spawns} · 전용 홈 지정 {homes}"));
        }
    }

    assert!(
        total_spawns >= ISOLATED_HOME_MIN_SPAWNS,
        "spawn 자리를 {total_spawns} 개밖에 못 찾았다(하한 {ISOLATED_HOME_MIN_SPAWNS}). \
         스캔이 죽었거나 호출 형태가 바뀐 것이다 — 0 건은 초록이 아니라 미측정이다."
    );
    assert!(
        offenders.is_empty(),
        "하네스가 전용 tasty 루트 없이 인스턴스를 띄운다 — 그 회차는 사용자의 진짜 홈에 \
         설치하고 거기 저장된 레이아웃을 복원한다:\n\x20 {offenders:?}"
    );
}

/// 아래 대조들은 픽스처를 **마커 상수에서 만든다.** 손으로 적으면 이 가드 파일의 원문에
/// 그 형태가 그대로 존재하게 되어 스캔이 자기 픽스처를 하네스 spawn 으로 센다(실측:
/// 처음에 그렇게 짜서 이 파일이 `spawn 4 · 홈 0` 인 위반자로 나왔다). 그러면 두 가지가
/// 동시에 망가진다 — 면제를 하나 더 만들어야 하고, [`ISOLATED_HOME_MIN_SPAWNS`] 하한이
/// **픽스처만으로도 채워져** 마커가 실제 코드와 어긋나도 초록이 된다.
#[test]
fn detects_a_spawn_that_never_sets_a_tasty_home() {
    let src = format!("let c = {};\nc.env(\"HOME\", &h);\n", SPAWN_FORMS[0]);
    let (spawns, homes) = spawn_and_home_counts(&src);
    assert_eq!((spawns, homes), (1, 0), "spawn 은 세고 홈은 못 찾아야 한다");
}

#[test]
fn detects_a_second_spawn_that_the_first_home_does_not_cover() {
    let src = format!(
        "{}{}, &h);\n{};\n",
        SPAWN_FORMS[0], ISOLATED_HOME_MARKER, SPAWN_FORMS[1]
    );
    let (spawns, homes) = spawn_and_home_counts(&src);
    assert!(
        homes < spawns,
        "두 번째 spawn 이 홈 없이 늘면 걸려야 한다: spawn {spawns} · 홈 {homes}"
    );
}

#[test]
fn does_not_flag_a_spawn_that_sets_its_own_home() {
    let src = format!("{}{}, &iso);\n", SPAWN_FORMS[0], ISOLATED_HOME_MARKER);
    let (spawns, homes) = spawn_and_home_counts(&src);
    assert!(homes >= spawns, "짝이 맞으면 위반이 아니다");
}

#[test]
fn does_not_flag_prose_that_merely_names_the_isolated_home() {
    let src = "//! 하네스는 전용 tasty 루트를 세운다. 그 자리는 spawn 이 아니다.\n";
    let (spawns, _) = spawn_and_home_counts(src);
    assert_eq!(spawns, 0, "산문은 spawn 자리가 아니다");
}

// ---------------------------------------------------------------------------
// 번들 opt-in 축 — 무엇을 어디에 띄우는가와 **무엇을 들고 띄우는가**는 또 다른 물음이다
// ---------------------------------------------------------------------------

/// 호스트를 실제로 부팅시키는 spawn 의 표지. `--port-file` 을 넘긴다는 것은 **인스턴스가
/// 떠서 포트를 쓴다**는 뜻이고, 부팅하면 번들 plugin 설치 경로를 탄다.
///
/// **`SPAWN_FORMS` 를 안 쓴 이유**: 그쪽은 부팅하지 않는 spawn 도 센다.
/// `tests/cli_stdout_broken_pipe.rs` 는 host 없이 끝나는 로컬 CLI 명령만 띄우므로 번들을
/// 스테이징하지 않는다 — 거기에 opt-in 을 요구하면 근거 없는 요구가 된다.
///
/// **따옴표를 품는 이유도 측정된 것이다.** 따옴표를 빼면 `tests/webhook_integration.rs` 의
/// doc 주석(백틱으로 이 플래그를 언급한다)이 모집단에 들어온다. 그 파일은 자기 Command 를
/// 안 만들고 `webhook_common` 을 거치므로 위반자가 아니다 — 산문을 코드로 세는 오탐이다.
const BOOT_ARG_MARKER: &str = concat!("\"--port-", "file\"");

/// 그 하네스가 번들 opt-in 을 거치는가.
const BUNDLE_OPT_IN_MARKER: &str = concat!("apply_bundle_", "opt_in");

/// 스캔이 살아 있는지 보는 하한. 지금 부팅 하네스는 3 이다
/// (`tests/common` · `tests/webhook_common` · `tests/gui_common`).
/// 경로나 마커가 어긋나 모집단이 0 이 되면 이 축은 **자극과 무관하게 초록**이 된다.
const BOOTING_HARNESS_MIN: usize = 3;

/// 부팅하는 하네스는 전부 번들 opt-in 을 거친다.
///
/// ## 왜 이 축이 따로 필요한가
///
/// 안 거치면 그 하네스가 띄우는 인스턴스는 부팅마다 격리 홈에 번들 전량을 복사한다
/// (debug 45 파일 ≈ 1.1 GB). 그 비용은 **초록이라 안 보인다** — 복사는 성공하고 단정은
/// 아무것도 안 건드린다. 결정과 명부는 [ADR-0182](docs/adr/0182-test-instances-do-not-stage-bundled-plugins-by-default.md),
/// 판정은 `tests/spawn_diag` 한 곳이다.
///
/// **이 가드가 생긴 이유가 실측이다.** 결정이 내려질 때 손에 있던 하네스가 둘이었고
/// (`tests/common` · `tests/webhook_common`) 셋째(`tests/gui_common`)는 그 문장 밖에
/// 남았다. 셋째는 자동 채널에서 한 번도 안 도는 스위트라(전수 `#[ignore]`) 아무도 그
/// 1.1 GB 를 안 봤다. 넷째 하네스가 생기면 같은 일이 다시 나는데, 그것을 사람이 기억으로
/// 막게 두지 않는다.
///
/// **이 가드가 답하지 않는 것**: 판정 단위가 **파일**이다. 한 파일에 부팅 spawn 이 둘인데
/// 그중 하나만 opt-in 을 거치면 통과한다. 자리 단위로 묶으려면 구문 분석이 필요하고, 이
/// 축이 잡으려는 결함(하네스가 통째로 안 거치는 것)에 비해 그 비용이 크다.
#[test]
fn every_booting_harness_goes_through_the_bundle_opt_in() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = root.join("tests");
    let mut booting = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for (rel, src) in all_test_sources(&tests_dir) {
        if !src.contains(BOOT_ARG_MARKER) {
            continue;
        }
        booting += 1;
        if !src.contains(BUNDLE_OPT_IN_MARKER) {
            offenders.push(rel);
        }
    }

    assert!(
        booting >= BOOTING_HARNESS_MIN,
        "부팅 하네스를 {booting} 개밖에 못 찾았다(하한 {BOOTING_HARNESS_MIN}). 스캔이 \
         죽었거나 표지가 바뀐 것이다 — 0 건은 초록이 아니라 미측정이다."
    );
    assert!(
        offenders.is_empty(),
        "부팅하는 하네스가 번들 opt-in 을 안 거친다 — 그 회차는 격리 홈마다 번들 전량을 \
         복사한다(debug 45 파일 약 1.1 GB). `spawn_diag::{BUNDLE_OPT_IN_MARKER}(&mut command)` \
         을 spawn 직전에 불러라:\n\x20 {offenders:?}"
    );
}

/// 아래 셋도 픽스처를 **마커 상수에서 만든다** — 이유는 위 격리 홈 축의 대조들과 같다.
#[test]
fn detects_a_booting_harness_that_skips_the_bundle_opt_in() {
    let src = format!("c.arg({BOOT_ARG_MARKER});\nc.spawn();\n");
    assert!(src.contains(BOOT_ARG_MARKER), "모집단에 들어와야 한다");
    assert!(!src.contains(BUNDLE_OPT_IN_MARKER), "위반자로 잡혀야 한다");
}

#[test]
fn does_not_flag_a_booting_harness_that_calls_it() {
    let src = format!("c.arg({BOOT_ARG_MARKER});\nspawn_diag::{BUNDLE_OPT_IN_MARKER}(&mut c);\n");
    assert!(src.contains(BOOT_ARG_MARKER) && src.contains(BUNDLE_OPT_IN_MARKER));
}

/// 산문이 플래그를 **이름으로만** 언급하는 형태는 모집단이 아니다.
///
/// 실측에서 온 대조다 — 따옴표 없는 표지를 쓰면 `tests/webhook_integration.rs` 의 doc
/// 주석이 들어와 위반자가 된다. 그 파일은 자기 Command 를 안 만든다.
#[test]
fn does_not_flag_prose_that_merely_names_the_port_file_flag() {
    let src = "//! CLI→IPC 매핑은 `--port-file` 로 붙인 실 CLI 바이너리로 검증한다.\n";
    assert!(
        !src.contains(BOOT_ARG_MARKER),
        "백틱 산문은 부팅 spawn 이 아니다"
    );
}

/// 스캔 마커 중 **따옴표를 품지 않은 것**은 소스 원문에 그대로 적히므로, 이 파일이
/// 자기 상수를 하네스 코드로 세게 된다. 실측으로 그렇게 났다 — 처음엔 이 파일이
/// `spawn 4 · 전용 홈 지정 0` 인 위반자로 나왔다. 그래서 그런 마커는 `concat!` 로
/// 쪼갠다. 쪼개면 원문에 연속 형태가 없어 자기 매칭이 사라진다.
///
/// **따옴표를 품은 마커는 다르다** — 소스에서는 `\"` 로 이스케이프되어 런타임 값과
/// 글자가 달라지므로, 쪼개지 않아도 자기 매칭이 원리적으로 안 일어난다. 그쪽의 쪼갬은
/// 형태를 맞춰 둔 것이지 이 결함을 막는 장치가 아니다. **두 부류를 섞어 "쪼개서 안전하다"
/// 로 적으면 따옴표 없는 마커가 새로 붙었을 때 같은 근거로 안 쪼개게 된다.**
///
/// 그리고 **쪼갬은 주석만으로는 안 지켜진다.** 다음 사람이 "왜 나눠 놨지" 하며 합치면
/// 조용히 되돌아가고, 그 순간 이 파일이 자기 상수 때문에 위반자가 된다. 그래서 쪼갬이
/// 살아 있는지를 **시험이 묻는다.** 대안이었던 "이 파일을 순회에서 빼기" 를 안 쓴 이유는
/// 그 면제가 **이 파일에 나중에 붙는 모든 축까지 덮기** 때문이다(726 이 짚었다).
/// 선례: `crates/tasty-doc-guards/tests/complexity_allowlist_docs_parity.rs` 의
/// `the_needle_is_not_written_whole_in_this_file`.
///
/// ★ **소스 텍스트를 읽는 시험에는 변이가 선택이 아니라 필수다.** 이 시험의 첫 판은
/// 런타임 값을 그대로 `src.contains` 했고 **초록이었다.** 그런데 Rust 소스에서 문자열
/// 리터럴은 `\"` 로 이스케이프되어 적히므로 런타임 값의 글자가 원문에 **절대 안 나온다**
/// — `contains` 가 항상 거짓이라 단정이 항상 참이었다. 시험도 대상도 게이트도 전부
/// 초록이고 빨간 곳이 아무 데도 없었다. **쪼갬을 되돌리는 변이를 넣어 그것이 살아남는
/// 것을 보고서야** 알았다. 이 파일을 고칠 때는 변이를 한 번 넣어 빨간지 확인해라.
#[test]
fn the_quoteless_scan_markers_are_not_written_whole_in_this_file() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(SELF_FILE),
    )
    .expect("자기 소스 read 실패");

    // 따옴표가 없으면 런타임 값 == 소스에 적히는 글자다. 그 부류만 자기 매칭을 낸다.
    let quoteless: Vec<(&str, &str)> = [
        ("SPAWN_FORMS[0]", SPAWN_FORMS[0]),
        ("SPAWN_FORMS[1]", SPAWN_FORMS[1]),
        ("BIN_SELECTION_MARKER", BIN_SELECTION_MARKER),
        ("ISOLATED_HOME_MARKER", ISOLATED_HOME_MARKER),
        ("BUNDLE_OPT_IN_MARKER", BUNDLE_OPT_IN_MARKER),
    ]
    .into_iter()
    .filter(|(_, needle)| !needle.contains('"'))
    .collect();

    // ★ 비영 대조 ① — 판정 대상이 0 건이면 아래 단정은 자극과 무관하게 초록이다.
    assert!(
        !quoteless.is_empty(),
        "비영 대조 실패 — 따옴표 없는 마커가 하나도 없다. 그러면 아래 통과는 쪼갬의 \
         증거가 아니라 **아무것도 안 봤다**는 뜻이다."
    );

    let whole: Vec<String> = quoteless
        .iter()
        .filter(|(_, needle)| src.contains(*needle))
        .map(|(label, needle)| format!("{label} = {needle:?}"))
        .collect();
    assert!(
        whole.is_empty(),
        "따옴표 없는 스캔 마커가 원문에 통째로 있다 — `concat!` 쪼갬을 되돌리지 마라. \
         되돌리면 이 가드가 자기 상수를 하네스 spawn 으로 센다. 되돌린 것: {whole:?}"
    );

    // ★ 비영 대조 ② — `contains` 로 이 파일에서 무언가를 실제로 찾을 수 있음을 보인다.
    //   찾는 법 자체가 죽어 있으면 위 초록은 증거가 아니다. 조각은 원문에 반드시 있다.
    assert!(
        src.contains("Command::new(spawn_diag::"),
        "비영 대조 실패 — 조각조차 원문에서 못 찾는다. 탐색법이 죽었다."
    );
}
