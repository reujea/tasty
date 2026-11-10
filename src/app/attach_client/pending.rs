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
    /// 연결할 저장된 원격 행. 행 없는 수동 요청과 새 행을 만드는 팝업 연결은 None이다.
    pub row: Option<u32>,
    /// 행의 연결 대상. 처리 중에 행의 대상이 바뀌거나 행이 닫히면 이 시도를 버린다.
    pub binding: Option<crate::model::RemoteBinding>,
    /// 행의 연결 토큰. 행을 닫거나 사용자가 끊으면 바뀌어 진행 중인 시도가 낡은 것이 된다.
    pub row_token: Option<std::sync::Weak<()>>,
    pub reconnect: Option<(u32, tasty_remote::connection::ConnectionEpoch)>,
    pub resync: bool,
    /// 최종 결과를 받을 요청의 run. 결과 없이 모든 사본이 사라지면 `cancelled`로 끝난다.
    pub run: Option<crate::request_runs::RunGuard>,
    /// 사용자가 원격 연결 팝업에서 시작했다. 실패하면 시작한 창에 토스트를 띄운다.
    pub user: bool,
    /// 행 없는 수동 요청(`remote.attach`·`attach.into_gui`)의 대상. 같은 대상의 새 요청은
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

/// 시작한 창이나 engine이 사라져 시도를 버릴 때의 결과.
pub(crate) const TARGET_DISCARDED: &str = "the window or engine that started the attach is gone";
impl App {
    pub(crate) fn mirror_install_target(
        &self,
        explicit: Option<EngineId>,
        row: Option<u32>,
        reconnect: Option<usize>,
        activate: bool,
    ) -> anyhow::Result<PendingMirrorInstall> {
        let row = row.or_else(|| {
            reconnect
                .and_then(|index| self.remote.sessions.get(index))
                .and_then(|session| session.state.saved_row)
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
                row.and_then(|workspace| {
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
        let binding = row.and_then(|id| {
            owner
                .find_workspace_index_for_id(id)
                .and_then(|index| owner.workspace_at(index))
                .and_then(|workspace| workspace.remote_row.clone())
        });
        let row_token = row
            .and_then(|id| owner.remote.row_tokens.get(&id))
            .map(Arc::downgrade);
        // 재연결은 처음 연결한 프로필을 그대로 쓴다. 저장된 행은 행의 프로필이다.
        // 수동 요청은 호출자가 자기 프로필로 바꾼다.
        let profile = match reconnect.and_then(|index| self.remote.sessions.get(index)) {
            Some(session) => session.state.profile.clone(),
            None => binding.as_ref().map(|binding| binding.profile.clone()),
        };
        Ok(PendingMirrorInstall {
            profile,
            remote_workspace: None,
            resync,
            binding,
            row_token,
            engine,
            window,
            view: main.map(|main| main.base.state.identity()),
            selection: main.map(|main| main.state.navigation.generation()),
            activate,
            row,
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
        if let Some(row) = target.row {
            let current = engine
                .find_workspace_index_for_id(row)
                .and_then(|index| engine.workspace_at(index));
            if current.is_none_or(|workspace| workspace.remote_row != target.binding)
                || target.row_token.as_ref().is_none_or(|token| {
                    engine
                        .remote
                        .row_tokens
                        .get(&row)
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
    /// 시작한 창이 사라져 버린 설치 대기가 그 행의 진행 중 표지를 가졌으면 지운다. 행 토큰이
    /// 그대로일 때만 이 시도의 표지다. 행을 닫거나 끊거나 팝업이 넘겨받으면 토큰이 바뀌고, 표지는
    /// 그쪽이 정리하거나 새 시도가 가진다. 첫 연결은 다시 보이거나 다시 고를 때 연결하도록 남긴다.
    /// 그 행을 기다리던 요청과 합류한 run 은 다음 `advance_row_waits` 가 행 사실(창 없음이면
    /// window_parked)로 끝낸다.
    fn end_discarded_row_attempt(&mut self, target: &PendingMirrorInstall) {
        let Some(row) = target.row else {
            return;
        };
        let owned = target.row_token.as_ref().is_some_and(|token| {
            self.engines.get(target.engine).is_some_and(|engine| {
                engine
                    .remote
                    .row_tokens
                    .get(&row)
                    .is_some_and(|current| token.ptr_eq(&Arc::downgrade(current)))
            })
        });
        if !owned || !self.remote.active.remove(&row) {
            return;
        }
        if target.reconnect.is_none() {
            self.remote.pending_reactivation.insert(row);
        }
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
                row: None,
                binding: None,
            },
            tunnel,
            self.attach_client_wake.waker(),
            self.services.attach_heartbeat.get(),
        )
    }
    pub(crate) fn poll_pending_mirror_installs(&mut self) {
        self.remote.collect_connections();
        self.end_expired_reconnect_windows();
        let pending = self.state.mirror_attempts.connection_snapshot();
        for (ticket, target) in pending {
            if !self.mirror_install_target_is_current(&target) {
                target.cancel_run(TARGET_DISCARDED);
                self.state
                    .mirror_attempts
                    .cancel_connection(ticket, &mut self.remote);
                self.end_discarded_row_attempt(&target);
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
                // 저장된 행에 처음 붙을 때는 그 행의 ID를 쓴다.
                target.reconnect.is_none() && target.row.is_none(),
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
                    if let Some(row) = target.row {
                        crate::app::auto_attach::forget_row_backoff(&mut self.remote, row);
                        self.remote.refusals.clear_notice(row);
                    }
                }
                Err(error) => self.fail_pending_mirror(ticket, &target, error.to_string()),
            }
        }
    }
    pub(crate) fn fail_pending_mirror(
        &mut self,
        ticket: ConnectionTicket,
        target: &PendingMirrorInstall,
        error: String,
    ) {
        self.state
            .mirror_attempts
            .cancel_connection(ticket, &mut self.remote);
        tracing::warn!(engine=?target.engine,"mirror connection could not be installed: {error}");
        // 상태 전이는 서버 거절 머리가 남은 원 오류로 분류한다. 번역한 문장은 run 결과와 토스트에만 쓴다.
        let shown = describe_attach_error(&error, target.remote_workspace);
        target.fail_run(format!("the mirror could not be installed: {shown}"));
        if target.user {
            self.toast_attach_failure(target, &shown);
        }
        if target.resync
            && let Some((workspace, epoch)) = &target.reconnect
            && let Some(index) = self.remote.sessions.iter().position(|session| {
                session.state.local_workspace == *workspace
                    && session.transport.frame_tx.epoch().same(epoch)
            })
        {
            self.remote.sessions[index].state.resync_pending = None;
            if target.row.is_some() {
                self.enter_reconnecting(index);
            } else {
                let session = self.remote.sessions.remove(index);
                self.cleanup_mirror_workspace(&session, true);
            }
        }
        if let Some(row) = target.row {
            self.remote.active.remove(&row);
            if target.reconnect.is_some() {
                self.on_reconnect_attempt_failed(row, &anyhow::anyhow!(error));
            } else if let Some(binding) = &target.binding {
                self.on_first_connect_failed(row, binding, &anyhow::anyhow!(error));
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
                row: target.row,
                binding: target.binding.clone(),
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

// X11 이벤트 루프로 App 을 만드는 시험이라 Linux 에서만 컴파일한다(preset_shutdown_tests 와 같다).
#[cfg(all(test, target_os = "linux"))]
#[path = "pending_failure_tests.rs"]
mod failure_tests;
