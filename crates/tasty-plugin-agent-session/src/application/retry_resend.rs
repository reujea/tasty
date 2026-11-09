//! 429 재전송의 탭 쪽 — 턴 끝에서 예약하고(`note_turn_for_retry`), 때가 되면 사용자의 말과 같은 길로
//! 보낸다(`run_pending_retries`). 언제 · 몇 번은 `retry_429` 가 정한다.

use std::time::Instant;

use crate::application::session_surface;
use crate::domain::retry_429::{
    Due, Plan, RETRY_AFTER, after_turn, due, gave_up_note, is_rate_limited, on_due, waiting_note,
};
use crate::{application::clear, domain::send_queue};

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// `agent_session.turn_done` 의 끝 — 429 로 막힌 턴이면 다시 보낼 말을 예약하고 입력칸 위에 알린다.
    pub(crate) fn note_turn_for_retry(
        &mut self,
        sid: u32,
        sent: Option<&str>,
        got: &Result<String, String>,
        stopped: bool,
    ) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let profile = crate::domain::rate_block::profile_of(c).to_string();
        let now = Instant::now();
        // 현행화 턴의 결과도 센다 — 다시 보내지는 않지만 그 프로필이 막혔는지 풀렸는지는 같다.
        match got {
            Err(e) if is_rate_limited(e) => {
                let b = self.blocks.note_rejected(&profile, now);
                tracing::info!(
                    "[agent_session] surface={sid}: 프로필 {profile:?} 429 {}회째 — {}초 동안 자동 전송을 멈춘다",
                    b.hits,
                    b.until.saturating_duration_since(now).as_secs()
                );
            }
            Ok(_) => {
                if let Some(b) = self.blocks.note_reached(&profile, now) {
                    tracing::info!(
                        "[agent_session] surface={sid}: 프로필 {profile:?} 429 차단이 풀렸다 ({}초 · {}회)",
                        now.saturating_duration_since(b.since).as_secs(),
                        b.hits
                    );
                }
            }
            Err(_) => {}
        }
        let long = self.blocks.is_long(&profile, now);
        let wait = self
            .blocks
            .get(&profile)
            .map(|b| b.until.saturating_duration_since(now))
            .unwrap_or(RETRY_AFTER);
        let since = self
            .blocks
            .get(&profile)
            .map(|b| now.saturating_duration_since(b.since))
            .unwrap_or_default();
        let syncing = c.sync.running();
        let note = match after_turn(&mut c.retry, sent, got, stopped, syncing, long, now) {
            Plan::Ignore => return,
            Plan::Retry(attempt) => {
                if let Err(e) = crate::application::surface_link::record_metric(
                    &host,
                    sid,
                    "turn_429_retry",
                    f64::from(attempt),
                ) {
                    tracing::debug!("[agent_session] surface={sid}: 429 텔레메트리 실패: {e:?}");
                }
                tracing::info!(
                    "[agent_session] surface={sid}: 429 — {attempt}번째 재전송을 예약한다"
                );
                waiting_note(&profile, wait.max(RETRY_AFTER))
            }
            Plan::GiveUp => {
                tracing::warn!(
                    "[agent_session] surface={sid}: 프로필 {profile:?} 이 오래 막혀 재전송을 멈춘다"
                );
                gave_up_note(&profile, since)
            }
        };
        session_surface::patch(&host, sid, &clear::note_msg(Some(&note)));
    }

    /// `flush_queues` 의 끝 — 때가 된 재전송을 사용자의 말과 같은 길로 보낸다.
    pub(crate) fn run_pending_retries(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let now = Instant::now();
        let due_sids: Vec<u32> = self
            .convos
            .iter()
            .filter(|(_, c)| due(&c.retry, now))
            .filter(|(_, c)| !c.clearing && c.needs_input.is_none() && !c.sync.running())
            .map(|(&sid, _)| sid)
            .collect();
        let mut ready: Vec<(u32, String)> = Vec::new();
        for sid in due_sids {
            let Some(c) = self.convos.get_mut(&sid) else {
                continue;
            };
            let profile = crate::domain::rate_block::profile_of(c).to_string();
            match on_due(&mut self.blocks, &profile, now) {
                Due::Send => {}
                // 차단기가 허락한 하나만 — 같은 프로필의 나머지는 그 결과를 기다린다.
                Due::Wait => continue,
                // 긴 차단 — 더 두드리지 않고 사용자에게 넘긴다.
                Due::GiveUp => {
                    c.retry.pending = None;
                    c.retry.tries = 0;
                    let since = self
                        .blocks
                        .get(&profile)
                        .map(|b| now.saturating_duration_since(b.since))
                        .unwrap_or_default();
                    tracing::warn!(
                        "[agent_session] surface={sid}: 프로필 {profile:?} 이 오래 막혀 재전송을 멈춘다"
                    );
                    let note = gave_up_note(&profile, since);
                    session_surface::patch(&host, sid, &clear::note_msg(Some(&note)));
                    continue;
                }
            }
            let busy = self.activity.turn_busy(c);
            let Some(text) = c.retry.pending.take().map(|p| p.text) else {
                continue;
            };
            if let Some(t) = send_queue::take_or_queue(c, &text, busy) {
                ready.push((sid, t));
            }
        }
        for (sid, text) in ready {
            session_surface::patch(&host, sid, &clear::note_msg(None));
            tracing::info!("[agent_session] surface={sid}: 429 로 막혔던 말을 다시 보낸다");
            self.send_turn(&host, sid, &text, None);
        }
    }
}
