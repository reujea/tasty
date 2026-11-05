//! 수식키 글리프(cmdKey · optionKey · shiftKey)가 쓰이는 두 자리 — 키캡 칩과
//! Settings 의 수식키 표시 스타일 드롭다운. 시안 `icons.jsx` 의 "Modifier symbols in use".
//! 아래에 `option` 토큰의 OS 별 표기(시안 `overlays-windows-b12.jsx` 의 "Win · Super as option")를 둔다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{KbRecordSlot, KbdKey, kb_record_slot, kbd_parts};

use crate::catalog::icons::{CHECK, CHEVRON_DOWN, CMD_KEY, MockGlyph, OPTION_KEY, SHIFT_KEY};
use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};

// 시안 전시 치수. 대응 토큰이 없다.
/// 설정 행 라벨 칸 폭 — `width: 128`.
const ROW_LABEL_W: LogicalPx = LogicalPx(128.0);
/// 펼친 옵션 목록 폭 — `width: 168`.
const MENU_W: LogicalPx = LogicalPx(168.0);
/// 옵션 행 사이 — `gap: 1`.
const MENU_ROW_GAP: LogicalPx = LogicalPx(1.0);
/// 키캡 칩 줄의 칸 사이 — `gap: 20`.
const CHIP_ROW_GAP: LogicalPx = LogicalPx(20.0);
/// 두 묶음 사이 — 무대 `gap: 30`.
const STAGE_GAP: LogicalPx = LogicalPx(30.0);
/// OS 별 표기 표의 OS 이름 칸 — 시안 `gridTemplateColumns: "72px 140px 1fr"` 의 72.
const OS_LABEL_W: LogicalPx = LogicalPx(72.0);

/// `option` 토큰의 OS 별 표기 — (OS, 녹화 슬롯에 보이는 조합, 키캡 낱말). 시안 `wOS`.
/// Windows·Linux 는 아이콘 없이 낱말이고 Linux 는 데스크톱과 관계없이 Super 다. macOS 줄은 시안의
/// 기호 대신 글자 표시 스타일로 적는다 — 기호는 위 키캡처럼 벡터 글리프로만 그린다.
const OPTION_BY_OS: [(&str, &str, &str); 3] = [
    ("macOS", "Ctrl+Option+K", "Option"),
    ("Windows", "Ctrl+Win+K", "Win"),
    ("Linux", "Super+Shift+1", "Super"),
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing.x = STAGE_GAP.value();
        cluster(
            ui,
            theme,
            "keycap chip — text style vs. symbol style",
            |ui| {
                ui.spacing_mut().item_spacing.x = CHIP_ROW_GAP.value();
                kbd_parts(ui, theme, &[KbdKey::Text("Cmd"), KbdKey::Text("Option")]);
                ui.label(
                    egui::RichText::new("→")
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted_raised().to_egui()),
                );
                kbd_parts(
                    ui,
                    theme,
                    &[KbdKey::Icon(CMD_KEY), KbdKey::Icon(OPTION_KEY)],
                );
                kbd_parts(ui, theme, &[KbdKey::Icon(SHIFT_KEY), KbdKey::Text("4")]);
            },
        );
        cluster(
            ui,
            theme,
            "settings dropdown — trigger (symbol style) + open option list",
            |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    for (label, glyph) in [
                        ("Cmd key display:", CMD_KEY),
                        ("Option key display:", OPTION_KEY),
                        ("Shift key display:", SHIFT_KEY),
                    ] {
                        settings_row(ui, theme, label, glyph);
                    }
                });
                option_menu(ui, theme);
            },
        );
    });

    stage(ui, theme, StageVariant::Wrap, |ui| {
        cluster(ui, theme, "option per OS — Win · Super as words", |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                for (os, combo, word) in OPTION_BY_OS {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                        ui.allocate_ui(egui::vec2(OS_LABEL_W.value(), 0.0), |ui| {
                            ui.set_width(OS_LABEL_W.value());
                            ui.label(
                                egui::RichText::new(os)
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_secondary_raised().to_egui()),
                            );
                        });
                        kb_record_slot(
                            ui,
                            theme,
                            KbRecordSlot::Binding(combo),
                            theme.kb_record_width(),
                            true,
                        );
                        kbd_parts(ui, theme, &[KbdKey::Text(word)]);
                    });
                }
            });
        });
    });
    note(
        ui,
        theme,
        "option is Option (display setting: text or glyph) on macOS, Win on Windows and Super on \
         Linux on every desktop, KDE included. Words, no logo glyph. The display-style dropdown \
         stays macOS-only. The OS-reserved and OS key name captions are in Keybindings › Plugins.",
    );

    meta(
        ui,
        theme,
        &[
            ("names", "cmdKey · optionKey · shiftKey"),
            (
                "keycap chip size",
                "14px (matches the 12px mono keycap label)",
            ),
            ("dropdown size", "16px — Icon md default"),
            ("closed trigger", "glyph only"),
            ("option row", "glyph + text label + check on the active row"),
            (
                "color",
                "inherits currentColor from the cap / row (text-secondary at rest)",
            ),
        ],
        &[
            TokenChip::new(
                "text-secondary",
                "keycap glyph",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "cap / trigger fill",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "border-strong",
                "cap border",
                theme.border_strong().to_egui(),
            ),
            TokenChip::new(
                "overlay-hover",
                "active option row",
                theme.overlay_hover().to_egui_premultiplied(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "No new tokens — the glyphs drop into the existing Kbd cap and menu-row recipes \
         untouched. The + joiner between caps stays text; only the key faces become vectors.",
    );
    note(
        ui,
        theme,
        "The body settings row for the first key reads \u{201c}Alt key display:\u{201d} \
         (settings.general.alt_display_style_label in lang/en.toml); this spec keeps the kit \
         \u{201c}Cmd key display:\u{201d} label.",
    );
    dont(
        ui,
        theme,
        // 갤러리 UI 폰트에 ⌥·⇧ 글자가 없어 시안의 ⌘ / ⌥ / ⇧ 대신 코드 포인트로 적는다.
        "Don't type the Command / Option / Shift symbols (U+2318 / U+2325 / U+21E7) as unicode \
         text anywhere in the product — that is the bug these glyphs exist to fix. And don't reach for command when you mean the ⌘ keycap.",
    );
}

/// 설정 행 — 128 라벨 + 12 + 닫힌 트리거(글리프 단독 + chevronDown).
fn settings_row(ui: &mut egui::Ui, theme: &Theme, label: &str, glyph: MockGlyph) {
    ui.horizontal(|ui| {
        ui.set_min_height(theme.settings_row_min_height().value());
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ROW_LABEL_W.value(), theme.font_size_body.value()),
            egui::Sense::hover(),
        );
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(theme.font_size_body.value()),
                theme.text_secondary().to_egui(),
            )
        });
        let pos = egui::pos2(rect.left(), rect.center().y - galley.rect.height() * 0.5);
        ui.painter()
            .galley(pos, galley, theme.text_secondary().to_egui());
        trigger(ui, theme, glyph);
    });
}

/// 닫힌 트리거 — control-height · 좌우 space-sm · 글리프 16 + 8 + chevron 14.
fn trigger(ui: &mut egui::Ui, theme: &Theme, glyph: MockGlyph) {
    let pad = theme.spacing_sm.value();
    let gap = theme.spacing_sm.value();
    let g = theme.icon_glyph_size_md.value();
    let ch = theme.icon_glyph_size_sm.value();
    let size = egui::vec2(
        pad * 2.0 + g + gap + ch,
        theme.item_height_interactive.value(),
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(
        rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let ink = theme.text_primary().to_egui();
    let cy = rect.center().y;
    let glyph_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad, cy - g * 0.5),
        egui::vec2(g, g),
    );
    glyph.image(g, ink).paint_at(ui, glyph_rect);
    let chev_rect = egui::Rect::from_min_size(
        egui::pos2(glyph_rect.right() + gap, cy - ch * 0.5),
        egui::vec2(ch, ch),
    );
    CHEVRON_DOWN.image(ch, ink).paint_at(ui, chev_rect);
}

/// 펼친 옵션 목록 — 텍스트 행과 활성 Symbol 행(글리프 + 라벨 + 체크).
fn option_menu(ui: &mut egui::Ui, theme: &Theme) {
    let pad = theme.popup_content_margin().value();
    let bw = theme.border_width.value();
    egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(bw, theme.menu_border().to_egui()))
        .corner_radius(theme.menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| {
            let inner_w = MENU_W.value() - (pad + bw) * 2.0;
            ui.set_width(inner_w);
            // 프레임 안쪽은 감싸는 줄바꿈 줄의 가로 배치를 물려받으므로 세로로 다시 쌓는다.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = MENU_ROW_GAP.value();
                let text = "Text — \u{201c}Option\u{201d}";
                option_row(ui, theme, inner_w, text, None, false);
                option_row(ui, theme, inner_w, "Symbol", Some(OPTION_KEY), true);
            });
        });
}

fn option_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: f32,
    label: &str,
    glyph: Option<MockGlyph>,
    active: bool,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.menu_item_height().value()),
        egui::Sense::hover(),
    );
    if active {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }
    let ink = if active {
        theme.text_primary().to_egui()
    } else {
        theme.text_secondary().to_egui()
    };
    let pad = theme.spacing_sm.value();
    let gap = theme.spacing_sm.value();
    let slot = theme.icon_glyph_size_md.value();
    let cy = rect.center().y;
    if let Some(g) = glyph {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.left() + pad, cy - slot * 0.5),
            egui::vec2(slot, slot),
        );
        g.image(slot, ink).paint_at(ui, r);
    }
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(theme.font_size_body.value()),
            ink,
        )
    });
    let pos = egui::pos2(
        rect.left() + pad + slot + gap,
        cy - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, ink);
    if active {
        let ch = theme.icon_glyph_size_sm.value();
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.right() - pad - ch, cy - ch * 0.5),
            egui::vec2(ch, ch),
        );
        CHECK.image(ch, ink).paint_at(ui, r);
    }
}
