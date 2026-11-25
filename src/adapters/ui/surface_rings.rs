//! surface leaf를 두르는 링의 판정. 링을 그리는 곳(attention 링, 점유 링)과 포커스 띠 행 안의 링이 같은
//! 판정을 읽는다. 그리는 층과 순서는 각 호출자가 정한다.

use crate::core::AttentionKind;
use crate::runtime::engine_read::EngineRead;

/// leaf가 보이는 attention 링. 응답 필요는 점유 표시보다 우선하고, 완료 표시는 점유 중 숨긴다.
/// 점유는 [`occupancy_mark`]가 링을 그리는 판정과 같다(점유 workspace의 멤버 포함). 그래서 leaf의 링은 하나다.
pub(crate) fn attention_ring(engine: &EngineRead<'_>, surface_id: u32) -> Option<AttentionKind> {
    let occupied = engine.live.occupancy.occupancy_of(surface_id).is_some()
        || engine.live.occupancy.is_content_hidden(surface_id);
    engine
        .attention_kind(surface_id)
        .filter(|k| tasty_ui_widgets::surface_edge_attention(Some((*k).into()), occupied).is_some())
}

/// 점유된 leaf의 표시. 점유가 없으면 `None`이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OccupancyMark {
    pub(crate) hard: bool,
    /// 점유 링을 그리는지. 응답 대기 테두리가 같은 자리에 오면 그리지 않는다(강제 해제 버튼은 남긴다).
    pub(crate) edge: bool,
}

/// 점유 workspace의 멤버처럼 내용을 숨기는 surface는 hard로 본다.
pub(crate) fn occupancy_mark(engine: &EngineRead<'_>, surface_id: u32) -> Option<OccupancyMark> {
    let hard = if engine.live.occupancy.is_content_hidden(surface_id) {
        true
    } else {
        engine.live.occupancy.occupancy_of(surface_id)?.tier
            == crate::core::attach::OccupancyTier::Hard
    };
    Some(OccupancyMark {
        hard,
        edge: tasty_ui_widgets::occupancy_edge_shows(
            engine.attention_kind(surface_id).map(Into::into),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 점유 workspace의 멤버(surface lock 없음)는 hard 점유 링을 받으므로 완료 링을 숨긴다. 응답 필요는 점유 위다.
    /// 그래서 leaf의 링은 하나다.
    #[test]
    fn an_occupied_workspace_member_shows_one_ring() {
        let (_state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let sid = engine.workspace_at(0).expect("workspace").all_surface_ids()[0];
        engine.raise_attention(sid, AttentionKind::Completion);
        engine
            .live
            .occupancy
            .acquire_workspace(7, &[], &[sid], 41)
            .expect("workspace");
        let read = engine.read();
        assert!(occupancy_mark(&read, sid).is_some_and(|m| m.hard));
        assert_eq!(attention_ring(&read, sid), None, "completion hides");
        drop(read);
        engine.raise_attention(sid, AttentionKind::NeedsInput);
        let read = engine.read();
        assert_eq!(attention_ring(&read, sid), Some(AttentionKind::NeedsInput));
        assert!(
            occupancy_mark(&read, sid).is_some_and(|m| !m.edge),
            "needs input takes the ring's place"
        );
    }
}
