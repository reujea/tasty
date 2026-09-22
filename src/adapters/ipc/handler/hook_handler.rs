//! `hook_handler.*` IPC 핸들러 — 공유 훅 핸들러 레지스트리 조회/재로드/수동 발화.
//!
//! 파일 핸들러(`file_handler.*`)를 선례로 미러링한다. 상태는 `crate::hook_handler`
//! 전역 싱글턴 레지스트리라 engine/state 를 받지 않는다(단 dispatch 는 IpcSequence
//! 실행에 host injector 가 필요해 `core` 를 받는다).
//!
//! **불가침 원칙 2·3**: 핸들러 조회/발화는 에이전트 작업이라 IPC+CLI 양면 노출.
//! 대상은 id 로 직접 지정하고 list 는 전 범위(비활성 포함) 조회 — 사용자 포커스/
//! 상태에 부수효과 없음.
//!
//! ## 불변식
//! - **데이터/흐름 분리**: 발화되는 `IpcCall.method` 는 owner 가 등록 시 고정한
//!   리터럴이며 dispatch 페이로드는 `params` 값 슬롯에만 치환된다([`crate::hook_handler::exec`]).
//! - **단방향(fire-and-forget)**: dispatch 실행 결과는 시퀀스 실행기 스레드 안에 갇히고
//!   JSON 응답에 실행 결과가 실리지 않는다(응답은 "수리됨" ACK 만).
//!
//! ## `get` · `upsert` 가 왜 `local_only` 인가
//!
//! 시퀀스는 **Local 권한으로 실행된다.** plugin 이 임의 시퀀스를 쓸 수 있으면 자기
//! 권한 집합을 넘어선 IPC escalation 이 되므로, `webhook.register` 가 plugin 의 인라인
//! `sequence` 를 거부하는 것과 **같은 이유로** 이 둘도 local 전용이다. 그 게이트는
//! `crates/tasty-ipc/src/method_meta.rs` 의 `local_only()` 가 건다.
//!
//! 데이터/흐름 분리(ADR-0046)와 충돌하지 않는다 — 그 불변식이 막는 것은 **페이로드가
//! `method` 자리에 닿는 것**이고, 여기서 `method` 를 정하는 것은 owner 자신이다.
//! 웹훅 발신자는 이 경로에 도달할 수 없다.
//!
//! ## `upsert` 가 닿지 않는 자리 (실측)
//!
//! **이미 등록된 웹훅은 안 바뀐다.** 웹훅 엔트리는 등록 시점에 `calls` 스냅샷을 직접
//! 소유하고(`src/webhook/persist.rs` 의 `PersistedWebhook::calls`), 발화 시 그 스냅샷을
//! 실행한다(`src/webhook/registry.rs` 의 매칭이 `entry.calls` 를 복제해 넘긴다) —
//! 핸들러를 다시 조회하지 않는다. 인라인 `sequence` 로 등록한 것뿐 아니라
//! `--handler <id>` 로 바인딩한 것도 등록 시점에 복사된다. 그래서 upsert 가 바꾸는 것은
//! `hook_handler.dispatch` 와 **앞으로의** 바인딩이고, 이미 발급된 URL 이 무엇을 하는지는
//! 그대로다. 그것이 owner 가 등록 시 흐름을 고정한다는 ADR-0046 의 모양이다.

use serde_json::json;

use crate::core::Core;
use crate::hook_handler::{
    self, HookHandlerAction, HookHandlerId, HookShellEnv, HookSource, IpcCall, SequenceNotQueued,
    SequenceOrigin, SubstitutionContext, UserHookHandlerActionDecl, UserHookHandlerUpsertDecl,
    build_env, enqueue_sequence, spawn_shell,
};
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;

/// `hook_handler.list` — 등록된 모든 훅 핸들러(비활성 포함, 포커스 독립·전 범위).
///
/// 각 항목: id / source / priority / owner / action kind(+steps) / disabled /
/// display_name_i18n_key / webhook_bindable. action 의 실제 IPC 호출 목록·셸 명령은
/// 노출하지 않는다(요약만).
pub fn handle_list(id: serde_json::Value) -> JsonRpcResponse {
    let items: Vec<_> = hook_handler::global()
        .all_handlers_including_disabled()
        .into_iter()
        .map(|h| {
            let (action_kind, steps) = match &h.action {
                HookHandlerAction::IpcSequence { calls } => ("ipc_sequence", Some(calls.len())),
                HookHandlerAction::ShellCommand { .. } => ("shell_command", None),
            };
            json!({
                "id": h.id.0,
                "source": h.source,
                "priority": h.priority,
                "owner": h.owner.prefix(),
                "action": action_kind,
                "steps": steps,
                "disabled": h.disabled,
                "display_name_i18n_key": h.display_name_i18n_key,
                "webhook_bindable": h.action.is_webhook_bindable(),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "handlers": items }))
}

/// `hook_handler.reload` — `~/.tasty/hook-handlers.toml`(user 출처) 재로드.
///
/// host embedded 기본값 + plugin contribution 은 영향받지 않는다(user 출처만 교체).
/// 파일 핸들러 `file_handler.reload` 응답의 `{path, exists}` 를 미러링한다. 그쪽이 더한 `rejected`
/// (적용되지 않은 user 항목 보고, docs/adr/0426-file-handler-reload-reports-the-entries-it-dropped.md)는
/// 아직 없다.
pub fn handle_reload(id: serde_json::Value) -> JsonRpcResponse {
    let Some(path) = hook_handler::user_config_path() else {
        return JsonRpcResponse::internal_error(
            id,
            "cannot resolve tasty home for hook-handlers.toml",
        );
    };
    let exists = path.exists();
    hook_handler::global().reload_user_config(&path);
    JsonRpcResponse::success(
        id,
        json!({
            "path": path.display().to_string(),
            "exists": exists,
        }),
    )
}

/// `hook_handler.dispatch` — 등록된 핸들러를 id 로 **수동 발화**한다.
///
/// 파일 핸들러 `file_handler.dispatch`(임의 경로를 dispatch 흐름에 진입)의 훅 핸들러
/// 대응물 — 에이전트/CLI 가 트리거 없이 핸들러를 즉시 실행하는 진입점(테스트·자동화).
///
/// params:
/// - `id`: 발화할 핸들러 id (필수).
/// - `body` / `headers` / `query`: 치환 컨텍스트(선택). IpcSequence 핸들러의 `params`
///   값 슬롯(`${body.x}`/`${header.x}`/`${query.x}`)에 채워진다.
///
/// 응답은 "수리됨(accepted)" ACK 만 담는다 — 실행은 surface 훅과 같은 시퀀스 실행기 스레드에서
/// fire-and-forget 되고 결과가 응답에 실리지 않는다(단방향 불변식). 실행기에 넘기지 못했으면(대기
/// 시퀀스가 상한에 이름 · 실행기 스레드 없음) "실행하지 않았다" 오류로 답한다. 비활성 핸들러는 거부한다.
pub fn handle_dispatch(
    core: &Core,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(hid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    let hid = HookHandlerId::new(hid);
    let Some(handler) = hook_handler::global().get(&hid) else {
        return JsonRpcResponse::invalid_params(id, format!("hook handler '{hid}' not found"));
    };
    if handler.disabled {
        return JsonRpcResponse::invalid_params(id, format!("hook handler '{hid}' is disabled"));
    }

    match handler.action {
        HookHandlerAction::IpcSequence { calls } => {
            // injector 를 얻어 surface 훅과 같은 실행기 스레드에 넘긴다. 이 핸들러는 호스트 명령
            // 큐를 비우는 스레드에서 돌므로 여기서 스텝의 답을 기다릴 수 없다.
            let Some(injector) = core.host_ipc_injector_arc().get().cloned() else {
                return JsonRpcResponse::internal_error(
                    id,
                    "host IPC injector not initialized (dispatch unavailable before boot completes)",
                );
            };
            let ctx = build_context(params);
            let steps = calls.len();
            if let Err(e) = start_dispatched_sequence(&hid.0, &injector, &calls, ctx) {
                return JsonRpcResponse::internal_error(
                    id,
                    format!("hook handler '{hid}' not run — {e}"),
                );
            }
            JsonRpcResponse::success(
                id,
                json!({ "accepted": true, "id": hid.0, "action": "ipc_sequence", "steps": steps }),
            )
        }
        HookHandlerAction::ShellCommand { command, args } => {
            // 수동 발화 컨텍스트를 env 로 노출 — IpcSequence 가 `${body.*}` 치환으로
            // 받는 payload 를 셸은 `TASTY_HOOK_<KEY>` env 로 받는다(의미 대칭).
            let env = build_env(&HookShellEnv {
                event: hid.0.clone(),
                source: "dispatch",
                surface_id: None,
                payload: params
                    .get("body")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            });
            spawn_shell(command, args, env);
            JsonRpcResponse::success(
                id,
                json!({ "accepted": true, "id": hid.0, "action": "shell_command" }),
            )
        }
    }
}

/// 수동 발화한 IpcSequence 를 surface 훅과 **같은 실행기**에 줄 세운다 — 잇달아 발화한 시퀀스와
/// surface 훅의 시퀀스가 스텝 단위로 끼어들지 않고 넘겨받은 순서대로 하나씩 실행된다.
/// 근거: `docs/adr/0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md`.
fn start_dispatched_sequence(
    handler_id: &str,
    injector: &HostIpcInjector,
    calls: &[IpcCall],
    ctx: SubstitutionContext,
) -> Result<(), SequenceNotQueued> {
    let origin = SequenceOrigin::Dispatch;
    enqueue_sequence(origin, handler_id, injector, calls, ctx).inspect_err(|e| {
        tracing::error!("{origin} IpcSequence '{handler_id}' not run — {e}");
    })
}

/// `hook_handler.get` — 핸들러 한 건을 id 로 조회한다. **action 본문까지** 싣는다.
///
/// `list` 는 `steps` 수만 요약하므로 시퀀스를 고치려면 지금 무엇이 들었는지 읽을 길이
/// 있어야 한다 — 그 자리다. 응답의 `action` 은 [`handle_upsert`] 의 `action` 파라미터와
/// **같은 모양**이라 읽은 것을 그대로 고쳐 되돌려 보낼 수 있다(round-trip).
///
/// 돌려주는 것은 **병합된 유효 핸들러**다(host/plugin 기본 + user override). user
/// 기여분만 따로 보고 싶으면 `~/.tasty/hook-handlers.toml` 이 그 자리다.
pub fn handle_get(id: serde_json::Value, params: &serde_json::Value) -> JsonRpcResponse {
    let Some(hid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    let hid = HookHandlerId::new(hid);
    let Some(h) = hook_handler::global().get(&hid) else {
        return JsonRpcResponse::invalid_params(id, format!("hook handler '{hid}' not found"));
    };
    let action = match serde_json::to_value(&h.action) {
        Ok(v) => v,
        Err(e) => {
            return JsonRpcResponse::internal_error(id, format!("action not serializable: {e}"));
        }
    };
    JsonRpcResponse::success(
        id,
        json!({
            "id": h.id.0,
            "source": h.source,
            "priority": h.priority,
            "owner": h.owner.prefix(),
            "action": action,
            "disabled": h.disabled,
            "display_name_i18n_key": h.display_name_i18n_key,
            "webhook_bindable": h.action.is_webhook_bindable(),
        }),
    )
}

/// `hook_handler.upsert` — user 출처 핸들러를 **제자리 수정**하거나 새로 만든다.
///
/// 지우고 다시 만드는 것과 다르다 — id·우선순위·나머지 필드가 그대로 남고, 준 필드만
/// 덮인다(patch semantics). 그래서 이미 그 id 를 참조하는 훅 바인딩은 계속 같은 것을
/// 가리킨다. 안 준 필드는 **없애는 것이 아니라 그대로 둔다.**
///
/// params (`id` 외 전부 선택, 단 최소 하나는 있어야 한다):
/// - `id`: `<owner>/<short-name>` 형식 (필수).
/// - `source`: `hook` | `webhook` | `any`.
/// - `priority`: 정수.
/// - `display_name_i18n_key`: 문자열.
/// - `disabled`: 불리언.
/// - `action`: `{"kind":"ipc_sequence","calls":[{"method":…,"params":…}]}` 또는
///   `{"kind":"shell_command","command":…,"args":[…]}` — `get` 응답과 같은 모양.
///
/// 성공 시 `~/.tasty/hook-handlers.toml` 에 즉시 atomic write 한다. 파일 쓰기가
/// 실패하면 **성공으로 보고하지 않는다** — 메모리 레지스트리는 이미 바뀌었고 그 사실을
/// 오류문에 적는다(다음 부팅에 사라지는 변경을 초록으로 덮지 않는다).
pub fn handle_upsert(id: serde_json::Value, params: &serde_json::Value) -> JsonRpcResponse {
    let decl = match parse_upsert(params) {
        Ok(d) => d,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    };
    let hid = HookHandlerId::new(decl.id.clone());
    let existed = hook_handler::global().get(&hid).is_some();
    if let Err(e) = hook_handler::global().upsert_user_handler(decl) {
        return JsonRpcResponse::invalid_params(id, e.to_string());
    }
    match persist_user_config() {
        Ok(path) => JsonRpcResponse::success(
            id,
            json!({
                "id": hid.0,
                "created": !existed,
                "path": path,
            }),
        ),
        Err(msg) => JsonRpcResponse::internal_error(id, msg),
    }
}

/// `hook_handler.remove` — user 출처 기여분만 제거한다. host/plugin 기본값은 보존된다.
///
/// `upsert` 로 만들 수 있는 것을 지울 길이 없으면 같은 결함을 새 표면에 다시 만드는
/// 셈이라 짝으로 낸다. host/plugin 이 같은 id 에 기본값을 심어 뒀다면 그 기본값이
/// 다시 드러나므로, 응답의 `still_present` 가 그 사실을 값으로 말한다 — "지웠는데
/// 아직 보인다" 를 버그로 오인하지 않게.
pub fn handle_remove(id: serde_json::Value, params: &serde_json::Value) -> JsonRpcResponse {
    let Some(hid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    let hid = HookHandlerId::new(hid);
    let existed = hook_handler::global().get(&hid).is_some();
    if !existed {
        // 기여분이 하나도 없으면 지울 것이 없다 — 파일을 다시 쓰지 않는다.
        // (user 기여분이 있으면 `get` 이 반드시 그것을 돌려주므로 이 갈래는
        // "어느 출처에도 그 id 가 없다" 와 같다.)
        return JsonRpcResponse::success(
            id,
            json!({ "id": hid.0, "existed": false, "still_present": false }),
        );
    }
    hook_handler::global().remove_user_handler(&hid);
    let still_present = hook_handler::global().get(&hid).is_some();
    match persist_user_config() {
        Ok(path) => JsonRpcResponse::success(
            id,
            json!({
                "id": hid.0,
                "existed": existed,
                "still_present": still_present,
                "path": path,
            }),
        ),
        Err(msg) => JsonRpcResponse::internal_error(id, msg),
    }
}

/// user 기여분을 `~/.tasty/hook-handlers.toml` 에 atomic write 하고 경로를 돌려준다.
fn persist_user_config() -> Result<String, String> {
    let Some(path) = hook_handler::user_config_path() else {
        return Err(
            "registry updated in memory, but tasty home could not be resolved \
             so hook-handlers.toml was not written (the change is lost on restart)"
                .to_string(),
        );
    };
    match hook_handler::global().save_user_config(&path) {
        Ok(()) => Ok(path.display().to_string()),
        Err(e) => Err(format!(
            "registry updated in memory, but writing {} failed: {e} \
             (the change is lost on restart)",
            path.display()
        )),
    }
}

/// upsert params → [`UserHookHandlerUpsertDecl`]. 빠진 필드는 "패치 안 함" 이고,
/// **형식이 틀린 필드는 조용히 빠뜨리지 않는다** — 빠뜨리면 아무것도 안 고친 upsert 가
/// 성공으로 보고된다. CLI 는 미지정 필드를 JSON null 로 보내므로 null 을 부재로 읽는다.
fn parse_upsert(params: &serde_json::Value) -> Result<UserHookHandlerUpsertDecl, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required 'id' parameter")?;
    if !id.contains('/') {
        return Err(format!(
            "hook handler id must be '<owner>/<short-name>' (e.g. 'user/my-handler'), got '{id}'"
        ));
    }
    let present = |k: &str| params.get(k).filter(|v| !v.is_null());

    let source = match present("source") {
        None => None,
        Some(v) => Some(
            serde_json::from_value::<HookSource>(v.clone())
                .map_err(|e| format!("'source' must be one of hook|webhook|any ({e})"))?,
        ),
    };
    let priority = match present("priority") {
        None => None,
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| format!("'priority' must be an integer, got {v}"))?;
            Some(
                i32::try_from(n)
                    .map_err(|_| format!("'priority' is out of range for a 32-bit integer: {n}"))?,
            )
        }
    };
    let display_name_i18n_key = match present("display_name_i18n_key") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .ok_or("'display_name_i18n_key' must be a string")?
                .to_string(),
        ),
    };
    let disabled = match present("disabled") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or("'disabled' must be a boolean")?),
    };
    let action = match present("action") {
        None => None,
        Some(v) => Some(
            serde_json::from_value::<UserHookHandlerActionDecl>(v.clone()).map_err(|e| {
                format!(
                    "'action' must be {{\"kind\":\"ipc_sequence\",\"calls\":[…]}} or \
                     {{\"kind\":\"shell_command\",\"command\":\"…\"}} ({e})"
                )
            })?,
        ),
    };

    if source.is_none()
        && priority.is_none()
        && display_name_i18n_key.is_none()
        && disabled.is_none()
        && action.is_none()
    {
        return Err(
            "nothing to patch — give at least one of 'source', 'priority', \
             'display_name_i18n_key', 'disabled', 'action'"
                .to_string(),
        );
    }

    Ok(UserHookHandlerUpsertDecl {
        id: id.to_string(),
        source,
        priority,
        display_name_i18n_key,
        disabled,
        action,
    })
}

/// dispatch 치환 컨텍스트 조립. body 는 JSON 값 그대로, headers/query 는 문자열 맵.
fn build_context(params: &serde_json::Value) -> SubstitutionContext {
    let body = params
        .get("body")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let headers = string_map(params.get("headers"), true);
    let query = string_map(params.get("query"), false);
    SubstitutionContext {
        body,
        headers,
        query,
    }
}

/// JSON object → `BTreeMap<String,String>`. `lowercase_keys` 면 헤더처럼 키를 소문자
/// 정규화한다(exec 의 헤더 조회 규약과 일치). 값이 문자열이 아니면 JSON 표현으로.
fn string_map(
    v: Option<&serde_json::Value>,
    lowercase_keys: bool,
) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    if let Some(serde_json::Value::Object(obj)) = v {
        for (k, val) in obj {
            let key = if lowercase_keys {
                k.to_ascii_lowercase()
            } else {
                k.clone()
            };
            let value = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            map.insert(key, value);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 수동 발화 시퀀스는 surface 훅과 같은 실행기에 줄 선다 — 잇달아 발화한 두 수동 시퀀스와 그
    /// 사이에 발화한 surface 훅 시퀀스가 스텝 단위로 끼어들지 않고 넘겨받은 순서대로 하나씩 돈다.
    /// 수동 발화마다 스레드를 띄우면(ADR-0498 대안 A) 뒤 시퀀스의 첫 스텝이 앞 시퀀스의 첫 스텝
    /// 답을 기다리지 않고 들어온다.
    #[test]
    fn dispatched_sequences_do_not_interleave_with_each_other_or_with_surface_hooks() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;
        use tasty_ipc::server::IpcCommand;

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, Arc::new(|| {}));
        let step = |method: &str| IpcCall {
            method: method.into(),
            params: json!({}),
        };
        start_dispatched_sequence(
            "user/first",
            &injector,
            &[step("first.a"), step("first.b")],
            SubstitutionContext::default(),
        )
        .expect("the first dispatch is queued");
        enqueue_sequence(
            SequenceOrigin::SurfaceHook,
            "user/hook",
            &injector,
            &[step("hook.a"), step("hook.b")],
            SubstitutionContext::default(),
        )
        .expect("the surface hook sequence is queued");
        start_dispatched_sequence(
            "user/second",
            &injector,
            &[step("second.a")],
            SubstitutionContext::default(),
        )
        .expect("the second dispatch is queued");

        let mut arrived = Vec::new();
        for _ in 0..5 {
            let cmd = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("a step reaches the queue");
            arrived.push(cmd.request.method.clone());
            assert!(
                rx.recv_timeout(Duration::from_millis(200)).is_err(),
                "a step reached the queue while {:?} was unanswered (arrived so far: {arrived:?})",
                cmd.request.method
            );
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            cmd.response_tx
                .send(JsonRpcResponse::success(id, json!({})))
                .expect("answer the step");
        }
        assert_eq!(
            arrived,
            ["first.a", "first.b", "hook.a", "hook.b", "second.a"]
        );
    }

    #[test]
    fn string_map_lowercases_header_keys() {
        let m = string_map(Some(&json!({"X-Sig": "abc", "Y": 1})), true);
        assert_eq!(m.get("x-sig"), Some(&"abc".to_string()));
        assert_eq!(m.get("y"), Some(&"1".to_string()));
    }

    #[test]
    fn string_map_preserves_query_keys() {
        let m = string_map(Some(&json!({"Token": "t"})), false);
        assert_eq!(m.get("Token"), Some(&"t".to_string()));
    }

    #[test]
    fn parse_upsert_requires_owner_prefixed_id() {
        let e = parse_upsert(&json!({"id": "noslash", "priority": 1})).unwrap_err();
        assert!(e.contains("<owner>/<short-name>"), "{e}");
    }

    #[test]
    fn parse_upsert_rejects_a_patch_with_no_fields() {
        // 아무것도 안 고치는 upsert 가 성공으로 보고되면 "고쳤는데 그대로" 가 된다.
        let e = parse_upsert(&json!({"id": "user/x"})).unwrap_err();
        assert!(e.contains("nothing to patch"), "{e}");
    }

    #[test]
    fn parse_upsert_treats_cli_nulls_as_absent() {
        let d = parse_upsert(&json!({
            "id": "user/x", "priority": 7,
            "source": null, "display_name_i18n_key": null, "disabled": null, "action": null,
        }))
        .expect("priority alone is a patch");
        assert_eq!(d.priority, Some(7));
        assert!(d.source.is_none() && d.action.is_none());
    }

    #[test]
    fn parse_upsert_refuses_a_malformed_action_instead_of_dropping_it() {
        // 조용히 떨어뜨리면 "시퀀스를 바꿨다" 는 응답이 아무것도 안 바꾼 채 나간다.
        let e = parse_upsert(&json!({"id": "user/x", "action": {"kind": "nope"}})).unwrap_err();
        assert!(e.contains("ipc_sequence"), "{e}");
    }

    #[test]
    fn parse_upsert_reads_a_sequence_action() {
        let d = parse_upsert(&json!({
            "id": "user/x",
            "action": {"kind": "ipc_sequence", "calls": [{"method": "window.focus", "params": {}}]},
        }))
        .expect("well-formed sequence");
        match d.action.expect("action present") {
            UserHookHandlerActionDecl::IpcSequence { calls } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].method, "window.focus");
            }
            other => panic!("expected ipc_sequence, got {other:?}"),
        }
    }

    #[test]
    fn get_response_action_round_trips_into_upsert() {
        // `get` 이 내는 action 모양을 `upsert` 가 그대로 먹는가 — 읽고 고쳐서 되돌려
        // 보내는 것이 이 기능의 전부라, 두 모양이 어긋나면 기능이 성립하지 않는다.
        let action = HookHandlerAction::IpcSequence {
            calls: vec![crate::hook_handler::IpcCall {
                method: "notification.send".into(),
                params: json!({"message": "${body.text}"}),
            }],
        };
        let as_json = serde_json::to_value(&action).expect("serializable");
        let d = parse_upsert(&json!({"id": "user/x", "action": as_json}))
            .expect("get output is valid upsert input");
        assert!(matches!(
            d.action,
            Some(UserHookHandlerActionDecl::IpcSequence { .. })
        ));
    }

    #[test]
    fn build_context_defaults_null_body() {
        let ctx = build_context(&json!({}));
        assert_eq!(ctx.body, serde_json::Value::Null);
        assert!(ctx.headers.is_empty());
    }
}
