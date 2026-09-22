#![forbid(unsafe_code)]

//! Tasty 호스트와 외부 plugin이 공유하는 wire 타입 정의.
//!
//! 호스트 ↔ plugin 양방향 JSON 메시지 envelope ([`protocol`])을 제공한다.
//!
//! 기본 빌드는 `serde`/`serde_json` 외 의존성이 없도록 유지한다 — plugin은
//! 무거운 host 의존(예: egui/wgpu) 없이 컴파일 가능해야 한다.
//!
//! 예외 — `egui-mesh` feature 를 켜면 egui-mesh paint 출력을 POD 바이트로
//! 인코드/디코드하는 [`mesh_wire`] 코덱이 활성화된다(egui 의존을 옵셔널로 끌어옴).
//! egui-mesh surface 를 쓰는 host/plugin 만 이 feature 를 켠다.

pub mod events;
pub mod host_port;
pub mod ipc_method;
pub mod line;
#[cfg(feature = "egui-mesh")]
pub mod mesh_wire;
pub mod protocol;

pub use events::{EventEnvelope, EventMeta, EventOrigin, EventScope, LifecycleReason, MAX_HOP};
pub use ipc_method::{IpcInvokeParams, METHOD_IPC_INVOKE};
pub use line::write_line;
pub use protocol::{
    AuthAck, AuthAckEnvelope, AuthMessage, BannerCloseReason, BannerClosedParams, BannerOpenParams,
    BannerOpenResult, BannerSetContextParams, CommandInvokeParams, EventDispatchParams,
    ExtensionHookInvokeParams, ExtensionHookKind, ExtensionHookMode, ExtensionHookPhase,
    ExtensionHookResult, HandleChannelMessage, ImeCursorWire, ImeWire, IpcCallResult,
    ModifiersWire, PixelRect, PluginEvent, PluginRequest, PluginResponse, PointerButtonWire,
    PopupCloseReason, PopupClosedParams, PopupOpenParams, PopupOpenResult, PopupSetContextParams,
    RawInputEventWire, RawInputWire, RectWire, SharedBufferCreateParams, SharedBufferCreateResult,
    SharedBufferDirtyParams, SharedBufferId, SurfaceResult, SurfaceSetContextParams, ThemeWire,
    WebviewNavigationAttemptParams,
};
pub use protocol::{
    METHOD_BANNER_CLOSED, METHOD_BANNER_OPEN, METHOD_BANNER_SET_CONTEXT, METHOD_COMMAND_INVOKE,
    METHOD_EVENT_DISPATCH, METHOD_EXTENSION_INVOKE_HOOK, METHOD_HOST_HELLO,
    METHOD_HOST_SHARED_BUFFER_CREATE, METHOD_HOST_SHARED_BUFFER_DIRTY, METHOD_IPC_RESULT,
    METHOD_PING, METHOD_POPUP_CLOSED, METHOD_POPUP_OPEN, METHOD_POPUP_SET_CONTEXT, METHOD_SHUTDOWN,
    METHOD_SURFACE_CREATE, METHOD_SURFACE_DESTROY, METHOD_SURFACE_RESTORE,
    METHOD_SURFACE_SET_CONTEXT, METHOD_SURFACE_SNAPSHOT, METHOD_WEBVIEW_NAVIGATION_ATTEMPT,
};
