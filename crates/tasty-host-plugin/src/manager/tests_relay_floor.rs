//! EventBus의 재발행 hop 규칙이 매니저의 dispatch·응답·publish 처리에도 적용되는지 확인한다.

use std::sync::Arc;

use tasty_plugin_protocol::{EventEnvelope, EventMeta, EventOrigin, EventScope};
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::PluginManager;
use crate::process::PluginProcess;
use crate::protocol::PluginResponse;

const PLUGIN: &str = "com.example.relay";

fn relay(hop: u8) -> EventEnvelope {
    EventEnvelope {
        key: format!("{PLUGIN}.relay"),
        payload: serde_json::Value::Null,
        meta: EventMeta {
            trace_id: "forged-fresh".into(),
            hop,
            origin: EventOrigin::Plugin {
                plugin_id: PLUGIN.into(),
            },
            scope: EventScope::System,
        },
    }
}

fn last_hop(mgr: &PluginManager) -> u8 {
    let got = mgr
        .event_bus
        .fetch(0, crate::event_bus::EVENT_RING_CAPACITY, None);
    got.events.last().expect("링이 비었다").1.meta.hop
}

#[test]
fn a_publish_between_a_dispatch_and_its_answer_is_raised_and_one_after_is_not() {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let (proc, rx) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), proc);
    mgr.event_bus.set_plugin_declarations(
        PLUGIN,
        vec!["agent.*".into()],
        vec![format!("{PLUGIN}.*")],
    );
    mgr.event_bus
        .subscribe_plugin(PLUGIN, 1, "agent.*".into())
        .unwrap();

    mgr.emit_host_event(
        "agent.task_finished",
        &serde_json::json!({ "state": "succeeded" }),
        EventScope::System,
    );
    let sent = rx
        .try_recv()
        .expect("구독한 plugin 에 event.dispatch 가 나가야 한다");
    assert_eq!(sent.method, tasty_plugin_protocol::METHOD_EVENT_DISPATCH);

    // 응답 전 — plugin 이 hop 0 으로 적어 보내도 host 가 1 로 올린다.
    mgr.route_plugin_event_publish(PLUGIN, relay(0));
    assert_eq!(last_hop(&mgr), 1);

    // dispatch 응답 후에는 그 요청의 hop 하한을 적용하지 않는다.
    mgr.handle_plugin_response(
        PLUGIN,
        PluginResponse {
            id: sent.id,
            result: Some(serde_json::Value::Null),
            error: None,
            error_code: None,
        },
    );
    mgr.route_plugin_event_publish(PLUGIN, relay(0));
    assert_eq!(last_hop(&mgr), 0);
}

const EXTENSION: &str = "com.example.relay-ext";

fn manifest(
    id: &str,
    extends: Option<tasty_plugin_manifest::ExtendsDecl>,
) -> tasty_plugin_manifest::Manifest {
    tasty_plugin_manifest::Manifest {
        manifest_version: 1,
        id: id.to_string(),
        name: id.to_string(),
        version: "1.0.0".to_string(),
        authors: vec![],
        description: String::new(),
        homepage: String::new(),
        api_version: "1".to_string(),
        min_tasty_version: None,
        entry: tasty_plugin_manifest::Entry::Process {
            command: "x".to_string(),
            args: vec![],
        },
        surface_kinds: vec![],
        event_subscribe: vec![],
        event_publish: vec![],
        events_emitted: vec![],
        contributes: tasty_plugin_manifest::Contributes::default(),
        extends,
        lang_dir: "lang".to_string(),
        bundle: true,
    }
}

/// pre-event hook 을 거치는 publish 는 fan-out 이 hook 응답 뒤로 밀린다. 그 사이에
/// dispatch 응답이 오면 하한을 걸 기록이 사라지므로, 하한은 **도착한 순간**에 걸려
/// 있어야 한다 — hook 응답 뒤에 판정하면 hop 0 이 그대로 나간다(docs/reference/event-catalog.md#재발행과-응답).
#[test]
fn a_publish_held_by_a_pre_event_hook_keeps_the_floor_it_arrived_with() {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let (proc, rx) = PluginProcess::stub_with_request_rx(PLUGIN);
    mgr.processes.insert(PLUGIN.into(), proc);
    let (ext_proc, ext_rx) = PluginProcess::stub_with_request_rx(EXTENSION);
    mgr.processes.insert(EXTENSION.into(), ext_proc);
    mgr.event_bus.set_plugin_declarations(
        PLUGIN,
        vec!["agent.*".into()],
        vec![format!("{PLUGIN}.*")],
    );
    mgr.event_bus
        .subscribe_plugin(PLUGIN, 1, "agent.*".into())
        .unwrap();

    let target = manifest(PLUGIN, None);
    let ext = manifest(
        EXTENSION,
        Some(tasty_plugin_manifest::ExtendsDecl {
            plugin_id: PLUGIN.to_string(),
            version_req: ">=1.0.0".to_string(),
            api_version: "1".to_string(),
            pre_event: vec![tasty_plugin_manifest::EventHookDecl {
                event: format!("{PLUGIN}.relay"),
                modifies: vec![],
                mode: tasty_plugin_manifest::HookMode::Observe,
                timeout_ms: 60_000,
            }],
            post_event: vec![],
            pre_ipc: vec![],
            post_ipc: vec![],
        }),
    );
    mgr.extensions.recompute(&[&target, &ext], &|_| false);
    for m in [target, ext] {
        mgr.packages.push(tasty_plugin_manifest::PluginPackage {
            dir: std::path::PathBuf::new(),
            manifest: m,
        });
    }

    mgr.emit_host_event(
        "agent.task_finished",
        &serde_json::json!({ "state": "succeeded" }),
        EventScope::System,
    );
    let dispatch = rx
        .try_recv()
        .expect("구독한 plugin 에 event.dispatch 가 나가야 한다");

    // 응답 전 도착 — hook 이 붙잡는다. 아직 링에 안 들어간다.
    mgr.route_plugin_event_publish(PLUGIN, relay(0));
    let hook = ext_rx
        .try_recv()
        .expect("extension 에 pre-event hook 호출이 나가야 한다");
    assert!(
        mgr.event_bus
            .fetch(0, crate::event_bus::EVENT_RING_CAPACITY, None)
            .events
            .iter()
            .all(|(_, e)| e.key != format!("{PLUGIN}.relay")),
        "hook 응답 전에는 fan-out 되지 않는다"
    );

    // dispatch 응답이 hook 응답보다 먼저 온다 — 버스의 기록은 여기서 사라진다.
    let answer = |id| PluginResponse {
        id,
        result: Some(serde_json::Value::Null),
        error: None,
        error_code: None,
    };
    mgr.handle_plugin_response(PLUGIN, answer(dispatch.id));
    mgr.handle_plugin_response(EXTENSION, answer(hook.id));

    let got = mgr
        .event_bus
        .fetch(0, crate::event_bus::EVENT_RING_CAPACITY, None);
    let (_, last) = got.events.last().expect("링이 비었다");
    assert_eq!(last.key, format!("{PLUGIN}.relay"));
    assert_eq!(
        last.meta.hop, 1,
        "도착할 때 건 하한이 hook 을 지나도 남는다"
    );
}
