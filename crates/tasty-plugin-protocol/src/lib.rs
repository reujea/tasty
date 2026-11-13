#![forbid(unsafe_code)]

//! 호스트와 외부 플러그인이 공유하는 JSON 메시지 타입.
//! 기본 빌드에는 egui와 wgpu가 필요하지 않다. egui-mesh 기능을 켜면
//! mesh_wire 코덱과 egui 의존성을 함께 사용한다.

pub mod bed_focus;
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
    AuthAck, AuthAckEnvelope, AuthMessage, BannerBodyCutWire, BannerCloseReason,
    BannerClosedParams, BannerOpenParams, BannerOpenResult, BannerSetContextParams,
    CommandInvokeParams, EventDispatchParams, ExtensionHookInvokeParams, ExtensionHookKind,
    ExtensionHookMode, ExtensionHookPhase, ExtensionHookResult, HandleChannelMessage,
    ImeCursorWire, ImeWire, IpcCallResult, ModifiersWire, PixelRect, PluginEvent, PluginRequest,
    PluginResponse, PointerButtonWire, PopupCloseReason, PopupClosedParams, PopupOpenParams,
    PopupOpenResult, PopupSetContextParams, RawInputEventWire, RawInputWire, RectWire,
    SharedBufferCreateParams, SharedBufferCreateResult, SharedBufferDirtyParams, SharedBufferId,
    SurfaceResult, SurfaceSetContextParams, ThemeWire, WebviewNavigationAttemptParams,
};
pub use protocol::{
    METHOD_BANNER_CLOSED, METHOD_BANNER_OPEN, METHOD_BANNER_SET_CONTEXT, METHOD_COMMAND_INVOKE,
    METHOD_EVENT_DISPATCH, METHOD_EXTENSION_INVOKE_HOOK, METHOD_HOST_HELLO,
    METHOD_HOST_SHARED_BUFFER_CREATE, METHOD_HOST_SHARED_BUFFER_DIRTY, METHOD_IPC_RESULT,
    METHOD_PING, METHOD_POPUP_CLOSED, METHOD_POPUP_OPEN, METHOD_POPUP_SET_CONTEXT, METHOD_SHUTDOWN,
    METHOD_SURFACE_CREATE, METHOD_SURFACE_DESTROY, METHOD_SURFACE_RESTORE,
    METHOD_SURFACE_SET_CONTEXT, METHOD_SURFACE_SNAPSHOT, METHOD_WEBVIEW_NAVIGATION_ATTEMPT,
};
