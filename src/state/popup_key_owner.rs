//! 키를 받는 팝업 판정. 플러그인 팝업 렌더와 창의 키 처리가 같은 후보와 같은 규칙을 쓴다.
//! host 후보는 지금 열려 있고 직전 그리기에서 범위가 보인 host 팝업이고(렌더에서는 이번 프레임에 그린 host
//! 팝업도 더한다), plugin 후보는 직전 프레임에 배치한 plugin 팝업이다. 렌더 뒤 host 팝업을 열거나 닫으면
//! 다음 키부터 반영된다.

use crate::adapters::ui::popup::occlusion::{KeyCandidate, PopupKeyOwner, key_owner};

impl super::MainViewState {
    /// 지금 키를 받는 팝업. 보이는 팝업이 없으면 `None`이다.
    pub(crate) fn popup_key_owner(&self) -> Option<PopupKeyOwner> {
        self.key_owner_with(std::iter::empty())
    }

    /// 키를 받는 팝업이 plugin 팝업이면 그 instance id.
    pub(crate) fn plugin_popup_key_target(&self) -> Option<u64> {
        plugin_target(self.popup_key_owner())
    }

    /// 플러그인 팝업 렌더가 쓰는 [`Self::plugin_popup_key_target`]. 이번 프레임에 그린 host 팝업도 후보다.
    /// 그리는 중 키로 닫힌 host 팝업(Escape 로 닫은 팔레트)이 받은 키를 같은 프레임에 아래 plugin 팝업이 다시
    /// 받지 않게 한다.
    pub(crate) fn plugin_popup_key_target_in_render(&self) -> Option<u64> {
        let drawn = self.host_popup_hittest.iter().map(|h| h.z_seq);
        plugin_target(self.key_owner_with(drawn))
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
            Some(PopupKeyOwner::Plugin(id)) => self.plugin_popup_expanded.contains(&id),
            _ => false,
        }
    }

    fn key_owner_with(&self, drawn_host: impl Iterator<Item = u64>) -> Option<PopupKeyOwner> {
        let host = self
            .popups
            .visible_open_z_seqs()
            .chain(drawn_host)
            .map(|z_seq| KeyCandidate {
                z_seq,
                owner: PopupKeyOwner::Host,
            });
        let plugin = self
            .plugin_popup_key_candidates
            .iter()
            .map(|&(id, z_seq)| KeyCandidate {
                z_seq,
                owner: PopupKeyOwner::Plugin(id),
            });
        key_owner(host.chain(plugin))
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

    /// 렌더 뒤 그 위에 host 팝업을 열면 다음 키부터 host가 받고, 닫으면 바로 plugin 팝업이 다시 받는다.
    #[test]
    fn a_host_popup_opened_or_closed_after_render_changes_the_owner_at_once() {
        let mut state = state_with_host_popup();
        let plugin_z = tasty_host_plugin::next_popup_z_seq();
        state.plugin_popup_key_candidates = vec![(42, plugin_z)];
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
        assert_eq!(state.plugin_popup_key_target(), Some(42));
    }

    /// 이번 프레임에 그린 host 팝업이 그리는 중 닫혀도(Escape 로 닫은 팔레트) 렌더에서는 그 프레임 동안 그 팝업이
    /// 키를 가진다. 아래 plugin 팝업이 같은 Escape 로 함께 닫히지 않는다. 창의 다음 키부터는 plugin 팝업이 받는다.
    #[test]
    fn a_host_popup_closed_while_drawn_keeps_the_keys_for_that_render() {
        let mut state = state_with_host_popup();
        let plugin_z = tasty_host_plugin::next_popup_z_seq();
        state.plugin_popup_key_candidates = vec![(42, plugin_z)];
        state.host_popup_hittest = vec![crate::adapters::ui::popup::occlusion::Occluder {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0)),
            z_seq: tasty_host_plugin::next_popup_z_seq(),
        }];
        assert_eq!(state.plugin_popup_key_target_in_render(), None);
        assert_eq!(state.plugin_popup_key_target(), Some(42));
        state.host_popup_hittest.clear();
        assert_eq!(state.plugin_popup_key_target_in_render(), Some(42));
    }

    /// 키 주인이 펼친 plugin 팝업이면 Esc 다시 접기가 소비한다. 아래 포커스된 host 팝업은 닫히거나 포커스를
    /// 잃지 않는다. 펼치지 않은 plugin 팝업이나 펼치지 않은 host 팝업이 키 주인이면 다음 처리로 간다.
    #[cfg(feature = "gui")]
    #[test]
    fn escape_refolds_only_an_expanded_key_owner_and_leaves_the_popup_below() {
        let mut state = state_with_host_popup();
        let host = state
            .popups
            .open_centered_focused("host_popup", PopupScope::Window)
            .unwrap();
        assert!(!state.refold_key_owner(), "펼치지 않은 host 키 주인");

        let plugin_z = tasty_host_plugin::next_popup_z_seq();
        state.plugin_popup_key_candidates = vec![(42, plugin_z)];
        assert!(!state.refold_key_owner(), "펼치지 않은 plugin 키 주인");

        state.plugin_popup_expanded.insert(42);
        assert!(state.refold_key_owner());
        let p = state.popups.instance(host).unwrap();
        assert!(p.open && p.focused, "아래 host 팝업은 그대로다");
        assert!(
            state.plugin_popup_expanded.contains(&42),
            "plugin 팝업은 그 프레임 렌더가 접는다"
        );
    }
}
