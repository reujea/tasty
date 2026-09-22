//! 창 소유 목록 자원의 **합산 소속 명부**. `src/app/dispatch/list_global.rs` 의
//! 합산 집합과 이 명부가 양방향으로 맞물린다.
//!
//! ## 왜 이 판정기인가 — 자동 발견은 네 형태로 다 새었다
//!
//! "창 소유 컬렉션을 순회 · 대상 인자 없음 · 합산 집합에 없음"([ADR-0175](../../../docs/adr/0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md))
//! 을 **정적으로 자동 발견**하려는 시도는 이 저장소에서 네 번 다 실패했고, 넷 다
//! 실측이다.
//!
//! 1. **이름 모양**(`*.list` / `*_list`)으로 뽑으면 `tree` 가 빠진다 — 그리고 실제로
//!    새고 있던 것이 그 `tree` 였다.
//! 2. **"params 를 안 받는다"** 로 대상 인자 유무를 근사하면 `hook.list` 가 빠진다 —
//!    그 params 는 대상이 아니라 **필터**(`surface_id`)다. 거짓 음성이라 "고칠 것이
//!    없다" 로 보인다.
//! 3. **`engine.<필드>` 줄 단위 grep** 은 rustfmt 가 줄을 접은 `engine\n  .hook_manager`
//!    를 못 본다. 그 형태로 네 필드가 통째로, `workspaces` 는 일곱 자리가 안 보였다.
//! 4. **`CoreState` 의 컬렉션 필드 전수**로 올리면 명부가 ~50 이 되는데 대부분이
//!    `pending_*` 버퍼·per-surface 캐시라 **자원이 아니다.** 크기가 뜻을 죽인다.
//!
//! 그래서 자동 발견을 포기하고 **손으로 유지하는 명부**로 간다. 이 명부가 못 하는
//! 것을 먼저 적는다: **새 list 메서드가 들어와도 이 가드는 모른다.** 할 수 있는 것은
//! 두 가지다 — 합산 집합에서 무엇이 **빠지는** 것을 잡고, 지금 안 합산되는 자원이
//! **왜** 그런지를 검사받는 텍스트로 만든다.
//!
//! ## 옆 명부와 **물음이 다르다** — 합치지 않는다
//!
//! `src/source_guards/routing_key_method_scope.rs` 의 `PAIR_EXEMPT` 도 포커스 대체 축의
//! 명부이고 겹치는 메서드도 있지만, 묻는 것이 다르다:
//!
//! | | 묻는 것 | 답이 옳을 때 |
//! |---|---|---|
//! | 이 명부 | 이 목록을 **합쳐야 하는가** | 호출자가 전 창의 자원을 본다 |
//! | `PAIR_EXEMPT` | 이 요청이 **주인 창을 찾는가** | 지목한 자원이 있는 창으로 간다 |
//!
//! 읽기(전 창 합산)와 지목(한 창 해석)은 같은 자원에 대해 **둘 다** 필요할 수 있다 —
//! global hook 이 그 예다: 여기서는 `Aggregated` 이고 저기서는 `Kind::GlobalHook` 으로
//! 푼다. 하나로 합치면 그중 한 물음의 답이 사라진다.
//!
//! ## 이 가드가 도는 자리가 값이다
//!
//! 같은 축의 실행 단언은 `tests/e2e_tests.rs` 의 `multi_window_owner_routing` 에
//! 있는데, 그것은 창을 요구해 헤드리스 조합 CI 가 이름으로 `--skip` 하는 유일한
//! 테스트다. 즉 **자동으로 도는 채널이 없다.** 이 가드는 `tasty-doc-guards` 라
//! 경로 필터 없는 잡에서 push 마다 돈다. 둘은 겹치는 것이 아니라 **채널이 다르다.**

use std::path::{Path, PathBuf};
use tasty_doc_guards::match_arms::{Source, matching_close};

#[derive(PartialEq, Debug)]
enum Class {
    /// `dispatch_list_global` 이 전 engine 을 합쳐 답한다.
    Aggregated,
    /// 호출자가 대상 id 를 실어 라우터가 주인 창을 푼다. 합산이 필요 없다.
    TargetedByCallerId,
    /// 저장소가 engine 을 건너 공유된다. 어느 창으로 가도 같은 답이라 합산이 항등이다.
    SharedAcrossEngines,
    /// **창별인데 합산되지 않는다 — 열린 결함.** 사유 칸에 무엇이 막고 있는지 적는다.
    PerEngineNotAggregated,
}
use Class::*;

/// (메서드, 갈래, 사유). 사유는 모든 갈래에서 필수다 — 갈래 이름만으로는 다음 사람이
/// 판정을 재현하지 못한다.
const ROSTER: &[(&str, Class, &str)] = &[
    (
        "workspace.list",
        Aggregated,
        "워크스페이스는 창 소유. id 가 IdGenerator 공유라 이어 붙이면 키가 된다",
    ),
    ("surface.list", Aggregated, "상동 — surface id 공유"),
    ("pane.list", Aggregated, "상동 — pane id 공유"),
    (
        "pty.list",
        Aggregated,
        "headless pty. id 공유가 빠져 있던 동안 두 창의 pty 가 같은 id 를 받아 먼저 만든 쪽이 닿지 않았다",
    ),
    ("output.observe_list", Aggregated, "observer id 공유"),
    (
        "workspace_category.list",
        Aggregated,
        "category id 공유. 예약 `normal`(id 0)만 한 줄로 접는다",
    ),
    (
        "tree",
        Aggregated,
        "이름이 `*.list` 가 아니라 이름 기반 census 에서 빠져 있었다 — ADR-0175 가 그 자리다",
    ),
    (
        "tab.list",
        TargetedByCallerId,
        "`pane_id` 필수. 실측으로 비포커스 창의 pane 을 지목해 답이 온다",
    ),
    (
        "surface.meta.list",
        TargetedByCallerId,
        "`surface_id` 필수. 실측 동일",
    ),
    (
        "memory.list",
        SharedAcrossEngines,
        "memory store 는 `new_with_ids` 인자로 전 engine 이 같은 Arc 를 든다",
    ),
    (
        "hook.list",
        Aggregated,
        "hook id 가 IdGenerator 공유로 바뀌어 창을 건너 유일하다. `surface_id` 는 대상이 아니라 필터라 주인 창을 정하지 않는다 — 그래서 합산이 답이다",
    ),
    (
        "global_hook.list",
        Aggregated,
        "global hook id 도 IdGenerator 공유. 이름과 달리 창에 매인다(`global_hook_manager` 가 CoreState 필드) — 그 성질 때문에 합산이 필요하고, 지목은 Kind::GlobalHook 이 따로 푼다",
    ),
    (
        "notification.list",
        Aggregated,
        "각 engine의 알림을 합치며 IdGenerator 공유 카운터가 ID 충돌을 막는다. 생성 ID 역순으로 전체 50개를 반환하고 UI 패널은 창별로 유지한다",
    ),
    (
        "approval.list",
        SharedAcrossEngines,
        "생성자만 읽으면 engine 마다 `Arc::new` 라 창별로 보이는데, 두 번째 main window 를 세우는 `App::ensure_engine_and_plugins`(`src/app/window_lifecycle.rs`)가 첫 engine 의 `approval_store` Arc 로 덮어쓴다 — 저장소가 어디 사는지는 **생성자와 창 생성 경로를 함께** 읽어야 정해진다",
    ),
    (
        "attach.list",
        Aggregated,
        "engine 별 `OccupancyRegistry` 라 창별인 것은 맞지만, 두 배열의 키(`surface_id`·`workspace_id`)가 IdGenerator 공유라 이어 붙이면 그대로 키가 된다 — 지목 축이 안 막혀 있어 합산이 곧 답이다. 결과가 이름 붙은 배열 **둘**이라 합산 함수가 한 순회에서 둘을 꺼낸다(따로 부르면 두 배열이 서로 다른 시점의 스냅샷이 된다)",
    ),
    // ── 아래는 dispatch 표의 `.list` 전수를 명부와 대조하다 드러난 것들이다.
    // 그 대조가 없던 동안 이 명부는 `.list` 27 중 13 만 덮고 있었다.
    (
        "image.list",
        Aggregated,
        "`engine.workspaces` 를 순회한다. 항목의 키가 `surface_id` 라 창을 건너 유일하다(`surface.list` 와 같은 근거). 외부 호출은 plugin namespace forward 가 먼저 집지만 plugin 이 trampoline 으로 host 에 되돌리고 그 되돌림이 합산을 지난다",
    ),
    (
        "completion_strategy.list",
        SharedAcrossEngines,
        "`completion_strategy::global()` — 프로세스 전역 레지스트리라 어느 창으로 가도 같다",
    ),
    (
        "hook_handler.list",
        SharedAcrossEngines,
        "`hook_handler::global()` — 상동. hook **핸들러**는 전역이고 hook **인스턴스**만 창 소유다",
    ),
    (
        "webhook.list",
        SharedAcrossEngines,
        "`webhook::list()` — 프로세스 전역",
    ),
    (
        "session.list",
        SharedAcrossEngines,
        "`core.session_list()` — `core` 는 전 engine 이 같은 것을 본다",
    ),
    (
        "preset.list",
        SharedAcrossEngines,
        "`core.preset_store` (공유 Mutex). 핸들러가 `state` 를 받지만 `_state` 로 안 쓴다",
    ),
    (
        "memory.secret.list",
        SharedAcrossEngines,
        "`core.with_memory` — `memory.list` 와 같은 저장소",
    ),
    (
        "telemetry.cap.list",
        SharedAcrossEngines,
        "`core` 만 읽는다(`_state` · `_engine` 미사용)",
    ),
    ("telemetry.anomaly.list", SharedAcrossEngines, "상동"),
    (
        "remote.profile.list",
        SharedAcrossEngines,
        "`RemoteProfiles::load` — 파일에서 읽으므로 engine 과 무관",
    ),
    (
        "remote.passkey.list",
        SharedAcrossEngines,
        "`Passkeys::load` — 상동",
    ),
];

/// 이 축의 범위 밖인 `.list` 메서드와 그 이유.
///
/// debug IPC 는 **사용자 조작을 재현하는** 검증 도구이고 release 표면에 없다
/// (CLAUDE.md 의 불가침 원칙 1). 포커스 독립성은 *에이전트 기능*에 거는 요구라
/// 여기 셋은 같은 잣대로 재지 않는다 — 다만 범위 밖이라는 것을 **적어 둬야**
/// 다음 사람이 누락과 못 가른다.
const OUT_OF_SCOPE: &[(&str, &str)] = &[
    (
        "debug.tool.list",
        "debug 전용. `state.tool_registry` 라 창별인 것은 맞다",
    ),
    (
        "debug.banner.list",
        "debug 전용. 정의 목록 자체는 `all_defs()` 로 전역이다",
    ),
    ("debug.host_popup.list", "debug 전용. 상동"),
];

fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/tasty-doc-guards → 레포 루트.
    p.pop();
    p.pop();
    p
}

/// `dispatch_list_global` 의 match arm 에서 메서드 이름을 뽑는다.
///
/// 팔은 공용 판정기(`tasty_doc_guards::match_arms`)가 뗀다. 줄이 `"…" =>` 로 시작할 때만
/// 세던 판독은 `"a" | "b" =>` 와 guard 가 붙은 `"a" if … =>` 를 통째로 놓쳤다 — 그러면 "합산기에
/// 명부가 모르는 것이 없다" 방향이 조용히 초록이 된다. 따옴표 이름도 `_` 도 아닌 조각은 누가
/// 합산되는지 판정할 수 없으므로 실패시킨다. 정의가 `#[cfg]` 로 둘 이상이면 전부 읽는다.
fn aggregated_arms(root: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(root.join("src/app/dispatch/list_global.rs"))
        .expect("list_global.rs 를 읽지 못했다 — 경로가 바뀌었으면 이 가드도 함께 옮긴다");
    let src = Source::new(&text);
    let bodies = src.fn_bodies("dispatch_list_global");
    assert!(
        !bodies.is_empty(),
        "`fn dispatch_list_global` 을 못 찾았다 — 이름이 바뀌었으면 이 가드도 함께 옮긴다"
    );
    let mut out = Vec::new();
    for body in bodies {
        const HEAD: &str = "match request.method.as_str()";
        let block = src
            .code_slice(&body)
            .find(HEAD)
            .and_then(|k| {
                let from = body.start + k + HEAD.len();
                src.code[from..].find('{').map(|o| from + o)
            })
            .and_then(|open| matching_close(&src.code, open).map(|close| open..close + 1))
            .unwrap_or_else(|| panic!("`{HEAD} {{ … }}` 를 못 찾았다 — 합산기 모양이 바뀌었다"));
        let arms = src
            .match_arms(block)
            .unwrap_or_else(|e| panic!("합산기 팔을 못 읽었다 — {e}"));
        for arm in arms {
            for alt in src.alternatives(&arm.pattern) {
                match src.plain_string(&alt) {
                    Some(name) => out.push(name.to_string()),
                    None if src.slice(&alt) == "_" => {}
                    None => panic!(
                        "합산기 {}행의 팔 조각 `{}` 은 따옴표 이름이 아니라 누가 합산되는지 \
                         판정할 수 없다",
                        src.line_of(alt.start),
                        src.slice(&alt)
                    ),
                }
            }
        }
    }
    out
}

#[test]
fn every_aggregated_entry_is_actually_in_the_aggregator() {
    let arms = aggregated_arms(&repo_root());
    let missing: Vec<_> = ROSTER
        .iter()
        .filter(|(m, c, _)| *c == Aggregated && !arms.iter().any(|a| a == m))
        .map(|(m, _, _)| *m)
        .collect();
    assert!(
        missing.is_empty(),
        "명부가 합산이라고 적은 메서드가 `dispatch_list_global` 에 없다: {missing:?}\n\
         합산에서 빠지면 그 목록은 **포커스된 창의 것만 답하고 에러가 없다.**"
    );
}

#[test]
fn the_aggregator_has_nothing_the_roster_does_not_know() {
    let arms = aggregated_arms(&repo_root());
    let unknown: Vec<_> = arms
        .iter()
        .filter(|a| !ROSTER.iter().any(|(m, _, _)| m == *a))
        .collect();
    assert!(
        unknown.is_empty(),
        "합산 집합에 명부가 모르는 메서드가 있다: {unknown:?}\n\
         명부에 갈래와 사유를 적어라 — 합산은 id 가 창을 건너 유일할 때만 옳다."
    );
}

#[test]
fn every_entry_carries_a_reason() {
    let empty: Vec<_> = ROSTER
        .iter()
        .filter(|(_, _, why)| why.trim().is_empty())
        .map(|(m, _, _)| *m)
        .collect();
    assert!(empty.is_empty(), "사유가 빈 명부 항목: {empty:?}");
}

/// 한 메서드가 **두 행에** 있으면 빨감 — 명부 안에서도, 범위 밖 목록과 사이에서도.
///
/// 커버리지 검사(`the_roster_covers_…`)는 **빠진 것**만 본다. 두 행이 남는 형태는 그
/// 검사에 안 걸린다 — 덮이기는 덮이니까. 그런데 그 형태는 병합에서 생긴다: 두 회차가
/// 같은 메서드를 서로 다른 갈래로 넣고 둘 다 살아남으면, 이 명부는 **한 자원에 대해 두
/// 답을 든 채로 초록**이 된다. 실측으로 났다(`image.list` 병합).
///
/// 어느 행이 옳은지는 이 가드가 못 정한다. 정하라고 말하는 것이 이 가드의 일이다.
/// 같은 메서드가 두 행에 있지 않은지.
///
/// **하한이 없다.** 한때 `MIN_LISTED` 가 있었는데, 그 doc 이 스스로 적어 두었듯 그 수가
/// 지키는 것은 값의 정확성이 아니라 **명부가 공허하지 않다는 것**뿐이었다. 그리고 그
/// 공허는 여기서 볼 일이 아니다 — 명부가 비면 아래 셋 중 마지막이 먼저 빨개지고,
/// 그쪽은 실제 소스를 훑으므로 **자기 스캔 하한**을 따로 갖고 있다:
///
/// ```text
/// every_aggregated_entry_is_actually_in_the_aggregator       명부 → 합산기
/// the_aggregator_has_nothing_the_roster_does_not_know        합산기 → 명부
/// the_roster_covers_every_list_method_in_the_dispatch_table  디스패치 표 → 명부  ← 스캔 하한
/// ```
///
/// 그래서 여기서는 수를 손으로 박지 않고 **명부에서 도출한다.** 도출한 기대값은 명부가
/// 자라도 안 낡는다 — 손으로 박은 수는 자랄 때마다 누군가 올려야 하고, 안 올리면 그 차이가
/// 곧 안 보는 구간이 된다.
#[test]
fn no_method_is_listed_twice() {
    let mut seen: std::collections::BTreeMap<&str, Vec<String>> = std::collections::BTreeMap::new();
    for (m, class, _) in ROSTER {
        seen.entry(m)
            .or_default()
            .push(format!("ROSTER({class:?})"));
    }
    for (m, _) in OUT_OF_SCOPE {
        seen.entry(m).or_default().push("OUT_OF_SCOPE".to_string());
    }
    let dupes: Vec<String> = seen
        .iter()
        .filter(|(_, wheres)| wheres.len() > 1)
        .map(|(m, wheres)| format!("  {m} — {}", wheres.join(" + ")))
        .collect();
    // 서로 다른 이름 수가 두 명부의 항목 수 합과 같다는 것은 **중복이 없다는 것과
    // 같은 말**이다(각 항목이 정확히 한 번씩 들어가므로). 기대값을 명부에서 도출하므로
    // 명부가 자라도 이 수는 손볼 데가 없다.
    assert_eq!(
        seen.len(),
        ROSTER.len() + OUT_OF_SCOPE.len(),
        "같은 메서드가 여러 행에 있다. 갈래가 둘이면 답도 둘이고, 다음 사람은 먼저 읽은 \
         쪽을 믿는다 — 어느 쪽이 옳은지 정해서 한 행만 남겨라:\n{}",
        dupes.join("\n")
    );
}

/// dispatch 표에서 `"<이름>.list"` 형태의 메서드를 전부 뽑는다.
///
/// **자동 발견이 아니다.** 창 소유인지 판정하려는 것이 아니라 — 그 술어는 이 저장소에서
/// 네 번 다 샜다(모듈 doc 참조) — **이 명부가 표를 덮는지**만 본다. 이름 모양으로는
/// `tree` 같은 것을 못 보지만, 그런 것은 명부가 손으로 덮는다. 여기서 잡는 것은 그 반대
/// 방향이다: 표에 있는데 명부에도 범위 밖 목록에도 없는 것.
fn dispatch_list_methods(root: &Path) -> Vec<String> {
    let src = std::fs::read_to_string(root.join("src/adapters/ipc/handler.rs"))
        .expect("handler.rs 를 읽지 못했다 — 경로가 바뀌었으면 이 가드도 함께 옮긴다");
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let mut rest = t;
        while let Some(at) = rest.find('"') {
            let after = &rest[at + 1..];
            let Some(end) = after.find('"') else { break };
            let lit = &after[..end];
            if lit.ends_with(".list")
                && lit
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
            {
                out.push(lit.to_string());
            }
            rest = &after[end + 1..];
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 명부가 dispatch 표의 `.list` 를 덮는가.
///
/// 이 검사가 없던 동안 명부는 표의 `.list` 27 중 13 만 덮고 있었고, 빠진 것 중에
/// **창 소유가 하나 있었다**(`image.list` — `engine.workspaces` 를 순회한다). 명부의
/// 한계로 적혀 있던 "새 list 메서드가 들어와도 모른다" 가 실제로 그만큼 벌어져 있었다.
#[test]
fn the_roster_covers_every_list_method_in_the_dispatch_table() {
    let root = repo_root();
    let found = dispatch_list_methods(&root);
    assert!(
        found.len() >= 20,
        "dispatch 표에서 `.list` 를 {} 개밖에 못 뽑았다 — 추출이 깨졌다. \
         모수가 줄면 '빠진 것 없음' 은 언제나 참이다",
        found.len()
    );
    let known: std::collections::BTreeSet<&str> = ROSTER
        .iter()
        .map(|(m, _, _)| *m)
        .chain(OUT_OF_SCOPE.iter().map(|(m, _)| *m))
        .collect();
    let missing: Vec<&String> = found
        .iter()
        .filter(|m| !known.contains(m.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "dispatch 표에 있는데 명부에도 범위 밖 목록에도 없는 `.list` 메서드다. \
         창 소유 컬렉션을 순회하면 갈래와 사유를 달아 `ROSTER` 에, 그렇지 않으면 \
         `OUT_OF_SCOPE` 에 이유와 함께 적어라 — 어느 쪽인지 **적히지 않은 것**이 \
         이 명부의 사각이다:\n  {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn the_open_ones_are_not_silently_emptied() {
    // 열린 결함이 사라졌다면 그것은 고쳐졌다는 뜻이고, 그때 갈래를 옮기는 것이
    // 이 명부를 갱신하는 방법이다. 수를 박아 두는 것은 그 갱신을 **강제**하기 위한
    // 것이지 그 수가 옳다는 뜻이 아니다.
    let open = ROSTER
        .iter()
        .filter(|(_, c, _)| *c == PerEngineNotAggregated)
        .count();
    assert_eq!(
        open, 0,
        "창별인데 합산 안 되는 항목의 수가 바뀌었다. 고쳤으면 갈래를 옮기고 이 수를 \
         함께 내려라 — 남겨 두면 다음 사람이 이미 닫힌 것을 다시 센다."
    );
}
