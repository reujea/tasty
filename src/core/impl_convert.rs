//! `Core` — surface 변환(convert). `src/core/mod.rs` 의 `impl Core` 분할.

use super::*;

impl Core {
    /// `DomainIntent::ConvertSurface` 본문. tab 안 split leaf 만 교체 / sole
    /// surface tab 전체 교체. 옛 `replace_surface_for_id` + 4 variant 의
    /// surface 생성 로직 흡수.
    pub(super) fn apply_convert_surface(
        engine: &mut crate::core::CoreState,
        surface_id: u32,
        target: crate::core::intent::ConvertSurfaceTarget,
    ) -> CoreEvent {
        use crate::core::intent::ConvertSurfaceTarget;

        let is_terminal = matches!(target, ConvertSurfaceTarget::Terminal { .. });

        // Phase 1: 새 surface 생성 (실패 가능)
        let (new_surface, new_name) =
            match Self::create_surface_for_convert(engine, surface_id, target) {
                Ok(v) => v,
                Err(ev) => return ev,
            };

        // Phase 2: location 찾기 (workspace index, pane id, tab index)
        let (ws_idx, pane_id, tab_idx) = match Self::find_surface_location(engine, surface_id) {
            Some(loc) => loc,
            None => {
                return CoreEvent::SurfaceConverted {
                    surface_id,
                    replaced: false,
                    failure: Some(format!("surface {surface_id} not found")),
                };
            }
        };

        // Phase 3: replace
        let replaced = match Self::replace_surface_in_tab(
            engine,
            ws_idx,
            pane_id,
            tab_idx,
            surface_id,
            new_surface,
            new_name,
        ) {
            Some(r) => r,
            None => {
                return CoreEvent::SurfaceConverted {
                    surface_id,
                    replaced: false,
                    failure: Some(format!("pane {pane_id} not found")),
                };
            }
        };

        // Phase 4: engine mutate (pane borrow 끝)
        if replaced {
            engine.mark_layout_dirty();
            if is_terminal {
                engine.send_fast_init(surface_id);
            }
            // 변환으로 focused surface 의 종류/title 이 바뀌었을 수 있고 explicit_name
            // 이 해제(new_name=Some(None))됐을 수도 있으므로 osc_title 를 재투영한다
            // (explicit_name 이 남아있으면 refresh 가 no-op). 새 surface 가 title
            // 미보유(non-terminal 등)면 clear → fallback.
            engine.refresh_tab_osc_title(surface_id);
        }

        CoreEvent::SurfaceConverted {
            surface_id,
            replaced,
            // 탭 안 split 에서 leaf 교체가 대상을 못 찾은 경우 — 위치 탐색은 그 탭이 대상을
            // 담는다고 답했으므로 레이아웃 트리와 탭 판정이 어긋난 것이다.
            failure: (!replaced)
                .then(|| format!("surface {surface_id} not found in its tab layout")),
        }
    }

    /// Phase 1: target 에 맞는 새 surface 생성. 실패 시 `Err(CoreEvent)` 로
    /// `apply_convert_surface` 가 그대로 반환할 실패 이벤트를 만들어 넘긴다.
    fn create_surface_for_convert(
        engine: &mut crate::core::CoreState,
        surface_id: u32,
        target: crate::core::intent::ConvertSurfaceTarget,
    ) -> Result<(Box<dyn crate::model::Surface>, Option<Option<String>>), CoreEvent> {
        use crate::core::intent::ConvertSurfaceTarget;
        match target {
            ConvertSurfaceTarget::Terminal { cwd } => {
                let cols = engine.default_cols;
                let rows = engine.default_rows;
                let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
                let waker = engine.make_waker(surface_id);
                let terminal = match tasty_terminal::Terminal::new(
                    tasty_terminal::TerminalConfig {
                        cols,
                        rows,
                        shell: sh.shell_ref(),
                        args: &sh.args_ref(),
                        extra_env: &sh.envs_ref(),
                        surface_id,
                        working_dir: cwd.as_deref(),
                        initial_input: None,
                    },
                    waker,
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        return Err(CoreEvent::SurfaceConverted {
                            surface_id,
                            replaced: false,
                            failure: Some(e.to_string()),
                        });
                    }
                };
                engine.terminals.insert(surface_id, terminal);
                let node = crate::model::TerminalSurface { id: surface_id };
                // Terminal 변환은 explicit_name 클리어 (auto-derived from CWD).
                Ok((Box::new(node), Some(None)))
            }
            ConvertSurfaceTarget::Kind { cwd, kind, params } => {
                let new_surface = match engine.create_surface_via_registry(
                    &kind,
                    surface_id,
                    cwd.as_deref(),
                    &params,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("ConvertSurface kind='{}' failed: {}", kind, e);
                        return Err(CoreEvent::SurfaceConverted {
                            surface_id,
                            replaced: false,
                            failure: Some(e.to_string()),
                        });
                    }
                };
                // markdown 등 file 기반 kind 는 옛 Markdown variant 처럼
                // file basename 으로 자동 명명. 그 외 kind 는 클리어 — surface
                // 자체의 display_name 이 사용된다.
                let auto_name =
                    derive_auto_name(engine.surface_registry.get(&kind).as_deref(), &params);
                Ok((new_surface, Some(auto_name)))
            }
        }
    }

    /// Phase 2: 대상 surface 를 담고 있는 (workspace index, pane id, tab index) 탐색.
    fn find_surface_location(
        engine: &crate::core::CoreState,
        surface_id: u32,
    ) -> Option<(usize, u32, usize)> {
        for (ws_idx, workspace) in engine.workspaces.iter().enumerate() {
            for &pid in &workspace.pane_layout().all_pane_ids() {
                if let Some(pane) = workspace.pane_layout().find_pane(pid) {
                    for (tab_idx, tab) in pane.tabs.iter().enumerate() {
                        if tab.contains_surface(surface_id) {
                            return Some((ws_idx, pid, tab_idx));
                        }
                    }
                }
            }
        }
        None
    }

    /// Phase 3: 찾은 위치에 새 surface 를 반영. pane 을 못 찾으면 `None`
    /// (호출자가 실패 이벤트로 변환).
    fn replace_surface_in_tab(
        engine: &mut crate::core::CoreState,
        ws_idx: usize,
        pane_id: u32,
        tab_idx: usize,
        surface_id: u32,
        new_surface: Box<dyn crate::model::Surface>,
        new_name: Option<Option<String>>,
    ) -> Option<bool> {
        let ws = &mut engine.workspaces[ws_idx];
        let pane = ws.pane_layout_mut().find_pane_mut(pane_id)?;
        let tab = &mut pane.tabs[tab_idx];
        if tab.is_split() {
            // Tab has split layout — replace just the leaf. Tab name 은 변경 X.
            Some(tab.layout_mut().replace_surface(surface_id, new_surface))
        } else {
            // Tab's sole surface — replace whole tab surface.
            tab.put_surface(new_surface);
            if let Some(name_opt) = new_name {
                tab.explicit_name = name_opt;
            }
            Some(true)
        }
    }
}

/// `ConvertSurface` 의 Kind 분기에서 사용. kind 가 매니페스트 `name_from_param`
/// (registry `SurfaceKindDef.name_from_param`)을 선언하면 그 params 키 값의 basename 을
/// 자동 명명으로 쓴다(예: markdown="file"). 미선언이면 None — surface 자체의
/// display_name 이 자동 적용된다. 옛 `ConvertSurfaceTarget::Markdown` arm 의 명명 동작을
/// 본체 `kind == "markdown"` 하드코딩 없이 보존한다.
fn derive_auto_name(
    def: Option<&crate::core::surface_registry::SurfaceKindDef>,
    params: &serde_json::Value,
) -> Option<String> {
    let key = def.and_then(|d| d.name_from_param.as_deref())?;
    let p = params.get(key).and_then(|v| v.as_str())?;
    std::path::Path::new(p)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
}

#[cfg(test)]
mod derive_auto_name_tests {
    use super::derive_auto_name;
    use crate::core::surface_registry::{SurfaceKindRegistry, register_builtin_kinds};

    #[test]
    fn name_from_param_kind_yields_basename_else_none() {
        let reg = SurfaceKindRegistry::new();
        register_builtin_kinds(&reg);
        // explorer builtin 은 name_from_param="path" 선언 → path basename.
        let explorer = reg.get("explorer").unwrap();
        assert_eq!(
            derive_auto_name(Some(&explorer), &serde_json::json!({"path": "/a/b/proj"})),
            Some("proj".to_string())
        );
        // 선언 키가 없으면 None (자동명명 skip → surface display_name 사용).
        assert_eq!(
            derive_auto_name(Some(&explorer), &serde_json::json!({})),
            None
        );
        // name_from_param 미선언 kind(empty)는 항상 None.
        let empty = reg.get("empty").unwrap();
        assert_eq!(
            derive_auto_name(Some(&empty), &serde_json::json!({"path": "/a/b"})),
            None
        );
        // def 미등록이면 None.
        assert_eq!(
            derive_auto_name(None, &serde_json::json!({"file": "/x.md"})),
            None
        );
    }
}
