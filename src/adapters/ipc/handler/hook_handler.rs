//! 공유 훅 핸들러의 조회·수정·재로드·수동 실행.
//! 메서드 이름은 등록한 정의에 고정하고, 요청 payload는 params 값에만 치환한다.
//! 실행 결과는 응답에 포함하지 않으며 접수 여부만 반환한다.
//!
//! 시퀀스는 Local 권한으로 실행되므로 get/upsert는 local_only로 제한한다.
//! 플러그인이 임의 시퀀스를 등록해 자신에게 닫힌 Local 전용 메서드를 실행하지 못하게 하기 위해서다.
//!
//! 등록된 웹훅은 calls 사본을 보관하므로 upsert로 바뀌지 않는다.
//! 변경은 이후의 바인딩과 hook_handler.dispatch에 적용된다(docs/features/webhook/index.md).

use serde_json::json;

use crate::app::services::AppServices;
use crate::hook_handler::{
    self, HookHandlerAction, HookHandlerId, HookShellEnv, HookSource, IpcCall, SequenceOrigin,
    SubstitutionContext, UserHookHandlerActionDecl, UserHookHandlerUpsertDecl, build_env,
    spawn_shell,
};
use crate::hook_runtime::{Delivery, SequenceNotQueued, enqueue_sequence, runs};
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;

/// 비활성 항목도 모두 조회한다. 실제 호출 목록과 셸 명령은 공개하지 않고 종류와 단계 수만 요약한다.
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

/// 사용자 설정만 다시 읽는다. host/plugin 기본값은 유지하며 적용 거절 항목의 별도 보고는 없다.
pub fn handle_reload(_core: &AppServices, id: serde_json::Value) -> JsonRpcResponse {
    let hook = hook_handler::global();
    crate::app::file_consumers::report("hook-handlers.toml", hook.poll_file());
    match hook.reload_file() {
        Ok(path) => JsonRpcResponse::success(
            id,
            json!({ "path": path.display().to_string(), "exists": path.exists() }),
        ),
        Err(error) => JsonRpcResponse::internal_error(id, error),
    }
}

/// 등록된 핸들러를 ID로 실행한다. body/headers/query는 params의 값에만 치환한다.
/// 응답은 접수(`accepted`)와 최종 결과를 조회할 `run`이다. 결과는 [`handle_result`]가 돌려준다.
/// 대기 한도 초과나 실행기 부재로 넘기지 못하면 오류로 답한다. 비활성 핸들러는 거절한다.
pub fn handle_dispatch(
    core: &AppServices,
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
            // 이 스레드가 호스트 명령 큐를 처리하므로 단계별 응답 대기는 실행기에 맡긴다.
            let Some(injector) = core.host_ipc_injector_arc().get().cloned() else {
                return JsonRpcResponse::internal_error(
                    id,
                    "host IPC injector not initialized (dispatch unavailable before boot completes)",
                );
            };
            let ctx = build_context(params);
            let steps = calls.len();
            let run = runs::begin(&hid.0, runs::ACTION_SEQUENCE);
            if let Err(e) =
                start_dispatched_sequence(&hid.0, &injector, &calls, ctx, Delivery::Run(run))
            {
                runs::forget(run);
                return JsonRpcResponse::internal_error(
                    id,
                    format!("hook handler '{hid}' not run — {e}"),
                );
            }
            JsonRpcResponse::success(
                id,
                json!({ "accepted": true, "id": hid.0, "action": "ipc_sequence", "steps": steps, "run": run }),
            )
        }
        HookHandlerAction::ShellCommand { command, args } => {
            let env = build_env(&HookShellEnv {
                event: hid.0.clone(),
                source: "dispatch",
                surface_id: None,
                payload: params
                    .get("body")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            });
            let run = runs::begin(&hid.0, runs::ACTION_SHELL);
            spawn_shell(command, args, env, move |event| {
                runs::shell_event(run, event)
            });
            JsonRpcResponse::success(
                id,
                json!({ "accepted": true, "id": hid.0, "action": "shell_command", "run": run }),
            )
        }
    }
}

/// 수동 dispatch의 진행·최종 결과. `done`이 false면 아직 실행 중이다.
/// 보관 수를 넘어 지워졌거나 발급하지 않은 run은 invalid params다.
pub fn handle_result(id: serde_json::Value, params: &serde_json::Value) -> JsonRpcResponse {
    let run = match super::params::opt_int::<u64>(params, "run", &id) {
        Ok(Some(run)) => run,
        Ok(None) => {
            return JsonRpcResponse::invalid_params(id, "Missing required 'run' parameter");
        }
        Err(response) => return response,
    };
    match runs::lookup(run) {
        Some(result) => JsonRpcResponse::success(id, result),
        None => JsonRpcResponse::invalid_params(
            id,
            format!(
                "unknown run {run} — not issued, or older than the last {} runs",
                runs::KEEP
            ),
        ),
    }
}

/// surface 훅과 같은 실행기에 넣어 시퀀스의 각 단계가 서로 끼어들지 않도록 한다(docs/features/hooks/index.md).
fn start_dispatched_sequence(
    handler_id: &str,
    injector: &HostIpcInjector,
    calls: &[IpcCall],
    ctx: SubstitutionContext,
    delivery: Delivery,
) -> Result<(), SequenceNotQueued> {
    let origin = SequenceOrigin::Dispatch;
    enqueue_sequence(origin, handler_id, injector, calls, ctx, delivery).inspect_err(|e| {
        tracing::error!("{origin} IpcSequence '{handler_id}' not run — {e}");
    })
}

/// 병합된 핸들러를 action 본문까지 반환한다. action 형식은 upsert 입력과 같다.
/// 사용자 설정만 보려면 hook-handlers.toml을 읽는다.
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

/// 지정한 필드만 수정하며 생략한 필드는 유지한다. 같은 ID의 바인딩도 유지된다.
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
/// 파일 쓰기 실패는 오류로 보고하되 메모리 레지스트리는 이미 바뀌었음을 알린다.
pub fn handle_upsert(
    core: &AppServices,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let decl = match parse_upsert(params) {
        Ok(d) => d,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    };
    let hid = HookHandlerId::new(decl.id.clone());
    let existed = hook_handler::global().get(&hid).is_some();
    if let Err(e) = hook_handler::global().upsert_user_handler(decl) {
        return JsonRpcResponse::invalid_params(id, e.to_string());
    }
    persist_user_config(core, id, json!({ "id": hid.0, "created": !existed }))
}

/// 사용자 설정만 제거한다. host/plugin 기본값이 남으면 still_present로 알린다.
pub fn handle_remove(
    core: &AppServices,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(hid) = params.get("id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' parameter");
    };
    let hid = HookHandlerId::new(hid);
    let existed = hook_handler::global().get(&hid).is_some();
    if !existed {
        return JsonRpcResponse::success(
            id,
            json!({ "id": hid.0, "existed": false, "still_present": false }),
        );
    }
    hook_handler::global().remove_user_handler(&hid);
    let still_present = hook_handler::global().get(&hid).is_some();
    persist_user_config(
        core,
        id,
        json!({ "id": hid.0, "existed": existed, "still_present": still_present }),
    )
}

/// Memory edits are visible immediately; the original caller receives the file result once.
fn persist_user_config(
    core: &AppServices,
    id: serde_json::Value,
    body: serde_json::Value,
) -> JsonRpcResponse {
    persist_registry(core, hook_handler::global(), id, body)
}
fn persist_registry(
    core: &AppServices,
    registry: &hook_handler::registry::HookHandlerRegistry,
    id: serde_json::Value,
    mut body: serde_json::Value,
) -> JsonRpcResponse {
    let submitted = registry.submit_file(core.file_snapshot_sink(), false);
    let (path, ticket) = match submitted {
        Ok(value) => value,
        Err(error) => return save_failed(id, error),
    };
    body["path"] = json!(path.display().to_string());
    core.reply_with_file(
        id.clone(),
        move || ticket.poll(),
        move |_, result| match result {
            Ok(()) => {
                body["applied"] = json!(true);
                body["saved"] = json!(true);
                JsonRpcResponse::success(id, body)
            }
            Err(error) => save_failed(id, error),
        },
    )
}
fn save_failed(id: serde_json::Value, error: String) -> JsonRpcResponse {
    JsonRpcResponse::error_with_data(
        id,
        -32603,
        format!("hook handler applied for this run but not saved: {error}"),
        json!({ "applied": true, "saved": false }),
    )
}

/// 생략한 필드는 유지하고 잘못된 타입은 거절한다. CLI가 보내는 null은 생략으로 처리한다.
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

/// 헤더 키는 소문자로 정규화한다. 문자열이 아닌 값은 JSON 표현을 쓴다.
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

    fn receive_hook_file_reply(
        registry: &hook_handler::registry::HookHandlerRegistry,
        core: &crate::app::services::AppServices,
        receiver: &std::sync::mpsc::Receiver<JsonRpcResponse>,
    ) -> JsonRpcResponse {
        use super::super::deferred::MemoryReplies;
        use std::time::{Duration, Instant};
        // 두 poll 은 이미 끝난 파일 영수증과 writer 결과만 거둬 응답을 보낸다(새 제출 없음). 그래서 기한 뒤
        // 관측 읽기에서도 거둔 뒤 받는다.
        let end = Instant::now() + Duration::from_secs(5);
        let mut answer = None;
        assert!(tasty_utils::test_wait::holds_by_every(
            end,
            Duration::ZERO,
            |_| {
                assert!(registry.poll_file().is_empty());
                assert!(MemoryReplies::poll(core).is_empty());
                answer = receiver.try_recv().ok();
                answer.is_some()
            }
        ));
        answer.expect("checked above")
    }

    #[test]
    fn held_hook_save_answers_original_caller_once_after_receipt_with_applied_saved() {
        use super::super::deferred::{MemoryReplies, ReplyTo};
        use std::{
            sync::{Arc, mpsc},
            time::{Duration, Instant},
        };
        use tasty_utils::{batch_writer, file_replace};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.toml");
        let registry = hook_handler::registry::HookHandlerRegistry::new();
        registry.install_user_config(&path);
        let writer = crate::app::file_writer::start(
            Arc::new(tasty_settings::FileSettingsStorage),
            batch_writer::Limits::default(),
            Arc::new(batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        let mut core = super::super::cli_entry_tests::test_core_builder()
            .with_file_writer(writer)
            .build()
            .unwrap();
        for fail in [false, true] {
            let lock = file_replace::lock(&path).unwrap();
            registry.upsert_user_handler(parse_upsert(&json!({"id":"user/test","source":"any","priority":if fail {2} else {1},"action":{"kind":"ipc_sequence","calls":[]}})).unwrap()).unwrap();
            assert_eq!(
                registry
                    .get(&HookHandlerId::new("user/test"))
                    .unwrap()
                    .priority,
                if fail { 2 } else { 1 }
            );
            let response = persist_registry(&core, &registry, json!(17), json!({"id":"user/test"}));
            assert!(
                response.error.is_some(),
                "placeholder stays inside deferred route"
            );
            let deferred = core
                .memory_replies()
                .take_parked()
                .expect("file save is pending");
            let (tx, rx) = mpsc::sync_channel(2);
            core.defer_reply(deferred, ReplyTo::Ipc(tx));
            assert!(MemoryReplies::poll(&core).is_empty());
            assert!(rx.try_recv().is_err());
            if fail {
                std::fs::write(&path, "# external\n").unwrap();
            }
            drop(lock);
            let answer = receive_hook_file_reply(&registry, &core, &rx);
            assert_eq!(answer.id, json!(17));
            if fail {
                assert_eq!(
                    answer.error.unwrap().data.unwrap(),
                    json!({"applied":true,"saved":false})
                );
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "# external\n");
            } else {
                let body = answer.result.unwrap();
                assert_eq!(body["applied"], true);
                assert_eq!(body["saved"], true);
            }
            MemoryReplies::poll(&core);
            assert!(rx.try_recv().is_err());
        }
        // holds_by 로 바꾸지 않는다. 종료 poll 은 child cleanup 제출과 webhook 작업 처리를 함께 하는 진행이라
        // 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는 끝났는지 알 수 없다.
        let end = Instant::now() + Duration::from_secs(5);
        while core.poll_file_writer_shutdown() != 0 {
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
    }

    #[test]
    fn result_reads_run_through_the_shared_number_parser() {
        let missing = handle_result(json!(1), &json!({})).error.unwrap();
        assert_eq!(missing.code, -32602);
        assert_eq!(missing.message, "Missing required 'run' parameter");
        for bad in [json!(-1), json!("3"), json!(1.5)] {
            let error = handle_result(json!(1), &json!({ "run": bad }))
                .error
                .unwrap();
            assert_eq!(error.code, -32602);
            assert!(
                error.message.starts_with("'run' was given as"),
                "{}",
                error.message
            );
        }
        let run = runs::begin("h", runs::ACTION_SEQUENCE);
        let found = handle_result(json!(1), &json!({ "run": run }))
            .result
            .unwrap();
        assert_eq!(found["state"], "running");
    }

    // 수동 실행과 surface 훅 사이에도 시퀀스의 단계가 섞이지 않아야 한다.
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
            Delivery::Run(0),
        )
        .expect("the first dispatch is queued");
        enqueue_sequence(
            SequenceOrigin::SurfaceHook,
            "user/hook",
            &injector,
            &[step("hook.a"), step("hook.b")],
            SubstitutionContext::default(),
            Delivery::Run(0),
        )
        .expect("the surface hook sequence is queued");
        start_dispatched_sequence(
            "user/second",
            &injector,
            &[step("second.a")],
            SubstitutionContext::default(),
            Delivery::Run(0),
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
