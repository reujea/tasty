//! `task-graph`·`dag-get` 의 `--format dot` 출력. stdout 에는 Graphviz 에 바로 넘길 수 있는
//! DOT 본문만 쓰고, 순환과 runner 상태는 stderr 에 쓴다. JSON 응답 전체가 필요하면
//! `--format json` 을 쓴다.

use anyhow::Result;

use crate::out::{errln, out};

/// DOT 응답을 stdout 본문과 stderr 줄로 나눈다. DOT 응답이 아니면 None.
pub(super) fn split_dot_response(result: &serde_json::Value) -> Option<(String, Vec<String>)> {
    if result.get("format").and_then(|v| v.as_str()) != Some("dot") {
        return None;
    }
    let mut body = result.get("dot")?.as_str()?.to_string();
    if !body.ends_with('\n') {
        body.push('\n');
    }
    Some((body, graph_notes(result)))
}

/// stdout 본문과 나눠 stderr 에 쓰는 순환과 runner 요약 줄.
pub(super) fn graph_notes(result: &serde_json::Value) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(cycle) = result.get("cycle").and_then(|v| v.as_str()) {
        notes.push(format!("cycle: {cycle}"));
    }
    if let Some(runner) = result.get("runner").filter(|v| v.is_object()) {
        notes.push(super::format_runner_summary(runner));
    }
    notes
}

/// DOT 응답이면 나눠 쓰고 true, 아니면 아무것도 쓰지 않고 false.
pub(super) fn write_dot_response(result: &serde_json::Value) -> Result<bool> {
    let Some((body, notes)) = split_dot_response(result) else {
        return Ok(false);
    };
    out!("{body}")?;
    for note in notes {
        errln!("{note}");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_dot_response_writes_only_the_graph_to_stdout() {
        let (body, notes) = split_dot_response(&json!({
            "format": "dot", "dot": "digraph G {\n}", "cycle": "a -> b -> a",
            "runner": {"running": false, "crashed": false, "ready_count": 1, "running_count": 0}
        }))
        .expect("dot response");
        assert_eq!(body, "digraph G {\n}\n");
        assert_eq!(notes[0], "cycle: a -> b -> a");
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn a_json_response_is_left_to_the_json_printer() {
        assert!(split_dot_response(&json!({"format": "json", "nodes": []})).is_none());
        let (_, notes) =
            split_dot_response(&json!({"format": "dot", "dot": "digraph G {}\n", "cycle": null}))
                .expect("dot response");
        assert!(notes.is_empty());
    }
}
