//! `WebResourceRequested`의 원격(`http(s)`) 차단 판정.
//!
//! 실효값은 `전역 설정 || 문서 단위 허용`이다(docs/surfaces/html/index.md "원격 콘텐츠 차단"). 문서 값은
//! 요청마다 게이트에서 읽는다. WebView2에는 응답 단계가 없어 게이트는 `NavigationStarting`에서 그 로드의
//! 값을 정하고, commit 전에는 그 결정만, commit 뒤에는 화면 문서의 허용만 따른다. 막은 하위 리소스
//! 요청은 표지 근거로 게이트에 알린다. 문서(프레임) 요청은 하위 리소스가 아니므로 알리지 않는다.

use std::cell::{Cell, RefCell};

use webview2_com::Microsoft::Web::WebView2::Win32::*;

use super::super::script_gate::ScriptGate;

/// 이 원격 요청을 막아야 하면 `true`. 막은 하위 리소스 요청은 게이트에 관측으로 알린다.
pub(super) fn refuse(
    global_allowed: &Cell<bool>,
    gate: &RefCell<Option<ScriptGate>>,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
) -> bool {
    if global_allowed.get() {
        return false;
    }
    let gate = gate.borrow();
    let Some(gate) = gate.as_ref() else {
        return true;
    };
    if gate.document_remote() {
        return false;
    }
    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
    // SAFETY: args는 이 콜백 동안 valid하며 main thread에서 읽는다.
    let read = unsafe { args.ResourceContext(&mut context) };
    if read.is_ok() && context != COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT {
        gate.remote_block_observed();
    }
    true
}
