//! workspace 하나에 작용하는 확인·입력 팝업을 여는 한 지점.
//! 강제 분리 확인, 원격 닫기 확인, 사이드바의 workspace 이름·부제 바꾸기가 여기서 연다.
//! 범위는 작용 대상 workspace다. 그 workspace가 활성일 때만 그린다.

use crate::adapters::ui::popup::PopupId;
use crate::intent::{DispatchedIntent, OpenPopupMode, UiIntent};
use crate::model::popup_kind::PopupScope;
use crate::state::MainViewState;

/// `workspace_id` 범위로 팝업 `id`를 연다. `origin`은 요청 출처를 붙인다.
pub(crate) fn open(
    state: &mut MainViewState,
    workspace_id: u32,
    id: PopupId,
    origin: impl FnOnce(UiIntent) -> DispatchedIntent,
) {
    state.dispatch_intent(origin(UiIntent::OpenPopup {
        id,
        mode: OpenPopupMode::CenteredFocused(PopupScope::Workspace(workspace_id)),
    }));
}
