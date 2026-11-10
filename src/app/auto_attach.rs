//! 매핑된 워크스페이스의 자동 attach와 재연결을 처리한다.
//! SSH 연결 준비는 워커가 맡고 메인 루프가 결과를 mirror 세션에 적용한다.
//! 재연결은 예약 시각 또는 해당 워크스페이스 재활성화로 시도한다.
//! 자동 재시도 상한에 도달해도 사용자가 다시 활성화하면 재시도할 수 있다.
//! docs/dev-guide/attach-behavior.md#gui-자동-재연결-스코프 참조.

mod notice;

use std::time::Instant;

use tasty_remote_profiles::{Passkeys, RemoteProfiles};
use tasty_ssh::{self as ssh, PortMode, SshTarget, SshTunnel};

use crate::app::App;
use crate::model::WorkspaceAttachTarget;
use crate::view::ui::View as _;
use tasty_remote::client_session::SessionState;
use tasty_remote::outbound::{AutoAttachOutcome, ReconnectSlot};

/// 자동 재연결의 실패 횟수 상한. 워크스페이스 재활성화로 재시도하는 동작은 막지 않는다.
const MAX_RECONNECT_ATTEMPTS: u32 = 20;

fn reconnect_due(slot: Option<&ReconnectSlot>, now: Instant) -> bool {
    match slot {
        Some(slot) if slot.given_up => false,
        Some(slot) => now >= slot.next_attempt,
        None => true,
    }
}

/// 다음 재연결을 예약할 시각. None이어도 중단 상태를 가진 슬롯은 지우지 않는다.
/// 슬롯이 없는 첫 시도는 현재 프레임에서 처리한다.
fn reconnect_wakeup_at(slot: Option<&ReconnectSlot>) -> Option<Instant> {
    match slot {
        Some(slot) if slot.given_up => None,
        Some(slot) => Some(slot.next_attempt),
        None => None,
    }
}

/// 이번 실패까지 포함한 횟수를 받는다.
fn reconnect_exhausted(attempts: u32) -> bool {
    attempts >= MAX_RECONNECT_ATTEMPTS
}

/// 정상 진행 중인 백오프는 워크스페이스 전환만으로 초기화하지 않는다.
fn should_reset_given_up(slot: Option<&ReconnectSlot>, edge_now: bool) -> bool {
    edge_now && slot.is_some_and(|s| s.given_up)
}

impl App {
    /// 활성 워크스페이스의 전환을 한 번 계산해 두 트리거가 같은 전환을 보게 한다.
    pub(crate) fn poll_auto_attach(&mut self) {
        self.prune_first_attach_retries();
        let prev_active = self.remote.last_active_ws;
        let current_ws_id = self
            .focused_pair()
            .and_then(|(main, engine)| {
                engine
                    .core
                    .workspace_at(main.state.active_workspace_index(engine.core))
            })
            .map(|ws| ws.id);
        self.remote.last_active_ws = current_ws_id;
        if let Some(current) = current_ws_id
            && is_reactivation_edge(current_ws_id, prev_active)
        {
            self.remote.refusals.announce(current);
        }

        self.maybe_trigger_auto_attach(current_ws_id, prev_active);
        self.maybe_trigger_reconnect(current_ws_id, prev_active);
        self.drain_auto_attach_results();
        self.sync_mapping_notices();
    }

    /// 워크스페이스가 닫혔거나 매핑이 지워지거나 바뀐 anchor의 첫 attach 재시도 기록을 지운다.
    /// 창과 parked engine을 모두 본다.
    fn prune_first_attach_retries(&mut self) {
        if self.remote.attach_retry.is_empty() && self.remote.closed_mappings.is_empty() {
            return;
        }
        let live: std::collections::HashMap<u32, crate::model::WorkspaceAttachMapping> = self
            .engines()
            .sessions()
            .flat_map(|(_, engine)| {
                engine
                    .workspaces()
                    .into_iter()
                    .filter_map(|ws| ws.attach_mapping.clone().map(|mapping| (ws.id, mapping)))
                    .collect::<Vec<_>>()
            })
            .collect();
        retain_live_first_attach_retries(&mut self.remote, |anchor| live.get(&anchor));
        retain_live_closed_mappings(&mut self.remote, |anchor| live.get(&anchor));
    }

    /// 타이머만 등록·해제한다. 중단 상태를 담은 재연결 슬롯은 보존한다.
    pub(crate) fn sync_reconnect_timers(&mut self, now: Instant) {
        let mut wakeups: Vec<(u32, Instant)> = self
            .remote
            .sessions
            .iter()
            .filter(|s| s.state() == SessionState::Reconnecting)
            .filter_map(|s| s.anchor_ws_id())
            // 워커가 이미 떠 있는 anchor 는 결과 이벤트(`AutoAttachReady`)가 깨운다.
            .filter(|anchor| !self.remote.active.contains(anchor))
            .filter_map(|anchor| {
                reconnect_wakeup_at(self.remote.reconnect.get(&anchor)).map(|at| (anchor, at))
            })
            .collect();
        // 첫 attach의 재시도도 같은 타이머로 깨운다. 한 anchor는 둘 중 이른 시각을 쓴다.
        for (anchor, retry) in &self.remote.attach_retry {
            if self.remote.active.contains(anchor) {
                continue;
            }
            let Some(at) = first_attach_wakeup_at(retry, now) else {
                continue;
            };
            match wakeups.iter_mut().find(|(a, _)| a == anchor) {
                Some((_, existing)) => *existing = (*existing).min(at),
                None => wakeups.push((*anchor, at)),
            }
        }
        crate::app::timers::sync_reconnect_timers(&mut self.timers, &wakeups, now);
    }

    /// 신규 매핑은 활성 상태면 연결하고, 연결 해제 뒤 대기 중인 anchor는 재활성화를 요구한다.
    /// 대상 ID는 매핑에서 가져오며 포커스를 바꾸지 않는다.
    fn auto_attach_candidate(
        &self,
        current_ws_id: Option<u32>,
        prev_active: Option<u32>,
    ) -> Option<(u32, crate::model::WorkspaceAttachMapping, u32)> {
        let candidate = {
            let (main, engine) = self.focused_pair()?;
            let idx = main.state.active_workspace_index(engine.core);
            match engine.workspace_at(idx) {
                Some(ws) => ws.attach_mapping.as_ref().map(|m| (ws.id, m.clone())),
                None => None,
            }
        };
        let (anchor, mapping) = candidate?;
        if !anchor_may_attach(&self.remote, anchor, &mapping, current_ws_id, prev_active) {
            return None;
        }
        // 해석에 실패한 매핑은 간격을 두고 다시 시도한다. 사용자가 이 워크스페이스로 돌아오면 바로 시도한다.
        let edge_now =
            current_ws_id == Some(anchor) && is_reactivation_edge(current_ws_id, prev_active);
        if first_attach_waits(
            self.remote.attach_retry.get(&anchor),
            &mapping,
            Instant::now(),
            edge_now,
        ) {
            return None;
        }
        let remote_ws = mapping.remote_workspace?;

        Some((anchor, mapping, remote_ws))
    }

    fn maybe_trigger_auto_attach(&mut self, current_ws_id: Option<u32>, prev_active: Option<u32>) {
        let Some((anchor, mapping, remote_ws)) =
            self.auto_attach_candidate(current_ws_id, prev_active)
        else {
            return;
        };

        let attempt = match self
            .remote
            .begin_attempt(Some(anchor), Some(mapping.clone()))
        {
            Ok(attempt) => attempt,
            Err(error) => {
                tracing::warn!("{error}");
                return;
            }
        };
        let endpoint_target = match self.mirror_install_target(None, Some(anchor), None, false) {
            Ok(target) => target,
            Err(error) => {
                self.remote.finish_attempt(&attempt);
                tracing::warn!("remote target unavailable: {error}");
                return;
            }
        };
        self.state
            .mirror_attempts
            .register_endpoint(attempt.clone(), endpoint_target);
        self.remote.active.insert(anchor);
        self.remote.pending_reactivation.remove(&anchor);
        // 배너는 사용자 조작의 안내다. 매핑만 바뀌어 바로 시작한 시도의 안내는 행 표지만 보인다.
        let from_activation =
            current_ws_id == Some(anchor) && is_reactivation_edge(current_ws_id, prev_active);
        self.remote.refusals.begin_attempt(anchor, from_activation);
        let spawned =
            self.spawn_endpoint_attempt(attempt, anchor, remote_ws, mapping.target.clone(), false);
        if let Err(error) = spawned {
            self.reject_endpoint_start(anchor, error, false);
        }
    }

    /// 시도 중이 아닌 anchor를 예약 시각 또는 해당 워크스페이스 재활성화 때 재연결한다.
    fn maybe_trigger_reconnect(&mut self, current_ws_id: Option<u32>, prev_active: Option<u32>) {
        let anchors: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .filter(|s| s.state() == SessionState::Reconnecting)
            .filter_map(|s| s.anchor_ws_id())
            .collect();
        for anchor in anchors {
            self.trigger_reconnect_anchor(anchor, current_ws_id, prev_active);
        }
    }

    fn trigger_reconnect_anchor(
        &mut self,
        anchor: u32,
        current_ws_id: Option<u32>,
        prev_active: Option<u32>,
    ) {
        if self.remote.active.contains(&anchor) {
            return;
        }
        let edge_now =
            current_ws_id == Some(anchor) && is_reactivation_edge(current_ws_id, prev_active);
        let existing_slot = self.remote.reconnect.get(&anchor);
        let due = reconnect_due(existing_slot, Instant::now());
        if !edge_now && !due {
            return;
        }
        if should_reset_given_up(existing_slot, edge_now) {
            self.remote.reconnect.remove(&anchor);
        }
        // 대기 중 워크스페이스나 매핑이 사라졌을 수 있으므로 다시 읽는다.
        let mapping = self.engines().windows().find_map(|(_, e)| {
            e.workspaces()
                .into_iter()
                .find(|ws| ws.id == anchor)
                .and_then(|ws| ws.attach_mapping.clone())
        });
        let Some(mapping) = mapping else {
            forget_anchor_backoff(&mut self.remote, anchor);
            self.remote.pending_reactivation.remove(&anchor);
            return;
        };
        let Some(remote_ws) = mapping.remote_workspace else {
            return; // 원격 workspace id 미지정 — 자동 attach 와 같이 대상을 추측하지 않는다.
        };

        let attempt = match self
            .remote
            .begin_attempt(Some(anchor), Some(mapping.clone()))
        {
            Ok(attempt) => attempt,
            Err(error) => {
                tracing::warn!("{error}");
                return;
            }
        };
        let index = self
            .remote
            .sessions
            .iter()
            .position(|session| session.state.anchor_ws_id == Some(anchor));
        let endpoint_target = match self.mirror_install_target(None, Some(anchor), index, false) {
            Ok(target) => target,
            Err(error) => {
                self.remote.finish_attempt(&attempt);
                tracing::warn!("remote reconnect target unavailable: {error}");
                return;
            }
        };
        self.state
            .mirror_attempts
            .register_endpoint(attempt.clone(), endpoint_target);
        self.remote.active.insert(anchor);
        self.remote.pending_reactivation.remove(&anchor);
        let spawned =
            self.spawn_endpoint_attempt(attempt, anchor, remote_ws, mapping.target.clone(), true);
        if let Err(error) = spawned {
            self.reject_endpoint_start(anchor, error, true);
        }
    }

    fn reject_endpoint_start(&mut self, anchor: u32, error: String, reconnect: bool) {
        self.state.mirror_attempts.discard_inactive_endpoints();
        self.remote.active.remove(&anchor);
        let stage = if reconnect { "reconnect" } else { "endpoint" };
        tracing::warn!("remote {stage} start rejected: {error}");
    }

    // Both entry paths have already fixed the original target and registered the attempt.
    fn spawn_endpoint_attempt(
        &mut self,
        attempt: tasty_remote::outbound::AttemptToken,
        anchor: u32,
        remote_ws: u32,
        target: WorkspaceAttachTarget,
        is_reconnect: bool,
    ) -> Result<(), String> {
        let tx = self.remote.tx.clone();
        let proxy = self.view.proxy.clone();
        self.remote.spawn_attempt(attempt.clone(), move || {
            let result = resolve_endpoint_bound(&target, &attempt);
            let outcome = AutoAttachOutcome {
                attempt,
                anchor_ws_id: Some(anchor),
                remote_ws,
                result,
                is_reconnect,
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = tx.send(outcome); // 수신자가 종료되면 결과를 전달할 수 없다.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = proxy.send_event(crate::app::event::AppEvent::AutoAttachReady); // 이벤트 루프 종료 시 깨움 실패를 무시한다.
        })
    }

    pub(crate) fn drain_auto_attach_results(&mut self) {
        self.remote.reap_attempts();
        let stale: Vec<_> = self
            .state
            .mirror_attempts
            .endpoint_snapshot()
            .into_iter()
            .filter(|(_, target)| !self.mirror_install_target_is_current(target))
            .map(|(token, target)| (token, target.anchor))
            .collect();
        for (token, anchor) in stale {
            self.state
                .mirror_attempts
                .cancel_endpoint(&token, &mut self.remote);
            if let Some(anchor) = anchor {
                self.remote.active.remove(&anchor);
            }
        }
        self.state.mirror_attempts.discard_inactive_endpoints();
        while let Ok(outcome) = self.remote.rx.try_recv() {
            self.apply_auto_attach_outcome(outcome);
        }
    }

    fn take_current_endpoint_target(
        &mut self,
        outcome: &AutoAttachOutcome,
    ) -> Option<crate::app::attach_client::pending::PendingMirrorInstall> {
        let Some(target) = self.state.mirror_attempts.take_endpoint(&outcome.attempt) else {
            self.remote.finish_attempt(&outcome.attempt);
            return None;
        };
        if !self.mirror_install_target_is_current(&target) {
            target.cancel_run(crate::app::attach_client::pending::TARGET_DISCARDED);
            if let Some(retired) = self.remote.finish_attempt(&outcome.attempt)
                && let Some(anchor) = retired.anchor
            {
                self.remote.active.remove(&anchor);
            }
            return None;
        }
        Some(target)
    }

    fn accepted_mapping_is_current(
        &mut self,
        accepted: &tasty_remote::outbound::AttemptRecord,
    ) -> bool {
        if let Some(anchor) = accepted.anchor {
            let current = self.engines.all_sessions().find_map(|session| {
                session
                    .core_state
                    .workspaces()
                    .iter()
                    .find(|workspace| workspace.id == anchor)
                    .and_then(|workspace| workspace.attach_mapping.clone())
            });
            if current != accepted.mapping {
                self.remote.active.remove(&anchor);
                tracing::debug!("discarding SSH result after attach mapping changed");
                return false;
            }
        }
        true
    }

    fn accept_auto_attach_outcome(
        &mut self,
        outcome: &AutoAttachOutcome,
    ) -> Option<(
        crate::app::attach_client::pending::PendingMirrorInstall,
        tasty_remote::outbound::AttemptRecord,
    )> {
        let target = self.take_current_endpoint_target(outcome)?;
        let Some(accepted) = self.remote.finish_attempt(&outcome.attempt) else {
            tracing::debug!("discarding result from retired remote connection attempt");
            target.cancel_run("the attempt was replaced or cancelled");
            return None;
        };
        if accepted.anchor != outcome.anchor_ws_id {
            tracing::error!("remote outcome belongs to another anchor");
            target.fail_run("the attach result belongs to another workspace");
            return None;
        }
        if !self.accepted_mapping_is_current(&accepted) {
            target.cancel_run("the attach mapping changed");
            return None;
        }
        Some((target, accepted))
    }

    fn apply_auto_attach_outcome(&mut self, outcome: AutoAttachOutcome) {
        // Keep the accepted attempt owner through result dispatch, as before extraction.
        let Some((target, accepted)) = self.accept_auto_attach_outcome(&outcome) else {
            self.remote.discard_endpoint_outcome(outcome);
            return;
        };
        let AutoAttachOutcome {
            attempt: _,
            anchor_ws_id,
            remote_ws,
            result,
            is_reconnect,
        } = outcome;
        match result {
            Ok((tunnel, port)) => {
                self.handle_auto_attach_connected(
                    target,
                    &accepted,
                    remote_ws,
                    is_reconnect,
                    tunnel,
                    port,
                );
            }
            Err(e) => {
                target.fail_run(format!("the attach endpoint could not be resolved: {e:#}"));
                if let Some(this) = e.downcast_ref::<tasty_remote::self_instance::ThisInstance>() {
                    tracing::warn!(
                        "SSH attach 대상이 이 인스턴스 자신이라 거절했다 (anchor ws {anchor_ws_id:?}, remote ws {remote_ws}, reconnect={is_reconnect})"
                    );
                    self.record_self_refusal(&accepted, remote_ws, is_reconnect, this.port, true);
                    return;
                }
                tracing::warn!(
                    "attach 엔드포인트 해석 실패 (anchor ws {anchor_ws_id:?}, remote ws {remote_ws}, reconnect={is_reconnect}): {e}"
                );
                if let Some(anchor) = anchor_ws_id {
                    self.remote.active.remove(&anchor);
                    if is_reconnect {
                        self.on_reconnect_attempt_failed(anchor, &e);
                    } else {
                        if let Some(mapping) = accepted.mapping.as_ref() {
                            self.remote.refusals.note(
                                anchor,
                                notice::resolve_failure_kind(&e),
                                mapping,
                            );
                        }
                        self.on_first_attach_failed(anchor, accepted.mapping.as_ref());
                    }
                }
            }
        }
    }

    fn handle_auto_attach_connected(
        &mut self,
        target: crate::app::attach_client::pending::PendingMirrorInstall,
        accepted: &tasty_remote::outbound::AttemptRecord,
        remote_ws: u32,
        is_reconnect: bool,
        tunnel: Option<SshTunnel>,
        port: u16,
    ) {
        // accept_auto_attach_outcome가 결과의 anchor와 같음을 확인했다.
        let anchor_ws_id = accepted.anchor;
        let run = target.clone();
        // 자기 포트는 debug·release 모두 연결을 시도하기 전에 거절한다.
        let Some(attach_result) = self.queue_endpoint_mirror(target, port, remote_ws, tunnel)
        else {
            run.fail_run(format!("attach target port {port} is this instance"));
            self.record_self_refusal(accepted, remote_ws, is_reconnect, port, false);
            return;
        };
        match attach_result {
            // 접수는 성공 설치가 아니다. 실패 이력은 pending 설치 성공 때만 해제한다.
            Ok(()) => {}
            Err(e) => {
                run.fail_run(format!("the mirror connection could not start: {e}"));
                tracing::warn!(
                    "attach mirror 실패 (anchor ws {anchor_ws_id:?}, remote ws {remote_ws}, reconnect={is_reconnect}): {e}"
                );
                if let Some(anchor) = anchor_ws_id {
                    self.remote.active.remove(&anchor);
                    if is_reconnect {
                        self.on_reconnect_attempt_failed(anchor, &e);
                    } else {
                        self.on_first_attach_failed(anchor, accepted.mapping.as_ref());
                    }
                }
            }
        }
    }

    /// 거절을 IPC로 조회할 수 있게 남기고, 같은 대상을 매 프레임 다시 해석하지 않게 한다.
    /// 첫 attach는 재활성화 때까지 기다리고, 인라인 매핑은 매핑이 바뀔 때까지 보류한다.
    /// 재연결은 다른 재연결 실패와 같은 백오프를 따른다.
    fn record_self_refusal(
        &mut self,
        accepted: &tasty_remote::outbound::AttemptRecord,
        remote_ws: u32,
        is_reconnect: bool,
        port: u16,
        via_ssh: bool,
    ) {
        self.remote.refusals.record(
            tasty_remote::refusal::AttachRefusal {
                attempt: accepted.token.id(),
                anchor_workspace: accepted.anchor,
                remote_workspace: remote_ws,
                port,
                via_ssh,
                reconnect: is_reconnect,
            },
            accepted.mapping.as_ref(),
        );
        let Some(anchor) = accepted.anchor else {
            return;
        };
        if let Some(mapping) = accepted.mapping.as_ref() {
            self.remote.refusals.note(
                anchor,
                tasty_remote::refusal::MappingNoticeKind::SelfInstance,
                mapping,
            );
        }
        self.remote.active.remove(&anchor);
        if is_reconnect {
            self.on_reconnect_attempt_failed(
                anchor,
                &anyhow::anyhow!("attach target is this instance's own port {port}"),
            );
        } else {
            self.remote.pending_reactivation.insert(anchor);
        }
    }

    fn on_first_attach_failed(
        &mut self,
        anchor: u32,
        mapping: Option<&crate::model::WorkspaceAttachMapping>,
    ) {
        back_off_first_attach(&mut self.remote, anchor, mapping);
    }

    /// already_attached는 긴 고정 간격, 나머지 실패는 지수 백오프로 재시도한다.
    /// jitter로 동시 재시도 집중을 줄이며 상한에 도달하면 자동 재시도만 멈춘다.
    pub(super) fn on_reconnect_attempt_failed(&mut self, anchor: u32, err: &anyhow::Error) {
        let permanent_conflict = err.to_string().contains("already_attached");
        // mirror를 다시 mirror하는 대상은 다시 시도해도 같은 거절이다.
        let chain = tasty_remote::transport::attach_rejection_reason(&err.to_string())
            == Some(tasty_remote::transport::REJECTED_MIRROR_WORKSPACE);
        let slot = self.remote.reconnect.entry(anchor).or_default();
        slot.attempts += 1;
        if chain || reconnect_exhausted(slot.attempts) {
            slot.given_up = true;
            self.notify_reconnect_giveup(anchor);
            return;
        }
        if permanent_conflict {
            slot.backoff.reset();
            while slot.backoff.current() < std::time::Duration::from_secs(30) {
                slot.backoff.advance();
            }
        }
        let base = slot.backoff.current();
        slot.backoff.advance();
        use rand::Rng;
        let jitter = 0.8 + rand::rng().random::<f64>() * 0.4;
        slot.next_attempt = Instant::now() + base.mul_f64(jitter);
    }

    /// 자동 재시도 중단을 알린다. 워크스페이스 재활성화로 다시 시도할 상태는 유지한다.
    fn notify_reconnect_giveup(&mut self, anchor: u32) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if engine.workspaces().into_iter().any(|ws| ws.id == anchor) {
                main.state.toasts.push(
                    crate::i18n::t("attach.toast.mirror_reconnect_giveup").to_string(),
                    crate::adapters::ui::ToastKind::Warning,
                    crate::adapters::ui::ToastScope::Window,
                );
                main.mark_dirty();
                break;
            }
        }
        tracing::warn!(
            "attach 재연결 포기 (anchor ws {anchor}) — {MAX_RECONNECT_ATTEMPTS}회 시도 후 자동 재시도 중단"
        );
    }
}

/// 워커 스레드에서 프로필과 SSH 터널 또는 loopback 포트를 준비한다.
fn resolve_endpoint_bound(
    target: &WorkspaceAttachTarget,
    attempt: &tasty_remote::outbound::AttemptToken,
) -> anyhow::Result<(Option<SshTunnel>, u16)> {
    resolve_endpoint_bound_with(attempt, || resolve_endpoint(target))
}

/// 해석 함수를 받아 취소 등록·자기 판정을 붙인다. 시험은 SSH 없이 해석 결과를 넣는다.
fn resolve_endpoint_bound_with<T>(
    attempt: &tasty_remote::outbound::AttemptToken,
    resolve: impl FnOnce() -> anyhow::Result<(Option<T>, u16)>,
) -> anyhow::Result<(Option<T>, u16)> {
    let cancel = tasty_ssh::SshCancel::new();
    attempt
        .register_ssh(cancel.clone())
        .map_err(anyhow::Error::msg)?;
    let _scope = cancel.scope();
    let result = resolve()?;
    if !attempt.is_active() {
        anyhow::bail!("remote endpoint attempt cancelled");
    }
    tasty_remote::self_instance::refuse_this_instance(result.0.is_some(), result.1, Some(attempt))?;
    Ok(result)
}

fn resolve_endpoint(target: &WorkspaceAttachTarget) -> anyhow::Result<(Option<SshTunnel>, u16)> {
    let (ssh_target, remote_tasty, port_mode, port_file) = match target {
        WorkspaceAttachTarget::Profile { name } => {
            let profiles = RemoteProfiles::load();
            let passkeys = Passkeys::load();
            let p = profiles
                .get(name)
                .ok_or_else(|| notice::ProfileNotFound(name.clone()))?;
            ssh::resolve_attach_target(p, &profiles, &passkeys)?
        }
        WorkspaceAttachTarget::Inline {
            host,
            remote_tasty,
            port_mode,
            port_file,
        } => (
            SshTarget::parse(host),
            remote_tasty.clone().unwrap_or_else(|| "tasty".into()),
            port_mode.clone().unwrap_or_else(|| "auto".into()),
            port_file.clone(),
        ),
    };

    if let Some(port) = parse_loopback_port(&ssh_target.destination) {
        return Ok((None, port));
    }

    let ssh = ssh::resolve_ssh_path();
    let mode = PortMode::parse(&port_mode)?;
    // TASTY_SSH_VERIFY가 설정된 검증 환경에서는 accept-new를 사용한다. 없으면 host key 옵션을
    // 넘기지 않고 사용자 ssh 설정을 따른다.
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);
    let remote_port = ssh::discover_remote_port(
        &ssh,
        &ssh_target,
        &remote_tasty,
        mode,
        verify,
        debug,
        port_file.as_deref(),
    )?;
    let tunnel = SshTunnel::establish(&ssh, &ssh_target, remote_port, verify)?;
    let local_port = tunnel.local_port;
    Ok((Some(tunnel), local_port))
}

/// 첫 자동 attach가 실패하면 같은 매핑의 다음 시도를 재연결과 같은 지수 백오프(0.5초에서
/// 두 배씩, 상한 30초, jitter 0.8~1.2)로 미룬다. 자동 재시도 상한은 두지 않는다.
/// 해석 실패·접수 실패와 접수 뒤 연결 실패가 함께 쓴다. 매핑이 없으면 기록하지 않는다.
pub(crate) fn back_off_first_attach(
    remote: &mut tasty_remote::outbound::Remote,
    anchor: u32,
    mapping: Option<&crate::model::WorkspaceAttachMapping>,
) {
    let Some(mapping) = mapping else {
        return;
    };
    use rand::Rng;
    let jitter = 0.8 + rand::rng().random::<f64>() * 0.4;
    remote.record_first_attach_failure(anchor, mapping, Instant::now(), jitter);
}

/// anchor의 재연결·첫 attach 실패 기록을 함께 지운다. mirror 설치 성공, 매핑 소멸, mirror 닫기가 쓴다.
pub(crate) fn forget_anchor_backoff(remote: &mut tasty_remote::outbound::Remote, anchor: u32) {
    remote.reconnect.remove(&anchor);
    remote.attach_retry.remove(&anchor);
}

/// 기록한 매핑이 지금도 그 anchor의 매핑인 첫 attach 재시도 기록만 남긴다.
fn retain_live_first_attach_retries<'a>(
    remote: &mut tasty_remote::outbound::Remote,
    current: impl Fn(u32) -> Option<&'a crate::model::WorkspaceAttachMapping>,
) {
    remote
        .attach_retry
        .retain(|anchor, retry| current(*anchor) == Some(&retry.mapping));
}

/// 서버에서 닫은 기록은 그 anchor의 매핑이 그대로일 때만 남긴다. 매핑을 바꾸거나 지우면 다시 후보가 된다.
fn retain_live_closed_mappings<'a>(
    remote: &mut tasty_remote::outbound::Remote,
    current: impl Fn(u32) -> Option<&'a crate::model::WorkspaceAttachMapping>,
) {
    remote
        .closed_mappings
        .retain(|anchor, mapping| current(*anchor) == Some(&*mapping));
}

/// 연결 상태 쪽에서 이 anchor의 자동 attach를 막는 이유가 없는지 판정한다.
fn anchor_may_attach(
    remote: &tasty_remote::outbound::Remote,
    anchor: u32,
    mapping: &crate::model::WorkspaceAttachMapping,
    current_ws_id: Option<u32>,
    prev_active: Option<u32>,
) -> bool {
    // 재연결이 맡은 anchor에 새 mirror를 중복 생성하지 않는다.
    if remote
        .sessions
        .iter()
        .any(|s| s.anchor_ws_id() == Some(anchor) && s.state() == SessionState::Reconnecting)
    {
        return false;
    }
    let pending_reactivation = remote.pending_reactivation.contains(&anchor);
    is_attach_trigger_allowed(pending_reactivation, current_ws_id, prev_active)
        && !remote.active.contains(&anchor)
        && !remote.refusals.holds(anchor, mapping)
        // 사용자가 서버에서 닫은 workspace를 다시 열려고 하지 않는다.
        && remote.closed_mappings.get(&anchor) != Some(mapping)
}

/// 첫 자동 attach를 아직 미뤄야 하는지. 재활성화 직후에는 기다리지 않는다.
fn first_attach_waits(
    retry: Option<&tasty_remote::outbound::AttachRetry>,
    mapping: &crate::model::WorkspaceAttachMapping,
    now: Instant,
    edge_now: bool,
) -> bool {
    !edge_now && retry.is_some_and(|retry| retry.holds(mapping, now))
}

/// 아직 오지 않은 재시도 시각만 예약한다. 지난 시각을 예약하면 다른 워크스페이스에 있는 동안
/// 루프를 계속 깨운다.
fn first_attach_wakeup_at(
    retry: &tasty_remote::outbound::AttachRetry,
    now: Instant,
) -> Option<Instant> {
    (retry.next_attempt > now).then_some(retry.next_attempt)
}

fn is_reactivation_edge(current: Option<u32>, previous: Option<u32>) -> bool {
    current.is_some() && current != previous
}

fn is_attach_trigger_allowed(
    pending_reactivation: bool,
    current: Option<u32>,
    previous: Option<u32>,
) -> bool {
    !pending_reactivation || is_reactivation_edge(current, previous)
}

/// `127.0.0.1:PORT` / `localhost:PORT` / `[::1]:PORT` 면 PORT 를 돌려준다(loopback 직결).
/// 그 외(원격 호스트/alias)는 None → SSH 터널 경로.
fn parse_loopback_port(dest: &str) -> Option<u16> {
    let (host, port_str) = if let Some(rest) = dest.strip_prefix("[::1]:") {
        ("::1", rest)
    } else {
        let (h, p) = dest.rsplit_once(':')?;
        (h, p)
    };
    let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
    if !is_loopback {
        return None;
    }
    port_str.parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_up_slot_schedules_no_wakeup_but_survives() {
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = Instant::now() + std::time::Duration::from_secs(5);
        slot.given_up = true;
        assert_eq!(reconnect_wakeup_at(Some(&slot)), None);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn active_backoff_schedules_its_next_attempt() {
        let at = Instant::now() + std::time::Duration::from_secs(5);
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = at;
        assert_eq!(reconnect_wakeup_at(Some(&slot)), Some(at));
    }

    #[test]
    fn absent_slot_schedules_no_wakeup_because_it_is_already_due() {
        assert_eq!(reconnect_wakeup_at(None), None);
        assert!(reconnect_due(None, Instant::now()));
    }

    #[test]
    fn reactivation_edge_only_on_transition() {
        assert!(is_reactivation_edge(Some(1), None));
        assert!(!is_reactivation_edge(Some(1), Some(1)));
        assert!(is_reactivation_edge(Some(2), Some(1)));
        assert!(!is_reactivation_edge(None, Some(1)));
    }

    #[test]
    fn new_mapping_triggers_immediately_without_transition() {
        assert!(is_attach_trigger_allowed(false, Some(1), Some(1)));
        assert!(is_attach_trigger_allowed(false, Some(2), Some(1)));
    }

    #[test]
    fn disconnected_anchor_waits_for_transition_before_retrigger() {
        assert!(!is_attach_trigger_allowed(true, Some(1), Some(1)));
        assert!(is_attach_trigger_allowed(true, Some(1), Some(2)));
    }

    fn mapping(remote_workspace: u32) -> crate::model::WorkspaceAttachMapping {
        crate::model::WorkspaceAttachMapping::profile("srv", Some(remote_workspace))
    }

    /// 서버에서 닫은 매핑은 같은 anchor로 돌아와도 다시 attach하지 않는다. 매핑을 바꾸면 다시 후보가 되고,
    /// 매핑이 바뀌거나 사라지면 기록도 지운다.
    #[test]
    fn a_mapping_closed_on_the_server_is_not_attached_again() {
        let mut remote = tasty_remote::outbound::Remote::new();
        remote.closed_mappings.insert(5, mapping(514));
        assert!(!anchor_may_attach(
            &remote,
            5,
            &mapping(514),
            Some(5),
            Some(1)
        ));
        assert!(anchor_may_attach(
            &remote,
            5,
            &mapping(515),
            Some(5),
            Some(1)
        ));

        let same = mapping(514);
        retain_live_closed_mappings(&mut remote, |_| Some(&same));
        assert!(remote.closed_mappings.contains_key(&5));
        let changed = mapping(515);
        retain_live_closed_mappings(&mut remote, |_| Some(&changed));
        assert!(remote.closed_mappings.is_empty());
    }

    /// 연결을 끊은 뒤 정리로 선택이 anchor에 머물러도 그 프레임은 전환이 아니다. 다른 workspace를 거쳐
    /// 돌아올 때만 다시 attach한다.
    #[test]
    fn a_disconnected_anchor_waits_for_the_users_next_switch() {
        let mut remote = tasty_remote::outbound::Remote::new();
        remote.pending_reactivation.insert(5);
        // 정리 뒤 last_active_ws를 현재 활성(anchor)으로 맞춘 상태.
        assert!(!anchor_may_attach(
            &remote,
            5,
            &mapping(514),
            Some(5),
            Some(5)
        ));
        assert!(anchor_may_attach(
            &remote,
            5,
            &mapping(514),
            Some(5),
            Some(1)
        ));
    }

    #[test]
    fn loopback_ports_parsed() {
        assert_eq!(parse_loopback_port("127.0.0.1:45123"), Some(45123));
        assert_eq!(parse_loopback_port("localhost:8080"), Some(8080));
        assert_eq!(parse_loopback_port("[::1]:9000"), Some(9000));
    }

    #[test]
    fn non_loopback_is_none() {
        assert_eq!(parse_loopback_port("gx10"), None);
        assert_eq!(parse_loopback_port("user@host"), None);
        assert_eq!(parse_loopback_port("192.168.0.10:45123"), None);
        assert_eq!(parse_loopback_port("example.com:22"), None);
    }

    #[test]
    fn reconnect_due_when_no_slot_yet() {
        assert!(reconnect_due(None, Instant::now()));
    }

    #[test]
    fn reconnect_due_respects_backoff_next_attempt() {
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = Instant::now() + std::time::Duration::from_secs(10);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
        slot.next_attempt = Instant::now() - std::time::Duration::from_secs(1);
        assert!(reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn reconnect_due_is_false_when_given_up_even_past_next_attempt() {
        let mut slot = ReconnectSlot::new();
        slot.given_up = true;
        slot.next_attempt = Instant::now() - std::time::Duration::from_secs(1);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn reconnect_exhausted_at_max_not_before() {
        assert!(!reconnect_exhausted(MAX_RECONNECT_ATTEMPTS - 1));
        assert!(reconnect_exhausted(MAX_RECONNECT_ATTEMPTS));
        assert!(reconnect_exhausted(MAX_RECONNECT_ATTEMPTS + 1));
    }

    #[test]
    fn given_up_slot_resets_only_on_edge_reactivation() {
        let mut slot = ReconnectSlot::new();
        slot.given_up = true;
        assert!(!should_reset_given_up(Some(&slot), false));
        assert!(should_reset_given_up(Some(&slot), true));
        slot.given_up = false;
        assert!(!should_reset_given_up(Some(&slot), true));
        assert!(!should_reset_given_up(None, true));
    }

    fn profile_mapping(name: &str) -> crate::model::WorkspaceAttachMapping {
        crate::model::WorkspaceAttachMapping {
            target: WorkspaceAttachTarget::Profile { name: name.into() },
            remote_workspace: Some(1),
        }
    }

    #[test]
    fn first_attach_failures_space_out_until_the_cap() {
        let t0 = Instant::now();
        let mut retry = tasty_remote::outbound::AttachRetry::new(profile_mapping("p"));
        let mut gaps = Vec::new();
        for _ in 0..9 {
            retry.record_failure(t0, 1.0);
            gaps.push((retry.next_attempt - t0).as_millis());
        }
        assert_eq!(
            gaps,
            [500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]
        );
    }

    #[test]
    fn a_failed_mapping_waits_unless_the_user_returns_or_it_changes() {
        let t0 = Instant::now();
        let mapping = profile_mapping("p");
        let mut retry = tasty_remote::outbound::AttachRetry::new(mapping.clone());
        retry.record_failure(t0, 1.0);
        assert!(first_attach_waits(Some(&retry), &mapping, t0, false));
        assert!(!first_attach_waits(Some(&retry), &mapping, t0, true));
        assert!(!first_attach_waits(
            Some(&retry),
            &profile_mapping("other"),
            t0,
            false
        ));
        let later = t0 + std::time::Duration::from_millis(500);
        assert!(!first_attach_waits(Some(&retry), &mapping, later, false));
        assert!(!first_attach_waits(None, &mapping, t0, false));
    }

    #[test]
    fn automatic_attach_asks_a_tunnel_peer_and_refuses_this_instance() {
        let (port, server) = fake_peer::serve_once(
            serde_json::json!({ "instance_id": tasty_ipc::instance::instance_id() }),
        );
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        let error = resolve_endpoint_bound_with(&attempt, || Ok((Some(()), port)))
            .expect_err("this instance");
        assert!(server.join().expect("server").contains("\"system.info\""));
        assert!(
            error
                .downcast_ref::<tasty_remote::self_instance::ThisInstance>()
                .is_some(),
            "{error}"
        );
    }

    /// 프로필 파일 해석이 실제로 돌려주는 오류가 거절 안내의 `ProfileMissing` 으로 분류되는지 본다.
    /// 오류를 감싸는 단계가 생기면 안내가 `Unresolved` 로 바뀌어 배너의 이유가 달라진다.
    #[test]
    fn a_missing_profile_mapping_resolves_to_the_profile_missing_notice() {
        let name = format!(
            "no-such-profile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        let Err(error) = resolve_endpoint_bound(
            &WorkspaceAttachTarget::Profile { name: name.clone() },
            &attempt,
        ) else {
            panic!("a profile that does not exist cannot resolve");
        };
        assert_eq!(
            notice::resolve_failure_kind(&error),
            tasty_remote::refusal::MappingNoticeKind::ProfileMissing,
            "{error}"
        );
        assert!(error.to_string().contains(&name), "{error}");
    }

    #[test]
    fn automatic_attach_does_not_ask_a_direct_endpoint() {
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        // 아무도 듣지 않는 포트라도 터널이 아니면 묻지 않는다.
        let port = fake_peer::free_port();
        let resolved = resolve_endpoint_bound_with(&attempt, || Ok((None::<()>, port)));
        assert_eq!(resolved.expect("direct endpoint").1, port);
    }

    #[test]
    fn forgetting_an_anchor_clears_both_retry_records() {
        let mut remote = tasty_remote::outbound::Remote::new();
        let mapping = profile_mapping("a");
        remote.reconnect.insert(3, ReconnectSlot::new());
        back_off_first_attach(&mut remote, 3, Some(&mapping));
        back_off_first_attach(&mut remote, 4, Some(&mapping));
        forget_anchor_backoff(&mut remote, 3);
        assert!(!remote.reconnect.contains_key(&3));
        assert!(!remote.attach_retry.contains_key(&3));
        assert!(remote.attach_retry.contains_key(&4), "other anchors stay");
    }

    #[test]
    fn a_closed_or_remapped_workspace_drops_its_first_attach_record() {
        let mut remote = tasty_remote::outbound::Remote::new();
        let (kept, changed) = (profile_mapping("a"), profile_mapping("b"));
        for anchor in [1, 2, 3] {
            back_off_first_attach(&mut remote, anchor, Some(&kept));
        }
        // 1은 그대로, 2는 다른 매핑, 3은 워크스페이스나 매핑이 없다.
        retain_live_first_attach_retries(&mut remote, |anchor| match anchor {
            1 => Some(&kept),
            2 => Some(&changed),
            _ => None,
        });
        let mut left: Vec<u32> = remote.attach_retry.keys().copied().collect();
        left.sort();
        assert_eq!(left, vec![1]);
    }

    #[test]
    fn a_past_retry_time_arms_no_timer() {
        let t0 = Instant::now();
        let mut retry = tasty_remote::outbound::AttachRetry::new(profile_mapping("p"));
        retry.record_failure(t0, 1.0);
        assert_eq!(first_attach_wakeup_at(&retry, t0), Some(retry.next_attempt));
        assert_eq!(first_attach_wakeup_at(&retry, retry.next_attempt), None);
    }
}

/// SSH 경로의 자기 판정 배선을 시험하는 가짜 `system.info` 상대.
#[cfg(test)]
pub(crate) mod fake_peer {
    use std::io::{BufRead, BufReader, Write};

    /// 요청 한 줄을 읽고 주어진 결과 한 줄을 돌려준다. 받은 요청 줄을 돌려준다.
    pub(crate) fn serve_once(result: serde_json::Value) -> (u16, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).expect("read");
            let reply = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result });
            writeln!(&stream, "{reply}").expect("write");
            line
        });
        (port, handle)
    }

    /// 바인드했다 놓은 포트. 묻지 않는 경로가 연결하면 거절돼 드러난다.
    pub(crate) fn free_port() -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .expect("free port")
            .port()
    }
}
