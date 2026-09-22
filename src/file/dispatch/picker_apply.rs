//! identify 결과 · picker 결과를 적용한다 — GUI 동작이다.
//!
//! 매칭 handler 가 없으면 picker popup 을 열고(`state.dialogs`·`state.popups` 를 만진다),
//! 있으면 1 순위를 실행한다. 두 함수 모두 창 상태(`AppState`)를 바꾸므로 도메인(`core`)이
//! 아니라 이 GUI 쪽 모듈에 있다. 호출자는 `App::apply_identify_result`(`AppEvent::IdentifyDone`)
//! 와 `App::dispatch_pending_picker_results` 다.

use crate::core::{Core, CoreState};
use crate::file::dispatch::DispatchTarget;
use crate::file::format::{DetectorId, FileTarget};
use crate::state::{AppState, FileHandlerPickerResult};

/// IdentifyWorker 의 비동기 detect 결과 적용. `event_handler` 가
/// `AppEvent::IdentifyDone` 수신 시 직접 호출. detector 매칭 handler 가
/// 있으면 1순위 자동 실행, 없으면 picker popup 오픈 (state.dialogs +
/// state.popups mutate). 옛 `file_dispatch::apply_identify_result` 본문 흡수.
pub(crate) fn apply_identify_result(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    target: FileTarget,
    detector: Option<DetectorId>,
    origin_surface_id: Option<u32>,
    dispatch_origin: crate::file::dispatch::FileDispatchOrigin,
    ignore_size_limit: bool,
) {
    if let Some(sid) = origin_surface_id
        && let Err(message) = crate::file::dispatch::require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        return;
    }
    let handlers = match &detector {
        Some(d) => engine.file_handler.handlers_for(d),
        None => Vec::new(),
    };
    let target = DispatchTarget::File(target);
    if handlers.is_empty() {
        // 이 detector 에 매칭되는 handler 가 없다 — 시스템에 등록된 다른
        // handler 라도 fallback 후보로 보여준다(picker 의 empty-state 완화).
        // 선택되어도 이 detector 에 영구 등록되지 않는 1회성 dispatch.
        let fallback = engine.file_handler.all_handlers();
        crate::file::dispatch::open_picker(
            state,
            engine,
            target,
            detector,
            fallback,
            true,
            dispatch_origin,
            ignore_size_limit,
        );
        if let Some(picker) = state.dialogs.file_handler_picker.as_mut() {
            picker.origin_surface_id = origin_surface_id;
        }
        return;
    }
    // 정렬 1순위가 자동 선택. 단일 / 복수 동일 — 첫 항목 dispatch.
    let first = handlers.into_iter().next().expect("non-empty checked");
    // 파일 대상은 모든 핸들러가 받으므로(`handler_accepts_target`) 거절되지 않는다.
    crate::file::dispatch::execute_handler_action(
        core,
        state,
        engine,
        &first,
        &target,
        origin_surface_id,
        dispatch_origin,
        ignore_size_limit,
    );
}

/// Picker popup 결과를 적용 — `redraw.rs` frame end 가 직접 호출.
/// `Selected(id)` 면 handler 실행 + recent 기록, `Cancelled` 면 no-op.
/// 옛 `file_dispatch::consume_picker_result` 본문 흡수. dialogs 슬롯 해제는
/// caller (redraw) 가 본 method 호출 *전에* 처리한다 — 빠른 popup 재오픈
/// 시에도 결과 중복 처리 없음을 보장하기 위함.
pub(crate) fn apply_file_picker_result(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    target: DispatchTarget,
    result: FileHandlerPickerResult,
    origin_surface_id: Option<u32>,
    dispatch_origin: crate::file::dispatch::FileDispatchOrigin,
    ignore_size_limit: bool,
) {
    let Some(handler_id) = selected_handler_id(result) else {
        return;
    };
    if let Some(sid) = origin_surface_id
        && let Err(message) = crate::file::dispatch::require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        return;
    }
    let Some(handler) = engine.file_handler.get(&handler_id) else {
        tracing::warn!(handler_id = %handler_id,
            "apply_file_picker_result: handler id from picker no longer in registry");
        engine.record_file_handler_pick(&handler_id);
        return;
    };
    // Record only accepted actions; rejected targets must not return as recent picks.
    if crate::file::dispatch::execute_handler_action(
        core,
        state,
        engine,
        &handler,
        &target,
        origin_surface_id,
        dispatch_origin,
        ignore_size_limit,
    ) {
        engine.record_file_handler_pick(&handler_id);
    }
}

/// Non-selection results have no handler action or recent entry.
fn selected_handler_id(result: FileHandlerPickerResult) -> Option<crate::file::handler::HandlerId> {
    match result {
        FileHandlerPickerResult::Selected(id) => Some(id),
        FileHandlerPickerResult::Cancelled => None,
        FileHandlerPickerResult::OpenSettings => {
            tracing::warn!(
                "apply_file_picker_result: OpenSettings should be intercepted by the App layer before reaching Core",
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "picker_apply_origin_tests.rs"]
mod origin_tests;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use std::path::PathBuf;

    use super::*;
    use crate::core::builder::CoreBuilder;

    /// `mirror_structural_guard_tests::build_test_core` 와 동형(모든 port
    /// mock/in-memory 주입). `apply_identify_result` 의 empty-handler 분기는 어떤
    /// port 도 건드리지 않지만 메서드 자체가 `Core` 를 요구해 완전한 인스턴스가 필요.
    pub(super) fn build_test_core() -> (Core, CoreState) {
        use crate::adapters::test::{
            fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
            mock_process::MockProcessSpawner, tmp_home::TmpHome,
        };
        use crate::ports::notification_sound::NoopPlayer;

        let waker: tasty_terminal::Waker = Arc::new(|| {});
        let engine = CoreState::new(80, 24, waker).expect("engine");

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn tasty_memory::MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn tasty_themes::ThemeStorage> = Arc::new(tasty_themes::ThemeStore::new());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(TmpHome::new(
                tempfile::tempdir().expect("tmp").keep(),
            )))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core");
        (core, engine)
    }

    /// Case A — detector 가 매칭하는 handler 는 0개지만(`html-system` /
    /// `directory-system` 같은 host default 는 다른 detector 용으로 항상 존재)
    /// picker 는 empty-state 로 떨어지지 않고 `all_handlers()` fallback 을
    /// 후보로 노출해야 한다.
    #[test]
    fn picker_falls_back_to_all_handlers_when_no_detector_match() {
        let (mut core, mut engine) = build_test_core();
        let unmatched = DetectorId::new("no-such-detector");
        assert!(engine.file_handler.handlers_for(&unmatched).is_empty());
        assert!(
            !engine.file_handler.all_handlers().is_empty(),
            "host defaults (html-system/directory-system) should give a non-empty fallback pool"
        );

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn tasty_memory::MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let mut state = AppState::new(&mut engine, preset_store, memory);

        apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            FileTarget::new(PathBuf::from("/tmp/unmatched-target.unknown")),
            Some(unmatched),
            None,
            crate::file::dispatch::FileDispatchOrigin::Agent,
            false,
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

    /// URL 대상을 URL 을 못 받는 핸들러(Ipc)로 실행하려 하면 picker 결과가 와도 실행하지
    /// 않는다 — plugin 에 `{"path": "https://…"}` 가 가지 않고, recent 에도 안 남는다.
    #[test]
    fn picker_result_does_not_run_an_ipc_handler_on_a_url_target() {
        use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
        let (mut core, mut engine) = build_test_core();
        FileHandlerRegistryPort::install_plugin_handlers(
            engine.file_handler.as_ref(),
            "com.example.urlprobe",
            &[serde_json::json!({
                "id": "open",
                "detector": "markdown",
                "priority": 50,
                "action": { "kind": "ipc", "method": "com.example.urlprobe.open" },
            })],
        );
        let handler_id = crate::file::handler::HandlerId::new("com.example.urlprobe/open");
        assert!(engine.file_handler.get(&handler_id).is_some());
        let recent_before = engine.file_handler_recent.list().len();

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn tasty_memory::MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let mut state = AppState::new(&mut engine, preset_store, memory);

        apply_file_picker_result(
            &mut core,
            &mut state,
            &mut engine,
            DispatchTarget::http_url("https://example.com/page").expect("url"),
            FileHandlerPickerResult::Selected(handler_id),
            None,
            crate::file::dispatch::FileDispatchOrigin::Agent,
            false,
        );

        assert!(
            state.pending_handler_ipc.is_empty(),
            "a URL must not be enqueued for an Ipc handler (it would be sent as `path`)"
        );
        assert_eq!(engine.file_handler_recent.list().len(), recent_before);
    }
}
