//! WebView2 navigation helpers shared by the event handlers and the view methods.
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::HSTRING;

use super::super::NavState;
use super::super::process_end::ProcessEnd;
use super::super::scan_resume::{Replay, ScanResume};
use super::super::script_gate::ScriptGate;

/// 스캔을 기다리려고 취소하거나 멈춘 탐색을 같은 종류로 다시 시작한다.
pub(super) fn replay_navigation(
    webview: &ICoreWebView2,
    replay: Replay,
    url: &str,
) -> windows::core::Result<()> {
    // SAFETY: webview는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
    unsafe {
        match replay {
            Replay::Reload => webview.Reload(),
            Replay::Navigate => webview.Navigate(&HSTRING::from(url)),
            Replay::Back => webview.GoBack(),
            Replay::Forward => webview.GoForward(),
        }
    }
}

/// 붙잡아 둔 문서 요청을 놓아 준다. 확인을 통과했거나 그 탐색이 이미 끝난 요청이다.
pub(super) fn complete_requests(surface_id: u32, deferrals: Vec<ICoreWebView2Deferral>) {
    for deferral in deferrals {
        // SAFETY: deferral은 WebResourceRequested에서 받은 COM 객체이고 main thread 호출이다.
        if let Err(e) = unsafe { deferral.Complete() } {
            tracing::warn!(
                "WebView surface {surface_id}: completing a held document request failed: {e}"
            );
        }
    }
}

/// 확인하지 못했거나 지문이 다른 본문의 탐색을 멈춘다. 멈춘 탐색은 오류로 끝나 `NavigationCompleted`가
/// 온다(그 완료가 재시작을 준다). `Stop`이 실패하면 완료가 오지 않을 수 있으므로 이 로드를 실패로 끝내
/// Loading에 남지 않게 한다. `body_held`는 그 탐색의 문서 요청을 deferral로 붙잡고 있는지다.
/// 붙잡지 못했거나 이미 commit했으면 `Stop` 실패 뒤 그 본문은 확인 없이 그려질 수 있다.
pub(super) fn stop_navigation(
    surface_id: u32,
    webview: &ICoreWebView2,
    body_held: bool,
    resume: &RefCell<ScanResume<ICoreWebView2Deferral>>,
    gate: Option<&ScriptGate>,
    nav_state: &Cell<NavState>,
) {
    // SAFETY: webview는 호출 동안 valid, main thread 호출.
    let Err(e) = (unsafe { webview.Stop() }) else {
        return;
    };
    if body_held {
        tracing::warn!(
            "WebView surface {surface_id}: Stop failed ({e}); the load ends as failed and the \
             document request stays held"
        );
    } else {
        tracing::error!(
            "WebView surface {surface_id}: Stop failed ({e}) for a document that is not held; \
             the load ends as failed but the unchecked document may still be shown"
        );
    }
    resume.borrow_mut().stop_failed();
    if let Some(gate) = gate {
        gate.failed();
        if let Some(js) = gate.finished() {
            set_js(webview, js);
        }
    }
    nav_state.set(NavState::Failed);
}

/// `Source`를 문자열로 읽고 COM 할당을 해제한다.
pub(super) fn webview_source(wv: &ICoreWebView2) -> windows::core::Result<Option<String>> {
    let mut uri = windows::core::PWSTR::null();
    // SAFETY: wv는 호출 동안 valid, main thread 호출. 반환 PWSTR은 이 함수가 해제한다.
    unsafe { wv.Source(&mut uri)? };
    // SAFETY: Source가 성공하면 uri는 NUL로 끝나는 유효한 문자열이다.
    let text = unsafe { uri.to_string() }.ok();
    // SAFETY: 반환 URI의 할당은 호출자가 해제한다.
    unsafe { CoTaskMemFree(Some(uri.0 as *const c_void)) };
    Ok(text)
}

pub(super) fn set_js(webview: &ICoreWebView2, enabled: bool) {
    // SAFETY: webview/settings는 호출 동안 valid, main thread 호출.
    unsafe {
        match webview.Settings() {
            Ok(settings) => {
                if let Err(e) = settings.SetIsScriptEnabled(enabled) {
                    tracing::warn!("WebView2 SetIsScriptEnabled failed: {e}");
                }
            }
            Err(e) => tracing::warn!("WebView2 Settings() failed: {e}"),
        }
    }
}

/// `ProcessFailed`의 종류 가운데 main frame 문서를 그리던 process의 종료만 고른다. iframe·GPU·utility process
/// 종료와 `RENDER_PROCESS_UNRESPONSIVE`는 main frame 로드를 끝내지 않는다.
pub(super) fn process_end(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> ProcessEnd {
    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
        ProcessEnd::Renderer
    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
        ProcessEnd::Browser
    } else {
        ProcessEnd::Other
    }
}
