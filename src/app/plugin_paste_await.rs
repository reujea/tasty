//! 붙여넣을 글자를 기다리는 플러그인 편집. 네이티브 Wayland 에서 다른 앱의 선택은 늦게 읽히므로, 회신을 약속한
//! 플러그인 입력칸의 붙여넣기 키는 글자가 올 때까지 그 창에 붙잡는다. 이때는 아직 플러그인에 아무것도 보내지 않았다.
//! 글자가 오면 편집 요청을 보내고 회신 보류로 옮기며, 오지 않으면 원래 키를 재생하거나(빈 값·실패·시간 초과) 버린다
//! (대상 상실·다른 읽기의 대체·포인터 누름·창 포커스 상실·플러그인 종료).
//! 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use std::time::Instant;

use tasty_plugin_protocol::{EditRequestWire, RawInputEventWire};
use winit::event::WindowEvent;
use winit::window::WindowId;

use super::{PluginEditHold, PluginEditHolds, ResolvedPluginEdit, plugin_replay};
use crate::adapters::ui::input::edit_key::{EditKey, EditKeyHold, LineItem, Release};
use crate::gfx::gpu::edit_keys::{AWAIT_TEXT_LIMIT, HeldEvent, LineEntry};
use crate::gfx::gpu::plugin_paste_read::PluginPasteEnd;
use crate::plugin::manager::{PluginCopyTarget, PluginEditOutcome, PluginEditRequest};

/// 글자를 기다리는 붙여넣기. 요청 id 는 키 시점에 받아 둔다. 그 실행이 끝나면 회신 대기처럼 `Gone`이 와서, 다시
/// 시작한 같은 id 의 플러그인에 이전 붙여넣기를 보내지 않는다.
pub(super) struct PasteAwait {
    request: PluginEditRequest,
    tag: u64,
    /// 붙잡은 원래 키. 팔레트의 붙여넣기는 없다.
    original: Option<WindowEvent>,
    /// 기다리는 동안 그 창에 온 키·IME·modifier, 그리고 그 뒤의 포인터 입력(debug 주입 포함).
    queued: Vec<LineItem<WindowEvent>>,
    format_paste: bool,
    since: Instant,
}

impl PasteAwait {
    /// 줄 선 입력 가운데 가장 먼저 창에 도착한 시각(X11 관찰 정리, `explorer_ime`).
    pub(super) fn first_queued_at(&self) -> Option<Instant> {
        self.queued.iter().find_map(LineItem::arrived_at)
    }

    /// 키·IME·modifier 는 줄을 서고, 포인터 입력은 이미 줄 선 입력이 있을 때만 그 뒤에 선다.
    fn takes(&self, input: &impl HeldEvent) -> bool {
        input.queues_behind_hold() || (input.is_pointer() && !self.queued.is_empty())
    }
}

/// 기다림을 정리한 결과. App 이 요청을 취소하고, 읽기를 취소하고, 요청을 대상에 붙이고, 입력을 재생한다.
#[derive(Default)]
pub(crate) struct AwaitSettled {
    pub(crate) resolved: Vec<ResolvedPluginEdit>,
    /// 더 기다리지 않는 편집 요청 id.
    pub(crate) cancelled: Vec<u64>,
    /// 취소할 읽기(창, tag).
    pub(crate) reads: Vec<(WindowId, u64)>,
    /// 대상 입력에 붙일 편집 요청.
    pub(crate) sends: Vec<(WindowId, PluginCopyTarget, RawInputEventWire)>,
}

impl AwaitSettled {
    fn extend(&mut self, other: AwaitSettled) {
        self.resolved.extend(other.resolved);
        self.cancelled.extend(other.cancelled);
        self.reads.extend(other.reads);
        self.sends.extend(other.sends);
    }
}

impl PluginEditHolds {
    /// 글자를 기다리는 붙여넣기를 그 창에 붙잡는다. `request`는 키 시점에 받은 요청이고 글자가 오면 그 id 로 보낸다.
    pub(crate) fn await_paste(
        &mut self,
        window: WindowId,
        request: PluginEditRequest,
        tag: u64,
        original: Option<WindowEvent>,
        format_paste: bool,
        now: Instant,
    ) {
        let waiting = PasteAwait {
            request,
            tag,
            original,
            queued: Vec::new(),
            format_paste,
            since: now,
        };
        if let Some(earlier) = self.awaits.insert(window, waiting) {
            tracing::warn!(
                edit_id = earlier.request.edit_id,
                "a plugin paste await replaced an earlier one of the same window"
            );
        }
    }

    /// 기다리는 창의 입력을 줄 세운다. 기다림이 없으면 `None`이다. 키·IME·modifier 는 줄을 서고, 포인터 입력은 이미
    /// 줄 선 입력이 있을 때만 그 뒤에 선다(없으면 그대로 처리돼 기다리는 동안 창이 멈추지 않는다).
    pub(super) fn queue_behind_await(
        &mut self,
        window: WindowId,
        event: &impl LineEntry<WindowEvent>,
        at: Instant,
    ) -> Option<bool> {
        let waiting = self.awaits.get_mut(&window)?;
        let queues = waiting.takes(event);
        if queues {
            waiting.queued.push(event.line_item(at));
        }
        Some(queues)
    }

    /// 기다리는 창의 포인터 누름과 창 포커스 상실(`HeldEvent::interrupts_await`, debug 포인터 누름도 같다)은 기다림을 끝낸다. 호스트는 대상 안의 입력칸 포커스를 볼 수 없어
    /// 누름이 포커스를 옮겼는지 모르고, 포커스를 잃은 창에 늦게 붙여넣지 않는다. 원래 키는 재생하지 않고 줄 선
    /// 입력만 재생한다. 호출자는 그 뒤 `event`를 처리한다.
    pub(crate) fn interrupt_await(
        &mut self,
        window: WindowId,
        event: &impl HeldEvent,
    ) -> Option<AwaitSettled> {
        if !event.interrupts_await() {
            return None;
        }
        let waiting = self.awaits.remove(&window)?;
        let mut out = AwaitSettled::default();
        end_await(
            window,
            waiting,
            false,
            true,
            "pointer press or focus loss",
            &mut out,
        );
        Some(out)
    }

    /// pump 직후 회신 보류 정리 전에 부른다. 대상을 잃은 기다림을 먼저 끝내고, 그 실행이 끝난 기다림(`outcomes`의
    /// 그 요청 결과)을 끝낸 뒤, 끝난 읽기(`arrivals`)를 적용하고, 마지막에 시간 초과를 푼다. 기다림의 결과는
    /// `outcomes`에서 빼낸다.
    pub(crate) fn settle_awaits(
        &mut self,
        outcomes: &mut Vec<PluginEditOutcome>,
        arrivals: Vec<(WindowId, u64, PluginPasteEnd)>,
        now: Instant,
        live: &impl Fn(WindowId, &PluginCopyTarget) -> bool,
    ) -> AwaitSettled {
        let mut out = AwaitSettled::default();
        let lost: Vec<WindowId> = self
            .awaits
            .iter()
            .filter(|(w, a)| !live(**w, &a.request.target))
            .map(|(w, _)| *w)
            .collect();
        for window in lost {
            if let Some(waiting) = self.awaits.remove(&window) {
                end_await(window, waiting, false, true, "target lost", &mut out);
            }
        }
        outcomes.retain(|outcome| {
            let window = self
                .awaits
                .iter()
                .find(|(_, a)| a.request.edit_id == outcome.request.edit_id)
                .map(|(w, _)| *w);
            let Some(waiting) = window.and_then(|w| self.awaits.remove(&w).map(|a| (w, a))) else {
                return true;
            };
            let (window, waiting) = waiting;
            end_await(window, waiting, false, true, "plugin run ended", &mut out);
            false
        });
        for (window, tag, end) in arrivals {
            if self.awaits.get(&window).is_none_or(|a| a.tag != tag) {
                tracing::debug!("plugin paste read ended without an await (dropped)");
                continue;
            }
            let Some(waiting) = self.awaits.remove(&window) else {
                continue;
            };
            match end {
                PluginPasteEnd::Text(text) => {
                    let sent = self.send_awaited(
                        window,
                        waiting,
                        EditRequestWire::PasteText { text },
                        now,
                    );
                    out.extend(sent);
                }
                PluginPasteEnd::Nothing if waiting.format_paste => {
                    let sent = self.send_awaited(window, waiting, EditRequestWire::Paste, now);
                    out.extend(sent);
                }
                PluginPasteEnd::Nothing => {
                    end_await(window, waiting, true, false, "nothing to paste", &mut out);
                }
                PluginPasteEnd::Replaced => {
                    end_await(window, waiting, false, false, "read replaced", &mut out);
                }
            }
        }
        let late: Vec<WindowId> = self
            .awaits
            .iter()
            .filter(|(_, a)| now.saturating_duration_since(a.since) >= AWAIT_TEXT_LIMIT)
            .map(|(w, _)| *w)
            .collect();
        for window in late {
            if let Some(waiting) = self.awaits.remove(&window) {
                end_await(window, waiting, true, true, "read timed out", &mut out);
            }
        }
        out
    }

    /// 기다리던 붙여넣기의 요청을 대상에 붙이고 회신 보류로 옮긴다. 줄 선 입력은 그 보류 뒤에 그대로 선다. 키 없는
    /// 붙여넣기(팔레트)는 키 없는 요청으로 기록하고 줄 선 입력을 바로 재생한다.
    fn send_awaited(
        &mut self,
        window: WindowId,
        waiting: PasteAwait,
        edit: EditRequestWire,
        now: Instant,
    ) -> AwaitSettled {
        let PasteAwait {
            request,
            original,
            queued,
            ..
        } = waiting;
        let target = request.target;
        let mut out = AwaitSettled::default();
        out.sends.push((
            window,
            target,
            RawInputEventWire::EditRequest {
                edit_id: request.edit_id,
                edit,
            },
        ));
        match original {
            Some(original) => {
                let mut hold = EditKeyHold::new(original, EditKey::Paste, false);
                for item in queued {
                    hold.queue_item(item);
                }
                self.holds.insert(
                    window,
                    PluginEditHold {
                        request,
                        hold,
                        since: now,
                    },
                );
            }
            None => {
                self.track(window, request, EditKey::Paste, now);
                if !queued.is_empty() {
                    out.resolved.push(ResolvedPluginEdit {
                        window,
                        target,
                        steps: plugin_replay(Release {
                            replay: None,
                            queued,
                        }),
                        edit: EditKey::Paste,
                        result: None,
                        copied: None,
                    });
                }
            }
        }
        out
    }

    /// 가장 이른 기다림 시간 초과 시각.
    pub(super) fn await_deadline(&self) -> Option<Instant> {
        self.awaits
            .values()
            .map(|a| a.since + AWAIT_TEXT_LIMIT)
            .min()
    }
}

/// 기다림 정리를 실행하는 쪽(App). 정리가 재생한 입력은 그 창의 평소 경로를 다시 거친다.
pub(crate) trait AwaitSink {
    fn awaits(&mut self) -> &mut PluginEditHolds;
    fn apply_awaited(&mut self, settled: AwaitSettled);
}

/// 그 창의 포인터 누름·포커스 상실로 기다림을 끝내고 정리를 실행한다. 정리가 재생한 줄 선 붙여넣기 키는 새 읽기와
/// 새 기다림을 만들 수 있으므로, 기다림이 남지 않을 때까지 같은 이벤트로 다시 끝낸다. 그래서 이 이벤트 뒤에는 어떤
/// 기다림도 남지 않고 늦게 온 글자가 포커스를 잃은 창에 가지 않는다. 다시 끝낼 때마다 재생할 입력이 줄어 반복이 끝난다.
pub(crate) fn interrupt_awaits(
    sink: &mut impl AwaitSink,
    window: WindowId,
    event: &impl HeldEvent,
) {
    while let Some(ended) = sink.awaits().interrupt_await(window, event) {
        sink.apply_awaited(ended);
    }
}

/// 기다림을 끝낸다. 요청은 보내지 않았으므로 취소한다. `replay`면 원래 키를 한 번 재생한다(편집이 일어나지 않았음이
/// 확실하다). `cancel_read`면 아직 진행 중일 수 있는 읽기도 취소한다. 줄 선 입력은 늘 재생한다.
fn end_await(
    window: WindowId,
    waiting: PasteAwait,
    replay: bool,
    cancel_read: bool,
    reason: &'static str,
    out: &mut AwaitSettled,
) {
    let PasteAwait {
        request,
        tag,
        original,
        queued,
        ..
    } = waiting;
    tracing::debug!(
        edit_id = request.edit_id,
        replay,
        reason,
        "plugin paste await ended"
    );
    out.cancelled.push(request.edit_id);
    if cancel_read {
        out.reads.push((window, tag));
    }
    let release = Release {
        replay: original
            .filter(|_| replay)
            .map(|original| (original, EditKey::Paste)),
        queued,
    };
    out.resolved.push(ResolvedPluginEdit {
        window,
        target: request.target,
        steps: plugin_replay(release),
        edit: EditKey::Paste,
        result: None,
        copied: None,
    });
}

#[cfg(test)]
#[path = "plugin_paste_await_tests.rs"]
mod tests;
