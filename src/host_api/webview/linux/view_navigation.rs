//! 탐색 요청·이력 이동과 탐색 상태 조회. 신호 처리기는 `PlatformWebView::new`에 있다.
use webkit2gtk::WebViewExt;

use super::super::NavState;
use super::PlatformWebView;

impl PlatformWebView {
    pub fn load_url(&self, url: &str) {
        self.edit_guard.bump();
        // 같은 문서의 fragment 이동은 load-changed 없이 끝난다(측정). Loading을 세우면 풀리지 않는다.
        if self.nav_state.get() == NavState::Done
            && crate::model::is_same_document_fragment_move(self.webview.uri().as_deref(), url)
        {
            self.webview.load_uri(url);
            return;
        }
        self.loads.borrow_mut().requested();
        self.nav_state.set(NavState::Loading);
        self.webview.load_uri(url);
    }

    pub fn load_html(&self, html: &str) {
        self.edit_guard.bump();
        if self.entry_source.is_some() {
            tracing::warn!("raw HTML load rejected for a remote entry WebView");
            self.nav_state.set(NavState::Failed);
            return;
        }
        self.loads.borrow_mut().requested();
        self.nav_state.set(NavState::Loading);
        self.webview.load_html(html, None);
    }

    /// 호스트가 시작한 로드·이동은 web process의 load-changed보다 먼저 대기 중인 선택 질의를 무효화한다.
    /// web process가 바쁘면 load-changed가 선택 응답보다 늦게 온다(측정).
    pub fn reload(&self) {
        self.edit_guard.bump();
        self.webview.reload();
    }

    pub fn go_back(&self) {
        self.edit_guard.bump();
        self.webview.go_back();
    }

    pub fn go_forward(&self) {
        self.edit_guard.bump();
        self.webview.go_forward();
    }

    /// 주소 표시용 탐색 정보. 주소 선택 규칙은 `committed_document_url`이 정한다.
    pub fn nav_info(&self) -> crate::model::WebViewNavInfo {
        crate::model::WebViewNavInfo {
            url: crate::model::committed_document_url(
                self.nav_state.get(),
                self.webview.uri().map(|u| u.to_string()),
                self.committed_url.borrow().clone(),
            ),
            can_go_back: self.webview.can_go_back(),
            can_go_forward: self.webview.can_go_forward(),
        }
    }
}
