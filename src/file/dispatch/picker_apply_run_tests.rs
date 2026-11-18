//! 접수한 파일 열기 요청의 최종 결과가 그 요청의 run에만 기록되는지 확인한다.

use super::tests::build_test_core;
use super::{apply_file_picker_result, apply_identify_result};
use crate::file::dispatch::{
    DispatchTarget, FileDispatchOrigin, FileDispatchPolicy, execute_handler_action,
};
use crate::file::format::{DetectorId, FileTarget};
use crate::file::handler::{FileHandler, HandlerAction, HandlerId, HandlerOwner};
use crate::request_runs::{begin, lookup};
use crate::state::FileHandlerPickerResult;

fn policy(origin: FileDispatchOrigin, run: u64) -> FileDispatchPolicy {
    FileDispatchPolicy {
        dispatch_origin: origin,
        ignore_size_limit: false,
        run: Some(run),
        result_place: Default::default(),
    }
}

fn state_of(run: u64) -> (String, String) {
    let v = lookup(run, None).expect("run is kept");
    (
        v["state"].as_str().unwrap().to_string(),
        v["result"].as_str().unwrap().to_string(),
    )
}

fn handler(action: HandlerAction) -> FileHandler {
    FileHandler {
        id: HandlerId::new("host/run-test"),
        detector: DetectorId::new("run-test"),
        priority: 0,
        owner: HandlerOwner::Host,
        action,
        display_name_i18n_key: None,
        disabled: false,
    }
}

#[test]
fn an_agent_request_without_a_handler_fails_its_own_run_only() {
    let (mut core, _) = build_test_core();
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let first = begin("file_handler.dispatch", None, serde_json::Value::Null);
    let second = begin("file_handler.dispatch", None, serde_json::Value::Null);
    for run in [first, second] {
        apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            FileTarget::new("/same/unknown.zzz"),
            None,
            None,
            policy(FileDispatchOrigin::Agent, run),
        );
    }
    for run in [first, second] {
        assert_eq!(
            state_of(run),
            (
                "failed".into(),
                "no file handler matches /same/unknown.zzz".into()
            )
        );
    }
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
}

#[test]
fn a_dead_origin_fails_the_run_with_the_reason() {
    let (mut core, _) = build_test_core();
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for action in [
        HandlerAction::OpenSurface {
            surface_kind: "empty".into(),
            param_key: "file".into(),
        },
        HandlerAction::Ipc {
            method: "example.open".into(),
            owner_plugin_id: "example".into(),
        },
        HandlerAction::System,
    ] {
        let run = begin("file_handler.dispatch", None, serde_json::Value::Null);
        assert!(!execute_handler_action(
            &mut core,
            &mut state,
            &mut engine,
            &handler(action),
            &DispatchTarget::File(FileTarget::new("/missing")),
            Some(u32::MAX),
            policy(FileDispatchOrigin::Agent, run),
        ));
        let (st, result) = state_of(run);
        assert_eq!(st, "failed");
        assert!(result.contains(&u32::MAX.to_string()), "{result}");
    }
}

#[test]
fn a_handler_that_rejects_the_target_fails_the_run() {
    let (mut core, _) = build_test_core();
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let run = begin("file_handler.dispatch", None, serde_json::Value::Null);
    let ipc = handler(HandlerAction::Ipc {
        method: "example.open".into(),
        owner_plugin_id: "example".into(),
    });
    assert!(!execute_handler_action(
        &mut core,
        &mut state,
        &mut engine,
        &ipc,
        &DispatchTarget::Url("https://example.com".into()),
        None,
        policy(FileDispatchOrigin::Agent, run),
    ));
    let (st, result) = state_of(run);
    assert_eq!(st, "failed");
    assert!(result.contains("does not accept"), "{result}");
}

#[test]
fn queued_actions_carry_the_run_to_their_completion() {
    let (mut core, _) = build_test_core();
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let ipc_run = begin("file_handler.dispatch", None, serde_json::Value::Null);
    assert!(execute_handler_action(
        &mut core,
        &mut state,
        &mut engine,
        &handler(HandlerAction::Ipc {
            method: "example.open".into(),
            owner_plugin_id: "example".into(),
        }),
        &DispatchTarget::File(FileTarget::new("/a.txt")),
        None,
        policy(FileDispatchOrigin::Agent, ipc_run),
    ));
    assert_eq!(state.pending_handler_ipc.len(), 1);
    assert_eq!(state.pending_handler_ipc[0].2, Some(ipc_run));
    assert_eq!(state_of(ipc_run).0, "running");
    let fields = lookup(ipc_run, None).unwrap();
    assert_eq!(fields["handler"], "host/run-test");
    assert_eq!(fields["action"], "ipc");

    let tab_run = begin("file_handler.dispatch", None, serde_json::Value::Null);
    assert!(execute_handler_action(
        &mut core,
        &mut state,
        &mut engine,
        &handler(HandlerAction::OpenSurface {
            surface_kind: "empty".into(),
            param_key: "file".into(),
        }),
        &DispatchTarget::File(FileTarget::new("/a.txt")),
        None,
        policy(FileDispatchOrigin::Agent, tab_run),
    ));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].completion, Some(tab_run));
    assert_eq!(state_of(tab_run).0, "running");
}

#[test]
fn the_fallback_picker_ends_its_run_when_cancelled_replaced_or_dropped() {
    let (mut core, _) = build_test_core();
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let open = |state: &mut crate::state::RequestContext,
                engine: &mut crate::runtime::engine_access::EngineMut<'_>,
                core: &mut crate::app::services::AppServices,
                run: u64| {
        apply_identify_result(
            core,
            state,
            engine,
            FileTarget::new("/same/unknown.zzz"),
            None,
            None,
            policy(FileDispatchOrigin::PluginUnverified, run),
        );
        assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(state).is_some());
    };

    let replaced = begin("file_handler.dispatch", None, serde_json::Value::Null);
    open(&mut state, &mut engine, &mut core, replaced);
    assert_eq!(state_of(replaced).0, "running");
    let cancelled = begin("file_handler.dispatch", None, serde_json::Value::Null);
    open(&mut state, &mut engine, &mut core, cancelled);
    assert_eq!(
        state_of(replaced),
        (
            "cancelled".into(),
            "replaced by a newer file handler picker".into()
        )
    );
    assert_eq!(state_of(cancelled).0, "running");

    // App이 picker를 닫을 때처럼 run을 넘겨받아 취소 결과를 적용한다.
    let guard = crate::adapters::ui::popup::file_handler_picker::pickers_mut(&mut state)
        .find_map(|picker| picker.run.take())
        .expect("picker keeps the run");
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        DispatchTarget::File(FileTarget::new("/same/unknown.zzz")),
        FileHandlerPickerResult::Cancelled,
        None,
        policy(FileDispatchOrigin::PluginUnverified, guard.take()),
    );
    drop(guard);
    assert_eq!(
        state_of(cancelled),
        (
            "cancelled".into(),
            "the file handler picker closed without a choice".into()
        )
    );

    let dropped = begin("file_handler.dispatch", None, serde_json::Value::Null);
    open(&mut state, &mut engine, &mut core, dropped);
    // 선택창을 닫고 닫힌 데이터를 버린다.
    state
        .popups
        .close(crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID);
    drop(state.popups.take_closed_queue());
    assert_eq!(state_of(dropped).0, "cancelled");
}

/// 다른 surface에서 연 선택창은 앞 선택창을 바꾸지 않는다. 두 선택창이 각자 대상과 기다리는 요청을 가진다.
#[test]
fn a_picker_from_another_surface_keeps_the_first_picker_and_its_run() {
    use crate::adapters::ui::popup::file_handler_picker::{open_pickers, opened_picker};
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let all = read.file_handler.all_handlers();
    let first = begin("file_handler.dispatch", None, serde_json::Value::Null);
    let second = begin("file_handler.dispatch", None, serde_json::Value::Null);
    for (sid, path, run) in [(1, "/a.zzz", first), (2, "/b.zzz", second)] {
        crate::file::dispatch::open_picker_from_read(
            &mut state,
            &read,
            DispatchTarget::File(FileTarget::new(path)),
            None,
            all.clone(),
            true,
            crate::file::dispatch::PickerOrigin::ShownIn(sid),
            policy(FileDispatchOrigin::User, run),
        );
        assert!(opened_picker(&mut state).is_some());
    }
    assert_eq!(state_of(first).0, "running");
    assert_eq!(state_of(second).0, "running");
    let mut shown: Vec<String> = open_pickers(&state)
        .map(|(_, p)| p.target_display.clone())
        .collect();
    shown.sort();
    assert_eq!(shown, vec!["/a.zzz".to_string(), "/b.zzz".to_string()]);
}
