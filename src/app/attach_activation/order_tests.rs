use super::{PendingAttach, Target, begin, poll};
use crate::app::journal::JournalApplication;
use tasty_ipc::stream::StreamTag;
use tasty_ipc::stream_hub::StreamHub;

/// client 큐에 쌓인 Control 프레임의 (event, surface_id) 목록.
fn controls(rx: &tasty_ipc::stream_hub::SinkReceiver) -> Vec<(String, Option<u64>)> {
    let mut out = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        if frame.tag != StreamTag::Control {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_slice(&frame.payload).expect("control payload is JSON");
        out.push((
            value["event"].as_str().unwrap_or_default().to_string(),
            value["surface_id"].as_u64(),
        ));
    }
    out
}

/// Busy 타이머가 한 번 돌 때 서버가 하는 상태 전송.
fn busy_tick(session: &mut crate::runtime::engine_session::EngineSession, hub: &StreamHub) {
    let mut engine = session.borrow_mut();
    engine.forward_busy_activity(hub);
    engine.forward_attention(hub);
    engine.forward_surface_cwd(hub);
    engine.forward_surface_foreground(hub);
}

/// 점유를 잡고 descriptor를 보내기 전 사이에 Busy tick이 끼어도 client의 첫 Control은
/// descriptor다. descriptor 뒤 첫 tick에서 점유 터미널마다 busy·attention·cwd·전경 이름 최초 값이
/// 한 번씩 나간다. 준비 전 점유를 거르지 않으면 첫 Control이 activity가 되어 실패한다.
#[test]
fn a_busy_tick_before_the_descriptor_does_not_overtake_it() {
    let mut journal = JournalApplication::new(std::sync::Arc::new(|| {})).expect("journal");
    let (_, mut session) = crate::state::tests::test_state();
    let hub = StreamHub::new();
    let client = hub.alloc_id();
    let rx = hub.register(client);
    let id = session.id;
    let (workspace, terminals) = {
        let engine = session.borrow_mut();
        let workspace = engine.workspace_at(0).expect("fixture workspace").id;
        let terminals = engine.classify_attach_surfaces(workspace).terminals;
        (workspace, terminals)
    };
    assert!(!terminals.is_empty(), "fixture workspace has a terminal");

    let mut pending: Vec<PendingAttach> = Vec::new();
    begin(
        &mut pending,
        &journal,
        id,
        &mut session.borrow_mut(),
        Target::Workspace(workspace),
        client,
        &hub,
    );
    assert_eq!(pending.len(), 1, "attach waits for activation");
    busy_tick(&mut session, &hub);
    let before_ready = controls(&rx);

    poll(&mut pending, &mut journal, &mut [&mut session], &hub);
    assert!(pending.is_empty(), "live terminals are ready at once");
    busy_tick(&mut session, &hub);

    let mut frames = before_ready.clone();
    frames.extend(controls(&rx));
    assert_eq!(
        frames.first().map(|f| f.0.as_str()),
        Some("attached_workspace"),
        "{frames:?}"
    );
    assert!(
        before_ready.is_empty(),
        "준비 전 점유에는 상태 Control을 보내지 않는다"
    );
    for event in ["activity", "attention", "cwd", "foreground"] {
        for sid in &terminals {
            let count = frames
                .iter()
                .filter(|f| f.0 == event && f.1 == Some(u64::from(*sid)))
                .count();
            assert_eq!(
                count, 1,
                "{event} for {sid} once after the descriptor: {frames:?}"
            );
        }
    }
}
