//! `Core` — PTY 파이프라인(system loop wrapper). `src/core/mod.rs` 의 `impl Core` 분할.

use super::*;

/// Helper: layout / tab_bar 기반으로 surface_id 별 *목표 grid (cols, rows)* 를
/// 수집. 본 helper 는 read-only 로 workspaces 만 순회한다 — `terminals` store 에는
/// 직접 접근하지 않으므로 caller 가 결과를 받아 `engine.terminals.get_mut` 로
/// resize 호출할 때 borrow 충돌이 없다.
#[cfg(feature = "gui")]
fn collect_terminal_resize_targets(
    tab_bar_h: crate::model::PhysicalPx,
    engine: &crate::core::CoreState,
    terminal_rect: crate::model::PhysicalRect,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f32,
) -> Vec<(u32, usize, usize)> {
    let mut out = Vec::new();
    for ws in &engine.workspaces {
        let pane_rects = ws.pane_layout().compute_rects(terminal_rect, scale_factor);
        for (pane_id, pane_rect) in pane_rects {
            let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                continue;
            };
            let content_rect = crate::model::PhysicalRect {
                x: pane_rect.x,
                y: pane_rect.y + tab_bar_h,
                width: pane_rect.width,
                height: (pane_rect.height - tab_bar_h).max(crate::model::PhysicalPx(1.0)),
            };
            for tab in &pane.tabs {
                let Some(layout) = tab.layout_opt.as_ref() else {
                    continue;
                };
                for (sid, rect) in layout.compute_rects(content_rect, scale_factor) {
                    let cols = ((rect.width.value() / cell_width.max(1.0)).floor() as usize).max(1);
                    let rows =
                        ((rect.height.value() / cell_height.max(1.0)).floor() as usize).max(1);
                    out.push((sid, cols, rows));
                }
            }
        }
    }
    out
}

impl Core {
    /// 특정 surface 의 PTY 출력 drain + TerminalEvent → CoreEvent 변환.
    /// observer_router (OutputAppended) / command_index (PromptBoundary) /
    /// 시스템 clipboard (OSC 52) 의 부수효과는 본 함수가 직접 처리. 나머지
    /// terminal event 는 outcome.events 로 cascade dispatcher 에 전달.
    // headless 도 이 함수를 쓴다 — `boot::handle_terminal_output` 이 targeted wake
    // (`AppEvent::TerminalOutput(Some(sid))`)를 이 경로로 보낸다. gui 의 targeted
    // polling 전용이라는 서술이 한동안 붙어 있었으나 사실이 아니었다.
    pub(crate) fn process_pty_output(
        &mut self,
        engine: &mut crate::core::CoreState,
        surface_id: u32,
    ) -> ProcessPtyOutcome {
        engine.process_surface(surface_id);
        let events = self.drain_terminal_events(engine);
        ProcessPtyOutcome { events }
    }

    /// 모든 workspace 의 모든 terminal 을 drain + 변환. 반환: cascade 가 처리할
    /// CoreEvent 목록.
    pub(crate) fn process_all_pty_output(
        &mut self,
        engine: &mut crate::core::CoreState,
    ) -> ProcessPtyOutcome {
        engine.process_all();
        let events = self.drain_terminal_events(engine);
        ProcessPtyOutcome { events }
    }

    /// `engine.collect_events()` 결과를 CoreEvent 로 변환. observer_router /
    /// command_index / system clipboard 의 *직접 부수효과* 는 본 함수가 처리하고,
    /// cascade 가 필요한 event 만 Vec<CoreEvent> 로 반환.
    fn drain_terminal_events(&mut self, engine: &mut crate::core::CoreState) -> Vec<CoreEvent> {
        use tasty_terminal::TerminalEventKind;
        let raw = engine.collect_events();
        let mut out = Vec::with_capacity(raw.len());
        for ev in raw {
            let sid = ev.surface_id;
            match ev.kind {
                TerminalEventKind::OutputAppended { text } => {
                    self.handle_output_appended(engine, sid, &text, &mut out);
                }
                TerminalEventKind::PromptBoundary { phase, payload } => {
                    self.handle_prompt_boundary(engine, sid, phase, &payload, &mut out);
                }
                TerminalEventKind::ClipboardSet(text) => {
                    if let Err(e) = self.clipboard.write_text(&text) {
                        tracing::warn!("OSC 52 clipboard write failed: {e}");
                    }
                    out.push(CoreEvent::TerminalClipboardSet { surface_id: sid });
                }
                TerminalEventKind::ClipboardQuery => {
                    self.handle_clipboard_query(engine, sid);
                }
                TerminalEventKind::Notification { title, body } => {
                    out.push(CoreEvent::TerminalNotification {
                        surface_id: sid,
                        title,
                        body,
                    });
                }
                TerminalEventKind::BellRing => {
                    out.push(CoreEvent::TerminalBellRing { surface_id: sid });
                }
                TerminalEventKind::TitleChanged(title) => {
                    out.push(CoreEvent::TerminalTitleChanged {
                        surface_id: sid,
                        title,
                    });
                }
                TerminalEventKind::CwdChanged(_cwd) => {
                    out.push(CoreEvent::TerminalCwdChanged { surface_id: sid });
                }
                TerminalEventKind::ProcessExited => {
                    out.push(CoreEvent::TerminalProcessExited { surface_id: sid });
                }
            }
        }
        out
    }

    /// PTY 출력 청크 도착 — observer_router 라인 버퍼에 먹이고, OutputMatch 훅
    /// (완성된 라인 단위 매칭) + OSC 133 셸 통합 미설치 힌트를 발화한다.
    fn handle_output_appended(
        &mut self,
        engine: &mut crate::core::CoreState,
        sid: u32,
        text: &str,
        out: &mut Vec<CoreEvent>,
    ) {
        let completed_lines = engine.observer_router.dispatch_text(sid, text);
        // OutputMatch 훅 발사도 이 라인 버퍼를 공유 — 완성된 라인 단위로만
        // 매칭한다(패턴이 청크 경계에 걸쳐 있으면 라인이 완성될 때까지 매칭 안 됨).
        if engine.hook_manager.has_output_match_hook(sid) {
            for line in completed_lines {
                out.push(CoreEvent::TerminalOutputMatch {
                    surface_id: sid,
                    text: line,
                });
            }
        }
        // OSC 133 셸 통합 미설치 감지 — 첫 출력 시각을 기록하고, 지연 시간이
        // 지나도록 PromptBoundary 를 한 번도 못 받았으면 안내 배너 cascade 를
        // 1 회 요청한다.
        engine.note_first_output(sid);
        if engine.take_shell_integration_hint_due(sid) {
            out.push(CoreEvent::TerminalShellIntegrationHint { surface_id: sid });
        }
    }

    /// OSC 133 prompt boundary phase 도착 — command_index cap 알림 + D phase
    /// (명령 완료 + exit code, highlight 자동 발동/hook 커스터마이즈 cascade 공용)
    /// 를 발화한다.
    fn handle_prompt_boundary(
        &mut self,
        engine: &mut crate::core::CoreState,
        sid: u32,
        phase: char,
        payload: &str,
        out: &mut Vec<CoreEvent>,
    ) {
        engine.note_prompt_boundary_seen(sid);
        let mem = engine.memory.clone();
        if let Some(cap) = engine.command_index.on_boundary(&mem, sid, phase, payload) {
            use crate::core::command_index::CommandCapEvent;
            let (title, body) = match cap {
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
                surface_id: sid,
                title,
                body,
            });
        }
        // 항상 발화(필터 없음) — cascade 가 highlight 자동 발동 + hook
        // 커스터마이즈 경로 둘 다 처리한다.
        if phase == 'D' {
            let exit_code = crate::core::command_index::extract_exit_code(payload);
            out.push(CoreEvent::TerminalCommandCompleted {
                surface_id: sid,
                exit_code,
            });
        }
    }

    /// throttle 적용 PTY resize flush. 옛 `engine.flush_all_pty_resizes()` 의 진입점.
    /// 반환: 여전히 pending 이 남았는지 (redraw 재요청 신호).
    #[cfg(feature = "gui")]
    pub(crate) fn flush_pty_resizes(engine: &mut crate::core::CoreState) -> bool {
        engine.flush_all_pty_resizes()
    }

    /// 모든 workspace 의 모든 terminal 을 layout 에 맞춰 resize. 옛
    /// `state.resize_all(engine, ...)` 의 진입점. 탭 바 높이는 창이 정하는 값이라
    /// (`AppState::tab_bar_height`) 호출자가 값으로 넘긴다 — 도메인이 창 상태 전체를
    /// 받지 않게 하려는 것이다.
    ///
    /// TerminalSurface 는 id-marker 라 `Surface::resize_all` 은
    /// no-op. Terminal 본체는 `engine.terminals` (TerminalStore) 가 owner 이므로
    /// 여기서 직접 store 를 두드려 resize 한다.
    #[cfg(feature = "gui")]
    pub(crate) fn resize_all_terminals(
        tab_bar_height: crate::model::PhysicalPx,
        engine: &mut crate::core::CoreState,
        terminal_rect: crate::model::PhysicalRect,
        cell_width: f32,
        cell_height: f32,
        scale_factor: f32,
    ) {
        let targets = collect_terminal_resize_targets(
            tab_bar_height,
            engine,
            terminal_rect,
            cell_width,
            cell_height,
            scale_factor,
        );
        for (sid, cols, rows) in targets {
            // hard-점유된 surface(원격 client 가 mirror 로 구동 중인 서버측 실제 PTY)는
            // client-driven geometry(ADR-0045) — 점유 client 가 유일 구동자다. 이 host
            // 창의 레이아웃 sweep 이 원격 창 grid 로 되돌리면 client 의 ClientResize 가
            // 무력화되어 mirror 가 host 창 크기에 고정(레터박스)된다. 따라서 점유 중인
            // surface 는 여기서 skip 하고, 오직 `apply_attached_workspace_resize`(holder
            // 검증 후 client 요청 크기 적용)만 이 surface 의 grid 를 설정하게 한다. detach 로
            // lock 이 풀리면 다음 sweep 부터 host 창이 다시 구동한다(원복).
            if engine.attach.is_hard_occupied(sid) {
                continue;
            }
            if let Some(t) = engine.terminals.get_mut(sid) {
                // mirror(detached) 터미널은 client-driven geometry(ADR-0045):
                // 로컬 pane 목표 grid 를 로컬에 **직접 적용하지 않고**(로컬 grid 는
                // server 의 `Resize` echo 로만 갱신 → 원격 reflow 전 잘못된 grid 에
                // 바이트가 재생되는 desync 방지) 원격 PTY 를 그 크기로 구동하도록
                // forward 큐에 넣는다. 목표가 현재 mirror grid 와 같으면(정상상태)
                // enqueue 하지 않는다 — 전송할 변화가 없다. (transient 중복은
                // dispatch 의 세션 last-forwarded dedup 이 흡수한다.)
                if t.is_detached() {
                    if t.cols() != cols || t.rows() != rows {
                        engine.pending_resize_forward.insert(sid, (cols, rows));
                    }
                    continue;
                }
                t.resize(cols, rows);
            }
        }
    }

    /// busy surface 집합 갱신. 옛 `engine.refresh_busy_surfaces()` 의 진입점.
    /// `Tick::Busy` (1Hz 타이머) 에서 호출. 반환: 집합이 변했는지
    /// (window mark_dirty 결정 신호).
    #[cfg(feature = "gui")]
    pub(crate) fn update_busy_surfaces(engine: &mut crate::core::CoreState) -> bool {
        engine.refresh_busy_surfaces()
    }
}
