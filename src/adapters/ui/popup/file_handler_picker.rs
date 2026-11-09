//! 파일을 열 핸들러 선택. 후보와 최근 항목은 같은 목록에서 하나만 선택한다.
//! 이번 열기에만 적용하며 핸들러를 등록하거나 기본값을 저장하지 않는다.
//! 자체 헤더로 경로를 한 번만 표시한다. 아이콘은 surface kind에서, 이름은 선언된 표시명이나 ID에서 가져온다.
//! 갤러리 specimen과 FH_* 치수는 공유하지만 렌더 코드는 별개다.

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::file_handler as fh_model;
use tasty_ui_widgets::tokens::{
    FH_EDGE_PAD_X, FH_EMPTY_PAD_Y, FH_FRAME_WIDTH, FH_GAP_SM, FH_HEADER_PAD_BOTTOM,
    FH_HEADER_PAD_TOP, FH_ID_LINE_GAP, FH_LIST_FADE_HEIGHT, FH_LIST_MAX_HEIGHT, FH_LIST_PAD,
    FH_RECENT_DIM_OPACITY, FH_ROW_GAP, FH_ROW_PAD_X, STRUCT_GAP_2,
};
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, TagVariant, tag, tag_mono, tag_width};
use tasty_ui_widgets::{RowParent, RowState};

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::{t, t_fmt};
use crate::state::{FileHandlerPickerResult, MainViewState};
use crate::theme::{self, Theme};

pub const PICKER_POPUP_ID: &str = "file_handler_picker";

// FH_* 치수는 공용 토큰을 사용한다. 아래 값은 Ui 없이 계산하는 줄 높이 추정치다.
// 팝업 크기와 실제 헤더·푸터 영역에도 사용하며 목록의 초과분은 스크롤로 처리한다.

/// 제목 행의 공칭 높이(제목 14 line ≈ 20, Tag 16 보다 크다).
const TITLE_LINE_H: LogicalPx = LogicalPx(20.0);
/// caption(11) 한 줄의 공칭 높이.
const CAPTION_LINE_H: LogicalPx = LogicalPx(14.0);
/// body(13) 한 줄의 공칭 높이.
const BODY_LINE_H: LogicalPx = LogicalPx(16.0);
/// empty 블록 글리프의 공칭 높이 = `icon_glyph_size_md`.
const EMPTY_GLYPH_H: LogicalPx = LogicalPx(16.0);

/// 호출부에서 준비한 핸들러 한 행의 표시 정보.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHandlerPickerEntryView {
    /// handler id 원문. 둘째 줄에 앞자름으로 들어간다.
    pub id: String,
    /// 첫 줄 이름 — 선언된 표시명이거나 id 의 마지막 `/` 뒤 조각.
    pub name: String,
    /// 이름이 **선언된** 것이면 true(UI 폰트). false 면 id 조각이라 mono 로 그린다.
    pub named: bool,
    /// action 이 여는 surface kind — 글리프의 출처. `None` 이면 `file`.
    pub kind: Option<String>,
    /// plugin 소유 — 출처 낱말과 글리프만 mauve 로 물든다(이름은 아니다).
    pub plugin: bool,
    /// 둘째 줄 첫 조각 — 출처 낱말(built-in / you / plugin).
    pub origin: String,
    /// Recent 행의 마지막 사용 시각(상대 표기). 후보 행은 `None`.
    pub when: Option<String>,
}

/// 목록의 한 그룹 — 라벨 · 개수 · 한 줄 caption · attention 톤 여부.
struct GroupHead<'a> {
    label: &'a str,
    count: usize,
    caption: Option<&'a str>,
    attention: bool,
}

pub struct FileHandlerPickerProps<'a> {
    pub theme: &'a Theme,
    /// 헤더 mono 경로 — 이미 **앞에서** 잘린 값이다
    /// (`tasty_ui_widgets::file_handler::elide_target_front`).
    pub target_display: &'a str,
    /// 감지된 형식. `None` 이면 "format unknown" Tag.
    pub detector_label: Option<&'a str>,
    pub candidates: &'a [FileHandlerPickerEntryView],
    pub recent: &'a [FileHandlerPickerEntryView],
    pub selected_id: Option<&'a str>,
    /// 이 형식에서 자동으로 실행됐을 handler — `default` Tag 가 붙는 행.
    pub default_id: Option<&'a str>,
    /// 후보가 detector 매칭이 아니라 전체 핸들러 fallback 인가.
    pub fallback: bool,
    pub header_title: &'a str,
    pub format_unknown_label: &'a str,
    pub candidates_heading: &'a str,
    pub candidates_caption: Option<&'a str>,
    pub recent_heading: &'a str,
    pub recent_caption: &'a str,
    pub fallback_notice: &'a str,
    pub default_tag_label: &'a str,
    pub empty_label: &'a str,
    pub empty_hint: &'a str,
    pub open_button_label: &'a str,
    pub cancel_button_label: &'a str,
    pub open_settings_button_label: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileHandlerPickerAction {
    None,
    Cancel,
    Select(String),
    Dispatch(String),
    OpenSettings,
}

/// surface kind에 맞는 아이콘. 모르면 file 아이콘을 사용한다.
pub fn kind_glyph(kind: Option<&str>) -> icons::Icon {
    match kind {
        Some("markdown") => icons::MARKDOWN,
        Some("html") => icons::HTML,
        Some("image") => icons::IMAGE,
        Some("directory") => icons::FOLDER,
        Some("editor") => icons::EDIT,
        Some("pager") | Some("terminal") => icons::TERMINAL,
        Some("log") => icons::LIST,
        Some("table") => icons::COLUMNS,
        Some("binary") => icons::LAYERS,
        _ => icons::FILE,
    }
}

/// 선언된 이름이 없으면 ID의 마지막 / 뒤 부분을 사용한다.
pub fn id_local_segment(id: &str) -> &str {
    match id.rfind('/') {
        Some(i) => &id[i + 1..],
        None => id,
    }
}

/// 경로에 들어갈 문자 수를 폰트 폭으로 계산한다. 단일 글리프 폭이 아니라
/// "00"과 "0"의 폭 차이를 사용해 egui의 정수 픽셀 advance 반올림을 반영한다.
fn target_budget(ui: &egui::Ui, th: &Theme) -> usize {
    let font = egui::FontId::monospace(th.font_size_caption.value());
    let one = text_w(ui, "0", &font, egui::Color32::PLACEHOLDER);
    let two = text_w(ui, "00", &font, egui::Color32::PLACEHOLDER);
    fh_model::target_budget_chars(LogicalPx(two - one))
}

/// 최근 사용 시각의 표시 구간.
///
/// | 경과 | 표시 |
/// |---|---|
/// | < 60 s | `just now` |
/// | < 60 min | `{n}m ago` |
/// | < 24 h | `{n}h ago` |
/// | 24–48 h | `yesterday` |
/// | 2–7 d | `{n}d ago` (7 일에서 천장) |
/// | ≥ 7 d | `YYYY-MM-DD` |
///
/// n은 내림한다. 일주일 이상은 길이가 일정한 날짜로 표시한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenBucket {
    JustNow,
    Minutes(i64),
    Hours(i64),
    Yesterday,
    Days(i64),
    /// 일주일 이상 — 절대 날짜로 떨어진다.
    Date,
}

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;

/// 경과 초를 구간으로. 미래 시각(시계 되감김 등)은 `JustNow` 로 접는다.
pub fn when_bucket(now_secs: i64, used_at: i64) -> WhenBucket {
    let d = (now_secs - used_at).max(0);
    if d < MINUTE {
        WhenBucket::JustNow
    } else if d < HOUR {
        WhenBucket::Minutes(d / MINUTE)
    } else if d < DAY {
        WhenBucket::Hours(d / HOUR)
    } else if d < 2 * DAY {
        WhenBucket::Yesterday
    } else if d < WEEK {
        WhenBucket::Days(d / DAY)
    } else {
        WhenBucket::Date
    }
}

/// [`when_bucket`] 을 화면 문자열로.
pub fn relative_when(now_secs: i64, used_at: i64) -> String {
    match when_bucket(now_secs, used_at) {
        WhenBucket::JustNow => t("file_handler.picker.when_just_now").to_string(),
        WhenBucket::Minutes(n) => t_fmt("file_handler.picker.when_minutes", &n.to_string()),
        WhenBucket::Hours(n) => t_fmt("file_handler.picker.when_hours", &n.to_string()),
        WhenBucket::Yesterday => t("file_handler.picker.when_yesterday").to_string(),
        WhenBucket::Days(n) => t_fmt("file_handler.picker.when_days", &n.to_string()),
        WhenBucket::Date => local_date(used_at),
    }
}

/// 일주일 이상 지난 기록은 숫자로 된 YYYY-MM-DD를 사용하므로 번역 키가 없다.
/// 변환할 수 없는 시각은 빈 칸으로 두며 열 폭은 유지한다.
fn local_date(unix_secs: i64) -> String {
    chrono::DateTime::from_timestamp(unix_secs, 0).map_or_else(String::new, |t| {
        t.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
}

/// 그룹 헤딩 한 개의 공칭 높이(caption 유무로 갈린다).
fn group_head_h(th: &Theme, caption: bool) -> LogicalPx {
    let base = th.spacing_sm + CAPTION_LINE_H + th.spacing_xs;
    if caption {
        base + STRUCT_GAP_2 + CAPTION_LINE_H
    } else {
        base
    }
}

/// 행 한 개의 공칭 높이 — 위아래 8 + 이름(13) + id(11).
fn row_h(th: &Theme) -> LogicalPx {
    th.spacing_sm.scaled(2.0) + BODY_LINE_H + CAPTION_LINE_H
}

fn header_h(th: &Theme) -> LogicalPx {
    FH_HEADER_PAD_TOP
        + TITLE_LINE_H
        + FH_GAP_SM
        + CAPTION_LINE_H
        + FH_HEADER_PAD_BOTTOM
        + th.border_width
}

fn footer_h(th: &Theme) -> LogicalPx {
    th.spacing_sm.scaled(2.0) + LogicalPx(ControlSize::Md.height(th)) + th.border_width
}

fn empty_block_h(th: &Theme) -> LogicalPx {
    FH_EMPTY_PAD_Y.scaled(2.0)
        + EMPTY_GLYPH_H
        + BODY_LINE_H
        + CAPTION_LINE_H
        + LogicalPx(ControlSize::Sm.height(th))
        + th.spacing_sm.scaled(3.0)
        + th.spacing_xs
}

/// 목록 영역의 공칭 높이 — 상한([`FH_LIST_MAX_HEIGHT`])에서 잘린다.
fn list_h(th: &Theme, cand_n: usize, recent_n: usize, fallback: bool) -> LogicalPx {
    let mut h =
        FH_LIST_PAD.scaled(2.0) + group_head_h(th, fallback) + row_h(th).scaled(cand_n as f32);
    if recent_n > 0 {
        // 그룹 구분선(위 8 + 1px) + Recent 헤딩(caption 있음) + 행들.
        h = h
            + th.spacing_sm
            + th.border_width
            + group_head_h(th, true)
            + row_h(th).scaled(recent_n as f32);
    }
    LogicalPx(h.value().min(FH_LIST_MAX_HEIGHT.value()))
}

fn picker_size_for(th: &Theme, cand_n: usize, recent_n: usize, fallback: bool) -> egui::Vec2 {
    let body = if cand_n == 0 && recent_n == 0 {
        empty_block_h(th)
    } else {
        // fallback일 때만 안내 영역 높이를 더한다.
        let strip = (th.spacing_sm.scaled(2.0) + CAPTION_LINE_H.scaled(2.0) + th.border_width)
            .scaled(if fallback { 1.0 } else { 0.0 });
        strip + list_h(th, cand_n, recent_n, fallback)
    };
    egui::vec2(
        FH_FRAME_WIDTH.value(),
        (header_h(th) + body + footer_h(th)).value(),
    )
}

pub fn picker_default_size() -> egui::Vec2 {
    picker_size_for(&theme::theme(), 4, 0, false)
}

pub fn picker_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let th = theme::theme();
    let (c, r, fallback) = state
        .dialogs
        .file_handler_picker
        .as_ref()
        .map(|p| {
            (
                p.candidates.len(),
                p.recent.len(),
                p.candidates_are_fallback,
            )
        })
        .unwrap_or((0, 0, false));
    picker_size_for(&th, c, r, fallback)
}

/// 한 줄 말줄임 — 가용 폭을 넘으면 `…` 로 끝낸다. 그린 폭을 돌려준다.
fn paint_truncated(
    ui: &egui::Ui,
    pos: egui::Pos2,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_w: f32,
) -> f32 {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(0.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    let w = galley.rect.width();
    ui.painter().galley(pos, galley, color);
    w
}

fn text_w(ui: &egui::Ui, text: &str, font: &egui::FontId, color: egui::Color32) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font.clone(), color))
        .rect
        .width()
}

fn hline(ui: &egui::Ui, th: &Theme, rect: egui::Rect, y: f32) {
    ui.painter().hline(
        rect.x_range(),
        y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// 프레임 자기 헤더 — 제목 + 형식 Tag, 그 아래 mono 경로. 아래 border 1px.
fn header_band(ui: &mut egui::Ui, props: &FileHandlerPickerProps<'_>) {
    let th = props.theme;
    let band_h = header_h(th);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FH_FRAME_WIDTH.value(), band_h.value()),
        egui::Sense::hover(),
    );
    hline(ui, th, rect, rect.bottom());

    let title_rect = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + FH_EDGE_PAD_X.value(),
            rect.top() + FH_HEADER_PAD_TOP.value(),
        ),
        egui::pos2(
            rect.right() - FH_EDGE_PAD_X.value(),
            rect.top() + FH_HEADER_PAD_TOP.value() + TITLE_LINE_H.value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(title_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    match props.detector_label {
        Some(fmt) => {
            tag_mono(&mut child, th, fmt, TagVariant::Accent, false);
        }
        None => {
            tag(
                &mut child,
                th,
                props.format_unknown_label,
                TagVariant::Default,
                false,
            );
        }
    }
    let title_font = egui::FontId::proportional(th.font_size_max.value());
    paint_truncated(
        ui,
        title_rect.left_top(),
        props.header_title,
        title_font,
        th.text_primary().into(),
        child.available_width(),
    );

    // 경로는 이미 앞에서 줄였으므로 RTL로 다시 배열하지 않는다.
    paint_truncated(
        ui,
        egui::pos2(title_rect.left(), title_rect.bottom() + FH_GAP_SM.value()),
        props.target_display,
        egui::FontId::monospace(th.font_size_caption.value()),
        th.text_muted().into(),
        title_rect.width(),
    );
}

/// fallback 안내 띠 — 이 선택이 1회성이고 아무것도 등록하지 않는다는 사실.
fn fallback_strip(ui: &mut egui::Ui, props: &FileHandlerPickerProps<'_>) {
    let th = props.theme;
    egui::Frame::new()
        .fill(th.surface_raised().into())
        .inner_margin(egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: th.spacing_sm.value() as i8,
            bottom: th.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let gs = th.icon_glyph_size_md.value();
                let (grect, _) = ui.allocate_exact_size(egui::vec2(gs, gs), egui::Sense::hover());
                icons::ALERT_TRIANGLE
                    .image(gs, th.accent_attention_raised().into())
                    .paint_at(ui, grect);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(props.fallback_notice)
                            .size(th.font_size_caption.value())
                            .color(th.text_secondary_raised()),
                    )
                    .wrap(),
                );
            });
        });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FH_FRAME_WIDTH.value(), th.border_width.value()),
        egui::Sense::hover(),
    );
    hline(ui, th, rect, rect.center().y);
}

/// 그룹 제목·개수·설명. 디자인 원본에 그룹 제목의 자간 값이 없어 별도 자간은 적용하지 않는다.
fn group_head(ui: &mut egui::Ui, th: &Theme, head: &GroupHead<'_>) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: FH_ROW_PAD_X.value() as i8,
            right: FH_ROW_PAD_X.value() as i8,
            top: th.spacing_sm.value() as i8,
            bottom: th.spacing_xs.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = FH_GAP_SM.value();
                let label_color = if head.attention {
                    th.accent_attention()
                } else {
                    th.text_secondary()
                };
                ui.label(
                    egui::RichText::new(head.label.to_uppercase())
                        .size(th.font_size_caption.value())
                        .color(label_color),
                );
                ui.label(
                    egui::RichText::new(head.count.to_string())
                        .size(th.font_size_caption.value())
                        .monospace()
                        .color(th.text_muted()),
                );
            });
            if let Some(c) = head.caption {
                ui.label(
                    egui::RichText::new(c)
                        .size(th.font_size_caption.value())
                        .color(th.text_muted()),
                );
            }
        });
}

/// 행 하나 — 글리프 · 이름 · (출처 · id · 언제) · default Tag. 단일 클릭 = 선택,
/// 더블클릭 = 선택 + 열기.
fn handler_row(
    ui: &mut egui::Ui,
    th: &Theme,
    e: &FileHandlerPickerEntryView,
    sel: bool,
    dim: bool,
    default_tag: Option<&str>,
    action: &mut FileHandlerPickerAction,
) {
    let name_font = if e.named {
        egui::FontId::proportional(th.font_size_body.value())
    } else {
        egui::FontId::monospace(th.font_size_body.value())
    };
    let id_font = egui::FontId::monospace(th.font_size_caption.value());
    let meta_font = egui::FontId::proportional(th.font_size_caption.value());
    let name_h = ui.fonts(|f| f.row_height(&name_font));
    let id_h = ui.fonts(|f| f.row_height(&id_font));
    let h = th.spacing_sm.value() * 2.0 + name_h + id_h;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());

    let state = RowState::from_response(sel, &resp);
    state.paint_fill(ui.painter(), rect, th.corner_radius_sm.value(), th);
    if sel {
        // 목록 선택 막대이므로 값이 같은 탭 토큰 대신 listctrl 토큰을 사용한다.
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(th.listctrl_selected_bar_width().value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, th.listctrl_selected_bar());
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let glyph_color: egui::Color32 = if e.plugin {
        state
            .hue(
                th,
                th.accent_agent(),
                th.accent_agent(),
                th.accent_agent_on_active(),
            )
            .into()
    } else if sel {
        state.glyph(th).into()
    } else {
        let c: egui::Color32 = state.glyph(th).into();
        if dim {
            c.gamma_multiply(FH_RECENT_DIM_OPACITY)
        } else {
            c
        }
    };
    let gs = th.icon_glyph_size_md.value();
    let grect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + FH_ROW_PAD_X.value(),
            rect.center().y - gs / 2.0,
        ),
        egui::vec2(gs, gs),
    );
    kind_glyph(e.kind.as_deref())
        .image(gs, glyph_color)
        .paint_at(ui, grect);

    let tag_w = match default_tag {
        Some(label) => tag_width(ui, th, label) + FH_ROW_GAP.value(),
        None => 0.0,
    };
    let left = grect.right() + FH_ROW_GAP.value();
    let avail = rect.right() - FH_ROW_PAD_X.value() - tag_w - left;
    let top = rect.top() + th.spacing_sm.value();

    paint_truncated(
        ui,
        egui::pos2(left, top),
        &e.name,
        name_font,
        if sel {
            state.label(th, RowParent::Base, th.text_on_active())
        } else {
            th.text_primary()
        }
        .into(),
        avail,
    );

    // 둘째 줄: 출처 · id · (언제). id 만 말줄임 — 나머지 조각은 고정 폭.
    // 선택 행은 surface-active 위이므로 둘째 줄 조각도 한 글자색이다.
    let on_active: egui::Color32 = state.meta(th, RowParent::Base).into();
    let sep: egui::Color32 = if sel {
        on_active
    } else {
        th.text_disabled().into()
    };
    let muted: egui::Color32 = state.meta(th, RowParent::Base).into();
    let origin_color: egui::Color32 = if e.plugin && !sel {
        th.accent_agent_raised().into()
    } else {
        muted
    };
    let y = top + name_h;
    let mut x = left;
    x += paint_truncated(
        ui,
        egui::pos2(x, y),
        &e.origin,
        meta_font.clone(),
        origin_color,
        avail,
    ) + FH_ID_LINE_GAP.value();
    x += paint_truncated(ui, egui::pos2(x, y), "·", meta_font.clone(), sep, avail)
        + FH_ID_LINE_GAP.value();
    // 시각은 고정 폭을 예약한다. 표시 구간이 바뀌어도 ID 폭이 흔들리지 않게 하고
    // 공간이 부족하면 날짜를 자르는 대신 ID를 줄인다.
    let when_w = if e.when.is_some() {
        text_w(ui, "·", &meta_font, sep) + th.fh_when_width().value() + FH_ID_LINE_GAP.value() * 2.0
    } else {
        0.0
    };
    x += paint_truncated(
        ui,
        egui::pos2(x, y),
        &fh_model::elide_id_front(&e.id),
        id_font,
        muted,
        (left + avail - x - when_w).max(0.0),
    ) + FH_ID_LINE_GAP.value();
    if let Some(when) = e.when.as_deref() {
        x += paint_truncated(ui, egui::pos2(x, y), "·", meta_font.clone(), sep, when_w)
            + FH_ID_LINE_GAP.value();
        paint_truncated(
            ui,
            egui::pos2(x, y),
            when,
            meta_font,
            muted,
            th.fh_when_width().value(),
        );
    }

    if let Some(label) = default_tag {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(rect.right() - tag_w, rect.top()),
                    rect.max,
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        child.add_space(FH_ROW_PAD_X.value());
        tag(&mut child, th, label, TagVariant::Accent, false);
    }

    if resp.double_clicked() {
        *action = FileHandlerPickerAction::Dispatch(e.id.clone());
    } else if resp.clicked() && !matches!(action, FileHandlerPickerAction::Dispatch(_)) {
        *action = FileHandlerPickerAction::Select(e.id.clone());
    }
}

/// 등록된 핸들러가 없을 때 설정으로 이동하는 안내.
fn empty_block(ui: &mut egui::Ui, props: &FileHandlerPickerProps<'_>) -> bool {
    let th = props.theme;
    let mut open_settings = false;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: FH_EMPTY_PAD_Y.value() as i8,
            bottom: FH_EMPTY_PAD_Y.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                let gs = th.icon_glyph_size_md.value();
                let (grect, _) = ui.allocate_exact_size(egui::vec2(gs, gs), egui::Sense::hover());
                icons::FILE
                    .image(gs, th.text_disabled().into())
                    .paint_at(ui, grect);
                ui.label(
                    egui::RichText::new(props.empty_label)
                        .size(th.font_size_body.value())
                        .color(th.text_secondary()),
                );
                ui.label(
                    egui::RichText::new(props.empty_hint)
                        .size(th.font_size_caption.value())
                        .color(th.text_muted()),
                );
                ui.add_space(th.spacing_xs.value());
                open_settings = Button::new(props.open_settings_button_label)
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, th)
                    .clicked();
            });
        });
    open_settings
}

/// 목록 — 그룹 헤딩 + 행들. 내용이 상한을 넘으면 그 영역만 스크롤하고 하단에 페이드를
/// 얹는다(헤더 · footer 는 절대 스크롤하지 않는다).
fn handler_list(
    ui: &mut egui::Ui,
    props: &FileHandlerPickerProps<'_>,
    action: &mut FileHandlerPickerAction,
) {
    let th = props.theme;
    let top = ui.cursor().top();
    egui::ScrollArea::vertical()
        .id_salt("file_handler_picker_list")
        .max_height(FH_LIST_MAX_HEIGHT.value())
        .drag_to_scroll(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(FH_LIST_PAD.value() as i8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    group_head(
                        ui,
                        th,
                        &GroupHead {
                            label: props.candidates_heading,
                            count: props.candidates.len(),
                            caption: props.candidates_caption,
                            attention: props.fallback,
                        },
                    );
                    for e in props.candidates {
                        let is_default = props.default_id == Some(e.id.as_str());
                        handler_row(
                            ui,
                            th,
                            e,
                            props.selected_id == Some(e.id.as_str()),
                            false,
                            is_default.then_some(props.default_tag_label),
                            action,
                        );
                    }
                    if !props.recent.is_empty() {
                        ui.add_space(th.spacing_sm.value());
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: FH_ROW_PAD_X.value() as i8,
                                right: FH_ROW_PAD_X.value() as i8,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), th.border_width.value()),
                                    egui::Sense::hover(),
                                );
                                hline(ui, th, r, r.center().y);
                            });
                        group_head(
                            ui,
                            th,
                            &GroupHead {
                                label: props.recent_heading,
                                count: props.recent.len(),
                                caption: Some(props.recent_caption),
                                attention: false,
                            },
                        );
                        for e in props.recent {
                            let is_default = props.default_id == Some(e.id.as_str());
                            handler_row(
                                ui,
                                th,
                                e,
                                props.selected_id == Some(e.id.as_str()),
                                true,
                                is_default.then_some(props.default_tag_label),
                                action,
                            );
                        }
                    }
                });
        });
    let bottom = ui.cursor().top();
    if bottom - top >= FH_LIST_MAX_HEIGHT.value() {
        paint_bottom_fade(
            ui,
            th,
            egui::Rect::from_min_max(
                egui::pos2(ui.max_rect().left(), bottom - FH_LIST_FADE_HEIGHT.value()),
                egui::pos2(ui.max_rect().right(), bottom),
            ),
        );
    }
}

/// 하단 페이드 — 투명 → `bg-panel`. egui 에 gradient 가 없어 얇은 띠를 쌓아 만든다.
fn paint_bottom_fade(ui: &egui::Ui, th: &Theme, rect: egui::Rect) {
    const STEPS: usize = 10;
    let base: egui::Color32 = th.bg_panel().into();
    let step_h = rect.height() / STEPS as f32;
    for i in 0..STEPS {
        let t = (i + 1) as f32 / STEPS as f32;
        let band = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + step_h * i as f32),
            egui::vec2(rect.width(), step_h),
        );
        ui.painter().rect_filled(band, 0.0, base.gamma_multiply(t));
    }
}

/// footer — Cancel(ghost) / Open(primary). 선택이 없으면 Open 비활성.
fn footer_band(ui: &mut egui::Ui, props: &FileHandlerPickerProps<'_>) -> (bool, bool) {
    let th = props.theme;
    let band_h = footer_h(th);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FH_FRAME_WIDTH.value(), band_h.value()),
        egui::Sense::hover(),
    );
    hline(ui, th, rect, rect.top());
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + FH_EDGE_PAD_X.value(),
            rect.top() + th.spacing_sm.value(),
        ),
        egui::pos2(
            rect.right() - FH_EDGE_PAD_X.value(),
            rect.bottom() - th.spacing_sm.value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    let open = Button::new(props.open_button_label)
        .variant(ButtonVariant::Primary)
        .enabled(props.selected_id.is_some())
        .show(&mut child, th)
        .clicked();
    let cancel = Button::new(props.cancel_button_label)
        .variant(ButtonVariant::Ghost)
        .show(&mut child, th)
        .clicked();
    (open, cancel)
}

/// 화면 입력을 받아 사용자 동작을 반환한다.
pub fn draw_file_handler_picker_view(
    ui: &mut egui::Ui,
    props: &FileHandlerPickerProps<'_>,
) -> FileHandlerPickerAction {
    let mut action = FileHandlerPickerAction::None;

    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return FileHandlerPickerAction::Cancel;
    }

    let empty = props.candidates.is_empty() && props.recent.is_empty();
    let mut open_settings = false;
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    ui.vertical(|ui| {
        ui.set_width(FH_FRAME_WIDTH.value());
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        header_band(ui, props);
        if props.fallback && !empty {
            fallback_strip(ui, props);
        }
        if empty {
            open_settings = empty_block(ui, props);
        } else {
            handler_list(ui, props, &mut action);
        }
        let (open, cancel) = footer_band(ui, props);
        // 우선순위: 더블클릭(Dispatch) > [Open] > [Cancel] > 단일클릭(Select).
        if matches!(action, FileHandlerPickerAction::Dispatch(_)) {
            return;
        }
        if open && let Some(id) = props.selected_id {
            action = FileHandlerPickerAction::Dispatch(id.to_string());
        } else if cancel {
            action = FileHandlerPickerAction::Cancel;
        }
    });
    if open_settings {
        return FileHandlerPickerAction::OpenSettings;
    }
    action
}

/// 결과 없이 닫혔으면 Cancelled를 기록해 호출부가 계속 기다리지 않게 한다.
pub fn on_close_file_handler_picker(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if let Some(p) = state.dialogs.file_handler_picker.as_mut()
        && p.result.is_none()
    {
        p.result = Some(crate::state::FileHandlerPickerResult::Cancelled);
    }
}

fn to_entry(s: &crate::state::PickerHandlerSummary, now: i64) -> FileHandlerPickerEntryView {
    let id = s.id.as_str().to_string();
    let (name, named) = match &s.display_name {
        Some(n) => (n.clone(), true),
        None => (id_local_segment(&id).to_string(), false),
    };
    let plugin = matches!(s.owner, crate::file::handler::HandlerOwner::Plugin(_));
    let origin = match s.owner {
        crate::file::handler::HandlerOwner::Host => t("file_handler.picker.origin_host"),
        crate::file::handler::HandlerOwner::User => t("file_handler.picker.origin_user"),
        crate::file::handler::HandlerOwner::Plugin(_) => t("file_handler.picker.origin_plugin"),
    };
    FileHandlerPickerEntryView {
        id,
        name,
        named,
        kind: s.surface_kind.clone(),
        plugin,
        origin: origin.to_string(),
        when: s.last_used_at.map(|at| relative_when(now, at)),
    }
}

/// 실행 상태에서 화면 입력을 만들고 사용자가 고른 동작을 반영한다.
pub fn draw_file_handler_picker(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let Some(picker) = state.dialogs.file_handler_picker.as_ref() else {
        return PopupAction::Close;
    };

    let th = theme::theme();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let target_display =
        fh_model::elide_target_front(&picker.target_display, target_budget(ui, &th));
    let detector_str = picker.detector.as_ref().map(|d| d.as_str().to_string());
    let candidates: Vec<FileHandlerPickerEntryView> =
        picker.candidates.iter().map(|s| to_entry(s, now)).collect();
    let recent: Vec<FileHandlerPickerEntryView> =
        picker.recent.iter().map(|s| to_entry(s, now)).collect();
    let selected_id_owned = picker.selected.as_ref().map(|s| s.as_str().to_string());
    let default_id_owned = picker
        .default_handler
        .as_ref()
        .map(|s| s.as_str().to_string());
    let fallback = picker.candidates_are_fallback;

    // 매칭 후보가 없어 전체 핸들러를 보여줄 때는 별도 제목과 안내를 사용한다.
    let candidates_heading = if fallback {
        t("file_handler.picker.fallback_heading")
    } else {
        t("file_handler.picker.suggested_heading")
    };
    let candidates_caption = fallback.then(|| t("file_handler.picker.fallback_caption"));

    let props = FileHandlerPickerProps {
        theme: &th,
        target_display: &target_display,
        detector_label: detector_str.as_deref(),
        candidates: &candidates,
        recent: &recent,
        selected_id: selected_id_owned.as_deref(),
        default_id: default_id_owned.as_deref(),
        fallback,
        header_title: t("file_handler.picker.header_title"),
        format_unknown_label: t("file_handler.picker.format_unknown"),
        candidates_heading,
        candidates_caption,
        recent_heading: t("file_handler.picker.recent_heading"),
        recent_caption: t("file_handler.picker.recent_caption"),
        fallback_notice: t("file_handler.picker.fallback_notice"),
        default_tag_label: t("file_handler.picker.default_tag"),
        empty_label: t("file_handler.picker.empty"),
        empty_hint: t("file_handler.picker.empty_hint"),
        open_button_label: t("file_handler.picker.open_button"),
        cancel_button_label: t("button.cancel"),
        open_settings_button_label: t("file_handler.picker.open_settings_button"),
    };

    let action = draw_file_handler_picker_view(ui, &props);

    match action {
        FileHandlerPickerAction::None => PopupAction::None,
        FileHandlerPickerAction::Cancel => {
            if let Some(p) = state.dialogs.file_handler_picker.as_mut() {
                p.result = Some(FileHandlerPickerResult::Cancelled);
            }
            PopupAction::Close
        }
        FileHandlerPickerAction::Select(id) => {
            if let Some(p) = state.dialogs.file_handler_picker.as_mut() {
                p.selected = Some(crate::file::handler::HandlerId(id));
            }
            PopupAction::None
        }
        FileHandlerPickerAction::Dispatch(id) => {
            if let Some(p) = state.dialogs.file_handler_picker.as_mut() {
                p.result = Some(FileHandlerPickerResult::Selected(
                    crate::file::handler::HandlerId(id),
                ));
            }
            PopupAction::Close
        }
        FileHandlerPickerAction::OpenSettings => {
            if let Some(p) = state.dialogs.file_handler_picker.as_mut() {
                p.result = Some(FileHandlerPickerResult::OpenSettings);
            }
            PopupAction::Close
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_ui_widgets::tokens::FH_TARGET_MONO_ADVANCE;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    /// 실제 앱 폰트로 경로 길이를 계산한다. 단일 글리프 폭과 글자 추가 시 증분의 차이도 확인한다.
    #[test]
    fn the_budget_is_sixty_five_on_the_font_stack_the_app_installs() {
        let th = test_theme();
        let ctx = egui::Context::default();
        crate::gfx::gpu::GpuState::setup_egui_fonts(&ctx);
        // 출력은 그리지 않으며 클로저 안의 단언만 사용한다.
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let font = egui::FontId::monospace(th.font_size_caption.value());
                let one = text_w(ui, "0", &font, egui::Color32::PLACEHOLDER);
                let two = text_w(ui, "00", &font, egui::Color32::PLACEHOLDER);
                assert_eq!(
                    target_budget(ui, &th),
                    tasty_ui_widgets::tokens::FH_TARGET_ELIDE_FALLBACK
                );
                assert_eq!(
                    fh_model::target_budget_chars(LogicalPx(one)),
                    70,
                    "단일 글리프 폭으로 계산한 대조값은 70이어야 한다"
                );
                assert_eq!(LogicalPx(two - one), FH_TARGET_MONO_ADVANCE);
            });
        });
    }

    fn entry(id: &str, name: Option<&str>, kind: &str, plugin: bool) -> FileHandlerPickerEntryView {
        FileHandlerPickerEntryView {
            id: id.to_string(),
            name: name
                .map(str::to_string)
                .unwrap_or_else(|| id_local_segment(id).to_string()),
            named: name.is_some(),
            kind: Some(kind.to_string()),
            plugin,
            origin: if plugin { "plugin" } else { "built-in" }.to_string(),
            when: None,
        }
    }

    struct Case {
        theme: Theme,
        candidates: Vec<FileHandlerPickerEntryView>,
        recent: Vec<FileHandlerPickerEntryView>,
        selected: Option<String>,
        default_id: Option<String>,
        fallback: bool,
    }

    impl Case {
        fn new() -> Self {
            Self {
                theme: test_theme(),
                candidates: vec![
                    entry(
                        "com.tasty.markdown/preview",
                        Some("Markdown preview"),
                        "markdown",
                        false,
                    ),
                    entry("dev.git-helper.diff/viewer", None, "markdown", true),
                ],
                recent: Vec::new(),
                selected: None,
                default_id: None,
                fallback: false,
            }
        }

        fn props(&self) -> FileHandlerPickerProps<'_> {
            FileHandlerPickerProps {
                theme: &self.theme,
                target_display: "docs/architecture.md",
                detector_label: (!self.fallback).then_some("markdown"),
                candidates: &self.candidates,
                recent: &self.recent,
                selected_id: self.selected.as_deref(),
                default_id: self.default_id.as_deref(),
                fallback: self.fallback,
                header_title: "Open file with…",
                format_unknown_label: "format unknown",
                candidates_heading: if self.fallback {
                    "All handlers"
                } else {
                    "Suggested"
                },
                candidates_caption: self.fallback.then_some("No handler matches this format."),
                recent_heading: "Recent",
                recent_caption: "Recently used — not matched to this format.",
                fallback_notice: "One-time choice — nothing is registered for this format.",
                default_tag_label: "default",
                empty_label: "No handlers registered.",
                empty_hint: "Register one in Settings › Handlers to open this file.",
                open_button_label: "Open",
                cancel_button_label: "Cancel",
                open_settings_button_label: "Register a handler in Settings",
            }
        }

        fn run(&self, ctx: &egui::Context, raw: egui::RawInput) -> FileHandlerPickerAction {
            let mut out = FileHandlerPickerAction::None;
            drop(ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    out = draw_file_handler_picker_view(ui, &self.props());
                });
            }));
            out
        }

        /// 프레임 위 한 점을 눌렀다 뗀다(두 프레임) — 뗄 때의 action 을 돌려준다.
        fn click_at(&self, ctx: &egui::Context, pos: egui::Pos2) -> FileHandlerPickerAction {
            drop(self.run(ctx, pointer(pos, true)));
            self.run(ctx, pointer(pos, false))
        }

        /// 고정 좌표 대신 프레임을 세로로 훑어 첫 동작을 찾는다.
        fn probe_for(
            &self,
            x: f32,
            y0: f32,
            y1: f32,
            want: impl Fn(&FileHandlerPickerAction) -> bool,
        ) -> Option<FileHandlerPickerAction> {
            let mut y = y0;
            while y < y1 {
                let ctx = egui::Context::default();
                // 레이아웃을 한 번 확정한 뒤 눌러야 hit-test 가 그 프레임의 rect 를 본다.
                drop(self.run(&ctx, egui::RawInput::default()));
                let a = self.click_at(&ctx, egui::pos2(x, y));
                if want(&a) {
                    return Some(a);
                }
                y += 2.0;
            }
            None
        }

        /// 훑어서 나오는 첫 non-None action.
        fn probe(&self, x: f32, y0: f32, y1: f32) -> Option<FileHandlerPickerAction> {
            self.probe_for(x, y0, y1, |a| *a != FileHandlerPickerAction::None)
        }
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::RawInput {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerMoved(pos));
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        raw
    }

    fn key(k: egui::Key) -> egui::RawInput {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        raw
    }

    #[test]
    fn the_name_falls_back_to_the_id_segment_after_the_last_slash() {
        assert_eq!(id_local_segment("com.tasty.image/viewer"), "viewer");
        assert_eq!(id_local_segment("legacy"), "legacy");
    }

    #[test]
    fn an_unknown_surface_kind_falls_back_to_the_file_glyph() {
        assert_eq!(kind_glyph(Some("markdown")).uri, icons::MARKDOWN.uri);
        assert_eq!(kind_glyph(Some("pager")).uri, icons::TERMINAL.uri);
        assert_eq!(kind_glyph(Some("nope")).uri, icons::FILE.uri);
        // `Ipc` / `System` 핸들러는 여는 surface 가 없어 kind 가 None 이다.
        assert_eq!(kind_glyph(None).uri, icons::FILE.uri);
    }

    /// 지금 시각 하나를 고정해 경과만 움직인다.
    const NOW: i64 = 1_700_000_000;
    fn at(elapsed: i64) -> WhenBucket {
        when_bucket(NOW, NOW - elapsed)
    }

    #[test]
    fn the_relative_time_buckets_split_where_the_design_named_them() {
        assert_eq!(at(0), WhenBucket::JustNow);
        assert_eq!(when_bucket(NOW, NOW + 500), WhenBucket::JustNow);
        assert_eq!(at(2 * HOUR), WhenBucket::Hours(2));
        assert_eq!(at(DAY + HOUR), WhenBucket::Yesterday);
    }

    /// 각 시간 구간의 양쪽 경계를 검사한다.
    #[test]
    fn every_boundary_is_pinned_from_both_sides() {
        assert_eq!(at(59), WhenBucket::JustNow);
        assert_eq!(at(MINUTE), WhenBucket::Minutes(1));

        assert_eq!(at(59 * MINUTE), WhenBucket::Minutes(59));
        assert_eq!(at(HOUR), WhenBucket::Hours(1));

        assert_eq!(at(23 * HOUR), WhenBucket::Hours(23));
        assert_eq!(at(DAY), WhenBucket::Yesterday);

        assert_eq!(at(47 * HOUR), WhenBucket::Yesterday);
        assert_eq!(at(2 * DAY), WhenBucket::Days(2));

        assert_eq!(at(6 * DAY), WhenBucket::Days(6));
        assert_eq!(at(WEEK), WhenBucket::Date);
    }

    /// `n` 은 정수 **내림**이다 — 1분 59초는 아직 `1m` 이다.
    #[test]
    fn the_count_floors_rather_than_rounds() {
        assert_eq!(at(MINUTE + 59), WhenBucket::Minutes(1));
        assert_eq!(at(2 * MINUTE - 1), WhenBucket::Minutes(1));
        assert_eq!(at(HOUR + 59 * MINUTE), WhenBucket::Hours(1));
        assert_eq!(at(2 * DAY + 23 * HOUR), WhenBucket::Days(2));
    }

    #[test]
    fn nothing_beyond_a_week_grows_a_new_word() {
        for elapsed in [WEEK, 30 * DAY, 365 * DAY, 3 * 365 * DAY] {
            assert_eq!(at(elapsed), WhenBucket::Date, "elapsed={elapsed}");
        }
    }

    /// 로컬 시간대가 달라도 YYYY-MM-DD 형식이고 UTC 날짜의 하루 전후에 있어야 한다.
    #[test]
    fn the_week_old_row_shows_a_bare_ten_character_date() {
        let used_at = NOW - 30 * DAY;
        let out = relative_when(NOW, used_at);
        assert_eq!(out.chars().count(), 10, "{out}");
        assert!(
            out.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }),
            "{out}"
        );
        let utc = chrono::DateTime::from_timestamp(used_at, 0).expect("representable");
        let near: Vec<String> = [-1i64, 0, 1]
            .iter()
            .map(|d| {
                (utc + chrono::Duration::days(*d))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .collect();
        assert!(near.contains(&out), "{out} not near {near:?}");
    }

    #[test]
    fn the_frame_is_always_the_canonical_width() {
        let th = test_theme();
        for (c, r, f) in [(0, 0, false), (4, 0, false), (5, 2, false), (9, 0, true)] {
            assert_eq!(picker_size_for(&th, c, r, f).x, FH_FRAME_WIDTH.value());
        }
        assert_eq!(picker_default_size().x, FH_FRAME_WIDTH.value());
    }

    #[test]
    fn a_long_list_stops_growing_at_the_cap() {
        let th = test_theme();
        let tall = picker_size_for(&th, 40, 10, false).y;
        let taller = picker_size_for(&th, 400, 10, false).y;
        assert_eq!(tall, taller, "목록은 상한에서 스크롤로 바뀐다");
        assert!(
            tall <= header_h(&th).value()
                + FH_LIST_MAX_HEIGHT.value()
                + footer_h(&th).value()
                + 0.5,
            "헤더 + 264 + footer 를 넘지 않는다: {tall}"
        );
    }

    #[test]
    fn the_empty_branch_reserves_the_block_not_a_list() {
        let th = test_theme();
        let empty = picker_size_for(&th, 0, 0, false).y;
        assert!((empty - (header_h(&th) + empty_block_h(&th) + footer_h(&th)).value()).abs() < 0.5);
    }

    #[test]
    fn escape_cancels() {
        let c = Case::new();
        let ctx = egui::Context::default();
        assert_eq!(
            c.run(&ctx, key(egui::Key::Escape)),
            FileHandlerPickerAction::Cancel
        );
    }

    #[test]
    fn an_idle_frame_asks_for_nothing() {
        let c = Case::new();
        let ctx = egui::Context::default();
        assert_eq!(
            c.run(&ctx, egui::RawInput::default()),
            FileHandlerPickerAction::None
        );
    }

    #[test]
    fn every_state_renders_without_panicking() {
        for (cands, recent, fallback) in [
            (2usize, 0usize, false),
            (2, 2, false),
            (5, 0, true),
            (0, 0, false),
            (20, 3, false),
        ] {
            let mut c = Case::new();
            c.fallback = fallback;
            c.candidates = (0..cands)
                .map(|i| {
                    entry(
                        &format!("com.tasty.x{i}/viewer"),
                        None,
                        "editor",
                        i % 2 == 0,
                    )
                })
                .collect();
            c.recent = (0..recent)
                .map(|i| FileHandlerPickerEntryView {
                    when: Some("2h ago".into()),
                    ..entry(
                        &format!("com.tasty.r{i}/viewer"),
                        Some("Recent one"),
                        "log",
                        false,
                    )
                })
                .collect();
            c.default_id = c.candidates.first().map(|e| e.id.clone());
            let ctx = egui::Context::default();
            assert_eq!(
                c.run(&ctx, egui::RawInput::default()),
                FileHandlerPickerAction::None
            );
        }
    }

    #[test]
    fn a_single_click_selects_the_row_it_landed_on() {
        let c = Case::new();
        let size = picker_size_for(&c.theme, c.candidates.len(), 0, false);
        let got = c.probe(size.x / 2.0, 0.0, size.y);
        assert_eq!(
            got,
            Some(FileHandlerPickerAction::Select(
                "com.tasty.markdown/preview".into()
            )),
            "첫 후보 행을 눌렀을 때 선택이 나와야 한다"
        );
    }

    /// 선택은 두 그룹을 가로질러 하나다 — Recent 행도 같은 한 선택의 후보다.
    #[test]
    fn a_recent_row_is_selectable_like_any_other() {
        let mut c = Case::new();
        c.candidates.clear();
        c.recent = vec![FileHandlerPickerEntryView {
            when: Some("yesterday".into()),
            ..entry("io.binview.hex/viewer", None, "binary", true)
        }];
        let size = picker_size_for(&c.theme, 0, 1, false);
        let got = c.probe(size.x / 2.0, 0.0, size.y + 60.0);
        assert_eq!(
            got,
            Some(FileHandlerPickerAction::Select(
                "io.binview.hex/viewer".into()
            ))
        );
    }

    /// 선택이 있으면 Open 이 dispatch 한다. 선택이 없으면 그 자리를 눌러도 아무 일이 없다.
    #[test]
    fn open_dispatches_only_with_a_selection() {
        let mut c = Case::new();
        c.selected = Some("com.tasty.markdown/preview".into());
        // 실제 렌더링된 푸터의 세로 위치를 찾아 클릭한다.
        let x = FH_FRAME_WIDTH.value() - FH_EDGE_PAD_X.value() - 20.0;
        let span = picker_size_for(&c.theme, c.candidates.len(), 0, false).y * 2.0;
        let got = c.probe_for(x, 0.0, span, |a| {
            matches!(a, FileHandlerPickerAction::Dispatch(_))
        });
        assert_eq!(
            got,
            Some(FileHandlerPickerAction::Dispatch(
                "com.tasty.markdown/preview".into()
            ))
        );

        let none = Case::new();
        assert_eq!(none.selected, None);
        let got = none.probe_for(x, 0.0, span, |a| {
            matches!(a, FileHandlerPickerAction::Dispatch(_))
        });
        assert_eq!(got, None, "선택이 없으면 Open 은 비활성이다");
    }

    #[test]
    fn the_empty_state_offers_the_way_out_to_settings() {
        let mut c = Case::new();
        c.candidates.clear();
        let size = picker_size_for(&c.theme, 0, 0, false);
        let got = c.probe(size.x / 2.0, 0.0, size.y);
        assert_eq!(got, Some(FileHandlerPickerAction::OpenSettings));
    }

    /// 더블클릭은 그 행을 고르고 바로 연다. 이미 다른 행이 선택돼 있어도 더블클릭한 행이 열린다.
    #[test]
    fn a_double_click_selects_and_opens_the_row_it_landed_on() {
        let mut c = Case::new();
        c.selected = Some("dev.git-helper.diff/viewer".into());
        let size = picker_size_for(&c.theme, c.candidates.len(), 0, false);
        let x = size.x / 2.0;
        let want = FileHandlerPickerAction::Select("com.tasty.markdown/preview".into());
        let mut y = 0.0;
        let row_y = loop {
            assert!(y < size.y, "첫 후보 행을 찾지 못했다");
            let ctx = egui::Context::default();
            drop(c.run(&ctx, egui::RawInput::default()));
            if c.click_at(&ctx, egui::pos2(x, y)) == want {
                break y;
            }
            y += 2.0;
        };

        let ctx = egui::Context::default();
        drop(c.run(&ctx, egui::RawInput::default()));
        assert_eq!(c.click_at(&ctx, egui::pos2(x, row_y)), want);
        assert_eq!(
            c.click_at(&ctx, egui::pos2(x, row_y)),
            FileHandlerPickerAction::Dispatch("com.tasty.markdown/preview".into()),
            "두 번째 클릭은 선택이 아니라 그 행의 열기여야 한다"
        );
    }

    /// 프레임에 그려진 글자열을 모은다.
    fn painted_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    /// URL 대상은 detector 를 거치지 않아 형식 Tag 가 "format unknown" 이고,
    /// 경로 자리에 URL 원문이 파일 경로와 같은 앞자름 규칙으로 들어간다.
    #[test]
    fn a_url_target_shows_format_unknown_and_the_front_elided_url() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        let url = "https://example.com/docs/guides/handlers/picker/canonical/form/\
                   with/a/very/long/tail/index.html";
        let (mut state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let read = engine.read();
        let all = read.file_handler.all_handlers();
        // 링크 메뉴와 같은 호출 — URL 은 detector 없이 picker 로 간다.
        crate::file::dispatch::open_picker_from_read(
            &mut state,
            &read,
            crate::file::dispatch::DispatchTarget::http_url(url).expect("http(s) url"),
            None,
            all,
            true,
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                ignore_size_limit: false,
                run: None,
            },
        );

        let th = test_theme();
        let ctx = egui::Context::default();
        crate::gfx::gpu::GpuState::setup_egui_fonts(&ctx);
        let mut expected_path = String::new();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                expected_path = fh_model::elide_target_front(url, target_budget(ui, &th));
                assert!(matches!(
                    draw_file_handler_picker(ui, &mut state, &read),
                    PopupAction::None
                ));
            });
        });
        let texts = painted_texts(&out.shapes);

        assert!(
            expected_path.starts_with('…') && url.ends_with(&expected_path['…'.len_utf8()..]),
            "예제 URL 은 예산보다 길어 앞에서 잘려야 한다: {expected_path}"
        );
        assert!(
            texts.contains(&expected_path),
            "경로 자리에 앞자름한 URL 이 그려져야 한다: {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t == url),
            "잘리지 않은 URL 원문이 그대로 그려지면 안 된다"
        );
        assert!(
            texts
                .iter()
                .any(|s| s == t("file_handler.picker.format_unknown")),
            "형식 Tag 는 format unknown 이어야 한다: {texts:?}"
        );
    }
}
