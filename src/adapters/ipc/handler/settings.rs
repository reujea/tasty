//! 요청의 plugin_id 대신 CallerContext의 owner로 플러그인 설정을 조회한다.

use super::params::{self, p_try};
use serde_json::{Value, json};

use crate::runtime::engine_read::EngineRead;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

/// caller.owner()와 storage_key에 해당하는 값을 반환한다. 없으면 null이다.
/// Local은 _host, Plugin은 plugin_id, Agent는 agent_id를 키로 쓰며 별도 타입 거절은 없다.
pub fn handle_get_plugin_setting(
    engine: &EngineRead<'_>,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let Some(storage_key) = params.get("storage_key").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing 'storage_key'");
    };
    let plugin_id = caller.owner();
    let value = engine
        .settings
        .plugin_setting(plugin_id, storage_key)
        .and_then(|v| serde_json::to_value(v).ok());
    JsonRpcResponse::success(id, json!({ "value": value }))
}

/// `settings.get_remote_transfer {}` → `RemoteTransferSettings` 직렬화
/// (`{ "dir": <string>, "max_mb": <u64> }`). 원격 전송 저장 정책 조회.
pub fn handle_get_remote_transfer(engine: &EngineRead<'_>, id: Value) -> JsonRpcResponse {
    match serde_json::to_value(&engine.settings.remote_transfer) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, -32603, format!("failed to serialize settings: {e}")),
    }
}

/// 지정 필드만 실행 시점의 설정에 적용하고, 메모리 적용과 저장이 끝난 뒤 응답한다.
/// 응답의 `remote_transfer`는 적용 뒤의 값이다.
pub fn handle_set_remote_transfer(
    core: &crate::app::services::AppServices,
    out: &mut crate::ipc::window_port::IntentOutbox,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let mut dir = None;
    if let Some(value) = params.get("dir") {
        let Some(value) = value.as_str() else {
            return JsonRpcResponse::invalid_params(id, "'dir' must be a string");
        };
        dir = Some(value.to_string());
    }
    let mut max_mb = None;
    if params.get("max_mb").is_some_and(|v| !v.is_null()) {
        let Some(value) = p_try!(params::opt_int::<u64>(params, "max_mb", &id)) else {
            return JsonRpcResponse::invalid_params(id, "'max_mb' must be a non-negative integer");
        };
        max_mb = Some(value);
    }
    if dir.is_none() && max_mb.is_none() {
        return JsonRpcResponse::invalid_params(id, "expected at least one of 'dir', 'max_mb'");
    }

    core.request_settings_change(
        id,
        crate::app::settings_change::SettingsChange::RemoteTransfer { dir, max_mb },
        |intent| out.push(intent),
        |id, _changed, settings| {
            let applied = serde_json::to_value(&settings.remote_transfer).unwrap_or(Value::Null);
            JsonRpcResponse::success(id, json!({ "applied": true, "remote_transfer": applied }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tasty_settings::PluginSettingValue;

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    fn plugin_caller(plugin_id: &str) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: plugin_id.to_string(),
        }
    }

    #[test]
    fn plugin_can_read_back_its_own_stored_setting() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        e.runtime.settings.set_plugin_setting(
            "com.tasty.claude",
            "spawn_child_warn_threshold",
            PluginSettingValue::Number(8.0),
        );
        let caller = plugin_caller("com.tasty.claude");
        let resp = handle_get_plugin_setting(
            &e.read(),
            &caller,
            json!(1),
            &json!({ "storage_key": "spawn_child_warn_threshold" }),
        );
        assert_eq!(resp.result.unwrap()["value"], json!(8.0));
    }

    #[test]
    fn unset_setting_returns_null_not_error() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let caller = plugin_caller("com.tasty.claude");
        let resp = handle_get_plugin_setting(
            &e.read(),
            &caller,
            json!(1),
            &json!({ "storage_key": "never_stored" }),
        );
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["value"], Value::Null);
    }

    #[test]
    fn get_remote_transfer_returns_defaults() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let resp = handle_get_remote_transfer(&e.read(), json!(1));
        assert!(resp.error.is_none());
        let v = resp.result.unwrap();
        assert_eq!(v["dir"], json!(""));
        assert_eq!(v["max_mb"], json!(500));
    }

    #[test]
    fn get_remote_transfer_reflects_live_settings() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        e.runtime.settings.remote_transfer.dir = "/tmp/xfer".to_string();
        e.runtime.settings.remote_transfer.max_mb = 42;
        let resp = handle_get_remote_transfer(&e.read(), json!(1));
        let v = resp.result.unwrap();
        assert_eq!(v["dir"], json!("/tmp/xfer"));
        assert_eq!(v["max_mb"], json!(42));
    }

    #[test]
    fn plugin_cannot_read_another_plugins_setting() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        e.runtime.settings.set_plugin_setting(
            "com.tasty.codex",
            "spawn_child_warn_threshold",
            PluginSettingValue::Number(3.0),
        );
        let caller = plugin_caller("com.tasty.claude");
        // 요청의 plugin_id로 다른 플러그인 설정을 읽을 수 없어야 한다.
        let resp = handle_get_plugin_setting(
            &e.read(),
            &caller,
            json!(1),
            &json!({ "storage_key": "spawn_child_warn_threshold", "plugin_id": "com.tasty.codex" }),
        );
        assert_eq!(resp.result.unwrap()["value"], Value::Null);
    }

    use crate::app::settings_change::test_support::{ProbeStorage, core_with, settle};

    fn set_remote_transfer(
        storage: Arc<ProbeStorage>,
        params: Value,
        meanwhile: impl FnOnce(&mut tasty_settings::Settings),
    ) -> (JsonRpcResponse, tasty_settings::Settings) {
        let mut core = core_with(storage);
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let placeholder = handle_set_remote_transfer(&core, &mut out, json!(1), &params);
        assert!(
            placeholder.error.is_some(),
            "the answer waits for the result"
        );
        meanwhile(&mut engine.runtime.settings);
        let response = settle(&mut core, &mut state, &mut engine, out.into_vec());
        (response, engine.runtime.settings.clone())
    }

    /// 핸들러 뒤에 바뀐 `dir`을 지우지 않고, 응답은 적용 뒤의 값을 보여 준다.
    #[test]
    fn remote_transfer_applies_only_the_given_field_on_the_latest_settings() {
        let (response, settings) = set_remote_transfer(
            Arc::new(ProbeStorage::default()),
            json!({ "max_mb": 7 }),
            |settings| settings.remote_transfer.dir = "/changed-meanwhile".into(),
        );
        let result = response.result.expect("applied and saved");
        assert_eq!(result["remote_transfer"]["dir"], "/changed-meanwhile");
        assert_eq!(result["remote_transfer"]["max_mb"], 7);
        assert_eq!(settings.remote_transfer.dir, "/changed-meanwhile");
    }

    #[test]
    fn remote_transfer_reports_a_failed_save_instead_of_success() {
        let (response, settings) = set_remote_transfer(
            Arc::new(ProbeStorage {
                fail: true,
                ..Default::default()
            }),
            json!({ "max_mb": 9 }),
            |_| {},
        );
        let error = response.error.expect("not a success");
        assert_eq!(error.code, -32603);
        assert_eq!(
            error.data.unwrap(),
            json!({ "applied": true, "saved": false })
        );
        assert_eq!(settings.remote_transfer.max_mb, 9, "in effect for this run");
    }

    #[test]
    fn remote_transfer_reports_a_dropped_request() {
        let core = core_with(Arc::new(ProbeStorage::default()));
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        handle_set_remote_transfer(&core, &mut out, json!(1), &json!({ "dir": "/x" }));
        drop(out);
        let error = core.answer_parked_for_test().error.expect("not a success");
        assert!(error.message.contains("dropped"), "{}", error.message);
    }
}
