//! **에이전트 내역**의 재료 — 도구 호출 단계 · 메인 대화 기록 읽기 · 백그라운드 작업.
//!
//! 패널(`subagents`)이 카드로 그린다. 여기는 화면을 모른다.
//!
//! # 범위
//!
//! 내역은 **마지막 `/clear` · `/compact` 이후**다. `/compact` 는 대화 id 를 그대로 두고 기록에
//! `{"type":"system","subtype":"compact_boundary","timestamp":…}` 를 남기고(실측 2026-11-04, claude
//! 2.1.311: 한 파일에 4건 · sessionId 1종), `/clear` 는 새 대화 id 로 새 기록을 연다. 그래서 메인
//! 기록은 처음부터 읽되 경계를 만날 때마다 비우고, 대화 id 가 바뀌면 패널이 통째로 다시 찾는다.
//!
//! # 메인 단계는 기록에서 읽는다
//!
//! 탭을 열 때 메인 기록을 한 줄씩 끝까지 읽고(`history` 의 줄 상한 규약), 그 자리부터는 덧붙은 줄만
//! 읽는다. 원문 줄(`assistant.tool_use` · `user.tool_result`)은 다시 읽을 때를 알리기만 한다 —
//! 지난 것과 실시간이 같은 출처라 겹쳐 세지 않고, 경계 줄도 기록에서 그대로 본다.
//!
//! # 메모리
//!
//! 카드마다 단계는 [`STEP_CAP`] 개까지 두고 오래된 것부터 버린다(버린 수는 "N개 생략" 으로 보인다).
//! 단계는 도구 이름과 입력 한 줄 요약만 담는다 — 도구 결과 본문은 담지 않는다. 대화 턴(`memory_turns`)
//! 과는 따로 센다.

use std::collections::VecDeque;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tasty_plugin_agent_common::tail::{TailPoll, TailState};

/// 카드 하나가 들고 있는 단계의 상한.
pub const STEP_CAP: usize = 200;
/// 단계 요약의 상한(글자).
pub const SUMMARY_CAP: usize = 120;
/// 들고 있는 백그라운드 작업의 상한 — 넘치면 끝난 것부터 버린다.
pub const BG_CAP: usize = 50;

/// 도구 호출 하나.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// `tool_use` 의 id — 결과(`tool_result.tool_use_id`)를 이 단계에 붙인다.
    pub id: String,
    /// 부른 시각(유닉스 초) — 기록 줄의 `timestamp`.
    pub at: Option<i64>,
    pub tool: String,
    pub summary: String,
    /// 결과가 왔으면 `is_error` 의 반대. 아직 안 왔거나 결과 줄이 너무 길어 못 풀었으면 `None`.
    pub ok: Option<bool>,
}

/// 카드 하나의 단계 목록 — 상한을 넘으면 오래된 것을 버리고 센다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Steps {
    pub items: VecDeque<Step>,
    /// 상한 때문에 버린 단계 수.
    pub omitted: usize,
}

impl Steps {
    pub fn len(&self) -> usize {
        self.items.len() + self.omitted
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.omitted = 0;
    }

    fn push(&mut self, s: Step) {
        self.items.push_back(s);
        while self.items.len() > STEP_CAP {
            self.items.pop_front();
            self.omitted += 1;
        }
    }

    /// 결과를 그 단계에 붙인다 — 버린 단계의 결과면 아무것도 안 한다.
    fn settle(&mut self, id: &str, ok: bool) {
        if let Some(s) = self.items.iter_mut().rev().find(|s| s.id == id) {
            s.ok = Some(ok);
        }
    }

    /// 기록 줄 하나(`assistant` 의 `tool_use` · `user` 의 `tool_result`)를 반영한다. 바뀌었으면 `true`.
    pub fn apply(&mut self, v: &Value) -> bool {
        let at = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::adapters::driven::store::budget::epoch);
        let blocks = v.pointer("/message/content").and_then(Value::as_array);
        let mut changed = false;
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for b in blocks.into_iter().flatten() {
                    if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let tool = b
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    self.push(Step {
                        id: b
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        at,
                        summary: summary(&tool, b.get("input").unwrap_or(&Value::Null)),
                        tool,
                        ok: None,
                    });
                    changed = true;
                }
            }
            Some("user") => {
                for b in blocks.into_iter().flatten() {
                    if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    if let Some(id) = b.get("tool_use_id").and_then(Value::as_str) {
                        let ok = b.get("is_error").and_then(Value::as_bool) != Some(true);
                        let before = self.items.clone();
                        self.settle(id, ok);
                        changed |= before != self.items;
                    }
                }
            }
            _ => {}
        }
        changed
    }
}

/// 도구 입력의 한 줄 요약 — 도구마다 사람이 알아볼 칸 하나를 고르고, 모르는 도구는 첫 글자 칸이다.
pub fn summary(tool: &str, input: &Value) -> String {
    let field = |k: &str| input.get(k).and_then(Value::as_str);
    let picked = match tool {
        "Bash" | "Monitor" => field("command").or_else(|| field("description")),
        "Read" | "Edit" | "Write" | "MultiEdit" => field("file_path"),
        "NotebookEdit" => field("notebook_path"),
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Agent" | "Task" => field("description").or_else(|| field("subagent_type")),
        "Skill" => field("skill").or_else(|| field("command")),
        _ => None,
    };
    let text = picked.map(str::to_string).or_else(|| {
        input
            .as_object()?
            .values()
            .find_map(Value::as_str)
            .map(str::to_string)
    });
    one_line(text.as_deref().unwrap_or_default())
}

/// 공백을 하나로 접고 [`SUMMARY_CAP`] 에서 자른다.
fn one_line(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SUMMARY_CAP {
        return flat;
    }
    let mut cut: String = flat.chars().take(SUMMARY_CAP).collect();
    cut.push('…');
    cut
}

/// 줄의 `timestamp`(유닉스 초).
fn at_of(v: &Value) -> Option<i64> {
    v.get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::adapters::driven::store::budget::epoch)
}

/// 메인 대화 기록 — 마지막 경계 이후의 단계.
#[derive(Clone, Debug)]
pub struct MainLog {
    path: PathBuf,
    tail: TailState,
    pub steps: Steps,
    /// 마지막 `compact_boundary` 의 시각 — 그보다 먼저 시작한 하위 에이전트는 내역 밖이다.
    pub boundary: Option<i64>,
    /// 너무 길어 풀지 않은 줄 수(도구 결과 본문 등).
    pub skipped: usize,
}

impl MainLog {
    /// 기록을 처음부터 한 줄씩 읽어 마지막 경계 이후만 남긴다 — 다음부터는 읽은 자리에서 이어 읽는다.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mut log = Self {
            path: path.to_path_buf(),
            tail: TailState::default(),
            steps: Steps::default(),
            boundary: None,
            skipped: 0,
        };
        let mut r = BufReader::new(std::fs::File::open(path)?);
        let mut buf = Vec::new();
        let mut offset = 0u64;
        while let Some(line) = crate::application::history::read_capped(&mut r, &mut buf)? {
            // 쓰는 도중의 반쪽 줄은 이어 읽기에 맡긴다 — 그 앞까지만 읽은 것으로 센다.
            if !line.ended {
                break;
            }
            offset += line.bytes;
            if !line.fits {
                log.skipped += 1;
                continue;
            }
            if !relevant(&String::from_utf8_lossy(&buf)) {
                continue;
            }
            if let Ok(v) = serde_json::from_slice::<Value>(&buf) {
                log.apply(&v);
            }
        }
        log.tail = TailState::resume_at(path, offset);
        Ok(log)
    }

    /// 덧붙은 줄을 읽는다. 바뀌었으면 `true`.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.tail.poll(&self.path) {
                Ok(TailPoll::Missing) => return changed,
                Ok(TailPoll::Lines { lines, resynced }) => {
                    if resynced {
                        // 다른 파일로 바뀌었다 — 처음부터 다시 쌓는다.
                        self.steps.clear();
                        self.boundary = None;
                        changed = true;
                    }
                    if lines.is_empty() {
                        return changed;
                    }
                    for l in &lines {
                        if !relevant(l) {
                            continue;
                        }
                        if let Ok(v) = serde_json::from_str::<Value>(l) {
                            changed |= self.apply(&v);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "[agent_session] 대화 기록을 이어 읽지 못했다 {:?}: {e}",
                        self.path
                    );
                    return changed;
                }
            }
        }
    }

    fn apply(&mut self, v: &Value) -> bool {
        if v.get("type").and_then(Value::as_str) == Some("system")
            && v.get("subtype").and_then(Value::as_str) == Some("compact_boundary")
        {
            self.steps.clear();
            self.boundary = at_of(v).or(self.boundary);
            return true;
        }
        // 메인 기록에 섞인 하위 에이전트 줄은 그 카드 몫이다.
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true)
            || v.get("parent_tool_use_id").is_some_and(Value::is_string)
        {
            return false;
        }
        self.steps.apply(v)
    }
}

/// 단계 · 경계에 관한 줄인가 — 나머지 줄은 풀지 않는다.
fn relevant(line: &str) -> bool {
    line.contains("\"tool_use") || line.contains("tool_result") || line.contains("compact_boundary")
}

/// 백그라운드 작업 하나.
#[derive(Clone, Debug, PartialEq)]
pub struct BgTask {
    pub id: String,
    /// `task_type` — `local_bash`(Bash · Monitor) 등.
    pub kind: String,
    pub description: String,
    /// 끝났으면 그 상태(`completed` · `failed` …). 도는 중이면 `None`.
    pub ended: Option<String>,
    /// 처음 본 시각(유닉스 초).
    pub at: i64,
}

/// 이 대화의 백그라운드 작업 — 끝난 것도 경계까지 남긴다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BgTasks {
    pub items: Vec<BgTask>,
}

/// 하위 에이전트로 도는 작업 — 하위 에이전트 카드가 이미 보이므로 작업 카드로 겹쳐 싣지 않는다.
const AGENT_TASK: &str = "local_agent";

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn final_status(s: &str) -> bool {
    matches!(
        s,
        "completed" | "failed" | "killed" | "stopped" | "cancelled" | "error"
    )
}

impl BgTasks {
    #[cfg(test)]
    fn running(&self) -> usize {
        self.items.iter().filter(|t| t.ended.is_none()).count()
    }

    /// 경계를 지났다 — 끝난 것은 지난 범위다. 도는 것은 남긴다.
    pub fn forget_ended(&mut self) {
        self.items.retain(|t| t.ended.is_none());
    }

    fn upsert(&mut self, id: &str, kind: &str, description: &str) -> bool {
        if kind == AGENT_TASK {
            return false;
        }
        if let Some(t) = self.items.iter_mut().find(|t| t.id == id) {
            let before = t.clone();
            if !kind.is_empty() {
                t.kind = kind.to_string();
            }
            if !description.is_empty() {
                t.description = description.to_string();
            }
            t.ended = None;
            return before != *t;
        }
        self.items.push(BgTask {
            id: id.to_string(),
            kind: kind.to_string(),
            description: description.to_string(),
            ended: None,
            at: now(),
        });
        while self.items.len() > BG_CAP {
            match self.items.iter().position(|t| t.ended.is_some()) {
                Some(i) => self.items.remove(i),
                None => self.items.remove(0),
            };
        }
        true
    }

    fn end(&mut self, id: &str, status: &str) -> bool {
        match self.items.iter_mut().find(|t| t.id == id) {
            Some(t) if t.ended.as_deref() != Some(status) => {
                t.ended = Some(status.to_string());
                true
            }
            _ => false,
        }
    }

    /// 데몬 세션 행의 `background_tasks`(`[{id, type, description}]`) — 탭을 열 때 도는 작업을 싣는다.
    pub fn seed(&mut self, rows: &[Value]) {
        for r in rows {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default();
            if !s("id").is_empty() {
                self.upsert(s("id"), s("type"), s("description"));
            }
        }
    }

    /// 원문 줄 하나 — `system/task_*` · `background_tasks_changed` 만 본다. 바뀌었으면 `true`.
    pub fn apply_raw(&mut self, line: &str) -> bool {
        if !(line.contains("task_") || line.contains("background_tasks_changed")) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        if v.get("type").and_then(Value::as_str) != Some("system") {
            return false;
        }
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
        match s("subtype") {
            "task_started" if !s("task_id").is_empty() => {
                self.upsert(s("task_id"), s("task_type"), s("description"))
            }
            "task_updated" => match v.pointer("/patch/status").and_then(Value::as_str) {
                Some(st) if final_status(st) => self.end(s("task_id"), st),
                _ => false,
            },
            "task_notification" if !s("task_id").is_empty() => {
                let st = match s("status") {
                    "" => "completed",
                    other => other,
                };
                self.end(s("task_id"), st)
            }
            // 살아 있는 작업 전체 — 빠진 것은 끝났다(끝 상태는 알림이 따로 고친다).
            "background_tasks_changed" => {
                let live: Vec<&Value> = v
                    .get("tasks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .collect();
                let mut changed = false;
                for t in &live {
                    let f = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default();
                    if !f("task_id").is_empty() {
                        changed |= self.upsert(f("task_id"), f("task_type"), f("description"));
                    }
                }
                let gone: Vec<String> = self
                    .items
                    .iter()
                    .filter(|t| t.ended.is_none())
                    .filter(|t| {
                        !live
                            .iter()
                            .any(|l| l.get("task_id").and_then(Value::as_str) == Some(&t.id))
                    })
                    .map(|t| t.id.clone())
                    .collect();
                for id in gone {
                    changed |= self.end(&id, "completed");
                }
                changed
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sup-alog-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tool_use(id: &str, name: &str, input: Value, ts: &str) -> String {
        format!(
            "{}\n",
            json!({"type":"assistant","timestamp":ts,"message":{"content":[
                {"type":"text","text":"보는 중"},
                {"type":"tool_use","id":id,"name":name,"input":input}]}})
        )
    }

    fn tool_result(id: &str, is_error: bool) -> String {
        format!(
            "{}\n",
            json!({"type":"user","message":{"content":[
                {"type":"tool_result","tool_use_id":id,"is_error":is_error,"content":"결과"}]}})
        )
    }

    fn boundary(ts: &str) -> String {
        format!(
            "{}\n",
            json!({"type":"system","subtype":"compact_boundary","timestamp":ts})
        )
    }

    /// 경계 앞의 단계는 버리고 뒤의 것만 남긴다 — 경계가 여럿이면 마지막 것이 기준이다.
    #[test]
    fn only_the_steps_after_the_last_compact_boundary_are_kept() {
        let dir = scratch();
        let f = dir.join("c.jsonl");
        let body = [
            tool_use(
                "t1",
                "Bash",
                json!({"command":"ls"}),
                "2026-11-04T00:00:00Z",
            ),
            tool_result("t1", false),
            boundary("2026-11-04T00:01:00Z"),
            tool_use(
                "t2",
                "Read",
                json!({"file_path":"/a"}),
                "2026-11-04T00:02:00Z",
            ),
            boundary("2026-11-04T00:03:00Z"),
            tool_use(
                "t3",
                "Grep",
                json!({"pattern":"fn main"}),
                "2026-11-04T00:04:00Z",
            ),
            tool_result("t3", true),
            tool_use(
                "t4",
                "Edit",
                json!({"file_path":"/b"}),
                "2026-11-04T00:05:00Z",
            ),
        ]
        .concat();
        std::fs::write(&f, body).unwrap();
        let log = MainLog::open(&f).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let ids: Vec<&str> = log.steps.items.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["t3", "t4"],
            "경계 앞 단계가 남았거나 뒤 단계를 잃었다"
        );
        assert_eq!(
            log.boundary,
            crate::adapters::driven::store::budget::epoch("2026-11-04T00:03:00Z")
        );
        assert_eq!(
            log.steps.items[0].ok,
            Some(false),
            "is_error 를 실패로 안 읽었다"
        );
        assert_eq!(log.steps.items[0].summary, "fn main");
        assert_eq!(
            log.steps.items[1].ok, None,
            "결과 없는 단계를 끝났다고 했다"
        );
    }

    /// 연 뒤 덧붙은 줄만 읽는다 — 반쪽 줄은 다음에 잇고, 새 경계는 그 자리에서 비운다.
    #[test]
    fn appended_lines_extend_the_log_and_a_new_boundary_empties_it() {
        let dir = scratch();
        let f = dir.join("c.jsonl");
        let first = tool_use(
            "t1",
            "Bash",
            json!({"command":"cargo test"}),
            "2026-11-04T00:00:00Z",
        );
        let second = tool_result("t1", false);
        let (head, rest) = second.split_at(12);
        std::fs::write(&f, format!("{first}{head}")).unwrap();
        let mut log = MainLog::open(&f).unwrap();
        assert_eq!(log.steps.len(), 1);
        assert_eq!(log.steps.items[0].ok, None);
        let append = |s: &str| {
            let mut w = std::fs::OpenOptions::new().append(true).open(&f).unwrap();
            std::io::Write::write_all(&mut w, s.as_bytes()).unwrap();
        };
        append(rest);
        assert!(log.poll(), "덧붙은 결과를 못 읽었다");
        assert_eq!(
            log.steps.items[0].ok,
            Some(true),
            "반쪽 줄을 이어 읽지 못했다"
        );
        assert!(!log.poll(), "읽은 줄을 다시 읽었다");
        append(&boundary("2026-11-04T00:09:00Z"));
        append(&tool_use(
            "t2",
            "Read",
            json!({"file_path":"/x"}),
            "2026-11-04T00:10:00Z",
        ));
        assert!(log.poll());
        std::fs::remove_dir_all(&dir).unwrap();
        let ids: Vec<&str> = log.steps.items.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["t2"], "새 경계가 앞 단계를 안 비웠다");
        assert_eq!(
            log.boundary,
            crate::adapters::driven::store::budget::epoch("2026-11-04T00:09:00Z")
        );
    }

    /// 상한을 넘으면 오래된 것부터 버리고 버린 수를 센다 — 버린 단계의 결과는 남은 단계를 안 건드린다.
    #[test]
    fn the_step_cap_drops_the_oldest_and_counts_them() {
        let mut s = Steps::default();
        for i in 0..STEP_CAP + 7 {
            let v: Value = serde_json::from_str(&tool_use(
                &format!("t{i}"),
                "Bash",
                json!({"command": format!("echo {i}")}),
                "2026-11-04T00:00:00Z",
            ))
            .unwrap();
            s.apply(&v);
        }
        assert_eq!(s.items.len(), STEP_CAP);
        assert_eq!(s.omitted, 7);
        assert_eq!(s.len(), STEP_CAP + 7);
        assert_eq!(s.items[0].id, "t7", "오래된 것부터 버리지 않았다");
        let v: Value = serde_json::from_str(&tool_result("t0", true)).unwrap();
        assert!(!s.apply(&v), "버린 단계의 결과가 무언가를 바꿨다");
        assert!(s.items.iter().all(|x| x.ok.is_none()));
    }

    #[test]
    fn a_summary_picks_the_field_a_person_reads_and_stays_one_capped_line() {
        assert_eq!(
            summary("Bash", &json!({"command":"ls  -la\n| head"})),
            "ls -la | head"
        );
        assert_eq!(
            summary("Read", &json!({"file_path":"/w/a.rs","limit":5})),
            "/w/a.rs"
        );
        assert_eq!(
            summary("Grep", &json!({"pattern":"fn helper","path":"/w"})),
            "fn helper"
        );
        assert_eq!(
            summary("Agent", &json!({"description":"찾기","prompt":"긴 지시"})),
            "찾기"
        );
        assert_eq!(summary("Mystery", &json!({"n":3,"q":"뭔가"})), "뭔가");
        assert_eq!(summary("TodoWrite", &json!({"todos":[]})), "");
        let long = summary("Bash", &json!({"command": "가".repeat(SUMMARY_CAP + 10)}));
        assert_eq!(long.chars().count(), SUMMARY_CAP + 1);
    }

    /// 하위 에이전트 · 사이드체인 줄은 메인 단계가 아니다.
    #[test]
    fn subagent_lines_in_the_main_log_are_not_main_steps() {
        let dir = scratch();
        let f = dir.join("c.jsonl");
        let side = json!({"type":"assistant","isSidechain":true,"message":{"content":[
            {"type":"tool_use","id":"s1","name":"Read","input":{"file_path":"/s"}}]}});
        std::fs::write(
            &f,
            format!(
                "{side}\n{}",
                tool_use(
                    "m1",
                    "Bash",
                    json!({"command":"ls"}),
                    "2026-11-04T00:00:00Z"
                )
            ),
        )
        .unwrap();
        let log = MainLog::open(&f).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(log.steps.items.len(), 1);
        assert_eq!(log.steps.items[0].id, "m1");
    }

    /// 실측 줄 순서 — 시작 · 전체 목록 · 끝 알림. 끝난 것은 상태와 함께 남고, 하위 에이전트 작업은 안 싣는다.
    #[test]
    fn background_tasks_keep_the_ended_ones_with_their_status() {
        let mut b = BgTasks::default();
        let line = |v: Value| v.to_string();
        assert!(
            b.apply_raw(&line(json!({"type":"system","subtype":"task_started",
            "task_id":"b1","task_type":"local_bash","description":"dev server"})))
        );
        assert!(
            !b.apply_raw(&line(json!({"type":"system","subtype":"task_started",
            "task_id":"a1","task_type":"local_agent","description":"agent"})))
        );
        assert!(b.apply_raw(&line(
            json!({"type":"system","subtype":"background_tasks_changed",
            "tasks":[{"task_id":"b1","task_type":"local_bash","description":"dev server"},
                     {"task_id":"m1","task_type":"local_bash","description":"watch"}]})
        )));
        assert_eq!((b.items.len(), b.running()), (2, 2));
        assert!(
            b.apply_raw(&line(json!({"type":"system","subtype":"task_notification",
            "task_id":"m1","status":"failed"})))
        );
        assert!(b.apply_raw(&line(
            json!({"type":"system","subtype":"background_tasks_changed",
            "tasks":[]})
        )));
        assert_eq!(b.running(), 0);
        assert_eq!(b.items[0].ended.as_deref(), Some("completed"));
        assert_eq!(
            b.items[1].ended.as_deref(),
            Some("failed"),
            "알림의 상태를 덮어썼다"
        );
        assert!(!b.apply_raw(r#"{"type":"assistant","message":{"content":"task_started"}}"#));
        b.forget_ended();
        assert!(b.items.is_empty());
        b.seed(&[json!({"id":"b9","type":"local_bash","description":"from status"})]);
        assert_eq!(b.items[0].description, "from status");
    }
}
