//! 창이 egui 에 실은 누름 가운데 뗌이 오지 않은 키(남는 눌림). 뗌은 짝 누름의 수신자에게 간다: 누름을 egui 에
//! 실었으면 지금 게이트와 관계없이 egui 로 보낸다. 그 누름을 받은 plugin 팝업이 지금 수신자가 아니면 뗌은 그 팝업에만
//! 직접 붙이고(`plugin_popup_key_releases`) egui 로 싣지 않는다(지금 수신자에게 새 입력이 되지 않는다). 누름을 egui 에
//! 싣지 않았으면(터미널·egui-mesh 는
//! 누름만 받는다) 뗌도 egui 로 보내지 않는다. OS 포커스 상실과 모달 진입은 뗌 사건을 기다리지 않고 그 자리에서
//! 남은 눌림을 모두 푼다. Escape 는 Esc 몫의 기록(`PopupManager::note_unfed_escape_release`)이 맡는다.

use super::MainViewState;

/// egui 에 실은 누름 하나. `plugin` 은 그 누름을 받은 plugin 팝업 instance 다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeldKey {
    pub(crate) key: egui::Key,
    pub(crate) plugin: Option<u64>,
}

impl MainViewState {
    /// 키 사건 하나의 egui 게이트 결과(`window_feeds_key`)를 짝 누름 기준으로 고친다. `key` 는 그 사건의 egui 키와
    /// 누름 여부이고(egui 키가 없거나 Escape 면 게이트 결과 그대로다), `plugin` 은 이 사건을 받는 plugin 팝업이다
    /// (`edit_target::plugin_popup_edit_target`). 반환은 `window_feeds_key` 와 같은 꼴이다.
    pub(crate) fn pair_key_release(
        &mut self,
        key: Option<(egui::Key, bool)>,
        plugin: Option<u64>,
        fed: Option<bool>,
    ) -> Option<bool> {
        let Some((key, pressed)) = key.filter(|(key, _)| *key != egui::Key::Escape) else {
            return fed;
        };
        let held = self.egui_held_keys.iter().position(|h| h.key == key);
        if pressed {
            if fed.is_some() && held.is_none() {
                self.egui_held_keys.push(HeldKey { key, plugin });
            }
            return fed;
        }
        let held = self.egui_held_keys.swap_remove(held?);
        match held.plugin {
            Some(id) if plugin != Some(id) => {
                self.plugin_popup_key_releases.push((id, key));
                None
            }
            _ => Some(fed.unwrap_or(false)),
        }
    }

    /// OS 포커스가 바뀐 창의 남은 눌림. 포커스를 잃으면 egui 에 실은 눌림을 모두 푸는 뗌이다(창의 egui 입력에
    /// 넣는다). X11·Windows 가 포커스를 잃을 때 합성한 짝뗌은 App 이 거르므로 기다리지 않는다.
    pub(crate) fn release_on_window_focus(&mut self, focused: bool) -> Vec<egui::Event> {
        if focused {
            return Vec::new();
        }
        self.take_held_key_releases()
    }

    /// 모달이 열릴 때 창의 남은 눌림을 푸는 뗌. 모달이 열린 동안 오는 뗌은 단계 0·1 에서 막힌다.
    pub(crate) fn release_on_modal_entry(&mut self) -> Vec<egui::Event> {
        self.take_held_key_releases()
    }

    fn take_held_key_releases(&mut self) -> Vec<egui::Event> {
        let held = std::mem::take(&mut self.egui_held_keys);
        self.plugin_popup_key_releases
            .extend(held.iter().filter_map(|h| h.plugin.map(|id| (id, h.key))));
        held.iter()
            .map(|h| egui::Event::Key {
                key: h.key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    }
}
