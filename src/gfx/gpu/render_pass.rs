use crate::app::plugin_display::PluginDisplay;
use crate::model::PhysicalRect;
use crate::renderer::RenderPreedit;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;

use super::GpuState;

/// 터미널 pass 한 번의 구간 시간과 직전 프레임과 바뀐 양.
pub(super) struct TerminalPassCost {
    /// `begin_frame`부터 모든 surface의 인스턴스를 쌓을 때까지.
    pub build_ms: f64,
    /// 버퍼 업로드, draw 기록, 제출.
    pub submit_ms: f64,
    /// perf 계측이 꺼진 프레임은 `None`이다.
    pub changes: Option<crate::gfx::renderer::FrameChanges>,
}

impl GpuState {
    pub(super) fn render_clear_pass(
        &self,
        view: &wgpu::TextureView,
        _state: &MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let bg_alpha = engine.settings.appearance.background_opacity as f64;
        let th = crate::theme::theme();
        let bg = th.bg_panel().to_gpu_rgba();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear_pass"),
            });
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg.r() as f64,
                            g: bg.g() as f64,
                            b: bg.b() as f64,
                            a: bg_alpha,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.submit_commands(std::iter::once(encoder.finish()));
    }

    #[allow(clippy::too_many_arguments)] // reason: 터미널 렌더 컨텍스트 전체
    pub(super) fn render_terminals(
        &mut self,
        view: &wgpu::TextureView,
        regions: &[(u32, PhysicalRect, Vec<crate::model::SurfaceRegion<'_>>)],
        engine: &EngineRead<'_>,
        key_focus: Option<u32>,
        selection: Option<&tasty_selection::TextSelection>,
        vi_cursor: Option<(u32, tasty_selection::SelectionPoint)>,
        terminal_views: &crate::state::terminal_view::TerminalViewports,
        preedit: Option<&super::ImePreeditState>,
        link_hover: Option<(u32, &tasty_terminal_link::LinkHighlight)>,
        search: Option<&crate::search_state::SearchState>,
    ) -> TerminalPassCost {
        let build_start = std::time::Instant::now();
        self.terminal_cursor_restore_pending = false;
        let theme = crate::theme::theme();
        let term_surface = theme.surface("terminal");
        // ANSI 16 팔레트는 *프레임당 1회* 만 추출 — 셀별 lock 비용 제거.
        let ansi = theme.ansi_palette();
        // DECSCNM 렌더 허용 여부 — 프레임당 1회 읽어 모든 surface 에 동일 적용.
        let reverse_screen_enabled = engine.settings.general.reverse_screen_enabled;

        // Accumulate instance data for every surface into the renderer's
        // shared vecs, recording per-surface (rect, bg range, glyph range).
        self.renderer.begin_frame();
        self.renderer
            .begin_change_frame(crate::gfx::perf::changes_enabled());

        for (_pane_id, _pane_rect, surface_regions) in regions {
            for region in surface_regions {
                // hard 점유 surface는 서버가 보관한 읽기 전용 mirror를 그린다.
                let is_readonly = engine.live.occupancy.is_hard_occupied(region.id);
                // 첫 attach 뷰 tick 전이면 mirror 가 아직 없다 — 다음 tick 에 채워진다.
                let Some(terminal) = engine.visible_terminal(region.id) else {
                    continue;
                };
                let surface_id = &region.id;
                let rect = &region.rect;
                // 읽기 전용 화면은 포커스 값과 포커스 커서를 표시하지 않는다.
                let paint = crate::state::surface_focus::TerminalFocusPaint::of(
                    key_focus,
                    *surface_id,
                    &engine.live.occupancy,
                    || terminal.should_suppress_cursor_during_output(),
                );
                let is_focused = paint.bed_focused;
                let bg = if is_focused {
                    term_surface.focused_bg.to_gpu_rgba()
                } else {
                    term_surface.unfocused_bg.to_gpu_rgba()
                };
                let fg = if is_focused {
                    term_surface.focused_fg.to_gpu_rgba()
                } else {
                    term_surface.unfocused_fg.to_gpu_rgba()
                };

                // 읽기 전용에서도 로컬 선택·복사는 표시한다. IME·vi 커서·링크·검색은 제외한다.
                let sel_info = selection
                    .filter(|s| s.surface_id == *surface_id && !s.is_empty())
                    .map(|s| {
                        (
                            s.normalized(),
                            theme.selection_bg.to_gpu_rgba(),
                            theme.text_on_active().to_gpu_rgba(),
                        )
                    });
                let sel_ref = sel_info.as_ref();

                let vi_cursor_info = vi_cursor
                    .filter(|_| !is_readonly)
                    .filter(|(sid, _)| sid == surface_id)
                    .map(|(_, pt)| {
                        (
                            pt,
                            theme.vi_cursor_bg.to_gpu_rgba(),
                            theme.vi_cursor_fg().to_gpu_rgba(),
                        )
                    });
                let vi_cursor_ref = vi_cursor_info.as_ref();

                let render_preedit = preedit
                    .filter(|_| !is_readonly)
                    .filter(|ime| ime.surface_id == *surface_id && !ime.text.is_empty())
                    .map(|ime| RenderPreedit {
                        text: ime.text.clone(),
                        anchor: ime.anchor,
                        bg_color: theme.accent_primary().to_gpu_rgba(),
                        fg_color: theme.bg_panel().to_gpu_rgba(),
                    });
                let render_preedit_ref = render_preedit.as_ref();

                let link_for_this = link_hover
                    .filter(|_| !is_readonly)
                    .filter(|(sid, _)| sid == surface_id)
                    .map(|(_, h)| h);

                let search_highlights = search
                    .filter(|_| !is_readonly)
                    .filter(|s| s.surface_id == *surface_id && !s.matches.is_empty())
                    .map(|s| {
                        crate::renderer::SearchHighlights::for_theme(
                            &theme,
                            is_focused,
                            s.epoch,
                            &s.matches,
                            s.current_index,
                        )
                    });
                let search_ref = search_highlights.as_ref();

                if paint.cursor_suppressed {
                    self.terminal_cursor_restore_pending = true;
                }
                let show_cursor = paint.show_cursor;

                self.renderer.track_surface(*surface_id);
                terminal.with_view(&terminal_views.get(engine, *surface_id), |view| {
                    self.renderer.append_terminal_viewport(
                        &view,
                        &self.queue,
                        rect,
                        &ansi,
                        bg,
                        fg,
                        show_cursor,
                        sel_ref,
                        vi_cursor_ref,
                        render_preedit_ref,
                        link_for_this,
                        search_ref,
                        reverse_screen_enabled,
                    );
                });
            }
        }

        let changes = self.renderer.finish_change_frame();
        let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;
        let submit_start = std::time::Instant::now();

        // Single buffer upload (auto-grows if needed) + single encoder/submit.
        self.renderer.flush_buffers(&self.device, &self.queue);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terminal_pass"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer
                .render_all(&mut render_pass, self.size.width, self.size.height);
        }
        self.submit_commands(std::iter::once(encoder.finish()));
        TerminalPassCost {
            build_ms,
            submit_ms: submit_start.elapsed().as_secs_f64() * 1000.0,
            changes,
        }
    }

    pub(super) fn render_egui_pass(
        &mut self,
        view: &wgpu::TextureView,
        textures_delta: &egui::TexturesDelta,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
    ) {
        for (id, image_delta) in &textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let mut egui_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("egui_encoder"),
                });

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut egui_encoder,
            paint_jobs,
            screen_descriptor,
        );

        {
            let render_pass = egui_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, paint_jobs, screen_descriptor);
        }

        self.submit_commands(std::iter::once(egui_encoder.finish()));
    }

    /// 이번 프레임에 해제할 egui 텍스처를 푼다. 메인 pass 와 WebView 오버레이 pass 가 모두 끝난 뒤
    /// 부른다. 오버레이 pass 가 같은 텍스처를 쓰기 때문이다.
    pub(super) fn free_egui_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }

    /// 팝업 z_seq 판정에 따라 host UI와 plugin 콘텐츠 합성 순서를 정한다.
    /// plugin 셸은 content_rect를 비워 어느 순서든 자기 콘텐츠를 가리지 않는다.
    /// mesh 도 `screen_descriptor` 의 surface 크기로 그리고, `page_clip` 이 있으면 그 페이지 안만 칠한다.
    #[allow(clippy::too_many_arguments)] // reason: 두 pass 호출에 필요한 인자 그대로 전달
    pub(super) fn render_egui_pass_and_mesh_popups(
        &mut self,
        view: &wgpu::TextureView,
        textures_delta: &egui::TexturesDelta,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        mesh_popup_regions: &[(u64, PhysicalRect)],
        plugin_manager: Option<PluginDisplay<'_>>,
        host_popup_on_top: bool,
        page_clip: Option<&[egui::Rect]>,
    ) {
        let size_in_pixels = screen_descriptor.size_in_pixels;
        let render_mesh_popups = |this: &mut Self| {
            if let Some(mgr) = plugin_manager {
                this.render_egui_mesh_popups(
                    view,
                    mesh_popup_regions,
                    mgr,
                    size_in_pixels,
                    page_clip,
                );
            }
        };
        let render_egui = |this: &mut Self| {
            this.render_egui_pass(view, textures_delta, paint_jobs, screen_descriptor);
        };

        if host_popup_on_top {
            render_mesh_popups(self);
            render_egui(self);
        } else {
            render_egui(self);
            render_mesh_popups(self);
        }
    }
}
