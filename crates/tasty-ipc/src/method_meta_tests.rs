//! `method_meta` 단위 테스트.

use crate::method_meta::{
    METHOD_TABLE, PREFIX_RULES, is_registered_plugin_prefix, method_meta, test_namespace_table,
};
use tasty_plugin_manifest::Permission;

const TEST_OWNER: &str = "com.test.namespace";

fn ns_write() -> std::sync::RwLockWriteGuard<'static, crate::ipc_namespace::IpcNamespaceRegistry> {
    test_namespace_table()
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn ns_clear() {
    ns_write().clear();
}

fn ns_register(prefix: &str) {
    ns_write()
        .register(TEST_OWNER, prefix)
        .expect("test prefix must be free");
}

fn ns_unregister(_prefix: &str) {
    ns_write().unregister_plugin(TEST_OWNER);
}

/// 프로세스 전역 namespace 표를 바꾸는 시험은 같은 TEST_LOCK으로 직렬화한다.
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

#[test]
fn unknown_method_returns_none() {
    assert!(method_meta("not.a.real.method").is_none());
}

#[test]
fn no_duplicate_method_names() {
    let mut seen = std::collections::HashSet::new();
    for (name, _) in METHOD_TABLE {
        assert!(seen.insert(*name), "duplicate method name: {name}");
    }
}

#[test]
fn all_registered_methods_match_naming_policy() {
    const ROOT_EXCEPTIONS: &[&str] = &["split", "tree"];

    for (name, _) in METHOD_TABLE {
        if ROOT_EXCEPTIONS.contains(name) {
            continue;
        }
        let parts: Vec<&str> = name.split('.').collect();
        assert!(
            parts.len() >= 2 && parts.len() <= 3,
            "method '{name}' must be <namespace>.<verb> or <namespace>.<sub>.<verb> \
             (or registered in ROOT_EXCEPTIONS)"
        );
        for part in &parts {
            assert!(!part.is_empty(), "method '{name}' has empty segment");
            assert!(
                part.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "method '{name}': segment '{part}' has invalid characters \
                 (only lowercase a-z, 0-9, _)"
            );
        }
    }
}

#[test]
fn prefix_rules_target_valid_namespaces() {
    for (prefix, _) in PREFIX_RULES {
        assert!(
            prefix.contains('.'),
            "prefix '{prefix}' must include a namespace separator"
        );
        assert!(
            prefix.ends_with('_') || prefix.ends_with('.'),
            "prefix '{prefix}' should end with `_` or `.` to avoid mid-token matches"
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn debug_methods_are_local_only() {
    let m = method_meta("debug.inject_key").expect("registered (debug build)");
    assert!(!m.plugin_callable);
}

#[test]
#[cfg(debug_assertions)]
fn fullscreen_debug_methods_are_local_only() {
    for name in [
        "debug.fullscreen.list",
        "debug.fullscreen.open",
        "debug.fullscreen.close",
        "debug.fullscreen.state",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("{name} registered (debug build)"));
        assert!(!m.plugin_callable, "{name} must be local_only");
    }
}

#[test]
#[cfg(not(debug_assertions))]
fn fullscreen_debug_methods_absent_in_release() {
    for name in [
        "debug.fullscreen.list",
        "debug.fullscreen.open",
        "debug.fullscreen.close",
        "debug.fullscreen.state",
    ] {
        assert!(
            method_meta(name).is_none(),
            "{name} must not exist in release"
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn native_menu_answer_and_pending_move_are_local_only_debug_methods() {
    for name in ["debug.native_menu.answer", "debug.pending_move"] {
        let m = method_meta(name).unwrap_or_else(|| panic!("{name} registered (debug build)"));
        assert!(!m.plugin_callable, "{name} must be local_only");
    }
}

#[test]
#[cfg(not(debug_assertions))]
fn native_menu_answer_and_pending_move_absent_in_release() {
    for name in ["debug.native_menu.answer", "debug.pending_move"] {
        assert!(
            method_meta(name).is_none(),
            "{name} must not exist in release"
        );
    }
}

#[test]
#[cfg(not(debug_assertions))]
fn debug_methods_absent_in_release() {
    assert!(method_meta("debug.inject_key").is_none());
    assert!(method_meta("system.shutdown").is_none());
}

/// mirror 를 만드는 두 경로는 호출자 범위가 같아야 한다. 호출자가 터널을 여는지만 다르다.
#[test]
fn remote_attach_needs_no_token_like_attach_into_gui() {
    for method in ["remote.attach", "attach.into_gui", "remote.refusals"] {
        let meta = method_meta(method).expect(method);
        assert!(meta.plugin_callable, "{method}");
        assert!(meta.required.is_empty(), "{method}: {:?}", meta.required);
    }
    assert_eq!(
        method_meta("remote.attach").map(|m| m.key_contract),
        method_meta("attach.into_gui").map(|m| m.key_contract),
    );
}

#[test]
fn ui_screenshot_promoted_to_release() {
    let m = method_meta("ui.screenshot").expect("registered in release METHOD_TABLE");
    assert!(!m.plugin_callable, "ui.screenshot must be local_only");
    assert!(m.required.is_empty());
}

#[test]
fn clipboard_set_text_is_release() {
    let m = method_meta("clipboard.set_text").expect("registered in release METHOD_TABLE");
    assert!(m.plugin_callable);
    assert!(m.required.contains(&Permission::ClipboardWrite));
}

#[test]
fn surface_list_requires_surface_read() {
    let m = method_meta("surface.list").expect("registered");
    assert!(m.plugin_callable);
    assert!(m.required.contains(&Permission::SurfaceRead));
}

#[test]
fn file_picker_trigger_requires_fs_read() {
    let m = method_meta("file_picker.trigger").expect("registered");
    assert!(m.plugin_callable);
    assert!(m.required.contains(&Permission::FsRead));
}

#[test]
fn the_output_scan_cursor_is_callable_with_the_permission_its_only_caller_holds() {
    let m = method_meta("surface.read_since_scan_mark").expect("registered");
    assert!(m.plugin_callable, "plugin 이 못 부르면 스캐너가 멎는다");
    assert!(!m.plugin_only, "외부 dispatch arm 이 있는 메서드다");
    assert_eq!(
        m.required,
        &[Permission::TerminalRead],
        "형제 `surface.read_since_mark` 와 같은 버킷이어야 한다 — 같은 출력을 읽는다"
    );
}

#[test]
fn terminal_star_is_plugin_callable_within_agent_plugin_permissions() {
    use Permission::*;
    let held = [
        SurfaceRead,
        SurfaceWrite,
        TerminalSpawn,
        TerminalWrite,
        TerminalRead,
    ];
    for method in [
        "terminal.spawn",
        "terminal.tell",
        "terminal.children",
        "terminal.parent",
        "terminal.kill",
        "terminal.respawn",
        "terminal.broadcast",
        "terminal.set_state",
        "terminal.adopt",
        "terminal.release",
    ] {
        let m = method_meta(method).unwrap_or_else(|| panic!("{method} not registered"));
        assert!(m.plugin_callable, "{method} must be plugin-callable");
        for needed in m.required {
            assert!(
                held.contains(needed),
                "{method} requires '{}' which codex/claude do not hold",
                needed.as_token()
            );
        }
    }
}

#[test]
fn recent_query_requires_surface_read() {
    let m = method_meta("recent.query").expect("registered");
    assert!(
        m.plugin_callable,
        "address bar plugin must be able to query recents"
    );
    assert!(m.required.contains(&Permission::SurfaceRead));
    assert!(
        !m.required.contains(&Permission::FsRead),
        "recent 은 임의 path read 가 아니므로 FsRead 를 요구하지 않는다"
    );
}

#[test]
fn tab_create_requires_surface_write() {
    let m = method_meta("tab.create").expect("registered");
    assert!(m.plugin_callable);
    assert!(m.required.contains(&Permission::SurfaceWrite));
}

#[test]
fn surface_completion_requires_notification() {
    let m = method_meta("surface.completion").expect("registered");
    assert!(
        m.plugin_callable,
        "agents must be able to signal completion"
    );
    assert!(m.required.contains(&Permission::Notification));
}

#[test]
fn ime_methods_are_local_only_via_prefix() {
    let m = method_meta("surface.ime_commit").expect("registered");
    assert!(!m.plugin_callable);
}

#[test]
fn plugin_management_is_local_only() {
    let m = method_meta("plugin.enable").expect("registered");
    assert!(!m.plugin_callable);
}

#[test]
fn agent_task_methods_require_agent_manage() {
    for name in [
        "agent.task_create",
        "agent.task_list",
        "agent.task_get",
        "agent.task_cancel",
        "agent.task_retry",
        "agent.task_graph",
        "agent.dag_list",
        "agent.dag_get",
        "agent.dag_report",
        "agent.report_append",
        "agent.task_run",
        "agent.task_delete",
        "agent.task_purge",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(m.plugin_callable, "{name} should be plugin-callable");
        assert!(
            m.required.contains(&Permission::AgentManage),
            "{name} should require AgentManage"
        );
    }
}

#[test]
fn agent_task_await_is_local_only() {
    let m = method_meta("agent.task_await").expect("registered");
    assert!(!m.plugin_callable);
}

#[test]
fn agent_task_set_result_is_local_only() {
    let m = method_meta("agent.task_set_result").expect("registered");
    assert!(!m.plugin_callable);
}

#[test]
fn agent_lease_methods_require_agent_manage() {
    for name in [
        "agent.lease_acquire",
        "agent.lease_release",
        "agent.lease_list",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(m.plugin_callable, "{name} should be plugin-callable");
        assert!(
            m.required.contains(&Permission::AgentManage),
            "{name} should require AgentManage"
        );
    }
}

#[test]
fn agent_barrier_semaphore_methods_require_agent_manage() {
    for name in [
        "agent.barrier_create",
        "agent.barrier_signal",
        "agent.barrier_await",
        "agent.barrier_state",
        "agent.semaphore_create",
        "agent.semaphore_acquire",
        "agent.semaphore_release",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(m.plugin_callable, "{name} should be plugin-callable");
        assert!(
            m.required.contains(&Permission::AgentManage),
            "{name} should require AgentManage"
        );
    }
}

#[test]
fn agent_rate_limit_methods_require_agent_manage() {
    for name in [
        "agent.rate_limit_set",
        "agent.rate_limit_list",
        "agent.rate_limit_remove",
        "agent.rate_limit_status",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(m.plugin_callable, "{name} should be plugin-callable");
        assert!(
            m.required.contains(&Permission::AgentManage),
            "{name} should require AgentManage"
        );
    }
}

/// 턴 보고는 전용 권한만 요구한다. 그 권한이 다른 agent.* 를 열지 않는지도 본다.
#[test]
fn agent_turn_report_needs_only_its_own_permission() {
    let m = method_meta("agent.task_turn_report").expect("registered");
    assert!(m.plugin_callable);
    assert_eq!(m.required, &[Permission::AgentTurnReport][..]);
    let opened: Vec<&str> = METHOD_TABLE
        .iter()
        .filter(|(_, meta)| meta.required.contains(&Permission::AgentTurnReport))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(opened, ["agent.task_turn_report"]);
    let submit = method_meta("agent.task_submit_result").expect("registered");
    assert_eq!(submit.required, &[Permission::AgentManage][..]);
}

#[test]
fn session_issue_revoke_require_agent_manage() {
    for name in ["session.issue", "session.revoke"] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(m.plugin_callable, "{name} should be plugin-callable");
        assert!(
            m.required.contains(&Permission::AgentManage),
            "{name} should require AgentManage"
        );
    }
}

#[test]
fn session_list_is_local_only() {
    let m = method_meta("session.list").expect("registered");
    assert!(!m.plugin_callable);
}

#[test]
fn agent_grant_revoke_are_local_only() {
    for name in [
        "plugin.grant_agent_permission",
        "plugin.revoke_agent_permission",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(!m.plugin_callable, "{name} should be local-only");
    }
}

#[test]
fn agent_list_permissions_is_plugin_readonly() {
    let m = method_meta("plugin.list_agent_permissions").expect("registered");
    assert!(m.plugin_callable);
    assert!(m.required.is_empty(), "list should not require permissions");
}

#[test]
fn request_permission_is_plugin_callable_with_approval() {
    let m = method_meta("plugin.request_permission").expect("registered");
    assert!(m.plugin_callable, "agents must be able to self-request");
    assert!(
        m.required.contains(&Permission::Approval),
        "should require Approval"
    );
}

#[test]
fn plugin_prefix_registration_resolves() {
    let _g = test_lock();
    ns_clear();
    ns_register("codex");
    let m = method_meta("codex.spawn").expect("registered via runtime");
    assert!(m.plugin_callable);
    assert!(m.required.is_empty());
    assert!(
        m.namespace_forward,
        "a name resolved through a plugin prefix must carry the forward mark, or the gate \
         cannot ask for ipc.invoke:<prefix>"
    );
    ns_clear();
}

fn gated(kind: &str, id: &str, perms: &[Permission]) -> crate::caller::CallerContext {
    let permissions = std::sync::Arc::new(perms.iter().cloned().collect());
    match kind {
        "plugin" => crate::caller::CallerContext::Plugin {
            plugin_id: id.into(),
            permissions,
        },
        _ => crate::caller::CallerContext::Agent {
            agent_id: id.into(),
            permissions,
        },
    }
}

#[test]
fn a_gated_caller_needs_the_namespace_token_to_reach_a_plugin_namespace() {
    let _g = test_lock();
    ns_clear();
    ns_register("codex");

    let err = gated("agent", "child:1", &[])
        .ensure_allowed("codex.spawn")
        .expect_err("a zero-permission agent must not reach a plugin namespace");
    assert!(
        matches!(
            &err,
            crate::caller::CallerError::MissingPermission { permission, .. }
                if *permission == Permission::IpcInvoke("codex".into())
        ),
        "the refusal must name ipc.invoke:codex so elevation can offer it, got {err:?}"
    );
    assert!(
        gated("agent", "child:1", &[Permission::IpcInvoke("codex".into())])
            .ensure_allowed("codex.spawn")
            .is_ok()
    );
    assert!(
        gated(
            "agent",
            "child:1",
            &[Permission::IpcInvoke("claude".into())]
        )
        .ensure_allowed("codex.spawn")
        .is_err()
    );
    assert!(
        gated("plugin", "com.other.plugin", &[])
            .ensure_allowed("codex.spawn")
            .is_err()
    );
    assert!(
        crate::caller::CallerContext::local()
            .ensure_allowed("codex.spawn")
            .is_ok()
    );
    ns_clear();
}

#[test]
fn only_the_owning_plugin_process_is_exempt_from_the_namespace_token() {
    let _g = test_lock();
    ns_clear();
    ns_register("codex");

    assert!(
        gated("plugin", TEST_OWNER, &[])
            .ensure_allowed("codex.spawn")
            .is_ok(),
        "the owner calling its own namespace is a trampoline and must not need ipc.invoke:<self>"
    );
    assert!(
        gated("agent", TEST_OWNER, &[])
            .ensure_allowed("codex.spawn")
            .is_err(),
        "an agent named after the owner must not inherit the owner exemption"
    );
    ns_clear();
}

#[test]
fn a_table_name_under_a_plugin_prefix_keeps_the_table_requirement_only() {
    let _g = test_lock();
    ns_clear();
    ns_register("image");

    let m = method_meta("image.open").expect("static");
    assert!(!m.namespace_forward);
    assert!(
        gated(
            "agent",
            "child:1",
            &[Permission::SurfaceWrite, Permission::FsRead]
        )
        .ensure_allowed("image.open")
        .is_ok()
    );
    ns_clear();
}

#[test]
fn plugin_prefix_unregister_removes() {
    let _g = test_lock();
    ns_clear();
    ns_register("codex");
    ns_unregister("codex");
    assert!(method_meta("codex.spawn").is_none());
}

#[test]
fn static_table_wins_over_plugin_prefix() {
    let _g = test_lock();
    ns_clear();
    ns_register("image");
    let m = method_meta("image.open").expect("static");
    assert!(m.required.contains(&Permission::SurfaceWrite));
    ns_clear();
}

#[test]
fn plugin_prefix_idempotent_register() {
    let _g = test_lock();
    ns_clear();
    ns_register("codex");
    ns_register("codex");
    let m = method_meta("codex.spawn").expect("still registered");
    assert!(m.plugin_callable);
    ns_unregister("codex");
    assert!(method_meta("codex.spawn").is_none());
    ns_clear();
}

#[test]
fn audit_methods_are_local_only() {
    for name in [
        "plugin.audit_query",
        "plugin.audit_summary",
        "plugin.audit_follow",
        "plugin.audit_clear",
    ] {
        let m = method_meta(name).unwrap_or_else(|| panic!("registered: {name}"));
        assert!(!m.plugin_callable, "{name} should be local-only");
    }
}

/// poison 뒤 등록된 prefix로 검사해야 복구를 false로 대체한 결함을 검출할 수 있다.
#[test]
fn a_poisoned_prefix_registry_still_blocks_the_owner_bypass() {
    let _g = test_lock();
    ns_clear();

    let panicked = std::thread::spawn(|| {
        let _held = test_namespace_table().write().expect("not poisoned yet");
        panic!("poison the prefix registry");
    })
    .join();
    assert!(panicked.is_err(), "the helper thread must have panicked");
    assert!(
        test_namespace_table().read().is_err(),
        "the registry lock must actually be poisoned now"
    );

    // 읽기와 쓰기 복구를 모두 확인하려고 poison 이후에 등록한다.
    ns_register("codex");
    assert!(
        is_registered_plugin_prefix("codex"),
        "a poisoned registry must not report a registered prefix as free — that opens the \
         very owner bypass this check exists to close"
    );
    assert!(
        method_meta("codex.spawn")
            .expect("still resolves")
            .plugin_callable,
        "prefix lookup must survive the poison too"
    );

    ns_unregister("codex");
    assert!(
        !is_registered_plugin_prefix("codex"),
        "writes must land on a poisoned registry as well, or a dead plugin keeps its namespace"
    );
    ns_clear();
}

#[test]
fn the_unrouted_third_branch_asks_the_exact_table_not_the_prefix_fallback() {
    let _g = test_lock();
    ns_clear();
    ns_register("zzztestns");

    let name = "zzztestns.whatever";
    assert!(
        method_meta(name).is_some(),
        "prefix 등록이 안 먹었다 — 이 테스트의 대조군이 죽었다"
    );
    assert!(
        !crate::method_meta::is_registered_name(name),
        "prefix fallback 을 '표에 있다' 로 셌다 — plugin 표면을 host 가 삼킨다"
    );
    let resp =
        crate::protocol::JsonRpcResponse::unrouted_for_external_caller(serde_json::json!(1), name);
    assert_eq!(
        resp.error.expect("에러여야 한다").code,
        -32601,
        "plugin namespace 이름이 -32601 이 아니면 헤드리스 forward 가 안 탄다"
    );

    let resp = crate::protocol::JsonRpcResponse::unrouted_for_external_caller(
        serde_json::json!(1),
        "window.create",
    );
    assert_eq!(resp.error.expect("에러여야 한다").code, -32017);

    ns_unregister("zzztestns");
}

#[test]
fn every_branch_of_the_effect_classification_is_used() {
    use crate::method_meta::{DEBUG_METHODS, MethodEffect};
    let mut read = 0;
    let mut idem = 0;
    let mut mutate = 0;
    for (_, meta) in METHOD_TABLE.iter().chain(DEBUG_METHODS.iter()) {
        match meta.effect {
            MethodEffect::Read => read += 1,
            MethodEffect::Idempotent => idem += 1,
            MethodEffect::Mutate => mutate += 1,
        }
    }
    assert!(read > 0 && idem > 0 && mutate > 0, "{read} {idem} {mutate}");
}

#[test]
fn the_effect_axis_is_redelivery_not_the_verb() {
    use crate::method_meta::MethodEffect::*;
    let eff = |m: &str| {
        method_meta(m)
            .unwrap_or_else(|| panic!("표에 없다: {m}"))
            .effect
    };

    assert_eq!(eff("ui.screenshot"), Mutate);
    assert_eq!(eff("message.read"), Mutate);
    assert_eq!(eff("surface.read_since_mark"), Read);
    assert_eq!(eff("surface.read_since_scan_mark"), Mutate);
    assert_eq!(eff("surface.set_mark"), Mutate);
    assert_eq!(eff("surface.meta.set"), Idempotent);
    assert_eq!(eff("agent.lease_acquire"), Idempotent);
    assert_eq!(eff("workspace.create"), Mutate);
    assert_eq!(eff("output.observe_start"), Mutate);
    assert_eq!(eff("tab.close"), Idempotent);
    assert_eq!(eff("webhook.register"), Mutate);
    assert_eq!(eff("preset.capture"), Mutate);
}

#[cfg(debug_assertions)]
#[test]
fn a_debug_method_is_judged_on_the_same_axis_as_the_rest() {
    use crate::method_meta::MethodEffect::*;
    // 같은 조합에서 elapsed_ms를 반복하면 hold_since가 다시 앞당겨져 Mutate다.
    assert_eq!(
        method_meta("debug.modifier_hint.hold").map(|m| m.effect),
        Some(Mutate)
    );
}

#[test]
fn a_forwarded_namespace_name_is_assumed_unsafe_to_redeliver() {
    use crate::method_meta::MethodEffect;
    let _g = test_lock();
    ns_clear();
    ns_register("zzzeffectns");
    let meta = method_meta("zzzeffectns.anything").expect("prefix 등록이 안 먹었다");
    assert!(meta.namespace_forward);
    assert_eq!(meta.effect, MethodEffect::Mutate);
    ns_unregister("zzzeffectns");
}

#[test]
fn a_forwarded_namespace_name_is_declared_outside_the_key_contract() {
    use crate::method_meta::{KeyContract, key_contract};
    let _g = test_lock();
    ns_clear();
    assert_eq!(key_contract("zzzkeyns.anything"), KeyContract::Outside);
    ns_register("zzzkeyns");
    let meta = method_meta("zzzkeyns.anything").expect("prefix 등록이 안 먹었다");
    assert!(meta.namespace_forward);
    assert_eq!(meta.key_contract, KeyContract::Outside);
    assert_eq!(key_contract("zzzkeyns.anything"), KeyContract::Outside);
    ns_unregister("zzzkeyns");
}

#[test]
fn a_host_method_declaration_follows_its_effect() {
    use crate::method_meta::{
        DEBUG_METHODS, KEY_KEPT_BY_APP_LAYER, KEY_KEPT_BY_ROUTER, KEY_KEPT_ON_EVERY_HOST_PATH,
        KeyContract, METHOD_TABLE, MethodEffect,
    };
    for (name, meta) in METHOD_TABLE.iter().chain(DEBUG_METHODS) {
        match (meta.effect, meta.key_contract) {
            (
                MethodEffect::Mutate | MethodEffect::Idempotent,
                KeyContract::Kept {
                    since: crate::method_meta::KEY_KEPT_IN_STRUCTURE_JOURNAL,
                },
            ) => {}
            (MethodEffect::Read | MethodEffect::Idempotent, KeyContract::Unneeded) => {}
            (MethodEffect::Mutate, KeyContract::Kept { since })
                if since == KEY_KEPT_BY_ROUTER
                    || since == KEY_KEPT_BY_APP_LAYER
                    || since == KEY_KEPT_ON_EVERY_HOST_PATH => {}
            (effect, contract) => panic!("{name}: {effect:?} 에 {contract:?} 선언"),
        }
    }
    assert_eq!(
        crate::method_meta::key_contract("image.open"),
        KeyContract::Kept {
            since: KEY_KEPT_ON_EVERY_HOST_PATH
        }
    );
    assert_eq!(
        crate::method_meta::key_contract("window.create"),
        KeyContract::Kept {
            since: KEY_KEPT_BY_APP_LAYER
        }
    );
    assert_eq!(
        crate::method_meta::key_contract("workspace.create"),
        KeyContract::Kept {
            since: crate::method_meta::KEY_KEPT_IN_STRUCTURE_JOURNAL
        }
    );
    assert_eq!(
        crate::method_meta::key_contract("workspace.list"),
        KeyContract::Unneeded
    );
}

#[test]
fn a_host_mutation_under_a_claimable_prefix_is_kept_from_version_three() {
    use crate::method_meta::{
        KEY_KEPT_ON_EVERY_HOST_PATH, KeyContract, METHOD_TABLE, MethodEffect,
    };
    use tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES;
    let claimable: Vec<&str> = METHOD_TABLE
        .iter()
        .filter(|(_, m)| m.effect == MethodEffect::Mutate)
        .map(|(name, _)| *name)
        .filter(|name| !RESERVED_IPC_PREFIXES.contains(&name.split('.').next().unwrap_or(name)))
        .collect();
    // 빈 목록끼리의 비교가 통과하지 않도록 예약 밖 Mutate 개수도 확인한다.
    assert!(
        claimable.len() >= 6,
        "예약 밖 prefix 의 Mutate 를 {} 개만 읽었다: {claimable:?}",
        claimable.len()
    );
    let declared: Vec<&str> = METHOD_TABLE
        .iter()
        .filter(|(_, m)| {
            m.key_contract
                == KeyContract::Kept {
                    since: KEY_KEPT_ON_EVERY_HOST_PATH,
                }
        })
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        claimable, declared,
        "예약 밖 prefix 의 Mutate(왼쪽)와 표의 판 3 선언(오른쪽)이 다르다 — 그 prefix 에 Mutate 를 \
         더했으면 `.kept_on_every_host_path()` 를 붙인다"
    );
}

#[test]
fn an_unregistered_name_has_no_since_answer_at_all() {
    use crate::method_meta::method_since;
    assert_eq!(method_since("zzz.not.a.method"), None);
}

#[test]
fn the_frozen_split_puts_names_on_both_sides() {
    use crate::method_meta::{METHOD_TABLE, MethodSince, method_since};
    let mut frozen = 0;
    let mut after = 0;
    for (name, _) in METHOD_TABLE {
        match method_since(name) {
            Some(MethodSince::FrozenBaseline) => frozen += 1,
            Some(MethodSince::AfterFrozenBaseline) => after += 1,
            None => panic!("등재된 이름인데 답이 없다: {name}"),
        }
    }
    assert!(frozen > 0 && after > 0, "frozen {frozen} after {after}");
}

#[test]
fn the_since_answer_comes_from_the_frozen_file_not_from_a_second_list() {
    use crate::method_meta::{MethodSince, method_since};
    assert_eq!(
        method_since("agent.barrier_create"),
        Some(MethodSince::FrozenBaseline)
    );
    assert_eq!(
        method_since("attach.acquire"),
        Some(MethodSince::AfterFrozenBaseline)
    );
}

#[test]
#[cfg(debug_assertions)]
fn toast_debug_method_is_local_only() {
    let m = method_meta("debug.toast").expect("registered (debug build)");
    assert!(!m.plugin_callable, "debug.toast must be local_only");
}

/// 토스트는 사용자 행동의 결과에만 뜬다. 범위를 지정해 띄우는 경로는 release에 없다.
#[test]
#[cfg(not(debug_assertions))]
fn toast_debug_method_absent_in_release() {
    assert!(
        method_meta("debug.toast").is_none(),
        "debug.toast must not exist in release"
    );
}

#[test]
#[cfg(debug_assertions)]
fn window_list_debug_method_is_local_only_read() {
    let m = method_meta("debug.window.list").expect("registered (debug build)");
    assert!(!m.plugin_callable, "debug.window.list must be local_only");
    assert_eq!(m.effect, crate::method_meta::MethodEffect::Read);
}

/// 보조 창 ID 는 debug 입력 주입에만 쓰인다. release `window.list` 는 메인 창만 다룬다.
#[test]
#[cfg(not(debug_assertions))]
fn window_list_debug_method_absent_in_release() {
    assert!(
        method_meta("debug.window.list").is_none(),
        "debug.window.list must not exist in release"
    );
}

/// 원격 사용자를 강제로 끊는 공개 메서드는 release 표에 없다. 로컬 사용자는 GUI로 끊는다.
#[test]
fn release_table_has_no_force_detach_method() {
    let offenders: Vec<&str> = METHOD_TABLE
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| name.contains("force_detach"))
        .collect();
    assert!(offenders.is_empty(), "release force-detach: {offenders:?}");
    #[cfg(not(debug_assertions))]
    for name in [
        "attach.force_detach",
        "attach.force_detach_workspace",
        "debug.attach.force_detach",
        "debug.attach.force_detach_workspace",
    ] {
        assert!(
            method_meta(name).is_none(),
            "{name} must not exist in release"
        );
    }
}

/// debug 빌드는 로컬 사용자의 강제 끊기를 local 전용 메서드로 재현한다.
#[test]
#[cfg(debug_assertions)]
fn force_detach_reproduction_is_debug_local_only() {
    for name in [
        "debug.attach.force_detach",
        "debug.attach.force_detach_workspace",
    ] {
        let m = method_meta(name).expect("registered (debug build)");
        assert!(!m.plugin_callable, "{name} must be local_only");
    }
    for name in ["attach.force_detach", "attach.force_detach_workspace"] {
        assert!(method_meta(name).is_none(), "{name} must stay unregistered");
    }
}
