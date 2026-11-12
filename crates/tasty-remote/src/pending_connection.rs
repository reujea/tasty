//! Resource ownership while a host waits for its fixed engine's durable logical ID reservation.
use super::outbound::{AttemptToken, Remote};
use super::transport::PreparedConnection;
// This slot transfers the tunnel with Option::take; it does not protect protocol frame writes.
static TUNNEL_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionTicket(pub u64);
pub(crate) enum PendingConnection {
    Connecting(AttemptToken),
    Ready(AttemptToken, Box<PreparedConnection>),
    Failed(AttemptToken, String),
}
pub(crate) struct ConnectionOutcome {
    pub ticket: ConnectionTicket,
    pub result: Result<PreparedConnection, String>,
}
/// 연결이 무엇에 붙는지 — 원격 끝점과 그것을 받을 로컬 자리. 어떻게 붙는지를
/// 정하는 터널·깨우기·디코더와 달리 이 넷은 대상 하나를 함께 가리킨다.
pub struct ConnectionTarget {
    /// 원격 끝점의 포트.
    pub port: u16,
    /// 이 연결을 받을 로컬 workspace.
    pub workspace: u32,
    /// 연결할 저장된 원격 행. 같은 행의 이전 시도를 대체한다. 수동 요청은 None이다.
    pub row: Option<u32>,
    /// 저장된 원격 행의 연결 대상.
    pub binding: Option<tasty_model::RemoteBinding>,
}

/// 첫 attach 연결의 실패를 연결 오류 문구로 바꾼다. 터널 너머 원격 포트에 리스너가 없다고 판정하면
/// 그 안내([`tasty_ssh::RemoteListenerAbsent`])를 앞에 두고 원래 원인을 뒤에 붙인다.
fn connect_failure_message(
    verdict: Option<&tasty_ssh::ListenerVerdict>,
    error: anyhow::Error,
) -> String {
    let Some(verdict) = verdict else {
        return error.to_string();
    };
    let error = verdict.explain_failure(error);
    if error
        .downcast_ref::<tasty_ssh::RemoteListenerAbsent>()
        .is_some()
    {
        format!("{error:#}")
    } else {
        error.to_string()
    }
}

impl Remote {
    pub fn queue_connection(
        &mut self,
        target: ConnectionTarget,
        tunnel: Option<tasty_ssh::SshTunnel>,
        wake: std::sync::Arc<dyn Fn() + Send + Sync>,
        decode: fn(&[u8]) -> Option<super::client_session::MirrorEvent>,
        heartbeat_timeout: std::time::Duration,
    ) -> Result<ConnectionTicket, String> {
        let ConnectionTarget {
            port,
            workspace,
            row,
            binding,
        } = target;
        if self.pending_connections.len() >= 8 {
            self.retire_tunnel(tunnel);
            return Err("pending remote connection capacity exhausted".into());
        }
        let ticket = ConnectionTicket(self.next_connection);
        let Some(next) = self.next_connection.checked_add(1) else {
            self.retire_tunnel(tunnel);
            return Err("connection ticket exhausted".into());
        };
        let token = match self.begin_attempt(row, binding) {
            Ok(token) => token,
            Err(error) => {
                self.retire_tunnel(tunnel);
                return Err(error.into());
            }
        };
        self.next_connection = next;
        self.pending_connections
            .insert(ticket, PendingConnection::Connecting(token.clone()));
        let tx = self.connection_tx.clone();
        let worker_wake = wake.clone();
        let worker_token = token.clone();
        // Spawn rejection drops its closure on the caller thread; keep the tunnel recoverable.
        let tunnel = std::sync::Arc::new(std::sync::Mutex::new(tunnel));
        let worker_tunnel = tunnel.clone();
        if let Err(error) = self.spawn_attempt(token.clone(), move || {
            let tunnel = worker_tunnel
                .lock()
                .unwrap_or_else(|poison| {
                    tasty_utils::poison::recover_poisoned(
                        poison,
                        "pending remote connection tunnel",
                        &TUNNEL_POISON_REPORTED,
                    )
                })
                .take();
            // 터널은 연결 안으로 넘어가므로 실패를 분류할 판정 핸들을 먼저 받아 둔다.
            let verdict = tunnel.as_ref().map(tasty_ssh::SshTunnel::listener_verdict);
            let result = PreparedConnection::connect(
                worker_token,
                port,
                workspace,
                tunnel,
                worker_wake,
                decode,
                heartbeat_timeout,
            )
            .map_err(|error| connect_failure_message(verdict.as_ref(), error));
            // Deliver cancelled success too: collect_connections tracks the exact I/O retirement
            // receipt before dropping its prepared owner. Shutdown keeps draining this bounded queue.
            if let Err(error) = tx.send(ConnectionOutcome { ticket, result }) {
                drop(error);
            }
            wake();
        }) {
            self.pending_connections.remove(&ticket);
            self.retire_tunnel(
                tunnel
                    .lock()
                    .unwrap_or_else(|poison| {
                        tasty_utils::poison::recover_poisoned(
                            poison,
                            "pending remote connection tunnel",
                            &TUNNEL_POISON_REPORTED,
                        )
                    })
                    .take(),
            );
            return Err(error);
        }
        Ok(ticket)
    }
    pub fn collect_connections(&mut self) {
        self.reap_attempts();
        while let Ok(outcome) = self.connection_rx.try_recv() {
            if let Ok(prepared) = &outcome.result {
                self.track_workers(prepared.transport.workers.receipt());
            }
            let Some(PendingConnection::Connecting(token)) =
                self.pending_connections.remove(&outcome.ticket)
            else {
                drop(outcome);
                continue;
            };
            if !token.is_active() || self.shutdown_started.is_some() {
                drop(outcome);
                continue;
            }
            self.pending_connections.insert(
                outcome.ticket,
                match outcome.result {
                    Ok(prepared) => PendingConnection::Ready(token, Box::new(prepared)),
                    Err(error) => PendingConnection::Failed(token, error),
                },
            );
        }
    }
    pub fn prepared_connection(&self, ticket: ConnectionTicket) -> Option<&PreparedConnection> {
        match self.pending_connections.get(&ticket) {
            Some(PendingConnection::Ready(token, prepared)) if token.is_active() => Some(prepared),
            _ => None,
        }
    }
    pub fn connection_error(&self, ticket: ConnectionTicket) -> Option<String> {
        match self.pending_connections.get(&ticket) {
            Some(PendingConnection::Failed(_, error)) => Some(error.clone()),
            Some(PendingConnection::Connecting(token) | PendingConnection::Ready(token, _))
                if !token.is_active() =>
            {
                Some("connection attempt was retired".into())
            }
            None => Some("connection attempt no longer exists".into()),
            _ => None,
        }
    }
    pub fn take_connection(&mut self, ticket: ConnectionTicket) -> Option<PreparedConnection> {
        let PendingConnection::Ready(token, prepared) = self.pending_connections.remove(&ticket)?
        else {
            return None;
        };
        let active = self.finish_attempt(&token).is_some();
        active.then_some(*prepared)
    }
    pub fn cancel_connection(&mut self, ticket: ConnectionTicket) {
        if let Some(pending) = self.pending_connections.remove(&ticket) {
            let token = match &pending {
                PendingConnection::Connecting(token)
                | PendingConnection::Ready(token, _)
                | PendingConnection::Failed(token, _) => token,
            };
            self.cancel_attempt(token);
            drop(pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 원격 포트에 리스너가 없어 첫 연결이 끊기면 그 안내가 연결 오류에 들어간다. 판정 근거가
    /// 없거나 터널이 없으면 원래 문구 그대로다.
    #[test]
    fn a_first_connection_dropped_by_a_missing_listener_says_so() {
        tasty_i18n::init("en");
        let dropped = || tasty_ssh::connection_closed_error("attach handshake closed");
        let absent = tasty_ssh::ListenerVerdict::with_observation(4321, true);
        let message = connect_failure_message(Some(&absent), dropped());
        let expected = tasty_ssh::RemoteListenerAbsent { remote_port: 4321 }.to_string();
        assert!(message.starts_with(&expected), "{message}");
        assert!(message.ends_with("attach handshake closed"), "{message}");

        let present = tasty_ssh::ListenerVerdict::with_observation(4321, false);
        assert_eq!(
            connect_failure_message(Some(&present), dropped()),
            "attach handshake closed"
        );
        assert_eq!(
            connect_failure_message(None, dropped()),
            "attach handshake closed"
        );
    }
}
