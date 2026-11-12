//! 고정 폭의 왼쪽 목록과 오른쪽 콘텐츠를 나란히 배치한다.
//! 현재 갤러리 레이아웃 예제에서 사용하며 본체 Settings의 사이드바와는 구조·폭이 다르다.
//! 호출자가 헤더·푸터를 제외한 가용 높이를 계산해 전달해야 한다.

use tasty_type_appearance::theme::Theme;

use crate::tokens;

/// left는 왼쪽 패널에서, content는 최대 높이가 제한된 오른쪽 세로 영역에서 실행한다.
pub fn two_depth_layout(
    ui: &mut egui::Ui,
    theme: &Theme,
    available_height: f32,
    left: impl FnOnce(&mut egui::Ui),
    content: impl FnOnce(&mut egui::Ui),
) {
    two_depth_layout_inner(ui, theme, available_height, None, left, content);
}

/// 왼쪽 패널 위에 필터 입력을 추가한다. 실제 목록 필터링은 left 클로저가 맡는다.
pub fn two_depth_layout_filtered(
    ui: &mut egui::Ui,
    theme: &Theme,
    available_height: f32,
    filter: &mut String,
    placeholder: &str,
    left: impl FnOnce(&mut egui::Ui),
    content: impl FnOnce(&mut egui::Ui),
) {
    two_depth_layout_inner(
        ui,
        theme,
        available_height,
        Some((filter, placeholder)),
        left,
        content,
    );
}

fn two_depth_layout_inner(
    ui: &mut egui::Ui,
    theme: &Theme,
    available_height: f32,
    filter: Option<(&mut String, &str)>,
    left: impl FnOnce(&mut egui::Ui),
    content: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal_top(|ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_app()))
            .stroke(egui::Stroke::new(
                tokens::PANEL_STROKE_WIDTH,
                egui::Color32::from(theme.border_default()),
            ))
            .corner_radius(tokens::PANEL_CORNER_RADIUS)
            .inner_margin(egui::Margin::symmetric(
                tokens::PANEL_INNER_MARGIN,
                tokens::PANEL_INNER_MARGIN,
            ))
            .show(ui, |ui| {
                ui.set_width(tokens::SUB_TAB_PANEL_WIDTH);
                ui.set_min_height(available_height);

                ui.vertical(|ui| {
                    if let Some((filter, placeholder)) = filter {
                        ui.add(
                            egui::TextEdit::singleline(filter)
                                .hint_text(tasty_egui_theme::hint_text(theme, placeholder))
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(tokens::PANEL_SPACING);
                        ui.separator();
                        ui.add_space(tokens::PANEL_SPACING);
                    }
                    left(ui);
                });
            });

        ui.add_space(tokens::PANEL_SPACING);

        ui.vertical(|ui| {
            ui.set_max_height(available_height);
            content(ui);
        });
    });
}
