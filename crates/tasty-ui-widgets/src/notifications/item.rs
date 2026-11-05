use super::*;
use crate::{Button, ButtonVariant, ControlSize, IconButton};

pub(super) struct ItemPlan {
    title: std::sync::Arc<egui::Galley>,
    body: std::sync::Arc<egui::Galley>,
    pub height: LogicalPx,
    cut: bool,
    content_width: LogicalPx,
}

pub(super) fn plan(
    ui: &egui::Ui,
    th: &Theme,
    row: &NotificationRow<'_>,
    width: LogicalPx,
    expanded: bool,
    wide: bool,
) -> ItemPlan {
    let content_width = LogicalPx(
        (width.value()
            - th.notification_item_pad_x().value() * 2.0
            - th.notification_item_glyph_size().value()
            - th.notification_item_glyph_gap().value())
        .max(0.0),
    );
    let time = ui.painter().layout_no_wrap(
        row.time.into(),
        egui::FontId::monospace(th.font_size_micro.value()),
        th.notification_item_meta_fg().into(),
    );
    let title_width = LogicalPx(
        (content_width.value()
            - time.size().x
            - th.spacing_sm.value() * 2.0
            - th.notification_item_unread_dot_size().value())
        .max(0.0),
    );
    let title_color = if row.unread {
        th.notification_item_title_fg()
    } else {
        th.notification_item_title_fg_read()
    }
    .into();
    let title = text_job(
        ui,
        th,
        row.title,
        th.font_size_body,
        title_color,
        title_width,
        if expanded { usize::MAX } else { 1 },
    );
    let body_color = if row.unread {
        th.notification_item_body_fg()
    } else {
        th.notification_item_body_fg_read()
    }
    .into();
    let body = text_job(
        ui,
        th,
        row.body,
        th.font_size_caption,
        body_color,
        content_width,
        if expanded {
            usize::MAX
        } else if wide {
            3
        } else {
            2
        },
    );
    let cut = body.elided;
    let body_h = if row.body.is_empty() {
        0.0
    } else {
        body.size().y + th.notification_item_line_gap().value()
    };
    let more_h = if !row.body.is_empty() && (cut || expanded) {
        th.font_size_caption.value() * th.line_height_ui + th.notification_item_line_gap().value()
    } else {
        0.0
    };
    let height = LogicalPx(
        th.notification_item_pad_y().value() * 2.0
            + title
                .size()
                .y
                .max(th.font_size_body.value() * th.line_height_ui)
            + body_h
            + more_h
            + th.notification_item_line_gap().value()
            + th.button_height_sm().value()
            + th.border_width.value(),
    );
    ItemPlan {
        title,
        body,
        height,
        cut,
        content_width,
    }
}

/// hover는 바탕에 overlay를 얹으므로 안 읽은 본문을 `text-secondary-raised`로 다시 배치한다.
/// 계획 단계의 배치는 hover 여부를 모른다. 읽은 본문의 잉크는 hover에도 그대로다.
fn body_galley(
    ui: &egui::Ui,
    th: &Theme,
    row: &NotificationRow<'_>,
    plan: &ItemPlan,
    hovered: bool,
) -> std::sync::Arc<egui::Galley> {
    if !hovered || !row.unread {
        return plan.body.clone();
    }
    let mut job = (*plan.body.job).clone();
    for section in &mut job.sections {
        section.format.color = th.text_secondary_raised().to_egui();
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}

pub(super) fn draw(
    ui: &mut egui::Ui,
    th: &Theme,
    row: &NotificationRow<'_>,
    labels: &NotificationLabels<'_>,
    plan: &ItemPlan,
    expanded: bool,
) -> Option<NotificationAction> {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), plan.height.value()),
        egui::Sense::hover(),
    );
    let response = ui.interact(rect, ui.id().with(row.id), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.0,
            th.notification_item_bg_hover().to_egui_premultiplied(),
        );
    }
    let top = rect.top() + th.notification_item_pad_y().value();
    let glyph = th.notification_item_glyph_size().value();
    let x = rect.left() + th.notification_item_pad_x().value();
    row.tone
        .icon()
        .image(glyph, row.tone.glyph_color(th))
        .paint_at(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(
                    x,
                    top + (th.font_size_body.value() * th.line_height_ui - glyph) * 0.5,
                ),
                egui::vec2(glyph, glyph),
            ),
        );
    let x = x + glyph + th.notification_item_glyph_gap().value();
    let title_color = if row.unread {
        th.notification_item_title_fg()
    } else {
        th.notification_item_title_fg_read()
    }
    .into();
    ui.painter()
        .galley(egui::pos2(x, top), plan.title.clone(), title_color);
    let right = rect.right() - th.notification_item_pad_x().value();
    let dot = th.notification_item_unread_dot_size().value();
    let y = top + th.font_size_body.value() * th.line_height_ui * 0.5;
    if row.unread {
        ui.painter().circle_filled(
            egui::pos2(right - dot * 0.5, y),
            dot * 0.5,
            th.notification_item_unread_dot(),
        );
    }
    ui.painter().text(
        egui::pos2(right - dot - th.spacing_sm.value(), y),
        egui::Align2::RIGHT_CENTER,
        row.time,
        egui::FontId::monospace(th.font_size_micro.value()),
        th.notification_item_meta_fg().into(),
    );
    let mut y = top
        + plan
            .title
            .size()
            .y
            .max(th.font_size_body.value() * th.line_height_ui)
        + th.notification_item_line_gap().value();
    if !row.body.is_empty() {
        let body = body_galley(ui, th, row, plan, response.hovered());
        if expanded {
            let mut body_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("notification_body", row.id))
                    .max_rect(egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        plan.body.size(),
                    )),
            );
            body_ui.add(egui::Label::new(body).selectable(true));
        } else {
            ui.painter().galley(
                egui::pos2(x, y),
                body,
                th.notification_item_body_fg().into(),
            );
        }
        y += plan.body.size().y + th.notification_item_line_gap().value();
    }
    let mut action = None;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("notification_actions", row.id))
            .max_rect(egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(plan.content_width.value(), rect.bottom() - y),
            )),
    );
    child.spacing_mut().item_spacing.y = th.notification_item_line_gap().value();
    if !row.body.is_empty()
        && (plan.cut || expanded)
        && child
            .add(
                egui::Label::new(
                    egui::RichText::new(if expanded { labels.less } else { labels.more })
                        .size(th.font_size_caption.value())
                        .color(th.notification_item_more_fg()),
                )
                .sense(egui::Sense::click()),
            )
            .clicked()
    {
        action = Some(NotificationAction::Toggle(row.id));
    }
    child.allocate_ui_with_layout(
        egui::vec2(plan.content_width.value(), th.button_height_sm().value()),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if Button::new(labels.jump)
                .size(ControlSize::Sm)
                .variant(ButtonVariant::Ghost)
                .enabled(!row.source_closed)
                .show(ui, th)
                .on_hover_text(if row.source_closed {
                    labels.source_closed
                } else {
                    row.source
                })
                .clicked()
            {
                action = Some(NotificationAction::Jump(row.id));
            }
            if row.unread
                && IconButton::new()
                    .size(ControlSize::Sm)
                    .show(ui, th, &|ui, rect, color| {
                        tasty_icons::CHECK
                            .image(rect.width(), color)
                            .paint_at(ui, rect)
                    })
                    .on_hover_text(labels.mark_read)
                    .clicked()
            {
                action = Some(NotificationAction::Read(row.id));
            }
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), th.button_height_sm().value()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let closed_width = if row.source_closed {
                        ui.painter()
                            .layout_no_wrap(
                                labels.closed.into(),
                                egui::FontId::proportional(th.font_size_micro.value()),
                                th.notification_item_closed_fg().into(),
                            )
                            .size()
                            .x
                            + th.spacing_xs.value()
                    } else {
                        0.0
                    };
                    let natural_width = ui
                        .painter()
                        .layout_no_wrap(
                            row.source.into(),
                            egui::FontId::monospace(th.font_size_micro.value()),
                            th.notification_item_meta_fg().into(),
                        )
                        .size()
                        .x;
                    let (source_rect, response) = ui.allocate_exact_size(
                        egui::vec2(
                            (ui.available_width() - closed_width)
                                .max(0.0)
                                .min(natural_width),
                            th.button_height_sm().value(),
                        ),
                        egui::Sense::hover(),
                    );
                    let source = ui.painter().layout_job(egui::text::LayoutJob {
                        text: row.source.into(),
                        sections: vec![egui::text::LayoutSection {
                            leading_space: 0.0,
                            byte_range: 0..row.source.len(),
                            format: egui::TextFormat {
                                font_id: egui::FontId::monospace(th.font_size_micro.value()),
                                color: th.notification_item_meta_fg().into(),
                                ..Default::default()
                            },
                        }],
                        wrap: egui::text::TextWrapping {
                            max_width: source_rect.width(),
                            max_rows: 1,
                            break_anywhere: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                    ui.painter().galley(
                        egui::pos2(
                            source_rect.left(),
                            source_rect.center().y - source.size().y * 0.5,
                        ),
                        source,
                        th.notification_item_meta_fg().into(),
                    );
                    response.on_hover_text(row.source);
                    if row.source_closed {
                        ui.label(
                            egui::RichText::new(labels.closed)
                                .size(th.font_size_micro.value())
                                .color(th.notification_item_closed_fg()),
                        );
                    }
                },
            );
        },
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.notification_item_separator().to_egui_premultiplied(),
        ),
    );
    let response = response.on_hover_text(format!(
        "{}: {}",
        labels.levels[row.tone.index()],
        row.title
    ));
    action.or_else(|| {
        response
            .clicked()
            .then_some(NotificationAction::Toggle(row.id))
    })
}
