//! mirror 문서 원문을 원격에 요청한다. request_id를 즉시 반환하고
//! 결과는 markdown_mirror.content_result 이벤트로 해당 플러그인에게만 보낸다.
//! markdown prefix는 플러그인이 소유하므로 호스트가 받을 별도 namespace를 쓴다.

use serde_json::json;

use tasty_ipc::protocol::JsonRpcResponse;

use super::params::require_u32;
use crate::core::PendingMarkdownContentForward;

/// 로컬 mirror surface ID로 원문을 요청한다. 원격 ID 변환은 attach 세션 매핑을 사용한다.
/// 대상 세션이 없으면 App의 요청 처리 단계에서 플러그인에 ok:false 결과를 보낸다.
pub fn handle_content_request(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
    caller: &tasty_ipc::caller::CallerContext,
) -> JsonRpcResponse {
    let surface_id = match require_u32(params, "surface_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // 사용자 대기는 그 원문 mirror surface를 소유한 plugin의 요청에만 건다.
    let owner_request = match caller {
        tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } => engine
            .runtime
            .surfaces
            .get(&surface_id)
            .and_then(|surface| {
                surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            })
            .is_some_and(|remote| {
                remote.plugin_id == *plugin_id && remote.capabilities.mirror_content
            }),
        _ => false,
    };
    // 에이전트 요청의 잘림 알림은 사용자에게 띄우지 않는다.
    let agent_origin = params
        .get("agent_origin")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let request_id = crate::core::next_markdown_content_request_id();
    engine
        .remote
        .pending_markdown_content_forward
        .push(PendingMarkdownContentForward {
            user_wait: owner_request
                && !agent_origin
                && params
                    .get("user_initiated")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
            admitted: std::time::Instant::now(),
            local_surface_id: surface_id,
            request_id,
            agent_origin,
        });
    JsonRpcResponse::success(id, json!({ "request_id": request_id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::surface_registry::KindCapabilities;
    use tasty_ipc::caller::CallerContext;

    fn plugin(id: &str) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: id.into(),
            permissions: Default::default(),
        }
    }

    /// 로컬 mirror 문서 surface `id`를 `owner` plugin 소유로 둔다.
    fn document(
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        id: u32,
        owner: &str,
        mirror_content: bool,
    ) {
        let surface = crate::plugin_bridge::remote_surface::RemoteSurface::new(
            id,
            "notes",
            owner.into(),
            "notes".into(),
        )
        .with_capabilities(KindCapabilities {
            mirror_content,
            ..KindCapabilities::default()
        });
        engine.runtime.surfaces.insert(id, Box::new(surface));
    }

    #[test]
    fn content_request_queues_and_returns_nonzero_request_id() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_content_request(
            &mut engine,
            json!(1),
            &json!({ "surface_id": 7 }),
            &plugin("com.example.notes"),
        );
        let rid = resp
            .result
            .as_ref()
            .and_then(|r| r.get("request_id"))
            .and_then(|v| v.as_u64())
            .expect("request_id");
        // 0은 요청 취소 신호로 예약된 값이다.
        assert_ne!(rid, 0);
        assert_eq!(engine.remote.pending_markdown_content_forward.len(), 1);
        assert_eq!(
            engine.remote.pending_markdown_content_forward[0].local_surface_id,
            7
        );
        assert_eq!(
            engine.remote.pending_markdown_content_forward[0].request_id,
            rid
        );
        assert!(
            !engine.remote.pending_markdown_content_forward[0].agent_origin,
            "요청 출처가 없으면 plugin 자체 요청으로 처리한다"
        );
    }

    #[test]
    fn content_request_carries_the_agent_origin() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_content_request(
            &mut engine,
            json!(1),
            &json!({ "surface_id": 7, "agent_origin": true }),
            &plugin("com.example.notes"),
        );
        assert!(resp.error.is_none());
        assert!(engine.remote.pending_markdown_content_forward[0].agent_origin);
    }

    #[test]
    fn automatic_and_agent_fetches_are_not_user_waits() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = session.borrow_mut();
        document(&mut engine, 7, "com.example.notes", true);
        for (params, expected) in [
            (json!({"surface_id": 7}), false),
            (json!({"surface_id": 7, "user_initiated": true}), true),
            (
                json!({"surface_id": 7, "user_initiated": true, "agent_origin": true}),
                false,
            ),
        ] {
            assert!(
                handle_content_request(
                    &mut engine,
                    json!(1),
                    &params,
                    &plugin("com.example.notes")
                )
                .error
                .is_none()
            );
            assert_eq!(
                engine
                    .remote
                    .pending_markdown_content_forward
                    .pop()
                    .unwrap()
                    .user_wait,
                expected
            );
        }
    }

    #[test]
    fn malformed_params_return_invalid_params_without_panicking_or_enqueueing() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = session.borrow_mut();
        for params in [
            json!(null),
            json!("not an object"),
            json!([]),
            json!([7]),
            json!({"surface_id": "bad"}),
        ] {
            for caller in [CallerContext::local(), plugin("com.example.notes")] {
                let response = handle_content_request(&mut engine, json!(1), &params, &caller);
                assert_eq!(response.error.unwrap().code, -32602);
                assert!(engine.remote.pending_markdown_content_forward.is_empty());
            }
        }
        let response = handle_content_request(
            &mut engine,
            json!(1),
            &json!({"surface_id": 7, "user_initiated": true}),
            &CallerContext::local(),
        );
        assert!(response.error.is_none());
        assert!(!engine.remote.pending_markdown_content_forward[0].user_wait);
        assert!(
            !engine.remote.pending_markdown_content_forward[0].agent_origin,
            "wait provenance must not rewrite the existing truncation-notice flag"
        );
    }

    #[test]
    fn content_request_without_surface_id_is_invalid_params() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = engine_session.borrow_mut();
        let resp =
            handle_content_request(&mut engine, json!(1), &json!({}), &CallerContext::local());
        assert!(resp.error.is_some());
        assert!(engine.remote.pending_markdown_content_forward.is_empty());
    }

    /// 사용자 대기는 요청한 plugin이 원문 mirror를 선언한 그 surface의 소유자일 때만 건다.
    #[test]
    fn only_the_owner_of_a_content_document_starts_a_user_wait() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = session.borrow_mut();
        document(&mut engine, 7, "com.example.notes", true);
        document(&mut engine, 8, "com.example.notes", false);
        let user = json!({"surface_id": 7, "user_initiated": true});
        let undeclared = json!({"surface_id": 8, "user_initiated": true});
        let absent = json!({"surface_id": 9, "user_initiated": true});
        for (params, caller, expected) in [
            (&user, plugin("com.example.notes"), true),
            (&user, plugin("com.example.other"), false),
            (&user, CallerContext::local(), false),
            (&undeclared, plugin("com.example.notes"), false),
            (&absent, plugin("com.example.notes"), false),
        ] {
            assert!(
                handle_content_request(&mut engine, json!(1), params, &caller)
                    .error
                    .is_none()
            );
            let queued = engine.remote.pending_markdown_content_forward.pop();
            assert_eq!(queued.unwrap().user_wait, expected, "{params} {caller:?}");
        }
    }
}
