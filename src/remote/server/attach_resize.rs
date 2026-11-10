//! attach client의 크기 요청을 적용하고 모든 요청에 응답한다.
//! 크기 변경과 같은 크기 확인은 terminal의 attach tap이 출력과 같은 순서로 Resize를 보낸다.
//! 거절은 크기를 바꾸지 않으므로 이 모듈이 바로 ResizeRejected를 보낸다.
//! 협상 규칙: docs/dev-guide/attach-behavior.md#리사이즈-전파-mirror-geometry.

use crate::core::attach::AttachClientId;
use crate::runtime::engine_access::EngineMut;
use tasty_ipc::stream::{StreamControl, StreamFrame, StreamTag};
use tasty_ipc::stream_hub::{PushResult, StreamHub};

/// 한 engine에서 크기 요청을 처리한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachResizeOutcome {
    /// grid를 바꿨거나 이미 같은 크기라 tap이 Resize로 응답한다.
    Answered,
    /// 이 engine의 점유에 그 surface가 없다. 다른 engine이 처리할 수 있다.
    NotHere,
    /// 점유자가 아니거나 terminal이 아니라 적용하지 않았다. 호출자가 거절을 회신한다.
    Rejected,
}

impl EngineMut<'_> {
    /// 점유를 확인한 뒤 실제 PTY 크기 변경을 시도한다. OS PTY 적용 성공은 확인하지 않는다.
    pub fn apply_attached_workspace_resize(
        &mut self,
        client_id: AttachClientId,
        remote_surface_id: u32,
        cols: usize,
        rows: usize,
    ) -> AttachResizeOutcome {
        let Some(ws) = self.live.occupancy.workspace_of_surface(remote_surface_id) else {
            return AttachResizeOutcome::NotHere;
        };
        if self.live.occupancy.workspace_holder(ws) != Some(client_id) {
            return AttachResizeOutcome::Rejected;
        }
        if self
            .runtime
            .terminals
            .resize_for_attach(remote_surface_id, cols, rows)
        {
            AttachResizeOutcome::Answered
        } else {
            AttachResizeOutcome::Rejected
        }
    }
}

/// 크기 요청 하나를 마무리한다. `outcome`은 요청을 받은 engine의 결과이고, 어느 engine도 받지
/// 않았으면 `None`이나 `NotHere`다. `Answered`면 tap이 이미 응답하므로 아무것도 보내지 않고,
/// 그 밖에는 거절을 회신한다. GUI 루프와 헤드리스 루프가 같은 규칙을 쓰도록 여기서 정한다.
pub(crate) fn finish_resize_request(
    hub: &StreamHub,
    client_id: AttachClientId,
    surface_id: u32,
    cols: usize,
    rows: usize,
    outcome: Option<AttachResizeOutcome>,
) {
    if outcome != Some(AttachResizeOutcome::Answered) {
        reply_resize_rejected(hub, client_id, surface_id, cols, rows);
    }
}

/// 여러 engine을 가진 GUI 루프의 크기 요청 하나. `for_engines`는 창·parked engine을 차례로 넘기다가
/// 클로저가 true를 돌려준 engine에서 멈춘다. 요청을 받은 engine의 결과로 [`finish_resize_request`]를 부른다.
#[cfg(feature = "gui")]
pub(crate) fn answer_resize_on_engines(
    hub: &StreamHub,
    client_id: AttachClientId,
    surface_id: u32,
    cols: usize,
    rows: usize,
    for_engines: impl FnOnce(&mut dyn FnMut(&mut EngineMut<'_>) -> bool),
) {
    let mut outcome = None;
    for_engines(&mut |engine| {
        let o = engine.apply_attached_workspace_resize(client_id, surface_id, cols, rows);
        outcome = Some(o);
        o != AttachResizeOutcome::NotHere
    });
    finish_resize_request(hub, client_id, surface_id, cols, rows, outcome);
}

/// 적용하지 않은 크기 요청을 요청자에게 알린다. 값은 거절한 요청의 것이다.
fn reply_resize_rejected(
    hub: &StreamHub,
    client_id: AttachClientId,
    surface_id: u32,
    cols: usize,
    rows: usize,
) {
    let reply = StreamControl::ResizeRejected {
        surface_id,
        cols,
        rows,
    };
    let payload = match serde_json::to_vec(&reply) {
        Ok(payload) => payload,
        Err(e) => {
            tracing::warn!("attach resize: 거절 회신을 직렬화하지 못했다: {e}");
            return;
        }
    };
    let result = hub.push(client_id, StreamFrame::new(StreamTag::Control, payload));
    if result != PushResult::Sent {
        // client는 응답 마감으로 같은 실패를 판정하므로 여기서 다시 보내지 않는다.
        tracing::warn!(
            "attach resize: client {client_id} 에 surface {surface_id} 거절 회신을 넣지 못했다: {result:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_terminal::AttachEvent;

    const HOLDER: AttachClientId = 7;

    /// 첫 workspace를 HOLDER가 점유하고 그 안의 terminal ID를 돌려준다.
    fn occupy_first_workspace(engine: &mut EngineMut<'_>) -> u32 {
        let ws = engine.workspace_at(0).expect("workspace index is valid");
        let (ws_id, members) = (ws.id, ws.all_surface_ids());
        engine
            .live
            .occupancy
            .acquire_workspace(ws_id, &members, &members, HOLDER)
            .expect("workspace is free in the fixture");
        *members
            .iter()
            .find(|id| engine.runtime.terminals.contains(**id))
            .expect("fixture workspace has a terminal")
    }

    #[test]
    fn every_request_gets_an_outcome_and_a_same_size_request_is_confirmed_in_order() {
        let (_, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let sid = occupy_first_workspace(&mut engine);
        let terminal = engine.runtime.terminals.get_mut(sid).expect("terminal");
        let (cols, rows) = (terminal.cols(), terminal.rows());
        let mut tap = terminal.snapshot_and_stream();

        assert_eq!(
            engine.apply_attached_workspace_resize(HOLDER, sid, cols, rows),
            AttachResizeOutcome::Answered
        );
        assert!(matches!(
            tap.events.try_recv(),
            Ok(AttachEvent::Resize { cols: c, rows: r }) if (c, r) == (cols, rows)
        ));

        assert_eq!(
            engine.apply_attached_workspace_resize(HOLDER + 1, sid, cols + 1, rows),
            AttachResizeOutcome::Rejected,
            "점유자가 아니면 적용하지 않고 거절한다"
        );
        assert_eq!(
            engine.apply_attached_workspace_resize(HOLDER, u32::MAX, cols, rows),
            AttachResizeOutcome::NotHere,
            "점유에 없는 surface는 다른 engine에 맡긴다"
        );
        assert!(
            tap.events.try_recv().is_err(),
            "거절과 다른 engine 요청은 tap에 아무것도 남기지 않는다"
        );
    }

    /// 마무리 뒤 client가 받은 거절 회신. 응답이 tap 몫이면 비어 있다.
    fn rejection_after(outcome: Option<AttachResizeOutcome>) -> Option<StreamControl> {
        let hub = StreamHub::new();
        let client = hub.alloc_id();
        let rx = hub.register(client);
        finish_resize_request(&hub, client, 4, 90, 30, outcome);
        let frame = rx.try_recv().ok()?;
        Some(serde_json::from_slice(&frame.payload).expect("control payload"))
    }

    #[test]
    fn only_an_answered_request_goes_without_a_rejection() {
        let rejected = Some(StreamControl::ResizeRejected {
            surface_id: 4,
            cols: 90,
            rows: 30,
        });
        assert_eq!(rejection_after(Some(AttachResizeOutcome::Answered)), None);
        assert_eq!(
            rejection_after(Some(AttachResizeOutcome::Rejected)),
            rejected
        );
        assert_eq!(
            rejection_after(Some(AttachResizeOutcome::NotHere)),
            rejected,
            "어느 engine도 받지 않은 요청도 거절한다"
        );
        assert_eq!(rejection_after(None), rejected, "engine이 없어도 거절한다");
    }

    /// engine 둘을 차례로 묻는 GUI 루프. 앞 engine에 없으면 뒤 engine이 받고, 아무도 받지 않거나
    /// 받은 engine이 거절하면 거절을 회신한다.
    #[cfg(feature = "gui")]
    #[test]
    fn the_gui_loop_asks_engines_in_order_and_rejects_what_none_took() {
        let (_, mut empty) = crate::state::tests::test_state();
        let (_, mut holding) = crate::state::tests::test_state();
        let mut empty = empty.borrow_mut();
        let mut holding = holding.borrow_mut();
        let sid = occupy_first_workspace(&mut holding);
        let terminal = holding.runtime.terminals.get(sid).expect("terminal");
        let (cols, rows) = (terminal.cols(), terminal.rows());
        let hub = StreamHub::new();
        let rx = hub.register(HOLDER);
        let other = hub.register(HOLDER + 1);

        let mut both = |apply: &mut dyn FnMut(&mut EngineMut<'_>) -> bool| {
            let _taken = apply(&mut empty) || apply(&mut holding);
        };
        answer_resize_on_engines(&hub, HOLDER, sid, cols, rows, &mut both);
        assert!(rx.try_recv().is_err(), "뒤 engine이 받았으므로 회신이 없다");

        let replies = |rx: &tasty_ipc::stream_hub::SinkReceiver| -> Vec<StreamControl> {
            std::iter::from_fn(|| rx.try_recv().ok())
                .map(|f| serde_json::from_slice(&f.payload).expect("control payload"))
                .collect()
        };
        answer_resize_on_engines(&hub, HOLDER + 1, sid, cols, rows, &mut both);
        assert_eq!(
            replies(&other),
            vec![StreamControl::ResizeRejected {
                surface_id: sid,
                cols,
                rows
            }],
            "받은 engine이 점유자가 아니라고 거절하면 그 client에게 거절이 간다"
        );
        answer_resize_on_engines(&hub, HOLDER, u32::MAX, 80, 24, &mut both);
        assert_eq!(
            replies(&rx),
            vec![StreamControl::ResizeRejected {
                surface_id: u32::MAX,
                cols: 80,
                rows: 24
            }],
            "어느 engine에도 없는 surface는 거절한다"
        );
    }

    /// 연결마다 응답 보장을 확인할 수 있도록 workspace descriptor가 capability를 싣는다.
    #[test]
    fn workspace_descriptor_announces_the_resize_ack_capability() {
        let (_, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let ws_id = engine.workspace_at(0).expect("workspace index is valid").id;
        let descriptor = engine.build_workspace_descriptor(
            0,
            ws_id,
            &crate::model::AttachSurfaceClass::default(),
        );
        assert_eq!(
            descriptor[tasty_ipc::stream::DESCRIPTOR_CAPABILITIES],
            serde_json::json!([
                tasty_ipc::stream::RESIZE_ACK_CAPABILITY,
                tasty_ipc::stream::INPUT_ACK_CAPABILITY,
                tasty_ipc::stream::WORKSPACE_CLOSE_CAPABILITY,
                tasty_ipc::explorer_op::CAPABILITY,
                tasty_ipc::agent_child::CAPABILITY,
                tasty_ipc::terminal_read::CAPABILITY,
                tasty_ipc::file_download::CAPABILITY,
                tasty_ipc::html_transfer::CAPABILITY,
                tasty_ipc::html_transfer::VIEWER_CAPABILITY,
                tasty_ipc::html_transfer::RESOURCE_CAPABILITY,
                tasty_ipc::html_http::CAPABILITY,
                tasty_ipc::html_http::HTTPS_CAPABILITY,
                tasty_ipc::html_http::RELOAD_CAPABILITY
            ])
        );
        // client는 이 값으로 mirror 키 인코딩을 정한다. 클라이언트 OS가 아니다.
        assert_eq!(
            descriptor[tasty_ipc::stream::DESCRIPTOR_PTY_INPUT],
            if cfg!(windows) { "win32-console" } else { "vt" }
        );
        assert_eq!(
            tasty_ipc::stream::descriptor_wants_win32_console_input(&descriptor),
            cfg!(windows)
        );
    }
}
