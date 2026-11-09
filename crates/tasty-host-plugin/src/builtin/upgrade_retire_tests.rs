//! 번들 업데이트가 프로세스 회수를 기다린 뒤에도 재시작 예약을 유지하는지 확인한다.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{BuiltinSpec, BuiltinUpgradeAction, apply_builtin_upgrade_decision};
use crate::manager::PluginManager;
use crate::process::PluginProcess;
use crate::test_fake_plugin::{self as fake, ID};
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;

/// 무응답 재시작을 걸어 회수 중으로 만든 manager 와, 번들(src)·설치본(dest) 경로.
fn restarting(home: &HomeEnvGuard) -> (PluginManager, std::path::PathBuf, std::path::PathBuf) {
    let src = home.path().join("bundle");
    let dest = home.path().join("installed");
    fake::write(&src);
    fake::write(&dest);

    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![fake::package(&dest)]);
    let stalled = Command::new("sleep").arg("30").spawn().expect("child");
    let proc = PluginProcess::stub_with_child(ID, stalled);
    proc.backdate_pong_for_test(Duration::from_secs(120));
    mgr.processes.insert(ID.into(), proc);
    mgr.restart_unresponsive_for_test();
    assert_eq!(
        mgr.retiring_respawn(ID),
        Some(true),
        "재시작이 예약돼야 한다"
    );
    (mgr, src, dest)
}

/// 설치본과 번들 버전을 지정해 업데이트하고 처리 결과를 반환한다.
fn upgrade(
    mgr: &mut PluginManager,
    src: &Path,
    dest: &Path,
    installed: u64,
    bundle: u64,
    force: bool,
) -> BuiltinUpgradeAction {
    let spec = BuiltinSpec {
        id: ID,
        crate_dir: "",
    };
    apply_builtin_upgrade_decision(
        mgr,
        &spec,
        src,
        dest,
        Some(semver::Version::new(1, installed, 0)),
        Some(semver::Version::new(1, bundle, 0)),
        force,
        false,
    )
    .item
    .action
}

fn pump_until_running(mgr: &mut PluginManager) {
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.is_running(ID) {
        assert!(
            Instant::now() < give_up,
            "upgrade 뒤 재시작 중이던 plugin 이 다시 뜨지 않았다"
        );
        mgr.pump(Instant::now());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn shut_down(mgr: &mut PluginManager) {
    mgr.begin_shutdown_all();
    let give_up = Instant::now() + Duration::from_secs(10);
    while !mgr.poll_shutdown_all() {
        assert!(Instant::now() < give_up, "종료가 10 초 안에 안 끝났다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// (이름, 설치본 minor, 번들 minor, `--force`, 그 갈래를 탔는가).
type Case = (
    &'static str,
    u64,
    u64,
    bool,
    fn(&BuiltinUpgradeAction) -> bool,
);

/// 같은 버전의 내용 변경, 버전 증가, 강제 업데이트가 모두 재시작 예약을 보존하는지 확인한다.
#[test]
fn an_upgrade_during_a_restart_keeps_the_restart() {
    let cases: [Case; 3] = [
        ("same version, changed content", 0, 0, false, |a| {
            matches!(a, BuiltinUpgradeAction::Skipped { .. })
        }),
        ("version upgrade", 0, 1, false, |a| {
            matches!(a, BuiltinUpgradeAction::Upgraded { .. })
        }),
        ("force overwrite", 0, 0, true, |a| {
            matches!(a, BuiltinUpgradeAction::Reinstalled { .. })
        }),
    ];
    for (label, installed, bundle, force, took_branch) in cases {
        let home = HomeEnvGuard::tasty_home();
        let (mut mgr, src, dest) = restarting(&home);
        std::fs::write(src.join("new-asset"), "x").expect("bundle change");

        let action = upgrade(&mut mgr, &src, &dest, installed, bundle, force);
        assert!(
            took_branch(&action),
            "{label}: 다른 갈래를 탔다: {action:?}"
        );
        assert_eq!(
            mgr.retiring_count(),
            0,
            "{label}: 쓰는 갈래는 회수를 끝까지 기다린다"
        );
        assert!(
            dest.join("new-asset").exists(),
            "{label}: 바뀐 내용이 안 쓰였다"
        );
        pump_until_running(&mut mgr);
        shut_down(&mut mgr);
    }
}

/// 쓸 파일이 없으면 회수를 기다리지 않고 재시작 예약을 유지한다.
/// pump를 호출하기 전 회수 기록이 남아 있는지 검사해 시간 측정에 의존하지 않는다.
#[test]
fn an_upgrade_that_writes_nothing_does_not_wait_for_a_retirement() {
    for (installed, bundle) in [(0, 0), (1, 0)] {
        let home = HomeEnvGuard::tasty_home();
        let (mut mgr, src, dest) = restarting(&home);

        upgrade(&mut mgr, &src, &dest, installed, bundle, false);
        assert_eq!(
            mgr.retiring_count(),
            1,
            "쓸 것이 없는 upgrade(v1.{installed} ← v1.{bundle})가 회수를 기다렸다 — 회수 기록이 \
             pump 없이 사라졌다"
        );
        assert_eq!(
            mgr.retiring_respawn(ID),
            Some(true),
            "재시작 예약이 사라졌다"
        );
        pump_until_running(&mut mgr);
        shut_down(&mut mgr);
    }
}
