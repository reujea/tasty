//! 제목·본문·설명·고정폭 글꼴의 크기와 강조를 비교한다.

use tasty_design_tokens::generated::semantic::ICON_SIZE_XS;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::icons::CHEVRON_DOWN;
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

/// 정본 표본 칸의 `size-64`, `size-140`. primitive는 공개되지 않아 예제 치수로 이름을 둔다.
const LABEL_FAMILY_CAP_W: LogicalPx = LogicalPx(64.0);
const LABEL_FAMILY_TOKEN_W: LogicalPx = LogicalPx(140.0);

/// 카테고리 머리글과 개수. `Projects`는 사용자가 지은 이름이라 이 언어들에도 라틴이 온다.
const LABEL_FAMILY_ROWS: [(&str, &str, u32); 4] = [
    ("ja", "ワークスペース", 1),
    ("ja", "Projects", 0),
    ("zh-Hans", "工作区", 1),
    ("zh-Hans", "Projects", 0),
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
        let cap_w = LABEL_FAMILY_CAP_W.value();
        let token_w = LABEL_FAMILY_TOKEN_W.value();
        ui.spacing_mut().item_spacing.y = gap;
        for (language, token, heading, section) in LABEL_FAMILY {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let word_w = ((ui.available_width() - cap_w - token_w - gap * 3.0) / 2.0).max(0.0);
                family_cap(ui, theme, cap_w, language);
                family_cap(ui, theme, token_w, token);
                for label in [heading, section] {
                    ui.allocate_ui(egui::vec2(word_w, 0.0), |ui| {
                        ui.set_min_width(word_w);
                        ui.label(
                            egui::RichText::new(label.to_uppercase())
                                .family(tasty_ui_widgets::label_font_family_for(language))
                                .size(size.value())
                                .extra_letter_spacing(
                                    theme.sidebar_section_heading_tracking(size).value(),
                                )
                                .color(ec(theme.text_muted())),
                        );
                    });
                }
            });
        }
        for (language, name, count) in LABEL_FAMILY_ROWS {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                family_cap(ui, theme, cap_w, language);
                label_family_row(ui, theme, language, name, count);
            });
        }
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
            (
                "line height, baseline in ja, zh-Hans",
                "font-mono's at the same size, measured in the app",
            ),
            ("count", "same baseline as the label"),
            ("icon", "centred in the row"),
            ("Latin in ja, zh-Hans", "UI sans, no second face"),
        ],
        &[
            TokenChip::new(
                "sidebar-category-header-fg",
                "heading and icon ink",
                ec(theme.sidebar_category_header_fg()),
            ),
            TokenChip::new(
                "sidebar-category-header-count-fg",
                "count ink",
                ec(theme.sidebar_category_header_count_fg()),
            ),
            TokenChip::without_color("font-label", "words in a small label"),
            TokenChip::without_color("font-mono", "en, ko; values in every language"),
            TokenChip::without_color("font-ui", "ja, zh-Hans"),
            TokenChip::without_color("sidebar-section-heading-font-size", "10px in this specimen"),
            TokenChip::without_color(
                "sidebar-section-heading-tracking",
                "0.07em in this specimen",
            ),
        ],
    );
}

fn family_cap(ui: &mut egui::Ui, theme: &Theme, width: f32, text: &str) {
    ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
        ui.set_min_width(width);
        ui.label(
            egui::RichText::new(text)
                .monospace()
                .size(theme.font_size_micro.value())
                .color(ec(theme.text_muted())),
        );
    });
}

/// 정본 `LabelFamilyRow`. 머리글 이름과 mono 개수가 한 행에 놓여 baseline을 비교한다.
fn label_family_row(ui: &mut egui::Ui, theme: &Theme, language: &str, name: &str, count: u32) {
    let pad_x = theme.sidebar_category_header_pad_x().value();
    let pad_y = theme.sidebar_category_header_pad_y().value();
    let icon_size = ICON_SIZE_XS.value();
    let fg = ec(theme.sidebar_category_header_fg());
    let size = theme.sidebar_section_heading_font_size;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &name.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(
                size.value(),
                tasty_ui_widgets::label_font_family_for(language),
            ),
            extra_letter_spacing: theme.sidebar_section_heading_tracking(size).value(),
            color: fg,
            ..Default::default()
        },
    );
    let heading = ui.painter().layout_job(job);
    let count = ui.painter().layout_no_wrap(
        count.to_string(),
        egui::FontId::monospace(theme.sidebar_category_header_count_font_size().value()),
        ec(theme.sidebar_category_header_count_fg()),
    );
    let content_h = heading.size().y.max(count.size().y).max(icon_size);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), content_h + pad_y * 2.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.sidebar_category_header_bg().to_egui());
    let border = egui::Stroke::new(
        theme.border_width.value(),
        theme
            .sidebar_category_header_border()
            .to_egui_premultiplied(),
    );
    painter.hline(rect.x_range(), rect.min.y, border);
    painter.hline(rect.x_range(), rect.max.y, border);
    let center_y = rect.center().y;
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.min.x + pad_x + icon_size / 2.0, center_y),
        egui::vec2(icon_size, icon_size),
    );
    CHEVRON_DOWN.image(icon_size, fg).paint_at(ui, icon);
    painter.galley(
        egui::pos2(
            icon.max.x + theme.spacing_xs.value(),
            center_y - heading.size().y / 2.0,
        ),
        heading,
        fg,
    );
    painter.galley(
        egui::pos2(
            rect.max.x - pad_x - count.size().x,
            center_y - count.size().y / 2.0,
        ),
        count,
        ec(theme.sidebar_category_header_count_fg()),
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
