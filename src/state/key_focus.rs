//! host 키 포커스가 들어 있는 surface와 프레임마다 그 값을 한 번 정해 둔 칸. 칠하기(바탕·띠·커서·검색 잉크),
//! 강조 해제·soft 점유 정리, mesh에 보내는 focused 값, plugin `surface.focused`가 모두 이 판정 하나를 읽는다.
//! 선택(`focused_surface_id`)만으로는 focused가 아니다. 계약은 `docs/design/policies/focus.md`의
//! "Surface 포커스 표현" 절에 있다. 판정은 창의 포커스 노드(`focus_node`, 정규화는 `normalize_focus` 한 곳)를 surface로
//! 옮기는 대응표다. 순서를 따로 다시 판정하지 않는다. 입력용 판정(`pass_key_node`, 팝업 키 주인)은 이 모듈을 읽지 않는다.
//!
//! 한 프레임의 소비자는 그 프레임 첫머리에 채운 값(`frame_key_focus`)을 읽는다. 그리기 중 포커스가 바뀌면
//! 프레임 끝에 다음 프레임을 요청해 모든 소비자가 함께 바뀐다. GPU 제출이 성공한 프레임의 값은 따로 남겨
//! (`shown_*`) 그 프레임을 따라야 하는 바깥 소비자(WebView 바탕, plugin 이벤트)가 읽는다.

use super::focus_node::FocusNode;
use crate::core::CoreState;
use crate::runtime::engine_read::EngineRead;

/// 다시 그리기 첫머리에 꺼낸 모달 열기 요청.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModalOpenRequests {
    /// 설정 창 열기 요청이 있었다.
    pub(crate) settings: bool,
    /// plugin 창 열기 요청이 있었다.
    pub(crate) plugins: bool,
}

/// 프레임마다 정한 키 포커스 값들.
#[derive(Debug, Default)]
pub(crate) struct KeyFocusFrames {
    /// 지금 그리는 프레임의 값.
    frame: Option<u32>,
    /// 이 프레임에서 `frame`을 채웠는지. 프레임을 마칠 때 지운다.
    fresh: bool,
    /// 마지막으로 제출에 성공한 프레임의 값.
    shown: Option<u32>,
    /// 마지막으로 제출에 성공한 프레임에서 focused 바탕을 칠한 surface(키 포커스이고 내용을 숨기는 점유가 아님).
    shown_bed: Option<u32>,
    /// 제출에 성공한 프레임에서 키 포커스가 들어간 surface와 그 앞 값. plugin 이벤트 감지가 꺼낸다.
    entered: Vec<(u32, Option<u32>)>,
}

impl super::MainViewState {
    /// host 키 포커스가 들어 있는 surface. 선택 surface의 본문이나 그 surface의 주소 띠에 키가 있을 때만 값이 있다.
    /// 창의 포커스 노드(`focus_node`)와 복원 안내 안쪽 포커스(`restore_notice::has_keyboard_focus`, 키 경로가 안내로
    /// 키를 보내는 판정)를 [`key_focus_of`]로 옮긴 값이다.
    pub(crate) fn key_focus_surface(&self, core: &CoreState) -> Option<u32> {
        key_focus_of(
            self.focus_node(core),
            crate::adapters::ui::restore_notice::has_keyboard_focus(&self.popups),
            self.focused_surface_id(core),
        )
    }

    /// 그리기가 남긴 설정·plugin 모달 열기 요청을 꺼내고 지운다. 다시 그리기 첫머리(`prepare_redraw`의
    /// `dispatch_pending_modal_opens`)가 프레임 키 포커스를 채우기 전에 부른다. 그래서 그리는 중 생긴 설정 요청은
    /// 프레임 사이의 입력만 보고, 다음 프레임 첫머리 키 포커스 값에는 설정 요청 갈래([`key_focus_of`])가 나타나지
    /// 않는다.
    pub(crate) fn take_modal_open_requests(&mut self) -> ModalOpenRequests {
        ModalOpenRequests {
            settings: std::mem::take(&mut self.settings_open_requested),
            plugins: std::mem::take(&mut self.plugins_open),
        }
    }

    /// 이번 프레임의 키 포커스 값을 지금 상태로 채운다. 프레임의 첫 소비자보다 먼저 부른다.
    pub(crate) fn refresh_frame_key_focus(&mut self, core: &CoreState) {
        self.key_focus.frame = self.key_focus_surface(core);
        self.key_focus.fresh = true;
    }

    /// 이번 프레임에 아직 채우지 않았으면 채운다. 준비 단계를 건너뛰고 그리는 경로가 부른다.
    pub(crate) fn ensure_frame_key_focus(&mut self, core: &CoreState) {
        if !self.key_focus.fresh {
            self.refresh_frame_key_focus(core);
        }
    }

    /// 지금 그리는 프레임의 키 포커스 surface.
    pub(crate) fn frame_key_focus(&self) -> Option<u32> {
        self.key_focus.frame
    }

    /// 그리기를 마친다. `shown`이면 이 프레임을 화면에 냈으므로 그 값을 표시한 값으로 남기고, 키 포커스가 새로 들어간
    /// surface를 감지 큐에 넣는다.
    pub(crate) fn finish_frame_key_focus(&mut self, engine: &EngineRead<'_>, shown: bool) {
        let frames = &mut self.key_focus;
        if shown {
            let frame = frames.frame;
            if frame != frames.shown
                && let Some(sid) = frame
            {
                frames.entered.push((sid, frames.shown));
            }
            frames.shown = frame;
            frames.shown_bed = frame.filter(|sid| !engine.live.occupancy.is_content_hidden(*sid));
        }
        frames.fresh = false;
    }

    /// 그리기 중 키 포커스가 바뀌어 지금 값이 이번 프레임 값과 다른지. 참이면 다음 프레임을 그린다.
    pub(crate) fn key_focus_moved_since_frame(&self, core: &CoreState) -> bool {
        self.key_focus_surface(core) != self.key_focus.frame
    }

    /// 마지막으로 낸 프레임에서 focused 바탕을 칠한 surface.
    pub(crate) fn shown_bed_focus(&self) -> Option<u32> {
        self.key_focus.shown_bed
    }

    /// 낸 프레임들에서 키 포커스가 들어간 surface와 그 앞 값을 순서대로 꺼낸다.
    pub(crate) fn take_key_focus_entries(&mut self) -> Vec<(u32, Option<u32>)> {
        std::mem::take(&mut self.key_focus.entered)
    }

    /// 시험이 프레임 하나를 낸 것처럼 값을 채우고 마친다.
    #[cfg(test)]
    pub(crate) fn show_key_focus_frame(&mut self, engine: &EngineRead<'_>) {
        self.refresh_frame_key_focus(engine);
        self.finish_frame_key_focus(engine, true);
    }
}

/// 포커스 노드에서 host 키 포커스 surface로 가는 대응표. `notice`는 복원 안내 안쪽 컨트롤이 키를 가졌는지, `selected`는
/// 선택 surface다. 무대·키를 가진 팝업·튜토리얼 말풍선은 surface가 아니다. 설정 창 열기 요청은 다른 OS 창으로 가는
/// 중이라 선택 surface를 그대로 둔다. 안내가 키를 가지면(무대·팝업·설정 요청이 아닐 때 키 경로의 출발점) surface가
/// 아니다. 주소 띠는 그 surface가 선택일 때만 키 포커스다. 이번 몫 키의 수신자(`bound_input_key_owner`)와 사건 시점
/// 몫 값(`pass_key_node`)은 키 하나·몫 하나의 수신자라 포커스 위치가 아니므로 읽지 않는다.
pub(crate) fn key_focus_of(node: FocusNode, notice: bool, selected: Option<u32>) -> Option<u32> {
    match node {
        FocusNode::Stage | FocusNode::Popup(_) => None,
        FocusNode::SettingsRequest => selected,
        _ if notice => None,
        FocusNode::Tutorial => None,
        FocusNode::HtmlAddress(sid) => (selected == Some(sid)).then_some(sid),
        FocusNode::Surface(surface) => surface,
    }
}

/// surface에 알리는 focused 값(egui-mesh·attach mesh). 점유와 무관하게 키 포커스만 따른다.
pub(crate) fn told_focused(key_focus: Option<u32>, surface_id: u32) -> bool {
    key_focus == Some(surface_id)
}

/// 마지막으로 알린 값과 달라 알려야 하는지. 처음 알리는 surface도 알린다.
pub(crate) fn focus_changed(last_told: Option<bool>, told: bool) -> bool {
    last_told != Some(told)
}

#[cfg(test)]
#[path = "key_focus_tests.rs"]
mod tests;
