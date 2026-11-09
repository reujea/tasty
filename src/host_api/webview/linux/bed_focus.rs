//! 바탕 포커스 상태를 WebView의 사용자 스타일시트로 둔다. 스크립트를 실행하지 않으므로 JS를 끈 문서에도
//! 적용된다. 스타일시트는 UserContentManager에 남아 그 뒤 싣는 문서에도 첫 렌더부터 적용되므로 문서 교체와
//! 순서 경합이 없다. 측정은 `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

use std::cell::Cell;

use webkit2gtk::{
    UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebView,
    WebViewExt,
};

use crate::webview::bed_focus::BED_FOCUSED_SHEET;

pub(super) struct BedFocusSheet {
    sheet: UserStyleSheet,
    /// 스타일시트가 지금 들어 있는지. 같은 값을 다시 받으면 아무것도 하지 않는다.
    on: Cell<bool>,
}

impl BedFocusSheet {
    pub(super) fn new() -> Self {
        Self {
            sheet: UserStyleSheet::new(
                BED_FOCUSED_SHEET,
                UserContentInjectedFrames::TopFrame,
                UserStyleLevel::User,
                &[],
                &[],
            ),
            on: Cell::new(false),
        }
    }

    pub(super) fn set(&self, webview: &WebView, focused: bool) {
        if self.on.get() == focused {
            return;
        }
        let Some(ucm) = WebViewExt::user_content_manager(webview) else {
            tracing::warn!("WebView has no user content manager; bed focus is not applied");
            return;
        };
        if focused {
            ucm.add_style_sheet(&self.sheet);
        } else {
            ucm.remove_style_sheet(&self.sheet);
        }
        self.on.set(focused);
    }
}
