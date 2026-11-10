//! 연결하지 않은 자동 attach 매핑의 창별 표시 상태. 안내 내용은 App이 넘기고,
//! 이 창의 사이드바 행 표지와 Workspace 배너가 읽는다. 닫기(×)는 이 창의 현재 활성화에만 적용한다.

use std::collections::HashMap;

use tasty_remote::refusal::RowNoticeKind;

use crate::i18n::{t, t_fmt};

use super::banner::{BannerManager, BannerScope, BannerState};

/// 한 워크스페이스의 안내.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachNoticeView {
    pub(crate) kind: RowNoticeKind,
    pub(crate) target: String,
    /// 사용자가 그 워크스페이스를 활성화한 뒤의 안내인지. false면 행 표지만 보인다.
    pub(crate) announced: bool,
}

#[derive(Default)]
pub(crate) struct AttachNotices {
    by_workspace: HashMap<u32, AttachNoticeView>,
    /// 마지막으로 받은 안내 세대. 같으면 다시 받지 않는다.
    generation: Option<u64>,
    /// 이번 활성화에서 배너를 닫은 워크스페이스. 다른 워크스페이스가 활성화되면 지운다.
    dismissed: Option<u32>,
}

pub(crate) fn reason_key(kind: RowNoticeKind) -> &'static str {
    match kind {
        RowNoticeKind::SelfInstance => "remote.refusal.self",
        RowNoticeKind::ProfileMissing => "remote.refusal.profile_missing",
        // 원격 연결 정보 팝업이 이 이유들을 대상 이름과 함께 보인다. 배너에는 일반 문장을 쓴다.
        RowNoticeKind::Unresolved
        | RowNoticeKind::RemoteWorkspaceMissing
        | RowNoticeKind::Occupied
        | RowNoticeKind::MirrorChain => "remote.refusal.unresolved",
    }
}

/// 번역문 `remote.refusal.title`의 대상 자리 앞뒤.
pub(crate) fn title_parts() -> (&'static str, &'static str) {
    let title = t("remote.refusal.title");
    title.split_once("{}").unwrap_or((title, ""))
}

/// 배너 본문. 이유 뒤에 할 일을 잇는다.
pub(crate) fn banner_body(kind: RowNoticeKind) -> String {
    format!("{} {}", t(reason_key(kind)), t("remote.refusal.hint"))
}

impl AttachNotices {
    pub(crate) fn generation(&self) -> Option<u64> {
        self.generation
    }

    /// 새 세대의 안내로 바꾼다. 내용이 바뀌었으면 true다.
    pub(crate) fn replace(
        &mut self,
        generation: u64,
        notices: HashMap<u32, AttachNoticeView>,
    ) -> bool {
        self.generation = Some(generation);
        if self.by_workspace == notices {
            return false;
        }
        self.by_workspace = notices;
        true
    }

    pub(crate) fn get(&self, workspace_id: u32) -> Option<&AttachNoticeView> {
        self.by_workspace.get(&workspace_id)
    }

    /// 사이드바 행 표지의 툴팁. 대상과 이유를 두 줄로 보인다.
    pub(crate) fn tooltip(&self, workspace_id: u32) -> Option<String> {
        let notice = self.by_workspace.get(&workspace_id)?;
        Some(format!(
            "{}\n{}",
            t_fmt("remote.refusal.title", &notice.target),
            t(reason_key(notice.kind))
        ))
    }

    pub(crate) fn dismiss(&mut self, workspace_id: u32) {
        self.dismissed = Some(workspace_id);
    }

    /// 활성 워크스페이스(이 창의 인덱스와 ID)에 맞춰 배너를 표시하거나 거둔다.
    /// 포커스와 선택은 바꾸지 않는다.
    pub(crate) fn sync_banner(
        &mut self,
        banners: &mut BannerManager,
        active: Option<(usize, u32)>,
    ) {
        if self.dismissed.is_some() && self.dismissed != active.map(|(_, id)| id) {
            self.dismissed = None;
        }
        let desired = active.and_then(|(index, id)| {
            if self.dismissed == Some(id) {
                return None;
            }
            let notice = self.by_workspace.get(&id).filter(|n| n.announced)?;
            Some(BannerState::attach_refusal(
                BannerScope::Workspace(index),
                id,
                notice.kind,
                notice.target.clone(),
            ))
        });
        banners.sync_attach_refusal(desired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notices() -> AttachNotices {
        let mut n = AttachNotices::default();
        n.replace(
            1,
            HashMap::from([(
                7,
                AttachNoticeView {
                    kind: RowNoticeKind::SelfInstance,
                    target: "127.0.0.1:7420".into(),
                    announced: true,
                },
            )]),
        );
        n
    }

    fn shown(banners: &BannerManager) -> Vec<BannerState> {
        banners.shown_banners().cloned().collect()
    }

    #[test]
    fn the_banner_follows_the_active_workspace_with_a_notice() {
        let mut n = notices();
        let mut banners = BannerManager::new();
        n.sync_banner(&mut banners, Some((2, 7)));
        let banner = shown(&banners);
        assert_eq!(banner.len(), 1);
        assert_eq!(banner[0].scope, BannerScope::Workspace(2));

        n.sync_banner(&mut banners, Some((0, 3)));
        assert!(shown(&banners).is_empty());
    }

    #[test]
    fn dismiss_hides_it_for_this_activation_only() {
        let mut n = notices();
        let mut banners = BannerManager::new();
        n.sync_banner(&mut banners, Some((2, 7)));
        n.dismiss(7);
        n.sync_banner(&mut banners, Some((2, 7)));
        assert!(shown(&banners).is_empty());
        assert!(n.tooltip(7).is_some(), "the row mark stays after ×");

        n.sync_banner(&mut banners, Some((0, 3)));
        n.sync_banner(&mut banners, Some((2, 7)));
        assert_eq!(shown(&banners).len(), 1);
    }

    #[test]
    fn an_unannounced_notice_keeps_the_mark_without_a_banner() {
        let mut n = AttachNotices::default();
        n.replace(
            1,
            HashMap::from([(
                7,
                AttachNoticeView {
                    kind: RowNoticeKind::SelfInstance,
                    target: "127.0.0.1:7420".into(),
                    announced: false,
                },
            )]),
        );
        let mut banners = BannerManager::new();
        n.sync_banner(&mut banners, Some((2, 7)));
        assert!(shown(&banners).is_empty());
        assert!(n.tooltip(7).is_some());
    }

    #[test]
    fn a_cleared_notice_takes_the_banner_and_the_mark_away() {
        let mut n = notices();
        let mut banners = BannerManager::new();
        n.sync_banner(&mut banners, Some((2, 7)));
        assert!(n.replace(2, HashMap::new()));
        n.sync_banner(&mut banners, Some((2, 7)));
        assert!(shown(&banners).is_empty());
        assert!(n.tooltip(7).is_none());
    }
}
