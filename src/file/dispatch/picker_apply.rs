//! 비동기 식별·picker 결과를 GUI 상태에 적용한다.

use crate::app::services::AppServices;
use crate::file::dispatch::DispatchTarget;
use crate::file::format::{DetectorId, FileTarget};
use crate::runtime::engine_access::EngineMut;
use crate::state::{FileHandlerPickerResult, RequestContext};

pub(crate) fn apply_identify_result(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    target: FileTarget,
    detector: Option<DetectorId>,
    origin_surface_id: Option<u32>,
    policy: crate::file::dispatch::FileDispatchPolicy,
) {
    let run = policy.run;
    if let Some(sid) = origin_surface_id
        && let Err(message) = crate::file::dispatch::require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        crate::file::dispatch::fail_run(run, message);
        return;
    }
    if let Some(sid) = origin_surface_id
        && engine.is_mirror_surface(sid)
    {
        crate::file::dispatch::remote::apply_remote_identify_result(
            core, state, engine, target, detector, sid, policy,
        );
        return;
    }
    let handlers = match &detector {
        Some(d) => engine.runtime.file_handler.handlers_for(d),
        None => Vec::new(),
    };
    if handlers.is_empty()
        && policy.dispatch_origin == crate::file::dispatch::FileDispatchOrigin::Agent
    {
        // 외부 IPC 요청은 사용자 화면에 picker를 띄우지 않는다. 사용자 입력을 증명하지 못한 plugin 중계 요청은
        // 사용자 클릭일 수 있어 fallback picker를 연다.
        tracing::warn!(target = %target.display(), "no file handler matches this file; not executed");
        crate::file::dispatch::fail_run(
            run,
            format!("no file handler matches {}", target.display()),
        );
        return;
    }
    let target = DispatchTarget::File(target);
    if handlers.is_empty() {
        // 매칭이 없으면 전체 핸들러를 일회성 선택지로 보여준다. detector 연결을 저장하지 않는다.
        let fallback = engine.runtime.file_handler.all_handlers();
        crate::file::dispatch::open_picker(state, engine, target, detector, fallback, true, policy);
        if let Some(picker) = state.dialogs.file_handler_picker.as_mut() {
            crate::file::dispatch::set_picker_origin(picker, &engine.read(), origin_surface_id);
        }
        return;
    }
    let first = handlers.into_iter().next().expect("non-empty checked");
    crate::file::dispatch::execute_handler_action(
        core,
        state,
        engine,
        &first,
        &target,
        origin_surface_id,
        policy,
    );
}

/// 선택된 핸들러 실행을 요청한다. picker 상태 해제는 호출자가 먼저 처리한다.
/// 핸들러가 사라진 경우에도 선택 이력을 기록하는 현재 동작이 있다.
pub(crate) fn apply_file_picker_result(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    target: DispatchTarget,
    result: FileHandlerPickerResult,
    origin_surface_id: Option<u32>,
    policy: crate::file::dispatch::FileDispatchPolicy,
) {
    let run = policy.run;
    let Some(handler_id) = selected_handler_id(result) else {
        crate::file::dispatch::finish_run(
            run,
            crate::request_runs::RunState::Cancelled,
            "the file handler picker closed without a choice",
            serde_json::Value::Null,
        );
        return;
    };
    if let Some(sid) = origin_surface_id
        && let Err(message) = crate::file::dispatch::require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        crate::file::dispatch::fail_run(run, message);
        return;
    }
    let Some(handler) = engine.runtime.file_handler.get(&handler_id) else {
        tracing::warn!(handler_id = %handler_id,
            "apply_file_picker_result: handler id from picker no longer in registry");
        crate::file::dispatch::fail_run(
            run,
            format!("file handler {handler_id} is no longer registered"),
        );
        state.record_file_handler_pick(&handler_id);
        return;
    };
    // true는 요청 수락을 뜻하며 외부 프로그램이나 plugin의 최종 성공 확인은 아니다.
    if crate::file::dispatch::execute_handler_action(
        core,
        state,
        engine,
        &handler,
        &target,
        origin_surface_id,
        policy,
    ) {
        state.record_file_handler_pick(&handler_id);
    }
}

fn selected_handler_id(result: FileHandlerPickerResult) -> Option<crate::file::handler::HandlerId> {
    match result {
        FileHandlerPickerResult::Selected(id) => Some(id),
        FileHandlerPickerResult::Cancelled => None,
        FileHandlerPickerResult::OpenSettings => {
            tracing::warn!(
                "apply_file_picker_result: OpenSettings should be intercepted by the App layer before reaching AppServices",
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "picker_apply_origin_tests.rs"]
mod origin_tests;

#[cfg(test)]
#[path = "picker_apply_run_tests.rs"]
mod run_tests;

#[cfg(test)]
pub(super) mod tests {
    use std::sync::{Arc, Mutex};

    use std::path::PathBuf;

    use super::*;
    use crate::app::services::builder::AppServicesBuilder;

    /// 직접 쓰지 않는 port도 AppServices 생성에 필요하므로 검사 대역을 주입한다.
    pub(in crate::file::dispatch) fn build_test_core()
    -> (AppServices, crate::runtime::engine_session::EngineSession) {
        use crate::adapters::test::{
            fake_clock::FakeClock, mock_clipboard::MockClipboard, tmp_home::TmpHome,
        };
        use crate::ports::notification_sound::NoopPlayer;

        let waker: tasty_terminal::Waker = Arc::new(|| {});
        let engine_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine");

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn tasty_memory::MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn tasty_themes::ThemeStorage> = Arc::new(tasty_themes::ThemeStore::new());

        let core = AppServicesBuilder::new()
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_home(Arc::new(TmpHome::new(
                tempfile::tempdir().expect("tmp").keep(),
            )))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test AppServices");
        (core, engine_session)
    }

    #[test]
    fn picker_falls_back_to_all_handlers_when_no_detector_match() {
        let (mut core, mut engine_session) = build_test_core();
        let mut engine = engine_session.borrow_mut();
        let unmatched = DetectorId::new("no-such-detector");
        assert!(
            engine
                .runtime
                .file_handler
                .handlers_for(&unmatched)
                .is_empty()
        );
        assert!(
            !engine.runtime.file_handler.all_handlers().is_empty(),
            "host defaults (html-system/directory-system) should give a non-empty fallback pool"
        );

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let mut state = RequestContext::new(&engine.read(), preset_store);

        apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            FileTarget::new(PathBuf::from("/tmp/unmatched-target.unknown")),
            Some(unmatched),
            None,
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                ignore_size_limit: false,
                run: None,
            },
        );

        let picker = state
            .dialogs
            .file_handler_picker
            .as_ref()
            .expect("picker popup should open with fallback candidates, not stay empty");
        assert!(
            !picker.candidates.is_empty(),
            "fallback candidates should be non-empty when the system has other handlers"
        );
        assert!(
            picker.candidates_are_fallback,
            "candidates originate from all_handlers() fallback, not a detector match"
        );
    }

    #[test]
    fn picker_result_does_not_run_an_ipc_handler_on_a_url_target() {
        use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
        let (mut core, mut engine_session) = build_test_core();
        let mut engine = engine_session.borrow_mut();
        FileHandlerRegistryPort::install_plugin_handlers(
            engine.runtime.file_handler.as_ref(),
            "com.example.urlprobe",
            &[serde_json::json!({
                "id": "open",
                "detector": "markdown",
                "priority": 50,
                "action": { "kind": "ipc", "method": "com.example.urlprobe.open" },
            })],
        );
        let handler_id = crate::file::handler::HandlerId::new("com.example.urlprobe/open");
        assert!(engine.runtime.file_handler.get(&handler_id).is_some());
        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let mut state = RequestContext::new(&engine.read(), preset_store);
        let recent_before = state.file_handler_recent.list().len();

        apply_file_picker_result(
            &mut core,
            &mut state,
            &mut engine,
            DispatchTarget::http_url("https://example.com/page").expect("url"),
            FileHandlerPickerResult::Selected(handler_id),
            None,
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: crate::file::dispatch::FileDispatchOrigin::Agent,
                ignore_size_limit: false,
                run: None,
            },
        );

        assert!(
            state.pending_handler_ipc.is_empty(),
            "a URL must not be enqueued for an Ipc handler (it would be sent as `path`)"
        );
        assert_eq!(state.file_handler_recent.list().len(), recent_before);
    }
}
