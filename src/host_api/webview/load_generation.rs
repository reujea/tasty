//! 로드 종료 신호가 스크립트 게이트가 시작한 마지막 로드의 것인지 판정한다(docs/surfaces/html/index.md).
//! 앞 로드의 늦은 종료가 새 로드의 대기 값을 지우지 않게 한다. WebView2는 `NavigationId`, macOS는 `WKNavigation`을 세대로 쓴다.
//! chrome의 `NavState`도 같은 판정으로 마지막에 시작한 main frame 탐색의 종료만 따른다.
//! WebKitGTK 종료 신호에는 navigation 식별자가 없어 Linux는 신호 순서로 세대를 매긴다(`SignalOrderLoads`).

use super::NavState;

/// 종료 신호가 게이트가 시작한 마지막 로드의 것인지. ID를 알 수 없는 쪽이 있으면 현재 로드로 본다.
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
pub(super) fn is_current_load(current: Option<u64>, ended: Option<u64>) -> bool {
    match (current, ended) {
        (Some(c), Some(e)) => c == e,
        _ => true,
    }
}

/// 로드 종료 뒤 chrome 상태. `started`는 마지막에 시작한 main frame 탐색의 세대다.
/// 앞 로드가 늦게 끝나면 새 로드의 Loading을 Done이나 Failed로 덮지 않도록 None을 돌려준다.
#[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
pub(super) fn nav_state_after_end(
    started: Option<u64>,
    ended: Option<u64>,
    success: bool,
) -> Option<NavState> {
    if !is_current_load(started, ended) {
        return None;
    }
    Some(if success {
        NavState::Done
    } else {
        NavState::Failed
    })
}

/// 식별자 없는 종료 신호를 받는 백엔드(WebKitGTK)의 chrome 세대.
/// 종료 신호는 마지막 `STARTED`를 받은 로드(진행 중 로드)의 것으로 본다. `load_url`·`load_html`은 새 세대를
/// 현재 로드로 두므로, 그 뒤에 오는 진행 중 로드의 취소 실패와 `FINISHED`는 앞 로드의 늦은 종료가 된다.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Default)]
pub(super) struct SignalOrderLoads {
    next: u64,
    /// 마지막에 요청했거나 시작한 로드.
    current: Option<u64>,
    /// 요청한 로드가 아직 `STARTED`를 받지 않았다.
    requested: bool,
    /// `STARTED`를 받고 `FINISHED`를 받지 않은 로드. 종료 신호를 이 로드에 돌린다.
    in_flight: Option<u64>,
}

#[cfg(any(target_os = "linux", test))]
impl SignalOrderLoads {
    fn fresh(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// `load_url`·`load_html`. 시작 신호 전부터 이 로드가 현재 로드다.
    pub(super) fn requested(&mut self) {
        self.current = Some(self.fresh());
        self.requested = true;
    }

    /// main frame `STARTED`. 요청한 로드가 있으면 그 로드이고, 없으면(링크·뒤로 가기) 새 로드다.
    pub(super) fn started(&mut self) {
        if !std::mem::take(&mut self.requested) {
            self.current = Some(self.fresh());
        }
        self.in_flight = self.current;
    }

    /// `load-failed`. 뒤따르는 `FINISHED`도 같은 로드의 것이라 진행 중 로드를 그대로 둔다.
    pub(super) fn failed(&self) -> Option<NavState> {
        nav_state_after_end(self.current, self.in_flight, false)
    }

    /// `FINISHED`. 진행 중 로드를 비운다.
    pub(super) fn finished(&mut self) -> Option<NavState> {
        let state = nav_state_after_end(self.current, self.in_flight, true);
        self.in_flight = None;
        state
    }

    /// web process 종료. `FINISHED`가 오지 않으므로 진행 중 로드를 비운다. chrome은 세대와 관계없이 Failed다.
    pub(super) fn terminated(&mut self) {
        self.in_flight = None;
    }
}

/// macOS에서 Tasty가 Cancel로 끝낸 정책 결정의 실패 신호를 가려낸다.
///
/// WebKit은 정책 Cancel 뒤 그 navigation에 `didFailProvisionalNavigation` 계열을 부를 수 있다. 다만
/// 1:1로 부른다는 보장은 없다. 새 로드의 정책 결정부터 첫 provisional 시작까지는 게이트 세대가 비어
/// (모름 = 현재) 이 늦은 실패가 새 로드의 것으로 보이고, 새 로드가 보류한 결정까지 Cancel된다.
/// 반대로 새 로드의 진짜 실패(정책 단계 실패는 provisional 시작 없이 온다)를 삼키면 로드가 끝나지 않는다.
///
/// 정책 결정 콜백은 navigation을 알려 주지 않는다. 호스트 API가 돌려준 `WKNavigation`을 종류가 맞는
/// 결정에 붙이면, 같은 종류의 페이지 탐색 결정이 먼저 와서 그 navigation을 가져갈 수 있다. 그래서
/// navigation으로 귀속하지 않고 오류로 가린다. 세대가 빈 구간에서 정책 변경으로 중단됐다는 오류
/// (`WebKitErrorDomain` 102)만 Cancel한 수만큼 흡수한다. Tasty는 main frame 탐색 정책을 이 두 Cancel
/// 경로로만 바꾸고 응답 정책 delegate가 없으므로, 이 구간의 102는 Tasty가 Cancel한 결정에서 나온다.
/// 다른 오류는 흡수하지 않는다. provisional 시작 뒤에는 세대 판정이 앞 결정의 실패를 무시하므로 수를 비운다.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Default)]
pub(super) struct CancelledDecisions {
    expected: u32,
}

#[cfg(any(target_os = "macos", test))]
impl CancelledDecisions {
    /// 보류한 결정을 Cancel로 끝냈다.
    pub(super) fn cancelled(&mut self) {
        self.expected = self.expected.saturating_add(1);
    }

    /// main frame provisional 시작. 이후 세대가 생기므로 수를 비운다.
    pub(super) fn started(&mut self) {
        self.expected = 0;
    }

    /// 실패가 Tasty가 Cancel한 결정의 것이면 `true`이며 수를 하나 소비한다.
    /// `interrupted_by_policy`는 오류가 `WebKitErrorDomain` 102인지, `current_gate`는 게이트 세대다.
    pub(super) fn absorbs_failure(
        &mut self,
        interrupted_by_policy: bool,
        current_gate: Option<u64>,
    ) -> bool {
        if current_gate.is_some() || self.expected == 0 || !interrupted_by_policy {
            return false;
        }
        self.expected -= 1;
        true
    }
}

/// 거절된 호스트 로드의 chrome 세대. WebView2 `NavigationId`는 view마다 1부터 하나씩 늘어 이 값에 닿지
/// 않으므로, 어떤 탐색의 종료도 이 로드의 것이 되지 않는다.
#[cfg(any(windows, test))]
const REJECTED_LOAD: u64 = u64::MAX;

/// 호스트가 보낸 로드 호출(WebView2 `Navigate`·`NavigateToString`)이 거절됐다. 탐색이 시작되지 않아
/// 종료 신호가 오지 않으므로 chrome을 여기서 Failed로 끝낸다. `latest`는 마지막에 요청한 호스트 로드의
/// 번호이고 `rejected`는 거절된 호출의 번호다. 붙잡혔다가 늦게 보낸 앞 요청의 거절은 그 뒤에 요청한
/// 로드의 상태를 덮지 않는다. 거절 전까지 새 main 탐색이 시작하지 않았으면(세대가 `begin_load`가 비운
/// None 그대로면) 거절된 로드가 자기 세대를 현재로 남겨(`chrome_generation`), 그보다 앞서 시작한 탐색의
/// 늦은 종료가 오류 타일을 지우지 못하게 한다. 거절 전에 시작한 탐색이 세대를 세웠으면 그 세대를 두어 그
/// 탐색의 종료를 따른다. chrome을 바꿨으면 true다.
#[cfg(any(windows, test))]
pub(super) fn end_rejected_load(
    nav_state: &std::cell::Cell<NavState>,
    chrome_generation: &std::cell::Cell<Option<u64>>,
    latest: u64,
    rejected: u64,
) -> bool {
    if latest != rejected {
        return false;
    }
    nav_state.set(NavState::Failed);
    if chrome_generation.get().is_none() {
        chrome_generation.set(Some(REJECTED_LOAD));
    }
    true
}

/// WebView2 `NavigationCompleted`의 chrome 상태. 완료의 `NavigationId`를 읽지 못했으면(None) 지금 로드의
/// 종료로 보지 않고 상태를 바꾸지 않는다(닫힌 쪽). 나머지는 `nav_state_after_end`와 같다.
#[cfg(any(windows, test))]
pub(super) fn chrome_state_after_end(
    started: Option<u64>,
    ended: Option<u64>,
    success: bool,
) -> Option<NavState> {
    ended?;
    nav_state_after_end(started, ended, success)
}

/// 시작 처리기가 host 전용 주소의 탐색을 취소했다(`host_document`). 취소한 탐색이 chrome의 지금 로드인지는
/// 아래로 가린다.
///
/// - 세대가 빈 구간(`begin_load` 뒤 요청의 시작 전): 리다이렉트가 아니고 지금 호스트 로드가 보낸
///   탐색(`host_sent`)일 때만 지금 로드다. 탐색 ID를 읽지 못했어도 그렇다. 그 로드가 기다리는 탐색을 이
///   취소로 잃었으므로 Loading에 두지 않는다. 이 구간의 리다이렉트는 새 요청보다 앞선 탐색의 것이다.
/// - 세대가 있을 때: 그 탐색의 ID가 세대인 리다이렉트만 지금 로드다. 그 뒤의 새 시작은 보이던 문서가
///   시작한 탐색이다. 탐색 ID를 모르면 지금 로드로 보지 않는다(닫힌 쪽).
///
/// 지금 로드면 시작 처리기가 하던 원격 차단 지목 소비를 하고(리다이렉트가 아닐 때), 열린 로드를 닫고
/// Failed로 끝내며 그 ID를 세대로 둔다. ID를 모르면 거절된 로드의 세대(`REJECTED_LOAD`)를 둔다. 그 완료는 보통의 실패로 처리하면 된다. 지금 로드가 아니면 chrome·
/// 세대·지목을 그대로 두고 true를 돌려준다. 호출자는 그 완료가 chrome을 바꾸지 않게 건너뛴다.
#[cfg(any(windows, test))]
pub(super) fn end_cancelled_navigation(
    nav_state: &std::cell::Cell<NavState>,
    chrome_generation: &std::cell::Cell<Option<u64>>,
    top_level: &std::cell::RefCell<tasty_model::html_script::top_level::TopLevelAdmission>,
    nav: Option<u64>,
    redirected: bool,
    host_sent: bool,
) -> bool {
    let current = match (chrome_generation.get(), nav) {
        (None, _) => !redirected && host_sent,
        (Some(generation), Some(id)) => redirected && generation == id,
        (Some(_), None) => false,
    };
    if !current {
        return true;
    }
    let mut top = top_level.borrow_mut();
    if !redirected {
        top.drop_next();
    }
    top.end();
    nav_state.set(NavState::Failed);
    chrome_generation.set(Some(nav.unwrap_or(REJECTED_LOAD)));
    false
}

#[cfg(test)]
mod tests {
    use super::{
        CancelledDecisions, NavState, REJECTED_LOAD, SignalOrderLoads, chrome_state_after_end,
        end_cancelled_navigation, end_rejected_load, is_current_load, nav_state_after_end,
    };

    /// 세대가 빈 구간의 정책 중단 오류만 Cancel한 수만큼 흡수한다. 다른 오류는 수를 남긴 채 두고,
    /// provisional 시작 뒤에는 흡수하지 않는다.
    #[test]
    fn only_policy_interruptions_are_absorbed_for_cancelled_decisions() {
        let mut cancelled = CancelledDecisions::default();
        assert!(
            !cancelled.absorbs_failure(true, None),
            "Cancel한 결정이 없다"
        );
        cancelled.cancelled();
        assert!(
            !cancelled.absorbs_failure(false, None),
            "정책 중단이 아닌 실패"
        );
        assert!(
            !cancelled.absorbs_failure(true, Some(9)),
            "세대가 있으면 세대 판정"
        );
        assert!(cancelled.absorbs_failure(true, None));
        assert!(!cancelled.absorbs_failure(true, None), "수만큼만");
        cancelled.cancelled();
        cancelled.started();
        assert!(
            !cancelled.absorbs_failure(true, None),
            "provisional 시작 뒤에는 잊는다"
        );
    }

    /// WebKitGTK 2.50.4에서 측정한 순서: 진행 중 로드를 `load_uri`로 바꾸면 앞 로드의 취소 실패와
    /// `FINISHED`가 새 로드의 `STARTED`보다 먼저 온다.
    #[test]
    fn a_load_cancelled_by_a_new_request_does_not_fail_the_chrome() {
        let mut loads = SignalOrderLoads::default();
        loads.requested();
        loads.started();
        loads.requested();
        assert_eq!(loads.failed(), None);
        assert_eq!(loads.finished(), None);
        loads.started();
        assert_eq!(loads.finished(), Some(NavState::Done));
    }

    #[test]
    fn the_current_load_failing_fails_the_chrome() {
        let mut loads = SignalOrderLoads::default();
        loads.requested();
        loads.started();
        assert_eq!(loads.failed(), Some(NavState::Failed));
        // 실패 뒤 FINISHED의 Done은 호출자가 Failed를 덮지 않게 거른다.
        assert_eq!(loads.finished(), Some(NavState::Done));
    }

    #[test]
    fn a_navigation_without_a_request_is_its_own_load() {
        let mut loads = SignalOrderLoads::default();
        loads.started();
        assert_eq!(loads.failed(), Some(NavState::Failed));
        loads.finished();
        loads.started();
        assert_eq!(loads.finished(), Some(NavState::Done));
    }

    #[test]
    fn a_request_failing_before_its_start_fails_the_chrome() {
        let mut loads = SignalOrderLoads::default();
        loads.requested();
        assert_eq!(loads.failed(), Some(NavState::Failed));
        let mut after_finish = SignalOrderLoads::default();
        after_finish.requested();
        after_finish.started();
        after_finish.finished();
        after_finish.requested();
        assert_eq!(after_finish.failed(), Some(NavState::Failed));
    }

    #[test]
    fn a_terminated_web_process_leaves_no_load_in_flight() {
        let mut loads = SignalOrderLoads::default();
        loads.requested();
        loads.started();
        loads.terminated();
        loads.requested();
        assert_eq!(loads.failed(), Some(NavState::Failed));
    }

    #[test]
    fn the_end_of_the_load_the_gate_started_is_current() {
        assert!(is_current_load(Some(7), Some(7)));
    }

    #[test]
    fn the_late_end_of_a_superseded_load_is_not_current() {
        assert!(!is_current_load(Some(8), Some(7)));
    }

    #[test]
    fn an_unknown_id_on_either_side_falls_back_to_the_current_load() {
        assert!(is_current_load(Some(8), None));
        assert!(is_current_load(None, Some(7)));
        assert!(is_current_load(None, None));
    }

    #[test]
    fn the_end_of_the_last_started_load_sets_the_chrome_state() {
        assert_eq!(
            nav_state_after_end(Some(7), Some(7), true),
            Some(NavState::Done)
        );
        assert_eq!(
            nav_state_after_end(Some(7), Some(7), false),
            Some(NavState::Failed)
        );
    }

    #[test]
    fn a_superseded_load_ending_late_keeps_the_new_load_state() {
        assert_eq!(nav_state_after_end(Some(8), Some(7), true), None);
        assert_eq!(nav_state_after_end(Some(8), Some(7), false), None);
    }

    #[test]
    fn an_unknown_id_ends_as_the_current_load() {
        assert_eq!(
            nav_state_after_end(None, Some(7), true),
            Some(NavState::Done)
        );
        assert_eq!(
            nav_state_after_end(Some(8), None, false),
            Some(NavState::Failed)
        );
    }

    #[test]
    fn an_end_after_a_load_request_without_a_start_signal_is_applied() {
        // load_url·load_html이 chrome 세대를 비운 뒤 시작 신호 없이 끝나는 로드.
        assert_eq!(
            nav_state_after_end(None, Some(9), false),
            Some(NavState::Failed)
        );
        assert_eq!(
            nav_state_after_end(None, Some(9), true),
            Some(NavState::Done)
        );
        assert_eq!(
            nav_state_after_end(None, None, false),
            Some(NavState::Failed)
        );
    }

    #[test]
    fn a_rejected_load_call_fails_the_chrome() {
        // 호출이 거절되면 시작·종료 신호가 오지 않아 Loading이 남는다(mermaid를 넣은 2MB 넘는 문서).
        let nav = std::cell::Cell::new(NavState::Loading);
        let generation = std::cell::Cell::new(None);
        assert!(end_rejected_load(&nav, &generation, 3, 3));
        assert_eq!(nav.get(), NavState::Failed);
    }

    #[test]
    fn a_late_rejection_of_an_earlier_request_keeps_the_newer_load() {
        let nav = std::cell::Cell::new(NavState::Loading);
        let generation = std::cell::Cell::new(None);
        assert!(!end_rejected_load(&nav, &generation, 4, 3));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None, "새 로드의 세대를 건드리지 않는다");
    }

    #[test]
    fn an_earlier_navigation_ending_after_a_rejection_keeps_the_failure() {
        // begin_load가 세대를 비운(모름 = 현재) 뒤 호출이 거절되고, 앞서 시작한 탐색 7이 성공으로 끝난다.
        let nav = std::cell::Cell::new(NavState::Loading);
        let generation = std::cell::Cell::new(None);
        assert!(end_rejected_load(&nav, &generation, 3, 3));
        assert_eq!(nav_state_after_end(generation.get(), Some(7), true), None);
        assert_eq!(nav_state_after_end(generation.get(), Some(7), false), None);
        assert_eq!(nav.get(), NavState::Failed);
    }

    use std::cell::{Cell, RefCell};
    use tasty_model::html_script::top_level::{Designation, Origin, StartKind, TopLevelAdmission};

    const NEXT_PAGE: &str = "https://example.com/next";

    #[test]
    fn a_navigation_started_before_the_rejection_follows_its_own_end() {
        // begin_load가 세대를 비운 뒤, 거절보다 먼저 페이지가 탐색 8을 시작했다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(Some(8));
        assert!(end_rejected_load(&nav, &generation, 3, 3));
        assert_eq!(nav.get(), NavState::Failed);
        assert_eq!(
            generation.get(),
            Some(8),
            "시작한 탐색의 세대를 지우지 않는다"
        );
        assert_eq!(
            chrome_state_after_end(generation.get(), Some(8), true),
            Some(NavState::Done)
        );
    }

    #[test]
    fn an_end_without_an_id_after_a_rejection_keeps_the_failure() {
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        assert!(end_rejected_load(&nav, &generation, 3, 3));
        assert_eq!(chrome_state_after_end(generation.get(), None, true), None);
        assert_eq!(chrome_state_after_end(None, None, true), None);
        assert_eq!(
            chrome_state_after_end(Some(7), Some(7), false),
            Some(NavState::Failed)
        );
    }

    /// 지목이 남았는지. 남았으면 원격 차단 중에도 다음 탐색이 열린다.
    fn next_start_admitted(top: &RefCell<TopLevelAdmission>) -> bool {
        top.borrow_mut()
            .start(NEXT_PAGE, StartKind::Other, false, Origin::default())
    }

    #[test]
    fn a_cancelled_host_navigation_ends_its_load_and_its_designation() {
        // 호스트 로드가 Loading·빈 세대·지목을 세운 뒤, 지금 로드 번호 머리글을 단 그 탐색이 받히지 않고 취소된다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().designate(Designation::Next);
        assert!(!end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(5),
            false,
            true
        ));
        assert_eq!(nav.get(), NavState::Failed);
        assert_eq!(generation.get(), Some(5));
        assert!(
            !next_start_admitted(&top),
            "지목이 다음 탐색으로 이어지지 않는다"
        );
        // 앞서 시작한 탐색 4의 늦은 성공도 Failed를 덮지 않는다.
        assert_eq!(nav_state_after_end(generation.get(), Some(4), true), None);
    }

    #[test]
    fn a_redirect_into_an_unknown_reserved_address_fails_the_load() {
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(Some(6));
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().start(
            NEXT_PAGE,
            StartKind::Other,
            false,
            Origin {
                remote_allowed: true,
                remote_document: false,
            },
        );
        assert!(top.borrow().admitting());
        assert!(!end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(6),
            true,
            false
        ));
        assert_eq!(nav.get(), NavState::Failed);
        assert!(!top.borrow().admitting(), "열린 로드를 닫는다");
    }

    #[test]
    fn a_page_navigation_to_an_unknown_reserved_address_keeps_the_shown_document() {
        let nav = Cell::new(NavState::Done);
        let generation = Cell::new(Some(6));
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().designate(Designation::Next);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(7),
            false,
            false
        ));
        assert_eq!(nav.get(), NavState::Done);
        assert_eq!(generation.get(), Some(6));
        assert!(
            next_start_admitted(&top),
            "그 로드의 탐색이 아니면 지목을 남긴다"
        );
    }

    #[test]
    fn a_cancelled_redirect_of_an_earlier_navigation_keeps_the_requested_load() {
        // begin_load가 세대를 비우고 지목을 세운 뒤, 요청의 시작 전에 앞 탐색 4의 리다이렉트가 취소된다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().designate(Designation::Next);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(4),
            true,
            false
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
        // 같은 구간의, 지금 호스트 로드가 보내지 않은 새 탐색도 요청을 끝내지 않는다.
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(5),
            false,
            false
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
        assert!(next_start_admitted(&top), "요청의 지목은 남는다");
    }

    #[test]
    fn a_cancelled_history_start_with_an_earlier_load_number_keeps_the_new_load() {
        use super::super::host_document::{HostLoadHeader, sent_for_current_load};
        // 로드 3이 요청돼 세대가 빈 구간에, 로드 2의 머리글을 다시 보낸 이력 이동이 취소된다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().designate(Designation::Next);
        let host_sent = sent_for_current_load(HostLoadHeader::Load(2), 3, false);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(9),
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
        assert!(next_start_admitted(&top), "새 로드의 지목은 남는다");
        assert!(sent_for_current_load(HostLoadHeader::Load(3), 3, false));
        assert!(!sent_for_current_load(HostLoadHeader::Absent, 3, true));
    }

    #[test]
    fn a_cancelled_page_navigation_to_the_retry_address_keeps_the_pending_load() {
        use super::super::host_document::{HostLoadHeader, sent_for_current_load};
        // 로드 3의 다시 싣기가 장벽을 기다리는 동안, 페이지가 같은 주소로 가는 탐색(머리글 없음)이 취소된다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        top.borrow_mut().designate(Designation::Next);
        let host_sent = sent_for_current_load(HostLoadHeader::Absent, 3, true);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(9),
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
        assert!(next_start_admitted(&top), "기다리는 요청의 지목은 남는다");
    }

    #[test]
    fn an_unreadable_header_on_the_awaited_retry_fails_the_load() {
        use super::super::host_document::{HostLoadHeader, sent_for_current_load};
        // 로드 3의 다시 싣기를 보냈고 아직 시작이 없다. 그 시작의 머리글 조회가 실패해 취소된다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        let host_sent = sent_for_current_load(HostLoadHeader::Unreadable, 3, true);
        assert!(!end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(9),
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Failed);
        assert_eq!(generation.get(), Some(9));
        // 머리글이 없는 시작(페이지 탐색)은 기다리는 동안에도 로드를 끝내지 않는다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let host_sent = sent_for_current_load(HostLoadHeader::Absent, 3, true);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            Some(10),
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
    }

    #[test]
    fn a_cancelled_navigation_without_an_id_is_current_only_with_the_current_load_number() {
        use super::super::host_document::{HostLoadHeader, sent_for_current_load};
        // 로드 3의 다시 싣기가 시작했으나 NavigationId를 읽지 못해 취소된다. 머리글 번호가 지금 로드다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let top = RefCell::new(TopLevelAdmission::default());
        let host_sent = sent_for_current_load(HostLoadHeader::Load(3), 3, false);
        assert!(!end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            None,
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Failed);
        assert_eq!(generation.get(), Some(REJECTED_LOAD));
        // 그 탐색의 늦은 종료(ID가 있든 없든)는 Failed를 바꾸지 않는다.
        assert_eq!(
            chrome_state_after_end(generation.get(), Some(4), true),
            None
        );
        assert_eq!(chrome_state_after_end(generation.get(), None, true), None);

        // 번호가 지금 로드가 아니면 지금 로드로 보지 않는다.
        let nav = Cell::new(NavState::Loading);
        let generation = Cell::new(None);
        let host_sent = sent_for_current_load(HostLoadHeader::Load(2), 3, false);
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            None,
            false,
            host_sent
        ));
        assert_eq!(nav.get(), NavState::Loading);
        assert_eq!(generation.get(), None);
        // 세대가 있는 동안 ID를 모르는 취소도 지금 로드가 아니다.
        let generation = Cell::new(Some(6));
        assert!(end_cancelled_navigation(
            &nav,
            &generation,
            &top,
            None,
            true,
            true
        ));
        assert_eq!(generation.get(), Some(6));
    }
}
