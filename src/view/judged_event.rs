//! 자체 키 처리를 가진 egui 전용 창(설정·종료 확인·Preset·Plugins)이 창 이벤트 하나를 편집 키 판정, egui,
//! 창 자체 처리 순서로 넘기는 공통 흐름. 판정이 가져간 이벤트는 창 자체 처리에 가지 않는다. 팝업 관리자를 가진 창은
//! 판정 앞에서 사건 시점 노드로 입력을 끊고(`window_input::settle_window_input`), egui 에 실은 키의 몫을 그 노드로
//! 묶는다. 계약: `docs/design/policies/key-mapping.md`의 "보류와 재생".

use winit::event::WindowEvent;

use super::ViewAction;
use super::key_event::{KeyEventSteps, run_key_event};
use super::ui::View;
use super::window_input::{bind_window_input, incoming_of, settle_window_input};
use crate::adapters::ui::input::edit_key::EditKey;
use crate::adapters::ui::popup::PopupManager;

/// 별도 창의 GPU 와 팝업 관리자. 관리자가 없는 창(종료 확인)은 `None` 이고 노드는 창 위젯 하나다.
pub(crate) type WindowInputOf<V> =
    fn(&mut V) -> Option<(&mut crate::gpu::GpuState, &mut PopupManager)>;

/// 창 이벤트 하나. `to_egui`면 편집 키 판정을 거쳐 egui에 넘긴다. 판정이 가져간 이벤트는 다시 그리기만 요청하고
/// `own`(창 자체 처리)에 주지 않는다. 보류가 풀리면 `super::replay_edit_keys`가 다시 이 함수로 보낸다.
/// `judged`는 그때 원래 키가 판정받은 편집이다. `input` 은 끊기와 결속에 쓰는 창의 GPU 와 관리자다.
pub(crate) fn judged_window_event<V: View>(
    view: &mut V,
    event: WindowEvent,
    judged: Option<EditKey>,
    to_egui: bool,
    own: fn(&mut V, WindowEvent) -> ViewAction,
    input: WindowInputOf<V>,
) -> ViewAction {
    let node = WindowKeyNode {
        settle: |view: &mut V, event: &WindowEvent| {
            let mods = view.base_mut().state.modifiers;
            if let Some((gpu, popups)) = input(view)
                && let Some(incoming) = incoming_of(event, mods, gpu.egui_modifiers())
            {
                let ctx = gpu.egui_ctx.clone();
                settle_window_input(gpu, &ctx, popups, incoming);
            }
        },
        bind: |view: &mut V, event: &WindowEvent| {
            let mods = view.base_mut().state.modifiers;
            if let Some((gpu, popups)) = input(view)
                && let Some(incoming) = incoming_of(event, mods, gpu.egui_modifiers())
            {
                bind_window_input(&gpu.egui_ctx, popups, incoming);
            }
        },
    };
    after_edit_judgment(
        view,
        event,
        judged,
        to_egui,
        |view, event, judged| {
            let base = view.base_mut();
            base.gpu
                .handle_egui_event_for_window(&base.winit, event, judged)
        },
        |view| view.mark_dirty(),
        own,
        node,
    )
}

/// 별도 창 단계 3~5(`settle`)와 egui 에 실은 키의 몫 결속(`bind`).
struct WindowKeyNode<S, B> {
    settle: S,
    bind: B,
}

/// [`judged_window_event`]의 분기. `egui`는 판정과 egui 처리 결과이며 `None`이면 판정이 이벤트를 가져갔다.
/// 단계 순서는 메인 창과 같은 [`run_key_event`]가 정한다. 별도 창이 가진 단계는 편집 판정과 egui 적재(한 호출),
/// 창 자체 처리뿐이다.
fn after_edit_judgment<V, E>(
    view: &mut V,
    event: E,
    judged: Option<EditKey>,
    to_egui: bool,
    egui: impl FnOnce(&mut V, &E, Option<EditKey>) -> Option<(bool, bool)>,
    mark_dirty: impl FnMut(&mut V),
    own: impl FnOnce(&mut V, E) -> ViewAction,
    node: WindowKeyNode<impl FnMut(&mut V, &E), impl FnMut(&mut V, &E)>,
) -> ViewAction {
    let mut steps = JudgedWindowSteps {
        view,
        judged,
        to_egui,
        egui: Some(egui),
        mark_dirty,
        own: Some(own),
        node,
    };
    run_key_event(&mut steps, event)
}

/// 별도 창의 단계. 편집 판정은 egui 적재와 같은 GPU 호출 안에 있어 단계 6은 받은 표시를 그대로 넘긴다.
struct JudgedWindowSteps<'v, V, G, D, O, S, B> {
    view: &'v mut V,
    judged: Option<EditKey>,
    to_egui: bool,
    egui: Option<G>,
    mark_dirty: D,
    own: Option<O>,
    node: WindowKeyNode<S, B>,
}

impl<V, E, G, D, O, S, B> KeyEventSteps<E> for JudgedWindowSteps<'_, V, G, D, O, S, B>
where
    G: FnOnce(&mut V, &E, Option<EditKey>) -> Option<(bool, bool)>,
    D: FnMut(&mut V),
    O: FnOnce(&mut V, E) -> ViewAction,
    S: FnMut(&mut V, &E),
    B: FnMut(&mut V, &E),
{
    fn settle_key_node(&mut self, event: &E) {
        (self.node.settle)(self.view, event);
    }

    fn judge_edit_key(&mut self, _event: &E) -> Option<Option<EditKey>> {
        Some(self.judged)
    }

    fn feed_egui(&mut self, event: &E, judged: Option<EditKey>) -> Option<(bool, bool)> {
        if !self.to_egui {
            return Some((false, false));
        }
        let egui = self.egui.take()?;
        let fed = egui(self.view, event, judged);
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
        match self.own.take() {
            Some(own) => own(self.view, event),
            None => ViewAction::None,
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
        after_edit_judgment(
            window,
            "alt+c",
            None,
            to_egui,
            |window: &mut Window, _, _| {
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
                settle: |window: &mut Window, _: &&str| window.steps.push("settle"),
                bind: |window: &mut Window, _: &&str| window.steps.push("bind"),
            },
        )
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
}
