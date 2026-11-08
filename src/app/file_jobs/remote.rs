//! 원격 explorer 파일 작업. attach holder 가 보낸 작업을 서버가 이 엔진으로 실행하고, 진행·충돌 질문·
//! 결과를 같은 연결로 돌려준다. client 는 받은 값을 로컬 작업과 같은 진행 표시·충돌 popup·결과 카드로 보인다.
//!
//! 권한은 원격 디렉터리 조회와 같다. 요청한 연결이 이 서버의 workspace 를 점유하고 있으면 받고, 경로를
//! 점유 workspace 안으로 제한하지 않는다. 신뢰 경계는 attach 연결을 연 SSH 다. 서버 프로세스 사용자의
//! 파일 권한이 그대로 적용되고 권한 거부는 항목별 실패로 돌아간다.
//!
//! 연결마다 한 번에 하나만 실행한다. client 는 결과를 받은 뒤 다음 요청을 보낸다. 연결이 끊기면 실행 중인
//! 작업을 취소한다. 엔진의 취소 규칙대로 지금 항목에서 멈추고 준비 중 사본은 지운다.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tasty_ipc::explorer_op::{
    ConflictChoice, ExplorerOpMsg, PROGRESS_EVENT, RESULT_EVENT, RefusedOp,
};
use tasty_ipc::stream::{StreamFrame, StreamTag};
use tasty_ipc::stream_hub::{StreamClientId, StreamHub};

use super::job::leftover::Leftover;
use super::job::{self, Answer, Ask, Choice, Report, Shared, Snapshot, UndoStep};
use super::ops;

/// 결과 한 건의 JSON 상한. Control 프레임 상한(1 MiB)보다 여유를 둔다. 목록 조회와 같은 값이다.
const RESULT_BUDGET: usize = 700 * 1024;
/// 서버가 내려갈 때 취소한 작업이 멈추기를 기다리는 시간. 엔진은 복사 한 조각마다 취소를 본다.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// 원격에서 실행하는 작업. 로컬 explorer 의 작업 중 서버 파일을 바꾸는 것만 있다.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum RemoteOp {
    Paste {
        paths: Vec<PathBuf>,
        destination: PathBuf,
        cut: bool,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    Rename {
        path: PathBuf,
        name: String,
    },
    /// 새 폴더나 빈 파일. 같은 이름이 있으면 덮어쓰지 않고 실패한다.
    Create {
        dir: PathBuf,
        name: String,
        folder: bool,
    },
    Undo {
        steps: Vec<UndoStep>,
    },
    RemoveLeftovers {
        dest: Option<PathBuf>,
        leftovers: Vec<Leftover>,
    },
}

/// `explorer_op_result` 가 싣는 결과.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpResult {
    /// 항목별 결과. `truncated` 면 결과가 커서 되돌리기 단계·남은 원본을 빼고 목록을 줄였다.
    Report { report: Report, truncated: bool },
    /// 이름 변경·새 항목. 실패면 OS 오류 문구다.
    Simple { error: Option<String> },
    /// 작업을 시작하지 않았다.
    Refused { reason: RefusedOp },
}

/// 서버 → client 진행 통지.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Progress {
    pub op_id: u64,
    pub progress: Snapshot,
    /// 답을 기다리는 충돌. 없으면 None.
    pub ask: Option<Ask>,
}

/// 서버 → client 결과.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Finished {
    pub op_id: u64,
    pub result: OpResult,
}

fn choice_of(choice: ConflictChoice) -> Choice {
    match choice {
        ConflictChoice::KeepBoth => Choice::KeepBoth,
        ConflictChoice::Skip => Choice::Skip,
        ConflictChoice::Replace => Choice::Replace,
        ConflictChoice::CancelRest => Choice::CancelRest,
    }
}

impl RemoteOp {
    fn run(self, shared: &Shared) -> OpResult {
        let report = |report| OpResult::Report {
            report,
            truncated: false,
        };
        let simple = |result: std::io::Result<()>| OpResult::Simple {
            error: result.err().map(|e| e.to_string()),
        };
        match self {
            Self::Paste {
                paths,
                destination,
                cut,
            } => report(job::run_transfer(shared, &paths, &destination, cut)),
            Self::Trash { paths } => report(job::run_trash(shared, &paths)),
            Self::Undo { steps } => report(job::run_undo(shared, &steps)),
            Self::RemoveLeftovers { dest, leftovers } => report(
                job::leftover::run_remove_leftovers(shared, dest, &leftovers),
            ),
            Self::Rename { path, name } => simple(ops::rename_entry(&path, &name)),
            Self::Create { dir, name, folder } => simple(ops::create_entry(&dir, &name, folder)),
        }
    }
}

/// 결과 통지. 예산을 넘으면 되돌리기 단계·남은 원본을 빼고 항목 목록을 반씩 줄인다.
/// 줄인 결과로 Undo·원본 지우기를 보내면 일부만 처리되므로 client 는 그 결과를 이력과 Undo 에 쓰지 않는다.
fn result_message(op_id: u64, result: OpResult) -> Vec<u8> {
    let encode = |result: OpResult| {
        let mut value = serde_json::to_value(Finished { op_id, result }).unwrap_or_default();
        if let Some(map) = value.as_object_mut() {
            map.insert("event".into(), RESULT_EVENT.into());
        }
        serde_json::to_vec(&value).unwrap_or_default()
    };
    let OpResult::Report { mut report, .. } = result else {
        return encode(result);
    };
    let whole = encode(OpResult::Report {
        report: report.clone(),
        truncated: false,
    });
    if whole.len() <= RESULT_BUDGET {
        return whole;
    }
    report.undo.clear();
    report.leftovers.clear();
    loop {
        let bytes = encode(OpResult::Report {
            report: report.clone(),
            truncated: true,
        });
        let lists = report.failed.len()
            + report.skipped.len()
            + report.unprocessed.len()
            + report.staging_left.len();
        if bytes.len() <= RESULT_BUDGET || lists == 0 {
            return bytes;
        }
        report.failed.truncate(report.failed.len() / 2);
        report.skipped.truncate(report.skipped.len() / 2);
        report.unprocessed.truncate(report.unprocessed.len() / 2);
        report.staging_left.truncate(report.staging_left.len() / 2);
    }
}

fn progress_message(op_id: u64, shared: &Shared) -> Vec<u8> {
    let mut value = serde_json::to_value(Progress {
        op_id,
        progress: shared.snapshot(),
        ask: shared.pending_ask(),
    })
    .unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("event".into(), PROGRESS_EVENT.into());
    }
    serde_json::to_vec(&value).unwrap_or_default()
}

fn send(hub: &StreamHub, client: StreamClientId, payload: Vec<u8>) {
    // 손실·끊김은 허브가 처리한다. 끊긴 client 의 작업은 끊김 통지로 취소한다.
    let _ = hub.push(client, StreamFrame::new(StreamTag::Control, payload));
}

struct Running {
    op_id: u64,
    shared: Arc<Shared>,
}

type RunningMap = Arc<Mutex<HashMap<StreamClientId, Running>>>;

/// 서버 쪽 실행 상태. 연결마다 실행 중인 작업 하나를 기억한다.
#[derive(Default)]
pub(crate) struct RemoteJobs {
    running: RunningMap,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

/// 실행 상태는 단순 값이라 작업 스레드가 패닉해도 그대로 읽는다. 처음 한 번만 보고한다.
static POISON_REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    tasty_utils::poison::recover_mutex(m.lock(), "remote explorer jobs", &POISON_REPORTED)
}

impl RemoteJobs {
    /// client 의 요청·답·취소를 처리한다. `holder` 는 그 연결이 이 서버의 workspace 를 점유하는지다.
    pub(crate) fn handle(
        &self,
        hub: &StreamHub,
        client: StreamClientId,
        holder: bool,
        msg: ExplorerOpMsg,
    ) {
        match msg {
            ExplorerOpMsg::ExplorerOpRequest { op_id, op } => {
                self.start(hub, client, holder, op_id, op);
            }
            ExplorerOpMsg::ExplorerOpAnswer {
                op_id,
                choice,
                apply_all,
            } => {
                if let Some(shared) = self.shared(client, op_id) {
                    shared.answer(Answer {
                        choice: choice_of(choice),
                        apply_all,
                    });
                }
            }
            ExplorerOpMsg::ExplorerOpCancel { op_id } => {
                if let Some(shared) = self.shared(client, op_id) {
                    shared.cancel();
                }
            }
        }
    }

    /// 끊긴 연결의 작업을 멈춘다.
    pub(crate) fn forget_client(&self, client: StreamClientId) {
        if let Some(running) = lock(&self.running).get(&client) {
            running.shared.cancel();
        }
    }

    fn shared(&self, client: StreamClientId, op_id: u64) -> Option<Arc<Shared>> {
        lock(&self.running)
            .get(&client)
            .filter(|r| r.op_id == op_id)
            .map(|r| Arc::clone(&r.shared))
    }

    fn start(
        &self,
        hub: &StreamHub,
        client: StreamClientId,
        holder: bool,
        op_id: u64,
        op: serde_json::Value,
    ) {
        let refuse = |reason| {
            send(
                hub,
                client,
                result_message(op_id, OpResult::Refused { reason }),
            );
        };
        if !holder {
            refuse(RefusedOp::NotHolder);
            return;
        }
        let Ok(op) = serde_json::from_value::<RemoteOp>(op) else {
            refuse(RefusedOp::Invalid);
            return;
        };
        let mut running = lock(&self.running);
        if running.contains_key(&client) {
            drop(running);
            refuse(RefusedOp::Busy);
            return;
        }
        let me: Arc<OnceLock<Weak<Shared>>> = Arc::default();
        let wake = {
            let (hub, me) = (hub.clone(), Arc::clone(&me));
            move || {
                if let Some(shared) = me.get().and_then(Weak::upgrade) {
                    send(&hub, client, progress_message(op_id, &shared));
                }
            }
        };
        let shared = Arc::new(Shared::interactive(wake));
        me.get_or_init(|| Arc::downgrade(&shared));
        running.insert(
            client,
            Running {
                op_id,
                shared: Arc::clone(&shared),
            },
        );
        drop(running);
        let (hub_w, map) = (hub.clone(), Arc::clone(&self.running));
        let spawned = std::thread::Builder::new()
            .name("explorer-remote-op".into())
            .spawn(move || {
                let result = op.run(&shared);
                // 다음 요청이 바쁨으로 거절되지 않게 결과를 보내기 전에 지운다.
                let mut running = lock(&map);
                if running.get(&client).is_some_and(|r| r.op_id == op_id) {
                    running.remove(&client);
                }
                drop(running);
                send(&hub_w, client, result_message(op_id, result));
            });
        match spawned {
            Ok(worker) => {
                let mut workers = lock(&self.workers);
                workers.retain(|w| !w.is_finished());
                workers.push(worker);
            }
            Err(error) => {
                tracing::warn!(%error, "remote explorer worker spawn failed");
                lock(&self.running).remove(&client);
                refuse(RefusedOp::Unavailable);
            }
        }
    }
}

impl Drop for RemoteJobs {
    fn drop(&mut self) {
        for running in lock(&self.running).values() {
            running.shared.cancel();
        }
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        let workers = std::mem::take(&mut *lock(&self.workers));
        for worker in workers {
            while !worker.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if worker.is_finished() {
                if worker.join().is_err() {
                    tracing::warn!("remote explorer worker panicked");
                }
            } else {
                tracing::warn!("remote explorer worker still running at shutdown");
            }
        }
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    /// 스트림 수신을 분류하고 원격 explorer 파일 작업을 처리한다. 나머지는 `apply_stream_outcome` 이 맡는다.
    /// 점유 확인은 디렉터리 조회와 같다. 끊긴 연결의 작업은 취소한다.
    /// 한 회차에 `budget`개까지만 꺼낸다. 두 번째 값은 예산에 닿았는지다.
    pub(crate) fn pump_stream_inbound(
        &mut self,
        budget: usize,
    ) -> (tasty_ipc::stream_hub::PumpOutcome, bool) {
        let (mut outcome, cut) = self
            .stream_hub
            .pump_inbound_budgeted(&self.stream_inbound_rx, budget);
        for (client_id, msg) in std::mem::take(&mut outcome.explorer_ops) {
            let holder = self
                .engines()
                .windowed_and_parked()
                .any(|engine| engine.live.occupancy.client_holds_workspace(client_id));
            self.remote_file_jobs
                .handle(&self.stream_hub, client_id, holder, msg);
        }
        for &client_id in &outcome.disconnected {
            self.remote_file_jobs.forget_client(client_id);
            crate::remote::agent_children::disconnect(client_id);
        }
        let agent = std::mem::take(&mut outcome.agent_children);
        if !agent.is_empty() {
            // 엔진을 읽는 동안 plugin 호출을 넘겨야 해 관리자를 잠시 꺼낸다. 이 안에서 다른 경로는 관리자를 쓰지 않는다.
            let mut plugins = self.plugin_manager.take();
            {
                let engines = self.engines();
                let engines: Vec<_> = engines.windowed_and_parked().collect();
                crate::remote::agent_children::handle(
                    &self.stream_hub,
                    plugins.as_mut(),
                    &engines,
                    agent,
                );
            }
            self.plugin_manager = plugins;
        }
        (outcome, cut)
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
