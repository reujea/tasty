//! Attach 탭 표본 Meta의 inactive name 칩을 그려진 도형으로 확인한다.

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
