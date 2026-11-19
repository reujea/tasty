//! 팝업 인스턴스가 가진 데이터와 열기 요청이 그 데이터를 실어 오는 칸.

use tasty_type_geometry::length::LogicalPx;

/// 인스턴스가 가진 팝업 데이터. 닫으면 함께 사라진다.
///
/// 아직 옮기지 않은 팝업의 데이터는 `DialogState`·`MainViewState`에 있다. 그 팝업의 인스턴스 값은 `None`이다.
#[derive(Default)]
pub enum PopupPayload {
    #[default]
    None,
    /// 변환 팝업의 대상 surface와 키보드 선택.
    Convert(super::convert::ConvertPayload),
    /// 검색창이 검색하는 터미널 surface.
    Search { surface_id: u32 },
    /// 마우스 캡처 배너 더보기 메뉴의 대상과 열 때 잰 크기.
    MouseCaptureMenu(crate::adapters::ui::mouse_capture_menu::MenuPayload),
    /// 파일 작업 충돌 질문을 띄운 탐색기 칸.
    ExplorerConflict { surface_id: u32 },
    /// 이름 바꾸기의 대상·입력·원격 대기.
    Rename(Box<crate::adapters::ui::rename_popup::RenamePayload>),
    /// 탐색기 Properties가 연 항목과 진행 중인 읽기.
    ExplorerProperties(Box<super::explorer_properties::ExplorerProperties>),
    /// 파일 피커의 탐색·선택 상태와 결과를 받을 요청.
    FilePicker(Box<crate::state::FilePickerData>),
    /// 파일 처리기 선택의 대상·후보·고른 결과와 기다리는 요청.
    HandlerPicker(Box<crate::state::FileHandlerPickerData>),
    /// 삭제를 확인할 카테고리.
    ConfirmDeleteCategory(crate::model::WorkspaceCategoryId),
    /// 점유를 강제로 풀지 확인할 workspace.
    ConfirmForceDetach(crate::model::WorkspaceId),
    /// 원격에서 닫기 전 확인을 기다리는 닫기 요청.
    ConfirmCloseRemote(Box<super::confirm_close_remote::PendingCloseRemote>),
    /// 도구 메뉴 셸 안쪽 폭. 열 때와 그릴 때 가장 넓은 라벨로 잰다.
    ToolsMenu { inner_width: LogicalPx },
    /// 축소 사이드바 카테고리 팝업의 대상.
    RailCategory(crate::model::WorkspaceCategoryId),
    /// 변경된 Lua 스크립트 실행 확인.
    ScriptConfirm(Box<crate::state::PendingScriptConfirm>),
    /// DAG 목록의 검색·필터·보기 상태.
    DagList(Box<super::dag_list::DagListState>),
    /// 승인 팝업이 보이는 요청과 그 코멘트.
    Approval(super::approval::ShownApproval),
    /// 명령 팔레트의 검색어와 선택.
    CommandPalette(crate::state::command_palette::PaletteInput),
    /// 저장된 원격 행 연결 팝업의 행과 모양.
    RemoteRow(super::remote_row::RemoteRowPayload),
    /// 차례를 기다리는 안내와 맨 앞 안내의 잰 본문 높이.
    InfoModal(crate::adapters::ui::info_modal::InfoModalQueue),
}

impl PopupPayload {
    /// 이 데이터가 `popup_id` 정의의 큐 데이터인지. 데이터 없이 연 큐 인스턴스에 설치할 데이터를 거른다.
    pub(super) fn is_queue_data_for(&self, popup_id: &str) -> bool {
        match self {
            Self::InfoModal(_) => popup_id == crate::adapters::ui::info_modal::INFO_MODAL_ID,
            Self::Approval(_) => popup_id == super::approval::APPROVAL_POPUP_ID,
            _ => false,
        }
    }

    /// `popup_id` 큐 팝업의 데이터면 `incoming`의 항목을 차례 뒤에 잇는다. 데이터 없이 연 인스턴스(debug
    /// 열기)는 `incoming`이 그 정의의 큐 데이터일 때만 그대로 자기 데이터로 삼는다. 다른 데이터는 그대로 둔다.
    pub(super) fn append_queue(&mut self, popup_id: &str, incoming: PopupPayload) {
        if matches!(self, Self::None) && incoming.is_queue_data_for(popup_id) {
            *self = incoming;
            return;
        }
        match (self, incoming) {
            (Self::InfoModal(queue), Self::InfoModal(mut more)) => {
                queue.items.append(&mut more.items)
            }
            // 대기 중인 승인은 이미 창의 inbox에 있다. 다시 연 요청은 보이는 승인과 코멘트를 그대로 둔다.
            (Self::Approval(_), Self::Approval(_)) => {}
            _ => tracing::warn!("queued popup data does not match the open instance"),
        }
    }

    /// `popup_id` 큐 팝업의 데이터면 `earlier`의 항목을 차례 앞에 넣는다. 맨 앞 항목이 바뀌므로 잰 본문
    /// 높이는 버린다. 데이터 없는 인스턴스는 `earlier`가 그 정의의 큐 데이터일 때만 그대로 자기 데이터로
    /// 삼는다. 다른 데이터는 그대로 둔다.
    pub(super) fn prepend_queue(&mut self, popup_id: &str, earlier: PopupPayload) {
        if matches!(self, Self::None) && earlier.is_queue_data_for(popup_id) {
            *self = earlier;
            if let Self::InfoModal(queue) = self {
                queue.body_height = None;
            }
            return;
        }
        match (self, earlier) {
            (Self::InfoModal(queue), Self::InfoModal(mut first)) => {
                first.items.append(&mut queue.items);
                queue.items = first.items;
                queue.body_height = None;
            }
            _ => tracing::warn!("earlier queued popup data does not match the open instance"),
        }
    }
}

impl std::fmt::Debug for PopupPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Convert(c) => f.debug_tuple("Convert").field(c).finish(),
            Self::MouseCaptureMenu(m) => f.debug_tuple("MouseCaptureMenu").field(m).finish(),
            Self::ExplorerConflict { surface_id } => f
                .debug_struct("ExplorerConflict")
                .field("surface_id", surface_id)
                .finish(),
            Self::Search { surface_id } => f
                .debug_struct("Search")
                .field("surface_id", surface_id)
                .finish(),
            Self::Rename(r) => f.debug_tuple("Rename").field(&r.target).finish(),
            Self::ExplorerProperties(_) => f.write_str("ExplorerProperties"),
            Self::FilePicker(p) => f.debug_tuple("FilePicker").field(&p.request_id).finish(),
            Self::HandlerPicker(p) => f
                .debug_tuple("HandlerPicker")
                .field(&p.target_display)
                .finish(),
            Self::ConfirmDeleteCategory(id) => {
                f.debug_tuple("ConfirmDeleteCategory").field(id).finish()
            }
            Self::ConfirmForceDetach(id) => f.debug_tuple("ConfirmForceDetach").field(id).finish(),
            Self::ConfirmCloseRemote(p) => f.debug_tuple("ConfirmCloseRemote").field(p).finish(),
            Self::ToolsMenu { inner_width } => f
                .debug_struct("ToolsMenu")
                .field("inner_width", inner_width)
                .finish(),
            Self::RailCategory(id) => f.debug_tuple("RailCategory").field(id).finish(),
            Self::ScriptConfirm(p) => f.debug_tuple("ScriptConfirm").field(&p.script_id).finish(),
            Self::DagList(_) => f.write_str("DagList"),
            Self::Approval(a) => f.debug_tuple("Approval").field(a).finish(),
            Self::CommandPalette(p) => f.debug_tuple("CommandPalette").field(p).finish(),
            Self::RemoteRow(r) => f.debug_tuple("RemoteRow").field(r).finish(),
            Self::InfoModal(q) => f.debug_tuple("InfoModal").field(&q.items.len()).finish(),
        }
    }
}

/// 열기 요청이 실어 오는 인스턴스 데이터. 진행 중인 읽기처럼 복제할 수 없는 데이터도 실을 수 있게
/// 요청을 복제해도 한 칸을 함께 가리키고, 처음 꺼낸 쪽이 가져간다.
#[derive(Clone, Default)]
pub struct PayloadSlot(std::sync::Arc<std::sync::Mutex<PopupPayload>>);

impl PayloadSlot {
    pub fn new(payload: PopupPayload) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(payload)))
    }

    /// 꺼내지 않고 데이터를 읽는다.
    pub fn peek<R>(&self, read: impl FnOnce(&PopupPayload) -> R) -> R {
        match self.0.lock() {
            Ok(payload) => read(&payload),
            Err(poisoned) => {
                tracing::warn!("popup payload slot was poisoned; reading its value anyway");
                read(&poisoned.into_inner())
            }
        }
    }

    /// 데이터를 꺼낸다. 이미 꺼냈으면 `None` 값이다.
    pub fn take(&self) -> PopupPayload {
        match self.0.lock() {
            Ok(mut payload) => std::mem::take(&mut *payload),
            Err(poisoned) => {
                tracing::warn!("popup payload slot was poisoned; taking its value anyway");
                std::mem::take(&mut *poisoned.into_inner())
            }
        }
    }
}

impl std::fmt::Debug for PayloadSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PayloadSlot")
    }
}
