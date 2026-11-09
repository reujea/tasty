//! Agent memory CLI subcommands — `MemoryCommands` + 5개 sub (Cache / Goal / Plan / Bb / Secret).
//!
//! Scope formats: `global`, `account:<userid>`, `window:<id>`, `workspace:<id>`, `surface:<id>`.
//! `--surface <id>` 같은 alias 가 대응 scope 로 정규화된다.

use clap::{Args, Subcommand};

// ScopeArgs를 flatten해 선택자와 상호 배타 조건을 공유한다.
// ///로 바꾸면 clap이 이 유지보수 설명을 사용자 도움말로 사용할 수 있다.
#[derive(Args)]
pub struct ScopeArgs {
    /// Scope token (`global`, `surface:3`, `workspace:7`, ...).
    #[arg(long, conflicts_with_all = ["surface", "workspace", "window", "account", "global"])]
    pub scope: Option<String>,
    /// Alias: `--surface 3` → `surface:3`.
    #[arg(long, conflicts_with_all = ["scope", "workspace", "window", "account", "global"])]
    pub surface: Option<u32>,
    /// Alias: `--workspace 7` → `workspace:7`.
    #[arg(long, conflicts_with_all = ["scope", "surface", "window", "account", "global"])]
    pub workspace: Option<u32>,
    /// Alias: `--window 42` → `window:42`.
    #[arg(long, conflicts_with_all = ["scope", "surface", "workspace", "account", "global"])]
    pub window: Option<u64>,
    /// Alias: `--account alice` → `account:alice`.
    #[arg(long, conflicts_with_all = ["scope", "surface", "workspace", "window", "global"])]
    pub account: Option<String>,
    /// Alias: `--global` → `global`.
    #[arg(long, conflicts_with_all = ["scope", "surface", "workspace", "window", "account"])]
    pub global: bool,
}

/// Agent memory CLI. Scope formats:
/// `global`, `account:<userid>`, `window:<id>`, `workspace:<id>`, `surface:<id>`.
/// Aliases such as `--surface <id>` are normalized to the matching scope.
#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Store a value at scope/key. Default content type inferred from value
    /// (string → text/plain, JSON literal → application/json).
    Put {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Key (1..256 `[a-z0-9._-]+`).
        #[arg(long)]
        key: String,
        /// Value. Treated as JSON if it parses, otherwise as a plain text string.
        /// Prefix with `@path` to read from a file (UTF-8 only; binary needs --value-b64).
        #[arg(long)]
        value: Option<String>,
        /// Base64-encoded binary payload. Overrides --value.
        #[arg(long)]
        value_b64: Option<String>,
        /// Force content type. Defaults: text/plain (string), application/json (JSON literal),
        /// application/octet-stream (with --value-b64).
        #[arg(long)]
        content_type: Option<String>,
        /// Relative TTL in seconds. Conflicts with --expires-at.
        #[arg(long, conflicts_with = "expires_at")]
        ttl: Option<u64>,
        /// Absolute expiry timestamp (unix ms). No-op if omitted.
        #[arg(long)]
        expires_at: Option<i64>,
        /// CAS version (must match current entry, otherwise cas_conflict).
        #[arg(long)]
        cas: Option<u64>,
    },
    /// Read a single entry.
    Get {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Entry key within the scope.
        #[arg(long)]
        key: String,
    },
    /// Delete a key.
    Delete {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Entry key within the scope.
        #[arg(long)]
        key: String,
        /// CAS version; if specified and mismatched, returns cas_conflict.
        #[arg(long)]
        cas: Option<u64>,
    },
    /// Check existence.
    Exists {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Entry key within the scope.
        #[arg(long)]
        key: String,
    },
    /// List entries in a scope (prefix + since/until/limit/offset).
    List {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Only keys starting with this prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Maximum number of entries to return.
        #[arg(long)]
        limit: Option<usize>,
        /// Only entries with `updated_at >= since` (unix ms).
        #[arg(long)]
        since: Option<i64>,
        /// Only entries with `updated_at < until` (unix ms).
        #[arg(long)]
        until: Option<i64>,
        /// Skip the first N matching entries (use with --limit for pagination).
        #[arg(long)]
        offset: Option<usize>,
    },
    /// Filter JSON entries by a dot-path equality (`--path a.b --equals <json>`).
    Query {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Dot path, e.g. `"task.status"`. Only `application/json` entries are inspected.
        #[arg(long)]
        path: String,
        /// JSON literal (or quoted string) to compare for equality.
        #[arg(long)]
        equals: String,
        /// Only keys starting with this prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Maximum number of entries to return.
        #[arg(long)]
        limit: Option<usize>,
        /// Only entries with `updated_at >= since` (unix ms).
        #[arg(long)]
        since: Option<i64>,
        /// Only entries with `updated_at < until` (unix ms).
        #[arg(long)]
        until: Option<i64>,
        /// Skip the first N matching entries (use with --limit for pagination).
        #[arg(long)]
        offset: Option<usize>,
    },
    /// Export regular entries to JSON (optional `--scope` filter). Secret area is
    /// never exported.
    Export {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Import regular entries from a JSON file (output of `memory export`).
    /// `--replace` overwrites existing keys; default skips conflicts.
    Import {
        /// Path to JSON file (entries array, or `{ "entries": [...] }`).
        #[arg(long)]
        file: String,
        /// Overwrite existing keys (default: skip).
        #[arg(long)]
        replace: bool,
    },
    /// Count entries in a scope (prefix optional).
    Count {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Only keys starting with this prefix.
        #[arg(long)]
        prefix: Option<String>,
    },
    /// List scopes currently in use.
    Scopes,
    /// Stats: total entries + bytes (per scope or aggregate).
    Stats {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Garbage-collect expired entries (regular + secret). Reads already filter
    /// expired rows; this only reclaims disk + quota. Local-only.
    Gc,
    /// Secret memory store. CLI acts as `_host` owner; no --owner flag exists.
    /// Plugin secret areas are inaccessible from the CLI by design.
    ///
    /// Secret means "another owner cannot read it", and nothing beyond that. The
    /// values sit as plain bytes in the same `memory.db`, so whoever can open
    /// that file reads them — the separation is enforced on the IPC path, not on
    /// disk. For something that has to survive that, use the OS keyring, or keep
    /// the data in a file of its own with its own permissions and store only the
    /// path here.
    Secret {
        #[command(subcommand)]
        command: MemorySecretCommands,
    },
    /// Blackboard — per-workspace key-value collections (`tasty.bb.<name>.*`).
    Bb {
        #[command(subcommand)]
        command: MemoryBbCommands,
    },
    /// Plan — per-workspace declarative work breakdown (`tasty.plan.<plan_id>`).
    Plan {
        #[command(subcommand)]
        command: MemoryPlanCommands,
    },
    /// Cache — per-workspace TTL cache (`tasty.cache.<key>`).
    Cache {
        #[command(subcommand)]
        command: MemoryCacheCommands,
    },
    /// Goal — a single goal sentence per surface (`tasty.goal`).
    ///
    /// Nothing inherits it: a surface spawned from this one starts with no goal.
    /// It carries no TTL because it needs none — the surface's whole memory scope
    /// is dropped when the surface closes, and again at startup for surfaces that
    /// were not restored, so a goal lives exactly as long as its surface.
    Goal {
        #[command(subcommand)]
        command: MemoryGoalCommands,
    },
}

mod bb;
mod cache;
mod goal;
mod plan;
mod secret;

pub use bb::MemoryBbCommands;
pub use cache::MemoryCacheCommands;
pub use goal::MemoryGoalCommands;
pub use plan::MemoryPlanCommands;
pub use secret::MemorySecretCommands;

/// 생성된 clap 명령 트리에서 모든 scope 선택자의 설명과 충돌 규칙을 확인한다.
#[cfg(test)]
mod scope_selector_pin {

    const SCOPE_FLAGS: [&str; 6] = [
        "scope",
        "surface",
        "workspace",
        "window",
        "account",
        "global",
    ];

    /// `tasty memory ...` 아래의 리프 서브커맨드를 (경로, Command) 로 편다.
    fn memory_leaves() -> Vec<(String, clap::Command)> {
        fn walk(prefix: &str, cmd: &clap::Command, out: &mut Vec<(String, clap::Command)>) {
            let mut kids = cmd.get_subcommands().peekable();
            if kids.peek().is_none() {
                out.push((prefix.to_string(), cmd.clone()));
                return;
            }
            for sub in cmd.get_subcommands() {
                walk(&format!("{prefix} {}", sub.get_name()), sub, out);
            }
        }
        let root = crate::help_i18n::command();
        let memory = root
            .get_subcommands()
            .find(|c| c.get_name() == "memory")
            .expect("`tasty memory` 서브커맨드가 있어야 한다")
            .clone();
        let mut out = Vec::new();
        walk("memory", &memory, &mut out);
        out
    }

    /// scope 선택자를 가진 자리 = `--scope` 를 노출하는 리프.
    fn scope_bearing_sites() -> Vec<(String, clap::Command)> {
        memory_leaves()
            .into_iter()
            .filter(|(_, c)| c.get_arguments().any(|a| a.get_id() == "scope"))
            .collect()
    }

    /// 순회가 비어도 나머지 검사가 통과하지 않도록 대상 수를 확인한다.
    #[test]
    fn the_population_of_scope_bearing_sites_is_not_empty() {
        let n = scope_bearing_sites().len();
        assert!(
            n >= 16,
            "scope 선택자 명령이 {n}개로 하한 16개보다 적다. 순회와 실제 명령 목록을 확인한다."
        );
    }

    #[test]
    fn every_scope_bearing_site_carries_the_whole_selector() {
        let mut bad = Vec::new();
        for (path, cmd) in scope_bearing_sites() {
            for flag in SCOPE_FLAGS {
                if !cmd.get_arguments().any(|a| a.get_id() == flag) {
                    bad.push(format!("{path} — `--{flag}` 없음"));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "scope 선택자가 반쪽인 자리가 있다. 여섯은 한 덩어리이므로 `ScopeArgs` 를 \
             flatten 해야 한다:\n  {}",
            bad.join("\n  ")
        );
    }

    #[test]
    fn every_scope_flag_is_documented_at_every_site() {
        let mut bad = Vec::new();
        for (path, cmd) in scope_bearing_sites() {
            for arg in cmd.get_arguments() {
                let id = arg.get_id().as_str();
                if SCOPE_FLAGS.contains(&id)
                    && arg
                        .get_help()
                        .is_none_or(|h| h.to_string().trim().is_empty())
                {
                    bad.push(format!("{path} — `--{id}` 설명 없음"));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "scope 플래그의 도움말 설명이 없다:\n  {}",
            bad.join("\n  ")
        );
    }

    #[test]
    fn the_selector_stays_mutually_exclusive_at_every_site() {
        let mut bad = Vec::new();
        for (path, cmd) in scope_bearing_sites() {
            // 필수 인자를 채워 넣어야 충돌이 아니라 "필수 누락" 으로 먼저 죽지 않는다.
            let mut base: Vec<String> = path.split(' ').map(str::to_string).collect();
            for arg in cmd.get_arguments() {
                if arg.is_required_set()
                    && let Some(long) = arg.get_long()
                {
                    base.push(format!("--{long}"));
                    if arg.get_num_args().is_none_or(|n| n.takes_values()) {
                        base.push("1".to_string());
                    }
                }
            }
            for (i, first) in SCOPE_FLAGS.iter().enumerate() {
                for second in SCOPE_FLAGS.iter().skip(i + 1) {
                    let mut argv = vec!["tasty".to_string()];
                    argv.extend(base.iter().cloned());
                    for f in [*first, *second] {
                        argv.push(format!("--{f}"));
                        if f != "global" {
                            argv.push("1".to_string());
                        }
                    }
                    let kind = crate::help_i18n::command()
                        .try_get_matches_from(&argv)
                        .err()
                        .map(|e| e.kind());
                    if kind != Some(clap::error::ErrorKind::ArgumentConflict) {
                        bad.push(format!(
                            "{path} — `--{}` + `--{}` 가 충돌로 안 걸린다 ({kind:?})",
                            first, second
                        ));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "scope 선택자 여섯 중 둘을 함께 줘도 통과하는 자리가 있다:\n  {}",
            bad.join("\n  ")
        );
    }

    #[test]
    fn no_memory_site_borrows_its_about_from_the_flattened_struct() {
        let mut bad = Vec::new();
        for (path, cmd) in memory_leaves() {
            if cmd
                .get_about()
                .is_none_or(|a| a.to_string().trim().is_empty())
            {
                bad.push(path);
            }
        }
        assert!(
            bad.is_empty(),
            "about 이 없는 자리가 있다. clap 은 그때 flatten 한 `ScopeArgs` 의 doc 을 \
             대신 쓰므로, 유지보수용 산문이 사용자 `--help` 맨 위에 찍힌다:\n  {}",
            bad.join("\n  ")
        );
    }
}
