//! 창이 자기 `PopupManager`에 연 파일 피커 정의의 인스턴스와 그 결과 보관.
use super::{FileChooserMode, FileChooserOutcome, LocalFileChooser};
use crate::adapters::ui::popup::file_picker::{FILE_PICKER_POPUP_ID, report_hidden_crumbs_menu};
use crate::adapters::ui::popup::{PopupManager, defs};
use std::path::PathBuf;
use tasty_type_geometry::length::LogicalPx;

/// 본창 파일 피커 정의를 창의 관리자에 등록한다. 콘텐츠는 창이 그린다.
pub(crate) fn register(popups: &mut PopupManager) {
    let def = defs::find(FILE_PICKER_POPUP_ID).expect("file picker PopupDef");
    popups.register_def(def, 1.0);
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
        self.popups.is_open(FILE_PICKER_POPUP_ID)
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
        if let Some(p) = self.popups.get_mut(FILE_PICKER_POPUP_ID) {
            p.title = self.chooser.title().to_owned();
            p.set_needed_size(if self.chooser.is_folder() {
                folder_size()
            } else {
                super::chooser_size()
            });
        }
        // intent-exempt: user Browse in an independent View opens its own registered PopupDef.
        self.popups.open_centered_focused(
            FILE_PICKER_POPUP_ID,
            crate::model::popup_kind::PopupScope::Window,
        );
        true
    }
    pub(crate) fn cancel(&mut self) {
        self.chooser.cancel();
        // intent-exempt: the owning editor invalidated this View-local request.
        self.popups.close(FILE_PICKER_POPUP_ID);
    }
    pub(crate) fn poll(&mut self, owner: &mut crate::app::local_reads::ReadRequests) -> bool {
        self.chooser.poll_local_reads(owner)
    }
    /// `keys` 는 창 `render` 가 이번 프레임에 정한 키 값이다. 다시 그리는 패스에도 같은 값을 넣도록 팝업을 그리기 전에
    /// 패스마다 관리자에 넣는다.
    pub(crate) fn draw(
        &mut self,
        ctx: &egui::Context,
        keys: &crate::adapters::ui::popup::escape_step::WindowPassKeys,
    ) {
        self.popups.put_window_pass_keys(ctx, keys);
        let chooser = &mut self.chooser;
        let closed = self.popups.draw_view_local(ctx, &mut |cx, ui, _| {
            if cx.id != FILE_PICKER_POPUP_ID {
                return false;
            }
            // 이 패스 Esc 는 경로 메뉴가 닫혀 있을 때만 피커 것이다.
            let escape = !crate::adapters::ui::popup::child_popup_open(ui.ctx(), cx.instance)
                && cx
                    .keys
                    .consume(crate::adapters::ui::popup::keys::BasicKey::Escape);
            let drawn = chooser.draw(ui, &crate::theme::theme(), &mut cx.keys, escape);
            report_hidden_crumbs_menu(ui, cx.instance, drawn.child_menu);
            drawn.done
        });
        // 응답을 끝낸 뒤 닫혔으면 결과가 남고, 그 밖의 경로로 닫혔으면 취소다.
        if closed.contains(&FILE_PICKER_POPUP_ID) {
            self.chooser.cancel();
        }
    }
    pub(crate) fn take_outcome(&mut self, consumer: &'static str) -> Option<FileChooserOutcome> {
        self.chooser.take_outcome(consumer)
    }
    /// 창 위젯이 이 패스 키를 받는지(파일 선택 창이 이 패스 키 주인이 아님). 창 위젯을 파일 선택 창보다 먼저 그리는 창이
    /// 그 앞에서 부른다. 같은 `run` 의 그리기(`draw`)는 같은 값을 쓴다.
    pub(crate) fn window_takes_keys(
        &mut self,
        ctx: &egui::Context,
        keys: &crate::adapters::ui::popup::escape_step::WindowPassKeys,
    ) -> bool {
        self.popups.put_window_pass_keys(ctx, keys)
    }
    /// 이 창의 W1 값. `record`는 직전 패스 뒤의 포커스 기록이다.
    #[cfg(test)]
    pub(crate) fn key_owner(
        &self,
        ctx: &egui::Context,
        record: crate::adapters::ui::popup::window_keys::WindowFocusRecord,
    ) -> Option<crate::adapters::ui::popup::PopupInstanceId> {
        self.popups.window_key_owner(ctx, record)
    }
}

impl crate::view::window_input::WindowPopups for WindowFileChooser {
    fn window_popups(&mut self) -> &mut PopupManager {
        &mut self.popups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given 창의 파일 선택, When 열면, Then 본창 파일 피커 정의의 인스턴스이고 그 선언의 모양을 쓴다.
    #[test]
    fn the_view_chooser_is_an_instance_of_the_file_picker_declaration() {
        let mut window = WindowFileChooser::default();
        assert!(window.open("view_chooser_test", FileChooserMode::Open, "~"));
        let shown = window
            .popups
            .instances_of(FILE_PICKER_POPUP_ID)
            .next()
            .expect("an open file picker instance");
        let mut main = PopupManager::new();
        main.register_def(
            defs::find(FILE_PICKER_POPUP_ID).expect("file picker PopupDef"),
            1.0,
        );
        let declared = main
            .get_mut(FILE_PICKER_POPUP_ID)
            .expect("registered file picker")
            .look();
        assert_eq!(shown.look(), declared);
    }
}
