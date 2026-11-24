//! OS 크기 통지 뒤 앱이 다시 그리며 내는 출력을 실행 중 신호에서 빼는 구간.
//!
//! Tasty가 OS PTY 크기를 바꾸면(창 크기 변경, 숨은 터미널이 보이게 될 때의 적용, 절전 복귀) 앱은 화면을
//! 다시 그린다. 이 출력은 busy를 새로 켜지 않고(이미 busy면 유지), 자식 상태·idle-timeout 훅이 세는
//! 출력 침묵도 다시 시작시키지 않는다. 규칙: docs/design/policies/render-on-draw.md#적용이-일으킨-출력.

use std::time::{Duration, Instant};

use crate::output_origins::ChunkOrigin;
use crate::{Pty, Terminal, TerminalState};

/// OS 크기 통지 뒤 재그리기 출력으로 보는 시간. 사용자 결정(2026-11-24)으로 700ms다. Codex가 크기 변경을
/// 약 500ms 모았다가 다시 그리는 것을 측정해 PTY throttle(100ms)과 여유를 더했다.
pub(crate) const RESIZE_REDRAW_WINDOW: Duration = Duration::from_millis(700);

/// 연속 통지 구간 하나. 끊어서 이어지는 통지는 앞 구간을 이어받아 그 사이 재그리기도 덮는다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResizeRedraw {
    /// busy 판정이 재그리기로 보는 활동 청크의 시작 경계.
    activity_start: Instant,
    /// 침묵 판정이 재그리기로 보는 출력 청크의 시작 경계.
    output_start: Instant,
    /// 구간 끝(구간 밖). 마지막 통지 + 창, 재연결이면 새 연결이 시작한 시각으로 당겨진다.
    end: Instant,
    /// 구간을 처음 열 때 침묵 판정이 쓰던 출력 시각. 구간 안 출력 대신 이 값을 쓴다.
    silence_base: Instant,
}

impl ResizeRedraw {
    /// 끝 시각은 구간 밖이다. 재연결은 새 연결이 시작한 시각에 구간을 닫는데, 새 연결의 출력은 그 시각과
    /// 같은 시각에 받았을 수 있다.
    fn contains(start: Instant, end: Instant, t: Instant) -> bool {
        start <= t && t < end
    }
}

/// 열기 전 구간 상태. OS 적용이 실패하면 이것으로 되돌린다.
#[must_use]
pub struct ResizeWindowToken(Option<ResizeRedraw>);

impl TerminalState {
    /// 마지막 활동 청크가 재그리기 구간 안인가. busy 판정의 입력 에코 자리에서 함께 본다.
    pub(crate) fn last_activity_is_resize_redraw(&self) -> bool {
        self.resize_redraw.is_some_and(|r| {
            ResizeRedraw::contains(r.activity_start, r.end, self.last_activity_chunk_at)
        })
    }

    /// 침묵 판정이 쓸 출력 시각. 마지막 출력이 재그리기면 구간 전부터 세던 시각이다.
    pub(crate) fn silence_since(&self) -> Instant {
        match self.resize_redraw {
            Some(r) if ResizeRedraw::contains(r.output_start, r.end, self.last_output_at) => {
                r.silence_base
            }
            _ => self.last_output_at,
        }
    }

    fn open_resize_window(&mut self, now: Instant) -> ResizeWindowToken {
        let prev = self.resize_redraw;
        // 끝 시각은 구간 밖이라([`ResizeRedraw::contains`]) 끝 시각의 통지는 그 시각만으로는 끝난 구간을
        // 이어받지 않는다. 마지막 청크가 옛 구간 안이면 둘째 조건으로 이어받는다.
        let open = |start: Instant, t: Instant| {
            prev.is_some_and(|r| now < r.end || ResizeRedraw::contains(start, r.end, t))
        };
        let activity_start = match prev {
            Some(r) if open(r.activity_start, self.last_activity_chunk_at) => r.activity_start,
            _ => now,
        };
        let (output_start, silence_base) = match prev {
            Some(r) if open(r.output_start, self.last_output_at) => {
                (r.output_start, r.silence_base)
            }
            _ => (now, self.silence_since()),
        };
        self.resize_redraw = Some(ResizeRedraw {
            activity_start,
            output_start,
            end: now + RESIZE_REDRAW_WINDOW,
            silence_base,
        });
        ResizeWindowToken(prev)
    }

    fn confirm_resize_window(&mut self, now: Instant) {
        if let Some(r) = self.resize_redraw.as_mut() {
            r.end = r.end.max(now + RESIZE_REDRAW_WINDOW);
        }
    }

    /// 새 연결의 출력은 옛 구간으로 분류하지 않는다. `began`은 새 연결의 reader가 출력을 받기 전 시각이라
    /// 설치가 늦어도 새 연결이 받은 출력은 구간 밖이다. 이미 억제한 출력의 침묵 기준은 남긴다.
    pub(crate) fn close_resize_window_at(&mut self, began: Instant) {
        if let Some(r) = self.resize_redraw.as_mut() {
            r.end = r.end.min(began);
        }
    }

    /// 지금 들어온 청크의 출처. 침묵 판정([`Self::silence_since`])과 같은 경계(`output_start`)와 술어로
    /// 가르므로 그 판정이 재그리기로 보는 마지막 청크는 여기서도 재그리기다. 투입 때 정한 값이 정의다.
    /// 진행 중인 OS 적용이 있으면 성공하면 언제나 재그리기라(열기 시각 이후 청크, 확정이 끝을 늘린다)
    /// 열기 전 구간도 포함하는 청크만 바로 재그리기로 두고 나머지는 적용 결과를 기다린다.
    pub(crate) fn chunk_origin(&self, chunk_at: Instant) -> ChunkOrigin {
        let within = |r: Option<ResizeRedraw>| {
            r.is_some_and(|r| ResizeRedraw::contains(r.output_start, r.end, chunk_at))
        };
        match self.resize_in_flight {
            None if within(self.resize_redraw) => ChunkOrigin::Redraw,
            None => ChunkOrigin::Normal,
            Some(before) if within(before) => ChunkOrigin::Redraw,
            Some(_) => ChunkOrigin::Pending,
        }
    }

    /// 로컬 OS 적용을 시작한다. 구간을 열고 결과가 나올 때까지 들어오는 청크를 잠정으로 둔다.
    pub(crate) fn begin_os_resize(&mut self, now: Instant) -> ResizeWindowToken {
        let token = self.open_resize_window(now);
        self.resize_in_flight = Some(token.0);
        token
    }

    /// 로컬 OS 적용이 성공했다. 잠정 청크는 재그리기다.
    pub(crate) fn confirm_os_resize(&mut self, now: Instant) {
        self.confirm_resize_window(now);
        self.resize_in_flight = None;
        self.output.settle_pending_origins(ChunkOrigin::Redraw);
    }

    /// 로컬 OS 적용이 실패했다. 구간을 되돌리고 잠정 청크는 일반 출력이다.
    pub(crate) fn restore_os_resize(&mut self, token: ResizeWindowToken) {
        self.resize_redraw = token.0;
        self.resize_in_flight = None;
        self.output.settle_pending_origins(ChunkOrigin::Normal);
    }
}

impl Terminal {
    /// OS 크기 통지 직전에 재그리기 구간을 연다. 적용이 성공하면 [`Terminal::confirm_resize_window`],
    /// 실패하면 [`Terminal::restore_resize_window`]를 부른다. 열고 나서 적용 전까지 들어온 출력도
    /// 구간 안으로 셀 수 있다(OS 호출 동안 상태 락을 쥐지 않는다).
    pub fn open_resize_window(&self) -> ResizeWindowToken {
        let mut st = self.lock_state();
        let now = Instant::now();
        st.begin_os_resize(now)
    }

    /// OS 적용이 끝난 시각부터 구간 끝을 다시 잰다.
    pub fn confirm_resize_window(&self) {
        let mut st = self.lock_state();
        st.confirm_os_resize(Instant::now());
    }

    /// OS 적용이 실패했으면 구간을 열기 전으로 되돌린다.
    pub fn restore_resize_window(&self, token: ResizeWindowToken) {
        self.lock_state().restore_os_resize(token);
    }

    /// 크기 통지를 이미 받은 쪽(attach mirror의 서버 `Resize` 적용)이 받은 시각 `received`부터 구간을
    /// 열고 확정한다. 적용이 밀려도 구간은 받은 시각 기준이다.
    pub fn note_resize_redraw_at(&self, received: Instant) {
        let mut st = self.lock_state();
        let _previous = st.open_resize_window(received); // 확정하므로 되돌릴 상태가 필요 없다.
        st.confirm_resize_window(received);
    }

    /// 자식 상태의 출력 침묵과 idle-timeout 훅이 쓰는 마지막 출력 시각. OS 크기 통지가 일으킨
    /// 재그리기 출력은 세지 않는다. 락이 사용 중이면 최근 활동으로 보고 현재 시각을 돌려준다.
    pub fn silence_since(&self) -> Instant {
        match self.state.try_lock() {
            Ok(st) => st.silence_since(),
            Err(std::sync::TryLockError::WouldBlock) => Instant::now(),
            Err(std::sync::TryLockError::Poisoned(p)) => tasty_utils::poison::recover_poisoned(
                p,
                crate::STATE_WHAT,
                &crate::STATE_POISON_REPORTED,
            )
            .silence_since(),
        }
    }
}

/// 예약된 OS 크기를 적용하고 크기가 실제로 바뀌면 그 터미널에 재그리기 구간을 연다. `throttled`면
/// throttle 간격을 지킨다. OS 크기를 바꿨으면 true다. 같은 크기 재적용과 실패는 구간을 열지 않는다.
pub fn flush_os_resize(terminal: &Terminal, pty: &mut Pty, throttled: bool) -> bool {
    let Some(plan) = pty.take_os_resize(throttled) else {
        return false;
    };
    let token = plan.changes_size().then(|| terminal.open_resize_window());
    let changed = pty.apply_planned(plan);
    if let Some(token) = token {
        if changed {
            terminal.confirm_resize_window();
        } else {
            terminal.restore_resize_window(token);
        }
    }
    changed
}

#[cfg(test)]
#[path = "resize_redraw_tests.rs"]
mod tests;
