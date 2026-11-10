//! debug 빌드 전용: host 입력 경로를 끈 WebView를 만들어 `AcceleratorKeyPressed` 경로를 재현한다.
//! 옛 WebView2 runtime처럼 옵션을 쓸 수 없는 view의 키 처리를 검증할 때 쓴다(`docs/design/systems/webview.md`).

/// `TASTY_DEBUG_WEBVIEW2_NO_HOST_INPUT`에 값이 있으면 true다.
pub(super) fn host_input_disabled() -> bool {
    std::env::var_os("TASTY_DEBUG_WEBVIEW2_NO_HOST_INPUT").is_some_and(|v| !v.is_empty())
}
