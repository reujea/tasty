//! 현행화 → compact 의 규칙 쪽 — 호스트 · 데몬 없이 정해지는 값. 껍데기는 `application::sync_compact` 다.

/// 누가 시작했나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
    /// 쉬는 탭의 캐시가 식기 전(`sync_compact::idle_wait`) — 임계 초과 자동(`Auto`)과 따로 센다.
    Idle,
    /// 5시간·7일 사용 한도가 설정 문턱(`sync_compact::Knobs::limit_share`)을 넘었다 — 한도에 막혀
    /// 마지막 요청이 끊기기 전에 남긴다.
    Limit,
}
