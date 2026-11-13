//! 같은 데이터 홈을 다른 Tasty가 쓰고 있을 때의 GUI 부팅.
//!
//! 부팅 첫머리에서 저널 writer 잠금을 선점하지 못하면 이 경로로 온다. 실행 중인 인스턴스의 홈에
//! 부작용을 내지 않도록 로그 파일·memory.db·저널·설정 저장·플러그인·테마·IPC·트레이를 만들지 않고,
//! 창과 GPU만 만들어 기존 부팅 오류 화면을 보여 준다. 종료는 실패 코드 1이다.
//! App을 만들지 못했을 때도 같은 화면으로 실행을 막는다.
//!
//! 화면의 복사는 다른 창처럼 공통 복사 함수로 쓰고 토스트로 결과를 보인다. App 루프가 없으므로
//! egui repaint 요청과 네이티브 Wayland 선택의 큐는 이 루프가 직접 처리한다.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::AppEvent;
use crate::gpu::{BootErrorInfo, GpuState};

/// 오류 화면에서 종료할 때의 종료 코드. 다른 부팅 오류 화면과 같다.
const EXIT_CODE: u8 = 1;

/// "이미 사용 중인 홈" 문구. body에 사용 중인 홈의 실제 경로를 넣고, 같은 내용을 로그에도 남긴다.
pub(crate) fn error_info(home: &Path) -> BootErrorInfo {
    let title = crate::i18n::t("boot.home_in_use.title").to_string();
    let body = crate::i18n::t_fmt("boot.home_in_use.body", &home.display().to_string());
    let hint = crate::i18n::t("boot.home_in_use.hint").to_string();
    tracing::error!("boot refused: data home in use\n{title}\n{body}\n{hint}");
    BootErrorInfo { title, body, hint }
}

/// 오류 화면만 띄우는 이벤트 루프를 돌리고 사용자가 닫으면 종료 코드 1을 돌려준다.
/// 창이나 GPU를 만들지 못하면 OS 메시지 상자로 같은 문구를 보이고 같은 코드로 끝낸다.
pub(crate) fn run(home: &Path) -> anyhow::Result<std::process::ExitCode> {
    crate::boot::locale::init();
    run_screen(error_info(home))
}

/// 주어진 문구로 오류 화면만 띄운다. 호출자가 문구를 로그에 남긴다.
pub(crate) fn run_screen(info: BootErrorInfo) -> anyhow::Result<std::process::ExitCode> {
    let (event_loop, proxy) = crate::boot::event_loop::build()?;
    run_screen_on(event_loop, proxy, info)
}

/// 이미 만든 이벤트 루프로 오류 화면을 띄운다. 이벤트 루프는 프로세스당 하나만 만들 수 있으므로
/// 루프를 만든 뒤의 부팅 실패는 이 함수를 쓴다. 창이나 GPU를 만들지 못하면 OS 메시지 상자로 알린다.
pub(crate) fn run_screen_on(
    event_loop: winit::event_loop::EventLoop<AppEvent>,
    proxy: EventLoopProxy<AppEvent>,
    info: BootErrorInfo,
) -> anyhow::Result<std::process::ExitCode> {
    let mut screen = HomeInUseScreen {
        info,
        proxy,
        instance: Arc::new(crate::app::new_gpu_instance()),
        window: None,
        gpu: None,
        clipboard: None,
        toasts: crate::adapters::ui::ToastManager::new(),
        repaint_at: None,
    };
    event_loop.run_app(&mut screen)?;
    Ok(std::process::ExitCode::from(EXIT_CODE))
}

struct HomeInUseScreen {
    info: BootErrorInfo,
    proxy: EventLoopProxy<AppEvent>,
    instance: Arc<wgpu::Instance>,
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    /// Made on the first copy, with the event loop's display so a native Wayland start writes the
    /// system selection.
    clipboard: Option<crate::clipboard::terminal::TerminalClipboard>,
    toasts: crate::adapters::ui::ToastManager,
    /// A delayed egui repaint (a toast fading out) not yet due.
    repaint_at: Option<Instant>,
}

impl HomeInUseScreen {
    /// 창으로 보일 수 없을 때 같은 문구를 OS 메시지 상자(Linux는 zenity 또는 데스크톱 알림)로 보인다.
    fn show_without_window(&self) {
        let body = format!("{}\n\n{}", self.info.body, self.info.hint);
        let shown = crate::boot::single_instance::second::show_notice(&self.info.title, &body);
        tracing::error!("boot error shown without a window: {shown}");
    }

    fn create_gpu(&mut self, window: &Arc<Window>) -> anyhow::Result<GpuState> {
        // 설정 파일은 읽기만 한다. 정규화 결과를 저장하는 일반 부팅과 달리 쓰지 않는다.
        // 읽지 못하면 기본값이라 토스트 수명도 기본 수명이다.
        let settings = crate::settings::Settings::load();
        self.toasts
            .set_lifetime_ms(settings.overlay.toast_duration_ms);
        let theme_runtime = tasty_themes::ThemeRuntime {
            ui_zoom: settings.appearance.ui_scale_factor(),
            ..settings.theme_runtime()
        };
        pollster::block_on(async {
            let adapter = Arc::new(crate::app::select_gpu_adapter(&self.instance, window).await?);
            GpuState::new_shared(
                &self.instance,
                &adapter,
                window.clone(),
                &settings.appearance,
                theme_runtime,
                settings.general.wheel_line_scroll,
                self.proxy.clone(),
            )
            .await
        })
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(window)) = (&mut self.gpu, &self.window) else {
            return;
        };
        let result = gpu.render_boot_error(window, &self.info, &mut self.toasts);
        if gpu.has_egui_copy() {
            let clipboard = self.clipboard.get_or_insert_with(|| {
                crate::clipboard::terminal::TerminalClipboard::new(
                    window,
                    Some(event_loop.owned_display_handle()),
                )
            });
            if crate::clipboard::copy::copy_egui_text(gpu, clipboard, &mut self.toasts) {
                window.request_redraw();
            }
        }
        match result {
            Ok(true) => event_loop.exit(),
            Ok(false) => {}
            Err(e) => tracing::warn!("home-in-use screen render failed: {e}"),
        }
    }
}

impl ApplicationHandler<AppEvent> for HomeInUseScreen {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = match event_loop.create_window(crate::App::boot_window_attributes()) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                tracing::error!("home-in-use screen: window creation failed: {e}");
                self.show_without_window();
                event_loop.exit();
                return;
            }
        };
        match self.create_gpu(&window) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(e) => {
                tracing::error!("home-in-use screen: GPU initialization failed: {e:#}");
                self.show_without_window();
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window.clone());
        // 첫 프레임을 그린 뒤 표시한다. 렌더에 실패해도 창은 보인다.
        self.render(event_loop);
        #[cfg(debug_assertions)]
        crate::boot::debug_background::show_boot_window(&window);
        #[cfg(not(debug_assertions))]
        window.set_visible(true);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::RedrawRequested => {
                self.render(event_loop);
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu
                    && size.width > 0
                    && size.height > 0
                {
                    gpu.resize(*size);
                }
            }
            _ => {}
        }
        if let (Some(gpu), Some(window)) = (&mut self.gpu, &self.window) {
            gpu.handle_egui_event(window, &event);
            // RedrawRequested에서 다시 요청하면 유휴 상태에도 렌더가 계속 반복된다.
            if !matches!(event, WindowEvent::RedrawRequested) {
                window.request_redraw();
            }
        }
    }

    /// egui's repaint requests (a toast fading) come as events, as in App.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::EguiRepaint { .. } => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            AppEvent::EguiRepaintAfter { at, .. } => {
                self.repaint_at = Some(self.repaint_at.map_or(at, |cur| cur.min(at)));
            }
            _ => {}
        }
    }

    /// Draws a delayed repaint once due and keeps the Wayland selection's queue running, then
    /// sleeps until the next of the two.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let sending = self
            .clipboard
            .as_mut()
            .is_some_and(|clipboard| clipboard.pump_selection());
        let now = Instant::now();
        if self.repaint_at.is_some_and(|at| at <= now) {
            self.repaint_at = None;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        let poll_at = sending.then(|| now + crate::app::timers::PENDING_MENU_POLL_INTERVAL);
        let wake = match (self.repaint_at, poll_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        event_loop.set_control_flow(match wake {
            Some(at) => winit::event_loop::ControlFlow::WaitUntil(at),
            None => winit::event_loop::ControlFlow::Wait,
        });
    }
}
