//! 대화 탭의 **컨텍스트 비우기** — Claude Code 의 `/clear` 와 같은 일이다.
//!
//! supervisor 세션과 탭은 그대로 두고, 그 세션의 에이전트 대화만 새로 시작한다. 데몬에
//! 전용 메서드는 없다 — 세션에 `/clear` 를 입력으로 보내면 claude 가 대화를 갈고
//! (`conversation_reset` 뒤 새 `system/init`), 데몬은 턴이 끝날 때 새 대화 id 를 좌표로
//! 저장한다. 그래서 회수 뒤 재개해도 지운 대화가 되살아나지 않는다.
//!
//! # 들어오는 길
//!
//! 셋이 같은 처리를 부른다 — 탭의 비우기 단추(두 번 눌러 확정), 입력칸에 친 `/clear`,
//! IPC `agent_session.clear`(CLI `tasty agent-session clear`). 마지막 것은 에이전트가 쓰는
//! 길이라 대상 surface 를 id 로 받는다.
//!
//! # 턴이 도는 동안은 거절한다
//!
//! 도는 턴 뒤에 `/clear` 를 보내면 그 턴의 답이 지운 화면에 붙고, 쌓인 말(`queued`)이
//! 비운 대화로 넘어간다. 멈출지 기다릴지는 사용자가 정할 일이라 여기서 고르지 않는다.
//!
//! # 탭에서 지우는 것
//!
//! 화면의 턴 · 디스크로 옮긴 턴(`turn_archive`) · 펼친 이전 대화 · 지난 대화의 상태줄 값
//! (사용량 · 대화 이름 · 에이전트 쪽 대화 id). 대화 id 를 비워 두는 이유는 다음 활동
//! 폴링이 새 값을 채우기 전까지 기록 · 비용 계산이 **옛 대화 파일**을 읽지 않게 하려는
//! 것이다.
use std::collections::HashMap;

use serde_json::{Value, json};

use crate::application::app_error::AppError;

use crate::application::request::session_for;
use crate::application::session_surface;
use crate::application::tell::{self, Reply};
use crate::domain::convo::Convo;
use crate::ports::host::SendHost;
use crate::{adapters::driven::store::turn_archive, domain::i18n};

/// 워커가 `/clear` 턴을 끝내고 되던지는 자기 호출.
pub const METHOD: &str = "agent_session.clear_done";

/// 세션에 보내는 말 — Claude Code 의 슬래시 명령 그대로다.
pub const COMMAND: &str = "/clear";

/// 입력칸에 친 말이 비우기 명령인가 — 앞뒤 공백만 허용한다. `/clear 뭔가` 는 명령이
/// 아니라 말이다(Claude Code 도 인자를 안 받는다).
pub fn is_command(text: &str) -> bool {
    text.trim() == COMMAND
}

/// 대화를 비운다 — 세션 · 탭 설정 · 입력 히스토리 말고 대화에 딸린 것을 전부.
///
/// 입력 히스토리(↑)는 `turns` 의 보낸 말에서 만들므로 함께 비워진다.
pub fn reset(c: &mut Convo) {
    c.turns.clear();
    c.sys.clear();
    c.older.clear();
    c.overdue.clear();
    c.stopped.clear();
    c.stop_requested = false;
    c.archived = 0;
    c.dropped = 0;
    c.agent_session_id = None;
    c.transcript_name = None;
    c.usage = Default::default();
    c.needs_input = None;
    // 에이전트 내역은 새 대화의 기록에서 다시 시작한다.
    c.subs.forget();
    // 대화가 새로 시작됐다 — 현행화 판정도 처음부터(진행 중이던 것은 비우기가 끊었다).
    c.sync = Default::default();
    // 비우기도 사람이 한 조작이다 — 휴식 시계만 민다. 캐시 시계는 비어 다음 턴 끝까지 유휴 현행화를 안 한다.
    c.sync.touch(std::time::Instant::now());
}

/// 결과 한 줄을 보이거나(사유) 거둔다(`None`).
pub fn note_msg(why: Option<&str>) -> serde_json::Value {
    serde_json::json!({ "type": "note.set", "text": why })
}

/// 비우기를 기다리는 IPC 호출 — 탭이 `/clear` 턴을 끝내면 답한다(surface 별 하나).
#[derive(Default)]
pub struct Waiting {
    replies: HashMap<u32, Vec<Reply>>,
}

impl Waiting {
    /// IPC 로 온 비우기면 그 답을 맡는다 — 탭에서 누른 것이면(`None`) 할 일이 없다.
    pub fn park_opt(&mut self, sid: u32, reply: Option<Reply>) {
        if let Some(reply) = reply {
            self.replies.entry(sid).or_default().push(reply);
        }
    }

    /// 그 탭의 비우기가 끝났다 — 기다리던 호출 전부에 같은 결과로 답한다.
    pub fn settle(&mut self, sid: u32, session: &str, got: &Result<(), String>) {
        for reply in self.replies.remove(&sid).unwrap_or_default() {
            let r = match got {
                Ok(()) => Ok(answer(sid, session)),
                Err(why) => Err(AppError::new(why.clone())),
            };
            if !reply(r) {
                tracing::debug!("[agent_session] surface={sid}: 비우기 답을 받을 호출이 이미 없다");
            }
        }
    }

    /// 탭이 닫혔다 — 기다리던 호출에 그 사실로 답한다.
    pub fn drop_surface(&mut self, sid: u32) {
        let why = Err(crate::domain::i18n::t("sup.err.clear_tab_closed"));
        self.settle(sid, "", &why);
    }
}

/// 비우기가 끝났을 때 IPC 가 돌려주는 값.
pub fn answer(sid: u32, session: &str) -> Value {
    json!({ "surface_id": sid, "session_id": session, "cleared": true })
}

/// 워커 결과를 읽는다 — `(surface, 성공이면 Ok, 아니면 사유)`.
pub fn parse_done(params: &Value) -> Option<(u32, Result<(), String>)> {
    let p = params.get("params").unwrap_or(params);
    let sid = u32::try_from(p.get("surface_id")?.as_u64()?).ok()?;
    let ok = p.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let why = p
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((sid, if ok { Ok(()) } else { Err(why) }))
}

/// 탭에 붙지 않은 세션의 비우기 — 워커에서 보내고 결과를 바로 돌려준다.
pub fn direct<S: crate::ports::supervisor::Supervisor + Send + 'static>(
    sup: S,
    reply: Reply,
    sid: u32,
    session: String,
) {
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-clear-{sid}"))
        .spawn({
            let session = session.clone();
            move || {
                let r = sup
                    .send_input(&session, COMMAND)
                    .map(|_| answer(sid, &session))
                    .map_err(|e| AppError::new(e.to_string()));
                if !reply(r) {
                    tracing::debug!(
                        "[agent_session] surface={sid}: 비우기 답을 받을 호출이 이미 없다"
                    );
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("[agent_session] surface={sid}: 비우기 워커를 못 띄웠다: {e}");
    }
}

/// 이 탭의 비우기를 시작한다 — 거절이면 그 사유. 보내는 것은 워커가 하고 끝은 [`METHOD`] 로 온다.
///
/// `busy` 는 호출부가 `activity::Activity::is_busy` 로 잰 값이다(그쪽이 데몬의 턴까지 본다).
pub fn begin<S: crate::ports::supervisor::Supervisor + Send + 'static>(
    host: &impl SendHost,
    sup: S,
    sid: u32,
    c: &mut Convo,
    busy: bool,
) -> Result<(), String> {
    if busy || c.clearing || !c.queued.is_empty() {
        return Err(crate::domain::i18n::t("sup.err.clear_busy"));
    }
    let Some(session) = c.session_id.clone() else {
        return Err(crate::domain::i18n::t("sup.err.clear_no_session"));
    };
    c.clearing = true;
    // 비우기도 이 탭이 연 턴이다 — 그 끝 줄이 "에이전트가 이어서" 로 붙지 않게(`own_turns`).
    c.own_turns.opened();
    let worker_host = host.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-clear-{sid}"))
        .spawn(move || {
            let (ok, error) = match sup.send_input(&session, COMMAND) {
                Ok(_) => (true, String::new()),
                Err(e) => (false, e.to_string()),
            };
            // 되던지기가 실패하면 plugin 이 내려가는 중이다 — 조용히 끝낸다.
            #[expect(clippy::let_underscore_must_use, reason = "A failed self-invoke means the plugin is shutting down; nobody is left to tell.")]
            let _ = worker_host.self_invoke(
                METHOD,
                json!({ "surface_id": sid, "ok": ok, "error": error }),
            );
        });
    if spawned.is_err() {
        c.clearing = false;
        c.own_turns.closed(std::time::Instant::now());
        return Err(crate::domain::i18n::t("sup.err.worker_spawn"));
    }
    Ok(())
}

/// 탭 쪽 처리 — plugin 상태(대화 · 활동 · 대기 호출)를 함께 만지므로 plugin 의 메서드로 둔다.
impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 입력칸에 친 `/clear` 면 비우고 `true` — 페이지가 대기 칸에 올린 임시 항목을 거둔다.
    pub(crate) fn clear_typed(&mut self, host: &impl SendHost, sid: u32, url: &str) -> bool {
        if !crate::adapters::driving::url::parse_input(url).is_some_and(|t| is_command(&t)) {
            return false;
        }
        if let Some(c) = self.convos.get(&sid) {
            let staged = crate::domain::send_queue::parse_nonce(url);
            session_surface::patch(
                host,
                sid,
                &crate::domain::send_queue::settle_msg(staged, &c.queued, None),
            );
        }
        self.clear_from_tab(host, sid, None);
        true
    }

    /// 대화를 비운다(`clear`) — 단추 · 입력칸의 `/clear` · `agent_session.clear` 가 같이 부른다.
    ///
    /// 거절되면 그 사유를 탭에 보이고, IPC 로 왔으면 그 호출에도 같은 사유로 답한다.
    pub(crate) fn clear_from_tab(
        &mut self,
        host: &impl SendHost,
        sid: u32,
        reply: Option<tell::Reply>,
    ) {
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let busy = self.activity.turn_busy(c);
        match begin(host, self.sup, sid, c, busy) {
            Ok(()) => {
                tracing::info!("[agent_session] surface={sid}: 대화를 비우러 보냈다");
                self.clears.park_opt(sid, reply);
                session_surface::patch(host, sid, &note_msg(None));
                if let Some(c) = self.convos.get(&sid) {
                    self.activity.refresh_status(host, sid, c);
                }
            }
            Err(why) => {
                tracing::info!("[agent_session] surface={sid}: 비우기 거절: {why}");
                session_surface::patch(host, sid, &note_msg(Some(&why)));
                if let Some(reply) = reply
                    && !reply(Err(AppError::new(why)))
                {
                    tracing::debug!(
                        "[agent_session] surface={sid}: 비우기 답을 받을 호출이 이미 없다"
                    );
                }
            }
        }
    }

    /// `agent_session.clear` — 대화 탭이면 그 탭을 비우고, 아니면 세션에 바로 `/clear` 를 보낸다.
    pub(crate) fn clear_later(&mut self, params: &Value, reply: tell::Reply) {
        let Some(hh) = self.host.clone() else {
            reply(Err(AppError::new(i18n::t("sup.err.worker_spawn"))));
            return;
        };
        let (sid, session) = match session_for(&hh, params) {
            Ok(t) => t,
            Err(e) => {
                reply(Err(e));
                return;
            }
        };
        if self.convos.contains_key(&sid) {
            self.clear_from_tab(&hh, sid, Some(reply));
        } else {
            direct(self.sup, reply, sid, session);
        }
    }

    /// 비우기 턴이 끝났다 — 성공이면 대화와 보관 파일을 비우고 문서를 다시 싣는다.
    pub(crate) fn clear_done(&mut self, sid: u32, got: Result<(), String>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        c.clearing = false;
        c.own_turns.closed(std::time::Instant::now());
        let session = c.session_id.clone().unwrap_or_default();
        match &got {
            Ok(()) => {
                reset(c);
                turn_archive::remove_for(&session);
                // 남겨 둔 상태줄 값도 비운다 — 안 비우면 다시 뜬 탭이 비우기 전 값을 되살린다.
                crate::application::surface_link::save_usage(
                    &host,
                    sid,
                    &crate::domain::usage::to_snapshot(&c.usage),
                );
                tracing::info!("[agent_session] surface={sid}: 대화를 비웠다");
                // 대화 전체가 바뀌었다 — 고칠 자리가 없으니 문서를 다시 싣는다.
                let busy = self.activity.is_busy(c);
                session_surface::push(&host, &self.sup, sid, c, busy);
            }
            Err(why) => {
                tracing::warn!("[agent_session] surface={sid}: 비우기 실패: {why}");
                let why = i18n::t1("sup.err.clear_failed", why);
                session_surface::patch(&host, sid, &note_msg(Some(&why)));
            }
        }
        if let Some(c) = self.convos.get(&sid) {
            self.activity.refresh_status(&host, sid, c);
        }
        self.clears.settle(sid, &session, &got);
        self.flush_queues();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bare_command_clears() {
        assert!(is_command("/clear"));
        assert!(is_command("  /clear\n"));
        assert!(!is_command("/clear 이건 말이다"));
        assert!(!is_command("/clearer"));
        assert!(!is_command("clear"));
    }

    #[test]
    fn reset_empties_the_conversation_but_keeps_the_session() {
        let mut c = Convo {
            session_id: Some("s1".into()),
            agent_session_id: Some("old-conv".into()),
            archived: 3,
            dropped: 2,
            stop_requested: true,
            cwd_pick: "/w".into(),
            ..Default::default()
        };
        crate::domain::convo::begin_turn(&mut c, "hi");
        c.older.push(c.turns[0].clone());
        reset(&mut c);
        assert!(c.turns.is_empty() && c.older.is_empty());
        assert_eq!((c.archived, c.dropped), (0, 0));
        assert!(!c.stop_requested);
        // 옛 대화 id 를 들고 있으면 기록 · 비용이 지운 대화 파일을 읽는다.
        assert_eq!(c.agent_session_id, None);
        assert_eq!(c.session_id.as_deref(), Some("s1"));
        assert_eq!(c.cwd_pick, "/w");
    }

    #[test]
    fn the_done_call_carries_the_surface_and_the_reason() {
        let ok = json!({ "params": { "surface_id": 4, "ok": true } });
        assert_eq!(parse_done(&ok), Some((4, Ok(()))));
        let bad = json!({ "surface_id": 4, "ok": false, "error": "no daemon" });
        assert_eq!(parse_done(&bad), Some((4, Err("no daemon".into()))));
    }

    #[test]
    fn the_note_text_cannot_close_the_script() {
        // 글은 데이터로 가고 처리기가 `textContent` 로 넣는다 — 원문 그대로 싣는다.
        assert_eq!(note_msg(Some("</script><b>x"))["text"], "</script><b>x");
        assert_eq!(note_msg(None), json!({ "type": "note.set", "text": null }));
    }
}
