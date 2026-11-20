//! 창의 포커스 노드 하나. 키는 이 노드에서 시작해 트리 부모로 올라간다. 팝업이 포커스 위치면 그 노드는 팝업
//! 관리자가 갖고(`PopupManager::focus`), 나머지(엔진 포커스 surface, html 주소 띠, tutorial 말풍선)는 각자의
//! 상태에서 계산한다.
//!
//! 포커스 위치는 `PopupManager` 의 포커스 칸을 바꾸는 호출(`set_focus`·`give_focus`·`unfocus_node`·`focus_on_press`
//! 등), 무대·html 주소 띠의 전이, 설정 열기 요청, 튜토리얼 말풍선의 키 포커스, 엔진 포커스 surface 선택으로 바뀐다
//! (`focus_node` 가 이 순서로 고른다). 그 호출 자리는 팝업 설계 문서(docs/design/systems/popup.md)의 Esc 소유권 절에
//! 있다. 사건 처리 중의 전이는 즉시 적용하고, 사건 시점의 노드가 지금 입력 몫의 노드와 다르면 그 사건에서 몫을
//! 끊는다(`cut_key_frame`). 정규화(`normalize_focus`)는 창의 입력과 프레임마다 돌며 보이지 않거나 닫힌 팝업, 엔진
//! 포커스가 옮겨 간 뒤 경로에서 빠진 팝업의 포커스를 풀고, 덮인 노드에 있는 포커스를 덮는 노드로 옮긴다. 범위마다
//! 포커스를 기억하지 않는다.
//!
//! 그리기는 끊어 둔 입력 몫 하나를 그리므로 그리는 동안의 판정은 그 몫의 포커스 노드(`pass_focus_node`)를 읽는다.

use crate::adapters::ui::popup::PopupScope;
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::frame_keys::Incoming;
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

/// 끊은 자리 뒤 그리기 몫 하나의 누름 스냅샷과 포커스 노드.
#[derive(Debug, Default)]
pub(crate) struct SegmentAfterCut {
    /// 그 몫의 첫 누름 스냅샷.
    press: Option<PressSnapshot>,
    /// 그 몫에 처음 든 키 사건 시점의 포커스 노드. 누름만 든 몫은 끊어 닫을 때의 포커스 노드다.
    focus: Option<FocusNode>,
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
        // 포커스 노드는 창에 하나다. 팝업이 포커스 노드면 주소 띠와 튜토리얼 말풍선은 키 포커스를 놓는다(둘이
        // 포커스를 얻는 누름은 이미 팝업 칸을 비운다).
        if self.popup_focus().is_some() {
            self.html_address.release_focus();
            self.tutorial.keyboard_focus = false;
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
        match self.segments_after_cut.back_mut() {
            Some(slot) => {
                slot.press.get_or_insert(snapshot);
            }
            None => {
                self.press_snapshot.get_or_insert(snapshot);
            }
        }
    }

    /// 누름·키(`incoming`)가 egui 입력 큐 `events` 끝에 들어오기 전에 부른다. 그 사건 시점의 포커스 노드가 지금 몫의
    /// 노드와 다르거나, 같은 노드라도 그 사건이 몫을 나눠야 하면(`PopupManager::cut_key_frame` 의 누름·누름 뒤 키 조건)
    /// 그 자리에서 끊어 그 사건부터를 뒤 그리기 몫으로 둔다. 몫의 노드는 그 몫에 처음 든 키 사건 시점의 포커스
    /// 노드다. 키 없이 누름만 든 몫은 끊는 시점(누름이 옮긴 뒤)의 포커스 노드로 닫는다. 사건 처리 중에 포커스 노드를
    /// 바꾸는 전이(누름, 무대 열기·닫기, 주소 띠 해제 등)는 즉시 적용되고, 다음 사건이 그 차이로 끊는다. 그 몫을
    /// 그리는 동안 포커스 노드를 읽는 판정은 그 몫의 노드를 읽는다(`pass_focus_node`). 노드 차이로 끊은 자리에 키가
    /// 오면 같은 자리에서 한 번 더 끊어 새 노드의 빈 몫을 둔다(`PopupManager::cut_key_frame`). 끊은 수를 돌려준다.
    /// 호출자는 egui 입력도 그 자리에서 그 수만큼 끊는다.
    pub(crate) fn cut_key_frame(
        &mut self,
        core: &CoreState,
        events: &[egui::Event],
        incoming: Incoming,
    ) -> usize {
        let focus = self.focus_node(core);
        let mut cuts = 0;
        loop {
            let node = *self.open_segment_focus();
            let popup = match node.unwrap_or(focus) {
                FocusNode::Popup(node) => Some(node),
                _ => None,
            };
            let changed = node.is_some_and(|node| node != focus);
            if !self.popups.cut_key_frame(events, incoming, popup, changed) {
                break;
            }
            self.open_segment_focus().get_or_insert(focus);
            self.segments_after_cut
                .push_back(SegmentAfterCut::default());
            cuts += 1;
        }
        if incoming == Incoming::Key {
            self.open_segment_focus().get_or_insert(focus);
        }
        cuts
    }

    /// 지금 사건을 받는 몫(마지막으로 끊은 뒤 몫, 없으면 이번 그리기 몫)의 포커스 노드 칸.
    fn open_segment_focus(&mut self) -> &mut Option<FocusNode> {
        match self.segments_after_cut.back_mut() {
            Some(slot) => &mut slot.focus,
            None => &mut self.segment_focus,
        }
    }

    /// 지금 그리는 몫의 포커스 노드. 그 몫에 키 사건이 들었으면 첫 키 사건 시점의 값, 누름만 든 몫을 끊어 닫았으면
    /// 그때의 값, 아니면 지금 포커스 노드다. 그리기 중 포커스
    /// 노드를 읽는 판정(튜토리얼 말풍선 등)은 이 값을 읽는다.
    pub(crate) fn pass_focus_node(&self, core: &CoreState) -> FocusNode {
        self.segment_focus.unwrap_or_else(|| self.focus_node(core))
    }

    /// 지금 그리는 몫을 전체화면 무대로 그리는지. 그 몫의 포커스 노드가 무대일 때다. 창의 패스 경로
    /// (`GpuState::render`)가 이 값으로 무대와 일반 화면을 고른다. 그 몫 뒤의 종료 키가 그리기 전에 무대를 닫았어도 무대
    /// 몫의 입력(무대 안 누름 등)은 무대 패스로 그려 뒤 화면에 닿지 않는다. 닫힌 무대의 패스는 아무것도 그리지 않는다.
    pub(crate) fn pass_draws_stage(&self, core: &CoreState) -> bool {
        self.pass_focus_node(core) == FocusNode::Stage
    }

    /// 지금 그리는 몫의 키를 받는 포커스 노드. 그 몫의 키가 팝업에 묶이지 않았을 때 그 몫의 포커스 노드다. 그리기 중
    /// surface 본문(탐색기 타입어헤드)과 html 주소 입력칸이 그 몫의 키를 받을지는 지금 포커스(`focused_surface_id`·
    /// `window_keys_to_overlay`·주소 띠의 소유권)가 아니라 이 값이 자기 노드(`Surface`·`HtmlAddress`)인지로 정한다.
    /// 뒤 몫의 사건이 옮긴 포커스로 앞 몫의 키를 판정하지 않기 위해서다.
    pub(crate) fn pass_key_node(&self, core: &CoreState) -> Option<FocusNode> {
        self.surfaces_take_keys()
            .then(|| self.pass_focus_node(core))
    }

    /// egui 패스 하나(입력 몫 하나)를 닫는다. 그 몫의 결속·넘길 Esc(`PopupManager::close_key_pass`)와 누름 스냅샷·
    /// 포커스 노드를 함께 꺼내, 다음 몫의 것(끊어 넘긴 몫의 것, 없으면 비움)을 다음 패스에 남긴다. 창의 egui 패스는
    /// 모두 이 함수로 닫는다(`GpuState::run_main_pass`). 그 몫에 egui 로 보내지 않은 Esc 뗌이 왔으면 패스를 마친
    /// `ctx` 의 Escape 눌림을 푼다(`PopupManager::note_unfed_escape_release`).
    pub(crate) fn close_input_pass(&mut self, ctx: &egui::Context) {
        if self.popups.close_key_pass() {
            crate::adapters::ui::popup::body_keys::release_escape(ctx);
        }
        let next = self.segments_after_cut.pop_front().unwrap_or_default();
        self.press_snapshot = next.press;
        self.segment_focus = next.focus;
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
