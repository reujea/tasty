//! `agent_session.tell` — 답을 **나중에** 돌려주는 턴 보내기.
//!
//! # 왜 미루나
//!
//! `send_input` 은 턴이 끝나야 돌아온다. 그것을 처리기 안에서 부르면 그동안 plugin
//! 의 worker 가 서서, 다른 대화 탭도 호스트의 생존 확인(`ping`)도 기다린다. 턴이 60 초를
//! 넘기면 호스트가 plugin 을 응답 없음으로 재시작했다(실측 2026-10-13: 80 초 턴 · 60 초째
//! 재시작 · `tell` 은 `-32004` 를 받았다). 그래서 처리기는 답할 권리를 받아(SDK
//! `IpcMethodCtx::defer`) 곧바로 돌아오고, 답은 턴이 끝난 자리에서 보낸다.
//!
//! # 두 길
//!
//! - **대화 탭이 있는 surface** — 탭의 대기열로 넣는다(`send_queue`). 그 탭이 보내는
//!   말과 겹치지 않고, 보낸 말과 답이 탭에도 보인다. 쌓였다가 다른 말과 합쳐져 나가면
//!   합쳐진 턴의 답으로 답한다.
//! - **탭이 없는 surface**(터미널에 이어 둔 세션 등) — 워커가 보내고 답한다.
//!
//! # 상한
//!
//! 호스트는 namespace 호출 하나를 150 초까지 기다리고, 만료가 연달아 세 번이면 plugin 을
//! 재시작한다. 그래서 [`BUDGET`] 안에 답이 없으면 **아직 도는 중**이라고 먼저 답한다 —
//! 실패가 아니다. 턴은 데몬에서 계속 돌고, 답은 탭과 `supervisor output` 에 남는다.

use crate::ports::host::SendHost;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::application::app_error::AppError;
use crate::application::request::{linked_error, session_for, tell_text};

/// 답을 기다리는 상한 — 호스트의 namespace 호출 상한(150 초)보다 넉넉히 짧게.
pub const BUDGET: Duration = Duration::from_secs(120);

/// 답을 보내는 한 번짜리 함수 — 실물은 SDK 의 `DeferredReply::send` 를 감싼 것이다(`ipc::defer_reply`). 시험이 가짜를 넣을 수
/// 있게 함수로 받는다. 못 보냈으면 `false`.
pub type Reply = Box<dyn FnOnce(Result<Value, AppError>) -> bool + Send>;

/// 답할 권리 — 턴이 끝난 자리와 상한 감시가 함께 들고, 먼저 꺼낸 쪽이 답한다.
type Slot = Arc<Mutex<Option<Reply>>>;

/// 대화 탭의 턴을 기다리는 `tell` 하나.
struct Waiter {
    sid: u32,
    session: String,
    /// 이 말이 실린 턴 — 대기열에 있으면 아직 `None` 이다.
    at: Option<usize>,
    slot: Slot,
}

#[derive(Default)]
pub struct Tells {
    waiters: Vec<Waiter>,
}

impl Tells {
    /// 대화 탭 길 — 턴이 정해지면 [`Self::bind`], 끝나면 [`Self::settle`] 이 답한다.
    pub fn park(&mut self, sid: u32, session: String, reply: Reply, budget: Duration) {
        let slot = watched(reply, sid, session.clone(), budget);
        self.waiters.push(Waiter {
            sid,
            session,
            at: None,
            slot,
        });
    }

    /// 이 탭에서 턴 `at` 이 나갔다 — 대기열에 있던 `tell` 은 모두 그 턴에 실렸다.
    pub fn bind(&mut self, sid: u32, at: usize) {
        for w in self
            .waiters
            .iter_mut()
            .filter(|w| w.sid == sid && w.at.is_none())
        {
            w.at = Some(at);
        }
    }

    /// 턴 `at` 을 기다리는 `tell` 이 있나 — 답을 만들기 전에 묻는다(표시 거두기는 그때만).
    pub fn waiting(&self, sid: u32, at: usize) -> bool {
        self.waiters
            .iter()
            .any(|w| w.sid == sid && w.at == Some(at))
    }

    /// 턴 `at` 이 끝났다 — 그 턴을 기다리던 `tell` 에 답한다. `answer` 는 세션 id 를 받아 답을 만든다.
    pub fn settle(
        &mut self,
        sid: u32,
        at: usize,
        answer: impl Fn(&str) -> Result<Value, AppError>,
    ) {
        let (done, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiters)
            .into_iter()
            .partition(|w| w.sid == sid && w.at == Some(at));
        self.waiters = rest;
        for w in done {
            send(&w.slot, answer(&w.session));
        }
    }

    /// 탭이 닫혔다 — 그 탭의 턴을 기다리던 `tell` 은 답을 못 받는다.
    pub fn drop_surface(&mut self, sid: u32) {
        let (gone, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiters)
            .into_iter()
            .partition(|w| w.sid == sid);
        self.waiters = rest;
        for w in gone {
            send(
                &w.slot,
                Err(AppError::new(crate::domain::i18n::t("sup.tell.tab_closed"))),
            );
        }
    }
}

/// 탭이 없는 길 — 워커가 보내고 답한다. 상한 감시는 같다.
pub fn direct<S: crate::ports::supervisor::Supervisor + Send + 'static>(
    host: impl SendHost,
    sup: S,
    reply: Reply,
    sid: u32,
    session: String,
    params: Value,
) {
    let slot = watched(reply, sid, session, BUDGET);
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-tell-{sid}"))
        .spawn({
            let slot = slot.clone();
            move || {
                let out = send_now(&host, &sup, &params);
                send(&slot, out);
            }
        })
        .is_ok();
    if !spawned {
        send(
            &slot,
            Err(AppError::new(crate::domain::i18n::t(
                "sup.err.worker_spawn",
            ))),
        );
    }
}

/// 이 surface 의 세션에 **턴을 넣고 답을 받는다** — 탭이 없는 길. 턴이 끝날 때까지 돌아오지 않는다.
///
/// 재워 둔 세션이면 그쪽이 자동 재개하므로 여기서 `resume` 을 먼저 부르지
/// 않는다 — 같은 검사를 두 곳에 두면 한쪽이 죽은 조건이 된다.
pub fn send_now(
    host: &impl crate::ports::host::HostCall,
    sup: &impl crate::ports::supervisor::Supervisor,
    params: &Value,
) -> Result<Value, AppError> {
    let (surface_id, session) = session_for(host, params)?;
    let text = tell_text(params)?;
    let out = sup
        .send_input(&session, &text)
        .map_err(|e| linked_error(surface_id, &session, e))?;
    // 턴이 돌았으면 깨어 있다 — 회수 표시를 거둔다. 안 거두면 하이라이트가 남아
    // *"아직 재워져 있다"* 고 거짓말한다. 그 턴이 켠 완료 표시는 남긴다(`clear_reclaimed_mark`).
    let cleared = crate::application::surface_link::clear_reclaimed_mark(host, surface_id);
    Ok(json!({
        "surface_id": surface_id,
        "session_id": session,
        "result": out,
        "cleared": matches!(cleared, Ok(true)),
    }))
}

/// 대화 탭의 턴이 답을 받았을 때 `tell` 이 돌려주는 모양 — 탭 없는 길(`handle_tell`)과 같다.
pub fn answer(sid: u32, session: &str, raw: &Value, cleared: bool) -> Value {
    json!({
        "surface_id": sid,
        "session_id": session,
        "result": raw,
        "cleared": cleared,
    })
}

/// 상한 안에 답이 없을 때의 모양 — 실패가 아니라 "아직 도는 중" 이다.
pub fn still_running(sid: u32, session: &str) -> Value {
    json!({
        "surface_id": sid,
        "session_id": session,
        "answered": false,
        "note": crate::domain::i18n::t("sup.tell.still_running"),
    })
}

/// 탭이 물음(권한·질문)에 답을 기다릴 때의 모양 — 말은 대기열에 들어갔고 물음은 그대로다.
///
/// 보낸 말이 물음의 답이 된다고 여기면 상대는 풀리지 않은 물음 앞에서 계속 기다린다. 물음은 탭에서 답한다.
pub fn queued_behind_question(sid: u32, session: &str) -> Value {
    json!({
        "surface_id": sid,
        "session_id": session,
        "answered": false,
        "queued": true,
        "warning": crate::domain::i18n::t("sup.tell.queued_behind_question"),
    })
}

/// 탭이 물음(권한 요청 · `AskUserQuestion`)에 답을 기다리는가 — 그동안 보낸 말은 물음을 풀지 못한다.
pub fn waits_on_question(c: &crate::domain::convo::Convo) -> bool {
    c.needs_input.is_some() || c.hookless.ask.is_some()
}

/// 답할 권리를 칸에 넣고 상한 감시를 붙인다.
fn watched(reply: Reply, sid: u32, session: String, budget: Duration) -> Slot {
    let slot: Slot = Arc::new(Mutex::new(Some(reply)));
    let watch = slot.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-tell-budget-{sid}"))
        .spawn(move || {
            std::thread::sleep(budget);
            send(&watch, Ok(still_running(sid, &session)));
        })
        .is_ok();
    if !spawned {
        // 감시가 없으면 턴이 길 때 호스트 상한에 걸린다 — 그래도 답은 턴이 끝난 자리가 보낸다.
        tracing::warn!("agent_session.tell surface {sid}: 상한 감시를 못 띄웠다");
    }
    slot
}

/// 칸에 답할 권리가 남아 있으면 답한다 — 이미 다른 쪽이 답했으면 아무것도 안 한다.
fn send(slot: &Slot, result: Result<Value, AppError>) {
    // poison 이어도 칸을 연다 — 넘겨 버리면 그 호출이 끝내 답을 못 받는다.
    let reply = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(r) = reply
        && !r(result)
    {
        tracing::warn!("agent_session.tell: 답을 못 보냈다 — plugin 이 내려가는 중이다");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// 답을 받는 가짜 — 보낸 답을 채널로 넘긴다.
    fn probe() -> (Reply, mpsc::Receiver<Result<Value, AppError>>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(move |r| tx.send(r).is_ok()), rx)
    }

    const LONG: Duration = Duration::from_secs(3600);

    /// 대기열에 있던 `tell` 은 **자기가 실린 턴**의 답을 받는다 — 앞서 돌던 턴의 답이 아니다.
    #[test]
    fn a_queued_tell_waits_for_the_turn_it_rides_on() {
        let mut t = Tells::default();
        let (reply, rx) = probe();
        t.park(7, "s1".into(), reply, LONG);
        // 앞 턴 0 이 끝났다 — 이 `tell` 은 아직 대기열에 있다.
        assert!(!t.waiting(7, 0));
        t.settle(7, 0, |_| Ok(json!("wrong")));
        assert!(rx.try_recv().is_err(), "앞 턴의 답을 받았다");
        // 대기열이 턴 1 로 나갔다.
        t.bind(7, 1);
        assert!(t.waiting(7, 1));
        t.settle(7, 1, |session| {
            Ok(answer(7, session, &json!({"output":"A"}), true))
        });
        let got = rx.try_recv().expect("답이 안 왔다").expect("오류로 왔다");
        assert_eq!(got["session_id"], "s1");
        assert_eq!(got["result"]["output"], "A");
        assert!(!t.waiting(7, 1), "답한 뒤에도 남았다");
    }

    /// 다른 탭의 턴은 이 `tell` 과 무관하다.
    #[test]
    fn another_tabs_turn_does_not_answer() {
        let mut t = Tells::default();
        let (reply, rx) = probe();
        t.park(7, "s1".into(), reply, LONG);
        t.bind(8, 0);
        t.settle(8, 0, |_| Ok(json!("wrong")));
        assert!(rx.try_recv().is_err());
    }

    /// 상한 안에 답이 없으면 "아직 도는 중" 으로 먼저 답하고, 늦게 온 턴의 답은 다시 안 보낸다.
    #[test]
    fn the_budget_answers_first_and_only_once() {
        let mut t = Tells::default();
        let (reply, rx) = probe();
        t.park(7, "s1".into(), reply, Duration::from_millis(50));
        let first = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("상한이 답하지 않았다")
            .expect("오류로 왔다");
        assert_eq!(first["answered"], false);
        t.bind(7, 0);
        t.settle(7, 0, |_| Ok(json!("late")));
        assert!(rx.try_recv().is_err(), "한 호출에 두 번 답했다");
    }

    /// 탭이 닫히면 기다리던 `tell` 은 오류로 끝난다 — 상한까지 기다리지 않는다.
    #[test]
    fn closing_the_tab_fails_its_tells() {
        let mut t = Tells::default();
        let (reply, rx) = probe();
        t.park(7, "s1".into(), reply, LONG);
        t.drop_surface(7);
        assert!(rx.try_recv().expect("답이 안 왔다").is_err());
    }

    /// 물음을 기다리는 탭 — 권한 요청(데몬 `needs_input`)도 `AskUserQuestion`(hookless)도 같은 축이다.
    #[test]
    fn a_tab_waiting_on_a_question_is_told_apart() {
        let mut c = crate::domain::convo::Convo::default();
        assert!(!waits_on_question(&c), "물음이 없는데 기다린다고 했다");
        c.hookless.ask = Some("tu1".into());
        assert!(waits_on_question(&c), "AskUserQuestion 대기를 놓쳤다");
        c.hookless.ask = None;
        c.needs_input = Some(crate::application::prompts::Pending {
            request_id: "r".into(),
            kind: "permission".into(),
            tool: "Bash".into(),
            input: Value::Null,
        });
        assert!(waits_on_question(&c), "권한 요청 대기를 놓쳤다");

        let v = queued_behind_question(3, "s");
        assert_eq!(v["queued"], true);
        assert_eq!(v["answered"], false);
        assert!(v["warning"].as_str().is_some_and(|w| !w.is_empty()));
    }
}
