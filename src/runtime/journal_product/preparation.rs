//! Persisted launch parameters. Terminal output bytes and parser content never enter this input.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PreparationInput {
    #[serde(default)]
    pub(crate) adopt: Option<AdoptRecipe>,
    #[serde(default)]
    pub(crate) child: Option<ChildRecipe>,
    pub(crate) kind: String,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) params: serde_json::Value,
    pub(crate) shell: Option<ShellRecipe>,
    pub(crate) restore: Option<serde_json::Value>,
    /// 원본 capture를 읽지 못한 복원이다. 활성화 때 capture를 다시 읽지 않고 빈 상태로 시작한다.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) discard_capture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ShellRecipe {
    pub(crate) executable: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) environment: Vec<(String, String)>,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) scrollback_lines: usize,
    pub(crate) disk_scrollback: bool,
    /// Disk line cap (`None` = unlimited). Records written before the cap existed get the
    /// default cap.
    #[serde(default = "default_disk_scrollback_lines")]
    pub(crate) disk_scrollback_lines: Option<usize>,
    pub(crate) startup_command: String,
    pub(crate) restore_command: Option<String>,
}

fn default_disk_scrollback_lines() -> Option<usize> {
    tasty_settings::disk_scrollback_cap(tasty_settings::DEFAULT_SCROLLBACK_DISK_LINES)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EffectLease {
    pub(crate) effect_id: String,
    pub(crate) operation: tasty_core::OperationId,
    pub(crate) stream: String,
    pub(crate) runtime_epoch: u64,
    pub(crate) resource_generation: u64,
    pub(crate) attempt: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaimedPreparation {
    pub(crate) lease: EffectLease,
    pub(crate) input: PreparationInput,
    pub(crate) plan: tasty_core::CreationPlan,
    /// Immutable capture bytes read by the storage worker, never replayed as PTY input.
    pub(crate) capture: Option<Vec<u8>>,
    pub(crate) engine_incarnation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RecordedEffect {
    pub(super) stream: String,
    pub(super) instruction: tasty_core::StructuralEffect,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaimedRetirement {
    pub(crate) lease: EffectLease,
    pub(crate) plan: tasty_core::RetirementPlan,
    pub(crate) engine_incarnation: u64,
}

/// Process-local transfer proof. Recovery never turns an earlier epoch's PTY number into a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdoptRecipe {
    pub pty_id: u32,
    pub resource_generation: u64,
    pub runtime_epoch: u64,
}

/// Registry relation metadata is a service obligation, not a duplicate domain tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChildRecipe {
    /// 숫자로 기록된 이전 레코드는 Surface 부모로 읽힌다.
    pub parent: crate::core::child_parent::ParentKey,
    pub index: u32,
    pub workspace: u32,
    pub runtime_epoch: u64,
    pub cwd: Option<String>,
    pub role: Option<String>,
    /// 새 자식이면 그 nickname이다. 교체(`replacing`)면 respawn이 명시한 값만 담고, 없으면 설치 때의 값을 둔다.
    pub nickname: Option<String>,
    /// The bytes live only in the original bounded request continuation.
    pub has_command: bool,
    #[serde(default)]
    pub replacing: bool,
}

impl ChildRecipe {
    /// 교체 설치가 관계 항목에 반영하는 값. nickname은 respawn이 명시했을 때만 덮는다. 준비 뒤에
    /// `terminal.set_nickname`으로 바뀌거나 지워진 nickname을 준비 시점 값으로 되살리지 않는다.
    pub(crate) fn apply_replacement(self, entry: &mut crate::runtime::child_terminal::ChildEntry) {
        entry.cwd = self.cwd;
        entry.role = self.role;
        if let Some(nickname) = self.nickname {
            entry.nickname = Some(nickname);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShellRecipe;

    fn recipe(extra: &str) -> ShellRecipe {
        serde_json::from_str(&format!(
            r#"{{"executable":"","arguments":[],"environment":[],"cols":80,"rows":24,
                "scrollback_lines":100,"disk_scrollback":true,"startup_command":"",
                "restore_command":null{extra}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn recipes_without_a_disk_cap_get_the_default_and_null_stays_unlimited() {
        assert_eq!(recipe("").disk_scrollback_lines, Some(10_000));
        assert_eq!(
            recipe(r#","disk_scrollback_lines":null"#).disk_scrollback_lines,
            None
        );
        assert_eq!(
            recipe(r#","disk_scrollback_lines":5"#).disk_scrollback_lines,
            Some(5)
        );
    }
}
