//! 사용자 편집 바인딩을 WebKitGTK 편집 명령으로 실행한다. 계약: `docs/design/systems/webview.md`의 편집 액션 절.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use gtk::glib::Propagation;
use gtk::prelude::CancellableExt as _;
use webkit2gtk::gio::Cancellable;
use webkit2gtk::{UserMessage, WebView, WebViewExt};

use super::selection::{self, ArmKey, QueryError, ViewMessage};
use crate::webview::WebViewEditAction;

/// 응답 시점에 host 모달·overlay가 그 키를 막는지 답한다.
pub(super) type Blocked = Box<dyn Fn() -> bool>;

/// 비동기 응답이 아직 유효한지 판정한다.
///
/// - `lifecycle`은 클릭·포커스 이탈·숨김·문서 로드마다 바뀐다. 키 경로의 응답은 이것만 본다. 그 뒤의
///   키는 WebKit 입력 큐가 응답 뒤로 정렬한다.
/// - `epoch`는 그에 더해 새 키 누름마다 바뀐다. host가 키 없이 시작한 직접 질의는 이것을 본다.
/// - WebView를 버리면 retired가 된다.
#[derive(Clone, Default)]
pub(super) struct EditGuard {
    epoch: Rc<Cell<u64>>,
    lifecycle: Rc<Cell<u64>>,
    retired: Rc<Cell<bool>>,
    /// 마지막으로 센 키 누름의 X 서버 시각. 처음 받은 시각은 값과 무관하게 새 입력이다.
    key_time: Rc<Cell<Option<u32>>>,
    /// 직접 질의마다 새로 정해 응답과 대조한다.
    nonce: Rc<Cell<u64>>,
    /// 응답을 기다리는 직접 질의. 입력·로드·폐기 때 취소한다.
    pending: Rc<RefCell<Option<Cancellable>>>,
    /// 확장이 리스너를 붙였다고 알린 page. 새 문서가 commit되면 비운다.
    ready: Rc<Cell<Option<u64>>>,
    arm_nonce: Rc<Cell<u64>>,
    /// 무장한 키 명령. 무장한 순서대로 확장이 답한다.
    armed: Rc<RefCell<VecDeque<Armed>>>,
    /// 확장이 소유하는 조합. 무장할 때 더하고, 바인딩이 아닌 키로 다시 눌리면 지운다.
    claims: Rc<RefCell<std::collections::HashSet<ArmKey>>>,
    /// 확장이 무장 상한을 넘어 순서 경로를 닫은 page. 그 page가 사는 동안 바인딩 편집 키는 명령 없이 막는다.
    overflowed: Rc<Cell<Option<u64>>>,
}

struct Armed {
    nonce: u64,
    action: WebViewEditAction,
    lifecycle: u64,
    blocked: Blocked,
}

/// 답이 오지 않은 무장(페이지가 키를 멈춘 경우)이 쌓이지 않도록 오래된 것부터 버린다. 확장도 같은 상한이다.
const MAX_ARMED: usize = 64;

impl EditGuard {
    /// 선택이나 문서가 바뀌었을 수 있는 클릭·포커스 이탈·숨김·로드에서 부른다.
    pub(super) fn bump(&self) {
        self.lifecycle.set(self.lifecycle.get().wrapping_add(1));
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.cancel_pending();
        self.disarm("focus, pointer or document changed");
    }

    /// 새 키 누름에서만 센다. WebKit은 페이지가 처리하지 않은 키 누름을 원래 시각 그대로 다시 보내므로
    /// 이미 본 시각 이하(순환을 고려한 순서)의 누름은 같은 입력으로 본다. 새 누름이면 true다.
    pub(super) fn note_key(&self, time: u32) -> bool {
        let newer = is_newer_key(self.key_time.get(), time);
        if newer {
            self.key_time.set(Some(time));
            self.epoch.set(self.epoch.get().wrapping_add(1));
            self.cancel_pending();
        }
        newer
    }

    /// 새 문서가 commit됐다. 그 문서의 리스너가 준비됐다는 알림 전까지 키 경로의 Copy·Cut은 실행하지 않는다.
    pub(super) fn unready(&self) {
        self.ready.set(None);
        self.disarm("document committed");
    }

    pub(super) fn retire(&self) {
        self.retired.set(true);
        self.cancel_pending();
        self.disarm("web view dropped");
    }

    fn disarm(&self, reason: &str) {
        let count = std::mem::take(&mut *self.armed.borrow_mut()).len();
        if count > 0 {
            tracing::debug!("webview: {count} armed edit keys dropped ({reason})");
        }
    }

    /// 확장이 이 page의 순서 경로를 닫았다. 기다리는 무장을 모두 버린다.
    fn overflow(&self, page: u64) {
        if self.overflowed.replace(Some(page)) != Some(page) {
            tracing::warn!(
                "webview: more than 64 edit keys waited, so bound edit keys do nothing in this view \
                 until it is closed and opened again"
            );
        }
        self.disarm("edit keys overflowed");
    }

    fn is_overflowed(&self, page: u64) -> bool {
        self.overflowed.get() == Some(page)
    }

    /// 확장이 소유하던 조합이 더는 바인딩이 아니면 true를 돌려주고 목록에서 지운다.
    pub(super) fn unclaim(&self, key: ArmKey) -> bool {
        self.claims.borrow_mut().remove(&key)
    }

    fn push_armed(&self, action: WebViewEditAction, blocked: Blocked) -> u64 {
        let nonce = self.arm_nonce.get().wrapping_add(1);
        self.arm_nonce.set(nonce);
        let mut armed = self.armed.borrow_mut();
        if armed.len() >= MAX_ARMED {
            armed.pop_front();
        }
        armed.push_back(Armed {
            nonce,
            action,
            lifecycle: self.lifecycle.get(),
            blocked,
        });
        nonce
    }

    /// 무장이 거절되거나 실패했을 때 그 nonce만 지운다. 앞선 무장은 아직 자기 키를 기다린다.
    fn remove_armed(&self, nonce: u64) -> bool {
        let mut armed = self.armed.borrow_mut();
        let Some(at) = armed.iter().position(|a| a.nonce == nonce) else {
            return false;
        };
        armed.remove(at);
        true
    }

    /// 확장이 키에 답했을 때 이 nonce의 무장을 꺼낸다. 확장은 무장 순서대로 답하므로 그보다 앞선 무장은
    /// 페이지가 멈춘 키라 버린다.
    fn take_armed(&self, nonce: u64) -> Option<Armed> {
        let mut armed = self.armed.borrow_mut();
        let at = armed.iter().position(|a| a.nonce == nonce)?;
        armed.drain(..at);
        armed.pop_front()
    }

    fn cancel_pending(&self) {
        if let Some(pending) = self.pending.borrow_mut().take() {
            pending.cancel();
        }
    }

    fn ticket(&self) -> u64 {
        self.epoch.get()
    }

    fn still_current(&self, ticket: u64) -> bool {
        !self.retired.get() && self.epoch.get() == ticket
    }

    /// 새 질의의 nonce와 취소 수단. 앞선 질의는 취소한다.
    fn begin_query(&self) -> (u64, Cancellable) {
        self.cancel_pending();
        let nonce = self.nonce.get().wrapping_add(1);
        self.nonce.set(nonce);
        let cancellable = Cancellable::new();
        *self.pending.borrow_mut() = Some(cancellable.clone());
        (nonce, cancellable)
    }

    /// 이 질의가 아직 대기 상태의 주인이면 대기를 끝내고 true를 돌려준다. 취소됐거나 더 새 질의가
    /// 시작됐으면 false다.
    fn end_query(&self, nonce: u64) -> bool {
        self.nonce.get() == nonce && self.pending.borrow_mut().take().is_some()
    }
}

fn command_name(action: WebViewEditAction) -> &'static str {
    use WebViewEditAction as A;
    match action {
        A::Copy => "Copy",
        A::Cut => "Cut",
        A::Paste => "Paste",
        A::SelectAll => "SelectAll",
        A::Undo => "Undo",
        A::Redo => "Redo",
    }
}

/// 사용자 편집 바인딩 키를 처리한다. 확장이 이 문서에 준비돼 있으면 키를 무장하고 WebKit 입력 큐로
/// 보낸다(Proceed). 확장의 keydown 리스너가 그 키를 입력 순서대로 받아 기본 동작을 막고 답하면
/// [`on_view_message`]가 명령을 실행한다. 그래서 앞서 입력한 선택 키와 뒤의 키 사이에서 순서가 지켜진다.
/// 범위 선택이 없는 Copy·Cut은 확장이 막지 않고 답하므로 명령을 실행하지 않고 키는 엔진·페이지로 간다.
/// 포인터 입력은 WebKit의 다른 큐라서 같은 묶음의 드래그와의 순서는 보장하지 않는다. 준비되지 않았거나 키를 keyCode로 옮길 수 없으면 Copy·Cut은 실행하지 않고(엔진의 빈 복사를
/// 막도록 키를 소비한다), 다른 명령은 바로 실행한다. 함께 돌려주는 번호는 [`echo_arm`]의 무장 번호다.
pub(super) fn run_key_action(
    webview: &WebView,
    action: WebViewEditAction,
    key: Option<ArmKey>,
    guard: &EditGuard,
    blocked: Blocked,
) -> (Propagation, Option<u64>) {
    let command = command_name(action);
    let page = webview.page_id();
    if guard.is_overflowed(page) {
        note(command, "not run (edit keys overflowed in this view)");
        return (Propagation::Stop, None);
    }
    let ready = guard.ready.get() == Some(page);
    match key.filter(|_| ready) {
        Some(key) => {
            let nonce = guard.push_armed(action, blocked);
            guard.claims.borrow_mut().insert(key);
            let guard = guard.clone();
            let needs_range = matches!(action, WebViewEditAction::Copy | WebViewEditAction::Cut);
            selection::arm(webview, nonce, key, needs_range, move |armed| match armed {
                Ok(()) => {}
                Err(QueryError::Overflow) => guard.overflow(page),
                Err(e) => {
                    if guard.remove_armed(nonce) {
                        tracing::warn!(
                            "webview: editing command {command} not run: key not armed ({e})"
                        );
                    }
                }
            });
            (Propagation::Proceed, echo_arm(action, nonce))
        }
        None if matches!(action, WebViewEditAction::Copy | WebViewEditAction::Cut) => {
            let why = if ready {
                "the key has no WebKit key code"
            } else {
                "the selection extension is not ready for this document"
            };
            tracing::warn!("webview: editing command {command} not run: {why}");
            (Propagation::Stop, None)
        }
        None => {
            webview.execute_editing_command(command);
            note(command, "runs (no ordered path)");
            (Propagation::Stop, None)
        }
    }
}

/// 무장한 키 중 확장이 막지 않고 엔진·페이지로 넘길 수 있는 키의 무장 번호. 범위 선택이 필요한 Copy·Cut만
/// 해당하며, 다른 동작은 확장이 늘 기본 동작을 막아 WebKit이 반송하지 않는다.
pub(super) fn echo_arm(action: WebViewEditAction, nonce: u64) -> Option<u64> {
    matches!(action, WebViewEditAction::Copy | WebViewEditAction::Cut).then_some(nonce)
}

/// 확장의 답이 그 키의 기본 동작을 막았다는 뜻이면 true다. Copy·Cut은 범위가 있을 때만 막는다.
pub(super) fn extension_took_key(action: WebViewEditAction, range: bool) -> bool {
    range || echo_arm(action, 0).is_none()
}

/// 확장이 보낸 메시지를 처리한다. 확장의 메시지가 아니면 false다.
/// `settled`는 확장이 기본 동작을 막아 반송되지 않을 무장 번호를 받는다.
pub(super) fn on_view_message(
    webview: &WebView,
    message: &UserMessage,
    guard: &EditGuard,
    settled: impl Fn(u64),
) -> bool {
    match ViewMessage::parse(message) {
        None => false,
        Some(ViewMessage::Ready { page }) => {
            if page == webview.page_id() {
                guard.ready.set(Some(page));
            }
            true
        }
        Some(ViewMessage::Key { nonce, page, range }) => {
            let Some(armed) = guard.take_armed(nonce) else {
                if range {
                    settled(nonce);
                }
                tracing::debug!("webview: edit key reply for a dropped arm");
                return true;
            };
            if extension_took_key(armed.action, range) {
                settled(nonce);
            }
            let command = command_name(armed.action);
            let copy_like = matches!(
                armed.action,
                WebViewEditAction::Copy | WebViewEditAction::Cut
            );
            if guard.retired.get()
                || page != webview.page_id()
                || guard.lifecycle.get() != armed.lifecycle
            {
                note(command, "dropped (focus, pointer or document changed)");
            } else if (armed.blocked)() {
                note(command, "dropped (host modal or overlay open)");
            } else if copy_like && !range {
                // 확장이 이 키를 막지 않아 엔진 기본 동작과 페이지 keydown으로 갔다.
                note(
                    command,
                    "not run (nothing selected; the key went to the engine and the page)",
                );
            } else {
                webview.execute_editing_command(command);
                note(command, "runs (in input order)");
            }
            true
        }
    }
}

/// host가 키 없이 요청한 명령(팔레트·IPC)을 실행한다. Copy·Cut은 포커스 frame의 선택을 직접 물어 확인한
/// 뒤, 그 사이 입력·로드·폐기가 없었을 때만 실행한다. 확인하지 못하면 실행하지 않는다.
pub(super) fn run_editing_command(webview: &WebView, action: WebViewEditAction, guard: &EditGuard) {
    let command = command_name(action);
    if !matches!(action, WebViewEditAction::Copy | WebViewEditAction::Cut) {
        webview.execute_editing_command(command);
        return;
    }
    // WebKitGTK의 Copy·Cut은 편집 불가 본문에 선택이 없어도 빈 내용으로 클립보드를 차지해 기존 값을
    // 지운다. EditorState는 이 경우에도 참이고 최대 수백 ms 늦다.
    let ticket = guard.ticket();
    let (nonce, cancellable) = guard.begin_query();
    let guard = guard.clone();
    let target = webview.clone();
    selection::query(webview, nonce, &cancellable, move |result| {
        if !guard.end_query(nonce) {
            note(command, "dropped (query cancelled or superseded)");
        } else if guard.still_current(ticket) {
            finish(&target, command, result);
        } else {
            note(command, "dropped (input or document changed)");
        }
    });
}

/// 아직 유효한 응답으로 명령을 실행하거나 건너뛴다. 선택 내용은 기록하지 않는다.
fn finish(target: &WebView, command: &str, result: Result<bool, QueryError>) {
    match result {
        Ok(true) => {
            target.execute_editing_command(command);
            note(command, "runs (selection confirmed)");
        }
        Ok(false) => note(command, "skipped (nothing selected)"),
        Err(e) => {
            tracing::warn!(
                "webview: editing command {command} not run: selection unconfirmed ({e})"
            );
        }
    }
}

/// X 서버 시각은 약 49일마다 0으로 돌아가므로 차이의 부호로 순서를 정한다. 첫 시각과 시각 0(CurrentTime)은
/// 순서를 알 수 없어 새 입력으로 본다.
fn is_newer_key(last: Option<u32>, time: u32) -> bool {
    match last {
        None => true,
        Some(_) if time == 0 => true,
        Some(last) => (time.wrapping_sub(last) as i32) > 0,
    }
}

fn note(command: &str, outcome: &str) {
    tracing::debug!("webview: editing command {command} {outcome}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 질의 뒤 입력·로드가 있었거나 WebView를 버렸으면 응답을 쓰지 않고 기다리던 질의를 취소한다.
    #[test]
    fn a_reply_after_input_or_retirement_is_stale() {
        let guard = EditGuard::default();
        let ticket = guard.ticket();
        assert!(guard.still_current(ticket));
        let (_, pending) = guard.begin_query();
        guard.bump();
        assert!(!guard.still_current(ticket), "새 입력 뒤의 응답은 버린다");
        assert!(
            pending.is_cancelled(),
            "새 입력이나 로드는 기다리던 질의를 취소한다"
        );
        let ticket = guard.ticket();
        let (_, pending) = guard.begin_query();
        let shared = guard.clone();
        shared.retire();
        assert!(pending.is_cancelled(), "폐기하면 응답을 기다리지 않는다");
        assert!(
            !guard.still_current(ticket),
            "폐기한 WebView에는 실행하지 않는다"
        );
    }

    /// 무장은 순서대로 꺼내고, 답이 건너뛴 앞선 무장은 버린다. 뒤의 키는 무장을 무효로 하지 않지만
    /// 클릭·로드·commit·폐기는 모두 버린다.
    #[test]
    fn armed_keys_are_answered_in_order_and_dropped_on_lifecycle_changes() {
        let guard = EditGuard::default();
        let never = || -> Blocked { Box::new(|| false) };
        let a = guard.push_armed(WebViewEditAction::Copy, never());
        let b = guard.push_armed(WebViewEditAction::Paste, never());
        let c = guard.push_armed(WebViewEditAction::Undo, never());
        let lifecycle = guard.lifecycle.get();
        assert!(guard.note_key(10), "뒤의 키");
        assert_eq!(
            guard.lifecycle.get(),
            lifecycle,
            "키는 키 경로 응답을 무효로 하지 않는다"
        );
        let got = guard.take_armed(b).expect("b");
        assert_eq!(got.action, WebViewEditAction::Paste);
        assert!(guard.take_armed(a).is_none(), "건너뛴 앞선 무장은 버린다");
        assert!(guard.take_armed(b).is_none(), "한 번만 꺼낸다");
        guard.bump();
        assert!(guard.take_armed(c).is_none(), "클릭·로드는 무장을 버린다");
        assert_ne!(guard.lifecycle.get(), lifecycle);
        let d = guard.push_armed(WebViewEditAction::Copy, never());
        guard.unready();
        assert!(
            guard.take_armed(d).is_none(),
            "새 문서 commit은 무장을 버린다"
        );
        let e = guard.push_armed(WebViewEditAction::Copy, never());
        guard.retire();
        assert!(guard.take_armed(e).is_none(), "폐기는 무장을 버린다");
    }

    /// 상한을 넘으면 그 page의 무장을 모두 버리고, 같은 page에서는 계속 닫혀 있다. 새 page는 영향이 없다.
    #[test]
    fn an_overflow_closes_the_page_and_drops_every_arm() {
        let guard = EditGuard::default();
        let old = guard.push_armed(WebViewEditAction::Copy, Box::new(|| false));
        guard.push_armed(WebViewEditAction::Paste, Box::new(|| false));
        guard.overflow(5);
        assert!(guard.armed.borrow().is_empty());
        assert!(
            guard.take_armed(old).is_none(),
            "닫히기 전 무장의 키 답도 실행하지 않는다"
        );
        assert!(guard.is_overflowed(5));
        guard.unready();
        assert!(guard.is_overflowed(5), "문서가 바뀌어도 열리지 않는다");
        assert!(!guard.is_overflowed(6), "다시 만든 page는 순서 경로를 쓴다");
    }

    /// 거절·실패는 그 무장만 지우고, 키 답은 앞선 무장까지 버린다.
    #[test]
    fn a_refused_arm_removes_only_itself() {
        let guard = EditGuard::default();
        let a = guard.push_armed(WebViewEditAction::Copy, Box::new(|| false));
        let b = guard.push_armed(WebViewEditAction::Paste, Box::new(|| false));
        let c = guard.push_armed(WebViewEditAction::Undo, Box::new(|| false));
        assert!(guard.remove_armed(b));
        assert!(!guard.remove_armed(b));
        assert_eq!(guard.armed.borrow().len(), 2, "앞선 무장은 남는다");
        assert!(guard.take_armed(a).is_some());
        assert!(guard.take_armed(c).is_some());
    }

    /// 답이 오지 않은 무장은 상한을 넘으면 오래된 것부터 버린다.
    #[test]
    fn unanswered_arms_are_bounded() {
        let guard = EditGuard::default();
        let first = guard.push_armed(WebViewEditAction::Copy, Box::new(|| false));
        for _ in 0..MAX_ARMED {
            guard.push_armed(WebViewEditAction::Copy, Box::new(|| false));
        }
        assert_eq!(guard.armed.borrow().len(), MAX_ARMED);
        assert!(guard.take_armed(first).is_none());
    }

    /// 다시 보낸 옛 키 누름은 새 입력으로 세지 않는다.
    #[test]
    fn a_redelivered_key_press_does_not_invalidate_the_reply() {
        let guard = EditGuard::default();
        guard.note_key(100);
        let ticket = guard.ticket();
        guard.note_key(90);
        guard.note_key(100);
        assert!(guard.still_current(ticket));
        guard.note_key(101);
        assert!(!guard.still_current(ticket));
    }

    /// 취소된 질의나 이미 대체된 질의의 응답은 대기 상태의 주인이 아니다.
    #[test]
    fn only_the_live_query_owns_the_pending_state() {
        let guard = EditGuard::default();
        let (first, _c1) = guard.begin_query();
        guard.bump();
        assert!(!guard.end_query(first), "취소된 질의");
        let (second, _c2) = guard.begin_query();
        assert!(
            !guard.end_query(first),
            "대체된 질의는 새 질의의 대기를 끝내지 못한다"
        );
        assert!(guard.pending.borrow().is_some());
        assert!(guard.end_query(second));
        assert!(!guard.end_query(second), "한 번만 주인이다");
    }

    /// 첫 키는 시각이 2^31 이상이어도 새 입력이고, 그 뒤로는 순환을 고려해 순서를 정한다.
    #[test]
    fn key_order_survives_a_late_start_and_the_wrap_of_x_time() {
        let guard = EditGuard::default();
        let ticket = guard.ticket();
        guard.note_key(0x9000_0000);
        assert!(!guard.still_current(ticket), "첫 키는 큰 시각이어도 센다");
        let ticket = guard.ticket();
        guard.note_key(0x9000_0000);
        guard.note_key(0x8fff_ff00);
        assert!(
            guard.still_current(ticket),
            "같거나 이전 시각의 재전달은 세지 않는다"
        );

        let guard = EditGuard::default();
        guard.note_key(u32::MAX - 5);
        let ticket = guard.ticket();
        guard.note_key(u32::MAX - 10);
        assert!(
            guard.still_current(ticket),
            "순환 전 이전 시각은 재전달이다"
        );
        guard.note_key(3);
        assert!(
            !guard.still_current(ticket),
            "순환한 뒤의 작은 시각은 새 누름이다"
        );
        let ticket = guard.ticket();
        guard.note_key(u32::MAX - 1);
        assert!(
            guard.still_current(ticket),
            "순환 뒤에 늦게 온 순환 전 누름은 재전달이다"
        );

        // 시각 0은 같은 시각이라도 새 입력이다.
        let ticket = guard.ticket();
        guard.note_key(0);
        assert!(!guard.still_current(ticket));
        let ticket = guard.ticket();
        guard.note_key(0);
        assert!(!guard.still_current(ticket));
    }
}
