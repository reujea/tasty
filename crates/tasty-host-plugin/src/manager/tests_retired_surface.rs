//! 원 프로세스가 회수된 뒤 그 프로세스가 만든 surface 를 닫으면 회수를 소멸 증거로 쓴다.

use std::sync::Arc;

use serde_json::json;
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::PluginManager;
use crate::host_cmd::{HostCmd, SurfaceHandles};
use crate::process::PluginProcess;

const PLUGIN: &str = "com.test.document";

fn handles() -> SurfaceHandles {
    SurfaceHandles {
        display_name: Arc::default(),
        snapshot_cache: Arc::default(),
    }
}

/// 실행 중인 stub 프로세스와 그 프로세스에 게시된 surface 7.
fn manager_with_surface() -> (PluginManager, SurfaceHandles) {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let (process, _requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), process);
    let surface = handles();
    mgr.host_cmd_tx
        .send(HostCmd::RemoteSurfaceCreated {
            surface_id: 7,
            plugin_id: PLUGIN.into(),
            kind: "document".into(),
            cwd: None,
            params: json!({}),
            handles: surface.clone(),
        })
        .unwrap();
    mgr.drain_host_cmds();
    (mgr, surface)
}

fn retire(mgr: &mut PluginManager) {
    let process = mgr.processes.remove(PLUGIN).expect("running stub");
    mgr.retire_process(PLUGIN, process, false);
}

fn close(
    mgr: &mut PluginManager,
    surface: &SurfaceHandles,
) -> crate::host_cmd::RemoteRetirementReceipt {
    let receipt = mgr
        .enqueue_observed_remote_retirement(7, surface.binding())
        .unwrap();
    mgr.drain_host_cmds();
    receipt
}

#[test]
fn closing_after_the_original_process_was_reaped_succeeds() {
    let (mut mgr, surface) = manager_with_surface();
    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    let receipt = close(&mut mgr, &surface);
    assert_eq!(receipt.observation(), Some(Ok(())));
}

#[test]
fn closing_after_a_new_process_started_still_uses_the_old_generation() {
    let (mut mgr, surface) = manager_with_surface();
    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    let (fresh, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), fresh);
    let receipt = close(&mut mgr, &surface);
    assert_eq!(receipt.observation(), Some(Ok(())));
    assert!(
        requests.try_recv().is_err(),
        "새 프로세스는 옛 surface 를 모르므로 파괴 요청을 받지 않는다"
    );
}

#[test]
fn closing_while_the_original_process_is_retiring_settles_at_the_reap() {
    let (mut mgr, surface) = manager_with_surface();
    retire(&mut mgr);
    let receipt = close(&mut mgr, &surface);
    assert_eq!(receipt.observation(), None, "회수 전에는 확정하지 않는다");
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    assert_eq!(receipt.observation(), Some(Ok(())));
}

#[test]
fn a_process_that_vanished_without_a_retirement_stays_unconfirmed() {
    let (mut mgr, surface) = manager_with_surface();
    drop(mgr.processes.remove(PLUGIN));
    let receipt = close(&mut mgr, &surface);
    assert_eq!(
        receipt.observation(),
        Some(Err(
            "original plugin process is unavailable for destruction".into()
        ))
    );
}

/// 닫기 정리 중에는 pump 가 돌지 않고 `poll_publication_retirements` 만 돈다. 그 폴링만으로
/// 회수 완료가 확정되고, 다시 띄우기는 Retire 틱으로 미뤄진다.
#[test]
fn closing_while_retiring_settles_through_the_paused_publication_poll() {
    let (mut mgr, surface) = manager_with_surface();
    let process = mgr.processes.remove(PLUGIN).expect("running stub");
    mgr.retire_process(PLUGIN, process, true);
    let receipt = mgr
        .enqueue_observed_remote_retirement(7, surface.binding())
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // holds_by 로 바꾸지 않는다. 퇴역 응답은 poll 이 처리해야 receipt 에 남으므로 기한 뒤 poll 없이 읽는
    // 관측으로는 완료를 볼 수 없다. 기한 검사는 이미 읽기 뒤·poll 앞에 있다.
    while receipt.observation().is_none() && std::time::Instant::now() < deadline {
        mgr.poll_publication_retirements().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(receipt.observation(), Some(Ok(())));
    assert_eq!(
        mgr.retiring_respawn(PLUGIN),
        Some(true),
        "관측이 멈춘 동안에는 다시 띄우지 않고 Retire 틱에 넘긴다"
    );
    assert!(mgr.wait_retired(PLUGIN), "재시작 예약이 남아 있다");
}

fn retire_and_reap(mgr: &mut PluginManager) -> std::sync::Weak<()> {
    let process = mgr.processes.remove(PLUGIN).expect("running stub");
    let generation = process.reply_binding();
    mgr.retire_process(PLUGIN, process, false);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    generation
}

fn holds(mgr: &PluginManager, generation: &std::sync::Weak<()>) -> bool {
    mgr.reaped_generations.iter().any(|g| g.ptr_eq(generation))
}

#[test]
fn reaped_generations_keep_only_generations_a_surface_still_points_at() {
    let (mut mgr, surface) = manager_with_surface();
    let first = retire_and_reap(&mut mgr);
    mgr.processes
        .insert(PLUGIN.into(), PluginProcess::stub_with_request_rx(PLUGIN).0);
    let second = retire_and_reap(&mut mgr);
    mgr.processes
        .insert(PLUGIN.into(), PluginProcess::stub_with_request_rx(PLUGIN).0);
    let third = retire_and_reap(&mut mgr);
    assert!(holds(&mgr, &first), "surface 7 이 가리키는 세대는 남는다");
    assert!(!holds(&mgr, &second), "아무도 가리키지 않는 세대는 지운다");
    assert!(
        holds(&mgr, &third),
        "방금 회수한 세대는 다음 회수까지 남는다"
    );
    let receipt = close(&mut mgr, &surface);
    assert_eq!(receipt.observation(), Some(Ok(())));
    mgr.processes
        .insert(PLUGIN.into(), PluginProcess::stub_with_request_rx(PLUGIN).0);
    retire_and_reap(&mut mgr);
    assert!(
        !holds(&mgr, &first),
        "닫힌 surface 의 세대는 다음 회수 때 지운다"
    );
    assert_eq!(mgr.reaped_generations.len(), 1);
}

#[test]
fn reaped_generations_keep_a_generation_a_mesh_bootstrap_points_at() {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let (process, _requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), process);
    let mesh = crate::host_cmd::MeshBinding::default();
    mgr.send_egui_mesh_surface_create(PLUGIN, 9, "canvas", None, "canvas", &mesh);
    let context = tasty_plugin_protocol::SurfaceSetContextParams {
        surface_id: 9,
        ..Default::default()
    };
    mgr.send_surface_set_context(PLUGIN, &context);
    let meshed = retire_and_reap(&mut mgr);
    mgr.processes
        .insert(PLUGIN.into(), PluginProcess::stub_with_request_rx(PLUGIN).0);
    retire_and_reap(&mut mgr);
    assert!(
        holds(&mgr, &meshed),
        "mesh bootstrap 이 가리키는 세대는 남는다"
    );
    drop(mesh);
    mgr.processes
        .insert(PLUGIN.into(), PluginProcess::stub_with_request_rx(PLUGIN).0);
    retire_and_reap(&mut mgr);
    assert!(!holds(&mgr, &meshed), "bootstrap 이 사라지면 지운다");
}

/// 요청 수신단을 살려 둔 채 surface 7 을 게시한다. 파괴 요청이 실제로 원 프로세스 큐에 들어간다.
fn manager_with_listening_surface() -> (PluginManager, SurfaceHandles, crate::process::RequestTap) {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let (process, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), process);
    let surface = handles();
    mgr.host_cmd_tx
        .send(HostCmd::RemoteSurfaceCreated {
            surface_id: 7,
            plugin_id: PLUGIN.into(),
            kind: "document".into(),
            cwd: None,
            params: json!({}),
            handles: surface.clone(),
        })
        .unwrap();
    mgr.drain_host_cmds();
    (mgr, surface, requests)
}

/// 살아 있는 원 프로세스에 파괴 요청을 보낸 뒤 그 프로세스를 회수한다(먼저 닫고 뒤에 disable).
fn close_then_retire(
    mgr: &mut PluginManager,
    surface: &SurfaceHandles,
) -> crate::host_cmd::RemoteRetirementReceipt {
    let receipt = close(mgr, surface);
    assert_eq!(receipt.observation(), None, "원 프로세스의 응답을 기다린다");
    let process = mgr.processes.remove(PLUGIN).expect("running stub");
    mgr.retire_process(PLUGIN, process, false);
    receipt
}

#[test]
fn a_sent_destroy_settles_when_disable_retires_its_process() {
    let (mut mgr, surface, _tap) = manager_with_listening_surface();
    let receipt = close_then_retire(&mut mgr, &surface);
    mgr.cancel_pending_namespace_calls(PLUGIN, "plugin disabled");
    assert_eq!(receipt.observation(), None, "회수가 끝날 때 확정한다");
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    assert_eq!(receipt.observation(), Some(Ok(())));
}

#[test]
fn a_sent_destroy_settles_through_the_paused_poll_after_its_process_retired() {
    let (mut mgr, surface, _tap) = manager_with_listening_surface();
    let receipt = close_then_retire(&mut mgr, &surface);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // holds_by 로 바꾸지 않는다. 퇴역 응답은 poll 이 처리해야 receipt 에 남으므로 기한 뒤 poll 없이 읽는
    // 관측으로는 완료를 볼 수 없다. 기한 검사는 이미 읽기 뒤·poll 앞에 있다.
    while receipt.observation().is_none() && std::time::Instant::now() < deadline {
        mgr.poll_publication_retirements().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(receipt.observation(), Some(Ok(())));
}

#[test]
fn a_lost_destroy_of_a_process_that_was_not_retired_stays_a_failure() {
    let (mut mgr, surface, _tap) = manager_with_listening_surface();
    let receipt = close(&mut mgr, &surface);
    mgr.cancel_pending_namespace_calls(PLUGIN, "plugin crashed");
    assert_eq!(
        receipt.observation(),
        Some(Err(
            "plugin 'com.test.document' unavailable: plugin crashed".into()
        ))
    );
}

/// 닫기 정리가 receipt 대기로 넘어가면 관측이 재개된다. 재개 첫 턴은 보류한 enable 을 pump
/// 보다 먼저 실행할 수 있으므로, 회수 게시를 먼저 적용해 둔 surface 는 새 프로세스에 다시
/// 게시되지 않아야 한다.
#[test]
fn an_applied_retirement_is_not_republished_to_a_restarted_process() {
    for applied in [true, false] {
        let (mut mgr, surface) = manager_with_surface();
        retire(&mut mgr);
        assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
        let _receipt = mgr
            .enqueue_observed_remote_retirement(7, surface.binding())
            .unwrap();
        if applied {
            mgr.poll_publication_retirements().unwrap();
        }
        let (fresh, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
        mgr.processes.insert(PLUGIN.into(), fresh);
        mgr.reattach_orphan_surfaces(PLUGIN);
        assert_eq!(
            requests.try_recv().is_ok(),
            !applied,
            "회수 게시를 적용했으면 재게시하지 않고, 적용 전이면 재게시한다(대조)"
        );
    }
}

/// 닫기는 접수 뒤 대상을 해석하기까지 시간이 걸린다. 그 사이 새 프로세스가 떠도 닫기가
/// 회수할 수 있는 surface 는 다시 게시하지 않고, 닫기가 그 surface 를 남긴 채 끝나면 게시한다.
#[test]
fn a_closing_surface_waits_for_its_close_before_republishing() {
    let (mut mgr, _surface) = manager_with_surface();
    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    mgr.set_closing_surfaces([7].into());
    let (fresh, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), fresh);
    mgr.reattach_orphan_surfaces(PLUGIN);
    assert!(
        requests.try_recv().is_err(),
        "닫는 중인 surface 는 보내지 않는다"
    );
    mgr.set_closing_surfaces(Default::default());
    let restored = requests
        .try_recv()
        .expect("닫기가 남긴 surface 는 게시한다");
    assert_eq!(restored.params["surface_id"], 7);
    assert!(requests.try_recv().is_err(), "한 번만 보낸다");
}

#[test]
fn a_released_surface_that_its_close_retired_is_not_republished() {
    let (mut mgr, surface) = manager_with_surface();
    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    mgr.set_closing_surfaces([7].into());
    let (fresh, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), fresh);
    mgr.reattach_orphan_surfaces(PLUGIN);
    let receipt = close(&mut mgr, &surface);
    assert_eq!(
        receipt.observation(),
        Some(Ok(())),
        "옛 세대 회수로 확정한다"
    );
    mgr.set_closing_surfaces(Default::default());
    assert!(
        requests.try_recv().is_err(),
        "회수된 surface 는 게시하지 않는다"
    );
}

/// 닫기 접수보다 먼저 처리된 enable 은 살아 있는 surface 를 새 프로세스에 게시한다. 뒤이은 닫기는
/// 옛 세대 회수가 아니라 새 프로세스의 인스턴스를 파괴해야 하고, 그 응답으로 확정한다.
#[test]
fn a_close_after_republication_destroys_the_instance_in_the_new_process() {
    let (mut mgr, surface) = manager_with_surface();
    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    let (fresh, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), fresh);
    mgr.reattach_orphan_surfaces(PLUGIN);
    let published = requests.try_recv().expect("닫기 전 enable 은 게시한다");
    assert_eq!(published.params["surface_id"], 7);

    let receipt = close(&mut mgr, &surface);
    let destroy = requests
        .try_recv()
        .expect("새 프로세스에 게시된 인스턴스를 파괴한다");
    assert_eq!(
        destroy.method,
        tasty_plugin_protocol::METHOD_SURFACE_DESTROY
    );
    assert_eq!(destroy.params["surface_id"], 7);
    assert_eq!(
        receipt.observation(),
        None,
        "옛 세대 회수로 확정하지 않고 새 프로세스의 응답을 기다린다"
    );
    mgr.handle_plugin_response(
        PLUGIN,
        tasty_plugin_protocol::PluginResponse {
            id: destroy.id,
            result: Some(serde_json::Value::Null),
            error: None,
            error_code: None,
        },
    );
    assert_eq!(receipt.observation(), Some(Ok(())));

    retire(&mut mgr);
    assert!(!mgr.wait_retired(PLUGIN), "재시작 예약이 없는 회수다");
    let (again, requests) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), again);
    mgr.reattach_orphan_surfaces(PLUGIN);
    assert!(
        requests.try_recv().is_err(),
        "닫힌 surface 는 다시 게시하지 않는다"
    );
}
