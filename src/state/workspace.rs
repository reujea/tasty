use crate::core::CoreState;

use super::AppState;

/// 워크스페이스 닫기를 **누가** 요청했는가.
///
/// 사용자 경로와 에이전트 경로는 세 가지 부수효과가 갈린다 — 되돌리기 스택에
/// 쌓는지, plugin `surface.closed` 에 실리는 reason 이 무엇인지, close 계측의
/// 경로값이 무엇인지. 셋 다 **이 값 하나에서 파생**시킨다.
///
/// 종전에는 `save_snapshot: bool` 하나만 인자였고 나머지 둘은 함수 안에 상수로
/// 박혀 있었다. 그래서 에이전트 경로를 추가했을 때 되돌리기 스택만 갈리고
/// plugin 에는 `LifecycleReason::User` 가, 계측에는 `snapshot=true` 가 그대로
/// 나갔다 — 같은 축을 나타내는 값이 셋인데 인자는 하나였던 형태가 원인이다.
/// **축 하나 = 인자 하나**로 고정해 그 실수가 다시 생기지 않게 한다.
///
/// 원칙 1(에이전트 행동의 부수효과는 사용자 상태에 닿지 않는다)은
/// [`docs/identity.md`] — 집행 지점이 여기다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCloseOrigin {
    /// 사용자가 자기 손으로 닫았다 — 단축키 · 사이드바 컨텍스트 메뉴 · 사용자
    /// 입력을 재현하는 debug 전용 IPC.
    ///
    /// 만드는 자리가 그 셋(gui · debug IPC)과 테스트뿐이라 release 헤드리스에는 이 값이
    /// 없다 — 그 조합에서 워크스페이스를 닫는 경로는 에이전트뿐이다.
    #[cfg(any(feature = "gui", debug_assertions, test))]
    User,
    /// 에이전트가 release IPC/CLI(`workspace.close`)로 닫았다.
    Agent,
}

impl WorkspaceCloseOrigin {
    /// 사용자의 "닫은 항목" 되돌리기 스택(`Ctrl+Shift+T`)에 쌓을지.
    /// 그 스택은 사용자가 자기 손으로 닫은 것만 담는다.
    fn saves_snapshot(self) -> bool {
        self.is_user()
    }

    /// plugin `surface.closed` payload 의 `reason` — `LifecycleReason::User` 인지
    /// `::Ipc` 인지를 가른다(`app::dispatch::surface_lifecycle`).
    fn is_user_close(self) -> bool {
        self.is_user()
    }

    /// [`Self::User`] 인가. `matches!` 가 아니라 `match` 인 것은 그 variant 가 cfg 에
    /// 걸려 있어서다 — 없는 조합에서도 이 판정은 컴파일되어야 한다.
    fn is_user(self) -> bool {
        match self {
            #[cfg(any(feature = "gui", debug_assertions, test))]
            Self::User => true,
            Self::Agent => false,
        }
    }

    /// close 계측(`close_trace`)의 경로 구분값.
    fn trace_path(self) -> &'static str {
        match self {
            #[cfg(any(feature = "gui", debug_assertions, test))]
            Self::User => "gui",
            Self::Agent => "ipc",
        }
    }
}

/// 워크스페이스 하나가 `removed_idx` 에서 제거된 뒤, **인덱스로 저장된 활성 포인터**가
/// 계속 같은 워크스페이스를 가리키도록 보정한 값.
///
/// `active_workspace` 는 인덱스가 진실 소스라, 앞쪽 워크스페이스가 빠지면 뒤 워크스페이스
/// 들이 한 칸씩 당겨지면서 손대지 않은 인덱스가 **다른 워크스페이스**를 가리키게 된다.
/// 사용자가 보고 있던 것을 닫은 경우(`active == removed_idx`)에만 시야가 움직이고, 그때는
/// 그 자리로 밀려 들어온 워크스페이스(마지막이었다면 직전 것)로 착지한다.
/// 근거: [`docs/design/policies/focus.md`] "삭제로 인한 인덱스 이동".
///
/// `remaining` 은 제거 **후** 남은 워크스페이스 수다. 0 이면 호출자가 곧 workspace 를
/// 자동 재생성하므로(빈 화면 invariant) 0 을 돌려준다.
pub(crate) fn active_index_after_removal(
    active: usize,
    removed_idx: usize,
    remaining: usize,
) -> usize {
    if remaining == 0 {
        return 0;
    }
    if active > removed_idx {
        active - 1
    } else if active == removed_idx {
        active.min(remaining - 1)
    } else {
        active
    }
}

/// 워크스페이스가 `from` 에서 `to` 로 옮겨진 뒤, **인덱스로 저장된 활성 포인터**가
/// 계속 같은 워크스페이스를 가리키도록 보정한 값.
///
/// 세 갈래다. (1) 옮겨진 것을 보고 있었으면 따라간다(`active == from` → `to`).
/// (2) 뒤로 옮겨(`from < to`) 그 구간을 자기 위치가 통과당하면 한 칸 당겨진다.
/// (3) 앞으로 옮겨(`from > to`) 그 구간에 자기 위치가 들어가면 한 칸 밀린다.
/// 구간 밖이면 그대로다. 제거 축의 [`active_index_after_removal`] 과 같은 역할이다.
pub(crate) fn active_index_after_move(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else if from < to && active > from && active <= to {
        active - 1
    } else if from > to && active >= to && active < from {
        active + 1
    } else {
        active
    }
}

impl AppState {
    /// 워크스페이스가 하나도 남지 않았으면 기본 워크스페이스를 만들어 활성으로 삼는다. 만들었으면
    /// `true`.
    ///
    /// 창이 워크스페이스 0 개로 남으면 다음 redraw 의 `active_workspace()` 가 panic 한다. 이
    /// 함수를 부르는 자리는 **사용자가 창을 닫으려 한 것이 아닌** 제거다(원격 끊김으로 mirror
    /// 워크스페이스가 사라지는 것 등) — 그 제거가 사용자 창을 닫게 두지 않는다(identity 원칙 1).
    /// 시스템 invariant restorer 라 host event 는 내지 않는다
    /// ([`crate::core::Core::create_default_workspace`] 와 같은 의미 — 에이전트가 마지막 surface 를
    /// 닫은 경우의 `close_surface_by_id_no_snapshot` 도 같은 모양이다).
    pub(crate) fn recreate_workspace_if_empty(
        &mut self,
        engine: &mut CoreState,
        context: &str,
    ) -> bool {
        if !engine.workspaces.is_empty() {
            return false;
        }
        match crate::core::apply_create_workspace_inner(
            engine,
            crate::core::WorkspaceCreationParams::terminal(),
        ) {
            Ok(crate::core::intent::CoreEvent::WorkspaceCreated { index, .. }) => {
                self.active_workspace = index;
                true
            }
            Ok(_) => unreachable!("apply_create_workspace_inner 는 WorkspaceCreated 만 반환"),
            Err(e) => {
                tracing::warn!("{context}: auto-recreate workspace failed: {e}");
                false
            }
        }
    }

    /// 워크스페이스 제거 직후, 인덱스를 값으로 들고 있는 활성 포인터를 대상 기준으로
    /// 보정한다.
    ///
    /// 지금 그런 포인터는 `active_workspace` **하나뿐**이다 — 카테고리 quick-switch
    /// 착지점([`AppState::category_last_active`])은 id 를 들고 있어 이 밀림을 겪지
    /// 않는다. 인덱스를 값으로 드는 상태를 새로 만들면 여기에 등록한다.
    ///
    /// 호출자는 `engine.workspaces.remove(removed_idx)` **직후**에 부른다.
    pub(crate) fn fix_workspace_pointers_after_removal(
        &mut self,
        removed_idx: usize,
        remaining: usize,
    ) {
        self.active_workspace =
            active_index_after_removal(self.active_workspace, removed_idx, remaining);
    }

    /// 워크스페이스 재정렬 직후, 인덱스를 값으로 들고 있는 활성 포인터를 대상 기준으로
    /// 보정한다 — 제거 축의
    /// [`fix_workspace_pointers_after_removal`](Self::fix_workspace_pointers_after_removal)
    /// 과 대칭이다.
    ///
    /// 재정렬은 **두 경로**로 들어온다(사이드바 드래그·컨텍스트 메뉴가 부르는
    /// [`move_workspace`](Self::move_workspace), 그리고 `CoreEvent::WorkspaceMoved`
    /// 의 `cascade_workspace_moved`). 같은 보정 규칙을 두 곳이 각자 인라인으로
    /// 복제하고 있었고, 그 형태가 실제로 한쪽만 갱신되는 결함의 모양이다
    /// (ADR-0113 이 제거 축에서 같은 진단을 했다). 규칙은 여기 하나뿐이어야 한다.
    ///
    /// 호출자는 `workspaces` 재배열 **직후**에 부른다.
    pub(crate) fn fix_workspace_pointers_after_move(&mut self, from: usize, to: usize) {
        self.active_workspace = active_index_after_move(self.active_workspace, from, to);
    }

    /// Switch to workspace by index (0-based).
    // 부르는 자리가 gui · debug `debug.switch_workspace` · 테스트뿐이다(원칙 1/3 — release
    // IPC 에는 활성 워크스페이스를 옮기는 표면이 없다).
    #[cfg(any(feature = "gui", debug_assertions, test))]
    pub fn switch_workspace(&mut self, engine: &mut CoreState, index: usize) {
        if index < engine.workspaces.len() {
            self.active_workspace = index;
            // 카테고리별 last-active 기록 — 카테고리 quick-switch(T4WS ⑤) 착지점.
            let cat = engine.workspaces[index].category;
            self.category_last_active
                .insert(cat, engine.workspaces[index].id);
            self.ensure_active_workspace_initialized(engine);
        }
    }

    /// 섹션 순서 인덱스(0=reserved normal, 1.. = 사용자 카테고리)로 **카테고리 자체**를
    /// 전환한다 (T4WS ②⑤, `Alt+Shift+숫자`). folders 기능 on 에서만 호출된다.
    ///
    /// 동작: (1) 대상 카테고리가 접혀 있으면 **auto-expand**(persist) — 접힌 채면 착지
    /// 워크스페이스가 안 보이므로 펼치는 게 옳다. (2) 그 카테고리의 **last-active**
    /// 워크스페이스로 착지(없거나 stale 이면 **first**). 착지는 전역 인덱스 SoT 를 쓰는
    /// [`switch_workspace`](Self::switch_workspace) 재사용.
    //
    // 사용자 키 경로로만 호출(원칙 1/3: active_workspace 이동은 release IPC/CLI 노출 금지).
    #[cfg(any(feature = "gui", test))]
    pub fn switch_to_category(&mut self, engine: &mut CoreState, section_idx: usize) {
        let Some(cat) = engine.categories().get(section_idx).map(|c| c.id) else {
            return;
        };
        // (1) auto-expand + persist.
        let collapsed = engine
            .categories()
            .get(section_idx)
            .is_some_and(|c| c.collapsed);
        if collapsed {
            engine.set_category_collapsed(cat, false);
            engine.mark_layout_dirty();
        }
        // (2) last-active(소속 재검증) → 없으면 first-in-category.
        // id → 전역 인덱스. 제거됐거나 다른 카테고리로 옮겨졌으면 못 찾고 first 로 간다.
        // 워크스페이스 수가 수십 규모이고 이 조회는 사용자 키 입력당 한 번이라 선형
        // 탐색으로 충분하다.
        let target = self
            .category_last_active
            .get(&cat)
            .copied()
            .and_then(|ws_id| {
                engine
                    .workspaces
                    .iter()
                    .position(|w| w.id == ws_id && w.category == cat)
            })
            .or_else(|| {
                engine
                    .workspaces_in_category(cat)
                    .first()
                    .map(|(gi, _)| *gi)
            });
        if let Some(global) = target {
            self.switch_workspace(engine, global);
        }
    }

    /// 카테고리-로컬 인덱스로 전환 (S-WSCAT). 현재 active 워크스페이스가 속한
    /// 카테고리의 로컬 목록에서 `local_idx` 번째를 골라 **전역 인덱스로 변환**한 뒤
    /// 기존 [`switch_workspace`](Self::switch_workspace) 를 재사용한다. 전역 인덱스가
    /// 단일 진실 소스이므로 move/close/cascade 의 active 보정 로직을 그대로 쓴다.
    /// 카테고리 토글 off 거나 active 카테고리에 `local_idx` 가 없으면 no-op.
    #[cfg(feature = "gui")]
    pub fn switch_workspace_in_active_category(
        &mut self,
        engine: &mut CoreState,
        local_idx: usize,
    ) {
        if self.active_workspace >= engine.workspaces.len() {
            return;
        }
        let cat = engine.workspaces[self.active_workspace].category;
        let global = engine
            .workspaces_in_category(cat)
            .get(local_idx)
            .map(|(gi, _)| *gi);
        if let Some(global) = global {
            self.switch_workspace(engine, global);
        }
    }

    /// 현재 active 워크스페이스가 속한 카테고리 내에서 **다음** 워크스페이스로 이동한다.
    /// 표시(=저장) 순서를 따르고, 마지막에서 다음으로 가면 `workspace_switch_crosses_category`
    /// 설정에 따라 같은 카테고리의 첫 항목으로 wrap-around 하거나(off, 기본) 다음
    /// 카테고리의 첫 워크스페이스로 넘어간다(on). 카테고리에 자기 자신뿐이면 옵션 off 시
    /// no-op(`Pane::next_tab` 의 `len > 1` 가드와 동형), on 이면 인접 카테고리로 이동.
    /// 전역 인덱스만 뽑아 불변 빌림을 끝낸 뒤 [`switch_workspace`](Self::switch_workspace)
    /// 를 재사용하므로 active 보정 로직을 그대로 탄다.
    //
    // quick-switch 키바인딩(`handle_numeric_switch_shortcuts`)에서 **사용자 키 경로로만** 호출된다. (원칙 1/3:
    // active_workspace 를 바꾸는 사용자 포커스 이동 — release IPC/CLI 로 노출 금지.)
    #[cfg(any(feature = "gui", test))]
    pub fn next_workspace_in_active_category(&mut self, engine: &mut CoreState) {
        if let Some(target) = self.relative_workspace_in_active_category(engine, 1) {
            self.switch_workspace(engine, target);
        }
    }

    /// 현재 active 워크스페이스가 속한 카테고리 내에서 **이전** 워크스페이스로 이동한다.
    /// [`next_workspace_in_active_category`](Self::next_workspace_in_active_category) 의
    /// 역방향(첫 항목에서 이전으로 가면 `workspace_switch_crosses_category` off 시 마지막
    /// 항목으로 wrap-around, on 시 이전 카테고리의 마지막 워크스페이스로 이동).
    // quick-switch 키바인딩에서 사용자 키 경로로 호출 (next_ 동일).
    #[cfg(any(feature = "gui", test))]
    pub fn prev_workspace_in_active_category(&mut self, engine: &mut CoreState) {
        if let Some(target) = self.relative_workspace_in_active_category(engine, -1) {
            self.switch_workspace(engine, target);
        }
    }

    /// active 워크스페이스가 속한 카테고리 로컬 목록에서 `delta`(±1) 만큼 이동한 대상의
    /// **전역 인덱스** 를 반환한다. active OOB · 로컬 위치 미검출(방어) 시 `None`.
    /// 반환값은 usize 복사본이라 호출부에서 불변 빌림 없이 가변 `switch_workspace` 를
    /// 호출할 수 있다.
    ///
    /// `workspace_switch_crosses_category` 옵션이 on 이고 이동이 카테고리 경계를 벗어나면
    /// [`relative_category_boundary_workspace`](Self::relative_category_boundary_workspace)
    /// 로 인접 카테고리의 첫/마지막 워크스페이스로 넘어간다(카테고리가 1개뿐이라 넘어갈
    /// 곳이 없으면 아래 로컬 wrap 으로 자연히 폴백). off 이거나 로컬 목록이 1개 이하면
    /// 기존과 동일하게 카테고리 로컬 wrap 만 수행한다.
    #[cfg(any(feature = "gui", test))]
    fn relative_workspace_in_active_category(
        &self,
        engine: &CoreState,
        delta: isize,
    ) -> Option<usize> {
        if self.active_workspace >= engine.workspaces.len() {
            return None;
        }
        let cat = engine.workspaces[self.active_workspace].category;
        let locals = engine.workspaces_in_category(cat);
        let len = locals.len();
        let pos = locals
            .iter()
            .position(|(gi, _)| *gi == self.active_workspace)?;

        if engine.settings.general.workspace_switch_crosses_category {
            let raw = pos as isize + delta;
            if raw < 0 || raw >= len as isize {
                if let Some(target) = self.relative_category_boundary_workspace(engine, delta) {
                    return Some(target);
                }
                // 인접 카테고리가 없음(카테고리 1개) → 아래 로컬 wrap 으로 폴백.
            } else {
                return Some(locals[raw as usize].0);
            }
        }

        if len <= 1 {
            return None;
        }
        // len - 1 == delta.rem_euclid 을 위한 wrap: (pos + len ± 1) % len.
        let new_pos = (pos as isize + delta).rem_euclid(len as isize) as usize;
        Some(locals[new_pos].0)
    }

    /// `relative_workspace_in_active_category` 가 카테고리 경계를 넘을 때 호출한다.
    /// `delta` 방향의 인접 카테고리로 넘어가 그 카테고리의 **첫**(다음 방향, `delta > 0`)
    /// 또는 **마지막**(이전 방향) 워크스페이스의 전역 인덱스를 반환한다.
    /// [`switch_to_category`](Self::switch_to_category) 의 last-active 착지와 달리
    /// 항상 방향에 맞는 끝 원소로 착지해야 방향성이 유지된다. 카테고리가 1개 이하이면
    /// [`relative_category_section`](Self::relative_category_section) 이 `None` 을
    /// 반환해 호출부가 로컬 wrap 으로 폴백한다.
    #[cfg(any(feature = "gui", test))]
    fn relative_category_boundary_workspace(
        &self,
        engine: &CoreState,
        delta: isize,
    ) -> Option<usize> {
        let section_idx = self.relative_category_section(engine, delta)?;
        let target_cat = engine.categories().get(section_idx)?.id;
        let locals = engine.workspaces_in_category(target_cat);
        if delta > 0 {
            locals.first().map(|(gi, _)| *gi)
        } else {
            locals.last().map(|(gi, _)| *gi)
        }
    }

    /// 현재 active 워크스페이스가 속한 카테고리의 **다음** 카테고리로 전환한다(T4WS
    /// 카테고리 축 quick-switch next/prev). `engine.categories()`(0=reserved normal,
    /// 1.. = 사용자 카테고리) 리스트 안에서 현재 카테고리 위치를 찾아 `rem_euclid` 로
    /// wrap-around ±1 이동한 뒤 그 section_idx 로 [`switch_to_category`](Self::switch_to_category)
    /// 를 재사용한다(auto-expand + last-active 착지 포함). 카테고리가 1개 이하면
    /// no-op([`relative_workspace_in_active_category`] 의 `len <= 1` 가드와 동형).
    //
    // quick-switch 키바인딩에서 **사용자 키 경로로만** 호출된다(원칙 1/3).
    #[cfg(any(feature = "gui", test))]
    pub fn next_category(&mut self, engine: &mut CoreState) {
        if let Some(section_idx) = self.relative_category_section(engine, 1) {
            self.switch_to_category(engine, section_idx);
        }
    }

    /// 현재 active 워크스페이스가 속한 카테고리의 **이전** 카테고리로 전환한다.
    /// [`next_category`](Self::next_category) 의 역방향(wrap-around 포함).
    #[cfg(any(feature = "gui", test))]
    pub fn prev_category(&mut self, engine: &mut CoreState) {
        if let Some(section_idx) = self.relative_category_section(engine, -1) {
            self.switch_to_category(engine, section_idx);
        }
    }

    /// active 워크스페이스가 속한 카테고리로부터 `delta`(±1) 만큼 wrap-around 이동한
    /// 카테고리의 **section_idx**(= `engine.categories()` 리스트 내 위치)를 반환한다.
    /// active OOB · 카테고리 1개 이하 · 위치 미검출(방어) 시 `None`.
    #[cfg(any(feature = "gui", test))]
    fn relative_category_section(&self, engine: &CoreState, delta: isize) -> Option<usize> {
        if self.active_workspace >= engine.workspaces.len() {
            return None;
        }
        let cat = engine.workspaces[self.active_workspace].category;
        let categories = engine.categories();
        let len = categories.len();
        if len <= 1 {
            return None;
        }
        let pos = categories.iter().position(|c| c.id == cat)?;
        let new_pos = (pos as isize + delta).rem_euclid(len as isize) as usize;
        Some(new_pos)
    }

    /// Move a workspace from one index to another, adjusting active_workspace accordingly.
    /// Returns false if indices are out of bounds or equal.
    #[cfg(any(feature = "gui", test))]
    pub fn move_workspace(&mut self, engine: &mut CoreState, from: usize, to: usize) -> bool {
        let len = engine.workspaces.len();
        if from == to || from >= len || to >= len {
            return false;
        }
        let ws = engine.workspaces.remove(from);
        engine.workspaces.insert(to, ws);
        self.fix_workspace_pointers_after_move(from, to);
        true
    }

    /// 활성 workspace에서 사용자가 보고 있는 active_tab의 deferred surface(들)만 PTY를
    /// spawn. 같은 pane의 비활성 tab은 deferred로 남았다가 tab 전환 시 깨어난다.
    /// active_tab이 split layout이면 그 안의 모든 deferred placeholder를 한번에 spawn한다.
    // [`Self::switch_workspace`] 만 부른다 — 같은 게이트.
    #[cfg(any(feature = "gui", debug_assertions, test))]
    fn ensure_active_workspace_initialized(&mut self, engine: &mut CoreState) {
        let mut spawned: Vec<(u32, tasty_terminal::Terminal, Option<String>)> = Vec::new();
        {
            let ws = &mut engine.workspaces[self.active_workspace];
            let pane_ids: Vec<u32> = ws.pane_layout().all_pane_ids();
            for pane_id in pane_ids {
                if let Some(pane) = ws.pane_layout_mut().find_pane_mut(pane_id) {
                    let active_idx = pane.active_tab;
                    if let Some(tab) = pane.tabs.get_mut(active_idx) {
                        let mut entries = tab.ensure_all_initialized();
                        spawned.append(&mut entries);
                    }
                }
            }
        }
        for (surface_id, terminal, persist_id) in spawned {
            engine.terminals.insert(surface_id, terminal);
            if let Some(pid) = persist_id {
                engine.terminals.set_scrollback_persist_id(surface_id, pid);
            }
            engine.send_fast_init(surface_id);
            engine.apply_pending_scrollback_inject(surface_id);
        }
    }

    /// Close the active workspace. Returns true if the workspace was removed.
    /// Cleans up all surfaces (surface meta + per-surface view state) in the workspace.
    #[cfg(feature = "gui")]
    pub fn close_active_workspace(&mut self, engine: &mut CoreState) -> bool {
        self.close_workspace_at(engine, self.active_workspace, WorkspaceCloseOrigin::User)
    }

    /// Close a specific workspace by index (context menu 등 임의 지정 close).
    /// Cleans up all surfaces + memory scope purge.
    ///
    /// 사용자 경로와 에이전트 경로의 차이는 [`WorkspaceCloseOrigin`] 하나로만
    /// 표현한다 — 갈리는 부수효과 셋(되돌리기 스택 / plugin 에 실리는 close
    /// reason / close 계측 경로값)을 전부 거기서 파생시킨다.
    ///
    /// 포커스는 어느 경로든 `fix_workspace_pointers_after_removal` 이 제거 직후
    /// 보정한다 — 보던 워크스페이스가 아닌 것을 닫으면 화면은 그대로다.
    /// `workspace.closed` host event 는 origin 과 무관하게 발화한다: 워크스페이스가
    /// 사라졌다는 사실 자체는 누가 닫았든 동일하다. 발화는 이 함수가 직접 하지 않고
    /// 제거 경로 셋이 공유하는 초크포인트
    /// [`AppState::after_workspace_removed`] 가 한다 — cascade 경로 둘(Core cascade ·
    /// `state/pane.rs` 의 인라인 cascade)도 같은 자리를 지난다.
    pub fn close_workspace_at(
        &mut self,
        engine: &mut CoreState,
        ws_idx: usize,
        origin: WorkspaceCloseOrigin,
    ) -> bool {
        use crate::close_trace;
        use std::time::Instant;

        let save_snapshot = origin.saves_snapshot();
        let path = origin.trace_path();

        if ws_idx >= engine.workspaces.len() {
            return false;
        }
        // 이 워크스페이스가 품은 surface 중 하나라도 원격이 하드 점유 중이면 거절한다.
        // `workspace.close` IPC 는 더 자세한 사유를 실어 이 앞에서 이미 거절한다 — 여기
        // 검사는 그 경로의 중복이 아니라 **규칙의 소유자**다(GUI 컨텍스트 메뉴 ·
        // `close_active_workspace` 단축키 · debug IPC 가 전부 이리로 온다).
        let in_ws = engine.workspaces[ws_idx].all_surface_ids();
        if self.refuse_if_hard_occupied(engine, in_ws) {
            return false;
        }
        let t_close = Instant::now();
        // C1/C2 — snapshot 은 조건부다. 에이전트 경로는 두 단계를 통째로 건너뛴다.
        if save_snapshot {
            let t = Instant::now();
            let snapshot = super::AppState::capture_workspace_snapshot(engine, ws_idx);
            close_trace::log_snapshot(t, &snapshot, path);
            // C2 — restore.command 주입 + 스크롤백 디스크 write + evict.
            let t = Instant::now();
            engine.push_closed_item(snapshot).log(t.elapsed(), path);
        }
        // C3 — Collect all (surface_id, persist_id) for cleanup before removing.
        let t = Instant::now();
        let targets = super::AppState::collect_workspace_close_targets(engine, ws_idx);
        close_trace::log_collect(t, targets.len(), path);
        let workspace_id = engine.workspaces[ws_idx].id;
        engine.workspaces.remove(ws_idx);
        // C4 — Workspace scope 의 memory entry 정리. 안의 surface 들은 아래
        // cleanup_surface 에서 각자 자기 scope 를 purge 한다.
        self.after_workspace_removed(workspace_id, path);
        // 활성 포인터를 대상 기준으로 보정 — 앞쪽 워크스페이스를 닫아도 보고 있던
        // 워크스페이스가 그대로 남는다.
        self.fix_workspace_pointers_after_removal(ws_idx, engine.workspaces.len());
        // Cleanup
        // workspace.remove 후엔 surface_kind 가 None 을 반환할 수 있으나, plugin
        // lifecycle 구독자는 surface_id 만으로 cleanup 가능 (R1 분석).
        let zipped: Vec<(u32, Option<String>, Option<&'static str>)> = targets
            .into_iter()
            .map(|(sid, pid)| {
                let kind = self.surface_kind(engine, sid);
                (sid, pid, kind)
            })
            .collect();
        let surfaces = zipped.len();
        self.cleanup_targets(engine, zipped, origin.is_user_close(), Some(path));
        engine.mark_layout_dirty();
        close_trace::log_total(t_close, surfaces, save_snapshot, path);
        true
    }
}

#[cfg(test)]
mod workspace_pointer_tests {
    //! workspace **제거**·**재정렬** 후 인덱스 활성 포인터 보정 규칙
    //! (`active_index_after_removal` · `active_index_after_move`)과 그 적용
    //! (`fix_workspace_pointers_after_removal` · `fix_workspace_pointers_after_move`)
    //! 을 고정한다.
    use super::*;

    #[test]
    fn removing_an_earlier_workspace_shifts_the_active_pointer_down() {
        // [0,1,2,3] 에서 사용자는 2 를 보는 중, 0 이 제거됨 → 같은 대상은 이제 1.
        assert_eq!(active_index_after_removal(2, 0, 3), 1);
    }

    #[test]
    fn removing_a_later_workspace_leaves_the_active_pointer_alone() {
        assert_eq!(active_index_after_removal(2, 3, 3), 2);
    }

    #[test]
    fn removing_the_active_workspace_lands_on_the_one_that_slid_in() {
        assert_eq!(active_index_after_removal(1, 1, 3), 1);
    }

    #[test]
    fn removing_the_active_last_workspace_falls_back_to_the_previous_one() {
        assert_eq!(active_index_after_removal(3, 3, 3), 2);
    }

    #[test]
    fn removing_the_only_workspace_yields_zero() {
        // 호출자가 곧 workspace 를 자동 재생성한다(빈 화면 invariant).
        assert_eq!(active_index_after_removal(0, 0, 0), 0);
    }

    /// **두 실행 형태 모두** close cascade 가 이 헬퍼를 지나는지 소스 수준으로 고정한다.
    ///
    /// cascade 는 두 빌드(gui / headless)가 같은 파일(`core/structural_cascade.rs`)을
    /// 컴파일하고, 빌드 형태의 차이는 그 안의 `#[cfg(feature = "gui")]` 블록으로만 있다.
    /// 그래서 이 가드가 묻는 것은 "헬퍼를 부르는가" 와 **"그 호출이 gui 블록 안에 갇히지
    /// 않았는가"** 둘이다. 뒤쪽은 호출 **직전 줄**의 속성만 본다 — `#[cfg(feature = "gui")]`
    /// 가 붙은 `{ … }` 블록 안으로 옮기는 형태는 못 잡는다. 소스 가드가 여전히 필요한
    /// 이유는 이 불변식이 headless **행동** 테스트로 원리적으로 잡히지 않기 때문이다:
    /// 오늘의 headless 는 `active_workspace` 가
    /// 0 을 벗어날 수단이 없어(레이아웃 복원 없음 · `preset.apply` 가 `focus: false` 강제 ·
    /// `debug.switch_workspace` 는 gui 게이트) 올바른 보정과 옛 범위 초과 clamp 의 **결과가
    /// 같다**. 그래서 headless 쪽만 옛 clamp 로 남아도 어떤 실행 테스트도 실패하지 않는다 —
    /// 실제로 그렇게 한 번 놓쳤고, 그때 잡은 것도 (기본 빌드에서 도는) 이 소스 가드였다.
    /// 근거 [ADR-0113](../../docs/adr/0113-close-preserves-the-focused-target.md).
    #[test]
    fn both_close_cascades_route_through_the_pointer_helper() {
        let src = include_str!("../core/structural_cascade.rs");
        let lines: Vec<&str> = src.lines().collect();
        let calls: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("state.fix_workspace_pointers_after_removal("))
            .map(|(i, _)| i)
            .collect();
        assert!(
            !calls.is_empty(),
            "close cascade 가 활성 포인터 보정 헬퍼를 부르지 않는다"
        );
        for i in calls {
            assert!(
                !lines[i - 1].contains("cfg(feature = \"gui\")"),
                "structural_cascade.rs:{} 의 포인터 보정이 gui 전용으로 갇혔다 — headless 는 \
                 보정 없이 남는다",
                i + 1
            );
        }
        assert!(
            !src.contains("state.active_workspace = engine.workspaces.len() - 1"),
            "close cascade 에 범위 초과 clamp 만 하는 옛 보정이 남아 있다"
        );
    }

    #[test]
    fn moving_a_workspace_forward_drags_a_passed_over_pointer_back() {
        // [0,1,2,3] 에서 0 을 2 로 옮기면 1·2 가 한 칸씩 당겨진다.
        assert_eq!(active_index_after_move(1, 0, 2), 0);
        assert_eq!(active_index_after_move(2, 0, 2), 1);
    }

    #[test]
    fn moving_a_workspace_backward_pushes_a_passed_over_pointer_up() {
        // [0,1,2,3] 에서 3 을 1 로 옮기면 1·2 가 한 칸씩 밀린다.
        assert_eq!(active_index_after_move(1, 3, 1), 2);
        assert_eq!(active_index_after_move(2, 3, 1), 3);
    }

    #[test]
    fn moving_the_active_workspace_takes_the_pointer_with_it() {
        assert_eq!(active_index_after_move(1, 1, 3), 3);
        assert_eq!(active_index_after_move(3, 3, 0), 0);
    }

    #[test]
    fn a_move_outside_the_pointer_leaves_it_alone() {
        // 이동 구간(0..=1)이 활성(3)을 지나가지 않는다.
        assert_eq!(active_index_after_move(3, 0, 1), 3);
        assert_eq!(active_index_after_move(0, 2, 3), 0);
    }

    /// 재정렬 경로가 **실제로** 활성 워크스페이스를 따라가는지 id 로 확인한다
    /// (인덱스만 보면 규칙이 틀려도 우연히 맞을 수 있다).
    #[test]
    fn reordering_keeps_the_active_pointer_on_the_same_workspace() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        while engine.workspaces.len() < 4 {
            crate::core::apply_create_workspace_inner(
                &mut engine,
                crate::core::WorkspaceCreationParams::terminal(),
            )
            .unwrap();
        }
        let ids: Vec<u32> = engine.workspaces.iter().map(|w| w.id).collect();

        // 사용자는 ids[2] 를 보는 중, 그 앞의 ids[0] 이 맨 뒤로 간다.
        state.active_workspace = 2;
        assert!(state.move_workspace(&mut engine, 0, 3));
        assert_eq!(
            engine.workspaces[state.active_workspace].id, ids[2],
            "앞쪽 워크스페이스가 뒤로 가면 보고 있던 것은 한 칸 당겨진다"
        );

        // 보고 있던 것 자체를 맨 앞으로 옮기면 포인터가 따라간다.
        let from = state.active_workspace;
        assert!(state.move_workspace(&mut engine, from, 0));
        assert_eq!(engine.workspaces[state.active_workspace].id, ids[2]);
        assert_eq!(state.active_workspace, 0);
    }

    /// `CoreEvent::WorkspaceMoved` cascade 도 `move_workspace` 와 **같은 규칙**을
    /// 써야 한다 — 두 경로가 갈리면 어느 쪽으로 재정렬했느냐에 따라 포커스가 달라진다.
    #[test]
    fn the_move_cascade_applies_the_same_rule_as_move_workspace() {
        for (active, from, to) in [(2, 0, 3), (1, 3, 1), (1, 1, 3), (3, 0, 1)] {
            let (mut state, _engine) = crate::state::tests::test_state();
            state.active_workspace = active;
            crate::app::dispatch_domain::cascade_workspace_moved(&mut state, from, to);
            assert_eq!(
                state.active_workspace,
                active_index_after_move(active, from, to),
                "cascade 가 헬퍼와 다른 답을 냈다 (active={active}, {from}->{to})"
            );
        }
    }

    /// 제거 축의 `both_close_cascades_route_through_the_pointer_helper` 와 같은 이유로
    /// 재정렬 축도 소스 수준으로 고정한다 — headless 는 오늘 `active_workspace` 가 0 을
    /// 벗어날 수단이 없어 **옳은 보정과 아무것도 안 하는 것의 결과가 같고**, 그래서
    /// 어떤 실행 테스트도 headless 쪽 회귀를 못 잡는다. 실제로 headless stub 은 이
    /// cascade 를 빈 함수로 두고 있었다.
    #[test]
    fn both_move_cascades_route_through_the_pointer_helper() {
        for (label, src) in [
            ("gui", include_str!("../app/dispatch_domain.rs")),
            ("headless", include_str!("../app/dispatch_domain_stubs.rs")),
        ] {
            assert!(
                src.contains("fix_workspace_pointers_after_move"),
                "{label} cascade 가 재정렬 포인터 보정 헬퍼를 부르지 않는다"
            );
            assert!(
                !src.contains("state.active_workspace = to_index"),
                "{label} cascade 에 보정 규칙이 인라인으로 복제돼 있다"
            );
        }
    }

    /// 재현: 카테고리 착지점이 **재정렬**에서 밀린다.
    ///
    /// 완충(`switch_to_category` 의 소속 재검증)이 "다른 카테고리로 튀는" 형태는
    /// 막지만, 밀린 인덱스가 **같은 카테고리의 다른 워크스페이스**를 가리키면
    /// 재검증을 통과해 그대로 착지한다 — 그게 남은 오작동이다.
    ///
    /// 배치는 **정답이 그 카테고리의 first 가 아니도록** 잡는다. first 와 겹치면
    /// 착지점이 완전히 망가져 폴백을 타도 우연히 정답이 나와 테스트가 통과한다.
    #[test]
    fn reordering_keeps_the_category_landing_on_the_same_workspace() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        while engine.workspaces.len() < 4 {
            crate::core::apply_create_workspace_inner(
                &mut engine,
                crate::core::WorkspaceCreationParams::terminal(),
            )
            .unwrap();
        }
        let cat_a = engine.create_category("A").unwrap();
        let cat_b = engine.create_category("B").unwrap();
        let ids: Vec<u32> = engine.workspaces.iter().map(|w| w.id).collect();
        for (i, cat) in [cat_a, cat_b, cat_a, cat_a].into_iter().enumerate() {
            engine.set_workspace_category(ids[i], cat).unwrap();
        }
        // categories(): [normal, A, B] → A 의 섹션 인덱스는 1.
        assert_eq!(engine.categories()[1].id, cat_a);

        // 사용자가 A 안에서 마지막으로 본 것은 ids[3], 그 다음 B 로 이동해 있다.
        state.switch_workspace(&mut engine, 3);
        state.switch_workspace(&mut engine, 1);

        // ids[0] 을 맨 뒤로 옮긴다 → [ids[1], ids[2], ids[3], ids[0]].
        assert!(state.move_workspace(&mut engine, 0, 3));
        assert_eq!(
            engine.workspaces.iter().map(|w| w.id).collect::<Vec<_>>(),
            vec![ids[1], ids[2], ids[3], ids[0]]
        );
        // 정답(ids[3])은 A 의 first(ids[2]) 가 아니다 — 폴백으로는 도달할 수 없다.
        assert_eq!(engine.workspaces_in_category(cat_a)[0].1.id, ids[2]);

        // A 로 quick-switch → 마지막으로 본 ids[3] 으로 돌아가야 한다.
        state.switch_to_category(&mut engine, 1);
        assert_eq!(
            engine.workspaces[state.active_workspace].id, ids[3],
            "재정렬 뒤에도 그 카테고리에서 마지막으로 본 워크스페이스로 착지해야 한다"
        );
    }

    /// 제거로 인한 인덱스 밀림이 **착지점을 건드리지 않는다** — 값이 id 라서
    /// 밀릴 것이 없기 때문이다. 앞쪽 워크스페이스를 지워 인덱스를 한 칸씩 당긴 뒤에도
    /// 같은 워크스페이스로 착지하는지, 그리고 제거된 워크스페이스를 가리키던 착지점은
    /// first 로 폴백하는지를 본다.
    #[test]
    fn category_landing_points_survive_a_removal_because_they_hold_ids() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        while engine.workspaces.len() < 3 {
            crate::core::apply_create_workspace_inner(
                &mut engine,
                crate::core::WorkspaceCreationParams::terminal(),
            )
            .unwrap();
        }
        let cat_a = engine.create_category("A").unwrap();
        let cat_b = engine.create_category("B").unwrap();
        let ids: Vec<u32> = engine.workspaces.iter().map(|w| w.id).collect();
        engine.set_workspace_category(ids[0], cat_b).unwrap();
        engine.set_workspace_category(ids[1], cat_a).unwrap();
        engine.set_workspace_category(ids[2], cat_a).unwrap();

        state.switch_workspace(&mut engine, 2); // A 의 착지점 = ids[2]
        state.switch_workspace(&mut engine, 0); // B 의 착지점 = ids[0]

        // ids[0] 제거 → 남은 것은 [ids[1], ids[2]], 인덱스가 한 칸씩 당겨진다.
        engine.workspaces.remove(0);
        state.fix_workspace_pointers_after_removal(0, engine.workspaces.len());

        // A: 인덱스는 2 → 1 로 밀렸지만 id 는 그대로라 같은 워크스페이스에 착지한다.
        state.switch_to_category(&mut engine, 1);
        assert_eq!(
            engine.workspaces[state.active_workspace].id, ids[2],
            "제거로 인덱스가 밀려도 같은 워크스페이스로 착지해야 한다"
        );

        // B: 착지 대상이 사라졌으므로 그 카테고리의 first 로 폴백 — 여기서는 아무것도
        // 없으니 전환 자체가 일어나지 않는다(직전 착지 그대로).
        let before = state.active_workspace;
        state.switch_to_category(&mut engine, 2);
        assert_eq!(
            state.active_workspace, before,
            "빈 카테고리로는 전환하지 않는다(stale 착지점을 그대로 쓰지도 않는다)"
        );
    }
}
