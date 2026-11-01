//! Navigation delegate state: deferred policy decisions and load generations.
use std::ptr::NonNull;
use std::rc::Weak;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_foundation::NSError;
use objc2_web_kit::{WKNavigation, WKNavigationActionPolicy, WKWebpagePreferences};

use super::NavDelegateIvars;
use super::http;
use crate::webview::load_generation::{is_current_load, nav_state_after_end};
use crate::webview::script_gate::{DeferredScan, ReadSlot};

/// WebKit의 정책 결정 handler. 보류한 결정을 콜백 밖에서 끝내려고 복사해 둔다.
pub(super) type DecisionHandler =
    RcBlock<dyn Fn(WKNavigationActionPolicy, NonNull<WKWebpagePreferences>)>;

/// 스캔 결과를 기다리며 보류한 main frame 정책 결정. 메인 스레드에만 있고 작업 스레드에는
/// 경로와 표만 넘어간다. handler는 Allow나 Cancel로 정확히 한 번 부른다.
pub(super) struct PendingDecision {
    pub(super) scan: DeferredScan,
    pub(super) read: ReadSlot,
    pub(super) handler: DecisionHandler,
    pub(super) preferences: Retained<WKWebpagePreferences>,
}

/// 보류한 결정을 끝낸다. 결정마다 정확히 한 번 부른다.
pub(super) fn complete_decision(
    handler: &DecisionHandler,
    preferences: &WKWebpagePreferences,
    policy: WKNavigationActionPolicy,
) {
    handler.call((policy, NonNull::from(preferences)));
}

/// 세대 비교용 WKNavigation 식별 값. 알 수 없으면 None이다.
pub(super) fn navigation_key(navigation: Option<&WKNavigation>) -> Option<u64> {
    navigation.map(|n| std::ptr::from_ref(n) as usize as u64)
}

impl NavDelegateIvars {
    /// 신호가 게이트가 시작한 마지막 로드의 것인지. 어느 쪽이든 WKNavigation을 알 수 없으면
    /// 현재 로드로 본다(Windows와 같은 규칙).
    pub(super) fn is_current_navigation(&self, navigation: Option<&WKNavigation>) -> bool {
        let current = navigation_key(self.gate_navigation.borrow().as_deref());
        is_current_load(current, navigation_key(navigation))
    }

    /// 보류한 결정을 Cancel로 끝낸다. 새 탐색·로드 요청·webview 종료가 앞 결정을 무효화할 때 부른다.
    pub(super) fn cancel_pending_decision(&self) {
        let Some(pending) = self.pending_decision.borrow_mut().take() else {
            return;
        };
        tracing::debug!(
            "WebView surface {}: cancelling the navigation that waited for its scan",
            self.surface_id
        );
        pending.scan.abandon();
        self.cancelled_decisions.borrow_mut().cancelled();
        complete_decision(
            &pending.handler,
            &pending.preferences,
            WKNavigationActionPolicy::Cancel,
        );
    }

    /// 실패 신호가 Tasty가 Cancel한 결정의 것이면 소비하고 `true`를 준다. 그 신호는 chrome과
    /// 게이트를 건드리지 않는다. 귀속 규칙은 `load_generation::CancelledDecisions`에 있다.
    pub(super) fn absorbs_cancelled_failure(
        &self,
        navigation: Option<&WKNavigation>,
        error: &NSError,
    ) -> bool {
        // WebKitErrorFrameLoadInterruptedByPolicyChange(102). 상수 이름의 기능 플래그 없이 값으로 비교한다.
        let interrupted_by_policy =
            error.domain().to_string().as_str() == "WebKitErrorDomain" && error.code() == 102;
        let current = navigation_key(self.gate_navigation.borrow().as_deref());
        if !self
            .cancelled_decisions
            .borrow_mut()
            .absorbs_failure(interrupted_by_policy, current)
        {
            return false;
        }
        tracing::debug!(
            "WebView surface {}: ignoring the failure of a decision Tasty cancelled ({:?})",
            self.surface_id,
            navigation_key(navigation)
        );
        true
    }

    /// 읽기가 끝났으면 보류한 결정을 끝낸다. 표가 아직 현재 로드의 것일 때만 JS를 적용해 Allow하고,
    /// 낡았으면 Cancel한다. 읽는 중이면 아무것도 하지 않는다.
    pub(super) fn resume_pending_decision(&self) {
        let read = match self.pending_decision.borrow().as_ref() {
            Some(pending) => pending.read.take(),
            None => return,
        };
        let Some(read) = read else {
            return;
        };
        let Some(pending) = self.pending_decision.borrow_mut().take() else {
            return;
        };
        // 표 확인과 상태 반영은 결정을 넘기기 직전 이 메인 스레드 호출 안에서 한다.
        let gate = pending.scan.gate().clone();
        let policy = match pending.scan.apply(read) {
            Some(js) => {
                // SAFETY: main thread. preferences는 이 결정이 붙잡아 둔 Retained 값이다.
                unsafe { pending.preferences.setAllowsContentJavaScript(js) };
                self.remote.sync(&gate);
                WKNavigationActionPolicy::Allow
            }
            None => {
                self.cancelled_decisions.borrow_mut().cancelled();
                WKNavigationActionPolicy::Cancel
            }
        };
        complete_decision(&pending.handler, &pending.preferences, policy);
    }

    /// 종료 신호가 게이트가 시작한 마지막 로드의 것일 때만 [`Self::gate_finished`]를 부른다.
    pub(super) fn gate_navigation_ended(&self, navigation: Option<&WKNavigation>) {
        if self.is_current_navigation(navigation) {
            self.gate_finished();
        } else {
            tracing::debug!(
                "WebView surface {}: ignoring the end of a superseded load",
                self.surface_id
            );
        }
    }

    /// 로드 종료를 chrome 상태에 반영한다. 마지막에 시작한 navigation의 종료일 때만 바꾸고,
    /// 앞 로드의 늦은 종료는 새 로드의 Loading을 덮지 않는다.
    pub(super) fn chrome_navigation_ended(&self, navigation: Option<&WKNavigation>, success: bool) {
        let started = navigation_key(self.chrome_navigation.borrow().as_deref());
        match nav_state_after_end(started, navigation_key(navigation), success) {
            Some(state) => self.nav_state.set(state),
            None => tracing::debug!(
                "WebView surface {}: keeping the chrome state of the newer load",
                self.surface_id
            ),
        }
    }

    /// 로드가 실패했다. 게이트가 시작한 마지막 로드의 실패일 때만 다음 commit까지 배너와 탭
    /// 표지를 내린 뒤 로드를 끝낸다. 앞 로드가 늦게 취소된 실패는 새 로드의 게이트를 건드리지 않는다.
    pub(super) fn gate_navigation_failed(&self, navigation: Option<&WKNavigation>) {
        if self.is_current_navigation(navigation) {
            self.gate_failed();
        }
        self.gate_navigation_ended(navigation);
    }

    /// The policy interruption of a navigation that continues on its loopback alias.
    pub(super) fn absorbs_alias_cancel(&self, error: &NSError) -> bool {
        error.domain().to_string().as_str() == "WebKitErrorDomain"
            && error.code() == 102
            && self.alias_cancelled.replace(false)
    }

    /// The current load of a remote HTTP document failed.
    pub(super) fn http_load_failed(&self, navigation: Option<&WKNavigation>) {
        if self.is_current_navigation(navigation)
            && let Some(document) = self.http.as_ref().and_then(Weak::upgrade)
        {
            http::load_failed(&document.session());
        }
    }

    /// 로드가 실패했다. 다음 commit까지 배너와 탭 표지를 내린다.
    pub(super) fn gate_failed(&self) {
        if let Some(gate) = self.script_gate.borrow().as_ref() {
            gate.failed();
            self.remote.sync(gate);
        }
    }

    /// 로드가 commit 없이 끝났을 때 게이트 상태를 되돌린다. 화면 문서의 JS는 탐색 단위라 그대로다.
    /// provisional 시작 없이 끝난 로드의 대기 표시도 내려, 뒤에 오는 무관한 provisional 시작을
    /// 게이트 로드의 세대로 기록하지 않게 한다.
    pub(super) fn gate_finished(&self) {
        self.gate_load_pending.set(false);
        if let Some(js) = self
            .script_gate
            .borrow()
            .as_ref()
            .and_then(|g| g.finished())
        {
            tracing::debug!(
                "WebView surface {}: the load ended without a commit; the next navigation decides js (now {js})",
                self.surface_id
            );
        }
        if let Some(gate) = self.script_gate.borrow().as_ref() {
            self.remote.sync(gate);
        }
    }
}
