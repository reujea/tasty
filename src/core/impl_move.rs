//! `Core` — surface move(cut & attach). `src/core/mod.rs` 의 `impl Core` 분할.

use super::*;

impl Core {
    /// `DomainIntent::MoveSurface` 본문 (T9). source(A) 를 살아있는 채로 떼어
    /// target(B) 위치로 replace 한다. B 는 닫힌다(PTY kill). **A 의 Terminal/store/
    /// scrollback 은 절대 만지지 않는다(PTY 보존 — R1).** 모든 위치 탐색은
    /// surface_id 검색식이라 focused_* 같은 사용자 포커스 상태에 의존하지 않는다
    /// (포커스 독립 원칙). 슬롯 비움도 여기서 처리한다.
    pub(super) fn apply_move_surface(
        engine: &mut crate::core::CoreState,
        source_id: u32,
        target_id: u32,
    ) -> CoreEvent {
        use crate::core::intent::CascadeLevel;

        // 이 intent 가 적용되는 시점에 cut 슬롯은 소비된다 (성공/no-op 무관).
        engine.pending_move_surface = None;

        let noop = || CoreEvent::MoveSurfaceApplied {
            moved: false,
            b_cleanup: None,
            cascade_level: CascadeLevel::Surface,
            closed_tab_ids: vec![],
            closed_pane_ids: vec![],
            workspace_purged: None,
            workspaces_now_empty: false,
        };

        // 가드 (명세 항목 6): self-ref / source 무효(이미 닫힘) / target 무효 → no-op.
        if source_id == target_id {
            return noop();
        }
        if engine.find_workspace_index_for_surface(source_id).is_none() {
            return noop();
        }
        if engine.find_workspace_index_for_surface(target_id).is_none() {
            return noop();
        }

        // 1) A 를 트리에서 떼어내 살아있는 Box 획득 (store 불변). sole 이면 A 의 옛
        //    tab/pane/workspace 를 구조적으로 닫고 그 cascade 정보를 함께 받는다.
        let (
            a_box,
            cascade_level,
            closed_tab_ids,
            closed_pane_ids,
            workspace_purged,
            workspaces_now_empty,
        ) = match Self::detach_surface_for_move(engine, source_id) {
            Some(v) => v,
            None => return noop(),
        };

        Self::attach_a_to_target(
            engine,
            source_id,
            target_id,
            a_box,
            cascade_level,
            closed_tab_ids,
            closed_pane_ids,
            workspace_purged,
            workspaces_now_empty,
        )
    }

    /// `apply_move_surface` 헬퍼 — 떼어낸 A(source) 를 B(target) 위치로 옮겨
    /// 붙인다. B 위치 재검색(1 단계가 인덱스를 바꿨을 수 있어 매번 id 재검색) /
    /// leaf replace / focused_surface 승계를 담당. 세 실패 분기 모두 구조적으로
    /// unreachable 인 방어 코드라 동일한 `moved: false` 이벤트 조립을 공유한다.
    #[allow(clippy::too_many_arguments)]
    fn attach_a_to_target(
        engine: &mut crate::core::CoreState,
        source_id: u32,
        target_id: u32,
        a_box: Box<dyn crate::model::Surface>,
        cascade_level: crate::core::intent::CascadeLevel,
        closed_tab_ids: Vec<u32>,
        closed_pane_ids: Vec<u32>,
        workspace_purged: Option<(usize, u32)>,
        workspaces_now_empty: bool,
    ) -> CoreEvent {
        // `moved: false` 이벤트도 1 단계가 실제로 지운 것(탭/pane/workspace)을 그대로
        // 싣는다. 소비자(`App::handle_core_event`)가 `moved` 로 cascade 를 막으므로
        // `workspace_purged` 는 이 분기에서 **쓰이지 않는다** — 그래도 비우지 않는 것은,
        // 이벤트가 "무슨 일이 일어났는가" 를 기술해야지 "소비자가 무엇을 쓸 것인가" 를
        // 미리 판단하면 안 되기 때문이다. 세 실패 분기 모두 구조적으로 unreachable 인
        // 방어 코드라(아래 각 주석) 실제로 여기 실린 값이 버려지는 일은 없다.
        let fail =
            |closed_tab_ids: &[u32], closed_pane_ids: &[u32]| CoreEvent::MoveSurfaceApplied {
                moved: false,
                b_cleanup: None,
                cascade_level,
                closed_tab_ids: closed_tab_ids.to_vec(),
                closed_pane_ids: closed_pane_ids.to_vec(),
                workspace_purged,
                workspaces_now_empty,
            };

        // 2) B 위치 *재검색* + b_tab_idx/b_persist 수집.
        let Some((ws_idx, pane_id, b_tab_idx, b_persist)) =
            Self::locate_target_slot(engine, source_id, target_id)
        else {
            return fail(&closed_tab_ids, &closed_pane_ids);
        };

        // 3) B leaf 를 A 로 replace. B 의 옛 id-marker 는 drop 되지만 B 의 Terminal 은
        //    아직 store 에 남아있다 → 4 단계 cleanup 이 PTY kill.
        let replaced = Self::replace_b_with_a(engine, ws_idx, pane_id, b_tab_idx, target_id, a_box);
        if !replaced {
            tracing::error!(
                source_id,
                target_id,
                "move surface: B replace failed (unreachable)"
            );
            return fail(&closed_tab_ids, &closed_pane_ids);
        }

        // B(target) 가 이 탭의 focused 였다면 그 자리를 A 가 승계하므로 focused_surface
        // 를 A 로 이어준다 (put_surface 는 sole 케이스에서 이미 A 로 세팅하지만, split
        // replace_surface 는 focused_surface 를 갱신하지 않아 dangling 방지 필요).
        // 그 후 새 focused 의 title 로 탭 제목을 재투영해 죽는 B 의 title 이 남지 않게 한다.
        Self::transfer_focus_to_a(engine, ws_idx, pane_id, b_tab_idx, source_id, target_id);
        engine.mark_layout_dirty();
        engine.refresh_tab_osc_title(source_id);

        CoreEvent::MoveSurfaceApplied {
            moved: true,
            b_cleanup: Some((target_id, b_persist)),
            cascade_level,
            closed_tab_ids,
            closed_pane_ids,
            workspace_purged,
            workspaces_now_empty,
        }
    }

    /// B(target) 위치 *재검색* (1 단계가 같은-tab 형제 끌어올림 / workspace 제거로
    /// 인덱스를 바꿨을 수 있음 — 인덱스 캐시 금지, 매번 id 재검색. 구조적 증명상
    /// B 는 A detach 후에도 항상 살아있다 — B≠A, 공유 구조면 형제 승격) + 그
    /// tab 안 인덱스(b_tab_idx) + scrollback persist_id 수집.
    fn locate_target_slot(
        engine: &crate::core::CoreState,
        source_id: u32,
        target_id: u32,
    ) -> Option<(usize, u32, usize, Option<String>)> {
        let (ws_idx, pane_id) = match engine.find_workspace_index_for_surface(target_id) {
            Some(v) => v,
            None => {
                tracing::error!(
                    source_id,
                    target_id,
                    "move surface: target vanished after detaching source (unreachable)"
                );
                return None;
            }
        };
        let b_persist = engine
            .terminals
            .scrollback_persist_id(target_id)
            .map(str::to_string);
        let b_tab_idx = {
            let ws = &engine.workspaces[ws_idx];
            match ws.pane_layout().find_pane(pane_id) {
                Some(pane) => pane.tabs.iter().position(|t| t.contains_surface(target_id)),
                None => None,
            }
        };
        let b_tab_idx = match b_tab_idx {
            Some(i) => i,
            None => {
                tracing::error!(
                    source_id,
                    target_id,
                    "move surface: B tab not found (unreachable)"
                );
                return None;
            }
        };
        Some((ws_idx, pane_id, b_tab_idx, b_persist))
    }

    /// B leaf 를 A 로 replace.
    fn replace_b_with_a(
        engine: &mut crate::core::CoreState,
        ws_idx: usize,
        pane_id: u32,
        b_tab_idx: usize,
        target_id: u32,
        a_box: Box<dyn crate::model::Surface>,
    ) -> bool {
        let ws = &mut engine.workspaces[ws_idx];
        let pane = ws
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .expect("pane re-search must hit (just found above)");
        let tab = &mut pane.tabs[b_tab_idx];
        if tab.is_split() {
            // split 안 leaf 교체 — tab name 불변.
            tab.layout_mut().replace_surface(target_id, a_box)
        } else {
            // B 가 sole 이던 tab — A 가 그 tab 의 단독 surface 가 된다.
            tab.put_surface(a_box);
            true
        }
    }

    /// B(target) 가 이 탭의 focused 였다면 A(source) 로 승계.
    fn transfer_focus_to_a(
        engine: &mut crate::core::CoreState,
        ws_idx: usize,
        pane_id: u32,
        b_tab_idx: usize,
        source_id: u32,
        target_id: u32,
    ) {
        let ws = &mut engine.workspaces[ws_idx];
        if let Some(pane) = ws.pane_layout_mut().find_pane_mut(pane_id) {
            let tab = &mut pane.tabs[b_tab_idx];
            if tab.focused_surface == target_id {
                tab.focused_surface = source_id;
            }
        }
    }

    /// `apply_move_surface` 헬퍼 — A(source) 를 트리에서 떼어 살아있는 Box 로 반환.
    /// **A 의 Terminal/store/scrollback 은 절대 만지지 않는다(PTY 보존).** A 가 split
    /// 안 leaf 면 형제를 끌어올리고(`Surface` level), sole-in-tab 이면 그 tab/pane/
    /// workspace 를 `apply_close_surface` Case 2/3/4 와 동형으로 구조적 close 한다 —
    /// 단 **A 의 cleanup_surface/terminals.remove/snapshot 은 일절 없다**(A 는 살아서
    /// 이동). A 못 찾으면 None.
    #[allow(clippy::type_complexity)]
    fn detach_surface_for_move(
        engine: &mut crate::core::CoreState,
        source_id: u32,
    ) -> Option<(
        Box<dyn crate::model::Surface>,
        crate::core::intent::CascadeLevel,
        Vec<u32>,
        Vec<u32>,
        // A 의 옛 자리가 workspace 째 사라졌다면 그 **(인덱스, id)**. 인덱스는
        // cascade 가 `active_workspace` 를 대상 기준으로 보정하는 데 쓴다.
        Option<(usize, u32)>,
        bool,
    )> {
        use crate::core::intent::CascadeLevel;

        let (ws_idx, pane_id) = engine.find_workspace_index_for_surface(source_id)?;

        // tab_idx + sole/split 판정.
        let (tab_idx, is_split) = {
            let ws = &engine.workspaces[ws_idx];
            let pane = ws.pane_layout().find_pane(pane_id)?;
            let mut found = None;
            for (i, tab) in pane.tabs.iter().enumerate() {
                if tab.contains_surface(source_id) {
                    found = Some((i, tab.is_split()));
                    break;
                }
            }
            found?
        };

        // Split tab: 형제 끌어올림, A 의 Box 만 반환. 구조적 close 없음.
        if is_split {
            let (a_box, source_tab_focused) = {
                let ws = &mut engine.workspaces[ws_idx];
                let pane = ws.pane_layout_mut().find_pane_mut(pane_id)?;
                let tab = &mut pane.tabs[tab_idx];
                let layout = tab.take_layout();
                let (new_layout, extracted) = layout.extract_surface(source_id);
                tab.put_layout(new_layout);
                // A 가 이 tab 의 focused 였다면 형제 승격에 맞춰 focused_surface 를
                // 살아있는 surface 로 재배정 (close_surface 와 동일 패턴, dangling 방지).
                if tab.focused_surface == source_id
                    && let Some(first_id) = tab.layout().first_surface_id()
                {
                    tab.focused_surface = first_id;
                }
                let a_box = extracted?; // split 안이면 형제가 있어 항상 Some.
                (a_box, tab.focused_surface)
            };
            engine.mark_layout_dirty();
            // A 가 떠난 source tab 의 제목을 새 focused(형제)의 title 로 재투영해
            // A 의 stale title 이 배경 탭에 남지 않게 한다.
            engine.refresh_tab_osc_title(source_tab_focused);
            return Some((a_box, CascadeLevel::Surface, vec![], vec![], None, false));
        }

        // sole-in-tab: 구조 정보 수집 후 A 의 Box salvage → 구조적 close.
        let (tabs_len, panes_len, tab_id) = {
            let ws = &engine.workspaces[ws_idx];
            let pane = ws.pane_layout().find_pane(pane_id)?;
            (
                pane.tabs.len(),
                ws.pane_layout().all_pane_ids().len(),
                pane.tabs[tab_idx].id,
            )
        };

        // sole leaf 에서 A 의 Box 추출 (tab.layout_opt 는 잠시 None — 동기 경로라 안전).
        let a_box = {
            let ws = &mut engine.workspaces[ws_idx];
            let pane = ws.pane_layout_mut().find_pane_mut(pane_id)?;
            let tab = &mut pane.tabs[tab_idx];
            match tab.take_layout() {
                crate::model::SurfaceLayout::Leaf(b) => b,
                other => {
                    // sole 인데 split — 예상 밖. 원복 후 포기.
                    tab.put_layout(other);
                    return None;
                }
            }
        };

        if tabs_len > 1 {
            // Case 2: tab close (pane/workspace 유지).
            let ws = &mut engine.workspaces[ws_idx];
            let pane = ws.pane_layout_mut().find_pane_mut(pane_id)?;
            pane.remove_tab_preserving_active(tab_idx);
            engine.mark_layout_dirty();
            return Some((a_box, CascadeLevel::Tab, vec![tab_id], vec![], None, false));
        }

        if panes_len > 1 {
            // Case 3: pane close (workspace 유지).
            let ws = &mut engine.workspaces[ws_idx];
            ws.close_pane_preserving_focus(pane_id);
            engine.mark_layout_dirty();
            return Some((
                a_box,
                CascadeLevel::Pane,
                vec![tab_id],
                vec![pane_id],
                None,
                false,
            ));
        }

        // Case 4: workspace close. (이동에서 A 가 sole-in-workspace 면 B 는 다른
        //  workspace 에 있으므로 workspaces 가 비지 않는다 — 그래도 일반식으로 계산.)
        let workspace_id = engine.workspaces[ws_idx].id;
        engine.workspaces.remove(ws_idx);
        let workspaces_now_empty = engine.workspaces.is_empty();
        engine.mark_layout_dirty();
        Some((
            a_box,
            CascadeLevel::Workspace,
            vec![tab_id],
            vec![pane_id],
            Some((ws_idx, workspace_id)),
            workspaces_now_empty,
        ))
    }
}

#[cfg(test)]
mod move_surface_tests {
    use super::*;
    use crate::model::SplitDirection;

    fn test_engine() -> CoreState {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    /// R1(PTY 보존) 잠금: A 를 B 위치로 이동해도 A 의 Terminal 은 store 에 그대로
    /// 남고(이동=분리+재부착, kill 아님), 이벤트는 B 를 cleanup 대상으로 보고한다.
    /// B 의 store 제거는 cascade(dispatch_domain) 책임이라 apply 단계에선 미발생.
    #[test]
    fn move_preserves_source_terminal_and_reports_b_cleanup() {
        let mut engine = test_engine();
        // 기본 워크스페이스의 단일 surface = A. detached mirror 를 직접 등록해
        // 실제 PTY 스폰 없이 deterministic 하게 store 점유를 만든다.
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine
            .terminals
            .insert(a, tasty_terminal::Terminal::new_detached(80, 24));

        // A 와 같은 tab 에 B 를 split 으로 추가.
        let b = 7777;
        let (ws_idx, pane_id) = engine.find_workspace_index_for_surface(a).unwrap();
        engine.workspaces[ws_idx]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .unwrap()
            .split_surface_by_id_marker(a, SplitDirection::Horizontal, b)
            .unwrap();
        engine
            .terminals
            .insert(b, tasty_terminal::Terminal::new_detached(80, 24));

        // 사전 조건.
        assert!(engine.terminals.contains(a));
        assert!(engine.terminals.contains(b));
        assert!(engine.find_workspace_index_for_surface(b).is_some());

        let ev = Core::apply_move_surface(&mut engine, a, b);

        // 이벤트: moved=true, B 가 cleanup 대상.
        match ev {
            CoreEvent::MoveSurfaceApplied {
                moved, b_cleanup, ..
            } => {
                assert!(moved, "move must succeed");
                assert_eq!(b_cleanup.map(|(id, _)| id), Some(b));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        // R1: A 의 Terminal 은 store 에 그대로 (PTY 보존).
        assert!(
            engine.terminals.contains(a),
            "source terminal must survive move"
        );
        // A 는 여전히 트리에 존재.
        assert!(engine.find_workspace_index_for_surface(a).is_some());
        // B 의 id-marker 는 replace 로 트리에서 사라짐. 단, B 의 Terminal store
        // 제거는 cascade(dispatch_domain) 책임이라 apply 직후엔 아직 남아있다.
        assert!(engine.find_workspace_index_for_surface(b).is_none());
        assert!(
            engine.terminals.contains(b),
            "apply 단계는 B store 를 건드리지 않는다 (cascade 가 kill)"
        );
        // cut 슬롯 소비.
        assert!(engine.pending_move_surface.is_none());
    }

    /// self-ref(source==target) 는 no-op.
    #[test]
    fn move_self_ref_is_noop() {
        let mut engine = test_engine();
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine.pending_move_surface = Some(a);
        let ev = Core::apply_move_surface(&mut engine, a, a);
        assert!(matches!(
            ev,
            CoreEvent::MoveSurfaceApplied { moved: false, .. }
        ));
        // no-op 여도 슬롯은 소비된다.
        assert!(engine.pending_move_surface.is_none());
    }

    /// target 부재(이미 닫힘) 는 no-op, A 는 무사.
    #[test]
    fn move_missing_target_is_noop() {
        let mut engine = test_engine();
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine
            .terminals
            .insert(a, tasty_terminal::Terminal::new_detached(80, 24));
        engine.pending_move_surface = Some(a);

        let ev = Core::apply_move_surface(&mut engine, a, 999_999);
        assert!(matches!(
            ev,
            CoreEvent::MoveSurfaceApplied { moved: false, .. }
        ));
        // A 는 그대로 살아있고 슬롯만 소비.
        assert!(engine.terminals.contains(a));
        assert!(engine.find_workspace_index_for_surface(a).is_some());
        assert!(engine.pending_move_surface.is_none());
    }
}
