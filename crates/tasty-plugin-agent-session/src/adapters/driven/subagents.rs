//! 대화 탭 오른쪽의 **에이전트 내역 패널** — 이 대화의 메인 에이전트 · 서브에이전트 · 백그라운드 작업이
//! 마지막 `/clear` · `/compact` 이후 한 일(읽기 전용).
//!
//! 카드는 셋이다. 맨 위 **현재 에이전트**는 메인 대화의 도구 호출 단계([`crate::adapters::driven::agent_log::MainLog`]),
//! 그 아래 **하위 에이전트**(지시 · 답 · 단계)와 **백그라운드 작업**(종류 · 설명 · 상태)이 도는 것 먼저 ·
//! 최근 먼저 놓인다.
//!
//! claude 는 서브에이전트 대화를 부모 대화 기록 옆 `<대화 id>/subagents/agent-*.jsonl` 에 따로
//! 남기고, 같은 이름의 `.meta.json` 에 설명 · 종류 · 끝났는지를 적는다(실측 2026-10-26,
//! Claude Code 2.1.308). 정본은 그 파일과 메인 기록이다 — 탭은 복사해 두지 않고 다시 읽는다.
//! 마지막 `compact_boundary` 보다 먼저 시작한 서브에이전트는 내역 밖이다.
//!
//! # 언제 읽나
//!
//! - **원문 줄이 내역에 관한 것일 때**(`signals`) — 메인의 도구 호출 · 결과, `Agent` 도구 호출 ·
//!   서브에이전트가 낸 줄(`parent_tool_use_id`) · 백그라운드 작업의 끝 알림 · 경계 · 턴의 `result`.
//!   표시만 해 두고 읽기는 [`READ_GAP`] 에 한 번으로 묶는다 — 도는 동안 줄이 초당 여러 개 온다.
//! - **도는 스레드가 있는 동안 활동 폴링마다**([`RUNNING_GAP`]) — 백그라운드 서브에이전트는 부모
//!   턴이 끝난 뒤에도 돌고, 그동안 부모 쪽 원문 줄이 없다.
//! - 사람이 "에이전트 내역" 줄 · 패널의 "다시 읽기" 를 눌렀을 때.
//!
//! 기록은 덧붙기만 하므로 파일마다 읽은 자리를 기억해 **새로 붙은 줄만** 읽는다(실측 2026-10-27:
//! 7분 돈 서브에이전트 하나가 282줄 · 911KB). 백그라운드 작업은 기록이 아니라 원문 `system/task_*`
//! 줄과 세션 행의 `background_tasks` 로 안다([`crate::adapters::driven::agent_log::BgTasks`]).
//!
//! # 화면
//!
//! 새 서브에이전트가 보이면 패널이 열린다. 사람이 닫으면 다음 서브에이전트가 생길 때까지 닫혀
//! 있다. 문서를 다시 싣지 않고 대화 기록 맨 위 줄과 패널만 갈아 끼운다 — 다시 실으면 입력 중이던
//! 글이 지워진다. 카드는 바뀌지 않는 key 로 그려 사람이 펼친 카드와 스크롤 자리가 그대로다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tasty_plugin_agent_common::tail::{TailPoll, TailState};

use serde_json::Value;

use crate::adapters::driven::agent_log::{BgTasks, MainLog, Steps};
#[cfg(test)]
use crate::adapters::driving::html::escape;
use crate::domain::convo::Convo;
use crate::ports::host::SendHost;
use crate::ports::supervisor::Supervisor;

/// "에이전트 내역" 줄의 링크 표지(패널 열기 · 닫기) — 다른 표지(`agent-older` 등)와 달라야 한다.
const MARK: &str = "agent-subs";
/// 패널 머리의 "다시 읽기".
const RELOAD_MARK: &str = "agent-subs-reload";
/// 패널 머리의 닫기.
const CLOSE_MARK: &str = "agent-subs-close";

/// 한 스레드에서 보여 줄 글의 상한(글자) — 긴 지시 · 답이 문서를 무겁게 하지 않게.
const TEXT_CAP: usize = 20_000;

/// 원문 줄이 알린 뒤 다시 읽기까지의 최소 간격 — 도는 동안 줄마다 읽지 않게 묶는다.
pub const READ_GAP: Duration = Duration::from_secs(2);
/// 도는 스레드가 있을 때 원문 줄 없이도 다시 읽는 간격 — 백그라운드 서브에이전트의 끝을 잡는다.
pub const RUNNING_GAP: Duration = Duration::from_secs(4);

/// 서브에이전트 하나.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    /// 기록 파일 이름(`agent-*`) — 다시 그려도 같은 카드로 남기는 key.
    pub id: String,
    pub description: String,
    pub agent_type: String,
    /// `.meta.json` 의 `finished` — 없으면 아직 도는 것으로 본다.
    pub finished: bool,
    /// 맡긴 지시(첫 user 줄).
    pub prompt: String,
    /// 마지막 assistant 글 — 서브에이전트가 돌려준 답이다.
    pub answer: String,
    /// 지금까지 부른 도구 수.
    pub tools: u32,
    /// 마지막으로 부른 도구 — 도는 동안 "지금 무엇을 하나" 로 보인다.
    pub last_tool: Option<String>,
    /// 첫 줄 · 마지막 줄의 시각(유닉스 초) — 걸린 시간.
    pub started: Option<i64>,
    pub updated: Option<i64>,
    /// 도구 호출 단계 — 상한이 있다([`crate::adapters::driven::agent_log::STEP_CAP`]).
    pub steps: Steps,
}

/// 기록 파일 하나를 어디까지 읽었나.
#[derive(Clone, Debug, Default)]
struct Tail {
    /// 덧붙은 줄만 읽는다 — 쓰는 도중의 반쪽 줄은 다음으로 미루고, 파일이 바뀌면 처음부터다.
    reader: TailState,
    thread: Thread,
    prompt_seen: bool,
}

/// 탭이 들고 있는 하위 에이전트 목록의 상태.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum View {
    /// 아직 안 읽었다.
    #[default]
    Closed,
    /// 읽었다 — 비어 있을 수 있다.
    Loaded(Vec<Thread>),
    /// 기록 자리를 못 찾았다(claude 가 아닌 에이전트 · 데몬이 대화 id 를 모른다).
    Unavailable,
}

/// 탭 하나의 패널 — 화면 상태와 읽기 기억.
#[derive(Clone, Debug, Default)]
pub struct Panel {
    pub view: View,
    /// 패널이 열려 있나.
    pub open: bool,
    /// 지난 읽기에 있던 스레드(파일 이름) — 처음 보는 것이 생기면 패널을 연다. 수로 세면 경계가 앞 것을
    /// 걸러 목록이 바뀔 때 새 스레드를 놓친다.
    seen: std::collections::HashSet<String>,
    /// 원문 줄이 서브에이전트를 알렸다 — 다음 읽기 때 읽는다.
    pub dirty: bool,
    last_read: Option<Instant>,
    /// 찾아 둔 폴더와 그때의 대화 id — `/clear` 로 대화가 바뀌면 다시 찾는다.
    dir: Option<(String, PathBuf)>,
    files: HashMap<PathBuf, Tail>,
    /// 메인 대화 기록 — 폴더를 찾을 때 함께 연다.
    pub main: Option<MainLog>,
    pub bg: BgTasks,
    /// 원문 줄이 화면 값을 이미 바꿨다(백그라운드 작업) — 다음 읽기가 화면을 갈아 끼운다.
    pub changed: bool,
}

impl Panel {
    fn threads(&self) -> &[Thread] {
        match &self.view {
            View::Loaded(t) => t,
            _ => &[],
        }
    }

    fn running(&self) -> usize {
        self.threads().iter().filter(|t| !t.finished).count()
    }

    /// 지금 다시 읽을 때인가.
    pub fn due(&self, now: Instant) -> bool {
        let since = |gap| self.last_read.is_none_or(|t| now.duration_since(t) >= gap);
        ((self.dirty || self.changed) && since(READ_GAP))
            || (self.running() > 0 && since(RUNNING_GAP))
    }

    /// 대화를 비웠다(`/clear`) — 내역을 버리고 다음 읽기에서 새 대화의 기록을 다시 찾는다.
    /// 도는 백그라운드 작업은 대화와 무관하게 살아 있으므로 남긴다. 패널이 열렸는지는 그대로다.
    pub fn forget(&mut self) {
        self.view = View::Loaded(Vec::new());
        self.seen.clear();
        self.dir = None;
        self.files.clear();
        self.main = None;
        self.bg.forget_ended();
        self.dirty = true;
    }

    /// 원문 줄 하나 — 다시 읽을 표시를 하고, 백그라운드 작업 줄이면 그 자리에서 반영한다.
    pub fn on_raw(&mut self, format: &str, line: &str) {
        self.dirty |= signals(format, line);
        if format == crate::domain::usage::CLAUDE_STREAM_JSON && self.bg.apply_raw(line) {
            self.changed = true;
        }
    }

    /// 마지막 경계 — 이보다 먼저 시작한 서브에이전트는 내역 밖이다.
    fn boundary(&self) -> Option<i64> {
        self.main.as_ref().and_then(|m| m.boundary)
    }

    /// 새로 읽은 스레드를 싣는다 — 늘었으면 패널을 연다.
    fn settle(&mut self, view: View, now: Instant) {
        let now_ids: std::collections::HashSet<String> = match &view {
            View::Loaded(t) => t.iter().map(|t| t.id.clone()).collect(),
            _ => Default::default(),
        };
        if now_ids.iter().any(|id| !self.seen.contains(id)) {
            self.open = true;
        }
        self.seen = now_ids;
        self.view = view;
        self.dirty = false;
        self.changed = false;
        self.last_read = Some(now);
    }
}

/// 링크가 무엇을 눌렀나.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    /// 대화 기록 맨 위의 줄 — 패널을 열고 닫는다.
    Toggle,
    Reload,
    Close,
}

/// **하위 에이전트 링크를 눌렀나.**
pub fn parse(url: &str) -> Option<Click> {
    let q = crate::adapters::driving::url::query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    let has = |m| crate::adapters::driving::url::has_mark(&fields, m);
    if has(RELOAD_MARK) {
        Some(Click::Reload)
    } else if has(CLOSE_MARK) {
        Some(Click::Close)
    } else if has(MARK) {
        Some(Click::Toggle)
    } else {
        None
    }
}

/// **이 원문 줄이 내역에 관한 것인가** — 그러면 패널을 다시 읽을 표시를 한다.
pub fn signals(format: &str, line: &str) -> bool {
    if format != crate::domain::usage::CLAUDE_STREAM_JSON {
        return false;
    }
    // 대부분의 줄은 여기서 걸러진다 — 전부 파싱하지 않는다.
    if line.contains("<task-notification>") || line.contains("compact_boundary") {
        return true;
    }
    if !(line.contains("\"Agent\"")
        || line.contains("parent_tool_use_id\":\"")
        || line.contains("\"type\":\"result\"")
        || line.contains("\"tool_use")
        || line.contains("tool_result"))
    {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if v.get("parent_tool_use_id").is_some_and(Value::is_string) {
        return true;
    }
    if v.get("type").and_then(Value::as_str) == Some("result") {
        return true;
    }
    // 메인의 도구 호출 · 결과 — 단계가 붙는다.
    v.pointer("/message/content")
        .and_then(Value::as_array)
        .is_some_and(|blocks| {
            blocks.iter().any(|b| {
                matches!(
                    b.get("type").and_then(Value::as_str),
                    Some("tool_use" | "tool_result")
                )
            })
        })
}

/// 그 세션의 기록 자리 — 데몬 `status` 한 번으로 모은다.
pub struct Located {
    pub convo: String,
    /// 메인 대화 기록.
    pub transcript: PathBuf,
    /// `<대화>/subagents`.
    pub subagents: PathBuf,
    /// 세션 행의 `background_tasks` — 탭을 열 때 이미 도는 작업.
    pub tasks: Vec<Value>,
}

/// 그 세션이 붙든 대화의 기록 자리 — `history::fill` 과 같은 길로 대화 기록을 찾는다.
pub fn locate<S: Supervisor>(sup: &S, session: &str) -> Option<Located> {
    let st = sup.status(session).ok()?;
    let agent = st.get("agent_name").and_then(Value::as_str)?;
    let convo = st.get("agent_session_id").and_then(Value::as_str)?;
    let profile = sup
        .agent_list()
        .ok()?
        .into_iter()
        .find(|p| p.name == agent && p.kind == "claude")?;
    let dir = crate::adapters::driven::store::budget::config_dir(&profile.env)?;
    let transcript = crate::adapters::driven::store::budget::find_transcript(&dir, convo)?;
    Some(Located {
        convo: convo.to_string(),
        subagents: transcript.with_extension("").join("subagents"),
        transcript,
        tasks: st
            .get("background_tasks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    })
}

/// 폴더의 스레드 전부를 새로 붙은 줄만 읽어 맞춘다 — 오래된 것부터. 폴더가 없으면(띄운 적 없다) 빈 목록이다.
fn read_threads(dir: &Path, files: &mut HashMap<PathBuf, Tail>) -> Vec<Thread> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        files.clear();
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|p| {
            let at = std::fs::metadata(&p)
                .and_then(|m| m.created().or_else(|_| m.modified()))
                .ok()?;
            Some((at, p))
        })
        .collect();
    found.sort_by_key(|(at, _)| *at);
    files.retain(|p, _| found.iter().any(|(_, f)| f == p));
    found
        .into_iter()
        .map(|(_, p)| {
            let tail = files.entry(p.clone()).or_default();
            read_tail(&p, tail);
            tail.thread.id = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            tail.thread.clone()
        })
        .collect()
}

/// 파일 하나 — 지난번 자리부터 읽고, meta 는 매번 다시 읽는다(작고, `finished` 가 바뀐다).
fn read_tail(jsonl: &Path, tail: &mut Tail) {
    let meta: Value = std::fs::read_to_string(jsonl.with_extension("meta.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let s = |k: &str| {
        meta.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    tail.thread.description = s("description");
    tail.thread.agent_type = s("agentType");
    tail.thread.finished = meta.get("finished").and_then(Value::as_bool) == Some(true);

    loop {
        match tail.reader.poll(jsonl) {
            Ok(TailPoll::Missing) => return,
            Ok(TailPoll::Lines { lines, resynced }) => {
                if resynced {
                    // 다른 파일로 바뀌었다 — 처음부터 다시 센다.
                    tail.prompt_seen = false;
                    let t = &mut tail.thread;
                    t.prompt.clear();
                    t.answer.clear();
                    t.tools = 0;
                    t.last_tool = None;
                    t.started = None;
                    t.updated = None;
                    t.steps.clear();
                }
                if lines.is_empty() {
                    return;
                }
                for line in &lines {
                    apply_line(tail, line);
                }
            }
            Err(e) => {
                tracing::warn!("[agent_session] 하위 에이전트 기록을 못 읽었다 {jsonl:?}: {e}");
                return;
            }
        }
    }
}

/// 한 줄을 스레드에 반영한다 — 첫 user 글은 지시, 마지막 assistant 글은 답, 도구 호출은 단계로 쌓고
/// 결과로 성패를 붙인다. 깨진 줄은 건너뛴다.
fn apply_line(tail: &mut Tail, line: &str) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let t = &mut tail.thread;
    if let Some(at) = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::adapters::driven::store::budget::epoch)
    {
        t.started.get_or_insert(at);
        t.updated = Some(at);
    }
    t.steps.apply(&v);
    let content = v.pointer("/message/content");
    match v.get("type").and_then(Value::as_str) {
        Some("user") if !tail.prompt_seen => {
            let p = content.map(text_of).unwrap_or_default();
            if !p.is_empty() {
                t.prompt = cap(p);
                tail.prompt_seen = true;
            }
        }
        Some("assistant") => {
            let a = content.map(text_of).unwrap_or_default();
            if !a.is_empty() {
                t.answer = cap(a);
            }
            for b in content.and_then(Value::as_array).into_iter().flatten() {
                if b.get("type").and_then(Value::as_str) == Some("tool_use") {
                    t.tools += 1;
                    t.last_tool = b.get("name").and_then(Value::as_str).map(str::to_string);
                }
            }
        }
        _ => {}
    }
}

/// 내용이 글 하나면 그대로, 블록 배열이면 text 블록만 잇는다(도구 호출 · 결과는 뺀다).
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn cap(mut s: String) -> String {
    if s.chars().count() > TEXT_CAP {
        s = s.chars().take(TEXT_CAP).collect();
        s.push('…');
    }
    s
}

/// 걸린 시간을 짧게 — `42초` · `3분 20초` · `1시간 5분`.
fn elapsed(secs: i64) -> String {
    let s = secs.max(0);
    let (h, m, sec) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        crate::domain::i18n::tn("sup.chat.alog_hm", &[&h.to_string(), &m.to_string()])
    } else if m > 0 {
        crate::domain::i18n::tn("sup.chat.alog_ms", &[&m.to_string(), &sec.to_string()])
    } else {
        crate::domain::i18n::t1("sup.chat.alog_s", &sec.to_string())
    }
}

/// 패널 모양 — 문서에 한 번 싣는다(`view::render`).
///
/// 패널은 창 오른쪽에 붙고, 열려 있는 동안 본문이 그 너비만큼 비켜선다 — 덮으면 대화의 오른쪽
/// 끝과 입력칸이 가린다. 좁은 창에서는 본문을 너무 깎지 않게 너비를 창의 40% 로 묶는다.
///
/// 너비는 왼쪽 가장자리를 끌어 바꾼다(`app.js` 가 `--as-sub-w` 를 올린다 · localStorage 에 남는다).
/// 내용은 줄바꿈 없이 두고 가로로 스크롤한다 — 잘린 요약·경로를 패널을 넓히거나 밀어서 끝까지 본다.
pub const STYLE: &str = r#"<style>
 :root{--as-sub-w:min(380px,40vw)}
 #subpanel{position:fixed;top:0;right:0;bottom:0;width:var(--as-sub-w);display:flex;
   flex-direction:column;background:#1a1d23;border-left:1px solid #2a2e37;z-index:5}
 #subpanel[hidden]{display:none}
 body:has(#subpanel:not([hidden])){padding-right:var(--as-sub-w)}
 /* 너비 조절 손잡이 — 왼쪽 테두리에 겹쳐 둔다. 커서가 가장자리임을 알린다. */
 #subpanel .spr{position:absolute;left:-5px;top:0;bottom:0;width:10px;cursor:col-resize;
   z-index:6;touch-action:none}
 #subpanel .spr:hover{background:#2a2e37}
 #subpanel .sph{display:flex;align-items:center;gap:8px;padding:10px 12px;
   border-bottom:1px solid #2a2e37;color:#c8ccd4}
 #subpanel .sph b{font-weight:700}
 #subpanel .sph .cnt{color:#5a6070;flex:1}
 #subpanel .sph a{color:#8b93a1;text-decoration:none}
 #subpanel .sph a:hover{color:#c8ccd4}
 #subpanel .spl{flex:1;overflow:auto;padding:8px 12px}
 /* 카드는 가장 긴 줄만큼 넓어지고(최소 패널 폭), 넘치는 폭은 위 `.spl` 이 가로로 스크롤한다.
    줄을 접지 않는 이유: 도구 요약·경로는 접히면 읽기 어렵고, 사용자가 폭을 직접 정한다. */
 #subpanel .spl>*{width:max-content;min-width:100%}
 #subpanel .spe{color:#5a6070;padding:12px 0;white-space:pre}
 #subpanel details.sa{border-left:2px solid #2a2e37;padding:4px 0 4px 10px;margin-bottom:10px}
 #subpanel details.sa.run{border-left-color:#5ac17d}
 #subpanel details.sa.main{border-left-color:#6a9bd8}
 #subpanel summary{cursor:pointer;list-style:none;display:flex;flex-direction:column;gap:2px}
 #subpanel summary::-webkit-details-marker{display:none}
 #subpanel .sat{display:flex;gap:6px;align-items:baseline}
 #subpanel .sad{color:#e6e9ef;overflow:visible;text-overflow:clip;white-space:nowrap;flex:1}
 #subpanel .sas{color:#5ac17d;flex-shrink:0}
 #subpanel .main .sas{color:#6a9bd8}
 #subpanel .done .sas{color:#5a6070}
 #subpanel .fail .sas{color:#e06c75}
 #subpanel .sam{color:#5a6070;font-size:12px}
 #subpanel .sab{color:#8b93a1;font-size:12px;margin-top:8px}
 #subpanel .sax{white-space:pre;color:#c8ccd4;max-height:40vh;overflow-y:auto;
   margin-top:2px;padding-left:2ch}
 #subpanel .sax.none{color:#5a6070}
 #subpanel .sst{max-height:40vh;overflow:auto;margin-top:2px;padding-left:2ch;font-size:12px}
 #subpanel .ss{display:flex;gap:4px;color:#c8ccd4;white-space:nowrap}
 #subpanel .ss .ts{color:#5a6070;flex-shrink:0}
 #subpanel .ss .sok{color:#5a6070;flex-shrink:0;width:1.2em}
 #subpanel .ss.ok .sok{color:#5ac17d}
 #subpanel .ss.err .sok{color:#e06c75}
 #subpanel .ss b{font-weight:600;flex-shrink:0}
 #subpanel .ss .ssm{color:#8b93a1;overflow:visible;text-overflow:clip}
 #subpanel .sso{color:#5a6070;font-size:12px}
</style>"#;

/// 단계 목록의 화면 값 — 성패는 class 와 표지로 싣는다.
fn steps_json(steps: &Steps) -> (Value, Value) {
    let items: Vec<Value> = steps
        .items
        .iter()
        .map(|s| {
            let (cls, mark) = match s.ok {
                Some(true) => ("ok", "\u{2714}"),
                Some(false) => ("err", "\u{2716}"),
                None => ("", "\u{00B7}"),
            };
            serde_json::json!({
                "k": s.id, "at": s.at, "tool": s.tool, "summary": s.summary, "cls": cls, "mark": mark,
            })
        })
        .collect();
    let omitted = (steps.omitted > 0)
        .then(|| crate::domain::i18n::t1("sup.chat.alog_omitted", &steps.omitted.to_string()));
    (Value::Array(items), serde_json::json!(omitted))
}

/// 맨 위 카드 — 메인 대화의 도구 호출 단계.
fn main_card(m: Option<&MainLog>) -> Value {
    use crate::domain::i18n::{t, t1};
    let empty = Steps::default();
    let steps = m.map_or(&empty, |m| &m.steps);
    let mut meta = Vec::new();
    if steps.len() > 0 {
        meta.push(t1("sup.chat.alog_tools", &steps.len().to_string()));
    }
    if let Some(last) = steps.items.back().filter(|s| s.ok.is_none()) {
        meta.push(t1("sup.chat.alog_now", &last.tool));
    }
    let (items, omitted) = steps_json(steps);
    serde_json::json!({
        "k": "main", "cls": "main", "mark": "\u{25C6}", "desc": t("sup.chat.alog_main"),
        "meta": meta.join(" · "), "prompt": null, "answer": null, "steps": items, "omitted": omitted,
    })
}

/// 하위 에이전트 카드 — 지시 · 답 · 단계.
fn card(t: &Thread) -> Value {
    let (cls, mark) = if t.finished {
        ("done", "\u{2714}")
    } else {
        ("run", "\u{25CF}")
    };
    let mut meta = vec![t.agent_type.clone()];
    if t.tools > 0 {
        meta.push(crate::domain::i18n::t1(
            "sup.chat.alog_tools",
            &t.tools.to_string(),
        ));
    }
    if let (Some(a), Some(b)) = (t.started, t.updated) {
        meta.push(elapsed(b - a));
    }
    if !t.finished
        && let Some(tool) = &t.last_tool
    {
        meta.push(crate::domain::i18n::t1("sup.chat.alog_now", tool));
    }
    let meta = meta
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let desc = if t.description.is_empty() {
        crate::domain::i18n::t("sup.chat.alog_sub")
    } else {
        t.description.clone()
    };
    let (items, omitted) = steps_json(&t.steps);
    serde_json::json!({
        "k": format!("s:{}", t.id),
        "cls": cls,
        "mark": mark,
        "desc": desc,
        "meta": meta,
        "prompt": t.prompt,
        "answer": t.answer,
        "steps": items,
        "omitted": omitted,
    })
}

/// 백그라운드 작업 카드 — 종류 · 설명 · 상태. 펼칠 본문은 없다.
fn bg_card(b: &crate::adapters::driven::agent_log::BgTask) -> Value {
    use crate::domain::i18n::t;
    let (cls, mark, state) = match b.ended.as_deref() {
        None => ("run", "\u{25CF}", t("sup.chat.alog_running")),
        Some("completed") => ("done", "\u{2714}", t("sup.chat.alog_bg_done")),
        Some("failed" | "error") => ("fail", "\u{2716}", t("sup.chat.alog_bg_failed")),
        Some(_) => ("done", "\u{25A0}", t("sup.chat.alog_bg_stopped")),
    };
    let desc = if b.description.is_empty() {
        t("sup.chat.alog_bg")
    } else {
        b.description.clone()
    };
    let meta = [b.kind.as_str(), state.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    serde_json::json!({
        "k": format!("b:{}", b.id), "cls": cls, "mark": mark, "desc": desc, "meta": meta,
        "prompt": null, "answer": null, "steps": null, "omitted": null,
    })
}

/// 맨 위 카드 아래의 카드들(하위 에이전트 · 백그라운드 작업) — 도는 것 먼저 · 최근 먼저.
fn other_cards(p: &Panel) -> (Vec<Value>, usize) {
    // (끝났나, 시작 시각, 들어온 순서, 카드) — 시각을 모르는 것은 막 생긴 것으로 본다.
    let mut rows: Vec<(bool, i64, usize, Value)> = Vec::new();
    for (i, t) in p.threads().iter().enumerate() {
        rows.push((t.finished, t.started.unwrap_or(i64::MAX), i, card(t)));
    }
    let base = rows.len();
    for (i, b) in p.bg.items.iter().enumerate() {
        rows.push((b.ended.is_some(), b.at, base + i, bg_card(b)));
    }
    let running = rows.iter().filter(|r| !r.0).count();
    rows.sort_by_key(|r| (r.0, std::cmp::Reverse(r.1), std::cmp::Reverse(r.2)));
    (rows.into_iter().map(|r| r.3).collect(), running)
}

/// 에이전트 내역 줄과 패널의 화면 값 — 페이지 앱이 그린다. 카드 순서는 여기서 정한다.
pub fn state(convo: &Convo) -> Value {
    use crate::domain::i18n::{t, t1, tn};
    let p = &convo.subs;
    let (others, running) = other_cards(p);
    let label = match &p.view {
        View::Closed => t("sup.chat.alog_title"),
        _ if p.open => t1("sup.chat.alog_link_open", &others.len().to_string()),
        _ => t1("sup.chat.alog_link_n", &others.len().to_string()),
    };
    let (empty, cards): (Option<String>, Vec<Value>) = match &p.view {
        View::Closed => (None, Vec::new()),
        View::Unavailable => (Some(t("sup.chat.alog_unavailable")), others.clone()),
        View::Loaded(_) => {
            let mut cards = vec![main_card(p.main.as_ref())];
            cards.extend(others.iter().cloned());
            (None, cards)
        }
    };
    serde_json::json!({
        "row": { "hidden": convo.session_id.is_none(), "label": label, "mark": MARK },
        "panel": {
            "hidden": !(convo.session_id.is_some() && p.open),
            "title": t("sup.chat.alog_title"),
            "count": tn("sup.chat.alog_count", &[&others.len().to_string(), &running.to_string()]),
            "reloadTitle": t("sup.chat.alog_reload"),
            "reloadMark": RELOAD_MARK,
            "closeTitle": t("sup.chat.alog_close"),
            "closeMark": CLOSE_MARK,
            "resizeTitle": t("sup.chat.alog_resize"),
            "empty": empty,
            "cards": cards,
            "promptLabel": t("sup.chat.alog_prompt"),
            "answerLabel": t("sup.chat.alog_answer"),
            "noAnswer": t("sup.chat.alog_no_answer"),
            "stepsLabel": t("sup.chat.alog_steps"),
            "noSteps": t("sup.chat.alog_no_steps"),
        },
    })
}

/// 페이지 앱의 에이전트 내역 줄과 같은 모양 — 시험 전용(jsdom 대조).
#[cfg(test)]
pub fn html(convo: &Convo) -> String {
    let r = &state(convo)["row"];
    format!(
        r##"<div id="subs" class="older"{}><a class="more" href="#{}=1">{}</a></div>"##,
        if r["hidden"] == true {
            r#" hidden="""#
        } else {
            ""
        },
        r["mark"].as_str().unwrap_or_default(),
        escape(r["label"].as_str().unwrap_or_default())
    )
}

/// 앱의 `clock` 과 같은 `[HH:MM:SS] ` — 시험 전용이라 UTC 로 낸다(jsdom 은 `TZ=UTC` 로 돌린다).
#[cfg(test)]
fn clock_utc(at: i64) -> String {
    let s = at.rem_euclid(86_400);
    format!("[{:02}:{:02}:{:02}] ", s / 3600, s % 3600 / 60, s % 60)
}

/// 페이지 앱의 에이전트 내역 패널과 같은 모양 — 시험 전용(jsdom 대조).
#[cfg(test)]
pub fn panel_html(convo: &Convo) -> String {
    let p = &state(convo)["panel"];
    let s = |x: &Value| escape(x.as_str().unwrap_or_default());
    let mut h = format!(
        r##"<aside id="subpanel"{}><div class="spr" title="{}"></div><div class="sph"><b>{}</b><span class="cnt">{}</span><a href="#{}=1" title="{}">↻</a><a href="#{}=1" title="{}">✕</a></div><div class="spl">"##,
        if p["hidden"] == true {
            r#" hidden="""#
        } else {
            ""
        },
        s(&p["resizeTitle"]),
        s(&p["title"]),
        s(&p["count"]),
        s(&p["reloadMark"]),
        s(&p["reloadTitle"]),
        s(&p["closeMark"]),
        s(&p["closeTitle"]),
    );
    if let Some(e) = p["empty"].as_str() {
        h.push_str(&format!(r#"<div class="spe">{}</div>"#, escape(e)));
    }
    for c in p["cards"].as_array().into_iter().flatten() {
        h.push_str(&format!(
            r#"<details class="sa {}" data-k="{}"><summary><span class="sat"><span class="sas">{}</span><span class="sad" title="{}">{}</span></span><span class="sam">{}</span></summary>"#,
            s(&c["cls"]),
            s(&c["k"]),
            s(&c["mark"]),
            s(&c["desc"]),
            s(&c["desc"]),
            s(&c["meta"]),
        ));
        if c["prompt"].is_string() {
            let answer = match c["answer"].as_str().filter(|a| !a.is_empty()) {
                Some(a) => format!(r#"<div class="sax">{}</div>"#, escape(a)),
                None => format!(r#"<div class="sax none">{}</div>"#, s(&p["noAnswer"])),
            };
            h.push_str(&format!(
                r#"<div class="sab">{}</div><div class="sax">{}</div><div class="sab">{}</div>{answer}"#,
                s(&p["promptLabel"]),
                s(&c["prompt"]),
                s(&p["answerLabel"]),
            ));
        }
        if let Some(steps) = c["steps"].as_array() {
            h.push_str(&format!(
                r#"<div class="sab">{}</div>"#,
                s(&p["stepsLabel"])
            ));
            if steps.is_empty() {
                h.push_str(&format!(
                    r#"<div class="sax none">{}</div>"#,
                    s(&p["noSteps"])
                ));
            } else {
                h.push_str(r#"<div class="sst">"#);
                if let Some(o) = c["omitted"].as_str() {
                    h.push_str(&format!(r#"<div class="sso">{}</div>"#, escape(o)));
                }
                for st in steps {
                    let ts = st["at"]
                        .as_i64()
                        .map(|at| format!(r#"<span class="ts">{}</span>"#, clock_utc(at)))
                        .unwrap_or_default();
                    h.push_str(&format!(
                        r#"<div class="ss {}">{ts}<span class="sok">{}</span><b>{}</b><span class="ssm" title="{}">{}</span></div>"#,
                        s(&st["cls"]),
                        s(&st["mark"]),
                        s(&st["tool"]),
                        s(&st["summary"]),
                        s(&st["summary"]),
                    ));
                }
                h.push_str("</div>");
            }
        }
        h.push_str("</details>");
    }
    h.push_str("</div></aside>");
    h
}

pub fn message(convo: &Convo) -> Value {
    serde_json::json!({ "type": "subs.set", "subs": state(convo) })
}

/// 다시 읽는다 — 대화 id 가 바뀌었으면 기록 자리를 다시 찾고 내역을 새로 시작한다. 화면이 달라졌으면 `true`.
pub fn refresh<S: Supervisor>(sup: &S, c: &mut Convo, now: Instant) -> bool {
    let before = state(c);
    let Some(session) = c.session_id.clone() else {
        return false;
    };
    let stale = match (&c.subs.dir, &c.agent_session_id) {
        (Some((convo, _)), Some(now_convo)) => convo != now_convo,
        (Some(_), None) => false,
        (None, _) => true,
    };
    let p = &mut c.subs;
    if stale {
        if p.dir.is_some() {
            // `/clear` — 지난 대화의 끝난 작업은 새 범위 밖이다.
            p.bg.forget_ended();
        }
        p.files.clear();
        p.main = None;
        p.dir = None;
        if let Some(l) = locate(sup, &session) {
            p.bg.seed(&l.tasks);
            p.main = match MainLog::open(&l.transcript) {
                Ok(m) => {
                    tracing::info!(
                        "[agent_session] 에이전트 내역: 메인 단계 {} · 경계 {:?} · 건너뛴 줄 {}",
                        m.steps.len(),
                        m.boundary,
                        m.skipped
                    );
                    Some(m)
                }
                Err(e) => {
                    tracing::warn!(
                        "[agent_session] 대화 기록을 못 열었다({}): {e}",
                        l.transcript.display()
                    );
                    None
                }
            };
            p.dir = Some((l.convo, l.subagents));
        }
    } else if let Some(m) = p.main.as_mut() {
        let was = m.boundary;
        m.poll();
        if m.boundary != was {
            // `/compact` — 경계 앞에서 끝난 작업은 지난 범위다.
            p.bg.forget_ended();
        }
    }
    let boundary = p.boundary();
    let view = match &p.dir {
        Some((_, dir)) => {
            let mut threads = read_threads(dir, &mut p.files);
            if let Some(b) = boundary {
                threads.retain(|t| t.started.is_none_or(|s| s >= b));
            }
            View::Loaded(threads)
        }
        None => View::Unavailable,
    };
    let pushed = p.changed;
    p.settle(view, now);
    pushed || before != state(c)
}

/// 눌렀다 — 다시 읽고 줄과 패널을 갈아 끼운다.
pub fn click<H: crate::ports::host::HostCall, S: Supervisor>(
    host: &H,
    sup: &S,
    sid: u32,
    c: &mut Convo,
    what: Click,
) {
    match what {
        Click::Close => c.subs.open = false,
        Click::Toggle if c.subs.open => c.subs.open = false,
        Click::Toggle | Click::Reload => {
            refresh(sup, c, Instant::now());
            c.subs.open = true;
        }
    }
    crate::application::session_surface::patch(host, sid, &message(c));
}

/// 읽을 때가 된 탭의 패널을 다시 읽고, 화면이 달라진 탭만 갈아 끼운다 — 같으면 안 건드린다(깜빡임 · 스크롤).
pub fn refresh_due<H: crate::ports::host::HostCall, S: Supervisor>(
    host: &H,
    sup: &S,
    convos: &mut crate::domain::convo::Convos,
    now: Instant,
) {
    for (&sid, c) in convos.iter_mut() {
        if c.session_id.is_none() || !c.subs.due(now) {
            continue;
        }
        if refresh(sup, c, now) {
            crate::application::session_surface::patch(host, sid, &message(c));
        }
    }
}

/// `agent_session.agent_log` — 그 탭의 내역을 다시 읽고 화면 값을 그대로 돌려준다(읽기 전용). 패널이
/// 닫혀 있어도 답한다 — 에이전트 · 시험이 화면을 거치지 않고 내역을 본다. 다시 읽어 달라졌으면 화면도
/// 갈아 끼운다(답과 화면이 어긋나지 않게).
pub fn by_ipc<H: crate::ports::host::HostCall, S: Supervisor>(
    host: &H,
    sup: &S,
    convos: &mut crate::domain::convo::Convos,
    sid: u32,
) -> Option<Value> {
    let c = convos.get_mut(&sid)?;
    c.session_id.as_ref()?;
    if refresh(sup, c, Instant::now()) {
        crate::application::session_surface::patch(host, sid, &message(c));
    }
    let s = state(c);
    Some(serde_json::json!({ "surface_id": sid, "row": s["row"], "panel": s["panel"] }))
}

/// 탭 쪽 처리 — 대화를 만지므로 plugin 의 메서드로 둔다.
impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 원문 줄 뒤 · 활동 폴링마다 부른다.
    pub(crate) fn refresh_subagents(&mut self, host: &impl SendHost) {
        refresh_due(host, &self.sup, &mut self.convos, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    fn scratch() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sup-subs-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fed(jsonl: &str) -> Thread {
        let mut t = Tail::default();
        for l in jsonl.lines() {
            apply_line(&mut t, l);
        }
        t.thread
    }

    #[test]
    fn the_prompt_is_the_first_user_text_and_the_answer_the_last_assistant_text() {
        let jsonl = [
            line(
                serde_json::json!({"type":"user","message":{"content":"찾아라"},
                "timestamp":"2026-10-27T00:17:13.761Z"}),
            ),
            line(serde_json::json!({"type":"attachment"})),
            "깨진 줄\n".to_string(),
            line(serde_json::json!({"type":"assistant","message":{"content":[
                {"type":"text","text":"보는 중"},{"type":"tool_use","name":"Grep"}]}})),
            line(serde_json::json!({"type":"user","message":{"content":[
                {"type":"tool_result","content":"결과"}]}})),
            line(serde_json::json!({"type":"assistant","message":{"content":[
                {"type":"text","text":"찾았다: a.rs"}]}})),
            line(serde_json::json!({"type":"assistant","message":{"content":[
                {"type":"tool_use","name":"Read"}]},"timestamp":"2026-10-27T00:20:33.000Z"})),
        ]
        .concat();
        let t = fed(&jsonl);
        assert_eq!(t.prompt, "찾아라");
        assert_eq!(t.answer, "찾았다: a.rs", "도구만 쓴 줄이 답을 지웠다");
        assert_eq!(t.tools, 2);
        assert_eq!(t.last_tool.as_deref(), Some("Read"));
        assert_eq!(t.updated.unwrap() - t.started.unwrap(), 200);
    }

    #[test]
    fn threads_read_meta_and_come_oldest_first() {
        let dir = scratch();
        for (name, desc, finished) in [("agent-b", "둘째", false), ("agent-a", "첫째", true)] {
            std::fs::write(
                dir.join(format!("{name}.jsonl")),
                line(serde_json::json!({"type":"user","message":{"content":desc}})),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}.meta.json")),
                serde_json::json!({"description":desc,"agentType":"Explore","finished":finished})
                    .to_string(),
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let t = read_threads(&dir, &mut HashMap::new());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(t.len(), 2, "meta 파일을 스레드로 셌다");
        assert_eq!(t[0].description, "둘째", "오래된 것이 먼저다");
        assert!(!t[0].finished && t[1].finished);
        assert_eq!(t[1].prompt, "첫째");
    }

    /// 덧붙은 줄만 읽는다 — 반만 쓰인 줄은 다음에 이어 읽고, 두 번 세지 않는다.
    #[test]
    fn a_growing_file_is_read_from_where_it_stopped() {
        let dir = scratch();
        let f = dir.join("agent-x.jsonl");
        let first = line(serde_json::json!({"type":"user","message":{"content":"맡김"}}));
        let tool = line(serde_json::json!({"type":"assistant","message":{"content":[
            {"type":"tool_use","name":"Bash"}]}}));
        let (head, rest) = tool.split_at(10);
        std::fs::write(&f, format!("{first}{head}")).unwrap();
        let mut files = HashMap::new();
        let t = read_threads(&dir, &mut files);
        assert_eq!((t[0].prompt.as_str(), t[0].tools), ("맡김", 0));
        let mut more = std::fs::OpenOptions::new().append(true).open(&f).unwrap();
        std::io::Write::write_all(&mut more, rest.as_bytes()).unwrap();
        let t = read_threads(&dir, &mut files);
        assert_eq!(t[0].tools, 1, "반쪽 줄을 이어 붙여 읽지 못했다");
        let t = read_threads(&dir, &mut files);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(t[0].tools, 1, "읽은 줄을 다시 셌다");
    }

    #[test]
    fn a_missing_folder_is_no_threads() {
        assert!(read_threads(Path::new("/nonexistent/sup-subs"), &mut HashMap::new()).is_empty());
    }

    fn running(desc: &str) -> Thread {
        Thread {
            id: desc.into(),
            description: desc.into(),
            agent_type: "Explore".into(),
            ..Thread::default()
        }
    }

    #[test]
    fn a_new_thread_opens_the_panel_and_a_closed_panel_stays_closed_until_the_next() {
        let mut p = Panel::default();
        let now = Instant::now();
        p.settle(View::Loaded(vec![]), now);
        assert!(!p.open, "빈 목록이 패널을 열었다");
        p.settle(View::Loaded(vec![running("a")]), now);
        assert!(p.open, "새 서브에이전트가 패널을 안 열었다");
        p.open = false;
        p.settle(View::Loaded(vec![running("a")]), now);
        assert!(!p.open, "사람이 닫은 패널을 같은 목록이 다시 열었다");
        p.settle(View::Loaded(vec![running("a"), running("b")]), now);
        assert!(p.open);
    }

    #[test]
    fn reading_waits_for_the_gap_and_keeps_going_while_a_thread_runs() {
        let mut p = Panel::default();
        let t0 = Instant::now();
        assert!(!p.due(t0), "알림도 도는 것도 없는데 읽는다");
        p.dirty = true;
        assert!(p.due(t0), "처음 알림을 바로 안 읽는다");
        p.settle(View::Loaded(vec![running("a")]), t0);
        p.dirty = true;
        assert!(!p.due(t0 + READ_GAP / 2), "간격 안에 다시 읽는다");
        assert!(p.due(t0 + READ_GAP));
        p.dirty = false;
        assert!(!p.due(t0 + READ_GAP));
        assert!(p.due(t0 + RUNNING_GAP), "도는 스레드를 다시 안 읽는다");
        let mut done = running("a");
        done.finished = true;
        p.settle(View::Loaded(vec![done]), t0);
        assert!(!p.due(t0 + RUNNING_GAP * 10), "다 끝났는데 계속 읽는다");
    }

    #[test]
    fn subagent_lines_are_told_apart_from_the_rest() {
        let f = crate::domain::usage::CLAUDE_STREAM_JSON;
        let agent = serde_json::json!({"type":"assistant","message":{"content":[
            {"type":"tool_use","name":"Agent","input":{}}]}})
        .to_string();
        let child = serde_json::json!({"type":"assistant","parent_tool_use_id":"toolu_1",
            "message":{"content":[]}})
        .to_string();
        let top = serde_json::json!({"type":"assistant","parent_tool_use_id":null,
            "message":{"content":[{"type":"text","text":"Agent"}]}})
        .to_string();
        assert!(signals(f, &agent));
        assert!(signals(f, &child));
        assert!(signals(f, r#"{"type":"result","subtype":"success"}"#));
        assert!(signals(f, "<task-notification> done"));
        assert!(
            top.contains("\"Agent\""),
            "시험 줄이 앞단 거르기를 못 지나 판정까지 안 간다"
        );
        assert!(
            !signals(f, &top),
            "글 속의 낱말이나 빈 부모 칸을 서브에이전트 줄로 셌다"
        );
        assert!(!signals("other", &agent), "모르는 형식을 읽었다");
    }

    #[test]
    fn the_row_and_panel_hide_without_a_session_and_escape_what_they_show() {
        let mut c = Convo::default();
        assert!(html(&c).contains(r#"id="subs" class="older" hidden"#));
        assert!(panel_html(&c).contains(r#"<aside id="subpanel" hidden="">"#));
        c.session_id = Some("s".into());
        assert!(!html(&c).contains(" hidden"));
        c.subs
            .settle(View::Loaded(vec![running("<b>")]), Instant::now());
        let h = panel_html(&c);
        assert!(
            h.contains(r#"<aside id="subpanel">"#),
            "새 스레드에 패널이 안 열렸다"
        );
        assert!(h.contains("&lt;b&gt;") && !h.contains("<b><"));
        assert_eq!(message(&c)["subs"], state(&c), "패널 값을 그대로 싣는다");
    }

    #[test]
    fn the_panel_carries_a_resize_handle_and_scrolls_sideways() {
        let mut c = Convo {
            session_id: Some("s".into()),
            ..Convo::default()
        };
        c.subs
            .settle(View::Loaded(vec![running("x")]), Instant::now());
        let h = panel_html(&c);
        assert!(
            h.contains(r#"<div class="spr" title=""#),
            "패널에 너비 조절 손잡이가 없다"
        );
        assert_eq!(
            state(&c)["panel"]["resizeTitle"],
            crate::domain::i18n::t("sup.chat.alog_resize"),
            "손잡이 설명이 문구 표를 안 쓴다"
        );
        assert!(STYLE.contains("--as-sub-w"), "패널 폭이 변수로 안 빠졌다");
        assert!(
            STYLE.contains("#subpanel .spl{flex:1;overflow:auto"),
            "패널 몸통이 가로로 스크롤되지 않는다"
        );
        assert!(
            STYLE.contains("#subpanel .sad{color:#e6e9ef;overflow:visible"),
            "카드 설명이 아직 잘린다"
        );
    }

    #[test]
    fn running_threads_come_first_then_the_newest() {
        let mut c = Convo {
            session_id: Some("s".into()),
            ..Convo::default()
        };
        let mut old = running("old-done");
        old.finished = true;
        let mut new = running("new-done");
        new.finished = true;
        c.subs.settle(
            View::Loaded(vec![old, running("mid-run"), new]),
            Instant::now(),
        );
        let h = panel_html(&c);
        let at = |s| h.find(s).unwrap();
        assert!(at("mid-run") < at("new-done") && at("new-done") < at("old-done"));
    }

    #[test]
    fn the_three_links_are_told_apart() {
        assert_eq!(parse("about:blank#agent-subs=1"), Some(Click::Toggle));
        assert_eq!(
            parse("about:blank#agent-subs-reload=1"),
            Some(Click::Reload)
        );
        assert_eq!(parse("about:blank#agent-subs-close=1"), Some(Click::Close));
        assert_eq!(parse("about:blank#agent-older=1"), None);
    }

    /// 기록하는 호스트 — 갈아 끼운 탭을 센다.
    #[derive(Default)]
    struct Patches(std::cell::RefCell<Vec<u32>>);
    impl crate::ports::host::HostCall for Patches {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            assert_eq!(method, "webview.post_message");
            let sid = params["surface_id"].as_u64().unwrap() as u32;
            self.0.borrow_mut().push(sid);
            Ok(Value::Null)
        }
    }

    /// 새 스레드는 패널을 열며 갈아 끼우고, 아무것도 안 바뀐 다시 읽기는 화면을 안 건드리며, 때가 안 된 탭은 안 읽는다.
    #[test]
    fn only_a_due_tab_whose_view_changed_is_patched() {
        let dir = scratch();
        let mut convos = crate::domain::convo::Convos::new();
        for sid in [1, 2] {
            let mut c = Convo {
                session_id: Some(format!("s{sid}")),
                ..Convo::default()
            };
            c.subs.dir = Some(("convo".into(), dir.clone()));
            convos.insert(sid, c);
        }
        convos.get_mut(&1).unwrap().subs.dirty = true;
        std::fs::write(
            dir.join("agent-a.jsonl"),
            line(serde_json::json!({"type":"user","message":{"content":"맡김"}})),
        )
        .unwrap();
        let host = Patches::default();
        let sup = crate::ports::supervisor::fake::FakeSupervisor::default();
        let t0 = Instant::now();
        refresh_due(&host, &sup, &mut convos, t0);
        assert_eq!(
            *host.0.borrow(),
            vec![1],
            "알림 없는 탭까지 읽었거나 새 스레드를 안 그렸다"
        );
        assert!(convos[&1].subs.open, "새 스레드가 패널을 안 열었다");
        // 도는 스레드라 4초 뒤 다시 읽지만 바뀐 것이 없다 — 화면을 갈아 끼우지 않는다.
        refresh_due(&host, &sup, &mut convos, t0 + RUNNING_GAP);
        assert_eq!(host.0.borrow().len(), 1, "같은 화면을 다시 갈아 끼웠다");
        std::fs::write(
            dir.join("agent-a.meta.json"),
            serde_json::json!({"description":"끝","finished":true}).to_string(),
        )
        .unwrap();
        refresh_due(&host, &sup, &mut convos, t0 + RUNNING_GAP * 2);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(*host.0.borrow(), vec![1, 1], "끝난 것을 화면에 안 옮겼다");
        assert!(
            sup.calls.borrow().is_empty(),
            "찾아 둔 폴더를 두고 데몬에 다시 물었다"
        );
    }

    /// 서브에이전트 기록의 도구 호출이 단계로 쌓이고 결과의 `is_error` 가 성패를 붙인다.
    #[test]
    fn a_thread_keeps_its_steps_and_their_outcome() {
        let jsonl = [
            line(serde_json::json!({"type":"user","message":{"content":"찾아라"}})),
            line(
                serde_json::json!({"type":"assistant","timestamp":"2026-11-04T00:00:01Z",
                "message":{"content":[
                {"type":"tool_use","id":"g1","name":"Grep","input":{"pattern":"fn main"}},
                {"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"/w/a.rs"}}]}}),
            ),
            line(serde_json::json!({"type":"user","message":{"content":[
                {"type":"tool_result","tool_use_id":"g1","is_error":true,"content":"x"},
                {"type":"tool_result","tool_use_id":"r1","content":"y"}]}})),
        ]
        .concat();
        let t = fed(&jsonl);
        assert_eq!(t.prompt, "찾아라", "결과 줄이 지시를 덮었다");
        assert_eq!(t.tools, 2);
        let got: Vec<(&str, &str, Option<bool>)> = t
            .steps
            .items
            .iter()
            .map(|s| (s.tool.as_str(), s.summary.as_str(), s.ok))
            .collect();
        assert_eq!(
            got,
            [
                ("Grep", "fn main", Some(false)),
                ("Read", "/w/a.rs", Some(true))
            ]
        );
    }

    /// 프로필 설정 폴더 하나 — 메인 기록 · 서브에이전트 폴더를 만든다.
    struct Fixture {
        root: PathBuf,
        project: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = scratch();
            let project = root.join("projects").join("-w");
            std::fs::create_dir_all(&project).unwrap();
            Self { root, project }
        }
        fn main(&self, convo: &str) -> PathBuf {
            self.project.join(format!("{convo}.jsonl"))
        }
        fn sub(&self, convo: &str, id: &str, started: &str, finished: bool) {
            let d = self.project.join(convo).join("subagents");
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join(format!("{id}.jsonl")),
                line(serde_json::json!({"type":"user","timestamp":started,
                    "message":{"content":format!("{id} 지시")}})),
            )
            .unwrap();
            std::fs::write(
                d.join(format!("{id}.meta.json")),
                serde_json::json!({"description":id,"agentType":"Explore","finished":finished})
                    .to_string(),
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        fn sup(&self, convo: &str) -> crate::ports::supervisor::fake::FakeSupervisor {
            let sup = crate::ports::supervisor::fake::FakeSupervisor::default();
            sup.dev_env
                .borrow_mut()
                .insert("CLAUDE_CONFIG_DIR".into(), self.root.display().to_string());
            *sup.agent_session.borrow_mut() = Some(convo.into());
            sup
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.root) {
                tracing::warn!("시험 폴더를 못 지웠다: {e}");
            }
        }
    }

    fn tool(id: &str, name: &str, input: Value, ts: &str) -> String {
        line(
            serde_json::json!({"type":"assistant","timestamp":ts,"message":{"content":[
            {"type":"tool_use","id":id,"name":name,"input":input}]}}),
        )
    }

    fn compact(ts: &str) -> String {
        line(serde_json::json!({"type":"system","subtype":"compact_boundary","timestamp":ts}))
    }

    fn keys(c: &Convo) -> Vec<String> {
        state(c)["panel"]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["k"].as_str().unwrap().to_string())
            .collect()
    }

    fn main_steps(c: &Convo) -> Vec<String> {
        state(c)["panel"]["cards"][0]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["k"].as_str().unwrap().to_string())
            .collect()
    }

    /// 탭을 열면 마지막 경계 이후만 보인다 — 메인 단계도, 경계 전에 시작한 서브에이전트도 걸러진다.
    /// 세션 행의 백그라운드 작업은 카드가 되고, 카드는 현재 에이전트 → 도는 것 → 끝난 것 순서다.
    #[test]
    fn opening_a_compacted_conversation_shows_only_what_came_after_the_boundary() {
        let fx = Fixture::new();
        std::fs::write(
            fx.main("c1"),
            [
                tool(
                    "m1",
                    "Bash",
                    serde_json::json!({"command":"ls"}),
                    "2026-11-04T00:00:00Z",
                ),
                compact("2026-11-04T00:10:00Z"),
                tool(
                    "m2",
                    "Read",
                    serde_json::json!({"file_path":"/a"}),
                    "2026-11-04T00:11:00Z",
                ),
            ]
            .concat(),
        )
        .unwrap();
        fx.sub("c1", "agent-old", "2026-11-04T00:05:00Z", true);
        fx.sub("c1", "agent-done", "2026-11-04T00:12:00Z", true);
        fx.sub("c1", "agent-run", "2026-11-04T00:13:00Z", false);
        let sup = fx.sup("c1");
        *sup.status_extra.borrow_mut() = Some(serde_json::json!({"background_tasks":[
            {"id":"b1","type":"local_bash","description":"dev server"}]}));
        let mut c = Convo {
            session_id: Some("s".into()),
            agent_session_id: Some("c1".into()),
            ..Convo::default()
        };
        assert!(refresh(&sup, &mut c, Instant::now()));
        assert_eq!(main_steps(&c), ["m2"], "경계 앞 메인 단계가 남았다");
        assert_eq!(
            keys(&c),
            ["main", "b:b1", "s:agent-run", "s:agent-done"],
            "경계 전 서브에이전트가 보이거나 순서가 틀렸다"
        );
        assert_eq!(
            state(&c)["panel"]["title"],
            crate::domain::i18n::t("sup.chat.alog_title")
        );
        assert!(c.subs.open, "새 서브에이전트가 패널을 안 열었다");

        // 이어서 compact — 기록에 새 경계가 붙으면 그 앞 단계 · 서브에이전트 · 끝난 작업이 빠진다.
        c.subs.bg.apply_raw(
            &serde_json::json!({"type":"system","subtype":"task_notification","task_id":"b1",
                "status":"completed"})
            .to_string(),
        );
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(fx.main("c1"))
            .unwrap();
        std::io::Write::write_all(
            &mut f,
            [
                compact("2026-11-04T00:20:00Z"),
                tool(
                    "m3",
                    "Grep",
                    serde_json::json!({"pattern":"x"}),
                    "2026-11-04T00:21:00Z",
                ),
            ]
            .concat()
            .as_bytes(),
        )
        .unwrap();
        fx.sub("c1", "agent-after", "2026-11-04T00:22:00Z", false);
        c.subs.open = false;
        assert!(refresh(&sup, &mut c, Instant::now()));
        assert_eq!(main_steps(&c), ["m3"]);
        assert_eq!(keys(&c), ["main", "s:agent-after"]);
        assert!(c.subs.open, "경계 뒤 첫 서브에이전트가 패널을 안 열었다");

        // `/clear` — 새 대화 id 의 기록으로 처음부터 다시 시작한다.
        std::fs::write(
            fx.main("c2"),
            tool(
                "n1",
                "Bash",
                serde_json::json!({"command":"pwd"}),
                "2026-11-04T01:00:00Z",
            ),
        )
        .unwrap();
        *sup.agent_session.borrow_mut() = Some("c2".into());
        *sup.status_extra.borrow_mut() = None;
        crate::application::clear::reset(&mut c);
        assert!(refresh(&sup, &mut c, Instant::now()));
        assert_eq!(main_steps(&c), ["n1"], "비운 뒤 새 대화의 단계가 아니다");
        assert_eq!(keys(&c), ["main"], "지난 대화의 서브에이전트가 남았다");
    }

    /// 상한을 넘긴 카드는 "N개 생략" 을 싣는다 — 화면 값의 단계 수도 상한이다.
    #[test]
    fn a_card_over_the_step_cap_says_how_many_it_left_out() {
        let fx = Fixture::new();
        let body: String = (0..crate::adapters::driven::agent_log::STEP_CAP + 3)
            .map(|i| {
                tool(
                    &format!("m{i}"),
                    "Bash",
                    serde_json::json!({"command":"true"}),
                    "2026-11-04T00:00:00Z",
                )
            })
            .collect();
        std::fs::write(fx.main("c1"), body).unwrap();
        let sup = fx.sup("c1");
        let mut c = Convo {
            session_id: Some("s".into()),
            agent_session_id: Some("c1".into()),
            ..Convo::default()
        };
        refresh(&sup, &mut c, Instant::now());
        let card = &state(&c)["panel"]["cards"][0];
        assert_eq!(
            card["steps"].as_array().unwrap().len(),
            crate::adapters::driven::agent_log::STEP_CAP
        );
        assert_eq!(
            card["omitted"],
            crate::domain::i18n::t1("sup.chat.alog_omitted", "3")
        );
        assert!(panel_html(&c).contains(r#"<div class="sso">"#));
    }

    /// 백그라운드 작업 줄은 원문으로만 온다 — 그 자리에서 반영하고, 다음 읽기가 화면을 갈아 끼운다.
    #[test]
    fn a_background_task_line_is_patched_on_the_next_read() {
        let dir = scratch();
        let mut convos = crate::domain::convo::Convos::new();
        let mut c = Convo {
            session_id: Some("s".into()),
            ..Convo::default()
        };
        c.subs.dir = Some(("convo".into(), dir.clone()));
        convos.insert(1, c);
        let host = Patches::default();
        let sup = crate::ports::supervisor::fake::FakeSupervisor::default();
        let t0 = Instant::now();
        refresh_due(&host, &sup, &mut convos, t0);
        convos.get_mut(&1).unwrap().subs.on_raw(
            crate::domain::usage::CLAUDE_STREAM_JSON,
            &serde_json::json!({"type":"system","subtype":"task_started","task_id":"b1",
                "task_type":"local_bash","description":"watch"})
            .to_string(),
        );
        refresh_due(&host, &sup, &mut convos, t0 + READ_GAP);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(host.0.borrow().len(), 1, "작업 줄이 화면을 안 바꿨다");
        assert_eq!(keys(&convos[&1]), ["main", "b:b1"]);
    }

    #[test]
    fn main_tool_lines_and_boundaries_signal_a_read() {
        let f = crate::domain::usage::CLAUDE_STREAM_JSON;
        let use_line = serde_json::json!({"type":"assistant","parent_tool_use_id":null,
            "message":{"content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}})
        .to_string();
        let result_line = serde_json::json!({"type":"user","parent_tool_use_id":null,
            "message":{"content":[{"type":"tool_result","tool_use_id":"t"}]}})
        .to_string();
        assert!(signals(f, &use_line));
        assert!(signals(f, &result_line));
        assert!(signals(
            f,
            r#"{"type":"system","subtype":"compact_boundary"}"#
        ));
        assert!(!signals(
            f,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"tool_use"}]}}"#
        ));
    }

    #[test]
    fn a_long_text_is_capped() {
        let s = cap("가".repeat(TEXT_CAP + 5));
        assert_eq!(s.chars().count(), TEXT_CAP + 1);
    }

    /// 링크를 누르면 이 모듈이 받고, 원문 줄과 활동 폴링이 다시 읽기를 부르며, 문서를 그릴 때 줄 · 모양 · 패널이
    /// 실린다 — 하나만 빠져도 패널이 죽거나 저절로 안 열린다.
    #[test]
    fn the_panel_is_drawn_fed_and_routed_here() {
        let shipped = |src: &'static str| &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        let main = crate::test_support::wiring_source();
        assert!(main.contains("subagents::parse(url)"));
        assert!(main.contains("subagents::click("));
        let surface = include_str!("../driving/view.rs");
        let render = &surface[surface.find("fn render(").unwrap()..];
        assert!(
            include_str!("../driving/client.rs")
                .contains("crate::adapters::driven::subagents::state(c)")
        );
        assert!(render.contains("crate::adapters::driven::subagents::STYLE"));
        assert!(render.contains(r#"<div id="subpanelbar"></div>"#));
        let stream = shipped(include_str!("../../application/stream.rs"));
        assert!(stream.contains("convo.subs.on_raw(format, line)"));
        let catch_up = shipped(include_str!("../../application/catch_up.rs"));
        assert!(catch_up.contains("self.refresh_subagents(host)"));
        let tick = &main[main.find("fn activity_polled(").unwrap()..];
        let tick = &tick[..tick.find("\n    }\n").unwrap()];
        assert!(tick.contains("self.refresh_subagents(host)"));
    }
}
