//! 플러그인 입력칸에 보낸 편집 키의 보류. 편집 요청을 보낸 원래 키를 그 창에 붙잡아 두고, pump 직후 꺼낸
//! 플러그인 회신으로 소비·재생을 정한다. 보류 중에 그 창에 온 키·IME·포인터 입력은 받은 순서대로 줄을 선다.
//! 붙여넣을 글자를 아직 읽는 붙여넣기는 요청을 보내기 전 단계로 먼저 기다린다(`plugin_paste_await`).
//! 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use winit::event::WindowEvent;
use winit::window::WindowId;

use super::App;
use super::edit_key_replay::{ReplayStep, replay_steps};
use crate::adapters::ui::input::edit_key::{EditKey, EditKeyHold, Outcome};
use crate::plugin::PluginManager;
use crate::plugin::manager::{
    PluginCopyTarget, PluginEditOutcome, PluginEditRequest, PluginEditResult,
};

#[path = "plugin_paste_await.rs"]
mod paste_await;
pub(crate) use paste_await::AwaitSettled;

/// 회신을 기다리는 상한. 넘으면 실행됐을 수 있으므로 원래 키를 소비하고 재생하지 않는다.
const PLUGIN_HOLD_LIMIT: Duration = Duration::from_secs(1);

struct PluginEditHold {
    request: PluginEditRequest,
    hold: EditKeyHold<WindowEvent>,
    since: Instant,
}

/// 키 없이 보낸 편집 요청(팔레트). 붙잡은 입력이 없어 창 입력을 줄 세우지 않고, 회신의 복사 글자만 쓴다.
struct KeylessEdit {
    window: WindowId,
    request: PluginEditRequest,
    edit: EditKey,
    since: Instant,
}

/// 창마다 하나인 플러그인 편집 보류. 보류 중의 다음 편집 키는 줄을 서므로 창마다 하나다.
#[derive(Default)]
pub(crate) struct PluginEditHolds {
    holds: HashMap<WindowId, PluginEditHold>,
    /// 요청 id 별 키 없는 편집 요청.
    keyless: HashMap<u64, KeylessEdit>,
    /// 붙여넣을 글자를 기다리는 창. 그 창에는 회신 보류가 없다(입력이 줄을 선다).
    awaits: HashMap<WindowId, paste_await::PasteAwait>,
}

/// 풀린 보류. `steps`는 재생할 입력, `copied`는 소비한 편집이 복사한 글자다. `target`은 요청을 보낸 때의 대상이다.
pub(crate) struct ResolvedPluginEdit {
    pub(crate) window: WindowId,
    pub(crate) target: PluginCopyTarget,
    pub(crate) steps: Vec<ReplayStep<WindowEvent>>,
    pub(crate) edit: EditKey,
    pub(crate) result: Option<PluginEditResult>,
    pub(crate) copied: Option<String>,
}

impl PluginEditHolds {
    /// 플러그인에 편집 요청을 보낸 원래 키를 붙잡는다. 회신이 없으면 재생하지 않는다(실행됐을 수 있다).
    /// 이미 보류가 있는 창에서는 부르지 않는다. 그 창의 키는 [`Self::queue`]로 줄을 선다.
    pub(crate) fn hold(
        &mut self,
        window: WindowId,
        request: PluginEditRequest,
        original: WindowEvent,
        edit: EditKey,
        now: Instant,
    ) {
        let hold = PluginEditHold {
            request,
            hold: EditKeyHold::new(original, edit, false),
            since: now,
        };
        if let Some(earlier) = self.holds.insert(window, hold) {
            tracing::warn!(
                edit_id = earlier.request.edit_id,
                "a plugin edit hold replaced an earlier one of the same window"
            );
        }
    }

    /// 키 없이 보낸 편집 요청을 회신까지 기록한다. 회신이 오면 재생할 것 없이 복사 글자만 돌려준다.
    pub(crate) fn track(
        &mut self,
        window: WindowId,
        request: PluginEditRequest,
        edit: EditKey,
        now: Instant,
    ) {
        self.keyless.insert(
            request.edit_id,
            KeylessEdit {
                window,
                request,
                edit,
                since: now,
            },
        );
    }

    /// 이 창의 플러그인 편집 보류 줄과 붙여넣기 기다림 줄에 선 입력 가운데 가장 먼저 창에 도착한 시각(X11 관찰 정리,
    /// `explorer_ime`).
    #[allow(
        dead_code,
        reason = "읽는 곳인 X11 관찰 정리(`explorer_ime`)는 Linux 에서만 빌드한다"
    )]
    pub(crate) fn oldest_waiting(&self, window: WindowId) -> Option<Instant> {
        let hold = self
            .holds
            .get(&window)
            .and_then(|h| h.hold.oldest_queued_at());
        let waiting = self
            .awaits
            .get(&window)
            .and_then(paste_await::PasteAwait::oldest_queued_at);
        [hold, waiting].into_iter().flatten().min()
    }

    /// 보류 중인 창의 키·IME·modifier·포인터 입력을 줄 세운다. 줄을 섰으면 true 이고 호출자는 처리하지 않는다.
    /// `at` 은 그 입력이 창에 처음 도착한 시각이고 재생 때 그대로 쓴다.
    pub(crate) fn queue(
        &mut self,
        window: WindowId,
        event: &impl crate::gfx::gpu::edit_keys::LineEntry<WindowEvent>,
        at: Instant,
    ) -> bool {
        if let Some(queued) = self.queue_behind_await(window, event, at) {
            return queued;
        }
        let Some(held) = self.holds.get_mut(&window) else {
            return false;
        };
        if !(event.queues_behind_hold() || event.is_pointer()) {
            return false;
        }
        held.hold.queue_item(event.line_item(at));
        true
    }

    /// 회신으로 보류를 푼다. 보류가 없는 회신(창이 닫혔거나 이미 시간이 지났다)은 버린다.
    pub(crate) fn resolve(&mut self, outcomes: Vec<PluginEditOutcome>) -> Vec<ResolvedPluginEdit> {
        let mut resolved = Vec::new();
        for outcome in outcomes {
            let window = self
                .holds
                .iter()
                .find(|(_, h)| h.request.edit_id == outcome.request.edit_id)
                .map(|(w, _)| *w);
            let Some((window, held)) = window.and_then(|w| self.holds.remove_entry(&w)) else {
                if let Some(keyless) = self.keyless.remove(&outcome.request.edit_id) {
                    resolved.push(ResolvedPluginEdit {
                        window: keyless.window,
                        target: keyless.request.target,
                        steps: Vec::new(),
                        edit: keyless.edit,
                        result: Some(outcome.result),
                        copied: outcome.copied,
                    });
                    continue;
                }
                tracing::debug!(
                    edit_id = outcome.request.edit_id,
                    "plugin edit outcome without a hold (dropped)"
                );
                continue;
            };
            let key_outcome = match outcome.result {
                PluginEditResult::Acted | PluginEditResult::Unknown => Outcome::Acted,
                PluginEditResult::NotActed => Outcome::NotActed,
                PluginEditResult::Gone => Outcome::TargetGone,
            };
            let edit = held.hold.edit();
            resolved.push(ResolvedPluginEdit {
                window,
                target: held.request.target,
                steps: plugin_replay(held.hold.resolve(key_outcome)),
                edit,
                result: Some(outcome.result),
                copied: outcome.copied,
            });
        }
        resolved
    }

    /// 상한까지 회신이 없는 보류를 시간 초과로 푼다. 돌려준 요청 id 는 더 기다리지 않는다.
    pub(crate) fn expire(&mut self, now: Instant) -> (Vec<ResolvedPluginEdit>, Vec<u64>) {
        let late: Vec<WindowId> = self
            .holds
            .iter()
            .filter(|(_, h)| now.saturating_duration_since(h.since) >= PLUGIN_HOLD_LIMIT)
            .map(|(w, _)| *w)
            .collect();
        let mut resolved = Vec::new();
        let mut cancelled =
            self.drop_keyless(|k| now.saturating_duration_since(k.since) >= PLUGIN_HOLD_LIMIT);
        for window in late {
            let Some(held) = self.holds.remove(&window) else {
                continue;
            };
            cancelled.push(held.request.edit_id);
            let edit = held.hold.edit();
            resolved.push(ResolvedPluginEdit {
                window,
                target: held.request.target,
                steps: plugin_replay(held.hold.resolve(Outcome::TimedOut)),
                edit,
                result: None,
                copied: None,
            });
        }
        (resolved, cancelled)
    }

    /// pump 직후 한 번 부른다. 대상을 잃은 보류를 먼저 끝낸 뒤 회신을 적용하고, 마지막에 시간 초과를 푼다. 회신을
    /// 먼저 적용하면 닫혔거나 키를 잃은 대상의 회신이 원래 키를 재생하거나 복사 글자를 쓴다. 끝낸 보류의 회신은
    /// 보류가 없어 버려진다. 돌려준 요청 id 는 더 기다리지 않는다.
    pub(crate) fn settle(
        &mut self,
        outcomes: Vec<PluginEditOutcome>,
        now: Instant,
        live: impl Fn(WindowId, &PluginCopyTarget) -> bool,
    ) -> (Vec<ResolvedPluginEdit>, Vec<u64>) {
        let (mut resolved, mut cancelled) = self.end_lost_targets(live);
        resolved.extend(self.resolve(outcomes));
        let (late, expired) = self.expire(now);
        resolved.extend(late);
        cancelled.extend(expired);
        (resolved, cancelled)
    }

    /// 대상을 잃은 보류를 바로 끝낸다(`live`가 false 인 창·대상). 대상 팝업·surface 가 닫혔거나, 그 창의 키를
    /// 다른 팝업이 받게 됐다. 실행 여부를 모르므로 원래 키는 재생하지 않고 줄 선 입력만 재생한다. 돌려준 요청
    /// id 는 더 기다리지 않는다.
    pub(crate) fn end_lost_targets(
        &mut self,
        live: impl Fn(WindowId, &PluginCopyTarget) -> bool,
    ) -> (Vec<ResolvedPluginEdit>, Vec<u64>) {
        let closed: Vec<WindowId> = self
            .holds
            .iter()
            .filter(|(w, h)| !live(**w, &h.request.target))
            .map(|(w, _)| *w)
            .collect();
        let mut resolved = Vec::new();
        let mut cancelled = self.drop_keyless(|k| !live(k.window, &k.request.target));
        for window in closed {
            let Some(held) = self.holds.remove(&window) else {
                continue;
            };
            tracing::debug!(
                edit_id = held.request.edit_id,
                "plugin edit hold ended: its target closed or lost the keys"
            );
            cancelled.push(held.request.edit_id);
            let edit = held.hold.edit();
            resolved.push(ResolvedPluginEdit {
                window,
                target: held.request.target,
                steps: plugin_replay(held.hold.resolve(Outcome::TargetGone)),
                edit,
                result: None,
                copied: None,
            });
        }
        (resolved, cancelled)
    }

    /// `gone`이 참인 키 없는 요청을 지우고 그 요청 id 를 돌려준다. 재생할 것이 없어 결과도 없다.
    fn drop_keyless(&mut self, gone: impl Fn(&KeylessEdit) -> bool) -> Vec<u64> {
        let ids: Vec<u64> = self
            .keyless
            .iter()
            .filter(|(_, k)| gone(k))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.keyless.remove(id);
        }
        ids
    }

    /// 가장 이른 시간 초과 시각. 이벤트가 없어도 이때 깨어 보류를 푼다.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        let held = self.holds.values().map(|h| h.since);
        let keyless = self.keyless.values().map(|k| k.since);
        let replies = held
            .chain(keyless)
            .map(|since| since + PLUGIN_HOLD_LIMIT)
            .min();
        match (replies, self.await_deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// 팝업 대상 보류가 아직 살아 있는지. 그 팝업이 열려 있고 그 창의 키를 지금도 받아야 한다. 회신과 재생은 키를
/// 보낸 때의 instance 로 짝지으므로, 다른 팝업(호스트 팝업 포함)이 키를 받게 됐으면 그 보류는 끝난다.
pub(crate) fn popup_target_live(
    open: Option<&HashSet<u64>>,
    key_target: Option<u64>,
    instance: u64,
) -> bool {
    key_target == Some(instance) && open.is_some_and(|open| open.contains(&instance))
}

/// 창의 키를 지금 받는 플러그인 대상. 많아야 하나가 있다(팝업이 키를 받으면 surface 는 받지 않는다).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WindowKeyTargets {
    /// 키를 받는 plugin 팝업 instance(`MainViewState::plugin_popup_key_target`).
    pub(crate) popup: Option<u64>,
    /// 키를 받는 로컬 egui-mesh surface(`MainView::plugin_surface_key_target`).
    pub(crate) surface: Option<u32>,
}

/// 보류 대상이 살아 있는지의 판정. 열린 팝업은 지금 `mgr`에서 한 번 모으고, 창의 키를 받는 대상은
/// `key_targets_of`로 묻는다. 플러그인 관리자가 없으면 열린 팝업이 없다. surface 대상은 그 창의 키를 지금도 그
/// surface 가 받을 때만 살아 있다. 닫힌 surface 는 포커스를 가질 수 없으므로 같은 판정으로 끝난다.
pub(crate) fn hold_target_live(
    mgr: Option<&PluginManager>,
    key_targets_of: impl Fn(WindowId) -> WindowKeyTargets,
) -> impl Fn(WindowId, &PluginCopyTarget) -> bool {
    let open: Option<HashSet<u64>> =
        mgr.map(|mgr| mgr.popup_instances().map(|(id, _)| id).collect());
    move |window, target| match target {
        PluginCopyTarget::Popup(id) => {
            popup_target_live(open.as_ref(), key_targets_of(window).popup, *id)
        }
        PluginCopyTarget::Surface(id) => key_targets_of(window).surface == Some(*id),
    }
}

/// 편집 결과 복사의 토스트 범위. surface 의 편집은 그 surface 에, 팝업의 편집은 창에 띄운다.
pub(crate) fn copy_scope(target: PluginCopyTarget) -> crate::adapters::ui::ToastScope {
    match target {
        PluginCopyTarget::Popup(_) => crate::adapters::ui::ToastScope::Window,
        PluginCopyTarget::Surface(surface) => crate::adapters::ui::ToastScope::Surface(surface),
    }
}

/// 원래 키는 view 에서 붙잡혔고, 줄 선 입력은 플러그인 단축키 단계 앞에서 붙잡혔다.
fn plugin_replay(
    release: crate::adapters::ui::input::edit_key::Release<WindowEvent>,
) -> Vec<ReplayStep<WindowEvent>> {
    replay_steps(release)
}

impl App {
    /// 보류 중인 창의 입력이면 줄 세운다. 줄을 섰으면 true 다. 글자를 기다리는 창의 포인터 누름·포커스 상실은
    /// 기다림을 끝내 줄 선 입력을 먼저 재생한 뒤 그 입력을 처리하게 한다. 재생이 만든 새 기다림도 끝낸다.
    pub(super) fn queue_behind_plugin_edit(
        &mut self,
        id: WindowId,
        event: &impl crate::gfx::gpu::edit_keys::LineEntry<WindowEvent>,
        at: Instant,
    ) -> bool {
        paste_await::interrupt_awaits(self, id, event);
        self.plugin_edit_holds.queue(id, event, at)
    }

    /// 창마다 끝난 플러그인 붙여넣기 읽기.
    fn take_plugin_paste_arrivals(
        &mut self,
    ) -> Vec<(
        WindowId,
        u64,
        crate::gfx::gpu::plugin_paste_read::PluginPasteEnd,
    )> {
        let mut arrivals = Vec::new();
        for (id, view) in self.view.views.iter_mut() {
            for (tag, end) in view.base_mut().gpu.take_plugin_paste_arrivals() {
                arrivals.push((*id, tag, end));
            }
        }
        arrivals
    }

    /// 기다림 정리를 실행한다. 요청과 읽기를 취소하고, 글자가 온 요청을 대상 입력에 붙이고, 입력을 재생한다.
    fn apply_await_settled(&mut self, settled: AwaitSettled) {
        let AwaitSettled {
            resolved,
            cancelled,
            reads,
            sends,
        } = settled;
        if let Some(mgr) = self.plugin_manager.as_mut() {
            for edit_id in cancelled {
                mgr.cancel_edit_request(edit_id);
            }
        }
        for (window, tag) in reads {
            if let Some(view) = self.view.views.get_mut(&window) {
                view.base_mut().gpu.cancel_plugin_paste(tag);
            }
        }
        for (window, target, wire) in sends {
            if let Some(main) = self
                .view
                .views
                .get_mut(&window)
                .and_then(|v| v.as_main_mut())
            {
                main.queue_plugin_edit_request(target, wire);
            }
        }
        for item in resolved {
            self.finish_plugin_edit(item);
        }
    }

    /// pump 직후 플러그인 편집 회신을 꺼내 보낸 창의 보류를 풀고, 시간이 지난 보류도 푼다. 소비한 편집의 복사
    /// 글자는 그 창의 복사 문으로 쓰고, 넘길 키와 줄 선 입력은 그 창의 평소 이벤트 경로로 재생한다. 그 사이 창이
    /// 닫혔으면 복사 글자와 재생할 입력은 받을 곳이 없어 버려진다.
    pub(super) fn settle_plugin_edit_holds(&mut self, now: Instant) {
        let mut outcomes = self
            .plugin_manager
            .as_mut()
            .map(|mgr| mgr.take_edit_outcomes())
            .unwrap_or_default();
        let arrivals = self.take_plugin_paste_arrivals();
        let views = &self.view.views;
        let engines = &self.engines;
        let live = hold_target_live(self.plugin_manager.as_ref(), |window| {
            let main = views.get(&window).and_then(|v| v.as_main());
            WindowKeyTargets {
                popup: main.and_then(|main| main.state.plugin_popup_key_target()),
                surface: main
                    .zip(engines.window_read(window))
                    .and_then(|(main, engine)| main.plugin_surface_key_target(&engine)),
            }
        });
        let awaited = self
            .plugin_edit_holds
            .settle_awaits(&mut outcomes, arrivals, now, &live);
        let (resolved, cancelled) = self.plugin_edit_holds.settle(outcomes, now, live);
        self.apply_await_settled(awaited);
        if let Some(mgr) = self.plugin_manager.as_mut() {
            for edit_id in cancelled {
                mgr.cancel_edit_request(edit_id);
            }
        }
        for item in resolved {
            self.finish_plugin_edit(item);
        }
    }

    fn finish_plugin_edit(&mut self, item: ResolvedPluginEdit) {
        let ResolvedPluginEdit {
            window,
            target,
            steps,
            edit,
            result,
            copied,
        } = item;
        tracing::debug!(?edit, ?target, ?result, "plugin edit hold resolved");
        if let Some(text) = copied
            && let Some(main) = self
                .view
                .views
                .get_mut(&window)
                .and_then(|v| v.as_main_mut())
        {
            main.copy_text(&text, false, "toast.copied", Vec::new(), copy_scope(target));
        }
        self.replay_edit_steps(window, steps);
    }
}

impl paste_await::AwaitSink for App {
    fn awaits(&mut self) -> &mut PluginEditHolds {
        &mut self.plugin_edit_holds
    }

    fn apply_awaited(&mut self, settled: AwaitSettled) {
        self.apply_await_settled(settled);
    }
}

#[cfg(test)]
#[path = "plugin_edit_holds_tests.rs"]
mod tests;
