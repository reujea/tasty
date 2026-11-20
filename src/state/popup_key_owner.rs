//! 키를 받는 팝업 판정. 키 주인은 창의 포커스 노드가 팝업일 때 그 팝업이다(`MainViewState::focus_node`).
//! 그리기와 창의 키 처리가 같은 값을 쓴다. 창의 키 처리는 그리기보다 먼저 돌며, 그때 포커스 노드를 키 주인으로
//! 묶는다(`bind_window_key_owner`). 그 키를 실은 그리기는 묶인 값을, 묶인 값이 없으면 이번 프레임 범위·표시 판정
//! 뒤 후보로 정한 값을 그 프레임 키 주인 하나로 정하고(`decide_frame_key_owner`), host 본문·버튼과 plugin 렌더가
//! 그 값을 쓴다.
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::occlusion::PopupKeyOwner;

impl super::MainViewState {
    /// 지금 키를 받는 팝업: 포커스 노드인 팝업이다. 포커스 노드가 팝업이 아니면 `None`이다. 덮인 팝업으로 포커스
    /// 노드를 옮기는 일은 정규화(`normalize_focus`)가 맡는다.
    pub(crate) fn popup_key_owner(&self) -> Option<PopupKeyOwner> {
        self.live_key_owner().map(|node| match node {
            PopupNode::Host(_) => PopupKeyOwner::Host,
            PopupNode::Plugin(id) => PopupKeyOwner::Plugin(id),
        })
    }

    /// 키를 받는 팝업이 plugin 팝업이면 그 instance id. 창이 이번 키의 주인을 묶었으면 그 값을 따른다.
    pub(crate) fn plugin_popup_key_target(&self) -> Option<u64> {
        self.frame_plugin_key_target(None)
    }

    /// 이번 프레임의 키 주인을 정한다. host 팝업 그리기가 이번 프레임 범위·표시 판정(`prepare_frame_for_draw`)
    /// 뒤, 그리기 전에 한 번 부른다. plugin 후보는 그리기 전에 만든 이번 프레임 인스턴스 중 범위가 보이는 것이다.
    /// 창이 입력 시점에 묶은 키 주인이 있으면 그것이 이 프레임 키 주인이다(`PopupManager::decide_frame_keys`).
    /// host 그리기와 plugin 렌더(`frame_plugin_key_target`)가 이 값을 쓴다.
    pub(crate) fn decide_frame_key_owner(
        &mut self,
        layout: Option<&crate::adapters::ui::LayoutContext>,
    ) -> Option<PopupKeyOwner> {
        self.refresh_plugin_key_candidates(layout);
        let live = self.popup_key_owner();
        let owner = self.popups.decide_frame_keys(live).owner;
        self.frame_key_owner = Some(owner);
        owner
    }

    /// 이번 프레임 plugin 인스턴스(그리기 전에 만든 것)로 보이는 plugin 팝업과 범위에 숨은 plugin 팝업을 정한다.
    /// 이번 프레임 인스턴스가 없으면(이미 정했거나 렌더 전) 그대로 둔다.
    pub(crate) fn refresh_plugin_key_candidates(
        &mut self,
        layout: Option<&crate::adapters::ui::LayoutContext>,
    ) {
        if let Some(entries) = self.plugin_popup_frame_entries.take() {
            self.plugin_popup_hidden.clear();
            let mut candidates = Vec::with_capacity(entries.len());
            for (id, z_seq, scope) in entries {
                let visible = scope.is_some_and(|scope| {
                    crate::adapters::ui::PopupManager::is_scope_visible(&scope, layout)
                });
                if visible {
                    candidates.push((id, z_seq));
                } else {
                    self.plugin_popup_hidden.insert(id);
                }
            }
            self.plugin_popup_key_candidates = candidates;
        }
    }

    /// 창이 키를 받은 입력 시점의 키 주인을 묶는다. 그 키를 실은 다음 그리기에서 이 값이 그 프레임 키 주인이다
    /// (창의 키 처리는 그리기보다 먼저 돈다). host 키 주인은 그 인스턴스로 묶는다. 키 주인이 있으면 참이다.
    pub(crate) fn bind_window_key_owner(&mut self) -> bool {
        let live = self.live_key_owner();
        if let Some(live) = live {
            self.popups.bind_input_key_owner(live);
        }
        live.is_some()
    }

    /// 창이 받은 키 사건을 egui 로 보낼지(`view::main::feed_egui_keyboard` 의 게이트). 팝업·무대 같은 키 오버레이가
    /// 열렸거나 egui 로 입력을 받는 host 위젯 surface(`egui_surface`)에 포커스가 있을 때만 보낸다. 보낼 키 입력
    /// (`binds`: 키·IME)이면 그 키를 실은 그리기의 키 주인을 입력 시점에 묶는다. 보내지 않으면 `None`, 보내면 키
    /// 주인을 묶었는지다(묶었으면 그 그리기를 요청한다).
    pub(crate) fn window_feeds_egui(&mut self, egui_surface: bool, binds: bool) -> Option<bool> {
        let overlay_open = self.window_keys_to_overlay();
        if !(overlay_open || egui_surface) {
            return None;
        }
        Some(binds && self.bind_window_key_owner())
    }

    /// 창이 키를 터미널(PTY)·단축키로 보내지 않는지(`view::main::keyboard` 의 게이트). 팝업 키 주인이 있거나 창이
    /// 이번 프레임 키를 팝업에 묶었으면 참이다.
    pub(crate) fn window_holds_terminal_keys(&self) -> bool {
        self.keyboard_overlay_open()
    }

    /// 팝업 층 아래 egui 화면(surface UI·사이드바 등)이 이번 프레임 키 입력을 볼지. 창이 이번 프레임 키를 팝업
    /// 키 주인에 묶었으면 그 키는 팝업 것이라 아래 화면은 보지 않는다(팝업 층보다 먼저 그리므로 이 값으로 막는다).
    pub(crate) fn surfaces_take_keys(&self) -> bool {
        self.popups.frame_bound_key_owner().is_none()
    }

    /// 지금 키를 받을 수 있는 포커스 노드 팝업. host 는 인스턴스다.
    pub(crate) fn live_key_owner(&self) -> Option<PopupNode> {
        #[cfg(feature = "gui")]
        {
            self.popup_focus()
        }
        #[cfg(not(feature = "gui"))]
        {
            self.popups
                .host_key_owner()
                .map(|p| PopupNode::Host(p.instance))
        }
    }

    /// plugin 팝업 렌더가 쓰는 이번 프레임 키 주인 plugin 팝업. `decided`는 렌더를 시작할 때 꺼낸
    /// `frame_key_owner`다. 그리는 중 키로 닫힌 host 팝업(Escape 로 닫은 팔레트)이 받은 키를 같은 프레임에 아래
    /// plugin 팝업이 다시 받지 않는다. 이번 프레임에 정하지 않았으면(host 그리기 없이 렌더만 부른 경우) 창이 묶은
    /// 값, 없으면 지금 후보로 정한다.
    pub(crate) fn frame_plugin_key_target(
        &self,
        decided: Option<Option<PopupKeyOwner>>,
    ) -> Option<u64> {
        plugin_target(
            decided.unwrap_or_else(|| match self.popups.frame_bound_key_owner() {
                Some(PopupNode::Plugin(id)) => Some(PopupKeyOwner::Plugin(id)),
                Some(PopupNode::Host(_)) => Some(PopupKeyOwner::Host),
                None => self.popup_key_owner(),
            }),
        )
    }

    /// 키 주인 팝업의 논리 범위. host 는 포커스를 가진 맨 위 인스턴스, plugin 은 선언·결속으로 정한 범위다.
    /// 키 주인이 없거나 host 팝업이 모두 포커스를 놓았으면 `None`이다.
    pub(crate) fn key_owner_scope(&self) -> Option<crate::adapters::ui::popup::PopupScope> {
        match self.popup_key_owner()? {
            PopupKeyOwner::Host => {
                let (_, iid, _) = self.popups.focused_dismissal_target()?;
                self.popups.instance(iid).map(|p| p.scope.clone())
            }
            #[cfg(feature = "gui")]
            PopupKeyOwner::Plugin(id) => self
                .plugin_popup_bound_scopes
                .get(&id)
                .and_then(|(_, scope)| scope.clone()),
            #[cfg(not(feature = "gui"))]
            PopupKeyOwner::Plugin(_) => None,
        }
    }

    /// Esc 의 다시 접기. 키 주인이 펼친 host 팝업이면 지금 접고, 펼친 plugin 팝업이면 참만 돌려준다.
    /// plugin 팝업은 이번 프레임 렌더가 접고 그 Esc 를 mesh 내용에 보내지 않는다(`plugin_bridge::popup_render`).
    pub(crate) fn refold_key_owner(&mut self) -> bool {
        match self.popup_key_owner() {
            Some(PopupKeyOwner::Host) => self.popups.refold_focused(),
            #[cfg(feature = "gui")]
            Some(PopupKeyOwner::Plugin(id)) => self.popups.plugin_expanded().contains(&id),
            _ => false,
        }
    }
}

fn plugin_target(owner: Option<PopupKeyOwner>) -> Option<u64> {
    match owner {
        Some(PopupKeyOwner::Plugin(id)) => Some(id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::popup::{PopupScope, PopupState};

    fn state_with_host_popup() -> crate::state::MainViewState {
        let (mut state, _session) = crate::state::tests::test_state();
        state.popups.register(PopupState::new(
            "host_popup",
            "Host".to_string(),
            egui::vec2(200.0, 100.0),
            crate::model::popup_kind::PopupScope::Window,
        ));
        state
    }

    /// 배치한 plugin 팝업이 없으면(첫 프레임 전) plugin 팝업은 키를 받지 않는다.
    #[test]
    fn no_placed_plugin_popup_means_no_plugin_key_target() {
        let state = state_with_host_popup();
        assert_eq!(state.plugin_popup_key_target(), None);
    }

    /// 열린 plugin 팝업 하나를 창에 둔다(결속 범위·키 후보). `focus`면 창의 포커스 노드로 한다.
    fn place_plugin_popup(state: &mut crate::state::MainViewState, id: u64, focus: bool) {
        let z = tasty_host_plugin::next_popup_z_seq();
        state.plugin_popup_key_candidates.push((id, z));
        state
            .plugin_popup_bound_scopes
            .insert(id, ("p".to_string(), Some(PopupScope::Window)));
        if focus {
            state.popups.set_focus(Some(PopupNode::Plugin(id)));
        }
    }

    /// Given 포커스 노드인 plugin 팝업, When 그 위에 host 팝업을 포커스를 주며 열면, Then 다음 키부터 host 가
    /// 받는다. When host 팝업을 닫으면, Then 포커스 노드는 열린 부모로 가고 부모가 없으면 팝업이 아니다(범위마다
    /// 포커스를 기억하지 않는다).
    #[test]
    fn a_host_popup_opened_over_a_plugin_owner_takes_the_keys_until_it_closes() {
        let mut state = state_with_host_popup();
        place_plugin_popup(&mut state, 42, true);
        assert_eq!(state.plugin_popup_key_target(), Some(42));

        state
            .popups
            .open_centered_focused("host_popup", PopupScope::Window);
        assert_eq!(
            state.popup_key_owner(),
            Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Host)
        );
        assert_eq!(state.plugin_popup_key_target(), None);

        state.popups.close("host_popup");
        assert_eq!(state.popup_key_owner(), None);
    }

    /// 이번 프레임에 그린 host 팝업이 그리는 중 닫혀도(Escape 로 닫은 팔레트) 렌더에서는 그 프레임 동안 그 팝업이
    /// 키를 가진다. 아래 plugin 팝업이 같은 Escape 로 함께 닫히지 않는다. 닫힌 host 팝업이 plugin 팝업이 연
    /// 자식이면 창의 다음 키부터는 그 plugin 팝업이 받는다.
    #[test]
    fn a_host_popup_closed_while_drawn_keeps_the_keys_for_that_render() {
        let mut state = state_with_host_popup();
        place_plugin_popup(&mut state, 42, false);
        let host = state
            .popups
            .open_centered_focused("host_popup", PopupScope::Window)
            .unwrap();
        state.popups.instance_mut(host).unwrap().plugin_parent = Some(42);
        let decided = state.decide_frame_key_owner(None);
        state.popups.close("host_popup");
        assert!(!state.popups.instance(host).is_some_and(|p| p.open));
        assert_eq!(state.plugin_popup_key_target(), Some(42));
        assert_eq!(state.frame_plugin_key_target(Some(decided)), None);
        assert_eq!(state.frame_plugin_key_target(None), Some(42));
    }

    /// 키 주인이 펼친 plugin 팝업이면 Esc 다시 접기가 소비한다. 아래 host 팝업은 닫히지 않는다. 펼치지 않은 plugin
    /// 팝업이나 펼치지 않은 host 팝업이 키 주인이면 다음 처리로 간다.
    #[cfg(feature = "gui")]
    #[test]
    fn escape_refolds_only_an_expanded_key_owner_and_leaves_the_popup_below() {
        let mut state = state_with_host_popup();
        let host = state
            .popups
            .open_centered_focused("host_popup", PopupScope::Window)
            .unwrap();
        assert!(!state.refold_key_owner(), "펼치지 않은 host 키 주인");

        place_plugin_popup(&mut state, 42, true);
        assert!(!state.refold_key_owner(), "펼치지 않은 plugin 키 주인");

        state.popups.expand_plugin(42);
        assert!(state.refold_key_owner());
        let p = state.popups.instance(host).unwrap();
        assert!(p.open, "아래 host 팝업은 열린 채다");
        assert!(
            state.popups.plugin_expanded().contains(&42),
            "plugin 팝업은 그 프레임 렌더가 접는다"
        );
    }

    /// Given L 범위의 host 팝업, 그 위에 포커스 노드인 펼친 plugin 팝업(file-open), 맨 위에 그 plugin 팝업이 연
    /// passive 피커가 있을 때, Then passive 피커는 포커스 노드를 바꾸지 않으므로 키 주인은 펼친 plugin 팝업이다.
    /// When Esc 의 다시 접기를 하면, Then 소비되고 L 팝업은 열린 채다. host 대상은 없다.
    #[cfg(feature = "gui")]
    #[test]
    fn a_passive_picker_on_top_takes_no_keys_and_escape_never_reaches_the_popup_below() {
        let mut state = state_with_host_popup();
        state.popups.register(PopupState::new(
            "picker",
            "Picker".to_string(),
            egui::vec2(200.0, 100.0),
            crate::model::popup_kind::PopupScope::Window,
        ));
        let below = state
            .popups
            .open_request(
                &crate::intent::OpenRequest::new("host_popup", PopupScope::Surface(1)).focused(),
                true,
            )
            .unwrap();
        place_plugin_popup(&mut state, 7, true);
        state.popups.expand_plugin(7);
        let picker = state
            .popups
            .open_request(
                &crate::intent::OpenRequest::new("picker", PopupScope::Surface(2)),
                false,
            )
            .unwrap();
        assert!(!state.popups.is_instance_focused(picker));
        assert_eq!(
            state.popup_key_owner(),
            Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(7))
        );
        assert!(
            state.popups.host_key_owner().is_none(),
            "다시 접기·일반 Esc 의 host 대상은 없다(passive 피커도 아래 L 팝업도 아니다)"
        );
        assert!(
            state.refold_key_owner(),
            "펼친 plugin 키 주인이 Esc 를 소비한다"
        );
        let p = state.popups.instance(below).unwrap();
        assert!(p.open, "아래 L 팝업은 닫히지 않는다");
    }
}
