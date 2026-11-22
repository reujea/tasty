//! Folder-only keyboard actions and the footer shared with gallery specimens.
use super::{FilePickerAction, FilePickerMode, FilePickerProps, FpViewState, join_dir};
use tasty_ui_widgets::FolderPickerFooter;

/// 폴더 모드의 확정 대상. 고른 하위 폴더가 있으면 그 폴더, 없으면 지금 보고 있는 폴더다.
pub(super) fn target(d: &crate::state::FilePickerData) -> String {
    match d.selected.as_slice() {
        [name] if d.entries.iter().any(|e| &e.name == name && e.is_dir) => {
            join_dir(d.mirror_ws_id.is_some(), &d.current_dir, name)
        }
        _ => d.current_dir.clone(),
    }
}

pub(super) fn footer<'a>(p: &'a FilePickerProps<'a>) -> Option<FolderPickerFooter<'a>> {
    let FilePickerMode::Folder { path, hint, .. } = p.mode else {
        return None;
    };
    Some(FolderPickerFooter {
        label: p.name_field_label,
        path,
        hint,
        selected: p.selected.first().map(String::as_str),
        confirm: p.confirm_label,
        cancel: p.cancel_label,
        ready: matches!(p.state, FpViewState::Loaded | FpViewState::Empty),
    })
}

/// 폴더 모드의 키. 키 통로의 노드 키맵 읽기라 egui 매처처럼 패턴에 없는 Shift·Alt 가 붙은 누름도 같은 조작이다.
pub(super) fn keyboard(
    ui: &egui::Ui,
    p: &FilePickerProps<'_>,
    keys: &mut crate::adapters::ui::popup::keys::PopupKeys,
) -> Option<FilePickerAction> {
    if ui.memory(|m| m.any_popup_open())
        || !keys.receives()
        || !matches!(p.mode, FilePickerMode::Folder { .. })
    {
        return None;
    }
    let mut consume = |key, modifiers| keys.consume_keymap_key(modifiers, key);
    if consume(egui::Key::ArrowUp, egui::Modifiers::ALT) {
        return Some(FilePickerAction::NavigateUp);
    }
    if !matches!(p.state, FpViewState::Loaded | FpViewState::Empty) {
        return None;
    }
    if consume(egui::Key::Enter, egui::Modifiers::NONE) {
        return Some(FilePickerAction::Confirm);
    }
    let folders: Vec<_> = p
        .entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| &e.name)
        .collect();
    let current = folders
        .iter()
        .position(|name| p.selected.first() == Some(*name));
    if consume(egui::Key::ArrowRight, egui::Modifiers::NONE) {
        return current.map(|i| FilePickerAction::NavigateInto(folders[i].clone()));
    }
    if folders.is_empty() {
        return None;
    }
    let next = if consume(egui::Key::ArrowDown, egui::Modifiers::NONE) {
        Some(current.map_or(0, |i| (i + 1).min(folders.len() - 1)))
    } else if consume(egui::Key::ArrowUp, egui::Modifiers::NONE) {
        Some(current.map_or(folders.len() - 1, |i| i.saturating_sub(1)))
    } else {
        None
    };
    next.filter(|i| Some(*i) != current)
        .map(|i| FilePickerAction::Select(folders[i].clone()))
}
