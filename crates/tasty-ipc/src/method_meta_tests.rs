//! `method_meta` 단위 테스트.

use crate::method_meta::{
    METHOD_TABLE, PREFIX_RULES, is_registered_plugin_prefix, method_meta, test_namespace_table,
};
use tasty_plugin_manifest::Permission;

/// 표를 조작하는 테스트가 쓰는 가짜 소유자. 소유는 plugin 단위라 prefix 만으로는
/// 등록할 수 없다 — 표가 답하는 물음이 "누가 소유하나" 이기 때문이다.
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

/// 표가 process-global 이라 동일 binary 안에서 병렬 test 가
/// 표를 동시 변형하지 못하게 직렬화.
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

/// 모든 등록 메서드는 명명 규칙을 따라야 한다 (docs/dev-guide/api-conventions.md):
///
/// 1. `<namespace>.<verb>` 또는 `<namespace>.<sub>.<verb>` 3단까지
/// 2. 또는 [`ROOT_EXCEPTIONS`]에 등록된 root 메서드
/// 3. 각 부분은 소문자 알파벳/숫자/`_` 만 허용
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

/// 무대 debug IPC 4 종은 전부 plugin 에 노출되지 않는다. 무대는 창 전체를 덮는
/// 화면 점유라 plugin 이 열 수 있으면 사용자 화면을 가로챌 수 있다.
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

/// release 에는 무대 debug IPC 가 아예 없어야 한다 — `#[cfg(debug_assertions)]`
/// 격리가 메타 테이블까지 일관되게 적용됐는지 확인한다.
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
#[cfg(not(debug_assertions))]
fn debug_methods_absent_in_release() {
    assert!(method_meta("debug.inject_key").is_none());
    assert!(method_meta("system.shutdown").is_none());
    // ui.screenshot 은 focus-독립 정식 기능으로 승격됨 — release 에 노출된다
    // (아래 `ui_screenshot_promoted_to_release` 참조).
}

/// `ui.screenshot` 은 debug 전용에서 focus-독립 정식 기능으로 승격됐다 —
/// release `METHOD_TABLE` 에 존재하고 local_only(파일 쓰기 표면, plugin 미노출)여야
/// 한다. (구 debug-only 대칭 테스트 `debug_methods_absent_in_release` 와 짝.)
#[test]
fn ui_screenshot_promoted_to_release() {
    let m = method_meta("ui.screenshot").expect("registered in release METHOD_TABLE");
    assert!(!m.plugin_callable, "ui.screenshot must be local_only");
    assert!(m.required.is_empty());
}

/// `clipboard.set_text` — release `METHOD_TABLE` 에 존재하고 plugin_callable +
/// `Permission::ClipboardWrite` 필수여야 한다 (원격 mirror 캡처를 원격 인스턴스의
/// clipboard.set_text 로 반영하는 attach 전송 경로도 이 등록을 사용).
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

/// ADR-0058: `file_picker.trigger` 는 `git_viewer.query` 와
/// 동일 근거(파일을 고르는 read 관심사)로 FsRead 권한이 필요하고, plugin 이 직접
/// host.call 로 호출 가능해야 한다.
#[test]
fn file_picker_trigger_requires_fs_read() {
    let m = method_meta("file_picker.trigger").expect("registered");
    assert!(m.plugin_callable);
    assert!(m.required.contains(&Permission::FsRead));
}

/// 출력 스캐너의 전용 커서는 그 스캐너가 **이미 들고 있는** 권한으로 닿아야 한다.
///
/// 스캐너는 claude plugin 안에서 800 ms 주기로 돈다. 이 메서드가 새 토큰을 요구하면,
/// 매니페스트를 함께 고치지 않은 설치본에서 스캔이 `permission_denied` 로 **조용히**
/// 멎는다 — 에러 감지가 통째로 사라지는데 아무 신호도 없다. 그래서 같은 출력을 읽는
/// `surface.read_since_mark` 과 같은 버킷(`terminal.read`)인지 여기서 못 박는다
/// (`docs/adr/0307-the-output-scanner-reads-its-own-cursor.md`).
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

/// occupancy-05: codex/claude plugin 이 자식 관리를 호스트 `terminal.*` 로 위임할 수
/// 있어야 한다. 모든 terminal.* 메서드가 plugin_callable 이고, 요구 권한이 두
/// plugin 이 매니페스트에 이미 선언한 권한 집합(surface.read/write, terminal.spawn/
/// write/read) 부분집합이어야 위임이 permission_denied 없이 통과한다.
#[test]
fn terminal_star_is_plugin_callable_within_agent_plugin_permissions() {
    use Permission::*;
    // codex/claude 매니페스트가 보유한 terminal-관련 권한 상한.
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
    // generic per-kind recent 조회는 임의 파일 read(FsRead) 가 아니라 이미 열었던 목록
    // 반환뿐 → 더 약한 SurfaceRead 권한. plugin(주소창 03)이 호출 가능해야 한다.
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
    // completion 은 read 가 아니라 highlight 발동(PushNotification 계열) →
    // notification.* 와 동일한 Notification 권한, plugin 이 호출 가능해야 한다.
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

// approval.await 와 대칭 — 진짜 blocking 이라 plugin 의 단일 워커 스레드를 막을
// 위험이 있어 local caller 전용으로 닫는다.
#[test]
fn agent_task_await_is_local_only() {
    let m = method_meta("agent.task_await").expect("registered");
    assert!(!m.plugin_callable);
}

/// 러너가 Custom task 의 생명주기를 단독 소유한다 — plugin 이 task_set_result
/// 로 같은 task 를 별도 전이시키면 쓰기 주체가 이중화돼 러너의 완료 판정과
/// 경합한다. plugin 은 완료 판정 전략 선언으로 우회한다(agent.task_await 와
/// 같은 이유 계열). 등재 자체는 "누락"과 "의도적 local_only" 를 구분하기 위한
/// 것이다 — 미등재면 plugin 호출자가 UnknownMethod 로 거부돼, 표를 읽는 쪽이
/// 정책인지 실수인지 판별할 수 없다.
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

/// 표에 없는 plugin namespace 이름은 권한 셋을 가진 caller 에게 `ipc.invoke:<prefix>` 를
/// 요구한다. 이 검사가 없던 때에는 권한 0 agent 토큰이 `markdown.recent` 같은 이름으로
/// 설치된 plugin 의 namespace 전체를 불렀다.
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
    // 다른 namespace 의 토큰으로는 안 열린다.
    assert!(
        gated(
            "agent",
            "child:1",
            &[Permission::IpcInvoke("claude".into())]
        )
        .ensure_allowed("codex.spawn")
        .is_err()
    );
    // plugin caller 도 같은 토큰을 요구한다 — plugin→plugin forward 가 요구하던 것과 같다.
    assert!(
        gated("plugin", "com.other.plugin", &[])
            .ensure_allowed("codex.spawn")
            .is_err()
    );
    // Local 은 여전히 무검사다.
    assert!(
        crate::caller::CallerContext::Local
            .ensure_allowed("codex.spawn")
            .is_ok()
    );
    ns_clear();
}

/// 소유 plugin 이 자기 namespace 를 부르는 것은 trampoline 이라 토큰을 요구하지 않는다.
/// 그 면제는 **plugin 프로세스**에만 선다 — agent 의 `agent_id` 는 발급자가 고른 문자열이라
/// 소유자 id 와 같게 지을 수 있다.
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

/// 표에 이름 그대로 있는 것은 plugin prefix 아래여도 표가 적은 권한만 요구한다 —
/// 그 이름들은 host 가 답하는 메서드다.
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

/// registry 락이 poison 돼도 소유자 우회 차단이 계속 선다.
///
/// 조준점이 **등록된** prefix 인 것이 이 테스트의 요점이다. 미등록 prefix 로 겨누면
/// 복구를 지워도 `unwrap_or(false)` 가 우연히 같은 답을 내서 변이가 살아남는다.
/// 호출부(`method_allowed_for_owner`)가 이 함수를 `!` 로 뒤집어 쓰므로, poison 시
/// `false` 를 돌려주는 것은 곧 Host/User 가 남의 plugin namespace 를 부르게 열어
/// 주는 것이다 — 이 함수가 애초에 막으려던 우회 그 자체다.
///
/// poison 은 sticky 라 이 테스트 이후 같은 바이너리의 모든 접근이 복구 경로를 지난다.
/// 그래도 다른 테스트가 깨지지 않는다는 것이 곧 복구가 자리잡았다는 증거다.
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

    // 등록도 poison 이후에 한다 — 그래야 읽기 경로와 쓰기 경로가 **둘 다** 겨냥된다.
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

/// 종단 응답의 셋째 갈래는 **정확 표 조회**로 갈린다 — prefix fallback 을 타면 안 된다.
///
/// [`method_meta`] 는 마지막 단계에서 런타임 등록 plugin prefix 까지 해소하므로, 그것으로
/// `unrouted_for_external_caller` 의 갈래를 태우면 설치된 plugin 의 이름과 그 아래 오타까지
/// host 가 삼킨다 — 실측 2026-09-05 로 `claude.children` · `agent_stream.list` ·
/// `markdown.no_such_thing` 이 전부 `-32017` 이 되고, plugin 으로 갈 호출이 안 갔다.
/// 근거는 [ADR-0167](../../../docs/adr/0167-a-registered-name-answers-whether-it-is-in-this-binary.md).
///
/// 이 테스트가 `method_meta_tests.rs` 에 사는 이유는 **런타임 prefix 레지스트리를 만지기
/// 때문**이다. 그 전역을 만지는 테스트는 이 파일의 `TEST_LOCK` 을 잡아야 한다.
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

    // 대조군의 반대편 — 표에 그 이름 그대로 있는 것은 셋째 갈래를 탄다.
    let resp = crate::protocol::JsonRpcResponse::unrouted_for_external_caller(
        serde_json::json!(1),
        "window.create",
    );
    assert_eq!(resp.error.expect("에러여야 한다").code, -32017);

    ns_unregister("zzztestns");
}

// ── 재전달 분류 (`MethodEffect`) ──────────────────────────────────────

/// 표의 모든 이름이 분류를 갖는다 — 이 시험이 아니라 **타입**이 그것을 강제한다.
///
/// 여기서 재는 것은 그 다음 명제다: 세 갈래가 **전부 쓰인다.** 한 갈래가 비면 그 값은
/// 계약이 아니라 장식이고, 소비자가 그것으로 갈래를 만들 수 없다. 하한을 둔 이유는
/// 상한을 두면 메서드를 더할 때마다 이 수를 부양해야 하기 때문이다.
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
    // 합이 표 길이와 같은지는 **안 묻는다.** 위 `match` 가 망라적이라 그 등식은 항상
    // 참이고, 물으면 아무것도 안 재면서 "갈래가 늘었는데 안 따라왔다" 를 재는 것처럼
    // 읽힌다. 갈래가 늘면 그 `match` 가 **컴파일**에서 막는다 — 그것이 그 사실의
    // 채널이고, 이 시험이 대신할 수 있는 자리가 아니다.
}

/// 분류의 축은 "읽기인가" 가 **아니라** "두 번 전달하면 차이가 남는가" 다.
///
/// 그 차이가 드러나는 자리를 앵커로 박는다 — 이름만 보고 다시 칠하면 여기서 빨개진다.
/// 각 줄이 재는 명제가 다르다:
#[test]
fn the_effect_axis_is_redelivery_not_the_verb() {
    use crate::method_meta::MethodEffect::*;
    let eff = |m: &str| {
        method_meta(m)
            .unwrap_or_else(|| panic!("표에 없다: {m}"))
            .effect
    };

    // 이름이 조회인데 파일을 남긴다.
    assert_eq!(eff("ui.screenshot"), Mutate);
    // 이름이 읽기인데 소비한다(`peek` 기본 false).
    assert_eq!(eff("message.read"), Mutate);
    // 이름이 읽기이고 실제로 커서를 안 옮긴다.
    assert_eq!(eff("surface.read_since_mark"), Read);
    // ★ 위와 **커서만 다른 짝**이다. 이름도 read 이고 같은 출력을 읽는데, 이쪽은
    // 읽은 구간을 소비한다 — 재전달이 두 번째 구간을 먹고 첫 응답의 바이트는
    // 어디에서도 다시 안 나온다. 두 줄이 붙어 있는 것이 이 축의 뜻이다: 갈래를
    // 정하는 것은 동사가 아니라 두 번째 전달이 남기는 것이다.
    assert_eq!(eff("surface.read_since_scan_mark"), Mutate);
    // 이름이 `set` 인데 값이 "지금" 이라 재전달이 위치를 옮긴다.
    assert_eq!(eff("surface.set_mark"), Mutate);
    // 값을 호출자가 주는 `set` 은 수렴한다.
    assert_eq!(eff("surface.meta.set"), Idempotent);
    // 이름이 `acquire` 인데 같은 holder 면 멱등이라고 구현이 이미 적어 뒀다.
    assert_eq!(eff("agent.lease_acquire"), Idempotent);
    // 이름이 `create` 인데 id 를 호출자가 안 줘서 두 번이면 둘이 생긴다.
    assert_eq!(eff("workspace.create"), Mutate);
    // 이름이 `start` 인데 observer_id 가 새로 난다.
    assert_eq!(eff("output.observe_start"), Mutate);
    // 닫힌 것은 닫힌 채로 있다.
    assert_eq!(eff("tab.close"), Idempotent);
    // 이름이 `register` 인데 부를 때마다 새 opaque id 와 새 공개 URL 이 난다.
    assert_eq!(eff("webhook.register"), Mutate);
    // 이름을 안 주면 서버가 unique_name 을 지어 슬롯이 하나 더 생긴다 — 주는
    // 경로만 보고 멱등이라고 읽으면 안 된다. 한 이름에 두 성질이 있으면
    // 조심스러운 쪽이 그 이름의 값이다.
    assert_eq!(eff("preset.capture"), Mutate);
}

/// debug 표에도 같은 축이 선다 — 이름이 "누른 상태로 맞춰라" 인데 재전달이 수렴하지
/// 않는 자리가 있다.
#[cfg(debug_assertions)]
#[test]
fn a_debug_method_is_judged_on_the_same_axis_as_the_rest() {
    use crate::method_meta::MethodEffect::*;
    // `update_hold` 는 조합이 같으면 `hold_since` 를 **보존**하고, 그 뒤
    // `debug_backdate` 가 그 값에서 다시 뺀다 — 같은 `elapsed_ms` 를 두 번 보내면
    // 타이머가 두 번 뒤로 간다.
    assert_eq!(
        method_meta("debug.modifier_hint.hold").map(|m| m.effect),
        Some(Mutate)
    );
}

/// plugin namespace 로 넘어가는 이름은 호스트가 뜻을 모른다 — 그때 고르는 값은
/// **조심스러운 쪽**이어야 한다. `Read` 로 새면 소비자가 재전달해도 된다고 읽는다.
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

/// plugin namespace forward 는 멱등 키 계약 **밖**이라고 선언된다 — 호스트 프로세스(소유
/// 표가 설치된 쪽)에서도, 표가 없는 client 프로세스에서도 같은 답이 나와야 한다. ADR-0361.
#[test]
fn a_forwarded_namespace_name_is_declared_outside_the_key_contract() {
    use crate::method_meta::{KeyContract, key_contract};
    let _g = test_lock();
    ns_clear();
    // 소유 표에 없는 이름 = client 프로세스가 plugin 메서드를 보는 모양.
    assert_eq!(key_contract("zzzkeyns.anything"), KeyContract::Outside);
    ns_register("zzzkeyns");
    // 호스트 프로세스가 같은 이름을 보는 모양 — namespace fallback 으로 해소된다.
    let meta = method_meta("zzzkeyns.anything").expect("prefix 등록이 안 먹었다");
    assert!(meta.namespace_forward);
    assert_eq!(meta.key_contract, KeyContract::Outside);
    assert_eq!(key_contract("zzzkeyns.anything"), KeyContract::Outside);
    ns_unregister("zzzkeyns");
}

/// 호스트 메서드의 선언은 [`MethodEffect`] 에서 유도되고, 판이 다른 이름만 손으로 고친다 —
/// 재전달이 원래 안전한 것은 `Unneeded`, `Mutate` 는 `Kept`(판 1 = engine 라우터, 판 2 = App
/// 층, 판 3 = GUI debug step · namespace forward 로 나가는 표 이름). **`Mutate` 가 아닌데 `Kept`
/// 인 이름은 없고, 표의 이름이 `Outside` 인 경우도 없다** — 보존소는 `Mutate` 만 받고, 호스트가
/// 아는 `Mutate` 이름은 어느 경로로 가든 보존소를 지난다(ADR-0566). 어느 이름이 판 2 · 3 인지가 실제
/// 배선과 맞는지는 본체의 `source_guards::key_contract_by_layer` 가 잰다. ADR-0423.
#[test]
fn a_host_method_declaration_follows_its_effect() {
    use crate::method_meta::{
        DEBUG_METHODS, KEY_KEPT_BY_APP_LAYER, KEY_KEPT_BY_ROUTER, KEY_KEPT_ON_EVERY_HOST_PATH,
        KeyContract, METHOD_TABLE, MethodEffect,
    };
    for (name, meta) in METHOD_TABLE.iter().chain(DEBUG_METHODS) {
        match (meta.effect, meta.key_contract) {
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
            since: KEY_KEPT_BY_ROUTER
        }
    );
    assert_eq!(
        crate::method_meta::key_contract("workspace.list"),
        KeyContract::Unneeded
    );
}

/// 표의 `Mutate` 가운데 **plugin 이 점유할 수 있는 prefix**(예약 목록 밖) 아래의 것은 전부 판 3
/// 이고, `METHOD_TABLE` 의 판 3 은 그것뿐이다.
///
/// 그 이름들은 prefix 를 점유한 plugin 이 켜져 있으면 engine 라우터가 아니라 namespace forward 로
/// 먼저 나간다 — 판 2 서버는 거기서 키를 무시했다. 좌변을 예약 목록에서 유도하는 이유는 "지금
/// 어느 plugin 이 무엇을 점유했나" 가 설치마다 달라서다: 예약 밖이면 **점유될 수 있고**, 점유되면
/// forward 로 간다. ADR-0566.
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
    // 좌변이 비면 아래 대조가 "둘 다 빈 집합" 으로 초록이 된다. 실측 2026-09-23 여섯.
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

/// "언제부터 있었나" 는 **등재 여부와 다른 물음**이다. 미등록 이름에 "예전부터
/// 있었다" 를 답하면 client 가 없는 메서드를 부를 수 있다고 읽는다.
#[test]
fn an_unregistered_name_has_no_since_answer_at_all() {
    use crate::method_meta::method_since;
    assert_eq!(method_since("zzz.not.a.method"), None);
}

/// 두 값이 **실제로 갈린다.** 한쪽만 나오면 동결 파일을 못 읽었거나 표를 못 읽은
/// 것이고, 그 상태에서도 위 시험은 통과한다.
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

/// 값의 출처가 **동결 파일**이라는 것을 이름 둘로 못박는다. 손으로 적은 칸이었다면
/// 파일을 고쳐도 이 둘이 안 움직인다.
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
