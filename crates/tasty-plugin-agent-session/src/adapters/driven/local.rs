//! 데몬 없이 **plugin 안에서** claude 를 직접 띄우는 [`Supervisor`] 구현.
//!
//! # 계약
//!
//! 자식은 `claude -p --input-format stream-json --output-format stream-json --verbose
//! --include-partial-messages` 로 뜨고, 한 줄 = JSON 하나로 말을 주고받는다.
//!
//! | 하는 일 | stdin 에 쓰는 것 / stdout 에서 읽는 것 |
//! |---|---|
//! | 턴 입력 | `{"type":"user","uuid":<꼬리표>,"message":{"role":"user","content":[{"type":"text","text":…}]}}` |
//! | 입력 수명 | `{"type":"command_lifecycle","command_uuid":<꼬리표>,"state":"started"}` — 그 입력이 실행에 들어갔다 |
//! | 턴 끝 | `{"type":"result",…}` — `is_error` 로 성패 |
//! | 턴 취소 | `{"type":"control_request","request_id":…,"request":{"subtype":"interrupt"}}` |
//! | 권한·질문 | 읽기 `control_request{subtype:"can_use_tool"}` → 답 `control_response{behavior}` |
//!
//! 구독자는 줄마다 `{"result":{"event_raw":{"progress":…,"raw":<원문>,"format":"claude-stream-json"}}}`
//! 를 받는다 — [`crate::application::stream`] 이 같은 모양을 푼다.
//!
//! # 어느 `result` 가 어느 입력의 답인가
//!
//! 자식은 입력 없이도 턴을 연다 — 백그라운드 작업이 끝나면 `system/task_notification` →
//! `system/init` → … → `result` 로 스스로 이어 간다(자가 턴). 턴 끝을 도착 순서로 나눠 주면
//! 자가 턴 도중 보낸 입력이 자가 턴의 답을 받고 이후 답이 한 칸씩 밀린다. 그래서 입력마다
//! `uuid` 꼬리표를 싣고, `command_lifecycle` 을 한 번이라도 본 세션은 **`started` 된 입력만**
//! 턴 끝을 받는다(합쳐진 턴이면 여럿이 같은 답을 받는다). 받을 입력이 없는 턴 끝은 자가 턴이라
//! 구독자와 출력 기록에만 남는다. 수명 이벤트를 내지 않는 claude 는 도착 순서로 짝짓는다.
//!
//! # 상태는 이 프로세스가 쥔다
//!
//! 세션 표는 전역 하나([`registry`])고 [`LocalSupervisor`] 는 그것을 가리키는 핸들이다 —
//! 조립 루트(main.rs)가 만든 핸들을 모듈이 `Supervisor` 포트로 받는다. 레코드(좌표·정책)는
//! `$TASTY_PLUGIN_DATA_DIR/sessions.json` 에 남아 plugin 이 다시 떠도 `--resume` 으로 잇는다.
//! 살아 있는 자식은 plugin 과 함께 내려간다.

mod background;
mod child_env;
mod pi;
mod turns;

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::adapters::driven::shell_path;
use crate::adapters::driven::store::{agents, roles};
use crate::application::prompts::Answer;
use crate::domain::i18n;
use crate::ports::supervisor::{ClientError, SessionRow, SpawnOpts, Supervisor};

use turns::{feed, settle_exit};

/// 원문 구독 프레임의 `format`.
pub const FORMAT: &str = "claude-stream-json";

/// 턴 하나를 기다리는 상한 — 넘기면 [`ClientError::TimedOut`] 이고 턴은 자식에서 계속 돈다.
const TURN_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// 끝난 턴의 답을 몇 개까지 들고 있나(`output`).
const OUTPUT_KEEP: usize = 200;

/// 받는 권한 모드 — 밖이면 자식을 띄우기 전에 거절한다.
pub const PERMISSION_MODES: &[&str] = &[
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 새 세션 id — UUID v4 모양(claude 의 `--session-id` 가 그 모양만 받는다).
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut words = [0u64; 2];
    for (i, w) in words.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_u64(SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        h.write_usize(i);
        h.write_u32(std::process::id());
        *w = h.finish();
    }
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&words[0].to_be_bytes());
    b[8..].copy_from_slice(&words[1].to_be_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 디스크에 남는 것 — plugin 이 다시 떠도 대화로 돌아갈 좌표와 기동 정책.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub id: String,
    pub agent_name: String,
    pub kind: String,
    pub cwd: Option<String>,
    /// claude 쪽 세션 id — 새 세션이면 `id` 와 같고, `/clear` 뒤에는 claude 가 낸 새 id 다.
    pub coord: String,
    /// 좌표에 대화가 실제로 있나 — 첫 `system/init` 을 봤거나 이어받은 대화다.
    pub has_conversation: bool,
    pub name: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub permission_prompts: bool,
    pub add_dirs: Vec<String>,
    /// 고른 역할 폴더 이름(`roles` 명부) — 띄울 때마다 경로로 풀어 얹는다.
    pub roles: Vec<String>,
    pub created_unix: u64,
}

impl Record {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "agent_name": self.agent_name,
            "kind": self.kind,
            "cwd": self.cwd,
            "coord": self.coord,
            "has_conversation": self.has_conversation,
            "name": self.name,
            "model": self.model,
            "permission_mode": self.permission_mode,
            "permission_prompts": self.permission_prompts,
            "add_dirs": self.add_dirs,
            "roles": self.roles,
            "created_unix": self.created_unix,
        })
    }

    fn from_json(v: &Value) -> Option<Self> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            id: s("id")?,
            agent_name: s("agent_name")?,
            kind: s("kind").unwrap_or_else(|| "claude".into()),
            cwd: s("cwd"),
            coord: s("coord")?,
            has_conversation: v
                .get("has_conversation")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            name: s("name"),
            model: s("model"),
            permission_mode: s("permission_mode"),
            permission_prompts: v
                .get("permission_prompts")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            add_dirs: v
                .get("add_dirs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            roles: v
                .get("roles")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            created_unix: v.get("created_unix").and_then(Value::as_u64).unwrap_or(0),
        })
    }
}

/// 기다리는 물음 하나 — 들어온 순서대로 쌓이고 답하면 빠진다.
#[derive(Debug, Clone)]
struct Pending {
    request_id: String,
    kind: &'static str,
    tool: String,
    input: Value,
    since_unix: u64,
}

/// 턴 하나의 결말 — 답 본문 또는 사유.
type TurnOutcome = Result<String, ClientError>;

/// 쓴 입력 하나 — 답을 받을 대기표.
struct Waiter {
    tag: String,
    /// `command_lifecycle started` 를 봤다 — 다음 턴 끝이 이 입력의 답이다.
    started: bool,
    /// `None` = 쓰고 안 기다린다(spawn 프롬프트·broadcast 가 아닌 자리에서 쓴 줄).
    reply: Option<Sender<TurnOutcome>>,
}

impl Waiter {
    fn wake(&self, outcome: TurnOutcome) {
        if let Some(tx) = &self.reply {
            wake(tx, outcome);
        }
    }
}

struct Live {
    child: Child,
    stdin: ChildStdin,
    pid: u32,
    started_unix: u64,
    /// 어느 기동인가 — 내려간 자식의 리더가 새 자식의 상태를 건드리지 않게 한다.
    generation: u64,
}

#[derive(Default)]
struct Inner {
    rec: Record,
    live: Option<Live>,
    generation: u64,
    turn_active: bool,
    /// 턴 밖에서 도는 작업(백그라운드 Bash · Monitor · 하위 에이전트) — [`background`].
    tasks: background::Tasks,
    /// pi 세션의 줄 변환 상태([`pi`]) — claude 세션은 안 쓴다.
    pi: pi::Translate,
    last_event: Option<Instant>,
    turn_text: Vec<String>,
    waiters: VecDeque<Waiter>,
    /// `command_lifecycle` 을 봤다 — 이후 턴 끝은 `started` 된 대기표에만 간다.
    lifecycle_seen: bool,
    /// 쉬어서 놓았다(`reclaim`) — 행이 `reclaimed` 를 싣고, 다시 뜨면(`launch`) 내린다.
    reclaimed: bool,
    prompts: VecDeque<Pending>,
    output: VecDeque<String>,
    output_total: usize,
    subs: Vec<Sender<Value>>,
    stderr_tail: VecDeque<String>,
}

struct Session {
    inner: Mutex<Inner>,
}

impl Session {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // poison 이어도 표를 읽는다 — 버리면 그 세션이 영영 안 보인다.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// 세션 표 — plugin 프로세스에 하나.
pub struct Registry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    store: Option<PathBuf>,
    profiles_dir: Option<PathBuf>,
    roles_file: Option<PathBuf>,
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("TASTY_PLUGIN_DATA_DIR").map(PathBuf::from)
}

impl Registry {
    fn load() -> Self {
        let dir = data_dir();
        let mut reg = Self::at(
            dir.as_ref().map(|d| d.join("sessions.json")),
            dir.as_ref().map(|d| d.join("profiles")),
        );
        reg.roles_file = dir.map(|d| d.join("roles.json"));
        reg
    }

    /// 저장 자리를 정해 연다 — 시험은 임시 폴더를 준다.
    pub fn at(store: Option<PathBuf>, profiles_dir: Option<PathBuf>) -> Self {
        let mut map = HashMap::new();
        if let Some(p) = &store
            && let Ok(text) = std::fs::read_to_string(p)
        {
            match serde_json::from_str::<Value>(&text) {
                Ok(v) => {
                    for r in v
                        .get("sessions")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Record::from_json)
                    {
                        map.insert(
                            r.id.clone(),
                            Arc::new(Session {
                                inner: Mutex::new(Inner {
                                    rec: r,
                                    ..Inner::default()
                                }),
                            }),
                        );
                    }
                }
                Err(e) => tracing::warn!("세션 기록을 못 읽었다({}): {e}", p.display()),
            }
        }
        Self {
            sessions: Mutex::new(map),
            store,
            profiles_dir,
            roles_file: None,
        }
    }

    fn table(&self) -> MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, ClientError> {
        self.table().get(id).cloned().ok_or_else(|| not_found(id))
    }

    fn save(&self) {
        let Some(path) = &self.store else { return };
        let mut recs: Vec<Record> = self
            .table()
            .values()
            .map(|s| s.lock().rec.clone())
            .collect();
        recs.sort_by(|a, b| (a.created_unix, &a.id).cmp(&(b.created_unix, &b.id)));
        let body = json!({ "sessions": recs.iter().map(Record::to_json).collect::<Vec<_>>() });
        let tmp = path.with_extension("json.tmp");
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&tmp, body.to_string()))
            .and_then(|_| std::fs::rename(&tmp, path));
        if let Err(e) = written {
            tracing::warn!("세션 기록을 못 남겼다({}): {e}", path.display());
        }
    }

    fn profiles(&self) -> Vec<agents::AgentProfile> {
        agents::load(self.profiles_dir.as_deref())
    }

    fn roles(&self) -> Vec<roles::Role> {
        roles::load(self.roles_file.as_deref())
    }
}

/// 전역 세션 표.
pub fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(Registry::load)
}

fn not_found(id: &str) -> ClientError {
    ClientError::Daemon {
        kind: "not_found".into(),
        detail: i18n::t1("sup.err.no_such_session", id),
    }
}

fn invalid_state(detail: String) -> ClientError {
    ClientError::Daemon {
        kind: "invalid_state".into(),
        detail,
    }
}

fn bad_request(detail: String) -> ClientError {
    ClientError::Daemon {
        kind: "bad_request".into(),
        detail,
    }
}

/// 세션 행 — 데몬 시절 `session.list` 행과 같은 칸 이름을 쓴다(소비자가 그 이름으로 읽는다).
fn view(inner: &Inner) -> Value {
    let r = &inner.rec;
    let live = inner.live.as_ref();
    let mut v = json!({
        "id": r.id,
        "kind": r.kind,
        "state": if live.is_some() { "running" } else { "stopped" },
        "agent_name": r.agent_name,
        "agent_session_id": r.coord,
        "resumable": live.is_none() && r.has_conversation,
        "has_conversation": r.has_conversation,
        "reclaimed": live.is_none() && inner.reclaimed,
        "add_dirs": r.add_dirs,
        "roles": r.roles,
        "turn_active": inner.turn_active,
        "background_tasks": inner.tasks.to_json(),
        "busy": busy_of(inner),
        "permission_prompts": r.permission_prompts,
    });
    let o = v.as_object_mut().expect("json! 객체");
    for (k, val) in [
        ("cwd", &r.cwd),
        ("name", &r.name),
        ("model", &r.model),
        ("permission_mode", &r.permission_mode),
    ] {
        if let Some(s) = val {
            o.insert(k.into(), Value::String(s.clone()));
        }
    }
    // 마지막 이벤트 뒤 지난 초 — 세션 목록의 "마지막 활동"(`remaining`).
    if let Some(t) = inner.last_event {
        o.insert("idle_secs".into(), json!(t.elapsed().as_secs()));
    }
    if inner.turn_active
        && let Some(t) = inner.last_event
    {
        o.insert("turn_quiet_secs".into(), json!(t.elapsed().as_secs()));
    }
    if let Some(p) = inner.prompts.front() {
        o.insert(
            "needs_input".into(),
            json!({
                "request_id": p.request_id,
                "kind": p.kind,
                "tool": p.tool,
                "input": p.input,
                "since_unix": p.since_unix,
            }),
        );
    }
    if let Some(l) = live {
        o.insert("pid".into(), json!(l.pid));
        o.insert("started_unix".into(), json!(l.started_unix));
    }
    v
}

/// 세션이 일하는 중인가 — 턴 · 기다리는 물음 · 턴 밖 작업 중 하나라도 있으면.
///
/// 자식을 내리면 잃는 것이 있는가와 같은 질문이다. 소비자(탭 표시 · 정지 전 질의 · 회수)는
/// 행의 `busy` 를 읽고, 입력을 보낼지만 정하는 자리는 `turn_active` 를 읽는다.
fn busy_of(inner: &Inner) -> bool {
    inner.live.is_some()
        && (inner.turn_active || !inner.prompts.is_empty() || !inner.tasks.is_empty())
}

fn write_line(inner: &mut Inner, v: &Value) -> Result<(), ClientError> {
    let Some(l) = inner.live.as_mut() else {
        return Err(invalid_state(i18n::t("sup.err.not_running")));
    };
    let mut line = v.to_string();
    line.push('\n');
    l.stdin
        .write_all(line.as_bytes())
        .and_then(|_| l.stdin.flush())
        .map_err(|e| ClientError::Unavailable(e.to_string()))
}

/// 띄울 명령 — 프로필과 레코드에서 만든다. 순서가 계약이다(시험이 잰다).
///
/// 역할 폴더는 claude 면 `--add-dir` 로 얹는다(지침 `CLAUDE.md` 는 [`roles::CLAUDE_ADD_DIR_GUIDE_ENV`]
/// 가 읽게 한다). pi 는 [`build_pi_args`] 가 만든다.
pub fn build_args(
    profile: &agents::AgentProfile,
    rec: &Record,
    resume: bool,
    fresh_name: bool,
    roles: &[roles::Role],
) -> (String, Vec<String>) {
    if profile.kind == "pi" {
        return build_pi_args(profile, rec, fresh_name, roles);
    }
    let mut words = profile.command.split_whitespace().map(str::to_string);
    let program = words.next().unwrap_or_else(|| "claude".into());
    let mut args: Vec<String> = words.collect();
    args.extend(
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
        ]
        .map(str::to_string),
    );
    if resume {
        args.extend(["--resume".into(), rec.coord.clone()]);
    } else {
        args.extend(["--session-id".into(), rec.coord.clone()]);
    }
    if fresh_name && let Some(n) = &rec.name {
        args.extend(["--name".into(), n.clone()]);
    }
    if let Some(m) = rec.model.as_ref().or(profile.model.as_ref()) {
        args.extend(["--model".into(), m.clone()]);
    }
    if let Some(m) = rec
        .permission_mode
        .as_ref()
        .or(profile.permission_mode.as_ref())
    {
        args.extend(["--permission-mode".into(), m.clone()]);
    }
    if let Some(s) = &profile.settings {
        args.extend(["--settings".into(), s.to_string()]);
    }
    if rec.permission_prompts {
        args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
    }
    for d in profile.add_dirs.iter().chain(rec.add_dirs.iter()) {
        args.extend(["--add-dir".into(), d.clone()]);
    }
    for r in roles {
        args.extend(["--add-dir".into(), r.path.clone()]);
    }
    args.extend(profile.args.iter().cloned());
    (program, args)
}

/// pi 의 명령줄 — `--mode rpc` 로 띄우고 좌표를 `--session-id` 로 준다. pi 는 그 id 의 세션이 없으면
/// 만들고 있으면 잇는다(실측 2026-10-30) — 새 세션과 재개가 같은 인자다.
///
/// pi 에는 추가 폴더가 없다 — 역할은 지침 파일을 `--append-system-prompt` 로 싣고, 추가 폴더는 그
/// 목록을 지침 끝에 한 줄로 알린다.
fn build_pi_args(
    profile: &agents::AgentProfile,
    rec: &Record,
    fresh_name: bool,
    roles: &[roles::Role],
) -> (String, Vec<String>) {
    let mut words = profile.command.split_whitespace().map(str::to_string);
    let program = words.next().unwrap_or_else(|| "pi".into());
    let mut args: Vec<String> = words.collect();
    args.extend(pi::RPC_ARGS.iter().map(|a| a.to_string()));
    args.extend(["--session-id".into(), rec.coord.clone()]);
    if fresh_name && let Some(n) = &rec.name {
        args.extend(["--name".into(), n.clone()]);
    }
    if let Some(m) = rec.model.as_ref().or(profile.model.as_ref()) {
        args.extend(["--model".into(), m.clone()]);
    }
    for g in roles.iter().filter_map(roles::Role::guide) {
        args.extend(["--append-system-prompt".into(), g.display().to_string()]);
    }
    let dirs: Vec<&str> = profile
        .add_dirs
        .iter()
        .chain(rec.add_dirs.iter())
        .map(String::as_str)
        .collect();
    if !dirs.is_empty() {
        args.extend([
            "--append-system-prompt".into(),
            format!("Additional working directories: {}", dirs.join(", ")),
        ]);
    }
    args.extend(profile.args.iter().cloned());
    (program, args)
}

/// 고른 역할을 명부에서 푼다 — 명부에서 빠진 역할은 로그를 남기고 건너뛴다(세션은 뜬다).
fn roles_of(reg: &Registry, rec: &Record) -> Vec<roles::Role> {
    if rec.roles.is_empty() {
        return Vec::new();
    }
    let all = reg.roles();
    rec.roles
        .iter()
        .filter_map(|n| {
            let found = all.iter().find(|r| &r.name == n).cloned();
            if found.is_none() {
                tracing::warn!("세션 {}: 역할 {n} 이 명부에 없어 빼고 띄운다", rec.id);
            }
            found
        })
        .collect()
}

/// 자식을 띄운다 — 이미 살아 있으면 그대로 둔다.
fn launch(
    reg: &'static Registry,
    session: &Arc<Session>,
    inner: &mut Inner,
    resume: bool,
    fresh_name: bool,
) -> Result<(), ClientError> {
    if inner.live.is_some() {
        return Ok(());
    }
    inner.reclaimed = false;
    let profiles = reg.profiles();
    let profile = profiles
        .iter()
        .find(|p| p.name == inner.rec.agent_name)
        .cloned()
        .ok_or_else(|| bad_request(i18n::t1("sup.err.unknown_profile", &inner.rec.agent_name)))?;
    let roles = roles_of(reg, &inner.rec);
    let (program, args) = build_args(&profile, &inner.rec, resume, fresh_name, &roles);
    // 앱으로 띄운 tasty 의 PATH 에는 CLI 설치 위치가 없을 수 있다 — 로그인 셸의 PATH 로 찾고 물린다.
    let mut cmd = shell_path::command(&program, profile.env.get("PATH"));
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    child_env::apply(&mut cmd, &profile, !roles.is_empty());
    let cwd = inner
        .rec
        .cwd
        .clone()
        .or_else(|| profile.cwd.clone())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
    if let Some(d) = &cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().map_err(|e| ClientError::Daemon {
        kind: "precondition".into(),
        detail: i18n::tn("sup.err.spawn_failed", &[&program, &e.to_string()]),
    })?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        if let Err(e) = child.kill() {
            tracing::warn!("파이프 없는 자식을 못 거뒀다: {e}");
        }
        reap(&mut child);
        return Err(ClientError::Unavailable("pipe".into()));
    };
    inner.generation += 1;
    let generation = inner.generation;
    inner.stderr_tail.clear();
    inner.pi = pi::Translate::default();
    inner.live = Some(Live {
        pid: child.id(),
        child,
        stdin,
        started_unix: now_unix(),
        generation,
    });
    let s = Arc::clone(session);
    let stderr_reader = std::thread::Builder::new()
        .name("agent-session-stderr".into())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut g = s.lock();
                g.stderr_tail.push_back(line);
                while g.stderr_tail.len() > 50 {
                    g.stderr_tail.pop_front();
                }
            }
        });
    if let Err(e) = stderr_reader {
        // 진단용 꼬리만 잃는다 — 자식은 계속 쓴다.
        tracing::warn!("stderr 리더를 못 띄웠다: {e}");
    }
    let s = Arc::clone(session);
    let spawned = std::thread::Builder::new()
        .name("agent-session-reader".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let changed = {
                    let mut g = s.lock();
                    if g.generation != generation {
                        return;
                    }
                    feed(&mut g, &line)
                };
                if changed {
                    reg.save();
                }
            }
            settle_exit(&mut s.lock(), generation);
        });
    if spawned.is_err() {
        if let Some(mut l) = inner.live.take() {
            stop_child(&mut l.child);
        }
        return Err(ClientError::Unavailable("reader thread".into()));
    }
    Ok(())
}

fn kill(inner: &mut Inner) {
    if let Some(mut l) = inner.live.take() {
        stop_child(&mut l.child);
        let g = l.generation;
        // 리더가 볼 세대를 넘겨 — 내려간 자식의 마지막 줄이 상태를 건드리지 않는다.
        inner.generation = g + 1;
    }
    inner.turn_active = false;
    inner.tasks.clear();
    inner.prompts.clear();
    inner.turn_text.clear();
    for w in inner.waiters.drain(..) {
        w.wake(Err(invalid_state(i18n::t("sup.err.stopped"))));
    }
}

/// 기다리는 턴을 깨운다 — 기다리던 쪽이 상한에 걸려 이미 떠났으면 받을 이가 없다.
fn wake(w: &Sender<TurnOutcome>, outcome: TurnOutcome) {
    if w.send(outcome).is_err() {
        tracing::debug!("턴 결과를 받을 쪽이 이미 떠났다");
    }
}

fn reap(child: &mut Child) {
    if let Err(e) = child.wait() {
        tracing::warn!("자식 종료를 못 거뒀다: {e}");
    }
}

/// 자식을 내린다 — 이미 끝났으면 kill 이 실패하는 것이 정상이다.
fn stop_child(child: &mut Child) {
    if let Err(e) = child.kill() {
        tracing::debug!("자식이 이미 내려가 있었다: {e}");
    }
    reap(child);
}

/// 입력 한 줄 — 세션 종류에 맞는 모양으로.
fn input_line(inner: &Inner, text: &str, tag: &str) -> Value {
    if inner.rec.kind == "pi" {
        pi::input_line(text, tag)
    } else {
        user_line(text, tag)
    }
}

fn user_line(text: &str, tag: &str) -> Value {
    json!({ "type": "user", "uuid": tag, "message": { "role": "user", "content": [{ "type": "text", "text": text }] } })
}

/// 대기표를 걸고 입력 한 줄을 쓴다 — 쓰기가 실패하면 대기표를 거둔다.
fn write_input(
    inner: &mut Inner,
    text: &str,
    reply: Option<Sender<TurnOutcome>>,
) -> Result<(), ClientError> {
    let tag = new_id();
    inner.waiters.push_back(Waiter {
        tag: tag.clone(),
        started: false,
        reply,
    });
    inner.turn_active = true;
    let line = input_line(inner, text, &tag);
    if let Err(e) = write_line(inner, &line) {
        inner.waiters.retain(|w| w.tag != tag);
        inner.turn_active = !inner.waiters.is_empty();
        return Err(e);
    }
    Ok(())
}

/// 전역 세션 표를 가리키는 핸들 — 몇 번 만들어도 같은 표를 본다.
///
/// 복사가 공짜인 값이라 조립 루트(main.rs)가 하나 만들어 plugin 이 들고, 워커 스레드에는 복사본을
/// 넘긴다. 표는 처음 쓸 때 연다 — 핸들을 만드는 것만으로 `sessions.json` 을 읽지 않는다. 핸들을
/// 조립 루트에서 미리 만들게 되면서도, 핸들러가 그때그때 만들던 때와 표를 여는 시점이 같다.
#[derive(Clone, Copy)]
pub struct LocalSupervisor {
    /// 시험이 준 표. 없으면 전역 표([`registry`])를 쓴다.
    fixed: Option<&'static Registry>,
}

impl LocalSupervisor {
    pub fn new() -> Self {
        Self { fixed: None }
    }

    /// 지정한 표를 쓴다 — 시험이 임시 폴더의 표를 준다.
    #[cfg(test)]
    fn with(reg: &'static Registry) -> Self {
        Self { fixed: Some(reg) }
    }

    /// 빈 임시 표의 핸들 — plugin 껍데기 시험이 사용자의 실물 표를 건드리지 않게 한다. 세션이 없어 묻는 것마다
    /// 데몬처럼 거절한다.
    #[cfg(test)]
    pub(crate) fn empty_for_test(tag: &str) -> Self {
        // 같은 시험 안에서 두 번 불러도 표가 섞이지 않게 부를 때마다 번호를 올린다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "agent-session-empty-{tag}-{}-{n}",
            std::process::id()
        ));
        Self::with(Box::leak(Box::new(Registry::at(
            Some(dir.join("sessions.json")),
            Some(dir.join("profiles")),
        ))))
    }

    fn reg(&self) -> &'static Registry {
        self.fixed.unwrap_or_else(registry)
    }

    /// plugin 이 내려갈 때 자식을 모두 거둔다.
    pub fn shutdown() {
        for s in registry().table().values() {
            kill(&mut s.lock());
        }
    }

    fn send_turn(&self, id: &str, text: &str) -> Result<Receiver<TurnOutcome>, ClientError> {
        let s = self.reg().get(id)?;
        let mut g = s.lock();
        if g.live.is_none() {
            if !g.rec.has_conversation {
                // 대화가 없는 좌표는 `--resume` 이 거절한다 — 같은 id 로 새로 연다.
                launch(self.reg(), &s, &mut g, false, true)?;
            } else {
                launch(self.reg(), &s, &mut g, true, false)?;
            }
        }
        let (tx, rx) = mpsc::channel();
        g.last_event = Some(Instant::now());
        write_input(&mut g, text, Some(tx))?;
        Ok(rx)
    }
}

impl Supervisor for LocalSupervisor {
    fn subscribe(&self, id: &str) -> Result<Receiver<Value>, ClientError> {
        let s = self.reg().get(id)?;
        let (tx, rx) = mpsc::channel();
        s.lock().subs.push(tx);
        Ok(rx)
    }

    fn endpoint(&self) -> String {
        self.reg()
            .store
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "local".into())
    }

    fn list(&self) -> Result<(Vec<SessionRow>, usize), ClientError> {
        let mut views: Vec<(u64, Value)> = self
            .reg()
            .table()
            .values()
            .map(|s| {
                let g = s.lock();
                (g.rec.created_unix, view(&g))
            })
            .collect();
        views.sort_by_key(|a| a.0);
        let mut rows = Vec::new();
        let mut bad = 0;
        for (_, v) in views {
            match SessionRow::from_value(&v) {
                Some(r) => rows.push(r),
                None => bad += 1,
            }
        }
        Ok((rows, bad))
    }

    fn status(&self, id: &str) -> Result<Value, ClientError> {
        Ok(view(&self.reg().get(id)?.lock()))
    }

    fn stop(&self, id: &str) -> Result<(), ClientError> {
        kill(&mut self.reg().get(id)?.lock());
        Ok(())
    }

    fn reclaim(&self, id: &str) -> Result<(), ClientError> {
        let s = self.reg().get(id)?;
        let mut g = s.lock();
        kill(&mut g);
        g.reclaimed = true;
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), ClientError> {
        let s = self.reg().get(id)?;
        if s.lock().live.is_some() {
            return Err(invalid_state(i18n::t("sup.err.delete_live")));
        }
        self.reg().table().remove(id);
        self.reg().save();
        Ok(())
    }

    fn abort(&self, id: &str) -> Result<(), ClientError> {
        let s = self.reg().get(id)?;
        let mut g = s.lock();
        if g.live.is_none() {
            return Ok(());
        }
        let line = if g.rec.kind == "pi" {
            pi::abort_line()
        } else {
            json!({ "type": "control_request", "request_id": "abort", "request": { "subtype": "interrupt" } })
        };
        write_line(&mut g, &line)
    }

    fn agent_list(&self) -> Result<Vec<agents::AgentProfile>, ClientError> {
        Ok(self.reg().profiles())
    }

    fn spawn(&self, opts: &SpawnOpts<'_>) -> Result<Value, ClientError> {
        let profiles = self.reg().profiles();
        let profile = profiles
            .iter()
            .find(|p| p.name == opts.agent)
            .ok_or_else(|| bad_request(i18n::t1("sup.err.unknown_profile", opts.agent)))?;
        if !agents::supported(&profile.kind) {
            return Err(ClientError::Daemon {
                kind: "precondition".into(),
                detail: i18n::t1("sup.err.kind_unsupported", &profile.kind),
            });
        }
        if let Some(m) = opts.permission_mode
            && !PERMISSION_MODES.contains(&m)
        {
            return Err(bad_request(i18n::t1("sup.err.bad_permission_mode", m)));
        }
        let known_roles = self.reg().roles();
        roles::resolve(&known_roles, opts.roles)
            .map_err(|missing| bad_request(i18n::t1("sup.role.unknown", &missing.join(", "))))?;
        let mut add_dirs = Vec::new();
        for d in opts.add_dirs {
            let p = std::fs::canonicalize(d).map_err(|e| bad_request(format!("{d}: {e}")))?;
            add_dirs.push(p.display().to_string());
        }
        // 같은 대화를 이미 쥔 살아 있는 세션이 있으면 그것에 붙는다 — 한 대화에 자식 둘을 안 띄운다.
        if let Some(coord) = opts.adopt {
            let found = self.reg().table().values().find_map(|s| {
                let g = s.lock();
                (g.live.is_some() && g.rec.coord == coord).then(|| view(&g))
            });
            if let Some(mut v) = found {
                v["reused"] = Value::Bool(true);
                return Ok(v);
            }
        }
        let id = new_id();
        let rec = Record {
            id: id.clone(),
            agent_name: profile.name.clone(),
            kind: profile.kind.clone(),
            cwd: opts.cwd.map(str::to_string),
            coord: opts.adopt.map_or_else(|| id.clone(), str::to_string),
            has_conversation: opts.adopt.is_some(),
            name: opts.name.map(str::to_string),
            model: opts.model.map(str::to_string),
            permission_mode: opts.permission_mode.map(str::to_string),
            // pi 는 권한을 묻지 않는다 — 켜 두면 탭이 오지 않을 물음을 기다린다.
            permission_prompts: opts.permission_prompts && profile.kind == "claude",
            add_dirs,
            roles: opts.roles.to_vec(),
            created_unix: now_unix(),
        };
        let session = Arc::new(Session {
            inner: Mutex::new(Inner {
                rec,
                ..Inner::default()
            }),
        });
        {
            let mut g = session.lock();
            launch(self.reg(), &session, &mut g, opts.adopt.is_some(), true)?;
        }
        self.reg().table().insert(id.clone(), Arc::clone(&session));
        self.reg().save();
        if let Some(p) = opts.prompt {
            write_input(&mut session.lock(), p, None)?;
        }
        let mut v = view(&session.lock());
        v["reused"] = Value::Bool(false);
        v["warnings"] = json!([]);
        Ok(v)
    }

    fn add_dir(&self, id: &str, path: &str) -> Result<Value, ClientError> {
        let p = std::fs::canonicalize(path).map_err(|e| bad_request(format!("{path}: {e}")))?;
        self.change_dir(id, p.display().to_string(), true)
    }

    fn remove_dir(&self, id: &str, path: &str) -> Result<Value, ClientError> {
        let p = std::fs::canonicalize(path)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());
        self.change_dir(id, p, false)
    }

    fn respond(&self, id: &str, answer: &Answer) -> Result<(), ClientError> {
        let s = self.reg().get(id)?;
        let mut g = s.lock();
        let Some(pos) = g
            .prompts
            .iter()
            .position(|p| p.request_id == answer.request_id)
        else {
            return Err(invalid_state(i18n::t1(
                "sup.err.no_such_prompt",
                &answer.request_id,
            )));
        };
        let pending = g.prompts[pos].clone();
        let body = if answer.decision == "allow" {
            json!({ "behavior": "allow", "updatedInput": answer.updated_input.clone().unwrap_or(pending.input) })
        } else {
            json!({ "behavior": "deny", "message": answer.message.clone().unwrap_or_else(|| i18n::t("sup.prompt.denied")) })
        };
        write_line(
            &mut g,
            &json!({ "type": "control_response", "response": {
                "subtype": "success", "request_id": answer.request_id, "response": body,
            } }),
        )?;
        g.prompts.remove(pos);
        Ok(())
    }

    fn send_input(&self, id: &str, text: &str) -> Result<Value, ClientError> {
        let rx = self.send_turn(id, text)?;
        match rx.recv_timeout(TURN_TIMEOUT) {
            Ok(Ok(text)) => Ok(json!({ "output": { "text": text } })),
            Ok(Err(e)) => Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ClientError::TimedOut(TURN_TIMEOUT)),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ClientError::Daemon {
                kind: "failed".into(),
                detail: i18n::t("sup.err.stream_ended"),
            }),
        }
    }

    fn broadcast(&self, text: &str, agent: Option<&str>) -> Result<Value, ClientError> {
        let ids: Vec<String> = self
            .reg()
            .table()
            .values()
            .filter_map(|s| {
                let g = s.lock();
                (g.live.is_some() && agent.is_none_or(|a| a == g.rec.agent_name))
                    .then(|| g.rec.id.clone())
            })
            .collect();
        let mut sent = Vec::new();
        let mut failed = Vec::new();
        let waits: Vec<(String, Result<Receiver<TurnOutcome>, ClientError>)> = ids
            .into_iter()
            .map(|id| {
                let r = self.send_turn(&id, text);
                (id, r)
            })
            .collect();
        for (id, r) in waits {
            match r.and_then(|rx| {
                rx.recv_timeout(TURN_TIMEOUT)
                    .unwrap_or(Err(ClientError::TimedOut(TURN_TIMEOUT)))
            }) {
                Ok(t) => sent.push(json!({ "id": id, "output": { "text": t } })),
                Err(e) => failed.push(json!({ "id": id, "error": e.to_string() })),
            }
        }
        Ok(json!({ "sent": sent, "failed": failed }))
    }

    fn output(&self, id: &str, lines: Option<usize>) -> Result<Value, ClientError> {
        let s = self.reg().get(id)?;
        let g = s.lock();
        let n = lines.unwrap_or(g.output.len()).min(g.output.len());
        let tail: Vec<&String> = g.output.iter().skip(g.output.len() - n).collect();
        Ok(json!({ "lines": tail, "total": g.output_total }))
    }

    fn watch(
        &self,
        id: &str,
        max_events: usize,
        until_finished: bool,
        budget: Duration,
    ) -> Result<(Vec<Value>, bool), ClientError> {
        let rx = self.subscribe(id)?;
        let deadline = Instant::now() + budget;
        let mut events = Vec::new();
        let mut finished_seen = false;
        while events.len() < max_events {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let Ok(f) = rx.recv_timeout(left) else { break };
            let Some(ev) = f.pointer("/result/event_raw/progress").cloned() else {
                continue;
            };
            let finished = ev.get("phase").and_then(Value::as_str) == Some("finished");
            events.push(ev);
            if finished {
                finished_seen = true;
                if until_finished {
                    break;
                }
            }
        }
        Ok((events, finished_seen))
    }
}

impl LocalSupervisor {
    /// 추가 작업 폴더를 바꾼다 — 살아 있으면 자식을 내렸다 `--resume` 으로 다시 띄운다(대화는 그대로).
    fn change_dir(&self, id: &str, path: String, add: bool) -> Result<Value, ClientError> {
        let s = self.reg().get(id)?;
        let mut g = s.lock();
        if g.turn_active {
            return Err(invalid_state(i18n::t("sup.err.turn_running")));
        }
        // 다시 띄우면 턴 밖 작업이 함께 죽는다.
        if !g.tasks.is_empty() {
            return Err(invalid_state(i18n::t("sup.err.background_running")));
        }
        let before = g.rec.add_dirs.clone();
        if add {
            if !g.rec.add_dirs.contains(&path) {
                g.rec.add_dirs.push(path);
            }
        } else {
            g.rec.add_dirs.retain(|d| *d != path);
        }
        let changed = g.rec.add_dirs != before;
        let mut applied = false;
        if changed && g.live.is_some() {
            kill(&mut g);
            let resume = g.rec.has_conversation;
            launch(self.reg(), &s, &mut g, resume, false)?;
            applied = true;
        }
        let dirs = g.rec.add_dirs.clone();
        drop(g);
        if changed {
            self.reg().save();
        }
        Ok(json!({ "add_dirs": { "dirs": dirs, "changed": changed, "applied": applied } }))
    }
}

#[cfg(test)]
mod tests {
    use super::turns::{apply_line, progress_of};
    use super::*;

    fn line(v: Value) -> String {
        v.to_string()
    }

    #[test]
    fn ids_are_uuid_v4_shaped_and_distinct() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'), "v4 표지: {a}");
        assert!(
            matches!(&parts[3][..1], "8" | "9" | "a" | "b"),
            "변형 표지: {a}"
        );
    }

    #[test]
    fn progress_maps_the_stream_json_types() {
        assert_eq!(
            progress_of(&json!({"type":"system","subtype":"init"}))["phase"],
            "started"
        );
        let tool =
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}});
        assert_eq!(
            progress_of(&tool),
            json!({"phase":"tool_use","name":"Bash"})
        );
        let text =
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"안녕"}]}});
        assert_eq!(progress_of(&text), json!({"phase":"output","text":"안녕"}));
        let delta =
            json!({"type":"stream_event","event":{"delta":{"type":"text_delta","text":"a"}}});
        assert_eq!(
            progress_of(&delta),
            json!({"phase":"text_delta","text":"a"})
        );
        let failed = json!({"type":"result","is_error":true,"errors":["x","y"]});
        assert_eq!(
            progress_of(&failed),
            json!({"phase":"finished","ok":false,"reason":"x · y"})
        );
        assert_eq!(
            progress_of(&json!({"type":"rate_limit_event"})),
            json!({"phase":"other","kind":"rate_limit_event"})
        );
    }

    #[test]
    fn a_result_line_answers_the_oldest_waiter_with_the_joined_text() {
        let mut g = Inner::default();
        let (tx, rx) = mpsc::channel();
        g.waiters.push_back(waiter("u1", Some(tx)));
        for t in ["첫째", "둘째"] {
            apply_line(
                &mut g,
                &line(json!({"type":"assistant","message":{"content":[{"type":"text","text":t}]}})),
            );
        }
        apply_line(&mut g, &line(json!({"type":"result","is_error":false})));
        assert_eq!(rx.recv().unwrap().unwrap(), "첫째\n\n둘째");
        assert_eq!(g.output.back().map(String::as_str), Some("첫째\n\n둘째"));
        assert!(!g.turn_active);
    }

    /// API 오류로 끝난 턴은 `subtype` 이 `success` 다 — 사유에 본문을 실어야 429 를 알아보고,
    /// `success` 는 실패 사유로 읽히지 않게 뺀다.
    #[test]
    fn a_rejected_turn_carries_its_text_in_the_reason() {
        let mut g = Inner::default();
        let (tx, rx) = mpsc::channel();
        g.waiters.push_back(waiter("u1", Some(tx)));
        apply_line(
            &mut g,
            &said(
                "API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later.",
            ),
        );
        apply_line(
            &mut g,
            &line(json!({"type":"result","subtype":"success","is_error":true})),
        );
        let why = rx.recv().unwrap().unwrap_err().to_string();
        assert!(crate::domain::retry_429::is_rate_limited(&why), "{why}");
        assert!(why.starts_with("API Error"), "{why}");
        assert!(
            !why.contains("success"),
            "실패 사유에 success 가 남았다: {why}"
        );
    }

    /// pi 의 실행 하나는 그 사이 쌓인 입력(`followUp`)을 함께 처리한다 — 바꾼 `result` 하나가
    /// 기다리는 입력 **모두**를 깨운다. 도착 순서로 짝지으면 둘째 입력은 오지 않을 답을 기다린다.
    #[test]
    fn one_settled_pi_run_answers_every_queued_input() {
        let mut g = Inner::default();
        g.rec.kind = "pi".into();
        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        g.waiters.push_back(waiter("u1", Some(tx1)));
        g.waiters.push_back(waiter("u2", Some(tx2)));
        for l in [
            json!({"type":"agent_start"}),
            json!({"type":"message_end","message":{"role":"assistant","stopReason":"stop",
                "content":[{"type":"text","text":"둘 다"}]}}),
            json!({"type":"agent_settled"}),
        ] {
            feed(&mut g, &l.to_string());
        }
        assert_eq!(rx1.try_recv().unwrap().unwrap(), "둘 다");
        assert_eq!(
            rx2.try_recv().unwrap().unwrap(),
            "둘 다",
            "쌓인 입력이 답을 못 받았다"
        );
        assert!(g.waiters.is_empty());
    }

    fn waiter(tag: &str, reply: Option<Sender<TurnOutcome>>) -> Waiter {
        Waiter {
            tag: tag.into(),
            started: false,
            reply,
        }
    }

    fn said(t: &str) -> String {
        line(json!({"type":"assistant","message":{"content":[{"type":"text","text":t}]}}))
    }

    fn started(tag: &str) -> String {
        line(json!({"type":"command_lifecycle","command_uuid":tag,"state":"started"}))
    }

    /// 자가 턴 도중 쓴 입력은 자가 턴의 `result` 를 받지 않고 제 턴의 답을 받는다.
    #[test]
    fn a_self_opened_turn_does_not_take_a_pending_reply() {
        let mut g = Inner {
            lifecycle_seen: true,
            ..Default::default()
        };
        apply_line(
            &mut g,
            &line(json!({"type":"system","subtype":"task_notification"})),
        );
        apply_line(
            &mut g,
            &line(json!({"type":"system","subtype":"init","session_id":"s"})),
        );
        let (tx, rx) = mpsc::channel();
        g.waiters.push_back(waiter("u1", Some(tx)));
        apply_line(&mut g, &said("자가"));
        apply_line(&mut g, &line(json!({"type":"result","is_error":false})));
        assert!(rx.try_recv().is_err(), "자가 턴의 답이 입력에 갔다");
        assert!(g.turn_active, "기다리는 입력이 있으면 아직 바쁘다");
        assert_eq!(g.output.back().map(String::as_str), Some("자가"));

        apply_line(&mut g, &started("u1"));
        apply_line(&mut g, &said("내 답"));
        apply_line(&mut g, &line(json!({"type":"result","is_error":false})));
        assert_eq!(rx.try_recv().unwrap().unwrap(), "내 답");
        assert!(!g.turn_active);
    }

    /// 한 턴에 합쳐진 입력은 모두 그 턴의 답을 받고, 아직 시작 안 한 입력은 남는다.
    #[test]
    fn a_merged_turn_answers_every_started_input_and_only_those() {
        let mut g = Inner::default();
        let (a, ra) = mpsc::channel();
        let (b, rb) = mpsc::channel();
        let (c, rc) = mpsc::channel();
        g.waiters.push_back(waiter("a", Some(a)));
        g.waiters.push_back(waiter("b", Some(b)));
        g.waiters.push_back(waiter("c", Some(c)));
        apply_line(&mut g, &started("a"));
        apply_line(&mut g, &started("b"));
        apply_line(&mut g, &said("합침"));
        apply_line(&mut g, &line(json!({"type":"result","is_error":false})));
        // 답은 apply_line 안에서 바로 간다 — recv 로 기다리면 결함이 실패 대신 멈춤이 된다.
        assert_eq!(ra.try_recv().unwrap().unwrap(), "합침");
        assert_eq!(rb.try_recv().unwrap().unwrap(), "합침");
        assert!(rc.try_recv().is_err());
        assert_eq!(g.waiters.len(), 1);
    }

    #[test]
    fn init_records_the_coordinate_claude_reports() {
        let mut g = Inner::default();
        g.rec.coord = "old".into();
        let changed = apply_line(
            &mut g,
            &line(json!({"type":"system","subtype":"init","session_id":"new"})),
        );
        assert!(changed);
        assert_eq!(g.rec.coord, "new");
        assert!(g.rec.has_conversation);
        assert!(g.turn_active);
    }

    #[test]
    fn permission_requests_queue_and_cancel() {
        let mut g = Inner::default();
        apply_line(
            &mut g,
            &line(
                json!({"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}),
            ),
        );
        apply_line(
            &mut g,
            &line(
                json!({"type":"control_request","request_id":"r2","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{}}}),
            ),
        );
        let v = view(&g);
        assert_eq!(v["needs_input"]["request_id"], "r1");
        assert_eq!(v["needs_input"]["kind"], "permission");
        assert_eq!(v["needs_input"]["tool"], "Bash");
        apply_line(
            &mut g,
            &line(json!({"type":"control_cancel_request","request_id":"r1"})),
        );
        let v = view(&g);
        assert_eq!(v["needs_input"]["request_id"], "r2");
        assert_eq!(v["needs_input"]["kind"], "question");
    }

    #[test]
    fn subscribers_get_the_wrapped_raw_line_and_dead_ones_are_dropped() {
        let mut g = Inner::default();
        let (tx, rx) = mpsc::channel();
        g.subs.push(tx);
        let (dead, gone) = mpsc::channel();
        drop(gone);
        g.subs.push(dead);
        let raw = line(json!({"type":"system","subtype":"init","session_id":"s"}));
        apply_line(&mut g, &raw);
        let f = rx.recv().unwrap();
        assert_eq!(f["result"]["event_raw"]["format"], FORMAT);
        assert_eq!(f["result"]["event_raw"]["raw"], raw);
        assert_eq!(f["result"]["event_raw"]["progress"]["phase"], "started");
        assert_eq!(g.subs.len(), 1, "받을 쪽이 사라진 구독은 빠진다");
    }

    #[test]
    fn args_follow_the_documented_order() {
        let profile = crate::adapters::driven::store::agents::AgentProfile {
            name: "cc".into(),
            kind: "claude".into(),
            command: "claude".into(),
            model: Some("opus".into()),
            settings: Some(json!({"a":1})),
            add_dirs: vec!["/p".into()],
            args: vec!["--x".into()],
            ..Default::default()
        };
        let rec = Record {
            coord: "c".into(),
            name: Some("n".into()),
            permission_mode: Some("plan".into()),
            permission_prompts: true,
            add_dirs: vec!["/r".into()],
            ..Default::default()
        };
        let (prog, args) = build_args(&profile, &rec, false, true, &[]);
        assert_eq!(prog, "claude");
        assert_eq!(
            args.join(" "),
            "-p --input-format stream-json --output-format stream-json --verbose \
             --include-partial-messages --session-id c --name n --model opus \
             --permission-mode plan --settings {\"a\":1} --permission-prompt-tool stdio \
             --add-dir /p --add-dir /r --x"
        );
        let (_, resumed) = build_args(&profile, &rec, true, false, &[]);
        let resumed = resumed.join(" ");
        assert!(resumed.contains("--resume c"), "{resumed}");
        assert!(
            !resumed.contains("--name"),
            "재개는 이름을 다시 안 단다: {resumed}"
        );
    }

    #[test]
    fn records_round_trip_through_json() {
        let r = Record {
            id: "i".into(),
            agent_name: "cc".into(),
            kind: "claude".into(),
            cwd: Some("/w".into()),
            coord: "c".into(),
            has_conversation: true,
            name: None,
            model: Some("m".into()),
            permission_mode: None,
            permission_prompts: true,
            add_dirs: vec!["/a".into()],
            roles: vec!["builder".into()],
            created_unix: 7,
        };
        assert_eq!(Record::from_json(&r.to_json()), Some(r));
    }

    #[test]
    fn child_env_drops_host_markers() {
        for k in [
            "TASTY_SURFACE_ID",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_PID",
            "CLAUDECODE",
        ] {
            assert!(child_env::stripped_env(k, std::ffi::OsStr::new("1")), "{k}");
        }
        for k in ["HOME", "CLAUDE_CONFIG_DIR", "PATH"] {
            assert!(
                !child_env::stripped_env(k, std::ffi::OsStr::new("1")),
                "{k}"
            );
        }
    }

    /// stream-json 을 흉내 내는 가짜 claude — 받은 말을 인자와 함께 되돌려 주고, `perm` 이면 권한을 묻는다.
    #[cfg(unix)]
    const FAKE_CLAUDE: &str = r#"#!/bin/sh
ARGS="$*"
sid=""
while [ $# -gt 0 ]; do
  case "$1" in --session-id|--resume) sid="$2"; shift ;; esac
  shift
done
turn() {
  text=$(printf '%s' "$1" | sed -n 's/.*"text":"\([^"]*\)".*/\1/p')
  tag=$(printf '%s' "$1" | sed -n 's/.*"uuid":"\([^"]*\)".*/\1/p')
  printf '{"type":"command_lifecycle","command_uuid":"%s","state":"started"}\n' "$tag"
  printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
  if [ "$text" = perm ]; then
    printf '{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}\n'
    IFS= read -r ans
    case "$ans" in *'"behavior":"allow"'*) out=allowed ;; *) out=denied ;; esac
  else
    out="echo:$text args:$ARGS guide:${CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD:-}"
  fi
  printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' "$out"
  if [ "$text" = endbg ]; then
    printf '{"type":"system","subtype":"background_tasks_changed","tasks":[]}\n'
  fi
  printf '{"type":"result","subtype":"success","is_error":false}\n'
  if [ "$text" = spawnbg ]; then
    # 턴은 끝났지만 백그라운드 작업이 남는다(claude 2.1.308 실측 줄).
    printf '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1","task_type":"local_bash","description":"srv"}]}\n'
  fi
  if [ "$text" = bg ]; then
    # 백그라운드 작업이 끝나 스스로 연 턴 — 다음 입력이 들어온 뒤에야 끝난다.
    printf '{"type":"system","subtype":"task_notification"}\n'
    printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
    printf '{"type":"assistant","message":{"content":[{"type":"text","text":"self"}]}}\n'
    IFS= read -r next
    printf '{"type":"result","subtype":"success","is_error":false}\n'
    turn "$next"
  fi
}
while IFS= read -r line; do
  case "$line" in
    *'"type":"user"'*) turn "$line" ;;
  esac
done
"#;

    /// 가짜 claude 를 프로필로 둔 임시 표.
    #[cfg(unix)]
    fn fake_registry(tag: &str) -> (&'static Registry, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "agent-session-local-{tag}-{}-{n}",
            std::process::id()
        ));
        let profiles = dir.join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let script = dir.join("fake-claude");
        std::fs::write(&script, FAKE_CLAUDE).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let body =
            json!({ "command": script.display().to_string(), "cwd": dir.display().to_string() });
        std::fs::write(profiles.join("fake.json"), body.to_string()).unwrap();
        let reg = Box::leak(Box::new(Registry::at(
            Some(dir.join("sessions.json")),
            Some(profiles),
        )));
        (reg, dir)
    }

    #[cfg(unix)]
    fn answer(v: &Value) -> String {
        v.pointer("/output/text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// 실제 자식 프로세스로 한 바퀴 — 띄우고 · 말하고 · 멈추고 · `--resume` 으로 되살린다.
    #[cfg(unix)]
    #[test]
    fn a_session_runs_a_turn_stops_and_resumes_with_its_coordinate() {
        let (reg, dir) = fake_registry("cycle");
        let sup = LocalSupervisor::with(reg);
        let spawned = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                ..Default::default()
            })
            .expect("spawn");
        let id = spawned["id"].as_str().unwrap().to_string();
        assert_eq!(spawned["state"], "running");
        assert_eq!(spawned["reused"], false);

        let first = answer(&sup.send_input(&id, "hi").expect("첫 턴"));
        assert!(first.starts_with("echo:hi"), "{first}");
        assert!(
            first.contains(&format!("--session-id {id}")),
            "새 세션은 id 를 좌표로 쓴다: {first}"
        );
        assert!(first.contains("--input-format stream-json"), "{first}");

        sup.stop(&id).expect("stop");
        let st = sup.status(&id).unwrap();
        assert_eq!(st["state"], "stopped");
        assert_eq!(st["resumable"], true, "대화를 본 세션은 재개할 수 있다");

        let again = answer(&sup.send_input(&id, "back").expect("재개 턴"));
        assert!(
            again.contains(&format!("--resume {id}")),
            "멈춘 세션은 --resume 으로 뜬다: {again}"
        );
        assert!(!again.contains("--session-id"), "{again}");

        let out = sup.output(&id, Some(1)).unwrap();
        assert_eq!(out["total"], 2);
        assert!(out["lines"][0].as_str().unwrap().starts_with("echo:back"));

        // 기록이 디스크에 남아 다시 읽힌다.
        let reread = Registry::at(Some(dir.join("sessions.json")), None);
        assert!(reread.get(&id).is_ok(), "세션 기록이 저장되지 않았다");

        sup.reclaim(&id).expect("reclaim");
        let st = sup.status(&id).unwrap();
        assert_eq!(st["state"], "stopped");
        assert_eq!(
            st["reclaimed"], true,
            "쉬어서 놓은 세션은 행이 표지를 싣는다"
        );
        assert_eq!(st["resumable"], true);
        let woke = answer(&sup.send_input(&id, "woke").expect("회수 뒤 재개 턴"));
        assert!(woke.contains(&format!("--resume {id}")), "{woke}");
        assert_eq!(
            sup.status(&id).unwrap()["reclaimed"],
            false,
            "다시 뜨면 표지를 내린다"
        );

        sup.stop(&id).unwrap();
        assert_eq!(
            sup.status(&id).unwrap()["reclaimed"],
            false,
            "손으로 멈춘 것은 회수가 아니다"
        );
        sup.delete(&id).expect("멈춘 세션은 지운다");
        assert!(sup.status(&id).is_err());
    }

    /// rpc 를 흉내 내는 가짜 pi — 받은 말을 좌표 · 인자와 함께 되돌리고, `new_session` 뒤 `get_state` 에
    /// 새 좌표를 낸다.
    #[cfg(unix)]
    const FAKE_PI: &str = r#"#!/bin/sh
ARGS="$*"
sid=""
while [ $# -gt 0 ]; do
  case "$1" in --session-id) sid="$2"; shift ;; esac
  shift
done
while IFS= read -r line; do
  case "$line" in
    *'"type":"prompt"'*)
      msg=$(printf '%s' "$line" | sed -n 's/.*"message":"\([^"]*\)".*/\1/p')
      printf '{"type":"response","command":"prompt","success":true}\n'
      printf '{"type":"agent_start"}\n'
      printf '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"pi"}}\n'
      printf '{"type":"message_end","message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"pi:%s sid:%s args:%s"}]}}\n' "$msg" "$sid" "$ARGS"
      printf '{"type":"agent_settled"}\n' ;;
    *'"type":"new_session"'*)
      sid="pi-new-session"
      printf '{"type":"response","command":"new_session","success":true,"data":{"cancelled":false}}\n' ;;
    *'"type":"get_state"'*)
      printf '{"type":"response","command":"get_state","success":true,"data":{"sessionId":"%s"}}\n' "$sid" ;;
  esac
done
"#;

    /// 가짜 실행 파일을 프로필로 둔 임시 표 — 역할 `builder`(지침 있음)를 명부에 둔다.
    #[cfg(unix)]
    fn registry_with(tag: &str, agent_type: &str, script: &str) -> (&'static Registry, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "agent-session-kind-{tag}-{}-{n}",
            std::process::id()
        ));
        let profiles = dir.join("profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        let exe = dir.join("fake-agent");
        std::fs::write(&exe, script).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let body = json!({ "agent_type": agent_type, "command": exe.display().to_string(),
                           "cwd": dir.display().to_string() });
        std::fs::write(profiles.join("fake.json"), body.to_string()).unwrap();
        let role = dir.join("roles").join("builder");
        std::fs::create_dir_all(&role).unwrap();
        std::fs::write(role.join("CLAUDE.md"), "# builder").unwrap();
        let roles_file = dir.join("roles.json");
        crate::adapters::driven::store::roles::add(&roles_file, "builder", role.to_str().unwrap())
            .unwrap();
        let mut reg = Registry::at(Some(dir.join("sessions.json")), Some(profiles));
        reg.roles_file = Some(roles_file);
        (Box::leak(Box::new(reg)), dir)
    }

    /// pi 세션 한 바퀴 — rpc 로 띄워 말하고, 역할 지침을 시스템 프롬프트로 싣고, `/clear` 가 새 좌표를
    /// 적고, 멈춘 뒤 그 좌표로 다시 뜬다. 모르는 역할은 띄우기 전에 거절한다.
    #[cfg(unix)]
    #[test]
    fn a_pi_session_runs_clears_and_resumes_on_its_coordinate() {
        let (reg, dir) = registry_with("pi", "pi", FAKE_PI);
        let sup = LocalSupervisor::with(reg);
        let roles = vec!["builder".to_string()];
        let spawned = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                roles: &roles,
                permission_prompts: true,
                ..Default::default()
            })
            .expect("spawn");
        let id = spawned["id"].as_str().unwrap().to_string();
        assert_eq!(spawned["kind"], "pi");
        assert_eq!(
            spawned["permission_prompts"], false,
            "pi 는 권한을 안 묻는다"
        );
        let first = answer(&sup.send_input(&id, "hi").expect("첫 턴"));
        assert!(first.starts_with("pi:hi"), "{first}");
        assert!(
            first.contains(&format!("--mode rpc --session-id {id}")),
            "{first}"
        );
        let guide = dir.join("roles/builder/CLAUDE.md");
        let guide = std::fs::canonicalize(guide).unwrap();
        assert!(
            first.contains(&format!("--append-system-prompt {}", guide.display())),
            "역할 지침이 시스템 프롬프트로 안 실렸다: {first}"
        );
        sup.send_input(&id, "/clear").expect("비우기");
        assert_eq!(
            sup.status(&id).unwrap()["agent_session_id"],
            "pi-new-session"
        );
        sup.stop(&id).unwrap();
        let back = answer(&sup.send_input(&id, "back").expect("재개"));
        assert!(back.contains("--session-id pi-new-session"), "{back}");
        sup.stop(&id).unwrap();
        let unknown = vec!["nope".to_string()];
        let err = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                roles: &unknown,
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    /// claude 세션의 역할은 `--add-dir` 로 얹고, 그 폴더의 `CLAUDE.md` 를 읽게 하는 환경을 켠다.
    #[cfg(unix)]
    #[test]
    fn claude_roles_ride_on_add_dir_with_the_guide_env() {
        let (reg, dir) = registry_with("claude-role", "claude", FAKE_CLAUDE);
        let sup = LocalSupervisor::with(reg);
        let roles = vec!["builder".to_string()];
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                roles: &roles,
                ..Default::default()
            })
            .expect("spawn")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let out = answer(&sup.send_input(&id, "hi").expect("턴"));
        let role = std::fs::canonicalize(dir.join("roles/builder")).unwrap();
        assert!(
            out.contains(&format!("--add-dir {}", role.display())),
            "{out}"
        );
        assert!(out.ends_with("guide:1"), "{out}");
        assert_eq!(sup.status(&id).unwrap()["roles"], json!(["builder"]));
        sup.stop(&id).unwrap();
    }

    /// 턴이 끝나도 백그라운드 작업이 남으면 행이 `busy` 다 — 입력은 받고, 다시 띄우는 폴더 변경은
    /// 거절한다(작업이 함께 죽는다). 작업이 끝나면 풀린다.
    #[cfg(unix)]
    #[test]
    fn background_tasks_keep_the_row_busy_after_the_turn() {
        let (reg, dir) = fake_registry("bg-busy");
        let sup = LocalSupervisor::with(reg);
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                ..Default::default()
            })
            .expect("spawn")["id"]
            .as_str()
            .unwrap()
            .to_string();
        sup.send_input(&id, "spawnbg").expect("턴");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut st = sup.status(&id).unwrap();
        while st["background_tasks"].as_array().is_none_or(Vec::is_empty)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
            st = sup.status(&id).unwrap();
        }
        assert_eq!(st["turn_active"], false, "{st}");
        assert_eq!(st["busy"], true, "작업이 남은 세션은 바쁘다: {st}");
        assert_eq!(st["background_tasks"][0]["type"], "local_bash");
        let refused = sup.add_dir(&id, dir.to_str().unwrap()).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains(&crate::domain::i18n::t("sup.err.background_running")),
            "{refused}"
        );
        let out = answer(
            &sup.send_input(&id, "endbg")
                .expect("작업 중에도 입력은 간다"),
        );
        assert!(out.starts_with("echo:endbg"), "{out}");
        let st = sup.status(&id).unwrap();
        assert_eq!(st["busy"], false, "{st}");
        sup.stop(&id).unwrap();
    }

    /// 자가 턴 도중 보낸 입력은 제 턴의 답을 받는다 — 자가 턴은 출력 기록에만 남는다.
    #[cfg(unix)]
    #[test]
    fn a_send_during_a_self_opened_turn_gets_its_own_reply() {
        let (reg, _dir) = fake_registry("self-turn");
        let sup = LocalSupervisor::with(reg);
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                ..Default::default()
            })
            .expect("spawn")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let rx = sup.subscribe(&id).unwrap();
        let first = answer(&sup.send_input(&id, "bg").expect("첫 턴"));
        assert!(first.starts_with("echo:bg"), "{first}");
        // 자가 턴의 본문이 구독에 올 때까지 기다린다 — 그 뒤에 보내야 경합이 재현된다.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let f = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("자가 턴이 안 왔다");
            if f.pointer("/result/event_raw/progress/text")
                .and_then(Value::as_str)
                == Some("self")
            {
                break;
            }
        }
        let next = answer(&sup.send_input(&id, "next").expect("자가 턴 뒤 턴"));
        assert!(
            next.starts_with("echo:next"),
            "자가 턴의 답을 받았다: {next}"
        );
        let out = sup.output(&id, Some(3)).unwrap();
        assert_eq!(out["total"], 3, "자가 턴도 출력 기록에 남는다: {out}");
        assert_eq!(out["lines"][1], "self");
        sup.stop(&id).unwrap();
    }

    /// 권한 확인 — 기다리는 물음이 행에 실리고, 답하면 자식이 그 답으로 턴을 마친다.
    #[cfg(unix)]
    #[test]
    fn a_permission_prompt_is_answered_from_outside_the_turn() {
        let (reg, _dir) = fake_registry("perm");
        let sup = LocalSupervisor::with(reg);
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                permission_prompts: true,
                ..Default::default()
            })
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let rx = sup.send_turn(&id, "perm").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let pending = loop {
            let st = sup.status(&id).unwrap();
            if let Some(n) = st.get("needs_input") {
                break n.clone();
            }
            assert!(Instant::now() < deadline, "물음이 행에 안 실렸다");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(pending["tool"], "Bash");
        assert_eq!(pending["kind"], "permission");
        let a = crate::application::prompts::Answer {
            request_id: pending["request_id"].as_str().unwrap().to_string(),
            decision: "allow".into(),
            updated_input: None,
            message: None,
        };
        sup.respond(&id, &a).expect("respond");
        let got = rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(got, "allowed");
        assert!(sup.status(&id).unwrap().get("needs_input").is_none());
        sup.stop(&id).unwrap();
    }

    /// 추가 폴더를 바꾸면 살아 있는 자식이 `--resume` 과 새 `--add-dir` 로 다시 뜬다.
    #[cfg(unix)]
    #[test]
    fn adding_a_dir_restarts_a_live_child_with_the_new_list() {
        let (reg, dir) = fake_registry("adddir");
        let sup = LocalSupervisor::with(reg);
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                ..Default::default()
            })
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        sup.send_input(&id, "hi").unwrap();
        let extra = dir.join("extra");
        std::fs::create_dir_all(&extra).unwrap();
        let r = sup.add_dir(&id, extra.to_str().unwrap()).unwrap();
        assert_eq!(r["add_dirs"]["changed"], true);
        assert_eq!(r["add_dirs"]["applied"], true);
        let again = sup.add_dir(&id, extra.to_str().unwrap()).unwrap();
        assert_eq!(
            again["add_dirs"]["changed"], false,
            "같은 폴더는 바뀐 것이 아니다"
        );
        let canon = std::fs::canonicalize(&extra).unwrap();
        let said = answer(&sup.send_input(&id, "x").unwrap());
        assert!(
            said.contains(&format!("--add-dir {}", canon.display())),
            "{said}"
        );
        assert!(
            said.contains("--resume"),
            "다시 뜰 때 대화를 잇는다: {said}"
        );
        sup.stop(&id).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unknown_profiles_and_live_deletes_are_refused() {
        let (reg, _dir) = fake_registry("refuse");
        let sup = LocalSupervisor::with(reg);
        let e = sup
            .spawn(&SpawnOpts {
                agent: "nope",
                ..Default::default()
            })
            .expect_err("없는 프로필");
        assert!(
            matches!(e, ClientError::Daemon { ref kind, .. } if kind == "bad_request"),
            "{e}"
        );
        let id = sup
            .spawn(&SpawnOpts {
                agent: "fake",
                ..Default::default()
            })
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let e = sup.delete(&id).expect_err("살아 있는 세션 지우기");
        assert!(
            matches!(e, ClientError::Daemon { ref kind, .. } if kind == "invalid_state"),
            "{e}"
        );
        sup.stop(&id).unwrap();
    }
}
