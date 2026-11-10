//! 원격 workspace 조회·attach의 블로킹 연결 준비를 워커에서 수행한다.

use crate::AppEvent;
use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::ipc as host_ipc;
use crate::ipc::server::{IpcCommand, send_response};

impl App {
    pub(super) fn ipc_dispatch_remote_workspaces(&mut self, cmd: &IpcCommand) {
        crate::app::services::spawn_remote_workspaces(
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            &cmd.request.params,
            &cmd.response_tx,
        );
    }

    /// 자기 포트라서 연결하지 않은 최근 attach 시도(오래된 것부터)와, 지금 연결하지 않은 채
    /// 남은 자동 attach 매핑의 안내(워크스페이스 ID 순)를 돌려준다.
    pub(super) fn ipc_dispatch_remote_refusals(&mut self, cmd: &IpcCommand) {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let refusals: Vec<_> = self.remote.refusals.recent().collect();
        let notices: Vec<_> = self
            .remote
            .refusals
            .notices()
            .into_iter()
            .map(|(workspace_id, notice)| {
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "kind": notice.kind,
                    "target": notice.target,
                })
            })
            .collect();
        send_response(
            &cmd.response_tx,
            host_ipc::protocol::JsonRpcResponse::success(
                rpc_id,
                serde_json::json!({ "refusals": refusals, "notices": notices }),
            ),
        );
    }

    /// GUI의 "워크스페이스 연결 끊기"와 같다. 이쪽 mirror·세션·재연결 시도만 정리하고
    /// 원격 workspace와 그 프로세스는 남긴다. 같은 원격 workspace에 다시 attach할 수 있다.
    pub(super) fn ipc_dispatch_remote_disconnect(&mut self, cmd: &IpcCommand) {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let workspace_id = match params::require_u32(&cmd.request.params, "workspace_id", &rpc_id) {
            Ok(id) => id,
            Err(response) => {
                send_response(&cmd.response_tx, response);
                return;
            }
        };
        let remote_workspace = self
            .remote
            .sessions
            .iter()
            .find(|session| session.state.local_workspace == workspace_id)
            .map(|session| session.state.remote_workspace);
        let response = match remote_workspace {
            Some(remote_workspace) if self.disconnect_mirror_workspace(workspace_id) => {
                host_ipc::protocol::JsonRpcResponse::success(
                    rpc_id,
                    serde_json::json!({
                        "disconnected": true,
                        "workspace_id": workspace_id,
                        "remote_workspace": remote_workspace,
                    }),
                )
            }
            _ => host_ipc::protocol::JsonRpcResponse::invalid_params(
                rpc_id,
                format!("workspace {workspace_id} is not a mirror with an attach session"),
            ),
        };
        send_response(&cmd.response_tx, response);
    }

    /// mirror를 만들되 로컬·원격 포커스는 옮기지 않는다.
    /// 기존 workspace는 attaching으로 즉시 응답한다. 새 workspace는 생성 결과 ID를 응답한 뒤
    /// mirror 연결을 이어 가므로 두 경우 모두 응답이 mirror 연결 완료를 뜻하지 않는다.
    /// 자기 자신이 대상이면 loopback 표기는 여기서 동기 오류로 끝난다. SSH 경로는 터널 너머
    /// `system.info` 왕복이 필요해, 새 workspace는 생성 전 같은 오류로 응답하고 기존 workspace는
    /// 즉시 응답한 뒤 비동기로 거절해 `remote.refusals`에 남긴다.
    pub(super) fn ipc_dispatch_remote_attach(
        &mut self,
        cmd: &IpcCommand,
        caller: &host_ipc::caller::CallerContext,
    ) {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let conn = match RemoteConnParams::parse(&cmd.request.params) {
            Ok(c) => c,
            Err(msg) => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::invalid_params(rpc_id, msg),
                );
                return;
            }
        };
        let target = match RemoteAttachTarget::parse(&cmd.request.params) {
            Ok(t) => t,
            Err(msg) => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::invalid_params(rpc_id, msg),
                );
                return;
            }
        };

        // loopback 대상은 포트가 인자(또는 프로필)로 정해지므로 원격에 workspace를 만들거나
        // 연결 시도를 시작하기 전에 자기 포트인지 판정한다.
        if let Some(port) = conn.loopback_port()
            && let Some(refused) = crate::ipc::handler::attach::refuse_own_port(
                self.services.own_ipc_port(),
                port,
                rpc_id.clone(),
            )
        {
            send_response(&cmd.response_tx, refused);
            return;
        }
        let attempt = match self.remote.begin_attempt(None, None) {
            Ok(attempt) => attempt,
            Err(error) => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::error(rpc_id, -32050, error),
                );
                return;
            }
        };
        // 기존 workspace 연결은 접수 응답 뒤에 끝나므로 결과를 run으로 남긴다. 새 workspace는
        // 생성 결과를 지연 회신하므로 run을 만들지 않는다.
        let run = match &target {
            RemoteAttachTarget::Existing(remote_ws) => Some(crate::request_runs::begin(
                "remote.attach",
                crate::request_runs::reader_of(caller),
                serde_json::json!({ "remote_workspace": remote_ws, "attempt": attempt.id() }),
            )),
            RemoteAttachTarget::Create { .. } => None,
        };
        let manual = match &target {
            RemoteAttachTarget::Existing(remote_ws) => Some(format!(
                "profile {:?} ssh {:?} workspace {remote_ws}",
                conn.profile, conn.ssh
            )),
            RemoteAttachTarget::Create { .. } => None,
        };
        let target_binding = match self.mirror_install_target(None, None, None, false) {
            Ok(target) => crate::app::attach_client::pending::PendingMirrorInstall {
                manual,
                profile: conn.profile.clone(),
                ..target.with_run(run)
            },
            Err(error) => {
                run.into_iter().for_each(crate::request_runs::forget);
                self.remote.finish_attempt(&attempt);
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::invalid_params(rpc_id, error.to_string()),
                );
                return;
            }
        };
        self.state
            .mirror_attempts
            .supersede_manual(&target_binding, &mut self.remote);
        self.state
            .mirror_attempts
            .register_endpoint(attempt.clone(), target_binding);
        let attempt_id = attempt.id();
        let tx = self.remote.tx.clone();
        let proxy = self.view.proxy.clone();
        match target {
            RemoteAttachTarget::Existing(remote_ws) => {
                if let Err(error) = self.remote.spawn_attempt(attempt.clone(), move || {
                    let result = resolve_existing_endpoint(&attempt, || conn.resolve_endpoint());
                    send_attach_outcome(&tx, &proxy, attempt, remote_ws, result);
                }) {
                    run.into_iter().for_each(crate::request_runs::forget);
                    send_response(
                        &cmd.response_tx,
                        host_ipc::protocol::JsonRpcResponse::internal_error(rpc_id, error),
                    );
                    return;
                }
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::success(
                        rpc_id,
                        serde_json::json!({
                            "attaching": true,
                            "remote_workspace": remote_ws,
                            "attempt": attempt_id,
                            "run": run,
                        }),
                    ),
                );
            }
            RemoteAttachTarget::Create { name, cwd } => {
                let response_tx = cmd.response_tx.clone();
                let failure_id = rpc_id.clone();
                if let Err(error) = self.remote.spawn_attempt(attempt.clone(), move || {
                    remote_attach_create_worker(
                        conn,
                        name,
                        cwd,
                        rpc_id,
                        &response_tx,
                        &tx,
                        &proxy,
                        attempt,
                    );
                }) {
                    send_response(
                        &cmd.response_tx,
                        host_ipc::protocol::JsonRpcResponse::internal_error(failure_id, error),
                    );
                }
            }
        }
    }
}

use crate::app::services::RemoteConnParams;

impl RemoteConnParams {
    /// 대상이 loopback 직결이면 그 포트. 프로필은 파일에서 읽어 해석하며, 해석에 실패하면
    /// 판정하지 않고 워커의 해석 오류에 맡긴다.
    fn loopback_port(&self) -> Option<u16> {
        let (target, ..) = tasty_remote::browse::resolve_connection_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            &self.remote_tasty,
            &self.remote_port_mode,
        )
        .ok()?;
        tasty_remote::browse::parse_loopback_port(&target.destination)
    }
    /// SSH 접속 준비는 블로킹하므로 워커에서 호출한다.
    fn resolve_endpoint(&self) -> anyhow::Result<(Option<tasty_ssh::SshTunnel>, u16)> {
        let (target, rt, pm, pf) = tasty_remote::browse::resolve_connection_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            &self.remote_tasty,
            &self.remote_port_mode,
        )?;
        tasty_remote::browse::resolve_endpoint(&target, &rt, &pm, pf.as_deref())
    }
}

enum RemoteAttachTarget {
    Existing(u32),
    Create {
        name: Option<String>,
        cwd: Option<String>,
    },
}

impl RemoteAttachTarget {
    fn parse(params: &serde_json::Value) -> Result<Self, String> {
        // 범위 밖 숫자를 잘라 다른 workspace ID로 바꾸지 않는다.
        let remote_ws = params::read_int::<u32>(params, "remote_workspace")?;
        let new_workspace = params
            .get("new_workspace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        match (remote_ws, new_workspace) {
            (Some(_), true) => {
                Err("'remote_workspace' and 'new_workspace' are mutually exclusive".to_string())
            }
            (None, false) => Err(
                "one of 'remote_workspace' (u32) or 'new_workspace' (bool) is required".to_string(),
            ),
            (Some(id), false) => {
                if name.is_some() || cwd.is_some() {
                    return Err(
                        "'name'/'cwd' require 'new_workspace': true (they describe the workspace to create)"
                            .to_string(),
                    );
                }
                Ok(Self::Existing(id))
            }
            (None, true) => Ok(Self::Create { name, cwd }),
        }
    }
}

/// 결과 채널에 터널을 넘기고 메인 루프를 깨운다. mirror 생성은 이후에 수행한다.
fn send_attach_outcome(
    tx: &std::sync::mpsc::Sender<tasty_remote::outbound::AutoAttachOutcome>,
    proxy: &winit::event_loop::EventLoopProxy<AppEvent>,
    attempt: tasty_remote::outbound::AttemptToken,
    remote_ws: u32,
    result: anyhow::Result<(Option<tasty_ssh::SshTunnel>, u16)>,
) {
    let outcome = tasty_remote::outbound::AutoAttachOutcome {
        attempt,
        row: None,
        remote_ws,
        result,
        is_reconnect: false,
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "The receiving session or event loop may have already ended."
    )]
    let _ = tx.send(outcome); // 수신자(메인 루프) drop 시에만 실패 — 무시.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "The receiving session or event loop may have already ended."
    )]
    let _ = proxy.send_event(AppEvent::AutoAttachReady); // event loop 종료 시에만 실패 — 무시
}

/// 기존 workspace attach의 엔드포인트를 해석하고 SSH 너머의 자기 자신을 거절한다.
/// 해석은 시험에서 SSH 없이 바꿔 끼울 수 있게 받는다.
fn resolve_existing_endpoint<T>(
    attempt: &tasty_remote::outbound::AttemptToken,
    resolve: impl FnOnce() -> anyhow::Result<(Option<T>, u16)>,
) -> anyhow::Result<(Option<T>, u16)> {
    let (tunnel, port) = resolve()?;
    tasty_remote::self_instance::refuse_this_instance(tunnel.is_some(), port, Some(attempt))?;
    Ok((tunnel, port))
}

/// 새 workspace를 만들기 전에 엔드포인트를 준비한다. 실패하면 호출자에게 보낼 응답과
/// 시도 결과로 남길 오류를 함께 돌려준다. 해석은 시험에서 바꿔 끼울 수 있게 받는다.
fn prepare_create_endpoint<T>(
    rpc_id: &serde_json::Value,
    attempt: &tasty_remote::outbound::AttemptToken,
    resolve: impl FnOnce() -> anyhow::Result<(Option<T>, u16)>,
) -> Result<(Option<T>, u16), (host_ipc::protocol::JsonRpcResponse, anyhow::Error)> {
    let (tunnel, port) = resolve().map_err(|e| {
        (
            host_ipc::protocol::JsonRpcResponse::error(
                rpc_id.clone(),
                -32050,
                // 중첩 오류의 접속 실패 원인도 응답에 포함한다.
                format!("remote endpoint resolve failed: {e:#}"),
            ),
            anyhow::anyhow!("remote attach preparation failed: {e:#}"),
        )
    })?;
    if !attempt.is_active() {
        return Err((
            host_ipc::protocol::JsonRpcResponse::error(
                rpc_id.clone(),
                -32050,
                "remote connection attempt cancelled",
            ),
            anyhow::anyhow!("remote connection attempt cancelled"),
        ));
    }
    // 호스트명·LAN IP로 자기 머신을 가리킨 SSH 대상은 원격(=자기)에 workspace를 만들기 전에 거절한다.
    if let Err(e) =
        tasty_remote::self_instance::refuse_this_instance(tunnel.is_some(), port, Some(attempt))
    {
        let response = if e
            .downcast_ref::<tasty_remote::self_instance::ThisInstance>()
            .is_some()
        {
            host_ipc::protocol::JsonRpcResponse::invalid_params(rpc_id.clone(), e.to_string())
        } else {
            host_ipc::protocol::JsonRpcResponse::error(
                rpc_id.clone(),
                -32050,
                format!("remote system.info failed: {e:#}"),
            )
        };
        return Err((
            response,
            anyhow::anyhow!("remote attach preparation failed: {e:#}"),
        ));
    }
    Ok((tunnel, port))
}

/// 새 workspace의 생성 결과를 응답하고 attach를 요청한다. 실패하면 attach 요청은 보내지 않는다.
#[allow(clippy::too_many_arguments)] // reason: spawn_attempt 클로저가 move 로 잡은 값을 그대로 펼친 워커 진입점이다 — 묶으면 캡처를 한 번 더 옮기는 구조체만 생긴다
fn remote_attach_create_worker(
    conn: RemoteConnParams,
    name: Option<String>,
    cwd: Option<String>,
    rpc_id: serde_json::Value,
    response_tx: &std::sync::mpsc::SyncSender<host_ipc::protocol::JsonRpcResponse>,
    tx: &std::sync::mpsc::Sender<tasty_remote::outbound::AutoAttachOutcome>,
    proxy: &winit::event_loop::EventLoopProxy<AppEvent>,
    attempt: tasty_remote::outbound::AttemptToken,
) {
    let (tunnel, port) =
        match prepare_create_endpoint(&rpc_id, &attempt, || conn.resolve_endpoint()) {
            Ok(v) => v,
            Err((response, error)) => {
                send_response(response_tx, response);
                send_attach_outcome(tx, proxy, attempt, 0, Err(error));
                return;
            }
        };
    let created = match tasty_remote::create::create_via_port(port, name.as_deref(), cwd.as_deref())
    {
        Ok(c) => c,
        Err(e) => {
            send_response(
                response_tx,
                host_ipc::protocol::JsonRpcResponse::error(
                    rpc_id,
                    -32050,
                    format!("remote workspace.create failed: {e:#}"),
                ),
            );
            send_attach_outcome(
                tx,
                proxy,
                attempt,
                0,
                Err(anyhow::anyhow!("remote attach preparation failed: {e:#}")),
            );
            return;
        }
    };
    send_response(
        response_tx,
        host_ipc::protocol::JsonRpcResponse::success(
            rpc_id,
            serde_json::json!({
                "attaching": true,
                "created": true,
                "remote_workspace": created.id,
                "name": created.name,
                "index": created.index,
                "attempt": attempt.id(),
            }),
        ),
    );
    send_attach_outcome(tx, proxy, attempt, created.id, Ok((tunnel, port)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::auto_attach::fake_peer;

    fn attempt() -> (
        tasty_remote::outbound::Remote,
        tasty_remote::outbound::AttemptToken,
    ) {
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        (remote, attempt)
    }

    fn this_instance() -> serde_json::Value {
        serde_json::json!({ "instance_id": tasty_ipc::instance::instance_id() })
    }

    #[test]
    fn an_existing_workspace_attach_asks_a_tunnel_peer() {
        let (port, server) = fake_peer::serve_once(this_instance());
        let (_remote, attempt) = attempt();
        let error =
            resolve_existing_endpoint(&attempt, || Ok((Some(()), port))).expect_err("refused");
        assert!(server.join().expect("server").contains("\"system.info\""));
        assert!(
            error
                .downcast_ref::<tasty_remote::self_instance::ThisInstance>()
                .is_some(),
            "{error}"
        );
    }

    #[test]
    fn an_existing_workspace_attach_does_not_ask_a_direct_endpoint() {
        let (_remote, attempt) = attempt();
        let port = fake_peer::free_port();
        let resolved = resolve_existing_endpoint(&attempt, || Ok((None::<()>, port)));
        assert_eq!(resolved.expect("direct endpoint").1, port);
    }

    #[test]
    fn a_new_workspace_is_refused_with_invalid_params_before_creation() {
        let (port, server) = fake_peer::serve_once(this_instance());
        let (_remote, attempt) = attempt();
        let rpc_id = serde_json::json!(7);
        let Err((response, _)) =
            prepare_create_endpoint(&rpc_id, &attempt, || Ok((Some(()), port)))
        else {
            panic!("this instance must be refused");
        };
        assert!(server.join().expect("server").contains("\"system.info\""));
        let error = response.error.expect("error response");
        assert_eq!(error.code, -32602);
        assert!(
            error.message.contains("attaching to itself is refused"),
            "{}",
            error.message
        );
        assert_eq!(response.id, rpc_id);
    }

    #[test]
    fn a_new_workspace_on_a_direct_endpoint_is_not_asked() {
        let (_remote, attempt) = attempt();
        let port = fake_peer::free_port();
        let prepared =
            prepare_create_endpoint(&serde_json::json!(1), &attempt, || Ok((None::<()>, port)));
        assert_eq!(prepared.map(|(_, p)| p).ok(), Some(port));
    }
}
