//! 바탕 포커스 상태를 WKWebView 문서에 둔다. WKWebView에는 공개 사용자 스타일시트 API가 없어
//! 스크립트로 `:root` 인라인 스타일에 계약 속성을 쓴다(`super::super::bed_focus::bed_focus_script`).
//! 지금 문서에는 `evaluateJavaScript`로 쓰고, 그 뒤 싣는 문서에는 같은 내용의 문서 시작 사용자
//! 스크립트가 첫 렌더 전에 쓴다. 둘 다 페이지와 분리된 client world에서 실행한다. page world의
//! `evaluateJavaScript`는 페이지 스크립트를 끈 문서(`sandbox_scripts`)에서 실패한다.
//! 지금 문서 적용의 실패와 성공한 문서 탐색을 상위층에 알린다(`BedFocusApply`). 문서 시작 스크립트는
//! 같은 호출 안에서 바뀌고 실패를 알리지 않아 Windows의 등록 세대와 탐색 장벽이 없다.
//! 측정은 `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

use std::cell::RefCell;
use std::rc::Rc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{WKContentWorld, WKUserScript, WKUserScriptInjectionTime, WKWebView};

use crate::webview::bed_focus::{BedFocusApply, bed_focus_script};

/// 바탕 포커스 전달 기록을 함께 쓰는 핸들.
pub(super) type Delivery = Rc<RefCell<BedFocusApply>>;

#[derive(Default)]
pub(super) struct BedFocus {
    /// 요청 값과 적용 번호, 상위층에 알릴 실패와 탐색 완료. 적용 완료와 navigation delegate가 함께 쓴다.
    delivery: Delivery,
    /// 지금 컨트롤러에 들어 있는 이 상태의 문서 시작 스크립트.
    script: RefCell<Option<Retained<WKUserScript>>>,
}

impl super::PlatformWebView {
    /// 바탕 포커스 상태를 문서에 둔다. 다음에 싣는 문서에도 남는다.
    pub fn set_bed_focused(&self, focused: bool) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("set_bed_focused called off the main thread; bed focus is not applied");
            return;
        };
        self.bed_focus.set(&self.webview, focused, mtm);
    }

    /// 마지막으로 둔 바탕 포커스 값의 지금 문서 적용이 실패했는지 돌려주고 표시를 지운다.
    pub fn take_bed_focus_failed(&self) -> bool {
        self.bed_focus.delivery.borrow_mut().take_failed()
    }

    /// 지난번 뒤로 문서 탐색이 성공으로 끝났는지 돌려주고 표시를 지운다.
    pub fn take_bed_focus_document_loaded(&self) -> bool {
        self.bed_focus.delivery.borrow_mut().take_loaded()
    }
}

impl BedFocus {
    /// navigation delegate가 탐색 완료를 적는 기록.
    pub(super) fn delivery(&self) -> Delivery {
        self.delivery.clone()
    }

    fn set(&self, webview: &WKWebView, focused: bool, mtm: MainThreadMarker) {
        let Some(work) = self.delivery.borrow_mut().request(focused) else {
            return;
        };
        let source = NSString::from_str(&bed_focus_script(focused));
        if work.replace_script {
            self.replace_document_start_script(webview, &source, mtm);
        }
        let number = work.apply;
        let delivery = self.delivery.clone();
        let handler = RcBlock::new(move |_result: *mut AnyObject, err: *mut NSError| {
            // SAFETY: WebKit이 main thread에서 완료를 부르며 err는 그동안 유효하거나 null이다.
            let Some(err) = (unsafe { err.as_ref() }) else {
                return;
            };
            let mut delivery = delivery.borrow_mut();
            delivery.apply_failed(number);
            let reason = err.localizedDescription();
            if delivery.first_failure() {
                tracing::warn!(focused, number, "bed focus script failed: {reason}");
            } else {
                tracing::debug!(focused, number, "bed focus script failed: {reason}");
            }
        });
        // SAFETY: main thread WKWebView API. frame이 None이면 main frame이다. WebKit이 블록을 복사해
        // 완료 때 한 번 부른다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            webview.evaluateJavaScript_inFrame_inContentWorld_completionHandler(
                &source,
                None,
                &WKContentWorld::defaultClientWorld(mtm),
                Some(&handler),
            )
        };
    }

    /// 이 상태의 문서 시작 스크립트를 새 값으로 바꾼다. 컨트롤러에는 하나씩 빼는 공개 API가 없어
    /// 모두 뺀 뒤 다른 스크립트(WebRTC 완화 등)를 순서대로 다시 넣고 새 스크립트를 더한다.
    fn replace_document_start_script(
        &self,
        webview: &WKWebView,
        source: &NSString,
        mtm: MainThreadMarker,
    ) {
        // SAFETY: main thread. 새 client world 스크립트이고 아래 컨트롤러가 보관한다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        let script = unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly_inContentWorld(
                mtm.alloc(),
                source,
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
                &WKContentWorld::defaultClientWorld(mtm),
            )
        };
        let mut current = self.script.borrow_mut();
        // SAFETY: main thread WKWebView 속성 읽기와 컨트롤러 변경. 컨트롤러가 다시 넣은 스크립트를 보관한다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            let controller = webview.configuration().userContentController();
            let kept: Vec<Retained<WKUserScript>> = controller
                .userScripts()
                .to_vec()
                .into_iter()
                .filter(|s| {
                    current
                        .as_ref()
                        .is_none_or(|old| Retained::as_ptr(old) != Retained::as_ptr(s))
                })
                .collect();
            controller.removeAllUserScripts();
            for s in &kept {
                controller.addUserScript(s);
            }
            controller.addUserScript(&script);
        }
        *current = Some(script);
    }
}
