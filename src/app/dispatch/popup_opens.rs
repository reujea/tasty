//! 도구 메뉴의 popup 열기 요청과 생성 대기 초안에 걸어 둔 명령을 플러그인 매니저로 전달한다.

use crate::app::App;
use crate::plugin::PluginManager;
use crate::runtime::engine_access::EngineMut;
use crate::state::MainViewState;

impl App {
    pub(crate) fn dispatch_pending_popup_opens(&mut self) {
        open_pending_popups(
            self.plugin_manager.as_mut(),
            crate::app::window_access::engines_mut!(self).sessions(),
        );
    }
}

/// 세션마다 생성이 끝난 초안에 걸어 둔 명령을 실행하고, 쌓인 팝업 열기 요청을 연다.
/// 요청은 입력이 일어난 세션의 큐에 있으므로 세션 단위로 처리한다.
pub(crate) fn open_pending_popups<'a>(
    mut mgr: Option<&mut PluginManager>,
    sessions: impl Iterator<Item = (&'a mut MainViewState, EngineMut<'a>)>,
) {
    for (state, engine) in sessions {
        // 생성이 끝난 초안에 걸어 둔 명령을 먼저 실행해 그 팝업 요청도 이번에 연다.
        let notices = crate::adapters::ui::tools_menu::run_deferred_tools(state, &engine.read());
        let drained = std::mem::take(&mut state.pending_popup_opens);
        let Some(mgr) = mgr.as_deref_mut() else {
            continue;
        };
        for notice in notices {
            crate::plugin_bridge::key_dispatch::emit_command_invoked(
                mgr,
                &notice.plugin_id,
                &notice.command_id,
                notice.surface_id,
            );
        }
        for open in drained {
            open_one(state, mgr, open);
        }
    }
}

/// 요청을 연다. 창 범위 팝업은 요청이 쌓인 창, 곧 명령을 입력한 창에 결속한다.
fn open_one(
    state: &mut MainViewState,
    mgr: &mut PluginManager,
    open: crate::state::PendingPopupOpen,
) {
    // 큐에 넣는 쪽이 먼저 거르지만, 큐에 넣은 뒤 선언이 바뀐 경우를 위해 여기서도 거른다.
    if crate::plugin_bridge::popup_scope::lacks_required_target(
        crate::plugin_bridge::popup_scope::declared_popup_scope(
            mgr,
            &open.plugin_id,
            &open.popup_id,
        ),
        open.target_surface,
    ) {
        tracing::warn!(
            "popup '{}/{}' not opened: it needs a surface target and none is focused",
            open.plugin_id,
            open.popup_id
        );
        return;
    }
    let was_open = crate::plugin_bridge::popup_scope::is_open(mgr, &open.plugin_id, &open.popup_id);
    let Some(instance_id) = mgr.open_popup_instance(&open.plugin_id, &open.popup_id, open.context)
    else {
        return;
    };
    crate::plugin_bridge::popup_scope::bind_window_popup(state, mgr, instance_id, was_open);
    // 대상 surface 를 결속한다. 범위는 선언과 결속 대상으로 렌더 경로가 정한다.
    if let Some(sid) = open.target_surface {
        mgr.bind_popup_instance_surface(instance_id, sid);
    }
    // 이 큐는 사용자가 이 창에서 고른 명령(도구 메뉴·명령 팔레트·단축키·변환 입력)이 채운다.
    crate::plugin_bridge::popup_scope::focus_opened_popup(
        state,
        mgr,
        instance_id,
        crate::plugin_bridge::popup_scope::PopupOpener::UserCommand,
    );
}

#[cfg(test)]
#[path = "popup_opens_tests.rs"]
mod tests;
