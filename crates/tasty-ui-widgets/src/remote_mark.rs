//! 원격 위치 표지 — remote 글리프 + 호스트 글자. 핸들러 picker 경로 줄과 링크 툴팁이 쓴다.
//! 호스트 글자는 mono, 비었을 때 그 자리에 쓰는 낱말은 font-label 이다. 끝을 `…` 로 줄인다.
//! 상태바 원격 항목은 바의 축소 규칙을 따르므로 `status_bar` 에 따로 있다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 표지에 들어갈 글자. `host` 가 비면 `empty_label` 을 쓴다.
#[derive(Clone, Copy, Debug)]
pub struct RemoteMark<'a> {
    pub host: &'a str,
    pub empty_label: &'a str,
}

/// 표지 한 벌의 색·간격·상한.
#[derive(Clone, Copy)]
struct MarkStyle {
    fg: egui::Color32,
    gap: LogicalPx,
    /// 호스트 글자 상한(글리프와 gap 은 밖).
    max_w: LogicalPx,
}

fn host_galley(
    ctx: &egui::Context,
    th: &Theme,
    mark: &RemoteMark<'_>,
    max_w: f32,
) -> Arc<egui::Galley> {
    let caption = th.font_size_caption.value();
    let (text, font) = if mark.host.is_empty() {
        (mark.empty_label, crate::label_font(caption))
    } else {
        (mark.host, egui::FontId::monospace(caption))
    };
    let mut job =
        egui::text::LayoutJob::simple_singleline(text.to_owned(), font, egui::Color32::PLACEHOLDER);
    job.wrap = egui::text::TextWrapping {
        max_width: max_w.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ctx.fonts(|f| f.layout_job(job))
}

/// 글리프 + gap + 호스트 galley. 호스트 글자는 `max_w` 상한에서 끝을 줄인다.
fn layout(
    ui: &egui::Ui,
    th: &Theme,
    mark: &RemoteMark<'_>,
    style: MarkStyle,
) -> (Arc<egui::Galley>, LogicalPx) {
    let galley = host_galley(ui.ctx(), th, mark, style.max_w.value());
    let w = th.icon_glyph_size_xs + style.gap + LogicalPx(galley.rect.width());
    (galley, w)
}

fn paint(
    ui: &egui::Ui,
    th: &Theme,
    left_center: egui::Pos2,
    galley: Arc<egui::Galley>,
    style: MarkStyle,
) {
    let gs = th.icon_glyph_size_xs.value();
    let grect = egui::Rect::from_center_size(
        egui::pos2(left_center.x + gs / 2.0, left_center.y),
        egui::Vec2::splat(gs),
    );
    tasty_icons::REMOTE.image(gs, style.fg).paint_at(ui, grect);
    let pos = egui::pos2(
        left_center.x + gs + style.gap.value(),
        left_center.y - galley.rect.height() / 2.0,
    );
    ui.painter().galley(pos, galley, style.fg);
}

fn picker_style(th: &Theme) -> MarkStyle {
    MarkStyle {
        fg: th.fh_remote_fg().into(),
        gap: th.fh_remote_gap(),
        max_w: th.fh_remote_max_width(),
    }
}

/// 핸들러 picker 경로 줄 앞 표지의 폭(글리프 + gap + 호스트, 호스트는 `fh-remote-max-width` 상한).
pub fn fh_remote_mark_width(ui: &egui::Ui, th: &Theme, mark: &RemoteMark<'_>) -> LogicalPx {
    layout(ui, th, mark, picker_style(th)).1
}

/// 핸들러 picker 경로 줄 앞 표지를 `left_center`(왼쪽 끝, 세로 가운데)에 그리고 폭을 돌려준다.
pub fn paint_fh_remote_mark(
    ui: &egui::Ui,
    th: &Theme,
    left_center: egui::Pos2,
    mark: &RemoteMark<'_>,
) -> LogicalPx {
    let style = picker_style(th);
    let (galley, w) = layout(ui, th, mark, style);
    paint(ui, th, left_center, galley, style);
    w
}

/// 링크 툴팁의 내용 — `Remote` 낱말(tooltip-fg) · space-sm · 글리프 + 호스트(link-remote-fg).
/// 호스트가 비면 낱말과 글리프만 그린다(낱말을 두 번 쓰지 않는다). 전체가 `tooltip-max-width`
/// 안에 들도록 호스트 끝을 줄인다.
pub struct LinkRemoteTip<'a> {
    pub word: &'a str,
    pub host: &'a str,
}

impl LinkRemoteTip<'_> {
    fn word_galley(&self, ctx: &egui::Context, th: &Theme) -> Arc<egui::Galley> {
        ctx.fonts(|f| {
            f.layout_no_wrap(
                self.word.to_owned(),
                egui::FontId::proportional(th.font_size_caption.value()),
                egui::Color32::PLACEHOLDER,
            )
        })
    }

    fn style(th: &Theme) -> MarkStyle {
        MarkStyle {
            fg: th.link_remote_fg().into(),
            gap: th.link_remote_gap(),
            // 링크 툴팁은 따로 상한이 없고 툴팁 최대 폭 안에서 줄인다.
            max_w: th.tooltip_max_width,
        }
    }

    /// 버블 안쪽(여백 제외)의 내용 폭과 높이.
    fn content(
        &self,
        ctx: &egui::Context,
        th: &Theme,
    ) -> (Arc<egui::Galley>, Option<Arc<egui::Galley>>, egui::Vec2) {
        let word = self.word_galley(ctx, th);
        let inner_max = th.tooltip_max_width.value() - th.spacing_sm.scaled(2.0).value();
        let before = word.rect.width() + th.spacing_sm.value();
        let style = Self::style(th);
        let glyph = th.icon_glyph_size_xs.value();
        let host = (!self.host.is_empty()).then(|| {
            let room = inner_max - before - glyph - style.gap.value();
            host_galley(
                ctx,
                th,
                &RemoteMark {
                    host: self.host,
                    empty_label: "",
                },
                room,
            )
        });
        let host_w = host
            .as_ref()
            .map_or(0.0, |g| style.gap.value() + g.rect.width());
        let h = word
            .rect
            .height()
            .max(glyph)
            .max(host.as_ref().map_or(0.0, |g| g.rect.height()));
        (word, host, egui::vec2(before + glyph + host_w, h))
    }

    /// 버블 크기 — 내용 + 여백(space-sm · space-xs) + 테두리.
    pub fn bubble_size(&self, ctx: &egui::Context, th: &Theme) -> egui::Vec2 {
        let (_, _, inner) = self.content(ctx, th);
        let border = th.border_width.scaled(2.0).value();
        inner
            + egui::vec2(
                th.spacing_sm.scaled(2.0).value() + border,
                th.spacing_xs.scaled(2.0).value() + border,
            )
    }

    /// `rect`(버블 바깥 사각형)에 `ui` 의 painter 로 버블을 그린다. 갤러리처럼 Area 없이 그리는 곳도
    /// 이 함수를 쓴다.
    pub fn paint(&self, ui: &egui::Ui, th: &Theme, rect: egui::Rect) {
        let ctx = ui.ctx();
        let painter = ui.painter();
        let (word, host, inner) = self.content(ctx, th);
        let style = Self::style(th);
        let radius = th.corner_radius.value();
        painter.add(th.shadow_popover().to_egui().as_shape(rect, radius));
        painter.rect(
            rect,
            radius,
            th.surface_raised().to_egui(),
            egui::Stroke::new(th.border_width.value(), th.border_strong().to_egui()),
            egui::StrokeKind::Inside,
        );
        let pad = egui::vec2(th.spacing_sm.value(), th.spacing_xs.value());
        let r = egui::Rect::from_min_size(
            rect.min + pad + egui::Vec2::splat(th.border_width.value()),
            inner,
        );
        let cy = r.center().y;
        let word_w = word.rect.width();
        let word_h = word.rect.height();
        painter.galley(
            egui::pos2(r.left(), cy - word_h / 2.0),
            word,
            th.tooltip_fg().into(),
        );
        let left = r.left() + word_w + th.spacing_sm.value();
        let gs = th.icon_glyph_size_xs.value();
        let grect =
            egui::Rect::from_center_size(egui::pos2(left + gs / 2.0, cy), egui::Vec2::splat(gs));
        tasty_icons::REMOTE.image(gs, style.fg).paint_at(ui, grect);
        if let Some(host) = host {
            let h = host.rect.height();
            painter.galley(
                egui::pos2(left + gs + style.gap.value(), cy - h / 2.0),
                host,
                style.fg,
            );
        }
    }

    /// `anchor`(링크 런) 아래 툴팁 층에 버블을 그린다. 아래에 자리가 없으면 위, 가로는 `bounds` 안으로 당긴다.
    pub fn show(&self, ctx: &egui::Context, th: &Theme, anchor: egui::Rect, bounds: egui::Rect) {
        let size = self.bubble_size(ctx, th);
        let rect = link_tip_rect(anchor, size, th.tooltip_offset().value(), bounds);
        egui::Area::new(egui::Id::new("tasty_link_remote_tip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
                self.paint(ui, th, r);
            });
    }
}

/// 링크 툴팁 자리 — 링크 아래, 아래에 자리가 없으면 위. 가로는 링크 가운데에서 `bounds` 안으로 당긴다.
pub fn link_tip_rect(
    anchor: egui::Rect,
    size: egui::Vec2,
    offset: f32,
    bounds: egui::Rect,
) -> egui::Rect {
    let below = anchor.bottom() + offset;
    let top = if below + size.y <= bounds.bottom() {
        below
    } else {
        (anchor.top() - offset - size.y).max(bounds.top())
    };
    let left = (anchor.center().x - size.x / 2.0)
        .min(bounds.right() - size.x)
        .max(bounds.left());
    egui::Rect::from_min_size(egui::pos2(left, top), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    #[test]
    fn the_link_tip_sits_below_the_run_when_there_is_room() {
        let anchor = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(84.0, 18.0));
        let r = link_tip_rect(anchor, egui::vec2(120.0, 24.0), 4.0, bounds());
        assert_eq!(r.top(), anchor.bottom() + 4.0);
        assert_eq!(r.center().x, anchor.center().x);
    }

    #[test]
    fn the_link_tip_goes_above_when_the_bottom_is_full_and_stays_in_the_window() {
        let anchor = egui::Rect::from_min_size(egui::pos2(780.0, 580.0), egui::vec2(20.0, 18.0));
        let r = link_tip_rect(anchor, egui::vec2(120.0, 24.0), 4.0, bounds());
        assert_eq!(r.bottom(), anchor.top() - 4.0);
        assert!(bounds().contains_rect(r));
    }
}
