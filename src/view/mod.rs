//! winit 창, egui 입력과 렌더링, 모달·포커스를 관리하는 GUI 어댑터.
//! View trait은 src/view/ui.rs에 있다.

pub(crate) mod base;
pub(crate) mod file_chooser;
pub(crate) mod judged_event;
pub(crate) mod key_event;
pub(crate) mod main;
pub(crate) mod modal;
pub(crate) mod plugins;
pub(crate) mod preset;
pub(crate) mod quit;
pub(crate) mod repaint;
pub(crate) mod settings;
pub(crate) mod state;
pub(crate) mod sync_hold;
pub(crate) mod ui;

pub(crate) use base::ViewBase;
pub(crate) use judged_event::judged_window_event;
pub(crate) use main::MainView;
pub(crate) use modal::ModalView;
pub(crate) use plugins::PluginsView;
pub(crate) use preset::PresetView;
pub(crate) use quit::QuitView;
pub(crate) use repaint::{RepaintSource, install_redraw_caps};
pub(crate) use settings::SettingsView;

use std::collections::HashMap;

use winit::event_loop::EventLoopProxy;
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

/// 자체 키 처리를 가진 egui 전용 창(설정·종료 확인·Preset·Plugins)이 다시 그리기 전에 부른다. 편집 키
/// 판정이 보류했다 푼 이벤트를 받은 순서대로 `handle`(egui와 창 자체 처리)에 다시 준다. 원래 키는 판정한
/// 편집으로 한 번만 넘기고, 보류 중에 온 이벤트는 판정부터 다시 거친다. 창이 닫히면 그 결과를 돌려준다.
pub(crate) fn replay_edit_keys<V: ui::View>(
    view: &mut V,
    handle: fn(
        &mut V,
        winit::event::WindowEvent,
        Option<crate::adapters::ui::input::edit_key::EditKey>,
    ) -> ViewAction,
) -> Option<ViewAction> {
    let release = view.base_mut().gpu.take_edit_release()?;
    crate::gfx::gpu::edit_keys::replay_release(
        release,
        |action| !matches!(action, ViewAction::None),
        |event, judged| handle(view, event, judged),
    )
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
    /// 현재 모달 View 가 활성 상태인지. true면 비모달 View 는 입력을 차단해야 한다.
    pub(crate) modal_active: bool,
    /// 이 창에 연결된 engine. App의 engine registry가 창 ID로 찾아 넘긴다. 모달 View는 None이다.
    pub(crate) engine: Option<crate::runtime::engine_read::EngineRead<'a>>,
    /// App 이 이 키 사건의 출발 노드를 plugin 단축키 단계 앞에서 이미 정했는지(`MainView::settle_key_event`). 참이면
    /// 메인 창은 같은 사건에서 다시 정하지 않고 그 결과를 읽는다.
    pub(crate) key_settled: bool,
}

/// 열린 모달의 종류. 창을 열 때 기록해 debug 조회에서 모달을 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalKind {
    Settings,
    Plugins,
    Quit,
}

impl ModalKind {
    /// debug IPC 응답에 쓰는 이름. 내부 판정은 enum을 사용한다.
    #[cfg(debug_assertions)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Plugins => "plugins",
            Self::Quit => "quit",
        }
    }
}

/// 활성 모달의 창 ID와 종류. 둘은 항상 함께 바뀐다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveModal {
    pub(crate) id: WindowId,
    pub(crate) kind: ModalKind,
}

pub(crate) struct ViewRegistry {
    /// Strong native display owner for foreign-queue integrations.
    pub(crate) display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    /// winit event loop 의 proxy. AppEvent 를 enqueue 하기 위한 채널.
    /// View 영역은 GUI 어댑터로서 winit 과 직접 결합되어 있다.
    pub proxy: EventLoopProxy<AppEvent>,
    /// 활성 모달의 유일한 원본. Some이면 다른 View는 입력을 무시한다. 모달은 최대 1개다.
    /// 쓰기는 `set_active_modal`/`take_active_modal`만 사용하도록 필드를 비공개로 둔다.
    active_modal: Option<ActiveModal>,
    /// The view that currently has focus (receives IPC commands targeting "focused" view).
    pub focused_view_id: Option<WindowId>,
    /// 모든 View(모달 포함). `active_modal`로 현재 활성 모달을 식별한다.
    /// 모달도 여기에 들어가며, 모달은 엔진 전역에 최대 1개라는 불변식을 유지한다.
    /// key 는 winit `WindowId`.
    pub views: HashMap<WindowId, Box<dyn ui::View>>,
}

impl ViewRegistry {
    pub(crate) fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            display_owner: None,
            proxy,
            active_modal: None,
            focused_view_id: None,
            views: HashMap::new(),
        }
    }

    /// Check if a modal is active.
    pub fn is_modal_active(&self) -> bool {
        self.active_modal.is_some()
    }

    /// debug ui.state 투영만 종류까지 읽는다. 다른 호출부는 `active_modal_id`를 사용한다.
    #[cfg(debug_assertions)]
    pub(crate) fn active_modal(&self) -> Option<ActiveModal> {
        self.active_modal
    }

    pub(crate) fn active_modal_id(&self) -> Option<WindowId> {
        self.active_modal.map(|m| m.id)
    }

    pub(crate) fn set_active_modal(&mut self, id: WindowId, kind: ModalKind) {
        self.active_modal = Some(ActiveModal { id, kind });
        self.note_modal_to_webviews(true);
    }

    pub(crate) fn take_active_modal(&mut self) -> Option<ActiveModal> {
        self.note_modal_to_webviews(false);
        self.active_modal.take()
    }

    /// native WebView 콜백은 host 큐보다 먼저 편집 키를 실행하므로 모달 상태를 바로 알린다.
    fn note_modal_to_webviews(&mut self, open: bool) {
        for view in self.views.values_mut() {
            if let Some(main) = view.as_main_mut() {
                main.webview_key_bridge.set_modal_open(open);
            }
        }
    }
}
