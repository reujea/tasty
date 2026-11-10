//! 행 판정을 실제 `App` 의 행 상태로 잰다. 새 프로세스에서 혼자 돈다(`test_rig`).

use super::*;
use crate::adapters::ui::popup::remote_row::popup_kind;
use crate::app::auto_attach::test_rig::{ROW, app_with_saved_row, event_loop};
use tasty_ui_widgets::{RemoteRowPopupKind, RemoteRowState};

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

/// Given 사용자가 끊은 저장 행, When 팝업 Reconnect 로 다시 붙은 뒤 끊겨 재연결 시간이 지나면,
/// Then 팝업은 GaveUp 이고 그 행을 쓰는 요청은 user_disconnected 로 거절되지 않는다.
#[test]
fn an_explicit_reconnect_clears_the_user_disconnect_mark() {
    alone!("an_explicit_reconnect_clears_the_user_disconnect_mark");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    assert!(app.disconnect_mirror_workspace(ROW));
    assert!(app.remote.user_disconnected.contains(&ROW));

    app.connect_saved_row(ROW, true);
    assert!(!app.remote.user_disconnected.contains(&ROW));

    // 다시 붙었다가 끊기고 재연결 시간이 지났다.
    app.remote.active.remove(&ROW);
    app.remote.sessions[0].state.phase = SessionState::Reconnecting;
    app.enter_disconnected(ROW);
    let name = app.row_state_name(ROW);
    assert_eq!(name, "disconnected");
    let view = app.row_view(ROW, name);
    assert_eq!(
        popup_kind(RemoteRowState::Disconnected, view, false),
        RemoteRowPopupKind::GaveUp
    );
    let decision = decide(app.row_facts(ROW));
    assert!(
        !matches!(decision, RowUse::Refuse(ref refusal) if refusal.reason == "user_disconnected"),
        "{decision:?}"
    );
}

fn remote_attach(app: &mut App) -> serde_json::Value {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "remote.attach",
        "params": {"profile": "rv", "remote_workspace": 3},
    }))
    .unwrap();
    let cmd = crate::ipc::server::IpcCommand::new(request, tx);
    app.attach_to_saved_row(&cmd, &crate::ipc::caller::CallerContext::local(), ROW, 3);
    let response = rx.try_recv().expect("an immediate response");
    serde_json::to_value(response).unwrap()["result"].clone()
}

/// Given 연결을 시도 중인 저장 행, When `remote.attach` 가 그 시도에 합류하고 시도가 끝내 실패하면,
/// Then 합류한 요청의 run 도 실패로 끝나 CLI 가 rc 1 을 낸다.
#[test]
fn a_joined_remote_attach_gets_the_final_failure() {
    alone!("a_joined_remote_attach_gets_the_final_failure");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Reconnecting));
    app.remote.active.insert(ROW);
    let result = remote_attach(&mut app);
    let run = result["run"].as_u64().expect("a joined request has a run");

    app.advance_row_waits();
    assert_eq!(
        crate::request_runs::lookup(run, None).unwrap()["state"],
        "running"
    );

    app.enter_disconnected(ROW);
    app.advance_row_waits();
    let ended = crate::request_runs::lookup(run, None).unwrap();
    assert_eq!(ended["state"], "failed", "{ended}");
}

/// Given 연결을 시도 중인 저장 행, When 합류한 뒤 그 시도가 연결되면, Then 합류한 run 은 성공이다.
#[test]
fn a_joined_remote_attach_gets_the_final_success() {
    alone!("a_joined_remote_attach_gets_the_final_success");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Reconnecting));
    app.remote.active.insert(ROW);
    let run = remote_attach(&mut app)["run"].as_u64().unwrap();
    app.remote.active.remove(&ROW);
    app.remote.sessions[0].state.phase = SessionState::Connected;
    app.advance_row_waits();
    assert_eq!(
        crate::request_runs::lookup(run, None).unwrap()["state"],
        "succeeded"
    );
}
