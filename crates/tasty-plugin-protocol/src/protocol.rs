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
/// hop 이 올라가므로, SDK 는 `on_event` 가 돌아온 뒤에 응답한다(ADR-0406).
pub const METHOD_EVENT_DISPATCH: &str = "event.dispatch";
/// host → plugin: 사용자 단축키 매칭으로 plugin command가 트리거됨.
/// params에 [`CommandInvokeParams`]. plugin은 그에 따라 surface state를 변경하고,
/// 변경 결과는 `SurfaceResult` 형태로 응답한다 (display_name/snapshot 갱신).
pub const METHOD_COMMAND_INVOKE: &str = "command.invoke";
/// host → plugin: webview(`rendering = "webview"`) surface 가 네비게이션을 시도했다
/// (링크 클릭 등). `webview.set_url`(plugin→host)의 반대 방향 — "원격 http(s) 차단"
/// 판정과는 독립적으로, 차단 여부와 무관하게 모든 navigation 시도(로컬 파일 링크
/// 포함)마다 발사되는 fire-and-forget 통지다. 정책 판단(차단 여부)은 host 가 하고,
/// 그 결과로 열지 말지는 plugin 이 이 URL 을 보고 스스로 라우팅한다(예: 로컬 파일
/// 링크는 `file_handler.dispatch`로, 외부 URL 은 OS open 으로). params 에
/// [`WebviewNavigationAttemptParams`]. plugin 은 응답하지 않는다(host 가 무시).
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

// ── egui-mesh surface: set_context (host → plugin) wire types ──
//
// epaint 의 `serde` feature 가 꺼져 있어 egui `RawInput` 을 그대로 직렬화할 수 없다.
// 따라서 plugin 프로세스가 egui 입력을 재구성하는 데 필요한 필드만 추린 POD-friendly
// 미러 타입을 둔다. plugin SDK(A1-S4)가 [`RawInputWire`] 를 `egui::RawInput` 으로 매핑한다.
// 이 타입들은 egui 의존이 없어 default(=egui-mesh feature off) 빌드에도 포함된다.
//
// identity 경계(원칙 1·3): set_context 는 host 가 받은 *실제* 사용자 입력을 surface
// 영역으로 forward 하는 경로만 담는다. 에이전트 IPC/CLI 가 raw_input 을 합성·주입하는
// 진입로는 만들지 않는다 (release 에 없음; debug 주입이 필요하면 debug 격리).

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
    /// host 가 resolve 한 현재 Theme 스냅샷 (egui-mesh plugin 의 Theme parity).
    /// `None` 이면 plugin 은 직전 값을 유지하거나 자체 기본값으로 그린다. host 는
    /// 크기/ppp/입력 변경뿐 아니라 **테마 변경 시에도** 이 값을 갱신해 재forward 한다.
    /// generic 필드 — 모든 egui-mesh surface(markdown/git-viewer 등)가 공유한다.
    #[serde(default)]
    pub theme: Option<ThemeWire>,
    /// host 의 텍스처 상태 복구 요청. true 면 plugin SDK 는 출력 dedup 을 우회하고,
    /// 보유한 **전체 텍스처 상태**(font atlas 포함 임의 Managed 텍스처 전부)를 full
    /// image delta 로 재구성해 동봉한 frame 을 강제 송신한다(`full_textures = true` 로
    /// 마킹). host 는 자기 텍스처 상태가 불완전할 때(신규 Renderer / frame_seq 체인
    /// 단절)만 이 플래그를 세운다.
    #[serde(default)]
    pub need_full_textures: bool,
}

/// host 가 resolve 한 Theme 을 프로세스 경계 너머로 운반하는 POD 스냅샷.
///
/// egui 의존 없이 직렬화 가능한 색 집합([`ThemeColors`](tasty_type_appearance::theme::ThemeColors))
/// + `is_light` + host UI zoom 만 담는다. plugin 은 이를
/// [`Theme::with_colors_and_zoom`](tasty_type_appearance::theme::Theme::with_colors_and_zoom)
/// 로 풀어 host 와 동일한 Theme 인스턴스를 재구성한다 (sizing 은 zoom 으로 재도출).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ThemeWire {
    /// resolved (사용자 override 반영) 색 집합. zoom 독립적.
    pub colors: tasty_type_appearance::theme::ThemeColors,
    /// 라이트/다크 — hover/active/separator overlay 도출에 필요.
    pub is_light: bool,
    /// host UI zoom 배율. sizing token 에 곱해진다 (`with_colors_and_zoom`).
    pub ui_zoom: f32,
}

/// egui `RawInput` 의 직렬화 가능한 최소 미러. markdown 검증엔 pointer+scroll+key 로 충분
/// (research-a1 §2-3). IME/터치 등 부족분은 후속 단계에서 확장.
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
    /// 복사 단축키(host keybinding `copy` 매칭). 물리 키가 아니라 host 가 그 키를
    /// 의미론적으로 해석한 결과 — egui-winit 이 플랫폼 Ctrl+C 를 `Event::Copy` 로
    /// 변환해 넘기는 것과 동일한 host/platform-integration 역할이다. plugin SDK 가
    /// `egui::Event::Copy` 로 매핑하면, plugin 자신의 텍스트 선택(selectable label /
    /// `TextEdit`)이 있을 때 egui 가 `platform_output.commands` 에 `OutputCommand::CopyText`
    /// 를 채운다(옛 `PlatformOutput::copied_text` 필드는 deprecated).
    Copy,
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

/// egui [`IMEOutput`] 미러 — plugin 프로세스의 egui 가 매 pass 계산하는 "IME 를 원하는
/// 위젯이 지금 어디에 있는가". [`PluginEvent::PaintFrame`] · [`PluginEvent::PopupPaintFrame`]
/// 에 실려 **plugin → host** 방향으로 돌아온다(입력 와이어의 역방향).
///
/// 좌표는 그 mesh 콘텐츠 영역 로컬 논리 포인트(좌상단 0,0) — 입력 와이어의 포인터 좌표와
/// 같은 계다. host 가 콘텐츠 영역 origin 을 더해 창 좌표로 바꾼 뒤 winit
/// `set_ime_cursor_area` 로 OS 에 알린다. OS IME 후보창은 plugin 프로세스가 그리는 mesh
/// 밖(OS 소유 창)이라 plugin 이 직접 위치를 정할 수 없고, host 만 winit 창을 쥐고 있다.
///
/// [`IMEOutput`]: https://docs.rs/egui/0.31/egui/output/struct.IMEOutput.html
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct ImeCursorWire {
    /// 편집 위젯(`TextEdit`) 전체가 차지하는 사각형.
    pub rect: RectWire,
    /// 주 캐럿의 사각형(아주 얇다).
    pub cursor_rect: RectWire,
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
// plugin이 OS 공유 메모리 영역을 만들고 dirty rect를 알릴 때 사용한다. 실제 핸들
// (fd/HANDLE) 전송은 *보조 채널*을 통해 이루어지고, 이 메인 채널 메서드는 id/size/
// rect 같은 메타데이터만 운반한다. 보조 채널 wire 포맷은 아래 [`HandleChannelMessage`]
// 절이 정의한다.
//
// 권한: manifest의 `[memory]` 섹션에 `max_shared_buffer_bytes`가 선언된 plugin만
// 호출 가능. 미선언 plugin이 호출하면 호스트가 -32001 PermissionDenied 응답.

/// plugin → host: 새 공유 메모리 영역 생성 요청.
pub const METHOD_HOST_SHARED_BUFFER_CREATE: &str = "host.shared_buffer.create";
/// plugin → host: 변경된 영역(dirty rect) 통지.
pub const METHOD_HOST_SHARED_BUFFER_DIRTY: &str = "host.shared_buffer.dirty";

/// 호스트가 발급한 shared buffer 식별자. plugin 인스턴스마다 단조 증가.
/// u64를 옵셔널 직렬화 호환을 위해 직접 직렬화.
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
    /// 요청 영역 크기 (바이트). manifest의 max_shared_buffer_bytes를 초과하면 거부.
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
    /// 이 요청 **직전까지** 호스트가 같은 plugin 에게 보내려다 큐 포화로 **버린**
    /// 요청 수. 0 이면 버린 것이 없고, 그때는 와이어에 키 자체가 안 실린다.
    ///
    /// 호스트 → plugin 큐는 유한하고 포화 시 대기가 아니라 거절이다(그 방향에서
    /// 기다리면 호스트 프레임이 선다 — ADR-0315). 그래서 plugin 은 자기에게 오던
    /// 요청이 소리 없이 사라지는 것을 원리적으로 알 수 없었다. 이 필드가 그것을
    /// 알린다.
    ///
    /// **별도 통지 메시지가 아닌 이유**: 포화를 알리는 메시지도 같은 큐를 써야 하고,
    /// 그 큐가 찼기 때문에 통지가 생긴 것이라 자기모순이다. 대신 **다음으로 실제
    /// 큐에 들어가는 요청**에 얹는다 — 큐에 자리가 났다는 것은 plugin 이 하나라도
    /// 소비했다는 뜻이므로, 살아서 밀리는 plugin 은 반드시 이 값을 본다. 전혀
    /// 소비하지 않는 plugin 은 ping 도 못 받아 healthcheck 가 거둔다.
    ///
    /// 구버전 호스트가 보낸 줄에는 이 키가 없고 `0` 으로 읽힌다.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub dropped_requests: u64,
}

/// `#[serde(skip_serializing_if)]` 용 — 0 은 "버린 것이 없다" 라서 와이어에 안 싣는다.
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

impl PluginRequest {
    /// 호스트 본문이 요청을 만드는 유일한 입구.
    ///
    /// [`PluginRequest::dropped_requests`] 는 **송신 지점**이 채운다(호스트의
    /// `PluginProcess::try_send_request`). 본문이 그 값을 알 방법이 없고 알 필요도
    /// 없으므로, 리터럴로 짓는 대신 이 생성자로 짓는다 — 필드가 하나 더 늘어도
    /// 호출부가 안 움직이고, 새 호출부가 그 필드의 존재를 몰라도 틀리지 않는다.
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
    /// surface invalidated — 호스트가 다음 프레임에 redraw (단계 06).
    SurfaceInvalidated { surface_id: u32 },
    /// egui-mesh popup invalidated — [`PluginEvent::SurfaceInvalidated`] 의 popup
    /// 대응. plugin 이 out-of-band 로(예: egui `viewport_output` 의 self-repaint
    /// 요청) 재-forward 를 요청할 때 쓴다. host 는 다음 tick 에 해당
    /// instance 의 `popup.set_context` 를 무입력으로 1회 재forward 한다.
    PopupInvalidated { instance_id: u64 },
    /// egui-mesh banner invalidated — [`PluginEvent::PopupInvalidated`] 의 banner
    /// 대응. 같은 이유로 존재한다: banner 도 `EguiMeshCore` 를 공유하므로 egui 가
    /// `viewport_output` 으로 다음 pass 를 요청할 수 있고(hover fade·스크롤 스무딩·
    /// 스피너), 그 요청을 여기로 올리지 않으면 다음 geom/입력/theme 변경까지 화면이
    /// 멈춘다. host 는 다음 tick 에 해당 instance 의 `banner.set_context` 를 무입력으로
    /// 1회 재forward 한다.
    BannerInvalidated { instance_id: u64 },
    /// egui-mesh surface: plugin 이 자기 프로세스에서 tessellate→POD 인코드(A1-S2,
    /// [`crate::mesh_wire`])한 mesh 바이트를 shared buffer 에 commit 했음을 알린다.
    /// mesh 본체는 buffer 안에 있고(`decode_paint` 로 복원), 이 알림은 어떤 buffer 의
    /// 어떤 generation 인지 메타만 운반한다 — Canvas dirty 알림과 동급의 경량 알림이다.
    /// 정적 화면은 invalidate 시에만 보내므로, host 는 generation 비교로 재합성을 건너뛴다.
    PaintFrame {
        surface_id: u32,
        buffer_id: SharedBufferId,
        /// plugin 이 commit 한 shared buffer footer generation. host 는 footer 를
        /// Acquire-load 해 일치/최신 여부를 검증한다 (tear 방지).
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
        /// `mesh_wire::encode_paint` 가 실제로 만든 바이트 길이. shared buffer 는
        /// `size.next_power_of_two()` 로 할당돼 뒤쪽에 이전 frame 의 잔여(trailing
        /// capacity) 바이트가 남을 수 있다 — 로컬(같은 프로세스) GPU 디코드는
        /// self-terminating 파싱이라 이를 무시하지만, attach mesh mirror 가 buffer
        /// 를 네트워크로 그대로 내보낼 때는 정확한 payload 경계가 필요하다(attach
        /// mesh mirror가 소비). 0 이면 구버전 plugin — attach 쪽은 버퍼 전체
        /// capacity 를 fallback 으로 쓴다.
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
        /// plugin 이 commit 한 shared buffer footer generation (tear 방지).
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
        /// plugin 이 commit 한 shared buffer footer generation (tear 방지).
        generation: u64,
        /// 송신 frame 단조 시퀀스 — [`PluginEvent::PaintFrame::frame_seq`] 와 동일 의미.
        #[serde(default)]
        frame_seq: u64,
        /// 전체 텍스처 상태 동봉 여부 — [`PluginEvent::PaintFrame::full_textures`] 와 동일 의미.
        #[serde(default)]
        full_textures: bool,
    },
    /// host action 트리거 (단계 06).
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
    /// 호스트가 준 JSON-RPC 오류 코드. 없으면 SDK 가 server error(-32000)로 본다.
    ///
    /// **반대 방향([`PluginResponse::error_code`])에는 처음부터 있었다.** 이쪽만 없어서
    /// 호스트가 "인자를 고쳐라"(`-32602`)로 거절한 것이 plugin 을 거쳐 나오면
    /// "서버 사정"(`-32000`)이 됐다 — 호출자가 다음에 할 일이 반대로 바뀐다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i32>,
}

/// plugin이 connection 직후 첫 줄로 보내는 인증 메시지.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthMessage {
    pub plugin_id: String,
    pub token: String,
}

/// 호스트 → plugin 인증 단계 전용 ack. plugin이 [`AuthMessage`]를 보낸 뒤
/// 메인 메시지 루프에 진입하기 전, **단 한 번** 같은 NDJSON 채널로 수신한다.
///
/// `ok=false`이면 plugin SDK가 [`crate::PluginError::HandshakeRejected`](
/// 같은 이름의 SDK variant)로 즉시 실패한다. 호스트 측은
/// `crates/tasty-host-plugin/src/listener.rs`에서 토큰 검증 결과에 따라 송신한다.
///
/// envelope: `{"auth_ack": { "ok": true }}` 또는 `{"auth_ack": { "ok": false, "reason": "..." }}`.
/// 메인 루프의 `PluginRequest`와 다른 envelope를 사용해 파서 분리.
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
// 메인 TCP 채널은 fd/HANDLE을 운반할 수 없으므로 보조 채널을 별도로 둔다. Unix는
// AF_UNIX (SCM_RIGHTS 가능), Windows는 Named Pipe (DuplicateHandle 가능). 이 채널의
// wire 포맷은 NDJSON이며, NDJSON 한 줄 직후 OS-네이티브 ancillary data로
// 핸들을 함께 전송한다.
//
// 인증 단계는 메인 채널의 [`AuthMessage`] / [`AuthAckEnvelope`]를 그대로 재사용한다 —
// endpoint가 다르므로 채널 라우팅 혼선 위험이 없고, 토큰은 동일한 plugin spawn 토큰이다.

/// 보조 채널 위에서 양쪽이 주고받는 NDJSON 메시지.
///
/// 02b에서는 ping/pong만 정의됐고, 02c에서 `HandleAttach`(host → plugin: 새 buffer
/// 핸들의 메타)와 `Dirty`(plugin → host: dirty rect 알림)가 추가됐다.
///
/// `HandleAttach`는 NDJSON 한 줄 *직후* OS-네이티브 ancillary data(SCM_RIGHTS / 직렬화된
/// HANDLE)를 함께 전송한다 — 같은 sendmsg/write 호출 안에 묶여 전달되어야 plugin이 핸들과
/// 메타를 일관되게 짝지을 수 있다.
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

/// `popup.open` params — 호스트가 plugin에게 popup 인스턴스를 열도록 요청.
///
/// `instance_id`는 호스트가 발급한 인스턴스 식별자로, 같은 popup_id의 여러
/// 인스턴스를 구분하기 위한 키. plugin은 응답으로 초기 트리를 [`PopupOpenResult`]에
/// 담아 돌려준다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PopupOpenParams {
    pub popup_id: String,
    pub instance_id: u64,
    /// 호스트가 trigger 시점에 알 수 있던 컨텍스트(예: trigger event payload). plugin이
    /// 초기 트리 구성에 활용할 수 있다. 없으면 Null.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// `popup.open` 응답. egui-mesh popup 은 콘텐츠를 mesh 채널로 그리므로 별도
/// 콘텐츠 필드가 없다 (빈 결과 — 향후 확장 여지용 struct 존치).
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
/// identity 경계(원칙 1·3): set_context 는 host 가 받은 *실제* 사용자 입력만 forward
/// 한다. 에이전트 IPC/CLI 가 raw_input 을 합성·주입하는 진입로는 만들지 않는다.
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

/// `banner.open` 응답.
///
/// egui-mesh banner 는 tree 가 아니라 mesh 채널([`METHOD_BANNER_SET_CONTEXT`])로
/// 콘텐츠를 그리므로 초기 tree 를 담지 않는다 (빈 결과). popup 의 [`PopupOpenResult`] 와
/// 평행하되, banner 는 UiTree 렌더링을 (아직) 지원하지 않아 필드가 없다.
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
/// 입력만 forward 된다(host 합성기가 content_rect 로 한정). identity 경계(원칙 1·3):
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

/// banner 인스턴스가 닫힌 이유. popup 과 달리 outside-click/Esc 가 없다(non-modal, D3).
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
