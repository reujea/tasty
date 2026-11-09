//! 제목·본문·설명·고정폭 글꼴의 크기와 강조를 비교한다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog";

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        scale_row(
            ui,
            theme,
            "heading",
            egui::RichText::new(SAMPLE)
                .size(theme.font_size_heading.value())
                .strong(),
        );
        scale_row(
            ui,
            theme,
            "body",
            egui::RichText::new(SAMPLE).size(theme.font_size_body.value()),
        );
        scale_row(
            ui,
            theme,
            "caption",
            egui::RichText::new(SAMPLE).size(theme.font_size_caption.value()),
        );
        scale_row(
            ui,
            theme,
            "mono",
            egui::RichText::new(SAMPLE)
                .monospace()
                .size(theme.font_size_max.value()),
        );
    });

    meta(
        ui,
        theme,
        &[
            (
                "UI cap",
                "font-size-max 14px at scale 1; larger roles are separate",
            ),
            (
                "heading",
                "body size; strong color approximates design weight 600",
            ),
            ("mono", "D2Coding, term/code surfaces"),
        ],
        &[
            TokenChip::without_color("font-ui", "chrome (system sans)"),
            TokenChip::without_color("font-mono", "D2Coding — code/term"),
            TokenChip::without_color("font-size-body", "13px default"),
            TokenChip::without_color("font-size-caption", "11px meta"),
            TokenChip::without_color("font-weight-semibold", "600 = heading"),
        ],
    );
    note(
        ui,
        theme,
        "디자인의 제목은 본문과 같은 기본 크기에 굵기 600을 사용한다. 이 egui 예제의 RichText::strong은 글꼴 굵기를 바꾸지 않고 강조색을 선택하므로 그 차이를 색으로 근사한다.",
    );
    draw_label_family(ui, theme);
}

/// 디자인 정본의 `label-family` 표본. 한 프로세스는 한 언어로 실행되므로 네 언어를 함께 보이려고 행마다
/// family를 직접 고른다. 문구는 앱 언어 파일의 머리글과 설정 절 제목이다.
const LABEL_FAMILY: [(&str, &str, &str, &str); 4] = [
    ("en", "font-mono", "Workspaces", "Attach connections"),
    ("ko", "font-mono", "워크스페이스", "Attach 연결"),
    ("ja", "font-ui", "ワークスペース", "アタッチ接続"),
    ("zh-Hans", "font-ui", "工作区", "附加连接"),
];

fn draw_label_family(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(theme.spacing_lg.value());
    note(
        ui,
        theme,
        "label-family: small translated labels read font-label. English and Korean use mono; \
         Japanese and Simplified Chinese use UI sans because mono lacks some of their Han glyphs. \
         Paths, IDs and other values stay mono in every language.",
    );
    stage(ui, theme, StageVariant::Column, |ui| {
        let size = theme.sidebar_section_heading_font_size;
        let gap = theme.spacing_sm.value();
        egui::Grid::new("label_family")
            .spacing(egui::vec2(gap, gap))
            .show(ui, |ui| {
                for (language, token, heading, section) in LABEL_FAMILY {
                    for cap in [language, token] {
                        ui.label(
                            egui::RichText::new(cap)
                                .monospace()
                                .size(theme.font_size_micro.value())
                                .color(ec(theme.text_muted())),
                        );
                    }
                    for label in [heading, section] {
                        ui.label(
                            egui::RichText::new(label.to_uppercase())
                                .family(tasty_ui_widgets::label_font_family_for(language))
                                .size(size.value())
                                .extra_letter_spacing(
                                    theme.sidebar_section_heading_tracking(size).value(),
                                )
                                .color(ec(theme.text_muted())),
                        );
                    }
                    ui.end_row();
                }
            });
    });
    meta(
        ui,
        theme,
        &[
            ("family", "font-label"),
            ("language rule", "tokens/base.css, RULES section 8"),
            (
                "size, ink, tracking, uppercase",
                "the label's own, in every language",
            ),
        ],
        &[
            TokenChip::without_color("font-label", "words in a small label"),
            TokenChip::without_color("font-mono", "en, ko; values in every language"),
            TokenChip::without_color("font-ui", "ja, zh-Hans"),
        ],
    );
}

/// typeScaleRow — [150px 라벨(토큰 + px)] [샘플].
fn scale_row(ui: &mut egui::Ui, theme: &Theme, name: &str, sample: egui::RichText) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        ui.allocate_ui(
            egui::vec2(
                theme.tab_width.value(),
                theme.item_height_interactive.value(),
            ),
            |ui| {
                ui.label(
                    egui::RichText::new(name)
                        .monospace()
                        .size(theme.font_size_micro.value())
                        .color(ec(theme.text_muted())),
                );
            },
        );
        ui.label(sample.color(ec(theme.text_primary())));
    });
}
