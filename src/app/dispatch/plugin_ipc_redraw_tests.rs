//! 플러그인 host call 처리 여부를 about_to_wait의 redraw 판단에 넘기는지 잰다. 창이 없는 App이라
//! dirty 자체는 볼 수 없어서 판단 입력(반환값)을 확인한다. 회신은 게이트가 호출을 보류할 수 있어 보지 않는다.
//! `App`을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop`는 프로세스에 하나뿐이라 새 프로세스에서 혼자 돈다.

use std::sync::Arc;

use serde_json::json;

const CALLER: &str = "com.test.plugin-ipc-redraw-caller";

/// 창 없는 gui App에 호출자 stub 플러그인을 붙인다. stub을 놓으면 송신이 끊기므로 함께 돌려준다.
fn app_with_caller(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
) -> (crate::app::App, tasty_host_plugin::manager::CallerStub) {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    let mut manager = crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let caller = manager.attach_caller_stub_for_test(CALLER);
    app.plugin_manager = Some(manager);
    (app, caller)
}

/// Given 대기 중인 host call 하나, When 처리하면, Then true를 반환하고 그 호출은 대기열에서 빠진다.
/// Given 대기 중인 호출이 없음, When 다시 처리하면, Then false다.
#[test]
fn processing_a_plugin_call_reports_it_for_the_redraw() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "processing_a_plugin_call_reports_it_for_the_redraw",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = crate::x11_test_loop::x11_event_loop::<crate::AppEvent>();
    let (mut app, _caller) = app_with_caller(&event_loop);

    assert!(
        !app.process_plugin_ipc_calls(),
        "대기 호출이 없는데 처리했다고 보고했다"
    );

    app.plugin_manager
        .as_mut()
        .expect("manager")
        .queue_plugin_call_for_test(CALLER, 1, "banner.close", json!({}));
    assert!(
        app.process_plugin_ipc_calls(),
        "호출을 처리하고도 redraw 판단에 넘기지 않았다"
    );
    assert!(
        !app.process_plugin_ipc_calls(),
        "꺼낸 호출을 다시 처리했다고 보고했다"
    );
}

/// Given 요청 게이트에 보류한 플러그인 호출, When IPC 회차가 게이트를 진행해 그 호출을 재개하면,
/// Then 재개 사실이 플러그인 redraw 판단에 한 번 남는다.
#[test]
fn a_plugin_call_resumed_from_the_request_gate_reaches_the_redraw_decision() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_plugin_call_resumed_from_the_request_gate_reaches_the_redraw_decision",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = crate::x11_test_loop::x11_event_loop::<crate::AppEvent>();
    let (mut app, _caller) = app_with_caller(&event_loop);
    let manager = app.plugin_manager.as_mut().expect("manager");
    manager.queue_plugin_call_for_test(CALLER, 1, "banner.close", json!({}));
    let call = manager.take_pending_plugin_calls().remove(0);
    let request: crate::ipc::protocol::JsonRpcRequest = serde_json::from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "banner.close", "params": {},
    }))
    .expect("request");
    app.gate_queue.park(
        crate::app::request_gate::Gated::Plugin {
            call,
            request,
            progress: Default::default(),
        },
        false,
    );
    assert!(!app.plugin_calls_resumed, "재개 전에 표지가 섰다");

    app.process_ipc();
    assert!(
        std::mem::take(&mut app.plugin_calls_resumed),
        "게이트에서 재개한 플러그인 호출을 redraw 판단에 넘기지 않았다"
    );
    app.process_ipc();
    assert!(
        !app.plugin_calls_resumed,
        "재개할 호출이 없는데 표지가 섰다"
    );
}
