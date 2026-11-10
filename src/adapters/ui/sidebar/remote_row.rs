//! 저장된 원격 행의 사이드바 표지 값. 연결 상태, 실행 수 숨김, 이유 표지 툴팁을 행 뷰에 싣는다.
//! 그리기는 `view::card_status`가 맡는다.

use tasty_remote::refusal::RowNoticeKind;
use tasty_ui_widgets::{RemoteRowReason, RemoteRowState};

use super::view::WorkspaceEntryView;
use crate::adapters::ui::attach_notice::{AttachNoticeView, AttachNotices};
use crate::runtime::engine_read::EngineRead;

/// 안내 이유를 행 팝업·툴팁의 이유 문장으로 옮긴다.
pub(crate) fn reason_of(kind: RowNoticeKind) -> RemoteRowReason {
    match kind {
        RowNoticeKind::SelfInstance => RemoteRowReason::SelfInstance,
        RowNoticeKind::ProfileMissing => RemoteRowReason::ProfileMissing,
        RowNoticeKind::Unresolved => RemoteRowReason::Endpoint,
        RowNoticeKind::RemoteWorkspaceMissing => RemoteRowReason::WorkspaceMissing,
        RowNoticeKind::Occupied => RemoteRowReason::Occupied,
        RowNoticeKind::MirrorChain => RemoteRowReason::Chain,
    }
}

/// 원격 workspace 칸의 값. 저장 행은 원격 workspace 의 ID 만 기록하므로 그 번호를 보인다.
pub(crate) fn remote_label(binding: &crate::model::RemoteBinding) -> String {
    binding.remote_workspace.to_string()
}

/// 이유 문장. 인자는 이유에 따라 프로필 이름이나 원격 workspace 다.
pub(crate) fn reason_text(
    reason: RemoteRowReason,
    binding: &crate::model::RemoteBinding,
) -> String {
    let arg = if reason.names_profile() {
        binding.profile.clone()
    } else {
        remote_label(binding)
    };
    crate::i18n::t_fmt(reason.key(), &arg)
}

/// 행·레일 툴팁: 무엇인지, 어디를 가리키는지, 이유.
fn row_tip(binding: &crate::model::RemoteBinding, notice: &AttachNoticeView) -> String {
    format!(
        "{}\n{} → {}\n{}",
        crate::i18n::t("attach.mirror_workspace"),
        binding.profile,
        remote_label(binding),
        reason_text(reason_of(notice.kind), binding)
    )
}

impl WorkspaceEntryView {
    /// 저장된 원격 행이면 연결 상태를 싣는다. 연결되지 않은 동안의 실행 수는 끊긴 연결의 낡은 값이라
    /// 숨기고(idle 점), 거절 표지 툴팁은 원격 행의 이유 문장으로 바꾼다.
    pub(super) fn with_remote_row(
        mut self,
        notices: &AttachNotices,
        engine: &EngineRead<'_>,
        ws: &crate::model::Workspace,
    ) -> Self {
        let Some(binding) = ws.remote_row.as_ref().filter(|_| ws.mirror) else {
            return self;
        };
        let state = engine.remote_row_state(ws.id);
        self.row_state = Some(state);
        if state != RemoteRowState::Connected {
            self.busy_count = 0;
        }
        self.attach_refusal = notices.get(ws.id).map(|notice| row_tip(binding, notice));
        self
    }
}
