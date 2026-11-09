//! 설정의 override 한 행 — 라벨 열(설정 행 격자의 열 폭) · 컨트롤(자기 필드 폭) · 뒤따르는 "Use default".
//! 색 override 행과 같은 형태로, 글꼴 override 격자가 이 행을 쌓는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::field_error::{FieldErrorLine, paint_field_error_line};
use crate::toggle::checkbox_width;

/// override 한 행의 칸. 한 클로저가 세 칸을 차례로 그려 같은 override 필드를 빌린다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverrideCell {
    Label,
    Control,
    UseDefault,
}

/// 체크박스가 컨트롤 오른쪽에 들어가는지. 들어가지 않으면 컨트롤 아래 줄로 내린다.
pub fn override_row_fits(
    theme: &Theme,
    label_col: LogicalPx,
    available: LogicalPx,
    control_w: LogicalPx,
    check_w: LogicalPx,
) -> bool {
    let gap = theme.settings_label_gap();
    label_col + gap + control_w + gap + check_w <= available
}

/// override 한 행의 배치.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverrideLayout {
    /// 라벨 · 컨트롤 · 체크박스가 한 줄이다.
    Wide,
    /// 체크박스가 컨트롤 아래 줄로 내려간다.
    Dropped,
    /// 컨트롤도 라벨 옆에 들어가지 않아 행 시작으로 내려가고 체크박스가 그 아래다.
    Stacked,
}

/// 남은 폭에서 행 배치를 정한다.
pub fn override_row_layout(
    theme: &Theme,
    label_col: LogicalPx,
    available: LogicalPx,
    control_w: LogicalPx,
    check_w: LogicalPx,
) -> OverrideLayout {
    if override_row_fits(theme, label_col, available, control_w, check_w) {
        OverrideLayout::Wide
    } else if label_col + theme.settings_label_gap() + control_w <= available {
        OverrideLayout::Dropped
    } else {
        OverrideLayout::Stacked
    }
}

/// override 한 행을 그린다. 라벨 칸은 `label_col` 폭(보통 [`crate::settings_label_column`] 의 값)이고,
/// 라벨은 그 칸 안에 [`crate::settings_label_cell`] 로 그린다. 칸 사이 `settings-label-gap`, 위아래 `space-xs`, 최소 높이
/// `settings-row-min-height`. 체크박스가 남은 폭에 들어가지 않으면 컨트롤 아래 줄로 내려가며
/// 줄 사이는 `space-xs`다. `check_label` 은 체크박스 폭을 재는 데만 쓴다.
pub fn override_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    control_w: LogicalPx,
    check_label: &str,
    mut cell: impl FnMut(&mut egui::Ui, OverrideCell),
) {
    override_row_with_error(
        ui,
        theme,
        label_col,
        control_w,
        check_label,
        |ui, which, _| {
            cell(ui, which);
        },
    );
}

/// [`override_row`] 에 오류 줄을 더한다. `OverrideCell::Control` 칸이 세 번째 인자에 넣은 줄을 컨트롤
/// 줄 바로 아래에 그리므로 체크박스가 내려가면 순서는 컨트롤, 줄, 체크박스다. 다른 칸이 넣은 줄은 쓰지 않는다.
pub fn override_row_with_error<'a>(
    ui: &mut egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    control_w: LogicalPx,
    check_label: &str,
    mut cell: impl FnMut(&mut egui::Ui, OverrideCell, &mut Option<FieldErrorLine<'a>>),
) {
    let label_w = label_col;
    let gap = theme.settings_label_gap();
    let row_h = theme.settings_row_min_height().value();
    let check_w = LogicalPx(checkbox_width(ui, theme, check_label));
    let layout = override_row_layout(
        theme,
        label_w,
        LogicalPx(ui.available_width()),
        control_w,
        check_w,
    );
    let indent = match layout {
        OverrideLayout::Stacked => LogicalPx(0.0),
        _ => label_w + gap,
    };
    let pad_y = theme.spacing_xs.value().round() as i8;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            top: pad_y,
            bottom: pad_y,
            ..egui::Margin::ZERO
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
            let mut line = None;
            let label = |ui: &mut egui::Ui,
                         cell: &mut dyn FnMut(
                &mut egui::Ui,
                OverrideCell,
                &mut Option<FieldErrorLine<'a>>,
            )| {
                ui.allocate_ui_with_layout(
                    egui::vec2(label_w.value(), row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(label_w.value());
                        cell(ui, OverrideCell::Label, &mut None);
                    },
                );
            };
            if layout == OverrideLayout::Stacked {
                ui.horizontal(|ui| {
                    ui.set_min_height(row_h);
                    label(ui, &mut cell);
                });
            }
            ui.horizontal(|ui| {
                ui.set_min_height(row_h);
                ui.spacing_mut().item_spacing.x = gap.value();
                if layout != OverrideLayout::Stacked {
                    label(ui, &mut cell);
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(control_w.value(), row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| cell(ui, OverrideCell::Control, &mut line),
                );
                if layout == OverrideLayout::Wide {
                    cell(ui, OverrideCell::UseDefault, &mut None);
                }
            });
            if let Some(line) = line {
                ui.add_space(theme.settings_row_caption_gap().value() - theme.spacing_xs.value());
                paint_field_error_line(ui, theme, indent, line);
            }
            if layout != OverrideLayout::Wide {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(indent.value());
                    cell(ui, OverrideCell::UseDefault, &mut None);
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checkbox_drops_under_the_control_only_when_it_does_not_fit() {
        let th = tasty_themes::mocha_fallback();
        let control = th.field_width_lg;
        let check = LogicalPx(100.0);
        let gap = th.settings_label_gap();
        for col in [th.settings_label_width(), th.settings_label_max_width()] {
            let exact = col + gap + control + gap + check;
            assert!(override_row_fits(&th, col, exact, control, check));
            assert!(!override_row_fits(
                &th,
                col,
                exact - LogicalPx(1.0),
                control,
                check
            ));
        }
    }

    #[test]
    fn the_layout_stacks_only_when_the_control_does_not_fit_beside_the_label() {
        let th = tasty_themes::mocha_fallback();
        let control = th.field_width_lg;
        let check = LogicalPx(100.0);
        let col = th.settings_label_width();
        let gap = th.settings_label_gap();
        let wide = col + gap + control + gap + check;
        let beside = col + gap + control;
        let layout = |w| override_row_layout(&th, col, w, control, check);
        assert_eq!(layout(wide), OverrideLayout::Wide);
        assert_eq!(layout(wide - LogicalPx(1.0)), OverrideLayout::Dropped);
        assert_eq!(layout(beside), OverrideLayout::Dropped);
        assert_eq!(layout(beside - LogicalPx(1.0)), OverrideLayout::Stacked);
    }

    /// 칸을 그린 위쪽 경계를 돌려준다. 체크박스 칸은 오류 줄 뒤에 그려져야 한다.
    fn cell_tops(width: f32) -> (f32, f32, f32) {
        let th = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let col = th.settings_label_width();
        let (mut control, mut check, mut text) = (0.0, 0.0, 0.0);
        for _ in 0..3 {
            let out = ctx.run(Default::default(), |ctx| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        ui.set_max_width(width);
                        override_row_with_error(
                            ui,
                            &th,
                            col,
                            th.field_width_lg,
                            "Use default",
                            |ui, which, line| match which {
                                OverrideCell::Label => {}
                                OverrideCell::Control => {
                                    control = ui.cursor().top();
                                    *line = Some(FieldErrorLine {
                                        text: "bad",
                                        ink: true,
                                    });
                                }
                                OverrideCell::UseDefault => {
                                    check = ui.cursor().top();
                                }
                            },
                        );
                    });
            });
            for clipped in &out.shapes {
                if let egui::Shape::Text(t) = &clipped.shape
                    && t.galley.text() == "bad"
                {
                    text = t.pos.y;
                }
            }
        }
        (control, text, check)
    }

    #[test]
    fn a_dropped_checkbox_sits_under_the_error_line() {
        let (control, text, check) = cell_tops(400.0);
        assert!(control < text && text < check, "{control} {text} {check}");
    }

    #[test]
    fn a_wide_checkbox_stays_on_the_control_line_above_the_error_line() {
        let (control, text, check) = cell_tops(1000.0);
        assert!(check < text, "{control} {text} {check}");
    }
}
