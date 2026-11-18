//! Tools 메뉴와 단축키의 "파일 열기". 파일 피커를 열고, 돌려받은 경로를 파일 핸들러로 연다.
//! 원격 목록에서 고른 경로는 원격 탐색기에서 연 것처럼 그 mirror workspace의 surface를 출발점으로 연다.

use crate::adapters::ui::popup::file_picker;
use crate::app::command::DomainIntent;
use crate::core::CoreState;
use crate::runtime::engine_read::EngineRead;
use crate::state::{
    FilePickerLocation, FilePickerOutcome, FilePickerRequestId, MainViewState,
    OpenFilePickerRequest,
};

/// 포커스 surface의 소속, 없으면 활성 workspace를 둘러보는 피커를 연다. 위치는 결과를 해석하려고 보관한다.
pub(crate) fn open(state: &mut MainViewState, engine: &EngineRead<'_>) {
    let surface = state.focused_surface_id(engine);
    let workspace = state.active_workspace_index(engine);
    open_at(state, engine, surface, workspace);
}

/// 명령 대상 `surface` 의 소속, 없으면 `workspace` 번째 workspace 를 둘러보는 피커를 연다.
pub(crate) fn open_at(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    surface: Option<u32>,
    workspace: usize,
) {
    let location = file_picker::locate(state, engine, surface, workspace, None);
    let request_id = FilePickerRequestId::next();
    if file_picker::open(
        state,
        engine,
        request_id,
        location.clone(),
        file_picker::FilePickerOptions::default(),
        file_picker::FilePickerOpenedBy::ToolsMenu,
        // 창 기능이다. 포커스 surface 는 시작 위치만 정한다.
        crate::model::popup_kind::PopupScope::Window,
    )
    .is_err()
    {
        report_picker_busy(state);
        return;
    }
    state.dialogs.file_picker_callers.open_file = Some(OpenFilePickerRequest {
        request_id,
        location,
    });
}

/// 본체 사용자 조작에만 중복 거절을 알린다. IPC는 busy 응답으로 호출자에게 돌려준다.
pub(super) fn report_picker_busy(state: &mut MainViewState) {
    state.toasts.push_info(
        crate::i18n::t("filepicker.already_open"),
        crate::adapters::ui::ToastScope::Window,
    );
}

/// 보관한 요청과 같은 식별자의 결과만 한 번 적용한다. 취소면 아무것도 열지 않는다.
pub(crate) fn apply_outcome(
    state: &mut MainViewState,
    engine: &CoreState,
    outcome: &FilePickerOutcome,
) {
    let callers = &mut state.dialogs.file_picker_callers;
    if callers.open_file.as_ref().map(|r| r.request_id) != Some(outcome.request_id) {
        return;
    }
    let Some(request) = callers.open_file.take() else {
        return;
    };
    if outcome.cancelled {
        return;
    }
    let origin_surface_id = match request.location {
        FilePickerLocation::Local { .. } => None,
        FilePickerLocation::Remote {
            mirror_ws_id,
            surface_id,
            ..
        } => match remote_origin(engine, mirror_ws_id, surface_id) {
            Some(sid) => Some(sid),
            None => {
                tracing::warn!(
                    mirror_ws_id,
                    "file_picker: mirror workspace closed before the picked remote file opened; ignored"
                );
                return;
            }
        },
    };
    for path in &outcome.paths {
        state.dispatch_intent(
            DomainIntent::DispatchFile {
                target: crate::file::format::FileTarget::new(path.clone()),
                depth: crate::file::format::DetectDepth::Deep,
                origin_surface_id,
                dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                ignore_size_limit: false,
                run: None,
            }
            .from_user_menu("file_picker_confirm"),
        );
    }
}

/// 원격 파일을 열 출발 surface. 보관한 surface가 피커가 열린 동안 닫혔으면 같은 mirror workspace의
/// 다른 surface를 쓴다. workspace가 사라졌으면 None이다.
fn remote_origin(engine: &CoreState, mirror_ws_id: u32, surface_id: Option<u32>) -> Option<u32> {
    let in_workspace = |sid: u32| {
        engine
            .find_workspace_index_for_surface(sid)
            .and_then(|(idx, _)| engine.workspace_at(idx))
            .is_some_and(|ws| ws.id == mirror_ws_id && ws.mirror)
    };
    if let Some(sid) = surface_id.filter(|sid| in_workspace(*sid)) {
        return Some(sid);
    }
    let ws = engine.workspace_at(engine.find_workspace_index_for_id(mirror_ws_id)?)?;
    ws.all_surface_ids()
        .into_iter()
        .find(|sid| in_workspace(*sid))
}

#[cfg(test)]
#[path = "file_open_tool_tests.rs"]
mod tests;
