//! `popup.declare`·`popup.show`: 대상 맞춤 판정과, 실제 App 진입부를 거친 오류 응답 코드.
//!
//! App 시험은 창이 없는 App이다. 그래서 대상이 없는 경우(없는 surface, 없는 창, 포커스 창 없음)를 잰다.
//! `App` 을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop` 는 프로세스에 하나뿐이라,
//! App 시험은 `crate::test_support::rerun_alone` 으로 새 프로세스에서 혼자 돈다.

use super::*;
use serde_json::json;

fn decl(kind: &str) -> PopupDecl {
    serde_json::from_value::<tasty_plugin_protocol::PopupDeclSpec>(
        json!({ "scope_kind": kind, "size": { "width": 10, "height": 10 } }),
    )
    .expect("spec")
    .to_decl()
}

#[test]
fn each_supported_target_matches_its_declaration() {
    assert_eq!(
        match_target(
            "p",
            "c",
            Some(decl("surface")),
            PopupShowTarget::Surface { id: 42 }
        ),
        Ok(ShowTarget::Surface(42))
    );
    assert_eq!(
        match_target(
            "p",
            "c",
            Some(decl("window")),
            PopupShowTarget::Window { id: 3 }
        ),
        Ok(ShowTarget::Window(3))
    );
    assert_eq!(
        match_target(
            "p",
            "c",
            Some(decl("window")),
            PopupShowTarget::FocusedWindow
        ),
        Ok(ShowTarget::FocusedWindow)
    );
}

#[test]
fn an_undeclared_popup_is_refused() {
    let (code, msg) =
        match_target("p", "c", None, PopupShowTarget::Surface { id: 1 }).expect_err("undeclared");
    assert_eq!(code, POPUP_INVALID_PARAMS);
    assert!(msg.contains("'p/c' is not declared"), "{msg}");
}

#[test]
fn the_target_kind_must_match_the_declaration() {
    for (kind, target) in [
        ("surface", PopupShowTarget::FocusedWindow),
        ("surface", PopupShowTarget::Window { id: 1 }),
        ("window", PopupShowTarget::Surface { id: 1 }),
    ] {
        let (code, msg) =
            match_target("p", "c", Some(decl(kind)), target).expect_err("kind mismatch");
        assert_eq!(code, POPUP_INVALID_PARAMS);
        assert!(msg.contains("declared with scope"), "{msg}");
    }
}

#[test]
fn workspace_pane_and_tab_targets_are_not_opened_yet() {
    for (kind, target) in [
        ("pane", PopupShowTarget::Pane { id: 1 }),
        ("tab", PopupShowTarget::Tab { id: 1 }),
        ("workspace", PopupShowTarget::Workspace { id: 1 }),
    ] {
        let (code, msg) = match_target("p", "c", Some(decl(kind)), target).expect_err(kind);
        assert_eq!(code, POPUP_INVALID_PARAMS);
        assert!(msg.contains("not supported yet"), "{kind}: {msg}");
    }
}

const SCOPE_PLUGIN: &str = "com.example.popup-scope";

/// 실행 중인 plugin 하나가 있는 관리자. 열린 인스턴스의 범위 판정만 잰다.
fn scope_manager() -> crate::plugin::PluginManager {
    let mut mgr = crate::plugin::PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let manifest = toml::from_str(&format!(
        "manifest_version = 1\nid = \"{SCOPE_PLUGIN}\"\nname = \"Popup scope\"\nversion = \"1.0.0\"\n\
         api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n",
        crate::plugin::manifest::HOST_API_VERSION
    ))
    .expect("manifest");
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/popup-scope"),
        manifest,
    }]);
    mgr
}

/// `from` 범위로 선언해 연 뒤 `to` 범위로 다시 선언하고, `to` 범위 대상으로 여는 판정을 잰다.
fn reopen_with_other_scope(from: &str, to: &str, to_scope: PopupScopeDecl) {
    let mut mgr = scope_manager();
    let _stub = mgr.attach_caller_stub_for_test(SCOPE_PLUGIN);
    mgr.declare_popup(SCOPE_PLUGIN, "note", decl(from))
        .expect("declare");
    let iid = mgr
        .open_popup_instance(SCOPE_PLUGIN, "note", serde_json::Value::Null)
        .expect("opens");
    let opened_scope = mgr
        .popup_instances()
        .find(|(id, _)| *id == iid)
        .expect("instance")
        .1
        .contribute
        .scope;
    mgr.declare_popup(SCOPE_PLUGIN, "note", decl(to))
        .expect("redeclare");

    let (code, msg) =
        check_open_scope(&mgr, SCOPE_PLUGIN, "note", to_scope).expect_err("other scope");
    assert_eq!(code, POPUP_INVALID_PARAMS);
    assert!(msg.contains("is open with a different scope"), "{msg}");
    let still: Vec<_> = mgr
        .popup_instances()
        .map(|(id, i)| (id, i.contribute.scope, i.scope_surface))
        .collect();
    assert_eq!(
        still,
        vec![(iid, opened_scope, None)],
        "the open instance is left as it was"
    );

    // 같은 범위로 다시 여는 것은 지금처럼 그 인스턴스를 돌려준다.
    check_open_scope(&mgr, SCOPE_PLUGIN, "note", opened_scope).expect("same scope");
}

#[test]
fn a_window_popup_reopened_as_surface_is_refused_and_left_open() {
    reopen_with_other_scope("window", "surface", PopupScopeDecl::Surface);
}

#[test]
fn a_surface_popup_reopened_as_window_is_refused_and_left_open() {
    reopen_with_other_scope("surface", "window", PopupScopeDecl::Window);
}

// App 시험이 쓰는 X11 EventLoop 도우미와 GUI App 생성기는 Linux 시험 빌드에만 있다.
#[cfg(target_os = "linux")]
mod app_level {
    use super::*;
    use std::sync::Arc;

    const PLUGIN: &str = "com.example.popup-ipc";

    fn app(event_loop: &winit::event_loop::EventLoop<crate::AppEvent>) -> App {
        let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
        // 창 없는 engine 하나를 둔다. engine이 없으면 plugin 호출이 입장 검사에서 거절되고,
        // 창이 없으므로 포커스 창도 없다.
        let (state, session) = crate::state::tests::test_state();
        app.engines.park_for_test(state, session);
        let mut mgr = crate::plugin::PluginManager::with_registries(
            Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
            Arc::new(crate::file::format::FileFormatRegistry::new()),
            Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        let manifest = toml::from_str(&format!(
            "manifest_version = 1\nid = \"{PLUGIN}\"\nname = \"Popup IPC\"\nversion = \"1.0.0\"\n\
             api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n",
            crate::plugin::manifest::HOST_API_VERSION
        ))
        .expect("manifest");
        mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
            dir: std::path::PathBuf::from("/nonexistent/popup-ipc"),
            manifest,
        }]);
        app.plugin_manager = Some(mgr);
        app
    }

    /// 호출 하나를 진입부부터 처리하고 그 호출의 회신 (result, error, error_code)를 돌려준다.
    fn call(
        app: &mut App,
        stub: &tasty_host_plugin::manager::CallerStub,
        call_id: u64,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        app.plugin_manager
            .as_mut()
            .expect("manager")
            .queue_plugin_call_for_test(PLUGIN, call_id, method, params);
        app.process_plugin_ipc_calls();
        // plugin 호출은 cap 조회 결과를 기다리며 게이트에 선다. 호스트 루프처럼 결과가 나올 때까지 진행한다.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !app.gate_queue.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "{method}: a gated plugin call never finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.advance_request_gates(&mut crate::app::ipc::StepTally::default());
        }
        let replies: Vec<_> = stub
            .drain_requests()
            .into_iter()
            .filter(|(_, p)| p["call_id"] == call_id)
            .collect();
        assert_eq!(replies.len(), 1, "{method}: {replies:?}");
        replies[0].1.clone()
    }

    #[test]
    fn missing_vanished_and_unfocused_targets_are_refused_with_their_codes() {
        if crate::test_support::rerun_alone(
            module_path!(),
            "missing_vanished_and_unfocused_targets_are_refused_with_their_codes",
        ) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
        let event_loop = crate::x11_test_loop::x11_event_loop();
        let mut app = app(&event_loop);
        let stub = app
            .plugin_manager
            .as_mut()
            .expect("manager")
            .attach_caller_stub_for_test(PLUGIN);

        for (id, popup, kind) in [(1, "confirm", "surface"), (2, "viewer", "window")] {
            let r = call(
                &mut app,
                &stub,
                id,
                "popup.declare",
                json!({ "popup_id": popup, "decl": { "scope_kind": kind, "size": { "width": 360, "height": 150 } } }),
            );
            assert_eq!(r["result"], json!({}), "{r}");
        }

        let cases = [
            (
                json!({ "popup_id": "confirm" }),
                POPUP_INVALID_PARAMS,
                "missing field `target`",
            ),
            (
                json!({ "popup_id": "absent", "target": { "kind": "surface", "id": 1 } }),
                POPUP_INVALID_PARAMS,
                "is not declared",
            ),
            (
                json!({ "popup_id": "confirm", "target": { "kind": "surface", "id": 999 } }),
                POPUP_TARGET_NOT_FOUND,
                "surface 999 not found",
            ),
            (
                json!({ "popup_id": "viewer", "target": { "kind": "window", "id": 12345 } }),
                POPUP_TARGET_NOT_FOUND,
                "window 12345 not found",
            ),
            (
                json!({ "popup_id": "viewer", "target": { "kind": "focused_window" } }),
                POPUP_TARGET_NOT_FOUND,
                "no window is focused",
            ),
        ];
        for (i, (params, code, text)) in cases.into_iter().enumerate() {
            let r = call(&mut app, &stub, 10 + i as u64, "popup.show", params.clone());
            assert!(r["result"].is_null(), "{params}: {r}");
            assert_eq!(r["error_code"], code, "{params}: {r}");
            let msg = r["error"].as_str().unwrap_or_default();
            assert!(msg.contains(text), "{params}: {msg}");
        }
        assert_eq!(
            app.plugin_manager
                .as_ref()
                .expect("manager")
                .popup_instances()
                .count(),
            0,
            "refused requests must not open anything"
        );

        let r = call(
            &mut app,
            &stub,
            30,
            "popup.declare",
            json!({ "popup_id": "Bad_Id", "decl": { "scope_kind": "window", "size": { "width": 1, "height": 1 } } }),
        );
        assert_eq!(r["error_code"], POPUP_INVALID_PARAMS, "{r}");
    }
}
