//! 원격(`http(s)`) 차단의 실효값을 WKContentRuleList에 적용한다.
//!
//! 실효값은 `전역 설정 || 탐색 결정 단계에서 정한 문서 단위 허용` 하나다(docs/surfaces/html/index.md
//! "원격 콘텐츠 차단"). 전역 설정과 게이트의 문서 값은 이 한 곳에서만 규칙 목록에 쓴다. 문서 값은 게이트가
//! 상태를 바꾸는 지점(탐색 결정·commit·종료)마다 다시 읽는다. entry mirror는 늘 막고, HTTP view는
//! endpoint가 정하므로 이 규칙을 쓰지 않는다. 규칙 컴파일 전·실패 시에는 막지 못한다.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKWebView};

use super::super::script_gate::ScriptGate;
use super::content_rules::apply_block_state;

/// http(s) content-blocker 규칙. 컴파일·설치 전 요청은 이 규칙으로 막지 못한다.
const REMOTE_BLOCK_RULE_JSON: &str =
    r#"[{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}]"#;

/// 한 webview의 원격 차단 상태. delegate와 PlatformWebView가 복제해 쓴다.
#[derive(Clone)]
pub(super) struct RemoteBlock {
    webview: Retained<WKWebView>,
    /// HTTP view는 endpoint가 정하므로 이 규칙을 쓰지 않는다.
    http: bool,
    entry: bool,
    /// 컴파일 완료 전에는 차단 규칙이 없다.
    rules: Rc<RefCell<Option<Retained<WKContentRuleList>>>>,
    block: Rc<Cell<bool>>,
    global_allowed: Rc<Cell<bool>>,
    document_allowed: Rc<Cell<bool>>,
}

impl RemoteBlock {
    /// 규칙을 비동기로 컴파일한다. 완료 콜백이 그때의 차단 상태로 설치한다.
    pub(super) fn install(
        webview: Retained<WKWebView>,
        entry: bool,
        http: bool,
        mtm: MainThreadMarker,
    ) -> Self {
        let this = Self {
            webview,
            http,
            entry,
            rules: Rc::new(RefCell::new(None)),
            block: Rc::new(Cell::new(true)),
            global_allowed: Rc::new(Cell::new(false)),
            document_allowed: Rc::new(Cell::new(false)),
        };
        if http {
            return this;
        }
        // SAFETY: main thread(mtm)에서 기본 저장소를 얻는다.
        let Some(store) = (unsafe { WKContentRuleListStore::defaultStore(mtm) }) else {
            tracing::warn!("WKContentRuleListStore::defaultStore 없음 — 원격 차단 비활성");
            return this;
        };
        let cb = this.clone();
        let handler = RcBlock::new(move |list: *mut WKContentRuleList, err: *mut NSError| {
            // SAFETY: WebKit 이 main thread 에서 컴파일 완료를 호출하며 list/err 는 이 시점 valid.
            // list non-null 이면 +0 참조라 보관 위해 retain 한다.
            if let Some(retained) = unsafe { Retained::retain(list) } {
                tracing::debug!("WKContentRuleList 컴파일 성공 — 원격 차단 룰 설치");
                *cb.rules.borrow_mut() = Some(retained);
                cb.apply_rules();
            // SAFETY: err 는 완료 콜백 동안 valid 한 NSError 이거나 null 이다.
            } else if let Some(err) = unsafe { err.as_ref() } {
                tracing::warn!(
                    "WKContentRuleList compile 실패: {}",
                    err.localizedDescription()
                );
            }
        });
        let id = NSString::from_str("tasty-block-remote");
        let json = NSString::from_str(REMOTE_BLOCK_RULE_JSON);
        // SAFETY: main thread WebKit API. 문자열과 handler 는 호출 동안 살아 있다.
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&id),
                Some(&json),
                Some(&handler),
            );
        }
        this
    }

    /// 전역 "Allow remote content"를 바꾼다.
    pub(super) fn set_global(&self, allowed: bool) {
        self.global_allowed.set(allowed);
        self.apply();
    }

    /// 게이트의 문서 단위 값을 다시 읽어 적용한다. 게이트 상태가 바뀐 지점마다 부른다.
    pub(super) fn sync(&self, gate: &ScriptGate) {
        self.document_allowed.set(gate.document_remote());
        self.apply();
    }

    fn apply(&self) {
        if self.http {
            return;
        }
        let block = self.entry || !(self.global_allowed.get() || self.document_allowed.get());
        if self.block.replace(block) != block {
            self.apply_rules();
        }
    }

    fn apply_rules(&self) {
        apply_block_state(
            &self.webview,
            self.block.get(),
            self.rules.borrow().as_deref(),
        );
    }
}
