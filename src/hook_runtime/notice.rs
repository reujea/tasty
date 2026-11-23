//! 자동으로 실행한 surface 훅·전역 훅·웹훅의 실패를 알림 채널로 남긴다.
//! 수동 dispatch는 알림 대신 호출자가 조회하는 결과로 받는다.
//!
//! 레벨: 자동화가 전혀 실행되지 않았거나 모든 스텝이 실패하면 오류, 일부 스텝만 실패했거나
//! 결과를 모르는 스텝이 있으면 경고다. 웹훅 등록 파일 저장 실패는 경고다.

use crate::hook_handler::outcome::{SequenceOutcome, Verdict};
use crate::notification::NotificationLevel;
use crate::system_notice::{SystemNotice, post};

/// 실패한 자동 실행의 출처. 알림의 source와 제목을 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoOrigin {
    /// 발생 surface가 알림을 받는다.
    SurfaceHook {
        surface_id: u32,
    },
    GlobalHook,
    Webhook,
}

impl AutoOrigin {
    pub(crate) fn source(self) -> &'static str {
        match self {
            Self::SurfaceHook { .. } => "hook",
            Self::GlobalHook => "global_hook",
            Self::Webhook => "webhook",
        }
    }

    fn surface_id(self) -> u32 {
        match self {
            Self::SurfaceHook { surface_id } => surface_id,
            Self::GlobalHook | Self::Webhook => 0,
        }
    }

    fn title(self, verdict: Verdict) -> &'static str {
        let key = match (self, verdict) {
            (Self::GlobalHook, Verdict::Failed) => "hook_notice.global_failed",
            (Self::SurfaceHook { .. }, Verdict::Failed) => "hook_notice.hook_failed",
            (Self::Webhook, Verdict::Failed) => "hook_notice.webhook_failed",
            (Self::Webhook, Verdict::Unknown) => "hook_notice.webhook_unknown",
            (Self::Webhook, _) => "hook_notice.webhook_partial",
            (_, Verdict::Unknown) => "hook_notice.hook_unknown",
            _ => "hook_notice.hook_partial",
        };
        crate::i18n::t(key)
    }

    fn notice(self, verdict: Verdict, body: String) -> SystemNotice {
        let level = if verdict == Verdict::Failed {
            NotificationLevel::Error
        } else {
            NotificationLevel::Warning
        };
        SystemNotice::new(level, self.source(), self.title(verdict).to_string(), body)
            .on_surface(self.surface_id())
    }
}

/// 자동화가 실행되지 않은 실패. `body`는 원인을 담은 번역 문구다.
pub(crate) fn not_run(origin: AutoOrigin, body: String) {
    post(origin.notice(Verdict::Failed, body));
}

/// 셸 작업을 실행기에 넣지 못했다. 대기열이 찼으면 새 문구, 실행기 스레드를 못 만들었으면 기존 시작 실패 문구다.
pub(crate) fn shell_not_queued(
    origin: AutoOrigin,
    command: &str,
    reason: &super::shell_pool::ShellNotQueued,
) {
    post(shell_not_queued_notice(origin, command, reason));
}

fn shell_not_queued_notice(
    origin: AutoOrigin,
    command: &str,
    reason: &super::shell_pool::ShellNotQueued,
) -> SystemNotice {
    use super::shell_pool::{ShellNotQueued, WAITING_LIMIT};
    let body = match reason {
        ShellNotQueued::Full => crate::i18n::t_fmt2(
            "hook_notice.shell_not_queued",
            command,
            &WAITING_LIMIT.to_string(),
        ),
        ShellNotQueued::WorkerUnavailable(e) => {
            crate::i18n::t_fmt2("hook_notice.spawn_failed", command, e)
        }
    };
    origin.notice(Verdict::Failed, body)
}

/// 시퀀스를 실행했지만 성공하지 못한 스텝이 있으면 남긴다. 모두 성공했으면 아무것도 남기지 않는다.
pub(crate) fn sequence(origin: AutoOrigin, label: &str, outcome: &SequenceOutcome) {
    if let Some(notice) = sequence_notice(origin, label, outcome) {
        post(notice);
    }
}

fn sequence_notice(
    origin: AutoOrigin,
    label: &str,
    outcome: &SequenceOutcome,
) -> Option<SystemNotice> {
    let verdict = outcome.verdict();
    if verdict == Verdict::Succeeded {
        return None;
    }
    let body = crate::i18n::t_fmt2("hook_notice.sequence", label, &outcome.failure_summary());
    Some(origin.notice(verdict, body))
}

/// 응답할 호출자가 없는 웹훅 파일 저장 실패.
pub(crate) fn webhook_save_failed(error: &str) {
    post(SystemNotice::warning(
        AutoOrigin::Webhook.source(),
        crate::i18n::t("hook_notice.webhook_save_title").to_string(),
        crate::i18n::t_fmt("hook_notice.webhook_save", error),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_handler::outcome::{StepResult, StepStatus};

    fn outcome(statuses: Vec<StepStatus>) -> SequenceOutcome {
        SequenceOutcome {
            steps: statuses
                .into_iter()
                .enumerate()
                .map(|(index, status)| StepResult {
                    index,
                    method: format!("m{index}"),
                    status,
                })
                .collect(),
        }
    }

    #[test]
    fn levels_follow_how_much_of_the_automation_ran() {
        let surface = AutoOrigin::SurfaceHook { surface_id: 7 };
        let fail = || StepStatus::Failed("rpc error".into());
        assert!(sequence_notice(surface, "h", &outcome(vec![StepStatus::Ok])).is_none());
        let all = sequence_notice(surface, "h", &outcome(vec![fail()])).unwrap();
        assert_eq!(all.level, NotificationLevel::Error);
        assert_eq!(all.surface_id, 7);
        assert_eq!(all.source, "hook");
        let some = sequence_notice(
            AutoOrigin::Webhook,
            "w",
            &outcome(vec![StepStatus::Ok, fail()]),
        )
        .unwrap();
        assert_eq!(some.level, NotificationLevel::Warning);
        assert_eq!(some.surface_id, 0);
        assert_eq!(some.source, "webhook");
        assert!(some.body.contains("m1"), "{}", some.body);
    }

    /// 대기열이 차서 실행하지 않은 셸 작업은 surface 훅이면 그 surface에, 전역 훅이면 활성 workspace에
    /// 오류로 남고, 본문은 대기열 문구다.
    #[test]
    fn a_refused_shell_job_is_an_error_from_its_origin() {
        use crate::hook_runtime::shell_pool::{ShellNotQueued, WAITING_LIMIT};
        let expected = crate::i18n::t_fmt2(
            "hook_notice.shell_not_queued",
            "echo hi",
            &WAITING_LIMIT.to_string(),
        );
        let surface = shell_not_queued_notice(
            AutoOrigin::SurfaceHook { surface_id: 9 },
            "echo hi",
            &ShellNotQueued::Full,
        );
        assert_eq!(surface.level, NotificationLevel::Error);
        assert_eq!((surface.source, surface.surface_id), ("hook", 9));
        assert_eq!(surface.title, crate::i18n::t("hook_notice.hook_failed"));
        assert_eq!(surface.body, expected);
        let global =
            shell_not_queued_notice(AutoOrigin::GlobalHook, "echo hi", &ShellNotQueued::Full);
        assert_eq!((global.source, global.surface_id), ("global_hook", 0));
        assert_eq!(global.title, crate::i18n::t("hook_notice.global_failed"));
        assert_eq!(global.body, expected);
        let thread = shell_not_queued_notice(
            AutoOrigin::GlobalHook,
            "echo hi",
            &ShellNotQueued::WorkerUnavailable("boom".into()),
        );
        assert_eq!(
            thread.body,
            crate::i18n::t_fmt2("hook_notice.spawn_failed", "echo hi", "boom")
        );
    }
}
