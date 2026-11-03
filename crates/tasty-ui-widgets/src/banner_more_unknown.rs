//! 배너 ⋯ 메뉴의 이름 미상 형태. 행이 저장할 프로그램 이름을 모를 때 메뉴는 숨기지 않고
//! 비대화형 사유 줄, 구분선, 이름 대신 "이 프로그램"으로 쓴 두 행을 비활성으로 그린다.
//!
//! - 사유 줄: info 글리프(`icon-glyph-size-sm`) + caption 크기 `text-muted` 문장. 줄을 바꾸며
//!   위아래 `space-xs`, 좌우 `space-md`, 글리프와 문장 사이 `space-sm` 이다. 글리프는 문장 위쪽에 맞춘 뒤
//!   `banner-more-reason-glyph-offset` 만큼 내린다. 포커스·호버가 없다.
//! - 구분선: 위아래 `space-xs`, 두께 `border-width`, 색은 모든 메뉴 구분선과 같은 `menu-separator`.
//! - 행: MenuItem disabled(글자·아이콘 `state-disabled-fg`, 호버 채움 없음, 클릭 없음)에 wrap 을 켠 모습.
//!   최소 `menu-item-height`, 줄을 바꾸면 위아래 `menu-item-wrap-padding-y`, 줄 높이 `line-height-ui`.
//!   툴팁은 없다(사유가 메뉴 안에 보인다).

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::icon_button::IconPainter;

/// 이름 미상 형태의 문구. 문장 전체를 받는다(앞·뒤 분할 없음).
#[derive(Clone, Copy, Debug)]
pub struct BannerMoreUnknownCopy<'a> {
    pub reason: &'a str,
    pub rows: [&'a str; 2],
}

/// 사유 줄 문장 폭 — 좌우 `space-md`, 글리프, 글리프 간격을 뺀다.
fn reason_text_width(theme: &Theme, row_width: LogicalPx) -> LogicalPx {
    (row_width - theme.spacing_md * 2.0 - theme.icon_glyph_size_sm - theme.spacing_sm)
        .max(LogicalPx(0.0))
}

/// 행 라벨 폭 — 행 좌우 여백, 아이콘, 아이콘 간격을 뺀다.
fn row_label_width(theme: &Theme, row_width: LogicalPx) -> LogicalPx {
    (row_width - theme.menu_item_padding_x() * 2.0 - theme.icon_glyph_size_md - theme.spacing_sm)
        .max(LogicalPx(0.0))
}

fn wrapped(
    fonts: &egui::epaint::Fonts,
    text: &str,
    size: LogicalPx,
    line_height: f32,
    width: LogicalPx,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size.value()),
            color,
            line_height: Some((size * line_height).value()),
            ..Default::default()
        },
    );
    job.wrap.max_width = width.value();
    fonts.layout_job(job)
}

fn reason_galley(
    fonts: &egui::epaint::Fonts,
    theme: &Theme,
    text: &str,
    row_width: LogicalPx,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    wrapped(
        fonts,
        text,
        theme.font_size_caption,
        theme.line_height_ui,
        reason_text_width(theme, row_width),
        color,
    )
}

fn row_galley(
    fonts: &egui::epaint::Fonts,
    theme: &Theme,
    text: &str,
    row_width: LogicalPx,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    wrapped(
        fonts,
        text,
        theme.font_size_body,
        theme.line_height_ui,
        row_label_width(theme, row_width),
        color,
    )
}

/// 사유 줄 높이. 글리프와 문장을 위쪽에 맞춰 나란히 두므로 둘 중 높은 쪽에 위아래 `space-xs` 를 더한다.
fn reason_height(theme: &Theme, galley: &egui::Galley) -> LogicalPx {
    let glyph = theme.icon_glyph_size_sm + theme.banner_more_reason_glyph_offset();
    LogicalPx(galley.size().y).max(glyph) + theme.spacing_xs * 2.0
}

fn separator_height(theme: &Theme) -> LogicalPx {
    theme.spacing_xs * 2.0 + theme.border_width
}

fn row_height(theme: &Theme, galley: &egui::Galley) -> LogicalPx {
    (LogicalPx(galley.size().y) + theme.menu_item_wrap_padding_y() * 2.0)
        .max(theme.menu_item_height())
}

/// `row_width` 폭에서 사유 줄·구분선·두 행을 합한 높이. 메뉴 셸 높이를 정할 때 쓴다.
pub fn banner_more_unknown_height(
    ctx: &egui::Context,
    theme: &Theme,
    copy: BannerMoreUnknownCopy<'_>,
    row_width: LogicalPx,
) -> LogicalPx {
    ctx.fonts(|f| {
        let ph = egui::Color32::PLACEHOLDER;
        let reason = reason_height(theme, &reason_galley(f, theme, copy.reason, row_width, ph));
        let rows = copy.rows.iter().fold(LogicalPx(0.0), |acc, r| {
            acc + row_height(theme, &row_galley(f, theme, r, row_width, ph))
        });
        reason + separator_height(theme) + rows
    })
}

/// 사유 줄·구분선·두 비활성 행을 가용 폭에 위에서부터 그린다. 행의 응답을 돌려준다.
/// 행은 포인터·키보드 포커스를 받지 않아 눌러도 `clicked()` 가 되지 않는다.
pub fn banner_more_unknown(
    ui: &mut egui::Ui,
    theme: &Theme,
    info: IconPainter<'_>,
    row_icons: [IconPainter<'_>; 2],
    copy: BannerMoreUnknownCopy<'_>,
) -> [egui::Response; 2] {
    let width = LogicalPx(ui.available_width());
    let muted = theme.text_muted().to_egui();
    let disabled = theme.state_disabled_fg().to_egui();

    // 사유 줄 — 글리프는 문장 위쪽에서 사유 글리프 오프셋만큼 내린다.
    let reason = ui
        .ctx()
        .fonts(|f| reason_galley(f, theme, copy.reason, width, muted));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), reason_height(theme, &reason).value()),
        egui::Sense::hover(),
    );
    let glyph = theme.icon_glyph_size_sm.value();
    let top = rect.top() + theme.spacing_xs.value();
    let x = rect.left() + theme.spacing_md.value();
    info(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, top + theme.banner_more_reason_glyph_offset().value()),
            egui::vec2(glyph, glyph),
        ),
        muted,
    );
    ui.painter().galley(
        egui::pos2(x + glyph + theme.spacing_sm.value(), top),
        reason,
        muted,
    );

    // 구분선 — 메뉴 행 폭 전체, 공용 메뉴 구분선 색.
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), separator_height(theme).value()),
        egui::Sense::hover(),
    );
    let line = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + theme.spacing_xs.value()),
        egui::vec2(width.value(), theme.border_width.value()),
    );
    ui.painter()
        .rect_filled(line, 0.0, theme.menu_separator().to_egui());

    let [first, second] = row_icons;
    [
        disabled_row(ui, theme, first, copy.rows[0], width, disabled),
        disabled_row(ui, theme, second, copy.rows[1], width, disabled),
    ]
}

fn disabled_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: IconPainter<'_>,
    text: &str,
    width: LogicalPx,
    ink: egui::Color32,
) -> egui::Response {
    let galley = ui.ctx().fonts(|f| row_galley(f, theme, text, width, ink));
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width.value(), row_height(theme, &galley).value()),
        egui::Sense::hover(),
    );
    let glyph = theme.icon_glyph_size_md.value();
    let x = rect.left() + theme.menu_item_padding_x().value();
    let cy = rect.center().y;
    icon(
        ui,
        egui::Rect::from_center_size(egui::pos2(x + glyph * 0.5, cy), egui::vec2(glyph, glyph)),
        ink,
    );
    ui.painter().galley(
        egui::pos2(
            x + glyph + theme.spacing_sm.value(),
            cy - galley.size().y * 0.5,
        ),
        galley,
        ink,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    const EN: BannerMoreUnknownCopy<'static> = BannerMoreUnknownCopy {
        reason: "Can't tell which program is running, so nothing can be saved.",
        rows: [
            "Don't show again for this program",
            "Let Tasty handle clicks in this program",
        ],
    };

    /// 시안 메뉴 상한 288 에서 셸 여백·테두리를 뺀 행 폭.
    fn max_row_width(th: &Theme) -> f32 {
        (th.banner_more_menu_max_width() - (th.banner_more_menu_padding() + th.border_width) * 2.0)
            .value()
    }

    /// 가로 폭 `w` 에서 그린 결과: (사유 줄·구분선·두 행을 합한 높이, 두 행 사각형, 잰 높이).
    fn draw(
        th: &Theme,
        copy: BannerMoreUnknownCopy<'_>,
        w: f32,
        events: Vec<egui::Event>,
    ) -> (f32, [egui::Response; 2], f32) {
        let ctx = egui::Context::default();
        let mut out = None;
        let _frame = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::Area::new(egui::Id::new("unknown"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_width(w);
                        ui.set_max_width(w);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let top = ui.cursor().top();
                        let noop = |_: &mut egui::Ui, _: egui::Rect, _: egui::Color32| {};
                        let rows = banner_more_unknown(ui, th, &noop, [&noop, &noop], copy);
                        let drawn = rows[1].rect.bottom() - top;
                        out = Some((
                            drawn,
                            rows,
                            banner_more_unknown_height(ctx, th, copy, LogicalPx(w)).value(),
                        ));
                    });
            },
        );
        out.expect("drawn")
    }

    /// 열 때 잰 높이와 그린 높이가 같아야 셸이 행을 자르지 않는다. en 두 행은 한 줄이라 `menu-item-height` 다.
    #[test]
    fn the_measured_height_matches_the_drawn_rows_and_one_line_rows_keep_the_row_height() {
        let th = theme();
        let (drawn, rows, measured) = draw(&th, EN, max_row_width(&th), vec![]);
        assert_eq!(drawn, measured);
        for row in &rows {
            assert_eq!(row.rect.height(), th.menu_item_height().value());
        }
    }

    /// 좁은 폭에서는 행이 줄을 바꿔 높아지고, 잰 높이도 그만큼 커진다.
    #[test]
    fn a_narrow_menu_wraps_the_rows_and_the_reason() {
        let th = theme();
        let narrow = 120.0;
        let (drawn, rows, measured) = draw(&th, EN, narrow, vec![]);
        let (wide, _, _) = draw(&th, EN, max_row_width(&th), vec![]);
        assert_eq!(drawn, measured);
        assert!(rows[1].rect.height() > th.menu_item_height().value());
        assert!(drawn > wide);
    }

    /// 메뉴를 그린 프레임들을 차례로 돌린다. 각 프레임의 두 행 응답(클릭됨, 포커스)을 모은다.
    fn run_frames(th: &Theme, frames: Vec<Vec<egui::Event>>) -> Vec<[(bool, bool); 2]> {
        let w = max_row_width(th);
        let ctx = egui::Context::default();
        let mut seen = Vec::new();
        for events in frames {
            let _frame = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::Area::new(egui::Id::new("unknown"))
                        .fixed_pos(egui::Pos2::ZERO)
                        .show(ctx, |ui| {
                            ui.set_width(w);
                            ui.set_max_width(w);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let noop = |_: &mut egui::Ui, _: egui::Rect, _: egui::Color32| {};
                            let rows = banner_more_unknown(ui, th, &noop, [&noop, &noop], EN);
                            seen.push(rows.map(|r| (r.clicked(), r.has_focus())));
                        });
                },
            );
        }
        seen
    }

    /// 이름 미상 메뉴를 한 프레임 그린다. (사유 줄 위쪽, info 글리프 사각형, 그린 도형 목록).
    fn draw_shapes(th: &Theme, w: f32) -> (f32, egui::Rect, Vec<egui::epaint::ClippedShape>) {
        let ctx = egui::Context::default();
        let glyph = std::cell::Cell::new(None);
        let mut top = 0.0;
        // 새 Area 의 첫 프레임은 크기만 재고 그리지 않으므로 두 번째 프레임의 도형을 본다.
        // 등장 페이드가 색을 곱하지 않도록 끈다.
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let frame = ctx.run(egui::RawInput::default(), |ctx| {
                egui::Area::new(egui::Id::new("unknown"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .fade_in(false)
                    .show(ctx, |ui| {
                        ui.set_width(w);
                        ui.set_max_width(w);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        top = ui.cursor().top();
                        let info =
                            |_: &mut egui::Ui, r: egui::Rect, _: egui::Color32| glyph.set(Some(r));
                        let noop = |_: &mut egui::Ui, _: egui::Rect, _: egui::Color32| {};
                        let _rows = banner_more_unknown(ui, th, &info, [&noop, &noop], EN);
                    });
            });
            shapes = frame.shapes;
        }
        (top, glyph.get().expect("info glyph painted"), shapes)
    }

    /// 사유 줄 info 글리프는 문장 위쪽(사유 줄 위 `space-xs`)에서 `banner-more-reason-glyph-offset` 만큼 내린
    /// 자리에 놓인다. 첫 줄 세로 가운데에 두면 오프셋과 어긋난다.
    #[test]
    fn the_reason_glyph_sits_at_the_reason_glyph_offset_below_the_text_top() {
        let th = theme();
        let (top, glyph, _) = draw_shapes(&th, max_row_width(&th));
        let expected = top + (th.spacing_xs + th.banner_more_reason_glyph_offset()).value();
        assert_eq!(glyph.top(), expected);
        assert_eq!(glyph.left(), th.spacing_md.value());
        assert_eq!(glyph.height(), th.icon_glyph_size_sm.value());
    }

    /// 사유 줄과 행 사이 구분선은 모든 메뉴 구분선과 같은 `menu-separator` 색으로 그린다.
    /// 카드 안 hairline 색 `separator` 로 그리지 않는다.
    #[test]
    fn the_separator_uses_the_menu_separator_color() {
        let th = theme();
        assert_ne!(
            th.menu_separator().to_egui(),
            th.separator.to_egui_premultiplied()
        );
        let (_, _, shapes) = draw_shapes(&th, max_row_width(&th));
        let fills: Vec<egui::Color32> = shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.rect.height() == th.border_width.value() => Some(r.fill),
                _ => None,
            })
            .collect();
        assert_eq!(fills, vec![th.menu_separator().to_egui()]);
    }

    /// 비활성 행은 누름·뗌을 받아도 클릭으로 응답하지 않는다.
    #[test]
    fn the_disabled_rows_never_report_a_click() {
        let th = theme();
        let (_, rows, _) = draw(&th, EN, max_row_width(&th), vec![]);
        for row in rows {
            let at = row.rect.center();
            let press = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let frames = vec![
                vec![],
                vec![egui::Event::PointerMoved(at)],
                vec![press(true)],
                vec![press(false)],
                vec![],
            ];
            for [a, b] in run_frames(&th, frames) {
                assert!(!a.0 && !b.0, "a disabled row reported a click");
            }
        }
    }

    /// 비활성 행은 Tab 으로 포커스를 받지 않는다. 포커스 이동은 다음 프레임에 반영되므로
    /// Tab 마다 빈 프레임을 하나씩 더 돌려 확인한다.
    #[test]
    fn the_disabled_rows_never_take_focus() {
        let th = theme();
        let tab = || egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut frames = vec![vec![]];
        for _ in 0..3 {
            frames.push(vec![tab()]);
            frames.push(vec![]);
        }
        for [a, b] in run_frames(&th, frames) {
            assert!(!a.1 && !b.1, "a disabled row took focus");
        }
    }
}
