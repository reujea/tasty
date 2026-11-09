#![forbid(unsafe_code)]
// 이유: 테스트의 임시 파일 정리 실패는 무시한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

//! 본체와 갤러리에서 공유하는 Theme ↔ egui 어댑터.
//! 테마의 밝기에 맞는 egui 기본값을 고른 뒤 위젯 색, 글꼴, 간격, 그림자를 적용한다.

pub mod overlay_edge;

use std::sync::Arc;

use egui::emath::GuiRounding as _;
use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

/// egui 기본 약한 텍스트 색 대신 Theme의 placeholder 색을 적용한다.
pub fn hint_text(theme: &Theme, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(egui::Color32::from(theme.placeholder))
}

#[inline]
fn stroke1(theme: &Theme, c: HexColor) -> egui::Stroke {
    egui::Stroke::new(theme.border_width.value(), c)
}

/// 버튼에 붙는 egui 팝오버(`popup_below_widget` · `popup_above_or_below_widget` ·
/// `ComboBox`)를 `add` 안에서 열면 그 틀이 메뉴 컨테이너 토큰(menu-bg · menu-border ·
/// menu-radius)을 쓰고, 안쪽 둘레는 앵커 메뉴 공통인 `popup-content-margin` 이다.
/// egui 는 팝오버 틀을 부모 `Ui` 스타일로, tooltip 틀을 전역 스타일로 그리므로 부모 스타일만
/// 잠시 바꾼다. 자식 `Ui` 를 만들지 않아 위젯·팝업 ID 는 그대로다.
pub fn with_popover_frame<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let ring = egui::Margin::same(theme.popup_content_margin().value().round() as i8);
    with_popover_frame_ring(ui, theme, ring, add)
}

/// [`with_popover_frame`] 와 같고 안쪽 둘레만 `ring` 으로 정한다. 자체 둘레 토큰을 가진
/// 팝오버(MultiSelect 메뉴)나 구획이 틀 끝까지 닿는 팝오버(둘레 0)가 쓴다.
pub fn with_popover_frame_ring<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    ring: egui::Margin,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let prev = ui.style().clone();
    // 이 호출이 그릴 수 있는 드롭다운은 지금 열린 egui 팝업이다. 직전 프레임 영역이 불투명
    // 페이지와 겹치거나 부모 셸이 불투명이면 상자 전체를 각진 모서리·그림자 없음으로 그린다.
    let ctx = ui.ctx().clone();
    let open = overlay_edge::open_popup_layers(&ctx);
    let pages = overlay_edge::opaque_pages(&ctx);
    let parent_opaque = overlay_edge::opaque_parent(&ctx);
    let edge = open
        .iter()
        .filter_map(|l| ctx.memory(|m| m.area_rect(l.id)))
        .map(|r| overlay_edge::edge_for(r, parent_opaque, &pages))
        .find(|e| e.opaque)
        .unwrap_or(overlay_edge::BoxEdge {
            opaque: parent_opaque,
        });
    let visuals = &mut ui.style_mut().visuals;
    apply_raised_scrollbar(theme, visuals);
    visuals.window_fill = theme.menu_bg().into();
    visuals.window_stroke = stroke1(theme, theme.menu_border());
    visuals.menu_corner_radius = edge.radius(theme, theme.menu_radius()).value().into();
    if edge.opaque {
        visuals.popup_shadow = theme.shadow_none().to_egui();
    }
    ui.style_mut().spacing.menu_margin = ring;
    let before = overlay_edge::paint_counts(&ctx, &open);
    let out = add(ui);
    // 이 프레임에 처음 연 팝업은 호출 뒤에야 레이어 목록에 들어온다. 그 전 그림 수는 0이다.
    let mut layers = open.clone();
    let mut counts = before;
    for l in overlay_edge::open_popup_layers(&ctx) {
        if !layers.contains(&l) {
            layers.push(l);
            counts.push(0);
        }
    }
    overlay_edge::record_drawn(&ctx, ui.layer_id(), edge, &layers, &counts);
    ui.set_style(prev);
    out
}

/// Theme를 egui에 적용한다. Theme에 이미 UI 배율이 반영되어 있어야 한다.
pub fn apply_theme_to_egui(theme: &Theme, ctx: &egui::Context) {
    ctx.set_visuals(theme_visuals(theme));
    apply_theme_style(theme, ctx);
}

/// surface-raised 위에 그리는 스크롤 영역의 thumb 색. 호출 범위의 `Ui` visuals에 적용한다.
pub fn apply_raised_scrollbar(theme: &Theme, visuals: &mut egui::Visuals) {
    visuals.widgets.inactive.fg_stroke = stroke1(theme, theme.scrollbar_thumb_raised());
}

/// Theme 의 egui 색 설정. 다른 테마로 고정해 그리는 영역(갤러리의 테마별 열)은
/// 이 값을 `Ui` 스코프에만 적용한다.
pub fn theme_visuals(theme: &Theme) -> egui::Visuals {
    // 직접 지정하지 않는 필드도 테마 밝기에 맞도록 기본값을 선택한다.
    let mut visuals = if theme.is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };

    visuals.panel_fill = theme.mantle.into();
    // egui 의 popup·tooltip 프레임은 같은 window 값을 읽는다. tooltip 은 전역 스타일로
    // 그려지므로 전역 값은 tooltip 토큰이다. 부모 `Ui` 스타일로 그려지는 팝오버는
    // [`with_popover_frame`] 이 그 호출 동안만 메뉴 토큰으로 바꾼다.
    visuals.window_fill = theme.tooltip_bg().into();
    visuals.window_stroke = stroke1(theme, theme.tooltip_border());
    visuals.menu_corner_radius = theme.tooltip_radius().value().into();

    // 위젯에 붙는 팝업은 popover, 독립 창은 modal 그림자를 사용한다.
    // 프레임을 직접 지정하는 호출부도 같은 두 토큰 중에서 선택한다.
    visuals.popup_shadow = theme.shadow_popover().to_egui();
    visuals.window_shadow = theme.shadow_modal().to_egui();
    visuals.extreme_bg_color = theme.crust.into();
    visuals.faint_bg_color = theme.surface0.into();
    visuals.code_bg_color = theme.surface0.into();

    visuals.widgets.noninteractive.bg_fill = theme.mantle.into();
    visuals.widgets.noninteractive.weak_bg_fill = theme.mantle.into();
    visuals.widgets.noninteractive.bg_stroke = stroke1(theme, theme.surface0);
    visuals.widgets.noninteractive.fg_stroke = stroke1(theme, theme.text);

    visuals.widgets.inactive.bg_fill = theme.base.into();
    visuals.widgets.inactive.weak_bg_fill = theme.base.into();
    visuals.widgets.inactive.bg_stroke = stroke1(theme, theme.surface0);
    visuals.widgets.inactive.fg_stroke = stroke1(theme, theme.text);

    visuals.widgets.hovered.bg_fill = theme.surface0.into();
    visuals.widgets.hovered.weak_bg_fill = theme.surface0.into();
    visuals.widgets.hovered.bg_stroke = stroke1(theme, theme.surface1);
    visuals.widgets.hovered.fg_stroke = stroke1(theme, theme.text);

    visuals.widgets.active.bg_fill = theme.surface1.into();
    visuals.widgets.active.weak_bg_fill = theme.surface1.into();
    visuals.widgets.active.bg_stroke = stroke1(theme, theme.surface2);
    visuals.widgets.active.fg_stroke = stroke1(theme, theme.text);

    // 떠 있는 스크롤바의 thumb는 상태별 fg_stroke를 읽는다. 글자는 override_text_color가 정한다.
    visuals.widgets.inactive.fg_stroke = stroke1(theme, theme.scrollbar_thumb());
    visuals.widgets.hovered.fg_stroke = stroke1(theme, theme.scrollbar_thumb_hover());
    visuals.widgets.active.fg_stroke = stroke1(theme, theme.scrollbar_thumb_hover());

    visuals.widgets.open.bg_fill = theme.surface1.into();
    visuals.widgets.open.weak_bg_fill = theme.surface1.into();
    visuals.widgets.open.bg_stroke = stroke1(theme, theme.surface2);
    visuals.widgets.open.fg_stroke = stroke1(theme, theme.text);

    // Straight RGBA는 to_egui()에서 sRGB 바이트에 알파를 곱해 premultiplied 색이 된다.
    const SELECTION_BG_ALPHA: u8 = 80;
    visuals.selection.bg_fill = theme
        .accent_primary()
        .with_alpha(SELECTION_BG_ALPHA)
        .to_egui();
    visuals.selection.stroke =
        egui::Stroke::new(theme.focus_ring_width.value(), theme.border_focus());

    visuals.hyperlink_color = theme.accent_primary().into();
    visuals.error_fg_color = theme.accent_danger().into();
    visuals.warn_fg_color = theme.accent_warning().into();

    visuals.override_text_color = Some(theme.text.into());

    // UI 입력란 커서는 깜박이지 않는 1px text-primary 막대다. 터미널 커서처럼 움직임이 없고,
    // 깜박이면 포커스된 입력란이 있는 동안 유휴 상태에서도 0.5초마다 프레임을 그린다.
    visuals.text_cursor.blink = false;
    visuals.text_cursor.stroke = stroke1(theme, theme.text_primary());
    visuals
}

fn apply_theme_style(theme: &Theme, ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(theme.font_size_body.value().round_ui()),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(theme.font_size_caption.value().round_ui()),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional((theme.font_size_heading.value() * 1.15).round_ui()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(theme.font_size_body.value().round_ui()),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::monospace(theme.font_size_body.value().round_ui()),
    );
    style.spacing.item_spacing = egui::vec2(
        theme.spacing_sm.value().round_ui(),
        theme.spacing_xs.value().round_ui(),
    );
    style.spacing.button_padding = egui::vec2(
        theme.spacing_sm.value().round_ui(),
        theme.spacing_xs.value().round_ui(),
    );
    // 기본 tooltip 틀은 전역 스타일의 `menu_margin` 을 안쪽 여백으로 쓴다. 팝오버는
    // `with_popover_frame` 이 부모 스타일에서 메뉴 둘레로 바꾸므로 전역 값은 tooltip 토큰이다.
    style.spacing.menu_margin = egui::Margin::symmetric(
        theme.tooltip_padding_x().value().round() as i8,
        theme.tooltip_padding_y().value().round() as i8,
    );
    // thumb는 3:1 등급이라 반투명으로 낮추지 않는다. 표시·숨김 동작은 그대로다.
    style.spacing.scroll.active_handle_opacity = 1.0;
    style.spacing.scroll.interact_handle_opacity = 1.0;
    // 스크롤에는 transition을 적용하지 않는다.
    style.scroll_animation = egui::style::ScrollAnimation::none();
    ctx.set_style(style);
}

/// egui `FontDefinitions` 안에서 시스템 CJK 글꼴을 가리키는 키.
pub const SYSTEM_CJK_FONT_KEY: &str = "system_cjk";

// 본체와 플러그인이 같은 글꼴을 쓰도록 후보는 여기에만 둔다.
#[cfg(target_os = "windows")]
const SYSTEM_CJK_CANDIDATES: &[&str] = &["C:/Windows/Fonts/malgun.ttf"];

#[cfg(target_os = "macos")]
const SYSTEM_CJK_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
];

#[cfg(target_os = "linux")]
const SYSTEM_CJK_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
    // 가변 글꼴이라 egui는 축 기본값인 Thin으로 그린다. 고정 글꼴이 없을 때 □ 대신 쓴다.
    "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc",
];

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
const SYSTEM_CJK_CANDIDATES: &[&str] = &[];

/// 간체 UI 언어. 별칭(`zh`, `zh-CN`)은 언어 코드로 받지 않으므로 여기서도 정확히 비교한다.
const SIMPLIFIED_CHINESE_LOCALE: &str = "zh-Hans";
const SIMPLIFIED_CHINESE_FACE: &str = "Noto Sans CJK SC";

/// 파일과 그 안에서 읽을 글꼴 번호.
struct SystemCjkFont {
    bytes: Vec<u8>,
    face: u32,
}

impl SystemCjkFont {
    fn into_font_data(self) -> egui::FontData {
        let mut data = egui::FontData::from_owned(self.bytes);
        data.index = self.face;
        data
    }
}

/// 한중일 글꼴 묶음은 공용 한자를 지역마다 다르게 그린다. 간체 UI만 SC를 고르고 다른
/// 언어는 0번을 유지해 기존 사용자의 한자 형태를 바꾸지 않는다. 묶음에 한글과 가나도
/// 들어 있어 간체 UI에서 다른 문자가 □가 되지 않는다.
fn cjk_face_for_locale(bytes: &[u8], locale: &str) -> u32 {
    if locale == SIMPLIFIED_CHINESE_LOCALE {
        tasty_i18n::font::collection_face_by_family(bytes, SIMPLIFIED_CHINESE_FACE).unwrap_or(0)
    } else {
        0
    }
}

/// 후보에서 egui가 읽을 수 있는 첫 글꼴을 고른다. 없는 파일은 설치되지 않은 것이므로
/// 말없이 넘어가고, egui가 panic할 파일은 경고하고 다음 후보로 넘어간다.
fn first_usable_cjk_font<'a>(
    candidates: impl IntoIterator<Item = &'a std::path::Path>,
    locale: &str,
) -> Option<SystemCjkFont> {
    candidates.into_iter().find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        let face = cjk_face_for_locale(&bytes, locale);
        match tasty_i18n::font::validate_face(&bytes, face) {
            Ok(()) => Some(SystemCjkFont { bytes, face }),
            Err(e) => {
                tracing::warn!(
                    "system CJK font {} (face {face}) is not usable: {e}",
                    path.display()
                );
                None
            }
        }
    })
}

/// 시스템 CJK 글꼴을 Proportional·Monospace의 마지막 폴백으로 추가한다.
/// 언어는 호스트가 부팅 때 정해 자기 환경과 플러그인 환경에 넣은 `TASTY_LOCALE`을 읽는다.
/// 쓸 수 있는 글꼴이 없으면 `fonts`를 그대로 두고 `false`를 돌려준다.
pub fn install_system_cjk_fallback(fonts: &mut egui::FontDefinitions) -> bool {
    let locale = std::env::var("TASTY_LOCALE").unwrap_or_default();
    let candidates = SYSTEM_CJK_CANDIDATES.iter().map(std::path::Path::new);
    let Some(font) = first_usable_cjk_font(candidates, &locale) else {
        return false;
    };
    fonts.font_data.insert(
        SYSTEM_CJK_FONT_KEY.to_owned(),
        Arc::new(font.into_font_data()),
    );
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(fam)
            .or_default()
            .push(SYSTEM_CJK_FONT_KEY.to_owned());
    }
    true
}

/// 시스템 CJK 폰트를 Proportional·Monospace의 마지막 폴백으로 추가한다.
/// 찾지 못하면 경고만 남긴다. 번들 D2Coding은 설치하지 않으므로 본체와 같은
/// 고정폭 측정에는 사용할 수 없다. 본체와 갤러리는 D2Coding을 먼저 설치한 뒤
/// `install_system_cjk_fallback`을 사용한다.
pub fn install_cjk_fallback(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if !install_system_cjk_fallback(&mut fonts) {
        tracing::warn!("no system CJK font found; Korean/Japanese/Chinese labels will render as □");
        return;
    }
    ctx.set_fonts(fonts);
}

/// egui `FontDefinitions` 안에서 언어팩 폰트를 가리키는 키.
const LOCALE_FONT_KEY: &str = "locale_pack";

/// 언어팩 폰트를 읽거나 검증하지 못한 이유.
pub use tasty_i18n::font::LocaleFontError;

/// `TASTY_LOCALE_FONT`와 함께 호스트가 넘긴 글꼴 번호. 값이 없으면 0번이고, 읽을 수 없는
/// 값은 경고하고 0번으로 읽는다.
pub fn locale_font_face_from_env() -> u32 {
    let Some(raw) = std::env::var_os("TASTY_LOCALE_FONT_FACE").filter(|v| !v.is_empty()) else {
        return 0;
    };
    raw.to_str()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            tracing::warn!("TASTY_LOCALE_FONT_FACE is not a face number: {raw:?}");
            0
        })
}

/// 언어팩 폰트를 Proportional·Monospace의 마지막 폴백으로 추가한다.
/// 앞선 폰트에 없는 글리프에만 사용한다. 추가 전에 ab_glyph로 검증하며 실패하면
/// `fonts`를 그대로 두고 오류를 반환한다. 본체와 플러그인이 같은 검증을 사용한다.
pub fn install_locale_font_fallback(
    fonts: &mut egui::FontDefinitions,
    path: &std::path::Path,
    face: u32,
) -> Result<(), LocaleFontError> {
    let bytes = tasty_i18n::font::read_validated(path, face)?;
    fonts.font_data.insert(
        LOCALE_FONT_KEY.to_owned(),
        Arc::new(SystemCjkFont { bytes, face }.into_font_data()),
    );
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(fam)
            .or_default()
            .push(LOCALE_FONT_KEY.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod system_cjk_tests {
    use super::*;

    const D2CODING: &[u8] = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");

    #[test]
    fn a_missing_or_broken_candidate_gives_way_to_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let (broken, valid) = (dir.path().join("broken.ttf"), dir.path().join("valid.ttf"));
        std::fs::write(&broken, b"not a font").unwrap();
        std::fs::write(&valid, D2CODING).unwrap();
        let missing = dir.path().join("missing.ttf");

        let found =
            first_usable_cjk_font([missing.as_path(), broken.as_path(), valid.as_path()], "en")
                .unwrap();
        assert_eq!(found.bytes, D2CODING);
        assert_eq!(found.face, 0);
    }

    #[test]
    fn no_usable_candidate_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.ttf");
        std::fs::write(&broken, b"not a font").unwrap();
        let missing = dir.path().join("missing.ttf");

        assert!(first_usable_cjk_font([missing.as_path(), broken.as_path()], "en").is_none());
    }

    #[test]
    fn a_font_without_the_simplified_face_stays_on_the_first_face() {
        assert_eq!(cjk_face_for_locale(D2CODING, SIMPLIFIED_CHINESE_LOCALE), 0);
    }

    #[test]
    fn the_chosen_face_reaches_egui() {
        let font = SystemCjkFont {
            bytes: D2CODING.to_vec(),
            face: 2,
        };
        assert_eq!(font.into_font_data().index, 2);
    }

    /// 저장소에 넣을 수 없는 시스템 글꼴을 읽는다. 없는 컴퓨터에서는 아무것도 재지 않는다.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_installed_noto_collection_gives_simplified_chinese_its_own_face() {
        let Some(path) = SYSTEM_CJK_CANDIDATES
            .iter()
            .map(std::path::Path::new)
            .find(|path| path.exists())
        else {
            tracing::warn!("skipped: no Noto CJK collection is installed");
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let simplified = cjk_face_for_locale(&bytes, SIMPLIFIED_CHINESE_LOCALE);
        assert_ne!(simplified, 0, "{} has no simplified face", path.display());
        for other in ["en", "ko", "ja", "zh-CN", ""] {
            assert_eq!(cjk_face_for_locale(&bytes, other), 0, "{other}");
        }
        assert!(tasty_i18n::font::validate_face(&bytes, simplified).is_ok());
    }
}

#[cfg(test)]
mod locale_font_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn validated_bytes_are_appended_without_reordering_existing_fonts() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let valid_dir = std::env::temp_dir().join(format!(
            "tasty-locale-font-valid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&valid_dir).unwrap();
        let path = valid_dir.join("valid.ttf");
        let bytes = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");
        std::fs::write(&path, bytes).unwrap();
        let mut fonts = egui::FontDefinitions::default();
        let before = fonts.families.clone();
        install_locale_font_fallback(&mut fonts, &path, 0).unwrap();
        assert_eq!(fonts.font_data[LOCALE_FONT_KEY].font.as_ref(), bytes);
        assert_eq!(fonts.font_data[LOCALE_FONT_KEY].index, 0);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let mut expected = before[&family].clone();
            expected.push(LOCALE_FONT_KEY.to_owned());
            assert_eq!(fonts.families[&family], expected);
        }
        std::fs::remove_dir_all(valid_dir).unwrap();
    }

    /// 같은 글꼴 둘을 묶음 파일로 만든다. 표의 위치는 파일 처음부터 세므로 뒤쪽 글꼴은 옮긴다.
    fn collection_of_two(font: &[u8]) -> Vec<u8> {
        let header = 12 + 4 * 2;
        let second_at = header + font.len();
        let mut out = b"ttcf\0\x01\0\0".to_vec();
        out.extend_from_slice(&2u32.to_be_bytes());
        for at in [header, second_at] {
            out.extend_from_slice(&(at as u32).to_be_bytes());
        }
        for at in [header, second_at] {
            let mut font = font.to_vec();
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
    fn the_locale_font_face_number_reaches_egui() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.ttc");
        let font = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");
        std::fs::write(&path, collection_of_two(font)).unwrap();
        let mut fonts = egui::FontDefinitions::default();
        install_locale_font_fallback(&mut fonts, &path, 1).unwrap();
        assert_eq!(fonts.font_data[LOCALE_FONT_KEY].index, 1);
    }

    #[test]
    fn a_face_past_the_end_of_the_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("valid.ttf");
        std::fs::write(
            &path,
            include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf"),
        )
        .unwrap();
        let mut fonts = egui::FontDefinitions::default();
        let err = install_locale_font_fallback(&mut fonts, &path, 1);
        assert!(matches!(err, Err(LocaleFontError::Parse)));
        assert!(!fonts.font_data.contains_key(LOCALE_FONT_KEY));
    }

    #[test]
    fn missing_file_is_a_read_error_and_leaves_fonts_untouched() {
        let mut fonts = egui::FontDefinitions::default();
        let before = fonts.font_data.len();
        let err = install_locale_font_fallback(&mut fonts, std::path::Path::new("/no/such.ttf"), 0);
        assert!(matches!(err, Err(LocaleFontError::Read(_))));
        assert_eq!(fonts.font_data.len(), before);
        assert!(!fonts.font_data.contains_key(LOCALE_FONT_KEY));
    }

    #[test]
    fn non_font_bytes_are_a_parse_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("tasty-locale-font-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.ttf");
        std::fs::write(&path, b"not a font at all").unwrap();
        let mut fonts = egui::FontDefinitions::default();
        let err = install_locale_font_fallback(&mut fonts, &path, 0);
        assert!(matches!(err, Err(LocaleFontError::Parse)));
        assert!(!fonts.font_data.contains_key(LOCALE_FONT_KEY));
        let _ = std::fs::remove_file(&path);
    }
}
