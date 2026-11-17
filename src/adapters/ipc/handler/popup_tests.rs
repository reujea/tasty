//! `debug.popup.open`의 대상 검사. surface 선언 팝업은 살아 있는 `surface_id`가 있어야 연다.

use std::sync::Arc;

use super::handle_open;
use crate::plugin::PluginManager;

const MANIFEST: &str = r#"
manifest_version = 1
id = "com.example.popups"
name = "Popups"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[[contributes.popup]]
id = "on-surface"
trigger = { kind = "ipc" }
rendering = "egui-mesh"
anchor = "screen-center"
scope = "surface"

[[contributes.popup]]
id = "on-window"
trigger = { kind = "ipc" }
rendering = "egui-mesh"
anchor = "screen-center"
"#;

fn manager() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let manifest = toml::from_str(MANIFEST).expect("manifest");
    mgr.install_packages_for_test(vec![tasty_plugin_manifest::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/popups"),
        manifest,
    }]);
    mgr
}

/// 살아 있는 surface 는 7 하나다.
fn open(params: serde_json::Value) -> tasty_ipc::protocol::JsonRpcResponse {
    let mut mgr = manager();
    handle_open(Some(&mut mgr), serde_json::json!(1), &params, |sid| {
        sid == 7
    })
}

fn refusal(response: &tasty_ipc::protocol::JsonRpcResponse) -> (i32, String) {
    let e = response.error.as_ref().expect("refused");
    (e.code, e.message.clone())
}

#[test]
fn a_surface_popup_without_a_surface_id_is_refused() {
    let response = open(serde_json::json!({
        "plugin_id": "com.example.popups",
        "popup_id": "on-surface",
    }));
    assert_eq!(
        refusal(&response),
        (
            -32602,
            "popup 'com.example.popups/on-surface' is declared with surface scope; surface_id required"
                .into()
        )
    );
}

#[test]
fn a_surface_id_that_names_no_surface_is_refused() {
    let response = open(serde_json::json!({
        "plugin_id": "com.example.popups",
        "popup_id": "on-surface",
        "surface_id": 99,
    }));
    assert_eq!(refusal(&response), (-32602, "surface 99 not found".into()));
}

/// 대상 검사를 지난 요청은 열기까지 간다. 이 시험의 plugin 은 실행 중이 아니라 열기에서 거절된다.
#[test]
fn a_live_surface_or_a_window_popup_passes_the_target_check() {
    for params in [
        serde_json::json!({
            "plugin_id": "com.example.popups",
            "popup_id": "on-surface",
            "surface_id": 7,
        }),
        serde_json::json!({
            "plugin_id": "com.example.popups",
            "popup_id": "on-window",
        }),
    ] {
        let (_, message) = refusal(&open(params.clone()));
        assert!(
            message.ends_with("not found or plugin not running"),
            "{params}: {message}"
        );
    }
}
