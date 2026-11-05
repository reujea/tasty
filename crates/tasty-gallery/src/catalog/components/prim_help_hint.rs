//! 라벨 옆 도움말, 호버 상태, 네 방향 툴팁 배치 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{HelpHint, TooltipPlacement};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, note, stage};

/// 4 placement 강제 open 데모의 예시 문장 — max-width 240 에서 줄바꿈을 보이도록 다문장.
const SAMPLE: &str =
    "Shows a short explanation on hover. Wraps at 240px so two or three sentences stay readable.";

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        cluster(ui, theme, "in a settings row — the common case", |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            ui.label(
                egui::RichText::new("Blink cursor")
                    .size(theme.font_size_body.value())
                    .color(egui::Color32::from(theme.text_primary())),
            );
            HelpHint::new("When on, the block cursor blinks in focused terminals.")
                .placement(TooltipPlacement::Top)
                .show(ui, theme);
        });

        cluster(ui, theme, "rest · hover (hover the right glyph)", |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            HelpHint::new(SAMPLE).show(ui, theme);
            HelpHint::new(SAMPLE)
                .placement(TooltipPlacement::Bottom)
                .show(ui, theme);
        });

        cluster(
            ui,
            theme,
            "placement — top · bottom · left · right",
            |ui| {
                // 동시에 보이는 툴팁의 ID가 겹치지 않도록 한다.
                ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
                for (place, key) in [
                    (TooltipPlacement::Top, "hh_top"),
                    (TooltipPlacement::Bottom, "hh_bottom"),
                    (TooltipPlacement::Left, "hh_left"),
                    (TooltipPlacement::Right, "hh_right"),
                ] {
                    HelpHint::new(SAMPLE)
                        .placement(place)
                        .open(true)
                        .id_source(key)
                        .show(ui, theme);
                }
            },
        );
    });

    note(
        ui,
        theme,
        "HelpHint = the (?) glyph + a Tooltip. The bubble is opaque (surface-raised, \
         border-strong, popover shadow) with no arrow — never egui's default on_hover_text.",
    );

    meta(
        ui,
        theme,
        &[
            ("glyph", "14px (?) · icon-size-sm"),
            ("gap to label", "4px · space-xs"),
            ("bubble", "surface-raised card · no arrow"),
            ("max-width", "240px · wraps"),
            (
                "over native content",
                "tab strip · pane head: top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip — Tab strips › Tooltips in the strip open upward. No anchor cell (outside a strip): top clamped, 4 from the window edge",
            ),
            ("text", "caption 11 · line-height 1.4"),
            ("delay", "150ms hover"),
        ],
        &[
            TokenChip::new(
                "text-muted",
                "rest glyph",
                egui::Color32::from(theme.text_muted_raised()),
            ),
            TokenChip::new(
                "text-secondary",
                "hover glyph · bubble text",
                egui::Color32::from(theme.text_secondary_raised()),
            ),
            TokenChip::new(
                "surface-raised",
                "bubble bg",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::new(
                "border-strong",
                "bubble border",
                egui::Color32::from(theme.border_strong()),
            ),
            TokenChip::new(
                "help-hint-color",
                "rest glyph",
                egui::Color32::from(theme.help_hint_color()),
            ),
            TokenChip::new(
                "help-hint-color-hover",
                "hover glyph",
                egui::Color32::from(theme.help_hint_color_hover()),
            ),
            TokenChip::without_color("help-hint-gap", "label → (?)"),
        ],
    );

    meta(
        ui,
        theme,
        &[
            ("bg", "tooltip-bg"),
            ("border", "1px tooltip-border"),
            ("radius", "tooltip-radius"),
            ("max-width", "240 tooltip-max-width"),
            ("placement", "top / bottom / left / right"),
            (
                "over native content",
                "tab strip · pane head: top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip — Layouts › Pane tab strip. No anchor cell (outside a strip): top clamped, 4 from the window edge",
            ),
            ("delay", "150ms tooltip-delay"),
        ],
        &[
            TokenChip::new(
                "tooltip-bg",
                "bubble fill",
                egui::Color32::from(theme.tooltip_bg()),
            ),
            TokenChip::new(
                "tooltip-border",
                "1px edge",
                egui::Color32::from(theme.tooltip_border()),
            ),
            TokenChip::new(
                "tooltip-fg",
                "copy",
                egui::Color32::from(theme.tooltip_fg()),
            ),
            TokenChip::without_color("tooltip-shadow", "lift"),
        ],
    );
}
