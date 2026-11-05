use super::*;

pub struct NotificationSummary<'a> {
    pub counts: [usize; 3],
    pub label: &'a str,
    pub tooltip: &'a str,
    pub level_labels: [&'a str; 3],
    pub latest: Option<(NotificationTone, &'a str, &'a str)>,
    pub open: bool,
}

pub fn notification_summary(
    ui: &mut egui::Ui,
    th: &Theme,
    props: &NotificationSummary<'_>,
    rail: bool,
) -> egui::Response {
    let any = props.counts.iter().any(|n| *n > 0);
    let badge = badges_size(ui, th, props.counts, rail);
    let pad = th.notification_summary_pad().value();
    let glyph = th.icon_glyph_size_md.value();
    let preview = any && props.latest.is_some() && !rail;
    let preview_h = th.notification_summary_preview_font_size().value() * th.line_height_ui;
    let height = if rail {
        if any {
            th.spacing_xs.value() * 2.0 + glyph + th.spacing_xs.value() + badge.y
        } else {
            th.item_height_interactive.value()
        }
    } else {
        pad * 2.0
            + th.notification_summary_row_height().value()
            + if preview {
                preview_h + th.spacing_xs.value()
            } else {
                0.0
            }
    };
    let width = if rail {
        th.notification_rail_width().value()
    } else {
        ui.available_width()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let button = if rail { rect } else { rect.shrink(pad) };
    if !rail {
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            egui::Stroke::new(
                th.border_width.value(),
                th.separator.to_egui_premultiplied(),
            ),
        );
    }
    if props.open || response.hovered() {
        let color = if props.open {
            th.notification_summary_bg_open()
        } else {
            th.notification_summary_bg_hover()
        };
        ui.painter().rect_filled(
            button,
            th.corner_radius.value(),
            color.to_egui_premultiplied(),
        );
    }
    // 채움이 켜지면 글자와 글리프는 그 칸을 통과하는 raised 잉크로 바뀐다.
    let lit = props.open || response.hovered();
    let label_ink = if lit {
        th.text_secondary_raised()
    } else {
        th.notification_summary_label_fg()
    };
    let meta_ink = if lit {
        th.text_muted_raised()
    } else {
        th.notification_summary_meta_fg()
    };
    let fg = if any {
        label_ink
    } else {
        th.notification_summary_label_fg_empty()
    }
    .to_egui();
    if rail {
        let center = egui::pos2(
            button.center().x,
            if any {
                button.top() + th.spacing_xs.value() + glyph * 0.5
            } else {
                button.center().y
            },
        );
        tasty_icons::BELL.image(glyph, fg).paint_at(
            ui,
            egui::Rect::from_center_size(center, egui::vec2(glyph, glyph)),
        );
        let badges = egui::Rect::from_min_size(
            egui::pos2(
                button.left(),
                center.y + glyph * 0.5 + th.spacing_xs.value(),
            ),
            egui::vec2(button.width(), badge.y),
        );
        paint_badges(ui, th, badges, props.counts, props.level_labels, true);
    } else {
        let inner = button.shrink2(egui::vec2(th.notification_summary_pad_x().value(), 0.0));
        let y = inner.top() + th.notification_summary_row_height().value() * 0.5;
        let icon = egui::Rect::from_center_size(
            egui::pos2(inner.left() + glyph * 0.5, y),
            egui::vec2(glyph, glyph),
        );
        tasty_icons::BELL.image(glyph, fg).paint_at(ui, icon);
        let x = icon.right() + th.notification_summary_icon_gap().value();
        let badges = egui::Rect::from_min_size(
            egui::pos2(inner.right() - badge.x, y - badge.y * 0.5),
            badge,
        );
        paint_badges(ui, th, badges, props.counts, props.level_labels, false);
        let title = text_job(
            ui,
            th,
            props.label,
            th.font_size_body,
            fg,
            LogicalPx((badges.left() - x - th.spacing_xs.value()).max(0.0)),
            1,
        );
        ui.painter()
            .galley(egui::pos2(x, y - title.size().y * 0.5), title, fg);
        if preview && let Some((tone, title, time)) = props.latest {
            let y = inner.top() + th.notification_summary_row_height().value() + preview_h * 0.5;
            let glyph = th.icon_glyph_size_xs.value();
            let icon = egui::Rect::from_center_size(
                egui::pos2(x + glyph * 0.5, y),
                egui::vec2(glyph, glyph),
            );
            tone.icon().image(glyph, meta_ink.into()).paint_at(ui, icon);
            let clock = ui.painter().layout_no_wrap(
                time.into(),
                egui::FontId::monospace(th.font_size_micro.value()),
                meta_ink.into(),
            );
            let tx = inner.right() - clock.size().x;
            ui.painter().galley(
                egui::pos2(tx, y - clock.size().y * 0.5),
                clock,
                meta_ink.into(),
            );
            let x = icon.right() + th.spacing_xs.value();
            let title = text_job(
                ui,
                th,
                title,
                th.notification_summary_preview_font_size(),
                label_ink.into(),
                LogicalPx((tx - th.spacing_xs.value() - x).max(0.0)),
                1,
            );
            ui.painter().galley(
                egui::pos2(x, y - title.size().y * 0.5),
                title,
                label_ink.into(),
            );
        }
    }
    response.on_hover_text(props.tooltip)
}
