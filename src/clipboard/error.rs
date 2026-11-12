//! Clipboard access failures are distinct from an offer without the requested content.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardError {
    /// Wayland 창이라 X11 클립보드에 닿지 못했다. 터미널 텍스트 클립보드는 창에 Wayland selection
    /// 소유자가 없을 때(컴포지터에 data device가 없을 때)만 이 값을 만난다. 소유자가 있으면 arboard를
    /// 거치지 않는다. primary selection과 Explorer 파일 클립보드는 소유자와 관계없이 이 값이다.
    /// `x_failed`는 `DISPLAY`의 X 서버에 연결하지 못해 Wayland로 내려간 경우(원인 1b)다.
    #[cfg(feature = "gui")]
    UnsupportedNativeWayland {
        reason: String,
        x_failed: bool,
    },
    /// 창에 Wayland selection 소유자가 있어 일반 복사는 외부 앱에 닿는데 primary(vi `"*`) 쓰기만
    /// X11에 닿지 못했다. 소유자가 없으면(데스크톱이 클립보드를 주지 않음) `UnsupportedNativeWayland`다.
    #[cfg(feature = "gui")]
    PrimaryUnsupportedNativeWayland {
        reason: String,
        x_failed: bool,
    },
    BackendUnavailable(String),
    TransferFailed(String),
}

impl ClipboardError {
    #[cfg(feature = "gui")]
    pub(crate) fn for_display(self, display: DisplayBackend) -> Self {
        match self {
            Self::BackendUnavailable(reason) => match display {
                DisplayBackend::Wayland { x_failed } => {
                    Self::UnsupportedNativeWayland { reason, x_failed }
                }
                DisplayBackend::Unknown | DisplayBackend::Other => Self::BackendUnavailable(reason),
            },
            error => error,
        }
    }

    /// primary 쓰기 실패를 분류한다. 창에 Wayland selection 소유자가 있으면(`has_selection`) 네이티브
    /// Wayland 실패는 primary만의 실패다. 소유자가 없으면 모든 복사가 막힌 경우라 그대로 둔다.
    #[cfg(feature = "gui")]
    pub(crate) fn for_primary(self, has_selection: bool) -> Self {
        match self {
            Self::UnsupportedNativeWayland { reason, x_failed } if has_selection => {
                Self::PrimaryUnsupportedNativeWayland { reason, x_failed }
            }
            error => error,
        }
    }

    #[cfg(feature = "gui")]
    pub(crate) fn from_arboard(error: arboard::Error) -> Self {
        match error {
            arboard::Error::ClipboardNotSupported => Self::BackendUnavailable(error.to_string()),
            _ => Self::TransferFailed(error.to_string()),
        }
    }

    #[cfg(feature = "gui")]
    pub(crate) fn feedback(&self) -> (&'static str, crate::adapters::ui::ToastKind) {
        use crate::adapters::ui::ToastKind;
        match self {
            // 원인과 해결책을 담은 안내다. 사용자가 Tasty 밖에서 고칠 일이라 warning이다.
            Self::UnsupportedNativeWayland {
                x_failed: false, ..
            } => ("toast.clipboard_wayland_unsupported", ToastKind::Warning),
            Self::UnsupportedNativeWayland { x_failed: true, .. } => {
                ("toast.clipboard_wayland_x_failed", ToastKind::Warning)
            }
            Self::PrimaryUnsupportedNativeWayland {
                x_failed: false, ..
            } => (
                "toast.clipboard_primary_wayland_unsupported",
                ToastKind::Warning,
            ),
            Self::PrimaryUnsupportedNativeWayland { x_failed: true, .. } => (
                "toast.clipboard_primary_wayland_x_failed",
                ToastKind::Warning,
            ),
            Self::BackendUnavailable(_) => ("toast.clipboard_unavailable", ToastKind::Info),
            Self::TransferFailed(_) => ("toast.clipboard_failed", ToastKind::Error),
        }
    }
}

/// Explorer 파일 클립보드의 어느 동작이 시스템 클립보드에 닿지 못했는지.
#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileClipboardOp {
    Copy,
    Paste,
}

#[cfg(feature = "gui")]
impl ClipboardError {
    /// Explorer가 시스템 클립보드 대신 앱 안 목록을 쓸 때의 토스트. 네이티브 Wayland가 원인이면 원인과
    /// 해결책을 담은 warning이고, 그 밖의 실패는 원인을 가리지 않는 info다.
    pub(crate) fn files_feedback(
        &self,
        op: FileClipboardOp,
    ) -> (&'static str, crate::adapters::ui::ToastKind) {
        use crate::adapters::ui::ToastKind;
        match (self, op) {
            (
                Self::UnsupportedNativeWayland {
                    x_failed: false, ..
                },
                FileClipboardOp::Copy,
            ) => ("toast.clipboard_files_wayland_copy", ToastKind::Warning),
            (
                Self::UnsupportedNativeWayland {
                    x_failed: false, ..
                },
                FileClipboardOp::Paste,
            ) => ("toast.clipboard_files_wayland_paste", ToastKind::Warning),
            (Self::UnsupportedNativeWayland { x_failed: true, .. }, FileClipboardOp::Copy) => (
                "toast.clipboard_files_wayland_x_failed_copy",
                ToastKind::Warning,
            ),
            (Self::UnsupportedNativeWayland { x_failed: true, .. }, FileClipboardOp::Paste) => (
                "toast.clipboard_files_wayland_x_failed_paste",
                ToastKind::Warning,
            ),
            (_, FileClipboardOp::Copy) => ("toast.clipboard_files_internal_copy", ToastKind::Info),
            (_, FileClipboardOp::Paste) => {
                ("toast.clipboard_files_internal_paste", ToastKind::Info)
            }
        }
    }
}

/// The GUI supplies the actual display handle; service-only callers leave it unknown.
#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DisplayBackend {
    #[default]
    Unknown,
    /// `x_failed`: `DISPLAY`의 X 서버 연결 실패로 Wayland로 내려간 창이다.
    Wayland {
        x_failed: bool,
    },
    Other,
}

#[cfg(feature = "gui")]
impl DisplayBackend {
    pub(crate) fn of(window: &winit::window::Window) -> Self {
        use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        match window.display_handle().map(|h| h.as_raw()) {
            Ok(RawDisplayHandle::Wayland(_)) => Self::Wayland {
                x_failed: crate::boot::display_backend::started_after_x_failure(),
            },
            Ok(_) => Self::Other,
            Err(_) => Self::Unknown,
        }
    }

    /// The backend the process started on, for clipboard calls with no window at hand (OSC 52,
    /// IPC, capture publishing). A native Wayland start keeps its cause like a window does; an
    /// X11 start, no display, or no recorded start stays `Unknown` as before.
    pub(crate) fn at_start() -> Self {
        Self::from_start(crate::boot::display_backend::start().map(|start| start.backend))
    }

    fn from_start(backend: Option<crate::boot::display_backend::StartBackend>) -> Self {
        use crate::boot::display_backend::StartBackend;
        match backend {
            Some(StartBackend::Wayland) => Self::Wayland { x_failed: false },
            Some(StartBackend::WaylandFallback) => Self::Wayland { x_failed: true },
            Some(StartBackend::X11 | StartBackend::Unset) | None => Self::Unknown,
        }
    }
}

impl From<String> for ClipboardError {
    fn from(reason: String) -> Self {
        Self::TransferFailed(reason)
    }
}
impl From<&str> for ClipboardError {
    fn from(reason: &str) -> Self {
        Self::TransferFailed(reason.into())
    }
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (label, reason) = match self {
            #[cfg(feature = "gui")]
            Self::UnsupportedNativeWayland { reason, .. } => (
                "no Wayland selection for this clipboard; X11 clipboard unavailable",
                reason,
            ),
            #[cfg(feature = "gui")]
            Self::PrimaryUnsupportedNativeWayland { reason, .. } => (
                "primary selection needs the X11 clipboard, which is unavailable",
                reason,
            ),
            Self::BackendUnavailable(reason) => ("clipboard backend unavailable", reason),
            Self::TransferFailed(reason) => ("clipboard transfer failed", reason),
        };
        write!(f, "{label}: {reason}")
    }
}
impl std::error::Error for ClipboardError {}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;
    use crate::adapters::ui::ToastKind;

    fn wayland(x_failed: bool) -> ClipboardError {
        ClipboardError::BackendUnavailable("fixture".into())
            .for_display(DisplayBackend::Wayland { x_failed })
    }

    /// Given Wayland 창의 백엔드 실패, Then 원인 1과 1b가 갈린다. 다른 창은 그대로다.
    #[test]
    fn a_wayland_window_keeps_which_cause_made_it() {
        assert_eq!(
            wayland(false),
            ClipboardError::UnsupportedNativeWayland {
                reason: "fixture".into(),
                x_failed: false
            }
        );
        assert_eq!(
            wayland(true),
            ClipboardError::UnsupportedNativeWayland {
                reason: "fixture".into(),
                x_failed: true
            }
        );
        for display in [DisplayBackend::Other, DisplayBackend::Unknown] {
            assert!(matches!(
                ClipboardError::BackendUnavailable("fixture".into()).for_display(display),
                ClipboardError::BackendUnavailable(_)
            ));
        }
    }

    /// Given 네이티브 Wayland에서 selection 소유자가 없는 창, When 터미널 복사·붙여넣기가 실패한다,
    /// Then 원인별 키의 warning이다.
    #[test]
    fn terminal_clipboard_on_native_wayland_warns_with_its_cause() {
        assert_eq!(
            wayland(false).feedback(),
            ("toast.clipboard_wayland_unsupported", ToastKind::Warning)
        );
        assert_eq!(
            wayland(true).feedback(),
            ("toast.clipboard_wayland_x_failed", ToastKind::Warning)
        );
        assert_eq!(
            ClipboardError::BackendUnavailable("x".into()).feedback(),
            ("toast.clipboard_unavailable", ToastKind::Info)
        );
    }

    /// Given 네이티브 Wayland로 시작했고 selection 소유자가 없다, When 창 없는 Core 쓰기(OSC 52·IPC·
    /// 캡처 게시)가 실패한다, Then 터미널 복사와 같은 원인별 warning이다. X11 시작은 그대로다.
    #[test]
    fn a_core_write_on_a_native_wayland_start_warns_with_its_cause() {
        use crate::boot::display_backend::StartBackend;
        let core = |backend| {
            ClipboardError::BackendUnavailable("fixture".into())
                .for_display(DisplayBackend::from_start(backend))
                .feedback()
        };
        assert_eq!(
            core(Some(StartBackend::Wayland)),
            ("toast.clipboard_wayland_unsupported", ToastKind::Warning)
        );
        assert_eq!(
            core(Some(StartBackend::WaylandFallback)),
            ("toast.clipboard_wayland_x_failed", ToastKind::Warning)
        );
        for backend in [Some(StartBackend::X11), Some(StartBackend::Unset), None] {
            assert_eq!(
                core(backend),
                ("toast.clipboard_unavailable", ToastKind::Info)
            );
        }
    }

    /// Given 네이티브 Wayland, When vi `"*` 쓰기가 X11에 닿지 못했다, Then selection 소유자가 있으면
    /// 원인별 primary 키이고 없으면(데스크톱이 클립보드를 주지 않음) 터미널 키가 우선한다.
    /// X11·XWayland의 실패는 primary 키로 가지 않는다.
    #[test]
    fn a_primary_failure_names_only_the_star_register_unless_nothing_reaches_other_apps() {
        for (x_failed, primary, terminal) in [
            (
                false,
                "toast.clipboard_primary_wayland_unsupported",
                "toast.clipboard_wayland_unsupported",
            ),
            (
                true,
                "toast.clipboard_primary_wayland_x_failed",
                "toast.clipboard_wayland_x_failed",
            ),
        ] {
            assert_eq!(
                wayland(x_failed).for_primary(true).feedback(),
                (primary, ToastKind::Warning)
            );
            assert_eq!(
                wayland(x_failed).for_primary(false).feedback(),
                (terminal, ToastKind::Warning)
            );
        }
        for display in [DisplayBackend::Other, DisplayBackend::Unknown] {
            let error = ClipboardError::BackendUnavailable("fixture".into()).for_display(display);
            for has_selection in [false, true] {
                assert_eq!(
                    error.clone().for_primary(has_selection).feedback(),
                    ("toast.clipboard_unavailable", ToastKind::Info)
                );
            }
        }
        assert_eq!(
            ClipboardError::TransferFailed("x".into())
                .for_primary(true)
                .feedback(),
            ("toast.clipboard_failed", ToastKind::Error)
        );
    }

    /// Given Explorer 파일 클립보드 실패, Then 네이티브 Wayland만 원인별 warning이고 그 밖은 기존 info다.
    #[test]
    fn explorer_files_warn_only_for_native_wayland() {
        use FileClipboardOp::{Copy, Paste};
        let cases = [
            (wayland(false), Copy, "toast.clipboard_files_wayland_copy"),
            (wayland(false), Paste, "toast.clipboard_files_wayland_paste"),
            (
                wayland(true),
                Copy,
                "toast.clipboard_files_wayland_x_failed_copy",
            ),
            (
                wayland(true),
                Paste,
                "toast.clipboard_files_wayland_x_failed_paste",
            ),
        ];
        for (error, op, key) in cases {
            assert_eq!(error.files_feedback(op), (key, ToastKind::Warning));
        }
        for error in [
            ClipboardError::BackendUnavailable("x".into()),
            ClipboardError::TransferFailed("x".into()),
        ] {
            assert_eq!(
                error.files_feedback(Copy),
                ("toast.clipboard_files_internal_copy", ToastKind::Info)
            );
            assert_eq!(
                error.files_feedback(Paste),
                ("toast.clipboard_files_internal_paste", ToastKind::Info)
            );
        }
    }
}
