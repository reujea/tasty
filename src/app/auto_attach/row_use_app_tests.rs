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
