//! 현행화 → compact 의 규칙 쪽 — 호스트 · 데몬 없이 정해지는 값. 껍데기는 `application::sync_compact` 다.

/// 누가 시작했나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
    /// 쉬는 탭의 캐시가 식기 전(`sync_compact::idle_wait`) — 임계 초과 자동(`Auto`)과 따로 센다.
    Idle,
}
