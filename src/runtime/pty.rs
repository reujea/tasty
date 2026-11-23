use std::time::Instant;

use crate::notification::NotificationLevel;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::terminal_store::PtyReclaimReason;
#[cfg(all(test, unix))]
use tasty_terminal::Terminal;
use tasty_terminal::TerminalEvent;

impl crate::runtime::engine_access::EngineRef<'_> {
    pub fn is_surface_deferred(&self, surface: u32) -> bool {
        self.runtime.surfaces.get(&surface).is_some_and(|surface| {
            surface
                .as_any()
                .is::<crate::runtime::surface_restorer::JournalPlaceholder>()
        })
    }
    pub(crate) fn find_mesh_surface_info(&self, surface: u32) -> Option<(String, String)> {
        self.runtime
            .surfaces
            .get(&surface)?
            .attach_mesh_info()
            .map(|(kind, plugin)| (kind.into(), plugin.into()))
    }
}

impl EngineMut<'_> {
    /// 정책상 회수할 standalone 항목을 지우고 Terminal·waker 기록도 함께 정리한다.
    /// 실행 중인 항목은 호출자가 spawn에서 유휴 제한을 고른 경우에만 회수한다.
    /// 실제 자식 종료·회수는 같은 항목에 있던 Pty owner의 플랫폼별 Drop이 수행한다.
    /// 회수 기록과 알림까지 여기서 끝내므로 호출 경로마다 결과 전달이 갈라지지 않는다.
    /// 반환값은 회수한 항목 수다.
    pub(crate) fn reclaim_standalone_ptys(&mut self, now: Instant) -> usize {
        let due = self.runtime.terminals.due_standalone_reclaims(now);
        for &(pty_id, reason) in &due {
            self.runtime.terminals.remove(pty_id);
            if let Some(factory) = self.runtime.waker_factory.as_ref() {
                factory.forget_surface(pty_id);
            }
            self.runtime.terminals.record_reclaim(pty_id, reason);
            tracing::info!("{}", reason.describe(pty_id));
        }
        self.notify_pty_reclaims(&due);
        due.len()
    }

    /// 실행 중 프로세스를 끝낸 회수는 경고, 끝난 항목 정리는 정보로 묶어 한 번씩 알린다.
    fn notify_pty_reclaims(&mut self, due: &[(u32, PtyReclaimReason)]) {
        let join = |idle: bool| {
            due.iter()
                .filter(|(_, reason)| {
                    matches!(reason, PtyReclaimReason::IdleTimeout { .. }) == idle
                })
                .map(|(id, _)| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let batches = [
            (
                join(true),
                NotificationLevel::Warning,
                "notification.pty_idle_timeout_title",
                "notification.pty_idle_timeout_body",
            ),
            (
                join(false),
                NotificationLevel::Info,
                "notification.pty_exited_removed_title",
                "notification.pty_exited_removed_body",
            ),
        ];
        for (ids, level, title_key, body_key) in batches {
            if ids.is_empty() {
                continue;
            }
            let title = crate::i18n::t(title_key).to_string();
            let body = crate::i18n::t_fmt(body_key, &ids);
            self.push_host_notification(level, title, body);
        }
    }

    /// 트리는 유지하고 store의 Terminal을 교체한 뒤 기존 Terminal을 drop한다.
    /// 기존 ID가 없으면 새 Terminal을 등록한 상태에서 Err를 반환한다.
    #[cfg(all(test, unix))]
    pub fn replace_terminal_by_id(
        &mut self,
        surface_id: u32,
        new_terminal: (Terminal, tasty_terminal::Pty),
    ) -> anyhow::Result<()> {
        if let Some(old) =
            self.runtime
                .terminals
                .replace(surface_id, new_terminal.0, Some(new_terminal.1))
        {
            drop(old);
            return Ok(());
        }
        anyhow::bail!("Surface {} not found", surface_id)
    }

    /// OutputAppended 발생 여부를 현재 observer 목록에 맞춘다. 생성 직후만 설정하면 등록 변경을 놓친다.
    pub(crate) fn sync_output_event_gates(&mut self) {
        let router = &self.observer_router;
        let hooks = &self.hooks;
        for (sid, t) in self.runtime.terminals.iter_mut() {
            t.set_output_events_enabled(router.wants(sid) || hooks.has_output_match_hook(sid));
        }
    }

    /// 모든 터미널을 처리하고 읽은 출력이 있던 surface ID를 돌려준다.
    pub fn process_all(&mut self) -> Vec<u32> {
        self.sync_output_event_gates();
        self.runtime.terminals.process_all()
    }

    /// Windows 절전 복귀 후 살아 있는 자식에 wake를 시도하고 비셸 전경 surface를 알림 후보로 반환한다.
    /// 실제 응답 회복을 확인한 결과는 아니다.
    #[cfg(all(windows, feature = "gui"))]
    pub(crate) fn wake_terminals_after_resume(&mut self) -> Vec<u32> {
        let mut suspects = Vec::new();
        let ids: Vec<_> = self.runtime.terminals.iter().map(|(sid, _)| sid).collect();
        for sid in ids {
            let (cols, rows) = self
                .runtime
                .terminals
                .get(sid)
                .expect("collected ID")
                .dimensions();
            let Some(pty) = self.runtime.terminals.pty_mut(sid) else {
                continue;
            };
            if !pty.check_alive() {
                continue; // 죽음 — process_all 의 ProcessExited cascade 가 정리.
            }
            if let Err(error) = self.runtime.terminals.nudge_os_size(sid, cols, rows) {
                tracing::warn!("wake_nudge PTY resize failed: {error}");
            }
            let pty = self.runtime.terminals.pty_mut(sid).expect("checked above");
            let shell_pid = pty.process_id();
            if let Some(info) = pty.foreground_process_info()
                && Some(info.pid) != shell_pid
                && !tasty_terminal::foreground_process::is_known_shell_name(&info.name)
            {
                suspects.push(sid);
            }
        }
        suspects
    }

    pub fn process_surface(&mut self, surface_id: u32) -> bool {
        let enabled =
            self.observer_router.wants(surface_id) || self.hooks.has_output_match_hook(surface_id);
        if let Some(t) = self.runtime.terminals.get_mut(surface_id) {
            t.set_output_events_enabled(enabled);
        }
        self.runtime.terminals.process_surface(surface_id)
    }

    #[cfg(feature = "gui")]
    pub fn flush_all_pty_resizes(&mut self) -> bool {
        self.runtime.terminals.flush_pty_resizes()
    }

    /// 락을 즉시 얻지 못한 Terminal은 이번 수집에서 건너뛴다.
    /// 그 이벤트는 큐에 남지만 여기서 다음 호출 시각이나 전달 성공까지 보장하지는 않는다.
    pub fn collect_events(&mut self) -> Vec<TerminalEvent> {
        let mut all_events = Vec::new();
        for (sid, terminal) in self.runtime.terminals.iter_mut() {
            let Some(mut events) = terminal.try_take_events() else {
                continue;
            };
            for event in &mut events {
                event.surface_id = sid;
            }
            all_events.extend(events);
        }
        all_events
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    // 이유: 현재 호출자가 없으며 터미널 ID 조회 API를 유지한다.
    #[allow(dead_code)]
    pub fn all_terminal_surface_ids(&mut self) -> Vec<u32> {
        self.runtime.terminals.iter().map(|(id, _)| id).collect()
    }
}

impl EngineMut<'_> {
    pub fn mark_layout_dirty(&mut self) {
        self.persistence.dirty.mark_dirty();
    }
}
