//! 원격(mirror) explorer 의 파일 작업. 요청을 그 연결의 서버로 보내고, 서버가 보낸 진행·충돌 질문·결과를
//! 로컬 작업과 같은 진행 표시·충돌 popup·결과 카드로 보인다. 실제 작업은 서버의 작업 엔진이 한다
//! (`file_jobs::remote`).
//!
//! 연결마다 한 번에 하나를 보낸다. 나머지는 로컬 작업과 같은 대기열에서 기다리고, 로컬 작업과는 서로
//! 기다리지 않는다. 진행 표시와 충돌 popup 은 로컬 작업의 [`Shared`] 를 그대로 읽으므로, 서버가 보낸 값을
//! 이 쪽 [`Shared`] 에 옮기고 사용자의 답·취소를 꺼내 서버로 보낸다.
//!
//! 결과를 받기 전에 연결이 끊기거나 바뀌면 서버가 작업을 취소하지만, 어디까지 했는지는 알 수 없다.
//! 그때는 결과를 모른다고 알리고 원격 목록을 다시 읽게 한다.

use std::sync::Arc;

use tasty_ipc::explorer_op::{ConflictChoice, ExplorerOpMsg, PROGRESS_EVENT, RESULT_EVENT};
use tasty_ipc::stream::StreamTag;
use tasty_remote::client_session::{AttachClientSession, MirrorEvent};
use tasty_remote::connection::ConnectionEpoch;

use super::job::{Choice, Resend, Shared};
use super::{Affected, Done, Finished, Operation, Target};
use crate::app::file_jobs::remote::{OpResult, Progress, RemoteOp};

/// 결과를 기다리는 원격 작업 하나.
pub(super) struct RemoteJob {
    pub(super) op_id: u64,
    /// 작업을 보낸 mirror workspace(로컬 ID).
    pub(super) workspace: u32,
    /// 보낸 연결. 바뀌거나 끝나면 결과가 오지 않는다.
    epoch: ConnectionEpoch,
    pub(super) window: winit::window::WindowId,
    pub(super) engine: crate::runtime::engine_session::EngineId,
    pub(super) target: Target,
    affected: Affected,
    pub(super) shared: Arc<Shared>,
    undo_of: Option<super::history::UndoOf>,
    /// 보낸 경로와 남은 원본. 서버가 결과를 줄여 보내면 Retry 가 이 목록을 그대로 다시 보낸다.
    sent: Resend,
    pub(super) seen: super::ui_sync::ProgressSeen,
    cancel_sent: bool,
    result: Option<Done>,
    /// 연결이 끊겼을 때 카드에 쓸 원격 이름과 작업 동사 키. 동사가 없으면 카드 대신 오류 토스트다.
    host: String,
    verb: Option<&'static str>,
}

/// 결과를 모른 채 끝난 원격 작업. 칸에 경고 카드로 보인다.
#[derive(Debug)]
pub(crate) struct Lost {
    host: String,
    verb: &'static str,
}

impl Lost {
    pub(super) fn title(&self) -> String {
        crate::i18n::t_fmt("explorer.remote.lost_title", &self.host)
    }
    pub(super) fn body(&self) -> String {
        crate::i18n::t_fmt("explorer.remote.lost_body", crate::i18n::t(self.verb))
    }
}

/// 연결 끊김 카드 본문의 작업 동사 키. 시안이 정한 동사(복사·이동·휴지통 이동·이름 변경·새 폴더·새
/// 파일)만 카드로 보이고, 되돌리기와 남은 원본 지우기는 오류 토스트로 알린다.
fn lost_verb(operation: &Operation) -> Option<&'static str> {
    Some(match operation {
        Operation::Paste { cut: false, .. } => "explorer.remote.op_copy",
        Operation::Paste { cut: true, .. } => "explorer.remote.op_move",
        Operation::Trash(_) => "explorer.remote.op_trash",
        Operation::Rename { .. } => "explorer.menu.op_rename",
        Operation::Create { folder: true, .. } => "explorer.menu.op_new_folder",
        Operation::Create { folder: false, .. } => "explorer.menu.op_new_file",
        Operation::Undo(_)
        | Operation::RemoveLeftovers { .. }
        | Operation::Open(_)
        | Operation::Reveal(_)
        | Operation::SetPermissions { .. } => return None,
    })
}

/// 연결을 잃은 작업의 결과.
fn lost_result(host: &str, verb: Option<&'static str>) -> Done {
    match verb {
        Some(verb) => Done::Lost(Lost {
            host: host.to_string(),
            verb,
        }),
        None => Done::Simple(Err(
            crate::i18n::t("explorer.state.remote_result_unknown").to_string()
        )),
    }
}

/// 작업이 보내는 경로와 남은 원본.
fn sent_of(operation: &Operation) -> Resend {
    match operation {
        Operation::Paste { paths, .. } | Operation::Trash(paths) => Resend {
            paths: paths.clone(),
            leftovers: Vec::new(),
        },
        Operation::RemoveLeftovers { leftovers, .. } => Resend {
            paths: Vec::new(),
            leftovers: leftovers.clone(),
        },
        _ => Resend::default(),
    }
}

/// 원격으로 보낼 작업. 시스템 열기는 원격 파일에 하지 않는다.
fn remote_op(operation: Operation) -> Option<RemoteOp> {
    Some(match operation {
        Operation::Paste {
            paths,
            destination,
            cut,
        } => RemoteOp::Paste {
            paths,
            destination,
            cut,
        },
        Operation::Trash(paths) => RemoteOp::Trash { paths },
        Operation::Rename { path, name } => RemoteOp::Rename { path, name },
        Operation::Create { dir, name, folder } => RemoteOp::Create { dir, name, folder },
        Operation::Undo(steps) => RemoteOp::Undo { steps },
        Operation::RemoveLeftovers { dest, leftovers } => {
            RemoteOp::RemoveLeftovers { dest, leftovers }
        }
        Operation::Open(_) | Operation::Reveal(_) | Operation::SetPermissions { .. } => {
            return None;
        }
    })
}

fn wire_choice(choice: Choice) -> ConflictChoice {
    match choice {
        Choice::KeepBoth => ConflictChoice::KeepBoth,
        Choice::Skip => ConflictChoice::Skip,
        Choice::Replace => ConflictChoice::Replace,
        Choice::CancelRest => ConflictChoice::CancelRest,
    }
}

/// `explorer_op_progress`·`explorer_op_result` 제어 프레임을 mirror 이벤트로 읽는다.
pub(crate) fn mirror_event(payload: &[u8]) -> Option<MirrorEvent> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let event = value.get("event").and_then(|v| v.as_str())?;
    (event == PROGRESS_EVENT || event == RESULT_EVENT).then_some(MirrorEvent::ExplorerOp(value))
}

/// 서버 통지 하나.
enum Incoming {
    Progress(Progress),
    Finished(crate::app::file_jobs::remote::Finished),
}

fn parse(value: serde_json::Value) -> Option<Incoming> {
    match value.get("event").and_then(|v| v.as_str())? {
        PROGRESS_EVENT => serde_json::from_value(value).ok().map(Incoming::Progress),
        RESULT_EVENT => serde_json::from_value(value).ok().map(Incoming::Finished),
        _ => None,
    }
}

/// 서버 결과를 로컬 작업의 결과 모양으로 바꾼다. 줄인 결과는 되돌리기 단계가 이미 빠져 있고 실패 목록도
/// 일부라서, Retry 는 줄인 목록 대신 보냈던 목록(`sent`)을 다시 보낸다.
fn done_of(result: OpResult, sent: &Resend) -> Done {
    match result {
        OpResult::Report {
            mut report,
            truncated,
        } => {
            if truncated {
                report.resend = Some(Box::new(sent.clone()));
            }
            Done::Report(report)
        }
        OpResult::Simple { error } => Done::Simple(error.map_or(Ok(()), Err)),
        OpResult::Refused { reason } => {
            tracing::warn!(?reason, "remote explorer op: 서버가 작업을 거절했다");
            Done::Simple(Err(
                crate::i18n::t("explorer.state.remote_refused").to_string()
            ))
        }
    }
}

fn send(sess: &AttachClientSession, msg: &ExplorerOpMsg) -> bool {
    let payload = serde_json::to_vec(msg).unwrap_or_default();
    match sess.send_frame(StreamTag::Control, payload) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("remote explorer op: 전송 큐가 닫혀 보내지 못했다: {e}");
            false
        }
    }
}

/// 원격 작업을 시작하는 데 필요한 요청 정보.
pub(super) struct Start {
    pub(super) window: winit::window::WindowId,
    pub(super) engine: crate::runtime::engine_session::EngineId,
    pub(super) workspace: u32,
    pub(super) target: Target,
    pub(super) operation: Operation,
}

impl super::ExplorerFiles {
    /// 이 mirror workspace 에서 결과를 기다리는 작업이 있는가.
    pub(super) fn remote_busy(&self, workspace: u32) -> bool {
        self.remote.iter().any(|j| j.workspace == workspace)
    }

    /// 요청을 서버로 보낸다. 보내지 못하면 결과 없는 실패로 돌려준다.
    pub(super) fn start_remote(
        &mut self,
        sessions: &[AttachClientSession],
        start: Start,
        shared: Arc<Shared>,
    ) -> Result<(), Target> {
        let Start {
            window,
            engine,
            workspace,
            target,
            operation,
        } = start;
        let affected = operation.affected();
        let undo_of = operation.undo_of();
        let sent = sent_of(&operation);
        let verb = lost_verb(&operation);
        let Some(op) = remote_op(operation) else {
            return Err(target);
        };
        let Some(sess) = sessions
            .iter()
            .find(|s| s.state.local_workspace == workspace)
        else {
            return Err(target);
        };
        self.next_op += 1;
        let op_id = self.next_op;
        let msg = ExplorerOpMsg::ExplorerOpRequest {
            op_id,
            op: serde_json::to_value(&op).unwrap_or_default(),
        };
        if !send(sess, &msg) {
            return Err(target);
        }
        self.remote.push(RemoteJob {
            op_id,
            workspace,
            epoch: sess.transport.frame_tx.epoch(),
            window,
            engine,
            target,
            affected,
            shared,
            undo_of,
            sent,
            seen: Default::default(),
            cancel_sent: false,
            result: None,
            host: sess.state.host_text(),
            verb,
        });
        Ok(())
    }

    /// 서버 통지를 반영하고, 사용자의 답·취소를 보내고, 끝났거나 연결을 잃은 작업을 꺼낸다.
    pub(super) fn poll_remote(&mut self, sessions: &mut [AttachClientSession]) -> Vec<Finished> {
        for sess in sessions.iter_mut() {
            let workspace = sess.state.local_workspace;
            for value in std::mem::take(&mut sess.transport.explorer_inbox) {
                let Some(incoming) = parse(value) else {
                    tracing::warn!("remote explorer op: 읽을 수 없는 통지를 버렸다");
                    continue;
                };
                let op_id = match &incoming {
                    Incoming::Progress(p) => p.op_id,
                    Incoming::Finished(f) => f.op_id,
                };
                let Some(job) = self
                    .remote
                    .iter_mut()
                    .find(|j| j.workspace == workspace && j.op_id == op_id)
                else {
                    continue;
                };
                match incoming {
                    Incoming::Progress(p) => job.shared.mirror(&p.progress, p.ask),
                    Incoming::Finished(f) => job.result = Some(done_of(f.result, &job.sent)),
                }
            }
        }
        for job in self.remote.iter_mut().filter(|j| j.result.is_none()) {
            let sess = sessions.iter().find(|s| {
                s.state.local_workspace == job.workspace
                    && job.epoch.is_active()
                    && job.epoch.same(&s.transport.frame_tx.epoch())
            });
            let Some(sess) = sess else {
                job.result = Some(lost_result(&job.host, job.verb));
                continue;
            };
            if let Some(answer) = job.shared.take_answer() {
                send(
                    sess,
                    &ExplorerOpMsg::ExplorerOpAnswer {
                        op_id: job.op_id,
                        choice: wire_choice(answer.choice),
                        apply_all: answer.apply_all,
                    },
                );
            }
            if job.shared.cancelled() && !job.cancel_sent {
                job.cancel_sent = send(sess, &ExplorerOpMsg::ExplorerOpCancel { op_id: job.op_id });
            }
        }
        let (done, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.remote)
            .into_iter()
            .partition(|j| j.result.is_some());
        self.remote = waiting;
        done.into_iter()
            .map(|job| Finished {
                window: job.window,
                engine: job.engine,
                remote: Some(job.workspace),
                target: job.target,
                affected: job.affected,
                shared: job.shared,
                undo_of: job.undo_of,
                recorded: None,
                result: job.result.expect("partitioned on a result"),
            })
            .collect()
    }
}

impl crate::app::App {
    /// 연결마다 원격 쓰기를 받는지 engine 에 맞춘다. 화면의 메뉴·단축키·끌기가 이 값을 본다.
    pub(super) fn sync_remote_write_support(&mut self) {
        let writable: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .filter(|s| {
                s.transport.explorer_write
                    && s.state.phase == tasty_remote::client_session::SessionState::Connected
                    && !s
                        .transport
                        .disconnected
                        .load(std::sync::atomic::Ordering::Acquire)
            })
            .map(|s| s.state.local_workspace)
            .collect();
        // 붙은 원격의 이름과, 연결돼 있지만 원격 쓰기를 알리지 않은 옛 서버인지.
        let hosts: Vec<(u32, crate::remote::state::ExplorerRemoteHost)> = self
            .remote
            .sessions
            .iter()
            .map(|s| {
                let connected = s.state.phase
                    == tasty_remote::client_session::SessionState::Connected
                    && !s
                        .transport
                        .disconnected
                        .load(std::sync::atomic::Ordering::Acquire);
                (
                    s.state.local_workspace,
                    crate::remote::state::ExplorerRemoteHost {
                        label: s.state.host_text(),
                        needs_update: connected && !s.transport.explorer_write,
                    },
                )
            })
            .collect();
        let hosts_of = |core: &crate::core::CoreState| {
            hosts
                .iter()
                .filter(|(ws, _)| core.find_workspace_index_for_id(*ws).is_some())
                .cloned()
                .collect::<std::collections::HashMap<_, _>>()
        };
        let mine = |core: &crate::core::CoreState| -> std::collections::HashSet<u32> {
            writable
                .iter()
                .copied()
                .filter(|ws| core.find_workspace_index_for_id(*ws).is_some())
                .collect()
        };
        for (_, main, engine) in self.engines_mut().window_pairs() {
            let set = mine(engine.core);
            let hosts = hosts_of(engine.core);
            main.state.explorer_views.set_remote_writable(&set, &hosts);
            engine.remote.explorer_writable = set;
            engine.remote.explorer_hosts = hosts;
        }
        for (state, engine) in self.engines_mut().parked_sessions() {
            let set = mine(engine.core);
            let hosts = hosts_of(engine.core);
            state.explorer_views.set_remote_writable(&set, &hosts);
            engine.remote.explorer_writable = set;
            engine.remote.explorer_hosts = hosts;
        }
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
