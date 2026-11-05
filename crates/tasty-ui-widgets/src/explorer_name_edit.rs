//! Explorer 목록 맨 위에 새 항목 이름을 받는 인라인 입력과 그 아래 오류 상자.
//! Detail·List 는 행 높이(28) 한 줄, Grid 는 칸 글리프 아래 `field-width-md` 폭 입력을 칸 가운데에 둔다.
//! 입력 칸은 공용 Input 이다. 원격 응답을 기다리는 동안에는 읽기 전용 입력과 Spinner 를 그린다.
//! 본체와 갤러리가 같은 함수를 불러 같은 모양을 그린다.

use tasty_icons::Icon;
use tasty_type_appearance::theme::Theme;

use crate::input::Input;
use crate::spinner::Spinner;
use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// 입력 줄을 그릴 보기 모드.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExplorerNameLayout {
    /// 줄 왼쪽에서 글리프까지 여백과 이름 열 폭. 호출자의 상세 표 행과 같은 값을 넘긴다.
    /// 입력 칸은 이름 열 안에만 둔다.
    Detail { inset: f32, name_width: f32 },
    /// 아래 `tree_row` 행들과 아이콘 가운데를 맞춘다.
    List,
    /// Grid 칸 크기와 글리프 자리 높이. 호출자의 Grid 칸과 같은 값을 넘긴다.
    Grid { cell: egui::Vec2, slot: f32 },
}

/// 편집 상태. 호출자가 소유하고 프레임마다 넘긴다.
#[derive(Clone, Debug)]
pub struct ExplorerNameEdit {
    pub buf: String,
    /// 처음 그릴 때 입력에 포커스를 주고 고를 글자 범위(문자 단위). 한 번 쓰면 비운다.
    pub initial_selection: Option<std::ops::Range<usize>>,
}

impl ExplorerNameEdit {
    /// 기본 이름을 넣고 처음 프레임에 `select` 범위를 고르게 한다.
    pub fn new(name: String, select: std::ops::Range<usize>) -> Self {
        Self {
            buf: name,
            initial_selection: Some(select),
        }
    }
}

/// 이번 프레임의 입력 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplorerNameEvent {
    None,
    /// Enter.
    Confirm,
    /// Esc.
    Cancel,
    /// 포커스를 잃었다. 호출자가 이름이 유효하면 확정하고 아니면 취소한다.
    Blur,
}

/// 편집 줄의 덧붙임. `indent`·`caption` 은 폴더 행 아래에 여는 줄이 Detail·List 에서 쓰고 Grid 는
/// 무시한다.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExplorerNameOptions<'a> {
    /// 입력의 고정 id. 화면 밖으로 나간 줄을 다른 자리에 그려도 포커스를 지킨다.
    pub field_id: Option<egui::Id>,
    /// 글리프 앞에 더하는 들여쓰기(explorer-create-indent).
    pub indent: f32,
    /// 입력 칸 뒤의 muted caption("in {folder}"). caption 이 있으면 칸은 `explorer-create-field-max-width`
    /// 에서 멈추고 caption 이 `space-sm` 뒤에 붙는다. 칸 끝이 줄 끝에 닿으면 caption 폭만큼 칸을 줄인다.
    pub caption: Option<&'a str>,
    /// 확정한 이름을 원격이 만들기를 기다린다. 입력은 읽기 전용으로 글자를 유지하고, 칸 바로 뒤
    /// `space-sm` 자리에 `icon-size-sm` Spinner 를 둔다. 값은 Spinner 의 툴팁("Waiting for {host}")이다.
    /// 기다리는 동안 Enter·Esc·포커스 잃음은 이벤트로 내지 않는다.
    pub pending: Option<&'a str>,
}

/// 편집 줄을 그린다. 차지한 줄 rect 와 입력 칸 rect, 이벤트를 돌려준다.
pub fn explorer_name_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    layout: ExplorerNameLayout,
    glyph: Icon,
    edit: &mut ExplorerNameEdit,
    invalid: bool,
) -> (egui::Rect, egui::Rect, ExplorerNameEvent) {
    explorer_name_row_in(
        ui,
        theme,
        layout,
        ExplorerNameOptions::default(),
        glyph,
        edit,
        invalid,
    )
}

/// `explorer_name_row` 에 입력 id, 폴더 안 만들기의 들여쓰기와 caption 을 더한다.
pub fn explorer_name_row_in(
    ui: &mut egui::Ui,
    theme: &Theme,
    layout: ExplorerNameLayout,
    options: ExplorerNameOptions<'_>,
    glyph: Icon,
    edit: &mut ExplorerNameEdit,
    invalid: bool,
) -> (egui::Rect, egui::Rect, ExplorerNameEvent) {
    let row_h = theme.table_cell_height().value();
    let glyph_size = theme.icon_glyph_size_md.value();
    let gap = theme.spacing_sm.value();
    // 입력 줄 전체가 surface-active 위다. 글리프와 안내 글자도 그 위에서 읽히는 색이다.
    let ink = theme.text_on_active().to_egui();
    match layout {
        ExplorerNameLayout::Detail { .. } | ExplorerNameLayout::List => {
            let (row, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), row_h),
                egui::Sense::hover(),
            );
            let pad = theme.spacing_sm.value();
            let (radius, inset, field_right) = match layout {
                ExplorerNameLayout::Detail { inset, name_width } => {
                    (0.0, inset, row.left() + name_width)
                }
                _ => (
                    theme.corner_radius_sm.value(),
                    crate::tree_row::tree_row_icon_center(theme) - glyph_size * 0.5,
                    row.right(),
                ),
            };
            ui.painter()
                .rect_filled(row, radius, theme.surface_active().to_egui());
            let inset = inset + options.indent;
            let caption = options.caption.map(|text| {
                ui.painter().layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::proportional(theme.font_size_caption.value()),
                    ink,
                )
            });
            let field_right = match &caption {
                // 칸 끝(field_right - pad) 뒤에 gap·caption·pad 가 줄 안에 들어오게 한다.
                Some(g) => field_right.min(row.right() - g.size().x - gap),
                None => field_right,
            };
            // 기다리는 동안 칸 뒤에 Spinner 자리를 남긴다.
            let spinner_slot = options.pending.map_or(0.0, |_| pending_slot(theme));
            let field_right = field_right - spinner_slot;
            let glyph_rect = egui::Rect::from_center_size(
                egui::pos2(row.left() + inset + glyph_size * 0.5, row.center().y),
                egui::vec2(glyph_size, glyph_size),
            );
            glyph.image(glyph_size, ink).paint_at(ui, glyph_rect);
            let field_left = glyph_rect.right() + gap;
            let mut field_end = (field_right - pad).max(field_left);
            if caption.is_some() {
                field_end =
                    field_end.min(field_left + theme.explorer_create_field_max_width().value());
            }
            let field = egui::Rect::from_min_max(
                egui::pos2(field_left, row.top()),
                egui::pos2(field_end, row.bottom()),
            );
            if let Some(g) = caption {
                let pos = egui::pos2(
                    field.right() + spinner_slot + gap,
                    row.center().y - g.size().y * 0.5,
                );
                ui.painter().with_clip_rect(row).galley(pos, g, ink);
            }
            let event = field_ui(ui, theme, field, edit, invalid, options);
            (row, field, event)
        }
        ExplorerNameLayout::Grid { cell, slot } => {
            let (rect, _) = ui.allocate_exact_size(cell, egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                theme.corner_radius.value(),
                theme.surface_active().to_egui(),
            );
            let pad = theme.spacing_sm.value();
            let glyph_rect = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + pad + slot * 0.5),
                egui::vec2(glyph_size, glyph_size),
            );
            glyph.image(glyph_size, ink).paint_at(ui, glyph_rect);
            // 입력은 칸보다 넓어 양옆 칸 위로 나온다. 목록 가장자리의 칸이면 보이는 영역 안으로 민다.
            let width = theme.field_width_md.value();
            let field = egui::Rect::from_min_size(
                egui::pos2(
                    rect.center().x - width * 0.5,
                    rect.top() + pad + slot + theme.spacing_xs.value(),
                ),
                egui::vec2(width, theme.input_height().value()),
            );
            let field = field
                .translate(egui::Vec2::X * inside_shift(field.x_range(), ui.clip_rect().x_range()));
            let event = field_ui(ui, theme, field, edit, invalid, options);
            (rect, field, event)
        }
    }
}

/// `span` 을 `bounds` 안으로 옮기는 가로 거리. 들어가지 않으면 왼쪽 끝을 맞춘다.
fn inside_shift(span: egui::Rangef, bounds: egui::Rangef) -> f32 {
    if span.min < bounds.min {
        bounds.min - span.min
    } else if span.max > bounds.max {
        (bounds.max - span.max).max(bounds.min - span.min)
    } else {
        0.0
    }
}

/// 기다리는 동안 칸 뒤에 남기는 폭: 간격과 Spinner.
fn pending_slot(theme: &Theme) -> f32 {
    theme.spacing_sm.value() + theme.icon_glyph_size_sm.value()
}

/// 칸 바로 뒤에 Spinner 를 그리고, 오래 올려 두면 `tooltip` 을 보인다.
fn pending_spinner(ui: &mut egui::Ui, theme: &Theme, field: egui::Rect, tooltip: &str) {
    let size = theme.icon_glyph_size_sm.value();
    let rect = egui::Rect::from_center_size(
        egui::pos2(
            field.right() + theme.spacing_sm.value() + size * 0.5,
            field.center().y,
        ),
        egui::vec2(size, size),
    );
    Spinner::new().size(size).paint_in(ui, theme, rect);
    ui.ctx().request_repaint();
    let resp = ui.interact(
        rect,
        ui.id().with("explorer_name_pending"),
        egui::Sense::hover(),
    );
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
        Tooltip::new(tooltip)
            .id_source(resp.id)
            .show(ui, theme, rect);
    }
}

fn field_ui(
    ui: &mut egui::Ui,
    theme: &Theme,
    field: egui::Rect,
    edit: &mut ExplorerNameEdit,
    invalid: bool,
    options: ExplorerNameOptions<'_>,
) -> ExplorerNameEvent {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(field)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let pending = options.pending.is_some();
    let input = Input::new()
        .invalid(invalid && !pending)
        .read_only(pending)
        .width(field.width());
    let input = match options.field_id {
        Some(id) => input.id(id),
        None => input,
    };
    let resp = input.show(&mut child, theme, &mut edit.buf);
    if let Some(tooltip) = options.pending {
        pending_spinner(ui, theme, field, tooltip);
        return ExplorerNameEvent::None;
    }
    if let Some(range) = edit.initial_selection.take() {
        resp.request_focus();
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(range.start),
                    egui::text::CCursor::new(range.end),
                )));
            state.store(ui.ctx(), resp.id);
        }
        return ExplorerNameEvent::None;
    }
    if resp.lost_focus() {
        let (enter, esc) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        return if enter {
            ExplorerNameEvent::Confirm
        } else if esc {
            ExplorerNameEvent::Cancel
        } else {
            ExplorerNameEvent::Blur
        };
    }
    ExplorerNameEvent::None
}

/// 입력 칸 바로 아래에 오류 상자를 띄운다. 아래 행을 덮으며 목록은 밀리지 않는다.
/// menu-bg 바탕, 1px `explorer-name-error-fg` 테두리, caption text-primary, 최대 폭 240.
pub fn explorer_name_error(ui: &egui::Ui, theme: &Theme, field: egui::Rect, text: &str) {
    let pad_x = theme.spacing_sm.value();
    let pad_y = theme.spacing_xs.value();
    let max_w = theme.explorer_name_error_max_width().value();
    let font = egui::FontId::proportional(theme.font_size_caption.value());
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        font.clone(),
        theme.text_primary().to_egui(),
        max_w - 2.0 * pad_x,
    );
    job.sections[0].format.line_height = Some(font.size * theme.line_height_ui);
    let galley = ui.painter().layout_job(job);
    let size = galley.size() + egui::vec2(2.0 * pad_x, 2.0 * pad_y);
    let min = egui::pos2(
        field.left(),
        field.bottom() + crate::tokens::STRUCT_GAP_2.value(),
    );
    let rect = egui::Rect::from_min_size(min, size);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        ui.id().with("explorer_name_error"),
    ));
    let radius = theme.corner_radius.value();
    painter.add(theme.shadow_popover().to_egui().as_shape(rect, radius));
    painter.rect(
        rect,
        radius,
        theme.menu_bg().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.explorer_name_error_fg().to_egui(),
        ),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        rect.min + egui::vec2(pad_x, pad_y),
        galley,
        theme.text_primary().to_egui(),
    );
}

#[cfg(test)]
mod tests;
