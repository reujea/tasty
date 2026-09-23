//! `View` trait — 엔진이 관리하는 render target 의 추상화.
//!
//! ```text
//! View (sealed trait)
//! ├── ModalView (supertrait)  — Settings, Quit, Plugins
//! ├── MainView                — 사이드바/워크스페이스 호스팅 (View + Sealed 직접 구현)
//! └── PresetView               — Modeless 에디터 (View + Sealed 직접 구현)
//! ```
//!
//! 모든 구현체는 `ViewBase` 를
//! composition 하여 공통 필드를 공유한다. `View` 는 sealed 이므로 크레이트
//! 외부에서 직접 구현할 수 없다 — 모달 계열은 `ModalView` supertrait 를 거치고,
//! 그 외(`MainView`/`PresetView`)는 `View` + `sealed::Sealed` 를 직접 구현한다
//! (trait object dispatch 실사용이 없던 `TerminalHostView`/`EditorView` marker
//! supertrait 및 `Modality`/`.modality()`/`.as_modal()` 스캐폴딩은 제거됨).

/// Sealed 모듈 — 외부에서 `View` 를 직접 구현하지 못하게 차단한다.
/// 모달 계열은 `ModalView` supertrait 를 경유하고, 그 외 구현체는 `View` +
/// `sealed::Sealed` 를 직접 구현한다.
pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

use winit::event::WindowEvent;

use crate::view::repaint::RepaintSource;
use crate::view::{MainView, ViewAction, ViewBase, ViewCtx};

/// 새로 만든 창의 **첫 프레임을 반드시 올린다.**
///
/// 모달·보조 창은 전부 `with_visible(false)` 로 만들어 첫 렌더 뒤에 보여준다. 그런데
/// **Windows 에서는 숨은 창에 `RedrawRequested` 가 오지 않는다** — [`View::mark_dirty`] 는
/// 요청을 큐에 넣을 뿐이라 그 창은 영영 안 그려지고, 사용자에게는 "눌렀는데 아무 일도
/// 안 일어난다" 로 보인다. 그래서 그 플랫폼에서만 그 자리에서 한 프레임을 그린다.
///
/// **네 자리가 이 블록을 글자까지 같게 갖고 있었다** — settings · plugins · quit · preset.
/// 그중 정책이 든 부분(창 속성 · 실패 처리 · 등록)은 자리마다 다르고 그 다름이 각각
/// 문서화돼 있어 안 묶었다. 묶은 것은 **정책이 0 인 순수 플랫폼 우회 하나**뿐이다.
///
/// 묶은 이유가 중복 줄 수가 아니라 **실패의 모양**이다: 빠뜨리면 창이 **Windows 에서만**
/// 안 뜨고 다른 모든 곳에서는 초록이라, 빠뜨렸다는 사실이 그 플랫폼에 닿기 전까지
/// 아무 신호도 안 낸다. 다섯째 모달을 만드는 사람이 이 한 줄을 부르는 것과, 아홉 줄짜리
/// `#[cfg]` 짝을 기억해 내는 것은 난이도가 다르다.
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

    /// 사용자 조작발 리페인트 요청 — 상한 없이 즉시 발화한다.
    ///
    /// 출력·애니메이션 계열 유발원은 이 기본형이 아니라 [`Self::mark_dirty_from`] 으로
    /// 분류해야 상한이 걸린다.
    fn mark_dirty(&mut self) {
        self.mark_dirty_from(RepaintSource::Interactive);
    }

    /// 유발원을 밝힌 리페인트 요청. 상한 대상이면 `request_redraw()` 발화가 다음
    /// 프레임 창까지 미뤄진다 — `dirty` 는 그래도 즉시 세워지고, 미뤄진 발화는
    /// `about_to_wait` 이 `WaitUntil` 로 반드시 되살린다([`crate::view::repaint`]).
    fn mark_dirty_from(&mut self, source: RepaintSource) {
        let base = self.base_mut();
        base.dirty = true;
        if base
            .repaint
            .admit(source, std::time::Instant::now(), &base.winit)
        {
            base.winit.request_redraw();
        }
    }
}
