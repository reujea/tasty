//! Global input rules use the same settings update/save path as the settings UI.
use serde_json::{Value, json};

use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::window_port::IntentOutbox;
use crate::runtime::engine_read::EngineRead;

pub fn get(engine: &EngineRead<'_>, id: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "rules": engine.settings.terminal_input.rules }))
}

/// 규칙 변경을 실행 시점의 설정에 적용하고 적용·저장 뒤에 응답한다. `changed`는 실행 시점에 정한다
/// (이미 기본값을 넣은 `initialize`는 `changed:false`이며 저장하지 않는다).
pub fn handle_input_rule_update(
    core: &crate::app::services::AppServices,
    out: &mut IntentOutbox,
    caller: &CallerContext,
    id: Value,
    params: &Value,
    method: &str,
) -> JsonRpcResponse {
    use crate::app::settings_change::{InputRuleOp, SettingsChange};
    let Some(app) = params.get("app").and_then(Value::as_str) else {
        return JsonRpcResponse::invalid_params(id, "'app' must be a string");
    };
    let (op, newline) = if method == "settings.remove_input_rule" {
        (InputRuleOp::Remove, false)
    } else {
        let Some(newline) = params.get("shift_enter_newline").and_then(Value::as_bool) else {
            return JsonRpcResponse::invalid_params(id, "'shift_enter_newline' must be a boolean");
        };
        let op = if method == "settings.initialize_input_rule" {
            InputRuleOp::Initialize
        } else {
            InputRuleOp::Set
        };
        // 잘못된 이름은 접수하지 않고 바로 인자 오류로 답한다.
        if let Err(error) =
            tasty_settings::terminal_input::TerminalInputSettings::default().set_rule(app, newline)
        {
            return JsonRpcResponse::invalid_params(id, error);
        }
        (op, newline)
    };
    let change = SettingsChange::InputRule {
        op,
        owner: caller.owner().to_string(),
        app: app.to_string(),
        newline,
    };
    core.request_settings_change(
        id,
        change,
        |intent| out.push(intent),
        |id, changed, _settings| JsonRpcResponse::success(id, json!({ "changed": changed })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::DomainIntent;
    use crate::intent::Intent;

    #[test]
    fn plugins_can_only_seed_defaults() {
        let caller = CallerContext::Plugin {
            plugin_id: "com.tasty.claude".into(),
        };
        assert!(
            caller
                .ensure_allowed("settings.initialize_input_rule")
                .is_ok()
        );
        for method in [
            "settings.set_input_rule",
            "settings.remove_input_rule",
            "settings.get_input_rules",
        ] {
            assert!(caller.ensure_allowed(method).is_err());
        }
    }

    use crate::app::settings_change::test_support::{ProbeStorage, core_with, settle};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    fn update(
        core: &crate::app::services::AppServices,
        caller: &CallerContext,
        params: Value,
        method: &str,
    ) -> (JsonRpcResponse, Vec<crate::intent::DispatchedIntent>) {
        let mut out = IntentOutbox::default();
        let response = handle_input_rule_update(core, &mut out, caller, json!(1), &params, method);
        (response, out.into_vec())
    }

    #[test]
    fn rule_updates_answer_after_the_rule_is_applied_and_saved() {
        let storage = Arc::new(ProbeStorage::default());
        let mut core = core_with(storage.clone());
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let (placeholder, intents) = update(
            &core,
            &CallerContext::local(),
            json!({ "app": "claude", "shift_enter_newline": true }),
            "settings.set_input_rule",
        );
        assert!(
            placeholder.error.is_some(),
            "the answer waits for the result"
        );
        assert!(matches!(
            intents[0].body,
            Intent::Domain(DomainIntent::ChangeSettings { .. })
        ));
        assert!(engine.runtime.settings.terminal_input.rules.is_empty());

        let response = settle(&mut core, &mut state, &mut engine, intents);
        assert_eq!(response.result.unwrap()["changed"], true);
        assert!(
            engine
                .runtime
                .settings
                .terminal_input
                .shift_enter_newline(Some("claude.exe"))
        );
        assert_eq!(storage.saves.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn invalid_rule_values_are_answered_at_once_without_a_request() {
        let core = core_with(Arc::new(ProbeStorage::default()));
        for params in [
            json!({}),
            json!({"app":"claude", "shift_enter_newline":"yes"}),
            json!({"app":"", "shift_enter_newline":true}),
        ] {
            let (response, intents) = update(
                &core,
                &CallerContext::local(),
                params,
                "settings.set_input_rule",
            );
            assert_eq!(response.error.unwrap().code, -32602);
            assert!(intents.is_empty());
        }
        core.expect_no_parked_reply();
    }

    #[test]
    fn a_failed_save_is_an_error_even_though_the_rule_is_in_effect() {
        let storage = Arc::new(ProbeStorage {
            fail: true,
            ..Default::default()
        });
        let mut core = core_with(storage);
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let (_, intents) = update(
            &core,
            &CallerContext::local(),
            json!({ "app": "claude", "shift_enter_newline": true }),
            "settings.set_input_rule",
        );
        let error = settle(&mut core, &mut state, &mut engine, intents)
            .error
            .expect("a failed save is not a success");
        assert!(error.message.contains("not saved"), "{}", error.message);
        assert_eq!(
            error.data.unwrap(),
            json!({ "applied": true, "saved": false })
        );
        assert!(
            engine
                .runtime
                .settings
                .terminal_input
                .shift_enter_newline(Some("claude"))
        );
    }

    /// 핸들러와 실행 사이에 바뀐 다른 설정을 덮지 않는다(실행 시점의 최신 설정에 적용한다).
    #[test]
    fn the_rule_lands_on_the_settings_current_when_it_runs() {
        let mut core = core_with(Arc::new(ProbeStorage::default()));
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let (_, intents) = update(
            &core,
            &CallerContext::local(),
            json!({ "app": "claude", "shift_enter_newline": true }),
            "settings.set_input_rule",
        );
        engine.runtime.settings.general.startup_command = "changed-meanwhile".into();
        assert!(
            settle(&mut core, &mut state, &mut engine, intents)
                .error
                .is_none()
        );
        assert_eq!(
            engine.runtime.settings.general.startup_command,
            "changed-meanwhile"
        );
    }

    #[test]
    fn plugin_initialization_uses_authenticated_owner_and_does_not_reapply() {
        let storage = Arc::new(ProbeStorage::default());
        let mut core = core_with(storage.clone());
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let caller = CallerContext::Plugin {
            plugin_id: "com.tasty.claude".into(),
        };
        let params = json!({"app":"claude", "shift_enter_newline":true, "owner":"spoof"});
        let (_, intents) = update(
            &core,
            &caller,
            params.clone(),
            "settings.initialize_input_rule",
        );
        let response = settle(&mut core, &mut state, &mut engine, intents);
        assert_eq!(response.result.unwrap()["changed"], true);
        assert!(
            engine
                .runtime
                .settings
                .terminal_input
                .initialized_defaults
                .contains("com.tasty.claude/claude")
        );
        engine.runtime.settings.terminal_input.remove_rule("claude");
        let (_, intents) = update(&core, &caller, params, "settings.initialize_input_rule");
        let response = settle(&mut core, &mut state, &mut engine, intents);
        assert_eq!(response.result.unwrap()["changed"], false);
        assert_eq!(
            storage.saves.load(Ordering::SeqCst),
            1,
            "an unchanged initialization is not saved"
        );
    }
}
