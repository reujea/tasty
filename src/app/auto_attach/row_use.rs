//! 연결되지 않은 저장된 원격 행을 쓰려는 IPC·plugin 요청을 연결될 때까지 보류한다.
//! 연결되면 같은 요청을 이어서 실행하고, 실패하거나 상한을 넘으면 이유를 담은 오류로 답한다.
//! 연결을 일으키는 메서드와 일으키지 않는 조회는 docs/features/remote-attach/index.md의
//! "저장된 원격 행을 쓰는 요청" 절에 있다.

use std::time::{Duration, Instant};

use tasty_remote::client_session::SessionState;

use crate::app::App;
use crate::app::request_gate::Gated;
use crate::core::request_target::{Kind, ResourceId};
use crate::ipc::protocol::JsonRpcRequest;

/// 연결을 기다리는 상한. 원격 자식 call의 응답 상한과 같다.
pub(crate) const ROW_WAIT_LIMIT: Duration = crate::remote::agent_children::client::CALL_REPLY_LIMIT;

/// 연결을 기다리는 동안 잡아 둔 요청 수의 상한. 넘으면 기다리지 않고 거절한다.
const ROW_WAIT_CAP: usize = 64;

/// 연결하지 못한 행을 대상으로 한 요청의 오류 코드.
pub(crate) const ERR_ROW_UNAVAILABLE: i32 = -32000;

struct RowWait {
    gated: Gated,
    row: u32,
    deadline: Instant,
}

/// 저장된 원격 행의 연결을 기다리는 요청들.
#[derive(Default)]
pub(crate) struct RowWaits {
    waiting: Vec<RowWait>,
    /// 진행 중인 행 연결에 합류한 `remote.attach`의 run. 행이 연결되거나 연결을 끝내면 그 결과를 남긴다.
    runs: Vec<(u32, u64)>,
}

impl RowWaits {
    pub(crate) fn is_empty(&self) -> bool {
        self.waiting.is_empty() && self.runs.is_empty()
    }

    /// 진행 중인 행 연결에 run을 합류시킨다. 행이 연결되거나 연결을 끝내면 결과가 남는다.
    pub(crate) fn join_run(&mut self, row: u32, run: Option<u64>) {
        self.runs.extend(run.map(|run| (row, run)));
    }

    /// 재연결 타이머와 함께 깨울 시각. 행마다 가장 이른 상한이다.
    pub(crate) fn deadlines(&self) -> Vec<(u32, Instant)> {
        let mut out: Vec<(u32, Instant)> = Vec::new();
        for wait in &self.waiting {
            match out.iter_mut().find(|(row, _)| *row == wait.row) {
                Some((_, at)) => *at = (*at).min(wait.deadline),
                None => out.push((wait.row, wait.deadline)),
            }
        }
        out
    }

    /// 종료가 시작되면 기다리던 요청에 종료 응답을 보낸다.
    pub(crate) fn reject_all(&mut self, mut plugins: Option<&mut crate::plugin::PluginManager>) {
        for (_, run) in self.runs.drain(..) {
            crate::request_runs::finish(
                run,
                crate::request_runs::RunState::Failed,
                "host is shutting down",
                serde_json::Value::Null,
            );
        }
        for wait in self.waiting.drain(..) {
            answer(
                wait.gated,
                ERR_ROW_UNAVAILABLE,
                "host is shutting down".to_string(),
                None,
                plugins.as_deref_mut(),
            );
        }
    }
}

/// 요청을 지금 실행할지, 연결을 기다릴지, 바로 거절할지.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RowUse {
    Pass,
    Wait(u32),
    Refuse(RowRefusal),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RowRefusal {
    pub(crate) row: u32,
    pub(crate) reason: String,
    pub(crate) message: String,
}

/// 원격 행을 쓰려는 것으로 보고 연결을 일으키는 메서드.
/// 조회·로컬 메타·행 정보 변경은 연결하지 않는다.
fn connects(method: &str) -> bool {
    matches!(
        crate::ipc::alias::canonicalize(method),
        "terminal.spawn"
            | "attach.agent_target"
            | "surface.send"
            | "surface.send_key"
            | "surface.send_combo"
            | "surface.send_to"
            | "surface.send_wait_idle"
            | "terminal.tell"
            | "surface.screen_text"
            | "surface.read_since_mark"
            | "surface.set_mark"
            | "split"
            | "tab.create"
            | "tab.move"
            | "tab.close"
            | "pane.close"
            | "surface.close"
            | "surface.close_self"
            | "terminal.kill"
            | "image.open"
            | "markdown.navigate"
            | "git_viewer.query"
            | "file_handler.dispatch"
    )
}

/// `terminal.spawn`·`attach.agent_target`의 workspace 인자. 숫자 문자열도 받는다.
fn workspace_param(request: &JsonRpcRequest) -> Option<u32> {
    let value = request.params.get("workspace")?;
    value
        .as_u64()
        .and_then(|id| u32::try_from(id).ok())
        .or_else(|| value.as_str()?.parse().ok())
}

fn notice_reason(kind: tasty_remote::refusal::RowNoticeKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unresolved".to_owned())
}

fn refusal(row: u32, reason: &str) -> RowRefusal {
    let message = match reason {
        "window_parked" => "the window that holds this remote workspace is closed; it does not connect until the window is open again".to_owned(),
        "user_disconnected" => "this remote workspace was disconnected by the user; connect it from the remote workspace row".to_owned(),
        "connect_timeout" => format!(
            "this remote workspace did not connect within {} seconds",
            ROW_WAIT_LIMIT.as_secs()
        ),
        "busy" => "too many requests are waiting for remote workspaces to connect".to_owned(),
        other => format!("this remote workspace could not be connected ({other})"),
    };
    RowRefusal {
        row,
        reason: reason.to_owned(),
        message,
    }
}

fn answer(
    gated: Gated,
    code: i32,
    message: String,
    data: Option<serde_json::Value>,
    plugins: Option<&mut crate::plugin::PluginManager>,
) {
    match gated {
        Gated::Ipc { cmd, .. } => {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response = match data {
                Some(data) => {
                    crate::ipc::protocol::JsonRpcResponse::error_with_data(id, code, message, data)
                }
                None => crate::ipc::protocol::JsonRpcResponse::error(id, code, message),
            };
            crate::ipc::server::send_response(&cmd.response_tx, response);
        }
        Gated::Plugin { call, .. } => {
            if let Some(manager) = plugins {
                manager.send_plugin_call_result(&call, None, Some(message), Some(code));
            }
        }
    }
}

/// 기다리는 동안의 행 상태.
#[derive(Debug, PartialEq, Eq)]
enum Progress {
    Ready,
    Waiting,
    Failed(String),
}

/// 판정에 쓰는 행의 사실.
struct RowFacts {
    row: u32,
    saved: bool,
    phase: Option<SessionState>,
    /// 세션 소켓이 살아 있는지. 재동기화(Resync) 중이면 단계가 Connected여도 거짓이다.
    live: bool,
    /// 이 행의 연결 시도가 진행 중인지.
    active: bool,
    has_window: bool,
    user_disconnected: bool,
    notice: Option<tasty_remote::refusal::RowNoticeKind>,
    waiting: usize,
}

impl RowFacts {
    /// 화면의 "연결됨"과 같은 판정이다. 재동기화 중인 행은 연결됐다고 보지 않는다.
    fn connected(&self) -> bool {
        self.phase == Some(SessionState::Connected) && self.live
    }
}

/// 연결을 일으키는 요청의 처리. `Wait`이면 호출자가 연결을 시작한다.
fn decide(facts: RowFacts) -> RowUse {
    let row = facts.row;
    if !facts.saved || facts.connected() {
        return RowUse::Pass;
    }
    if !facts.has_window {
        return RowUse::Refuse(refusal(row, "window_parked"));
    }
    // 에이전트가 그 행을 쓰는 것도 그 행을 여는 것과 같이 본다(N2).
    if super::user_disconnect_holds(facts.user_disconnected, true) {
        return RowUse::Refuse(refusal(row, "user_disconnected"));
    }
    if facts.waiting >= ROW_WAIT_CAP {
        return RowUse::Refuse(refusal(row, "busy"));
    }
    RowUse::Wait(row)
}

/// 보류한 요청의 행이 지금 어디에 있는지.
fn progress(facts: &RowFacts) -> Progress {
    if !facts.saved {
        // 행이 닫혔으면 요청을 그대로 실행해 "대상 없음"으로 답하게 한다.
        return Progress::Ready;
    }
    if !facts.has_window {
        return Progress::Failed("window_parked".into());
    }
    if facts.connected() {
        return Progress::Ready;
    }
    if facts.active
        || matches!(
            facts.phase,
            Some(SessionState::Reconnecting | SessionState::Connected)
        )
    {
        return Progress::Waiting;
    }
    if facts.user_disconnected {
        return Progress::Failed("user_disconnected".into());
    }
    Progress::Failed(
        facts
            .notice
            .map_or_else(|| "unresolved".to_owned(), notice_reason),
    )
}

/// 그 행을 가진 engine에 창이 있는지. parked engine에는 mirror를 설치할 View가 없다.
fn row_has_window(engines: &crate::app::engine_registry::EngineRegistry, row: u32) -> bool {
    engines.all_sessions().any(|session| {
        session.core_state.has_workspace(row) && engines.window_of(session.id).is_some()
    })
}

impl App {
    /// 요청이 가리키는 workspace. surface·pane·tab은 그것을 가진 workspace로 바꾼다.
    fn request_row_target(&self, request: &JsonRpcRequest) -> Option<u32> {
        let resource = crate::core::request_target::request_resource_id(
            crate::ipc::alias::canonicalize(&request.method),
            &request.params,
        );
        let resource = match resource {
            Some(resource) => resource,
            None => {
                return workspace_param(request);
            }
        };
        let id = u32::try_from(resource.id).ok()?;
        self.engines.all_sessions().find_map(|session| {
            let core = &session.core_state;
            let workspace_of_pane = |pane: u32| {
                core.find_workspace_index_for_pane(pane)
                    .and_then(|index| core.workspace_at(index))
                    .map(|ws| ws.id)
            };
            match resource {
                ResourceId {
                    kind: Kind::Workspace,
                    ..
                } => core.has_workspace(id).then_some(id),
                ResourceId {
                    kind: Kind::Surface,
                    ..
                } => core
                    .find_workspace_index_for_surface(id)
                    .and_then(|(index, _)| core.workspace_at(index))
                    .map(|ws| ws.id),
                ResourceId {
                    kind: Kind::Pane, ..
                } => workspace_of_pane(id),
                ResourceId {
                    kind: Kind::Tab, ..
                } => core.find_pane_for_tab(id).and_then(workspace_of_pane),
                _ => None,
            }
        })
    }

    fn row_has_window(&self, row: u32) -> bool {
        row_has_window(&self.engines, row)
    }

    /// 요청이 연결되지 않은 저장된 원격 행을 쓰려는지 판정하고, 그렇다면 연결을 시작한다.
    pub(crate) fn remote_row_use(&mut self, request: &JsonRpcRequest) -> RowUse {
        if !connects(&request.method) {
            return RowUse::Pass;
        }
        let Some(row) = self.request_row_target(request) else {
            return RowUse::Pass;
        };
        let facts = self.row_facts(row);
        // 재동기화 중인 행은 이미 다시 붙는 중이라 새 연결을 시작하지 않고 기다린다.
        let resyncing = facts.phase == Some(SessionState::Connected);
        let decision = decide(facts);
        if decision == RowUse::Wait(row) && !resyncing {
            self.connect_saved_row(row, false);
        }
        decision
    }

    fn row_facts(&self, row: u32) -> RowFacts {
        RowFacts {
            row,
            saved: self.saved_row_binding(row).is_some(),
            phase: self.row_session_phase(row),
            live: self.row_link_live(row),
            active: self.remote.active.contains(&row),
            has_window: self.row_has_window(row),
            user_disconnected: self.remote.user_disconnected.contains(&row),
            notice: self.remote.refusals.notice(row).map(|notice| notice.kind),
            waiting: self.row_waits.waiting.len(),
        }
    }

    /// 판정 결과대로 요청을 보관하거나 거절한다. `Pass`는 호출자가 그대로 실행한다.
    pub(crate) fn hold_for_remote_row(&mut self, gated: Gated, decision: RowUse) {
        match decision {
            RowUse::Pass => {
                tracing::error!("a request that needs no remote row connection was held");
                self.resume_row_request(gated);
            }
            RowUse::Wait(row) => {
                self.row_waits.waiting.push(RowWait {
                    gated,
                    row,
                    deadline: Instant::now() + ROW_WAIT_LIMIT,
                });
            }
            RowUse::Refuse(refusal) => {
                let data = serde_json::json!({
                    "reason": refusal.reason,
                    "workspace_id": refusal.row,
                });
                answer(
                    gated,
                    ERR_ROW_UNAVAILABLE,
                    refusal.message,
                    Some(data),
                    self.plugin_manager.as_mut(),
                );
            }
        }
    }

    /// `remote.attach`가 같은 연결 대상의 저장된 원격 행을 가리키면 새 mirror를 만들지 않고 그 행을
    /// 연결한다. 연결된 행이면 바로 성공으로 끝난 run을 준다. 이미 연결 중인 행에 합류하면 그 시도가
    /// 끝날 때 run에 같은 결과를 남긴다.
    pub(crate) fn attach_to_saved_row(
        &mut self,
        cmd: &crate::ipc::server::IpcCommand,
        caller: &crate::ipc::caller::CallerContext,
        row: u32,
        remote_ws: u32,
    ) {
        use crate::ipc::protocol::JsonRpcResponse;
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let refused = if !self.row_has_window(row) {
            Some("window_parked")
        } else if super::user_disconnect_holds(self.remote.user_disconnected.contains(&row), true) {
            Some("user_disconnected")
        } else {
            None
        };
        if let Some(reason) = refused {
            let refusal = refusal(row, reason);
            let data = serde_json::json!({"reason": refusal.reason, "workspace_id": row});
            crate::ipc::server::send_response(
                &cmd.response_tx,
                JsonRpcResponse::error_with_data(id, ERR_ROW_UNAVAILABLE, refusal.message, data),
            );
            return;
        }
        let connected = self.row_facts(row).connected();
        let joining = !connected && self.remote.active.contains(&row);
        let run = crate::request_runs::begin(
            "remote.attach",
            crate::request_runs::reader_of(caller),
            serde_json::json!({ "remote_workspace": remote_ws, "workspace_id": row }),
        );
        if connected {
            crate::request_runs::finish(
                run,
                crate::request_runs::RunState::Succeeded,
                format!("already attached as workspace {row}"),
                serde_json::json!({ "workspace_id": row }),
            );
        } else if joining {
            // 진행 중인 시도의 최종 결과를 이 run에도 남긴다.
            self.row_waits.join_run(row, Some(run));
        } else {
            self.connect_saved_row_with_run(row, false, Some(run));
        }
        crate::ipc::server::send_response(
            &cmd.response_tx,
            JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "attaching": !connected,
                    "remote_workspace": remote_ws,
                    "workspace_id": row,
                    "saved_row": true,
                    "run": run,
                }),
            ),
        );
    }

    /// 저장된 원격 행의 연결 상태 이름. `workspace.list`의 `remote_state`다.
    fn row_state_name(&self, row: u32) -> &'static str {
        match self.row_session_phase(row) {
            // 손실 뒤 재동기화(Resync)는 단계가 Connected여도 소켓을 놓은 동안이라 재연결 중으로 본다.
            Some(SessionState::Connected) if !self.row_link_live(row) => "reconnecting",
            Some(SessionState::Connected) => "connected",
            Some(SessionState::Reconnecting) => "reconnecting",
            Some(SessionState::Disconnected) => "disconnected",
            None if self.remote.active.contains(&row) => "connecting",
            None if self.remote.pending_reactivation.contains(&row) => "failed",
            None => "not_connected",
        }
    }

    /// 행 세션의 소켓이 살아 있는지. 원격 위치 표시의 재연결 판정(`locations_of`)과 같은 기준이다.
    fn row_link_live(&self, row: u32) -> bool {
        self.remote
            .sessions
            .iter()
            .find(|session| session.state.saved_row == Some(row))
            .is_some_and(|session| {
                !session
                    .transport
                    .disconnected
                    .load(std::sync::atomic::Ordering::Acquire)
            })
    }

    /// 각 engine에 저장된 원격 행의 연결 상태를 싣는다.
    pub(crate) fn publish_row_states(&mut self) {
        let rows: Vec<(u32, &'static str)> = self
            .engines
            .all_sessions()
            .flat_map(|session| session.core_state.mirror_workspaces())
            .filter(|ws| ws.remote_row.is_some())
            .map(|ws| ws.id)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|row| (row, self.row_state_name(row)))
            .collect();
        let now = std::time::Instant::now();
        for session in self.engines.all_sessions_mut() {
            session.remote.row_states = rows
                .iter()
                .filter(|(row, _)| session.core_state.has_workspace(*row))
                .copied()
                .collect();
        }
        let views: Vec<(u32, crate::remote::state::RowView)> = rows
            .iter()
            .map(|&(row, name)| (row, self.row_view(row, name)))
            .collect();
        for session in self.engines.all_sessions_mut() {
            let previous = std::mem::take(&mut session.remote.row_views);
            session.remote.row_views = views
                .iter()
                .filter(|(row, _)| session.core_state.has_workspace(*row))
                .map(|&(row, mut view)| {
                    // 시도 시작 시각은 시도가 이어지는 동안 유지해 로딩 표시의 지연이 한 번만 지나게 한다.
                    if view.trying_since.is_some() {
                        view.trying_since = previous
                            .get(&row)
                            .and_then(|old| old.trying_since)
                            .or(Some(now));
                    }
                    (row, view)
                })
                .collect();
        }
    }

    /// 이번 실행에서 연결돼 있던 행의 원격 workspace 가 사라졌는지. 형상이 남은 끊긴 행에서만 참이다.
    /// 재시작 뒤 실패는 연결 시도의 결과라 해당하지 않는다. 안내는 메모리에만 있어 재시작하면 지워진다.
    pub(super) fn row_closed_elsewhere(&self, row: u32) -> bool {
        self.row_session_phase(row) == Some(SessionState::Disconnected)
            && self.remote.refusals.notice(row).is_some_and(|notice| {
                notice.kind == tasty_remote::refusal::RowNoticeKind::RemoteWorkspaceMissing
            })
    }

    /// 상태 이름 밖에서 화면이 읽는 행 사실. `trying_since`는 시도 중 표지일 뿐이고 실제 시각은
    /// `publish_row_states`가 채운다.
    fn row_view(&self, row: u32, name: &'static str) -> crate::remote::state::RowView {
        crate::remote::state::RowView {
            user_disconnected: self.remote.user_disconnected.contains(&row),
            closed_elsewhere: self.row_closed_elsewhere(row),
            trying_since: matches!(name, "connecting" | "reconnecting")
                .then(std::time::Instant::now),
        }
    }

    fn row_wait_progress(&self, row: u32) -> Progress {
        progress(&self.row_facts(row))
    }

    /// 연결이 끝난 행의 요청을 이어서 실행하고, 실패하거나 상한을 넘은 요청에 답한다.
    pub(crate) fn advance_row_waits(&mut self) {
        if self.row_waits.is_empty() {
            return;
        }
        let now = Instant::now();
        let waiting = std::mem::take(&mut self.row_waits.waiting);
        let mut ready = Vec::new();
        for wait in waiting {
            match self.row_wait_progress(wait.row) {
                Progress::Ready => ready.push(wait.gated),
                Progress::Waiting if now < wait.deadline => self.row_waits.waiting.push(wait),
                Progress::Waiting => {
                    let refusal = refusal(wait.row, "connect_timeout");
                    self.hold_for_remote_row(wait.gated, RowUse::Refuse(refusal));
                }
                Progress::Failed(reason) => {
                    let refusal = refusal(wait.row, &reason);
                    self.hold_for_remote_row(wait.gated, RowUse::Refuse(refusal));
                }
            }
        }
        for gated in ready {
            self.resume_row_request(gated);
        }
        self.settle_joined_runs();
    }

    /// 합류한 run을 행이 연결되면 성공으로, 연결을 끝내면 그 이유로 실패로 끝낸다.
    fn settle_joined_runs(&mut self) {
        use crate::request_runs::{RunState, finish};
        let runs = std::mem::take(&mut self.row_waits.runs);
        for (row, run) in runs {
            let facts = self.row_facts(row);
            let (state, result) = match progress(&facts) {
                Progress::Waiting => {
                    self.row_waits.runs.push((row, run));
                    continue;
                }
                Progress::Ready if facts.saved => {
                    (RunState::Succeeded, format!("attached as workspace {row}"))
                }
                Progress::Ready => (
                    RunState::Failed,
                    "the saved remote workspace row was closed".to_owned(),
                ),
                Progress::Failed(reason) => (RunState::Failed, refusal(row, &reason).message),
            };
            finish(
                run,
                state,
                result,
                serde_json::json!({ "workspace_id": row }),
            );
        }
    }

    fn resume_row_request(&mut self, gated: Gated) {
        match gated {
            Gated::Ipc { cmd, progress } => {
                self.sync_closing_surfaces();
                // 보관 전에 게이트를 모두 지났다. debug 종료 요청은 이 목록에 들지 않는다.
                self.ipc_dispatch_command(cmd, progress, true);
                self.services.expect_no_parked_reply();
            }
            Gated::Plugin {
                call,
                request,
                progress,
            } => {
                let caller = Self::plugin_caller(&call);
                self.dispatch_plugin_call(call, caller, request, progress, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
        }))
        .unwrap()
    }

    #[test]
    fn using_a_row_connects_but_queries_do_not() {
        for method in [
            "terminal.spawn",
            "attach.agent_target",
            "surface.send",
            "surface.screen_text",
            "split",
            "surface.close",
            "file_handler.dispatch",
        ] {
            assert!(connects(method), "{method}");
        }
        for method in [
            "workspace.list",
            "surface.list",
            "tree",
            "terminal.state",
            "workspace.update",
            "workspace.close",
            "remote.disconnect",
            "attach.agent_call",
            "attach.agent_commit",
            "surface.meta.set",
            "notification.create",
        ] {
            assert!(!connects(method), "{method}");
        }
    }

    #[test]
    fn the_workspace_argument_reads_numbers_and_numeric_strings() {
        assert_eq!(
            workspace_param(&request(
                "terminal.spawn",
                serde_json::json!({"workspace": 7})
            )),
            Some(7)
        );
        assert_eq!(
            workspace_param(&request(
                "terminal.spawn",
                serde_json::json!({"workspace": "8"})
            )),
            Some(8)
        );
        assert_eq!(
            workspace_param(&request(
                "terminal.spawn",
                serde_json::json!({"workspace": "main"})
            )),
            None
        );
    }

    #[test]
    fn refusal_reasons_use_the_notice_names() {
        use tasty_remote::refusal::RowNoticeKind as K;
        assert_eq!(notice_reason(K::Occupied), "occupied");
        assert_eq!(notice_reason(K::MirrorChain), "mirror_workspace");
        assert_eq!(
            notice_reason(K::RemoteWorkspaceMissing),
            "remote_workspace_missing"
        );
        assert_eq!(refusal(3, "window_parked").reason, "window_parked");
        assert!(refusal(3, "connect_timeout").message.contains("35"));
    }

    fn facts(phase: Option<SessionState>, has_window: bool) -> RowFacts {
        RowFacts {
            row: 9,
            saved: true,
            phase,
            live: true,
            active: false,
            has_window,
            user_disconnected: false,
            notice: None,
            waiting: 0,
        }
    }

    /// Given 손실 뒤 재동기화 중인 행(단계 Connected, 소켓 끊김), When 그 행을 쓰는 요청이 오면,
    /// Then 판정은 기다리고 보류한 요청도 다시 붙을 때까지 기다린다. 다시 붙으면 실행한다.
    #[test]
    fn a_resyncing_row_waits_like_the_screen_says_and_runs_once_live() {
        let resyncing = RowFacts {
            live: false,
            ..facts(Some(SessionState::Connected), true)
        };
        assert_eq!(progress(&resyncing), Progress::Waiting);
        assert_eq!(decide(resyncing), RowUse::Wait(9));
        let live = facts(Some(SessionState::Connected), true);
        assert_eq!(progress(&live), Progress::Ready);
        assert_eq!(decide(live), RowUse::Pass);
    }

    #[test]
    fn a_row_in_a_parked_window_is_refused_without_connecting() {
        let decision = decide(facts(None, false));
        let RowUse::Refuse(refusal) = decision else {
            panic!("a parked row must not wait: {decision:?}");
        };
        assert_eq!((refusal.row, refusal.reason.as_str()), (9, "window_parked"));
        assert!(matches!(
            decide(facts(Some(SessionState::Disconnected), false)),
            RowUse::Refuse(RowRefusal { ref reason, .. }) if reason == "window_parked"
        ));
    }

    #[test]
    fn a_parked_engine_has_no_window_for_its_rows() {
        let mut engines = crate::app::engine_registry::EngineRegistry::default();
        let (state, mut session) = crate::state::tests::test_state();
        let mut mirror = crate::model::Workspace::new_with_terminal_marker(
            9_000,
            "remote".to_string(),
            9_001,
            9_002,
            9_003,
        );
        mirror.mirror = true;
        session.borrow_mut().push_mirror_workspace(mirror);
        engines.park_for_test(state, session);
        assert!(!row_has_window(&engines, 9_000));
        assert!(!row_has_window(&engines, 12_345));
    }

    #[test]
    fn using_an_unconnected_row_waits_and_a_connected_row_passes() {
        assert_eq!(decide(facts(None, true)), RowUse::Wait(9));
        assert_eq!(
            decide(facts(Some(SessionState::Reconnecting), true)),
            RowUse::Wait(9)
        );
        assert_eq!(
            decide(facts(Some(SessionState::Disconnected), true)),
            RowUse::Wait(9)
        );
        assert_eq!(
            decide(facts(Some(SessionState::Connected), false)),
            RowUse::Pass
        );
        let local = RowFacts {
            saved: false,
            ..facts(None, false)
        };
        assert_eq!(decide(local), RowUse::Pass);
        let disconnected_by_user = RowFacts {
            user_disconnected: true,
            ..facts(Some(SessionState::Disconnected), true)
        };
        assert!(matches!(
            decide(disconnected_by_user),
            RowUse::Refuse(RowRefusal { ref reason, .. }) if reason == "user_disconnected"
        ));
        let crowded = RowFacts {
            waiting: ROW_WAIT_CAP,
            ..facts(None, true)
        };
        assert!(
            matches!(decide(crowded), RowUse::Refuse(RowRefusal { ref reason, .. }) if reason == "busy")
        );
    }

    #[test]
    fn deadlines_keep_the_earliest_per_row() {
        let now = Instant::now();
        let mut waits = RowWaits::default();
        for (row, after) in [(4, 9), (4, 3), (5, 7)] {
            let (tx, _rx) = std::sync::mpsc::sync_channel(1);
            waits.waiting.push(RowWait {
                gated: Gated::Ipc {
                    cmd: crate::ipc::server::IpcCommand::new(
                        request("surface.send", serde_json::json!({})),
                        tx,
                    ),
                    progress: Default::default(),
                },
                row,
                deadline: now + Duration::from_secs(after),
            });
        }
        let mut deadlines = waits.deadlines();
        deadlines.sort_by_key(|(row, _)| *row);
        assert_eq!(
            deadlines,
            vec![
                (4, now + Duration::from_secs(3)),
                (5, now + Duration::from_secs(7))
            ]
        );
    }
}

// X11 이벤트 루프로 App 을 만드는 시험이라 Linux 에서만 컴파일한다(preset_shutdown_tests 와 같다).
#[cfg(all(test, target_os = "linux"))]
#[path = "row_use_app_tests.rs"]
mod app_tests;
