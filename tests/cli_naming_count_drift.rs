//! METHOD_TABLE의 namespace별 메서드 수를 기록과 비교한다. 이름 추가·제거는 api-conventions의 호환성 정책을 따르고 이 기록도 갱신한다.

use std::collections::BTreeMap;

use tasty_ipc::method_meta::METHOD_TABLE;

/// 메서드 표 변경 시 검토할 namespace별 개수.
const EXPECTED: &[(&str, usize)] = &[
    ("agent", 39),
    ("approval", 9),
    ("attach", 9),
    ("banner", 2),
    ("clipboard", 1),
    ("completion_strategy", 1),
    ("events", 1),
    ("file_handler", 3),
    ("file_picker", 1),
    ("git_viewer", 1),
    ("global_hook", 3),
    ("hook", 3),
    ("hook_handler", 7),
    ("host", 1), // host.shared_buffer.create — plugin 보조 채널 전용, CLI 진입점 없음
    ("image", 8),
    ("markdown", 1),
    ("markdown_mirror", 1), // markdown_mirror.content_request — plugin 전용 host 메서드, CLI 진입점 없음
    ("memory", 49),
    ("message", 4),
    ("notification", 4),
    ("output", 4),
    ("pane", 2),
    ("plugin", 19),
    ("popup", 1),
    ("preset", 7),
    ("pty", 7),
    ("recent", 1),
    ("remote", 15),
    ("run", 1),
    ("session", 3),
    ("settings", 8),
    // 입력 재현 메서드는 release에서 제외된 debug 표에 있어 이 개수에 포함되지 않는다(docs/dev-guide/plugin-permissions.md).
    ("surface", 40),
    ("system", 3),
    ("tab", 4),
    ("telemetry", 12),
    ("terminal", 11),
    ("theme", 1),
    ("timer", 1),
    ("ui", 1),
    ("view", 3),
    ("webhook", 6),
    ("webview", 2), // + webview.open_external — plugin 전용, CLI 진입점 없음
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
        "namespace별 메서드 수가 기록과 다르다. 실제 추가·제거를 확인하고 EXPECTED를 갱신한다:\n  {}",
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
