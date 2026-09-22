use crate::core::CoreState;
#[cfg(test)]
use crate::model::SplitDirection;

use super::AppState;

/// Helper: tab 내 surface_id 에 해당하는 TerminalSurface 를 찾는다 (downcast).
fn terminal_surface_in_tab(
    tab: &crate::model::Tab,
    surface_id: u32,
) -> Option<&crate::model::TerminalSurface> {
    tab.layout_opt
        .as_ref()?
        .find_surface(surface_id)?
        .as_any()
        .downcast_ref::<crate::model::TerminalSurface>()
}

impl AppState {
    /// 이 close **요청**이 죽일 surface 중 원격이 하드 점유한 것이 있으면 거절한다.
    /// 거절이면 `true` 를 돌리고 호출부는 아무것도 하지 않는다.
    ///
    /// 하드 점유(ADR-0040)는 "이 surface 는 지금 원격 사용자가 쓰고 있다" 는 선언이다.
    /// 닫으면 그 세션이 예고 없이 죽고, 되돌리기 스택에 남는 것은 살아 있는 PTY 가 아니라
    /// **같은 명령으로 새 세션을 여는 레시피**라(`capture_workspace_snapshot`) 복구가 아니다.
    /// 그래서 close 는 거절이 맞다 — 같은 판정을 `workspace.close`·`surface.attention.clear`
    /// IPC 가 이미 한다(ADR-0120 ④). 이 메서드가 **그 규칙의 소유자**이고, 호출부는 자기가
    /// 죽일 대상 집합만 넘긴다.
    ///
    /// # 요청 경로만 이것을 본다 — 사후 정리 경로는 보면 안 된다
    ///
    /// PTY 가 스스로 끝나서 도는 정리(`cascade_terminal_process_exited`)는 이미 죽은
    /// 프로세스를 치우는 것이다. 거기서 거절하면 점유 락 때문에 **좀비 surface 가 영구히
    /// 남는다.** 그래서 검사는 공용 cascade 초크포인트(`close_surface_by_id_inner`)가 아니라
    /// **사용자 제스처·에이전트 요청의 진입점**에 붙인다.
    ///
    /// # 로컬 사용자가 막히지 않는 근거
    ///
    /// 하드 점유 surface 위에는 강제 해제 버튼이 그려진다(`adapters/ui/egui_panels.rs` 의
    /// `draw_occupied_overlays`) — 로컬 사용자는 그것을 눌러 점유를 끊고 닫을 수 있다.
    /// 그 버튼이 없었다면 이 거절은 "로컬에서 영영 못 닫는" 상태를 만들었을 것이다.
    pub(crate) fn refuse_if_hard_occupied(
        &mut self,
        engine: &CoreState,
        targets: impl IntoIterator<Item = u32>,
    ) -> bool {
        let Some(_occupied) = targets
            .into_iter()
            .find(|sid| engine.attach.is_hard_occupied(*sid))
        else {
            return false;
        };
        #[cfg(feature = "gui")]
        self.toasts.push(
            crate::i18n::t("attach.toast.close_blocked_hard_occupied"),
            crate::model::toast_kind::ToastKind::Warning,
            crate::model::toast_kind::ToastScope::Window,
        );
        true
    }

    /// active 워크스페이스가 mirror(원격 attach client)면 구조 변경을 원격으로
    /// **forward** 하고 `true` 를 돌려 로컬 실행/폴백 체인을 멈춘다(전체 mirror 뷰는
    /// 유지). mirror 워크스페이스는 원격 워크스페이스의 뷰라, 그 안의 구조 변경
    /// (new-tab / close / move-tab)을 로컬에서 실행하면 "workspace 전체가 remote"
    /// 불변식을 깬다 — 대신 원격 authority 로 forward 해 거기서 실행된다.
    ///
    /// `Core::apply` 를 우회하는 UI-layer 직접 조작 경로(`add_tab`·`close_active_*`)는
    /// split(intent→`Core::apply`→forward)과 달리 forward 가 자동으로 붙지 않으므로
    /// 여기서 직접 `op` 를 실어 준다. `op` 가 `None`(앵커 surface 를 못 찾음)이면
    /// forward 없이 차단 toast 만 띄운다.
    ///
    /// mirror 워크스페이스 **자체를 닫는 것**(`close_active_workspace`)은 로컬 mirror
    /// 뷰를 걷어내는 정당한 로컬 동작이므로 이 가드를 태우지 않는다.
    ///
    /// 이 메서드의 모든 호출부는 GUI 단축키/버튼/컨텍스트 메뉴 직접 조작이다(IPC/CLI
    /// 는 `Core::apply`→`DomainIntent` 경로만 탄다) — 그래서 항상 `user_triggered: true`
    /// 로 push 한다(new-tab/split focus 이동의 근거). `close_focus_candidates`(로컬 surface id, 우선순위 순)는 close
    /// 계열 호출부가 닫히기 **전** 트리에서 계산해 넘긴다 — 닫힌 surface 가 focus 였고
    /// 원격의 옛 focus 복원이 실패할 때 client-only fallback 대상이 된다. new-tab/
    /// split/move-tab 등 close 가 아닌 op 은 빈 벡터를 넘긴다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn forward_mirror_structural(
        &mut self,
        engine: &mut CoreState,
        op: Option<crate::ipc::stream::StructuralOp>,
        close_focus_candidates: Vec<u32>,
    ) -> bool {
        if !self.active_workspace(engine).mirror {
            return false;
        }
        match op {
            Some(op) => {
                engine
                    .pending_structural_forward
                    .push(crate::core::PendingStructuralForward {
                        op,
                        user_triggered: true,
                        close_focus_candidates,
                        silent_failure: false,
                    });
            }
            None => {
                #[cfg(feature = "gui")]
                self.toasts.push(
                    crate::i18n::t("attach.toast.mirror_structural_blocked"),
                    crate::model::toast_kind::ToastKind::Warning,
                    crate::model::toast_kind::ToastScope::Window,
                );
            }
        }
        true
    }

    /// pane 안에서 `closing_tab_index` 탭이 닫힐 때 client-side focus fallback 후보
    /// (로컬 surface id, 우선순위 순)를 반환한다. 로컬(비-mirror)의 "탭 하나만
    /// 남기고 닫음" 케이스(`close_case_tab`, `src/core/mod.rs`)와 동일한 규칙: 닫히는
    /// 탭이 마지막이 아니면 다음 탭, 마지막이면 이전 탭이 1순위. 그 슬롯도 못 쓰게 되는
    /// (예상 밖) 경우를 대비해 나머지 탭도 순서대로 방어적 fallback 으로 담는다. pane
    /// 에 탭이 하나뿐이면 빈 벡터(호출부가 기존 동작 — 원격 고정값 — 으로 남는다).
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn pane_sibling_tab_focus_candidates(
        pane: &crate::model::Pane,
        closing_tab_index: usize,
    ) -> Vec<u32> {
        let n = pane.tabs.len();
        if n <= 1 {
            return Vec::new();
        }
        let primary = if closing_tab_index + 1 < n {
            closing_tab_index + 1
        } else {
            closing_tab_index.wrapping_sub(1)
        };
        let mut out = Vec::new();
        if let Some(sid) = pane.tabs.get(primary).and_then(|t| t.focused_surface_id()) {
            out.push(sid);
        }
        for (idx, tab) in pane.tabs.iter().enumerate() {
            if idx == closing_tab_index || idx == primary {
                continue;
            }
            if let Some(sid) = tab.focused_surface_id() {
                out.push(sid);
            }
        }
        out
    }

    /// focused pane 의 active tab 안에서 `surface_id` 가 닫힐 때 client-side focus
    /// fallback 후보(로컬 surface id, 우선순위 순)를 반환한다. split 된 tab 이면
    /// 같은 tab 안의 다른 leaf surface(구조상 순서, `close_active_surface` 가 로컬
    /// 실행 시 쓰는 `Tab::close_surface`/`SurfaceLayout::close_surface` 의 "첫 leaf
    /// 승격"과 동형)를, split 안 된 tab(닫으면 탭 자체가 사라짐)이면
    /// [`pane_sibling_tab_focus_candidates`] 를 그대로 위임한다.
    #[cfg(any(feature = "gui", test))]
    fn active_surface_close_focus_candidates(
        &self,
        engine: &CoreState,
        surface_id: u32,
    ) -> Vec<u32> {
        let ws = self.active_workspace(engine);
        let Some(pane) = ws.pane_layout().find_pane(ws.focused_pane) else {
            return Vec::new();
        };
        let tab_index = pane.active_tab;
        let Some(tab) = pane.tabs.get(tab_index) else {
            return Vec::new();
        };
        if tab.is_split() {
            tab.layout_opt
                .as_ref()
                .map(|l| l.all_surface_ids())
                .unwrap_or_default()
                .into_iter()
                .filter(|&sid| sid != surface_id)
                .collect()
        } else {
            Self::pane_sibling_tab_focus_candidates(pane, tab_index)
        }
    }

    /// Close the focused pane (unsplit). Returns true if a pane was removed.
    #[cfg(any(feature = "gui", test))]
    pub fn close_active_pane(&mut self, engine: &mut CoreState) -> bool {
        // mirror 워크스페이스면 로컬 트리를 건드리지 않고 ClosePane 을 원격으로
        // forward 한다. true 를 돌려 fallback 체인(→ close_active_workspace)을 멈춘다.
        let mirror_op = self.focused_surface_id(engine).map(|sid| {
            crate::ipc::stream::StructuralOp::ClosePane {
                anchor_surface_id: sid,
            }
        });
        // pane 레벨 close 는 로컬도 무조건 "워크스페이스 첫 pane" 으로 이동하는 cascade
        // 케이스(`close_case_pane`)와 같은 성격이라 인접 후보를 계산하지 않는다 — close
        // focus fallback 의 스코프(같은 pane 안 인접 탭/surface)에 포함하지 않기로 한 결정.
        if self.forward_mirror_structural(engine, mirror_op, Vec::new()) {
            return true;
        }
        let target_id = self.active_workspace(engine).focused_pane;
        // 이 pane 이 품은 surface 중 하나라도 원격이 잡고 있으면 거절한다.
        let in_pane: Vec<u32> = {
            let ws = self.active_workspace(engine);
            ws.pane_layout()
                .find_pane(target_id)
                .map(|pane| {
                    let mut t: Vec<(u32, Option<String>)> = Vec::new();
                    for tab in &pane.tabs {
                        crate::core::impl_close::collect_close_targets(tab, engine, &mut t);
                    }
                    t.into_iter().map(|(sid, _)| sid).collect()
                })
                .unwrap_or_default()
        };
        if self.refuse_if_hard_occupied(engine, in_pane) {
            return false;
        }

        // Capture closed-item snapshot (전용 `close_pane` 단축키는 항상 사용자
        // 행동이라 조건 없이 캡처한다 — `close_active_surface`의 무조건 스냅샷과
        // 동일 관례). 캡처는 `close_pane`이 트리를 재배치하기 *전*이어야 한다 —
        // 제거 후엔 부모 Split 노드 자체가 사라져 split context 를 복구할 수 없다
        // (`CoreState::capture_closed_pane` 의 doc 이 그 제약을 소유한다).
        if let Some(item) = engine.capture_closed_pane(target_id) {
            engine.push_closed_item(item);
        }

        // Collect all (surface_id, persist_id) in the pane being closed for cleanup.
        let mut targets: Vec<(u32, Option<String>)> = Vec::new();
        {
            let ws = self.active_workspace(engine);
            if let Some(pane) = ws.pane_layout().find_pane(target_id) {
                for tab in &pane.tabs {
                    crate::core::impl_close::collect_close_targets(tab, engine, &mut targets);
                }
            }
        }

        let ws = self.active_workspace_mut(engine);
        let removed = ws.pane_layout_mut().close_pane(target_id);
        if removed {
            // Update focus to the first available pane
            if let Some(first) = ws.pane_layout().first_pane() {
                ws.focused_pane = first.id;
            }
            // cleanup_surface 직후엔 layout 에서 surface 가 사라져 kind 조회 불가 →
            // 미리 캡쳐. plugin lifecycle 큐에 cleanup_targets 모두 enqueue (R1 분석).
            for (sid, pid) in targets {
                let kind = self.surface_kind(engine, sid);
                self.cleanup_surface(engine, sid, pid);
                self.enqueue_surface_closed(sid, kind, true);
            }
            engine.mark_layout_dirty();
        }
        removed
    }

    /// Close the focused surface. For split tabs, closes the focused surface
    /// within the tab. For single-surface tabs, delegates to close_surface_by_id
    /// which handles tab/pane/workspace cascading.
    #[cfg(any(feature = "gui", test))]
    pub fn close_active_surface(&mut self, engine: &mut CoreState) -> bool {
        // mirror 워크스페이스면 로컬 트리를 건드리지 않고 CloseSurface 를 원격으로
        // forward 한다. true 를 돌려 호출부의 close fallback 체인을 멈춘다.
        let focused_sid = self.focused_surface_id(engine);
        let mirror_op = focused_sid
            .map(|sid| crate::ipc::stream::StructuralOp::CloseSurface { surface_id: sid });
        let candidates = focused_sid
            .map(|sid| self.active_surface_close_focus_candidates(engine, sid))
            .unwrap_or_default();
        if self.forward_mirror_structural(engine, mirror_op, candidates) {
            return true;
        }
        // 이 경로가 죽이는 것은 포커스된 surface 하나다 — tab/pane/workspace 로 cascade 하는
        // 경우도 그 surface 가 그 컨테이너의 유일한 거주자일 때뿐이라 대상 집합은 같다.
        if self.refuse_if_hard_occupied(engine, focused_sid) {
            return false;
        }
        let surface_id;
        // split 케이스의 closed-item 스냅샷용 tab_name — tab 이 mutate 되기 전(아래
        // `focused_pane_mut` 블록 이전)에 여기서 미리 구해둔다. close_case_split 과
        // 동일한 캡처 로직이지만, 여기선 이미 이 블록이 `surface_id` 를 얻으려고
        // `engine` 을 immutable 하게 빌리는 중이라 재사용한다(중복이면 헬퍼화가
        // 바람직하나, borrow 형태가 서로 달라 강제하지 않음).
        let mut tab_name_for_snapshot: Option<String> = None;
        if let Some(pane) = self.focused_pane(engine) {
            let tab = match pane.tabs.get(pane.active_tab) {
                Some(t) => t,
                None => return false,
            };
            surface_id = tab.focused_surface;
            if tab.is_split() && terminal_surface_in_tab(tab, surface_id).is_some() {
                tab_name_for_snapshot = Some(tab.display_name().to_string());
            }
        } else {
            return false;
        }
        let persist_id = engine
            .terminals
            .scrollback_persist_id(surface_id)
            .map(str::to_string);
        let kind = self.surface_kind(engine, surface_id);
        // tab.close_surface(surface_id) 로 mutate 되기 전에 스냅샷을 완성해둔다
        // (순서 뒤바뀌면 이미 제거된 surface 정보를 읽게 됨 — close_case_split 참고).
        let split_snapshot =
            tab_name_for_snapshot.map(|tab_name| crate::model::ClosedItem::Surface {
                surface: crate::model::closed_item::ClosedSurface::from_surface_id(
                    surface_id,
                    engine.terminals.get(surface_id),
                ),
                tab_name,
            });
        let split_handled;
        if let Some(pane) = self.focused_pane_mut(engine) {
            let tab = match pane.active_tab_mut() {
                Some(t) => t,
                None => return false,
            };
            if tab.is_split() {
                if !tab.close_surface(surface_id) {
                    return self.close_surface_by_id(engine, surface_id, true);
                }
                split_handled = true;
            } else {
                return self.close_surface_by_id(engine, surface_id, true);
            }
        } else {
            return false;
        }
        if split_handled {
            if let Some(item) = split_snapshot {
                engine.push_closed_item(item);
            }
            self.cleanup_surface(engine, surface_id, persist_id);
            self.enqueue_surface_closed(surface_id, kind, true);
            engine.mark_layout_dirty();
        }
        true
    }

    /// Close a specific surface by ID. Cascades up the hierarchy:
    /// surface -> tab -> pane -> workspace as needed.
    /// When `save_snapshot` is true, the closed item is saved for user restore (Ctrl+Shift+T).
    /// Agent/IPC closures should pass false to avoid polluting the user's undo stack.
    #[cfg(any(feature = "gui", test))]
    pub fn close_surface_by_id(
        &mut self,
        engine: &mut CoreState,
        surface_id: u32,
        is_user_close: bool,
    ) -> bool {
        self.close_surface_by_id_inner(engine, surface_id, true, is_user_close)
    }

    /// Close without saving snapshot (for IPC/agent-initiated closures).
    ///
    /// Agent 가 마지막 workspace 까지 닫아 windows 상태가 비어 버리면, 다음
    /// redraw 가 `active_workspace()` 를 호출하다 패닉한다. 사용자의 window 를
    /// 에이전트가 끄는 부작용도 피해야 하므로 (CLAUDE.md "사용자 행동과 에이전트
    /// 행동의 분리"), cascade 결과 workspaces 가 비면 즉시 새 empty workspace
    /// 를 만들어 invariant 를 유지한다.
    pub fn close_surface_by_id_no_snapshot(
        &mut self,
        engine: &mut CoreState,
        surface_id: u32,
        is_user_close: bool,
    ) -> bool {
        let closed = self.close_surface_by_id_inner(engine, surface_id, false, is_user_close);
        if closed {
            // Core 통과 없이 invariant 복구. 본 호출처는 PTY exit cleanup
            // (cascade_terminal_process_exited) 과 egui 의 diff close 두 곳에서 도달 — 둘 다
            // *시스템 invariant restorer* 라 host event 발화 불필요.
            self.recreate_workspace_if_empty(engine, "close_surface_by_id_no_snapshot");
        }
        closed
    }

    /// surface→tab→pane→workspace cascade close 의 인라인 실행형 디스패처.
    /// Step1 판정(공유 `locate_surface_in_pane`)으로 위치를 잡고 case1..4 메서드에
    /// 순차 위임한다. C2(`Core::apply_close_surface`)가 CoreEvent 를 *반환*해
    /// caller 가 cleanup 을 실행하는 것과 달리, 여기서는 각 case 메서드가
    /// cleanup/enqueue 를 *직접 실행*한다.
    ///
    /// # `save_snapshot` 과 `is_user_close` 를 왜 따로 받나
    ///
    /// [`AppState::close_workspace_at`] 은 같은 두 축을
    /// [`WorkspaceCloseOrigin`](crate::state::WorkspaceCloseOrigin) 하나로 접었다 —
    /// 그 경로에서는 두 값이 **항상 같이 움직이는데** 인자가 하나뿐이라 둘 중
    /// 하나만 갈리는 사고가 실제로 났기 때문이다. **여기서 같은 일을 하면 안 된다.**
    /// 이 경로는 두 축이 실제로 독립이고, 프로덕션에 서로 다른 조합이 둘 다 있다:
    ///
    /// | 호출자 | `save_snapshot` | `is_user_close` |
    /// |---|---|---|
    /// | egui 닫기([`AppState::close_active_surface`]) | `true` | `true` |
    /// | PTY 프로세스 종료 cleanup(`app::dispatch_domain` 의 `cascade_terminal_process_exited`) | **`false`** | `true` |
    ///
    /// 셸이 스스로 끝난 경우 되살릴 것이 없어 스냅샷을 남기지 않지만, 그 종료를
    /// 일으킨 것은 에이전트가 아니라 사람이므로 plugin 에는 사용자 close 로 나간다.
    /// 두 값을 하나로 접으면 이 조합에서 둘 중 하나가 반드시 틀린 값이 된다.
    /// 독립성은 `pty_exit_close_skips_the_snapshot_but_still_reports_a_user_close`
    /// 가 고정한다.
    fn close_surface_by_id_inner(
        &mut self,
        engine: &mut CoreState,
        surface_id: u32,
        save_snapshot: bool,
        is_user_close: bool,
    ) -> bool {
        let loc = match crate::core::locate_surface_in_pane(engine, surface_id) {
            Some(l) => l,
            None => return false,
        };
        if !loc.surface_is_sole_in_tab && loc.can_close_surface_in_group {
            return self.close_case_split(engine, &loc, surface_id, save_snapshot, is_user_close);
        }
        if self.close_case_tab(engine, &loc, save_snapshot, is_user_close) {
            return true;
        }
        if self.close_case_pane(engine, &loc, save_snapshot, is_user_close) {
            return true;
        }
        self.close_case_workspace(engine, &loc, save_snapshot, is_user_close)
    }

    /// Case 1: split tab 내 다중 surface 중 하나 close. 닫혔으면 true.
    fn close_case_split(
        &mut self,
        engine: &mut CoreState,
        loc: &crate::core::SurfaceCloseLocation,
        surface_id: u32,
        save_snapshot: bool,
        is_user_close: bool,
    ) -> bool {
        // Capture surface snapshot before closing (user actions only)
        if save_snapshot {
            let ws = &engine.workspaces[loc.ws_idx];
            let pane = ws.pane_layout().find_pane(loc.pane_id).unwrap();
            let tab = &pane.tabs[loc.tab_idx];
            if terminal_surface_in_tab(tab, surface_id).is_some() {
                let snapshot = crate::model::closed_item::ClosedSurface::from_surface_id(
                    surface_id,
                    engine.terminals.get(surface_id),
                );
                let tab_name = tab.display_name().to_string();
                engine.push_closed_item(crate::model::ClosedItem::Surface {
                    surface: snapshot,
                    tab_name,
                });
            }
        }
        // close 이전에 leaf surface 의 persist_id 를 추출해 둔다.
        let persist_id = engine
            .terminals
            .scrollback_persist_id(surface_id)
            .map(str::to_string);
        let kind = self.surface_kind(engine, surface_id);
        let ws = &mut engine.workspaces[loc.ws_idx];
        let pane = ws.pane_layout_mut().find_pane_mut(loc.pane_id).unwrap();
        let tab = &mut pane.tabs[loc.tab_idx];
        if tab.close_surface(surface_id) {
            self.cleanup_surface(engine, surface_id, persist_id);
            self.enqueue_surface_closed(surface_id, kind, is_user_close);
            engine.mark_layout_dirty();
            return true;
        }
        false
    }

    /// Case 2: surface 가 tab 유일 content 이고 pane.tabs>1 — tab close.
    /// 처리했으면 true, 조건 불충족이면 false(fallthrough).
    fn close_case_tab(
        &mut self,
        engine: &mut CoreState,
        loc: &crate::core::SurfaceCloseLocation,
        save_snapshot: bool,
        is_user_close: bool,
    ) -> bool {
        // Capture tab snapshot before removing (user actions only).
        // Must be done in a separate scope to avoid borrow conflicts.
        if save_snapshot {
            let ws = &engine.workspaces[loc.ws_idx];
            let pane = ws.pane_layout().find_pane(loc.pane_id).unwrap();
            if pane.tabs.len() > 1 {
                let snapshot_opt = {
                    let mut snap_fn =
                        crate::core::surface_registry::snapshot_fn_for(&engine.surface_registry);
                    let terminals = &engine.terminals;
                    crate::model::closed_item::ClosedTab::from_tab(
                        &pane.tabs[loc.tab_idx],
                        &mut snap_fn,
                        &|id| terminals.get(id),
                    )
                };
                if let Some(snapshot) = snapshot_opt {
                    engine.push_closed_item(crate::model::ClosedItem::Tab(snapshot));
                }
            }
        }
        // tab 의 모든 leaf surface 의 persist_id 수집 후 close.
        let mut targets: Vec<(u32, Option<String>)> = Vec::new();
        {
            let ws = &engine.workspaces[loc.ws_idx];
            let pane = ws.pane_layout().find_pane(loc.pane_id).unwrap();
            if pane.tabs.len() > 1 {
                crate::core::impl_close::collect_close_targets(
                    &pane.tabs[loc.tab_idx],
                    engine,
                    &mut targets,
                );
            }
        }
        let ws = &mut engine.workspaces[loc.ws_idx];
        let pane = ws.pane_layout_mut().find_pane_mut(loc.pane_id).unwrap();
        if pane.tabs.len() > 1 {
            pane.remove_tab_preserving_active(loc.tab_idx);
            for (sid, pid) in targets {
                let kind = self.surface_kind(engine, sid);
                self.cleanup_surface(engine, sid, pid);
                self.enqueue_surface_closed(sid, kind, is_user_close);
            }
            engine.mark_layout_dirty();
            return true;
        }
        false
    }

    /// Case 3: pane 의 마지막 tab 이고 ws 안 pane>1 — pane close.
    /// 처리했으면 true, 조건 불충족이면 false(fallthrough).
    fn close_case_pane(
        &mut self,
        engine: &mut CoreState,
        loc: &crate::core::SurfaceCloseLocation,
        save_snapshot: bool,
        is_user_close: bool,
    ) -> bool {
        // Capture pane snapshot before removing (user actions only). Split
        // context(sibling/direction/ratio/side)는 `close_pane`이 트리를
        // 재배치하기 *전*에 캡처해야 한다 — 제거 후엔 부모 Split 노드 자체가
        // 사라져 복구할 수 없다.
        if save_snapshot {
            let ws = &engine.workspaces[loc.ws_idx];
            if ws.pane_layout().all_pane_ids().len() > 1
                && let Some(pane) = ws.pane_layout().find_pane(loc.pane_id)
                && let Some((direction, ratio, was_first, sibling_pane_id)) =
                    ws.pane_layout().locate_split_context(loc.pane_id)
            {
                let snapshot = {
                    let mut snap_fn =
                        crate::core::surface_registry::snapshot_fn_for(&engine.surface_registry);
                    let terminals = &engine.terminals;
                    crate::model::ClosedItem::from_pane(
                        pane,
                        sibling_pane_id,
                        direction,
                        ratio,
                        was_first,
                        &mut snap_fn,
                        &|id| terminals.get(id),
                    )
                };
                engine.push_closed_item(snapshot);
            }
        }
        // pane 내 모든 tab 의 leaf surface persist_id 수집.
        let mut targets: Vec<(u32, Option<String>)> = Vec::new();
        {
            let ws = &engine.workspaces[loc.ws_idx];
            if ws.pane_layout().all_pane_ids().len() > 1
                && let Some(pane) = ws.pane_layout().find_pane(loc.pane_id)
            {
                for tab in &pane.tabs {
                    crate::core::impl_close::collect_close_targets(tab, engine, &mut targets);
                }
            }
        }
        let ws = &mut engine.workspaces[loc.ws_idx];
        if ws.pane_layout().all_pane_ids().len() > 1 {
            ws.close_pane_preserving_focus(loc.pane_id);
            for (sid, pid) in targets {
                let kind = self.surface_kind(engine, sid);
                self.cleanup_surface(engine, sid, pid);
                self.enqueue_surface_closed(sid, kind, is_user_close);
            }
            engine.mark_layout_dirty();
            return true;
        }
        false
    }

    /// Case 4 & 5: workspace 의 마지막 pane — workspace close. 항상 true.
    /// `target_kinds` 를 `workspaces.remove` **전에** 선캡처하고(remove 후
    /// surface_kind 조회 불가), memory purge + active_workspace 보정을 포함한다.
    ///
    /// 두 bool 을 각각 받는 이유는 [`AppState::close_surface_by_id_inner`] 의
    /// "왜 따로 받나" 참조 — 이 경로에서는 두 축이 독립이다.
    fn close_case_workspace(
        &mut self,
        engine: &mut CoreState,
        loc: &crate::core::SurfaceCloseLocation,
        save_snapshot: bool,
        is_user_close: bool,
    ) -> bool {
        use crate::close_trace;
        use std::time::Instant;

        /// close 계측의 경로 구분값 — surface→workspace cascade 를 한 함수 안에서
        /// 끝내는 인라인 디스패처.
        const PATH: &str = "inline";

        let t_close = Instant::now();
        // C1/C2 — snapshot 은 조건부다. `save_snapshot=false`(에이전트/PTY exit)면
        // 두 단계가 통째로 생략되고, 그 사실은 close_total 의 `snapshot` 필드에 남는다.
        if save_snapshot {
            let t = Instant::now();
            let item = Self::capture_workspace_snapshot(engine, loc.ws_idx);
            close_trace::log_snapshot(t, &item, PATH);
            let t = Instant::now();
            engine.push_closed_item(item).log(t.elapsed(), PATH);
        }
        // C3 — Workspace 전체의 모든 leaf surface persist_id 수집 (제거 전).
        let t = Instant::now();
        let targets = Self::collect_workspace_close_targets(engine, loc.ws_idx);
        close_trace::log_collect(t, targets.len(), PATH);
        // workspaces.remove 이후엔 surface_kind 조회 불가 → 미리 캡쳐.
        let target_kinds: Vec<Option<&'static str>> = targets
            .iter()
            .map(|(sid, _)| self.surface_kind(engine, *sid))
            .collect();
        let workspace_id = engine.workspaces[loc.ws_idx].id;
        engine.workspaces.remove(loc.ws_idx);
        self.fix_workspace_pointers_after_removal(loc.ws_idx, engine.workspaces.len());
        // C4 — 제거 후 공통 뒷정리(`workspace.closed` 발화 + workspace scope memory
        // purge). 이 경로가 이 호출을 빠뜨렸던 탓에, 워크스페이스의 마지막 터미널이
        // 스스로 종료돼 사라질 때만 plugin 이 `workspace.closed` 를 못 받았다.
        self.after_workspace_removed(workspace_id, PATH);
        let zipped: Vec<(u32, Option<String>, Option<&'static str>)> = targets
            .into_iter()
            .zip(target_kinds)
            .map(|((sid, pid), kind)| (sid, pid, kind))
            .collect();
        let surfaces = zipped.len();
        self.cleanup_targets(engine, zipped, is_user_close, Some(PATH));
        engine.mark_layout_dirty();
        close_trace::log_total(t_close, surfaces, save_snapshot, PATH);
        true
    }
}

#[cfg(test)]
impl AppState {
    /// Test-only helper: split the focused pane along the given direction.
    ///
    /// Production code uses `DomainIntent::SplitPane` dispatched through `Core`.
    /// 테스트 setup 편의 용도로만 직접 호출한다.
    pub(crate) fn test_split_pane(
        &mut self,
        engine: &mut CoreState,
        direction: SplitDirection,
    ) -> anyhow::Result<()> {
        let cwd = self.resolve_inherit_cwd(engine);
        let new_pane_id = engine.next_ids.next_pane();
        let new_tab_id = engine.next_ids.next_tab();
        let new_surface_id = engine.next_ids.next_surface();
        let cols = engine.default_cols;
        let rows = engine.default_rows;

        let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
        let terminal = crate::model::Pane::spawn_terminal(
            new_surface_id,
            crate::model::ShellSpawnOpts {
                cols,
                rows,
                shell: sh.shell_ref(),
                shell_args: &sh.args_ref(),
                extra_env: &sh.envs_ref(),
                waker: engine.make_waker(new_surface_id),
                working_dir: cwd.as_deref(),
            },
        )?;
        engine.terminals.insert(new_surface_id, terminal);
        let new_pane =
            crate::model::Pane::new_with_terminal_marker(new_pane_id, new_tab_id, new_surface_id);

        let ws = self.active_workspace_mut(engine);
        let target_pane_id = ws.focused_pane;
        ws.pane_layout_mut()
            .split_pane_in_place(target_pane_id, direction, new_pane);
        ws.focused_pane = new_pane_id;
        engine.send_fast_init(new_surface_id);
        engine.mark_layout_dirty();
        self.enqueue_host_event(super::PendingHostEvent::PaneSplit {
            original_pane: target_pane_id,
            new_pane: new_pane_id,
            direction,
        });
        Ok(())
    }
}

/// split 튜토리얼 관찰. 호출자는 구조 변경 cascade(`core::structural_cascade`)의 사용자
/// origin 분기다 — 튜토리얼 런타임이 gui 전용이라 이 관찰도 gui 에만 있다.
#[cfg(feature = "gui")]
impl AppState {
    /// surface split 뒤: 새 surface 가 든 tab 을 찾아 `SplitSurface` 연습을 기록한다.
    pub(crate) fn observe_tutorial_surface_split(
        &mut self,
        engine: &CoreState,
        workspace_index: usize,
        pane_id: u32,
        new_surface_id: u32,
    ) {
        if let Some(ws) = engine.workspaces.get(workspace_index)
            && let Some(pane) = ws.pane_layout().find_pane(pane_id)
            && let Some(tab) = pane
                .tabs
                .iter()
                .find(|tab| tab.contains_surface(new_surface_id))
        {
            self.tutorial
                .observe(crate::adapters::ui::tutorial::PracticeEvent::SplitSurface {
                    workspace: ws.id,
                    pane: pane_id,
                    tab: tab.id,
                });
        }
    }

    /// pane split 뒤: `SplitPane` 연습을 기록한다.
    pub(crate) fn observe_tutorial_pane_split(
        &mut self,
        workspace: u32,
        original: u32,
        new_pane: u32,
    ) {
        self.tutorial
            .observe(crate::adapters::ui::tutorial::PracticeEvent::SplitPane {
                workspace,
                original,
                new_pane,
            });
    }
}
