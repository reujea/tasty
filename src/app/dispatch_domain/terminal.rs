//! 터미널 사건을 알림·셸 상태·호스트 효과로 전달한다.

use super::*;

impl App {
    pub(super) fn cascade_terminal_notification(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        title: String,
        body: String,
    ) {
        let Some(DispatchCtx {
            state,
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        if engine.runtime.settings.notification.enabled {
            let Some((workspace_index, _)) = engine.find_workspace_index_for_surface(surface_id)
            else {
                return;
            };
            let ws_id = engine
                .workspace_at(workspace_index)
                .expect("source workspace exists")
                .id;
            state.dispatch_intent(
                crate::app::command::DomainIntent::PushNotification {
                    level: crate::notification::NotificationLevel::Info,
                    ws_id,
                    surface_id,
                    title,
                    body,
                    source: "host".to_string(),
                    merge: crate::app::command::NotificationMerge::Coalesce,
                }
                .from_system(),
            );
        }
        let exec = self.services.hook_executor();
        for fired in engine
            .hooks
            .fire(&exec, surface_id, tasty_hooks::HookEvent::Notification)
        {
            engine.enqueue_host_event(fired);
        }
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    pub(super) fn cascade_terminal_bell_ring(&mut self, source: DispatchSource, surface_id: u32) {
        let Some(DispatchCtx {
            state,
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        // 사용자가 등록한 Bell 훅은 알림·벨 표시 설정을 꺼도 실행한다.
        if engine.runtime.settings.notification.enabled
            && engine.runtime.settings.general.bell_notification
        {
            let Some((workspace_index, _)) = engine.find_workspace_index_for_surface(surface_id)
            else {
                return;
            };
            let ws_id = engine
                .workspace_at(workspace_index)
                .expect("source workspace exists")
                .id;
            state.dispatch_intent(
                crate::app::command::DomainIntent::PushNotification {
                    level: crate::notification::NotificationLevel::Info,
                    ws_id,
                    surface_id,
                    title: crate::i18n::t("notification.bell_title").to_string(),
                    body: String::new(),
                    source: "host".to_string(),
                    merge: crate::app::command::NotificationMerge::Coalesce,
                }
                .from_system(),
            );
        }
        let exec = self.services.hook_executor();
        for fired in engine
            .hooks
            .fire(&exec, surface_id, tasty_hooks::HookEvent::Bell)
        {
            engine.enqueue_host_event(fired);
        }
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    /// 등록된 OutputMatch 훅은 알림 설정과 무관하게 확인한다.
    pub(super) fn cascade_terminal_output_match(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        text: String,
    ) {
        let Some(DispatchCtx {
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        let exec = self.services.hook_executor();
        for fired in engine
            .hooks
            .fire(&exec, surface_id, tasty_hooks::HookEvent::OutputMatch(text))
        {
            engine.enqueue_host_event(fired);
        }
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    pub(super) fn cascade_terminal_title_changed(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        title: String,
        generation: tasty_terminal::ResourceGeneration,
    ) {
        let Some(DispatchCtx {
            state,
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        engine.enqueue_host_event(crate::state::PendingHostEvent::SurfaceTitleChanged {
            surface_id,
            generation,
            title: title.clone(),
        });
        state.dispatch_intent(
            crate::app::command::DomainIntent::UpdateTabName {
                surface_id,
                generation,
                name: title,
            }
            .from_system(),
        );
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    pub(super) fn cascade_terminal_pty_cwd_changed(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    ) {
        let Some(DispatchCtx {
            state,
            engine: _,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        state.dispatch_intent(
            crate::app::command::DomainIntent::SurfaceCwdChanged {
                surface_id,
                generation,
            }
            .from_system(),
        );
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    /// OSC 133 완료는 attention과 등록된 완료 훅에 모두 전달한다.
    /// 별도 알림 패널 항목은 만들지 않으며 훅에는 실제 exit_code를 넘긴다.
    pub(super) fn cascade_terminal_command_completed(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        exit_code: Option<i32>,
    ) {
        let Some(DispatchCtx {
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        tracing::debug!(
            surface_id,
            exit_code = ?exit_code,
            "command completed — raising surface attention"
        );
        engine.raise_attention(surface_id, AttentionKind::Completion);
        engine.mark_layout_dirty();
        let exec = self.services.hook_executor();
        for fired in engine.hooks.fire(
            &exec,
            surface_id,
            tasty_hooks::HookEvent::CommandCompleted(exit_code),
        ) {
            engine.enqueue_host_event(fired);
        }
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    /// 셸 통합 미설치 추정을 surface마다 한 번 배너로 알릴 뿐 자동 설정이나 attention 변경은 하지 않는다.
    pub(super) fn cascade_terminal_shell_integration_hint(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
    ) {
        let Some(DispatchCtx {
            state,
            engine: _,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        if !state.take_first_shell_integration_hint(surface_id) {
            return;
        }
        state
            .banners
            .push(shell_integration_hint_banner(surface_id));
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }

    /// OSC 52 복사 안내만 표시한다. 클립보드 쓰기는 Core가 이미 처리했다.
    pub(super) fn cascade_terminal_clipboard_set(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        result: Result<(), crate::clipboard::error::ClipboardError>,
    ) {
        let Some(DispatchCtx {
            state,
            engine: _,
            view: _,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        state.clipboard_copy_feedback(
            result,
            "toast.copied_osc52",
            Vec::new(),
            crate::adapters::ui::ToastScope::Surface(surface_id),
        );
    }

    /// 종료 후 자원 정리·훅·알림은 공용 process_exit 처리로 전달한다.
    pub(super) fn cascade_terminal_process_exited(
        &mut self,
        source: DispatchSource,
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    ) {
        let Some(DispatchCtx {
            state,
            mut engine,
            view: dirty_main,
            ..
        }) = engines_mut!(self).resolve(source.engine())
        else {
            return;
        };
        super::super::process_exit::handle(
            &mut self.services,
            state,
            &mut engine,
            surface_id,
            generation,
        );
        if let Some(base) = dirty_main {
            base.state.dirty = true;
        }
    }
}

/// 셸 통합 안내 배너. 정의의 TTL을 그대로 따르므로 동작이 없는 이 배너는 스스로 닫힌다.
fn shell_integration_hint_banner(surface_id: u32) -> crate::adapters::ui::BannerState {
    crate::adapters::ui::BannerState::from_def(
        &crate::adapters::ui::banner::defs::SHELL_INTEGRATION_MISSING,
        crate::adapters::ui::BannerScope::Surface(surface_id),
    )
}

#[cfg(test)]
mod tests {
    use crate::adapters::ui::BannerScope;
    use crate::adapters::ui::banner::defs;

    #[test]
    fn shell_integration_hint_opens_a_ten_second_ttl_banner() {
        let banner = super::shell_integration_hint_banner(7);
        assert_eq!(banner.id, defs::BANNER_SHELL_INTEGRATION_MISSING);
        assert_eq!(banner.scope, BannerScope::Surface(7));
        assert_eq!(banner.ttl_ms, Some(10_000.0));
        assert_eq!(banner.remaining_ms, 10_000.0);
    }
}
