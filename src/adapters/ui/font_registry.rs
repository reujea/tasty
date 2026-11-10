//! 종류별 글꼴 설정을 egui의 font_<kind> 패밀리로 등록하고 설정 변경 시 갱신한다.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::adapters::ui::font_judge::{
    FontLookup, FontOrigin, UiFamily, load_ui_custom, load_ui_family,
};
use crate::font::FontFileFace;
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

    let build = build_font_definitions(appearance, None);
    ctx.set_fonts(build.fonts);
    state.sigs = new_sigs;
    state.initialized = true;
}

/// 등록하지 못한 글꼴 하나. 거절은 글꼴 단위다. custom 이 거절돼도 같은 슬롯의 named 는 살 수 있다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedFont {
    /// 등록하려던 egui family 이름.
    pub slot: String,
    pub origin: FontOrigin,
    pub value: String,
    pub reason: String,
}

/// 만든 글꼴 구성과 그 과정에서 거절된 글꼴들.
pub struct FontBuild {
    pub fonts: egui::FontDefinitions,
    pub rejected: Vec<RejectedFont>,
}

/// 기본·CJK·종류별 글꼴과 선택적 preview 글꼴을 함께 만든다.
/// preview의 set_fonts가 기존 종류별 글꼴을 없애지 않도록 전체 구성을 제공한다.
pub fn build_font_definitions(
    appearance: &AppearanceSettings,
    preview: Option<(&str, &EffectiveFont)>,
) -> FontBuild {
    let mut lookup = FontLookup::default();
    let mut rejected = Vec::new();
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

    let has_cjk = tasty_egui_theme::install_system_cjk_fallback(&mut fonts);

    // 부팅과 같은 헬퍼로 CJK 뒤에 언어팩 글꼴을 추가한다.
    if let Some(path) = crate::boot::locale::font_env_path()
        && let Err(e) = tasty_egui_theme::install_locale_font_fallback(
            &mut fonts,
            &path,
            tasty_egui_theme::locale_font_face_from_env(),
        )
    {
        tracing::warn!(
            "locale font at {} could not be installed: {e}",
            path.display()
        );
    }

    for kind in appearance.plugin_font_overrides.keys() {
        let eff = appearance.effective_font_for_kind(kind);
        let family = family_name_for_kind(kind);
        register_surface_family(
            &mut fonts,
            &family,
            kind,
            &eff,
            has_cjk,
            |name| lookup.load_family(name),
            &mut rejected,
        );
    }

    if let Some((slot, eff)) = preview {
        let prefix = format!("preview_{}", slot);
        register_surface_family(
            &mut fonts,
            slot,
            &prefix,
            eff,
            has_cjk,
            |name| lookup.load_family(name),
            &mut rejected,
        );
    }

    FontBuild { fonts, rejected }
}

fn register_surface_family(
    fonts: &mut egui::FontDefinitions,
    family_name: &str,
    data_prefix: &str,
    eff: &EffectiveFont,
    has_cjk: bool,
    load_family: impl FnOnce(&str) -> Option<FontFileFace<Vec<u8>>>,
    rejected: &mut Vec<RejectedFont>,
) {
    let mut family_fonts: Vec<String> = Vec::new();

    let key = format!("{}_custom", data_prefix);
    rejected.extend(register_custom_font(
        fonts,
        &mut family_fonts,
        family_name,
        key,
        eff,
    ));
    let key = format!("{}_named", data_prefix);
    rejected.extend(register_named_font(
        fonts,
        &mut family_fonts,
        family_name,
        key,
        eff,
        load_family,
    ));

    // 지정한 글꼴을 찾지 못해도 사용할 기본 monospace 패밀리를 남긴다.
    if let Some(monospace) = fonts.families.get(&egui::FontFamily::Monospace).cloned() {
        for name in monospace {
            if !family_fonts.contains(&name) {
                family_fonts.push(name);
            }
        }
    }

    if has_cjk {
        for cjk in tasty_egui_theme::SYSTEM_CJK_FONT_KEYS {
            if fonts.font_data.contains_key(*cjk) && !family_fonts.iter().any(|n| n == cjk) {
                family_fonts.push((*cjk).to_owned());
            }
        }
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
) -> Option<RejectedFont> {
    if eff.custom_font_path.is_empty() {
        return None;
    }
    match load_ui_custom(&eff.custom_font_path, &eff.font_family) {
        Ok(found) => {
            insert_font(fonts, family_fonts, key, found.file, found.face);
            None
        }
        Err(e) => {
            tracing::warn!(
                "failed to load custom font for {} ({}): {e}",
                family_name,
                eff.custom_font_path
            );
            Some(RejectedFont {
                slot: family_name.to_owned(),
                origin: FontOrigin::Custom,
                value: eff.custom_font_path.clone(),
                reason: e.to_string(),
            })
        }
    }
}

fn register_named_font(
    fonts: &mut egui::FontDefinitions,
    family_fonts: &mut Vec<String>,
    family_name: &str,
    key: String,
    eff: &EffectiveFont,
    load_family: impl FnOnce(&str) -> Option<FontFileFace<Vec<u8>>>,
) -> Option<RejectedFont> {
    let reason = match load_ui_family(&eff.font_family, &eff.custom_font_path, load_family) {
        UiFamily::Skip => return None,
        UiFamily::Usable(found) => {
            insert_font(fonts, family_fonts, key, found.file, found.face);
            return None;
        }
        UiFamily::Refused(e) => {
            tracing::warn!(
                "failed to load font family for {} ({}): {e}",
                family_name,
                eff.font_family
            );
            e.to_string()
        }
        // 입력 중에도 이 경로를 지나므로 없는 이름은 경고 로그를 남기지 않는다.
        UiFamily::Missing => "no font with this family name".to_owned(),
    };
    Some(RejectedFont {
        slot: family_name.to_owned(),
        origin: FontOrigin::Named,
        value: eff.font_family.clone(),
        reason,
    })
}

fn insert_font(
    fonts: &mut egui::FontDefinitions,
    family_fonts: &mut Vec<String>,
    key: String,
    bytes: Vec<u8>,
    face: u32,
) {
    let mut data = egui::FontData::from_owned(bytes);
    data.index = face;
    fonts.font_data.insert(key.clone(), Arc::new(data));
    family_fonts.push(key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ui::font_judge::fixtures::{
        VALID, negative_height, not_a_font, table_record, whole_file, without_table,
    };

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
            mono_advance(build_font_definitions(&AppearanceSettings::default(), None).fonts);

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

    /// Runs what the render loop and the settings preview run: installs the
    /// definitions, runs a pass, then lays out with the family. Some fonts panic
    /// only at layout, so a pass alone would miss them.
    fn draw_with(fonts: egui::FontDefinitions, family: &str) {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        // 폰트 상태는 첫 pass 에서 채워진다. 빈 pass 의 산출물은 볼 것이 없다.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        // 글꼴 판정용 값이라 화면 크기와 무관하다. 줄 높이의 부호만 보므로 어떤 양수든 된다.
        // UI 폰트 토큰과 연결하지 않으려고 토큰에 없는 값을 쓴다.
        const ANY_POSITIVE_SIZE: f32 = 9.0;
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

        let build = build_font_definitions(&appearance, Some(("preview_default", &eff)));
        for slot in ["font_explorer", "preview_default"] {
            assert!(
                build
                    .rejected
                    .iter()
                    .any(|r| r.slot == slot && r.origin == FontOrigin::Custom),
                "{slot}: {:?}",
                build.rejected
            );
        }
        let fonts = build.fonts;
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

    /// Real outlines with `unitsPerEm` lowered, so they are far bigger than intended.
    /// The check in front of egui passes it because it parses and has a line height.
    fn tiny_units_per_em() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let record = table_record(&font, b"head");
        let head = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize;
        font[head + 18..head + 20].copy_from_slice(&16u16.to_be_bytes());
        font
    }

    #[test]
    fn a_custom_font_with_glyphs_over_the_atlas_stays_in_use_and_draws() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, tiny_units_per_em()).unwrap();
        let (appearance, eff) = appearance_with_custom_font(&path);
        let build = build_font_definitions(&appearance, Some(("preview_default", &eff)));
        assert_eq!(build.rejected, []);
        let fonts = build.fonts;
        let name = egui::FontFamily::Name("preview_default".into());
        assert_eq!(fonts.families[&name][0], "preview_preview_default_custom");

        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        // 폰트 상태는 첫 pass 에서 채워진다. 빈 pass 의 산출물은 볼 것이 없다.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        // 입력란의 상한이다. 이 글꼴의 "A"는 이 크기에서 폭 2206px로 atlas 2048을 넘는다.
        let largest_input_size = 72.0;
        let galley = ctx.fonts(|f| {
            let font = egui::FontId::new(largest_input_size, name);
            f.layout_no_wrap("A".to_owned(), font, egui::Color32::WHITE)
        });
        assert!(galley.rows[0].glyphs[0].uv_rect.is_nothing());
        // 그린 뒤의 pass 가 패닉하지 않는지만 본다. 산출물은 볼 것이 없다.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
    }

    #[test]
    fn a_custom_font_egui_can_load_is_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, VALID).unwrap();
        let (appearance, eff) = appearance_with_custom_font(&path);

        let build = build_font_definitions(&appearance, Some(("preview_default", &eff)));
        assert_eq!(build.rejected, []);
        let fonts = build.fonts;
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

    fn eff_named(family: &str) -> EffectiveFont {
        let mut eff = AppearanceSettings::default().effective_font_for_kind("explorer");
        eff.font_family = family.to_owned();
        eff.custom_font_path = String::new();
        eff
    }

    // 이름으로 찾은 시스템 글꼴도 같은 함수를 지나므로 시스템 글꼴 폴더 없이 잰다.
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
                |_| Some(whole_file(bytes)),
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
                Some(whole_file(VALID.to_vec()))
            },
        );
        assert_eq!(asked, ["Some Family"]);
        assert_eq!(family_fonts, ["k"]);
        assert!(fonts.font_data.contains_key("k"));
    }

    /// 같은 글꼴 둘을 묶음 파일로 만든다. 표의 위치는 파일 처음부터 세므로 뒤쪽 글꼴은 옮긴다.
    fn collection_of_two() -> Vec<u8> {
        let header = 12 + 4 * 2;
        let second_at = header + VALID.len();
        let mut out = b"ttcf\0\x01\0\0".to_vec();
        out.extend_from_slice(&2u32.to_be_bytes());
        for at in [header, second_at] {
            out.extend_from_slice(&(at as u32).to_be_bytes());
        }
        for at in [header, second_at] {
            let mut font = VALID.to_vec();
            let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
            for record in (0..tables).map(|i| 12 + 16 * i) {
                let offset = record + 8;
                let old = u32::from_be_bytes(font[offset..offset + 4].try_into().unwrap());
                font[offset..offset + 4].copy_from_slice(&(old + at as u32).to_be_bytes());
            }
            out.extend_from_slice(&font);
        }
        out
    }

    #[test]
    fn a_font_found_by_name_keeps_its_face_number() {
        let mut fonts = egui::FontDefinitions::default();
        let mut family_fonts = Vec::new();
        register_named_font(
            &mut fonts,
            &mut family_fonts,
            "f",
            "k".to_owned(),
            &eff_named("Some Family"),
            |_| {
                Some(FontFileFace {
                    file: collection_of_two(),
                    face: 1,
                })
            },
        );
        assert_eq!(family_fonts, ["k"]);
        assert_eq!(fonts.font_data["k"].index, 1);
    }

    #[test]
    fn a_face_past_the_end_of_the_file_is_refused() {
        let mut fonts = egui::FontDefinitions::default();
        let mut family_fonts = Vec::new();
        // 이 파일에 1번 글꼴은 없으므로 번호를 그대로 읽으면 거절된다. 번호를 버리면 등록된다.
        register_named_font(
            &mut fonts,
            &mut family_fonts,
            "f",
            "past-the-end".to_owned(),
            &eff_named("Some Family"),
            |_| {
                Some(FontFileFace {
                    file: VALID.to_vec(),
                    face: 1,
                })
            },
        );
        assert!(family_fonts.is_empty());
    }

    #[test]
    fn a_refused_custom_font_leaves_a_valid_named_font_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ttf");
        std::fs::write(&path, not_a_font()).unwrap();
        let mut eff = eff_named("Some Family");
        eff.custom_font_path = path.display().to_string();

        let mut fonts = egui::FontDefinitions::default();
        let mut rejected = Vec::new();
        register_surface_family(
            &mut fonts,
            "f",
            "p",
            &eff,
            false,
            |_| Some(whole_file(VALID.to_vec())),
            &mut rejected,
        );

        let family = &fonts.families[&egui::FontFamily::Name("f".into())];
        assert_eq!(family[0], "p_named");
        assert_eq!(rejected.len(), 1, "{rejected:?}");
        assert_eq!(rejected[0].origin, FontOrigin::Custom);
        assert_eq!(rejected[0].value, eff.custom_font_path);
    }

    /// 없어진 이름은 거절로 남지만 경고 로그는 늘리지 않고, 슬롯은 기본 글꼴로 그린다.
    #[test]
    fn a_family_name_that_is_gone_is_reported_and_the_slot_still_draws() {
        let eff = eff_named("Some Family");
        let mut fonts = egui::FontDefinitions::default();
        let mut rejected = Vec::new();
        register_surface_family(&mut fonts, "f", "p", &eff, false, |_| None, &mut rejected);
        assert_eq!(rejected.len(), 1, "{rejected:?}");
        assert_eq!(rejected[0].origin, FontOrigin::Named);
        assert_eq!(rejected[0].value, "Some Family");
        draw_with(fonts, "f");
    }

    /// family 거절도 호출 경로 그대로 잰다. 줄 높이 음수는 layout 까지 가야 패닉하므로 그려 본다.
    #[test]
    fn a_refused_family_is_reported_through_the_surface_family_path() {
        for bytes in [not_a_font(), negative_height(), without_table(b"hhea")] {
            let eff = eff_named("Some Family");
            let mut fonts = egui::FontDefinitions::default();
            let mut rejected = Vec::new();
            register_surface_family(
                &mut fonts,
                "f",
                "p",
                &eff,
                false,
                |_| Some(whole_file(bytes)),
                &mut rejected,
            );
            assert_eq!(rejected.len(), 1, "{rejected:?}");
            assert_eq!(rejected[0].origin, FontOrigin::Named);
            assert!(!fonts.font_data.contains_key("p_named"));
            draw_with(fonts, "f");
        }
    }
}
