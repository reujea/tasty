//! 바탕 kind WebView의 페이지에 바탕 포커스 상태를 넘긴다. 값은 `state::surface_focus::bed_focused`가
//! 세션의 포커스와 점유로 정하고, 페이지 쪽 계약은 `crate::webview::bed_focus`에 있다.

use crate::state::surface_focus::{FocusMark, bed_focused, focus_mark};
use crate::view::MainView;
use crate::webview::PlatformWebView;

use super::find_remote_surface;

/// 페이지가 상태를 받는 surface인지. 바탕 kind인 WebView만 받는다.
/// html은 원격·mirror를 포함해 띠 kind라 받지 않고, 페이지에 아무것도 넣지 않는다.
fn receives(kind: &str, plugin_id: &str) -> bool {
    focus_mark(kind, Some(plugin_id)) == FocusMark::Bed
}

/// 페이지에 보낼 값. 받지 않는 surface이거나 이미 보낸 값과 같으면 `None`이다.
fn to_send(kind: &str, plugin_id: &str, focused: bool, sent: Option<bool>) -> Option<bool> {
    (receives(kind, plugin_id) && sent != Some(focused)).then_some(focused)
}

/// surface가 받는 값. 받지 않는 surface면 `None`이다.
fn wanted(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
) -> Option<bool> {
    let remote = find_remote_surface(view, engine, sid)?;
    let focused = bed_focused(
        view.state.focused_surface_id(engine),
        sid,
        &engine.live.occupancy,
    );
    to_send(remote.kind_static, &remote.plugin_id, focused, None)
}

/// 백엔드가 상위층에 주는 연산. 값 두기는 비동기일 수 있으므로, 실패는 나중 동기화에서
/// `take_bed_focus_failed`로 회수한다. 참이면 그 백엔드는 자기 기록을 지운 상태라 같은 값을 다시 받는다.
pub(crate) trait BedFocusSink {
    fn set_bed_focused(&self, focused: bool);
    /// 마지막으로 둔 값이 실패했는지 돌려주고 표시를 지운다.
    fn take_bed_focus_failed(&self) -> bool;
}

#[cfg(windows)]
impl BedFocusSink for PlatformWebView {
    fn set_bed_focused(&self, focused: bool) {
        PlatformWebView::set_bed_focused(self, focused);
    }
    fn take_bed_focus_failed(&self) -> bool {
        PlatformWebView::take_bed_focus_failed(self)
    }
}

/// Linux는 사용자 스타일시트를 같은 호출 안에서 넣고 빼며 다시 보내서 고칠 실패가 없다.
#[cfg(target_os = "linux")]
impl BedFocusSink for PlatformWebView {
    fn set_bed_focused(&self, focused: bool) {
        PlatformWebView::set_bed_focused(self, focused);
    }
    fn take_bed_focus_failed(&self) -> bool {
        false
    }
}

/// macOS 백엔드에는 이 연산이 아직 없다(`docs/design/systems/webview.md` 연산 표).
#[cfg(not(any(target_os = "linux", windows)))]
impl BedFocusSink for PlatformWebView {
    fn set_bed_focused(&self, _focused: bool) {}
    fn take_bed_focus_failed(&self) -> bool {
        false
    }
}

/// 한 WebView의 한 번 동기화. `sent`는 마지막으로 보낸 값이고 돌려준 값이 새 기록이다. 백엔드가 실패를
/// 알리면 기록을 지워 같은 값을 다시 보낸다.
fn step(sink: &impl BedFocusSink, wanted: Option<bool>, sent: Option<bool>) -> Option<bool> {
    let sent = if sink.take_bed_focus_failed() {
        None
    } else {
        sent
    };
    match wanted {
        Some(focused) if sent != Some(focused) => {
            sink.set_bed_focused(focused);
            Some(focused)
        }
        _ => sent,
    }
}

fn record(view: &mut MainView, sid: u32, sent: Option<bool>) {
    match sent {
        Some(focused) => view.webview_bed_focus.insert(sid, focused),
        None => view.webview_bed_focus.remove(&sid),
    };
}

/// 새 WebView에 첫 문서를 싣기 전에 상태를 둔다. 포커스를 알면 첫 렌더부터 그 값이다.
pub(super) fn apply_initial(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    wv: &PlatformWebView,
) {
    let sent = step(wv, wanted(view, engine, sid), None);
    record(view, sid, sent);
}

/// 상태가 바뀌었거나 앞 전달이 실패한 WebView에만 다시 둔다. 숨긴 WebView도 갱신해 다시 보일 때 숨기기 전
/// 값을 쓰지 않는다.
pub(super) fn sync(view: &mut MainView, engine: &crate::runtime::engine_access::EngineRef<'_>) {
    let sids: Vec<u32> = view.webviews.keys().copied().collect();
    for sid in sids {
        let wanted = wanted(view, engine, sid);
        let sent = view.webview_bed_focus.get(&sid).copied();
        let Some(wv) = view.webviews.get(&sid) else {
            continue;
        };
        let next = step(wv, wanted, sent);
        record(view, sid, next);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::{BedFocusSink, receives, step, to_send};

    /// 받은 값을 적고, 다음 회수에서 돌려줄 실패 표시를 갖는 가짜 백엔드.
    #[derive(Default)]
    struct FakeSink {
        sets: RefCell<Vec<bool>>,
        failed: Cell<bool>,
    }

    impl BedFocusSink for FakeSink {
        fn set_bed_focused(&self, focused: bool) {
            self.sets.borrow_mut().push(focused);
        }
        fn take_bed_focus_failed(&self) -> bool {
            self.failed.replace(false)
        }
    }

    #[test]
    fn a_value_is_set_once_while_it_holds() {
        let sink = FakeSink::default();
        let sent = step(&sink, Some(false), None);
        assert_eq!(sent, Some(false));
        assert_eq!(step(&sink, Some(false), sent), Some(false));
        assert_eq!(*sink.sets.borrow(), vec![false]);
    }

    #[test]
    fn a_failed_value_is_sent_again_on_the_next_sync() {
        let sink = FakeSink::default();
        let sent = step(&sink, Some(false), None);
        sink.failed.set(true);
        let sent = step(&sink, Some(false), sent);
        assert_eq!(sent, Some(false));
        assert_eq!(*sink.sets.borrow(), vec![false, false]);
        // 다시 보낸 뒤 실패가 없으면 더 보내지 않는다.
        assert_eq!(step(&sink, Some(false), sent), Some(false));
        assert_eq!(sink.sets.borrow().len(), 2);
    }

    #[test]
    fn a_failure_on_a_surface_that_no_longer_receives_clears_the_record() {
        let sink = FakeSink::default();
        sink.failed.set(true);
        assert_eq!(step(&sink, None, Some(true)), None);
        assert!(sink.sets.borrow().is_empty());
    }

    #[test]
    fn only_the_bundled_markdown_page_receives_the_state() {
        assert!(receives("markdown", "com.tasty.markdown"));
        assert!(!receives("html", "com.tasty.html"));
        assert!(!receives("markdown", "com.example.markdown"));
        assert!(!receives("custom-view", "com.example.view"));
    }

    #[test]
    fn a_page_is_sent_the_value_only_when_it_changes() {
        // 첫 문서 전에는 보낸 값이 없으므로 두 상태 모두 보낸다.
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", true, None),
            Some(true)
        );
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", false, None),
            Some(false)
        );
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", true, Some(false)),
            Some(true)
        );
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", false, Some(true)),
            Some(false)
        );
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", true, Some(true)),
            None
        );
        assert_eq!(
            to_send("markdown", "com.tasty.markdown", false, Some(false)),
            None
        );
    }

    #[test]
    fn a_page_outside_the_bed_kinds_is_never_sent_a_value() {
        for sent in [None, Some(false), Some(true)] {
            for focused in [false, true] {
                assert_eq!(to_send("html", "com.tasty.html", focused, sent), None);
                assert_eq!(
                    to_send("markdown", "com.example.markdown", focused, sent),
                    None
                );
            }
        }
    }
}
