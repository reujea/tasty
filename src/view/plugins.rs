//! 플러그인 관리 모달. 읽기 전용 스냅샷을 표시하고 사용자 조작은 큐에 넣는다.
//! App이 조작을 PluginManager에 반영한 뒤 스냅샷을 갱신한다.

pub mod ui;

use std::sync::Arc;

use winit::event::WindowEvent;

use crate::adapters::ui::{LayoutContext, ToastManager};
use crate::gpu::GpuState;
use crate::plugins_ui::{self, PluginsAction, PluginsSnapshot, PluginsUiState};
use crate::view::ui::{View, sealed};
use crate::view::{ModalView, ViewAction, ViewBase, ViewCtx};

pub struct PluginsView {
    pub base: ViewBase,
    pub snapshot: PluginsSnapshot,
    pub pending_actions: Vec<PluginsAction>,
    ui_state: PluginsUiState,
    shown: bool,
    should_close: bool,
    pub(crate) toasts: ToastManager,
    /// This window's clipboard: egui widget copies go through the copy door with it.
    clipboard: crate::clipboard::terminal::TerminalClipboard,
}

impl PluginsView {
    pub fn new(
        gpu: GpuState,
        winit: Arc<winit::window::Window>,
        clipboard: crate::clipboard::terminal::TerminalClipboard,
        snapshot: PluginsSnapshot,
    ) -> Self {
        Self {
            base: ViewBase::new(gpu, winit),
            clipboard,
            snapshot,
            pending_actions: Vec::new(),
            ui_state: PluginsUiState::default(),
            shown: false,
            should_close: false,
            toasts: ToastManager::new(),
        }
    }

    /// 메인 루프가 actions를 적용한 뒤 호출하여 화면을 새 데이터로 갱신.
    pub fn refresh_snapshot(&mut self, snapshot: PluginsSnapshot) {
        self.snapshot = snapshot;
        self.mark_dirty();
    }

    /// App이 Add 탭의 추가 요청을 처리했다. 성공하면 입력 화면으로 돌아가고, 실패하면 확인한
    /// 판정과 버튼을 그대로 둔다. 실패 원인은 함께 띄우는 토스트가 알린다.
    pub fn finish_add(&mut self, installed: bool) {
        self.ui_state.finish_add(installed);
        self.mark_dirty();
    }

    /// App이 예약 id 폴더 제거를 끝냈다. 바를 평소로 되돌린다. 행은 스냅샷이 정한다.
    pub fn finish_remove_reserved(&mut self) {
        self.ui_state.finish_remove_reserved();
        self.mark_dirty();
    }

    /// 모달 윈도우 영역에 toast를 띄운다. `Add` 탭에서 설치 성공/실패 알림 등에 사용.
    pub fn push_toast(&mut self, message: impl Into<String>, kind: crate::adapters::ui::ToastKind) {
        self.toasts
            .push(message, kind, crate::adapters::ui::ToastScope::Window);
        self.mark_dirty();
    }
}

impl View for PluginsView {
    fn poll_local_reads(&mut self, owner: &mut crate::app::local_reads::ReadRequests) {
        if self.ui_state.folder_chooser.poll(owner) {
            self.mark_dirty();
        }
    }

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
            WindowEvent::CloseRequested => {
                self.should_close = true;
                return ViewAction::Close;
            }
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.base.state.modifiers = modifiers.state();
            }
            _ => {}
        }

        if self.should_close {
            ViewAction::Close
        } else {
            ViewAction::None
        }
    }

    fn render(&mut self) {
        if !self.base.state.dirty {
            return;
        }
        self.base.begin_frame();

        let raw_input = self.base.gpu.take_egui_input(&self.base.winit);
        let snapshot = &self.snapshot;
        let ui_state = &mut self.ui_state;
        let actions = &mut self.pending_actions;
        let toasts = &mut self.toasts;

        let full_output = self.base.gpu.run_egui(raw_input, |ctx| {
            plugins_ui::draw_plugins_panel(ctx, snapshot, ui_state, actions);

            let empty_layout = LayoutContext {
                active_workspace: 0,
                active_workspace_id: 0,
                pane_rects: Vec::new(),
                surface_rects: Vec::new(),
                active_tabs: Vec::new(),
                active_tab_ids: Vec::new(),
                workspace_rect: None,
            };
            toasts.draw(ctx, &empty_layout, false);
        });

        self.base
            .gpu
            .finish_egui_frame(&self.base.winit, full_output);
        if crate::clipboard::copy::copy_egui_text(
            &mut self.base.gpu,
            &mut self.clipboard,
            &mut self.toasts,
        ) {
            self.mark_dirty();
        }

        self.reveal_after_first_render();

        if !self.pending_actions.is_empty() {
            // actions를 메인 루프에서 처리하도록 윈도우를 다시 깨움.
            self.base.winit.request_redraw();
        }

        if self.base.state.dirty {
            self.base.winit.request_redraw();
        }
    }
}

impl ModalView for PluginsView {
    fn shown(&self) -> bool {
        self.shown
    }
    fn set_shown(&mut self, v: bool) {
        self.shown = v;
    }
}

impl sealed::Sealed for PluginsView {}
