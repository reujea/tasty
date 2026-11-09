//! 파일 선택기 footer 의 확장자 필터 표시. 호출자가 넘긴 필터를 보여 줄 뿐 바꿀 수 없으므로
//! 컨트롤이 아니라 읽기 전용 표시로 그린다 — chevron·채움·hover·포커스가 없다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// 확장자 목록을 `*.toml, *.json` 형태로 만든다. 호출자 순서를 지키고 소문자로 바꾼다.
/// 필터가 없으면 칩을 그리지 않으므로 `None`.
pub fn filter_readout_label<S: AsRef<str>>(extensions: &[S]) -> Option<String> {
    if extensions.is_empty() {
        return None;
    }
    let parts: Vec<String> = extensions
        .iter()
        .map(|e| format!("*.{}", e.as_ref().trim_start_matches('.').to_lowercase()))
        .collect();
    Some(parts.join(", "))
}

/// 내용 폭으로 배치하되 `fp-filter-max-width` 를 넘으면 끝을 `…` 로 줄인 한 줄.
fn readout_galley(ui: &egui::Ui, theme: &Theme, label: &str) -> Arc<egui::Galley> {
    let pad = theme.spacing_sm;
    let text_max = (theme.fp_filter_max_width() - pad - pad).max(LogicalPx(0.0));
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        crate::label_font(theme.font_size_caption.value()),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: text_max.value(),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

/// 칩의 바깥 폭. 옆 이름 칸이 남은 폭을 가지려면 칩보다 먼저 이 값을 알아야 한다.
pub fn filter_readout_width(ui: &egui::Ui, theme: &Theme, label: &str) -> LogicalPx {
    let pad = theme.spacing_sm;
    LogicalPx(readout_galley(ui, theme, label).rect.width()) + pad + pad
}

/// 필터 칩을 그린다. `tooltip` 은 호출자가 번역한 전체 문구("Showing *.toml, *.json")로,
/// 줄어든 목록 전체를 hover 툴팁으로 보여 준다.
pub fn filter_readout(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    tooltip: &str,
) -> egui::Response {
    let pad = theme.spacing_sm.value();
    let galley = readout_galley(ui, theme, label);
    let size = egui::vec2(
        galley.rect.width() + 2.0 * pad,
        theme.fp_filter_height().value(),
    );
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(
            rect.left() + pad,
            rect.center().y - galley.rect.height() * 0.5,
        ),
        galley,
        theme.text_muted().to_egui(),
    );
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
        Tooltip::new(tooltip)
            .id_source(resp.id)
            .show(ui, theme, rect);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_filter_means_no_chip() {
        assert_eq!(filter_readout_label::<&str>(&[]), None);
    }

    #[test]
    fn the_label_keeps_caller_order_and_lowercases() {
        assert_eq!(
            filter_readout_label(&["TOML", "json", ".Md"]).as_deref(),
            Some("*.toml, *.json, *.md")
        );
    }
}
