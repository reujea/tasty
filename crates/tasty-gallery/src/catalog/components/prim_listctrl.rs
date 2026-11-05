//! 공용 ListCtrl의 선택·비활성 행 예제. 항목별 설명과 Active 태그를 함께 표시한다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{BadgeVariant, ListCtrl, ListCtrlItem, TagVariant, badge, tag};

use crate::catalog::spec::{StageVariant, TokenChip, meta, stage};

/// 시안 Spec 패널의 바깥 폭 `--tasty-size-320`(시안은 border-box라 padding·border 포함).
/// 공개 토큰 접근자가 없어 갤러리 무대 치수로 둔다.
const THEME_PANEL_WIDTH: LogicalPx = LogicalPx(320.0);

thread_local! {
    static SEL: RefCell<usize> = const { RefCell::new(0) };
}

/// ListCtrl — label · description · trailing Tag · chevron · selected · disabled.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                egui::Color32::from(theme.border_default()),
            ))
            .corner_radius(theme.corner_radius.value())
            .show(ui, |ui| {
                ui.set_width(theme.measure_md.value());
                SEL.with(|s| {
                    let mut sel = s.borrow_mut();
                    let active_tag = |ui: &mut egui::Ui, th: &Theme| {
                        tag(ui, th, "Active", TagVariant::Success, true);
                    };
                    let items = [
                        ListCtrlItem::new("Default")
                            .description(crate::i18n::t("settings.keybindings.preset_desc_tasty"))
                            .trailing(&active_tag),
                        ListCtrlItem::new("Mac").description("⌘-based, TextEdit-style"),
                        ListCtrlItem::new("Vim").description("modal, hjkl motions"),
                        ListCtrlItem::new("Custom").disabled(true),
                    ];
                    let out = ListCtrl::new().show(ui, theme, &items, Some(*sel));
                    if let Some(i) = out.clicked {
                        *sel = i;
                    }
                });
            });
    });

    meta(
        ui,
        theme,
        &[
            ("row", "min-height 36 + desc"),
            ("selected", "surface-active + 2px bar"),
            ("divided", "separator hairline"),
            (
                "disabled",
                "state-disabled-fg ink · no chevron · non-selectable",
            ),
            ("disabled trailing", "Tag/Badge → disabled variant"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "selected row",
                egui::Color32::from(theme.listctrl_row_bg_selected()),
            ),
            TokenChip::new(
                "accent-primary",
                "selected left bar",
                egui::Color32::from(theme.listctrl_selected_bar()),
            ),
            TokenChip::new(
                "overlay-hover",
                "hover row",
                theme.listctrl_row_bg_hover().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "text-muted",
                "desc · chevron",
                egui::Color32::from(theme.listctrl_desc_fg()),
            ),
            TokenChip::new(
                "listctrl-row-bg-hover",
                "row hover",
                theme.listctrl_row_bg_hover().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "listctrl-row-bg-selected",
                "selected row",
                egui::Color32::from(theme.listctrl_row_bg_selected()),
            ),
            TokenChip::new(
                "listctrl-selected-bar",
                "accent left bar",
                egui::Color32::from(theme.listctrl_selected_bar()),
            ),
            TokenChip::new(
                "listctrl-label-fg",
                "label",
                egui::Color32::from(theme.listctrl_label_fg()),
            ),
            TokenChip::new(
                "listctrl-desc-fg",
                "description",
                egui::Color32::from(theme.listctrl_desc_fg()),
            ),
            TokenChip::new(
                "listctrl-chevron-fg",
                "drill-in chevron",
                egui::Color32::from(theme.listctrl_chevron_fg()),
            ),
        ],
    );
}

/// disabled 행의 trailing 표지 — ink 규칙. 한 테마의 패널 하나를 그린다.
fn disabled_trailing_panel(ui: &mut egui::Ui, th: &Theme, name: &str) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_default()),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8));
    // 바깥 폭이 시안 값이 되도록 padding·border를 뺀 폭을 콘텐츠에 준다.
    let content_w = THEME_PANEL_WIDTH.value() - frame.total_margin().sum().x;
    frame.show(ui, |ui| {
        // 바깥 horizontal_top의 가로 배치를 물려받지 않게 세로로 쌓는다.
        ui.vertical(|ui| {
            ui.set_width(content_w);
            disabled_trailing_rows(ui, th, name);
        });
    });
}

/// 패널 안 — 테마 이름과 네 행.
fn disabled_trailing_rows(ui: &mut egui::Ui, th: &Theme, name: &str) {
    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
    ui.label(
        egui::RichText::new(name)
            .size(th.font_size_caption.value())
            .color(egui::Color32::from(th.text_muted())),
    );
    let success = |ui: &mut egui::Ui, th: &Theme| {
        tag(ui, th, "Active", TagVariant::Success, true);
    };
    let accent = |ui: &mut egui::Ui, th: &Theme| {
        tag(ui, th, "edited", TagVariant::Accent, false);
    };
    let count = |ui: &mut egui::Ui, th: &Theme| {
        badge(ui, th, "3", BadgeVariant::Primary);
    };
    let items = [
        ListCtrlItem::new("Default")
            .description("enabled · success Tag")
            .trailing(&success),
        ListCtrlItem::new("Readline")
            .description("disabled · success Tag")
            .trailing(&success)
            .disabled(true),
        ListCtrlItem::new("Custom")
            .description("disabled · accent Tag")
            .trailing(&accent)
            .disabled(true),
        ListCtrlItem::new("Imported")
            .description("disabled · Badge")
            .trailing(&count)
            .disabled(true),
    ];
    ui.push_id(("listctrl_disabled_trailing", name), |ui| {
        ListCtrl::new().show(ui, th, &items, Some(0));
    });
}

/// 키보드 활성 행 — 선택 막대 없이 선택 행 채움과 잉크를 쓴다. disabled 행에는 chevron이 없다.
pub fn draw_keyboard_active(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        crate::catalog::spec::cluster(ui, theme, "keyboard-active row", |ui| {
            ui.set_width(theme.measure_md.value());
            let items = [
                ListCtrlItem::new("Selected").description("surface-active + bar"),
                ListCtrlItem::new("Keyboard active").description("surface-active, no bar"),
                ListCtrlItem::new("Rest").description("not touched"),
                ListCtrlItem::new("Archived")
                    .description("no chevron")
                    .disabled(true),
            ];
            ListCtrl::new()
                .active(Some(1))
                .show(ui, theme, &items, Some(0));
        });
    });
}

/// disabled 행의 trailing Tag·Badge — 숨기지 않고 disabled 변형으로 그린다(Mocha·Latte).
pub fn draw_disabled_trailing(ui: &mut egui::Ui, theme: &Theme) {
    // 갤러리 배율을 따르도록 두 팔레트에 현재 zoom을 입힌다.
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            disabled_trailing_panel(ui, &mocha, "Mocha");
            disabled_trailing_panel(ui, &latte, "Latte");
        });
    });

    meta(
        ui,
        theme,
        &[
            ("row ink", "state-disabled-fg"),
            ("chevron", "hidden"),
            ("trailing", "kept · disabled variant (neutral box + ink)"),
            ("accent Tag", "fill drops out"),
            ("tint edge", "→ state-disabled-border"),
            ("opacity", "none"),
        ],
        &[
            TokenChip::new(
                "tag-disabled-bg",
                "→ state-disabled-fill",
                egui::Color32::from(theme.tag_disabled_bg()),
            ),
            TokenChip::new(
                "tag-disabled-border",
                "→ state-disabled-border",
                egui::Color32::from(theme.tag_disabled_border()),
            ),
            TokenChip::new(
                "tag-disabled-fg",
                "→ state-disabled-fg",
                egui::Color32::from(theme.tag_disabled_fg()),
            ),
            TokenChip::new(
                "badge-disabled-bg",
                "→ state-disabled-fill",
                egui::Color32::from(theme.badge_disabled_bg()),
            ),
        ],
    );
}
