//! 플러그인이 호스트 파일 피커를 연다. request_id를 즉시 반환하고
//! 선택·취소 결과는 file_picker.result 이벤트로 나중에 보낸다(docs/design/systems/popup.md).
//! 호스트는 고른 경로로 파일을 열거나 복사하지 않는다. 경로의 쓰임은 플러그인이 정한다.
//! 이미 열린 피커는 대체하지 않고 두 번째 요청을 거절한다.
//! 대체하면 기존 요청자에게 취소를 통지할 수 없어 결과를 기다리는 요청이 남기 때문이다.
//! 다만 같은 요청자의 사용자 입력이 확인되면 passive로 열린 그 피커에 포커스를 준다.

use crate::runtime::engine_access::EngineMut;
use serde::Deserialize;
use serde_json::json;

use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use crate::adapters::ui::popup::file_picker;
use crate::state::{FilePickerRequester, RequestContext};

/// `file_picker.trigger { filters?: string[], owner_popup_instance?: u64, start_dir?: string,
/// origin_surface_id?: u32, mode?: "open" | "save", default_name?: string, location?: "local",
/// user_navigation_url?: string }` 요청.
#[derive(Deserialize)]
struct FilePickerTriggerReq {
    #[serde(default)]
    mode: PickerMode,
    #[serde(default)]
    default_name: String,
    /// Explicit local browsing; omission retains origin-surface routing for open mode.
    location: Option<PickerLocation>,
    /// 확장자 필터(점 없이, 예: `["md", "markdown"]`). 비면 필터 없음.
    #[serde(default)]
    filters: Vec<String>,
    /// 플러그인 팝업 안에서 열면 자기 인스턴스를 지정해 부모·자식으로 연결한다.
    /// 팝업 밖에서 여는 경우에는 생략한다.
    #[serde(default)]
    owner_popup_instance: Option<u64>,
    /// 시작 디렉토리. 로컬 출발이면 절대경로여야 하고 디렉토리가 아니면 홈으로 폴백한다.
    /// 원격(mirror) 출발이면 원격 경로 문자열을 그대로 서버에 보낸다. 생략하면 출발
    /// surface 의 cwd, 그것도 없으면 홈.
    #[serde(default)]
    start_dir: Option<String>,
    /// 피커를 띄운 surface id. 로컬/원격 판정을 이 surface 의 workspace 로 한다
    /// (생략하거나 찾지 못하면 활성 workspace).
    #[serde(default)]
    origin_surface_id: Option<u32>,
    /// 플러그인 페이지에서 사용자가 눌러 생긴 navigation URL. `origin_surface_id` 의 WebView 에서
    /// 확인되면 사용자 요청으로 연다. `file_handler.dispatch` 의 같은 필드와 같은 규칙이다.
    #[serde(default)]
    user_navigation_url: Option<String>,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum PickerMode {
    #[default]
    Open,
    Save,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PickerLocation {
    Local,
}

#[cfg(test)]
pub fn handle_trigger(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    handle_trigger_with_proofs(state, engine, caller, id, params, None)
}

/// `navigation_proofs` 는 플러그인 페이지의 사용자 navigation 기록이다. 없으면 사용자 요청으로 열지 않는다.
pub fn handle_trigger_with_proofs(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    id: serde_json::Value,
    params: &serde_json::Value,
    navigation_proofs: Option<&crate::app::html_runtime::NavigationProofs>,
) -> JsonRpcResponse {
    // 결과는 플러그인에게만 전달된다. 외부 호출은 결과 없이 사용자 포커스만 바꾸므로 거절한다.
    let requester_plugin = match caller {
        CallerContext::Plugin { plugin_id, .. } => plugin_id.clone(),
        CallerContext::Local { .. } | CallerContext::Agent { .. } => {
            return JsonRpcResponse::error(
                id,
                -32016,
                "method 'file_picker.trigger' answers only a plugin caller: the picked path \
                 is pushed to the calling plugin, so a CLI or agent caller would only take \
                 the user's input focus",
            );
        }
    };

    if file_picker::ensure_available(state).is_err() {
        return match focus_passive_picker(
            state,
            engine,
            &requester_plugin,
            params,
            navigation_proofs,
        ) {
            Some(request_id) => JsonRpcResponse::success(id, json!({ "request_id": request_id })),
            None => busy_response(id),
        };
    }

    let req: FilePickerTriggerReq = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => return JsonRpcResponse::error(id, -32602, format!("invalid params: {e}")),
    };

    let request_id = crate::state::FilePickerRequestId::next();
    let requester = FilePickerRequester {
        plugin_id: requester_plugin,
        request_id: request_id.0,
        owner_popup_instance: req.owner_popup_instance,
        origin_surface_id: req.origin_surface_id,
    };

    // 확인하지 못하면 포커스 없는 passive 피커가 되고, 키를 받는 WebView 가 피커를 덮는다
    // (docs/design/systems/popup.md#passive-팝업).
    let by_user = match (
        navigation_proofs,
        req.user_navigation_url.as_deref(),
        req.origin_surface_id,
    ) {
        (Some(proofs), Some(url), Some(sid)) => proofs.take(
            &state.webview_identity,
            &engine.as_ref(),
            &requester.plugin_id,
            sid,
            url,
        ),
        _ => false,
    };
    let opened_by = if by_user {
        file_picker::FilePickerOpenedBy::PluginForUser(requester)
    } else {
        file_picker::FilePickerOpenedBy::Plugin(requester)
    };

    let engine = engine.read();
    let location = if matches!(req.location, Some(PickerLocation::Local)) {
        crate::state::FilePickerLocation::Local { dir: req.start_dir }
    } else {
        file_picker::locate(
            state,
            &engine,
            req.origin_surface_id,
            state.active_workspace_index(&engine),
            req.start_dir,
        )
    };
    if req.mode == PickerMode::Save
        && matches!(location, crate::state::FilePickerLocation::Remote { .. })
    {
        return JsonRpcResponse::error(id, -32602, "save mode requires a local location");
    }
    if file_picker::open(state, &engine, request_id, location, req.filters, opened_by).is_err() {
        return busy_response(id);
    }

    if req.mode == PickerMode::Save {
        state
            .dialogs
            .file_picker
            .as_mut()
            .expect("opened picker")
            .save_name = Some(req.default_name);
    }
    JsonRpcResponse::success(id, json!({ "request_id": request_id.0 }))
}

/// 연타 경합 때 첫 요청은 뒤 입력이 기록을 덮어 passive로 열린다. 그 뒤 요청의 입력이 확인되면 첫 요청의 피커에
/// 포커스를 주고 첫 `request_id`를 돌려준다. 요청자·결과 전달은 그대로다. 같은 plugin·같은 surface의 피커만
/// 올리고, 증명은 실제로 올릴 때만 소비한다. 그 밖에는 None이고 호출자는 busy로 답한다.
fn focus_passive_picker(
    state: &mut RequestContext,
    engine: &EngineMut<'_>,
    plugin_id: &str,
    params: &serde_json::Value,
    navigation_proofs: Option<&crate::app::html_runtime::NavigationProofs>,
) -> Option<u64> {
    let proofs = navigation_proofs?;
    let req: FilePickerTriggerReq = serde_json::from_value(params.clone()).ok()?;
    let url = req.user_navigation_url.as_deref()?;
    let surface = req.origin_surface_id?;
    let picker = state.dialogs.file_picker.as_ref()?;
    let requester = picker.requester.as_ref()?;
    if requester.plugin_id != plugin_id
        || requester.origin_surface_id != Some(surface)
        || picker.result.is_some()
    {
        return None;
    }
    let request_id = requester.request_id;
    if !(state
        .popups
        .is_visible_passive(file_picker::FILE_PICKER_POPUP_ID)
        || passive_open_is_queued(state))
    {
        return None;
    }
    if !proofs.take(
        &state.webview_identity,
        &engine.as_ref(),
        plugin_id,
        surface,
        url,
    ) {
        return None;
    }
    state.dispatch_intent(
        crate::intent::UiIntent::FocusPassivePopup {
            id: file_picker::FILE_PICKER_POPUP_ID,
        }
        .from_user_menu("plugin_page"),
    );
    Some(request_id)
}

/// 첫 요청의 열기 Intent가 아직 처리되지 않았고 그 열기가 passive가 될 때. 포커스 Intent는 그 뒤에 처리된다.
fn passive_open_is_queued(state: &RequestContext) -> bool {
    state.pending_intents.iter().any(|intent| {
        !intent.origin.is_user()
            && matches!(
                &intent.body,
                crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup { id, .. })
                    if *id == file_picker::FILE_PICKER_POPUP_ID
            )
    })
}

fn busy_response(id: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id,
        -32000,
        "file_picker popup is already open — retry after it closes",
    )
}

/// 확정과 취소 모두 request_id·paths·cancelled를 포함한다.
const FILE_PICKER_RESULT_EVENT: &str = "file_picker.result";

/// 플러그인이 연 피커의 결과를 그 플러그인에만 보낸다. 소유 popup이 사라져도 보낸다.
/// 플러그인이 popup 밖에서 요청 상태를 관리할 수도 있기 때문이다.
pub(crate) fn deliver_result(
    plugin_manager: Option<&mut crate::plugin::PluginManager>,
    requester: &FilePickerRequester,
    outcome: &crate::state::FilePickerOutcome,
) {
    // 확정은 사용자 입력이다. 플러그인이 이 선택으로 낸 요청을 host가 사용자 요청으로 판정한다.
    if !outcome.cancelled {
        super::picker_proofs::record(&requester.plugin_id, requester.request_id, &outcome.paths);
    }
    let Some(mgr) = plugin_manager else {
        return;
    };
    if let Some(owner) = requester.owner_popup_instance
        && !mgr.popup_instances().any(|(id, _)| id == owner)
    {
        tracing::warn!(
            "file_picker request {} completed after owner popup {} closed; plugin {} may ignore the result",
            requester.request_id,
            owner,
            requester.plugin_id
        );
    }
    mgr.emit_host_event_to_plugin(
        &requester.plugin_id,
        FILE_PICKER_RESULT_EVENT,
        &result_payload(requester, outcome),
        tasty_plugin_protocol::EventScope::System,
    );
}

pub(crate) fn result_payload(
    requester: &FilePickerRequester,
    outcome: &crate::state::FilePickerOutcome,
) -> serde_json::Value {
    json!({
        "request_id": requester.request_id,
        "paths": outcome.paths,
        "cancelled": outcome.cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_test_state() -> (
        RequestContext,
        crate::runtime::engine_session::EngineSession,
    ) {
        crate::state::tests::test_state()
    }

    fn plugin_caller(id: &str) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: id.to_string(),
            permissions: Arc::new(Default::default()),
        }
    }

    #[test]
    fn trigger_from_plugin_records_requester() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({}),
        );
        let result = resp.result.expect("trigger should succeed");
        let request_id = result["request_id"].as_u64().expect("request_id");

        let data = state
            .dialogs
            .file_picker
            .as_ref()
            .expect("popup should be open");
        let req = data.requester.as_ref().expect("requester recorded");
        assert_eq!(req.plugin_id, "com.tasty.markdown");
        assert_eq!(req.request_id, request_id);
        assert_eq!(req.owner_popup_instance, None);
    }

    #[test]
    fn trigger_records_the_declared_owner_popup_instance() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "owner_popup_instance": 42 }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        let req = data.requester.as_ref().expect("requester recorded");
        assert_eq!(req.owner_popup_instance, Some(42));
        assert!(state.plugin_popup_has_open_child(42));
        assert!(!state.plugin_popup_has_open_child(43));
    }

    #[test]
    fn trigger_from_a_non_plugin_caller_is_refused_without_opening_the_popup() {
        let agent = CallerContext::Agent {
            agent_id: "child:1".to_string(),
            permissions: Arc::new(Default::default()),
        };
        for caller in [CallerContext::local(), agent] {
            let (mut state, mut engine_session) = make_test_state();
            let mut engine = engine_session.borrow_mut();
            let resp = handle_trigger(&mut state, &mut engine, &caller, json!(1), &json!({}));
            let err = resp.error.expect("non-plugin caller must be refused");
            assert_eq!(err.code, -32016, "{caller:?}");
            assert!(
                state.dialogs.file_picker.is_none(),
                "{caller:?} opened the picker"
            );
            assert!(
                state.take_pending_intents().is_empty(),
                "{caller:?} queued a popup intent"
            );
        }
    }

    #[test]
    fn second_trigger_while_open_is_rejected() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let first = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({}),
        );
        assert!(first.result.is_some());

        let second = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.other"),
            json!(2),
            &json!({}),
        );
        assert!(second.result.is_none());
        assert!(second.error.is_some());

        let req = state
            .dialogs
            .file_picker
            .as_ref()
            .and_then(|d| d.requester.as_ref())
            .expect("first requester still present");
        assert_eq!(req.plugin_id, "com.tasty.markdown");
    }

    #[test]
    fn trigger_passes_filters_through_to_popup_state() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "filters": ["md", "markdown"] }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        assert_eq!(data.filters, vec!["md".to_string(), "markdown".to_string()]);
    }

    fn home() -> String {
        directories::BaseDirs::new()
            .map(|d| d.home_dir().to_string_lossy().into_owned())
            .expect("home dir")
    }

    #[test]
    fn local_start_dir_is_used_when_it_is_a_directory() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tasty_fp_start_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "start_dir": dir.to_string_lossy() }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        assert!(data.mirror_ws_id.is_none());
        assert_eq!(data.current_dir, dir.to_string_lossy());
        std::fs::remove_dir_all(&dir).expect("cleanup temp dir");
    }

    #[test]
    fn missing_local_start_dir_falls_back_to_home() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "start_dir": "/definitely/not/a/tasty/dir" }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        assert_eq!(data.current_dir, home());
    }

    // 활성 workspace는 로컬로 두어 출발 surface 대신 활성 대상을 잘못 읽으면 드러나게 한다.
    fn push_background_mirror(engine: &mut crate::core::CoreState) -> (u32, u32) {
        let (ws_id, surface_id) = (9_000u32, 9_003u32);
        let mut ws = crate::model::Workspace::new_with_terminal_marker(
            ws_id,
            "mirror".to_string(),
            9_001,
            9_002,
            surface_id,
        );
        ws.mirror = true;
        engine.push_mirror_workspace(ws);
        (ws_id, surface_id)
    }

    #[test]
    fn origin_surface_decides_remote_even_when_active_workspace_is_local() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let (ws_id, sid) = push_background_mirror(&mut engine);
        assert!(
            !engine
                .workspace_at(state.active_workspace_index(&engine))
                .expect("workspace index is valid")
                .mirror
        );

        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "start_dir": "/srv/remote/proj", "origin_surface_id": sid }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        assert_eq!(data.mirror_ws_id, Some(ws_id));
        assert_eq!(data.current_dir, "/srv/remote/proj");
        let requests: Vec<_> = state
            .pending_intents
            .iter()
            .filter_map(|intent| match &intent.body {
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ListDirectory {
                        request,
                        projection,
                        target,
                    },
                ) => Some((request, projection, target)),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let (request, projection, target) = requests[0];
        assert_eq!(request.local_ws_id, ws_id);
        assert_eq!(
            request.dir, "/srv/remote/proj",
            "원격 경로는 stat 없이 그대로 간다"
        );
        assert!(engine.matches_mirror_projection(ws_id, projection));
        assert!(target.is_none());
        assert!(
            engine.remote.pending_list_dir_forward.is_empty(),
            "View only queues an App request"
        );
    }

    #[test]
    fn origin_without_start_dir_uses_the_pushed_remote_cwd() {
        let (mut state, mut engine_session) = make_test_state();
        let mut engine = engine_session.borrow_mut();
        let (ws_id, sid) = push_background_mirror(&mut engine);
        engine.set_mirror_surface_cwd(sid, Some("/srv/pushed".to_string()));

        let resp = handle_trigger(
            &mut state,
            &mut engine,
            &plugin_caller("com.tasty.markdown"),
            json!(1),
            &json!({ "origin_surface_id": sid }),
        );
        assert!(resp.result.is_some());
        let data = state.dialogs.file_picker.as_ref().expect("popup open");
        assert_eq!(data.mirror_ws_id, Some(ws_id));
        assert_eq!(data.current_dir, "/srv/pushed");
    }
}

#[cfg(test)]
#[path = "file_picker_scope_tests.rs"]
mod scope_tests;
