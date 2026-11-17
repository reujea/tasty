//! 파일 피커가 남긴 결과를 꺼내 피커를 연 호출자에게 넘긴다. 피커와 이 전달 경로는
//! 파일을 열거나 탭을 만들거나 클립보드에 쓰지 않는다. 결과로 무엇을 할지는 각 호출자가 정한다.

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::core::CoreState;
use crate::state::{FilePickerOutcome, FilePickerRequester, MainViewState};
use crate::view::ui::View;

impl App {
    /// 현재 MainView의 결과만 처리하며 parked 상태는 순회하지 않는다.
    pub(crate) fn dispatch_pending_file_picker_results(&mut self) {
        let pending: Vec<winit::window::WindowId> = self
            .view
            .views
            .iter()
            .filter_map(|(id, w)| {
                let main = w.as_main()?;
                (!main.state.dialogs.finished_file_pickers.is_empty()).then_some(*id)
            })
            .collect();
        for id in pending {
            let Some((main, engine)) = engines_mut!(self).window_pair(id) else {
                continue;
            };
            let finished = take_outcomes(&mut main.state);
            for (outcome, requester) in &finished {
                deliver_outcome(
                    &mut main.state,
                    &engine,
                    self.plugin_manager.as_mut(),
                    requester.as_ref(),
                    outcome,
                );
            }
            main.mark_dirty();
        }
    }
}

/// 닫힌 피커들의 결과를 한 번 꺼내 출력함을 비운다.
pub(crate) fn take_outcomes(
    state: &mut MainViewState,
) -> Vec<(FilePickerOutcome, Option<FilePickerRequester>)> {
    std::mem::take(&mut state.dialogs.finished_file_pickers)
        .into_iter()
        .map(|f| (FilePickerOutcome::new(f.request_id, f.result), f.requester))
        .collect()
}

/// 시험용: 결과가 정해진 피커를 닫고(그리기가 Close를 돌려준 것처럼) 닫기 훅을 대신 실행한 뒤 결과 하나를 꺼낸다.
#[cfg(test)]
pub(crate) fn take_outcome(
    state: &mut MainViewState,
) -> Option<(FilePickerOutcome, Option<FilePickerRequester>)> {
    use crate::adapters::ui::popup::file_picker::instances;
    let settled: Vec<_> = instances::open_pickers(state)
        .filter(|(_, data)| data.result.is_some())
        .map(|(iid, _)| iid)
        .collect();
    for iid in settled {
        // intent-exempt: 시험 전용. 그리기가 Close 를 돌려준 프레임을 대신한다.
        state
            .popups
            .close_instance(iid, crate::adapters::ui::popup::PopupCloseReason::Content);
    }
    for closed in state.popups.take_closed_queue() {
        instances::finish(state, &closed);
    }
    let mut outcomes = take_outcomes(state);
    let first = (!outcomes.is_empty()).then(|| outcomes.remove(0));
    for (outcome, _) in &outcomes {
        tracing::warn!(
            request_id = outcome.request_id.0,
            "test take_outcome dropped an outcome"
        );
    }
    first
}

/// 결과를 호출자에게 넘긴다. 본체 호출자는 자기가 보관한 식별자와 같을 때만 적용하고,
/// 플러그인 요청이면 그 플러그인에 `file_picker.result`를 보낸다.
pub(crate) fn deliver_outcome(
    state: &mut MainViewState,
    engine: &CoreState,
    plugin_manager: Option<&mut crate::plugin::PluginManager>,
    requester: Option<&FilePickerRequester>,
    outcome: &FilePickerOutcome,
) {
    crate::adapters::ui::file_open_tool::apply_outcome(state, engine, outcome);
    crate::adapters::ui::new_tab_picker::apply_outcome(state, engine, outcome);
    if let Some(req) = requester {
        crate::adapters::ipc::handler::file_picker::deliver_result(plugin_manager, req, outcome);
    }
}

#[cfg(test)]
#[path = "file_picker_tests.rs"]
mod tests;
