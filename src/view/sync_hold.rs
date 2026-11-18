//! DEC 2026 동기화 출력 블록 안의 PTY 출력이 redraw를 요청하지 않게 보류한다.
//!
//! 보류는 그 surface 출력이 내는 redraw 요청만 막는다. dirty는 그대로 세워 두므로
//! 사용자 입력·다른 surface 출력·egui가 연 프레임은 막지 않는다. 그 프레임은 grid를
//! 직접 읽으므로 블록 중간 상태가 보일 수 있다. 정책은 `docs/dev-guide/gpu-rendering.md`의
//! "동기화 출력 보류" 절에 있다.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// 블록 종료(`CSI ? 2026 l`)가 오지 않아도 이 시간이 지나면 그 시점 grid를 그린다.
/// 앱이 블록 중에 죽거나 멈춰도 화면이 갱신되게 하는 안전 상한이다.
pub(crate) const SYNC_HOLD_LIMIT: Duration = Duration::from_millis(150);

/// 창 하나의 동기화 보류 상태. 상한 예약은 `RepaintGate`의 지연 슬롯과 따로 둔다.
/// 그 슬롯은 아무 present에서나 비워지므로 같이 두면 다른 프레임 하나로 마감 깨움이 사라진다.
#[derive(Default)]
pub(crate) struct SyncHold {
    /// 보류 중인 surface와 마감 시각. 마감은 블록에 처음 들어간 출력에서 정한다.
    held: HashMap<u32, Instant>,
    /// 마감이 지나 강제로 그린 surface. 블록이 끝날 때까지 다시 보류하지 않는다.
    expired: HashSet<u32>,
    /// surface마다 마지막으로 본 블록 세대. 출력 묶음 하나가 블록을 끝내고 다음 블록을
    /// 시작하면 모드는 계속 켜진 것으로 읽히므로 세대로 블록을 구별한다.
    blocks: HashMap<u32, u64>,
    /// `RepaintGate`가 미룬 요청 중 surface 출력이 낸 것.
    deferred_output: HashSet<u32>,
    /// `RepaintGate`가 미룬 요청 중 surface 출력이 아닌 원인이 낸 것이 있는가.
    deferred_other: bool,
}

impl SyncHold {
    /// 출력을 처리한 뒤 본 블록 세대를 기록한다. 세대가 바뀌었으면 이전 블록의 보류와
    /// 만료를 버려, 새 블록이 다시 보류되고 마감도 새로 정해지게 한다.
    pub(crate) fn enter_block(&mut self, surface_id: u32, block: u64) {
        if self.blocks.insert(surface_id, block) != Some(block) {
            self.held.remove(&surface_id);
            self.expired.remove(&surface_id);
        }
    }

    /// 처리를 마친 surface 출력의 redraw 요청을 보류할지 정한다. 블록이 끝난 상태면 보류를 푼다.
    pub(crate) fn hold_output(
        &mut self,
        surface_id: u32,
        synchronized: bool,
        now: Instant,
    ) -> bool {
        if !synchronized {
            self.forget(surface_id);
            return false;
        }
        if self.expired.contains(&surface_id) {
            return false;
        }
        self.held.entry(surface_id).or_insert(now + SYNC_HOLD_LIMIT);
        true
    }

    /// `RepaintGate`가 요청을 미뤘다. `surface_id`가 없으면 출력이 아닌 원인이다.
    pub(crate) fn note_deferred(&mut self, surface_id: Option<u32>) {
        match surface_id {
            Some(sid) => {
                self.deferred_output.insert(sid);
            }
            None => self.deferred_other = true,
        }
    }

    /// present가 미뤘던 요청을 모두 소진했다.
    pub(crate) fn note_present(&mut self) {
        self.deferred_output.clear();
        self.deferred_other = false;
    }

    /// 만기된 지연 요청을 그릴지 정하고 소유 기록을 비운다. 미룬 요청이 모두 지금
    /// 보류 중인 surface의 출력이면 그리지 않는다. 그 surface는 블록 종료나 마감에서 다시 요청한다.
    pub(crate) fn admit_due(&mut self) -> bool {
        let only_held = !self.deferred_other
            && !self.deferred_output.is_empty()
            && self
                .deferred_output
                .iter()
                .all(|sid| self.held.contains_key(sid));
        self.note_present();
        !only_held
    }

    /// 마감이 지난 보류를 꺼낸다. 꺼낸 surface는 블록이 끝날 때까지 보류하지 않는다.
    pub(crate) fn take_expired(&mut self, now: Instant) -> Vec<u32> {
        let due: Vec<u32> = self
            .held
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(sid, _)| *sid)
            .collect();
        for sid in &due {
            self.held.remove(sid);
            self.expired.insert(*sid);
        }
        due
    }

    /// 가장 이른 보류 마감. about_to_wait에서 WaitUntil로 예약한다.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.held.values().min().copied()
    }

    /// surface의 블록 기록을 지운다. 다음 블록 출력은 처음부터 다시 보류한다.
    fn forget(&mut self, surface_id: u32) {
        self.held.remove(&surface_id);
        self.expired.remove(&surface_id);
        self.blocks.remove(&surface_id);
    }

    /// `keep`이 거짓인 surface의 블록 기록을 지운다.
    pub(crate) fn retain_surfaces(&mut self, mut keep: impl FnMut(u32) -> bool) {
        let tracked: HashSet<u32> = self
            .held
            .keys()
            .chain(&self.expired)
            .chain(self.blocks.keys())
            .copied()
            .collect();
        for sid in tracked {
            if !keep(sid) {
                self.forget(sid);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.held.is_empty() && self.expired.is_empty() && self.blocks.is_empty()
    }
}

impl crate::view::main::MainView {
    /// 보이지 않거나 없어진 surface의 블록 기록을 지운다. 보류 마감을 처리하기 전에 불러,
    /// 숨은 surface의 마감이 이 창에 dirty와 redraw를 만들지 않게 한다. 그 surface는 다시 보일 때
    /// 전환 프레임이 그리고, 그 뒤 블록 출력은 처음부터 다시 보류한다.
    pub(crate) fn forget_hidden_sync_holds(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        if self.base.state.sync_hold.is_empty() {
            return;
        }
        let mut hold = std::mem::take(&mut self.base.state.sync_hold);
        hold.retain_surfaces(|sid| self.is_surface_visible(engine, sid));
        self.base.state.sync_hold = hold;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_inside_a_block_is_held_and_released_by_the_end() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        assert!(hold.hold_output(7, true, t0));
        assert_eq!(hold.next_deadline(), Some(t0 + SYNC_HOLD_LIMIT));
        // 이어지는 블록 안 출력은 마감을 늦추지 않는다.
        assert!(hold.hold_output(7, true, t0 + Duration::from_millis(50)));
        assert_eq!(hold.next_deadline(), Some(t0 + SYNC_HOLD_LIMIT));
        assert!(!hold.hold_output(7, false, t0 + Duration::from_millis(60)));
        assert_eq!(hold.next_deadline(), None);
    }

    #[test]
    fn output_outside_a_block_is_not_held() {
        let mut hold = SyncHold::default();
        assert!(!hold.hold_output(1, false, Instant::now()));
        assert_eq!(hold.next_deadline(), None);
    }

    #[test]
    fn expired_hold_draws_and_stays_released_until_the_block_ends() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        assert!(hold.hold_output(3, true, t0));
        assert!(
            hold.take_expired(t0 + Duration::from_millis(149))
                .is_empty()
        );
        assert_eq!(hold.take_expired(t0 + SYNC_HOLD_LIMIT), vec![3]);
        assert_eq!(hold.next_deadline(), None);
        // 같은 블록이 계속되어도 다시 보류하지 않는다.
        assert!(!hold.hold_output(3, true, t0 + Duration::from_millis(200)));
        // 블록이 끝난 뒤 새 블록은 다시 보류한다.
        assert!(!hold.hold_output(3, false, t0 + Duration::from_millis(210)));
        assert!(hold.hold_output(3, true, t0 + Duration::from_millis(220)));
    }

    #[test]
    fn a_block_begun_in_the_batch_that_ended_the_last_is_held_with_its_own_deadline() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        hold.enter_block(7, 1);
        assert!(hold.hold_output(7, true, t0));
        // 같은 묶음에서 블록 1이 끝나고 블록 2가 시작됐다. 모드는 켜진 채로 읽힌다.
        let t1 = t0 + Duration::from_millis(100);
        hold.enter_block(7, 2);
        assert!(hold.hold_output(7, true, t1));
        assert_eq!(hold.next_deadline(), Some(t1 + SYNC_HOLD_LIMIT));
    }

    #[test]
    fn a_new_block_after_an_expired_one_is_held_again() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        hold.enter_block(7, 1);
        assert!(hold.hold_output(7, true, t0));
        assert_eq!(hold.take_expired(t0 + SYNC_HOLD_LIMIT), vec![7]);
        // 만료된 블록이 계속되는 동안은 보류하지 않는다.
        hold.enter_block(7, 1);
        assert!(!hold.hold_output(7, true, t0 + Duration::from_millis(160)));
        // 같은 묶음 안의 종료·시작으로 블록 2가 왔다.
        let t1 = t0 + Duration::from_millis(200);
        hold.enter_block(7, 2);
        assert!(hold.hold_output(7, true, t1));
        assert_eq!(hold.next_deadline(), Some(t1 + SYNC_HOLD_LIMIT));
    }

    #[test]
    fn a_hidden_surface_deadline_draws_nothing_and_restarts_when_shown() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        hold.enter_block(5, 1);
        assert!(hold.hold_output(5, true, t0));
        hold.enter_block(6, 1);
        assert!(hold.hold_output(6, true, t0));
        // 탭 전환으로 5가 숨었다. 마감을 처리하기 전에 숨은 surface의 기록을 지운다.
        hold.retain_surfaces(|sid| sid == 6);
        assert_eq!(hold.take_expired(t0 + SYNC_HOLD_LIMIT), vec![6]);
        // 5만 남았다면 마감이 없으므로 창을 깨우지도 다시 그리지도 않는다.
        hold.retain_surfaces(|sid| sid == 5);
        assert!(hold.is_empty());
        assert_eq!(hold.next_deadline(), None);
        // 같은 블록이 계속된 채 다시 보이면 처음부터 보류한다.
        let t1 = t0 + Duration::from_millis(300);
        hold.enter_block(5, 1);
        assert!(hold.hold_output(5, true, t1));
        assert_eq!(hold.next_deadline(), Some(t1 + SYNC_HOLD_LIMIT));
    }

    #[test]
    fn due_deferral_owned_only_by_held_output_is_skipped() {
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        hold.note_deferred(Some(5));
        assert!(hold.hold_output(5, true, t0));
        assert!(!hold.admit_due());
        // 기록은 비워졌으므로 다음 만기는 그린다.
        assert!(hold.admit_due());
    }

    #[test]
    fn due_deferral_with_another_owner_is_drawn() {
        let t0 = Instant::now();
        let mut other_surface = SyncHold::default();
        other_surface.note_deferred(Some(5));
        other_surface.note_deferred(Some(6));
        assert!(other_surface.hold_output(5, true, t0));
        assert!(other_surface.admit_due());

        let mut other_cause = SyncHold::default();
        other_cause.note_deferred(Some(5));
        other_cause.note_deferred(None);
        assert!(other_cause.hold_output(5, true, t0));
        assert!(other_cause.admit_due());
    }

    #[test]
    fn present_clears_deferral_owners() {
        let mut hold = SyncHold::default();
        hold.note_deferred(None);
        hold.note_present();
        hold.note_deferred(Some(2));
        assert!(hold.hold_output(2, true, Instant::now()));
        assert!(!hold.admit_due());
    }
}
