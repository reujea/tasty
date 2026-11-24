//! surface leaf를 두르는 링의 판정. 링을 그리는 곳(attention 링, 점유 링)과 포커스 띠 행 안의 링이 같은
//! 판정을 읽는다. 그리는 층과 순서는 각 호출자가 정한다.

use crate::core::AttentionKind;
use crate::runtime::engine_read::EngineRead;

/// leaf가 보이는 attention 링. 응답 필요는 점유 표시보다 우선하고, 완료 표시는 점유 중 숨긴다.
pub(crate) fn attention_ring(engine: &EngineRead<'_>, surface_id: u32) -> Option<AttentionKind> {
    let occupied = engine.live.occupancy.occupancy_of(surface_id).is_some();
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
