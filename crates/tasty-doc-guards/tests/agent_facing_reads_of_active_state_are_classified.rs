//! 에이전트 대면 경로가 **전역 활성/포커스 포인터**를 읽는 자리를 전수 분류한다.
//!
//! 지키는 것은 `docs/identity.md` §2.3(포커스 독립성)과
//! [`docs/design/policies/focus.md`](../../../docs/design/policies/focus.md) 이다 —
//! "모든 명령은 대상을 ID 로 직접 지정, 활성 상태 의존 동작 금지".
//!
//! ## 이 가드가 **안 묻는 것** — 그리고 왜 그래도 값이 있는가
//!
//! 물어야 할 진짜 물음은 "이 읽기는 **보고**인가 **선택**인가" 다. 그런데 둘이 **같은
//! 식별자**를 쓴다 — `"active": i == state.active_workspace` 는 합법이고
//! `engine.workspaces[state.active_workspace]` 로 대상을 고르는 것은 위반인데, 텍스트로는
//! 안 갈린다. 그래서 이 축은 오래 `[구두]` 로 남아 있었다.
//!
//! **그 판단은 물음을 안 가른 것이었다.** 규칙 옆에 **배치 물음**이 붙어 있다:
//!
//! - 의미 물음 — "보고인가 선택인가". 사람이 판정한다. 여기서 **안 묻는다**.
//! - 배치 물음 — "그 자리가 **갈래·사유와 함께 명부에 적혀 있는가**". 판정된다.
//!
//! 같은 갈림이 `debug_handlers_live_in_cfg_declared_modules` 에서 먼저 풀렸고, 같은
//! 명부 형태가 `window_owned_lists_are_classified` 에서 이미 두 번째로 선다.
//!
//! ## 무엇이 잡히는가
//!
//! 새 읽기가 에이전트 대면 경로에 들어오면 명부에 없어서 빨갛다. 그때 사람이 갈래를
//! 골라야 하고, 고르는 순간 **그 자리가 무엇인지 적힌다.** 지금 `OpenDefect` 가 0 이라는
//! 사실도 이 명부가 있어야 다음 사람이 믿을 수 있다 — 0 은 안 세면 언제나 참이다.
//!
//! ## 모수 — 세 단계로 좁혔고 각 단계의 값을 적는다
//!
//! 2026-09-07 실측, **출현 단위**(줄이 아니다 — `let active_tab = … p.active_tab` 처럼 한
//! 줄에 둘인 자리가 있다). 저장소 전체로 세면 **922 출현**이고 그 수가 "값싸게는 못
//! 만든다" 의 근거였다. 그런데 이 원칙이 말하는 것은 **에이전트가 부르는 경로**다:
//!
//! | 모수 | 출현 |
//! |---|---|
//! | 저장소 전체 | 922 |
//! | 에이전트 대면 경로 원문 | 55 |
//! | + 주석·문자열 마스킹 | 41 |
//! | + `#[cfg(test)]` 제거 | **33** |
//!
//! 마지막 단계가 특히 크다 — `handler/webview.rs` 의 셋과 `handler/workspace.rs` 의 둘은
//! **테스트 헬퍼**인데 눈으로는 위반처럼 보인다(`expect` 를 쓰고 id 해석이 없다). 마스킹만
//! 걸고 읽으면 없는 결함을 셋 본다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tasty_doc_guards::cfg_predicate::blank_gated_lines;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

/// 전역 활성/포커스 포인터의 이름.
///
/// `active_tab` 은 페인의 **필드**이기도 하다 — 그래서 명부에 `IdResolved` 갈래가 있다.
/// 바늘에서 빼면 "ID 로 푼 페인의 활성 탭" 과 "전역 활성 탭" 을 **바늘 단계에서** 가르는
/// 셈이 되는데, 그 구분이야말로 사람이 판정할 것이라 여기서 미리 갈라 두면 안 된다.
const NEEDLES: &[&str] = &[
    "active_workspace",
    "focused_window",
    "active_surface",
    "active_tab",
    "active_pane",
];

/// 에이전트 대면 경로. IPC 핸들러와 그것이 부르는 도메인 cascade.
///
/// 구조 변경 실행(`src/core/structural_exec.rs`)과 그 cascade(split / tab / close,
/// `src/core/structural_cascade.rs`)는 core 에 있다 — IPC 핸들러와 원격 forward 실행이
/// 부르므로 에이전트 대면이다.
///
/// **도메인이 선언하고 창 쪽이 구현하는 포트의 구현 파일**도 같은 경로다
/// (`src/state/cascade_window.rs` — `CascadeWindow` · `src/file/identify_worker.rs` —
/// `IdentifySpawner`). 도메인은 포트 메서드를 부를 뿐이고, 그 메서드가 전역 활성 포인터로
/// 대상을 고르는지는 구현 파일에서만 보인다. 호출 자리의 메서드 이름이 바늘을 담는 것
/// (`set_active_workspace`)은 우연이다 — 이름이 바늘을 안 담는 포트 메서드면 cascade 쪽은
/// 초록이고 구현만 활성 포인터를 읽는다([ADR-0490](../../../docs/adr/0490-boundary-guards-close-three-holes-found-by-mutation.md)).
/// adapters 가 선언한 포트의 구현도 같다(`src/state/ipc_window.rs` — IPC 엔진 핸들러의
/// `IpcWindow`, [ADR-0471](../../../docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md)).
/// `src/state` · `src/file` 전체를 올리지 않는 이유: 사용자 입력 경로가 섞여 명부가 사람 판정
/// 없이 부풀고, 이 가드의 물음("에이전트가 부르는 경로인가")이 흐려진다. 포트 구현 파일이
/// 새로 생기면 여기 한 줄을 더한다.
const AGENT_FACING: &[&str] = &[
    "src/adapters/ipc/handler.rs",
    "src/adapters/ipc/handler/",
    "src/app/dispatch/",
    "src/app/dispatch_domain.rs",
    "src/core/structural_cascade.rs",
    "src/core/structural_exec.rs",
    "src/file/identify_worker.rs",
    "src/state/cascade_window.rs",
    "src/state/ipc_window.rs",
];

/// 스캔 루트 — 위 접두사를 담는 가장 작은 디렉토리들. 파일 단위 항목(`src/state/…` ·
/// `src/file/…`)은 순회가 디렉토리만 받으므로 그 부모를 걷고 [`is_agent_facing`] 이 거른다.
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
    /// **ID 로 해석한 객체의 자기 속성.** `find_pane_by_id(id) … .active_tab` 은 전역
    /// 포커스를 안 읽는다 — 호출자가 준 id 로 푼 페인이 자기 활성 탭을 아는 것뿐이다.
    IdResolved,
    /// 권한·상한 게이트가 정책을 **귀속**시킬 워크스페이스. 대상 선택이 아니다.
    PolicyScope,
    /// 기록·알림·승인의 **기본 워크스페이스 귀속**. 호출자가 안 주면 활성으로 채운다.
    Attribution,
    /// debug 격리 파일. 원칙 1 로 이 축의 범위 밖이다.
    DebugOnly,
    /// **사용자 기원** 경로가 활성 포인터를 갱신한다. 에이전트 행동이 아니다.
    UserOrigin,
    /// 워크스페이스가 0 이 된 뒤의 복구 — 활성 포인터를 **다시 만든다**.
    Recovery,
    /// 구조분해에서 `_` 로 버린다. 읽기가 아니다.
    PatternOnly,
    /// focused 가 있으면 쓰고 없으면 아무 창 — **결과가 focus 에 안 걸린다**고 소스가 밝힌다.
    AnyWindow,
    /// **아직 활성 상태로 대상을 고르는 자리 — 열린 결함.** 사유 칸에 무엇이 막고 있는지 적는다.
    OpenDefect,
}
use Kind::*;

/// (파일, 갈래, 그 파일에서 그 갈래인 출현 수, 사유).
///
/// 좌표를 **줄 번호로 안 잡는다** — 줄은 무관한 편집에도 밀리고, 밀린 명부를 맞추는
/// 가장 싼 길은 수를 다시 세는 것이라 판정을 안 거친다. 파일+갈래+수 는 그 셋이 함께
/// 움직일 때만 갱신되고, 그때는 사람이 무엇이 변했는지 봐야 한다.
const ROSTER: &[(&str, Kind, usize, &str)] = &[
    (
        "src/adapters/ipc/handler/tab.rs",
        Report,
        3,
        "탭 목록의 \"active\" 플래그와 TabCreated 이벤트가 실어 온 active_tab 을 응답에 그대로 싣는다",
    ),
    (
        "src/core/structural_exec.rs",
        IdResolved,
        2,
        "탭 생성과 split 의 cwd 상속 원본을 고를 때 호출자가 준 pane_id(split 은 resolved_pane_id)로 푼 페인의 활성 탭을 본다 — 전역 포커스가 아니다",
    ),
    (
        "src/core/structural_exec.rs",
        Report,
        3,
        "TabCreated 이벤트가 실어 온 active_tab 을 결과 값(TabCreated)에 옮겨 담는다 — 핸들러가 그것을 응답에 싣는다",
    ),
    (
        "src/adapters/ipc/handler/workspace.rs",
        Report,
        1,
        "워크스페이스 목록에서 어느 것이 활성인지를 플래그로 알린다",
    ),
    (
        "src/adapters/ipc/handler/checked.rs",
        PolicyScope,
        1,
        "permission·cap 게이트에 넘길 workspace_id — 요청 대상을 고르는 값이 아니라 정책을 귀속시킬 스코프다",
    ),
    (
        "src/adapters/ipc/handler.rs",
        Report,
        5,
        "system_info의 활성 index 선언·응답·ID 해소와 워크스페이스 표의 활성 플래그",
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
        "승인 요청 생성 쪽의 같은 귀속 — 명시 workspace_id 도 surface_id 도 없을 때만 읽는다(surface 를 댔으면 그 워크스페이스, ADR-0533). approval.rs 와 파일이 달라 사유를 따로 적는다",
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
        "상한 판정의 귀속 워크스페이스 — 상한을 **어느 창에 물릴지**가 아니라 어느 창 몫으로 셀지다",
    ),
    (
        "src/adapters/ipc/handler/debug_state.rs",
        DebugOnly,
        5,
        "debug 상태 덤프 전용 파일. 원칙 1 로 release 에 없다",
    ),
    (
        "src/adapters/ipc/handler/debug.rs",
        DebugOnly,
        1,
        "debug 핸들러 파일. 위와 같은 이유로 범위 밖이고 파일이 달라 따로 적는다",
    ),
    (
        "src/app/dispatch_domain.rs",
        PatternOnly,
        1,
        "TabCreated 를 구조분해하며 active_tab 을 `_` 로 버린다 — 값을 읽지 않는다",
    ),
    (
        "src/app/dispatch_domain.rs",
        Attribution,
        2,
        "알림을 밀어 넣을 때 어느 워크스페이스 알림인지를 채운다",
    ),
    (
        "src/app/dispatch_domain.rs",
        UserOrigin,
        3,
        "닫은 항목 복원·워크스페이스 이동 — 전부 origin 이 User 일 때만 도는 cascade 다",
    ),
    (
        "src/core/structural_cascade.rs",
        UserOrigin,
        1,
        "surface split 뒤 포커스 서피스 갱신 — origin 이 User 일 때만 돈다(에이전트 split 은 앞에서 return)",
    ),
    (
        "src/core/structural_cascade.rs",
        Recovery,
        1,
        "마지막 워크스페이스가 닫힌 뒤 기본 워크스페이스를 다시 만들고 그 인덱스를 활성으로 둔다 — 활성이 없는 상태를 안 남기는 것",
    ),
    (
        "src/state/cascade_window.rs",
        Recovery,
        2,
        "포트 메서드 set_active_workspace 의 구현 이름과 그 몸체의 대입 — 도메인의 호출은 structural_cascade 의 Recovery 한 자리뿐이다(마지막 워크스페이스가 닫힌 뒤 되만든 기본 워크스페이스를 활성으로 둔다)",
    ),
    (
        "src/state/ipc_window.rs",
        Attribution,
        2,
        "포트 메서드 active_workspace_index 의 구현 이름과 그 몸체의 읽기 — 핸들러 호출 자리의 분류를 물려받는다. 호출 자리 11 곳은 Attribution 7 · Report 3 · PolicyScope 1 로 명부에 있고 대상을 고르는 자리는 0 이다; 한 구현이 여러 갈래로 나뉠 수 없어 가장 많은 Attribution 에 둔다",
    ),
    (
        "src/app/dispatch/intents.rs",
        AnyWindow,
        1,
        "appearance 의 단일 출처를 고른다. focused 가 없으면 아무 main 이든 된다고 소스 주석이 밝히므로 결과가 포커스에 안 걸린다",
    ),
];

/// 이 모수에서 파일이 이 수 아래로 떨어지면 걷기가 깨진 것이다.
///
/// 하한이 아니라 **모수 붕괴 탐지**다 — 명부 대조는 양쪽이 비면 공짜로 성립한다.
const MIN_FILES_SCANNED: usize = 60;
/// 같은 이유의 출현 하한. 2026-09-07 실측 33.
const MIN_OCCURRENCES: usize = 20;

fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/tasty-doc-guards → 레포 루트.
    p.pop();
    p.pop();
    p
}

/// 출하되는 코드만 남긴 사본. 주석·문자열·`#[cfg(test)]`·`cfg_attr(test, …)` 를 뺀다.
///
/// **두 판정기를 이어 붙일 뿐 여기서 다시 세지 않는다.** 한때 이 자리가
/// [`blank_gated_lines`] 와 **글자 그대로 같은 루프**를 들고 있었다(앞에 마스킹만 더
/// 붙은 사본이었다). 같은 물음에 답이 둘이면 갈린 쪽은 조용하다 — 그 사본은 정본이
/// 렉싱을 고쳐도 안 따라오므로, 여기만 옛 판정으로 남는다. `cfg_predicate` 의
/// `one_span_judge` 가 그 갈림을 이제 채널로 잡는다.
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

/// (스캔한 파일 수, 파일별 출하 출현 수).
///
/// 출하 판정이 **두 켜**다. [`shipped_code`] 는 파일 **안**의 `#[cfg(test)]` 구간을 지우고,
/// [`test_only_files`] 는 `#[cfg(test)] mod x;` 로만 선언돼 **파일 통째로** 안 나가는 것을
/// 뺀다. 줄 켜만 두면 그런 파일은 자기 안에 `test` 라는 낱말이 없어 **출하 코드로 읽힌다** —
/// 위 모수 표가 마지막 단계로 적어 둔 바로 그 "없는 결함" 이 파일 단위로 다시 생긴다.
/// 두 켜 다 이미 있는 판정기라 여기서 새로 세지 않는다.
///
/// 이 켜를 더할 때 실측한 것(2026-09-20): 스캔 루트에서 파일 단위로 빠지는 것은 5 개이고
/// 그중 넷은 바늘이 **0 개**라 명부에도 없었다. 즉 이 켜가 가린 것은 없다 — 옮겨진 것은
/// 새로 들어온 `file_picker_scope_tests.rs` 의 4 개뿐이고, 그 넷은 테스트가 scope 를 숨히려
/// 워크스페이스를 **바꾸는** fixture 와 `active_tabs` 가 바늘 `active_tab` 의 부분문자열로
/// 걸린 것이다.
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
        "스캔 루트 {SCAN_ROOTS:?} 에서 .rs 를 {scanned} 개만 걷었다 — 2026-09-07 실측은 훨씬 크다. \
         걷기가 깨지면 아래 대조는 양쪽이 비어 공짜로 성립한다.\n\
         ★ 이 하한을 내려서 통과시키지 마라 — 이 값이 막는 사고는 하나뿐이고, 내리면 그 하나가 사라진다."
    );
    let total: usize = found.values().sum();
    assert!(
        total >= MIN_OCCURRENCES,
        "에이전트 대면 경로에서 활성 상태 읽기를 {total} 개만 찾았다 (2026-09-07 실측 33). \
         정말 줄었으면 명부와 이 하한을 함께 내리고 근거 날짜를 갱신하라 — 다만 **먼저 의심할 것은 \
         마스킹·cfg 제거가 너무 많이 지운 것**이다."
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
        "에이전트 대면 경로의 활성 상태 읽기가 명부와 안 맞는다.\n{}\n\
         새 읽기가 들어왔으면 **갈래를 고르고 사유를 적어** ROSTER 에 넣어라. 갈래를 고르는 \
         그 순간이 이 가드가 사려는 것이다 — 수만 맞추면 아무것도 판정되지 않는다.\n\
         ★ 대상을 고르는 자리라면 갈래는 `OpenDefect` 다. 그 갈래로 적으면 아래 시험이 수를 \
         묻고, 그것이 이 저장소가 그 결함을 아는 유일한 방법이 된다.",
        bad.join("\n")
    );
}

/// `AGENT_FACING` 의 항목이 트리에 실재한다. 파일 단위 항목(포트 구현 파일)은 옮겨지거나
/// 이름이 바뀌면 **아무것도 안 걸러** 명부 대조가 공짜로 성립한다 — 바늘이 0 인 파일은
/// 명부에도 없어서 위 대조가 그 소실을 못 본다.
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
        "에이전트 대면 경로 항목이 스캔 루트 {SCAN_ROOTS:?} 안에 없다 — 옮겨졌으면 새 자리로 \
         고쳐라. 지우지 마라: 포트 구현이 사라진 것이 아니라 옮겨진 것이면 그 구현이 명부 밖으로 \
         나간다.\n  {missing:?}"
    );
}

#[test]
fn the_roster_has_no_file_the_tree_does_not_have() {
    let (_, found) = measure();
    let listed: BTreeSet<&str> = ROSTER.iter().map(|(p, _, _, _)| *p).collect();
    let stale: Vec<&&str> = listed.iter().filter(|p| !found.contains_key(**p)).collect();
    assert!(
        stale.is_empty(),
        "명부에 있는데 트리에 그 읽기가 없는 파일이다. 사라진 자리의 잔재는 다음 사람이 \
         계속 검토하게 만든다 — 지워라:\n  {stale:?}"
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
        "활성 상태로 **대상을 고르는** 자리의 수가 바뀌었다. 늘었으면 그것이 원칙 2.3 위반이고 \
         이 가드가 잡으려던 것이다. 고쳐서 0 이 됐으면 이 수는 이미 0 이라 여기 안 걸린다 — \
         걸렸다는 것은 늘었다는 뜻이다."
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
        "같은 파일의 두 행이 **같은 사유**를 쓴다. 갈래가 둘인데 사유가 하나면 둘 중 하나는 \
         복사된 것이고, 복사된 사유는 판정을 안 거친 표시다:\n{}\n\
         ☆ 파일이 다르면 사유가 비슷해도 된다 — 두 파일이 같은 이유로 같은 일을 할 수 있다.",
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
        "사유가 너무 짧다. 갈래 이름은 사유가 아니다 — 다음 사람이 그 판정을 **재현**할 수 \
         있어야 한다:\n{}",
        thin.join("\n")
    );
}

/// 추출기의 극성 — 무엇을 세고 무엇을 안 세는가.
///
/// 이 픽스처가 없으면 위 대조들은 "추출기가 아무것도 안 센다" 여도 명부를 함께 비우는
/// 순간 통과한다. 특히 `cfg(test)` 제거는 **없는 결함 셋을 만들었다가 지운** 단계라
/// (모듈 머리말 참조) 그 동작을 여기서 단정해 둔다.
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
        "출하 코드의 한 건만 세야 한다 — 주석·문자열·cfg(test) 안의 셋은 빼고. 실제 {n}"
    );
    // 반대 극성: 마스킹 없이 원문을 세면 넷이다. 이 값이 같아지면 마스킹이 죽은 것이다.
    assert_eq!(
        count_needles(fixture),
        4,
        "픽스처 자체가 바뀌었다 — 극성 대조가 성립하지 않는다"
    );
}
