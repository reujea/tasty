//! 프로세스·알림·훅·플러그인의 호스트 이벤트를 전달한다.

use serde_json::json;
use tasty_plugin_protocol::EventScope;
use tasty_plugin_protocol::events::LifecycleReason;
use tasty_plugin_protocol::events::payloads::{
    HookFired, NotificationCreated, PluginError, PluginLoaded, ProcessExited,
};

use crate::plugin::PluginManager;

pub(super) fn emit_process_exited(mgr: &mut PluginManager, surface_id: u32) {
    let payload = ProcessExited {
        surface_id,
        exit_code: None,
    };
    mgr.emit_host_event("process.exited", &payload, EventScope::Surface);
}

pub(super) fn emit_notification_created(
    mgr: &mut PluginManager,
    id: u64,
    title: String,
    body: String,
    source: String,
    level: crate::notification::NotificationLevel,
) {
    let payload = NotificationCreated {
        id: id.to_string(),
        level: level.as_str().to_owned(),
        title,
        body,
        source,
    };
    mgr.emit_host_event("notification.created", &payload, EventScope::System);
}

pub(super) fn emit_hook_fired(
    mgr: &mut PluginManager,
    hook_id: u64,
    event_kind: String,
    surface_id: u32,
) {
    let scope = if surface_id != 0 {
        EventScope::Surface
    } else {
        EventScope::System
    };
    let payload = HookFired {
        hook_id: hook_id.to_string(),
        event_kind,
        surface_id: if surface_id != 0 {
            Some(surface_id)
        } else {
            None
        },
        payload: serde_json::Value::Null,
    };
    mgr.emit_host_event("hook.fired", &payload, scope);
}

pub(super) fn emit_plugin_loaded(mgr: &mut PluginManager, plugin_id: String, version: String) {
    let payload = PluginLoaded { plugin_id, version };
    mgr.emit_host_event("plugin.loaded", &payload, EventScope::System);
}

/// 헤드리스와 같은 이벤트 정의를 사용하도록 App의 공통 발행 함수를 부른다.
pub(super) fn emit_plugin_enable_toggled(
    mgr: &mut PluginManager,
    plugin_id: String,
    enabled: bool,
) {
    crate::app::plugin_events::emit_enable_toggled(mgr, plugin_id, enabled);
}

pub(super) fn emit_plugin_unloaded(mgr: &mut PluginManager, plugin_id: String, reason: String) {
    let lr = match reason.as_str() {
        "ipc" => LifecycleReason::Ipc,
        "crash" => LifecycleReason::Crash,
        _ => LifecycleReason::User,
    };
    crate::app::plugin_events::emit_unloaded(mgr, plugin_id, lr);
}

pub(super) fn emit_plugin_error(
    mgr: &mut PluginManager,
    plugin_id: String,
    error_kind: String,
    message: String,
) {
    let payload = PluginError {
        plugin_id,
        error_kind,
        message,
    };
    mgr.emit_host_event("plugin.error", &payload, EventScope::System);
}

pub(super) fn emit_plugin_registry_changed(
    mgr: &mut PluginManager,
    plugin_id: String,
    change_kind: String,
    detail: serde_json::Value,
) {
    let key = match change_kind.as_str() {
        "installed" => "plugin.installed",
        "removed" => "plugin.removed",
        other => {
            tracing::warn!("emit_plugin_registry_changed: unknown change_kind '{other}'");
            return;
        }
    };
    let payload = json!({
        "plugin_id": plugin_id,
        "detail": detail,
    });
    mgr.emit_host_event(key, &payload, EventScope::System);
}

pub(super) fn emit_plugin_surface_kind_registered(
    mgr: &mut PluginManager,
    plugin_id: String,
    kind: String,
    rendering: String,
) {
    let payload = json!({
        "plugin_id": plugin_id,
        "kind": kind,
        "rendering": rendering,
    });
    mgr.emit_host_event(
        "plugin.surface_kind_registered",
        &payload,
        EventScope::System,
    );
}

/// window 기여 선언을 알린다. 이 함수가 실제 창을 생성하지는 않는다.
pub(super) fn emit_plugin_window_declared(
    mgr: &mut PluginManager,
    plugin_id: String,
    window_id: String,
) {
    let payload = json!({
        "plugin_id": plugin_id,
        "window_id": window_id,
    });
    mgr.emit_host_event("plugin.window_declared", &payload, EventScope::System);
}
