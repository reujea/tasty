//! `popup_spec` 단위 테스트. wire 모양은 plugin 이행이 기대는 계약이라 JSON 그대로 고정한다.

use super::*;
use crate::popup_decl::{PopupChrome, PopupDragHandle};
use serde_json::json;
use tasty_type_geometry::length::LogicalPx;

const SIZE: PopupSize = PopupSize {
    width: LogicalPx(360.0),
    height: LogicalPx(150.0),
};

#[test]
fn an_empty_spec_is_the_plugin_default() {
    let spec = PopupDeclSpec::new(PopupScopeKind::Surface, SIZE);
    assert_eq!(
        spec.to_decl(),
        PopupDecl::plugin(PopupScopeKind::Surface, SIZE)
    );
}

#[test]
fn given_fields_override_the_default() {
    let mut spec = PopupDeclSpec::new(PopupScopeKind::Window, SIZE);
    spec.outside_click = Some(PopupOutsideClick::Close);
    spec.keyboard = Some(PopupKeyboard::Text);
    spec.instances = Some(PopupInstances::Single);
    spec.covers_scope = Some(false);
    let d = spec.to_decl();
    assert_eq!(d.outside_click, PopupOutsideClick::Close);
    assert_eq!(d.keyboard, PopupKeyboard::Text);
    assert_eq!(d.instances, PopupInstances::Single);
    assert!(!d.covers_scope);
    // 셸 모양 필드는 plugin이 정하지 않는다.
    assert_eq!(d.chrome, PopupChrome::Headless);
    assert_eq!(d.drag_handle, PopupDragHandle::None);
}

#[test]
fn the_declare_params_read_from_minimal_json() {
    let p: PopupDeclareParams = serde_json::from_value(json!({
        "popup_id": "large-file-confirm",
        "decl": { "scope_kind": "surface", "size": { "width": 360, "height": 150 } }
    }))
    .expect("minimal declare");
    assert_eq!(p.popup_id, "large-file-confirm");
    assert_eq!(p.decl, PopupDeclSpec::new(PopupScopeKind::Surface, SIZE));
}

#[test]
fn unknown_and_host_owned_fields_are_ignored() {
    let p: PopupDeclareParams = serde_json::from_value(json!({
        "popup_id": "viewer",
        "decl": {
            "scope_kind": "window",
            "size": { "width": 360, "height": 150 },
            "drag_handle": "region",
            "shell": "dialog",
            "chrome": "title_bar",
            "a_field_from_the_future": 1
        }
    }))
    .expect("extra fields");
    let d = p.decl.to_decl();
    assert_eq!(d.drag_handle, PopupDragHandle::None);
    assert_eq!(d.chrome, PopupChrome::Headless);
}

#[test]
fn the_declare_params_keep_their_json_shape() {
    let full = json!({
        "popup_id": "large-file-confirm",
        "decl": {
            "scope_kind": "surface",
            "size": { "width": 360.0, "height": 150.0 },
            "instances": "per_scope_target",
            "covers_scope": true,
            "fit": "collapse",
            "anchor": "scope_center",
            "scope_inset": true,
            "focus_on_open": true,
            "keyboard": "keys",
            "escape": "close",
            "outside_click": "close",
            "sticky_focus": false,
            "resizable": false,
            "min_size": { "width": 100.0, "height": 50.0 }
        }
    });
    let p: PopupDeclareParams = serde_json::from_value(full.clone()).expect("full declare");
    assert_eq!(serde_json::to_value(&p).expect("serialize"), full);
    // 생략한 필드는 직렬화에도 나오지 않는다.
    let minimal = PopupDeclareParams {
        popup_id: "x".into(),
        decl: PopupDeclSpec::new(PopupScopeKind::Window, SIZE),
    };
    assert_eq!(
        serde_json::to_value(&minimal).expect("serialize"),
        json!({
            "popup_id": "x",
            "decl": { "scope_kind": "window", "size": { "width": 360.0, "height": 150.0 } }
        })
    );
}

#[test]
fn the_show_params_and_result_keep_their_json_shape() {
    let cases = [
        (
            json!({ "kind": "window", "id": 3 }),
            PopupShowTarget::Window { id: 3 },
        ),
        (
            json!({ "kind": "focused_window" }),
            PopupShowTarget::FocusedWindow,
        ),
        (
            json!({ "kind": "workspace", "id": 4 }),
            PopupShowTarget::Workspace { id: 4 },
        ),
        (
            json!({ "kind": "pane", "id": 5 }),
            PopupShowTarget::Pane { id: 5 },
        ),
        (
            json!({ "kind": "tab", "id": 6 }),
            PopupShowTarget::Tab { id: 6 },
        ),
        (
            json!({ "kind": "surface", "id": 42 }),
            PopupShowTarget::Surface { id: 42 },
        ),
    ];
    for (wire, target) in cases {
        let params = json!({ "popup_id": "p", "target": wire, "context": { "a": 1 } });
        let p: PopupShowParams = serde_json::from_value(params.clone()).expect("show params");
        assert_eq!(p.target, target);
        assert_eq!(serde_json::to_value(&p).expect("serialize"), params);
    }
    let no_context: PopupShowParams = serde_json::from_value(json!({
        "popup_id": "p", "target": { "kind": "surface", "id": 1 }
    }))
    .expect("context is optional");
    assert!(no_context.context.is_null());
    // 대상은 필수다. 생략하거나 모르는 종류면 읽지 않는다.
    for bad in [
        json!({ "popup_id": "p" }),
        json!({ "popup_id": "p", "target": null }),
        json!({ "popup_id": "p", "target": { "kind": "surface" } }),
        json!({ "popup_id": "p", "target": { "kind": "somewhere" } }),
    ] {
        assert!(
            serde_json::from_value::<PopupShowParams>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        serde_json::to_value(PopupShowResult { instance_id: 9 }).expect("serialize"),
        json!({ "instance_id": 9 })
    );
    assert_eq!(METHOD_POPUP_DECLARE, "popup.declare");
    assert_eq!(METHOD_POPUP_SHOW, "popup.show");
}

#[test]
fn a_target_names_its_scope_kind() {
    assert_eq!(
        PopupShowTarget::Window { id: 1 }.scope_kind(),
        PopupScopeKind::Window
    );
    assert_eq!(
        PopupShowTarget::FocusedWindow.scope_kind(),
        PopupScopeKind::Window
    );
    assert_eq!(
        PopupShowTarget::Surface { id: 1 }.scope_kind(),
        PopupScopeKind::Surface
    );
    assert_eq!(
        PopupShowTarget::Tab { id: 1 }.scope_kind(),
        PopupScopeKind::Tab
    );
}
