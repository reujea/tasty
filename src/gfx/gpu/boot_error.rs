//! 엔진 부팅 실패 진단을 창에 표시한다. GPU나 창 생성 자체가 실패한 경우에는 사용할 수 없다.

use winit::window::Window;

use super::{BootErrorInfo, GpuState};

impl GpuState {
    /// 부팅 실패 진단을 그린다. 반환 `Ok(true)` = 사용자가 종료를 눌렀다(caller 가
    /// `exit` 한다). 터미널 없이 egui 만 그린다 — `render_shell_setup` 과 같은 구조다.
    /// 화면은 갤러리와 같은 `tasty_ui_widgets::boot_error_screen`이다.
    pub fn render_boot_error(
        &mut self,
        window: &Window,
        info: &BootErrorInfo,
    ) -> Result<bool, wgpu::SurfaceError> {
        let output = self.acquire_surface()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let raw_input = self.take_egui_input(window);
        let mut quit = false;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let th = crate::theme::theme();
            tasty_egui_theme::apply_theme_to_egui(&th, ctx);

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let screen = tasty_ui_widgets::BootErrorView {
                        title: &info.title,
                        body: &info.body,
                        hint: &info.hint,
                        quit: crate::i18n::t("button.quit"),
                    };
                    if tasty_ui_widgets::boot_error_screen(ui, &th, &screen)
                        || ui.input(|i| {
                            i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)
                        })
                    {
                        quit = true;
                    }
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
                    label: Some("boot_error_update"),
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
                label: Some("boot_error_encoder"),
            });
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("boot_error_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.12,
                            b: 0.14,
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

        Ok(quit)
    }
}
