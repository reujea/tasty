//! mirror가 지금 화면에 그려지는지. 숨은 mirror는 크기 요청을 보내거나 재전송하지 않는다
//! (docs/design/policies/render-on-draw.md). 판정은 보낼 때의 창 상태를 직접 읽는다.

use crate::app::App;
use crate::runtime::terminal_store::TerminalStore;
use std::collections::{HashMap, HashSet};

impl App {
    /// 지금 그려지는 mirror의 로컬 surface ID. 창이 최소화·숨김이거나 무대가 덮으면 그 창의 mirror는
    /// 숨은 것이다. parked engine에는 창이 없으므로 그 mirror도 숨은 것이다.
    pub(crate) fn drawn_mirror_surfaces(&self) -> HashSet<u32> {
        let mut drawn = HashSet::new();
        let locals: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .flat_map(|sess| sess.state.remote_to_local.values().copied())
            .collect();
        if locals.is_empty() {
            return drawn;
        }
        for (_, main, engine) in self.engines().window_pairs() {
            if crate::gfx::gpu::edit_keys::frames_stopped(&main.base.winit)
                || main.state.fullscreen_stage_active()
            {
                continue;
            }
            drawn.extend(
                locals
                    .iter()
                    .copied()
                    .filter(|&sid| main.state.draws_surface(engine.core, sid)),
            );
        }
        drawn
    }

    /// 마감 처리가 볼 mirror 상태: 지금 그려지는 mirror와 각 mirror의 최신 레이아웃 목표.
    pub(super) fn mirror_view(&self) -> MirrorView {
        let drawn = self.drawn_mirror_surfaces();
        // 그려지는 mirror는 창이 있는 engine에만 있다.
        let stores = self
            .engines()
            .window_pairs()
            .map(|(_, _, engine)| &engine.runtime.terminals);
        MirrorView::read(drawn, stores)
    }

    /// 보낼 resize 요청을 큐에서 꺼낸다. 큐에 넣은 뒤 같은 프레임에서 숨겨진 mirror와
    /// grid가 이미 목표인 항목은 버린다. 다시 보이면 레이아웃 스윕이 큐에 다시 넣는다.
    pub(super) fn take_resize_forwards(&mut self) -> Vec<(u32, usize, usize)> {
        let drawn = self.drawn_mirror_surfaces();
        let mut pending = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            for (sid, (cols, rows)) in engine.remote.pending_resize_forward.drain() {
                let settled = engine
                    .runtime
                    .terminals
                    .get(sid)
                    .is_some_and(|t| t.dimensions() == (cols, rows));
                if drawn.contains(&sid) && !settled {
                    pending.push((sid, cols, rows));
                }
            }
        }
        pending
    }

    /// 숨김·최소화 요청은 다음 창 이벤트보다 먼저 레이아웃 스윕을 만날 수 있다. 숨긴 것으로 둔다.
    pub(crate) fn mark_main_windows_hidden(&mut self) {
        for view in self.view.views.values_mut() {
            if let Some(main) = view.as_main_mut() {
                main.state.window_drawn = false;
            }
        }
    }
}

/// 마감 처리 시점의 mirror 상태. [`App::mirror_view`]가 만든다.
pub(super) struct MirrorView {
    drawn: HashSet<u32>,
    targets: HashMap<u32, (usize, usize)>,
}

impl MirrorView {
    /// 그려지는 mirror마다 터미널 store가 기록한 최신 레이아웃 목표를 읽는다.
    fn read<'a>(drawn: HashSet<u32>, stores: impl Iterator<Item = &'a TerminalStore>) -> Self {
        let mut targets = HashMap::new();
        for store in stores {
            for &sid in &drawn {
                if let Some(target) = store.layout_target(sid) {
                    targets.insert(sid, target);
                }
            }
        }
        MirrorView { drawn, targets }
    }

    /// 이 요청의 mirror가 숨었는가(모르는 surface 포함). 숨은 mirror의 마감 요청은 시도 종류와 관계없이 버린다.
    pub(super) fn hides(&self, local: Option<u32>) -> bool {
        local.is_none_or(|local| !self.drawn.contains(&local))
    }

    /// 그려지는 mirror의 최신 레이아웃 목표가 이 요청과 다른가. 자동 재전송만 이 조건으로 버린다.
    /// 목표가 바뀐 요청을 다시 보내면 이미 맞춘 grid를 옛 크기로 되돌린다.
    pub(super) fn retargets(&self, local: Option<u32>, target: (usize, usize)) -> bool {
        local
            .and_then(|local| self.targets.get(&local))
            .is_some_and(|t| *t != target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::terminal_store::PTY_ID_BASE;
    use std::sync::{Arc, atomic::AtomicU32};
    use tasty_terminal::Terminal;

    /// Given grid A인 mirror가 B를 요청한 뒤 숨었다가 목표 A로 다시 보일 때, When B의 마감이 오면,
    /// Then 목표가 바뀐 요청이다. 목표와 같은 요청과 목표 기록이 없는 요청은 아니고, 숨은 mirror와 모르는
    /// surface는 숨은 것이다.
    #[test]
    fn a_due_request_is_resent_only_for_a_drawn_mirror_whose_target_still_matches() {
        let view = MirrorView {
            drawn: HashSet::from([1, 2]),
            targets: HashMap::from([(1, (80, 24))]),
        };
        assert!(view.retargets(Some(1), (100, 30)), "목표가 바뀐 요청");
        assert!(!view.retargets(Some(1), (80, 24)));
        assert!(!view.retargets(Some(2), (100, 30)), "목표 기록 없음");
        assert!(!view.hides(Some(1)));
        assert!(view.hides(Some(3)), "숨은 mirror");
        assert!(view.hides(None));
    }

    /// Given 레이아웃 스윕이 mirror의 목표를 B로 기록한 store, When 마감 처리가 볼 상태를 읽으면, Then 그
    /// mirror의 A 요청은 목표가 바뀐 요청이고 B 요청은 아니다. 숨은 mirror의 목표는 읽지 않는다.
    #[test]
    fn the_view_reads_the_layout_target_recorded_by_the_store() {
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        store.insert(1, Terminal::new_detached(80, 24), None);
        store.insert(2, Terminal::new_detached(80, 24), None);
        store.apply_layout_size(1, 100, 30, true);
        store.apply_layout_size(2, 100, 30, false);
        let view = MirrorView::read(HashSet::from([1]), std::iter::once(&store));
        assert!(view.retargets(Some(1), (80, 24)));
        assert!(!view.retargets(Some(1), (100, 30)));
        assert!(!view.targets.contains_key(&2));
    }
}
