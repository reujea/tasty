//! 파일 핸들러 설정 재로드, detector 조회, 파일 열기 요청.
//! reload는 사용자 설정만 바꾸고 적용하지 못한 항목과 사유를 반환한다.
//! dispatch는 GUI 전용이다. 헤드리스에는 결과를 적용할 창과 worker가 없어 -32017로 거절한다.

#[cfg(feature = "gui")]
use std::path::PathBuf;

#[cfg(feature = "gui")]
use serde::Deserialize;
use serde_json::json;

use crate::app::services::AppServices;
#[cfg(feature = "gui")]
use crate::file::format::{DetectDepth, FileTarget};
use tasty_ipc::protocol::JsonRpcResponse;

pub fn handle_reload(core: &AppServices, id: serde_json::Value) -> JsonRpcResponse {
    let outcome = match core.reload_file_handlers() {
        Ok(outcome) => outcome,
        Err(error) => return JsonRpcResponse::internal_error(id, error),
    };
    JsonRpcResponse::success(
        id,
        json!({
            "path": outcome.path.display().to_string(),
            "exists": outcome.exists,
            "rejected": rejected_json(&outcome.rejected),
        }),
    )
}

/// detector를 ID 순으로 반환한다. 병합 결과와 출처별 원본을 함께 보여준다.
/// contributions는 설치 순서이며 병합 우선순위가 아니다. rules는 사용자 설정과 같은 키를 쓴다.
pub fn handle_detectors(core: &AppServices, id: serde_json::Value) -> JsonRpcResponse {
    let detectors: Vec<serde_json::Value> = core
        .registries
        .file_format
        .detector_snapshots()
        .iter()
        .map(detector_json)
        .collect();
    JsonRpcResponse::success(id, json!({ "detectors": detectors }))
}

fn detector_json(s: &crate::file::format::DetectorSnapshot) -> serde_json::Value {
    let det = &s.detector;
    let rules: Vec<serde_json::Value> = det
        .rules
        .iter()
        .map(|r| {
            let mut v = rule_json(&r.kind);
            if let Some(obj) = v.as_object_mut() {
                obj.insert("origin".into(), json!(r.origin.label()));
            }
            v
        })
        .collect();
    let contributions: Vec<serde_json::Value> = s
        .contributions
        .iter()
        .map(|c| {
            json!({
                "origin": c.origin.label(),
                "display_name_i18n_key": c.display_name_i18n_key,
                "icon": c.icon,
                "disabled": c.disabled,
                "rules": c.rules.iter().map(rule_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "id": det.id.0,
        "display_name_i18n_key": det.display_name_i18n_key,
        "icon": det.icon,
        "disabled": det.disabled,
        "install_order": det.install_order,
        "rules": rules,
        "contributions": contributions,
    })
}

fn rule_json(kind: &crate::file::format::DetectorRuleKind) -> serde_json::Value {
    match serde_json::to_value(kind.to_config_table()) {
        Ok(v) => v,
        Err(e) => {
            // 규칙 하나의 직렬화 실패로 목록 전체를 잃지 않도록 해당 항목만 오류로 표시한다.
            tracing::warn!(error = %e, "file_handler.detectors: rule 을 JSON 으로 옮기지 못했다");
            json!({ "error": e.to_string() })
        }
    }
}

fn rejected_json(rejected: &[tasty_file_handler::RejectedUserHandler]) -> serde_json::Value {
    rejected
        .iter()
        .map(|r| json!({ "id": r.id, "reason": r.reason.as_str() }))
        .collect()
}

#[cfg(feature = "gui")]
#[derive(Deserialize)]
struct DispatchReq {
    path: String,
    #[serde(default = "default_depth")]
    depth: String,
    /// 결과를 이 surface의 pane에 새 탭으로 연다. 생략하면 포커스된 pane을 쓴다.
    #[serde(default)]
    origin_surface_id: Option<u32>,
    /// 대용량 markdown 확인을 건너뛴다. 기본 false이며 1 MiB 초과 시 확인한다.
    #[serde(default)]
    ignore_size_limit: bool,
    /// 호출 플러그인이 소유하고 사용자의 확정 입력을 받은 팝업 인스턴스.
    /// 사용자 요청 판정에 쓰며 외부 IPC·다른 소유자·닫힌 팝업·입력 없는 팝업은 인정하지 않는다.
    #[serde(default)]
    owner_popup_instance: Option<u64>,
    /// origin_surface_id의 webview에서 통지받은 navigation URL.
    /// 엔진이 사용자 제스처로 보고하고 호출 플러그인이 surface와 페이지를 소유한 경우만 인정한다.
    #[serde(default)]
    user_navigation_url: Option<String>,
}

#[cfg(feature = "gui")]
fn default_depth() -> String {
    "deep".to_string()
}

/// 파일 식별을 worker에 맡기고 즉시 응답한다. 결과는 IdentifyDone으로 적용한다.
/// 응답의 `run`으로 `run.result`가 최종 결과(열었는지, 왜 못 열었는지)를 돌려준다.
/// depth는 확장자/glob만 보는 cheap 또는 magic/MIME도 보는 deep이며 기본값은 deep이다.
/// origin_surface_id가 mirror surface이면 경로는 원격 파일이라 depth와 관계없이 이름 기반 식별만 한다.
/// 응답의 depth는 요청값을 그대로 돌려주며 실제 식별 방식을 나타내지 않는다.
/// 상대 경로는 실행 환경에 따라 달라질 수 있어 절대 경로를 권장한다.
/// 사용자 요청 여부는 dispatch_origin_of가 호스트에 기록된 입력으로 판정한다.
#[cfg(feature = "gui")]
pub fn handle_dispatch(
    out: &mut crate::ipc::window_port::IntentOutbox,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    id: serde_json::Value,
    params: serde_json::Value,
) -> JsonRpcResponse {
    let req: DispatchReq = match serde_json::from_value(params) {
        Ok(r) => r,
        Err(e) => {
            return JsonRpcResponse::error(id, -32602, format!("invalid params: {e}"));
        }
    };
    if let Some(sid) = req.origin_surface_id
        && let Err(message) = crate::file::dispatch::require_origin_pane(engine, sid)
    {
        return JsonRpcResponse::invalid_params(id, message);
    }
    let depth = match req.depth.as_str() {
        "cheap" => DetectDepth::Cheap,
        "deep" => DetectDepth::Deep,
        other => {
            return JsonRpcResponse::error(
                id,
                -32602,
                format!("invalid depth '{other}': expected 'cheap' or 'deep'"),
            );
        }
    };
    // FsRead 권한은 URL 열기를 허용하지 않는다. 확장자로 파일처럼 식별되기 전에 거절한다.
    if crate::file::format::looks_like_url(&req.path) {
        return JsonRpcResponse::error(
            id,
            -32602,
            format!(
                "invalid path '{}': file_handler.dispatch accepts file paths, not URLs",
                req.path
            ),
        );
    }
    let target = FileTarget::new(PathBuf::from(&req.path));
    let dispatch_origin = dispatch_origin_of(
        window,
        engine,
        caller,
        req.owner_popup_instance,
        req.origin_surface_id,
        req.user_navigation_url.as_deref(),
    );
    let run = crate::request_runs::begin(
        "file_handler.dispatch",
        crate::request_runs::reader_of(caller),
        json!({ "path": req.path }),
    );
    let intent = crate::app::command::DomainIntent::DispatchFile {
        target,
        depth,
        origin_surface_id: req.origin_surface_id,
        dispatch_origin,
        ignore_size_limit: req.ignore_size_limit,
        run: Some(run),
        // IPC 요청은 결과 자리를 정하지 않는다. 출처 surface 가 있으면 그 pane 이다.
        result_place: crate::file::dispatch::FileResultPlace::Unbound,
    };
    // 새 탭 선택과 실패 알림도 같은 요청 출처를 사용한다.
    out.push(match dispatch_origin {
        crate::file::dispatch::FileDispatchOrigin::User => intent.from_user_menu("plugin_popup"),
        crate::file::dispatch::FileDispatchOrigin::Agent
        | crate::file::dispatch::FileDispatchOrigin::PluginUnverified => intent.from_agent_ipc(),
    });
    JsonRpcResponse::success(
        id,
        json!({
            "accepted": true,
            "depth": req.depth,
            "ignore_size_limit": req.ignore_size_limit,
            "run": run,
        }),
    )
}

/// 플러그인 호출도 실제 사용자 조작에서 시작됐는지 확인한다. 요청 필드만으로는 인정하지 않는다.
/// 팝업은 호출 플러그인 소유로 열려 있고 확정 입력을 받은 상태여야 한다.
/// webview는 소유 플러그인이 쓴 페이지의 사용자 제스처를 host가 같은 플러그인에 통지했어야 한다.
/// 통지된 URL과 요청 URL이 같아야 하며 이 기록은 한 번만 쓸 수 있다.
/// 외부 IPC 호출은 에이전트 요청이다. 플러그인 호출이 어느 조건에도 맞지 않으면 PluginUnverified로 처리한다.
/// markdown.navigate도 팝업 확정 요청을 같은 규칙으로 판정한다.
#[cfg(feature = "gui")]
pub(super) fn dispatch_origin_of(
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    owner_popup_instance: Option<u64>,
    origin_surface_id: Option<u32>,
    user_navigation_url: Option<&str>,
) -> crate::file::dispatch::FileDispatchOrigin {
    use crate::file::dispatch::FileDispatchOrigin;
    let tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } = caller else {
        return FileDispatchOrigin::Agent;
    };
    if let Some(instance_id) = owner_popup_instance {
        if window.plugin_popup_user_activated(plugin_id, instance_id) {
            return FileDispatchOrigin::User;
        }
        tracing::debug!(
            plugin_id = %plugin_id,
            instance_id,
            "owner_popup_instance is not a user-activated popup of the caller; treated as an unverified plugin request",
        );
    }
    if let Some(url) = user_navigation_url {
        if let Some(surface_id) = origin_surface_id
            && window.take_webview_user_navigation(engine, plugin_id, surface_id, url)
        {
            return FileDispatchOrigin::User;
        }
        tracing::debug!(
            plugin_id = %plugin_id,
            ?origin_surface_id,
            "file_handler.dispatch: user_navigation_url is not an unused user-gesture navigation the caller received on origin_surface_id over a page the caller wrote; treated as an unverified plugin request",
        );
    }
    FileDispatchOrigin::PluginUnverified
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

    #[test]
    fn ignore_size_limit_defaults_false() {
        let req: DispatchReq =
            serde_json::from_value(serde_json::json!({ "path": "/a.md" })).unwrap();
        assert!(!req.ignore_size_limit);
    }

    #[test]
    fn ignore_size_limit_parsed_true() {
        let req: DispatchReq = serde_json::from_value(
            serde_json::json!({ "path": "/a.md", "ignore_size_limit": true }),
        )
        .unwrap();
        assert!(req.ignore_size_limit);
    }

    #[test]
    fn dispatch_rejects_a_url_in_the_path_param() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let mut scope =
            crate::ipc::request_scope::RequestScope::capture(&mut state, engine.core, None);
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &tasty_ipc::caller::CallerContext::local(),
            serde_json::json!(1),
            serde_json::json!({ "path": "https://example.com/a.md" }),
        );
        let err = resp.error.expect("URL path must be rejected");
        assert_eq!(err.code, -32602);
        assert!(out.is_empty());

        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &tasty_ipc::caller::CallerContext::local(),
            serde_json::json!(2),
            serde_json::json!({ "path": "/tmp/a.md" }),
        );
        assert!(resp.error.is_none());
        assert_eq!(out.into_vec().len(), 1);
    }
    #[test]
    fn dispatch_rejects_missing_origin_before_enqueueing() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let mut scope =
            crate::ipc::request_scope::RequestScope::capture(&mut state, engine.core, None);
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let response = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &tasty_ipc::caller::CallerContext::local(),
            serde_json::json!(42),
            serde_json::json!({"path":"/a", "origin_surface_id":u32::MAX}),
        );
        assert_eq!(response.error.unwrap().code, -32602);
        assert!(out.is_empty());
    }
}

#[cfg(all(test, feature = "gui"))]
#[path = "file_handler_origin_tests.rs"]
mod origin_tests;
