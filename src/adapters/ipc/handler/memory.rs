//! `memory.*` / `memory.secret.*` / `memory.bb.*` / `memory.plan.*` / `memory.cache.*` /
//! `memory.goal.*` IPC 핸들러. 도메인별로 sub-module (bb / plan / cache / goal /
//! secret) 로 분리. 본 `mod.rs` 는 basic `memory.*` 와 공용 helpers 만 포함.
//!
//! `owner` 는 [`CallerContext`] 에서 도출하며 plugin 이 인자로 명시할 수 없다.

mod advanced;

pub mod bb;
pub mod cache;
pub mod goal;
pub mod plan;
pub mod secret;

#[cfg(test)]
mod durable_tests;

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

/// surface 스코프 오버레이용 명시 인자. 활성 surface 로 폴백하지 않는다 —
/// 포커스 독립성(`docs/design/policies/focus.md`).
///
/// `>= PTY_ID_BASE` 는 headless PTY id 공간이라 실재 surface 가 가질 수 없다 — 거부한다
/// (`docs/adr/0094-surface-id-space-bounded-below-pty-base.md`).
pub(super) fn require_surface_id(params: &Value, id: &Value) -> Result<u32, JsonRpcResponse> {
    params::opt_int::<u64>(params, "surface_id", id)?
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| crate::core::pty_registry::is_surface_id_space(*n))
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

use crate::core::Core;
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

/// `surface:<id>` scope 의 id 가 headless PTY id 공간이면 거부한다. `memory.*` 는 임의
/// scope 토큰을 받으므로 `surface_id` 파라미터 검증만으로는 오염을 막지 못한다 — 여기서
/// 같은 경계를 적용해야 `Scope::Surface(pty id)` 가 memory.db 에 심기지 않는다
/// (`docs/adr/0094-surface-id-space-bounded-below-pty-base.md`).
fn reject_pty_space_surface_scope(scope: &Scope, id: &Value) -> Result<(), JsonRpcResponse> {
    match scope {
        Scope::Surface(sid) if !crate::core::pty_registry::is_surface_id_space(*sid) => {
            Err(JsonRpcResponse::invalid_params(
                id.clone(),
                format!("invalid scope: surface id {sid} is inside the headless PTY id space"),
            ))
        }
        _ => Ok(()),
    }
}

/// 호스트가 소유한 키 namespace. `tasty.audit.` · `tasty.telemetry.` · `tasty.agent.` 등
/// 11 개 하위 namespace 가 여기 산다.
///
/// **raw `memory.*` kv 표면에서 이 namespace 는 권한 caller 에게 존재하지 않는다.**
/// regular memory 는 설계상 "모든 caller 가 읽는" 공유 네임스페이스인데(`tasty_memory`
/// 모듈 doc), 호스트가 자기 상태를 거기 두면서 전용 메서드로만 잠갔다 — 예컨대 감사
/// 로그의 `plugin.audit_*` 는 넷 다 `local_only()` 인데 같은 행이 `tasty.audit.` 키로
/// 앉아 있어 `memory.list` 로 읽히고 `memory.put` 으로 위조됐다. 잠근 문 옆에 잠기지
/// 않은 문이 있었던 것이고, 사용자가 승인 화면에서 본 것은 "메모리 읽기/쓰기" 다.
///
/// 접두 하나로 예약하는 이유: 하위 namespace 를 목록으로 들면 그 목록이 또 하나의
/// 손목록이 되어 새 호스트 namespace 가 생길 때마다 조용히 새는 자리가 늘어난다.
/// 근거·대안·재검토 조건은 [ADR-0141](../../../../docs/adr/0141-host-key-namespace-is-reserved-in-raw-memory-kv.md).
pub(super) const HOST_KEY_NAMESPACE: &str = "tasty.";

/// 권한 게이트를 받는 caller(plugin / agent)인가. `Local`(CLI·사용자)은 `ensure_allowed`
/// 가 무조건 통과시키는 신뢰 caller 라 여기서도 제한하지 않는다 — CLI 의
/// `memory list --prefix tasty.audit.` 은 그대로 동작해야 한다.
///
/// `is_plugin()` 이 아니라 권한 셋의 유무로 가른다. agent caller 도 같은 권한 모델을
/// 받으므로 함께 막혀야 하고, 이렇게 두면 권한을 받는 caller 종류가 새로 생겨도
/// 자동으로 덮인다.
fn is_permissioned(caller: &CallerContext) -> bool {
    caller.permissions().is_some()
}

fn is_host_key(key: &str) -> bool {
    key.starts_with(HOST_KEY_NAMESPACE)
}

/// 이 prefix 로 센 수에 호스트 키가 섞일 수 있는가. prefix 가 없거나 호스트
/// namespace 의 앞토막(`"ta"`)이면 섞인다.
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

/// 열거 경로(list / query / export)의 결과에서 호스트 키를 제거한다. 여기서는 거부가
/// 아니라 필터인 이유는, 이 메서드들이 prefix 없이도 불릴 수 있어 "지목했는가" 로
/// 가를 수 없기 때문이다 — 호스트 키가 **애초에 없는 것처럼** 보여야 한다.
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

/// Regular entry — `owner` 가 응답에 포함된다.
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

/// Secret entry — plugin 에게 `owner` 차원을 노출하지 않으므로 무조건 생략.
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

fn map_error(id: Value, err: MemoryError) -> JsonRpcResponse {
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
        // 코드와 문장은 원인 분류가 생기기 전 그대로다 — 소비자가 이미 그것으로
        // 분기한다. 원인 갈래는 `data` 에 **덧붙인다**: 잠김·용량·I/O·손상은 처방이
        // 달라 문장 파싱 없이 갈려야 한다(`tasty_memory::StorageFailure`).
        Db(e) => {
            let failure = tasty_memory::StorageFailure::classify(&e);
            JsonRpcResponse::error_with_data(
                id,
                -32603,
                format!("memory db error: {e}"),
                json!({ "storage_failure": failure.as_str() }),
            )
        }
    }
}

/// 쓰기 성공 응답. 저장소가 `memory.db` 초기화 실패의 in-memory 대체면 `durable: false`
/// 를 더한다 — 그 쓰기는 프로세스와 함께 사라진다. `ok` 는 그대로 두고(기존 호출자는
/// 그것만 본다), 정상 저장소에서는 칸을 싣지 않아 응답이 종전과 같다(ADR-0485).
///
/// `memory.*` 밖에서 같은 `memory.db` 에 쓰는 메서드(`agent.*` · `approval.*` ·
/// `surface.meta.*` · `telemetry.*` · `session.*`)도 이것이나 [`mark_durability`] 로
/// 답한다 — 어느 이름이 쓰기 계열인지는 `tasty_ipc::method_meta` 의 효과 분류가 정하고,
/// 시험 `every_memory_write_reports_a_fallback_store_as_not_durable` 가 그 표로 잰다.
pub(crate) fn written(core: &Core, id: Value, body: Value) -> JsonRpcResponse {
    mark_durability(core, JsonRpcResponse::success(id, body))
}

/// 이미 만든 응답에 [`written`] 과 같은 칸을 단다. 성공 응답의 **객체** 결과에만 붙고,
/// 오류 응답은 그대로다 — 실패한 쓰기는 남은 것이 없어 durable 을 말할 대상이 없다.
/// 성공 경로가 여럿인 핸들러가 반환값 하나를 감싸는 데 쓴다.
pub(crate) fn mark_durability(core: &Core, mut resp: JsonRpcResponse) -> JsonRpcResponse {
    if core.memory_init_fallback().is_some()
        && let Some(Value::Object(obj)) = resp.result.as_mut()
    {
        obj.insert("durable".into(), json!(false));
    }
    resp
}

// ============================================================
// Regular `memory.*`
// ============================================================

pub fn handle_put(
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    let value = match parse_value(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let opts = PutOpts {
        expires_at: p_try!(params::opt_i64(params, "expires_at", &id)),
        cas: p_try!(params::opt_int::<u64>(params, "cas", &id)),
    };
    let owner = caller.owner().to_string();

    match core.with_memory(|s| s.put(&owner, &scope, &key, &value, &opts)) {
        Ok(version) => written(core, id, json!({ "ok": true, "version": version })),
        Err(e) => map_error(id, e),
    }
}

pub fn handle_get(
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    match core.with_memory(|s| s.get(&scope, &key)) {
        Ok(Some(entry)) => JsonRpcResponse::success(id, entry_to_json(&entry)),
        Ok(None) => JsonRpcResponse::success(id, Value::Null),
        Err(e) => map_error(id, e),
    }
}

pub fn handle_delete(
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    match core.with_memory(|s| s.delete(&owner, &scope, &key, cas)) {
        Ok(()) => written(core, id, json!({ "ok": true })),
        Err(e) => map_error(id, e),
    }
}

pub fn handle_list(
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    match core.with_memory(|s| s.list(&scope, &opts)) {
        Ok(entries) => {
            let entries = hide_host_keys(caller, entries);
            let arr: Vec<Value> = entries.iter().map(entry_to_json).collect();
            JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
        }
        Err(e) => map_error(id, e),
    }
}

pub fn handle_exists(
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    match core.with_memory(|s| s.exists(&scope, &key)) {
        Ok(b) => JsonRpcResponse::success(id, json!({ "exists": b })),
        Err(e) => map_error(id, e),
    }
}

pub fn handle_count(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match require_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let prefix = params.get("prefix").and_then(|v| v.as_str());
    if !is_permissioned(caller) {
        return match core.with_memory(|s| s.count(&scope, prefix)) {
            Ok(n) => JsonRpcResponse::success(id, json!({ "count": n })),
            Err(e) => map_error(id, e),
        };
    }
    // 권한 caller 에게 호스트 키는 없는 것이다 — 세는 수에서도 빠져야 한다.
    // prefix 가 호스트 namespace 안이면 셀 것이 없고, namespace 를 걸치거나
    // 없으면 전체에서 호스트 키 수를 뺀다.
    match prefix {
        Some(p) if is_host_key(p) => JsonRpcResponse::success(id, json!({ "count": 0 })),
        _ => {
            let visible = core.with_memory(|s| {
                let total = s.count(&scope, prefix)?;
                let hidden = if prefix_may_include_host(prefix) {
                    s.count(&scope, Some(HOST_KEY_NAMESPACE))?
                } else {
                    0
                };
                Ok(total.saturating_sub(hidden))
            });
            match visible {
                Ok(n) => JsonRpcResponse::success(id, json!({ "count": n })),
                Err(e) => map_error(id, e),
            }
        }
    }
}

pub fn handle_scopes(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    _params: &Value,
) -> JsonRpcResponse {
    match core.with_memory(|s| s.scopes()) {
        Ok(list) => JsonRpcResponse::success(id, json!({ "scopes": list })),
        Err(e) => map_error(id, e),
    }
}

pub fn handle_stats(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let scope = match optional_scope(params, &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match core.with_memory(|s| s.stats(scope.as_ref())) {
        Ok(stats) => JsonRpcResponse::success(id, stats_to_json(&stats)),
        Err(e) => map_error(id, e),
    }
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
            permissions: Arc::new(HashSet::new()),
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

    /// 감사 로그는 `plugin.audit_*` 가 넷 다 `local_only()` 라 전용 문이 닫혀 있는데,
    /// 같은 행이 `tasty.audit.` 키로 공유 kv 에 앉아 있었다. 그 옆문을 막는다.
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

    /// CLI·사용자는 `ensure_allowed` 가 무조건 통과시키는 신뢰 caller 다. 여기서
    /// 막으면 `tasty memory list --prefix tasty.audit.` 이 깨진다.
    #[test]
    fn the_local_caller_still_reaches_host_keys() {
        let id = json!(1);
        assert!(reject_host_key(&CallerContext::Local, "tasty.audit.0001", &id).is_ok());
        let kept = hide_host_keys(&CallerContext::Local, vec![entry("tasty.audit.0001")]);
        assert_eq!(
            kept.len(),
            1,
            "Local 에게서 호스트 키를 숨기면 CLI 가 깨진다"
        );
    }

    /// 디렉토리 순회. 실패를 삼키지 않는다 — 못 읽은 디렉토리는 위반이 없는 디렉토리와
    /// 구분되지 않으므로, 조용히 건너뛰면 가드가 초록인 채로 눈이 먼다.
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

    /// plugin 자기 키는 예약에 걸리지 않는다. `tasty-` 는 점이 없어 namespace 가 아니고,
    /// `tastyx.` 도 마찬가지다 — 예약은 `tasty` 로 시작하는 이름이 아니라 `tasty.`
    /// namespace 다.
    #[test]
    fn the_reservation_does_not_catch_plugin_keys() {
        for key in ["my.state", "tasty-thing.state", "tastyx.y", "", "tast"] {
            assert!(!is_host_key(key), "{key} 가 잘못 예약에 걸린다");
        }
    }

    /// 접두 하나로 예약하는 것의 값은 **호스트 키 공간을 목록으로 들지 않아도 된다**
    /// 는 데 있다. 그 값이 실제로 있는지 손목록 대신 레포에서 직접 센다 — 선언된
    /// 키 상수를 전부 긁어, `tasty.` 로 시작하는 것이 하나도 빠짐없이 예약에 걸리는지
    /// 본다.
    ///
    /// 개수를 함께 고정하는 이유: 스캐너가 조용히 0 건을 세면 이 테스트는 초록인 채로
    /// 눈이 먼다. 호스트 키 공간이 늘거나 줄면 그 자리에서 걸리고, 그때 사람이
    /// "새로 생긴 것이 정말 `tasty.` 안인가" 를 한 번 본다.
    #[test]
    fn every_declared_host_key_space_is_inside_the_reservation() {
        const EXPECTED: usize = 20;
        /// `tasty` 로 시작하지만 memory store 키가 아닌 상수. 이름만 보고는 가를 수 없어
        /// (스캐너에 그 오라클이 없다) 여기 이유와 함께 적는다.
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

    /// 열거 경로는 거부가 아니라 필터다 — prefix 없이도 불리므로 "지목했는가" 로
    /// 가를 수 없다.
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

    /// raw kv 표면의 핸들러는 **전부** 예약 정책을 거쳐야 한다. 이 결함의 형태가
    /// 바로 "한 경로에만 문을 달았다" 였다 — `plugin.audit_*` 는 잠갔는데 같은
    /// 데이터로 가는 `memory.*` 는 안 잠갔다. 새 핸들러가 문 없이 추가되면 같은
    /// 형태가 다시 생기므로, 정책을 안 거치는 핸들러는 여기 이름으로 적혀야 한다.
    ///
    /// 면제된 셋은 **키 이름도 값도 내보내지 않는다**: `scopes` 는 scope 토큰만,
    /// `stats` 는 집계 개수·바이트만 돌려주고, `gc` 는 `local_only()` 라 권한
    /// caller 가 애초에 못 부른다.
    #[test]
    fn every_raw_kv_handler_consults_the_reserved_namespace() {
        const EXEMPT: &[&str] = &["handle_scopes", "handle_stats", "handle_gc"];
        // 정책 진입점은 경로의 모양마다 하나씩 셋이다: 키를 지목하면 거부,
        // 열거하면 필터, 세면 보정.
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

    /// 세는 수에서도 빠져야 한다 — 내용을 못 봐도 개수는 감사 기록의 존재와 규모를
    /// 드러낸다.
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

    /// 저장소 실패는 원인 갈래를 `data` 로 싣고, 코드와 문장은 전과 같다.
    ///
    /// 문장·코드가 바뀌면 이미 그것으로 분기하는 소비자가 깨지고, `data` 가 빠지면
    /// 잠김과 용량 부족이 다시 같은 오류로 보인다 — 두 방향을 다 본다.
    #[test]
    fn a_storage_failure_keeps_its_message_and_adds_its_cause() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let expected_message = format!("memory db error: {busy}");
        let resp = super::map_error(json!(7), tasty_memory::MemoryError::Db(busy));
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32603);
        assert_eq!(err.message, expected_message);
        assert_eq!(err.data, Some(json!({ "storage_failure": "busy" })));

        let refused = super::map_error(
            json!(8),
            tasty_memory::MemoryError::NotFound {
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
