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
    /// Event Bus 발화 허용 패턴 일람. plugin이 자기 namespace로 publish할 때 필요.
    /// 예약 네임스페이스(`surface.*`, `system.*` 등)는 호스트만 publish 가능 — plugin이
    /// 적어도 매니페스트 검증 단계에서 거부된다.
    #[serde(default)]
    pub event_publish: Vec<String>,
    /// plugin이 publish하는 이벤트 카탈로그. 검토 항목 #6 — 1.0에 포함.
    /// `event_publish` 권한 패턴이 *허용 범위*라면, `events_emitted`는 그 안에서
    /// 실제로 어떤 정확 키를 발화하는지 plugin이 사전 선언하는 *카탈로그*다.
    /// 외부 tool(`tasty plugin show`)이 어떤 이벤트가 나오는지 확인할 수 있게 하고,
    /// extension plugin이 hook 대상으로 참조 가능하게 한다.
    #[serde(default, rename = "events_emitted")]
    pub events_emitted: Vec<EventEmittedDecl>,
    #[serde(default)]
    pub contributes: Contributes,
    /// Extension 선언. 존재 시 이 plugin은 다른 plugin의 IPC/이벤트 흐름을 가로채는
    /// 확장 plugin이 된다. 일반 contribute(`commands`, `surface_kinds`, …)와 공존 가능.
    /// 한 plugin은 정확히 하나의 target만 지정 가능 (1.0 제약).
    #[serde(default)]
    pub extends: Option<ExtendsDecl>,
    /// plugin이 동봉한 lang 파일 디렉터리 (매니페스트 디렉터리 기준 상대).
    /// 기본 `"lang"`. 호스트는 `<plugin_dir>/<lang_dir>/<locale>.toml`을 i18n
    /// registry에 머지한다.
    #[serde(default = "default_lang_dir")]
    pub lang_dir: String,
    /// 배포 패키징(DMG/AppImage/MSIX) 스테이징 포함 여부. 기본 `true`.
    /// `false`면 dist 빌드 스크립트(`scripts/build-*.{sh,ps1}`)가 이 plugin을
    /// 번들에서 제외한다 — dev 빌드(`just build-plugins`/`link-plugins`)는 영향
    /// 없이 그대로 스테이징하므로 데모/PoC plugin을 로컬에선 쓰되 출하판엔 빼는
    /// 용도. 스크립트는 매니페스트를 직접 파싱하므로 이 필드는 스키마 문서화 +
    /// 오타 방지(정확한 키명 고정)를 위해 존재한다.
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
    // WASM entry는 0.7 이후 재검토. 보류 이유는 docs/dev-guide/plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류
    // §1 참조 (강제 가능한 sandbox 가치 vs. 1.0 전 보안/도구체인 비용).
}

/// Plugin이 매니페스트에 선언할 수 있는 권한 카테고리.
///
/// 평면 enum — `fs.write`는 `fs.read`를 자동 포함하지 않는다.
/// 매니페스트에 두 권한이 모두 필요하면 명시적으로 선언해야 한다.
///
/// `IpcInvoke(prefix)`는 동적 토큰을 보유하므로 `Copy`를 derive할 수 없다.
/// 정적 enum variant도 함께 `Clone`만 derive하여 일관성을 유지한다.
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
    /// Approval namespace 접근 (`approval.*`). 휴먼 결정 게이트를 요청·응답·조회.
    /// 토큰 `approval`.
    Approval,
    /// Telemetry namespace 접근 (`telemetry.*`). 메트릭 기록·조회, cap/anomaly 관리.
    /// 토큰 `telemetry`.
    Telemetry,
    /// Agent namespace 접근 (`agent.*`). Task/Barrier/Semaphore/Lease/Reducer/RateLimit
    /// 협업 primitive 사용. 토큰 `agent`.
    AgentManage,
    /// 다른 plugin이 점유한 IPC namespace prefix의 메서드 호출.
    /// 토큰 형식: `ipc.invoke:<prefix>` (예: `ipc.invoke:codex`).
    IpcInvoke(String),
    /// 다른 plugin(target)의 IPC/이벤트 흐름을 가로채는 extension 권한.
    /// 토큰 형식: `ext:<target_plugin_id>` (예: `ext:com.tasty.clipboard`).
    ///
    /// `[extends]` 블록을 선언한 plugin은 매니페스트의 `permissions`에 반드시 이 토큰을
    /// 포함해야 한다. 사용자가 grant하기 전까지 extension은 `Pending(PermissionNotGranted)`
    /// 상태로 유지된다. 세부 mode(transform/filter/observe)와 hook 대상(event/method)은
    /// 매니페스트의 `[[extends.*]]` 항목으로 표현되며 별도 권한 grant는 받지 않는다 —
    /// 사용자 인지 부하를 낮추기 위해 target plugin 단위 단일 토큰만 노출.
    Extension(String),
    /// 사이드바 도구 메뉴에 항목을 contribute할 수 있는 권한. 토큰 `ui.tool_item`.
    /// `[[contributes.tool]]` 항목을 선언한 plugin은 매니페스트의 `permissions`에 반드시
    /// 이 토큰을 포함해야 한다.
    UiToolItem,
    /// Popup contribute 권한. 토큰 `ui.popup`.
    /// `[[contributes.popup]]` 항목을 선언한 plugin은 매니페스트의 `permissions`에
    /// 반드시 이 토큰을 포함해야 한다.
    UiPopup,
    /// Banner contribute 권한. 토큰 `ui.banner`.
    /// `[[contributes.banner]]` 항목을 선언한 plugin 은 매니페스트의 `permissions` 에
    /// 반드시 이 토큰을 포함해야 한다.
    UiBanner,
    /// Settings page contribute 권한. 토큰 `ui.settings_page`.
    /// `[[contributes.settings_pages]]` 항목을 선언한 plugin 은 매니페스트의
    /// `permissions` 에 반드시 이 토큰을 포함해야 한다.
    UiSettingsPage,
    /// `[[contributes.window]]` 권한. 토큰 `window.spawn`.
    /// Plugin 이 OS-level 별도 윈도우를 contribute 하려면 이 권한이 필요하다.
    /// 1.0 에서는 schema + stub 만 — 실제 spawn handler 는 별도 영역에서 도입.
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
    /// 새 hook 핸들러 정의 권한. 토큰 `hook_handler.define`.
    /// `[[contributes.hook_handler]]` 로 훅 핸들러(IpcSequence)를 선언할 때 필요.
    /// 파일 핸들러 `file_handler.define` 미러 — 훅 핸들러엔 detector 재선언(extend)
    /// 개념이 없어 define + handle 두 토큰만 둔다.
    HookHandlerDefine,
    /// 특정 hook 핸들러 id 에 대한 scope 권한. 토큰 형식: `hook_handler.handle:<id>`.
    /// `<id>` 는 hook 핸들러 short-name(`[a-z0-9-]{1,32}`). 파일 핸들러
    /// `file_handler.handle:<detector>` 미러 — 특정 핸들러 id 단위 grant 가시성을
    /// 위해 예약된 scoped 토큰.
    HookHandlerHandle(String),
    /// 새 완료 판정 전략 정의 권한. 토큰 `completion_strategy.define`.
    /// `[[contributes.completion_strategy]]` 로 전략(poll/push)을 선언할 때 필요.
    /// 훅 핸들러 `hook_handler.define` 미러 — 별도 레지스트리이므로 레지스트리당
    /// 1토큰 선례에 따라 재사용하지 않고 신설한다.
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
                    // F.B.6-2: is_valid_detector_id 의 schema 부분만 inline 검증.
                    // host file 도메인의 추가 검증은 install 시점에 한다.
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
                    // 훅 핸들러 short-name 규칙(`[a-z0-9-]{1,32}`). `$`-prefix reserved
                    // 개념이 없으므로 detector 와 달리 그대로 검증만 한다.
                    if !is_valid_hook_handler_id(id) {
                        return None;
                    }
                    return Some(Self::HookHandlerHandle(id.to_string()));
                }
                return None;
            }
        })
    }

    // F.B.6-2 — host 의 `file::format::is_valid_detector_id` 와 동일 규칙을 local
    // 복제. 본 crate 가 호스트 file 도메인 결합 없이 token 파싱을 마치기 위함이며,
    // 두 함수가 어긋나면 install 단계의 file 도메인 검증에서 reject 된다.
    /// 권한의 토큰 문자열 형태. `IpcInvoke`는 prefix를 포함하므로 owned `String`을
    /// 반환한다. 비교/저장에는 `&token`을 그대로 사용하면 된다.
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

/// plugin이 publish할 이벤트 카탈로그 항목. `events_emitted = [...]`로 선언.
///
/// - `key`: 정확한 이벤트 키 (와일드카드 불가, 예약 네임스페이스 불가).
///   `event_publish` 권한 패턴 안에 포함되어야 한다 (그렇지 않으면 publish 시점에 호스트가 거부).
/// - `description`: 사람용 짧은 설명.
/// - `stability`: 이벤트 안정성 등급. 기본 `stable`. 새 이벤트를 도입할 때 plugin 작성자가
///   `experimental`로 표기해 호환성 약속을 약화시킬 수 있다.
/// - `payload_schema`: 옵션. 페이로드 JSON Schema 파일의 매니페스트 디렉터리 기준 상대 경로.
///   1.0에서는 호스트가 검증에 사용하지 않고 카탈로그용으로만 보유한다.
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

/// `[[contributes.hook_events]]` 한 항목 — plugin 이 `surface.fire_hook` 로 발사하는
/// surface hook 이벤트 키 카탈로그. `events_emitted`(pub/sub 이벤트 버스)와는 **별개
/// 시스템**이다 — 본 선언은 host 의 `hook.set` / `surface.fire_hook` 검증에만 쓰인다.
///
/// 코어 `HookEvent` 는 claude 등 에이전트 고유 이벤트명을 모른 채 미인식 문자열을
/// `Custom(String)` 으로 수용한다. 그 결과 오타·미존재 이벤트도 조용히 등록될 수
/// 있으므로, plugin 이 자기가 발사하는 키를 선언하고 host 가 (내장 ∪ 활성 plugin 선언)
/// 집합으로 검증해 죽은 hook 등록을 막는다.
///
/// - `key`: 정확한 hook 이벤트 키 (소문자 ascii + 숫자 + `-`, 알파벳으로 시작,
///   와일드카드 불가). 내장 이벤트(`process-exit`/`bell`/`notification`/
///   `output-match:`/`idle-timeout:`)와 충돌 불가.
/// - `description`: 사람용 짧은 설명.
/// - `stability`: 이벤트 안정성 등급. 기본 `stable`.
#[derive(Debug, Clone, Deserialize)]
pub struct HookEventDecl {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub stability: EventStability,
}

/// 프리셋 편집기가 이 kind 의 surface 를 편집할 때 노출할 입력 필드 한 항목
/// (`[[surface_kinds.preset_fields]]`). settings_pages 의 [`SettingsItemDecl`] 구조를
/// **참고**하되(재사용 아님 — settings 는 `storage_key` 기반 저장 모델이라 목적이 다르다)
/// 저장 대상이 `PresetSurface.params` 인 프리셋 편집용 별도 타입이다.
///
/// plugin 필드는 항상 `param_key` 로 `PresetSurface.params.<param_key>` 에 write 한다
/// (builtin terminal 의 cwd/startup 전용 컬럼 라우팅은 host 측 표현에서만 구분되며
/// 매니페스트로는 선언되지 않는다).
///
/// - `id`: kind 내 항목 식별자 (소문자/숫자 + `_`/`-`, 1..=64). kind 안에서 유일.
/// - `label_key`: 항목 라벨 i18n 키 (비어있지 않음).
/// - `param_key`: 값을 write 할 `PresetSurface.params` 의 키 (비어있지 않음).
/// - `input_type`: 편집 위젯 결정. `text`/`url` → 단순 Input, `file_path`/`dir` →
///   Input + 파일 다이얼로그. 기본 `text`.
/// - `required`: 프리셋 적용에 필수인 값인지. 기존 `required_params` 의 진실원이며,
///   `required_params` 를 함께 선언하면 `required=true` 인 필드의 `param_key` 집합과
///   정합해야 한다(검증).
/// - `placeholder_key`: 옵션. 빈 입력 placeholder i18n 키.
/// - `default`: 옵션. kind 로 새로 전환 시 초기값(문자열).
/// - `derive_cwd`: 옵션. 적용 시 이 필드 경로의 부모 디렉토리를 cwd 로 파생
///   (`input_type = file_path` 에서만 유효 — url/text 는 파생 무의미).
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

/// [`PresetFieldDecl::input_type`] — 편집 위젯 + 값 해석 정책.
///
/// 1차 스코프는 문자열 값(`text`/`file_path`/`dir`/`url`)만 다룬다. 향후
/// `bool`/`number`/`select` 확장을 위해 별도 enum 으로 두어 여지를 남긴다.
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
    /// surface 열기 요청 시 반드시 포함되어야 하는 IPC params 키 목록. 호스트가
    /// surface 생성 요청을 generic 하게 검증할 때 사용한다 — plugin 별로 host
    /// 본체에 박혀 있던 `if kind == "markdown"` 류 직결 분기를 대체.
    /// 예: markdown 의 경우 `["file"]`.
    #[serde(default)]
    pub required_params: Vec<String>,
    /// surface 열기 요청 params 의 key alias 매핑. caller 가 옛 키로 넘기면
    /// host 가 canonical 키로 정규화한다. host 본체의 `kind == "markdown"` 일 때
    /// `file_path` → `file` 정규화 같은 결합을 generic 화 하기 위한 메타.
    /// 예: `{"file_path": "file"}`.
    #[serde(default)]
    pub param_aliases: HashMap<String, String>,
    /// surface 생성 시 params 에 없으면 host 가 주입하는 kind별 기본값. 값은 리터럴
    /// 이거나 host 가 해석하는 정책 토큰(`@settings.explorer_view_mode`, `@home`)이다.
    /// "어느 kind 가 어떤 기본키를 요구하는가"를 decl 로 옮겨 host 본체의
    /// `kind == "explorer"` 기본값 주입 하드코딩을 generic 화한다.
    #[serde(default)]
    pub default_params: HashMap<String, String>,
    /// host 가 이 kind 를 host egui 위젯으로 렌더해 winit 키/IME 이벤트를 host egui
    /// 입력 시스템으로 흘려야 하는지(예: explorer). egui-mesh 로 렌더되는 kind
    /// (markdown/image)는 host egui 에 대응 위젯이 없어 false — 중앙 키 디스패처가
    /// surface 로 forward 한다. host 본체의 `kind == "explorer"` 라우팅 분기를 generic 화.
    #[serde(default)]
    pub consumes_egui_input: bool,
    /// zoom in/out/reset 단축키로 이 kind 의 폰트 크기 override 를 조절할 수 있는지
    /// (예: markdown/explorer). host 본체의 `kind == "markdown" || kind == "explorer"`
    /// 줌 게이트를 generic 화. 기본 false.
    #[serde(default)]
    pub zoomable: bool,
    /// copy 단축키가 이 kind 에 egui `Event::Copy` 를 주입해야 하는지(선택 텍스트를
    /// plugin egui 가 클립보드로 복사, 예: markdown). host 본체의 `kind == "markdown"`
    /// copy 게이트를 generic 화. 기본 false.
    #[serde(default)]
    pub egui_copy: bool,
    /// 이 kind 가 select-all / copy-path 단축키(선택 항목 경로 복사)를 소비하는지
    /// (예: explorer). host 본체의 `kind == "explorer"` 게이트를 generic 화. 기본 false.
    #[serde(default)]
    pub copy_path: bool,
    /// paste 단축키를 이 kind 가 자체 소비하는지(host 가 terminal paste 로 흘리지 않음,
    /// 예: image 는 plugin `image.paste` 로 처리). host 본체의 `kind == "image"` paste
    /// 게이트를 generic 화. 기본 false.
    #[serde(default)]
    pub egui_paste: bool,
    /// 자동 탭 명명 시 basename 을 파생할 params 키(예: markdown="file",
    /// image="file"). `Some(key)` 이면 params 의 그 키 값 basename 을 표시명으로 쓴다.
    /// host 본체의 `kind == "markdown"` basename 명명 하드코딩을 generic 화. 미선언이면
    /// 파생 없이 kind 표시명 fallback.
    #[serde(default)]
    pub name_from_param: Option<String>,
    /// 이 kind 의 surface 를 파일로 열 때 host 가 "최근 연 파일" 목록에 기록할지
    /// (예: markdown=true). host 는 특정 kind 이름을 모르고 이 플래그로 기록 대상을
    /// 판정한다(generic per-kind). host 본체의 `kind == "markdown"` recent 기록 분기를
    /// generic 화. 미선언이면 기록 안 함.
    #[serde(default)]
    pub records_recent: bool,
    /// 이 kind 로 convert 하려면 host 가 먼저 "파일 입력 팝업"을 띄워야 하는지
    /// (예: markdown=true — 어느 파일을 열지 골라야 한다). host 는 특정 kind 이름을
    /// 모르고 이 플래그로 convert 라우팅을 판정한다: `true` 면 즉시 빈 params 변환
    /// 대신 `convert_input_popup` 이 가리키는 plugin 팝업을 연다. host 본체의
    /// `kind == "markdown"` convert 분기를 generic 화. 미선언이면 빈 params 즉시 변환.
    #[serde(default)]
    pub convert_requires_input: bool,
    /// `convert_requires_input == true` 일 때 host 가 열 이 plugin 의 file-input
    /// 팝업 **local id**(예: markdown="file-open" — 같은 plugin 의
    /// `[[contributes.popup]] id`). host 는 이를 `<plugin_id>/<popup_id>` 로 qualify
    /// 해 `open_popup_instance` 로 연다(payload 에 convert 대상 surface_id 를 실어
    /// 제자리 변환, 미포함이면 새 탭 열기). markdown/event-key 하드코딩 없이 데이터로만
    /// 라우팅하기 위한 필드다. 미선언이면 convert-input 팝업 없음.
    #[serde(default)]
    pub convert_input_popup: Option<String>,
    /// 프리셋 편집기가 이 kind 를 편집할 때 노출할 입력 필드 스키마
    /// (`[[surface_kinds.preset_fields]]`). 편집기가 kind 별로 다른 폼을 generic
    /// 하게 렌더/저장하는 근거다 (예: markdown 은 파일 경로, html 은 URL). 빈 vec 이면
    /// 편집기가 이 plugin kind 전용 필드를 표시하지 않는다.
    #[serde(default)]
    pub preset_fields: Vec<PresetFieldDecl>,
}

/// surface kind의 렌더링 방식. plugin 매니페스트 `rendering = "remote" | "webview" | "egui-mesh"`.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKindRendering {
    /// 기본값. webview kind 가 공유하는 `RemoteSurface` stand-in 으로 등록된다.
    /// (host 가 콘텐츠를 그리는 UiNode tree 렌더 경로는 제거됨.)
    #[default]
    Remote,
    /// 호스트가 OS-level native WebView overlay 로 surface 영역을 자동 관리한다.
    /// plugin 은 `webview.set_url(surface_id, url)` 등 IPC 로 URL/navigation 만 제어.
    /// host 는 어떤 컨텐츠 (html/svg/...) 인지 모름 — webview 토대만 제공.
    Webview,
    /// plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성한다
    /// (ADR-0028). bundled 전용 화이트리스트 + api_version 게이트로 제한된다.
    ///
    /// 와이어 키는 하이픈 포함 `"egui-mesh"` — `rename_all = "lowercase"` 가 만드는
    /// `"eguimesh"` 를 variant 단위 rename 으로 덮어쓴다. 이 rename 을 빠뜨리면
    /// 매니페스트 `rendering = "egui-mesh"` 가 파싱되지 않는다.
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
/// - `Transform`: payload를 변경할 수 있다 (반환값으로 덮어쓰기). 가장 강력.
/// - `Filter`: `pass: bool`만 반환. 차단 가능하지만 payload 변경 불가.
/// - `Observe`: 결과는 호스트가 무시. 단순 관찰/로깅. timeout/실패도 체인에 영향 없음.
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
    /// 사이드바 "도구" 팝업에 표시될 항목. 클릭 시 plugin이 정의한 동작(event publish,
    /// surface 열기, popup 열기) 발생. 항목당 `[[contributes.tool]]` 한 블록.
    #[serde(default)]
    pub tool: Vec<ToolContribute>,
    /// Plugin 이 contribute 하는 OS-level 윈도우 정의. 본 schema 는 1.0 에서
    /// *등록 stub 만 동작* — 실제 spawn handler 는 별도 영역. validator + 권한
    /// 검사 + hello 시 `CoreEvent::PluginWindowDeclared` 발화까지 수행한다.
    #[serde(default)]
    pub window: Vec<WindowContribute>,
    /// Plugin이 띄울 수 있는 popup 정의. trigger 종류에 따라 자동으로 열리거나
    /// 명시적인 IPC 호출로 열린다.
    #[serde(default)]
    pub popup: Vec<PopupContribute>,
    /// Plugin 이 띄울 수 있는 banner 정의(A3). banner 는 non-modal 공지로, plugin 이
    /// egui-mesh 로 content 만 그리고 셸/스택/위치/dismiss 타이밍은 host 소유다.
    /// phase1 은 IPC(`banner.open`) 트리거만 지원한다.
    #[serde(default)]
    pub banner: Vec<BannerContribute>,
    /// 새 detector 정의 또는 기존 detector 재선언(rule 추가 + 메타 patch).
    /// 같은 id 가 host/다른 plugin/user 에 이미 있으면 rule union, 메타는
    /// last-writer-wins (install 순서 host → plugin → user).
    ///
    /// **Opaque payload** (F.B.2): manifest crate 가 본 바이너리 `file::format::config`
    /// 결합 없이 분리 가능하도록 raw JSON Value 로 보관. concrete `DetectorDecl`
    /// 변환/검증은 본 바이너리 `plugin_bridge::manifest_validate` 또는 host
    /// FileFormatRegistryPort impl 에서 수행.
    #[serde(default)]
    pub detector: Vec<serde_json::Value>,
    /// 파일 핸들러 contribute. plugin 은 `OpenSurface` / `Ipc` 만 사용 가능.
    ///
    /// **Opaque payload** (F.B.2) — detector 와 동일 사유.
    #[serde(default)]
    pub handler: Vec<serde_json::Value>,
    /// 훅 핸들러 contribute (webhook/hook 공유 레지스트리). plugin 은 `IpcSequence`
    /// 만 사용 가능(셸 `ShellCommand` 는 host/user 전용 — 타입 레벨 배제). 각 항목은
    /// `[[contributes.hook_handler]]` 한 블록이며 `hook_handler.define` 권한을 요구한다.
    ///
    /// **Opaque payload** (F.B.2 동일 사유) — manifest crate 가 본 바이너리
    /// `hook_handler::config` 결합 없이 분리 가능하도록 raw JSON Value 로 보관.
    /// concrete `HookHandlerDecl<PluginHookHandlerActionDecl>` 변환/검증은 host
    /// `HookHandlerRegistryPort` impl(install 시점)에서 수행한다.
    #[serde(default)]
    pub hook_handler: Vec<serde_json::Value>,
    /// 완료 판정 전략 contribute (독립 `CompletionStrategyRegistry`,
    /// 훅 핸들러와 형태만 미러링, 코드/타입 공유 없음). poll 형(자체 폴링 사양)과
    /// push 형(`notify_via: HookHandlerId` + 필수 timeout) 둘 다 이 필드로 선언한다.
    /// 각 항목은 `[[contributes.completion_strategy]]` 한 블록이며
    /// `completion_strategy.define` 권한을 요구한다.
    ///
    /// **Opaque payload** (F.B.2 동일 사유, hook_handler 와 동일) — manifest crate 가
    /// 본 바이너리 `completion_strategy::config` 결합 없이 분리 가능하도록 raw JSON
    /// Value 로 보관. concrete decl 변환/검증은 host
    /// `CompletionStrategyRegistryPort` impl(install 시점)에서 수행한다.
    #[serde(default)]
    pub completion_strategy: Vec<serde_json::Value>,
    /// Plugin 이 설정 모달에 노출할 sub-page 정의. 항목당 `[[contributes.settings_pages]]`
    /// 한 블록. host 가 plugin registry 를 순회해 동적으로 sub-tab 을 그린다.
    /// 1 차 schema 는 `FontOverride` 항목만 지원 (Color/Bool/Enum 등은 후속 확장).
    #[serde(default)]
    pub settings_pages: Vec<SettingsPageContribute>,
    /// Plugin 이 `surface.fire_hook` 로 발사하는 surface hook 이벤트 키 카탈로그.
    /// host 가 `hook.set` / `surface.fire_hook` 검증을 (내장 ∪ 활성 plugin 선언)
    /// 집합으로 수행하는 데 쓴다. 항목당 `[[contributes.hook_events]]` 한 블록.
    #[serde(default)]
    pub hook_events: Vec<HookEventDecl>,
}

/// Plugin 이 contribute 하는 설정 모달 sub-page 정의 (`[[contributes.settings_pages]]`).
///
/// host 가 설정 UI 의 카테고리별 sub-tab 영역에 plugin registry 를 순회해
/// 동적으로 추가한다. plugin 비활성 시 자동으로 사라지므로 dead-setting 이
/// 노출되지 않는다.
///
/// - `id`: plugin 내 고유 (소문자/숫자 + `_` + `-`). 호스트는 `<plugin_id>/<id>`
///   로 전역 식별.
/// - `title_key`: sub-tab 라벨 i18n 키.
/// - `category`: 호스트가 받아들이는 설정 카테고리 (예: `appearance`).
/// - `items`: 이 page 에서 host 가 generic 렌더할 항목 목록.
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

/// `[[contributes.settings_pages.items]]` 의 한 항목.
///
/// `#[serde(tag = "kind", rename_all = "snake_case")]` 로 직렬화한다(toml `kind = "..."`).
/// 1 차는 `FontOverride` 만 지원했고, 16-B 에서 generic 컨트롤(`Toggle`/`Select`/`Number`)
/// 이 추가됐다. 모든 variant 는 공통으로 `id`(page 내 식별자) · `label_key`(라벨 i18n 키) ·
/// `storage_key`(host 측 저장 슬롯 키) 를 갖는다.
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
    },

    /// 선택지(드롭다운). host 는 디자인 Select 로 그리고 선택된 `value` 를 문자열로 저장한다.
    /// `default` 는 `options` 의 `value` 중 하나여야 한다(검증). 옵션 라벨은 `label_key` i18n.
    Select {
        id: String,
        label_key: String,
        storage_key: String,
        options: Vec<SelectOptionDecl>,
        default: String,
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
    },
}

/// `Select` 항목의 한 선택지 — 저장될 `value` 와 표시용 `label_key`(i18n).
#[derive(Debug, Clone, Deserialize)]
pub struct SelectOptionDecl {
    pub value: String,
    pub label_key: String,
}

impl SettingsItemDecl {
    /// 모든 variant 가 공통으로 갖는 `(id, label_key, storage_key)`. 검증이 variant 무관하게
    /// 공통 형식 검사를 한 곳에서 수행하도록 노출한다.
    pub(crate) fn common(&self) -> (&str, &str, &str) {
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
            } => (id, label_key, storage_key),
        }
    }
}

/// Plugin 이 contribute 하는 OS-level 윈도우 정의 (`[[contributes.window]]`).
///
/// 1.0 에서는 *schema + 등록 stub* 까지만 동작한다. 실 spawn handler /
/// multi-window 라우팅은 별도 영역에서 추가될 때까지 호스트는 hello 시
/// `tracing::info!` 와 `CoreEvent::PluginWindowDeclared` 만 발화.
///
/// - `id`: plugin 내 고유 식별자 (`is_valid_kind` 규칙 — 소문자/숫자/`_`).
/// - `display_name_i18n_key`: 메뉴 등에 노출될 라벨 키.
/// - `icon`: 옵션. 아이콘 이름.
/// - `default_size`: 옵션. LogicalPx 단위 권장 크기.
/// - `multi_instance`: 동시 여러 인스턴스를 허용하는지. 기본 false.
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
    /// popup 이 속한 범위. 필드가 없는 매니페스트는 `window` 로 읽혀 이전 동작(항상
    /// 보이고 창 전체에 clamp)을 그대로 갖는다.
    #[serde(default)]
    pub scope: PopupScopeDecl,
    #[serde(default = "default_dismiss_on_outside_click")]
    pub dismiss_on_outside_click: bool,
    /// popup 콘텐츠 렌더링 방식. `egui-mesh`(기본이자 유일) — plugin 이 자기
    /// 프로세스에서 egui mesh 를 tessellate 하고 host 가 popup 영역에 합성한다
    /// (ADR-0028, A2). 셸(scrim/border/outside-click/Esc)은 host 소유.
    #[serde(default)]
    pub rendering: PopupRendering,
}

/// popup 콘텐츠의 렌더링 방식. UiNode(ui-tree) 채널은 제거돼 egui-mesh
/// 가 기본이자 유일한 channel 이다.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PopupRendering {
    /// plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성한다
    /// (ADR-0028). bundled 전용 — host 화이트리스트 매칭이 필요하다.
    ///
    /// 와이어 키는 하이픈 포함 `"egui-mesh"` — `rename_all = "kebab-case"` 가 만드는
    /// `"egui-mesh"` 와 일치하지만, surface 쪽 [`SurfaceKindRendering`] 과 표기를
    /// 맞추기 위해 명시적으로 rename 한다.
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

/// popup이 열리는 조건.
///
/// - `event`: 매니페스트에 적힌 이벤트가 발화하면 자동으로 열림. plugin은 같은 키를
///   `event_subscribe` 또는 `event_publish`로 매니페스트에 노출해야 한다 (자기 plugin이
///   발화한 이벤트로 자기 popup을 여는 흐름이 일반적).
/// - `ipc`: plugin이 host IPC `popup.open`을 호출해 명시적으로 연다.
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

/// plugin popup 이 매니페스트에서 선언하는 소속 범위.
///
/// 선언은 **종류**만 정한다. 대상 surface 는 런타임에야 알 수 있으므로 popup 을 여는
/// host 진입점이 바인딩한다(host `PopupDef` 의 `OpenPopupMode::WithScope` 와 같은 분담).
/// 바인딩된 대상이 없으면(plugin 이 스스로 연 popup 등) host 는 `window` 로 다룬다.
/// 근거는 `docs/adr/0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md`.
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

/// Plugin 이 contribute 하는 banner 의 정의(A3).
///
/// banner 는 non-modal 공지 오버레이(4번째 오버레이 개념). plugin 은 content 만
/// egui-mesh 로 그리고, 셸(컨테이너/border/close X/카운트다운)·스택(큐)·위치·dismiss
/// 타이밍은 **host 소유**다(identity 원칙 1·3, popup chrome 과 동일).
///
/// - `id`: plugin 내 고유. 호스트는 `<plugin_id>/<banner_id>` 로 전역 식별.
/// - `trigger`: 어떤 조건으로 banner 를 여는지. phase1 은 `ipc` 만.
/// - `scope`: banner 가 뜰 스코프. plugin 은 `surface` 만 선언할 수 있고, host 는
///   그 plugin 이 *소유한* surface 로만 표시를 허용한다(남의 scope 차단, D1).
/// - `ttl_seconds`: 옵션. `Some` 이면 카운트다운 후 자동 소멸, `None` 이면 사용자
///   close X 까지 유지(persistent). host 는 persistent 여도 close X 를 항상 노출한다.
/// - `size_hint`: 옵션. banner 높이(LogicalPx) 힌트. 너비는 스코프 폭에 도킹되므로
///   높이만 의미가 있다.
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
    /// banner 콘텐츠 렌더링 방식. phase1 은 `egui-mesh` 만 지원한다.
    #[serde(default)]
    pub rendering: BannerRendering,
}

/// banner 콘텐츠의 렌더링 방식. phase1 은 egui-mesh 만.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BannerRendering {
    /// plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성한다
    /// (ADR-0028, A3). bundled 전용 — host api_version 게이트가 필요하다.
    #[default]
    #[serde(rename = "egui-mesh")]
    EguiMesh,
}

/// banner 가 열리는 조건. phase1 은 `ipc` 만 지원한다(event 트리거는 defer).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BannerTrigger {
    /// plugin 이 host IPC `banner.open` 을 호출해 명시적으로 연다.
    Ipc,
}

/// banner 스코프 선언. plugin 은 `surface` 만 선언 가능(D1) — host 가 그 plugin 이
/// 소유한 surface 에만 배치한다. 다른 스코프(workspace/view 등)는 host 전용이다.
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
/// - `event`: 호스트가 envelope를 `publish_from_host`로 발화. plugin이 자기 namespace의
///   key를 subscribe하고 있으면 받음. payload는 `{ "tool_id": "<plugin_id>/<tool_id>" }`.
/// - `open_surface`: 활성 tab에 해당 surface kind를 추가. plugin이 `[[surface_kinds]]`로
///   선언한 kind만 허용.
/// - `open_popup`: plugin이 contribute한 popup을 연다. (phase2-popup이 완성되기 전까지는
///   호스트가 warn 로그를 남기고 무시.)
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
    /// true 면 stdin 이 TTY 가 아닐 때 stdin 의 JSON 한 덩이를 읽어, CLI 인자로
    /// 명시되지 않은 params 필드를 채운다. Claude Code 처럼 hook payload 를
    /// stdin JSON 으로 전달하는 외부 시스템과 연동할 때 사용. 매칭 키는 각
    /// `CliArg.stdin_field` 또는 (없으면) `CliArg.name`.
    #[serde(default)]
    pub stdin_json: bool,
    /// 있으면 CLI 가 *1 회 응답 + 즉시 종료* 가 아니라, `terminal_states` 중
    /// 하나에 도달하거나 `--timeout` 초가 지날 때까지 *반복 IPC 호출* 한다.
    /// `tasty claude wait` 처럼 *진짜 blocking subprocess* 가 필요한 경우 활성화.
    /// timeout 도달 시 마지막 응답을 그대로 출력한다.
    #[serde(default)]
    pub polling: Option<PollingDecl>,
    /// 있으면 CLI 가 1 차 IPC 응답을 출력한 *뒤* `auto_wait.method` 를 chain
    /// 호출하여 terminal_states 도달까지 block 한다 (claude/codex spawn·tell 의
    /// 자동 wait). `polling` 과 동시 선언 금지 (validator 가 reject).
    #[serde(default)]
    pub auto_wait: Option<AutoWaitDecl>,
}

/// `spawn` / `tell` 같이 1 차 응답 후 *chained wait* 를 자동으로 거는 CLI 명령
/// 의 설정. CLI 는 1 차 IPC 응답을 line-delimited JSON 으로 출력한 뒤,
/// `--no-wait` 가 아닐 경우 `method` 를 호출하여 `polling.terminal_states`
/// 도달까지 block 하고 wait 응답을 두 번째 JSON line 으로 출력한다.
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

/// Plugin 이 이름으로 등록하는 완결 감지 전략(completion strategy) 선언.
///
/// `AutoWaitDecl.strategy` 가 이름으로 참조하는 CLI 측 실행형(`PollingDecl`)과
/// `tasty_agent::task::PollSpec`(agent DAG `TaskCommand::Custom.poll`) 양쪽으로
/// 변환될 수 있는 host-facing 공통 표현이다. 이 타입 자체는 `[[contributes.*]]`
/// 배열에 담기는 opaque 계약 필드가 아니다 — 계약 필드는 해당 registry(host 측)
/// 가 별도로 소유하고, 여기서는 그 값을 이 타입으로 디코드한 뒤 아래 변환
/// 메서드로 실행형을 얻는다.
///
/// 필드 의미는 `PollSpec`/`PollingDecl` 과 동형으로 맞춘다. 필드가 갈라지면
/// `to_polling_decl()`/host 측 `completion_strategy_to_poll_spec()` 변환과 그
/// 단위테스트가 깨지므로, 대응 관계는 컴파일/테스트로 강제된다(주석만으로
/// 지켜지는 불변식이 아니다).
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
    /// **실패**로 간주할 상태값 목록. 적중하면 task 가 `Failed` 로 종결된다
    /// (`PollSpec.failure_states` 와 동형 — 의미·우선순위 설명은 그쪽 doc 참조).
    /// 생략 가능 — 이 필드 이전에 작성된 매니페스트가 그대로 로드돼야 한다.
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
    /// CLI `AutoWaitDecl.strategy` 경로용 변환. `PollingDecl.timeout_field` 는
    /// 항상 `None` 으로 고정된다 — named strategy 는 이름으로 공유되는 고정
    /// 사양이라 CLI 요청마다 다른 `--timeout` 오버라이드 키를 가질 수 없다
    /// (`PollingDecl` 자신의 shape 은 불변으로 유지, 인라인 `polling` 경로만
    /// 여전히 CLI `--timeout` 오버라이드를 지원). `timeout_ms` 는 이 경로에서
    /// 쓰이지 않는다 — CLI 폴링은 raw ms 데드라인이 아니라 요청 params 조회
    /// 기반이라 표현 형태가 근본적으로 다르다(§ 대표 doc comment 참조).
    ///
    /// `failure_states` 도 버린다 — CLI auto_wait 폴링(`crates/tasty-cli/src/run.rs`)
    /// 은 terminal 도달 시 block 을 풀 뿐 프로세스 exit code 를 가르지 않는다.
    /// 여기에 실패 축을 흘리면 `tasty claude spawn` 같은 기존 명령이 자식 사망 시
    /// 비-0 으로 죽게 되어 **CLI 계약이 바뀐다**. 실패/성공 분기는 지금 DAG
    /// 러너(`PollSpec.failure_states`)만의 관심사이므로 그 경계를 여기서 끊는다 —
    /// CLI 를 실패로 죽이는 건 그 계약 변경을 의도적으로 결정할 때 별도로 한다.
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
    /// path 인자의 의미론적 종류. `Some("directory")` 또는 `Some("file")` 을 쓴다 —
    /// CLI 가 이 인자를 발견하면 호출자 cwd 기준 absolute path 로 정규화 +
    /// 존재 검증(directory 는 디렉토리인지, file 은 일반 파일인지) 후 IPC 로 전달.
    /// (호스트는 absolute + valid 만 받는다는 contract.) 파일 *내용* 검증(예: JSON
    /// 파싱)은 하지 않는다 — 그 내용을 아는 plugin 핸들러 몫이다.
    #[serde(default)]
    pub path_kind: Option<String>,
    /// `true`면 이 인자가 두 번 이상 지정됐을 때(예: `--profile-file a --profile-file b`)
    /// clap 기본값인 last-wins(앞선 값이 조용히 사라짐) 대신 에러로 거부한다.
    /// `--settings`류(단일 슬롯, 반복 지정 시 앞선 값이 통째로 무시됨을 실측 확인)
    /// 인자에 쓴다 — 나머지 인자는 기존 last-wins 그대로 둔다.
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
    /// command 발화 범위. `global`은 어디서나 동작(조합키 권장), `surface`는
    /// owner plugin이 만든 surface에 포커스가 있을 때만 동작(단일 키 허용).
    /// 기본 `global`.
    #[serde(default)]
    pub scope: CommandScope,
    /// 선언적 액션. `[[contributes.tool]].action`과 동일한 `ToolAction` 재사용 —
    /// popup/surface/event 를 호스트가 `handle_command` IPC 없이 직접 처리한다.
    ///
    /// **우선순위**: `action`이 있으면 호스트가 그 액션만 직접 실행하고, 옛
    /// `command.invoke` IPC(`handle_command`)는 이 command에 대해 아예 발사되지
    /// 않는다 — plugin이 `handle_command`를 구현했더라도 호출되지 않는다(action과
    /// handle_command 동시 실행은 popup 중복 오픈 등 부작용 위험이 있어 금지).
    /// `action`이 없으면 기존 `handle_command` IPC 왕복 경로를 그대로 쓴다. Event
    /// Bus `command.invoked` 통지는 `action` 유무와 무관하게 항상 발사된다
    /// (informational — plugin이 옵저버 목적으로만 구독해도 안전).
    #[serde(default)]
    pub action: Option<ToolAction>,
}

/// command가 어떤 포커스 컨텍스트에서 발화하는지.
///
/// - `Global`: 어디서나 동작. 단축키는 조합키만 권장.
/// - `Surface`: owner plugin이 만든 surface에 포커스가 있을 때만 동작.
///   단일 키도 허용.
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

/// `Deserialize` 가 받는 TOML 표기를 그대로 되돌려준다 —
/// `"independent"` · `"inherit:<host_action>"`.
///
/// 이 impl 이 없으면 이 열거를 밖으로 내보내는 자리가 `Debug` 파생을 문자열화하게 되고,
/// 그러면 `inherithost("clipboard.copy")` 처럼 **입력 표기로 되돌릴 수 없는** 값이 나간다
/// (실제로 `plugin.show` 가 그 값을 내보내고 있었다). `Debug` 는 사람이 읽는 표기지
/// 와이어 키가 아니다 — 둘을 같은 것으로 쓰면 컴파일러는 어긋남을 모른다.
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

/// `file::format::is_valid_detector_id` 의 로컬 복제. 길이 1..=64, lowercase ascii
/// + 숫자 + `-`, optional `$`-prefix. 호스트 측 식별기와 동일 규칙이며 schema 차원
///   검증 only.
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
