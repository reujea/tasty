//! `terminal|claude|codex children` 의 기본 출력. 세 응답 모양(`{"children"}`·bare 배열,
//! `surface_id`·`child_surface_id`)에서 같은 부모 ID 표를 만든다. 호스트에는 직계 자식 조회만 있어
//! 표는 한 단계이고 `parents` 는 모든 행에서 같다.

use anyhow::{Result, anyhow};
use serde_json::{Map, Value};

pub(crate) use super::parent_table::TABLE_FORMAT;
use super::parent_table::{self, IdOrder, TableRow};
use crate::out::out;

const COLUMNS: [&str; 7] = [
    "nickname",
    "state",
    "cwd",
    "last_report_at",
    "index",
    "confidence",
    "evidence",
];

/// 매니페스트의 문자열 인자는 값을 제한하지 못하므로 요청을 보내기 전에 CLI 가 확인한다.
pub(crate) fn wants_table(params: &Value) -> Result<bool> {
    match params.get("format").and_then(Value::as_str) {
        None | Some(TABLE_FORMAT) => Ok(true),
        Some("json") => Ok(false),
        Some(other) => Err(anyhow!(
            "{}",
            tasty_i18n::t_fmt("cli.plugin_cli.invalid_children_format", other)
        )),
    }
}

/// 자식 목록 응답이면 표를 만든다. 목록이 아니면 None.
fn table_body(result: &Value) -> Option<String> {
    let entries = match result {
        Value::Array(entries) => entries,
        other => other.get("children")?.as_array()?,
    };
    let rows = entries
        .iter()
        .filter_map(Value::as_object)
        .map(child_row)
        .collect();
    Some(parent_table::render(&COLUMNS, rows, IdOrder::Numeric))
}

fn child_row(child: &Map<String, Value>) -> TableRow {
    let scalar = |key: &str| child.get(key).and_then(scalar_text);
    TableRow {
        id: scalar("child_surface_id")
            .or_else(|| scalar("surface_id"))
            .unwrap_or_default(),
        parents: scalar("parent").into_iter().collect(),
        cells: vec![
            scalar("nickname"),
            scalar("state"),
            scalar("cwd"),
            child
                .get("last_report_at")
                .and_then(Value::as_u64)
                .and_then(report_time),
            scalar("index"),
            scalar("confidence"),
            scalar("evidence"),
        ],
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// 밀리초 보고 시각을 초 단위 UTC 로 내린다.
fn report_time(millis: u64) -> Option<String> {
    let secs = i64::try_from(millis / 1000).ok()?;
    Some(crate::hook_failure::utc_from_secs(secs))
}

/// 자식 목록 응답이면 표를 쓰고 true, 아니면 아무것도 쓰지 않고 false.
pub(crate) fn write_table(result: &Value) -> Result<bool> {
    let Some(body) = table_body(result) else {
        return Ok(false);
    };
    out!("{body}")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const HEADER: &str = "| id | parents | nickname | state | cwd | last_report_at | index | confidence | evidence |\n\
                          |---|---|---|---|---|---|---|---|---|\n";

    fn fields() -> Value {
        json!({
            "index": 2, "nickname": "lane", "state": "idle", "cwd": "/w",
            "parent": 7, "last_report_at": 1_767_613_212_999_u64,
            "confidence": "reported", "evidence": "hook_idle", "role": "ignored"
        })
    }

    #[test]
    fn the_three_response_shapes_print_the_same_table() {
        let with_id = |key: &str| {
            let mut child = fields();
            child[key] = json!(10);
            child
        };
        let expected = format!(
            "{HEADER}| 10 | 7 | lane | idle | /w | 2026-01-05T11:40:12Z | 2 | reported | hook_idle |\n"
        );
        let terminal_or_codex = json!({ "children": [with_id("surface_id")] });
        let claude = json!([with_id("child_surface_id")]);
        assert_eq!(table_body(&terminal_or_codex).as_deref(), Some(&*expected));
        assert_eq!(table_body(&claude).as_deref(), Some(&*expected));
    }

    #[test]
    fn missing_null_and_old_host_values_read_as_no_value() {
        let body = table_body(&json!({ "children": [{
            "surface_id": 3, "index": 0, "state": "active", "nickname": null, "cwd": null,
            "parent": null, "last_report_at": null,
        }, {
            "surface_id": 4, "index": 1, "state": "idle",
        }] }))
        .expect("list");
        assert_eq!(
            body,
            format!(
                "{HEADER}| 3 | - | - | active | - | - | 0 | - | - |\n| 4 | - | - | idle | - | - | 1 | - | - |\n"
            )
        );
    }

    #[test]
    fn an_external_parent_is_its_key_and_rows_sort_as_numbers() {
        let child = |id: u32| json!({ "surface_id": id, "parent": "codex:t1", "index": 0 });
        let body = table_body(&json!({ "children": [child(10), child(9)] })).expect("list");
        let lines: Vec<&str> = body.lines().skip(2).collect();
        assert!(lines[0].starts_with("| 9 | codex:t1 |"), "{body}");
        assert!(lines[1].starts_with("| 10 | codex:t1 |"), "{body}");
    }

    #[test]
    fn no_children_is_the_two_header_lines() {
        assert_eq!(table_body(&json!([])).as_deref(), Some(HEADER));
        assert_eq!(
            table_body(&json!({ "children": [] })).as_deref(),
            Some(HEADER)
        );
    }

    #[test]
    fn a_response_that_is_not_a_list_is_left_to_the_json_printer() {
        assert!(table_body(&json!({ "error": "x" })).is_none());
    }

    #[test]
    fn report_time_rounds_down_to_the_second() {
        assert_eq!(
            report_time(1_767_613_200_999).as_deref(),
            Some("2026-01-05T11:40:00Z")
        );
    }

    #[test]
    fn only_table_and_json_are_accepted_and_table_is_the_default() {
        assert!(wants_table(&json!({})).unwrap());
        assert!(wants_table(&json!({ "format": "table" })).unwrap());
        assert!(!wants_table(&json!({ "format": "json" })).unwrap());
        assert!(wants_table(&json!({ "format": "dot" })).is_err());
    }
}
