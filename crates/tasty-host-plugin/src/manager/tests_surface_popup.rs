//! surface 범위 이벤트 팝업은 대상 소유 확인을 기다리고, 대상이 없으면 열리지 않는다.
//! 창 범위 이벤트 팝업은 바로 열린다.

use std::sync::Arc;

use tasty_plugin_protocol::{EventEnvelope, EventMeta, EventOrigin, EventScope};
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::PluginManager;
use crate::process::PluginProcess;

const PLUGIN: &str = "com.example.viewer";
const KEY: &str = "com.example.viewer.confirm";

fn manager(scope_line: &str) -> PluginManager {
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

[[contributes.popup]]
id = "confirm"
trigger = {{ kind = "event", event_key = "{KEY}" }}
rendering = "egui-mesh"
{scope_line}
"#,
        api = tasty_plugin_manifest::HOST_API_VERSION,
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

fn surface_payload() -> serde_json::Value {
    serde_json::json!({ "surface_id": 7, "path": "/a.md" })
}

#[test]
fn a_surface_popup_with_a_surface_event_waits_for_the_owner_check() {
    let mut mgr = manager(r#"scope = "surface""#);
    mgr.emit_host_event(KEY, &surface_payload(), EventScope::Surface);
    assert_eq!(
        mgr.popup_instances().count(),
        0,
        "소유 확인 전에는 열지 않는다"
    );
    let pending = mgr.take_pending_surface_popups();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].plugin_id, PLUGIN);
    assert_eq!(pending[0].popup_id, "confirm");
    assert_eq!(pending[0].surface_id, 7);
    assert_eq!(pending[0].publisher, None);
    assert!(mgr.take_pending_surface_popups().is_empty());
}

#[test]
fn a_plugin_publish_records_the_publisher() {
    let mut mgr = manager(r#"scope = "surface""#);
    mgr.event_bus
        .set_plugin_permissions(PLUGIN, vec![], vec![format!("{PLUGIN}.*")]);
    mgr.publish_and_dispatch(
        PLUGIN,
        EventEnvelope {
            key: KEY.into(),
            payload: surface_payload(),
            meta: EventMeta {
                trace_id: "t".into(),
                hop: 0,
                origin: EventOrigin::Plugin {
                    plugin_id: PLUGIN.into(),
                },
                scope: EventScope::Surface,
            },
        },
    );
    let pending = mgr.take_pending_surface_popups();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].publisher.as_deref(), Some(PLUGIN));
}

#[test]
fn a_window_popup_opens_at_once() {
    let mut mgr = manager("");
    mgr.emit_host_event(KEY, &surface_payload(), EventScope::Surface);
    assert!(mgr.take_pending_surface_popups().is_empty());
    assert_eq!(mgr.popup_instances().count(), 1);
}

/// surface 선언 팝업은 대상이 없는 이벤트로 열리지 않는다. 창 범위로 바꾸지 않는다.
#[test]
fn a_surface_popup_without_a_surface_target_does_not_open() {
    for (payload, scope) in [
        (surface_payload(), EventScope::System),
        (serde_json::json!({ "path": "/a.md" }), EventScope::Surface),
    ] {
        let mut mgr = manager(r#"scope = "surface""#);
        mgr.emit_host_event(KEY, &payload, scope);
        assert!(mgr.take_pending_surface_popups().is_empty(), "{scope:?}");
        assert_eq!(mgr.popup_instances().count(), 0, "{scope:?}");
    }
}

fn publish_from(
    mgr: &mut PluginManager,
    publisher: &str,
    payload: serde_json::Value,
    scope: EventScope,
) {
    mgr.event_bus
        .set_plugin_permissions(publisher, vec![], vec![format!("{PLUGIN}.*")]);
    mgr.publish_and_dispatch(
        publisher,
        EventEnvelope {
            key: KEY.into(),
            payload,
            meta: EventMeta {
                trace_id: "t".into(),
                hop: 0,
                origin: EventOrigin::Plugin {
                    plugin_id: publisher.into(),
                },
                scope,
            },
        },
    );
}

#[test]
fn another_plugins_event_opens_no_surface_popup_with_or_without_a_target() {
    const OTHER: &str = "com.example.other";
    for (payload, scope) in [
        (surface_payload(), EventScope::Surface),
        (surface_payload(), EventScope::System),
        (serde_json::json!({ "path": "/a.md" }), EventScope::Surface),
    ] {
        let mut mgr = manager(r#"scope = "surface""#);
        publish_from(&mut mgr, OTHER, payload, scope);
        assert!(mgr.take_pending_surface_popups().is_empty());
        assert_eq!(mgr.popup_instances().count(), 0, "{scope:?}");
    }
}

#[test]
fn the_owners_own_event_without_a_target_does_not_open() {
    let mut mgr = manager(r#"scope = "surface""#);
    publish_from(
        &mut mgr,
        PLUGIN,
        serde_json::json!({ "path": "/a.md" }),
        EventScope::System,
    );
    assert!(mgr.take_pending_surface_popups().is_empty());
    assert_eq!(mgr.popup_instances().count(), 0);
}

#[test]
fn a_window_popup_keeps_opening_from_another_plugins_event() {
    let mut mgr = manager("");
    publish_from(
        &mut mgr,
        "com.example.other",
        surface_payload(),
        EventScope::Surface,
    );
    assert_eq!(mgr.popup_instances().count(), 1);
}
