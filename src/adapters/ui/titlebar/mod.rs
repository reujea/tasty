//! 공용 타이틀바를 그리고 사용자 동작을 winit 창 조작으로 전달한다.

mod caption;
mod view;

use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::AppEvent;
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::PhysicalPx;

pub use view::{TitlebarAction, TitlebarControls, TitlebarProps, draw_titlebar_view};

/// macOS 네이티브 신호등(standardWindowButton) 클러스터가 차지하는 좌측 폭
/// (logical points). fullsize-content-view 에서 신호등은 OS 가 좌상단 고정 위치에
/// 그리므로, egui titlebar 의 드래그 영역은 이 폭만큼 비운다(carve-out). 디자인
/// inset(padding 12 + 점 3×12 + gap 2×8) 기준 + 네이티브 클러스터 여유.
/// OS 가 고정하는 geometry 라 테마 토큰이 아니다.
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_INSET: tasty_type_geometry::length::LogicalPx =
    tasty_type_geometry::length::LogicalPx(78.0);

/// 타이틀바가 차지하는 물리 높이. 터미널 영역과 사이드바의 시작 위치에 사용한다.
pub fn top_inset(scale_factor: f32) -> PhysicalPx {
    theme::theme().titlebar_height.to_physical(scale_factor)
}

/// Linux는 오른쪽 최소화·최대화·닫기 버튼을 사용한다. DE 자동 감지는 하지 않는다.
#[cfg(target_os = "linux")]
fn os_controls() -> Option<TitlebarControls> {
    use view::{ControlSide, WindowButton};
    Some(TitlebarControls {
        buttons: vec![
            WindowButton::Minimize,
            WindowButton::Maximize,
            WindowButton::Close,
        ],
        side: ControlSide::Right,
    })
}

#[cfg(not(target_os = "linux"))]
fn os_controls() -> Option<TitlebarControls> {
    None
}

/// 타이틀바를 사이드바보다 먼저 그린다. 닫기 버튼은 공용 CloseWindow 이벤트로 전달한다.
/// 가장자리 버튼 hover도 여기서 초기화하고 이후 화면에서 누적한다.
pub fn draw_titlebar(
    ctx: &egui::Context,
    state: &mut MainViewState,
    window: &Window,
    proxy: &EventLoopProxy<AppEvent>,
) {
    let th = theme::theme();
    #[cfg(target_os = "macos")]
    let left_inset = MACOS_TRAFFIC_LIGHT_INSET.value();
    #[cfg(not(target_os = "macos"))]
    let left_inset = 0.0;
    let props = TitlebarProps {
        theme: &th,
        active: window.has_focus(),
        height: th.titlebar_height.value(),
        left_inset,
        controls: os_controls(),
        maximized: window.is_maximized(),
    };

    let result = draw_titlebar_view(ctx, &props);
    state.resize_edge_widget_hovered = result.resize_priority_hovered;
    state.titlebar_close_rect = result.close_rect;
    state.titlebar_rect = Some(physical_panel_rect(
        result.panel_rect,
        ctx.pixels_per_point(),
    ));
    for action in result.actions {
        match action {
            TitlebarAction::StartDrag => {
                // 창 이동은 마우스를 누른 상태에서 시작해야 한다. 실패하면 로그를 남긴다.
                if let Err(e) = window.drag_window() {
                    tracing::warn!("titlebar drag_window failed: {e}");
                }
            }
            TitlebarAction::ToggleMaximize => {
                window.set_maximized(!window.is_maximized());
            }
            TitlebarAction::Minimize => {
                window.set_minimized(true);
            }
            TitlebarAction::Close => {
                crate::shortcuts::send_app_event(proxy, AppEvent::CloseWindow(window.id()));
            }
        }
    }
}

/// 패널 배치 영역을 그 패스의 배율로 물리 좌표에 옮긴다. 포인터 사건 좌표(물리)와 비교하는 데 쓴다.
fn physical_panel_rect(rect: egui::Rect, pixels_per_point: f32) -> crate::model::PhysicalRect {
    crate::model::LogicalRect {
        x: crate::model::LogicalPx(rect.min.x),
        y: crate::model::LogicalPx(rect.min.y),
        width: crate::model::LogicalPx(rect.width()),
        height: crate::model::LogicalPx(rect.height()),
    }
    .to_physical(pixels_per_point)
}

/// 창 리사이즈 방향을 egui 커서로 바꾼다.
pub fn resize_cursor(dir: winit::window::ResizeDirection) -> egui::CursorIcon {
    use winit::window::ResizeDirection as D;
    match dir {
        D::North => egui::CursorIcon::ResizeNorth,
        D::South => egui::CursorIcon::ResizeSouth,
        D::East => egui::CursorIcon::ResizeEast,
        D::West => egui::CursorIcon::ResizeWest,
        D::NorthEast => egui::CursorIcon::ResizeNorthEast,
        D::NorthWest => egui::CursorIcon::ResizeNorthWest,
        D::SouthEast => egui::CursorIcon::ResizeSouthEast,
        D::SouthWest => egui::CursorIcon::ResizeSouthWest,
    }
}
