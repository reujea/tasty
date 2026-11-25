//! 바탕 kind WebView의 페이지에 바탕 포커스 상태를 넘긴다. 값은 마지막으로 낸 프레임이 focused 바탕을 칠한
//! surface(`MainViewState::shown_bed_focus`, 키 포커스와 점유로 정함)이고, 페이지 쪽 계약은 `crate::webview::bed_focus`에 있다.

use crate::runtime::surface_registry::KindCapabilities;
use crate::state::surface_focus::{FocusMark, focus_mark};
use crate::view::MainView;
use crate::webview::PlatformWebView;

use super::find_remote_surface;

/// 페이지가 상태를 받는 surface인지. 바탕 kind인 WebView만 받는다.
/// html은 원격·mirror를 포함해 띠 kind라 받지 않고, 페이지에 아무것도 넣지 않는다.
fn receives(kind: &str, capabilities: KindCapabilities) -> bool {
    focus_mark(kind, capabilities) == FocusMark::Bed
}

/// 페이지에 보낼 값. 받지 않는 surface이거나 이미 보낸 값과 같으면 `None`이다.
fn to_send(
    kind: &str,
    capabilities: KindCapabilities,
    focused: bool,
    sent: Option<bool>,
) -> Option<bool> {
    (receives(kind, capabilities) && sent != Some(focused)).then_some(focused)
}

/// surface가 받는 값. 받지 않는 surface면 `None`이다.
fn wanted(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
) -> Option<bool> {
    let remote = find_remote_surface(view, engine, sid)?;
    // GPU 프레임이 보류되거나 실패한 동안에도 화면에 낸 바탕과 같은 값을 둔다.
    let focused = view.state.shown_bed_focus() == Some(sid);
    to_send(remote.kind_static, remote.capabilities, focused, None)
}

/// 백엔드가 상위층에 주는 연산. 값 두기는 비동기일 수 있으므로, 실패는 나중 동기화에서
/// `take_bed_focus_failed`로 회수한다. 참이면 상위층이 같은 값을 다시 둔다. 백엔드는 같은 값을 다시 받으면
/// 실패한 부분만 다시 한다.
pub(crate) trait BedFocusSink {
    fn set_bed_focused(&self, focused: bool);
    /// 마지막으로 둔 값이 실패했는지 돌려주고 표시를 지운다.
    fn take_bed_focus_failed(&self) -> bool;
    /// 지난번 뒤로 문서 탐색이 성공으로 끝났는지 돌려주고 표시를 지운다. 다시 보내기를 멈춘 값을 한 번 더
    /// 보낼 기회다. 실패를 보고하지 않는 백엔드는 늘 거짓이다.
    fn take_bed_focus_document_loaded(&self) -> bool;
}

#[cfg(any(target_os = "macos", windows))]
impl BedFocusSink for PlatformWebView {
    fn set_bed_focused(&self, focused: bool) {
        PlatformWebView::set_bed_focused(self, focused);
    }
    fn take_bed_focus_failed(&self) -> bool {
        PlatformWebView::take_bed_focus_failed(self)
    }
    fn take_bed_focus_document_loaded(&self) -> bool {
        PlatformWebView::take_bed_focus_document_loaded(self)
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
    fn take_bed_focus_document_loaded(&self) -> bool {
        false
    }
}

/// 같은 값을 다시 보내는 최대 횟수. 그 뒤에도 실패하면 목표 값이 바뀌거나 그 view의 문서 탐색이 성공으로
/// 끝날 때까지 보내지 않는다. 탐색 완료 하나마다 한 번 더 보낸다. 실패가 계속되는 WebView(브라우저
/// 프로세스가 끝난 뒤 남은 것 등)에 프레임마다 다시 보내지 않기 위해서다. 그런 view에는 탐색 완료도 오지
/// 않는다.
const MAX_RETRIES: u8 = 1;

/// 한 surface에 마지막으로 둔 값.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BedFocusRecord {
    value: bool,
    /// 백엔드가 이 값의 실패를 알렸다.
    failed: bool,
    /// 이 값을 다시 보낸 횟수.
    retries: u8,
}

/// 한 WebView의 한 번 동기화. `record`는 마지막으로 둔 값이고 돌려준 값이 새 기록이다. 백엔드가 실패를
/// 알리면 같은 값을 [`MAX_RETRIES`]번까지 다시 보내고, 다 쓴 뒤에는 문서 탐색 완료마다 한 번 더 보낸다. 값을
/// 받지 않게 된 surface에서 실패하면 기록을 지운다.
fn step(
    sink: &impl BedFocusSink,
    wanted: Option<bool>,
    record: Option<BedFocusRecord>,
) -> Option<BedFocusRecord> {
    let mut record = record;
    if sink.take_bed_focus_failed()
        && let Some(record) = record.as_mut()
    {
        record.failed = true;
    }
    if sink.take_bed_focus_document_loaded()
        && let Some(record) = record.as_mut()
        && record.failed
        && record.retries >= MAX_RETRIES
    {
        record.retries = MAX_RETRIES - 1;
    }
    let Some(focused) = wanted else {
        return record.filter(|record| !record.failed);
    };
    match record {
        Some(record) if record.value == focused && !record.failed => Some(record),
        Some(record) if record.value == focused => {
            if record.retries >= MAX_RETRIES {
                return Some(record);
            }
            sink.set_bed_focused(focused);
            Some(BedFocusRecord {
                failed: false,
                retries: record.retries + 1,
                ..record
            })
        }
        _ => {
            sink.set_bed_focused(focused);
            Some(BedFocusRecord {
                value: focused,
                failed: false,
                retries: 0,
            })
        }
    }
}

fn record(view: &mut MainView, sid: u32, sent: Option<BedFocusRecord>) {
    match sent {
        Some(sent) => view.webview_bed_focus.insert(sid, sent),
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

    use super::{BedFocusRecord, BedFocusSink, MAX_RETRIES, receives, step, to_send};
    use crate::runtime::surface_registry::KindCapabilities;

    /// 아무것도 선언하지 않은 kind가 등록 때 받는 능력.
    fn caps(kind: &str, plugin: &str) -> KindCapabilities {
        KindCapabilities::resolve(&Default::default(), kind, plugin)
    }
    use crate::webview::bed_focus::{BedFocusApply, BedFocusDelivery, BedFocusWork};

    /// 받은 값을 적고, 다음 회수에서 돌려줄 실패 표시를 갖는 가짜 백엔드.
    #[derive(Default)]
    struct FakeSink {
        sets: RefCell<Vec<bool>>,
        failed: Cell<bool>,
        loaded: Cell<bool>,
    }

    impl BedFocusSink for FakeSink {
        fn set_bed_focused(&self, focused: bool) {
            self.sets.borrow_mut().push(focused);
        }
        fn take_bed_focus_failed(&self) -> bool {
            self.failed.replace(false)
        }
        fn take_bed_focus_document_loaded(&self) -> bool {
            self.loaded.replace(false)
        }
    }

    /// 실제 백엔드 기록(`BedFocusDelivery`)에 요청을 넘기고 할 일을 적는 가짜 백엔드. 실패와 탐색 완료는
    /// 시험이 백엔드 기록에 직접 알린다.
    #[derive(Default)]
    struct DeliverySink {
        delivery: RefCell<BedFocusDelivery<()>>,
        work: RefCell<Vec<BedFocusWork>>,
    }

    impl BedFocusSink for DeliverySink {
        fn set_bed_focused(&self, focused: bool) {
            let work = self.delivery.borrow_mut().request(focused);
            self.work.borrow_mut().extend(work);
        }
        fn take_bed_focus_failed(&self) -> bool {
            self.delivery.borrow_mut().take_failed()
        }
        fn take_bed_focus_document_loaded(&self) -> bool {
            self.delivery.borrow_mut().take_loaded()
        }
    }

    /// macOS 백엔드 기록(`BedFocusApply`)에 요청을 넘기고 지금 문서 적용 번호를 적는 가짜 백엔드.
    #[derive(Default)]
    struct ApplySink {
        delivery: RefCell<BedFocusApply>,
        applies: RefCell<Vec<(bool, u64)>>,
    }

    impl BedFocusSink for ApplySink {
        fn set_bed_focused(&self, focused: bool) {
            if let Some(work) = self.delivery.borrow_mut().request(focused) {
                self.applies.borrow_mut().push((focused, work.apply));
            }
        }
        fn take_bed_focus_failed(&self) -> bool {
            self.delivery.borrow_mut().take_failed()
        }
        fn take_bed_focus_document_loaded(&self) -> bool {
            self.delivery.borrow_mut().take_loaded()
        }
    }

    fn value(record: Option<BedFocusRecord>) -> Option<bool> {
        record.map(|record| record.value)
    }

    #[test]
    fn a_value_is_set_once_while_it_holds() {
        let sink = FakeSink::default();
        let sent = step(&sink, Some(false), None);
        assert_eq!(value(sent), Some(false));
        assert_eq!(step(&sink, Some(false), sent), sent);
        assert_eq!(*sink.sets.borrow(), vec![false]);
    }

    /// 실패가 없는 백엔드(Linux)는 기점과 같은 전이다. 값이 바뀔 때만 두고, 값을 받지 않게 된 surface는
    /// 기록을 그대로 둔다.
    #[test]
    fn without_failures_a_value_is_set_only_when_it_changes() {
        let sink = FakeSink::default();
        let mut sent = None;
        let mut expected = Vec::new();
        let mut last = None;
        for wanted in [
            Some(true),
            Some(true),
            Some(false),
            None,
            Some(false),
            Some(true),
            None,
            None,
            Some(true),
        ] {
            sent = step(&sink, wanted, sent);
            if let Some(focused) = wanted
                && last != Some(focused)
            {
                expected.push(focused);
                last = Some(focused);
            }
            assert_eq!(value(sent), last);
        }
        assert_eq!(*sink.sets.borrow(), expected);
    }

    #[test]
    fn a_failed_value_is_sent_again_on_the_next_sync() {
        let sink = FakeSink::default();
        let sent = step(&sink, Some(false), None);
        sink.failed.set(true);
        let sent = step(&sink, Some(false), sent);
        assert_eq!(value(sent), Some(false));
        assert_eq!(*sink.sets.borrow(), vec![false, false]);
        // 다시 보낸 뒤 실패가 없으면 더 보내지 않는다.
        assert_eq!(step(&sink, Some(false), sent), sent);
        assert_eq!(sink.sets.borrow().len(), 2);
    }

    #[test]
    fn a_value_that_keeps_failing_is_sent_again_only_up_to_the_limit() {
        let sink = FakeSink::default();
        let mut sent = None;
        for _ in 0..10 {
            sent = step(&sink, Some(false), sent);
            sink.failed.set(true);
        }
        assert_eq!(sink.sets.borrow().len(), 1 + usize::from(MAX_RETRIES));
    }

    #[test]
    fn a_new_target_is_sent_after_the_retries_ran_out() {
        let sink = FakeSink::default();
        let mut sent = None;
        for _ in 0..10 {
            sent = step(&sink, Some(false), sent);
            sink.failed.set(true);
        }
        let before = sink.sets.borrow().len();
        let sent = step(&sink, Some(true), sent);
        assert_eq!(sink.sets.borrow().len(), before + 1);
        assert_eq!(sink.sets.borrow().last(), Some(&true));
        assert_eq!(value(sent), Some(true));
        // 새 값은 다시 보낼 횟수를 새로 갖는다.
        sink.failed.set(true);
        step(&sink, Some(true), sent);
        assert_eq!(sink.sets.borrow().len(), before + 2);
    }

    /// 최초 등록과 재등록이 모두 실패해 상한에 걸린 뒤, 같은 view가 문서를 다시 실으면 한 번 더 보낸다.
    #[test]
    fn a_registration_that_ran_out_of_retries_is_sent_again_after_a_navigation() {
        let sink = DeliverySink::default();
        let sent = step(&sink, Some(false), None);
        sink.delivery.borrow_mut().registration_failed(1);
        let sent = step(&sink, Some(false), sent);
        sink.delivery.borrow_mut().registration_failed(2);
        let mut sent = step(&sink, Some(false), sent);
        for _ in 0..5 {
            sent = step(&sink, Some(false), sent);
        }
        assert_eq!(sink.work.borrow().len(), 2);
        sink.delivery.borrow_mut().document_loaded();
        let mut sent = step(&sink, Some(false), sent);
        assert_eq!(
            sink.work.borrow().last(),
            Some(&BedFocusWork {
                register: Some(3),
                apply: Some(3)
            })
        );
        // 다시 보낸 등록이 끝나면 더 보내지 않는다. 그 뒤 탐색 완료도 다시 보내게 하지 않는다.
        sink.delivery.borrow_mut().registered(3, "c".into());
        sink.delivery.borrow_mut().document_loaded();
        for _ in 0..5 {
            sent = step(&sink, Some(false), sent);
        }
        assert_eq!(sink.work.borrow().len(), 3);
    }

    /// 적용 실패로 재전송을 쓴 뒤 등록이 실패하면 상한에 걸린다. 탐색 완료 뒤 등록을 다시 한다.
    #[test]
    fn a_registration_failure_after_an_apply_retry_is_sent_again_after_a_navigation() {
        let sink = DeliverySink::default();
        let sent = step(&sink, Some(true), None);
        sink.delivery.borrow_mut().apply_failed(1);
        let sent = step(&sink, Some(true), sent);
        assert_eq!(
            sink.work.borrow().last(),
            Some(&BedFocusWork {
                register: None,
                apply: Some(2)
            })
        );
        sink.delivery.borrow_mut().registration_failed(1);
        let mut sent = step(&sink, Some(true), sent);
        for _ in 0..5 {
            sent = step(&sink, Some(true), sent);
        }
        assert_eq!(sink.work.borrow().len(), 2);
        sink.delivery.borrow_mut().document_loaded();
        step(&sink, Some(true), sent);
        assert_eq!(
            sink.work.borrow().last(),
            Some(&BedFocusWork {
                register: Some(2),
                apply: Some(3)
            })
        );
    }

    /// 탐색 완료 하나에 한 번만 더 보낸다. 실패가 계속되고 탐색 완료가 없으면 더 보내지 않는다.
    #[test]
    fn a_value_that_keeps_failing_is_sent_once_per_completed_navigation() {
        let sink = FakeSink::default();
        let mut sent = None;
        for _ in 0..10 {
            sent = step(&sink, Some(false), sent);
            sink.failed.set(true);
        }
        let capped = sink.sets.borrow().len();
        sink.loaded.set(true);
        for _ in 0..10 {
            sent = step(&sink, Some(false), sent);
            sink.failed.set(true);
        }
        assert_eq!(sink.sets.borrow().len(), capped + 1);
    }

    /// 성공한 값은 탐색 완료가 와도 다시 보내지 않는다.
    #[test]
    fn a_completed_navigation_does_not_resend_a_value_that_holds() {
        let sink = FakeSink::default();
        let mut sent = step(&sink, Some(true), None);
        for _ in 0..3 {
            sink.loaded.set(true);
            sent = step(&sink, Some(true), sent);
        }
        assert_eq!(*sink.sets.borrow(), vec![true]);
    }

    #[test]
    fn a_failure_on_a_surface_that_no_longer_receives_clears_the_record() {
        let sink = FakeSink::default();
        let sent = step(&sink, Some(true), None);
        sink.failed.set(true);
        assert_eq!(step(&sink, None, sent), None);
        assert_eq!(*sink.sets.borrow(), vec![true]);
    }

    #[test]
    fn only_a_bed_declaring_page_receives_the_state() {
        assert!(receives("markdown", caps("markdown", "com.tasty.markdown")));
        assert!(!receives("html", caps("html", "com.tasty.html")));
        assert!(!receives(
            "markdown",
            caps("markdown", "com.example.markdown")
        ));
        assert!(!receives(
            "custom-view",
            caps("custom-view", "com.example.view")
        ));
        let declared = KindCapabilities {
            bed_focus: true,
            ..KindCapabilities::default()
        };
        assert!(receives("custom-view", declared), "a declared bed kind");
    }

    #[test]
    fn a_page_is_sent_the_value_only_when_it_changes() {
        // 첫 문서 전에는 보낸 값이 없으므로 두 상태 모두 보낸다.
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                true,
                None
            ),
            Some(true)
        );
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                false,
                None
            ),
            Some(false)
        );
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                true,
                Some(false)
            ),
            Some(true)
        );
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                false,
                Some(true)
            ),
            Some(false)
        );
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                true,
                Some(true)
            ),
            None
        );
        assert_eq!(
            to_send(
                "markdown",
                caps("markdown", "com.tasty.markdown"),
                false,
                Some(false)
            ),
            None
        );
    }

    #[test]
    fn a_page_outside_the_bed_kinds_is_never_sent_a_value() {
        for sent in [None, Some(false), Some(true)] {
            for focused in [false, true] {
                assert_eq!(
                    to_send("html", caps("html", "com.tasty.html"), focused, sent),
                    None
                );
                assert_eq!(
                    to_send(
                        "markdown",
                        caps("markdown", "com.example.markdown"),
                        focused,
                        sent
                    ),
                    None
                );
            }
        }
    }

    /// macOS: 지금 문서 적용이 실패하면 다음 동기화에서 다시 적용하고, 성공하면 더 보내지 않는다.
    #[test]
    fn a_failed_apply_is_applied_again_and_stops_after_it_holds() {
        let sink = ApplySink::default();
        let sent = step(&sink, Some(false), None);
        sink.delivery.borrow_mut().apply_failed(1);
        let mut sent = step(&sink, Some(false), sent);
        assert_eq!(*sink.applies.borrow(), vec![(false, 1), (false, 2)]);
        for _ in 0..5 {
            sent = step(&sink, Some(false), sent);
        }
        assert_eq!(sink.applies.borrow().len(), 2);
        // 탐색 완료가 와도 성공한 값은 다시 보내지 않는다.
        sink.delivery.borrow_mut().document_loaded();
        step(&sink, Some(false), sent);
        assert_eq!(sink.applies.borrow().len(), 2);
    }

    /// macOS: 실패가 계속되면 한 번 다시 적용한 뒤 멈추고, 탐색 완료 하나마다 한 번 더 적용한다.
    #[test]
    fn an_apply_that_keeps_failing_is_applied_again_once_per_completed_navigation() {
        let sink = ApplySink::default();
        let mut sent = None;
        for _ in 0..5 {
            sent = step(&sink, Some(true), sent);
            let last = sink.applies.borrow().last().map(|(_, n)| *n);
            if let Some(n) = last {
                sink.delivery.borrow_mut().apply_failed(n);
            }
        }
        assert_eq!(sink.applies.borrow().len(), 1 + usize::from(MAX_RETRIES));
        sink.delivery.borrow_mut().document_loaded();
        step(&sink, Some(true), sent);
        assert_eq!(sink.applies.borrow().len(), 2 + usize::from(MAX_RETRIES));
    }
}
