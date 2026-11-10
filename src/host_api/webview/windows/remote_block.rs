//! `WebResourceRequested`의 원격(`http(s)`) 차단 판정.
//!
//! 실효값은 `전역 설정 || 문서 단위 허용`이다(docs/surfaces/html/index.md "원격 콘텐츠 차단"). 문서 값은
//! 요청마다 게이트에서 읽는다. WebView2에는 응답 단계가 없어 게이트는 `NavigationStarting`에서 그 로드의
//! 값을 정하고, commit 전에는 그 결정만, commit 뒤에는 화면 문서의 허용만 따른다. 막은 하위 리소스
//! 요청은 표지 근거로 게이트에 알린다. 문서(프레임) 요청은 하위 리소스가 아니므로 알리지 않는다.
//!
//! 차단 중에도 연 top-level 탐색([`TopLevelAdmission`])의 main 문서 요청은 보낸다. `NavigationStarting`은
//! main frame 탐색에서만 오므로 시작·리다이렉트마다 허용을 기록하고, 문서 요청은 그 hop 주소와 대조한다.
//! 로드 시작부터 하위 리소스 차단은 그대로라 commit된 새 문서의 하위 리소스는 그 문서의 상태를 따른다.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use tasty_model::html_script::top_level::{Origin, StartKind, TopLevelAdmission, is_remote_url};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::core::BOOL;

use super::super::click_gesture::ClickGesture;
use super::super::script_gate::ScriptGate;

/// host 입력 경로의 입력 증명. 경로를 쓰지 못한 view는 비어 있다.
pub(super) type ClickSlot = Rc<RefCell<Option<Rc<ClickGesture>>>>;

/// 탐색의 사용자 제스처 근거. WebView2의 `IsUserInitiated`는 meta refresh에도 참으로 왔다(Windows 실 기기).
/// 그래서 원격 문서의 사용자 이동 허용에는 host 입력 경로가 본 OS 입력(`ClickGesture`)도 함께 요구한다.
pub(super) struct Gesture<'a> {
    pub(super) initiated: bool,
    pub(super) click: &'a ClickSlot,
}

impl Gesture<'_> {
    /// 클릭 증명은 필요할 때만 소비한다. 한 입력은 main frame 탐색 하나만 증명한다.
    fn proven(&self) -> bool {
        self.initiated
            && self
                .click
                .borrow()
                .as_ref()
                .is_some_and(|click| click.take(Instant::now(), Some(true)))
    }
}

/// 이 원격 요청을 막아야 하면 `true`. 막은 하위 리소스 요청은 게이트에 관측으로 알린다.
pub(super) fn refuse(
    global_allowed: &Cell<bool>,
    gate: &RefCell<Option<ScriptGate>>,
    top_level: &RefCell<TopLevelAdmission>,
    uri: &str,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
) -> bool {
    if global_allowed.get() {
        return false;
    }
    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
    // SAFETY: args는 이 콜백 동안 valid하며 main thread에서 읽는다.
    let read = unsafe { args.ResourceContext(&mut context) };
    let document = read.is_ok() && context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT;
    if document && top_level.borrow().admits(uri) {
        return false;
    }
    let gate = gate.borrow();
    let Some(gate) = gate.as_ref() else {
        return true;
    };
    if gate.document_remote() {
        return false;
    }
    if read.is_ok() && !document {
        gate.remote_block_observed();
    }
    true
}

/// 지금 시작하는 탐색의 출처. 전역 설정은 게이트가 없는 view에도 더한다.
pub(super) fn origin(global_allowed: bool, gate: Option<&ScriptGate>) -> Origin {
    let mut origin = gate.map_or_else(Origin::default, ScriptGate::navigation_origin);
    origin.remote_allowed |= global_allowed;
    origin
}

/// `NavigationStarting`에서 이 hop의 top-level 허용을 기록한다. 리다이렉트는 연 로드를 따라가고, 새 탐색은
/// 앞 로드의 허용을 닫고 출처(지목·화면 문서·사용자 클릭)로 다시 정한다.
pub(super) fn navigation_starting(
    top_level: &RefCell<TopLevelAdmission>,
    args: &ICoreWebView2NavigationStartingEventArgs,
    uri: &str,
    kind: Option<COREWEBVIEW2_NAVIGATION_KIND>,
    gesture: Gesture<'_>,
    origin: Origin,
    surface_id: u32,
) {
    let mut redirected = BOOL(0);
    // SAFETY: args는 이 콜백 동안 valid하며 main thread에서 읽는다. 출력 BOOL은 지역 변수다.
    let redirect = match unsafe { args.IsRedirected(&mut redirected) } {
        Ok(()) => redirected.as_bool(),
        Err(e) => {
            tracing::warn!(
                "WebView surface {surface_id}: IsRedirected failed ({e}); \
                 the hop is treated as a new navigation"
            );
            false
        }
    };
    let mut top = top_level.borrow_mut();
    if redirect {
        let admitted = top.redirect(uri);
        tracing::debug!(
            "WebView surface {surface_id}: top-level redirect {uri} admitted={admitted}"
        );
        return;
    }
    top.end();
    let kind = match kind {
        Some(k) if k == COREWEBVIEW2_NAVIGATION_KIND_RELOAD => StartKind::Reload,
        Some(k) if k == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD => StartKind::History,
        _ => StartKind::Other,
    };
    let user_gesture =
        origin.remote_document && !origin.remote_allowed && is_remote_url(uri) && gesture.proven();
    let admitted = top.start(uri, kind, user_gesture, origin);
    tracing::debug!(
        "WebView surface {surface_id}: top-level navigation {uri} admitted={admitted} \
         (origin allowed={} remote={}, initiated={}, gesture={user_gesture})",
        origin.remote_allowed,
        origin.remote_document,
        gesture.initiated
    );
}
