//! 숨김 보류 중인 native webview의 탐색이 끝나면 그 창을 다시 그린다.
//!
//! OS의 로드 완료·실패 콜백은 native 탐색 상태만 바꾸고 다시 그리기를 요청하지 않는다. 드러날
//! 자리에 있지만 아직 숨긴 webview는 다음 동기화에서만 드러나므로, 입력이나 다른 프레임이 없으면
//! 숨긴 자리가 그대로 남는다. 로드 중 스피너는 reduced motion에서 연속 다시 그리기를 요청하지 않으므로
//! 완료를 화면에 반영하는 일을 스피너에 맡길 수 없다.
//!
//! 깨움은 변화 한 번에 한 번이다. 깨울 때 보류 기록을 지금 상태로 바꾸므로, 다시 그리기 요청을 받은 창이
//! 동기화 전에 돌아가(journal 관측 정지, workspace 없음 등) 기록이 그대로여도 같은 변화로 다시 깨우지 않는다.

use std::collections::HashMap;
use std::time::Instant;

use crate::app::App;
use crate::view::ui::View;
use crate::webview::NavState;

/// surface별 숨김 보류 기록: 시작 시각, 경고 여부, 마지막으로 본 탐색 상태(`MainView::webview_reveal_pending`).
pub(crate) type RevealPending = HashMap<u32, (Instant, bool, NavState)>;

impl App {
    /// native 이벤트를 처리한 회차 끝에서 부른다. 숨김 보류로 기록한 뒤 탐색 상태가 바뀐 webview가
    /// 있는 창을 다시 그려, 다음 동기화가 Done이면 페이지를 드러내고 Failed이면 오류 화면을 그리게 한다.
    pub(crate) fn redraw_settled_webview_reveals(&mut self) {
        for w in self.view.views.values_mut() {
            let Some(main) = w.as_main_mut() else {
                continue;
            };
            if wake_window(&mut main.webview_reveal_pending, &main.webviews) {
                main.mark_dirty();
            }
        }
    }
}

/// native 탐색 상태를 읽을 수 있는 webview. 창의 webview 맵을 시험에서 대신할 수 있게 한다.
trait NavSource {
    fn nav_state(&self) -> NavState;
}

impl NavSource for crate::webview::PlatformWebView {
    fn nav_state(&self) -> NavState {
        crate::webview::PlatformWebView::nav_state(self)
    }
}

/// 창 하나의 깨움 판정: 그 창의 webview 맵에서 보류 항목의 native 탐색 상태를 읽어 `take_settled`에 넘긴다.
fn wake_window<W: NavSource>(record: &mut RevealPending, webviews: &HashMap<u32, W>) -> bool {
    take_settled(record, |sid| webviews.get(&sid).map(NavSource::nav_state))
}

/// 동기화가 본 숨김 보류 목록을 기록에 반영한다. 기록의 탐색 상태는 지금 상태로 바꾸고, 목록에 없는
/// surface(드러났거나 사라진 것)는 뺀다. 처음 오래 숨겨진 surface마다 `warn_after`가 지나면 `warn`을 한 번 부른다.
pub(super) fn record_pending(
    record: &mut RevealPending,
    pending: &[(u32, NavState)],
    now: Instant,
    warn_after: std::time::Duration,
    mut warn: impl FnMut(u32, NavState),
) {
    for &(sid, nav) in pending {
        let entry = record.entry(sid).or_insert((now, false, nav));
        entry.2 = nav;
        if !entry.1 && now.duration_since(entry.0) >= warn_after {
            entry.1 = true;
            warn(sid, nav);
        }
    }
    // 다시 보류되면 시계가 새로 시작한다.
    record.retain(|sid, _| pending.iter().any(|(p, _)| p == sid));
}

/// 기록한 탐색 상태와 지금 native 상태가 다른 보류 항목이 있으면 true를 돌려주고, 그 항목의 기록을 지금
/// 상태로 바꾼다. 사라진 webview는 세지 않는다. 같은 변화로는 한 번만 true다.
fn take_settled(record: &mut RevealPending, current: impl Fn(u32) -> Option<NavState>) -> bool {
    let mut settled = false;
    for (sid, (_, _, seen)) in record.iter_mut() {
        if let Some(now) = current(*sid)
            && now != *seen
        {
            *seen = now;
            settled = true;
        }
    }
    settled
}

#[cfg(test)]
mod tests {
    use super::{NavSource, RevealPending, record_pending, take_settled, wake_window};
    use crate::webview::NavState;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    /// native 탐색 상태만 가진 시험용 webview.
    struct FakeWebView(NavState);

    impl NavSource for FakeWebView {
        fn nav_state(&self) -> NavState {
            self.0
        }
    }

    const WARN: Duration = Duration::from_secs(5);

    fn record(pending: &[(u32, NavState)]) -> RevealPending {
        let mut record = RevealPending::new();
        record_pending(&mut record, pending, Instant::now(), WARN, |_, _| {});
        record
    }

    /// Given 원격 entry 본문이 게시돼 host가 아무것도 그리지 않고 native가 Loading이라 숨긴 webview,
    /// When native 탐색이 Done이 됨, Then 그 창을 입력 없이 한 번 다시 그린다.
    #[test]
    fn a_hidden_load_that_finishes_wakes_the_window_once() {
        let mut pending = record(&[(1, NavState::Loading)]);
        assert!(take_settled(&mut pending, |_| Some(NavState::Done)));
        // 다시 그리기를 받은 창이 동기화 전에 돌아가 기록이 갱신되지 않은 회차가 이어져도 다시 깨우지 않는다.
        assert!(!take_settled(&mut pending, |_| Some(NavState::Done)));
        assert!(!take_settled(&mut pending, |_| Some(NavState::Done)));
    }

    /// Given 같은 숨김 보류, When native 탐색이 실패함, Then 오류 화면을 그리도록 한 번 다시 그리고,
    /// 동기화가 Failed로 남은 항목을 다시 기록해도 반복하지 않는다.
    #[test]
    fn a_hidden_load_that_fails_wakes_the_window_once() {
        let mut pending = record(&[(1, NavState::Loading)]);
        assert!(take_settled(&mut pending, |_| Some(NavState::Failed)));
        record_pending(
            &mut pending,
            &[(1, NavState::Failed)],
            Instant::now(),
            WARN,
            |_, _| {},
        );
        assert!(!take_settled(&mut pending, |_| Some(NavState::Failed)));
    }

    /// Given 마지막 동기화 뒤 탐색 상태가 그대로이거나 webview가 사라짐, Then 다시 그리지 않는다.
    #[test]
    fn an_unchanged_or_closed_webview_does_not_wake() {
        let mut pending = record(&[(1, NavState::Loading), (2, NavState::Loading)]);
        assert!(!take_settled(&mut pending, |sid| (sid == 1).then_some(NavState::Loading)));
        assert!(!take_settled(&mut RevealPending::new(), |_| Some(
            NavState::Done
        )));
    }

    /// Given 동기화가 보류 항목의 탐색 상태를 Idle에서 Loading으로 다시 기록함, Then 그 Loading은 변화가 아니다.
    /// 기록을 갱신하지 않으면 동기화가 이미 본 상태로 매 회차 깨운다.
    #[test]
    fn a_sync_records_the_state_it_saw() {
        let mut pending = record(&[(1, NavState::Idle)]);
        record_pending(
            &mut pending,
            &[(1, NavState::Loading)],
            Instant::now(),
            WARN,
            |_, _| {},
        );
        assert!(!take_settled(&mut pending, |_| Some(NavState::Loading)));
    }

    /// Given 드러났거나 사라져 목록에서 빠진 surface, Then 기록에서 빠지고, 오래 숨은 surface는 한 번만 경고한다.
    #[test]
    fn the_record_follows_the_pending_list_and_warns_once() {
        let start = Instant::now();
        let mut pending = RevealPending::new();
        let mut warned = Vec::new();
        record_pending(
            &mut pending,
            &[(1, NavState::Loading), (2, NavState::Loading)],
            start,
            WARN,
            |sid, _| warned.push(sid),
        );
        record_pending(
            &mut pending,
            &[(1, NavState::Loading)],
            start + WARN,
            WARN,
            |sid, _| warned.push(sid),
        );
        record_pending(
            &mut pending,
            &[(1, NavState::Loading)],
            start + WARN * 2,
            WARN,
            |sid, _| warned.push(sid),
        );
        assert_eq!(pending.keys().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(warned, vec![1]);
    }

    /// Given 창의 webview 맵에서 숨김 보류 중인 webview, When 그 webview의 native 탐색이 Done이나 Failed가 됨,
    /// Then 창 판정이 그 맵의 상태를 읽어 한 번 깨운다. 맵에 없는 surface의 기록은 깨우지 않는다.
    #[test]
    fn the_window_reads_native_states_from_its_webview_map() {
        for settled in [NavState::Done, NavState::Failed] {
            let mut pending = record(&[(1, NavState::Loading)]);
            let mut webviews = HashMap::from([(1, FakeWebView(NavState::Loading))]);
            assert!(!wake_window(&mut pending, &webviews));
            webviews.insert(1, FakeWebView(settled));
            assert!(wake_window(&mut pending, &webviews));
            assert!(!wake_window(&mut pending, &webviews));
        }
        let mut orphan = record(&[(2, NavState::Loading)]);
        let webviews = HashMap::from([(1, FakeWebView(NavState::Done))]);
        assert!(!wake_window(&mut orphan, &webviews));
    }
}
