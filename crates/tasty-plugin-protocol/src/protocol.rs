//! Plugin ↔ Host 메시지 정의.
//!
//! Plugin이 connection 직후 첫 줄로 보내는 `AuthMessage`로 token 인증.
//! 이후 한 줄당 하나의 JSON 메시지(NDJSON 스타일).
//!
//! - 응답: `{"id": N, "result": ...}` 또는 `{"id": N, "error": "..."}`
//! - 알림 (id 없음): `{"event": {"kind": "...", ...}}`

use serde::{Deserialize, Serialize};

use crate::events::EventEnvelope;

// ── Host → plugin method names ──
pub const METHOD_PING: &str = "ping";
pub const METHOD_SHUTDOWN: &str = "shutdown";
pub const METHOD_HOST_HELLO: &str = "host.hello";
pub const METHOD_SURFACE_CREATE: &str = "surface.create";
pub const METHOD_SURFACE_SNAPSHOT: &str = "surface.snapshot";
pub const METHOD_SURFACE_RESTORE: &str = "surface.restore";
pub const METHOD_SURFACE_DESTROY: &str = "surface.destroy";
/// host → plugin (egui-mesh surface 전용): surface 의 렌더 컨텍스트(크기/ppp/raw input)를
/// 전달한다. plugin 은 이를 받아 자기 프로세스에서 egui 를 구동·tessellate 한 뒤
/// [`PluginEvent::PaintFrame`] 로 mesh 를 회신한다. fire-and-forget — 응답은
/// `surface.event` 처럼 별도 알림(`PaintFrame`)으로 비동기 도착한다.
/// params 에 [`SurfaceSetContextParams`].
pub const METHOD_SURFACE_SET_CONTEXT: &str = "surface.set_context";
/// host → plugin: plugin이 보낸 ipc.call에 대한 결과.
/// params에 [`IpcCallResult`].
pub const METHOD_IPC_RESULT: &str = "ipc.result";
/// host → plugin: Event Bus dispatch. params에 [`EventDispatchParams`].
/// 호스트는 응답을 기다리지 않고 응답 합치기도 없다(broadcast 모델). 다만 응답이 오면 그
/// dispatch 의 재발화 hop 하한 기록을 지운다 — 응답 전에 그 plugin 이 publish 한 사건은
/// hop 이 올라가므로, SDK 는 `on_event` 가 돌아온 뒤에 응답한다(docs/reference/event-catalog.md#재발행과-응답).
pub const METHOD_EVENT_DISPATCH: &str = "event.dispatch";
/// host → plugin: 사용자 단축키 매칭으로 plugin command가 트리거됨.
/// params에 [`CommandInvokeParams`]. plugin은 그에 따라 surface state를 변경하고,
/// 변경 결과는 `SurfaceResult` 형태로 응답한다 (display_name/snapshot 갱신).
pub const METHOD_COMMAND_INVOKE: &str = "command.invoke";
/// 호스트가 WebView의 이동 시도 URL을 플러그인에 알린다.
/// 원격 콘텐츠 차단 정책과는 별개이며 플러그인은 URL을 보고 파일 열기나
/// 외부 앱 열기를 요청할 수 있다. Params는 WebviewNavigationAttemptParams다.
pub const METHOD_WEBVIEW_NAVIGATION_ATTEMPT: &str = "webview.navigation_attempt";
/// host → extension plugin: extension의 pre/post hook 호출.
/// params에 [`ExtensionHookInvokeParams`]. plugin은 mode에 따라 transform/filter/observe
/// 의미로 [`ExtensionHookResult`]를 반환한다 (PluginResponse.result).
pub const METHOD_EXTENSION_INVOKE_HOOK: &str = "extension.invoke_hook";
/// host → plugin: plugin이 contribute한 popup의 인스턴스가 열림. plugin은
/// 응답으로 [`PopupOpenResult`]를 돌려준다. params에 [`PopupOpenParams`].
pub const METHOD_POPUP_OPEN: &str = "popup.open";
/// host → plugin: popup 인스턴스가 닫힘 (사용자 outside-click / Esc / plugin이
/// host IPC로 명시 닫기 요청한 경우 모두 포함). fire-and-forget.
/// params에 [`PopupClosedParams`].
pub const METHOD_POPUP_CLOSED: &str = "popup.closed";
/// host → plugin (egui-mesh popup 전용): popup 인스턴스의 렌더 컨텍스트(크기/ppp/raw
/// input)를 전달한다. [`METHOD_SURFACE_SET_CONTEXT`] 의 popup 대응 — surface_id 대신
/// host 발급 instance_id 로 키잉한다. plugin 은 자기 프로세스에서 egui 를
/// tessellate 한 mesh 를 [`PluginEvent::PopupPaintFrame`] 로 비동기 회신한다.
/// fire-and-forget. params 에 [`PopupSetContextParams`].
pub const METHOD_POPUP_SET_CONTEXT: &str = "popup.set_context";
/// host → plugin: plugin이 contribute한 banner의 인스턴스가 열림(A3). banner 는
/// popup 과 달리 scrim/포커스 없는 non-modal 공지라, plugin 은 mesh 콘텐츠만 그리고
/// 셸/스택/위치/dismiss 타이밍은 host 소유([`BannerSetContextParams`] 참고). plugin 은
/// 응답으로 [`BannerOpenResult`]를 돌려준다. params 에 [`BannerOpenParams`].
pub const METHOD_BANNER_OPEN: &str = "banner.open";
/// host → plugin: banner 인스턴스가 닫힘 (TTL 만료 / 사용자 close X / plugin 이 host
/// IPC `banner.close` 요청 / host shutdown). fire-and-forget. params 에
/// [`BannerClosedParams`].
pub const METHOD_BANNER_CLOSED: &str = "banner.closed";
/// host → plugin (egui-mesh banner 전용, A3): banner 인스턴스의 렌더 컨텍스트(크기/ppp/
/// raw input/theme)를 전달한다. [`METHOD_POPUP_SET_CONTEXT`] 의 banner 대응 — host 발급
/// `instance_id` 로 키잉한다. plugin 은 자기 프로세스에서 tessellate 한 mesh 를
/// [`PluginEvent::BannerPaintFrame`] 로 비동기 회신한다. fire-and-forget. params 에
/// [`BannerSetContextParams`].
pub const METHOD_BANNER_SET_CONTEXT: &str = "banner.set_context";

/// `surface.create` / `surface.restore` / `command.invoke` 응답에 포함되는 standard 결과.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SurfaceResult {
    #[serde(default)]
    pub display_name: Option<String>,
    /// plugin 측 surface state 의 영속화 가능한 표현. `Some` 이면 host 가
    /// `RemoteSurface::snapshot_cache` 를 갱신해 다음 layout 저장 시 그대로
    /// `SavedSurface::Generic { kind, data }` 로 round-trip 한다. `None` 이면
    /// 호스트는 기존 캐시를 유지.
    #[serde(default)]
    pub snapshot: Option<serde_json::Value>,
}

// egui 입력을 프로세스 사이에 전달하기 위한 타입. 기본 빌드에서도 사용할 수 있다.
// 호스트가 받은 사용자 입력을 전달하며, release API로 입력을 합성하지 않는다.

/// `surface.set_context` params — egui-mesh surface 의 렌더 컨텍스트.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SurfaceSetContextParams {
    pub surface_id: u32,
    /// surface 의 물리 픽셀 너비.
    pub width_px: u32,
    /// surface 의 물리 픽셀 높이.
    pub height_px: u32,
    /// 논리→물리 스케일 (egui `ScreenDescriptor.pixels_per_point`).
    pub pixels_per_point: f32,
    /// 이번 frame 의 사용자 입력.
    #[serde(default)]
    pub raw_input: RawInputWire,
    /// 호스트의 현재 테마. None이면 플러그인은 이전 테마나 자체 기본값을 사용한다.
    #[serde(default)]
    pub theme: Option<ThemeWire>,
    /// 텍스처 복구 요청. SDK는 출력 중복 생략을 건너뛰고 보관 중인 전체
    /// 텍스처를 full_textures 프레임에 넣어 보낸다.
    #[serde(default)]
    pub need_full_textures: bool,
}

/// 프로세스 사이에 전달하는 테마 스냅샷. 색 집합, is_light와 zoom을 담는다.
/// 플러그인은 Theme::with_colors_and_zoom으로 길이 값을 다시 계산한다.
/// reduced_motion 같은 다른 설정은 이 구조에 포함되지 않는다.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ThemeWire {
    /// resolved (사용자 override 반영) 색 집합. zoom 독립적.
    pub colors: tasty_type_appearance::theme::ThemeColors,
    /// 라이트/다크 — hover/active/separator overlay 도출에 필요.
    pub is_light: bool,
    /// host UI zoom 배율. sizing token 에 곱해진다 (`with_colors_and_zoom`).
    pub ui_zoom: f32,
}

/// 플러그인에 전달할 egui 입력. IME 조합 이벤트도 events에 포함된다.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct RawInputWire {
    /// frame 시각(초). egui 애니메이션/더블클릭 타이밍용. `None` 이면 plugin 이 자체 시계 사용.
    #[serde(default)]
    pub time: Option<f64>,
    /// 이 surface 가 키보드 포커스를 가지는지.
    #[serde(default)]
    pub focused: bool,
    /// 활성 modifier 상태.
    #[serde(default)]
    pub modifiers: ModifiersWire,
    /// 이번 frame 에 누적된 입력 이벤트 (순서 보존).
    #[serde(default)]
    pub events: Vec<RawInputEventWire>,
}

/// egui `Modifiers` 미러.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ModifiersWire {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    /// macOS Cmd 키.
    pub mac_cmd: bool,
    /// 플랫폼 공통 "command" (macOS=Cmd, 그 외=Ctrl).
    pub command: bool,
}

/// 포인터 버튼 종류. egui `PointerButton` 의 주요 3종 미러.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointerButtonWire {
    Primary,
    Secondary,
    Middle,
}

/// egui `RawInput.events` 의 직렬화 가능한 최소 미러. 좌표는 surface-local 논리 포인트
/// (좌상단 0,0). plugin SDK 가 `egui::Event` 로 매핑한다.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum RawInputEventWire {
    /// 포인터 이동.
    PointerMoved { x: f32, y: f32 },
    /// 포인터 버튼 누름/뗌.
    PointerButton {
        x: f32,
        y: f32,
        button: PointerButtonWire,
        pressed: bool,
        #[serde(default)]
        modifiers: ModifiersWire,
    },
    /// 포인터가 surface 영역 밖으로 나감.
    PointerGone,
    /// 스크롤 델타 (논리 포인트).
    Scroll { x: f32, y: f32 },
    /// 키 누름/뗌. `key` 는 egui `Key` 의 이름 문자열 (plugin 이 파싱). 매핑 불가한 키는
    /// plugin 이 무시한다.
    Key {
        key: String,
        pressed: bool,
        #[serde(default)]
        repeat: bool,
        #[serde(default)]
        modifiers: ModifiersWire,
    },
    /// 텍스트 입력 (IME 확정 포함).
    Text { text: String },
    /// IME 조합 라이프사이클(라이브 preedit + commit). egui `Event::Ime` 미러 —
    /// plugin SDK 가 `egui::Event::Ime(egui::ImeEvent::…)` 로 매핑한다. `Text` 는 조합이
    /// 끝난 최종 문자열만 나르지만, `Ime` 는 조합 중 preedit 문자열을 라이브로 전달해
    /// plugin 의 `TextEdit` 이 조합 중간 상태를 인라인 표시하게 한다.
    Ime { event: ImeWire },
    /// 호스트 단축키 설정에서 해석한 복사 요청. SDK는 egui::Event::Copy로
    /// 변환하고 egui가 선택 내용에 따라 CopyText 출력을 만든다.
    Copy,
    /// 형식 붙여넣기 신호. 호스트 단축키 설정에서 해석한 붙여넣기 요청이며 `egui_paste`
    /// kind 에만 보낸다. 클립보드 내용은 싣지 않는다 — plugin 이 필요한 형식(이미지 등)을
    /// 직접 읽는다. SDK 는 빈 문자열의 `egui::Event::Paste` 로 변환하므로 `TextEdit` 에는
    /// 효과가 없다. 텍스트 붙여넣기는 [`RawInputEventWire::PasteText`] 가 나른다.
    Paste,
    /// 호스트가 OS 클립보드에서 읽은 붙여넣기 텍스트. SDK 는 `egui::Event::Paste(text)` 로
    /// 변환해 포커스된 `TextEdit` 에 넣는다. 키 입력 문자인 [`RawInputEventWire::Text`] 와
    /// 달리 붙여넣기 의미를 유지한다.
    PasteText { text: String },
    /// 호스트 단축키 설정에서 해석한 잘라내기 요청. SDK 는 `egui::Event::Cut` 으로
    /// 변환하고, egui 가 선택을 지우며 `CopyText` 출력을 만든다.
    Cut,
    /// 미래 variant 의 forward-compat fallback — 이 변형을 아는 SDK 로 빌드한 plugin 은
    /// 모르는 입력 이벤트를 받아도 `surface.set_context` 전체를 실패하지 않고 그 이벤트만
    /// 버린다([`PluginEvent::Unknown`] 과 같은 정책: 새 variant 는 fallback 가능한 형태로만).
    /// SDK 는 egui 이벤트로 바꾸지 않고 무시한다.
    #[serde(other)]
    Unknown,
}

/// egui `ImeEvent` 미러 — IME 조합 세션의 4단계. `RawInputEventWire::Ime` 에 실린다.
/// winit `Ime` 의 preedit cursor range 는 egui `ImeEvent::Preedit(String)` 이 담지
/// 않으므로 여기서도 문자열만 나른다(candidate 위치는 host 가 별도로 관리).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "ime", rename_all = "snake_case")]
pub enum ImeWire {
    /// IME 활성화 알림.
    Enabled,
    /// 조합 중 preedit 후보 문자열(라이브 표시).
    Preedit { text: String },
    /// 조합이 이 최종 문자열로 확정됨.
    Commit { text: String },
    /// IME 비활성화 알림.
    Disabled,
}

/// 플러그인의 egui가 계산한 IME 입력 위젯과 캐럿 위치.
/// 좌표는 mesh 콘텐츠 영역 안의 논리 포인트다. 호스트가 창 좌표로 바꿔
/// OS의 IME 후보창 위치를 지정한다.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct ImeCursorWire {
    /// 편집 위젯(`TextEdit`) 전체가 차지하는 사각형.
    pub rect: RectWire,
    /// 주 캐럿의 사각형(아주 얇다).
    pub cursor_rect: RectWire,
}

/// plugin 배너가 이번 frame에 말줄임한 본문. 호스트가 배너 카드 아래 툴팁으로 전문을 보인다.
/// `body_rect`는 본문을 그린 사각형이며 좌표는 mesh 콘텐츠 영역 안의 논리 포인트다.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BannerBodyCutWire {
    pub text: String,
    pub body_rect: RectWire,
}

/// 논리 포인트 사각형 — 좌상단 (x, y) + 크기. egui `Rect` 미러.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct RectWire {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// `command.invoke` params — 사용자 단축키 매칭 시 호스트가 plugin에 보내는 명령.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandInvokeParams {
    pub surface_id: u32,
    pub command_id: String,
}

/// `webview.navigation_attempt` params — webview surface 가 시도한 navigation 의 URL.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebviewNavigationAttemptParams {
    pub surface_id: u32,
    pub url: String,
}

/// hook 호출이 이벤트인지 IPC인지 구분.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionHookKind {
    Event,
    Ipc,
}

/// hook이 pre인지 post인지.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionHookPhase {
    Pre,
    Post,
}

/// hook의 동작 모드. host는 mode에 따라 plugin 응답을 다르게 해석한다.
/// 매니페스트의 `HookMode`와 1:1 대응.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionHookMode {
    Transform,
    Filter,
    Observe,
}

/// `extension.invoke_hook` params — host가 extension plugin에 hook 호출을 위임.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionHookInvokeParams {
    /// hook 종류 (event/ipc).
    pub kind: ExtensionHookKind,
    pub phase: ExtensionHookPhase,
    pub mode: ExtensionHookMode,
    /// 매칭된 hook의 대상. `kind=event`면 event key, `kind=ipc`면 IPC method 이름.
    pub target: String,
    /// hook이 가공할 페이로드.
    /// - `kind=event`: envelope.payload
    /// - `kind=ipc, phase=pre`: 호출 params
    /// - `kind=ipc, phase=post`: 응답 result
    pub payload: serde_json::Value,
}

/// `extension.invoke_hook` 응답. mode별 의미:
///
/// - **transform**: `modified_payload`가 Some이면 host가 그 값으로 덮어쓴다.
///   None이면 원본 유지.
/// - **filter**: `pass`가 Some(false)면 host는 흐름을 차단한다.
///   None 또는 Some(true)면 통과. `modified_payload`는 무시.
/// - **observe**: 모든 필드 무시. plugin이 단순 관찰만 한 결과.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ExtensionHookResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<bool>,
}

// ── Shared buffer 메서드 (plugin → host via PluginEvent::IpcCall) ──
//
// 공유 메모리 생성은 호스트에 요청하며 핸들은 보조 채널로 받는다.
// 생성 크기는 현재 호스트의 create_shared_buffer_for에서 제한한다.

/// plugin → host: 새 공유 메모리 영역 생성 요청.
pub const METHOD_HOST_SHARED_BUFFER_CREATE: &str = "host.shared_buffer.create";
/// plugin → host: 변경된 영역(dirty rect) 통지.
pub const METHOD_HOST_SHARED_BUFFER_DIRTY: &str = "host.shared_buffer.dirty";

/// 호스트 관리자가 발급하는 공유 버퍼 ID. JSON에는 u64로 직렬화한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct SharedBufferId(pub u64);

/// 픽셀(또는 추후 다른 단위) 좌표계의 정수 사각형. shared buffer dirty 영역 표현에 사용.
///
/// 비어 있는 rect(`w == 0 || h == 0`)는 "갱신 없음"이 아니라 "유효하지 않음"으로 간주.
/// "전체 갱신"은 `Option<PixelRect>::None`으로 표현한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `host.shared_buffer.create` params.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedBufferCreateParams {
    /// 요청 영역 크기(바이트). 호스트의 크기 제한을 적용한다.
    pub size: u64,
}

/// `host.shared_buffer.create` result. 보조 채널로 핸들이 별도 전송된 *후* 메인 채널
/// 응답으로 이 값이 도착한다. plugin SDK는 두 정보가 모두 도착한 시점에 `SharedBuffer`를
/// 호출자에게 반환한다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedBufferCreateResult {
    pub id: SharedBufferId,
    /// 실제 매핑된 크기. 보통 요청한 size와 동일하나, OS가 페이지 경계로 올린 경우 size
    /// 자체는 요청값을 보존한다 (SharedMemory::len이 요청값을 반환).
    pub size: u64,
}

/// `host.shared_buffer.dirty` params.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedBufferDirtyParams {
    pub id: SharedBufferId,
    /// `None`이면 전체 영역이 dirty.
    #[serde(default)]
    pub rect: Option<PixelRect>,
}

/// 호스트 → plugin 요청.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginRequest {
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
    /// 이 요청이 큐에 들어가기 전까지 포화로 버린 요청 수. 0이면 필드를 생략한다.
    /// 포화된 큐에 별도 알림을 넣는 대신 다음으로 큐에 들어가는 요청에 싣는다.
    /// 이후 요청 전달도 실패하면 플러그인이 이 값을 받는다고 보장할 수는 없다.
    /// 필드가 없는 구버전 요청은 0으로 읽는다.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub dropped_requests: u64,
}

/// `#[serde(skip_serializing_if)]` 용 — 0 은 "버린 것이 없다" 라서 와이어에 안 싣는다.
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

impl PluginRequest {
    /// 요청을 만든다. dropped_requests는 0으로 두고 송신 시점에 채운다.
    pub fn new(method: impl Into<String>, params: serde_json::Value, id: u64) -> Self {
        Self {
            method: method.into(),
            params,
            id,
            dropped_requests: 0,
        }
    }
}

/// plugin → 호스트 응답.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginResponse {
    pub id: u64,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    /// JSON-RPC 에러 코드. 없으면 host가 -32000 (server error)으로 간주.
    /// 예: -32601 method not found, -32602 invalid params.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i32>,
}

/// plugin → 호스트 비동기 알림 (요청 응답이 아님).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginEvent {
    /// 매니페스트 검증 후 호스트가 받는 첫 메시지.
    Hello { plugin_id: String, version: String },
    /// 호스트에 surface의 다시 그리기를 요청한다.
    SurfaceInvalidated { surface_id: u32 },
    /// 호스트에 popup의 입력 없는 렌더링 갱신을 요청한다.
    PopupInvalidated { instance_id: u64 },
    /// 호스트에 banner의 입력 없는 렌더링 갱신을 요청한다.
    BannerInvalidated { instance_id: u64 },
    /// 공유 버퍼에 쓴 surface mesh의 버퍼 ID와 세대 등을 알린다.
    PaintFrame {
        surface_id: u32,
        buffer_id: SharedBufferId,
        /// 공유 버퍼 footer의 세대. 알림과 버퍼의 세대를 비교하는 값이며,
        /// 일치만으로 payload 읽기 중 다음 쓰기까지 배제하지는 못한다.
        generation: u64,
        /// 이 surface 렌더 코어가 지금까지 **송신한** frame 의 단조 증가 시퀀스(1부터).
        /// footer generation 과 달리 shared buffer 재생성(성장)과 무관하게 이어진다.
        /// host 는 `frame_seq == last_seq + 1` 로 textures_delta 체인의 연속성을 검증하고,
        /// 단절이면(latest-wins buffer 에서 중간 frame 관측 누락) full 재전송을 요청한다.
        /// 구버전 plugin 은 0 — host 는 체인 단절로 보고 full 재전송을 요청한다.
        #[serde(default)]
        frame_seq: u64,
        /// 이 frame 의 textures_delta 가 plugin 이 보유한 **전체 텍스처 상태**를 full
        /// image 로 담고 있는가 (첫 frame, 또는 host 의 `need_full_textures` 요청 응답).
        /// true 면 host 는 체인 연속성과 무관하게 수락하고 텍스처 상태를 리셋한다.
        #[serde(default)]
        full_textures: bool,
        /// 인코딩한 mesh 바이트 길이. 버퍼 할당 크기와 구분한다.
        /// Attach 전송은 이 길이를 사용하며 0이면 버퍼 전체 크기를 사용한다.
        #[serde(default)]
        byte_len: u32,
        /// 이 frame 을 그린 pass 의 egui `PlatformOutput::ime` — IME 를 원하는 위젯이
        /// focus 중이었다면 그 위치(콘텐츠 로컬 논리 포인트). host 가 OS IME 후보창
        /// 위치를 정하는 데 쓴다([`ImeCursorWire`]). 포커스된 편집 위젯이 없으면 `None`.
        #[serde(default)]
        ime_cursor: Option<ImeCursorWire>,
    },
    /// egui-mesh popup: plugin 이 popup 인스턴스용 mesh 를 commit 했음을 알린다.
    /// [`PluginEvent::PaintFrame`] 의 popup 대응 — surface_id 대신 host 발급
    /// `instance_id` 로 키잉한다. 본체(mesh 바이트)는 shared buffer 안에 있고, 이
    /// 알림은 어떤 buffer 의 어떤 generation 인지 메타만 운반한다.
    PopupPaintFrame {
        instance_id: u64,
        buffer_id: SharedBufferId,
        /// 공유 버퍼 footer에 기록한 세대.
        generation: u64,
        /// 송신 frame 단조 시퀀스 — [`PluginEvent::PaintFrame::frame_seq`] 와 동일 의미.
        #[serde(default)]
        frame_seq: u64,
        /// 전체 텍스처 상태 동봉 여부 — [`PluginEvent::PaintFrame::full_textures`] 와 동일 의미.
        #[serde(default)]
        full_textures: bool,
        /// [`PluginEvent::PaintFrame::ime_cursor`] 와 동일 의미 — popup 콘텐츠 영역 로컬.
        #[serde(default)]
        ime_cursor: Option<ImeCursorWire>,
    },
    /// egui-mesh banner(A3): plugin 이 banner 인스턴스용 mesh 를 commit 했음을 알린다.
    /// [`PluginEvent::PopupPaintFrame`] 의 banner 대응 — `instance_id` 로 키잉한다.
    /// 본체(mesh 바이트)는 shared buffer 안에 있고, 이 알림은 어떤 buffer 의 어떤
    /// generation 인지 메타만 운반한다.
    BannerPaintFrame {
        instance_id: u64,
        buffer_id: SharedBufferId,
        /// 공유 버퍼 footer에 기록한 세대.
        generation: u64,
        /// 송신 frame 단조 시퀀스 — [`PluginEvent::PaintFrame::frame_seq`] 와 동일 의미.
        #[serde(default)]
        frame_seq: u64,
        /// 전체 텍스처 상태 동봉 여부 — [`PluginEvent::PaintFrame::full_textures`] 와 동일 의미.
        #[serde(default)]
        full_textures: bool,
        /// 이 frame에서 말줄임한 본문. 없거나 구버전 plugin이면 `None`이고 호스트는 툴팁을 그리지 않는다.
        #[serde(default)]
        body_cut: Option<BannerBodyCutWire>,
    },
    /// 호스트 동작을 요청한다.
    NotifyHost {
        surface_id: u32,
        event: String,
        payload: serde_json::Value,
    },
    /// plugin 측 로그 (호스트 로그에 합쳐짐).
    Log { level: String, message: String },
    /// plugin → 호스트 IPC 호출. 호스트가 권한을 검사하고 라우터에 보낸 뒤,
    /// 결과를 `ipc.result` 요청으로 회신한다 (`call_id`로 매칭).
    IpcCall {
        call_id: u64,
        method: String,
        params: serde_json::Value,
    },
    /// plugin → 호스트: Event Bus에 이벤트 publish.
    /// 호스트가 권한(매니페스트 `event_publish` 패턴)을 검증하고 hop을 증가시킨 뒤
    /// 구독자에게 fan-out한다. fire-and-forget.
    EventPublish { envelope: EventEnvelope },
    /// plugin → 호스트: 이벤트 키 패턴 구독.
    /// `pattern`은 정확한 키 또는 `<namespace>.*` 와일드카드.
    /// 호스트는 매니페스트 `event_subscribe`로 허용된 패턴 안에 들어가는지 검증한다.
    EventSubscribe { sub_id: u64, pattern: String },
    /// plugin → 호스트: 이전 [`PluginEvent::EventSubscribe`]의 구독 해제.
    EventUnsubscribe { sub_id: u64 },
    /// plugin 이 자기 측 shared buffer 매핑을 폐기했음을 알린다 — 예: egui-mesh
    /// buffer 가 성장으로 재생성되어 구버퍼가 교체될 때(SDK `ensure_buffer`).
    /// host 는 대응 매핑(`plugin_buffers`)을 해제한다. 이 통지가 없으면 구세대
    /// 버퍼의 `SharedMemory` 매핑이 plugin 수명 내내 host 에 남는다.
    /// surface/popup/banner *닫힘* 경로는 host 가 frame 메타 기반으로 자체 해제
    /// 하므로, 이 이벤트는 인스턴스 생존 중 교체 시점용이다. fire-and-forget.
    SharedBufferReleased { id: SharedBufferId },
    /// 미래 variant 의 forward-compat fallback — 구버전 host 가 모르는 kind 를
    /// 받아도 메시지 단위 파싱 실패 없이 무시할 수 있게 한다 (CHANGELOG 정책:
    /// 새 variant 는 fallback 가능한 형태로만). host 는 debug 로그 후 버린다.
    #[serde(other)]
    Unknown,
}

/// [`METHOD_EVENT_DISPATCH`] params — 호스트가 구독자 plugin에게 보내는 이벤트.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventDispatchParams {
    /// plugin이 구독 등록 시 발급한 `sub_id`. 같은 plugin이 여러 패턴을 구독한 경우
    /// 어느 구독에 매칭됐는지 구분하기 위한 식별자.
    pub sub_id: u64,
    pub envelope: EventEnvelope,
}

/// `ipc.result` 요청의 params — plugin의 ipc.call에 대한 호스트의 응답.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IpcCallResult {
    pub call_id: u64,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    /// 호스트가 반환한 JSON-RPC 오류 코드. 없으면 SDK는 -32000으로 처리한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i32>,
}

/// plugin이 connection 직후 첫 줄로 보내는 인증 메시지.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthMessage {
    pub plugin_id: String,
    pub token: String,
}

/// 인증 메시지에 대한 호스트 응답. 메인 요청 루프 전에 같은 NDJSON 채널로 받는다.
/// ok가 false이면 SDK는 HandshakeRejected 오류를 반환한다.
/// 일반 요청과 구분하기 위해 auth_ack 키로 감싼다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthAck {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// AuthAck의 envelope wrapper. NDJSON 한 줄에 담기는 최상위 구조.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthAckEnvelope {
    pub auth_ack: AuthAck,
}

// ── 보조 핸들 채널 ──
//
// 메인 TCP 채널과 별도로 공유 메모리 핸들을 전달한다. Unix는 AF_UNIX와
// SCM_RIGHTS, Windows는 Named Pipe와 DuplicateHandle을 사용한다.
// 인증 형식과 토큰은 메인 채널과 같다.

/// 보조 채널의 NDJSON 메시지. Unix에서는 HandleAttach 메시지와 fd를
/// 같은 sendmsg로 보내고, Windows에서는 복제된 HANDLE 값을 메시지에 넣는다.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HandleChannelMessage {
    /// 살아있는지 확인용. host 또는 plugin 어느 쪽이든 보낼 수 있고, 받은 쪽은 동일한
    /// `seq`로 [`HandleChannelMessage::Pong`]을 응답한다.
    Ping { seq: u64 },
    /// [`HandleChannelMessage::Ping`]의 응답.
    Pong { seq: u64 },
    /// host → plugin: 새로 만든 shared buffer 핸들을 plugin에 전달한다.
    /// `request_id`는 메인 채널의 `host.shared_buffer.create` call_id와 1:1 매칭.
    ///
    /// **Unix**: 핸들은 이 NDJSON 라인과 같은 `sendmsg`의 ancillary data(SCM_RIGHTS)로
    /// fd가 동행하며, `handle` 필드는 `None`이다.
    /// **Windows**: `DuplicateHandle`이 plugin 프로세스 핸들 테이블에 이미 복제해 넣은
    /// HANDLE u64 값을 `handle` 필드에 in-band로 실어 보낸다(ancillary data 없음).
    HandleAttach {
        /// 메인 채널 `host.shared_buffer.create` 요청의 call_id.
        request_id: u64,
        /// 호스트가 부여한 shared buffer id.
        id: SharedBufferId,
        /// 매핑 크기. SDK가 `tasty_shm::receive`에 그대로 넣는다.
        size: u64,
        /// Windows 전용: plugin 핸들 테이블에 복제된 HANDLE u64. Unix는 `None`(fd는
        /// ancillary data로 도착). `skip_serializing_if`로 Unix wire를 그대로 유지한다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handle: Option<u64>,
    },
    /// plugin → host: 특정 buffer의 dirty 영역을 통지. fire-and-forget.
    Dirty {
        /// 어떤 buffer가 dirty한지.
        id: SharedBufferId,
        /// `None`이면 전체 영역.
        #[serde(default)]
        rect: Option<PixelRect>,
    },
}

// ── Popup wire types ──

/// 팝업 열기 요청. 호스트가 발급한 instance_id로 같은 팝업의 여러 인스턴스를 구분한다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PopupOpenParams {
    pub popup_id: String,
    pub instance_id: u64,
    /// 호스트가 trigger 시점에 알 수 있던 컨텍스트(예: trigger event payload). plugin이
    /// 초기 트리 구성에 활용할 수 있다. 없으면 Null.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// 팝업 열기 응답. 콘텐츠는 mesh로 전달하므로 이 결과에는 담지 않는다.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PopupOpenResult {}

/// `popup.closed` params — popup 인스턴스가 닫혔음을 plugin에 통보. fire-and-forget.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PopupClosedParams {
    pub instance_id: u64,
    /// 닫힌 이유. 텍스트는 호스트가 결정한 카테고리.
    pub reason: PopupCloseReason,
}

/// `popup.set_context` params — egui-mesh popup 인스턴스의 렌더 컨텍스트.
///
/// [`SurfaceSetContextParams`] 의 popup 대응이다 — surface_id 대신 host 가 발급한
/// `instance_id` 로 popup 인스턴스를 식별한다. 나머지 필드(크기/ppp/raw_input)와
/// 좌표 규약(surface-local 논리 포인트, 좌상단 0,0)은 surface 와 동일하다.
///
/// 사용자·에이전트 경계(docs/design/policies/focus.md): set_context 는 host 가 받은
/// *실제* 사용자 입력만 forward 한다. 에이전트 IPC/CLI 가 raw_input 을 합성·주입하는 진입로는 만들지 않는다.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PopupSetContextParams {
    /// host 가 발급한 popup 인스턴스 식별자.
    pub instance_id: u64,
    /// popup 콘텐츠 영역의 물리 픽셀 너비.
    pub width_px: u32,
    /// popup 콘텐츠 영역의 물리 픽셀 높이.
    pub height_px: u32,
    /// 논리→물리 스케일 (egui `ScreenDescriptor.pixels_per_point`).
    pub pixels_per_point: f32,
    /// 이번 frame 의 사용자 입력.
    #[serde(default)]
    pub raw_input: RawInputWire,
    /// host 가 resolve 한 현재 Theme 스냅샷 (egui-mesh popup 의 Theme parity).
    /// `None` 이면 plugin 은 직전 값을 유지하거나 자체 기본값으로 그린다. host 는
    /// 크기/ppp/입력 변경뿐 아니라 **테마 변경 시에도** 이 값을 갱신해 재forward 한다.
    /// [`SurfaceSetContextParams::theme`] 와 동형 — 모든 egui-mesh popup(git-viewer/
    /// clipboard-viewer 등)이 공유하는 generic 필드.
    #[serde(default)]
    pub theme: Option<ThemeWire>,
    /// host 의 텍스처 상태 복구 요청 —
    /// [`SurfaceSetContextParams::need_full_textures`] 와 동일 의미.
    #[serde(default)]
    pub need_full_textures: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupCloseReason {
    /// 사용자가 popup 바깥을 클릭했고 매니페스트가 dismiss_on_outside_click=true.
    OutsideClick,
    /// 사용자가 Esc 키를 눌렀음.
    Escape,
    /// plugin이 host IPC `popup.close`로 닫기 요청.
    PluginRequest,
    /// 호스트 측에서 강제 닫힘 (plugin disable / unload 등).
    HostShutdown,
}

// ── Banner wire types (A3) ──

/// `banner.open` params — 호스트가 plugin 에게 banner 인스턴스를 열도록 요청.
///
/// [`PopupOpenParams`] 의 banner 대응. `instance_id` 는 호스트가 발급한 인스턴스
/// 식별자로, 같은 (plugin_id, banner_id) 의 중복 인스턴스를 host 가 dedup 한다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BannerOpenParams {
    pub banner_id: String,
    pub instance_id: u64,
    /// 호스트가 trigger 시점에 알 수 있던 컨텍스트. 없으면 Null.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// 배너 열기 응답. 콘텐츠는 mesh로 전달하므로 이 결과에는 담지 않는다.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BannerOpenResult {}

/// `banner.closed` params — banner 인스턴스가 닫혔음을 plugin 에 통보. fire-and-forget.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BannerClosedParams {
    pub instance_id: u64,
    /// 닫힌 이유. 텍스트는 호스트가 결정한 카테고리.
    pub reason: BannerCloseReason,
}

/// `banner.set_context` params — egui-mesh banner 인스턴스의 렌더 컨텍스트.
///
/// [`PopupSetContextParams`] 의 banner 대응이다 — surface_id/popup instance 대신 host 가
/// 발급한 banner `instance_id` 로 식별한다. 나머지 필드(크기/ppp/raw_input/theme)와
/// 좌표 규약(banner content-local 논리 포인트, 좌상단 0,0)은 popup 과 동일하다.
///
/// banner 는 non-modal 공지라 scrim/키보드 포커스가 없다 — content 영역 위 포인터/스크롤
/// 입력만 forward 된다(host 합성기가 content_rect 로 한정). 사용자·에이전트 경계:
/// set_context 는 host 가 받은 *실제* 사용자 입력만 forward 한다. 에이전트 IPC/CLI 가
/// raw_input 을 합성·주입하거나 배너를 강제 표시하는 진입로는 만들지 않는다.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BannerSetContextParams {
    /// host 가 발급한 banner 인스턴스 식별자.
    pub instance_id: u64,
    /// banner 콘텐츠 영역의 물리 픽셀 너비.
    pub width_px: u32,
    /// banner 콘텐츠 영역의 물리 픽셀 높이.
    pub height_px: u32,
    /// 논리→물리 스케일 (egui `ScreenDescriptor.pixels_per_point`).
    pub pixels_per_point: f32,
    /// 이번 frame 의 사용자 입력.
    #[serde(default)]
    pub raw_input: RawInputWire,
    /// host 가 resolve 한 현재 Theme 스냅샷 (egui-mesh banner 의 Theme parity).
    /// `None` 이면 plugin 은 직전 값을 유지하거나 자체 기본값으로 그린다. host 는
    /// 크기/ppp/입력 변경뿐 아니라 **테마 변경 시에도** 이 값을 갱신해 재forward 한다.
    /// [`PopupSetContextParams::theme`] 와 동형 — 모든 egui-mesh banner 가 공유한다.
    #[serde(default)]
    pub theme: Option<ThemeWire>,
    /// host 의 텍스처 상태 복구 요청 —
    /// [`SurfaceSetContextParams::need_full_textures`] 와 동일 의미.
    #[serde(default)]
    pub need_full_textures: bool,
}

/// 배너를 닫은 사유. 배너는 바깥 클릭이나 Esc로 닫지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerCloseReason {
    /// TTL 카운트다운이 0 에 도달해 자동 소멸.
    Ttl,
    /// 사용자가 셸 우상단 close X 를 눌렀음.
    UserClose,
    /// plugin 이 host IPC `banner.close` 로 닫기 요청.
    PluginRequest,
    /// 호스트 측에서 강제 닫힘 (plugin disable / unload 등).
    HostShutdown,
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
