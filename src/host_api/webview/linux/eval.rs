//! plugin 소유 surface 의 `webview.post_message` — 실린 문서 안에서 호스트가 만든 스크립트를 돌린다.
use webkit2gtk::{SettingsExt, WebView, WebViewExt};

use super::PlatformWebView;

pub(super) fn set_js(wv: &WebView, enabled: bool) {
    if let Some(settings) = WebViewExt::settings(wv) {
        settings.set_enable_javascript(enabled);
    }
}

impl PlatformWebView {
    /// 실린 문서를 다시 싣지 않고 그 안에서 스크립트를 돌린다. 결과는 받지 않고 실패만 로그에 남긴다.
    /// 페이지 JS 설정과 무관하게 실행되므로 허용 판정은 호출 전(`webview.post_message` 핸들러)에 끝나 있어야 한다.
    pub fn eval_js(&self, script: &str) {
        self.webview.evaluate_javascript(
            script,
            None,
            None,
            gtk::gio::Cancellable::NONE,
            |result| {
                if let Err(e) = result {
                    tracing::warn!("WebKitGTK evaluate_javascript failed: {e}");
                }
            },
        );
    }
}
