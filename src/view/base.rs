use std::sync::Arc;

use crate::gpu::GpuState;

/// Per-View OS/GPU resource owner, composed with common display values.
pub struct ViewBase {
    pub gpu: GpuState,
    pub winit: Arc<winit::window::Window>,
    pub(crate) state: crate::view::state::ViewState,
}

impl ViewBase {
    pub fn new(gpu: GpuState, winit: Arc<winit::window::Window>) -> Self {
        Self {
            gpu,
            winit,
            state: Default::default(),
        }
    }

    /// 사용자 입력에 따른 redraw 요청. 창 View를 빌리지 않고 이 상태만 가진 경로가 쓴다.
    pub fn mark_dirty(&mut self) {
        self.mark_dirty_from(crate::view::RepaintSource::Interactive);
    }

    /// 요청 원인에 따라 redraw를 미루되 dirty는 즉시 설정한다.
    /// 미룬 시각은 about_to_wait에서 WaitUntil로 예약한다.
    pub fn mark_dirty_from(&mut self, source: crate::view::RepaintSource) {
        self.state.dirty = true;
        self.admit_redraw(source, None, std::time::Instant::now());
    }

    /// 보이는 surface의 출력을 반영한 뒤 부른다. `block`은 출력 처리 뒤의 DEC 2026 블록 세대다.
    pub(crate) fn mark_block_output(&mut self, surface_id: u32, block: Option<u64>) {
        if let Some(block) = block {
            self.state.sync_hold.enter_block(surface_id, block);
        }
        self.mark_surface_output(surface_id, block.is_some());
    }

    /// 보이는 surface의 출력을 반영한 뒤 부른다. DEC 2026 블록 안이면 dirty만 세우고
    /// redraw는 블록 종료나 보류 마감에서 요청한다.
    pub(crate) fn mark_surface_output(&mut self, surface_id: u32, synchronized: bool) {
        self.state.dirty = true;
        let now = std::time::Instant::now();
        if !self
            .state
            .sync_hold
            .hold_output(surface_id, synchronized, now)
        {
            self.admit_redraw(
                crate::view::RepaintSource::TerminalOutput,
                Some(surface_id),
                now,
            );
        }
    }

    fn admit_redraw(
        &mut self,
        source: crate::view::RepaintSource,
        surface_id: Option<u32>,
        now: std::time::Instant,
    ) {
        if self.state.repaint.admit(source, now, &self.winit) {
            self.winit.request_redraw();
        } else {
            self.state.sync_hold.note_deferred(surface_id);
        }
    }

    /// 미룬 redraw와 동기화 보류 마감을 처리하고 다음으로 깨울 시각을 돌려준다.
    pub(crate) fn drive_deferred(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        for sid in self.state.sync_hold.take_expired(now) {
            self.state.dirty = true;
            self.admit_redraw(crate::view::RepaintSource::TerminalOutput, Some(sid), now);
        }
        if self.state.repaint.take_due(now) && self.state.sync_hold.admit_due() {
            self.winit.request_redraw();
        }
        match (
            self.state.repaint.deferred_deadline(),
            self.state.sync_hold.next_deadline(),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Main surface attempts consume dirty without recording a successful present.
    pub(crate) fn begin_surface_frame(&mut self) {
        self.state.dirty = false;
    }

    pub(crate) fn surface_presented(&mut self) {
        tracing::debug!(target: "tasty::gpu::surface", phase = "main_present", window_id = u64::from(self.winit.id()));
        self.state.repaint.note_present(std::time::Instant::now());
        self.state.sync_hold.note_present();
    }

    pub(crate) fn surface_failed(&mut self, error: &wgpu::SurfaceError) {
        self.state.dirty = true;
        self.state.sync_hold.note_deferred(None);
        if self.state.repaint.surface_failed(std::time::Instant::now()) {
            let message = format!("surface error: {error}; delayed retry scheduled");
            tracing::warn!("{message}");
            crate::crash_report::record_error(&message);
        }
    }

    pub(crate) fn surface_ready(&self) -> bool {
        self.state.repaint.surface_ready(std::time::Instant::now())
    }

    /// 프레임을 그릴 때 dirty 해제와 다시 그리기 시간 기준 갱신을 함께 처리한다.
    pub fn begin_frame(&mut self) {
        self.state.dirty = false;
        self.state.repaint.note_present(std::time::Instant::now());
        self.state.sync_hold.note_present();
    }
}
