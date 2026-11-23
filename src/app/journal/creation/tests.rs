use super::*;
use crate::runtime::journal_product::EffectLease;
use crate::runtime::resource_retirement::EngineRelease;

fn lease() -> EffectLease {
    EffectLease {
        effect_id: "retained-effect".into(),
        operation: OperationId("retained-operation".into()),
        stream: "structure:slot-1".into(),
        runtime_epoch: 3,
        resource_generation: 4,
        attempt: 1,
    }
}

#[test]
fn uncertain_installation_release_waits_for_its_original_remote_receipt() {
    let binding = crate::plugin_bridge::host_cmd::MeshBinding::default();
    let (receipt, completion) =
        crate::plugin_bridge::host_cmd::RemoteRetirementReceipt::pending(1, binding.binding());
    let stage = Stage::Installing {
        installed: Installed::with_test_retirement(lease(), receipt),
        answered: true,
    };
    let owner = UncertainOwner::from_stage(stage).expect("installation owner");
    assert!(matches!(&owner, UncertainOwner::Installed(installed) if installed.lease == lease()));
    let mut release = EngineRelease::default();
    owner.retain_for_release(&mut release);
    let mut session = EngineSession::new(80, 24, Arc::new(|| {})).unwrap();
    assert!(
        !release.poll(&mut session, None),
        "enqueue or owner transfer is not a destroy ACK"
    );
    completion.finish(Ok(()));
    assert!(release.poll(&mut session, None));
}

#[cfg(unix)]
#[test]
fn uncertain_discard_release_retains_the_exact_pty_generation() {
    let (terminal, pty) = tasty_terminal::spawn_terminal(
        tasty_terminal::TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args: &["-c", "exit 0"],
            surface_id: 1,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        Arc::new(|| {}),
    )
    .unwrap();
    let generation = pty.generation();
    let retirement = pty.retire();
    drop(terminal);
    let owner = UncertainOwner::from_stage(Stage::Rejected {
        lease: lease(),
        retirement: Some(retirement),
        discard_committed: true,
        reason: "await original reap".into(),
        answered: true,
        started: std::time::Instant::now(),
    })
    .expect("discard owner");
    assert!(
        matches!(&owner, UncertainOwner::Discard { lease: actual, retirement: Some(receipt), discard_committed: true, reason } if *actual == lease() && receipt.generation() == generation && reason == "await original reap")
    );
    let mut release = EngineRelease::default();
    owner.retain_for_release(&mut release);
    assert_eq!(release.retained_pty_generations(), vec![generation]);
    let mut session = EngineSession::new(80, 24, Arc::new(|| {})).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // holds_by_every 로 바꾸지 않는다. 완료는 이 poll 의 반환값으로만 보이고 poll 은 종료 진행(재제출·회수)을
    // 함께 돌아, 기한 뒤 진행 없이 관측할 저장 상태가 없다.
    while !release.poll(&mut session, None) {
        assert!(
            std::time::Instant::now() < deadline,
            "exact child did not reap"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn publication_keeps_the_original_owner_with_either_completion_order() {
    for reply_first in [false, true] {
        let binding = crate::plugin_bridge::host_cmd::MeshBinding::default();
        let (receipt, completion) =
            crate::plugin_bridge::host_cmd::RemoteRetirementReceipt::pending(1, binding.binding());
        let installed = Installed::with_test_retirement(lease(), receipt);
        let mut stage = if reply_first {
            Stage::AwaitPublication(installed)
        } else {
            Stage::Finish {
                installed,
                publication: Publication::Pending,
            }
        };
        assert!(!stage.is_published());
        stage.mark_published();
        assert!(stage.is_published());
        assert_eq!(
            matches!(stage, Stage::Published(_)),
            reply_first,
            "the router releases only after both the reply and publication"
        );
        let mut release = EngineRelease::default();
        UncertainOwner::from_stage(stage)
            .unwrap()
            .retain_for_release(&mut release);
        let mut session = EngineSession::new(80, 24, Arc::new(|| {})).unwrap();
        assert!(!release.poll(&mut session, None));
        completion.finish(Ok(()));
        assert!(release.poll(&mut session, None));
    }
}

fn executed() -> ResultValue {
    ResultValue::Executed(crate::runtime::command_executor::Executed {
        command_id: "reply".into(),
        status: tasty_event_store::CommandStatus::Completed,
        response: None,
        source: crate::runtime::command_executor::Source::Committed { batch: None },
    })
}

/// Synthesizes the uncertain stage with a Discard owner instead of calling `discard_candidate`,
/// whose EngineOwned owner never reconciles. The Discard owner reconciles, so the full overlap
/// chain that ends in an ownerless reply is reachable here.
/// Guards the in-flight gate in `flush` and the reply-kind matching in `answered`.
#[test]
fn synthesized_uncertain_discard_keeps_one_outstanding_request_per_creation() {
    let home = tempfile::tempdir().unwrap();
    let worker = JournalWorker::spawn(home.path().to_owned(), Arc::new(|| {})).unwrap();
    let spawner = effect_runner::TerminalSpawner::spawn(Arc::new(|| {})).unwrap();
    let mut session = EngineSession::new(80, 24, Arc::new(|| {})).unwrap();
    let binding = crate::runtime::journal_product::EngineBinding {
        journal_id: "journal".into(),
        stream: "structure:slot-1".into(),
        incarnation: 1,
        runtime_epoch: 3,
        published_cut: None,
        revision: None,
    };
    let mut creation = Creation::committed(7, binding, &worker, lease().operation.clone()).unwrap();
    // A: the preparation reply is in flight when its Publish batch arrives.
    creation.queued = Some(Work::Prepared {
        lease: lease(),
        result: PreparationResult::Ready { data: None },
    });
    assert!(creation.flush(&worker).unwrap());
    creation.stage = Stage::Prepared {
        operation: lease().operation,
        acknowledged: false,
    };
    // B: the uncertain record queued while A is in flight, with a synthesized Discard owner.
    let reason = "installation failed during publish".to_owned();
    creation
        .submit(
            &worker,
            Work::PreparationUncertain {
                lease: lease(),
                reason: reason.clone(),
            },
        )
        .unwrap();
    creation.stage = Stage::Uncertain(Uncertain::pending(
        reason.clone(),
        UncertainOwner::Discard {
            lease: lease(),
            retirement: None,
            discard_committed: true,
            reason,
        },
    ));
    let view = crate::runtime::journal_product::CompletionView::default();
    creation
        .poll_cleanup(&worker, view.clone(), &mut session)
        .unwrap();
    assert!(
        creation.queued.is_some(),
        "the uncertain record must wait for the in-flight preparation reply"
    );
    assert!(
        !creation
            .answered(&worker, &mut session, &spawner, executed())
            .unwrap()
    );
    std::thread::sleep(std::time::Duration::from_millis(1100));
    creation
        .poll_cleanup(&worker, view.clone(), &mut session)
        .unwrap();
    assert!(creation.queued.is_none(), "B is submitted after A's reply");
    creation
        .poll_cleanup(&worker, view.clone(), &mut session)
        .unwrap();
    assert!(
        matches!(creation.stage, Stage::Uncertain(_)),
        "A's reply must not acknowledge B, so C waits for B's reply"
    );
    assert!(
        !creation
            .answered(&worker, &mut session, &spawner, executed())
            .unwrap()
    );
    creation
        .poll_cleanup(&worker, view.clone(), &mut session)
        .unwrap();
    assert!(
        matches!(creation.stage, Stage::Failed(_)),
        "C is submitted once B's reply arrives"
    );
    assert!(
        creation
            .answered(&worker, &mut session, &spawner, executed())
            .unwrap(),
        "C's reply finishes the creation with no reply left without an owner"
    );
}
