use std::sync::Arc;

use winit::event::WindowEvent;

use crate::AppEvent;
use crate::gpu::GpuState;
use crate::i18n::t;
use crate::view::ui::{View, sealed};
use crate::view::{ModalView, ViewAction, ViewBase, ViewCtx};
use tasty_ui_widgets::{hspace, vspace};

/// 종료 확인 다이얼로그. 사용자에게 종료/최소화를 묻는다.
pub struct QuitView {
    pub base: ViewBase,
    shown: bool,
    pending_action: ViewAction,
    /// This window's clipboard. The dialog has no toast stack, so a copy from its selectable
    /// labels is written through the copy door without a toast.
    clipboard: crate::clipboard::terminal::TerminalClipboard,
}

impl QuitView {
    pub fn new(
        gpu: GpuState,
        winit: Arc<winit::window::Window>,
        clipboard: crate::clipboard::terminal::TerminalClipboard,
    ) -> Self {
        Self {
            base: ViewBase::new(gpu, winit),
            shown: false,
            pending_action: ViewAction::None,
            clipboard,
        }
    }
}

impl View for QuitView {
    fn base(&self) -> &ViewBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ViewBase {
        &mut self.base
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_event(&mut self, event: WindowEvent, _ctx: &mut ViewCtx<'_>) -> ViewAction {
        // RedrawRequested 를 egui 에 넘기면 항상 repaint=true → mark_dirty →
        // request_redraw → RedrawRequested 무한 루프(busy-loop)가 된다. 렌더는
        // 아래 RedrawRequested arm 이 담당하므로 egui input 으로 넘기지 않는다.
        if !matches!(&event, WindowEvent::RedrawRequested) {
            let (_, egui_repaint) = self.base.gpu.handle_egui_event(&self.base.winit, &event);
            if egui_repaint {
                self.mark_dirty();
            }
        }

        match event {
            WindowEvent::CloseRequested => return ViewAction::Close,
            WindowEvent::Resized(new_size) => {
                self.base.gpu.resize(new_size);
                self.mark_dirty();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::CursorMoved { .. } => {
                self.mark_dirty();
            }
            WindowEvent::KeyboardInput { ref event, .. } => {
                use winit::event::ElementState;
                use winit::keyboard::{Key, NamedKey};
                if event.state == ElementState::Pressed
                    && let Key::Named(NamedKey::Escape) = &event.logical_key
                {
                    return self.on_escape();
                }
            }
            _ => {}
        }

        match &self.pending_action {
            ViewAction::None => ViewAction::None,
            _ => std::mem::replace(&mut self.pending_action, ViewAction::None),
        }
    }

    fn render(&mut self) {
        if !self.base.state.dirty {
            return;
        }
        self.base.begin_frame();

        let raw_input = self.base.gpu.take_egui_input(&self.base.winit);
        let pending = &mut self.pending_action;
        let th = crate::theme::theme();

        let full_output = self.base.gpu.run_egui(raw_input, |ctx| {
            egui::TopBottomPanel::bottom("quit_buttons")
                .exact_height(th.item_height_interactive.value() + th.spacing_md.value() * 2.0)
                .show(ctx, |ui| {
                    vspace(ui, th.spacing_md);
                    let available_width = ui.available_width() - 32.0;
                    let button_width = available_width / 2.0 - 4.0;
                    ui.horizontal(|ui| {
                        hspace(ui, th.spacing_lg);
                        if ui
                            .add_sized(
                                [button_width, th.item_height_interactive.value()],
                                egui::Button::new(t("quit_modal.quit_button")),
                            )
                            .clicked()
                        {
                            *pending = ViewAction::CloseWithEvent(AppEvent::Shutdown);
                        }
                        hspace(ui, th.spacing_sm);
                        if ui
                            .add_sized(
                                [button_width, th.item_height_interactive.value()],
                                egui::Button::new(t("quit_modal.minimize_button")),
                            )
                            .clicked()
                        {
                            *pending = ViewAction::CloseWithEvent(AppEvent::Minimize);
                        }
                    });
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    vspace(ui, th.spacing_xl);
                    ui.heading(t("quit_modal.title"));
                    vspace(ui, th.spacing_md);
                    ui.label(t("quit_modal.message"));
                    vspace(ui, th.spacing_sm);
                    ui.label(
                        egui::RichText::new(t("quit_modal.settings_hint"))
                            .small()
                            .weak(),
                    );
                });
            });
        });

        self.base
            .gpu
            .finish_egui_frame(&self.base.winit, full_output);
        if let Some(text) = self.base.gpu.take_egui_copy() {
            crate::clipboard::copy::copy_text_unannounced(&mut self.clipboard, &text);
        }

        self.reveal_after_first_render();

        if self.base.state.dirty {
            self.base.winit.request_redraw();
        }
    }
}

impl ModalView for QuitView {
    fn shown(&self) -> bool {
        self.shown
    }
    fn set_shown(&mut self, v: bool) {
        self.shown = v;
    }
}

impl sealed::Sealed for QuitView {}
