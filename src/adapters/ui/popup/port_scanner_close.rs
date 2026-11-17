//! 포트 스캐너 팝업의 닫기 훅.

use super::port_scanner::PortScanState;
use crate::state::MainViewState;

/// 닫기 훅. 콘텐츠의 닫기 버튼과 Esc로 닫으면 스캔 요청을 버린다. 바깥 클릭·제목줄 닫기·명령으로
/// 닫으면 스캔 상태를 남겨 다시 열 때 이어 보인다.
pub fn on_close_port_scanner_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    use crate::adapters::ui::popup::PopupCloseReason;
    if matches!(
        closed.reason,
        PopupCloseReason::Content | PopupCloseReason::Escape
    ) {
        // 요청 identity를 버려 App이 이전 스캔 결과를 새 popup에 적용하지 않게 한다.
        state.port_scan = PortScanState::Idle;
        state.port_favorites_scan = PortScanState::Idle;
    }
}
