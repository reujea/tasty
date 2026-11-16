//! WebKitGTK 쪽 스크립트 설정과 평가 — surface 하나의 JS 를 켜고 끄는 자리.
use webkit2gtk::{SettingsExt, WebView, WebViewExt};

pub(super) fn set_js(wv: &WebView, enabled: bool) {
    if let Some(settings) = WebViewExt::settings(wv) {
        settings.set_enable_javascript(enabled);
    }
}
