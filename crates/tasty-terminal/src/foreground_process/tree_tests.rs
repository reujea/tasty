//! Synthetic process-tree tests for [`ProcessSnapshot::resolve`]. They build the
//! snapshot directly and make no OS calls, so they run on every platform.
use super::*;

/// Build a snapshot from `(pid, parent_pid, name)` triples.
fn snapshot(rows: &[(u32, u32, &str)]) -> ProcessSnapshot {
    ProcessSnapshot::from_entries(
        rows.iter()
            .map(|(pid, ppid, name)| (*pid, *ppid, name.to_string()))
            .collect(),
    )
}

// Ignore transient descendants below the shallowest non-shell process.
#[test]
fn picks_shallowest_non_shell_over_deep_helpers() {
    let snap = snapshot(&[
        (100, 1, "bash.exe"),
        (200, 100, "node.exe"),
        (300, 200, "bash.exe"),
        (301, 200, "git.exe"),
        (302, 200, "rg.exe"),
    ]);
    let info = snap.resolve(100).expect("resolve");
    assert_eq!(info.name, "node");
    assert_eq!(info.pid, 200);
}

// pwsh(100) → cmd(200, a shell) → node(300): nested shell is walked through
// but never chosen; node is selected.
#[test]
fn walks_through_nested_shell() {
    let snap = snapshot(&[
        (100, 1, "pwsh.exe"),
        (200, 100, "cmd.exe"),
        (300, 200, "node.exe"),
    ]);
    let info = snap.resolve(100).expect("resolve");
    assert_eq!(info.name, "node");
    assert_eq!(info.pid, 300);
}

// shell(100) with no descendants → fall back to the shell itself.
#[test]
fn falls_back_to_shell_when_no_descendant() {
    let snap = snapshot(&[(100, 1, "bash.exe")]);
    let info = snap.resolve(100).expect("resolve");
    assert_eq!(info.name, "bash");
    assert_eq!(info.pid, 100);
}

// Two non-shell children at the same (shallowest) depth → lowest PID wins.
// node is listed after cargo on purpose so insertion order can't pass by luck.
#[test]
fn ties_break_by_lowest_pid() {
    let snap = snapshot(&[
        (100, 1, "bash.exe"),
        (301, 100, "cargo.exe"),
        (300, 100, "node.exe"),
    ]);
    let info = snap.resolve(100).expect("resolve");
    assert_eq!(info.pid, 300);
    assert_eq!(info.name, "node");
}

/// Resolve on a helper thread so a walk that never ends fails the test instead of hanging it.
fn resolve_within(rows: &[(u32, u32, &str)], shell_pid: u32) -> Option<ForegroundProcessInfo> {
    let snap = snapshot(rows);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // The receiver is gone only after the test already failed on its timeout.
        if tx.send(snap.resolve(shell_pid)).is_err() {
            tracing::debug!("resolve finished after the test gave up");
        }
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("resolve did not finish on a cyclic parent graph")
}

// A process listed as its own parent (PID 0 on Windows) must not loop the walk.
#[test]
fn a_self_parented_shell_resolves_to_itself() {
    let info = resolve_within(&[(100, 100, "bash.exe")], 100).expect("resolve");
    assert_eq!((info.name.as_str(), info.pid), ("bash", 100));
}

// A reused PID can make the shell a descendant of its own child. The walk still
// finds the real child once and stops at the shell it already visited.
#[test]
fn a_shell_inside_a_parent_cycle_still_finds_its_child() {
    let info = resolve_within(
        &[
            (100, 300, "cmd.exe"),
            (200, 100, "pwsh.exe"),
            (300, 200, "cmd.exe"),
            (400, 200, "node.exe"),
        ],
        100,
    )
    .expect("resolve");
    assert_eq!((info.name.as_str(), info.pid), ("node", 400));
}

// A cycle of shells only falls back to the deepest shell reached before the cycle closes.
#[test]
fn a_cycle_of_shells_resolves_to_the_deepest_one_reached() {
    let info = resolve_within(
        &[
            (100, 300, "bash.exe"),
            (200, 100, "bash.exe"),
            (300, 200, "zsh.exe"),
        ],
        100,
    )
    .expect("resolve");
    assert_eq!((info.name.as_str(), info.pid), ("zsh", 300));
}

// A PID listed twice can close a cycle below the shell without the shell in it.
#[test]
fn a_cycle_below_the_shell_is_walked_once() {
    let info = resolve_within(
        &[
            (100, 1, "bash.exe"),
            (200, 100, "cmd.exe"),
            (300, 200, "pwsh.exe"),
            (200, 300, "cmd.exe"),
            (500, 300, "cargo.exe"),
        ],
        100,
    )
    .expect("resolve");
    assert_eq!((info.name.as_str(), info.pid), ("cargo", 500));
}

// A very long chain of nested shells ends at its single program.
#[test]
fn a_long_shell_chain_reaches_its_program() {
    let depth = 100_000u32;
    let mut rows: Vec<(u32, u32, &str)> = (1..=depth).map(|i| (i + 1, i, "cmd.exe")).collect();
    rows.push((depth + 2, depth + 1, "node.exe"));
    let info = resolve_within(&rows, 1).expect("resolve");
    assert_eq!((info.name.as_str(), info.pid), ("node", depth + 2));
}
