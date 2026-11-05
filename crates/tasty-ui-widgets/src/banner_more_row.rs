//! 배너 ⋯ 메뉴의 행. 라벨은 고정 문구(앞·뒤)와 프로그램 이름(mono) 두 부분이다.
//!
//! 고정 문구와 이름은 모두 행 글자색(쉼 `menu-item-fg`, 호버·active `menu-item-fg-hover`)이고 이름은
//! mono 글꼴로만 구분한다. 한 줄에 들어가지 않으면 이름만 끝 말줄임한다. 고정 문구에 이름 한 글자와
//! 말줄임표를 더한 폭도 들어가지 않으면(ja) 고정 문구를 줄이지 않고 그 행만 줄을 바꾼다. 줄을 바꾼 행은
//! 위아래 `menu-item-wrap-padding-y`, 줄 높이 `line-height-ui`, 최소 `menu-item-height` 이고 아이콘은
//! 세로 가운데다. 이름은 단어 중간에서 끊지 않고, 이름 혼자 한 줄보다 길 때만 말줄임한다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;

use crate::icon_button::IconPainter;

/// 행 라벨. 언어마다 이름의 위치가 달라 앞·뒤 고정 문구를 따로 받는다.
#[derive(Clone, Copy, Debug)]
pub struct BannerMoreLabel<'a> {
    pub prefix: &'a str,
    pub app: &'a str,
    pub suffix: &'a str,
}

/// 줄인 이름과 그 배치.
enum RowLayout {
    /// 한 줄 — 앞 문구 · 이름 · 뒤 문구 갤리.
    Line {
        prefix: Arc<egui::Galley>,
        app: Arc<egui::Galley>,
        suffix: Arc<egui::Galley>,
    },
    /// 줄을 바꾼 라벨 전체.
    Wrap(Arc<egui::Galley>),
}

struct Laid {
    layout: RowLayout,
    truncated: bool,
}

/// 아이콘 왼쪽 여백·아이콘·간격·오른쪽 여백을 뺀 라벨 폭.
fn label_width(theme: &Theme, row_width: f32) -> f32 {
    (row_width
        - theme.menu_item_padding_x().value() * 2.0
        - theme.icon_glyph_size_md.value()
        - theme.spacing_sm.value())
    .max(0.0)
}

fn no_wrap(fonts: &egui::epaint::Fonts, text: &str, font: egui::FontId) -> Arc<egui::Galley> {
    fonts.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
}

/// `max_w` 안에 들어가도록 이름 끝을 말줄임한다. 한 글자까지 줄여도 넘치면 한 글자 + 말줄임표다.
fn truncate_app(
    fonts: &egui::epaint::Fonts,
    app: &str,
    font: &egui::FontId,
    max_w: f32,
) -> (String, bool) {
    if no_wrap(fonts, app, font.clone()).size().x <= max_w {
        return (app.to_owned(), false);
    }
    let chars: Vec<char> = app.chars().collect();
    for n in (1..chars.len()).rev() {
        let candidate: String = chars[..n].iter().chain(std::iter::once(&'…')).collect();
        if no_wrap(fonts, &candidate, font.clone()).size().x <= max_w || n == 1 {
            return (candidate, true);
        }
    }
    (app.to_owned(), false)
}

/// 고정 문구에 이름 한 글자와 말줄임표를 더한 폭이 라벨 폭을 넘으면 줄을 바꾼다.
fn needs_wrap(fonts: &egui::epaint::Fonts, theme: &Theme, label: BannerMoreLabel, w: f32) -> bool {
    let body = theme.font_size_body.value();
    let fixed = no_wrap(fonts, label.prefix, egui::FontId::proportional(body))
        .size()
        .x
        + no_wrap(fonts, label.suffix, egui::FontId::proportional(body))
            .size()
            .x;
    let least_app = match label.app.chars().next() {
        Some(c) => {
            no_wrap(fonts, &format!("{c}…"), egui::FontId::monospace(body))
                .size()
                .x
        }
        None => 0.0,
    };
    fixed + least_app > w
}

fn lay_out(
    fonts: &egui::epaint::Fonts,
    theme: &Theme,
    label: BannerMoreLabel,
    row_width: f32,
    ink: egui::Color32,
    app_ink: egui::Color32,
) -> Laid {
    let body = theme.font_size_body.value();
    let w = label_width(theme, row_width);
    let mono = egui::FontId::monospace(body);
    if needs_wrap(fonts, theme, label, w) {
        // 이름은 혼자 한 줄보다 길 때만 줄인다. 이름 안의 공백은 줄바꿈 후보가 되지 않게 바꾼다.
        let (app, truncated) = truncate_app(fonts, label.app, &mono, w);
        let app = app.replace(' ', "\u{a0}");
        let line_height = Some(body * theme.line_height_ui);
        let format = |font_id: egui::FontId, color: egui::Color32| egui::TextFormat {
            font_id,
            color,
            line_height,
            ..Default::default()
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(
            label.prefix,
            0.0,
            format(egui::FontId::proportional(body), ink),
        );
        job.append(&app, 0.0, format(mono.clone(), app_ink));
        job.append(
            label.suffix,
            0.0,
            format(egui::FontId::proportional(body), ink),
        );
        job.wrap.max_width = w;
        return Laid {
            layout: RowLayout::Wrap(fonts.layout_job(job)),
            truncated,
        };
    }
    let prefix = no_wrap(fonts, label.prefix, egui::FontId::proportional(body));
    let suffix = no_wrap(fonts, label.suffix, egui::FontId::proportional(body));
    let (app, truncated) = truncate_app(
        fonts,
        label.app,
        &mono,
        (w - prefix.size().x - suffix.size().x).max(0.0),
    );
    Laid {
        layout: RowLayout::Line {
            prefix,
            app: no_wrap(fonts, &app, mono),
            suffix,
        },
        truncated,
    }
}

fn row_height(theme: &Theme, laid: &Laid) -> f32 {
    let min = theme.menu_item_height().value();
    match &laid.layout {
        RowLayout::Line { .. } => min,
        RowLayout::Wrap(g) => {
            (g.size().y + theme.menu_item_wrap_padding_y().value() * 2.0).max(min)
        }
    }
}

/// 한 줄로 줄이지 않고 그리는 데 필요한 행 폭. 메뉴 폭(min..max)을 정할 때 쓴다.
pub fn banner_more_row_natural_width(
    ctx: &egui::Context,
    theme: &Theme,
    label: BannerMoreLabel,
) -> f32 {
    let body = theme.font_size_body.value();
    let text_w = ctx.fonts(|f| {
        no_wrap(f, label.prefix, egui::FontId::proportional(body))
            .size()
            .x
            + no_wrap(f, label.app, egui::FontId::monospace(body))
                .size()
                .x
            + no_wrap(f, label.suffix, egui::FontId::proportional(body))
                .size()
                .x
    });
    theme.menu_item_padding_x().value() * 2.0
        + theme.icon_glyph_size_md.value()
        + theme.spacing_sm.value()
        + text_w
}

/// `row_width` 폭의 행 높이. 한 줄이면 `menu-item-height`, 줄을 바꾸면 그보다 클 수 있다.
pub fn banner_more_row_height(
    ctx: &egui::Context,
    theme: &Theme,
    label: BannerMoreLabel,
    row_width: f32,
) -> f32 {
    let laid = ctx.fonts(|f| {
        lay_out(
            f,
            theme,
            label,
            row_width,
            egui::Color32::PLACEHOLDER,
            egui::Color32::PLACEHOLDER,
        )
    });
    row_height(theme, &laid)
}

/// 행 하나를 가용 폭에 그린다. `active` 는 키보드 active(`surface-active`), `force_hover` 는 포인터
/// 없이 호버 모습을 보이는 상태 견본이다. `danger` 는 기록용으로 남긴 기각 변형이다. 이름을 줄였으면
/// 행 툴팁으로 전체 이름을 보인다.
pub fn banner_more_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: IconPainter<'_>,
    label: BannerMoreLabel,
    active: bool,
    force_hover: bool,
    danger: bool,
) -> egui::Response {
    let width = ui.available_width();
    // 높이는 색과 무관하므로 먼저 재고, 호버를 안 뒤 색을 넣어 다시 배치한다.
    let height = banner_more_row_height(ui.ctx(), theme, label, width);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let hovered = force_hover || resp.hovered();
    let radius = theme.menu_item_radius().value();
    if active {
        ui.painter()
            .rect_filled(rect, radius, theme.surface_active().to_egui());
    } else if hovered {
        ui.painter().rect_filled(
            rect,
            radius,
            theme.menu_item_bg_hover().to_egui_premultiplied(),
        );
    }
    let lit = active || hovered;
    let ink = match (danger, lit) {
        (true, _) if active => theme.menu_item_part_fg_active().to_egui(),
        (true, _) => theme.accent_danger().to_egui(),
        (false, true) => theme.menu_item_fg_hover().to_egui(),
        (false, false) => theme.menu_item_fg().to_egui(),
    };
    let app_ink = if lit {
        theme.banner_more_app_fg_hover().to_egui()
    } else {
        theme.banner_more_app_fg().to_egui()
    };
    let icon_ink = if active {
        theme.menu_item_part_fg_active().to_egui()
    } else if danger {
        theme.accent_danger().to_egui()
    } else {
        theme.text_muted().to_egui()
    };

    let pad_x = theme.menu_item_padding_x().value();
    let glyph = theme.icon_glyph_size_md.value();
    let mut x = rect.left() + pad_x;
    let cy = rect.center().y;
    icon(
        ui,
        egui::Rect::from_center_size(egui::pos2(x + glyph * 0.5, cy), egui::vec2(glyph, glyph)),
        icon_ink,
    );
    x += glyph + theme.spacing_sm.value();

    let laid = ui
        .ctx()
        .fonts(|f| lay_out(f, theme, label, width, ink, app_ink));
    let painter = ui.painter();
    match &laid.layout {
        RowLayout::Line {
            prefix,
            app,
            suffix,
        } => {
            for (g, color) in [(prefix, ink), (app, app_ink), (suffix, ink)] {
                if g.text().is_empty() {
                    continue;
                }
                painter.galley(egui::pos2(x, cy - g.size().y * 0.5), Arc::clone(g), color);
                x += g.size().x;
            }
        }
        RowLayout::Wrap(g) => {
            painter.galley(egui::pos2(x, cy - g.size().y * 0.5), Arc::clone(g), ink);
        }
    }
    if laid.truncated {
        resp.on_hover_text(label.app)
    } else {
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 시안 메뉴 상한 288 에서 셸 여백·테두리를 뺀 행 폭.
    fn max_row_width(th: &Theme) -> f32 {
        th.banner_more_menu_max_width().value()
            - (th.banner_more_menu_padding().value() + th.border_width.value()) * 2.0
    }

    fn laid(th: &Theme, label: BannerMoreLabel) -> (bool, bool, f32) {
        let ctx = egui::Context::default();
        let mut out = (false, false, 0.0);
        let _frame = ctx.run(egui::RawInput::default(), |ctx| {
            let w = max_row_width(th);
            let l =
                ctx.fonts(|f| lay_out(f, th, label, w, egui::Color32::WHITE, egui::Color32::WHITE));
            out = (
                matches!(l.layout, RowLayout::Wrap(_)),
                l.truncated,
                row_height(th, &l),
            );
        });
        out
    }

    const JA_DISABLE: &str = "このプログラムのマウスキャプチャを常に無効にする ";

    #[test]
    fn a_short_label_stays_on_one_line_at_the_row_height() {
        let th = theme();
        let label = BannerMoreLabel {
            prefix: "Disable mouse capture for ",
            app: "vim",
            suffix: "",
        };
        assert_eq!(
            laid(&th, label),
            (false, false, th.menu_item_height().value())
        );
    }

    #[test]
    fn a_long_name_ellipsises_on_one_line() {
        let th = theme();
        let label = BannerMoreLabel {
            prefix: "Disable mouse capture for ",
            app: "some-very-long-tool-name",
            suffix: "",
        };
        assert_eq!(
            laid(&th, label),
            (false, true, th.menu_item_height().value())
        );
    }

    /// 고정 문구만으로 288 을 넘으면 말줄임하지 않고 줄을 바꾸며 행이 높아진다.
    #[test]
    fn fixed_copy_past_the_cap_wraps_and_grows_the_row() {
        let th = theme();
        let label = BannerMoreLabel {
            prefix: JA_DISABLE,
            app: "vim",
            suffix: "",
        };
        let (wrapped, truncated, height) = laid(&th, label);
        assert!(wrapped);
        assert!(
            !truncated,
            "a short name is not ellipsised in a wrapped row"
        );
        let two_lines = th.font_size_body.value() * th.line_height_ui * 2.0
            + th.menu_item_wrap_padding_y().value() * 2.0;
        assert!(
            (height - two_lines).abs() < 1.0,
            "height {height} != two lines {two_lines}"
        );
    }

    /// 줄을 바꾼 행에서도 이름 혼자 한 줄보다 길면 말줄임한다.
    #[test]
    fn a_name_longer_than_a_line_ellipsises_in_a_wrapped_row() {
        let th = theme();
        let label = BannerMoreLabel {
            prefix: JA_DISABLE,
            app: &"x".repeat(80),
            suffix: "",
        };
        let (wrapped, truncated, _) = laid(&th, label);
        assert!(wrapped && truncated);
    }

    /// 행 글자색: 쉼은 menu-item-fg, 호버는 menu-item-fg-hover. 이름도 같은 행 글자색 토큰이다.
    #[test]
    fn the_name_takes_the_row_ink_at_rest_and_on_hover() {
        let th = theme();
        for hover in [false, true] {
            let ctx = egui::Context::default();
            let mut colors = Vec::new();
            for _ in 0..2 {
                let frame = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let _row = banner_more_row(
                            ui,
                            &th,
                            &|_, _, _| {},
                            BannerMoreLabel {
                                prefix: "Disable mouse capture for ",
                                app: "vim",
                                suffix: "",
                            },
                            false,
                            hover,
                            false,
                        );
                    });
                });
                colors = frame
                    .shapes
                    .iter()
                    .filter_map(|c| match &c.shape {
                        egui::Shape::Text(t) => Some(t.fallback_color),
                        _ => None,
                    })
                    .collect();
            }
            let (fixed, app) = if hover {
                (th.menu_item_fg_hover(), th.banner_more_app_fg_hover())
            } else {
                (th.menu_item_fg(), th.banner_more_app_fg())
            };
            // en 은 뒤 문구가 비어 앞 문구와 이름 두 조각이다.
            assert_eq!(colors, vec![fixed.to_egui(), app.to_egui()]);
        }
    }
}
