//! 부팅·종료의 공용 로딩 화면. 상태별 번역 키를 받아 로고·스피너·진행 문구를 그린다.

use winit::window::Window;

use super::GpuState;
use crate::app::state::BootPhase;

/// WaitingEngine은 GpuInit과 같은 진행 문구를 사용한다.
pub fn boot_phase_text_key(phase: &BootPhase) -> &'static str {
    match phase {
        BootPhase::GpuInit | BootPhase::WaitingEngine { .. } => "boot.phase_gpu_init",
        BootPhase::WaitingPlugins { .. } => "boot.phase_waiting_plugins",
        BootPhase::WaitingJournal | BootPhase::RestoringLayout { .. } => {
            "boot.phase_restoring_layout"
        }
    }
}

impl GpuState {
    /// 번역 키에 해당하는 문구로 로딩 프레임을 그린다.
    pub fn render_loading(
        &mut self,
        window: &Window,
        phase_text_key: &str,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.acquire_surface()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let th = crate::theme::theme();
        let bg = th.bg_app();

        let raw_input = self.take_egui_input(window);
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            tasty_egui_theme::apply_theme_to_egui(&th, ctx);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(bg.into()))
                .show(ctx, |ui| {
                    let content_height = th.loading_screen_wordmark_icon_size().value()
                        + th.spacing_xl.value()
                        + th.loading_screen_spinner_size().value()
                        + th.spacing_lg.value()
                        + th.loading_screen_phase_slot_height().value();
                    let top_pad = ((ui.available_height() - content_height) / 2.0).max(0.0);
                    ui.add_space(top_pad);
                    ui.vertical_centered(|ui| {
                        crate::adapters::ui::brand::draw_wordmark(
                            ui,
                            &th,
                            th.loading_screen_wordmark_icon_size(),
                            th.loading_screen_wordmark_font_size(),
                            th.loading_lockup_tracking(),
                        );
                        ui.add_space(th.spacing_xl.value());
                        tasty_ui_widgets::Spinner::new()
                            .size(th.loading_screen_spinner_size().value())
                            .color(th.accent_primary().to_egui())
                            .show(ui, &th);
                        ui.add_space(th.spacing_lg.value());
                        let (slot_rect, _) = ui.allocate_exact_size(
                            egui::vec2(
                                ui.available_width(),
                                th.loading_screen_phase_slot_height().value(),
                            ),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            slot_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            crate::i18n::t(phase_text_key),
                            egui::FontId::proportional(th.font_size_body.value()),
                            th.text_muted().to_egui(),
                        );
                    });
                });
        });
        self.apply_egui_output(window, full_output.platform_output);

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: self.scale_factor,
        };
        let tris = self
            .egui_ctx
            .tessellate(full_output.shapes, self.scale_factor);
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }

        let mut update_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("loading_update"),
                });
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut update_encoder,
            &tris,
            &screen_descriptor,
        );
        self.submit_commands(std::iter::once(update_encoder.finish()));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("loading_encoder"),
            });
        {
            let gpu_bg = bg.to_gpu_rgba();
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loading_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: gpu_bg.r() as f64,
                            g: gpu_bg.g() as f64,
                            b: gpu_bg.b() as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer
                .render(&mut render_pass, &tris, &screen_descriptor);
        }
        self.submit_commands(std::iter::once(encoder.finish()));
        self.present_surface(output);

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}
