//! Plugin trait — plugin 작성자가 구현하는 진입점.
//!
//! 사용 예:
//!
//! ```ignore
//! struct MyExplorer;
//!
//! impl Plugin for MyExplorer {
//!     fn id(&self) -> &str { "com.example.explorer" }
//!     fn version(&self) -> &str { "0.1.0" }
//!     fn surface_kinds(&self) -> Vec<&str> { vec!["explorer"] }
//!
//!     fn create_surface(&mut self, ctx: SurfaceCreateCtx) -> SurfaceResult {
//!         SurfaceResult { display_name: Some("Files".into()), ..Default::default() }
//!     }
//! }
//!
//! fn main() -> anyhow::Result<()> {
//!     tasty_plugin_sdk::run(MyExplorer)
//! }
//! ```

use std::path::PathBuf;

use serde_json::Value;
pub use tasty_plugin_protocol::PopupOpenResult;
pub use tasty_plugin_protocol::SurfaceResult;

use crate::host::HostHandle;

#[derive(Debug, Clone)]
pub struct SurfaceCreateCtx {
    pub surface_id: u32,
    pub kind: String,
    /// 호스트가 source surface 로부터 carry 한 시작 cwd. None 이면 plugin 측에서
    /// params 우선순위 또는 자체 fallback (예: home dir) 으로 결정.
    /// Surface cwd invariant — `docs/design/policies/cwd.md#surface-cwd-invariant`.
    pub cwd: Option<PathBuf>,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub struct SurfaceRestoreCtx {
    pub surface_id: u32,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct SurfaceSnapshotCtx {
    pub surface_id: u32,
}

/// `surface.set_context` 콜백 컨텍스트 — egui-mesh surface 의 렌더 컨텍스트가 도착했을 때.
///
/// plugin 은 [`EguiMeshSurface`](crate::EguiMeshSurface)(egui-mesh feature) 를 들고
/// `surface.paint(&ctx.host, &ctx.params, |egui_ctx| { ... })` 를 호출해 mesh 를 회신한다.
/// `params.raw_input` 의 좌표는 surface-local 논리 포인트(좌상단 0,0)다.
#[derive(Clone)]
pub struct SurfaceSetContextCtx {
    /// surface 의 크기(물리 px)/ppp/이번 frame 의 사용자 입력.
    pub params: tasty_plugin_protocol::SurfaceSetContextParams,
    /// `PaintFrame` 알림 송신 및 shared buffer 생성에 쓰는 호스트 핸들. `clone()` 해
    /// 자기 스레드로 옮길 수 있다.
    pub host: HostHandle,
}

impl std::fmt::Debug for SurfaceSetContextCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceSetContextCtx")
            .field("params", &self.params)
            .field("host", &"HostHandle { .. }")
            .finish()
    }
}

/// `popup.set_context` 콜백 컨텍스트 — egui-mesh popup 인스턴스의 렌더 컨텍스트가 도착했을 때.
///
/// plugin 은 [`EguiMeshPopup`](crate::EguiMeshPopup)(egui-mesh feature) 를 들고
/// `popup.paint(&ctx.host, &ctx.params, |egui_ctx| { ... })` 를 호출해 mesh 를 회신한다.
/// `params.raw_input` 의 좌표는 popup 콘텐츠 영역 기준 논리 포인트(좌상단 0,0)다.
#[derive(Clone)]
pub struct PopupSetContextCtx {
    /// popup 콘텐츠 영역의 크기(물리 px)/ppp/이번 frame 의 사용자 입력.
    pub params: tasty_plugin_protocol::PopupSetContextParams,
    /// `PopupPaintFrame` 알림 송신 및 shared buffer 생성에 쓰는 호스트 핸들.
    pub host: HostHandle,
}

impl std::fmt::Debug for PopupSetContextCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopupSetContextCtx")
            .field("params", &self.params)
            .field("host", &"HostHandle { .. }")
            .finish()
    }
}

/// 매니페스트 `[[contributes.commands]]`로 등록한 command가 사용자 단축키
/// 매칭으로 호출됐을 때 전달되는 컨텍스트.
#[derive(Debug, Clone)]
pub struct CommandInvokeCtx {
    pub surface_id: u32,
    pub command_id: String,
}

/// `webview.navigation_attempt` 콜백 컨텍스트 — webview(`rendering = "webview"`) surface
/// 가 navigation 을 시도(링크 클릭 등)함. "원격 http(s) 차단" 판정과는 독립적으로, 호스트가
/// 차단했는지 여부와 무관하게 모든 시도(로컬 파일 링크 포함)마다 도착한다. fire-and-forget
/// 이므로 반환값 없음.
#[derive(Debug, Clone)]
pub struct WebviewNavigationAttemptCtx {
    pub surface_id: u32,
    pub url: String,
}

/// `popup.open` 콜백 컨텍스트.
#[derive(Debug, Clone)]
pub struct PopupOpenCtx {
    /// 매니페스트 `[[contributes.popup]]`의 `id` (plugin 내 로컬 id).
    pub popup_id: String,
    /// 같은 popup_id의 여러 인스턴스를 구분하기 위해 호스트가 발급한 id.
    pub instance_id: u64,
    /// trigger 시점에 호스트가 알 수 있던 컨텍스트 (event payload 등). 없으면 Null.
    pub context: Value,
}

/// `popup.closed` 콜백 컨텍스트. fire-and-forget이므로 반환값 없음.
#[derive(Debug, Clone)]
pub struct PopupClosedCtx {
    pub instance_id: u64,
    pub reason: tasty_plugin_protocol::PopupCloseReason,
}

/// `banner.open` 콜백 컨텍스트(A3).
#[derive(Debug, Clone)]
pub struct BannerOpenCtx {
    /// 매니페스트 `[[contributes.banner]]`의 `id` (plugin 내 로컬 id).
    pub banner_id: String,
    /// 호스트가 발급한 banner 인스턴스 식별자.
    pub instance_id: u64,
    /// trigger 시점에 호스트가 알 수 있던 컨텍스트. 없으면 Null.
    pub context: Value,
}

/// `banner.set_context` 콜백 컨텍스트(A3) — egui-mesh banner 인스턴스의 렌더 컨텍스트.
///
/// plugin 은 [`EguiMeshBanner`](crate::EguiMeshBanner) 를 들고
/// `banner.paint(&ctx.host, &ctx.params, |egui_ctx| { ... })` 를 호출해 mesh 를 회신한다.
/// `params.raw_input` 의 좌표는 banner content 영역 기준 논리 포인트(좌상단 0,0)다.
/// banner 는 non-modal 이라 키보드 포커스가 없어 포인터/스크롤 입력만 도착한다.
#[derive(Clone)]
pub struct BannerSetContextCtx {
    /// banner content 영역의 크기(물리 px)/ppp/이번 frame 의 사용자 입력/theme.
    pub params: tasty_plugin_protocol::BannerSetContextParams,
    /// `BannerPaintFrame` 알림 송신 및 shared buffer 생성에 쓰는 호스트 핸들.
    pub host: HostHandle,
}

impl std::fmt::Debug for BannerSetContextCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BannerSetContextCtx")
            .field("params", &self.params)
            .field("host", &"HostHandle { .. }")
            .finish()
    }
}

/// `banner.closed` 콜백 컨텍스트(A3). fire-and-forget이므로 반환값 없음.
#[derive(Debug, Clone)]
pub struct BannerClosedCtx {
    pub instance_id: u64,
    pub reason: tasty_plugin_protocol::BannerCloseReason,
}

/// `extension.invoke_hook` 컨텍스트. extension plugin이 hook 호출을 받았을 때.
#[derive(Clone)]
pub struct ExtensionHookCtx {
    /// event(이벤트 발화 가로채기) / ipc(IPC 호출 가로채기).
    pub kind: tasty_plugin_protocol::ExtensionHookKind,
    /// pre(흐름 시작 전) / post(흐름 종료 후).
    pub phase: tasty_plugin_protocol::ExtensionHookPhase,
    /// transform(payload 수정) / filter(차단 결정) / observe(관찰).
    pub mode: tasty_plugin_protocol::ExtensionHookMode,
    /// 매칭된 hook의 대상 — event key 또는 IPC method.
    pub target: String,
    /// 가공/관찰 대상 payload (envelope.payload, IPC params, IPC result 중 하나).
    pub payload: Value,
    pub host: HostHandle,
}

impl std::fmt::Debug for ExtensionHookCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionHookCtx")
            .field("kind", &self.kind)
            .field("phase", &self.phase)
            .field("mode", &self.mode)
            .field("target", &self.target)
            .field("payload", &self.payload)
            .field("host", &"HostHandle { .. }")
            .finish()
    }
}

/// extension hook의 응답. host에 `ExtensionHookResult`로 직렬화되어 전달된다.
///
/// 헬퍼 생성자(`Self::pass`, `Self::block`, `Self::transformed`, `Self::observed`)를
/// 사용하면 mode와 의미가 맞지 않는 잘못된 조합을 피하기 쉽다.
#[derive(Debug, Clone, Default)]
pub struct ExtensionHookOutcome {
    pub modified_payload: Option<Value>,
    pub pass: Option<bool>,
}

impl ExtensionHookOutcome {
    /// observe / filter pass / transform no-op 모두에 사용할 수 있는 "그대로 통과" 결과.
    pub fn pass() -> Self {
        Self::default()
    }

    /// filter mode에서 흐름을 차단.
    pub fn block() -> Self {
        Self {
            pass: Some(false),
            modified_payload: None,
        }
    }

    /// transform mode에서 payload를 새 값으로 교체.
    pub fn transformed(payload: Value) -> Self {
        Self {
            pass: None,
            modified_payload: Some(payload),
        }
    }

    pub(crate) fn into_proto(self) -> tasty_plugin_protocol::ExtensionHookResult {
        tasty_plugin_protocol::ExtensionHookResult {
            modified_payload: self.modified_payload,
            pass: self.pass,
        }
    }
}

/// 매니페스트 `[[contributes.ipc_namespace]]`로 점유한 prefix의 메서드가
/// IPC 라우터로부터 forward됐을 때 plugin에 전달되는 컨텍스트.
#[derive(Clone)]
pub struct IpcMethodCtx {
    /// 호스트가 받은 원본 메서드 이름 (예: "codex.spawn"). plugin은 이걸로
    /// 내부 dispatch한다.
    pub method: String,
    pub params: Value,
    /// caller가 plugin이면 그 plugin id, CLI/사용자면 `None`.
    pub caller_plugin_id: Option<String>,
    /// 호스트 IPC 메서드를 동기로 호출할 수 있는 핸들. `clone()`해 자기 스레드로
    /// 옮길 수도 있다.
    pub host: HostHandle,
}

impl std::fmt::Debug for IpcMethodCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcMethodCtx")
            .field("method", &self.method)
            .field("params", &self.params)
            .field("caller_plugin_id", &self.caller_plugin_id)
            .field("host", &"HostHandle { .. }")
            .finish()
    }
}

/// `handle_ipc_method`에서 반환할 에러. JSON-RPC 에러 코드와 메시지를 담아
/// 호스트가 원 caller에게 그대로 전달한다.
#[derive(Debug, Clone)]
pub struct IpcMethodError {
    pub message: String,
    /// JSON-RPC 에러 코드. 기본 -32000 (server error).
    pub code: i32,
}

impl IpcMethodError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: -32000,
        }
    }

    pub fn with_code(message: impl Into<String>, code: i32) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }

    pub fn not_found(method: &str) -> Self {
        Self {
            message: format!("method '{method}' not found"),
            code: -32601,
        }
    }

    pub fn not_implemented() -> Self {
        Self {
            message: "method not implemented".into(),
            code: -32601,
        }
    }

    pub fn invalid_params(msg: &str) -> Self {
        Self {
            message: format!("invalid params: {msg}"),
            code: -32602,
        }
    }
}

/// Plugin 핸들러 안에서 SDK 호출이 실패하면 `?` 한 번으로 IPC 응답까지
/// 흘려보낼 수 있게 자동 변환을 제공한다.
///
/// **호스트가 코드를 준 실패는 그 코드를 그대로 낸다.** 그러지 않으면 호스트가
/// "인자를 고쳐라"(`-32602`)로 거절한 것이 plugin 을 거치는 순간 "서버 사정"
/// (`-32000`)이 되어, 호출자가 재시도 정책을 반대로 고른다 — 같은 잘못에 답이
/// 둘인 형태다(ADR-0154 · ADR-0163 · ADR-0167 과 같은 축).
///
/// 코드가 없는 실패(SDK 자체의 연결·인코딩 오류 등)는 종전대로 server error(-32000).
impl From<crate::error::PluginError> for IpcMethodError {
    fn from(err: crate::error::PluginError) -> Self {
        let message = err.to_string();
        match err {
            crate::error::PluginError::HostCall {
                code: Some(code), ..
            } => IpcMethodError::with_code(message, code),
            _ => IpcMethodError::new(message),
        }
    }
}

/// Plugin 생명주기 진입점. 모든 메서드는 동기적으로 호출되고, plugin 로직은
/// 내부에서 자유롭게 thread/async runtime을 사용할 수 있다.
pub trait Plugin: Send + 'static {
    /// 매니페스트 id와 일치해야 함. SDK가 hello 송신 시 사용.
    fn id(&self) -> &str;

    /// hello에 포함할 plugin 자체 버전.
    fn version(&self) -> &str {
        "0.0.0"
    }

    /// `surface.create`에 응답. display_name/snapshot 반환.
    fn create_surface(&mut self, ctx: SurfaceCreateCtx) -> SurfaceResult;

    /// `surface.restore`에 응답. 영속화된 데이터로부터 surface 복원.
    fn restore_surface(&mut self, _ctx: SurfaceRestoreCtx) -> SurfaceResult {
        SurfaceResult::default()
    }

    /// `surface.snapshot` — 영속화할 데이터 반환. 기본 구현은 null.
    fn snapshot_surface(&mut self, _ctx: SurfaceSnapshotCtx) -> Value {
        Value::Null
    }

    /// `surface.destroy` — 호스트가 surface를 닫을 때 호출. 자원 해제용.
    fn destroy_surface(&mut self, _surface_id: u32) {}

    /// `surface.set_context` — egui-mesh surface 의 렌더 컨텍스트(크기/ppp/raw input)가
    /// 도착함. plugin 은 자기 프로세스에서 egui 를 구동·tessellate 한 뒤
    /// [`PluginEvent::PaintFrame`](tasty_plugin_protocol::PluginEvent::PaintFrame) 로
    /// mesh 를 비동기 회신한다([`EguiMeshSurface`](crate::EguiMeshSurface) 헬퍼가 은닉).
    /// fire-and-forget — 이 콜백은 반환값이 없다. 기본 구현은 no-op.
    fn paint_surface(&mut self, _ctx: SurfaceSetContextCtx) {}

    /// `command.invoke` — 매니페스트로 등록한 command가 사용자 단축키 매칭으로
    /// 호출됨. tree가 None이면 호스트는 이전 tree 유지. 기본 구현은 no-op.
    fn handle_command(&mut self, _ctx: CommandInvokeCtx) -> SurfaceResult {
        SurfaceResult::default()
    }

    /// `ipc.invoke` — 매니페스트 `[[contributes.ipc_namespace]]`로 점유한 prefix에
    /// 해당하는 IPC 메서드 호출이 호스트로부터 forward됨. plugin은 method 이름으로
    /// 자체 dispatch하고 JSON 결과 또는 [`IpcMethodError`]를 반환한다.
    /// 기본 구현은 `not_implemented` 에러.
    fn handle_ipc_method(&mut self, _ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        Err(IpcMethodError::not_implemented())
    }

    /// `event.dispatch` — [`BusHandle::subscribe`]로 등록한 패턴이 매칭되는
    /// 이벤트가 fan-out돼 도착했을 때 호출된다. fire-and-forget이라 반환값은 없다.
    /// 기본 구현은 no-op.
    fn on_event(&mut self, _ctx: EventDispatchCtx) {}

    /// `webview.navigation_attempt` — 소유한 webview surface 가 navigation 을 시도함
    /// (`webview.set_url` 의 반대 방향). fire-and-forget이라 반환값은 없다. 기본 구현은 no-op.
    fn on_webview_navigation_attempt(&mut self, _ctx: WebviewNavigationAttemptCtx) {}

    /// `popup.open` — 매니페스트 `[[contributes.popup]]`로 contribute한 popup의
    /// 새 인스턴스가 열림. 콘텐츠는 egui-mesh 채널(`paint_popup`)로 그린다.
    /// 기본 구현은 빈 결과.
    fn open_popup(&mut self, _ctx: PopupOpenCtx) -> PopupOpenResult {
        PopupOpenResult::default()
    }

    /// `popup.set_context` — egui-mesh popup 인스턴스의 렌더 컨텍스트(크기/ppp/raw input)가
    /// 도착함. plugin 은 자기 프로세스에서 egui 를 구동·tessellate 한 뒤
    /// [`PluginEvent::PopupPaintFrame`](tasty_plugin_protocol::PluginEvent::PopupPaintFrame)
    /// 로 mesh 를 비동기 회신한다([`EguiMeshPopup`](crate::EguiMeshPopup) 헬퍼가 은닉).
    /// fire-and-forget — 이 콜백은 반환값이 없다. 기본 구현은 no-op.
    fn paint_popup(&mut self, _ctx: PopupSetContextCtx) {}

    /// `popup.closed` — popup 인스턴스가 닫혔음을 통보. fire-and-forget.
    /// plugin은 인스턴스별 자체 상태를 정리한다. 기본 구현은 no-op.
    fn on_popup_closed(&mut self, _ctx: PopupClosedCtx) {}

    /// `banner.open` — 매니페스트 `[[contributes.banner]]`로 contribute한 banner의 새
    /// 인스턴스가 열림(A3). banner 는 egui-mesh 채널(`paint_banner`)로만 그리므로 초기
    /// tree 가 없다. plugin 은 인스턴스별 렌더 상태를 초기화한다. 기본 구현은 no-op.
    fn open_banner(&mut self, _ctx: BannerOpenCtx) {}

    /// `banner.set_context` — egui-mesh banner 인스턴스의 렌더 컨텍스트(크기/ppp/raw
    /// input/theme)가 도착함. plugin 은 자기 프로세스에서 egui 를 구동·tessellate 한 뒤
    /// [`PluginEvent::BannerPaintFrame`](tasty_plugin_protocol::PluginEvent::BannerPaintFrame)
    /// 로 mesh 를 비동기 회신한다([`EguiMeshBanner`](crate::EguiMeshBanner) 헬퍼가 은닉).
    /// fire-and-forget — 이 콜백은 반환값이 없다. 기본 구현은 no-op.
    fn paint_banner(&mut self, _ctx: BannerSetContextCtx) {}

    /// `banner.closed` — banner 인스턴스가 닫혔음을 통보(TTL/close X/plugin 요청/shutdown).
    /// fire-and-forget. plugin 은 인스턴스별 자체 상태를 정리한다. 기본 구현은 no-op.
    fn on_banner_closed(&mut self, _ctx: BannerClosedCtx) {}

    /// `extension.invoke_hook` — 이 plugin이 다른 plugin(target)의 IPC 또는 이벤트
    /// 흐름을 가로채는 extension일 때, host가 매니페스트 `[[extends.*]]` 항목에 매칭되는
    /// 시점에 호출한다.
    ///
    /// 반환값은 [`ExtensionHookOutcome`]. mode에 맞는 헬퍼를 사용하면 안전:
    /// - `transform` → `ExtensionHookOutcome::transformed(new_payload)` 또는 `pass()`
    /// - `filter` → `block()` 또는 `pass()`
    /// - `observe` → `pass()` (반환값은 호스트가 무시)
    ///
    /// 기본 구현은 `pass()` — extension이 아니거나 hook을 처리하지 않는 plugin은
    /// 안전하게 통과시킨다.
    fn handle_extension_hook(&mut self, _ctx: ExtensionHookCtx) -> ExtensionHookOutcome {
        ExtensionHookOutcome::pass()
    }

    /// Plugin 부트스트랩이 끝나고 worker가 첫 dispatch에 들어가기 직전 1회 호출.
    /// plugin이 자체 background thread를 spawn해 polling 등 능동 작업을 시작할
    /// 때 사용한다. 전달된 [`HostHandle`]은 `Clone`이므로 spawn한 thread로 옮길
    /// 수 있다. 기본 구현은 no-op.
    ///
    /// `bus`는 plugin이 Event Bus에 publish/subscribe할 때 사용. 매니페스트의
    /// `event_subscribe`/`event_publish` 패턴이 비어 있으면 호스트가 등록을 거부하므로
    /// 핸들은 받아도 의미 없는 호출만 가능하다.
    fn on_start(&mut self, _host: HostHandle, _bus: BusHandle) {}

    /// 호스트가 이 plugin 에게 보내려던 요청 `dropped` 건을 **버렸다**.
    ///
    /// 호스트 → plugin 큐는 유한하고 포화 시 대기가 아니라 거절이다(그 방향에서
    /// 기다리면 호스트 프레임이 선다). 그래서 밀리는 plugin 은 자기에게 오던 요청이
    /// 소리 없이 사라지는 것을 원리적으로 알 수 없었다. 이 콜백이 그것을 알린다 —
    /// 호출은 **드롭 뒤 처음 도착한 요청을 처리하기 직전** 한 번이고, 그때까지 쌓인
    /// 수를 합쳐서 넘긴다.
    ///
    /// 무엇이 버려졌는지는 알 수 없다(호스트도 안 들고 있다). 그러므로 재요청이 아니라
    /// **자기 작업량을 줄이는 것**이 처방이다 — polling 간격을 늘리거나 렌더 빈도를
    /// 낮추거나, 사용자에게 알린다. 누적값은 [`HostHandle::dropped_by_host`] 로 아무
    /// 스레드에서나 읽을 수 있다. 기본 구현은 no-op — SDK 가 이미 warn 로그를 남긴다.
    fn on_host_dropped_requests(&mut self, _dropped: u64) {}
}

/// `event.dispatch` 콜백 컨텍스트.
#[derive(Debug, Clone)]
pub struct EventDispatchCtx {
    /// plugin이 [`BusHandle::subscribe`] 시 받은 sub_id. 한 plugin이 여러 구독을
    /// 유지할 때 어떤 구독이 fire 됐는지 식별.
    pub sub_id: u64,
    pub envelope: tasty_plugin_protocol::EventEnvelope,
}

use crate::bus::BusHandle;

#[cfg(test)]
mod ipc_method_error_tests {
    use super::IpcMethodError;
    use crate::error::PluginError;

    /// 호스트가 준 코드가 외부 응답까지 온다.
    ///
    /// 이 변환이 코드를 버리던 동안, 호스트가 `-32602`("인자를 고쳐라")로 거절한 것이
    /// plugin 을 거치면 `-32000`("서버 사정")이 됐다. 두 코드는 호출자의 **재시도 정책**을
    /// 반대로 가른다 — 앞엣것은 인자를 고쳐 다시 걸고, 뒤엣것은 포기한다.
    #[test]
    fn a_host_supplied_code_survives_the_plugin_boundary() {
        let err = PluginError::HostCall {
            method: "call#7".into(),
            message: "no live surface 999 (named by 'terminal.parent')".into(),
            code: Some(-32602),
        };
        let out: IpcMethodError = err.into();
        assert_eq!(out.code, -32602, "호스트 코드가 -32000 으로 뭉개졌다");
        assert!(
            out.message
                .contains("no live surface 999 (named by 'terminal.parent')"),
            "문구가 바뀌었다: {}",
            out.message
        );
    }

    /// 코드가 없는 호스트 실패는 종전대로 server error 다.
    #[test]
    fn a_host_failure_without_a_code_is_still_a_server_error() {
        let err = PluginError::HostCall {
            method: "call#7".into(),
            message: "denied".into(),
            code: None,
        };
        let out: IpcMethodError = err.into();
        assert_eq!(out.code, -32000);
    }

    /// SDK 자신의 실패(연결·인코딩 등)도 종전대로 server error 다 — 호스트가 답한 것이
    /// 아니므로 실을 코드가 없다.
    #[test]
    fn an_sdk_internal_failure_is_a_server_error() {
        let out: IpcMethodError = PluginError::HostClosed.into();
        assert_eq!(out.code, -32000);
        assert_eq!(out.message, "host closed connection");
    }
}
