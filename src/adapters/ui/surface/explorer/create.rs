//! 새 폴더·새 파일. 목록 맨 위의 인라인 이름 입력과 이름 검사.
//! 입력 중에는 정렬 자리로 옮기지 않아 줄이 튀지 않는다. 확정하면 `ExplorerAction::Create`
//! 를 내고, 만든 뒤 다시 읽은 목록에서 정렬 자리로 간다. 원격 폴더면 server 의 답이 올 때까지 줄을
//! 제자리에 읽기 전용으로 남긴다. 실패하면 글자를 그대로 두고 다시 편집하게 한다.

use std::path::{Path, PathBuf};

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    ExplorerNameEdit, ExplorerNameEvent, ExplorerNameLayout, ExplorerNameOptions,
    explorer_name_error, explorer_name_row_in,
};

use super::ExplorerAction;
use super::view::ExplorerView;
use crate::i18n::{t, t_fmt};

pub(super) mod place;

/// 이름을 받고 있는 새 항목. 대상 폴더는 명령을 시작할 때 정한다.
#[derive(Clone, Debug)]
pub(crate) struct CreateEdit {
    pub(crate) dir: PathBuf,
    pub(crate) folder: bool,
    pub(crate) edit: ExplorerNameEdit,
    /// 대상 폴더에 이미 있는 이름. 명령을 시작할 때 알던 목록이다.
    taken: Vec<String>,
    /// Enter 에서 이미 있다고 확인한 이름. 글자를 바꾸면 이 오류는 사라진다.
    exists: Option<String>,
    /// Windows 이름 규칙을 쓰는가. 원격 폴더면 그 경로의 형식, 로컬이면 이 OS 를 따른다.
    windows: bool,
    /// 대소문자만 다른 이름을 같은 이름으로 보는가.
    fold_case: bool,
    /// 확정한 이름을 원격이 만들기를 기다린다. 끝나면 App 이 줄을 닫거나 다시 편집하게 한다.
    pub(crate) waiting: bool,
}

/// 지금 목록에 없는 폴더(폴더 행 메뉴의 대상)에 만들기 전의 쓰기 가능 확인.
pub(crate) struct WriteCheck {
    dir: PathBuf,
    folder: bool,
    /// 요청할 때 보던 폴더. 결과가 오기 전에 다른 폴더로 가면 입력을 열지 않는다.
    shown: Option<PathBuf>,
    query: crate::app::local_reads::Query<bool>,
}

/// 이름 오류.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NameError {
    Empty,
    InvalidChar(char),
    /// 앞이나 뒤에 공백이 있다. 조용히 떼지 않고 거절한다.
    EdgeSpace,
    Reserved,
    Exists,
}

/// Windows 가 파일 이름에 받지 않는 글자. 제어 문자도 받지 않는다.
const WINDOWS_INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// 입력하는 동안 보는 검사. 이미 있는지는 Enter 에서 따로 본다.
/// 공백만인 이름은 빈 이름이다. 앞뒤 공백은 모든 OS 에서 거절한다. 사용자가 일부러 넣었을 수도 있어
/// 떼어 내면 입력한 이름과 달라지고, 남겨 두면 눈에 보이지 않는 차이가 생기기 때문이다.
pub(crate) fn check_name(name: &str, windows: bool) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    let invalid = |c: char| {
        c == '/' || c == '\0' || (windows && (WINDOWS_INVALID.contains(&c) || (c as u32) < 0x20))
    };
    if let Some(c) = name.chars().find(|c| invalid(*c)) {
        return Err(NameError::InvalidChar(c));
    }
    if name.starts_with(char::is_whitespace) || name.ends_with(char::is_whitespace) {
        return Err(NameError::EdgeSpace);
    }
    // Windows 는 끝의 점을 떼고 만들어 다른 이름이 되므로 예약 이름처럼 막는다.
    let trailing = windows && name.ends_with('.');
    if name == "." || name == ".." || trailing || (windows && reserved_on_windows(name)) {
        return Err(NameError::Reserved);
    }
    Ok(())
}

/// CON·PRN·AUX·NUL·COM1~9·LPT1~9 는 확장자가 붙어도 장치 이름이다.
fn reserved_on_windows(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
}

/// `dir` 에 만들 이름에 Windows 규칙을 쓰는가. 원격 폴더는 서버 OS 를 모르므로 경로 형식으로 고르고,
/// 서버가 최종 판정한다.
pub(crate) fn windows_rules(dir: &Path, remote: bool) -> bool {
    if remote {
        super::address::remote::is_windows_style(&dir.to_string_lossy())
    } else {
        cfg!(windows)
    }
}

impl NameError {
    pub(crate) fn message(&self, name: &str) -> String {
        match self {
            Self::Empty => t("explorer.name.empty").to_string(),
            Self::InvalidChar(c) => t_fmt("explorer.name.invalid_char", &c.to_string()),
            Self::EdgeSpace => t("explorer.name.edge_space").to_string(),
            Self::Reserved => t("explorer.name.reserved").to_string(),
            Self::Exists => t_fmt("explorer.name.exists", name),
        }
    }
}

/// `dir` 의 파일시스템이 대소문자만 다른 이름을 같은 항목으로 보는가. macOS·Windows 의 기본
/// 파일시스템은 그렇다. 원격 폴더는 Windows 형식 경로만 그렇게 보고, 나머지는 서버가 판정한다.
pub(crate) fn case_insensitive(dir: &Path, remote: bool) -> bool {
    if remote {
        super::address::remote::is_windows_style(&dir.to_string_lossy())
    } else {
        cfg!(any(target_os = "macos", windows))
    }
}

fn same_name(a: &str, b: &str, fold_case: bool) -> bool {
    a == b || (fold_case && a.to_lowercase() == b.to_lowercase())
}

/// `base` 가 이미 있으면 "{stem} 2{.ext}", "{stem} 3{.ext}" 순으로 비는 이름을 찾는다.
pub(crate) fn default_name(base: &str, taken: &[String], fold_case: bool) -> String {
    let is_taken = |n: &str| taken.iter().any(|t| same_name(t, n, fold_case));
    if !is_taken(base) {
        return base.to_string();
    }
    let (stem, ext) = split_ext(base);
    (2u32..)
        .map(|i| format!("{stem} {i}{ext}"))
        .find(|n| !is_taken(n))
        .expect("an unused name exists")
}

/// 마지막 점부터를 확장자로 본다. 맨 앞의 점(숨김 파일)은 확장자가 아니다.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

impl CreateEdit {
    fn new(dir: PathBuf, folder: bool, taken: Vec<String>, remote: bool) -> Self {
        let base = if folder {
            t("explorer.new.folder_default")
        } else {
            t("explorer.new.file_default")
        };
        let fold_case = case_insensitive(&dir, remote);
        let name = default_name(base, &taken, fold_case);
        // 폴더는 이름 전체를, 파일은 확장자 앞까지 고른다.
        let selected = if folder {
            name.chars().count()
        } else {
            split_ext(&name).0.chars().count()
        };
        Self {
            windows: windows_rules(&dir, remote),
            fold_case,
            dir,
            folder,
            edit: ExplorerNameEdit::new(name, 0..selected),
            taken,
            exists: None,
            waiting: false,
        }
    }

    /// 기다리는 동안 만들기를 요청한 경로.
    pub(crate) fn waiting_path(&self) -> Option<PathBuf> {
        self.waiting.then(|| self.dir.join(&self.edit.buf))
    }

    /// 답 없이 끝난 기다림을 풀어 다시 편집하게 한다. 글자는 두고 커서를 끝으로 옮긴다.
    pub(crate) fn resume_editing(&mut self) {
        self.waiting = false;
        let end = self.edit.buf.chars().count();
        self.edit.initial_selection = Some(end..end);
    }

    /// 지금 보일 오류. 입력하는 동안의 검사와 Enter 에서 확인한 "이미 있음" 이다.
    fn error(&self) -> Option<NameError> {
        let name = self.edit.buf.as_str();
        check_name(name, self.windows)
            .err()
            .or_else(|| (self.exists.as_deref() == Some(name)).then_some(NameError::Exists))
    }

    /// 확정할 수 있는 이름이면 그대로, 아니면 오류.
    fn confirmable(&self) -> Result<(), NameError> {
        check_name(&self.edit.buf, self.windows)?;
        if self
            .taken
            .iter()
            .any(|n| same_name(n, &self.edit.buf, self.fold_case))
        {
            return Err(NameError::Exists);
        }
        Ok(())
    }
}

impl ExplorerView {
    /// 새 항목 이름 입력을 연다. 지금 보는 폴더가 아닌 로컬 폴더면 읽기 worker 에 쓰기 가능 여부를
    /// 먼저 묻고, 쓸 수 있으면 입력을 연다. 지금 보는 폴더는 목록과 함께 확인한 값을 호출자가 본다.
    /// 원격 폴더의 쓰기 권한은 server 가 만들 때 판정한다.
    pub(crate) fn request_create(&mut self, dir: PathBuf, folder: bool) {
        if self.is_remote() || self.shown_dir() == Some(dir.as_path()) {
            self.create_check = None;
            self.start_create(dir, folder);
            return;
        }
        self.create = None;
        self.create_check = Some(WriteCheck {
            query: crate::app::local_reads::writable(dir.clone()),
            shown: self.shown_dir().map(Path::to_path_buf),
            dir,
            folder,
        });
    }

    /// 대상 폴더 확인 결과를 받아 입력을 열거나 거절을 남긴다. 받았으면 true.
    pub(super) fn poll_create_check(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) -> bool {
        let Some(result) = self.create_check.as_mut().and_then(|c| c.query.poll(owner)) else {
            return false;
        };
        let Some(check) = self.create_check.take() else {
            return false;
        };
        if self.shown_dir() != check.shown.as_deref() {
            return false;
        }
        // 확인하지 못했으면 쓸 수 있다고 보고 만들 때의 오류로 알린다.
        if result.unwrap_or(true) {
            self.start_create(check.dir, check.folder);
        } else {
            self.create_refused = true;
        }
        true
    }

    /// 대상 폴더에 쓸 수 없어 입력을 열지 않은 요청이 있었는가. 읽으면 지운다.
    pub(crate) fn take_create_refused(&mut self) -> bool {
        std::mem::take(&mut self.create_refused)
    }

    /// 쓰기 가능 확인을 기다리는 만들기 요청이 있는가. 읽기 전용 폴더를 만들 수 있는 Unix 시험만 쓴다.
    #[cfg(all(test, unix))]
    pub(crate) fn create_pending(&self) -> bool {
        self.create_check.is_some()
    }

    /// 새 항목 이름 입력을 연다. 이미 열려 있으면 새로 시작한다.
    /// 대상 폴더의 이름 목록은 지금 보는 목록(숨김 파일을 끈 동안 뺀 항목 포함)이나 사이드바 트리에서
    /// 읽은 것만 쓴다.
    /// 화면 스레드는 파일시스템을 읽지 않으므로, 모르는 이름과 겹치면 만들 때 실패로 알린다.
    pub(crate) fn start_create(&mut self, dir: PathBuf, folder: bool) {
        let taken: Vec<String> = if self.shown_dir() == Some(dir.as_path()) {
            let shown = self.entries.iter().map(|e| e.name.as_str());
            shown
                .chain(self.hidden_names())
                .map(str::to_owned)
                .collect()
        } else {
            self.tree_children
                .get(&dir)
                .map(|children| children.iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default()
        };
        self.create = Some(CreateEdit::new(dir, folder, taken, self.is_remote()));
    }
}

/// 보기 모드별 편집 줄 배치.
pub(super) enum Slot {
    /// 상세 표가 비워 둔 행 위에 겹쳐 그린다. 표 전체 폭과 이름 칸 rect.
    Detail {
        row: egui::Rect,
        name_cell: egui::Rect,
    },
    List,
    Grid,
}

/// 상세 표의 가짜 행 이름 칸 위치를 받아 편집 줄을 겹쳐 그린다.
pub(super) fn detail_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    name_cell: Option<egui::Rect>,
    action: &mut Option<ExplorerAction>,
) {
    let Some(name_cell) = name_cell else {
        return;
    };
    let row = egui::Rect::from_x_y_ranges(
        ui.max_rect().x_range(),
        egui::Rangef::point(name_cell.center().y).expand(theme.table_cell_height().value() * 0.5),
    );
    name_row(ui, theme, view, Slot::Detail { row, name_cell }, action);
}

/// 편집 줄을 그리고 입력을 처리한다. 확정하면 `Create` 를 낸다.
pub(super) fn name_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    slot: Slot,
    action: &mut Option<ExplorerAction>,
) {
    // 폴더 행 아래에 열면 들여 쓰고 "in {folder}" 를 붙인다(Grid 는 칸 자리만 바뀐다).
    let caption = place::target_row(view).and(view.create.as_ref()).map(|c| {
        let name = c.dir.file_name().map(|n| n.to_string_lossy().into_owned());
        t_fmt(
            "explorer.create.in_folder",
            name.as_deref().unwrap_or_default(),
        )
    });
    let waiting = view
        .create
        .as_ref()
        .is_some_and(|c| c.waiting)
        .then(|| remote_waiting(view.remote_host.as_ref()));
    let options = ExplorerNameOptions {
        // 대상 폴더가 멀리 있으면 줄을 화면 밖 자리에 따로 그린다. 그리는 순서가 바뀌어도 같은 입력이다.
        field_id: view
            .create
            .as_ref()
            .map(|c| egui::Id::new(("explorer_create_field", &c.dir, c.folder))),
        indent: caption
            .as_ref()
            .map_or(0.0, |_| theme.explorer_create_indent().value()),
        caption: caption.as_deref(),
        pending: waiting.as_deref(),
    };
    let Some(create) = view.create.as_mut() else {
        return;
    };
    let glyph = if create.folder {
        crate::adapters::ui::icons::FOLDER
    } else {
        crate::adapters::ui::icons::FILE
    };
    let before = create.edit.buf.clone();
    let first_frame = create.edit.initial_selection.is_some();
    let error = create.error().filter(|_| !create.waiting);
    let (field, event) = match slot {
        Slot::Detail { row, name_cell } => {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row));
            let layout = ExplorerNameLayout::Detail {
                inset: name_cell.left() - row.left(),
                name_width: name_cell.right() - row.left(),
            };
            let (_, field, event) = explorer_name_row_in(
                &mut child,
                theme,
                layout,
                options,
                glyph,
                &mut create.edit,
                error.is_some(),
            );
            (field, event)
        }
        Slot::List => {
            let (_, field, event) = explorer_name_row_in(
                ui,
                theme,
                ExplorerNameLayout::List,
                options,
                glyph,
                &mut create.edit,
                error.is_some(),
            );
            (field, event)
        }
        Slot::Grid => {
            // 다른 Grid 칸의 글리프 자리와 같은 높이여야 글리프가 한 줄에 선다.
            let slot = theme.icon_glyph_size_md.value();
            let pad = theme.spacing_sm.value();
            let cell = egui::vec2(
                super::CELL_W.value(),
                pad + slot + theme.spacing_xs.value() + theme.input_height().value() + pad,
            );
            let (_, field, event) = explorer_name_row_in(
                ui,
                theme,
                ExplorerNameLayout::Grid { cell, slot },
                options,
                glyph,
                &mut create.edit,
                error.is_some(),
            );
            (field, event)
        }
    };
    if first_frame {
        ui.scroll_to_rect(field, None);
    }
    if create.edit.buf != before {
        create.exists = None;
    }
    if let Some(error) = &error {
        explorer_name_error(ui, theme, field, &error.message(&create.edit.buf));
    }
    match event {
        ExplorerNameEvent::None => {}
        ExplorerNameEvent::Cancel => view.create = None,
        ExplorerNameEvent::Confirm => match create.confirmable() {
            Ok(()) => confirm(view, action),
            Err(error) => {
                if error == NameError::Exists {
                    create.exists = Some(create.edit.buf.clone());
                }
                // 입력은 열어 두고 글자를 유지한다. 커서는 끝으로 둔다.
                let end = create.edit.buf.chars().count();
                create.edit.initial_selection = Some(end..end);
            }
        },
        ExplorerNameEvent::Blur => {
            if create.confirmable().is_ok() {
                confirm(view, action);
            } else {
                view.create = None;
            }
        }
    }
}

/// 이름을 확정해 `Create` 를 낸다. 로컬은 줄을 닫고, 원격은 답을 기다리는 동안 줄을 남긴다.
fn confirm(view: &mut ExplorerView, action: &mut Option<ExplorerAction>) {
    if action.is_some() {
        view.create = None;
        return;
    }
    let remote = view.is_remote();
    let Some(create) = view.create.as_mut() else {
        return;
    };
    *action = Some(ExplorerAction::Create {
        dir: create.dir.clone(),
        name: create.edit.buf.clone(),
        folder: create.folder,
    });
    if remote {
        create.waiting = true;
    } else {
        view.create = None;
    }
}

/// 상세 표에서 편집 줄 자리를 비워 둘 가짜 행. 이름이 비어 있어 실제 항목과 겹치지 않는다.
pub(super) fn placeholder_row(dir: &Path) -> super::DirEntryInfo {
    super::DirEntryInfo {
        path: dir.to_path_buf(),
        name: String::new(),
        is_dir: false,
        size: 0,
        modified: None,
        ext: String::new(),
        link: crate::core::fs_list::EntryLink::NotALink,
        os_hidden: false,
        change: None,
    }
}

/// 원격의 답을 기다리는 새 항목 줄의 Spinner 툴팁. 호스트 글자가 비면(프로필 이름이 없는 attach)
/// 자리표시 없는 문구다.
pub(crate) fn remote_waiting(host: Option<&crate::remote::state::ExplorerRemoteHost>) -> String {
    match host.map(|h| h.label.as_str()).unwrap_or_default() {
        "" => t("explorer.remote.waiting_no_host").to_string(),
        label => t_fmt("explorer.remote.waiting", label),
    }
}

#[cfg(test)]
mod tests;
