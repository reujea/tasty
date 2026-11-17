//! 탐색기 Properties 팝업. 시안 `YProps`(갤러리 "Properties")를 따른다.
//! 컨텍스트 메뉴의 Properties 나 `explorer_properties` 단축키로 열고, 그 탐색기 칸 범위에 뜬다.
//! 연 순간의 항목을 끝까지 보이며 이후 선택 변경을 따라가지 않는다. 폴더 크기는 read worker 가
//! 배경에서 세는 동안 Spinner 와 지금까지 센 값을 보이고, 팝업을 닫으면 세기를 멈춘다.
//! 원격 항목은 목록이 준 값(종류·크기·수정 시각·위치)만 보이고 그렇다고 알린다.
//! 한 항목이면 아래 Open 이 목록의 Enter 와 같은 동작을 하고 팝업은 남는다. 시각은 설정의 형식을 쓴다.
//! 로컬 한 항목의 권한은 Permissions 행의 Edit 로 바꾼다([`perm_edit`]).

mod actions;
mod perm_edit;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Spinner,
};

use crate::adapters::ui::icons::{self, Icon};
use crate::adapters::ui::popup::PopupAction;
use crate::adapters::ui::surface::explorer::display_text::nfc;
use crate::adapters::ui::surface::explorer::props_time;
use crate::adapters::ui::surface::explorer::view::ExplorerView;
use crate::app::local_reads::{
    self, FolderCount, ItemFacts, ItemKind, LinkTarget, PropertiesFacts, Query,
};
use crate::core::fs_list::{DirEntryInfo, group_digits, human_size};
use crate::i18n::{t, t_count, t_fmt};
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme::{self, Theme};
use actions::{Footer, apply_perm, footer, footer_row};
use perm_edit::{PermAccess, PermClick, PermEdit};

pub const EXPLORER_PROPERTIES_POPUP_ID: &str = "explorer_properties";

/// 팝업이 연 항목과 읽은 정보.
pub struct ExplorerProperties {
    surface_id: u32,
    paths: Vec<PathBuf>,
    /// 원격이면 목록이 준 항목. 로컬이면 `None` 이고 read worker 가 읽는다.
    remote: Option<Vec<DirEntryInfo>>,
    query: Option<Query<PropertiesFacts>>,
    facts: Option<Result<PropertiesFacts, String>>,
    count: Arc<FolderCount>,
    cancel: Arc<AtomicBool>,
    /// 직전 프레임에 잰 내용 높이. sizer 가 팝업 높이로 쓴다.
    content_h: Option<f32>,
    /// 열린 권한 편집 상자.
    perm: Option<PermEdit>,
}

/// 팝업을 연다. `remote` 는 원격 탐색기의 목록 항목이다. 이미 열려 있으면 새 항목으로 바꾼다.
/// 다른 surface 에 열려 있으면 그 팝업을 닫고 요청한 surface 범위로 다시 연다. 같은 id 의
/// OpenPopup 은 범위를 바꾸지 않으므로(공통 dedup) 내용과 범위가 다른 surface 를 가리키게 된다.
pub(crate) fn open(
    state: &mut MainViewState,
    surface_id: u32,
    paths: Vec<PathBuf>,
    remote: Option<Vec<DirEntryInfo>>,
    from_shortcut: bool,
) {
    if paths.is_empty() {
        return;
    }
    let moved = state
        .dialogs
        .explorer_properties
        .as_ref()
        .is_some_and(|old| old.surface_id != surface_id);
    if let Some(old) = state.dialogs.explorer_properties.take() {
        old.cancel.store(true, Ordering::Relaxed);
    }
    let count = Arc::new(FolderCount::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let query = remote
        .is_none()
        .then(|| local_reads::properties(paths.clone(), count.clone(), cancel.clone()));
    state.dialogs.explorer_properties = Some(ExplorerProperties {
        surface_id,
        paths,
        remote,
        query,
        facts: None,
        count,
        cancel,
        content_h: None,
        perm: None,
    });
    let dispatch = |state: &mut MainViewState, intent: crate::intent::UiIntent| {
        state.dispatch_intent(if from_shortcut {
            intent.from_user_shortcut("explorer_properties")
        } else {
            intent.from_user_context_menu()
        });
    };
    if moved {
        dispatch(
            state,
            crate::intent::UiIntent::ClosePopup {
                id: EXPLORER_PROPERTIES_POPUP_ID,
            },
        );
    }
    dispatch(
        state,
        crate::intent::UiIntent::OpenPopup {
            id: EXPLORER_PROPERTIES_POPUP_ID,
            mode: crate::intent::OpenPopupMode::CenteredFocused(
                crate::model::popup_kind::PopupScope::Surface(surface_id),
            ),
        },
    );
}

/// 컨텍스트 메뉴의 Properties. 대상이 없으면(빈 영역) 현재 폴더를 연다. 원격이면 목록 항목을 넘긴다.
pub(crate) fn open_from_menu(
    state: &mut MainViewState,
    remote: bool,
    surface_id: u32,
    paths: &[PathBuf],
    cwd: &Path,
) {
    let paths = if paths.is_empty() {
        vec![cwd.to_path_buf()]
    } else {
        paths.to_vec()
    };
    let listed = remote.then(|| remote_entries(state, surface_id, &paths));
    open(state, surface_id, paths, listed, false);
}

/// `explorer_properties` 단축키. 선택이 있으면 목록 순서대로 그 항목들을, 없으면 현재 폴더를 연다.
pub(crate) fn open_for_shortcut(
    state: &mut MainViewState,
    remote: bool,
    surface_id: u32,
    cwd: &Path,
) {
    let paths = shortcut_paths(state.explorer_views.get(surface_id), cwd);
    let listed = remote.then(|| remote_entries(state, surface_id, &paths));
    open(state, surface_id, paths, listed, true);
}

/// 단축키가 여는 경로. 선택이 있으면 보이는 목록 순서대로 그 항목들(하위 폴더 검색 결과 포함),
/// 없으면 현재 폴더.
pub(crate) fn shortcut_paths(view: Option<&ExplorerView>, cwd: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = view
        .map(|v| v.selected_shown().map(|e| e.path.clone()).collect())
        .unwrap_or_default();
    if paths.is_empty() {
        paths.push(cwd.to_path_buf());
    }
    paths
}

/// 원격 목록에서 경로들의 항목을 찾는다. 목록에 없는 경로(현재 폴더 자신)는 이름만 있는 폴더로 둔다.
fn remote_entries(state: &MainViewState, surface_id: u32, paths: &[PathBuf]) -> Vec<DirEntryInfo> {
    let view = state.explorer_views.get(surface_id);
    paths
        .iter()
        .map(|p| {
            view.and_then(|v| v.listed_entry(p))
                .cloned()
                .unwrap_or_else(|| DirEntryInfo {
                    path: p.clone(),
                    name: file_name(p),
                    is_dir: true,
                    size: 0,
                    modified: None,
                    ext: String::new(),
                    link: Default::default(),
                    os_hidden: false,
                    change: None,
                })
        })
        .collect()
}

/// 정보 읽기 결과를 받는다. 받았으면 다시 그리도록 true 를 돌려준다.
pub(crate) fn poll(
    state: &mut MainViewState,
    owner: &mut crate::app::local_reads::ReadRequests,
) -> bool {
    let Some(props) = state.dialogs.explorer_properties.as_mut() else {
        return false;
    };
    let Some(result) = props.query.as_mut().and_then(|q| q.poll(owner)) else {
        return false;
    };
    props.query = None;
    props.facts = Some(result.map_err(|e| e.to_string()));
    // 권한을 적용한 뒤 다시 읽은 값으로 편집 상자를 맞춘다. 항목을 못 읽게 됐으면 상자를 닫는다.
    match &props.facts {
        Some(Ok(facts)) => {
            if let (Some(edit), [item]) = (props.perm.as_mut(), facts.items.as_slice()) {
                edit.reread(item);
            }
        }
        _ => props.perm = None,
    }
    true
}

/// 첫 프레임에서 내용 높이를 재기 전의 크기. 폭은 토큰, 높이는 필드 여덟 줄 몫이다.
pub fn default_size() -> egui::Vec2 {
    let th = theme::theme();
    egui::vec2(
        th.explorer_props_width().value(),
        th.explorer_props_row_min_height().value() * 8.0,
    )
}

pub fn sizer(state: &MainViewState, _engine: &EngineRead<'_>) -> egui::Vec2 {
    let th = theme::theme();
    let h = state
        .dialogs
        .explorer_properties
        .as_ref()
        .and_then(|p| p.content_h)
        .unwrap_or(default_size().y);
    egui::vec2(th.explorer_props_width().value(), h)
}

pub fn on_close(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    // 다른 surface 로 옮기려고 닫은 뒤 같은 차례에 다시 열었다. 내용은 이미 새 대상이다.
    if state.popups.is_open(EXPLORER_PROPERTIES_POPUP_ID) {
        return;
    }
    if let Some(props) = state.dialogs.explorer_properties.take() {
        props.cancel.store(true, Ordering::Relaxed);
    }
}

/// 한 줄의 내용.
struct Field {
    label: &'static str,
    value: String,
    mono: bool,
    copy: bool,
    counting: bool,
    warning: Option<&'static str>,
    /// 시각 값. 날짜·시각·오프셋 사이에서만 줄을 바꾼다.
    time: bool,
    /// 권한 행. 바꿀 수 있으면 Edit, 소유자가 아니면 안내 줄을 단다.
    perm: PermAccess,
}

fn field(label: &'static str, value: String) -> Field {
    Field {
        label,
        value,
        mono: false,
        copy: false,
        counting: false,
        warning: None,
        time: false,
        perm: PermAccess::None,
    }
}

fn mono(label: &'static str, value: String, copy: bool) -> Field {
    Field {
        mono: true,
        copy,
        ..field(label, value)
    }
}

fn time_field(label: &'static str, t: Option<SystemTime>, format: &str) -> Field {
    Field {
        time: true,
        ..mono(label, props_time::time_text(t, format), false)
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let th = theme::theme();
    let Some(props) = state.dialogs.explorer_properties.as_ref() else {
        return PopupAction::Close;
    };
    // 탐색기 칸이 닫히면 팝업도 닫는다.
    let Some(view) = state.explorer_views.get(props.surface_id) else {
        return PopupAction::Close;
    };
    let format = engine.settings.general.explorer_props_time_format.clone();
    let (mut glyph, name, fields, note) = content(props, &format);
    let failure = match &props.facts {
        Some(Err(reason)) => {
            glyph = item_glyph(state, props.surface_id, &props.paths[0]);
            Some(reason.clone())
        }
        _ => None,
    };
    let footer = footer(props, view);
    let surface_id = props.surface_id;
    let mut perm = props.perm.clone();
    let mut retry = false;
    let mut edit = false;
    let mut perm_click = PermClick::None;
    let mut open = false;
    let pad_x = th.explorer_props_padding_x().value();
    let mut close = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let top = ui.cursor().top();
    let mut close_rect = egui::Rect::NOTHING;
    ui.vertical(|ui| {
        // 폭은 sizer 의 `explorer_props_width` 이고, 칸이 그보다 좁으면 팝업 관리자가 줄인 폭을 그대로 쓴다.
        ui.set_width(ui.available_width().min(th.explorer_props_width().value()));
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: pad_x as i8,
                right: pad_x as i8,
                top: th.spacing_md.value() as i8,
                bottom: th.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                let broken = fields
                    .iter()
                    .any(|f| f.warning == Some("explorer.props.target_missing"));
                close_rect = header(ui, &th, glyph, &name, broken);
            });
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: pad_x as i8,
                right: pad_x as i8,
                top: 0,
                bottom: th.spacing_md.value() as i8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(reason) = &failure {
                    retry = unreadable_body(ui, &th, reason);
                }
                for f in &fields {
                    let row = field_row(ui, &th, f, perm.as_mut());
                    edit |= row.edit;
                    if row.perm != PermClick::None {
                        perm_click = row.perm;
                    }
                }
                if let Some(text) = note {
                    ui.add_space(th.spacing_sm.value());
                    ui.label(
                        egui::RichText::new(text)
                            .size(th.font_size_caption.value())
                            .color(th.text_muted().to_egui()),
                    );
                }
            });
        if !matches!(footer, Footer::None) {
            open = footer_row(ui, &th, &footer);
        }
    });
    // 닫기는 Tab 순서가 필드 버튼·Open 뒤에 오도록 마지막에 만든다. 자리는 머리가 잡아 두었다.
    if close_rect.is_positive() {
        let mut slot = ui.new_child(egui::UiBuilder::new().max_rect(close_rect));
        close = icon_button(&mut slot, &th, icons::CLOSE)
            .on_hover_text(t("explorer.properties.close"))
            .clicked();
    }
    let measured = ui.cursor().top() - top;
    let apply = perm
        .as_ref()
        .filter(|_| perm_click == PermClick::Apply)
        .map(|p| (p.path.clone(), p.draft));
    if let Some(props) = state.dialogs.explorer_properties.as_mut() {
        // 높이는 다음 프레임에 sizer 가 쓴다. 비동기 결과로 내용이 줄거나 늘면 입력이 없어도 그 프레임을
        // 그려야 팝업이 내용에 맞는다.
        if props.content_h != Some(measured) {
            ui.ctx().request_repaint();
        }
        props.content_h = Some(measured);
        // 그리는 동안 바꾼 칸을 돌려 놓는다. 그 사이 끝난 결과는 그대로 둔다.
        if let (Some(old), Some(new)) = (props.perm.as_mut(), perm)
            && old.phase == new.phase
        {
            old.draft = new.draft;
        }
        if edit {
            props.open_perm();
        }
        if perm_click == PermClick::Cancel {
            props.perm = None;
        }
        if retry {
            props.retry();
        }
    }
    if let Some((path, change)) = apply {
        apply_perm(state, engine, surface_id, path, change);
    }
    if open && let Footer::Open(action) = footer {
        crate::adapters::ui::apply_explorer_action(state, engine, surface_id, action);
    }
    if close {
        PopupAction::Close
    } else {
        PopupAction::None
    }
}

impl ExplorerProperties {
    /// 읽기에 실패한 로컬 항목을 다시 읽는다. 원격 항목은 목록 값을 쓰므로 실패하지 않는다.
    fn retry(&mut self) {
        if self.remote.is_some() {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.count = Arc::new(FolderCount::default());
        self.facts = None;
        self.query = Some(local_reads::properties(
            self.paths.clone(),
            self.count.clone(),
            self.cancel.clone(),
        ));
    }
}

/// 읽지 못한 항목의 제목 글리프. 탐색기 목록에 있으면 그 종류를, 없으면 지금 폴더인지로 정한다.
fn item_glyph(state: &MainViewState, surface_id: u32, path: &Path) -> Icon {
    let view = state.explorer_views.get(surface_id);
    let is_dir = view.and_then(|v| v.entries.iter().find(|e| e.path == path).map(|e| e.is_dir));
    let is_dir = is_dir.unwrap_or_else(|| view.and_then(|v| v.shown_dir()) == Some(path));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if is_dir {
        icons::FOLDER
    } else if crate::adapters::ui::surface::explorer::is_image_ext(&ext) {
        icons::IMAGE
    } else {
        icons::FILE
    }
}

/// 읽기 실패 본문: alertTriangle 과 "Can't read properties"(explorer-error-fg), OS 이유(mono muted), Retry.
/// Retry 를 누르면 true.
fn unreadable_body(ui: &mut egui::Ui, th: &Theme, reason: &str) -> bool {
    let error = th.explorer_error_fg().to_egui();
    let gap = th.spacing_xs.value();
    ui.add_space(gap);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let g = th.icon_glyph_size_md.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(g, g), egui::Sense::hover());
        icons::ALERT_TRIANGLE.image(g, error).paint_at(ui, r);
        ui.label(
            egui::RichText::new(t("explorer.properties.unreadable"))
                .size(th.font_size_body.value())
                .color(error),
        );
    });
    ui.add_space(gap);
    ui.add(
        egui::Label::new(
            egui::RichText::new(reason)
                .monospace()
                .size(th.font_size_caption.value())
                .color(th.text_muted().to_egui()),
        )
        .wrap(),
    );
    ui.add_space(gap * 2.0);
    Button::new(t("explorer.properties.retry"))
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Sm)
        .show(ui, th)
        .clicked()
}

/// 머리: 글리프 · 이름(14, regular, 말줄임) · 닫기 자리. 닫기 버튼은 Tab 순서 때문에 `draw` 가 마지막에
/// 그 자리에 만든다. 제목은 regular 다. 테마에 semibold UI 글꼴이 없어 크기로 제목을 구분한다.
fn header(ui: &mut egui::Ui, th: &Theme, glyph: Icon, name: &str, broken: bool) -> egui::Rect {
    let mut close = egui::Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let g = th.icon_glyph_size_md.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(g, g), egui::Sense::hover());
        let color = if broken {
            th.explorer_link_broken_fg()
        } else {
            th.text_muted()
        };
        glyph.image(g, color.to_egui()).paint_at(ui, r);
        let close_w = ControlSize::Sm.height(th);
        let name_w = (ui.available_width() - close_w - th.spacing_sm.value()).max(0.0);
        let (name_rect, _) = ui.allocate_exact_size(egui::vec2(name_w, g), egui::Sense::hover());
        let mut job = egui::text::LayoutJob::simple_singleline(
            nfc(name).into_owned(),
            egui::FontId::proportional(th.font_size_max.value()),
            th.text_primary().to_egui(),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(name_w);
        let galley = ui.fonts(|f| f.layout_job(job));
        let y = name_rect.center().y - galley.rect.height() / 2.0;
        ui.painter().galley(
            egui::pos2(name_rect.left(), y),
            galley,
            th.text_primary().to_egui(),
        );
        close = ui
            .allocate_exact_size(egui::vec2(close_w, close_w), egui::Sense::hover())
            .0;
    });
    close
}

fn icon_button(ui: &mut egui::Ui, th: &Theme, g: Icon) -> egui::Response {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(ui, th, &|ui, rect, c| {
            g.image(rect.height(), c).paint_at(ui, rect)
        })
}

/// 행에서 누른 것.
struct RowClicks {
    edit: bool,
    perm: PermClick,
}

/// 라벨(96, caption muted) · 값(body 또는 mono caption, 어디서나 줄바꿈. 시각은 부분 사이에서만) ·
/// 선택 Copy · 권한 행의 Edit(상자가 열려 있으면 숨김) 또는 소유자 아님 안내. 열린 권한 편집 상자는
/// 행 아래에 내용 폭으로 그린다.
fn field_row(ui: &mut egui::Ui, th: &Theme, f: &Field, perm: Option<&mut PermEdit>) -> RowClicks {
    let mut clicks = RowClicks {
        edit: false,
        perm: PermClick::None,
    };
    let perm_open = perm.is_some();
    let label_w = th.explorer_props_label_width().value();
    let line_h = th.explorer_props_row_line().value();
    let min_h = th.explorer_props_row_min_height().value();
    let top = th.explorer_props_row_pad_top().value();
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, min_h),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            ui.set_min_height(min_h);
            ui.vertical(|ui| {
                ui.add_space(top);
                ui.set_width(label_w);
                ui.allocate_ui_with_layout(
                    egui::vec2(label_w, line_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(t(f.label))
                                    .size(th.font_size_caption.value())
                                    .color(th.text_muted().to_egui()),
                            )
                            .truncate(),
                        )
                    },
                );
            });
            ui.vertical(|ui| {
                ui.add_space(top);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                    if f.counting {
                        let s = th.icon_glyph_size_sm.value();
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(s, line_h), egui::Sense::hover());
                        let mut slot =
                            ui.new_child(egui::UiBuilder::new().max_rect(
                                egui::Rect::from_center_size(r.center(), egui::vec2(s, s)),
                            ));
                        Spinner::new().size(s).show(&mut slot, th);
                    }
                    let edit_label = t("explorer.props.perm.edit");
                    let show_edit = f.perm == PermAccess::Editable && !perm_open;
                    let copy_w = if f.copy {
                        LogicalPx(ControlSize::Sm.height(th)) + th.spacing_xs
                    } else if show_edit {
                        sm_button_width(ui, th, edit_label) + th.spacing_xs
                    } else {
                        LogicalPx(0.0)
                    };
                    let font = if f.mono {
                        egui::FontId::monospace(th.font_size_caption.value())
                    } else {
                        egui::FontId::proportional(th.font_size_body.value())
                    };
                    let color = th.explorer_props_value_fg().to_egui();
                    let text_w = (ui.available_width() - copy_w.value()).max(0.0);
                    let mut job = egui::text::LayoutJob::simple(
                        nfc(&f.value).into_owned(),
                        font,
                        color,
                        text_w,
                    );
                    // 시각은 날짜·시각·오프셋을 한 덩어리씩 두고 그 사이에서만 줄을 바꾼다.
                    job.wrap.break_anywhere = !f.time;
                    let galley = ui.fonts(|fonts| fonts.layout_job(job));
                    // 첫 줄을 줄 높이 가운데에 두고, 줄을 바꾼 값은 글자 줄 높이만큼만 늘린다.
                    let first_h = galley.rows.first().map_or(line_h, |r| r.height());
                    let text_h = (galley.rect.height() + line_h - first_h).max(line_h);
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(text_w, text_h), egui::Sense::hover());
                    let y = r.top() + (line_h - first_h) / 2.0;
                    ui.painter().galley(egui::pos2(r.left(), y), galley, color);
                    if f.copy
                        && icon_button(ui, th, icons::COPY)
                            .on_hover_text(t("explorer.properties.copy"))
                            .clicked()
                    {
                        ui.ctx().copy_text(f.value.clone());
                    }
                    if show_edit
                        && Button::new(edit_label)
                            .variant(ButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .show(ui, th)
                            .clicked()
                    {
                        clicks.edit = true;
                    }
                });
                if f.perm == PermAccess::Locked {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(t("explorer.props.perm.locked"))
                                .size(th.font_size_caption.value())
                                .color(th.explorer_props_perm_locked_fg().to_egui()),
                        )
                        .wrap(),
                    );
                }
                if let Some(key) = f.warning {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                        let color = if key == "explorer.props.target_missing" {
                            th.explorer_link_broken_fg()
                        } else {
                            th.explorer_props_partial_fg()
                        }
                        .to_egui();
                        let size = th.icon_glyph_size_xs.value();
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                        icons::ALERT_TRIANGLE.image(size, color).paint_at(ui, rect);
                        ui.label(
                            egui::RichText::new(t(key))
                                .size(th.font_size_caption.value())
                                .color(color),
                        );
                    });
                }
            });
        },
    );
    // 시안은 상자를 값 열 아래(`under`)에 두지만 3×3 격자(칸 4 × 56 + 안쪽 여백 + 테두리)가 360 폭의
    // 값 열보다 넓어 넘친다. 시안 설명의 "full content width" 를 따라 내용 폭 전체에 둔다.
    if f.perm == PermAccess::Editable
        && let Some(edit) = perm
    {
        clicks.perm = perm_edit::draw(ui, th, edit);
    }
    clicks
}

/// sm 버튼 하나의 폭. 값 글자가 버튼 자리를 침범하지 않도록 먼저 잰다.
fn sm_button_width(ui: &egui::Ui, th: &Theme, label: &str) -> LogicalPx {
    let size = ControlSize::Sm;
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(size.font_size(th)),
        egui::Color32::PLACEHOLDER,
    );
    LogicalPx(galley.rect.width()) + LogicalPx(size.pad_x(th)) * 2.0
}

/// (글리프, 제목, 필드, 안내 문구).
fn content(
    props: &ExplorerProperties,
    format: &str,
) -> (Icon, String, Vec<Field>, Option<&'static str>) {
    let location = common_parent(&props.paths);
    if let Some(entries) = &props.remote {
        return remote_content(entries, location, format);
    }
    let counting = !props.count.done.load(Ordering::Acquire);
    let partial = props.count.partial.load(Ordering::Acquire);
    let counted_items = props.count.items.load(Ordering::Relaxed);
    let counted_bytes = props.count.bytes.load(Ordering::Relaxed);
    let facts = match &props.facts {
        None => {
            let title = file_name(&props.paths[0]);
            let fields = vec![Field {
                counting: true,
                ..field("explorer.properties.kind", String::new())
            }];
            return (icons::FILE, title, fields, None);
        }
        // 글리프와 본문은 `draw` 가 탐색기 목록을 보고 채운다.
        Some(Err(_)) => {
            let title = file_name(&props.paths[0]);
            return (icons::FILE, title, Vec::new(), None);
        }
        Some(Ok(facts)) => facts,
    };
    if let ([_], [item]) = (props.paths.as_slice(), facts.items.as_slice()) {
        return single_content(
            &props.paths[0],
            item,
            (partial, counting, counted_items, counted_bytes),
            format,
        );
    }
    let folders = facts
        .items
        .iter()
        .filter(|f| matches!(f.kind, ItemKind::Folder))
        .count();
    let files = facts.items.len() - folders;
    let total = human_size(false, facts.file_bytes + counted_bytes);
    let fields = vec![
        // 고른 항목 중 metadata 를 못 읽은 것이 있으면 읽은 것만 세고 여기서 알린다.
        Field {
            warning: (!facts.failed.is_empty()).then_some("explorer.props.unreadable_items"),
            ..field("explorer.properties.kinds", kinds_text(files, folders))
        },
        Field {
            counting: counting && folders > 0,
            warning: partial.then_some("explorer.props.partial"),
            ..field("explorer.properties.total_size", total)
        },
        mono("explorer.properties.location", location, true),
    ];
    let n = props.paths.len();
    let title = t_count("explorer.properties.items", n as u64, &[&n.to_string()]);
    (icons::LAYERS, title, fields, None)
}

/// `count` 는 (부분 집계, 세는 중, 센 항목 수, 센 바이트)다.
fn single_content(
    path: &Path,
    item: &ItemFacts,
    count: (bool, bool, u64, u64),
    format: &str,
) -> (Icon, String, Vec<Field>, Option<&'static str>) {
    let (partial, counting, counted_items, counted_bytes) = count;
    let name = file_name(path);
    let path_row = || mono("explorer.props.path", item.path.display().to_string(), true);
    if let ItemKind::Link { target, state } = &item.kind {
        let missing = matches!(state, LinkTarget::Missing);
        let mut fields = vec![
            field(
                "explorer.properties.kind",
                t(if missing {
                    "explorer.kind.broken_link"
                } else {
                    "explorer.properties.symlink"
                })
                .to_owned(),
            ),
            Field {
                warning: missing.then_some("explorer.props.target_missing"),
                ..mono(
                    "explorer.properties.link_target",
                    target.display().to_string(),
                    true,
                )
            },
        ];
        if let LinkTarget::Readable {
            path,
            is_dir,
            size,
            modified,
        } = state
        {
            fields.push(field(
                "explorer.props.target_kind",
                if *is_dir {
                    t("explorer.type.folder").to_owned()
                } else {
                    crate::adapters::ui::surface::explorer::file_kind_word(&extension(path))
                },
            ));
            fields.push(field(
                "explorer.props.target_size",
                if *is_dir {
                    human_size(true, *size)
                } else {
                    size_bytes(*size)
                },
            ));
            fields.push(time_field(
                "explorer.props.target_modified",
                *modified,
                format,
            ));
        }
        fields.push(path_row());
        // 링크를 지우거나 이름을 바꾸는 작업은 링크 자신에게만 일어난다는 안내.
        let note = Some(t("explorer.props.link_note"));
        return (icons::LINK, name, fields, note);
    }
    let folder = matches!(item.kind, ItemKind::Folder);
    let ext = extension(path);
    let items = group_digits(counted_items);
    let bytes = human_size(false, counted_bytes);
    let size_args = if counting {
        [items.as_str(), bytes.as_str()]
    } else {
        [bytes.as_str(), items.as_str()]
    };
    let size = if folder {
        t_count(
            if counting {
                "explorer.properties.counting"
            } else {
                "explorer.properties.folder_size"
            },
            counted_items,
            &size_args,
        )
    } else {
        size_bytes(item.size)
    };
    let fields = vec![
        field(
            "explorer.properties.kind",
            if folder {
                t("explorer.type.folder").to_owned()
            } else {
                crate::adapters::ui::surface::explorer::file_kind_word(&ext)
            },
        ),
        Field {
            counting: folder && counting,
            warning: (folder && partial).then_some("explorer.props.partial"),
            ..field("explorer.properties.size", size)
        },
        time_field("explorer.properties.modified", item.modified, format),
        time_field("explorer.properties.created", item.created, format),
        path_row(),
        Field {
            perm: perm_edit::access(item),
            ..mono(
                "explorer.properties.permissions",
                permissions_text(item),
                false,
            )
        },
    ];
    let glyph = if folder {
        icons::FOLDER
    } else if crate::adapters::ui::surface::explorer::is_image_ext(&ext) {
        icons::IMAGE
    } else {
        icons::FILE
    };
    (glyph, name, fields, None)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn size_bytes(size: u64) -> String {
    t_count(
        "explorer.properties.size_bytes",
        size,
        &[&human_size(false, size), &group_digits(size)],
    )
}

fn remote_content(
    entries: &[DirEntryInfo],
    location: String,
    format: &str,
) -> (Icon, String, Vec<Field>, Option<&'static str>) {
    let note = Some(t("explorer.properties.remote_note"));
    if let [e] = entries {
        let glyph = if e.is_dir { icons::FOLDER } else { icons::FILE };
        let fields = vec![
            field(
                "explorer.properties.kind",
                crate::adapters::ui::surface::explorer::kind_word(e),
            ),
            field("explorer.properties.size", human_size(e.is_dir, e.size)),
            time_field("explorer.properties.modified", e.modified, format),
            mono("explorer.props.path", e.path.display().to_string(), true),
        ];
        return (glyph, e.name.clone(), fields, note);
    }
    let folders = entries.iter().filter(|e| e.is_dir).count();
    let files = entries.len() - folders;
    // 원격 목록에는 폴더 크기가 없다. 폴더가 섞이면 합을 모르므로 단일 폴더와 같은 알 수 없음 표시를 쓴다.
    let bytes: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    let fields = vec![
        field("explorer.properties.kinds", kinds_text(files, folders)),
        field(
            "explorer.properties.total_size",
            human_size(folders > 0, bytes),
        ),
        mono("explorer.properties.location", location, true),
    ];
    let n = entries.len();
    let title = t_count("explorer.properties.items", n as u64, &[&n.to_string()]);
    (icons::LAYERS, title, fields, note)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// 항목들의 공통 상위 폴더.
fn common_parent(paths: &[PathBuf]) -> String {
    let mut parent = paths
        .first()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for p in paths.iter().skip(1) {
        while !p.starts_with(&parent) {
            if !parent.pop() {
                break;
            }
        }
    }
    parent.display().to_string()
}

/// "2 files, 1 folder" 처럼 파일·폴더 수. 미리보기 패널의 여러 개 선택 머리도 쓴다.
pub(crate) fn kinds_text(files: usize, folders: usize) -> String {
    let count = |n: usize, key: &str| t_count(key, n as u64, &[&n.to_string()]);
    let mut parts = Vec::new();
    if files > 0 {
        parts.push(count(files, "explorer.properties.files"));
    }
    if folders > 0 {
        parts.push(count(folders, "explorer.properties.folders"));
    }
    parts.join(t("explorer.properties.list_separator"))
}

fn permissions_text(item: &ItemFacts) -> String {
    let flag = if item.read_only {
        t("explorer.properties.yes")
    } else {
        t("explorer.properties.no")
    };
    let read_only = t_fmt("explorer.properties.read_only", flag);
    match &item.mode {
        Some(mode) => format!("{mode} · {read_only}"),
        None => read_only,
    }
}

/// 다른 모듈의 결과 전달 시험이 쓰는 상태. 권한 편집 상자가 `request` 의 적용을 기다린다.
#[cfg(test)]
impl ExplorerProperties {
    pub(crate) fn applying_for_test(request: u64) -> Self {
        use crate::app::file_jobs::permissions::PermChange;
        let count = FolderCount::default();
        count.done.store(true, Ordering::Release);
        let path = PathBuf::from("/w/a.txt");
        Self {
            surface_id: 1,
            paths: vec![path.clone()],
            remote: None,
            query: None,
            facts: None,
            count: Arc::new(count),
            cancel: Arc::new(AtomicBool::new(false)),
            content_h: None,
            perm: Some(PermEdit {
                path,
                current: PermChange::Mode(0o644),
                draft: PermChange::Mode(0o600),
                folder: false,
                special: false,
                phase: perm_edit::Phase::Applying(request),
            }),
        }
    }

    /// 상자가 보이는 실패 이유. 상자가 없거나 실패가 아니면 None.
    pub(crate) fn perm_failure_for_test(&self) -> Option<&str> {
        match &self.perm.as_ref()?.phase {
            perm_edit::Phase::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    pub(crate) fn perm_open_for_test(&self) -> bool {
        self.perm.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_path_permissions_target_fields_and_partial_warning_keep_their_order() {
        crate::i18n::init("en");
        let mut item = ItemFacts {
            path: PathBuf::from("/a/folder"),
            kind: ItemKind::Folder,
            size: 0,
            modified: None,
            created: None,
            mode: Some("rwxr-x---".into()),
            read_only: false,
            mode_bits: Some(0o750),
            can_change_mode: true,
            hardlink: None,
        };
        let (_, _, fields, _) = single_content(&item.path, &item, (true, false, 3, 2), "minutes");
        assert_eq!(
            fields.iter().map(|f| f.label).collect::<Vec<_>>(),
            vec![
                "explorer.properties.kind",
                "explorer.properties.size",
                "explorer.properties.modified",
                "explorer.properties.created",
                "explorer.props.path",
                "explorer.properties.permissions"
            ]
        );
        assert_eq!(fields[1].warning, Some("explorer.props.partial"));
        assert!(fields[4].copy);
        assert_eq!(fields[4].value, "/a/folder");
        item.kind = ItemKind::Link {
            target: PathBuf::from("../target.md"),
            state: LinkTarget::Readable {
                path: PathBuf::from("/target.md"),
                is_dir: false,
                size: 7,
                modified: None,
            },
        };
        item.size = 999;
        let (_, _, fields, note) =
            single_content(&item.path, &item, (false, false, 0, 0), "minutes");
        assert_eq!(note, Some(t("explorer.props.link_note")));
        assert_eq!(
            fields.iter().map(|f| f.label).collect::<Vec<_>>(),
            vec![
                "explorer.properties.kind",
                "explorer.properties.link_target",
                "explorer.props.target_kind",
                "explorer.props.target_size",
                "explorer.props.target_modified",
                "explorer.props.path"
            ]
        );
        assert_eq!(fields[2].value, "Markdown");
        assert!(fields[3].value.contains('7'));
        assert!(!fields.iter().any(|f| f.value.contains("999")));
        item.kind = ItemKind::Link {
            target: PathBuf::from("missing"),
            state: LinkTarget::Missing,
        };
        let (_, _, fields, note) =
            single_content(&item.path, &item, (false, false, 0, 0), "minutes");
        assert_eq!(fields.len(), 3);
        assert_eq!(
            note,
            Some(t("explorer.props.link_note")),
            "a broken link has the note too"
        );
        assert_eq!(fields[0].value, t("explorer.kind.broken_link"));
        assert_eq!(fields[1].warning, Some("explorer.props.target_missing"));
    }

    #[test]
    fn retry_reads_a_failed_local_item_again_and_leaves_remote_items_alone() {
        let failed = |remote: Option<Vec<DirEntryInfo>>| ExplorerProperties {
            surface_id: 1,
            paths: vec![PathBuf::from("/srv/private.key")],
            remote,
            query: None,
            facts: Some(Err("Permission denied (os error 13)".into())),
            count: Arc::new(FolderCount::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            content_h: None,
            perm: None,
        };
        let mut local = failed(None);
        local.retry();
        assert!(
            local.facts.is_none(),
            "the old error is cleared while it reads again"
        );
        assert!(local.query.is_some());

        let mut remote = failed(Some(Vec::new()));
        remote.retry();
        assert!(remote.facts.is_some());
        assert!(remote.query.is_none());
    }

    #[test]
    fn the_unreadable_body_shows_the_reason_and_a_retry_button() {
        crate::i18n::init("en");
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!unreadable_body(ui, &th, "Permission denied (os error 13)"));
            });
        });
        for shape in output.shapes {
            if let egui::Shape::Text(text) = shape.shape {
                texts.push(text.galley.text().to_owned());
            }
        }
        for want in [
            "Can't read properties",
            "Permission denied (os error 13)",
            "Retry",
        ] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
    }

    #[test]
    fn several_items_share_their_nearest_parent() {
        let paths = [PathBuf::from("/a/b/c.txt"), PathBuf::from("/a/b/d/e.txt")];
        assert_eq!(common_parent(&paths), "/a/b");
        let paths = [PathBuf::from("/a/x"), PathBuf::from("/b/y")];
        assert_eq!(common_parent(&paths), "/");
    }

    use crate::app::local_reads::LocalReads;

    /// 열린 팝업의 읽기를 worker 로 돌려 결과를 받을 때까지 기다린다.
    fn settle(state: &mut MainViewState, owner: &mut LocalReads) {
        loop {
            owner.reap();
            let mut got = false;
            owner.drive(|requests| {
                got = poll(state, requests);
                true
            });
            if got {
                return;
            }
            std::thread::yield_now();
        }
    }

    fn props(state: &MainViewState) -> &ExplorerProperties {
        state.dialogs.explorer_properties.as_ref().unwrap()
    }

    fn read_paths(state: &MainViewState) -> Vec<PathBuf> {
        match &props(state).facts {
            Some(Ok(facts)) => facts.items.iter().map(|f| f.path.clone()).collect(),
            other => panic!("not read: {:?}", other.as_ref().map(|r| r.is_ok())),
        }
    }

    fn shutdown(mut owner: LocalReads) {
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
    }

    /// 메인 루프처럼 쌓인 요청을 처리하고 닫힌 팝업의 on_close 를 부른다.
    fn run_frame(
        state: &mut MainViewState,
        session: &mut crate::runtime::engine_session::EngineSession,
    ) {
        let mut engine = session.borrow_mut();
        for intent in state.take_pending_intents() {
            crate::intent::popup::handle(state, &mut engine, &intent);
        }
        let ctx = egui::Context::default();
        for id in state.popups.take_closed_ids() {
            if id == EXPLORER_PROPERTIES_POPUP_ID {
                on_close(
                    &ctx,
                    state,
                    &engine.read(),
                    &crate::adapters::ui::popup::ClosedPopup::test("test"),
                );
            }
        }
    }

    fn surface_scope(state: &MainViewState) -> Option<u32> {
        match state.popups.open_scope(EXPLORER_PROPERTIES_POPUP_ID)? {
            crate::model::popup_kind::PopupScope::Surface(id) => Some(*id),
            _ => None,
        }
    }

    #[test]
    fn a_request_from_another_surface_moves_the_popup_with_its_content() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let dir = tempfile::tempdir().unwrap();
        let x = dir.path().join("x.txt");
        let y = dir.path().join("y.txt");
        std::fs::write(&x, b"x").unwrap();
        std::fs::write(&y, b"yy").unwrap();
        open(&mut state, 1, vec![x], None, true);
        run_frame(&mut state, &mut session);
        assert_eq!(surface_scope(&state), Some(1));
        let first_cancel = props(&state).cancel.clone();

        open(&mut state, 2, vec![y.clone()], None, true);
        run_frame(&mut state, &mut session);
        assert_eq!(
            surface_scope(&state),
            Some(2),
            "the popup follows the request"
        );
        assert_eq!(
            props(&state).surface_id,
            2,
            "content and scope name one surface"
        );
        assert_eq!(props(&state).paths, [y]);
        assert!(first_cancel.load(Ordering::Relaxed));
        assert!(
            !props(&state).cancel.load(Ordering::Relaxed),
            "closing the old popup does not stop the new read"
        );
    }

    #[test]
    fn a_request_from_the_same_surface_keeps_the_popup_and_replaces_the_items() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let dir = tempfile::tempdir().unwrap();
        let x = dir.path().join("x.txt");
        let y = dir.path().join("y.txt");
        std::fs::write(&x, b"x").unwrap();
        std::fs::write(&y, b"yy").unwrap();
        open(&mut state, 1, vec![x], None, true);
        run_frame(&mut state, &mut session);
        let first_cancel = props(&state).cancel.clone();

        open(&mut state, 1, vec![y.clone()], None, true);
        let pending = state.take_pending_intents();
        assert!(
            pending.iter().all(|i| !matches!(
                i.body,
                crate::intent::Intent::Ui(crate::intent::UiIntent::ClosePopup { .. })
            )),
            "the same surface does not close and reopen"
        );
        for intent in pending {
            crate::intent::popup::handle(&mut state, &mut session.borrow_mut(), &intent);
        }
        assert!(state.popups.take_closed_ids().is_empty());
        assert_eq!(surface_scope(&state), Some(1));
        assert_eq!(props(&state).paths, [y]);
        assert!(first_cancel.load(Ordering::Relaxed), "the old count stops");
    }

    #[test]
    fn closing_the_popup_still_clears_its_state() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let dir = tempfile::tempdir().unwrap();
        let x = dir.path().join("x.txt");
        std::fs::write(&x, b"x").unwrap();
        open(&mut state, 1, vec![x], None, true);
        run_frame(&mut state, &mut session);
        let cancel = props(&state).cancel.clone();
        state.popups.close(EXPLORER_PROPERTIES_POPUP_ID);
        run_frame(&mut state, &mut session);
        assert!(state.dialogs.explorer_properties.is_none());
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn reopening_on_another_item_drops_the_late_result_of_the_first() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        std::fs::write(&first, b"1").unwrap();
        std::fs::write(&second, b"22").unwrap();
        let mut owner = LocalReads::default();
        open(&mut state, 1, vec![first], None, false);
        // 첫 읽기를 넘기고 끝나기를 기다린다. 결과는 받지 않은 채 남는다.
        owner.drive(|requests| {
            poll(&mut state, requests);
            true
        });
        owner.wait_idle();
        let old_cancel = props(&state).cancel.clone();

        open(&mut state, 1, vec![second.clone()], None, false);
        assert!(
            old_cancel.load(Ordering::Relaxed),
            "the first read is told to stop"
        );
        settle(&mut state, &mut owner);
        assert_eq!(read_paths(&state), [second]);
        shutdown(owner);
    }

    #[test]
    fn a_vanished_item_reads_as_an_error_and_retry_reads_it_once_it_is_back() {
        crate::i18n::init("en");
        let (mut state, _engine) = crate::state::tests::test_state();
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone.txt");
        let mut owner = LocalReads::default();
        open(&mut state, 1, vec![gone.clone()], None, false);
        settle(&mut state, &mut owner);
        let p = props(&state);
        let Some(Err(reason)) = &p.facts else {
            panic!("a missing item is a read error");
        };
        assert_eq!(
            reason,
            &std::fs::symlink_metadata(&gone).unwrap_err().to_string(),
            "the OS reason is shown as is"
        );
        assert!(
            p.count.done.load(Ordering::Acquire),
            "nothing is left counting"
        );
        let (_, title, fields, _) = content(p, "minutes");
        assert_eq!(title, "gone.txt");
        assert!(fields.is_empty(), "no made-up values beside the error");

        std::fs::write(&gone, b"back").unwrap();
        state.dialogs.explorer_properties.as_mut().unwrap().retry();
        settle(&mut state, &mut owner);
        assert_eq!(read_paths(&state), [gone]);
        shutdown(owner);
    }

    #[test]
    fn the_current_remote_folder_shows_unknown_values_instead_of_zero() {
        crate::i18n::init("en");
        let (mut state, _engine) = crate::state::tests::test_state();
        let cwd = PathBuf::from("/srv/remote/project");
        open_from_menu(&mut state, true, 1, &[], &cwd);
        let p = props(&state);
        assert!(p.query.is_none(), "a remote item is never read locally");
        let (glyph, title, fields, note) = content(p, "minutes");
        assert_eq!(glyph.uri, icons::FOLDER.uri);
        assert_eq!(title, "project");
        assert_eq!(note, Some(t("explorer.properties.remote_note")));
        let value = |label: &str| {
            fields
                .iter()
                .find(|f| f.label == label)
                .map(|f| f.value.clone())
                .unwrap()
        };
        assert_eq!(value("explorer.properties.size"), "—");
        assert_eq!(value("explorer.properties.modified"), "—");
        assert_eq!(value("explorer.props.path"), "/srv/remote/project");
        assert!(
            !fields.iter().any(|f| f.value.starts_with('0')),
            "{:?}",
            fields.iter().map(|f| &f.value).collect::<Vec<_>>()
        );
    }

    fn file_item(path: &str, size: u64) -> ItemFacts {
        ItemFacts {
            path: PathBuf::from(path),
            kind: ItemKind::File,
            size,
            modified: None,
            created: None,
            mode: None,
            read_only: false,
            mode_bits: None,
            can_change_mode: true,
            hardlink: None,
        }
    }

    fn local_props(paths: &[&str], facts: PropertiesFacts) -> ExplorerProperties {
        let count = FolderCount::default();
        count.done.store(true, Ordering::Release);
        ExplorerProperties {
            surface_id: 1,
            paths: paths.iter().map(PathBuf::from).collect(),
            remote: None,
            query: None,
            facts: Some(Ok(facts)),
            count: Arc::new(count),
            cancel: Arc::new(AtomicBool::new(false)),
            content_h: None,
            perm: None,
        }
    }

    #[test]
    fn some_unreadable_items_keep_the_read_values_and_warn_on_kinds() {
        crate::i18n::init("en");
        let facts = PropertiesFacts {
            items: vec![file_item("/w/a.txt", 2048)],
            file_bytes: 2048,
            failed: vec![(
                PathBuf::from("/w/d/x"),
                std::io::ErrorKind::PermissionDenied,
            )],
        };
        let (glyph, title, fields, _) =
            content(&local_props(&["/w/a.txt", "/w/d/x"], facts), "minutes");
        assert_eq!(
            glyph.svg,
            icons::LAYERS.svg,
            "two chosen items stay a several-items view"
        );
        assert_eq!(title, t_count("explorer.properties.items", 2, &["2"]));
        assert_eq!(fields[0].label, "explorer.properties.kinds");
        assert_eq!(fields[0].warning, Some("explorer.props.unreadable_items"));
        assert_eq!(fields[1].value, human_size(false, 2048));
        assert_eq!(fields[1].warning, None, "the folder count was not partial");
    }

    #[test]
    fn several_remote_items_with_a_folder_show_an_unknown_total_instead_of_zero() {
        crate::i18n::init("en");
        let entry = |name: &str, is_dir: bool, size: u64| DirEntryInfo {
            path: PathBuf::from("/srv").join(name),
            name: name.into(),
            is_dir,
            size,
            ..Default::default()
        };
        let total = |entries: &[DirEntryInfo]| {
            let (_, _, fields, _) = remote_content(entries, "/srv".into(), "minutes");
            fields
                .into_iter()
                .find(|f| f.label == "explorer.properties.total_size")
                .unwrap()
                .value
        };
        assert_eq!(total(&[entry("a", true, 0), entry("b", true, 4096)]), "—");
        assert_eq!(total(&[entry("a", true, 0), entry("f", false, 10)]), "—");
        assert_eq!(
            total(&[entry("f", false, 10), entry("g", false, 20)]),
            human_size(false, 30)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_permission_result_settles_only_its_request_and_rereads_the_item() {
        use super::perm_edit::Phase;
        use crate::app::file_jobs::permissions::PermChange;
        let mut item = file_item("/w/a.txt", 1);
        item.mode_bits = Some(0o644);
        let facts = |item: &ItemFacts| PropertiesFacts {
            items: vec![item.clone()],
            file_bytes: 1,
            failed: Vec::new(),
        };
        let mut p = local_props(&["/w/a.txt"], facts(&item));
        p.open_perm();
        let edit = p
            .perm
            .as_mut()
            .expect("a local owned file opens the editor");
        edit.draft = PermChange::Mode(0o600);
        edit.phase = Phase::Applying(9);
        assert_eq!(p.perm_request(), Some(9));

        // 다른 요청의 결과는 상자를 건드리지 않는다.
        p.settle_perm(8, Some(Ok(())));
        assert_eq!(p.perm_request(), Some(9));

        p.settle_perm(9, Some(Err("Operation not permitted (os error 1)".into())));
        let edit = p.perm.as_ref().unwrap();
        assert_eq!(
            edit.phase,
            Phase::Failed("Operation not permitted (os error 1)".into())
        );
        assert!(p.query.is_some(), "the real mode is read again");
        assert!(p.facts.is_some(), "the old values stay while reading");

        p.perm.as_mut().unwrap().phase = Phase::Applying(10);
        p.settle_perm(10, Some(Ok(())));
        assert!(p.perm.is_none(), "success closes the editor");

        // 링크·원격·소유자 아님은 상자를 열지 않는다.
        item.can_change_mode = false;
        let mut locked = local_props(&["/w/a.txt"], facts(&item));
        locked.open_perm();
        assert!(locked.perm.is_none());
    }
}
