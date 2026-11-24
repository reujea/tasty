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

/// 폴더 모드 노드 키맵(상위 폴더, 확정, 들어가기, 다음·앞 선택). 본문과 키 경로의 수신 판정이 같은 표를 쓴다.
const FOLDER_UP: (egui::Modifiers, egui::Key) = (egui::Modifiers::ALT, egui::Key::ArrowUp);
const FOLDER_CONFIRM: (egui::Modifiers, egui::Key) = (egui::Modifiers::NONE, egui::Key::Enter);
const FOLDER_INTO: (egui::Modifiers, egui::Key) = (egui::Modifiers::NONE, egui::Key::ArrowRight);
const FOLDER_NEXT: (egui::Modifiers, egui::Key) = (egui::Modifiers::NONE, egui::Key::ArrowDown);
const FOLDER_PREV: (egui::Modifiers, egui::Key) = (egui::Modifiers::NONE, egui::Key::ArrowUp);
const FOLDER_KEYMAP: [(egui::Modifiers, egui::Key); 5] = [
    FOLDER_UP,
    FOLDER_CONFIRM,
    FOLDER_INTO,
    FOLDER_NEXT,
    FOLDER_PREV,
];

/// 키 누름 하나가 폴더 모드 노드 키맵에 맞는지. 본문 읽기(`PopupKeys::consume_keymap_key`, egui `consume_key`)와
/// 같은 규칙으로 패턴에 없는 Shift·Alt 가 붙은 누름도 맞는다. 폴더 모드인지는 부르는 쪽이 본다.
pub(crate) fn folder_keymap_receives(modifiers: egui::Modifiers, key: egui::Key) -> bool {
    FOLDER_KEYMAP
        .iter()
        .any(|(pattern, k)| *k == key && modifiers.matches_logically(*pattern))
}

/// 폴더 모드의 키. 키 통로의 노드 키맵 읽기라 egui 매처처럼 패턴에 없는 Shift·Alt 가 붙은 누름도 같은 조작이다. 수신
/// 판정은 창 키 경로가 했으므로(`folder_keymap_receives`) 이번 몫이 이 피커에 결속됐을 때만 읽는다. 그리기 시점 조건(열린
/// 하위 메뉴, 목록 상태)은 여기서 본다.
pub(super) fn keyboard(
    ui: &egui::Ui,
    p: &FilePickerProps<'_>,
    keys: &mut crate::adapters::ui::popup::keys::PopupKeys,
) -> Option<FilePickerAction> {
    if ui.memory(|m| m.any_popup_open())
        || !keys.receives()
        || !keys.routed_by_window()
        || !matches!(p.mode, FilePickerMode::Folder { .. })
    {
        return None;
    }
    let mut consume =
        |(modifiers, key): (egui::Modifiers, egui::Key)| keys.consume_keymap_key(modifiers, key);
    if consume(FOLDER_UP) {
        return Some(FilePickerAction::NavigateUp);
    }
    if !matches!(p.state, FpViewState::Loaded | FpViewState::Empty) {
        return None;
    }
    if consume(FOLDER_CONFIRM) {
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
    if consume(FOLDER_INTO) {
        return current.map(|i| FilePickerAction::NavigateInto(folders[i].clone()));
    }
    if folders.is_empty() {
        return None;
    }
    let next = if consume(FOLDER_NEXT) {
        Some(current.map_or(0, |i| (i + 1).min(folders.len() - 1)))
    } else if consume(FOLDER_PREV) {
        Some(current.map_or(folders.len() - 1, |i| i.saturating_sub(1)))
    } else {
        None
    };
    next.filter(|i| Some(*i) != current)
        .map(|i| FilePickerAction::Select(folders[i].clone()))
}

#[cfg(test)]
mod keymap_tests {
    use super::*;

    /// 본문 읽기가 쓰는 egui `consume_key`로 누름 하나를 이 패턴이 지우는지 잰다.
    fn body_consumes(
        event: (egui::Modifiers, egui::Key),
        pattern: (egui::Modifiers, egui::Key),
    ) -> bool {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            modifiers: event.0,
            events: vec![egui::Event::Key {
                key: event.1,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: event.0,
            }],
            ..Default::default()
        };
        ctx.begin_pass(raw);
        let hit = ctx.input_mut(|i| i.consume_key(pattern.0, pattern.1));
        let _output = ctx.end_pass();
        hit
    }

    /// 수신 판정은 본문 읽기(egui `consume_key`)가 표의 어느 패턴으로든 지우는 누름과 정확히 같다.
    #[test]
    fn the_folder_keymap_receives_what_the_body_consumes() {
        let mods = [
            egui::Modifiers::NONE,
            egui::Modifiers::SHIFT,
            egui::Modifiers::ALT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT | egui::Modifiers::SHIFT,
            egui::Modifiers::COMMAND,
        ];
        let mut hits = 0;
        for key in egui::Key::ALL {
            for m in mods {
                let body = FOLDER_KEYMAP
                    .iter()
                    .any(|pattern| body_consumes((m, *key), *pattern));
                assert_eq!(folder_keymap_receives(m, *key), body, "{m:?}+{key:?}");
                hits += usize::from(body);
            }
        }
        assert!(hits > 0);
        assert!(folder_keymap_receives(
            egui::Modifiers::ALT,
            egui::Key::ArrowUp
        ));
        assert!(folder_keymap_receives(
            egui::Modifiers::ALT,
            egui::Key::Enter
        ));
        assert!(!folder_keymap_receives(
            egui::Modifiers::CTRL,
            egui::Key::ArrowUp
        ));
    }
}
