//! mirror가 지금 화면에 그려지는지. 숨은 mirror는 크기 요청을 보내거나 재전송하지 않는다
//! (docs/design/policies/render-on-draw.md). 판정은 보낼 때의 창 상태를 직접 읽는다.

use crate::app::App;
use std::collections::HashSet;

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
