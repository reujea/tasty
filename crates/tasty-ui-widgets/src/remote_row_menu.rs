//! 저장된 원격 행의 연결 상태와 그 상태에서 행 메뉴가 켜는 항목.
//! 본체는 OS 메뉴로, 갤러리는 egui 메뉴로 그리지만 켜짐 규칙과 문구 키는 이 모듈 하나를 쓴다.

/// 사이드바 행이 보이는 연결 상태. 저장된 원격 행에만 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRowState {
    Connected,
    /// 이번 실행에서 처음 붙는 중이다. 형상이 없다.
    Connecting,
    /// 붙어 있다가 끊겨 다시 붙는 중이다. 형상이 남아 있다.
    Reconnecting,
    /// 붙어 있다가 끊긴 채 멈췄다. 형상이 남아 있다.
    Disconnected,
    /// 이번 실행에서 아직 붙지 않았다(재시작 뒤, 첫 연결 실패). 형상이 없다.
    NotConnected,
}

impl RemoteRowState {
    /// 이번 실행에서 형상이 있었던 상태. 연결 항목의 문구가 Reconnect 가 된다.
    pub fn had_shape(self) -> bool {
        matches!(
            self,
            Self::Connected | Self::Reconnecting | Self::Disconnected
        )
    }

    /// 연결을 시도하는 중이다(점 슬롯 Spinner, 레일 로딩 링).
    pub fn attempting(self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting)
    }
}

/// 행 메뉴의 연결·닫기 묶음에서 켜지는 항목.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteRowMenu {
    pub connect: bool,
    pub disconnect: bool,
    pub close_local: bool,
    pub close_remote: bool,
    /// 연결 항목의 문구 키(Connect 또는 Reconnect).
    pub connect_key: &'static str,
}

/// 연결 항목의 키. 형상이 있었던 행은 Reconnect 를 쓴다.
pub const REMOTE_ROW_CONNECT_KEY: &str = "remote.row.popup.connect";
pub const REMOTE_ROW_RECONNECT_KEY: &str = "child_graph.reconnect";
pub const REMOTE_ROW_DISCONNECT_KEY: &str = "remote.row.menu.disconnect";
pub const REMOTE_ROW_CLOSE_LOCAL_KEY: &str = "remote.row.menu.close_local";
pub const REMOTE_ROW_CLOSE_REMOTE_KEY: &str = "remote.row.menu.close_remote";

/// 상태별 켜짐. 연결되지 않은 동안에는 원격에 닿을 수 없어 원격 닫기는 연결됐을 때만 켜진다.
/// 연결 중·재연결 중의 Disconnect 는 진행 중인 시도를 취소한다.
/// `closed_elsewhere`는 이번 실행에서 연결돼 있던 동안 원격 workspace 가 다른 곳에서 닫힌 행이다.
/// 다시 붙을 곳이 없어 Reconnect 를 숨기지 않고 끈다. 이 표지는 저장하지 않아 재시작 뒤에는
/// 아직 붙지 않은 행이 되고 Connect 가 켜진다.
pub fn remote_row_menu(state: RemoteRowState, closed_elsewhere: bool) -> RemoteRowMenu {
    use RemoteRowState as S;
    RemoteRowMenu {
        connect: matches!(state, S::Disconnected | S::NotConnected) && !closed_elsewhere,
        disconnect: matches!(state, S::Connected | S::Connecting | S::Reconnecting),
        close_local: true,
        close_remote: state == S::Connected,
        connect_key: if state.had_shape() {
            REMOTE_ROW_RECONNECT_KEY
        } else {
            REMOTE_ROW_CONNECT_KEY
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 정본 `remoteMenuEnabled` 표와 같은지 상태마다 확인한다.
    #[test]
    fn each_state_enables_what_the_design_table_lists() {
        use RemoteRowState as S;
        let row = |s| {
            let m = remote_row_menu(s, false);
            (m.connect, m.disconnect, m.close_local, m.close_remote)
        };
        assert_eq!(row(S::Connected), (false, true, true, true));
        assert_eq!(row(S::Connecting), (false, true, true, false));
        assert_eq!(row(S::Reconnecting), (false, true, true, false));
        assert_eq!(row(S::Disconnected), (true, false, true, false));
        assert_eq!(row(S::NotConnected), (true, false, true, false));
    }

    /// 다른 곳에서 닫혀 남은 행은 Reconnect 를 끄고 나머지는 끊긴 행과 같다.
    #[test]
    fn a_row_closed_elsewhere_keeps_reconnect_disabled() {
        let m = remote_row_menu(RemoteRowState::Disconnected, true);
        assert_eq!(
            (m.connect, m.disconnect, m.close_local, m.close_remote),
            (false, false, true, false)
        );
        assert_eq!(m.connect_key, REMOTE_ROW_RECONNECT_KEY);
    }

    #[test]
    fn a_row_that_had_a_screen_says_reconnect() {
        use RemoteRowState as S;
        for (state, key) in [
            (S::Connected, REMOTE_ROW_RECONNECT_KEY),
            (S::Reconnecting, REMOTE_ROW_RECONNECT_KEY),
            (S::Disconnected, REMOTE_ROW_RECONNECT_KEY),
            (S::Connecting, REMOTE_ROW_CONNECT_KEY),
            (S::NotConnected, REMOTE_ROW_CONNECT_KEY),
        ] {
            assert_eq!(remote_row_menu(state, false).connect_key, key, "{state:?}");
        }
    }
}
