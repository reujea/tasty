//! Window observations shared by the event callbacks.
use super::*;

impl App {
    pub(super) fn apply_window_focus_observation(&mut self, id: WindowId, event: &WindowEvent) {
        // 첫 Focused 이벤트를 창 map의 신호로 사용해 에이전트 창의 초기 포커스 힌트를 지운다.
        if let WindowEvent::Focused(_) = event
            && self.pending_focus_hint_clear.remove(&id)
            && let Some(view) = self.view.views.get(&id)
            && let Err(e) =
                crate::platform::window_stacking::clear_initial_focus_hint(&view.base().winit)
        {
            tracing::warn!("agent window: clearing the initial focus hint failed: {e}");
        }

        // 포커스 사건의 창 상태 몫은 도착 때 적용한다. 입력 순서 몫(수식키·키 추적 초기화)은 창 입력으로 가서 보류
        // 뒤에 줄을 선다(`held_order`).
        if let WindowEvent::Focused(focused) = *event
            && let Some(main) = self.view.views.get_mut(&id).and_then(|v| v.as_main_mut())
        {
            main.note_window_focus(focused);
        }
        if let WindowEvent::Focused(true) = event {
            self.handle_window_focused(id);
        }
    }

    /// 부팅 첫 창의 속성. hidden으로 만들고 첫 렌더를 시도한 뒤 표시한다.
    pub(crate) fn boot_window_attributes() -> winit::window::WindowAttributes {
        let mut attrs = winit::window::WindowAttributes::default()
            .with_visible(false)
            .with_title(if cfg!(debug_assertions) {
                "Tasty (Debug)"
            } else {
                "Tasty"
            })
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .with_min_inner_size(winit::dpi::LogicalSize::new(640, 480));
        if let Some(icon) = crate::app_icon::winit_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }
        crate::platform::window_chrome::apply_csd_attributes(attrs)
    }
    /// hidden 상태의 첫 창을 만든다. 로딩·shell setup 경로가 렌더를 시도한 뒤 표시한다.
    pub(super) fn boot_create_hidden_window(
        event_loop: &ActiveEventLoop,
        boot_t0: std::time::Instant,
    ) -> std::sync::Arc<winit::window::Window> {
        let mut attrs =
            crate::app::window_lifecycle::main_view_class(Self::boot_window_attributes());
        // 이 실행의 실행기 토큰은 첫 창이 받아 실행기의 대기 표시를 끝낸다.
        let evidence = crate::boot::single_instance::evidence::take();
        if evidence.is_present() {
            attrs = crate::boot::single_instance::with_activation_token(
                attrs,
                &evidence,
                crate::boot::single_instance::is_wayland(event_loop),
            );
        }
        // 표시할 창이 없으면 오류를 로그로 남기고 실패 코드로 종료한다.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => std::sync::Arc::new(w),
            Err(e) => {
                tracing::error!(
                    "boot window creation failed: {e}\n{}\n{}\n{}",
                    crate::i18n::t("boot.window_error.title"),
                    crate::i18n::t_fmt("boot.window_error.body", &e.to_string()),
                    crate::i18n::t("boot.window_error.hint"),
                );
                std::process::exit(1);
            }
        };
        tracing::info!(
            target: "tasty::boot",
            ms = boot_t0.elapsed().as_secs_f64() * 1000.0,
            "T1 window_create (resumed enter -> create_window return)"
        );
        window
    }
}

/// Watchdog-thread probe: cached queue obligations only, no App/store/native access.
pub(super) fn pending_ipc_work() -> Option<crate::stall_watchdog::PendingWork> {
    tasty_ipc::progress::pending_main_loop_work().map(|p| crate::stall_watchdog::PendingWork {
        count: p.count,
        since: p.since,
        generation: p.generation,
    })
}

#[cfg(debug_assertions)]
pub(super) fn record_wake(cause: winit::event::StartCause, shutting_down: bool) {
    if let winit::event::StartCause::ResumeTimeReached {
        requested_resume, ..
    } = cause
    {
        super::super::loop_measurement::wake(requested_resume, shutting_down);
    }
}
