//! Application values identify the sole target; Remote retains pending connection resources.

use super::projection::mirror_id_needs;
use super::wire::mirror_event_from_control;
use crate::app::App;
use crate::runtime::engine_session::EngineId;
use crate::view::ui::View as _;
use std::sync::Arc;
use tasty_remote::pending_connection::{ConnectionTarget, ConnectionTicket};

#[derive(Clone)]
pub(crate) struct PendingMirrorInstall {
    pub engine: EngineId,
    pub window: Option<winit::window::WindowId>,
    pub view: Option<std::sync::Weak<()>>,
    pub selection: Option<std::sync::Weak<()>>,
    pub activate: bool,
    pub anchor: Option<u32>,
    pub mapping: Option<crate::model::WorkspaceAttachMapping>,
    pub mapping_token: Option<std::sync::Weak<()>>,
    pub reconnect: Option<(u32, tasty_remote::connection::ConnectionEpoch)>,
    pub resync: bool,
    /// 최종 결과를 받을 요청의 run. 결과 없이 모든 사본이 사라지면 `cancelled`로 끝난다.
    pub run: Option<crate::request_runs::RunGuard>,
    /// 사용자가 원격 연결 팝업에서 시작했다. 실패하면 시작한 창에 토스트를 띄운다.
    pub user: bool,
    /// anchor 없는 수동 요청(`remote.attach`·`attach.into_gui`)의 대상. 같은 대상의 새 요청은
    /// 진행 중인 이전 요청을 대체한다.
    pub manual: Option<String>,
    /// 연결할 때 고른 attach 프로필 이름. 설치할 때 세션에 옮겨 원격 호스트 표시의 글자가 된다.
    pub profile: Option<String>,
    /// 붙을 원격 workspace id. 연결을 큐에 넣을 때 정해진다.
    pub remote_workspace: Option<u32>,
}

impl PendingMirrorInstall {
    /// 이 시도를 기다리는 요청에 최종 결과를 남긴다. 처음 기록한 결과만 남는다.
    pub(crate) fn finish_run(
        &self,
        state: crate::request_runs::RunState,
        result: impl Into<String>,
        extra: serde_json::Value,
    ) {
        if let Some(run) = &self.run {
            crate::request_runs::finish(run.id(), state, result, extra);
        }
    }

    pub(crate) fn fail_run(&self, reason: impl Into<String>) {
        self.finish_run(
            crate::request_runs::RunState::Failed,
            reason,
            serde_json::Value::Null,
        );
    }

    pub(crate) fn cancel_run(&self, reason: &str) {
        self.finish_run(
            crate::request_runs::RunState::Cancelled,
            reason,
            serde_json::Value::Null,
        );
    }

    /// 요청의 run을 붙인다. 정해진 결과 없이 사라지면 그 이유가 남는다.
    pub(crate) fn with_run(mut self, run: Option<u64>) -> Self {
        self.run = run.map(|run| {
            crate::request_runs::RunGuard::new(run, "the attach attempt ended without a result")
        });
        self
    }
}

/// 자동 attach 매핑이 가리키는 프로필 이름. 인라인 매핑은 프로필이 없다.
pub(crate) fn profile_of_mapping(mapping: &crate::model::WorkspaceAttachMapping) -> Option<String> {
    match &mapping.target {
        crate::model::WorkspaceAttachTarget::Profile { name } => Some(name.clone()),
        crate::model::WorkspaceAttachTarget::Inline { .. } => None,
    }
}

/// 시작한 창이나 engine이 사라져 시도를 버릴 때의 결과.
pub(crate) const TARGET_DISCARDED: &str = "the window or engine that started the attach is gone";
impl App {
    pub(crate) fn mirror_install_target(
        &self,
        explicit: Option<EngineId>,
        anchor: Option<u32>,
        reconnect: Option<usize>,
        activate: bool,
    ) -> anyhow::Result<PendingMirrorInstall> {
        let anchor = anchor.or_else(|| {
            reconnect
                .and_then(|index| self.remote.sessions.get(index))
                .and_then(|session| session.state.anchor_ws_id)
        });
        let resync = reconnect
            .and_then(|index| self.remote.sessions.get(index))
            .is_some_and(|session| session.state.resync_pending.is_some());
        let old = reconnect
            .and_then(|index| self.remote.sessions.get(index))
            .map(|session| {
                (
                    session.state.local_workspace,
                    session.transport.frame_tx.epoch(),
                )
            });
        let engine = explicit
            .or_else(|| {
                old.as_ref().and_then(|(workspace, _)| {
                    self.engines
                        .all_sessions()
                        .find(|engine| engine.core_state.has_workspace(*workspace))
                        .map(|engine| engine.id)
                })
            })
            .or_else(|| {
                anchor.and_then(|workspace| {
                    self.engines
                        .all_sessions()
                        .find(|engine| engine.core_state.has_workspace(workspace))
                        .map(|engine| engine.id)
                })
            })
            .or_else(|| {
                self.focused_window()
                    .and_then(|main| self.engines.of_window(main.base.winit.id()))
            })
            .ok_or_else(|| anyhow::anyhow!("no engine is available to receive a mirror"))?;
        let window = self.engines.window_of(engine);
        let main = window
            .and_then(|window| self.view.views.get(&window))
            .and_then(|view| view.as_main());
        if old.is_none() && main.is_none() {
            anyhow::bail!("mirror creation has no originating View");
        }
        let owner = self
            .engines
            .get(engine)
            .ok_or_else(|| anyhow::anyhow!("mirror target engine missing"))?;
        let mapping = anchor.and_then(|id| {
            owner
                .find_workspace_index_for_id(id)
                .and_then(|index| owner.workspace_at(index))
                .and_then(|workspace| workspace.attach_mapping.clone())
        });
        let mapping_token = anchor
            .and_then(|id| owner.remote.attach_mapping_tokens.get(&id))
            .map(Arc::downgrade);
        // 재연결은 처음 연결한 프로필을 그대로 쓴다. 자동 attach는 매핑의 프로필이다.
        // 수동 요청은 호출자가 자기 프로필로 바꾼다.
        let profile = match reconnect.and_then(|index| self.remote.sessions.get(index)) {
            Some(session) => session.state.profile.clone(),
            None => mapping.as_ref().and_then(profile_of_mapping),
        };
        Ok(PendingMirrorInstall {
            profile,
            remote_workspace: None,
            resync,
            mapping,
            mapping_token,
            engine,
            window,
            view: main.map(|main| main.base.state.identity()),
            selection: main.map(|main| main.state.navigation.generation()),
            activate,
            anchor,
            reconnect: old,
            run: None,
            user: false,
            manual: None,
        })
    }
    pub(crate) fn mirror_install_target_is_current(&self, target: &PendingMirrorInstall) -> bool {
        let Some(engine) = self.engines.get(target.engine) else {
            return false;
        };
        if let Some(anchor) = target.anchor {
            let current = engine
                .find_workspace_index_for_id(anchor)
                .and_then(|index| engine.workspace_at(index));
            if current.is_none_or(|workspace| workspace.attach_mapping != target.mapping)
                || target.mapping_token.as_ref().is_none_or(|token| {
                    engine
                        .remote
                        .attach_mapping_tokens
                        .get(&anchor)
                        .is_none_or(|current| !token.ptr_eq(&Arc::downgrade(current)))
                })
            {
                return false;
            }
        }
        if let Some((workspace, epoch)) = &target.reconnect {
            return engine.has_workspace(*workspace)
                && self.remote.sessions.iter().any(|session| {
                    session.state.local_workspace == *workspace
                        && session.transport.frame_tx.epoch().same(epoch)
                });
        }
        target.window.is_some_and(|window| {
            self.engines.of_window(window) == Some(target.engine)
                && self
                    .view
                    .views
                    .get(&window)
                    .and_then(|view| view.as_main())
                    .is_some_and(|main| {
                        target
                            .view
                            .as_ref()
                            .is_some_and(|identity| main.base.state.matches_identity(identity))
                    })
        })
    }
    pub(crate) fn queue_mirror_connection(
        &mut self,
        target: PendingMirrorInstall,
        port: u16,
        workspace: u32,
        tunnel: Option<tasty_ssh::SshTunnel>,
    ) -> anyhow::Result<()> {
        if !self.mirror_install_target_is_current(&target) {
            self.remote.retire_tunnel(tunnel);
            target.cancel_run(TARGET_DISCARDED);
            anyhow::bail!("mirror origin was retired before connection");
        }
        queue_on(
            &mut self.state.mirror_attempts,
            &mut self.remote,
            target,
            ConnectionTarget {
                port,
                workspace,
                anchor: None,
                mapping: None,
            },
            tunnel,
            self.attach_client_wake.waker(),
            self.services.attach_heartbeat.get(),
        )
    }
    pub(crate) fn poll_pending_mirror_installs(&mut self) {
        self.remote.collect_connections();
        let pending = self.state.mirror_attempts.connection_snapshot();
        for (ticket, target) in pending {
            if !self.mirror_install_target_is_current(&target) {
                target.cancel_run(TARGET_DISCARDED);
                self.state
                    .mirror_attempts
                    .cancel_connection(ticket, &mut self.remote);
                continue;
            }
            if let Some(error) = self.remote.connection_error(ticket) {
                self.fail_pending_mirror(ticket, &target, error);
                continue;
            }
            if target.reconnect.is_some() && self.engines.window_of(target.engine).is_none() {
                continue;
            }
            let Some(prepared) = self.remote.prepared_connection(ticket) else {
                continue;
            };
            let Some(engine) = self.engines.get(target.engine) else {
                continue;
            };
            let admission = mirror_id_needs(
                &prepared.tree,
                prepared.surfaces.len(),
                target.reconnect.is_none(),
            )
            .and_then(|needed| {
                engine
                    .runtime
                    .ids
                    .ensure(&needed)
                    .map_err(anyhow::Error::new)
            });
            if let Err(error) = admission {
                if matches!(
                    error.downcast_ref::<crate::runtime::id_reservations::ReservationError>(),
                    Some(crate::runtime::id_reservations::ReservationError::Pending)
                ) {
                    (engine.runtime.waker)();
                    continue;
                }
                self.fail_pending_mirror(ticket, &target, error.to_string());
                continue;
            }
            let Some(prepared) = self.remote.take_connection(ticket) else {
                continue;
            };
            self.state.mirror_attempts.finish_connection(ticket);
            let installed = if let Some((workspace, epoch)) = &target.reconnect {
                let index = self.remote.sessions.iter().position(|session| {
                    session.state.local_workspace == *workspace
                        && session.transport.frame_tx.epoch().same(epoch)
                });
                index
                    .ok_or_else(|| anyhow::anyhow!("reconnect target retired"))
                    .and_then(|index| self.install_reconnected_mirror(index, &target, prepared))
                    .map(|_| *workspace)
            } else {
                self.install_new_mirror(&target, prepared)
            };
            match installed {
                Ok(workspace) => {
                    target.finish_run(
                        crate::request_runs::RunState::Succeeded,
                        format!("attached as workspace {workspace}"),
                        serde_json::json!({ "workspace_id": workspace }),
                    );
                    if target.activate
                        && let Some(window) = target.window
                        && let Some((main, engine)) = self.engines().window_pair(window)
                        && target
                            .view
                            .as_ref()
                            .is_some_and(|view| main.base.state.matches_identity(view))
                        && target.selection.as_ref().is_some_and(|selection| {
                            main.state.navigation.matches_generation(selection)
                        })
                        && engine.has_workspace(workspace)
                    {
                        self.focus_mirror_workspace(workspace);
                    }
                    if let Some(anchor) = target.anchor {
                        crate::app::auto_attach::forget_anchor_backoff(&mut self.remote, anchor);
                        self.remote.refusals.clear_notice(anchor);
                    }
                }
                Err(error) => self.fail_pending_mirror(ticket, &target, error.to_string()),
            }
        }
    }
    fn fail_pending_mirror(
        &mut self,
        ticket: ConnectionTicket,
        target: &PendingMirrorInstall,
        error: String,
    ) {
        self.state
            .mirror_attempts
            .cancel_connection(ticket, &mut self.remote);
        tracing::warn!(engine=?target.engine,"mirror connection could not be installed: {error}");
        let error = describe_attach_error(&error, target.remote_workspace);
        target.fail_run(format!("the mirror could not be installed: {error}"));
        if target.user {
            self.toast_attach_failure(target, &error);
        }
        if target.resync
            && let Some((workspace, epoch)) = &target.reconnect
            && let Some(index) = self.remote.sessions.iter().position(|session| {
                session.state.local_workspace == *workspace
                    && session.transport.frame_tx.epoch().same(epoch)
            })
        {
            self.remote.sessions[index].state.resync_pending = None;
            if target.anchor.is_some() {
                self.enter_reconnecting(index);
            } else {
                let session = self.remote.sessions.remove(index);
                self.cleanup_mirror_workspace(&session, true);
            }
        }
        if let Some(anchor) = target.anchor {
            self.remote.active.remove(&anchor);
            if target.reconnect.is_some() {
                self.on_reconnect_attempt_failed(anchor, &anyhow::anyhow!(error));
            } else {
                back_off_failed_first_attach(
                    &mut self.remote,
                    target.anchor,
                    target.reconnect.is_some(),
                    target.mapping.as_ref(),
                );
            }
        }
    }
}

/// 서버 거절 사유 중 사람이 고칠 수 없는 것은 그 이유 문장으로 바꾼다. 나머지는 그대로 둔다.
pub(crate) fn describe_attach_error(error: &str, remote_workspace: Option<u32>) -> String {
    match tasty_remote::transport::attach_rejection_reason(error) {
        Some(tasty_remote::transport::REJECTED_MIRROR_WORKSPACE) => crate::i18n::t_fmt(
            "remote.row.reason.chain",
            &remote_workspace.map_or_else(|| "?".to_string(), |id| id.to_string()),
        ),
        _ => error.to_string(),
    }
}

impl App {
    /// 팝업에서 시작한 시도가 실패하면 그 창에만 알린다. 창이 바뀌었으면 띄우지 않는다.
    pub(crate) fn toast_attach_failure(&mut self, target: &PendingMirrorInstall, error: &str) {
        let Some(window) = target.window else {
            return;
        };
        let Some((main, _)) = self.engines_mut().window_pair(window) else {
            return;
        };
        if !target
            .view
            .as_ref()
            .is_some_and(|view| main.base.state.matches_identity(view))
        {
            return;
        }
        let base = crate::i18n::t("attach.toast.mirror_connect_failed");
        let message = crate::i18n::fit_fragment(error, |fragment| format!("{base} ({fragment})"));
        main.state.toasts.push(
            message,
            crate::adapters::ui::ToastKind::Error,
            crate::adapters::ui::ToastScope::Window,
        );
        main.mark_dirty();
    }
}

/// 같은 대상의 이전 시도를 대체하고 연결을 큐에 넣는다. 큐가 거절하면 target이 이 함수 안에서
/// 사라지므로, 결과 없이 사라졌다는 `cancelled`보다 먼저 구체적인 실패를 run에 남긴다.
pub(super) fn queue_on(
    attempts: &mut super::attempts::MirrorAttempts,
    remote: &mut tasty_remote::outbound::Remote,
    mut target: PendingMirrorInstall,
    connection: ConnectionTarget,
    tunnel: Option<tasty_ssh::SshTunnel>,
    wake: Arc<dyn Fn() + Send + Sync>,
    heartbeat_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    target.remote_workspace = Some(connection.workspace);
    attempts.supersede_connections(&target, remote);
    attempts.supersede_manual(&target, remote);
    let ticket = remote
        .queue_connection(
            ConnectionTarget {
                anchor: target.anchor,
                mapping: target.mapping.clone(),
                ..connection
            },
            tunnel,
            wake,
            mirror_event_from_control,
            heartbeat_timeout,
        )
        .map_err(|error| {
            target.fail_run(format!(
                "the mirror connection could not be queued: {error}"
            ));
            anyhow::Error::msg(error)
        })?;
    attempts.register_connection(ticket, target);
    Ok(())
}

/// 접수 뒤 연결 단계에서 실패한 첫 자동 attach도 해석 실패와 같은 간격으로 미룬다.
/// 재연결(`reconnecting`)은 재연결 백오프가 맡으므로 기록하지 않는다.
fn back_off_failed_first_attach(
    remote: &mut tasty_remote::outbound::Remote,
    anchor: Option<u32>,
    reconnecting: bool,
    mapping: Option<&crate::model::WorkspaceAttachMapping>,
) {
    if let (Some(anchor), false) = (anchor, reconnecting) {
        crate::app::auto_attach::back_off_first_attach(remote, anchor, mapping);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> crate::model::WorkspaceAttachMapping {
        crate::model::WorkspaceAttachMapping {
            target: crate::model::WorkspaceAttachTarget::Profile { name: "p".into() },
            remote_workspace: Some(1),
        }
    }

    #[test]
    fn a_first_attach_that_fails_after_queueing_backs_off() {
        let mut remote = tasty_remote::outbound::Remote::new();
        back_off_failed_first_attach(&mut remote, Some(10), false, Some(&mapping()));
        let retry = remote.attach_retry.get(&10).expect("retry recorded");
        assert_eq!(retry.mapping, mapping());
        assert!(retry.holds(&mapping(), std::time::Instant::now()));
    }

    #[test]
    fn a_reconnect_or_an_unanchored_failure_records_no_retry() {
        let mut remote = tasty_remote::outbound::Remote::new();
        back_off_failed_first_attach(&mut remote, Some(10), true, Some(&mapping()));
        back_off_failed_first_attach(&mut remote, None, false, Some(&mapping()));
        assert!(remote.attach_retry.is_empty());
    }
}
