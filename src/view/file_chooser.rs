//! 독립 View가 공유하는 로컬 파일·폴더 선택. 메인 파일 피커의 draw_file_picker_view를 재사용한다.
//! 별도 winit 창이므로 상태와 PopupManager는 설정 창이 소유한다.
//!
//! 원격 조회 없이 로컬 파일시스템만 읽으며 native 포털에 의존하지 않는다.
//! 디렉터리 조회는 App worker에 요청하고 현재 선택 세션의 receipt로만 결과를 받는다.
//! 저장 모드는 파일을 골라도 이름만 채우고 하단 버튼으로 확정한다.
//! 기능 문서: docs/features/native-file-picker/index.md.

#[cfg(test)]
mod folder_tests;
mod window;
pub(crate) use window::{WindowFileChooser, folder_size, owns_input, register, start_directory};

use crate::adapters::ui::popup::file_picker::save;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use tasty_type_appearance::theme::Theme;

// 크기는 메인 창 파일 피커와 같은 상수를 읽는다 — 저장 모드도 같은 640×480 프레임이다.
use crate::adapters::ui::popup::file_picker::{
    CrumbView, FilePickerAction, FilePickerEntryView, FilePickerMode, FilePickerProps, FpViewState,
    POPUP_HEIGHT, POPUP_WIDTH, crumb_label, draw_file_picker_view, matches_filters, path_ancestors,
};
use crate::core::fs_list::DirEntryInfo;
use crate::i18n::{t, t_count_template};

/// 설정 창 `PopupManager` 에 등록되는 id.
pub(crate) const FILE_CHOOSER_POPUP_ID: &str = "local_file_chooser";

/// 무엇을 고르는가.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileChooserMode {
    Folder,
    /// 기존 파일 하나를 고른다.
    Open,
    /// 디렉토리를 고르고 파일명을 입력해 저장 경로를 정한다. `default_name` 은 이름 칸의
    /// 초깃값이다.
    Save {
        default_name: String,
    },
}

/// 선택 결과. 메인 파일 피커처럼 경로 문자열 배열만 담는다. 경로의 쓰임은 호출처가 정한다.
/// 호출처가 자기 `consumer` 키로 [`LocalFileChooser::take_outcome`] 한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileChooserOutcome {
    Confirmed(Vec<String>),
    Cancelled,
}

impl FileChooserOutcome {
    /// 이 선택 화면은 단일 선택이라 확정 경로는 하나다.
    fn confirmed(path: PathBuf) -> Self {
        Self::Confirmed(vec![path.to_string_lossy().into_owned()])
    }

    /// 확정한 첫 경로. 취소면 None이다.
    pub(crate) fn into_first_path(self) -> Option<PathBuf> {
        match self {
            Self::Confirmed(paths) => paths.into_iter().next().map(PathBuf::from),
            Self::Cancelled => None,
        }
    }
}

/// 열려 있는 선택 화면 하나의 상태.
struct ChooserSession {
    consumer: &'static str,
    mode: FileChooserMode,
    current_dir: PathBuf,
    entries: Vec<DirEntryInfo>,
    load: FpViewState,
    query: Option<crate::app::local_reads::Query<Vec<DirEntryInfo>>>,
    /// 선택된 행. 저장 모드에서는 비었거나 `[save_name]`(나열된 파일) 둘 중 하나다.
    selected: Vec<String>,
    /// 확장자 필터(점 없이). 비면 필터 없음. 디렉토리는 거르지 않는다.
    filters: Vec<String>,
    /// 저장 모드의 파일명 입력.
    save_name: String,
    /// 호출처가 정한 타이틀. 없으면 모드 기본 문구.
    title: Option<&'static str>,
}

/// 독립 View가 소유하는 파일 선택 상태. 한 번에 하나만 열린다.
#[derive(Default)]
pub(crate) struct LocalFileChooser {
    session: Option<ChooserSession>,
    /// 닫힌 선택의 결과. 호출처가 가져갈 때까지 남는다.
    outcome: Option<(&'static str, FileChooserOutcome)>,
}

impl LocalFileChooser {
    pub(crate) fn is_pending(&self) -> bool {
        self.session.is_some() || self.outcome.is_some()
    }
    pub(crate) fn is_folder(&self) -> bool {
        self.session.as_ref().is_some_and(ChooserSession::is_folder)
    }

    pub(crate) fn poll_local_reads(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        if let Some(result) = session.query.as_mut().and_then(|query| query.poll(owner)) {
            session.query = None;
            session.accept_read(result);
            return true;
        }
        false
    }

    /// 선택 상태를 만들고 홈 디렉터리 읽기를 worker에 예약한다. popup을 여는 것은 호출처
    /// (`SettingsUiState::open_file_chooser`)다 — 이 타입은 popup 매니저를 모른다.
    pub(crate) fn begin(
        &mut self,
        consumer: &'static str,
        mode: FileChooserMode,
        filters: Vec<String>,
    ) {
        let start = directories::BaseDirs::new()
            .map(|d| d.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.begin_at(consumer, mode, filters, start);
    }

    pub(crate) fn begin_at(
        &mut self,
        consumer: &'static str,
        mode: FileChooserMode,
        filters: Vec<String>,
        start: PathBuf,
    ) {
        let save_name = match &mode {
            FileChooserMode::Save { default_name } => default_name.clone(),
            FileChooserMode::Open | FileChooserMode::Folder => String::new(),
        };
        let mut session = ChooserSession {
            consumer,
            mode,
            current_dir: start,
            entries: Vec::new(),
            load: FpViewState::Empty,
            query: None,
            selected: Vec::new(),
            filters,
            save_name,
            title: None,
        };
        session.reload();
        self.session = Some(session);
        self.outcome = None;
    }

    /// 열려 있는 선택의 타이틀을 호출처 문구로 바꾼다(popup 타이틀과 view 헤더 공용).
    pub(crate) fn set_title(&mut self, title: &'static str) {
        if let Some(s) = self.session.as_mut() {
            s.title = Some(title);
        }
    }

    /// 열려 있는 선택의 타이틀.
    pub(crate) fn title(&self) -> &'static str {
        match self.session.as_ref() {
            Some(s) => s.title(),
            None => chooser_title(false),
        }
    }

    /// popup 이 view 밖의 경로(타이틀바 ✕)로 닫혔을 때 — 미확정이면 취소로 남긴다.
    pub(crate) fn cancel(&mut self) {
        if let Some(s) = self.session.take() {
            self.outcome = Some((s.consumer, FileChooserOutcome::Cancelled));
        }
    }

    /// `consumer` 가 연 선택의 결과를 1 회 가져간다.
    pub(crate) fn take_outcome(&mut self, consumer: &'static str) -> Option<FileChooserOutcome> {
        if self.outcome.as_ref().is_some_and(|(c, _)| *c == consumer) {
            self.outcome.take().map(|(_, o)| o)
        } else {
            None
        }
    }

    /// popup 콘텐츠. 닫혀야 하면 `true`.
    pub(crate) fn draw(&mut self, ui: &mut egui::Ui, th: &Theme, owns_escape: bool) -> bool {
        let Some(s) = self.session.as_mut() else {
            return true;
        };
        let action = draw_view(ui, th, s, owns_escape);
        match s.apply(action) {
            Some(o) => {
                self.finish(o);
                true
            }
            None => false,
        }
    }

    fn finish(&mut self, outcome: FileChooserOutcome) {
        if let Some(s) = self.session.take() {
            self.outcome = Some((s.consumer, outcome));
        }
    }
}

/// popup 크기 — 두 모드 모두 메인 피커와 같은 640×480 이다. 저장 모드의 이름 칸·경고 줄은
/// footer 안에 있고 본문이 그만큼 준다.
pub(crate) fn chooser_size() -> egui::Vec2 {
    egui::vec2(POPUP_WIDTH.value(), POPUP_HEIGHT.value())
}

/// popup 타이틀. view 헤더도 같은 문자열을 쓴다.
pub(crate) fn chooser_title(save_mode: bool) -> &'static str {
    if save_mode {
        t("filepicker.save.title")
    } else {
        t("filepicker.title")
    }
}

impl ChooserSession {
    fn title(&self) -> &'static str {
        if self.is_folder() {
            return t("picker.folder.title");
        }
        self.title
            .unwrap_or_else(|| chooser_title(matches!(self.mode, FileChooserMode::Save { .. })))
    }

    /// 현재 디렉터리 조회를 요청한다. 이전 receipt와 선택은 버린다.
    fn reload(&mut self) {
        self.selected.clear();
        self.query = Some(crate::app::local_reads::directory(self.current_dir.clone()));
        self.entries.clear();
        self.load = FpViewState::Loading;
    }

    fn accept_read(&mut self, result: std::io::Result<Vec<DirEntryInfo>>) {
        match result {
            Ok(mut entries) => {
                crate::core::fs_list::sort_entries(
                    &mut entries,
                    tasty_model::SortColumn::Name,
                    tasty_model::SortDir::Asc,
                );
                self.load = if entries.is_empty() {
                    FpViewState::Empty
                } else {
                    FpViewState::Loaded
                };
                self.entries = entries;
            }
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::PermissionDenied {
                    t("filepicker.error_perm.reason_permission").to_string()
                } else {
                    e.to_string()
                };
                self.entries.clear();
                self.load = FpViewState::ErrorPerm(msg);
            }
        }
    }

    fn navigate(&mut self, target: PathBuf) {
        self.current_dir = target;
        self.reload();
    }

    fn ancestors(&self) -> Vec<String> {
        path_ancestors(false, &self.current_dir.to_string_lossy())
    }

    fn visible(&self, e: &DirEntryInfo) -> bool {
        self.is_folder() || e.is_dir || matches_filters(&self.filters, &e.name)
    }

    fn is_file_entry(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.name == name && !e.is_dir && self.visible(e))
    }

    /// 그 이름이 지금 나열된 **폴더** 행인가. 폴더는 필터와 무관하게 늘 보인다.
    fn is_dir_entry(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name == name && e.is_dir)
    }

    fn is_folder(&self) -> bool {
        matches!(self.mode, FileChooserMode::Folder)
    }

    fn folder_target(&self) -> PathBuf {
        self.selected
            .first()
            .filter(|name| self.is_dir_entry(name))
            .map_or_else(
                || self.current_dir.clone(),
                |name| self.current_dir.join(name),
            )
    }

    fn is_save(&self) -> bool {
        matches!(self.mode, FileChooserMode::Save { .. })
    }

    /// 저장 모드: 입력한 이름이 지금 나열된 폴더에 파일로 이미 있다(덮어쓰기 상태).
    /// 판정은 나열된 목록만 본다 — 나열되지 않은 경로를 stat 하지 않는다.
    fn overwrites(&self) -> bool {
        self.is_save() && save::overwrites(&self.save_name, &self.entries, &self.filters)
    }

    /// view 의 의도를 상태 변경으로 환원한다. 선택이 끝났으면 결과를 낸다.
    fn apply(&mut self, action: FilePickerAction) -> Option<FileChooserOutcome> {
        match action {
            FilePickerAction::None => None,
            FilePickerAction::ClearSelection => {
                self.selected.clear();
                None
            }
            FilePickerAction::Cancel | FilePickerAction::Escape => {
                Some(FileChooserOutcome::Cancelled)
            }
            // 단일 클릭은 선택만 한다. 저장 모드의 이름 칸은 파일을 골랐을 때만 바꾼다.
            FilePickerAction::Select(name) => {
                if self.is_folder() {
                    if !self.is_dir_entry(&name) {
                        return None;
                    }
                    if self.selected.first() == Some(&name) {
                        self.selected.clear();
                        return None;
                    }
                }
                if self.is_save() && self.is_file_entry(&name) {
                    self.save_name = name.clone();
                }
                self.selected = vec![name];
                None
            }
            FilePickerAction::EditName(name) => {
                if self.is_save() {
                    // 입력한 이름이 달라지면 기존 행 선택을 해제한다.
                    if self.selected.first() != Some(&name) {
                        self.selected.clear();
                    }
                    self.save_name = name;
                }
                None
            }
            FilePickerAction::NavigateInto(name) => {
                let target = self.current_dir.join(name);
                self.navigate(target);
                None
            }
            FilePickerAction::NavigateTo(idx) => {
                if let Some(target) = self.ancestors().get(idx) {
                    self.navigate(PathBuf::from(target));
                }
                None
            }
            FilePickerAction::NavigateUp => {
                let ancestors = self.ancestors();
                if ancestors.len() > 1 {
                    self.navigate(PathBuf::from(&ancestors[ancestors.len() - 2]));
                }
                None
            }
            FilePickerAction::Refresh => {
                self.reload();
                None
            }
            // 저장 모드의 확정 대상은 이름 칸 하나다.
            FilePickerAction::Confirm if self.is_folder() => {
                matches!(self.load, FpViewState::Loaded | FpViewState::Empty)
                    .then(|| FileChooserOutcome::confirmed(self.folder_target()))
            }
            FilePickerAction::Confirm if self.is_save() => self.confirm_save(),
            FilePickerAction::Confirm => {
                let [name] = self.selected.as_slice() else {
                    return None;
                };
                let name = name.clone();
                // 고른 것이 폴더면 확정은 **그 폴더로 들어간다** — 더블클릭 말고 키보드로
                // 내려가는 길이 이것이다(디자인 제스처 표).
                if self.is_dir_entry(&name) {
                    self.navigate(self.current_dir.join(&name));
                    return None;
                }
                // view 의 활성 조건을 우회해도 디렉토리를 파일로 확정하지 않는다.
                self.is_file_entry(&name)
                    .then(|| FileChooserOutcome::confirmed(self.current_dir.join(&name)))
            }
            // 저장 모드의 파일 행 더블클릭은 고르기와 같다 — 덮어쓰기 경고를 보지 않고 기존
            // 파일을 확정하는 길을 만들지 않는다.
            FilePickerAction::ConfirmEntry(_) if self.is_folder() => None,
            FilePickerAction::ConfirmEntry(name) if self.is_save() => {
                self.apply(FilePickerAction::Select(name))
            }
            FilePickerAction::ConfirmEntry(name) => self
                .is_file_entry(&name)
                .then(|| FileChooserOutcome::confirmed(self.current_dir.join(name))),
        }
    }

    /// 저장 모드 이름 칸의 확정 — 현재 디렉토리가 읽혔고 파일명이 한 경로 성분이며, 나열된
    /// 폴더 이름과 겹치지 않을 때만(폴더 자리에 파일을 쓸 수 없다).
    fn confirm_save(&self) -> Option<FileChooserOutcome> {
        save::confirmed_path(
            &self.current_dir,
            &self.save_name,
            matches!(self.load, FpViewState::Loaded | FpViewState::Empty),
            &self.entries,
        )
        .map(|path| FileChooserOutcome::Confirmed(vec![path]))
    }
}

fn draw_view(
    ui: &mut egui::Ui,
    th: &Theme,
    s: &ChooserSession,
    owns_escape: bool,
) -> FilePickerAction {
    let crumbs: Vec<CrumbView> = s
        .ancestors()
        .iter()
        .map(|full| CrumbView {
            label: crumb_label(false, full),
        })
        .collect();
    let entries: Vec<FilePickerEntryView> = s
        .entries
        .iter()
        .filter(|e| s.visible(e))
        .map(|e| FilePickerEntryView {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size_display: crate::core::fs_list::human_size(e.is_dir, e.size),
            modified_display: crate::core::fs_list::format_modified(e.modified),
        })
        .collect();
    let selection_text = s.selected.join(", ");
    let overwrite = s.overwrites();
    let target = s.folder_target().to_string_lossy().into_owned();
    let (mode, name_placeholder, confirm_label) = if s.is_folder() {
        (
            FilePickerMode::Folder {
                path: &target,
                hint: t(if s.selected.is_empty() {
                    "picker.folder.hint.current"
                } else {
                    "picker.folder.hint.selected"
                }),
                file_tooltip: t("picker.folder.file.tooltip"),
            },
            "",
            t("picker.folder.confirm"),
        )
    } else if s.is_save() {
        (
            FilePickerMode::Save {
                name: &s.save_name,
                overwrite,
                can_confirm: s.confirm_save().is_some(),
            },
            t("filepicker.save.name_placeholder"),
            if overwrite {
                t("filepicker.save.overwrite_button")
            } else {
                t("filepicker.save.save_button")
            },
        )
    } else {
        (
            FilePickerMode::Open {
                selection_text: &selection_text,
            },
            t("filepicker.no_file_selected"),
            t("filepicker.open_button"),
        )
    };
    let props = FilePickerProps {
        theme: th,
        remote_host: None,
        crumbs: &crumbs,
        state: s.load.clone(),
        entries: &entries,
        selected: &s.selected,
        mode,
        owns_escape,
        title_label: s.title(),
        name_field_label: t(if s.is_folder() {
            "picker.folder.label"
        } else {
            "filepicker.name_field_label"
        }),
        name_placeholder,
        cancel_label: t("button.cancel"),
        confirm_label,
        overwrite_warning: t("filepicker.save.overwrite_warning"),
        folder_not_save_target: t("filepicker.folder_not_save_target"),
        folder_open_enters: t("filepicker.folder_open_enters"),
        hidden_folders_one: t_count_template("filepicker.hidden_folders", 1),
        hidden_folders_many: t("filepicker.hidden_folders"),
        empty_label: t(if s.is_folder() {
            "picker.folder.empty"
        } else {
            "filepicker.empty"
        }),
        loading_label: t("filepicker.loading"),
        loading_body_local: t("filepicker.loading_body_local"),
        loading_body_remote: t("filepicker.loading_body_remote"),
        error_perm_title: t("filepicker.error_perm.title"),
        error_perm_retry: t("filepicker.error_perm.retry"),
        error_conn_title: t("filepicker.error_conn.title"),
        error_conn_reconnect: t("filepicker.error_conn.reconnect"),
        col_name: t("filepicker.column.name"),
        col_size: t("filepicker.column.size"),
        col_modified: t("filepicker.column.modified"),
        filters: &s.filters,
        filter_showing: t("filepicker.filter_showing"),
    };
    let out = draw_file_picker_view(ui, &props);
    crate::adapters::ui::popup::report_header_drag_rect(
        ui.ctx(),
        FILE_CHOOSER_POPUP_ID,
        out.header_rect,
    );
    out.action
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "test_consumer";

    fn tempdir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tasty-settings-file-chooser-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
        std::fs::write(dir.join("a.lua"), "x").expect("write");
        std::fs::write(dir.join("b.txt"), "x").expect("write");
        dir
    }

    fn settle(session: &mut ChooserSession) {
        if let Some(mut query) = session.query.take() {
            let mut owner = crate::app::local_reads::LocalReads::default();
            let result = owner.finish(&mut query);
            session.accept_read(result);
            while owner.poll_shutdown() != 0 {
                std::thread::yield_now();
            }
        }
    }
    fn session(chooser: &mut LocalFileChooser) -> &mut ChooserSession {
        let session = chooser.session.as_mut().expect("open session");
        settle(session);
        session
    }

    #[test]
    fn open_mode_confirms_existing_file_and_hands_path_to_consumer() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(C, FileChooserMode::Open, Vec::new(), dir.clone());
        assert!(session(&mut ch).load == FpViewState::Loaded);

        let s = session(&mut ch);
        assert_eq!(s.apply(FilePickerAction::Select("a.lua".into())), None);
        let out = s.apply(FilePickerAction::Confirm).expect("confirmed");
        ch.finish(out);

        assert_eq!(ch.take_outcome("someone_else"), None);
        assert_eq!(
            ch.take_outcome(C),
            Some(FileChooserOutcome::confirmed(dir.join("a.lua")))
        );
        assert_eq!(ch.take_outcome(C), None, "결과는 한 번만 가져간다");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 필터에서 제외된 파일이나 디렉터리를 파일로 확정하지 않는다.
    #[test]
    fn directories_and_filtered_out_files_are_not_confirmable() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(C, FileChooserMode::Open, vec!["lua".into()], dir.clone());
        let s = session(&mut ch);
        assert_eq!(s.apply(FilePickerAction::ConfirmEntry("sub".into())), None);
        assert_eq!(
            s.apply(FilePickerAction::ConfirmEntry("b.txt".into())),
            None
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 디자인 제스처 표 전 갈래 — 저장/열기 두 모드 × 파일/폴더 두 종류.
    ///
    /// | | 단일 클릭 | 더블클릭 |
    /// |---|---|---|
    /// | 폴더 | 고른다(두 모드) | 들어간다(두 모드) |
    /// | 파일 | 고른다(저장은 이름 칸도 채운다) | 열기는 확정 · 저장은 고르기까지 |
    ///
    /// 그리고 **확정**: 열기 모드에서 폴더를 고른 채 확정하면 들어간다(키보드로 내려가는 길).
    #[test]
    fn the_gesture_table_holds_in_both_modes() {
        let dir = tempdir();

        // ── 열기 모드 ────────────────────────────────────────────────
        let mut ch = LocalFileChooser::default();
        ch.begin_at(C, FileChooserMode::Open, Vec::new(), dir.clone());
        let s = session(&mut ch);
        s.apply(FilePickerAction::Select("sub".into()));
        assert_eq!(
            s.selected,
            vec!["sub".to_string()],
            "폴더를 한 번 클릭하면 선택되어야 한다"
        );
        assert_eq!(
            s.apply(FilePickerAction::Confirm),
            None,
            "선택한 폴더를 열 때 파일 선택 결과를 반환하면 안 된다"
        );
        assert_eq!(
            s.current_dir,
            dir.join("sub"),
            "확정하면 선택한 폴더로 이동해야 한다"
        );
        s.apply(FilePickerAction::NavigateUp);
        settle(s);
        s.apply(FilePickerAction::Select("b.txt".into()));
        assert_eq!(
            s.apply(FilePickerAction::ConfirmEntry("b.txt".into())),
            Some(FileChooserOutcome::confirmed(dir.join("b.txt"))),
            "열기 모드 파일 더블클릭이 확정이 아니다"
        );

        // ── 저장 모드 ────────────────────────────────────────────────
        let mut ch = save_session(&dir);
        let s = session(&mut ch);
        s.apply(FilePickerAction::Select("sub".into()));
        assert_eq!(
            s.selected,
            vec!["sub".to_string()],
            "저장 모드에서도 폴더를 한 번 클릭하면 선택되어야 한다"
        );
        assert_eq!(s.save_name, "keys.toml", "폴더가 이름 칸을 건드렸다");
        assert_eq!(
            s.apply(FilePickerAction::ConfirmEntry("sub".into())),
            None,
            "저장 모드의 폴더 더블클릭은 저장 결과를 반환하면 안 된다"
        );
        s.apply(FilePickerAction::NavigateInto("sub".into()));
        settle(s);
        assert_eq!(
            s.current_dir,
            dir.join("sub"),
            "폴더를 더블클릭하면 해당 폴더로 이동해야 한다"
        );
        s.apply(FilePickerAction::NavigateUp);
        settle(s);
        assert_eq!(
            s.apply(FilePickerAction::ConfirmEntry("b.txt".into())),
            None,
            "저장 모드의 파일 더블클릭은 저장을 확정하면 안 된다"
        );
        assert_eq!(
            s.save_name, "b.txt",
            "저장 모드의 파일 더블클릭은 이름 칸을 채워야 한다"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn navigation_moves_into_and_back_up() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(C, FileChooserMode::Open, Vec::new(), dir.clone());
        let s = session(&mut ch);
        s.apply(FilePickerAction::NavigateInto("sub".into()));
        settle(s);
        assert_eq!(s.current_dir, dir.join("sub"));
        assert!(s.load == FpViewState::Empty);
        s.apply(FilePickerAction::NavigateUp);
        settle(s);
        assert_eq!(s.current_dir, dir);
        let root_idx = 0;
        s.apply(FilePickerAction::NavigateTo(root_idx));
        assert_eq!(s.current_dir.parent(), None, "첫 경로 항목은 루트다");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unreadable_directory_is_an_error_state_not_a_hang() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(
            C,
            FileChooserMode::Open,
            Vec::new(),
            dir.join("does-not-exist"),
        );
        assert!(matches!(session(&mut ch).load, FpViewState::ErrorPerm(_)));
        assert_eq!(session(&mut ch).confirm_save(), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_mode_joins_typed_name_and_rejects_path_like_names() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(
            C,
            FileChooserMode::Save {
                default_name: "keys.toml".into(),
            },
            Vec::new(),
            dir.clone(),
        );
        let s = session(&mut ch);
        assert!(s.is_save());
        assert_eq!(
            s.confirm_save(),
            Some(FileChooserOutcome::confirmed(dir.join("keys.toml")))
        );
        for bad in [
            "",
            "  ",
            ".",
            "..",
            "sub/x.toml",
            "../x.toml",
            "a\\b",
            "sub",
        ] {
            s.save_name = bad.into();
            assert_eq!(s.confirm_save(), None, "{bad:?} 는 저장 이름이 아니다");
        }
        // 기존 파일을 고르면 그 이름이 입력으로 간다.
        s.apply(FilePickerAction::Select("b.txt".into()));
        assert_eq!(s.save_name, "b.txt");
        // 폴더를 골라도 저장 파일명은 바꾸지 않는다.
        s.apply(FilePickerAction::Select("sub".into()));
        assert_eq!(s.save_name, "b.txt");
        assert_eq!(s.selected, vec!["sub".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn save_session(dir: &Path) -> LocalFileChooser {
        let mut ch = LocalFileChooser::default();
        ch.begin_at(
            C,
            FileChooserMode::Save {
                default_name: "keys.toml".into(),
            },
            Vec::new(),
            dir.to_path_buf(),
        );
        ch
    }

    /// 저장 모드의 확정 대상은 이름 칸 하나다 — 목록에서 고른 뒤 이름을 고치면 확정은 고친
    /// 이름을 돌려주고, 고른 행은 풀린다.
    #[test]
    fn save_mode_has_one_target_the_name_field() {
        let dir = tempdir();
        let mut ch = save_session(&dir);
        let s = session(&mut ch);

        // 새 이름 — 덮어쓰기 아님.
        assert!(!s.overwrites());

        // 기존 파일을 고르면 확정이 아니라 이름 칸이 채워지고, 곧 덮어쓰기 상태다.
        assert_eq!(s.apply(FilePickerAction::Select("a.lua".into())), None);
        assert_eq!(s.save_name, "a.lua");
        assert_eq!(s.selected, vec!["a.lua".to_string()]);
        assert!(s.overwrites());

        // 이름을 고친 순간 선택이 풀리고 덮어쓰기에서 벗어난다.
        assert_eq!(s.apply(FilePickerAction::EditName("a2.lua".into())), None);
        assert!(
            s.selected.is_empty(),
            "이름이 고른 행과 달라지면 선택이 풀린다"
        );
        assert!(!s.overwrites());

        // 확정은 이름 칸을 읽는다 — 앞서 고른 파일이 아니다.
        assert_eq!(
            s.apply(FilePickerAction::Confirm),
            Some(FileChooserOutcome::confirmed(dir.join("a2.lua")))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 고른 이름과 같은 값으로의 편집(예: 같은 글자를 지웠다 다시 씀)은 선택을 유지한다.
    #[test]
    fn save_mode_keeps_selection_while_the_name_still_matches() {
        let dir = tempdir();
        let mut ch = save_session(&dir);
        let s = session(&mut ch);
        s.apply(FilePickerAction::Select("a.lua".into()));
        s.apply(FilePickerAction::EditName("a.lua".into()));
        assert_eq!(s.selected, vec!["a.lua".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 직접 입력한 이름이 나열된 파일과 같아도 덮어쓰기 상태다(판정은 나열된 폴더 조회).
    #[test]
    fn typed_existing_name_is_an_overwrite() {
        let dir = tempdir();
        let mut ch = save_session(&dir);
        let s = session(&mut ch);
        s.apply(FilePickerAction::EditName("b.txt".into()));
        assert!(s.overwrites());
        assert!(s.selected.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 저장 모드에서 파일 행 더블클릭은 확정하지 않는다 — 덮어쓰기 경고를 건너뛰는 길이다.
    #[test]
    fn save_mode_double_click_fills_the_name_instead_of_confirming() {
        let dir = tempdir();
        let mut ch = save_session(&dir);
        let s = session(&mut ch);
        assert_eq!(
            s.apply(FilePickerAction::ConfirmEntry("a.lua".into())),
            None
        );
        assert_eq!(s.save_name, "a.lua");
        assert!(s.overwrites());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn external_close_leaves_a_cancel_outcome() {
        let dir = tempdir();
        let mut ch = LocalFileChooser::default();
        ch.begin_at(C, FileChooserMode::Open, Vec::new(), dir.clone());
        ch.cancel();
        assert_eq!(ch.take_outcome(C), Some(FileChooserOutcome::Cancelled));
        std::fs::remove_dir_all(&dir).ok();
    }
}
