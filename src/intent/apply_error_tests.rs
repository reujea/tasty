//! 적용 실패 신호의 origin 분기 — 사용자 발화의 실패만 toast 를 내고, 에이전트 발화의
//! 실패는 로그로 끝난다(identity 원칙 1, `docs/design/systems/toast.md` "트리거 정책").
//!
//! gui 전용이다 — toast 매니저와, forward 큐를 비우고 원격 회신을 받는 쪽(attach client)이
//! gui 에만 있다.

use super::*;

fn fixture() -> (
    crate::core::Core,
    crate::state::AppState,
    crate::core::CoreState,
) {
    let (state, engine) = crate::state::tests::test_state();
    (
        crate::ipc::handler::cli_entry_tests::test_core(),
        state,
        engine,
    )
}

fn blocked(forwarded: bool) -> anyhow::Error {
    anyhow::Error::new(crate::core::MirrorStructuralBlocked {
        workspace_index: 0,
        forwarded,
    })
}

fn agent() -> IntentOrigin {
    IntentOrigin::Agent {
        source: AgentSource::Ipc,
    }
}

fn user() -> IntentOrigin {
    IntentOrigin::User {
        source: UserSource::Shortcut("test"),
    }
}

/// forward 할 수 없는 차단(`forwarded=false`)은 사용자 발화에서만 차단 toast 를 낸다.
/// `report_apply_error` 의 `origin.is_user()` 가드를 지우는 변이에서 실패해야 한다.
#[test]
fn an_unforwardable_block_toasts_only_for_the_user() {
    let (_core, mut state, mut engine) = fixture();
    report_apply_error(&mut state, &mut engine, &agent(), "t", &blocked(false));
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 발화의 차단은 toast 를 안 낸다"
    );

    report_apply_error(&mut state, &mut engine, &user(), "t", &blocked(false));
    assert_eq!(
        state.toasts.len(),
        1,
        "사용자 발화의 차단은 종전대로 toast 를 낸다"
    );
}

/// 철회된 kind(제공 plugin 이 꺼졌거나 아직 다시 연결되지 않았다)로 만들려다 거절되면 사용자 발화에서만 그 사유를
/// toast 로 보인다 — 에이전트 발화는 로그뿐이다(ADR-0534 · ADR-0503).
#[test]
fn a_withdrawn_kind_refusal_toasts_only_for_the_user() {
    let (_core, mut state, mut engine) = fixture();
    let err = anyhow::Error::new(crate::core::surface_registry::SurfaceKindWithdrawn {
        kind: "markdown".to_string(),
        plugin_id: "com.tasty.markdown".to_string(),
    });
    report_apply_error(&mut state, &mut engine, &agent(), "t", &err);
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 발화의 거절은 toast 를 안 낸다"
    );

    report_apply_error(&mut state, &mut engine, &user(), "t", &err);
    assert_eq!(
        state.toasts.len(),
        1,
        "사용자 발화의 거절은 사유 toast 를 낸다"
    );
}

/// IPC(`markdown.navigate`)가 mirror 워크스페이스의 surface 를 바꾸면 op 는 원격으로
/// forward 되고, 그 op 는 실패 회신이 toast 가 아니라 로그로 가도록 표시된다. 같은 op 를
/// 사용자가 발화하면 표시되지 않는다(원격 실패 toast 는 종전대로).
#[test]
fn an_agent_forward_is_marked_for_a_silent_failure() {
    let (mut core, mut state, mut engine) = fixture();
    let surface_id = *state
        .active_workspace(&engine)
        .all_surface_ids()
        .first()
        .expect("fixture surface");
    engine.workspaces[0].mirror = true;
    let file = tempfile::NamedTempFile::new().expect("tmp file");

    let req = crate::ipc::protocol::JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        method: "markdown.navigate".to_string(),
        params: serde_json::json!({
            "surface_id": surface_id,
            "path": file.path().to_string_lossy(),
        }),
        id: Some(serde_json::json!(1)),
        session_token: None,
    };
    let resp = crate::ipc::handler::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &req,
        &crate::ipc::caller::CallerContext::Local,
    );
    assert!(resp.error.is_none(), "navigate: {:?}", resp.error);
    crate::intent::headless::drain_pending_intents(&mut core, &mut state, &mut engine);

    assert_eq!(
        engine.pending_structural_forward.len(),
        1,
        "convert 는 forward 된다"
    );
    assert!(
        engine.pending_structural_forward[0].silent_failure,
        "에이전트 발화 op 는 실패 회신을 로그로 보낸다"
    );
    assert_eq!(state.toasts.len(), 0);

    // 같은 op 를 사용자 발화로 — 표시되지 않는다.
    engine.pending_structural_forward.clear();
    crate::intent::surface::handle(
        &mut core,
        &mut state,
        &mut engine,
        &Intent::ConvertSurface {
            surface_id,
            target: ConvertTarget::Terminal,
        }
        .from_user_menu("test"),
    );
    assert_eq!(engine.pending_structural_forward.len(), 1);
    assert!(!engine.pending_structural_forward[0].silent_failure);
}

/// preset 적용 실패(없는 이름)는 사용자 발화에서만 실패 toast 를 낸다.
/// `preset::apply` 의 `origin.is_user()` 가드를 `true` 로 바꾸는 변이에서 실패해야 한다.
#[test]
fn a_preset_apply_failure_toasts_only_for_the_user() {
    let (core, mut state, mut engine) = fixture();
    let apply = || Intent::ApplyPreset {
        kind: tasty_presets::PresetKind::Tab,
        name: "no-such-preset-apply-error-tests".to_string(),
        category: None,
    };
    preset::handle(&core, &mut state, &mut engine, &apply().from_agent_ipc());
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 발화의 적용 실패는 toast 를 안 낸다"
    );

    preset::handle(
        &core,
        &mut state,
        &mut engine,
        &apply().from_user_menu("test"),
    );
    assert_eq!(
        state.toasts.len(),
        1,
        "사용자 발화의 적용 실패는 toast 를 낸다"
    );
}

/// preset 저장 실패(저장소가 거절하는 이름)는 사용자 발화에서만 실패 toast 를 낸다.
/// 이름 검증에서 거절되므로 디스크에 아무것도 쓰지 않는다.
/// `preset::save` 의 `show_toast` 가드를 `true` 로 바꾸는 변이에서 실패해야 한다.
#[test]
fn a_preset_save_failure_toasts_only_for_the_user() {
    let (core, mut state, mut engine) = fixture();
    let ws = &engine.workspaces[0];
    let captured = crate::intent::preset_capture::capture_workspace_preset(
        &engine,
        ws,
        None,
        &engine.surface_registry,
    )
    .expect("capture");
    let save = || Intent::SavePreset {
        base_name: "unused".to_string(),
        explicit_name: Some("bad/name".to_string()),
        overwrite: true,
        preset: ClonedPreset::Workspace(captured.clone()),
    };
    preset::handle(&core, &mut state, &mut engine, &save().from_agent_ipc());
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 발화의 저장 실패는 toast 를 안 낸다"
    );

    preset::handle(
        &core,
        &mut state,
        &mut engine,
        &save().from_user_menu("test"),
    );
    assert_eq!(
        state.toasts.len(),
        1,
        "사용자 발화의 저장 실패는 toast 를 낸다"
    );
}

/// 철회된 kind 로의 제자리 변환은 거절되므로 최근 목록에도 남지 않는다 — 기록은 적용 전에
/// 일어나므로, 그 판정이 정의 잔존(`get`)이 아니라 새로 만들 수 있는가(`get_live`)를 물어야
/// 한다(ADR-0534 "운영 비용"). 다시 등록해 철회가 풀리면 같은 변환이 기록된다(대조군).
#[test]
fn a_convert_to_a_withdrawn_kind_leaves_no_recent_entry() {
    let (mut core, mut state, mut engine) = fixture();
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "probe_recent",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "webview",
        "records_recent": true,
    }))
    .expect("probe decl");
    let register = |engine: &crate::core::CoreState| {
        let (host_cmd_tx, _host_cmd_rx) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.surface_registry,
            "com.x.probe",
            &decl,
            host_cmd_tx,
        );
    };
    register(&engine);
    assert_eq!(
        engine.surface_registry.withdraw_plugin("com.x.probe"),
        vec!["probe_recent"]
    );
    let surface_id = *state
        .active_workspace(&engine)
        .all_surface_ids()
        .first()
        .expect("fixture surface");
    let convert = |file: &str| {
        Intent::ConvertSurface {
            surface_id,
            target: ConvertTarget::Kind {
                cwd: None,
                kind: "probe_recent".to_string(),
                params: serde_json::json!({ "file": file }),
            },
        }
        .from_user_menu("test")
    };

    crate::intent::surface::handle(
        &mut core,
        &mut state,
        &mut engine,
        &convert("/notes/withdrawn.md"),
    );
    assert_eq!(
        state.recent_files.get("probe_recent"),
        Vec::<String>::new(),
        "거절된 변환은 최근 목록에 남지 않는다"
    );

    register(&engine);
    crate::intent::surface::handle(
        &mut core,
        &mut state,
        &mut engine,
        &convert("/notes/live.md"),
    );
    assert_eq!(
        state.recent_files.get("probe_recent"),
        vec!["/notes/live.md".to_string()],
        "철회가 풀린 kind 로의 변환은 기록된다"
    );
}

/// intent 를 안 거치고 IPC 에서 직접 적용되는 구조 요청(split · tab.create/close/move ·
/// pane.close · surface.close · image.open)도 mirror 워크스페이스에서 forward 되면 실패 회신이
/// 로그로 가도록 표시된다 — 에이전트 요청이다. `structural_exec::apply_as_agent` 의 표시 호출이나
/// `image::handle_open` 의 표시 호출을 지우는 변이에서 해당 메서드 줄이 실패해야 한다.
#[test]
fn an_ipc_direct_structural_forward_is_marked_for_a_silent_failure() {
    let (mut core, mut state, mut engine) = fixture();
    let ws = state.active_workspace(&engine);
    let surface_id = *ws.all_surface_ids().first().expect("fixture surface");
    let pane_id = engine.find_pane_for_surface(surface_id).expect("pane");
    let tab_id = engine
        .find_pane_by_id(pane_id)
        .and_then(|p| p.tabs.first())
        .map(|t| t.id)
        .expect("tab");
    engine.workspaces[0].mirror = true;

    let requests = [
        (
            "split",
            serde_json::json!({ "level": "surface", "target_surface": surface_id }),
        ),
        (
            "split",
            serde_json::json!({ "level": "pane", "target_pane": pane_id }),
        ),
        ("tab.create", serde_json::json!({ "pane_id": pane_id })),
        ("tab.close", serde_json::json!({ "tab_id": tab_id })),
        (
            "tab.move",
            serde_json::json!({ "pane_id": pane_id, "from_index": 0, "to_index": 0 }),
        ),
        ("pane.close", serde_json::json!({ "pane_id": pane_id })),
        (
            "surface.close",
            serde_json::json!({ "surface_id": surface_id }),
        ),
        (
            "image.open",
            serde_json::json!({ "surface_id": surface_id, "path": "/tmp/x.png" }),
        ),
    ];
    for (method, params) in requests {
        engine.pending_structural_forward.clear();
        let req = crate::ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params.clone(),
            id: Some(serde_json::json!(1)),
            session_token: None,
        };
        let resp = crate::ipc::handler::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &crate::ipc::caller::CallerContext::Local,
        );
        assert!(resp.error.is_none(), "{method} {params}: {:?}", resp.error);
        assert_eq!(
            resp.result.as_ref().and_then(|r| r.get("forwarded")),
            Some(&serde_json::json!(true)),
            "{method} {params}: forward 된다"
        );
        assert_eq!(
            engine.pending_structural_forward.len(),
            1,
            "{method} {params}"
        );
        assert!(
            engine.pending_structural_forward[0].silent_failure,
            "{method} {params}: 에이전트 요청의 원격 실패는 로그로 간다"
        );
    }
    assert_eq!(state.toasts.len(), 0);
}
