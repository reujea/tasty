//! 창의 포커스 노드 하나. 키는 이 노드에서 시작해 트리 부모로 올라간다. 팝업이 포커스 위치면 그 노드는 팝업
//! 관리자가 갖고(`PopupManager::focus`), 나머지(엔진 포커스 surface, html 주소 띠, tutorial 말풍선)는 각자의
//! 상태에서 계산한다.
//!
//! 포커스 위치를 바꾸는 곳은 셋이다. 사용자가 연 팝업(`open_request`), 누름(`take_press_snapshot`), 그리고 창의
//! 입력과 프레임마다 도는 정규화(`normalize_focus`)다. 정규화는 보이지 않거나 닫힌 팝업, 엔진 포커스가 옮겨 간 뒤
//! 경로에서 빠진 팝업의 포커스를 풀고, 덮인 노드에 있는 포커스를 덮는 노드로 옮긴다. 범위마다 포커스를 기억하지
//! 않는다.

use crate::adapters::ui::popup::PopupScope;
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::route::{PressSnapshot, ScopePath, path_of_focus, path_of_scope};
use crate::core::CoreState;

/// 창의 포커스 노드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusNode {
    /// 전체화면 무대가 창을 덮었다.
    Stage,
    /// 아직 처리하지 않은 설정 창 열기 요청.
    SettingsRequest,
    /// 팝업 노드.
    Popup(PopupNode),
    /// 이 surface 의 html 주소 띠.
    HtmlAddress(u32),
    /// tutorial 말풍선.
    Tutorial,
    /// 엔진 포커스 surface. surface 가 없으면 `None`이다.
    Surface(Option<u32>),
}

impl FocusNode {
    /// debug 상태 조회(`debug.ui_state`의 `focus_node`)에 싣는 값. `kind`와 대상 id 다.
    #[cfg(debug_assertions)]
    pub(crate) fn describe(self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Stage => json!({"kind": "stage"}),
            Self::SettingsRequest => json!({"kind": "settings_request"}),
            Self::Popup(PopupNode::Host(iid)) => json!({"kind": "host_popup", "instance": iid.0}),
            Self::Popup(PopupNode::Plugin(id)) => json!({"kind": "plugin_popup", "instance": id}),
            Self::HtmlAddress(sid) => json!({"kind": "html_address", "surface": sid}),
            Self::Tutorial => json!({"kind": "tutorial"}),
            Self::Surface(sid) => json!({"kind": "surface", "surface": sid}),
        }
    }
}

impl super::MainViewState {
    /// 지금 포커스 노드. 팝업 노드는 정규화한 값(`normalize_focus`)이다.
    pub(crate) fn focus_node(&self, core: &CoreState) -> FocusNode {
        if self.fullscreen_stage_active() {
            FocusNode::Stage
        } else if self.has_settings_open_request() {
            FocusNode::SettingsRequest
        } else if let Some(node) = self.popup_focus() {
            FocusNode::Popup(node)
        } else if let Some(sid) = self.html_address.focused_surface() {
            FocusNode::HtmlAddress(sid)
        } else if self.tutorial.active.is_some() && self.tutorial.keyboard_focus {
            FocusNode::Tutorial
        } else {
            FocusNode::Surface(self.focused_surface_id(core))
        }
    }

    /// 포커스 위치인 팝업 노드가 지금 키를 받을 수 있으면 그 노드. host 는 열려 있고 범위가 보이며 부모 버튼이
    /// 대표하지 않는 인스턴스, plugin 은 열려 있고(결속 범위가 있고) 범위가 숨지 않은 인스턴스다.
    pub(crate) fn popup_focus(&self) -> Option<PopupNode> {
        match self.popups.focus()? {
            PopupNode::Host(iid) => self
                .popups
                .host_key_owner()
                .filter(|p| p.instance == iid)
                .map(|_| PopupNode::Host(iid)),
            PopupNode::Plugin(id) => self.plugin_node_alive(id).then_some(PopupNode::Plugin(id)),
        }
    }

    fn plugin_node_alive(&self, id: u64) -> bool {
        self.plugin_popup_bound_scopes.contains_key(&id) && !self.plugin_popup_hidden.contains(&id)
    }

    /// 포커스 위치를 지금 상태에 맞춘다. 창이 키·IME 를 받을 때, 누름 뒤, 그리고 팝업 층을 그리기 전에 부른다.
    pub(crate) fn normalize_focus(&mut self, core: &CoreState) {
        // 닫히거나 숨은 팝업은 포커스 위치가 아니다.
        if self.popups.focus().is_some() && self.popup_focus().is_none() {
            self.popups.set_focus(None);
        }
        // 엔진 포커스 surface 가 바뀌었으면(포커스 이동 단축키·IPC·workspace 전환) 새 경로 위에 없는 팝업의 포커스를
        // 푼다. Window 범위 팝업은 늘 경로 위다. 바뀌지 않았거나 처음 보는 값이면 다른 범위 팝업을 누른 포커스를
        // 그대로 둔다.
        let engine_surface = self.focused_surface_id(core);
        let seen = self.focus_seen_surface.replace(engine_surface);
        if seen.is_some_and(|seen| seen != engine_surface)
            && let Some(node) = self.popups.focus()
            && let Some(scope) = self.node_logical_scope(node)
            && !path_of_focus(self, core).contains(&scope)
        {
            self.popups.set_focus(None);
        }
        // 덮인 노드는 포커스 노드가 될 수 없다. 포커스 위치를 덮는 노드가 있으면 그 노드가 포커스 위치다
        // (passive 여도 같다).
        let (path, above) = match self.popups.focus() {
            Some(node) => match self.node_placement(node) {
                Some((placement, z)) => (path_of_scope(core, &placement), Some(z)),
                None => (path_of_focus(self, core), None),
            },
            None => (path_of_focus(self, core), None),
        };
        if let Some(cover) = self.top_cover_over(&path, above) {
            self.popups.set_focus(Some(cover));
        }
        // 포커스 노드는 창에 하나다. 팝업이 포커스 노드면 주소 띠는 포커스를 놓는다(주소 띠가 포커스를 얻는 누름은
        // 이미 팝업 칸을 비운다).
        if self.popup_focus().is_some() && self.html_address.has_keyboard_focus() {
            self.html_address.release_focus();
        }
    }

    /// 누름 하나의 스냅샷을 남기고 그 누름이 정하는 포커스 위치로 옮긴다. 창의 누름(winit), debug 주입, 그리고
    /// 스냅샷 없이 egui 프레임에 들어온 누름이 모두 이 함수를 부른다. 같은 이벤트 묶음에서 누름 뒤에 오는 키는
    /// 옮긴 포커스 위치를 따른다. 포인터 대상(`popup_pointer_target`)이 팝업이면 그 팝업, 아니면(surface·창 크롬)
    /// 포커스 위치는 팝업이 아니게 된다.
    pub(crate) fn take_press_snapshot(
        &mut self,
        core: &CoreState,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> PressSnapshot {
        let snapshot = PressSnapshot::take(self, core, pos, button);
        match self.popup_pointer_target(pos) {
            Some(target) => {
                if let Some(node) = self.node_of_z(target.z_seq) {
                    self.popups.focus_on_press(Some(node));
                }
            }
            None => self.popups.focus_on_press(None),
        }
        self.normalize_focus(core);
        snapshot
    }

    /// 창이 받은 누름의 스냅샷을 그 누름을 처리할 그리기 몫에 남긴다. 한 몫에는 첫 누름만 남긴다. 입력을 끊었으면
    /// (`cut_key_frame`) 마지막으로 끊은 뒤 몫의 것이다.
    pub(crate) fn keep_press_snapshot(&mut self, snapshot: PressSnapshot) {
        match self.press_snapshots_after_cut.back_mut() {
            Some(slot) => {
                slot.get_or_insert(snapshot);
            }
            None => {
                self.press_snapshot.get_or_insert(snapshot);
            }
        }
    }

    /// 누름이 egui 입력 큐 `events` 끝에 들어오기 전에 부른다. 지금 몫에 키가 실렸으면 그 자리에서 끊어 누름부터를
    /// 뒤 그리기 몫으로 둔다(`PopupManager::cut_key_frame`). 참이면 호출자는 egui 입력도 그 자리에서 끊는다.
    pub(crate) fn cut_key_frame(&mut self, events: &[egui::Event]) -> bool {
        let cut = self.popups.cut_key_frame(events);
        if cut {
            self.press_snapshots_after_cut.push_back(None);
        }
        cut
    }

    /// 그리기 몫 하나를 마쳤다. 다음 몫의 누름 스냅샷(끊어 넘긴 누름의 것, 없으면 비움)을 다음 그리기에 남긴다.
    pub(crate) fn end_press_frame(&mut self) {
        self.press_snapshot = self.press_snapshots_after_cut.pop_front().flatten();
    }

    /// 순번 하나의 팝업 노드. host·plugin 이 같은 전역 순번을 쓴다.
    fn node_of_z(&self, z: u64) -> Option<PopupNode> {
        if let Some(p) = self.popups.open_with_z(z) {
            return Some(PopupNode::Host(p));
        }
        self.plugin_popup_key_candidates
            .iter()
            .find(|(_, pz)| *pz == z)
            .map(|(id, _)| PopupNode::Plugin(*id))
    }

    /// 팝업 노드의 논리 범위. plugin 팝업의 범위가 없으면 Window 다.
    fn node_logical_scope(&self, node: PopupNode) -> Option<PopupScope> {
        match node {
            PopupNode::Host(iid) => self.popups.instance(iid).map(|p| p.scope.clone()),
            PopupNode::Plugin(id) => Some(
                self.plugin_popup_bound_scopes
                    .get(&id)
                    .and_then(|(_, scope)| scope.clone())
                    .unwrap_or(PopupScope::Window),
            ),
        }
    }

    /// 팝업 노드의 배치 범위와 순번. 펼친 팝업은 창에 선다.
    fn node_placement(&self, node: PopupNode) -> Option<(PopupScope, u64)> {
        match node {
            PopupNode::Host(iid) => self
                .popups
                .instance(iid)
                .map(|p| (p.placement_scope(), p.z_seq)),
            PopupNode::Plugin(id) => {
                let z = self
                    .plugin_popup_key_candidates
                    .iter()
                    .find(|(pid, _)| *pid == id)
                    .map(|(_, z)| *z)?;
                let placement = if self.popups.plugin_expanded().contains(&id) {
                    PopupScope::Window
                } else {
                    self.node_logical_scope(node)?
                };
                Some((placement, z))
            }
        }
    }

    /// `path` 위의 범위를 덮는 보이는 팝업 중 순번이 가장 큰 것. `above` 를 주면 그보다 순번이 큰 것만 본다.
    fn top_cover_over(&self, path: &ScopePath, above: Option<u64>) -> Option<PopupNode> {
        let host = self.popups.covering_nodes().filter_map(|(iid, scope, z)| {
            path.contains(&scope).then_some((PopupNode::Host(iid), z))
        });
        let plugin = self
            .plugin_popup_key_candidates
            .iter()
            .filter(|(_, z)| self.plugin_popup_covers.iter().any(|c| c.z_seq == *z))
            .filter_map(|&(id, _)| {
                let (scope, z) = self.node_placement(PopupNode::Plugin(id))?;
                path.contains(&scope).then_some((PopupNode::Plugin(id), z))
            });
        host.chain(plugin)
            .filter(|(_, z)| above.is_none_or(|a| *z > a))
            .max_by_key(|(_, z)| *z)
            .map(|(node, _)| node)
    }
}
