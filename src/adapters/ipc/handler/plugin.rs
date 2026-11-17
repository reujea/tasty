//! PluginManager가 필요한 IPC. App의 라우터에서 직접 호출한다.

use serde_json::{Value, json};

use crate::plugin::PluginManager;
use tasty_ipc::protocol::JsonRpcResponse;

/// 매니저가 없는 상태를 설치된 플러그인이 없는 상태와 구분해 오류로 답한다.
pub fn handle_list(mgr: Option<&PluginManager>, id: Value) -> JsonRpcResponse {
    let mgr = match mgr {
        Some(m) => m,
        None => return JsonRpcResponse::error(id, -32000, "plugin manager not initialized"),
    };
    let arr: Vec<Value> = mgr
        .packages()
        .iter()
        .map(|p| {
            json!({
                "id": p.manifest.id,
                "name": p.manifest.name,
                "version": p.manifest.version,
                "description": p.manifest.description,
                "enabled": !mgr.config.is_disabled(&p.manifest.id),
                "running": mgr.is_running(&p.manifest.id),
                "auto_disabled": mgr.is_auto_disabled(&p.manifest.id),
                "last_failure": failure_json(mgr.last_failure(&p.manifest.id)),
                "start": start_json(mgr.start_status(&p.manifest.id)),
                "surface_kinds": p.manifest.surface_kinds.iter().map(|k| &k.kind).collect::<Vec<_>>(),
                "log_path": mgr.log_path(&p.manifest.id).to_string_lossy(),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "plugins": arr }))
}

/// 기동 실패 한 건. `generation` 은 그 실패가 속한 기동 세대다.
fn failure_json(failure: Option<&tasty_host_plugin::manager::PluginFailure>) -> Value {
    match failure {
        Some(f) => json!({
            "kind": f.kind.as_str(),
            "message": f.message,
            "generation": f.generation,
        }),
        None => Value::Null,
    }
}

/// 가장 최근 기동 요청의 세대와 상태(`starting`·`connected`·`failed`·`cancelled`).
pub(crate) fn start_json(status: Option<&tasty_host_plugin::manager::StartStatus>) -> Value {
    match status {
        Some(s) => json!({
            "generation": s.generation,
            "state": s.state.as_str(),
            "failure": failure_json(s.failure.as_ref()),
        }),
        None => Value::Null,
    }
}

/// 매니페스트의 선언과 실제 등록 결과를 구분한다.
/// 헤드리스의 미지원 종류나 화이트리스트/API 조건 때문에 선언이 등록되지 않을 수 있다.
/// declared_rendering은 선언 값이고 effective_rendering은 이 플러그인 소유로 등록된 경로다.
/// 다른 소유자가 같은 kind를 등록했다면 registered는 false이고 registered_by로 그 소유자를 알린다.
fn surface_kind_json(
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    plugin_id: &str,
    k: &tasty_plugin_manifest::SurfaceKindDecl,
) -> Value {
    let def = registry.get_live(&k.kind);
    let owner = def.as_ref().map(|d| d.source.clone());
    let mine = matches!(
        &owner,
        Some(crate::runtime::surface_registry::KindSource::Plugin(id)) if id == plugin_id
    );
    json!({
        "kind": k.kind,
        "display_name_i18n_key": k.display_name_i18n_key,
        "icon": k.icon,
        "declared_rendering": k.rendering,
        "registered": mine,
        "effective_rendering": if mine {
            def.as_ref().map(|d| d.rendering.as_str())
        } else {
            None
        },
        "registered_by": owner.as_ref().map(|o| match o {
            crate::runtime::surface_registry::KindSource::HostBuiltin => "host",
            crate::runtime::surface_registry::KindSource::Plugin(id) => id.as_str(),
        }),
    })
}

pub fn handle_show(
    mgr: Option<&PluginManager>,
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let mgr = match mgr {
        Some(m) => m,
        None => return JsonRpcResponse::error(id, -32000, "plugin manager not initialized"),
    };
    let plugin_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing 'id' parameter"),
    };
    let pkg = match mgr.packages().iter().find(|p| p.manifest.id == plugin_id) {
        Some(p) => p,
        None => {
            return JsonRpcResponse::error(
                id,
                -32003,
                format!("plugin '{plugin_id}' not installed"),
            );
        }
    };
    let manifest = &pkg.manifest;

    let surface_kinds: Vec<Value> = manifest
        .surface_kinds
        .iter()
        .map(|k| surface_kind_json(registry, &plugin_id, k))
        .collect();

    let events_emitted: Vec<Value> = manifest
        .events_emitted
        .iter()
        .map(|e| {
            json!({
                "key": e.key,
                "description": e.description,
                "stability": e.stability,
                "payload_schema": e.payload_schema,
            })
        })
        .collect();

    let hook_events: Vec<Value> = manifest
        .contributes
        .hook_events
        .iter()
        .map(|h| {
            json!({
                "key": h.key,
                "description": h.description,
                "stability": h.stability,
            })
        })
        .collect();

    let commands: Vec<Value> = manifest
        .contributes
        .commands
        .iter()
        .map(|c| {
            let override_repr =
                mgr.config
                    .shortcut_override(&plugin_id, &c.id)
                    .map(|ov| match ov {
                        crate::plugin::registry_state::ShortcutOverride::Key { value } => {
                            json!({ "mode": "key", "value": value })
                        }
                        crate::plugin::registry_state::ShortcutOverride::Inherit { source } => {
                            json!({ "mode": "inherit", "source": source })
                        }
                        crate::plugin::registry_state::ShortcutOverride::None => {
                            json!({ "mode": "none" })
                        }
                    });
            json!({
                "id": c.id,
                "title_i18n_key": c.title_i18n_key,
                "scope": c.scope,
                "binding_mode": &c.binding_mode,
                "default_keybinding": c.default_keybinding,
                "shortcut_override": override_repr,
            })
        })
        .collect();

    let menu_items: Vec<Value> = manifest
        .contributes
        .menu_items
        .iter()
        .map(|m| json!({ "menu": m.menu, "command": m.command, "when": m.when }))
        .collect();

    let ipc_namespace: Vec<Value> = manifest
        .contributes
        .ipc_namespace
        .iter()
        .map(|n| json!({ "prefix": n.prefix }))
        .collect();

    let extension_state = mgr.extension_state(&plugin_id).map(extension_state_to_json);

    let extends = manifest.extends.as_ref().map(|d| {
        let to_event_hook = |h: &crate::plugin::manifest::EventHookDecl| {
            json!({
                "event": h.event,
                "modifies": h.modifies,
                "mode": h.mode,
                "timeout_ms": h.timeout_ms,
            })
        };
        let to_ipc_hook = |h: &crate::plugin::manifest::IpcHookDecl| {
            json!({
                "method": h.method,
                "modifies": h.modifies,
                "mode": h.mode,
                "timeout_ms": h.timeout_ms,
            })
        };
        json!({
            "plugin_id": d.plugin_id,
            "version_req": d.version_req,
            "api_version": d.api_version,
            "pre_event": d.pre_event.iter().map(to_event_hook).collect::<Vec<_>>(),
            "post_event": d.post_event.iter().map(to_event_hook).collect::<Vec<_>>(),
            "pre_ipc": d.pre_ipc.iter().map(to_ipc_hook).collect::<Vec<_>>(),
            "post_ipc": d.post_ipc.iter().map(to_ipc_hook).collect::<Vec<_>>(),
        })
    });

    let cli: Vec<Value> = manifest
        .contributes
        .cli
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "description": c.description,
                "subcommands": c.subcommands.iter().map(|s| {
                    json!({ "name": s.name, "ipc_method": s.ipc_method, "description": s.description })
                }).collect::<Vec<_>>(),
            })
        })
        .collect();

    JsonRpcResponse::success(
        id,
        json!({
            "id": manifest.id,
            "name": manifest.name,
            "version": manifest.version,
            "description": manifest.description,
            "authors": manifest.authors,
            "homepage": manifest.homepage,
            "api_version": manifest.api_version,
            "manifest_version": manifest.manifest_version,
            "dir": pkg.dir.to_string_lossy(),
            "enabled": !mgr.config.is_disabled(&plugin_id),
            "running": mgr.is_running(&plugin_id),
            "auto_disabled": mgr.is_auto_disabled(&plugin_id),
            "last_failure": failure_json(mgr.last_failure(&plugin_id)),
            "start": start_json(mgr.start_status(&plugin_id)),
            "log_path": mgr.log_path(&plugin_id).to_string_lossy(),
            "event_subscribe": manifest.event_subscribe,
            "event_publish": manifest.event_publish,
            "events_emitted": events_emitted,
            "hook_events": hook_events,
            "surface_kinds": surface_kinds,
            "commands": commands,
            "menu_items": menu_items,
            "ipc_namespace": ipc_namespace,
            "cli": cli,
            "extends": extends,
            "extension_state": extension_state,
        }),
    )
}

/// `plugin.extension.list` — 모든 extension의 현재 상태를 반환.
/// `[extends]` 블록이 없는 plugin은 결과에 포함되지 않는다.
pub fn handle_extension_list(mgr: Option<&PluginManager>, id: Value) -> JsonRpcResponse {
    let mgr = match mgr {
        Some(m) => m,
        None => return JsonRpcResponse::error(id, -32000, "plugin manager not initialized"),
    };
    let mut entries: Vec<Value> = mgr
        .extensions_iter()
        .map(|(ext_id, state)| {
            let target_id = mgr
                .packages()
                .iter()
                .find(|p| p.manifest.id == ext_id)
                .and_then(|p| p.manifest.extends.as_ref().map(|e| e.plugin_id.clone()));
            json!({
                "extension_id": ext_id,
                "target_id": target_id,
                "state": extension_state_to_json(state),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.get("extension_id")
            .and_then(|v| v.as_str())
            .cmp(&b.get("extension_id").and_then(|v| v.as_str()))
    });
    JsonRpcResponse::success(id, json!({ "extensions": entries }))
}

fn extension_state_to_json(state: &crate::plugin::extension_registry::ExtensionState) -> Value {
    use crate::plugin::extension_registry::{ExtensionState, PendingReason};
    match state {
        ExtensionState::Active {
            target_id,
            target_version,
        } => json!({
            "status": "active",
            "target_id": target_id,
            "target_version": target_version,
        }),
        ExtensionState::Pending(reason) => {
            let r = match reason {
                PendingReason::TargetMissing => json!({ "kind": "target_missing" }),
                PendingReason::TargetDisabled => json!({ "kind": "target_disabled" }),
                PendingReason::VersionMismatch {
                    target_version,
                    required,
                } => json!({
                    "kind": "version_mismatch",
                    "target_version": target_version,
                    "required": required,
                }),
                PendingReason::InvalidTargetVersion { target_version } => json!({
                    "kind": "invalid_target_version",
                    "target_version": target_version,
                }),
            };
            json!({ "status": "pending", "reason": r })
        }
        ExtensionState::Disabled => json!({ "status": "disabled" }),
        ExtensionState::Conflict { other_extension_id } => json!({
            "status": "conflict",
            "other_extension_id": other_extension_id,
        }),
    }
}

/// 헤드리스에서 응답을 만들기 전 매니저 준비 여부를 결정할 때 쓰는 메서드 목록.
/// dispatch_readonly의 match와 일치하는지는 시험으로 확인한다.
pub const READONLY_METHODS: &[&str] = &[
    "plugin.list",
    "plugin.show",
    "plugin.extension.list",
    "plugin.audit_query",
    "plugin.audit_summary",
    "plugin.list_agent_permissions",
];

pub fn is_readonly_method(method: &str) -> bool {
    READONLY_METHODS.contains(&method)
}

/// GUI와 헤드리스의 공용 조회 라우터. 처리하지 않는 메서드는 None으로 반환한다.
/// 쓰기·플러그인 생명주기·창이 필요한 요청은 각각의 별도 경로에서 처리한다.
pub fn dispatch_readonly(
    core: &crate::app::services::AppServices,
    mgr: Option<&PluginManager>,
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    method: &str,
    id: Value,
    params: &Value,
) -> Option<JsonRpcResponse> {
    if !is_readonly_method(method) {
        return None;
    }
    let response = match method {
        "plugin.list" => handle_list(mgr, id),
        "plugin.show" => handle_show(mgr, registry, id, params),
        "plugin.extension.list" => handle_extension_list(mgr, id),
        "plugin.audit_query" => super::audit::handle_query(core, id, params),
        "plugin.audit_summary" => super::audit::handle_summary(core, id, params),
        "plugin.list_agent_permissions" => {
            super::session::handle_list_agent_permissions(core, id, params)
        }
        // 선언한 메서드의 구현 누락을 미지원 메서드와 구별한다.
        other => JsonRpcResponse::internal_error(
            id,
            format!("READONLY_METHODS 에 '{other}' 가 있으나 dispatch arm 이 없다"),
        ),
    };
    Some(response)
}

// 실행은 app::plugin_lifecycle, 이벤트 발행은 app::plugin_events가 맡는다.
// 여기에는 메서드 분류, 파라미터 decode, 응답·오류 코드 매핑만 둔다.

/// 생명주기 중 enable/disable만 처리한다. 설치·삭제는 별도 경로다.
pub const LIFECYCLE_TOGGLE_METHODS: &[&str] = &["plugin.enable", "plugin.disable"];

pub fn is_lifecycle_toggle_method(method: &str) -> bool {
    LIFECYCLE_TOGGLE_METHODS.contains(&method)
}

/// Both routers defer the original response until the shared worker receipt and runtime effects.
pub(crate) fn dispatch_lifecycle_toggle(
    app: &mut crate::app::App,
    method: &str,
    id: Value,
    params: &Value,
) -> Option<JsonRpcResponse> {
    if !is_lifecycle_toggle_method(method) {
        return None;
    }
    let Some(plugin_id) = params.get("id").and_then(Value::as_str) else {
        return Some(JsonRpcResponse::invalid_params(
            id,
            "Missing 'id' parameter",
        ));
    };
    Some(app.reply_plugin_config(
        id,
        tasty_host_plugin::manager::file_config::ConfigChange::Enabled {
            id: plugin_id.to_owned(),
            enabled: method == "plugin.enable",
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 미등록 선언과 다른 소유자가 이미 등록한 경우를 확인한다. 정상 등록 경로는 포함하지 않는다.
    #[test]
    fn a_declaration_that_did_not_register_says_so() {
        let registry = crate::runtime::surface_registry::SurfaceKindRegistry::new();
        crate::runtime::surface_registry::register_builtin_kinds(&registry);
        let decl = |kind: &str| -> tasty_plugin_manifest::SurfaceKindDecl {
            serde_json::from_value(json!({
                "kind": kind,
                "display_name_i18n_key": "test.kind",
            }))
            .expect("decl")
        };

        let absent = surface_kind_json(&registry, "com.example.x", &decl("nope_kind"));
        assert_eq!(absent["registered"], json!(false));
        assert_eq!(absent["effective_rendering"], Value::Null);
        assert_eq!(absent["registered_by"], Value::Null);

        let taken = surface_kind_json(&registry, "com.example.x", &decl("explorer"));
        assert_eq!(taken["registered"], json!(false));
        assert_eq!(taken["effective_rendering"], Value::Null);
        assert_eq!(taken["registered_by"], json!("host"));
    }

    #[test]
    fn every_plugin_handler_reports_a_missing_manager_the_same_way() {
        let id = || Value::from(1);
        let params = json!({"id": "any"});
        let responses = [
            ("plugin.list", handle_list(None, id())),
            (
                "plugin.show",
                handle_show(
                    None,
                    &crate::runtime::surface_registry::SurfaceKindRegistry::new(),
                    id(),
                    &params,
                ),
            ),
            ("plugin.extension.list", handle_extension_list(None, id())),
        ];

        for (method, resp) in &responses {
            let err = resp
                .error
                .as_ref()
                .unwrap_or_else(|| panic!("{method} 가 매니저 없음을 에러로 답하지 않았다"));
            assert_eq!(err.code, -32000, "{method} 의 에러 코드");
            assert_eq!(
                err.message, "plugin manager not initialized",
                "{method} 의 에러 문구"
            );
        }
    }

    // 여기서는 목록 판정만 확인한다. 실제 라우터 응답은 헤드리스 통합 시험에서 검사한다.
    #[test]
    fn the_readonly_table_holds_reads_and_excludes_writes() {
        for method in READONLY_METHODS {
            assert!(
                is_readonly_method(method),
                "표에 있는 {method} 를 판정이 부정했다"
            );
        }
        assert_eq!(
            READONLY_METHODS.len(),
            6,
            "표 크기가 바뀌었다 — 문서도 같이 고쳐라"
        );

        for method in [
            "plugin.audit_clear",
            "plugin.grant_agent_permission",
            "plugin.revoke_agent_permission",
            "plugin.request_permission",
            "plugin.enable",
        ] {
            assert!(
                !is_readonly_method(method),
                "{method} 는 읽기 전용이 아닌데 표가 받아들였다"
            );
        }
    }

    // 매니저가 없는 경우만으로는 정상 응답을 보장할 수 없다. 실제 매니저 조회는 통합 시험에서 확인한다.
}
