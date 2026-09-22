//! METHOD_TABLE 의 host namespace 별 메서드 수가 기대 스냅샷과 일치하는지 검증한다.
//! drift 발생 시 fail.
//!
//! 0.7.x SemVer 가드의 테스트 측면 — 메서드가 추가되거나 (0.7.x 내 허용) 의도치 않게
//! 제거되었을 때 변화를 강제 가시화한다. 명명 규칙·버전 정책 본문은
//! [`docs/dev-guide/api-conventions.md`]. 그 문서가 명시하듯 **카운트 snapshot 은 본 테스트가
//! 단일 진실 원천(SoT)** 이라 문서에 박지 않고 아래 `EXPECTED` 표에 둔다.

use std::collections::BTreeMap;

use tasty_ipc::method_meta::METHOD_TABLE;

/// host namespace 별 기대 메서드 수 스냅샷. `METHOD_TABLE` 이 SoT 이고 본 표는 그 카운트
/// 미러다. 메서드 추가/제거 시 동기화한다 (추가 = 같은 minor 내 OK, 제거 = SemVer major).
const EXPECTED: &[(&str, usize)] = &[
    ("agent", 33), // + agent.semaphore_set_permits
    ("approval", 9),
    ("attach", 6),
    ("banner", 2),
    ("clipboard", 1),
    ("completion_strategy", 1),
    ("events", 1),
    ("file_handler", 3),
    ("file_picker", 1),
    ("git_viewer", 1),
    ("global_hook", 3),
    ("hook", 3),
    ("hook_handler", 6), // + hook_handler.{get,upsert,remove}
    ("host", 1),         // host.shared_buffer.create — plugin 보조 채널 전용, CLI 진입점 없음
    ("image", 8),
    ("markdown", 1),
    ("markdown_mirror", 1), // markdown_mirror.content_request — plugin 전용 host 메서드, CLI 진입점 없음
    ("memory", 49),
    ("message", 4),
    ("notification", 2),
    ("output", 4),
    ("pane", 2),
    ("plugin", 19),
    ("popup", 1),
    ("preset", 7),
    ("pty", 7),
    ("recent", 1),
    ("remote", 13),
    ("session", 3),
    ("settings", 3),
    // 32 → 30: `surface.raw_key` / `surface.switch_input_source` 가 debug 표
    // (`DEBUG_METHODS`)로 이동. 사용자 입력 재현을 release 표면에서 뺀 보안
    // 목적 제거라 major bump 없이 처리된다(ADR-0115 · api-conventions.md
    // "안정성 정책" 의 보안 예외).
    // + `surface.read_since_scan_mark` — 출력 스캐너 전용 커서 (ADR-0307).
    ("surface", 33),
    ("system", 3), // + system.pressure
    ("tab", 4),
    ("telemetry", 12),
    ("terminal", 11),
    ("theme", 1),
    ("timer", 1),
    ("ui", 1),
    ("view", 3),
    ("webhook", 6),
    ("webview", 1),
    ("window", 3),
    ("workspace", 5),
    ("workspace_category", 5),
];

#[test]
fn cli_naming_namespace_counts_match_method_table() {
    let documented: BTreeMap<String, usize> = EXPECTED
        .iter()
        .map(|(ns, c)| (ns.to_string(), *c))
        .collect();
    let actual = actual_namespace_counts();

    let mut errors = Vec::new();

    for (ns, count) in &actual {
        match documented.get(ns) {
            Some(d) if d == count => {}
            Some(d) => errors.push(format!(
                "namespace `{ns}`: EXPECTED={d}, METHOD_TABLE={count}"
            )),
            None => errors.push(format!(
                "namespace `{ns}` (METHOD_TABLE={count}) 가 EXPECTED 스냅샷에 누락"
            )),
        }
    }
    for ns in documented.keys() {
        if !actual.contains_key(ns) {
            errors.push(format!(
                "namespace `{ns}` 가 EXPECTED 스냅샷에 있지만 METHOD_TABLE 에 없음"
            ));
        }
    }

    assert!(
        errors.is_empty(),
        "host namespace count 스냅샷이 METHOD_TABLE 과 drift:\n  {}\n\
         갱신: 메서드 추가/제거 후 tests/cli_naming_count_drift.rs 의 \
         `EXPECTED` 표를 동기화.",
        errors.join("\n  ")
    );
}

fn actual_namespace_counts() -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for (name, _) in METHOD_TABLE {
        if let Some((ns, _)) = name.split_once('.') {
            *out.entry(ns.to_string()).or_insert(0) += 1;
        }
    }
    out
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn actual_counts_has_known_namespaces() {
        let counts = actual_namespace_counts();
        assert!(counts.contains_key("memory"));
        assert!(counts.contains_key("agent"));
        assert!(counts.contains_key("surface"));
    }
}
