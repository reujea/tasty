//! 플러그인 host call 처리 여부를 about_to_wait의 redraw 판단에 넘기는지 잰다. 창이 없는 App이라
//! dirty 자체는 볼 수 없어서 판단 입력(반환값)을 확인한다. 회신 내용은 판단하지 않고, 게이트에 보류한
//! 호출이 끝났는지 확인할 때만 회신 도착을 본다.
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
/// Then 첫 재개 회차에 재개 사실이 플러그인 redraw 판단에 남는다.
/// When 호출이 끝나 재개할 호출이 없는 회차(행 대기도 없다)를 돌면, Then 표지가 서지 않는다.
/// 재개한 호출은 writer 결과를 기다리며 게이트에 다시 설 수 있고, 다시 선 호출은 재개할 때마다 다시 세운다.
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
    let (mut app, caller) = app_with_caller(&event_loop);
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
    // 재개한 호출은 cap 조회 결과를 기다리며 게이트에 다시 설 수 있다. 호스트 루프가 부르는 것과 같은
    // process_ipc를 게이트가 빌 때까지 부른다. 그 회차들의 재개 표지는 이 시험이 판정하지 않으므로 소비만 한다.
    // 기한은 회차를 돈 뒤에도 게이트가 남았을 때만 판정한다. 결과가 나온 뒤 시험 스레드가 늦게 깨어나도
    // 그 회차가 결과를 거둔다.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !app.gate_queue.is_empty() {
        std::thread::sleep(std::time::Duration::from_millis(1));
        app.process_ipc();
        app.plugin_calls_resumed = false;
        assert!(
            app.gate_queue.is_empty() || std::time::Instant::now() < deadline,
            "a gated plugin call never finished"
        );
    }
    let replies: Vec<_> = caller
        .drain_requests()
        .into_iter()
        .filter(|(method, params)| {
            method == tasty_host_plugin::protocol::METHOD_IPC_RESULT && params["call_id"] == 1
        })
        .collect();
    assert_eq!(replies.len(), 1, "게이트를 지난 호출의 회신: {replies:?}");

    assert!(app.gate_queue.is_empty());
    app.process_ipc();
    assert!(
        !app.plugin_calls_resumed,
        "재개할 호출이 없는데 표지가 섰다"
    );
}
