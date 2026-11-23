//! 새 알림의 소리를 서버와 attach client 중 어디서 낼지 정한다.

use crate::runtime::engine_access::EngineMut;
use tasty_ipc::stream::{StreamControl, StreamFrame, StreamTag};
use tasty_ipc::stream_hub::PushResult;

impl EngineMut<'_> {
    /// 새 알림 한 건의 소리를 attach client가 맡으면 true다. 이때 서버는 소리를 내지 않는다.
    /// 기준은 이 서버의 새 알림 id다. 출처(터미널 바이트·IPC·명령 기록 한도)와 무관하게 대상 surface
    /// (없으면 workspace)를 준비된 점유로 가진 holder가 `ClientTerminalEffects`를 선언했으면
    /// `NotificationSound`를 한 번 보낸다. 선언하지 않은 client(CLI attach, 구 client)나 선언 처리
    /// 전이면 서버가 지금처럼 낸다. 큐가 가득 차 버려져도 화면 재동기화를 만들지 않고, 서버가 대신
    /// 재생하지도 않는다.
    pub(crate) fn hand_notification_sound_to_attach_client(
        &mut self,
        ws_id: u32,
        surface_id: u32,
    ) -> bool {
        let Some(holder) = self.notification_sound_holder(ws_id, surface_id) else {
            return false;
        };
        let Some(hub) = self.remote.notifier() else {
            return false;
        };
        if !hub.accepts_terminal_effects(holder) {
            return false;
        }
        let frame = StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&StreamControl::NotificationSound { surface_id })
                .unwrap_or_default(),
        );
        let result = hub.push_droppable(holder, frame);
        if result != PushResult::Sent {
            tracing::warn!(
                surface_id,
                holder,
                ?result,
                "notification sound could not reach the attach client"
            );
        }
        true
    }

    /// surface의 준비된 hard lock holder, 없으면 workspace의 준비된 점유 holder.
    fn notification_sound_holder(
        &self,
        ws_id: u32,
        surface_id: u32,
    ) -> Option<crate::core::attach::AttachClientId> {
        let occupancy = &self.live.occupancy;
        if surface_id != 0
            && let Some(lock) = occupancy.surface_lock(surface_id)
        {
            return lock.ready.then_some(lock.holder);
        }
        occupancy
            .workspace_holder(ws_id)
            .filter(|_| occupancy.workspace_attachment_ready(ws_id))
    }
}

#[cfg(test)]
mod tests {
    use tasty_ipc::stream::StreamTag;

    fn sounds(rx: &tasty_ipc::stream_hub::SinkReceiver) -> usize {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|frame| {
                frame.tag == StreamTag::Control
                    && String::from_utf8_lossy(&frame.payload).contains("notification_sound")
            })
            .count()
    }

    /// Given surface를 점유한 attach client, When 그 surface에 새 알림이 생긴다, Then 터미널 효과를
    /// 넘겨받겠다고 선언한 holder에게 알림음을 한 번 보내고 서버는 비킨다. 선언 전이거나 점유가 없으면
    /// 서버가 낸다.
    #[test]
    fn a_new_notification_sound_goes_to_the_attach_client_that_holds_its_surface() {
        let (_state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let ws = engine.workspace_at(0).expect("workspace").id;
        let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let hub = tasty_ipc::stream_hub::StreamHub::new();
        let client = hub.alloc_id();
        let rx = hub.register(client);
        engine.remote.set_notifier(hub.clone());

        assert!(!engine.hand_notification_sound_to_attach_client(ws, sid));
        engine.attach_surface_for_stream(sid, client, &hub);
        sounds(&rx); // 연결 안내와 snapshot 프레임을 비운다.
        assert!(
            !engine.hand_notification_sound_to_attach_client(ws, sid),
            "선언하지 않은 client"
        );
        assert_eq!(sounds(&rx), 0);
        hub.enable_terminal_effects(client);
        assert!(engine.hand_notification_sound_to_attach_client(ws, sid));
        assert_eq!(sounds(&rx), 1);
        // surface만 점유했으면 workspace 알림은 서버 몫이다.
        assert!(!engine.hand_notification_sound_to_attach_client(ws, 0));
        assert_eq!(sounds(&rx), 0);
    }
}
