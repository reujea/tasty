//! 수동 `hook_handler.dispatch` 실행의 최종 결과. 공용 실행 기록([`crate::request_runs`])에
//! `id`(핸들러)·`action` 필드를 붙여 남기며, 접수 응답의 `run` 값으로 `hook_handler.result`와
//! `run.result`가 조회한다.

use serde_json::{Value, json};

use crate::hook_handler::exec::ShellEvent;
use crate::hook_handler::outcome::{SequenceOutcome, Verdict};
use crate::request_runs::{self, RunState};

pub(crate) use crate::request_runs::KEEP;

pub(crate) const ACTION_SEQUENCE: &str = "ipc_sequence";
pub(crate) const ACTION_SHELL: &str = "shell_command";

const METHOD: &str = "hook_handler.dispatch";

/// 진행 중인 실행을 만들고 run 값을 돌려준다. 수동 dispatch는 로컬 호출만 받으므로 소유자가 없다.
pub(crate) fn begin(handler: &str, action: &'static str) -> u64 {
    let mut fields = json!({ "id": handler, "action": action });
    if action == ACTION_SHELL {
        fields["exit_code"] = Value::Null;
    }
    request_runs::begin(METHOD, None, fields)
}

/// 셸 핸들러를 실행기에 넣고 run 값을 돌려준다. run은 실행기가 받기로 확정한 뒤에만 만든다.
/// 거절된 요청은 run을 만들지 않으므로 보관 수 안의 기존 run을 밀어내지 않는다.
pub(crate) fn submit_shell(
    pool: &std::sync::Arc<super::shell_pool::ShellPool>,
    handler: &str,
    build: impl FnOnce() -> std::process::Command + Send + 'static,
) -> Result<u64, super::shell_pool::ShellNotQueued> {
    let mut run = None;
    pool.submit(|| {
        let id = begin(handler, ACTION_SHELL);
        run = Some(id);
        super::shell_pool::ShellJob::new(handler, build, move |event| shell_event(id, event))
    })?;
    run.ok_or_else(|| {
        super::shell_pool::ShellNotQueued::WorkerUnavailable("no run was recorded".into())
    })
}

/// 실행기 대기열에 넣지 못해 동기 오류로 답한 실행은 조회 대상에서 지운다.
pub(crate) fn forget(id: u64) {
    request_runs::forget(id);
}

pub(crate) fn finish_sequence(id: u64, outcome: &SequenceOutcome) {
    let state = match outcome.verdict() {
        Verdict::Succeeded => RunState::Succeeded,
        Verdict::Partial => RunState::Partial,
        Verdict::Failed => RunState::Failed,
        Verdict::Unknown => RunState::Unknown,
    };
    request_runs::finish(
        id,
        state,
        outcome.result_line(),
        json!({ "steps": outcome.steps_json() }),
    );
}

/// 셸 핸들러는 프로세스를 띄웠는지로 판정한다. 종료 코드는 정보로만 싣는다.
pub(crate) fn shell_event(id: u64, event: ShellEvent) {
    match event {
        ShellEvent::Started => {
            request_runs::finish(id, RunState::Succeeded, "process started", Value::Null);
        }
        ShellEvent::NotStarted(error) => {
            request_runs::finish(
                id,
                RunState::Failed,
                format!("process could not start: {error}"),
                Value::Null,
            );
        }
        ShellEvent::Exited(code) => {
            let result = match code {
                Some(code) => format!("process started and exited with code {code}"),
                None => "process started and exited without a code".into(),
            };
            request_runs::amend(id, Some(result), json!({ "exit_code": code }));
        }
    }
}

pub(crate) fn lookup(id: u64) -> Option<Value> {
    request_runs::lookup(id, None).filter(|run| run["method"] == METHOD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_handler::outcome::{StepResult, StepStatus};

    #[test]
    fn a_run_moves_from_running_to_its_final_state() {
        let id = begin("user/t", ACTION_SEQUENCE);
        assert_eq!(lookup(id).unwrap()["state"], "running");
        assert_eq!(lookup(id).unwrap()["done"], false);
        let outcome = SequenceOutcome {
            steps: vec![
                StepResult {
                    index: 0,
                    method: "a.ok".into(),
                    status: StepStatus::Ok,
                },
                StepResult {
                    index: 1,
                    method: "b.missing".into(),
                    status: StepStatus::Failed("rpc error -32601: Method not found".into()),
                },
            ],
        };
        finish_sequence(id, &outcome);
        let got = lookup(id).unwrap();
        assert_eq!(got["state"], "partial");
        assert_eq!(got["done"], true);
        assert_eq!(got["steps"][1]["status"], "failed");
        assert!(got["result"].as_str().unwrap().contains("b.missing"));
    }

    #[test]
    fn a_shell_run_is_judged_by_starting_not_by_exit_code() {
        let ok = begin("user/s", ACTION_SHELL);
        shell_event(ok, ShellEvent::Started);
        shell_event(ok, ShellEvent::Exited(Some(3)));
        let got = lookup(ok).unwrap();
        assert_eq!(got["state"], "succeeded");
        assert_eq!(got["exit_code"], 3);
        let bad = begin("user/s", ACTION_SHELL);
        shell_event(bad, ShellEvent::NotStarted("No such file".into()));
        assert_eq!(lookup(bad).unwrap()["state"], "failed");
        forget(bad);
        assert!(lookup(bad).is_none());
    }
}
