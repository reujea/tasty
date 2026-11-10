//! `remote.connect`(명시적 연결 요청)를 실제 `App` 의 행 상태로 잰다. 새 프로세스에서 혼자 돈다(`test_rig`).

use super::*;
use crate::app::auto_attach::test_rig::{ROW, app_with_saved_row, event_loop};

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

/// 요청 하나를 받는 `IpcCommand` 와 그 응답을 받을 쪽.
fn command(
    method: &'static str,
    params: serde_json::Value,
) -> (
    crate::ipc::server::IpcCommand,
    std::sync::mpsc::Receiver<crate::ipc::protocol::JsonRpcResponse>,
) {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
    }))
    .unwrap();
    (crate::ipc::server::IpcCommand::new(request, tx), rx)
}

fn connect(app: &mut App, row: u32) -> serde_json::Value {
    let (cmd, rx) = command("remote.connect", serde_json::json!({ "workspace_id": row }));
    let caller = crate::ipc::caller::CallerContext::local();
    app.connect_saved_row_by_request(&cmd, &caller, row);
    serde_json::to_value(rx.try_recv().expect("an immediate response")).unwrap()
}

/// 같은 연결 대상(`rv`, 원격 3)의 `remote.attach` 가 저장 행에 합류한다.
fn attach(app: &mut App) -> serde_json::Value {
    let (cmd, rx) = command(
        "remote.attach",
        serde_json::json!({ "profile": "rv", "remote_workspace": 3 }),
    );
    let caller = crate::ipc::caller::CallerContext::local();
    app.attach_to_saved_row(&cmd, &caller, ROW, 3);
    serde_json::to_value(rx.try_recv().expect("an immediate response")).unwrap()
}

fn reason(response: &serde_json::Value) -> &str {
    response["error"]["data"]["reason"].as_str().unwrap_or("")
}

fn run_state(run: u64) -> String {
    crate::request_runs::lookup(run, None).unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// 행 `ROW` 를 쓰는 요청(`terminal.spawn`)을 보류 판정에 넣는다. 거절이면 바로 답이 온다.
fn hold_send(app: &mut App) -> std::sync::mpsc::Receiver<crate::ipc::protocol::JsonRpcResponse> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 9, "method": "terminal.spawn", "params": {"workspace": ROW},
    }))
    .unwrap();
    let decision = decide(app.row_facts(ROW));
    let held = Gated::Ipc {
        cmd: crate::ipc::server::IpcCommand::new(request, tx),
        progress: Default::default(),
    };
    app.hold_for_remote_row(held, decision);
    rx
}

/// Given 사용자가 끊은 저장 행, When 그 행을 쓰는 요청과 같은 대상의 `remote.attach` 가 오면,
/// Then 둘 다 `user_disconnected` 로 거절되고 메시지가 `remote.connect` 를 안내한다. 행은 붙지 않는다.
#[test]
fn a_user_disconnected_row_refuses_use_and_remote_attach() {
    alone!("a_user_disconnected_row_refuses_use_and_remote_attach");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    assert!(app.disconnect_mirror_workspace(ROW));

    let answered = serde_json::to_value(hold_send(&mut app).try_recv().unwrap()).unwrap();
    assert_eq!(reason(&answered), "user_disconnected", "{answered}");
    let message = answered["error"]["message"].as_str().unwrap();
    assert!(message.contains("remote.connect"), "{message}");
    assert!(
        message.contains("tasty remote connect --workspace"),
        "{message}"
    );

    let attached = attach(&mut app);
    assert_eq!(reason(&attached), "user_disconnected", "{attached}");
    assert!(!app.remote.active.contains(&ROW));
    assert!(app.remote.user_disconnected.contains(&ROW));
}

/// Given 사용자가 끊은 저장 행, When `remote.connect` 를 부르면, Then 끊음 표지가 풀리고 새 시도를 시작하며
/// run 을 그 시도에 싣는다. 그 사이 온 요청은 거절되지 않고 기다렸다가 연결되면 실행된다.
#[test]
fn remote_connect_clears_the_user_disconnect_and_a_held_request_runs_after_it() {
    alone!("remote_connect_clears_the_user_disconnect_and_a_held_request_runs_after_it");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    assert!(app.disconnect_mirror_workspace(ROW));

    let accepted = connect(&mut app, ROW);
    let result = &accepted["result"];
    assert_eq!(result["attaching"], true, "{accepted}");
    assert_eq!(result["workspace_id"], ROW);
    let run = result["run"].as_u64().unwrap();
    assert!(!app.remote.user_disconnected.contains(&ROW));
    assert!(
        app.remote.active.contains(&ROW),
        "a new attempt holds the row"
    );
    assert_eq!(run_state(run), "running");
    // run 은 새 시도에 실려 그 시도의 설치 결과로 끝난다.
    assert!(
        app.state
            .mirror_attempts
            .endpoint_snapshot()
            .iter()
            .any(|(_, target)| target.row == Some(ROW)
                && target.run.as_ref().is_some_and(|guard| guard.id() == run)),
        "the run rides on the new attempt"
    );

    let held = hold_send(&mut app);
    assert!(
        held.try_recv().is_err(),
        "the request waits for the connection"
    );

    // 시도가 붙었다.
    app.remote.sessions[0].state.phase = SessionState::Connected;
    app.remote.sessions[0]
        .transport
        .disconnected
        .store(false, std::sync::atomic::Ordering::Release);
    app.advance_row_waits();
    // 요청은 대기에서 나와 실행됐다. 응답은 실행 경로가 보내며 행 거절이 아니다.
    assert!(
        app.row_waits.waiting.is_empty(),
        "the request left the wait"
    );
    if let Ok(answered) = held.try_recv() {
        let answered = serde_json::to_value(answered).unwrap();
        assert_ne!(answered["error"]["code"], ERR_ROW_UNAVAILABLE, "{answered}");
    }
}

/// Given 저장된 원격 행이 아닌 workspace, 원격 workspace 가 없다고 확인된 행, 창이 닫힌 행,
/// When `remote.connect` 를 부르면, Then 각각 `not_a_saved_row`·`remote_workspace_missing`·`window_parked` 로
/// 바로 거절하고 연결을 시작하지 않는다.
#[test]
fn remote_connect_refuses_at_once_for_the_three_reasons() {
    alone!("remote_connect_refuses_at_once_for_the_three_reasons");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    let local = app
        .engines
        .all_sessions()
        .flat_map(|engine| {
            engine
                .core_state
                .local_workspaces()
                .iter()
                .map(|ws| ws.id)
                .collect::<Vec<_>>()
        })
        .next()
        .expect("a local workspace");
    let refused = connect(&mut app, local);
    assert_eq!(reason(&refused), "not_a_saved_row", "{refused}");
    assert_eq!(refused["error"]["code"], ERR_ROW_UNAVAILABLE);
    assert_eq!(reason(&connect(&mut app, 123_456)), "not_a_saved_row");

    app.on_first_connect_failed(
        ROW,
        &crate::app::auto_attach::test_rig::binding(),
        &anyhow::anyhow!("workspace attach rejected: workspace_not_found"),
    );
    let refused = connect(&mut app, ROW);
    assert_eq!(reason(&refused), "remote_workspace_missing", "{refused}");
    assert!(!app.remote.active.contains(&ROW));

    let mut parked = app_with_saved_row(&event_loop, None);
    let (state, _) = crate::state::tests::test_state();
    parked
        .engines
        .park(winit::window::WindowId::from(u64::MAX - 2), state)
        .expect("the row's window is parked");
    let refused = connect(&mut parked, ROW);
    assert_eq!(reason(&refused), "window_parked", "{refused}");
    assert!(!parked.remote.active.contains(&ROW));
}

/// Given 연결된 저장 행, When `remote.connect` 를 부르면, Then `attaching: false` 와 성공으로 끝난 run 이다.
#[test]
fn remote_connect_on_a_connected_row_succeeds_at_once() {
    alone!("remote_connect_on_a_connected_row_succeeds_at_once");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    let accepted = connect(&mut app, ROW);
    assert_eq!(accepted["result"]["attaching"], false, "{accepted}");
    let run = accepted["result"]["run"].as_u64().unwrap();
    assert_eq!(run_state(run), "succeeded");
}

/// Given 첫 연결을 시도 중인 저장 행, When `remote.connect` 를 부르면, Then 그 시도에 합류해 새 시도를 만들지
/// 않고, 시도가 실패하면 run 도 그 이유로 실패한다.
#[test]
fn remote_connect_joins_an_attempt_in_progress() {
    alone!("remote_connect_joins_an_attempt_in_progress");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    app.ensure_row_token(ROW);
    app.remote.active.insert(ROW);
    let accepted = connect(&mut app, ROW);
    assert_eq!(accepted["result"]["attaching"], true, "{accepted}");
    let run = accepted["result"]["run"].as_u64().unwrap();
    app.advance_row_waits();
    assert_eq!(run_state(run), "running");

    app.on_first_connect_failed(
        ROW,
        &crate::app::auto_attach::test_rig::binding(),
        &anyhow::anyhow!("workspace attach rejected: already_attached"),
    );
    app.advance_row_waits();
    let ended = crate::request_runs::lookup(run, None).unwrap();
    assert_eq!(ended["state"], "failed", "{ended}");
    assert!(ended.to_string().contains("occupied"), "{ended}");
}
