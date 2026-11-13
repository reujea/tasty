//! close_behavior가 ask일 때 종료 여부를 확인한다.

use tasty_type_geometry::length::LogicalPx;
use winit::event_loop::ActiveEventLoop;

use crate::AppEvent;
use crate::app::App;

/// OS 종료 확인 창의 크기. 갤러리 사본과의 일치를 gallery_copied_dimensions가 검사한다.
/// 테마 내부 간격이 아니므로 디자인 토큰으로 치환하지 않는다.
const WINDOW_W: LogicalPx = LogicalPx(400.0);
const WINDOW_H: LogicalPx = LogicalPx(200.0);

impl App {
    pub(crate) fn handle_quit_requested(&mut self, event_loop: &ActiveEventLoop) {
        let quit_modal_open = self
            .view
            .active_modal_id()
            .and_then(|id| self.view.views.get(&id))
            .map(|m| m.as_any().downcast_ref::<crate::view::QuitView>().is_some())
            .unwrap_or(false);
        if quit_modal_open {
            self.close_active_modal();
            self.begin_shutdown(event_loop);
            return;
        }

        let behavior = self
            .engines()
            .all()
            .next()
            .map(|e| e.runtime.settings.general.close_behavior.clone())
            .unwrap_or_else(|| "ask".to_string());

        match behavior.as_str() {
            "quit" => {
                self.begin_shutdown(event_loop);
            }
            "minimize" => {
                crate::shortcuts::send_app_event(&self.view.proxy, AppEvent::Minimize);
            }
            _ => {
                self.close_active_modal();
                self.open_quit_modal(event_loop);
            }
        }
    }

    /// 확인 창을 만들 수 없으면 이미 요청한 종료를 계속한다.
    /// toast가 종료 화면에 가릴 수 있어 생략 사실을 오류 로그에도 남긴다.
    fn quit_without_confirmation(
        &mut self,
        context: &str,
        err: impl std::fmt::Display,
        event_loop: &ActiveEventLoop,
    ) {
        tracing::error!("{context}: {err} — quitting without the confirmation step");
        if let Some(view) = self.notice_window_mut() {
            view.state.toasts.push(
                crate::i18n::t("window_error.quit_confirm.skipped"),
                crate::adapters::ui::ToastKind::Error,
                crate::adapters::ui::ToastScope::Window,
            );
        }
        self.begin_shutdown(event_loop);
    }

    pub(crate) fn open_quit_modal(&mut self, event_loop: &ActiveEventLoop) {
        use winit::window::WindowAttributes;

        let mut attrs = WindowAttributes::default()
            .with_title("Tasty")
            .with_inner_size(winit::dpi::LogicalSize::new(
                WINDOW_W.value(),
                WINDOW_H.value(),
            ))
            .with_resizable(false)
            .with_visible(false);
        if let Some(icon) = crate::app_icon::winit_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        // 확인 창 실패 때문에 종료 자체를 막지 않는다.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => std::sync::Arc::new(w),
            Err(e) => {
                self.quit_without_confirmation("failed to create quit modal window", e, event_loop);
                return;
            }
        };

        let gpu = match self.create_gpu_state(
            window.clone(),
            &crate::settings::Settings::load().appearance,
        ) {
            Ok(g) => g,
            Err(e) => {
                self.quit_without_confirmation(
                    "failed to initialize GPU for quit modal",
                    e,
                    event_loop,
                );
                return;
            }
        };

        let window_id = window.id();
        let mut modal = {
            let clipboard = self.window_clipboard(&window);
            crate::view::QuitView::new(gpu, window, clipboard)
        };
        crate::view::ui::present_first_frame(&mut modal);
        self.open_modal(Box::new(modal), window_id, crate::view::ModalKind::Quit);
    }
}
