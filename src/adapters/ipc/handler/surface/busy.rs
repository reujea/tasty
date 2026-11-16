//! `surface.set_busy` — surface가 실행 중이라고 알리는 입구. plugin surface는 소유 plugin이,
//! 터미널은 셸·plugin 누구나 알린다(docs/design/policies/busy-indicator.md (P)·(R)).

use serde_json::json;
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;

/// 이 surface를 소유해 busy를 알릴 수 있는 로컬 plugin의 id. 소유자가 없으면 `None`이다.
///
/// plugin이 그리는 surface는 rendering 방식에 따라 다른 타입이 된다 — `remote`·`webview`는
/// [`RemoteSurface`](crate::plugin_bridge::remote_surface::RemoteSurface), `egui-mesh`는
/// [`EguiMeshSurface`](crate::runtime::egui_mesh_surface::EguiMeshSurface). 둘 다 그 plugin이
/// 그리므로 전경 프로세스로 판단할 수 없고, 같은 입구를 쓴다.
///
/// 호스트 builtin(`terminal`·`empty`·`explorer`·`dag_graph`)과 복원 대기 placeholder 에는
/// 소유자가 없다. `AttachMeshSurface`는 원격 호스트의 surface를 비추기만 하고 그 `plugin_id`로
/// 로컬 plugin을 조회하지 않으므로 여기서도 소유자로 보지 않는다.
pub(crate) fn owning_plugin(surface: &dyn tasty_model::Surface) -> Option<&str> {
    let any = surface.as_any();
    if let Some(rs) = any.downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>() {
        return Some(&rs.plugin_id);
    }
    any.downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
        .map(|m| m.plugin_id.as_str())
}

/// surface의 busy를 켜거나 끈다. 탭 점·사이드바 점과 개수·`surface.list`의 `busy`가 같은 판정을
/// 읽는다. plugin surface는 소유 plugin만, 터미널은 누구나 바꿀 수 있고 나머지 surface는 거절한다.
pub(crate) fn handle_set_busy(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let Some(busy) = params.get("busy").and_then(|v| v.as_bool()) else {
        return JsonRpcResponse::invalid_params(id, "missing boolean 'busy'");
    };
    if engine.runtime.terminals.get(surface_id).is_none() {
        let Some(surface) = engine.find_surface_by_id(surface_id) else {
            return JsonRpcResponse::error(id, -32000, "surface_id not found");
        };
        let Some(owner_id) = owning_plugin(surface) else {
            return JsonRpcResponse::error(
                id,
                -32000,
                "surface is neither a terminal nor a plugin surface",
            );
        };
        let owner = matches!(
            caller,
            tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } if plugin_id == owner_id
        );
        if !owner {
            tracing::warn!(
                "surface.set_busy: surface {surface_id} refused: caller does not own it"
            );
            return JsonRpcResponse::error(id, -32000, "caller does not own this surface");
        }
    }
    engine.set_reported_surface_busy(surface_id, busy);
    JsonRpcResponse::success(
        id,
        json!({ "ok": true, "surface_id": surface_id, "busy": busy }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    fn plugin(plugin_id: &str) -> tasty_ipc::caller::CallerContext {
        tasty_ipc::caller::CallerContext::Plugin {
            plugin_id: plugin_id.to_string(),
        }
    }

    fn set(
        e: &mut crate::runtime::engine_access::EngineMut<'_>,
        caller: &tasty_ipc::caller::CallerContext,
        sid: u32,
        busy: bool,
    ) -> JsonRpcResponse {
        handle_set_busy(
            e,
            caller,
            json!(1),
            &json!({ "surface_id": sid, "busy": busy }),
        )
    }

    fn add_plugin_surface(e: &mut crate::runtime::engine_access::EngineMut<'_>, sid: u32) {
        e.runtime.surfaces.insert(
            sid,
            Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
                sid,
                "chat_kind",
                "com.example.chat".into(),
                "chat".into(),
            )),
        );
    }

    // 소유 plugin이 켠 값은 판정에 들어가고, 끄면 빠진다. 같은 값을 두 번 보내도 결과가 같다.
    #[test]
    fn the_owner_turns_its_surface_busy_on_and_off() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        add_plugin_surface(&mut e, 7);
        let me = plugin("com.example.chat");
        assert!(set(&mut e, &me, 7, true).error.is_none());
        assert!(set(&mut e, &me, 7, true).error.is_none());
        assert!(e.read().is_surface_busy(7));
        assert_eq!(e.read().busy_count(&[7, 8]), 1);
        assert!(set(&mut e, &me, 7, false).error.is_none());
        assert!(!e.read().is_surface_busy(7));
    }

    // 다른 plugin·로컬 CLI·없는 surface·인자 누락은 거절하고 값을 바꾸지 않는다.
    #[test]
    fn other_callers_and_bad_targets_are_refused() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        add_plugin_surface(&mut e, 7);
        assert!(
            set(&mut e, &plugin("com.example.other"), 7, true)
                .error
                .is_some()
        );
        assert!(
            set(&mut e, &tasty_ipc::caller::CallerContext::local(), 7, true)
                .error
                .is_some()
        );
        assert!(
            set(&mut e, &plugin("com.example.chat"), 99, true)
                .error
                .is_some()
        );
        let missing = handle_set_busy(
            &mut e,
            &plugin("com.example.chat"),
            json!(1),
            &json!({ "surface_id": 7 }),
        );
        assert!(missing.error.is_some());
        assert!(!e.read().is_surface_busy(7));
    }

    // 터미널은 소유자가 없다 — 셸(로컬 CLI)과 plugin 모두 알릴 수 있고, 판정은 전경 판단과 합집합이다.
    #[test]
    fn anyone_reports_a_terminal_busy() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        e.runtime
            .terminals
            .insert(5, tasty_terminal::Terminal::new_detached(80, 24), None);
        let shell = tasty_ipc::caller::CallerContext::local();
        assert!(set(&mut e, &shell, 5, true).error.is_none());
        assert!(e.read().is_surface_busy(5));
        assert!(
            set(&mut e, &plugin("com.example.hook"), 5, false)
                .error
                .is_none()
        );
        assert!(!e.read().is_surface_busy(5));
    }

    fn add_egui_mesh_surface(
        e: &mut crate::runtime::engine_access::EngineMut<'_>,
        sid: u32,
        plugin_id: &str,
    ) {
        e.runtime.surfaces.insert(
            sid,
            Box::new(crate::runtime::egui_mesh_surface::EguiMeshSurface::new(
                sid,
                "image",
                plugin_id.into(),
                "image".into(),
                None,
            )),
        );
    }

    // egui-mesh plugin surface 도 그 plugin 이 그리므로 소유자가 알릴 수 있다. webview 만 받으면
    // `com.tasty.image` 같은 plugin 이 자기 surface 의 busy 를 켜지 못한다.
    #[test]
    fn an_egui_mesh_owner_reports_its_own_surface() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        add_egui_mesh_surface(&mut e, 11, "com.tasty.image");
        let owner = plugin("com.tasty.image");
        assert!(set(&mut e, &owner, 11, true).error.is_none());
        assert!(e.read().is_surface_busy(11));
        // 다른 plugin 은 egui-mesh surface 도 못 바꾼다.
        assert!(
            set(&mut e, &plugin("com.example.other"), 11, false)
                .error
                .is_some()
        );
        assert!(e.read().is_surface_busy(11));
    }

    // 소유 plugin 이 없는 호스트 builtin surface 는 거절한다 — 두 거절 사유가 구분된다.
    #[test]
    fn a_host_builtin_surface_has_no_owner() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        e.runtime
            .surfaces
            .insert(21, Box::new(crate::model::EmptySurface::new(21)));
        let refusal = set(&mut e, &plugin("com.example.any"), 21, true);
        let message = refusal.error.expect("refused").message;
        assert!(
            message.contains("neither a terminal nor a plugin surface"),
            "소유자 없음이 아니라 다른 사유로 거절했다: {message}"
        );
        assert!(!e.read().is_surface_busy(21));
    }

    // 폴링은 터미널 busy 집합을 통째로 바꾼다. plugin 값이 그때 지워지면 점이 1초 만에 꺼진다.
    // surface가 닫히면 값도 지워야 턴 도중 닫힌 탭이 워크스페이스를 busy로 남기지 않는다.
    #[test]
    fn the_value_survives_the_poll_and_goes_with_the_surface() {
        let mut s = engine();
        let mut e = s.borrow_mut();
        add_plugin_surface(&mut e, 7);
        add_plugin_surface(&mut e, 8);
        let me = plugin("com.example.chat");
        set(&mut e, &me, 7, true);
        set(&mut e, &me, 8, true);
        e.refresh_busy_surfaces();
        assert!(e.read().is_surface_busy(7), "폴링이 plugin 값을 지웠다");
        e.cleanup_surface_observations(7);
        assert!(!e.read().is_surface_busy(7), "닫힌 surface의 값이 남았다");
        assert!(e.read().is_surface_busy(8), "다른 surface 값까지 지웠다");
    }
}
