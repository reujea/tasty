//! `View` — GUI 어댑터. winit 윈도우, egui, modal/focus 식별자, event loop proxy,
//! views HashMap 등 *사용자의 화면* 측면을 모은다.
//!
//! trait 본체는 `src/view/ui.rs`, 구현체 모듈은 본 모듈 안 형제다.

pub(crate) mod base;
pub(crate) mod main;
pub(crate) mod modal;
pub(crate) mod plugins;
pub(crate) mod preset;
pub(crate) mod quit;
pub(crate) mod repaint;
pub(crate) mod settings;
pub(crate) mod ui;

pub(crate) use base::ViewBase;
pub(crate) use main::MainView;
pub(crate) use modal::ModalView;
pub(crate) use plugins::PluginsView;
pub(crate) use preset::PresetView;
pub(crate) use quit::QuitView;
pub(crate) use repaint::RepaintSource;
pub(crate) use settings::SettingsView;

use std::collections::HashMap;

use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::AppEvent;

/// `Box<dyn View>`에서 `MainView` 소유권을 추출한다.
/// 인자가 MainView가 아니면 `None` — 호출자가 인지 후 다르게 처리.
pub(crate) fn unbox_main(w: Box<dyn ui::View>) -> Option<Box<MainView>> {
    if !w.as_any().is::<MainView>() {
        return None;
    }
    let any: Box<dyn std::any::Any> = w;
    any.downcast::<MainView>().ok()
}

/// 이벤트 처리 결과로 View 가 요청하는 동작.
#[must_use]
pub(crate) enum ViewAction {
    /// 아무 일도 하지 않음.
    None,
    /// 이 View 를 닫음.
    Close,
    /// 이 View 를 닫고 AppEvent를 발행함.
    CloseWithEvent(AppEvent),
}

/// 이벤트 핸들러에 함께 전달되는 맥락.
pub(crate) struct ViewCtx<'a> {
    pub(crate) event_loop: &'a ActiveEventLoop,
    /// 현재 모달 View 가 활성 상태인지. true면 비모달 View 는 입력을 차단해야 한다.
    pub(crate) modal_active: bool,
    /// 현재 active plugin manager. MainView 가 frame prepare 시 plugin canvas의
    /// SharedMemory와 dirty rect에 접근하기 위해 사용한다. plugin 비활성 빌드/초기 시점에는 None.
    pub(crate) plugin_manager: Option<&'a crate::plugin::PluginManager>,
    /// attach 스트림 허브. MainView 가 로컬 redraw 로 만든 egui-mesh frame 을 attach
    /// mesh mirror 구독자에게 중계할 때 쓴다(`docs/dev-guide/egui-mesh-channel.md`의
    /// "서버측(GUI, 살아있는 window)" 참고) — Arc 기반 clone 이라 참조만 전달.
    pub(crate) stream_hub: &'a tasty_ipc::stream_hub::StreamHub,
}

pub(crate) struct ViewRegistry {
    /// winit event loop 의 proxy. AppEvent 를 enqueue 하기 위한 채널.
    /// View 영역은 GUI 어댑터로서 winit 과 직접 결합되어 있다.
    pub proxy: EventLoopProxy<AppEvent>,
    /// When Some, a modal view is active and all other views should ignore input.
    /// At most one modal can exist at a time.
    pub active_modal_id: Option<WindowId>,
    /// The view that currently has focus (receives IPC commands targeting "focused" view).
    pub focused_view_id: Option<WindowId>,
    /// 모든 View(모달 포함). `active_modal_id`로 현재 활성 모달을 식별한다.
    /// 모달도 여기에 들어가며, 모달은 엔진 전역에 최대 1개라는 불변식을 유지한다.
    /// key 는 winit `WindowId`.
    pub views: HashMap<WindowId, Box<dyn ui::View>>,
}

impl ViewRegistry {
    pub(crate) fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            proxy,
            active_modal_id: None,
            focused_view_id: None,
            views: HashMap::new(),
        }
    }

    /// Check if a modal is active.
    pub fn is_modal_active(&self) -> bool {
        self.active_modal_id.is_some()
    }
}
