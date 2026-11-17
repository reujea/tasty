//! 인바운드 웹훅 등록·조회·해제. 대상은 ID로 지정하고 목록은 전체를 반환한다.
//! 웹훅 항목별 소유자 격리는 없다. 웹훅은 AppServices의 공유 registry, hook_handler는 전역 저장소다.
//!
//! 등록한 핸들러는 Webhook source로 바인딩 가능한지 검사하며 셸/hook 전용은 거절한다.
//! HTTP payload는 등록 경로에 들어오지 않고 실행할 메서드를 바꾸지 못한다.
//! 플러그인은 Network 권한으로 자기 핸들러만 등록할 수 있다. Local 권한으로 실행되는
//! 인라인 시퀀스를 임의 등록하면 권한을 우회할 수 있으므로 플러그인에는 허용하지 않는다.

mod pending;
mod port;
use crate::app::services::AppServices;
use crate::webhook::registry::Value as RegistryValue;

use super::params::{self, p_try};
use serde_json::json;

use crate::hook_handler::{
    self, HookHandler, HookHandlerAction, HookHandlerId, HookHandlerOwner, HookSource, IpcCall,
    TriggerSource, validate_binding,
};
use crate::ipc::caller::CallerContext;
use crate::webhook::lifetime::now_unix;
use crate::webhook::{self, AuthLocation, Lifetime, Limit, Persistence, WebhookAuth, auth_summary};
use tasty_ipc::protocol::JsonRpcResponse;

/// 기본 허용 메서드(요청에 `methods` 미지정 시).
const DEFAULT_METHODS: &[&str] = &["POST"];

/// `webhook.register` — 웹훅 등록 후 발급 URL 반환.
///
/// params:
/// - `methods`: `["POST", ...]` (선택, 기본 `["POST"]`) — 허용 HTTP 메서드.
/// - `handler`: 등록된 훅 핸들러 id (선택) — source 게이트 검증 후 그 IpcSequence 사용.
/// - `sequence`: 인라인 IpcCall 배열 (선택) — owner 가 직접 정의(익명 웹훅 핸들러).
/// - `auth`: 선택적 인증 `{ location, key?, token }` — 미지정 시 무인증 통과.
///
/// `handler` 와 `sequence` 중 정확히 하나를 지정한다.
///
/// 플러그인은 인라인 sequence 대신 자기 소유 핸들러를 지정해야 한다.
///
/// lifetime params (선택):
/// - `persistent`: bool (기본 false → Temporary). true 면 재시작 후에도 복원.
/// - `ttl_secs`: u64 — 시간제한(now+ttl 절대 deadline). `count` 와 상호배타.
/// - `count`: u64 — 횟수제한(잔여 호출 수). `ttl_secs` 와 상호배타.
/// - 둘 다 없으면 무제한.
pub fn handle_register(
    core: &AppServices,
    caller: &CallerContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let plugin_caller: Option<&str> = match caller {
        CallerContext::Plugin { plugin_id, .. } => Some(plugin_id.as_str()),
        _ => None,
    };

    let methods = parse_methods(params);
    if methods.is_empty() {
        return JsonRpcResponse::invalid_params(id, "'methods' must be a non-empty string array");
    }

    let lifetime = match parse_lifetime(params) {
        Ok(lt) => lt,
        Err(e) => return JsonRpcResponse::invalid_params(id, e),
    };

    let auth = match parse_auth(params) {
        Ok(auth) => auth,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    };

    // CLI 는 미지정 필드를 JSON null 로 보내므로 null 을 "부재" 로 취급한다.
    let handler_field = params.get("handler").and_then(|v| v.as_str());
    let sequence_field = params.get("sequence").filter(|v| !v.is_null());

    let (handler_id, calls) = match (handler_field, sequence_field) {
        (Some(_), Some(_)) => {
            return JsonRpcResponse::invalid_params(
                id,
                "specify exactly one of 'handler' or 'sequence', not both",
            );
        }
        (Some(hid), None) => {
            if let Some(pid) = plugin_caller {
                let owned_prefix = format!("{pid}/");
                if !hid.starts_with(&owned_prefix) {
                    return JsonRpcResponse::invalid_params(
                        id,
                        format!(
                            "plugin '{pid}' may only bind its own hook handlers ('{pid}/…'), not '{hid}'"
                        ),
                    );
                }
            }
            let hid = HookHandlerId::new(hid);
            let Some(handler) = hook_handler::global().get(&hid) else {
                return JsonRpcResponse::invalid_params(
                    id,
                    format!("hook handler '{hid}' not found"),
                );
            };
            if let Err(e) = validate_binding(&handler, TriggerSource::Webhook) {
                return JsonRpcResponse::invalid_params(id, e.to_string());
            }
            match &handler.action {
                HookHandlerAction::IpcSequence { calls } => (Some(hid.clone()), calls.clone()),
                // is_webhook_bindable 이 이미 걸러내지만 방어적 이중 확인.
                HookHandlerAction::ShellCommand { .. } => {
                    return JsonRpcResponse::invalid_params(
                        id,
                        format!("hook handler '{hid}' is a shell command; not webhook-bindable"),
                    );
                }
            }
        }
        (None, Some(seq)) => {
            if let Some(pid) = plugin_caller {
                return JsonRpcResponse::invalid_params(
                    id,
                    format!(
                        "plugin '{pid}' cannot register an inline 'sequence'; reference a hook handler id via 'handler' instead"
                    ),
                );
            }
            match serde_json::from_value::<Vec<IpcCall>>(seq.clone()) {
                Ok(calls) if !calls.is_empty() => {
                    let anon = anonymous_handler(&calls);
                    let anon_id = anon.id.clone();
                    (Some(anon_id), calls)
                }
                Ok(_) => {
                    return JsonRpcResponse::invalid_params(id, "'sequence' must be non-empty");
                }
                Err(e) => {
                    return JsonRpcResponse::invalid_params(
                        id,
                        format!("'sequence' must be an array of {{method, params}}: {e}"),
                    );
                }
            }
        }
        (None, None) => {
            return JsonRpcResponse::invalid_params(id, "provide 'handler' or 'sequence'");
        }
    };

    let auth_json = auth.as_ref().map(auth_summary);
    let anonymous = sequence_field.is_some();
    let receipt =
        core.webhooks
            .register(methods.clone(), handler_id.clone(), calls, lifetime, auth);
    pending::answer(core, id, receipt, move |core, id, completed| {
        let RegistryValue::Registered(outcome) = completed.value else {
            return JsonRpcResponse::internal_error(id, "unexpected webhook registration result");
        };
        if anonymous
            && let Some((entry, _)) = core.webhooks.info(&outcome.id)
            && let Err(error) =
                hook_handler::global().upsert_full_handler(anonymous_handler(&entry.calls))
        {
            tracing::warn!(%error,"anonymous hook handler upsert failed");
        }
        JsonRpcResponse::success(
            id,
            json!({"id":outcome.id,"url":outcome.url,"methods":methods,"handler":handler_id.map(|h|h.0),"lifetime":lifetime_json(&lifetime),"auth":auth_json}),
        )
    })
}

/// Expiration keeps the established in-memory policy; saved:false exposes a known failed rewrite.
pub fn handle_sweep(core: &AppServices, id: serde_json::Value) -> JsonRpcResponse {
    pending::answer(core, id, core.webhooks.sweep(), |_, id, completed| {
        let RegistryValue::Swept(swept) = completed.value else {
            return JsonRpcResponse::internal_error(id, "unexpected webhook sweep result");
        };
        let mut value = json!({"count":swept.len(),"swept":swept});
        if let Some(saved) = completed.saved {
            value["saved"] = json!(saved);
        }
        JsonRpcResponse::success(id, value)
    })
}

/// lifetime params 를 파싱한다. `ttl_secs`/`count` 는 상호배타.
fn parse_lifetime(params: &serde_json::Value) -> Result<Lifetime, String> {
    let persistence = if params
        .get("persistent")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        Persistence::Persistent
    } else {
        Persistence::Temporary
    };

    let ttl = params::read_int::<u64>(params, "ttl_secs")?;
    let count = params::read_int::<u64>(params, "count")?;

    let limit = match (ttl, count) {
        (Some(_), Some(_)) => {
            return Err("specify at most one of 'ttl_secs' or 'count', not both".to_string());
        }
        (Some(secs), None) => {
            if secs == 0 {
                return Err("'ttl_secs' must be greater than 0".to_string());
            }
            Limit::TimeLimit {
                deadline_unix: now_unix().saturating_add(secs),
            }
        }
        (None, Some(remaining)) => {
            if remaining == 0 {
                return Err("'count' must be greater than 0".to_string());
            }
            Limit::CountLimit { remaining }
        }
        (None, None) => Limit::Unlimited,
    };

    Ok(Lifetime { persistence, limit })
}

/// lifetime → JSON(등록/조회 응답, 로컬 owner 채널). 시간제한은 절대 시각 +
/// 현재 기준 잔여 초를, 횟수제한은 남은 카운트를 노출한다.
fn lifetime_json(lifetime: &Lifetime) -> serde_json::Value {
    let persistence = match lifetime.persistence {
        Persistence::Persistent => "persistent",
        Persistence::Temporary => "temporary",
    };
    match lifetime.limit {
        Limit::Unlimited => json!({ "persistence": persistence, "limit": "unlimited" }),
        Limit::TimeLimit { deadline_unix } => {
            let now = now_unix();
            json!({
                "persistence": persistence,
                "limit": "time",
                "expires_at_unix": deadline_unix,
                "expires_in_secs": deadline_unix.saturating_sub(now),
                "expired": now >= deadline_unix,
            })
        }
        Limit::CountLimit { remaining } => json!({
            "persistence": persistence,
            "limit": "count",
            "remaining": remaining,
        }),
    }
}

pub fn handle_list(core: &AppServices, id: serde_json::Value) -> JsonRpcResponse {
    let items: Vec<_> = core
        .webhooks
        .list()
        .into_iter()
        .map(|(e, url)| entry_json(&e, &url))
        .collect();
    JsonRpcResponse::success(id, json!({ "webhooks": items }))
}

pub fn handle_info(
    core: &AppServices,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(wid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    match core.webhooks.info(wid) {
        Some((e, url)) => JsonRpcResponse::success(id, entry_json(&e, &url)),
        None => JsonRpcResponse::invalid_params(id, format!("webhook '{wid}' not found")),
    }
}

/// `webhook.unregister` — 웹훅 해제. path 회수 → 이후 그 URL 은 404.
pub fn handle_unregister(
    core: &AppServices,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(wid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    let wid = wid.to_owned();
    pending::answer(
        core,
        id,
        core.webhooks.unregister(&wid),
        move |_, id, completed| {
            let RegistryValue::Removed(removed) = completed.value else {
                return JsonRpcResponse::internal_error(id, "unexpected webhook removal result");
            };
            JsonRpcResponse::success(id, json!({"unregistered":removed,"id":wid}))
        },
    )
}

/// 인자가 없으면 실행 중인 리스너를 조회한다(실제로 bind한 주소와 정한 방법, 다음 실행에 쓸 값).
/// `port`를 주면 webhooks.toml에 저장하고, `unset_port`가 true면 저장한 값을 지운다.
/// `allow_external`은 설정(config.toml)의 외부 수신 허용을 바꾼다. 모두 다음 실행부터 적용된다.
pub fn handle_config(
    core: &crate::app::services::AppServices,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    if let Some(value) = params.get("allow_external").filter(|v| !v.is_null()) {
        let Some(allow) = value.as_bool() else {
            return JsonRpcResponse::invalid_params(id, "'allow_external' must be a boolean");
        };
        if params.get("port").is_some_and(|v| !v.is_null())
            || params.get("unset_port").is_some_and(|v| v == &json!(true))
        {
            return JsonRpcResponse::invalid_params(
                id,
                "'allow_external' is set on its own, without 'port' or 'unset_port'",
            );
        }
        let restart_required =
            core.ports.webhook().map(|b| !b.addr.ip().is_loopback()) != Some(allow);
        // 설정에 적용하고 저장한 뒤에 응답한다. 저장하지 못하면 다음 실행에 쓰이지 않으므로 오류다.
        return core.request_settings_change(
            id,
            crate::app::settings_change::SettingsChange::WebhookAllowExternal(allow),
            |intent| out.push(intent),
            move |id, _changed, _settings| {
                JsonRpcResponse::success(
                    id,
                    json!({
                        "allow_external": allow,
                        "applies_from": "next_start",
                        "restart_required": restart_required,
                    }),
                )
            },
        );
    }
    let unset = params
        .get("unset_port")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let port = p_try!(params::opt_int::<u16>(params, "port", &id));
    let operation = match (port, unset) {
        (Some(_), true) => {
            return JsonRpcResponse::invalid_params(id, "'port' and 'unset_port' are exclusive");
        }
        (Some(0), false) => {
            return JsonRpcResponse::invalid_params(id, "'port' must be in range 1..=65535");
        }
        (Some(port), false) => port::Operation::Set(port),
        (None, true) => port::Operation::Clear,
        (None, false) => port::Operation::Read,
    };
    let status = matches!(operation, port::Operation::Read)
        .then(|| listener_status(&core.ports, engine.settings.webhook.allow_external));
    port::request(core, id, operation, status)
}

/// 실행 중인 리스너의 상태. 파일에 저장한 값은 다음 실행에 쓸 값이라 따로 적는다.
fn listener_status(
    ports: &crate::runtime_ports::RuntimePorts,
    allow_external: bool,
) -> serde_json::Value {
    let bound = ports.webhook();
    json!({
        "allow_external": allow_external,
        "port": bound.map(|b| b.addr.port()),
        "address": bound.map(|b| b.addr.to_string()),
        "bound": bound.is_some(),
        "source": bound.map(|b| b.source),
        "explicit": bound.is_some_and(|b| b.source.is_explicit()),
        "unavailable": ports.webhook_failure().map(|f| json!({
            "port": f.addr.port(),
            "source": f.source,
            "error": f.error,
        })),
    })
}

fn parse_methods(params: &serde_json::Value) -> Vec<String> {
    match params.get("methods") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_ascii_uppercase())
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.to_ascii_uppercase()],
        _ => DEFAULT_METHODS.iter().map(|s| s.to_string()).collect(),
    }
}

/// `auth` 파라미터 파싱(선택). 미지정/`null` 이면 `Ok(None)`(무인증).
///
/// 형식: `{ "location": "query"|"bearer"|"body"|"header", "key": "<name>",
/// "token": "<secret>" }`. `bearer` 는 `key` 불요, 나머지는 필수. `token` 은 항상 필수.
fn parse_auth(params: &serde_json::Value) -> Result<Option<WebhookAuth>, String> {
    let Some(auth) = params.get("auth").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let obj = auth
        .as_object()
        .ok_or_else(|| "'auth' must be an object".to_string())?;

    let token = obj
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "'auth.token' is required and must be a non-empty string".to_string())?
        .to_string();

    let location_kind = obj
        .get("location")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "'auth.location' is required".to_string())?;
    let key = obj.get("key").and_then(|v| v.as_str());

    let require_key = |what: &str| -> Result<String, String> {
        key.filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("'auth.key' is required for {what} auth location"))
    };

    let location = match location_kind {
        "query" => AuthLocation::QueryKey {
            key: require_key("query")?,
        },
        "bearer" => AuthLocation::BearerHeader,
        "body" => AuthLocation::BodyField {
            field: require_key("body")?,
        },
        "header" => AuthLocation::HeaderKey {
            name: require_key("header")?,
        },
        other => {
            return Err(format!(
                "'auth.location' must be one of query|bearer|body|header, got '{other}'"
            ));
        }
    };

    Ok(Some(WebhookAuth { location, token }))
}

/// 인라인 시퀀스로부터 익명 웹훅 핸들러 생성 — 결정적 id(첫 method 기반 + 카운트).
fn anonymous_handler(calls: &[IpcCall]) -> HookHandler {
    let first = calls.first().map(|c| c.method.as_str()).unwrap_or("seq");
    // short-name 규약: [a-z0-9-]{1,32}. method 의 '.' 를 '-' 로.
    let slug: String = first
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(24)
        .collect();
    HookHandler {
        id: HookHandlerId::new(format!("user/wh-{slug}")),
        source: HookSource::Webhook,
        priority: 0,
        owner: HookHandlerOwner::User,
        action: HookHandlerAction::IpcSequence {
            calls: calls.to_vec(),
        },
        display_name_i18n_key: None,
        disabled: false,
    }
}

/// 발급 URL과 실행·수명 설정을 반환한다. 인증은 위치와 키만 공개하고 토큰은 제외한다.
fn entry_json(e: &webhook::WebhookEntry, url: &str) -> serde_json::Value {
    json!({
        "id": e.id,
        "url": url,
        "methods": e.methods,
        "handler": e.handler_id.as_ref().map(|h| h.0.clone()),
        "steps": e.calls.len(),
        "lifetime": lifetime_json(&e.lifetime),
        "auth": e.auth.as_ref().map(auth_summary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn plugin_caller(id: &str) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: id.to_string(),
        }
    }

    fn err_message(resp: &JsonRpcResponse) -> String {
        resp.error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_default()
    }

    #[test]
    fn plugin_cannot_register_inline_sequence() {
        let caller = plugin_caller("com.example.p");
        let params = json!({
            "sequence": [{ "method": "notification.create", "params": { "body": "x" } }]
        });
        let resp = handle_register(
            &crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
                .build()
                .unwrap(),
            &caller,
            json!(1),
            &params,
        );
        assert!(
            resp.error.is_some(),
            "plugin inline sequence must be rejected"
        );
        assert!(
            err_message(&resp).contains("inline 'sequence'"),
            "got: {}",
            err_message(&resp)
        );
    }

    #[test]
    fn plugin_cannot_bind_foreign_handler() {
        let caller = plugin_caller("com.example.p");
        let params = json!({ "handler": "host/notify" });
        let resp = handle_register(
            &crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
                .build()
                .unwrap(),
            &caller,
            json!(1),
            &params,
        );
        assert!(
            resp.error.is_some(),
            "foreign handler bind must be rejected"
        );
        assert!(
            err_message(&resp).contains("may only bind its own"),
            "got: {}",
            err_message(&resp)
        );
    }

    #[test]
    fn local_caller_inline_sequence_not_gated() {
        // 실제 등록 성공이 아니라 플러그인 전용 거절 조건에 걸리지 않는지만 검사한다.
        let params = json!({
            "sequence": [{ "method": "notification.create", "params": { "body": "x" } }]
        });
        let resp = handle_register(
            &crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
                .build()
                .unwrap(),
            &CallerContext::local(),
            json!(1),
            &params,
        );
        let msg = err_message(&resp);
        assert!(
            !msg.contains("inline 'sequence'") && !msg.contains("may only bind its own"),
            "local caller must not hit plugin gate, got: {msg}"
        );
    }

    #[test]
    fn allow_external_answers_after_the_setting_is_saved_and_reports_a_failed_save() {
        use crate::app::settings_change::test_support::{ProbeStorage, core_with, settle};
        for fail in [false, true] {
            let mut core = core_with(Arc::new(ProbeStorage {
                fail,
                ..Default::default()
            }));
            let (mut state, mut engine_session) = crate::state::tests::test_state();
            let mut engine = engine_session.borrow_mut();
            let mut out = crate::ipc::window_port::IntentOutbox::default();
            let placeholder = handle_config(
                &core,
                &mut out,
                &engine.read(),
                json!(1),
                &json!({ "allow_external": true }),
            );
            assert!(
                placeholder.error.is_some(),
                "the answer waits for the result"
            );
            let response = settle(&mut core, &mut state, &mut engine, out.into_vec());
            assert!(engine.runtime.settings.webhook.allow_external);
            if fail {
                let error = response.error.expect("a failed save is not a success");
                assert_eq!(error.data.unwrap()["saved"], false);
            } else {
                let result = response.result.expect("saved");
                assert_eq!(result["allow_external"], true);
                assert_eq!(result["applies_from"], "next_start");
            }
        }
    }
}
