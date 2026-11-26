//! main frame 문서를 그리던 process가 끝났을 때 처리기가 따르는 순서: 종료 종류별로 상태를 기록한 뒤 창을
//! 한 번 깨운다. 다시 싣기와 Failed 표시는 다음 동기화에서 일어나므로, 깨움이 상태 기록보다 앞서거나 빠지면
//! 입력이 올 때까지 엔진 오류 페이지가 남는다. Windows `ProcessFailed` 처리기가 쓴다. 엔진 종류를 이 종류로
//! 바꾸는 일과 깨우기 호출(창에 그리기 요청)은 처리기 쪽에 있다.

use std::cell::Cell;

use super::NavState;

/// 끝난 process의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessEnd {
    /// 렌더 process만 끝났다. 같은 WebView로 다시 탐색해 복구할 수 있다.
    Renderer,
    /// browser process가 끝났다. WebView를 다시 만들어야 하므로 복구 대상으로 표시하지 않는다.
    Browser,
    /// main frame 로드를 끝내지 않는 종료(iframe·GPU·utility process 등). 아무것도 바꾸지 않는다.
    Other,
}

/// 종료를 기록하고 창을 깨운다. 로드를 Failed로 끝내고, 렌더 process 종료면 복구 대상으로 표시한 뒤
/// `record`에 렌더 process 종료인지를 넘겨 나머지 상태(게이트·문서 표 등)를 정리하게 한다. 그다음
/// `wake`를 한 번 부른다. 상태를 바꾼 종료면 true다.
pub(super) fn end_process(
    end: ProcessEnd,
    nav_state: &Cell<NavState>,
    content_process_ended: &Cell<bool>,
    record: impl FnOnce(bool),
    wake: impl FnOnce(),
) -> bool {
    let renderer = match end {
        ProcessEnd::Renderer => true,
        ProcessEnd::Browser => false,
        ProcessEnd::Other => return false,
    };
    nav_state.set(NavState::Failed);
    if renderer {
        content_process_ended.set(true);
    }
    record(renderer);
    wake();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 한 종료를 처리하고 (반환값, 탐색 상태, 복구 표시, record가 받은 값, 깨움 수)를 돌려준다. 깨울 때마다
    /// 그 시점의 상태가 이미 기록됐는지(Failed, 렌더 process면 복구 표시, record 호출)를 본다.
    fn run(end: ProcessEnd) -> (bool, NavState, bool, Option<bool>, u32) {
        let nav = Cell::new(NavState::Done);
        let ended = Cell::new(false);
        let recorded = Cell::new(None);
        let wakes = Cell::new(0u32);
        let changed = end_process(
            end,
            &nav,
            &ended,
            |renderer| recorded.set(Some(renderer)),
            || {
                assert_eq!(nav.get(), NavState::Failed, "깨우기 전에 Failed를 기록한다");
                assert_eq!(
                    ended.get(),
                    end == ProcessEnd::Renderer,
                    "깨우기 전에 복구 표시를 기록한다"
                );
                assert!(
                    recorded.get().is_some(),
                    "깨우기 전에 나머지 상태를 정리한다"
                );
                wakes.set(wakes.get() + 1);
            },
        );
        (changed, nav.get(), ended.get(), recorded.get(), wakes.get())
    }

    #[test]
    fn a_renderer_end_records_the_failed_load_and_recovery_then_wakes_once() {
        assert_eq!(
            run(ProcessEnd::Renderer),
            (true, NavState::Failed, true, Some(true), 1)
        );
    }

    #[test]
    fn a_browser_end_records_the_failed_load_without_recovery_then_wakes_once() {
        assert_eq!(
            run(ProcessEnd::Browser),
            (true, NavState::Failed, false, Some(false), 1)
        );
    }

    #[test]
    fn another_end_changes_nothing_and_does_not_wake() {
        assert_eq!(
            run(ProcessEnd::Other),
            (false, NavState::Done, false, None, 0)
        );
    }
}
