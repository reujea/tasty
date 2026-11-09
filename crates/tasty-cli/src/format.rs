mod graph_dot;
pub(crate) mod graph_table;
mod parent_table;

use anyhow::Result;

use super::{AgentCommands, Commands, ListCommands};
use crate::out::outln;

pub fn format_output(command: &Commands, result: &serde_json::Value) -> Result<()> {
    match command {
        Commands::List { command } => format_list_output(command, result),
        Commands::Agent { command } => format_agent_output(command, result),
        _ => {
            outln!("{}", serde_json::to_string_pretty(result).unwrap())
        }
    }
}

/// 작업 목록·상세(`--json` 이 없을 때)·runner 상태는 텍스트로, 그래프는 기본 부모 ID 표나 DOT 본문으로 표시하고 나머지는 JSON으로 출력한다.
fn format_agent_output(command: &AgentCommands, result: &serde_json::Value) -> Result<()> {
    // 목록·상세는 `--json` 이 없을 때만 텍스트다. 나머지 경우는 아래 기본 갈래의 JSON 이다.
    match task_text(command) {
        Some(TaskText::List) => return format_task_list(result),
        Some(TaskText::Get) => return format_task_get(result),
        None => {}
    }
    match command {
        AgentCommands::TaskRun { .. } => format_task_run(result),
        // 성공하면 아무것도 쓰지 않는다. 이 명령을 부르는 후처리·reduce 셸에서는 표준 출력이
        // 작업의 결과다. 오류는 다른 명령처럼 표준 오류로 나간다.
        AgentCommands::Report { .. } => Ok(()),
        _ if graph_wants_table(command) && graph_table::write_table_response(result)? => Ok(()),
        AgentCommands::TaskGraph { .. } | AgentCommands::DagGet { .. }
            if graph_dot::write_dot_response(result)? =>
        {
            Ok(())
        }
        _ => outln!("{}", serde_json::to_string_pretty(result).unwrap()),
    }
}

/// `task-graph`·`dag-get` 을 부모 ID 표로 낼지.
fn graph_wants_table(command: &AgentCommands) -> bool {
    matches!(
        command,
        AgentCommands::TaskGraph { format, .. } | AgentCommands::DagGet { format, .. }
            if format == graph_table::TABLE_FORMAT
    )
}

/// 텍스트로 내는 작업 명령.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskText {
    List,
    Get,
}

/// `task-list`·`task-get` 을 텍스트로 낼지. `--json` 이면 `None` 이라 IPC 응답을 그대로 낸다.
fn task_text(command: &AgentCommands) -> Option<TaskText> {
    match command {
        AgentCommands::TaskList { json: false, .. } => Some(TaskText::List),
        AgentCommands::TaskGet { json: false, .. } => Some(TaskText::Get),
        _ => None,
    }
}

/// runner 요약 한 줄 — `{running, crashed, ready_count, running_count, store_error,
/// list_failures}` 형태의 `runner` 서브객체를 공유(`task_list`/`task_graph`/`task_run`
/// 응답 공통 shape).
///
/// 카운트는 **`null` 일 수 있다** — store 를 못 읽었다는 뜻이라 `?` 로 렌더하고
/// `store_error` 를 붙인다. 0 으로 렌더하면 "task 가 없다" 와 구분이 안 된다.
fn format_runner_summary(runner: &serde_json::Value) -> String {
    let running = runner
        .get("running")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let crashed = runner
        .get("crashed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ready = runner.get("ready_count").and_then(|v| v.as_u64());
    let inflight = runner.get("running_count").and_then(|v| v.as_u64());
    let store_error = runner.get("store_error").and_then(|v| v.as_str());
    let list_failures = runner
        .get("list_failures")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let status = if crashed {
        "crashed"
    } else if running {
        "running"
    } else {
        "stopped"
    };
    let fmt_count = |c: Option<u64>| c.map_or_else(|| "?".to_string(), |v| v.to_string());
    let mut line = format!(
        "runner: {status} (ready={} running={})",
        fmt_count(ready),
        fmt_count(inflight)
    );
    if let Some(err) = store_error {
        line.push_str(&format!(" — task store unreadable: {err}"));
    } else if !running && (ready.unwrap_or(0) > 0 || inflight.unwrap_or(0) > 0) {
        line.push_str(" — pending work but no runner; `agent task-run --action start` to resume");
    }
    if list_failures > 0 {
        line.push_str(&format!(
            " — runner failed to read the task store {list_failures}x in a row; no task is \
             advancing"
        ));
    }
    line
}

/// `state` 는 `TaskState` 의 internally-tagged 직렬화(`{"kind": "...", ...}`)라
/// 최상위 문자열이 아니다 — `kind` 서브필드를 꺼내야 한다.
fn task_state_kind(task: &serde_json::Value) -> &str {
    task.get("state")
        .and_then(|s| s.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("?")
}

/// 목록의 한 줄. Running 인 v2 task 는 세부 단계를 괄호로 붙인다(`task-get` 의 `phase` 줄과 같은 값).
fn task_list_row(t: &serde_json::Value) -> String {
    let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let state = task_state_kind(t);
    match t.get("phase").and_then(|v| v.as_str()) {
        Some(phase) => format!("{state:<10} {id}  {name}  ({phase})"),
        None => format!("{state:<10} {id}  {name}"),
    }
}

fn format_task_list(result: &serde_json::Value) -> Result<()> {
    let tasks = result.get("tasks").and_then(|v| v.as_array());
    let Some(tasks) = tasks else {
        outln!("{}", serde_json::to_string_pretty(result).unwrap())?;
        return Ok(());
    };
    if tasks.is_empty() {
        outln!("No tasks")?;
    } else {
        for t in tasks {
            outln!("{}", task_list_row(t))?;
        }
    }
    if let Some(runner) = result.get("runner") {
        outln!("{}", format_runner_summary(runner))?;
    }
    Ok(())
}

/// 작업 명령의 한 줄 요약.
fn format_command_summary(command: &serde_json::Value) -> String {
    let kind = command.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    match kind {
        "run" => {
            let cmd = command
                .get("command")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("run: {cmd}")
        }
        "custom" => {
            let method = command
                .get("ipc_method")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("custom: {method}")
        }
        "reduce" => {
            let strategy = command
                .get("strategy")
                .and_then(|s| s.get("kind"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let inputs = command
                .get("inputs")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            format!("reduce: strategy={strategy} inputs=[{inputs}]")
        }
        "wait_barrier" => {
            let name = command
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<input>");
            format!("wait_barrier: {name}")
        }
        "agent" => {
            let provider = command
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let session = command.get("session");
            let num = |k: &str| session.and_then(|s| s.get(k)).and_then(|v| v.as_u64());
            match (num("surface_id"), num("parent_surface")) {
                (Some(s), _) => format!("agent: {provider} (surface {s})"),
                (None, Some(p)) => format!("agent: {provider} (new session under surface {p})"),
                _ => format!("agent: {provider}"),
            }
        }
        other => other.to_string(),
    }
}

/// agent task 회차가 묶인 세션. 단계(`phase`)는 후처리와 같은 `phase:` 줄이 보인다.
fn task_agent_line(task: &serde_json::Value) -> Option<String> {
    let link = task.pointer("/attempt/agent").filter(|v| !v.is_null())?;
    let provider = link.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
    let surface = link.get("surface_id").and_then(|v| v.as_u64()).unwrap_or(0);
    let mut line = format!("agent session: {provider} surface {surface}");
    if let Some(since) = link.get("awaiting_input_since").and_then(|v| v.as_u64()) {
        line.push_str(&format!(", awaiting input since {since}"));
    }
    Some(line)
}

/// 실패 정책의 한 줄 요약.
fn format_on_failure_summary(on_failure: &serde_json::Value) -> String {
    let kind = on_failure
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("abort");
    if kind != "fallback" {
        return kind.to_string();
    }
    if let Some(task) = on_failure.get("task").and_then(|v| v.as_str()) {
        format!("fallback:{task}")
    } else if on_failure.get("inline").is_some() {
        "fallback:inline".to_string()
    } else {
        "fallback".to_string()
    }
}

/// 타입 작업이 전이로 고른 경로. 고른 대상이 없으면 `(none)` 이다.
fn task_route_line(task: &serde_json::Value) -> Option<String> {
    let route = task.get("route").filter(|v| !v.is_null())?;
    let selected: Vec<&str> = route
        .get("selected")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let list = if selected.is_empty() {
        "(none)".to_string()
    } else {
        selected.join(", ")
    };
    let otherwise = if route.get("otherwise").and_then(|v| v.as_bool()) == Some(true) {
        " (otherwise)"
    } else {
        ""
    };
    Some(format!("route: {list}{otherwise}"))
}

/// 실행 없이 건너뛴 타입 작업의 이유. 앞 작업 때문이면 그 작업과 상태를 붙인다.
fn task_skip_line(task: &serde_json::Value) -> Option<String> {
    let skip = task.get("skip").filter(|v| !v.is_null())?;
    let reason = skip.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
    match (
        skip.get("source").and_then(|v| v.as_str()),
        skip.get("source_state").and_then(|v| v.as_str()),
    ) {
        (Some(source), Some(state)) => Some(format!("skip: {reason} ({source} {state})")),
        _ => Some(format!("skip: {reason}")),
    }
}

/// v2 task 의 회차·revision·입력 출처·최종 출력. 결과가 없거나 출력이 없으면 성공처럼 보이지
/// 않도록 `output: none` 과 실패 단계를 적는다. v1 task 는 revision 줄만 낸다.
fn task_typed_lines(task: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(attempt) = task.get("attempt").filter(|v| !v.is_null()) {
        let id = attempt.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        lines.push(format!("attempt: {id}"));
    }
    if let Some(revision) = task.get("revision").and_then(|v| v.as_u64()) {
        lines.push(format!("revision: {revision}"));
    }
    if let Some(snapshot) = task.get("input_snapshot").filter(|v| !v.is_null()) {
        let sources = snapshot.get("sources").and_then(|v| v.as_array());
        for source in sources.into_iter().flatten() {
            let field = source.get("field").and_then(|v| v.as_str()).unwrap_or("?");
            let from = source
                .get("from_task")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let pointer = source.get("pointer").and_then(|v| v.as_str()).unwrap_or("");
            let mut line = format!("input: {field} <- {from}{pointer}");
            if let Some(a) = source.get("producer_attempt").and_then(|v| v.as_str()) {
                line.push_str(&format!(" (attempt {a})"));
            }
            lines.push(line);
        }
        if let Some(failure) = snapshot.get("failure").filter(|v| !v.is_null()) {
            lines.push(format!("input failed: {}", failure_summary(failure)));
        }
    }
    let Some(typed) = task.get("typed_result").filter(|v| !v.is_null()) else {
        return lines;
    };
    let has_output = typed.get("has_output").and_then(|v| v.as_bool()) == Some(true);
    let source = typed
        .get("provenance")
        .and_then(|p| p.get("output_source"))
        .and_then(|v| v.as_str());
    match (has_output, typed.get("output")) {
        (true, Some(output)) => {
            let value = serde_json::to_string(output).unwrap_or_else(|_| output.to_string());
            match source {
                Some(src) => lines.push(format!("output: {value} (from {src})")),
                None => lines.push(format!("output: {value}")),
            }
        }
        _ => lines.push("output: none".to_string()),
    }
    if let Some(error) = typed.get("error").filter(|v| !v.is_null()) {
        lines.push(format!("error: {}", failure_summary(error)));
    }
    lines
}

/// 러너가 TTL 을 갱신하지 못해 lease·permit 을 잃은 기록(`holding_warnings`) 한 줄씩.
fn task_holding_warning_lines(task: &serde_json::Value) -> Vec<String> {
    task.get("holding_warnings")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|w| {
            let message = w.get("message").and_then(|v| v.as_str()).unwrap_or("?");
            format!("warning: {message}")
        })
        .collect()
}

/// `TaskFailure` 한 줄: `<단계>[/<코드>]: <메시지>`. 메시지가 이미 단계 이름으로 시작하면
/// (후처리 실패의 `postprocess <원인>: …`) 단계를 다시 붙이지 않는다.
fn failure_summary(failure: &serde_json::Value) -> String {
    let stage = failure.get("stage").and_then(|v| v.as_str()).unwrap_or("?");
    let message = failure
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match failure.get("code").and_then(|v| v.as_str()) {
        Some(code) => format!("{stage}/{code}: {message}"),
        None if message
            .strip_prefix(stage)
            .is_some_and(|rest| rest.starts_with([' ', ':'])) =>
        {
            message.to_string()
        }
        None => format!("{stage}: {message}"),
    }
}

fn format_task_get(result: &serde_json::Value) -> Result<()> {
    let id = result.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let name = result.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let state = task_state_kind(result);
    outln!("id: {id}")?;
    outln!("name: {name}")?;
    // 실패는 error, 결과 불명은 reason 에 사유가 있다.
    if let Some(error) = result
        .get("state")
        .and_then(|s| s.get("error").or_else(|| s.get("reason")))
        .and_then(|v| v.as_str())
    {
        outln!("state: {state} ({error})")?;
    } else {
        outln!("state: {state}")?;
    }
    for line in task_postprocess_lines(result) {
        outln!("{line}")?;
    }
    for line in task_holding_warning_lines(result) {
        outln!("{line}")?;
    }
    if let Some(wait) = result.get("awaiting_external") {
        let wait_key = wait.get("wait_key").and_then(|v| v.as_str()).unwrap_or("?");
        let deadline_ms = wait
            .get("deadline_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        outln!("awaiting external signal — wait_key={wait_key}, deadline_ms={deadline_ms}")?;
    }
    if let Some(command) = result.get("command") {
        outln!("command: {}", format_command_summary(command))?;
    }
    if let Some(deps) = result.get("depends_on").and_then(|v| v.as_array())
        && !deps.is_empty()
    {
        let list = deps
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        outln!("depends_on: {list}")?;
    }
    if let Some(on_failure) = result.get("on_failure") {
        outln!("on_failure: {}", format_on_failure_summary(on_failure))?;
    }
    if let Some(line) = task_route_line(result) {
        outln!("{line}")?;
    }
    if let Some(line) = task_skip_line(result) {
        outln!("{line}")?;
    }
    if let Some(line) = task_agent_line(result) {
        outln!("{line}")?;
    }
    for line in task_typed_lines(result) {
        outln!("{line}")?;
    }
    if let Some(metadata) = result.get("metadata")
        && !metadata.is_null()
    {
        outln!(
            "metadata: {}",
            serde_json::to_string_pretty(metadata).unwrap()
        )?;
    }
    if let Some(result_val) = result.get("result")
        && !result_val.is_null()
    {
        outln!(
            "result: {}",
            serde_json::to_string_pretty(result_val).unwrap()
        )?;
    }
    Ok(())
}

/// 후처리가 있는 v2 task 의 진행과 결과. 진행 중이면 단계와 실행 번호를, 끝났으면 마지막
/// 실행의 결과와 재시도로 넘어간 실행의 원인을 보인다.
pub fn task_postprocess_lines(result: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(phase) = result.get("phase").and_then(|v| v.as_str()) {
        let run = result
            .pointer("/attempt/postprocess/phase/run")
            .and_then(|v| v.as_u64());
        lines.push(match run {
            // 재시도한 task 가 이전 회차의 프로세스 종료를 확인할 때까지 시작하지 않고 기다린다.
            _ if phase == "waiting_previous_attempt" => format!(
                "phase: {phase} (starts after the previous attempt's processes are confirmed ended)"
            ),
            Some(run) => format!("phase: {phase} (run {run})"),
            None => format!("phase: {phase}"),
        });
    }
    let Some(raw) = result.pointer("/typed_result/raw/postprocess") else {
        return lines;
    };
    let run = raw.get("run").and_then(|v| v.as_u64()).unwrap_or(0);
    let exit = |v: &serde_json::Value| match v.get("exit_code").and_then(|c| c.as_i64()) {
        Some(code) => format!(", exit_code {code}"),
        None => String::new(),
    };
    lines.push(match raw.get("cause").and_then(|v| v.as_str()) {
        Some(cause) => format!("postprocess: run {run} failed ({cause}){}", exit(raw)),
        None => format!("postprocess: run {run} succeeded{}", exit(raw)),
    });
    if let Some(failed) = raw.get("failed_runs").and_then(|v| v.as_array())
        && !failed.is_empty()
    {
        let list = failed
            .iter()
            .map(|r| {
                let n = r.get("run").and_then(|v| v.as_u64()).unwrap_or(0);
                let cause = r.get("cause").and_then(|v| v.as_str()).unwrap_or("?");
                format!("run {n} {cause}{}", exit(r))
            })
            .collect::<Vec<_>>()
            .join("; ");
        lines.push(format!("postprocess retried after: {list}"));
    }
    lines
}

fn format_task_run(result: &serde_json::Value) -> Result<()> {
    outln!("{}", format_runner_summary(result))
}

fn format_list_output(command: &ListCommands, result: &serde_json::Value) -> Result<()> {
    match command {
        ListCommands::Tree => format_tree(result),
        ListCommands::Workspaces => format_workspace_list(result),
        ListCommands::Panes => format_pane_list(result),
        ListCommands::Notifications => format_notification_list(result),
        ListCommands::Timers => format_timer_list(result),
        _ => outln!("{}", serde_json::to_string_pretty(result).unwrap()),
    }
}

/// Render workspaces, panes, tabs, and surfaces as a tree.
fn format_tree(result: &serde_json::Value) -> Result<()> {
    if let Some(workspaces) = result.as_array() {
        for ws in workspaces {
            let ws_id = ws.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = ws.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let active = ws.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
            let marker = if active { " *" } else { "" };
            outln!("Workspace: {} (id:{}){}", name, ws_id, marker)?;

            if let Some(panes) = ws.get("panes").and_then(|v| v.as_array()) {
                for pane in panes {
                    format_pane(pane)?;
                }
            }
        }
    }
    Ok(())
}

/// Render a single pane line and its tabs.
fn format_pane(pane: &serde_json::Value) -> Result<()> {
    let pid = pane.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    let focused = pane
        .get("focused")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let pfx = if focused { ">" } else { " " };
    outln!("  {} Pane {} (id:{})", pfx, pid, pid)?;

    if let Some(tabs) = pane.get("tabs").and_then(|v| v.as_array()) {
        for tab in tabs {
            format_tab(tab)?;
        }
    }
    Ok(())
}

/// Render a single tab line and, for split tabs, its nested layout tree.
fn format_tab(tab: &serde_json::Value) -> Result<()> {
    let tid = tab.get("id").and_then(|v| v.as_u64());
    let tname = tab.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let tactive = tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
    let tpfx = if tactive { "*" } else { " " };

    let surface = tab.get("surface");
    let stype = surface.and_then(|s| s.get("type")).and_then(|v| v.as_str());
    let sid = surface.and_then(|s| s.get("id")).and_then(|v| v.as_u64());
    let surfaces_arr = surface
        .and_then(|s| s.get("surfaces"))
        .and_then(|v| v.as_array());

    if stype == Some("SplitLayout")
        && let Some(layout) = surface.and_then(|s| s.get("layout"))
        && !layout.is_null()
    {
        let focused = surface
            .and_then(|s| s.get("focused_surface"))
            .and_then(|v| v.as_u64());
        match tid {
            Some(t) => outln!("      {} {} (tab:{})", tpfx, tname, t)?,
            None => outln!("      {} {}", tpfx, tname)?,
        }
        let mut lines = Vec::new();
        render_layout(layout, "        ", true, focused, &mut lines);
        for l in lines {
            outln!("{}", l)?;
        }
        return Ok(());
    }

    let ids = format_tab_ids(tid, sid, surfaces_arr, stype);

    if ids.is_empty() {
        outln!("      {} {}", tpfx, tname)?;
    } else {
        outln!("      {} {} [{}]", tpfx, tname, ids)?;
    }
    Ok(())
}

/// Build the bracketed `[tab:.., surface:.., <type>]` id list for a tab.
fn format_tab_ids(
    tid: Option<u64>,
    sid: Option<u64>,
    surfaces_arr: Option<&Vec<serde_json::Value>>,
    stype: Option<&str>,
) -> String {
    let mut ids = String::new();
    if let Some(t) = tid {
        ids.push_str(&format!("tab:{}", t));
    }
    if let Some(s) = sid {
        if !ids.is_empty() {
            ids.push_str(", ");
        }
        ids.push_str(&format!("surface:{}", s));
    } else if let Some(arr) = surfaces_arr {
        for s in arr {
            if let Some(sv) = s.as_u64() {
                if !ids.is_empty() {
                    ids.push_str(", ");
                }
                ids.push_str(&format!("surface:{}", sv));
            }
        }
    }
    if let Some(t) = stype
        && t != "Terminal"
    {
        if !ids.is_empty() {
            ids.push_str(", ");
        }
        ids.push_str(t);
    }
    ids
}

/// Render a `to_tree_json_full` split tree as an indented ASCII tree.
///
/// `prefix` is the running indent for this node's line; `is_last` controls the
/// branch glyph (`└─` vs `├─`). `focused` marks the focused surface leaf.
///
/// Split node label carries direction · ratio · child positions:
/// `vertical (L|R)` = first child left, second right; `horizontal (T|B)` =
/// first top, second bottom (Vertical splits width, Horizontal splits height).
fn render_layout(
    node: &serde_json::Value,
    prefix: &str,
    is_last: bool,
    focused: Option<u64>,
    out: &mut Vec<String>,
) {
    let branch = if is_last { "└─ " } else { "├─ " };
    let child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });

    match node.get("type").and_then(|v| v.as_str()) {
        Some("Split") => {
            let dir = node
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let ratio = node.get("ratio").and_then(|v| v.as_f64()).unwrap_or(0.5);
            let pct = (ratio * 100.0).round() as i64;
            let sides = if dir == "vertical" { "L|R" } else { "T|B" };
            out.push(format!(
                "{}{}{} ({}) {}:{}",
                prefix,
                branch,
                dir,
                sides,
                pct,
                100 - pct
            ));
            if let Some(first) = node.get("first") {
                render_layout(first, &child_prefix, false, focused, out);
            }
            if let Some(second) = node.get("second") {
                render_layout(second, &child_prefix, true, focused, out);
            }
        }
        _ => {
            let kind = node.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let id = node.get("id").and_then(|v| v.as_u64());
            let focus_mark = if id.is_some() && id == focused {
                " *focus"
            } else {
                ""
            };
            match id {
                Some(i) => out.push(format!(
                    "{}{}surface:{} ({}){}",
                    prefix, branch, i, kind, focus_mark
                )),
                None => out.push(format!("{}{}({}){}", prefix, branch, kind, focus_mark)),
            }
        }
    }
}

/// workspace ID와 mirror 여부를 표시한다.
/// (id:N), [mirror] 같은 구조 토큰은 출력 파서가 사용하므로 번역하지 않는다.
fn format_workspace_row(ws: &serde_json::Value) -> String {
    let id = ws.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    let name = ws.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let active = ws.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
    let pane_count = ws.get("pane_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let mirror = ws.get("mirror").and_then(|v| v.as_bool()).unwrap_or(false);
    let active_marker = if active { " *" } else { "" };
    let mirror_marker = if mirror { " [mirror]" } else { "" };
    format!("{name} (id:{id}){active_marker}{mirror_marker} ({pane_count} panes)")
}

fn format_workspace_list(result: &serde_json::Value) -> Result<()> {
    if let Some(workspaces) = result.as_array() {
        for ws in workspaces {
            outln!("{}", format_workspace_row(ws))?;
        }
    }
    Ok(())
}

fn format_pane_list(result: &serde_json::Value) -> Result<()> {
    if let Some(panes) = result.as_array() {
        for pane in panes {
            let pid = pane.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let focused = pane
                .get("focused")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let tab_count = pane.get("tab_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let ws_id = pane
                .get("workspace_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let ws_name = pane
                .get("workspace_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let marker = if focused { " *" } else { "" };
            outln!(
                "Pane {}{} ({} tabs) [ws:{} {}]",
                pid,
                marker,
                tab_count,
                ws_id,
                ws_name
            )?;
        }
    }
    Ok(())
}

/// timer.list의 타이머와 다음 hard deadline을 표시한다.
/// Lax의 hard deadline은 next_due + slack이다.
fn format_timer_list(result: &serde_json::Value) -> Result<()> {
    const HEADER: [&str; 5] = ["key", "interval", "next_due", "precision", "last_fired"];
    let empty = Vec::new();
    let timers = result
        .get("timers")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let rows: Vec<TimerRowText> = timers.iter().map(timer_row_text).collect();

    let mut widths = HEADER.map(str::len);
    for row in &rows {
        for (w, cell) in widths.iter_mut().zip(row.cells.iter()) {
            *w = (*w).max(cell.chars().count());
        }
    }

    let header_cells = HEADER.map(str::to_string);
    outln!("{}", timer_row_line(&header_cells, &widths, ""))?;
    for row in &rows {
        outln!("{}", timer_row_line(&row.cells, &widths, row.marker))?;
    }
    outln!("{}", timer_hard_deadline_line(result))
}

/// 한 타이머의 표시용 셀 + 소속 허브 표시.
struct TimerRowText {
    cells: [String; 5],
    marker: &'static str,
}

fn timer_row_text(t: &serde_json::Value) -> TimerRowText {
    let key = t
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    // 반복이 아니면(`once_after`/`once_at`) 주기가 없다 — 빈칸 대신 성격을 적는다.
    let interval = t
        .get("interval_ms")
        .and_then(|v| v.as_u64())
        .map_or_else(|| "once".to_string(), fmt_duration_ms);
    let next_due = fmt_offset_ms(t.get("next_due_ms").and_then(|v| v.as_i64()).unwrap_or(0));
    let precision = match t.get("precision").and_then(|v| v.as_str()).unwrap_or("?") {
        "lax" => {
            let slack = t.get("slack_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("lax(slack {})", fmt_duration_ms(slack))
        }
        other => other.to_string(),
    };
    let last_fired = t
        .get("last_fired_ms_ago")
        .and_then(|v| v.as_i64())
        .map_or_else(|| "-".to_string(), fmt_ago_ms);
    // 본체 허브가 대다수라 그쪽은 표시하지 않고, 별도 허브만 꼬리표를 단다.
    let marker = match t.get("hub").and_then(|v| v.as_str()) {
        Some("plugin") => "[plugin hub]",
        _ => "",
    };
    TimerRowText {
        cells: [key, interval, next_due, precision, last_fired],
        marker,
    }
}

fn timer_row_line(cells: &[String; 5], widths: &[usize; 5], marker: &str) -> String {
    let mut out = String::new();
    for (i, (cell, w)) in cells.iter().zip(widths.iter()).enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(cell);
        for _ in cell.chars().count()..*w {
            out.push(' ');
        }
    }
    if !marker.is_empty() {
        out.push_str("  ");
        out.push_str(marker);
    }
    out.trim_end().to_string()
}

fn timer_hard_deadline_line(result: &serde_json::Value) -> String {
    let hard = result.get("hard_deadline").filter(|v| !v.is_null());
    match hard {
        Some(h) => {
            let key = h.get("key").and_then(|v| v.as_str()).unwrap_or("?");
            let in_ms = h.get("in_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            format!("\u{2500} hard deadline: {} ({key})", fmt_offset_ms(in_ms))
        }
        None => {
            "\u{2500} hard deadline: none (nothing is scheduled to wake this instance)".to_string()
        }
    }
}

fn fmt_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// 지금 기준 상대 시각. 음수(=이미 지난 데드라인)를 0 으로 접지 않는다 — 밀려 있는
/// 타이머는 스핀/기아 진단의 1차 단서다.
fn fmt_offset_ms(ms: i64) -> String {
    let sign = if ms < 0 { '-' } else { '+' };
    format!("{sign}{}", fmt_duration_ms(ms.unsigned_abs()))
}

fn fmt_ago_ms(ms: i64) -> String {
    if ms < 0 {
        format!("in {}", fmt_duration_ms(ms.unsigned_abs()))
    } else {
        format!("{} ago", fmt_duration_ms(ms.unsigned_abs()))
    }
}

fn format_notification_list(result: &serde_json::Value) -> Result<()> {
    if let Some(notifs) = result.as_array() {
        if notifs.is_empty() {
            outln!("No notifications")?;
            return Ok(());
        }
        for n in notifs {
            let title = n.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let body = n.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let read = n.get("read").and_then(|v| v.as_bool()).unwrap_or(false);
            let marker = if read { " " } else { "*" };
            let id = n.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let level = n.get("level").and_then(|v| v.as_str()).unwrap_or("info");
            if body.is_empty() {
                outln!("{} #{} [{}] {}", marker, id, level, title)?;
            } else {
                outln!("{} #{} [{}] {}: {}", marker, id, level, title, body)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TaskText, format_runner_summary, format_workspace_row, graph_wants_table, render_layout,
        task_agent_line, task_holding_warning_lines, task_list_row, task_postprocess_lines,
        task_route_line, task_skip_line, task_text, task_typed_lines, timer_hard_deadline_line,
        timer_row_line, timer_row_text,
    };
    use clap::Parser;
    use serde_json::json;

    fn agent_command(args: &[&str]) -> crate::AgentCommands {
        match crate::Cli::try_parse_from(args).expect("parse").command {
            Some(crate::Commands::Agent { command }) => command,
            _ => panic!("not an agent command: {args:?}"),
        }
    }

    #[test]
    fn only_a_graph_command_without_another_format_is_written_as_a_table() {
        let graph = ["tasty", "agent", "task-graph", "--workspace", "1"];
        let dag = ["tasty", "agent", "dag-get", "--id", "c:a"];
        for base in [&graph[..], &dag[..]] {
            assert!(graph_wants_table(&agent_command(base)), "{base:?}");
            for format in ["json", "dot"] {
                let args = [base, &["--format", format][..]].concat();
                assert!(!graph_wants_table(&agent_command(&args)), "{args:?}");
            }
        }
        assert!(!graph_wants_table(&agent_command(&[
            "tasty", "agent", "dag-list"
        ])));
    }

    #[test]
    fn task_list_and_task_get_print_text_unless_json_is_asked() {
        use clap::Parser;
        let text = |args: &[&str]| {
            let cli = crate::Cli::try_parse_from(args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
            let Some(crate::Commands::Agent { command }) = cli.command else {
                panic!("{args:?}: not an agent command");
            };
            task_text(&command)
        };
        let get = [
            "tasty",
            "agent",
            "task-get",
            "--workspace-id",
            "1",
            "--id",
            "a",
        ];
        let list = ["tasty", "agent", "task-list", "--workspace-id", "1"];
        assert_eq!(text(&get), Some(TaskText::Get));
        assert_eq!(text(&list), Some(TaskText::List));
        assert_eq!(text(&[&get[..], &["--json"]].concat()), None);
        assert_eq!(text(&[&list[..], &["--json"]].concat()), None);
    }

    #[test]
    fn a_running_typed_task_row_shows_its_phase() {
        let row = task_list_row(&json!({"id": "judge", "name": "Judge",
            "state": {"kind": "running"}, "phase": "awaiting_input"}));
        assert_eq!(row, "running    judge  Judge  (awaiting_input)");
        let row = task_list_row(&json!({"id": "a", "name": "A", "state": {"kind": "ready"}}));
        assert_eq!(row, "ready      a  A");
    }

    #[test]
    fn task_get_shows_the_agent_session_and_leaves_the_phase_to_the_phase_line() {
        let task = json!({
            "state": {"kind": "running"}, "phase": "awaiting_input",
            "attempt": {"agent": {"provider": "claude", "surface_id": 12, "awaiting_input_since": 5}},
        });
        assert_eq!(
            task_agent_line(&task).as_deref(),
            Some("agent session: claude surface 12, awaiting input since 5")
        );
        assert_eq!(task_postprocess_lines(&task), ["phase: awaiting_input"]);
        assert_eq!(task_agent_line(&json!({"phase": "retry_wait"})), None);
        let waiting = json!({"id": "t-1", "name": "z", "state": {"kind": "ready"},
                             "phase": "waiting_previous_attempt"});
        assert_eq!(
            task_list_row(&waiting),
            "ready      t-1  z  (waiting_previous_attempt)"
        );
        assert_eq!(
            task_postprocess_lines(&waiting),
            [
                "phase: waiting_previous_attempt (starts after the previous attempt's processes are confirmed ended)"
            ]
        );
    }

    #[test]
    fn task_get_shows_each_lost_holding_as_a_warning_line() {
        assert!(task_holding_warning_lines(&json!({"state": {"kind": "running"}})).is_empty());
        let lost = json!({"holding_warnings": [
            {"kind": "lease", "name": "db", "message": "lease 'db' is no longer held by 't1'"}]});
        assert_eq!(
            task_holding_warning_lines(&lost),
            ["warning: lease 'db' is no longer held by 't1'"]
        );
    }

    #[test]
    fn task_get_shows_the_postprocess_phase_while_it_runs() {
        let lines = task_postprocess_lines(&json!({
            "state": {"kind": "running"}, "phase": "retry_wait",
            "attempt": {"postprocess": {"phase": {"state": "pending", "run": 2}}},
        }));
        assert_eq!(lines, ["phase: retry_wait (run 2)"]);
        // 후처리가 없는 task 는 아무 줄도 더하지 않는다.
        assert!(task_postprocess_lines(&json!({"state": {"kind": "running"}})).is_empty());
    }

    #[test]
    fn task_get_shows_the_postprocess_cause_and_retried_runs() {
        let lines = task_postprocess_lines(&json!({
            "typed_result": {"raw": {"postprocess": {
                "command": ["judge"], "run": 3, "exit_code": 4, "cause": "nonzero_exit",
                "failed_runs": [
                    {"run": 1, "cause": "timeout", "message": "m"},
                    {"run": 2, "exit_code": 1, "cause": "invalid_json", "message": "m"},
                ],
            }}},
        }));
        assert_eq!(
            lines,
            [
                "postprocess: run 3 failed (nonzero_exit), exit_code 4",
                "postprocess retried after: run 1 timeout; run 2 invalid_json, exit_code 1",
            ]
        );
        let ok = task_postprocess_lines(&json!({
            "typed_result": {"raw": {"postprocess": {"command": ["judge"], "run": 1, "exit_code": 0}}},
        }));
        assert_eq!(ok, ["postprocess: run 1 succeeded, exit_code 0"]);
    }

    #[test]
    fn task_detail_names_the_route_and_why_a_task_was_skipped() {
        assert_eq!(task_route_line(&json!({})), None);
        assert_eq!(
            task_route_line(&json!({"route": {"matched": [1], "selected": ["fix", "notify"]}})),
            Some("route: fix, notify".into())
        );
        assert_eq!(
            task_route_line(
                &json!({"route": {"matched": [], "otherwise": true, "selected": ["human"]}})
            ),
            Some("route: human (otherwise)".into())
        );
        assert_eq!(
            task_route_line(&json!({"route": {"matched": [], "selected": []}})),
            Some("route: (none)".into())
        );
        assert_eq!(task_skip_line(&json!({"skip": null})), None);
        assert_eq!(
            task_skip_line(&json!({"skip": {"reason": "branch_not_selected"}})),
            Some("skip: branch_not_selected".into())
        );
        assert_eq!(
            task_skip_line(
                &json!({"skip": {"reason": "upstream_unavailable", "source": "flaky", "source_state": "failed"}})
            ),
            Some("skip: upstream_unavailable (flaky failed)".into())
        );
    }

    #[test]
    fn typed_task_detail_shows_sources_output_and_a_missing_output() {
        assert_eq!(task_typed_lines(&json!({"revision": 4})), ["revision: 4"]);
        let lines = task_typed_lines(&json!({
            "attempt": {"id": "review#2", "number": 2},
            "revision": 9,
            "input_snapshot": {"sources": [
                {"field": "summary", "from_task": "implement", "pointer": "", "producer_attempt": "implement#1"},
                {"field": "n", "from_task": "count", "pointer": "/total"}]},
            "typed_result": {"has_output": true, "output": {"verdict": "pass"},
                             "provenance": {"contract_version": 2, "kind": "agent", "output_source": "agent.submission"}},
        }));
        assert_eq!(
            lines,
            [
                "attempt: review#2",
                "revision: 9",
                "input: summary <- implement (attempt implement#1)",
                "input: n <- count/total",
                "output: {\"verdict\":\"pass\"} (from agent.submission)",
            ]
        );
        let lines = task_typed_lines(&json!({
            "typed_result": {"has_output": false, "output": null,
                             "error": {"stage": "execution", "code": "result_missing", "message": "turn ended without a result"},
                             "provenance": {"contract_version": 2, "kind": "agent", "output_source": "agent.final_answer"}},
        }));
        assert_eq!(
            lines,
            [
                "output: none",
                "error: execution/result_missing: turn ended without a result",
            ]
        );
        let lines = task_typed_lines(&json!({
            "typed_result": {"has_output": false, "output": null,
                             "error": {"stage": "postprocess", "message": "postprocess nonzero_exit: exited with code 3"}},
        }));
        assert_eq!(
            lines,
            [
                "output: none",
                "error: postprocess nonzero_exit: exited with code 3",
            ]
        );
        let lines = task_typed_lines(&json!({
            "typed_result": {"has_output": false, "output": null,
                             "error": {"stage": "input", "message": "inputs: binding failed"}},
        }));
        assert_eq!(lines[1], "error: input: inputs: binding failed");
    }

    #[test]
    fn timer_summary_names_what_is_waking_the_instance() {
        let line = timer_hard_deadline_line(&json!({
            "hard_deadline": {"key": "DagGraph(41)", "hub": "app", "in_ms": 200},
        }));
        assert_eq!(line, "\u{2500} hard deadline: +200ms (DagGraph(41))");
    }

    #[test]
    fn timer_summary_says_so_when_nothing_is_scheduled() {
        let line = timer_hard_deadline_line(&json!({"hard_deadline": null}));
        assert!(line.contains("none"), "got: {line}");
    }

    #[test]
    fn an_overdue_timer_keeps_its_negative_offset_in_the_summary() {
        let line = timer_hard_deadline_line(&json!({
            "hard_deadline": {"key": "LayoutFlush", "hub": "app", "in_ms": -1500},
        }));
        assert_eq!(line, "\u{2500} hard deadline: -1.5s (LayoutFlush)");
    }

    #[test]
    fn a_lax_row_shows_its_slack_and_a_plugin_row_is_tagged() {
        let row = timer_row_text(&json!({
            "key": "PluginRss", "hub": "plugin", "interval_ms": 30000,
            "next_due_ms": 12000, "precision": "lax", "slack_ms": 15000,
            "last_fired_ms_ago": 18000,
        }));
        assert_eq!(
            row.cells,
            [
                "PluginRss".to_string(),
                "30s".to_string(),
                "+12s".to_string(),
                "lax(slack 15s)".to_string(),
                "18s ago".to_string(),
            ]
        );
        assert_eq!(row.marker, "[plugin hub]");
    }

    #[test]
    fn a_one_shot_timer_that_never_fired_renders_placeholders() {
        let row = timer_row_text(&json!({
            "key": "NativeMenu", "hub": "app", "interval_ms": null,
            "next_due_ms": 8, "precision": "strict", "slack_ms": null,
            "last_fired_ms_ago": null,
        }));
        assert_eq!(row.cells[1], "once");
        assert_eq!(row.cells[4], "-");
        assert_eq!(row.marker, "");
    }

    #[test]
    fn columns_are_padded_to_the_widest_cell() {
        let cells = [
            "Busy".to_string(),
            "1s".to_string(),
            "+400ms".to_string(),
            "strict".to_string(),
            "600ms ago".to_string(),
        ];
        let line = timer_row_line(&cells, &[12, 8, 8, 14, 10], "");
        assert_eq!(
            line,
            "Busy          1s        +400ms    strict          600ms ago"
        );
    }

    #[test]
    fn runner_summary_stopped_with_no_pending_work_has_no_hint() {
        let s = format_runner_summary(&json!({
            "running": false, "crashed": false, "ready_count": 0, "running_count": 0,
        }));
        assert_eq!(s, "runner: stopped (ready=0 running=0)");
    }

    /// 결정 1 — "정지 상태 발견 가능성": runner 가 꺼져 있는데 대기 중인 task 가
    /// 있으면 재개 방법까지 안내한다.
    #[test]
    fn runner_summary_stopped_with_pending_work_hints_resume() {
        let s = format_runner_summary(&json!({
            "running": false, "crashed": false, "ready_count": 2, "running_count": 1,
        }));
        assert_eq!(
            s,
            "runner: stopped (ready=2 running=1) — pending work but no runner; \
             `agent task-run --action start` to resume"
        );
    }

    #[test]
    fn runner_summary_crashed_takes_precedence_over_running() {
        let s = format_runner_summary(&json!({
            "running": true, "crashed": true, "ready_count": 0, "running_count": 0,
        }));
        assert_eq!(s, "runner: crashed (ready=0 running=0)");
    }

    #[test]
    fn runner_summary_running_with_pending_work_has_no_hint() {
        let s = format_runner_summary(&json!({
            "running": true, "crashed": false, "ready_count": 3, "running_count": 1,
        }));
        assert_eq!(s, "runner: running (ready=3 running=1)");
    }

    /// store 를 못 읽으면 카운트를 0 으로 렌더하지 않는다 — 0 은 "task 가 없다" 와
    /// 값이 같아서, 조회가 실패했다는 사실이 화면에서 사라진다.
    #[test]
    fn runner_summary_unreadable_store_shows_unknown_counts_and_reason() {
        let s = format_runner_summary(&json!({
            "running": true, "crashed": false,
            "ready_count": null, "running_count": null,
            "store_error": "db is locked", "list_failures": 0,
        }));
        assert_eq!(
            s,
            "runner: running (ready=? running=?) — task store unreadable: db is locked"
        );
    }

    /// 러너가 살아는 있는데 계속 못 읽는 상태 — `running: true` 만 보면 정상으로
    /// 보이므로 연속 실패 횟수를 같은 줄에 붙인다.
    #[test]
    fn runner_summary_reports_consecutive_list_failures() {
        let s = format_runner_summary(&json!({
            "running": true, "crashed": false, "ready_count": 2, "running_count": 1,
            "store_error": null, "list_failures": 9,
        }));
        assert_eq!(
            s,
            "runner: running (ready=2 running=1) — runner failed to read the task store 9x in \
             a row; no task is advancing"
        );
    }

    /// `terminal.spawn` 의 workspace-not-found 오류가 이 명령을 가리킨다 — 행에서
    /// `--workspace` 에 넣을 값을 바로 얻을 수 있어야 한다.
    #[test]
    fn workspace_row_includes_id() {
        let row = format_workspace_row(&json!({
            "id": 3, "name": "project-a", "active": false, "pane_count": 2, "mirror": false,
        }));
        assert_eq!(row, "project-a (id:3) (2 panes)");
    }

    /// mirror 워크스페이스는 구조 변경이 원격으로 forward 되어 로컬과 동작이 다르다 —
    /// 에이전트가 조작 전에 판별할 수 있어야 한다.
    #[test]
    fn workspace_row_marks_mirror() {
        let row = format_workspace_row(&json!({
            "id": 7, "name": "remote-ws", "active": false, "pane_count": 1, "mirror": true,
        }));
        assert_eq!(row, "remote-ws (id:7) [mirror] (1 panes)");
    }

    /// active 표시는 id 뒤, mirror 앞. 둘은 서로 다른 축이라 함께 붙을 수 있다.
    #[test]
    fn workspace_row_shows_active_and_mirror_together() {
        let row = format_workspace_row(&json!({
            "id": 2, "name": "ws", "active": true, "pane_count": 4, "mirror": true,
        }));
        assert_eq!(row, "ws (id:2) * [mirror] (4 panes)");
    }

    /// 구버전 호스트 응답처럼 `mirror` 가 없으면 없는 것으로 본다(마커 없음) —
    /// 필드 추가가 구버전 CLI/호스트 조합을 깨지 않는다.
    #[test]
    fn workspace_row_without_mirror_field_defaults_to_local() {
        let row = format_workspace_row(&json!({
            "id": 1, "name": "Workspace 1", "active": true, "pane_count": 2,
        }));
        assert_eq!(row, "Workspace 1 (id:1) * (2 panes)");
    }

    #[test]
    fn render_layout_nested_split_tree() {
        // vertical(L|R) 60:40 → [left leaf 396, right = horizontal(T|B) 50:50 of 417/418]
        let layout = json!({
            "type": "Split",
            "direction": "vertical",
            "ratio": 0.6,
            "first": { "type": "Leaf", "id": 396, "kind": "terminal" },
            "second": {
                "type": "Split",
                "direction": "horizontal",
                "ratio": 0.5,
                "first": { "type": "Leaf", "id": 417, "kind": "terminal" },
                "second": { "type": "Leaf", "id": 418, "kind": "markdown" },
            },
        });
        let mut out = Vec::new();
        render_layout(&layout, "        ", true, Some(417), &mut out);
        let expected = vec![
            "        └─ vertical (L|R) 60:40",
            "           ├─ surface:396 (terminal)",
            "           └─ horizontal (T|B) 50:50",
            "              ├─ surface:417 (terminal) *focus",
            "              └─ surface:418 (markdown)",
        ];
        assert_eq!(out, expected);
    }
}
