//! 같은 문서 fragment 이동을 `HistoryChanged`로 받아 탐색 큐에 넣는다. 주소 판정과 큐 규칙은 상위
//! `same_document`에 있다. 사용자 제스처는 host 입력 경로가 본 OS 입력(왼쪽 클릭, 페이지로 간
//! Enter·Space, `ClickGesture`)으로 정한다. 이 이벤트에는 `IsUserInitiated`가 없다.
//! host 입력 경로를 쓰지 못한 view는 입력을 볼 수 없어 제스처가 늘 거짓이다.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{RDW_INTERNALPAINT, RedrawWindow};
use windows::core::HSTRING;

use super::super::PendingNavigation;
use super::super::click_gesture::ClickGesture;
use super::super::same_document::{HREF_SCRIPT, INIT_SCRIPT, from_href_result};

/// `webview`의 같은 문서 fragment 이동을 `pending`에 쌓게 한다. 첫 문서를 열기 전에 부른다.
/// 쌓으면 `parent`(winit 창)에 그리기를 요청한다. Windows에는 WebView 폴링이 없어, 깨우지 않으면
/// 다음 입력이나 타이머까지 큐를 꺼내지 않는다.
pub(super) fn install(
    webview: &ICoreWebView2,
    surface_id: u32,
    parent: HWND,
    pending: &Rc<RefCell<Vec<PendingNavigation>>>,
    click: Option<Rc<ClickGesture>>,
) -> windows::core::Result<()> {
    let registered = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
        move |result, _id| -> windows::core::Result<()> {
            if let Err(e) = result {
                tracing::warn!(
                    "WebView surface {surface_id}: the same-document address script was not \
                     added ({e}); fragment moves are not reported"
                );
            }
            Ok(())
        },
    ));
    // SAFETY: webview와 handler는 호출 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
    unsafe {
        webview.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(INIT_SCRIPT), &registered)?
    };
    let pending = pending.clone();
    let handler = HistoryChangedEventHandler::create(Box::new(
        move |sender, _args| -> windows::core::Result<()> {
            let Some(webview) = sender else {
                return Ok(());
            };
            // 입력은 이벤트 시점에 소비한다. 새 문서의 기록 변경도 소비하므로 한 입력은 한 이동만 증명한다.
            // 주소는 스크립트 결과로 늦게 오므로 그 사이의 입력과 섞지 않는다.
            let user_gesture = click
                .as_ref()
                .is_some_and(|click| click.take(Instant::now(), Some(true)));
            let pending = pending.clone();
            let done = ExecuteScriptCompletedHandler::create(Box::new(
                move |result, json| -> windows::core::Result<()> {
                    if let Err(e) = result {
                        tracing::warn!(
                            "WebView surface {surface_id}: reading the moved address failed ({e}); \
                             the same-document navigation is not reported"
                        );
                        return Ok(());
                    }
                    let nav = from_href_result(&json, user_gesture, &pending.borrow());
                    if let Some(nav) = nav {
                        pending.borrow_mut().push(nav);
                        // winit `request_redraw`와 같은 요청이다. 무효화 영역 없이 WM_PAINT만 만든다.
                        // SAFETY: parent는 이 view를 만든 winit 창이며 GUI 스레드 호출이다.
                        if !unsafe { RedrawWindow(Some(parent), None, None, RDW_INTERNALPAINT) }
                            .as_bool()
                        {
                            tracing::warn!(
                                "WebView surface {surface_id}: RedrawWindow failed; the moved \
                                 address waits for the next frame"
                            );
                        }
                    }
                    Ok(())
                },
            ));
            // SAFETY: webview는 이벤트 동안 valid, main thread 호출. HSTRING은 호출 끝까지 산다.
            if let Err(e) = unsafe { webview.ExecuteScript(&HSTRING::from(HREF_SCRIPT), &done) } {
                tracing::warn!(
                    "WebView surface {surface_id}: ExecuteScript failed ({e}); \
                     the same-document navigation is not reported"
                );
            }
            Ok(())
        },
    ));
    let mut token: i64 = 0;
    // SAFETY: webview와 handler는 호출 동안 valid, main thread 호출. 출력 token은 지역 변수다.
    unsafe { webview.add_HistoryChanged(&handler, &mut token) }
}
