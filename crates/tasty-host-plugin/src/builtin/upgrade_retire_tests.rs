//! `upgrade-builtins` 가 회수 중인 plugin 의 재기동 예약을 잇는다는 것.
//!
//! 무응답 재시작은 옛 프로세스를 뒤에서 회수하고 회수가 끝나면 다시 띄운다
//! (`manager::retire`). 그 사이 `upgrade-builtins` 가 그 디렉토리에 쓰려고 회수를 기다리면
//! 예약은 회수 기록과 함께 사라진다 — 그것을 이어받지 않으면 enabled 인 plugin 이 꺼진 채
//! 남는다.
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

/// 설치본 `1.<installed>.0` 을 번들 `1.<bundle>.0` 으로 맞춘다. 돌려주는 것은 탄 갈래의 보고다.
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
        bin_name: "",
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

/// 재시작으로 회수 중인 plugin 에 **쓰는** upgrade 가 오면, 회수를 기다려 쓰고 그 뒤에 결국
/// 다시 뜬다 — 기다리며 가져온 재기동 예약을 버리지 않는다.
///
/// 쓰는 갈래 셋(같은 버전에 바뀐 내용 · 버전이 오름 · `--force`)은 예약을 각자 받아 끝에서
/// 한 줄로 잇는다. 갈래마다 받는 대입이 따로 빠질 수 있으므로 셋을 모두 태운다 — 보고의
/// 종류로 그 갈래를 탔는지 함께 단정한다.
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

/// 쓸 것이 없는 upgrade(같은 버전·같은 내용 / 설치본이 더 높음)는 회수를 기다리지 않는다.
/// 회수는 뒤에서 이어지고, 재시작 예약도 그대로 남아 결국 다시 뜬다.
///
/// "기다리지 않았다" 는 시계가 아니라 **회수 기록**으로 판정한다. 기다리는 길
/// (`wait_retired`)은 기록을 가져가고, 기다리지 않으면 기록은 pump 가 거둘 때까지 남는다 —
/// upgrade 와 단정 사이에 pump 가 없으므로 회수가 뒤에서 이미 끝났어도 기록은 그대로다.
/// 그래서 부하가 upgrade 를 아무리 늦춰도 판정이 안 바뀐다. 예전의 벽시계 단언(500 ms)은
/// 이 호출이 실제로 수 µs~수백 µs 인데도 그 차이를 시계에 맡겼다.
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
