//! 입력 필드의 아이콘·단위·글꼴·오류·비활성·읽기 전용 상태 예제.
//! 편집 내용은 예제마다 별도 버퍼에 보관한다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Ground, Input};

use super::glyph;
use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, stage};

thread_local! {
    // 예제 초깃값이 입력 중인 내용을 덮지 않도록 한 번만 초기화한다.
    static BUFS: RefCell<Option<[String; 11]>> = const { RefCell::new(None) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    // 시안의 클러스터 폭(200 · 160 · 110)을 field-width 토큰(lg · md · color)으로 읽는다.
    let xs = theme.field_width_xs.value();
    let md = theme.field_width_md.value();
    let lg = theme.field_width_lg.value();
    let unit = theme.field_width_color.value();

    BUFS.with(|b| {
        let mut slot = b.borrow_mut();
        let bufs = slot.get_or_insert_with(|| {
            [
                String::new(),
                String::new(),
                "14".to_string(),
                "s_01HXK9".to_string(),
                "bad value".to_string(),
                String::new(),
                "#89b4fa".to_string(),
                "#89b4fa".to_string(),
                String::new(),
                "#89b4fa".to_string(),
                "#89b4fa".to_string(),
            ]
        });
        stage(ui, theme, StageVariant::Column, |ui| {
            cluster(
                ui,
                theme,
                "default · icon · addon — click to focus",
                |ui| {
                    Input::new().placeholder("Workspace name").width(lg).show(
                        ui,
                        theme,
                        &mut bufs[0],
                    );
                    Input::new()
                        .placeholder("Filter…")
                        .width(lg)
                        .icon(&|ui, rect, c| {
                            glyph::SEARCH.image(rect.height(), c).paint_at(ui, rect)
                        })
                        .show(ui, theme, &mut bufs[1]);
                    Input::new()
                        .mono(true)
                        .addon("px")
                        .width(unit)
                        .show(ui, theme, &mut bufs[2]);
                },
            );
            cluster(ui, theme, "mono · invalid · disabled", |ui| {
                Input::new()
                    .mono(true)
                    .width(lg)
                    .show(ui, theme, &mut bufs[3]);
                Input::new()
                    .invalid(true)
                    .width(md)
                    .show(ui, theme, &mut bufs[4]);
                Input::new()
                    .enabled(false)
                    .placeholder("Disabled")
                    .width(md)
                    .show(ui, theme, &mut bufs[5]);
            });
            // 읽기 전용은 disabled와 같은 상자에 값을 text-secondary로 둔다. 포커스·선택·복사가 된다.
            cluster(
                ui,
                theme,
                "readOnly vs disabled — same box, readable value",
                |ui| {
                    Input::new()
                        .mono(true)
                        .read_only(true)
                        .width(xs)
                        .show(ui, theme, &mut bufs[6]);
                    Input::new()
                        .mono(true)
                        .enabled(false)
                        .width(xs)
                        .show(ui, theme, &mut bufs[7]);
                },
            );
            // 시안 ground="raised": surface-raised 위에서는 상자가 bg-panel + border-control-raised로 내려선다.
            cluster(
                ui,
                theme,
                "ground=\"raised\" — on surface-raised, box steps down to bg-panel + border-control-raised",
                |ui| {
                    egui::Frame::new()
                        .fill(theme.surface_raised().to_egui())
                        .corner_radius(theme.corner_radius.value())
                        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                                Input::new()
                                    .ground(Ground::Raised)
                                    .placeholder("Editable")
                                    .width(md)
                                    .show(ui, theme, &mut bufs[8]);
                                Input::new()
                                    .ground(Ground::Raised)
                                    .mono(true)
                                    .read_only(true)
                                    .width(xs)
                                    .show(ui, theme, &mut bufs[9]);
                                Input::new()
                                    .ground(Ground::Raised)
                                    .mono(true)
                                    .enabled(false)
                                    .width(xs)
                                    .show(ui, theme, &mut bufs[10]);
                            });
                        });
                },
            );
        });
    });

    meta(
        ui,
        theme,
        &[
            ("height", "28 control-height"),
            ("border", "1px → focus 2px"),
            ("padding", "0 space-sm"),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "field fill",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::new(
                "border-focus",
                "focus ring",
                egui::Color32::from(theme.border_focus()),
            ),
            TokenChip::new(
                "accent-danger",
                "invalid edge",
                egui::Color32::from(theme.accent_danger()),
            ),
            TokenChip::without_color("text-placeholder", "hint"),
            TokenChip::new(
                "input-readonly-fg",
                "read-only value → text-secondary",
                egui::Color32::from(theme.input_readonly_fg()),
            ),
        ],
    );
}
