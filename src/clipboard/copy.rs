//! The one door every host text copy goes through: the window's [`TerminalClipboard`] picks the
//! backend (its Wayland selection on a native Wayland start, arboard elsewhere) and the window's
//! toast stack reports the result. Terminal selections, vi yanks, link menus, copy shortcuts and
//! egui widget copies (a text field's Ctrl+C or Ctrl+X, a selectable label, `ctx.copy_text`) all
//! end here, so a failed write never shows a success toast.

use super::error::ClipboardError;
use super::terminal::TerminalClipboard;
use crate::adapters::ui::{ToastKind, ToastManager, ToastScope};

/// Writes `text` and shows the result: `key` (with `hint`) on success, the error's own toast on
/// failure. Returns whether the write succeeded.
pub(crate) fn copy_text(
    clipboard: &mut TerminalClipboard,
    toasts: &mut ToastManager,
    text: &str,
    primary: bool,
    key: &str,
    hint: Vec<String>,
    scope: ToastScope,
) -> bool {
    let result = write(clipboard, text, primary);
    copy_feedback(toasts, result, key, hint, scope)
}

/// Writes what an egui widget of the window copied since the last call, if anything, and shows
/// the result in that window. A widget belongs to no surface, so the toast is window-wide; a cut
/// shows the same toast because egui's output does not tell the two apart. Returns whether there
/// was a copy, so the caller redraws to show the toast.
pub(crate) fn copy_egui_text(
    gpu: &mut crate::gpu::GpuState,
    clipboard: &mut TerminalClipboard,
    toasts: &mut ToastManager,
) -> bool {
    let Some(text) = gpu.take_egui_copy() else {
        return false;
    };
    copy_text(
        clipboard,
        toasts,
        &text,
        false,
        "toast.copied",
        Vec::new(),
        ToastScope::Window,
    );
    true
}

/// Writes `text` for a screen that has no toast stack (the boot error and shell setup screens, the quit dialog).
/// A failure is only logged.
pub(crate) fn copy_text_unannounced(clipboard: &mut TerminalClipboard, text: &str) -> bool {
    match write(clipboard, text, false) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!("clipboard copy failed: {error}");
            false
        }
    }
}

fn write(
    clipboard: &mut TerminalClipboard,
    text: &str,
    primary: bool,
) -> Result<(), ClipboardError> {
    if primary {
        clipboard.set_text_primary(text)
    } else {
        clipboard.set_text(text)
    }
}

/// The toast for a finished copy. Also used by writes that cannot take the door itself (OSC 52
/// arrives on the Core side), so their feedback matches.
pub(crate) fn copy_feedback(
    toasts: &mut ToastManager,
    result: Result<(), ClipboardError>,
    key: &str,
    hint: Vec<String>,
    scope: ToastScope,
) -> bool {
    match result {
        Ok(()) => {
            toasts.push_with_hint(crate::i18n::t(key), ToastKind::Info, hint, scope);
            true
        }
        Err(error) => {
            error_toast(toasts, &error, scope);
            false
        }
    }
}

pub(crate) fn error_toast(toasts: &mut ToastManager, error: &ClipboardError, scope: ToastScope) {
    let (key, kind) = error.feedback();
    toasts.push(crate::i18n::t(key), kind, scope);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unavailable() -> TerminalClipboard {
        TerminalClipboard::unavailable(ClipboardError::BackendUnavailable("fixture".into()))
    }

    /// Given a window whose clipboard cannot be written, When text is copied, Then the error's
    /// toast shows instead of the success key and the door reports failure.
    #[test]
    fn a_failed_copy_shows_the_error_toast_not_the_success_toast() {
        let mut toasts = ToastManager::new();
        let copied = copy_text(
            &mut unavailable(),
            &mut toasts,
            "text",
            false,
            "toast.copied",
            Vec::new(),
            ToastScope::Window,
        );
        assert!(!copied);
        assert_eq!(
            toasts.messages(),
            vec![crate::i18n::t("toast.clipboard_unavailable")]
        );
    }

    /// Given a finished write, When its feedback is shown, Then exactly one toast with the given
    /// key appears.
    #[test]
    fn a_successful_copy_shows_one_toast_with_its_key() {
        let mut toasts = ToastManager::new();
        assert!(copy_feedback(
            &mut toasts,
            Ok(()),
            "toast.copied",
            Vec::new(),
            ToastScope::Window,
        ));
        assert_eq!(toasts.messages(), vec![crate::i18n::t("toast.copied")]);
    }

    /// Given a screen without toasts, When its copy fails, Then the door reports failure only.
    #[test]
    fn an_unannounced_copy_reports_its_failure() {
        assert!(!copy_text_unannounced(&mut unavailable(), "text"));
    }
}
