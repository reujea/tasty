//! 터미널 셀 색을 계산한다. ANSI 기본 팔레트와 기본 전경·배경은 호출자가 테마에서 가져온다.

use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;
use termwiz::color::ColorAttribute;

/// 첫 ANSI 색은 테마를 사용하고 나머지 큐브·회색조는 xterm 규격을 따른다.
pub(crate) fn palette_index_to_rgb(idx: u8, ansi: &[GpuRgb; 16]) -> GpuRgb {
    if idx < 16 {
        ansi[idx as usize]
    } else if idx < 232 {
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let to_f = |v: u8| {
            if v == 0 {
                0.0
            } else {
                (55.0 + 40.0 * v as f32) / 255.0
            }
        };
        // 테마 값이 아닌 xterm 규격의 색을 GPU 색으로 변환한다.
        GpuRgb::dangerously_force_from_array([to_f(r), to_f(g), to_f(b)])
    } else {
        let level = (8 + 10 * (idx - 232) as u16) as f32 / 255.0;
        // 테마 값이 아닌 xterm 규격의 회색조를 GPU 색으로 변환한다.
        GpuRgb::dangerously_force_from_array([level, level, level])
    }
}

/// 기본색·색 속성을 적용하고 reverse 뒤 dim 순서로 처리해 (배경, 전경)을 반환한다.
/// dim은 전경을 배경과 반씩 섞는다. 선택·링크·커서·IME의 표시 보정은 포함하지 않는다.
pub fn compute_cell_colors(
    attrs: &CellAttributes,
    default_bg: GpuRgba,
    default_fg: GpuRgba,
    ansi: &[GpuRgb; 16],
) -> (GpuRgba, GpuRgba) {
    let mut bg = color_attr_to_rgba(&attrs.background(), default_bg, ansi);
    let mut fg = color_attr_to_rgba(&attrs.foreground(), default_fg, ansi);
    if attrs.reverse() {
        std::mem::swap(&mut bg, &mut fg);
    }
    if attrs.intensity() == termwiz::cell::Intensity::Half {
        let bg_a = bg.as_array();
        let fg_a = fg.as_array();
        // 입력 색에서 보간한 값이며 알파는 전경 값을 유지한다.
        fg = GpuRgba::dangerously_force_from_array([
            (fg_a[0] + bg_a[0]) * 0.5,
            (fg_a[1] + bg_a[1]) * 0.5,
            (fg_a[2] + bg_a[2]) * 0.5,
            fg_a[3],
        ]);
    }
    (bg, fg)
}

/// 프로그램이 전경색을 고르지 않은 셀인지 답한다. 값이 기본색과 같아도 SGR로 고른 색은 아니다.
/// 반전 셀의 전경은 기본 바탕이므로 기본 글자가 아니다. bold·dim은 색을 고르지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn reads_default_ink(attrs: &CellAttributes) -> bool {
    attrs.foreground() == ColorAttribute::Default && !attrs.reverse()
}

pub(crate) fn color_attr_to_rgba(
    attr: &ColorAttribute,
    default: GpuRgba,
    ansi: &[GpuRgb; 16],
) -> GpuRgba {
    match attr {
        ColorAttribute::Default => default,
        ColorAttribute::PaletteIndex(idx) => {
            let rgb = palette_index_to_rgb(*idx, ansi).as_array();
            // 기존 RGB에 불투명 알파만 붙인다.
            GpuRgba::dangerously_force_from_array([rgb[0], rgb[1], rgb[2], 1.0])
        }
        ColorAttribute::TrueColorWithPaletteFallback(srgba, _) => {
            // 터미널 escape로 받은 외부 색 값이다.
            GpuRgba::dangerously_force_from_array([srgba.0, srgba.1, srgba.2, srgba.3])
        }
        ColorAttribute::TrueColorWithDefaultFallback(srgba) => {
            // 터미널 escape로 받은 외부 색 값이다.
            GpuRgba::dangerously_force_from_array([srgba.0, srgba.1, srgba.2, srgba.3])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termwiz::cell::Intensity;
    use termwiz::color::{ColorAttribute, SrgbaTuple};

    fn test_ansi() -> [GpuRgb; 16] {
        [GpuRgb::dangerously_force_from_array([0.0; 3]); 16]
    }
    fn test_default_fg() -> GpuRgba {
        GpuRgba::dangerously_force_from_array([0.8, 0.8, 0.95, 1.0])
    }

    #[test]
    fn normal_intensity_keeps_default_fg() {
        let attrs = CellAttributes::default();
        let bg = GpuRgba::dangerously_force_from_array([0.0, 0.0, 0.0, 1.0]);
        let fg = test_default_fg();
        let (out_bg, out_fg) = compute_cell_colors(&attrs, bg, fg, &test_ansi());
        assert_eq!(out_bg, bg);
        assert_eq!(out_fg, fg);
    }

    #[test]
    fn dim_lerps_fg_halfway_toward_bg() {
        let mut attrs = CellAttributes::default();
        attrs.set_intensity(Intensity::Half);
        let bg = GpuRgba::dangerously_force_from_array([0.0, 0.0, 0.0, 1.0]);
        let fg = test_default_fg();
        let (out_bg, out_fg) = compute_cell_colors(&attrs, bg, fg, &test_ansi());
        assert_eq!(out_bg, bg);
        let fg_a = fg.as_array();
        let out_fg_a = out_fg.as_array();
        assert!((out_fg_a[0] - fg_a[0] * 0.5).abs() < 1e-6);
        assert!((out_fg_a[1] - fg_a[1] * 0.5).abs() < 1e-6);
        assert!((out_fg_a[2] - fg_a[2] * 0.5).abs() < 1e-6);
        assert_eq!(out_fg_a[3], 1.0);
    }

    #[test]
    fn dim_applies_after_reverse_swap() {
        let mut attrs = CellAttributes::default();
        attrs.set_intensity(Intensity::Half);
        attrs.set_reverse(true);
        attrs.set_foreground(ColorAttribute::TrueColorWithDefaultFallback(SrgbaTuple(
            1.0, 1.0, 1.0, 1.0,
        )));
        attrs.set_background(ColorAttribute::TrueColorWithDefaultFallback(SrgbaTuple(
            0.0, 0.0, 0.0, 1.0,
        )));
        let default_bg = GpuRgba::dangerously_force_from_array([0.5, 0.5, 0.5, 1.0]);
        let (bg, fg) = compute_cell_colors(&attrs, default_bg, test_default_fg(), &test_ansi());
        let bg_a = bg.as_array();
        let fg_a = fg.as_array();
        assert_eq!(bg_a, [1.0, 1.0, 1.0, 1.0]);
        assert!((fg_a[0] - 0.5).abs() < 1e-6);
        assert!((fg_a[1] - 0.5).abs() < 1e-6);
        assert!((fg_a[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn palette_index_picks_from_ansi_for_index_below_16() {
        let mut ansi = test_ansi();
        ansi[1] = GpuRgb::dangerously_force_from_array([0.9, 0.2, 0.2]);
        let rgb = palette_index_to_rgb(1, &ansi);
        assert_eq!(rgb.as_array(), [0.9, 0.2, 0.2]);
    }

    #[test]
    fn palette_index_color_cube_independent_of_ansi() {
        let ansi = [GpuRgb::dangerously_force_from_array([1.0; 3]); 16];
        let rgb = palette_index_to_rgb(16, &ansi);
        assert_eq!(rgb.as_array(), [0.0, 0.0, 0.0]);
    }
}
