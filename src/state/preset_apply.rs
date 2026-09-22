//! Preset → live workspace/tab/pane 적용.
//!
//! `tasty-presets` 의 데이터 모델을 받아 엔진의 mutable 상태에 instance 를 끼워넣는다.
//! `src/core/restore_rebuild.rs` 의 rebuild_* helpers 와 같은 패턴 (initial_input 기반 startup
//! command 주입, send_fast_init, ratio clamp 등). 단, focus 정책은 `ApplyOptions.focus`
//! 로 명시 — CLI/IPC 는 false (포커스 독립), 단축키 호출만 true.

use tasty_presets::{
    PanePreset, PresetPane, PresetPaneNode, PresetSplitDirection, PresetSurface,
    PresetSurfaceLayout, PresetTab, TabPreset, WorkspacePreset,
};

use crate::core::CoreState;
use crate::model::{
    DeferredPlugin, EmptySurface, Pane, PaneNode, SplitDirection, Surface, SurfaceLayout, Tab,
    TerminalSurface, Workspace,
};

/// PresetSplitDirection → live SplitDirection. presets crate 는 외부 enum 을 모르므로
/// 본 바이너리 측에서 변환한다.
fn from_preset_split(d: PresetSplitDirection) -> SplitDirection {
    match d {
        PresetSplitDirection::Horizontal => SplitDirection::Horizontal,
        PresetSplitDirection::Vertical => SplitDirection::Vertical,
    }
}

use super::AppState;

/// Apply 호출자의 의도. focus 가 true 면 새 ws/tab/pane 으로 활성 전환.
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions {
    pub focus: bool,
}

#[derive(Debug)]
pub enum ApplyError {
    PaneNotFound(u32),
    WorkspaceNotFound(u32),
    Empty,
    NoActiveWorkspace,
    TerminalSpawn(String),
    Other(anyhow::Error),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PaneNotFound(id) => write!(f, "target pane not found: {id}"),
            Self::WorkspaceNotFound(id) => write!(f, "target workspace not found: {id}"),
            Self::Empty => write!(f, "preset has no usable leaves"),
            Self::NoActiveWorkspace => {
                write!(f, "no active workspace to apply tab/pane preset")
            }
            Self::TerminalSpawn(e) => write!(f, "terminal spawn failed: {e}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for ApplyError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl AppState {
    /// WorkspacePreset 적용 — 새 워크스페이스를 push.
    /// `category` 가 `Some` 이고 존재하는 카테고리면 그 소속으로 지정한다(없거나
    /// dangling 이면 normal 유지 — `apply_create_workspace_inner` 와 동일 정책).
    /// 반환은 새 워크스페이스의 인덱스.
    pub fn apply_workspace_preset(
        &mut self,
        engine: &mut CoreState,
        preset: &WorkspacePreset,
        category: Option<crate::model::WorkspaceCategoryId>,
        opts: ApplyOptions,
    ) -> Result<usize, ApplyError> {
        let ws_id = engine.next_ids.next_workspace();
        let pane_node = self.build_pane_node(engine, &preset.layout)?;

        let all_pane_ids = pane_node.all_pane_ids();
        let focused = *all_pane_ids.first().ok_or(ApplyError::Empty)?;

        let name = if preset.name.is_empty() {
            format!("Workspace {}", engine.workspaces.len() + 1)
        } else {
            preset.name.clone()
        };

        let mut ws =
            Workspace::from_restored(ws_id, name, preset.subtitle.clone(), pane_node, focused);
        ws.description = preset.description.clone();
        engine.workspaces.push(ws);
        let idx = engine.workspaces.len() - 1;

        if let Some(cat_id) = category
            && let Err(e) = engine.set_workspace_category(ws_id, cat_id)
        {
            tracing::warn!("apply_workspace_preset: set_workspace_category failed: {e:?}");
        }

        if opts.focus {
            self.active_workspace = idx;
        }
        engine.mark_layout_dirty();
        Ok(idx)
    }

    /// TabPreset 적용 — `target_pane_id` 가 Some 이면 해당 pane, None 이면 active workspace
    /// 의 focused_pane 에 새 탭을 push. 반환은 새 tab_id.
    pub fn apply_tab_preset(
        &mut self,
        engine: &mut CoreState,
        preset: &TabPreset,
        target_pane_id: Option<u32>,
        opts: ApplyOptions,
    ) -> Result<u32, ApplyError> {
        let (ws_idx, pane_id) = self.resolve_target_pane(engine, target_pane_id)?;

        let tab = self.build_tab(engine, &preset.tab)?;
        let tab_id = tab.id;

        let ws = &mut engine.workspaces[ws_idx];
        let pane = ws
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .ok_or(ApplyError::PaneNotFound(pane_id))?;
        pane.tabs.push(tab);
        let new_idx = pane.tabs.len() - 1;
        if opts.focus {
            pane.active_tab = new_idx;
        }

        engine.mark_layout_dirty();
        Ok(tab_id)
    }

    /// PanePreset 적용 — `target_workspace_id` 가 Some 이면 해당 ws, None 이면 active.
    /// 현재 ws 의 focused_pane 오른쪽에 vertical split 으로 새 pane 추가.
    /// 반환은 새 pane_id.
    pub fn apply_pane_preset(
        &mut self,
        engine: &mut CoreState,
        preset: &PanePreset,
        target_workspace_id: Option<u32>,
        opts: ApplyOptions,
    ) -> Result<u32, ApplyError> {
        let ws_idx = match target_workspace_id {
            Some(id) => engine
                .find_workspace_index_for_id(id)
                .ok_or(ApplyError::WorkspaceNotFound(id))?,
            None => {
                if engine.workspaces.is_empty() {
                    return Err(ApplyError::NoActiveWorkspace);
                }
                self.active_workspace.min(engine.workspaces.len() - 1)
            }
        };

        let new_pane = self.build_pane(engine, &preset.pane)?;
        let new_pane_id = new_pane.id;

        let ws = &mut engine.workspaces[ws_idx];
        let target_pane_id = ws.focused_pane;
        let remaining = ws.pane_layout_mut().split_pane_in_place(
            target_pane_id,
            SplitDirection::Vertical,
            new_pane,
        );
        if remaining.is_some() {
            // focused_pane 이 stale 이면 첫 leaf 로 fallback.
            let fallback = ws.pane_layout().first_pane().map(|p| p.id);
            if let (Some(pane), Some(fb_id)) = (remaining, fallback) {
                let remaining2 =
                    ws.pane_layout_mut()
                        .split_pane_in_place(fb_id, SplitDirection::Vertical, pane);
                if remaining2.is_some() {
                    return Err(ApplyError::PaneNotFound(target_pane_id));
                }
            } else {
                return Err(ApplyError::PaneNotFound(target_pane_id));
            }
        }

        if opts.focus {
            ws.focused_pane = new_pane_id;
        }

        engine.mark_layout_dirty();
        Ok(new_pane_id)
    }

    // ── 내부 helpers ─────────────────────────────────────────────────────

    fn resolve_target_pane(
        &self,
        engine: &CoreState,
        target_pane_id: Option<u32>,
    ) -> Result<(usize, u32), ApplyError> {
        if engine.workspaces.is_empty() {
            return Err(ApplyError::NoActiveWorkspace);
        }
        if let Some(pid) = target_pane_id {
            let ws_idx = engine
                .find_workspace_index_for_pane(pid)
                .ok_or(ApplyError::PaneNotFound(pid))?;
            return Ok((ws_idx, pid));
        }
        let ws_idx = self.active_workspace.min(engine.workspaces.len() - 1);
        let ws = &engine.workspaces[ws_idx];
        let pid = ws.focused_pane;
        if ws.pane_layout().find_pane(pid).is_some() {
            return Ok((ws_idx, pid));
        }
        let first = ws
            .pane_layout()
            .first_pane()
            .map(|p| p.id)
            .ok_or(ApplyError::Empty)?;
        Ok((ws_idx, first))
    }

    fn build_pane_node(
        &mut self,
        engine: &mut CoreState,
        node: &PresetPaneNode,
    ) -> Result<PaneNode, ApplyError> {
        match node {
            PresetPaneNode::Leaf { pane } => {
                let p = self.build_pane(engine, pane)?;
                Ok(PaneNode::Leaf(p))
            }
            PresetPaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let f = self.build_pane_node(engine, first)?;
                let s = self.build_pane_node(engine, second)?;
                Ok(PaneNode::Split {
                    direction: from_preset_split(*direction),
                    ratio: ratio.clamp(0.05, 0.95),
                    first: Box::new(f),
                    second: Box::new(s),
                })
            }
        }
    }

    fn build_pane(
        &mut self,
        engine: &mut CoreState,
        preset: &PresetPane,
    ) -> Result<Pane, ApplyError> {
        if preset.tabs.is_empty() {
            return Err(ApplyError::Empty);
        }
        let pane_id = engine.next_ids.next_pane();
        let mut tabs = Vec::with_capacity(preset.tabs.len());
        for preset_tab in &preset.tabs {
            tabs.push(self.build_tab(engine, preset_tab)?);
        }
        let active_tab = preset.active_tab.min(tabs.len() - 1);
        Ok(Pane {
            id: pane_id,
            tabs,
            active_tab,
            tab_scroll_offset: 0.0,
        })
    }

    fn build_tab(&mut self, engine: &mut CoreState, preset: &PresetTab) -> Result<Tab, ApplyError> {
        let tab_id = engine.next_ids.next_tab();
        let layout = self.build_surface_layout(engine, &preset.layout)?;
        let focused_surface = layout.first_surface_id().ok_or(ApplyError::Empty)?;

        let auto_name = preset_default_tab_name(engine, &preset.layout);
        let name = preset
            .explicit_name
            .clone()
            .unwrap_or_else(|| auto_name.clone());

        match layout {
            SurfaceLayout::Leaf(surface) => {
                let mut tab = Tab::new_with_surface(tab_id, name, surface);
                tab.explicit_name = preset.explicit_name.clone();
                Ok(tab)
            }
            split @ SurfaceLayout::Split { .. } => Ok(Tab {
                id: tab_id,
                name,
                explicit_name: preset.explicit_name.clone(),
                layout_opt: Some(split),
                focused_surface,
                osc_title: None,
                cached_display_name: None,
            }),
        }
    }

    fn build_surface_layout(
        &mut self,
        engine: &mut CoreState,
        preset: &PresetSurfaceLayout,
    ) -> Result<SurfaceLayout, ApplyError> {
        match preset {
            PresetSurfaceLayout::Leaf { surface } => {
                let s = self.build_leaf_surface(engine, surface)?;
                Ok(SurfaceLayout::Leaf(s))
            }
            PresetSurfaceLayout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let f = self.build_surface_layout(engine, first)?;
                let s = self.build_surface_layout(engine, second)?;
                Ok(SurfaceLayout::Split {
                    direction: from_preset_split(*direction),
                    ratio: ratio.clamp(0.05, 0.95),
                    first: Box::new(f),
                    second: Box::new(s),
                    focus_second: false,
                })
            }
        }
    }

    fn build_leaf_surface(
        &mut self,
        engine: &mut CoreState,
        preset: &PresetSurface,
    ) -> Result<Box<dyn Surface>, ApplyError> {
        let surface_id = engine.next_ids.next_surface();
        if preset.kind == "terminal" {
            let terminal = self.build_terminal(engine, surface_id, preset)?;
            engine.terminals.insert(surface_id, terminal);
            engine.send_fast_init(surface_id);
            return Ok(Box::new(TerminalSurface { id: surface_id }));
        }

        let Some(def) = engine.surface_registry.get_live(&preset.kind) else {
            // 미등록 kind 를 restore(rebuild_surface)와 동형으로 deferred plugin placeholder 로
            // 흡수한다. Err 전파는 build_surface_layout 의 Split 에서 `?` 로 퍼져 이 leaf 의
            // 형제(무고한 terminal 포함)까지 통째로 버린다 — 같은 kind miss 를 restore 는
            // 흡수하고 apply 는 거부하면 그 자체가 결함이다.
            //
            // ★ restore 와 다른 점을 명시한다(다음 사람이 "restore 와 같으니 reify 가
            // 반드시 온다"로 읽으면 틀린다): restore 의 kind miss 는 plugin hello 전 **일시
            // 부재**라 reify(`reify_displayed_surfaces`)가 뒤따른다. apply 의 미등록은 plugin
            // 미설치·제거인 **영구 부재일 수 있어** placeholder 가 빈 채 남을 수 있다(reify 가
            // 안 올 수 있다). 그래도 형제 유실(회복 불가)보다 빈 placeholder 탭(사용자가 닫으면
            // 되는 회복 가능)이 낫고, placeholder 는 화면에 보이므로 조용한 손실도 아니다.
            return Ok(Box::new(EmptySurface::new_deferred_plugin(
                surface_id,
                DeferredPlugin {
                    kind: preset.kind.clone(),
                    snapshot: preset.params.clone(),
                },
            )));
        };
        // cwd 우선순위: 명시된 preset.cwd > derive_cwd file_path 필드의 부모 디렉토리.
        // 사용자 요구("파일 kind 의 cwd 는 파일이 있는 폴더")를 apply 시점에 실현한다.
        let cwd = preset
            .cwd
            .as_ref()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                crate::core::surface_registry::PresetFieldSpec::derive_cwd(
                    &def.preset_fields,
                    &preset.params,
                )
            });
        engine
            .create_surface_via_registry(&preset.kind, surface_id, cwd.as_deref(), &preset.params)
            .map_err(ApplyError::Other)
    }

    fn build_terminal(
        &self,
        engine: &CoreState,
        surface_id: u32,
        preset: &PresetSurface,
    ) -> Result<tasty_terminal::Terminal, ApplyError> {
        let cols = engine.default_cols;
        let rows = engine.default_rows;
        let shell_string = engine.settings.general.shell.clone();
        let shell = if shell_string.is_empty() {
            None
        } else {
            Some(shell_string)
        };
        let shell_args_owned = engine.settings.general.effective_shell_args();
        let shell_args: Vec<&str> = shell_args_owned.iter().map(|s| s.as_str()).collect();
        let shell_envs_owned = engine.settings.general.effective_shell_envs();
        let shell_envs: Vec<(&str, &str)> = shell_envs_owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let waker = engine.make_waker(surface_id);

        // restore.rs:163-184 와 동일 — cwd 와 startup_command 를 합쳐 PTY 첫 입력으로 주입.
        let mut initial = String::new();
        if let Some(dir) = preset.cwd.as_deref() {
            initial.push_str(&format!("cd {}\r", shell_escape(dir)));
        }
        if let Some(cmd) = preset.startup_command.as_deref() {
            let trimmed = cmd.trim();
            if !trimmed.is_empty() {
                initial.push_str(trimmed);
                initial.push('\r');
            }
        }
        let initial_input = if initial.is_empty() {
            None
        } else {
            Some(initial.as_str())
        };

        tasty_terminal::Terminal::new(
            tasty_terminal::TerminalConfig {
                cols,
                rows,
                shell: shell.as_deref(),
                args: &shell_args,
                extra_env: &shell_envs,
                surface_id,
                working_dir: None,
                initial_input,
            },
            waker,
        )
        .map_err(|e| ApplyError::TerminalSpawn(e.to_string()))
    }
}

/// preset leaf 의 kind 로부터 자동 탭 이름 도출.
/// `core/surface_registry.rs::default_tab_name_for_kind` 와 동일 정책이지만 PresetSurfaceLayout
/// 트리에서 첫 leaf 를 직접 찾는다.
fn preset_default_tab_name(engine: &CoreState, layout: &PresetSurfaceLayout) -> String {
    let first = first_preset_leaf(layout);
    crate::core::surface_registry::default_tab_name_for_kind(
        &first.kind,
        &first.params,
        engine.surface_registry.get(&first.kind).as_deref(),
    )
}

fn first_preset_leaf(layout: &PresetSurfaceLayout) -> &PresetSurface {
    match layout {
        PresetSurfaceLayout::Leaf { surface } => surface,
        PresetSurfaceLayout::Split { first, .. } => first_preset_leaf(first),
    }
}

/// shell 안전 escape (restore.rs:282 와 동일).
fn shell_escape(s: &str) -> String {
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::ApplyOptions;
    use crate::core::CoreState;
    use crate::core::surface_registry::{PresetFieldInput, PresetFieldSpec, PresetFieldTarget};
    use serde_json::json;
    use tasty_presets::{
        PresetPane, PresetPaneNode, PresetSurface, PresetSurfaceLayout, PresetTab, WorkspacePreset,
    };

    fn file_field(param_key: &str, derive: bool, input: PresetFieldInput) -> PresetFieldSpec {
        PresetFieldSpec {
            id: "file".to_string(),
            label_key: "l".to_string(),
            target: PresetFieldTarget::Params(param_key.to_string()),
            input,
            required: true,
            placeholder_key: None,
            default: None,
            derive_cwd: derive,
        }
    }

    #[test]
    fn derives_parent_of_file_path_field() {
        let fields = vec![file_field("file", true, PresetFieldInput::FilePath)];
        let params = json!({ "file": "/a/b/x.md" });
        let cwd = PresetFieldSpec::derive_cwd(&fields, &params).unwrap();
        assert_eq!(cwd, std::path::PathBuf::from("/a/b"));
    }

    #[test]
    fn no_derive_when_flag_off_or_wrong_input() {
        let params = json!({ "file": "/a/b/x.md" });
        // derive_cwd = false → 파생 안 함.
        let off = vec![file_field("file", false, PresetFieldInput::FilePath)];
        assert!(PresetFieldSpec::derive_cwd(&off, &params).is_none());
        // url 은 파생 제외(경로 파생 무의미).
        let url = vec![file_field("url", true, PresetFieldInput::Url)];
        let up = json!({ "url": "https://example.com/x" });
        assert!(PresetFieldSpec::derive_cwd(&url, &up).is_none());
    }

    #[test]
    fn no_derive_for_bare_filename_or_missing_param() {
        // 부모 없는 경로(파일명만) → 파생 안 함.
        let fields = vec![file_field("file", true, PresetFieldInput::FilePath)];
        assert!(PresetFieldSpec::derive_cwd(&fields, &json!({ "file": "x.md" })).is_none());
        // param 부재 → 파생 안 함.
        assert!(PresetFieldSpec::derive_cwd(&fields, &json!({})).is_none());
    }

    // ── apply_workspace_preset: category 지정 ──────────────────────────────

    fn test_state() -> (crate::state::AppState, CoreState) {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = CoreState::new(80, 24, waker).expect("CoreState::new");
        let preset_store = std::sync::Arc::new(std::sync::Mutex::new(
            tasty_presets::PresetStore::load_default(),
        ));
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::testing::InMemoryStorage::new(),
            ));
        let state = crate::state::AppState::new(&mut engine, preset_store, memory);
        (state, engine)
    }

    fn minimal_workspace_preset() -> WorkspacePreset {
        WorkspacePreset {
            name: "test".to_string(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Leaf {
                pane: PresetPane {
                    tabs: vec![PresetTab {
                        explicit_name: None,
                        layout: PresetSurfaceLayout::Leaf {
                            surface: PresetSurface {
                                id: None,
                                kind: "terminal".to_string(),
                                cwd: None,
                                startup_command: None,
                                params: serde_json::Value::Null,
                            },
                        },
                    }],
                    active_tab: 0,
                },
            },
        }
    }

    #[test]
    fn apply_workspace_preset_sets_target_category() {
        let (mut state, mut engine) = test_state();
        let work = engine.create_category("Work").unwrap();
        let preset = minimal_workspace_preset();

        let idx = state
            .apply_workspace_preset(
                &mut engine,
                &preset,
                Some(work),
                ApplyOptions { focus: false },
            )
            .expect("apply_workspace_preset");

        assert_eq!(engine.workspaces[idx].category, work);
    }

    #[test]
    fn apply_workspace_preset_without_category_stays_normal() {
        let (mut state, mut engine) = test_state();
        let preset = minimal_workspace_preset();

        let idx = state
            .apply_workspace_preset(&mut engine, &preset, None, ApplyOptions { focus: false })
            .expect("apply_workspace_preset");

        assert_eq!(
            engine.workspaces[idx].category,
            crate::model::NORMAL_CATEGORY_ID
        );
    }

    #[test]
    fn apply_workspace_preset_ignores_dangling_category() {
        // 존재하지 않는 카테고리 id 를 넘기면 set_workspace_category 가 실패하고
        // (경고 로그) normal 로 남는다 — apply_create_workspace_inner 의
        // "없거나 dangling 이면 normal 유지" 정책과 동형.
        let (mut state, mut engine) = test_state();
        let dangling: crate::model::WorkspaceCategoryId = 9999;
        let preset = minimal_workspace_preset();

        let idx = state
            .apply_workspace_preset(
                &mut engine,
                &preset,
                Some(dangling),
                ApplyOptions { focus: false },
            )
            .expect("apply_workspace_preset");

        assert_eq!(
            engine.workspaces[idx].category,
            crate::model::NORMAL_CATEGORY_ID
        );
    }

    #[test]
    fn missing_kind_leaf_is_absorbed_keeping_sibling_terminal() {
        use crate::model::{EmptySurface, SurfaceLayout};
        use tasty_presets::PresetSplitDirection;

        let (mut state, mut engine) = test_state();
        let split = PresetSurfaceLayout::Split {
            direction: PresetSplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PresetSurfaceLayout::Leaf {
                surface: PresetSurface {
                    id: None,
                    kind: "terminal".to_string(),
                    cwd: None,
                    startup_command: None,
                    params: serde_json::Value::Null,
                },
            }),
            second: Box::new(PresetSurfaceLayout::Leaf {
                surface: PresetSurface {
                    id: None,
                    kind: "no_such_plugin".to_string(),
                    cwd: None,
                    startup_command: None,
                    params: json!({ "file": "/x" }),
                },
            }),
        };

        // Err 전파였다면 여기서 apply 전체가 실패해 형제 terminal 까지 잃었다.
        let layout = state
            .build_surface_layout(&mut engine, &split)
            .expect("미등록 kind 가 있어도 형제를 살리고 성공해야 한다");

        let SurfaceLayout::Split { first, second, .. } = layout else {
            panic!("Split 이어야 한다");
        };
        // 형제 terminal 은 살아남는다.
        let SurfaceLayout::Leaf(term) = *first else {
            panic!("first 는 Leaf 여야 한다");
        };
        assert_eq!(term.kind(), "terminal");
        // 미등록 kind 는 deferred plugin placeholder 로 흡수된다(kind/snapshot 보존).
        let SurfaceLayout::Leaf(ph) = *second else {
            panic!("second 는 Leaf 여야 한다");
        };
        let es = ph
            .as_any()
            .downcast_ref::<EmptySurface>()
            .expect("미등록 kind 는 EmptySurface placeholder 여야 한다");
        assert!(es.is_deferred());
        let dp = es
            .deferred_plugin()
            .expect("plugin deferred placeholder 여야 한다");
        assert_eq!(dp.kind, "no_such_plugin");
        assert_eq!(dp.snapshot, json!({ "file": "/x" }));
    }
}
