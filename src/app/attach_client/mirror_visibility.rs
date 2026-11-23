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
}
