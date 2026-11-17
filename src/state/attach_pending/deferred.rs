//! 생성 대기 초안을 대상으로 입력한 명령을 걸어 두고, 생성된 surface가 정해지면 실행 목록으로 옮긴다.
//! 대상은 입력 시점에 정해지며 실행 시점의 포커스를 다시 읽지 않는다.
//! 생성이 실패하거나 끝나지 않고 사라진 초안의 명령은 실행하지 않고 버린다.

use super::PendingId;
use crate::plugin::tool_registry::ToolItem;

/// 명령을 입력한 시점의 대상 surface. 생성 대기 중이면 아직 id가 없는 그 초안을 가리킨다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandSurfaceTarget {
    Surface(u32),
    Pending(PendingId),
}

/// 실제로 실행할 때 보내는 `command.invoked` 알림. 걸어 둘 때는 보내지 않는다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvokedNotice {
    pub plugin_id: String,
    pub command_id: String,
    pub surface_id: Option<u32>,
}

#[derive(Clone, Debug)]
pub(crate) enum DeferredCommand {
    Tool {
        item: ToolItem,
        invoked: Option<InvokedNotice>,
    },
    /// 새 탭을 만들 `kind`의 변환 입력 팝업. 생성된 surface 를 문맥으로 연다.
    ConvertInput { kind: String },
    /// surface 하나를 대상으로 하는 내장 액션. `kind` 는 접수 때 초안이 만들 surface 의 kind 다.
    Action {
        id: &'static str,
        kind: String,
        trigger: crate::intent::RefreshTrigger,
    },
}

impl DeferredCommand {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Tool { item, .. } => &item.key,
            Self::ConvertInput { kind } => kind,
            Self::Action { id, .. } => id,
        }
    }
}

/// 초안별로 걸어 둔 명령과, 생성된 surface가 정해져 실행을 기다리는 명령.
#[derive(Default)]
pub(crate) struct DeferredCommands {
    next_seq: u64,
    waiting: Vec<(PendingId, u64, DeferredCommand)>,
    ready: Vec<(u32, u64, DeferredCommand)>,
    /// 생성된 surface 에서 실행할 차례가 됐지만 창이 가진 것(클립보드·복사 모드)이 필요한 액션.
    view: Vec<(&'static str, u32)>,
}

impl DeferredCommands {
    /// 창이 실행할 액션으로 넘긴다. 같은 surface 의 뒤 명령은 이 액션이 실행될 때까지 기다린다.
    pub(crate) fn push_view(&mut self, id: &'static str, local: u32) {
        self.view.push((id, local));
    }

    /// 창이 실행할 액션을 기다리는 surface 다. 그 surface 의 명령은 접수 순서를 지키려 뒤로 미룬다.
    pub(crate) fn waits_for_view(&self, local: u32) -> bool {
        self.view.iter().any(|(_, l)| *l == local)
    }

    /// `take_ready` 로 꺼냈지만 창 액션 뒤에 실행할 명령을 꺼낸 순서대로 되돌린다.
    pub(crate) fn put_back(&mut self, local: u32, command: DeferredCommand) {
        self.next_seq = self.next_seq.wrapping_add(1);
        self.ready.push((local, self.next_seq, command));
    }

    pub(crate) fn take_view(&mut self) -> Vec<(&'static str, u32)> {
        std::mem::take(&mut self.view)
    }

    pub(crate) fn push(&mut self, pending: PendingId, command: DeferredCommand) {
        self.next_seq = self.next_seq.wrapping_add(1);
        self.waiting.push((pending, self.next_seq, command));
    }

    #[cfg(test)]
    pub(crate) fn waiting_for(&self, pending: PendingId) -> usize {
        self.waiting
            .iter()
            .filter(|(id, ..)| *id == pending)
            .count()
    }

    /// 초안이 local surface로 생성됐다. 선택 여부와 무관하게 그 surface에서 실행한다.
    pub(super) fn created(&mut self, pending: PendingId, local: u32) {
        let (done, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(id, ..)| *id == pending);
        self.waiting = rest;
        self.ready.extend(
            done.into_iter()
                .map(|(_, seq, command)| (local, seq, command)),
        );
    }

    /// 결속할 surface가 없어진 초안의 명령을 버리고 로그를 남긴다.
    pub(super) fn dropped(&mut self, pending: PendingId, reason: &str) {
        self.waiting.retain(|(id, _, command)| {
            let keep = *id != pending;
            if !keep {
                tracing::info!(
                    "deferred command '{}' for pending {} dropped: {reason}",
                    command.key(),
                    pending.0
                );
            }
            keep
        });
    }

    /// 실행할 명령을 접수 순서대로 꺼낸다. 창 액션을 기다리는 surface 의 명령은 남겨 둔다.
    pub(crate) fn take_ready(&mut self) -> Vec<(u32, DeferredCommand)> {
        let (waiting, mut ready): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ready)
            .into_iter()
            .partition(|(local, ..)| self.waits_for_view(*local));
        self.ready = waiting;
        ready.sort_by_key(|(_, seq, _)| *seq);
        ready
            .into_iter()
            .map(|(local, _, command)| (local, command))
            .collect()
    }
}

impl super::AttachPending {
    /// 아직 생성을 기다리며 명령을 받을 수 있는 초안. 결과가 적용됐거나 실패·접힘으로 끝났으면 없다.
    pub(crate) fn waiting_entry(
        &self,
        core: &crate::core::CoreState,
        pending: PendingId,
    ) -> Option<&super::Entry> {
        self.entries.iter().find(|entry| {
            entry.id == pending && entry.creation.is_some() && entry.visible && entry.live(core)
        })
    }
}

impl crate::state::MainViewState {
    /// 변환 입력 팝업을 생성 대기 초안에 걸어 둔다. 팝업 선언 범위와 관계없이 생성된 surface 의
    /// 문맥(cwd·mirror)을 받도록 건다. kind 에 변환 입력 팝업이 없으면 걸지 않고 false 다.
    pub(in crate::state) fn defer_convert_input(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        kind: &str,
        pending: PendingId,
    ) -> bool {
        let declared = engine
            .surface_registry
            .get(kind)
            .and_then(|d| d.convert_input_popup.clone())
            .is_some_and(|popup_ref| popup_ref.contains('/'));
        if !declared {
            tracing::warn!("convert-input popup: kind '{kind}' has no usable convert_input_popup");
            return false;
        }
        self.attach_pending.deferred.push(
            pending,
            DeferredCommand::ConvertInput {
                kind: kind.to_string(),
            },
        );
        true
    }

    /// 걸어 둔 변환 입력 팝업을 생성된 `local` surface 를 문맥·대상으로 큐에 넣는다. 새 탭을 만드는
    /// 입력이므로 제자리 변환 표시(`surface_id`)는 넣지 않는다. kind 는 지금 다시 찾는다.
    pub(crate) fn open_deferred_convert_input(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        kind: &str,
        local: u32,
    ) -> bool {
        self.queue_convert_input(engine, kind, Some(local), None)
    }
}

/// 시험용 plugin 팝업 도구 명령.
#[cfg(test)]
pub(crate) fn test_popup_command(popup_id: &str) -> DeferredCommand {
    DeferredCommand::Tool {
        item: ToolItem {
            source: crate::plugin::tool_registry::ToolSource::Plugin {
                plugin_id: "com.example.viewer".into(),
                tool_id: "t".into(),
            },
            key: popup_id.into(),
            label_i18n_key: String::new(),
            icon: None,
            action: crate::plugin::manifest::ToolAction::OpenPopup {
                popup_id: popup_id.into(),
            },
            order_hint: 0,
        },
        invoked: None,
    }
}

/// 시험용: 첫 생성 요청이 remote 99 → `local`로 성공해 적용된 것으로 정산한다.
#[cfg(test)]
pub(crate) fn settle_first_created(
    state: &mut crate::state::MainViewState,
    core: &crate::core::CoreState,
    local: u32,
) {
    let entry = &state.attach_pending.entries[0];
    entry.receipt.bind(0);
    let (epoch, view) = (entry.epoch.clone(), entry.view.clone());
    state.attach_pending.result(&epoch, 0, Some(99), true);
    state.attach_pending.applied(
        core,
        &state.navigation,
        &epoch,
        Some(&view),
        &[(99, local)].into_iter().collect(),
        &[99],
    );
}
