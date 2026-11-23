//! 비동기 번들 업데이트가 넘겨받은 자동 재시작 재시도를 완료 처리에서 어떻게 다루는지 확인한다.
//! 설치본이 바뀌지 않은 실패는 재시도를 되돌리고, 결과가 불확실하면 띄우지 않는다. 연결을 기다리던
//! 자동 재시작을 멈춘 교체도 같은 경로로 되돌린다.
//! `App` 을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop` 는 프로세스에 하나뿐이라,
//! 각 시험은 `crate::test_support::rerun_alone` 으로 새 프로세스에서 혼자 돈다.
use super::*;
use std::sync::Arc;
use std::time::Instant;
use tasty_host_plugin::BuiltinUpgradeItem;

const ID: &str = "com.example.upgrade-retry";

fn event_loop() -> winit::event_loop::EventLoop<crate::AppEvent> {
    crate::x11_test_loop::x11_event_loop()
}

/// 재시도할 때 실행 파일이 없어 기동이 곧바로 실패하는 plugin 하나를 둔 App.
fn app(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    plugin_dir: &std::path::Path,
) -> App {
    app_with_entry(event_loop, plugin_dir, "missing-plugin-bin")
}

/// `command` 를 실행 파일로 쓰는 plugin 하나를 둔 App.
fn app_with_entry(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    plugin_dir: &std::path::Path,
    command: &str,
) -> App {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    let mut mgr = crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let manifest = toml::from_str(&format!(
        "manifest_version = 1\nid = \"{ID}\"\nname = \"Retry\"\nversion = \"1.0.0\"\n\
         api_version = \"1\"\n[entry]\ntype = \"process\"\ncommand = \"{command}\"\n"
    ))
    .unwrap();
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: plugin_dir.to_path_buf(),
        manifest,
    }]);
    app.plugin_manager = Some(mgr);
    app
}

fn mgr(app: &mut App) -> &mut crate::plugin::PluginManager {
    app.plugin_manager.as_mut().unwrap()
}

/// 재시도가 한 번 실패해 이미 지난 시각에 다음 시도를 기다리는 중 교체를 시작한다.
fn begin_swap(app: &mut App, due: Instant) -> FileSwap {
    mgr(app).await_respawn_retry_for_test(ID, 1, due);
    let swap = mgr(app).begin_file_swap(ID, true);
    assert!(swap.restart && swap.retry.is_some());
    assert_eq!(mgr(app).respawn_retry_for_test(ID), None);
    swap
}

fn written(app: &mut App, changed: bool, uncertain: bool) -> Written {
    Written {
        item: BuiltinUpgradeItem {
            id: ID.into(),
            action: BuiltinUpgradeAction::Failed {
                reason: "injected".into(),
            },
        },
        package: None,
        config: mgr(app).config.clone(),
        changed,
        uncertain,
    }
}

fn upgrade(retry: TakenRetry) -> Upgrade {
    Upgrade {
        entries: VecDeque::new(),
        items: Vec::new(),
        restart: true,
        applied: false,
        held: Some((ID.into(), retry)),
        finish: Box::new(|_, _| {}),
    }
}

/// 되돌린 재시도가 원래의 실패 수와 시각을 갖고, 다음 tick에서 실제로 다시 띄워지는지 본다.
/// 실행 파일이 없어 그 기동이 실패하므로 실패 수가 하나 늘고 다음 재시도가 잡힌다.
fn assert_restored_and_retried(app: &mut App, due: Instant) {
    assert_eq!(
        mgr(app).respawn_retry_for_test(ID),
        Some((1, Some(due))),
        "설치본이 그대로인 실패에서 넘겨받은 재시도를 되돌리지 않았다"
    );
    mgr(app).pump(Instant::now());
    let (failures, next) = mgr(app).respawn_retry_for_test(ID).expect("재시도 기록");
    assert_eq!(failures, 2, "되돌린 재시도를 다시 띄우지 않았다");
    assert!(next.is_some_and(|next| next > due));
}

#[test]
fn a_validation_failure_before_the_write_restores_the_taken_retry() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_validation_failure_before_the_write_restores_the_taken_retry",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let dir = tempfile::tempdir().unwrap();
    let event_loop = event_loop();
    let mut app = app(&event_loop, dir.path());
    let due = Instant::now();
    let swap = begin_swap(&mut app, due);
    let written = written(&mut app, false, false);
    app.apply_builtin_written(written, swap.swapping, swap.restart, swap.retry);
    assert_restored_and_retried(&mut app, due);
}

#[test]
fn a_failed_file_operation_restores_the_taken_retry() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_failed_file_operation_restores_the_taken_retry",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let dir = tempfile::tempdir().unwrap();
    let event_loop = event_loop();
    let mut app = app(&event_loop, dir.path());
    let due = Instant::now();
    let swap = begin_swap(&mut app, due);
    upgrade(swap.retry.unwrap()).fail(&mut app, Failure::before("injected"));
    assert_restored_and_retried(&mut app, due);
}

/// 교체를 기동 없이 끝내는 완료 처리 하나.
type Settle<'a> = dyn Fn(&mut App, FileSwap) + 'a;

#[test]
fn an_uncertain_outcome_neither_restores_nor_starts() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "an_uncertain_outcome_neither_restores_nor_starts",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let dir = tempfile::tempdir().unwrap();
    let event_loop = event_loop();
    let mut app = app(&event_loop, dir.path());
    let mut unknown = Failure::before("injected");
    unknown.saved = None;
    let paths: [&Settle<'_>; 2] = [
        &|app, swap| {
            let written = written(app, true, true);
            app.apply_builtin_written(written, swap.swapping, swap.restart, swap.retry);
        },
        &|app, swap| upgrade(swap.retry.unwrap()).fail(app, unknown.clone()),
    ];
    for (path, settle) in paths.iter().enumerate() {
        let swap = begin_swap(&mut app, Instant::now());
        settle(&mut app, swap);
        mgr(&mut app).pump(Instant::now());
        assert_eq!(
            mgr(&mut app).respawn_retry_for_test(ID),
            None,
            "결과가 불확실한 교체 뒤 재시도를 되돌렸다 (경로 {path})"
        );
        assert!(!mgr(&mut app).is_running(ID), "경로 {path}");
    }
}

/// 연결을 기다리는 자동 재시작을 `--restart-running` 교체가 멈추고 넘겨받았다가 쓰기 전 검증에서
/// 실패하면, 그 재시도를 실패 수 그대로 되돌려 다음 tick에서 다시 띄운다.
#[test]
fn a_restart_running_swap_of_a_connecting_restart_restores_it_on_validation_failure() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_restart_running_swap_of_a_connecting_restart_restores_it_on_validation_failure",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let dir = tempfile::tempdir().unwrap();
    // 연결하지 않고 기다리기만 한다.
    tasty_test_support::write_executable(
        &dir.path().join("entry.sh"),
        "#!/bin/sh\nexec sleep 30\n",
    )
    .unwrap();
    let event_loop = event_loop();
    let mut app = app_with_entry(&event_loop, dir.path(), "entry.sh");
    let due = Instant::now();
    mgr(&mut app).await_respawn_retry_for_test(ID, 1, due);
    mgr(&mut app).pump(due);
    assert!(
        mgr(&mut app).is_connecting(ID),
        "재시도가 띄운 기동이 연결을 기다리지 않는다"
    );
    assert_eq!(mgr(&mut app).respawn_retry_for_test(ID), Some((1, None)));

    let before = Instant::now();
    let swap = mgr(&mut app).begin_file_swap(ID, true);
    assert!(
        swap.swapping && swap.restart,
        "교체가 연결 중인 재시작을 멈추지 않았다"
    );
    assert_eq!(mgr(&mut app).respawn_retry_for_test(ID), None);
    assert!(
        !mgr(&mut app).wait_retired(ID),
        "교체가 멈춘 프로세스에 회수 뒤 기동이 걸렸다"
    );
    let written = written(&mut app, false, false);
    app.apply_builtin_written(written, swap.swapping, swap.restart, swap.retry);

    let (failures, restored) = mgr(&mut app)
        .respawn_retry_for_test(ID)
        .expect("연결 대기 중 넘겨받은 재시도를 되돌리지 않았다");
    assert_eq!(failures, 1, "되돌린 재시도가 실패 수를 잃었다");
    assert!(restored.is_some_and(|at| before <= at && at <= Instant::now()));
    assert!(!mgr(&mut app).is_running(ID));
    mgr(&mut app).pump(Instant::now());
    assert!(
        mgr(&mut app).is_running(ID),
        "되돌린 재시도를 다음 tick에 띄우지 않았다"
    );

    mgr(&mut app).begin_shutdown_all();
    let give_up = Instant::now() + std::time::Duration::from_secs(10);
    // poll 은 끝난 자식을 거두고 원래도 받던 기한 지난 강제 종료만 해 새 시도가 없다. 마지막 읽기로 쓴다.
    assert!(
        tasty_utils::test_wait::holds_by_every(
            give_up,
            std::time::Duration::from_millis(20),
            |_| mgr(&mut app).poll_shutdown_all()
        ),
        "종료가 10 초 안에 안 끝났다"
    );
}
