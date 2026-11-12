use crate::app::command::CoreEvent;

/// 셸 통합이 보내는 바이트를 surface에 넣고 drain해 명령 완료 이벤트 수를 센다.
fn completions(
    core: &mut crate::app::services::AppServices,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
    bytes: &[u8],
) -> usize {
    engine
        .find_terminal_by_id_mut(sid)
        .expect("terminal surface")
        .feed_bytes(bytes);
    let outcome = core.process_all_pty_output(engine);
    outcome
        .events
        .iter()
        .filter(|e| matches!(e, CoreEvent::TerminalCommandCompleted { surface_id, .. } if *surface_id == sid))
        .count()
}

/// zsh·bash 통합은 첫 프롬프트를 그리기 전에도 precmd가 `D;0`을 보낸다. 그 D는 명령 완료가 아니다.
#[test]
fn the_shell_startup_report_is_not_a_command_completion() {
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .build()
        .expect("core");
    let (state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = *engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .expect("workspace")
        .all_surface_ids()
        .first()
        .expect("surface");

    let startup = completions(
        &mut core,
        &mut engine,
        sid,
        b"\x1b]133;D;0\x07\x1b]133;A\x07",
    );
    assert_eq!(startup, 0, "첫 프롬프트 전 D가 명령 완료로 나갔다");

    let command = completions(
        &mut core,
        &mut engine,
        sid,
        b"\x1b]133;C;cmd=true\x07\x1b]133;D;0\x07\x1b]133;A\x07",
    );
    assert_eq!(command, 1, "프롬프트 뒤 명령의 D는 명령 완료다");
}

#[test]
#[cfg(feature = "gui")]
fn osc52_clipboard_result_retains_generation_and_rpc_failure_has_no_event() {
    use crate::clipboard::error::ClipboardError;
    use std::sync::Arc;
    struct Failing;
    impl crate::ports::clipboard::ClipboardSystem for Failing {
        fn read_text(&self) -> anyhow::Result<String> {
            Err(ClipboardError::BackendUnavailable("fixture".into()).into())
        }
        fn write_text(&self, _: &str) -> anyhow::Result<()> {
            Err(ClipboardError::BackendUnavailable("fixture".into()).into())
        }
    }
    for succeeds in [false, true] {
        let adapter: Arc<dyn crate::ports::clipboard::ClipboardSystem> = if succeeds {
            Arc::new(crate::adapters::test::mock_clipboard::MockClipboard::default())
        } else {
            Arc::new(Failing)
        };
        let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
            .with_clipboard(adapter)
            .build()
            .unwrap();
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let reply = crate::app::services::surface::clipboard_set_text(
            &core,
            serde_json::json!(1),
            &serde_json::json!({"text":"fixture"}),
        );
        assert_eq!(reply.error.is_none(), succeeds);
        if let Some(error) = reply.error {
            assert_eq!(error.code, -32000);
            assert!(error.message.contains("fixture"));
        }
        assert_eq!(state.toasts.len(), 0);
        assert!(
            core.process_all_pty_output(&mut engine)
                .events
                .iter()
                .all(|e| !matches!(e, CoreEvent::TerminalClipboardSet { .. }))
        );
        engine
            .find_terminal_by_id_mut(sid)
            .unwrap()
            .feed_bytes(b"\x1b]52;c;Zml4dHVyZQ==\x07");
        let events = core.process_all_pty_output(&mut engine).events;
        let event = events
            .iter()
            .find(|e| matches!(e, CoreEvent::TerminalClipboardSet { .. }))
            .expect("write result");
        let (bound_sid, generation) = event.terminal_binding().unwrap();
        assert_eq!(bound_sid, sid);
        assert!(engine.runtime.terminals.matches_generation(sid, generation));
        let CoreEvent::TerminalClipboardSet { result, .. } = event else {
            unreachable!()
        };
        assert_eq!(result.is_ok(), succeeds);
        if !succeeds {
            assert!(matches!(result, Err(ClipboardError::BackendUnavailable(_))));
        }
        state.clipboard_copy_feedback(
            result.clone(),
            "toast.copied_osc52",
            Vec::new(),
            crate::adapters::ui::ToastScope::Surface(sid),
        );
        let key = if succeeds {
            "toast.copied_osc52"
        } else {
            "toast.clipboard_unavailable"
        };
        assert_eq!(state.toasts.messages(), vec![crate::i18n::t(key)]);
    }
}

/// 훅·observer가 없는 surface는 `OutputAppended`를 만들지 않는다. 그래도 출력 뒤 마감이 지나면
/// 셸 통합 안내가 판정돼야 한다. 출력 이벤트 게이트가 꺼진 상태를 유지하는지도 확인한다.
/// 판정 대상은 로컬 PTY가 있는 surface이므로 출력이 없는 실제 자식을 붙인다.
#[test]
#[cfg(unix)]
fn shell_integration_hint_is_judged_without_output_events() {
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .build()
        .expect("core");
    let (state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = *engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .expect("workspace")
        .all_surface_ids()
        .first()
        .expect("surface");
    let local = tasty_terminal::spawn_terminal(
        tasty_terminal::TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args: &["-c", "exec sleep 60"],
            surface_id: sid,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        std::sync::Arc::new(|| {}),
    )
    .expect("local child");
    engine
        .replace_terminal_by_id(sid, local)
        .expect("replace terminal");
    assert!(!engine.observer_router.wants(sid));
    assert!(!engine.hooks.has_output_match_hook(sid));
    assert!(
        engine.runtime.terminals.pty(sid).is_some(),
        "판정 대상은 로컬 PTY가 있는 surface다"
    );

    // 게이트는 처리 회차 시작에 맞춰지므로 한 번 처리한 뒤 출력을 넣고 drain 전에 이벤트를 본다.
    core.process_all_pty_output(&mut engine);
    engine
        .find_terminal_by_id_mut(sid)
        .expect("terminal surface")
        .feed_bytes(b"$ echo hi\r\nhi\r\n$ ");
    let raw = engine.collect_events();
    assert!(
        !raw.iter().any(|e| matches!(
            e.kind,
            tasty_terminal::TerminalEventKind::OutputAppended { .. }
        )),
        "훅·observer가 없는 surface에서 출력 이벤트 게이트가 켜졌다"
    );

    assert!(engine.take_due_shell_integration_hints().is_empty());
    let first = engine
        .live
        .shell_integration_first_output_at
        .get_mut(&sid)
        .expect("출력 이벤트 없이도 첫 출력 시각을 기록해야 한다");
    *first = std::time::Instant::now() - std::time::Duration::from_secs(999);

    let generation = engine
        .runtime
        .terminals
        .generation(sid)
        .expect("generation");
    assert_eq!(
        engine.take_due_shell_integration_hints(),
        vec![(sid, generation)]
    );
    assert!(engine.take_due_shell_integration_hints().is_empty());
}

/// 로컬 PTY가 없는 attach mirror는 출력이 있고 마감이 지나도 셸 통합 안내를 요청하지 않는다.
#[test]
fn shell_integration_hint_skips_terminals_without_a_local_pty() {
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let mirror_sid = 9_001;
    let mut mirror = tasty_terminal::Terminal::new_detached(80, 24);
    mirror.feed_bytes(b"user@remote:~$ ");
    assert!(mirror.first_output_at().is_some());
    engine.runtime.terminals.insert(mirror_sid, mirror, None);
    engine.live.shell_integration_first_output_at.insert(
        mirror_sid,
        std::time::Instant::now() - std::time::Duration::from_secs(999),
    );

    assert!(
        !engine
            .take_due_shell_integration_hints()
            .iter()
            .any(|(sid, _)| *sid == mirror_sid),
        "로컬 PTY가 없는 mirror에 셸 통합 안내를 요청했다"
    );
}

/// Given 바로 읽을 수 없는 클립보드, When OSC 52 읽기 질의가 온다, Then 질의를 보관했다가 결과가 온
/// 뒤에만 그 터미널 세대와 함께 꺼낸다. 설정이 꺼져 있으면 읽기를 시작하지도 않는다.
#[test]
#[cfg(feature = "gui")]
fn a_deferred_osc52_read_is_kept_until_its_text_arrives() {
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Deferred {
        begun: Mutex<u32>,
        polls: Mutex<u32>,
    }
    impl crate::ports::clipboard::ClipboardSystem for Deferred {
        fn read_text(&self) -> anyhow::Result<String> {
            panic!("a deferred clipboard is not read synchronously")
        }
        fn write_text(&self, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn begin_read_text(&self) -> crate::ports::clipboard::TextRead {
            *self.begun.lock().unwrap() += 1;
            Err(7)
        }
        fn finish_read_text(&self, token: u64) -> Option<anyhow::Result<String>> {
            assert_eq!(token, 7);
            let mut polls = self.polls.lock().unwrap();
            *polls += 1;
            (*polls >= 2).then(|| Ok("fixture".to_string()))
        }
    }
    let clipboard = Arc::new(Deferred::default());
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .with_clipboard(clipboard.clone())
        .build()
        .unwrap();
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
    let query = b"\x1b]52;c;?\x07";

    engine
        .find_terminal_by_id_mut(sid)
        .unwrap()
        .feed_bytes(query);
    core.process_all_pty_output(&mut engine);
    assert_eq!(
        *clipboard.begun.lock().unwrap(),
        0,
        "설정이 꺼지면 읽지 않는다"
    );
    assert!(core.pending_clipboard_reads.is_empty());

    engine.runtime.settings.general.allow_clipboard_read = true;
    engine
        .find_terminal_by_id_mut(sid)
        .unwrap()
        .feed_bytes(query);
    core.process_all_pty_output(&mut engine);
    assert_eq!(*clipboard.begun.lock().unwrap(), 1);
    assert_eq!(core.pending_clipboard_reads.len(), 1);

    assert!(core.take_finished_clipboard_reads().is_empty());
    assert_eq!(core.pending_clipboard_reads.len(), 1, "결과 전에는 남긴다");
    let finished = core.take_finished_clipboard_reads();
    assert!(core.pending_clipboard_reads.is_empty());
    let [(read_sid, generation, Ok(text))] = finished.as_slice() else {
        panic!("one finished read expected");
    };
    assert_eq!((*read_sid, text.as_str()), (sid, "fixture"));
    assert!(
        engine
            .runtime
            .terminals
            .matches_generation(sid, *generation)
    );
}
