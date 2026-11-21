//! 완료 전략의 병합·namespace·참조·기본 메서드 선택과 이름 해석을 검사한다.

use super::*;
use crate::completion_strategy::types::CompletionStrategyId;
use crate::hook_handler::types::{
    HookHandler, HookHandlerAction, HookHandlerId, HookHandlerOwner, HookSource,
};
use crate::namespace_table_for_tests::installed_test_table;

fn poll_toml(id: &str, priority: i32, method: &str, default_for: &str) -> String {
    format!(
        r#"
        [[strategy]]
        id = "{id}"
        priority = {priority}
        default_for_methods = [{default_for}]
        [strategy.spec]
        kind = "poll"
        poll_method = "{method}"
        state_field = "state"
        terminal_states = ["idle", "needs_input"]
        "#
    )
}

/// 전역 훅 레지스트리를 함께 쓰므로 다른 시험과 겹치지 않는 ID를 사용한다.
fn ensure_hook_handler(id: &str, owner: HookHandlerOwner) {
    crate::hook_handler::global()
        .upsert_full_handler(HookHandler {
            id: HookHandlerId::new(id),
            source: HookSource::Hook,
            priority: 100,
            owner,
            action: HookHandlerAction::IpcSequence { calls: vec![] },
            display_name_i18n_key: None,
            disabled: false,
        })
        .expect("test hook handler upsert");
}

#[test]
fn plugin_poll_strategy_installs_and_resolves() {
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&poll_toml(
        "spawn-wait",
        100,
        "acme.wait",
        r#""acme.spawn""#,
    ))
    .expect("parse");
    reg.install_plugin_strategies("acme", &decls);

    let id = CompletionStrategyId::new("acme/spawn-wait");
    let s = reg.get(&id).expect("strategy");
    assert_eq!(s.owner, CompletionStrategyOwner::Plugin("acme".into()));
    assert_eq!(s.priority, 100);
    assert_eq!(s.default_for_methods, vec!["acme.spawn".to_string()]);
    match &s.kind {
        CompletionStrategyKind::Poll(spec) => {
            assert_eq!(spec.poll_method, "acme.wait");
            assert_eq!(spec.interval_ms, 500);
        }
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }

    let spec = reg.resolve_poll_spec(&id).expect("resolve");
    assert_eq!(spec.poll_method, "acme.wait");

    let winner = reg
        .resolve_default_for_method("acme.spawn")
        .expect("default");
    assert_eq!(winner.id, id);
}

/// TOML에서 레지스트리 해석까지 map·scalar 필드가 보존되는지 확인한다.
/// 모든 필드 대응은 completion_strategy_to_poll_spec의 별도 시험이 검사한다.
#[test]
fn poll_decl_survives_registry_install_and_resolve() {
    let toml = r#"
        [[strategy]]
        id = "full-map"
        priority = 5
        [strategy.spec]
        kind = "poll"
        poll_method = "acme.wait"
        map_from_response = { child_index = "child_index" }
        state_field = "st"
        terminal_states = ["done"]
        timeout_ms = 9000
    "#;
    let decls = parse_strategy_section(toml).expect("parse");
    let reg = CompletionStrategyRegistry::new();
    reg.install_plugin_strategies("acme", &decls);
    let spec = reg
        .resolve_poll_spec(&CompletionStrategyId::new("acme/full-map"))
        .expect("resolve");
    assert_eq!(spec.poll_method, "acme.wait");
    assert_eq!(
        spec.map_from_response.get("child_index").unwrap(),
        "child_index"
    );
    assert_eq!(spec.timeout_ms, Some(9000));
}

#[test]
fn plugin_poll_method_outside_own_namespace_is_dropped() {
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&poll_toml("evil", 100, "other.wait", "")).expect("parse");
    reg.install_plugin_strategies("acme", &decls);
    assert!(reg.get(&CompletionStrategyId::new("acme/evil")).is_none());
}

#[test]
fn host_poll_method_inside_registered_plugin_prefix_is_dropped() {
    installed_test_table()
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .register("com.test.cstest", "cstest_acme")
        .expect("test prefix must be free");
    let reg = CompletionStrategyRegistry::new();
    reg.install_host_defaults(&poll_toml("h1", 100, "cstest_acme.wait", ""));
    assert!(reg.get(&CompletionStrategyId::new("host/h1")).is_none());
    installed_test_table()
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .unregister_plugin("com.test.cstest");
}

#[test]
fn host_poll_method_outside_any_plugin_prefix_is_kept() {
    let reg = CompletionStrategyRegistry::new();
    reg.install_host_defaults(&poll_toml("h2", 100, "terminal.child_state", ""));
    assert!(reg.get(&CompletionStrategyId::new("host/h2")).is_some());
}

fn push_toml(id: &str, notify_via: &str) -> String {
    format!(
        r#"
        [[strategy]]
        id = "{id}"
        priority = 100
        [strategy.spec]
        kind = "push"
        notify_via = "{notify_via}"
        timeout_ms = 5000
        "#
    )
}

#[test]
fn push_strategy_missing_hook_handler_is_dropped() {
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&push_toml("orphan", "acme/does-not-exist")).expect("parse");
    reg.install_plugin_strategies("acme", &decls);
    assert!(reg.get(&CompletionStrategyId::new("acme/orphan")).is_none());
}

#[test]
fn push_strategy_self_owned_hook_handler_resolves() {
    ensure_hook_handler(
        "cstest-plugin/notify-self",
        HookHandlerOwner::Plugin("cstest-plugin".into()),
    );
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&push_toml("notify-strategy", "cstest-plugin/notify-self"))
        .expect("parse");
    reg.install_plugin_strategies("cstest-plugin", &decls);
    let s = reg
        .get(&CompletionStrategyId::new("cstest-plugin/notify-strategy"))
        .expect("kept");
    match s.kind {
        CompletionStrategyKind::Push { timeout_ms, .. } => assert_eq!(timeout_ms, 5000),
        CompletionStrategyKind::Poll(_) => panic!("expected push"),
    }
}

#[test]
fn push_strategy_host_owned_hook_handler_resolves() {
    ensure_hook_handler("host/cstest-notify", HookHandlerOwner::Host);
    let reg = CompletionStrategyRegistry::new();
    let decls =
        parse_strategy_section(&push_toml("via-host", "host/cstest-notify")).expect("parse");
    reg.install_plugin_strategies("cstest-plugin2", &decls);
    assert!(
        reg.get(&CompletionStrategyId::new("cstest-plugin2/via-host"))
            .is_some()
    );
}

#[test]
fn push_strategy_other_plugin_owned_hook_handler_is_rejected() {
    ensure_hook_handler(
        "cstest-other/notify",
        HookHandlerOwner::Plugin("cstest-other".into()),
    );
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&push_toml("cross", "cstest-other/notify")).expect("parse");
    reg.install_plugin_strategies("cstest-plugin3", &decls);
    assert!(
        reg.get(&CompletionStrategyId::new("cstest-plugin3/cross"))
            .is_none()
    );
}

#[test]
fn default_for_methods_conflict_picks_lower_priority_winner() {
    let reg = CompletionStrategyRegistry::new();
    let low = parse_strategy_section(&poll_toml("low-prio", 10, "acme.wait_a", r#""acme.spawn""#))
        .expect("parse");
    let high = parse_strategy_section(&poll_toml(
        "high-prio",
        200,
        "acme.wait_b",
        r#""acme.spawn""#,
    ))
    .expect("parse");
    reg.install_plugin_strategies("acme", &low);
    reg.install_plugin_strategies("acme", &high);
    let winner = reg
        .resolve_default_for_method("acme.spawn")
        .expect("winner");
    assert_eq!(winner.id, CompletionStrategyId::new("acme/low-prio"));
}

#[test]
fn uninstall_plugin_removes_its_strategies() {
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&poll_toml("temp", 100, "acme.wait", "")).expect("parse");
    reg.install_plugin_strategies("acme", &decls);
    assert!(reg.get(&CompletionStrategyId::new("acme/temp")).is_some());
    reg.uninstall_plugin("acme");
    assert!(reg.get(&CompletionStrategyId::new("acme/temp")).is_none());
}

#[test]
fn resolve_poll_spec_not_found() {
    let reg = CompletionStrategyRegistry::new();
    let err = reg
        .resolve_poll_spec(&CompletionStrategyId::new("acme/nope"))
        .unwrap_err();
    assert!(matches!(err, StrategyResolveError::NotFound { .. }));
}

#[test]
fn resolve_poll_spec_rejects_push_kind() {
    ensure_hook_handler(
        "cstest-plugin4/n",
        HookHandlerOwner::Plugin("cstest-plugin4".into()),
    );
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&push_toml("push-one", "cstest-plugin4/n")).expect("parse");
    reg.install_plugin_strategies("cstest-plugin4", &decls);
    let err = reg
        .resolve_poll_spec(&CompletionStrategyId::new("cstest-plugin4/push-one"))
        .unwrap_err();
    assert!(matches!(err, StrategyResolveError::NotPollKind { .. }));
}

#[test]
fn resolve_poll_spec_rejects_disabled() {
    let reg = CompletionStrategyRegistry::new();
    let toml = r#"
        [[strategy]]
        id = "disabled-one"
        priority = 100
        disabled = true
        [strategy.spec]
        kind = "poll"
        poll_method = "acme.wait"
        state_field = "state"
        terminal_states = ["done"]
    "#;
    let decls = parse_strategy_section(toml).expect("parse");
    reg.install_plugin_strategies("acme", &decls);
    let err = reg
        .resolve_poll_spec(&CompletionStrategyId::new("acme/disabled-one"))
        .unwrap_err();
    assert!(matches!(err, StrategyResolveError::Disabled { .. }));
}

/// 실제 두 기본 TOML에서 완료 전략이 참조하는 훅 핸들러를 찾을 수 있어야 한다.
#[test]
fn host_command_completed_default_strategy_resolves_after_hook_handler_registered() {
    crate::hook_handler::global().install_host_defaults(include_str!(
        "../hook_handler/defaults/default-hook-handlers.toml"
    ));

    let reg = CompletionStrategyRegistry::new();
    reg.install_host_defaults(include_str!("defaults/default-completion-strategies.toml"));

    let strat = reg
        .resolve_strategy(&CompletionStrategyId::new("host/command-completed"))
        .expect("host/command-completed should resolve now that its hook handler exists");
    match strat.kind {
        CompletionStrategyKind::Push {
            notify_via,
            timeout_ms,
        } => {
            assert_eq!(notify_via.as_str(), "host/command-completed");
            assert_eq!(timeout_ms, 300_000);
        }
        CompletionStrategyKind::Poll(_) => panic!("expected push"),
    }
}

#[test]
fn resolve_strategy_returns_poll_kind() {
    let reg = CompletionStrategyRegistry::new();
    let decls =
        parse_strategy_section(&poll_toml("spawn-wait", 100, "acme.wait", "")).expect("parse");
    reg.install_plugin_strategies("acme", &decls);

    let s = reg
        .resolve_strategy(&CompletionStrategyId::new("acme/spawn-wait"))
        .expect("resolve");
    assert!(matches!(s.kind, CompletionStrategyKind::Poll(_)));
}

#[test]
fn resolve_strategy_returns_push_kind_unlike_resolve_poll_spec() {
    ensure_hook_handler(
        "cstest-resolve-strategy/h",
        HookHandlerOwner::Plugin("cstest-resolve-strategy".into()),
    );
    let reg = CompletionStrategyRegistry::new();
    let decls =
        parse_strategy_section(&push_toml("push-one", "cstest-resolve-strategy/h")).expect("parse");
    reg.install_plugin_strategies("cstest-resolve-strategy", &decls);

    let id = CompletionStrategyId::new("cstest-resolve-strategy/push-one");
    let s = reg.resolve_strategy(&id).expect("resolve");
    assert!(matches!(s.kind, CompletionStrategyKind::Push { .. }));
}

#[test]
fn resolve_strategy_not_found() {
    let reg = CompletionStrategyRegistry::new();
    let err = reg
        .resolve_strategy(&CompletionStrategyId::new("acme/nope"))
        .unwrap_err();
    assert!(matches!(err, StrategyResolveError::NotFound { .. }));
}

#[test]
fn resolve_strategy_rejects_disabled() {
    let reg = CompletionStrategyRegistry::new();
    let toml = r#"
        [[strategy]]
        id = "disabled-one"
        priority = 100
        disabled = true
        [strategy.spec]
        kind = "poll"
        poll_method = "acme.wait"
        state_field = "state"
        terminal_states = ["done"]
    "#;
    let decls = parse_strategy_section(toml).expect("parse");
    reg.install_plugin_strategies("acme", &decls);
    let err = reg
        .resolve_strategy(&CompletionStrategyId::new("acme/disabled-one"))
        .unwrap_err();
    assert!(matches!(err, StrategyResolveError::Disabled { .. }));
}

#[test]
fn plugin_display_name_i18n_key_is_preserved() {
    let toml = r#"
        [[strategy]]
        id = "with-label"
        priority = 100
        display_name_i18n_key = "acme.strategy.with_label"
        [strategy.spec]
        kind = "poll"
        poll_method = "acme.wait"
        state_field = "state"
        terminal_states = ["done"]
    "#;
    let decls = parse_strategy_section(toml).expect("parse");
    let reg = CompletionStrategyRegistry::new();
    reg.install_plugin_strategies("acme", &decls);
    let s = reg
        .get(&CompletionStrategyId::new("acme/with-label"))
        .expect("kept");
    assert_eq!(
        s.display_name_i18n_key.as_deref(),
        Some("acme.strategy.with_label")
    );
}

// 실제 매니페스트에서 첫 IPC namespace를 owner로 선택하는 공용 규칙을 사용한다.
// 시험이 짧은 owner 이름을 직접 주면 reverse-DNS ID와 namespace의 차이를 놓칠 수 있다.
fn owner_id_for(m: &tasty_plugin_manifest::Manifest) -> &str {
    m.contributes
        .ipc_namespace
        .first()
        .map(|ns| ns.prefix.as_str())
        .unwrap_or(m.id.as_str())
}

fn install_bundled_manifest_strategies(plugin_id: &str) -> (CompletionStrategyRegistry, String) {
    let path = tasty_test_support::official_plugin_dir(plugin_id);
    let m = tasty_plugin_manifest::Manifest::load(&path).expect("bundled manifest should load");
    let owner_id = owner_id_for(&m).to_string();
    let decls: Vec<CompletionStrategyDecl> = m
        .contributes
        .completion_strategy
        .iter()
        .map(|v| {
            serde_json::from_value(v.clone()).expect("bundled completion_strategy decl parses")
        })
        .collect();
    assert!(
        !decls.is_empty(),
        "{plugin_id} manifest should declare at least one completion strategy"
    );
    let reg = CompletionStrategyRegistry::new();
    reg.install_plugin_strategies(&owner_id, &decls);
    (reg, owner_id)
}

#[test]
fn bundled_claude_manifest_spawn_default_strategy_resolves() {
    let (reg, owner_id) = install_bundled_manifest_strategies("com.tasty.claude");
    assert_eq!(owner_id, "claude");
    let winner = reg
        .resolve_default_for_method("claude.spawn")
        .expect("claude.spawn should resolve a default completion strategy");
    match winner.kind {
        CompletionStrategyKind::Poll(spec) => assert_eq!(spec.poll_method, "claude.state"),
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }
}

#[test]
fn bundled_codex_manifest_spawn_default_strategy_resolves() {
    let (reg, owner_id) = install_bundled_manifest_strategies("com.tasty.codex");
    assert_eq!(owner_id, "codex");
    let winner = reg
        .resolve_default_for_method("codex.spawn")
        .expect("codex.spawn should resolve a default completion strategy");
    match winner.kind {
        CompletionStrategyKind::Poll(spec) => assert_eq!(spec.poll_method, "codex.state"),
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }
}

/// tell의 기본 전략이 빠지면 자식 완료를 기다리지 않고 다음 작업을 시작할 수 있다.
#[test]
fn bundled_claude_manifest_tell_default_strategy_resolves() {
    let (reg, owner_id) = install_bundled_manifest_strategies("com.tasty.claude");
    assert_eq!(owner_id, "claude");
    let winner = reg
        .resolve_default_for_method("claude.tell")
        .expect("claude.tell should resolve a default completion strategy");
    match winner.kind {
        CompletionStrategyKind::Poll(spec) => {
            assert_eq!(spec.poll_method, "claude.state");
            // tell 응답의 surface_id를 같은 이름의 폴링 인자로 넘긴다.
            assert_eq!(
                spec.map_from_response.get("surface_id"),
                Some(&"surface_id".to_string())
            );
        }
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }
}

/// 자식 종료는 실패로 분류해 후속 작업이 성공 산출물을 기대하지 않게 한다.
#[test]
fn bundled_manifests_classify_exited_as_failure() {
    for (plugin_id, methods) in [
        ("com.tasty.claude", ["claude.spawn", "claude.tell"]),
        ("com.tasty.codex", ["codex.spawn", "codex.tell"]),
    ] {
        let (reg, _) = install_bundled_manifest_strategies(plugin_id);
        for method in methods {
            let winner = reg
                .resolve_default_for_method(method)
                .unwrap_or_else(|| panic!("{method} should resolve a default strategy"));
            match winner.kind {
                CompletionStrategyKind::Poll(spec) => {
                    assert!(
                        spec.failure_states.iter().any(|s| s == "exited"),
                        "{method}: exited should be a failure state"
                    );
                    assert!(
                        !spec.terminal_states.iter().any(|s| s == "exited"),
                        "{method}: exited should not also be a success state"
                    );
                    assert!(
                        spec.terminal_states.iter().any(|s| s == "idle"),
                        "{method}: idle stays a success state"
                    );
                }
                CompletionStrategyKind::Push { .. } => panic!("{method}: expected poll"),
            }
        }
    }
}

/// Claude의 needs_input은 성공 상태로 취급하는 현재 정책을 확인한다.
#[test]
fn bundled_claude_manifest_keeps_needs_input_as_success() {
    let (reg, _) = install_bundled_manifest_strategies("com.tasty.claude");
    for method in ["claude.spawn", "claude.tell"] {
        let winner = reg.resolve_default_for_method(method).expect("resolves");
        match winner.kind {
            CompletionStrategyKind::Poll(spec) => {
                assert!(
                    spec.terminal_states.iter().any(|s| s == "needs_input"),
                    "{method}"
                );
                assert!(
                    !spec.failure_states.iter().any(|s| s == "needs_input"),
                    "{method}"
                );
            }
            CompletionStrategyKind::Push { .. } => panic!("expected poll"),
        }
    }
}

/// Codex는 폴링 대상 인자로 surface를 사용한다.
#[test]
fn bundled_codex_manifest_tell_default_strategy_maps_surface_key() {
    let (reg, _) = install_bundled_manifest_strategies("com.tasty.codex");
    let winner = reg
        .resolve_default_for_method("codex.tell")
        .expect("codex.tell should resolve a default completion strategy");
    match winner.kind {
        CompletionStrategyKind::Poll(spec) => {
            assert_eq!(spec.poll_method, "codex.state");
            assert_eq!(
                spec.map_from_response.get("surface_id"),
                Some(&"surface".to_string())
            );
        }
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }
}

#[test]
fn user_override_patches_priority_only() {
    let dir = tempfile::tempdir().unwrap();
    let reg = CompletionStrategyRegistry::new();
    let decls = parse_strategy_section(&poll_toml("patchme", 100, "acme.wait", "")).expect("parse");
    reg.install_plugin_strategies("acme", &decls);

    let p = dir.path().join("completion-strategies.toml");
    std::fs::write(
        &p,
        r#"
        [[strategy]]
        id = "acme/patchme"
        priority = 1
        "#,
    )
    .unwrap();
    reg.install_user_config(&p);

    let s = reg
        .get(&CompletionStrategyId::new("acme/patchme"))
        .expect("kept");
    assert_eq!(s.priority, 1);
    assert_eq!(s.owner, CompletionStrategyOwner::User);
    match s.kind {
        CompletionStrategyKind::Poll(spec) => assert_eq!(spec.poll_method, "acme.wait"),
        CompletionStrategyKind::Push { .. } => panic!("expected poll"),
    }
}

/// 락을 잡은 채 패닉한 뒤에도 설치·조회가 동작하는지 확인한다.
/// 갱신 중간의 패닉이나 모든 내부 불변식을 검증하는 시험은 아니다.
#[test]
fn a_poisoned_registry_still_installs_and_lists() {
    let reg = std::sync::Arc::new(CompletionStrategyRegistry::new());

    let held = std::sync::Arc::clone(&reg);
    let joined = std::thread::spawn(move || {
        let _guard = held.inner.write().expect("fresh rwlock");
        panic!("a thread dies while holding the registry");
    })
    .join();
    assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
    assert!(reg.inner.read().is_err(), "poison 됐어야 한다");

    reg.install_host_defaults(&poll_toml(
        "poisoned_probe",
        10,
        "surface.locate",
        "\"surface.locate\"",
    ));

    let ids: Vec<String> = reg
        .all_strategies_including_disabled()
        .into_iter()
        .map(|s| s.id.0)
        .collect();
    assert!(
        ids.iter().any(|id| id.ends_with("poisoned_probe")),
        "poison 이후에도 설치가 반영돼야 한다: {ids:?}"
    );
    assert!(
        reg.resolve_default_for_method("surface.locate").is_some(),
        "poison 복구 뒤 기본 전략을 조회하지 못했다"
    );
}
