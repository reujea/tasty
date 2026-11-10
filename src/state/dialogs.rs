//! GUI 대화상자·팝업의 임시 입력 상태.

use std::collections::VecDeque;

use crate::adapters::ui::info_modal::InfoModal;
use crate::adapters::ui::popup::transfer::{TransferError, TransferProgress};

use super::selection;

/// 링크 우클릭 시점의 대상. 출력·스크롤·크기가 바뀌어도 같은 대상을 처리하도록
/// 표시 문자열과 선택 좌표를 저장하며 렌더 캐시를 참조하지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLinkMenu {
    pub(crate) surface_id: u32,
    /// 첫 세그먼트의 시작 셀(포함).
    pub(crate) start: selection::SelectionPoint,
    /// 마지막 세그먼트의 끝 셀(포함).
    pub(crate) end: selection::SelectionPoint,
    /// 우클릭 시점에 그 범위에서 추출한 화면 텍스트. OSC 8 이면 URI 가 아니라 라벨이다.
    pub(crate) text: String,
    /// "연결 동작" 이 picker 에 넘길 대상. `None` 이면 항목을 노출하지 않는다
    /// (mailto 등 핸들러로 열 곳이 없는 scheme).
    pub(crate) open_with: Option<crate::file::dispatch::DispatchTarget>,
    /// 원격(mirror) surface 의 경로 링크 — 원격에 열 수 있는 핸들러만 picker 에 싣는다.
    pub(crate) remote_path: bool,
}

/// A pending native context menu request.
#[derive(Clone)]
pub enum PendingNativeMenu {
    Tab {
        pane_id: u32,
        tab_index: usize,
        x: f32,
        y: f32,
    },
    Pane {
        pane_id: u32,
        x: f32,
        y: f32,
    },
    Workspace {
        ws_idx: usize,
        x: f32,
        y: f32,
    },
    /// 터미널 메뉴 위치는 논리 픽셀이다.
    TerminalSurface {
        surface_id: u32,
        x: f32,
        y: f32,
    },
    /// 링크 선택·복사·연결 메뉴. 위치는 논리 픽셀이다.
    TerminalLink {
        link: TerminalLinkMenu,
        x: f32,
        y: f32,
    },
    /// 터미널 외 surface의 메뉴. 위치는 논리 픽셀이다.
    Surface {
        surface_id: u32,
        x: f32,
        y: f32,
    },
    /// 파일·다중 선택·빈 영역의 파일 메뉴. 빈 paths는 cwd를 뜻한다.
    /// single_is_dir는 paths가 하나일 때만 유효하며 위치는 논리 픽셀이다.
    Explorer {
        surface_id: u32,
        paths: Vec<std::path::PathBuf>,
        cwd: std::path::PathBuf,
        single_is_dir: bool,
        x: f32,
        y: f32,
    },
    /// 즐겨찾기의 폴더를 현재 explorer에 적용할 수 있도록 surface_id를 보관한다.
    ExplorerFavorite {
        surface_id: u32,
        path: std::path::PathBuf,
        x: f32,
        y: f32,
    },
    /// 좁은 explorer 칸에서 접은 툴바 명령 메뉴. 위치는 논리 픽셀이다.
    ExplorerMore {
        surface_id: u32,
        x: f32,
        y: f32,
    },
    NewWorkspaceButton {
        x: f32,
        y: f32,
    },
    WorkspaceCategoryHeader {
        cat_id: crate::model::WorkspaceCategoryId,
        x: f32,
        y: f32,
    },
    SidebarBackground {
        x: f32,
        y: f32,
    },
    NewTabButton {
        pane_id: u32,
        x: f32,
        y: f32,
    },
}

/// 원격 이름 변경 요청 하나를 기다리는 이름 변경 팝업.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenameWaiting {
    /// 파일 작업 대기열이 준 요청 번호.
    pub(crate) request: u64,
    /// 대기 줄에 적을 원격 이름.
    pub(crate) host: String,
    /// 원격이 성공을 알렸다. 팝업이 다음 프레임에 닫힌다.
    pub(crate) done: bool,
}

/// 대화상자·팝업의 임시 상태. 새 항목은 RequestContext 최상위 대신 여기에 둔다.
pub struct DialogState {
    pub(crate) rename: Option<(RenameTarget, String)>,
    /// 이름 변경 팝업이 직전 프레임에 그린 콘텐츠 높이. 오류·대기 줄이 생기고 사라지는 대로 sizer 가
    /// 팝업 높이를 맞춘다. 아직 그리지 않았으면 `None`.
    pub(crate) rename_content_height: Option<tasty_type_geometry::length::LogicalPx>,
    /// 원격 이름 변경의 답을 기다린다. 기다리는 동안 팝업은 열린 채 입력과 버튼을 막는다.
    pub(crate) rename_waiting: Option<RenameWaiting>,
    /// 마우스 캡처 배너 메뉴의 대상 surface ID.
    pub(crate) mouse_capture_banner_menu_target: Option<u32>,
    /// 열 때 프로그램 이름을 몰라 이름 미상 형태(사유 줄 + 비활성 두 행)로 열었는지.
    /// 열린 동안 이름이 생겨도 형태를 바꾸지 않고 다음 열기에 정상 행을 보인다.
    pub(crate) mouse_capture_banner_menu_unknown: bool,
    /// 마우스 캡처 배너 메뉴를 열 때 잰 (셸 안쪽 폭, 셸 높이). 폭은 프로그램 이름에 따라,
    /// 높이는 줄을 바꾼 행(고정 문구가 상한 폭을 넘는 언어)에 따라 달라진다.
    pub(crate) mouse_capture_banner_menu_size: Option<(
        tasty_type_geometry::length::LogicalPx,
        tasty_type_geometry::length::LogicalPx,
    )>,
    /// 도구 메뉴 셸 안쪽 폭. 열 때와 그릴 때 가장 넓은 라벨로 잰다.
    pub(crate) tools_menu_width: Option<tasty_type_geometry::length::LogicalPx>,
    /// 변환 팝업 대상. None이면 닫힌 상태다.
    pub(crate) convert_popup: Option<u32>,
    pub(crate) convert_popup_selected: Option<usize>,
    pub(crate) pending_native_menu: Option<PendingNativeMenu>,
    pub(crate) tab_drag: Option<TabDragState>,
    pub(crate) ws_drag: Option<WsDragState>,
    /// 부팅 안내 대기열. 확인 버튼으로 맨 앞 항목을 처리한다.
    pub(crate) info_modal_queue: VecDeque<InfoModal>,
    /// 맨 앞 안내의 본문 높이를 직전 프레임에 잰 값. sizer가 셸 높이를 정할 때 읽고,
    /// 맨 앞 항목이 바뀌면 비운다.
    pub(crate) info_modal_body_height: Option<tasty_type_geometry::length::LogicalPx>,
    /// 안내 모달의 [권한 설정 열기]가 눌렸는지 나타낸다. 팝업을 그리는 코드는 winit
    /// 이벤트 루프에 접근할 수 없어 여기에 표시만 하고, App 계층이 프레임 시작에 읽어
    /// 처리한다.
    pub(crate) permission_settings_requested: bool,
    /// 응답 대기 중인 approval 큐. 맨 앞 요청을 표시한다.
    pub(crate) approval_records:
        std::collections::HashMap<tasty_approval::ApprovalId, tasty_approval::ApprovalRecord>,
    pub(crate) approval_submitting: Option<tasty_approval::ApprovalId>,
    pub(crate) pending_approval_ids: VecDeque<tasty_approval::ApprovalId>,
    pub(crate) approval_comment_buffer: String,
    pub(crate) file_handler_picker: Option<FileHandlerPickerData>,
    /// 파일 피커의 탐색·조회·선택 상태. None이면 요청이 없다.
    pub(crate) file_picker: Option<FilePickerData>,
    /// 피커를 연 본체 호출자가 보관하는 요청 정보. 결과의 의미는 각 호출자가 정한다.
    pub(crate) file_picker_callers: FilePickerCallers,
    /// DAG 목록 팝업의 검색·필터·보기 상태. 닫을 때 초기화하며 영속화하지 않는다.
    pub(crate) dag_list: crate::adapters::ui::popup::dag_list::DagListState,
    /// 프리셋 창 열기 요청. App이 처리한 뒤 지운다.
    pub(crate) pending_open_preset_window: bool,
    /// 프리셋 창을 열 때 선택할 항목.
    pub(crate) pending_preset_window_selection: Option<(tasty_presets::PresetKind, String)>,
    pub(crate) preset_picker_selected: Option<String>,
    /// 복사 모드 진입 요청. 후속 단축키 처리에서 포커스를 확인한다.
    pub(crate) pending_enter_copy_mode: bool,
    /// 축소 사이드바 카테고리 팝업의 대상 ID.
    pub(crate) rail_category_popup: Option<crate::model::WorkspaceCategoryId>,
    /// 워크스페이스 프리셋 적용 대상 카테고리. 적용할 때 소비하고 닫을 때 지운다.
    pub(crate) preset_apply_target_category: Option<crate::model::WorkspaceCategoryId>,
    pub(crate) pending_category_delete: Option<crate::model::WorkspaceCategoryId>,
    /// 강제 연결 해제 대상 워크스페이스 ID. 팝업이 열린 동안 재정렬될 수 있어 인덱스를 쓰지 않는다.
    pub(crate) pending_force_detach_workspace: Option<crate::model::WorkspaceId>,
    /// 등록 해시와 다른 Lua 스크립트를 실행할지 확인하는 보류 상태.
    pub(crate) pending_script_confirm: Option<PendingScriptConfirm>,
    /// 원격 파일 전송 진행 상태. 모든 행이 끝나면 지운다.
    pub(crate) transfer_progress: Option<TransferProgress>,
    /// 전송 실패 알림 큐. 맨 앞 항목을 표시하고 Dismiss로 제거한다.
    pub(crate) transfer_error: VecDeque<TransferError>,
    /// 탐색기 Properties 팝업이 연 항목과 읽은 정보.
    pub(crate) explorer_properties:
        Option<crate::adapters::ui::popup::explorer_properties::ExplorerProperties>,
}

/// 변경된 Lua 소스를 실행하기 전 사용자 확인에 사용하는 상태.
#[derive(Debug, Clone)]
pub struct PendingScriptConfirm {
    pub(crate) script_id: String,
    pub(crate) name: String,
    /// 확인할 때 다시 읽지 않고 실행할 파일 소스.
    pub(crate) source: String,
    /// 승인 시 등록할 현재 소스의 SHA256.
    pub(crate) new_hash: String,
    /// Some(true)는 실행, Some(false)는 취소다.
    pub(crate) result: Option<bool>,
    /// 직전 프레임에 잰 popup 콘텐츠 높이. 본문이 줄바꿈되면 popup 높이를 이 값에 맞춘다.
    pub(crate) content_height: Option<tasty_type_geometry::length::LogicalPx>,
}

/// Tab drag-and-drop state (UI-only, not persisted).
#[derive(Clone)]
pub struct TabDragState {
    pub(crate) pane_id: u32,
    pub(crate) tab_index: usize,
    /// Current mouse x in logical pixels (for insert position calculation).
    pub(crate) current_x: f32,
}

/// Workspace drag-and-drop state (UI-only, not persisted).
#[derive(Clone)]
pub struct WsDragState {
    pub(crate) ws_idx: usize,
    /// Current mouse y in logical pixels (for insert position calculation).
    pub(crate) current_y: f32,
}

impl DialogState {
    pub fn new() -> Self {
        Self {
            rename: None,
            rename_content_height: None,
            rename_waiting: None,
            mouse_capture_banner_menu_target: None,
            mouse_capture_banner_menu_unknown: false,
            mouse_capture_banner_menu_size: None,
            tools_menu_width: None,
            convert_popup: None,
            convert_popup_selected: None,
            pending_native_menu: None,
            tab_drag: None,
            ws_drag: None,
            info_modal_queue: VecDeque::new(),
            info_modal_body_height: None,
            permission_settings_requested: false,
            approval_records: Default::default(),
            approval_submitting: None,
            pending_approval_ids: VecDeque::new(),
            approval_comment_buffer: String::new(),
            file_handler_picker: None,
            file_picker: None,
            file_picker_callers: FilePickerCallers::default(),
            dag_list: Default::default(),
            pending_preset_window_selection: None,
            pending_open_preset_window: false,
            preset_picker_selected: None,
            pending_enter_copy_mode: false,
            rail_category_popup: None,
            preset_apply_target_category: None,
            pending_category_delete: None,
            pending_force_detach_workspace: None,
            pending_script_confirm: None,
            transfer_progress: None,
            transfer_error: VecDeque::new(),
            explorer_properties: None,
        }
    }

    /// 텍스트 입력 중인 이름 변경 창이 있는지 확인한다.
    pub fn has_text_input_open(&self) -> bool {
        self.rename.is_some()
    }

    /// 현재 이 판정에서는 이름 변경 창만 포함한다.
    pub fn has_any_overlay(&self) -> bool {
        self.has_text_input_open()
    }
}

/// 파일 핸들러 선택 행에 필요한 표시 정보.
#[derive(Debug, Clone)]
pub struct PickerHandlerSummary {
    pub(crate) id: crate::file::handler::HandlerId,
    /// 번역한 표시명. 없으면 id의 마지막 조각을 고정폭 글꼴로 표시한다.
    pub(crate) display_name: Option<String>,
    /// 내장·사용자·플러그인 출처.
    pub(crate) owner: crate::file::handler::HandlerOwner,
    /// `OpenSurface` 가 여는 surface kind. 행 글리프의 출처이고, `Ipc`/`System` 은 `None`.
    pub(crate) surface_kind: Option<String>,
    /// Recent 행의 마지막 사용 시각(unix epoch 초). 후보 행은 `None`.
    pub(crate) last_used_at: Option<i64>,
}

/// 핸들러 선택 결과를 보관한다. App이 프레임 끝에 실행하고 최근 선택을 기록한다.
#[derive(Debug, Clone)]
pub struct FileHandlerPickerData {
    /// Explicit dispatch owner, retained through picker selection and cancellation.
    pub(crate) origin_surface_id: Option<u32>,
    /// 선택 후 실행까지 유지할 원래 요청 출처. 사용자 요청일 때만 결과를 선택한다.
    pub(crate) dispatch_origin: crate::file::dispatch::FileDispatchOrigin,
    /// 실행에 쓸 원본 대상. 표시용 target_display는 축약돼 있을 수 있어 대신 사용하지 않는다.
    pub(crate) target: crate::file::dispatch::DispatchTarget,
    /// 표시용 — picker 헤더에 보일 대상 (예: 파일 경로).
    pub(crate) target_display: String,
    /// 탐지된 detector — 없을 수도 있음 ($unknown 등 unmatched).
    pub(crate) detector: Option<crate::file::format::DetectorId>,
    /// 목록의 첫 그룹(`Suggested` / fallback 이면 전체 핸들러) — handler id 사전순.
    pub(crate) candidates: Vec<PickerHandlerSummary>,
    /// 일치한 핸들러가 없어 전체 목록을 표시하는지 나타낸다.
    /// 여기서 고른 항목은 한 번만 실행하며 detector의 기본 핸들러로 저장하지 않는다.
    pub(crate) candidates_are_fallback: bool,
    /// 같은 목록의 둘째 그룹(`Recent`) — 현재 등록된 것만, 저장 파일 순서.
    pub(crate) recent: Vec<PickerHandlerSummary>,
    /// 이 형식의 기본 핸들러. fallback 목록에서는 None이다.
    pub(crate) default_handler: Option<crate::file::handler::HandlerId>,
    /// 현재 선택된 handler. 더블클릭/[열기]로 dispatch.
    pub(crate) selected: Option<crate::file::handler::HandlerId>,
    /// dispatch 결과. host 본체 layer 가 frame 끝에서 소비.
    pub(crate) result: Option<FileHandlerPickerResult>,
    /// 원래 요청의 크기 제한 우회 설정. 선택 후 실행까지 유지한다.
    pub(crate) ignore_size_limit: bool,
    /// 최종 결과를 기다리는 요청의 run. 선택 없이 사라지면 `cancelled`로 끝난다.
    pub(crate) run: Option<crate::request_runs::RunGuard>,
    /// 대상이 mirror 원격 호스트의 경로이면 그 호스트 글자(비면 `Remote` 낱말). 로컬 대상은 None.
    pub(crate) remote_host: Option<String>,
}

/// picker 의 닫기 사유.
#[derive(Debug, Clone)]
pub enum FileHandlerPickerResult {
    /// 사용자가 handler 선택. host 본체 layer 가 실행 + recent 기록.
    Selected(crate::file::handler::HandlerId),
    /// 취소 또는 ESC — dispatch 없음.
    Cancelled,
    /// 등록된 핸들러가 없을 때 설정 열기 선택. App이 파일 핸들러 설정 모달을 연다.
    OpenSettings,
}

/// 파일 피커의 조회 상태. ErrorConn은 원격 연결·응답 오류에 사용한다.
#[derive(Debug, Clone)]
pub(crate) enum FpLoadState {
    /// 원격 응답 대기. wrapper가 sent_at으로 soft timeout을 확인한다.
    Loading {
        request_id: u64,
        sent_at: std::time::Instant,
    },
    /// 엔트리 로드 완료(`FilePickerData::entries` 참조, 비어있지 않음).
    Loaded,
    /// 로드 완료했으나 디렉토리가 비어있음.
    Empty,
    /// 권한 거부(로컬/원격 공통 — 파일시스템 자체의 read 실패).
    ErrorPerm(String),
    /// 원격 연결 끊김/타임아웃(원격 전용 — mirror 세션의 `disconnected` 플래그 또는
    /// soft timeout).
    ErrorConn(String),
}

/// 파일 피커 요청 하나의 식별자. 열 때마다 새 값이며 플러그인 요청이면 wire의 request_id와 같다.
/// 호출자는 보관한 값과 같은 결과만 적용한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FilePickerRequestId(pub(crate) u64);

impl FilePickerRequestId {
    pub(crate) fn next() -> Self {
        Self(crate::core::next_file_picker_request_id())
    }
}

/// 피커가 둘러볼 위치. 호출자가 열기 전에 정해 넘기고 결과를 해석할 때 다시 쓴다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilePickerLocation {
    /// 로컬 디스크. `dir`이 절대 디렉터리가 아니면 홈에서 출발한다.
    Local { dir: Option<String> },
    /// mirror workspace가 보여 주는 원격 목록. `surface_id`는 그 workspace의 surface이며
    /// 결과로 원격 파일을 열 때 출발 surface로 쓴다. `dir`이 없으면 서버가 홈으로 해석한다.
    Remote {
        mirror_ws_id: u32,
        surface_id: Option<u32>,
        dir: Option<String>,
    },
}

/// 결과 이벤트를 요청한 플러그인에만 보낼 때 사용하는 정보.
#[derive(Debug, Clone)]
pub(crate) struct FilePickerRequester {
    pub(crate) plugin_id: String,
    /// 피커 호출 ID. 디렉터리 조회의 Loading.request_id와는 별개다.
    pub(crate) request_id: u64,
    /// 플러그인이 부모로 지정한 팝업 인스턴스. 피커 상태와 함께 보관하고 제거한다.
    pub(crate) owner_popup_instance: Option<u64>,
    /// 피커를 띄운 surface. 사용자가 다른 surface에서 누른 입력으로 이 피커에 포커스를 주지 않게 비교한다.
    pub(crate) origin_surface_id: Option<u32>,
}

/// 파일 피커의 탐색·선택 상태. 피커는 경로를 고르는 데서 끝나며 결과로 무엇을 할지 모른다.
pub(crate) struct FilePickerData {
    /// None opens existing files; Some edits a single save name without writing.
    pub(crate) save_name: Option<String>,
    /// 이 요청의 식별자. 결과와 함께 호출자에게 간다.
    pub(crate) request_id: FilePickerRequestId,
    /// 원격 탐색 대상인 로컬 mirror workspace ID. None이면 로컬 탐색이다.
    pub(crate) mirror_ws_id: Option<u32>,
    /// 헤더 host 배지 문자열(`mirror_ws_id.is_some()` 일 때만 유효).
    pub(crate) remote_host: Option<String>,
    /// 현재 표시 중인 디렉토리(로컬: 절대경로, 원격: 원격 경로 문자열).
    pub(crate) current_dir: String,
    pub(crate) load: FpLoadState,
    pub(crate) local_query:
        Option<crate::app::local_reads::Query<Vec<crate::core::fs_list::DirEntryInfo>>>,
    /// 현재 디렉토리의 엔트리(`load` 가 `Loaded`/`Empty` 일 때만 최신).
    pub(crate) entries: Vec<crate::core::fs_list::DirEntryInfo>,
    /// 현재 디렉터리에서 선택한 이름. Select는 단일 선택으로 교체한다.
    pub(crate) selected: Vec<String>,
    /// Confirm/Cancel 결과. App이 프레임 끝에서 꺼내 호출자에게 넘긴다.
    pub(crate) result: Option<FilePickerResult>,
    /// `Some` 이면 `file_picker.trigger` IPC 로 이 popup 을 연 plugin.
    /// 결과 전달 대상이면서 부모 popup 범위·숨김 판정에도 쓴다. 본체 호출자는 `None`.
    pub(crate) requester: Option<FilePickerRequester>,
    /// 점 없는 확장자 필터. 비어 있으면 제한하지 않으며 디렉터리는 항상 표시한다.
    pub(crate) filters: Vec<String>,
}

/// 파일 피커가 남기는 결과. 경로 문자열만 담는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilePickerResult {
    /// 취소, ESC, 결과 없이 닫힘.
    Cancelled,
    /// 사용자가 [열기]로 고른 절대 경로(로컬) 또는 원격 경로 문자열.
    Confirmed { paths: Vec<String> },
}

/// 호출자에게 넘기는 결과. 취소면 `paths`가 비고 `cancelled`가 참이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilePickerOutcome {
    pub(crate) request_id: FilePickerRequestId,
    pub(crate) paths: Vec<String>,
    pub(crate) cancelled: bool,
}

impl FilePickerOutcome {
    pub(crate) fn new(request_id: FilePickerRequestId, result: FilePickerResult) -> Self {
        match result {
            FilePickerResult::Cancelled => Self {
                request_id,
                paths: Vec::new(),
                cancelled: true,
            },
            FilePickerResult::Confirmed { paths } => Self {
                request_id,
                paths,
                cancelled: false,
            },
        }
    }
}

/// 본체 호출자별로 기다리는 요청. 호출자는 열 때 자기 칸을 채우고, 같은 식별자의 결과를 받으면 비운다.
/// 플러그인 요청은 [`FilePickerData::requester`]가 대신한다.
#[derive(Debug, Default)]
pub(crate) struct FilePickerCallers {
    /// Tools 메뉴·단축키의 파일 열기.
    pub(crate) open_file: Option<OpenFilePickerRequest>,
    /// 탭 스트립 메뉴의 "새 HTML..." 같은 탭 생성.
    pub(crate) new_tab: Option<NewTabPickerRequest>,
}

/// Tools 메뉴·단축키가 연 피커. 둘러보게 한 위치로 로컬 열기와 원격 열기를 고른다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenFilePickerRequest {
    pub(crate) request_id: FilePickerRequestId,
    pub(crate) location: FilePickerLocation,
}

/// 고른 파일로 `pane_id` pane에 `kind` 탭을 만들고 경로를 `param_key` 파라미터로 넘긴다.
/// pane은 ID로 고정하므로 피커가 열린 동안 활성 pane이 바뀌어도 대상은 그대로다.
#[derive(Debug, Clone)]
pub(crate) struct NewTabPickerRequest {
    pub(crate) mirror: Option<(u32, std::sync::Weak<()>)>,
    pub(crate) request_id: FilePickerRequestId,
    pub(crate) pane_id: u32,
    pub(crate) kind: String,
    pub(crate) param_key: String,
}

impl PartialEq for NewTabPickerRequest {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.pane_id == other.pane_id
            && self.kind == other.kind
            && self.param_key == other.param_key
            && match (&self.mirror, &other.mirror) {
                (None, None) => true,
                (Some((a, x)), Some((b, y))) => a == b && x.ptr_eq(y),
                _ => false,
            }
    }
}
impl Eq for NewTabPickerRequest {}

/// workspace·탭 대상은 ID로 고정한다. 팝업이 열린 동안 에이전트가 닫기·이동으로
/// 인덱스를 바꿔도 사용자가 고른 대상에만 적용되고, 대상이 사라지면 팝업을 닫는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    WorkspaceName {
        workspace_id: u32,
    },
    WorkspaceSubtitle {
        workspace_id: u32,
    },
    TabName {
        tab_id: u32,
    },
    /// 이름을 바꿀 파일·폴더와 explorer surface. `binding`은 메뉴를 연 시점의 surface 세대다.
    ExplorerEntry {
        surface_id: u32,
        path: std::path::PathBuf,
        binding: crate::runtime::surface_binding::SurfaceBinding,
    },
    /// 경로를 입력한 표시 이름으로 전역 즐겨찾기에 등록한다.
    ExplorerAddFavorite {
        path: std::path::PathBuf,
    },
    NewCategory,
    CategoryName {
        cat_id: crate::model::WorkspaceCategoryId,
    },
}

impl RenameTarget {
    pub fn heading_key(&self) -> &'static str {
        match self {
            Self::WorkspaceName { .. } => "rename_dialog.title_heading",
            Self::WorkspaceSubtitle { .. } => "rename_dialog.subtitle_heading",
            Self::TabName { .. } => "rename_dialog.tab_heading",
            Self::ExplorerEntry { .. } => "explorer.popup.rename.title",
            Self::ExplorerAddFavorite { .. } => "explorer.popup.add_favorite.title",
            Self::NewCategory => "rename_dialog.new_category_heading",
            Self::CategoryName { .. } => "rename_dialog.category_heading",
        }
    }

    /// 대상 ID로 표시 범위를 정한다. 대상을 찾지 못하면 창 범위로 연다.
    pub fn popup_scope(
        &self,
        engine: &crate::core::CoreState,
    ) -> crate::model::popup_kind::PopupScope {
        use crate::model::popup_kind::PopupScope;
        match self {
            Self::WorkspaceName { workspace_id } | Self::WorkspaceSubtitle { workspace_id } => {
                engine
                    .find_workspace_index_for_id(*workspace_id)
                    .map_or(PopupScope::Window, |_| PopupScope::Workspace(*workspace_id))
            }
            Self::TabName { tab_id } => engine
                .find_pane_for_tab(*tab_id)
                .map_or(PopupScope::Window, |_| PopupScope::Tab(*tab_id)),
            Self::ExplorerEntry { surface_id, .. } => {
                crate::model::popup_kind::PopupScope::Surface(*surface_id)
            }
            Self::ExplorerAddFavorite { .. } => crate::model::popup_kind::PopupScope::Window,
            Self::NewCategory | Self::CategoryName { .. } => {
                crate::model::popup_kind::PopupScope::Window
            }
        }
    }
}
