//! 원격 위치 표지 — remote 글리프 + 호스트 글자. 핸들러 picker 경로 줄이 쓴다.
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
