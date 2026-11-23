//! 레이아웃 크기 적용: 그리지 않는 터미널은 목표만 기록하고 보이게 된 첫 스윕에 한 번 적용한다.
use super::*;
use std::sync::{Arc, atomic::AtomicU32};

fn store() -> TerminalStore {
    TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)))
}

#[cfg(unix)]
fn pty_terminal(cols: usize, rows: usize) -> (Terminal, Pty) {
    tasty_terminal::spawn_terminal(
        tasty_terminal::TerminalConfig {
            cols,
            rows,
            shell: Some("/bin/sh"),
            args: &["-c", "sleep 30"],
            surface_id: 1,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        Arc::new(|| {}),
    )
    .expect("terminal/PTY creation")
}

/// Given 숨은 mirror, When 목표가 바뀌면, Then grid는 그대로이고 목표만 기록한다. 목표가 grid와 같아지면
/// 기록을 지운다. 보이게 되면 기록을 지우고 서버 요청이 필요하다고 알린다.
#[test]
fn hidden_mirror_records_only_and_visible_one_asks_to_forward() {
    let mut store = store();
    store.insert(1, Terminal::new_detached(80, 24), None);

    assert_eq!(
        store.apply_layout_size(1, 100, 30, false),
        LayoutSizeOutcome::Deferred
    );
    assert_eq!(store.get(1).unwrap().dimensions(), (80, 24));
    assert_eq!(store.deferred_size(1), Some((100, 30)));

    assert_eq!(
        store.apply_layout_size(1, 80, 24, false),
        LayoutSizeOutcome::Deferred
    );
    assert_eq!(
        store.deferred_size(1),
        None,
        "원래 크기로 돌아오면 기록이 없다"
    );

    store.apply_layout_size(1, 100, 30, false);
    assert_eq!(
        store.apply_layout_size(1, 100, 30, true),
        LayoutSizeOutcome::ForwardMirror
    );
    assert_eq!(store.deferred_size(1), None);
    assert_eq!(
        store.apply_layout_size(1, 80, 24, true),
        LayoutSizeOutcome::MirrorSettled
    );
}

/// Given PTY가 있는 숨은 터미널, When 첫 목표를 받으면, Then 숨어 있어도 한 번 적용한다(첫 배치).
/// 그 뒤의 숨은 목표는 grid·PTY 예약을 건드리지 않고 기록만 한다. 보이게 되면 마지막 목표로 한 번 적용한다.
#[cfg(unix)]
#[test]
fn first_placement_applies_once_then_hidden_targets_only_record() {
    let mut store = store();
    let (terminal, pty) = pty_terminal(80, 24);
    store.insert(1, terminal, Some(pty));

    assert_eq!(
        store.apply_layout_size(1, 90, 20, false),
        LayoutSizeOutcome::Applied
    );
    assert_eq!(store.get(1).unwrap().dimensions(), (90, 20));
    assert!(store.pty(1).unwrap().has_pending_resize());
    store.pty_mut(1).unwrap().force_flush_resize();

    for (cols, rows) in [(120, 40), (70, 18), (110, 33)] {
        assert_eq!(
            store.apply_layout_size(1, cols, rows, false),
            LayoutSizeOutcome::Deferred
        );
    }
    assert_eq!(store.get(1).unwrap().dimensions(), (90, 20));
    assert!(!store.pty(1).unwrap().has_pending_resize());
    assert_eq!(store.deferred_size(1), Some((110, 33)));

    assert_eq!(
        store.apply_layout_size(1, 110, 33, true),
        LayoutSizeOutcome::Applied
    );
    assert_eq!(store.get(1).unwrap().dimensions(), (110, 33));
    assert!(store.pty(1).unwrap().has_pending_resize());
    assert_eq!(store.deferred_size(1), None);
}

/// Given spawn 크기와 같은 첫 목표, When 받으면, Then grid는 바뀌지 않지만 첫 배치는 끝나 다음 숨은 목표는
/// 기록만 된다.
#[cfg(unix)]
#[test]
fn first_placement_ends_even_when_the_size_already_matches() {
    let mut store = store();
    let (terminal, pty) = pty_terminal(80, 24);
    store.insert(1, terminal, Some(pty));
    store.apply_layout_size(1, 80, 24, false);
    assert_eq!(
        store.apply_layout_size(1, 100, 30, false),
        LayoutSizeOutcome::Deferred
    );
    assert_eq!(store.get(1).unwrap().dimensions(), (80, 24));
}

/// entry를 바꾸는 경로는 기록과 첫 배치 표시를 새 entry로 시작하고, attach 크기는 기록을 버린다.
#[test]
fn entry_replacement_and_attach_resize_reset_the_deferred_size() {
    let mut store = store();
    store.insert(1, Terminal::new_detached(80, 24), None);
    store.apply_layout_size(1, 100, 30, false);
    store.insert(1, Terminal::new_detached(80, 24), None);
    assert_eq!(store.deferred_size(1), None, "insert 덮어쓰기");

    store.apply_layout_size(1, 100, 30, false);
    store.replace(1, Terminal::new_detached(80, 24), None);
    assert_eq!(store.deferred_size(1), None, "replace");

    store.apply_layout_size(1, 100, 30, false);
    assert!(store.resize_for_attach(1, 90, 25));
    assert_eq!(store.deferred_size(1), None, "attach 크기");

    store.apply_layout_size(1, 100, 30, false);
    store.remove(1);
    store.insert(1, Terminal::new_detached(80, 24), None);
    assert_eq!(store.deferred_size(1), None, "remove 뒤 새 entry");
}

/// 서버가 정한 크기가 기록한 목표와 같아지면 기록을 지우고, 다른 크기면 남긴다.
#[test]
fn server_size_reaching_the_target_settles_the_record() {
    let mut store = store();
    store.insert(1, Terminal::new_detached(80, 24), None);
    store.apply_layout_size(1, 100, 30, false);
    store.get_mut(1).unwrap().resize(90, 25);
    store.settle_deferred_size(1);
    assert_eq!(store.deferred_size(1), Some((100, 30)));
    store.get_mut(1).unwrap().resize(100, 30);
    store.settle_deferred_size(1);
    assert_eq!(store.deferred_size(1), None);
}
