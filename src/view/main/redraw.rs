use crate::app::plugin_display::PluginDisplay;
use crate::runtime::engine_read::EngineRead;
use crate::view::ui::View;

use super::MainView;

/// WebView 생성 재시도 상한. 실패 중 발생한 X 이벤트가 다음 프레임을 깨워
/// 재시도가 반복될 수 있으므로 surface별 시도 횟수를 제한한다.
pub(crate) const MAX_WEBVIEW_CREATE_ATTEMPTS: u32 = 8;

/// 영구 실패와 일시 실패의 재시도 횟수가 달라야 한다.
const _: () = assert!(MAX_WEBVIEW_CREATE_ATTEMPTS > 1);

/// 페이지가 계속 숨겨져 있으면 한 번 경고할 시간. 로드를 중단하는 제한은 아니다.
pub(crate) const REVEAL_PENDING_WARN_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// 진단 로그에 남길 URL 의 **종류와 크기**. 원문은 남기지 않는다 — markdown 처럼
/// 문서 전체를 raw HTML 로 싣는 kind 가 있어 로그가 문서만큼 커진다.
pub(crate) fn describe_webview_url(url: Option<&String>) -> String {
    let Some(url) = url else {
        return "none".to_string();
    };
    let kind = if url.starts_with("file://") {
        "file"
    } else if url.starts_with("http://") || url.starts_with("https://") {
        "http"
    } else {
        "raw-html"
    };
    format!("{kind} {} bytes", url.len())
}

/// 이 surface 를 또 시도할 것인가.
pub(crate) fn should_attempt_webview(attempts: u32) -> bool {
    attempts < MAX_WEBVIEW_CREATE_ATTEMPTS
}

/// 영구 실패는 재시도를 멈추도록 시도 횟수를 상한까지 올린다.
pub(crate) fn next_webview_attempts(
    attempts: u32,
    err: &crate::webview::WebViewCreateError,
) -> u32 {
    if err.is_permanent() {
        MAX_WEBVIEW_CREATE_ATTEMPTS
    } else {
        attempts.saturating_add(1).min(MAX_WEBVIEW_CREATE_ATTEMPTS)
    }
}

/// host 창이 OS 포커스를 가졌는지. webview 자식이 포커스를 쥐면 winit은 부모 창에
/// Focused(false)를 보내므로 자식이 포커스를 가진 경우도 활성으로 본다.
/// `webview_holds`는 OS에 포커스를 묻는 호출이라 winit 값이 거짓일 때만 부른다.
pub(crate) fn host_window_has_os_focus(
    winit_focused: bool,
    webview_holds: impl FnOnce() -> bool,
) -> bool {
    winit_focused || webview_holds()
}

/// 포커스 회수 대상으로 남길 surface인지. 닫힌 surface의 webview는 같은 프레임 뒤에서
/// 제거되므로 map에 남아 있어도 이번 프레임의 HTML surface 목록에 없으면 제외한다.
pub(crate) fn webview_release_candidate_is_live(
    sid: u32,
    all_html_ids: &[u32],
    has_webview: impl Fn(u32) -> bool,
) -> bool {
    all_html_ids.contains(&sid) && has_webview(sid)
}

/// 활성 탭에서 빠진 webview 중 이번 프레임에 키보드 포커스를 회수할 surface를 고른다.
///
/// 직전 프레임에 활성이었다가 빠진 surface를 `pending`에 모은다. 다시 활성이 되었거나
/// 사라진 surface는 뺀다. host 창이 OS 포커스를 가질 때만 모은 surface를 꺼내므로
/// 배경 창에서 일어난 전환은 사용자가 창으로 돌아올 때까지 미뤄진다.
/// `host_focused`는 모은 surface가 있을 때만 부른다.
pub(crate) fn take_hidden_webview_focus_targets(
    prev_active: &std::collections::HashSet<u32>,
    now_active: &std::collections::HashSet<u32>,
    is_live: impl Fn(u32) -> bool,
    pending: &mut std::collections::HashSet<u32>,
    host_focused: impl FnOnce() -> bool,
) -> Vec<u32> {
    pending.extend(prev_active.difference(now_active).copied());
    pending.retain(|sid| !now_active.contains(sid) && is_live(*sid));
    if pending.is_empty() || !host_focused() {
        return Vec::new();
    }
    let mut targets: Vec<u32> = pending.drain().collect();
    targets.sort_unstable();
    targets
}

impl MainView {
    pub(crate) fn prepare_redraw(&mut self, engine: &EngineRead<'_>) {
        self.state.reconcile_presentation(engine);
        // 열기 요청은 render_if_dirty 전에 소비해야 한다. egui가 이번 프레임에 만든
        // 요청이 다음 프레임까지 남아 키·마우스 차단과 Escape 취소에 사용되기 때문이다.
        // 순서 검증: crates/tasty-doc-guards/tests/fullscreen_stage_render_gate.rs.
        self.dispatch_pending_modal_opens();

        // 렌더링 전에 무대 진입으로 중단된 드래그와 IME 조합을 정리한다.
        self.sync_fullscreen_stage_transition(engine);

        // PTY 출력과 TerminalEvent 처리는 AppEvent::TerminalOutput에서 수행한다.

        self.resync_scale_factor(engine);

        // 레이아웃 변경에 맞춰 터미널 크기를 갱신한다. 무대 중에는 원본 grid를
        // 보존하고 무대를 나온 뒤 현재 영역에 맞춘다.
        if !self.state.fullscreen_stage_active() {
            let terminal_rect = self.compute_terminal_rect();
            let cell_w = self.base.gpu.cell_width();
            let cell_h = self.base.gpu.cell_height();
            let scale_factor = self.base.gpu.scale_factor();
            self.state
                .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
        }
    }

    pub(crate) fn finish_redraw(
        &mut self,
        engine: &EngineRead<'_>,
        _plugin_manager: Option<PluginDisplay<'_>>,
    ) {
        // 무대가 draw 중 닫힐 수 있으므로 렌더 뒤 OS 전체화면 상태를 맞춘다.
        self.sync_window_fullscreen();

        self.dispatch_pending_command_palette(engine);

        // 닫힌 메뉴의 후처리를 먼저 마쳐 새 메뉴 요청을 처리할 수 있게 한다.
        self.poll_pending_native_menu(engine);

        // Process pending native context menu (after egui frame, before webview sync)
        self.process_pending_native_menu(engine);

        // file handler 선택 결과는 App의 다음 frame begin에서 처리한다.

        // 외부 drag&drop 으로 받은 파일 큐 처리.
        self.process_pending_file_drops(engine);

        self.poll_outbound_drag(engine);

        if self.base.state.dirty && self.base.surface_ready() {
            self.base.winit.request_redraw();
        }
    }

    /// 전체화면 무대 진입 시 진행 중인 드래그·IME·native 메뉴를 취소한다.
    /// 배경으로 release가 전달되지 않으므로 드래그를 확정하지 않고 버린다.
    /// 이미 확정된 텍스트 선택과 vi 복사 모드는 유지한다.
    fn sync_fullscreen_stage_transition(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let active = self.state.fullscreen_stage_active();
        if active == self.stage_was_active {
            return;
        }
        self.stage_was_active = active;
        if !active {
            // 무대 종료 시 복원할 진행 상태는 없다.
            return;
        }

        // 무대를 열던 중의 조합 문자는 PTY로 보내지 않고 버린다.
        if self.ime_preedit.is_some() {
            self.clear_ime_preedit(engine);
        }

        // 분할선은 마지막 유효 비율을 확정하고 나머지 포인터 제스처를 끝낸다.
        self.finish_divider_drag(engine);
        self.left_mouse_down = false;
        self.left_select_bypass = false;
        self.state.popups.cancel_pointer_interactions();

        // 뒤 좌표 기반 hover 잔재 — 무대 중에는 갱신도 안 되므로 여기서 비운다.
        self.hovered_link = None;
        self.state.pending_resize_cursor = None;

        // OS 메뉴는 무대 위에도 표시되므로 닫고, 아직 열지 않은 요청도 버린다.
        self.dismiss_pending_native_menu();
        self.state.dialogs.pending_native_menu = None;
        self.cancel_outbound_drag();

        self.mark_dirty();
    }

    /// settings/plugins 모달 오픈 요청 dispatch. `ui.rs`가 `state.settings_open_requested`/
    /// `state.plugins_open`을 true로 세팅하면 여기서 소비해 `AppEvent`로 변환한다.
    fn dispatch_pending_modal_opens(&mut self) {
        // Check if settings button was clicked (ui.rs sets state.settings_open_requested = true)
        if self.state.settings_open_requested {
            self.state.settings_open_requested = false;
            crate::shortcuts::send_app_event(&self.proxy, crate::AppEvent::OpenSettings);
        }
        // Same flow for plugins modal.
        if self.state.plugins_open {
            self.state.plugins_open = false;
            crate::shortcuts::send_app_event(&self.proxy, crate::AppEvent::OpenPlugins);
        }
    }

    /// Re-sync scale factor before render — macOS may not fire
    /// ScaleFactorChanged reliably during monitor hot-swap or sleep/wake.
    fn resync_scale_factor(&mut self, engine: &crate::runtime::engine_read::EngineRead<'_>) {
        if self.base.gpu.sync_scale_factor(&self.base.winit) {
            let new_size = self.base.winit.inner_size();
            self.base.gpu.resize(new_size);
            // 무대 중에는 swapchain만 창 크기에 맞추고 기본 grid 갱신은 종료 후로 미룬다.
            if self.state.fullscreen_stage_active() {
                self.state.stage_deferred_grid_resync = true;
            } else {
                self.apply_grid_resync(engine);
            }
            // Schedule another redraw to verify scale factor has stabilized.
            self.base.state.dirty = true;
        } else if self.state.stage_deferred_grid_resync && !self.state.fullscreen_stage_active() {
            // 무대를 나온 첫 프레임 — 보류했던 갱신을 여기서 소진한다.
            self.state.stage_deferred_grid_resync = false;
            self.apply_grid_resync(engine);
            self.base.state.dirty = true;
        }
    }

    /// 현재 창/스케일 기준으로 신규 터미널의 기본 grid(cols/rows)를 갱신한다.
    fn apply_grid_resync(&mut self, _engine: &crate::runtime::engine_read::EngineRead<'_>) {
        let terminal_rect = self.compute_terminal_rect();
        let (cols, rows) = self.base.gpu.grid_size_for_rect(&terminal_rect);
        self.state.dispatch_intent(
            crate::intent::Intent::Frame(
                crate::app::frame_effect::GeometryEffect::DefaultGrid { cols, rows }.into(),
            )
            .from_user_shortcut("grid-resize"),
        );
    }

    /// dirty일 때 입력·mesh 중계와 GPU 렌더링, full 재전송 요청을 처리한다.
    /// 로컬 무대 중에도 attach 구독자에게 mesh를 중계해야 하므로 조기 반환하지 않는다.
    pub(crate) fn prepare_render_inputs(
        &mut self,
        engine: &EngineRead<'_>,
        plugin_manager: Option<PluginDisplay<'_>>,
    ) {
        if !self.base.state.dirty {
            return;
        }
        // App drains these outputs before drawing this frame, including an otherwise idle
        // terminal. Fullscreen stage rendering must not acknowledge hidden terminal content.
        if !self.state.fullscreen_stage_active()
            && let Some(sid) = self.state.focused_surface_id(engine)
            && let Some(target) =
                crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        {
            // 포커스 surface는 포커스 pane과 현재 workspace의 것이다. 창까지 OS 포커스를
            // 가져야 사용자가 실제로 본 것으로 보고 attention을 지운다.
            let window_focused = host_window_has_os_focus(self.base.state.focused, || {
                self.webviews.values().any(|wv| wv.holds_keyboard_focus())
            });
            self.state.dispatch_intent(
                crate::intent::Intent::Frame(
                    crate::app::frame_effect::InputEffect::FocusObserved {
                        target,
                        window_focused,
                    }
                    .into(),
                )
                .from_user_menu("render-focus"),
            );
        }
        // Reconcile composition for every displayed content source, including
        // global PTY wakes, direct parser injection and attach mirrors.
        self.recalc_ime_preedit_anchor(engine);
        self.update_ime_cursor_area(&engine.as_ref());
        // 불변 차용 전에 plugin에 크기·배율·입력을 보내고 회신한 mesh를 합성한다.
        if let Some(mgr) = plugin_manager {
            self.forward_egui_mesh_context(engine, mgr);
        }
        // attach mesh mirror surface — 위와 동형이되 목적지가 원격이라
        // PluginManager 가 필요 없다(로컬에 plugin 프로세스가 없다).
        self.forward_attach_mesh_context(engine);
    }

    pub(crate) fn render_if_dirty(
        &mut self,
        engine: &EngineRead<'_>,
        plugin_manager: Option<PluginDisplay<'_>>,
    ) {
        if !self.base.state.dirty || !self.base.surface_ready() {
            return;
        }
        // Preparation and drawing share the same dirty frame. Consume it only after
        // this final admission check, so prepare_render_inputs cannot skip the draw.
        self.base.begin_surface_frame();
        self.submit_gpu_frame(engine, plugin_manager);
        self.drain_full_texture_requests(engine);
    }

    /// 실제 GPU 프레임 제출 + surface 에러 분기 처리.
    fn submit_gpu_frame(
        &mut self,
        engine: &EngineRead<'_>,
        plugin_manager: Option<PluginDisplay<'_>>,
    ) {
        if self.state.dialogs.remote_row_popup.take_dropped() {
            self.hovered_link = None;
        }
        crate::adapters::ui::link_remote_tip::sync(
            &mut self.state.link_remote_tip,
            self.hovered_link
                .as_ref()
                .and_then(|h| h.remote_tip.as_ref().map(|tip| (h.surface_id, tip))),
            std::time::Instant::now(),
        );
        let link_hover = self
            .hovered_link
            .as_ref()
            .map(|h| (h.surface_id, &h.highlight));
        let active_sel = self.active_text_selection();
        let vi_cursor = self.vi_copy.as_ref().map(|v| (v.surface_id, v.cursor));
        match self.base.gpu.render(
            &mut self.state,
            engine,
            &self.base.winit,
            self.ime_preedit.as_ref(),
            active_sel.as_ref(),
            vi_cursor,
            link_hover,
            plugin_manager,
        ) {
            Ok(()) => {
                self.base.surface_presented();
                // 실제 present에 성공한 첫 시각만 부팅 계측에 기록한다.
                crate::boot::trace::mark_first_paint();
                if self.base.gpu.take_terminal_cursor_restore_pending() {
                    self.base.state.dirty = true;
                }
            }
            Err(e @ (wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated)) => {
                self.base.gpu.reconfigure(self.base.winit.inner_size());
                self.base.surface_failed(&e);
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                self.base.state.repaint.cancel_surface_retry();
                tracing::error!("GPU out of memory");
                crate::crash_report::record_error("GPU out of memory");
            }
            Err(e) => self.base.surface_failed(&e),
        }
        self.copy_egui_text();
    }

    /// egui-mesh(로컬 plugin 대상) + attach mesh mirror(원격 대상) full 재전송
    /// 요청 drain — 렌더 prepare 가 textures_delta 체인 단절을 감지한 대상들.
    /// surface 는 forward 추적 상태에, popup/banner 는 MainViewState 에 옮겨 두면
    /// 다음 tick 의 forward 가 need_full_textures `set_context`/`MeshFullResendRequest`
    /// 를 보낸다. plugin/원격은 스스로 재송신하지 않으므로 다음 tick 을 dirty 로
    /// 보장한다.
    fn drain_full_texture_requests(&mut self, engine: &EngineRead<'_>) {
        let full_reqs = self.base.gpu.take_egui_mesh_full_requests();
        let popup_full_reqs = self.base.gpu.take_egui_mesh_popup_full_requests();
        let banner_full_reqs = self.base.gpu.take_egui_mesh_banner_full_requests();
        if !full_reqs.is_empty() || !popup_full_reqs.is_empty() || !banner_full_reqs.is_empty() {
            for sid in full_reqs {
                self.egui_mesh.entry(sid).or_default().set_pending_full();
            }
            for iid in popup_full_reqs {
                self.state
                    .plugin_mesh_popup_forward
                    .entry(iid)
                    .or_default()
                    .pending_full = true;
            }
            for iid in banner_full_reqs {
                self.state
                    .plugin_mesh_banner_forward
                    .entry(iid)
                    .or_default()
                    .pending_full = true;
            }
            self.base.state.dirty = true;
        }

        // attach mesh mirror(`docs/dev-guide/attach-behavior.md#mesh-mirror-채널` 참고)
        // full 재전송 요청 drain — 위와 동형이되 대상이
        // 로컬 plugin 이 아니라 원격이므로, `about_to_wait`(`dispatch_pending_mesh_full_resend_forwards`)
        // 가 세션을 통해 `MeshFullResendRequest` 로 forward 하도록 큐에 옮긴다.
        let attach_full_reqs = self.base.gpu.take_attach_mesh_full_requests();
        if !attach_full_reqs.is_empty() {
            let targets = attach_full_reqs
                .into_iter()
                .filter_map(|surface| {
                    crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface)
                })
                .collect();
            self.state.dispatch_intent(
                crate::intent::Intent::Frame(
                    crate::app::frame_effect::DrawEffect::RemoteMeshFull { targets }.into(),
                )
                .from_user_shortcut("mesh-full-recovery"),
            );
            self.base.state.dirty = true;
        }
    }

    /// 팝업이 닫힌 뒤 팔레트 명령을 실행한다.
    /// 호스트 명령은 여기서 실행하고 plugin 명령은 App의 처리 큐에 넣는다.
    fn dispatch_pending_command_palette(&mut self, engine: &EngineRead<'_>) {
        if let Some(cmd) = self.state.command_palette.pending_run.take() {
            match cmd {
                crate::state::command_palette::PaletteCommand::Host { id, .. } => {
                    // 알 수 없는 action_id는 dispatch_action_by_id가 경고를 기록한다.
                    self.dispatch_action_by_id(engine, id);
                }
                crate::state::command_palette::PaletteCommand::Plugin {
                    plugin_id,
                    command_id,
                    ..
                } => {
                    self.state
                        .pending_plugin_command_invokes
                        .push((plugin_id, command_id));
                }
            }
        }
    }

    /// 활성 워크스페이스의 각 패널에서 활성 탭에 속한 surface인지 확인한다.
    /// 분할된 탭은 모든 leaf가 보인다. 숨겨진 surface의 출력도 읽되 redraw만 생략한다.
    pub(crate) fn is_surface_visible(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
    ) -> bool {
        let Some(ws) = engine.workspace_at(self.state.active_workspace_index(engine)) else {
            return false;
        };
        for pane_id in ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pane_id)
                && let Some(tab) = pane.tabs.get(self.state.navigation.tab_index(pane))
                && tab.contains_surface(surface_id)
            {
                return true;
            }
        }
        false
    }

    /// native 메뉴를 열고 결과를 cont에 전달한다.
    /// macOS/Windows의 Ready는 즉시 처리하며 Linux의 Pending은 이후 폴링으로 회수한다.
    pub(super) fn open_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        items: &[crate::platform::native_menu::MenuItem],
        cont: impl FnOnce(&mut MainView, &EngineRead<'_>, Option<u32>) + 'static,
    ) {
        use crate::platform::native_menu::{
            MenuOutcome, show_context_menu, warn_if_menu_anchor_scale_premise_broken,
        };
        // `x`/`y` 는 논리 좌표다. 플랫폼 백엔드가 그것을 **자기** 논리 좌표계로
        // 읽으므로, 두 좌표계의 배율이 같다는 전제가 성립해야 앵커가 맞는다.
        // Linux/GTK 에서만 그 둘의 출처가 갈린다(winit vs GDK) — 깨지면 경고한다.
        warn_if_menu_anchor_scale_premise_broken(self.base.winit.scale_factor());
        #[cfg(debug_assertions)]
        super::debug_menu::answer_or_return!(self, engine, items, cont);
        // TrackPopupMenu는 중첩 메시지 루프를 돈다.
        #[cfg(windows)]
        crate::host_api::webview::entering_nested_message_loop();
        match show_context_menu(self.base.winit.as_ref(), x as f64, y as f64, items) {
            MenuOutcome::Ready(result) => {
                cont(self, engine, result);
                self.mark_dirty();
            }
            MenuOutcome::Pending(handle) => {
                self.pending_menu = Some((handle, Box::new(cont)));
                // 메뉴를 연 프레임만 그린다. 이후 폴링은 about_to_wait가 진행한다.
                self.mark_dirty();
            }
        }
    }

    /// 메뉴 결과를 비차단 조회하고 완료됐으면 후처리를 실행한다.
    /// redraw에서는 새 메뉴 요청보다 먼저, 대기 중에는 8ms 주기로 호출한다.
    pub(crate) fn poll_pending_native_menu(&mut self, engine: &EngineRead<'_>) {
        let Some((handle, _)) = self.pending_menu.as_mut() else {
            return;
        };
        let Some(result) = handle.poll() else {
            return;
        };
        let Some((_, cont)) = self.pending_menu.take() else {
            return;
        };
        cont(self, engine, result);
        self.mark_dirty();
    }

    /// 바깥 클릭으로 native 메뉴를 닫는다. GTK grab이 실패한 경우도 처리한다.
    /// 결과는 다음 poll_pending_native_menu에서 회수한다.
    pub(crate) fn dismiss_pending_native_menu(&mut self) -> bool {
        let Some((handle, _)) = self.pending_menu.as_mut() else {
            return false;
        };
        handle.dismiss();
        self.mark_dirty();
        true
    }

    /// Process pending native context menu request.
    /// Called after egui frame so we have access to the window handle.
    fn process_pending_native_menu(&mut self, engine: &EngineRead<'_>) {
        use crate::state::PendingNativeMenu;

        // OS 메뉴는 무대 위에 뜨므로 요청을 버린다. 무대 종료 뒤에는 좌표도 유효하지 않다.
        if self.state.fullscreen_stage_active() {
            self.state.dialogs.pending_native_menu = None;
            return;
        }

        // 이미 메뉴가 떠 있으면 새로 띄우지 않는다 — 요청은 슬롯에 남겨 두고
        // (take 하지 않는다) 현재 메뉴가 닫힌 뒤 프레임에 처리한다.
        if self.pending_menu.is_some() {
            return;
        }

        let pending = match self.state.dialogs.pending_native_menu.take() {
            Some(p) => p,
            None => return,
        };

        // debug에서는 실제 native 메뉴 없이 egui의 메뉴 요청을 관찰하거나, 열 메뉴의 종류를 기록한다.
        #[cfg(debug_assertions)]
        super::debug_menu::capture_or_note!(self, pending);

        match pending {
            PendingNativeMenu::Tab {
                pane_id,
                tab_index,
                x,
                y,
            } => self.handle_tab_native_menu(engine, pane_id, tab_index, x, y),
            PendingNativeMenu::Pane { pane_id, x, y } => {
                self.handle_pane_native_menu(engine, pane_id, x, y)
            }
            PendingNativeMenu::Workspace { ws_idx, x, y } => {
                self.handle_workspace_native_menu(engine, ws_idx, x, y)
            }
            PendingNativeMenu::WorkspaceCategoryHeader { cat_id, x, y } => {
                self.handle_workspace_category_header_native_menu(engine, cat_id, x, y)
            }
            PendingNativeMenu::SidebarBackground { x, y } => {
                self.handle_sidebar_background_native_menu(engine, x, y)
            }
            PendingNativeMenu::TerminalSurface { surface_id, x, y } => {
                self.handle_terminal_surface_native_menu(engine, surface_id, x, y)
            }
            PendingNativeMenu::TerminalLink { link, x, y } => {
                self.handle_terminal_link_native_menu(engine, link, x, y)
            }
            PendingNativeMenu::Surface { surface_id, x, y } => {
                self.handle_surface_native_menu(engine, surface_id, x, y)
            }
            PendingNativeMenu::Explorer {
                surface_id,
                paths,
                cwd,
                single_is_dir,
                x,
                y,
            } => self.handle_explorer_native_menu(
                engine,
                surface_id,
                paths,
                cwd,
                single_is_dir,
                x,
                y,
            ),
            PendingNativeMenu::ExplorerFavorite {
                surface_id,
                path,
                x,
                y,
            } => self.handle_explorer_favorite_native_menu(engine, surface_id, path, x, y),
            PendingNativeMenu::ExplorerMore { surface_id, x, y } => {
                self.handle_explorer_more_native_menu(engine, surface_id, x, y)
            }
            PendingNativeMenu::NewWorkspaceButton { x, y } => {
                self.handle_new_workspace_button_native_menu(engine, x, y)
            }
            PendingNativeMenu::NewTabButton { pane_id, x, y } => {
                self.handle_new_tab_button_native_menu(engine, pane_id, x, y)
            }
        }
    }

    fn handle_tab_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        pane_id: u32,
        tab_index: usize,
        x: f32,
        y: f32,
    ) {
        // 메뉴가 열린 동안 탭 순서가 바뀌어도 같은 탭을 가리키도록 ID로 고정한다.
        let target = super::menu_target::TabMenuTarget::capture(engine, pane_id, tab_index);
        let tab_id = target.tab_id();
        let items = self.build_tab_context_menu_items(engine, pane_id, tab_index, tab_id);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            if let (Some(tab_id), Some(item)) = (tab_id, result)
                && this.apply_tab_move_selection(engine, tab_id, item)
            {
                return;
            }
            // continuation 은 메뉴가 닫힌 뒤(플랫폼에 따라 여러 프레임 뒤)
            // 실행된다 — 그 사이 탭이 닫혔거나 옮겨졌을 수 있으므로 현재 위치를 다시 찾는다.
            let Some((pane_id, tab_index)) = target.resolve(engine) else {
                return;
            };
            this.apply_tab_menu_selection(engine, pane_id, tab_index, result);
        });
    }

    /// tab 컨텍스트 메뉴 선택 적용. 대상 유효성은 호출부(continuation)가 미리
    /// 검사한다.
    fn apply_tab_menu_selection(
        &mut self,
        engine: &EngineRead<'_>,
        pane_id: u32,
        tab_index: usize,
        result: Option<u32>,
    ) {
        match result {
            Some(1) => self.rename_tab(engine, pane_id, tab_index),
            Some(2) => {
                if self.state.close_tab(engine, pane_id, tab_index)
                    && engine.workspaces().is_empty()
                {
                    self.request_close();
                }
            }
            Some(3) => {
                // Move Left — mirror 워크스페이스는 로컬 탭 순서 변경 대신 MoveTab 을
                // 원격으로 forward 한다(로컬 실행은 원격 트리와 어긋남).
                if tab_index > 0 {
                    self.move_tab_via_mirror_or_local(engine, pane_id, tab_index, tab_index - 1);
                }
            }
            Some(4) => {
                // Move Right — mirror 워크스페이스는 로컬 탭 순서 변경 대신 MoveTab 을
                // 원격으로 forward 한다(로컬 실행은 원격 트리와 어긋남).
                self.move_tab_via_mirror_or_local(engine, pane_id, tab_index, tab_index + 1);
            }
            Some(5) => {
                if let Err(e) = self.save_tab_preset_from_pane_tab(engine, pane_id, tab_index) {
                    tracing::warn!("save tab preset failed: {e}");
                    self.state.toasts.push(
                        crate::i18n::t("preset.toast.save_failed"),
                        crate::adapters::ui::ToastKind::Error,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
            }
            Some(6) => {
                if let Err(e) = self.save_pane_preset_from_pane_id(engine, pane_id) {
                    tracing::warn!("save pane preset failed: {e}");
                    self.state.toasts.push(
                        crate::i18n::t("preset.toast.save_failed"),
                        crate::adapters::ui::ToastKind::Error,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
            }
            _ => {}
        }
    }

    /// tab 우클릭 컨텍스트 메뉴 항목 구성. move left/right 는 인접 위치
    /// 존재 여부로 활성/비활성을 미리 계산하고, 이동 항목은 대기 슬롯에 따라 붙인다.
    fn build_tab_context_menu_items(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        pane_id: u32,
        tab_index: usize,
        tab_id: Option<u32>,
    ) -> Vec<crate::platform::native_menu::MenuItem> {
        use crate::platform::native_menu::MenuItem;
        let tab_count = self
            .state
            .active_workspace(engine)
            .pane_layout()
            .find_pane(pane_id)
            .map(|p| p.tabs.len())
            .unwrap_or(0);

        let move_left = MenuItem {
            enabled: tab_index > 0,
            ..MenuItem::new(3, crate::i18n::t("tab_context_menu.move_left"))
        };
        let move_right = MenuItem {
            enabled: tab_index + 1 < tab_count,
            ..MenuItem::new(4, crate::i18n::t("tab_context_menu.move_right"))
        };

        let mut items = vec![
            MenuItem::new(1, crate::i18n::t("tab_context_menu.rename")),
            MenuItem::new(2, crate::i18n::t("tab_context_menu.close")),
            MenuItem::separator(),
            move_left,
            move_right,
            MenuItem::separator(),
            MenuItem::new(5, crate::i18n::t("preset.context.save_as_tab_preset")),
            MenuItem::new(6, crate::i18n::t("preset.context.save_as_pane_preset")),
        ];
        if let Some(tab_id) = tab_id {
            self.push_tab_move_items(engine, &mut items, tab_id);
        }
        items
    }

    /// mirror의 탭 이동은 원격으로 보내고, 그 외에는 로컬 탭 순서를 변경한다.
    fn move_tab_via_mirror_or_local(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        pane_id: u32,
        from_index: usize,
        to_index: usize,
    ) {
        let mirror_op = engine
            .find_pane_by_id(pane_id)
            .and_then(|p| p.tabs.get(self.state.navigation.tab_index(p)))
            .and_then(|t| self.state.navigation.surface_id(t))
            .map(|sid| crate::ipc::stream::StructuralOp::MoveTab {
                anchor_surface_id: sid,
                from_index,
                to_index,
            });
        if !self
            .state
            .forward_mirror_structural(engine, mirror_op, Vec::new())
            && let Some(tab_id) = engine
                .find_pane_by_id(pane_id)
                .and_then(|pane| pane.tabs.get(from_index))
                .map(|tab| tab.id)
        {
            self.state.dispatch_intent(
                crate::app::command::DomainIntent::MoveTab {
                    pane_id,
                    tab_id,
                    to_index,
                }
                .from_user_context_menu(),
            );
        }
    }

    /// tab rename 팝업을 연다 — 현재 표시명을 prefill 하고 `RenameTarget::TabName`
    /// scope 로 `rename` 팝업을 dispatch.
    fn rename_tab(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        pane_id: u32,
        tab_index: usize,
    ) {
        let Some((tab_id, current_name)) = self
            .state
            .active_workspace(engine)
            .pane_layout()
            .find_pane(pane_id)
            .and_then(|p| p.tabs.get(tab_index))
            .map(|t| {
                (
                    t.id,
                    engine.tab_display_name(t, self.state.navigation.surface_id(t)),
                )
            })
        else {
            return;
        };
        self.open_rename_dialog(
            engine,
            crate::state::RenameTarget::TabName { tab_id },
            current_name,
        );
    }

    fn handle_pane_native_menu(&mut self, engine: &EngineRead<'_>, pane_id: u32, x: f32, y: f32) {
        use crate::platform::native_menu::MenuItem;
        let items = [
            MenuItem::new(1, crate::i18n::t("pane_context_menu.new_terminal")),
            MenuItem::new(2, crate::i18n::t("pane_context_menu.new_markdown")),
            MenuItem::new(4, crate::i18n::t("pane_context_menu.new_html")),
            MenuItem::new(5, crate::i18n::t("pane_context_menu.new_image")),
            MenuItem::separator(),
            MenuItem::new(6, crate::i18n::t("preset.context.save_as_pane_preset")),
            MenuItem::separator(),
            MenuItem::new(7, crate::i18n::t("preset.context.apply_tab_preset")),
            MenuItem::new(8, crate::i18n::t("preset.context.apply_pane_preset")),
        ];
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 pane 이 닫혔을 수 있다.
            if !engine.has_pane(pane_id) {
                return;
            }
            this.apply_pane_menu_selection(engine, pane_id, result);
        });
    }

    /// pane 컨텍스트 메뉴 선택 적용. 대상 유효성은 호출부(continuation)가 미리
    /// 검사한다.
    fn apply_pane_menu_selection(
        &mut self,
        engine: &EngineRead<'_>,
        pane_id: u32,
        result: Option<u32>,
    ) {
        match result {
            Some(1) => {
                let selected = engine
                    .find_pane_by_id(pane_id)
                    .and_then(|pane| pane.tabs.get(self.state.navigation.tab_index(pane)))
                    .and_then(|tab| self.state.navigation.surface_id(tab));
                let cwd = selected.and_then(|surface| {
                    self.state.resolve_inherit_cwd_from_surface(engine, surface)
                });
                self.state.dispatch_intent(
                    crate::app::command::DomainIntent::CreateTab {
                        pane_id,
                        cwd,
                        kind: "terminal".into(),
                        name: None,
                        surface_params: serde_json::json!({}),
                        activate: true,
                    }
                    .from_user_context_menu(),
                );
            }
            Some(2) => {
                self.state.dispatch_intent(
                    crate::intent::Intent::NewTabWithFollowup {
                        pane_id,
                        followup: crate::intent::CreateFollowup::Prompt {
                            kind: "markdown".into(),
                        },
                    }
                    .from_user_context_menu(),
                );
            }
            Some(4) => crate::adapters::ui::new_tab_picker::open(
                &mut self.state,
                engine,
                pane_id,
                "html",
                "url",
                &["html", "htm", "svg"],
            ),
            Some(5) => {
                self.state.dispatch_intent(
                    crate::app::command::DomainIntent::CreateTab {
                        pane_id,
                        cwd: None,
                        kind: "image".into(),
                        name: None,
                        surface_params: serde_json::json!({}),
                        activate: true,
                    }
                    .from_user_context_menu(),
                );
            }
            Some(6) => {
                if let Err(e) = self.save_pane_preset_from_pane_id(engine, pane_id) {
                    tracing::warn!("save pane preset failed: {e}");
                    self.state.toasts.push(
                        crate::i18n::t("preset.toast.save_failed"),
                        crate::adapters::ui::ToastKind::Error,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
            }
            Some(7) => {
                self.state.select_pane(engine, pane_id);
                self.state.dialogs.preset_picker_selected = None;
                self.state.dispatch_intent(
                    crate::intent::UiIntent::OpenPopup {
                        id: crate::adapters::ui::popup::preset_apply::APPLY_TAB_POPUP_ID,
                        mode: crate::intent::OpenPopupMode::CenteredFocused,
                    }
                    .from_user_context_menu(),
                );
            }
            Some(8) => {
                self.state.select_pane(engine, pane_id);
                self.state.dialogs.preset_picker_selected = None;
                self.state.dispatch_intent(
                    crate::intent::UiIntent::OpenPopup {
                        id: crate::adapters::ui::popup::preset_apply::APPLY_PANE_POPUP_ID,
                        mode: crate::intent::OpenPopupMode::CenteredFocused,
                    }
                    .from_user_context_menu(),
                );
            }
            _ => {}
        }
    }

    fn handle_workspace_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        ws_idx: usize,
        x: f32,
        y: f32,
    ) {
        let target = super::menu_target::WorkspaceMenuTarget::capture(engine, ws_idx);
        let (items, move_targets) = self.build_workspace_context_menu_items(engine, ws_idx);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 워크스페이스가 닫히거나 옮겨졌을 수 있어 현재 위치를 다시 찾는다.
            let Some(ws_idx) = target.resolve(engine) else {
                return;
            };
            match result {
                Some(1) => {
                    let ws = engine
                        .workspace_at(ws_idx)
                        .expect("workspace index is valid");
                    let (workspace_id, name) = (ws.id, ws.name.clone());
                    this.open_rename_dialog(
                        engine,
                        crate::state::RenameTarget::WorkspaceName { workspace_id },
                        name,
                    );
                }
                Some(2) => {
                    let ws = engine
                        .workspace_at(ws_idx)
                        .expect("workspace index is valid");
                    let (workspace_id, subtitle) = (ws.id, ws.subtitle.clone());
                    this.open_rename_dialog(
                        engine,
                        crate::state::RenameTarget::WorkspaceSubtitle { workspace_id },
                        subtitle,
                    );
                }
                Some(3) => {
                    // Move Up
                    if ws_idx > 0 {
                        this.state.move_workspace(engine, ws_idx, ws_idx - 1);
                    }
                }
                Some(4) => {
                    // Move Down
                    if ws_idx + 1 < engine.workspaces().len() {
                        this.state.move_workspace(engine, ws_idx, ws_idx + 1);
                    }
                }
                Some(5) => {
                    if let Err(e) = this.save_workspace_preset_from_idx(engine, ws_idx) {
                        tracing::warn!("save workspace preset failed: {e}");
                        this.state.toasts.push(
                            crate::i18n::t("preset.toast.save_failed"),
                            crate::adapters::ui::ToastKind::Error,
                            crate::adapters::ui::ToastScope::Window,
                        );
                    }
                }
                // 상태를 변경하는 닫기 호출은 match guard에 넣지 않는다.
                #[allow(clippy::collapsible_match)]
                Some(6) => {
                    // Close workspace (모든 surface + closed_item snapshot)
                    if this.state.close_workspace_at(
                        engine,
                        ws_idx,
                        crate::state::WorkspaceCloseOrigin::User,
                    ) && engine.workspaces().is_empty()
                    {
                        this.request_close();
                    }
                }
                Some(8) => {
                    // 메뉴를 연 mirror의 로컬 연결만 끊는다. 서버 워크스페이스와 프로세스는 유지한다.
                    let ws = engine
                        .workspace_at(ws_idx)
                        .expect("workspace index is valid");
                    if ws.mirror {
                        this.state.dispatch_intent(
                            crate::intent::Intent::DisconnectMirror {
                                workspace_id: ws.id,
                            }
                            .from_user_context_menu(),
                        );
                    }
                }
                Some(7) => {
                    // 확인 팝업을 기다리는 동안 인덱스가 바뀔 수 있으므로 ID를 보관한다.
                    let ws_id = engine
                        .workspace_at(ws_idx)
                        .expect("workspace index is valid")
                        .id;
                    this.state.dialogs.pending_force_detach_workspace = Some(ws_id);
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::confirm_force_detach_workspace
                                ::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_menu("workspace/force-detach"),
                    );
                }
                Some(100) => {
                    // 새 카테고리 생성 다이얼로그.
                    crate::adapters::ui::category_actions::open_new_category_dialog(
                        &mut this.state,
                        engine,
                    );
                }
                Some(id) if id >= 200 => {
                    this.move_workspace_to_category(engine, ws_idx, &move_targets, id);
                }
                Some(id) => {
                    if let Some(ws) = engine.workspace_at(ws_idx) {
                        super::saved_row_menu::handle(&mut this.state, ws.id, id);
                    }
                }
                _ => {}
            }
        });
    }

    /// workspace 우클릭 컨텍스트 메뉴 항목 구성 + "카테고리로 이동" 대상 목록.
    /// 반환된 `Vec<WorkspaceCategoryId>` 는 인덱스 i 가 메뉴 항목 id `200+i` 에
    /// 대응한다(카테고리 토글이 꺼져 있으면 빈 벡터).
    fn build_workspace_context_menu_items(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        ws_idx: usize,
    ) -> (
        Vec<crate::platform::native_menu::MenuItem>,
        Vec<crate::model::WorkspaceCategoryId>,
    ) {
        use crate::platform::native_menu::MenuItem;
        let ws_count = engine.workspaces().len();

        let move_up = MenuItem {
            enabled: ws_idx > 0,
            ..MenuItem::new(3, crate::i18n::t("context_menu.move_up"))
        };
        let move_down = MenuItem {
            enabled: ws_idx + 1 < ws_count,
            ..MenuItem::new(4, crate::i18n::t("context_menu.move_down"))
        };

        let mut items = vec![
            MenuItem::new(1, crate::i18n::t("context_menu.rename_title")),
            MenuItem::new(2, crate::i18n::t("context_menu.rename_subtitle")),
            MenuItem::separator(),
            move_up,
            move_down,
            MenuItem::separator(),
            MenuItem::new(5, crate::i18n::t("preset.context.save_as_workspace_preset")),
        ];

        // 현재 카테고리를 제외한 이동 대상과 새 카테고리 항목을 만든다.
        let mut move_targets: Vec<crate::model::WorkspaceCategoryId> = Vec::new();
        if engine.settings.general.workspace_categories_enabled
            && ws_idx < engine.workspaces().len()
        {
            let cur_cat = engine
                .workspace_at(ws_idx)
                .expect("workspace index is valid")
                .category;
            items.push(MenuItem::separator());
            // 비클릭 헤더(disabled) + 대상 카테고리 항목들.
            items.push(MenuItem::disabled(
                0,
                crate::i18n::t("workspace_category.move_to_category"),
            ));
            for cat in engine.categories() {
                if cat.id == cur_cat {
                    continue;
                }
                let label = if cat.is_normal() {
                    crate::i18n::t("sidebar.workspaces_heading").to_string()
                } else {
                    cat.name.clone()
                };
                items.push(MenuItem::new(200 + move_targets.len() as u32, label));
                move_targets.push(cat.id);
            }
            items.push(MenuItem::separator());
            items.push(MenuItem::new(
                100,
                crate::i18n::t("workspace_category.new_category"),
            ));
        }

        // 사이드바 점유 표시와 같은 기준으로 강제 끊기 항목을 추가한다.
        if ws_idx < engine.workspaces().len()
            && engine
                .live
                .occupancy
                .workspace_holder(
                    engine
                        .workspace_at(ws_idx)
                        .expect("workspace index is valid")
                        .id,
                )
                .is_some()
        {
            items.push(MenuItem::separator());
            items.push(MenuItem::new(7, crate::i18n::t("attach.force_detach")));
        }

        // 카테고리 토글 상태와 무관하게 "닫기"는 항상 최하단.
        match super::saved_row_menu::saved_row_state(engine, ws_idx) {
            Some(state) => items.extend(super::saved_row_menu::saved_row_menu_items(state)),
            None => items.extend(workspace_close_menu_items(
                engine.workspace_at(ws_idx).is_some_and(|ws| ws.mirror),
            )),
        }

        (items, move_targets)
    }

    /// 현재 값을 채운 `rename` 팝업을 `target`의 scope로 연다. 컨텍스트 메뉴의 이름 변경이 모두 쓴다.
    fn open_rename_dialog(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        target: crate::state::RenameTarget,
        current_value: String,
    ) {
        let scope = target.popup_scope(engine);
        self.state.dialogs.rename = Some((target, current_value));
        self.state.dispatch_intent(
            crate::intent::UiIntent::OpenPopup {
                id: "rename",
                mode: crate::intent::OpenPopupMode::WithScope(scope),
            }
            .from_user_context_menu(),
        );
    }

    /// workspace 를 `move_targets[id-200]` 카테고리로 이동. `id` 는 컨텍스트
    /// 메뉴가 회신한 원본 값(200 이상 — `build_workspace_context_menu_items`
    /// 참고) 그대로 받는다.
    fn move_workspace_to_category(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        ws_idx: usize,
        move_targets: &[crate::model::WorkspaceCategoryId],
        id: u32,
    ) {
        if let Some(&cat_id) = move_targets.get((id - 200) as usize) {
            let ws_id = engine
                .workspace_at(ws_idx)
                .expect("workspace index is valid")
                .id;
            self.state.dispatch_intent(
                crate::intent::Intent::Domain(
                    crate::app::command::DomainIntent::SetWorkspaceCategory {
                        workspace_id: ws_id,
                        category: cat_id,
                    },
                )
                .from_user_context_menu(),
            );
        }
    }

    fn handle_workspace_category_header_native_menu(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        cat_id: crate::model::WorkspaceCategoryId,
        x: f32,
        y: f32,
    ) {
        // 카테고리 메뉴를 구성한다. 기본 카테고리는 이름 변경과 삭제를 제외한다.
        let is_normal = engine
            .categories()
            .iter()
            .find(|c| c.id == cat_id)
            .map(|c| c.is_normal())
            .unwrap_or(true);
        let items = category_header_menu_items(is_normal);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 카테고리가 삭제됐을 수 있다.
            if !engine.categories().iter().any(|c| c.id == cat_id) {
                return;
            }
            match result {
                Some(3) => {
                    // 이 카테고리 소속으로 새 워크스페이스 — 레일 `---` 팝업의
                    // Add workspace 와 동일 인텐트 (소스 문자열만 구별).
                    this.state.dispatch_intent(
                        crate::intent::Intent::NewWorkspace {
                            kind: None,
                            params: serde_json::Value::Null,
                            category: Some(cat_id),
                        }
                        .from_user_menu("category_header/add_workspace"),
                    );
                }
                Some(4) => {
                    // 원격 워크스페이스 추가 팝업 — 카테고리 헤더 우클릭 진입(사용자 조작).
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::remote_attach::REMOTE_ATTACH_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                Some(5) => {
                    // 이 카테고리 소속으로 프리셋 적용 — "+" 버튼 메뉴와 동일 팝업
                    // (APPLY_WORKSPACE_POPUP_ID) 재사용, 대상 카테고리만 임시 상태로 기억.
                    this.state.dialogs.preset_apply_target_category = Some(cat_id);
                    this.state.dialogs.preset_picker_selected = None;
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::preset_apply::APPLY_WORKSPACE_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                Some(1) => {
                    crate::adapters::ui::category_actions::open_rename_category_dialog(
                        &mut this.state,
                        engine,
                        cat_id,
                    );
                }
                Some(2) => {
                    crate::adapters::ui::category_actions::open_delete_category_confirm(
                        &mut this.state,
                        cat_id,
                    );
                }
                Some(100) => {
                    crate::adapters::ui::category_actions::open_new_category_dialog(
                        &mut this.state,
                        engine,
                    );
                }
                _ => {}
            }
        });
    }

    fn handle_sidebar_background_native_menu(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        // 빈 배경 우클릭 — 새 카테고리 · 원격 워크스페이스 추가. 그룹모드 배경(카테고리
        // ON)·flat모드 배경(카테고리 OFF, `docs/features/workspace-category/index.md`
        // 참고) 양쪽에서 이 핸들러로 라우팅되므로
        // 카테고리 상태 분기 없이 공통 메뉴로 노출한다.
        let items = [
            MenuItem::new(100, crate::i18n::t("workspace_category.new_category")),
            MenuItem::separator(),
            MenuItem::new(2, crate::i18n::t("context_menu.add_remote_workspace")),
        ];
        // 사이드바 배경 메뉴는 특정 대상(탭/pane/워크스페이스)에 매이지 않아
        // continuation 시점에 재확인할 대상 자체가 없다.
        self.open_native_menu(
            engine,
            x,
            y,
            &items,
            move |this, engine, result| match result {
                Some(100) => {
                    crate::adapters::ui::category_actions::open_new_category_dialog(
                        &mut this.state,
                        engine,
                    );
                }
                Some(2) => {
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::remote_attach::REMOTE_ATTACH_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                _ => {}
            },
        );
    }

    fn handle_terminal_surface_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        // Show copy items only when there is an active (non-empty) selection.
        let has_selection = self.text_selection.as_ref().is_some_and(|s| !s.is_empty());
        // 경로 열기는 선택과 우클릭 surface가 같을 때만 표시한다.
        // 복사 항목은 surface와 관계없이 전역 선택을 사용한다.
        let selection_open_path = if has_selection {
            self.text_selection
                .as_ref()
                .filter(|s| s.surface_id == surface_id)
                .and_then(|s| Self::resolve_selection_open_path(&engine.as_ref(), s))
        } else {
            None
        };
        let mut items = Vec::new();
        if has_selection {
            items.push(MenuItem::new(
                2,
                crate::i18n::t("terminal_context_menu.copy"),
            ));
            items.push(MenuItem::new(
                3,
                crate::i18n::t("terminal_context_menu.copy_no_newline"),
            ));
            if let Some(target) = &selection_open_path {
                items.push(MenuItem::new(
                    20,
                    if target.is_dir() {
                        crate::i18n::t("terminal_context_menu.open_folder")
                    } else {
                        crate::i18n::t("terminal_context_menu.open_file")
                    },
                ));
            }
            items.push(MenuItem::separator());
        }
        items.push(MenuItem::new(
            1,
            crate::i18n::t("terminal_context_menu.copy_surface_id"),
        ));
        items.push(MenuItem::separator());
        self.push_surface_move_items(&mut items);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 대상 surface 가 닫혔을 수 있다.
            if !engine.has_surface(surface_id) {
                return;
            }
            match result {
                Some(1) => {
                    let text = surface_id.to_string();
                    this.copy_text(
                        &text,
                        false,
                        "toast.copied",
                        Vec::new(),
                        crate::adapters::ui::ToastScope::Surface(surface_id),
                    );
                }
                Some(2) => {
                    let hint = crate::adapters::ui::toast::binding_hint(engine.settings, "copy");
                    this.copy_selection_with_hint(engine, hint);
                }
                Some(3) => {
                    this.copy_selection_no_newline(engine);
                }
                Some(20) => {
                    if let Some(target) = &selection_open_path {
                        this.state.request_explorer_file(
                            engine,
                            surface_id,
                            crate::app::explorer_files::Operation::Open(vec![target.clone()]),
                            crate::intent::IntentOrigin::User {
                                source: crate::intent::UserSource::ContextMenu,
                            },
                        );
                    }
                }
                Some(item) => {
                    // 사용자 우클릭 조작(release 경로)이다. 이동 항목이 아니면 무시한다.
                    this.apply_surface_move_selection(surface_id, item);
                }
                None => {}
            }
        });
    }

    /// 선택한 실제 파일·폴더 경로를 찾는다. surface 일치는 호출자가 확인한다.
    /// 원격 호스트 경로를 로컬 파일 관리자로 열 수 없으므로 mirror는 제외한다.
    fn resolve_selection_open_path(
        engine: &EngineRead<'_>,
        sel: &crate::selection::TextSelection,
    ) -> Option<std::path::PathBuf> {
        let terminal = engine.visible_terminal(sel.surface_id)?;
        engine.terminals.process_id(sel.surface_id)?;
        let raw_text = crate::selection::extract_selected_text(terminal, sel);
        let cwd = engine.terminals.cwd(sel.surface_id);
        crate::adapters::ui::terminal_link::longest_existing_selection_path(
            &raw_text,
            cwd.as_deref(),
            false,
        )
    }

    fn handle_surface_native_menu(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        // 비-terminal surface: 전용 항목(copy surface id) + 구분선 +
        // 서피스 이동 / (서피스가 대기 중일 때) 서피스를 이곳으로 이동.
        let mut items = vec![MenuItem::new(
            1,
            crate::i18n::t("terminal_context_menu.copy_surface_id"),
        )];
        items.push(MenuItem::separator());
        self.push_surface_move_items(&mut items);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 대상 surface 가 닫혔을 수 있다.
            if !engine.has_surface(surface_id) {
                return;
            }
            match result {
                Some(1) => {
                    let text = surface_id.to_string();
                    this.copy_text(
                        &text,
                        false,
                        "toast.copied",
                        Vec::new(),
                        crate::adapters::ui::ToastScope::Surface(surface_id),
                    );
                }
                Some(item) => {
                    // 사용자 우클릭 조작(release 경로)이다. 이동 항목이 아니면 무시한다.
                    this.apply_surface_move_selection(surface_id, item);
                }
                None => {}
            }
        });
    }

    #[allow(clippy::too_many_arguments)] // reason: 호출부가 PendingNativeMenu::Explorer 를 해체해 그대로 넘기는 형제 핸들러다 — 다시 묶으면 방금 해체한 것을 되돌리는 꼴이다
    fn handle_explorer_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        paths: Vec<std::path::PathBuf>,
        cwd: std::path::PathBuf,
        single_is_dir: bool,
        x: f32,
        y: f32,
    ) {
        let multi = paths.len() > 1;
        let is_empty_target = paths.is_empty();
        let is_folder = paths.len() == 1 && single_is_dir;
        // 쓸 수 없는 원격 칸은 쓰기 행을 꺼 두고 이유를 툴팁으로 단다(docs/features/remote-attach/index.md).
        // 다른 호출 경로도 있으므로 각 핸들러의 검사도 유지한다.
        let is_mirror = engine.is_mirror_surface(surface_id);
        let refusal = engine.explorer_write_refusal(surface_id);
        // 메뉴를 연 surface 세대를 고정한다. 그 사이 닫히거나 바뀐 surface에는 결과를 적용하지 않는다.
        let binding = crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface_id);
        let mut items = self.with_show_in_folder(
            surface_id,
            &paths,
            Self::build_explorer_context_menu(
                multi,
                is_empty_target,
                is_folder,
                self.explorer_menu_has_clip(engine, surface_id),
                (is_mirror, refusal.as_deref()),
            ),
        );
        self.push_explorer_history_items(surface_id, is_empty_target && !is_mirror, &mut items);
        crate::state::explorer_menu::attach_shortcut_hints(&mut items, engine.settings);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 경로 복사·복사·루트 설정은 파일시스템을 바꾸지 않아 같은 explorer 인지만 본다.
            let read_only = matches!(result, Some(1 | 10 | 61 | 70));
            if !crate::state::explorer_menu::explorer_menu_admits(
                binding.as_ref(),
                engine,
                read_only,
            ) {
                return;
            }
            match result {
                Some(1) => {
                    // 빈 곳을 눌러 현재 폴더를 복사하는 동작에는 대응하는 단축키가 없다.
                    let hint = if is_empty_target {
                        Vec::new()
                    } else {
                        crate::adapters::ui::toast::binding_hint(engine.settings, "copy_path")
                    };
                    this.explorer_menu_copy_path(surface_id, &paths, &cwd, is_empty_target, hint)
                }
                Some(10) => this.explorer_menu_set_clipboard(engine, surface_id, &paths, false),
                Some(11) => this.explorer_menu_set_clipboard(engine, surface_id, &paths, true),
                Some(12) => this.explorer_menu_paste(
                    engine,
                    surface_id,
                    &paths,
                    &cwd,
                    is_folder,
                    crate::intent::IntentOrigin::User {
                        source: crate::intent::UserSource::ContextMenu,
                    },
                ),
                Some(30) => this.explorer_menu_trash(engine, surface_id, &paths),
                Some(20) => this.explorer_menu_open_in_system(engine, surface_id, &paths, &cwd),
                Some(21) => this.explorer_menu_reveal(engine, surface_id, &paths),
                Some(40) => {
                    if let Some(binding) = binding {
                        this.explorer_menu_rename(engine, surface_id, &paths, binding)
                    }
                }
                Some(50) => this.explorer_menu_add_favorite(
                    engine,
                    surface_id,
                    &paths,
                    &cwd,
                    is_empty_target,
                ),
                Some(60) => this.explorer_menu_open_in_new_tab(engine, surface_id, &paths),
                Some(61) => this.explorer_menu_set_root(engine, surface_id, &paths),
                Some(70) => crate::adapters::ui::popup::explorer_properties::open_from_menu(
                    &mut this.state,
                    engine.is_mirror_surface(surface_id),
                    surface_id,
                    &paths,
                    &cwd,
                ),
                Some(id) => this.explorer_menu_more(engine, surface_id, id, &paths, &cwd),
                None => {}
            }
        });
    }

    /// explorer 컨텍스트 메뉴 아이템 목록 구성 (design §3.3). `is_mirror` 면 로컬 fs
    /// 쓰기 항목(붙여넣기/잘라내기/이름변경/삭제/시스템에서 열기/새탭으로 열기)을
    /// 숨긴다 — copy_path/복사/즐겨찾기 추가/이 폴더로 루트 설정은 fs 를 쓰지
    /// 않거나(즐겨찾기 추가는 클릭 시 별도 가드) 안전해 그대로 노출한다.
    fn build_explorer_context_menu(
        multi: bool,
        is_empty_target: bool,
        is_folder: bool,
        has_clip: bool,
        (is_mirror, refusal): (bool, Option<&str>),
    ) -> Vec<crate::platform::native_menu::MenuItem> {
        use crate::platform::native_menu::MenuItem;
        // 쓰기 행. 쓸 수 없는 원격 칸에서는 숨기지 않고 꺼 두며 이유를 툴팁으로 단다.
        let write_row = |id: u32, key: &str| match refusal {
            None => MenuItem::new(id, crate::i18n::t(key)),
            Some(reason) => MenuItem::disabled(id, crate::i18n::t(key)).with_tooltip(reason),
        };
        let copy_path_label = if multi {
            crate::i18n::t("explorer.context_menu.copy_path_multi")
        } else {
            crate::i18n::t("explorer.context_menu.copy_path")
        };
        let mut items = Vec::new();
        if is_empty_target {
            Self::push_explorer_create_items(&mut items, true, refusal);
        }
        items.push(MenuItem::new(1, copy_path_label));
        // 즐겨찾기 추가는 단일 폴더 또는 빈 영역(cwd, 디렉토리)에서만 (design §3.3).
        if is_empty_target || is_folder {
            items.push(MenuItem::new(
                50,
                crate::i18n::t("explorer.context_menu.add_to_favorites"),
            ));
        }
        // 단일 폴더: 새 탭으로 열기 / 이 폴더로 루트 설정 (빈 영역=cwd 자기
        // 자신엔 무의미 → 제외). 새 탭으로 열기는 mirror 에서 숨김(로컬 전용
        // 유령 tab 생성 방지). 이 폴더로 루트 설정은 순수 로컬 뷰 이동이라 mirror 에서도 노출.
        if is_folder {
            if !is_mirror {
                items.push(MenuItem::new(
                    60,
                    crate::i18n::t("explorer.context_menu.open_in_new_tab"),
                ));
            }
            items.push(MenuItem::new(
                61,
                crate::i18n::t("explorer.context_menu.set_as_root"),
            ));
        }
        if is_empty_target {
            // 빈 영역(cwd): 붙여넣기만 (붙여넣을 수 있는 클립보드가 있을 때).
            if has_clip {
                items.push(MenuItem::separator());
                items.push(write_row(12, "explorer.context_menu.paste"));
            }
        } else {
            // 파일/폴더/다중: 복사 · 잘라내기. 잘라내기는 쓸 수 없는 원격에서 꺼진다.
            // 원격 클립보드는 출처 workspace 를 함께 적어 다른 곳에 붙여넣지 않는다.
            items.push(MenuItem::new(
                10,
                crate::i18n::t("explorer.context_menu.copy_files"),
            ));
            items.push(write_row(11, "explorer.context_menu.cut"));
            if is_folder && has_clip {
                items.push(write_row(12, "explorer.context_menu.paste_into"));
            }
            if is_folder {
                Self::push_explorer_create_items(&mut items, false, refusal);
            }
            // 이름 변경 (단일 파일/폴더만).
            if !multi {
                items.push(write_row(40, "explorer.context_menu.rename"));
            }
            // 휴지통으로 이동 (파일/폴더/다중 공통).
            items.push(MenuItem::separator());
            items.push(write_row(30, "explorer.context_menu.delete"));
            // OS 연계 행은 로컬 칸에서만 자기 묶음으로 둔다. 원격 칸은 두 행과 구분선을 함께 숨긴다.
            if !is_mirror {
                items.push(MenuItem::separator());
                Self::push_explorer_os_items(&mut items, multi);
            }
        }
        // 모든 모양의 메뉴는 Properties 로 끝난다. 빈 영역이면 현재 폴더가 대상이다.
        items.push(MenuItem::separator());
        items.push(MenuItem::new(
            70,
            crate::i18n::t("explorer.context_menu.properties"),
        ));
        items
    }

    /// (docs/features/remote-attach/index.md) mirror explorer 에서 하지 않는 로컬 전용 조작(시스템 열기·즐겨찾기 추가·새 탭)의 안내 —
    /// `apply_explorer_action` 의 `OpenFile` mirror 가드(`egui_panels.rs`)와 동일한 toast kind/scope.
    /// 파일을 바꾸는 조작은 `refuse_remote_write` 가 그 칸에 이유를 알린다.
    pub(crate) fn toast_remote_write_unsupported(&mut self) {
        self.state.toasts.push(
            crate::i18n::t("explorer.state.remote_write_unsupported").to_string(),
            crate::adapters::ui::ToastKind::Info,
            crate::adapters::ui::ToastScope::Window,
        );
    }

    /// 경로 복사 (아이템 1).
    fn explorer_menu_copy_path(
        &mut self,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
        is_empty_target: bool,
        hint: Vec<String>,
    ) {
        let text = if is_empty_target {
            cwd.display().to_string()
        } else {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.copy_text(
            &text,
            false,
            "toast.copied_path",
            hint,
            crate::adapters::ui::ToastScope::Surface(surface_id),
        );
    }

    /// 복사(cut=false, 아이템 10) / 잘라내기(cut=true, 아이템 11) 클립보드 설정.
    /// 컨텍스트 메뉴와 키보드 단축키(`handle_explorer_shortcut`) 양쪽에서 공유한다.
    /// 잘라내기는 원격 쓰기를 받지 않는 칸에서만 막는다. 클립보드는 앱의 모든 explorer 가 공유하므로
    /// 담은 칸의 컴퓨터(`source`)를 함께 적고, 다른 컴퓨터 칸의 붙여넣기는
    /// [`ExplorerClipboard::pastes_into`](crate::state::ExplorerClipboard::pastes_into) 가 막는다.
    pub(crate) fn explorer_menu_set_clipboard(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        cut: bool,
    ) {
        if cut && self.state.refuse_remote_write(engine, surface_id) {
            return;
        }
        let Some(source) = crate::state::ExplorerPathSource::of_surface(engine, surface_id) else {
            return;
        };
        if let crate::state::explorer_os_clipboard::FileCopyOutcome::InternalOnly(error) = self
            .state
            .explorer_set_clipboard(paths.to_vec(), cut, source)
        {
            tracing::debug!(kind = ?std::mem::discriminant(&error), "file copy retained in app slot");
            let (key, kind) = error.files_feedback(crate::clipboard::error::FileClipboardOp::Copy);
            self.state.toasts.push(
                crate::i18n::t(key),
                kind,
                crate::adapters::ui::ToastScope::Surface(surface_id),
            );
        }
    }

    /// 붙여넣기 (아이템 12). 컨텍스트 메뉴와 키보드 단축키 양쪽에서 공유한다.
    pub(crate) fn explorer_menu_paste(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
        is_folder: bool,
        origin: crate::intent::IntentOrigin,
    ) {
        // mirror surface 와 원격 클립보드 거부는 `explorer_paste` 가 맡는다.
        let dest = if is_folder {
            paths.first().cloned().unwrap_or_else(|| cwd.to_path_buf())
        } else {
            cwd.to_path_buf()
        };
        self.state.explorer_paste(engine, surface_id, dest, origin);
    }

    /// 휴지통으로 이동 (아이템 30, 가역적이라 별도 확인 모달 없음).
    fn explorer_menu_trash(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
    ) {
        // (docs/features/remote-attach/index.md) 원격 쓰기를 알리지 않은 서버의 mirror explorer 는 파일을 바꾸지 않는다.
        if self.state.refuse_remote_write(engine, surface_id) {
            return;
        }
        self.state.request_explorer_file(
            engine,
            surface_id,
            crate::app::explorer_files::Operation::Trash(paths.to_vec()),
            crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::ContextMenu,
            },
        );
    }

    /// 시스템에서 열기 (아이템 20).
    fn explorer_menu_open_in_system(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
    ) {
        // (docs/features/remote-attach/index.md) mirror explorer 는 파일 변경을 지원하지 않는다.
        if engine.is_mirror_surface(surface_id) {
            self.toast_remote_write_unsupported();
            return;
        }
        let targets = if paths.is_empty() {
            vec![cwd.to_path_buf()]
        } else {
            paths.to_vec()
        };
        self.state.request_explorer_file(
            engine,
            surface_id,
            crate::app::explorer_files::Operation::Open(targets),
            crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::ContextMenu,
            },
        );
    }

    /// 이름 변경 (아이템 40).
    pub(crate) fn explorer_menu_rename(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        binding: crate::runtime::surface_binding::SurfaceBinding,
    ) {
        // (docs/features/remote-attach/index.md) 원격 쓰기를 알리지 않은 서버의 mirror explorer 는 파일을 바꾸지 않는다.
        // 이 가드가 먼저 막아 rename 팝업(`draw_rename_popup`) 자체가 열리지 않는다.
        if self.state.refuse_remote_write(engine, surface_id) {
            return;
        }
        if let Some(path) = paths.first().cloned() {
            let current_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let target = crate::state::RenameTarget::ExplorerEntry {
                surface_id,
                path,
                binding,
            };
            self.open_rename_dialog(engine, target, current_name);
        }
    }

    /// 즐겨찾기 추가 (아이템 50) — 대상: 단일 폴더면 그 폴더, 빈 영역이면 cwd.
    fn explorer_menu_add_favorite(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
        is_empty_target: bool,
    ) {
        // 원격 경로를 전역 즐겨찾기에 저장하면 로컬·다른 호스트에서도 사용될 수 있다.
        // 팝업에는 surface_id가 없으므로 여기서 mirror를 차단한다(docs/features/remote-attach/index.md).
        if engine.is_mirror_surface(surface_id) {
            self.toast_remote_write_unsupported();
            return;
        }
        let path = if is_empty_target {
            cwd.to_path_buf()
        } else {
            paths.first().cloned().unwrap_or_else(|| cwd.to_path_buf())
        };
        let seed = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = crate::state::RenameTarget::ExplorerAddFavorite { path };
        self.open_rename_dialog(engine, target, seed);
    }

    /// 대상 폴더로 새 explorer 탭을 연다.
    /// add_kind_tab_by_owner는 원격 구조 변경을 전달하지 않고 로컬만 변경하므로
    /// mirror에서는 금지한다. 로컬에만 만든 탭은 다음 원격 구조 동기화에서 사라진다.
    fn explorer_menu_open_in_new_tab(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
    ) {
        if engine.is_mirror_surface(surface_id) {
            self.toast_remote_write_unsupported();
            return;
        }
        if let Some(folder) = paths.first().cloned() {
            let params = serde_json::json!({ "path": folder.to_string_lossy() });
            if let Err(e) = self
                .state
                .add_kind_tab_by_owner(engine, surface_id, "explorer", &params)
            {
                tracing::warn!("explorer: open in new tab failed: {e}");
            }
        }
    }

    /// 이 폴더로 루트 설정 (아이템 61) — 현재 explorer 의 cwd 를 그 폴더로 이동.
    fn explorer_menu_set_root(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
    ) {
        if let Some(folder) = paths.first().cloned() {
            self.state.set_explorer_cwd(engine, surface_id, folder);
        }
    }

    fn handle_explorer_favorite_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        path: std::path::PathBuf,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        // 즐겨찾기 행의 "새 탭으로 열기" 도 `add_kind_tab_by_owner` 를 공유하는
        // 동일 mirror 문제(`explorer_menu_open_in_new_tab` 주석 참고) — mirror 에서는
        // 메뉴에서부터 숨긴다. "이 폴더로 루트 설정"/"즐겨찾기에서 제거"는 로컬 뷰
        // 상태만 바꾸는 안전한 동작이라 그대로 노출.
        let is_mirror = engine.is_mirror_surface(surface_id);
        let mut items = Vec::new();
        if !is_mirror {
            items.push(MenuItem::new(
                60,
                crate::i18n::t("explorer.context_menu.open_in_new_tab"),
            ));
        }
        items.push(MenuItem::new(
            61,
            crate::i18n::t("explorer.context_menu.set_as_root"),
        ));
        items.push(MenuItem::separator());
        items.push(MenuItem::new(
            1,
            crate::i18n::t("explorer.context_menu.remove_from_favorites"),
        ));
        let binding = crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface_id);
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 explorer surface 가 닫히거나 바뀌었을 수 있다.
            // 루트 설정은 파일시스템을 바꾸지 않아 같은 explorer 인지만 본다.
            if !crate::state::explorer_menu::explorer_menu_admits(
                binding.as_ref(),
                engine,
                result == Some(61),
            ) {
                return;
            }
            match result {
                Some(60) => {
                    if is_mirror {
                        // 메뉴에서 숨겼으므로 정상 경로로는 도달하지 않는다 — 방어적 가드.
                        this.toast_remote_write_unsupported();
                    } else {
                        let params = serde_json::json!({ "path": path.to_string_lossy() });
                        if let Err(e) = this
                            .state
                            .add_kind_tab_by_owner(engine, surface_id, "explorer", &params)
                        {
                            tracing::warn!("explorer: open favorite in new tab failed: {e}");
                        }
                    }
                }
                Some(61) => {
                    this.state
                        .set_explorer_cwd(engine, surface_id, path.clone());
                }
                Some(1) => {
                    // 사이드바는 다음 프레임 스냅샷에서 갱신 — redraw 만 요청.
                    this.state.dispatch_intent(
                        crate::intent::Intent::Engine(
                            crate::app::engine_action::EngineAction::RemoveExplorerFavorite {
                                path: path.clone(),
                            },
                        )
                        .from_user_context_menu(),
                    );
                }
                _ => {}
            }
        });
    }

    fn handle_new_workspace_button_native_menu(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        let items = [
            MenuItem::new(1, crate::i18n::t("preset.context.apply_workspace_preset")),
            MenuItem::separator(),
            MenuItem::new(2, crate::i18n::t("context_menu.add_remote_workspace")),
        ];
        // 두 항목 모두 특정 대상에 매이지 않는 팝업 열기라 재확인할 대상이 없다.
        self.open_native_menu(
            engine,
            x,
            y,
            &items,
            move |this, _engine, result| match result {
                Some(1) => {
                    this.state.dialogs.preset_picker_selected = None;
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::preset_apply::APPLY_WORKSPACE_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                Some(2) => {
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::remote_attach::REMOTE_ATTACH_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                _ => {}
            },
        );
    }

    fn handle_new_tab_button_native_menu(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        pane_id: u32,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        let items = [
            MenuItem::new(1, crate::i18n::t("preset.context.apply_tab_preset")),
            MenuItem::new(2, crate::i18n::t("preset.context.apply_pane_preset")),
        ];
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 pane 이 닫혔을 수 있다.
            if !engine.has_pane(pane_id) {
                return;
            }
            match result {
                Some(1) => {
                    this.state.select_pane(engine, pane_id);
                    this.state.dialogs.preset_picker_selected = None;
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::preset_apply::APPLY_TAB_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                Some(2) => {
                    this.state.select_pane(engine, pane_id);
                    this.state.dialogs.preset_picker_selected = None;
                    this.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup {
                            id: crate::adapters::ui::popup::preset_apply::APPLY_PANE_POPUP_ID,
                            mode: crate::intent::OpenPopupMode::CenteredFocused,
                        }
                        .from_user_context_menu(),
                    );
                }
                _ => {}
            }
        });
    }
}

/// workspace 메뉴의 마지막 묶음. 에이전트 mirror에는 서버를 유지하는 연결 끊기(8)를 서버 닫기(6) 위에
/// 둔다. 저장된 원격 행은 `saved_row_menu`의 묶음을 쓴다.
fn workspace_close_menu_items(mirror: bool) -> Vec<crate::platform::native_menu::MenuItem> {
    use crate::platform::native_menu::MenuItem;
    let mut items = vec![MenuItem::separator()];
    if mirror {
        items.push(MenuItem::new(
            8,
            crate::i18n::t("context_menu.disconnect_workspace"),
        ));
    }
    items.push(MenuItem::new(
        6,
        crate::i18n::t("context_menu.close_workspace"),
    ));
    items
}

/// 카테고리 헤더 메뉴를 구성한다. 기본 카테고리만 이름 변경·삭제를 제외한다.
/// 구성과 순서는 아래 단위 테스트로 확인한다.
fn category_header_menu_items(is_normal: bool) -> Vec<crate::platform::native_menu::MenuItem> {
    use crate::platform::native_menu::MenuItem;
    let mut items = vec![
        MenuItem::new(3, crate::i18n::t("workspace_category.add_workspace")),
        MenuItem::new(5, crate::i18n::t("preset.context.apply_workspace_preset")),
        MenuItem::separator(),
        MenuItem::new(4, crate::i18n::t("context_menu.add_remote_workspace")),
        MenuItem::separator(),
    ];
    if !is_normal {
        items.push(MenuItem::new(
            1,
            crate::i18n::t("workspace_category.rename_category"),
        ));
        items.push(MenuItem::new(
            2,
            crate::i18n::t("workspace_category.delete_category"),
        ));
        items.push(MenuItem::separator());
    }
    items.push(MenuItem::new(
        100,
        crate::i18n::t("workspace_category.new_category"),
    ));
    items
}

#[cfg(test)]
mod tests {
    use super::{MAX_WEBVIEW_CREATE_ATTEMPTS, next_webview_attempts, should_attempt_webview};
    use super::{category_header_menu_items, workspace_close_menu_items};
    use super::{
        host_window_has_os_focus, take_hidden_webview_focus_targets,
        webview_release_candidate_is_live,
    };
    use crate::webview::WebViewCreateError;

    fn set(ids: &[u32]) -> std::collections::HashSet<u32> {
        ids.iter().copied().collect()
    }

    /// 활성 탭에서 빠진 surface만 한 번 회수 대상이 되고 다음 프레임에는 다시 나오지 않는다.
    #[test]
    fn a_surface_leaving_the_active_tab_is_released_once() {
        let mut pending = set(&[]);
        let got = take_hidden_webview_focus_targets(
            &set(&[1, 2]),
            &set(&[2]),
            |_| true,
            &mut pending,
            || true,
        );
        assert_eq!(got, vec![1]);
        let again = take_hidden_webview_focus_targets(
            &set(&[2]),
            &set(&[2]),
            |_| true,
            &mut pending,
            || true,
        );
        assert!(again.is_empty(), "같은 전이를 매 프레임 회수하면 안 된다");
    }

    /// 활성으로 남거나 새로 활성이 된 surface는 회수하지 않는다.
    #[test]
    fn staying_or_becoming_active_is_not_a_release() {
        let mut pending = set(&[]);
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[1, 3]),
            |_| true,
            &mut pending,
            || true,
        );
        assert!(got.is_empty());
    }

    /// 창이 OS 포커스를 갖지 않으면 회수를 미루고, 포커스를 되찾은 프레임에 한 번 회수한다.
    #[test]
    fn an_unfocused_window_defers_the_release_until_it_is_focused() {
        let mut pending = set(&[]);
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[]),
            |_| true,
            &mut pending,
            || false,
        );
        assert!(got.is_empty(), "배경 창에서는 OS 포커스를 건드리지 않는다");
        let got = take_hidden_webview_focus_targets(
            &set(&[]),
            &set(&[]),
            |_| true,
            &mut pending,
            || true,
        );
        assert_eq!(got, vec![1]);
        assert!(pending.is_empty());
    }

    /// 활성 탭의 surface를 닫은 프레임에는 webview가 아직 map에 남아 있어도 회수하지 않는다.
    #[test]
    fn closing_an_active_surface_is_not_a_release() {
        let mut pending = set(&[]);
        let all_html_ids: Vec<u32> = vec![];
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[]),
            |sid| webview_release_candidate_is_live(sid, &all_html_ids, |_| true),
            &mut pending,
            || true,
        );
        assert!(got.is_empty(), "닫힌 surface에 회수를 호출했다");
        assert!(pending.is_empty());
    }

    /// 다른 탭으로 전환된 surface는 목록과 map에 모두 남아 회수 대상이다.
    #[test]
    fn a_switched_away_surface_remains_a_release_candidate() {
        let mut pending = set(&[]);
        let all_html_ids: Vec<u32> = vec![1];
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[]),
            |sid| webview_release_candidate_is_live(sid, &all_html_ids, |_| true),
            &mut pending,
            || true,
        );
        assert_eq!(got, vec![1]);
    }

    /// 미룬 사이 다시 활성이 되었거나 닫힌 surface는 회수 대상에서 빠진다.
    #[test]
    fn a_deferred_release_is_dropped_when_the_surface_returns_or_closes() {
        let mut pending = set(&[]);
        take_hidden_webview_focus_targets(
            &set(&[1, 2]),
            &set(&[]),
            |_| true,
            &mut pending,
            || false,
        );
        let got = take_hidden_webview_focus_targets(
            &set(&[]),
            &set(&[1]),
            |sid| sid != 2,
            &mut pending,
            || true,
        );
        assert!(
            got.is_empty(),
            "돌아온 surface와 닫힌 surface는 회수하지 않는다"
        );
        assert!(pending.is_empty());
    }

    /// 회수할 surface가 없는 프레임에는 OS에 포커스를 묻지 않는다.
    #[test]
    fn nothing_pending_does_not_query_the_os_focus() {
        let mut pending = set(&[]);
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[1]),
            |_| true,
            &mut pending,
            || panic!("회수 대상이 없는데 포커스를 물었다"),
        );
        assert!(got.is_empty());
    }

    /// winit이 포커스를 잃었다고 해도 webview 자식이 포커스를 쥐었으면 창은 활성이다.
    #[test]
    fn a_webview_child_holding_focus_counts_as_an_active_window() {
        assert!(host_window_has_os_focus(false, || true));
        assert!(!host_window_has_os_focus(false, || false));
        assert!(host_window_has_os_focus(true, || panic!(
            "winit 값이 참이면 OS에 묻지 않는다"
        )));
    }

    /// X focus가 자식 안에 있는 상태(winit false)에서 탭을 바꾸면 미루지 않고 바로 회수한다.
    #[test]
    fn a_tab_switch_while_the_child_holds_focus_is_released_at_once() {
        let mut pending = set(&[]);
        let got = take_hidden_webview_focus_targets(
            &set(&[1]),
            &set(&[]),
            |_| true,
            &mut pending,
            || host_window_has_os_focus(false, || true),
        );
        assert_eq!(got, vec![1]);
    }

    /// 영구 실패는 첫 시도 뒤 재시도를 멈춘다.
    #[test]
    fn a_permanent_failure_gives_up_at_once() {
        let err = WebViewCreateError::Permanent("no display".into());
        let next = next_webview_attempts(0, &err);
        assert_eq!(next, MAX_WEBVIEW_CREATE_ATTEMPTS);
        assert!(!should_attempt_webview(next), "재시도를 중단해야 한다");
    }

    /// 일시 실패의 재시도가 정해진 상한에서 멈추는지 확인한다.
    #[test]
    fn a_transient_failure_stops_exactly_at_the_cap() {
        let err = WebViewCreateError::Transient("server busy".into());
        let mut attempts = 0;
        let mut tried = 0;
        while should_attempt_webview(attempts) {
            tried += 1;
            attempts = next_webview_attempts(attempts, &err);
            assert!(tried <= 1000, "상한이 없다 — 무한 재시도");
        }
        assert_eq!(
            tried, MAX_WEBVIEW_CREATE_ATTEMPTS,
            "시도 횟수가 상한과 다르다"
        );
    }

    /// 첫 실패부터 영구 실패와 일시 실패의 재시도 횟수가 달라야 한다.
    #[test]
    fn the_cap_leaves_room_for_the_two_prescriptions_to_differ() {
        let t = next_webview_attempts(0, &WebViewCreateError::Transient("x".into()));
        let p = next_webview_attempts(0, &WebViewCreateError::Permanent("x".into()));
        assert_ne!(t, p, "첫 실패에서 두 종류가 같은 결과를 낸다");
    }

    /// 라벨은 i18n 상태에 좌우되므로 id·separator 위치로 구성·순서를 고정한다.
    fn shape(items: &[crate::platform::native_menu::MenuItem]) -> Vec<Option<u32>> {
        items
            .iter()
            .map(|i| (!i.is_separator()).then_some(i.id))
            .collect()
    }

    #[test]
    fn category_header_menu_non_normal_order() {
        // Add workspace · Create from preset · ─ · Add remote workspace · ─ ·
        // Rename · Delete · ─ · New category.
        let items = category_header_menu_items(false);
        assert_eq!(
            shape(&items),
            vec![
                Some(3),
                Some(5),
                None,
                Some(4),
                None,
                Some(1),
                Some(2),
                None,
                Some(100)
            ]
        );
    }

    /// mirror 카드만 서버 닫기와 함께 연결 끊기를 보이고, 로컬 카드에는 닫기만 둔다.
    #[test]
    fn only_a_mirror_card_offers_disconnect_beside_close() {
        assert_eq!(
            shape(&workspace_close_menu_items(true)),
            vec![None, Some(8), Some(6)]
        );
        assert_eq!(
            shape(&workspace_close_menu_items(false)),
            vec![None, Some(6)]
        );
        let labels: Vec<String> = workspace_close_menu_items(true)
            .iter()
            .filter(|item| !item.is_separator())
            .map(|item| item.label.clone())
            .collect();
        assert_eq!(
            labels,
            vec![
                crate::i18n::t("context_menu.disconnect_workspace").to_string(),
                crate::i18n::t("context_menu.close_workspace").to_string()
            ]
        );
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn category_header_menu_normal_is_additive_only() {
        // reserved normal: Add workspace · Create from preset · ─ · Add remote
        // workspace · ─ · New category (rename/delete 금지).
        let items = category_header_menu_items(true);
        assert_eq!(
            shape(&items),
            vec![Some(3), Some(5), None, Some(4), None, Some(100)]
        );
    }

    // build_explorer_context_menu(multi, is_empty_target, is_folder, has_clip, is_mirror) 의
    // 위치별 메뉴 구성을 id·separator 위치로 고정한다. id: 1=copy_path, 10=copy_files,
    // 11=cut, 12=paste/paste_into, 20=open_in_system, 21=reveal, 30=delete, 40=rename,
    // 50=add_to_favorites, 60=open_in_new_tab, 61=set_as_root, 70=properties, 80=new_folder,
    // 81=new_file. None=separator.
    // 모든 모양이 구분선과 Properties 로 끝나는지 여기서 확인하고, 아래 기대값은 그 앞부분만 적는다.
    fn explorer_menu_shape(
        multi: bool,
        is_empty_target: bool,
        is_folder: bool,
        has_clip: bool,
    ) -> Vec<Option<u32>> {
        explorer_menu_shape_mirror(multi, is_empty_target, is_folder, has_clip, false)
    }

    fn explorer_menu_shape_mirror(
        multi: bool,
        is_empty_target: bool,
        is_folder: bool,
        has_clip: bool,
        is_mirror: bool,
    ) -> Vec<Option<u32>> {
        let mut items = shape(&super::MainView::build_explorer_context_menu(
            multi,
            is_empty_target,
            is_folder,
            has_clip,
            (is_mirror, is_mirror.then_some("reason")),
        ));
        assert_eq!(items.split_off(items.len() - 2), vec![None, Some(70)]);
        items
    }

    #[test]
    fn explorer_menu_empty_no_clip() {
        // 빈 영역, 클립보드 없음: 새 폴더 · 새 파일 · ─ · 경로복사 · 즐겨찾기추가.
        assert_eq!(
            explorer_menu_shape(false, true, false, false),
            vec![Some(80), Some(81), None, Some(1), Some(50)]
        );
    }

    #[test]
    fn explorer_menu_empty_with_clip() {
        // 빈 영역, 클립보드 있음: + ─ · 붙여넣기.
        assert_eq!(
            explorer_menu_shape(false, true, false, true),
            vec![Some(80), Some(81), None, Some(1), Some(50), None, Some(12)]
        );
    }

    #[test]
    fn explorer_menu_single_file() {
        // 단일 파일(클립보드 무관): 경로복사 · 복사 · 잘라내기 · 이름변경 · ─ · 휴지통 · ─ ·
        // 시스템열기 · 파일 관리자에서 보기.
        let shape = vec![
            Some(1),
            Some(10),
            Some(11),
            Some(40),
            None,
            Some(30),
            None,
            Some(20),
            Some(21),
        ];
        assert_eq!(explorer_menu_shape(false, false, false, false), shape);
        // has_clip 은 단일 파일 shape 에 영향 없음.
        assert_eq!(explorer_menu_shape(false, false, false, true), shape);
    }

    #[test]
    fn explorer_menu_single_folder_no_clip() {
        // 단일 폴더, 클립보드 없음: 경로복사 · 즐겨찾기 · 새탭 · 루트설정 · 복사 ·
        // 잘라내기 · ─ · 새 폴더 · 새 파일 · 이름변경 · ─ · 휴지통 · ─ · 시스템열기 · 파일 관리자에서 보기.
        assert_eq!(
            explorer_menu_shape(false, false, true, false),
            vec![
                Some(1),
                Some(50),
                Some(60),
                Some(61),
                Some(10),
                Some(11),
                None,
                Some(80),
                Some(81),
                Some(40),
                None,
                Some(30),
                None,
                Some(20),
                Some(21)
            ]
        );
    }

    #[test]
    fn explorer_menu_single_folder_with_clip() {
        // 단일 폴더, 클립보드 있음: 위 + 붙여넣기(12, cut/paste 그룹 안).
        assert_eq!(
            explorer_menu_shape(false, false, true, true),
            vec![
                Some(1),
                Some(50),
                Some(60),
                Some(61),
                Some(10),
                Some(11),
                Some(12),
                None,
                Some(80),
                Some(81),
                Some(40),
                None,
                Some(30),
                None,
                Some(20),
                Some(21)
            ]
        );
    }

    #[test]
    fn explorer_menu_multi() {
        // 다중 선택(클립보드 무관): 경로복사 · 복사 · 잘라내기 · ─ · 휴지통 · ─ · 시스템열기(모두 연다).
        // 파일 관리자에서 보기는 한 항목만 고르므로 없다.
        assert_eq!(
            explorer_menu_shape(true, false, false, false),
            vec![Some(1), Some(10), Some(11), None, Some(30), None, Some(20)]
        );
        assert_eq!(
            explorer_menu_shape(true, false, false, true),
            vec![Some(1), Some(10), Some(11), None, Some(30), None, Some(20)]
        );
    }

    // 쓸 수 없는 mirror explorer 는 파일을 바꾸는 행(붙여넣기/잘라내기/새 폴더/새 파일/이름변경/
    // 휴지통)을 숨기지 않고 꺼 두며 이유를 툴팁으로 단다. 로컬 전용 행(새탭으로 열기/시스템에서
    // 열기)은 mirror 에서 숨는다. copy_path/복사/즐겨찾기 추가/이 폴더로 루트 설정은 그대로 켜져 있다.

    /// mirror 메뉴에서 꺼진 행의 id. 꺼진 행은 모두 같은 이유 툴팁을 단다.
    fn explorer_menu_mirror_disabled(
        multi: bool,
        is_empty_target: bool,
        is_folder: bool,
        has_clip: bool,
    ) -> Vec<u32> {
        let items = super::MainView::build_explorer_context_menu(
            multi,
            is_empty_target,
            is_folder,
            has_clip,
            (true, Some("reason")),
        );
        items
            .iter()
            .filter(|i| !i.is_separator() && !i.enabled)
            .inspect(|i| assert_eq!(i.tooltip.as_deref(), Some("reason"), "row {}", i.id))
            .map(|i| i.id)
            .collect()
    }

    #[test]
    fn explorer_menu_mirror_empty_with_clip_disables_create_and_paste() {
        // 빈 영역 + 클립보드 있음 + mirror: 로컬과 같은 모양에서 생성 행(80·81)과 붙여넣기(12)가 꺼진다.
        assert_eq!(
            explorer_menu_shape_mirror(false, true, false, true, true),
            vec![Some(80), Some(81), None, Some(1), Some(50), None, Some(12)]
        );
        assert_eq!(
            explorer_menu_mirror_disabled(false, true, false, true),
            vec![80, 81, 12]
        );
    }

    #[test]
    fn explorer_menu_mirror_single_file_disables_cut_rename_delete() {
        // 단일 파일 + mirror: 로컬과 같은 모양에서 잘라내기(11)/이름변경(40)/휴지통(30)이 꺼진다.
        assert_eq!(
            explorer_menu_shape_mirror(false, false, false, false, true),
            vec![Some(1), Some(10), Some(11), Some(40), None, Some(30)]
        );
        assert_eq!(
            explorer_menu_mirror_disabled(false, false, false, false),
            vec![11, 40, 30]
        );
    }

    #[test]
    fn explorer_menu_mirror_single_folder_disables_write_ops() {
        // 단일 폴더 + 클립보드 있음 + mirror: 새탭(60)/시스템열기(20)/파일 관리자에서 보기(21)는
        // 사라지고, 쓰기 행은 꺼진다.
        assert_eq!(
            explorer_menu_shape_mirror(false, false, true, true, true),
            vec![
                Some(1),
                Some(50),
                Some(61),
                Some(10),
                Some(11),
                Some(12),
                None,
                Some(80),
                Some(81),
                Some(40),
                None,
                Some(30)
            ]
        );
        assert_eq!(
            explorer_menu_mirror_disabled(false, false, true, true),
            vec![11, 12, 80, 81, 40, 30]
        );
    }

    #[test]
    fn explorer_menu_mirror_multi_disables_cut_delete() {
        // 다중 선택 + mirror: 로컬과 같은 모양에서 잘라내기(11)/휴지통(30)이 꺼진다.
        assert_eq!(
            explorer_menu_shape_mirror(true, false, false, false, true),
            vec![Some(1), Some(10), Some(11), None, Some(30)]
        );
        assert_eq!(
            explorer_menu_mirror_disabled(true, false, false, false),
            vec![11, 30]
        );
    }
}
