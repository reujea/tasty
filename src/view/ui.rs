//! `View` trait — 엔진이 관리하는 render target 의 추상화.
//!
//! ```text
//! View (sealed trait)
//! ├── ModalView (supertrait)  — Settings, Quit, Plugins
//! ├── MainView                — 사이드바/워크스페이스 호스팅 (View + Sealed 직접 구현)
//! └── PresetView               — Modeless 에디터 (View + Sealed 직접 구현)
//! ```
//!
//! 구현체는 ViewBase로 공통 필드를 공유한다. 외부 크레이트는 View를 구현할 수 없다.
//! 모달은 ModalView를, MainView와 PresetView는 View와 sealed::Sealed를 구현한다.

/// Sealed 모듈 — 외부에서 `View` 를 직접 구현하지 못하게 차단한다.
/// 모달 계열은 `ModalView` supertrait 를 경유하고, 그 외 구현체는 `View` +
/// `sealed::Sealed` 를 직접 구현한다.
pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

use winit::event::WindowEvent;

use crate::view::repaint::RepaintSource;
use crate::view::{MainView, ViewAction, ViewBase, ViewCtx};

/// 새 보조 창의 첫 프레임을 요청한다.
/// Windows는 숨긴 창에 RedrawRequested가 오지 않아 직접 한 번 렌더링한 뒤 표시한다.
pub(crate) fn present_first_frame(view: &mut impl View) {
    #[cfg(windows)]
    view.render();
    #[cfg(not(windows))]
    view.mark_dirty();
}

/// 모든 View 타입이 공유하는 최상위 트레잇.
///
/// 모달 계열은 `ModalView` 를 구현하라. 그 외(`MainView`/`PresetView` 등)는 `View`
/// 를 직접 구현하면 된다. 각 구현체는 `impl sealed::Sealed for MyView {}` 를
/// 별도로 추가해야 한다.
pub(crate) trait View: sealed::Sealed + std::any::Any {
    fn base(&self) -> &ViewBase;
    fn base_mut(&mut self) -> &mut ViewBase;

    fn handle_event(&mut self, event: WindowEvent, ctx: &mut ViewCtx<'_>) -> ViewAction;
    fn render(&mut self);

    fn poll_local_reads(&mut self, _owner: &mut crate::app::local_reads::ReadRequests) {}

    /// 팝업 관리자를 가진 별도 창의 GPU 와 관리자. 사건 입구(`judged_window_event`, 별도 창 debug 키 주입)가 끊기·결속·
    /// Esc 적용에 쓴다. 관리자가 없는 창은 `None` 이다.
    fn window_input(
        &mut self,
    ) -> Option<(
        &mut crate::gpu::GpuState,
        &mut crate::adapters::ui::popup::PopupManager,
    )> {
        None
    }

    /// MainView 다운캐스트. MainView가 아니면 `None`.
    fn as_main(&self) -> Option<&MainView> {
        self.as_any().downcast_ref::<MainView>()
    }
    fn as_main_mut(&mut self) -> Option<&mut MainView> {
        self.as_any_mut().downcast_mut::<MainView>()
    }

    /// `std::any::Any` 다운캐스트용.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// 사용자 입력에 따른 redraw 요청. 출력·애니메이션은 mark_dirty_from으로 분류한다.
    fn mark_dirty(&mut self) {
        self.mark_dirty_from(RepaintSource::Interactive);
    }

    /// 요청 원인에 따라 redraw를 미루되 dirty는 즉시 설정한다. 규칙은 [`ViewBase::mark_dirty_from`]과 같다.
    fn mark_dirty_from(&mut self, source: RepaintSource) {
        self.base_mut().mark_dirty_from(source);
    }
}
