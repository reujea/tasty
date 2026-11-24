//! terminal.tell·broadcast·respawn 실행이 빌리는 능력만 모은 대여.
//! 읽기는 구조·점유·터미널 세대, 쓰기는 터미널 입력·제출 예약·child 등록 상태로 한정한다.
//! hooks·task 범위·observer·remote·persistence는 여기서 닿지 않는다.
use super::child_terminal::{ChildTerminalRegistry, ParentKey};
use super::pending_submit::PendingSubmit;
use super::terminal_store::TerminalStore;
use crate::core::CoreState;
use crate::core::attach::OccupancyRegistry;
use tasty_terminal::{ResourceGeneration, Waker};

pub(crate) struct TerminalInputAccess<'a> {
    pub(super) core: &'a CoreState,
    pub(super) occupancy: &'a OccupancyRegistry,
    pub(super) terminals: &'a mut TerminalStore,
    pub(super) pending_submits: &'a mut Vec<PendingSubmit>,
    pub(super) waker: &'a Waker,
    pub(super) children: &'a mut ChildTerminalRegistry,
}

impl TerminalInputAccess<'_> {
    /// 원격 holder 가 점유한 터미널인지. 그 holder 출처의 자식을 만든 plugin 이 처리 중인 입력은 막지 않는다.
    pub(crate) fn is_hard_occupied(&self, surface: u32) -> bool {
        self.occupancy.is_hard_occupied(surface)
            && !crate::remote::agent_children::may_drive(self.occupancy, self.children, surface)
    }

    pub(crate) fn matches_generation(&self, surface: u32, generation: ResourceGeneration) -> bool {
        self.terminals.matches_generation(surface, generation)
    }

    pub(crate) fn matches_mirror_projection(
        &self,
        workspace: u32,
        token: &std::sync::Weak<()>,
    ) -> bool {
        self.core.matches_mirror_projection(workspace, token)
    }

    /// 바이트를 그대로 바로 쓴다. 점유·세대 검사는 호출자가 먼저 한다. 터미널이 없으면 false다.
    pub(crate) fn write_bytes(&mut self, surface: u32, bytes: &[u8]) -> bool {
        self.terminals
            .get_mut(surface)
            .map(|terminal| terminal.send_bytes(bytes))
            .is_some()
    }

    /// 텍스트를 UTF-8 바이트 그대로 바로 쓴다. 점유 검사는 호출자가 먼저 한다. 터미널이 없으면 false다.
    pub(crate) fn write_text(&mut self, surface: u32, text: &str) -> bool {
        self.terminals
            .get_mut(surface)
            .map(|terminal| terminal.send_key(text))
            .is_some()
    }

    /// 등록된 child의 현재 surface. 등록이 없으면 None이다.
    pub(crate) fn child_surface(&self, parent: &ParentKey, index: u32) -> Option<u32> {
        self.children
            .find_child(parent, index)
            .map(|entry| entry.child_surface_id)
    }

    /// 트리에 없는 child 등록을 정리하고, 바뀌었으면 저장한다.
    pub(crate) fn reconcile_children(&mut self) {
        let live = self.core.live_surface_ids();
        if self.children.reconcile_with_live_surfaces(&live).changed() {
            self.children.save();
        }
    }

    /// 입력을 받은 등록 child의 idle/needs_input을 해제한다. 바뀐 항목이 있으면 한 번만 저장한다.
    pub(crate) fn mark_prompted(&mut self, surfaces: impl IntoIterator<Item = u32>) {
        let mut changed = false;
        for surface in surfaces {
            changed |= self.children.clear_idle_for_new_prompt(surface);
        }
        if changed {
            self.children.save();
        }
    }

    /// respawn이 지정한 role을 반영하고 child를 active로 돌린 뒤 저장한다. 지정한 nickname은 조회 값에 먼저
    /// 넣지 않고 set_nickname과 같은 명시 대입으로 저장한다. role·상태는 그 저장의 필드 차이로 함께 간다.
    /// 조회 값의 nickname은 쓰기가 성공한 뒤에 들어오고, 같은 자식의 nickname 요청은 접수 순서대로 쓰인다.
    pub(crate) fn record_respawn(
        &mut self,
        parent: &ParentKey,
        index: u32,
        surface: u32,
        role: Option<&str>,
        nickname: Option<&str>,
    ) {
        if let Some(role) = role {
            self.children.update_child(parent, index, |entry| {
                entry.role = Some(role.to_owned());
            });
        }
        self.children.set_idle(surface, false);
        match nickname {
            Some(nickname) => {
                if let Err(refused) =
                    self.children
                        .set_nickname(parent, index, surface, Some(nickname.to_owned()))
                {
                    tracing::warn!(?refused, "child respawn nickname not admitted");
                }
            }
            None => self.children.save(),
        }
    }
}
