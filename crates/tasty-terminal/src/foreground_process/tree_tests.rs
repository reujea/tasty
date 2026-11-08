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
