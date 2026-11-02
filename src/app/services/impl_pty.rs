//! PTY 출력을 읽고 터미널 이벤트·크기 변경을 처리한다.

use super::*;
use crate::runtime::engine_access::EngineMut;

impl AppServices {
    /// 지정 터미널의 출력을 읽고 engine에 쌓인 터미널 이벤트를 처리한다. GUI·헤드리스가 함께 사용한다.
    pub(crate) fn process_pty_output(
        &mut self,
        engine: &mut EngineMut<'_>,
        surface_id: u32,
    ) -> ProcessPtyOutcome {
        engine.process_surface(surface_id);
        let events = self.drain_terminal_events(engine);
        ProcessPtyOutcome { events }
    }

    pub(crate) fn process_all_pty_output(
        &mut self,
        engine: &mut EngineMut<'_>,
    ) -> ProcessPtyOutcome {
        engine.process_all();
        let events = self.drain_terminal_events(engine);
        ProcessPtyOutcome { events }
    }

    /// observer·명령 이력·클립보드는 여기서 처리하고 App 후속 처리가 필요한 이벤트를 반환한다.
    fn drain_terminal_events(&mut self, engine: &mut EngineMut<'_>) -> Vec<CoreEvent> {
        use tasty_terminal::TerminalEventKind;
        let raw = engine.collect_events();
        let mut out = Vec::with_capacity(raw.len());
        // 앞 회차에 넣은 명령 기록의 한도 판정을 거둔다.
        Self::push_command_cap_notices(engine, &mut out);
        for ev in raw {
            let sid = ev.surface_id;
            let generation = ev.generation;
            if !engine.runtime.terminals.matches_generation(sid, generation) {
                continue;
            }
            match ev.kind {
                TerminalEventKind::OutputAppended { text } => {
                    self.handle_output_appended(engine, sid, generation, &text, &mut out);
                }
                TerminalEventKind::PromptBoundary { phase, payload } => {
                    self.handle_prompt_boundary(engine, sid, generation, phase, &payload, &mut out);
                }
                TerminalEventKind::ClipboardSet(text) => {
                    let result = self.clipboard.write_text(&text).map_err(|error| {
                        // Keep clipboard contents out of diagnostics, including custom adapter errors.
                        tracing::warn!("OSC 52 clipboard write failed");
                        error
                            .downcast_ref::<crate::clipboard::error::ClipboardError>()
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::clipboard::error::ClipboardError::TransferFailed(
                                    error.to_string(),
                                )
                            })
                    });
                    out.push(CoreEvent::TerminalClipboardSet {
                        result,
                        generation,
                        surface_id: sid,
                    });
                }
                TerminalEventKind::ClipboardQuery => {
                    self.handle_clipboard_query(engine, sid, generation);
                }
                TerminalEventKind::Notification { title, body } => {
                    out.push(CoreEvent::TerminalNotification {
                        generation,
                        surface_id: sid,
                        title,
                        body,
                    });
                }
                TerminalEventKind::BellRing => {
                    out.push(CoreEvent::TerminalBellRing {
                        generation,
                        surface_id: sid,
                    });
                }
                TerminalEventKind::TitleChanged(title) => {
                    out.push(CoreEvent::TerminalTitleChanged {
                        generation,
                        surface_id: sid,
                        title,
                    });
                }
                TerminalEventKind::CwdChanged(_cwd) => {
                    out.push(CoreEvent::TerminalCwdChanged {
                        generation,
                        surface_id: sid,
                    });
                }
                TerminalEventKind::ProcessExited => {
                    out.push(CoreEvent::TerminalProcessExited {
                        generation,
                        surface_id: sid,
                    });
                }
            }
        }
        out
    }

    fn handle_output_appended(
        &mut self,
        engine: &mut EngineMut<'_>,
        sid: u32,
        generation: tasty_terminal::ResourceGeneration,
        text: &str,
        out: &mut Vec<CoreEvent>,
    ) {
        let completed_lines = engine.observer_router.dispatch_text(sid, text);
        // 청크 경계와 무관하게 완성된 줄만 OutputMatch에 넘긴다.
        if engine.hooks.has_output_match_hook(sid) {
            for line in completed_lines {
                out.push(CoreEvent::TerminalOutputMatch {
                    generation,
                    surface_id: sid,
                    text: line,
                });
            }
        }
    }

    /// 끝난 명령 기록의 한도 알림을 연결이 그대로인 터미널에만 낸다.
    fn push_command_cap_notices(engine: &mut EngineMut<'_>, out: &mut Vec<CoreEvent>) {
        use crate::core::command_index::CommandCapEvent;
        for (sid, notice) in engine.live.command_index.take_cap_notices() {
            if !engine
                .runtime
                .terminals
                .matches_generation(sid, notice.generation)
            {
                continue;
            }
            let (title, body) = match notice.event {
                CommandCapEvent::SoftWarn { count, .. } => (
                    crate::i18n::t("command_index.cap.soft.title").to_string(),
                    crate::i18n::t_fmt("command_index.cap.soft.body", &count.to_string()),
                ),
                CommandCapEvent::HardBlocked { .. } => (
                    crate::i18n::t("command_index.cap.hard.title").to_string(),
                    crate::i18n::t("command_index.cap.hard.body").to_string(),
                ),
            };
            out.push(CoreEvent::TerminalNotification {
                generation: notice.generation,
                surface_id: sid,
                title,
                body,
            });
        }
    }

    fn handle_prompt_boundary(
        &mut self,
        engine: &mut EngineMut<'_>,
        sid: u32,
        generation: tasty_terminal::ResourceGeneration,
        phase: char,
        payload: &str,
        out: &mut Vec<CoreEvent>,
    ) {
        engine.note_prompt_boundary_seen(sid);
        engine.live.command_index.on_boundary(
            &engine.runtime.memory_access,
            sid,
            generation,
            phase,
            payload,
        );
        if phase == 'A' {
            engine.note_prompt_shown(sid, generation.value());
        }
        // 명령 이력 저장 성공 여부와 무관하게 D 경계는 완료 후속 처리로 넘긴다. 첫 프롬프트 전의 D는
        // 셸 시작 보고라 완료 attention·command-completed 훅을 일으키지 않는다.
        if phase == 'D' && engine.take_command_completed(sid, generation.value()) {
            let exit_code = crate::core::command_index::extract_exit_code(payload);
            out.push(CoreEvent::TerminalCommandCompleted {
                generation,
                surface_id: sid,
                exit_code,
            });
        }
    }

    /// resize 제한 주기에 따라 반영하고 미처리 요청이 남았는지 반환한다.
    #[cfg(feature = "gui")]
    pub(crate) fn flush_pty_resizes(engine: &mut EngineMut<'_>) -> bool {
        engine.flush_all_pty_resizes()
    }

    /// 레이아웃의 목표 크기를 Terminal store에 적용한다. 탭 바 높이는 창 계층이 값으로 넘긴다.
    #[cfg(feature = "gui")]
    pub(crate) fn resize_terminals(engine: &mut EngineMut<'_>, targets: Vec<(u32, usize, usize)>) {
        for (sid, cols, rows) in targets {
            // 점유 client가 정한 크기를 서버 창 레이아웃으로 덮지 않는다.
            if engine.live.occupancy.is_hard_occupied(sid) {
                continue;
            }
            if let Some(t) = engine.runtime.terminals.get(sid) {
                // mirror에 먼저 크기를 적용하면 서버의 reflow 전 출력과 어긋날 수 있다.
                // 서버에 resize를 요청하고 echo를 받아 로컬 크기를 바꾼다.
                if engine.runtime.terminals.pty(sid).is_none() {
                    if t.cols() != cols || t.rows() != rows {
                        engine
                            .remote
                            .pending_resize_forward
                            .insert(sid, (cols, rows));
                    }
                    continue;
                }
                engine.runtime.terminals.resize(sid, cols, rows);
            }
        }
    }

    /// busy 집합이 바뀌었는지 반환해 창의 다시 그리기 여부를 정한다.
    #[cfg(feature = "gui")]
    pub(crate) fn update_busy_surfaces(engine: &mut EngineMut<'_>) -> bool {
        engine.refresh_busy_surfaces()
    }
}

#[cfg(test)]
#[path = "impl_pty_tests.rs"]
mod tests;
