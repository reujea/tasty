//! 공통 그래프 뷰에 스냅샷을 대는 데이터 소스 계약.
//! 조회 주기·취소·세대 검사와 마지막 스냅샷은 소스가 맡고 그래프 뷰(`tasty_ui_widgets::GraphView`)는 시간을 모른다.

use std::time::Instant;

use crate::runtime::engine_read::EngineRead;

/// 그래프 뷰 하나의 데이터 소스. DAG 그래프는 `DagGraphSource`가 구현한다.
pub trait GraphSource {
    /// 조회 대상. 소스마다 다르다.
    type Target<'a>;

    /// 조회 주기가 지났으면 다음 조회를 내고, 끝난 조회가 있으면 결과를 반영한다.
    fn poll_if_stale(&mut self, engine: &EngineRead<'_>, target: Self::Target<'_>);

    /// 주기를 기다리지 않고 다음 프레임에 다시 읽게 한다.
    fn invalidate_poll(&mut self);

    /// 다음 조회 시각. 아직 읽은 적이 없으면 `None`(즉시).
    fn next_poll_at(&self) -> Option<Instant>;
}
