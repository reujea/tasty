//! `markdown_mirror.*` IPC — markdown plugin 이 attach mirror 문서의 원문을 원격에서
//! 가져오게 하는 진입점(`docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md`).
//!
//! `git_viewer.query`(ADR-0056) 와 같은 **비동기 accept** 다 — 원문은 attach Control 채널
//! 왕복(`markdown_content_request`/`markdown_content_result`)을 거쳐야 하므로, 이 핸들러는
//! 요청을 `CoreState::pending_markdown_content_forward` 에 큐잉하고 `request_id` 만 즉시
//! 회신한다. 실제 원문은 attach 응답 도착 후 `markdown_mirror.content_result` 이벤트로
//! markdown plugin 에 unicast 된다(`src/app/attach_client.rs`).
//!
//! namespace 가 `markdown.` 이 아닌 이유: `markdown` prefix 는 번들 plugin 이 점유하고
//! 있어 그 이름의 외부 호출은 plugin 으로 forward 된다(ADR-0153). 이 메서드는 plugin 이
//! host 에 거는 서비스라 host 가 곧바로 받아야 한다.

use serde_json::json;

use tasty_ipc::protocol::JsonRpcResponse;

use super::params::require_u32;
use crate::core::{CoreState, PendingMarkdownContentForward};

/// `markdown_mirror.content_request { surface_id }` — 원격 원문 조회를 큐잉하고
/// `request_id` 만 즉시 회신한다. `surface_id` 는 **로컬** mirror markdown surface 다(host 가
/// attach 세션 매핑으로 원격 id 로 치환한다).
///
/// 대상이 mirror surface 인지는 여기서 판정하지 않는다 — 세션 조회는 App 레이어의 drain
/// 이 하고, 못 찾으면 그 자리에서 `ok:false` 결과를 plugin 에 돌려준다(무한 로딩 없음).
pub fn handle_content_request(
    engine: &mut CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_u32(params, "surface_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // 에이전트가 건 요청(plugin 이 `markdown.reload` 를 받아 건 것)은 그 회신의 잘림 toast 를
    // 사용자에게 띄우지 않는다(ADR-0503). 칸이 없으면 종전대로 plugin 자신의 요청이다.
    let agent_origin = params
        .get("agent_origin")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let request_id = crate::core::next_markdown_content_request_id();
    engine
        .pending_markdown_content_forward
        .push(PendingMarkdownContentForward {
            local_surface_id: surface_id,
            request_id,
            agent_origin,
        });
    JsonRpcResponse::success(id, json!({ "request_id": request_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_request_queues_and_returns_nonzero_request_id() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = CoreState::new(80, 24, waker).unwrap();
        let resp = handle_content_request(&mut engine, json!(1), &json!({ "surface_id": 7 }));
        let rid = resp
            .result
            .as_ref()
            .and_then(|r| r.get("request_id"))
            .and_then(|v| v.as_u64())
            .expect("request_id");
        // 0 은 host 의 abandon sentinel 이라 발급되면 안 된다.
        assert_ne!(rid, 0);
        assert_eq!(engine.pending_markdown_content_forward.len(), 1);
        assert_eq!(
            engine.pending_markdown_content_forward[0].local_surface_id,
            7
        );
        assert_eq!(engine.pending_markdown_content_forward[0].request_id, rid);
        assert!(
            !engine.pending_markdown_content_forward[0].agent_origin,
            "칸이 없으면 plugin 자신의 요청이다"
        );
    }

    /// `agent_origin: true` 를 실은 요청은 큐 원소에 그대로 표시된다 — 회신의 잘림 toast 를
    /// 사용자에게 띄우지 않는 근거다(ADR-0503).
    #[test]
    fn content_request_carries_the_agent_origin() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = CoreState::new(80, 24, waker).unwrap();
        let resp = handle_content_request(
            &mut engine,
            json!(1),
            &json!({ "surface_id": 7, "agent_origin": true }),
        );
        assert!(resp.error.is_none());
        assert!(engine.pending_markdown_content_forward[0].agent_origin);
    }

    #[test]
    fn content_request_without_surface_id_is_invalid_params() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = CoreState::new(80, 24, waker).unwrap();
        let resp = handle_content_request(&mut engine, json!(1), &json!({}));
        assert!(resp.error.is_some());
        assert!(engine.pending_markdown_content_forward.is_empty());
    }
}
