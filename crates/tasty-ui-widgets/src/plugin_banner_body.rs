//! plugin이 제공한 배너 본문. 세 줄과 콘텐츠 높이에 맞는 줄 수 중 작은 값까지 감싸고 넘치면 말줄임한다.
//! 잘린 본문의 전문은 호스트가 배너 카드 아래 툴팁으로 보인다. 호스트 채널이 없으면 위젯이 본문 아래에 직접 그린다.
//! Tasty 고정 문구의 배너는 이 위젯을 쓰지 않고 줄 수를 제한하지 않는다.
//! 규칙: docs/design/systems/banner.md#본문-줄-수.

use tasty_type_appearance::theme::Theme;

use crate::tooltip::{Tooltip, TooltipPlacement, tooltip_hover_delay_elapsed};

/// plugin 본문이 보이는 최대 줄 수. 디자인이 정한 규칙 상수이며 토큰이 아니다.
pub const PLUGIN_BANNER_BODY_MAX_ROWS: usize = 3;

/// 이번 프레임에 잘린 plugin 본문. `body_rect`는 본문을 그린 사각형이며 좌표는 그 egui 화면 기준 논리 포인트다.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginBannerBodyCut {
    pub text: String,
    pub body_rect: egui::Rect,
}

fn host_channel_id() -> egui::Id {
    egui::Id::new("tasty_plugin_banner_body_host_tooltip")
}

fn cut_id() -> egui::Id {
    egui::Id::new("tasty_plugin_banner_body_cut")
}

/// `run` 안의 plugin 본문이 툴팁을 직접 그리지 않고 잘림만 기록하게 한다.
/// 호스트가 툴팁을 대신 그리는 배너 frame에서 SDK가 감싼다. 기록은 [`take_plugin_banner_body_cut`]로 꺼낸다.
pub fn plugin_banner_body_host_tooltip<R>(ctx: &egui::Context, run: impl FnOnce() -> R) -> R {
    ctx.data_mut(|d| {
        d.remove::<PluginBannerBodyCut>(cut_id());
        d.insert_temp(host_channel_id(), true);
    });
    let out = run();
    ctx.data_mut(|d| d.remove::<bool>(host_channel_id()));
    out
}

/// 직전 [`plugin_banner_body_host_tooltip`] frame에서 잘린 본문을 꺼낸다. 잘린 본문이 없으면 `None`이다.
/// 한 frame에 여러 본문이 잘렸으면 마지막에 그린 것만 남는다.
pub fn take_plugin_banner_body_cut(ctx: &egui::Context) -> Option<PluginBannerBodyCut> {
    ctx.data_mut(|d| {
        let cut = d.get_temp::<PluginBannerBodyCut>(cut_id());
        d.remove::<PluginBannerBodyCut>(cut_id());
        cut
    })
}

/// plugin 본문 한 줄의 높이. 콘텐츠 높이에 들어가는 줄 수를 셀 때 쓴다.
pub fn plugin_banner_body_line_height(theme: &Theme) -> f32 {
    theme.banner_body_font_size().value() * theme.line_height_ui
}

/// 보이는 줄 수 = min(3, 남은 높이에 들어가는 줄 수). 한 줄도 안 들어가도 한 줄은 그린다.
/// `available_height`는 본문 위쪽부터 콘텐츠 영역 아래 끝까지다.
fn visible_rows(theme: &Theme, available_height: f32) -> usize {
    // 줄 높이의 배수에서 부동소수 오차로 한 줄을 잃지 않도록 작은 여유를 둔다.
    let fit = ((available_height + 0.01) / plugin_banner_body_line_height(theme)).floor();
    let fit = if fit.is_finite() && fit >= 1.0 {
        fit as usize
    } else {
        1
    };
    fit.min(PLUGIN_BANNER_BODY_MAX_ROWS)
}

fn body_galley(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    wrap_width: f32,
    max_rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let size = theme.banner_body_font_size().value();
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: theme.banner_body_fg().to_egui(),
            line_height: Some(plugin_banner_body_line_height(theme)),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap_width;
    job.wrap.max_rows = max_rows;
    ui.fonts(|f| f.layout_job(job))
}

/// 남은 폭에 본문을 그린다. 줄이 넘치면 마지막으로 보이는 줄 끝을 말줄임한다.
/// 잘렸으면 호스트 채널 안에서는 잘림을 기록하고, 밖에서는 hover 때 본문 아래에 툴팁을 보인다.
pub fn plugin_banner_body(ui: &mut egui::Ui, theme: &Theme, text: &str) -> egui::Response {
    // plugin 배너의 콘텐츠 영역은 egui 화면이자 clip 영역이다. `available_height`는 `ui.horizontal` 같은
    // 레이아웃 안에서 한 줄 높이로 줄어들므로 쓰지 않고 clip 아래 끝까지의 높이를 잰다.
    let rows = visible_rows(theme, ui.clip_rect().bottom() - ui.cursor().top());
    let galley = body_galley(ui, theme, text, ui.available_width(), rows);
    let clamped = galley.elided;
    let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    ui.painter()
        .galley(rect.min, galley, theme.banner_body_fg().to_egui());
    if !clamped {
        return resp;
    }
    let host_draws = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(host_channel_id()))
        .unwrap_or(false);
    if host_draws {
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                cut_id(),
                PluginBannerBodyCut {
                    text: text.to_owned(),
                    body_rect: rect,
                },
            );
        });
    } else if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
        // 위로 띄우면 화면 맨 위의 본문에서 버블이 포인터를 덮어 hover가 끊기므로 아래에만 둔다.
        Tooltip::new(text)
            .id_source(resp.id)
            .placement(TooltipPlacement::Bottom)
            .show(ui, theme, rect);
    }
    resp
}

/// 호스트가 잘린 plugin 본문의 툴팁을 배너 카드 아래에 그린다. 포인터가 본문 위에 `tooltip-delay`만큼
/// 머물면 왼쪽을 본문 왼쪽에, 위를 카드 아래 `tooltip-offset`에 맞춘다. `body`·`card`는 창 좌표다.
/// 툴팁을 그렸으면 `true`를 돌려준다.
pub fn show_plugin_banner_body_tooltip(
    ctx: &egui::Context,
    theme: &Theme,
    id: egui::Id,
    text: &str,
    body: egui::Rect,
    card: egui::Rect,
    pointer: Option<egui::Pos2>,
) -> bool {
    let hovered = pointer.is_some_and(|p| body.contains(p));
    if !tooltip_hover_delay_elapsed(ctx, theme, id, hovered) {
        return false;
    }
    Tooltip::new(text)
        .id_source(id)
        .at(egui::pos2(
            body.left(),
            card.bottom() + theme.tooltip_offset().value(),
        ))
        .show_in(ctx, theme, body);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: &str = "This plugin body keeps going for a while so that a narrow banner has to \
                        wrap it onto many rows, well past the three rows a plugin body may use \
                        before the rest is cut and moved into the tooltip.";
    const SHORT: &str = "Short plugin body.";

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 마지막 프레임에서 관측한 본문과 툴팁.
    struct Seen {
        body_rect: egui::Rect,
        body_rows: usize,
        body_elided: bool,
        /// 본문 자리가 아닌 곳에 그린 전문 도형(툴팁 글)의 위치.
        tooltip_text_at: Option<egui::Pos2>,
        cut: Option<PluginBannerBodyCut>,
    }

    /// 본문을 `top`에서 그리고 포인터를 본문 첫 줄 위에 둔 채 툴팁 대기 시간보다 길게 프레임을 돌린다.
    /// `height`가 있으면 본문에 그 높이만 남긴다. `host`면 호스트 채널 안에서 그린다.
    /// `in_row`면 본문을 `ui.horizontal` 행 안(글리프 옆 자리)에 그린다.
    fn run(width: f32, top: f32, height: Option<f32>, text: &str, host: bool) -> Seen {
        run_in(width, top, height, text, host, false)
    }

    fn run_in(
        width: f32,
        top: f32,
        height: Option<f32>,
        text: &str,
        host: bool,
        in_row: bool,
    ) -> Seen {
        let theme = theme();
        let ctx = egui::Context::default();
        let mut seen = None;
        for i in 0..20 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 600.0),
                )),
                time: Some(f64::from(i) * 0.25),
                events: vec![egui::Event::PointerMoved(egui::pos2(12.0, top + 4.0))],
                ..Default::default()
            };
            let mut body_rect = egui::Rect::NOTHING;
            let draw = |ctx: &egui::Context, body_rect: &mut egui::Rect| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        ui.add_space(top);
                        if let Some(h) = height {
                            // 콘텐츠 영역 아래 끝을 본문 위쪽에서 `h`로 둔다.
                            let mut clip = ui.clip_rect();
                            clip.max.y = ui.cursor().top() + h;
                            ui.set_clip_rect(clip);
                        }
                        if in_row {
                            ui.horizontal(|ui| {
                                ui.label("!");
                                *body_rect = plugin_banner_body(ui, &theme, text).rect;
                            });
                        } else {
                            *body_rect = plugin_banner_body(ui, &theme, text).rect;
                        }
                    });
            };
            let out = ctx.run(input, |ctx| {
                if host {
                    plugin_banner_body_host_tooltip(ctx, || draw(ctx, &mut body_rect));
                } else {
                    draw(ctx, &mut body_rect);
                }
            });
            let cut = take_plugin_banner_body_cut(&ctx);
            let mut body = None;
            let mut tooltip_text_at = None;
            for c in &out.shapes {
                if let egui::Shape::Text(t) = &c.shape
                    && t.galley.text() == text
                {
                    if t.pos == body_rect.min {
                        body = Some((t.galley.rows.len(), t.galley.elided));
                    } else {
                        tooltip_text_at = Some(t.pos);
                    }
                }
            }
            let (body_rows, body_elided) = body.expect("body shape");
            seen = Some(Seen {
                body_rect,
                body_rows,
                body_elided,
                tooltip_text_at,
                cut,
            });
        }
        seen.expect("frames")
    }

    #[test]
    fn a_long_body_stops_at_three_rows_with_an_ellipsis() {
        let s = run(160.0, 200.0, None, LONG, false);
        assert_eq!(s.body_rows, PLUGIN_BANNER_BODY_MAX_ROWS);
        assert!(s.body_elided);
    }

    #[test]
    fn a_short_body_is_not_cut() {
        let s = run(400.0, 200.0, None, SHORT, false);
        assert_eq!(s.body_rows, 1);
        assert!(!s.body_elided);
    }

    #[test]
    fn a_content_height_for_two_rows_clamps_at_two() {
        let line = plugin_banner_body_line_height(&theme());
        let s = run(160.0, 200.0, Some(line * 2.0), LONG, false);
        assert_eq!(s.body_rows, 2);
        assert!(s.body_elided);
    }

    /// 글리프 옆 행 안의 본문도 콘텐츠 높이로 줄 수를 정한다(행 높이가 아니다).
    #[test]
    fn a_body_inside_a_row_still_gets_three_rows() {
        let s = run_in(200.0, 200.0, None, LONG, false, true);
        assert_eq!(s.body_rows, PLUGIN_BANNER_BODY_MAX_ROWS);
    }

    #[test]
    fn a_height_short_of_one_row_still_draws_one_row() {
        let line = plugin_banner_body_line_height(&theme());
        let s = run(160.0, 200.0, Some(line * 0.5), LONG, false);
        assert_eq!(s.body_rows, 1);
        assert!(s.body_elided);
    }

    #[test]
    fn hovering_a_cut_body_shows_the_tooltip_below_the_body() {
        let s = run(160.0, 200.0, None, LONG, false);
        let at = s.tooltip_text_at.expect("tooltip");
        assert!(at.y > s.body_rect.bottom(), "{at:?} vs {:?}", s.body_rect);
    }

    /// 화면 맨 위 본문에서도 툴팁이 포인터를 덮지 않고 뜬다(위로 띄우던 때는 깜박였다).
    #[test]
    fn a_body_at_the_top_of_the_screen_still_gets_its_tooltip_below() {
        let s = run(160.0, 0.0, None, LONG, false);
        let at = s.tooltip_text_at.expect("tooltip");
        assert!(at.y > s.body_rect.bottom());
    }

    #[test]
    fn hovering_an_uncut_body_shows_no_tooltip() {
        let s = run(400.0, 200.0, None, SHORT, false);
        assert!(s.tooltip_text_at.is_none());
        assert!(s.cut.is_none());
    }

    #[test]
    fn inside_the_host_channel_a_cut_is_reported_instead_of_drawn() {
        let s = run(160.0, 200.0, None, LONG, true);
        assert!(s.tooltip_text_at.is_none());
        let cut = s.cut.expect("cut");
        assert_eq!(cut.text, LONG);
        assert_eq!(cut.body_rect, s.body_rect);
    }

    #[test]
    fn inside_the_host_channel_an_uncut_body_reports_nothing() {
        let s = run(400.0, 200.0, None, SHORT, true);
        assert!(s.cut.is_none());
    }

    /// 호스트 툴팁을 대기 시간보다 길게 돌려 마지막 프레임의 툴팁 글 위치를 돌려준다.
    fn host_tooltip_text_at(pointer: egui::Pos2) -> Option<egui::Pos2> {
        let theme = theme();
        let ctx = egui::Context::default();
        let body = egui::Rect::from_min_size(egui::pos2(40.0, 100.0), egui::vec2(160.0, 45.0));
        let card = egui::Rect::from_min_max(egui::pos2(8.0, 90.0), egui::pos2(400.0, 160.0));
        let mut at = None;
        for i in 0..20 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                time: Some(f64::from(i) * 0.25),
                ..Default::default()
            };
            let out = ctx.run(input, |ctx| {
                show_plugin_banner_body_tooltip(
                    ctx,
                    &theme,
                    egui::Id::new("t"),
                    LONG,
                    body,
                    card,
                    Some(pointer),
                );
            });
            at = out.shapes.iter().find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == LONG => Some(t.pos),
                _ => None,
            });
        }
        at
    }

    #[test]
    fn the_host_tooltip_sits_below_the_card_at_the_body_left() {
        let theme = theme();
        let at = host_tooltip_text_at(egui::pos2(50.0, 110.0)).expect("tooltip");
        // 글은 버블 안쪽 여백(가로 space-sm, 세로 space-xs)과 테두리만큼 안에서 시작한다.
        let inset_x = theme.spacing_sm.value() + theme.border_width.value();
        let inset_y = theme.spacing_xs.value() + theme.border_width.value();
        assert!((at.x - (40.0 + inset_x)).abs() < 1.0, "{at:?}");
        assert!(
            (at.y - (160.0 + theme.tooltip_offset().value() + inset_y)).abs() < 1.0,
            "{at:?}"
        );
    }

    #[test]
    fn the_host_tooltip_waits_for_the_pointer_on_the_body() {
        assert!(host_tooltip_text_at(egui::pos2(300.0, 150.0)).is_none());
    }
}
