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

    /// Writes what the shell setup or boot error screen copied through the copy door. These screens
    /// have no toast stack, so a failure is only logged. The clipboard is made on the first copy and
    /// dropped when a screen is entered (`enter_shell_setup_mode`, `enter_boot_error_mode`), so it
    /// always belongs to the window shown now.
    pub(crate) fn copy_boot_screen_text(
        clipboard: &mut Option<crate::clipboard::terminal::TerminalClipboard>,
        display_owner: &Option<winit::event_loop::OwnedDisplayHandle>,
        gpu: &mut crate::gpu::GpuState,
        window: &winit::window::Window,
    ) {
        let Some(text) = gpu.take_egui_copy() else {
            return;
        };
        let clipboard = clipboard.get_or_insert_with(|| {
            crate::clipboard::terminal::TerminalClipboard::new(window, display_owner.clone())
        });
        crate::clipboard::copy::copy_text_unannounced(clipboard, &text);
    }

    /// Answers OSC 52 reads whose clipboard text arrived, in the engine that holds the terminal.
    /// A terminal that is gone gets nothing.
    pub(crate) fn poll_clipboard_reads(&mut self) {
        if self.services.pending_clipboard_reads.is_empty() {
            return;
        }
        for (sid, generation, result) in self.services.take_finished_clipboard_reads() {
            let mut result = Some(result);
            let scan = crate::app::window_access::engines_mut!(self);
            for (_, _, mut engine) in scan.window_entries() {
                if engine.find_terminal_by_id(sid).is_some()
                    && let Some(result) = result.take()
                {
                    crate::app::services::AppServices::reply_clipboard_query(
                        &mut engine,
                        sid,
                        generation,
                        result,
                    );
                }
            }
            for (_, _, mut engine) in
                crate::app::window_access::engines_mut!(self).parked_sessions_with_ids()
            {
                if engine.find_terminal_by_id(sid).is_some()
                    && let Some(result) = result.take()
                {
                    crate::app::services::AppServices::reply_clipboard_query(
                        &mut engine,
                        sid,
                        generation,
                        result,
                    );
                }
            }
        }
    }

    /// A paste is being read or copied text is being sent, so the poll timer must keep running.
    pub(crate) fn text_selection_busy(&self) -> bool {
        !self.services.pending_clipboard_reads.is_empty()
            || self
                .view
                .views
                .values()
                .any(|w| w.base().gpu.text_selection_busy())
            || [&self.shell_setup_gpu, &self.boot_error_gpu]
                .into_iter()
                .flatten()
                .any(|gpu| gpu.text_selection_busy())
    }
}
