//! 엔진이 사용자 제스처 값을 주지 않는 탐색의 사용자 클릭 증명. 이 뷰가 받은 OS 마우스 클릭으로 대신 증명한다.
//! - macOS: WKNavigationAction 공개 API 에 Linux `is_user_gesture`·Windows `IsUserInitiated` 에 해당하는
//!   값이 없다. NSResponder 의 `mouseDown:`·`mouseUp:` 이 클릭을 기록한다.
//! - Windows: 같은 문서 fragment 이동은 `HistoryChanged` 로만 알 수 있고 거기에 `IsUserInitiated` 가
//!   없다. host 입력 경로의 메시지 훅이 WebView 로 가는 `WM_LBUTTONDOWN`·`WM_LBUTTONUP` 을 기록한다.
//!
//! 페이지 스크립트가 만든 이벤트는 이 OS 경로를 지나지 않으므로 클릭으로 세지 않는다.
//! 규칙은 docs/design/systems/webview.md "탐색" 절에 있다.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// 클릭 뒤 이 시간 안에 시작한 main frame 탐색만 클릭이 낸 것으로 본다. WebKit 이 사용자 제스처를
/// 타이머로 넘겨주는 상한(`UserGestureToken::maximumIntervalForUserGestureForwarding`, 1초)과 같다.
pub(crate) const GESTURE_WINDOW: Duration = Duration::from_secs(1);

/// 마지막 OS 클릭 시각. 한 클릭은 main frame 탐색 하나만 증명한다.
#[derive(Debug, Default)]
pub(crate) struct ClickGesture {
    armed: Cell<Option<Instant>>,
}

impl ClickGesture {
    /// OS 가 이 뷰에 왼쪽 버튼 누름을 보냈다.
    pub(crate) fn press(&self, now: Instant) {
        self.armed.set(Some(now));
    }

    /// 누름에 이어 버튼을 뗐다. 페이지의 click 이벤트는 뗄 때 생기므로 시각을 뗀 때로 옮긴다.
    /// 누름 없이 온 뗌은 증명하지 않는다.
    pub(crate) fn release(&self, now: Instant) {
        if self.armed.get().is_some() {
            self.armed.set(Some(now));
        }
    }

    /// 호스트(에이전트·IPC 포함)가 로드·이동을 시작했다. 그 탐색을 클릭이 낸 것으로 보지 않는다.
    pub(crate) fn cancel(&self) {
        self.armed.set(None);
    }

    /// 탐색 결정 시점에 사용자 클릭 증명을 꺼낸다. main frame 탐색만 클릭을 소비한다.
    /// 시간 창을 넘긴 클릭은 버린다.
    pub(crate) fn take(&self, now: Instant, main_frame: Option<bool>) -> bool {
        if main_frame != Some(true) {
            return false;
        }
        self.armed
            .take()
            .is_some_and(|at| now.saturating_duration_since(at) <= GESTURE_WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn navigation_without_click_is_not_user_gesture() {
        let g = ClickGesture::default();
        assert!(!g.take(Instant::now(), Some(true)));
    }

    #[test]
    fn click_proves_one_main_frame_navigation() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(80));
        assert!(g.take(t + ms(120), Some(true)));
        assert!(
            !g.take(t + ms(130), Some(true)),
            "한 클릭은 한 번만 증명한다"
        );
    }

    #[test]
    fn subframe_or_unknown_frame_neither_proves_nor_consumes() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        assert!(!g.take(t + ms(10), Some(false)));
        assert!(!g.take(t + ms(10), None));
        assert!(g.take(t + ms(20), Some(true)));
    }

    #[test]
    fn expired_click_is_dropped() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(50));
        assert!(!g.take(t + ms(50) + GESTURE_WINDOW + ms(1), Some(true)));
        assert!(!g.take(t + ms(60) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn window_counts_from_release() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(1500));
        assert!(g.take(t + ms(1500) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn release_without_press_does_not_arm() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.release(t);
        assert!(!g.take(t + ms(10), Some(true)));
    }

    #[test]
    fn host_load_cancels_pending_click() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.cancel();
        assert!(!g.take(t + ms(10), Some(true)));
    }
}
