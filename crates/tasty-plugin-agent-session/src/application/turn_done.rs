//! 워커가 되던진 끝 — 턴 · 맥락 턴 · 활동 폴링의 결과를 대화 탭에 반영한다.
//!
//! 셋 다 워커만 부르는 자기 호출(`self_invoke`)이다. 바깥에 열려 있어도 하는 일은 *"그 자리에 이 결과를
//! 넣는다"* 뿐이라 데몬을 건드리지 않는다.

use serde_json::{Value, json};

use crate::application::app_error::AppError;
use crate::application::request::require_surface;
use crate::ports::host::SendHost;
use crate::{
    adapters::driven::skills, adapters::driven::store::turn_archive, application::session_surface,
    application::surface_link, application::tell, domain::commands,
};

/// 맥락 워커가 끝을 알리는 자기 호출 — `begin_started` 가 띄운 워커만 부른다.
pub(crate) const BRIEF_DONE: &str = "agent_session.brief_done";

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// `agent_session.turn_done` — 워커가 답을 들고 돌아왔다. 그 턴의 자리를 채우고 다시 그린다.
    pub(crate) fn turn_done(
        &mut self,
        host: &impl SendHost,
        params: &Value,
    ) -> Result<Value, AppError> {
        let sid = require_surface(params)?;
        let at = params
            .pointer("/params/at")
            .or_else(|| params.get("at"))
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
        let ok = params
            .pointer("/params/ok")
            .or_else(|| params.get("ok"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = params
            .pointer("/params/text")
            .or_else(|| params.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let overdue = params
            .pointer("/params/overdue")
            .or_else(|| params.get("overdue"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if overdue {
            tracing::warn!(
                "[agent_session] surface={sid}: 턴 {at} 이 상한을 넘겼다 — 답은 구독으로 받는다 ({text})"
            );
            if let Some(c) = self.convos.get_mut(&sid)
                && crate::domain::convo::mark_overdue(c, at)
            {
                session_surface::patch(host, sid, &session_surface::overdue_msg(at));
            }
            self.tells
                .settle(sid, at, |session| Ok(tell::still_running(sid, session)));
            return Ok(json!({ "surface_id": sid, "at": at, "overdue": true }));
        }
        // 커맨드가 답 글을 비우면(`/compact`) 무슨 일이 있었는지 적는다(`commands`).
        let sent = self
            .convos
            .get(&sid)
            .and_then(|c| c.turns.get(at))
            .map(|t| t.sent.clone());
        // 스킬 · 플러그인을 다시 읽은 턴이면 `/` 목록도 다시 훑는다(`skills`).
        if ok
            && sent.as_deref().is_some_and(skills::reloads)
            && let Some(c) = self.convos.get(&sid)
            && let Some(session) = c.session_id.as_deref()
        {
            skills::rescan(sid, session, c.agent_kind.as_deref());
        }
        let got = if ok {
            Ok(commands::shown_answer(
                sent.as_deref().unwrap_or_default(),
                text,
            ))
        } else {
            Err(text)
        };
        if self.tells.waiting(sid, at) {
            let raw = params.get("raw").cloned().unwrap_or(Value::Null);
            // 턴이 돌았으면 깨어 있다 — 탭 없는 길(`handle_tell`)처럼 표시를 거둔다.
            let cleared = surface_link::clear_reclaimed(host, sid).is_ok();
            let got = got.clone();
            self.tells.settle(sid, at, |session| match &got {
                Ok(_) => Ok(tell::answer(sid, session, &raw, cleared)),
                Err(why) => Err(AppError::new(why.clone())),
            });
        }
        let mut stopped = false;
        if let Some(c) = self.convos.get_mut(&sid) {
            crate::domain::convo::finish_turn(c, at, got.clone());
            stopped = c.stopped.contains(&at);
        }
        self.note_turn_for_retry(sid, sent.as_deref(), &got, stopped);
        //  **문서를 다시 싣지 않는다**(0.1.24). `push` 를 쓰면 답이
        // 올 때마다 화면이 깜빡이고 **입력 중이던 텍스트가 사라진다**
        // (실사용 보고 2026-09-30). 그 자리만 고친다.
        let script = if stopped {
            session_surface::stopped_msg(at)
        } else {
            session_surface::finish_msg(at, &got)
        };
        session_surface::patch(host, sid, &script);
        // 쌓인 말을 보내기 **전에** 자른다 — 보내면 새 턴이 돌기 시작해 자를 수 없다.
        // 상태줄보다도 먼저 — 그 줄의 주고받은 수가 자른 뒤의 값을 보여야 한다.
        if let Some(c) = self.convos.get_mut(&sid) {
            turn_archive::enforce_on_tab(host, sid, c);
        }
        if let Some(c) = self.convos.get(&sid) {
            self.activity.refresh_status(host, sid, c);
        }
        self.flush_queues();
        Ok(json!({ "surface_id": sid, "at": at }))
    }

    /// 맥락 워커가 끝났다 — 바쁨을 풀고, 쌓인 첫 메시지를 이 탭의 턴으로 보낸다.
    pub(crate) fn brief_done(
        &mut self,
        host: &impl SendHost,
        params: &Value,
    ) -> Result<Value, AppError> {
        let sid = require_surface(params)?;
        if let Some(c) = self.convos.get_mut(&sid) {
            c.briefing = false;
            c.own_turns.closed(std::time::Instant::now());
        }
        if let Some(c) = self.convos.get(&sid) {
            self.activity.refresh_status(host, sid, c);
        }
        self.flush_queues();
        Ok(json!({ "surface_id": sid }))
    }

    /// 워커가 데몬의 `turn_active` 를 들고 돌아왔다(`activity` 모듈) — 표시 · 구독 · 밀린 답을 함께 맞춘다.
    pub(crate) fn activity_polled(&mut self, host: &impl SendHost, params: &Value) {
        self.activity.apply(host, &self.convos, params);
        self.activity
            .note_agent_sessions(host, &mut self.convos, params);
        // 같은 주기에 구독도 맞춘다 — 이어진 세션이 바뀐 것은 여기서 드러난다.
        if let Some(h) = self.host.clone() {
            self.streams.sync(&h, self.sup, &self.convos);
        }
        // 데몬은 끝났다는데 탭이 기다리는 overdue 턴을 닫는다(`overdue` 모듈, B37).
        self.close_stale_overdue(host);
        // 복원된 탭 중 한가해진 것은 빠진 답을 채운다(`catch_up` 모듈).
        self.catch_up_idle(host);
        // 도는 서브에이전트가 있는 탭은 패널을 다시 읽는다 — 백그라운드는 부모 줄 없이 끝난다.
        self.refresh_subagents(host);
        self.flush_queues();
    }
}
