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

/// 플러그인이 선언한 팝업의 범위 종류. 실행 중 선언이 매니페스트 선언보다 먼저다.
/// 선언이 없거나 지금 plugin 팝업이 쓸 수 없는 범위 종류면 None이다(그런 선언은 열리지 않는다).
pub(crate) fn declared_popup_scope(
    mgr: &tasty_host_plugin::PluginManager,
    plugin_id: &str,
    popup_id: &str,
) -> Option<PopupScopeDecl> {
    use tasty_plugin_protocol::popup_decl::PopupScopeKind;
    match mgr.popup_decl(plugin_id, popup_id)?.scope_kind {
        PopupScopeKind::Window => Some(PopupScopeDecl::Window),
        PopupScopeKind::Surface => Some(PopupScopeDecl::Surface),
        PopupScopeKind::Workspace | PopupScopeKind::Pane | PopupScopeKind::Tab => None,
    }
}

/// 이 창에 그릴 인스턴스인지. 창 범위 인스턴스는 이 창에 결속한 것만 그린다.
/// surface 범위 인스턴스는 그 surface 가 이 창 배치에 보이는지로 렌더 경로가 정한다.
#[cfg(feature = "gui")]
pub(crate) fn drawn_in_window(
    window_popups: &std::collections::BTreeSet<u64>,
    instance_id: u64,
    decl: PopupScopeDecl,
) -> bool {
    decl != PopupScopeDecl::Window || window_popups.contains(&instance_id)
}

/// 같은 plugin·팝업의 인스턴스가 이미 열려 있는지. 다시 열면 그 인스턴스를 돌려받으므로
/// 여는 진입점은 열기 전에 이 값을 읽어, 이미 열린 인스턴스의 창 결속을 바꾸지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn is_open(
    mgr: &tasty_host_plugin::PluginManager,
    plugin_id: &str,
    popup_id: &str,
) -> bool {
    mgr.popup_instances()
        .any(|(_, inst)| inst.plugin_id == plugin_id && inst.popup_id == popup_id)
}

/// 방금 연 인스턴스가 창 범위면 `window` 창에 결속한다. 이미 열려 있던 인스턴스(`was_open`)는
/// 처음 결속한 창을 유지한다. 결속한 창을 버리면 그 인스턴스도 닫히므로(`App::retire_main_window`)
/// 열려 있는 창 범위 인스턴스는 늘 살아 있는 창이나 보관된(parked) 창 상태에 결속돼 있다.
#[cfg(feature = "gui")]
pub(crate) fn bind_window_popup(
    window: &mut crate::state::MainViewState,
    mgr: &tasty_host_plugin::PluginManager,
    instance_id: u64,
    was_open: bool,
) {
    if was_open {
        return;
    }
    let window_scoped = mgr
        .popup_instances()
        .any(|(id, inst)| id == instance_id && inst.contribute.scope == PopupScopeDecl::Window);
    if window_scoped {
        window.plugin_window_popups.insert(instance_id);
    }
}

/// plugin 팝업을 연 입력의 출처. 그 팝업이 창의 포커스 노드가 될지(`opened_by_user`) 정한다. 여는 경로 이름이
/// 아니라 입력 시점에 사용자가 그 자리에 있었는지로 나눈다. plugin 이름·kind 로 나누지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupOpener<'a> {
    /// 사용자가 이 창에서 고른 명령(도구 메뉴·명령 팔레트·단축키·변환 입력).
    UserCommand,
    /// plugin 이 스스로 연 팝업(surface 이벤트·`popup.show`). `event_surface`는 surface 이벤트로 열었으면 그 이벤트를
    /// 낸 surface 이고 `popup.show` 면 없다. `focused_surface`·`focused_surface_owner`는 여는 시점에 그 창의 엔진
    /// 포커스 surface 와 그 surface 를 가진 plugin 이다.
    Plugin {
        plugin_id: &'a str,
        event_surface: Option<u32>,
        focused_surface: Option<u32>,
        focused_surface_owner: Option<&'a str>,
    },
    /// 외부 에이전트·debug 의 IPC 열기(debug 빌드의 `debug.popup.open`).
    #[cfg(debug_assertions)]
    Agent,
}

/// 사용자가 연 팝업인지. 사용자가 고른 명령이면 사용자다. plugin 이 surface 이벤트로 연 팝업은 그 이벤트를 낸
/// surface 가 여는 시점에 엔진 포커스 surface 일 때만, `popup.show` 로 연 팝업은 그 plugin 의 surface 가 엔진 포커스
/// surface 일 때만 사용자 출처로 본다. 에이전트가 연 팝업은 passive 다(포커스 노드를 바꾸지 않는다. 덮는 팝업이면
/// 정규화가 포커스 노드로 한다).
pub(crate) fn opened_by_user(opener: PopupOpener<'_>) -> bool {
    match opener {
        PopupOpener::UserCommand => true,
        PopupOpener::Plugin {
            plugin_id,
            event_surface,
            focused_surface,
            focused_surface_owner,
        } => match event_surface {
            Some(surface) => focused_surface == Some(surface),
            None => focused_surface_owner == Some(plugin_id),
        },
        #[cfg(debug_assertions)]
        PopupOpener::Agent => false,
    }
}

/// 방금 연 plugin 팝업 인스턴스를, 사용자가 연 것이면 그 팝업을 그리는 창의 포커스 노드로 한다. 다음 그리기 전에
/// 키가 와도 살아 있는 노드로 보도록 결속 범위를 지금 기록한다. 대상 surface 결속 뒤에 부른다.
pub(crate) fn focus_opened_popup(
    window: &mut crate::state::MainViewState,
    mgr: &tasty_host_plugin::PluginManager,
    instance_id: u64,
    opener: PopupOpener<'_>,
) {
    if !opened_by_user(opener) {
        return;
    }
    let Some((_, inst)) = mgr.popup_instances().find(|(id, _)| *id == instance_id) else {
        return;
    };
    // 포커스 노드는 이 창에 그리는 팝업만 될 수 있다. 다른 창에 결속한 창 범위 인스턴스를 이 창에서 다시 요청해도
    // 이 창의 포커스·결속 범위를 바꾸지 않는다.
    if !drawn_in_window(
        &window.plugin_window_popups,
        instance_id,
        inst.contribute.scope,
    ) {
        return;
    }
    window.plugin_popup_bound_scopes.insert(
        instance_id,
        (
            inst.plugin_id.clone(),
            popup_scope(inst.contribute.scope, inst.scope_surface),
        ),
    );
    window.plugin_popup_hidden.remove(&instance_id);
    window
        .popups
        .set_focus(Some(crate::adapters::ui::popup::focus::PopupNode::Plugin(
            instance_id,
        )));
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
    let instances: Vec<(u64, &PopupInstance)> = instances.collect();
    // 키 후보는 이 창에 그리는 인스턴스만이다(다른 창에 결속한 창 범위 인스턴스는 빠진다).
    let window_popups = &state.plugin_window_popups;
    state.plugin_popup_frame_entries = Some(
        instances
            .iter()
            .filter(|(id, inst)| drawn_in_window(window_popups, *id, inst.contribute.scope))
            .map(|(id, inst)| {
                (
                    *id,
                    inst.z_seq,
                    popup_scope(inst.contribute.scope, inst.scope_surface),
                )
            })
            .collect(),
    );
    state.plugin_popup_bound_scopes = bound_popup_scopes(instances.into_iter());
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

    /// J2. plugin 이 surface 이벤트로 연 팝업은 이벤트를 낸 surface 가 엔진 포커스일 때만 사용자 출처다. 같은 plugin 의
    /// 배경 surface B 가 낸 이벤트는 A 에 포커스가 있어도 passive 다. `popup.show`(이벤트 surface 없음)는 그 plugin 의
    /// surface 가 엔진 포커스면 사용자 출처다.
    #[test]
    fn a_surface_event_popup_is_user_opened_only_when_its_own_surface_is_focused() {
        const A: u32 = 11;
        const B: u32 = 12;
        let opener = |event_surface| PopupOpener::Plugin {
            plugin_id: "p",
            event_surface,
            focused_surface: Some(A),
            focused_surface_owner: Some("p"),
        };
        assert!(opened_by_user(opener(Some(A))));
        assert!(!opened_by_user(opener(Some(B))), "배경 surface 의 이벤트");
        assert!(opened_by_user(opener(None)), "popup.show");
        assert!(!opened_by_user(PopupOpener::Plugin {
            plugin_id: "p",
            event_surface: None,
            focused_surface: Some(A),
            focused_surface_owner: Some("q"),
        }));
        assert!(opened_by_user(PopupOpener::UserCommand));
        #[cfg(debug_assertions)]
        assert!(!opened_by_user(PopupOpener::Agent));
    }

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
