//! 파일 핸들러 선택 결과를 처리한다.

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::view::ui::View;

impl App {
    /// MainView의 결과만 처리하며 parked 상태는 순회하지 않는다.
    pub(crate) fn dispatch_pending_picker_results(&mut self) {
        let pending: Vec<winit::window::WindowId> = self
            .view
            .views
            .iter()
            .filter_map(|(id, w)| {
                let main = w.as_main()?;
                (!main.state.dialogs.finished_handler_pickers.is_empty()).then_some(*id)
            })
            .collect();
        for id in pending {
            let Some((main, _)) = engines_mut!(self).window_pair(id) else {
                continue;
            };
            let finished = std::mem::take(&mut main.state.dialogs.finished_handler_pickers);
            for data in finished {
                self.apply_finished_picker(id, data);
            }
        }
    }

    /// 닫힌 선택창 하나의 결과를 실행한다.
    fn apply_finished_picker(
        &mut self,
        id: winit::window::WindowId,
        mut data: crate::state::FileHandlerPickerData,
    ) {
        let core = &mut self.services;
        let Some((main, mut engine)) = engines_mut!(self).window_pair(id) else {
            return;
        };
        let Some(result) = data.result.take() else {
            return;
        };
        let target = data.target.clone();
        let origin_surface_id = data.origin_surface_id;
        let dispatch_origin = data.dispatch_origin;
        let ignore_size_limit = data.ignore_size_limit;
        let result_place = data.result_place;
        let run = data.run.take().map(|guard| guard.take());
        // 설정 창 열기는 ActiveEventLoop가 필요해 App에서 처리한다.
        if matches!(result, crate::state::FileHandlerPickerResult::OpenSettings) {
            crate::file::dispatch::finish_run(
                run,
                crate::request_runs::RunState::Cancelled,
                "the file handler picker closed to open settings",
                serde_json::Value::Null,
            );
            self.state.pending_settings_file_handler_tab = true;
            crate::shortcuts::send_app_event(&self.view.proxy, crate::AppEvent::OpenSettings);
        } else {
            crate::file::dispatch::apply_file_picker_result(
                core,
                &mut main.state,
                &mut engine,
                target,
                result,
                origin_surface_id,
                crate::file::dispatch::FileDispatchPolicy {
                    dispatch_origin,
                    ignore_size_limit,
                    run,
                    result_place,
                },
            );
        }
        main.mark_dirty();
    }
}
