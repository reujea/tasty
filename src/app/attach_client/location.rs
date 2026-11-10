//! mirror workspace마다 원격 위치 표시 값(호스트 글자, 재연결 중)을 engine에 맞춘다.
//! 상태바·핸들러 picker·링크 툴팁은 App의 세션 목록 대신 engine의 이 값을 읽는다.

use crate::app::App;
use crate::remote::state::MirrorLocation;
use std::collections::HashMap;
use tasty_remote::client_session::{AttachClientSession, SessionState};

/// 세션 목록에서 mirror workspace(로컬 ID)별 위치 값을 만든다.
/// 연결 단계가 Connected여도 소켓이 끊긴 것이 확인됐으면 재연결 중으로 본다.
pub(super) fn locations_of(sessions: &[AttachClientSession]) -> HashMap<u32, MirrorLocation> {
    sessions
        .iter()
        .map(|s| {
            let live = s.state.phase == SessionState::Connected
                && !s
                    .transport
                    .disconnected
                    .load(std::sync::atomic::Ordering::Acquire);
            (
                s.state.local_workspace,
                MirrorLocation {
                    host: s.state.host_text(),
                    reconnecting: !live,
                },
            )
        })
        .collect()
}

impl App {
    /// 각 engine에 자기가 가진 mirror workspace의 위치 값만 넣는다.
    pub(crate) fn sync_mirror_locations(&mut self) {
        let all = locations_of(&self.remote.sessions);
        let mine = |core: &crate::core::CoreState| -> HashMap<u32, MirrorLocation> {
            all.iter()
                .filter(|(ws, _)| core.find_workspace_index_for_id(**ws).is_some())
                .map(|(ws, location)| (*ws, location.clone()))
                .collect()
        };
        use crate::view::ui::View as _;
        for (_, main, engine) in self.engines_mut().window_pairs() {
            let next = mine(engine.core);
            if engine.remote.mirror_locations != next {
                engine.remote.mirror_locations = next;
                main.mark_dirty();
            }
        }
        for (_, engine) in self.engines_mut().parked_sessions() {
            engine.remote.mirror_locations = mine(engine.core);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_mirror_shows_the_profile_name_and_a_lost_link_is_reconnecting() {
        let mut session = crate::app::attach_client::tests::test_session(10, HashMap::new());
        session.state.profile = Some("mac-company-attach".into());
        let live = locations_of(std::slice::from_ref(&session));
        assert_eq!(
            live[&10],
            MirrorLocation {
                host: "mac-company-attach".into(),
                reconnecting: false
            }
        );
        session.state.phase = SessionState::Reconnecting;
        assert!(locations_of(std::slice::from_ref(&session))[&10].reconnecting);
        session.state.phase = SessionState::Connected;
        session
            .transport
            .disconnected
            .store(true, std::sync::atomic::Ordering::Release);
        assert!(locations_of(std::slice::from_ref(&session))[&10].reconnecting);
    }
}
