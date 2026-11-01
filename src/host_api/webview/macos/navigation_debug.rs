//! WebKit이 준 탐색 입력의 진단 로그.
use objc2_web_kit::WKNavigationAction;

/// 사용자 클릭 판정을 실측과 대조할 수 있게 WebKit 이 준 입력 정보를 남긴다. 판정에는 쓰지 않는다.
pub(super) fn log_navigation_action(
    surface_id: u32,
    action: &WKNavigationAction,
    main_frame: Option<bool>,
) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    // SAFETY: main thread WebKit delegate 호출 동안 유효한 navigation_action 의 값 읽기.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    let (kind, button, modifiers) = unsafe {
        (
            action.navigationType().0,
            action.buttonNumber(),
            action.modifierFlags().0,
        )
    };
    tracing::debug!(
        surface_id,
        ?main_frame,
        navigation_type = kind,
        button_number = button,
        modifier_flags = modifiers,
        "WKNavigationAction input"
    );
}
