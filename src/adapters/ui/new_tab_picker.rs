//! 탭 스트립 메뉴의 "새 HTML..."처럼 고른 파일로 탭을 만드는 호출자. 피커가 돌려준 경로로
//! 메뉴를 연 pane에 탭을 만든다.

use crate::adapters::ui::popup::file_picker;
use crate::app::command::DomainIntent;
use crate::core::CoreState;
use crate::runtime::engine_read::EngineRead;
use crate::state::{
    FilePickerLocation, FilePickerOutcome, FilePickerRequestId, MainViewState, NewTabPickerRequest,
};

/// 고른 파일로 `pane_id` pane에 `kind` 탭을 만드는 피커를 연다. 메뉴를 연 pane의
/// 선택 탭 cwd에서 출발한다. 지원 연결의 HTML은 원격 목록을 둘러보고 원 pane에 생성한다.
pub(crate) fn open(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    pane_id: u32,
    kind: &str,
    param_key: &str,
    extensions: &[&str],
) {
    let workspace = engine
        .find_workspace_index_for_pane(pane_id)
        .and_then(|idx| engine.workspace_at(idx));
    let remote = workspace.filter(|workspace| workspace.mirror);
    // 원격 파일은 원격 문서 viewer로 연다. 그 kind가 두 능력을 모두 선언해야 한다.
    if remote.is_some_and(|workspace| {
        !engine.html_entry_available(workspace.id)
            || engine
                .surface_registry
                .get_live(kind)
                .is_none_or(|def| !(def.capabilities.opens_url && def.capabilities.mirror_viewer))
    }) {
        state.toasts.push(
            crate::i18n::t("explorer.state.remote_open_unsupported").to_string(),
            crate::adapters::ui::ToastKind::Info,
            crate::adapters::ui::ToastScope::Window,
        );
        return;
    }
    let source = engine
        .find_pane_by_id(pane_id)
        .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
        .and_then(|tab| state.navigation.surface_id(tab));
    let cwd = source.and_then(|sid| engine.surface_cwd(sid));
    let mirror = remote.and_then(|workspace| {
        engine
            .mirror_projection_token(workspace.id)
            .map(|token| (workspace.id, token))
    });
    if remote.is_some() && mirror.is_none() {
        return;
    }
    let location = if let Some(workspace) = remote {
        // A remote picker never interprets the server's working directory on the client.
        let dir = cwd.and_then(|cwd| match cwd {
            crate::core::state::SurfaceCwd::Remote(path) => Some(path.as_str().to_owned()),
            _ => None,
        });
        FilePickerLocation::Remote {
            mirror_ws_id: workspace.id,
            surface_id: source,
            dir,
        }
    } else {
        let dir = cwd.and_then(|cwd| match cwd {
            crate::core::state::SurfaceCwd::Local(path) => {
                Some(path.to_string_lossy().into_owned())
            }
            _ => None,
        });
        FilePickerLocation::Local { dir }
    };
    let request_id = FilePickerRequestId::next();
    if file_picker::open(
        state,
        engine,
        request_id,
        location,
        file_picker::FilePickerOptions {
            filters: extensions.iter().map(|e| e.to_string()).collect(),
            ..Default::default()
        },
        file_picker::FilePickerOpenedBy::ContextMenu,
        // 고른 파일로 이 pane 에 새 탭을 만든다.
        crate::model::popup_kind::PopupScope::Pane(pane_id),
    )
    .is_err()
    {
        super::file_open_tool::report_picker_busy(state);
        return;
    }
    state
        .dialogs
        .file_picker_callers
        .new_tab
        .push(NewTabPickerRequest {
            mirror,
            request_id,
            pane_id,
            kind: kind.to_string(),
            param_key: param_key.to_string(),
        });
}

/// 보관한 요청과 같은 식별자의 결과만 한 번 적용한다. 경로마다 탭을 하나씩 만든다.
/// 취소했거나 피커가 열린 동안 pane이 닫혔으면 아무것도 만들지 않는다.
pub(crate) fn apply_outcome(
    state: &mut MainViewState,
    engine: &CoreState,
    outcome: &FilePickerOutcome,
) {
    let callers = &mut state.dialogs.file_picker_callers;
    let Some(index) = callers
        .new_tab
        .iter()
        .position(|r| r.request_id == outcome.request_id)
    else {
        return;
    };
    let request = callers.new_tab.remove(index);
    if outcome.cancelled
        || !engine.has_pane(request.pane_id)
        || request
            .mirror
            .as_ref()
            .is_some_and(|(workspace, token)| !engine.matches_mirror_projection(*workspace, token))
    {
        return;
    }
    for path in &outcome.paths {
        state.dispatch_intent(
            DomainIntent::CreateTab {
                pane_id: request.pane_id,
                cwd: None,
                cwd_source: None,
                kind: request.kind.clone(),
                name: None,
                surface_params: serde_json::json!({ request.param_key.as_str(): path }),
                activate: true,
            }
            .from_user_context_menu(),
        );
    }
}

#[cfg(test)]
#[path = "new_tab_picker_tests.rs"]
mod tests;
