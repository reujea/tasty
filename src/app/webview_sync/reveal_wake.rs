//! 숨김 보류 중인 native webview의 탐색이 끝나면 그 창을 다시 그린다.
//!
//! OS의 로드 완료·실패 콜백은 native 탐색 상태만 바꾸고 다시 그리기를 요청하지 않는다. 드러날
//! 자리에 있지만 아직 숨긴 webview는 다음 동기화에서만 드러나므로, 입력이나 다른 프레임이 없으면
//! 숨긴 자리가 그대로 남는다. 로드 중 스피너는 reduced motion에서 연속 다시 그리기를 요청하지 않으므로
//! 완료를 화면에 반영하는 일을 스피너에 맡길 수 없다.

use crate::app::App;
use crate::view::MainView;
use crate::view::ui::View;
use crate::webview::NavState;

impl App {
    /// native 이벤트를 처리한 회차 끝에서 부른다. 숨김 보류로 기록한 뒤 탐색 상태가 바뀐 webview가
    /// 있는 창을 다시 그려, 다음 동기화가 Done이면 페이지를 드러내고 Failed이면 오류 화면을 그리게 한다.
    pub(crate) fn redraw_settled_webview_reveals(&mut self) {
        for w in self.view.views.values_mut() {
            if let Some(main) = w.as_main_mut()
                && reveal_pending_changed(main)
            {
                main.mark_dirty();
            }
        }
    }
}

fn reveal_pending_changed(view: &MainView) -> bool {
    pending_nav_changed(
        view.webview_reveal_pending
            .iter()
            .map(|(sid, (_, _, seen))| (*seen, view.webviews.get(sid).map(|wv| wv.nav_state()))),
    )
}

/// 보류로 기록할 때 본 탐색 상태와 지금 native 상태가 다른 webview가 하나라도 있으면 true다.
/// 사라진 webview는 세지 않는다. 다시 그린 동기화가 보류 기록을 지금 상태로 갱신하므로 반복되지 않는다.
fn pending_nav_changed(mut pending: impl Iterator<Item = (NavState, Option<NavState>)>) -> bool {
    pending.any(|(seen, now)| now.is_some_and(|now| now != seen))
}

#[cfg(test)]
mod tests {
    use super::pending_nav_changed;
    use crate::webview::NavState;

    /// Given 원격 entry 본문이 게시돼 host가 아무것도 그리지 않고 native가 Loading이라 숨긴 webview,
    /// When native 탐색이 Done이 됨, Then 그 창을 입력 없이 다시 그린다.
    #[test]
    fn a_hidden_load_that_finishes_redraws_the_window() {
        assert!(pending_nav_changed(
            [(NavState::Loading, Some(NavState::Done))].into_iter()
        ));
    }

    /// Given 같은 숨김 보류, When native 탐색이 실패함, Then 오류 화면을 그리도록 다시 그린다.
    #[test]
    fn a_hidden_load_that_fails_redraws_the_window() {
        assert!(pending_nav_changed(
            [(NavState::Loading, Some(NavState::Failed))].into_iter()
        ));
    }

    /// Given 마지막 동기화 뒤 탐색 상태가 그대로이거나 webview가 사라짐, Then 다시 그리지 않는다.
    #[test]
    fn an_unchanged_or_closed_webview_does_not_redraw() {
        assert!(!pending_nav_changed(std::iter::empty()));
        assert!(!pending_nav_changed(
            [
                (NavState::Loading, Some(NavState::Loading)),
                (NavState::Loading, None),
            ]
            .into_iter()
        ));
    }
}
