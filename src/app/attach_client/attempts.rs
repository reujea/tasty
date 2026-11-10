//! App continuations retain targets; Remote owns sockets, tunnels and cancellation resources.
use super::pending::PendingMirrorInstall;
use std::collections::HashMap;
use tasty_remote::{
    connection::ConnectionEpoch,
    outbound::{AttemptToken, Remote},
    pending_connection::ConnectionTicket,
};

const REPLACED: &str = "replaced by a newer attach to the same target";

#[derive(Default)]
pub(crate) struct MirrorAttempts {
    endpoints: HashMap<AttemptToken, PendingMirrorInstall>,
    connections: HashMap<ConnectionTicket, PendingMirrorInstall>,
}
impl MirrorAttempts {
    pub(crate) fn register_endpoint(&mut self, token: AttemptToken, target: PendingMirrorInstall) {
        self.discard_inactive_endpoints();
        self.endpoints.insert(token, target);
    }
    /// 같은 대상의 수동 요청 중 아직 끝나지 않은 것을 취소한다. 엔드포인트를 해석 중인 요청과
    /// 연결 중인 요청을 모두 본다. 새 요청 자신은 등록 전에 부른다.
    pub(crate) fn supersede_manual(&mut self, target: &PendingMirrorInstall, remote: &mut Remote) {
        let Some(key) = target.manual.as_deref() else {
            return;
        };
        let same = |pending: &PendingMirrorInstall| pending.manual.as_deref() == Some(key);
        self.endpoints.retain(|token, pending| {
            if !same(pending) {
                return true;
            }
            pending.cancel_run(REPLACED);
            remote.cancel_attempt(token);
            false
        });
        self.connections.retain(|ticket, pending| {
            if !same(pending) {
                return true;
            }
            pending.cancel_run(REPLACED);
            remote.cancel_connection(*ticket);
            false
        });
    }
    pub(crate) fn take_endpoint(&mut self, token: &AttemptToken) -> Option<PendingMirrorInstall> {
        self.endpoints.remove(token).filter(|_| token.is_active())
    }
    pub(crate) fn cancel_endpoint(&mut self, token: &AttemptToken, remote: &mut Remote) {
        self.endpoints.remove(token);
        remote.cancel_attempt(token);
    }
    pub(crate) fn discard_inactive_endpoints(&mut self) {
        self.endpoints.retain(|token, _| token.is_active());
    }
    pub(crate) fn endpoint_snapshot(&self) -> Vec<(AttemptToken, PendingMirrorInstall)> {
        self.endpoints
            .iter()
            .map(|(token, target)| (token.clone(), target.clone()))
            .collect()
    }
    pub(crate) fn supersede_connections(
        &mut self,
        target: &PendingMirrorInstall,
        remote: &mut Remote,
    ) {
        self.connections.retain(|ticket, pending| {
            let replaced = (target.row.is_some() || target.reconnect.is_some())
                && pending.engine == target.engine
                && pending.reconnect.as_ref().map(|(id, _)| *id)
                    == target.reconnect.as_ref().map(|(id, _)| *id)
                && pending.row == target.row;
            if replaced {
                pending.cancel_run(REPLACED);
                remote.cancel_connection(*ticket);
            }
            !replaced
        });
    }
    pub(crate) fn register_connection(
        &mut self,
        ticket: ConnectionTicket,
        target: PendingMirrorInstall,
    ) {
        self.connections.insert(ticket, target);
    }
    pub(crate) fn finish_connection(&mut self, ticket: ConnectionTicket) {
        self.connections.remove(&ticket);
    }
    pub(crate) fn cancel_connection(&mut self, ticket: ConnectionTicket, remote: &mut Remote) {
        self.finish_connection(ticket);
        remote.cancel_connection(ticket);
    }
    pub(crate) fn connection_snapshot(&self) -> Vec<(ConnectionTicket, PendingMirrorInstall)> {
        self.connections
            .iter()
            .map(|(ticket, target)| (*ticket, target.clone()))
            .collect()
    }
    pub(crate) fn installing_epoch(&self, workspace: u32, epoch: &ConnectionEpoch) -> bool {
        self.connections.values().any(|pending| {
            pending
                .reconnect
                .as_ref()
                .is_some_and(|(id, pending_epoch)| *id == workspace && epoch.same(pending_epoch))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(
        engine: crate::runtime::engine_session::EngineId,
        row: Option<u32>,
    ) -> PendingMirrorInstall {
        PendingMirrorInstall {
            engine,
            row,
            window: None,
            view: None,
            selection: None,
            activate: false,
            binding: None,
            row_token: None,
            reconnect: None,
            resync: false,
            run: None,
            user: false,
            profile: None,
            remote_workspace: None,
            manual: None,
        }
    }

    #[test]
    fn a_manual_request_replaces_only_the_earlier_one_for_the_same_target() {
        use crate::request_runs::{begin, lookup};
        let (_, engine) = crate::state::tests::test_state();
        let mut remote = Remote::new();
        let mut pending = MirrorAttempts::default();
        let manual = |key: &str, run: u64| PendingMirrorInstall {
            manual: Some(key.to_string()),
            ..target(engine.id, None).with_run(Some(run))
        };
        let resolving = begin("remote.attach", None, serde_json::Value::Null);
        let connecting = begin("attach.into_gui", None, serde_json::Value::Null);
        let other = begin("attach.into_gui", None, serde_json::Value::Null);
        let newer = begin("attach.into_gui", None, serde_json::Value::Null);
        let token = remote.begin_attempt(None, None).unwrap();
        pending.register_endpoint(token.clone(), manual("port 1 workspace 2", resolving));
        pending.register_connection(
            ConnectionTicket(1),
            manual("port 1 workspace 2", connecting),
        );
        pending.register_connection(ConnectionTicket(2), manual("port 1 workspace 3", other));

        let replacement = manual("port 1 workspace 2", newer);
        pending.supersede_manual(&replacement, &mut remote);
        pending.register_connection(ConnectionTicket(3), replacement);

        for run in [resolving, connecting] {
            let ended = lookup(run, None).unwrap();
            assert_eq!(ended["state"], "cancelled");
            assert_eq!(ended["result"], REPLACED);
        }
        assert!(
            !token.is_active(),
            "the replaced endpoint attempt is cancelled"
        );
        assert_eq!(lookup(other, None).unwrap()["state"], "running");
        assert_eq!(lookup(newer, None).unwrap()["state"], "running");
        // 행 없는 요청은 기존 대체 규칙으로는 서로 바꾸지 않는다.
        pending.supersede_connections(&target(engine.id, None), &mut remote);
        assert_eq!(lookup(other, None).unwrap()["state"], "running");
    }

    #[test]
    fn a_full_connection_queue_fails_the_run_with_its_reason() {
        use crate::request_runs::{begin, lookup};
        let (_, engine) = crate::state::tests::test_state();
        let mut remote = Remote::new();
        let mut pending = MirrorAttempts::default();
        // 연결을 받고 답하지 않는 상대라 앞의 8건은 큐에 남는다.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(|| {});
        let queue = |pending: &mut MirrorAttempts, remote: &mut Remote, workspace: u32| {
            let run = begin("attach.into_gui", None, serde_json::Value::Null);
            let target = PendingMirrorInstall {
                manual: Some(format!("port {port} workspace {workspace}")),
                ..target(engine.id, None).with_run(Some(run))
            };
            let queued = super::super::pending::queue_on(
                pending,
                remote,
                target,
                tasty_remote::pending_connection::ConnectionTarget {
                    port,
                    workspace,
                    row: None,
                    binding: None,
                },
                None,
                wake.clone(),
                tasty_ipc::stream::HEARTBEAT_TIMEOUT,
            );
            (run, queued)
        };
        for workspace in 0..8 {
            assert!(queue(&mut pending, &mut remote, workspace).1.is_ok());
        }
        let (run, queued) = queue(&mut pending, &mut remote, 8);
        assert!(queued.is_err());
        let ended = lookup(run, None).unwrap();
        assert_eq!(ended["state"], "failed");
        assert_eq!(
            ended["result"],
            "the mirror connection could not be queued: pending remote connection capacity exhausted"
        );
        drop(listener);
    }

    #[test]
    fn a_replaced_or_dropped_attempt_ends_its_own_run_only() {
        use crate::request_runs::{begin, lookup};
        let (_, engine) = crate::state::tests::test_state();
        let mut remote = Remote::new();
        let mut pending = MirrorAttempts::default();
        let first = begin("remote.attach", None, serde_json::Value::Null);
        let second = begin("remote.attach", None, serde_json::Value::Null);
        pending.register_connection(
            ConnectionTicket(1),
            target(engine.id, Some(10)).with_run(Some(first)),
        );
        let replacement = target(engine.id, Some(10)).with_run(Some(second));
        pending.supersede_connections(&replacement, &mut remote);
        pending.register_connection(ConnectionTicket(3), replacement);
        let replaced = lookup(first, None).unwrap();
        assert_eq!(replaced["state"], "cancelled");
        assert_eq!(
            replaced["result"],
            "replaced by a newer attach to the same target"
        );
        // 대체한 시도의 run은 아직 진행 중이고, 늦게 온 결과도 이미 끝난 run을 바꾸지 못한다.
        assert_eq!(lookup(second, None).unwrap()["state"], "running");
        target(engine.id, Some(10))
            .with_run(Some(first))
            .fail_run("late failure");
        assert_eq!(lookup(first, None).unwrap()["state"], "cancelled");

        let dropped = begin("attach.into_gui", None, serde_json::Value::Null);
        pending.register_connection(
            ConnectionTicket(2),
            target(engine.id, None).with_run(Some(dropped)),
        );
        pending.cancel_connection(ConnectionTicket(2), &mut remote);
        let ended = lookup(dropped, None).unwrap();
        assert_eq!(ended["state"], "cancelled");
        assert_eq!(ended["result"], "the attach attempt ended without a result");
    }

    #[test]
    fn cancelled_or_superseded_endpoint_cannot_claim_a_late_result() {
        let (_, engine) = crate::state::tests::test_state();
        let mut remote = Remote::new();
        let mut pending = MirrorAttempts::default();
        let old = remote.begin_attempt(Some(10), None).unwrap();
        pending.register_endpoint(old.clone(), target(engine.id, Some(10)));
        pending.cancel_endpoint(&old, &mut remote);
        assert!(!old.is_active());
        assert!(pending.take_endpoint(&old).is_none());

        let replaced = remote.begin_attempt(Some(10), None).unwrap();
        pending.register_endpoint(replaced.clone(), target(engine.id, Some(10)));
        let current = remote.begin_attempt(Some(10), None).unwrap();
        pending.register_endpoint(current.clone(), target(engine.id, Some(10)));
        assert!(pending.take_endpoint(&replaced).is_none());
        assert_eq!(pending.take_endpoint(&current).unwrap().engine, engine.id);
        assert!(pending.take_endpoint(&current).is_none());
        remote.finish_attempt(&current);
    }

    #[test]
    fn connection_replacement_and_failure_keep_other_targets_and_epochs() {
        let (_, first) = crate::state::tests::test_state();
        let (_, other) = crate::state::tests::test_state();
        let mut remote = Remote::new();
        let mut pending = MirrorAttempts::default();
        let (old_sender, _old_receiver) = tasty_remote::connection::channel();
        let (new_sender, _new_receiver) = tasty_remote::connection::channel();
        let mut old = target(first.id, Some(10));
        old.reconnect = Some((20, old_sender.epoch()));
        pending.register_connection(ConnectionTicket(1), old.clone());
        pending.register_connection(ConnectionTicket(2), target(other.id, Some(10)));
        let mut replacement = old.clone();
        replacement.reconnect = Some((20, new_sender.epoch()));
        pending.supersede_connections(&replacement, &mut remote);
        pending.register_connection(ConnectionTicket(3), replacement);
        assert!(!pending.installing_epoch(20, &old_sender.epoch()));
        assert!(pending.installing_epoch(20, &new_sender.epoch()));
        // A late failure for the replaced ticket cannot remove the replacement.
        pending.cancel_connection(ConnectionTicket(1), &mut remote);
        assert!(pending.installing_epoch(20, &new_sender.epoch()));
        assert_eq!(pending.connection_snapshot().len(), 2);
        pending.finish_connection(ConnectionTicket(3));
        assert!(!pending.installing_epoch(20, &new_sender.epoch()));
        let remaining = pending.connection_snapshot();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].1.engine, other.id);
    }
}
