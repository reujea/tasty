use super::MarkerScanner;

fn scan_cut(chunks: &[&[u8]]) -> Vec<(String, u64)> {
    let mut s = MarkerScanner::default();
    let mut out = Vec::new();
    for c in chunks {
        s.feed(c, |t, cut| out.push((t.to_string(), cut)));
    }
    s.finish(|t, cut| out.push((t.to_string(), cut)));
    out
}

fn scan(chunks: &[&[u8]]) -> Vec<String> {
    scan_cut(chunks).into_iter().map(|(t, _)| t).collect()
}

#[test]
fn only_whole_marker_lines_are_picked_from_stderr() {
    assert_eq!(
        scan(&[
            b"warn\n::tasty-report::one\r\n  ::tasty-report::indented\n::tasty-re",
            b"port::two"
        ]),
        ["one", "two"]
    );
    assert_eq!(scan(&[b"::tasty-report::\n"]), [""]);
    assert!(scan(&[b"no markers\nat all"]).is_empty());
}

#[test]
fn an_overlong_marker_line_keeps_its_head_and_the_next_line_still_counts() {
    let mut long = b"::tasty-report::".to_vec();
    long.extend(std::iter::repeat_n(b'x', super::MARKER_LINE_CAP * 2));
    long.extend_from_slice(b"\n::tasty-report::next\n");
    let got = scan(&[&long]);
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0].len(),
        super::MARKER_LINE_CAP - "::tasty-report::".len()
    );
    assert_eq!(got[1], "next");
}

/// 줄 상한을 넘어 버린 바이트는 수로 넘어가고(줄 끝 `\r` 제외), append 상한이 자른 수와
/// 합쳐 `omit_by_limit` 이 된다. 버린 것이 없는 줄은 0 이다.
#[test]
fn bytes_dropped_past_the_line_cap_are_counted_into_omit_by_limit() {
    let prefix = "::tasty-report::";
    let body = "a".repeat(70_000);
    let line = format!("{prefix}{body}\r\n{prefix}short\r\n");
    let got = scan_cut(&[line.as_bytes()]);
    let kept = super::MARKER_LINE_CAP - prefix.len();
    assert_eq!(got[0].0.len(), kept);
    assert_eq!(got[0].1, (body.len() - kept) as u64);
    assert_eq!(got[1], ("short".to_string(), 0));

    let mut block = tasty_agent::task::report::ReportBlock::new(1);
    let limits = tasty_agent::task::report::ReportLimits::default();
    let outcome = block.append_cut(
        tasty_agent::task::report::ReportSource::StderrMarker,
        &got[0].0,
        got[0].1,
        limits,
        1,
    );
    // 저장한 1024 B 외의 본문 전부가 잘린 수다.
    assert_eq!(
        block.entries[0].omit_by_limit,
        Some(body.len() as u64 - 1024)
    );
    assert!(matches!(
        outcome,
        tasty_agent::task::report::AppendOutcome::Stored {
            omit_by_limit: Some(_),
            ..
        }
    ));
}

#[cfg(unix)]
mod run {
    use serde_json::{Value, json};
    use tasty_agent::TaskState;
    use tasty_agent::runner::RunnerLoop;
    use tasty_agent::task::report::{ReportAddress, ReportSource};
    use tasty_agent::task::{TaskGraphSpec, TaskStore};

    use super::super::super::tests::fresh_ctx;
    use super::super::super::*;

    fn store_op<R>(ctx: &RunnerContext, f: impl FnOnce(&mut TaskStore) -> R) -> R {
        ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            f(&mut store)
        })
    }

    fn get(ctx: &RunnerContext, id: &str) -> Task {
        store_op(ctx, |s| s.get(1, &id.to_string()).unwrap().expect("task"))
    }

    fn run_to_end(ctx: &RunnerContext, graph: Value, id: &str) -> Task {
        let spec: TaskGraphSpec = serde_json::from_value(graph).unwrap();
        store_op(ctx, |s| s.submit_graph(1, spec, 0).unwrap());
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        for n in 0..200 {
            let snapshot = store_op(ctx, |s| s.list(1).unwrap());
            let (a, b) = (ctx.clone(), ctx.clone());
            runner.tick(
                1,
                10 + n,
                &snapshot,
                move |ws, id, st, n| store_op(&a, |s| s.set_state(ws, id, st, n).map(|_| ())),
                move |ws, id, c, n| store_op(&b, |s| s.complete(ws, id, c, n).map(|_| ())),
            );
            if get(ctx, id).state.is_terminal() {
                return get(ctx, id);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // 마지막 sleep 뒤 러너를 더 돌리지 않고 한 번 더 읽는다.
        let task = get(ctx, id);
        assert!(task.state.is_terminal(), "{id} did not finish");
        task
    }

    fn stream(t: &Task, name: &str) -> String {
        t.typed_result
            .as_ref()
            .unwrap()
            .raw
            .execution
            .as_ref()
            .unwrap()[name]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// run 은 자기 블록 주소를 환경 변수로 받고, stderr 의 표지 줄은 블록에 들어가며 stderr 에도 남는다.
    #[test]
    fn a_run_gets_its_address_and_its_stderr_markers_reach_its_block() {
        let (_td, ctx) = fresh_ctx();
        let script = "printf '%s' \"$TASTY_TASK_REPORT\"; \
                      echo 'plain' >&2; echo '::tasty-report::checked 3 files' >&2; \
                      printf '::tasty-report::no newline' >&2";
        let t = run_to_end(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "r", "command": {"kind": "run", "workspace_id": 1,
                    "command": ["sh", "-c", script]}}]}),
            "r",
        );
        assert_eq!(t.state, TaskState::Succeeded);
        let addr = ReportAddress::parse(&stream(&t, "stdout")).expect("address");
        assert_eq!(addr.task_id, "r");
        assert_eq!(addr.attempt, 1);
        assert_eq!(addr.source, ReportSource::Run);
        assert_eq!(addr.token, t.report_token.as_ref().unwrap().token);
        assert!(stream(&t, "stderr").contains("::tasty-report::checked 3 files"));
        let blocks = store_op(&ctx, |s| s.report_blocks(&t).unwrap());
        let entries: Vec<_> = blocks[0]
            .entries
            .iter()
            .map(|e| (e.source, e.text.as_str()))
            .collect();
        assert_eq!(
            entries,
            [
                (ReportSource::StderrMarker, "checked 3 files"),
                (ReportSource::StderrMarker, "no newline")
            ]
        );
    }

    /// 블록이 가득 차면 뒤 표지 줄은 저장소를 거치지 않고 세기만 하다가, 끝에 한 번에 더한다.
    #[test]
    fn markers_past_a_full_block_are_counted_once_at_the_end() {
        let (_td, ctx) = fresh_ctx();
        let spec: TaskGraphSpec = serde_json::from_value(json!({"contract_version": 2, "tasks": [
            {"id": "f", "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]}))
        .unwrap();
        store_op(&ctx, |s| s.submit_graph(1, spec, 0).unwrap());
        let id = "f".to_string();
        store_op(&ctx, |s| {
            s.issue_report_token(
                1,
                &id,
                tasty_agent::task::report::ReportToken {
                    attempt: 1,
                    token: "tk".into(),
                },
            )
            .unwrap();
            s.set_state(1, &id, TaskState::Running, 1).unwrap();
        });
        let addr = ReportAddress::parse("1/1/stderr_marker/tk/f").unwrap();
        let mut sink = super::super::MarkerSink::new(
            ctx.memory.clone(),
            ctx.agent_seq.clone(),
            ctx.report_limits.clone(),
            addr,
        );
        let limits = ctx.report_limits.get();
        let line = format!(
            "::tasty-report::{}\n",
            "x".repeat(limits.append_bytes as usize)
        );
        let fill = (limits.block_bytes / limits.append_bytes) as usize;
        for _ in 0..fill + 5 {
            sink.observe(line.as_bytes());
        }
        let block = |ctx: &RunnerContext| {
            let task = get(ctx, "f");
            store_op(ctx, |s| s.report_blocks(&task).unwrap()).remove(0)
        };
        // 가득 찬 뒤의 다섯 줄은 아직 저장소에 닿지 않았다.
        let before = block(&ctx);
        assert_eq!(before.entries.len(), fill);
        assert_eq!(before.omitted_appends, 0);
        sink.finish();
        let after = block(&ctx);
        assert_eq!(after.entries.len(), fill);
        assert_eq!(after.omitted_appends, 5);
    }

    /// reduce 의 custom 셸도 같은 블록 주소를 받는다.
    #[test]
    fn a_custom_reduce_shell_gets_the_address_of_its_own_block() {
        let (_td, ctx) = fresh_ctx();
        let t = run_to_end(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "a", "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}},
                {"id": "m", "command": {"kind": "reduce", "inputs": ["a"], "strategy": {
                    "kind": "custom",
                    "command": "printf '\"%s\"' \"$TASTY_TASK_REPORT\""}},
                 "output_schema": {"type": "string"}}]}),
            "m",
        );
        assert_eq!(t.state, TaskState::Succeeded, "{:?}", t.typed_result);
        let out = t.typed_result.as_ref().unwrap().output.to_wire();
        let addr = ReportAddress::parse(out.as_str().unwrap()).expect("address");
        assert_eq!(addr.task_id, "m");
        assert_eq!(addr.source, ReportSource::ReduceCustom);
    }
}
