//! Manifest schema 타입 정의 — 모든 `struct` / `enum` + `impl Permission`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::validators::{
    is_reserved_ipc_prefix, is_valid_hook_handler_id, is_valid_ipc_prefix, is_valid_plugin_id,
};

/// 호스트가 지원하는 plugin protocol 메이저 버전.
/// plugin 매니페스트의 `api_version`과 일치해야 한다.
pub const HOST_API_VERSION: &str = "1";

/// 매니페스트 스키마 버전 (이 파일 형식 자체의 버전).
pub const MANIFEST_VERSION: u32 = 1;

/// hook 항목의 `timeout_ms` 상한. 1초를 넘는 hook은 거부.
pub const HOOK_TIMEOUT_MS_MAX: u32 = 1000;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    pub api_version: String,
    pub entry: Entry,
    #[serde(default)]
    pub surface_kinds: Vec<SurfaceKindDecl>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Event Bus 구독 허용 패턴 일람. 정확한 키 또는 `<namespace>.*` 와일드카드.
    /// 비어 있으면 plugin은 이벤트를 구독할 수 없다.
    #[serde(default)]
    pub event_subscribe: Vec<String>,
    /// 발행할 수 있는 이벤트 패턴. 호스트 전용 namespace를 선언하면 검증에서 거부한다.
    #[serde(default)]
    pub event_publish: Vec<String>,
    /// event_publish 범위 안에서 실제로 발행할 이벤트의 키·설명 목록.
    /// 외부 도구의 조회와 확장 플러그인의 훅 참조에 사용한다.
    #[serde(default, rename = "events_emitted")]
    pub events_emitted: Vec<EventEmittedDecl>,
    #[serde(default)]
    pub contributes: Contributes,
    /// 다른 플러그인 하나의 IPC·이벤트를 확장한다. 일반 기능 선언과 함께 사용할 수 있다.
    #[serde(default)]
    pub extends: Option<ExtendsDecl>,
    /// 매니페스트 기준 언어 파일 디렉터리. 기본값은 lang이며 호스트가 카탈로그에 합친다.
    #[serde(default = "default_lang_dir")]
    pub lang_dir: String,
    /// 배포 패키지에 포함할지 여부. 기본값은 true다.
    /// false여도 개발용 build-plugins/link-plugins에는 영향을 주지 않는다.
    #[serde(default = "default_bundle")]
    pub bundle: bool,
}

fn default_lang_dir() -> String {
    "lang".to_string()
}

fn default_bundle() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Entry {
    #[serde(rename = "process")]
    Process {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

/// 선언할 수 있는 권한. 쓰기 권한이 읽기 권한을 포함하지 않으므로 각각 선언해야 한다.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Permission {
    /// surface/tab/workspace 트리 조회
    SurfaceRead,
    /// surface 생성/닫기/이동
    SurfaceWrite,
    /// 알림 생성/관리
    Notification,
    /// 클립보드 읽기
    ClipboardRead,
    /// 클립보드 쓰기
    ClipboardWrite,
    /// 호스트 노출 fs 읽기
    FsRead,
    /// 파일 쓰기
    FsWrite,
    /// 외부 프로세스 실행
    ProcessSpawn,
    /// 새 터미널 surface 생성
    TerminalSpawn,
    /// 터미널 입력 송신
    TerminalWrite,
    /// 터미널 출력/scrollback 읽기
    TerminalRead,
    /// 호스트를 통한 네트워크 (예약)
    Network,
    /// 에이전트 메모리(`memory.*`) 읽기
    MemoryRead,
    /// 에이전트 메모리(`memory.*`) 쓰기
    MemoryWrite,
    /// Secret memory(`memory.secret.*`) 접근. plugin 별 사전 분할이라 R/W를 분리하지 않는다.
    MemorySecret,
    /// approval.*의 승인 요청·응답·조회. 토큰은 approval이다.
    Approval,
    /// Telemetry namespace 접근 (`telemetry.*`). 메트릭 기록·조회, cap/anomaly 관리.
    /// 토큰 `telemetry`.
    Telemetry,
    /// agent.*의 작업·동기화·실행 조정 기능. 토큰은 agent다.
    AgentManage,
    /// provider 플러그인이 agent task 의 턴 시작·끝을 보고하는 `agent.task_turn_report` 하나만 연다.
    /// `agent` 와 달리 다른 agent.* 메서드는 열지 않는다. 토큰은 agent.turn_report다.
    AgentTurnReport,
    /// 다른 plugin이 점유한 IPC namespace prefix의 메서드 호출.
    /// 토큰 형식: `ipc.invoke:<prefix>` (예: `ipc.invoke:codex`).
    IpcInvoke(String),
    /// 대상 플러그인의 IPC·이벤트를 확장할 권한. 토큰은 ext:<target_plugin_id>다.
    /// extends 선언에 필요하며 사용자가 승인하기 전에는 활성화하지 않는다.
    /// 세부 동작·훅 대상은 extends에서 정하고 대상별 권한 한 개로 승인한다.
    Extension(String),
    /// 도구 메뉴 항목 선언에 필요한 ui.tool_item 권한.
    UiToolItem,
    /// 팝업 선언에 필요한 ui.popup 권한.
    UiPopup,
    /// 배너 선언에 필요한 ui.banner 권한.
    UiBanner,
    /// 설정 페이지 선언에 필요한 ui.settings_page 권한.
    UiSettingsPage,
    /// 별도 창 선언에 필요한 window.spawn 권한.
    WindowSpawn,
    /// 새 detector 정의 권한. 토큰 `file_handler.define`.
    /// `[[contributes.detector]]` 로 **신규** detector id 를 선언할 때 필요.
    /// 기존 id 재선언(rule 추가)은 `FileHandlerExtend` 가 담당.
    FileHandlerDefine,
    /// 기존 detector 재선언(rule 추가) 권한. 토큰 형식: `file_handler.extend:<id>`.
    /// `$unknown` 은 존재하지 않는 detector 라서 token parse 단계에서 reject.
    FileHandlerExtend(String),
    /// 특정 detector 에 handler attach 권한. 토큰 형식: `file_handler.handle:<id>`.
    /// `$unknown` 은 reject. `$directory` 등 실 등록된 reserved id 는 허용.
    FileHandlerHandle(String),
    /// IpcSequence 훅 핸들러 선언에 필요한 hook_handler.define 권한.
    HookHandlerDefine,
    /// 특정 훅 핸들러용 예약 권한. hook_handler.handle:<id> 형식이다.
    /// ID는 영문 소문자·숫자·하이픈으로 된 1..=32자 이름이다.
    HookHandlerHandle(String),
    /// poll/push 완료 전략 선언에 필요한 completion_strategy.define 권한.
    CompletionStrategyDefine,
}

impl Permission {
    pub fn from_token(s: &str) -> Option<Self> {
        Some(match s {
            "surface.read" => Self::SurfaceRead,
            "surface.write" => Self::SurfaceWrite,
            "notification" => Self::Notification,
            "clipboard.read" => Self::ClipboardRead,
            "clipboard.write" => Self::ClipboardWrite,
            "fs.read" => Self::FsRead,
            "fs.write" => Self::FsWrite,
            "process.spawn" => Self::ProcessSpawn,
            "terminal.spawn" => Self::TerminalSpawn,
            "terminal.write" => Self::TerminalWrite,
            "terminal.read" => Self::TerminalRead,
            "network" => Self::Network,
            "memory.read" => Self::MemoryRead,
            "memory.write" => Self::MemoryWrite,
            "memory.secret" => Self::MemorySecret,
            "approval" => Self::Approval,
            "telemetry" => Self::Telemetry,
            "agent" => Self::AgentManage,
            "agent.turn_report" => Self::AgentTurnReport,
            "ui.tool_item" => Self::UiToolItem,
            "ui.popup" => Self::UiPopup,
            "ui.banner" => Self::UiBanner,
            "ui.settings_page" => Self::UiSettingsPage,
            "window.spawn" => Self::WindowSpawn,
            "file_handler.define" => Self::FileHandlerDefine,
            "hook_handler.define" => Self::HookHandlerDefine,
            "completion_strategy.define" => Self::CompletionStrategyDefine,
            other => {
                if let Some(prefix) = other.strip_prefix("ipc.invoke:") {
                    if !is_valid_ipc_prefix(prefix) || is_reserved_ipc_prefix(prefix) {
                        return None;
                    }
                    return Some(Self::IpcInvoke(prefix.to_string()));
                }
                if let Some(target) = other.strip_prefix("ext:") {
                    if !is_valid_plugin_id(target) {
                        return None;
                    }
                    return Some(Self::Extension(target.to_string()));
                }
                if let Some(id) = other.strip_prefix("file_handler.extend:") {
                    // 여기서는 형식을 검사하고 호스트가 설치 때 실제 감지기와 대조한다.
                    if !is_valid_detector_id_local(id) || id == "$unknown" {
                        return None;
                    }
                    return Some(Self::FileHandlerExtend(id.to_string()));
                }
                if let Some(id) = other.strip_prefix("file_handler.handle:") {
                    if !is_valid_detector_id_local(id) || id == "$unknown" {
                        return None;
                    }
                    return Some(Self::FileHandlerHandle(id.to_string()));
                }
                if let Some(id) = other.strip_prefix("hook_handler.handle:") {
                    // 훅 핸들러 이름에는 예약 ID용 $ 접두어가 없다.
                    if !is_valid_hook_handler_id(id) {
                        return None;
                    }
                    return Some(Self::HookHandlerHandle(id.to_string()));
                }
                return None;
            }
        })
    }

    /// 저장·비교에 사용할 권한 토큰을 반환한다. 대상 이름이 있는 권한도 포함한다.
    pub fn as_token(&self) -> String {
        match self {
            Self::SurfaceRead => "surface.read".into(),
            Self::SurfaceWrite => "surface.write".into(),
            Self::Notification => "notification".into(),
            Self::ClipboardRead => "clipboard.read".into(),
            Self::ClipboardWrite => "clipboard.write".into(),
            Self::FsRead => "fs.read".into(),
            Self::FsWrite => "fs.write".into(),
            Self::ProcessSpawn => "process.spawn".into(),
            Self::TerminalSpawn => "terminal.spawn".into(),
            Self::TerminalWrite => "terminal.write".into(),
            Self::TerminalRead => "terminal.read".into(),
            Self::Network => "network".into(),
            Self::MemoryRead => "memory.read".into(),
            Self::MemoryWrite => "memory.write".into(),
            Self::MemorySecret => "memory.secret".into(),
            Self::Approval => "approval".into(),
            Self::Telemetry => "telemetry".into(),
            Self::AgentManage => "agent".into(),
            Self::AgentTurnReport => "agent.turn_report".into(),
            Self::IpcInvoke(prefix) => format!("ipc.invoke:{prefix}"),
            Self::Extension(target) => format!("ext:{target}"),
            Self::UiToolItem => "ui.tool_item".into(),
            Self::UiPopup => "ui.popup".into(),
            Self::UiBanner => "ui.banner".into(),
            Self::UiSettingsPage => "ui.settings_page".into(),
            Self::WindowSpawn => "window.spawn".into(),
            Self::FileHandlerDefine => "file_handler.define".into(),
            Self::FileHandlerExtend(id) => format!("file_handler.extend:{id}"),
            Self::FileHandlerHandle(id) => format!("file_handler.handle:{id}"),
            Self::HookHandlerDefine => "hook_handler.define".into(),
            Self::HookHandlerHandle(id) => format!("hook_handler.handle:{id}"),
            Self::CompletionStrategyDefine => "completion_strategy.define".into(),
        }
    }
}

/// 발행 이벤트의 설명.
/// key는 와일드카드·예약 namespace 없이 event_publish 허용 범위에 들어야 한다.
/// description은 설명, stability는 stable 또는 experimental이다.
/// payload_schema는 매니페스트 기준 JSON Schema 경로이며 호스트의 본문 검증에는 사용하지 않는다.
#[derive(Debug, Clone, Deserialize)]
pub struct EventEmittedDecl {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub stability: EventStability,
    #[serde(default)]
    pub payload_schema: Option<String>,
}

/// `events_emitted` 항목의 안정성 등급. `event-catalog.md`의 정책을 따른다.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventStability {
    #[default]
    Stable,
    Experimental,
}

/// hook.set/surface.fire_hook에 사용할 플러그인 훅 이벤트 선언. 이벤트 버스와는 별개다.
/// 호스트는 내장 이벤트와 활성 플러그인의 선언을 함께 검사한다.
/// key는 알파벳으로 시작하는 영문 소문자·숫자·하이픈이며 와일드카드는 허용하지 않는다.
/// 내장 process-exit/bell/notification과 output-match:/idle-timeout: 접두어는 사용할 수 없다.
/// description은 설명이고 stability의 기본값은 stable이다.
#[derive(Debug, Clone, Deserialize)]
pub struct HookEventDecl {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub stability: EventStability,
}

/// 프리셋 편집기에서 표시할 필드. 값은 PresetSurface.params의 param_key에 저장한다.
///
/// - id: kind 안에서 고유한 영문 소문자·숫자·밑줄·하이픈, 1..=64자
/// - label_key/param_key: 비어 있지 않은 라벨 번역 키와 저장 키
/// - input_type: text/url은 텍스트 입력, file_path/dir은 파일·폴더 선택 지원. 기본 text
/// - required: 프리셋 적용에 필요한 값인지 여부. required_params도 선언하면 그 각 키가
///   required=true인 필드에 있어야 한다.
/// - placeholder_key/default: 빈 입력 안내의 번역 키와 kind 전환 시 초기 문자열
/// - derive_cwd: file_path에서만 해당 파일의 부모 경로를 cwd로 사용
#[derive(Debug, Clone, Deserialize)]
pub struct PresetFieldDecl {
    pub id: String,
    pub label_key: String,
    pub param_key: String,
    #[serde(default)]
    pub input_type: PresetFieldInputType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder_key: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub derive_cwd: bool,
}

/// 프리셋 필드의 입력 방식. 값은 문자열로 저장한다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetFieldInputType {
    /// 단순 텍스트 Input.
    #[default]
    Text,
    /// 파일 경로 — Input + 파일 선택 다이얼로그. `derive_cwd` 대상.
    FilePath,
    /// 디렉토리 경로 — Input + 폴더 선택 다이얼로그.
    Dir,
    /// URL — 단순 Input (경로 파생 제외).
    Url,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceKindDecl {
    pub kind: String,
    pub display_name_i18n_key: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// surface 렌더링 방식 — [`SurfaceKindRendering`] 참조. 기본 `Remote`.
    #[serde(default)]
    pub rendering: SurfaceKindRendering,
    /// plugin 이 권장하는 surface 기본 색. 사용자 theme TOML 의
    /// `[surfaces.<kind>]` 정의가 있으면 *그쪽이 우선*.
    /// fallback chain: 사용자 TOML > plugin default > FALLBACK_SURFACE.
    #[serde(default)]
    pub default_colors: Option<tasty_type_appearance::theme::PartialSurfaceTheme>,
    /// 화면 생성 때 필요한 파라미터 이름. 호스트가 필수 값 누락을 확인한다.
    #[serde(default)]
    pub required_params: Vec<String>,
    /// 옛 파라미터 이름을 현재 이름으로 바꿀 매핑. 예: file_path → file.
    #[serde(default)]
    pub param_aliases: HashMap<String, String>,
    /// 화면 생성 때 없는 파라미터에 넣을 기본값.
    /// 리터럴 또는 호스트가 해석할 @settings.explorer_view_mode/@home 같은 값이다.
    #[serde(default)]
    pub default_params: HashMap<String, String>,
    /// 키·IME 입력을 호스트 egui로 보낼지 여부. 기본 false이며 플러그인 mesh 입력과 구분한다.
    #[serde(default)]
    pub consumes_egui_input: bool,
    /// 확대 단축키로 이 화면의 글꼴 크기 설정을 바꿀 수 있는지 여부. 기본 false.
    #[serde(default)]
    pub zoomable: bool,
    /// 복사 단축키를 egui Event::Copy로 전달할지 여부. 기본 false.
    #[serde(default)]
    pub egui_copy: bool,
    /// 전체 선택·선택 항목 경로 복사를 이 화면이 처리할지 여부. 기본 false.
    #[serde(default)]
    pub copy_path: bool,
    /// 붙여넣기를 터미널 대신 이 화면이 처리할지 여부. 기본 false.
    #[serde(default)]
    pub egui_paste: bool,
    /// 탭 이름에 사용할 파일명이 든 파라미터 키. 없으면 kind의 표시 이름을 사용한다.
    #[serde(default)]
    pub name_from_param: Option<String>,
    /// 이 화면으로 연 파일을 최근 파일 목록에 기록할지 여부. 기본 false.
    #[serde(default)]
    pub records_recent: bool,
    /// 화면 변환 전에 convert_input_popup으로 입력을 받을지 여부.
    /// 기본 false이며 이때는 빈 파라미터로 바로 변환한다.
    #[serde(default)]
    pub convert_requires_input: bool,
    /// 변환 입력을 받을 팝업의 로컬 ID. 같은 플러그인의 popup 선언을 가리킨다.
    /// 호스트가 플러그인 ID를 붙이고 변환 대상 surface_id를 전달한다.
    /// 대상이 없으면 새 탭을 여는 용도로 쓰며, 이 필드가 없으면 입력 팝업도 없다.
    #[serde(default)]
    pub convert_input_popup: Option<String>,
    /// 프리셋 편집기에 표시할 필드. 비어 있으면 전용 입력을 표시하지 않는다.
    #[serde(default)]
    pub preset_fields: Vec<PresetFieldDecl>,
    /// webview kind의 WebView 설정 기본값. `rendering = "webview"`에서만 쓸 수 있다.
    /// 사용자 설정과 같은 저장 키의 설정 페이지 항목 기본값이 없을 때 적용한다.
    #[serde(default)]
    pub webview_defaults: Option<WebViewDefaultsDecl>,
}

/// webview kind의 설정 기본값. 키 이름은 `plugin_settings.<plugin_id>`의 저장 키와 같다.
/// 비운 항목은 호스트 기본값(zoom 100, sandbox 켬, 원격 콘텐츠 끔, follow)을 쓴다.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WebViewDefaultsDecl {
    /// 페이지 배율(%). 25..=500.
    #[serde(default)]
    pub zoom: Option<f64>,
    /// true면 JS를 끈다.
    #[serde(default)]
    pub sandbox_scripts: Option<bool>,
    #[serde(default)]
    pub allow_remote_content: Option<bool>,
    /// follow | light | dark.
    #[serde(default)]
    pub color_scheme: Option<String>,
}

/// webview 설정 zoom의 허용 범위(%). 단축키 조정도 같은 범위로 자른다.
pub const WEBVIEW_ZOOM_MIN: f64 = 25.0;
pub const WEBVIEW_ZOOM_MAX: f64 = 500.0;
/// webview 설정 color_scheme에 쓸 수 있는 값.
pub const WEBVIEW_COLOR_SCHEMES: &[&str] = &["follow", "light", "dark"];

/// surface kind의 렌더링 방식. plugin 매니페스트 `rendering = "remote" | "webview" | "egui-mesh"`.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKindRendering {
    /// 기본값. 호스트의 RemoteSurface로 등록한다.
    #[default]
    Remote,
    /// 호스트가 네이티브 WebView 영역을 관리하고 플러그인은 IPC로 URL을 전달한다.
    Webview,
    /// 플러그인이 만든 egui mesh를 호스트가 합성한다. 등록 때 API 버전과 kind 이름 충돌을 검사한다.
    /// 직렬화 이름은 하이픈을 포함한 egui-mesh다.
    #[serde(rename = "egui-mesh")]
    EguiMesh,
}

/// Plugin extension 선언. 대상(plugin_id + version_req) 하나에 대해
/// pre/post IPC/event hook을 걸 수 있다.
///
/// - `plugin_id`: 확장 대상 plugin id (정확 일치)
/// - `version_req`: 대상 버전 범위 (semver). 벗어나면 extension은 pending.
/// - `api_version`: extension 자체가 따르는 호스트 protocol 버전. `HOST_API_VERSION`과 같아야.
/// - `pre_event` / `post_event`: 대상이 publisher인 envelope의 fan-out 전/후 hook
/// - `pre_ipc` / `post_ipc`: 대상이 caller 또는 callee인 IPC 호출의 invoke 전/응답 후 hook
#[derive(Debug, Clone, Deserialize)]
pub struct ExtendsDecl {
    pub plugin_id: String,
    pub version_req: String,
    pub api_version: String,
    #[serde(default)]
    pub pre_event: Vec<EventHookDecl>,
    #[serde(default)]
    pub post_event: Vec<EventHookDecl>,
    #[serde(default)]
    pub pre_ipc: Vec<IpcHookDecl>,
    #[serde(default)]
    pub post_ipc: Vec<IpcHookDecl>,
}

/// Event hook 한 항목. `[[extends.pre_event]]` 또는 `[[extends.post_event]]`.
#[derive(Debug, Clone, Deserialize)]
pub struct EventHookDecl {
    /// 정확한 이벤트 키. 와일드카드 불가. 대상 plugin의 `events_emitted`에 선언된 키여야
    /// 한다 (실 매칭은 ExtensionRegistry 활성화 시점에 검증).
    pub event: String,
    /// transform 모드에서 변경하려는 payload 경로 일람. observe/filter는 빈 배열 가능.
    #[serde(default)]
    pub modifies: Vec<String>,
    pub mode: HookMode,
    pub timeout_ms: u32,
}

/// IPC hook 한 항목. `[[extends.pre_ipc]]` 또는 `[[extends.post_ipc]]`.
#[derive(Debug, Clone, Deserialize)]
pub struct IpcHookDecl {
    /// 정확한 IPC 메서드 이름 (예: `clipboard.add`). 대상 plugin의 IPC namespace prefix에
    /// 속해야 한다 (실 매칭은 ExtensionRegistry 활성화 시점에 검증).
    pub method: String,
    #[serde(default)]
    pub modifies: Vec<String>,
    pub mode: HookMode,
    pub timeout_ms: u32,
}

/// Hook의 동작 모드.
///
/// - Transform: 반환한 값으로 본문을 바꾼다.
/// - Filter: pass 여부로 차단하며 본문은 바꾸지 않는다.
/// - Observe: 관찰·기록용이며 결과·실패·시간 초과는 처리 결과에 반영하지 않는다.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookMode {
    Transform,
    Filter,
    Observe,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub commands: Vec<CommandDecl>,
    #[serde(default)]
    pub menu_items: Vec<MenuItemDecl>,
    #[serde(default)]
    pub ipc_namespace: Vec<IpcNamespaceDecl>,
    #[serde(default)]
    pub cli: Vec<CliCommandDecl>,
    /// 도구 메뉴에서 실행할 이벤트·화면 열기·팝업 열기 항목.
    #[serde(default)]
    pub tool: Vec<ToolContribute>,
    /// 플러그인이 선언하는 별도 창 정의.
    #[serde(default)]
    pub window: Vec<WindowContribute>,
    /// Plugin이 띄울 수 있는 popup 정의. trigger 종류에 따라 자동으로 열리거나
    /// 명시적인 IPC 호출로 열린다.
    #[serde(default)]
    pub popup: Vec<PopupContribute>,
    /// 플러그인은 배너 내용만 그린다. 배치·쌓기·닫기는 호스트가 맡으며 IPC로 연다.
    #[serde(default)]
    pub banner: Vec<BannerContribute>,
    /// 새 감지기 또는 기존 감지기에 추가할 규칙과 메타데이터.
    /// 호스트 타입에 의존하지 않도록 JSON으로 보관하고 설치 때 구체 형식을 검사한다.
    #[serde(default)]
    pub detector: Vec<serde_json::Value>,
    /// 파일 핸들러 선언. JSON으로 보관하고 허용 액션과 참조는 호스트가 설치 때 검사한다.
    #[serde(default)]
    pub handler: Vec<serde_json::Value>,
    /// IpcSequence 훅 핸들러 선언. hook_handler.define 권한이 필요하다.
    /// JSON으로 보관하고 호스트가 설치 때 해석·검증하며 ShellCommand는 허용하지 않는다.
    #[serde(default)]
    pub hook_handler: Vec<serde_json::Value>,
    /// poll/push 완료 전략 선언. completion_strategy.define 권한이 필요하다.
    /// JSON으로 보관하고 호스트가 설치 때 구체 전략과 참조를 검사한다.
    #[serde(default)]
    pub completion_strategy: Vec<serde_json::Value>,
    /// 호스트 설정 화면에 추가할 페이지와 항목.
    #[serde(default)]
    pub settings_pages: Vec<SettingsPageContribute>,
    /// hook.set/surface.fire_hook 검증에 사용할 이벤트 이름 목록.
    #[serde(default)]
    pub hook_events: Vec<HookEventDecl>,
    /// Tasty가 띄우는 자식(터미널 셸·훅·작업 자식)에게 넘기지 않을 상속 환경변수.
    /// 설치·활성 상태만 따르며 프로세스 실행 여부와 무관하게 적용한다.
    #[serde(default)]
    pub strip_inherited_env: StripInheritedEnvDecl,
    /// tasty CLI를 실행한 호출자 대화를 알아보는 규칙. 설치된 모든 플러그인의 선언을
    /// 모든 플러그인 명령에 적용하며 형식 오류 거절은 엄격 대상 명령에만 한다.
    #[serde(default)]
    pub caller_context: Vec<crate::caller_context::CallerContextDecl>,
    /// 이 플러그인 명령이 형식 오류를 거절할 다른 플러그인의 호출자 provider.
    /// 자기가 선언한 provider는 적지 않아도 엄격 대상이다.
    #[serde(default)]
    pub requires_caller_context: Vec<String>,
    /// attach holder 가 서버에서 실행할 수 있는 메서드와 client 쪽 결과 수신 메서드.
    #[serde(default)]
    pub remote_attach: crate::remote_attach::RemoteAttachDecl,
}

/// 상속 환경변수 제거 선언. `TASTY_`로 시작하는 이름·접두사는 본체 소유라 선언할 수 없다.
///
/// - names: 이름이 같으면 지운다.
/// - prefixes: 이름이 접두사로 시작하면 지운다.
/// - values: 이름이 같고 값이 접두사 중 하나로 시작할 때만 지운다. 사용자가 직접 넣는
///   값과 이름을 공유하는 변수에 쓴다.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct StripInheritedEnvDecl {
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub values: Vec<EnvValuePrefixDecl>,
}

impl StripInheritedEnvDecl {
    /// 자식 생성 경로가 판정에 쓰는 규칙 형식으로 바꾼다.
    pub fn to_rules(&self) -> tasty_utils::process::InheritedEnvRules {
        tasty_utils::process::InheritedEnvRules {
            names: self.names.clone(),
            prefixes: self.prefixes.clone(),
            value_prefixes: self
                .values
                .iter()
                .map(|v| (v.name.clone(), v.prefixes.clone()))
                .collect(),
        }
    }

    /// 제거할 환경변수 이름·접두사의 형식과 본체 소유 TASTY_ 접두사를 검사한다.
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        let value_names = self.values.iter().map(|v| &v.name);
        for (what, raw) in self
            .names
            .iter()
            .map(|n| ("name", n))
            .chain(self.prefixes.iter().map(|p| ("prefix", p)))
            .chain(value_names.map(|n| ("values.name", n)))
        {
            if !crate::validators::is_valid_env_name(raw) {
                anyhow::bail!(
                    "invalid contributes.strip_inherited_env {what} '{raw}': must be ascii letters, \
                     digits and '_', not starting with a digit"
                );
            }
            if raw.to_ascii_uppercase().starts_with("TASTY_") {
                anyhow::bail!(
                    "contributes.strip_inherited_env {what} '{raw}' targets TASTY_* — \
                     the host owns its own environment"
                );
            }
        }
        for value in &self.values {
            if value.prefixes.is_empty() || value.prefixes.iter().any(|p| p.is_empty()) {
                anyhow::bail!(
                    "contributes.strip_inherited_env values '{}' needs non-empty value prefixes",
                    value.name
                );
            }
        }
        Ok(())
    }
}

/// 값 접두사로 판정하는 제거 규칙 한 항목.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct EnvValuePrefixDecl {
    pub name: String,
    pub prefixes: Vec<String>,
}

/// 설정 페이지. 호스트가 플러그인 목록에서 읽어 카테고리별로 표시한다.
/// id는 플러그인 안에서 고유하며 전역에서는 플러그인 ID를 붙인다.
/// title_key는 제목 번역 키, category는 표시할 카테고리, items는 입력 항목이다.
#[derive(Debug, Clone, Deserialize)]
pub struct SettingsPageContribute {
    pub id: String,
    pub title_key: String,
    pub category: SettingsCategory,
    #[serde(default)]
    pub items: Vec<SettingsItemDecl>,
}

/// 설정 모달의 상위 카테고리. host 가 받아들이는 값만 enumerate.
/// 알 수 없는 카테고리는 `Other(name)` 로 보존 (host 측에서 무시 또는 fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCategory {
    Appearance,
    General,
    Keybindings,
    Plugin,
    Other(String),
}

impl<'de> Deserialize<'de> for SettingsCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "appearance" => SettingsCategory::Appearance,
            "general" => SettingsCategory::General,
            "keybindings" => SettingsCategory::Keybindings,
            "plugin" => SettingsCategory::Plugin,
            _ => SettingsCategory::Other(s),
        })
    }
}

/// 설정 항목. kind로 폰트·토글·선택·수치 입력과 묶음 머리를 구분한다.
/// 각 항목은 ID·라벨 번역 키를 갖고, 묶음 머리를 뺀 항목은 저장 키도 갖는다.
/// 토글·선택·수치 항목은 `group` 으로 같은 페이지의 묶음에 들어간다.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsItemDecl {
    /// 폰트 override 항목. host 는 `plugin_font_overrides.<storage_key>` 슬롯에
    /// FontOverride 값을 read/write 하는 generic UI 를 그린다.
    ///
    /// - `id`: page 내 항목 식별자 (소문자/숫자 + `_` + `-`).
    /// - `label_key`: 항목 라벨 i18n 키.
    /// - `storage_key`: host 측 settings 의 `plugin_font_overrides.<storage_key>` slot.
    FontOverride {
        id: String,
        label_key: String,
        storage_key: String,
    },

    /// on/off 토글. host 는 디자인 Switch 로 그리고 `plugin_settings.<plugin_id>.<storage_key>`
    /// 에 bool 로 저장한다. `default` 는 키 부재 시 적용할 초기값.
    Toggle {
        id: String,
        label_key: String,
        storage_key: String,
        #[serde(default)]
        default: bool,
        /// 들어갈 묶음(`kind = "group"` 항목의 `id`). 없으면 묶음 밖이다.
        #[serde(default)]
        group: Option<String>,
    },

    /// 선택지(드롭다운). host 는 디자인 Select 로 그리고 선택된 `value` 를 문자열로 저장한다.
    /// `default` 는 `options` 의 `value` 중 하나여야 한다(검증). 옵션 라벨은 `label_key` i18n.
    Select {
        id: String,
        label_key: String,
        storage_key: String,
        options: Vec<SelectOptionDecl>,
        default: String,
        /// 들어갈 묶음(`kind = "group"` 항목의 `id`). 없으면 묶음 밖이다.
        #[serde(default)]
        group: Option<String>,
    },

    /// 수치 입력. host 는 Input(+선택적 suffix) 으로 그리고 f64 로 저장한다(정수 표기 가능).
    /// `min`/`max` 가 주어지면 clamp 범위이며 `default` 는 그 범위 안이어야 한다(검증).
    Number {
        id: String,
        label_key: String,
        storage_key: String,
        #[serde(default)]
        default: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        /// 값 뒤에 붙일 단위 라벨 i18n 키 (예 `"%"`). 없으면 suffix 미표시.
        #[serde(default)]
        suffix_key: Option<String>,
        /// 들어갈 묶음(`kind = "group"` 항목의 `id`). 없으면 묶음 밖이다.
        #[serde(default)]
        group: Option<String>,
    },

    /// 읽기 전용 글. host 는 고칠 수 없는 글(선택·복사는 된다)로 그린다. plugin 이 쓰는 값을 보여 주기만 한다.
    ///
    /// - `storage_key` 의 저장값이 문자열이면 그것을 보인다. 없으면 `default_key`(i18n) 를, 그것도 없으면
    ///   `default` 를 보인다. `default_key` · `default` 중 하나는 있어야 한다(검증).
    /// - host 는 이 칸에 쓰지 않는다.
    Text {
        id: String,
        label_key: String,
        #[serde(default)]
        storage_key: Option<String>,
        #[serde(default)]
        default: String,
        #[serde(default)]
        default_key: Option<String>,
        /// 들어갈 묶음(`kind = "group"` 항목의 `id`). 없으면 묶음 밖이다.
        #[serde(default)]
        group: Option<String>,
    },

    /// 묶음 머리. host 는 머리 줄을 그리고 그 아래에 `group = "<id>"` 인 항목을 모아 그린다.
    ///
    /// - `storage_key` 가 있으면 머리에 Switch 를 두고 `plugin_settings.<plugin_id>.<storage_key>` 에
    ///   bool 로 저장한다(`toggle` 과 같은 칸). 끄면 묶음의 항목을 숨긴다 — 그 기능을 끄는 스위치다.
    ///   plugin 은 이 값을 읽어 기능을 끈다.
    /// - `storage_key` 가 없으면 스위치 없는 머리이고 항목은 늘 보인다. 이때는 항목이 하나 이상 있어야 한다.
    /// - `default` 는 키 부재 시 스위치 값이다(기본 켬).
    Group {
        id: String,
        label_key: String,
        #[serde(default)]
        storage_key: Option<String>,
        #[serde(default = "group_default_on")]
        default: bool,
    },
}

fn group_default_on() -> bool {
    true
}

/// 설정 페이지를 그리는 순서의 한 칸 — [`SettingsPageContribute::entries`].
#[derive(Debug, Clone)]
pub enum SettingsEntry<'a> {
    /// 묶음 밖의 항목.
    Item(&'a SettingsItemDecl),
    /// 묶음 머리(`SettingsItemDecl::Group`)와 그 항목들(선언 순서).
    Group {
        header: &'a SettingsItemDecl,
        members: Vec<&'a SettingsItemDecl>,
    },
}

impl SettingsPageContribute {
    /// 그리는 순서. 묶음 밖 항목과 묶음 머리는 선언 순서대로 놓고, 묶음의 항목은 그 머리 바로
    /// 뒤에 선언 순서대로 모은다. 묶음을 가리키는 항목의 선언 위치는 순서에 영향을 주지 않는다.
    pub fn entries(&self) -> Vec<SettingsEntry<'_>> {
        self.items
            .iter()
            .filter_map(|item| match item {
                SettingsItemDecl::Group { id, .. } => Some(SettingsEntry::Group {
                    header: item,
                    members: self
                        .items
                        .iter()
                        .filter(|m| m.group() == Some(id.as_str()))
                        .collect(),
                }),
                _ if item.group().is_none() => Some(SettingsEntry::Item(item)),
                _ => None,
            })
            .collect()
    }
}

/// `Select` 항목의 한 선택지 — 저장될 `value` 와 표시용 `label_key`(i18n).
#[derive(Debug, Clone, Deserialize)]
pub struct SelectOptionDecl {
    pub value: String,
    pub label_key: String,
}

impl SettingsItemDecl {
    /// 공통 형식 검사에 쓸 ID·라벨 번역 키·저장 키를 반환한다. 스위치 없는 묶음 머리와 저장 키 없는
    /// 읽기 전용 글은 저장 키가 없다.
    pub(crate) fn common(&self) -> (&str, &str, Option<&str>) {
        match self {
            SettingsItemDecl::FontOverride {
                id,
                label_key,
                storage_key,
            }
            | SettingsItemDecl::Toggle {
                id,
                label_key,
                storage_key,
                ..
            }
            | SettingsItemDecl::Select {
                id,
                label_key,
                storage_key,
                ..
            }
            | SettingsItemDecl::Number {
                id,
                label_key,
                storage_key,
                ..
            } => (id, label_key, Some(storage_key)),
            SettingsItemDecl::Group {
                id,
                label_key,
                storage_key,
                ..
            }
            | SettingsItemDecl::Text {
                id,
                label_key,
                storage_key,
                ..
            } => (id, label_key, storage_key.as_deref()),
        }
    }

    /// 이 항목이 들어간 묶음의 `id`. 묶음 밖이거나 묶음에 못 드는 종류(글꼴 · 묶음 머리)면 `None`.
    pub fn group(&self) -> Option<&str> {
        match self {
            SettingsItemDecl::Toggle { group, .. }
            | SettingsItemDecl::Select { group, .. }
            | SettingsItemDecl::Number { group, .. }
            | SettingsItemDecl::Text { group, .. } => group.as_deref(),
            SettingsItemDecl::FontOverride { .. } | SettingsItemDecl::Group { .. } => None,
        }
    }
}

/// 별도 창 선언. ID는 플러그인 안에서 고유한 영문 소문자·숫자·밑줄이다.
/// display_name_i18n_key는 표시 이름, icon은 선택적 아이콘 이름이다.
/// default_size는 선택적 LogicalPx 크기이며 multi_instance의 기본값은 false다.
#[derive(Debug, Clone, Deserialize)]
pub struct WindowContribute {
    pub id: String,
    pub display_name_i18n_key: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub default_size: Option<WindowSizeHint>,
    #[serde(default)]
    pub multi_instance: bool,
}

/// `WindowContribute.default_size` 의 LogicalPx 권장 크기.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct WindowSizeHint {
    pub width: u32,
    pub height: u32,
}

/// Plugin이 contribute하는 popup의 정의.
///
/// - `id`: plugin 내 고유 (소문자+숫자+`-`, 글자로 시작, 길이 ≤ 64).
///   호스트는 `<plugin_id>/<popup_id>`로 전역 식별.
/// - `trigger`: 어떤 조건으로 popup을 여는지. `event` 또는 `ipc`.
/// - `size_hint`: 옵션. 호스트가 LogicalPx 단위로 popup 크기에 적용.
/// - `anchor`: 옵션. 위치 정책. 기본 `screen-center`. 기준 사각형은 `scope` 의 경계다.
/// - `scope`: 옵션. popup 의 소속 범위(가시성 + 경계 clamp). 기본 `window`.
/// - `dismiss_on_outside_click`: 옵션. 기본 true.
#[derive(Debug, Clone, Deserialize)]
pub struct PopupContribute {
    pub id: String,
    pub trigger: PopupTrigger,
    #[serde(default)]
    pub size_hint: Option<PopupSizeHint>,
    #[serde(default = "default_popup_anchor")]
    pub anchor: PopupAnchor,
    /// 소속 범위. 생략하면 창 범위로 표시·배치한다.
    #[serde(default)]
    pub scope: PopupScopeDecl,
    #[serde(default = "default_dismiss_on_outside_click")]
    pub dismiss_on_outside_click: bool,
    /// 플러그인은 egui-mesh로 내용만 그린다. 배경·테두리·닫기는 호스트가 맡는다.
    #[serde(default)]
    pub rendering: PopupRendering,
}

/// 팝업은 egui-mesh로만 그린다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PopupRendering {
    /// 번들 허용 목록을 확인한 뒤 플러그인의 egui mesh를 합성한다.
    #[default]
    #[serde(rename = "egui-mesh")]
    EguiMesh,
}

fn default_dismiss_on_outside_click() -> bool {
    true
}

fn default_popup_anchor() -> PopupAnchor {
    PopupAnchor::ScreenCenter
}

/// 팝업을 열 조건.
/// event는 발생한 이벤트로 열며 해당 키를 구독·발행 선언에 포함해야 한다.
/// ipc는 popup.open 호출로 연다.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PopupTrigger {
    Event { event_key: String },
    Ipc,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PopupSizeHint {
    pub width: u32,
    pub height: u32,
}

/// 매니페스트는 범위 종류만 선언하고 호스트가 열 때 실제 대상 화면을 정한다.
/// 대상이 없으면 창 범위를 사용한다. docs/design/systems/popup.md#plugin-popup-의-스코프 참고.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupScopeDecl {
    /// 항상 보이고 창 전체에 clamp 된다.
    #[default]
    Window,
    /// 여는 진입점이 지목한 surface 가 보일 때만 그려지고 그 surface 영역에 clamp 된다.
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupAnchor {
    ScreenCenter,
    ActiveSurfaceCenter,
    Cursor,
}

/// 배너 선언. 플러그인은 내용만 그리고 배치·쌓기·닫기는 호스트가 맡는다.
///
/// - id: 플러그인 안에서 고유하며 전역에서는 플러그인 ID를 붙인다.
/// - trigger: IPC 호출로 연다.
/// - scope: 플러그인 소유 surface만 허용한다.
/// - ttl_seconds: 있으면 시간이 지난 뒤 닫고, 없으면 사용자가 닫을 때까지 유지한다.
///   호스트는 두 경우 모두 닫기 버튼을 표시한다.
/// - size_hint: LogicalPx 높이 힌트. 너비는 대상 영역에 맞춘다.
#[derive(Debug, Clone, Deserialize)]
pub struct BannerContribute {
    pub id: String,
    pub trigger: BannerTrigger,
    #[serde(default = "default_banner_scope")]
    pub scope: BannerScopeDecl,
    #[serde(default)]
    pub ttl_seconds: Option<u32>,
    #[serde(default)]
    pub size_hint: Option<BannerSizeHint>,
    /// 배너는 egui-mesh로 그린다.
    #[serde(default)]
    pub rendering: BannerRendering,
}

/// 배너 렌더링 방식. egui-mesh만 지원한다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BannerRendering {
    /// 플러그인의 egui mesh를 호스트가 합성한다. 열 때 플러그인의 API 버전을 검사한다.
    #[default]
    #[serde(rename = "egui-mesh")]
    EguiMesh,
}

/// 배너는 banner.open IPC로 연다.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BannerTrigger {
    /// plugin 이 host IPC `banner.open` 을 호출해 명시적으로 연다.
    Ipc,
}

/// 플러그인 배너는 자기 surface에만 놓을 수 있다. 다른 범위는 호스트 전용이다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BannerScopeDecl {
    #[default]
    Surface,
}

/// banner 높이 힌트(LogicalPx). 너비는 스코프 폭 도킹이라 높이만 둔다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct BannerSizeHint {
    pub height: u32,
}

fn default_banner_scope() -> BannerScopeDecl {
    BannerScopeDecl::Surface
}

/// 사이드바 도구 메뉴 항목. plugin이 자기 동작을 사용자 진입점으로 노출하는 방식.
///
/// - `id`: plugin 내 고유. 호스트는 `<plugin_id>/<tool_id>`로 전역 식별.
/// - `label_i18n_key`: 라벨. `t()` 카탈로그에 키가 없으면 원본 문자열 fallback.
/// - `icon`: 옵션. 아이콘 이름 (호스트 catalog 또는 plugin 패키지 내 SVG path).
/// - `action`: 클릭 시 수행할 동작.
/// - `order_hint`: 작을수록 위. 호스트 내장은 0..=99, plugin은 100 이상 권장. 기본 100.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolContribute {
    pub id: String,
    pub label_i18n_key: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub action: ToolAction,
    #[serde(default = "default_tool_order_hint")]
    pub order_hint: i32,
}

fn default_tool_order_hint() -> i32 {
    100
}

/// `[[contributes.tool]]` 클릭 시 수행되는 동작.
///
/// - event: 호스트가 {tool_id: <plugin_id>/<tool_id>}를 본문으로 이벤트를 발행한다.
/// - open_surface: 플러그인이 선언한 kind의 화면을 연다.
/// - open_popup: 플러그인이 선언한 팝업을 연다.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolAction {
    Event { event_key: String },
    OpenSurface { surface_kind: String },
    OpenPopup { popup_id: String },
}

/// Plugin이 점유할 IPC 메서드 namespace prefix.
///
/// 호스트는 `<prefix>.*` 패턴의 모든 IPC 메서드를 등록된 plugin에 forward한다.
/// 예: prefix="codex" → "codex.spawn", "codex.wait" 등을 모두 그 plugin이 처리.
#[derive(Debug, Clone, Deserialize)]
pub struct IpcNamespaceDecl {
    pub prefix: String,
    #[serde(default)]
    pub description_i18n_key: Option<String>,
}

/// Plugin이 contributes하는 최상위 CLI 명령. `tasty <name> <sub>` 형태로 노출된다.
#[derive(Debug, Clone, Deserialize)]
pub struct CliCommandDecl {
    pub name: String,
    /// CLI help용 한 줄 설명. plain text는 plugin manager 없이 동작하는 CLI 클라이언트
    /// 진입 경로(`tasty <plugin> --help`)에서도 곧장 사용할 수 있다.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub description_i18n_key: Option<String>,
    /// 루트 도움말에서 이 명령을 표시할 그룹. `basic` · `agents` · `background` ·
    /// `automation` · `files` · `settings` 중 하나다. 생략하거나 모르는 값이면 plugin 그룹에
    /// 표시한다. 도움말 표시에만 쓰이며 실행·권한에는 영향이 없다.
    #[serde(default)]
    pub help_group: Option<String>,
    #[serde(default)]
    pub subcommands: Vec<CliSubcommandDecl>,
    /// arg group 이름 → 정의. subcommand가 `args = "<key>"`로 참조한다.
    #[serde(default)]
    pub arg_groups: HashMap<String, CliArgGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CliSubcommandDecl {
    pub name: String,
    /// 이 서브커맨드가 호출할 IPC 메서드 (예: "codex.spawn").
    /// plugin 자기 namespace prefix로 시작해야 한다.
    pub ipc_method: String,
    /// `arg_groups`의 키. 비어있는 그룹이라도 명시적으로 가리켜야 한다.
    pub args: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub description_i18n_key: Option<String>,
    /// stdin이 TTY가 아니면 JSON을 읽어 생략한 CLI 인자를 채운다.
    /// stdin_field를 키로 사용하고 없으면 인자 이름을 쓴다.
    #[serde(default)]
    pub stdin_json: bool,
    /// 훅·도우미가 호출하는 내부 연동 명령. 도움말에서 마지막 "내부 연동" 그룹에 표시한다.
    /// 숨기거나 실행을 막지 않는다. 생략하면 일반 명령이다.
    #[serde(default)]
    pub internal: bool,
    /// 상태가 terminal_states에 들거나 시간 제한에 도달할 때까지 IPC를 반복 호출한다.
    /// 시간 제한에 도달하면 마지막 응답을 출력한다.
    #[serde(default)]
    pub polling: Option<PollingDecl>,
    /// 첫 응답을 출력한 뒤 별도 메서드를 폴링한다. polling과 동시에 선언할 수 없다.
    #[serde(default)]
    pub auto_wait: Option<AutoWaitDecl>,
    /// 호스트 CLI 가 응답을 찍는 방식. 생략하면 JSON 이다. polling·auto_wait 와 함께 선언할 수 없다.
    #[serde(default)]
    pub output: Option<CliOutputDecl>,
}

/// 호스트 CLI 가 플러그인 응답을 찍는 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliOutputDecl {
    /// `terminal.children` 항목의 목록을 부모 ID 표로 찍는다. `--format json` 이면 JSON 이다.
    ChildTable,
}

/// 첫 응답 뒤 실행할 추가 대기. --no-wait가 아니면 지정한 메서드를 폴링하고
/// 마지막 응답을 두 번째 JSON 줄로 출력한다.
#[derive(Debug, Clone, Deserialize)]
pub struct AutoWaitDecl {
    /// chain 호출할 IPC method (예: `"claude.wait"`, `"claude.wait_by_surface"`).
    pub method: String,
    /// 1 차 응답 JSON 의 어떤 키를 wait params 의 어떤 키로 매핑할지.
    /// 키: 1 차 응답 키, 값: wait params 키.
    /// 예: `{ "child_index": "child_index", "parent_surface_id": "surface" }`.
    #[serde(default)]
    pub map_from_response: std::collections::HashMap<String, String>,
    /// 1 차 응답에 없는 wait params 를 *원 요청* 의 어떤 params 키에서 가져올지.
    /// 키: 원 요청 params 키, 값: wait params 키.
    /// `tell` 처럼 응답에 child_index 가 없는 경우 fallback 매핑에 사용.
    #[serde(default)]
    pub map_from_request: std::collections::HashMap<String, String>,
    /// wait 폴링 사양 (`PollingDecl` 과 동일 모양 — state_field / terminal_states /
    /// interval_ms). `timeout_field` 는 본 `AutoWaitDecl` 의 `timeout_field` 가
    /// 우선이므로 무시된다. `strategy` 와 동시 선언 금지, 정확히 하나만 선언해야
    /// 한다(validator 강제, `validate_cli_subcommands`).
    #[serde(default)]
    pub polling: Option<PollingDecl>,
    /// 이름으로 등록된 completion strategy 참조. 형식 `<plugin_id>/<short-name>`
    /// (`HookHandlerId` 와 동일 관례). `polling` 과 동시 선언 금지, prefix 는 반드시
    /// 자기 자신의 plugin id 와 일치해야 한다(validator 강제) — 다른 plugin 의
    /// strategy 를 참조할 수 없다. CLI 는 같은 매니페스트 안에서만 이름을 해석한다.
    #[serde(default)]
    pub strategy: Option<String>,
    /// `--no-wait` flag 의 CLI arg name. CLI 가 이 키를 true 로 받으면 chain skip.
    /// 기본 `"no_wait"`.
    #[serde(default = "default_no_wait_field")]
    pub no_wait_field: String,
    /// `--timeout` flag 의 CLI arg name. wait 폴링의 timeout 으로 사용.
    /// 비어있으면 무한 대기. 기본 `"timeout"`.
    #[serde(default = "default_timeout_field")]
    pub timeout_field: String,
}

fn default_no_wait_field() -> String {
    "no_wait".into()
}

fn default_timeout_field() -> String {
    "timeout".into()
}

/// CLI 명령이 *blocking polling* 모드일 때의 설정.
#[derive(Debug, Clone, Deserialize)]
pub struct PollingDecl {
    /// IPC 응답 JSON 의 어떤 필드를 보고 terminal 판정할지. 보통 `"state"`.
    pub state_field: String,
    /// 이 값들 중 하나에 도달하면 polling 종료. 예: `["idle", "needs_input", "exited"]`.
    pub terminal_states: Vec<String>,
    /// polling 간격 (밀리초). 기본 500ms.
    #[serde(default = "default_polling_interval_ms")]
    pub interval_ms: u64,
    /// `--timeout` 플래그 이름. CLI args 의 이 필드 (u32) 가 초 단위 timeout.
    /// 비어 있으면 무한 대기 (CLI process 가 죽을 때까지).
    #[serde(default)]
    pub timeout_field: Option<String>,
}

fn default_polling_interval_ms() -> u64 {
    500
}

/// 이름으로 참조할 공용 완료 전략. CLI 폴링 설정과 호스트의 작업 폴링 설정으로 변환한다.
/// contributes의 JSON 선언은 호스트 레지스트리가 따로 해석한다.
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionStrategyDecl {
    /// 폴링 시 호출할 IPC method.
    pub poll_method: String,
    /// dispatch 응답 키 → poll params 키 매핑.
    #[serde(default)]
    pub map_from_response: HashMap<String, String>,
    /// 원 dispatch params 키 → poll params 키 매핑.
    #[serde(default)]
    pub map_from_request: HashMap<String, String>,
    /// 응답에서 상태를 읽을 필드명.
    pub state_field: String,
    /// terminal 로 간주할 상태값 목록.
    pub terminal_states: Vec<String>,
    /// 작업 실패로 처리할 상태. 생략하면 빈 목록이다.
    #[serde(default)]
    pub failure_states: Vec<String>,
    /// 폴링 간격 (밀리초). 기본 500ms.
    #[serde(default = "default_polling_interval_ms")]
    pub interval_ms: u64,
    /// 전체 폴링 timeout (밀리초). `None` 이면 무한 대기.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl CompletionStrategyDecl {
    /// CLI 폴링 설정으로 변환한다. 이 변환에서는 timeout_ms와 failure_states를 버린다.
    /// timeout_field도 None이라 CLI 인자로 대기 제한을 바꿀 수 없다.
    /// 작업 러너의 성공·실패 판정과 CLI 폴링의 동작을 구분한다.
    pub fn to_polling_decl(&self) -> PollingDecl {
        PollingDecl {
            state_field: self.state_field.clone(),
            terminal_states: self.terminal_states.clone(),
            interval_ms: self.interval_ms,
            timeout_field: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CliArgGroup {
    #[serde(default)]
    pub positional: Vec<CliArg>,
    #[serde(default)]
    pub flags: Vec<CliArg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CliArg {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: CliArgType,
    /// `flags`에 들어가는 인자에만 존재. `positional`에서는 None.
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<toml::Value>,
    /// 한 줄 도움말. clap의 `Arg::help`에 그대로 전달된다.
    #[serde(default)]
    pub help: Option<String>,
    /// Optional plugin catalog key; missing translations retain `help`.
    #[serde(default)]
    pub help_i18n_key: Option<String>,
    /// subcommand 의 `stdin_json = true` 일 때, stdin JSON 의 어느 키에서
    /// 이 인자의 fallback 값을 가져올지. 없으면 `name` 을 그대로 키로 쓴다.
    /// 예: Claude Code hook payload 의 `session_id` 를 `--session` 인자에
    /// 매핑하려면 `stdin_field = "session_id"`.
    #[serde(default)]
    pub stdin_field: Option<String>,
    /// directory/file 경로 인자를 호출자 cwd 기준 절대 경로로 바꾸고 존재·종류를 검사한다.
    /// `url_or_file` 은 `scheme://` 로 시작하는 값을 그대로 두고 나머지를 file 처럼 다룬다.
    /// 파일 내용은 검사하지 않으며 플러그인에서 별도로 확인해야 한다.
    #[serde(default)]
    pub path_kind: Option<String>,
    /// 같은 인자를 반복 지정하면 마지막 값으로 덮지 않고 오류로 거부한다. 기본 false.
    #[serde(default)]
    pub reject_repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliArgType {
    U32,
    I64,
    String,
    Bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandDecl {
    pub id: String,
    pub title_i18n_key: String,
    #[serde(default)]
    pub default_keybinding: Option<String>,
    /// 단축키를 호스트의 의미론적 액션과 어떻게 묶을지 plugin 작성자가 선언한다.
    /// `"independent"` (기본) 또는 `"inherit:<host_action>"`.
    #[serde(default)]
    pub binding_mode: BindingMode,
    /// 단축키 적용 범위. 기본 global이며 surface는 소유 플러그인의 화면에 포커스가 있어야 한다.
    #[serde(default)]
    pub scope: CommandScope,
    /// 선언한 액션은 호스트가 직접 처리하며 command.invoke는 호출하지 않는다.
    /// 액션이 없으면 command.invoke를 사용한다. command.invoked 이벤트는 별도로 보낸다.
    #[serde(default)]
    pub action: Option<ToolAction>,
}

/// 단축키 범위. global은 전역이며 surface는 소유 플러그인의 화면에 포커스가 있어야 한다.
/// global 단축키에는 조합키를 사용하고 surface에서는 단일 키도 허용한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CommandScope {
    #[default]
    Global,
    Surface,
}

/// command가 호스트 액션 키와 어떤 관계를 갖는지.
///
/// - `Independent`: plugin 자체 키. 사용자가 설정에서 자유롭게 변경 가능.
/// - `InheritHost(action)`: 호스트의 의미론적 액션(예: `"clipboard.copy"`)
///   키 설정을 그대로 따라감. 사용자가 설정 UI에서 떼어내 독립 키로 만들 수 있다.
///
/// TOML 표기: `binding_mode = "independent"` 또는
/// `binding_mode = "inherit:clipboard.copy"`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BindingMode {
    #[default]
    Independent,
    InheritHost(String),
}

impl<'de> Deserialize<'de> for BindingMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "independent" {
            return Ok(BindingMode::Independent);
        }
        if let Some(action) = s.strip_prefix("inherit:") {
            let action = action.trim();
            if action.is_empty() {
                return Err(serde::de::Error::custom(
                    "binding_mode 'inherit:' must be followed by a host action id",
                ));
            }
            return Ok(BindingMode::InheritHost(action.to_string()));
        }
        Err(serde::de::Error::custom(format!(
            "invalid binding_mode '{}': expected 'independent' or 'inherit:<host_action>'",
            s
        )))
    }
}

/// 입력과 같은 independent 또는 inherit:<action> 형식으로 직렬화한다.
impl Serialize for BindingMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            BindingMode::Independent => serializer.serialize_str("independent"),
            BindingMode::InheritHost(action) => {
                serializer.serialize_str(&format!("inherit:{action}"))
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MenuItemDecl {
    pub menu: String,
    pub command: String,
    #[serde(default)]
    pub when: Option<String>,
}

/// 권한 토큰에 쓸 감지기 ID 형식. 1..=64자 영문 소문자·숫자·하이픈과 선택적 $ 접두어를 허용한다.
fn is_valid_detector_id_local(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    if let Some(rest) = s.strip_prefix('$') {
        if rest.is_empty() {
            return false;
        }
        return rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
