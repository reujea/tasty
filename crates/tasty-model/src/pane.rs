use super::tab::Tab;
use super::{PaneId, SplitDirection, SurfaceId, TabId, TerminalSurface};
use tasty_terminal::{Terminal, Waker};

/// [`Pane::spawn_terminal`] 이 받는 shell-spawning 옵션 묶음.
///
/// 인자를 따로 받으면 `too_many_arguments` 임계를 넘어 struct 로 묶었다.
pub struct ShellSpawnOpts<'a> {
    pub cols: usize,
    pub rows: usize,
    pub shell: Option<&'a str>,
    pub shell_args: &'a [&'a str],
    pub waker: Waker,
    pub working_dir: Option<&'a std::path::Path>,
    /// 자식 셸에 추가로 심을 환경변수(docs/features/terminal-output/index.md#명령-인덱싱-osc-133,
    /// `ShellConfig::envs_ref` 참고).
    pub extra_env: &'a [(&'a str, &'a str)],
}

/// 탭 전환 요청의 결과 — **성공/실패가 아니라 무엇이 일어났는가**다.
///
/// 가르는 이유는 [`Pane::goto_tab`] 의 doc 에 있다. 요점만: 세 갈래 중 둘은 정상이고
/// 하나만 오류인데, 예전 `bool` 은 그 둘을 하나로 눌렀다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabSwitch {
    /// 다른 탭으로 옮겼다.
    Switched,
    /// 이미 그 탭을 보고 있었다 — 정상이고, 바뀐 것이 없을 뿐이다.
    AlreadyActive,
    /// 그런 탭이 없다. 실제 탭 수를 함께 실어 보낸다 — 실패문이 "몇 개 중 몇 번" 을
    /// 말할 수 있어야 호출자가 자기 인덱스를 의심할지 대상을 의심할지 가른다.
    OutOfRange { tabs: usize },
    /// 포커스된 pane 이 없어 물음이 성립하지 않는다. [`Pane::goto_tab`] 은 이 갈래를
    /// 내지 않는다 — pane 을 찾아 주는 바깥 층이 더한다.
    NoPane,
}

/// A screen region with its own independent tab bar.
pub struct Pane {
    pub id: PaneId,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    /// Horizontal scroll offset for the tab bar (in logical pixels).
    pub tab_scroll_offset: f32,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            id: 0,
            tabs: Vec::new(),
            active_tab: 0,
            tab_scroll_offset: 0.0,
        }
    }
}

impl Pane {
    /// Create a Pane with a Surface trait object.
    pub fn new_with_surface(
        id: PaneId,
        tab_id: TabId,
        name: String,
        surface: Box<dyn super::Surface>,
    ) -> Self {
        let tab = super::tab::Tab::new_with_surface(tab_id, name, surface);
        Self {
            id,
            tabs: vec![tab],
            active_tab: 0,
            tab_scroll_offset: 0.0,
        }
    }

    /// Spawn a Terminal with the given shell spawn options. Caller registers the
    /// returned Terminal into `CoreState::terminals` *before* it inserts the
    /// returned Pane (or its surface) into a workspace — so the layout never sees
    /// a missing-store-entry state.
    pub fn spawn_terminal(
        surface_id: SurfaceId,
        spawn: ShellSpawnOpts<'_>,
    ) -> anyhow::Result<Terminal> {
        Terminal::new(
            tasty_terminal::TerminalConfig {
                cols: spawn.cols,
                rows: spawn.rows,
                shell: spawn.shell,
                args: spawn.shell_args,
                surface_id,
                working_dir: spawn.working_dir,
                initial_input: None,
                extra_env: spawn.extra_env,
            },
            spawn.waker,
        )
    }

    /// Create a Pane with a TerminalSurface marker. Caller must have already
    /// `engine.terminals.insert(surface_id, terminal)` for the spawned Terminal.
    pub fn new_with_terminal_marker(id: PaneId, tab_id: TabId, surface_id: SurfaceId) -> Self {
        let surface: Box<dyn super::Surface> = Box::new(TerminalSurface { id: surface_id });
        let tab = Tab::new_with_surface(tab_id, "Shell".to_string(), surface);
        Self {
            id,
            tabs: vec![tab],
            active_tab: 0,
            tab_scroll_offset: 0.0,
        }
    }

    /// Add a TerminalSurface-marker tab (active). Caller must have already
    /// inserted the spawned Terminal into the store.
    pub fn add_terminal_marker_tab(&mut self, tab_id: TabId, surface_id: SurfaceId) {
        let surface: Box<dyn super::Surface> = Box::new(TerminalSurface { id: surface_id });
        let tab = Tab::new_with_surface(tab_id, "Shell".to_string(), surface);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Same as [`add_terminal_marker_tab`] but does NOT change `active_tab`.
    pub fn add_terminal_marker_tab_background(
        &mut self,
        tab_id: TabId,
        surface_id: SurfaceId,
        explicit_name: Option<String>,
    ) {
        let surface: Box<dyn super::Surface> = Box::new(TerminalSurface { id: surface_id });
        let tab = Tab::new_named(tab_id, "Shell".to_string(), explicit_name, surface);
        self.tabs.push(tab);
    }

    /// Collect all surface IDs across all tabs in this pane.
    pub fn all_surface_ids(&self) -> Vec<SurfaceId> {
        let mut ids = Vec::new();
        for tab in &self.tabs {
            ids.extend(tab.all_surface_ids());
        }
        ids
    }

    /// 활성 tab 안의 모든 deferred placeholder를 spawn. 반환은
    /// `(surface_id, Terminal, persist_id)` 목록 — caller 가 store insert.
    pub fn ensure_active_tab_initialized_all(
        &mut self,
    ) -> Vec<(SurfaceId, Terminal, Option<String>)> {
        if self.tabs.is_empty() {
            return Vec::new();
        }
        let idx = self.active_tab.min(self.tabs.len() - 1);
        self.tabs[idx].ensure_all_initialized()
    }

    /// Split the active panel's focused surface with a TerminalSurface marker.
    /// Caller must have already inserted the spawned Terminal into the store.
    pub fn split_active_surface_marker(
        &mut self,
        direction: SplitDirection,
        new_surface_id: SurfaceId,
    ) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.active_tab.min(self.tabs.len() - 1);
        self.tabs[active].split_focused_surface(direction, new_surface_id);
    }

    /// Split a specific surface by ID with a TerminalSurface marker. Caller must
    /// have already inserted the spawned Terminal into the store.
    pub fn split_surface_by_id_marker(
        &mut self,
        target_surface_id: SurfaceId,
        direction: SplitDirection,
        new_surface_id: SurfaceId,
    ) -> anyhow::Result<()> {
        for tab in &mut self.tabs {
            if tab.contains_surface(target_surface_id) {
                tab.split_surface_by_id(target_surface_id, direction, new_surface_id);
                return Ok(());
            }
        }
        anyhow::bail!("surface {} not found in this pane", target_surface_id)
    }

    /// Split a specific surface by ID with any surface type (not just terminal).
    pub fn split_surface_by_id_with_surface(
        &mut self,
        target_surface_id: SurfaceId,
        direction: SplitDirection,
        new_surface: Box<dyn super::Surface>,
    ) -> anyhow::Result<()> {
        for tab in &mut self.tabs {
            if tab.contains_surface(target_surface_id) {
                tab.split_surface_by_id_generic(target_surface_id, direction, new_surface);
                return Ok(());
            }
        }
        anyhow::bail!("surface {} not found in this pane", target_surface_id)
    }

    /// Remove the tab at `tab_index`, keeping `active_tab` pointed at the **same
    /// tab** it pointed at before.
    ///
    /// `active_tab` is an index, so removing an earlier tab shifts every later
    /// tab down one slot and the untouched index silently starts naming a
    /// different tab. Only closing the active tab itself may move the view, and
    /// then it lands on the tab that slid into the slot (or the last one).
    /// See `docs/design/policies/focus.md`.
    pub fn remove_tab_preserving_active(&mut self, tab_index: usize) {
        if tab_index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(tab_index);
        if tab_index < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    /// Close the tab at the given index. Returns false if the tab can't be closed
    /// (e.g., it's the last tab).
    pub fn close_tab(&mut self, tab_index: usize) -> bool {
        if self.tabs.len() <= 1 {
            return false; // Can't close last tab
        }
        if tab_index < self.tabs.len() {
            self.remove_tab_preserving_active(tab_index);
            true
        } else {
            false
        }
    }

    /// Close the currently active tab. Returns false if it's the last tab.
    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_tab)
    }

    /// Close a tab by its ID. Returns false if not found or it's the last tab.
    pub fn close_tab_by_id(&mut self, tab_id: TabId) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.remove_tab_preserving_active(idx);
            true
        } else {
            false
        }
    }

    /// Check if any tab in this pane contains the given surface ID.
    pub fn contains_surface(&self, surface_id: SurfaceId) -> bool {
        self.tabs.iter().any(|tab| tab.contains_surface(surface_id))
    }

    /// Switch to tab by index (0-based).
    ///
    /// 세 갈래를 **따로** 돌려준다. 예전에는 `bool` 이었는데, 그러면 "안 바뀌었다" 와
    /// "못 바꾼다" 가 같은 `false` 로 눌린다 — 이미 보고 있는 탭으로 전환하는 것은
    /// 정상이고 바뀔 것이 없을 뿐인데, 호출자가 그것을 실패로 읽을 길이 없었다.
    /// 실제로 IPC 핸들러가 그 `false` 를 전부 범위 밖으로 적었고, 그 문구를 믿고
    /// "탭이 안 만들어졌다" 로 읽어 한 회차를 헛짚었다.
    pub fn goto_tab(&mut self, index: usize) -> TabSwitch {
        if index >= self.tabs.len() {
            TabSwitch::OutOfRange {
                tabs: self.tabs.len(),
            }
        } else if index == self.active_tab {
            TabSwitch::AlreadyActive
        } else {
            self.active_tab = index;
            TabSwitch::Switched
        }
    }

    /// Switch to next tab.
    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Switch to previous tab.
    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Add a tab with a Surface trait object and switch to it.
    pub fn add_surface_tab(
        &mut self,
        tab_id: TabId,
        name: String,
        explicit_name: Option<String>,
        surface: Box<dyn super::Surface>,
    ) {
        let tab = super::tab::Tab::new_named(tab_id, name, explicit_name, surface);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Same as [`add_surface_tab`](Self::add_surface_tab) but does NOT change `active_tab`.
    pub fn add_surface_tab_background(
        &mut self,
        tab_id: TabId,
        name: String,
        explicit_name: Option<String>,
        surface: Box<dyn super::Surface>,
    ) {
        let tab = super::tab::Tab::new_named(tab_id, name, explicit_name, surface);
        self.tabs.push(tab);
    }

    /// Get the active tab (mutable). Returns None if tabs are empty.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        if self.tabs.is_empty() {
            return None;
        }
        let idx = self.active_tab.min(self.tabs.len() - 1);
        Some(&mut self.tabs[idx])
    }

    /// Move a tab from one index to another, adjusting active_tab accordingly.
    /// Returns false if indices are out of bounds or equal.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        // Adjust active_tab to follow the moved tab or account for the shift
        if self.active_tab == from {
            self.active_tab = to;
        } else if from < to && self.active_tab > from && self.active_tab <= to {
            self.active_tab -= 1;
        } else if from > to && self.active_tab >= to && self.active_tab < from {
            self.active_tab += 1;
        }
        true
    }

    /// Produce a JSON tree representation of this pane.
    pub fn to_tree_json(&self) -> serde_json::Value {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let mut t = tab.to_tree_json();
                t["active"] = serde_json::json!(i == self.active_tab);
                t
            })
            .collect();
        serde_json::json!({
            "id": self.id,
            "tabs": tabs,
        })
    }

    /// attach 디스크립터용 pane JSON: `{"id", "tabs":[{"id","name","active",
    /// "focused_surface","layout"}, ...]}`. `Workspace::to_attach_tree_json`(평면
    /// "panes")과 `PaneNode::to_tree_json_full`(트리 Leaf)이 이 메서드를 공유해
    /// 두 표현이 서로 다른 pane 직렬화를 갖지 않도록 한다.
    pub fn to_attach_json(&self) -> serde_json::Value {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let layout = tab
                    .layout_if_initialized()
                    .map(|l| l.to_tree_json_full())
                    .unwrap_or(serde_json::Value::Null);
                serde_json::json!({
                    "id": tab.id,
                    "name": tab.display_name(),
                    "active": i == self.active_tab,
                    "focused_surface": tab.focused_surface,
                    "layout": layout,
                })
            })
            .collect();
        serde_json::json!({
            "id": self.id,
            "tabs": tabs,
        })
    }
}

#[cfg(test)]
mod goto_tab_branch_tests {
    //! 탭 전환이 **네 갈래를 안 눌러서** 돌려주는지 고정한다.
    //!
    //! 예전에는 `bool` 하나였다. 그러면 "이미 그 탭이라 바뀔 게 없다" 와 "그런 탭이
    //! 없다" 가 같은 `false` 라, IPC 핸들러가 둘 다 범위 밖으로 적었다 — 인덱스 0 이
    //! 범위 안인데도 "Tab index 0 out of range" 가 나갔고, 그 문구를 믿고 "탭이 안
    //! 만들어졌다" 로 읽어 한 회차를 헛짚었다. 그래서 여기서 고정하는 것은 값이 아니라
    //! **두 갈래가 서로 다르다는 것**이다.
    use super::*;

    /// 탭 3 개(id 10/11/12). `active_tab` 은 호출자가 정한다.
    fn pane_with_three_tabs(active: usize) -> Pane {
        let mut pane = Pane::new_with_terminal_marker(1, 10, 100);
        pane.add_terminal_marker_tab(11, 101);
        pane.add_terminal_marker_tab(12, 102);
        pane.active_tab = active;
        pane
    }

    #[test]
    fn moving_to_another_tab_switches() {
        let mut pane = pane_with_three_tabs(0);
        assert_eq!(pane.goto_tab(2), TabSwitch::Switched);
        assert_eq!(pane.active_tab, 2, "전환됐다면 활성 탭이 따라가야 한다");
    }

    #[test]
    fn moving_to_the_tab_already_shown_is_not_an_error() {
        let mut pane = pane_with_three_tabs(1);
        assert_eq!(
            pane.goto_tab(1),
            TabSwitch::AlreadyActive,
            "이미 보고 있는 탭으로 가는 것은 정상이다 — 바뀔 것이 없을 뿐이다"
        );
        assert_eq!(pane.active_tab, 1);
    }

    #[test]
    fn an_index_past_the_end_is_out_of_range_and_says_how_many_there_are() {
        let mut pane = pane_with_three_tabs(0);
        assert_eq!(
            pane.goto_tab(3),
            TabSwitch::OutOfRange { tabs: 3 },
            "탭 수를 함께 실어야 호출자가 자기 인덱스를 의심할지 대상을 의심할지 가른다"
        );
        assert_eq!(
            pane.active_tab, 0,
            "실패한 전환이 활성 탭을 움직이면 안 된다"
        );
    }

    /// ★ 이 결함의 본체 — 두 갈래가 **같은 값이면 안 된다**.
    ///
    /// 위 셋이 각각 통과해도 두 갈래가 같은 값으로 돌아오면 호출자는 여전히 못 가른다.
    /// 그 자리를 따로 못 박는다.
    #[test]
    fn already_active_and_out_of_range_are_told_apart() {
        let mut already = pane_with_three_tabs(0);
        let mut past_end = pane_with_three_tabs(0);
        assert_ne!(
            already.goto_tab(0),
            past_end.goto_tab(9),
            "안 바뀌었다와 못 바꾼다가 같은 값으로 나가면 실패문이 거짓말을 한다"
        );
    }
}

#[cfg(test)]
mod tab_removal_focus_tests {
    //! 탭 제거가 `active_tab` 을 **대상 기준으로** 보존하는지 고정한다.
    //!
    //! `active_tab` 은 인덱스가 진실 소스라, 앞쪽 탭이 빠지면 손대지 않은 인덱스가
    //! 다른 탭을 가리키게 된다 — 사용자가 아무 조작도 하지 않았는데 보던 탭이
    //! 바뀌는 것이라 불가침 원칙 1 위반이다. 단정은 인덱스가 아니라 **탭 id** 로 한다.
    use super::*;

    /// 탭 3 개(id 10/11/12)를 가진 pane. `active_tab` 은 호출자가 정한다.
    fn pane_with_three_tabs(active: usize) -> Pane {
        let mut pane = Pane::new_with_terminal_marker(1, 10, 100);
        pane.add_terminal_marker_tab(11, 101);
        pane.add_terminal_marker_tab(12, 102);
        pane.active_tab = active;
        pane
    }

    fn active_tab_id(pane: &Pane) -> TabId {
        pane.tabs[pane.active_tab].id
    }

    #[test]
    fn removing_an_earlier_tab_keeps_the_same_tab_active() {
        let mut pane = pane_with_three_tabs(1); // 사용자는 tab 11 을 본다
        pane.remove_tab_preserving_active(0);
        assert_eq!(
            active_tab_id(&pane),
            11,
            "앞쪽 탭 제거가 보던 탭을 바꾸면 안 된다"
        );
    }

    #[test]
    fn removing_a_later_tab_keeps_the_same_tab_active() {
        let mut pane = pane_with_three_tabs(1);
        pane.remove_tab_preserving_active(2);
        assert_eq!(active_tab_id(&pane), 11);
    }

    #[test]
    fn removing_the_active_tab_lands_on_the_tab_that_slid_in() {
        let mut pane = pane_with_three_tabs(1);
        pane.remove_tab_preserving_active(1);
        assert_eq!(
            active_tab_id(&pane),
            12,
            "보던 탭 자체를 닫으면 그 자리로 밀려 들어온 탭으로 간다"
        );
    }

    #[test]
    fn removing_the_active_last_tab_falls_back_to_the_previous_one() {
        let mut pane = pane_with_three_tabs(2);
        pane.remove_tab_preserving_active(2);
        assert_eq!(active_tab_id(&pane), 11);
    }

    #[test]
    fn close_tab_by_index_preserves_the_active_tab() {
        // active 는 **가운데**(1)여야 한다. 마지막(2)이면 수정 전의 범위 초과 clamp
        // 로도 우연히 같은 탭에 착지해 이 wrapper 가 헬퍼를 타는지 판별하지 못한다.
        let mut pane = pane_with_three_tabs(1); // 사용자는 tab 11 을 본다
        assert!(pane.close_tab(0));
        assert_eq!(active_tab_id(&pane), 11);
    }

    #[test]
    fn close_tab_by_id_preserves_the_active_tab() {
        let mut pane = pane_with_three_tabs(1);
        assert!(pane.close_tab_by_id(10));
        assert_eq!(active_tab_id(&pane), 11);
    }

    #[test]
    fn removing_an_out_of_range_index_is_a_noop() {
        let mut pane = pane_with_three_tabs(1);
        pane.remove_tab_preserving_active(9);
        assert_eq!(pane.tabs.len(), 3);
        assert_eq!(active_tab_id(&pane), 11);
    }
}
