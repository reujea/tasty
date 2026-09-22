//! `file_handler.*` IPC 메서드 — reload + detectors + dispatch.
//!
//! - `file_handler.reload`: user TOML 재로드. host/plugin 영향 없음.
//!   Method call wrapper (`Core::reload_file_handlers`) 직접 호출. 응답의 `rejected` 는
//!   이번 reload 가 적용하지 않은 user 항목과 사유다.
//! - `file_handler.detectors`: finalize 된 file format detector 전체와 각자의 출처별
//!   contribution 을 돌려준다. 조회 전용 — registry 를 finalize 하는 것 말고는 아무것도
//!   바꾸지 않는다(finalize 는 identify 가 어차피 하는 lazy 캐시 갱신이다).
//! - `file_handler.dispatch`: 임의 경로를 file_handler 시스템에 진입시킴.
//!   `DomainIntent::DispatchFile` 발화 — Core::apply 가 worker spawn,
//!   결과는 `AppEvent::IdentifyDone` 경로로 비동기 적용. **gui 빌드에만 있다** — 그 intent
//!   를 적용할 identify worker 와 결과를 여는 창이 headless 에 없어, headless 에서 받으면
//!   요청을 버리고도 수락했다고 답하게 된다. 그래서 headless 에서는 arm 이 없고 라우터
//!   끝이 `-32017` 로 답한다
//!   ([ADR-0425](../../../../docs/adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md)).

#[cfg(feature = "gui")]
use std::path::PathBuf;

#[cfg(feature = "gui")]
use serde::Deserialize;
use serde_json::json;

use crate::core::Core;
#[cfg(feature = "gui")]
use crate::file::format::{DetectDepth, FileTarget};
use tasty_ipc::protocol::JsonRpcResponse;

pub fn handle_reload(
    core: &Core,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let outcome = core.reload_file_handlers(engine);
    JsonRpcResponse::success(
        id,
        json!({
            "path": outcome.path.display().to_string(),
            "exists": outcome.exists,
            "rejected": rejected_json(&outcome.rejected),
        }),
    )
}

/// `file_handler.detectors` — finalize 된 detector 목록(id 순).
///
/// 각 항목은 병합 결과(`display_name_i18n_key` · `icon` · `disabled` · `rules`)와 그것을
/// 만든 출처별 원본(`contributions`)을 함께 싣는다. 병합 결과만으로는 어느 출처가 이겼는지
/// 밖에서 재현할 수 없어서다. `contributions` 는 설치 순서이고 병합 순서가 아니다.
/// rule 은 user 설정 파일의 `[[detector.rule]]` 과 같은 키로 적는다.
pub fn handle_detectors(engine: &crate::core::CoreState, id: serde_json::Value) -> JsonRpcResponse {
    let detectors: Vec<serde_json::Value> = engine
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
            // 문자열 키 표라 지금 실패할 값은 없다. 그래도 rule 하나 때문에 목록 전체를
            // 잃지 않게 그 rule 자리에만 이유를 싣는다.
            tracing::warn!(error = %e, "file_handler.detectors: rule 을 JSON 으로 옮기지 못했다");
            json!({ "error": e.to_string() })
        }
    }
}

/// reload 가 적용하지 않은 user 항목을 `[{ "id", "reason" }]` 로 싣는다. 없으면 빈 배열이다
/// (docs/adr/0426-file-handler-reload-reports-the-entries-it-dropped.md).
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
    /// 결과 surface 를 이 surface 가 속한 *Pane* 에 새 tab 으로 추가한다.
    /// None 이면 기존 동작 (focused pane 의 새 탭).
    #[serde(default)]
    origin_surface_id: Option<u32>,
    /// true 면 대용량 markdown 확인 팝업을 건너뛰고 즉시 연다(에이전트 강제 열기).
    /// 기본 false — 1MB 초과 markdown 은 확인 팝업이 뜬다.
    #[serde(default)]
    ignore_size_limit: bool,
}

#[cfg(feature = "gui")]
fn default_depth() -> String {
    "deep".to_string()
}

/// 임의 경로를 file_handler 디스패치 흐름에 진입시킨다. ctrl+click / drag&drop 과
/// 동일 흐름이지만 plugin / CLI 가 프로그래밍으로 호출하는 진입점.
///
/// - `params.path`: 절대 경로 권장. 상대 경로는 caller cwd 기준이라 비결정적.
/// - `params.depth`: `"cheap"` (확장자/glob 만) 또는 `"deep"` (magic/MIME 포함).
///   기본 `"deep"`. 두 경우 모두 worker thread 경유 (통일된 경로) — 응답은
///   즉시 돌아오고 handler 실행은 `AppEvent::IdentifyDone` 경로로 진행.
#[cfg(feature = "gui")]
pub fn handle_dispatch(
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &crate::core::CoreState,
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
    // 이 IPC 는 파일 경로만 받는다. URL 을 여는 것은 이 메서드의 권한 토큰(`FsRead`)이
    // 덮는 능력이 아니고, 경로 자리에 담긴 URL 은 확장자로 오식별된다 — 거절한다.
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
    out.push(
        crate::core::intent::DomainIntent::DispatchFile {
            target,
            depth,
            origin_surface_id: req.origin_surface_id,
            // 채널이 아니라 행위의 성질로 정한다 — 그런데 host 는 이 메서드의 호출자가
            // 에이전트인지 plugin 을 거친 사용자 클릭인지 **구분할 수단이 없다**(요청에
            // 그 값이 없다). 그래서 보수적인 쪽으로 고정한다: 포커스를 안 옮기는 쪽이다.
            dispatch_origin: crate::file::dispatch::FileDispatchOrigin::Agent,
            ignore_size_limit: req.ignore_size_limit,
        }
        .from_agent_ipc(),
    );
    JsonRpcResponse::success(
        id,
        json!({
            "accepted": true,
            "depth": req.depth,
            "ignore_size_limit": req.ignore_size_limit,
        }),
    )
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

    /// 경로 자리에 URL 이 오면 dispatch 인텐트를 세우지 않고 거절한다 — 세우면
    /// `https://example.com/a.md` 가 확장자로 markdown 핸들러에 걸린다.
    #[test]
    fn dispatch_rejects_a_url_in_the_path_param() {
        let (_state, engine) = crate::state::tests::test_state();
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let resp = handle_dispatch(
            &mut out,
            &engine,
            serde_json::json!(1),
            serde_json::json!({ "path": "https://example.com/a.md" }),
        );
        let err = resp.error.expect("URL path must be rejected");
        assert_eq!(err.code, -32602);
        assert!(out.is_empty());

        let resp = handle_dispatch(
            &mut out,
            &engine,
            serde_json::json!(2),
            serde_json::json!({ "path": "/tmp/a.md" }),
        );
        assert!(resp.error.is_none());
        assert_eq!(out.into_vec().len(), 1);
    }
    #[test]
    fn dispatch_rejects_missing_origin_before_enqueueing() {
        let (_state, engine) = crate::state::tests::test_state();
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let response = handle_dispatch(
            &mut out,
            &engine,
            serde_json::json!(42),
            serde_json::json!({"path":"/a", "origin_surface_id":u32::MAX}),
        );
        assert_eq!(response.error.unwrap().code, -32602);
        assert!(out.is_empty());
    }
}
