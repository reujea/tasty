//! Passkeys 탭 표본 Meta의 unknown kind 칩을 그려진 도형으로 확인한다.

use crate::catalog::components::painted::paint;

use super::draw_passkeys;

/// unknown kind 칩은 accent-warning 이름, 용도 글자 'unknown kind', raised 스와치를 보인다. Mocha는
/// accent-warning과 raised가 같은 값이라 기본 잉크와의 차이는 두 값이 갈리는 Latte가 가른다.
#[test]
fn passkeys_meta_shows_the_raised_unknown_kind_swatch() {
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
        let p = paint(|ui| draw_passkeys(ui, &th));
        let (chip, swatch) = p.token_chip(th.surface_raised().to_egui(), "— unknown kind");
        assert!(
            p.has_text_in("accent-warning", chip),
            "the unknown kind chip is named accent-warning"
        );
        assert_eq!(swatch, th.accent_warning_raised().to_egui(), "chip swatch");
    }
}
