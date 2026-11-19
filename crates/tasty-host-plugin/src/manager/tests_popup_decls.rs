//! 실행 중 팝업 선언: 등록 검사, 매니페스트보다 먼저 쓰는 조회, 프로세스 정리, 옛 인스턴스 표에 여는 값.

use std::sync::Arc;

use serde_json::json;
use tasty_plugin_protocol::popup_decl::{PopupDecl, PopupOutsideClick, PopupScopeKind};
use tasty_plugin_protocol::popup_spec::PopupDeclSpec;
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::PluginManager;
use crate::process::PluginProcess;

const PLUGIN: &str = "com.example.viewer";

fn manager_with(popups: &str, api: &str) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
        r#"
manifest_version = 1
id = "{PLUGIN}"
name = "Viewer"
version = "0.0.1"
api_version = "{api}"

[entry]
type = "process"
command = "echo"
args = []
{popups}
"#
    ))
    .expect("manifest parses");
    mgr.set_packages_for_tests(vec![crate::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/viewer"),
        manifest,
    }]);
    let (process, _rx) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.to_string(), process);
    mgr
}

fn manager() -> PluginManager {
    manager_with("", tasty_plugin_manifest::HOST_API_VERSION)
}

fn decl(spec: serde_json::Value) -> PopupDecl {
    serde_json::from_value::<PopupDeclSpec>(spec)
        .expect("spec")
        .to_decl()
}

fn surface_decl() -> PopupDecl {
    decl(json!({ "scope_kind": "surface", "size": { "width": 360, "height": 150 } }))
}

#[test]
fn a_plugin_without_manifest_popups_declares_and_opens_one() {
    let mut mgr = manager();
    assert!(mgr.popup_decl(PLUGIN, "confirm").is_none());
    mgr.declare_popup(PLUGIN, "confirm", surface_decl())
        .expect("declare");
    assert_eq!(mgr.popup_decl(PLUGIN, "confirm"), Some(surface_decl()));
    assert!(
        mgr.plugin_surface_popup_ids()
            .contains(&format!("{PLUGIN}/confirm"))
    );
    let iid = mgr
        .open_popup_instance(PLUGIN, "confirm", json!({ "surface_id": 7 }))
        .expect("opens");
    mgr.bind_popup_instance_surface(iid, 7);
    let (_, inst) = mgr
        .popup_instances()
        .find(|(id, _)| *id == iid)
        .expect("instance");
    let size = inst.contribute.size_hint.expect("size");
    assert_eq!((size.width, size.height), (360, 150));
    assert_eq!(
        inst.contribute.scope,
        tasty_plugin_manifest::PopupScopeDecl::Surface
    );
    assert_eq!(inst.scope_surface, Some(7));
    // 기본 선언의 바깥 클릭은 포커스 해제라 렌더러에서는 닫지 않는다.
    assert!(!inst.contribute.dismiss_on_outside_click);
}

/// 덮음·좁은 범위·Esc 선언은 여는 인스턴스까지 그대로 간다. 매니페스트 선언은 기본값이다.
#[test]
fn the_layout_of_a_run_time_declaration_reaches_the_instance() {
    use tasty_plugin_protocol::popup_decl::{PopupEscape, PopupFit};
    let mut mgr = manager();
    let layout_decl = decl(json!({
        "scope_kind": "surface", "size": { "width": 360, "height": 150 },
        "covers_scope": false, "fit": "shrink", "scope_inset": false, "escape": "ignore"
    }));
    mgr.declare_popup(PLUGIN, "confirm", layout_decl.clone())
        .expect("declare");
    let iid = mgr
        .open_popup_instance(PLUGIN, "confirm", json!({ "surface_id": 7 }))
        .expect("opens");
    let (_, inst) = mgr
        .popup_instances()
        .find(|(id, _)| *id == iid)
        .expect("instance");
    let layout = inst.contribute.layout;
    assert!(!layout.covers_scope);
    assert_eq!(layout.fit, PopupFit::Shrink);
    assert!(!layout.scope_inset);
    assert_eq!(layout.escape, PopupEscape::Ignore);
    assert_eq!(layout, tasty_plugin_manifest::PopupLayout::of(&layout_decl));
    assert_eq!(
        tasty_plugin_manifest::PopupLayout::default(),
        tasty_plugin_manifest::PopupLayout::of(&PopupDecl::new(
            PopupScopeKind::Window,
            tasty_plugin_manifest::DEFAULT_POPUP_SIZE
        ))
    );
}

#[test]
fn a_run_time_declaration_wins_over_the_manifest_one() {
    let mut mgr = manager_with(
        r#"
[[contributes.popup]]
id = "viewer"
trigger = { kind = "ipc" }
size_hint = { width = 480, height = 360 }
"#,
        tasty_plugin_manifest::HOST_API_VERSION,
    );
    let from_manifest = mgr.popup_decl(PLUGIN, "viewer").expect("manifest decl");
    assert_eq!(from_manifest.scope_kind, PopupScopeKind::Window);
    assert_eq!(from_manifest.outside_click, PopupOutsideClick::Close);
    assert!(mgr.plugin_surface_popup_ids().is_empty());

    mgr.declare_popup(PLUGIN, "viewer", surface_decl())
        .expect("declare");
    assert_eq!(mgr.popup_decl(PLUGIN, "viewer"), Some(surface_decl()));
    assert_eq!(mgr.declared_popup_keys().len(), 1);
    assert!(
        mgr.plugin_surface_popup_ids()
            .contains(&format!("{PLUGIN}/viewer"))
    );
}

#[test]
fn invalid_declarations_are_refused() {
    let mut mgr = manager();
    let err = mgr
        .declare_popup(PLUGIN, "Bad_Id", surface_decl())
        .expect_err("bad id");
    assert!(err.contains("invalid popup id"), "{err}");
    let err = mgr
        .declare_popup(
            PLUGIN,
            "zero",
            decl(json!({ "scope_kind": "window", "size": { "width": 0, "height": 10 } })),
        )
        .expect_err("zero size");
    assert!(err.contains("positive"), "{err}");
    let err = mgr
        .declare_popup(
            PLUGIN,
            "point",
            decl(json!({
                "scope_kind": "window",
                "size": { "width": 10, "height": 10 },
                "anchor": "at_point"
            })),
        )
        .expect_err("at_point");
    assert!(err.contains("at_point"), "{err}");
    let err = mgr
        .declare_popup("com.example.absent", "p", surface_decl())
        .expect_err("not running");
    assert!(err.contains("not running"), "{err}");
    assert!(mgr.declared_popup_keys().is_empty());
}

#[test]
fn an_older_api_version_cannot_declare() {
    let mut mgr = manager_with("", "0.0.0-old");
    let err = mgr
        .declare_popup(PLUGIN, "confirm", surface_decl())
        .expect_err("api mismatch");
    assert!(err.contains("api_version"), "{err}");
}

#[test]
fn forgetting_the_process_drops_its_declarations() {
    let mut mgr = manager();
    mgr.declare_popup(PLUGIN, "confirm", surface_decl())
        .expect("declare");
    mgr.forget_plugin_runtime(PLUGIN, "test");
    assert!(mgr.popup_decl(PLUGIN, "confirm").is_none());
}

#[test]
fn scopes_the_current_renderer_cannot_draw_do_not_open() {
    let mut mgr = manager();
    mgr.declare_popup(
        PLUGIN,
        "pane-note",
        decl(json!({ "scope_kind": "pane", "size": { "width": 10, "height": 10 } })),
    )
    .expect("pane scope is a valid declaration");
    assert!(
        mgr.open_popup_instance(PLUGIN, "pane-note", serde_json::Value::Null)
            .is_none()
    );
    assert_eq!(mgr.popup_instances().count(), 0);
}

#[test]
fn an_open_instance_keeps_the_declaration_it_opened_with() {
    let mut mgr = manager();
    mgr.declare_popup(PLUGIN, "confirm", surface_decl())
        .expect("declare");
    let iid = mgr
        .open_popup_instance(PLUGIN, "confirm", serde_json::Value::Null)
        .expect("opens");
    mgr.declare_popup(
        PLUGIN,
        "confirm",
        decl(json!({ "scope_kind": "surface", "size": { "width": 500, "height": 300 } })),
    )
    .expect("redeclare");
    let (_, inst) = mgr
        .popup_instances()
        .find(|(id, _)| *id == iid)
        .expect("instance");
    assert_eq!(inst.contribute.size_hint.expect("size").width, 360);
}

#[test]
fn a_column_declaration_opens_at_the_center_until_the_renderer_draws_columns() {
    let mut mgr = manager();
    mgr.declare_popup(
        PLUGIN,
        "notice",
        decl(json!({
            "scope_kind": "window",
            "size": { "width": 320, "height": 120 },
            "anchor": "column"
        })),
    )
    .expect("a column anchor is a valid plugin declaration");
    let iid = mgr
        .open_popup_instance(PLUGIN, "notice", serde_json::Value::Null)
        .expect("opens");
    let (_, inst) = mgr
        .popup_instances()
        .find(|(id, _)| *id == iid)
        .expect("instance");
    assert_eq!(
        inst.contribute.anchor,
        tasty_plugin_manifest::PopupAnchor::ScreenCenter
    );
}
