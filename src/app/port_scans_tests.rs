//! 포트 스캔 폴링이 요청 없이 EventLoopProxy를 clone하지 않는지 잰다. clone은 macOS·Wayland에서 메인 루프를
//! 깨우지만 시험 루프(X11)에서는 깨우지 않으므로 깨움 수 대신 worker용 clone 수를 센다. 창이 없는 App이라
//! MainView 슬롯의 요청 처리는 지나지 않는다. 요청을 꺼낸 뒤에만 clone한다는 순서는 source guard가 확인한다.
//! `App`을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop`는 프로세스에 하나뿐이라 새 프로세스에서 혼자 돈다.

use super::*;
use crate::app::App;
use winit::event_loop::EventLoop;

fn clones() -> usize {
    WORKER_WAKE_CLONES.with(std::cell::Cell::get)
}

fn event_loop() -> EventLoop<crate::AppEvent> {
    crate::x11_test_loop::x11_event_loop()
}

/// Given 스캔 요청이 없는 App, When 폴링을 여러 번 하면, Then worker용 proxy clone은 0회다.
/// Given 스캔 worker 시작, When worker를 만들면, Then clone은 1회이고 결과가 채널로 돌아온다.
#[test]
fn polling_without_a_request_clones_no_proxy() {
    if crate::test_support::rerun_alone(module_path!(), "polling_without_a_request_clones_no_proxy")
    {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut app: App = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    let before = clones();
    for _ in 0..64 {
        app.poll_port_scans();
    }
    assert_eq!(clones() - before, 0, "요청이 없는 폴링이 proxy를 clone했다");

    let snapshot = ScanSnapshot {
        surfaces: Vec::new(),
        show_all_system: false,
    };
    let (result, worker) = spawn_scan_worker(&app.view.proxy, snapshot).unwrap();
    assert_eq!(clones() - before, 1, "worker 하나에 clone 하나");
    worker.join().unwrap();
    assert!(result.try_recv().unwrap().is_ok());
}
