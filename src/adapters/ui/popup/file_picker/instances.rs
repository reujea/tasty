//! 열린 파일 피커 인스턴스와 닫힌 피커의 결과. 피커 데이터는 인스턴스에 있고 범위마다 하나씩 열린다.

use super::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::{ClosedPopup, PopupInstanceId, PopupPayload};
use crate::model::popup_kind::PopupScope;
use crate::state::{FilePickerData, FilePickerResult, FinishedFilePicker, MainViewState};

pub(crate) fn picker_of(payload: &PopupPayload) -> Option<&FilePickerData> {
    match payload {
        PopupPayload::FilePicker(p) => Some(p),
        _ => None,
    }
}

pub(crate) fn picker_of_mut(payload: &mut PopupPayload) -> Option<&mut FilePickerData> {
    match payload {
        PopupPayload::FilePicker(p) => Some(p),
        _ => None,
    }
}

/// 열린 피커들의 인스턴스 id와 데이터.
pub(crate) fn open_pickers(
    state: &MainViewState,
) -> impl Iterator<Item = (PopupInstanceId, &FilePickerData)> {
    state
        .popups
        .instances_of(FILE_PICKER_POPUP_ID)
        .filter_map(|p| picker_of(&p.payload).map(|data| (p.instance, data)))
}

/// 열린 피커들의 데이터.
pub(crate) fn pickers_mut(state: &mut MainViewState) -> impl Iterator<Item = &mut FilePickerData> {
    state
        .popups
        .entries_of_mut(FILE_PICKER_POPUP_ID)
        .filter(|p| p.open)
        .filter_map(|p| picker_of_mut(&mut p.payload))
}

/// 목록 요청 `request_id` 의 응답을 기다리는 피커. 응답은 그 요청을 보낸 인스턴스에만 반영한다.
pub(crate) fn waiting_for_listing(
    state: &mut MainViewState,
    request_id: u64,
) -> Option<&mut FilePickerData> {
    pickers_mut(state).find(|picker| {
        matches!(
            picker.load,
            crate::state::FpLoadState::Loading { request_id: rid, .. } if rid == request_id
        )
    })
}

/// `scope` 에 열기를 기다리는 피커 요청이 큐에 있는지. 열기 요청은 프레임의 Intent 처리에서 반영된다.
fn open_is_queued(state: &MainViewState, scope: &PopupScope) -> bool {
    state.pending_intents.iter().any(|intent| {
        matches!(
            &intent.body,
            crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(request))
                if request.id == FILE_PICKER_POPUP_ID && request.scope == *scope
        )
    })
}

/// `scope` 에 피커가 있는지. 열린 피커, 열기를 기다리는 요청, 닫혔지만 결과를 아직 넘기지 않은 피커를 센다.
pub(crate) fn has_picker_in(state: &MainViewState, scope: &PopupScope) -> bool {
    state
        .popups
        .instance_for(FILE_PICKER_POPUP_ID, scope)
        .is_some()
        || open_is_queued(state, scope)
        || state.popups.is_closing_in(FILE_PICKER_POPUP_ID, scope)
        || state
            .dialogs
            .finished_file_pickers
            .iter()
            .any(|f| f.scope == *scope)
}

/// 닫힌 피커의 결과를 출력함에 넣는다. 결과 없이 닫혔으면 취소다. App이 프레임 끝에 호출자에게 넘긴다.
pub(crate) fn finish(state: &mut MainViewState, closed: &ClosedPopup) {
    let Some(data) = picker_of(&closed.payload) else {
        return;
    };
    state
        .dialogs
        .finished_file_pickers
        .push(FinishedFilePicker {
            scope: closed.scope.clone(),
            request_id: data.request_id,
            result: data.result.clone().unwrap_or(FilePickerResult::Cancelled),
            requester: data.requester.clone(),
        });
}

/// 시험용: 쌓인 피커 열기 요청을 반영한다. 다른 요청은 큐에 남긴다.
#[cfg(test)]
pub(crate) fn apply_opens_for_test(state: &mut MainViewState) {
    for intent in state.take_pending_intents() {
        match &intent.body {
            crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(request))
                if request.id == FILE_PICKER_POPUP_ID =>
            {
                crate::intent::popup::open_for_test(state, request, intent.origin.is_user());
            }
            _ => state.dispatch_intent(intent),
        }
    }
}

/// 시험용: 쌓인 열기 요청을 반영하고 맨 위 피커의 데이터를 돌려준다.
#[cfg(test)]
pub(crate) fn current(state: &mut MainViewState) -> Option<&FilePickerData> {
    apply_opens_for_test(state);
    open_pickers(state).last().map(|(_, data)| data)
}

/// 시험용: 큐에 쌓인 피커 열기 요청의 `request_id`. 요청을 반영하지 않는다.
#[cfg(test)]
pub(crate) fn queued_request_id(
    state: &MainViewState,
) -> Option<crate::state::FilePickerRequestId> {
    state
        .pending_intents
        .iter()
        .find_map(|intent| match &intent.body {
            crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(request))
                if request.id == FILE_PICKER_POPUP_ID =>
            {
                request
                    .payload
                    .peek(|payload| picker_of(payload).map(|data| data.request_id))
            }
            _ => None,
        })
}

/// 시험용: 맨 위에 열린 피커의 데이터. 쌓인 열기 요청은 반영하지 않는다.
#[cfg(test)]
pub(crate) fn latest(state: &MainViewState) -> Option<&FilePickerData> {
    open_pickers(state).last().map(|(_, data)| data)
}

/// 시험용: 쌓인 열기 요청을 반영하고 맨 위 피커의 데이터를 고칠 수 있게 돌려준다.
#[cfg(test)]
pub(crate) fn current_mut(state: &mut MainViewState) -> Option<&mut FilePickerData> {
    apply_opens_for_test(state);
    pickers_mut(state).last()
}

/// 시험용: `data` 로 피커를 `scope` 에 바로 연다.
#[cfg(test)]
pub(crate) fn install_for_test(state: &mut MainViewState, scope: PopupScope, data: FilePickerData) {
    crate::intent::popup::open_for_test(
        state,
        &crate::intent::OpenRequest::new(FILE_PICKER_POPUP_ID, scope)
            .centered()
            .focused()
            .with_payload(PopupPayload::FilePicker(Box::new(data))),
        true,
    );
}

/// 시험용: 맨 위 피커의 데이터를 잠시 꺼내 동작 하나를 반영하고 돌려놓는다. 닫기 요청은 돌려줄 뿐 닫지 않는다.
#[cfg(test)]
pub(crate) fn apply_to_current(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: super::FilePickerAction,
) -> crate::adapters::ui::popup::PopupAction {
    apply_opens_for_test(state);
    let iid = open_pickers(state)
        .last()
        .map(|(iid, _)| iid)
        .expect("open picker");
    let mut payload =
        std::mem::take(&mut state.popups.instance_mut(iid).expect("instance").payload);
    let data = picker_of_mut(&mut payload).expect("picker data");
    let action = super::apply_action(data, state, engine, action);
    state.popups.instance_mut(iid).expect("instance").payload = payload;
    action
}
