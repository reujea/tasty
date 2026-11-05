//! Mocha 파일을 읽지 못했을 때 사용할 내장 색상.
//! 내장 mocha.toml과 같은 색인지 검사한다.

use std::collections::BTreeMap;

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::{SurfaceTheme, Theme, ThemeColors};

/// Mocha를 읽지 못했을 때 사용할 전체 색상 집합.
#[allow(clippy::disallowed_methods)] // reason: 내장 Mocha 팔레트를 정의하는 곳이다.
pub fn mocha_fallback_colors() -> ThemeColors {
    let mut surface_themes = BTreeMap::new();
    surface_themes.insert("terminal".to_string(), terminal_surface());
    surface_themes.insert("markdown".to_string(), markdown_surface());

    ThemeColors {
        // Surfaces
        crust: HexColor::from_rgb(0x11, 0x11, 0x1b),
        mantle: HexColor::from_rgb(0x18, 0x18, 0x25),
        base: HexColor::from_rgb(0x1e, 0x1e, 0x2e),
        surface0: HexColor::from_rgb(0x31, 0x32, 0x44),
        surface1: HexColor::from_rgb(0x45, 0x47, 0x5a),
        surface2: HexColor::from_rgb(0x58, 0x5b, 0x70),
        // Overlays
        overlay0: HexColor::from_rgb(0x6c, 0x70, 0x86),
        overlay1: HexColor::from_rgb(0x7f, 0x84, 0x9c),
        overlay2: HexColor::from_rgb(0x93, 0x99, 0xb2),
        // Text
        text: HexColor::from_rgb(0xcd, 0xd6, 0xf4),
        subtext1: HexColor::from_rgb(0xba, 0xc2, 0xde),
        subtext0: HexColor::from_rgb(0xa6, 0xad, 0xc8),
        placeholder: HexColor::from_rgb(0xa6, 0xad, 0xc8), // = subtext0
        // Accent
        blue: HexColor::from_rgb(0x89, 0xb4, 0xfa),
        green: HexColor::from_rgb(0xa6, 0xe3, 0xa1),
        red: HexColor::from_rgb(0xf3, 0x8b, 0xa8),
        yellow: HexColor::from_rgb(0xf9, 0xe2, 0xaf),
        peach: HexColor::from_rgb(0xfa, 0xb3, 0x87),
        mauve: HexColor::from_rgb(0xcb, 0xa6, 0xf7),
        teal: HexColor::from_rgb(0x94, 0xe2, 0xd5),
        sky: HexColor::from_rgb(0x89, 0xdc, 0xeb),
        lavender: HexColor::from_rgb(0xb4, 0xbe, 0xfe),
        flamingo: HexColor::from_rgb(0xf2, 0xcd, 0xcd),
        pink: HexColor::from_rgb(0xf5, 0xc2, 0xe7),
        maroon: HexColor::from_rgb(0xeb, 0xa0, 0xac),
        rosewater: HexColor::from_rgb(0xf5, 0xe0, 0xdc),
        // Terminal-specific
        selection_bg: HexColor::from_rgb(0x58, 0x5b, 0x70), // = surface2
        vi_cursor_bg: HexColor::from_rgb(0xb4, 0xbe, 0xfe), // = lavender
        search_match_bg: HexColor::from_rgba(0xf9, 0xe2, 0xaf, 0x4d), // yellow @ ~30%
        search_match_active_bg: HexColor::from_rgba(0xf9, 0xe2, 0xaf, 0xb3), // yellow @ ~70%
        search_match_active_fg: HexColor::from_rgb(0x1e, 0x1e, 0x2e), // = base
        // ANSI 16
        ansi_black: HexColor::from_rgb(0x45, 0x47, 0x5a), // surface1
        ansi_red: HexColor::from_rgb(0xf3, 0x8b, 0xa8),
        ansi_green: HexColor::from_rgb(0xa6, 0xe3, 0xa1),
        ansi_yellow: HexColor::from_rgb(0xf9, 0xe2, 0xaf),
        ansi_blue: HexColor::from_rgb(0x89, 0xb4, 0xfa),
        ansi_magenta: HexColor::from_rgb(0xcb, 0xa6, 0xf7),
        ansi_cyan: HexColor::from_rgb(0x94, 0xe2, 0xd5),
        ansi_white: HexColor::from_rgb(0xba, 0xc2, 0xde), // subtext1
        ansi_bright_black: HexColor::from_rgb(0x6c, 0x70, 0x86), // overlay0
        ansi_bright_red: HexColor::from_rgb(0xf3, 0x8b, 0xa8),
        ansi_bright_green: HexColor::from_rgb(0xa6, 0xe3, 0xa1),
        ansi_bright_yellow: HexColor::from_rgb(0xf9, 0xe2, 0xaf),
        ansi_bright_blue: HexColor::from_rgb(0x89, 0xb4, 0xfa),
        ansi_bright_magenta: HexColor::from_rgb(0xcb, 0xa6, 0xf7),
        ansi_bright_cyan: HexColor::from_rgb(0x89, 0xdc, 0xeb), // sky
        ansi_bright_white: HexColor::from_rgb(0xcd, 0xd6, 0xf4), // text
        surface_themes,
    }
}

/// 전역 테마의 최초 값으로도 사용하는 Mocha 테마.
pub fn mocha_fallback() -> Theme {
    Theme::with_colors(mocha_fallback_colors(), false)
}

/// 빌트인 terminal SurfaceTheme. 검은 배경 + Mocha text/subtext.
#[allow(clippy::disallowed_methods)] // reason: 빌트인 mocha 색상값 리터럴 정의
fn terminal_surface() -> SurfaceTheme {
    SurfaceTheme {
        focused_bg: HexColor::from_rgb(0, 0, 0),            // #000000
        focused_fg: HexColor::from_rgb(0xcd, 0xd6, 0xf4),   // text
        unfocused_bg: HexColor::from_rgb(0x1e, 0x1e, 0x2e), // base
        unfocused_fg: HexColor::from_rgb(0xa6, 0xad, 0xc8), // subtext0
    }
}

/// Markdown의 기본 색상. 웹뷰는 focused_bg를 배경으로 사용한다.
#[allow(clippy::disallowed_methods)] // reason: 빌트인 mocha 색상값 리터럴 정의
fn markdown_surface() -> SurfaceTheme {
    SurfaceTheme {
        focused_bg: HexColor::from_rgb(0x11, 0x11, 0x1b), // crust
        focused_fg: HexColor::from_rgb(0xcd, 0xd6, 0xf4), // text
        unfocused_bg: HexColor::from_rgb(0x18, 0x18, 0x25), // mantle
        unfocused_fg: HexColor::from_rgb(0xa6, 0xad, 0xc8), // subtext0
    }
}
