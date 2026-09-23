//! Appearance color primitives.
//!
//! - [`HexColor`] — `#RRGGBB(AA)` 직렬화 색상 (settings/theme 파일에 저장되는 모양)
//! - [`GpuRgba`] / [`GpuRgb`] — GPU 셰이더 입력용 newtype (private field 로 array
//!   literal 직접 대입 차단)
//!
//! surface 종류별 색 묶음 (focused/unfocused × bg/fg) 은 [`crate::theme::SurfaceTheme`]
//! 에 정의돼 있다.
//!
//! 색 생성 강제 모델은 `docs/design/systems/theme.md#색-생성-정책` 참고.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

// ============================================================================
//  HexColor — settings/theme 파일 직렬화용 (#RRGGBB / #RRGGBBAA)
// ============================================================================

/// Straight (unmultiplied) RGBA color stored as u8 components.
///
/// alpha 채널은 0(투명) ~ 255(불투명) 사이의 straight 값이며, GPU/egui 등으로 보낼 때
/// 변환 헬퍼([`Self::to_egui`], [`Self::to_gpu_rgba`])를 사용한다.
/// egui는 내부적으로 premultiplied 표현을 쓰므로 [`Self::to_egui`]는
/// `Color32::from_rgba_unmultiplied`를 호출한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HexColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl HexColor {
    /// Opaque RGB.
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Straight RGBA (alpha is *not* premultiplied into RGB).
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 채널 접근자 (egui::Color32 호환을 위한 method-style getter).
    #[inline]
    pub const fn r(self) -> u8 {
        self.r
    }
    #[inline]
    pub const fn g(self) -> u8 {
        self.g
    }
    #[inline]
    pub const fn b(self) -> u8 {
        self.b
    }
    #[inline]
    pub const fn a(self) -> u8 {
        self.a
    }

    /// Replace alpha channel (straight).
    #[inline]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }

    /// Multiply alpha by `factor` (saturating). RGB는 보존되며, premultiplied로
    /// 변환된 시점의 효과는 egui::Color32::gamma_multiply과 시각적으로 동등하다.
    #[inline]
    pub fn gamma_multiply(self, factor: f32) -> Self {
        let a = ((self.a as f32) * factor).clamp(0.0, 255.0) as u8;
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }

    /// Convert to GPU-friendly `[r, g, b, a]` floats in `0..=1`.
    /// Components are returned **straight** (not premultiplied).
    ///
    /// **Deprecated for new GPU buffer writes** — 새 코드는 [`Self::to_gpu_rgba`] 사용.
    /// 이 메서드는 디버그 출력(`rgba_to_json`) 등 raw array 가 필요한 경계 케이스용.
    pub fn to_float(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }

    /// theme 색 → GPU 표현 변환. **정상 변환 경로**.
    ///
    /// GPU buffer struct(`BgInstance.bg_color` 등) 와 렌더러 함수 시그니처는 모두
    /// [`GpuRgba`] 를 받으므로 호출자는 이 메서드를 거쳐야 한다.
    pub const fn to_gpu_rgba(self) -> GpuRgba {
        GpuRgba::from_hex_color(self)
    }

    /// theme 색 → GPU 3채널 표현 (ANSI 팔레트 등). **정상 변환 경로**.
    pub const fn to_gpu_rgb(self) -> GpuRgb {
        GpuRgb::from_hex_color(self)
    }

    /// Convert to `egui::Color32` (gamma-aware premultiplication via
    /// `Color32::from_rgba_unmultiplied`). 일반적인 변환은 이 메서드를 쓴다.
    ///
    /// alpha < 255인 경우 egui가 sRGB → linear → premultiply → sRGB 순서로
    /// 변환하므로, RGB 채널이 단순히 `r * a / 255`가 아니라 감마 보정된 값으로
    /// 저장된다.
    ///
    /// `egui-compat` 기능이 켜져 있을 때만 노출된다. 헤드리스 플러그인 프로세스는
    /// `default-features = false`로 컴파일하면 이 변환 헬퍼 없이 `HexColor` 자체만
    /// 사용한다.
    #[cfg(feature = "egui-compat")]
    #[allow(clippy::disallowed_methods)] // reason: HexColor → egui 변환 헬퍼의 정의 본거지
    pub fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }

    /// Convert to `egui::Color32` treating `(r, g, b, a)` as **already
    /// premultiplied sRGB bytes**.
    ///
    /// 거의 쓸 일이 없지만, egui 0.31의 `from_rgba_premultiplied`와 비트 단위로
    /// 동일한 결과가 필요할 때(예: 과거 시각 결과를 정확히 재현해야 하는 회귀
    /// 케이스) 사용한다.
    #[cfg(feature = "egui-compat")]
    #[allow(clippy::disallowed_methods)] // reason: HexColor → egui premultiplied 변환 헬퍼의 정의 본거지
    pub fn to_egui_premultiplied(self) -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(self.r, self.g, self.b, self.a)
    }

    /// Serialize to `#RRGGBB` (alpha=255) or `#RRGGBBAA` (otherwise).
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Parse `#RGB`, `#RRGGBB`, or `#RRGGBBAA` (leading `#` optional).
    /// 3-digit shorthand expands each nibble (e.g. `#abc` → `#aabbcc`).
    /// 6-digit form is opaque (alpha=255); 8-digit preserves alpha.
    pub fn from_hex(hex: &str) -> Option<Self> {
        Self::from_hex_const(hex)
    }

    /// `from_hex` 의 const fn 버전. [`hex!`] 매크로가 컴파일 타임 검증에 사용.
    ///
    /// byte 단위 hex digit 변환 — `u8::from_str_radix` 가 stable const fn 이
    /// 아니므로 직접 작성. 동작은 [`Self::from_hex`] 와 동일.
    #[allow(clippy::disallowed_methods)] // reason: hex 문자열 파싱 결과를 HexColor 로 조립하는 정의 본거지
    pub const fn from_hex_const(hex: &str) -> Option<Self> {
        let bytes = hex.as_bytes();
        let (off, len) = if !bytes.is_empty() && bytes[0] == b'#' {
            (1, bytes.len() - 1)
        } else {
            (0, bytes.len())
        };
        match len {
            3 => {
                let r = match hex_nibble(bytes, off) {
                    Some(v) => v,
                    None => return None,
                };
                let g = match hex_nibble(bytes, off + 1) {
                    Some(v) => v,
                    None => return None,
                };
                let b = match hex_nibble(bytes, off + 2) {
                    Some(v) => v,
                    None => return None,
                };
                Some(Self::from_rgb(r * 17, g * 17, b * 17))
            }
            6 => {
                let r = match hex_pair(bytes, off) {
                    Some(v) => v,
                    None => return None,
                };
                let g = match hex_pair(bytes, off + 2) {
                    Some(v) => v,
                    None => return None,
                };
                let b = match hex_pair(bytes, off + 4) {
                    Some(v) => v,
                    None => return None,
                };
                Some(Self::from_rgb(r, g, b))
            }
            8 => {
                let r = match hex_pair(bytes, off) {
                    Some(v) => v,
                    None => return None,
                };
                let g = match hex_pair(bytes, off + 2) {
                    Some(v) => v,
                    None => return None,
                };
                let b = match hex_pair(bytes, off + 4) {
                    Some(v) => v,
                    None => return None,
                };
                let a = match hex_pair(bytes, off + 6) {
                    Some(v) => v,
                    None => return None,
                };
                Some(Self::from_rgba(r, g, b, a))
            }
            _ => None,
        }
    }
}

/// hex digit 한 글자 → 0..=15. const 컨텍스트에서 작동.
const fn hex_nibble(bytes: &[u8], i: usize) -> Option<u8> {
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'0'..=b'9' => Some(bytes[i] - b'0'),
        b'a'..=b'f' => Some(bytes[i] - b'a' + 10),
        b'A'..=b'F' => Some(bytes[i] - b'A' + 10),
        _ => None,
    }
}

/// hex digit 두 글자 → 0..=255. const 컨텍스트에서 작동.
const fn hex_pair(bytes: &[u8], i: usize) -> Option<u8> {
    let hi = match hex_nibble(bytes, i) {
        Some(v) => v,
        None => return None,
    };
    let lo = match hex_nibble(bytes, i + 1) {
        Some(v) => v,
        None => return None,
    };
    Some(hi * 16 + lo)
}

/// 컴파일 타임에 hex 문자열을 검증해서 [`HexColor`] const 로 expansion.
///
/// `pub const X: HexColor = hex!("#1e1e2e");` 처럼 const 컨텍스트에서 사용 가능.
/// 잘못된 hex 는 빌드 에러로 잡힌다 — 런타임 검증 불필요.
///
/// 지원 포맷: `#RGB`, `#RRGGBB`, `#RRGGBBAA` (leading `#` 선택).
///
/// # 예시
///
/// ```
/// use tasty_type_appearance::{color::HexColor, hex};
///
/// pub const BRAND: HexColor = hex!("#89b4fa");
/// pub const TRANSLUCENT: HexColor = hex!("#89b4fa80");
/// ```
#[macro_export]
macro_rules! hex {
    ($s:literal) => {{
        const COLOR: $crate::color::HexColor = match $crate::color::HexColor::from_hex_const($s) {
            ::core::option::Option::Some(c) => c,
            ::core::option::Option::None => {
                panic!(concat!("invalid hex color literal: ", $s))
            }
        };
        COLOR
    }};
}

#[cfg(feature = "egui-compat")]
impl From<HexColor> for egui::Color32 {
    fn from(c: HexColor) -> Self {
        c.to_egui()
    }
}

impl Serialize for HexColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid hex color: {s}")))
    }
}

// ============================================================================
//  GpuRgba / GpuRgb — GPU 셰이더 입력 newtype
// ============================================================================

/// GPU 셰이더 입력용 straight RGBA. wgpu vertex buffer 에 그대로 들어가는 표현.
///
/// 생성 경로:
/// - 정상: [`HexColor::to_gpu_rgba`]
/// - 외부 입력: [`GpuRgba::dangerously_force_from_array`]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct GpuRgba([f32; 4]);

/// GPU 셰이더 입력용 RGB (ANSI 팔레트 등).
///
/// 생성 경로:
/// - 정상: [`HexColor::to_gpu_rgb`]
/// - 외부 입력: [`GpuRgb::dangerously_force_from_array`]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct GpuRgb([f32; 3]);

impl GpuRgba {
    /// 보관 중인 raw `[f32; 4]` 추출. wgpu vertex layout, JSON 직렬화 등에 사용.
    /// 색을 **새로 만드는** 게 아니라 **꺼내는** 용도.
    #[inline]
    pub const fn as_array(self) -> [f32; 4] {
        self.0
    }

    #[inline]
    pub const fn r(self) -> f32 {
        self.0[0]
    }
    #[inline]
    pub const fn g(self) -> f32 {
        self.0[1]
    }
    #[inline]
    pub const fn b(self) -> f32 {
        self.0[2]
    }
    #[inline]
    pub const fn a(self) -> f32 {
        self.0[3]
    }

    /// 내부 정상 변환 경로 — `HexColor::to_gpu_rgba` 가 호출.
    #[inline]
    const fn from_hex_color(c: HexColor) -> Self {
        Self([
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
            c.a as f32 / 255.0,
        ])
    }

    /// ⚠ **외부 입력 전용**.
    ///
    /// 다음 경우에만 사용:
    /// - termwiz `SrgbaTuple` 등 외부 라이브러리가 만든 색 데이터를 GPU 표현으로 받기
    /// - 사용자 픽커/브러시 픽셀 값
    /// - 디스크에서 복원된 scrollback 색
    /// - 테스트 더미
    ///
    /// **theme 색을 만들거나 색을 "디자인" 하는 용도로는 절대 사용 금지.**
    /// 그건 반드시 `~/.tasty/themes/*.toml` 또는 tasty-core 의 const 를 통해야 한다.
    ///
    /// 호출 시 반드시 위 사유 중 하나를 주석으로 명시할 것.
    #[inline]
    pub const fn dangerously_force_from_array(arr: [f32; 4]) -> Self {
        Self(arr)
    }
}

impl GpuRgb {
    /// 보관 중인 raw `[f32; 3]` 추출.
    #[inline]
    pub const fn as_array(self) -> [f32; 3] {
        self.0
    }

    #[inline]
    pub const fn r(self) -> f32 {
        self.0[0]
    }
    #[inline]
    pub const fn g(self) -> f32 {
        self.0[1]
    }
    #[inline]
    pub const fn b(self) -> f32 {
        self.0[2]
    }

    /// 내부 정상 변환 경로 — `HexColor::to_gpu_rgb` 가 호출.
    #[inline]
    const fn from_hex_color(c: HexColor) -> Self {
        Self([c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0])
    }

    /// ⚠ **외부 입력 전용**. 사용 가이드는 [`GpuRgba::dangerously_force_from_array`] 참고.
    #[inline]
    pub const fn dangerously_force_from_array(arr: [f32; 3]) -> Self {
        Self(arr)
    }
}

#[cfg(test)]
mod tests {
    // 테스트는 HexColor 생성/변환 round-trip 을 검증하려고 원시 색상값을 직접
    // 만든다 (UI 색 "디자인" 이 아니라 primitive 자체의 테스트). clippy 의 정상 예외 경로.
    #![allow(clippy::disallowed_methods)]

    use super::*;

    // ── HexColor ──

    #[test]
    fn from_hex_round_trip() {
        let c = HexColor::from_rgb(0x12, 0x34, 0x56);
        assert_eq!(c.to_hex(), "#123456");
        assert_eq!(HexColor::from_hex("#123456"), Some(c));
        assert_eq!(HexColor::from_hex("123456"), Some(c));
    }

    #[test]
    fn from_hex_shorthand_3_digit() {
        // #abc → #aabbcc
        assert_eq!(
            HexColor::from_hex("#abc"),
            Some(HexColor::from_rgb(0xaa, 0xbb, 0xcc))
        );
        assert_eq!(
            HexColor::from_hex("f09"),
            Some(HexColor::from_rgb(0xff, 0x00, 0x99))
        );
    }

    #[test]
    fn from_hex_8_digit_preserves_alpha() {
        let c = HexColor::from_hex("#1234567f").unwrap();
        assert_eq!(c, HexColor::from_rgba(0x12, 0x34, 0x56, 0x7f));
        assert_eq!(c.to_hex(), "#1234567f");
    }

    #[test]
    fn to_hex_drops_alpha_when_opaque() {
        let c = HexColor::from_rgba(0x12, 0x34, 0x56, 0xff);
        assert_eq!(c.to_hex(), "#123456");
    }

    #[test]
    fn from_hex_const_matches_from_hex() {
        let cases = ["#abc", "#abcdef", "#abcdef80", "abc", "abcdef", "abcdef80"];
        for s in cases {
            assert_eq!(HexColor::from_hex(s), HexColor::from_hex_const(s), "{s}",);
        }
    }

    #[test]
    fn hex_macro_basic() {
        const C: HexColor = crate::hex!("#1e1e2e");
        assert_eq!(C, HexColor::from_rgb(0x1e, 0x1e, 0x2e));
    }

    #[test]
    fn hex_macro_with_alpha() {
        const C: HexColor = crate::hex!("#1e1e2e80");
        assert_eq!(C, HexColor::from_rgba(0x1e, 0x1e, 0x2e, 0x80));
    }

    #[test]
    fn hex_macro_shorthand() {
        const C: HexColor = crate::hex!("#abc");
        assert_eq!(C, HexColor::from_rgb(0xaa, 0xbb, 0xcc));
    }

    #[test]
    fn hex_macro_without_hash() {
        const C: HexColor = crate::hex!("89b4fa");
        assert_eq!(C, HexColor::from_rgb(0x89, 0xb4, 0xfa));
    }

    #[test]
    fn from_hex_rejects_bad_lengths() {
        assert_eq!(HexColor::from_hex(""), None);
        assert_eq!(HexColor::from_hex("#12"), None);
        assert_eq!(HexColor::from_hex("#12345"), None);
        assert_eq!(HexColor::from_hex("#1234567"), None);
        assert_eq!(HexColor::from_hex("#123456789"), None);
        assert_eq!(HexColor::from_hex("#zzz"), None);
    }

    #[cfg(feature = "egui-compat")]
    #[test]
    fn straight_alpha_round_trip_via_egui() {
        let c = HexColor::from_rgba(255, 255, 255, 20);
        let e = c.to_egui();
        assert_eq!(e.a(), 20);
        assert!(e.r() > 20 && e.r() < 100);

        let c = HexColor::from_rgba(0, 0, 0, 20);
        let e = c.to_egui();
        assert_eq!(e.r(), 0);
        assert_eq!(e.g(), 0);
        assert_eq!(e.b(), 0);
        assert_eq!(e.a(), 20);

        let opaque = HexColor::from_rgb(0x12, 0x34, 0x56).to_egui();
        assert_eq!(opaque.r(), 0x12);
        assert_eq!(opaque.g(), 0x34);
        assert_eq!(opaque.b(), 0x56);
        assert_eq!(opaque.a(), 255);
    }

    #[cfg(feature = "egui-compat")]
    #[test]
    fn to_egui_premultiplied_bypasses_gamma() {
        let c = HexColor::from_rgba(20, 20, 20, 20);
        let e = c.to_egui_premultiplied();
        assert_eq!(e.r(), 20);
        assert_eq!(e.g(), 20);
        assert_eq!(e.b(), 20);
        assert_eq!(e.a(), 20);
    }

    #[test]
    fn to_float_straight() {
        let c = HexColor::from_rgb(0xff, 0x80, 0x00);
        let f = c.to_float();
        assert!((f[0] - 1.0).abs() < 1e-6);
        assert!((f[1] - 128.0 / 255.0).abs() < 1e-6);
        assert!((f[2] - 0.0).abs() < 1e-6);
        assert!((f[3] - 1.0).abs() < 1e-6);
    }

    // ── GpuRgba / GpuRgb ──

    #[test]
    fn gpu_rgba_round_trip() {
        let c = GpuRgba::dangerously_force_from_array([0.1, 0.2, 0.3, 0.4]);
        assert_eq!(c.as_array(), [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(c.r(), 0.1);
        assert_eq!(c.g(), 0.2);
        assert_eq!(c.b(), 0.3);
        assert_eq!(c.a(), 0.4);
    }

    #[test]
    fn gpu_rgb_round_trip() {
        let c = GpuRgb::dangerously_force_from_array([0.5, 0.6, 0.7]);
        assert_eq!(c.as_array(), [0.5, 0.6, 0.7]);
        assert_eq!(c.r(), 0.5);
        assert_eq!(c.g(), 0.6);
        assert_eq!(c.b(), 0.7);
    }

    #[test]
    fn gpu_rgba_pod_byte_size_matches_array() {
        let c = GpuRgba::dangerously_force_from_array([1.0, 0.5, 0.0, 1.0]);
        let bytes = bytemuck::bytes_of(&c);
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn hex_color_to_gpu_rgba_normalizes() {
        let c = HexColor::from_rgb(255, 128, 0).to_gpu_rgba();
        let a = c.as_array();
        assert!((a[0] - 1.0).abs() < 1e-6);
        assert!((a[1] - 128.0 / 255.0).abs() < 1e-6);
        assert!((a[2] - 0.0).abs() < 1e-6);
        assert!((a[3] - 1.0).abs() < 1e-6);
    }
}
