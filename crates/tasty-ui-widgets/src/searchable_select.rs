//! 열린 목록 위에 검색칸이 있는 단일 선택 드롭다운(디자인 `components/forms/SearchableSelect`).
//! 닫힌 모습은 Select 트리거와 같고, 열면 메뉴 팝오버 안에 검색칸 · 구분선 · 고정 행(+구분선) ·
//! 필터한 옵션 행 · 상태 행(결과 없음·로딩)을 쌓는다. 옵션 목록만 스크롤한다.
//!
//! 열면 검색칸이 포커스를 받는다. ↑/↓는 보이는 활성 행 사이를 끝에서 멈추며 움직이고(고정 행 먼저),
//! Enter는 확정하고 닫으며, Esc는 닫고 트리거로 포커스를 돌려주고, Tab은 닫는다. 열 때 활성 행은
//! 현재 값이고 검색어가 바뀔 때마다 첫 일치 행으로 간다. 닫을 때마다 검색어를 비운다.
//! 위젯은 번역 문구를 소유하지 않으며 [`SearchableSelectLabels`] 로 받는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::menu_item::{menu_option_natural_width, menu_option_row};
use crate::select::{alloc_trigger, paint_chevron, paint_trigger_box};

/// 목록의 한 옵션. `value` 는 호출자가 저장하는 값이고 `label` 은 보이는 문구다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchableSelectOption<'a> {
    pub value: &'a str,
    pub label: &'a str,
    /// 보이지만 고를 수 없다. 키보드 커서도 건너뛴다.
    pub disabled: bool,
}

impl<'a> SearchableSelectOption<'a> {
    /// 값과 문구가 같은 옵션.
    pub const fn new(label: &'a str) -> Self {
        Self {
            value: label,
            label,
            disabled: false,
        }
    }

    /// 값과 문구가 다른 옵션. 예: 값 `""` 에 문구 "Default (monospace)".
    pub const fn with_value(value: &'a str, label: &'a str) -> Self {
        Self {
            value,
            label,
            disabled: false,
        }
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// 위젯이 보여 주는 문구. 번역은 호출자가 제공한다.
#[derive(Clone, Copy, Debug)]
pub struct SearchableSelectLabels<'a> {
    /// 값이 없을 때 트리거 문구(placeholder 색).
    pub placeholder: &'a str,
    /// 검색칸이 비었을 때의 안내 문구.
    pub search_placeholder: &'a str,
    /// 검색어와 맞는 옵션이 없을 때의 상태 행.
    pub empty: &'a str,
    /// 옵션을 아직 받는 중일 때 목록 끝의 상태 행.
    pub loading: &'a str,
}

/// 갤러리·시험이 상호작용 없이 한 상태를 그대로 그릴 때 쓰는 값. 시안의 specimen hook
/// (`open` · `query` · `activeIndex` · `hoverIndex` · `focusSearch` · `hoverTrigger`)과 같다.
/// `active`·`hover` 는 보이는 행(고정 행 먼저) 기준 인덱스다.
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchableSelectSpecimen<'a> {
    pub open: bool,
    pub query: &'a str,
    pub active: Option<usize>,
    pub hover: Option<usize>,
    pub focus_search: bool,
    pub hover_trigger: bool,
}

/// 검색어로 남는 옵션의 인덱스. 앞뒤 공백을 뺀 검색어가 문구 어디든 대소문자 무관하게 들어 있으면
/// 남긴다. 빈 검색어는 전부 남긴다.
pub fn searchable_select_filter(options: &[SearchableSelectOption<'_>], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    options
        .iter()
        .enumerate()
        .filter(|(_, o)| q.is_empty() || o.label.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

/// 외부에서 팝업 상태를 다룰 때 쓰는 ID. 호출자가 ID 생성 규칙을 복제하지 않게 공개한다.
pub fn searchable_select_popup_id(ui: &egui::Ui, id_salt: &str) -> egui::Id {
    ui.make_persistent_id(("tasty_searchable_select", id_salt))
}

/// 검색칸의 TextEdit ID. 시험이 포커스를 확인할 때도 쓴다.
pub fn searchable_select_search_id(popup_id: egui::Id) -> egui::Id {
    popup_id.with("search")
}

/// 열린 동안만 사는 상태. 닫히면 지운다.
#[derive(Clone, Default)]
struct OpenState {
    query: String,
    /// 보이는 행 기준 키보드 활성 행.
    active: Option<usize>,
    /// 다음에 그리는 검색칸이 포커스를 가져간다.
    focus_search: bool,
}

fn state_id(popup_id: egui::Id) -> egui::Id {
    popup_id.with("state")
}

/// 보이는 행: 고정 행 전부 뒤에 검색어로 남은 옵션.
fn visible_rows<'o>(
    pinned: &'o [SearchableSelectOption<'o>],
    options: &'o [SearchableSelectOption<'o>],
    query: &str,
) -> Vec<&'o SearchableSelectOption<'o>> {
    pinned
        .iter()
        .chain(
            searchable_select_filter(options, query)
                .into_iter()
                .map(|i| &options[i]),
        )
        .collect()
}

/// 검색어가 바뀐 뒤의 활성 행: 첫 활성 일치 행, 없으면 첫 활성 고정 행.
fn first_match(rows: &[&SearchableSelectOption<'_>], pinned_len: usize) -> Option<usize> {
    (pinned_len..rows.len())
        .chain(0..pinned_len.min(rows.len()))
        .find(|i| !rows[*i].disabled)
}

/// 열 때의 활성 행: 현재 값의 행, 없으면 첫 활성 행.
fn initial_active(rows: &[&SearchableSelectOption<'_>], value: Option<&str>) -> Option<usize> {
    value
        .and_then(|v| rows.iter().position(|o| o.value == v && !o.disabled))
        .or_else(|| rows.iter().position(|o| !o.disabled))
}

/// 활성 행 사이를 한 칸 움직인다. 목록 끝에서는 멈춘다. 커서가 없으면 첫 활성 행에서 시작한다.
fn step(
    rows: &[&SearchableSelectOption<'_>],
    active: Option<usize>,
    forward: bool,
) -> Option<usize> {
    let enabled: Vec<usize> = (0..rows.len()).filter(|i| !rows[*i].disabled).collect();
    let last = enabled.len().checked_sub(1)?;
    let pos = active.and_then(|a| enabled.iter().position(|i| *i == a));
    let next = match (pos, forward) {
        (None, _) => 0,
        (Some(p), true) => (p + 1).min(last),
        (Some(p), false) => p.saturating_sub(1),
    };
    Some(enabled[next])
}

/// 트리거에 보일 문구와 placeholder 여부. 목록에 없는 값도 값 그대로 보인다.
fn trigger_label<'a>(
    pinned: &'a [SearchableSelectOption<'a>],
    options: &'a [SearchableSelectOption<'a>],
    value: Option<&'a str>,
    placeholder: &'a str,
) -> (&'a str, bool) {
    match value {
        None => (placeholder, true),
        Some(v) => (
            pinned
                .iter()
                .chain(options)
                .find(|o| o.value == v)
                .map_or(v, |o| o.label),
            false,
        ),
    }
}

/// 이번 프레임에 열린 목록이 가져가는 키.
#[derive(Default, Clone, Copy)]
struct NavKeys {
    up: bool,
    down: bool,
    enter: bool,
    escape: bool,
    tab: bool,
}

/// 검색칸이 그리기 전에 방향키·Enter·Esc를 소비한다. Tab은 egui가 포커스를 옮기도록 남긴다.
fn take_nav_keys(ui: &egui::Ui) -> NavKeys {
    let none = egui::Modifiers::NONE;
    ui.input_mut(|i| NavKeys {
        up: i.consume_key(none, egui::Key::ArrowUp),
        down: i.consume_key(none, egui::Key::ArrowDown),
        enter: i.consume_key(none, egui::Key::Enter),
        escape: i.consume_key(none, egui::Key::Escape),
        tab: i.key_pressed(egui::Key::Tab),
    })
}

/// 트리거의 상태별 모습.
#[derive(Clone, Copy)]
struct TriggerLook {
    open: bool,
    focus: bool,
    hover: bool,
}

#[allow(clippy::too_many_arguments)] // reason: 트리거 한 칸을 그리는 데 필요한 값을 그대로 받는다
fn paint_trigger(
    ui: &egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    label: &str,
    is_placeholder: bool,
    enabled: bool,
    look: TriggerLook,
) {
    let pad_x = theme.select_padding_x();
    let chevron_room = theme.select_chevron_room();
    let focused = enabled && (look.open || look.focus);
    let border = if focused {
        theme.select_border_focus()
    } else if enabled && look.hover {
        theme.select_search_trigger_border_hover()
    } else {
        theme.select_border()
    };
    paint_trigger_box(
        ui.painter(),
        theme,
        crate::Ground::Panel,
        rect,
        border,
        enabled,
    );
    // 열림·포커스는 테두리 바깥 1px ring을 더한다(시안 box-shadow 0 0 0 1px).
    if focused {
        ui.painter().rect_stroke(
            rect,
            theme.select_radius().value(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.select_border_focus().to_egui(),
            ),
            egui::StrokeKind::Outside,
        );
    }
    let ink = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };
    let fg = if is_placeholder {
        theme.select_search_value_placeholder()
    } else {
        theme.select_fg()
    };
    // 긴 값은 끝을 줄이고 트리거는 커지지 않는다.
    let text_max_width = (LogicalPx(rect.width()) - chevron_room - pad_x).max(LogicalPx(0.0));
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(theme.select_font_size().value()),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_max_width.value());
    let galley = ui.fonts(|f| f.layout_job(job));
    let pos = egui::pos2(
        rect.left() + pad_x.value(),
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, ink(fg.to_egui()));
    let cx = rect.right() - (chevron_room * 0.5).value();
    paint_chevron(
        ui.painter(),
        egui::pos2(cx, rect.center().y),
        ink(theme.select_chevron_fg().to_egui()),
        look.open,
    );
}

/// 검색칸 내용. 실제 위젯은 TextEdit, 견본은 고정 문구를 그린다.
enum Field<'s> {
    Live { buf: &'s mut String, id: egui::Id },
    Static { text: &'s str, focus: bool },
}

impl Field<'_> {
    fn text(&self) -> &str {
        match self {
            Field::Live { buf, .. } => buf,
            Field::Static { text, .. } => text,
        }
    }
}

/// 검색칸 한 줄. TextEdit 응답(실제 위젯일 때)과 검색어 변경 여부를 돌려준다.
fn search_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    placeholder: &str,
    field: &mut Field<'_>,
    focus_now: bool,
) -> bool {
    let height = theme.select_search_field_height();
    let pad_x = theme.select_search_field_padding_x();
    let gap = theme.select_search_field_gap();
    let icon = theme.icon_glyph_size_sm;
    let radius = theme.corner_radius.value();
    let font = egui::FontId::proportional(theme.select_font_size().value());
    let width = ui.available_width();
    let (outer, _) =
        ui.allocate_exact_size(egui::vec2(width, height.value()), egui::Sense::hover());
    ui.painter()
        .rect_filled(outer, radius, theme.select_search_field_bg().to_egui());

    let inner = outer.shrink2(egui::vec2(pad_x.value(), 0.0));
    let irect = egui::Rect::from_center_size(
        egui::pos2(inner.left() + (icon * 0.5).value(), inner.center().y),
        egui::vec2(icon.value(), icon.value()),
    );
    tasty_icons::SEARCH
        .image(icon.value(), theme.select_search_icon_fg().to_egui())
        .paint_at(ui, irect);
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(irect.right() + gap.value(), inner.top()),
        inner.max,
    );

    let (changed, focused) = match field {
        Field::Live { buf, id } => {
            // 부모에는 위의 칸 하나만 할당하고 TextEdit은 할당하지 않는 자식 Ui에 둔다.
            let mut slot = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(text_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let te = egui::TextEdit::singleline(*buf)
                .id(*id)
                .frame(false)
                .desired_width(text_rect.width())
                .font(font)
                .text_color(theme.select_search_field_fg().to_egui())
                .hint_text(
                    egui::RichText::new(placeholder)
                        .color(theme.select_search_placeholder().to_egui()),
                );
            let r = slot.add(te);
            if focus_now {
                r.request_focus();
            }
            (r.changed(), r.has_focus() || focus_now)
        }
        Field::Static { text, focus } => {
            let (shown, color) = if text.is_empty() {
                (placeholder, theme.select_search_placeholder())
            } else {
                (*text, theme.select_search_field_fg())
            };
            let mut job = egui::text::LayoutJob::simple_singleline(
                shown.to_owned(),
                font,
                egui::Color32::PLACEHOLDER,
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(text_rect.width().max(0.0));
            let g = ui.fonts(|f| f.layout_job(job));
            let pos = egui::pos2(
                text_rect.left(),
                text_rect.center().y - g.rect.height() * 0.5,
            );
            ui.painter().galley(pos, g, color.to_egui());
            (false, *focus)
        }
    };
    // 포커스는 테두리 색만 바꾸고 ring은 그리지 않는다 — ring은 열린 트리거에 있다.
    let edge = if focused {
        theme.select_search_field_border_focus()
    } else {
        theme.select_search_field_border()
    };
    ui.painter().rect_stroke(
        outer,
        radius,
        egui::Stroke::new(theme.border_width.value(), edge.to_egui()),
        egui::StrokeKind::Inside,
    );
    changed
}

/// 검색칸 아래와 고정 행 아래의 구분선. 모든 메뉴 구분선과 같은 `menu-separator` 색이다.
fn menu_list_separator(ui: &mut egui::Ui, theme: &Theme) {
    crate::multi_select::list_separator(ui, theme, theme.menu_separator().to_egui());
}

/// 결과 없음·로딩 상태 행. 누를 수 없다. 로딩은 Spinner + 간격 + 문구다.
fn status_row(ui: &mut egui::Ui, theme: &Theme, label: &str, loading: bool) {
    let height = theme.menu_item_height();
    let pad = theme.menu_item_padding_x();
    let gap = theme.spacing_sm;
    let spinner = theme.icon_glyph_size_sm;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height.value()), egui::Sense::hover());
    // 행 왼쪽 끝에서 글자가 시작하는 자리까지.
    let mut lead = pad;
    if loading {
        let srect = egui::Rect::from_min_size(
            egui::pos2(
                rect.left() + lead.value(),
                rect.center().y - (spinner * 0.5).value(),
            ),
            egui::vec2(spinner.value(), spinner.value()),
        );
        crate::Spinner::new()
            .size(spinner.value())
            .paint_in(ui, theme, srect);
        lead += spinner + gap;
    }
    let text_w = (LogicalPx(rect.width()) - lead - pad).max(LogicalPx(0.0));
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(theme.select_font_size().value()),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_w.value());
    let g = ui.fonts(|f| f.layout_job(job));
    let pos = egui::pos2(
        rect.left() + lead.value(),
        rect.center().y - g.rect.height() * 0.5,
    );
    ui.painter()
        .galley(pos, g, theme.select_search_status_fg().to_egui());
}

/// 열린 목록을 그리는 데 필요한 값.
struct Menu<'a> {
    id_salt: &'a str,
    pinned: &'a [SearchableSelectOption<'a>],
    options: &'a [SearchableSelectOption<'a>],
    labels: &'a SearchableSelectLabels<'a>,
    loading: bool,
    value: Option<&'a str>,
    /// 목록 안쪽 폭(팝업 − 안쪽 여백·테두리).
    content_width: LogicalPx,
}

/// 목록 본문 한 번의 결과.
#[derive(Default)]
struct MenuOutput {
    /// 누른 옵션의 값.
    picked: Option<String>,
}

/// 팝업 안쪽 폭. 트리거 폭을 하한으로, 가장 넓은 행을 담되 `select-search-menu-max-width` 까지만 커진다.
fn content_width(
    ctx: &egui::Context,
    theme: &Theme,
    width: LogicalPx,
    pinned: &[SearchableSelectOption<'_>],
    options: &[SearchableSelectOption<'_>],
    labels: &SearchableSelectLabels<'_>,
    loading: bool,
) -> LogicalPx {
    let chrome = menu_chrome(theme);
    let min = (width - chrome).max(LogicalPx(0.0));
    let max = (theme.select_search_menu_max_width() - chrome).max(min);
    let font = egui::FontId::proportional(theme.select_font_size().value());
    let text_w = |s: &str| {
        LogicalPx(
            ctx.fonts(|f| {
                f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                    .rect
                    .width()
            })
            .ceil(),
        )
    };
    let pad2 = theme.menu_item_padding_x() * 2.0;
    let status = if loading {
        text_w(labels.loading) + theme.icon_glyph_size_sm + theme.spacing_sm
    } else {
        text_w(labels.empty)
    } + pad2;
    pinned
        .iter()
        .chain(options)
        .map(|o| LogicalPx(menu_option_natural_width(ctx, theme, false, o.label)))
        .fold(status, LogicalPx::max)
        .clamp(min, max)
}

/// 팝업 틀이 가로·세로 각각 차지하는 폭: 안쪽 여백과 테두리 양쪽.
fn menu_chrome(theme: &Theme) -> LogicalPx {
    (theme.select_search_inset() + theme.border_width) * 2.0
}

fn menu_margin(theme: &Theme) -> egui::Margin {
    egui::Margin::same(theme.select_search_inset().value().round() as i8)
}

/// 검색칸 · 구분선 · 고정 행(+구분선) · 스크롤 목록 · 상태 행.
/// `active`·`hover` 는 보이는 행 기준이다. 실제 위젯이면 검색어가 바뀔 때 `active` 를 첫 일치 행으로
/// 옮기고, 포인터가 움직여 올라간 행을 활성 행으로 삼는다.
#[allow(clippy::too_many_arguments)] // reason: 실제 위젯과 견본이 함께 쓰는 본문이라 상태를 낱낱이 받는다
fn menu_body(
    ui: &mut egui::Ui,
    theme: &Theme,
    menu: &Menu<'_>,
    mut field: Field<'_>,
    focus_search: bool,
    active: &mut Option<usize>,
    hover: Option<usize>,
    scroll_to_active: bool,
) -> MenuOutput {
    let live = matches!(field, Field::Live { .. });
    ui.set_min_width(menu.content_width.value());
    ui.set_max_width(menu.content_width.value());
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    let edited = search_field(
        ui,
        theme,
        menu.labels.search_placeholder,
        &mut field,
        focus_search,
    );
    menu_list_separator(ui, theme);

    let rows = visible_rows(menu.pinned, menu.options, field.text());
    let pinned_len = menu.pinned.len();
    if edited {
        *active = first_match(&rows, pinned_len);
    }
    // 목록이 바뀌어 갈 곳을 잃은 커서는 버린다.
    *active = active.filter(|i| rows.get(*i).is_some_and(|o| !o.disabled));

    let mut out = MenuOutput::default();
    let pointer_moved = live && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
    let row = |ui: &mut egui::Ui, i: usize, out: &mut MenuOutput, active: &mut Option<usize>| {
        let o = rows[i];
        let resp = menu_option_row(
            ui,
            theme,
            o.label,
            menu.value == Some(o.value),
            *active == Some(i),
            !o.disabled,
            hover == Some(i),
        );
        // 끝이 줄어든 문구는 호버하면 전체를 보여 준다.
        let natural = LogicalPx(menu_option_natural_width(ui.ctx(), theme, false, o.label));
        let resp = if natural > LogicalPx(resp.rect.width()) {
            resp.on_hover_text(o.label)
        } else {
            resp
        };
        if !live {
            return;
        }
        if pointer_moved && !o.disabled && resp.hovered() {
            *active = Some(i);
        }
        if scroll_to_active && *active == Some(i) {
            ui.scroll_to_rect(resp.rect, None);
        }
        if resp.clicked() && !o.disabled {
            out.picked = Some(o.value.to_owned());
        }
    };

    for i in 0..pinned_len {
        row(ui, i, &mut out, active);
    }
    if pinned_len > 0 {
        menu_list_separator(ui, theme);
    }

    // 팝업 전체가 select-search-list-max-height 를 넘지 않도록 옵션 목록 높이를 남은 만큼으로 제한한다.
    let sep = theme.spacing_xs * 2.0 + theme.border_width;
    let row_h = theme.menu_item_height();
    let fixed = menu_chrome(theme)
        + theme.select_search_field_height()
        + sep
        + if pinned_len > 0 {
            row_h * pinned_len as f32 + sep
        } else {
            LogicalPx(0.0)
        };
    let list_max = (theme.select_search_list_max_height() - fixed).max(LogicalPx(0.0));
    // 스크롤 영역은 부모의 남은 높이에도 묶인다. 행 높이로 잰 목록 높이를 하한으로 주어 견본처럼
    // 높이가 정해진 자리에 놓여도 상한(list_max)까지는 펼쳐지게 한다.
    let status_rows = usize::from(menu.loading || rows.len() == pinned_len);
    let list_rows = rows.len() - pinned_len + status_rows;
    let list_h = (row_h * list_rows as f32).min(list_max);
    egui::ScrollArea::vertical()
        .id_salt(("tasty_searchable_select_list", menu.id_salt))
        .max_height(list_max.value())
        .min_scrolled_height(list_h.value())
        .auto_shrink([false, true])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for i in pinned_len..rows.len() {
                row(ui, i, &mut out, active);
            }
            if menu.loading {
                status_row(ui, theme, menu.labels.loading, true);
            } else if rows.len() == pinned_len {
                status_row(ui, theme, menu.labels.empty, false);
            }
        });
    out
}

/// [`SearchableSelect::show_response`] 의 결과.
pub struct SearchableSelectResponse {
    /// 트리거 상자의 응답. 행 배치처럼 트리거 자리가 필요한 호출자가 읽는다.
    pub response: egui::Response,
    /// 이번 프레임에 값이 바뀌었다.
    pub changed: bool,
    /// 이 프레임을 끝낸 뒤 목록이 열려 있다.
    pub open: bool,
}

/// SearchableSelect 빌더. 값은 `show` 인자로 받는다.
pub struct SearchableSelect<'a> {
    id_salt: &'a str,
    options: &'a [SearchableSelectOption<'a>],
    pinned: &'a [SearchableSelectOption<'a>],
    labels: SearchableSelectLabels<'a>,
    /// 트리거 폭. `None` 이면 가용 폭(시안 `block`).
    width: Option<LogicalPx>,
    enabled: bool,
    loading: bool,
}

impl<'a> SearchableSelect<'a> {
    pub fn new(
        id_salt: &'a str,
        options: &'a [SearchableSelectOption<'a>],
        labels: SearchableSelectLabels<'a>,
    ) -> Self {
        Self {
            id_salt,
            options,
            pinned: &[],
            labels,
            width: None,
            enabled: true,
            loading: false,
        }
    }

    /// 검색어와 관계없이 맨 위에 늘 보이는 행. 자체 구분선 아래로 옵션이 이어진다.
    pub fn pinned(mut self, pinned: &'a [SearchableSelectOption<'a>]) -> Self {
        self.pinned = pinned;
        self
    }

    /// 옵션을 아직 받는 중이다. 목록 끝에 로딩 상태 행을 둔다.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// 트리거 폭. 미지정이면 가용 폭을 채운다.
    pub fn width(mut self, width: LogicalPx) -> Self {
        self.width = Some(width);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn menu<'m>(
        &'m self,
        ctx: &egui::Context,
        theme: &Theme,
        width: LogicalPx,
        value: Option<&'m str>,
    ) -> Menu<'m> {
        Menu {
            id_salt: self.id_salt,
            pinned: self.pinned,
            options: self.options,
            labels: &self.labels,
            loading: self.loading,
            value,
            content_width: content_width(
                ctx,
                theme,
                width,
                self.pinned,
                self.options,
                &self.labels,
                self.loading,
            ),
        }
    }

    /// 그리고 값이 바뀌면 `true` 를 반환한다. `value` 가 `None` 이면 트리거는 placeholder 다.
    /// 목록에 없는 값이나 검색으로 숨은 값도 트리거에 그대로 보이며 위젯이 바꾸지 않는다.
    pub fn show(self, ui: &mut egui::Ui, theme: &Theme, value: &mut Option<String>) -> bool {
        self.show_response(ui, theme, value).changed
    }

    /// [`show`](Self::show) 와 같고 트리거의 [`egui::Response`] 도 돌려준다. 응답의 rect 는 트리거
    /// 상자이며 팝업은 포함하지 않는다. 값이 바뀐 프레임에는 응답에 `changed` 가 표시된다.
    pub fn show_response(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        value: &mut Option<String>,
    ) -> SearchableSelectResponse {
        let popup_id = searchable_select_popup_id(ui, self.id_salt);
        let sid = state_id(popup_id);
        // 위젯 자체나 감싼 Ui 가 비활성이면 열려 있던 자기 목록을 닫는다. egui 팝업은 기억된 열림
        // 상태만 보고 독립 Area 로 그려지므로 닫지 않으면 비활성인 동안에도 옵션을 고를 수 있다.
        // 다른 위젯의 팝업은 건드리지 않는다.
        let enabled = self.enabled && ui.is_enabled();
        if !enabled && ui.memory(|m| m.is_popup_open(popup_id)) {
            ui.memory_mut(|m| m.close_popup());
        }
        let mut open = enabled && ui.memory(|m| m.is_popup_open(popup_id));
        if !open {
            // 바깥 클릭·다른 팝업 열기처럼 위젯 밖에서 닫혀도 다음에 열 때 검색어가 비어 있다.
            ui.data_mut(|d| d.remove::<OpenState>(sid));
        }
        let mut st: OpenState = ui.data(|d| d.get_temp(sid)).unwrap_or_default();

        let mut changed = false;
        let mut refocus_trigger = false;
        let mut scroll_to_active = false;
        if open {
            let nav = take_nav_keys(ui);
            let rows = visible_rows(self.pinned, self.options, &st.query);
            for (pressed, forward) in [(nav.down, true), (nav.up, false)] {
                if pressed {
                    st.active = step(&rows, st.active, forward);
                    scroll_to_active = true;
                }
            }
            // 이번 프레임의 트리거 문구에 반영되도록 트리거보다 먼저 확정한다.
            let commit = nav
                .enter
                .then(|| st.active.and_then(|i| rows.get(i)))
                .flatten()
                .filter(|o| !o.disabled)
                .map(|o| o.value.to_owned());
            let committed = commit.is_some();
            if let Some(v) = commit {
                changed = value.as_deref() != Some(v.as_str());
                *value = Some(v);
            }
            // 활성 행 없이 누른 Enter는 아무것도 하지 않고 목록을 연 채로 둔다.
            if nav.escape || nav.tab || committed {
                ui.memory_mut(|m| m.close_popup());
                open = false;
                // Tab은 egui의 포커스 이동에 맡긴다.
                refocus_trigger = !nav.tab;
            }
        }

        let width = self
            .width
            .unwrap_or_else(|| LogicalPx(ui.available_width()));
        let (rect, mut resp) = alloc_trigger(ui, theme, width.value(), enabled);
        let open_by_key = enabled
            && !open
            && resp.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        if (enabled && resp.clicked()) || open_by_key {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
            open = ui.memory(|m| m.is_popup_open(popup_id));
            st = OpenState::default();
            if open {
                let rows = visible_rows(self.pinned, self.options, "");
                st.active = initial_active(&rows, value.as_deref());
                st.focus_search = true;
                scroll_to_active = true;
            }
        }
        if refocus_trigger {
            resp.request_focus();
        }
        if resp.has_focus() {
            // 닫힌 트리거의 ↓가 egui 포커스 이동으로 새지 않게 한다.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }

        let (label, is_placeholder) = trigger_label(
            self.pinned,
            self.options,
            value.as_deref(),
            self.labels.placeholder,
        );
        paint_trigger(
            ui,
            theme,
            rect,
            label,
            is_placeholder,
            enabled,
            TriggerLook {
                open,
                focus: resp.has_focus(),
                hover: resp.hovered(),
            },
        );

        let current = value.clone();
        let menu = self.menu(ui.ctx(), theme, width, current.as_deref());
        // 트리거와 팝업 사이 select-search-menu-gap. 팝업은 기준 rect 바로 아래에 붙으므로 기준만 내린다.
        let mut anchor = resp.clone();
        anchor.rect = resp
            .rect
            .translate(egui::vec2(0.0, theme.select_search_menu_gap().value()));
        let mut parent = ui.new_child(egui::UiBuilder::new().max_rect(resp.rect));
        let search_id = searchable_select_search_id(popup_id);
        let focus_now = std::mem::take(&mut st.focus_search);
        let mut picked = None;
        let OpenState { query, active, .. } = &mut st;
        tasty_egui_theme::with_popover_frame_ring(&mut parent, theme, menu_margin(theme), |ui| {
            egui::popup_below_widget(
                ui,
                popup_id,
                &anchor,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    picked = menu_body(
                        ui,
                        theme,
                        &menu,
                        Field::Live {
                            buf: query,
                            id: search_id,
                        },
                        focus_now,
                        active,
                        None,
                        scroll_to_active,
                    )
                    .picked;
                },
            )
        });
        if let Some(v) = picked {
            changed |= value.as_deref() != Some(v.as_str());
            *value = Some(v);
            ui.memory_mut(|m| m.close_popup());
            resp.request_focus();
            open = false;
        }
        if open {
            ui.data_mut(|d| d.insert_temp(sid, st));
        } else {
            ui.data_mut(|d| d.remove::<OpenState>(sid));
        }
        if changed {
            resp.mark_changed();
        }
        SearchableSelectResponse {
            response: resp,
            changed,
            open: ui.memory(|m| m.is_popup_open(popup_id)),
        }
    }

    /// 상호작용 없이 `spec` 상태를 그린다. 열린 상태면 목록을 팝업 대신 트리거 아래 자리에 그대로 둔다.
    /// 갤러리와 시험이 같은 그리기 코드로 각 상태를 나란히 보여 줄 때 쓴다.
    pub fn show_specimen(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        value: Option<&str>,
        spec: &SearchableSelectSpecimen<'_>,
    ) {
        let width = self
            .width
            .unwrap_or_else(|| LogicalPx(ui.available_width()));
        // 실제 팝업처럼 트리거 아래 select-search-menu-gap 만큼만 띄운다. 부모의 세로 item_spacing 이
        // 트리거와 목록 사이에 끼지 않도록 둘을 간격 0 인 범위 안에 쌓는다. 범위 뒤의 간격은 부모 그대로다.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(width.value(), theme.select_height().value()),
                egui::Sense::hover(),
            );
            let (label, is_placeholder) =
                trigger_label(self.pinned, self.options, value, self.labels.placeholder);
            let open = self.enabled && spec.open;
            paint_trigger(
                ui,
                theme,
                rect,
                label,
                is_placeholder,
                self.enabled,
                TriggerLook {
                    open,
                    focus: false,
                    hover: spec.hover_trigger,
                },
            );
            if !open {
                return;
            }
            let menu = self.menu(ui.ctx(), theme, width, value);
            let mut active = spec.active;
            ui.add_space(theme.select_search_menu_gap().value());
            tasty_egui_theme::with_popover_frame_ring(ui, theme, menu_margin(theme), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    menu_body(
                        ui,
                        theme,
                        &menu,
                        Field::Static {
                            text: spec.query,
                            focus: spec.focus_search,
                        },
                        false,
                        &mut active,
                        spec.hover,
                        false,
                    );
                })
            });
        });
    }
}
