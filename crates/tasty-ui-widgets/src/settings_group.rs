//! 설정 페이지의 묶음 머리. 위 구분선과 머리 줄(라벨 열 · 선택적 Switch)이다.
//!
//! 머리 줄은 [`crate::SettingsRow`] 와 같은 라벨 열 · gap · 컨트롤 격자를 써서 아래 행들과 열이 맞는다.
//! 라벨은 행 라벨(`text-secondary`)보다 진한 `text-primary` 다. 스위치가 있으면 컨트롤 열에 두고,
//! 끄면 호출자가 묶음의 행을 그리지 않는다 — 머리는 숨김을 모른다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::settings_row::settings_label_gap;
use crate::toggle::switch;

/// 묶음 머리를 그린다. `first` 면 위 구분선을 생략한다(페이지 맨 위). `switch_on` 이 있으면 머리에
/// Switch 를 두고 그 응답을 돌려준다.
pub fn settings_group_header(
    ui: &mut egui::Ui,
    theme: &Theme,
    label_col: LogicalPx,
    label: &str,
    switch_on: Option<&mut bool>,
    first: bool,
) -> Option<egui::Response> {
    if !first {
        ui.add_space(theme.spacing_md.value());
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(w, theme.border_width.value()),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, 0.0, theme.border_default().to_egui());
    }
    ui.add_space(theme.spacing_sm.value());
    let row_h = theme.settings_row_min_height().value();
    ui.horizontal(|ui| {
        let galley = ui.fonts(|f| {
            f.layout(
                label.to_owned(),
                egui::FontId::proportional(theme.font_size_heading.value()),
                theme.text_primary().to_egui(),
                label_col.value(),
            )
        });
        let h = row_h.max(galley.size().y);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(label_col.value(), h), egui::Sense::hover());
        let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
        ui.painter()
            .galley(pos, galley, theme.text_primary().to_egui());
        switch_on.map(|on| {
            settings_label_gap(ui, theme);
            switch(ui, theme, on, None, true)
        })
    })
    .inner
}
