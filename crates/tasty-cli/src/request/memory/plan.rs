use crate::commands::MemoryPlanCommands;

pub(super) fn memory_plan_command_to_method_params(
    command: &MemoryPlanCommands,
) -> (&'static str, serde_json::Value) {
    use MemoryPlanCommands::*;
    match command {
        Create {
            workspace,
            plan_id,
            title,
            steps,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": workspace,
                "plan_id": plan_id,
                "title": title,
            });
            if let Some(raw) = steps.as_deref() {
                let arr: serde_json::Value = match serde_json::from_str(raw) {
                    Ok(v) => v,
                    Err(e) => {
                        crate::out::errln!(
                            "{}",
                            tasty_i18n::t_fmt2(
                                "cli.memory.option_not_json",
                                "--steps",
                                &e.to_string()
                            )
                        );
                        std::process::exit(1);
                    }
                };
                p["steps"] = arr;
            }
            ("memory.plan_create", p)
        }
        Get { workspace, plan_id } => (
            "memory.plan_get",
            serde_json::json!({ "workspace_id": workspace, "plan_id": plan_id }),
        ),
        List { workspace } => (
            "memory.plan_list",
            serde_json::json!({ "workspace_id": workspace }),
        ),
        Delete { workspace, plan_id } => (
            "memory.plan_delete",
            serde_json::json!({ "workspace_id": workspace, "plan_id": plan_id }),
        ),
        AddStep {
            workspace,
            plan_id,
            step,
            position,
            cas,
        } => {
            let step_v: serde_json::Value = match serde_json::from_str(step) {
                Ok(v) => v,
                Err(e) => {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt2("cli.memory.option_not_json", "--step", &e.to_string())
                    );
                    std::process::exit(1);
                }
            };
            let mut p = serde_json::json!({
                "workspace_id": workspace,
                "plan_id": plan_id,
                "step": step_v,
            });
            if let Some(pos) = position {
                p["position"] = serde_json::json!(pos);
            }
            if let Some(c) = cas {
                p["cas"] = serde_json::json!(c);
            }
            ("memory.plan_add_step", p)
        }
        RemoveStep {
            workspace,
            plan_id,
            step_id,
            cas,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": workspace,
                "plan_id": plan_id,
                "step_id": step_id,
            });
            if let Some(c) = cas {
                p["cas"] = serde_json::json!(c);
            }
            ("memory.plan_remove_step", p)
        }
        UpdateStep {
            workspace,
            plan_id,
            step_id,
            state,
            notes,
            clear_notes,
            cas,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": workspace,
                "plan_id": plan_id,
                "step_id": step_id,
            });
            if let Some(s) = state.as_deref() {
                p["state"] = serde_json::json!(s);
            }
            if *clear_notes {
                p["clear_notes"] = serde_json::json!(true);
            } else if let Some(n) = notes.as_deref() {
                p["notes"] = serde_json::json!(n);
            }
            if let Some(c) = cas {
                p["cas"] = serde_json::json!(c);
            }
            ("memory.plan_update_step", p)
        }
    }
}
