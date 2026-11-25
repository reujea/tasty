//! 자체 키 처리를 가진 egui 전용 창(설정·종료 확인·Preset·Plugins)이 창 이벤트 하나를 편집 키 판정, egui,
//! 창 자체 처리 순서로 넘기는 공통 흐름. 판정이 가져간 이벤트는 창 자체 처리에 가지 않는다. 팝업 관리자를 가진 창은
//! 판정 앞에서 사건 시점 노드로 입력을 끊고 그 노드에서 키의 수신 노드를 정한다(`window_input::settle_window_input`).
//! egui 에 실은 키의 몫을 그 노드로 묶고, 창 자체 처리는 창 노드가 받은 키만 본다. 더블탭 검출기는 단계 11 맨 앞
//! 메인 창과 같은 자리(`key_event::key_ends_at_latch`)에서 모든 키를 받고, 검출한 동작은 경로 level 로 거른다. 계약:
//! `docs/design/policies/key-mapping.md`의 "보류와 재생".

use winit::event::WindowEvent;

use super::ViewAction;
use super::key_event::{KeyEventSteps, key_ends_at_latch, run_key_event};
use super::ui::View;
use super::window_input::{
    bind_window_input, incoming_of, settle_window_input, window_edit_target, window_key_of,
};
use crate::adapters::ui::input::edit_key::EditKey;
use crate::adapters::ui::input::key_level::PathLevels;
use crate::adapters::ui::popup::PopupManager;
use crate::adapters::ui::popup::window_segments::WindowReceiver;
use crate::double_tap::{DoubleTapDetector, DoubleTapKey};

/// 별도 창의 GPU 와 팝업 관리자. 관리자가 없는 창(종료 확인)은 `None` 이고 노드는 창 위젯 하나다.
pub(crate) type WindowInputOf<V> =
    fn(&mut V) -> Option<(&mut crate::gpu::GpuState, &mut PopupManager)>;

/// 별도 창의 창 자체 처리. `event` 는 단계 11 의 창 노드 처리, `keys` 는 그 처리가 키 누름을 바인딩으로 쓰는지(설정
/// 창의 toggle_settings, 단계 4 의 창 노드 후보), `taps` 는 더블탭 검출기를 가진 창의 단계 11 맨 앞 자리다.
pub(crate) struct WindowOwn<V> {
    pub(crate) event: fn(&mut V, WindowEvent) -> ViewAction,
    pub(crate) keys: fn(&V, &winit::event::KeyEvent) -> bool,
    pub(crate) taps: Option<WindowTaps<V>>,
}

impl<V> WindowOwn<V> {
    /// 키 바인딩도 더블탭 검출기도 없는 창 자체 처리.
    pub(crate) fn plain(event: fn(&mut V, WindowEvent) -> ViewAction) -> Self {
        Self {
            event,
            keys: |_, _| false,
            taps: None,
        }
    }
}

impl<V> Clone for WindowOwn<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for WindowOwn<V> {}

/// 별도 창의 더블탭 자리. `detector` 는 그 창의 검출기다. `run` 은 검출한 더블탭 하나를 그 사건 경로의 level 집합과
/// 단계 4 수신 노드가 창 노드인지로 처리하고, 처리가 사건을 끝내면 그 결과를 돌려준다.
pub(crate) struct WindowTaps<V> {
    pub(crate) detector: fn(&mut V) -> &mut DoubleTapDetector,
    pub(crate) run: fn(&mut V, DoubleTapKey, PathLevels, bool) -> Option<ViewAction>,
}

impl<V> Clone for WindowTaps<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for WindowTaps<V> {}

/// 단계 11 맨 앞(별도 창): 메인 창과 같은 자리(`key_ends_at_latch`, 별도 창에는 래치 노드가 없다)로 검출기에 키 사건
/// 하나를 준다. 단계 4 수신 노드와 관계없이 온다. 검출한 더블탭은 `taps.run` 이 그 사건 경로의 level(`levels`)로 거른다.
/// 사건이 여기서 끝나면 그 결과다: 뗌, 또는 더블탭 처리가 끝낸 누름. `at` 은 사건 도착 시각, `receives` 는 단계 4
/// 수신 노드가 창 노드인지다.
pub(crate) fn window_tap_front<V>(
    view: &mut V,
    taps: WindowTaps<V>,
    key: &winit::keyboard::Key,
    pressed: bool,
    at: std::time::Instant,
    levels: PathLevels,
    receives: bool,
) -> Option<ViewAction> {
    let ends = key_ends_at_latch((taps.detector)(view), key, pressed, at, || false);
    if let Some(dt) = (taps.detector)(view).take()
        && let Some(action) = (taps.run)(view, dt, levels, receives)
    {
        return Some(action);
    }
    ends.then_some(ViewAction::None)
}

/// 창 이벤트 하나. `to_egui`면 편집 키 판정을 거쳐 egui에 넘긴다. 판정이 가져간 이벤트는 다시 그리기만 요청하고
/// 창 자체 처리(`own.event`)에 주지 않는다. 팝업이 받은 키도 `own.event` 에 주지 않는다(단계 4 수신 노드가 창 노드인
/// 키만 간다). 더블탭 자리(`own.taps`)는 그 앞에서 모든 키를 받는다. 보류가 풀리면 `super::replay_edit_keys`가 다시
/// 이 함수로 보낸다. `judged`는 그때 원래 키가 판정받은 편집이다. `input` 은 끊기와 결속에 쓰는 창의 GPU 와 관리자다.
pub(crate) fn judged_window_event<V: View>(
    view: &mut V,
    event: WindowEvent,
    judged: Option<EditKey>,
    to_egui: bool,
    own: WindowOwn<V>,
    input: WindowInputOf<V>,
) -> ViewAction {
    // 보류 검사는 어느 단계도 사건을 읽기 전 맨 앞 한 자리다. 보류가 풀리면 `replay_edit_keys` 가 이 입구로 다시
    // 보낸다. 판정받은 원래 키(`judged`)는 이미 지났다.
    if judged.is_none() && hold_window_input(view, &event, input) {
        return ViewAction::None;
    }
    let node = WindowKeyNode {
        settle: |view: &mut V, event: &WindowEvent| {
            let mods = view.base_mut().state.modifiers;
            let key = window_key_of(event, mods, |key| (own.keys)(&*view, key));
            let (gpu, popups) = input(view)?;
            let incoming = incoming_of(event, mods)?;
            let ctx = gpu.egui_ctx.clone();
            settle_window_input(gpu, &ctx, popups, incoming, key)
        },
        bind: |view: &mut V, event: &WindowEvent| {
            let mods = view.base_mut().state.modifiers;
            if let Some((gpu, popups)) = input(view)
                && let Some(incoming) = incoming_of(event, mods)
            {
                let held = gpu.egui_escape_held();
                let ctx = gpu.egui_ctx.clone();
                bind_window_input(&ctx, popups, incoming, gpu.egui_input_events_mut(), held);
            }
        },
        taps: |view: &mut V, event: &WindowEvent, receives: bool| {
            let taps = own.taps?;
            let WindowEvent::KeyboardInput { event: key, .. } = event else {
                return None;
            };
            let levels = input(view)
                .and_then(|(_, popups)| popups.window_key_levels())
                .unwrap_or_else(|| {
                    crate::adapters::ui::input::key_nodes::separate_window_levels(None)
                });
            let pressed = key.state == winit::event::ElementState::Pressed;
            let at = view.base_mut().gpu.event_arrival();
            window_tap_front(view, taps, &key.logical_key, pressed, at, levels, receives)
        },
    };
    let mut steps = window_steps(
        view,
        judged,
        to_egui,
        |view, event, judged, receiver| {
            let mods = view.base_mut().state.modifiers;
            let routed = input(view).is_some();
            let edit_target = window_edit_target(incoming_of(event, mods), receiver, routed);
            let base = view.base_mut();
            base.gpu
                .handle_egui_event_for_window(&base.winit, event, judged, edit_target)
        },
        |view| view.mark_dirty(),
        own.event,
        node,
    );
    run_key_event(&mut steps, event)
}

/// 팝업 관리자를 가진 창(설정·Preset·Plugins)의 보류 줄(`GpuState::hold_window_input`). 관리자가 없는 종료 확인
/// 창은 지금의 편집 키 보류(단계 6 안)만 쓴다.
fn hold_window_input<V: View>(view: &mut V, event: &WindowEvent, input: WindowInputOf<V>) -> bool {
    let Some((_, popups)) = input(view) else {
        return false;
    };
    let handed = popups.has_undrawn_handed_escape();
    let base = view.base_mut();
    base.gpu.hold_window_input(&base.winit, event, handed)
}

/// 별도 창 단계 3~5(`settle`, 키 사건의 단계 4 수신 결과. 관리자가 없는 창이나 키가 아닌 사건은 None), egui 에 실은
/// 키의 몫 결속(`bind`), 단계 11 맨 앞 더블탭 자리(`taps`, 단계 4 수신 노드가 창 노드인지를 받는다. 사건을 끝내면 그
/// 결과).
struct WindowKeyNode<S, B, T> {
    settle: S,
    bind: B,
    taps: T,
}

/// [`judged_window_event`]의 단계. `egui`는 판정과 egui 처리 결과이며 `None`이면 판정이 이벤트를 가져갔다.
/// 단계 순서는 메인 창과 같은 [`run_key_event`]가 정한다. 별도 창이 가진 단계는 편집 판정과 egui 적재(한 호출),
/// 창 자체 처리뿐이다.
fn window_steps<V, E, G, D, O, S, B, T>(
    view: &mut V,
    judged: Option<EditKey>,
    to_egui: bool,
    egui: G,
    mark_dirty: D,
    own: O,
    node: WindowKeyNode<S, B, T>,
) -> JudgedWindowSteps<'_, V, G, D, O, S, B, T>
where
    G: FnOnce(&mut V, &E, Option<EditKey>, Option<WindowReceiver>) -> Option<(bool, bool)>,
    D: FnMut(&mut V),
    O: FnOnce(&mut V, E) -> ViewAction,
    S: FnMut(&mut V, &E) -> Option<WindowReceiver>,
    B: FnMut(&mut V, &E),
    T: FnMut(&mut V, &E, bool) -> Option<ViewAction>,
{
    JudgedWindowSteps {
        view,
        judged,
        to_egui,
        egui: Some(egui),
        mark_dirty,
        own: Some(own),
        node,
        receiver: None,
    }
}

/// 별도 창의 단계. 편집 판정은 egui 적재와 같은 GPU 호출 안에 있어 단계 6은 받은 표시를 그대로 넘긴다.
struct JudgedWindowSteps<'v, V, G, D, O, S, B, T> {
    view: &'v mut V,
    judged: Option<EditKey>,
    to_egui: bool,
    egui: Option<G>,
    mark_dirty: D,
    own: Option<O>,
    node: WindowKeyNode<S, B, T>,
    /// 키 사건의 단계 4 수신 결과(`settle`). egui 수신자가 없으면 egui 에 싣지 않고, 창 노드가 받지 않으면 창 자체
    /// 처리에 가지 않는다. None(관리자가 없는 창, 키가 아닌 사건)은 둘 다 간다.
    receiver: Option<WindowReceiver>,
}

impl<V, E, G, D, O, S, B, T> KeyEventSteps<E> for JudgedWindowSteps<'_, V, G, D, O, S, B, T>
where
    G: FnOnce(&mut V, &E, Option<EditKey>, Option<WindowReceiver>) -> Option<(bool, bool)>,
    D: FnMut(&mut V),
    O: FnOnce(&mut V, E) -> ViewAction,
    S: FnMut(&mut V, &E) -> Option<WindowReceiver>,
    B: FnMut(&mut V, &E),
    T: FnMut(&mut V, &E, bool) -> Option<ViewAction>,
{
    fn settle_key_node(&mut self, event: &E) {
        self.receiver = (self.node.settle)(self.view, event);
    }

    fn judge_edit_key(&mut self, _event: &E) -> Option<Option<EditKey>> {
        Some(self.judged)
    }

    fn feed_egui(&mut self, event: &E, judged: Option<EditKey>) -> Option<(bool, bool)> {
        if !self.to_egui || self.receiver.is_some_and(|r| r.egui_node().is_none()) {
            return Some((false, false));
        }
        let egui = self.egui.take()?;
        let fed = egui(self.view, event, judged, self.receiver);
        match fed {
            Some(_) => (self.node.bind)(self.view, event),
            None => (self.mark_dirty)(self.view),
        }
        fed
    }

    fn run_receiver(
        &mut self,
        event: E,
        _judged: Option<EditKey>,
        (_, repaint): (bool, bool),
    ) -> ViewAction {
        if repaint {
            (self.mark_dirty)(self.view);
        }
        let reaches_window = self.receiver.is_none_or(WindowReceiver::reaches_window);
        if let Some(action) = (self.node.taps)(self.view, &event, reaches_window) {
            return action;
        }
        match self.own.take() {
            Some(own) if reaches_window => own(self.view, event),
            _ => ViewAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Window {
        dirty: u32,
        egui: u32,
        own: Vec<&'static str>,
        steps: Vec<&'static str>,
    }

    /// `egui`가 돌려줄 판정·egui 결과로 이벤트 하나를 넘긴다.
    fn run(window: &mut Window, to_egui: bool, egui: Option<(bool, bool)>) -> ViewAction {
        let mut steps = window_steps(
            window,
            None,
            to_egui,
            |window: &mut Window, _, _, _| {
                window.egui += 1;
                window.steps.push("egui");
                egui
            },
            |window| window.dirty += 1,
            |window, event| {
                window.own.push(event);
                ViewAction::Close
            },
            WindowKeyNode {
                settle: |window: &mut Window, _: &&str| {
                    window.steps.push("settle");
                    None
                },
                bind: |window: &mut Window, _: &&str| window.steps.push("bind"),
                taps: |_: &mut Window, _: &&str, _| None,
            },
        );
        run_key_event(&mut steps, "alt+c")
    }

    /// 사건 시점 노드는 판정·egui 적재 앞에서 정하고(단계 3~5), 몫 결속은 egui 에 실린 사건만 남긴다. 판정이 가져간
    /// 사건과 egui 에 넘기지 않는 사건은 묶지 않는다.
    #[test]
    fn the_node_is_settled_before_loading_and_only_a_loaded_event_binds() {
        for (to_egui, egui, steps) in [
            (true, Some((true, true)), &["settle", "egui", "bind"][..]),
            (true, None, &["settle", "egui"][..]),
            (false, None, &["settle"][..]),
        ] {
            let mut window = Window::default();
            let action = run(&mut window, to_egui, egui);
            assert!(matches!(action, ViewAction::Close | ViewAction::None));
            assert_eq!(window.steps, steps, "to_egui={to_egui} egui={egui:?}");
        }
    }

    /// 판정이 가져간 키(예: copy와 toggle_settings를 같은 키에 묶고 복사를 보류)는 창 자체 처리(설정 창 닫기)에
    /// 가지 않고 다시 그리기만 요청한다.
    #[test]
    fn a_taken_key_skips_the_window_s_own_handling() {
        let mut window = Window::default();
        let action = run(&mut window, true, None);
        assert!(matches!(action, ViewAction::None));
        assert!(window.own.is_empty(), "the window did not handle it");
        assert_eq!(window.dirty, 1);
    }

    /// 팝업 관리자를 가진 별도 창 대역: egui 입력 큐(`window_input::tests::Harness`), 관리자, 창 자체 처리가 본 키.
    struct PopupWindow {
        h: crate::view::window_input::tests::Harness,
        popups: PopupManager,
        own: Vec<egui::Key>,
    }

    impl PopupWindow {
        /// 팝업 하나(선언 기본: 덮지 않음, 소비 집합 `Keys`)를 포커스를 주며 연 창. `open` 이 거짓이면 팝업이 없다.
        fn new(open: bool) -> Self {
            use crate::adapters::ui::popup::{PopupScope, PopupState};
            let mut popups = PopupManager::new();
            popups.register(PopupState::new(
                "judged_event_popup",
                "p",
                egui::vec2(200.0, 100.0),
                PopupScope::Window,
            ));
            if open {
                let request =
                    crate::intent::OpenRequest::new("judged_event_popup", PopupScope::Window)
                        .at(egui::pos2(100.0, 100.0))
                        .focused();
                assert!(popups.open_request(&request, true).is_some());
            }
            Self {
                h: crate::view::window_input::tests::Harness::new(),
                popups,
                own: Vec::new(),
            }
        }

        /// 수식키 없는 키 누름 하나를 제품 단계(사건 입구 끊기·egui 적재·결속·창 자체 처리)로 넘긴다.
        fn key(&mut self, key: egui::Key) -> ViewAction {
            use crate::adapters::ui::popup::frame_keys::Incoming;
            use crate::view::main::input_segment::EguiPressSink;
            let incoming = if key == egui::Key::Escape {
                Incoming::Escape
            } else {
                Incoming::Key
            };
            let mut steps = window_steps(
                self,
                None,
                true,
                |w: &mut PopupWindow, key: &egui::Key, _, _| {
                    w.h.queued_events().push(egui::Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    Some((true, false))
                },
                |_| {},
                |w, key| {
                    w.own.push(key);
                    ViewAction::None
                },
                WindowKeyNode {
                    settle: move |w: &mut PopupWindow, key: &egui::Key| {
                        let ctx = w.h.ctx.clone();
                        let key = crate::adapters::ui::popup::window_segments::WindowKey::Path {
                            key: crate::adapters::ui::input::key_route::RouteKey {
                                key: Some(*key),
                                modifiers: egui::Modifiers::NONE,
                                super_held: false,
                            },
                            own: false,
                        };
                        settle_window_input(&mut w.h, &ctx, &mut w.popups, incoming, Some(key))
                    },
                    bind: move |w: &mut PopupWindow, _: &egui::Key| {
                        let ctx = w.h.ctx.clone();
                        bind_window_input(
                            &ctx,
                            &mut w.popups,
                            incoming,
                            w.h.queued_events(),
                            false,
                        );
                    },
                    taps: |_: &mut PopupWindow, _: &egui::Key, _| None,
                },
            );
            run_key_event(&mut steps, key)
        }
    }

    /// 더블탭 검출기를 가진 별도 창 대역(설정 창의 더블탭 처리 `settings_double_tap`): 관리자, egui 입력 큐, 검출기,
    /// 단축키 설정, 녹화 캡처를 둔 횟수.
    struct TapWindow {
        popups: PopupManager,
        h: crate::view::window_input::tests::Harness,
        detector: crate::double_tap::DoubleTapDetector,
        kb: crate::settings::KeybindingSettings,
        captured: u32,
    }

    impl TapWindow {
        const TAPS: WindowTaps<TapWindow> = WindowTaps {
            detector: |w| &mut w.detector,
            run: |w, dt, levels, receives| match crate::view::settings::settings_double_tap(
                &w.kb, false, dt, levels, receives,
            ) {
                crate::view::settings::SettingsTap::Close => Some(ViewAction::Close),
                crate::view::settings::SettingsTap::Capture => {
                    w.captured += 1;
                    None
                }
                crate::view::settings::SettingsTap::Nothing => None,
            },
        };

        /// `covered` 면 창을 덮는 팝업(설정 창 충돌 질문 선언)을 포커스를 주며 연 창. `toggle_settings` 는 `binding`.
        fn new(covered: bool, binding: Option<&str>) -> Self {
            let mut popups = PopupManager::new();
            popups.register_def(&crate::view::settings::ui::keybinding_conflict_def(), 1.0);
            if covered {
                let request = crate::intent::OpenRequest::new(
                    crate::view::settings::ui::KEYBINDING_CONFLICT_POPUP_ID,
                    crate::adapters::ui::popup::PopupScope::Window,
                )
                .at(egui::pos2(500.0, 400.0))
                .focused();
                let iid = popups.open_request(&request, true).expect("opened");
                assert!(popups.instance(iid).expect("open").covers());
            }
            let mut kb = crate::settings::KeybindingSettings::default();
            if let Some(binding) = binding {
                assert!(kb.set_field("toggle_settings", binding));
            }
            Self {
                popups,
                h: crate::view::window_input::tests::Harness::new(),
                detector: crate::double_tap::DoubleTapDetector::new(),
                kb,
                captured: 0,
            }
        }

        /// Shift 누름·뗌 하나를 제품 단계(사건 입구 끊기·단계 4 수신 노드·단계 11 맨 앞 더블탭 자리·창 자체 처리)로
        /// 넘긴다. 수식키 단독 누름·뗌은 지금 경로로 남는 사건이다.
        fn shift(&mut self, pressed: bool) -> ViewAction {
            use crate::adapters::ui::popup::frame_keys::Incoming;
            use crate::adapters::ui::popup::window_segments::WindowKey;
            let mut steps = window_steps(
                self,
                None,
                true,
                |_: &mut TapWindow, _: &bool, _, _| Some((false, false)),
                |_| {},
                |_, _| ViewAction::None,
                WindowKeyNode {
                    settle: |w: &mut TapWindow, _: &bool| {
                        let ctx = w.h.ctx.clone();
                        let key = Some(WindowKey::Current);
                        settle_window_input(&mut w.h, &ctx, &mut w.popups, Incoming::Key, key)
                    },
                    bind: |_: &mut TapWindow, _: &bool| {},
                    taps: |w: &mut TapWindow, pressed: &bool, receives| {
                        let levels = w.popups.window_key_levels().expect("a key was settled");
                        let shift = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Shift);
                        let at = std::time::Instant::now();
                        window_tap_front(w, Self::TAPS, &shift, *pressed, at, levels, receives)
                    },
                },
            );
            run_key_event(&mut steps, pressed)
        }

        fn double_tap(&mut self) -> [ViewAction; 3] {
            [self.shift(true), self.shift(false), self.shift(true)]
        }
    }

    /// Given 창을 덮는 팝업이 포커스를 가지고 열린 설정 창(toggle_settings = shift+shift), When Shift 를 누르고 떼고 다시
    /// 누르면, Then engine level 더블탭이라 덮여도 창을 닫는다.
    #[test]
    fn a_toggle_settings_double_tap_under_a_covering_popup_closes_the_window() {
        let mut window = TapWindow::new(true, Some("shift+shift"));
        let [first, release, second] = window.double_tap();
        assert!(matches!(first, ViewAction::None));
        assert!(matches!(release, ViewAction::None));
        assert!(matches!(second, ViewAction::Close));
    }

    /// Given 팝업이 없는 설정 창(toggle_settings = shift+shift), When 같은 더블탭이면, Then 창을 닫는다.
    #[test]
    fn a_toggle_settings_double_tap_without_a_popup_closes_the_window() {
        let mut window = TapWindow::new(false, Some("shift+shift"));
        assert!(matches!(window.double_tap()[2], ViewAction::Close));
    }

    /// Given 창을 덮는 팝업이 포커스를 가지고 열린 설정 창(더블탭 바인딩 없음), When Shift 를 두 번 탭하면, Then 그
    /// 더블탭은 창 범위 처리(녹화 캡처, 창 내용)에 가지 않는다.
    #[test]
    fn a_double_tap_under_a_covering_popup_leaves_the_window_capture_alone() {
        let mut window = TapWindow::new(true, None);
        assert!(
            window
                .double_tap()
                .iter()
                .all(|a| matches!(a, ViewAction::None))
        );
        assert_eq!(window.captured, 0);
    }

    /// Given 팝업이 없는 설정 창(더블탭 바인딩 없음), When 같은 더블탭이면, Then 창 노드가 받아 녹화 캡처로 둔다.
    #[test]
    fn a_double_tap_without_a_popup_is_captured_for_the_window() {
        let mut window = TapWindow::new(false, None);
        assert!(
            window
                .double_tap()
                .iter()
                .all(|a| matches!(a, ViewAction::None))
        );
        assert_eq!(window.captured, 1);
    }

    /// Given 별도 창에서 팝업(소비 집합 `Keys`)이 포커스 노드, When 수식키 없는 글자 키를 누르면, Then 그 키는 팝업이
    /// 받고 창 자체 처리(설정 창의 toggle_settings·더블탭)는 보지 않는다.
    #[test]
    fn a_key_the_window_popup_receives_skips_the_window_s_own_handling() {
        let mut window = PopupWindow::new(true);
        drop(window.key(egui::Key::Q));
        assert!(window.own.is_empty(), "own saw {:?}", window.own);
    }

    /// Given 별도 창에 팝업이 없다, When 같은 글자 키를 누르면, Then 창 노드가 받아 창 자체 처리가 그 키를 본다.
    #[test]
    fn a_key_without_a_window_popup_reaches_the_window_s_own_handling() {
        let mut window = PopupWindow::new(false);
        drop(window.key(egui::Key::Q));
        assert_eq!(window.own, [egui::Key::Q]);
    }

    /// 판정이 넘긴 이벤트는 egui를 거쳐 창 자체 처리에 간다. egui에 넘기지 않는 이벤트는 판정 없이 바로 간다.
    #[test]
    fn a_free_event_reaches_the_window_s_own_handling() {
        for (to_egui, egui, dirty, egui_calls) in [
            (true, Some((true, true)), 1, 1),
            (true, Some((false, false)), 0, 1),
            (false, None, 0, 0),
        ] {
            let mut window = Window::default();
            let action = run(&mut window, to_egui, egui);
            assert!(matches!(action, ViewAction::Close));
            assert_eq!(window.own, ["alt+c"]);
            assert_eq!((window.dirty, window.egui), (dirty, egui_calls));
        }
    }

    /// 단계 4 결과로 egui 적재·결속·창 자체 처리를 가르는 별도 창 대역: 관리자, egui 입력 큐, 창 자체 처리가 본 키 수,
    /// egui 에 실은 키 수.
    struct RouteWindow {
        popups: PopupManager,
        h: crate::view::window_input::tests::Harness,
        own: u32,
        loaded: u32,
    }

    /// 창을 덮는 팝업 선언(설정 창 충돌 질문, 소비 집합 Keys). `escape` 가 있으면 그 Esc 선언, `covers` 가 거짓이면
    /// 덮지 않는다.
    fn route_def(
        escape: Option<crate::adapters::ui::popup::PopupEscape>,
        covers: bool,
    ) -> crate::adapters::ui::popup::PopupDef {
        let mut def = crate::view::settings::ui::keybinding_conflict_def();
        if let Some(escape) = escape {
            def.decl.escape = escape;
        }
        def.decl.covers_scope = covers;
        def
    }

    impl RouteWindow {
        /// `def` 팝업을 포커스를 주며 연 창. None 이면 팝업이 없다.
        fn new(def: Option<crate::adapters::ui::popup::PopupDef>) -> Self {
            let mut popups = PopupManager::new();
            if let Some(def) = def {
                popups.register_def(&def, 1.0);
                let request = crate::intent::OpenRequest::new(
                    def.id,
                    crate::adapters::ui::popup::PopupScope::Window,
                )
                .at(egui::pos2(500.0, 400.0))
                .focused();
                assert!(popups.open_request(&request, true).is_some());
            }
            Self {
                popups,
                h: crate::view::window_input::tests::Harness::new(),
                own: 0,
                loaded: 0,
            }
        }

        /// 키 하나(`modifiers`, 창 자체 처리 바인딩에 맞으면 `own`)를 제품 단계(단계 4 → 적재 게이트 → egui 적재 →
        /// 결속 → 창 자체 처리)로 넘긴다.
        fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers, own: bool) -> ViewAction {
            use crate::adapters::ui::popup::frame_keys::Incoming;
            use crate::adapters::ui::popup::window_segments::WindowKey;
            use crate::view::main::input_segment::EguiPressSink;
            let route = crate::adapters::ui::input::key_route::RouteKey {
                key: Some(key),
                modifiers,
                super_held: false,
            };
            let incoming = if route.bare_escape() {
                Incoming::Escape
            } else {
                Incoming::Key
            };
            let mut steps = window_steps(
                self,
                None,
                true,
                move |w: &mut RouteWindow, key: &egui::Key, _, _| {
                    w.loaded += 1;
                    w.h.queued_events().push(egui::Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    });
                    Some((true, false))
                },
                |_| {},
                |w, _| {
                    w.own += 1;
                    ViewAction::Close
                },
                WindowKeyNode {
                    settle: move |w: &mut RouteWindow, _: &egui::Key| {
                        let ctx = w.h.ctx.clone();
                        let key = Some(WindowKey::Path { key: route, own });
                        settle_window_input(&mut w.h, &ctx, &mut w.popups, incoming, key)
                    },
                    bind: move |w: &mut RouteWindow, _: &egui::Key| {
                        let ctx = w.h.ctx.clone();
                        bind_window_input(
                            &ctx,
                            &mut w.popups,
                            incoming,
                            w.h.queued_events(),
                            false,
                        );
                    },
                    taps: |_: &mut RouteWindow, _: &egui::Key, _| None,
                },
            );
            run_key_event(&mut steps, key)
        }

        /// 지금 몫을 창 위젯이 받는지(`PopupManager::window_widget_takes_keys`).
        fn widget_takes_keys(&mut self) -> bool {
            let ctx = self.h.ctx.clone();
            self.popups.window_widget_takes_keys(&ctx)
        }
    }

    /// Given 창을 덮는 팝업(Keys)이 노드, When 그 팝업이 쓰지 않는 Ctrl+X 를 치면, Then 그 키는 egui 에 실리지 않고 창
    /// 자체 처리에도 가지 않는다.
    #[test]
    fn a_key_a_covering_popup_does_not_use_is_not_loaded_into_egui() {
        let mut window = RouteWindow::new(Some(route_def(None, true)));
        drop(window.key(egui::Key::X, egui::Modifiers::CTRL, false));
        assert_eq!((window.loaded, window.own), (0, 0));
    }

    /// Given 창을 덮는 팝업(Keys)이 노드, When 그 팝업이 쓰는 x 를 치면, Then 그 팝업 몫으로 egui 에 실린다.
    #[test]
    fn a_key_a_covering_popup_uses_is_loaded_into_egui() {
        let mut window = RouteWindow::new(Some(route_def(None, true)));
        drop(window.key(egui::Key::X, egui::Modifiers::NONE, false));
        assert_eq!((window.loaded, window.own), (1, 0));
        assert!(!window.widget_takes_keys());
    }

    /// Given 창을 덮는 팝업(Esc 선언 Ignore)이 노드, When bare Esc 를 치면, Then 그 Esc 는 위로 올라가 받는 노드가 없고
    /// egui 원시 입력에 실리지 않는다(위젯 포커스를 바꾸지 않는다).
    #[test]
    fn an_escape_an_ignoring_covering_popup_passes_up_is_not_loaded_into_egui() {
        let ignore = Some(crate::adapters::ui::popup::PopupEscape::Ignore);
        let mut window = RouteWindow::new(Some(route_def(ignore, true)));
        drop(window.key(egui::Key::Escape, egui::Modifiers::NONE, false));
        assert_eq!((window.loaded, window.own), (0, 0));
        assert!(
            crate::view::main::input_segment::EguiPressSink::queued_events(&mut window.h)
                .is_empty()
        );
    }

    /// Given 창을 덮지 않는 팝업(Esc 선언 Ignore)이 노드, When bare Esc 를 치면, Then 그 Esc 는 붙은 팝업에서 올라간 Esc 라
    /// 창 범위 노드의 내용(창 위젯)을 건너뛰고(`key_route::route_escape` 의 붙은 팝업 갈래) 받는 노드가 없다: egui 원시
    /// 입력에 실리지 않아 창 위젯이 받지 못하고 창 자체 처리에도 가지 않는다.
    #[test]
    fn a_bare_escape_an_ignoring_uncovering_popup_passes_up_skips_the_window_content() {
        let ignore = Some(crate::adapters::ui::popup::PopupEscape::Ignore);
        let mut window = RouteWindow::new(Some(route_def(ignore, false)));
        drop(window.key(egui::Key::Escape, egui::Modifiers::NONE, false));
        assert_eq!((window.loaded, window.own), (0, 0));
        assert!(
            crate::view::main::input_segment::EguiPressSink::queued_events(&mut window.h)
                .is_empty()
        );
    }

    /// Given 창을 덮지 않는 팝업(Esc 를 받는 선언)이 노드, When bare Esc 를 치면, Then 그 팝업이 받고 창 위젯 몫이 아니다.
    #[test]
    fn a_bare_escape_an_uncovering_popup_takes_stays_off_the_window_widget() {
        let mut window = RouteWindow::new(Some(route_def(None, false)));
        drop(window.key(egui::Key::Escape, egui::Modifiers::NONE, false));
        assert_eq!(window.own, 0);
        assert!(!window.widget_takes_keys());
    }

    /// Given 창을 덮는 팝업(Keys)이 노드, When 창 자체 처리 바인딩(engine level toggle_settings) Ctrl+F7 을 치면, Then
    /// 창 자체 처리가 창을 닫고 그 키는 팝업 몫으로 egui 에 실리지 않는다.
    #[test]
    fn an_engine_key_under_a_covering_popup_closes_without_loading_into_egui() {
        let mut window = RouteWindow::new(Some(route_def(None, true)));
        let action = window.key(egui::Key::F7, egui::Modifiers::CTRL, true);
        assert!(matches!(action, ViewAction::Close));
        assert_eq!((window.loaded, window.own), (0, 1));
    }

    /// Given 팝업이 없는 창, When 같은 Ctrl+F7 을 치면, Then 창 자체 처리가 창을 닫는다.
    #[test]
    fn an_engine_key_without_a_popup_closes_the_window() {
        let mut window = RouteWindow::new(None);
        let action = window.key(egui::Key::F7, egui::Modifiers::CTRL, true);
        assert!(matches!(action, ViewAction::Close));
        assert_eq!(window.own, 1);
    }

    /// Esc 누름 하나. winit 의 키 사건은 winit 밖에서 만들 수 없어 편집 판정이 읽는 누른 키만 가진다.
    #[derive(Clone, Debug, PartialEq)]
    struct EscPress(winit::keyboard::Key);

    impl crate::gfx::gpu::edit_keys::HeldEvent for EscPress {
        fn press(&self) -> Option<(&winit::keyboard::Key, winit::keyboard::PhysicalKey)> {
            let code = winit::keyboard::KeyCode::Escape;
            Some((&self.0, winit::keyboard::PhysicalKey::Code(code)))
        }
        fn queues_behind_hold(&self) -> bool {
            true
        }
        fn is_pointer(&self) -> bool {
            false
        }
        fn is_key(&self) -> bool {
            true
        }
        fn interrupts_await(&self) -> bool {
            false
        }
    }

    /// 복사 키를 Esc 에 둔(`copy = ["escape"]`) 별도 창. `managed` 면 Esc 를 받는 비덮음 팝업(설정 창 충돌 질문)이
    /// 포커스를 가진 팝업 관리자 창이고, 아니면 관리자가 없는 종료 확인 창이다. 창은 아직 패스를 돌리지 않은 누름
    /// (`pending`)을 가진다.
    struct EscWindow {
        popups: Option<PopupManager>,
        h: crate::view::window_input::tests::Harness,
        edits: crate::gfx::gpu::edit_keys::EditKeyState<EscPress>,
        pending: Vec<egui::Event>,
        own: u32,
        loaded: u32,
    }

    impl EscWindow {
        fn new(managed: bool) -> Self {
            let popups = managed.then(|| RouteWindow::new(Some(route_def(None, false))).popups);
            let settings = crate::settings::KeybindingSettings {
                copy: vec!["escape".into()],
                ..Default::default()
            };
            let mut edits = crate::gfx::gpu::edit_keys::EditKeyState::default();
            edits.set_bindings(Some(
                crate::adapters::ui::input::edit_key::EditBindings::from_settings(&settings),
            ));
            let press = egui::Event::PointerButton {
                pos: egui::pos2(5.0, 5.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            };
            Self {
                popups,
                h: crate::view::window_input::tests::Harness::new(),
                edits,
                pending: vec![press],
                own: 0,
                loaded: 0,
            }
        }

        /// 수식키 없는 Esc 누름 하나(`replayed` 는 보류가 풀려 다시 오는 키의 판정)를 제품 단계(단계 4 → 편집 판정
        /// `window_edit_target`·`window_key_judgement` → egui 적재 → 창 자체 처리, 창이 그 Esc 를 바인딩으로 쓴다)로
        /// 넘긴다.
        fn escape(&mut self, replayed: Option<EditKey>) -> ViewAction {
            use crate::adapters::ui::popup::frame_keys::Incoming;
            use crate::adapters::ui::popup::window_segments::WindowKey;
            use crate::gfx::gpu::edit_keys::{
                ClipText, EditKeyOwner, EguiKey, KeyCx, window_key_judgement,
            };
            let route = crate::adapters::ui::input::key_route::RouteKey {
                key: Some(egui::Key::Escape),
                modifiers: egui::Modifiers::NONE,
                super_held: false,
            };
            let mut steps = window_steps(
                self,
                replayed,
                true,
                |w: &mut EscWindow, event: &EscPress, judged, receiver| {
                    let routed = w.popups.is_some();
                    let edit_target = window_edit_target(Some(Incoming::Escape), receiver, routed);
                    let ctx = w.h.ctx.clone();
                    let (edits, events) = (&mut w.edits, &mut w.pending);
                    let mut clipboard = |_tag: u64| ClipText::Empty;
                    let judged = window_key_judgement(judged, edit_target, || {
                        let cx = KeyCx {
                            ctx: &ctx,
                            events,
                            clipboard: &mut clipboard,
                        };
                        edits.judge(event, EditKeyOwner::Window, std::time::Instant::now(), cx)
                    });
                    if judged == EguiKey::Taken {
                        return None;
                    }
                    w.loaded += 1;
                    Some((false, false))
                },
                |_| {},
                |w, _| {
                    w.own += 1;
                    ViewAction::Close
                },
                WindowKeyNode {
                    settle: move |w: &mut EscWindow, _: &EscPress| {
                        let ctx = w.h.ctx.clone();
                        let key = Some(WindowKey::Path {
                            key: route,
                            own: true,
                        });
                        let popups = w.popups.as_mut()?;
                        settle_window_input(&mut w.h, &ctx, popups, Incoming::Escape, key)
                    },
                    bind: |_: &mut EscWindow, _: &EscPress| {},
                    taps: |_: &mut EscWindow, _: &EscPress, _| None,
                },
            );
            run_key_event(
                &mut steps,
                EscPress(winit::keyboard::Key::Named(
                    winit::keyboard::NamedKey::Escape,
                )),
            )
        }

        /// 기다리던 누름을 받는 패스. 그 뒤 보류한 키를 다시 판정하고(`GpuState`와 같다), 풀린 키의 판정을 낸다.
        fn pass(&mut self) -> Option<EditKey> {
            use crate::gfx::gpu::edit_keys::{ClipText, EditKeyOwner, KeyCx};
            let pending = std::mem::take(&mut self.pending);
            self.edits.input_taken(&pending);
            let input = egui::RawInput {
                events: pending,
                ..Default::default()
            };
            let ctx = self.h.ctx.clone();
            let output = ctx.run(input, |_| {}).platform_output;
            // 이 창에는 복사할 선택이 없어 패스 출력에 남길 것이 없다.
            let _ = self.edits.after_pass(output, &ctx);
            if self.edits.needs_rejudge() {
                let mut clipboard = |_tag: u64| ClipText::Empty;
                let mut next = Vec::new();
                let cx = KeyCx {
                    ctx: &ctx,
                    events: &mut next,
                    clipboard: &mut clipboard,
                };
                self.edits.rejudge(cx, std::time::Instant::now());
            }
            let release = self.edits.take_release(EditKeyOwner::Window)?;
            release.split().0.map(|(_, edit)| edit)
        }
    }

    /// Given 복사 키가 Esc 이고 아직 패스를 돌리지 않은 누름이 있는 종료 확인 창(팝업 관리자 없음), When 수식키 없는
    /// Esc 를 치면, Then 편집 판정이 그 Esc 를 누름의 패스까지 보류해 egui 에도 창 자체 처리에도 가지 않고, 그 패스
    /// 뒤 풀린 Esc 가 창 자체 처리에 간다.
    #[test]
    fn a_bare_escape_in_the_quit_window_waits_for_the_pass_of_an_unhandled_press() {
        let mut window = EscWindow::new(false);
        drop(window.escape(None));
        assert_eq!((window.loaded, window.own), (0, 0));
        assert!(window.edits.busy());
        let replayed = window.pass();
        assert_eq!(replayed, Some(EditKey::Copy));
        drop(window.escape(replayed));
        assert_eq!((window.loaded, window.own), (1, 1));
    }

    /// Given 복사 키가 Esc 이고 아직 패스를 돌리지 않은 누름이 있는, Esc 를 받는 팝업이 노드인 관리자 창, When 수식키
    /// 없는 Esc 를 치면, Then 편집 판정을 건너 그 팝업의 Esc 처리로 바로 실리고 보류가 없다.
    #[test]
    fn a_bare_escape_in_a_managed_window_reaches_its_receiver_without_an_edit_hold() {
        let mut window = EscWindow::new(true);
        drop(window.escape(None));
        assert_eq!((window.loaded, window.own), (1, 0));
        assert!(!window.edits.busy());
    }
}
