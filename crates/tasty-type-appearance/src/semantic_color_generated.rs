//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! Semantic 색 접근자는 대응하는 Theme 필드를 반환한다.
//! 테마 밝기에 따른 분기, 합성색, OS·브랜드 고정 색처럼 단순 필드 반환이
//! 아닌 접근자는 theme.rs에 직접 구현한다.

use crate::color::HexColor;

impl crate::theme::Theme {
    /// `semantic.bg-app` → `{primitive.color-neutral-0}`
    #[inline]
    pub fn bg_app(&self) -> HexColor {
        self.crust
    }

    /// `semantic.bg-sidebar` → `{primitive.color-neutral-100}`
    #[inline]
    pub fn bg_sidebar(&self) -> HexColor {
        self.mantle
    }

    /// `semantic.bg-panel` → `{primitive.color-neutral-200}`
    #[inline]
    pub fn bg_panel(&self) -> HexColor {
        self.base
    }

    /// `semantic.surface-raised` → `{primitive.color-neutral-300}`
    #[inline]
    pub fn surface_raised(&self) -> HexColor {
        self.surface0
    }

    /// `semantic.surface-hover` → `{primitive.color-neutral-400}`
    #[inline]
    pub fn surface_hover(&self) -> HexColor {
        self.surface1
    }

    /// `semantic.surface-active` → `{primitive.color-neutral-500}`
    #[inline]
    pub fn surface_active(&self) -> HexColor {
        self.surface2
    }

    /// `semantic.text-primary` → `{primitive.color-neutral-1100}`
    #[inline]
    pub fn text_primary(&self) -> HexColor {
        self.text
    }

    /// `semantic.text-secondary` → `{primitive.color-neutral-1000}`
    #[inline]
    pub fn text_secondary(&self) -> HexColor {
        self.subtext1
    }

    /// `semantic.text-placeholder` → `{primitive.color-neutral-900}`
    #[inline]
    pub fn text_placeholder(&self) -> HexColor {
        self.placeholder
    }

    /// `semantic.accent-primary` → `{primitive.color-blue}`
    #[inline]
    pub fn accent_primary(&self) -> HexColor {
        self.blue
    }

    /// `semantic.accent-info` → `{primitive.color-sky}`
    #[inline]
    pub fn accent_info(&self) -> HexColor {
        self.sky
    }

    /// `semantic.accent-remote` → `{primitive.color-sky}`
    #[inline]
    pub fn accent_remote(&self) -> HexColor {
        self.sky
    }

    /// `semantic.accent-success` → `{primitive.color-green}`
    #[inline]
    pub fn accent_success(&self) -> HexColor {
        self.green
    }

    /// `semantic.accent-warning` → `{primitive.color-yellow}`
    #[inline]
    pub fn accent_warning(&self) -> HexColor {
        self.yellow
    }

    /// `semantic.accent-attention` → `{primitive.color-peach}`
    #[inline]
    pub fn accent_attention(&self) -> HexColor {
        self.peach
    }

    /// `semantic.accent-occupied-soft` → `{primitive.color-green}`
    #[inline]
    pub fn accent_occupied_soft(&self) -> HexColor {
        self.green
    }

    /// `semantic.accent-occupied-hard` → `{primitive.color-peach}`
    #[inline]
    pub fn accent_occupied_hard(&self) -> HexColor {
        self.peach
    }

    /// `semantic.accent-danger` → `{primitive.color-red}`
    #[inline]
    pub fn accent_danger(&self) -> HexColor {
        self.red
    }

    /// `semantic.accent-agent` → `{primitive.color-mauve}`
    #[inline]
    pub fn accent_agent(&self) -> HexColor {
        self.mauve
    }

    /// `semantic.accent-attached` → `{primitive.color-lavender}`
    #[inline]
    pub fn border_attached(&self) -> HexColor {
        self.lavender
    }

    /// `semantic.accent-move` → `{primitive.color-pink}`
    #[inline]
    pub fn accent_move(&self) -> HexColor {
        self.pink
    }

    /// `semantic.accent-data` → `{primitive.color-teal}`
    #[inline]
    pub fn accent_data(&self) -> HexColor {
        self.teal
    }

    /// `semantic.accent-route` → `{primitive.color-lavender}`
    #[inline]
    pub fn accent_route(&self) -> HexColor {
        self.lavender
    }

    /// `semantic.accent-decorative` → `{primitive.color-peach}`
    #[inline]
    pub fn accent_decorative(&self) -> HexColor {
        self.peach
    }

    /// `semantic.attention-needs-input` → `{semantic.accent-warning}`
    #[inline]
    pub fn attention_needs_input(&self) -> HexColor {
        self.yellow
    }

    /// `semantic.attention-completion` → `{semantic.accent-primary}`
    #[inline]
    pub fn attention_completion(&self) -> HexColor {
        self.blue
    }

    /// `semantic.border-default` → `{primitive.color-neutral-300}`
    #[inline]
    pub fn border_default(&self) -> HexColor {
        self.surface0
    }

    /// `semantic.border-strong` → `{primitive.color-neutral-400}`
    #[inline]
    pub fn border_strong(&self) -> HexColor {
        self.surface1
    }

    /// `semantic.border-focus` → `{primitive.color-blue}`
    #[inline]
    pub fn border_focus(&self) -> HexColor {
        self.blue
    }

    /// `semantic.border-frame` → `{primitive.color-neutral-500}`
    #[inline]
    pub fn border_frame(&self) -> HexColor {
        self.surface2
    }
}
