//! 호스트 IPC prefix가 예약돼 있거나 번들 플러그인의 namespace로 분류돼 있는지 확인한다.
//! 플러그인이 호스트용 이름을 점유하면 같은 prefix의 호출이 플러그인으로 전달될 수 있다.
//! 규칙은 [플러그인 namespace 가이드](../../docs/dev-guide/plugin-development.md#cli--ipc-namespace)를 따른다.
//! 예약 목록·메서드 표는 연결된 상수로 비교하고, 번들 namespace와 겹치는 호스트 처리 이름은 소스에서 수집한다.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tasty_ipc::method_meta::{DEBUG_METHODS, METHOD_TABLE, PREFIX_RULES};
use tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES;

/// 2026-09-05 호스트 메서드 276개를 측정한 뒤 빈 표를 찾도록 둔 하한이다.
const MIN_HOST_METHODS: usize = 200;

/// 2026-09-05 prefix 45개를 측정했다. 메서드 수가 그대로여도 prefix 추출이 실패할 수 있어 별도 하한을 둔다.
const MIN_HOST_PREFIXES: usize = 35;

/// 번들 플러그인이 사용하는 namespace를 예약하면 매니페스트가 거절된다. 겹치는 호스트 구현은 아래 중계 목록과 별도로 대조한다.
const CLAIMED_BY_A_BUNDLED_PLUGIN: &[(&str, &str)] = &[
    ("image", "공식 image 플러그인의 ipc_namespace"),
    ("markdown", "공식 markdown 플러그인의 ipc_namespace"),
];

/// 현재 호스트 메서드는 없지만 선점하지 못하도록 예약한 이름과 그 사유.
const RESERVED_AHEAD_OF_ANY_METHOD: &[(&str, &str)] = &[
    (
        "ime",
        "`surface.ime_*`(debug 전용 prefix 규칙)의 이름 공간. 최상위 `ime.*` 는 아직 없다",
    ),
    (
        "fs",
        "최상위 fs.*를 플러그인이 호스트 파일시스템 기능처럼 사용하지 못하도록 예약한다. 현재 이 prefix의 호스트 메서드는 없다.",
    ),
    (
        "ipc",
        "IPC 자체를 가리키는 이름 — plugin 이 가질 자리가 아니다",
    ),
    (
        "tool",
        "매니페스트 `[[contributes.tool]]` 이 쓰는 이름 공간",
    ),
];

/// 현재 빌드의 메서드·debug 표에서 prefix를 모은다. debug_assertions에 따라 집합이 달라지고 점 없는 이름은 전체를 prefix로 센다.
fn host_prefixes() -> BTreeSet<&'static str> {
    let methods = METHOD_TABLE
        .iter()
        .chain(DEBUG_METHODS.iter())
        .map(|(name, _)| *name);
    let rules = PREFIX_RULES.iter().map(|(name, _)| *name);
    methods
        .chain(rules)
        .filter_map(|name| name.split('.').next())
        .filter(|p| !p.is_empty())
        .collect()
}

#[test]
fn every_host_method_prefix_is_reserved_or_carries_a_reason() {
    assert!(
        METHOD_TABLE.len() >= MIN_HOST_METHODS,
        "호스트 메서드가 {}개로 하한 {MIN_HOST_METHODS} 미만이다(2026-09-05 측정 276개). 표 수집을 확인한다.",
        METHOD_TABLE.len()
    );
    let host = host_prefixes();
    assert!(
        host.len() >= MIN_HOST_PREFIXES,
        "호스트 prefix 가 {} 개뿐이다(하한 {MIN_HOST_PREFIXES})",
        host.len()
    );
    let reserved: BTreeSet<&str> = RESERVED_IPC_PREFIXES.iter().copied().collect();
    let excused: BTreeSet<&str> = CLAIMED_BY_A_BUNDLED_PLUGIN
        .iter()
        .map(|(p, _)| *p)
        .collect();

    let both: Vec<&str> = excused
        .iter()
        .copied()
        .filter(|p| reserved.contains(p))
        .collect();
    assert!(
        both.is_empty(),
        "예약과 예외에 동시에 있는 prefix다. 실제 정책을 확인해 한쪽 목록에서 제거한다: {both:?}"
    );

    let unaccounted: Vec<&str> = host
        .iter()
        .copied()
        .filter(|p| !reserved.contains(p) && !excused.contains(p))
        .collect();
    let stale: Vec<&str> = excused
        .iter()
        .copied()
        .filter(|p| !host.contains(p))
        .collect();

    assert!(
        unaccounted.is_empty() && stale.is_empty(),
        "호스트 prefix 분류가 다르다.\n  예약·예외에 없는 이름: {unaccounted:?}\n  예외에만 있는 이름: {stale:?}\n새 호스트 이름은 RESERVED_IPC_PREFIXES에 예약하거나 번들 namespace로 유지해야 하는 사유를 등록한다."
    );
}

#[test]
fn every_reserved_prefix_without_a_host_method_says_why() {
    let host = host_prefixes();
    let explained: BTreeSet<&str> = RESERVED_AHEAD_OF_ANY_METHOD
        .iter()
        .map(|(p, _)| *p)
        .collect();

    let unexplained: Vec<&str> = RESERVED_IPC_PREFIXES
        .iter()
        .copied()
        .filter(|p| !host.contains(p) && !explained.contains(p))
        .collect();
    let stale: Vec<&str> = explained
        .iter()
        .copied()
        .filter(|p| host.contains(p) || !RESERVED_IPC_PREFIXES.contains(p))
        .collect();

    assert!(
        unexplained.is_empty() && stale.is_empty(),
        "메서드 없는 예약: {unexplained:?} / 사유가 낡은 항목: {stale:?}. \
         뒤의 것은 그 prefix 에 호스트 메서드가 생겼거나 예약이 풀렸다는 뜻이라, \
         RESERVED_AHEAD_OF_ANY_METHOD 에서 빼야 한다"
    );
}

/// 집합 비교만으로는 중복을 찾지 못하므로 원래 목록의 정렬·중복도 확인한다.
#[test]
fn the_reserved_list_is_sorted_and_free_of_duplicates() {
    let mut sorted = RESERVED_IPC_PREFIXES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.as_slice(),
        RESERVED_IPC_PREFIXES,
        "RESERVED_IPC_PREFIXES 는 사전순 · 중복 없이 유지한다"
    );
}

const DISPATCH_ROOT: &str = "src/adapters/ipc/handler.rs";

/// 2026-09-05 dispatch의 메서드 리터럴 213개를 측정한 뒤 빈 수집을 찾도록 둔 하한이다.
const MIN_DISPATCH_METHOD_LITERALS: usize = 150;

/// 번들 namespace와 겹치는 호스트 처리 이름이다. 외부 호출은 대부분 먼저 플러그인으로 전달되므로
/// 플러그인이 host.call로 다시 넘겨야 호스트 구현에 도달한다. GUI 호스트가 직접 변환하는 이름은
/// 플러그인을 거치지 않는다. 비활성화는 namespace 소유를 해제하지 않는다.
/// 이 검사는 겹치는 이름의 명부를 대조하며 실제 중계 동작은 검증하지 않는다.
const SHARED_WITH_A_BUNDLED_PLUGIN: &[(&str, &str)] = &[
    (
        "image.list",
        "플러그인이 요청을 받아 host.call로 호스트의 surface 목록 조회에 전달한다",
    ),
    (
        "image.open",
        "GUI 호스트는 직접 변환하고, 헤드리스에서는 플러그인이 받아 host.call로 호스트에 전달한다",
    ),
    (
        "markdown.navigate",
        "플러그인이 markdown.navigate를 수신해 host.call로 호스트에 전달한다",
    ),
];

/// 주석을 제거한 dispatch 소스에서 메서드 형태의 리터럴을 수집한다.
fn dispatch_method_literals(src: &str) -> BTreeSet<String> {
    let body = super::strip_comments(src);
    let mut out = BTreeSet::new();
    let mut rest = body.as_str();
    while let Some(at) = rest.find('"') {
        let after = &rest[at + 1..];
        match after.find('"') {
            Some(end) => {
                let lit = &after[..end];
                let ok = lit.split_once('.').is_some_and(|(a, b)| {
                    !a.is_empty()
                        && !b.is_empty()
                        && a.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                        && b.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                });
                if ok {
                    out.insert(lit.to_string());
                }
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

fn dispatch_root() -> PathBuf {
    super::repo_root().join(DISPATCH_ROOT)
}

#[test]
fn the_methods_a_bundled_plugin_can_shadow_are_pinned() {
    let src = std::fs::read_to_string(dispatch_root())
        .unwrap_or_else(|e| panic!("{DISPATCH_ROOT} 읽기 실패: {e}"));
    let literals = dispatch_method_literals(&src);
    assert!(
        literals.len() >= MIN_DISPATCH_METHOD_LITERALS,
        "dispatch 리터럴을 {}개만 읽었다(하한 {MIN_DISPATCH_METHOD_LITERALS}, 2026-09-05 측정 213개). 추출 범위를 확인한다.",
        literals.len()
    );

    let claimed: BTreeSet<&str> = CLAIMED_BY_A_BUNDLED_PLUGIN
        .iter()
        .map(|(p, _)| *p)
        .collect();
    let found: BTreeSet<&str> = literals
        .iter()
        .filter(|m| m.split_once('.').is_some_and(|(p, _)| claimed.contains(p)))
        .map(|m| m.as_str())
        .collect();
    let pinned: BTreeSet<&str> = SHARED_WITH_A_BUNDLED_PLUGIN
        .iter()
        .map(|(m, _)| *m)
        .collect();

    assert_eq!(
        found, pinned,
        "번들 namespace와 겹치는 호스트 처리 이름이 달라졌다. 새 이름을 플러그인이 호스트로 중계하는지 확인해 명부에 근거를 적고, 없어진 이름은 제거한다."
    );
}

/// 검사 파일의 동결 목록을 실제 dispatch로 다시 읽지 않도록 대상 파일을 제한한다.
#[test]
fn the_dispatch_scan_root_is_a_single_file_that_is_not_this_one() {
    assert!(
        !DISPATCH_ROOT.contains("source_guards"),
        "스캔 루트({DISPATCH_ROOT})가 이 가드가 사는 곳을 포함한다"
    );
    assert!(
        dispatch_root().is_file(),
        "{DISPATCH_ROOT} 가 파일이 아니다 — 디렉터리로 넓어지면 이 가드도 삼켜진다"
    );
}
