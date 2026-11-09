//! Remote HTML receive decisions. Host and gallery share the received banner layout.
use crate::{Button, ButtonVariant, ControlSize, Ground, TagVariant, banner_shell};
use std::sync::Arc;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

pub enum HtmlSizeDetail<'a> {
    File {
        path: &'a str,
        size: &'a str,
        waiting: Option<&'a str>,
    },
    Limit {
        body: &'a str,
    },
}
pub struct HtmlSizeBannerView<'a> {
    pub title: &'a str,
    pub detail: HtmlSizeDetail<'a>,
    pub decline: &'a str,
    pub allow: &'a str,
    pub narrow: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HtmlSizeAction {
    #[default]
    None,
    Allow,
    Decline,
}
pub struct HtmlSizeBannerOutput {
    pub rect: egui::Rect,
    pub action: HtmlSizeAction,
}

fn text(
    ui: &egui::Ui,
    theme: &Theme,
    value: &str,
    size: LogicalPx,
    width: LogicalPx,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        value.into(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size.value()),
            color,
            line_height: Some(size.value() * theme.line_height_ui),
            ..Default::default()
        },
    );
    job.wrap.max_width = width.value();
    ui.painter().layout_job(job)
}
fn path_tail(ui: &egui::Ui, theme: &Theme, path: &str, width: LogicalPx) -> String {
    let font = egui::FontId::monospace(theme.banner_body_font_size().value());
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.into(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
            <= width.value()
    };
    if fits(path) {
        return path.into();
    }
    let bounds: Vec<_> = path
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(path.len()))
        .collect();
    let mut lo = 0;
    let mut hi = bounds.len() - 1;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(&format!("…{}", &path[bounds[mid]..])) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    format!("…{}", &path[bounds[lo]..])
}
fn buttons(ui: &mut egui::Ui, theme: &Theme, view: &HtmlSizeBannerView<'_>) -> HtmlSizeAction {
    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let decline = Button::new(view.decline)
        .size(ControlSize::Sm)
        .variant(ButtonVariant::Ghost)
        .ground(Ground::Raised)
        .show(ui, theme)
        .clicked();
    let allow = Button::new(view.allow)
        .size(ControlSize::Sm)
        .variant(ButtonVariant::Secondary)
        .show(ui, theme)
        .clicked();
    if decline {
        HtmlSizeAction::Decline
    } else if allow {
        HtmlSizeAction::Allow
    } else {
        HtmlSizeAction::None
    }
}
fn detail(ui: &mut egui::Ui, theme: &Theme, view: &HtmlSizeBannerView<'_>, rect: egui::Rect) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    match &view.detail {
        HtmlSizeDetail::Limit { body } => {
            let galley = text(
                ui,
                theme,
                body,
                theme.banner_body_font_size(),
                LogicalPx(rect.width()),
                theme.banner_body_fg().to_egui(),
            );
            child
                .painter()
                .galley(rect.min, galley, egui::Color32::PLACEHOLDER);
        }
        HtmlSizeDetail::File {
            path,
            size,
            waiting,
        } => {
            let size_w = crate::chip::tag_mono_width(ui, theme, size);
            let waiting_w = waiting.map_or(0.0, |s| {
                text(
                    ui,
                    theme,
                    s,
                    theme.banner_body_font_size(),
                    LogicalPx(f32::INFINITY),
                    theme.banner_body_fg().to_egui(),
                )
                .size()
                .x + theme.spacing_sm.value()
            });
            let path_w =
                LogicalPx((rect.width() - size_w - waiting_w - theme.spacing_sm.value()).max(0.0));
            let label = path_tail(ui, theme, path, path_w);
            let (path_rect, _) = child.allocate_exact_size(
                egui::vec2(path_w.value(), rect.height()),
                egui::Sense::hover(),
            );
            child.painter().text(
                path_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::monospace(theme.banner_body_font_size().value()),
                theme.banner_body_fg().to_egui(),
            );
            crate::chip::tag_mono(&mut child, theme, size, TagVariant::Warning, false);
            if let Some(waiting) = waiting {
                child.label(
                    egui::RichText::new(*waiting)
                        .size(theme.banner_body_font_size().value())
                        .color(theme.banner_body_fg().to_egui()),
                );
            }
        }
    }
}
pub fn html_size_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlSizeBannerView<'_>,
) -> HtmlSizeBannerOutput {
    let mut action = HtmlSizeAction::None;
    let rect = banner_shell(ui, theme, 1.0, |ui| {
        let glyph = theme.icon_glyph_size_md;
        let gap = theme.banner_gap();
        let width = LogicalPx(ui.available_width());
        let actions_w = Button::new(view.decline)
            .size(ControlSize::Sm)
            .desired_width(ui, theme)
            + theme.spacing_sm
            + Button::new(view.allow)
                .size(ControlSize::Sm)
                .desired_width(ui, theme);
        let text_w = LogicalPx(
            (width
                - glyph
                - gap
                - if view.narrow {
                    LogicalPx(0.0)
                } else {
                    gap + actions_w
                })
            .value()
            .max(0.0),
        );
        let title = text(
            ui,
            theme,
            view.title,
            theme.banner_title_font_size(),
            text_w,
            theme.banner_fg().to_egui(),
        );
        let detail_h = match &view.detail {
            HtmlSizeDetail::File { .. } => theme.tag_size().value(),
            HtmlSizeDetail::Limit { body } => {
                text(
                    ui,
                    theme,
                    body,
                    theme.banner_body_font_size(),
                    text_w,
                    theme.banner_body_fg().to_egui(),
                )
                .size()
                .y
            }
        };
        let body_h = title.size().y + theme.banner_text_gap().value() + detail_h;
        let button_h = ControlSize::Sm.height(theme);
        let first_h = body_h.max((glyph + theme.banner_glyph_offset()).value());
        let row_h = if view.narrow {
            first_h + gap.value() + button_h
        } else {
            first_h.max(button_h)
        };
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(width.value(), row_h), egui::Sense::hover());
        let (icon, color) = match view.detail {
            HtmlSizeDetail::File { .. } => (tasty_icons::DOWNLOAD, theme.html_size_banner_glyph()),
            HtmlSizeDetail::Limit { .. } => {
                (tasty_icons::ALERT_TRIANGLE, theme.html_limit_banner_glyph())
            }
        };
        let icon_rect = egui::Rect::from_min_size(
            row.min + egui::vec2(0.0, theme.banner_glyph_offset().value()),
            egui::vec2(glyph.value(), glyph.value()),
        );
        icon.image(glyph.value(), color.to_egui())
            .paint_at(ui, icon_rect);
        let origin = row.min
            + egui::vec2(
                (glyph + gap).value(),
                if view.narrow {
                    0.0
                } else {
                    (row_h - body_h) / 2.0
                },
            );
        let title_h = title.size().y;
        ui.painter()
            .galley(origin, title, theme.banner_fg().to_egui());
        detail(
            ui,
            theme,
            view,
            egui::Rect::from_min_size(
                origin + egui::vec2(0.0, title_h + theme.banner_text_gap().value()),
                egui::vec2(text_w.value(), detail_h),
            ),
        );
        let buttons_at = if view.narrow {
            row.min + egui::vec2((glyph + gap).value(), first_h + gap.value())
        } else {
            egui::pos2(
                row.right() - actions_w.value(),
                row.top() + (row_h - button_h) / 2.0,
            )
        };
        let mut child =
            crate::banner::action_slot(ui, buttons_at, egui::vec2(actions_w.value(), button_h));
        action = buttons(&mut child, theme, view);
    });
    HtmlSizeBannerOutput { rect, action }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_buttons_move_below_the_text_and_keep_the_banner_button_box() {
        for narrow in [false, true] {
            let theme =
                Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
            let ctx = egui::Context::default();
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(if narrow { 320.0 } else { 560.0 });
                    html_size_banner(
                        ui,
                        &theme,
                        &HtmlSizeBannerView {
                            title: "The remote wants to send a large file",
                            detail: HtmlSizeDetail::File {
                                path: "/remote/report.html",
                                size: "300 KB",
                                waiting: None,
                            },
                            decline: "Decline",
                            allow: "Allow",
                            narrow,
                        },
                    );
                });
            });
            let at = |label: &str| {
                out.shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
                        _ => None,
                    })
                    .unwrap()
            };
            assert_eq!(at("Allow").y, at("Decline").y);
            assert!(at("Allow").x > at("Decline").x);
            if narrow {
                assert!(at("Decline").y > at("/remote/report.html").y);
            }
            assert!(out.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == theme.banner_button_bg().to_egui())));
        }
    }
    /// 배너 셸은 surface-raised 다. Decline(Stop) ghost는 raised 바탕 글자색을 읽는다.
    #[test]
    fn the_decline_ghost_reads_the_raised_ink() {
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).unwrap();
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        let theme = Theme::with_colors(colors, is_light.unwrap_or(true));
        assert_ne!(theme.button_ghost_raised_fg(), theme.button_ghost_fg());
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(560.0);
                html_size_banner(
                    ui,
                    &theme,
                    &HtmlSizeBannerView {
                        title: "The remote wants to send a large file",
                        detail: HtmlSizeDetail::Limit { body: "Limit" },
                        decline: "Stop",
                        allow: "Continue",
                        narrow: false,
                    },
                );
            });
        });
        let ink = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Stop" => Some(t.fallback_color),
                _ => None,
            })
            .expect("Stop drawn");
        assert_eq!(ink, theme.button_ghost_raised_fg().to_egui());
    }

    #[test]
    fn left_ellipsis_keeps_unicode_tail_at_a_character_boundary() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let ctx = egui::Context::default();
        let mut result = String::new();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                result = path_tail(
                    ui,
                    &theme,
                    "/서버/長いフォルダ/보고서.html",
                    LogicalPx(100.0),
                );
            });
        });
        assert!(result.starts_with('…'));
        assert!(result.ends_with(".html"));
    }
}
