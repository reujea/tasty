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
        let Some(work) = self.delivery.borrow_mut().request(focused) else {
            return;
        };
        let script = HSTRING::from(bed_state_script(focused));
        if let Some(generation) = work.register {
            register(&self.delivery, webview, surface_id, generation, &script);
        }
        if let Some(number) = work.apply {
            apply(&self.delivery, webview, surface_id, number, &script);
        }
    }

    /// 호스트 탐색을 보낸다. 마지막 요청의 문서 생성 스크립트 등록이 끝나지 않았으면 끝날 때 보낸다.
    pub(super) fn navigate(&self, surface_id: u32, call: &str, navigation: Navigation) {
        let now = self.delivery.borrow_mut().navigate(navigation);
        match now {
            Some(navigation) => navigation(),
            None => tracing::debug!(
                "WebView surface {surface_id}: {call} waits for the bed focus document script"
            ),
        }
    }

    /// view가 사라진다. 붙잡은 탐색을 버리고 늦게 온 완료가 아무것도 하지 않게 한다.
    pub(super) fn close(&self) {
        self.delivery.borrow_mut().close();
    }

    /// 마지막 요청이 실패했는지 돌려주고 표시를 지운다. 참이면 같은 값을 다시 받아 적용한다.
    pub(super) fn take_failed(&self) -> bool {
        self.delivery.borrow_mut().take_failed()
    }
}

/// 지금 문서에 `number` 적용으로 스크립트를 실행한다. 실패하면 마지막 적용일 때 실패로 둔다.
fn apply(
    delivery: &Delivery,
    webview: &ICoreWebView2,
    surface_id: u32,
    number: u64,
    script: &HSTRING,
) {
    let on_done = delivery.clone();
    let done = ExecuteScriptCompletedHandler::create(Box::new(
        move |result, _json| -> windows::core::Result<()> {
            if let Err(e) = result {
                note_failure(
                    &on_done,
                    surface_id,
                    format_args!("the bed focus script failed ({e})"),
                );
                on_done.borrow_mut().apply_failed(number);
            }
            Ok(())
        },
    ));
    // SAFETY: webview는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
    let executed = unsafe { webview.ExecuteScript(script, &done) };
    if let Err(e) = executed {
        note_failure(
            delivery,
            surface_id,
            format_args!("ExecuteScript for bed focus failed ({e})"),
        );
        delivery.borrow_mut().apply_failed(number);
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
                    note_failure(
                        &on_done,
                        surface_id,
                        format_args!("the bed focus document script was not added ({e})"),
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
                    note_failure(
                        &on_done,
                        surface_id,
                        format_args!("removing an old bed focus document script failed ({e})"),
                    );
                }
            }
            if !navigations.is_empty() {
                tracing::debug!(
                    "WebView surface {surface_id}: bed focus document script {generation} done; \
                     sending {} held navigation(s)",
                    navigations.len()
                );
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
        note_failure(
            delivery,
            surface_id,
            format_args!("adding the bed focus document script failed ({e})"),
        );
        let navigations = delivery.borrow_mut().registration_failed(generation);
        for navigation in navigations {
            navigation();
        }
    }
}

/// 실패를 view마다 첫 번째만 warn, 그 뒤는 debug로 남긴다. 실패가 계속되는 view가 로그를 채우지 않게 한다.
fn note_failure(delivery: &Delivery, surface_id: u32, what: std::fmt::Arguments<'_>) {
    if delivery.borrow_mut().first_failure() {
        tracing::warn!(
            "WebView surface {surface_id}: {what} (later bed focus failures are logged at debug)"
        );
    } else {
        tracing::debug!("WebView surface {surface_id}: {what}");
    }
}
