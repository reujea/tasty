//! 에이전트가 일으킨 mirror 왕복의 결과를 사용자 toast 에서 떼어 낸다(identity 원칙 1,
//! `docs/adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md`).
//!
//! 원격 회신은 op_id · request_id 만 싣고 누가 요청했는지는 안 싣는다. 그래서 송신할 때 에이전트
//! 요청의 id 를 세션에 기억해 두고, 회신이 오면 그 id 로 가른다 — 에이전트 요청이면 로그,
//! 아니면 종전대로 toast.

use std::collections::HashSet;

use super::{AttachClientSession, MirrorHost};

/// 에이전트가 건 요청 중 회신을 기다리는 것의 id. 송신 때 채우고, 회신(성공이든 실패든)이
/// 오면 꺼내며, 재연결 때 비운다.
#[derive(Debug, Default)]
pub(super) struct AgentRequests {
    /// forward 한 구조 op 의 op_id(`PendingStructuralForward::silent_failure`).
    structural: HashSet<u64>,
    /// 원격 markdown 원문 요청의 request_id(`PendingMarkdownContentForward::agent_origin`).
    markdown: HashSet<u64>,
}

impl AgentRequests {
    /// 송신 때 부른다 — 에이전트 op(`silent_failure`)면 기억한다.
    pub(super) fn note_structural_from(
        &mut self,
        pending: &crate::core::PendingStructuralForward,
        op_id: u64,
    ) {
        if pending.silent_failure {
            self.structural.insert(op_id);
        }
    }

    /// 성공 회신에서 부른다 — 표시만 지운다.
    pub(super) fn forget_structural(&mut self, op_id: u64) {
        self.structural.remove(&op_id);
    }

    /// 송신에 성공한 뒤 부른다 — 에이전트 요청(`agent_origin`)이면 기억한다.
    pub(super) fn note_markdown_from(
        &mut self,
        req: &crate::core::PendingMarkdownContentForward,
        request_id: u64,
    ) {
        if req.agent_origin {
            self.markdown.insert(request_id);
        }
    }

    /// 회신에서 부른다 — 에이전트 요청이었으면 `true` 이고 표시는 지워진다.
    pub(super) fn take_markdown(&mut self, request_id: u64) -> bool {
        self.markdown.remove(&request_id)
    }

    pub(super) fn clear(&mut self) {
        self.structural.clear();
        self.markdown.clear();
    }
}

/// forward 한 구조 op 가 원격에서 실패(예: 미등록 kind)했다는 회신을 적용한다.
/// 사용자에게 실패 toast. 로컬/원격 어느 쪽도 구조 변경 없음(요청/응답).
///
/// "원격에 복원할 항목이 없다" 는 **실패가 아니다** — 아래 일반 문구
/// ("적용하지 못했습니다")로 내보내면 오류로 읽힌다. 서버가 전용 sentinel
/// (`STRUCTURAL_REASON_RESTORE_EMPTY`)로 그 경우를 표시하고 여기서 다른
/// 문구를 쓴다(ADR-0264 결정 2).
///
/// 에이전트 발화 op 의 실패는 사용자 toast 로 내지 않는다 — 에이전트 행동의
/// 결과이지 연결 상태 사건이 아니다.
pub(super) fn apply_structural_failed(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_>,
    op_id: u64,
    reason: Option<String>,
) {
    let reason = reason.unwrap_or_default();
    if sess.agent_requests.structural.remove(&op_id) {
        tracing::warn!(
            "structural forward op {op_id} (agent origin) failed on the remote: {reason}"
        );
        return;
    }
    if reason == tasty_ipc::stream::STRUCTURAL_REASON_RESTORE_EMPTY {
        host.toast(
            crate::i18n::t("attach.toast.mirror_restore_empty").to_string(),
            crate::adapters::ui::ToastKind::Info,
        );
        return;
    }
    let base = crate::i18n::t("attach.toast.mirror_structural_forward_failed");
    let msg: String = if reason.is_empty() {
        base.to_string()
    } else {
        format!("{base} ({reason})")
    };
    host.toast(msg, crate::adapters::ui::ToastKind::Warning);
}

/// 원격 markdown 원문이 잘려 왔음을 알린다 — 문서 본문에 "여기서 잘렸다" 를 심지 않고
/// toast 로 알린다(ADR-0255 항목 3). 에이전트가 건 요청의 회신이면 toast 대신 로그다.
pub(super) fn notify_markdown_truncated(host: &mut MirrorHost<'_>, local: u32, agent_origin: bool) {
    if agent_origin {
        tracing::info!(
            "markdown mirror surface {local}: agent-requested content arrived truncated (no toast)"
        );
        return;
    }
    host.toast(
        crate::i18n::t("attach.toast.mirror_markdown_truncated").to_string(),
        crate::adapters::ui::ToastKind::Warning,
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::tests::test_session;
    use super::super::{MirrorEvent, apply_mirror_events};
    use super::*;

    /// 송신 때 쌓이는 큐 원소 — `silent_failure` 만 가른다.
    fn structural(silent_failure: bool) -> crate::core::PendingStructuralForward {
        crate::core::PendingStructuralForward {
            op: tasty_ipc::stream::StructuralOp::NewTab {
                anchor_surface_id: 1,
                surface_kind: "terminal".to_string(),
                params: serde_json::Value::Null,
            },
            user_triggered: false,
            close_focus_candidates: Vec::new(),
            silent_failure,
        }
    }

    fn markdown(request_id: u64, agent_origin: bool) -> crate::core::PendingMarkdownContentForward {
        crate::core::PendingMarkdownContentForward {
            local_surface_id: 300,
            request_id,
            agent_origin,
        }
    }

    /// 에이전트 발화로 forward 한 op 의 원격 실패는 사용자 toast 를 내지 않고, 회신과
    /// 함께 표시가 지워진다. 표시가 없는 op 의 실패는 종전대로 toast 를 낸다.
    ///
    /// 세션 기록은 송신 쪽이 큐 원소를 그대로 넘겨 채운다 — `note_structural_from` 이 원소의
    /// `silent_failure` 를 안 읽는 변이, `apply_structural_failed` 의 에이전트 가드를 지우는
    /// 변이 어느 쪽에서도 실패해야 한다.
    #[test]
    fn an_agent_forward_failure_does_not_toast() {
        let mut sess = test_session(9_000, HashMap::new());
        sess.agent_requests
            .note_structural_from(&structural(true), 5);
        sess.agent_requests
            .note_structural_from(&structural(false), 6);
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        let (mut state, mut engine) = crate::state::tests::test_state();
        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![MirrorEvent::StructuralFailed(5, Some("nope".to_string()))],
            );
        }
        assert_eq!(state.toasts.len(), 0, "에이전트 op 의 실패는 로그로 끝난다");
        assert!(
            sess.agent_requests.structural.is_empty(),
            "회신이 오면 표시를 지운다"
        );

        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![MirrorEvent::StructuralFailed(6, Some("nope".to_string()))],
            );
        }
        assert_eq!(
            state.toasts.len(),
            1,
            "표시 없는 op 의 실패는 toast 를 낸다"
        );
    }

    /// 에이전트가 건 원문 요청(`markdown.reload`)의 회신은 잘려 와도 사용자 toast 를 내지
    /// 않고, 회신과 함께 표시가 지워진다. plugin 자신의 요청은 종전대로 toast 를 낸다.
    ///
    /// 세션 기록은 송신 쪽이 요청 원소를 그대로 넘겨 채운다 — `note_markdown_from` 이 원소의
    /// `agent_origin` 을 안 읽는 변이, `notify_markdown_truncated` 의 `agent_origin` 분기를
    /// 지우는 변이 어느 쪽에서도 실패해야 한다.
    #[test]
    fn an_agent_markdown_reload_truncation_does_not_toast() {
        let mut sess = test_session(9_000, HashMap::from([(30, 300)]));
        sess.markdown_locals.insert(300);
        sess.agent_requests
            .note_markdown_from(&markdown(11, true), 11);
        sess.agent_requests
            .note_markdown_from(&markdown(12, false), 12);
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        let (mut state, mut engine) = crate::state::tests::test_state();
        let truncated = |request_id| MirrorEvent::MarkdownContentResult {
            request_id,
            surface_id: 30,
            ok: true,
            file: Some("/r/a.md".to_string()),
            source: Some("# a".to_string()),
            truncated: true,
            reason: None,
        };
        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![truncated(11)],
            );
        }
        assert_eq!(
            state.toasts.len(),
            0,
            "에이전트 요청의 잘림은 toast 를 안 낸다"
        );
        assert!(
            sess.agent_requests.markdown.is_empty(),
            "회신이 오면 표시를 지운다"
        );

        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine);
            apply_mirror_events(
                &mut sess,
                &mut host,
                &mut plugin_manager,
                vec![truncated(12)],
            );
        }
        assert_eq!(
            state.toasts.len(),
            1,
            "plugin 자신의 요청은 종전대로 toast 를 낸다"
        );
    }
}
