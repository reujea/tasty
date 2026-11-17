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
}

impl DeferredCommand {
    fn key(&self) -> &str {
        match self {
            Self::Tool { item, .. } => &item.key,
        }
    }
}

/// 초안별로 걸어 둔 명령과, 생성된 surface가 정해져 실행을 기다리는 명령.
#[derive(Default)]
pub(crate) struct DeferredCommands {
    next_seq: u64,
    waiting: Vec<(PendingId, u64, DeferredCommand)>,
    ready: Vec<(u32, u64, DeferredCommand)>,
}

impl DeferredCommands {
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

    /// 실행할 명령을 접수 순서대로 꺼낸다.
    pub(crate) fn take_ready(&mut self) -> Vec<(u32, DeferredCommand)> {
        let mut ready = std::mem::take(&mut self.ready);
        ready.sort_by_key(|(_, seq, _)| *seq);
        ready
            .into_iter()
            .map(|(local, _, command)| (local, command))
            .collect()
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
