//! host 전용 주소(`super::super::host_document`)의 WebView2 배선: 예약 호스트 필터, 응답, 새 창 차단.
//! 예약 호스트 요청은 여기서 모두 로컬로 끝낸다. 판정과 본문 표는 OS 무관 모듈에 있다.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::NewWindowRequestedEventHandler;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::core::{BOOL, Error, HSTRING, Interface, PWSTR, Result, w};

use super::super::NavState;
use super::super::host_document::{
    HostDocuments, RESERVED_FILTERS, StartDecision, host_document_url, is_reserved,
    retry_via_host_document,
};
use super::super::load_generation::end_cancelled_navigation;
use tasty_model::html_script::top_level::{StartKind, TopLevelAdmission};

/// view 하나의 예약 호스트 응답 준비물. 404 응답은 미리 만들어 요청 때 실패할 호출을 줄인다.
pub(super) struct Responder {
    environment: ICoreWebView2Environment,
    not_found: ICoreWebView2WebResourceResponse,
}

/// 예약 호스트 필터와 새 창 차단을 단다. 하나라도 실패하면 이 view는 host 전용 주소를 쓰지 않는다.
pub(super) fn install(
    webview: &ICoreWebView2,
    environment: &ICoreWebView2Environment,
) -> Result<Responder> {
    let view22: ICoreWebView2_22 = webview.cast()?;
    for filter in RESERVED_FILTERS {
        let filter = HSTRING::from(filter);
        // SAFETY: live interface on the WebView2 apartment thread; HSTRING lives for the call.
        unsafe {
            view22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                &filter,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        }
    }
    let mut token = 0;
    let handler = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut uri = PWSTR::null();
        // SAFETY: WebView2 supplied live args on its apartment thread; the URI is freed below.
        unsafe { args.Uri(&mut uri)? };
        // SAFETY: Uri succeeded, so uri is a NUL-terminated string.
        let text = unsafe { uri.to_string() }.unwrap_or_default();
        // SAFETY: the caller owns the returned allocation.
        unsafe { CoTaskMemFree(Some(uri.0 as *const std::ffi::c_void)) };
        if is_reserved(&text) {
            // SAFETY: live args on the apartment thread. Handled without a window opens nothing.
            unsafe { args.SetHandled(true)? };
        }
        Ok(())
    }));
    // SAFETY: live WebView and callback on the COM apartment thread. WebView owns the callback.
    unsafe { webview.add_NewWindowRequested(&handler, &mut token)? };
    // SAFETY: environment is live on this thread; the strings are static. The reason phrase follows
    // the other host responses (one word, never shown).
    let not_found = unsafe {
        environment.CreateWebResourceResponse(
            None,
            404,
            w!("Blocked"),
            w!("Cache-Control: no-store\r\n"),
        )?
    };
    Ok(Responder {
        environment: environment.clone(),
        not_found,
    })
}

/// 예약 호스트 요청에 보낸 응답.
struct Reply {
    /// 본문을 실은 200 응답이다(아니면 404).
    body: bool,
    sent: Result<()>,
}

impl Responder {
    /// `WebResourceRequested`의 예약 호스트 요청을 끝낸다. 본문을 실은 응답을 넘긴 뒤에만 표에 알린다
    /// (`HostDocuments::served`). 200 응답을 만들지 못해 404를 보냈거나 넘기지 못한 탐색은 무엇이 commit돼도
    /// 실패로 끝난다(`HostDocuments::navigation_completed`).
    pub(super) fn answer(
        &self,
        documents: &RefCell<HostDocuments>,
        nav_state: &Cell<NavState>,
        subframe_pending: bool,
        surface: u32,
        uri: &str,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
    ) -> Result<()> {
        let reply = self.reply(documents, subframe_pending, uri, args)?;
        match reply.sent {
            Ok(()) if reply.body => documents.borrow_mut().served(),
            Ok(()) => {}
            Err(e) => {
                tracing::error!(
                    "WebView surface {surface}: answering a host document request failed ({e}); \
                     the request may reach name resolution"
                );
                if reply.body {
                    nav_state.set(NavState::Failed);
                }
            }
        }
        Ok(())
    }

    /// 예약 호스트 요청 하나를 끝낸다. 지금 main 탐색의 문서 요청이면 본문을, 아니면 404를 보낸다.
    /// `body`는 본문을 실은 200 응답인지, `sent`는 `SetResponse` 결과다. 실패하면 요청이 이름 해석으로 넘어갈 수 있다.
    fn reply(
        &self,
        documents: &RefCell<HostDocuments>,
        subframe_pending: bool,
        uri: &str,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
    ) -> Result<Reply> {
        // SAFETY: request COM objects are used on the WebView2 apartment thread.
        let (is_get, is_document) = unsafe {
            let request = args.Request()?;
            let mut method = PWSTR::null();
            request.Method(&mut method)?;
            let is_get = method.to_string().is_ok_and(|m| m == "GET");
            CoTaskMemFree(Some(method.0.cast()));
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
            args.ResourceContext(&mut context)?;
            (
                is_get,
                context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
            )
        };
        let body = documents
            .borrow_mut()
            .respond(uri, is_get, is_document, subframe_pending);
        let response = match body.map(|body| self.document(&body)) {
            Some(Ok(response)) => Some(response),
            Some(Err(e)) => {
                tracing::warn!("host document response could not be built ({e}); answering 404");
                None
            }
            None => None,
        };
        let body = response.is_some();
        let response = response.unwrap_or_else(|| self.not_found.clone());
        // SAFETY: live args and response on the apartment thread.
        let sent = unsafe { args.SetResponse(&response) };
        Ok(Reply { body, sent })
    }

    fn document(&self, body: &Arc<str>) -> Result<ICoreWebView2WebResourceResponse> {
        // The memory stream owns its byte copy.
        // SAFETY: SHCreateMemStream copies the slice; the environment is live on this thread.
        unsafe {
            let stream = SHCreateMemStream(Some(body.as_bytes()))
                .ok_or_else(|| Error::from_hresult(windows::Win32::Foundation::E_OUTOFMEMORY))?;
            let headers = format!(
                "Content-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
                 X-Content-Type-Options: nosniff\r\n",
                body.len()
            );
            let headers = HSTRING::from(headers);
            self.environment
                .CreateWebResourceResponse(&stream, 200, w!("OK"), &headers)
        }
    }
}

/// main frame commit(`ContentLoading`)을 표에 알린다. 오류 페이지 commit은 보이던 host 전용 문서를
/// 물리지 않는다. 조회에 실패하면 정상 문서로 본다.
pub(super) fn committed(
    documents: &RefCell<HostDocuments>,
    nav: Option<u64>,
    args: Option<&ICoreWebView2ContentLoadingEventArgs>,
) {
    let error_page = args.is_some_and(|a| {
        let mut page = BOOL(0);
        // SAFETY: live args on the apartment thread.
        unsafe { a.IsErrorPage(&mut page) }.is_ok() && page.as_bool()
    });
    documents.borrow_mut().committed(nav, error_page);
}

/// 탐색 완료를 표에 알리고 chrome이 볼 성공 여부를 돌려준다. 예약 주소가 로컬 본문 없이 끝났으면 실패다.
pub(super) fn completed(
    documents: &RefCell<HostDocuments>,
    surface: u32,
    nav: Option<u64>,
    success: bool,
) -> bool {
    let unserved = documents.borrow_mut().navigation_completed(nav, success);
    if unserved {
        tracing::warn!(
            "WebView surface {surface}: the host document address committed without the local \
             document; the load counts as failed"
        );
    }
    success && !unserved
}

/// WebView2 process 종료를 표에 알린다. 렌더 process만 끝났으면 commit된 문서는 다시 읽을 수 있게 남긴다.
pub(super) fn process_ended(documents: &RefCell<HostDocuments>, renderer: bool) {
    if renderer {
        documents.borrow_mut().renderer_ended();
    } else {
        documents.borrow_mut().browser_ended();
    }
}

/// main frame `NavigationStarting`을 표에 알린다(`HostDocuments::navigation_started`). 예약 주소인데
/// 호스트가 보낸 다시 싣기도 commit된 문서의 Reload도 아니면(본문 안 상대 링크, 페이지가 스스로 연 주소,
/// 그 주소로의 리다이렉트 등) 시작 전에 취소하고 `Cancel`을 돌려준다. chrome을 Loading으로 바꾸기 전이라
/// 보이던 문서를 그대로 둔다. 취소한 탐색이 chrome의 지금 로드면 Failed로 끝내고, 아니면 그 ID를 표에
/// 적어 완료가 chrome을 바꾸지 않게 한다(`end_cancelled_navigation`).
pub(super) fn start(
    documents: &RefCell<HostDocuments>,
    chrome: (&Cell<NavState>, &Cell<Option<u64>>),
    top_level: &RefCell<TopLevelAdmission>,
    surface: u32,
    args: Option<&ICoreWebView2NavigationStartingEventArgs>,
) -> Result<StartDecision> {
    let Some(args) = args else {
        return Ok(StartDecision::NotReserved);
    };
    let mut uri = PWSTR::null();
    // SAFETY: live args on the apartment thread; the URI is freed below.
    unsafe { args.Uri(&mut uri)? };
    // SAFETY: Uri succeeded, so uri is a NUL-terminated string.
    let text = unsafe { uri.to_string() }.unwrap_or_default();
    // SAFETY: the caller owns the returned allocation.
    unsafe { CoTaskMemFree(Some(uri.0 as *const std::ffi::c_void)) };
    let mut id = 0u64;
    // SAFETY: live args on the apartment thread.
    let nav = unsafe { args.NavigationId(&mut id) }.ok().map(|()| id);
    let mut redirected = BOOL(0);
    // SAFETY: live args on the apartment thread. A failed query counts as a new navigation.
    let redirected = unsafe { args.IsRedirected(&mut redirected) }.is_ok() && redirected.as_bool();
    let decision =
        documents
            .borrow_mut()
            .navigation_started(&text, nav, start_kind(args), redirected);
    if decision != StartDecision::Cancel {
        return Ok(decision);
    }
    // SAFETY: live args on the apartment thread.
    unsafe { args.SetCancel(true)? };
    if end_cancelled_navigation(chrome.0, chrome.1, top_level, nav, redirected) {
        documents.borrow_mut().cancelled(nav);
    }
    tracing::debug!(
        "WebView surface {surface}: cancelled a navigation to a host document address the host did \
         not send"
    );
    Ok(StartDecision::Cancel)
}

/// WebView2가 보고한 탐색 종류. 조회하지 못하면 `Other`다(Reload로 보지 않는다).
fn start_kind(args: &ICoreWebView2NavigationStartingEventArgs) -> StartKind {
    let kind = args
        .cast::<ICoreWebView2NavigationStartingEventArgs3>()
        .and_then(|a| {
            let mut k = COREWEBVIEW2_NAVIGATION_KIND::default();
            // SAFETY: live args on the apartment thread.
            unsafe { a.NavigationKind(&mut k) }.map(|()| k)
        });
    match kind {
        Ok(k) if k == COREWEBVIEW2_NAVIGATION_KIND_RELOAD => StartKind::Reload,
        Ok(k) if k == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD => StartKind::History,
        _ => StartKind::Other,
    }
}

/// `NavigateToString`이 거절한 문서를 host 전용 주소로 한 번 다시 싣는 준비물. 클로저가 본문을 소유해
/// bed focus 장벽에 붙잡힌 앞 로드도 자기 본문을 잃지 않는다.
pub(super) struct Fallback {
    pub(super) documents: std::rc::Rc<RefCell<HostDocuments>>,
    pub(super) allowed: std::rc::Rc<std::cell::Cell<bool>>,
    pub(super) latest: std::rc::Rc<std::cell::Cell<u64>>,
    pub(super) load: u64,
    pub(super) surface: u32,
    pub(super) view: u64,
    pub(super) body: Arc<str>,
}

impl Fallback {
    /// 거절 `rejected`가 다시 싣기 대상이면 host 전용 주소로 `Navigate`한다. 대상이 아니거나 다시 싣기도
    /// 거절되면 `rejected`를 돌려준다(호출자가 `NavigateToString` 거절 줄을 남기고 chrome을 Failed로 끝낸다).
    /// 다시 싣기의 거절은 두 호출이 로그에서 구별되도록 `host document Navigate` 줄로 따로 남긴다.
    pub(super) fn retry(self, webview: &ICoreWebView2, rejected: Error) -> Result<()> {
        if !retry_via_host_document(
            rejected.code().0,
            &self.body,
            self.latest.get(),
            self.load,
            self.allowed.get(),
        ) {
            return Err(rejected);
        }
        let url = host_document_url(self.surface, self.view, self.load);
        let target = HSTRING::from(url.as_str());
        self.documents.borrow_mut().sent(self.load, url, self.body);
        // SAFETY: live WebView on its apartment thread; the HSTRING lives for the call.
        if let Err(e) = unsafe { webview.Navigate(&target) } {
            tracing::warn!(
                "WebView surface {}: WebView2 host document Navigate failed: {e}",
                self.surface
            );
            self.documents.borrow_mut().rejected(self.load);
            return Err(rejected);
        }
        tracing::info!(
            "WebView surface {}: NavigateToString refused the document ({rejected}); \
             loading it from the host document address",
            self.surface
        );
        Ok(())
    }
}
