//! memory IPC와 공용 처리. 확장 기능은 하위 모듈에 둔다.
//! owner는 CallerContext에서 정하며 플러그인이 요청 인자로 바꿀 수 없다.

mod advanced;

pub mod bb;
pub mod cache;
pub mod goal;
pub mod plan;
pub mod secret;

#[cfg(test)]
mod durable_tests;
#[cfg(test)]
mod surface_scope_tests;

pub use advanced::{handle_export, handle_gc, handle_import, handle_query};
pub use bb::*;
pub use cache::*;
pub use goal::*;
pub use plan::*;
pub use secret::*;

pub(super) fn require_workspace_id(params: &Value, id: &Value) -> Result<u32, JsonRpcResponse> {
    params::opt_int::<u64>(params, "workspace_id", id)?
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            JsonRpcResponse::invalid_params(id.clone(), "Missing or invalid 'workspace_id'")
        })
}

/// 활성 surface로 대체하지 않고 명시한 ID를 검사한다. headless PTY ID 공간은 거절한다.
pub(super) fn require_surface_id(params: &Value, id: &Value) -> Result<u32, JsonRpcResponse> {
    params::opt_int::<u64>(params, "surface_id", id)?
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| crate::runtime::terminal_store::is_surface_id_space(*n))
        .ok_or_else(|| {
            JsonRpcResponse::invalid_params(id.clone(), "Missing or invalid 'surface_id'")
        })
}

pub(super) fn require_str<'a>(
    params: &'a Value,
    key: &str,
    id: &Value,
) -> Result<&'a str, JsonRpcResponse> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), format!("Missing '{key}'")))
}

use super::params::{self, p_try};
use serde_json::{Value, json};
use tasty_memory::{
    ListOpts, MemoryArea, MemoryEntry, MemoryError, MemoryStats, MemoryValue, PutOpts, Scope,
};

use crate::app::services::AppServices;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

fn require_scope(params: &Value, id: &Value) -> Result<Scope, JsonRpcResponse> {
    let raw = params
        .get("scope")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), "Missing 'scope' parameter"))?;
    let scope = Scope::parse(raw)
        .map_err(|s| JsonRpcResponse::invalid_params(id.clone(), format!("invalid scope: {s}")))?;
    reject_pty_space_surface_scope(&scope, id)?;
    Ok(scope)
}

/// surface scope에 새 항목을 만드는 쓰기는 열린 surface에만 허용한다. 라우터도 같은 대상을
/// 확인하지만 다른 경로로 핸들러가 불려도 닫혔거나 없는 surface의 scope에 항목이 생기지 않게 한다.
pub(super) fn reject_closed_surface_scope(
    engine: &crate::core::CoreState,
    scope: &Scope,
    id: &Value,
) -> Result<(), JsonRpcResponse> {
    match scope {
        Scope::Surface(sid) if !engine.has_surface(*sid) => Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("Surface {sid} not found"),
        )),
        _ => Ok(()),
    }
}

/// scope 문자열로 들어온 surface ID도 검사해 PTY ID가 surface scope로 저장되지 않게 한다.
fn reject_pty_space_surface_scope(scope: &Scope, id: &Value) -> Result<(), JsonRpcResponse> {
    match scope {
        Scope::Surface(sid) if !crate::runtime::terminal_store::is_surface_id_space(*sid) => {
            Err(JsonRpcResponse::invalid_params(
                id.clone(),
                format!("invalid scope: surface id {sid} is inside the headless PTY id space"),
            ))
        }
        _ => Ok(()),
    }
}

/// 호스트 전용 키 접두어. 플러그인·에이전트는 일반 memory API로 읽거나 바꿀 수 없다.
/// 전용 메서드의 권한 검사를 일반 KV API로 우회하지 못하도록 한다(docs/dev-guide/plugin-permissions.md).
/// 개별 하위 namespace를 나열하지 않아 새 호스트 키도 같은 제한을 받는다.
pub(super) const HOST_KEY_NAMESPACE: &str = "tasty.";

/// 플러그인·에이전트를 제한한다. 신뢰하는 Local 호출은 제외한다.
fn is_permissioned(caller: &CallerContext) -> bool {
    caller.is_external()
}

fn is_host_key(key: &str) -> bool {
    key.starts_with(HOST_KEY_NAMESPACE)
}

/// 지정 prefix의 결과에 호스트 키가 섞일 수 있는지 확인한다. 생략하거나 ta처럼 짧아도 해당된다.
fn prefix_may_include_host(prefix: Option<&str>) -> bool {
    prefix.is_none_or(|p| HOST_KEY_NAMESPACE.starts_with(p))
}

/// 키를 직접 지목하는 경로(put / get / delete / exists / import)의 차단.
pub(super) fn reject_host_key(
    caller: &CallerContext,
    key: &str,
    id: &Value,
) -> Result<(), JsonRpcResponse> {
    if is_permissioned(caller) && is_host_key(key) {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!(
                "reserved key namespace: '{HOST_KEY_NAMESPACE}' belongs to the host                  (use the dedicated methods for that data)"
            ),
        ));
    }
    Ok(())
}

/// 목록 조회는 prefix를 지정하지 않을 수도 있으므로 호스트 키만 결과에서 제외한다.
pub(super) fn hide_host_keys(
    caller: &CallerContext,
    entries: Vec<MemoryEntry>,
) -> Vec<MemoryEntry> {
    if !is_permissioned(caller) {
        return entries;
    }
    entries
        .into_iter()
        .filter(|e| !is_host_key(&e.key))
        .collect()
}

fn require_key<'a>(params: &'a Value, id: &Value) -> Result<&'a str, JsonRpcResponse> {
    params
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), "Missing 'key' parameter"))
}

fn optional_scope(params: &Value, id: &Value) -> Result<Option<Scope>, JsonRpcResponse> {
    match params.get("scope").and_then(|v| v.as_str()) {
        None => Ok(None),
        Some(raw) => {
            let scope = Scope::parse(raw).map_err(|s| {
                JsonRpcResponse::invalid_params(id.clone(), format!("invalid scope: {s}"))
            })?;
            reject_pty_space_surface_scope(&scope, id)?;
            Ok(Some(scope))
        }
    }
}

fn parse_value(params: &Value, id: &Value) -> Result<MemoryValue, JsonRpcResponse> {
    let content_type = params
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| match params.get("value") {
            Some(Value::String(_)) => "text/plain",
            Some(_) => "application/json",
            None if params.get("value_b64").is_some() => "application/octet-stream",
            None => "application/json",
        });
    match content_type {
        "text/plain" => match params.get("value") {
            Some(Value::String(s)) => Ok(MemoryValue::Text(s.clone())),
            Some(_) => Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "content_type=text/plain requires 'value' to be a string",
            )),
            None => Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "Missing 'value' parameter",
            )),
        },
        "application/json" => match params.get("value") {
            Some(v) => Ok(MemoryValue::Json(v.clone())),
            None => Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "Missing 'value' parameter",
            )),
        },
        "application/octet-stream" => match params.get("value_b64").and_then(|v| v.as_str()) {
            Some(b64) => decode_b64(b64)
                .map(MemoryValue::Binary)
                .map_err(|e| JsonRpcResponse::invalid_params(id.clone(), e)),
            None => Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "content_type=application/octet-stream requires 'value_b64'",
            )),
        },
        other => Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("unsupported content_type: {other}"),
        )),
    }
}

fn decode_b64(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("invalid base64: length must be multiple of 4".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut buf = [0u8; 4];
    for chunk in bytes.chunks(4) {
        for (i, &b) in chunk.iter().enumerate() {
            buf[i] = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => 64,
                _ => return Err(format!("invalid base64 char: {:?}", b as char)),
            };
        }
        let pad = chunk.iter().filter(|&&b| b == b'=').count();
        out.push((buf[0] << 2) | (buf[1] >> 4));
        if pad < 2 {
            out.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if pad < 1 {
            out.push((buf[2] << 6) | buf[3]);
        }
    }
    Ok(out)
}

fn encode_b64(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let b = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 6) & 0x3F) as usize] as char);
        out.push(ALPHA[(b & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let b = (bytes[i] as u32) << 16;
        out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(ALPHA[((b >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((b >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn value_to_json(v: &MemoryValue) -> Value {
    match v {
        MemoryValue::Text(s) => json!({
            "kind": "text",
            "content_type": "text/plain",
            "value": s,
        }),
        MemoryValue::Json(j) => json!({
            "kind": "json",
            "content_type": "application/json",
            "value": j,
        }),
        MemoryValue::Binary(b) => json!({
            "kind": "binary",
            "content_type": "application/octet-stream",
            "value_b64": encode_b64(b),
            "size": b.len(),
        }),
    }
}

fn entry_to_json(entry: &MemoryEntry) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("scope".into(), json!(entry.scope));
    obj.insert("key".into(), json!(entry.key));
    obj.insert("version".into(), json!(entry.version));
    obj.insert("created_at".into(), json!(entry.created_at));
    obj.insert("updated_at".into(), json!(entry.updated_at));
    obj.insert("expires_at".into(), json!(entry.expires_at));
    if let Some(owner) = &entry.owner {
        obj.insert("owner".into(), json!(owner));
    }
    if let Some(map) = value_to_json(&entry.value).as_object() {
        for (k, vv) in map {
            obj.insert(k.clone(), vv.clone());
        }
    }
    Value::Object(obj)
}

/// Secret 응답은 owner를 공개하지 않는다.
fn secret_entry_to_json(entry: &MemoryEntry) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("scope".into(), json!(entry.scope));
    obj.insert("key".into(), json!(entry.key));
    obj.insert("version".into(), json!(entry.version));
    obj.insert("created_at".into(), json!(entry.created_at));
    obj.insert("updated_at".into(), json!(entry.updated_at));
    obj.insert("expires_at".into(), json!(entry.expires_at));
    if let Some(map) = value_to_json(&entry.value).as_object() {
        for (k, vv) in map {
            obj.insert(k.clone(), vv.clone());
        }
    }
    Value::Object(obj)
}

fn stats_to_json(s: &MemoryStats) -> Value {
    json!({
        "scope": s.scope,
        "entries": s.entries,
        "bytes": s.bytes,
    })
}

/// 기존 오류 코드와 메시지를 유지한다. writer 결과는 오류를 빌려 주므로 참조로 받는다.
pub(crate) fn map_error_ref(id: Value, err: &MemoryError) -> JsonRpcResponse {
    use MemoryError::*;
    match err {
        NotFound { scope, key } => {
            JsonRpcResponse::error(id, -32004, format!("not_found: {scope}/{key}"))
        }
        AlreadyExists { scope, key } => {
            JsonRpcResponse::error(id, -32009, format!("already_exists: {scope}/{key}"))
        }
        CasConflict { expected, actual } => JsonRpcResponse::error(
            id,
            -32005,
            format!("cas_conflict: expected v{expected}, got v{actual}"),
        ),
        OwnedByOther { owner } => {
            JsonRpcResponse::error(id, -32006, format!("owned_by_other: {owner}"))
        }
        QuotaExceeded { area, used, limit } => {
            let area_str = match area {
                MemoryArea::Regular => "regular",
                MemoryArea::Secret => "secret",
            };
            JsonRpcResponse::error(
                id,
                -32007,
                format!("quota_exceeded ({area_str}): used {used}, limit {limit}"),
            )
        }
        InvalidKey(msg) => JsonRpcResponse::invalid_params(id, format!("invalid_key: {msg}")),
        InvalidScope(msg) => JsonRpcResponse::invalid_params(id, format!("invalid_scope: {msg}")),
        InvalidOwner(msg) => JsonRpcResponse::invalid_params(id, format!("invalid_owner: {msg}")),
        InvalidContentType(msg) => {
            JsonRpcResponse::invalid_params(id, format!("invalid_content_type: {msg}"))
        }
        ValueTooLarge { actual, max } => JsonRpcResponse::error(
            id,
            -32007,
            format!("value_too_large: {actual} bytes > {max}"),
        ),
        // 기존 오류 코드와 메시지는 유지하고, 원인별 처리를 위한 storage_failure를 추가한다.
        Db(e) => {
            let failure = tasty_memory::StorageFailure::classify(e);
            JsonRpcResponse::error_with_data(
                id,
                -32603,
                format!("memory db error: {e}"),
                json!({ "storage_failure": failure.as_str() }),
            )
        }
    }
}

/// 대체 메모리 저장소의 쓰기에는 durable:false를 붙인다. 정상 저장소에서는 생략한다.
/// 같은 저장소를 쓰는 agent/approval/meta/telemetry/session도 이 규칙을 따른다(docs/design/systems/storage-contract.md).
pub(crate) fn written(core: &AppServices, id: Value, body: Value) -> JsonRpcResponse {
    mark_durability(core, JsonRpcResponse::success(id, body))
}

/// 성공 응답의 객체에만 durable 표시를 더한다. 오류 응답은 그대로 둔다.
pub(crate) fn mark_durability(core: &AppServices, mut resp: JsonRpcResponse) -> JsonRpcResponse {
    if core.memory_init_fallback().is_some()
        && let Some(Value::Object(obj)) = resp.result.as_mut()
    {
        obj.insert("durable".into(), json!(false));
    }
    resp
}

pub fn handle_put(
    core: &AppServices,
    engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = reject_closed_surface_scope(engine, &scope, &id) {
        return e;
    }
    let key = match require_key(params, &id) {
        Ok(k) => k.to_string(),
        Err(e) => return e,
    };
    if let Err(e) = reject_host_key(caller, &key, &id) {
        return e;
    }
    let value = match parse_value(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let opts = PutOpts {
        expires_at: p_try!(params::opt_i64(params, "expires_at", &id)),
        cas: p_try!(params::opt_int::<u64>(params, "cas", &id)),
    };
    let owner = caller.owner().to_string();
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| s.put(&owner, &scope, &key, &value, &opts),
        |core, id, result| match result {
            Ok(version) => written(core, id, json!({ "ok": true, "version": version })),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_get(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let key = match require_key(params, &id) {
        Ok(k) => k.to_string(),
        Err(e) => return e,
    };
    if let Err(e) = reject_host_key(caller, &key, &id) {
        return e;
    }
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| s.get(&scope, &key),
        |_, id, result| match result {
            Ok(Some(entry)) => JsonRpcResponse::success(id, entry_to_json(&entry)),
            Ok(None) => JsonRpcResponse::success(id, Value::Null),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_delete(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let key = match require_key(params, &id) {
        Ok(k) => k.to_string(),
        Err(e) => return e,
    };
    if let Err(e) = reject_host_key(caller, &key, &id) {
        return e;
    }
    let cas = p_try!(params::opt_int::<u64>(params, "cas", &id));
    let owner = caller.owner().to_string();
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| s.delete(&owner, &scope, &key, cas),
        |core, id, result| match result {
            Ok(()) => written(core, id, json!({ "ok": true })),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_list(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let opts = ListOpts {
        prefix: params
            .get("prefix")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        limit: p_try!(params::opt_int::<usize>(params, "limit", &id)),
        since: p_try!(params::opt_i64(params, "since", &id)),
        until: p_try!(params::opt_i64(params, "until", &id)),
        offset: p_try!(params::opt_int::<usize>(params, "offset", &id)),
    };
    let caller = caller.clone();
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| s.list(&scope, &opts),
        move |_, id, result| match result {
            Ok(entries) => {
                let entries = hide_host_keys(&caller, entries);
                let arr: Vec<Value> = entries.iter().map(entry_to_json).collect();
                JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
            }
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_exists(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let key = match require_key(params, &id) {
        Ok(k) => k.to_string(),
        Err(e) => return e,
    };
    if let Err(e) = reject_host_key(caller, &key, &id) {
        return e;
    }
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| s.exists(&scope, &key),
        |_, id, result| match result {
            Ok(b) => JsonRpcResponse::success(id, json!({ "exists": b })),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_count(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let prefix = params
        .get("prefix")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    // 개수에서도 호스트 키를 제외한다. prefix가 일부만 겹치면 전체에서 호스트 키 수를 뺀다.
    let hide_host = is_permissioned(caller);
    if hide_host && prefix.as_deref().is_some_and(is_host_key) {
        return JsonRpcResponse::success(id, json!({ "count": 0 }));
    }
    let bytes = super::deferred::params_bytes(params);
    core.reply_unit(
        id,
        bytes,
        move |s| {
            let prefix = prefix.as_deref();
            let total = s.count(&scope, prefix)?;
            let hidden = if hide_host && prefix_may_include_host(prefix) {
                s.count(&scope, Some(HOST_KEY_NAMESPACE))?
            } else {
                0
            };
            Ok(total.saturating_sub(hidden))
        },
        |_, id, result| match result {
            Ok(n) => JsonRpcResponse::success(id, json!({ "count": n })),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_scopes(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    core.reply_unit(
        id,
        super::deferred::params_bytes(params),
        |s| s.scopes(),
        |_, id, result| match result {
            Ok(list) => JsonRpcResponse::success(id, json!({ "scopes": list })),
            Err(e) => map_error_ref(id, e),
        },
    )
}

pub fn handle_stats(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match optional_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    core.reply_unit(
        id,
        super::deferred::params_bytes(params),
        move |s| s.stats(scope.as_ref()),
        |_, id, result| match result {
            Ok(stats) => JsonRpcResponse::success(id, stats_to_json(&stats)),
            Err(e) => map_error_ref(id, e),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        decode_b64, encode_b64, hide_host_keys, is_host_key, prefix_may_include_host,
        reject_host_key,
    };
    use serde_json::json;
    use std::collections::HashSet;
    use std::sync::Arc;
    use tasty_ipc::caller::CallerContext;
    use tasty_memory::{MemoryEntry, MemoryValue, Scope};

    fn plugin() -> CallerContext {
        CallerContext::Plugin {
            plugin_id: "com.example.plugin".into(),
        }
    }

    fn agent() -> CallerContext {
        CallerContext::Agent {
            agent_id: "child:1".into(),
            permissions: Arc::new(HashSet::new()),
        }
    }

    fn entry(key: &str) -> MemoryEntry {
        MemoryEntry {
            scope: Scope::Global.as_token(),
            key: key.to_string(),
            value: MemoryValue::Text(String::new()),
            created_at: 0,
            updated_at: 0,
            expires_at: None,
            version: 1,
            owner: Some("_host".into()),
        }
    }

    #[test]
    fn the_audit_key_space_is_closed_to_permissioned_callers() {
        let id = json!(1);
        for caller in [plugin(), agent()] {
            assert!(
                reject_host_key(&caller, "tasty.audit.0001", &id).is_err(),
                "권한 caller 가 감사 로그 키를 지목할 수 있다"
            );
        }
    }

    #[test]
    fn the_local_caller_still_reaches_host_keys() {
        let id = json!(1);
        assert!(reject_host_key(&CallerContext::local(), "tasty.audit.0001", &id).is_ok());
        let kept = hide_host_keys(&CallerContext::local(), vec![entry("tasty.audit.0001")]);
        assert_eq!(
            kept.len(),
            1,
            "Local 에게서 호스트 키를 숨기면 CLI 가 깨진다"
        );
    }

    // 읽지 못한 디렉터리를 위반이 없는 것으로 처리하지 않도록 실패를 전파한다.
    fn collect_rs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("스캔 디렉토리 `{}` 를 열지 못했다: {e}", dir.display()));
        for entry in entries {
            let entry =
                entry.unwrap_or_else(|e| panic!("`{}` 의 항목을 읽지 못했다: {e}", dir.display()));
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                collect_rs(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    // tasty-와 tastyx.는 예약된 tasty. 접두어에 속하지 않는다.
    #[test]
    fn the_reservation_does_not_catch_plugin_keys() {
        for key in ["my.state", "tasty-thing.state", "tastyx.y", "", "tast"] {
            assert!(!is_host_key(key), "{key} 가 잘못 예약에 걸린다");
        }
    }

    // 실제 키 상수를 수집해 예약 접두어에 포함되는지 검사한다.
    // 수집 누락이나 새 namespace를 놓치지 않도록 개수도 확인한다.
    #[test]
    fn every_declared_host_key_space_is_inside_the_reservation() {
        const EXPECTED: usize = 26;
        // 이름만으로 구별할 수 없는 memory 외 저장소 키.
        const NOT_MEMORY_KEYS: &[(&str, &str)] = &[(
            "HOOKS_REGISTRY_KEY",
            "mlua 의 named registry 키 — memory store 가 아니다",
        )];
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        for dir in ["src", "crates"] {
            collect_rs(&root.join(dir), &mut files);
        }
        assert!(files.len() > 500, "스캔 파일이 {}개뿐이다", files.len());

        let mut found: Vec<(String, String)> = Vec::new();
        for f in &files {
            let src = std::fs::read_to_string(f)
                .unwrap_or_else(|e| panic!("`{}` 를 읽지 못했다: {e}", f.display()));
            for line in src.lines() {
                let Some(rest) = line.split_once("const ").map(|(_, r)| r) else {
                    continue;
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                    .collect();
                if !(name.ends_with("_KEY") || name.ends_with("_KEY_PREFIX")) {
                    continue;
                }
                let Some((_, tail)) = rest.split_once("&str = \"") else {
                    continue;
                };
                let Some((value, _)) = tail.split_once('"') else {
                    continue;
                };
                if value.starts_with("tasty") {
                    found.push((name, value.to_string()));
                }
            }
        }
        found.sort();
        found.dedup();

        let escaping: Vec<&(String, String)> = found
            .iter()
            .filter(|(n, v)| !is_host_key(v) && !NOT_MEMORY_KEYS.iter().any(|(e, _)| e == n))
            .collect();
        assert!(
            escaping.is_empty(),
            "`tasty` 로 시작하는 키 공간인데 예약(`{}`) 밖이다: {escaping:?}\n\
             예약은 접두 하나뿐이므로, 여기 걸리면 그 키는 권한 caller 의 raw kv 에서 \
             그대로 열려 있다. memory store 키가 아니라면 NOT_MEMORY_KEYS 에 이유와 \
             함께 적는다 — 그 목록이 이 스캔이 이름만으로 못 가르는 자리의 전부다.",
            super::HOST_KEY_NAMESPACE
        );
        assert_eq!(
            found.len(),
            EXPECTED,
            "호스트 키 상수가 {}개다(고정값 {EXPECTED}): {found:#?}\n\n\
             늘었다면 새 키 공간이 생긴 것이다 — 위 목록에서 그것이 `tasty.` 안인지 \
             확인하고 이 상수를 올린다. 줄었다면 없어진 것을 확인하고 내린다.",
            found.len()
        );
        for (name, _) in NOT_MEMORY_KEYS {
            assert!(
                found.iter().any(|(n, _)| n == name),
                "면제 목록의 `{name}` 이 더 이상 없다 — 면제가 낡았다"
            );
        }
    }

    #[test]
    fn enumerating_paths_hide_host_keys_instead_of_failing() {
        let visible = hide_host_keys(
            &plugin(),
            vec![
                entry("tasty.audit.0001"),
                entry("my.state"),
                entry("tasty.bb.x"),
            ],
        );
        assert_eq!(
            visible.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
            vec!["my.state"]
        );
    }

    // 모든 일반 KV 핸들러가 예약 키를 제한하는지 확인한다.
    // scopes/stats는 키와 값을 반환하지 않고 gc는 local_only이므로 제외한다.
    #[test]
    fn every_raw_kv_handler_consults_the_reserved_namespace() {
        const EXEMPT: &[&str] = &["handle_scopes", "handle_stats", "handle_gc"];
        const ENTRY_POINTS: &[&str] = &[
            "reject_host_key",
            "hide_host_keys",
            "prefix_may_include_host",
        ];
        let sources = [
            ("memory.rs", include_str!("memory.rs")),
            ("memory/advanced.rs", include_str!("memory/advanced.rs")),
        ];
        let mut seen = Vec::new();
        let mut naked = Vec::new();
        for (file, src) in sources {
            let mut rest = src;
            while let Some(at) = rest.find("\npub fn handle_") {
                let head = &rest[at + 1..];
                let name: String = head[7..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let body_end = head[1..].find("\npub fn ").map_or(head.len(), |i| i + 1);
                let body = &head[..body_end];
                seen.push(name.clone());
                let guarded = ENTRY_POINTS.iter().any(|e| body.contains(e));
                if !guarded && !EXEMPT.contains(&name.as_str()) {
                    naked.push(format!("  {file}::{name}"));
                }
                rest = &head[body_end..];
            }
        }
        assert!(
            seen.len() >= 12,
            "핸들러를 {}개밖에 못 찾았다 — 파서가 낡았다: {seen:?}",
            seen.len()
        );
        assert!(
            naked.is_empty(),
            "raw kv 핸들러가 예약 namespace 정책을 안 거친다 ({}건):\n{}\n\n\
             키를 지목하는 경로면 `reject_host_key`, 열거하는 경로면 `hide_host_keys`, \
             세는 경로면 `prefix_may_include_host` 를 거쳐야 한다. 키도 값도 내보내지 않는 경로라면 이 테스트의 EXEMPT 에 \
             이유와 함께 적는다 — 그 목록이 이 정책이 답하지 못하는 자리의 전부다.",
            naked.len(),
            naked.join("\n")
        );
        for name in EXEMPT {
            assert!(
                seen.iter().any(|s| s == name),
                "면제 목록의 `{name}` 이 더 이상 존재하지 않는다 — 면제가 낡았다"
            );
        }
    }

    // 키를 숨겨도 개수로 감사 기록의 규모가 드러나지 않아야 한다.
    #[test]
    fn counting_needs_correction_exactly_when_the_prefix_can_include_host_keys() {
        assert!(prefix_may_include_host(None));
        assert!(prefix_may_include_host(Some("")));
        assert!(prefix_may_include_host(Some("ta")));
        assert!(prefix_may_include_host(Some("tasty.")));
        assert!(!prefix_may_include_host(Some("my.")));
        assert!(!prefix_may_include_host(Some("tasty-")));
    }

    #[test]
    fn base64_roundtrip() {
        for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let encoded = encode_b64(input);
            let decoded = decode_b64(&encoded).unwrap();
            assert_eq!(decoded, input, "roundtrip failed for {input:?}");
        }
    }

    #[test]
    fn base64_invalid_inputs_rejected() {
        assert!(decode_b64("abc").is_err());
        assert!(decode_b64("ab*=").is_err());
    }

    // 원인은 data에 추가하고 기존 코드·메시지 및 일반 거부 응답은 유지한다.
    #[test]
    fn a_storage_failure_keeps_its_message_and_adds_its_cause() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let expected_message = format!("memory db error: {busy}");
        let resp = super::map_error_ref(json!(7), &tasty_memory::MemoryError::Db(busy));
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32603);
        assert_eq!(err.message, expected_message);
        assert_eq!(err.data, Some(json!({ "storage_failure": "busy" })));

        let refused = super::map_error_ref(
            json!(8),
            &tasty_memory::MemoryError::NotFound {
                scope: "global".into(),
                key: "k".into(),
            },
        );
        assert!(
            refused.error.expect("error").data.is_none(),
            "요청 거부에 저장 실패 원인을 붙이면 안 된다"
        );
    }
}
