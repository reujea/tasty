//! 매니페스트 팝업 변환과 공용 선언 검사.

use super::*;
use tasty_plugin_protocol::popup_decl::{
    PopupChrome, PopupDragHandle, PopupEscape, PopupFit, PopupShadow,
};

fn parse(toml_body: &str) -> PopupContribute {
    toml::from_str(toml_body).expect("popup contribute")
}

fn size(w: f32, h: f32) -> PopupSize {
    PopupSize {
        width: LogicalPx(w),
        height: LogicalPx(h),
    }
}

/// 공식 플러그인과 mesh-demo의 매니페스트 선언이 바뀌는 값.
#[test]
fn bundled_manifest_popups_convert_to_the_current_behaviour() {
    let cases = [
        (
            "markdown large-file-confirm",
            r#"
                id = "large-file-confirm"
                trigger = { kind = "event", event_key = "com.tasty.markdown.large_file_confirm" }
                rendering = "egui-mesh"
                size_hint = { width = 360, height = 150 }
                anchor = "screen-center"
                scope = "surface"
                dismiss_on_outside_click = true
            "#,
            PopupScopeKind::Surface,
            size(360.0, 150.0),
        ),
        (
            "markdown file-open",
            r#"
                id = "file-open"
                trigger = { kind = "event", event_key = "com.tasty.markdown.file_open" }
                rendering = "egui-mesh"
                size_hint = { width = 420, height = 170 }
                anchor = "screen-center"
                scope = "surface"
                dismiss_on_outside_click = true
            "#,
            PopupScopeKind::Surface,
            size(420.0, 170.0),
        ),
        (
            "git-viewer viewer",
            r#"
                id = "viewer"
                trigger = { kind = "ipc" }
                rendering = "egui-mesh"
                size_hint = { width = 960, height = 640 }
                anchor = "screen-center"
                dismiss_on_outside_click = true
            "#,
            PopupScopeKind::Window,
            size(960.0, 640.0),
        ),
        (
            "clipboard-viewer viewer",
            r#"
                id = "viewer"
                trigger = { kind = "ipc" }
                rendering = "egui-mesh"
                size_hint = { width = 480, height = 360 }
                anchor = "screen-center"
                dismiss_on_outside_click = true
            "#,
            PopupScopeKind::Window,
            size(480.0, 360.0),
        ),
        (
            "mesh-demo demo",
            r#"
                id = "demo"
                trigger = { kind = "ipc" }
                rendering = "egui-mesh"
                size_hint = { width = 480, height = 320 }
                anchor = "screen-center"
                dismiss_on_outside_click = true
            "#,
            PopupScopeKind::Window,
            size(480.0, 320.0),
        ),
    ];
    for (name, body, scope_kind, expected_size) in cases {
        let c = parse(body);
        let d = c.to_decl();
        assert_eq!(d.scope_kind, scope_kind, "{name}");
        assert_eq!(d.size, expected_size, "{name}");
        assert_eq!(d.anchor, DeclAnchor::ScopeCenter, "{name}");
        assert_eq!(d.outside_click, PopupOutsideClick::Close, "{name}");
        assert_eq!(d.instances, PopupInstances::Single, "{name}");
        assert_eq!(d.keyboard, PopupKeyboard::Text, "{name}");
        assert!(d.covers_scope, "{name}: every plugin popup lays a scrim");
        assert!(d.focus_on_open, "{name}");
        assert_eq!(d.escape, PopupEscape::Close, "{name}");
        assert_eq!(d.chrome, PopupChrome::Headless, "{name}");
        assert_eq!(d.drag_handle, PopupDragHandle::None, "{name}");
        assert_eq!(d.shadow, PopupShadow::Modal, "{name}");
        assert_eq!(d.fit, PopupFit::Collapse, "{name}");
        assert!(validate_popup_decl(&c.id, &d).is_ok(), "{name}");
    }
}

#[test]
fn omitted_fields_convert_to_the_manifest_defaults() {
    let d = parse(
        r#"
            id = "bare"
            trigger = { kind = "ipc" }
        "#,
    )
    .to_decl();
    assert_eq!(d.size, DEFAULT_POPUP_SIZE);
    assert_eq!(d.scope_kind, PopupScopeKind::Window);
    assert_eq!(d.anchor, DeclAnchor::ScopeCenter);
    assert_eq!(d.outside_click, PopupOutsideClick::Close);
}

#[test]
fn cursor_anchor_and_kept_outside_clicks_convert() {
    let d = parse(
        r#"
            id = "near-pointer"
            trigger = { kind = "ipc" }
            anchor = "cursor"
            dismiss_on_outside_click = false
        "#,
    )
    .to_decl();
    assert_eq!(d.anchor, DeclAnchor::Cursor);
    assert_eq!(d.outside_click, PopupOutsideClick::Ignore);
    let d = parse(
        r#"
            id = "centered"
            trigger = { kind = "ipc" }
            anchor = "active-surface-center"
        "#,
    )
    .to_decl();
    assert_eq!(d.anchor, DeclAnchor::ScopeCenter);
}

#[test]
fn the_shared_check_refuses_bad_ids_sizes_and_points() {
    let ok = PopupDecl::plugin(PopupScopeKind::Surface, size(10.0, 10.0));
    assert!(validate_popup_decl("large-file-confirm", &ok).is_ok());
    for bad in ["", "Upper", "1st", "under_score", &"a".repeat(65)] {
        let err = validate_popup_decl(bad, &ok).expect_err(bad);
        assert!(err.contains("invalid popup id"), "{err}");
    }
    let mut zero = ok;
    zero.size = size(0.0, 10.0);
    assert!(
        validate_popup_decl("p", &zero)
            .expect_err("zero size")
            .contains("size width and height must be positive")
    );
    let mut point = ok;
    point.anchor = DeclAnchor::AtPoint;
    assert!(
        validate_popup_decl("p", &point)
            .expect_err("at_point")
            .contains("at_point")
    );
}
