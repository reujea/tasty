use std::time::Instant;

use tasty_terminal::{Terminal, TerminalEvent};

use super::CoreState;

impl CoreState {
    /// idle TTL 을 넘긴 headless PTY 를 **두 store 에서 함께** 회수하고 회수한 pty id
    /// 들을 돌려준다(`docs/adr/0050-headless-pty-primitive.md` "좀비 방지 정책").
    ///
    /// 회수는 반드시 세 가지를 한 묶음으로 한다 — 어느 하나만 하면 누수나 좀비가
    /// 된다(ADR-0050 Consequences "어느 한 쪽만 지우면 누수/좀비"):
    ///
    /// 1. `pty_registry` entry 제거(= `sweep_idle`)
    /// 2. `TerminalStore` 의 `Terminal` 제거 — drop 되면서 PTY master 가 닫히고
    ///    자식이 SIGHUP 을 받는다. 이걸 빠뜨리면 자식 프로세스가 그대로 남는다.
    /// 3. waker dedup 게이트 해제 — 빠뜨리면 회수할 때마다 게이트가 영구 누적된다.
    ///
    /// **두 호출 경로가 이 함수를 공유한다.** `pty.spawn`/`pty.list` 접근 시점의 lazy
    /// sweep(`adapters/ipc/handler/pty.rs`)과 주기 타이머(`app/timers.rs` 의
    /// `Tick::PtySweep`)다. 후처리가 갈라지면 경로마다 다른 잔재가 남으므로 진입점을
    /// 하나로 묶었다. `sweep_idle` 이 idempotent 하고 시각을 주입받으므로 두 경로가
    /// 겹쳐 돌아도 안전하다.
    pub(crate) fn sweep_idle_ptys(&mut self, now: Instant) -> Vec<u32> {
        let expired = self.pty_registry.sweep_idle(now);
        for pty_id in &expired {
            let pty_id = *pty_id;
            self.terminals.remove(pty_id);
            if let Some(factory) = self.waker_factory.as_ref() {
                factory.forget_surface(pty_id);
            }
            tracing::debug!("headless pty {pty_id} swept (idle TTL exceeded)");
        }
        expired
    }

    pub fn send_fast_init(&mut self, surface_id: u32) {
        if let Err(e) = crate::surface_meta::SurfaceMetaStore::ensure_created(surface_id) {
            tracing::warn!("surface_meta ensure_created failed for surface {surface_id}: {e}");
        }
        let scrollback_limit = self.settings.general.scrollback_lines;
        let disk_swap = self.settings.performance.scrollback_disk_swap;
        if let Some(terminal) = self.find_terminal_by_id_mut(surface_id) {
            terminal.set_scrollback_limit(scrollback_limit);
            if disk_swap {
                terminal.enable_disk_scrollback(surface_id);
            }
        }
        // tasty 모드의 bashrc source 는 셸 `--rcfile` 인자로 처리한다
        // (effective_shell_args). 더 이상 PTY 입력으로 보내지 않는다 — 그래야
        // 화면 echo / 복원 시 claude 입력창 오염이 없다.
        let startup = self.settings.general.startup_command.trim();
        if !startup.is_empty() {
            let line = format!("{startup}\n");
            if let Some(terminal) = self.find_terminal_by_id_mut(surface_id) {
                terminal.send_key(&line);
            }
        }
    }

    /// Return `true` if the given surface_id is a deferred placeholder waiting
    /// for lazy PTY spawn (i.e. an `EmptySurface { deferred_spawn: Some(..) }` leaf
    /// in any tab layout).
    pub fn is_surface_deferred(&self, surface_id: u32) -> bool {
        for ws in &self.workspaces {
            let pane_ids = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                    for tab in &pane.tabs {
                        if tab.is_surface_deferred(surface_id) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// mesh-mirror 후보 판정(단일 surface attach 용 —
    /// `docs/dev-guide/attach-behavior.md` "mesh mirror 채널").
    /// `surface_id`가 어느 workspace 든 leaf 로 존재하고 `Surface::attach_mesh_info()`가
    /// `Some`을 반환하면 그 `(kind, plugin_id)`를 돌려준다 — 화이트리스트 검증은
    /// 호출자(`attach_surface_for_stream`)가 `mesh_mirror_candidates`와 동일한
    /// `is_egui_mesh_allowed` 게이트로 별도 수행한다.
    pub(crate) fn find_mesh_surface_info(&self, surface_id: u32) -> Option<(String, String)> {
        for ws in &self.workspaces {
            let pane_ids = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                    for tab in &pane.tabs {
                        if let Some(layout) = tab.layout_if_initialized()
                            && let Some(surface) = layout.find_surface(surface_id)
                            && let Some((kind, plugin_id)) = surface.attach_mesh_info()
                        {
                            return Some((kind.to_string(), plugin_id.to_string()));
                        }
                    }
                }
            }
        }
        None
    }

    /// [`Self::find_mesh_surface_info`]의 전체 메타데이터 버전 — attach mesh forward
    /// 루프가 `surface.create` bootstrap 에 필요한 `file`/`display_name`까지 필요해
    /// `EguiMeshSurface`(app 계층)로 직접 downcast 한다. `tasty-model`의
    /// `Surface::attach_mesh_info()`는 `(kind, plugin_id)`만 노출하도록 최소화됐다
    /// (attach-behavior.md "mesh mirror 채널" 참고 — crate 경계 최소화 결정) —
    /// 여기는 `tasty` 본체 crate 내부라
    /// `plugin_bridge::EguiMeshSurface` 참조가 경계를 넘지 않는다.
    ///
    /// 소비처: headless-as-attach-서버 forward 루프(`src/boot/headless_plugins.rs`)와
    /// gui-as-attach-서버 forward 훅(`src/view/main/egui_mesh.rs::forward_mesh_to_attach_subscribers`,
    /// attach-behavior.md "mesh mirror 채널" 참고) 둘 다 — 로컬에서 한 번도 렌더되지
    /// 않은 mesh surface 를 attach 구독이
    /// 가리킬 때 `surface.create` bootstrap 에 필요한 전체 메타데이터를 공급한다.
    pub(crate) fn find_egui_mesh_surface(
        &self,
        surface_id: u32,
    ) -> Option<&crate::core::egui_mesh_surface::EguiMeshSurface> {
        for ws in &self.workspaces {
            let pane_ids = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                    for tab in &pane.tabs {
                        if let Some(layout) = tab.layout_if_initialized()
                            && let Some(surface) = layout.find_surface(surface_id)
                            && let Some(ms) = surface
                                .as_any()
                                .downcast_ref::<crate::core::egui_mesh_surface::EguiMeshSurface>(
                            )
                        {
                            return Some(ms);
                        }
                    }
                }
            }
        }
        None
    }

    /// Lazy PTY init for a deferred surface. Returns `true` if a PTY was just
    /// spawned for this surface, `false` otherwise (already initialized or the
    /// surface_id isn't a known deferred placeholder).
    ///
    /// This is the IPC/CLI-facing counterpart to the workspace-switch path
    /// (`ensure_active_workspace_initialized`). It does *not* change focus,
    /// active workspace, or active tab — only the target surface's underlying
    /// PTY is materialized.
    pub fn ensure_surface_initialized(&mut self, surface_id: u32) -> bool {
        let mut spawned: Option<(Terminal, Option<String>)> = None;
        'outer: for ws in &mut self.workspaces {
            let pane_ids: Vec<u32> = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout_mut().find_pane_mut(pane_id) {
                    for tab in &mut pane.tabs {
                        if let Some(result) = tab.ensure_initialized(surface_id) {
                            spawned = Some(result);
                            break 'outer;
                        }
                    }
                }
            }
        }
        if let Some((terminal, persist_id)) = spawned {
            self.terminals.insert(surface_id, terminal);
            if let Some(pid) = persist_id {
                self.terminals.set_scrollback_persist_id(surface_id, pid);
            }
            self.send_fast_init(surface_id);
            self.apply_pending_scrollback_inject(surface_id);
            true
        } else {
            false
        }
    }

    /// plugin placeholder(`Deferred::Plugin`)를 registry 로 실제화한다 — terminal
    /// reify(`ensure_surface_initialized`)의 plugin 짝. `SurfaceKindRegistry.get`
    /// 은 값을 새로 만들지 않는 싼 read 라, kind 가 아직 없으면(plugin hello 전)
    /// 매 프레임 재시도해도 자원이 쌓이지 않는다(실패가 자기 재시도를 부르는
    /// 고리가 없다). kind 가 등록되는 순간 `restore` 로 실제 surface 를 만들어 leaf 를
    /// 교체한다. kind 가 영영 안 오면(plugin 이 죽었거나 매니페스트에서 사라짐)
    /// placeholder 가 그대로 남는다 — 의도된 상태다(`to_tree_json` 이 `ready:false`
    /// 로 노출; 상세 `docs/features/layout-persistence/index.md`).
    /// 반환: 이 프레임에 실제화됐으면 `true`.
    #[cfg(any(feature = "gui", test))]
    pub fn reify_plugin_surface(&mut self, surface_id: u32) -> bool {
        let registry = self.surface_registry.clone();
        for ws in &mut self.workspaces {
            let pane_ids: Vec<u32> = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout_mut().find_pane_mut(pane_id) {
                    for tab in &mut pane.tabs {
                        if tab.reify_deferred_plugin(surface_id, |kind, snap| {
                            let def = registry.get_live(kind)?;
                            (def.restore)(surface_id, snap).ok()
                        }) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Deferred terminal 이 spawn 된 직후 호출. layout 복원 시 큐에 적재된
    /// scrollback line 들을 해당 surface 의 terminal 에 inject. PTY 가 실제로
    /// 출력하기 전이라 사용자는 위로 스크롤하면 자연스러운 히스토리를 본다.
    pub fn apply_pending_scrollback_inject(&mut self, surface_id: u32) {
        let Some(lines) = self.pending_scrollback_inject.remove(&surface_id) else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        if let Some(terminal) = self.find_terminal_by_id_mut(surface_id) {
            terminal.inject_scrollback(lines);
            // 새 prompt 가 화면 중간부터 시작하도록 visible 상단 절반에 옛
            // 라인을 미리 그려둔다.
            let prefill = terminal.rows() / 2;
            terminal.prefill_visible_from_scrollback(prefill);
        }
    }

    /// Replace the terminal in a TerminalSurface, keeping the surface/layout intact.
    /// The old terminal's PTY process is dropped (SIGHUP sent).
    /// Returns Ok(()) on success, Err if the surface was not found.
    ///
    /// **D.3.E.4.f** — `TerminalStore::replace` 로 cutover. layout 트리의 옛
    /// Terminal owner 경로는 더 이상 사용 안 함.
    pub fn replace_terminal_by_id(
        &mut self,
        surface_id: u32,
        new_terminal: Terminal,
    ) -> anyhow::Result<()> {
        if let Some(old) = self.terminals.replace(surface_id, new_terminal) {
            drop(old); // SIGHUP
            return Ok(());
        }
        anyhow::bail!("Surface {} not found", surface_id)
    }

    /// terminal 들의 `OutputAppended` emit 게이트를 현재 옵저버 집합과 동기화.
    /// process 직전(lazy) + observer register/unregister 직후(eager) 호출 —
    /// terminal 생성 콜사이트가 게이트 초기화를 신경 쓸 필요가 없다.
    pub(crate) fn sync_output_event_gates(&mut self) {
        let router = &self.observer_router;
        let hook_manager = &self.hook_manager;
        for (sid, t) in self.terminals.iter_mut() {
            t.set_output_events_enabled(
                router.wants(sid) || hook_manager.has_output_match_hook(sid),
            );
        }
    }

    /// Process all terminals (read PTY output). **D.3.E.4.f** — store 가 owner.
    pub fn process_all(&mut self) -> bool {
        self.sync_output_event_gates();
        self.terminals.process_all()
    }

    /// OS 절전 복귀 후 헬스 패스 (Windows, ADR-0017). 살아있는 PTY 자식을 wake
    /// nudge 해 hang 에서 깨어나도록 유도하고, **자식 TUI 가 실행 중인(foreground
    /// 가 셸이 아닌) 살아있는 surface 들의 ID** 를 의심 목록으로 반환한다. 죽은
    /// 자식은 여기서 건드리지 않고 곧이은 `process_all` 의 `ProcessExited` cascade
    /// 가 정리한다. 호출자는 의심 목록으로 사용자 알림을 발행한다.
    ///
    /// cfg 는 유일한 호출자 `App::resume_health_pass` 와 같은 짝이다 — Windows 에서 gui 를
    /// 끄면 호출자가 없어 `dead_code = "deny"` 에 걸린다.
    #[cfg(all(windows, feature = "gui"))]
    pub(crate) fn wake_terminals_after_resume(&mut self) -> Vec<u32> {
        let mut suspects = Vec::new();
        for (sid, term) in self.terminals.iter_mut() {
            if !term.check_process_alive() {
                continue; // 죽음 — process_all 의 ProcessExited cascade 가 정리.
            }
            term.wake_nudge();
            let shell_pid = term.process_id();
            if let Some(info) = term.foreground_process_info()
                && Some(info.pid) != shell_pid
                && !tasty_terminal::foreground_process::is_known_shell_name(&info.name)
            {
                suspects.push(sid);
            }
        }
        suspects
    }

    /// Process a single terminal by surface ID (read PTY output).
    /// Returns true if data was processed.
    pub fn process_surface(&mut self, surface_id: u32) -> bool {
        let enabled = self.observer_router.wants(surface_id)
            || self.hook_manager.has_output_match_hook(surface_id);
        if let Some(t) = self.terminals.get_mut(surface_id) {
            t.set_output_events_enabled(enabled);
        }
        self.terminals.process_surface(surface_id)
    }

    /// Flush deferred PTY resizes (throttled). Returns true if any terminal still has pending resize.
    #[cfg(feature = "gui")]
    pub fn flush_all_pty_resizes(&mut self) -> bool {
        self.terminals.flush_pty_resizes()
    }

    /// Mark layout as dirty for persistence.
    pub fn mark_layout_dirty(&mut self) {
        self.layout_dirty.mark_dirty();
    }

    /// Collect events from all terminals. **D.3.E.4.f** — store iter.
    ///
    /// Uses a non-blocking take: a terminal whose parser thread currently holds
    /// the state lock (mid-chunk ingest) is skipped this round, so the input
    /// thread never serializes against busy parser threads (ADR-0002). Skipped
    /// events are not lost — the parser wakes the loop again after each ingest.
    pub fn collect_events(&mut self) -> Vec<TerminalEvent> {
        let mut all_events = Vec::new();
        for (sid, terminal) in self.terminals.iter_mut() {
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

    /// Collect all terminal surface IDs across all workspaces.
    /// 현재는 CWD 폴링(macOS/Linux)에서만 사용된다.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    // 이유: 현재 실제 호출처 없음(소비자 배선 대기) — 과거 engine.rs → core/ 재배치로
    // core 가 pub(crate) 로 캡슐화되며 드러남.
    #[allow(dead_code)]
    pub fn all_terminal_surface_ids(&mut self) -> Vec<u32> {
        self.terminals.iter().map(|(id, _)| id).collect()
    }
}
