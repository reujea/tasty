//! `remote.disconnect` 를 실제 `App` 의 행 상태로 잰다. 새 프로세스에서 혼자 돈다(`test_rig`).

use super::*;
use crate::app::auto_attach::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use std::collections::HashMap;

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

/// 에이전트 mirror 로 쓰는 local workspace id. 시험 장치의 다른 id 와 겹치지 않는다.
const AGENT_MIRROR: u32 = 9_100;

/// 시험 장치의 행 세션은 기본 원격 id 를 갖는다. 실제 저장 행처럼 binding 의 원격 id 로 맞춘다.
fn saved_row_app(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    phase: Option<SessionState>,
) -> App {
    let mut app = app_with_saved_row(event_loop, phase);
    for session in &mut app.remote.sessions {
        session.state.remote_workspace = binding().remote_workspace;
    }
    app
}

fn disconnect(app: &mut App, workspace_id: u32) -> serde_json::Value {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "remote.disconnect",
        "params": {"workspace_id": workspace_id},
    }))
    .unwrap();
    let cmd = crate::ipc::server::IpcCommand::new(request, tx);
    app.ipc_dispatch_remote_disconnect(&cmd);
    serde_json::to_value(rx.try_recv().expect("an immediate response")).unwrap()
}

fn row_token(app: &App) -> Option<std::sync::Arc<()>> {
    let engine = app.saved_remote_row_engine(ROW).expect("the row engine");
    app.engines
        .get(engine)
        .and_then(|session| session.remote.row_tokens.get(&ROW).cloned())
}

fn assert_disconnected_now(response: &serde_json::Value, previous_state: &str) {
    let result = &response["result"];
    assert_eq!(result["disconnected"], true, "{response}");
    assert_eq!(result["already_disconnected"], false, "{response}");
    assert_eq!(result["previous_state"], previous_state, "{response}");
    assert_eq!(result["workspace_id"], ROW, "{response}");
    assert_eq!(result["remote_workspace"], binding().remote_workspace);
}

fn assert_already_disconnected(response: &serde_json::Value, previous_state: &str) {
    let result = &response["result"];
    assert_eq!(result["disconnected"], false, "{response}");
    assert_eq!(result["already_disconnected"], true, "{response}");
    assert_eq!(result["previous_state"], previous_state, "{response}");
    assert_eq!(result["workspace_id"], ROW, "{response}");
    assert_eq!(result["remote_workspace"], binding().remote_workspace);
}

/// Given 연결된 저장 행, When `remote.disconnect` 를 보내면, Then 이번 호출이 끊었다고 답하고 행은
/// 사용자가 끊은 `disconnected` 다. When 같은 요청을 다시 보내면(닫기 회신을 기다리던 행),
/// Then 이미 끊겼다고 답하고 `Detach` 를 다시 보내지 않으며 행 토큰·재연결 항목·세션 단계가 그대로다.
/// 닫기 결과 안내 토스트는 `Detach` 전송과 같은 분기에서 건너뛴다(이 장치에는 토스트를 받을 창이 없다).
#[test]
fn a_second_disconnect_of_a_saved_row_answers_already_disconnected_and_changes_nothing() {
    alone!("a_second_disconnect_of_a_saved_row_answers_already_disconnected_and_changes_nothing");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, Some(SessionState::Connected));

    assert_disconnected_now(&disconnect(&mut app, ROW), "connected");
    assert_eq!(app.row_state_name(ROW), "disconnected");
    assert!(app.remote.user_disconnected.contains(&ROW));

    // 보낸 프레임을 받는 세션으로 바꿔 두 번째 호출이 원격에 무엇을 보내는지 본다.
    let (mut watched, frames) =
        crate::app::attach_client::tests::test_session_with_frames(ROW, HashMap::new());
    watched.state.saved_row = Some(ROW);
    watched.state.phase = SessionState::Disconnected;
    watched.state.remote_workspace = binding().remote_workspace;
    crate::app::attach_client::workspace_close::note_sent(&mut watched, 7);
    app.remote.sessions[0] = watched;
    let token = row_token(&app);
    let reconnect = app.remote.reconnect.get(&ROW).map(|entry| entry.given_up);

    assert_already_disconnected(&disconnect(&mut app, ROW), "disconnected");
    assert!(frames.try_recv().is_err(), "no second Detach");
    let after = row_token(&app);
    assert!(
        match (&token, &after) {
            (Some(before), Some(after)) => std::sync::Arc::ptr_eq(before, after),
            (None, None) => true,
            _ => false,
        },
        "the row token is not replaced"
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|entry| entry.given_up),
        reconnect
    );
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert!(app.remote.user_disconnected.contains(&ROW));
}

/// Given 재연결 중인 저장 행, When 끊기를 보내면, Then 이번 호출이 끊었고 행은 `disconnected` 다.
#[test]
fn disconnecting_a_reconnecting_row_disconnects_it() {
    alone!("disconnecting_a_reconnecting_row_disconnects_it");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, Some(SessionState::Reconnecting));

    assert_disconnected_now(&disconnect(&mut app, ROW), "reconnecting");
    assert_eq!(app.row_state_name(ROW), "disconnected");
}

/// Given 첫 연결 중이고 아직 시도가 큐에 없는 저장 행, When 끊기를 보내면, Then 상태를 시도 취소 전에 읽어
/// `previous_state: "connecting"` 으로 끊었다고 답하고 연결 시도 표지가 지워진다.
#[test]
fn a_connecting_row_without_queued_attempts_is_disconnected_not_already() {
    alone!("a_connecting_row_without_queued_attempts_is_disconnected_not_already");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, None);
    app.remote.active.insert(ROW);

    assert_disconnected_now(&disconnect(&mut app, ROW), "connecting");
    assert!(!app.remote.active.contains(&ROW));
    assert!(app.remote.user_disconnected.contains(&ROW));
}

/// Given 재시작 뒤 아직 붙지 않은 저장 행, When 끊기를 보내면, Then 이미 끊겼다고 답하되 끊음 표지를 걸어
/// 그 행을 쓰는 요청이 자동으로 연결하지 않고 `user_disconnected` 로 거절된다.
#[test]
fn a_not_connected_row_answers_already_disconnected_and_keeps_the_user_mark() {
    alone!("a_not_connected_row_answers_already_disconnected_and_keeps_the_user_mark");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, None);

    assert_already_disconnected(&disconnect(&mut app, ROW), "not_connected");
    assert!(app.remote.user_disconnected.contains(&ROW));
    let decision = decide(app.row_facts(ROW));
    assert!(
        matches!(decision, RowUse::Refuse(ref refusal) if refusal.reason == "user_disconnected"),
        "{decision:?}"
    );
}

/// Given 첫 연결에 실패한 저장 행(`failed`)과 다른 곳에서 원격 workspace 가 닫힌 행, When 끊기를 보내면,
/// Then 둘 다 이미 끊겼다고 답한다.
#[test]
fn failed_and_closed_elsewhere_rows_answer_already_disconnected() {
    alone!("failed_and_closed_elsewhere_rows_answer_already_disconnected");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, None);
    app.remote.pending_reactivation.insert(ROW);
    assert_already_disconnected(&disconnect(&mut app, ROW), "failed");

    let mut app = saved_row_app(&event_loop, Some(SessionState::Disconnected));
    app.remote.refusals.note(
        ROW,
        tasty_remote::refusal::RowNoticeKind::RemoteWorkspaceMissing,
        &binding(),
    );
    assert!(app.row_closed_elsewhere(ROW));
    assert_already_disconnected(&disconnect(&mut app, ROW), "disconnected");
}

/// Given 에이전트 mirror(저장 행이 아닌 attach 세션), When 끊기를 보내면, Then 이번 호출이 끊었고 세션이 사라진다.
/// When 같은 id 로 다시 보내면, Then `workspace_not_found` 오류다. 일반 workspace 는 `not_a_mirror` 오류다.
#[test]
fn agent_mirrors_disconnect_once_and_other_workspaces_are_refused_with_a_reason() {
    alone!("agent_mirrors_disconnect_once_and_other_workspaces_are_refused_with_a_reason");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, None);
    let mut session = crate::app::attach_client::tests::test_session(AGENT_MIRROR, HashMap::new());
    session.state.remote_workspace = 42;
    app.remote.sessions.push(session);

    let first = disconnect(&mut app, AGENT_MIRROR);
    let result = &first["result"];
    assert_eq!(result["disconnected"], true, "{first}");
    assert_eq!(result["already_disconnected"], false, "{first}");
    assert_eq!(result["remote_workspace"], 42, "{first}");
    assert!(result.get("previous_state").is_none(), "{first}");
    assert!(app.remote.sessions.is_empty());

    let again = disconnect(&mut app, AGENT_MIRROR);
    assert_eq!(again["error"]["code"], -32602, "{again}");
    assert_eq!(again["error"]["data"]["reason"], "workspace_not_found");
    assert_eq!(again["error"]["data"]["workspace_id"], AGENT_MIRROR);

    let plain = app
        .engines
        .all_sessions()
        .flat_map(|session| {
            session
                .core_state
                .workspaces()
                .into_iter()
                .map(|ws| ws.id)
                .collect::<Vec<_>>()
        })
        .find(|id| *id != ROW)
        .expect("a plain workspace");
    let refused = disconnect(&mut app, plain);
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
    assert_eq!(
        refused["error"]["data"]["reason"], "not_a_mirror",
        "{refused}"
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("is not a mirror with an attach session")
    );
}

/// 연결된 저장 행의 연결을 끝낸다. `forced` 면 서버의 강제 끊기 통지를 받은 것처럼 표지를 먼저 세운다.
fn end_row_connection(app: &mut App, forced: bool) {
    let session = app
        .remote
        .sessions
        .iter()
        .find(|session| session.state.saved_row == Some(ROW))
        .expect("the row session");
    if forced {
        session
            .transport
            .force_detached
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    session
        .transport
        .disconnected
        .store(true, std::sync::atomic::Ordering::SeqCst);
    app.apply_attach_client_output();
}

/// Given 연결된 저장 행, When 서버가 강제로 끊어 연결이 끝나면, Then 재연결 구간 없이 사용자가 끊은
/// `disconnected` 행이 되고, 그 행으로 다시 옮기거나 재연결 예약을 돌려도 연결을 시작하지 않는다.
/// 명시적 연결은 표지를 지우고 연결을 시작한다.
#[test]
fn a_force_detached_saved_row_stays_disconnected_until_an_explicit_connect() {
    alone!("a_force_detached_saved_row_stays_disconnected_until_an_explicit_connect");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, Some(SessionState::Connected));

    end_row_connection(&mut app, true);
    assert_eq!(app.row_state_name(ROW), "disconnected");
    assert!(app.remote.user_disconnected.contains(&ROW));
    assert!(
        app.remote
            .reconnect
            .get(&ROW)
            .is_some_and(|slot| slot.given_up)
    );

    app.maybe_connect_visible_row(Some(ROW), None);
    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(
        !app.remote.active.contains(&ROW),
        "no attempt after a force detach"
    );
    assert_eq!(app.row_state_name(ROW), "disconnected");

    assert!(force_detach_marked(&app), "the stop is saved for a restart");

    app.connect_saved_row(ROW, true);
    assert!(!app.remote.user_disconnected.contains(&ROW));
    assert!(!force_detach_marked(&app));
    assert_ne!(app.row_state_name(ROW), "disconnected");
}

/// 모든 창의 토스트 문구.
fn window_toasts(app: &mut App) -> Vec<String> {
    app.engines_mut()
        .window_pairs()
        .flat_map(|(_, main, _)| {
            main.state
                .toasts
                .messages()
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Given 연결된 저장 행, When 서버가 강제로 끊는다, Then 토스트를 띄우지 않는다. 저장 행은 행 표지와
/// 연결 팝업이 끊긴 사유를 보인다. 저장되지 않은 mirror 의 끊김 토스트는 그대로다.
#[test]
fn a_force_detached_saved_row_shows_no_toast() {
    alone!("a_force_detached_saved_row_shows_no_toast");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, Some(SessionState::Connected));
    assert!(
        app.engines_mut().window_pairs().count() > 0,
        "the rig has a window that would show a toast"
    );
    let before = window_toasts(&mut app);

    end_row_connection(&mut app, true);
    assert_eq!(app.row_state_name(ROW), "disconnected");
    assert_eq!(window_toasts(&mut app), before);
}

fn force_detach_marked(app: &App) -> bool {
    app.services
        .registries
        .remote_force_detached
        .read(|marks| marks.contains(&binding()))
}

/// Given 강제로 끊겨 저장된 표지가 남은 행, When 클라이언트를 다시 시작해 실행 중 표지를 잃고 그 행이 보여도,
/// Then 자동으로 붙지 않는다. 명시적 연결은 저장된 표지를 지우고, 그 뒤 연결만 끊기면 평소처럼 재연결한다.
/// 재시작은 실행 중 표지가 없는 새 App 과 저장된 목록으로 재현한다(프로세스 재시작은 아니다).
#[test]
fn a_force_detached_row_stays_stopped_after_a_restart_until_an_explicit_connect() {
    alone!("a_force_detached_row_stays_stopped_after_a_restart_until_an_explicit_connect");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, None);
    app.services
        .registries
        .remote_force_detached
        .update(|marks| marks.insert(binding()));
    assert!(!app.remote.user_disconnected.contains(&ROW));

    app.hold_force_detached_rows();
    app.maybe_connect_visible_row(Some(ROW), None);
    assert!(app.remote.user_disconnected.contains(&ROW));
    assert!(
        !app.remote.active.contains(&ROW),
        "a restored force-detached row does not connect by itself"
    );

    app.connect_saved_row(ROW, true);
    assert!(!force_detach_marked(&app));
    assert!(!app.remote.user_disconnected.contains(&ROW));
    app.hold_force_detached_rows();
    assert!(!app.remote.user_disconnected.contains(&ROW));
}

/// Given 연결된 저장 행, When 통지 없이 연결만 끊기면(EOF·heartbeat), Then 지금처럼 재연결 중이 된다.
#[test]
fn a_saved_row_that_only_lost_its_connection_reconnects() {
    alone!("a_saved_row_that_only_lost_its_connection_reconnects");
    let event_loop = event_loop();
    let mut app = saved_row_app(&event_loop, Some(SessionState::Connected));

    end_row_connection(&mut app, false);
    assert_eq!(app.row_state_name(ROW), "reconnecting");
    assert!(!app.remote.user_disconnected.contains(&ROW));
}
