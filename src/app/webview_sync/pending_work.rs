//! global drain 깨움 뒤 다음 redraw가 이어 가야 끝나는 native webview 대기 작업.

use crate::view::MainView;

/// 이 창에 다음 프레임의 `synchronize`가 결과를 꺼내야 하는 webview 대기 작업이 있는지 확인한다.
/// GTK 준비·reload 교체·trust 완료 콜백은 결과를 저장하고 engine 기본 waker(global drain)만 부른다.
/// 대기는 준비 중인 webview, reload 교체를 기다리는 HTTP webview, 이 세션의 trust 요청 대기다.
/// 프로세스 공용 trust 처리가 진행 중이면 창을 가릴 수 없어 HTTP webview가 있는 창을 모두 대기로 센다.
// 이유: macOS·Windows의 준비·교체 poll은 이 깨움을 쓰지 않는다.
#[cfg(target_os = "linux")]
pub(crate) fn awaits_webview_work(view: &MainView) -> bool {
    has_pending_work(
        view.http_preparing.len(),
        view.webviews
            .values()
            .map(|webview| webview.awaits_http_work()),
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn awaits_webview_work(_view: &MainView) -> bool {
    false
}

/// 준비 중인 webview가 있거나 결과를 기다리는 HTTP webview가 하나라도 있으면 true다.
#[cfg_attr(
    not(any(target_os = "linux", test)),
    expect(
        dead_code,
        reason = "only GTK completions wake through the global drain"
    )
)]
fn has_pending_work(preparing: usize, mut awaiting: impl Iterator<Item = bool>) -> bool {
    preparing > 0 || awaiting.any(|waits| waits)
}

#[cfg(test)]
mod tests {
    use super::has_pending_work;

    /// Given 준비 중인 webview 하나 또는 결과를 기다리는 HTTP webview 하나, Then 창은 대기 중이다.
    #[test]
    fn preparing_or_awaiting_webviews_keep_the_window_waiting() {
        assert!(has_pending_work(1, std::iter::empty()));
        assert!(has_pending_work(0, [false, true].into_iter()));
    }

    /// Given 로드·reload·trust가 모두 끝나 HTTP webview가 남아 있어도 기다리는 것이 없음,
    /// Then 창은 대기 중이 아니어서 global drain이 다시 그리지 않는다.
    #[test]
    fn settled_http_webviews_do_not_keep_the_window_waiting() {
        assert!(!has_pending_work(0, std::iter::empty()));
        assert!(!has_pending_work(0, [false, false].into_iter()));
    }
}
