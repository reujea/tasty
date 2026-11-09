//! SDK for writing external Tasty plugins.
//!
//! 작성자는 [`Plugin`] trait를 구현하고 [`run`]을 호출하면 된다. SDK가
//! 호스트와의 핸드셰이크/메시지 루프/JSON 직렬화를 처리한다.

// 이유: 테스트의 let _ = 사용은 제품 코드의 오류 무시 목록에서 제외한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

/// 빌드타임 베이크된 벡터 아이콘을 egui painter 로 그리는 helper. `egui-mesh` feature 를
/// 켰을 때만(= egui 링크 시) 컴파일된다. image / markdown plugin 이 공유한다.
#[cfg(feature = "egui-mesh")]
pub mod baked_icon;
pub mod bus;
pub mod connection;
pub mod deferred;
/// egui-mesh 기능을 켰을 때 제공하는 렌더링 도우미.
#[cfg(feature = "egui-mesh")]
pub mod egui_surface;
pub mod env;
pub mod error;
pub mod file_watch;
pub mod handle_channel;
pub mod host;
pub mod i18n;
pub mod plugin;
pub mod runtime;
pub mod shared_buffer;

pub use bus::BusHandle;
pub use deferred::DeferredReply;
#[cfg(feature = "egui-mesh")]
pub use egui_surface::{EguiMeshBanner, EguiMeshPopup, EguiMeshSurface};
pub use env::PluginEnv;
pub use error::{PluginError, Result};
#[allow(deprecated)]
pub use host::HostCallError;
pub use host::HostHandle;
pub use i18n::Translator;
pub use plugin::{
    BannerClosedCtx, BannerOpenCtx, BannerSetContextCtx, CommandInvokeCtx, EventDispatchCtx,
    ExtensionHookCtx, ExtensionHookOutcome, IpcMethodCtx, IpcMethodError, Plugin, PopupClosedCtx,
    PopupOpenCtx, PopupOpenResult, PopupSetContextCtx, SurfaceCreateCtx, SurfaceRestoreCtx,
    SurfaceResult, SurfaceSetContextCtx, SurfaceSnapshotCtx, WebviewNavigationAttemptCtx,
};
pub use runtime::run;
pub use shared_buffer::SharedBuffer;
pub use tasty_plugin_protocol::{
    EventEnvelope, EventMeta, EventOrigin, EventScope, ExtensionHookKind, ExtensionHookMode,
    ExtensionHookPhase, LifecycleReason, PixelRect, PluginEvent, SharedBufferId,
};
