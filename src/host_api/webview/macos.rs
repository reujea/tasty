//! macOS WKWebView wrapper.
//! Reference: wry/src/wkwebview/mod.rs (MIT license, Tauri)

mod content_rules;
mod delegate_state;
mod http;
mod keyboard;
mod navigation_debug;
mod overlay_backend;
mod remote_block;

pub(crate) use http::available as http_available;

use delegate_state::PendingDecision;
use navigation_debug::log_navigation_action;
use remote_block::RemoteBlock;

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::time::Instant;

use block2::DynBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSEvent, NSResponder, NSView,
};
use objc2_foundation::{
    NSError, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLAuthenticationChallenge,
    NSURLCredential, NSURLSessionAuthChallengeDisposition,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKNavigationType, WKWebView, WKWebViewConfiguration, WKWebpagePreferences,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use self::keyboard::{macos_keycode_to_physical, ns_chars_to_winit_key, nsevent_mods_to_winit};
use super::click_gesture::ClickGesture;
use super::entry_response::EntryResponseSource;
use super::keys::WebViewKeySink;
use super::load_generation::CancelledDecisions;
use super::script_gate::{Response, ScriptGate};
use super::{NavState, PendingNavigation, WebViewBounds};

/// 차단 여부와 별도로 기록한 탐색 시도 큐와, user_gesture 근거인 이 뷰의 OS 클릭.
struct NavigationRecorder {
    attempts: Rc<RefCell<Vec<PendingNavigation>>>,
    click: Rc<ClickGesture>,
}

struct NavDelegateIvars {
    surface_id: u32,
    entry_source: Option<EntryResponseSource>,
    /// The remote HTTP document this view shows through its endpoint proxy.
    http: Option<Weak<super::http_native::NativeDocument>>,
    /// A navigation cancelled to continue on its loopback alias; its interruption is no failure.
    alias_cancelled: Cell<bool>,
    nav_state: Rc<Cell<NavState>>,
    navigations: NavigationRecorder,
    /// html surface의 문서 단위 스크립트 허용. 없으면 탐색 단위 JS를 바꾸지 않는다.
    script_gate: Rc<RefCell<Option<ScriptGate>>>,
    /// 원격 차단의 실효값(전역 || 문서 허용). 게이트 상태가 바뀌는 지점마다 다시 읽는다.
    remote: RemoteBlock,
    /// 게이트가 정책 결정에서 로드를 시작했고 그 provisional 시작을 아직 받지 않았다.
    gate_load_pending: Cell<bool>,
    /// 게이트가 시작한 마지막 로드의 WKNavigation. 새 로드가 앞 로드를 취소하면 앞 로드의 종료가
    /// 새 로드 시작 뒤에 올 수 있으므로, 종료 신호는 이 세대의 것일 때만 게이트를 마무리한다(docs/surfaces/html/index.md).
    /// 값을 붙잡아 두어 같은 주소가 다른 navigation에 다시 쓰이지 않게 한다.
    gate_navigation: RefCell<Option<Retained<WKNavigation>>>,
    /// 마지막에 provisional 시작한 main frame navigation. 게이트 세대와 달리 모든 로드를 기록해,
    /// chrome 상태가 그 navigation의 종료만 따르게 한다. 붙잡아 두는 이유는 위와 같다.
    chrome_navigation: RefCell<Option<Retained<WKNavigation>>>,
    /// 마지막 main frame commit의 주소. 로드 중·실패 중 주소 표시가 쓴다.
    committed_url: RefCell<Option<String>>,
    /// 스캔 결과를 기다리는 정책 결정. 허용 기록과 같은 URL의 응답만 여기 온다.
    pending_decision: RefCell<Option<PendingDecision>>,
    /// Cancel로 끝낸 결정의 늦은 실패 신호를 새 로드의 실패로 보지 않게 센다.
    cancelled_decisions: RefCell<CancelledDecisions>,
    /// web content process가 끝났고 동기화 루프가 아직 꺼내 가지 않았다.
    content_process_ended: Cell<bool>,
}

define_class!(
    // SAFETY:
    // - 상위 클래스 NSObject 는 서브클래싱 제약이 없다.
    // - `MainThreadOnly` 가 맞다: WKWebView/WKNavigationDelegate 는 main thread 전용이며
    //   이 delegate 는 main thread 에서만 생성·호출된다(WebKit 이 콜백을 main thread 에서 발화).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "TastyNavDelegate"]
    #[ivars = NavDelegateIvars]
    struct NavDelegate;

    unsafe impl NSObjectProtocol for NavDelegate {}

    unsafe impl WKNavigationDelegate for NavDelegate {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start_provisional(&self, _web_view: &WKWebView, navigation: Option<&WKNavigation>) {
            let sid = self.ivars().surface_id;
            tracing::debug!("WebView surface {sid}: load started");
            self.ivars().nav_state.set(NavState::Loading);
            self.ivars().cancelled_decisions.borrow_mut().started();
            *self.ivars().chrome_navigation.borrow_mut() = navigation.map(|n| n.retain());
            // 정책 결정에서 게이트가 시작한 로드라면 이 navigation이 새 세대다(실 기기 미측정).
            if self.ivars().gate_load_pending.replace(false) {
                *self.ivars().gate_navigation.borrow_mut() = navigation.map(|n| n.retain());
            }
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _web_view: &WKWebView, navigation: Option<&WKNavigation>) {
            let sid = self.ivars().surface_id;
            tracing::debug!("WebView surface {sid}: load finished");
            self.ivars().chrome_navigation_ended(navigation, true);
            self.ivars().gate_navigation_ended(navigation);
        }

        /// main frame 문서가 commit됐다(docs/surfaces/html/index.md 후보 이벤트, 미측정).
        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            // SAFETY: main thread WebKit delegate 호출. URL()은 Retained 값을 반환한다.
            let url =
                unsafe { web_view.URL().and_then(|u| u.absoluteString()) }.map(|u| u.to_string());
            let http = self.ivars().http.as_ref().and_then(Weak::upgrade);
            let url = if http.is_some() {
                url.as_deref().map(http::logical)
            } else {
                url
            };
            *self.ivars().committed_url.borrow_mut() = url.clone();
            // A commit before the endpoint served the main response is WebKit's own page.
            if http.is_some_and(|document| document.session().awaiting_main_response()) {
                return;
            }
            if let Some(gate) = self.ivars().script_gate.borrow().as_ref() {
                let js = gate.committed(url.as_deref());
                self.ivars().remote.sync(gate);
                tracing::debug!(
                    "WebView surface {}: committed with js={js}",
                    self.ivars().surface_id
                );
            }
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            tracing::warn!(
                "WebView surface {}: WKWebView navigation failed: {}",
                self.ivars().surface_id,
                error.localizedDescription()
            );
            if self.ivars().absorbs_cancelled_failure(navigation, error)
                || self.ivars().absorbs_alias_cancel(error)
            {
                return;
            }
            self.ivars().http_load_failed(navigation);
            self.ivars().chrome_navigation_ended(navigation, false);
            self.ivars().gate_navigation_failed(navigation);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            _web_view: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            tracing::warn!(
                "WebView surface {}: WKWebView provisional navigation failed: {}",
                self.ivars().surface_id,
                error.localizedDescription()
            );
            if self.ivars().absorbs_cancelled_failure(navigation, error)
                || self.ivars().absorbs_alias_cancel(error)
            {
                return;
            }
            self.ivars().http_load_failed(navigation);
            self.ivars().chrome_navigation_ended(navigation, false);
            self.ivars().gate_navigation_failed(navigation);
        }

        /// web content process가 끝나면 Linux처럼 게이트 로드를 끝낸다(docs/surfaces/html/index.md, 실 기기 미측정).
        /// 실패 콜백이 뒤따라 와도 finished()는 로드 중이 아닐 때 아무것도 하지 않는다.
        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn web_content_process_did_terminate(&self, _web_view: &WKWebView) {
            tracing::warn!(
                "WebView surface {}: WebKit web content process terminated",
                self.ivars().surface_id
            );
            self.ivars().nav_state.set(NavState::Failed);
            self.ivars().content_process_ended.set(true);
            self.ivars().cancel_pending_decision();
            self.ivars().gate_failed();
            self.ivars().gate_finished();
        }

        /// Only a remote HTTP view answers challenges; other views keep WebKit's handling.
        #[unsafe(method(webView:didReceiveAuthenticationChallenge:completionHandler:))]
        fn challenge(
            &self,
            _web_view: &WKWebView,
            challenge: &NSURLAuthenticationChallenge,
            done: &DynBlock<dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential)>,
        ) {
            match &self.ivars().http {
                Some(document) => http::respond(document, self.ivars().surface_id, challenge, done),
                None => done.call((
                    NSURLSessionAuthChallengeDisposition::PerformDefaultHandling,
                    std::ptr::null_mut(),
                )),
            }
        }

        /// 탐색 시도를 기록하고 Allow로 응답한다. 원격 차단은 별도 content rule에 맡긴다.
        /// 이 메서드를 구현하면 preferences가 없는 판단 메서드는 불리지 않는다.
        /// main frame 탐색의 JS는 여기서 탐색 단위 preferences로 정한다(docs/surfaces/html/index.md 후보 이벤트, 미측정).
        #[unsafe(method(webView:decidePolicyForNavigationAction:preferences:decisionHandler:))]
        fn decide_policy(
            &self,
            _web_view: &WKWebView,
            navigation_action: &WKNavigationAction,
            preferences: &WKWebpagePreferences,
            decision_handler: &DynBlock<
                dyn Fn(WKNavigationActionPolicy, NonNull<WKWebpagePreferences>),
            >,
        ) {
            // SAFETY: main thread WebKit delegate 호출(WKNavigationDelegate 는
            // MainThreadOnly). request()/URL()/absoluteString() 은 이 호출 동안 살아있는
            // Retained 값을 반환하는 main thread AppKit/Foundation API.
            let url = unsafe {
                navigation_action
                    .request()
                    .URL()
                    .and_then(|u| u.absoluteString())
            };
            let url = url.map(|u| u.to_string());
            if let Some(document) = self.ivars().http.as_ref().and_then(Weak::upgrade) {
                self.decide_http(
                    _web_view,
                    navigation_action,
                    preferences,
                    decision_handler,
                    &document,
                    url,
                );
                return;
            }
            // SAFETY: main thread WebKit delegate 호출 동안 유효한 navigation_action을 읽는다.
            let target_frame = unsafe { navigation_action.targetFrame() };
            // SAFETY: 이 delegate 호출 동안 retain된 frame info를 main thread에서 읽는다.
            let main_frame = target_frame.map(|f| unsafe { f.isMainFrame() });
            log_navigation_action(self.ivars().surface_id, navigation_action, main_frame);
            // Native targetFrame distinguishes main navigation from iframe loads.
            // A cached child must still become a new received/scanned document when main.
            if let Some(source) = &self.ivars().entry_source {
                // SAFETY: live navigation action on the WebKit main thread.
                let frame = unsafe { navigation_action.targetFrame() };
                // SAFETY: frame info retained for this main-thread delegate call.
                let main = frame.map(|f| unsafe { f.isMainFrame() });
                let denied = url.as_deref().is_none_or(|uri| match main {
                    Some(true) => source.main_resource(uri),
                    Some(false) => !source.allows_uri(uri),
                    None => true,
                });
                if denied {
                    decision_handler
                        .call((WKNavigationActionPolicy::Cancel, NonNull::from(preferences)));
                    return;
                }
            }
            if let (Some(gate), Some(url)) = (self.ivars().script_gate.borrow().as_ref(), &url) {
                // SAFETY: main thread WebKit delegate 호출. targetFrame()/isMainFrame()/
                // navigationType()은 이 호출 동안 유효한 navigation_action을 읽는다.
                #[allow(clippy::multiple_unsafe_ops_per_block)]
                let (target_main_frame, reload) = unsafe {
                    (
                        navigation_action.targetFrame().map(|f| f.isMainFrame()),
                        navigation_action.navigationType() == WKNavigationType::Reload,
                    )
                };
                match target_main_frame {
                    // 같은 문서의 fragment 이동은 로드가 아니므로 건너뛴다. 재로드는 fragment가 있어도 거친다.
                    Some(true) if reload || !gate.is_fragment_move(url) => {
                        gate.load_started();
                        self.ivars().remote.sync(gate);
                        // 새 로드가 앞에서 보류한 결정을 대신한다. load_started 뒤라 그 표는 이미 낡았다.
                        self.ivars().cancel_pending_decision();
                        // provisional 시작이 새 세대를 기록할 때까지 어떤 종료든 이 로드를 끝낸다.
                        self.ivars().gate_load_pending.set(true);
                        *self.ivars().gate_navigation.borrow_mut() = None;
                        match gate.main_response(url) {
                            Response::Decided(js) => {
                                // SAFETY: main thread WebKit delegate 호출. preferences는 이 호출 동안 유효하다.
                                unsafe { preferences.setAllowsContentJavaScript(js) };
                                self.ivars().remote.sync(gate);
                            }
                            // 스캔 결과가 JS를 정한다. 결정을 붙잡아 두고 반환하면 WebKit은 응답을
                            // 기다린다. 작업 스레드는 파일만 읽고 호스트를 깨우며, 결정은 다음 webview
                            // 동기화에서 메인 스레드가 끝낸다(resume_script_decision).
                            Response::Deferred(scan) => {
                                let read = scan.spawn_read();
                                *self.ivars().pending_decision.borrow_mut() =
                                    Some(PendingDecision {
                                        scan,
                                        read,
                                        handler: decision_handler.copy(),
                                        preferences: preferences.retain(),
                                    });
                                self.record_navigation(url.clone(), main_frame);
                                return;
                            }
                        }
                    }
                    // 서브프레임은 기본 preferences(JS 켬) 대신 main frame 문서에 대한 판단을 받는다(실 기기 미측정).
                    Some(false) => {
                        let js = gate.effective_js();
                        // SAFETY: main thread WebKit delegate 호출. preferences는 이 호출 동안 유효하다.
                        unsafe { preferences.setAllowsContentJavaScript(js) };
                    }
                    _ => {}
                }
            }
            if let Some(url) = url {
                self.record_navigation(url, main_frame);
            }
            decision_handler.call((WKNavigationActionPolicy::Allow, NonNull::from(preferences)));
        }
    }
);

impl NavDelegate {
    /// The endpoint decides scripts from the verified main response with a response policy, so
    /// every frame runs with scripts enabled natively. A main-frame navigation to a remote
    /// loopback address continues on its alias.
    fn decide_http(
        &self,
        web_view: &WKWebView,
        navigation_action: &WKNavigationAction,
        preferences: &WKWebpagePreferences,
        decision_handler: &DynBlock<
            dyn Fn(WKNavigationActionPolicy, NonNull<WKWebpagePreferences>),
        >,
        document: &super::http_native::NativeDocument,
        url: Option<String>,
    ) {
        // SAFETY: main thread WebKit delegate call; reads the live navigation action.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        let (main, reload, request) = unsafe {
            (
                navigation_action.targetFrame().map(|f| f.isMainFrame()),
                navigation_action.navigationType() == WKNavigationType::Reload,
                navigation_action.request(),
            )
        };
        if main == Some(true)
            && let Some(url) = &url
            && http::redirect_to_alias(web_view, &request, url)
        {
            self.ivars().alias_cancelled.set(true);
            decision_handler.call((WKNavigationActionPolicy::Cancel, NonNull::from(preferences)));
            return;
        }
        // SAFETY: main thread WebKit delegate call; preferences are valid for this call.
        unsafe { preferences.setAllowsContentJavaScript(true) };
        let url = url.as_deref().map(http::logical);
        if main == Some(true)
            && let (Some(gate), Some(url)) = (self.ivars().script_gate.borrow().as_ref(), &url)
            && (reload || !gate.is_fragment_move(url))
        {
            gate.load_started();
            self.ivars().cancel_pending_decision();
            self.ivars().gate_load_pending.set(true);
            *self.ivars().gate_navigation.borrow_mut() = None;
            document.session().expect_main_response(url);
        }
        if let Some(url) = url {
            self.record_navigation(url, main);
        }
        decision_handler.call((WKNavigationActionPolicy::Allow, NonNull::from(preferences)));
    }

    /// 탐색 시도를 기록한다. 이 뷰의 OS 클릭 뒤 시간 창 안의 main frame 탐색만 사용자 조작으로 표시한다.
    fn record_navigation(&self, url: String, main_frame: Option<bool>) {
        let navigations = &self.ivars().navigations;
        let user_gesture = navigations.click.take(Instant::now(), main_frame);
        tracing::debug!(
            surface_id = self.ivars().surface_id,
            ?main_frame,
            user_gesture,
            "navigation attempt recorded"
        );
        navigations
            .attempts
            .borrow_mut()
            .push(PendingNavigation { url, user_gesture });
    }
}

impl NavDelegate {
    fn new(
        mtm: MainThreadMarker,
        surface_id: u32,
        nav_state: Rc<Cell<NavState>>,
        navigations: NavigationRecorder,
        (script_gate, remote): (Rc<RefCell<Option<ScriptGate>>>, RemoteBlock),
        entry_source: Option<EntryResponseSource>,
        http: Option<Weak<super::http_native::NativeDocument>>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NavDelegateIvars {
            surface_id,
            entry_source,
            http,
            alias_cancelled: Cell::new(false),
            nav_state,
            navigations,
            script_gate,
            remote,
            gate_load_pending: Cell::new(false),
            gate_navigation: RefCell::new(None),
            chrome_navigation: RefCell::new(None),
            committed_url: RefCell::new(None),
            pending_decision: RefCell::new(None),
            cancelled_decisions: RefCell::new(CancelledDecisions::default()),
            content_process_ended: Cell::new(false),
        });
        // SAFETY: NSObject 의 지정 초기화자 init 을 super 로 호출.
        unsafe { msg_send![super(this), init] }
    }
}

struct KeyWebViewIvars {
    surface_id: u32,
    key_bridge: Rc<dyn WebViewKeySink>,
    /// OS 가 이 뷰에 보낸 왼쪽 클릭. 탐색 delegate 가 user_gesture 근거로 꺼낸다.
    click: Rc<ClickGesture>,
    /// 위에 egui가 그리는 안내 카드와 host 입력 영역(이 뷰 좌표). 이 안의 포인터는 받지 않고 부모로 보낸다.
    clip_holes: RefCell<Vec<NSRect>>,
    /// 대상 팝업이 보이는 동안 참이다. 그림은 그대로 두고 뷰 전체의 포인터를 부모(winit 뷰)로 보낸다.
    host_pointer: Cell<bool>,
}

define_class!(
    // SAFETY:
    // - WKWebView 는 서브클래싱을 허용한다(wry 등 임베딩 구현의 표준 방식). 지정
    //   초기화자 `initWithFrame:configuration:` 를 아래 `KeyWebView::new` 가 super 로
    //   호출해 올바르게 초기화한다.
    // - `MainThreadOnly` 가 맞다: WKWebView/NSResponder 는 main thread 전용이고 아래
    //   오버라이드는 전부 AppKit 이 main thread 에서 발화한다.
    // - `Drop` 을 구현하지 않는다.
    #[unsafe(super(WKWebView))]
    #[thread_kind = MainThreadOnly]
    #[name = "TastyKeyWebView"]
    #[ivars = KeyWebViewIvars]
    struct KeyWebView;

    unsafe impl NSObjectProtocol for KeyWebView {}

    impl KeyWebView {
        /// key equivalent는 뷰 전체에 전달될 수 있어 이 WebView가 포커스를 가진 경우만 처리한다.
        /// define_class가 Bool 반환 shim을 만들므로 return 대신 마지막 bool 표현식으로 값을 돌려준다.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if view_holds_first_responder(self) && self.forward_key_to_host(event) {
                true
            } else {
                // SAFETY: main thread AppKit responder chain. super 구현에 위임.
                unsafe { msg_send![super(self), performKeyEquivalent: event] }
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if self.forward_key_to_host(event) {
                return;
            }
            // SAFETY: main thread AppKit responder chain. super 구현에 위임.
            unsafe { msg_send![super(self), keyDown: event] }
        }

        /// native 클릭을 host에 알리고 원래 이벤트 처리는 super에 맡긴다.
        /// 페이지 스크립트가 만든 이벤트는 여기 오지 않으므로 OS 클릭의 증명으로 쓴다.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let ivars = self.ivars();
            tracing::debug!(surface_id = ivars.surface_id, "WebView OS mouse down");
            ivars.click.press(Instant::now());
            ivars.key_bridge.note_focus(ivars.surface_id);
            // SAFETY: main thread AppKit. super 구현에 위임.
            unsafe { msg_send![super(self), mouseDown: event] }
        }

        /// 페이지의 click 이벤트는 버튼을 뗄 때 생기므로 클릭 증명의 시각을 뗀 때로 옮긴다.
        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.ivars().click.release(Instant::now());
            // SAFETY: main thread AppKit. super 구현에 위임.
            unsafe { msg_send![super(self), mouseUp: event] }
        }

        /// 안내 카드와 host 입력 영역(분할선 hit 띠 · 창 리사이즈 밴드) 위의 포인터는 받지 않아 뒤의 host
        /// 뷰로 간다. layer mask는 그림만 가리므로 따로 뺀다.
        /// 대상 팝업이 보이는 동안은 뷰 전체의 포인터를 host 로 보낸다.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            if self.ivars().host_pointer.get() || self.in_clip_hole(point) {
                std::ptr::null_mut()
            } else {
                // SAFETY: main thread AppKit. super 구현에 위임.
                unsafe { msg_send![super(self), hitTest: point] }
            }
        }

        /// 내부 content view가 first responder가 되면 mouseDown이 여기 오지 않을 수 있어 포커스 경로도 알린다.
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            let ivars = self.ivars();
            ivars.key_bridge.note_focus(ivars.surface_id);
            // SAFETY: main thread AppKit. super 구현에 위임.
            unsafe { msg_send![super(self), becomeFirstResponder] }
        }
    }
);

impl KeyWebView {
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        config: &WKWebViewConfiguration,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        click: Rc<ClickGesture>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(KeyWebViewIvars {
            surface_id,
            key_bridge,
            click,
            clip_holes: RefCell::new(Vec::new()),
            host_pointer: Cell::new(false),
        });
        // SAFETY: WKWebView 의 지정 초기화자를 super 로 호출.
        unsafe { msg_send![super(this), initWithFrame: frame, configuration: config] }
    }

    /// superview 좌표 `point` 가 포인터 구멍(안내 카드 · host 입력 영역) 안인지. 구멍은 이 뷰 좌표로 저장한다.
    fn in_clip_hole(&self, point: NSPoint) -> bool {
        // SAFETY: MainThreadOnly 뷰의 AppKit hitTest 콜백 동안 self와 연결된 부모는 살아 있다.
        // 조회 전에 재진입하거나 계층을 바꾸지 않으며, 반환된 Retained가 부모 수명을 유지한다.
        let local = match unsafe { self.superview() } {
            Some(parent) => self.convertPoint_fromView(point, Some(&parent)),
            None => point,
        };
        self.ivars().clip_holes.borrow().iter().any(|r| {
            local.x >= r.origin.x
                && local.x < r.origin.x + r.size.width
                && local.y >= r.origin.y
                && local.y < r.origin.y + r.size.height
        })
    }

    fn forward_key_to_host(&self, event: &NSEvent) -> bool {
        let mods = nsevent_mods_to_winit(event.modifierFlags());
        let chars = event.charactersIgnoringModifiers();
        let Some(key) = chars.and_then(|c| ns_chars_to_winit_key(&c.to_string())) else {
            return false;
        };
        let physical = macos_keycode_to_physical(event.keyCode());
        let ivars = self.ivars();
        // 자동 반복은 다시 실행하지 않고, 첫 누름을 막거나 가져갔던 조합이면 WebKit에도 보내지 않는다.
        if event.isARepeat() {
            return ivars
                .key_bridge
                .consumes_repeat(ivars.surface_id, &key, &physical, mods);
        }
        if ivars.key_bridge.blocks_edit_key(&key, &physical, mods) {
            return true;
        }
        // Edit 메뉴가 없어 Cmd 편집 키(실행 취소 포함)도 사용자 바인딩으로 이 WebView의 편집 명령을 실행한다.
        if let Some(action) = ivars.key_bridge.edit_action(&key, &physical, mods) {
            run_editing_command(self, action);
            return true;
        }
        if ivars
            .key_bridge
            .capture_key(ivars.surface_id, key.clone(), physical, mods)
        {
            return true;
        }
        false
    }
}

/// WKWebView의 NSResponder 편집 selector를 실행한다. 대상 frame과 편집 가능 여부는 WebKit이 판단한다.
fn run_editing_command(view: &NSView, action: super::WebViewEditAction) {
    use super::WebViewEditAction as A;
    let selector = match action {
        A::Copy => objc2::sel!(copy:),
        A::Cut => objc2::sel!(cut:),
        A::Paste => objc2::sel!(paste:),
        A::SelectAll => objc2::sel!(selectAll:),
        A::Undo => objc2::sel!(undo:),
        A::Redo => objc2::sel!(redo:),
    };
    // SAFETY: main thread AppKit 뷰에 응답 여부만 묻는다.
    let responds: bool = unsafe { msg_send![view, respondsToSelector: selector] };
    if !responds && matches!(action, A::Undo | A::Redo) {
        // undo:/redo:는 뷰가 아니라 응답자 체인의 undoManager가 처리할 수 있다.
        let app = objc2_app_kit::NSApplication::sharedApplication(view.mtm());
        let null = std::ptr::null::<objc2::runtime::AnyObject>();
        // SAFETY: main thread AppKit. target nil은 first responder부터 응답자 체인을 탄다.
        let handled: bool = unsafe { msg_send![&*app, sendAction: selector, to: null, from: null] };
        if !handled {
            tracing::warn!("webview: edit selector {selector:?} is not handled");
        }
        return;
    }
    if !responds {
        tracing::warn!("webview: edit selector {selector:?} is not handled");
        return;
    }
    // SAFETY: 응답을 확인한 편집 action selector에 sender nil을 넘긴다. 반환값은 쓰지 않는다.
    let _ignored: *mut objc2::runtime::AnyObject = unsafe {
        msg_send![view, performSelector: selector, withObject: std::ptr::null::<objc2::runtime::AnyObject>()]
    };
}

/// 자기 뷰 또는 자손이 first responder인지 확인한다. 키 처리와 포커스 반환에 같은 조건을 쓴다.
fn view_holds_first_responder(view: &NSView) -> bool {
    let Some(window) = view.window() else {
        return false;
    };
    let Some(responder) = window.firstResponder() else {
        return false;
    };
    responder
        .downcast_ref::<NSView>()
        .is_some_and(|v| v.isDescendantOf(view))
}

pub struct PlatformWebView {
    surface_id: u32,
    /// The remote HTTP document this view shows through its endpoint proxy.
    http: Option<http::HttpView>,
    entry_handler: Option<Retained<super::entry_response::macos::EntryHandler>>,
    entry_source: Option<EntryResponseSource>,
    webview: Retained<KeyWebView>,
    remote: RemoteBlock,
    nav_state: Rc<Cell<NavState>>,
    pending_navigations: Rc<RefCell<Vec<PendingNavigation>>>,
    script_gate: Rc<RefCell<Option<ScriptGate>>>,
    /// WKWebView의 delegate 참조가 weak이므로 여기서 수명을 유지한다.
    _nav_delegate: Retained<NavDelegate>,
    /// 그림에서 뺄 안내 카드(`clip`)·불투명 오버레이 상자와 배율. 둘을 합쳐 한 layer mask 를 만든다.
    mask: RefCell<MaskInput>,
    /// 마지막으로 적용한 mask 입력과 그때의 뷰 크기. 같으면 mask를 다시 만들지 않는다.
    applied_mask: RefCell<Option<(MaskInput, f64, f64)>>,
}

/// WebView 그림에서 뺄 구멍. WebView 왼쪽 위 기준 물리 px `[x, y, 너비, 높이]` 다.
#[derive(Clone, Default, PartialEq)]
struct MaskInput {
    clip: Vec<[i32; 4]>,
    /// host 입력 영역. 포인터에서만 빼고 그림에는 남긴다.
    input: Vec<[i32; 4]>,
    overlay: Vec<[i32; 4]>,
    scale_factor: f64,
}

/// 이 backend의 생성 실패는 Permanent로 분류해 같은 생성 요청을 재시도하지 않는다.
fn perm(msg: impl std::fmt::Display) -> super::WebViewCreateError {
    super::WebViewCreateError::Permanent(msg.to_string())
}

impl PlatformWebView {
    pub(crate) fn entry_response_source(&self) -> Option<EntryResponseSource> {
        self.entry_source.clone()
    }

    pub fn new(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<super::entry_response::EntryResponseSource>,
    ) -> Result<Self, super::WebViewCreateError> {
        Self::create(
            window,
            bounds,
            scale_factor,
            surface_id,
            key_bridge,
            entry_source,
            None,
        )
    }

    pub(crate) fn new_http(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        document: super::http_native::NativeDocument,
    ) -> Result<Self, super::WebViewCreateError> {
        Self::create(
            window,
            bounds,
            scale_factor,
            surface_id,
            key_bridge,
            None,
            Some(document),
        )
    }

    fn create(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<super::entry_response::EntryResponseSource>,
        document: Option<super::http_native::NativeDocument>,
    ) -> Result<Self, super::WebViewCreateError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| perm("Must be called from main thread"))?;

        let ns_view_ptr = match window.window_handle().map_err(perm)?.as_raw() {
            RawWindowHandle::AppKit(w) => w.ns_view.as_ptr(),
            _ => return Err(perm("Not an AppKit window")),
        };
        // SAFETY: ns_view_ptr는 winit이 만든 활성 NSView로, 본 함수 호출 동안 살아있다
        // (winit이 윈도우를 drop하지 않는 한). mtm 검증 통과로 main thread 확정.
        let ns_view: &NSView = unsafe { &*(ns_view_ptr as *const NSView) };

        // SAFETY: mtm으로 main thread 확정. WKWebView/WKPreferences API는 main thread only.
        // msg_send![setValue:forKey:]는 NSString 두 객체에 대한 KVC — 같은 thread, 같은 호출 흐름.
        // WKWebView init + addSubview 시퀀스는 한 setup 단위라 분할 시 가독성 저하.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            let entry_handler = entry_source.as_ref().map(|source| {
                super::entry_response::macos::configure(&config, source.clone(), mtm)
            });
            let http = document
                .map(|document| http::HttpView::configure(&config, document, mtm))
                .transpose()
                .map_err(perm)?;

            let prefs = config.preferences();
            let key = NSString::from_str("defaultTextEncodingName");
            let value = NSString::from_str("UTF-8");
            let _: () = objc2::msg_send![&prefs, setValue: &*value, forKey: &*key];

            let frame = logical_to_nsrect(ns_view, bounds, scale_factor);

            let click = Rc::new(ClickGesture::default());
            let webview =
                KeyWebView::new(mtm, frame, &config, surface_id, key_bridge, click.clone());

            ns_view.addSubview(&webview);

            // WebKit의 delegate는 weak 참조이므로 struct에도 보관한다.
            let nav_state = Rc::new(Cell::new(NavState::Idle));
            let pending_navigations: Rc<RefCell<Vec<PendingNavigation>>> =
                Rc::new(RefCell::new(Vec::new()));
            let script_gate: Rc<RefCell<Option<ScriptGate>>> = Rc::new(RefCell::new(None));
            if let Some(http) = &http {
                // The endpoint decides remote content; only direct loopback loads are refused.
                http.install_rules(&webview, surface_id, mtm);
            }
            // 컴파일 완료 전에는 차단 규칙이 없다. 콜백이 현재 차단 상태에 맞춰 설치한다.
            let remote = RemoteBlock::install(
                Retained::into_super(webview.clone()),
                entry_source.is_some(),
                http.is_some(),
                mtm,
            );
            let nav_delegate = NavDelegate::new(
                mtm,
                surface_id,
                nav_state.clone(),
                NavigationRecorder {
                    attempts: pending_navigations.clone(),
                    click,
                },
                (script_gate.clone(), remote.clone()),
                entry_source.clone(),
                http.as_ref().map(|http| Rc::downgrade(&http.document)),
            );
            let nav_proto = ProtocolObject::from_ref(&*nav_delegate);
            webview.setNavigationDelegate(Some(nav_proto));

            Ok(Self {
                surface_id,
                http,
                entry_handler,
                entry_source,
                webview,
                remote,
                nav_state,
                pending_navigations,
                script_gate,
                _nav_delegate: nav_delegate,
                mask: RefCell::new(MaskInput::default()),
                applied_mask: RefCell::new(None),
            })
        }
    }

    pub fn set_bounds(&self, bounds: WebViewBounds, scale_factor: f64) {
        // SAFETY: main thread에서만 호출 — PlatformWebView는 main thread 객체
        // (Retained<WKWebView>이므로 !Send/!Sync 기본). logical_to_nsrect도 main thread.
        // superview/setFrame 호출이 한 묶음이라 분할 불필요.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            if let Some(parent) = self.webview.superview() {
                let frame = logical_to_nsrect(&parent, bounds, scale_factor);
                self.webview.setFrame(frame);
            }
        }
    }

    /// 이 WebView나 자손이 first responder일 때만 부모 contentView로 돌린다.
    /// nil로 바꾸면 winit 뷰가 응답자에서 빠질 수 있어 contentView를 명시한다.
    pub fn release_keyboard_focus(&self) {
        let Some(window) = self.webview.window() else {
            return;
        };
        if !view_holds_first_responder(&self.webview) {
            return;
        }
        let Some(content) = window.contentView() else {
            return;
        };
        let content_responder: &NSResponder = &content;
        if !window.makeFirstResponder(Some(content_responder)) {
            tracing::warn!("webview: winit content view 로 first responder 복구 실패");
        }
    }

    /// 창이 key window이고 이 WebView나 자손이 first responder인지.
    /// first responder는 비활성 창에도 남으므로 key window 여부를 함께 본다.
    pub fn holds_keyboard_focus(&self) -> bool {
        self.webview.window().is_some_and(|w| w.isKeyWindow())
            && view_holds_first_responder(&self.webview)
    }

    /// 지금 키를 치면 이 WebView가 받는지. macOS의 키는 first responder로만 가므로
    /// [`Self::holds_keyboard_focus`]와 같다.
    pub fn receives_keyboard_input(&self) -> bool {
        self.holds_keyboard_focus()
    }

    pub fn set_visible(&self, visible: bool) {
        self.webview.setHidden(!visible);
    }

    /// Linux 전용 처리다. macOS의 layer mask 변경은 AppKit이 다시 그린다.
    pub fn sync_halted_shape(&self) {}

    /// Linux 전용 처리다. 이 백엔드는 halt 중 별도 이벤트 처리를 돌리지 않는다.
    pub fn hold_input_for_halt(&self) {}

    /// 위에 egui가 그리는 안내 카드(`clip`, 이 뷰 왼쪽 위 기준 물리 px `[x, y, 너비, 높이]`)를 그림과
    /// 포인터에서 뺀다. 그림은 layer mask로, 포인터는 `hitTest:`로 뺀다. frame·표시·first responder는
    /// 바꾸지 않는다. 비면 원래대로 돌아간다. 불투명 오버레이 상자가 있으면 같은 mask에 함께 넣는다.
    /// host 입력 영역(`input`, 같은 좌표)은 `hitTest:`에서만 빼 분할선 드래그·창 리사이즈를 host 가 받는다.
    pub fn set_native_holes(&self, input: &[[i32; 4]], clip: &[[i32; 4]], scale_factor: f64) {
        {
            let mut mask = self.mask.borrow_mut();
            mask.clip = clip.to_vec();
            mask.input = input.to_vec();
            mask.scale_factor = scale_factor;
        }
        if !self.apply_mask() {
            tracing::warn!("webview: no backing layer; the page may cover the notice");
        }
    }

    /// 안내 카드와 불투명 오버레이 상자를 겹치지 않는 보이는 사각형의 layer mask로 그림에서 뺀다. 안내
    /// 카드는 `hitTest:`에서도 뺀다. 같은 입력·크기면 다시 만들지 않는다. layer가 없으면 거짓이다.
    fn apply_mask(&self) -> bool {
        let input = self.mask.borrow().clone();
        let view_bounds = self.webview.bounds();
        let key = (
            input.clone(),
            view_bounds.size.width,
            view_bounds.size.height,
        );
        if self.applied_mask.borrow().as_ref() == Some(&key) {
            return true;
        }
        let scale = if input.scale_factor > 0.0 {
            input.scale_factor
        } else {
            1.0
        };
        let flipped = self.webview.isFlipped();
        let to_points = |&[x, y, w, h]: &[i32; 4]| {
            let (x, y, w, h) = (
                f64::from(x) / scale,
                f64::from(y) / scale,
                f64::from(w) / scale,
                f64::from(h) / scale,
            );
            let origin_y = if flipped {
                y
            } else {
                view_bounds.size.height - y - h
            };
            NSRect::new(NSPoint::new(x, origin_y), NSSize::new(w, h))
        };
        *self.webview.ivars().clip_holes.borrow_mut() = input
            .clip
            .iter()
            .chain(&input.input)
            .map(to_points)
            .collect();
        if input.clip.is_empty() && input.overlay.is_empty() {
            // SAFETY: main thread AppKit. 이 뷰의 layer를 읽기만 한다.
            let layer: Option<Retained<objc2_quartz_core::CALayer>> =
                unsafe { msg_send![&*self.webview, layer] };
            if let Some(layer) = layer {
                // SAFETY: main thread. mask를 지우면 layer 전체가 다시 보인다.
                unsafe { layer.setMask(None) };
            }
            *self.applied_mask.borrow_mut() = Some(key);
            return true;
        }
        self.webview.setWantsLayer(true);
        // SAFETY: main thread AppKit. layer-backed 뷰의 layer를 읽는다.
        let layer: Option<Retained<objc2_quartz_core::CALayer>> =
            unsafe { msg_send![&*self.webview, layer] };
        let Some(layer) = layer else {
            return false;
        };
        let holes: Vec<[i32; 4]> = input.clip.iter().chain(&input.overlay).copied().collect();
        // 뷰 크기를 물리 px 로 올림해 오른쪽·아래 끝까지 덮는다. layer 밖은 mask 와 관계없다.
        let px = |points: f64| (points * scale).ceil() as i32;
        let visible = super::page_mask::visible_rects(
            px(view_bounds.size.width),
            px(view_bounds.size.height),
            &holes,
        );
        let mask = objc2_quartz_core::CAShapeLayer::new();
        mask.setFrame(layer.bounds());
        let path = objc2_core_graphics::CGMutablePath::new();
        for r in &visible {
            // SAFETY: 변환 행렬은 null(항등)이다. path는 이 함수 동안 살아 있다.
            unsafe {
                objc2_core_graphics::CGMutablePath::add_rect(
                    Some(&path),
                    std::ptr::null(),
                    to_points(r),
                )
            };
        }
        let shape: &objc2_core_graphics::CGPath = &path;
        mask.setPath(Some(shape));
        // SAFETY: main thread. mask는 layer가 retain한다.
        unsafe { layer.setMask(Some(&mask)) };
        *self.applied_mask.borrow_mut() = Some(key);
        true
    }

    /// delegate 콜백이 기록한 navigation 상태.
    pub fn nav_state(&self) -> NavState {
        self.nav_state.get()
    }

    /// 마지막 호출 뒤 web content process가 끝났는지 돌려주고 기록을 지운다.
    pub fn take_content_process_ended(&self) -> bool {
        self._nav_delegate
            .ivars()
            .content_process_ended
            .replace(false)
    }

    /// 스캔을 기다리며 보류한 정책 결정을 읽기가 끝났으면 끝낸다. webview 동기화마다 부른다.
    pub fn resume_script_decision(&self) {
        if let Some(handler) = &self.entry_handler {
            handler.poll();
        }
        self._nav_delegate.ivars().resume_pending_decision();
    }

    pub fn take_pending_navigations(&self) -> Vec<PendingNavigation> {
        std::mem::take(&mut *self.pending_navigations.borrow_mut())
    }

    /// 로드를 요청하며 chrome을 Loading으로 둔다. 시작 신호 없이 끝나는 로드가 Loading에 남지 않게
    /// chrome 세대를 비워(모름 = 현재) 다음 시작 신호 전의 종료를 이 로드의 것으로 본다.
    fn begin_load(&self) {
        self.webview.ivars().click.cancel();
        self._nav_delegate.ivars().cancel_pending_decision();
        self.nav_state.set(NavState::Loading);
        *self._nav_delegate.ivars().chrome_navigation.borrow_mut() = None;
    }

    pub fn load_url(&self, url: &str) {
        if let Some(http) = &self.http {
            self.begin_load();
            if !http.load_url(&self.webview, url) {
                self.nav_state.set(NavState::Failed);
            }
            return;
        }
        if self
            .entry_source
            .as_ref()
            .is_some_and(|source| source.response(url).is_none())
        {
            self.nav_state.set(NavState::Failed);
            return;
        }
        self.begin_load();
        // SAFETY: main thread WKWebView API. NSString/NSURL은 호출 동안 살아있는 local Retained.
        // URL loading 시퀀스는 한 단위라 분할 시 가독성 저하.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            if let Some(path) = url.strip_prefix("file://") {
                // URL 문자열이 아닌 파일 경로 API로 비ASCII 경로를 전달한다.
                let file_url = NSURL::fileURLWithPath(&NSString::from_str(path));
                // 문서 폴더 밖의 상대 리소스도 읽도록 범위를 파일 시스템 루트로 준다.
                // 스크립트 실행과 외부 전송은 스크립트 게이트와 원격 차단이 따로 막는다.
                let root = super::file_read_scope::read_access_root(path);
                let root_url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(root), true);
                self.webview
                    .loadFileURL_allowingReadAccessToURL(&file_url, &root_url);
            } else {
                let ns_url = NSURL::URLWithString(&NSString::from_str(url));
                if let Some(ns_url) = ns_url {
                    let request = objc2_foundation::NSURLRequest::requestWithURL(&ns_url);
                    self.webview.loadRequest(&request);
                }
            }
        }
    }

    pub fn set_zoom(&self, factor: f64) {
        // SAFETY: main thread WKWebView property. self 는 main thread 객체(Retained<WKWebView>).
        unsafe {
            let _: () = objc2::msg_send![&self.webview, setPageZoom: factor];
        }
    }

    /// 게이트가 붙은 webview에서는 전역 JS를 켜 두고 탐색 단위 preferences로 문서 JS를 정한다.
    /// 전역 값이 꺼져 있으면 탐색 단위 허용도 효과가 없기 때문이다. 바뀐 sandbox 설정은 다음 탐색부터 적용된다.
    pub fn set_javascript_enabled(&self, enabled: bool) {
        let global = match self.script_gate.borrow().as_ref() {
            Some(gate) => {
                gate.set_sandbox(!enabled);
                true
            }
            None => enabled,
        };
        self.set_global_javascript(global);
    }

    fn set_global_javascript(&self, enabled: bool) {
        // SAFETY: main thread. configuration().preferences() 는 main thread KVC 대상.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            let config = self.webview.configuration();
            let prefs = config.preferences();
            let _: () = objc2::msg_send![&prefs, setJavaScriptEnabled: enabled];
        }
    }

    /// 문서 단위 스크립트 허용을 붙인다. 첫 로드 전에 호출한다.
    pub fn attach_script_gate(&self, gate: ScriptGate) {
        self.set_global_javascript(true);
        self.remote.sync(&gate);
        if let Some(http) = &self.http {
            http.session().attach_script_gate(gate.clone());
        }
        *self.script_gate.borrow_mut() = Some(gate);
    }

    /// 팔레트·host 키 경로의 편집 명령. 엔진이 포커스 frame과 편집 가능 여부를 판단한다.
    pub fn execute_edit(&self, action: super::WebViewEditAction) {
        run_editing_command(&self.webview, action);
    }

    pub fn reload(&self) {
        self.webview.ivars().click.cancel();
        self._nav_delegate.ivars().cancel_pending_decision();
        // SAFETY: main thread WKWebView API.
        if unsafe { self.webview.reload() }.is_none() {
            tracing::debug!("WKWebView reload started no navigation");
        }
    }

    pub fn go_back(&self) {
        self.webview.ivars().click.cancel();
        self._nav_delegate.ivars().cancel_pending_decision();
        // SAFETY: main thread WKWebView API.
        if unsafe { self.webview.goBack() }.is_none() {
            tracing::debug!("WKWebView goBack started no navigation");
        }
    }

    pub fn go_forward(&self) {
        self.webview.ivars().click.cancel();
        self._nav_delegate.ivars().cancel_pending_decision();
        // SAFETY: main thread WKWebView API.
        if unsafe { self.webview.goForward() }.is_none() {
            tracing::debug!("WKWebView goForward started no navigation");
        }
    }

    /// 주소 표시용 탐색 정보. 주소 선택 규칙은 `committed_document_url`이 정한다.
    pub fn nav_info(&self) -> crate::model::WebViewNavInfo {
        // SAFETY: main thread WKWebView 속성 읽기. URL()은 Retained 값을 반환한다.
        // 이유: 같은 객체의 읽기 전용 속성 세 개를 한 번에 읽는다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        let (live, can_go_back, can_go_forward) = unsafe {
            (
                self.webview
                    .URL()
                    .and_then(|u| u.absoluteString())
                    .map(|u| u.to_string()),
                self.webview.canGoBack(),
                self.webview.canGoForward(),
            )
        };
        let live = match (&self.http, live) {
            (Some(_), Some(url)) => Some(http::logical(&url)),
            (_, live) => live,
        };
        crate::model::WebViewNavInfo {
            url: crate::model::committed_document_url(
                self.nav_state.get(),
                live,
                self._nav_delegate.ivars().committed_url.borrow().clone(),
            ),
            can_go_back,
            can_go_forward,
        }
    }

    /// 사용자 탐색 조작에 없는 로드 중지를 debug IPC에서 재현한다.
    #[cfg(debug_assertions)]
    pub fn debug_stop(&self) {
        // SAFETY: main thread WKWebView API.
        unsafe { self.webview.stopLoading() };
    }

    /// Follow는 상속, Light/Dark는 AppKit appearance를 지정한다.
    pub fn set_color_scheme(&self, scheme: super::ColorScheme) {
        // SAFETY: main thread AppKit. self 는 main thread 객체(Retained<WKWebView>),
        // WKWebView : NSView 가 setAppearance: 에 응답한다. appearanceNamed: 와
        // NSAppearanceName* 정적은 main thread AppKit API.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            let appearance: Option<Retained<NSAppearance>> = match scheme {
                super::ColorScheme::Follow => None,
                super::ColorScheme::Light => NSAppearance::appearanceNamed(NSAppearanceNameAqua),
                super::ColorScheme::Dark => NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua),
            };
            let _: () = objc2::msg_send![&self.webview, setAppearance: appearance.as_deref()];
        }
    }

    /// 전역 "Allow remote content"를 적용한다. 문서 단위 허용은 게이트가 정하고 실효값은 둘의 OR다.
    /// 규칙 미준비·컴파일 실패 동안의 차단을 보장하지 않는다.
    pub fn set_remote_content_allowed(&self, allowed: bool) {
        if let Some(gate) = self.script_gate.borrow().as_ref() {
            gate.set_remote_global(allowed);
        }
        self.remote.set_global(allowed);
    }

    pub fn load_html(&self, html: &str) {
        if self.entry_source.is_some() || self.http.is_some() {
            self.nav_state.set(NavState::Failed);
            return;
        }
        self.begin_load();
        // SAFETY: main thread WKWebView API. NSString/NSURL은 호출 동안 살아있는 local Retained.
        unsafe {
            let ns_html = NSString::from_str(html);
            let base_url = NSURL::URLWithString(&NSString::from_str("about:blank"));
            self.webview
                .loadHTMLString_baseURL(&ns_html, base_url.as_deref());
        }
    }
}

impl Drop for PlatformWebView {
    fn drop(&mut self) {
        if let Some(source) = &self.entry_source {
            source.retire();
        }
        if let Some(handler) = &self.entry_handler {
            handler.close();
        }
        // WebKit은 부르지 않은 채 해제된 결정 handler를 오류로 본다.
        self._nav_delegate.ivars().cancel_pending_decision();
        if self.http.is_some() {
            // SAFETY: main thread WKWebView API.
            unsafe { self.webview.stopLoading() };
            // Revoke endpoint and body authority before the native view goes away.
            drop(self.http.take());
        }
        self.webview.removeFromSuperview();
    }
}

/// Convert logical bounds (top-left origin) to NSRect,
/// handling macOS coordinate system (bottom-left origin for non-flipped views).
///
/// # Safety
///
/// 호출자는 메인 스레드에서 호출해야 한다 — `parent.frame()`/
/// `parent.isFlipped()`는 AppKit 뷰 상태 조회이며 AppKit은 메인 스레드
/// 전용이다(objc2의 `frame()`/`isFlipped()` 자체는 안전한 `fn`으로
/// 노출되어 있어 이 계약은 objc2가 아니라 AppKit의 스레딩 모델에서 온다).
unsafe fn logical_to_nsrect(parent: &NSView, bounds: WebViewBounds, _scale_factor: f64) -> NSRect {
    let is_flipped = parent.isFlipped();
    let parent_h = parent.frame().size.height;

    let origin_y = if is_flipped {
        bounds.y
    } else {
        parent_h - bounds.y - bounds.height
    };

    NSRect {
        origin: NSPoint::new(bounds.x, origin_y),
        size: NSSize::new(bounds.width, bounds.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn ns_chars_map_to_character_and_named() {
        assert_eq!(ns_chars_to_winit_key("d"), Some(Key::Character("d".into())));
        assert_eq!(ns_chars_to_winit_key("="), Some(Key::Character("=".into())));
        assert_eq!(
            ns_chars_to_winit_key("\u{F708}"),
            Some(Key::Named(NamedKey::F5))
        );
        assert_eq!(
            ns_chars_to_winit_key("\u{1B}"),
            Some(Key::Named(NamedKey::Escape))
        );
        assert_eq!(
            ns_chars_to_winit_key("\u{F710}"),
            Some(Key::Named(NamedKey::F13))
        );
        assert_eq!(
            ns_chars_to_winit_key("\u{F71B}"),
            Some(Key::Named(NamedKey::F24))
        );
        assert_eq!(ns_chars_to_winit_key(""), None);
    }
}
