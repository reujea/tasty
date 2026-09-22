//! `Core` — tab 생성/이동/이름. `src/core/mod.rs` 의 `impl Core` 분할.

use super::*;

impl Core {
    /// `DomainIntent::UpdateTabName` 본문. surface_id 가 속한 tab 을 *모든*
    /// workspace 에서 검색 (포커스 독립) → `osc_title` 필드 set. explicit_name
    /// 은 건드리지 않는다 — 사용자가 직접 이름 지은 tab 보존.
    #[cfg(any(feature = "gui", test))]
    pub(super) fn apply_update_tab_name(
        engine: &mut crate::core::CoreState,
        surface_id: u32,
        name: String,
    ) -> CoreEvent {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return CoreEvent::TabNameUpdated {
                skipped_explicit: false,
            };
        }
        for ws in &mut engine.workspaces {
            let pane_ids = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout_mut().find_pane_mut(pane_id) {
                    for tab in &mut pane.tabs {
                        if tab.all_surface_ids().contains(&surface_id) {
                            if tab.explicit_name.is_some() {
                                return CoreEvent::TabNameUpdated {
                                    skipped_explicit: true,
                                };
                            }
                            // 오직 그 탭의 *focused* surface 발화만 탭 제목에 반영한다.
                            // 병렬 surface 의 title 발화가 last-writer-wins 로 제목을
                            // 흔드는 flicker 방지 (cwd 경로 refresh_tab_display_name 와
                            // 동일 정책). SurfaceTitleChanged host event 는 상류
                            // cascade_terminal_title_changed 에서 이미 발화되므로
                            // 이 가드가 plugin 호환에 영향 없다.
                            if tab.focused_surface != surface_id {
                                return CoreEvent::TabNameUpdated {
                                    skipped_explicit: false,
                                };
                            }
                            tab.osc_title = Some(name);
                            return CoreEvent::TabNameUpdated {
                                skipped_explicit: false,
                            };
                        }
                    }
                }
            }
        }
        CoreEvent::TabNameUpdated {
            skipped_explicit: false,
        }
    }

    /// `DomainIntent::CreateTab` 본문. borrow 분리:
    /// 1) settings / waker / surface 미리 추출 (engine 의 *불변* 의존)
    /// 2) scope block 으로 pane mutate (engine 의 가변 borrow 좁힘)
    /// 3) send_fast_init / mark_layout_dirty (pane borrow 끝난 후)
    ///
    /// 활성 탭은 `activate` 가 정한다 — 에이전트가 만든 탭은 사용자가 보던 탭을 바꾸지
    /// 않는다([ADR-0502](../../docs/adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md)).
    /// terminal 은 `activate` 와 무관하게 background 다.
    pub(super) fn apply_create_tab(
        engine: &mut crate::core::CoreState,
        pane_id: u32,
        cwd: Option<std::path::PathBuf>,
        kind: String,
        explicit_name: Option<String>,
        surface_params: serde_json::Value,
        activate: bool,
    ) -> anyhow::Result<Vec<CoreEvent>> {
        let tab_id = engine.next_ids.next_tab();
        let surface_id = engine.next_ids.next_surface();
        let is_terminal = kind == "terminal";

        let cols = engine.default_cols;
        let rows = engine.default_rows;
        let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
        let waker = engine.make_waker(surface_id);

        let prepared_non_terminal = if !is_terminal {
            let surface = engine.create_surface_via_registry(
                &kind,
                surface_id,
                cwd.as_deref(),
                &surface_params,
            )?;
            let name = crate::core::surface_registry::default_tab_name_for_kind(
                &kind,
                &surface_params,
                engine.surface_registry.get(&kind).as_deref(),
            );
            Some((surface, name))
        } else {
            None
        };

        // Terminal spawn 은 *pane 가변 borrow 시작 전* 에 끝낸다 — store 에 insert
        // 한 뒤 marker 만 pane 에 부착.
        let prepared_terminal = if is_terminal {
            let spawn = crate::model::ShellSpawnOpts {
                cols,
                rows,
                shell: sh.shell_ref(),
                shell_args: &sh.args_ref(),
                extra_env: &sh.envs_ref(),
                waker,
                working_dir: cwd.as_deref(),
            };
            let terminal = crate::model::Pane::spawn_terminal(surface_id, spawn)?;
            engine.terminals.insert(surface_id, terminal);
            true
        } else {
            false
        };

        {
            let pane = engine
                .find_pane_by_id_mut(pane_id)
                .ok_or_else(|| anyhow::anyhow!("pane {pane_id} not found"))?;
            if is_terminal {
                debug_assert!(prepared_terminal);
                pane.add_terminal_marker_tab_background(tab_id, surface_id, explicit_name);
            } else {
                let (surface, name) = prepared_non_terminal.unwrap();
                if activate {
                    pane.add_surface_tab(tab_id, name, explicit_name, surface);
                } else {
                    pane.add_surface_tab_background(tab_id, name, explicit_name, surface);
                }
            }
        }

        if is_terminal {
            engine.send_fast_init(surface_id);
        }
        engine.mark_layout_dirty();

        // attach 점유 중인 workspace 에 새로 생긴 멤버라면 편입 + 즉시 tap
        // (로컬 생성 경로 gap — forward-op 경로와 대칭으로 점유를 상속해야 한다).
        if let Some(ws_idx) = engine.find_workspace_index_for_pane(pane_id) {
            let ws_id = engine.workspaces[ws_idx].id;
            engine.tap_new_workspace_member(ws_id, surface_id, is_terminal);
        }

        let (tab_count, active_tab) = engine
            .find_pane_by_id(pane_id)
            .map(|p| (p.tabs.len(), p.active_tab))
            .unwrap_or((0, 0));

        Ok(vec![CoreEvent::TabCreated {
            pane_id,
            tab_id,
            surface_id,
            tab_count,
            active_tab,
        }])
    }

    /// `DomainIntent::MoveTab` 본문. pane_id 로 모든 workspace 순회
    /// (focused 의존 없음 — 포커스 독립 원칙).
    pub(super) fn apply_move_tab(
        engine: &mut crate::core::CoreState,
        pane_id: u32,
        from_index: usize,
        to_index: usize,
    ) -> CoreEvent {
        let moved = engine
            .find_pane_by_id_mut(pane_id)
            .map(|p| p.move_tab(from_index, to_index))
            .unwrap_or(false);
        if moved {
            engine.mark_layout_dirty();
        }
        CoreEvent::TabMoved { moved }
    }
}

#[cfg(test)]
mod create_tab_selection_tests {
    //! 새 탭의 선택은 `activate` 가 정한다 — 에이전트가 만든 비터미널 탭이 사용자가 보던
    //! 탭을 바꾸던 결함의 회귀 방지(ADR-0502).
    use super::*;

    fn create(engine: &mut CoreState, pane_id: u32, kind: &str, activate: bool) -> usize {
        let events = Core::apply_create_tab(
            engine,
            pane_id,
            None,
            kind.to_string(),
            None,
            serde_json::json!({}),
            activate,
        )
        .expect("create tab");
        let Some(CoreEvent::TabCreated { active_tab, .. }) = events.into_iter().next() else {
            panic!("expected TabCreated");
        };
        active_tab
    }

    fn engine_and_pane() -> (CoreState, u32) {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let engine = CoreState::new(80, 24, waker).expect("engine");
        let sid = engine.workspaces[0].all_surface_ids()[0];
        let pane_id = engine.find_pane_for_surface(sid).expect("pane");
        (engine, pane_id)
    }

    #[test]
    fn a_background_non_terminal_tab_keeps_the_selected_tab() {
        let (mut engine, pane_id) = engine_and_pane();
        assert_eq!(create(&mut engine, pane_id, "empty", false), 0);
        let pane = engine.find_pane_by_id(pane_id).unwrap();
        assert_eq!(pane.tabs.len(), 2);
        assert_eq!(pane.active_tab, 0);
    }

    #[test]
    fn an_activated_non_terminal_tab_becomes_the_selected_tab() {
        let (mut engine, pane_id) = engine_and_pane();
        assert_eq!(create(&mut engine, pane_id, "empty", true), 1);
        assert_eq!(engine.find_pane_by_id(pane_id).unwrap().active_tab, 1);
    }
}

#[cfg(test)]
mod tab_title_tests {
    //! 탭 제목이 그 탭의 *focused* surface 가 발화한 OSC title 만 반영하는지 검증.
    //! 병렬 surface 의 title 발화가 last-writer-wins 로 제목을 흔드는 flicker 회귀 방지.
    use super::*;
    use crate::model::SplitDirection;
    use tasty_terminal::Terminal;

    fn test_engine() -> CoreState {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    /// 기본 워크스페이스 단일 탭에 A(focused)+B 를 split 로 구성. 두 surface 모두
    /// detached terminal 로 store 에 등록. 반환 `(engine, pane_id, a, b)`.
    fn split_tab_engine() -> (CoreState, u32, u32, u32) {
        let mut engine = test_engine();
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine.terminals.insert(a, Terminal::new_detached(80, 24));
        let b = 7777;
        let (ws_idx, pane_id) = engine.find_workspace_index_for_surface(a).unwrap();
        engine.workspaces[ws_idx]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .unwrap()
            .split_surface_by_id_marker(a, SplitDirection::Horizontal, b)
            .unwrap();
        engine.terminals.insert(b, Terminal::new_detached(80, 24));
        set_focused(&mut engine, pane_id, a);
        (engine, pane_id, a, b)
    }

    fn set_focused(engine: &mut CoreState, pane_id: u32, sid: u32) {
        engine.workspaces[0]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .unwrap()
            .tabs[0]
            .focused_surface = sid;
    }

    /// OSC 2 를 feed 해 해당 surface 의 `current_title` 을 세팅한다.
    fn set_title(engine: &mut CoreState, sid: u32, title: &str) {
        engine
            .terminals
            .get_mut(sid)
            .unwrap()
            .feed_bytes(format!("\x1b]2;{title}\x07").as_bytes());
    }

    fn display_name(engine: &CoreState, pane_id: u32) -> String {
        engine.workspaces[0]
            .pane_layout()
            .find_pane(pane_id)
            .unwrap()
            .tabs[0]
            .display_name()
    }

    /// 비-focused surface 의 title 발화는 탭 제목을 흔들지 않는다. focused 발화만 반영.
    #[test]
    fn non_focused_surface_title_does_not_change_tab_name() {
        let (mut engine, pane_id, a, b) = split_tab_engine();
        // A 가 focused. B(non-focused)가 title 발화 → 탭 제목 불변.
        let ev = Core::apply_update_tab_name(&mut engine, b, "TITLE-FROM-B".to_string());
        assert!(matches!(
            ev,
            CoreEvent::TabNameUpdated {
                skipped_explicit: false,
                ..
            }
        ));
        assert_ne!(display_name(&engine, pane_id), "TITLE-FROM-B");

        // A(focused)가 발화 → 탭 제목 = TITLE-A.
        Core::apply_update_tab_name(&mut engine, a, "TITLE-A".to_string());
        assert_eq!(display_name(&engine, pane_id), "TITLE-A");
    }

    /// explicit_name 이 있으면 focused surface 발화도 무시(고정 이름 보존).
    #[test]
    fn explicit_name_survives_focused_title() {
        let (mut engine, pane_id, a, _b) = split_tab_engine();
        engine.workspaces[0]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .unwrap()
            .tabs[0]
            .explicit_name = Some("FIXED".to_string());
        let ev = Core::apply_update_tab_name(&mut engine, a, "TITLE-A".to_string());
        assert!(matches!(
            ev,
            CoreEvent::TabNameUpdated {
                skipped_explicit: true,
                ..
            }
        ));
        assert_eq!(display_name(&engine, pane_id), "FIXED");
    }

    /// 포커스가 B 로 이동하면 재투영으로 B 의 최신 title(unfocused 시절 발화분)이 반영.
    #[test]
    fn refresh_projects_new_focused_surface_title() {
        let (mut engine, pane_id, a, b) = split_tab_engine();
        set_title(&mut engine, a, "TITLE-A");
        set_title(&mut engine, b, "TITLE-B");
        Core::apply_update_tab_name(&mut engine, a, "TITLE-A".to_string());
        assert_eq!(display_name(&engine, pane_id), "TITLE-A");

        // 포커스를 B 로 전환 후 재투영 → B title.
        set_focused(&mut engine, pane_id, b);
        engine.refresh_tab_osc_title(b);
        assert_eq!(display_name(&engine, pane_id), "TITLE-B");
    }

    /// 새 focused surface 가 title 미보유면 osc_title clear → fallback 동작.
    #[test]
    fn refresh_clears_when_focused_has_no_title() {
        let (mut engine, pane_id, a, b) = split_tab_engine();
        set_title(&mut engine, a, "TITLE-A");
        Core::apply_update_tab_name(&mut engine, a, "TITLE-A".to_string());
        assert_eq!(display_name(&engine, pane_id), "TITLE-A");

        // B 는 title 없음 → 포커스 B 로 전환 + 재투영 → osc_title clear → fallback.
        set_focused(&mut engine, pane_id, b);
        engine.refresh_tab_osc_title(b);
        assert_ne!(display_name(&engine, pane_id), "TITLE-A");
    }

    /// focused surface 를 close 하면 생존 surface 로 focused 재배정 + 제목 재투영.
    #[test]
    fn closing_focused_surface_reprojects_to_survivor() {
        let (mut engine, pane_id, a, b) = split_tab_engine();
        set_title(&mut engine, a, "TITLE-A");
        set_title(&mut engine, b, "TITLE-B");
        Core::apply_update_tab_name(&mut engine, a, "TITLE-A".to_string());
        assert_eq!(display_name(&engine, pane_id), "TITLE-A");

        // focused A 를 close → 생존 B 로 focused 재배정 + 재투영 → B title.
        let ev = Core::apply_close_surface(&mut engine, a, false);
        assert!(matches!(ev, CoreEvent::SurfaceClosed { closed: true, .. }));
        assert_eq!(display_name(&engine, pane_id), "TITLE-B");
    }

    /// surface move 로 target tab 의 focused 가 A 로 승계되면 제목이 A 로 재투영.
    #[test]
    fn moving_surface_reprojects_target_tab_title() {
        let (mut engine, pane_id, a, b) = split_tab_engine();
        set_title(&mut engine, a, "TITLE-A");
        set_title(&mut engine, b, "TITLE-B");
        // focused=B 로 두고 B title 투영 (move 전 stale 상황 유도).
        set_focused(&mut engine, pane_id, b);
        engine.refresh_tab_osc_title(b);
        assert_eq!(display_name(&engine, pane_id), "TITLE-B");

        // A 를 B 위치로 move → 탭은 A 단독, 제목이 A 로 재투영 (B 의 stale title 제거).
        engine.pending_move_surface = Some(a);
        let ev = Core::apply_move_surface(&mut engine, a, b);
        assert!(matches!(
            ev,
            CoreEvent::MoveSurfaceApplied { moved: true, .. }
        ));
        assert_eq!(display_name(&engine, pane_id), "TITLE-A");
    }
}
