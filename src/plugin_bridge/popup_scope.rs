//! 플러그인 팝업과 자식 파일 피커의 소속 범위를 호스트가 정한다.
//! 범위는 작용 대상에서 나온다(docs/design/systems/popup.md "작용 대상 범위 규칙").
//! 대상이 없는 surface 팝업은 창 범위로 바꾸지 않고 범위가 없는 것으로 본다.

use crate::adapters::ui::popup::{PopupScope, file_picker::FILE_PICKER_POPUP_ID};
use crate::state::{FilePickerRequester, RequestContext};
use tasty_host_plugin::manager::PopupInstance;
use tasty_plugin_manifest::PopupScopeDecl;

/// 선언과 결속된 대상으로 범위를 정한다. surface 선언인데 대상이 없으면 None이다.
/// 플러그인이 다른 surface를 지정하지 못하도록 대상은 호스트 진입점에서만 채운다.
pub(crate) fn popup_scope(decl: PopupScopeDecl, scope_surface: Option<u32>) -> Option<PopupScope> {
    match (decl, scope_surface) {
        (PopupScopeDecl::Surface, Some(sid)) => Some(PopupScope::Surface(sid)),
        (PopupScopeDecl::Surface, None) => None,
        (PopupScopeDecl::Window, _) => Some(PopupScope::Window),
    }
}

/// surface 범위로 선언한 팝업인데 결속할 대상이 없는지. 그런 요청은 창 범위로 바꾸지 않고 열지 않는다.
pub(crate) fn lacks_required_target(decl: Option<PopupScopeDecl>, target: Option<u32>) -> bool {
    target.is_none() && decl == Some(PopupScopeDecl::Surface)
}

/// 활성 플러그인이 선언한 팝업의 범위 종류. 선언이 없으면 None이다.
pub(crate) fn declared_popup_scope(
    mgr: &tasty_host_plugin::PluginManager,
    plugin_id: &str,
    popup_id: &str,
) -> Option<PopupScopeDecl> {
    mgr.plugin_popup_contributes()
        .into_iter()
        .find(|e| e.plugin_id == plugin_id && e.contribute.id == popup_id)
        .map(|e| e.contribute.scope)
}

/// 열린 플러그인 팝업 인스턴스마다 소유 플러그인과 결속 범위. 매 프레임 그리기 전과 파일 피커 요청 직전에 다시 만든다.
/// 배치 결과(좁은 surface 라 창에 그린 범위)가 아니라 선언과 결속 대상으로 정한 값이다.
pub(crate) type BoundPopupScopes = std::collections::HashMap<u64, (String, Option<PopupScope>)>;

/// 플러그인이 요청한 파일 피커를 열 수 없는 이유. IPC 오류 메시지로 그대로 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerScopeRefusal {
    OwnerNotFound,
    OwnerHasNoTarget,
    OriginNotFound,
    NoTarget,
}

impl PickerScopeRefusal {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::OwnerNotFound => "owner_popup_instance is not an open popup of this plugin",
            Self::OwnerHasNoTarget => "owner popup has no surface target",
            Self::OriginNotFound => "origin_surface_id does not name an existing surface",
            Self::NoTarget => "owner_popup_instance or origin_surface_id required",
        }
    }
}

/// 플러그인이 요청한 파일 피커의 범위. 열 때와 owner 를 따라갈 때 같은 함수를 쓴다.
/// owner 팝업이 있으면 그 팝업의 결속 범위, 없으면 `origin_surface_id`의 surface 범위다.
/// 둘 다 없거나 가리키는 대상이 없으면 열지 않는다. 창 범위로 바꾸지 않는다.
pub(crate) fn plugin_picker_scope(
    bound: &BoundPopupScopes,
    requester: &FilePickerRequester,
    surface_exists: impl Fn(u32) -> bool,
) -> Result<PopupScope, PickerScopeRefusal> {
    if let Some(owner) = requester.owner_popup_instance {
        return match bound.get(&owner) {
            Some((plugin, scope)) if *plugin == requester.plugin_id => {
                scope.clone().ok_or(PickerScopeRefusal::OwnerHasNoTarget)
            }
            _ => Err(PickerScopeRefusal::OwnerNotFound),
        };
    }
    match requester.origin_surface_id {
        Some(sid) if surface_exists(sid) => Ok(PopupScope::Surface(sid)),
        Some(_) => Err(PickerScopeRefusal::OriginNotFound),
        None => Err(PickerScopeRefusal::NoTarget),
    }
}

/// 열린 인스턴스마다 소유 플러그인과 선언·결속 대상으로 정한 범위.
fn bound_popup_scopes<'a>(
    instances: impl Iterator<Item = (u64, &'a PopupInstance)>,
) -> BoundPopupScopes {
    instances
        .map(|(id, inst)| {
            (
                id,
                (
                    inst.plugin_id.clone(),
                    popup_scope(inst.contribute.scope, inst.scope_surface),
                ),
            )
        })
        .collect()
}

/// 파일 피커 요청은 그리기 전에도 올 수 있다. 방금 연 owner 팝업도 찾도록
/// 요청을 처리하기 직전에 plugin manager의 인스턴스로 결속 범위를 다시 만든다.
pub(crate) fn refresh_for_request<'a>(
    state: &mut RequestContext,
    method: &str,
    instances: impl Iterator<Item = (u64, &'a PopupInstance)>,
) {
    if method == "file_picker.trigger" {
        state.plugin_popup_bound_scopes = bound_popup_scopes(instances);
    }
}

/// 그리기 전에 인스턴스의 결속 범위를 기록하고, owner 팝업이 있는 파일 피커가 owner 를 따라가게 한다.
/// owner 가 없는 피커와 owner 가 사라진 피커는 열 때 정한 범위를 그대로 둔다.
/// 화면 배치만 바꾸며 입력 초안·선택·요청 ID는 유지한다.
pub(crate) fn sync_plugin_popup_scopes<'a>(
    state: &mut RequestContext,
    instances: impl Iterator<Item = (u64, &'a PopupInstance)>,
) {
    state.plugin_popup_bound_scopes = bound_popup_scopes(instances);
    let bound = &state.plugin_popup_bound_scopes;
    for popup in state.popups.entries_of_mut(FILE_PICKER_POPUP_ID) {
        let Some(requester) =
            crate::adapters::ui::popup::file_picker::instances::picker_of(&popup.payload)
                .and_then(|data| data.requester.as_ref())
                .filter(|r| r.owner_popup_instance.is_some())
        else {
            continue;
        };
        if let Ok(scope) = plugin_picker_scope(bound, requester, |_| true) {
            popup.scope = scope;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_surface_popup_without_a_target_lacks_its_target() {
        assert!(lacks_required_target(Some(PopupScopeDecl::Surface), None));
        assert!(!lacks_required_target(
            Some(PopupScopeDecl::Surface),
            Some(7)
        ));
        assert!(!lacks_required_target(Some(PopupScopeDecl::Window), None));
        assert!(!lacks_required_target(None, None));
    }
}
