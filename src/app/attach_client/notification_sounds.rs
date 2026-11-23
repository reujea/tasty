//! 서버가 넘긴 attach 대상의 알림음을 이 client에서 재생한다.
use crate::app::App;

impl App {
    /// 이번 출력 회차에 받은 알림음을 재생한다. 세션이 정리되기 전에 부른다.
    pub(super) fn play_attach_notification_sounds(&mut self) {
        let sounds: u32 = self
            .remote
            .sessions
            .iter_mut()
            .map(|sess| std::mem::take(&mut sess.state.notification_sounds))
            .sum();
        for _ in 0..sounds {
            self.services.sound_player().play();
        }
    }

    /// 이 mirror workspace의 서버가 터미널 유래 알림을 직접 만들어 그 소리를 넘겨주는지.
    pub(crate) fn mirror_receives_notification_sounds(&self, workspace: u32) -> bool {
        self.remote.sessions.iter().any(|session| {
            session.state.local_workspace == workspace && session.transport.notification_sounds
        })
    }
}
