//! 클립보드 뷰어 plugin 과 갤러리가 공유하는 타입 선택 세그먼트.
//! 타입 목록·번역·아이콘 그리기는 호출자가 넘긴다. plugin 은 baked 아이콘을, 갤러리는
//! SVG 글리프를 그리므로 아이콘만 콜백으로 받는다.

use tasty_type_appearance::theme::Theme;

use crate::ControlSize;

/// 타입이 이 개수 이상이면 선택하지 않은 세그먼트는 아이콘만 표시한다.
pub const SEG_COMPACT_AT: usize = 5;

/// 세그먼트 하나 — 라벨과 hover 툴팁.
pub struct TypeSegment<'a> {
    pub label: &'a str,
    pub tooltip: &'a str,
}

/// 압축 표시 중에는 선택한 타입만 라벨을 표시한다.
pub fn seg_shows_label(compact: bool, active: bool) -> bool {
    !compact || active
}

/// 세그먼트 아이콘을 그리는 콜백 — (ui, 세그먼트 번호, 아이콘 중심, 한 변, 색).
pub type SegmentIconPainter<'a> = dyn Fn(&egui::Ui, usize, egui::Pos2, f32, egui::Color32) + 'a;

/// 타입 선택 세그먼트를 그린다. 선택 중이 아닌 세그먼트를 누르면 그 번호를 반환한다.
/// 세그먼트가 [`SEG_COMPACT_AT`] 개 이상이면 선택한 세그먼트만 라벨을 표시한다.
pub fn draw_type_segments(
    ui: &mut egui::Ui,
    theme: &Theme,
    segments: &[TypeSegment<'_>],
    active: usize,
    paint_icon: &SegmentIconPainter<'_>,
) -> Option<usize> {
    let compact = segments.len() >= SEG_COMPACT_AT;
    let h = ControlSize::Sm.height(theme);
    let icon_sz = theme.icon_glyph_size_xs.value();
    let font = egui::FontId::proportional(theme.font_size_term_sm.value());
    let pad_x = theme.spacing_sm.value();
    let gap = theme.spacing_xs.value();
    let mut picked = None;

    egui::Frame::new()
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.horizontal(|ui| {
                for (i, seg) in segments.iter().enumerate() {
                    let on = i == active;
                    let show_label = seg_shows_label(compact, on);
                    let label_w = if show_label {
                        ui.fonts(|f| {
                            f.layout_no_wrap(
                                seg.label.to_owned(),
                                font.clone(),
                                egui::Color32::PLACEHOLDER,
                            )
                        })
                        .size()
                        .x
                    } else {
                        0.0
                    };
                    let row_gap = if show_label { gap } else { 0.0 };
                    let seg_w = pad_x * 2.0 + icon_sz + row_gap + label_w;
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(seg_w, h), egui::Sense::click());

                    if i > 0 {
                        ui.painter().vline(
                            rect.left(),
                            rect.y_range(),
                            egui::Stroke::new(
                                theme.border_width.value(),
                                theme.border_default().to_egui(),
                            ),
                        );
                    }
                    if on {
                        ui.painter().rect_filled(
                            rect,
                            0.0,
                            theme.accent_primary_raised().to_egui(),
                        );
                    }
                    let fg = if on {
                        theme.text_on_accent()
                    } else {
                        theme.text_secondary()
                    }
                    .to_egui();
                    let icon_center =
                        egui::pos2(rect.left() + pad_x + icon_sz * 0.5, rect.center().y);
                    paint_icon(ui, i, icon_center, icon_sz, fg);
                    if show_label {
                        ui.painter().text(
                            egui::pos2(icon_center.x + icon_sz * 0.5 + row_gap, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            seg.label,
                            font.clone(),
                            fg,
                        );
                    }
                    if resp.on_hover_text(seg.tooltip).clicked() && !on {
                        picked = Some(i);
                    }
                }
            });
        });

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_bar_labels_only_the_active_segment() {
        assert!(seg_shows_label(false, false));
        assert!(seg_shows_label(false, true));
        assert!(seg_shows_label(true, true));
        assert!(!seg_shows_label(true, false));
    }

    #[test]
    fn seg_compact_at_matches_design() {
        assert_eq!(SEG_COMPACT_AT, 5);
    }
}
