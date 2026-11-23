//! GUI와 헤드리스가 공유하는 PTY 종료 처리. 창이 없다는 이유로 호출하지 않는다.
use crate::app::services::AppServices;
use crate::runtime::engine_access::EngineMut;
use crate::state::RequestContext;

pub(crate) fn handle(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    surface: u32,
    generation: tasty_terminal::ResourceGeneration,
) {
    if !engine
        .runtime
        .terminals
        .matches_generation(surface, generation)
    {
        return;
    }
    // 원격 출처 자식의 종료는 구조 변경으로 매핑이 사라지기 전에 통지로 쌓는다.
    crate::remote::agent_children::relay(
        &engine.as_ref(),
        surface,
        tasty_ipc::agent_child::Notice::Event {
            name: tasty_ipc::agent_child::PROCESS_EXIT.into(),
        },
    );
    // 두 호스트 모두 HookFired로 작업 대기자를 깨우며 GUI는 이벤트도 방송한다.
    let exec = core.hook_executor();
    for fired in engine
        .hooks
        .fire(&exec, surface, tasty_hooks::HookEvent::ProcessExit)
    {
        engine.enqueue_host_event(fired);
    }
    #[cfg(feature = "gui")]
    engine.enqueue_host_event(crate::state::PendingHostEvent::ProcessExited {
        surface_id: surface,
    });
    // 종료한 프로세스는 복원 스냅샷에 남기지 않는다.
    // intent-exempt: explicit PTY exit cascade, not a new user or agent command
    state.dispatch_intent(
        crate::app::command::DomainIntent::RetireExitedSurface {
            surface_id: surface,
            generation,
        }
        .from_system(),
    );
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::app::command::CoreEvent;
    use std::time::{Duration, Instant};

    fn process(surface: u32, command: &str) -> (tasty_terminal::Terminal, tasty_terminal::Pty) {
        tasty_terminal::spawn_terminal(
            tasty_terminal::TerminalConfig {
                cols: 80,
                rows: 24,
                shell: Some("/bin/sh"),
                args: &["-c", command],
                surface_id: surface,
                working_dir: None,
                initial_input: None,
                extra_env: &[],
            },
            std::sync::Arc::new(|| {}),
        )
        .expect("actual child")
    }

    /// Retain a real exit result through respawn, then use the production final close boundary.
    #[test]
    fn an_old_exit_callback_cannot_close_a_replacement_with_the_same_surface_id() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let (mut core, _home) = crate::adapters::ipc::handler::pty::tests::core();
        let mut engine = session.borrow_mut();
        let surface = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        engine
            .replace_terminal_by_id(surface, process(surface, "exit 7"))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        // process_pty_output 은 이미 나온 출력과 자식 종료를 거둬 사건으로 꺼낼 뿐 새 요청을 만들지 않는다.
        let mut exited = None;
        assert!(
            tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(5), |_| {
                let output = core.process_pty_output(&mut engine, surface);
                exited = output.events.into_iter().find_map(|event| match event {
                    CoreEvent::TerminalProcessExited {
                        surface_id,
                        generation,
                    } if surface_id == surface => Some(generation),
                    _ => None,
                });
                exited.is_some()
            }),
            "real child exit was not observed"
        );
        let generation = exited.expect("the wait held");
        let replacement = process(surface, "exec sleep 60");
        let new_pid = replacement.1.process_id().expect("live replacement PID");
        let new_generation = replacement.1.generation();
        assert_ne!(generation, new_generation);
        engine.replace_terminal_by_id(surface, replacement).unwrap();
        drop(engine.take_pending_host_events());
        handle(&mut core, &mut state, &mut engine, surface, generation);
        assert!(
            engine.has_surface(surface),
            "late exit must not delete the new structure"
        );
        assert_eq!(
            engine.runtime.terminals.pty(surface).unwrap().process_id(),
            Some(new_pid)
        );
        assert!(
            engine
                .runtime
                .terminals
                .pty_mut(surface)
                .unwrap()
                .is_alive()
        );
        assert!(
            engine.take_pending_host_events().is_empty(),
            "late exit must not publish a new process-exit event"
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn queued_title_and_cwd_observations_keep_their_original_generation() {
        use crate::app::command::DomainIntent;
        let (_state, mut session) = crate::state::tests::test_state();
        let (mut core, _home) = crate::adapters::ipc::handler::pty::tests::core();
        let mut engine = session.borrow_mut();
        let surface = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let old = engine.runtime.terminals.generation(surface).unwrap();
        let title = |engine: &EngineMut<'_>| {
            engine
                .live
                .surface_titles
                .get(&surface)
                .and_then(|titles| titles.osc_title.clone())
        };
        let original_title = title(&engine);
        let pending_title = DomainIntent::UpdateTabName {
            surface_id: surface,
            generation: old,
            name: "old-title".into(),
        };
        let pending_cwd = DomainIntent::SurfaceCwdChanged {
            surface_id: surface,
            generation: old,
        };
        engine
            .replace_terminal_by_id(surface, process(surface, "exec sleep 60"))
            .unwrap();
        assert!(
            core.apply_live(&mut engine, pending_title)
                .unwrap()
                .is_empty()
        );
        assert!(
            core.apply_live(&mut engine, pending_cwd)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            title(&engine),
            original_title,
            "a dropped result must also leave the model unchanged"
        );
        let current = engine.runtime.terminals.generation(surface).unwrap();
        assert!(
            !core
                .apply_live(
                    &mut engine,
                    DomainIntent::UpdateTabName {
                        surface_id: surface,
                        generation: current,
                        name: "current-title".into()
                    }
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(title(&engine).as_deref(), Some("current-title"));
        let current_cwd = core
            .apply_live(
                &mut engine,
                DomainIntent::SurfaceCwdChanged {
                    surface_id: surface,
                    generation: current,
                },
            )
            .unwrap();
        assert_eq!(current_cwd.len(), 1);
        assert_eq!(current_cwd[0].terminal_binding(), Some((surface, current)));
    }
}
