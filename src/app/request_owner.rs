//! GUI 요청의 대상 자원을 가진 창을 찾는다. 자원 ID 해석은 core::request_target을 공유한다.

use crate::app::App;
use crate::core::request_target::request_resource_id;
use winit::window::WindowId;

/// 아래 메서드의 single_parent 폴백은 한 창 안에서만 유일성을 확인한다.
/// 대상 ID 없이 여러 창 중 하나를 골라야 하면 --surface를 요구한다.
/// 이미 ID가 지정됐다면 이 오류로 가로채지 않고 라우터가 대상 부재를 알리게 한다.
fn ambiguous_parent_fallback_requires_surface(
    method: &str,
    main_window_count: usize,
    target_named: bool,
) -> bool {
    !target_named
        && main_window_count > 1
        && matches!(
            method,
            "terminal.kill"
                | "terminal.release"
                | "terminal.respawn"
                | "terminal.broadcast"
                | "terminal.set_nickname"
        )
}

/// External 부모는 어느 창에도 속하지 않으므로 창이 여럿이면 `workspace`로 창을 고르게 한다.
/// 부모 생략 폴백이 없는 `terminal.children`도 포커스 창으로 새지 않도록 포함한다.
fn external_parent_requires_workspace(
    method: &str,
    main_window_count: usize,
    params: &serde_json::Value,
) -> bool {
    main_window_count > 1
        && params.get("parent_external").is_some_and(|v| !v.is_null())
        && matches!(
            method,
            "terminal.children"
                | "terminal.kill"
                | "terminal.release"
                | "terminal.respawn"
                | "terminal.broadcast"
                | "terminal.set_nickname"
        )
}

impl App {
    /// 한 요청이 여러 engine의 surface를 가리킬 수 있는 메서드면 모든 engine의 열린 surface를 붙인다.
    /// 핸들러는 전달받은 engine 하나만 보므로 다른 창의 열린 surface를 닫힌 것으로 오판하지 않게 한다.
    pub(crate) fn with_open_surfaces_if_needed<'a>(
        &self,
        checked: &crate::ipc::handler::CheckedRequest<'a>,
    ) -> Option<crate::ipc::handler::CheckedRequest<'a>> {
        let request = checked.request();
        crate::core::request_target::needs_open_surfaces_of_every_engine(&request.method).then(
            || {
                checked.with_open_surfaces(crate::core::request_target::open_surfaces_of(
                    self.engines().all(),
                ))
            },
        )
    }

    /// 자원 ID, 문자열 workspace 순서로 소유 창을 찾는다. surface nickname은 요청 게이트가 접수할 때
    /// surface ID로 바꿔 두므로 자원 ID로 찾는다.
    /// 모호한 workspace 이름이나 대상 없는 다중 창 요청의 오류를 포커스 폴백으로 덮지 않는다.
    pub(crate) fn find_request_owner(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<Option<WindowId>, String> {
        let named = request_resource_id(method, params);
        if let Some(rid) = named {
            let found = self.find_main_with_resource(rid);
            if found.is_some() {
                return Ok(found);
            }
        }
        if let Some(target) = params.get("workspace").and_then(|v| v.as_str()) {
            return self.find_main_with_workspace_target(target);
        }
        if external_parent_requires_workspace(method, self.main_window_count(), params) {
            return Err(format!(
                "multiple windows open; 'workspace' is required for '{method}' with \
                 'parent_external' (cannot infer the target window from focus)"
            ));
        }
        if ambiguous_parent_fallback_requires_surface(
            method,
            self.main_window_count(),
            named.is_some(),
        ) {
            return Err(format!(
                "multiple windows open; --surface is required for '{method}' \
                 (cannot infer the target window from focus)"
            ));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_window_allows_omitted_surface_for_ambiguous_methods() {
        for method in [
            "terminal.kill",
            "terminal.release",
            "terminal.respawn",
            "terminal.broadcast",
            "terminal.set_nickname",
        ] {
            assert!(
                !ambiguous_parent_fallback_requires_surface(method, 1, false),
                "{method}: 창이 하나이면 --surface 생략을 허용해야 한다"
            );
        }
        assert!(!ambiguous_parent_fallback_requires_surface(
            "terminal.kill",
            0,
            false
        ));
    }

    #[test]
    fn multiple_windows_reject_omitted_surface_for_ambiguous_methods() {
        for method in [
            "terminal.kill",
            "terminal.release",
            "terminal.respawn",
            "terminal.broadcast",
            "terminal.set_nickname",
        ] {
            assert!(
                ambiguous_parent_fallback_requires_surface(method, 2, false),
                "{method}: 창이 둘이면 --surface를 요구해야 한다"
            );
        }
    }

    /// 명시한 ID가 없다는 오류를 --surface 생략 오류로 덮지 않아야 한다.
    #[test]
    fn a_named_target_is_not_answered_with_the_ambiguity_error() {
        for method in [
            "terminal.kill",
            "terminal.release",
            "terminal.respawn",
            "terminal.broadcast",
            "terminal.set_nickname",
        ] {
            assert!(
                !ambiguous_parent_fallback_requires_surface(method, 2, true),
                "{method}: 이미 지정한 ID의 부재를 --surface 생략 오류로 덮으면 안 된다"
            );
            assert!(
                ambiguous_parent_fallback_requires_surface(method, 2, false),
                "{method}: 대상이 없으면 다중 창 폴백을 거절해야 한다"
            );
        }
    }

    #[test]
    fn unrelated_methods_are_unaffected_even_with_multiple_windows() {
        assert!(!ambiguous_parent_fallback_requires_surface(
            "terminal.spawn",
            2,
            false
        ));
        assert!(!ambiguous_parent_fallback_requires_surface(
            "terminal.tell",
            2,
            false
        ));
        assert!(!ambiguous_parent_fallback_requires_surface(
            "terminal.children",
            2,
            false
        ));
    }

    #[test]
    fn an_external_parent_needs_a_workspace_only_with_several_windows() {
        let external = serde_json::json!({ "parent_external": "codex:t", "child": 0 });
        for method in [
            "terminal.children",
            "terminal.kill",
            "terminal.release",
            "terminal.respawn",
            "terminal.broadcast",
            "terminal.set_nickname",
        ] {
            assert!(
                external_parent_requires_workspace(method, 2, &external),
                "{method}"
            );
            assert!(
                !external_parent_requires_workspace(method, 1, &external),
                "{method}"
            );
        }
        assert!(!external_parent_requires_workspace(
            "terminal.children",
            2,
            &serde_json::json!({ "surface": 7 })
        ));
        assert!(!external_parent_requires_workspace(
            "terminal.children",
            2,
            &serde_json::json!({ "parent_external": null })
        ));
        assert!(!external_parent_requires_workspace(
            "terminal.tell",
            2,
            &external
        ));
    }
}
