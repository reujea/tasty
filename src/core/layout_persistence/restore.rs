//! `SavedLayout` → live `CoreState` 복원.
//!
//! Plugin surface (`SavedSurface::Generic`) 는 그 kind 가 registry 에 등록된 후에만
//! 복원 가능. 호출자가 `required_plugin_kinds()` 로 미리 필요한 kind 목록을 받아 plugin
//! pump 를 기다린 뒤 `restore()` 를 호출한다.

// 아래 가져오기는 `restore` 계열(전부 `any(gui, test)`)만 쓴다 — headless 라이브러리
// 조합에서는 쓰는 자리가 없어 같은 cfg 로 가른다.
#[cfg(any(feature = "gui", test))]
use std::path::PathBuf;

#[cfg(any(feature = "gui", test))]
use crate::core::CoreState;
#[cfg(any(feature = "gui", test))]
use crate::core::state::ShellConfig;
#[cfg(any(feature = "gui", test))]
use crate::model::{Pane, PaneNode, Surface, SurfaceLayout, Tab, TerminalSurface, Workspace};

use super::schema::{
    SavedLayout, SavedPane, SavedPaneNode, SavedSurface, SavedSurfaceLayout, SavedTab,
    SavedWorkspace,
};
#[cfg(any(feature = "gui", test))]
use super::scrollback::queue_scrollback_for_surface;

impl SavedLayout {
    /// Layout 안의 모든 Generic surface kind 토큰을 수집. 호출자는 첫 plugin pump
    /// 후에 registry에 이 kind들이 등록됐는지 확인하여 복원 시점을 결정한다.
    #[cfg(feature = "gui")]
    pub fn required_plugin_kinds(&self) -> Vec<String> {
        let mut kinds = std::collections::HashSet::new();
        for ws in &self.workspaces {
            Self::collect_kinds_in_pane(&ws.pane_layout, &mut kinds);
        }
        kinds.into_iter().collect()
    }

    /// 이 레이아웃(=슬롯 하나) 안의 모든
    /// `SavedSurface::Terminal { scrollback_ref: Some(_) }` 값을 모은다.
    ///
    /// **이 집합은 삭제 판정의 주체가 아니라 union 의 한 항이다.** scrollback GC 는
    /// 부팅 1 회, 전 슬롯 집합의 **합집합**으로만 돈다
    /// (`layout_persistence::gc_scrollback_orphans_all_slots`). 슬롯 하나의 집합으로
    /// GC 하면 다른 슬롯이 참조하는 `.bin` 을 전부 orphan 으로 판정해 지운다.
    pub fn collect_scrollback_refs(&self) -> std::collections::HashSet<String> {
        let mut refs = std::collections::HashSet::new();
        for ws in &self.workspaces {
            Self::collect_scrollback_refs_in_pane(&ws.pane_layout, &mut refs);
        }
        refs
    }

    fn collect_scrollback_refs_in_pane(
        node: &SavedPaneNode,
        out: &mut std::collections::HashSet<String>,
    ) {
        match node {
            SavedPaneNode::Leaf(pane) => {
                for tab in &pane.tabs {
                    Self::collect_scrollback_refs_in_layout(&tab.surface, out);
                }
            }
            SavedPaneNode::Split { first, second, .. } => {
                Self::collect_scrollback_refs_in_pane(first, out);
                Self::collect_scrollback_refs_in_pane(second, out);
            }
        }
    }

    fn collect_scrollback_refs_in_layout(
        layout: &SavedSurfaceLayout,
        out: &mut std::collections::HashSet<String>,
    ) {
        match layout {
            SavedSurfaceLayout::Leaf(SavedSurface::Terminal {
                scrollback_ref: Some(id),
                ..
            }) => {
                out.insert(id.clone());
            }
            SavedSurfaceLayout::Leaf(_) => {}
            SavedSurfaceLayout::Split { first, second, .. } => {
                Self::collect_scrollback_refs_in_layout(first, out);
                Self::collect_scrollback_refs_in_layout(second, out);
            }
        }
    }

    #[cfg(feature = "gui")]
    fn collect_kinds_in_pane(node: &SavedPaneNode, out: &mut std::collections::HashSet<String>) {
        match node {
            SavedPaneNode::Leaf(pane) => {
                for tab in &pane.tabs {
                    Self::collect_kinds_in_layout(&tab.surface, out);
                }
            }
            SavedPaneNode::Split { first, second, .. } => {
                Self::collect_kinds_in_pane(first, out);
                Self::collect_kinds_in_pane(second, out);
            }
        }
    }

    #[cfg(feature = "gui")]
    fn collect_kinds_in_layout(
        layout: &SavedSurfaceLayout,
        out: &mut std::collections::HashSet<String>,
    ) {
        match layout {
            SavedSurfaceLayout::Leaf(SavedSurface::Generic { kind, .. }) => {
                out.insert(kind.clone());
            }
            SavedSurfaceLayout::Leaf(_) => {}
            SavedSurfaceLayout::Split { first, second, .. } => {
                Self::collect_kinds_in_layout(first, out);
                Self::collect_kinds_in_layout(second, out);
            }
        }
    }

    /// Restore layout into engine state. Returns true on success.
    /// On failure, engine state is left unchanged (caller should create default workspace).
    #[cfg(any(feature = "gui", test))]
    pub fn restore(self, engine: &mut CoreState) -> bool {
        if self.workspaces.is_empty() {
            return false;
        }

        let active_idx = self.active_workspace.min(self.workspaces.len() - 1);
        // 카테고리 복원 — 구버전(필드 없음)은 비어 있어 ensure_normal_category 가
        // normal 단일로 마이그레이션한다. ws 의 category 가 가리키는 대상이 없으면
        // 동일 함수가 normal 로 귀속한다(§4-1 무손실 마이그레이션).
        let categories: Vec<crate::model::WorkspaceCategory> = self
            .categories
            .into_iter()
            .map(|c| crate::model::WorkspaceCategory {
                id: c.id,
                name: c.name,
                collapsed: c.collapsed,
            })
            .collect();
        let mut workspaces = Vec::new();
        for (i, saved_ws) in self.workspaces.into_iter().enumerate() {
            let name = saved_ws.name.clone();
            let is_active = i == active_idx;
            match saved_ws.restore(engine, is_active) {
                Some(ws) => workspaces.push(ws),
                None => {
                    tracing::warn!("Failed to restore workspace '{}', skipping", name);
                }
            }
        }

        if workspaces.is_empty() {
            return false;
        }

        let active = self.active_workspace.min(workspaces.len() - 1);
        engine.workspaces = workspaces;
        engine.categories = categories;
        // normal 0번 고정 + 발급기 floor + dangling category 귀속 정규화.
        engine.ensure_normal_category();
        engine.restored_active_workspace = Some(active);
        true
    }
}

impl SavedWorkspace {
    #[cfg(any(feature = "gui", test))]
    fn restore(self, engine: &mut CoreState, is_active: bool) -> Option<Workspace> {
        let ws_id = engine.next_ids.next_workspace();
        let pane_layout = self.pane_layout.restore(engine, is_active)?;

        // Resolve focused pane by index.
        let all_ids = pane_layout.all_pane_ids();
        let focused_pane = all_ids
            .get(self.focused_pane_index)
            .copied()
            .or_else(|| all_ids.first().copied())
            .unwrap_or(0);

        let mut ws =
            Workspace::from_restored(ws_id, self.name, self.subtitle, pane_layout, focused_pane);
        // 단계 7 — 매핑 복원(재시작 후 활성화 시 자동 재attach). 생성자 churn 0(setter).
        ws.set_attach_mapping(self.attach_mapping);
        // 카테고리 소속 복원(구버전은 serde default 0=normal). dangling 은 restore
        // 말미의 ensure_normal_category 가 normal 로 귀속.
        ws.set_category(self.category);
        Some(ws)
    }
}

impl SavedPaneNode {
    #[cfg(any(feature = "gui", test))]
    fn restore(self, engine: &mut CoreState, is_active: bool) -> Option<PaneNode> {
        match self {
            SavedPaneNode::Leaf(saved_pane) => {
                let pane = saved_pane.restore(engine, is_active)?;
                Some(PaneNode::Leaf(pane))
            }
            SavedPaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first = first.restore(engine, is_active)?;
                let second = second.restore(engine, is_active)?;
                Some(PaneNode::Split {
                    direction: direction.into(),
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                })
            }
        }
    }
}

impl SavedPane {
    #[cfg(any(feature = "gui", test))]
    fn restore(self, engine: &mut CoreState, is_active_workspace: bool) -> Option<Pane> {
        let pane_id = engine.next_ids.next_pane();
        let saved_active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
        let tab_count = self.tabs.len();
        let mut tabs = Vec::new();
        for (idx, saved_tab) in self.tabs.into_iter().enumerate() {
            // 활성 workspace 안에서도 사용자가 보고 있는 active_tab만 즉시 PTY spawn.
            // 나머지 tab은 비활성 workspace와 동일하게 deferred — tab 전환 시 깨워짐.
            let tab_is_active = is_active_workspace && idx == saved_active_tab;
            match saved_tab.restore(engine, tab_is_active) {
                Some(tab) => tabs.push(tab),
                None => {
                    // 어느 탭이 사라졌는지 알 수 있어야 사용자 문의를 추적할 수 있다.
                    tracing::warn!(
                        "failed to restore tab {}/{} in pane {pane_id} — skipping it; \
                         the surface kind may be unregistered (plugin not loaded?)",
                        idx + 1,
                        tab_count,
                    );
                }
            }
        }
        if tabs.is_empty() {
            return None;
        }
        let active_tab = saved_active_tab.min(tabs.len() - 1);
        Some(Pane {
            id: pane_id,
            tabs,
            active_tab,
            tab_scroll_offset: 0.0,
        })
    }
}

impl SavedTab {
    #[cfg(any(feature = "gui", test))]
    fn restore(self, engine: &mut CoreState, is_active: bool) -> Option<Tab> {
        let tab_id = engine.next_ids.next_tab();
        let layout = self.surface.restore(engine, is_active)?;
        let focused_surface = layout.first_surface_id().unwrap_or(0);
        Some(Tab {
            id: tab_id,
            name: self.name,
            explicit_name: self.explicit_name,
            layout_opt: Some(layout),
            focused_surface,
            osc_title: None,
            cached_display_name: None,
        })
    }
}

impl SavedSurfaceLayout {
    /// is_active=false면 Terminal leaf를 deferred EmptySurface placeholder로 변환한다.
    /// is_active=true면 모든 leaf를 즉시 spawn한다. Split 노드는 재귀적으로 처리해
    /// 비활성 split 내부의 Terminal들도 deferred로 남는다.
    #[cfg(any(feature = "gui", test))]
    fn restore(self, engine: &mut CoreState, is_active: bool) -> Option<SurfaceLayout> {
        match self {
            SavedSurfaceLayout::Leaf(saved) => {
                let surface = saved.restore_leaf(engine, is_active)?;
                Some(SurfaceLayout::Leaf(surface))
            }
            SavedSurfaceLayout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first = first.restore(engine, is_active)?;
                let second = second.restore(engine, is_active)?;
                Some(SurfaceLayout::Split {
                    direction: direction.into(),
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                    focus_second: false,
                })
            }
        }
    }
}

impl SavedSurface {
    /// 단일 leaf 복원. Terminal이면 is_active에 따라 즉시 spawn 또는 deferred placeholder.
    /// Generic surface는 is_active와 관계없이 즉시 복원 (PTY가 아니므로 cheap).
    #[cfg(any(feature = "gui", test))]
    fn restore_leaf(self, engine: &mut CoreState, is_active: bool) -> Option<Box<dyn Surface>> {
        let surface_id = engine.next_ids.next_surface();
        match self {
            SavedSurface::Terminal {
                cwd,
                restore_command,
                scrollback_ref,
            } if !is_active => {
                let sh = ShellConfig::from_settings(&engine.settings);
                let waker = engine.make_waker(surface_id);
                // capture 단계가 surface_meta 의 restore.command 를 읽으므로
                // (capture_surface 의 deferred 분기 참조), DeferredSpawn 으로 옮기기
                // 전에 동일 값을 meta 에도 mirror 한다.
                if let Some(cmd) = restore_command.as_deref() {
                    let mut guard = crate::poison::recover_mutex(
                        engine.memory.lock(),
                        crate::core::MEMORY_WHAT,
                        &crate::core::MEMORY_POISONED,
                    );
                    if let Err(e) = crate::surface_meta::SurfaceMetaStore::set(
                        &mut *guard,
                        surface_id,
                        "restore.command",
                        cmd,
                    ) {
                        tracing::warn!(
                            "restore: failed to mirror restore.command for surface {surface_id}: {e}"
                        );
                    }
                }
                // scrollback_ref 가 있으면 PTY spawn 시점에 inject 할 라인을 큐에 쌓고,
                // 동일한 persist_id 를 DeferredSpawn 에도 들고 있어 spawn 후 새
                // TerminalSurface 의 scrollback_persist_id 필드로 이관한다.
                if let Some(persist_id) = scrollback_ref.as_deref() {
                    queue_scrollback_for_surface(engine, surface_id, persist_id);
                }
                let spawn = crate::model::DeferredSpawn {
                    shell: sh.shell_ref().map(|s| s.to_string()),
                    shell_args: sh.args_ref().iter().map(|s| s.to_string()).collect(),
                    extra_env: sh
                        .envs_ref()
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    cols: engine.default_cols,
                    rows: engine.default_rows,
                    waker,
                    working_dir: cwd.as_ref().map(PathBuf::from),
                    // PTY 가 실제로 spawn 되는 순간 inline 으로 send_key 된다 (ensure_initialized).
                    restore_command,
                    scrollback_persist_id: scrollback_ref,
                };
                let placeholder = crate::model::EmptySurface::new_deferred(surface_id, spawn);
                Some(Box::new(placeholder))
            }
            other => other.restore_immediate_inner(engine, surface_id),
        }
    }

    /// 항상 즉시 PTY를 spawn하거나 generic surface를 만들어 반환.
    #[cfg(any(feature = "gui", test))]
    fn restore_immediate_inner(
        self,
        engine: &mut CoreState,
        surface_id: u32,
    ) -> Option<Box<dyn Surface>> {
        match self {
            SavedSurface::Terminal {
                cwd,
                restore_command,
                scrollback_ref,
            } => {
                restore_terminal_immediate(engine, surface_id, cwd, restore_command, scrollback_ref)
            }
            SavedSurface::Generic { kind, data } => {
                restore_generic_immediate(engine, surface_id, kind, data)
            }
        }
    }
}

#[cfg(any(feature = "gui", test))]
fn restore_terminal_immediate(
    engine: &mut CoreState,
    surface_id: u32,
    cwd: Option<String>,
    restore_command: Option<String>,
    scrollback_ref: Option<String>,
) -> Option<Box<dyn Surface>> {
    let sh = ShellConfig::from_settings(&engine.settings);
    let waker = engine.make_waker(surface_id);
    let working_dir = cwd.as_ref().map(PathBuf::from);
    // PTY master 의 첫 입력으로 restore_command 를 미리 적재한다.
    // Terminal::new 가 writer thread spawn 전에 동기 write 하므로,
    // child shell 이 stdin 을 처음 read 하는 순간 이 바이트가 들어간다.
    let initial = restore_command.as_deref().map(|c| format!("{c}\r"));
    let initial_input = initial.as_deref();
    let mut terminal = match tasty_terminal::Terminal::new(
        tasty_terminal::TerminalConfig {
            cols: engine.default_cols,
            rows: engine.default_rows,
            shell: sh.shell_ref(),
            args: &sh.args_ref(),
            extra_env: &sh.envs_ref(),
            surface_id,
            working_dir: working_dir.as_deref(),
            initial_input,
        },
        waker,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to create terminal for restored surface: {e}");
            return None;
        }
    };
    // 즉시 복원 경로 — scrollback 을 inline 으로 inject. persist_id 는
    // 새 TerminalSurface 의 필드에 직접 들어가 (surface_meta mirror 없이)
    // 다음 capture 가 같은 ID 를 재사용한다.
    if let Some(persist_id) = scrollback_ref.as_deref()
        && let crate::scrollback_store::ScrollbackRead::Loaded(lines) =
            crate::scrollback_store::read(persist_id)
        && !lines.is_empty()
    {
        terminal.inject_scrollback(lines);
        // 새 prompt 가 화면 중간부터 시작하도록 visible 상단
        // 절반에 옛 라인을 미리 그려둔다.
        let prefill = terminal.rows() / 2;
        terminal.prefill_visible_from_scrollback(prefill);
    }
    engine.terminals.insert(surface_id, terminal);
    if let Some(pid) = scrollback_ref {
        engine.terminals.set_scrollback_persist_id(surface_id, pid);
    }
    engine.send_fast_init(surface_id);
    Some(Box::new(TerminalSurface { id: surface_id }))
}

#[cfg(any(feature = "gui", test))]
fn restore_generic_immediate(
    engine: &mut CoreState,
    surface_id: u32,
    kind: String,
    data: serde_json::Value,
) -> Option<Box<dyn Surface>> {
    let registry = engine.surface_registry.clone();
    let def = match registry.get_live(&kind) {
        Some(d) => d,
        None => {
            tracing::warn!(
                "Generic restore skipped: unknown kind '{}' (plugin not loaded?)",
                kind
            );
            return None;
        }
    };
    match (def.restore)(surface_id, &data) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!("Generic restore failed (kind={kind}): {e}");
            None
        }
    }
}
