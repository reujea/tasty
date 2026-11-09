use std::sync::Arc;

use crate::font::{D2CODING_FAMILY, D2CODING_REGULAR_TTF};

use super::GpuState;

impl GpuState {
    /// 기본 UI 글꼴, D2Coding 고정폭 글꼴과 시스템 CJK fallback을 등록한다.
    /// 테스트도 이 함수를 사용해 실제 앱과 같은 폰트의 폭을 잰다.
    pub(crate) fn setup_egui_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "d2coding".to_owned(),
            Arc::new(egui::FontData::from_static(D2CODING_REGULAR_TTF)),
        );
        let monospace = fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default();
        monospace.insert(0, "d2coding".to_owned());

        if !tasty_egui_theme::install_system_cjk_fallback(&mut fonts) {
            tracing::warn!(
                "no system CJK font found; UI may show □ for non-Latin/Hangul text outside D2Coding ({D2CODING_FAMILY})"
            );
        }

        // 언어팩 폰트는 마지막 fallback에 추가하며 설치 전에 파일을 다시 검증한다.
        if let Some(path) = crate::boot::locale::font_env_path()
            && let Err(e) = tasty_egui_theme::install_locale_font_fallback(&mut fonts, &path)
        {
            tracing::warn!(
                "locale font at {} could not be installed: {e}",
                path.display()
            );
        }

        ctx.set_fonts(fonts);
    }
}
