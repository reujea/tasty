//! Keybindings 서브탭(General ~ Scripts)의 바인딩 행 예제. 실제 녹화는 하지 않는다.
//!
//! 라벨 열은 다른 설정 행과 같이 서브탭의 가장 긴 라벨을 150 … 240 으로 clamp 한 폭이다. 라벨이 짧아도
//! 열을 그대로 차지해 바인딩 버튼이 같은 x 에서 시작한다. 열보다 긴 라벨과 사용자 스크립트 이름은 열
//! 안에서 줄을 바꾸고, 도움말 아이콘은 마지막 단어 뒤에 붙는다. 첫 버튼이 라벨 옆에 들어가지 않는
//! 좁은 폭에서는 행이 쌓인다(본체와 같은 `settings_stack_row`).

use tasty_settings::SwitchAxis;
use tasty_settings::keybindings::{
    KeybindingSettings, explorer_drag_flip_modifier_options, quick_switch_modifier_options,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    KbRecordSlot, SettingsRow, StackRow, kb_record_slot, select, settings_label_column,
    settings_stack_row, vspace,
};

use crate::catalog::modifier_label;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::{t, t_fmt};

/// 예제 프레임 폭 — 설정 콘텐츠 컬럼 상한(620)을 넘지 않는다.
const WIDTH: LogicalPx = LogicalPx(600.0);

/// (라벨, 도움말, 바인딩들). 바인딩이 없으면 "None" 슬롯을 그린다.
const ROWS: &[(&str, Option<&str>, &[&str])] = &[
    ("New workspace:", None, &["Ctrl+Shift+N"]),
    (
        "Close active:",
        Some("Closes in order: tab → pane → workspace."),
        &["Ctrl+W"],
    ),
    ("Quit:", None, &["Ctrl+Q", "Ctrl+Shift+Q"]),
    ("アクティブ項目を閉じる:", None, &[]),
    (
        "Explorer: reveal the current path in the folder tree:",
        Some("Expands the tree down to the folder of the focused surface."),
        &["Ctrl+Alt+E"],
    ),
    (
        "deploy-staging-and-rotate-every-log-on-all-the-hosts-tonight.lua",
        None,
        &["Ctrl+Alt+1"],
    ),
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
                // 행 사이는 kb-row-gap, 구분선 없음. 행 안 버튼 사이는 space-xs 다.
                ui.spacing_mut().item_spacing.y = theme.kb_row_gap().value();
                let flip = drag_flip_row();
                let measured: Vec<SettingsRow<'_>> = ROWS
                    .iter()
                    .map(|(label, hint, _)| {
                        let row = SettingsRow::new(label);
                        match hint {
                            Some(h) => row.hint(h),
                            None => row,
                        }
                    })
                    .chain([flip])
                    .collect();
                let col = settings_label_column(ui, theme, &measured);
                for (i, (label, hint, bindings)) in ROWS.iter().enumerate() {
                    ui.push_id(i, |ui| row(ui, theme, col, label, *hint, bindings));
                }
                vspace(ui, theme.spacing_sm);
                ui.separator();
                vspace(ui, theme.spacing_xs);
                drag_flip(ui, theme, col, flip);
            });
        });
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
                ui.spacing_mut().item_spacing.y = theme.kb_row_gap().value();
                switch_modifier_section(ui, theme);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "label column",
                "longest label of the subtab, clamp 150 … 240 · holds its width",
            ),
            ("gap", "16 label → first slot"),
            (
                "hint",
                "HelpHint inside the label column, after the last word",
            ),
            (
                "long label",
                "wraps inside the column (long labels, script names)",
            ),
            (
                "slot",
                "kb-record-width 140 × kb-record-height 24 · mono caption · surface-raised · 1px kb-record-border, kb-record-border-hover on hover · + (kb-record-add-width 32, border only, plus icon sm) adds a slot · a row with no binding is one None slot (border only, kb-record-empty-fg, no +)",
            ),
            (
                "rows",
                "kb-row-gap 8 between rows · no divider · 4 between slots",
            ),
            (
                "drag flip row",
                "General · after the entries · modifier Select field-width-md · caption · default first",
            ),
            (
                "switch modifier row",
                "Tab / Workspace / Category subtab · first row · plain Select field-width-md · no caption · modifier combos, a config value not in the list, then Individual",
            ),
        ],
        &[
            TokenChip::without_color("settings-label-width", "label column floor 150"),
            TokenChip::without_color("settings-label-max-width", "label column cap 240"),
            TokenChip::without_color("settings-label-gap", "label → slot"),
            TokenChip::without_color("kb-record-width", "→ kb-ie-slot-min-width 140"),
            TokenChip::without_color("kb-record-height", "→ kb-ie-slot-height 24"),
            TokenChip::without_color("kb-record-add-width", "→ size-32"),
            TokenChip::without_color("kb-row-gap", "→ space-sm 8"),
            TokenChip::new(
                "text-secondary",
                "label",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new("surface-raised", "slot", theme.surface_raised().to_egui()),
            TokenChip::new(
                "kb-record-border",
                "→ border-default · slot edge",
                theme.kb_record_border().to_egui(),
            ),
            TokenChip::new(
                "kb-record-border-hover",
                "→ border-strong · hover",
                theme.kb_record_border_hover().to_egui(),
            ),
            TokenChip::new(
                "kb-record-empty-fg",
                "→ text-muted · None",
                theme.kb_record_empty_fg().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Mirrors the app's keybinding rows. A short label no longer pulls its slots left: the label cell always takes the full column.",
    );
}

/// General 서브탭 끝의 탐색기 드래그 반전 modifier 행.
pub(super) fn drag_flip_row() -> SettingsRow<'static> {
    SettingsRow::new(t("settings.keybindings.explorer_drag_flip_modifier_label"))
        .caption(t("settings.keybindings.explorer_drag_flip_modifier_hint"))
}

pub(super) fn drag_flip(ui: &mut egui::Ui, theme: &Theme, col: LogicalPx, row: SettingsRow<'_>) {
    let names = explorer_drag_flip_modifier_options("");
    let labels: Vec<String> = names.iter().map(|n| modifier_label(n)).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut selected = 0;
    row.show(ui, theme, col, |ui| {
        select(
            ui,
            theme,
            "gallery_kb_drag_flip",
            &mut selected,
            &labels,
            theme.field_width_md.value(),
            true,
        );
    });
}

/// Workspace 서브탭의 전환 수식키 행과 그 아래 슬롯 행 둘. 수식키 행은 캡션 없는 Select다.
fn switch_modifier_section(ui: &mut egui::Ui, theme: &Theme) {
    let axis = SwitchAxis::Workspace;
    let label = t(axis.modifier_label_key());
    // 번역 문구에 이미 콜론이 있다.
    let slot = t_fmt("settings.keybindings.workspace_switch_slot_label", "1");
    let next = t("settings.keybindings.workspace_switch_next_label");
    let measured = [label, slot.as_str(), next].map(SettingsRow::new);
    let col = settings_label_column(ui, theme, &measured);
    let defaults = KeybindingSettings::default();
    let current = defaults.workspace_switch_modifier.clone();
    // 슬롯 행은 본체처럼 수식키와 단일 키를 합친 조합을 보인다.
    let composed = |key: &str| modifier_label(&format!("{current}+{key}"));
    let slot_combo = composed(defaults.workspace_slot_key(0).unwrap_or_default());
    let next_combo = composed(defaults.workspace_next_key());
    let names = quick_switch_modifier_options(&current);
    let labels: Vec<String> = names
        .iter()
        .map(|n| {
            if n == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER {
                t("settings.keybindings.quick_switch_individual_label").to_string()
            } else {
                modifier_label(n)
            }
        })
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut selected = names.iter().position(|n| *n == current).unwrap_or(0);
    let frame = StackRow {
        id: ui.id().with("gallery_kb_switch_modifier"),
        label_col: col,
        row_h: theme.kb_record_height(),
        label,
        hint: None,
        align_top: false,
    };
    settings_stack_row(ui, theme, frame, |ui| {
        select(
            ui,
            theme,
            "gallery_kb_switch_modifier",
            &mut selected,
            &labels,
            theme.field_width_md.value(),
            true,
        );
    });
    for (label, combo) in [(slot.as_str(), &slot_combo), (next, &next_combo)] {
        let frame = StackRow {
            id: ui.id().with(("gallery_kb_switch_slot", label)),
            label_col: col,
            row_h: theme.kb_record_height(),
            label,
            hint: None,
            align_top: true,
        };
        settings_stack_row(ui, theme, frame, |ui| {
            kb_record_slot(
                ui,
                theme,
                KbRecordSlot::Binding(combo),
                theme.kb_record_width(),
                true,
            );
        });
    }
}

pub(super) fn row(
    ui: &mut egui::Ui,
    theme: &Theme,
    col: LogicalPx,
    label: &str,
    hint: Option<&str>,
    bindings: &[&str],
) {
    let frame = StackRow {
        id: ui.id().with(("gallery_kb_row", label)),
        label_col: col,
        row_h: theme.kb_record_height(),
        label,
        hint,
        align_top: true,
    };
    settings_stack_row(ui, theme, frame, |ui| {
        ui.horizontal_wrapped(|ui| {
            let gap = theme.spacing_xs.value();
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for combo in bindings {
                kb_record_slot(
                    ui,
                    theme,
                    KbRecordSlot::Binding(combo),
                    theme.kb_record_width(),
                    true,
                );
            }
            // 바인딩이 없는 행은 None 슬롯 하나, 있으면 끝에 + 버튼이다.
            if bindings.is_empty() {
                kb_record_slot(
                    ui,
                    theme,
                    KbRecordSlot::Empty("None"),
                    theme.kb_record_width(),
                    true,
                );
            } else {
                kb_record_slot(
                    ui,
                    theme,
                    KbRecordSlot::Add,
                    theme.kb_record_add_width(),
                    true,
                );
            }
        });
    });
}
