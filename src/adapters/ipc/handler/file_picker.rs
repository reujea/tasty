//! `file_picker.trigger` IPC(ADR-0058) — plugin 이 host 소유 `file_picker`
//! popup(ADR-0053, `adapters::ui::popup::file_picker`)을 열도록 트리거한다.
//!
//! **popup 확정을 기다리지 않고** `request_id` 만 즉시 회신한다(비동기 accept) —
//! 실제 확정/취소 결과는 `src/app/dispatch/file_picker.rs` 가 확정 지점에서
//! `"file_picker.result"` 이벤트로 plugin 에 push 한다. `git_viewer.query`
//! (ADR-0056)와 동일한 "즉시 ack + 이벤트 push" shape.
//!
//! # 동시성 정책
//!
//! `file_picker` popup 은 단일 인스턴스만 존재한다(`AppState::dialogs.file_picker:
//! Option<..>`). 이미 열려 있는 상태에서 두 번째 `file_picker.trigger` 가 들어오면
//! **거부**한다(즉시 JSON-RPC 에러) — "이전 요청을 대체"하는 정책은 채택하지 않았다.
//! 이 트리거 핸들러는 `state`/`engine` 만 받고 `PluginManager` 접근권이 없어(이
//! 코드베이스의 확립된 관례 — IPC 핸들러는 `CoreState` 에 pending 을 큐잉하고
//! `App` 레벨 dispatch 가 `plugin_manager` 로 실제 이벤트를 emit 한다, `git_viewer`/
//! `list_dir` forward 와 동형), "대체" 정책을 택하면 밀려난 이전 요청의 plugin 에게
//! 즉시 취소를 통지할 방법이 없다 — 그 plugin 의 pending-map 항목이 응답을 영영
//! 받지 못한 채 무기한 남는다(`host.call` 자체는 이미 즉시 반환했으므로 타임아웃
//! 크래시는 아니지만, ADR-0058 이 세운 "모든 트리거는 정확히 하나의 결과를 받는다"
//! 계약을 조용히 깬다). 거부는 두 번째 plugin 의 `host.call` 이 즉시 에러로 끝나
//! 그 자리에서 재시도 여부를 판단할 수 있게 하므로 이 계약을 지킨다.

use serde::Deserialize;
use serde_json::json;

use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use crate::state::{AppState, FilePickerRequester};

/// `file_picker.trigger { filters?: string[], owner_popup_instance?: u64, start_dir?: string,
/// origin_surface_id?: u32 }` 요청.
#[derive(Deserialize)]
struct FilePickerTriggerReq {
    /// 확장자 필터(점 없이, 예: `["md", "markdown"]`). 비면 필터 없음.
    #[serde(default)]
    filters: Vec<String>,
    /// 이 트리거를 낸 **자기 popup instance_id**. plugin 이 자기 popup 안에서
    /// 피커를 열었다면 반드시 싣는다 — host 는 이 값으로 부모-자식 스택을 세운다
    /// (부모가 자식보다 먼저 닫히지 않게, 부모가 닫히면 자식을 정리; ADR-0084).
    /// popup 밖(surface 위젯 등)에서 호출하면 생략한다.
    #[serde(default)]
    owner_popup_instance: Option<u64>,
    /// 시작 디렉토리. 로컬 출발이면 절대경로여야 하고 디렉토리가 아니면 홈으로 폴백한다.
    /// 원격(mirror) 출발이면 원격 경로 문자열을 그대로 서버에 보낸다. 생략하면 출발
    /// surface 의 cwd, 그것도 없으면 홈.
    #[serde(default)]
    start_dir: Option<String>,
    /// 피커를 띄운 **로컬** surface id. 로컬/원격 판정을 이 surface 의 workspace 로 한다
    /// (생략하면 활성 workspace).
    #[serde(default)]
    origin_surface_id: Option<u32>,
}

/// `file_picker.trigger` — `file_picker` popup 을 열고 `request_id` 만 즉시
/// 회신한다. 이미 popup 이 열려 있으면 거부(위 모듈 doc "동시성 정책" 참고).
pub fn handle_trigger(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    // plugin 만 연다. 결과는 호출한 plugin 에게만 push 되므로 CLI·agent 호출에는 받을 곳이
    // 없고, 남는 효과는 사용자 입력 포커스를 가져가는 popup 뿐이다 — 원칙 1 ①·2.3 위반이다
    // (`docs/adr/0504-the-file-picker-trigger-answers-only-a-plugin-caller.md`).
    let requester_plugin = match caller {
        CallerContext::Plugin { plugin_id, .. } => plugin_id.clone(),
        CallerContext::Local | CallerContext::Agent { .. } => {
            return JsonRpcResponse::error(
                id,
                -32016,
                "method 'file_picker.trigger' answers only a plugin caller: the picked path \
                 is pushed to the calling plugin, so a CLI or agent caller would only take \
                 the user's input focus",
            );
        }
    };

    if state.dialogs.file_picker.is_some() {
        return JsonRpcResponse::error(
            id,
            -32000,
            "file_picker popup is already open — retry after it closes",
        );
    }

    let req: FilePickerTriggerReq = match serde_json::from_value(params.clone()) {
        Ok(r) => r,
        Err(e) => return JsonRpcResponse::error(id, -32602, format!("invalid params: {e}")),
    };

    let request_id = crate::core::next_file_picker_trigger_request_id();
    let requester = Some(FilePickerRequester {
        plugin_id: requester_plugin,
        request_id,
        owner_popup_instance: req.owner_popup_instance,
    });

    use crate::adapters::ui::popup::file_picker::FilePickerStart;
    let start = match req.start_dir {
        Some(dir) => FilePickerStart {
            dir: Some(dir),
            origin_surface_id: req.origin_surface_id,
        },
        None => FilePickerStart::from_surface(engine, req.origin_surface_id),
    };
    crate::adapters::ui::popup::file_picker::open(state, engine, requester, req.filters, start);

    JsonRpcResponse::success(id, json!({ "request_id": request_id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tasty_memory::MemoryStorage;

    fn make_test_state() -> (AppState, crate::core::CoreState) {
        let term_waker: crate::terminal::Waker = Arc::new(|| {});
        let mut engine = crate::core::CoreState::new(80, 24, term_waker).unwrap();
        let preset_store = Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let state = AppState::new(&mut engine, preset_store, memory);
        (state, engine)
    }

    fn plugin_caller(id: &str) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: id.to_string(),
            permissions: Arc::new(Default::default()),
        }
    }

    #[test]
    fn trigger_from_plugin_records_requester() {
        let (mut state, mut engine) = make_test_state();
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
        // 신고하지 않으면 부모 없음 — popup 밖에서의 호출과 같은 취급(ADR-0084).
        assert_eq!(req.owner_popup_instance, None);
    }

    /// `owner_popup_instance` 를 실으면 부모-자식 스택이 성립한다(ADR-0084).
    #[test]
    fn trigger_records_the_declared_owner_popup_instance() {
        let (mut state, mut engine) = make_test_state();
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

    /// CLI·agent 호출은 popup 을 열지 않고 `-32016` 으로 끝난다. 결과를 받을 plugin 이 없어
    /// 남는 효과가 사용자 입력 포커스를 가져가는 것뿐이었다(원칙 1 ①·2.3, ADR-0504).
    #[test]
    fn trigger_from_a_non_plugin_caller_is_refused_without_opening_the_popup() {
        let agent = CallerContext::Agent {
            agent_id: "child:1".to_string(),
            permissions: Arc::new(Default::default()),
        };
        for caller in [CallerContext::Local, agent] {
            let (mut state, mut engine) = make_test_state();
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

    /// 동시성 정책 — 이미 popup 이 열려 있으면 두 번째 trigger 를
    /// 거부한다("대체" 아님). 근거는 이 파일 모듈 doc 참고.
    #[test]
    fn second_trigger_while_open_is_rejected() {
        let (mut state, mut engine) = make_test_state();
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

        // 첫 요청의 requester 는 그대로 살아 있다 — 대체되지 않았다.
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
        let (mut state, mut engine) = make_test_state();
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

    // ---- 시작 디렉토리와 출발 surface ----

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
        let (mut state, mut engine) = make_test_state();
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

    /// 존재하지 않는 경로로 열면 빈 에러 화면이 아니라 홈에서 출발한다.
    #[test]
    fn missing_local_start_dir_falls_back_to_home() {
        let (mut state, mut engine) = make_test_state();
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

    /// 활성 workspace 에 딸린 mirror workspace 를 하나 붙이고 그 surface id 를 돌려준다.
    /// 활성 workspace 는 로컬 그대로라, 판정이 활성 기준이면 로컬로 나온다.
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
        engine.workspaces.push(ws);
        (ws_id, surface_id)
    }

    #[test]
    fn origin_surface_decides_remote_even_when_active_workspace_is_local() {
        let (mut state, mut engine) = make_test_state();
        let (ws_id, sid) = push_background_mirror(&mut engine);
        assert!(!engine.workspaces[state.active_workspace].mirror);

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
        let forward = engine
            .pending_list_dir_forward
            .last()
            .expect("원격 조회가 큐잉된다");
        assert_eq!(
            forward.dir, "/srv/remote/proj",
            "원격 경로는 stat 없이 그대로 간다"
        );
    }

    /// 시작 경로를 안 실어도 출발 surface 가 mirror 면 서버가 push 한 원격 cwd 에서 연다.
    #[test]
    fn origin_without_start_dir_uses_the_pushed_remote_cwd() {
        let (mut state, mut engine) = make_test_state();
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
