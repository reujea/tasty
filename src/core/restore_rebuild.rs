//! Closed item 복원의 *순수 engine* helper.
//!
//! `AppState::restore_closed_item` 의 rebuild_* helper 들이 본 모듈로 모인다.
//! 모두 `&mut CoreState` 만 받음 (AppState 의존 없음) — Core 도메인의 일부.
//!
//! 본 모듈의 함수들은 새 surface_id / tab_id / workspace_id / pane_id 를
//! `engine.next_ids` 에서 발급받고, PTY 를 spawn 하며, 새 surface tree 를
//! 구성한다. 호출 측 (state.rs 또는 Core::apply) 은 결과를 받아 적절한
//! 위치 (pane.tabs / engine.workspaces 등) 에 attach 한다.

use crate::core::CoreState;
use crate::model::closed_item::*;
use crate::model::{
    DeferredPlugin, EmptySurface, Pane, PaneNode, Surface, SurfaceLayout, Tab, TerminalSurface,
};

/// rebuild_surface 의 반환 — 단일 surface 인지 layout 인지.
pub(crate) enum RebuildResult {
    /// A single surface (Terminal, Markdown, Explorer, etc.)
    Single(Box<dyn Surface>),
    /// A full layout tree with focused_surface id
    Layout(SurfaceLayout, u32),
}

impl RebuildResult {
    /// Convert into a Tab.
    pub(crate) fn into_tab(self, tab_id: u32, name: String) -> Tab {
        match self {
            RebuildResult::Single(surface) => Tab::new_with_surface(tab_id, name, surface),
            RebuildResult::Layout(layout, focused_surface) => Tab {
                id: tab_id,
                name,
                explicit_name: None,
                osc_title: None,
                layout_opt: Some(layout),
                focused_surface,
                cached_display_name: None,
            },
        }
    }
}

pub(crate) fn rebuild_surface(
    engine: &mut CoreState,
    closed: ClosedPanel,
) -> Option<RebuildResult> {
    match closed {
        ClosedPanel::Terminal(surface) => {
            let node = rebuild_surface_node(engine, surface)?;
            Some(RebuildResult::Single(Box::new(node)))
        }
        ClosedPanel::Tab {
            layout,
            focused_surface: _,
        } => {
            let rebuilt_layout = rebuild_surface_layout(engine, layout)?;
            let first_id = rebuilt_layout.first_surface_id().unwrap_or(0);
            Some(RebuildResult::Layout(rebuilt_layout, first_id))
        }
        ClosedPanel::Generic { kind, snapshot } => {
            let id = engine.next_ids.next_surface();
            // kind 가 아직 registry 에 없으면(plugin 이 hello 전인 부팅 창) 여기서
            // None 을 반환해선 안 된다 — 호출자 `rebuild_pane` 의 tab 루프가 `?` 로
            // 그 pane 의 형제 tab(무고한 terminal 포함)까지 통째로 버린다. 대신
            // kind/snapshot 을 보존한 deferred placeholder 로 남겨 형제를 살리고,
            // reify(`reify_displayed_surfaces`)가 kind 등록 후 실제화한다.
            match engine.surface_registry.get_live(&kind) {
                None => {
                    let ph =
                        EmptySurface::new_deferred_plugin(id, DeferredPlugin { kind, snapshot });
                    Some(RebuildResult::Single(Box::new(ph)))
                }
                Some(def) => match (def.restore)(id, &snapshot) {
                    Ok(surface) => Some(RebuildResult::Single(surface)),
                    Err(e) => {
                        tracing::warn!("restore failed for kind '{}': {e}", kind);
                        None
                    }
                },
            }
        }
    }
}

pub(crate) fn rebuild_surface_node(
    engine: &mut CoreState,
    closed: ClosedSurface,
) -> Option<TerminalSurface> {
    let surface_id = engine.next_ids.next_surface();
    let cols = engine.default_cols;
    let rows = engine.default_rows;
    let shell = if engine.settings.general.shell.is_empty() {
        None
    } else {
        Some(engine.settings.general.shell.clone())
    };
    let shell_args_owned = engine.settings.general.effective_shell_args();
    let shell_args: Vec<&str> = shell_args_owned.iter().map(|s| s.as_str()).collect();
    let shell_envs_owned = engine.settings.general.effective_shell_envs();
    let shell_envs: Vec<(&str, &str)> = shell_envs_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let waker = engine.make_waker(surface_id);

    // PTY 의 첫 입력으로 cd + restore_command 를 합쳐 한 번에 주입한다. shell 이
    // stdin 을 처음 read 하는 순간 이 바이트가 들어가므로, GUI redraw / busy tick
    // 등 추가 트리거 없이 spawn 과 동시에 실행된다.
    let mut initial = String::new();
    if let Some(dir) = closed.cwd.as_deref() {
        initial.push_str(&format!("cd {}\r", shell_escape(dir)));
    }
    if let Some(cmd) = closed.restore_command.as_deref() {
        initial.push_str(&format!("{cmd}\r"));
    }
    let initial_input = if initial.is_empty() {
        None
    } else {
        Some(initial.as_str())
    };

    let mut terminal = tasty_terminal::Terminal::new(
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
    .ok()?;

    // Scrollback is persisted to disk at close time (see `push_closed_item`),
    // so read it back by reference here. A restore consumes the closed item, so
    // the backing file is deleted after the one-time read to avoid orphans.
    //
    // 삭제는 **읽기에 성공했을 때만** 한다. 예전에는 실패를 빈 값으로 폴백한 뒤 곧바로
    // 지워서, 일시적 IO 오류나 손상 파일 하나가 사용자 scrollback 의 영구 소실이 됐다.
    // 못 읽은 파일을 남기면 orphan 이 될 수는 있으나, 그건 부팅 GC 가 정리하는 반면
    // 지워진 내용은 되돌릴 방법이 없다.
    let scrollback_lines: Vec<tasty_terminal::ScrollbackLine> = match closed.scrollback {
        ClosedScrollback::Persisted(id) => match crate::scrollback_store::read(&id) {
            crate::scrollback_store::ScrollbackRead::Loaded(lines) => {
                crate::scrollback_store::delete(&id);
                lines
            }
            crate::scrollback_store::ScrollbackRead::Absent => Vec::new(),
            crate::scrollback_store::ScrollbackRead::Unreadable => {
                tracing::warn!(
                    "restore: scrollback {id} could not be read — restoring the surface empty \
                     and keeping the file for recovery"
                );
                Vec::new()
            }
        },
        ClosedScrollback::Inline(lines) => lines.into_iter().collect(),
        ClosedScrollback::Empty => Vec::new(),
    };
    if !scrollback_lines.is_empty() {
        terminal.inject_scrollback(scrollback_lines);
        // 새 prompt 가 화면 중간부터 시작하도록 visible 상단 절반에 옛
        // 라인을 미리 그려둔다.
        let prefill = terminal.rows() / 2;
        terminal.prefill_visible_from_scrollback(prefill);
    }

    engine.terminals.insert(surface_id, terminal);
    engine.send_fast_init(surface_id);

    Some(TerminalSurface { id: surface_id })
}

pub(crate) fn rebuild_surface_layout(
    engine: &mut CoreState,
    closed: ClosedSurfaceLayout,
) -> Option<SurfaceLayout> {
    match closed {
        ClosedSurfaceLayout::Single(surface) => {
            let node = rebuild_surface_node(engine, surface)?;
            Some(SurfaceLayout::Leaf(Box::new(node)))
        }
        ClosedSurfaceLayout::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first = rebuild_surface_layout(engine, *first)?;
            let second = rebuild_surface_layout(engine, *second)?;
            Some(SurfaceLayout::Split {
                direction,
                ratio,
                first: Box::new(first),
                second: Box::new(second),
                focus_second: false,
            })
        }
    }
}

pub(crate) fn rebuild_pane_node(
    engine: &mut CoreState,
    closed: ClosedPaneNode,
) -> Option<PaneNode> {
    match closed {
        ClosedPaneNode::Leaf(closed_pane) => {
            let pane = rebuild_pane(engine, closed_pane)?;
            Some(PaneNode::Leaf(pane))
        }
        ClosedPaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first = rebuild_pane_node(engine, *first)?;
            let second = rebuild_pane_node(engine, *second)?;
            Some(PaneNode::Split {
                direction,
                ratio,
                first: Box::new(first),
                second: Box::new(second),
            })
        }
    }
}

pub(crate) fn rebuild_pane(engine: &mut CoreState, closed: ClosedPane) -> Option<Pane> {
    let pane_id = engine.next_ids.next_pane();
    let mut tabs = Vec::new();
    for closed_tab in closed.tabs {
        let result = rebuild_surface(engine, closed_tab.panel)?;
        let tab_id = engine.next_ids.next_tab();
        let name = closed_tab.explicit_name.unwrap_or(closed_tab.name);
        tabs.push(result.into_tab(tab_id, name));
    }
    if tabs.is_empty() {
        return None;
    }
    let active_tab = closed.active_tab.min(tabs.len() - 1);
    Some(Pane {
        id: pane_id,
        tabs,
        active_tab,
        tab_scroll_offset: 0.0,
    })
}

/// Escape a path for shell use.
fn shell_escape(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod deferred_plugin_tests {
    use super::*;
    use crate::model::closed_item::{ClosedPane, ClosedPaneNode, ClosedPanel, ClosedTab};

    // ── (2) deadline=0 관점: hello 를 하나도 기다리지 않고(=registry 미충족 시점)
    // apply 해도 유실이 없다 ──
    //
    // deadline(`PLUGIN_WAIT_DEADLINE`)은 App 부팅의 타이밍값이라 그 자체를 test 에
    // 값으로 주입하려면 App boot 상태 머신 전체를 fixture 로 세워야 한다("fixture
    // 안 만든다" 와 충돌). deadline 의 유일한 관측 효과는 "apply 시점에 registry 가
    // 아직 비어 있는가"이고, deadline=0 은 그 극단(hello 를 하나도 안 기다림)이다.
    // 그래서 registry 미충족 상태에서 apply 경로(`rebuild_pane_node` 중첩)를 직접
    // 태우는 것이 deadline=0 주입과 동치다 — 아래 테스트의 `engine()` 은 plugin
    // kind 를 등록하지 않은 채라 그 자체가 "hello 전" 상태다.
    //
    // "유실이 없다" 를 세 술어로 나눠 단언한다(어느 전파 홉이 살아있는지 뭉개지
    // 않게):
    //   ① 그 노드가 placeholder 로 남는다(kind 보존)
    //        → missing_plugin_kind_rebuilds_as_deferred_placeholder
    //   ② 같은 pane 의 다른 tab 이 산다(1차 전파 = rebuild_pane 의 tab 루프 `?`)
    //        → missing_plugin_kind_preserves_sibling_tabs
    //   ③ 형제 pane 이 산다(2차 전파 = rebuild_pane_node::Split 의 `?`)
    //        → deadline_zero_apply_preserves_sibling_panes
    // 양성 대조: kind 가 등록돼 있으면(=hello 후) 같은 경로가 placeholder 가
    // 아니라 실제 surface 를 낸다 → registered_kind_restores_real_surface_not_placeholder.

    fn engine() -> CoreState {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    fn generic_tab(id: u32, kind: &str) -> ClosedTab {
        ClosedTab {
            id,
            name: kind.to_string(),
            explicit_name: None,
            panel: ClosedPanel::Generic {
                kind: kind.to_string(),
                snapshot: serde_json::json!({ "k": kind }),
            },
        }
    }

    // 부팅 창(plugin hello 전이라 registry 에 kind 없음)에서 Generic surface 를
    // 복원하면 None 이 아니라 kind/snapshot 을 보존한 deferred placeholder 여야 한다.
    // None 이면 호출자 rebuild_pane 의 `?` 가 형제 tab 을 통째로 버린다.
    #[test]
    fn missing_plugin_kind_rebuilds_as_deferred_placeholder() {
        let mut e = engine();
        let panel = ClosedPanel::Generic {
            kind: "no_such_plugin".to_string(),
            snapshot: serde_json::json!({ "a": 1 }),
        };
        let r = rebuild_surface(&mut e, panel).expect("miss must yield a placeholder, not None");
        match r {
            RebuildResult::Single(s) => {
                let es = s
                    .as_any()
                    .downcast_ref::<EmptySurface>()
                    .expect("placeholder is an EmptySurface");
                let p = es.deferred_plugin().expect("carries deferred plugin info");
                assert_eq!(p.kind, "no_such_plugin");
                assert_eq!(p.snapshot, serde_json::json!({ "a": 1 }));
            }
            _ => panic!("expected a single placeholder surface"),
        }
    }

    // 핵심 회귀(결함 1): 한 pane 에 plugin tab(kind miss)과 다른 tab 이 함께 있을 때,
    // miss 가 `?` 로 pane 전체를 죽이지 않고 형제 tab 이 보존돼야 한다. 형제가
    // terminal 이어도 동일 — `?` 는 rebuild_pane 의 tab 루프 하나라 형제 종류와
    // 무관하다(여기선 PTY spawn 을 피하려 형제도 Generic 으로 둔다).
    #[test]
    fn missing_plugin_kind_preserves_sibling_tabs() {
        let mut e = engine();
        let pane = ClosedPane {
            id: 0,
            tabs: vec![
                generic_tab(1, "no_such_plugin"),
                generic_tab(2, "also_missing"),
            ],
            active_tab: 0,
        };
        let rebuilt = rebuild_pane(&mut e, pane).expect("pane must survive a missing plugin kind");
        assert_eq!(
            rebuilt.tabs.len(),
            2,
            "형제 tab 이 보존되어야 한다 (? 전파가 끊겼는지의 명제)"
        );
    }

    // ③ 2차 전파: rebuild_pane_node::Split 에서 한 leaf pane 의 kind miss 가 `?` 로
    // Split 을 죽이면 형제 pane 이 통째로 사라진다. deadline=0(hello 전) 상태에서도
    // 형제 pane 이 살아야 한다.
    #[test]
    fn deadline_zero_apply_preserves_sibling_panes() {
        let mut e = engine();
        let node = ClosedPaneNode::Split {
            direction: crate::model::SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(ClosedPaneNode::Leaf(ClosedPane {
                id: 0,
                tabs: vec![generic_tab(1, "no_such_plugin")],
                active_tab: 0,
            })),
            second: Box::new(ClosedPaneNode::Leaf(ClosedPane {
                id: 0,
                tabs: vec![generic_tab(2, "sibling_pane")],
                active_tab: 0,
            })),
        };
        let rebuilt =
            rebuild_pane_node(&mut e, node).expect("split must survive a missing kind in one leaf");
        match rebuilt {
            PaneNode::Split { first, second, .. } => {
                assert!(
                    matches!(*first, PaneNode::Leaf(_)),
                    "miss leaf 가 placeholder pane 으로 남아야 한다"
                );
                assert!(
                    matches!(*second, PaneNode::Leaf(_)),
                    "형제 pane 이 보존되어야 한다 (rebuild_pane_node::Split 의 ? 전파가 끊겼는지)"
                );
            }
            _ => panic!("expected a split node"),
        }
    }

    // 양성 대조용 등록 kind — restore 가 성공(Ok)해 placeholder 가 아닌 실제 surface 를
    // 낸다(여기선 관측을 위해 deferred 가 아닌 EmptySurface 를 돌려준다).
    fn register_ok_kind(e: &mut CoreState, kind: &'static str) {
        use crate::core::surface_registry::{KindSource, RegisteredRendering, SurfaceKindDef};
        use std::collections::HashMap;
        use std::sync::Arc;
        e.surface_registry.register(SurfaceKindDef {
            kind,
            rendering: RegisteredRendering::HostEgui,
            source: KindSource::HostBuiltin,
            display_name_i18n_key: "test.dummy",
            icon: None,
            create: Arc::new(|_, _, _| Err(anyhow::anyhow!("dummy"))),
            restore: Arc::new(|id, _| Ok(Box::new(EmptySurface::new(id)) as Box<dyn Surface>)),
            snapshot: Arc::new(|_| None),
            preset_fields: Vec::new(),
            param_aliases: HashMap::new(),
            default_params: HashMap::new(),
            consumes_egui_input: false,
            zoomable: false,
            egui_copy: false,
            copy_path: false,
            egui_paste: false,
            name_from_param: None,
            records_recent: false,
            convert_requires_input: false,
            convert_input_popup: None,
        });
    }

    // 양성 대조: kind 가 registry 에 등록돼 있으면(=hello 도착 후) 같은 miss
    // 경로가 placeholder 가 아니라 restore 가 만든 실제 surface 를 낸다. 이게 없으면
    // "무조건 placeholder 가 나오는 것 아니냐" 를 배제하지 못한다.
    #[test]
    fn registered_kind_restores_real_surface_not_placeholder() {
        let mut e = engine();
        register_ok_kind(&mut e, "present_plugin");
        let panel = ClosedPanel::Generic {
            kind: "present_plugin".to_string(),
            snapshot: serde_json::json!({ "a": 1 }),
        };
        let r = rebuild_surface(&mut e, panel).expect("registered kind restores");
        match r {
            RebuildResult::Single(s) => {
                let es = s
                    .as_any()
                    .downcast_ref::<EmptySurface>()
                    .expect("our test restore returns an EmptySurface");
                assert!(
                    es.deferred_plugin().is_none(),
                    "등록된 kind 는 deferred placeholder 가 아니라 실제 복원이어야 한다"
                );
            }
            _ => panic!("expected a single surface"),
        }
    }
}
