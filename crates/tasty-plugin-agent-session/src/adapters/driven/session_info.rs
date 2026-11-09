//! 대화 탭 상태줄의 **세션 이름 · 프로세스** 칸.
//!
//! # 세션 이름은 어디서 오나
//!
//! supervisor 기록에는 이름이 없다 — 데몬은 `spawn --name` 을 claude 인자(`--name`)로만 넘기고
//! 기록에 남기지 않는다(실측 2026-10-21: `status`·`list` 행에 칸이 없다). 이름의 정본은 claude
//! 가 대화 기록(jsonl)에 적는 두 줄이다(실측 2026-10-21, Claude Code 2.1.304):
//!
//! | 줄 | 칸 |
//! |---|---|
//! | `{"type":"custom-title","customTitle":…}` | 사용자가 붙인 이름 — 이것이 이긴다 |
//! | `{"type":"agent-name","agentName":…}` | `--name` 으로 뜬 이름 |
//!
//! 이름을 바꾸면 줄이 다시 붙으므로 **마지막 줄**이 지금 이름이다. 데몬이 행에 `name` 을 실으면
//! 그쪽을 먼저 쓴다(`Convo::display_name`) — 이 판독은 그 칸이 없는 동안의 자리다.
//!
//! 기록이 수십 MB 라 plugin 루프에서 읽지 않는다. 구독 워커가 [`NameWatch`] 를 들고 붙을 때와
//! 턴이 끝날 때 **덧붙은 만큼만** 읽는다(`budget::Ledger` 와 같은 방식).
//!
//! # 프로세스 칸
//!
//! `pid`·`rss_kb`·`started_unix` 는 데몬이 **살아 있다고 판정한 세션의 행에만** 싣는다(데몬
//! consumer-contract · n70). 칸이 없으면 프로세스가 없거나 못 잰 것이라 그리지 않는다 —
//! 옛 값을 남기면 재사용된 pid 를 그 세션으로 읽는다.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tasty_plugin_agent_common::tail::{TailPoll, TailState};

/// 폴러가 실어 보낼 세션 행 — 상태줄에 쓰는 칸만.
pub fn rows_json(rows: &[crate::ports::supervisor::SessionRow]) -> Value {
    Value::Array(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "agent": r.agent_name,
                    "cwd": r.cwd(),
                    // 상태줄의 에이전트 세션 id 도 같은 주기의 목록으로 채운다.
                    "agent_session_id": r.agent_session_id(),
                    // 상태줄의 이름 · 프로세스 칸 — 데몬이 실을 때만 있다(`session_info`).
                    "name": r.raw().get("name"),
                    "kind": r.raw().get("kind"),
                    "turn_quiet_secs": r.raw().get("turn_quiet_secs"),
                    "needs_input": r.raw().get("needs_input"),
                    "pid": r.raw().get("pid"),
                    "rss_kb": r.raw().get("rss_kb"),
                    "started_unix": r.raw().get("started_unix"),
                })
            })
            .collect(),
    )
}

/// 데몬 행의 프로세스 칸. 셋 다 없으면 만들지 않는다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Proc {
    pub pid: Option<u64>,
    pub rss_kb: Option<u64>,
    pub started_unix: Option<i64>,
}

/// 목록 행([`rows_json`] 이 옮긴 것)에서 프로세스 칸을 꺼낸다.
pub fn proc_of_row(row: &Value) -> Option<Proc> {
    let p = Proc {
        pid: row.get("pid").and_then(Value::as_u64),
        rss_kb: row.get("rss_kb").and_then(Value::as_u64),
        started_unix: row.get("started_unix").and_then(Value::as_i64),
    };
    (p != Proc::default()).then_some(p)
}

/// 목록 행의 세션 이름 — 데몬이 그 칸을 낼 때만.
pub fn name_of_row(row: &Value) -> Option<String> {
    row.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// KB 를 짧게 — `312MB`, `1.2GB`.
pub(crate) fn memory(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1}GB", mb / 1024.0)
    } else {
        format!("{}MB", mb.round() as u64)
    }
}

/// 지난 초를 짧게 — `42s`, `7m`, `2h05m`, `3d4h`. 분 아래는 1분 동안만 보인다.
pub(crate) fn elapsed(secs: i64) -> String {
    let (m, h, d) = (secs / 60, secs / 3600, secs / 86_400);
    if secs < 60 {
        format!("{secs}s")
    } else if h == 0 {
        format!("{m}m")
    } else if d == 0 {
        format!("{h}h{:02}m", m % 60)
    } else {
        format!("{d}d{}h", h % 24)
    }
}

/// 시작 폼의 목록에 이름을 채운다 — 데몬이 이름을 안 낸 행만, 그 agent 프로필의 대화 기록에서.
///
/// 폼은 plugin 루프에서 그려지므로 **파일마다 이어 읽는다**([`cached_name`]). 처음 한 번은
/// 기록 전체를 훑지만(85MB 에 0.1초 남짓), 그 뒤로는 덧붙은 만큼만 읽는다.
pub fn fill_names(
    rows: &mut [crate::domain::convo::Attachable],
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
) {
    for a in rows.iter_mut().filter(|a| a.agent_session_name.is_none()) {
        let Some(id) = a.agent_session_id.as_deref() else {
            continue;
        };
        let Some(dir) = profiles
            .iter()
            .find(|p| p.name == a.agent)
            .and_then(|p| crate::adapters::driven::store::budget::config_dir(&p.env))
        else {
            continue;
        };
        a.agent_session_name = crate::adapters::driven::store::budget::find_transcript(&dir, id)
            .and_then(|p| cached_name(&p));
    }
}

/// 파일별로 읽던 자리를 기억하는 이름 판독 — 폼을 다시 그릴 때 처음부터 읽지 않는다.
fn cached_name(path: &Path) -> Option<String> {
    static CACHE: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, NameWatch>>,
    > = std::sync::LazyLock::new(Default::default);
    // poison 이어도 판독은 계속한다 — 칸 하나가 빌 뿐인 값이다.
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .entry(path.to_path_buf())
        .or_insert_with(|| NameWatch::new(path.to_path_buf()))
        .catch_up()
}

/// 대화 기록 한 파일에서 이름 줄을 덧붙은 만큼씩 읽는다.
#[derive(Debug)]
pub struct NameWatch {
    path: PathBuf,
    reader: TailState,
    custom: Option<String>,
    agent: Option<String>,
    /// 마지막 assistant 호출의 컨텍스트(입력 + 캐시 읽기 + 캐시 쓰기 + 출력) — 원문 `result` 줄을 놓친
    /// 턴도 턴 끝 판정이 이 값을 쓴다(`stream`).
    context: Option<u64>,
}

impl NameWatch {
    pub fn new(path: PathBuf) -> Self {
        NameWatch {
            path,
            reader: TailState::default(),
            custom: None,
            agent: None,
            context: None,
        }
    }

    /// 그 세션의 대화 기록을 찾는다. 기록이 없으면(claude 가 아니거나 첫 턴 전) `None`.
    pub fn resolve(sup: &impl crate::ports::supervisor::Supervisor, session: &str) -> Option<Self> {
        crate::adapters::driven::store::budget::Source::resolve(sup, session)?
            .transcript
            .map(Self::new)
    }

    /// 덧붙은 줄을 읽고 지금 이름을 낸다. 파일이 줄었으면(교체) 처음부터 다시 읽는다.
    ///
    /// 끝이 줄바꿈으로 안 끝난 꼬리는 읽지 않고 다음으로 미룬다 — 쓰는 중인 줄을 반만 읽으면
    /// JSON 이 아니다.
    pub fn catch_up(&mut self) -> Option<String> {
        if let Err(e) = self.read_new() {
            tracing::debug!("대화 기록을 못 읽었다({}): {e}", self.path.display());
        }
        self.custom.clone().or_else(|| self.agent.clone())
    }

    /// 지금까지 읽은 마지막 assistant 호출의 컨텍스트 — [`Self::catch_up`] 뒤에 읽는다.
    pub fn context(&self) -> Option<u64> {
        self.context
    }

    fn read_new(&mut self) -> std::io::Result<()> {
        // 한 번에 읽는 양에 상한이 있어, 새 줄이 더 없을 때까지 이어 읽는다.
        loop {
            match self.reader.poll(&self.path)? {
                TailPoll::Missing => return Ok(()),
                TailPoll::Lines { lines, resynced } => {
                    if resynced {
                        self.custom = None;
                        self.agent = None;
                        self.context = None;
                    }
                    if lines.is_empty() {
                        return Ok(());
                    }
                    for line in &lines {
                        self.note(line.as_bytes());
                    }
                }
            }
        }
    }

    fn note_usage(&mut self, line: &[u8]) {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let Some(u) = v.pointer("/message/usage") else {
            return;
        };
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        let total = n("input_tokens")
            + n("cache_read_input_tokens")
            + n("cache_creation_input_tokens")
            + n("output_tokens");
        if total > 0 {
            self.context = Some(total);
        }
    }

    fn note(&mut self, line: &[u8]) {
        // 대부분의 줄은 대화다 — 표지가 없으면 JSON 으로 풀지 않는다.
        let has = |pat: &[u8]| line.windows(pat.len()).any(|w| w == pat);
        if has(b"\"type\":\"assistant\"") && has(b"\"usage\"") {
            self.note_usage(line);
            return;
        }
        if !has(b"custom-title") && !has(b"agent-name") {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let text = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => {
                if let Some(t) = text("customTitle") {
                    self.custom = Some(t);
                }
            }
            Some("agent-name") => {
                if let Some(t) = text("agentName") {
                    self.agent = Some(t);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    /// 시험마다 다른 파일 — 지우지 않는다(다른 시험들과 같다 · 임시 폴더 아래).
    fn transcript(lines: &[&str]) -> ((), PathBuf) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sup-name-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("임시 폴더");
        let path = dir.join("t.jsonl");
        let mut f = std::fs::File::create(&path).expect("파일");
        for l in lines {
            writeln!(f, "{l}").expect("쓰기");
        }
        ((), path)
    }

    // 실측 줄(2026-10-21, 2.1.304) 모양 그대로.
    const AGENT: &str =
        r#"{"type":"agent-name","agentName":"agent-sample-18","sessionId":"0da10db9"}"#;
    const CUSTOM: &str =
        r#"{"type":"custom-title","customTitle":"renamed","sessionId":"0da10db9"}"#;
    const TALK: &str =
        r#"{"type":"user","message":{"content":"agent-name 이라는 말이 대화에 섞여도"}}"#;

    /// 마지막 assistant 호출의 컨텍스트를 기억한다 — 원문 `result` 줄을 놓친 턴의 판정이 이 값을 쓴다.
    /// 출력 토큰까지 더한다(`usage::apply_result` 와 같은 정의).
    #[test]
    fn the_last_assistant_call_sets_the_context() {
        let a = |inp: u64, read: u64| {
            format!(
                r#"{{"type":"assistant","message":{{"id":"m","usage":{{"input_tokens":{inp},"cache_read_input_tokens":{read},"cache_creation_input_tokens":10,"output_tokens":5}}}}}}"#
            )
        };
        let (l1, l2) = (a(1, 100), a(2, 900_000));
        let (_, path) = transcript(&[&l1, TALK, &l2]);
        let mut w = NameWatch::new(path);
        assert_eq!(w.context(), None, "읽기 전에는 없다");
        w.catch_up();
        assert_eq!(w.context(), Some(900_017));
    }

    /// 사용자가 붙인 이름이 `--name` 이름을 이긴다 — 순서와 무관하게.
    #[test]
    fn the_custom_title_wins_over_the_agent_name() {
        let (_d, p) = transcript(&[CUSTOM, TALK, AGENT]);
        assert_eq!(NameWatch::new(p).catch_up().as_deref(), Some("renamed"));
        let (_d, p) = transcript(&[AGENT, TALK]);
        assert_eq!(
            NameWatch::new(p).catch_up().as_deref(),
            Some("agent-sample-18")
        );
    }

    /// 이름을 바꾸면 마지막 줄이 지금 이름이다 — 덧붙은 것만 읽어도 그렇다.
    #[test]
    fn a_rename_appended_later_is_picked_up() {
        let (_d, p) = transcript(&[AGENT]);
        let mut w = NameWatch::new(p.clone());
        assert_eq!(w.catch_up().as_deref(), Some("agent-sample-18"));
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&p)
            .expect("열기");
        writeln!(f, "{CUSTOM}").expect("쓰기");
        assert_eq!(w.catch_up().as_deref(), Some("renamed"));
    }

    /// 기록이 짧은 파일로 바뀌면 옛 파일에서 읽은 이름을 버리고 처음부터 다시 읽는다.
    #[test]
    fn a_replaced_transcript_forgets_the_old_names() {
        let (_d, p) = transcript(&[CUSTOM, TALK]);
        let mut w = NameWatch::new(p.clone());
        assert_eq!(w.catch_up().as_deref(), Some("renamed"));
        std::fs::write(&p, format!("{AGENT}\n")).expect("쓰기");
        assert_eq!(w.catch_up().as_deref(), Some("agent-sample-18"));
    }

    /// 줄바꿈이 아직 안 온 꼬리는 미룬다 — 반쪽 줄을 읽고 지나치면 그 이름을 영영 놓친다.
    #[test]
    fn a_half_written_line_is_read_once_it_is_complete() {
        let (_d, p) = transcript(&[]);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&p)
            .expect("열기");
        write!(f, "{}", &AGENT[..20]).expect("쓰기");
        let mut w = NameWatch::new(p);
        assert_eq!(w.catch_up(), None);
        writeln!(f, "{}", &AGENT[20..]).expect("쓰기");
        assert_eq!(w.catch_up().as_deref(), Some("agent-sample-18"));
    }

    /// 폼의 이름 칸 — 그 agent 프로필 폴더의 대화 기록에서 채운다. 데몬이 낸 이름은 안 덮는다.
    #[test]
    fn the_form_names_come_from_the_profile_transcript() {
        let (_d, t) = transcript(&[AGENT]);
        let config = t.parent().expect("폴더").join("profile");
        let proj = config.join("projects").join("-w-proj");
        std::fs::create_dir_all(&proj).expect("폴더");
        std::fs::copy(&t, proj.join("conv-1.jsonl")).expect("복사");
        let profile = crate::adapters::driven::store::agents::AgentProfile {
            name: "dev".into(),
            kind: "claude".into(),
            cwd: None,
            env: [(
                "CLAUDE_CONFIG_DIR".to_string(),
                config.display().to_string(),
            )]
            .into(),
            ..Default::default()
        };
        let row = |id: &str, name: Option<&str>| crate::domain::convo::Attachable {
            id: "s".into(),
            state: "stopped".into(),
            agent: "dev".into(),
            reclaimed: false,
            cwd: None,
            taken_by: None,
            agent_session_id: Some(id.into()),
            agent_session_name: name.map(str::to_string),
            remaining: Default::default(),
        };
        let mut rows = vec![
            row("conv-1", None),
            row("conv-1", Some("from-daemon")),
            row("nope", None),
        ];
        fill_names(&mut rows, std::slice::from_ref(&profile));
        let names: Vec<_> = rows
            .iter()
            .map(|r| r.agent_session_name.as_deref())
            .collect();
        assert_eq!(
            names,
            vec![Some("agent-sample-18"), Some("from-daemon"), None]
        );
    }

    /// 프로세스 칸은 데몬이 실은 것만 — 셋 다 없으면 칸이 없다.
    #[test]
    fn process_fields_follow_what_the_row_carries() {
        assert_eq!(proc_of_row(&json!({"id":"s"})), None);
        let p = proc_of_row(&json!({"pid":7283,"rss_kb":320_000,"started_unix":1000})).expect("칸");
        assert_eq!(
            (p.pid, p.rss_kb, p.started_unix),
            (Some(7283), Some(320_000), Some(1000))
        );
        assert_eq!(memory(320_000), "313MB");
        assert_eq!(elapsed(2 * 3600 + 5 * 60), "2h05m");
    }

    #[test]
    fn short_forms() {
        assert_eq!(elapsed(42), "42s");
        assert_eq!(elapsed(7 * 60 + 3), "7m");
        assert_eq!(elapsed(3 * 86_400 + 4 * 3600), "3d4h");
        assert_eq!(memory(1536 * 1024), "1.5GB");
    }
}
