//! mirror workspace의 원격 git 조회. request_id를 즉시 반환하고
//! attach 응답이 오면 git_viewer.query_result 이벤트를 요청한 플러그인에게만 보낸다(docs/features/remote-attach/index.md).

use serde::Deserialize;
use serde_json::json;

use tasty_ipc::protocol::JsonRpcResponse;

use crate::core::PendingGitQueryForward;
use tasty_ipc::stream_hub::GitQueryKind;

/// `git_viewer.query { kind, local_surface_id, worktree_path?, diff_path? }` 요청.
#[derive(Deserialize)]
struct GitViewerQueryReq {
    #[serde(default)]
    consumer: Option<u64>,
    /// `"snapshot"` | `"diff"`.
    kind: String,
    /// 팝업을 연 로컬 mirror surface ID. 호스트가 원격 ID로 바꿔 전달한다.
    local_surface_id: u32,
    /// 이전 응답에서 받은 서버 경로. 없으면 원격 surface의 cwd에서 저장소를 찾는다.
    #[serde(default)]
    worktree_path: Option<String>,
    /// `kind = "diff"` 전용 — 대상 파일의 repo-relative 경로.
    #[serde(default)]
    diff_path: Option<String>,
}

pub fn handle_query(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
    caller: &tasty_ipc::caller::CallerContext,
) -> JsonRpcResponse {
    // 결과는 요청한 plugin에게만 간다. plugin이 아닌 호출자는 결과 이벤트를 받을 수 없다.
    let requester = match caller {
        tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } => Some(plugin_id.clone()),
        _ => None,
    };
    let req: GitViewerQueryReq = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => return JsonRpcResponse::error(id, -32602, format!("invalid params: {e}")),
    };
    let kind = match req.kind.as_str() {
        "snapshot" => GitQueryKind::Snapshot,
        "diff" => GitQueryKind::Diff,
        other => {
            return JsonRpcResponse::error(
                id,
                -32602,
                format!("invalid params: unknown kind '{other}'"),
            );
        }
    };
    let request_id = crate::core::next_git_query_request_id();
    engine
        .remote
        .pending_git_query_forward
        .push(PendingGitQueryForward {
            admitted: std::time::Instant::now(),
            consumer: requester.as_ref().and(req.consumer),
            requester,
            local_surface_id: req.local_surface_id,
            request_id,
            kind,
            worktree_path: req.worktree_path,
            diff_path: req.diff_path,
        });
    JsonRpcResponse::success(id, json!({ "request_id": request_id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_ipc::caller::CallerContext;

    /// 결과를 받을 plugin과 대기 표시 popup은 호출자에서 정한다. 이름은 보지 않는다.
    #[test]
    fn the_caller_plugin_is_the_requester_of_a_git_query() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).unwrap();
        let mut engine = session.borrow_mut();
        let params = json!({"kind": "snapshot", "local_surface_id": 7, "consumer": 41});
        let plugin = CallerContext::Plugin {
            plugin_id: "com.example.git".into(),
            permissions: Default::default(),
        };
        for (caller, requester, consumer) in [
            (plugin, Some("com.example.git"), Some(41)),
            (CallerContext::local(), None, None),
        ] {
            assert!(
                handle_query(&mut engine, json!(1), &params, &caller)
                    .error
                    .is_none()
            );
            let queued = engine.remote.pending_git_query_forward.pop().unwrap();
            assert_eq!(queued.requester.as_deref(), requester);
            assert_eq!(queued.consumer, consumer);
        }
    }
}
