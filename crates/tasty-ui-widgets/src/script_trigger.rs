//! Settings › Misc › Scripts의 자동 실행 줄 끝에 두는 Add trigger… 컨트롤과 남은 이벤트 메뉴.
//! 컨트롤은 1px 점선 테두리(border-dash / border-dash-gap)이고, 모든 이벤트가 이미 걸려 있으면
//! 숨기지 않고 disabled로 둔다. 메뉴는 menu 틀 안에 mono micro 이벤트 행을 쌓는다.
//! 본체와 갤러리가 함께 호출한다. 이벤트 목록과 문구는 호출부가 넘긴다.

use tasty_type_appearance::theme::Theme;

use crate::dashed_edge::paint_dashed_outline;

/// Add trigger… 컨트롤. `height`는 같은 줄의 트리거 칩 높이이고 `open`이면 overlay-active로 채운다.
/// disabled면 state-disabled-fg 글자로 그리고 클릭을 받지 않는다. 툴팁은 호출부가 붙인다.
pub fn script_trigger_add_control(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    height: f32,
    enabled: bool,
    open: bool,
) -> egui::Response {
    let fg = if enabled {
        theme.text_muted_raised()
    } else {
        theme.state_disabled_fg()
    }
    .to_egui();
    let glyph = theme.icon_glyph_size_xs.value();
    let gap = theme.spacing_xs.value();
    let pad_x = theme.spacing_xs.value();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        crate::label_font(theme.font_size_micro.value()),
        fg,
    );
    let w = pad_x * 2.0 + galley.rect.width() + gap + glyph;
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, height), sense);
    let radius = theme.corner_radius_sm.value();
    if open && enabled {
        ui.painter()
            .rect_filled(rect, radius, theme.overlay_active().to_egui_premultiplied());
    }
    paint_dashed_outline(
        ui.painter(),
        theme,
        rect,
        radius,
        theme.border_default().to_egui(),
    );
    let pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, fg);
    let gr = egui::Rect::from_min_size(
        egui::pos2(rect.right() - pad_x - glyph, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    tasty_icons::CHEVRON_DOWN.image(glyph, fg).paint_at(ui, gr);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// 남은 이벤트 메뉴의 내용. 폭은 최소 `trigger_menu_min_width`, 높이는 `trigger_menu_max_height`를
/// 넘으면 스크롤한다. 행은 menu-item-height · 좌우 space-sm · mono micro text-secondary이고
/// hover에서 overlay-hover · text-primary다. 고른 이벤트를 돌려준다.
/// menu 틀(배경·테두리·반경·안쪽 여백)은 호출부의 popup 또는 [`script_trigger_menu_frame`]이 그린다.
pub fn script_trigger_menu<'a>(
    ui: &mut egui::Ui,
    theme: &Theme,
    events: &[&'a str],
) -> Option<&'a str> {
    let font = egui::FontId::monospace(theme.font_size_micro.value());
    let pad_x = theme.spacing_sm.value();
    // 행 폭은 가장 긴 이벤트에 좌우 여백을 더한 값이며 메뉴 폭 하한보다 좁지 않다.
    let width = events
        .iter()
        .map(|ev| {
            ui.painter()
                .layout_no_wrap((*ev).to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .rect
                .width()
                + pad_x * 2.0
        })
        .fold(theme.trigger_menu_min_width().value(), f32::max);
    let mut picked = None;
    let row_h = theme.menu_item_height().value();
    let max_h = theme.trigger_menu_max_height().value();
    // 부모의 남은 높이가 짧아도 상한까지는 행을 다 보인다. 상한을 넘을 때만 스크롤한다.
    let content_h = row_h * events.len() as f32;
    egui::ScrollArea::vertical()
        .max_height(max_h)
        .min_scrolled_height(content_h.min(max_h))
        .drag_to_scroll(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for ev in events {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(width, row_h), egui::Sense::click());
                let hovered = resp.hovered();
                if hovered {
                    ui.painter().rect_filled(
                        rect,
                        theme.corner_radius_sm.value(),
                        theme.overlay_hover().to_egui_premultiplied(),
                    );
                }
                let fg = if hovered {
                    theme.menu_item_fg_hover()
                } else {
                    theme.menu_item_fg()
                };
                ui.painter().text(
                    egui::pos2(rect.left() + pad_x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    *ev,
                    font.clone(),
                    fg.to_egui(),
                );
                if resp
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    picked = Some(*ev);
                }
            }
        });
    picked
}

/// popup 없이 메뉴를 펼친 모습을 그릴 때 쓰는 menu 틀. 배경 menu-bg, 1px menu-border,
/// 반경 menu-radius, 그림자 shadow-popover, 안쪽 여백 popup-content-margin이다.
pub fn script_trigger_menu_frame<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.menu_border().to_egui(),
        ))
        .corner_radius(theme.menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(
            theme.popup_content_margin().value() as i8
        ))
        .show(ui, add)
        .inner
}
