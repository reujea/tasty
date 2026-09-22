use crate::commands::MemoryGoalCommands;
use crate::request::resolve_surface_id;

/// `--surface` 명시값 > `TASTY_SURFACE_ID` env. 둘 다 없으면 대상 불명이므로 종료.
fn require_surface(explicit: Option<u32>) -> u32 {
    match resolve_surface_id(explicit) {
        Some(id) => id,
        None => {
            crate::out::errln!("{}", tasty_i18n::t("cli.memory.goal_surface_required"));
            std::process::exit(1);
        }
    }
}

pub(super) fn memory_goal_command_to_method_params(
    command: &MemoryGoalCommands,
) -> (&'static str, serde_json::Value) {
    use MemoryGoalCommands::*;
    match command {
        Set { goal, surface } => (
            "memory.goal_set",
            serde_json::json!({ "surface_id": require_surface(*surface), "goal": goal }),
        ),
        Get { surface } => (
            "memory.goal_get",
            serde_json::json!({ "surface_id": require_surface(*surface) }),
        ),
        Clear { surface } => (
            "memory.goal_clear",
            serde_json::json!({ "surface_id": require_surface(*surface) }),
        ),
    }
}
