use winit::window::Window;

use crate::i18n::t;
use tasty_settings::ShellPathStatus;
use tasty_ui_widgets::{ShellSetupCheck, ShellSetupView, shell_setup_screen};

use super::{GpuState, ShellSetupAction};

/// 경로를 판정한다. 설정의 셸 판정([`ShellPathStatus`])과 같은 기준이라 런타임·부팅 판정·설정 창 경고와
/// 결론이 같다. 쓸 수 있는 셸만 확정할 수 있다.
fn shell_check(path: &str) -> ShellSetupCheck {
    match ShellPathStatus::of(path) {
        ShellPathStatus::Empty => ShellSetupCheck::Empty,
        ShellPathStatus::Missing => ShellSetupCheck::Missing,
        ShellPathStatus::NotShell => ShellSetupCheck::NotShell,
        ShellPathStatus::Directory => ShellSetupCheck::Directory,
        ShellPathStatus::NotExecutable => ShellSetupCheck::NotExecutable,
        ShellPathStatus::Usable => ShellSetupCheck::Valid,
    }
}

/// 판정 통과 문구. Windows는 무엇이 통과했는지 Git Bash로 말한다.
fn valid_line() -> &'static str {
    if cfg!(windows) {
        t("boot.shell_setup.check_valid_git_bash")
    } else {
        t("boot.shell_setup.check_valid")
    }
}

/// Windows 안내 줄의 키. 기본 위치 탐색 결과만 따르므로 입력 중 판정이 바뀌어도 그대로다.
fn git_bash_notice_key(detected: Option<String>) -> Option<&'static str> {
    cfg!(windows).then_some(if detected.is_some() {
        "boot.shell_setup.git_bash_required"
    } else {
        "boot.shell_setup.git_bash_missing"
    })
}

impl GpuState {
    /// 첫 실행 셸 설정 화면을 그린다. 터미널 없이 egui만 사용한다.
    pub fn render_shell_setup(
        &mut self,
        window: &Window,
        shell_path: &mut String,
    ) -> Result<ShellSetupAction, wgpu::SurfaceError> {
        let th = crate::theme::theme();
        let output = self.acquire_surface()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let raw_input = self.egui_state.take_egui_input(window);
        let mut action = ShellSetupAction::None;

        let title = t("boot.shell_setup.title");
        let subtitle = t("boot.shell_setup.subtitle");
        let git_bash_notice =
            git_bash_notice_key(tasty_settings::GeneralSettings::detect_bash()).map(t);
        let missing = t("boot.shell_setup.check_missing");
        let not_shell = t("boot.shell_setup.check_not_shell");
        let directory = t("boot.shell_setup.check_directory");
        let not_executable = t("boot.shell_setup.check_not_executable");
        let valid = valid_line();
        let quit = t("button.quit");
        let confirm = t("settings.terminal.shell_confirm");
        let placeholder = if cfg!(windows) {
            "C:/Program Files/Git/bin/bash.exe"
        } else {
            "/bin/zsh"
        };

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            tasty_egui_theme::apply_theme_to_egui(&th, ctx);
            let setup = ShellSetupView {
                title,
                subtitle,
                git_bash_notice,
                placeholder,
                missing,
                not_shell,
                directory,
                not_executable,
                valid,
                quit,
                confirm,
                check: shell_check(shell_path),
            };
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(th.bg_app().into()))
                .show(ctx, |ui| {
                    let out = shell_setup_screen(ui, &th, &setup, shell_path);
                    if out.confirm {
                        action = ShellSetupAction::Confirmed;
                    } else if out.quit {
                        action = ShellSetupAction::Exit;
                    }
                });
        });

        self.egui_state
            .handle_platform_output(window, full_output.platform_output);

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
                    label: Some("egui_update"),
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
                label: Some("shell_setup_encoder"),
            });
        {
            // 부팅 화면과 같이 화면 채움인 bg-app을 GPU clear 색으로도 쓴다.
            let gpu_bg = th.bg_app().to_gpu_rgba();
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shell_setup_pass"),
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

        Ok(action)
    }
}

#[cfg(test)]
mod notice_tests {
    use super::git_bash_notice_key;

    #[test]
    fn the_git_bash_notice_follows_the_default_location_search_on_windows_only() {
        let (missing, required) = if cfg!(windows) {
            (
                Some("boot.shell_setup.git_bash_missing"),
                Some("boot.shell_setup.git_bash_required"),
            )
        } else {
            (None, None)
        };
        assert_eq!(git_bash_notice_key(None), missing);
        assert_eq!(git_bash_notice_key(Some("bash".into())), required);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    /// 첫 실행 화면은 설정 판정과 같은 결론을 낸다. 허용하지 않는 값은 확정할 수 없다.
    /// 이름만 쓴 값은 PATH에서 찾으므로 찾으면 `NotShell`, 못 찾으면 `Missing`이다. 시험이 PATH를 정하지 않으므로
    /// `cmd.exe`는 설정 판정 결과와 대응하는지만 본다.
    #[test]
    fn the_setup_check_rejects_what_the_settings_judgement_rejects() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cmd = dir.path().join("cmd.exe");
        std::fs::write(&cmd, "").expect("write fixture");
        let absent = dir.path().join("absent").join("bash.exe");
        assert_eq!(shell_check(""), ShellSetupCheck::Empty);
        let bare = match ShellPathStatus::of("cmd.exe") {
            ShellPathStatus::Missing => ShellSetupCheck::Missing,
            ShellPathStatus::NotShell => ShellSetupCheck::NotShell,
            other => panic!("cmd.exe must be rejected: {other:?}"),
        };
        assert_eq!(shell_check("cmd.exe"), bare);
        assert_eq!(
            shell_check(&absent.to_string_lossy()),
            ShellSetupCheck::Missing
        );
        assert_eq!(
            shell_check(&cmd.to_string_lossy()),
            ShellSetupCheck::NotShell
        );
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    /// 첫 실행 화면은 설정 판정과 같은 결론을 낸다. macOS·Linux는 bash·zsh가 아닌 셸도 확정할 수 있다.
    #[test]
    fn the_setup_check_follows_the_settings_judgement() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let fish = dir.path().join("fish");
        std::fs::write(&fish, "#!/bin/sh\n").expect("write fixture");
        std::fs::set_permissions(&fish, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let plain = dir.path().join("bash");
        std::fs::write(&plain, "").expect("write fixture");
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
        assert_eq!(shell_check(""), ShellSetupCheck::Empty);
        assert_eq!(
            shell_check("/nonexistent/tasty-setup/zsh"),
            ShellSetupCheck::Missing
        );
        assert_eq!(shell_check(&s(&fish)), ShellSetupCheck::Valid);
        assert_eq!(shell_check(&s(dir.path())), ShellSetupCheck::Directory);
        assert_eq!(shell_check(&s(&plain)), ShellSetupCheck::NotExecutable);
    }
}
