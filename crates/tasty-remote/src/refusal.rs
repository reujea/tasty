//! 원격 행에 연결하지 못한 이유와, 자기 자신(자기 포트, 또는 SSH 너머의 같은 인스턴스)으로 향한
//! attach 거절의 기록.
//!
//! 거절은 엔드포인트를 해석한 뒤 메인 루프에서 정해지므로 `remote.attach` 응답에 실을 수 없다.
//! 호출자는 응답에 있는 시도 번호로 이 기록을 조회한다.
//!
//! 저장된 원격 행을 연결하지 못한 이유(자기 자신, 없는 프로필, 해석 실패, 원격 workspace 없음,
//! 다른 곳에서 사용 중, mirror 체인)는 행별 안내로 남긴다. 사이드바 행 표지와 원격 연결 정보
//! 팝업이 이 안내를 읽는다. 안내는 그 행이 닫히거나 연결에 성공할 때 사라진다.

use std::collections::{HashMap, VecDeque};

use tasty_model::RemoteBinding;

/// 보관하는 최근 거절 수. 넘으면 오래된 것부터 버린다.
pub const REFUSAL_HISTORY: usize = 32;

/// 자기 포트라서 연결하지 않은 attach 시도 하나.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AttachRefusal {
    /// `remote.attach` 응답의 `attempt`와 같은 번호.
    pub attempt: u64,
    /// 연결하려던 저장된 원격 행. 수동 `remote.attach`는 `None`이다.
    pub row_workspace: Option<u32>,
    pub remote_workspace: u32,
    /// 해석한 연결 포트. loopback 직결이면 이 인스턴스의 IPC 포트이고, SSH 터널이면 터널의 로컬 포트다.
    pub port: u16,
    /// SSH 터널 너머의 `instance_id`가 이 프로세스와 같아서 거절했으면 true.
    pub via_ssh: bool,
    pub reconnect: bool,
}

/// 원격 행을 연결하지 못한 이유. 이름은 IPC 오류의 `data.reason`과 같다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowNoticeKind {
    /// 대상이 이 인스턴스 자신이다.
    SelfInstance,
    /// 행이 가리키는 attach 프로필이 없다.
    ProfileMissing,
    /// 프로필의 연결 지점을 준비하지 못했다.
    Unresolved,
    /// 원격에 그 workspace가 없다.
    RemoteWorkspaceMissing,
    /// 다른 client가 그 원격 workspace를 점유하고 있다.
    Occupied,
    /// 원격 workspace가 그쪽에서도 mirror다.
    #[serde(rename = "mirror_workspace")]
    MirrorChain,
}

/// 원격 행 하나의 연결하지 못한 이유 안내.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowNotice {
    pub kind: RowNoticeKind,
    /// 표시용 대상. 프로필 이름이다.
    pub target: String,
    /// 안내를 만든 연결 대상.
    pub binding: RemoteBinding,
}

#[derive(Default)]
pub struct Refusals {
    recent: VecDeque<AttachRefusal>,
    notices: HashMap<u32, RowNotice>,
    /// 안내가 바뀔 때마다 오른다. 표시 쪽은 같은 세대를 다시 받지 않는다.
    notice_generation: u64,
}

impl Refusals {
    pub fn record(&mut self, refusal: AttachRefusal) {
        if self.recent.len() == REFUSAL_HISTORY {
            self.recent.pop_front();
        }
        self.recent.push_back(refusal);
    }

    /// 오래된 것부터.
    pub fn recent(&self) -> impl Iterator<Item = &AttachRefusal> {
        self.recent.iter()
    }

    /// 행의 안내를 새 이유로 바꾼다.
    pub fn note(&mut self, row: u32, kind: RowNoticeKind, binding: &RemoteBinding) {
        let notice = RowNotice {
            kind,
            target: binding.profile.clone(),
            binding: binding.clone(),
        };
        if self.notices.get(&row) != Some(&notice) {
            self.notices.insert(row, notice);
            self.notice_generation += 1;
        }
    }

    /// 연결했거나 행이 닫혔을 때 안내를 지운다.
    pub fn clear_notice(&mut self, row: u32) {
        if self.notices.remove(&row).is_some() {
            self.notice_generation += 1;
        }
    }

    pub fn notice_generation(&self) -> u64 {
        self.notice_generation
    }

    /// 지금 연결 대상이 안내를 만든 대상과 같은 행만 남긴다. 행이 닫혔으면 `None`이다.
    /// 지운 행이 있으면 true다.
    pub fn retain_current_notices<'a>(
        &mut self,
        current: impl Fn(u32) -> Option<&'a RemoteBinding>,
    ) -> bool {
        let before = self.notices.len();
        self.notices
            .retain(|row, notice| current(*row) == Some(&notice.binding));
        let removed = before != self.notices.len();
        if removed {
            self.notice_generation += 1;
        }
        removed
    }

    pub fn notice(&self, row: u32) -> Option<&RowNotice> {
        self.notices.get(&row)
    }

    /// 행 ID 순서로 정렬한 안내.
    pub fn notices(&self) -> Vec<(u32, &RowNotice)> {
        let mut all: Vec<_> = self.notices.iter().map(|(a, n)| (*a, n)).collect();
        all.sort_by_key(|(a, _)| *a);
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(attempt: u64, row: Option<u32>) -> AttachRefusal {
        AttachRefusal {
            attempt,
            row_workspace: row,
            remote_workspace: 1,
            port: 4000,
            via_ssh: false,
            reconnect: false,
        }
    }

    fn binding(profile: &str) -> RemoteBinding {
        RemoteBinding {
            profile: profile.into(),
            remote_workspace: 1,
        }
    }

    #[test]
    fn a_notice_stays_until_its_binding_changes_or_goes() {
        let mut refusals = Refusals::default();
        let refused = binding("box");
        refusals.note(7, RowNoticeKind::SelfInstance, &refused);
        assert_eq!(refusals.notice(7).map(|n| n.target.as_str()), Some("box"));
        let noted = refusals.notice_generation();
        refusals.note(7, RowNoticeKind::SelfInstance, &refused);
        assert_eq!(
            refusals.notice_generation(),
            noted,
            "the same notice is not news"
        );

        assert!(!refusals.retain_current_notices(|_| Some(&refused)));
        assert!(refusals.notice(7).is_some());

        let changed = binding("other");
        assert!(refusals.retain_current_notices(|_| Some(&changed)));
        assert!(refusals.notice(7).is_none());

        refusals.note(7, RowNoticeKind::SelfInstance, &refused);
        assert!(refusals.retain_current_notices(|_| None));
        assert!(refusals.notices().is_empty());
    }

    #[test]
    fn a_profile_notice_names_the_profile_and_clears_on_connect() {
        let mut refusals = Refusals::default();
        refusals.note(3, RowNoticeKind::ProfileMissing, &binding("prod-web"));
        let notice = refusals.notice(3).expect("noted");
        assert_eq!(notice.target, "prod-web");
        assert_eq!(notice.kind, RowNoticeKind::ProfileMissing);
        refusals.clear_notice(3);
        assert!(refusals.notice(3).is_none());
    }

    #[test]
    fn notice_kinds_use_the_ipc_reason_names() {
        let names: Vec<_> = [
            RowNoticeKind::SelfInstance,
            RowNoticeKind::ProfileMissing,
            RowNoticeKind::Unresolved,
            RowNoticeKind::RemoteWorkspaceMissing,
            RowNoticeKind::Occupied,
            RowNoticeKind::MirrorChain,
        ]
        .iter()
        .map(|kind| serde_json::to_value(kind).unwrap())
        .collect();
        assert_eq!(
            names,
            serde_json::json!([
                "self_instance",
                "profile_missing",
                "unresolved",
                "remote_workspace_missing",
                "occupied",
                "mirror_workspace"
            ])
            .as_array()
            .unwrap()
            .clone()
        );
    }

    #[test]
    fn history_keeps_the_newest_refusals() {
        let mut refusals = Refusals::default();
        for attempt in 0..(REFUSAL_HISTORY as u64 + 3) {
            refusals.record(refusal(attempt, None));
        }
        let attempts: Vec<_> = refusals.recent().map(|r| r.attempt).collect();
        assert_eq!(attempts.len(), REFUSAL_HISTORY);
        assert_eq!(attempts.first(), Some(&3));
        assert_eq!(attempts.last(), Some(&(REFUSAL_HISTORY as u64 + 2)));
        assert_eq!(refusals.recent().next().unwrap().row_workspace, None);
    }
}
