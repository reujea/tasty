mod nickname;
mod state_intents;

use super::*;
use crate::app::file_writer::{self, FileWriterParts};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use tasty_utils::batch_writer::SystemClock;
use tasty_utils::test_wait::holds_by_every;

struct Fixture {
    root: tempfile::TempDir,
    hub: Arc<Hub>,
    parts: FileWriterParts,
}
impl Fixture {
    fn new() -> Self {
        Self::limits(Limits::default())
    }
    fn limits(limits: Limits) -> Self {
        let root = tempfile::tempdir().unwrap();
        let hub = Hub::new(Some(root.path().join("registry.json")));
        let parts = file_writer::start(
            Arc::new(tasty_settings::FileSettingsStorage),
            limits,
            Arc::new(SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        hub.attach(parts.writer.handle());
        until(|| hub.ready());
        Self { root, hub, parts }
    }
    fn path(&self) -> PathBuf {
        self.root.path().join("registry.json")
    }
    fn disk(&self) -> RegistryData {
        persistence::read(&self.path()).unwrap()
    }
    fn client(&self) -> ChildTerminalRegistry {
        let mut value = self.hub.client();
        value.bind_slot(None);
        value
    }
    fn gate(&self) -> (mpsc::SyncSender<()>, command::Ticket<()>) {
        let (entered, ready) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::sync_channel(1);
        let receipt = command::submit(
            Some(&self.parts.writer.handle()),
            1,
            move || {
                entered.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
            },
            |_| 0,
        )
        .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        (release, receipt)
    }
    fn drain(&mut self) {
        until(|| {
            self.hub.poll();
            self.hub.admission_drained()
        });
        // writer.poll_shutdown 은 끝난 worker 를 join 만 해 기한 뒤 관측 읽기에서도 부른다. 그 뒤의 hub.poll 은
        // child cleanup 을 제출할 수 있어 관측 읽기에서는 부르지 않는다.
        let end = Instant::now() + UNTIL_LIMIT;
        assert!(
            holds_by_every(end, Duration::ZERO, |last| {
                let remaining = self.parts.writer.poll_shutdown();
                if !last {
                    self.hub.poll();
                }
                remaining == 0
            }),
            "receipt deadline"
        );
    }
}
const UNTIL_LIMIT: Duration = Duration::from_secs(5);
/// holds_by 로 바꾸지 않는다. 호출자 조건(hub.ready, admission_drained, ticket.poll 등)이 안에서 hub.poll 로
/// child cleanup 을 제출할 수 있어 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는
/// 완료를 알 수 없다. 진행 없는 조건(gate_done 의 영수증 poll 등)은 이 도우미 대신 holds_by_every 로 기다린다.
fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + UNTIL_LIMIT;
    while !ready() {
        assert!(Instant::now() < end, "receipt deadline");
        std::thread::yield_now();
    }
}
fn wait(ticket: &Ticket) -> ResultValue {
    let mut result = None;
    until(|| {
        result = ticket.poll();
        result.is_some()
    });
    result.unwrap()
}
fn entry(child: u32, index: u32) -> super::super::ChildEntry {
    super::super::ChildEntry {
        child_surface_id: child,
        index,
        cwd: None,
        role: None,
        nickname: None,
    }
}
fn gate_done(release: mpsc::SyncSender<()>, receipt: command::Ticket<()>) {
    release.send(()).unwrap();
    // writer 명령 영수증의 poll 은 이미 나온 결과만 꺼낸다. 원래 간격(yield)을 지킨다.
    assert!(
        holds_by_every(Instant::now() + UNTIL_LIMIT, Duration::ZERO, |_| {
            receipt.poll().is_some()
        }),
        "receipt deadline"
    );
}
#[test]
fn reservation_and_register_publish_only_after_real_file_receipts() {
    let mut f = Fixture::new();
    let mut a = f.client();
    let mut b = f.client();
    let (release, gate) = f.gate();
    let first = a.reserve(&10.into()).unwrap();
    let second = b.reserve(&10.into()).unwrap();
    assert!(first.poll().is_none());
    assert!(second.poll().is_none());
    assert!(!f.path().exists());
    gate_done(release, gate);
    assert_eq!(wait(&first), Ok(Some(0)));
    assert_eq!(wait(&second), Ok(Some(1)));
    a.register_child(10, entry(100, 0));
    b.register_child(10, entry(200, 1));
    let one = a.submit().unwrap();
    let two = b.submit().unwrap();
    assert_eq!(wait(&one), Ok(None));
    assert_eq!(wait(&two), Ok(None));
    assert_eq!(f.disk().list_children(&10.into()).len(), 2);
    assert_eq!(f.disk().next_index.get(&10), Some(&2));
    f.drain();
}
#[test]
fn late_ack_keeps_toggle_back_and_create_then_delete() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let (release, gate) = f.gate();
    client.set_idle(100, true);
    client.register_child(10, entry(100, 0));
    let first = client.submit().unwrap();
    client.set_idle(100, false);
    client.remove_child(&10.into(), 0);
    let second = client.submit().unwrap();
    assert!(first.poll().is_none());
    assert!(second.poll().is_none());
    gate_done(release, gate);
    wait(&first).unwrap();
    wait(&second).unwrap();
    assert!(f.disk().parent_of_child(100).is_none());
    assert!(client.parent_of_child(100).is_none());
    assert_eq!(client.state_of(100), "active");
    f.drain();
}
#[test]
fn late_ack_keeps_unsent_equal_value_assignment_and_unrelated_external_field() {
    let mut f = Fixture::new();
    let mut client = f.client();
    client.register_child(10, entry(100, 0));
    client.set_idle(100, false);
    wait(&client.submit().unwrap()).unwrap();
    let (release, gate) = f.gate();
    client.set_idle(200, true);
    let first = client.submit().unwrap();
    client.set_idle(100, false); // Explicit assignment equals the accepted snapshot.
    let mut external = f.disk();
    external.set_idle(100, true);
    external.update_child(&10.into(), 0, |child| {
        child.nickname = Some("external".into())
    });
    std::fs::write(f.path(), serde_json::to_vec(&external).unwrap()).unwrap();
    gate_done(release, gate);
    wait(&first).unwrap();
    assert_eq!(client.state_of(100), "active");
    wait(&client.submit().unwrap()).unwrap();
    assert_eq!(f.disk().state_of(100), "active");
    assert_eq!(
        f.disk()
            .find_child(&10.into(), 0)
            .unwrap()
            .nickname
            .as_deref(),
        Some("external")
    );
    f.drain();
}
#[test]
fn dropped_reply_and_owner_still_drain_the_accepted_command() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let (release, gate) = f.gate();
    client.register_child(10, entry(100, 0));
    let ticket = client.submit().unwrap();
    drop(ticket);
    drop(client);
    gate_done(release, gate);
    f.drain();
    assert_eq!(f.disk().parent_of_child(100), Some(10.into()));
    assert!(f.hub.state.lock().unwrap().pending.is_empty());
}
#[test]
fn failed_admission_keeps_baseline_and_explicit_retry_does_not_replay_old_command() {
    let limits = Limits {
        queue_items: 1,
        ..Limits::default()
    };
    let mut f = Fixture::limits(limits);
    let mut client = f.client();
    let (release, gate) = f.gate();
    client.set_idle(100, true);
    assert!(client.submit().is_err());
    assert!(
        !client
            .client
            .lock()
            .unwrap()
            .accepted
            .idle
            .contains_key(&100)
    );
    gate_done(release, gate);
    wait(&client.submit().unwrap()).unwrap();
    assert_eq!(f.disk().state_of(100), "idle");
    f.drain();
}
#[test]
fn corrupt_latest_refuses_reservation_without_index_or_replacement() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let (release, gate) = f.gate();
    let ticket = client.reserve(&10.into()).unwrap();
    std::fs::write(f.path(), b"invalid owned original").unwrap();
    gate_done(release, gate);
    assert!(wait(&ticket).is_err());
    assert_eq!(std::fs::read(f.path()).unwrap(), b"invalid owned original");
    assert!(client.client.lock().unwrap().visible.next_index.is_empty());
    f.drain();
}
#[test]
fn pending_adopt_close_retains_cleanup_until_admitted_before_shutdown_fence() {
    let limits = Limits {
        queue_items: 2,
        ..Limits::default()
    };
    let mut f = Fixture::limits(limits);
    let mut client = f.client();
    let (release, gate) = f.gate();
    let ticket = client.adopt(10.into(), entry(100, 0)).unwrap();
    assert!(ticket.poll().is_none());
    assert!(client.unregister_child_by_surface(100));
    assert!(!f.hub.admission_drained()); // Shared writer is full; cleanup remains owned.
    gate_done(release, gate);
    until(|| f.hub.admission_drained());
    assert!(client.parent_of_child(100).is_none());
    f.drain();
    assert!(f.disk().parent_of_child(100).is_none());
    assert_eq!(wait(&ticket), Ok(Some(0))); // Saved receipt is separate from a live effect.
}

#[test]
fn gated_initial_read_refuses_admission_without_losing_visible_intent() {
    let mut f = Fixture::new();
    let (release, gate) = f.gate();
    f.hub.prepare(false);
    assert!(!f.hub.ready());
    let mut client = f.client();
    client.set_idle(100, false);
    assert!(client.submit().is_err());
    gate_done(release, gate);
    until(|| f.hub.ready());
    wait(&client.submit().unwrap()).unwrap();
    assert_eq!(f.disk().idle.get(&100), Some(&false));
    f.drain();
}
#[test]
fn worker_panic_is_unknown_and_never_replays_the_reserved_operation() {
    let mut f = Fixture::new();
    let mut client = f.client();
    f.hub
        .panic_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let ticket = client.reserve(&10.into()).unwrap();
    assert!(wait(&ticket).unwrap_err().contains("unknown"));
    assert!(!f.path().exists());
    assert!(client.client.lock().unwrap().visible.next_index.is_empty());
    assert_eq!(f.parts.stats.snapshot().commands.unknown, 1);
    // A new caller request is independent of the failed operation, never an automatic replay.
    assert_eq!(wait(&client.reserve(&10.into()).unwrap()), Ok(Some(0)));
    f.drain();
}
#[cfg(unix)]
#[test]
fn readonly_replacement_is_known_failure_and_returns_no_index() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let mut client = f.client();
    wait(&client.reserve(&10.into()).unwrap()).unwrap();
    let before = std::fs::read(f.path()).unwrap();
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o400)).unwrap();
    let result = wait(&client.reserve(&10.into()).unwrap());
    assert!(result.is_err());
    assert!(!result.unwrap_err().contains("unknown"));
    assert_eq!(std::fs::read(f.path()).unwrap(), before);
    assert_eq!(
        client.client.lock().unwrap().visible.next_index.get(&10),
        Some(&1)
    );
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    f.drain();
}

#[test]
fn adopt_payload_bytes_are_bounded_before_any_command_is_admitted() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let mut child = entry(100, 0);
    child.cwd = Some("x".repeat(Limits::default().queue_bytes + 1));
    assert!(
        client
            .adopt(10.into(), child)
            .err()
            .expect("oversize rejected")
            .contains("budget")
    );
    assert!(f.hub.state.lock().unwrap().pending.is_empty());
    assert!(f.hub.state.lock().unwrap().cleanups.is_empty());
    assert!(!f.path().exists());
    assert!(client.parent_of_child(100).is_none());
    f.drain();
}

#[test]
fn live_occupancy_and_original_ticket_wait_for_receipt_then_engine_continuation() {
    let mut f = Fixture::new();
    let mut owner = crate::state::tests::test_state().1;
    owner.runtime.child_terminals = f.client();
    let target = owner.core_state.workspace_at(0).unwrap().all_surface_ids()[0];
    let generation = owner.runtime.terminals.generation(target);
    let (release, gate) = f.gate();
    let ticket = owner
        .runtime
        .child_terminals
        .adopt_with_effect(9000.into(), entry(target, 0), generation)
        .unwrap();
    assert!(ticket.poll().is_none());
    assert!(owner.live.occupancy.occupancy_of(target).is_none());
    gate_done(release, gate);
    until(|| {
        f.hub.poll();
        ticket.result.lock().unwrap().is_some()
    });
    assert!(
        ticket.poll().is_none(),
        "backend receipt alone is not a live effect receipt"
    );
    assert!(owner.live.occupancy.occupancy_of(target).is_none());
    owner.borrow_mut().poll_child_registry_effects();
    assert_eq!(ticket.poll(), Some(Ok(Some(0))));
    assert!(ticket.poll().is_none());
    assert_eq!(
        owner.live.occupancy.occupancy_of(target).unwrap().parent,
        Some(9000.into())
    );
    f.drain();
}
#[test]
fn engine_retirement_reports_saved_but_unapplied_and_never_hangs_the_original_ticket() {
    let mut f = Fixture::new();
    let mut owner = crate::state::tests::test_state().1;
    owner.runtime.child_terminals = f.client();
    let target = owner.core_state.workspace_at(0).unwrap().all_surface_ids()[0];
    let generation = owner.runtime.terminals.generation(target);
    let (release, gate) = f.gate();
    let ticket = owner
        .runtime
        .child_terminals
        .adopt_with_effect(9000.into(), entry(target, 0), generation)
        .unwrap();
    drop(owner);
    assert!(ticket.poll().is_none());
    gate_done(release, gate);
    assert!(wait(&ticket).unwrap_err().contains("saved; engine retired"));
    assert_eq!(f.disk().parent_of_child(target), Some(9000.into()));
    f.drain();
}

#[cfg(feature = "gui")]
#[test]
fn atomic_adopt_tags_new_identities_and_pending_window_retirement_removes_counters() {
    let limits = Limits {
        queue_items: 2,
        ..Limits::default()
    };
    let mut f = Fixture::limits(limits);
    let mut client = f.client();
    client.bind_slot(Some(17));
    let ticket = client.adopt(10.into(), entry(100, 0)).unwrap();
    assert_eq!(wait(&ticket), Ok(Some(0)));
    assert_eq!(f.disk().slot_of.get(&10), Some(&17));
    assert_eq!(f.disk().slot_of.get(&100), Some(&17));
    let (release, gate) = f.gate();
    let next = client.adopt(20.into(), entry(200, 0)).unwrap();
    client.release_owned();
    assert!(!f.hub.admission_drained());
    gate_done(release, gate);
    until(|| f.hub.admission_drained());
    f.drain();
    wait(&next).unwrap();
    let disk = f.disk();
    assert!(disk.parent_of_child(200).is_none());
    assert!(!disk.next_index.contains_key(&20));
    assert!(disk.parent_of_child(100).is_none());
    assert!(!disk.next_index.contains_key(&10));
    assert!(disk.slot_of.values().all(|slot| *slot != 17));
}

#[cfg(feature = "gui")]
#[test]
fn retirement_keeps_other_engines_unregistered_numeric_and_external_reservations() {
    let mut f = Fixture::new();
    let mut owner = f.client();
    owner.bind_slot(Some(17));
    assert_eq!(wait(&owner.reserve(&10.into()).unwrap()), Ok(Some(0)));
    let external = super::super::ParentKey::External("shared".into());
    assert_eq!(wait(&owner.reserve(&external).unwrap()), Ok(Some(0)));
    owner.register_child(10, entry(100, 0));
    owner.register_child(external.clone(), entry(200, 0));
    wait(&owner.submit().unwrap()).unwrap();
    let mut other = f.client();
    other.bind_slot(Some(18));
    assert_eq!(wait(&other.reserve(&10.into()).unwrap()), Ok(Some(1)));
    assert_eq!(wait(&other.reserve(&external).unwrap()), Ok(Some(1)));
    owner.release_owned();
    f.drain();
    let disk = f.disk();
    assert!(disk.parent_of_child(100).is_none());
    assert!(disk.parent_of_child(200).is_none());
    assert_eq!(disk.next_index.get(&10), Some(&2));
    assert_eq!(disk.external_next_index.get("shared"), Some(&2));
}

#[cfg(feature = "gui")]
#[test]
fn existing_owner_retirement_survives_full_queue_and_owner_drop_until_final_drain() {
    let limits = Limits {
        queue_items: 1,
        ..Limits::default()
    };
    let mut f = Fixture::limits(limits);
    let mut owner = f.client();
    owner.bind_slot(Some(17));
    owner.register_child(10, entry(100, 0));
    wait(&owner.submit().unwrap()).unwrap();
    let mut foreign = f.disk();
    foreign.register_child(88, entry(303, 0));
    std::fs::write(f.path(), serde_json::to_vec(&foreign).unwrap()).unwrap();
    let (release, gate) = f.gate();
    owner.release_owned();
    assert!(!f.hub.admission_drained());
    assert_eq!(f.hub.state.lock().unwrap().retirements.len(), 1);
    drop(owner);
    gate_done(release, gate);
    f.drain();
    assert!(f.disk().parent_of_child(100).is_none());
    assert_eq!(f.disk().parent_of_child(303), Some(88.into()));
    assert!(f.hub.state.lock().unwrap().pending.is_empty());
    assert!(f.hub.state.lock().unwrap().retirements.is_empty());
}
