use std::sync::Arc;

use tasty_host_plugin::manager::{FailureKind, FailureReport, PluginFailure};
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::*;

fn manager_with(reports: Vec<FailureReport>) -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    for r in reports {
        mgr.push_failure_report_for_tests(r);
    }
    mgr
}

fn failure(kind: FailureKind) -> PluginFailure {
    PluginFailure {
        kind,
        message: "plugin did not connect within 10s".into(),
        generation: 4,
    }
}

/// 한 번의 실패는 경고, 자동 비활성화는 오류로 남고, 원인 문장과 plugin ID 가 본문에 실린다.
#[test]
fn failures_become_a_warning_and_auto_disable_an_error_with_the_cause() {
    crate::i18n::init("en");
    let mut mgr = manager_with(vec![
        FailureReport::Failed {
            plugin_id: "com.example.a".into(),
            failure: failure(FailureKind::ConnectFailed),
        },
        FailureReport::AutoDisabled {
            plugin_id: "com.example.b".into(),
            failure: failure(FailureKind::SpawnFailed),
            failures: 3,
        },
    ]);
    let mut session = crate::state::tests::test_state().1;
    let mut engine = session.borrow_mut();
    assert_eq!(report_plugin_failures(&mut mgr, &mut engine), 2);
    assert!(!mgr.has_failure_reports());

    let found: Vec<_> = engine
        .live
        .notifications
        .all()
        .map(|n| (n.level, n.title.clone(), n.body.clone()))
        .collect();
    let warning = found
        .iter()
        .find(|n| n.0 == NotificationLevel::Warning)
        .expect("warning");
    assert!(warning.1.contains("com.example.a"), "{warning:?}");
    assert!(
        warning.2.contains("did not connect within 10s"),
        "{warning:?}"
    );
    let error = found
        .iter()
        .find(|n| n.0 == NotificationLevel::Error)
        .expect("error");
    assert!(
        error.1.contains("com.example.b") && error.2.contains('3'),
        "{error:?}"
    );
    let created = engine
        .take_pending_host_events()
        .into_iter()
        .filter(|e| {
            matches!(e, crate::state::PendingHostEvent::NotificationCreated { source, .. } if source == "plugin")
        })
        .count();
    assert_eq!(created, 2);
}

/// 알림을 남길 workspace 가 없으면 보고를 꺼내지 않아 다음 기회에 남긴다.
#[test]
fn reports_wait_until_a_workspace_exists() {
    let mut mgr = manager_with(vec![FailureReport::Failed {
        plugin_id: "com.example.a".into(),
        failure: failure(FailureKind::SpawnFailed),
    }]);
    let mut session = crate::state::tests::test_state().1;
    let mut engine = session.borrow_mut();
    assert_eq!(deliver(&mut mgr, &mut engine, None), 0);
    assert!(mgr.has_failure_reports());
    assert_eq!(engine.live.notifications.all().len(), 0);
}

/// 두 plugin 이 같은 레벨로 잇달아 실패하고 그중 하나가 자동 비활성화돼도 세 사건이 각각 남는다.
#[test]
fn simultaneous_failures_of_different_plugins_stay_separate() {
    crate::i18n::init("en");
    let mut mgr = manager_with(vec![
        FailureReport::Failed {
            plugin_id: "com.example.a".into(),
            failure: failure(FailureKind::ConnectFailed),
        },
        FailureReport::Failed {
            plugin_id: "com.example.b".into(),
            failure: failure(FailureKind::SpawnFailed),
        },
        FailureReport::AutoDisabled {
            plugin_id: "com.example.b".into(),
            failure: failure(FailureKind::SpawnFailed),
            failures: 3,
        },
    ]);
    let mut session = crate::state::tests::test_state().1;
    let mut engine = session.borrow_mut();
    assert_eq!(report_plugin_failures(&mut mgr, &mut engine), 3);
    let titles: Vec<_> = engine
        .live
        .notifications
        .all()
        .map(|n| (n.level, n.title.clone()))
        .collect();
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert!(titles[0].1.contains("com.example.a") && titles[1].1.contains("com.example.b"));
    assert_eq!(titles[2].0, NotificationLevel::Error);
}

/// 공식 목록에 없는 `com.tasty.*` 폴더는 예약 id 문구의 경고 알림으로 남는다. 기동 실패 제목이나
/// 판정 원문을 쓰지 않는다. 시작을 시도하지 않았기 때문이다.
#[test]
fn a_reserved_id_refusal_uses_the_reserved_copy() {
    crate::i18n::init("en");
    let mut mgr = manager_with(vec![FailureReport::ReservedId {
        plugin_id: "com.tasty.fake".into(),
    }]);
    let mut session = crate::state::tests::test_state().1;
    let mut engine = session.borrow_mut();
    assert_eq!(report_plugin_failures(&mut mgr, &mut engine), 1);
    let found: Vec<_> = engine
        .live
        .notifications
        .all()
        .map(|n| (n.level, n.title.clone(), n.body.clone()))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, NotificationLevel::Warning);
    assert_eq!(
        found[0].1,
        crate::i18n::t_fmt("plugin_notice.reserved_id_title", "com.tasty.fake")
    );
    assert_eq!(found[0].2, crate::i18n::t("plugin_notice.reserved_id_body"));
    assert!(!found[0].1.contains("did not start"), "{found:?}");
}
