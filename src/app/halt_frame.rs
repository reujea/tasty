//! 실행 중 journal이 halt된 뒤의 최소 프레임. halt 중에는 일반 프레임과 입력을 처리하지 않으므로,
//! 오른쪽 위 비포커스 안내만 그리고 닫을 수 있게 둔다.
//!
//! 그리는 프레임은 엔진을 읽기만 한다. 이 프레임이 만든 요청은 적용하지 않고 버린다. 포인터는
//! 안내 상자와 타이틀바 닫기 버튼 위에 있을 때만 egui에 넘기므로 다른 UI는 클릭을 받지 않는다.
//! 키 입력은 넘기지 않아 포커스는 halt 전 위치에 그대로 남는다.
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::App;

impl App {
    /// halt 알림을 알림 저장소와 안내 열로 옮기고, 새 안내가 생긴 창을 다시 그리게 한다.
    pub(crate) fn surface_halt_notice(&mut self) {
        self.deliver_recovery_notices();
        self.collect_restore_notice_acknowledgements();
        for view in self.view.views.values_mut() {
            if let Some(main) = view.as_main_mut()
                && crate::adapters::ui::restore_notice::has_undrawn(&main.state)
            {
                crate::view::ui::View::mark_dirty(main);
                main.base.winit.request_redraw();
            }
        }
    }

    /// halt 중 egui의 즉시 repaint 요청. 안내 Area는 첫 프레임에 크기만 재고 다음 프레임에 보이므로
    /// 이 요청을 버리면 안내가 그려지지 않는다. 타이머가 돌지 않으므로 지연 요청은 받지 않는다.
    pub(crate) fn request_halted_repaint(&mut self, id: WindowId) {
        if let Some(main) = self
            .view
            .views
            .get_mut(&id)
            .and_then(|view| view.as_main_mut())
        {
            main.base.winit.request_redraw();
        }
    }

    /// halt 중 이벤트 루프를 깨울 가장 이른 시각. engine 해제·runner 정지 확인과 native 페이지 그림 반영을 합친다.
    pub(crate) fn halted_wake_deadline(&mut self) -> Option<std::time::Instant> {
        use crate::app::timers::min_deadline;
        let paint = min_deadline(
            self.halted_webview_paint_deadline(),
            self.plugin_file_poll_deadline(),
        );
        min_deadline(
            min_deadline(
                self.engine_release_poll_deadline(),
                self.runner_stop_poll_deadline(),
            ),
            paint,
        )
    }

    /// halt 중에도 보이는 native 페이지의 그림이 반영되도록 일반 webview 폴링과 같은 간격으로 GTK
    /// 대기열만 비운다. 먼저 모든 페이지의 입력을 막으므로 키·포인터는 페이지와 host 어느 쪽에도
    /// 전달되지 않는다. 키·포커스 전달, 탐색 결정 반영, 설정 적용은 하지 않는다. 다음 마감을 돌려준다.
    pub(crate) fn halted_webview_paint_deadline(&mut self) -> Option<std::time::Instant> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let mut visible = false;
        for view in self.view.views.values() {
            let Some(main) = view.as_main() else {
                continue;
            };
            for wv in main.webviews.values() {
                wv.hold_input_for_halt();
            }
            visible |= main.webview_any_visible;
        }
        if !visible {
            return None;
        }
        #[cfg(target_os = "linux")]
        crate::system_tray::pump_gtk_events();
        Some(std::time::Instant::now() + crate::app::timers::WEBVIEW_KEY_POLL_INTERVAL)
    }

    /// halt 중 창 닫기. 종료 확인 창은 halt 중 입력을 받지 못하므로 마지막 창이면 확인 없이 종료한다.
    /// 창이 여럿이면 그 창만 닫는다.
    pub(crate) fn request_halted_close(&mut self, id: WindowId, event_loop: &ActiveEventLoop) {
        if self.state.preset_view_id == Some(id) || self.main_window_count() > 1 {
            self.request_close_window(id, event_loop);
        } else {
            tracing::info!(
                "window closed while the journal is halted; quitting without confirmation"
            );
            self.begin_shutdown(event_loop);
        }
    }

    /// halt 중 창 이벤트. `CloseRequested`는 호출자가 일반 경로로 보낸다.
    pub(crate) fn handle_halted_window_event(&mut self, id: WindowId, event: WindowEvent) {
        // Linux 설정창은 OS 닫기 버튼 대신 앱 제목 줄의 닫기를 쓴다. halt 중에도 그 버튼만 받아 OS 닫기와 같이 닫는다.
        #[cfg(target_os = "linux")]
        if let Some(settings) = self.view.views.get_mut(&id).and_then(|view| {
            view.as_any_mut()
                .downcast_mut::<crate::view::SettingsView>()
        }) {
            if settings.halted_close_press(&event) {
                self.close_active_modal();
            }
            return;
        }
        let Some(main) = self
            .view
            .views
            .get_mut(&id)
            .and_then(|view| view.as_main_mut())
        else {
            return;
        };
        let repaint = match &event {
            WindowEvent::RedrawRequested => {
                let Some(engine) = self.engines.window_read(id) else {
                    return;
                };
                let shown = crate::adapters::ui::restore_notice::count(&main.state);
                main.base.state.dirty = true;
                main.render_if_dirty(&engine, None);
                // 이번 프레임의 안내 카드에 맞춰 native 페이지의 그림·입력 구멍만 다시 맞춘다.
                if let Some(engine) = self.engines.window_engine(id) {
                    crate::app::webview_sync::sync_halted_clipping(main, &engine);
                }
                let dropped = main.state.take_pending_intents();
                if !dropped.is_empty() {
                    tracing::debug!(
                        count = dropped.len(),
                        "halted frame: dropped requests from the frame"
                    );
                }
                // 이번 프레임에서 닫힌 안내는 다음 프레임에서 지워진다.
                crate::adapters::ui::restore_notice::count(&main.state) != shown
            }
            WindowEvent::CursorMoved { position, .. } => {
                let ppp = main.base.gpu.egui_pixels_per_point();
                let pos = egui::pos2(
                    crate::model::PhysicalPx(position.x as f32)
                        .to_logical(ppp)
                        .value(),
                    crate::model::PhysicalPx(position.y as f32)
                        .to_logical(ppp)
                        .value(),
                );
                let was = main.state.halt_pointer_inside;
                let inside = main.state.popups.quiet_popup_covers(pos)
                    || main
                        .state
                        .titlebar_close_rect
                        .is_some_and(|rect| rect.contains(pos));
                main.state.halt_pointer_inside = inside;
                match (was, inside) {
                    (_, true) => main.base.gpu.handle_egui_event(&main.base.winit, &event).1,
                    // 영역을 벗어나면 hover가 남지 않도록 포인터가 사라졌다고 알린다.
                    (true, false) => {
                        main.base.gpu.push_egui_pointer_gone();
                        true
                    }
                    (false, false) => false,
                }
            }
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. }
                if main.state.halt_pointer_inside =>
            {
                main.base.gpu.handle_egui_event(&main.base.winit, &event).1
            }
            WindowEvent::CursorLeft { .. } => {
                main.base.gpu.handle_egui_event(&main.base.winit, &event).1
            }
            _ => false,
        };
        if repaint {
            main.base.winit.request_redraw();
        }
    }
}
