//! 이 탭이 보내지 않은 턴의 **답**을 대화에 붙인다.
//!
//! # 왜 따로 있나
//!
//! 탭은 자기가 보낸 말의 답만 받는다 — `send_input` 의 응답이 그것이다. 그런데
//! 에이전트는 그 밖에서도 턴을 돈다: 백그라운드 작업을 걸어 두고 답을 끝낸 뒤,
//! 완료 알림을 받아 **스스로 다음 턴을 이어 간다**(실측 2026-10-12). 그 답은
//! 데몬의 출력 기록에는 남는데 탭에는 끝내 안 나타났다. CLI 가 보낸 턴도 같다.
//!
//! ⚠️ 판 6 에서는(데몬 쪽 설명, `dd9e139`) **스스로 이어 가는 턴은 아무도 출력을 안
//! 읽어** 그 출력이 다음 `send_input` 의 읽기에 섞여 들어가므로 이 길로 안 붙는다.
//! 판 7 부터는 붙는다(실측 2026-10-13, `24b4e3c` · `lhs` haiku): 구독에
//! `system/task_notification` 뒤로 `started` → `output` → `finished` 가 왔고, 대화 탭에
//! "에이전트가 이어서" 표시와 함께 그 답이 붙었으며, 다음 `send_input` 의 답은 자기 답이었다.
//!
//! 그 턴을 아는 길은 데몬의 구독(`session.subscribe`)이다 — 턴마다 `output`
//! (완성된 메시지)과 `finished`(턴 끝)가 흐른다(실측: 한 턴에 `started` →
//! `text_delta` → `output` → … → `finished`). 구독은 연결 하나를 계속 붙잡으므로
//! **세션마다 워커**가 들고, 받은 이벤트를 `self_invoke` 로 plugin 루프에 넘긴다.
//!
//! # 내 턴과 남의 턴을 가르는 법
//!
//! 턴의 **첫 이벤트가 올 때** 이 탭이 기다리는 턴이 있으면 그 턴은 이 탭의
//! 것이다 — 답은 `send_input` 응답이 채우므로 여기서는 버린다. 기다리는 턴이
//! 없을 때 시작된 턴만 모아서, 끝나면 "에이전트가 이어서" 턴으로 붙인다.
//! 에이전트는 입력을 한 번에 하나씩 처리하므로 시작 순간의 판정이 그 턴 전체에
//! 맞다.
//!
//! # 원문 — 상태줄의 사용량
//!
//! 데몬이 `protocol` 6 이상이면 **원문 구독**(`raw:true`)으로 붙는다. 그때 프레임은
//! `event_raw{progress, raw, format}` 이고, `progress` 가 종전 `event` 와 같은 모양이라
//! 위 판정은 그대로다. 원문 줄은 [`crate::domain::usage`] 가 읽어 상태줄의 모델 · 컨텍스트 ·
//! 비용 · 사용률을 채운다 — 이 탭이 보낸 턴이든 아니든 같다(실측 2026-10-13, `dd9e139`:
//! 한 턴 185 프레임 전부 `event_raw` · `claude-stream-json`, `system/init` 은 `started`,
//! `result` 는 `finished` 로 왔다).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::application::session_surface;
use crate::domain::convo::Convo;
use crate::domain::convo::Convos;
use crate::domain::convo::Turn;
use crate::ports::host::HostCall;
use crate::ports::host::SendHost;

/// 워커가 되던지는 메서드 — plugin 만 부른다(`self_invoke`).
pub const METHOD: &str = "agent_session.stream";

/// 이벤트가 없을 때 깨어나 "아직 볼 것인가" 를 확인하는 주기.
const IDLE_CHECK: std::time::Duration = std::time::Duration::from_secs(5);
/// 연결이 끊긴 뒤 다시 붙기까지 — 재운 세션은 구독을 거절할 수 있어 촘촘히 두지 않는다.
const RECONNECT_AFTER: std::time::Duration = std::time::Duration::from_secs(10);
/// 잘 붙어 있던 연결이 끊겼을 때 다시 붙기까지 — 바로 붙는다(틈에 온 이벤트는 되돌아오지 않는다).
const RECONNECT_QUICK: std::time::Duration = std::time::Duration::from_millis(100);
/// 이만큼 살아 있던 연결은 "잘 붙어 있었다" 로 본다 — 붙자마자 끊기는 연결로 헛돌지 않게 가른다.
const LIVED_ENOUGH: std::time::Duration = std::time::Duration::from_secs(1);

/// 끊긴 연결 뒤 기다릴 시간.
///
/// 잘 붙어 있던 구독은 바로 다시 붙는다 — 그 틈에 끝난 턴의 끝 이벤트를 놓치면 완료 표시 · hook ·
/// 텔레메트리가 빠진다. 붙자마자 끊긴 것(지워진 세션 등)만 기다린다.
fn reconnect_delay(lived: std::time::Duration) -> std::time::Duration {
    if lived >= LIVED_ENOUGH {
        RECONNECT_QUICK
    } else {
        RECONNECT_AFTER
    }
}

/// 세션 id → 그 세션을 맡은 워커의 세대. 세대가 바뀌면 옛 워커는 물러난다.
type Wanted = Arc<Mutex<HashMap<String, u64>>>;

/// 진행 중인 턴 하나 — 첫 이벤트에서 열고 `finished` 에서 닫는다.
#[derive(Default)]
struct Cur {
    /// 이 탭이 보낸 턴인가 — 그렇다면 내용은 `send_input` 응답이 맡는다.
    own: bool,
    /// 완성된 메시지(`output`)들. 도구를 쓰는 턴은 메시지가 여럿이다.
    parts: Vec<String>,
}

#[derive(Default)]
pub struct Streams {
    wanted: Wanted,
    next_gen: u64,
    /// 탭별 진행 중인 턴.
    cur: BTreeMap<u32, Cur>,
}

impl Streams {
    /// 이어진 세션마다 워커가 하나씩 돌게 맞춘다 — 주기적으로 불린다.
    ///
    /// 새로 이어진 세션은 워커를 띄우고, 끊긴 세션은 표에서 빼서 그 워커가
    /// 다음 확인 때 물러나게 한다.
    pub fn sync<S: crate::ports::supervisor::Supervisor + Copy + Send + 'static>(
        &mut self,
        host: &impl SendHost,
        sup: S,
        convos: &Convos,
    ) {
        let linked: Vec<String> = convos
            .values()
            .filter_map(|c| c.session_id.clone())
            .collect();
        self.cur.retain(|sid, _| convos.contains_key(sid));
        let Ok(mut wanted) = self.wanted.lock() else {
            tracing::warn!("구독 표가 오염됐다 — 이어진 턴 표시가 멈춘다");
            return;
        };
        wanted.retain(|id, _| linked.contains(id));
        for id in linked {
            if wanted.contains_key(&id) {
                continue;
            }
            self.next_gen += 1;
            wanted.insert(id.clone(), self.next_gen);
            spawn_worker(
                host.clone(),
                sup,
                Arc::clone(&self.wanted),
                id,
                self.next_gen,
            );
        }
    }

    /// 워커가 이벤트 하나를 넘겼다. 이 세션을 든 탭마다 반영한다.
    ///
    /// 돌려주는 것은 **대화가 늘어난 탭** — 상태줄(턴 수)을 다시 그려야 한다.
    pub fn apply<H: HostCall>(
        &mut self,
        host: &H,
        convos: &mut Convos,
        params: &Value,
    ) -> Vec<u32> {
        let field = |k: &str| {
            params
                .pointer(&format!("/params/{k}"))
                .or_else(|| params.get(k))
        };
        let Some(session) = field("session").and_then(Value::as_str) else {
            return Vec::new();
        };
        let raw = field("raw").and_then(Value::as_str);
        let format = field("format").and_then(Value::as_str).unwrap_or_default();
        let mut grown = Vec::new();
        // 대화 기록에서 읽은 세션 이름 — 붙을 때와 턴 끝에 워커가 싣는다(`session_info`).
        if let Some(name) = field("name").and_then(Value::as_str) {
            grown.extend(note_name(convos, session, name));
        }
        // 프로필 계정 — 워커가 처음 붙을 때 한 번 싣는다(`send_email`).
        if let Some(email) = field("email").and_then(Value::as_str) {
            grown.extend(note_email(convos, session, email));
        }
        let Some(ev) = field("event") else {
            return grown;
        };
        for (&sid, convo) in convos.iter_mut() {
            if convo.session_id.as_deref() != Some(session) {
                continue;
            }
            // 값이 바뀔 때만 고친다 — 매 줄 고치면 상태줄이 깜빡인다.
            let by_line = raw.is_some_and(|line| {
                crate::domain::usage::apply_raw(&mut convo.usage, format, line)
            });
            let by_budget = field("budget")
                .is_some_and(|b| crate::domain::usage::apply_budget(&mut convo.usage, b));
            // 상태줄은 통째로 다시 그린다 — 돌려준 탭은 호출부가 그린다(`statusline`).
            if by_line || by_budget {
                grown.push(sid);
                // 다시 뜬 plugin · 재시작한 tasty 가 다음 턴 전에도 그리게 남긴다(`surface_link::USAGE_META_KEY`).
                crate::application::surface_link::save_usage(
                    host,
                    sid,
                    &crate::domain::usage::to_snapshot(&convo.usage),
                );
            }
            // 턴이 끝난 줄이면 그 턴의 걸린 시간 · 컨텍스트 토큰을 남긴다 — 값은 방금 반영했다.
            // hook 없는 신호 — 물음 · 턴 오류 · 대화 id(`hookless`).
            if let Some(line) = raw {
                crate::application::hookless::on_raw(host, sid, convo, format, line);
                // 도구 호출 · 서브에이전트 · 경계 줄이면 패널을 다시 읽게 표시만 한다(읽기는 묶어서, `subagents`).
                // 백그라운드 작업 줄은 기록에 없어 그 자리에서 반영한다.
                convo.subs.on_raw(format, line);
            }
            if raw.is_some_and(|line| crate::domain::usage::is_result(format, line)) {
                record_turn_metrics(host, sid, &convo.usage);
                // 컨텍스트 임계 판정은 plugin 이 이어서 한다(`sync_compact`) — 설정을 읽어야 한다.
                convo.sync.result_seen = true;
                convo.sync.turn_judged = true;
            }
            // 이 탭이 연 턴인가 — 표(`own_turns`)로 가른다. 시작 순간에 "기다리는 턴이 있나" 만
            // 보면, 탭이 그 턴을 이미 닫은 뒤에 시작이 처리될 때 남의 턴이 된다(그 모듈 주석).
            // 맥락 턴도 이 탭의 것이다 — 그 답을 "에이전트가 이어서" 로 붙이면 안 된다.
            let mut open = || Cur {
                own: convo.own_turns.claim(std::time::Instant::now())
                    || crate::domain::convo::waits_for_own_turn(convo)
                    || convo.briefing
                    || convo.clearing
                    || convo.sync.compacting(),
                parts: Vec::new(),
            };
            match ev.get("phase").and_then(Value::as_str) {
                // 턴은 **여기서만** 연다(실측: 같은 프로세스의 매 턴이 `started` 로
                // 시작한다). 턴 사이에 흐르는 다른 이벤트로 열면 그때의 판정이
                // 다음 턴까지 따라가, 이 탭의 답이 한 번 더 붙는다.
                Some("started") => {
                    self.cur.insert(sid, open());
                    // 새 턴 — 앞 턴의 오류 표지는 이 턴의 것이 아니다.
                    convo.hookless.turn_error = false;
                    convo.sync.turn_judged = false;
                }
                Some("output") => {
                    // 턴 중간에 구독이 붙었으면 `started` 를 못 봤다 — 여기서 연다.
                    let cur = self.cur.entry(sid).or_insert_with(open);
                    if let Some(t) = ev.get("text").and_then(Value::as_str) {
                        cur.parts.push(t.to_string());
                    }
                }
                Some("finished") => {
                    // 원문 `result` 줄을 구독 틈에 놓친 턴 — 워커가 대화 기록에서 읽은 컨텍스트로 채우고
                    // 턴 끝 판정을 연다. 놓친 턴이 판정을 건너뛰면 자동 현행화가 영영 안 열리고 쉰 시간도
                    // 낡은 턴 끝에서 잰다(실측 2026-10-26: 긴 턴 5 중 3 · 꼬리표 536k vs 실제 914k).
                    if !convo.sync.turn_judged {
                        if let Some(n) = field("context").and_then(Value::as_u64).filter(|n| *n > 0)
                        {
                            convo.usage.context_used = Some(n);
                        }
                        convo.sync.result_seen = true;
                    }
                    convo.sync.turn_judged = false;
                    // 어느 탭이 보낸 턴이든 끝났다는 사실은 같다 — 탭을 표시한다(`mark_done`).
                    if let Err(e) = crate::application::surface_link::mark_done(host, sid) {
                        tracing::warn!(
                            "[agent_session] surface={sid}: 턴 완료 표시를 못 켰다: {e}"
                        );
                    }
                    if let Some(event) = hook_of_finished(convo, ev)
                        && let Err(e) =
                            crate::application::surface_link::fire_hook(host, sid, event)
                    {
                        tracing::warn!(
                            "[agent_session] surface={sid}: hook '{event}' 발사 실패: {e}"
                        );
                    }
                    let done = self.cur.remove(&sid).unwrap_or_default();
                    // 이 탭의 턴인데 `send_input` 이 상한을 넘겨 답을 못 받았다 —
                    // 그 답은 여기만 갖고 있다.
                    if done.own && !convo.overdue.is_empty() {
                        let got = answer_of_finished(&done, ev);
                        if let Some(at) = crate::domain::convo::settle_overdue(convo, got.clone()) {
                            tracing::info!(
                                "[agent_session] surface={sid}: 상한을 넘긴 턴 {at} 의 답을 구독으로 받았다"
                            );
                            session_surface::patch(
                                host,
                                sid,
                                &session_surface::finish_msg(at, &got),
                            );
                            grown.push(sid);
                        }
                        continue;
                    }
                    if let Some(turn) = turn_of(done, ev) {
                        tracing::info!(
                            "[agent_session] surface={sid}: 이 탭이 안 보낸 턴의 답을 붙였다 ({}자)",
                            turn.got.chars().count()
                        );
                        session_surface::patch(host, sid, &append_msg(&turn));
                        convo.turns.push(turn);
                        grown.push(sid);
                    }
                }
                _ => {}
            }
        }
        grown
    }
}

/// 대화 기록의 세션 이름을 그 세션을 든 탭에 옮긴다 — 바뀐 탭만 돌려준다(상태줄을 다시 그린다).
/// 프로필 계정을 이 세션을 든 탭에 남긴다 — 바뀐 탭만 돌려준다(상태줄을 다시 그린다).
fn note_email(convos: &mut Convos, session: &str, email: &str) -> Vec<u32> {
    let mut changed = Vec::new();
    for (&sid, convo) in convos.iter_mut() {
        if convo.session_id.as_deref() == Some(session)
            && convo.account_email.as_deref() != Some(email)
        {
            convo.account_email = Some(email.to_string());
            changed.push(sid);
        }
    }
    changed
}

/// 그 세션 프로필의 계정을 한 번 찾아 싣는다 — 프로필을 찾았으면(계정이 없어도) 다시 안 묻는다.
/// `false` 면 plugin 이 내려가는 중이다.
fn send_email(
    host: &impl SendHost,
    sup: &impl crate::ports::supervisor::Supervisor,
    id: &str,
) -> Option<bool> {
    let source = crate::adapters::driven::store::budget::Source::resolve(sup, id)?;
    let Some(email) = crate::adapters::driven::store::budget::account_file(&source.env)
        .and_then(|f| crate::adapters::driven::store::budget::account_email(&f))
    else {
        return Some(true);
    };
    Some(
        host.self_invoke(METHOD, json!({ "session": id, "email": email }))
            .is_ok(),
    )
}

fn note_name(convos: &mut Convos, session: &str, name: &str) -> Vec<u32> {
    let mut changed = Vec::new();
    for (&sid, convo) in convos.iter_mut() {
        if convo.session_id.as_deref() == Some(session)
            && convo.transcript_name.as_deref() != Some(name)
        {
            convo.transcript_name = Some(name.to_string());
            changed.push(sid);
        }
    }
    changed
}

/// 턴 하나의 텔레메트리 — claude plugin 과 같은 이름(`wall_time_ms` · `input_tokens`).
fn record_turn_metrics<H: HostCall>(host: &H, sid: u32, usage: &crate::domain::usage::Usage) {
    for (metric, value) in [
        ("wall_time_ms", usage.turn_ms),
        ("input_tokens", usage.context_used),
    ] {
        if let Some(v) = value
            && let Err(e) =
                crate::application::surface_link::record_metric(host, sid, metric, v as f64)
        {
            tracing::debug!("[agent_session] surface={sid}: 텔레메트리 '{metric}' 실패: {e}");
        }
    }
}

/// 끝난 턴이 발사할 hook — 성공이면 `claude-idle`, 실패면 `claude-error`.
///
/// claude plugin 의 Stop 훅(`claude-idle`)과 오류 탐지(`claude-error`)에 해당하는 자리다. 이 탭에서
/// **중지**한 턴은 실패로 끝나지만 오류가 아니다 — 발사하지 않는다. 중지 표시는 이 탭이 보낸 턴의
/// 마감이 먼저 거둘 수 있어, 그 턴이 멈춘 턴(`stopped`)으로 적혔는지도 본다. claude 가 아닌 세션은
/// 이름이 맞지 않아 발사하지 않는다 — 종류를 아직 모르면(첫 목록 전) claude 로 친다.
fn hook_of_finished(convo: &Convo, finished: &Value) -> Option<&'static str> {
    if convo.agent_kind.as_deref().is_some_and(|k| k != "claude") {
        return None;
    }
    let last = convo.turns.len().checked_sub(1);
    let stopped = convo.stop_requested || last.is_some_and(|i| convo.stopped.contains(&i));
    if finished.get("ok").and_then(Value::as_bool).unwrap_or(true) {
        // 진행 이벤트는 ok 여도 결과 줄이 오류면(`is_error` · `error_*`) 오류다(`hookless`).
        return Some(if convo.hookless.turn_error && !stopped {
            "claude-error"
        } else {
            "claude-idle"
        });
    }
    (!stopped).then_some("claude-error")
}

/// 끝난 턴의 답 — 실패한 턴이면 사유. [`turn_of`] 와 같은 판정이다.
fn answer_of_finished(done: &Cur, finished: &Value) -> Result<String, String> {
    let ok = finished.get("ok").and_then(Value::as_bool).unwrap_or(true);
    if ok {
        Ok(done.parts.join("\n\n"))
    } else {
        Err(finished
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }
}

/// 끝난 턴을 대화에 붙일 모양으로 만든다 — 이 탭의 턴이거나 비었으면 `None`.
///
/// 실패한 턴은 사유를 남긴다 — 답이 안 온 것과 실패한 것은 다르다.
fn turn_of(done: Cur, finished: &Value) -> Option<Turn> {
    if done.own {
        return None;
    }
    let got = done.parts.join("\n\n");
    let ok = finished.get("ok").and_then(Value::as_bool).unwrap_or(true);
    let error = (!ok).then(|| {
        finished
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    if got.trim().is_empty() && error.is_none() {
        return None;
    }
    Some(Turn {
        // 보낸 말이 없는 턴 — 화면은 이 빈 값을 "에이전트가 이어서" 로 그린다.
        sent: String::new(),
        got,
        pending: false,
        at: Some(crate::adapters::driven::store::budget::now()),
        error,
    })
}

/// 이어진 턴을 로그 끝에 붙이는 스크립트.
///
/// 텍스트는 `textContent` 로 넣는다 — `innerHTML` 이면 답에 든 태그가 실행된다.
/// 사용자가 위로 스크롤해 읽는 중이면 끌어내리지 않는다.
fn append_msg(turn: &Turn) -> Value {
    let (cls, body) = match &turn.error {
        Some(e) => ("err", e.as_str()),
        None => ("got", turn.got.as_str()),
    };
    serde_json::json!({
        "type": "turn.auto",
        "label": crate::domain::i18n::t("sup.chat.auto_turn"),
        "cls": cls,
        "text": body,
    })
}

/// 세션 하나를 따라가는 워커 — 끊기면 다시 붙고, 표에서 빠지면 물러난다.
fn spawn_worker<S: crate::ports::supervisor::Supervisor + Send + 'static>(
    host: impl SendHost,
    sup: S,
    wanted: Wanted,
    id: String,
    generation: u64,
) {
    let still_wanted = {
        let wanted = Arc::clone(&wanted);
        let id = id.clone();
        move || {
            // poison 이어도 표를 읽는다 — 넘겨 버리면 워커가 할 일이 남았는데 물러난다.
            wanted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&id)
                == Some(&generation)
        }
    };
    let name = format!(
        "agent-session-stream-{}",
        id.chars().take(8).collect::<String>()
    );
    let worker_id = id.clone();
    if std::thread::Builder::new()
        .name(name)
        .spawn(move || {
            let id = worker_id;
            // 잔액 장부와 이름 판독은 **재개를 넘어** 쥔다 — 새로 만들면 대화 기록(수십 MB)을
            // 그때마다 처음부터 훑는다.
            let mut tracker: Option<crate::adapters::driven::store::budget::Tracker> = None;
            let mut names: Option<crate::adapters::driven::session_info::NameWatch> = None;
            let mut last_name: Option<String> = None;
            let mut email_done = false;
            while still_wanted() {
                let mut wait = RECONNECT_AFTER;
                match sup.subscribe(&id) {
                    Ok(rx) => {
                        let opened = std::time::Instant::now();
                        tracing::debug!("세션 {id} 구독을 열었다");
                        // API 세션이면 턴 끝마다 잔액 대비 사용을 싣는다 — 기록이 커서
                        // 루프가 아니라 이 스레드에서 센다(`budget` 모듈 머리글).
                        if tracker.is_none() {
                            tracker =
                                crate::adapters::driven::store::budget::Tracker::resolve(&sup, &id);
                        }
                        // 세션 이름은 대화 기록에 있다 — 처음 한 번, 그 뒤로 턴 끝마다(`session_info`).
                        if names.is_none() {
                            names = crate::adapters::driven::session_info::NameWatch::resolve(
                                &sup, &id,
                            );
                        }
                        // 프로필 계정은 세션 동안 안 바뀐다 — 프로필을 찾을 때까지만 묻는다.
                        if !email_done {
                            match send_email(&host, &sup, &id) {
                                Some(false) => return,
                                Some(true) => email_done = true,
                                None => {}
                            }
                        }
                        let name = names.as_mut().and_then(|w| w.catch_up());
                        if name.is_some() && name != last_name {
                            last_name = name.clone();
                            if host
                                .self_invoke(METHOD, json!({ "session": id, "name": name }))
                                .is_err()
                            {
                                return;
                            }
                        }
                        if !follow(&rx, &host, &id, &still_wanted, &mut tracker, &mut names) {
                            return;
                        }
                        wait = reconnect_delay(opened.elapsed());
                    }
                    // 세션 기록이 아직 없거나 지워졌다 — 조용히 다시 해 본다.
                    Err(e) => tracing::debug!("세션 {id} 구독 실패(다시 시도한다): {e}"),
                }
                std::thread::sleep(wait);
            }
        })
        .is_err()
    {
        tracing::warn!("세션 구독 워커를 못 띄웠다 — 이어진 턴의 답이 탭에 안 붙는다");
        // 표에서 빼 두면 다음 `sync` 가 다시 띄워 본다. poison 이어도 뺀다 — 안 빼면 다시 안 띄운다.
        wanted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }
}

/// 구독이 살아 있는 동안 이벤트를 넘긴다. `false` 면 plugin 이 내려가는 중이다.
///
/// 받기 상한(`IDLE_CHECK`)에 걸리는 것은 정상이다 — 그때 "아직 볼 것인가" 를 확인한다.
fn follow(
    rx: &std::sync::mpsc::Receiver<Value>,
    host: &impl SendHost,
    id: &str,
    still_wanted: &impl Fn() -> bool,
    tracker: &mut Option<crate::adapters::driven::store::budget::Tracker>,
    names: &mut Option<crate::adapters::driven::session_info::NameWatch>,
) -> bool {
    loop {
        if !still_wanted() {
            return true;
        }
        let v = match rx.recv_timeout(IDLE_CHECK) {
            Ok(v) => v,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            // 세션이 지워졌다 — 다시 붙어 본다.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
        };
        if let Some(frame) = frame_of(&v) {
            let mut params = json!({ "session": id, "event": frame.event });
            if let Some(raw) = frame.raw {
                params["raw"] = Value::String(raw.line);
                params["format"] = Value::String(raw.format);
            }
            let finished = frame.event.get("phase").and_then(Value::as_str) == Some("finished");
            if let Some(t) = tracker.as_mut()
                && finished
            {
                let b = t.budget();
                params["budget"] = json!({ "spent": b.spent, "balance": b.balance });
            }
            if let Some(name) = names
                .as_mut()
                .filter(|_| finished)
                .and_then(|w| w.catch_up())
            {
                params["name"] = Value::String(name);
            }
            // 턴 끝의 컨텍스트 — 원문 `result` 줄을 놓친 턴의 판정에 쓴다(`Streams::apply`). 위
            // `catch_up` 이 방금 기록을 읽었다.
            if let Some(n) = names
                .as_ref()
                .filter(|_| finished)
                .and_then(crate::adapters::driven::session_info::NameWatch::context)
            {
                params["context"] = json!(n);
            }
            if host.self_invoke(METHOD, params).is_err() {
                return false;
            }
        }
    }
}

/// 프레임 하나에서 꺼낸 것 — 진행 이벤트와, 원문 구독이면 그 원문.
#[derive(Debug, PartialEq)]
struct Frame {
    event: Value,
    raw: Option<Raw>,
}

#[derive(Debug, PartialEq)]
struct Raw {
    format: String,
    line: String,
}

/// 프레임 본문을 푼다 — `event`(종전) 또는 `event_raw`(원문 구독). 둘 다 아니면 `None`.
fn frame_of(v: &Value) -> Option<Frame> {
    let result = v.get("result")?;
    if let Some(ev) = result.get("event") {
        return Some(Frame {
            event: ev.clone(),
            raw: None,
        });
    }
    let r = result.get("event_raw")?;
    Some(Frame {
        event: r.get("progress")?.clone(),
        raw: r.get("raw").and_then(Value::as_str).map(|line| Raw {
            format: r
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            line: line.to_string(),
        }),
    })
}

/// 탭별로 무엇을 보냈는지 기록하는 시험용 호스트.
#[cfg(test)]
#[derive(Default)]
struct RecordingHost(
    std::cell::RefCell<Vec<String>>,
    /// 켠 attention — `(surface, kind)`.
    std::cell::RefCell<Vec<(u64, String)>>,
    /// 발사한 hook — `(surface, event)`.
    std::cell::RefCell<Vec<(u64, String)>>,
);

#[cfg(test)]
impl HostCall for RecordingHost {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        if method == "webview.post_message" {
            let s = params["data"].to_string();
            self.0.borrow_mut().push(s);
        }
        if method == "telemetry.record" {
            self.2.borrow_mut().push((
                params["value"].as_f64().unwrap_or_default() as u64,
                params["metric"].as_str().unwrap_or_default().to_string(),
            ));
        }
        if method == "surface.fire_hook" {
            self.2.borrow_mut().push((
                params["surface_id"].as_u64().unwrap_or_default(),
                params["event"].as_str().unwrap_or_default().to_string(),
            ));
        }
        if method == "surface.completion" {
            self.1.borrow_mut().push((
                params["surface_id"].as_u64().unwrap_or_default(),
                params["kind"].as_str().unwrap_or_default().to_string(),
            ));
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Convo;

    fn convos_with(session: &str, pending: bool) -> Convos {
        let mut turns = Vec::new();
        if pending {
            turns.push(Turn {
                sent: "hi".into(),
                got: String::new(),
                error: None,
                pending: true,
                at: None,
            });
        }
        let mut c = HashMap::new();
        c.insert(
            7,
            Convo {
                session_id: Some(session.into()),
                turns,
                ..Default::default()
            },
        );
        c
    }

    fn feed(s: &mut Streams, h: &RecordingHost, c: &mut Convos, ev: Value) -> Vec<u32> {
        s.apply(h, c, &json!({ "session": "s1", "event": ev }))
    }

    /// `send_input` 이 상한을 넘긴 이 탭의 턴은 **구독이 본 끝으로 닫힌다** — 버리지 않는다.
    #[test]
    fn an_overdue_turn_of_this_tab_is_closed_by_the_stream() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        assert!(crate::domain::convo::mark_overdue(
            c.get_mut(&7).unwrap(),
            0
        ));
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"late"}));
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert_eq!(grown, vec![7]);
        let cv = &c[&7];
        assert_eq!(
            cv.turns.len(),
            1,
            "새 턴으로 붙었다 — 기다리던 자리를 닫아야 한다"
        );
        assert!(!cv.turns[0].pending);
        assert_eq!(cv.turns[0].got, "late");
        assert!(cv.overdue.is_empty());
        assert!(
            h.0.borrow()[0].contains("late"),
            "화면의 그 자리를 안 고쳤다"
        );
    }

    /// 실패로 끝난 턴이면 그 사유로 닫힌다.
    #[test]
    fn an_overdue_turn_that_failed_keeps_the_reason() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        crate::domain::convo::mark_overdue(c.get_mut(&7).unwrap(), 0);
        feed(
            &mut s,
            &h,
            &mut c,
            json!({"phase":"finished","ok":false,"reason":"aborted"}),
        );
        assert_eq!(c[&7].turns[0].error.as_deref(), Some("aborted"));
    }

    /// 이 탭이 안 보낸 턴은 끝날 때 **한 턴으로** 붙는다 — 메시지가 여럿이어도.
    #[test]
    fn a_turn_this_tab_did_not_send_is_appended_when_it_finishes() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"one"}));
        assert!(c[&7].turns.is_empty(), "끝나기 전에 붙었다");
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"two"}));
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert_eq!(grown, vec![7]);
        assert_eq!(c[&7].turns.len(), 1);
        assert_eq!(c[&7].turns[0].sent, "");
        assert_eq!(c[&7].turns[0].got, "one\n\ntwo");
        assert_eq!(h.0.borrow().len(), 1, "화면에 한 번 붙여야 한다");
    }

    /// 띄운 세션에 자리를 알리는 맥락 턴의 답은 **화면에 안 붙는다** — 사용자가 안 보낸 말의 답이다.
    #[test]
    fn the_brief_turn_is_not_appended() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        c.get_mut(&7).unwrap().briefing = true;
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(
            &mut s,
            &h,
            &mut c,
            json!({"phase":"output","text":"알겠습니다"}),
        );
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert!(grown.is_empty(), "맥락 턴의 답이 이어진 턴으로 붙었다");
        assert!(c[&7].turns.is_empty());
    }

    /// 이 탭이 기다리는 동안 시작된 턴은 그 탭의 것이다 — 답이 두 번 붙으면 안 된다.
    #[test]
    fn a_turn_started_while_this_tab_waits_is_left_to_the_send_reply() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        // 답이 먼저 와서 대기가 풀려도, 그 턴의 남은 이벤트는 여전히 이 탭의 것이다.
        if let Some(cv) = c.get_mut(&7) {
            cv.turns[0].pending = false;
        }
        feed(
            &mut s,
            &h,
            &mut c,
            json!({"phase":"output","text":"answer"}),
        );
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert!(grown.is_empty());
        assert_eq!(c[&7].turns.len(), 1);
        assert!(h.0.borrow().is_empty());
    }

    /// **경합 순서** — 탭이 턴을 열고 **닫은 뒤에** 구독의 시작이 처리돼도 그 턴은 이 탭의 것이다.
    /// 시작 순간의 "기다리는 턴이 있나" 로 가르던 때는 이 순서에서 답이 한 번 더 붙었다
    /// (실측 2026-10-19 F37). 짝 — 탭이 연 턴이 없을 때 온 턴은 그대로 이어진 턴으로 붙는다.
    #[test]
    fn a_turn_the_tab_already_closed_is_not_appended_again() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        if let Some(cv) = c.get_mut(&7) {
            let at = crate::domain::convo::begin_turn(cv, "q");
            crate::domain::convo::finish_turn(cv, at, Ok("a".into()));
        }
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"a"}));
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert!(
            grown.is_empty(),
            "탭이 닫은 턴의 답이 이어진 턴으로 또 붙었다"
        );

        // 짝 — 표가 다 쓰인 뒤 온 턴은 남의 턴이다.
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"b"}));
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert_eq!(grown, vec![7], "스스로 이어 간 턴이 안 붙었다");
    }

    /// 턴 사이에 흐르는 이벤트는 턴을 열지 않는다 — 열면 그때의 판정이 다음
    /// 턴(이 탭이 보낸 것)까지 따라가 답이 두 번 붙는다.
    #[test]
    fn an_event_between_turns_does_not_decide_the_next_turn() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        feed(
            &mut s,
            &h,
            &mut c,
            json!({"phase":"other","kind":"system/status"}),
        );
        // 그 뒤 이 탭이 보낸다.
        if let Some(cv) = c.get_mut(&7) {
            cv.turns.push(Turn {
                sent: "q".into(),
                got: String::new(),
                error: None,
                pending: true,
                at: None,
            });
        }
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"a"}));
        let grown = feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert!(grown.is_empty(), "이 탭의 답이 이어진 턴으로 또 붙었다");
    }

    /// 실패한 턴은 사유를 남긴다.
    #[test]
    fn a_failed_turn_keeps_its_reason() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"started"}));
        feed(
            &mut s,
            &h,
            &mut c,
            json!({"phase":"finished","ok":false,"reason":"boom"}),
        );
        assert_eq!(c[&7].turns[0].error.as_deref(), Some("boom"));
    }

    /// 다른 세션의 이벤트는 이 탭에 안 붙는다.
    #[test]
    fn events_of_another_session_are_ignored() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("other", false),
        );
        feed(&mut s, &h, &mut c, json!({"phase":"output","text":"x"}));
        feed(&mut s, &h, &mut c, json!({"phase":"finished","ok":true}));
        assert!(c[&7].turns.is_empty());
    }

    /// 원문 구독의 프레임 — `progress` 가 이벤트가 되고 원문은 따로 실린다(실측 모양 2026-10-13).
    #[test]
    fn a_raw_frame_yields_its_progress_and_line() {
        let v = json!({"status":"ok","result":{"event_raw":{
            "progress":{"phase":"started"},
            "raw":"{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-haiku-4-5-20251001\"}",
            "format":"claude-stream-json"}}});
        let f = frame_of(&v).expect("원문 프레임을 못 풀었다");
        assert_eq!(f.event, json!({"phase":"started"}));
        let raw = f.raw.expect("원문이 빠졌다");
        assert_eq!(raw.format, "claude-stream-json");
        assert!(raw.line.contains("system"));
        // 종전 프레임은 원문 없이 그대로.
        let old = frame_of(&json!({"result":{"event":{"phase":"finished"}}})).unwrap();
        assert_eq!(old.raw, None);
    }

    /// 원문 줄이 상태줄을 고친다 — 바뀔 때만, 이 탭이 보낸 턴이어도.
    #[test]
    fn a_raw_line_updates_the_status_line_only_when_it_changes() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        let init = r#"{"type":"system","subtype":"init","model":"claude-haiku-4-5-20251001"}"#;
        let p = |raw: &str| {
            json!({"session":"s1","event":{"phase":"started"},
                   "raw":raw,"format":crate::domain::usage::CLAUDE_STREAM_JSON})
        };
        assert_eq!(
            s.apply(&h, &mut c, &p(init)),
            vec![7],
            "상태줄을 다시 그리라고 안 했다"
        );
        assert_eq!(
            c[&7].usage.model.as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        assert!(
            s.apply(&h, &mut c, &p(init)).is_empty(),
            "같은 값에 또 그리라고 했다"
        );
    }

    /// 워커가 턴 끝에 실은 잔액 대비 사용이 상태줄의 Total 칸이 된다.
    #[test]
    fn a_budget_from_the_worker_becomes_the_total_item() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        let p = json!({"session":"s1","event":{"phase":"finished","ok":true},
                       "budget":{"spent":18.17,"balance":50.0}});
        assert!(
            s.apply(&h, &mut c, &p).contains(&7),
            "상태줄을 다시 그리라고 안 했다"
        );
        assert!(
            crate::adapters::driving::statusline::render_html(&c[&7], false)
                .contains("$18.17/$50.00")
        );
        assert!(
            !s.apply(&h, &mut c, &p).contains(&7),
            "같은 값에 또 그리라고 했다"
        );
    }

    /// 턴이 끝나면 그 세션을 든 탭에 완료 표시를 켠다 — 시작·진행 이벤트에는 안 켠다.
    #[test]
    fn a_finished_turn_marks_the_tab_done() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"started"}}),
        );
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"output","text":"x"}}),
        );
        assert!(h.1.borrow().is_empty(), "끝나기 전에 켰다");
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"finished","ok":true}}),
        );
        assert_eq!(*h.1.borrow(), vec![(7, "completion".to_string())]);
        s.apply(
            &h,
            &mut c,
            &json!({"session":"other","event":{"phase":"finished","ok":true}}),
        );
        assert_eq!(h.1.borrow().len(), 1, "다른 세션의 턴에 이 탭을 켰다");
    }

    /// 원문 `result` 줄을 놓치고 `finished` 만 받은 턴 — 워커가 실은 컨텍스트로 사용량을 채우고 턴 끝
    /// 판정을 연다(실측 2026-10-26: 긴 턴 5 중 3 이 이 상태였다).
    #[test]
    fn a_finished_turn_without_its_result_line_is_still_judged() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        c.get_mut(&7).unwrap().usage.context_used = Some(536_000);
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"started"}}),
        );
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"finished","ok":true},"context":914_000}),
        );
        let convo = c.get(&7).unwrap();
        assert!(
            convo.sync.result_seen,
            "결과 줄 없이 끝난 턴이 판정되지 않았다"
        );
        assert_eq!(
            convo.usage.context_used,
            Some(914_000),
            "낡은 컨텍스트로 판정한다"
        );
    }

    /// 원문 `result` 줄로 이미 판정한 턴은 `finished` 가 다시 열지 않는다 — 캐시 감시가 한 턴을 두 번 센다.
    #[test]
    fn a_turn_judged_by_its_result_line_is_not_judged_again() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"started"}}),
        );
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"output","text":"x"},
                    "raw":r#"{"type":"result","usage":{"input_tokens":3,"cache_read_input_tokens":97,"cache_creation_input_tokens":0,"output_tokens":0}}"#,
                    "format":crate::domain::usage::CLAUDE_STREAM_JSON}),
        );
        let convo = c.get_mut(&7).unwrap();
        assert!(convo.sync.result_seen);
        convo.sync.result_seen = false; // plugin 이 판정하고 거뒀다
        s.apply(
            &h,
            &mut c,
            &json!({"session":"s1","event":{"phase":"finished","ok":true},"context":5}),
        );
        let convo = c.get(&7).unwrap();
        assert!(!convo.sync.result_seen, "같은 턴을 두 번 판정한다");
        assert_eq!(
            convo.usage.context_used,
            Some(100),
            "판정한 턴의 값을 덮었다"
        );
    }

    /// 잘 붙어 있던 연결은 바로 다시 붙고, 붙자마자 끊긴 연결만 기다린다 — 끝 이벤트를 놓치는 틈을 줄인다.
    #[test]
    fn a_connection_that_lived_reconnects_at_once() {
        assert_eq!(
            reconnect_delay(std::time::Duration::from_secs(30)),
            RECONNECT_QUICK
        );
        assert_eq!(
            reconnect_delay(std::time::Duration::from_millis(50)),
            RECONNECT_AFTER
        );
        assert!(RECONNECT_QUICK < std::time::Duration::from_secs(1));
    }

    /// 턴 끝 줄(`result`)이 그 턴의 걸린 시간 · 컨텍스트 토큰을 텔레메트리로 남긴다 — 다른 줄은 안 남긴다.
    #[test]
    fn a_result_line_records_turn_metrics() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", false),
        );
        let p = |raw: &str| {
            json!({"session":"s1","event":{"phase":"output","text":"x"},
                   "raw":raw,"format":crate::domain::usage::CLAUDE_STREAM_JSON})
        };
        s.apply(&h, &mut c, &p(r#"{"type":"assistant","message":{}}"#));
        assert!(h.2.borrow().is_empty(), "턴 끝이 아닌데 남겼다");
        s.apply(&h, &mut c, &p(r#"{"type":"result","duration_ms":1504,"usage":{"input_tokens":3,"cache_read_input_tokens":97,"cache_creation_input_tokens":0,"output_tokens":0}}"#));
        assert_eq!(
            *h.2.borrow(),
            vec![
                (1504, "wall_time_ms".to_string()),
                (100, "input_tokens".to_string())
            ]
        );
        // 값이 남아 있어도 턴 끝이 아닌 줄은 다시 남기지 않는다 — 매 줄 남기면 한 턴이 여러 점이 된다.
        s.apply(&h, &mut c, &p(r#"{"type":"assistant","message":{}}"#));
        assert_eq!(h.2.borrow().len(), 2, "턴 끝이 아닌 줄에 또 남겼다");
    }

    /// 끝난 턴이 hook 을 발사한다 — 성공은 `claude-idle`, 실패는 `claude-error`, 이 탭이 멈춘 턴과
    /// claude 가 아닌 세션은 발사하지 않는다.
    #[test]
    fn a_finished_turn_fires_the_claude_hooks() {
        let fin =
            |ok: bool| json!({"session":"s1","event":{"phase":"finished","ok":ok,"reason":"x"}});
        let fired = |c: &mut Convos, ok: bool| {
            let (mut s, h) = (Streams::default(), RecordingHost::default());
            s.apply(&h, c, &fin(ok));
            h.2.borrow()
                .iter()
                .map(|(_, e)| e.clone())
                .collect::<Vec<_>>()
        };
        let mut c = convos_with("s1", false);
        assert_eq!(fired(&mut c, true), vec!["claude-idle"]);
        assert_eq!(fired(&mut c, false), vec!["claude-error"]);
        // 이 탭이 중지한 턴 — 마감 전(표시가 남음)과 마감 뒤(멈춘 턴으로 적힘) 둘 다.
        c.get_mut(&7).unwrap().stop_requested = true;
        assert!(fired(&mut c, false).is_empty(), "중지한 턴을 오류로 알렸다");
        let cv = c.get_mut(&7).unwrap();
        cv.stop_requested = false;
        cv.turns.push(Turn {
            sent: "x".into(),
            got: String::new(),
            error: None,
            pending: false,
            at: None,
        });
        cv.stopped.push(cv.turns.len() - 1);
        assert!(
            fired(&mut c, false).is_empty(),
            "마감 뒤 중지한 턴을 오류로 알렸다"
        );
        // claude 가 아닌 세션은 이름이 안 맞는다.
        let mut p = convos_with("s1", false);
        p.get_mut(&7).unwrap().agent_kind = Some("pi".into());
        assert!(fired(&mut p, true).is_empty());
    }

    /// 워커가 실은 세션 이름이 탭에 붙고, 상태줄을 다시 그릴 탭으로 돌아온다 — 바뀔 때만.
    #[test]
    fn a_name_from_the_worker_reaches_the_tab() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        let p = json!({"session":"s1","name":"agent-sample-18"});
        assert_eq!(s.apply(&h, &mut c, &p), vec![7]);
        assert_eq!(c[&7].display_name(), Some("agent-sample-18"));
        assert!(
            s.apply(&h, &mut c, &p).is_empty(),
            "같은 이름에 또 그리라고 했다"
        );
    }

    /// 상태줄 값이 바뀌면 그 탭의 meta 에 남긴다 — 다시 뜬 plugin 이 그 값을 되읽어 다음 턴 전에도 그린다.
    #[test]
    fn a_changed_status_line_is_kept_in_the_surface_meta() {
        #[derive(Default)]
        struct Meta(std::cell::RefCell<Vec<(u64, String, String)>>);
        impl HostCall for Meta {
            fn call(&self, method: &str, p: Value) -> Result<Value, String> {
                if method == "surface.meta.set" {
                    self.0.borrow_mut().push((
                        p["surface_id"].as_u64().unwrap_or_default(),
                        p["key"].as_str().unwrap_or_default().into(),
                        p["value"].as_str().unwrap_or_default().into(),
                    ));
                }
                Ok(Value::Null)
            }
        }
        let (mut s, h, mut c) = (
            Streams::default(),
            Meta::default(),
            convos_with("s1", false),
        );
        let init = r#"{"type":"system","subtype":"init","model":"claude-haiku-5-5","permissionMode":"default","apiKeySource":"none","claude_code_version":"2.1.304"}"#;
        let p = json!({"session":"s1","event":{"phase":"output"},"raw":init,"format":crate::domain::usage::CLAUDE_STREAM_JSON});
        s.apply(&h, &mut c, &p);
        let saved = h.0.borrow().clone();
        assert_eq!(saved.len(), 1, "{saved:?}");
        assert_eq!(
            (saved[0].0, saved[0].1.as_str()),
            (7, crate::application::surface_link::USAGE_META_KEY)
        );
        let back = crate::domain::usage::from_snapshot(&serde_json::from_str(&saved[0].2).unwrap());
        assert_eq!(back, c[&7].usage);
        // 같은 줄이 또 오면 값이 안 바뀌므로 다시 쓰지 않는다.
        s.apply(&h, &mut c, &p);
        assert_eq!(h.0.borrow().len(), 1);
    }

    /// 워커가 실은 프로필 계정이 탭에 붙는다 — 다른 세션의 탭에는 안 붙고, 같은 값엔 다시 안 그린다.
    #[test]
    fn an_account_from_the_worker_reaches_the_tab() {
        let (mut s, h, mut c) = (
            Streams::default(),
            RecordingHost::default(),
            convos_with("s1", true),
        );
        let p = json!({"session":"s1","email":"me@example.com"});
        assert_eq!(s.apply(&h, &mut c, &p), vec![7]);
        assert_eq!(c[&7].account_email.as_deref(), Some("me@example.com"));
        assert!(s.apply(&h, &mut c, &p).is_empty());
        let other = json!({"session":"s2","email":"x@example.com"});
        assert!(s.apply(&h, &mut c, &other).is_empty());
        assert_eq!(c[&7].account_email.as_deref(), Some("me@example.com"));
    }

    /// 답은 `textContent` 로 들어간다 — 태그가 실행되지 않는다.
    #[test]
    fn the_appended_answer_is_text_not_markup() {
        let t = Turn {
            sent: String::new(),
            got: "<img src=x onerror=alert(1)></script>".into(),
            error: None,
            pending: false,
            at: None,
        };
        let m = append_msg(&t);
        assert_eq!(m["type"], "turn.auto");
        assert_eq!(m["cls"], "got");
        assert_eq!(
            m["text"], "<img src=x onerror=alert(1)></script>",
            "원문 그대로 — 처리기가 글로 넣는다"
        );
    }
}

#[cfg(test)]
mod hookless_finish_tests {
    use super::hook_of_finished;
    use crate::domain::convo::Convo;
    use serde_json::json;

    #[test]
    fn an_error_result_turns_an_ok_finish_into_claude_error() {
        // U-G6
        let mut c = Convo::default();
        assert_eq!(
            hook_of_finished(&c, &json!({"ok": true})),
            Some("claude-idle")
        );
        c.hookless.turn_error = true;
        assert_eq!(
            hook_of_finished(&c, &json!({"ok": true})),
            Some("claude-error")
        );
        c.stop_requested = true;
        assert_eq!(
            hook_of_finished(&c, &json!({"ok": true})),
            Some("claude-idle"),
            "중지한 턴은 오류가 아니다"
        );
    }
}
