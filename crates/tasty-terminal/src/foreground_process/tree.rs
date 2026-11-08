//! Foreground choice over one process snapshot. Pure so every OS can test it.

use std::collections::{HashMap, VecDeque};

use super::{ForegroundProcessInfo, is_known_shell_name};

/// One system-wide process snapshot, reusable for resolving the foreground
/// program of many shell PIDs in a single tick.
///
/// Windows has no Unix-style `tcgetpgrp`; ConPTY does not expose a foreground
/// process group, so each shell's foreground must be found by walking the
/// process tree. Building the parent→child map is identical for every shell in
/// one poll, so it is done once here and reused by [`resolve`](Self::resolve).
pub(super) struct ProcessSnapshot {
    /// `(pid, parent_pid, name)` for every process in the system.
    entries: Vec<(u32, u32, String)>,
    /// `parent_pid` → indices into `entries`.
    children: HashMap<u32, Vec<usize>>,
}

impl ProcessSnapshot {
    pub(super) fn from_entries(entries: Vec<(u32, u32, String)>) -> Self {
        let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, (_, ppid, _)) in entries.iter().enumerate() {
            children.entry(*ppid).or_default().push(i);
        }
        Self { entries, children }
    }

    /// Choose the shallowest non-shell descendant, breaking ties by lowest PID.
    /// ConPTY exposes no Unix-style foreground group, so this is a heuristic.
    /// Walk through nested shells. If no non-shell descendant exists, use the deepest
    /// leaf; without descendants, use the shell itself.
    pub(super) fn resolve(&self, shell_pid: u32) -> Option<ForegroundProcessInfo> {
        // BFS level-by-level from shell_pid. The explicit (depth, pid) comparison
        // below makes the choice independent of `children` iteration order, so
        // the result is deterministic even with multiple same-depth candidates.
        let mut queue: VecDeque<(u32, u32)> = VecDeque::new(); // (pid, depth)
        queue.push_back((shell_pid, 0));
        // (pid, name, depth) of the shallowest non-shell descendant so far.
        let mut shallowest_non_shell: Option<(u32, String, u32)> = None;
        // Fallback: deepest leaf, used only when no non-shell descendant exists.
        let mut best_leaf: Option<(u32, String, u32)> = None; // (pid, name, depth)
        let mut found_descendant = false;

        while let Some((pid, depth)) = queue.pop_front() {
            let Some(child_idxs) = self.children.get(&pid) else {
                continue;
            };
            for &idx in child_idxs {
                let (cpid, _, cname) = &self.entries[idx];
                found_descendant = true;
                let cdepth = depth + 1;
                if !is_known_shell_name(cname) {
                    // Shallowest wins; ties at the same depth → lowest PID.
                    let better = shallowest_non_shell
                        .as_ref()
                        .is_none_or(|(bpid, _, bd)| cdepth < *bd || (cdepth == *bd && cpid < bpid));
                    if better {
                        shallowest_non_shell = Some((*cpid, cname.clone(), cdepth));
                    }
                }
                // Track the deepest leaf in parallel for the no-non-shell fallback.
                let has_children = self.children.get(cpid).is_some_and(|v| !v.is_empty());
                if has_children {
                    queue.push_back((*cpid, cdepth));
                } else if best_leaf.as_ref().is_none_or(|(_, _, d)| cdepth > *d) {
                    best_leaf = Some((*cpid, cname.clone(), cdepth));
                }
            }
        }

        let (pid, raw_name) = if let Some((pid, name, _)) = shallowest_non_shell {
            (pid, name)
        } else if found_descendant {
            // Only nested/leaf shells under this shell — show the deepest one.
            let (pid, name, _) = best_leaf?;
            (pid, name)
        } else {
            // Shell is its own foreground process — return its name for parity
            // with Linux/macOS where tpgid resolves to the shell when idle.
            let (pid, _, name) = self.entries.iter().find(|(p, _, _)| *p == shell_pid)?;
            (*pid, name.clone())
        };

        let name = raw_name
            .strip_suffix(".exe")
            .or_else(|| raw_name.strip_suffix(".EXE"))
            .unwrap_or(&raw_name)
            .to_string();

        if name.is_empty() {
            return None;
        }
        Some(ForegroundProcessInfo { name, pid })
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
