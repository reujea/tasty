//! 설치된 플러그인 상세의 마지막 절 — 설치 경로와 로그 경로, `Open folder`. 본체와 갤러리가 함께 호출한다.
//!
//! `Open folder` 는 경로 줄이 아니라 머리글 줄 오른쪽에 둔다. 긴 경로가 버튼과 폭을 다투면
//! 좁은 창에서 버튼이 잘리기 때문이다. 경로는 아무 문자에서 줄바꿈하고 선택할 수 있어
//! 잘리는 글자가 없으므로 말줄임과 툴팁을 두지 않는다.

use tasty_type_appearance::theme::Theme;

use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::plugin_add::label_header;

/// 상세의 한 절. 디자인 `Mono` 머리글(대문자) 아래에 본문을 `space-sm` 간격으로 둔다.
pub fn plugin_detail_section<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    header: &str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        label_header(ui, theme, header);
        body(ui)
    })
    .inner
}

/// 상세의 절과 절 사이. 디자인은 구분선 없이 `space-lg` 만 띄운다. 호출한 Ui 의 세로
/// item_spacing 이 이미 들어가므로 나머지만 더한다.
pub fn plugin_detail_section_gap(ui: &mut egui::Ui, theme: &Theme) {
    let rest = theme.spacing_lg.value() - ui.spacing().item_spacing.y;
    ui.add_space(rest.max(0.0));
}

/// 호출부가 번역해 넘기는 문구와 경로.
pub struct PluginInstallPathsView<'a> {
    /// 머리글. 대문자로 그린다.
    pub label: &'a str,
    pub open_folder: &'a str,
    pub install_dir: &'a str,
    /// `Log: <path>` 처럼 접두를 붙인 한 줄.
    pub log_line: &'a str,
}

/// 절을 그리고 `Open folder` 가 눌렸는지 돌려준다.
pub fn plugin_install_paths(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginInstallPathsView<'_>,
) -> bool {
    let mut open = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            label_header(ui, theme, view.label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                open = Button::new(view.open_folder)
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .leading_icon(&|ui, rect, c| {
                        tasty_icons::FOLDER
                            .image(rect.height(), c)
                            .paint_at(ui, rect)
                    })
                    .show(ui, theme)
                    .clicked();
            });
        });
        path_line(ui, theme, view.install_dir);
        path_line(ui, theme, view.log_line);
    });
    open
}

/// 머리글과 폴더 경로 한 줄. `Open folder` 가 없는 사유(예약 id)가 쓴다. 경로는 설치 경로 절과 같다.
pub fn plugin_folder_path(ui: &mut egui::Ui, theme: &Theme, label: &str, path: &str) {
    plugin_detail_section(ui, theme, label, |ui| path_line(ui, theme, path));
}

/// mono caption · text-muted 한 줄. 공백이 없는 경로도 열 폭에서 끊기도록 아무 문자에서 줄바꿈한다.
fn path_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::monospace(theme.font_size_caption.value()),
            color: theme.text_muted().to_egui(),
            ..Default::default()
        },
    );
    job.wrap.break_anywhere = true;
    ui.add(egui::Label::new(job).wrap().selectable(true));
}

#[cfg(test)]
mod section_tests {
    use super::*;

    /// 세로 item_spacing 을 `item_gap` 으로 둔 Ui 에 (앞 줄, 절 머리글, 절 본문) 을 그려 사각형을 돌려준다.
    fn draw(item_gap: f32) -> (egui::Rect, egui::Rect, egui::Rect) {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let ctx = egui::Context::default();
        let mut out = None;
        for _ in 0..2 {
            drop(ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = item_gap;
                    let before = ui.label("before").rect;
                    plugin_detail_section_gap(ui, &theme);
                    let (header, body) = plugin_detail_section(ui, &theme, "Permissions", |ui| {
                        let header = ui.min_rect();
                        (header, ui.label("body").rect)
                    });
                    out = Some((before, header, body));
                });
            }));
        }
        out.expect("drawn")
    }

    #[test]
    fn sections_sit_space_lg_apart_whatever_the_item_spacing() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        for item_gap in [theme.spacing_xs.value(), theme.spacing_sm.value()] {
            let (before, header, _) = draw(item_gap);
            assert_eq!(
                header.top() - before.bottom(),
                theme.spacing_lg.value(),
                "item spacing {item_gap}"
            );
        }
    }

    #[test]
    fn the_section_body_sits_space_sm_under_its_header() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let (_, header, body) = draw(theme.spacing_xs.value());
        assert_eq!(body.top() - header.bottom(), theme.spacing_sm.value());
    }
}
