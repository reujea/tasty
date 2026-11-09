//! 테스트 진입 파일의 전용 인스턴스 생성 횟수와 하네스 사용 목록을 고정한다.
//! 기본은 바이너리당 인스턴스 하나를 공유하고 워크스페이스로 격리하는 것이다.
//! 바이너리 선택, 전용 홈, 번들 준비도 같은 소스 목록에서 확인한다.
//! 소스 문자열을 세는 검사이며 실제 PID 수나 각 호출의 실행 여부를 확인하지 않는다.

use std::collections::BTreeSet;
use std::path::Path;

const DOC: &str = "docs/dev-guide/e2e-tests.md (원칙 근거: docs/dev-guide/unit-test-isolation.md)";

const SELF_FILE: &str = "e2e_single_instance_guard.rs";

/// 공유 진입점은 제외하고 전용 인스턴스를 요청하는 호출 형태만 센다.
const DEDICATED_SPAWN_MARKERS: &[&str] = &[
    "TastyInstance::spawn(",
    "TastyInstance::spawn_with_inherit_cwd(",
    "WebhookInstance::builder(",
    "WebhookInstance::builder_for_restart(",
];

/// 호출을 import할 수 있어 하네스 모듈 선언으로 사용 파일을 찾는다. 프로토콜 헬퍼인 attach_common은 제외한다.
const INSTANCE_HARNESS_MARKERS: &[&str] =
    &["mod common;", "mod gui_common;", "mod webhook_common;"];

/// 전용 인스턴스가 필요한 파일과 허용 횟수다. 미등록 파일은 허용하지 않는다.
/// soak_memory는 다른 시험의 활동 없이 RSS를 측정한다. attach_convert_cwd는 기동 시 inherit_cwd 설정이 필요하다.
/// 웹훅 하네스는 HOME과 설정을 개별 준비하며, webhook_integration은 같은 HOME으로 재시작도 검사한다.
/// remote_attach_token_caller는 mirror를 받는 쪽과 다른 프로세스의 서버가 필요하다.
const ALLOWLIST_FILES: &[(&str, usize)] = &[
    ("tests/soak_memory.rs", 1),
    ("tests/attach_convert_cwd_loopback.rs", 1),
    ("tests/hook_env_integration.rs", 1),
    ("tests/remote_attach_token_caller.rs", 1),
    ("tests/webhook_integration.rs", 2),
];

/// 각 바이너리가 인스턴스를 하나씩만 띄워도 총수가 늘 수 있어 진입 파일 목록도 고정한다.
const EXPECTED_INSTANCE_TESTS: &[&str] = &[
    // 번들 agent-session 플러그인과 가짜 claude 프로필이 필요해 GUI 스위트와 바이너리를 나눈다.
    "tests/agent_session_gui.rs",
    "tests/attach_attention_loopback.rs",
    "tests/attach_convert_cwd_loopback.rs",
    "tests/attach_git_query_loopback.rs",
    "tests/attach_list_dir_loopback.rs",
    "tests/attach_local_creation_tap.rs",
    // 실제 Markdown 플러그인의 서피스를 검사하므로 이 스위트는 번들 준비가 필요하다.
    "tests/attach_markdown_content_loopback.rs",
    "tests/attach_html_transfer_loopback.rs",
    "tests/attach_silent_disconnect.rs",
    // 서버와 클라이언트 사이의 실제 attach 스트림으로 구조 변경과 anchor 부재 응답을 검사한다.
    "tests/attach_structure_sync_loopback.rs",
    "tests/e2e_tests.rs",
    "tests/gui_tests.rs",
    "tests/hook_env_integration.rs",
    "tests/hooks_detection_e2e.rs",
    // 공유 인스턴스의 워크스페이스로 격리해 알림 재시도 계약을 GUI 라우터와 헤드리스 처리기에서 각각 검사한다.
    "tests/notification_retry.rs",
    // 플러그인 활성 상태는 프로세스 전역이라 다른 Markdown 시험과 같은 바이너리에 넣지 않는다.
    "tests/plugin_disable_withdraws_surface_kinds.rs",
    "tests/remote_attach_token_caller.rs",
    "tests/shared_instance_harness.rs",
    "tests/soak_memory.rs",
    "tests/webhook_integration.rs",
];

/// 직접 바이너리를 선택할 수 있는 파일과 횟수다.
/// gui_common은 GUI 바이너리 자체를, cli_stdout_broken_pipe는 방금 빌드한 CLI 출력을 검사한다.
/// 나머지 인스턴스 하네스는 spawn_diag::instance_bin을 통해 override를 동일하게 적용한다.
const BIN_SELECTION_ALLOWLIST: &[(&str, usize)] = &[
    ("tests/gui_common/mod.rs", 1),
    ("tests/cli_stdout_broken_pipe.rs", 1),
];

const BIN_SELECTION_CHOKEPOINT: (&str, usize) = ("tests/spawn_diag/mod.rs", 1);

/// 선택 함수의 허용 횟수와 예외 명부의 합으로 계산한 하한이다. 측정값은 아니다.
const BIN_SELECTION_MIN_HITS: usize = 3;

/// 호출 형태를 찾는다. 이 파일이 스캔 문자열 자체에 걸리지 않도록 concat!으로 나눈다.
const BIN_SELECTION_MARKER: &str = concat!("env!(\"CARGO_BIN_EXE_", "tasty\")");

fn direct_binary_pick_lines(contents: &str) -> Vec<usize> {
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(BIN_SELECTION_MARKER))
        .map(|(i, _)| i + 1)
        .collect()
}

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

/// 실제 spawn이 있는 하네스 하위 디렉터리까지 재귀 수집한다.
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
        "바이너리 선택을 {hits} 건만 찾았다(명부에서 계산한 하한 {BIN_SELECTION_MIN_HITS}). 수집 경로와 명부의 실제 호출을 확인한다. 호출이 삭제됐다면 하한만 낮추지 말고 명부를 갱신한다."
    );

    assert!(
        violations.is_empty(),
        "하네스의 바이너리는 spawn_diag::instance_bin()에서 선택한다. 직접 선택하면 override를 따르지 않는다. 필요한 예외는 BIN_SELECTION_ALLOWLIST에 사유와 함께 등록한다. 근거: {DOC}\n{}",
        violations.join("\n")
    );
}

#[test]
fn detects_a_file_that_picks_the_binary_itself() {
    let src = "mod common;\nlet c = Command::new(env!(\"CARGO_BIN_EXE_tasty\"));\n";
    assert_eq!(direct_binary_pick_lines(src), vec![2]);
}

#[test]
fn detects_an_extra_pick_inside_an_exempted_file() {
    let src = "let a = env!(\"CARGO_BIN_EXE_tasty\");\nlet b = env!(\"CARGO_BIN_EXE_tasty\");\n";
    let lines = direct_binary_pick_lines(src);
    assert_eq!(lines.len(), 2);
    assert!(lines.len() > direct_pick_allowance("tests/gui_common/mod.rs"));
}

#[test]
fn does_not_flag_prose_that_merely_names_the_variable() {
    let src = "//! 하네스는 CARGO_BIN_EXE_tasty 를 띄우곤 했다.\n/// CARGO_BIN_EXE_tasty\n";
    assert!(direct_binary_pick_lines(src).is_empty());
}

#[test]
fn intentionally_does_not_see_a_runtime_lookup() {
    // 런타임 환경변수로 바이너리를 고르는 형태는 이 검사에서 찾지 못한다.
    let src = "let bin = std::env::var(\"TASTY_E2E_BIN\").unwrap();\n";
    assert!(direct_binary_pick_lines(src).is_empty());
}

/// 테스트 바이너리 진입 파일만 수집한다. 하위 하네스의 생성자 정의는 제외한다.
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
        "전용 인스턴스 호출 수가 허용값을 넘었다. 기본은 common::shared()를 공유하고 create_workspace()로 격리하는 것이다. 프로세스 경계 자체를 검사한다면 ALLOWLIST_FILES에 이유와 함께 등록한다. 원칙: {DOC}\n{}",
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
        "인스턴스 하네스를 사용하는 테스트 파일 목록이 달라졌다. 별도 바이너리가 필요한지 검토하고 EXPECTED_INSTANCE_TESTS를 갱신한다. 원칙: {DOC}\n  추가됨: {added:?}\n  사라짐: {removed:?}"
    );
}

/// 예외 대상 파일의 존재만 확인한다. 예외가 현재 필요한지까지 판단하지는 않는다.
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

/// 별도 분류 목록의 누락·오타는 SameCombo로 처리돼 실행 오류 없이 최적화가 사라질 수 있다.
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

    // 테스트 코드뿐 아니라 검증 대상 서버 경로가 빌드 조합에 따라 달라도 같은 조합의 서버가 필요하다.
    // attach 구조 변경은 GUI와 헤드리스에서 서로 다른 호출 경로를 거친다.
    // notification_retry는 GUI 빌드에서 GUI 라우터를, 헤드리스 빌드에서 헤드리스 처리기를 검사한다.
    // remote_attach_token_caller는 GUI에서 mirror 생성을, 헤드리스에서 미지원 응답을 검사한다.
    // agent_session_gui는 GUI 창의 webview 입력칸과 권한 카드를 실제로 누른다.
    let same_combo: BTreeSet<String> = [
        "e2e_tests",
        "gui_tests",
        "attach_structure_sync_loopback",
        "notification_retry",
        "remote_attach_token_caller",
        "agent_session_gui",
    ]
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
        "서버 빌드 조합 분류와 EXPECTED_INSTANCE_TESTS가 다르다. 검증 대상이 조합에 독립적이면 HEADLESS_OK_SUITES에, 조합별 경로를 검사하면 same_combo에 등록한다. 원칙: {DOC}\n  미분류: {unclassified:?}\n  없는 이름: {stale:?}"
    );
}

const SPAWN_FORMS: &[&str] = &[
    concat!("Command::new(spawn_diag::", "instance_bin())"),
    concat!("Command::new(env!(\"CARGO_BIN_EXE_", "tasty\"))"),
];

const ISOLATED_HOME_MARKER: &str = concat!(".env(\"TASTY_", "HOME\"");

/// 2026-09-07 측정 5곳: gui_common 1, common 1, webhook_common 2, CLI 파이프 시험 1.
/// 여유 없이 고정했으므로 실제 호출 삭제와 수집 누락을 구별한 뒤 갱신한다.
const ISOLATED_HOME_MIN_SPAWNS: usize = 5;

fn spawn_and_home_counts(src: &str) -> (usize, usize) {
    let spawns = SPAWN_FORMS.iter().map(|f| src.matches(f).count()).sum();
    (spawns, src.matches(ISOLATED_HOME_MARKER).count())
}

/// 사용자 설정·레이아웃을 읽거나 번들을 설치하지 않도록 테스트 전용 홈을 요구한다.
/// 파일별 spawn 수와 홈 지정 수만 비교하며, 각 spawn에 올바른 홈이 연결됐는지는 확인하지 않는다.
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
        "spawn을 {total_spawns} 개만 찾았다(하한 {ISOLATED_HOME_MIN_SPAWNS}). 수집 경로와 호출 형태를 확인한다."
    );
    assert!(
        offenders.is_empty(),
        "하네스가 전용 tasty 루트 없이 인스턴스를 띄운다 — 그 회차는 사용자의 진짜 홈에 \
         설치하고 거기 저장된 레이아웃을 복원한다:\n\x20 {offenders:?}"
    );
}

/// 합성 입력을 마커 상수로 만든다. 원문에 호출 형태를 그대로 쓰면 검사 자신을 하네스로 세어 하한까지 잘못 채울 수 있다.
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

/// 포트 파일 인자를 부팅 표지로 쓴다. 부팅하지 않는 로컬 CLI 명령은 제외한다.
/// 따옴표까지 포함해 인자 이름만 설명하는 산문이 표지로 수집되지 않게 한다.
const BOOT_ARG_MARKER: &str = concat!("\"--port-", "file\"");

const BUNDLE_OPT_IN_MARKER: &str = concat!("apply_bundle_", "opt_in");

/// common, webhook_common, gui_common 세 하네스를 기준으로 수집 누락을 확인한다.
const BOOTING_HARNESS_MIN: usize = 3;

/// 번들 준비를 생략하면 불필요한 번들까지 격리 홈에 복사할 수 있어 표지를 확인한다.
/// 파일에 표지가 하나라도 있으면 통과한다. 각 spawn이 준비 함수를 호출하는지는 확인하지 않는다.

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
        "부팅 표지가 있는 파일을 {booting} 개만 찾았다(하한 {BOOTING_HARNESS_MIN}). 수집 경로와 표지를 확인한다."
    );
    assert!(
        offenders.is_empty(),
        "부팅 표지가 있지만 번들 준비 표지가 없는 하네스다. spawn 전에 spawn_diag::{BUNDLE_OPT_IN_MARKER}(&mut command)를 적용한다:\n  {offenders:?}"
    );
}

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

#[test]
fn does_not_flag_prose_that_merely_names_the_port_file_flag() {
    let src = "//! CLI→IPC 매핑은 `--port-file` 로 붙인 실 CLI 바이너리로 검증한다.\n";
    assert!(
        !src.contains(BOOT_ARG_MARKER),
        "백틱 산문은 부팅 spawn 이 아니다"
    );
}

/// 따옴표 없는 마커는 원문과 런타임 값이 같아 자기 파일을 오탐할 수 있다. concat! 분할을 유지한다.
/// 따옴표가 있는 마커는 소스의 이스케이프 때문에 같은 방식으로 자기 매칭되지 않는다.
/// 실제 탐색이 동작하는지 조각 검색도 확인한다.
#[test]
fn the_quoteless_scan_markers_are_not_written_whole_in_this_file() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(SELF_FILE),
    )
    .expect("자기 소스 read 실패");

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

    assert!(
        !quoteless.is_empty(),
        "따옴표 없는 마커가 없어 concat! 분할 여부를 검사할 수 없다"
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

    assert!(
        src.contains("Command::new(spawn_diag::"),
        "원문에서 마커 조각도 찾지 못했다. 검색 대상과 탐색 방법을 확인한다."
    );
}
