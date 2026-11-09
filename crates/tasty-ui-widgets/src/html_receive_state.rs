//! Host-painted states while a remote entry has no native page to display.
use crate::Spinner;
use tasty_type_appearance::theme::Theme;

#[derive(Clone, Copy)]
pub enum HtmlReceiveKind {
    Receiving,
    Paused,
    Disconnected,
    UnsupportedHttp,
}
pub struct HtmlReceiveView<'a> {
    pub kind: HtmlReceiveKind,
    pub title: &'a str,
    pub subtitle: &'a str,
}
pub fn html_receive_state(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    view: &HtmlReceiveView<'_>,
) {
    let padding = theme.spacing_md;
    let gap = theme.spacing_sm;
    let width = (rect.width() - padding.value() * 2.0).max(0.0);
    let title = ui.painter().layout(
        view.title.into(),
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.html_receiving_fg().to_egui(),
        width,
    );
    let font = if matches!(view.kind, HtmlReceiveKind::Receiving) {
        crate::label_font(theme.font_size_caption.value())
    } else {
        egui::FontId::proportional(theme.font_size_caption.value())
    };
    let subtitle = ui.painter().layout(
        view.subtitle.into(),
        font,
        theme.text_muted().to_egui(),
        width,
    );
    let glyph = theme.icon_glyph_size_md;
    let height = glyph.value() + gap.value() * 2.0 + title.size().y + subtitle.size().y;
    let top = (rect.center().y - height / 2.0).max(rect.top() + padding.value());
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, top + glyph.value() / 2.0),
        egui::vec2(glyph.value(), glyph.value()),
    );
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    if matches!(view.kind, HtmlReceiveKind::Receiving) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
        Spinner::new().size(glyph.value()).show(&mut child, theme);
    } else {
        let icon = match view.kind {
            HtmlReceiveKind::Paused => tasty_icons::DOWNLOAD,
            HtmlReceiveKind::Disconnected => tasty_icons::REMOTE,
            HtmlReceiveKind::UnsupportedHttp => tasty_icons::INFO,
            HtmlReceiveKind::Receiving => unreachable!(),
        };
        icon.image(glyph.value(), theme.html_receiving_glyph().to_egui())
            .paint_at(ui, icon_rect);
    }
    let title_y = icon_rect.bottom() + gap.value();
    let subtitle_y = title_y + title.size().y + gap.value();
    painter.galley(
        egui::pos2(rect.center().x - title.size().x / 2.0, title_y),
        title,
        egui::Color32::PLACEHOLDER,
    );
    painter.galley(
        egui::pos2(rect.center().x - subtitle.size().x / 2.0, subtitle_y),
        subtitle,
        egui::Color32::PLACEHOLDER,
    );
}
