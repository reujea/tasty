//! 바탕 포커스 상태를 WebView2 스크립트로 둔다. WebView2에는 사용자 스타일시트가 없어, 지금 문서에는
//! `ExecuteScript`로, 그 뒤 싣는 문서에는 문서 생성 스크립트로 같은 adopted stylesheet를 넣는다.
//! 규칙은 두 상태를 모두 적어 페이지가 스스로 선언한 표지도 덮는다. 등록 완료가 비동기라, 등록이
//! 끝나기 전의 호스트 탐색은 붙잡았다가 끝나면 보낸다(`BedFocusDelivery`). 측정과 한계는
//! `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

use std::cell::RefCell;
use std::rc::Rc;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::core::HSTRING;

use super::super::bed_focus::{BedFocusDelivery, bed_state_script};

/// 등록이 끝나기를 기다리는 호스트 탐색.
pub(super) type Navigation = Box<dyn FnOnce()>;

type Delivery = Rc<RefCell<BedFocusDelivery<Navigation>>>;

#[derive(Default)]
pub(super) struct BedFocusScript {
    delivery: Delivery,
}

impl BedFocusScript {
    pub(super) fn set(&self, webview: &ICoreWebView2, surface_id: u32, focused: bool) {
        let Some(generation) = self.delivery.borrow_mut().request(focused) else {
            return;
        };
        let script = HSTRING::from(bed_state_script(focused));
        register(&self.delivery, webview, surface_id, generation, &script);
        let delivery = self.delivery.clone();
        let done = ExecuteScriptCompletedHandler::create(Box::new(
            move |result, _json| -> windows::core::Result<()> {
                if let Err(e) = result {
                    tracing::warn!(
                        "WebView surface {surface_id}: bed focus script failed ({e}); \
                         it is sent again"
                    );
                    delivery.borrow_mut().apply_failed(generation);
                }
                Ok(())
            },
        ));
        // SAFETY: webview는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
        let executed = unsafe { webview.ExecuteScript(&script, &done) };
        if let Err(e) = executed {
            tracing::warn!(
                "WebView surface {surface_id}: ExecuteScript failed ({e}); it is sent again"
            );
            self.delivery.borrow_mut().apply_failed(generation);
        }
    }

    /// 호스트 탐색을 보낸다. 마지막 요청의 문서 생성 스크립트 등록이 끝나지 않았으면 끝날 때 보낸다.
    pub(super) fn navigate(&self, navigation: Navigation) {
        let now = self.delivery.borrow_mut().navigate(navigation);
        if let Some(navigation) = now {
            navigation();
        }
    }

    /// 마지막 요청이 실패했는지 돌려주고 표시를 지운다. 참이면 같은 값을 다시 받아 적용한다.
    pub(super) fn take_failed(&self) -> bool {
        self.delivery.borrow_mut().take_failed()
    }
}

/// `generation`의 문서 생성 스크립트를 등록한다. 완료되면 앞 등록을 지우고 붙잡았던 탐색을 받은 순서대로 보낸다.
fn register(
    delivery: &Delivery,
    webview: &ICoreWebView2,
    surface_id: u32,
    generation: u64,
    script: &HSTRING,
) {
    let on_done = delivery.clone();
    let target = webview.clone();
    let registered = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
        move |result, id| -> windows::core::Result<()> {
            let (stale, navigations) = match result {
                Ok(()) => on_done.borrow_mut().registered(generation, id),
                Err(e) => {
                    tracing::warn!(
                        "WebView surface {surface_id}: the bed focus document script was not \
                         added ({e}); it is sent again"
                    );
                    (
                        Vec::new(),
                        on_done.borrow_mut().registration_failed(generation),
                    )
                }
            };
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
            for navigation in navigations {
                navigation();
            }
            Ok(())
        },
    ));
    // SAFETY: webview와 handler는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
    let added = unsafe { webview.AddScriptToExecuteOnDocumentCreated(script, &registered) };
    if let Err(e) = added {
        tracing::warn!(
            "WebView surface {surface_id}: adding the bed focus document script failed ({e}); \
             it is sent again"
        );
        let navigations = delivery.borrow_mut().registration_failed(generation);
        for navigation in navigations {
            navigation();
        }
    }
}
