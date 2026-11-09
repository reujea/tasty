//! 턴 밖에서 도는 작업 — 백그라운드 Bash · Monitor · 백그라운드 하위 에이전트.
//!
//! 턴이 끝나도(`result`) 이 작업들은 살아 있고, 끝나면 claude 가 스스로 턴을 연다. 자식을
//! 내리면(회수·plugin 정지·작업 폴더 변경) 작업도 함께 죽는다. 그래서 "세션이 바쁜가" 는 턴만이
//! 아니라 이 목록도 본다.
//!
//! claude 2.1.308 실측(`-p --output-format stream-json`): 작업마다
//!
//! | 줄 | 뜻 |
//! |---|---|
//! | `system/background_tasks_changed` `tasks:[…]` | 지금 살아 있는 작업 **전체**. 다 끝나면 `[]` |
//! | `system/task_started` `task_id` `task_type` `is_backgrounded` | 작업 하나가 섰다 |
//! | `system/task_updated` `patch.status` | 상태가 바뀌었다(`completed` 등) |
//! | `system/task_notification` `status` | 작업이 끝났다는 알림 — 뒤이어 자가 턴이 온다 |
//!
//! `task_type` 은 `local_bash`(Bash `run_in_background` · Monitor) · `local_agent`(하위 에이전트)
//! 등이다. 전체 목록이 오면 그것이 정본이고, 그 줄을 안 내는 claude 는 시작·끝 줄로 센다.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// 살아 있는 작업 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub task_type: String,
    pub description: String,
}

/// 세션의 작업 목록.
#[derive(Debug, Default)]
pub struct Tasks {
    live: BTreeMap<String, Task>,
}

/// 끝난 상태 — 이 값으로 바뀐 작업은 목록에서 뺀다.
fn is_final(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "killed" | "stopped" | "cancelled" | "error"
    )
}

fn task_of(v: &Value) -> Option<(String, Task)> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
    let id = s("task_id");
    if id.is_empty() {
        return None;
    }
    Some((
        id.to_string(),
        Task {
            task_type: s("task_type").to_string(),
            description: s("description").to_string(),
        },
    ))
}

impl Tasks {
    /// `system` 줄 하나를 반영한다 — 목록이 바뀌었으면 `true`.
    pub fn apply(&mut self, v: &Value) -> bool {
        let before = self.live.clone();
        match v.get("subtype").and_then(Value::as_str) {
            Some("background_tasks_changed") => {
                self.live = v
                    .get("tasks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(task_of)
                    .collect();
            }
            Some("task_started") => {
                if let Some((id, t)) = task_of(v) {
                    self.live.insert(id, t);
                }
            }
            Some("task_updated") => {
                let done = v
                    .pointer("/patch/status")
                    .and_then(Value::as_str)
                    .is_some_and(is_final);
                if done && let Some(id) = v.get("task_id").and_then(Value::as_str) {
                    self.live.remove(id);
                }
            }
            Some("task_notification") => {
                if let Some(id) = v.get("task_id").and_then(Value::as_str) {
                    self.live.remove(id);
                }
            }
            _ => {}
        }
        self.live != before
    }

    /// 자식이 내려갔다 — 작업도 함께 죽었다.
    pub fn clear(&mut self) {
        self.live.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// 세션 행에 싣는 모양.
    pub fn to_json(&self) -> Value {
        Value::Array(
            self.live
                .iter()
                .map(|(id, t)| {
                    json!({ "id": id, "type": t.task_type, "description": t.description })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(id: &str, ty: &str) -> Value {
        json!({"type":"system","subtype":"task_started","task_id":id,"task_type":ty,
               "description":"d","is_backgrounded":true})
    }

    /// 실측 줄 순서 그대로 — Bash 와 하위 에이전트를 함께 걸고 하나씩 끝난다.
    #[test]
    fn the_snapshot_and_the_start_and_end_lines_keep_the_live_set() {
        let mut t = Tasks::default();
        assert!(t.apply(
            &json!({"type":"system","subtype":"background_tasks_changed",
            "tasks":[{"task_id":"b1","task_type":"local_bash","description":"d"}]})
        ));
        assert!(!t.apply(&started("b1", "local_bash")), "이미 있는 작업");
        t.apply(&started("a1", "local_agent"));
        assert_eq!(t.to_json().as_array().unwrap().len(), 2);
        t.apply(
            &json!({"type":"system","subtype":"task_updated","task_id":"a1",
            "patch":{"status":"completed"}}),
        );
        assert_eq!(t.to_json()[0]["id"], "b1");
        t.apply(
            &json!({"type":"system","subtype":"task_notification","task_id":"b1",
            "status":"completed"}),
        );
        assert!(t.is_empty());
    }

    /// 전체 목록이 오면 시작·끝 줄로 센 것보다 그것을 믿는다.
    #[test]
    fn an_empty_snapshot_clears_what_the_start_lines_left() {
        let mut t = Tasks::default();
        t.apply(&started("m1", "local_bash"));
        t.apply(&json!({"type":"system","subtype":"background_tasks_changed","tasks":[]}));
        assert!(t.is_empty());
    }

    /// 진행 중 갱신(`running`)은 작업을 빼지 않는다.
    #[test]
    fn a_running_update_keeps_the_task() {
        let mut t = Tasks::default();
        t.apply(&started("b1", "local_bash"));
        assert!(!t.apply(
            &json!({"type":"system","subtype":"task_updated","task_id":"b1",
            "patch":{"status":"running"}})
        ));
        assert!(!t.is_empty());
    }
}
