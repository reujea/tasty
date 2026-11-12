//! Drives every window's native Wayland text clipboard (`GpuState::poll_text_selection`). Pastes
//! read from another app arrive between frames, so the window that asked is redrawn to take them.

use super::App;

impl App {
    pub(crate) fn poll_text_selections(&mut self) {
        for w in self.view.views.values_mut() {
            let base = w.base_mut();
            if base.gpu.poll_text_selection() {
                base.winit.request_redraw();
            }
        }
        // 셸 설정·부팅 오류 창은 views 밖에 있다.
        for (gpu, window) in [
            (&mut self.shell_setup_gpu, &self.shell_setup_window),
            (&mut self.boot_error_gpu, &self.boot_error_window),
        ] {
            if let (Some(gpu), Some(window)) = (gpu.as_mut(), window.as_ref())
                && gpu.poll_text_selection()
            {
                window.request_redraw();
            }
        }
    }

    /// A paste is being read or copied text is being sent, so the poll timer must keep running.
    pub(crate) fn text_selection_busy(&self) -> bool {
        self.view
            .views
            .values()
            .any(|w| w.base().gpu.text_selection_busy())
            || [&self.shell_setup_gpu, &self.boot_error_gpu]
                .into_iter()
                .flatten()
                .any(|gpu| gpu.text_selection_busy())
    }
}
