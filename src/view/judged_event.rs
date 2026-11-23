//! 자체 키 처리를 가진 egui 전용 창(설정·종료 확인·Preset·Plugins)이 창 이벤트 하나를 편집 키 판정, egui,
//! 창 자체 처리 순서로 넘기는 공통 흐름. 판정이 가져간 이벤트는 창 자체 처리에 가지 않는다. 계약:
//! `docs/design/policies/key-mapping.md`의 "보류와 재생".

use winit::event::WindowEvent;

use super::ViewAction;
use super::key_event::{KeyEventSteps, run_key_event};
use super::ui::View;
use crate::adapters::ui::input::edit_key::EditKey;

/// 창 이벤트 하나. `to_egui`면 편집 키 판정을 거쳐 egui에 넘긴다. 판정이 가져간 이벤트는 다시 그리기만 요청하고
/// `own`(창 자체 처리)에 주지 않는다. 보류가 풀리면 `super::replay_edit_keys`가 다시 이 함수로 보낸다.
/// `judged`는 그때 원래 키가 판정받은 편집이다.
pub(crate) fn judged_window_event<V: View>(
    view: &mut V,
    event: WindowEvent,
    judged: Option<EditKey>,
    to_egui: bool,
    own: fn(&mut V, WindowEvent) -> ViewAction,
) -> ViewAction {
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
    )
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
) -> ViewAction {
    let mut steps = JudgedWindowSteps {
        view,
        judged,
        to_egui,
        egui: Some(egui),
        mark_dirty,
        own: Some(own),
    };
    run_key_event(&mut steps, event)
}

/// 별도 창의 단계. 편집 판정은 egui 적재와 같은 GPU 호출 안에 있어 단계 6은 받은 표시를 그대로 넘긴다.
struct JudgedWindowSteps<'v, V, G, D, O> {
    view: &'v mut V,
    judged: Option<EditKey>,
    to_egui: bool,
    egui: Option<G>,
    mark_dirty: D,
    own: Option<O>,
}

impl<V, E, G, D, O> KeyEventSteps<E> for JudgedWindowSteps<'_, V, G, D, O>
where
    G: FnOnce(&mut V, &E, Option<EditKey>) -> Option<(bool, bool)>,
    D: FnMut(&mut V),
    O: FnOnce(&mut V, E) -> ViewAction,
{
    fn judge_edit_key(&mut self, _event: &E) -> Option<Option<EditKey>> {
        Some(self.judged)
    }

    fn feed_egui(&mut self, event: &E, judged: Option<EditKey>) -> Option<(bool, bool)> {
        if !self.to_egui {
            return Some((false, false));
        }
        let egui = self.egui.take()?;
        let fed = egui(self.view, event, judged);
        if fed.is_none() {
            (self.mark_dirty)(self.view);
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
                egui
            },
            |window| window.dirty += 1,
            |window, event| {
                window.own.push(event);
                ViewAction::Close
            },
        )
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
