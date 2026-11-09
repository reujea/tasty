//! 종류별 글꼴 설정을 egui의 font_<kind> 패밀리로 등록하고 설정 변경 시 갱신한다.

use std::collections::BTreeMap;
use std::sync::Arc;

use tasty_i18n::font::LocaleFontError;

use crate::settings::{AppearanceSettings, EffectiveFont};

fn family_name_for_kind(kind: &str) -> String {
    format!("font_{kind}")
}

#[derive(Default, Debug, Clone)]
pub struct SurfaceFontState {
    sigs: BTreeMap<String, String>,
    initialized: bool,
}

fn signature(font: &EffectiveFont) -> String {
    format!("{}|{}", font.font_family, font.custom_font_path)
}

fn current_signatures(appearance: &AppearanceSettings) -> BTreeMap<String, String> {
    appearance
        .plugin_font_overrides
        .keys()
        .map(|kind| {
            let eff = appearance.effective_font_for_kind(kind);
            (kind.clone(), signature(&eff))
        })
        .collect()
}

pub fn refresh_surface_fonts(
    ctx: &egui::Context,
    appearance: &AppearanceSettings,
    state: &mut SurfaceFontState,
) {
    let new_sigs = current_signatures(appearance);

    if state.initialized && state.sigs == new_sigs {
        return;
    }

    let fonts = build_font_definitions(appearance, None);
    ctx.set_fonts(fonts);
    state.sigs = new_sigs;
    state.initialized = true;
}

/// 기본·CJK·종류별 글꼴과 선택적 preview 글꼴을 함께 만든다.
/// preview의 set_fonts가 기존 종류별 글꼴을 없애지 않도록 전체 구성을 제공한다.
pub fn build_font_definitions(
    appearance: &AppearanceSettings,
    preview: Option<(&str, &EffectiveFont)>,
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    // set_fonts가 기존 구성을 덮으므로 부팅과 같은 D2Coding 우선순위도 다시 넣는다.
    fonts.font_data.insert(
        "d2coding".to_owned(),
        Arc::new(egui::FontData::from_static(
            crate::font::D2CODING_REGULAR_TTF,
        )),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "d2coding".to_owned());

    let cjk_data = load_system_cjk_font_data();
    if let Some(bytes) = &cjk_data {
        fonts.font_data.insert(
            "system_cjk".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes.clone())),
        );
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push("system_cjk".to_owned());
        }
    }

    // 부팅과 같은 헬퍼로 CJK 뒤에 언어팩 글꼴을 추가한다.
    if let Some(path) = crate::boot::locale::font_env_path()
        && let Err(e) = tasty_egui_theme::install_locale_font_fallback(&mut fonts, &path)
    {
        tracing::warn!(
            "locale font at {} could not be installed: {e}",
            path.display()
        );
    }

    for kind in appearance.plugin_font_overrides.keys() {
        let eff = appearance.effective_font_for_kind(kind);
        let family = family_name_for_kind(kind);
        register_surface_family(&mut fonts, &family, kind, &eff, cjk_data.is_some());
    }

    if let Some((slot, eff)) = preview {
        let prefix = format!("preview_{}", slot);
        register_surface_family(&mut fonts, slot, &prefix, eff, cjk_data.is_some());
    }

    fonts
}

fn register_surface_family(
    fonts: &mut egui::FontDefinitions,
    family_name: &str,
    data_prefix: &str,
    eff: &EffectiveFont,
    has_cjk: bool,
) {
    let mut family_fonts: Vec<String> = Vec::new();

    let key = format!("{}_custom", data_prefix);
    register_custom_font(fonts, &mut family_fonts, family_name, key, eff);
    let key = format!("{}_named", data_prefix);
    register_named_font(
        fonts,
        &mut family_fonts,
        family_name,
        key,
        eff,
        load_system_family,
    );

    // 지정한 글꼴을 찾지 못해도 사용할 기본 monospace 패밀리를 남긴다.
    if let Some(monospace) = fonts.families.get(&egui::FontFamily::Monospace).cloned() {
        for name in monospace {
            if !family_fonts.contains(&name) {
                family_fonts.push(name);
            }
        }
    }

    if has_cjk && !family_fonts.iter().any(|n| n == "system_cjk") {
        family_fonts.push("system_cjk".to_owned());
    }

    fonts
        .families
        .insert(egui::FontFamily::Name(family_name.into()), family_fonts);
}

fn register_custom_font(
    fonts: &mut egui::FontDefinitions,
    family_fonts: &mut Vec<String>,
    family_name: &str,
    key: String,
    eff: &EffectiveFont,
) {
    if eff.custom_font_path.is_empty() {
        return;
    }
    let result = std::fs::read(&eff.custom_font_path)
        .map_err(LocaleFontError::Read)
        .and_then(|bytes| insert_font(fonts, family_fonts, key, bytes));
    if let Err(e) = result {
        tracing::warn!(
            "failed to load custom font for {} ({}): {e}",
            family_name,
            eff.custom_font_path
        );
    }
}

fn register_named_font(
    fonts: &mut egui::FontDefinitions,
    family_fonts: &mut Vec<String>,
    family_name: &str,
    key: String,
    eff: &EffectiveFont,
    load_family: impl FnOnce(&str) -> Option<Vec<u8>>,
) {
    if eff.font_family.is_empty() || eff.font_family.eq_ignore_ascii_case("monospace") {
        return;
    }
    let Some(bytes) = load_family(&eff.font_family) else {
        return;
    };
    if let Err(e) = insert_font(fonts, family_fonts, key, bytes) {
        tracing::warn!(
            "failed to load font family for {} ({}): {e}",
            family_name,
            eff.font_family
        );
    }
}

fn load_system_family(family: &str) -> Option<Vec<u8>> {
    crate::font::FontConfig::new(14.0, "").load_family_data(family)
}

/// `validate`가 거절한 글꼴은 등록하지 않는다. fontdb는 이름 표만 읽어 ab_glyph가 거절하는
/// 파일도 받으므로 시스템 글꼴 이름으로 찾은 바이트도 같은 판정을 거친다.
fn insert_font(
    fonts: &mut egui::FontDefinitions,
    family_fonts: &mut Vec<String>,
    key: String,
    bytes: Vec<u8>,
) -> Result<(), LocaleFontError> {
    tasty_i18n::font::validate(&bytes)?;
    fonts
        .font_data
        .insert(key.clone(), Arc::new(egui::FontData::from_owned(bytes)));
    family_fonts.push(key);
    Ok(())
}

fn load_system_cjk_font_data() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        let path = "C:/Windows/Fonts/malgun.ttf";
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }

    #[cfg(target_os = "macos")]
    {
        for path in &[
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for path in &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caption_px() -> f32 {
        tasty_themes::mocha_fallback().font_size_caption.value()
    }

    /// 배치된 두 글자의 차이로 픽셀 반올림까지 반영한 글자 간격을 잰다.
    fn mono_advance(fonts: egui::FontDefinitions) -> f32 {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        // 폰트 상태는 첫 pass 에서 채워진다. 빈 pass 의 산출물은 볼 것이 없다.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let font = egui::FontId::monospace(caption_px());
        let w = |s: &str| {
            ctx.fonts(|f| {
                f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::WHITE)
                    .rect
                    .width()
            })
        };
        w("00") - w("0")
    }

    // 부팅 뒤 set_fonts가 기존 구성을 덮어도 같은 monospace 글꼴을 유지해야 한다.
    #[test]
    fn the_two_font_stacks_agree_on_the_mono_cell() {
        let boot = egui::Context::default();
        crate::gfx::gpu::GpuState::setup_egui_fonts(&boot);
        let _ = boot.run(egui::RawInput::default(), |_| {});
        let font = egui::FontId::monospace(caption_px());
        let w = |s: &str| {
            boot.fonts(|f| {
                f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::WHITE)
                    .rect
                    .width()
            })
        };
        let boot_advance = w("00") - w("0");

        let runtime_advance =
            mono_advance(build_font_definitions(&AppearanceSettings::default(), None));

        assert_eq!(
            runtime_advance, boot_advance,
            "부팅 스택과 런타임 스택의 mono 한 칸이 다르다 — 뒤쪽이 앞쪽을 덮어쓰므로 \
             화면에 남는 것은 런타임 쪽이다"
        );
        // 두 경로가 함께 다른 글꼴로 바뀌는 경우도 확인한다.
        assert_eq!(
            tasty_type_geometry::length::LogicalPx(runtime_advance),
            tasty_ui_widgets::tokens::FH_TARGET_MONO_ADVANCE
        );
    }

    const VALID: &[u8] = crate::font::D2CODING_REGULAR_TTF;

    /// Returns the bundled font with a table record renamed so the parser cannot find it.
    fn without_table(tag: &[u8; 4]) -> Vec<u8> {
        let mut font = VALID.to_vec();
        let record = table_record(&font, tag);
        font[record] = b'x';
        font
    }

    fn table_record(font: &[u8], tag: &[u8; 4]) -> usize {
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &font[r..r + 4] == tag)
            .unwrap()
    }

    /// Parses, but its height is negative. epaint asserts on it at layout time.
    fn negative_height() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let record = table_record(&font, b"hhea");
        let hhea = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize;
        font[hhea + 4..hhea + 6].copy_from_slice(&(-1000i16).to_be_bytes());
        font[hhea + 6..hhea + 8].copy_from_slice(&1000i16.to_be_bytes());
        font
    }

    fn not_a_font() -> Vec<u8> {
        b"this is not a font\n".to_vec()
    }

    /// Runs what the render loop and the settings preview run: installs the
    /// definitions, runs a pass, then lays out with the family. Some fonts panic
    /// only at layout, so a pass alone would miss them.
    fn draw_with(fonts: egui::FontDefinitions, family: &str) {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        // 폰트 상태는 첫 pass 에서 채워진다. 빈 pass 의 산출물은 볼 것이 없다.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        // 줄 높이 판정은 글자 크기와 무관하므로 어떤 양수든 된다.
        const ANY_POSITIVE_SIZE: f32 = 14.0;
        ctx.fonts(|f| {
            let font = egui::FontId::new(ANY_POSITIVE_SIZE, egui::FontFamily::Name(family.into()));
            f.layout_no_wrap("a".to_owned(), font, egui::Color32::WHITE);
        });
    }

    fn appearance_with_custom_font(path: &std::path::Path) -> (AppearanceSettings, EffectiveFont) {
        let mut appearance = AppearanceSettings::default();
        appearance.default_font.custom_font_path = path.display().to_string();
        appearance
            .plugin_font_overrides
            .insert("explorer".to_owned(), Default::default());
        let eff = appearance.effective_font_for_kind("explorer");
        (appearance, eff)
    }

    fn assert_custom_font_falls_back(bytes: Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, bytes).unwrap();
        let (appearance, eff) = appearance_with_custom_font(&path);

        let fonts = build_font_definitions(&appearance, Some(("preview_default", &eff)));
        draw_with(fonts.clone(), "font_explorer");
        draw_with(fonts.clone(), "preview_default");
        assert!(!fonts.font_data.contains_key("explorer_custom"));
        assert!(
            !fonts
                .font_data
                .contains_key("preview_preview_default_custom")
        );
    }

    #[test]
    fn a_text_file_as_custom_font_falls_back() {
        assert_custom_font_falls_back(not_a_font());
    }

    #[test]
    fn an_empty_file_as_custom_font_falls_back() {
        assert_custom_font_falls_back(Vec::new());
    }

    #[test]
    fn a_font_without_hhea_as_custom_font_falls_back() {
        assert_custom_font_falls_back(without_table(b"hhea"));
    }

    #[test]
    fn a_negative_height_as_custom_font_falls_back() {
        assert_custom_font_falls_back(negative_height());
    }

    #[test]
    fn a_custom_font_egui_can_load_is_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, VALID).unwrap();
        let (appearance, eff) = appearance_with_custom_font(&path);

        let fonts = build_font_definitions(&appearance, Some(("preview_default", &eff)));
        for (family, key) in [
            ("font_explorer", "explorer_custom"),
            ("preview_default", "preview_preview_default_custom"),
        ] {
            let name = egui::FontFamily::Name(family.into());
            assert_eq!(fonts.families[&name][0], key);
        }
        draw_with(fonts.clone(), "font_explorer");
        draw_with(fonts, "preview_default");
    }

    // 이름으로 찾은 시스템 글꼴도 같은 함수를 지나므로 시스템 글꼴 폴더 없이 잰다.
    fn eff_named(family: &str) -> EffectiveFont {
        let mut eff = AppearanceSettings::default().effective_font_for_kind("explorer");
        eff.font_family = family.to_owned();
        eff.custom_font_path = String::new();
        eff
    }

    #[test]
    fn a_font_found_by_name_passes_the_same_check() {
        let inputs = [
            ("text", not_a_font()),
            ("empty", Vec::new()),
            ("no hhea", without_table(b"hhea")),
            ("negative height", negative_height()),
        ];
        let eff = eff_named("Some Family");
        let mut accepted = Vec::new();
        for (name, bytes) in inputs {
            let mut fonts = egui::FontDefinitions::default();
            let mut family_fonts = Vec::new();
            register_named_font(
                &mut fonts,
                &mut family_fonts,
                "f",
                "k".to_owned(),
                &eff,
                |_| Some(bytes),
            );
            if fonts.font_data.contains_key("k") || !family_fonts.is_empty() {
                accepted.push(name);
            }
        }
        assert!(accepted.is_empty(), "accepted: {accepted:?}");
    }

    #[test]
    fn a_valid_font_found_by_name_is_registered() {
        let mut fonts = egui::FontDefinitions::default();
        let mut family_fonts = Vec::new();
        let mut asked = Vec::new();
        register_named_font(
            &mut fonts,
            &mut family_fonts,
            "f",
            "k".to_owned(),
            &eff_named("Some Family"),
            |family| {
                asked.push(family.to_owned());
                Some(VALID.to_vec())
            },
        );
        assert_eq!(asked, ["Some Family"]);
        assert_eq!(family_fonts, ["k"]);
        assert!(fonts.font_data.contains_key("k"));
    }

    #[test]
    fn a_refused_custom_font_leaves_a_valid_named_font_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ttf");
        std::fs::write(&path, not_a_font()).unwrap();
        let mut eff = eff_named("Some Family");
        eff.custom_font_path = path.display().to_string();

        let mut fonts = egui::FontDefinitions::default();
        let mut family_fonts = Vec::new();
        register_custom_font(
            &mut fonts,
            &mut family_fonts,
            "f",
            "custom".to_owned(),
            &eff,
        );
        register_named_font(
            &mut fonts,
            &mut family_fonts,
            "f",
            "named".to_owned(),
            &eff,
            |_| Some(VALID.to_vec()),
        );
        assert_eq!(family_fonts, ["named"]);
    }
}
