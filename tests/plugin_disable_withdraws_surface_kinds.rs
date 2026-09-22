//! plugin 을 끄면 그 plugin 이 등록한 surface kind 가 재부팅 없이 철회된다
//! ([ADR-0534](../docs/adr/0534-a-disabled-plugins-surface-kinds-are-withdrawn-not-erased.md)).
//!
//! 단위 시험이 못 보는 자리 — `plugin.disable` 이 registry 를 실제로 거두는 배선 — 을
//! 실제 데몬 상대로 잰다. 헤드리스 조합 전용이다: 전용 인스턴스가 필요하고(plugin 을 켜고
//! 끄므로 공유 인스턴스의 전제를 바꾼다), gui 조합의 전용 인스턴스는 창을 띄운다. 두 조합이
//! 같은 `dispatch_lifecycle_toggle` 을 부르므로 배선의 모양은 같다.
#![cfg(not(feature = "gui"))]

mod common;

use std::time::{Duration, Instant};

use common::TastyInstance;
use serde_json::{Value, json};

const MARKDOWN_PLUGIN: &str = "com.tasty.markdown";

fn kinds(server: &TastyInstance) -> Vec<String> {
    server.call("surface.kinds", json!({}))["kinds"]
        .as_array()
        .expect("surface.kinds returns kinds")
        .iter()
        .filter_map(|k| k["kind"].as_str().map(str::to_string))
        .collect()
}

fn create_markdown_tab(server: &TastyInstance, pane_id: u64, file: &std::path::Path) -> Value {
    server.call_raw(
        "tab.create",
        json!({ "pane_id": pane_id, "type": "markdown", "file": file.to_string_lossy() }),
    )
}

fn surface_ids(server: &TastyInstance) -> Vec<u64> {
    let list = server.call("surface.list", json!({}));
    let arr = list
        .as_array()
        .or_else(|| list["surfaces"].as_array())
        .cloned()
        .unwrap_or_default();
    arr.iter()
        .filter_map(|s| s["id"].as_u64().or_else(|| s["surface_id"].as_u64()))
        .collect()
}

#[test]
fn disabling_a_plugin_withdraws_its_surface_kinds_and_keeps_open_surfaces() {
    let server = TastyInstance::spawn();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("README.md");
    std::fs::write(&file, "# doc\n").expect("write doc");
    let pane_id = server.first_pane_id();

    // 켜진 상태: kind 지목 생성이 소유자를 띄우고 kind 를 등록한다(ADR-0259).
    let opened = create_markdown_tab(&server, pane_id, &file);
    let open_sid = opened["result"]["surface_id"].as_u64().unwrap_or_else(|| {
        panic!(
            "켜진 markdown 으로 탭을 못 만들었다: {opened}{}",
            common::bundle_staging_note()
        )
    });
    assert!(kinds(&server).iter().any(|k| k == "markdown"));

    server.call("plugin.disable", json!({ "id": MARKDOWN_PLUGIN }));

    assert!(
        !kinds(&server).iter().any(|k| k == "markdown"),
        "끈 plugin 의 kind 가 surface.kinds 에 남았다"
    );
    let refused = create_markdown_tab(&server, pane_id, &file);
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("is disabled or removed, or has not reconnected")
            && message.contains(MARKDOWN_PLUGIN),
        "끈 plugin 의 kind 로 만든 요청은 제공 plugin 이 꺼졌거나 아직 다시 연결되지 않았다는 사유로 거절돼야 한다: {refused}"
    );
    assert!(
        surface_ids(&server).contains(&open_sid),
        "이미 열린 surface 는 그대로 남아야 한다"
    );

    // 다시 켜면 hello 가 kind 를 다시 등록해 철회가 풀린다.
    server.call("plugin.enable", json!({ "id": MARKDOWN_PLUGIN }));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let again = create_markdown_tab(&server, pane_id, &file);
        if again["result"]["surface_id"].as_u64().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "다시 켠 뒤에도 그 kind 로 만들 수 없다: {again}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
