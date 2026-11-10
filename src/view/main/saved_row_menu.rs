//! 저장된 원격 행의 우클릭 메뉴 끝 묶음(연결·닫기)과 그 항목의 동작.
//! 켜짐 규칙과 문구 키는 갤러리 표본과 같은 `tasty_ui_widgets::remote_row_menu`를 쓴다.
//! 에이전트 mirror와 로컬 workspace는 `redraw::workspace_close_menu_items`를 그대로 쓴다.

use crate::platform::native_menu::MenuItem;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use tasty_ui_widgets::{
    REMOTE_ROW_CLOSE_LOCAL_KEY, REMOTE_ROW_CLOSE_REMOTE_KEY, REMOTE_ROW_DISCONNECT_KEY,
    RemoteRowState, remote_row_menu,
};

/// 연결 끊기는 mirror 메뉴의 항목 id를 그대로 쓴다.
pub(super) const DISCONNECT: u32 = 8;
pub(super) const CONNECT: u32 = 9;
pub(super) const CLOSE_LOCAL: u32 = 10;
pub(super) const CLOSE_REMOTE: u32 = 11;

/// 메뉴를 연 행이 저장된 원격 행이면 그 연결 상태와, 다른 곳에서 닫혀 남은 행인지.
pub(super) fn saved_row_state(
    engine: &EngineRead<'_>,
    ws_idx: usize,
) -> Option<(RemoteRowState, bool)> {
    let ws = engine.workspace_at(ws_idx)?;
    ws.is_saved_remote_row().then(|| {
        (
            engine.remote_row_state(ws.id),
            engine.remote_row_view(ws.id).closed_elsewhere,
        )
    })
}

/// 연결 묶음(Connect 또는 Reconnect · Disconnect)과 닫기 묶음(Close locally · Close remote…).
/// 할 수 없는 항목은 숨기지 않고 끈다. 다른 곳에서 닫혀 남은 행은 Reconnect 도 끈다.
pub(super) fn saved_row_menu_items((state, closed): (RemoteRowState, bool)) -> Vec<MenuItem> {
    let on = remote_row_menu(state, closed);
    let item = |id, key: &'static str, enabled| MenuItem {
        enabled,
        ..MenuItem::new(id, crate::i18n::t(key))
    };
    vec![
        MenuItem::separator(),
        item(CONNECT, on.connect_key, on.connect),
        item(DISCONNECT, REMOTE_ROW_DISCONNECT_KEY, on.disconnect),
        MenuItem::separator(),
        item(CLOSE_LOCAL, REMOTE_ROW_CLOSE_LOCAL_KEY, on.close_local),
        item(CLOSE_REMOTE, REMOTE_ROW_CLOSE_REMOTE_KEY, on.close_remote),
    ]
}

/// 연결·로컬 닫기·원격 닫기 항목을 처리한다. 다른 id면 false다. 연결 끊기는 mirror 항목과 같다.
pub(super) fn handle(state: &mut MainViewState, workspace_id: u32, id: u32) -> bool {
    use crate::intent::{Intent, SavedRowAction};
    let action = match id {
        CONNECT => SavedRowAction::Connect,
        CLOSE_LOCAL => SavedRowAction::CloseLocally,
        CLOSE_REMOTE => {
            crate::adapters::ui::popup::confirm_close_remote::open(
                state,
                crate::app::attach_client::workspace_close::SavedRowClose {
                    row: workspace_id,
                    last: false,
                },
                crate::app::command::DomainIntent::CloseWorkspace { workspace_id },
            );
            return true;
        }
        _ => return false,
    };
    state.dispatch_intent(
        Intent::SavedRow {
            workspace_id,
            action,
        }
        .from_user_context_menu(),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(items: &[MenuItem]) -> Vec<Option<(u32, bool)>> {
        items
            .iter()
            .map(|item| (!item.is_separator()).then_some((item.id, item.enabled)))
            .collect()
    }

    /// 연결된 행은 끊기와 두 닫기를, 끊긴 행은 연결과 로컬 닫기를 켠다. 항목은 숨기지 않는다.
    #[test]
    fn the_saved_row_menu_disables_what_the_state_cannot_do() {
        assert_eq!(
            shape(&saved_row_menu_items((RemoteRowState::Connected, false))),
            vec![
                None,
                Some((CONNECT, false)),
                Some((DISCONNECT, true)),
                None,
                Some((CLOSE_LOCAL, true)),
                Some((CLOSE_REMOTE, true)),
            ]
        );
        assert_eq!(
            shape(&saved_row_menu_items((RemoteRowState::Disconnected, false))),
            vec![
                None,
                Some((CONNECT, true)),
                Some((DISCONNECT, false)),
                None,
                Some((CLOSE_LOCAL, true)),
                Some((CLOSE_REMOTE, false)),
            ]
        );
        assert_eq!(
            shape(&saved_row_menu_items((RemoteRowState::Disconnected, true)))[1],
            Some((CONNECT, false)),
            "a row closed elsewhere keeps Reconnect, disabled"
        );
    }

    /// 형상이 있었던 행은 Reconnect, 아직 붙지 않은 행은 Connect 문구를 쓴다.
    #[test]
    fn the_connect_item_says_reconnect_only_for_a_row_that_had_a_screen() {
        let label = |state| saved_row_menu_items((state, false))[1].label.clone();
        assert_eq!(
            label(RemoteRowState::Disconnected),
            crate::i18n::t("child_graph.reconnect")
        );
        assert_eq!(
            label(RemoteRowState::NotConnected),
            crate::i18n::t("remote.row.popup.connect")
        );
    }
}
