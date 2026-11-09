//! `task-graph`·`dag-get` 의 기본 출력. 호스트의 JSON `nodes`·`edges` 로 부모 ID 표를 만들고,
//! 순환과 runner 상태는 `--format dot` 처럼 stderr 에 쓴다.

use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;

use super::parent_table::{self, IdOrder, TableRow};
use crate::out::{errln, out};

pub(crate) use super::parent_table::TABLE_FORMAT;

/// 호스트에 보낼 형식. 호스트에는 `json` 으로 요청해 호스트 버전과 무관하게 같은 표를 낸다.
pub(crate) fn ipc_format(format: &str) -> &str {
    if format == TABLE_FORMAT {
        "json"
    } else {
        format
    }
}

/// JSON 그래프 응답이면 표를 만든다. `nodes` 가 없는 응답이면 None.
fn table_body(result: &Value) -> Option<String> {
    let nodes = result.get("nodes")?.as_array()?;
    let mut parents: HashMap<&str, Vec<String>> = HashMap::new();
    let edges = result.get("edges").and_then(Value::as_array);
    for edge in edges.into_iter().flatten() {
        let from = edge.get("from").and_then(Value::as_str);
        let to = edge.get("to").and_then(Value::as_str);
        if let (Some(from), Some(to)) = (from, to) {
            parents.entry(to).or_default().push(from.to_string());
        }
    }
    let text = |node: &Value, key: &str| node.get(key).and_then(Value::as_str).map(str::to_string);
    let rows = nodes
        .iter()
        .map(|node| {
            let id = text(node, "id").unwrap_or_default();
            TableRow {
                parents: parents.remove(id.as_str()).unwrap_or_default(),
                cells: vec![text(node, "name"), Some(state_label(node))],
                id,
            }
        })
        .collect();
    Some(parent_table::render(
        &["name", "state"],
        rows,
        IdOrder::Text,
    ))
}

/// 갈래에서 고르지 않아 건너뛴 task 는 실패 전파로 건너뛴 task 와 구분한다. DOT 라벨과 같은 문구다.
fn state_label(node: &Value) -> String {
    let skip_reason = node.pointer("/skip/reason").and_then(Value::as_str);
    let not_selected = skip_reason == Some("branch_not_selected");
    if not_selected {
        return "not selected".to_string();
    }
    node.get("state")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// JSON 그래프 응답이면 나눠 쓰고 true, 아니면 아무것도 쓰지 않고 false.
pub(super) fn write_table_response(result: &Value) -> Result<bool> {
    let Some(body) = table_body(result) else {
        return Ok(false);
    };
    out!("{body}")?;
    for note in super::graph_dot::graph_notes(result) {
        errln!("{note}");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn edges_become_parents_and_each_source_is_written_once() {
        let body = table_body(&json!({
            "format": "json",
            "nodes": [
                {"id": "t-c", "name": "merge", "state": "waiting", "skip": null},
                {"id": "t-a", "name": "one", "state": "succeeded", "skip": null},
                {"id": "t-b", "name": "two", "state": "ready", "skip": null}
            ],
            "edges": [
                {"from": "t-b", "to": "t-c", "kind": "depends_on"},
                {"from": "t-a", "to": "t-c", "kind": "depends_on"},
                {"from": "t-a", "to": "t-c", "kind": "binding"}
            ],
            "cycle": null
        }))
        .expect("graph response");
        assert_eq!(
            body,
            "| id | parents | name | state |\n\
             |---|---|---|---|\n\
             | t-a | - | one | succeeded |\n\
             | t-c | t-a,t-b | merge | waiting |\n\
             | t-b | - | two | ready |\n"
        );
    }

    #[test]
    fn a_task_on_an_unselected_branch_reads_not_selected() {
        let body = table_body(&json!({
            "nodes": [
                {"id": "a", "name": "n", "state": "skipped", "skip": {"reason": "branch_not_selected"}},
                {"id": "b", "name": "n", "state": "skipped", "skip": {"reason": "upstream_unavailable", "source": "x", "source_state": "failed"}}
            ],
            "edges": []
        }))
        .expect("graph response");
        assert!(body.contains("| a | - | n | not selected |\n"), "{body}");
        assert!(body.contains("| b | - | n | skipped |\n"), "{body}");
    }

    #[test]
    fn a_parent_outside_the_dag_stays_in_the_cell() {
        let body = table_body(&json!({
            "nodes": [{"id": "a", "name": "n", "state": "ready"}],
            "edges": [{"from": "other-group", "to": "a", "kind": "depends_on"}]
        }))
        .expect("graph response");
        assert!(body.contains("| a | other-group | n | ready |\n"), "{body}");
    }

    #[test]
    fn an_empty_graph_is_the_two_header_lines() {
        assert_eq!(
            table_body(&json!({"nodes": [], "edges": []})).as_deref(),
            Some("| id | parents | name | state |\n|---|---|---|---|\n")
        );
    }

    #[test]
    fn a_response_without_nodes_is_left_to_the_json_printer() {
        assert!(table_body(&json!({"format": "dot", "dot": "digraph G {}"})).is_none());
    }

    #[test]
    fn only_the_table_format_is_rewritten_for_the_host() {
        assert_eq!(ipc_format("table"), "json");
        assert_eq!(ipc_format("json"), "json");
        assert_eq!(ipc_format("dot"), "dot");
    }
}
