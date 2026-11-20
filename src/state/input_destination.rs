//! 창이 egui 로 보내지 않은 키·IME 사건이 가는 곳. 창의 키 처리(`view::main::keyboard`)와 IME 처리
//! (`view::main::ime::handle_event`)가 이 판정 하나로 갈래를 고른다. 판정은 GPU 없이 시험이 직접 부른다.

use crate::runtime::engine_read::EngineRead;
use crate::state::FocusedSurfaceType;

/// 키·IME 사건이 가는 곳.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputDestination {
    /// 키 오버레이(팝업·무대·대화상자)가 받는다. 창은 배경으로 보내지 않는다.
    Overlay,
    /// 이 터미널 surface(PTY)가 받는다.
    Terminal(u32),
    /// 이 egui-mesh surface 의 plugin 이 받는다.
    EguiMesh(u32),
    /// 이 attach mesh mirror surface 의 원격 plugin 이 받는다.
    AttachMesh(u32),
    /// 받는 surface 가 없거나 입력을 받지 않는 surface 다. IME 조합은 이 surface 로 그린다.
    Other(Option<u32>),
}

impl super::MainViewState {
    /// `overlay_open`(키는 `window_holds_terminal_keys`, IME 는 `window_keys_to_overlay`)이 거짓일 때 지금 포커스
    /// surface 가 그 사건을 받는 갈래.
    pub(crate) fn input_destination(
        &self,
        engine: &EngineRead<'_>,
        overlay_open: bool,
    ) -> InputDestination {
        if overlay_open {
            return InputDestination::Overlay;
        }
        let sid = self.focused_surface_id(engine);
        if let (FocusedSurfaceType::Terminal, Some(sid)) = (self.focused_surface_type(engine), sid)
        {
            return InputDestination::Terminal(sid);
        }
        let surface = sid.and_then(|sid| engine.find_surface_by_id(sid).map(|s| (sid, s)));
        match surface {
            Some((sid, s)) if s.mesh().is_some() => InputDestination::EguiMesh(sid),
            Some((sid, s)) if s.attach_mesh().is_some() => InputDestination::AttachMesh(sid),
            _ => InputDestination::Other(sid),
        }
    }
}
