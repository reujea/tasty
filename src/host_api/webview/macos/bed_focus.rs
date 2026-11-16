//! 바탕 포커스 상태를 WKWebView 문서에 둔다. WKWebView에는 공개 사용자 스타일시트 API가 없어
//! 스크립트로 `:root` 인라인 스타일에 계약 속성을 쓴다(`super::super::bed_focus::bed_focus_script`).
//! 지금 문서에는 `evaluateJavaScript`로 쓰고, 그 뒤 싣는 문서에는 같은 내용의 문서 시작 사용자
//! 스크립트가 첫 렌더 전에 쓴다. 측정은 `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

use std::cell::{Cell, RefCell};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{WKUserScript, WKUserScriptInjectionTime, WKWebView};

use crate::webview::bed_focus::bed_focus_script;

#[derive(Default)]
pub(super) struct BedFocus {
    /// 마지막으로 둔 값. 같은 값을 다시 받으면 아무것도 하지 않는다.
    state: Cell<Option<bool>>,
    /// 지금 컨트롤러에 들어 있는 이 상태의 문서 시작 스크립트.
    script: RefCell<Option<Retained<WKUserScript>>>,
}

impl BedFocus {
    pub(super) fn set(&self, webview: &WKWebView, focused: bool, mtm: MainThreadMarker) {
        if self.state.get() == Some(focused) {
            return;
        }
        let source = NSString::from_str(&bed_focus_script(focused));
        self.replace_document_start_script(webview, &source, mtm);
        let handler = RcBlock::new(move |_result: *mut AnyObject, err: *mut NSError| {
            // SAFETY: WebKit이 main thread에서 완료를 부르며 err는 그동안 유효하거나 null이다.
            if let Some(err) = unsafe { err.as_ref() } {
                tracing::warn!(
                    focused,
                    "bed focus script failed: {}",
                    err.localizedDescription()
                );
            }
        });
        // SAFETY: main thread WKWebView API. WebKit이 블록을 복사해 완료 때 한 번 부른다.
        unsafe { webview.evaluateJavaScript_completionHandler(&source, Some(&handler)) };
        self.state.set(Some(focused));
    }

    /// 이 상태의 문서 시작 스크립트를 새 값으로 바꾼다. 컨트롤러에는 하나씩 빼는 공개 API가 없어
    /// 모두 뺀 뒤 다른 스크립트(WebRTC 완화 등)를 순서대로 다시 넣고 새 스크립트를 더한다.
    fn replace_document_start_script(
        &self,
        webview: &WKWebView,
        source: &NSString,
        mtm: MainThreadMarker,
    ) {
        // SAFETY: main thread. 새 page world 스크립트이고 아래 컨트롤러가 보관한다.
        let script = unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                mtm.alloc(),
                source,
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
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
