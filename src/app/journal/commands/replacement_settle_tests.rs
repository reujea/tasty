//! A replacement publishes its structure without waiting for the replaced owner to retire, and an
//! installation failure leaves only its own target unavailable instead of pausing observation.
//! Every scenario runs the public respawn through the production journal pump.
use super::*;
use crate::plugin_bridge::host_cmd::HostCmd;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::effect_runner::{INSTALL_FAULT, InstallFault};

const PLUGIN: &str = "com.test.replacement-settle";

fn terminal_workspace(journal: &mut JournalApplication, session: &mut EngineSession) -> u32 {
    let rx = send(
        journal,
        request(
            "workspace.create",
            serde_json::json!({"name":"replaced"}),
            None,
            700,
        ),
    );
    assert!(finish(journal, session, &rx).error.is_none());
    session
        .core_state
        .workspaces()
        .iter()
        .last()
        .unwrap()
        .all_surface_ids()[0]
}

fn respawn(journal: &mut JournalApplication, surface: u32, id: u64) -> Receiver {
    send(
        journal,
        request(
            "surface.respawn_terminal",
            serde_json::json!({"surface_id":surface}),
            None,
            id,
        ),
    )
}

type Receiver = std::sync::mpsc::Receiver<JsonRpcResponse>;

/// One production pump turn: the worker pump, then the window's resolution of new requests.
fn turn(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    plugins: Option<&mut crate::plugin::PluginManager>,
) {
    journal
        .poll_bootstrap(&mut [&mut *session], plugins)
        .unwrap();
    for (ticket, request) in journal.requests_needing_resolution() {
        match request.method.as_str() {
            "surface.respawn_terminal" | "pty.attach_surface" => {
                journal.resolve_public_creation(ticket, session)
            }
            "workspace.create" => journal.resolve_workspace_creation(ticket, session, None),
            _ => journal.resolve_ipc_for_engine(ticket, session),
        }
    }
}

fn reply(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    rx: &Receiver,
    mut observe: impl FnMut(&JournalApplication, &EngineSession),
) -> JsonRpcResponse {
    let mut stall = StallBudget::new(journal);
    loop {
        turn(journal, session, None);
        observe(journal, session);
        if let Ok(response) = rx.try_recv() {
            return response;
        }
        stall.nap("replacement reply");
    }
}

fn inject(fault: InstallFault) {
    INSTALL_FAULT.with(|slot| slot.set(Some(fault)));
}

fn injected() -> Option<InstallFault> {
    INSTALL_FAULT.with(|slot| slot.get())
}

fn activation(session: &EngineSession, surface: u32) -> Option<u64> {
    session
        .core_state
        .find_surface_by_id(surface)
        .and_then(|descriptor| descriptor.activation_generation)
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the PID exists; it delivers nothing.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[test]
fn a_failure_before_any_owner_changes_rejects_the_candidate_and_keeps_the_old_owner() {
    let (mut session, mut journal) = boot();
    let surface = terminal_workspace(&mut journal, &mut session);
    let old = session.runtime.terminals.generation(surface).unwrap();
    let before = activation(&session, surface);
    inject(InstallFault::BeforeChange);
    let rx = respawn(&mut journal, surface, 701);
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert_eq!(
        injected(),
        None,
        "the production installation reached the fault"
    );
    let error = response.error.expect("a known rejection is a failed reply");
    assert!(error.message.contains("injected"), "{error:?}");
    assert_ne!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED
    );
    assert_eq!(session.runtime.terminals.generation(surface), Some(old));
    assert_eq!(activation(&session, surface), before);
    assert!(session.pending_materializations.is_empty());
    assert!(!journal.pauses_observation());
    assert!(!journal.is_halted());
}

#[test]
fn a_failure_after_the_swap_withdraws_the_candidate_and_marks_only_its_target() {
    let (mut session, mut journal) = boot();
    let surface = terminal_workspace(&mut journal, &mut session);
    let other = terminal_workspace(&mut journal, &mut session);
    let other_generation = session.runtime.terminals.generation(other);
    let before = activation(&session, surface);
    inject(InstallFault::AfterSwap);
    let rx = respawn(&mut journal, surface, 702);
    let mut uncertain_seen = false;
    let response = reply(&mut journal, &mut session, &rx, |journal, _| {
        if injected().is_none() {
            uncertain_seen = true;
            // The withdrawn replacement never pauses observation while it reconciles.
            assert!(!journal.pauses_observation());
        }
    });
    assert!(uncertain_seen);
    let error = response.error.expect("the withdrawn replacement failed");
    assert!(error.message.contains("injected"), "{error:?}");
    assert_eq!(
        activation(&session, surface),
        before,
        "the structure is unchanged"
    );
    assert!(
        session.runtime.terminals.generation(surface).is_none(),
        "neither the retired owner nor the withdrawn candidate is live"
    );
    assert!(
        crate::runtime::surface_restorer::recovery_blocked_reason(&session, surface).is_some(),
        "the target whose owner is gone is shown as unavailable"
    );
    assert_eq!(
        session.runtime.terminals.generation(other),
        other_generation
    );
    assert!(session.pending_materializations.is_empty());
    assert!(!journal.is_halted());
    let next = send(
        &mut journal,
        request("workspace.create", serde_json::json!({}), None, 703),
    );
    assert!(finish(&mut journal, &mut session, &next).error.is_none());
}

#[cfg(unix)]
#[test]
fn the_reply_of_a_published_respawn_follows_the_reap_of_the_replaced_process() {
    let (mut session, mut journal) = boot();
    let surface = terminal_workspace(&mut journal, &mut session);
    let old = session.runtime.terminals.generation(surface).unwrap();
    let pid = session
        .runtime
        .terminals
        .pty(surface)
        .and_then(|pty| pty.process_id())
        .expect("old shell process");
    let rx = respawn(&mut journal, surface, 704);
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert!(response.error.is_none(), "{response:?}");
    assert_ne!(session.runtime.terminals.generation(surface), Some(old));
    assert!(
        !process_exists(pid),
        "the reply is not delivered before the replaced process is reaped"
    );
    assert!(!journal.pauses_observation());
    assert!(session.engine_release.is_none());
}

/// Binds the published surface to a remote owner whose stub never acknowledges a destroy.
fn unacknowledging_remote(
    session: &mut EngineSession,
    surface: u32,
    namespace: &str,
) -> crate::plugin::PluginManager {
    let mut plugins = crate::plugin::PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let stub = plugins.attach_namespace_stub_for_test(PLUGIN, namespace);
    let remote = RemoteSurface::new(surface, "terminal", PLUGIN.into(), "remote".into());
    plugins
        .host_cmd_tx
        .send(HostCmd::RemoteSurfaceCreated {
            surface_id: surface,
            plugin_id: PLUGIN.into(),
            kind: "terminal".into(),
            cwd: None,
            params: serde_json::json!({}),
            handles: remote.handles(),
        })
        .unwrap();
    session.runtime.surfaces.insert(surface, Box::new(remote));
    plugins.pump(Instant::now());
    assert_eq!(stub.drain_invokes().len(), 1, "the create was published");
    plugins
}

/// The failed installation's only evidence is a destroy receipt that never arrives. The
/// observation window ends, the owner moves to the engine release, and the caller is told.
#[test]
fn a_failed_installation_without_a_receipt_answers_recovery_required_and_stops_polling() {
    let (mut session, mut journal) = boot();
    let surface = terminal_workspace(&mut journal, &mut session);
    let mut plugins = unacknowledging_remote(&mut session, surface, "failed_install");
    inject(InstallFault::AfterSwap);
    let rx = respawn(&mut journal, surface, 706);
    let mut stall = StallBudget::new(&journal);
    let response = loop {
        turn(&mut journal, &mut session, Some(&mut plugins));
        assert!(!journal.pauses_observation());
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        stall.nap("failed installation without a receipt");
    };
    assert_eq!(
        injected(),
        None,
        "the production installation reached the fault"
    );
    let error = response
        .error
        .expect("an unreconciled installation is not a success");
    assert_eq!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED,
        "{error:?}"
    );
    assert!(error.message.contains("injected"), "{error:?}");
    assert!(
        session.engine_release.is_some(),
        "the withdrawn owner has one later owner"
    );
    assert!(!journal.has_creation(session.id));
    assert!(journal.cleanup_poll_deadline().is_none());
    assert!(!journal.is_halted());
}

#[test]
fn an_unacknowledged_plugin_destroy_neither_pauses_observation_nor_reports_success() {
    let (mut session, mut journal) = boot();
    let surface = terminal_workspace(&mut journal, &mut session);
    let mut plugins = unacknowledging_remote(&mut session, surface, "replacement_settle");
    let before = activation(&session, surface);

    let rx = respawn(&mut journal, surface, 705);
    let mut stall = StallBudget::new(&journal);
    let mut published_while_retiring = false;
    let response = loop {
        turn(&mut journal, &mut session, Some(&mut plugins));
        if activation(&session, surface) != before {
            // The new activation is published while the old destroy is still unanswered.
            assert!(!journal.pauses_observation());
            if journal.has_creation(session.id) {
                assert!(
                    journal.shutdown_waits_for_publication(),
                    "shutdown waits for the settling receipt within its deadline"
                );
            }
            published_while_retiring = true;
        }
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        stall.nap("unacknowledged replaced owner");
    };
    assert!(published_while_retiring);
    let error = response
        .error
        .expect("an unconfirmed retirement is not a success");
    assert_eq!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED
    );
    assert!(session.runtime.terminals.generation(surface).is_some());
    assert!(
        session.engine_release.is_some(),
        "the unconfirmed receipt has one later owner"
    );
    assert!(!journal.has_creation(session.id));
    assert!(!journal.pauses_observation());
    assert!(!journal.shutdown_waits_for_publication());
}

/// A standalone PTY owned by an agent, as `pty.spawn` registers it.
#[cfg(unix)]
fn standalone_pty(session: &mut EngineSession) -> (u32, u32, tasty_terminal::ResourceGeneration) {
    let pty_id = session.runtime.terminals.reserve_pty_id().unwrap();
    let (terminal, mut pty) = tasty_terminal::spawn_terminal(
        tasty_terminal::TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args: &["-c", "sleep 60"],
            surface_id: pty_id,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        std::sync::Arc::new(|| {}),
    )
    .unwrap();
    let pid = pty.process_id().expect("standalone process");
    let generation = pty.generation();
    let now = Instant::now();
    pty.set_standalone(tasty_terminal::StandalonePty {
        owner_agent_id: "agent".into(),
        cwd: None,
        command: vec!["sleep".into()],
        created_at: now,
        last_activity: now,
        idle_timeout: None,
        exited_at: None,
    });
    session
        .runtime
        .terminals
        .insert(pty_id, terminal, Some(pty));
    (pty_id, pid, generation)
}

#[cfg(unix)]
fn adopt(journal: &mut JournalApplication, session: &EngineSession, pty: u32, id: u64) -> Receiver {
    let pane = session
        .core_state
        .workspaces()
        .iter()
        .last()
        .unwrap()
        .pane_layout()
        .all_pane_ids()[0];
    send(
        journal,
        request(
            "pty.attach_surface",
            serde_json::json!({"pane_id":pane,"id":pty}),
            None,
            id,
        ),
    )
}

/// The adopted process belongs to the agent that spawned it. A failure after the swap returns it
/// to its standalone registration instead of retiring it after the observation window.
#[cfg(unix)]
#[test]
fn a_failed_adoption_returns_the_standalone_process_alive() {
    let (mut session, mut journal) = boot();
    terminal_workspace(&mut journal, &mut session);
    let (pty_id, pid, _) = standalone_pty(&mut session);
    inject(InstallFault::AfterSwap);
    let rx = adopt(&mut journal, &session, pty_id, 707);
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert_eq!(
        injected(),
        None,
        "the production installation reached the fault"
    );
    let error = response.error.expect("a failed adoption is not a success");
    assert!(error.message.contains("injected"), "{error:?}");
    assert!(
        session.runtime.terminals.is_standalone(pty_id),
        "the process returned to its standalone registration"
    );
    assert_eq!(
        session
            .runtime
            .terminals
            .pty(pty_id)
            .and_then(|pty| pty.process_id()),
        Some(pid)
    );
    assert!(process_exists(pid));
    assert!(!journal.has_creation(session.id));
    assert!(!journal.is_halted());
}

/// The prepared candidate that holds `generation` before its installation: its operation and the
/// surface it will install to.
#[cfg(unix)]
fn private_candidate(
    session: &EngineSession,
    generation: tasty_terminal::ResourceGeneration,
) -> Option<(tasty_core::OperationId, Option<u32>)> {
    session
        .pending_materializations
        .iter()
        .find_map(|(operation, candidate)| {
            let (_, pty) = candidate.connection.as_ref()?;
            (pty.generation() == generation).then(|| (operation.clone(), candidate.surface_id()))
        })
}

/// The terminal ID other than `pty_id` that holds `generation`, once an installation moved it.
#[cfg(unix)]
fn installed_elsewhere(
    session: &EngineSession,
    pty_id: u32,
    generation: tasty_terminal::ResourceGeneration,
) -> Option<u32> {
    session.runtime.terminals.iter().find_map(|(id, _)| {
        let pty = session.runtime.terminals.pty(id)?;
        (id != pty_id && pty.generation() == generation).then_some(id)
    })
}

/// Pumps until the adoption candidate holding `generation` is private and returns its operation and
/// surface. With `hold_while_private` set to that generation, the pump then stops taking
/// completions, so no installation can start while the test works on that state.
#[cfg(unix)]
fn claim_privately(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    pty_id: u32,
    generation: tasty_terminal::ResourceGeneration,
) -> (tasty_core::OperationId, u32) {
    let mut stall = StallBudget::new(journal);
    loop {
        turn(journal, session, None);
        if let Some(installed) = installed_elsewhere(session, pty_id, generation) {
            panic!(
                "the installation moved {generation:?} to surface {installed} before its candidate \
                 was seen private"
            );
        }
        if let Some((operation, surface)) = private_candidate(session, generation) {
            return (operation, surface.expect("prepared surface has a fixed ID"));
        }
        stall.nap_with(|| {
            format!(
                "adoption claims the standalone owner ({} candidates, standalone {} present)",
                session.pending_materializations.len(),
                session.runtime.terminals.contains(pty_id)
            )
        });
    }
}

/// Holds the adoption while its candidate is private, lets another owner take the standalone ID so
/// the withdrawal or discard cannot return the process, and only then lets the installation run.
#[cfg(unix)]
fn occupy_after_claim(
    journal: &mut JournalApplication,
    session: &mut EngineSession,
    pty_id: u32,
    generation: tasty_terminal::ResourceGeneration,
) {
    assert_eq!(journal.hold_while_private, Some(generation));
    claim_privately(journal, session, pty_id, generation);
    assert!(!session.runtime.terminals.contains(pty_id));
    assert!(
        injected().is_some(),
        "the occupant must be in place before the installation runs"
    );
    session
        .runtime
        .terminals
        .insert(pty_id, tasty_terminal::Terminal::new_detached(80, 24), None);
    journal.hold_while_private = None;
}

/// Where the engine release holds this PTY generation: kept alive outside the engine, retired with
/// its observation, or neither (the generation is in neither the kept list nor the retirement
/// receipts).
#[cfg(unix)]
fn release_ownership(
    session: &EngineSession,
    generation: tasty_terminal::ResourceGeneration,
) -> String {
    let Some(release) = session.engine_release.as_ref() else {
        return format!("{generation:?}: no engine release");
    };
    let kept = release.unreturned_pty_generations().contains(&generation);
    let retired: Vec<_> = release
        .retained_pty_receipts()
        .into_iter()
        .filter(|(retired, _)| *retired == generation)
        .map(|(_, observation)| observation)
        .collect();
    let path = match (kept, retired.is_empty()) {
        (true, true) => "kept",
        (false, false) => "retired",
        (true, false) => "kept and retired",
        (false, true) => "neither",
    };
    format!("{generation:?} {path}; retired observations {retired:?}")
}

/// The engine release must finish in bounded time while the unreturned process stays alive,
/// including after the released session is dropped. `kept` also requires that the release itself
/// holds the process.
#[cfg(unix)]
fn release_engine_keeping(
    mut session: EngineSession,
    pid: u32,
    generation: tasty_terminal::ResourceGeneration,
    kept: bool,
) {
    let until = Instant::now() + std::time::Duration::from_secs(10);
    // holds_by_every 로 바꾸지 않는다. 완료는 poll_engine_release 의 반환값으로만 보이고 그 poll 은 소유자
    // 회수를 함께 진행해, 기한 뒤 진행 없이 관측할 저장 상태가 없다.
    while !crate::runtime::resource_retirement::poll_engine_release(&mut session, None) {
        assert!(
            Instant::now() < until,
            "the engine release never finishes while an unreturned adoption exists: {}",
            release_ownership(&session, generation)
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        process_exists(pid),
        "the engine release never kills it: {}",
        release_ownership(&session, generation)
    );
    let ownership = release_ownership(&session, generation);
    assert!(
        !kept || ownership.contains(" kept;"),
        "the engine release keeps the unreturned process: {ownership}"
    );
    // Only the GUI releases an engine before the process ends.
    #[cfg(feature = "gui")]
    {
        let mut engines = crate::app::engine_registry::EngineRegistry::default();
        let id = engines
            .insert_pending(session)
            .unwrap_or_else(|_| panic!("the registry has no other pending engine"));
        assert!(engines.begin_retiring_pending(id));
        engines.mark_retiring_release(id);
        drop(engines.finish_retiring(id).expect("released owner"));
        assert_eq!(engines.unreturned_process_ids(), vec![Some(pid)]);
        assert!(
            process_exists(pid),
            "dropping the released session never kills it"
        );
        // A kept process that ends on its own is reaped when a later engine release finishes.
        // SAFETY: the test owns this child process; SIGKILL only ends it.
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        let until = Instant::now() + std::time::Duration::from_secs(10);
        // 기한 뒤 읽기는 새 엔진을 만들지 않고 직전 회수 결과와 프로세스만 본다.
        let observed = tasty_utils::test_wait::holds_by_every(
            until,
            std::time::Duration::from_millis(50),
            |last| {
                if !last {
                    let mut later = EngineSession::new(80, 24, std::sync::Arc::new(|| {}))
                        .expect("later engine");
                    later.engine_release = Some(Default::default());
                    let id = engines
                        .insert_pending(later)
                        .unwrap_or_else(|_| panic!("the registry has no other pending engine"));
                    assert!(engines.begin_retiring_pending(id));
                    engines.mark_retiring_release(id);
                    drop(engines.finish_retiring(id).expect("released owner"));
                }
                engines.unreturned_process_ids().is_empty() && !process_exists(pid)
            },
        );
        assert!(
            observed,
            "an exited kept process is never observed: kept {:?}, zombie {}",
            engines.unreturned_process_ids(),
            process_exists(pid)
        );
    }
}

/// A halt while an adoption candidate is still private must not kill the standalone process; the
/// halted release keeps it like one that could not return.
#[cfg(unix)]
#[test]
fn a_halt_keeps_a_pending_adoption_candidate_alive() {
    let (mut session, mut journal) = boot();
    terminal_workspace(&mut journal, &mut session);
    let (pty_id, pid, generation) = standalone_pty(&mut session);
    journal.hold_while_private = Some(generation);
    let _rx = adopt(&mut journal, &session, pty_id, 711);
    let (_, surface) = claim_privately(&mut journal, &mut session, pty_id, generation);
    assert!(!session.runtime.terminals.contains(pty_id));
    assert!(
        !session.runtime.terminals.contains(surface),
        "the private candidate {surface} still holds {generation:?} before the halt"
    );
    journal.halted = Some("halted while the adoption candidate is private".into());
    assert!(journal.release_halted_resources(&mut session, None));
    assert!(session.pending_materializations.is_empty());
    release_engine_keeping(session, pid, generation, true);
}

/// A rejection before any owner changed discards the candidate. When the registration is taken
/// the process cannot return, yet the creation ends and the engine release still finishes.
#[cfg(unix)]
#[test]
fn a_rejected_adoption_that_cannot_return_does_not_block_the_engine_release() {
    let (mut session, mut journal) = boot();
    terminal_workspace(&mut journal, &mut session);
    let (pty_id, pid, generation) = standalone_pty(&mut session);
    inject(InstallFault::BeforeChange);
    journal.hold_while_private = Some(generation);
    let rx = adopt(&mut journal, &session, pty_id, 709);
    occupy_after_claim(&mut journal, &mut session, pty_id, generation);
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert_eq!(injected(), None, "the installation reached the fault");
    let error = response
        .error
        .expect("an unreturned adoption is not a success");
    assert_eq!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED
    );
    assert!(!journal.has_creation(session.id));
    assert!(!journal.pauses_observation());
    assert!(session.pending_materializations.is_empty());
    assert!(process_exists(pid));
    release_engine_keeping(session, pid, generation, false);
}

/// After the swap the withdrawal cannot return the process either. The creation ends once its
/// uncertainty is recorded instead of holding the engine, and the process is never killed.
#[cfg(unix)]
#[test]
fn a_failed_adoption_that_cannot_return_does_not_block_the_engine_release() {
    let (mut session, mut journal) = boot();
    terminal_workspace(&mut journal, &mut session);
    let (pty_id, pid, generation) = standalone_pty(&mut session);
    inject(InstallFault::AfterSwap);
    journal.hold_while_private = Some(generation);
    let rx = adopt(&mut journal, &session, pty_id, 710);
    occupy_after_claim(&mut journal, &mut session, pty_id, generation);
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert_eq!(injected(), None, "the installation reached the fault");
    let error = response
        .error
        .expect("an unreturned adoption is not a success");
    assert_eq!(
        error.code,
        crate::ipc::protocol::ERR_OPERATION_RECOVERY_REQUIRED
    );
    assert!(!journal.has_creation(session.id));
    #[cfg(feature = "gui")]
    assert!(!journal.has_pending_engine_effects(session.id));
    assert!(process_exists(pid));
    release_engine_keeping(session, pid, generation, false);
}

/// The gate that holds a private candidate keeps its installation from starting even after the
/// worker has sent that installation's Publish, and releasing the gate lets the same installation
/// finish. Without the gate the turn after the send takes the Publish and installs.
#[cfg(unix)]
#[test]
fn a_held_private_candidate_keeps_its_sent_installation_waiting() {
    let (mut session, mut journal) = boot();
    terminal_workspace(&mut journal, &mut session);
    let (pty_id, _, generation) = standalone_pty(&mut session);
    journal.hold_while_private = Some(generation);
    let rx = adopt(&mut journal, &session, pty_id, 712);
    let (operation, surface) = claim_privately(&mut journal, &mut session, pty_id, generation);
    let sent = |journal: &JournalApplication| {
        journal
            .worker
            .installs_sent
            .lock()
            .unwrap()
            .contains(&operation)
    };
    let mut stall = StallBudget::new(&journal);
    loop {
        turn(&mut journal, &mut session, None);
        // Read after the pump and right before the stall verdict, with nothing in between: a send
        // recorded while that pump ran must end the wait instead of failing it as a stall.
        if sent(&journal) {
            break;
        }
        stall.nap("the worker sends the adoption's installation");
    }
    // The send may have landed after this turn's last receive, so one more turn could take it.
    turn(&mut journal, &mut session, None);
    assert_eq!(
        installed_elsewhere(&session, pty_id, generation),
        None,
        "the installation ran while its candidate was held"
    );
    assert_eq!(
        private_candidate(&session, generation).map(|(held, _)| held),
        Some(operation),
        "the candidate left its private state while held"
    );
    journal.hold_while_private = None;
    let response = reply(&mut journal, &mut session, &rx, |_, _| {});
    assert!(response.error.is_none(), "{:?}", response.error);
    let installed = response
        .result
        .as_ref()
        .and_then(|result| result["surface_id"].as_u64());
    assert_eq!(installed, Some(u64::from(surface)));
    assert_eq!(
        installed_elsewhere(&session, pty_id, generation),
        Some(surface)
    );
}
