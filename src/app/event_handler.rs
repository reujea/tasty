mod window_helpers;

use crate::runtime::engine_access::EngineMut;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::adapters::ui::input::synthetic::is_synthetic_key_event;
use crate::app::timers::{Tick, min_deadline};
use crate::app::window_access::engines_mut;
use crate::stall_watchdog::{self, Site};
use crate::view::ui::View;
use crate::view::{RepaintSource, ViewAction, ViewCtx};
use crate::{App, AppEvent};
use tasty_ipc::stream_hub::{PumpOutcome, StreamHub};

impl ApplicationHandler<AppEvent> for App {
    #[cfg(target_os = "linux")]
    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let _stall_guard = stall_watchdog::Guard::enter(Site::DeviceEvent);
        self.observe_explorer_device_key(event_loop, event);
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        let _stall_guard = stall_watchdog::Guard::enter(Site::NewEvents);
        #[cfg(debug_assertions)]
        window_helpers::record_wake(_cause, self.state.shutdown.is_some());
    }

    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 이벤트별 핸들러에 위임하는 평면 match이며 플랫폼 cfg를 포함한다. 분기를 한곳에서 볼 수 있도록 유지한다.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let _stall_guard = stall_watchdog::Guard::enter(Site::UserEvent);
        #[cfg(debug_assertions)]
        let _measurement = super::loop_measurement::Guard::enter(
            Site::UserEvent as u8,
            self.state.shutdown.is_some(),
        );
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let AppEvent::OutboundDragEnded { id, .. } = &event {
            crate::file::drag::desktop::release_completed(*id);
        }
        // 종료 중에는 정리된 상태를 다시 건드리지 않도록 새 이벤트를 버린다.
        if self.state.shutdown.is_some() {
            return;
        }

        if self.journal.is_halted()
            && !matches!(
                event,
                AppEvent::Shutdown
                    | AppEvent::QuitRequested
                    | AppEvent::IpcReady
                    | AppEvent::JournalReady
            )
        {
            match event {
                AppEvent::EguiRepaint { window_id } => self.request_halted_repaint(window_id),
                // 타이틀바 닫기 버튼은 OS 닫기와 같은 경로로 창을 닫는다.
                AppEvent::CloseWindow(id) => self.request_halted_close(id, event_loop),
                _ => {}
            }
            return;
        }

        if self.journal.pauses_observation()
            && !self.journal.is_halted()
            && !matches!(
                event,
                AppEvent::JournalReady
                    | AppEvent::Shutdown
                    | AppEvent::QuitRequested
                    | AppEvent::CloseWindow(_)
            )
        {
            self.defer_publication_event(crate::app::publication_input::DeferredEvent::App(event));
            return;
        }

        // 부팅 중 출력 이벤트를 소비하면 engine이 아직 views에 없어 wake를 잃을 수 있다. 완료 뒤 재생한다.
        if let Some(boot) = self.boot.as_mut()
            && !matches!(
                event,
                AppEvent::Shutdown | AppEvent::QuitRequested | AppEvent::JournalReady
            )
        {
            boot.pending_events.push(event);
            return;
        }
        match event {
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            AppEvent::OutboundDragEnded { window, id, result } => {
                if let Some(main) = self
                    .view
                    .views
                    .get_mut(&window)
                    .and_then(|view| view.as_main_mut())
                    && let Some(engine) = self.engines.window_read(window)
                {
                    main.finish_outbound_request(id, result, &engine);
                }
            }
            AppEvent::JournalReady => {
                if let Some(boot) = &self.boot {
                    boot.window.request_redraw();
                }
            }
            AppEvent::CreateWindow(origin, completion) => {
                let outcome = self.create_new_window(event_loop, origin);
                if let Some(pending) = self.pending_window.as_mut()
                    && outcome.as_ref().is_ok_and(|id| *id == pending.window.id())
                {
                    pending.completion = completion;
                } else if let Some(completion) = completion {
                    completion.reply_window_create(outcome.map(u64::from));
                }
            }
            AppEvent::RunLuaScript {
                source,
                name,
                window,
            } => self.submit_shortcut_script(window, &source, name),
            AppEvent::OpenSettings => {
                self.open_settings_modal(event_loop);
            }
            AppEvent::OpenPlugins => {
                self.open_plugins_modal(event_loop);
            }
            AppEvent::TerminalOutput(surface_id) => self.handle_terminal_output(surface_id),
            AppEvent::IpcReady => {
                // IPC 연속 이벤트가 타이머·입력·렌더 처리를 막지 않도록 회차 간격을 지킨다.
                if self
                    .ipc_pacer
                    .event_may_run_round(std::time::Instant::now())
                    && self.process_ipc()
                    && let Some(w) = self.focused_window_mut()
                {
                    w.mark_dirty();
                }
            }
            AppEvent::StreamReady => {
                self.run_stream_round(super::stream_round::StreamRoundSite::Event);
            }
            #[cfg(all(windows, feature = "gui"))]
            AppEvent::SystemResumed => {
                self.resume_health_pass();
            }
            AppEvent::EguiRepaint { window_id } => self.deliver_egui_repaint(window_id),
            AppEvent::EguiRepaintAfter { window_id, at } => {
                self.schedule_egui_repaint(window_id, at)
            }
            AppEvent::Shutdown => {
                self.begin_shutdown(event_loop);
            }
            AppEvent::Minimize => self.handle_minimize(),
            AppEvent::QuitRequested => {
                self.handle_quit_requested(event_loop);
            }
            AppEvent::CloseWindow(id) => {
                self.request_close_window(id, event_loop);
            }
            // Windows·Linux는 숨긴 창을 재사용한다. macOS는 parked 상태로 새 창을 만든다.
            #[cfg(any(windows, target_os = "linux"))]
            AppEvent::TrayShowWindow => {
                for w in self.view.views.values() {
                    w.base().winit.set_visible(true);
                    w.base().winit.set_minimized(false);
                    w.base().winit.focus_window();
                }
                tracing::info!(
                    "restored {} window(s) from system tray",
                    self.view.views.len()
                );
            }
            #[cfg(any(windows, target_os = "linux"))]
            AppEvent::ExternalActivate(request) => {
                self.handle_external_activation(event_loop, request);
            }
            // 타이머 실행은 이어지는 about_to_wait가 담당한다.
            AppEvent::TimerTick => {
                if !self.journal.is_halted() && !self.journal.pauses_observation() {
                    for session in self.engines.all_sessions_mut() {
                        session.borrow_mut().poll_input_submissions();
                    }
                }
            }
            AppEvent::AttachClientData => {
                self.run_attach_client_round(super::stream_round::StreamRoundSite::Event);
            }
            AppEvent::AutoAttachReady => {
                self.poll_remote_browsers();
                self.drain_auto_attach_results();
                self.drain_into_gui_checks();
            }
            AppEvent::ScreenshotCaptureReady => {
                self.drain_screenshot_capture_results();
            }
            AppEvent::ImageUploadReady => {
                self.drain_image_upload_results();
            }
            AppEvent::TransferProgressTick => {
                self.drain_transfer_progress();
            }
            AppEvent::IdentifyDone {
                request_id,
                target,
                detector,
                origin_surface_id,
                dispatch_origin,
                ignore_size_limit,
                run,
            } => self.handle_identify_done(
                request_id,
                target,
                detector,
                origin_surface_id,
                crate::file::dispatch::FileDispatchPolicy {
                    dispatch_origin,
                    ignore_size_limit,
                    run,
                },
            ),
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.view.display_owner = Some(event_loop.owned_display_handle());
        stall_watchdog::install_pending_probe(window_helpers::pending_ipc_work);
        let _stall_guard = stall_watchdog::Guard::enter(Site::Resumed);
        #[cfg(debug_assertions)]
        let _measurement = super::loop_measurement::Guard::enter(
            Site::Resumed as u8,
            self.state.shutdown.is_some(),
        );
        // resumed가 다시 호출돼도 부팅 중 아직 views가 비어 있는 창을 중복 생성하지 않는다.
        if !self.view.views.is_empty()
            || self.shell_setup_gpu.is_some()
            || self.boot.is_some()
            || self.boot_error_gpu.is_some()
        {
            return;
        }

        #[cfg(target_os = "macos")]
        crate::macos_delegate::inject_delegate_methods();

        let boot_t0 = std::time::Instant::now();

        // 첫 로딩 프레임을 그리기 전에 OS 기본 배경이 드러나지 않도록 hidden으로 만든다.
        let window = Self::boot_create_hidden_window(event_loop, boot_t0);

        let init_settings = Self::boot_load_normalized_settings();

        let gpu = self.try_init_boot_gpu(&window, &init_settings.appearance);

        let (window, gpu) = match self.enter_shell_setup_if_needed(&init_settings, window, gpu) {
            Some(pair) => pair,
            None => return,
        };

        window.set_ime_allowed(true);
        self.begin_boot(window, gpu, init_settings, boot_t0, true);

        #[cfg(windows)]
        crate::jump_list::setup_jump_list();

        // 창을 다시 만들더라도 트레이는 하나만 유지한다. 생성할 수 없으면 taskbar·dock 최소화로 동작한다.
        #[cfg(all(
            any(windows, target_os = "macos", target_os = "linux"),
            feature = "gui"
        ))]
        self.ensure_tray_icon_once();

        tracing::info!(
            target: "tasty::boot",
            ms = boot_t0.elapsed().as_secs_f64() * 1000.0,
            "resumed_total (T1~T2 + T2.5 + 첫 로딩 프레임 — 상태 머신 전개로 T2.6~T6 은 boot_total 로 이동)"
        );
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // GPU 렌더에서 멈춘 경우를 다른 콜백의 지연과 구별한다.
        let _stall_guard =
            stall_watchdog::Guard::enter(if matches!(event, WindowEvent::RedrawRequested) {
                Site::Redraw
            } else {
                Site::WindowEvent
            });
        #[cfg(debug_assertions)]
        let _measurement = super::loop_measurement::Guard::enter(
            if matches!(event, WindowEvent::RedrawRequested) {
                Site::Redraw
            } else {
                Site::WindowEvent
            } as u8,
            self.state.shutdown.is_some(),
        );
        // 포커스를 얻거나 잃을 때 winit이 합성한 키는 사용자가 이 창에서 누른 입력이 아니다.
        // 모든 창·모드로 보내기 전에 제외해 단축키·PTY·egui에 전달되지 않게 한다.
        if is_synthetic_key_event(&event) {
            return;
        }

        if self.state.boot_error_mode {
            self.handle_boot_error_window_event(event);
            return;
        }

        if self.state.shell_setup_mode {
            self.handle_shell_setup_window_event(event_loop, event);
            return;
        }

        if self.state.shutdown.is_some() {
            self.handle_shutdown_window_event(event_loop, id, event);
            return;
        }

        if self.boot.is_some() {
            self.handle_boot_window_event(event_loop, event);
            return;
        }

        if self.journal.pauses_observation()
            && !self.journal.is_halted()
            && !matches!(event, WindowEvent::CloseRequested)
        {
            let target = self.capture_publication_input(id, &event);
            self.defer_publication_event(crate::app::publication_input::DeferredEvent::Window {
                window: id,
                event,
                target,
            });
            return;
        }

        if self.journal.is_halted() && !matches!(event, WindowEvent::CloseRequested) {
            self.handle_halted_window_event(id, event);
            return;
        }

        if let Some(modal_id) = self.view.active_modal_id()
            && id == modal_id
        {
            self.handle_active_modal_window_event(id, event);
            return;
        }

        if let WindowEvent::CloseRequested = &event {
            if self.journal.is_halted() {
                self.request_halted_close(id, event_loop);
            } else {
                self.request_close_window(id, event_loop);
            }
            return;
        }

        self.apply_window_focus_observation(id, &event);

        if self.view.is_modal_active() {
            let is_mouse_press = matches!(
                &event,
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    ..
                }
            );
            if is_mouse_press {
                self.trigger_modal_shake();
            }
        }

        // 포커스된 plugin surface의 단축키를 호스트 action보다 먼저 처리한다.
        let plugin_consumed = if let WindowEvent::KeyboardInput { event: ke, .. } = &event {
            self.try_plugin_shortcut(id, ke)
        } else {
            false
        };
        if plugin_consumed {
            return;
        }

        self.dispatch_window_event_to_view(id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let _stall_guard = stall_watchdog::Guard::enter(Site::AboutToWait);
        #[cfg(debug_assertions)]
        let _measurement = super::loop_measurement::Guard::enter(
            Site::AboutToWait as u8,
            self.state.shutdown.is_some(),
        );
        self.poll_retiring_engine_owners();
        // 부팅·종료 중에는 일반 타이머를 처리하지 않고 각 상태 머신이 대기를 정한다.
        // 평상시 대기 시각은 말미에서 타이머·지연 repaint를 함께 반영한다.

        // exit 요청 뒤에도 콜백이 올 수 있어 종료 가드를 유지한다.
        // 종료 단계는 새 IPC를 실행하지 않고 종료 중이라는 응답을 보낸다.
        if self.state.shutdown.is_some() {
            self.drive_shutdown_frame(event_loop);
            event_loop.set_control_flow(if self.shutdown_needs_frames() {
                winit::event_loop::ControlFlow::WaitUntil(
                    std::time::Instant::now()
                        + crate::app::shutdown_machine::SHUTDOWN_FRAME_INTERVAL,
                )
            } else {
                winit::event_loop::ControlFlow::Wait
            });
            return;
        }

        // redraw가 오지 않아도 다음 부팅 단계를 진행하도록 대기를 예약한다.
        if self.boot.is_some() {
            self.drive_boot_frame(event_loop);
            event_loop.set_control_flow(if self.boot.is_some() {
                winit::event_loop::ControlFlow::WaitUntil(
                    std::time::Instant::now() + crate::app::boot_machine::BOOT_FRAME_INTERVAL,
                )
            } else {
                winit::event_loop::ControlFlow::Wait
            });
            return;
        }

        if self.plugin_files_pending() != 0 {
            self.collect_plugin_file_shutdown();
        }

        if self.journal.is_halted() {
            self.surface_halt_notice();
            self.ipc_pacer.loop_reached_about_to_wait();
            self.process_ipc();
            event_loop.set_control_flow(self.halted_wake_deadline().map_or(
                winit::event_loop::ControlFlow::Wait,
                winit::event_loop::ControlFlow::WaitUntil,
            ));
            return;
        }

        self.poll_journal_application();
        if !self.journal.pauses_observation() {
            self.poll_pending_window();
        }
        if self.journal.is_halted() {
            self.surface_halt_notice();
            event_loop.set_control_flow(self.halted_wake_deadline().map_or(
                winit::event_loop::ControlFlow::Wait,
                winit::event_loop::ControlFlow::WaitUntil,
            ));
            return;
        }

        if self.journal.pauses_observation() {
            self.sync_timer_control_flow(event_loop);
            return;
        }
        self.resume_publication_events(event_loop);
        if self.state.shutdown.is_some()
            || self.journal.is_halted()
            || self.journal.pauses_observation()
        {
            return;
        }

        self.poll_port_scans();
        self.poll_explorer_files();
        self.sync_mirror_locations();
        self.services.poll_memory_writer();
        self.collect_plugin_file_shutdown();
        self.services.poll_state_writer();
        self.settle_settings_saves();
        self.run_telemetry_follow_ups();
        self.poll_local_reads();
        self.poll_dag_reads();
        self.poll_profile_detections();
        self.poll_system_notices();
        for mut engine in self.engines_mut().windowed_and_parked() {
            engine.poll_attach_subscriptions();
        }

        // Lua 자동실행 재진입 상태는 이번 회차의 모든 이벤트 처리 전에 갱신한다.
        self.lua_autofire.checkpoint();
        self.ipc_pacer.loop_reached_about_to_wait();

        // 여기서 바뀐 상태와 이벤트가 같은 회차의 후속 처리에 포함되도록 타이머를 먼저 실행한다.
        let now = std::time::Instant::now();
        for key in self.timers.drain_due(now) {
            match key {
                Tick::Busy => {
                    self.poll_busy_states();
                    self.poll_global_hooks();
                    self.poll_idle_timeout_hooks();
                    self.poll_shell_integration_hints();
                }
                Tick::AttachView => self.poll_attach_views(),
                Tick::LayoutFlush => self.flush_layout_persistence(false),
                Tick::DagGraph(sid) => self.mark_dag_graph_window_dirty(sid),
                Tick::DagListPopup => self.mark_dag_list_popup_windows_dirty(),
                Tick::EguiRepaint(window_id) => self.deliver_egui_repaint(window_id),
                // 루프만 깨운다. 아래 poll_auto_attach가 재연결을, sync_surface_poll_timers가 탐색기 확인을 판단한다.
                Tick::Reconnect(_) | Tick::ExplorerPoll => {}
                // 아래 poll_attach_resize_sync가 마감을 처리한다.
                Tick::AttachResizeAck => {}
                // 아래의 메뉴 폴링이 실행되도록 깨운다.
                Tick::NativeMenu => {}
                // Linux의 별도 WebView 이벤트 큐를 확인하도록 깨운다.
                Tick::WebviewKeyPoll => {}
                // 접근 시점의 lazy 정리를 보완한다.
                Tick::PtySweep => self.poll_pty_sweep(),
                Tick::CaptureSweep => self.poll_capture_sweep(),
                Tick::LogPrune => self.poll_log_prune(),
            }
        }

        if self.process_ipc()
            && let Some(w) = self.focused_window_mut()
        {
            w.mark_dirty();
        }
        // 사용자 이벤트에서 간격 때문에 미룬 스트림 회차를 여기서 처리한다.
        self.run_deferred_stream_rounds();

        // 방금 IPC가 만든 attach 요청을 이번 회차에 처리한다.
        self.drain_into_gui_checks();
        self.dispatch_pending_gui_attach();

        // 사용자가 닫은 mirror의 연결도 정리해야 원격 점유가 남지 않는다.
        self.detach_orphaned_mirror_sessions();

        self.dispatch_pending_resize_forwards();
        self.poll_attach_resize_sync(std::time::Instant::now());

        self.dispatch_pending_list_dir_forwards();
        self.dispatch_pending_git_query_forwards();
        self.dispatch_pending_markdown_content_forwards();
        self.pump_terminal_reads();
        self.pump_file_downloads();
        self.dispatch_pending_mesh_full_resend_forwards();
        self.dispatch_pending_attention_clear_forwards();

        self.dispatch_pending_mesh_context_forwards();
        self.dispatch_pending_mesh_input_forwards();
        self.sync_attach_waits();

        self.poll_remote_browsers();
        self.poll_auto_attach();

        self.poll_screenshot_captures();

        self.poll_image_uploads();
        self.poll_image_jobs();

        let hello_pairs = if let Some(ref mut mgr) = self.plugin_manager {
            mgr.pump(now)
        } else {
            Vec::new()
        };
        self.finalize_plugin_hello(hello_pairs);
        self.report_plugin_failures();
        self.record_plugin_rss_samples_if_present();
        self.forward_mesh_frames_for_parked();
        self.mark_plugin_surfaces_dirty();
        self.mark_invalidated_popups_dirty();
        self.mark_invalidated_banners_dirty();
        self.process_plugin_ipc_calls();
        self.dispatch_pending_surface_lifecycle();
        self.dispatch_pending_host_events();
        self.dispatch_pending_memory_changes();
        self.dispatch_pending_agent_events();
        self.dispatch_pending_tool_events();
        self.dispatch_pending_palette_plugin_commands();
        self.dispatch_pending_intents();
        self.publish_lua_snapshot();
        self.dispatch_pending_lua_commands();
        self.settle_lua_runs();
        self.dispatch_pending_popup_opens();
        self.dispatch_pending_surface_popups();
        self.dispatch_pending_handler_ipc();
        self.dispatch_pending_picker_results();
        self.dispatch_pending_info_modal_requests();
        self.dispatch_pending_file_picker_results();
        self.dispatch_pending_script_confirm();
        self.dispatch_plugin_popup_events();
        self.process_plugins_window_actions();
        self.process_pending_open_preset_window(event_loop);

        self.poll_tray_menu_events();

        // 네이티브 WebView의 키 이벤트는 winit KeyboardInput과 별도 경로로 들어온다.
        let needs_key_poll = self.pump_webview_key_events();
        // Linux GDK는 별도 연결로 이벤트를 받아 winit을 깨우지 못하므로 폴링을 예약한다.
        crate::app::timers::reschedule_webview_key_poll(
            &mut self.timers,
            needs_key_poll,
            std::time::Instant::now(),
        );

        self.poll_pending_native_menus();

        // 긴 회차 뒤에도 미래 시각을 예약하도록 현재 시각을 다시 읽는다.
        let has_pending_menu = self.any_pending_native_menu();
        crate::app::timers::reschedule_pending_menu_poll(
            &mut self.timers,
            has_pending_menu,
            std::time::Instant::now(),
        );

        self.tick_modal_shake();

        let dirty_since = self.earliest_layout_dirty_since();
        crate::app::timers::sync_layout_flush_timer(&mut self.timers, dirty_since, now);

        self.sync_surface_poll_timers(now);

        self.sync_reconnect_timers(now);

        self.flush_pending_pty_resizes();

        // 타이머 대기를 먼저 정한 뒤 지연 repaint 시각과 이른 쪽으로 합친다.
        self.sync_timer_control_flow(event_loop);

        // 이번 회차의 모든 dirty 요청을 확인한 뒤 미룬 repaint를 실행·예약해야 한다.
        self.drive_deferred_repaints(event_loop);
    }
}

impl App {
    /// GPU 초기화 전에 설정값을 정규화한다. 저장은 테마 조정 뒤 같은 writer에 제출한다.
    fn boot_load_normalized_settings() -> crate::settings::Settings {
        let mut settings = crate::settings::Settings::load();
        settings.normalize();
        // begin_boot submits one final snapshot after shell/theme adjustments.
        settings
    }

    /// 어댑터 부재는 진단 후 실패 종료한다. 그 밖의 오류는 panic으로 보고한다.
    /// 엔진 초기화 실패처럼 창·GPU로 오류를 그릴 수 있는 경로와 구별한다.
    fn try_init_boot_gpu(
        &mut self,
        window: &std::sync::Arc<winit::window::Window>,
        appearance: &crate::settings::AppearanceSettings,
    ) -> crate::gpu::GpuState {
        let t2 = std::time::Instant::now();
        let gpu = match self.create_gpu_state(window.clone(), appearance) {
            Ok(gpu) => gpu,
            Err(e) if e.downcast_ref::<crate::app::NoGpuAdapter>().is_some() => {
                tracing::error!(
                    "boot gpu init failed: no compatible adapter\n{}\n{}\n{}",
                    crate::i18n::t("boot.gpu_error.title"),
                    crate::i18n::t("boot.gpu_error.body"),
                    crate::i18n::t("boot.gpu_error.hint"),
                );
                std::process::exit(1);
            }
            Err(e) => panic!("failed to initialize GPU: {e}"),
        };
        tracing::info!(
            target: "tasty::boot",
            ms = t2.elapsed().as_secs_f64() * 1000.0,
            "T2 gpu_init (create_gpu_state)"
        );
        gpu
    }

    /// 셸을 사용할 수 없고 bash도 찾지 못하면 setup 화면으로 소유권을 옮겨 None을 반환한다.
    fn enter_shell_setup_if_needed(
        &mut self,
        settings: &crate::settings::Settings,
        window: std::sync::Arc<winit::window::Window>,
        gpu: crate::gpu::GpuState,
    ) -> Option<(std::sync::Arc<winit::window::Window>, crate::gpu::GpuState)> {
        if boot_shell_usable(settings) {
            return Some((window, gpu));
        }
        // 설정한 경로를 입력란에 그대로 넣어 판정 줄이 원인(경로 없음·폴더·실행 권한 없음)을 바로 보인다.
        self.enter_shell_setup_mode(window, gpu, settings.general.shell.clone());
        None
    }

    /// setup 첫 렌더가 실패해도 창은 표시해 숨긴 채 남지 않게 한다.
    fn enter_shell_setup_mode(
        &mut self,
        window: std::sync::Arc<winit::window::Window>,
        mut gpu: crate::gpu::GpuState,
        initial_path: String,
    ) {
        tracing::warn!(
            shell = %initial_path,
            "configured shell is not usable and no substitute applies; entering shell setup mode"
        );
        self.state.shell_setup_mode = true;
        self.state.shell_setup_path = initial_path;
        if let Err(e) = gpu.render_shell_setup(&window, &mut self.state.shell_setup_path) {
            tracing::warn!("shell setup first frame render failed: {e} — showing window anyway");
        }
        #[cfg(debug_assertions)]
        crate::boot::debug_background::show_boot_window(&window);
        #[cfg(not(debug_assertions))]
        window.set_visible(true);
        self.shell_setup_gpu = Some(gpu);
        self.shell_setup_window = Some(window);
    }

    /// 창 복원 때 트레이를 다시 만들지 않는다. 생성 불가는 일반 최소화로 처리한다.
    #[cfg(all(
        any(windows, target_os = "macos", target_os = "linux"),
        feature = "gui"
    ))]
    fn ensure_tray_icon_once(&mut self) {
        if self.tray_icon.is_none()
            && let Some((tray, ids)) = crate::system_tray::create_tray_icon()
        {
            self.tray_icon = Some(tray);
            self.tray_menu_ids = Some(ids);
        }
    }

    /// Sampling and persistence belong to App. A View only selects the optional notification target.
    fn record_plugin_rss_samples_if_present(&mut self) {
        let samples = self
            .plugin_manager
            .as_mut()
            .map(|manager| manager.take_rss_samples())
            .unwrap_or_default();
        let time = self.services.now_unix_millis() as u64;
        let anomalies: Vec<_> = samples
            .into_iter()
            .filter_map(|(plugin, bytes)| self.services.record_rss_sample(&plugin, bytes, time))
            .collect();
        if !anomalies.is_empty()
            && let Some((_, main, mut engine)) = engines_mut!(self).window_pairs().next()
        {
            crate::adapters::ipc::handler::display_plugin_rss_anomalies(
                &mut main.state,
                &mut engine,
                &anomalies,
            );
        }
    }

    /// 창 없는 engine은 redraw가 돌지 않아 공용 mesh 구동·전송 함수를 직접 호출한다.
    /// 복원되지 않은 다른 engine도 계속 처리하도록 parked 항목을 모두 순회한다.
    fn forward_mesh_frames_for_parked(&mut self) {
        if let Some(ref mgr) = self.plugin_manager {
            for mut engine in engines_mut!(self).parked() {
                crate::plugin_bridge::mesh_forward::forward_mesh_frames_for_engine(
                    &mut engine,
                    mgr,
                    &self.stream_hub,
                );
            }
        }
    }

    /// popup의 소유 창을 모르므로 모든 MainView에 repaint 예약을 전달한다.
    fn mark_invalidated_popups_dirty(&mut self) {
        let invalidated_popups = self
            .plugin_manager
            .as_mut()
            .map(|mgr| mgr.take_invalidated_popups())
            .unwrap_or_default();
        if invalidated_popups.is_empty() {
            return;
        }
        for main in self.main_windows_iter_mut() {
            for &iid in &invalidated_popups {
                main.state.plugin_mesh_popup_pending_repaint.insert(iid);
            }
            main.mark_dirty();
        }
    }

    /// banner의 소유 창을 모르므로 모든 MainView에 repaint 예약을 전달한다.
    fn mark_invalidated_banners_dirty(&mut self) {
        let invalidated_banners = self
            .plugin_manager
            .as_mut()
            .map(|mgr| mgr.take_invalidated_banners())
            .unwrap_or_default();
        if invalidated_banners.is_empty() {
            return;
        }
        for main in self.main_windows_iter_mut() {
            for &iid in &invalidated_banners {
                main.state.plugin_mesh_banner_pending_repaint.insert(iid);
            }
            main.mark_dirty();
        }
    }

    /// Linux GTK에는 별도 메인 루프가 없어 winit에서 메뉴 이벤트를 처리한다.
    fn poll_tray_menu_events(&mut self) {
        // 트레이가 없어도 열린 GTK 메뉴는 redraw 사이에 계속 이벤트를 처리해야 한다.
        #[cfg(target_os = "linux")]
        if self.tray_icon.is_some() || self.any_pending_native_menu() {
            crate::system_tray::pump_gtk_events();
        }

        #[cfg(all(
            any(windows, target_os = "macos", target_os = "linux"),
            feature = "gui"
        ))]
        if let Some(ref ids) = self.tray_menu_ids
            && let Some(menu_id) = crate::system_tray::poll_menu_event()
        {
            if menu_id == ids.show_window {
                // 기존 MainView가 있으면 다시 표시하고, 모두 parked 상태일 때만 새 창을 만든다.
                #[cfg(target_os = "macos")]
                {
                    let target = self
                        .view
                        .focused_view_id
                        .filter(|id| {
                            self.view
                                .views
                                .get(id)
                                .is_some_and(|w| w.as_main().is_some())
                        })
                        .or_else(|| {
                            self.view
                                .views
                                .iter()
                                .find(|(_, w)| w.as_main().is_some())
                                .map(|(id, _)| *id)
                        });
                    if let Some(id) = target {
                        if let Some(w) = self.view.views.get(&id) {
                            w.base().winit.set_minimized(false);
                            w.base().winit.focus_window();
                        }
                        self.view.focused_view_id = Some(id);
                        tracing::info!("tray show: focusing existing main window");
                    } else {
                        tracing::info!("tray show: no live window, creating");
                        crate::shortcuts::send_app_event(
                            &self.view.proxy,
                            AppEvent::CreateWindow(
                                crate::app::event::WindowRequestOrigin::User,
                                None,
                            ),
                        );
                    }
                }
                #[cfg(any(windows, target_os = "linux"))]
                crate::shortcuts::send_app_event(&self.view.proxy, AppEvent::TrayShowWindow);
            } else if menu_id == ids.new_window {
                crate::shortcuts::send_app_event(
                    &self.view.proxy,
                    AppEvent::CreateWindow(crate::app::event::WindowRequestOrigin::User, None),
                );
            } else if menu_id == ids.quit {
                crate::shortcuts::send_app_event(&self.view.proxy, AppEvent::Shutdown);
            }
        }
    }

    /// 미뤄 둔 PTY resize를 처리한다. 남은 요청이 있으면 다음 redraw를 요청한다.
    fn flush_pending_pty_resizes(&mut self) {
        let mut any_pending = false;
        for mut engine in self.engines_mut().windowed_and_parked() {
            if crate::app::services::AppServices::flush_pty_resizes(&mut engine) {
                any_pending = true;
            }
        }
        if any_pending {
            for w in self.view.views.values() {
                w.base().winit.request_redraw();
            }
        }
    }

    /// 출력을 비우기 전에 해당 surface 또는 전역 wake 중복 방지를 해제한다.
    fn note_drained_all(&self, surface_id: Option<u32>) {
        for engine in self.engines().windowed_and_parked() {
            if let Some(factory) = engine.runtime.waker_factory.as_ref() {
                factory.note_drained(surface_id);
            }
        }
    }

    /// surface ID가 있으면 해당 engine을, 없으면 모든 창·parked engine의 출력을 처리한다.
    fn handle_terminal_output(&mut self, surface_id: Option<u32>) {
        use crate::app::command::CoreEvent;
        use crate::app::dispatch_domain::DispatchSource;
        // 비우는 동안 도착한 wake가 중복으로 버려지지 않도록 먼저 해제한다.
        self.note_drained_all(surface_id);
        let core = &mut self.services;
        let mut pending: Vec<(DispatchSource, Vec<CoreEvent>)> = Vec::new();
        if let Some(sid) = surface_id {
            let mut found = false;
            for (id, main, mut engine) in engines_mut!(self).window_entries() {
                if engine.find_terminal_by_id(sid).is_some() {
                    let outcome = core.process_pty_output(&mut engine, sid);
                    if !outcome.events.is_empty() {
                        pending.push((DispatchSource::Engine(id), outcome.events));
                    }
                    // 출력 처리는 끝냈다. 보이지 않는 surface는 전환할 때 새로 그리므로 지금 redraw하지 않는다.
                    if main.is_surface_visible(&engine.read(), sid) {
                        main.mark_dirty_from(RepaintSource::TerminalOutput);
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                for (id, _, mut engine) in engines_mut!(self).parked_sessions_with_ids() {
                    if engine.find_terminal_by_id(sid).is_some() {
                        let outcome = core.process_pty_output(&mut engine, sid);
                        if !outcome.events.is_empty() {
                            pending.push((DispatchSource::Engine(id), outcome.events));
                        }
                        break;
                    }
                }
            }
        } else {
            for (id, _, mut engine) in engines_mut!(self).window_entries() {
                let outcome = core.process_all_pty_output(&mut engine);
                if !outcome.events.is_empty() {
                    pending.push((DispatchSource::Engine(id), outcome.events));
                }
            }
            for w in self.view.views.values_mut() {
                w.mark_dirty_from(RepaintSource::TerminalOutput);
            }
            for (id, _, mut engine) in engines_mut!(self).parked_sessions_with_ids() {
                let outcome = core.process_all_pty_output(&mut engine);
                if !outcome.events.is_empty() {
                    pending.push((DispatchSource::Engine(id), outcome.events));
                }
            }
        }
        for (source, events) in pending {
            for ev in events {
                self.handle_core_event_system(source, ev);
            }
        }
    }

    /// macOS는 MainView 상태를 park하고 창을 없앤다. Windows·Linux는 창을 숨기거나 최소화한다.
    fn handle_minimize(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let drained: Vec<_> = self.view.views.drain().collect();
            for (wid, w) in drained {
                if let Some(main_box) = crate::view::unbox_main(w) {
                    self.park_main_window(wid, main_box);
                }
            }
            self.view.focused_view_id = None;
            // 모달 View는 위에서 함께 버렸으므로 park하지 않고 활성 모달도 비운다.
            self.view.take_active_modal();
            tracing::info!(
                "minimized to background ({} states parked)",
                self.engines().parked_count()
            );
        }
        #[cfg(windows)]
        {
            if self.tray_icon.is_some() {
                for w in self.view.views.values() {
                    w.base().winit.set_visible(false);
                }
                tracing::info!("hid {} window(s) to system tray", self.view.views.len());
            } else {
                for w in self.view.views.values() {
                    w.base().winit.set_minimized(true);
                }
                tracing::info!("minimized {} window(s) to taskbar", self.view.views.len());
            }
        }
        #[cfg(target_os = "linux")]
        {
            if self.tray_icon.is_some() {
                for w in self.view.views.values() {
                    w.base().winit.set_visible(false);
                }
                tracing::info!("hid {} window(s) to system tray", self.view.views.len());
            } else {
                for w in self.view.views.values() {
                    w.base().winit.set_minimized(true);
                }
                tracing::info!("minimized {} window(s) to taskbar", self.view.views.len());
            }
        }
    }

    /// 확인한 셸 경로를 설정에 반영하고 이미 보이는 setup 창으로 부팅을 계속한다.
    fn finish_shell_setup_confirmed(&mut self) {
        let mut settings = crate::settings::Settings::load();
        settings.normalize();
        settings.general.shell = self.state.shell_setup_path.clone();
        self.state.shell_setup_mode = false;
        let window = self.shell_setup_window.take().unwrap();
        let gpu = self.shell_setup_gpu.take().unwrap();
        self.begin_boot(window, gpu, settings, std::time::Instant::now(), false);
    }

    fn handle_shell_setup_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        use crate::app::offview_window_events::{offview_event_effect, render_error_reconfigures};
        if let WindowEvent::RedrawRequested = &event {
            if let (Some(gpu), Some(window)) = (&mut self.shell_setup_gpu, &self.shell_setup_window)
            {
                let result = gpu.render_shell_setup(window, &mut self.state.shell_setup_path);
                match result {
                    Ok(crate::gpu::ShellSetupAction::None) => {}
                    Ok(crate::gpu::ShellSetupAction::Confirmed) => {
                        self.finish_shell_setup_confirmed();
                    }
                    Ok(crate::gpu::ShellSetupAction::Exit) => {
                        event_loop.exit();
                    }
                    Err(e) if render_error_reconfigures(&e) => {
                        gpu.resize(window.inner_size());
                        window.request_redraw();
                    }
                    Err(e) => {
                        let msg = format!("shell setup render error: {e}");
                        tracing::warn!("{}", msg);
                        crate::crash_report::record_error(&msg);
                    }
                }
            }
            // RedrawRequested에서 다시 요청하면 유휴 상태에도 렌더가 계속 반복된다.
            if let (Some(gpu), Some(window)) = (&mut self.shell_setup_gpu, &self.shell_setup_window)
            {
                gpu.handle_egui_event(window, &event);
            }
            return;
        }
        if let (Some(gpu), Some(window)) = (&mut self.shell_setup_gpu, &self.shell_setup_window) {
            let (_, egui_repaint) = gpu.handle_egui_event(window, &event);
            let effect = offview_event_effect(&event, egui_repaint);
            if effect.reconfigure_surface {
                gpu.sync_scale_factor(window);
                gpu.resize(window.inner_size());
            }
            if effect.redraw {
                window.request_redraw();
            }
            if effect.exit {
                event_loop.exit();
            }
        }
    }

    /// 준비된 창과 GPU에 엔진 오류를 표시해 터미널 없이 실행한 사용자도 원인을 볼 수 있게 한다.
    pub(crate) fn enter_boot_error_mode(
        &mut self,
        window: std::sync::Arc<winit::window::Window>,
        mut gpu: crate::gpu::GpuState,
    ) {
        tracing::warn!("boot failed with a live GPU — showing the boot error screen");
        self.state.boot_error_mode = true;
        // 렌더 실패 시에도 창은 표시한다.
        if let Some(info) = &self.state.boot_error_info
            && let Err(e) = gpu.render_boot_error(&window, info)
        {
            tracing::warn!("boot error first frame render failed: {e} — showing window anyway");
        }
        #[cfg(debug_assertions)]
        crate::boot::debug_background::show_boot_window(&window);
        #[cfg(not(debug_assertions))]
        window.set_visible(true);
        self.boot_error_gpu = Some(gpu);
        self.boot_error_window = Some(window);
    }

    /// 부팅 오류 화면에서 종료를 요청하면 실패 코드 1로 끝낸다.
    fn handle_boot_error_window_event(&mut self, event: WindowEvent) {
        use crate::app::offview_window_events::{offview_event_effect, render_error_reconfigures};
        if let WindowEvent::RedrawRequested = &event {
            let quit = if let (Some(gpu), Some(window), Some(info)) = (
                &mut self.boot_error_gpu,
                &self.boot_error_window,
                &self.state.boot_error_info,
            ) {
                match gpu.render_boot_error(window, info) {
                    Ok(quit) => quit,
                    Err(e) if render_error_reconfigures(&e) => {
                        gpu.resize(window.inner_size());
                        window.request_redraw();
                        false
                    }
                    Err(e) => {
                        let msg = format!("boot error render error: {e}");
                        tracing::warn!("{}", msg);
                        crate::crash_report::record_error(&msg);
                        false
                    }
                }
            } else {
                false
            };
            if quit {
                exit_from_boot_error();
            }
            // RedrawRequested에서 다시 요청하면 유휴 상태에도 렌더가 계속 반복된다.
            if let (Some(gpu), Some(window)) = (&mut self.boot_error_gpu, &self.boot_error_window) {
                gpu.handle_egui_event(window, &event);
            }
            return;
        }
        if let (Some(gpu), Some(window)) = (&mut self.boot_error_gpu, &self.boot_error_window) {
            let (_, egui_repaint) = gpu.handle_egui_event(window, &event);
            let effect = offview_event_effect(&event, egui_repaint);
            if effect.reconfigure_surface {
                gpu.sync_scale_factor(window);
                gpu.resize(window.inner_size());
            }
            if effect.redraw {
                window.request_redraw();
            }
            if effect.exit {
                exit_from_boot_error();
            }
        }
    }

    /// 모달에 이벤트를 전달하고 반환한 닫기 요청을 처리한다. 호출자는 이후 일반 창 처리를 하지 않는다.
    fn handle_active_modal_window_event(&mut self, id: WindowId, event: WindowEvent) {
        self.sync_auxiliary_toast_lifetime(id);
        let action = if let Some(modal) = self.view.views.get_mut(&id) {
            let mut ctx = ViewCtx {
                modal_active: false,
                engine: None,
            };
            modal.handle_event(event, &mut ctx)
        } else {
            ViewAction::None
        };

        self.process_settings_file_requests(id);
        match action {
            ViewAction::None => {}
            ViewAction::Close => {
                self.close_active_modal();
            }
            ViewAction::CloseWithEvent(app_event) => {
                self.close_active_modal();
                crate::shortcuts::send_app_event(&self.view.proxy, app_event);
            }
        }
    }

    fn handle_window_focused(&mut self, id: WindowId) {
        // focused_view_id는 모달이 아닌 MainView를 가리킨다.
        let is_main = self
            .view
            .views
            .get(&id)
            .map(|w| w.as_main().is_some())
            .unwrap_or(false);
        if is_main {
            self.view.focused_view_id = Some(id);
        }
        if let Some(mgr) = self.plugin_manager.as_mut() {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::WindowFocused;
            let payload = WindowFocused {
                window_id: u64::from(id),
            };
            mgr.emit_host_event("window.focused", &payload, EventScope::System);
        }
        if let Some(modal_id) = self.view.active_modal_id()
            && let Some(modal) = self.view.views.get(&modal_id)
        {
            modal.base().winit.focus_window();
        }
    }

    fn dispatch_window_event_to_view(&mut self, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested)
            && self
                .view
                .views
                .get(&id)
                .is_some_and(|view| view.as_main().is_some())
        {
            self.redraw_main_window(id);
            return;
        }
        self.refresh_approval_presentations();
        self.refresh_preset_editor(id);
        self.sync_auxiliary_toast_lifetime(id);
        let modal_active = self.view.is_modal_active();
        let action = {
            if let Some(w) = self.view.views.get_mut(&id) {
                let engine = self.engines.window_read(id);
                let mut ctx = ViewCtx {
                    modal_active,
                    engine,
                };
                // modeless PresetView의 닫기는 여기서 처리하며 모달은 앞의 전용 경로가 처리한다.
                w.handle_event(event, &mut ctx)
            } else {
                ViewAction::None
            }
        };
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let request = self
                .view
                .views
                .get_mut(&id)
                .and_then(|view| view.as_main_mut())
                .and_then(|main| main.take_outbound_request());
            // OLE pumps nested Win32 messages; AppKit can synchronously invoke source callbacks.
            // The View and engine read guards above have both been released.
            if let Some(request) = request {
                request.start(self.view.proxy.clone());
            }
        }
        self.process_preset_edits(id);
        if self.view.views.contains_key(&id) {
            match action {
                ViewAction::None => {}
                ViewAction::Close => {
                    if self.state.preset_view_id == Some(id) {
                        self.on_preset_window_closed(id);
                        return;
                    }
                    debug_assert!(false, "non-modal window returned Close unexpectedly");
                }
                ViewAction::CloseWithEvent(app_event) => {
                    if self.state.preset_view_id == Some(id) {
                        self.on_preset_window_closed(id);
                        crate::shortcuts::send_app_event(&self.view.proxy, app_event);
                        return;
                    }
                    debug_assert!(
                        false,
                        "non-modal window returned CloseWithEvent unexpectedly"
                    );
                }
            }

            let close_requested = self
                .view
                .views
                .get(&id)
                .map(|w| w.base().state.close_requested)
                .unwrap_or(false);
            if close_requested {
                self.close_self_requesting_window(id);
            }
        }
    }

    /// 창 이벤트 밖에서도 마지막 workspace를 닫을 수 있어 그 자리에서 닫기 요청을 처리한다.
    /// 빈 workspace 목록을 가진 창이 다음 redraw로 넘어가지 않게 한다.
    /// close_request_consumed_in_place 검사가 호출 위치를 확인한다.
    pub(crate) fn close_self_requesting_windows(&mut self) {
        let ids: Vec<WindowId> = self
            .view
            .views
            .iter()
            .filter(|(_, w)| w.base().state.close_requested)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close_self_requesting_window(id);
        }
    }

    /// 내부 close_requested를 처리한다. 버튼 닫기와 달리 plugin·Lua 창 닫기 통지는 보내지 않는다.
    fn close_self_requesting_window(&mut self, id: WindowId) {
        self.pending_focus_hint_clear.remove(&id);
        // 조건식 안에서 drop되지 않게 먼저 꺼내 은퇴 처리를 수행한다.
        if let Some(w) = self.view.views.remove(&id) {
            let was_last_main = self.view.views.values().all(|w| w.as_main().is_none());
            match crate::view::unbox_main(w) {
                // 마지막 MainView는 engine을 park하므로 레이아웃 슬롯 점유도 유지한다.
                Some(main_box) if was_last_main => {
                    tracing::info!("last main window closed via request, parking state");
                    self.park_main_window(id, main_box);
                }
                Some(main_box) => self.retire_main_window(id, main_box),
                None => {}
            }
        }
        if self.view.focused_view_id == Some(id) {
            self.view.focused_view_id = self
                .view
                .views
                .iter()
                .find(|(_, w)| w.as_main().is_some())
                .map(|(id, _)| *id);
        }
    }

    /// Windows 절전 복귀 뒤 PTY 상태·출력을 확인하고 의심되는 surface를 알린다.
    #[cfg(all(windows, feature = "gui"))]
    pub(crate) fn resume_health_pass(&mut self) {
        use crate::app::command::CoreEvent;
        use crate::app::dispatch_domain::DispatchSource;
        tracing::info!(
            "system resumed — running PTY health pass (docs/surfaces/terminal/index.md)"
        );
        let core = &mut self.services;
        let mut pending: Vec<(DispatchSource, Vec<CoreEvent>)> = Vec::new();
        for (id, main, mut engine) in engines_mut!(self).window_entries() {
            let suspects = engine.wake_terminals_after_resume();
            let outcome = core.process_all_pty_output(&mut engine);
            if !outcome.events.is_empty() {
                pending.push((DispatchSource::Engine(id), outcome.events));
            }
            Self::notify_resume_suspects(&mut engine, &suspects);
            main.mark_dirty();
        }
        for (id, _, mut engine) in engines_mut!(self).parked_sessions_with_ids() {
            let suspects = engine.wake_terminals_after_resume();
            let outcome = core.process_all_pty_output(&mut engine);
            if !outcome.events.is_empty() {
                pending.push((DispatchSource::Engine(id), outcome.events));
            }
            Self::notify_resume_suspects(&mut engine, &suspects);
        }
        for (source, events) in pending {
            for ev in events {
                self.handle_core_event_system(source, ev);
            }
        }
    }

    #[cfg(all(windows, feature = "gui"))]
    fn notify_resume_suspects(
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        suspects: &[u32],
    ) {
        for &sid in suspects {
            let ws_id = engine
                .workspaces()
                .into_iter()
                .find(|w| w.all_surface_ids().contains(&sid))
                .map(|w| w.id)
                .unwrap_or(0);
            let title = crate::i18n::t("resume.suspect.title").to_string();
            let body = crate::i18n::t("resume.suspect.body").to_string();
            if engine
                .live
                .notifications
                .add_with_level(
                    ws_id,
                    sid,
                    title,
                    body,
                    crate::notification::NotificationLevel::Info,
                )
                .is_some()
            {
                engine.raise_attention(sid, crate::core::AttentionKind::Completion);
            }
        }
    }

    /// PresetView는 바로 닫고 MainView는 남은 개수에 따라 해당 창 닫기 또는 종료 흐름으로 보낸다.
    pub(crate) fn request_close_window(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        if self.state.preset_view_id == Some(id) {
            self.on_preset_window_closed(id);
            return;
        }
        if self.main_window_count() > 1 {
            self.close_main_window(id, tasty_plugin_protocol::LifecycleReason::User);
        } else {
            self.handle_quit_requested(event_loop);
        }
    }

    /// 창을 닫고 plugin·Lua에 알린다. 포커스 창이면 포커스를 넘긴다.
    /// 마지막 MainView의 종료 여부는 호출자가 결정한다.
    pub(crate) fn close_main_window(
        &mut self,
        id: WindowId,
        reason: tasty_plugin_protocol::LifecycleReason,
    ) {
        // engine이 drop되기 전에 슬롯을 마무리하고 닫기 통지를 보낸다.
        self.pending_focus_hint_clear.remove(&id);
        let removed = self.view.views.remove(&id);
        if let Some(main_box) = removed.and_then(crate::view::unbox_main) {
            self.retire_main_window(id, main_box);
        }
        let scripts = self.autofire_scripts();
        if let Some(mgr) = self.plugin_manager.as_mut() {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::WindowClosed;
            let payload = WindowClosed {
                window_id: u64::from(id),
                reason,
            };
            mgr.emit_host_event("window.closed", &payload, EventScope::System);
            crate::hooks::lua::fire(
                self.lua_engine.as_ref(),
                crate::hooks::lua::AutofireCtx {
                    scripts: &scripts,
                    guard: &mut self.lua_autofire,
                    runs: &mut self.lua_runs,
                },
                "window.delete.post",
                &payload,
            );
        }
        if self.view.focused_view_id == Some(id) {
            self.view.focused_view_id = self
                .view
                .views
                .iter()
                .find(|(_, w)| w.as_main().is_some())
                .map(|(id, _)| *id);
        }
    }

    /// 배치 시작에 모든 engine에 끊긴 client를 표시한다. 실제 점유 해제는 배치 끝에 수행한다.
    /// 표시·해제를 나누는 이유는 OccupancyRegistry::mark_clients_disconnected를 따른다.
    pub(crate) fn mark_disconnected_clients(&mut self, clients: &[u32]) {
        if clients.is_empty() {
            return;
        }
        for engine in self.engines_mut().windowed_and_parked() {
            engine.live.occupancy.mark_clients_disconnected(clients);
        }
    }

    pub(crate) fn release_attach_for_disconnected(&mut self, clients: &[u32]) {
        for engine in self.engines_mut().windowed_and_parked() {
            for &cid in clients {
                engine.live.occupancy.release_all_for_client(cid);
                engine.remote.forget_client_structures(cid);
                engine.remote.bulk_transfers.clear_client(cid);
                engine.remote.capture_uploads.clear_client(cid);
                engine.remote.mesh_mirror.remove_for_client(cid);
            }
        }
    }

    pub(crate) fn apply_stream_outcome(&mut self, mut outcome: PumpOutcome) {
        let hub = self.stream_hub.clone();

        // 같은 배치의 재attach가 곧 해제될 holder 때문에 거절되지 않게 먼저 끊김을 표시한다.
        self.mark_disconnected_clients(&outcome.disconnected);
        self.apply_attach_requests_batch(&mut outcome, &hub);
        self.apply_input_events_batch(outcome.input_events.drain(..), &hub);
        self.apply_forwarded_batch(&mut outcome, &hub);
        self.apply_mesh_batch(&mut outcome, &hub);
        self.apply_content_batch(&mut outcome, &hub);

        if !outcome.disconnected.is_empty() {
            self.release_attach_for_disconnected(&outcome.disconnected);
        }

        // 점유 변경 직후 readonly 화면을 갱신해 다음 주기 확인까지 빈 화면으로 남지 않게 한다.
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            main.state.reconcile_presentation(engine.core);
            engine.refresh_attach_presentation(&main.state.navigation);
            engine.push_structure_changes();
            if engine.refresh_readonly_views() {
                main.mark_dirty();
            }
        }
        for (state, mut engine) in self.engines_mut().parked_sessions() {
            state.reconcile_presentation(engine.core);
            engine.refresh_attach_presentation(&state.navigation);
            engine.push_structure_changes();
        }
    }

    fn apply_attach_requests_batch(&mut self, outcome: &mut PumpOutcome, hub: &StreamHub) {
        use crate::app::attach_activation::Target;
        for (client_id, surface_id) in outcome.attach_requests.drain(..) {
            if !self.attach_on_owning_engine(Target::Surface(surface_id), client_id, hub) {
                crate::remote::server::reject_attach(hub, client_id, "not_found", None);
            }
        }
        for (client_id, workspace_id) in outcome.workspace_attach_requests.drain(..) {
            if !self.attach_on_owning_engine(Target::Workspace(workspace_id), client_id, hub) {
                crate::remote::server::reject_attach(hub, client_id, "workspace_not_found", None);
            }
        }
    }

    fn apply_forwarded_batch(&mut self, outcome: &mut PumpOutcome, hub: &StreamHub) {
        for (client_id, op_id, op, origin) in outcome.structural_ops.drain(..) {
            self.apply_forwarded_structural_op(client_id, op_id, &op, origin, hub);
        }
        // 적용·같은 크기는 tap의 Resize가, 어느 engine도 받지 않은 요청은 거절 회신이 응답한다.
        for (client_id, sid, cols, rows) in outcome.resize_requests.drain(..) {
            crate::remote::server::answer_resize_on_engines(
                hub,
                client_id,
                sid,
                cols,
                rows,
                |apply| self.apply_on_first_engine(apply),
            );
        }
        // attention 해제는 저장 대상이 아니므로 레이아웃 저장은 예약하지 않는다.
        for (client_id, remote_surface_id) in outcome.attention_clear_requests.drain(..) {
            self.apply_on_first_engine(|engine| {
                engine.apply_attached_attention_clear(client_id, remote_surface_id)
            });
        }
    }

    /// mesh 구독·표시 정보와 입력은 대상과 점유를 가진 engine에 반영하고, 없으면 MeshError를 회신한다.
    /// 헤드리스·GUI parked는 쌓인 입력을 공용 구동 경로가 소비한다.
    /// 창이 살아 있는 GUI 서버는 이 입력 큐를 플러그인에 전달하는 경로가 아직 없다.
    fn apply_mesh_batch(&mut self, outcome: &mut PumpOutcome, hub: &StreamHub) {
        for (client_id, surface_id) in outcome.mesh_full_resend_requests.drain(..) {
            if !self
                .any_engine(|engine| engine.apply_attached_mesh_full_resend(surface_id, client_id))
            {
                reply_mesh_error(hub, client_id, surface_id, "not_attached");
            }
        }
    }

    fn apply_content_batch(&mut self, outcome: &mut PumpOutcome, hub: &StreamHub) {
        self.apply_html_batch(outcome, hub);
        for (client_id, msg) in outcome.capture_uploads.drain(..) {
            self.apply_capture_upload_msg(client_id, msg, hub);
        }
        for (client_id, msg) in outcome.list_dir_requests.drain(..) {
            self.apply_list_dir_request_msg(client_id, msg, hub);
        }
        for (client_id, msg) in outcome.git_query_requests.drain(..) {
            self.apply_git_query_request_msg(client_id, msg, hub);
        }
        for (client_id, msg) in outcome.markdown_content_requests.drain(..) {
            self.apply_markdown_content_request_msg(client_id, msg, hub);
        }
        for (client_id, msg) in outcome.terminal_reads.drain(..) {
            use crate::remote::terminal_read as reads;
            if !self.any_engine(|engine| reads::handle(engine, hub, client_id, &msg)) {
                reads::reply_missing(hub, client_id, &msg);
            }
        }
        for (client_id, msg) in outcome.file_downloads.drain(..) {
            let holds = find_workspace_holder_engine_mut(self.engines_mut(), client_id).is_some();
            crate::remote::file_download::handle(hub, client_id, holds, &msg);
        }
        // begin보다 chunk가 먼저 처리되지 않도록 bulk 이벤트는 도착 순서를 유지한다.
        for (client_id, event) in outcome.bulk_events.drain(..) {
            match hub.bulk_workspace(client_id) {
                Some(ws) => self.apply_bulk_event(client_id, event, ws, hub),
                None => tracing::warn!(
                    "bulk transfer: event from non-bulk client {client_id} — ignoring"
                ),
            }
        }
    }

    /// 창 → parked 순서로 처음 받아들인 engine에서 멈춘다. 창 engine이면 다시 그리도록 표시한다.
    pub(crate) fn apply_on_first_engine(
        &mut self,
        mut apply: impl FnMut(&mut EngineMut<'_>) -> bool,
    ) {
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            if apply(&mut engine) {
                main.mark_dirty();
                return;
            }
        }
        for mut engine in self.engines_mut().parked() {
            if apply(&mut engine) {
                return;
            }
        }
    }

    /// 창·parked engine 중 하나가 받아들이면 true다. 첫 engine에서 멈춘다.
    pub(super) fn any_engine(&mut self, mut apply: impl FnMut(&mut EngineMut<'_>) -> bool) -> bool {
        self.engines_mut()
            .windowed_and_parked()
            .any(|mut engine| apply(&mut engine))
    }
    pub(super) fn apply_input_frames_batch(
        &mut self,
        frames: impl IntoIterator<Item = (u32, Vec<u8>)>,
    ) {
        for (client_id, bytes) in frames {
            if self
                .pending_server_attaches
                .iter()
                .any(|pending| pending.client() == client_id)
            {
                crate::remote::server::reject_attach(
                    &self.stream_hub,
                    client_id,
                    "not_ready",
                    None,
                );
                for pending in &mut self.pending_server_attaches {
                    if pending.client() == client_id {
                        pending.cancel();
                    }
                }
                self.journal.wake_application();
                continue;
            }
            let routed = self.feed_stream_input(client_id, &bytes);
            #[cfg(debug_assertions)]
            if !routed {
                let echo_frame = crate::ipc::stream::StreamFrame::new(
                    crate::ipc::stream::StreamTag::Data,
                    bytes,
                );
                let _ = self.stream_hub.push(client_id, echo_frame); // debug echo의 전송 실패는 재시도하지 않는다.
            }
            #[cfg(not(debug_assertions))]
            let _ = routed; // release: echo 분기 없어 routed 미사용 — 값 drop(Result 아님).
        }
    }

    fn attach_on_owning_engine(
        &mut self,
        target: crate::app::attach_activation::Target,
        client: u32,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) -> bool {
        use crate::app::attach_activation::Target;
        let Some(session) = self
            .engines
            .all_sessions_mut()
            .find(|session| match target {
                Target::Surface(surface) => session.core_state.has_surface(surface),
                Target::Workspace(workspace) => session.core_state.has_workspace(workspace),
            })
        else {
            return false;
        };
        let id = session.id;
        crate::app::attach_activation::begin(
            &mut self.pending_server_attaches,
            &self.journal,
            id,
            &mut session.borrow_mut(),
            target,
            client,
            hub,
        );
        true
    }

    /// workspace 점유는 surface ID가 붙은 입력, 단일 surface 점유는 원시 입력으로 전달한다.
    fn feed_stream_input(&mut self, client_id: u32, bytes: &[u8]) -> bool {
        for mut engine in self.engines_mut().windowed_and_parked() {
            if engine.live.occupancy.client_holds_workspace(client_id) {
                return Self::demux_workspace_input(&mut engine, client_id, bytes);
            }
        }
        self.any_engine(|engine| engine.feed_attached_input(client_id, bytes))
    }

    fn demux_workspace_input(engine: &mut EngineMut<'_>, client_id: u32, bytes: &[u8]) -> bool {
        match crate::ipc::stream::decode_mux(bytes) {
            Some((sid, payload)) => engine.feed_attached_workspace_input(client_id, sid, payload),
            None => false,
        }
    }

    /// 구조 변경은 점유한 workspace를 가진 MainView에서만 실행한다.
    /// parked 상태에도 MainViewState는 있지만 현재 이 실행 루프의 대상은 아니다.
    fn apply_forwarded_structural_op(
        &mut self,
        client_id: u32,
        op_id: u64,
        op: &crate::ipc::stream::StructuralOp,
        origin: crate::ipc::stream::ForwardOrigin,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        let owner = self
            .engines
            .all_sessions()
            .find(|session| {
                session
                    .live
                    .occupancy
                    .workspace_held_by(client_id)
                    .is_some()
            })
            .map(|session| session.id);
        if let Some(session) = owner.and_then(|id| self.engines.session_mut(id))
            && let Some(binding) = session.journal_binding.as_ref()
        {
            if let Some((ticket, workspace)) = self.journal.admit_remote(
                session.id,
                &session.live,
                binding.runtime_epoch,
                hub,
                crate::app::journal::commands::inbound::InboundOp {
                    client: client_id,
                    op_id,
                    op: op.clone(),
                    origin,
                },
            ) {
                session
                    .remote
                    .pending_structure_replies
                    .insert(ticket, workspace);
            }
        } else {
            let reason = crate::remote::structure_sync::unresolved_forward_reason(
                self.engines.all_sessions().map(|session| session.as_ref()),
                client_id,
                op,
            );
            crate::app::journal::commands::inbound::reply(
                hub,
                client_id,
                op_id,
                false,
                Some(reason),
            );
        }
    }

    /// workspace 점유자를 찾아 캡처 청크를 누적하거나 파일 저장·클립보드 갱신을 완료한다.
    fn apply_capture_upload_msg(
        &mut self,
        client_id: u32,
        msg: tasty_ipc::stream_hub::CaptureUploadMsg,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        use tasty_ipc::stream_hub::CaptureUploadMsg;
        match msg {
            CaptureUploadMsg::CaptureChunk {
                upload_id,
                data_b64,
                ..
            } => {
                use base64::Engine as _;
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&data_b64) else {
                    tracing::warn!(
                        "capture upload: invalid base64 chunk (client {client_id}, upload {upload_id})"
                    );
                    return;
                };
                match find_workspace_holder_engine_mut(engines_mut!(self), client_id) {
                    Some(mut engine) => crate::remote::server::append_capture_upload(
                        &mut engine,
                        hub,
                        client_id,
                        upload_id,
                        &bytes,
                        std::time::Instant::now(),
                    ),
                    None => tracing::warn!(
                        "capture upload: client {client_id} does not hold a workspace — dropping chunk"
                    ),
                }
            }
            CaptureUploadMsg::CaptureCommit {
                upload_id,
                file_name,
            } => {
                let admitted = find_workspace_holder_engine_mut(engines_mut!(self), client_id).map(
                    |mut engine| {
                        crate::remote::server::admit_capture_upload(
                            &mut engine,
                            hub,
                            client_id,
                            upload_id,
                        )
                    },
                );
                match admitted {
                    Some(admitted) => {
                        self.store_capture_off_thread(
                            hub, client_id, upload_id, file_name, admitted,
                        );
                    }
                    None => reply_without_workspace_attach(
                        hub,
                        client_id,
                        serde_json::json!({ "event": "capture_result", "upload_id": upload_id }),
                    ),
                }
            }
        }
    }

    fn apply_list_dir_request_msg(
        &mut self,
        client_id: u32,
        msg: tasty_ipc::stream_hub::ListDirRequestMsg,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        use tasty_ipc::stream_hub::ListDirRequestMsg;
        let ListDirRequestMsg::ListDirRequest { request_id, dir } = msg;
        if let Some(mut engine) = find_workspace_holder_engine_mut(self.engines_mut(), client_id) {
            crate::remote::server::handle_list_dir_request(
                &mut engine,
                hub,
                client_id,
                request_id,
                &dir,
            );
            return;
        }
        reply_without_workspace_attach(
            hub,
            client_id,
            serde_json::json!({ "event": "list_dir_result", "request_id": request_id }),
        );
    }

    fn apply_markdown_content_request_msg(
        &mut self,
        client_id: u32,
        msg: tasty_ipc::stream_hub::MarkdownContentRequestMsg,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        use tasty_ipc::stream_hub::MarkdownContentRequestMsg;
        let MarkdownContentRequestMsg::MarkdownContentRequest {
            request_id,
            surface_id,
        } = msg;
        if let Some(mut engine) = find_workspace_holder_engine_mut(self.engines_mut(), client_id) {
            crate::remote::server::handle_markdown_content_request(
                &mut engine,
                hub,
                client_id,
                request_id,
                surface_id,
            );
            return;
        }
        reply_without_workspace_attach(
            hub,
            client_id,
            serde_json::json!({
                "event": "markdown_content_result",
                "request_id": request_id,
                "surface_id": surface_id,
            }),
        );
    }

    fn apply_git_query_request_msg(
        &mut self,
        client_id: u32,
        msg: tasty_ipc::stream_hub::GitQueryRequestMsg,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        use tasty_ipc::stream_hub::GitQueryRequestMsg;
        let GitQueryRequestMsg::GitQueryRequest {
            request_id,
            surface_id,
            kind,
            worktree_path,
            diff_path,
        } = msg;
        if let Some(mut engine) = find_workspace_holder_engine_mut(self.engines_mut(), client_id) {
            crate::remote::server::handle_git_query_request(
                &mut engine,
                hub,
                client_id,
                request_id,
                surface_id,
                kind,
                worktree_path,
                diff_path,
            );
            return;
        }
        reply_without_workspace_attach(
            hub,
            client_id,
            serde_json::json!({
                "event": "git_query_result",
                "request_id": request_id,
                "kind": kind.as_wire_str(),
            }),
        );
    }

    /// bulk 연결은 직접 점유자가 아니므로 결속된 workspace의 소유 engine으로 전달한다.
    /// begin·chunk·commit은 도착 순서대로 처리한다. 인가와 저장은 commit 경로가 검증한다.
    fn apply_bulk_event(
        &mut self,
        client_id: u32,
        event: tasty_ipc::stream_hub::BulkEvent,
        bulk_ws: u32,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        use tasty_ipc::stream_hub::BulkEvent;
        match event {
            BulkEvent::Begin {
                transfer_id,
                filename,
                total_size,
            } => {
                if self
                    .with_bulk_ws_engine(bulk_ws, |engine| {
                        crate::remote::server::begin_bulk_transfer(
                            engine,
                            hub,
                            client_id,
                            transfer_id,
                            filename,
                            total_size,
                        );
                    })
                    .is_none()
                {
                    tracing::warn!(
                        "bulk transfer: no engine owns workspace {bulk_ws} — dropping begin"
                    );
                }
            }
            BulkEvent::Chunk {
                transfer_id,
                seq,
                bytes,
            } => {
                let found = self.with_bulk_ws_engine(bulk_ws, |engine| {
                    crate::remote::server::append_bulk_transfer(
                        engine,
                        hub,
                        client_id,
                        transfer_id,
                        seq,
                        &bytes,
                    )
                });
                log_bulk_chunk_result(found, client_id, transfer_id, bulk_ws);
            }
            BulkEvent::Commit { transfer_id } => {
                let found = self.with_bulk_ws_engine(bulk_ws, |engine| {
                    // begin의 용량 검사와 같은 소유 engine 설정에서 저장 폴더를 구한다.
                    let dir =
                        crate::remote::server::resolve_bulk_transfer_dir(&engine.runtime.settings);
                    crate::remote::server::finalize_bulk_transfer(
                        engine,
                        hub,
                        client_id,
                        transfer_id,
                        bulk_ws,
                        dir,
                    );
                });
                if found.is_none() {
                    send_bulk_commit_failure(hub, client_id, transfer_id);
                }
            }
        }
    }

    fn with_bulk_ws_engine<R>(
        &mut self,
        bulk_ws: u32,
        f: impl FnOnce(&mut EngineMut<'_>) -> R,
    ) -> Option<R> {
        self.engines_mut()
            .windowed_and_parked()
            .find(|engine| engine.find_workspace_index_for_id(bulk_ws).is_some())
            .map(|mut engine| f(&mut engine))
    }
}

/// workspace를 점유하지 않은 클라이언트의 요청에 실패 결과를 회신한다.
/// `fields`는 event와 요청 식별 필드를 담은 객체다. 회신 전송 실패는 여기서 재시도하지 않는다.
fn reply_without_workspace_attach(
    hub: &tasty_ipc::stream_hub::StreamHub,
    client_id: u32,
    mut fields: serde_json::Value,
) {
    fields["ok"] = false.into();
    fields["reason"] = "client does not hold a workspace attach".into();
    let frame = crate::ipc::stream::StreamFrame::new(
        crate::ipc::stream::StreamTag::Control,
        serde_json::to_vec(&fields).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 회신 전송 실패는 여기서 재시도하지 않는다.
}

/// 창 → parked 순서로 workspace 점유자를 찾는다.
/// 다른 App 필드와 나눠 빌려야 하는 호출부는 [`engines_mut!`]로 핸들을 넘긴다.
pub(super) fn find_workspace_holder_engine_mut(
    engines: crate::app::window_access::EngineScanMut<'_>,
    client_id: u32,
) -> Option<EngineMut<'_>> {
    engines
        .windowed_and_parked()
        .find(|engine| engine.live.occupancy.client_holds_workspace(client_id))
}

/// transfer가 등록되지 않은 경우와 workspace 소유 engine이 없는 경우를 구별해 기록한다.
fn log_bulk_chunk_result(found: Option<bool>, client_id: u32, transfer_id: u64, bulk_ws: u32) {
    match found {
        Some(true) => {}
        Some(false) => tracing::warn!(
            "bulk transfer: chunk for unknown transfer (client {client_id}, transfer {transfer_id}) — no begin? dropping"
        ),
        None => {
            tracing::warn!("bulk transfer: no engine owns workspace {bulk_ws} — dropping chunk")
        }
    }
}

/// 결속된 workspace가 사라졌으면 bulk 실패를 회신한다.
fn send_bulk_commit_failure(
    hub: &tasty_ipc::stream_hub::StreamHub,
    client_id: u32,
    transfer_id: u64,
) {
    let reply = crate::ipc::stream::StreamControl::BulkResult {
        transfer_id,
        ok: false,
        path: None,
        reason: Some("no engine owns the bound workspace".to_string()),
    };
    let frame = crate::ipc::stream::StreamFrame::new(
        crate::ipc::stream::StreamTag::Control,
        serde_json::to_vec(&reply).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 회신 전송 실패는 여기서 재시도하지 않는다.
}

pub(super) fn reply_mesh_error(
    hub: &tasty_ipc::stream_hub::StreamHub,
    client_id: u32,
    surface_id: u32,
    reason: &str,
) {
    let reply = crate::ipc::stream::StreamControl::MeshError {
        surface_id,
        reason: reason.to_string(),
    };
    let frame = crate::ipc::stream::StreamFrame::new(
        crate::ipc::stream::StreamTag::Control,
        serde_json::to_vec(&reply).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 회신 전송 실패는 여기서 재시도하지 않는다.
}

/// 기존 대기와 새 시각 중 이른 쪽을 유지한다. Poll은 이미 즉시 처리하므로 그대로 둔다.
fn merge_wakeup(
    current: winit::event_loop::ControlFlow,
    at: std::time::Instant,
) -> winit::event_loop::ControlFlow {
    use winit::event_loop::ControlFlow;
    match current {
        ControlFlow::Poll => ControlFlow::Poll,
        ControlFlow::WaitUntil(cur) if cur <= at => current,
        _ => ControlFlow::WaitUntil(at),
    }
}

impl App {
    /// 상한 때문에 미룬 repaint를 실행하고 가장 이른 미완료 요청의 시각을 예약한다.
    /// 다른 이벤트가 오지 않아도 요청한 화면을 다시 그릴 수 있게 한다.
    fn drive_deferred_repaints(&mut self, event_loop: &ActiveEventLoop) {
        let now = std::time::Instant::now();
        let mut earliest: Option<std::time::Instant> = None;
        for w in self.view.views.values_mut() {
            let base = w.base_mut();
            if base.state.repaint.take_due(now) {
                base.winit.request_redraw();
            } else if let Some(at) = base.state.repaint.deferred_deadline() {
                earliest = Some(earliest.map_or(at, |cur: std::time::Instant| cur.min(at)));
            }
        }
        if let Some(at) = earliest {
            event_loop.set_control_flow(merge_wakeup(event_loop.control_flow(), at));
        }
    }

    fn any_pending_native_menu(&self) -> bool {
        self.view.views.values().any(|w| {
            w.as_main().is_some_and(|m| {
                #[cfg(target_os = "linux")]
                let inbound = m.has_inbound_drop();
                #[cfg(not(target_os = "linux"))]
                let inbound = false;
                m.has_pending_native_menu() || m.has_outbound_drag() || inbound
            })
        })
    }

    /// 본체·플러그인 타이머의 가장 이른 기한을 WaitUntil과 별도 waker에 함께 전달한다.
    fn sync_timer_control_flow(&mut self, event_loop: &ActiveEventLoop) {
        let deadline = if self.journal.pauses_observation() {
            None
        } else {
            min_deadline(
                self.timers.next_deadline(),
                self.plugin_manager.as_ref().and_then(|m| m.next_deadline()),
            )
        };
        let deadline = min_deadline(deadline, self.journal.cleanup_poll_deadline());
        let deadline = min_deadline(deadline, self.engine_release_poll_deadline());
        let deadline = min_deadline(deadline, self.runner_stop_poll_deadline());
        let deadline = min_deadline(deadline, self.plugin_file_poll_deadline());
        let deadline = if self.journal.pauses_observation() {
            deadline
        } else {
            min_deadline(
                deadline,
                (self.screenshot_workers.has_pending() || self.explorer_files.has_pending())
                    .then(|| std::time::Instant::now() + std::time::Duration::from_millis(20)),
            )
        };
        let deadline = if self.journal.pauses_observation() {
            deadline
        } else {
            min_deadline(
                deadline,
                self.services
                    .profile_detections
                    .has_pending()
                    .then(|| std::time::Instant::now() + std::time::Duration::from_millis(20)),
            )
        };
        let deadline = if self.journal.pauses_observation() {
            deadline
        } else {
            min_deadline(
                deadline,
                self.engines
                    .all_sessions()
                    .filter_map(|session| session.runtime.input_submit_deadline())
                    .min(),
            )
        };
        self.timer_waker.set_deadline(deadline);
        let deadline = if self.pending_window.is_some() {
            let opening = std::time::Instant::now() + crate::app::boot_machine::BOOT_FRAME_INTERVAL;
            Some(deadline.map_or(opening, |deadline| deadline.min(opening)))
        } else {
            deadline
        };
        event_loop.set_control_flow(match deadline {
            Some(at) => winit::event_loop::ControlFlow::WaitUntil(at),
            None => winit::event_loop::ControlFlow::Wait,
        });
    }

    /// 보이는 DAG만 폴링을 예약하고 숨겨진 뷰의 기존 예약은 취소한다.
    pub(super) fn sync_dag_poll_timers(&mut self, now: std::time::Instant) {
        use crate::adapters::ui::popup::dag_list::DAG_LIST_POPUP_ID;

        let mut active: Vec<(u32, std::time::Instant)> = Vec::new();
        let mut popup_next: Option<std::time::Instant> = None;
        for w in self.view.views.values() {
            let Some(main) = w.as_main() else {
                continue;
            };
            active.extend(main.state.dag_graph_views.pending_poll_deadlines(now));
            if main.state.popups.is_open(DAG_LIST_POPUP_ID) {
                let at = main.state.dialogs.dag_list.next_poll_at(now);
                popup_next = Some(popup_next.map_or(at, |p: std::time::Instant| p.min(at)));
            }
        }
        crate::app::timers::sync_dag_graph_timers(&mut self.timers, &active, now);
        crate::app::timers::sync_dag_list_popup_timer(&mut self.timers, popup_next, now);
    }

    fn mark_dag_graph_window_dirty(&mut self, surface_id: u32) {
        let now = std::time::Instant::now();
        for w in self.view.views.values_mut() {
            let showing = w.as_main().is_some_and(|m| {
                m.state
                    .dag_graph_views
                    .pending_poll_deadlines(now)
                    .iter()
                    .any(|(sid, _)| *sid == surface_id)
            });
            if showing {
                w.mark_dirty();
            }
        }
    }

    fn mark_dag_list_popup_windows_dirty(&mut self) {
        use crate::adapters::ui::popup::dag_list::DAG_LIST_POPUP_ID;
        for w in self.view.views.values_mut() {
            if w.as_main()
                .is_some_and(|m| m.state.popups.is_open(DAG_LIST_POPUP_ID))
            {
                w.mark_dirty();
            }
        }
    }

    /// 메뉴 결과가 마지막 workspace를 닫을 수 있어 처리 직후 빈 창의 닫기 요청도 소비한다.
    fn poll_pending_native_menus(&mut self) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            main.poll_pending_native_menu(&engine.read());
            main.poll_outbound_drag(&engine.read());
        }
        self.close_self_requesting_windows();
    }
}

/// 사용자가 정한 셸은 쓸 수 없어도 저장하지 않는다. 대체 셸은 실행할 때 `ShellConfig` 가 고른다.
/// 대체하지 않는 판정(`ShellPathStatus::falls_back`)은 첫 실행 셸 설정 화면에서 원인을 보여 준다.
fn boot_shell_usable(settings: &crate::settings::Settings) -> bool {
    let general = &settings.general;
    match general.boot_shell_choice() {
        tasty_settings::BootShell::Configured => true,
        tasty_settings::BootShell::Unusable => false,
        tasty_settings::BootShell::Fallback(fallback) => {
            tracing::warn!(
                configured = %general.shell,
                fallback = %fallback,
                "configured shell is unusable; new terminals use the fallback shell until it is fixed"
            );
            true
        }
    }
}

/// 부팅 오류 화면을 닫으면 이 실행이 쓴 instance 파일을 지우고 실패 코드로 끝낸다.
/// 정상 종료 경로를 타지 않으므로 다음 실행이 낡은 기록을 읽지 않게 여기서 지운다.
fn exit_from_boot_error() -> ! {
    crate::boot::single_instance::instance_file::remove_owned();
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::merge_wakeup;
    use winit::event_loop::ControlFlow;

    /// Windows 에서 허용하지 않는 값은 탐지한 Git Bash 가 있어도 설정 화면으로 간다. 설정값은 그대로다.
    #[cfg(windows)]
    #[test]
    fn a_rejected_windows_shell_opens_the_setup_screen_instead_of_a_fallback() {
        for configured in [
            "cmd.exe",
            "C:/Windows/System32/cmd.exe",
            "pwsh",
            r"C:\nonexistent\Git\bin\bash.exe",
        ] {
            let mut settings = crate::settings::Settings::default();
            settings.general.shell = configured.to_string();
            assert!(!super::boot_shell_usable(&settings), "{configured}");
            assert_eq!(settings.general.shell, configured);
            // 실행 셸은 판정에 쓴 경로(이름이면 PATH에서 찾은 파일)이며 탐지한 Git Bash로 대신하지 않는다.
            let shell = crate::core::state::ShellConfig::from_settings(&settings);
            if let Some(detected) = crate::settings::GeneralSettings::detect_bash() {
                assert_ne!(
                    std::path::Path::new(&shell.shell),
                    std::path::Path::new(&detected),
                    "{configured}"
                );
            }
        }
    }

    /// Windows 의 빈 값은 저장한 선택이 없다는 뜻이다. Git Bash 를 찾을 때만 설정 화면 없이 부팅한다.
    #[cfg(windows)]
    #[test]
    fn an_empty_windows_shell_boots_only_with_a_detected_git_bash() {
        let mut settings = crate::settings::Settings::default();
        settings.general.shell = String::new();
        assert_eq!(
            super::boot_shell_usable(&settings),
            crate::settings::GeneralSettings::detect_bash().is_some()
        );
    }

    /// 설정한 셸 경로가 없어도 설정값은 그대로 두고 실행할 때만 대체 셸을 쓴다.
    #[cfg(not(windows))]
    #[test]
    fn missing_configured_shell_is_not_overwritten_at_boot() {
        let Some(detected) = crate::settings::GeneralSettings::detect_bash() else {
            return;
        };
        let configured = "/nonexistent/tasty-t354/user-shell".to_string();
        let mut settings = crate::settings::Settings::default();
        settings.general.shell = configured.clone();
        assert!(super::boot_shell_usable(&settings));
        assert_eq!(settings.general.shell, configured);
        let shell = crate::core::state::ShellConfig::from_settings(&settings);
        assert_eq!(shell.shell, detected);
    }

    /// 실행할 수 없는 경로(디렉터리)는 대체 셸로 넘기지 않는다. 부팅은 원인을 보여 주는 첫 실행 셸 설정
    /// 화면으로 가고 설정값은 그대로다.
    #[cfg(unix)]
    #[test]
    fn a_directory_shell_opens_the_setup_screen_instead_of_a_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let configured = dir.path().to_string_lossy().into_owned();
        let mut settings = crate::settings::Settings::default();
        settings.general.shell = configured.clone();
        assert!(!super::boot_shell_usable(&settings));
        assert_eq!(settings.general.shell, configured);
        let shell = crate::core::state::ShellConfig::from_settings(&settings);
        assert_eq!(shell.shell, configured);
    }

    /// 실행 폴더 기준으로 디렉터리인 상대 값은 절대 경로로 바뀌어도 대체 성공으로 보지 않는다.
    /// 실행 권한 없는 상대 파일도 같다. 둘 다 첫 실행 셸 설정 화면으로 간다.
    #[cfg(unix)]
    #[test]
    fn an_unusable_relative_shell_opens_the_setup_screen() {
        use std::os::unix::fs::PermissionsExt;
        let launch = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(launch.path().join("shdir")).expect("dir");
        let plain = launch.path().join("plain");
        std::fs::write(&plain, "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        tasty_utils::path_resolve::push_launch_dir_override(
            tasty_utils::path_resolve::LaunchDir::Known(launch.path().to_path_buf()),
        );
        let results: Vec<_> = ["./shdir", "./plain"]
            .iter()
            .map(|configured| {
                let mut settings = crate::settings::Settings::default();
                settings.general.shell = configured.to_string();
                let usable = super::boot_shell_usable(&settings);
                let spawn = crate::core::state::ShellConfig::from_settings(&settings).shell;
                (usable, spawn)
            })
            .collect();
        tasty_utils::path_resolve::pop_launch_dir_override();
        assert_eq!(
            results,
            vec![
                (
                    false,
                    launch.path().join("shdir").to_string_lossy().into_owned()
                ),
                (false, plain.to_string_lossy().into_owned()),
            ]
        );
    }

    #[test]
    fn merge_wakeup_keeps_the_earlier_deadline() {
        let now = std::time::Instant::now();
        let early = now + std::time::Duration::from_millis(5);
        let late = now + std::time::Duration::from_millis(50);
        assert_eq!(
            merge_wakeup(ControlFlow::WaitUntil(late), early),
            ControlFlow::WaitUntil(early)
        );
        assert_eq!(
            merge_wakeup(ControlFlow::WaitUntil(early), late),
            ControlFlow::WaitUntil(early)
        );
    }

    #[test]
    fn merge_wakeup_overrides_indefinite_wait() {
        let at = std::time::Instant::now() + std::time::Duration::from_millis(5);
        assert_eq!(
            merge_wakeup(ControlFlow::Wait, at),
            ControlFlow::WaitUntil(at)
        );
    }

    #[test]
    fn merge_wakeup_leaves_poll_alone() {
        let at = std::time::Instant::now() + std::time::Duration::from_millis(5);
        assert_eq!(merge_wakeup(ControlFlow::Poll, at), ControlFlow::Poll);
    }
}
