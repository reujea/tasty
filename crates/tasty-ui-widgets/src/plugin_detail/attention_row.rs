//! Attention 목록의 한 행. 다섯 사유가 같은 행을 쓰고 색만 다르다. 본체와 갤러리가 함께 부른다.

use tasty_type_appearance::theme::Theme;

use crate::plugin_avatar::{PluginAvatarSize, paint_plugin_avatar};
use crate::tokens::STRUCT_GAP_2;

/// 행 입력.
pub struct PluginAttentionRowView<'a> {
    pub name: &'a str,
    /// 사유 라벨.
    pub label: &'a str,
    pub selected: bool,
    pub hovered: bool,
    /// 사유의 severity 색. 점과 선택 가장자리에 쓴다.
    pub severity: egui::Color32,
    /// 선택되지 않은 행의 라벨 색.
    pub label_ink: egui::Color32,
}

/// 아바타 · [이름 / 사유 라벨] · severity 점. 이름과 라벨은 점 앞에서 말줄임한다.
/// 선택 행은 surface-active 바탕에 왼쪽 안쪽 `selection-edge-width` 띠를 두고 글자를 text-on-active 로 그린다.
pub fn paint_plugin_attention_row(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    view: &PluginAttentionRowView<'_>,
) {
    let radius = theme.corner_radius.value();
    let on_active = theme.text_on_active().to_egui();
    if view.selected {
        painter.rect_filled(rect, radius, theme.surface_active().to_egui());
        let edge = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.selection_edge_width.value(), rect.height()),
        );
        painter.rect_filled(
            edge,
            egui::CornerRadius {
                nw: radius as u8,
                sw: radius as u8,
                ne: 0,
                se: 0,
            },
            view.severity,
        );
    } else if view.hovered {
        painter.rect_filled(rect, radius, theme.overlay_hover().to_egui_premultiplied());
    }

    let pad = theme.spacing_sm.value();
    let avatar = PluginAvatarSize::Row.side().value();
    paint_plugin_avatar(
        painter,
        theme,
        egui::pos2(rect.min.x + pad + avatar * 0.5, rect.center().y),
        view.name,
        PluginAvatarSize::Row,
    );

    let dot = theme.status_dot_size.value();
    let dot_center = egui::pos2(rect.max.x - theme.spacing_md.value(), rect.center().y);
    painter.circle_filled(dot_center, dot * 0.5, view.severity);

    let name_pos = rect.min + egui::vec2(pad + avatar + pad, pad);
    let text_w = (dot_center.x - dot * 0.5 - pad - name_pos.x).max(0.0);
    let one_line = |text: &str, font: egui::FontId, color: egui::Color32| {
        let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, text_w);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        painter.ctx().fonts(|f| f.layout_job(job))
    };
    let name_color = if view.selected {
        on_active
    } else {
        theme.text_secondary().to_egui()
    };
    let name = one_line(
        view.name,
        egui::FontId::proportional(theme.font_size_body.value()),
        name_color,
    );
    painter.galley(name_pos, name, name_color);
    let label_color = if view.selected {
        on_active
    } else {
        view.label_ink
    };
    let label = one_line(
        view.label,
        crate::label_font(theme.font_size_micro.value()),
        label_color,
    );
    painter.galley(
        name_pos + egui::vec2(0.0, theme.spacing_lg.value() + STRUCT_GAP_2.value()),
        label,
        label_color,
    );
}
