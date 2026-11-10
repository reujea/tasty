//! 저장된 원격 행을 연결하지 못한 이유의 안내를 정리하고 각 창에 넘긴다.
//! 안내 자체는 `tasty_remote::refusal::Refusals`가 보관한다.

use std::collections::HashMap;

use tasty_remote::refusal::RowNoticeKind;

use crate::adapters::ui::attach_notice::AttachNoticeView;
use crate::app::App;
use crate::view::ui::View as _;

/// 행이 가리키는 attach 프로필이 없다. 해석 실패 가운데 이것만 안내 이유가 따로 있다.
#[derive(Debug)]
pub(super) struct ProfileNotFound(pub(super) String);

impl std::fmt::Display for ProfileNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::t_fmt("cli.remote_profile.not_found", &self.0))
    }
}

impl std::error::Error for ProfileNotFound {}

/// 엔드포인트 해석 실패 이유. 자기 자신 거절은 이 경로로 오지 않는다.
pub(super) fn resolve_failure_kind(error: &anyhow::Error) -> RowNoticeKind {
    if error.downcast_ref::<ProfileNotFound>().is_some() {
        RowNoticeKind::ProfileMissing
    } else {
        RowNoticeKind::Unresolved
    }
}

/// 재연결 중 해석 실패 가운데 행 안내로 남길 것. 프로필이 없어진 것만 다시 시도해도 같다.
/// 연결 지점 준비 실패는 일시적일 수 있어 남기지 않는다. 남기면 재연결 중인 행에 이유 표지가
/// 붙고, 시간 창이 끝난 뒤의 팝업도 재연결 포기 대신 연결 실패로 그려진다.
pub(super) fn reconnect_keeps_resolve_notice(kind: &RowNoticeKind) -> bool {
    matches!(kind, RowNoticeKind::ProfileMissing)
}

/// 서버가 연결을 거절한 사유 가운데 다시 시도해도 같은 것. 그 밖의 실패는 `None`이다.
pub(crate) fn connect_failure_kind(error: &str) -> Option<RowNoticeKind> {
    match tasty_remote::transport::attach_rejection_reason(error)? {
        "already_attached" => Some(RowNoticeKind::Occupied),
        "workspace_not_found" => Some(RowNoticeKind::RemoteWorkspaceMissing),
        tasty_remote::transport::REJECTED_MIRROR_WORKSPACE => Some(RowNoticeKind::MirrorChain),
        _ => None,
    }
}

impl App {
    /// 닫혔거나 연결 대상이 바뀐 행의 안내를 지우고, 세대가 바뀐 창에 남은 안내를 넘긴다.
    /// 창과 parked engine의 행을 모두 본다.
    pub(super) fn sync_mapping_notices(&mut self) {
        if !self.remote.refusals.notices().is_empty() {
            let live: HashMap<u32, crate::model::RemoteBinding> = self
                .engines()
                .sessions()
                .flat_map(|(_, engine)| {
                    engine
                        .workspaces()
                        .into_iter()
                        .filter_map(|ws| ws.remote_row.clone().map(|b| (ws.id, b)))
                        .collect::<Vec<_>>()
                })
                .collect();
            self.remote
                .refusals
                .retain_current_notices(|row| live.get(&row));
        }
        let generation = self.remote.refusals.notice_generation();
        let notices: HashMap<u32, AttachNoticeView> = self
            .remote
            .refusals
            .notices()
            .into_iter()
            .map(|(anchor, notice)| {
                (
                    anchor,
                    AttachNoticeView {
                        kind: notice.kind,
                        target: notice.target.clone(),
                        announced: notice.announced,
                    },
                )
            })
            .collect();
        for (_, main, _) in self.engines_mut().window_pairs() {
            if main.state.attach_notices.generation() == Some(generation) {
                continue;
            }
            if main
                .state
                .attach_notices
                .replace(generation, notices.clone())
            {
                main.mark_dirty();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_profile_has_its_own_reason_and_other_failures_are_unresolved() {
        let missing = anyhow::Error::new(ProfileNotFound("prod-web".into()));
        assert_eq!(
            resolve_failure_kind(&missing),
            RowNoticeKind::ProfileMissing
        );
        let tunnel = anyhow::anyhow!("ssh: connect to host build-eu port 22: timed out");
        assert_eq!(resolve_failure_kind(&tunnel), RowNoticeKind::Unresolved);
    }

    #[test]
    fn a_server_rejection_that_will_repeat_has_its_own_reason() {
        let rejected = |reason: &str| {
            format!(
                "{}{reason} (holder 3)",
                tasty_remote::transport::ATTACH_REJECTED
            )
        };
        assert_eq!(
            connect_failure_kind(&rejected("already_attached")),
            Some(RowNoticeKind::Occupied)
        );
        assert_eq!(
            connect_failure_kind(&rejected("workspace_not_found")),
            Some(RowNoticeKind::RemoteWorkspaceMissing)
        );
        assert_eq!(
            connect_failure_kind(&rejected("mirror_workspace")),
            Some(RowNoticeKind::MirrorChain)
        );
        assert_eq!(connect_failure_kind("connection refused"), None);
    }

    /// 재연결 중에는 프로필이 없어진 것만 안내로 남기고 연결 지점 준비 실패는 남기지 않는다.
    #[test]
    fn a_reconnect_keeps_only_a_missing_profile_as_a_notice() {
        let missing = anyhow::Error::new(ProfileNotFound("rv".into()));
        let unresolved = anyhow::anyhow!("IPC read failed");
        assert!(reconnect_keeps_resolve_notice(&resolve_failure_kind(
            &missing
        )));
        assert!(!reconnect_keeps_resolve_notice(&resolve_failure_kind(
            &unresolved
        )));
    }
}
