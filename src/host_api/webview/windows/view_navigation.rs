//! 탐색 요청·이력 이동과 탐색 상태 조회. 이벤트 처리기는 `PlatformWebView::create`에 있다.
use super::super::entry_response::windows as entry_adapter;
use super::super::scan_resume::HistoryStep;
use super::super::{NavState, PendingNavigation};
use super::PlatformWebView;
use super::navigation::{
    complete_requests, replay_navigation, set_js, stop_navigation, webview_source,
};
use tasty_model::html_script::top_level::Designation;
use windows::core::{BOOL, HSTRING};

impl PlatformWebView {
    pub fn nav_state(&self) -> NavState {
        self.nav_state.get()
    }

    /// 마지막 호출 뒤 렌더 process가 끝났는지 돌려주고 기록을 지운다.
    pub fn take_content_process_ended(&self) -> bool {
        self.content_process_ended.replace(false)
    }

    /// 스캔을 기다리려고 취소한 탐색을 읽기가 끝났으면 같은 종류로 다시 시작한다.
    /// webview 동기화마다 부른다. 표가 낡았으면 다시 시작하지 않는다(더 새 탐색이 화면을 맡는다).
    pub fn resume_script_decision(&self) {
        if let Some(source) = &self.entry_source {
            entry_adapter::poll(source, &self._environment, &self.entry_pending);
        }
        // 지문 확인이 끝난 문서 요청: 같으면 놓아 주고, 다르면 본문을 넘기기 전에 멈춘다.
        let verified = self.scan_resume.borrow_mut().poll_verify();
        complete_requests(self.surface_id, verified.complete);
        if verified.stop {
            stop_navigation(
                self.surface_id,
                &self.webview,
                true,
                &self.scan_resume,
                self.script_gate.borrow().as_ref(),
                &self.nav_state,
            );
        }
        let Some((replay, url)) = self.scan_resume.borrow_mut().poll() else {
            return;
        };
        if let Err(e) = replay_navigation(&self.webview, replay, &url) {
            tracing::warn!(
                "WebView surface: replaying the navigation after its scan failed ({replay:?}): {e}"
            );
            self.scan_resume.borrow_mut().replay_failed();
            if let Some(gate) = self.script_gate.borrow().as_ref() {
                gate.failed();
                if let Some(js) = gate.finished() {
                    set_js(&self.webview, js);
                }
            }
            self.nav_state.set(NavState::Failed);
        }
    }

    pub fn take_pending_navigations(&self) -> Vec<PendingNavigation> {
        std::mem::take(&mut *self.pending_navigations.borrow_mut())
    }

    /// 로드를 요청하며 chrome을 Loading으로 둔다. 시작 신호 없이 끝나는 로드가 Loading에 남지 않게
    /// chrome 세대를 비워(모름 = 현재) 다음 시작 신호 전의 종료를 이 로드의 것으로 본다.
    fn begin_load(&self) {
        self.host_request(None);
        self.nav_state.set(NavState::Loading);
        self.chrome_generation.set(None);
    }

    pub fn load_url(&self, url: &str) {
        if self
            .entry_source
            .as_ref()
            .is_some_and(|source| source.response(url).is_none())
        {
            self.nav_state.set(NavState::Failed);
            return;
        }
        self.begin_load();
        self.top_level.borrow_mut().designate(Designation::Next);
        // SAFETY: HSTRING은 호출 끝까지 살아있고 Navigate는 main thread 호출.
        unsafe {
            let url = HSTRING::from(url);
            if let Err(e) = self.webview.Navigate(&url) {
                tracing::warn!("WebView2 Navigate failed: {e}");
            }
        }
    }

    pub fn load_html(&self, html: &str) {
        if self.entry_source.is_some() {
            self.nav_state.set(NavState::Failed);
            return;
        }
        self.begin_load();
        // SAFETY: HSTRING은 호출 끝까지 살아있고 NavigateToString은 main thread 호출.
        unsafe {
            let html = HSTRING::from(html);
            if let Err(e) = self.webview.NavigateToString(&html) {
                tracing::warn!("WebView2 NavigateToString failed: {e}");
            }
        }
    }

    /// 호스트 출처 로드·이동. 스캔 재시작 상태를 갱신하고 남은 사용자 클릭을 지운다.
    fn host_request(&self, step: Option<HistoryStep>) {
        self.scan_resume.borrow_mut().host_request(step);
        if let Some(route) = &self.host_input {
            route.cancel_click();
        }
    }

    pub fn reload(&self) {
        self.host_request(None);
        self.top_level.borrow_mut().designate(Designation::Next);
        // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출.
        if let Err(e) = unsafe { self.webview.Reload() } {
            tracing::warn!("WebView2 Reload failed: {e}");
        }
    }

    pub fn go_back(&self) {
        self.host_request(Some(HistoryStep::Back));
        self.designate_history(|wv, out| {
            // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출. 출력 BOOL은 지역 변수다.
            unsafe { wv.CanGoBack(out) }
        });
        // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출.
        if let Err(e) = unsafe { self.webview.GoBack() } {
            tracing::warn!("WebView2 GoBack failed: {e}");
        }
    }

    pub fn go_forward(&self) {
        self.host_request(Some(HistoryStep::Forward));
        self.designate_history(|wv, out| {
            // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출. 출력 BOOL은 지역 변수다.
            unsafe { wv.CanGoForward(out) }
        });
        // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출.
        if let Err(e) = unsafe { self.webview.GoForward() } {
            tracing::warn!("WebView2 GoForward failed: {e}");
        }
    }

    /// 주소 표시용 탐색 정보. 주소 선택 규칙은 `committed_document_url`이 정한다.
    pub fn nav_info(&self) -> crate::model::WebViewNavInfo {
        let live = webview_source(&self.webview).unwrap_or_else(|e| {
            self.note_nav_info_failure("Source", &e);
            None
        });
        let mut back = BOOL(0);
        let mut forward = BOOL(0);
        // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출. 출력 BOOL은 지역 변수다.
        if let Err(e) = unsafe { self.webview.CanGoBack(&mut back) } {
            self.note_nav_info_failure("CanGoBack", &e);
        }
        // SAFETY: 위와 같다.
        if let Err(e) = unsafe { self.webview.CanGoForward(&mut forward) } {
            self.note_nav_info_failure("CanGoForward", &e);
        }
        crate::model::WebViewNavInfo {
            url: crate::model::committed_document_url(
                self.nav_state.get(),
                live,
                self.committed_url.borrow().clone(),
            ),
            can_go_back: back.as_bool(),
            can_go_forward: forward.as_bool(),
        }
    }

    /// 이동할 이력이 있을 때만 다음 탐색을 지목한다. 이력이 없으면 탐색이 시작되지 않아 지목이 남는다.
    fn designate_history(
        &self,
        can: impl FnOnce(
            &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
            &mut BOOL,
        ) -> windows::core::Result<()>,
    ) {
        let mut out = BOOL(0);
        match can(&self.webview, &mut out) {
            Ok(()) if out.as_bool() => self.top_level.borrow_mut().designate(Designation::Next),
            Ok(()) => {}
            Err(e) => {
                tracing::warn!("WebView2 history query failed: {e}; the step is not designated")
            }
        }
    }

    /// `nav_info` 실패를 웹뷰별 첫 번째만 warn, 이후는 debug로 남긴다.
    fn note_nav_info_failure(&self, call: &str, e: &windows::core::Error) {
        if self.nav_info_failed.replace(true) {
            tracing::debug!("WebView2 {call} failed again: {e}");
        } else {
            tracing::warn!("WebView2 {call} failed: {e} (later failures are logged at debug)");
        }
    }

    /// 사용자 탐색 조작에 없는 로드 중지를 debug IPC에서 재현한다.
    #[cfg(debug_assertions)]
    pub fn debug_stop(&self) {
        // SAFETY: webview는 self가 살아있는 동안 valid, main thread 호출.
        if let Err(e) = unsafe { self.webview.Stop() } {
            tracing::warn!("WebView2 Stop failed: {e}");
        }
    }
}
