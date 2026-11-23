use serde_json::json;

use crate::runtime::engine_access::EngineRef;
use tasty_ipc::protocol::JsonRpcResponse;

pub(crate) fn handle_surface_list(
    engine: &EngineRef<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let mut surfaces = Vec::new();
    for ws in &engine.workspaces() {
        for &pane_id in &ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                for (tab_idx, tab) in pane.tabs.iter().enumerate() {
                    collect_tab_surface_info(engine, tab, pane_id, ws.id, tab_idx, &mut surfaces);
                }
            }
        }
    }
    JsonRpcResponse::success(id, json!(surfaces))
}

fn collect_tab_surface_info(
    engine: &EngineRef<'_>,
    tab: &crate::model::Tab,
    pane_id: u32,
    workspace_id: u32,
    tab_idx: usize,
    out: &mut Vec<serde_json::Value>,
) {
    if tab.is_split() {
        collect_surface_layout_info(engine, tab.layout(), pane_id, workspace_id, tab_idx, out);
    } else {
        let Some(surface) = tab
            .first_surface_id()
            .and_then(|id| engine.find_surface_by_id(id))
        else {
            return;
        };
        if let Some(node) = surface
            .as_any()
            .downcast_ref::<crate::model::TerminalSurface>()
        {
            let t = engine.runtime.terminals.get(node.id);
            let mut entry = json!({
                "id": node.id,
                "pane_id": pane_id,
                "workspace_id": workspace_id,
                "tab_index": tab_idx,
                "type": "Terminal",
                "cols": t.map(|x| x.cols()).unwrap_or(0),
                "rows": t.map(|x| x.rows()).unwrap_or(0),
                "busy": engine.read().is_surface_busy(node.id),
                "pty_ready": engine.runtime.terminals.contains(node.id),
                "attached": engine.live.occupancy.is_hard_occupied(node.id),
            });
            if let Some(fg) = engine
                .runtime
                .terminals
                .pty(node.id)
                .and_then(|pty| pty.foreground_process_info())
            {
                entry["foreground_process"] = json!(fg.name);
                entry["foreground_pid"] = json!(fg.pid);
            }
            add_pending_size(engine, node.id, &mut entry);
            out.push(entry);
        } else if let Some(id) = surface.surface_id() {
            // Non-terminal surfaces (Markdown, Explorer, Html, Empty) and restore slots.
            let mut entry = json!({
                "id": id,
                "pane_id": pane_id,
                "workspace_id": workspace_id,
                "tab_index": tab_idx,
                "type": surface.type_name(),
                "busy": false,
            });
            add_restore_fields(surface, &mut entry);
            out.push(entry);
        }
    }
}

/// 숨은 동안 기록한 목표 크기. `cols`·`rows`는 실제 grid이고 이 값은 보이게 될 때 적용된다.
/// 점유 client가 크기를 정하는 surface에는 내지 않는다(docs/design/policies/render-on-draw.md).
fn add_pending_size(engine: &EngineRef<'_>, id: u32, entry: &mut serde_json::Value) {
    if engine.live.occupancy.is_hard_occupied(id) {
        return;
    }
    if let Some((cols, rows)) = engine.runtime.terminals.deferred_size(id) {
        entry["pending_cols"] = json!(cols);
        entry["pending_rows"] = json!(rows);
    }
}

fn collect_surface_layout_info(
    engine: &EngineRef<'_>,
    layout: &crate::model::SurfaceLayout,
    pane_id: u32,
    workspace_id: u32,
    tab_idx: usize,
    out: &mut Vec<serde_json::Value>,
) {
    match layout {
        crate::model::SurfaceLayout::Leaf(surface) => {
            let id = surface.surface_id().unwrap_or(0);
            let mut entry = json!({
                "id": id,
                "pane_id": pane_id,
                "workspace_id": workspace_id,
                "tab_index": tab_idx,
                "type": engine.find_surface_by_id(id).map(|surface| surface.type_name()).unwrap_or("Empty"),
                "busy": engine.read().is_surface_busy(id),
                "attached": engine.live.occupancy.is_hard_occupied(id),
            });
            if let Some(surface) = engine.find_surface_by_id(id) {
                add_restore_fields(surface, &mut entry);
            }
            if let Some(terminal) = engine.runtime.terminals.get(id) {
                entry["cols"] = json!(terminal.cols());
                entry["rows"] = json!(terminal.rows());
                entry["pty_ready"] = json!(true);
                if let Some(fg) = engine
                    .runtime
                    .terminals
                    .pty(id)
                    .and_then(|pty| pty.foreground_process_info())
                {
                    entry["foreground_process"] = json!(fg.name);
                    entry["foreground_pid"] = json!(fg.pid);
                }
                add_pending_size(engine, id, &mut entry);
            }
            out.push(entry);
        }
        crate::model::SurfaceLayout::Split { first, second, .. } => {
            collect_surface_layout_info(engine, first, pane_id, workspace_id, tab_idx, out);
            collect_surface_layout_info(engine, second, pane_id, workspace_id, tab_idx, out);
        }
    }
}

/// 복원 자리는 탭 분할 여부와 관계없이 `list tree`와 같은 필드로 보고한다:
/// `type:"Pending"`, 목표 `kind`, `pty_ready:false`, `restore_error`.
/// plugin kind 등록을 기다리는 자리(EmptySurface의 deferred)는 type_name이 `Empty`라
/// type을 덮어쓰고, 실패가 없으므로 `restore_error`는 null, 대기 이유는 `list tree`와 같은
/// `pending_reason`으로 싣는다.
fn add_restore_fields(surface: &dyn crate::model::Surface, entry: &mut serde_json::Value) {
    if let Some(plugin) = surface
        .as_any()
        .downcast_ref::<crate::model::EmptySurface>()
        .and_then(|empty| empty.deferred_plugin())
    {
        entry["type"] = json!("Pending");
        entry["kind"] = json!(plugin.kind);
        entry["pty_ready"] = json!(false);
        entry["restore_error"] = serde_json::Value::Null;
        entry["pending_reason"] = json!("plugin_not_loaded");
        return;
    }
    let Some(slot) = surface
        .as_any()
        .downcast_ref::<crate::runtime::surface_restorer::JournalPlaceholder>()
    else {
        return;
    };
    entry["kind"] = json!(slot.kind);
    entry["pty_ready"] = json!(false);
    entry["restore_error"] = json!(slot.failure);
}

#[cfg(test)]
mod tests {
    use tasty_core::{DomainEvent as E, Placement, Ratio, SplitSpec, SurfaceSpec};

    use crate::runtime::surface_restorer::JournalPlaceholder;

    const ALONE: u32 = 11;
    const SPLIT: u32 = 21;
    const SPLIT_PEER: u32 = 22;

    fn markdown(id: u32) -> SurfaceSpec {
        SurfaceSpec {
            id,
            kind: "markdown".into(),
            data: None,
        }
    }

    fn placeholder(id: u32) -> Box<JournalPlaceholder> {
        Box::new(JournalPlaceholder {
            id,
            kind: "markdown".into(),
            data: None,
            creation_seed: None,
            activation: None,
            attempts: 0,
            failure: None,
            recovery_blocked: false,
        })
    }

    /// 분할되지 않은 탭과 분할된 탭의 markdown 복원 자리가 같은 필드로 보고된다.
    #[test]
    fn restore_slots_report_the_same_fields_in_split_and_single_tabs() {
        let model = crate::state::tests::test_model(vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "w".into(),
                category: 0,
                index: 0,
                pane: 10,
            },
            E::TabCreated {
                id: 100,
                pane: 10,
                index: 0,
                name: "alone".into(),
                surface: markdown(ALONE),
            },
            E::TabCreated {
                id: 101,
                pane: 10,
                index: 1,
                name: "split".into(),
                surface: markdown(SPLIT),
            },
            E::SurfaceSplit {
                target: SPLIT,
                surface: markdown(SPLIT_PEER),
                split: SplitSpec {
                    direction: crate::model::SplitDirection::Horizontal,
                    ratio: Ratio::from_f32(0.5),
                    placement: Placement::After,
                },
            },
        ]);
        let (_state, mut session) = crate::state::tests::test_state_from_model(model);
        for id in [ALONE, SPLIT, SPLIT_PEER] {
            session.runtime.surfaces.insert(id, placeholder(id));
        }
        let engine = session.borrow_mut();
        let response = super::handle_surface_list(&engine.as_ref(), serde_json::json!(1));
        let list = response.result.expect("surface.list result");
        let entry = |id: u32| {
            list.as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == id)
                .unwrap_or_else(|| panic!("surface {id} missing: {list}"))
                .clone()
        };
        let fields = |e: &serde_json::Value| {
            (
                e["type"].clone(),
                e["kind"].clone(),
                e["pty_ready"].clone(),
                e["restore_error"].clone(),
            )
        };
        let alone = entry(ALONE);
        assert_eq!(alone["type"], "Pending");
        assert_eq!(alone["kind"], "markdown");
        assert_eq!(alone["pty_ready"], false);
        assert!(alone["restore_error"].is_null());
        assert_eq!(fields(&alone), fields(&entry(SPLIT)));
        assert_eq!(fields(&alone), fields(&entry(SPLIT_PEER)));
    }

    fn plugin_wait(id: u32) -> Box<crate::model::EmptySurface> {
        Box::new(crate::model::EmptySurface::new_deferred_plugin(
            id,
            crate::model::DeferredPlugin {
                kind: "markdown".into(),
                snapshot: serde_json::json!({ "file": "/a.md" }),
            },
        ))
    }

    /// plugin 등록을 기다리는 자리도 journal 복원 자리와 같은 필드로, 분할 여부와 관계없이
    /// 보고된다. 비활성 빈 surface는 그대로 Empty다.
    #[test]
    fn plugin_wait_slots_report_pending_like_restore_slots() {
        let model = crate::state::tests::test_model(vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "w".into(),
                category: 0,
                index: 0,
                pane: 10,
            },
            E::TabCreated {
                id: 100,
                pane: 10,
                index: 0,
                name: "alone".into(),
                surface: markdown(ALONE),
            },
            E::TabCreated {
                id: 101,
                pane: 10,
                index: 1,
                name: "split".into(),
                surface: markdown(SPLIT),
            },
            E::SurfaceSplit {
                target: SPLIT,
                surface: markdown(SPLIT_PEER),
                split: SplitSpec {
                    direction: crate::model::SplitDirection::Horizontal,
                    ratio: Ratio::from_f32(0.5),
                    placement: Placement::After,
                },
            },
        ]);
        let (_state, mut session) = crate::state::tests::test_state_from_model(model);
        session.runtime.surfaces.insert(ALONE, plugin_wait(ALONE));
        session.runtime.surfaces.insert(SPLIT, plugin_wait(SPLIT));
        session.runtime.surfaces.insert(
            SPLIT_PEER,
            Box::new(crate::model::EmptySurface::new(SPLIT_PEER)),
        );
        let engine = session.borrow_mut();
        let response = super::handle_surface_list(&engine.as_ref(), serde_json::json!(1));
        let list = response.result.expect("surface.list result");
        let entry = |id: u32| {
            list.as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == id)
                .unwrap_or_else(|| panic!("surface {id} missing: {list}"))
                .clone()
        };
        for id in [ALONE, SPLIT] {
            let e = entry(id);
            assert_eq!(e["type"], "Pending", "{e}");
            assert_eq!(e["kind"], "markdown", "{e}");
            assert_eq!(e["pty_ready"], false, "{e}");
            assert!(e.get("restore_error").is_some_and(|v| v.is_null()), "{e}");
            assert_eq!(e["pending_reason"], "plugin_not_loaded", "{e}");
        }
        let empty = entry(SPLIT_PEER);
        assert_eq!(empty["type"], "Empty", "{empty}");
        assert!(empty.get("kind").is_none(), "{empty}");
    }
}

#[cfg(all(test, feature = "gui"))]
mod pending_size_tests {
    /// Given 숨은 동안 목표를 기록한 터미널과 기록이 없는 터미널, When `surface.list`를 읽으면, Then 기록한
    /// 쪽만 `pending_cols`·`pending_rows`를 내고 `cols`·`rows`는 실제 grid다.
    #[test]
    fn surface_list_reports_the_recorded_target_apart_from_the_grid() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let local = *engine
            .workspace_at(0)
            .expect("workspace")
            .all_surface_ids()
            .first()
            .expect("surface");
        let hidden = crate::state::tests::add_mirror_test_workspace(&mut state, &mut engine);
        engine
            .runtime
            .terminals
            .apply_layout_size(hidden, 100, 30, false);

        let response = super::handle_surface_list(&engine.as_ref(), serde_json::json!(1));
        let list = response.result.expect("result");
        let entry = |sid: u32| {
            list.as_array()
                .expect("array")
                .iter()
                .find(|e| e["id"] == sid)
                .cloned()
                .expect("listed")
        };
        let hidden_entry = entry(hidden);
        assert_eq!(hidden_entry["cols"], 80);
        assert_eq!(hidden_entry["rows"], 24);
        assert_eq!(hidden_entry["pending_cols"], 100);
        assert_eq!(hidden_entry["pending_rows"], 30);
        assert!(entry(local).get("pending_cols").is_none());
    }
}
