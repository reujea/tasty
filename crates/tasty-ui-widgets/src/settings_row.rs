//! 설정 서브탭의 행 격자. 라벨 열 · gap · 컨트롤과, 한 행에 딸린 caption 이나 경고 callout.
//!
//! 라벨 열은 서브탭에서 가장 긴 라벨에 맞추고 `settings-label-width` … `settings-label-max-width`
//! 로 clamp 한다. 열보다 긴 라벨은 열 안에서 줄을 바꾼다. 열 폭은 egui 가 자식 영역을 내용 폭으로
//! 줄이지 않도록 정확한 크기로 할당한다. 컨트롤이 라벨 옆에 들어가지 않는 행은 쌓는다
//! ([`crate::settings_row_stack`]).

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::field_error::{FieldErrorLine, paint_field_error_line};
use crate::help_hint::HelpHint;
use crate::icon_button::IconPainter;
use crate::settings_row_stack::{StackRow, row_is_stacked, settings_stack_row};
use crate::tooltip::TooltipPlacement;
use crate::warning_callout::warning_callout;

/// 행 바로 아래에 붙는 내용.
#[derive(Clone, Copy)]
enum Below<'a> {
    None,
    Caption(&'a str),
    Warning(&'a str, IconPainter<'a>),
}

/// 설정 한 행. 라벨 열 폭을 재는 데도 같은 값을 쓴다.
#[derive(Clone, Copy)]
pub struct SettingsRow<'a> {
    label: &'a str,
    hint: Option<&'a str>,
    below: Below<'a>,
}

impl<'a> SettingsRow<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            hint: None,
            below: Below::None,
        }
    }

    /// 라벨 뒤 물음표 도움말. 아래 caption 과 같은 내용을 두 번 쓰지 않는다.
    pub fn hint(mut self, text: &'a str) -> Self {
        self.hint = Some(text);
        self
    }

    /// 행 아래 muted 설명.
    pub fn caption(mut self, text: &'a str) -> Self {
        self.below = Below::Caption(text);
        self
    }

    /// 행 아래 항상 보이는 경고 callout.
    pub fn warning(mut self, text: &'a str, paint_icon: IconPainter<'a>) -> Self {
        self.below = Below::Warning(text, paint_icon);
        self
    }

    /// 줄을 바꾸지 않았을 때 라벨 칸이 차지하는 폭(도움말 아이콘 포함).
    pub fn natural_label_width(&self, ui: &egui::Ui, theme: &Theme) -> LogicalPx {
        let text = label_galley(ui, theme, self.label, f32::INFINITY).size().x;
        LogicalPx(text) + hint_width(theme, self.hint.is_some())
    }

    /// 라벨 칸만 그린다. 컨트롤 줄을 직접 짜는 행(override 행 등)이 쓴다. caption 은 그리지 않는다.
    pub fn show_label(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        label_col: LogicalPx,
        row_h: LogicalPx,
    ) -> egui::Response {
        settings_label_cell(ui, theme, label_col, row_h, self.label, self.hint)
    }

    /// 행을 그리고 `control` 이 돌려준 오류 줄을 컨트롤 열에서 시작해 그 아래에 둔다. 줄은 입력의
    /// 포커스에 따라 정해지므로 컨트롤을 그린 뒤에 받는다. 행이 쌓였으면 줄도 행 시작에서 시작한다.
    pub fn show_with_error<'b>(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        label_col: LogicalPx,
        control: impl FnOnce(&mut egui::Ui) -> Option<FieldErrorLine<'b>>,
    ) -> egui::Response {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = theme.settings_row_caption_gap().value();
            let id = ui.id().with(("settings_row", self.label));
            let row = StackRow {
                id,
                label_col,
                row_h: theme.settings_row_min_height(),
                label: self.label,
                hint: self.hint,
                align_top: false,
            };
            let mut line = None;
            settings_stack_row(ui, theme, row, |ui| line = control(ui));
            if let Some(line) = line {
                let indent = if row_is_stacked(ui, id) {
                    LogicalPx(0.0)
                } else {
                    label_col + theme.settings_label_gap()
                };
                paint_field_error_line(ui, theme, indent, line);
            }
        })
        .response
    }

    /// 행을 그린다. caption·callout 이 있으면 행과 `settings-row-caption-gap` 만큼 띄운다.
    /// 쌓임 상태는 부모 Ui id 와 라벨로 기억한다.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        label_col: LogicalPx,
        control: impl FnOnce(&mut egui::Ui),
    ) -> egui::Response {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = theme.settings_row_caption_gap().value();
            let row = StackRow {
                id: ui.id().with(("settings_row", self.label)),
                label_col,
                row_h: theme.settings_row_min_height(),
                label: self.label,
                hint: self.hint,
                align_top: false,
            };
            settings_stack_row(ui, theme, row, control);
            match self.below {
                Below::None => {}
                Below::Caption(text) => settings_row_caption(ui, theme, text),
                Below::Warning(text, paint_icon) => {
                    ui.scope(|ui| {
                        ui.set_max_width(below_width(ui, theme));
                        warning_callout(ui, theme, text, paint_icon);
                    });
                }
            }
        })
        .response
    }
}

/// 서브탭의 라벨 열 폭. 가장 긴 라벨을 `settings-label-width` … `settings-label-max-width` 로 clamp 한다.
pub fn settings_label_column<'a>(
    ui: &egui::Ui,
    theme: &Theme,
    rows: impl IntoIterator<Item = &'a SettingsRow<'a>>,
) -> LogicalPx {
    let longest = rows
        .into_iter()
        .map(|row| row.natural_label_width(ui, theme))
        .fold(LogicalPx(0.0), LogicalPx::max);
    longest.clamp(
        theme.settings_label_width(),
        theme.settings_label_max_width(),
    )
}

/// 라벨 열 한 칸. 폭 `label_col` 을 그대로 차지하고, 라벨은 그 안에서 줄을 바꾸며 행 높이
/// `row_h` 안에서 세로 가운데에 놓인다. 행을 직접 짜는 화면(단축키 탭 등)도 이 칸을 쓴다.
pub fn settings_label_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    row_h: LogicalPx,
    label: &str,
    hint: Option<&str>,
) -> egui::Response {
    let hint_w = hint_width(theme, hint.is_some());
    let galley = label_galley(ui, theme, label, (label_col - hint_w).value().max(1.0));
    let text_size = galley.size();
    let cell_h = row_h.value().max(text_size.y);
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(label_col.value(), cell_h), egui::Sense::hover());
    let text_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - text_size.y / 2.0),
        text_size,
    );
    let last_row = galley.rows.last().map(|r| r.rect);
    // `put` 은 부모 커서를 그 rect 뒤로 옮겨 열 폭을 되돌린다. 자리는 위에서 할당했으므로
    // 글자와 도움말은 커서와 무관한 자식 Ui 에 놓는다.
    let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    text_ui.add(egui::Label::new(galley));
    if let (Some(text), Some(last)) = (hint, last_row) {
        let glyph = theme.icon_glyph_size_sm.value();
        let x = text_rect.left() + last.right() + theme.help_hint_gap().value();
        let y = text_rect.top() + last.center().y - glyph / 2.0;
        let hint_rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(glyph, glyph));
        let mut hint_ui = ui.new_child(egui::UiBuilder::new().max_rect(hint_rect));
        HelpHint::new(text)
            .placement(TooltipPlacement::Bottom)
            .show(&mut hint_ui, theme);
    }
    resp
}

/// [`settings_label_cell`] 뒤 컨트롤까지의 간격을 `settings-label-gap` 으로 맞춘다. 부모 가로 줄의
/// `item_spacing.x` 가 칸 사이에 이미 들어가므로 그만큼 빼고 띄운다.
pub fn settings_label_gap(ui: &mut egui::Ui, theme: &Theme) {
    let auto = ui.spacing().item_spacing.x;
    ui.add_space((theme.settings_label_gap().value() - auto).max(0.0));
}

/// 행 아래 caption·callout 의 줄바꿈 폭. `measure-md` 를 넘지 않고, 남은 폭이 더 좁으면 그 폭에서
/// 줄을 바꾼다. 고정 `measure-md` 로 두면 좁은 창에서 글이 오른쪽 끝에서 잘린다.
fn below_width(ui: &egui::Ui, theme: &Theme) -> f32 {
    theme.measure_md.value().min(ui.available_width())
}

/// 한 행에 딸린 설명. 행 왼쪽 끝에서 시작하고 `measure-md` 폭(남은 폭이 더 좁으면 그 폭)에서 줄을 바꾼다.
/// 행과의 간격은 호출하는 쪽의 세로 간격이 정한다([`SettingsRow::caption`] 은 `settings-row-caption-gap`).
///
/// `- ` 로 시작하는 줄은 목록 항목(•), `  - ` 는 한 단계 안쪽 항목(–)이다. 항목이 줄을 넘기면 글머리표
/// 뒤 자리에서 이어진다. 목록 줄이 없는 글은 한 덩어리로 그린다.
pub fn settings_row_caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(below_width(ui, theme));
        if !text.lines().any(|l| caption_list_item(l).is_some()) {
            ui.add(caption_label(theme, text));
            return;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in text.lines() {
            let Some((depth, body)) = caption_list_item(line) else {
                ui.add(caption_label(theme, line));
                continue;
            };
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let step = theme.spacing_md.value();
                ui.add_space(step * depth as f32);
                let mark = ui.add(caption_label(theme, if depth == 0 { "•" } else { "–" }));
                ui.add_space((step - mark.rect.width()).max(0.0));
                let rest = ui.available_width();
                ui.vertical(|ui| {
                    ui.set_max_width(rest);
                    ui.add(caption_label(theme, body));
                });
            });
        }
    });
}

/// 설명 한 줄이 목록 항목이면 (깊이, 본문). 깊이 0 은 `- `, 1 은 `  - ` 로 시작한다.
fn caption_list_item(line: &str) -> Option<(usize, &str)> {
    if let Some(body) = line.strip_prefix("- ") {
        Some((0, body))
    } else {
        line.strip_prefix("  - ").map(|body| (1, body))
    }
}

fn caption_label(theme: &Theme, text: &str) -> egui::Label {
    egui::Label::new(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    )
    .wrap()
}

/// 서브탭 맨 위, 행들 위에 한 번 두는 중립 안내. info 글리프와 caption 본문이며 `measure-md`
/// (남은 폭이 더 좁으면 그 폭)에서 줄을 바꾸고 말줄임하지 않는다. 경고가 아니라 사실 안내라
/// 경고색·상자를 쓰지 않는다. 글리프는 본문 첫 줄 높이의 가운데에 둔다.
pub fn settings_apply_note(ui: &mut egui::Ui, theme: &Theme, text: &str) -> egui::Response {
    let glyph = theme.icon_glyph_size_sm.value();
    let gap = theme.settings_apply_note_gap().value();
    let caption = theme.font_size_caption.value();
    let line_h = caption * theme.line_height_ui;
    let text_w = (below_width(ui, theme) - glyph - gap).max(1.0);
    let galley = ui.fonts(|f| {
        let mut job = egui::text::LayoutJob::single_section(
            text.to_owned(),
            egui::TextFormat {
                font_id: egui::FontId::proportional(caption),
                color: theme.settings_apply_note_fg().to_egui(),
                line_height: Some(line_h),
                ..Default::default()
            },
        );
        job.wrap.max_width = text_w;
        f.layout_job(job)
    });
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let (slot, _) = ui.allocate_exact_size(egui::vec2(glyph, line_h), egui::Sense::hover());
        let glyph_rect = egui::Rect::from_center_size(slot.center(), egui::vec2(glyph, glyph));
        tasty_icons::INFO
            .image(glyph, theme.settings_apply_note_glyph().to_egui())
            .paint_at(ui, glyph_rect);
        ui.add(egui::Label::new(galley));
    })
    .response
}

fn hint_width(theme: &Theme, has_hint: bool) -> LogicalPx {
    if has_hint {
        theme.help_hint_gap() + theme.icon_glyph_size_sm
    } else {
        LogicalPx(0.0)
    }
}

/// 라벨 칸이 `label_col` 폭에서 줄을 바꿨을 때의 글자 높이(도움말 아이콘 자리 포함).
pub(crate) fn label_cell_height(
    ui: &egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    label: &str,
    hint: Option<&str>,
) -> LogicalPx {
    let wrap = (label_col - hint_width(theme, hint.is_some()))
        .value()
        .max(1.0);
    LogicalPx(label_galley(ui, theme, label, wrap).size().y)
}

fn label_galley(ui: &egui::Ui, theme: &Theme, text: &str, wrap_width: f32) -> Arc<egui::Galley> {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let color = theme.text_secondary().to_egui();
    ui.fonts(|f| f.layout(text.to_owned(), font, color, wrap_width))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `width` 폭 영역에 안내 한 개를 그리고 차지한 영역과 글자 색을 돌려준다.
    fn note_frame(width: f32, text: &str) -> (egui::Rect, Vec<egui::Color32>) {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut used = egui::Rect::NOTHING;
        // 새 Area 는 첫 프레임에 크기만 재고 그리지 않으므로 두 번째 프레임의 출력을 읽는다.
        let mut out = egui::FullOutput::default();
        for _ in 0..2 {
            out = ctx.run(Default::default(), |ctx| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        ui.set_max_width(width);
                        used = settings_apply_note(ui, &theme, text).rect;
                    });
            });
        }
        let inks = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                // 시험 컨텍스트에는 이미지 로더가 없어 글리프 자리에 egui 의 대체 글자가 그려진다.
                egui::Shape::Text(t) if t.galley.text() == text => {
                    t.galley.job.sections.first().map(|s| s.format.color)
                }
                _ => None,
            })
            .collect();
        (used, inks)
    }

    /// 안내는 measure-md(또는 더 좁은 남은 폭) 안에서 줄을 바꾸고, 글자는 apply-note 잉크다.
    #[test]
    fn the_apply_note_wraps_inside_the_measure_and_uses_the_note_ink() {
        let theme = tasty_themes::mocha_fallback();
        let text = "Saved changes apply to terminals you open afterward. Terminals that are already open don't change right away.";
        let measure = theme.measure_md.value();
        let (wide, inks) = note_frame(measure * 2.0, text);
        assert!(
            wide.width() <= measure + 0.5,
            "measure-md 를 넘었다: {wide:?}"
        );
        assert_eq!(inks, vec![theme.settings_apply_note_fg().to_egui()]);
        let narrow_w = measure / 2.0;
        let (narrow, _) = note_frame(narrow_w, text);
        assert!(
            narrow.width() <= narrow_w + 0.5,
            "좁은 폭을 넘었다: {narrow:?}"
        );
        assert!(
            narrow.height() > wide.height(),
            "좁은 폭에서 줄을 바꾸지 않았다"
        );
    }

    /// 컨트롤 줄 아래 오류 줄의 왼쪽 끝과 글자 수를 잰다.
    fn error_row(width: f32, ink: bool) -> (egui::Rect, egui::Rect, usize) {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let col = theme.settings_label_width();
        let mut control = egui::Rect::NOTHING;
        let mut after = egui::Rect::NOTHING;
        let mut out = egui::FullOutput::default();
        for _ in 0..3 {
            out = ctx.run(Default::default(), |ctx| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        ui.set_max_width(width);
                        SettingsRow::new("Label").show_with_error(ui, &theme, col, |ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(theme.field_width_lg.value(), 28.0),
                                egui::Sense::hover(),
                            );
                            control = rect;
                            Some(FieldErrorLine { text: "bad", ink })
                        });
                        after = ui
                            .allocate_exact_size(egui::vec2(1.0, 1.0), egui::Sense::hover())
                            .0;
                    });
            });
        }
        let text = out
            .shapes
            .iter()
            .filter(|c| matches!(&c.shape, egui::Shape::Text(t) if t.galley.text() == "bad"))
            .count();
        (control, after, text)
    }

    #[test]
    fn the_error_line_starts_in_the_control_column_under_the_control() {
        let (control, after, drawn) = error_row(620.0, true);
        assert_eq!(drawn, 1);
        assert!(after.top() > control.bottom());
    }

    /// 칸만 남긴 줄은 글자를 그리지 않고 같은 높이를 차지한다.
    #[test]
    fn a_held_line_takes_the_same_height_without_ink() {
        let (_, shown, _) = error_row(620.0, true);
        let (_, held, drawn) = error_row(620.0, false);
        assert_eq!(drawn, 0);
        assert_eq!(shown.top(), held.top());
    }

    /// 설명을 그리고 글 조각마다 (글, 왼쪽 위, 줄 수)를 돌려준다.
    fn caption_texts(width: f32, text: &str) -> Vec<(String, egui::Pos2, usize)> {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut out = egui::FullOutput::default();
        for _ in 0..2 {
            out = ctx.run(Default::default(), |ctx| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        ui.set_max_width(width);
                        settings_row_caption(ui, &theme, text);
                    });
            });
        }
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => {
                    Some((t.galley.text().to_owned(), t.pos, t.galley.rows.len()))
                }
                _ => None,
            })
            .collect()
    }

    /// `- ` 줄은 글머리표 뒤에서 시작하고 줄을 넘겨도 그 자리에서 이어진다. `  - ` 는 한 단계 더 들어간다.
    /// 목록 줄 앞의 글과 목록 아닌 설명은 왼쪽 끝에서 시작한다.
    #[test]
    fn list_lines_in_a_caption_hang_after_their_bullet() {
        let step = tasty_themes::mocha_fallback().spacing_md.value();
        let long = "a long item that keeps going well past one row of this narrow caption";
        let texts = caption_texts(
            200.0,
            &format!("Intro sentence.\n- {long}\n  - nested item"),
        );
        let at = |s: &str| {
            texts
                .iter()
                .find(|(t, ..)| t == s)
                .unwrap_or_else(|| panic!("{s} 가 없다: {texts:?}"))
                .clone()
        };
        let (_, intro, _) = at("Intro sentence.");
        let (_, bullet, _) = at("•");
        let (_, item, rows) = at(long);
        let (_, sub_mark, _) = at("–");
        let (_, sub, _) = at("nested item");
        assert!(rows > 1, "좁은 폭에서 항목이 줄을 넘기지 않았다");
        assert!((bullet.x - intro.x).abs() < 0.5, "{texts:?}");
        assert!((item.x - intro.x - step).abs() < 0.5, "{texts:?}");
        assert!((sub_mark.x - intro.x - step).abs() < 0.5, "{texts:?}");
        assert!((sub.x - intro.x - 2.0 * step).abs() < 0.5, "{texts:?}");
        let plain = caption_texts(200.0, "One line.\nTwo line.");
        assert_eq!(plain.len(), 1, "목록 없는 설명은 한 덩어리다: {plain:?}");
    }
}
