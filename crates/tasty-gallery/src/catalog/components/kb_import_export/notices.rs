//! 가져오기·내보내기의 정보, 경고, 오류 안내. 공통 블록에 색과 동작을 지정한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Ground};

use crate::catalog::icons::{self, MockGlyph};

use super::paint::{glyph_at, intro_secondary};
use super::{GROUP_CHEVRON_GAP, IE_DISCARDED};

pub(super) fn dropped_notice(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        glyph_at(
            ui,
            icons::HELP_CIRCLE,
            theme.icon_glyph_size_sm,
            theme.text_muted().to_egui(),
        );
        ui.label(
            egui::RichText::new(format!(
                "2 plugin overrides were dropped — those plugins aren't installed here \
                 ({IE_DISCARDED})."
            ))
            .size(theme.font_size_term_sm.value())
            .color(theme.text_muted().to_egui()),
        );
    });
}

/// 알림 블록의 색 — jsx `IeBlockG`의 tint(채움·테두리), `glyphTone`(글리프), `tone`(제목·개수).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NoticeInk {
    pub(super) tint: egui::Color32,
    pub(super) glyph: egui::Color32,
    pub(super) title: egui::Color32,
}

impl NoticeInk {
    /// tint·글리프·제목이 한 색인 블록.
    pub(super) fn same(tone: egui::Color32) -> Self {
        Self {
            tint: tone,
            glyph: tone,
            title: tone,
        }
    }
}

/// jsx `IeExportFailG` — raised 카드 위 danger tint. 제목은 accent-danger-strong, 글리프는
/// accent-danger-raised다.
pub(super) fn export_failure_ink(theme: &Theme) -> NoticeInk {
    NoticeInk {
        tint: theme.accent_danger().to_egui(),
        glyph: theme.accent_danger_raised().to_egui(),
        title: theme.accent_danger_strong().to_egui(),
    }
}

/// jsx `IeBlockG` 본문 잉크 — text-on-active.
pub(super) fn notice_body_ink(theme: &Theme) -> egui::Color32 {
    theme.text_on_active().to_egui()
}

/// 액션 버튼 행. 눌린 액션의 인덱스를 돌려준다.
fn action_buttons(
    ui: &mut egui::Ui,
    theme: &Theme,
    actions: &[(&str, ButtonVariant)],
    ground: Ground,
) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        for (i, (label, variant)) in actions.iter().enumerate() {
            if Button::new(label)
                .variant(*variant)
                .size(ControlSize::Sm)
                .ground(ground)
                .show(ui, theme)
                .clicked()
            {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// jsx `IeExportFailG`의 실패 블록과 그 아래 버튼. Export 행(raised 카드) 안에서 부른다.
/// 버튼은 tint 밖, 카드의 raised 바탕에 놓인다. 눌린 액션의 인덱스를 돌려준다.
pub(super) fn export_failure_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    body: impl FnOnce(&mut egui::Ui),
    actions: &[(&str, ButtonVariant)],
) -> Option<usize> {
    notice_block(
        ui,
        theme,
        export_failure_ink(theme),
        icons::ALERT_CIRCLE,
        crate::i18n::t("settings.keybindings.ie_export_failure_title"),
        None,
        body,
        &[],
    );
    action_buttons(ui, theme, actions, Ground::Raised)
}

/// 실패 블록 본문 한 문단 — 12 text-on-active.
pub(super) fn failure_body(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(theme.measure_lg.value());
        ui.label(
            egui::RichText::new(text)
                .size(theme.font_size_term_sm.value())
                .color(notice_body_ink(theme)),
        );
    });
}

/// jsx `IeBlockG` — 톤 틴트 배경 + 톤 테두리 · 헤더(글리프 · 제목 · 우측 개수) · 본문 · 액션
/// 행(버튼 라벨과 variant). 눌린 액션의 인덱스를 돌려준다.
// reason: 공통 안내 블록의 색, 아이콘, 제목, 개수, 본문, 동작을 각각 받는다.
#[allow(clippy::too_many_arguments)]
pub(super) fn notice_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    ink: NoticeInk,
    glyph: MockGlyph,
    title: &str,
    count: Option<&str>,
    body: impl FnOnce(&mut egui::Ui),
    actions: &[(&str, ButtonVariant)],
) -> Option<usize> {
    let mut clicked = None;
    egui::Frame::new()
        .fill(ink.tint.gamma_multiply(theme.tint_fill_alpha()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ink.tint.gamma_multiply(theme.tint_border_alpha()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(tasty_ui_widgets::margin_all(theme.spacing_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                glyph_at(ui, glyph, theme.icon_glyph_size_md, ink.glyph);
                ui.label(
                    egui::RichText::new(title)
                        .size(theme.font_size_body.value())
                        .color(ink.title),
                );
                if let Some(count) = count {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(count)
                                .family(tasty_ui_widgets::label_font_family())
                                .size(theme.font_size_caption.value())
                                .color(ink.title),
                        );
                    });
                }
            });
            body(ui);
            if !actions.is_empty() {
                tasty_ui_widgets::vspace(ui, theme.spacing_xs);
                clicked = action_buttons(ui, theme, actions, Ground::Panel);
            }
        });
    clicked
}

/// 파싱 실패 — 보던 대상에 대한 사실이라 toast/popup 이 아니라 detail 영역 안에 인라인.
/// 줄 번호가 없으면 줄 구절을 **빼지 않고 바꾼다**(jsx `IeParseFailG`).
pub(super) fn parse_failure(ui: &mut egui::Ui, theme: &Theme, with_line: bool) {
    let why = if with_line {
        "parsing stopped at line 1."
    } else {
        "the file isn't TOML."
    };
    notice_block(
        ui,
        theme,
        NoticeInk::same(theme.accent_danger().to_egui()),
        icons::ALERT_CIRCLE,
        crate::i18n::t("settings.keybindings.ie_failure_title"),
        None,
        |ui| {
            intro_secondary(
                ui,
                theme,
                &format!(
                    "~/Downloads/settings.json — expected a keybinding export (TOML, a \
                     [keybindings] table); {why} Nothing was changed."
                ),
            );
        },
        &[("Choose another file", ButtonVariant::Secondary)],
    );
}

/// 번들 경고 한 줄 — 흐린 글머리 · 12 text-secondary.
pub(super) fn notice_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GROUP_CHEVRON_GAP.value();
        ui.label(
            egui::RichText::new("·")
                .size(theme.font_size_term_sm.value())
                .color(theme.text_muted().to_egui()),
        );
        ui.label(
            egui::RichText::new(text)
                .size(theme.font_size_term_sm.value())
                .color(theme.text_secondary().to_egui()),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 본체 내보내기 실패 블록과 같은 잉크다. 제목은 strong, 글리프는 raised, 본문은 text-on-active.
    #[test]
    fn export_failure_reads_the_app_inks() {
        for theme in [
            tasty_themes::mocha_fallback(),
            crate::host_shell::latte_theme(),
        ] {
            let ink = export_failure_ink(&theme);
            assert_eq!(ink.title, theme.accent_danger_strong().to_egui(), "title");
            assert_eq!(ink.glyph, theme.accent_danger_raised().to_egui(), "glyph");
            assert_eq!(ink.tint, theme.accent_danger().to_egui(), "tint");
            assert_ne!(ink.title, ink.glyph, "the two inks must differ");
            assert_eq!(notice_body_ink(&theme), theme.text_on_active().to_egui());
        }
    }
}
