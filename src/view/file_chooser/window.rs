//! A View-owned instance of the host picker PopupDef, with local receipt storage.
use super::{FILE_CHOOSER_POPUP_ID, FileChooserMode, FileChooserOutcome, LocalFileChooser};
use crate::adapters::ui::popup::{PopupDef, PopupManager, defs};
use std::path::PathBuf;
use tasty_type_geometry::length::LogicalPx;

pub(crate) fn register(popups: &mut PopupManager) {
    let base = defs::find(crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID)
        .expect("file picker PopupDef");
    let def = PopupDef {
        id: FILE_CHOOSER_POPUP_ID,
        decl: base.decl,
        scrim_input: base.scrim_input,
        title_key: base.title_key,
        title_fn: base.title_fn,
        sizer: base.sizer,
        drag_region: base.drag_region,
        draw_fn: base.draw_fn,
        fullscreen_stage: base.fullscreen_stage,
        on_close: base.on_close,
    };
    popups.register_def(&def, 1.0);
}

pub(crate) fn folder_size() -> egui::Vec2 {
    // Received Settings folder-mode frame; main/gallery also retain 640×480 and 320-wide specimens.
    egui::vec2(LogicalPx(480.0).value(), LogicalPx(360.0).value())
}

pub(crate) fn start_directory(value: &str) -> PathBuf {
    let home = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));
    let path = if value == "~" {
        home.clone()
    } else if let Some(tail) = value.strip_prefix("~/") {
        home.join(tail)
    } else {
        PathBuf::from(value)
    };
    let directory = if path.is_dir() {
        Some(path)
    } else {
        path.parent().filter(|p| p.is_dir()).map(PathBuf::from)
    };
    directory
        .and_then(|path| {
            if path.is_absolute() {
                Some(path)
            } else {
                std::env::current_dir().ok().map(|cwd| cwd.join(path))
            }
        })
        .unwrap_or(home)
}

pub(crate) struct WindowFileChooser {
    pub(crate) chooser: LocalFileChooser,
    popups: PopupManager,
}
impl Default for WindowFileChooser {
    fn default() -> Self {
        let mut popups = PopupManager::new();
        register(&mut popups);
        Self {
            chooser: LocalFileChooser::default(),
            popups,
        }
    }
}
impl std::fmt::Debug for WindowFileChooser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowFileChooser")
            .field("open", &self.is_open())
            .finish()
    }
}
impl WindowFileChooser {
    pub(crate) fn is_open(&self) -> bool {
        self.popups.is_open(FILE_CHOOSER_POPUP_ID)
    }
    pub(crate) fn open(
        &mut self,
        consumer: &'static str,
        mode: FileChooserMode,
        value: &str,
    ) -> bool {
        if self.chooser.is_pending() {
            return false;
        }
        self.chooser
            .begin_at(consumer, mode, Vec::new(), start_directory(value));
        if let Some(p) = self.popups.get_mut(FILE_CHOOSER_POPUP_ID) {
            p.title = self.chooser.title().to_owned();
            p.size = if self.chooser.is_folder() {
                folder_size()
            } else {
                super::chooser_size()
            };
        }
        // intent-exempt: user Browse in an independent View opens its own registered PopupDef.
        self.popups.open_centered_focused(
            FILE_CHOOSER_POPUP_ID,
            crate::model::popup_kind::PopupScope::Window,
        );
        true
    }
    pub(crate) fn cancel(&mut self) {
        self.chooser.cancel();
        // intent-exempt: the owning editor invalidated this View-local request.
        self.popups.close(FILE_CHOOSER_POPUP_ID);
    }
    pub(crate) fn poll(&mut self, owner: &mut crate::app::local_reads::ReadRequests) -> bool {
        self.chooser.poll_local_reads(owner)
    }
    pub(crate) fn draw(&mut self, ctx: &egui::Context) {
        let owns_escape = self.is_open();
        let mut finished = false;
        let chooser = &mut self.chooser;
        let result = self.popups.draw(
            ctx,
            &mut |cx, ui| {
                if cx.id == FILE_CHOOSER_POPUP_ID {
                    finished |= chooser.draw(ui, &crate::theme::theme(), owns_escape);
                }
            },
            None,
            &[],
        );
        if finished {
            // intent-exempt: local chooser's completed response closes its own View popup.
            self.popups.close(FILE_CHOOSER_POPUP_ID);
        }
        // Raw View-local drawing returns close receipts instead of dispatching MainViewState hooks.
        let queued = self.popups.take_closed_queue();
        if result.closed.contains(&FILE_CHOOSER_POPUP_ID)
            || queued.iter().any(|c| c.id == FILE_CHOOSER_POPUP_ID)
        {
            self.chooser.cancel();
        }
    }
    pub(crate) fn take_outcome(&mut self, consumer: &'static str) -> Option<FileChooserOutcome> {
        self.chooser.take_outcome(consumer)
    }
    pub(crate) fn mark_input_owner(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("view_local_file_chooser_open"),
                self.is_open(),
            )
        });
    }
}
pub(crate) fn owns_input(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("view_local_file_chooser_open")))
        .unwrap_or(false)
}
