//! Attach 탭 표본 Meta의 inactive name·inactive badge 칩을 그려진 도형으로 확인한다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::components::painted::paint;

use super::draw_attach;

/// inactive name 칩은 text-muted를 이름과 스와치로 보이고, 그 스와치는 표본의 inactive 행 이름 잉크와 같다.
#[test]
fn attach_meta_names_the_inactive_name_ink_text_muted() {
    let themes: [Theme; 2] = [
        tasty_themes::mocha_fallback(),
        crate::host_shell::latte_theme(),
    ];
    for th in themes {
        assert_ne!(th.text_muted(), th.text_disabled());
        let p = paint(|ui| draw_attach(ui, &th));
        let (chip, swatch) = p.token_chip(th.surface_raised().to_egui(), "— inactive name");
        assert!(
            p.has_text_in("text-muted", chip),
            "the inactive name chip is named text-muted"
        );
        assert_eq!(swatch, th.text_muted().to_egui(), "chip swatch");
        let name = p
            .texts
            .iter()
            .find(|t| t.text == "legacy-attach")
            .expect("inactive attach row name");
        assert_eq!(name.color, swatch, "the row name reads the chip's ink");
    }
}

/// inactive badge 칩은 accent-warning 이름에 raised 스와치를 보인다. Mocha는 accent-warning과
/// raised가 같은 값이라 기본 잉크와의 차이는 두 값이 갈리는 Latte가 가른다.
#[test]
fn attach_meta_shows_the_raised_inactive_badge_swatch() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    assert_ne!(
        mocha.accent_warning_raised(),
        latte.accent_warning_raised(),
        "the two themes must give different raised values"
    );
    assert_ne!(
        latte.accent_warning(),
        latte.accent_warning_raised(),
        "Latte must tell the base accent from the raised one"
    );
    for th in [mocha, latte] {
        let p = paint(|ui| draw_attach(ui, &th));
        let (chip, swatch) = p.token_chip(th.surface_raised().to_egui(), "— inactive badge");
        assert!(
            p.has_text_in("accent-warning", chip),
            "the inactive badge chip is named accent-warning"
        );
        assert_eq!(swatch, th.accent_warning_raised().to_egui(), "chip swatch");
    }
}
