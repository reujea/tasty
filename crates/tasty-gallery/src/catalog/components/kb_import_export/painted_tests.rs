//! 내보내기 실패·파싱 실패·번들 경고 표본의 잉크, 본문 폭, 버튼 바탕을 그려진 도형으로 확인한다
//! (본체 notices_tests와 같은 방식).

use tasty_type_appearance::theme::Theme;

use crate::catalog::components::painted::{Painted, paint};

use super::open_values::draw_open_values;
use super::remaining_values::{draw_remaining_values, unknown_export_failure};

/// 미분류 내보내기 실패 표본: tint 안 글리프는 accent-danger-raised, 제목 글자는 strong, 본문과 OS 줄은
/// text-on-active, Try again 채움은 surface-on-raised, ghost 라벨은 raised 바탕 잉크다.
/// Mocha는 panel·raised ghost 잉크가 같은 값이라 바탕은 Latte가 가른다.
#[test]
fn unknown_export_failure_sample_paints_the_app_inks_on_the_raised_card() {
    let latte = crate::host_shell::latte_theme();
    assert_ne!(
        latte.button_ghost_raised_fg(),
        latte.button_ghost_fg(),
        "Latte must separate the raised ghost ink from the panel one"
    );
    let themes: [Theme; 2] = [tasty_themes::mocha_fallback(), latte];
    for th in themes {
        assert_ne!(th.text_on_active(), th.text_secondary());
        assert_ne!(th.accent_danger_raised(), th.accent_danger_strong());
        let p = paint(|ui| unknown_export_failure(ui, &th));

        let title = p.text(crate::i18n::t(
            "settings.keybindings.ie_export_failure_title",
        ));
        assert_eq!(title.color, th.accent_danger_strong().to_egui(), "title");
        let tint = p.rect_filled_with(
            th.accent_danger()
                .to_egui()
                .gamma_multiply(th.tint_fill_alpha()),
        );
        let glyphs: Vec<egui::Color32> = p
            .glyphs
            .iter()
            .filter(|(r, _)| tint.contains_rect(*r))
            .map(|(_, c)| *c)
            .collect();
        assert_eq!(
            glyphs,
            vec![th.accent_danger_raised().to_egui()],
            "title glyph"
        );
        assert_eq!(
            p.text(" Nothing was written.").color,
            th.text_on_active().to_egui(),
            "body"
        );
        assert_eq!(
            p.text("os error 28").color,
            th.text_on_active().to_egui(),
            "OS line"
        );

        let retry = p.text(crate::i18n::t("settings.keybindings.ie_export_retry"));
        assert!(
            retry.rect.top() >= tint.bottom(),
            "Try again below the tint"
        );
        assert_eq!(
            p.fill_under(retry.rect),
            th.surface_on_raised().to_egui(),
            "Try again fill"
        );
        let other = p.text(crate::i18n::t(
            "settings.keybindings.ie_export_choose_another",
        ));
        assert!(other.rect.top() >= tint.bottom(), "ghost below the tint");
        assert_eq!(
            other.color,
            th.button_ghost_raised_fg().to_egui(),
            "ghost label"
        );
    }
}

fn tint_fill(th: &Theme, tone: egui::Color32) -> egui::Color32 {
    tone.gamma_multiply(th.tint_fill_alpha())
}

fn both_themes() -> [Theme; 2] {
    [
        tasty_themes::mocha_fallback(),
        crate::host_shell::latte_theme(),
    ]
}

/// 번들 경고 블록: 제목·개수(`total`건)·tint 안 글리프는 accent-warning-raised, 보이는 각 줄은
/// text-on-active다.
fn assert_bundle_warning_block(p: &Painted, th: &Theme, total: u64, lines: &[&str]) {
    let raised = th.accent_warning_raised().to_egui();
    let title = p.text(crate::i18n::t("settings.keybindings.ie_notices_title"));
    assert_eq!(title.color, raised, "warning title");
    let tint = p.rect_filled_around(tint_fill(th, th.accent_warning().to_egui()), title.rect);
    assert_eq!(p.glyphs_in(tint), vec![raised], "warning glyph");
    let count = crate::i18n::t_count(
        "settings.keybindings.ie_notices_count",
        total,
        &[&total.to_string()],
    );
    assert_eq!(p.text(&count).color, raised, "warning count");
    for line in lines {
        assert_eq!(
            p.text(line).color,
            th.text_on_active().to_egui(),
            "warning line {line:?}"
        );
    }
}

/// open values 표본: 두 파싱 실패 블록의 제목과 tint 안 글리프는 accent-danger-strong, 본문은 text-secondary로
/// measure-md에서 줄바꿈하고, 번들 경고 블록은
/// accent-warning-raised 머리와 text-on-active 줄, 내보내기 실패 본문은 measure-md에서 줄바꿈한다.
/// Mocha는 accent-warning과 그 raised 값이 같아 경고 머리의 옛 잉크는 Latte가 가른다.
#[test]
fn open_values_sample_paints_parse_failure_and_bundle_warning_inks() {
    let latte = crate::host_shell::latte_theme();
    assert_ne!(
        latte.accent_warning_raised(),
        latte.accent_warning(),
        "Latte must separate the raised warning ink from the base one"
    );
    for th in both_themes() {
        assert_ne!(th.accent_danger_strong(), th.accent_danger_raised());
        assert_ne!(th.text_on_active(), th.text_secondary());
        assert_ne!(th.measure_md.value(), th.measure_lg.value());
        let p = paint(|ui| draw_open_values(ui, &th));

        let strong = th.accent_danger_strong().to_egui();
        let titles = p.texts_with(crate::i18n::t("settings.keybindings.ie_failure_title"));
        assert_eq!(titles.len(), 2, "with and without a line number");
        for title in titles {
            assert_eq!(title.color, strong, "parse failure title");
            let tint =
                p.rect_filled_around(tint_fill(&th, th.accent_danger().to_egui()), title.rect);
            assert_eq!(p.glyphs_in(tint), vec![strong], "parse failure glyph");
        }

        let bodies = p.texts_with("expected a keybinding export");
        assert_eq!(bodies.len(), 2, "with and without a line number");
        for body in bodies {
            assert_eq!(
                body.wrap_width,
                th.measure_md.value(),
                "parse failure body measure"
            );
            assert_eq!(
                body.color,
                th.text_secondary().to_egui(),
                "parse failure body ink"
            );
        }

        assert_bundle_warning_block(
            &p,
            &th,
            4,
            &[
                "Written by a newer schema",
                "2 unknown actions were skipped",
                "The group [keybindings.image] is empty",
            ],
        );

        assert_eq!(
            p.text("the folder is read-only. Nothing was written.")
                .wrap_width,
            th.measure_md.value(),
            "export failure body measure"
        );
    }
}

/// remaining values 표본: 한 건 경고 블록의 머리·줄 잉크와 미분류 실패 본문의 measure-md 폭.
#[test]
fn remaining_values_sample_paints_one_notice_inks_and_measured_body() {
    for th in both_themes() {
        let p = paint(|ui| draw_remaining_values(ui, &th));
        assert_bundle_warning_block(&p, &th, 1, &["1 unknown action was skipped"]);
        assert_eq!(
            p.text(" Nothing was written.").wrap_width,
            th.measure_md.value(),
            "export failure body measure"
        );
    }
}

/// remaining values Meta의 OS 줄 칩은 렌더 값 text-on-active를 이름과 스와치로 보인다. text-muted 칩은 없다.
#[test]
fn remaining_values_meta_names_the_os_line_ink_text_on_active() {
    for th in both_themes() {
        assert_ne!(th.text_on_active(), th.text_muted());
        let p = paint(|ui| draw_remaining_values(ui, &th));
        assert!(
            !p.texts.iter().any(|t| t.text == "text-muted"),
            "no text-muted chip"
        );
        let (chip, swatch) = p.token_chip(th.surface_raised().to_egui(), "— OS reason line");
        assert!(
            p.has_text_in("text-on-active", chip),
            "the OS line chip is named text-on-active"
        );
        assert_eq!(swatch, th.text_on_active().to_egui(), "OS line chip swatch");
    }
}
