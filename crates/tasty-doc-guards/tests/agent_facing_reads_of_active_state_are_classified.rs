//! 에이전트가 호출하는 경로의 활성 상태 읽기를 파일·분류·횟수·사유로 기록한다.
//! 대상을 ID로 지정해야 한다는 docs/design/policies/focus.md의 CLI/IPC 포커스 독립 규칙을 따른다.
//! 같은 식별자만으로 상태 보고와 대상 선택을 구별할 수 없어, 분류의 타당성은 사람이 판단한다.
//! 이 검사는 미등록·개수 차이·사유 누락·OpenDefect 등록을 찾는다.
//! 주석·문자열·테스트 전용 코드는 제외한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tasty_doc_guards::cfg_predicate::blank_gated_lines;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

/// 활성 상태 식별자. active_tab은 ID로 찾은 페인의 필드일 수도 있으므로
/// 제외하지 않고 사람이 명시 ID로 해소한 값인지 분류한다.
/// focused_pair는 포커스 창과 그 engine을 함께 돌려주는 focused_window의 짝이다.
const NEEDLES: &[&str] = &[
    "active_workspace",
    "focused_window",
    "focused_pair",
    "active_surface",
    "active_tab",
    "active_pane",
    "presentation()",
    ".navigation",
    ".selected_tabs",
    ".focused_panes",
    "focused_surface_id",
    "focused_pane_id",
];

/// IPC handler, 현재 journal resolver/completion과 RequestScope adapter를 검사한다.
/// 순수 domain은 선택을 소유하지 않으며 이 명부는 host 진입·소비 경계를 다룬다(docs/design/policies/focus.md).
/// src/state와 src/file에는 사용자 입력 경로도 있으므로 필요한 파일만 등록한다.
/// 새 진입·completion adapter가 추가되면 이 목록도 갱신한다. 임의 호출 그래프를 자동 추적하지 않는다.
const AGENT_FACING: &[&str] = &[
    "src/adapters/ipc/handler.rs",
    "src/adapters/ipc/handler/",
    "src/app/dispatch/",
    "src/app/dispatch_domain.rs",
    "src/app/dispatch_domain/",
    "src/file/identify_worker.rs",
    "src/adapters/ipc/request_scope.rs",
    "src/app/journal/commands.rs",
    "src/app/journal/commands/",
    "src/app/journal/forward.rs",
    "src/app/creation_intent.rs",
    "src/app/services/live_intent.rs",
];

/// 파일 단위 항목은 부모 디렉터리를 순회하고 is_agent_facing에서 거른다.
const SCAN_ROOTS: &[&str] = &[
    "src/adapters/ipc",
    "src/app",
    "src/core",
    "src/file",
    "src/state",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 응답에 "무엇이 활성인지" 를 싣는다. 대상 선택이 아니라 상태 보고다.
    Report,
    /// 저장·undo·원격 publication의 명시 선택 snapshot. 명령 대상을 고르지 않는다.
    Projection,
    /// 명시 ID로 해소한 pane/workspace 안의 선택값이다.
    IdResolved,
    /// 기존 target 생략 호환 규칙에만 쓰는 명령 기본 문맥이다.
    CompatibilityDefault,
    /// 권한·상한 정책을 적용할 워크스페이스다. 요청 대상을 고르는 값은 아니다.
    PolicyScope,
    /// 기록·알림·승인의 기본 워크스페이스. 호출자가 생략하면 활성 워크스페이스를 쓴다.
    Attribution,
    /// debug 전용 파일로 release 빌드에서 제외된다.
    DebugOnly,
    /// **사용자 기원** 경로가 활성 포인터를 갱신한다. 에이전트 행동이 아니다.
    UserOrigin,
    /// 워크스페이스가 0 이 된 뒤의 복구 — 활성 포인터를 **다시 만든다**.
    Recovery,
    /// navigation_proofs는 문서 권한 증거이며 사용자 선택 값이 아니다.
    NavigationProof,
    /// 활성 상태로 대상을 고르는 결함. 해결을 막는 이유를 적는다.
    OpenDefect,
}
use Kind::*;

/// (파일, 분류, 해당 분류의 출현 수, 사유).
/// 줄 번호는 무관한 편집에도 바뀌므로 파일별로 집계하고 변경 시 분류를 다시 검토한다.
const ROSTER: &[(&str, Kind, usize, &str)] = &[
    (
        "src/app/journal/commands/create_spec.rs",
        IdResolved,
        1,
        "호출자가 지정한 pane 안의 selected tab을 원 CWD 상속 입력으로 고정하며 전역 workspace 선택으로 바꾸지 않는다",
    ),
    (
        "src/app/journal/commands/assembly.rs",
        IdResolved,
        1,
        "명시 target_workspace_id 안의 focused pane을 확인하고 같은 workspace의 첫 pane으로만 대체한다",
    ),
    (
        "src/app/journal/commands/view_completion.rs",
        CompatibilityDefault,
        1,
        "preset.apply의 target pane 생략 호환값을 원 engine 문맥에서 고정한다. 명시 target의 owner 해소를 대체하지 않는다",
    ),
    (
        "src/app/journal/commands/headless.rs",
        CompatibilityDefault,
        1,
        "headless preset의 target pane 생략 호환값이며 로컬 GUI View를 새로 만들어 선택하지 않는다",
    ),
    (
        "src/app/creation_intent.rs",
        UserOrigin,
        1,
        "origin.is_user() 안에서 선택된 attach 임시 영역의 불가능한 조작만 거절한다. 에이전트 대상을 현재 선택으로 바꾸지 않는다",
    ),
    (
        "src/app/creation_intent.rs",
        CompatibilityDefault,
        5,
        "기존 target 없는 View intent의 preset 또는 생성 입력을 현재 명령 문맥의 ID로 고정한다. 외부 구조 IPC는 별도 journal resolver를 거친다",
    ),
    (
        "src/adapters/ipc/handler.rs",
        NavigationProof,
        2,
        "원 HTML 문서 권한 증거 handle을 RequestScope와 file_picker.trigger로 전달하며 navigation 선택을 읽거나 바꾸지 않는다",
    ),
    (
        "src/adapters/ipc/request_scope.rs",
        Projection,
        1,
        "원 RequestContext navigation을 읽기 전용 presentation으로 빌려 응답과 capture가 소비한다",
    ),
    (
        "src/adapters/ipc/handler/file_picker.rs",
        CompatibilityDefault,
        1,
        "file_picker.trigger가 origin_surface_id를 생략하거나 찾지 못할 때 둘러볼 위치의 workspace를 고르는 기존 호환값이다. plugin 호출자 전용이며 명시 surface가 있으면 그 소속을 쓴다",
    ),
    (
        "src/adapters/ipc/request_scope.rs",
        CompatibilityDefault,
        3,
        "호환 workspace 및 surface snapshot과 index accessor 선언이다. 실제 대상 선택은 각 method의 기존 기본값 규칙을 따른다",
    ),
    (
        "src/adapters/ipc/request_scope.rs",
        NavigationProof,
        1,
        "원 View의 문서 권한 증거를 소비하는 필드명이며 구조 navigation 선택과 무관하다",
    ),
    (
        "src/app/dispatch/lua_commands.rs",
        Report,
        2,
        "모든 engine의 현재 navigation과 active workspace를 Lua 읽기 snapshot tree에 보고한다",
    ),
    (
        "src/app/dispatch/plugin_ipc.rs",
        Projection,
        1,
        "명시 source ID의 owner engine을 먼저 찾고 preset capture에 원 View presentation snapshot을 고정한다",
    ),
    (
        "src/app/journal/commands/headless.rs",
        Recovery,
        1,
        "headless의 확정 replacement 결과를 명령 기본 문맥에 적용하며 GUI 사용자 선택을 생성하지 않는다",
    ),
    (
        "src/app/journal/commands/headless.rs",
        Projection,
        1,
        "확정 host notification을 현재 명령 문맥으로 해소하며 명령 실행 대상을 새로 고르지 않는다",
    ),
    (
        "src/app/journal/commands/view_completion.rs",
        Projection,
        3,
        "window와 parked의 completion snapshot 두 곳 및 확정 변경 후 attach presentation 갱신 한 곳이다",
    ),
    (
        "src/app/journal/commands/view_completion.rs",
        UserOrigin,
        10,
        "성공 user origin과 원 View identity 및 selection generation 확인 뒤에만 저장된 선택과 생성 대상을 적용한다",
    ),
    (
        "src/app/journal/commands/view_completion.rs",
        Recovery,
        1,
        "확정 replacement의 원 ID 대응을 현재 navigation에 적용하여 사라진 구조 참조를 보정한다",
    ),
    (
        "src/adapters/ipc/handler.rs",
        Projection,
        3,
        "요청 전후 attach 전송 문맥을 갱신하고 preset capture에 현재 View의 표시값을 전달한다",
    ),
    (
        "src/app/dispatch/list_global.rs",
        Report,
        4,
        "각 engine의 View를 함께 해소하여 pane과 category 전역 조회 결과를 합성한다",
    ),
    (
        "src/adapters/ipc/handler/workspace.rs",
        Report,
        1,
        "워크스페이스 목록에서 어느 것이 활성인지를 플래그로 알린다",
    ),
    (
        "src/adapters/ipc/handler/gate.rs",
        PolicyScope,
        2,
        "판정 단계(permission·cap)와 기록 결과 반영(throttle·Allow audit)에 넘길 workspace_id — 요청 대상을 고르는 값이 아니라 정책·감사를 귀속시킬 스코프다",
    ),
    (
        "src/adapters/ipc/handler.rs",
        Report,
        11,
        "system_info와 workspace/category/pane/tab/tree 응답에 해당 View의 선택 상태를 보고한다",
    ),
    (
        "src/adapters/ipc/handler.rs",
        PolicyScope,
        1,
        "대상을 생략한 preset.apply가 적용될 기본 workspace에 hard 점유 거부를 적용한다 — 대상은 preset 적용 코드가 이미 정한 값이고 이 읽기는 거부 여부만 정한다",
    ),
    (
        "src/adapters/ipc/handler/approval.rs",
        Attribution,
        1,
        "승인 요청을 어느 워크스페이스 것으로 기록할지 — 승인 대상은 이미 파라미터로 정해져 있다",
    ),
    (
        "src/adapters/ipc/handler/approval/request.rs",
        Attribution,
        1,
        "승인 요청에 workspace_id와 surface_id가 모두 없을 때만 활성 워크스페이스를 쓴다. surface_id가 있으면 해당 서피스의 워크스페이스를 쓴다(docs/design/policies/focus.md).",
    ),
    (
        "src/adapters/ipc/handler/telemetry/record.rs",
        Attribution,
        2,
        "호출자가 ws 를 안 주면 채울 기본값 — 이벤트를 어디 것으로 셀지의 문제다",
    ),
    (
        "src/adapters/ipc/handler/telemetry/anomaly.rs",
        Attribution,
        1,
        "이상 판정이 볼 워크스페이스 스코프. 대상 선택이 아니라 표본의 귀속이다",
    ),
    (
        "src/adapters/ipc/handler/telemetry/cap.rs",
        Attribution,
        2,
        "상한 계산에서 사용량을 집계할 워크스페이스를 정한다. 요청의 실행 대상은 바꾸지 않는다.",
    ),
    (
        "src/adapters/ipc/handler/debug_state.rs",
        DebugOnly,
        8,
        "debug 상태 덤프 전용 파일. attach 임시 선택/입력 대상 관측 두 자리도 읽기 전용이며 release 에 없다",
    ),
    (
        "src/adapters/ipc/handler/debug.rs",
        DebugOnly,
        2,
        "debug 전용 핸들러 파일이므로 release 빌드의 검사 대상에서 제외된다.",
    ),
    (
        "src/app/dispatch_domain.rs",
        Projection,
        1,
        "지연 완료 continuation의 원 selection generation을 고정한다. 실제 선택 변경은 user origin과 원 View를 다시 검사한다",
    ),
];

/// 수집 결과가 비어 명부 대조만 통과하는 경우를 막는다.
const MIN_FILES_SCANNED: usize = 60;
/// 같은 이유의 출현 하한. 2026-09-07 실측 33.
const MIN_OCCURRENCES: usize = 20;

fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

/// 공유 파서로 주석·문자열과 test 조건부 구간을 제외한다.
fn shipped_code(src: &str) -> String {
    blank_gated_lines(&mask_non_code(src), "test")
}

fn is_agent_facing(rel: &str) -> bool {
    AGENT_FACING
        .iter()
        .any(|p| rel == *p || (p.ends_with('/') && rel.starts_with(p)))
}

fn count_needles(text: &str) -> usize {
    NEEDLES
        .iter()
        .map(|n| text.matches(n).count())
        .sum::<usize>()
}

/// (스캔한 파일 수, 파일별 제품 코드의 출현 수).
/// 파일 내부 test 구간과 test 모듈로만 선언된 파일을 모두 제외한다.
fn measure() -> (usize, BTreeMap<String, usize>) {
    let root = repo_root();
    let sources = rust_sources(&root, SCAN_ROOTS);
    let scanned = sources.len();
    let test_only = test_only_files(&root, &sources);
    let mut found = BTreeMap::new();
    for (rel, text) in &sources {
        if test_only.contains(rel) {
            continue;
        }
        let rel = rel.to_string_lossy().into_owned();
        if !is_agent_facing(&rel) {
            continue;
        }
        let n = count_needles(&shipped_code(text));
        if n > 0 {
            found.insert(rel, n);
        }
    }
    (scanned, found)
}

#[test]
fn the_population_did_not_collapse() {
    let (scanned, found) = measure();
    assert!(
        scanned >= MIN_FILES_SCANNED,
        "스캔 루트 {SCAN_ROOTS:?}에서 .rs 파일을 {scanned}개만 수집했다. 하한을 낮추기 전에 경로와 수집 범위를 확인한다."
    );
    let total: usize = found.values().sum();
    assert!(
        total >= MIN_OCCURRENCES,
        "활성 상태 읽기를 {total}개만 찾았다(2026-09-07 실측 33). 마스킹과 cfg 제외가 과도한지 먼저 확인한다. 실제로 줄었다면 명부와 하한을 함께 갱신하고 측정 근거를 남긴다."
    );
}

#[test]
fn every_occurrence_is_registered() {
    let (_, found) = measure();
    let mut registered: BTreeMap<&str, usize> = BTreeMap::new();
    for (path, _, count, _) in ROSTER {
        *registered.entry(path).or_default() += count;
    }
    let mut bad = Vec::new();
    for (path, n) in &found {
        let r = registered.get(path.as_str()).copied().unwrap_or(0);
        if r != *n {
            bad.push(format!("  {path}: 실측 {n} · 명부 {r}"));
        }
    }
    assert!(
        bad.is_empty(),
        "에이전트 경로의 활성 상태 읽기가 명부와 다르다.\n{}\n새 읽기는 분류와 사유를 검토해 ROSTER에 등록한다. 활성 상태로 대상을 고르면 OpenDefect로 기록한다.",
        bad.join("\n")
    );
}

/// 출현 수가 0인 경로도 명부에서 빠지지 않도록 AGENT_FACING 항목의 존재를 확인한다.
#[test]
fn every_agent_facing_entry_exists() {
    let root = repo_root();
    let scanned: Vec<String> = rust_sources(&root, SCAN_ROOTS)
        .into_iter()
        .map(|(rel, _)| rel.to_string_lossy().replace('\\', "/"))
        .collect();
    let missing: Vec<&&str> = AGENT_FACING
        .iter()
        .filter(|p| {
            !scanned
                .iter()
                .any(|r| r == **p || (p.ends_with('/') && r.starts_with(**p)))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "AGENT_FACING 항목이 스캔 루트 {SCAN_ROOTS:?}에 없다. 파일이 이동했다면 항목을 삭제하지 말고 새 경로로 고친다.\n  {missing:?}"
    );
}

#[test]
fn the_roster_has_no_file_the_tree_does_not_have() {
    let (_, found) = measure();
    let listed: BTreeSet<&str> = ROSTER.iter().map(|(p, _, _, _)| *p).collect();
    let stale: Vec<&&str> = listed.iter().filter(|p| !found.contains_key(**p)).collect();
    assert!(
        stale.is_empty(),
        "명부에 등록됐지만 해당 읽기가 없는 파일이다. 코드 변경을 확인하고 오래된 항목을 제거한다:\n  {stale:?}"
    );
}

#[test]
fn the_open_ones_are_not_silently_emptied() {
    let open: usize = ROSTER
        .iter()
        .filter(|(_, k, _, _)| *k == OpenDefect)
        .map(|(_, _, c, _)| c)
        .sum();
    assert_eq!(
        open, 0,
        "활성 상태로 대상을 고르는 OpenDefect가 등록됐다. docs/design/policies/focus.md의 포커스 독립 규칙에 따라 대상을 ID로 지정하도록 수정해야 한다."
    );
}

#[test]
fn rows_in_the_same_file_do_not_share_evidence() {
    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (path, _, _, why) in ROSTER {
        *seen.entry((path, why)).or_default() += 1;
    }
    let dupes: Vec<String> = seen
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|((p, w), n)| format!("  {p} ×{n}: {w}"))
        .collect();
    assert!(
        dupes.is_empty(),
        "같은 파일의 서로 다른 분류에 동일한 사유가 쓰였다. 각 분류의 근거를 따로 설명한다:\n{}",
        dupes.join("\n")
    );
}

#[test]
fn every_row_carries_a_reason() {
    let thin: Vec<String> = ROSTER
        .iter()
        .filter(|(_, _, _, why)| why.split_whitespace().count() < 6)
        .map(|(p, k, _, w)| format!("  {p} {k:?}: {w}"))
        .collect();
    assert!(
        thin.is_empty(),
        "분류 사유가 너무 짧다. 코드에서 판단한 근거를 설명한다:\n{}",
        thin.join("\n")
    );
}

/// 제품 코드만 세는지 합성 입력의 원문과 마스킹 결과를 비교한다.
#[test]
fn the_extractor_counts_shipped_code_only() {
    let fixture = concat!(
        "fn shipped() { let a = state.active_workspace; }\n",
        "// active_workspace in a comment\n",
        "fn s2() { let m = \"active_workspace in a string\"; }\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    fn helper() { let b = state.active_workspace; }\n",
        "}\n",
    );
    let n = count_needles(&shipped_code(fixture));
    assert_eq!(
        n, 1,
        "제품 코드의 한 건만 세야 한다 — 주석·문자열·cfg(test) 안의 셋은 빼고. 실제 {n}"
    );
    // 원문에는 주석·문자열·테스트를 포함해 네 번 나타난다.
    assert_eq!(
        count_needles(fixture),
        4,
        "합성 원문에서 예상한 출현 수가 달라져 마스킹 결과와 비교할 수 없다"
    );
}

#[test]
fn view_and_projection_accesses_remain_visible_after_model_field_removal() {
    let input = "fn shipped() { state.navigation.tab_id(pane); state.presentation().surface_id(tab); view.selected_tabs.get(&pane); view.focused_panes.get(&workspace); state.focused_pane_id(engine); state.focused_surface_id(engine); }";
    assert_eq!(count_needles(&shipped_code(input)), 6);
}
