mod boot_error;
#[cfg(debug_assertions)]
mod debug_surface;
mod delayed_repaint;
mod egui_bridge;
mod egui_mesh_prepare;
mod fonts;
pub(crate) mod loading;
mod overlay_paint;
pub(crate) mod overlay_surface;
mod render_pass;
mod screenshot;
mod shell_setup;
mod surface_lifecycle;
mod text_selection;

use crate::runtime::engine_read::EngineRead;
#[cfg(test)]
pub(crate) use egui_bridge::keep_screen_while_minimized;
use std::sync::Arc;

use anyhow::Result;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::AppEvent;
use crate::gfx::perf::{FrameSample, PerfAggregator};
use crate::model::{LogicalPx, PhysicalPx, PhysicalRect};
use crate::renderer::CellRenderer;
use crate::settings::AppearanceSettings;
use crate::stall_watchdog;
use crate::state::MainViewState;

pub struct ImePreeditState {
    pub text: String,
    /// IME 조합 중 커서 위치. 현재 렌더링에서는 사용하지 않는다.
    #[allow(dead_code)] // 구조체 필드 — 향후 IME caret 렌더용 보존, 현재 미read
    pub cursor: Option<(usize, usize)>,
    pub anchor: tasty_selection::SelectionPoint,
    pub surface_id: u32,
}

/// Actions returned by the shell setup dialog.
pub enum ShellSetupAction {
    None,
    Confirmed,
    Exit,
}

/// GPU·창은 사용할 수 있지만 엔진 생성에 실패했을 때 표시할 번역된 진단.
pub struct BootErrorInfo {
    pub title: String,
    pub body: String,
    pub hint: String,
}

/// Mailbox 가 있으면 Mailbox, 없으면 Fifo. 주 surface 와 오버레이 surface 가 같은 규칙을 쓴다.
fn pick_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if modes.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// configure 크기를 1..=어댑터 한계로 제한한다. 0인 최소화 이벤트는 호출 전에 건너뛴다.
fn clamp_surface_dims(w: u32, h: u32, max: u32) -> (u32, u32) {
    (w.clamp(1, max), h.clamp(1, max))
}

pub struct GpuState {
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) surface_invalid: bool,
    #[cfg(debug_assertions)]
    surface_probe: debug_surface::Probe,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) size: PhysicalSize<u32>,
    pub(super) renderer: CellRenderer,
    pub(crate) egui_ctx: egui::Context,
    pub(super) egui_state: egui_winit::State,
    /// egui's clipboard on a native Wayland start (`text_selection.rs`).
    pub(super) text_selection: Option<crate::file::drag::TextSelection>,
    /// Pastes read from the selection that wait for the next egui frame, with their target tag.
    pub(super) held_pastes: Vec<(u64, String)>,
    pub(super) egui_renderer: egui_wgpu::Renderer,
    /// native WebView 위 창 단위 오버레이 host 와 그 surface. 그림 위치가 오버레이 창인 모드가 필요할 때
    /// WebView 동기화 뒤 훅이 붙인다. OS host 를 만들지 못하면 `None` 이다.
    pub(super) overlay_surface: Option<overlay_surface::OverlaySurface>,
    pub(super) scale_factor: f32,
    /// 마지막으로 적용한 터미널 폰트 설정. 달라졌을 때만 update_font를 호출한다.
    pub(super) last_term_font_sig: String,
    /// Tracks per-surface egui font signatures so we re-register only on change.
    pub(super) surface_font_state: crate::adapters::ui::font_registry::SurfaceFontState,
    /// surface별 Renderer·디코드 캐시. TextureId 충돌을 피하며 레이아웃에서 사라지면 정리한다.
    pub(in crate::gfx::gpu) egui_mesh_targets:
        std::collections::HashMap<u32, egui_mesh_prepare::EguiMeshRenderTarget>,
    /// 팝업별 Renderer·디코드 캐시. 팝업이 닫히면 정리한다.
    pub(in crate::gfx::gpu) egui_mesh_popup_targets:
        std::collections::HashMap<u64, egui_mesh_prepare::EguiMeshRenderTarget>,
    /// 배너별 Renderer·디코드 캐시. host가 그린 셸 위에 콘텐츠를 합성한다.
    pub(in crate::gfx::gpu) egui_mesh_banner_targets:
        std::collections::HashMap<u64, egui_mesh_prepare::EguiMeshRenderTarget>,
    /// textures_delta 체인 단절이 감지된 egui-mesh surface — full 재전송 요청 대기열.
    /// 렌더 prepare 가 적재하고, [`Self::take_egui_mesh_full_requests`] 로 forward 측이
    /// 다음 tick 에 소비해 `need_full_textures` set_context 를 보낸다.
    pub(in crate::gfx::gpu) egui_mesh_full_requests: std::collections::HashSet<u32>,
    /// popup 대응 full 재전송 요청 대기열 (instance_id).
    pub(in crate::gfx::gpu) egui_mesh_popup_full_requests: std::collections::HashSet<u64>,
    /// banner 대응 full 재전송 요청 대기열 (instance_id).
    pub(in crate::gfx::gpu) egui_mesh_banner_full_requests: std::collections::HashSet<u64>,
    /// attach mesh mirror surface(`AttachMeshSurface`) local surface_id → 전용
    /// `egui_wgpu::Renderer` + 디코드 캐시 (`docs/dev-guide/attach-behavior.md` "mesh mirror 채널").
    /// `egui_mesh_targets`와 별도 맵인 이유는
    /// [`egui_mesh_prepare::GpuState::render_attach_mesh_surfaces`] 문서 참조.
    pub(in crate::gfx::gpu) attach_mesh_targets:
        std::collections::HashMap<u32, egui_mesh_prepare::EguiMeshRenderTarget>,
    /// attach mesh mirror surface 의 텍스처 delta 체인 단절 → full 재전송 요청 대기열.
    /// [`Self::take_attach_mesh_full_requests`]로 drain, `attach_client`가 소비해
    /// `StreamControl::MeshFullResendRequest`를 서버로 보낸다.
    pub(in crate::gfx::gpu) attach_mesh_full_requests: std::collections::HashSet<u32>,
    /// When set, the next render will capture the full window frame to this path as PNG.
    pub pending_screenshot: Option<std::path::PathBuf>,
    /// When set, the next render will capture the given terminal surface to this
    /// path as PNG via an offscreen pass — independent of the swapchain, visible
    /// tab, and focus (the surface is rendered at its own grid size). See
    /// [`Self::capture_surface_to_png`].
    pub pending_surface_screenshot: Option<(u32, std::path::PathBuf)>,
    /// Frame timing 집계기. `TASTY_LOG=tasty::gfx::perf=info` 일 때만 출력.
    pub(super) perf: PerfAggregator,
    /// Set when a frame hid the focused terminal cursor during an output burst.
    /// The view consumes this to request one follow-up redraw so the cursor
    /// reappears after the burst settles even if no more PTY bytes arrive.
    pub(super) terminal_cursor_restore_pending: bool,
    /// 타이틀바 닫기 등을 해당 창의 AppEvent로 보내는 proxy.
    pub(super) proxy: EventLoopProxy<AppEvent>,
}

impl GpuState {
    /// App이 공유하는 Instance·Adapter로 창별 GPU 자원을 만든다.
    /// App은 모든 창보다 오래 Instance를 보관해 surface의 수명을 보장한다.
    pub(crate) async fn new_shared(
        instance: &Arc<wgpu::Instance>,
        adapter: &Arc<wgpu::Adapter>,
        window: Arc<Window>,
        appearance: &AppearanceSettings,
        theme_runtime: tasty_themes::ThemeRuntime,
        wheel_line_scroll: f32,
        proxy: EventLoopProxy<AppEvent>,
    ) -> Result<Self> {
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let surface = instance.create_surface(window.clone())?;

        tracing::info!(
            "GPU adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("tasty_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .or_else(|| surface_caps.formats.first().copied())
            .ok_or_else(|| anyhow::anyhow!("no supported surface format found"))?;

        // 초기 크기도 resize와 같은 어댑터 한계로 제한한다.
        let max_dim = device.limits().max_texture_dimension_2d;
        let (config_width, config_height) = clamp_surface_dims(size.width, size.height, max_dim);
        // 가상 디스플레이에서는 Fifo만으로 프레임 상한을 알 수 없어 실제 선택값을 기록한다.
        let present_mode = pick_present_mode(&surface_caps.present_modes);
        tracing::info!(
            "surface present_mode={present_mode:?} (available: {:?})",
            surface_caps.present_modes
        );
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: surface_format,
            width: config_width,
            height: config_height,
            present_mode,
            alpha_mode: surface_caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let (term_font, last_term_font_sig) =
            egui_bridge::boot_terminal_font(appearance, scale_factor);
        let renderer = CellRenderer::new(&device, &queue, surface_format, term_font);

        let egui_ctx = egui::Context::default();

        // Disable egui's built-in Ctrl+/- zoom — it only affects egui widgets
        // but not the terminal renderer, causing inconsistent scaling.
        egui_ctx.options_mut(|opts| {
            opts.zoom_with_keyboard = false;
            // 이 Context의 스크롤 거리는 Tasty 설정을 공유한다(docs/surfaces/terminal/index.md).
            opts.line_scroll_speed = wheel_line_scroll;
        });

        egui_extras::install_image_loaders(&egui_ctx);
        tasty_icons::install_texture_loader(&egui_ctx);

        Self::setup_egui_fonts(&egui_ctx);

        let repaint_proxy = proxy.clone();
        // 각 Context가 같은 root viewport ID를 사용하므로 window_id로 다시 그리기를 보낸다.
        let repaint_window_id = window.id();
        let delayed = delayed_repaint::DelayedRepaintGate::default();
        egui_ctx.set_request_repaint_callback(move |info: egui::RequestRepaintInfo| {
            // 지연 요청은 루프가 그 시각에 깨어나도록 타이머로 예약한다. 이미 더 이른 예약이 있으면 보내지 않는다.
            let event = if info.delay.is_zero() {
                AppEvent::EguiRepaint {
                    window_id: repaint_window_id,
                }
            } else if let Some(at) = delayed.admit(
                info.delay
                    .saturating_add(delayed_repaint::egui_frame_allowance()),
                std::time::Instant::now(),
            ) {
                AppEvent::EguiRepaintAfter {
                    window_id: repaint_window_id,
                    at,
                }
            } else {
                return;
            };
            crate::shortcuts::send_app_event(&repaint_proxy, event);
        });

        tasty_themes::install_global_with_runtime(appearance, theme_runtime);
        Self::apply_theme(&egui_ctx, &appearance.theme);

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            Some(scale_factor),
            None,
            Some(2048),
        );

        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1, false);

        Ok(Self {
            surface,
            surface_invalid: false,
            #[cfg(debug_assertions)]
            surface_probe: Default::default(),
            device,
            queue,
            config,
            size,
            renderer,
            egui_ctx,
            egui_state,
            text_selection: None,
            held_pastes: Vec::new(),
            egui_renderer,
            overlay_surface: None,
            scale_factor,
            last_term_font_sig,
            surface_font_state: crate::adapters::ui::font_registry::SurfaceFontState::default(),
            egui_mesh_targets: std::collections::HashMap::new(),
            egui_mesh_popup_targets: std::collections::HashMap::new(),
            egui_mesh_banner_targets: std::collections::HashMap::new(),
            egui_mesh_full_requests: std::collections::HashSet::new(),
            egui_mesh_popup_full_requests: std::collections::HashSet::new(),
            egui_mesh_banner_full_requests: std::collections::HashSet::new(),
            attach_mesh_targets: std::collections::HashMap::new(),
            attach_mesh_full_requests: std::collections::HashSet::new(),
            pending_screenshot: None,
            pending_surface_screenshot: None,
            perf: PerfAggregator::new(),
            terminal_cursor_restore_pending: false,
            proxy,
        })
    }

    /// Pass a winit event to egui. Returns (consumed, repaint).
    pub fn handle_egui_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> (bool, bool) {
        // egui 는 비-macOS 의 Win·Super 를 모르므로 option 축 판정용으로 따로 남긴다.
        // 포커스를 잃은 채 키를 떼면 ModifiersChanged 가 오지 않을 수 있어 그때 비운다.
        match event {
            winit::event::WindowEvent::ModifiersChanged(m) => {
                tasty_key_match::note_modifiers(&self.egui_ctx, m.state());
            }
            winit::event::WindowEvent::Focused(false) => {
                tasty_key_match::note_modifiers(&self.egui_ctx, Default::default());
            }
            _ => {}
        }
        self.route_paste_key(event);
        let response = self.egui_state.on_window_event(window, event);
        (response.consumed, response.repaint)
    }

    /// 다음 egui 프레임에 넣을 입력 이벤트. 호스트 입력 위젯의 키 재해석이 쓴다.
    pub fn egui_input_events_mut(&mut self) -> &mut Vec<egui::Event> {
        &mut self.egui_state.egui_input_mut().events
    }

    /// 지금 붙여 넣을 egui 클립보드 텍스트. 네이티브 Wayland에서 다른 앱의 선택은 나중에 Paste로 온다.
    pub fn egui_clipboard_text(&mut self) -> Option<String> {
        self.clipboard_text_now()
    }

    /// 메뉴를 닫는 클릭처럼 egui에 전달하지 않은 입력 뒤에 PointerGone을 넣어
    /// 이전 hover와 누름 상태를 정리한다.
    pub fn push_egui_pointer_gone(&mut self) {
        self.egui_state
            .egui_input_mut()
            .events
            .push(egui::Event::PointerGone);
    }

    /// 렌더 prepare 가 적재한 egui-mesh full 재전송 요청(surface_id)을 비우며 가져간다.
    /// MainView 가 render 직후 소비해, 다음 tick 의 forward 에서 해당 surface 에
    /// `need_full_textures` set_context 를 보낸다.
    pub fn take_egui_mesh_full_requests(&mut self) -> std::collections::HashSet<u32> {
        std::mem::take(&mut self.egui_mesh_full_requests)
    }

    /// popup 대응 full 재전송 요청(instance_id) drain — [`Self::take_egui_mesh_full_requests`].
    pub fn take_egui_mesh_popup_full_requests(&mut self) -> std::collections::HashSet<u64> {
        std::mem::take(&mut self.egui_mesh_popup_full_requests)
    }

    /// banner 대응 full 재전송 요청(instance_id) drain — [`Self::take_egui_mesh_full_requests`].
    pub fn take_egui_mesh_banner_full_requests(&mut self) -> std::collections::HashSet<u64> {
        std::mem::take(&mut self.egui_mesh_banner_full_requests)
    }

    /// attach mesh mirror surface 의 full 재전송 요청(local surface_id) drain —
    /// `attach_client`가 매 tick 소비해 owning 세션에 `MeshFullResendRequest`를 보낸다.
    pub fn take_attach_mesh_full_requests(&mut self) -> std::collections::HashSet<u32> {
        std::mem::take(&mut self.attach_mesh_full_requests)
    }

    /// Whether the last render hid a terminal cursor and needs one follow-up
    /// frame to restore it after the output burst quiets down.
    pub fn take_terminal_cursor_restore_pending(&mut self) -> bool {
        std::mem::take(&mut self.terminal_cursor_restore_pending)
    }

    /// Render the full frame: egui UI + terminal surfaces.
    #[allow(clippy::too_many_arguments)] // reason: 프레임 렌더 컨텍스트 전체
    pub fn render(
        &mut self,
        state: &mut MainViewState,
        engine: &EngineRead<'_>,
        window: &Window,
        preedit: Option<&ImePreeditState>,
        selection: Option<&tasty_selection::TextSelection>,
        vi_cursor: Option<(u32, tasty_selection::SelectionPoint)>,
        link_hover: Option<(u32, &tasty_terminal_link::LinkHighlight)>,
        plugin_manager: Option<crate::app::plugin_display::PluginDisplay<'_>>,
    ) -> Result<(), wgpu::SurfaceError> {
        let render_start = std::time::Instant::now();
        #[cfg(debug_assertions)]
        self.debug_main_frame(window.id())?;

        // surface 캡처를 먼저 처리한다. 별도 텍스처를 쓰고 투영값을 복원해 뒤의 화면 렌더에 영향이 없게 한다.
        self.handle_pending_surface_screenshot(state, &engine.as_ref());

        // 무대 분기는 surface 캡처 뒤, 레이아웃·PTY 크기 갱신 전에 둔다.
        // 앞당기면 surface 캡처를 처리하지 못하고 뒤로 미루면 배경 PTY 크기가 바뀐다.
        // 호출부의 attach 중계도 건너뛰지 않아야 한다. 무대 경로는 창 캡처와 present를 유지한다.
        // docs/design/systems/fullscreen-stage.md 참고.
        if state.fullscreen_stage_active() {
            return self.render_fullscreen_stage(state, engine, window);
        }

        state.sidebar_width = if !state.sidebar_visible {
            LogicalPx(0.0)
        } else if state.sidebar_collapsed {
            LogicalPx(48.0) // Compact mode: narrow width for collapse button
        } else {
            engine.settings.appearance.scaled_sidebar_width()
        };
        let terminal_rect = self.compute_terminal_rect(state.sidebar_width);
        let (pane_rects, dividers, focused_surface_id) =
            self.prepare_layout(state, engine, terminal_rect);

        let layout_ms = render_start.elapsed().as_secs_f64() * 1000.0;

        // 이름 있는 폰트 family를 첫 렌더 전에 등록한다.
        let prev_theme = engine.settings.appearance.theme.clone();
        crate::adapters::ui::font_registry::refresh_surface_fonts(
            &self.egui_ctx,
            &engine.settings.appearance,
            &mut self.surface_font_state,
        );

        // host·plugin 팝업의 셸 정렬과 GPU 콘텐츠 합성이 같은 결정을 사용하도록 한 번 계산한다.
        let host_top_z_seq = state.popups.max_open_z_seq();
        let plugin_top_z_seq =
            plugin_manager.and_then(|m| m.popup_instances().map(|(_, inst)| inst.z_seq).max());
        let host_popup_on_top =
            egui_bridge::host_popup_should_render_on_top(host_top_z_seq, plugin_top_z_seq);

        stall_watchdog::set_phase(stall_watchdog::Phase::Acquire);
        let output = self.acquire_surface()?;
        let t0 = std::time::Instant::now();
        let mut full_output = self.run_egui_frame(
            state,
            engine,
            window,
            &pane_rects,
            &dividers,
            terminal_rect,
            plugin_manager,
            host_popup_on_top,
        );
        let egui_frame_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 창 가장자리 리사이즈 커서를 surface·링크 커서로 덮지 않는다.
        if let Some(icon) = self.resolve_cursor_icon(state, engine, terminal_rect, link_hover) {
            full_output.platform_output.cursor_icon = icon;
        }

        let t0 = std::time::Instant::now();
        self.post_egui_update(engine, &prev_theme);
        let post_egui_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 터미널 IME가 egui의 비입력 프레임 때문에 꺼지지 않도록 처리한다.
        // 실제 허용 여부는 apply_platform_output에서 결정한다.
        let t0 = std::time::Instant::now();
        self.apply_platform_output(window, state, full_output.platform_output);
        let platform_output_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t0 = std::time::Instant::now();
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: full_output.pixels_per_point,
        };
        let tessellate_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t0 = std::time::Instant::now();
        let regions = state.surface_regions(engine, terminal_rect, self.scale_factor);
        stall_watchdog::set_phase(stall_watchdog::Phase::Submit);
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render_clear_pass(&view, state, engine);
        let clear_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t0 = std::time::Instant::now();
        let search_state = if state.search.matches.is_empty() {
            None
        } else {
            Some(&state.search)
        };
        self.render_terminals(
            &view,
            &regions,
            &engine.as_ref(),
            focused_surface_id,
            selection,
            vi_cursor,
            &state.terminal_views,
            preedit,
            link_hover,
            search_state,
        );
        let terminals_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 레이아웃에서 사라진 자원도 정리해야 하므로 합성 대상이 없어도 호출한다.
        if let Some(mgr) = plugin_manager {
            let mesh_targets = egui_mesh_prepare::collect_egui_mesh_targets(
                state,
                engine,
                terminal_rect,
                self.scale_factor,
            );
            let mesh_existing = state.egui_mesh_surfaces_existing(engine);
            self.render_egui_mesh_surfaces(&view, &mesh_targets, &mesh_existing, mgr);
        }

        // 원격 mesh는 로컬 PluginManager 없이 수신한 AttachMeshFrameStore에서 읽는다.
        let attach_mesh_targets = egui_mesh_prepare::collect_attach_mesh_targets(
            state,
            engine,
            terminal_rect,
            self.scale_factor,
        );
        let attach_mesh_existing = state.attach_mesh_surfaces_existing(engine);
        if !attach_mesh_targets.is_empty()
            || !attach_mesh_existing.is_empty()
            || !self.attach_mesh_targets.is_empty()
        {
            self.render_attach_mesh_surfaces(
                &view,
                &attach_mesh_targets,
                &attach_mesh_existing,
                engine.attach_mesh_frames,
            );
        }

        let t0 = std::time::Instant::now();
        self.render_egui_pass_and_mesh_popups(
            &view,
            &full_output.textures_delta,
            &paint_jobs,
            &screen_descriptor,
            state.plugin_mesh_popup_regions.as_slice(),
            plugin_manager,
            host_popup_on_top,
            None,
        );
        let egui_pass_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 구멍 방식 그림은 주 pass 가 이미 그렸다. 주 표면 present 뒤에 기록한다.
        let main_overlay_job =
            crate::state::webview_overlay::take_main_surface_job(&mut state.webview_overlay);
        self.paint_webview_overlay(
            state,
            full_output.pixels_per_point,
            plugin_manager,
            host_popup_on_top,
        );
        self.free_egui_textures(&full_output.textures_delta);

        // 배너 셸 위에 콘텐츠를 합성한다. 빈 regions로도 호출해 닫힌 배너의 Renderer를 정리한다.
        if let Some(mgr) = plugin_manager {
            let regions = state.plugin_mesh_banner_regions.clone();
            self.render_egui_mesh_banners(&view, &regions, mgr);
        }

        let t0 = std::time::Instant::now();
        if let Some(path) = self.pending_screenshot.take() {
            self.capture_frame_to_png(&output.texture, self.size.width, self.size.height, &path);
        }
        stall_watchdog::set_phase(stall_watchdog::Phase::Present);
        // debug 결함 주입 지점 — release 는 no-op.
        stall_watchdog::take_debug_stall();
        self.present_surface(output);
        if let Some(job) = main_overlay_job {
            let size = [self.size.width, self.size.height];
            state.webview_overlay.record_present(job, size);
        }
        let present_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let gpu_total_ms = render_start.elapsed().as_secs_f64() * 1000.0;
        const SLOW_RENDER_MS: f64 = 30.0;
        if gpu_total_ms > SLOW_RENDER_MS {
            tracing::warn!(
                "slow gpu render: {gpu_total_ms:.1}ms \
                 [layout={layout_ms:.1}, egui_frame={egui_frame_ms:.1}, \
                 post_egui={post_egui_ms:.1}, platform_output={platform_output_ms:.1}, \
                 tessellate={tessellate_ms:.1}, clear={clear_ms:.1}, \
                 terminals={terminals_ms:.1}, egui_pass={egui_pass_ms:.1}, \
                 present={present_ms:.1}]"
            );
        }

        let (_, _, draw_total) = self.renderer.draw_call_count();
        self.perf.push(FrameSample {
            gpu_total_ms,
            terminals_ms,
            draw_calls_total: draw_total,
            surfaces: self.renderer.active_surface_count(),
            atlas_evictions: self.renderer.atlas.eviction_count(),
            atlas_active_pages: self.renderer.atlas.active_page_count(),
            atlas_entry_count_sum: self.renderer.atlas.entry_count_sum(),
        });

        Ok(())
    }

    /// Pending offscreen surface screenshot(agent action, focus-independent) 소비.
    /// A hard-occupied surface shows a readonly mirror server-side; capture what
    /// the user would see (mirror), else the live terminal.
    fn handle_pending_surface_screenshot(
        &mut self,
        state: &MainViewState,
        engine: &EngineRead<'_>,
    ) {
        let Some((surface_id, path)) = self.pending_surface_screenshot.take() else {
            return;
        };
        let reverse_screen = engine.settings.general.reverse_screen_enabled;
        match engine.visible_terminal(surface_id) {
            Some(t) => self.capture_surface_to_png(
                t,
                &state.terminal_views.get(engine, surface_id),
                reverse_screen,
                &path,
            ),
            None => {
                tracing::warn!(
                    "surface screenshot: surface {surface_id} has no terminal to capture"
                )
            }
        }
    }

    /// 리사이즈 커서가 아니면 egui·surface·링크 순으로 커서를 판단한다.
    /// None이면 egui가 정한 커서를 그대로 둔다.
    fn resolve_cursor_icon(
        &self,
        state: &MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        terminal_rect: PhysicalRect,
        link_hover: Option<(u32, &tasty_terminal_link::LinkHighlight)>,
    ) -> Option<egui::CursorIcon> {
        let mut icon = None;
        if state.pending_resize_cursor.is_none()
            && !self.egui_ctx.is_pointer_over_area()
            && !state.popup_hovered
            && !state.banner_hovered
            && !state.modifier_hint_hovered
            && let Some(pos) = self.egui_ctx.input(|i| i.pointer.hover_pos())
        {
            // egui 가 준 hover 좌표는 논리, `winit_cursor_icon_at` 은 물리를 받는다.
            let px = LogicalPx(pos.x).to_physical(self.scale_factor).value();
            let py = LogicalPx(pos.y).to_physical(self.scale_factor).value();
            icon = state.winit_cursor_icon_at(
                engine,
                px,
                py,
                terminal_rect,
                crate::state::mouse::divider_hit_threshold_physical(self.scale_factor),
            );
        }
        if link_hover.is_some() && state.pending_resize_cursor.is_none() {
            icon = Some(egui::CursorIcon::PointingHand);
        }
        icon
    }

    /// 배경 콘텐츠 대신 전체화면 무대를 그린다. 창 캡처와 present는 이 경로에서도 처리한다.
    fn render_fullscreen_stage(
        &mut self,
        state: &mut MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        window: &Window,
    ) -> Result<(), wgpu::SurfaceError> {
        // 무대에서도 입력을 소비해 나간 뒤 한꺼번에 전달되지 않게 한다.
        stall_watchdog::set_phase(stall_watchdog::Phase::Acquire);
        let output = self.acquire_surface()?;
        let raw_input = self.take_egui_input(window);
        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output: _,
        } = self.egui_ctx.run(raw_input, |ctx| {
            crate::adapters::ui::draw_fullscreen_stage(ctx, state, engine);
        });
        self.apply_platform_output(window, state, platform_output);

        let paint_jobs = self.egui_ctx.tessellate(shapes, pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point,
        };

        stall_watchdog::set_phase(stall_watchdog::Phase::Submit);
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.render_clear_pass(&view, state, engine);
        self.render_egui_pass(&view, &textures_delta, &paint_jobs, &screen_descriptor);
        self.free_egui_textures(&textures_delta);

        if let Some(path) = self.pending_screenshot.take() {
            self.capture_frame_to_png(&output.texture, self.size.width, self.size.height, &path);
        }
        stall_watchdog::set_phase(stall_watchdog::Phase::Present);
        let present_start = std::time::Instant::now();
        output.present();
        tracing::debug!(target: "tasty::gpu::surface", phase = "present",
            elapsed_us = present_start.elapsed().as_micros() as u64);
        Ok(())
    }

    /// host 팝업이 포커스를 갖고 텍스트 입력이 아니면 IME를 끈다.
    /// Windows는 한/영 전환 문제를 피하기 위해 항상 허용한다.
    fn apply_platform_output(
        &mut self,
        window: &Window,
        state: &MainViewState,
        platform_output: egui::PlatformOutput,
    ) {
        self.egui_state.set_allow_ime(false);

        #[cfg(not(windows))]
        let ime_widget_focused = platform_output.ime.is_some();

        self.apply_egui_output(window, platform_output);

        #[cfg(not(windows))]
        {
            // plugin 팝업 입력은 host 위젯 포커스에 나타나지 않아 이 IME 비활성 조건에 포함하지 않는다.
            let disable_ime = state.popups.has_focused() && !ime_widget_focused;
            window.set_ime_allowed(!disable_ime);
        }
        #[cfg(windows)]
        {
            // Windows는 팝업 포커스와 무관하게 IME를 허용하므로 state를 읽지 않는다.
            let _ = state;
            window.set_ime_allowed(true);
        }
    }

    /// 자원 개수를 바꾸지 않고 읽어 system.gpu_stats에 제공한다.
    pub(crate) fn resource_stats(&self) -> serde_json::Value {
        let (bg_draws, glyph_draws, total_draws) = self.renderer.draw_call_count();
        serde_json::json!({
            "egui_mesh_targets": self.egui_mesh_targets.len(),
            "egui_mesh_popup_targets": self.egui_mesh_popup_targets.len(),
            "egui_mesh_banner_targets": self.egui_mesh_banner_targets.len(),
            "attach_mesh_targets": self.attach_mesh_targets.len(),
            "atlas": {
                "eviction_count": self.renderer.atlas.eviction_count(),
                "active_pages": self.renderer.atlas.active_page_count(),
                "entry_count_sum": self.renderer.atlas.entry_count_sum(),
            },
            "draw_calls": { "bg": bg_draws, "glyph": glyph_draws, "total": total_draws },
            "active_surfaces": self.renderer.active_surface_count(),
        })
    }

    fn compute_terminal_rect(&self, sidebar_width: LogicalPx) -> PhysicalRect {
        crate::model::compute_terminal_rect(
            PhysicalPx(self.size.width as f32),
            PhysicalPx(self.size.height as f32),
            sidebar_width,
            crate::adapters::ui::titlebar::top_inset(self.scale_factor),
            crate::adapters::ui::status_bar_bottom_inset(self.scale_factor),
            self.scale_factor,
        )
    }

    fn prepare_layout(
        &self,
        state: &MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        terminal_rect: PhysicalRect,
    ) -> (Vec<(u32, PhysicalRect)>, Vec<PhysicalRect>, Option<u32>) {
        let workspace = state.active_workspace(engine);
        let pane_layout = workspace.pane_layout();
        let pane_rects = state.pane_rects(engine, workspace, terminal_rect, self.scale_factor);
        let mut dividers = state.pane_dividers(engine, workspace, terminal_rect, self.scale_factor);

        let focused_surface_id = state.focused_surface_id(engine);
        for (pane_id, pane_rect) in &pane_rects {
            if let Some(pane) = pane_layout.find_pane(*pane_id) {
                let tab_bar_h = state.tab_bar_height;
                let content_rect = PhysicalRect {
                    x: pane_rect.x,
                    y: pane_rect.y + tab_bar_h,
                    width: pane_rect.width,
                    height: (pane_rect.height - tab_bar_h).max(PhysicalPx(1.0)),
                };
                if let Some(tab) = pane.tabs.get(state.navigation.tab_index(pane)) {
                    dividers.extend(state.surface_dividers(
                        engine,
                        tab,
                        content_rect,
                        self.scale_factor,
                    ));
                }
            }
        }
        (pane_rects, dividers, focused_surface_id)
    }

    /// Compute grid size for a given rect.
    pub fn grid_size_for_rect(&self, rect: &PhysicalRect) -> (usize, usize) {
        self.renderer.grid_size_for_rect(rect)
    }

    pub fn cell_width(&self) -> f32 {
        self.renderer.cell_width()
    }

    pub fn cell_height(&self) -> f32 {
        self.renderer.cell_height()
    }

    #[cfg(debug_assertions)]
    pub fn terminal_font_updates(&self) -> u64 {
        self.renderer.font_updates()
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Get egui's actual pixels_per_point (what it uses for rendering).
    // 이유: 호출부가 debug_info.rs/debug_input.rs(개발자 로컬 디버그 전용) 뿐이라
    // release 빌드에서 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn egui_pixels_per_point(&self) -> f32 {
        self.egui_ctx.pixels_per_point()
    }

    /// debug 전용: 다음 frame 의 egui 입력 큐에 합성 이벤트를 주입한다(egui-mesh popup
    /// 입력 forward 의 헤드리스 검증용). release 미노출 — 사용자 입력 재현은 debug 격리.
    #[cfg(debug_assertions)]
    pub fn debug_push_egui_events(&mut self, events: Vec<egui::Event>) {
        self.egui_state.egui_input_mut().events.extend(events);
        self.egui_ctx.request_repaint();
    }

    /// debug 전용: egui 가 누른 것으로 보는 수식 키를 바꾼다. egui-winit 은 실제
    /// `ModifiersChanged` 가 올 때만 이 값을 고치므로 다음 주입까지 유지된다.
    #[cfg(debug_assertions)]
    pub fn debug_set_egui_modifiers(&mut self, modifiers: egui::Modifiers) {
        self.egui_state.egui_input_mut().modifiers = modifiers;
    }

    /// Get egui's zoom factor.
    // 이유: 호출부가 debug_info.rs/debug_input.rs(개발자 로컬 디버그 전용) 뿐이라
    // release 빌드에서 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn egui_zoom_factor(&self) -> f32 {
        self.egui_ctx.zoom_factor()
    }

    /// Whether egui-winit currently allows IME on the window.
    // 이유: 호출부가 debug_info.rs(개발자 로컬 디버그 전용) 뿐이라 release 빌드에서
    // 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn egui_ime_allowed(&self) -> bool {
        self.egui_state.allow_ime()
    }

    /// Get the wgpu surface config dimensions.
    // 이유: 호출부가 debug_info.rs/debug_input.rs(개발자 로컬 디버그 전용) 뿐이라
    // release 빌드에서 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn surface_config_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Update the scale factor (e.g., when the window moves between monitors with different DPI).
    pub fn update_scale_factor(&mut self, new_scale_factor: f32) {
        self.scale_factor = new_scale_factor;
        // native_pixels_per_point를 직접 쓰도록 zoom을 1로 초기화한다.
        // set_pixels_per_point는 아직 갱신 전인 native 값으로 zoom을 계산할 수 있어 사용하지 않는다.
        self.egui_ctx.set_zoom_factor(1.0);
    }

    /// Re-sync scale factor from the window and resize if it changed.
    /// Returns true if scale factor was updated.
    pub fn sync_scale_factor(&mut self, window: &Window) -> bool {
        let current_sf = window.scale_factor() as f32;
        if (current_sf - self.scale_factor).abs() > f32::EPSILON {
            self.update_scale_factor(current_sf);
            let new_size = window.inner_size();
            self.resize(new_size);
            true
        } else {
            false
        }
    }
}

#[cfg(feature = "gui")]
impl GpuState {
    /// Native drag owns the release after handoff; prevent a stuck egui button or internal drop.
    pub(crate) fn release_drag_pointer(&mut self) {
        let pos = self
            .egui_ctx
            .input(|i| i.pointer.latest_pos())
            .unwrap_or_default();
        self.egui_state
            .egui_input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            });
        egui::DragAndDrop::clear_payload(&self.egui_ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_surface_dims;

    const MAX: u32 = 8192;

    #[test]
    fn passes_through_valid_dims() {
        assert_eq!(clamp_surface_dims(1, 1, MAX), (1, 1));
        assert_eq!(clamp_surface_dims(MAX, MAX, MAX), (MAX, MAX));
        assert_eq!(clamp_surface_dims(1280, 720, MAX), (1280, 720));
    }

    #[test]
    fn clamps_upper_bound() {
        assert_eq!(clamp_surface_dims(MAX + 1, 720, MAX), (MAX, 720));
        assert_eq!(clamp_surface_dims(1100, 65535, MAX), (1100, MAX));
        assert_eq!(clamp_surface_dims(65535, 65535, MAX), (MAX, MAX));
    }

    #[test]
    fn raises_zero_to_lower_bound() {
        assert_eq!(clamp_surface_dims(0, 720, MAX), (1, 720));
        assert_eq!(clamp_surface_dims(1280, 0, MAX), (1280, 1));
    }
}
