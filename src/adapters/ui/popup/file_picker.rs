//! Tasty 자체 파일 열기 팝업. 로컬과 원격 디렉터리 조회를 같은 UI로 제공한다.
//! 동작은 `docs/features/native-file-picker/index.md`, 원격 조회 권한은 docs/features/remote-attach/index.md를 따른다.
//! OS의 로컬 파일 선택 대화상자는 원격 경로를 탐색할 수 없어 이 화면을 따로 둔다.
//!
//! 표시 함수는 FilePickerProps를 받아 동작을 반환하며 갤러리에서 따로 그릴 수 있다.
//! 로컬 목록은 App의 제한된 read worker에서 읽고 현재 popup receipt에 반영한다.
//! 원격 목록은 pending_list_dir_forward에 넣고 attach 응답으로 갱신한다.
//! 응답 제한은 매 프레임 sent_at의 경과 시간으로 확인하며 로컬 읽기에는 적용하지 않는다.

#[cfg(test)]
mod admission_tests;
mod folder;
mod footer;
#[cfg(test)]
mod layout_tests;
mod location;
mod path_bar;
#[cfg(test)]
mod pure_return_tests;
mod rows;

mod paths;
pub(crate) mod save;
#[cfg(test)]
mod save_tests;

pub(crate) use location::locate;
pub(crate) use paths::{crumb_label, join_dir, matches_filters, path_ancestors};

use crate::runtime::engine_read::EngineRead;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::icons;

/// breadcrumb 구분자 크기. 대응 토큰이 없어 갤러리와 같은 별도 값을 사용한다.
pub(super) const CRUMB_GLYPH: LogicalPx = LogicalPx(13.0);
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::{t, t_count_template};
use crate::state::{FilePickerResult, FpLoadState, MainViewState};
use crate::theme::{self, Theme};
use tasty_ui_widgets::{CenterState, ControlSize, IconButton, IconButtonVariant};

pub const FILE_PICKER_POPUP_ID: &str = "file_picker";

pub(crate) const POPUP_WIDTH: LogicalPx = LogicalPx(640.0);
pub(crate) const POPUP_HEIGHT: LogicalPx = LogicalPx(480.0);

// FilePickerFrame/FpRow 열 치수 — gallery specimen 과 같은 구조 폭.
const SIZE_COL_W: LogicalPx = LogicalPx(68.0);
const MOD_COL_W: LogicalPx = LogicalPx(108.0);

// 원격 연결 화면과 같은 공용 중앙 안내 영역 치수.
/// 원격 응답이 이 시간 안에 오지 않으면 `ErrorConn` 으로 전이(soft timeout — 세션의
/// `disconnected` 플래그만으론 "서버는 살아있는데 응답이 안 오는" 케이스를 못 잡는다).
const LIST_DIR_SOFT_TIMEOUT: Duration = Duration::from_secs(8);

/// PopupDef.sizer — 고정 640×480(gallery specimen `FRAME_W`/`FRAME_H`).
pub fn picker_sizer(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    egui::vec2(POPUP_WIDTH.value(), POPUP_HEIGHT.value())
}

/// 갤러리에서도 사용할 수 있도록 표시 문자열로 준비한 목록 행.
#[derive(Clone)]
pub struct FilePickerEntryView {
    pub name: String,
    pub is_dir: bool,
    pub size_display: String,
    pub modified_display: String,
}

/// 순수 시각 view 의 로드 상태. gallery specimen `FpState` 와 1:1 대응.
#[derive(Clone, PartialEq, Eq)]
pub enum FpViewState {
    Loading,
    Empty,
    ErrorPerm(String),
    ErrorConn(String),
    Loaded,
}

/// 브레드크럼 한 항목의 표시 라벨.
#[derive(Clone)]
pub struct CrumbView {
    pub label: String,
}

/// 피커가 무엇을 하는가 — footer 이름 칸의 성질이 여기서 갈린다.
#[derive(Clone, Copy)]
pub enum FilePickerMode<'a> {
    /// A single local folder: selected row or the current directory.
    Folder {
        path: &'a str,
        hint: &'a str,
        file_tooltip: &'a str,
    },
    /// 기존 파일을 고른다. 이름 칸은 읽기 전용으로 현재 선택을 보여준다.
    Open { selection_text: &'a str },
    /// 경로를 만든다. 이름 칸이 **유일한 확정 대상**이고 편집 가능하다.
    Save {
        name: &'a str,
        /// 입력한 이름이 지금 나열된 폴더에 이미 있다 — 경고 줄을 띄운다.
        overwrite: bool,
        /// 확정 버튼 활성 여부(빈 이름·경로 같은 이름이면 `false`). 판정은 wrapper 가 한다.
        can_confirm: bool,
    },
}

/// 순수 시각 view 의 입력. MainViewState/CoreState 의존 없음.
pub struct FilePickerProps<'a> {
    pub theme: &'a Theme,
    /// `Some(host)` 면 헤더에 host 배지 렌더(원격 브라우징).
    pub remote_host: Option<&'a str>,
    /// root 부터 현재 폴더까지 **전체** 경로. 가운데 생략은 렌더 규칙이라 여기서 줄이지 않는다
    /// — `…` 메뉴가 숨긴 조상을 열어야 한다.
    pub crumbs: &'a [CrumbView],
    pub state: FpViewState,
    pub entries: &'a [FilePickerEntryView],
    /// 선택된 엔트리 이름(현재 디렉토리 기준).
    pub selected: &'a [String],
    pub mode: FilePickerMode<'a>,
    /// 이 프레임에 Esc 를 소비할 자격이 있는가(docs/design/systems/popup.md).
    /// `false` 면 위에 다른 popup 이 있다는 뜻이라 Esc 를 무시한다 — 한 번의 Esc 로
    /// 스택 전체가 닫히는 것을 막는다. 판정은 `MainViewState.popup_escape_owner`.
    pub owns_escape: bool,

    // i18n — 호출처가 t() 로 미리 해상해서 전달(file_handler_picker.rs 관례).
    pub title_label: &'a str,
    pub name_field_label: &'a str,
    /// 이름 칸이 비었을 때의 placeholder(열기: 선택 없음 / 저장: 이름 입력 안내).
    pub name_placeholder: &'a str,
    pub cancel_label: &'a str,
    /// footer primary 버튼 라벨(Open / Save / Overwrite — 모드와 덮어쓰기 여부로 호출처가 고른다).
    pub confirm_label: &'a str,
    /// 덮어쓰기 경고 줄. `{name}` 자리에 이름이 mono 로 들어간다.
    pub overwrite_warning: &'a str,
    /// 저장 모드에서 선택한 폴더가 저장 대상은 아니라는 안내. {name}은 고정폭 글꼴로 표시한다.
    pub folder_not_save_target: &'a str,
    /// 열기 모드에서 폴더 행을 고른 상태의 안내 줄. `{name}` 은 폴더 이름(mono),
    /// `{confirm}` 은 확정 버튼 이름이다 — 그 버튼이 곧 키보드로 들어가는 길이다.
    pub folder_open_enters: &'a str,
    /// 숨긴 조상이 하나일 때의 툴팁 — 단수 변형이 있으면 그 문구. `{}` 가 수로 치환된다.
    pub hidden_folders_one: &'a str,
    /// `…` 툴팁 — 숨긴 조상이 둘 이상일 때. `{}` 가 수로 치환된다.
    pub hidden_folders_many: &'a str,
    pub empty_label: &'a str,
    pub loading_label: &'a str,
    pub loading_body_local: &'a str,
    pub loading_body_remote: &'a str,
    pub error_perm_title: &'a str,
    pub error_perm_retry: &'a str,
    pub error_conn_title: &'a str,
    pub error_conn_reconnect: &'a str,
    /// 목록 머리의 열 이름. 대문자 변환은 렌더가 한다(kit `text-transform: uppercase`).
    pub col_name: &'a str,
    pub col_size: &'a str,
    pub col_modified: &'a str,
    /// 호출자가 넘긴 확장자 필터(점 없음, 호출자 순서). 비면 footer 에 필터 칩을 그리지 않는다.
    pub filters: &'a [String],
    /// 필터 칩 툴팁. `{list}` 자리에 줄이지 않은 확장자 목록이 들어간다.
    pub filter_showing: &'a str,
}

/// 선택한 폴더 하나의 이름. 푸터 안내와 확정 버튼이 같은 판정을 쓴다.
pub(super) fn selected_folder<'a>(props: &'a FilePickerProps<'a>) -> Option<&'a str> {
    let [name] = props.selected else {
        return None;
    };
    props
        .entries
        .iter()
        .find(|e| &e.name == name)
        .filter(|e| e.is_dir)
        .map(|e| e.name.as_str())
}

/// View 가 발생시킨 사용자 의도. Wrapper 가 mutation 으로 변환.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilePickerAction {
    ClearSelection,
    None,
    /// ESC 또는 헤더 ✕/[취소] — popup 닫기 + result=Cancelled.
    Cancel,
    /// 목록 행 단일 클릭 — 선택만 갱신, popup 유지.
    Select(String),
    /// 디렉토리 행 더블클릭 — 그 하위로 내비게이트.
    NavigateInto(String),
    /// 브레드크럼 세그먼트(또는 `…` 메뉴의 숨긴 조상) 클릭 — 그 인덱스까지의 경로로 내비게이트.
    NavigateTo(usize),
    /// 상위 폴더로.
    NavigateUp,
    /// path bar 의 refresh 버튼 또는 에러 상태의 Retry/Reconnect 버튼.
    Refresh,
    /// footer primary 버튼 — 열기는 현재 `selected`, 저장은 이름 칸으로 확정.
    Confirm,
    /// 파일 행 더블클릭 — 선택 상태와 무관하게 그 엔트리 하나로 즉시 확정.
    ConfirmEntry(String),
    /// 저장 모드 이름 칸이 편집됐다 — 새 전체 값.
    EditName(String),
}

/// 뷰 한 프레임의 결과.
pub struct FilePickerViewOut {
    pub action: FilePickerAction,
    /// 뷰가 그린 헤더 줄. 셸 타이틀바가 없는 popup 은 이 사각형을 이동 손잡이로 보고한다.
    /// Esc 로 바로 닫히는 프레임에는 헤더를 그리지 않아 `Rect::NOTHING` 이다.
    pub header_rect: egui::Rect,
}

/// 헤더·경로·푸터 영역을 먼저 정하고 목록에는 남은 높이를 준다.
/// 긴 경로는 breadcrumb 안에서 줄여 푸터 버튼 영역을 보존한다.
pub fn draw_file_picker_view(ui: &mut egui::Ui, props: &FilePickerProps<'_>) -> FilePickerViewOut {
    let ctx = ui.ctx().clone();
    let nested_popup_open = ctx.memory(|m| m.any_popup_open());
    if props.owns_escape
        && !nested_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        return FilePickerViewOut {
            action: FilePickerAction::Cancel,
            header_rect: egui::Rect::NOTHING,
        };
    }

    // 콘텐츠 영역은 창 가장자리까지 온다. 구역마다 배경·구분선은 전폭으로 두고
    // 내용은 `fp-*` 인셋 안에 그린다.
    let outer = ui.available_rect_before_wrap();
    let mut out = draw_view_sections(ui, props, outer);
    if !nested_popup_open && let Some(action) = folder::keyboard(ui, props) {
        out.action = action;
    }
    ui.advance_cursor_after_rect(outer);
    out
}

/// 헤더 높이. 고정값이 없고 위아래 `fp-header-pad-y`와 sm IconButton으로 정해진다.
fn header_height(th: &Theme) -> LogicalPx {
    let pad = th.fp_header_pad_y();
    pad + LogicalPx(ControlSize::Sm.height(th)) + pad
}

/// 경로 막대 높이. 위아래 `fp-path-pad-y`와 sm IconButton으로 정해진다.
fn path_bar_height(th: &Theme) -> LogicalPx {
    let pad = th.fp_path_pad_y();
    pad + LogicalPx(ControlSize::Sm.height(th)) + pad
}

/// 구역 사각형에서 좌우 인셋과 위아래 여백을 뺀 내용 사각형.
fn section_inner(
    rect: egui::Rect,
    start: LogicalPx,
    end: LogicalPx,
    pad_y: LogicalPx,
) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + start.value(), rect.top() + pad_y.value()),
        egui::pos2(rect.right() - end.value(), rect.bottom() - pad_y.value()),
    )
}

fn draw_view_sections(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    outer: egui::Rect,
) -> FilePickerViewOut {
    let th = props.theme;
    let mut action = FilePickerAction::None;

    // 헤더 줄 전체가 이동 손잡이다.
    let header_rect = egui::Rect::from_min_size(
        outer.min,
        egui::vec2(outer.width(), header_height(th).value()),
    );
    let mut header = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_header")
            .max_rect(section_inner(
                header_rect,
                th.fp_inset_start(),
                th.fp_inset_end(),
                th.fp_header_pad_y(),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    header.spacing_mut().item_spacing.x = th.fp_section_gap().value();
    header.add(
        (if matches!(props.mode, FilePickerMode::Folder { .. }) {
            icons::FOLDER
        } else {
            icons::FILE
        })
        .image(th.icon_glyph_size_md.value(), th.text_muted().into()),
    );
    header.label(
        egui::RichText::new(props.title_label)
            .size(th.font_size_max.value())
            .strong()
            .color(th.text_primary()),
    );
    if let Some(host) = props.remote_host {
        host_badge(&mut header, th, host);
    }
    header.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(ui, th, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            })
            .clicked()
        {
            action = FilePickerAction::Cancel;
        }
    });

    // 경로 막대는 전폭 bg-sidebar 띠 위에 그린다.
    let band = egui::Rect::from_min_size(
        egui::pos2(outer.left(), header_rect.bottom()),
        egui::vec2(outer.width(), path_bar_height(th).value()),
    );
    ui.painter()
        .rect_filled(band, 0.0, th.bg_sidebar().to_egui());
    hline(ui, th, outer.x_range(), band.top());
    let mut path_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_path")
            .max_rect(section_inner(
                band,
                th.fp_inset_start(),
                th.fp_inset_end(),
                th.fp_path_pad_y(),
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    path_bar::path_bar(&mut path_ui, props, &mut action);
    hline(ui, th, outer.x_range(), band.bottom());

    let rest = egui::Rect::from_min_max(egui::pos2(outer.left(), band.bottom()), outer.max);
    let footer_h =
        footer::footer_height(ui, props).min(LogicalPx(rest.height()).max(LogicalPx(0.0)));
    let footer_rect = egui::Rect::from_min_max(
        egui::pos2(rest.left(), rest.bottom() - footer_h.value()),
        rest.max,
    );
    let body_rect = egui::Rect::from_min_max(rest.min, egui::pos2(rest.right(), footer_rect.top()));

    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_body")
            .max_rect(body_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
    draw_body(
        &mut body_ui,
        props,
        LogicalPx(body_rect.height()),
        &mut action,
    );

    hline(ui, th, outer.x_range(), footer_rect.top());
    let footer_inner = section_inner(
        footer_rect,
        th.fp_inset_start(),
        th.fp_inset_start(),
        th.fp_footer_pad_y(),
    );
    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_footer")
            .max_rect(footer_inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    footer_ui.set_clip_rect(footer_rect.intersect(ui.clip_rect()));
    footer::draw_footer(&mut footer_ui, props, &mut action);

    FilePickerViewOut {
        action,
        header_rect,
    }
}

fn draw_body(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    body_height: LogicalPx,
    action: &mut FilePickerAction,
) {
    let th = props.theme;
    let mut body_rect = ui.available_rect_before_wrap();
    let mut row_hit = false;
    match &props.state {
        FpViewState::Loaded => {
            // 머리와 목록은 붙어 있다.
            ui.spacing_mut().item_spacing.y = 0.0;
            // 목록은 머리를 그리고 남은 높이 안에서 스크롤한다. egui 가 남은 높이로 제한한다.
            list_head(ui, props);
            body_rect.min.y = ui.next_widget_position().y;
            egui::ScrollArea::vertical()
                .id_salt("file_picker_list")
                .max_height(body_height.value())
                .auto_shrink([false, true])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    // 행은 붙어 있고 높이는 `row_height`가 모두 정한다.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for entry in props.entries {
                        let hit = egui::Rect::from_min_size(
                            ui.next_widget_position(),
                            egui::vec2(ui.available_width(), row_height(th).value()),
                        );
                        row_hit |=
                            ui.input(|i| i.pointer.interact_pos().is_some_and(|p| hit.contains(p)));
                        if let Some(a) = rows::entry_row(ui, props, entry)
                            && (matches!(action, FilePickerAction::None)
                                || !matches!(a, FilePickerAction::Select(_)))
                        {
                            *action = a;
                        }
                    }
                });
        }
        FpViewState::Loading => {
            let sub = if props.remote_host.is_some() {
                props.loading_body_remote
            } else {
                props.loading_body_local
            };
            CenterState::loading(props.loading_label)
                .sub_line(Some(sub))
                .show(ui, th, Some(body_height));
        }
        FpViewState::Empty => {
            CenterState::empty(icons::FOLDER_OPEN, props.empty_label).show(
                ui,
                th,
                Some(body_height),
            );
        }
        FpViewState::ErrorPerm(reason) => {
            let out = CenterState::error(props.error_perm_title)
                .sub_line(Some(reason.as_str()))
                .action(props.error_perm_retry, Some(icons::REFRESH))
                .show(ui, th, Some(body_height));
            if out.action_clicked {
                *action = FilePickerAction::Refresh;
            }
        }
        FpViewState::ErrorConn(reason) => {
            let out = CenterState::error(props.error_conn_title)
                .sub_line(Some(reason.as_str()))
                .action(props.error_conn_reconnect, Some(icons::REFRESH))
                .show(ui, th, Some(body_height));
            if out.action_clicked {
                *action = FilePickerAction::Refresh;
            }
        }
    }
    if matches!(props.mode, FilePickerMode::Folder { .. })
        && matches!(action, FilePickerAction::None)
        && !row_hit
        && ui.rect_contains_pointer(body_rect)
        && ui.input(|i| i.pointer.primary_clicked())
    {
        *action = FilePickerAction::ClearSelection;
    }
}

/// 목록 머리 높이. 위아래 `fp-list-head-pad-y`와 micro 글꼴 줄(`line-height-ui`)로 정해진다.
pub(super) fn list_head_height(th: &Theme) -> LogicalPx {
    let pad = th.fp_list_head_pad_y();
    pad + th.font_size_micro.scaled(th.line_height_ui) + pad
}

/// FpRow 열 위치. 목록 머리와 행이 같은 계산을 써서 열 이름이 값 위에 맞는다.
struct Cols {
    icon_left: LogicalPx,
    name_left: LogicalPx,
    name_right: LogicalPx,
    size_right: LogicalPx,
    modified_right: LogicalPx,
}

fn cols(rect: egui::Rect, th: &Theme) -> Cols {
    let icon_left = LogicalPx(rect.left()) + th.fp_inset_start();
    let name_left = icon_left + th.icon_glyph_size_md + th.spacing_sm;
    // 오른쪽의 고정 열부터 배치하고 이름은 남은 폭을 쓴다.
    let modified_right = LogicalPx(rect.right()) - th.fp_inset_start();
    let size_right = modified_right - MOD_COL_W - th.spacing_sm;
    let name_right = size_right - SIZE_COL_W - th.spacing_sm;
    Cols {
        icon_left,
        name_left,
        name_right,
        size_right,
        modified_right,
    }
}

/// 열 이름 행(kit list header). font-label micro 대문자, text-muted, 아래 구분선.
fn list_head(ui: &mut egui::Ui, props: &FilePickerProps<'_>) {
    let th = props.theme;
    let h = list_head_height(th);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), h.value()),
        egui::Sense::hover(),
    );
    let c = cols(rect, th);
    let font = tasty_ui_widgets::label_font(th.font_size_micro.value());
    let muted: egui::Color32 = th.text_muted().into();
    let painter = ui.painter();
    for (x, align, label) in [
        (c.name_left, egui::Align2::LEFT_CENTER, props.col_name),
        (c.size_right, egui::Align2::RIGHT_CENTER, props.col_size),
        (
            c.modified_right,
            egui::Align2::RIGHT_CENTER,
            props.col_modified,
        ),
    ] {
        painter.text(
            egui::pos2(x.value(), rect.center().y),
            align,
            label.to_uppercase(),
            font.clone(),
            muted,
        );
    }
    hline(ui, th, rect.x_range(), rect.bottom());
}

/// 목록 행 높이. 위아래 `fp-row-pad-y`와 아이콘·이름 줄(`line-height-ui`) 중 높은 쪽으로 정해진다.
/// 줄 높이를 글꼴 행 높이로 재지 않아 갤러리와 본체의 글꼴 구성이 달라도 같은 높이가 된다.
pub(super) fn row_height(th: &Theme) -> LogicalPx {
    let pad = th.fp_row_pad_y();
    let name = th.font_size_body.scaled(th.line_height_ui);
    pad + name.max(th.icon_glyph_size_md) + pad
}

/// 원격 호스트를 user@host 형태로 표시하는 배지.
fn host_badge(ui: &mut egui::Ui, th: &Theme, host: &str) {
    let info = th.accent_info();
    let font = egui::FontId::monospace(th.font_size_caption.value());
    let galley = ui
        .painter()
        .layout_no_wrap(host.to_owned(), font, egui::Color32::PLACEHOLDER);
    let glyph = th.icon_glyph_size_xs.value();
    let gap = th.spacing_xs.value();
    let pad_x = th.spacing_sm.value();
    let h = 22.0;
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = th.corner_radius.value();
    let info_color: egui::Color32 = info.into();
    // tinted 채움/테두리 짝 — `tint-fill-alpha` / `tint-border-alpha`.
    ui.painter().rect_filled(
        rect,
        radius,
        info_color.gamma_multiply(th.tint_fill_alpha()),
    );
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            th.border_width.value(),
            info_color.gamma_multiply(th.tint_border_alpha()),
        ),
        egui::StrokeKind::Inside,
    );
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::REMOTE.image(glyph, info_color).paint_at(ui, gy);
    let pos = egui::pos2(
        rect.left() + pad_x + glyph + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, info_color);
}

fn hline(ui: &egui::Ui, th: &Theme, x: egui::Rangef, y: f32) {
    ui.painter().hline(
        x,
        y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// 결과 없이 닫혔으면 Cancelled를 기록해 호출부가 계속 기다리지 않게 한다.
pub fn on_close_file_picker(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if let Some(p) = state.dialogs.file_picker.as_mut()
        && p.result.is_none()
    {
        p.result = Some(crate::state::FilePickerResult::Cancelled);
    }
}

/// 상태에서 화면 입력을 만들고 사용자 동작을 반영한다.
pub fn draw_file_picker(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let Some(data) = state.dialogs.file_picker.as_ref() else {
        return PopupAction::Close;
    };

    // 원격 mirror 워크스페이스가 사라지면 연결 오류로 표시한다.
    // 세션의 disconnected 플래그는 이 호출부에서 직접 읽을 수 없다.
    if let Some(mirror_ws_id) = data.mirror_ws_id
        && engine.find_workspace_index_for_id(mirror_ws_id).is_none()
        && !matches!(data.load, FpLoadState::ErrorConn(_))
    {
        let data = state.dialogs.file_picker.as_mut().unwrap();
        data.load = FpLoadState::ErrorConn(t("filepicker.error_conn.session_lost").to_string());
    }

    if let Some(FpLoadState::Loading { sent_at, .. }) =
        state.dialogs.file_picker.as_ref().map(|d| d.load.clone())
        && state
            .dialogs
            .file_picker
            .as_ref()
            .is_some_and(|data| data.mirror_ws_id.is_some())
        && sent_at.elapsed() > LIST_DIR_SOFT_TIMEOUT
    {
        let data = state.dialogs.file_picker.as_mut().unwrap();
        data.load = FpLoadState::ErrorConn(t("filepicker.error_conn.timeout").to_string());
    }

    let th = theme::theme();
    let owns_escape = state.popup_escape_owner == Some(FILE_PICKER_POPUP_ID);
    let data = state.dialogs.file_picker.as_ref().unwrap();

    let is_remote = data.mirror_ws_id.is_some();
    let crumb_targets = path_ancestors(is_remote, &data.current_dir);
    let crumbs: Vec<CrumbView> = crumb_targets
        .iter()
        .map(|full| CrumbView {
            label: crumb_label(is_remote, full),
        })
        .collect();

    let view_entries: Vec<FilePickerEntryView> = data
        .entries
        .iter()
        .filter(|e| e.is_dir || matches_filters(&data.filters, &e.name))
        .map(|e| FilePickerEntryView {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size_display: crate::core::fs_list::human_size(e.is_dir, e.size),
            modified_display: crate::core::fs_list::format_modified(e.modified),
        })
        .collect();

    let view_state = match &data.load {
        FpLoadState::Loading { .. } => FpViewState::Loading,
        FpLoadState::Loaded => FpViewState::Loaded,
        FpLoadState::Empty => FpViewState::Empty,
        FpLoadState::ErrorPerm(r) => FpViewState::ErrorPerm(r.clone()),
        FpLoadState::ErrorConn(r) => FpViewState::ErrorConn(r.clone()),
    };

    // 폴더를 고르면 이름 칸은 비우고 확정 버튼은 해당 폴더로 이동한다.
    let selection_text = data
        .selected
        .iter()
        .filter(|name| {
            data.entries
                .iter()
                .find(|e| &&e.name == name)
                .is_some_and(|e| !e.is_dir)
        })
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let overwrite = data
        .save_name
        .as_ref()
        .is_some_and(|name| save::overwrites(name, &data.entries, &data.filters));
    let title_label = t(if data.save_name.is_some() {
        "filepicker.save.title"
    } else {
        "filepicker.title"
    });
    let name_field_label = t("filepicker.name_field_label");
    let cancel_label = t("button.cancel");
    let empty_label = t("filepicker.empty");
    let loading_label = t("filepicker.loading");
    let loading_body_local = t("filepicker.loading_body_local");
    let loading_body_remote = t("filepicker.loading_body_remote");
    let error_perm_title = t("filepicker.error_perm.title");
    let error_perm_retry = t("filepicker.error_perm.retry");
    let error_conn_title = t("filepicker.error_conn.title");
    let error_conn_reconnect = t("filepicker.error_conn.reconnect");

    let props = FilePickerProps {
        theme: &th,
        remote_host: data.remote_host.as_deref(),
        crumbs: &crumbs,
        state: view_state,
        entries: &view_entries,
        selected: &data.selected,
        mode: match &data.save_name {
            Some(name) => FilePickerMode::Save {
                name,
                overwrite,
                can_confirm: save::confirmed_path(
                    std::path::Path::new(&data.current_dir),
                    name,
                    matches!(data.load, FpLoadState::Loaded | FpLoadState::Empty),
                    &data.entries,
                )
                .is_some(),
            },
            None => FilePickerMode::Open {
                selection_text: &selection_text,
            },
        },
        owns_escape,
        title_label,
        name_field_label,
        name_placeholder: t(if data.save_name.is_some() {
            "filepicker.save.name_placeholder"
        } else {
            "filepicker.no_file_selected"
        }),
        cancel_label,
        confirm_label: t(if overwrite {
            "filepicker.save.overwrite_button"
        } else if data.save_name.is_some() {
            "filepicker.save.save_button"
        } else {
            "filepicker.open_button"
        }),
        overwrite_warning: t("filepicker.save.overwrite_warning"),
        folder_not_save_target: t("filepicker.folder_not_save_target"),
        folder_open_enters: t("filepicker.folder_open_enters"),
        hidden_folders_one: t_count_template("filepicker.hidden_folders", 1),
        hidden_folders_many: t("filepicker.hidden_folders"),
        empty_label,
        loading_label,
        loading_body_local,
        loading_body_remote,
        error_perm_title,
        error_perm_retry,
        error_conn_title,
        error_conn_reconnect,
        col_name: t("filepicker.column.name"),
        col_size: t("filepicker.column.size"),
        col_modified: t("filepicker.column.modified"),
        filters: &data.filters,
        filter_showing: t("filepicker.filter_showing"),
    };

    let out = draw_file_picker_view(ui, &props);
    // 셸 타이틀바가 없으므로 위젯 헤더 줄을 이동 손잡이로 보고한다.
    super::report_header_drag_rect(ui.ctx(), FILE_PICKER_POPUP_ID, out.header_rect);
    apply_action(state, engine, out.action)
}

fn apply_action(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: FilePickerAction,
) -> PopupAction {
    match action {
        FilePickerAction::None => PopupAction::None,
        FilePickerAction::ClearSelection => {
            if let Some(d) = state.dialogs.file_picker.as_mut() {
                d.selected.clear();
            }
            PopupAction::None
        }
        FilePickerAction::Cancel => {
            if let Some(d) = state.dialogs.file_picker.as_mut() {
                d.result = Some(FilePickerResult::Cancelled);
            }
            PopupAction::Close
        }
        FilePickerAction::EditName(name) => {
            if let Some(d) = state.dialogs.file_picker.as_mut()
                && d.save_name.is_some()
            {
                if d.selected.first() != Some(&name) {
                    d.selected.clear();
                }
                d.save_name = Some(name);
            }
            PopupAction::None
        }
        FilePickerAction::Select(name) => {
            if let Some(d) = state.dialogs.file_picker.as_mut() {
                if d.save_name.is_some()
                    && d.entries.iter().any(|e| {
                        e.name == name && !e.is_dir && matches_filters(&d.filters, &e.name)
                    })
                {
                    d.save_name = Some(name.clone());
                }
                d.selected = vec![name];
            }
            PopupAction::None
        }
        FilePickerAction::NavigateInto(name) => {
            navigate(state, engine, |dir, is_remote| {
                join_dir(is_remote, dir, &name)
            });
            PopupAction::None
        }
        FilePickerAction::NavigateTo(idx) => {
            if let Some(d) = state.dialogs.file_picker.as_ref() {
                let is_remote = d.mirror_ws_id.is_some();
                let targets = path_ancestors(is_remote, &d.current_dir);
                if let Some(target) = targets.get(idx).cloned() {
                    navigate(state, engine, move |_dir, _is_remote| target.clone());
                }
            }
            PopupAction::None
        }
        FilePickerAction::NavigateUp => {
            if let Some(d) = state.dialogs.file_picker.as_ref() {
                let is_remote = d.mirror_ws_id.is_some();
                let targets = path_ancestors(is_remote, &d.current_dir);
                if targets.len() > 1 {
                    let parent = targets[targets.len() - 2].clone();
                    navigate(state, engine, move |_dir, _is_remote| parent.clone());
                }
            }
            PopupAction::None
        }
        FilePickerAction::Refresh => {
            navigate(state, engine, |dir, _is_remote| dir.to_string());
            PopupAction::None
        }
        FilePickerAction::Confirm => {
            if let Some(d) = state.dialogs.file_picker.as_mut()
                && let Some(name) = &d.save_name
            {
                if let Some(path) = save::confirmed_path(
                    std::path::Path::new(&d.current_dir),
                    name,
                    matches!(d.load, FpLoadState::Loaded | FpLoadState::Empty),
                    &d.entries,
                ) {
                    d.result = Some(FilePickerResult::Confirmed { paths: vec![path] });
                    return PopupAction::Close;
                }
                return PopupAction::None;
            }
            let folder = state.dialogs.file_picker.as_ref().and_then(|d| {
                let [name] = d.selected.as_slice() else {
                    return None;
                };
                d.entries
                    .iter()
                    .find(|e| &e.name == name)
                    .filter(|e| e.is_dir)
                    .map(|e| e.name.clone())
            });
            if let Some(name) = folder {
                navigate(state, engine, |dir, is_remote| {
                    join_dir(is_remote, dir, &name)
                });
                return PopupAction::None;
            }
            if let Some(d) = state.dialogs.file_picker.as_mut() {
                let is_remote = d.mirror_ws_id.is_some();
                // 화면의 활성화 검사와 별개로 디렉터리를 파일로 확정하지 않게 재확인한다.
                let all_files = d.selected.iter().all(|name| {
                    d.entries
                        .iter()
                        .find(|e| &e.name == name)
                        .is_some_and(|e| !e.is_dir)
                });
                let paths: Vec<String> = d
                    .selected
                    .iter()
                    .map(|name| join_dir(is_remote, &d.current_dir, name))
                    .collect();
                if all_files && !paths.is_empty() {
                    d.result = Some(FilePickerResult::Confirmed { paths });
                    return PopupAction::Close;
                }
            }
            PopupAction::None
        }
        FilePickerAction::ConfirmEntry(name) => {
            if state
                .dialogs
                .file_picker
                .as_ref()
                .is_some_and(|d| d.save_name.is_some())
            {
                return apply_action(state, engine, FilePickerAction::Select(name));
            }
            if let Some(d) = state.dialogs.file_picker.as_mut() {
                let is_remote = d.mirror_ws_id.is_some();
                let path = join_dir(is_remote, &d.current_dir, &name);
                d.result = Some(FilePickerResult::Confirmed { paths: vec![path] });
                return PopupAction::Close;
            }
            PopupAction::None
        }
    }
}

/// 원격은 로컬에서 검사하지 않고 주어진 경로를 사용한다. 없으면 서버가 홈으로 해석할 빈 문자열이다.
/// 로컬은 실제 절대 디렉터리만 채택하며 아니면 홈을 사용한다.
fn initial_dir(is_remote: bool, requested: Option<String>) -> String {
    if is_remote {
        return requested.unwrap_or_default();
    }
    requested
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
        .unwrap_or_else(|| {
            directories::BaseDirs::new()
                .map(|d| d.home_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"))
        })
        .to_string_lossy()
        .to_string()
}

/// 피커를 연 쪽. popup 열기 intent의 출처를 정하며 플러그인이면 결과를 받을 requester를 담는다.
#[derive(Debug, Clone)]
pub(crate) enum FilePickerOpenedBy {
    /// Tools 메뉴와 그 단축키.
    ToolsMenu,
    /// 탭 스트립 같은 컨텍스트 메뉴.
    ContextMenu,
    /// `file_picker.trigger`를 보낸 플러그인.
    Plugin(crate::state::FilePickerRequester),
    /// 플러그인 페이지에서 사용자가 누른 navigation을 host가 확인한 `file_picker.trigger`.
    PluginForUser(crate::state::FilePickerRequester),
}

/// 기존 요청은 결과를 호출자가 가져갈 때까지 새 요청으로 대체하지 않는다.
#[derive(Debug)]
pub(crate) struct FilePickerBusy;

/// 열린 피커뿐 아니라 닫혔지만 아직 전달되지 않은 확정·취소 결과도 보호한다.
pub(crate) fn ensure_available(state: &MainViewState) -> Result<(), FilePickerBusy> {
    if state.dialogs.file_picker.is_some() {
        Err(FilePickerBusy)
    } else {
        Ok(())
    }
}

/// 호출자가 정한 위치를 둘러보는 피커를 연다. 피커는 경로를 고르는 데서 끝나며,
/// 결과는 `request_id`와 함께 `FilePickerData::result`에 남아 App이 호출자에게 넘긴다.
/// 로컬은 navigate에서 동기로 읽기 시작하고 원격은 요청을 큐에 넣는다.
/// filters가 비면 확장자를 제한하지 않는다. Busy면 기존 상태와 pending intent를 그대로 둔다.
pub(crate) fn open(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    request_id: crate::state::FilePickerRequestId,
    location: crate::state::FilePickerLocation,
    filters: Vec<String>,
    opened_by: FilePickerOpenedBy,
) -> Result<(), FilePickerBusy> {
    ensure_available(state)?;
    let (mirror_ws_id, dir) = match location {
        crate::state::FilePickerLocation::Local { dir } => (None, dir),
        crate::state::FilePickerLocation::Remote {
            mirror_ws_id, dir, ..
        } => (Some(mirror_ws_id), dir),
    };
    let initial_dir = initial_dir(mirror_ws_id.is_some(), dir);
    let requester = match &opened_by {
        FilePickerOpenedBy::Plugin(req) | FilePickerOpenedBy::PluginForUser(req) => {
            Some(req.clone())
        }
        FilePickerOpenedBy::ToolsMenu | FilePickerOpenedBy::ContextMenu => None,
    };

    state.dialogs.file_picker = Some(crate::state::FilePickerData {
        save_name: None,
        request_id,
        mirror_ws_id,
        remote_host: None,
        current_dir: initial_dir,
        load: FpLoadState::Empty,
        local_query: None,
        entries: Vec::new(),
        selected: Vec::new(),
        result: None,
        requester,
        filters,
    });

    navigate(state, engine, |dir, _is_remote| dir.to_string());

    let open_popup = crate::intent::UiIntent::OpenPopup {
        id: FILE_PICKER_POPUP_ID,
        mode: crate::intent::OpenPopupMode::CenteredFocused,
    };
    let dispatched = match opened_by {
        FilePickerOpenedBy::Plugin(req) => open_popup.from_agent_plugin(req.plugin_id),
        FilePickerOpenedBy::PluginForUser(_) => open_popup.from_user_menu("plugin_page"),
        FilePickerOpenedBy::ContextMenu => open_popup.from_user_context_menu(),
        FilePickerOpenedBy::ToolsMenu => open_popup.from_user_menu("tools_menu"),
    };
    state.dispatch_intent(dispatched);
    Ok(())
}

/// 새 대상 디렉토리로 내비게이트: 선택 초기화 + 로컬은 즉시 동기 로드, 원격은
/// `pending_list_dir_forward` 큐잉 + `Loading` 전이.
fn navigate(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target_of: impl FnOnce(&str, bool) -> String,
) {
    let Some(d) = state.dialogs.file_picker.as_mut() else {
        return;
    };
    let is_remote = d.mirror_ws_id.is_some();
    let target = target_of(&d.current_dir, is_remote);
    d.current_dir = target.clone();
    d.selected.clear();

    if let Some(mirror_ws_id) = d.mirror_ws_id {
        d.local_query = None;
        let request_id = crate::core::next_list_dir_request_id();
        d.load = FpLoadState::Loading {
            request_id,
            sent_at: Instant::now(),
        };
        if let Some(projection) = engine.mirror_projection_token(mirror_ws_id) {
            state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ListDirectory {
                        request: crate::core::PendingListDirForward {
                            local_ws_id: mirror_ws_id,
                            request_id,
                            dir: target,
                            consumer: None,
                        },
                        projection,
                        target: None,
                    },
                )
                .from_user_menu("file_picker.list_directory"),
            );
        }
        return;
    }

    d.local_query = Some(crate::app::local_reads::directory(
        std::path::PathBuf::from(target),
    ));
    d.entries.clear();
    d.load = FpLoadState::Loading {
        request_id: crate::core::next_list_dir_request_id(),
        sent_at: Instant::now(),
    };
}

#[cfg(test)]
mod path_helper_tests {
    use super::{crumb_label, join_dir, matches_filters, path_ancestors};

    #[test]
    fn matches_filters_empty_always_passes() {
        assert!(matches_filters(&[], "notes.txt"));
        assert!(matches_filters(&[], "README.md"));
    }

    #[test]
    fn matches_filters_matches_case_insensitive_extension() {
        let filters = vec!["md".to_string(), "markdown".to_string()];
        assert!(matches_filters(&filters, "notes.md"));
        assert!(matches_filters(&filters, "NOTES.MD"));
        assert!(matches_filters(&filters, "readme.markdown"));
    }

    #[test]
    fn matches_filters_rejects_non_matching_extension() {
        let filters = vec!["md".to_string()];
        assert!(!matches_filters(&filters, "notes.txt"));
        assert!(!matches_filters(&filters, "no_extension"));
    }

    #[test]
    fn join_dir_remote_posix() {
        assert_eq!(
            join_dir(true, "/home/alice", "notes.txt"),
            "/home/alice/notes.txt"
        );
        assert_eq!(join_dir(true, "/", "etc"), "/etc");
    }

    #[test]
    fn join_dir_remote_windows() {
        assert_eq!(
            join_dir(true, "C:\\Users\\alice", "notes.txt"),
            "C:\\Users\\alice\\notes.txt"
        );
        assert_eq!(join_dir(true, "C:\\", "Users"), "C:\\Users");
    }

    #[test]
    fn join_dir_local_uses_platform_path() {
        let joined = join_dir(false, "/tmp/dir", "file.txt");
        assert!(joined.ends_with("file.txt"));
    }

    #[test]
    fn path_ancestors_remote_posix() {
        assert_eq!(
            path_ancestors(true, "/home/alice/proj"),
            vec!["/", "/home", "/home/alice", "/home/alice/proj"]
        );
    }

    #[test]
    fn path_ancestors_remote_windows() {
        assert_eq!(
            path_ancestors(true, "C:\\Users\\alice"),
            vec!["C:\\", "C:\\Users", "C:\\Users\\alice"]
        );
    }

    #[test]
    fn crumb_label_remote_windows_root_and_segment() {
        assert_eq!(crumb_label(true, "C:\\"), "C:\\");
        assert_eq!(crumb_label(true, "C:\\Users\\alice"), "alice");
    }

    #[test]
    fn crumb_label_remote_posix_root_and_segment() {
        assert_eq!(crumb_label(true, "/"), "/");
        assert_eq!(crumb_label(true, "/home/alice"), "alice");
    }
}
