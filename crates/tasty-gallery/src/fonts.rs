//! 갤러리와 글자 폭 검사가 같은 폰트 구성을 사용하도록 설치 함수를 공유한다.

use std::sync::Arc;

/// 번들 D2Coding을 Monospace 앞에 놓고 시스템 CJK 글꼴을 두 계열의 fallback으로 추가한다.
/// 본체와 글꼴을 맞춰 너비 기반 말줄임을 비교한다. 설정을 읽지 않으므로 언어팩 폰트는 제외한다.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "d2coding".to_owned(),
        Arc::new(egui::FontData::from_static(
            tasty_font::D2CODING_REGULAR_TTF,
        )),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "d2coding".to_owned());

    if !tasty_egui_theme::install_system_cjk_fallback(&mut fonts) {
        tracing::warn!(
            "no system CJK font found; Korean/Japanese/Chinese labels will render as \u{25a1}"
        );
    }

    ctx.set_fonts(fonts);
}
