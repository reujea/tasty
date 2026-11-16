//! 바탕 포커스 상태를 WebView2 스크립트로 둔다. WebView2에는 사용자 스타일시트가 없어, 지금 문서에는
//! `ExecuteScript`로, 그 뒤 싣는 문서에는 문서 생성 스크립트로 같은 adopted stylesheet를 넣는다.
//! 규칙은 두 상태를 모두 적어 페이지가 스스로 선언한 표지도 덮는다. 측정과 한계는
//! `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::core::HSTRING;

use super::super::bed_focus::{ScriptRegistrations, bed_state_script};

#[derive(Default)]
pub(super) struct BedFocusScript {
    /// 마지막으로 둔 값. 같은 값을 다시 받으면 아무것도 하지 않는다.
    state: Cell<Option<bool>>,
    registrations: Rc<RefCell<ScriptRegistrations>>,
}

impl BedFocusScript {
    pub(super) fn set(&self, webview: &ICoreWebView2, surface_id: u32, focused: bool) {
        if self.state.get() == Some(focused) {
            return;
        }
        self.state.set(Some(focused));
        let script = HSTRING::from(bed_state_script(focused));
        self.register(webview, surface_id, &script);
        let done = ExecuteScriptCompletedHandler::create(Box::new(
            move |result, _json| -> windows::core::Result<()> {
                if let Err(e) = result {
                    tracing::warn!(
                        "WebView surface {surface_id}: bed focus script failed ({e}); \
                         the document keeps its previous fill"
                    );
                }
                Ok(())
            },
        ));
        // SAFETY: webview는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
        if let Err(e) = unsafe { webview.ExecuteScript(&script, &done) } {
            tracing::warn!(
                "WebView surface {surface_id}: ExecuteScript failed ({e}); \
                 the document keeps its previous fill"
            );
        }
    }

    /// 다음 문서부터 이 값으로 시작하도록 문서 생성 스크립트를 바꾼다. 앞 등록은 새 등록이 끝난 뒤 지운다.
    fn register(&self, webview: &ICoreWebView2, surface_id: u32, script: &HSTRING) {
        let generation = self.registrations.borrow_mut().begin();
        let registrations = self.registrations.clone();
        let target = webview.clone();
        let registered = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
            move |result, id| -> windows::core::Result<()> {
                if let Err(e) = result {
                    tracing::warn!(
                        "WebView surface {surface_id}: the bed focus document script was not \
                         added ({e}); the next document starts with the previous value"
                    );
                    return Ok(());
                }
                let stale = registrations.borrow_mut().added(generation, id);
                for id in stale {
                    let id = HSTRING::from(id);
                    // SAFETY: target은 이 view의 WebView이고 완료 콜백은 main thread에서 온다.
                    let removed = unsafe { target.RemoveScriptToExecuteOnDocumentCreated(&id) };
                    if let Err(e) = removed {
                        tracing::warn!(
                            "WebView surface {surface_id}: removing an old bed focus document \
                             script failed ({e})"
                        );
                    }
                }
                Ok(())
            },
        ));
        // SAFETY: webview와 handler는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
        let added = unsafe { webview.AddScriptToExecuteOnDocumentCreated(script, &registered) };
        if let Err(e) = added {
            tracing::warn!(
                "WebView surface {surface_id}: adding the bed focus document script failed ({e})"
            );
        }
    }
}
