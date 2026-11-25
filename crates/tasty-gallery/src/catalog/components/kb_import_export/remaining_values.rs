//! 알 수 없는 내보내기 실패와 경고가 한 건일 때의 안내 예제.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::ButtonVariant;

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

use super::entry::action_row;
use super::notices::{
    bundle_notices_ink, export_failure_block, failure_body, notice_block, notice_line,
};
use super::paint::caption;
use super::{IE_FILE, SPECIMEN_W};

/// 분류하지 못한 오류에 쓸 고정 문구. OS 메시지는 별도 줄에 표시한다.
fn unknown_clause() -> &'static str {
    crate::i18n::t("settings.keybindings.ie_export_failure_unknown")
}
/// OS 가 낸 문장 — 문장 안이 아니라 아래 제 줄에 싣는다. 한 줄, 말줄임, 전문은 tooltip.
const OS_MESSAGE: &str = "os error 28: No space left on device";
/// 알림이 하나뿐인 경고 블록의 그 한 줄 — 개수 자리도, 줄 자신도 단수형이다.
const ONE_NOTICE: &str = "1 unknown action was skipped (tab.pin).";

pub fn draw_remaining_values(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        ui.vertical(|ui| {
            ui.set_width(SPECIMEN_W.value());
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            caption(ui, theme, "1 · export failure — cause unknown to Tasty");
            unknown_export_failure(ui, theme);
            caption(ui, theme, "2 · one notice — singular header, no fold link");
            one_notice(ui, theme);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "clause set",
                "read-only · permission · disk full · the write didn't finish.",
            ),
            (
                "OS text",
                "own line, mono 11, muted — never in the sentence",
            ),
            (
                "first / last sentence",
                "never change (as in parse failure)",
            ),
            ("one line", "ellipsised; the full text is the tooltip"),
            (
                "actions",
                "unchanged — Try again · Choose another location…",
            ),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "failure block",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new("text-muted", "OS reason line", theme.text_muted().to_egui()),
            TokenChip::without_color(
                "settings-content-max-width",
                "620 — now set once on the settings content column",
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Tasty selects one of four translated explanations: read-only, permission denied, disk full, or an unfinished write. The raw OS message appears on a separate line so it does not break the translated sentence.",
    );
    spec::dont(
        ui,
        theme,
        "Don't build the OS message into the sentence (\"… — os error 28: No space left on \
         device.\"). It breaks the sentence's grammar in three languages and buries the two parts \
         the user can act on: which path, and that nothing was written.",
    );
}

/// jsx `IeExportFailG reason="other"` — 같은 Export 행과 블록, catch-all 구절 + OS 줄 하나.
pub(super) fn unknown_export_failure(ui: &mut egui::Ui, theme: &Theme) {
    let mut notice = |ui: &mut egui::Ui| {
        export_failure_block(
            ui,
            theme,
            |ui| {
                failure_body(
                    ui,
                    theme,
                    &format!(
                        "~/tasty/{IE_FILE} — {} Nothing was written.",
                        unknown_clause()
                    ),
                );
                os_reason_line(ui, theme, OS_MESSAGE);
            },
            &[
                ("Try again", ButtonVariant::Secondary),
                (
                    crate::i18n::t("settings.keybindings.ie_export_choose_another"),
                    ButtonVariant::Ghost,
                ),
            ],
        );
    };
    action_row(
        ui,
        theme,
        icons::DOWNLOAD,
        crate::i18n::t("settings.keybindings.ie_export_title"),
        crate::i18n::t("settings.keybindings.ie_export_desc"),
        crate::i18n::t("settings.keybindings.ie_export_button"),
        ButtonVariant::Secondary,
        false,
        None,
        Some(&mut notice as &mut dyn FnMut(&mut egui::Ui)),
    );
}

/// jsx `IeBundleNoticesG one` — 헤더 개수와 그 한 줄이 모두 단수형이고, 접을 것이 없으므로
/// 액션 행이 비어 있다.
fn one_notice(ui: &mut egui::Ui, theme: &Theme) {
    notice_block(
        ui,
        theme,
        bundle_notices_ink(theme),
        icons::ALERT_TRIANGLE,
        "Read with warnings",
        Some(&crate::i18n::t_count(
            "settings.keybindings.ie_notices_count",
            1,
            &["1"],
        )),
        |ui| notice_line(ui, theme, ONE_NOTICE),
        &[],
    );
}

/// OS 가 낸 문장 한 줄 — 본문 아래 `space-xs`, mono caption, text-on-active, 한 줄 말줄임.
pub(super) fn os_reason_line(ui: &mut egui::Ui, theme: &Theme, message: &str) {
    ui.add_space(theme.spacing_xs.value());
    ui.add(
        egui::Label::new(
            egui::RichText::new(message)
                .size(theme.font_size_caption.value())
                .family(egui::FontFamily::Monospace)
                .color(super::notices::notice_body_ink(theme)),
        )
        .truncate(),
    )
    .on_hover_text(message);
}
