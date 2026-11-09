//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! Component 치수·색·시간을 Theme를 통해 읽는다.
//! 치수는 배율을 적용한 필드를 쓰거나 ui_zoom을 곱한다.
//! 색은 연결된 접근자로 읽고, 시간은 배율 없이 Millis로 반환한다.

use crate::color::{HexColor, PremulColor};
use crate::motion::Millis;
use tasty_type_geometry::length::LogicalPx;

impl crate::theme::Theme {
    /// `component.approval-danger-border` → `{semantic.accent-danger}`
    #[inline]
    pub fn approval_danger_border(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.approval-danger-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn approval_danger_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.attach-refusal-chip-glyph-size` → `{component.move-source-chip-glyph-size}` = 8px
    #[inline]
    pub fn attach_refusal_chip_glyph_size(&self) -> LogicalPx {
        self.move_source_chip_glyph_size()
    }

    /// `component.attach-refusal-chip-size` → `{component.move-source-chip-size}` = 12px
    #[inline]
    pub fn attach_refusal_chip_size(&self) -> LogicalPx {
        self.move_source_chip_size()
    }

    /// `component.attach-refusal-glyph` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn attach_refusal_glyph(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.attach-sync-glyph` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn attach_sync_glyph(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.attach-sync-name-max-width` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn attach_sync_name_max_width(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.attach-sync-name-min-width` → `{primitive.size-40}` = 40px
    #[inline]
    pub fn attach_sync_name_min_width(&self) -> LogicalPx {
        LogicalPx((40.0 * self.ui_zoom).round())
    }

    /// `component.autocomplete-empty-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn autocomplete_empty_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.autocomplete-match-fg` → `{semantic.accent-primary-strong}`
    #[inline]
    pub fn autocomplete_match_fg(&self) -> HexColor {
        self.accent_primary_strong()
    }

    /// `component.autocomplete-match-underline-offset` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn autocomplete_match_underline_offset(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.autocomplete-match-underline-width` → `{semantic.border-width}` = 1px
    #[inline]
    pub fn autocomplete_match_underline_width(&self) -> LogicalPx {
        self.border_width
    }

    /// `component.autocomplete-max-height` → `{primitive.size-220}` = 220px
    #[inline]
    pub fn autocomplete_max_height(&self) -> LogicalPx {
        LogicalPx((220.0 * self.ui_zoom).round())
    }

    /// `component.autocomplete-menu-bg` → `{component.menu-bg}`
    #[inline]
    pub fn autocomplete_menu_bg(&self) -> HexColor {
        self.menu_bg()
    }

    /// `component.autocomplete-menu-border` → `{component.menu-border}`
    #[inline]
    pub fn autocomplete_menu_border(&self) -> HexColor {
        self.menu_border()
    }

    /// `component.autocomplete-menu-radius` → `{component.menu-radius}` = 4px
    #[inline]
    pub fn autocomplete_menu_radius(&self) -> LogicalPx {
        self.menu_radius()
    }

    /// `component.autocomplete-part-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn autocomplete_part_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.autocomplete-row-bg-active` → `{semantic.surface-active}`
    #[inline]
    pub fn autocomplete_row_bg_active(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.autocomplete-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn autocomplete_row_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.autocomplete-row-fg` → `{component.menu-item-fg}`
    #[inline]
    pub fn autocomplete_row_fg(&self) -> HexColor {
        self.menu_item_fg()
    }

    /// `component.autocomplete-row-fg-active` → `{component.menu-item-fg-hover}`
    #[inline]
    pub fn autocomplete_row_fg_active(&self) -> HexColor {
        self.menu_item_fg_hover()
    }

    /// `component.autocomplete-row-height` → `{component.menu-item-height}` = 28px
    #[inline]
    pub fn autocomplete_row_height(&self) -> LogicalPx {
        self.menu_item_height()
    }

    /// `component.autocomplete-row-padding-x` → `{component.menu-item-padding-x}` = 12px
    #[inline]
    pub fn autocomplete_row_padding_x(&self) -> LogicalPx {
        self.menu_item_padding_x()
    }

    /// `component.badge-agent-bg` → `{semantic.accent-agent}`
    #[inline]
    pub fn badge_agent_bg(&self) -> HexColor {
        self.accent_agent()
    }

    /// `component.badge-agent-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn badge_agent_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.badge-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn badge_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.badge-danger-bg` → `{semantic.accent-danger}`
    #[inline]
    pub fn badge_danger_bg(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.badge-danger-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn badge_danger_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.badge-disabled-bg` → `{semantic.state-disabled-fill}`
    #[inline]
    pub fn badge_disabled_bg(&self) -> HexColor {
        self.state_disabled_fill()
    }

    /// `component.badge-disabled-fg` → `{semantic.state-disabled-fg}`
    #[inline]
    pub fn badge_disabled_fg(&self) -> HexColor {
        self.state_disabled_fg()
    }

    /// `component.badge-dot-size` → `{component.status-dot-size}` = 8px
    #[inline]
    pub fn badge_dot_size(&self) -> LogicalPx {
        self.status_dot_size
    }

    /// `component.badge-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn badge_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.badge-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn badge_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.badge-group-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn badge_group_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.badge-neutral-bg` → `{semantic.surface-active}`
    #[inline]
    pub fn badge_neutral_bg(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.badge-neutral-fg` → `{semantic.text-on-active}`
    #[inline]
    pub fn badge_neutral_fg(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.badge-padding-x` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn badge_padding_x(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.badge-primary-bg` → `{semantic.attention-completion}`
    #[inline]
    pub fn badge_primary_bg(&self) -> HexColor {
        self.attention_completion()
    }

    /// `component.badge-primary-fg` → `{semantic.attention-completion-fg}`
    #[inline]
    pub fn badge_primary_fg(&self) -> HexColor {
        self.attention_completion_fg()
    }

    /// `component.badge-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn badge_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.badge-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn badge_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.badge-success-bg` → `{semantic.accent-success-fill}`
    #[inline]
    pub fn badge_success_bg(&self) -> HexColor {
        self.accent_success_fill()
    }

    /// `component.badge-success-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn badge_success_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.badge-warning-bg` → `{semantic.attention-needs-input-fill}`
    #[inline]
    pub fn badge_warning_bg(&self) -> HexColor {
        self.attention_needs_input_fill()
    }

    /// `component.badge-warning-fg` → `{semantic.attention-needs-input-fg}`
    #[inline]
    pub fn badge_warning_fg(&self) -> HexColor {
        self.attention_needs_input_fg()
    }

    /// `component.banner-body-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn banner_body_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.banner-body-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn banner_body_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.banner-button-bg` → `{semantic.surface-on-raised}`
    #[inline]
    pub fn banner_button_bg(&self) -> HexColor {
        self.surface_on_raised()
    }

    /// `component.banner-button-border` → `{semantic.border-frame}`
    #[inline]
    pub fn banner_button_border(&self) -> HexColor {
        self.border_frame()
    }

    /// `component.banner-countdown-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn banner_countdown_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.banner-fade` → `{semantic.motion-ui}` = 120ms
    #[inline]
    pub fn banner_fade(&self) -> Millis {
        Millis(120.0)
    }

    /// `component.banner-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn banner_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.banner-glyph-offset` → `{primitive.size-1}` = 1px
    #[inline]
    pub fn banner_glyph_offset(&self) -> LogicalPx {
        LogicalPx((1.0 * self.ui_zoom).round())
    }

    /// `component.banner-inset-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn banner_inset_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.banner-margin` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn banner_margin(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.banner-more-app-fg` → `{component.menu-item-fg}`
    #[inline]
    pub fn banner_more_app_fg(&self) -> HexColor {
        self.menu_item_fg()
    }

    /// `component.banner-more-app-fg-hover` → `{component.menu-item-fg-hover}`
    #[inline]
    pub fn banner_more_app_fg_hover(&self) -> HexColor {
        self.menu_item_fg_hover()
    }

    /// `component.banner-more-column-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn banner_more_column_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.banner-more-menu-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn banner_more_menu_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.banner-more-menu-border` → `{semantic.border-strong}`
    #[inline]
    pub fn banner_more_menu_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.banner-more-menu-max-width` → `{primitive.size-288}` = 288px
    #[inline]
    pub fn banner_more_menu_max_width(&self) -> LogicalPx {
        LogicalPx((288.0 * self.ui_zoom).round())
    }

    /// `component.banner-more-menu-min-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn banner_more_menu_min_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.banner-more-menu-offset` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn banner_more_menu_offset(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.banner-more-menu-padding` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn banner_more_menu_padding(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.banner-more-menu-radius` → `{component.menu-radius}` = 4px
    #[inline]
    pub fn banner_more_menu_radius(&self) -> LogicalPx {
        self.menu_radius()
    }

    /// `component.banner-more-reason-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn banner_more_reason_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.banner-more-reason-glyph-offset` → `{primitive.size-1}` = 1px
    #[inline]
    pub fn banner_more_reason_glyph_offset(&self) -> LogicalPx {
        LogicalPx((1.0 * self.ui_zoom).round())
    }

    /// `component.banner-more-reserve` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn banner_more_reserve(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
    }

    /// `component.banner-narrow-below` → `{primitive.size-440}` = 440px
    #[inline]
    pub fn banner_narrow_below(&self) -> LogicalPx {
        LogicalPx((440.0 * self.ui_zoom).round())
    }

    /// `component.banner-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn banner_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.banner-padding-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn banner_padding_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.banner-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn banner_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.banner-text-gap` → `{semantic.label-detail-gap}` = 2px
    #[inline]
    pub fn banner_text_gap(&self) -> LogicalPx {
        self.label_detail_gap
    }

    /// `component.banner-title-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn banner_title_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.boot-error-glyph` → `{semantic.accent-danger}`
    #[inline]
    pub fn boot_error_glyph(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.boot-form-width` → `{primitive.size-360}` = 360px
    #[inline]
    pub fn boot_form_width(&self) -> LogicalPx {
        LogicalPx((360.0 * self.ui_zoom).round())
    }

    /// `component.button-agent-bg` → `{semantic.accent-agent}`
    #[inline]
    pub fn button_agent_bg(&self) -> HexColor {
        self.accent_agent()
    }

    /// `component.button-agent-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn button_agent_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.button-danger-bg` → `{semantic.accent-danger}`
    #[inline]
    pub fn button_danger_bg(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.button-danger-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn button_danger_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.button-disabled-bg` → `{semantic.state-disabled-fill}`
    #[inline]
    pub fn button_disabled_bg(&self) -> HexColor {
        self.state_disabled_fill()
    }

    /// `component.button-disabled-border` → `{semantic.state-disabled-border}`
    #[inline]
    pub fn button_disabled_border(&self) -> HexColor {
        self.state_disabled_border()
    }

    /// `component.button-disabled-fg` → `{semantic.state-disabled-fg}`
    #[inline]
    pub fn button_disabled_fg(&self) -> HexColor {
        self.state_disabled_fg()
    }

    /// `component.button-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn button_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.button-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn button_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.button-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn button_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.button-ghost-active-fg` → `{semantic.text-on-active}`
    #[inline]
    pub fn button_ghost_active_fg(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.button-ghost-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn button_ghost_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.button-ghost-fg-hover` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn button_ghost_fg_hover(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.button-ghost-raised-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn button_ghost_raised_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.button-ghost-raised-fg-hover` → `{semantic.text-on-active}`
    #[inline]
    pub fn button_ghost_raised_fg_hover(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.button-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn button_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.button-height-lg` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn button_height_lg(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.button-height-sm` → `{semantic.control-height-tab}` = 24px
    #[inline]
    pub fn button_height_sm(&self) -> LogicalPx {
        self.item_height_tab
    }

    /// `component.button-overlay-active` → `{semantic.overlay-active}`
    #[inline]
    pub fn button_overlay_active(&self) -> PremulColor {
        self.overlay_active()
    }

    /// `component.button-overlay-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn button_overlay_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.button-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn button_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.button-primary-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn button_primary_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.button-primary-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn button_primary_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.button-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn button_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.button-secondary-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn button_secondary_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.button-secondary-border` → `{semantic.border-default}`
    #[inline]
    pub fn button_secondary_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.button-secondary-border-hover` → `{semantic.border-strong}`
    #[inline]
    pub fn button_secondary_border_hover(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.center-state-action-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn center_state_action_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.center-state-error-fg` → `{semantic.accent-danger}`
    #[inline]
    pub fn center_state_error_fg(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.center-state-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn center_state_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.center-state-glyph-fg` → `{semantic.glyph-dim}`
    #[inline]
    pub fn center_state_glyph_fg(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.center-state-glyph-size` → `{semantic.icon-size-lg}` = 24px
    #[inline]
    pub fn center_state_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_lg
    }

    /// `component.center-state-line-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn center_state_line_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.center-state-max-width` → `{semantic.measure-sm}` = 300px
    #[inline]
    pub fn center_state_max_width(&self) -> LogicalPx {
        self.measure_sm
    }

    /// `component.center-state-sub-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn center_state_sub_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.center-state-title-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn center_state_title_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.checkbox-bg` → `{semantic.surface-field}`
    #[inline]
    pub fn checkbox_bg(&self) -> HexColor {
        self.surface_field()
    }

    /// `component.checkbox-bg-checked` → `{semantic.accent-primary}`
    #[inline]
    pub fn checkbox_bg_checked(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.checkbox-border` → `{semantic.border-control}`
    #[inline]
    pub fn checkbox_border(&self) -> HexColor {
        self.border_control()
    }

    /// `component.checkbox-border-focus` → `{semantic.border-focus}`
    #[inline]
    pub fn checkbox_border_focus(&self) -> HexColor {
        self.border_focus()
    }

    /// `component.checkbox-check-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn checkbox_check_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.checkbox-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn checkbox_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.checkbox-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn checkbox_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.codearea-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn codearea_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.codearea-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn codearea_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.codearea-gutter-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn codearea_gutter_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.codearea-gutter-border` → `{semantic.separator}`
    #[inline]
    pub fn codearea_gutter_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.codearea-gutter-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn codearea_gutter_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.codearea-gutter-width` → `{semantic.space-xl}` = 24px
    #[inline]
    pub fn codearea_gutter_width(&self) -> LogicalPx {
        self.spacing_xl
    }

    /// `component.codearea-max-height` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn codearea_max_height(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.codearea-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn codearea_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.codearea-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn codearea_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.convert-popup-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn convert_popup_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.cut-pending-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn cut_pending_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.dag-canvas-bg` → `{semantic.bg-panel}`
    #[inline]
    pub fn dag_canvas_bg(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.dag-canvas-dot` → `{semantic.border-default}`
    #[inline]
    pub fn dag_canvas_dot(&self) -> HexColor {
        self.border_default()
    }

    /// `component.dag-canvas-dot-gap` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn dag_canvas_dot_gap(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.dag-canvas-dot-size` → `{primitive.size-1}` = 1px
    #[inline]
    pub fn dag_canvas_dot_size(&self) -> LogicalPx {
        LogicalPx((1.0 * self.ui_zoom).round())
    }

    /// `component.dag-canvas-padding` → `{semantic.space-lg}` = 16px
    #[inline]
    pub fn dag_canvas_padding(&self) -> LogicalPx {
        self.spacing_lg
    }

    /// `component.dag-chrome-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn dag_chrome_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.dag-chrome-border` → `{semantic.border-strong}`
    #[inline]
    pub fn dag_chrome_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.dag-chrome-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn dag_chrome_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.dag-chrome-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn dag_chrome_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.dag-chrome-inset` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_chrome_inset(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-cycle-fg` → `{semantic.accent-warning-strong}`
    #[inline]
    pub fn dag_cycle_fg(&self) -> HexColor {
        self.accent_warning_strong()
    }

    /// `component.dag-cycle-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn dag_cycle_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.dag-detail-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn dag_detail_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.dag-detail-border` → `{semantic.border-default}`
    #[inline]
    pub fn dag_detail_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.dag-detail-log-bg` → `{semantic.bg-app}`
    #[inline]
    pub fn dag_detail_log_bg(&self) -> HexColor {
        self.bg_app()
    }

    /// `component.dag-detail-log-max-height` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn dag_detail_log_max_height(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.dag-detail-out-max-height` → `{primitive.size-112}` = 112px
    #[inline]
    pub fn dag_detail_out_max_height(&self) -> LogicalPx {
        LogicalPx((112.0 * self.ui_zoom).round())
    }

    /// `component.dag-detail-padding` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn dag_detail_padding(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.dag-detail-sheet-height` → `{primitive.size-220}` = 220px
    #[inline]
    pub fn dag_detail_sheet_height(&self) -> LogicalPx {
        LogicalPx((220.0 * self.ui_zoom).round())
    }

    /// `component.dag-detail-width` → `{primitive.size-288}` = 288px
    #[inline]
    pub fn dag_detail_width(&self) -> LogicalPx {
        LogicalPx((288.0 * self.ui_zoom).round())
    }

    /// `component.dag-edge-arrow-size` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn dag_edge_arrow_size(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.dag-edge-binding` → `{semantic.accent-data-strong}`
    #[inline]
    pub fn dag_edge_binding(&self) -> HexColor {
        self.accent_data_strong()
    }

    /// `component.dag-edge-corner-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn dag_edge_corner_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.dag-edge-depends` → `{semantic.border-control}`
    #[inline]
    pub fn dag_edge_depends(&self) -> HexColor {
        self.border_control()
    }

    /// `component.dag-edge-dim` → `{semantic.border-control}`
    #[inline]
    pub fn dag_edge_dim(&self) -> HexColor {
        self.border_control()
    }

    /// `component.dag-edge-fallback` → `{semantic.accent-attention-raised}`
    #[inline]
    pub fn dag_edge_fallback(&self) -> HexColor {
        self.accent_attention_raised()
    }

    /// `component.dag-edge-highlight` → `{semantic.accent-primary}`
    #[inline]
    pub fn dag_edge_highlight(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.dag-edge-reduce` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn dag_edge_reduce(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.dag-edge-selected-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn dag_edge_selected_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.dag-edge-transition` → `{semantic.accent-route-raised}`
    #[inline]
    pub fn dag_edge_transition(&self) -> HexColor {
        self.accent_route_raised()
    }

    /// `component.dag-edge-width` → `{primitive.size-1}` = 1px
    #[inline]
    pub fn dag_edge_width(&self) -> LogicalPx {
        LogicalPx((1.0 * self.ui_zoom).round())
    }

    /// `component.dag-header-count-fg` → `{component.dag-row-count-fg}`
    #[inline]
    pub fn dag_header_count_fg(&self) -> HexColor {
        self.dag_row_count_fg()
    }

    /// `component.dag-layer-gap` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn dag_layer_gap(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.dag-minimap-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn dag_minimap_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.dag-minimap-height` → `{primitive.size-112}` = 112px
    #[inline]
    pub fn dag_minimap_height(&self) -> LogicalPx {
        LogicalPx((112.0 * self.ui_zoom).round())
    }

    /// `component.dag-minimap-min-surface` → `{primitive.size-560}` = 560px
    #[inline]
    pub fn dag_minimap_min_surface(&self) -> LogicalPx {
        LogicalPx((560.0 * self.ui_zoom).round())
    }

    /// `component.dag-minimap-node` → `{semantic.border-control}`
    #[inline]
    pub fn dag_minimap_node(&self) -> HexColor {
        self.border_control()
    }

    /// `component.dag-minimap-node-dim` → `{semantic.border-control}`
    #[inline]
    pub fn dag_minimap_node_dim(&self) -> HexColor {
        self.border_control()
    }

    /// `component.dag-minimap-viewport` → `{semantic.accent-primary}`
    #[inline]
    pub fn dag_minimap_viewport(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.dag-minimap-width` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn dag_minimap_width(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.dag-node-bar-width` → `{primitive.size-3}` = 3px
    #[inline]
    pub fn dag_node_bar_width(&self) -> LogicalPx {
        LogicalPx((3.0 * self.ui_zoom).round())
    }

    /// `component.dag-node-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn dag_node_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.dag-node-border` → `{semantic.border-strong}`
    #[inline]
    pub fn dag_node_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.dag-node-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn dag_node_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.dag-node-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_node_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-node-height` → `{primitive.size-48}` = 48px
    #[inline]
    pub fn dag_node_height(&self) -> LogicalPx {
        LogicalPx((48.0 * self.ui_zoom).round())
    }

    /// `component.dag-node-hover-bg` → `{semantic.overlay-hover}`
    #[inline]
    pub fn dag_node_hover_bg(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.dag-node-meta-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn dag_node_meta_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.dag-node-meta-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn dag_node_meta_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.dag-node-meta-status-fg` → `{semantic.text-on-active}`
    #[inline]
    pub fn dag_node_meta_status_fg(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.dag-node-name-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn dag_node_name_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.dag-node-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_node_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-node-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn dag_node_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.dag-node-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn dag_node_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.dag-node-row-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn dag_node_row_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.dag-node-selected-ring` → `{semantic.accent-primary}`
    #[inline]
    pub fn dag_node_selected_ring(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.dag-node-selected-ring-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn dag_node_selected_ring_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.dag-node-width` → `{primitive.size-168}` = 168px
    #[inline]
    pub fn dag_node_width(&self) -> LogicalPx {
        LogicalPx((168.0 * self.ui_zoom).round())
    }

    /// `component.dag-phase-awaiting` → `{semantic.accent-warning-strong}`
    #[inline]
    pub fn dag_phase_awaiting(&self) -> HexColor {
        self.accent_warning_strong()
    }

    /// `component.dag-phase-awaiting-label` → `{semantic.accent-warning-strong}`
    #[inline]
    pub fn dag_phase_awaiting_label(&self) -> HexColor {
        self.accent_warning_strong()
    }

    /// `component.dag-popup-height` → `{primitive.size-460}` = 460px
    #[inline]
    pub fn dag_popup_height(&self) -> LogicalPx {
        LogicalPx((460.0 * self.ui_zoom).round())
    }

    /// `component.dag-popup-width` → `{primitive.size-560}` = 560px
    #[inline]
    pub fn dag_popup_width(&self) -> LogicalPx {
        LogicalPx((560.0 * self.ui_zoom).round())
    }

    /// `component.dag-row-count-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn dag_row_count_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.dag-row-count-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn dag_row_count_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.dag-row-height` → `{component.listctrl-row-min-height}` = 36px
    #[inline]
    pub fn dag_row_height(&self) -> LogicalPx {
        self.listctrl_row_min_height()
    }

    /// `component.dag-row-summary-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_row_summary_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-runner-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn dag_runner_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.dag-runner-border` → `{semantic.border-strong}`
    #[inline]
    pub fn dag_runner_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.dag-runner-crashed-fg` → `{semantic.accent-danger-strong}`
    #[inline]
    pub fn dag_runner_crashed_fg(&self) -> HexColor {
        self.accent_danger_strong()
    }

    /// `component.dag-runner-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn dag_runner_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.dag-runner-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_runner_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-runner-height` → `{semantic.control-height-tree}` = 22px
    #[inline]
    pub fn dag_runner_height(&self) -> LogicalPx {
        self.item_height_tree
    }

    /// `component.dag-runner-idle-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn dag_runner_idle_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.dag-runner-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn dag_runner_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.dag-runner-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn dag_runner_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.dag-runner-stalled-fg` → `{semantic.accent-warning-strong}`
    #[inline]
    pub fn dag_runner_stalled_fg(&self) -> HexColor {
        self.accent_warning_strong()
    }

    /// `component.dag-sibling-gap` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn dag_sibling_gap(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.dag-status-cancelled` → `{semantic.glyph-dim-raised}`
    #[inline]
    pub fn dag_status_cancelled(&self) -> HexColor {
        self.glyph_dim_raised()
    }

    /// `component.dag-status-cancelled-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_cancelled_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-cancelled-label` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn dag_status_cancelled_label(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.dag-status-failed` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn dag_status_failed(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.dag-status-failed-label` → `{semantic.accent-danger-strong}`
    #[inline]
    pub fn dag_status_failed_label(&self) -> HexColor {
        self.accent_danger_strong()
    }

    /// `component.dag-status-partially-failed` → `{semantic.accent-attention-raised}`
    #[inline]
    pub fn dag_status_partially_failed(&self) -> HexColor {
        self.accent_attention_raised()
    }

    /// `component.dag-status-partially-failed-label` → `{semantic.accent-attention-strong}`
    #[inline]
    pub fn dag_status_partially_failed_label(&self) -> HexColor {
        self.accent_attention_strong()
    }

    /// `component.dag-status-ready` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn dag_status_ready(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.dag-status-ready-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_ready_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-ready-label` → `{component.dag-status-ready}`
    #[inline]
    pub fn dag_status_ready_label(&self) -> HexColor {
        self.dag_status_ready()
    }

    /// `component.dag-status-running` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn dag_status_running(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.dag-status-running-label` → `{semantic.accent-primary-strong}`
    #[inline]
    pub fn dag_status_running_label(&self) -> HexColor {
        self.accent_primary_strong()
    }

    /// `component.dag-status-skipped` → `{semantic.glyph-dim-raised}`
    #[inline]
    pub fn dag_status_skipped(&self) -> HexColor {
        self.glyph_dim_raised()
    }

    /// `component.dag-status-skipped-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_skipped_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-skipped-label` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn dag_status_skipped_label(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.dag-status-succeeded` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn dag_status_succeeded(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.dag-status-succeeded-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_succeeded_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-succeeded-label` → `{component.dag-status-succeeded}`
    #[inline]
    pub fn dag_status_succeeded_label(&self) -> HexColor {
        self.dag_status_succeeded()
    }

    /// `component.dag-status-unknown` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn dag_status_unknown(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.dag-status-unknown-label` → `{semantic.accent-warning-strong}`
    #[inline]
    pub fn dag_status_unknown_label(&self) -> HexColor {
        self.accent_warning_strong()
    }

    /// `component.dag-status-waiting` → `{semantic.glyph-dim-raised}`
    #[inline]
    pub fn dag_status_waiting(&self) -> HexColor {
        self.glyph_dim_raised()
    }

    /// `component.dag-status-waiting-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_waiting_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-waiting-label` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn dag_status_waiting_label(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.dialog-bg` → `{semantic.bg-panel}`
    #[inline]
    pub fn dialog_bg(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.drilldown-backbar-border` → `{semantic.separator}`
    #[inline]
    pub fn drilldown_backbar_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.drilldown-backbar-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn drilldown_backbar_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.drilldown-backbar-height` → `{primitive.size-36}` = 36px
    #[inline]
    pub fn drilldown_backbar_height(&self) -> LogicalPx {
        LogicalPx((36.0 * self.ui_zoom).round())
    }

    /// `component.drilldown-backbar-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn drilldown_backbar_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.drilldown-backbar-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn drilldown_backbar_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.drilldown-title-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn drilldown_title_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.drilldown-title-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn drilldown_title_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.explorer-address-pending-delay` → `{semantic.motion-ui-fade}` = 200ms
    #[inline]
    pub fn explorer_address_pending_delay(&self) -> Millis {
        Millis(200.0)
    }

    /// `component.explorer-address-pending-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn explorer_address_pending_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.explorer-autoscroll-zone` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn explorer_autoscroll_zone(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.explorer-conflict-width` → `{primitive.size-400}` = 400px
    #[inline]
    pub fn explorer_conflict_width(&self) -> LogicalPx {
        LogicalPx((400.0 * self.ui_zoom).round())
    }

    /// `component.explorer-create-field-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn explorer_create_field_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.explorer-create-indent` → `{semantic.space-lg}` = 16px
    #[inline]
    pub fn explorer_create_indent(&self) -> LogicalPx {
        self.spacing_lg
    }

    /// `component.explorer-cursor-ring` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn explorer_cursor_ring(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.explorer-cursor-ring-width` → `{semantic.border-width}` = 1px
    #[inline]
    pub fn explorer_cursor_ring_width(&self) -> LogicalPx {
        self.border_width
    }

    /// `component.explorer-delete-list-max-height` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn explorer_delete_list_max_height(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.explorer-drag-chip-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn explorer_drag_chip_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.explorer-drag-copy-fg` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn explorer_drag_copy_fg(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.explorer-drag-expand-delay` → `{primitive.duration-800}` = 800ms
    #[inline]
    pub fn explorer_drag_expand_delay(&self) -> Millis {
        Millis(800.0)
    }

    /// `component.explorer-drag-move-fg` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn explorer_drag_move_fg(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.explorer-drag-refused-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn explorer_drag_refused_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.explorer-drop-target-border` → `{semantic.accent-primary}`
    #[inline]
    pub fn explorer_drop_target_border(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.explorer-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn explorer_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.explorer-favorites-hide-below` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn explorer_favorites_hide_below(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.explorer-favorites-pin-height` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn explorer_favorites_pin_height(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.explorer-favorites-pin-min-height` → `{primitive.size-120}` = 120px
    #[inline]
    pub fn explorer_favorites_pin_min_height(&self) -> LogicalPx {
        LogicalPx((120.0 * self.ui_zoom).round())
    }

    /// `component.explorer-favorites-pin-threshold` → `{primitive.size-600}` = 600px
    #[inline]
    pub fn explorer_favorites_pin_threshold(&self) -> LogicalPx {
        LogicalPx((600.0 * self.ui_zoom).round())
    }

    /// `component.explorer-grid-thumb-size` → `{primitive.size-40}` = 40px
    #[inline]
    pub fn explorer_grid_thumb_size(&self) -> LogicalPx {
        LogicalPx((40.0 * self.ui_zoom).round())
    }

    /// `component.explorer-hidden-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn explorer_hidden_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.explorer-link-broken-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn explorer_link_broken_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.explorer-link-glyph` → `{semantic.text-muted}`
    #[inline]
    pub fn explorer_link_glyph(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.explorer-link-glyph-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn explorer_link_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.explorer-list-min-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn explorer_list_min_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.explorer-match-fg` → `{component.autocomplete-match-fg}`
    #[inline]
    pub fn explorer_match_fg(&self) -> HexColor {
        self.autocomplete_match_fg()
    }

    /// `component.explorer-match-fg-active` → `{component.autocomplete-part-fg-active}`
    #[inline]
    pub fn explorer_match_fg_active(&self) -> HexColor {
        self.autocomplete_part_fg_active()
    }

    /// `component.explorer-min-height` → `{primitive.size-180}` = 180px
    #[inline]
    pub fn explorer_min_height(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }

    /// `component.explorer-name-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn explorer_name_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.explorer-name-error-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn explorer_name_error_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.explorer-preview-body-min-height` → `{primitive.size-120}` = 120px
    #[inline]
    pub fn explorer_preview_body_min_height(&self) -> LogicalPx {
        LogicalPx((120.0 * self.ui_zoom).round())
    }

    /// `component.explorer-preview-header-height` → `{primitive.size-40}` = 40px
    #[inline]
    pub fn explorer_preview_header_height(&self) -> LogicalPx {
        LogicalPx((40.0 * self.ui_zoom).round())
    }

    /// `component.explorer-preview-max-width` → `{primitive.size-460}` = 460px
    #[inline]
    pub fn explorer_preview_max_width(&self) -> LogicalPx {
        LogicalPx((460.0 * self.ui_zoom).round())
    }

    /// `component.explorer-preview-min-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn explorer_preview_min_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.explorer-preview-width` → `{primitive.size-288}` = 288px
    #[inline]
    pub fn explorer_preview_width(&self) -> LogicalPx {
        LogicalPx((288.0 * self.ui_zoom).round())
    }

    /// `component.explorer-progress-fill` → `{semantic.accent-primary}`
    #[inline]
    pub fn explorer_progress_fill(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.explorer-progress-height` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn explorer_progress_height(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.explorer-progress-track` → `{semantic.surface-raised}`
    #[inline]
    pub fn explorer_progress_track(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.explorer-props-footer-border` → `{semantic.separator}`
    #[inline]
    pub fn explorer_props_footer_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.explorer-props-label-width` → `{primitive.size-96}` = 96px
    #[inline]
    pub fn explorer_props_label_width(&self) -> LogicalPx {
        LogicalPx((96.0 * self.ui_zoom).round())
    }

    /// `component.explorer-props-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn explorer_props_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.explorer-props-partial-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn explorer_props_partial_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.explorer-props-perm-border` → `{semantic.border-strong}`
    #[inline]
    pub fn explorer_props_perm_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.explorer-props-perm-cell-width` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn explorer_props_perm_cell_width(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
    }

    /// `component.explorer-props-perm-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn explorer_props_perm_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.explorer-props-perm-grid-min-width` → `calc(4 * var(--tasty-explorer-props-perm-cell-width) + 2 * var(--tasty-space-sm) + 2 * var(--tasty-border-width))` = 242px
    #[inline]
    pub fn explorer_props_perm_grid_min_width(&self) -> LogicalPx {
        LogicalPx(
            self.explorer_props_perm_cell_width().value() * 4.0
                + self.spacing_sm.value() * 2.0
                + self.border_width.value() * 2.0,
        )
    }

    /// `component.explorer-props-perm-locked-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn explorer_props_perm_locked_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.explorer-props-perm-role-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn explorer_props_perm_role_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.explorer-props-perm-stack-min-width` → `calc(3 * var(--tasty-explorer-props-perm-cell-width) + 2 * var(--tasty-space-sm) + 2 * var(--tasty-border-width))` = 186px
    #[inline]
    pub fn explorer_props_perm_stack_min_width(&self) -> LogicalPx {
        LogicalPx(
            self.explorer_props_perm_cell_width().value() * 3.0
                + self.spacing_sm.value() * 2.0
                + self.border_width.value() * 2.0,
        )
    }

    /// `component.explorer-props-row-line` → `{primitive.size-20}` = 20px
    #[inline]
    pub fn explorer_props_row_line(&self) -> LogicalPx {
        LogicalPx((20.0 * self.ui_zoom).round())
    }

    /// `component.explorer-props-row-min-height` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn explorer_props_row_min_height(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.explorer-props-row-pad-top` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn explorer_props_row_pad_top(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.explorer-props-value-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn explorer_props_value_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.explorer-props-width` → `{primitive.size-360}` = 360px
    #[inline]
    pub fn explorer_props_width(&self) -> LogicalPx {
        LogicalPx((360.0 * self.ui_zoom).round())
    }

    /// `component.explorer-remote-queue-head-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn explorer_remote_queue_head_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.explorer-search-bar-height` → `{primitive.size-36}` = 36px
    #[inline]
    pub fn explorer_search_bar_height(&self) -> LogicalPx {
        LogicalPx((36.0 * self.ui_zoom).round())
    }

    /// `component.explorer-search-folder-col-width` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn explorer_search_folder_col_width(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.explorer-sidebar-width` → `{primitive.size-196}` = 196px
    #[inline]
    pub fn explorer_sidebar_width(&self) -> LogicalPx {
        LogicalPx((196.0 * self.ui_zoom).round())
    }

    /// `component.explorer-split-border` → `{semantic.separator}`
    #[inline]
    pub fn explorer_split_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.explorer-state-compact-below` → `{primitive.size-120}` = 120px
    #[inline]
    pub fn explorer_state_compact_below(&self) -> LogicalPx {
        LogicalPx((120.0 * self.ui_zoom).round())
    }

    /// `component.explorer-toolbar-compact-below` → `{primitive.size-440}` = 440px
    #[inline]
    pub fn explorer_toolbar_compact_below(&self) -> LogicalPx {
        LogicalPx((440.0 * self.ui_zoom).round())
    }

    /// `component.fh-when-width` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn fh_when_width(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
    }

    /// `component.file-drop-overlay-fg` → `{semantic.accent-primary-strong}`
    #[inline]
    pub fn file_drop_overlay_fg(&self) -> HexColor {
        self.accent_primary_strong()
    }

    /// `component.file-handler-empty-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn file_handler_empty_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.file-handler-form-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn file_handler_form_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.file-handler-form-border` → `{semantic.border-default}`
    #[inline]
    pub fn file_handler_form_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.file-handler-form-caption-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn file_handler_form_caption_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.file-handler-form-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn file_handler_form_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.file-handler-form-info-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn file_handler_form_info_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.file-handler-form-label-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn file_handler_form_label_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.file-handler-form-padding` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn file_handler_form_padding(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.file-handler-form-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn file_handler_form_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.file-handler-id-min-width` → `{primitive.size-96}` = 96px
    #[inline]
    pub fn file_handler_id_min_width(&self) -> LogicalPx {
        LogicalPx((96.0 * self.ui_zoom).round())
    }

    /// `component.file-handler-kind-width` → `{semantic.field-width-md}` = 160px
    #[inline]
    pub fn file_handler_kind_width(&self) -> LogicalPx {
        self.field_width_md
    }

    /// `component.file-handler-prio-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn file_handler_prio_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.file-handler-priority-width` → `{semantic.field-width-xs}` = 90px
    #[inline]
    pub fn file_handler_priority_width(&self) -> LogicalPx {
        self.field_width_xs
    }

    /// `component.file-handler-select-width` → `{semantic.field-width-lg}` = 200px
    #[inline]
    pub fn file_handler_select_width(&self) -> LogicalPx {
        self.field_width_lg
    }

    /// `component.file-handler-summary-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn file_handler_summary_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.file-handler-summary-removed-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn file_handler_summary_removed_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.font-combo-list-max-height` → `{component.select-search-list-max-height}` = 300px
    #[inline]
    pub fn font_combo_list_max_height(&self) -> LogicalPx {
        self.select_search_list_max_height()
    }

    /// `component.font-combo-search-inset` → `{component.select-search-inset}` = 4px
    #[inline]
    pub fn font_combo_search_inset(&self) -> LogicalPx {
        self.select_search_inset()
    }

    /// `component.font-preview-min-width` → `{semantic.field-width-lg}` = 200px
    #[inline]
    pub fn font_preview_min_width(&self) -> LogicalPx {
        self.field_width_lg
    }

    /// `component.font-preview-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn font_preview_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.font-preview-padding-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn font_preview_padding_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.fp-bar-hysteresis` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn fp_bar_hysteresis(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.fp-crumb-current-min-width` → `{primitive.size-96}` = 96px
    #[inline]
    pub fn fp_crumb_current_min_width(&self) -> LogicalPx {
        LogicalPx((96.0 * self.ui_zoom).round())
    }

    /// `component.fp-crumb-max-width` → `{primitive.size-180}` = 180px
    #[inline]
    pub fn fp_crumb_max_width(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }

    /// `component.fp-crumb-menu-max-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn fp_crumb_menu_max_width(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.fp-crumb-menu-min-width` → `{primitive.size-180}` = 180px
    #[inline]
    pub fn fp_crumb_menu_min_width(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }

    /// `component.fp-crumb-min-width` → `{primitive.size-64}` = 64px
    #[inline]
    pub fn fp_crumb_min_width(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }

    /// `component.fp-filter-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn fp_filter_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.fp-filter-max-width` → `{semantic.field-width-md}` = 160px
    #[inline]
    pub fn fp_filter_max_width(&self) -> LogicalPx {
        self.field_width_md
    }

    /// `component.fp-folder-file-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn fp_folder_file_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.fp-folder-hint-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn fp_folder_hint_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.fp-folder-path-fg` → `{component.input-readonly-fg}`
    #[inline]
    pub fn fp_folder_path_fg(&self) -> HexColor {
        self.input_readonly_fg()
    }

    /// `component.fp-footer-label-width` → `{primitive.size-64}` = 64px
    #[inline]
    pub fn fp_footer_label_width(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }

    /// `component.fp-footer-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn fp_footer_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.fp-header-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn fp_header_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.fp-inset-end` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn fp_inset_end(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.fp-inset-start` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn fp_inset_start(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.fp-list-head-pad-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn fp_list_head_pad_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.fp-path-pad-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn fp_path_pad_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.fp-popup-min-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn fp_popup_min_width(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.fp-row-pad-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn fp_row_pad_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.fp-section-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn fp_section_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.git-toolbar-height` → `{semantic.toolbar-height}` = 32px
    #[inline]
    pub fn git_toolbar_height(&self) -> LogicalPx {
        self.toolbar_height
    }

    /// `component.help-hint-color` → `{semantic.text-muted}`
    #[inline]
    pub fn help_hint_color(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.help-hint-color-hover` → `{semantic.text-secondary}`
    #[inline]
    pub fn help_hint_color_hover(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.help-hint-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn help_hint_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.help-hint-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn help_hint_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.html-address-bar-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn html_address_bar_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.html-address-bar-border` → `{semantic.separator}`
    #[inline]
    pub fn html_address_bar_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.html-address-bar-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn html_address_bar_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.html-address-bar-height` → `{primitive.size-40}` = 40px
    #[inline]
    pub fn html_address_bar_height(&self) -> LogicalPx {
        LogicalPx((40.0 * self.ui_zoom).round())
    }

    /// `component.html-address-bar-nav-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn html_address_bar_nav_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.html-address-bar-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn html_address_bar_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.html-address-error-glyph` → `{semantic.accent-danger}`
    #[inline]
    pub fn html_address_error_glyph(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.html-address-error-height` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn html_address_error_height(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.html-address-field-min-width` → `{primitive.size-96}` = 96px
    #[inline]
    pub fn html_address_field_min_width(&self) -> LogicalPx {
        LogicalPx((96.0 * self.ui_zoom).round())
    }

    /// `component.html-address-value-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn html_address_value_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.html-limit-banner-glyph` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn html_limit_banner_glyph(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.html-receiving-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn html_receiving_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.html-receiving-glyph` → `{semantic.glyph-dim}`
    #[inline]
    pub fn html_receiving_glyph(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.html-remote-banner-glyph` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn html_remote_banner_glyph(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.html-remote-marker-allowed-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn html_remote_marker_allowed_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.html-remote-marker-fg` → `{semantic.glyph-dim-raised}`
    #[inline]
    pub fn html_remote_marker_fg(&self) -> HexColor {
        self.glyph_dim_raised()
    }

    /// `component.html-remote-marker-fg-hover` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn html_remote_marker_fg_hover(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.html-remote-marker-hit` → `{primitive.size-20}` = 20px
    #[inline]
    pub fn html_remote_marker_hit(&self) -> LogicalPx {
        LogicalPx((20.0 * self.ui_zoom).round())
    }

    /// `component.html-remote-marker-hover-bg` → `{semantic.overlay-hover}`
    #[inline]
    pub fn html_remote_marker_hover_bg(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.html-remote-marker-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn html_remote_marker_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.html-script-banner-glyph` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn html_script_banner_glyph(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.html-script-marker-allowed-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn html_script_marker_allowed_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.html-script-marker-fg` → `{semantic.glyph-dim}`
    #[inline]
    pub fn html_script_marker_fg(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.html-script-marker-fg-hover` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn html_script_marker_fg_hover(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.html-script-marker-hit` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn html_script_marker_hit(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.html-script-marker-hover-bg` → `{semantic.overlay-hover}`
    #[inline]
    pub fn html_script_marker_hover_bg(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.html-script-marker-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn html_script_marker_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.html-size-banner-glyph` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn html_size_banner_glyph(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.icon-button-bg-active` → `{semantic.overlay-active}`
    #[inline]
    pub fn icon_button_bg_active(&self) -> PremulColor {
        self.overlay_active()
    }

    /// `component.icon-button-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn icon_button_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.icon-button-fg-hover` → `{semantic.text-secondary}`
    #[inline]
    pub fn icon_button_fg_hover(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.icon-button-overlay-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn icon_button_overlay_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.icon-button-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn icon_button_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.icon-button-size` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn icon_button_size(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.icon-button-size-sm` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn icon_button_size_sm(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.image-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn image_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.image-handle-size` → `{primitive.size-6}` = 6px
    #[inline]
    pub fn image_handle_size(&self) -> LogicalPx {
        LogicalPx((6.0 * self.ui_zoom).round())
    }

    /// `component.image-path-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn image_path_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.image-popup-btn-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn image_popup_btn_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.image-popup-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn image_popup_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.image-popup-pad-top` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn image_popup_pad_top(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.image-popup-pad-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn image_popup_pad_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.image-popup-title-font-size` → `{semantic.font-size-max}` = 14px
    #[inline]
    pub fn image_popup_title_font_size(&self) -> LogicalPx {
        self.font_size_max
    }

    /// `component.image-popup-width` → `{primitive.size-300}` = 300px
    #[inline]
    pub fn image_popup_width(&self) -> LogicalPx {
        LogicalPx((300.0 * self.ui_zoom).round())
    }

    /// `component.image-size-input-width` → `{primitive.size-64}` = 64px
    #[inline]
    pub fn image_size_input_width(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }

    /// `component.image-too-large-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn image_too_large_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.image-zoom-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn image_zoom_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.image-zoom-min-width` → `{primitive.size-40}` = 40px
    #[inline]
    pub fn image_zoom_min_width(&self) -> LogicalPx {
        LogicalPx((40.0 * self.ui_zoom).round())
    }

    /// `component.info-modal-max-height` → `{primitive.size-360}` = 360px
    #[inline]
    pub fn info_modal_max_height(&self) -> LogicalPx {
        LogicalPx((360.0 * self.ui_zoom).round())
    }

    /// `component.info-modal-min-height` → `{primitive.size-140}` = 140px
    #[inline]
    pub fn info_modal_min_height(&self) -> LogicalPx {
        LogicalPx((140.0 * self.ui_zoom).round())
    }

    /// `component.info-modal-para-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn info_modal_para_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.info-modal-scroll-edge` → `{semantic.border-default}`
    #[inline]
    pub fn info_modal_scroll_edge(&self) -> HexColor {
        self.border_default()
    }

    /// `component.info-modal-title-edge` → `{semantic.border-default}`
    #[inline]
    pub fn info_modal_title_edge(&self) -> HexColor {
        self.border_default()
    }

    /// `component.info-modal-width` → `{primitive.size-440}` = 440px
    #[inline]
    pub fn info_modal_width(&self) -> LogicalPx {
        LogicalPx((440.0 * self.ui_zoom).round())
    }

    /// `component.input-bg` → `{semantic.surface-field}`
    #[inline]
    pub fn input_bg(&self) -> HexColor {
        self.surface_field()
    }

    /// `component.input-border` → `{semantic.border-control}`
    #[inline]
    pub fn input_border(&self) -> HexColor {
        self.border_control()
    }

    /// `component.input-border-focus` → `{semantic.border-focus}`
    #[inline]
    pub fn input_border_focus(&self) -> HexColor {
        self.border_focus()
    }

    /// `component.input-border-invalid` → `{semantic.accent-danger}`
    #[inline]
    pub fn input_border_invalid(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.input-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn input_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.input-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn input_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.input-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn input_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.input-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn input_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.input-icon-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn input_icon_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.input-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn input_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.input-placeholder` → `{semantic.text-placeholder}`
    #[inline]
    pub fn input_placeholder(&self) -> HexColor {
        self.text_placeholder()
    }

    /// `component.input-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn input_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.input-raised-bg` → `{semantic.bg-panel}`
    #[inline]
    pub fn input_raised_bg(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.input-raised-border` → `{semantic.border-control-raised}`
    #[inline]
    pub fn input_raised_border(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.input-readonly-bg` → `{semantic.state-disabled-fill}`
    #[inline]
    pub fn input_readonly_bg(&self) -> HexColor {
        self.state_disabled_fill()
    }

    /// `component.input-readonly-border` → `{semantic.state-disabled-border}`
    #[inline]
    pub fn input_readonly_border(&self) -> HexColor {
        self.state_disabled_border()
    }

    /// `component.input-readonly-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn input_readonly_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.input-readonly-raised-bg` → `{semantic.state-disabled-fill-raised}`
    #[inline]
    pub fn input_readonly_raised_bg(&self) -> HexColor {
        self.state_disabled_fill_raised()
    }

    /// `component.input-readonly-raised-border` → `{semantic.state-disabled-border-raised}`
    #[inline]
    pub fn input_readonly_raised_border(&self) -> HexColor {
        self.state_disabled_border_raised()
    }

    /// `component.kb-ie-action-column-width` → `{primitive.size-288}` = 288px
    #[inline]
    pub fn kb_ie_action_column_width(&self) -> LogicalPx {
        LogicalPx((288.0 * self.ui_zoom).round())
    }

    /// `component.kb-ie-from-column-width` → `{primitive.size-120}` = 120px
    #[inline]
    pub fn kb_ie_from_column_width(&self) -> LogicalPx {
        LogicalPx((120.0 * self.ui_zoom).round())
    }

    /// `component.kb-ie-notice-inset` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn kb_ie_notice_inset(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.kb-ie-select-column-width` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn kb_ie_select_column_width(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.kb-ie-slot-height` → `{semantic.control-height-tab}` = 24px
    #[inline]
    pub fn kb_ie_slot_height(&self) -> LogicalPx {
        self.item_height_tab
    }

    /// `component.kb-ie-slot-min-width` → `{primitive.size-140}` = 140px
    #[inline]
    pub fn kb_ie_slot_min_width(&self) -> LogicalPx {
        LogicalPx((140.0 * self.ui_zoom).round())
    }

    /// `component.kb-os-reserved-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn kb_os_reserved_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.kb-plugin-caption-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn kb_plugin_caption_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.kb-plugin-caption-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn kb_plugin_caption_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.kb-plugin-control-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn kb_plugin_control_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.kb-plugin-control-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn kb_plugin_control_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.kb-plugin-draft-dot` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn kb_plugin_draft_dot(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.kb-plugin-draft-dot-size` → `{component.status-dot-size-compact}` = 6px
    #[inline]
    pub fn kb_plugin_draft_dot_size(&self) -> LogicalPx {
        self.status_dot_size_compact()
    }

    /// `component.kb-plugin-error-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn kb_plugin_error_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.kb-plugin-list-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn kb_plugin_list_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.kb-plugin-mode-width` → `{semantic.field-width-md}` = 160px
    #[inline]
    pub fn kb_plugin_mode_width(&self) -> LogicalPx {
        self.field_width_md
    }

    /// `component.kb-plugin-none-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn kb_plugin_none_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.kb-plugin-picker-width` → `{semantic.field-width-lg}` = 200px
    #[inline]
    pub fn kb_plugin_picker_width(&self) -> LogicalPx {
        self.field_width_lg
    }

    /// `component.kb-plugin-record-height` → `{component.kb-plugin-control-height}` = 28px
    #[inline]
    pub fn kb_plugin_record_height(&self) -> LogicalPx {
        self.kb_plugin_control_height()
    }

    /// `component.kb-plugin-row-min-height` → `{component.settings-row-min-height}` = 32px
    #[inline]
    pub fn kb_plugin_row_min_height(&self) -> LogicalPx {
        self.settings_row_min_height()
    }

    /// `component.kb-plugin-row-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn kb_plugin_row_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.kb-plugin-separator` → `{semantic.separator}`
    #[inline]
    pub fn kb_plugin_separator(&self) -> PremulColor {
        self.separator
    }

    /// `component.kb-plugin-slot-width` → `{semantic.field-width-lg}` = 200px
    #[inline]
    pub fn kb_plugin_slot_width(&self) -> LogicalPx {
        self.field_width_lg
    }

    /// `component.kb-plugin-title-gap` → `{component.settings-label-gap}` = 16px
    #[inline]
    pub fn kb_plugin_title_gap(&self) -> LogicalPx {
        self.settings_label_gap()
    }

    /// `component.kb-plugin-title-width` → `{component.settings-label-width}` = 150px
    #[inline]
    pub fn kb_plugin_title_width(&self) -> LogicalPx {
        self.settings_label_width()
    }

    /// `component.kb-record-add-width` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn kb_record_add_width(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.kb-record-border` → `{semantic.border-control}`
    #[inline]
    pub fn kb_record_border(&self) -> HexColor {
        self.border_control()
    }

    /// `component.kb-record-border-hover` → `{semantic.border-control-raised}`
    #[inline]
    pub fn kb_record_border_hover(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.kb-record-empty-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn kb_record_empty_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.kb-record-height` → `{component.kb-ie-slot-height}` = 24px
    #[inline]
    pub fn kb_record_height(&self) -> LogicalPx {
        self.kb_ie_slot_height()
    }

    /// `component.kb-record-width` → `{component.kb-ie-slot-min-width}` = 140px
    #[inline]
    pub fn kb_record_width(&self) -> LogicalPx {
        self.kb_ie_slot_min_width()
    }

    /// `component.kb-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn kb_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.kbd-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn kbd_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.kbd-border` → `{semantic.border-strong}`
    #[inline]
    pub fn kbd_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.kbd-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn kbd_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.kbd-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn kbd_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.kbd-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn kbd_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.kbd-padding-x` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn kbd_padding_x(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.kbd-plus-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn kbd_plus_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.kbd-plus-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn kbd_plus_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.kbd-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn kbd_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.kbd-shadow-depth` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn kbd_shadow_depth(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.kbd-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn kbd_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.listctrl-chevron-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn listctrl_chevron_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.listctrl-desc-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn listctrl_desc_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.listctrl-desc-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn listctrl_desc_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.listctrl-divider` → `{semantic.separator}`
    #[inline]
    pub fn listctrl_divider(&self) -> PremulColor {
        self.separator
    }

    /// `component.listctrl-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn listctrl_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.listctrl-icon-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn listctrl_icon_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.listctrl-label-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn listctrl_label_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.listctrl-label-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn listctrl_label_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.listctrl-part-fg-selected` → `{semantic.text-on-active}`
    #[inline]
    pub fn listctrl_part_fg_selected(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.listctrl-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn listctrl_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.listctrl-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn listctrl_row_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.listctrl-row-bg-selected` → `{semantic.surface-active}`
    #[inline]
    pub fn listctrl_row_bg_selected(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.listctrl-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn listctrl_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.listctrl-row-min-height` → `{primitive.size-36}` = 36px
    #[inline]
    pub fn listctrl_row_min_height(&self) -> LogicalPx {
        LogicalPx((36.0 * self.ui_zoom).round())
    }

    /// `component.listctrl-row-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn listctrl_row_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.listctrl-row-padding-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn listctrl_row_padding_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.listctrl-selected-bar` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn listctrl_selected_bar(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.listctrl-selected-bar-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn listctrl_selected_bar_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.loading-lockup-font-size` → `{semantic.font-size-brand-lockup}` = 38px
    #[inline]
    pub fn loading_lockup_font_size(&self) -> LogicalPx {
        LogicalPx((38.0 * self.ui_zoom).round())
    }

    /// `component.loading-lockup-tracking` → `{primitive.letter-spacing-n1px}` = -1px
    #[inline]
    pub fn loading_lockup_tracking(&self) -> LogicalPx {
        LogicalPx((-(1.0 * self.ui_zoom)).round())
    }

    /// `component.loading-overlay-delay` → `{semantic.motion-ui-fade}` = 200ms
    #[inline]
    pub fn loading_overlay_delay(&self) -> Millis {
        Millis(200.0)
    }

    /// `component.loading-overlay-indicator` → `{component.spinner-indicator}`
    #[inline]
    pub fn loading_overlay_indicator(&self) -> HexColor {
        self.spinner_indicator()
    }

    /// `component.loading-overlay-lnb-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn loading_overlay_lnb_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.loading-overlay-rail-size` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn loading_overlay_rail_size(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.loading-overlay-surface-bed-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn loading_overlay_surface_bed_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.loading-overlay-surface-bed-border` → `{semantic.border-default}`
    #[inline]
    pub fn loading_overlay_surface_bed_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.loading-overlay-surface-bed-size` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn loading_overlay_surface_bed_size(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.loading-overlay-surface-inset` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn loading_overlay_surface_inset(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.loading-overlay-surface-size` → `{component.spinner-size}` = 16px
    #[inline]
    pub fn loading_overlay_surface_size(&self) -> LogicalPx {
        self.spinner_size
    }

    /// `component.loading-overlay-tab-size` → `{component.tab-icon-size}` = 14px
    #[inline]
    pub fn loading_overlay_tab_size(&self) -> LogicalPx {
        self.tab_icon_size()
    }

    /// `component.md-address-bar-bg` → `{component.html-address-bar-bg}`
    #[inline]
    pub fn md_address_bar_bg(&self) -> HexColor {
        self.html_address_bar_bg()
    }

    /// `component.md-address-bar-border` → `{component.html-address-bar-border}`
    #[inline]
    pub fn md_address_bar_border(&self) -> PremulColor {
        self.html_address_bar_border()
    }

    /// `component.md-address-bar-gap` → `{component.html-address-bar-gap}` = 8px
    #[inline]
    pub fn md_address_bar_gap(&self) -> LogicalPx {
        self.html_address_bar_gap()
    }

    /// `component.md-address-bar-height` → `{component.html-address-bar-height}` = 40px
    #[inline]
    pub fn md_address_bar_height(&self) -> LogicalPx {
        self.html_address_bar_height()
    }

    /// `component.md-address-bar-nav-gap` → `{component.html-address-bar-nav-gap}` = 4px
    #[inline]
    pub fn md_address_bar_nav_gap(&self) -> LogicalPx {
        self.html_address_bar_nav_gap()
    }

    /// `component.md-address-bar-padding-x` → `{component.html-address-bar-padding-x}` = 8px
    #[inline]
    pub fn md_address_bar_padding_x(&self) -> LogicalPx {
        self.html_address_bar_padding_x()
    }

    /// `component.md-address-error-glyph` → `{component.html-address-error-glyph}`
    #[inline]
    pub fn md_address_error_glyph(&self) -> HexColor {
        self.html_address_error_glyph()
    }

    /// `component.md-address-error-height` → `{component.html-address-error-height}` = 24px
    #[inline]
    pub fn md_address_error_height(&self) -> LogicalPx {
        self.html_address_error_height()
    }

    /// `component.md-address-field-min-width` → `{component.html-address-field-min-width}` = 96px
    #[inline]
    pub fn md_address_field_min_width(&self) -> LogicalPx {
        self.html_address_field_min_width()
    }

    /// `component.md-address-go-hide-below` → `{semantic.field-width-md}` = 160px
    #[inline]
    pub fn md_address_go_hide_below(&self) -> LogicalPx {
        self.field_width_md
    }

    /// `component.md-address-pending-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn md_address_pending_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.md-address-stale-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn md_address_stale_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.md-address-stale-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn md_address_stale_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.md-address-value-fg` → `{component.html-address-value-fg}`
    #[inline]
    pub fn md_address_value_fg(&self) -> HexColor {
        self.html_address_value_fg()
    }

    /// `component.md-callout-icon-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn md_callout_icon_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.md-callout-marker-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn md_callout_marker_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.md-code-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn md_code_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.md-code-border` → `{semantic.separator}`
    #[inline]
    pub fn md_code_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.md-doc-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn md_doc_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.md-find-current-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn md_find_current_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.md-find-current-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn md_find_current_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.md-find-hit-bg` → `{semantic.notify-warning-fill}`
    #[inline]
    pub fn md_find_hit_bg(&self) -> HexColor {
        self.notify_warning_fill()
    }

    /// `component.md-find-hit-fg` → `{semantic.notify-warning-fg}`
    #[inline]
    pub fn md_find_hit_fg(&self) -> HexColor {
        self.notify_warning_fg()
    }

    /// `component.md-quote-bar` → `{semantic.border-strong}`
    #[inline]
    pub fn md_quote_bar(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.md-quote-bar-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn md_quote_bar_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.md-quote-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn md_quote_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.md-rule` → `{semantic.separator}`
    #[inline]
    pub fn md_rule(&self) -> PremulColor {
        self.separator
    }

    /// `component.md-table-border` → `{semantic.border-strong}`
    #[inline]
    pub fn md_table_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.md-table-cell-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn md_table_cell_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.md-table-cell-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn md_table_cell_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.md-table-cell-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn md_table_cell_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.md-table-header-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn md_table_header_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.md-table-header-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn md_table_header_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.md-table-row-bg` → `{semantic.bg-panel}`
    #[inline]
    pub fn md_table_row_bg(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.md-table-row-bg-unfocused` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn md_table_row_bg_unfocused(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.md-table-row-bg-zebra` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn md_table_row_bg_zebra(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.md-table-row-bg-zebra-unfocused` → `{semantic.bg-panel}`
    #[inline]
    pub fn md_table_row_bg_zebra_unfocused(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.menu-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn menu_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.menu-border` → `{semantic.border-strong}`
    #[inline]
    pub fn menu_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.menu-item-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn menu_item_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.menu-item-check-fg` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn menu_item_check_fg(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.menu-item-check-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn menu_item_check_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.menu-item-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn menu_item_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.menu-item-fg-hover` → `{semantic.text-on-active}`
    #[inline]
    pub fn menu_item_fg_hover(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.menu-item-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn menu_item_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.menu-item-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn menu_item_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.menu-item-part-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn menu_item_part_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.menu-item-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn menu_item_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.menu-item-selected-fg` → `{component.menu-item-fg-hover}`
    #[inline]
    pub fn menu_item_selected_fg(&self) -> HexColor {
        self.menu_item_fg_hover()
    }

    /// `component.menu-item-shortcut-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn menu_item_shortcut_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.menu-item-wrap-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn menu_item_wrap_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.menu-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn menu_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.menu-separator` → `{semantic.border-strong}`
    #[inline]
    pub fn menu_separator(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.modhint-empty-row-min-height` → `{primitive.size-20}` = 20px
    #[inline]
    pub fn modhint_empty_row_min_height(&self) -> LogicalPx {
        LogicalPx((20.0 * self.ui_zoom).round())
    }

    /// `component.modhint-fade` → `{semantic.motion-ui-fade}` = 200ms
    #[inline]
    pub fn modhint_fade(&self) -> Millis {
        Millis(200.0)
    }

    /// `component.modhint-grip-fg` → `{semantic.glyph-dim}`
    #[inline]
    pub fn modhint_grip_fg(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.modhint-grip-inset` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn modhint_grip_inset(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.modhint-grip-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn modhint_grip_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.modhint-header-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn modhint_header_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.modhint-header-height` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn modhint_header_height(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.modhint-hold-delay` → `{semantic.motion-hold-reveal}` = 500ms
    #[inline]
    pub fn modhint_hold_delay(&self) -> Millis {
        Millis(500.0)
    }

    /// `component.modhint-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn modhint_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.modhint-role-text-fg` → `{semantic.text-on-active}`
    #[inline]
    pub fn modhint_role_text_fg(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.modhint-row-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn modhint_row_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.modhint-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn modhint_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.modhint-row-min-height` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn modhint_row_min_height(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.move-source-chip-glyph-size` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn move_source_chip_glyph_size(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.move-source-chip-size` → `{primitive.size-12}` = 12px
    #[inline]
    pub fn move_source_chip_size(&self) -> LogicalPx {
        LogicalPx((12.0 * self.ui_zoom).round())
    }

    /// `component.move-source-dash` → `{primitive.size-4}` = 4px
    #[inline]
    pub fn move_source_dash(&self) -> LogicalPx {
        LogicalPx((4.0 * self.ui_zoom).round())
    }

    /// `component.move-source-dash-gap` → `{primitive.size-4}` = 4px
    #[inline]
    pub fn move_source_dash_gap(&self) -> LogicalPx {
        LogicalPx((4.0 * self.ui_zoom).round())
    }

    /// `component.move-source-glyph` → `{semantic.accent-move-raised}`
    #[inline]
    pub fn move_source_glyph(&self) -> HexColor {
        self.accent_move_raised()
    }

    /// `component.move-source-glyph-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn move_source_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.move-source-ring` → `{semantic.accent-move-raised}`
    #[inline]
    pub fn move_source_ring(&self) -> HexColor {
        self.accent_move_raised()
    }

    /// `component.move-source-ring-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn move_source_ring_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.multiselect-all-fg` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn multiselect_all_fg(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.multiselect-bg` → `{component.select-bg}`
    #[inline]
    pub fn multiselect_bg(&self) -> HexColor {
        self.select_bg()
    }

    /// `component.multiselect-border` → `{component.select-border}`
    #[inline]
    pub fn multiselect_border(&self) -> HexColor {
        self.select_border()
    }

    /// `component.multiselect-border-focus` → `{component.select-border-focus}`
    #[inline]
    pub fn multiselect_border_focus(&self) -> HexColor {
        self.select_border_focus()
    }

    /// `component.multiselect-border-hover` → `{semantic.border-control-raised}`
    #[inline]
    pub fn multiselect_border_hover(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.multiselect-chevron-fg` → `{component.select-chevron-fg}`
    #[inline]
    pub fn multiselect_chevron_fg(&self) -> HexColor {
        self.select_chevron_fg()
    }

    /// `component.multiselect-chevron-offset` → `{component.select-chevron-offset}` = 8px
    #[inline]
    pub fn multiselect_chevron_offset(&self) -> LogicalPx {
        self.select_chevron_offset()
    }

    /// `component.multiselect-chevron-room` → `{component.select-chevron-room}` = 28px
    #[inline]
    pub fn multiselect_chevron_room(&self) -> LogicalPx {
        self.select_chevron_room()
    }

    /// `component.multiselect-fg` → `{component.select-fg}`
    #[inline]
    pub fn multiselect_fg(&self) -> HexColor {
        self.select_fg()
    }

    /// `component.multiselect-font-size` → `{component.select-font-size}` = 13px
    #[inline]
    pub fn multiselect_font_size(&self) -> LogicalPx {
        self.select_font_size()
    }

    /// `component.multiselect-height` → `{component.select-height}` = 28px
    #[inline]
    pub fn multiselect_height(&self) -> LogicalPx {
        self.select_height()
    }

    /// `component.multiselect-menu-bg` → `{component.menu-bg}`
    #[inline]
    pub fn multiselect_menu_bg(&self) -> HexColor {
        self.menu_bg()
    }

    /// `component.multiselect-menu-border` → `{component.menu-border}`
    #[inline]
    pub fn multiselect_menu_border(&self) -> HexColor {
        self.menu_border()
    }

    /// `component.multiselect-menu-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn multiselect_menu_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.multiselect-menu-max-height` → `{component.autocomplete-max-height}` = 220px
    #[inline]
    pub fn multiselect_menu_max_height(&self) -> LogicalPx {
        self.autocomplete_max_height()
    }

    /// `component.multiselect-menu-max-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn multiselect_menu_max_width(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.multiselect-menu-padding` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn multiselect_menu_padding(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.multiselect-menu-radius` → `{component.menu-radius}` = 4px
    #[inline]
    pub fn multiselect_menu_radius(&self) -> LogicalPx {
        self.menu_radius()
    }

    /// `component.multiselect-padding-x` → `{component.select-padding-x}` = 12px
    #[inline]
    pub fn multiselect_padding_x(&self) -> LogicalPx {
        self.select_padding_x()
    }

    /// `component.multiselect-radius` → `{component.select-radius}` = 4px
    #[inline]
    pub fn multiselect_radius(&self) -> LogicalPx {
        self.select_radius()
    }

    /// `component.multiselect-row-bg-active` → `{semantic.surface-active}`
    #[inline]
    pub fn multiselect_row_bg_active(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.multiselect-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn multiselect_row_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.multiselect-row-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn multiselect_row_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.multiselect-row-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn multiselect_row_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.multiselect-row-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn multiselect_row_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.multiselect-row-height` → `{component.menu-item-height}` = 28px
    #[inline]
    pub fn multiselect_row_height(&self) -> LogicalPx {
        self.menu_item_height()
    }

    /// `component.multiselect-row-padding-x` → `{component.menu-item-padding-x}` = 12px
    #[inline]
    pub fn multiselect_row_padding_x(&self) -> LogicalPx {
        self.menu_item_padding_x()
    }

    /// `component.multiselect-separator` → `{semantic.separator}`
    #[inline]
    pub fn multiselect_separator(&self) -> PremulColor {
        self.separator
    }

    /// `component.multiselect-summary-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn multiselect_summary_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.multiselect-summary-fg-empty` → `{semantic.text-placeholder}`
    #[inline]
    pub fn multiselect_summary_fg_empty(&self) -> HexColor {
        self.text_placeholder()
    }

    /// `component.notice-column-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notice_column_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notice-column-inset` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notice_column_inset(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notice-column-width` → `{primitive.size-360}` = 360px
    #[inline]
    pub fn notice_column_width(&self) -> LogicalPx {
        LogicalPx((360.0 * self.ui_zoom).round())
    }

    /// `component.notification-badge-error-bg` → `{semantic.notify-error}`
    #[inline]
    pub fn notification_badge_error_bg(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.notification-badge-error-fg` → `{semantic.notify-error-fg}`
    #[inline]
    pub fn notification_badge_error_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.notification-badge-glyph-gap` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn notification_badge_glyph_gap(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.notification-badge-glyph-size` → `{primitive.size-10}` = 10px
    #[inline]
    pub fn notification_badge_glyph_size(&self) -> LogicalPx {
        LogicalPx((10.0 * self.ui_zoom).round())
    }

    /// `component.notification-badge-group-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn notification_badge_group_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.notification-badge-info-bg` → `{semantic.notify-info}`
    #[inline]
    pub fn notification_badge_info_bg(&self) -> HexColor {
        self.attention_completion()
    }

    /// `component.notification-badge-info-fg` → `{semantic.notify-info-fg}`
    #[inline]
    pub fn notification_badge_info_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.notification-badge-stack-gap` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn notification_badge_stack_gap(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.notification-badge-warning-bg` → `{semantic.notify-warning-fill}`
    #[inline]
    pub fn notification_badge_warning_bg(&self) -> HexColor {
        self.notify_warning_fill()
    }

    /// `component.notification-badge-warning-fg` → `{semantic.notify-warning-fg}`
    #[inline]
    pub fn notification_badge_warning_fg(&self) -> HexColor {
        self.notify_warning_fg()
    }

    /// `component.notification-item-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn notification_item_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.notification-item-bg-selected` → `{semantic.surface-active}`
    #[inline]
    pub fn notification_item_bg_selected(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.notification-item-body-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn notification_item_body_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.notification-item-body-fg-read` → `{semantic.text-muted}`
    #[inline]
    pub fn notification_item_body_fg_read(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.notification-item-closed-fg` → `{semantic.text-placeholder}`
    #[inline]
    pub fn notification_item_closed_fg(&self) -> HexColor {
        self.text_placeholder()
    }

    /// `component.notification-item-error-glyph` → `{semantic.notify-error}`
    #[inline]
    pub fn notification_item_error_glyph(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.notification-item-fg-selected` → `{semantic.text-on-active}`
    #[inline]
    pub fn notification_item_fg_selected(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.notification-item-glyph-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notification_item_glyph_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notification-item-glyph-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn notification_item_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.notification-item-info-glyph` → `{semantic.notify-info}`
    #[inline]
    pub fn notification_item_info_glyph(&self) -> HexColor {
        self.attention_completion()
    }

    /// `component.notification-item-line-gap` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn notification_item_line_gap(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.notification-item-meta-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn notification_item_meta_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.notification-item-more-fg` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn notification_item_more_fg(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.notification-item-pad-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn notification_item_pad_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.notification-item-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notification_item_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notification-item-separator` → `{semantic.separator}`
    #[inline]
    pub fn notification_item_separator(&self) -> PremulColor {
        self.separator
    }

    /// `component.notification-item-title-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn notification_item_title_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.notification-item-title-fg-read` → `{semantic.text-secondary}`
    #[inline]
    pub fn notification_item_title_fg_read(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.notification-item-unread-dot` → `{semantic.notify-unread}`
    #[inline]
    pub fn notification_item_unread_dot(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.notification-item-unread-dot-selected` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn notification_item_unread_dot_selected(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.notification-item-unread-dot-size` → `{component.badge-dot-size}` = 8px
    #[inline]
    pub fn notification_item_unread_dot_size(&self) -> LogicalPx {
        self.badge_dot_size()
    }

    /// `component.notification-item-warning-glyph` → `{semantic.accent-warning-glyph}`
    #[inline]
    pub fn notification_item_warning_glyph(&self) -> HexColor {
        self.accent_warning_glyph()
    }

    /// `component.notification-rail-width` → `{primitive.size-44}` = 44px
    #[inline]
    pub fn notification_rail_width(&self) -> LogicalPx {
        LogicalPx((44.0 * self.ui_zoom).round())
    }

    /// `component.notification-summary-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn notification_summary_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.notification-summary-bg-open` → `{semantic.overlay-active}`
    #[inline]
    pub fn notification_summary_bg_open(&self) -> PremulColor {
        self.overlay_active()
    }

    /// `component.notification-summary-icon-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notification_summary_icon_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notification-summary-label-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn notification_summary_label_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.notification-summary-label-fg-empty` → `{semantic.text-muted}`
    #[inline]
    pub fn notification_summary_label_fg_empty(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.notification-summary-meta-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn notification_summary_meta_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.notification-summary-pad` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notification_summary_pad(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notification-summary-pad-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn notification_summary_pad_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.notification-summary-preview-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn notification_summary_preview_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.notification-summary-preview-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn notification_summary_preview_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.notification-summary-row-height` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn notification_summary_row_height(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.notifications-popup-height` → `{primitive.size-400}` = 400px
    #[inline]
    pub fn notifications_popup_height(&self) -> LogicalPx {
        LogicalPx((400.0 * self.ui_zoom).round())
    }

    /// `component.notifications-popup-width` → `{primitive.size-352}` = 352px
    #[inline]
    pub fn notifications_popup_width(&self) -> LogicalPx {
        LogicalPx((352.0 * self.ui_zoom).round())
    }

    /// `component.notifications-toolbar-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn notifications_toolbar_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.number-field-min-width` → `{semantic.field-width-xs}` = 90px
    #[inline]
    pub fn number_field_min_width(&self) -> LogicalPx {
        self.field_width_xs
    }

    /// `component.number-field-note-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn number_field_note_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.number-field-unit-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn number_field_unit_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.number-field-unit-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn number_field_unit_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.palette-list-max-height` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn palette_list_max_height(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.palette-width` → `{primitive.size-540}` = 540px
    #[inline]
    pub fn palette_width(&self) -> LogicalPx {
        LogicalPx((540.0 * self.ui_zoom).round())
    }

    /// `component.pending-indicator-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn pending_indicator_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.pending-indicator-border` → `{semantic.border-strong}`
    #[inline]
    pub fn pending_indicator_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.pending-indicator-compact-below` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn pending_indicator_compact_below(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.pending-indicator-delay` → `{component.loading-overlay-delay}` = 200ms
    #[inline]
    pub fn pending_indicator_delay(&self) -> Millis {
        self.loading_overlay_delay()
    }

    /// `component.pending-indicator-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn pending_indicator_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.pending-indicator-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn pending_indicator_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.pending-indicator-height` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn pending_indicator_height(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.pending-indicator-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn pending_indicator_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.pending-indicator-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn pending_indicator_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.pending-indicator-sep-fg` → `{semantic.text-muted-raised}`
    #[inline]
    pub fn pending_indicator_sep_fg(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// `component.pending-indicator-spinner-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn pending_indicator_spinner_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.pending-indicator-time-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn pending_indicator_time_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.perm-granted-fg` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn perm_granted_fg(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.perm-missing-fg` → `{semantic.accent-warning-glyph}`
    #[inline]
    pub fn perm_missing_fg(&self) -> HexColor {
        self.accent_warning_glyph()
    }

    /// `component.perm-row-height` → `{component.settings-row-min-height}` = 32px
    #[inline]
    pub fn perm_row_height(&self) -> LogicalPx {
        self.settings_row_min_height()
    }

    /// `component.perm-status-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn perm_status_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.perm-unknown-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn perm_unknown_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.perm-unobservable-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn perm_unobservable_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.plugin-avatar-border-width` → `{semantic.border-width}` = 1px
    #[inline]
    pub fn plugin_avatar_border_width(&self) -> LogicalPx {
        self.border_width
    }

    /// `component.plugin-avatar-fg` → `{semantic.accent-primary-strong}`
    #[inline]
    pub fn plugin_avatar_fg(&self) -> HexColor {
        self.accent_primary_strong()
    }

    /// `component.plugin-avatar-initial-font-size-lg` → `{semantic.font-size-max}` = 14px
    #[inline]
    pub fn plugin_avatar_initial_font_size_lg(&self) -> LogicalPx {
        self.font_size_max
    }

    /// `component.plugin-avatar-initial-font-size-sm` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn plugin_avatar_initial_font_size_sm(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.plugin-avatar-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn plugin_avatar_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.plugin-avatar-size-lg` → `{primitive.size-46}` = 46px
    #[inline]
    pub fn plugin_avatar_size_lg(&self) -> LogicalPx {
        LogicalPx((46.0 * self.ui_zoom).round())
    }

    /// `component.plugin-avatar-size-sm` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn plugin_avatar_size_sm(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.plugin-trust-body-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn plugin_trust_body_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.plugin-trust-body-trusted-fg` → `{semantic.accent-success-ink}`
    #[inline]
    pub fn plugin_trust_body_trusted_fg(&self) -> HexColor {
        self.accent_success_ink()
    }

    /// `component.plugin-trust-glyph-danger` → `{semantic.accent-danger}`
    #[inline]
    pub fn plugin_trust_glyph_danger(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.plugin-trust-glyph-success` → `{semantic.accent-success-ink}`
    #[inline]
    pub fn plugin_trust_glyph_success(&self) -> HexColor {
        self.accent_success_ink()
    }

    /// `component.plugin-trust-glyph-warning` → `{semantic.accent-warning-glyph}`
    #[inline]
    pub fn plugin_trust_glyph_warning(&self) -> HexColor {
        self.accent_warning_glyph()
    }

    /// `component.plugin-trust-title-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn plugin_trust_title_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.plugins-header-glyph` → `{semantic.accent-decorative}`
    #[inline]
    pub fn plugins_header_glyph(&self) -> HexColor {
        self.accent_decorative()
    }

    /// `component.plugins-list-width` → `{primitive.size-288}` = 288px
    #[inline]
    pub fn plugins_list_width(&self) -> LogicalPx {
        LogicalPx((288.0 * self.ui_zoom).round())
    }

    /// `component.popup-content-margin` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn popup_content_margin(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.popup-title-btn-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn popup_title_btn_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.popup-title-btn-size` → `{component.icon-button-size-sm}` = 24px
    #[inline]
    pub fn popup_title_btn_size(&self) -> LogicalPx {
        self.icon_button_size_sm()
    }

    /// `component.popup-title-edge-inset` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn popup_title_edge_inset(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.popup-title-text-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn popup_title_text_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.port-addr-col-min-width` → `{primitive.size-140}` = 140px
    #[inline]
    pub fn port_addr_col_min_width(&self) -> LogicalPx {
        LogicalPx((140.0 * self.ui_zoom).round())
    }

    /// `component.port-favorites-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn port_favorites_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.port-favorites-border` → `{semantic.separator}`
    #[inline]
    pub fn port_favorites_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.port-favorites-max-height` → `{primitive.size-112}` = 112px
    #[inline]
    pub fn port_favorites_max_height(&self) -> LogicalPx {
        LogicalPx((112.0 * self.ui_zoom).round())
    }

    /// `component.port-favorites-row-height` → `{semantic.control-height-tree}` = 22px
    #[inline]
    pub fn port_favorites_row_height(&self) -> LogicalPx {
        self.item_height_tree
    }

    /// `component.port-process-col-min-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn port_process_col_min_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.port-star-col-width` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn port_star_col_width(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.port-star-off` → `{semantic.text-muted}`
    #[inline]
    pub fn port_star_off(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.port-star-off-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn port_star_off_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.port-star-on` → `{semantic.accent-warning-glyph}`
    #[inline]
    pub fn port_star_on(&self) -> HexColor {
        self.accent_warning_glyph()
    }

    /// `component.port-star-on-active` → `{semantic.accent-warning-on-active}`
    #[inline]
    pub fn port_star_on_active(&self) -> HexColor {
        self.accent_warning_on_active()
    }

    /// `component.port-state-none-dot` → `{component.status-dot-idle}`
    #[inline]
    pub fn port_state_none_dot(&self) -> HexColor {
        self.status_dot_idle()
    }

    /// `component.preset-cfg-draft-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn preset_cfg_draft_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.preset-cfg-field-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn preset_cfg_field_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.preset-cfg-footer-height` → `{primitive.size-52}` = 52px
    #[inline]
    pub fn preset_cfg_footer_height(&self) -> LogicalPx {
        LogicalPx((52.0 * self.ui_zoom).round())
    }

    /// `component.preset-cfg-footer-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn preset_cfg_footer_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.preset-cfg-form-max-width` → `{primitive.size-460}` = 460px
    #[inline]
    pub fn preset_cfg_form_max_width(&self) -> LogicalPx {
        LogicalPx((460.0 * self.ui_zoom).round())
    }

    /// `component.preset-cfg-form-padding` → `{semantic.space-lg}` = 16px
    #[inline]
    pub fn preset_cfg_form_padding(&self) -> LogicalPx {
        self.spacing_lg
    }

    /// `component.preset-cfg-header-height` → `{primitive.size-44}` = 44px
    #[inline]
    pub fn preset_cfg_header_height(&self) -> LogicalPx {
        LogicalPx((44.0 * self.ui_zoom).round())
    }

    /// `component.preset-leaf-label-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn preset_leaf_label_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.preset-leaf-selected-ring-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn preset_leaf_selected_ring_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.preset-leaf-summary-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn preset_leaf_summary_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.preset-leaf-value-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn preset_leaf_value_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.preset-split-divider-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn preset_split_divider_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.preset-split-zone-border` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn preset_split_zone_border(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.progress-fill-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn progress_fill_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.progress-height` → `{primitive.size-4}` = 4px
    #[inline]
    pub fn progress_height(&self) -> LogicalPx {
        LogicalPx((4.0 * self.ui_zoom).round())
    }

    /// `component.progress-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn progress_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.progress-track-bg` → `{semantic.bg-app}`
    #[inline]
    pub fn progress_track_bg(&self) -> HexColor {
        self.bg_app()
    }

    /// `component.rail-category-line` → `{semantic.text-muted}`
    #[inline]
    pub fn rail_category_line(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.rail-category-line-hover` → `{semantic.text-secondary}`
    #[inline]
    pub fn rail_category_line_hover(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.rail-category-line-length` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn rail_category_line_length(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.rail-category-line-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn rail_category_line_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.rail-flat-divider-length` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn rail_flat_divider_length(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.remote-filter-dropdown-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn remote_filter_dropdown_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.remote-filter-menu-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn remote_filter_menu_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.remote-label-col` → `{primitive.size-112}` = 112px
    #[inline]
    pub fn remote_label_col(&self) -> LogicalPx {
        LogicalPx((112.0 * self.ui_zoom).round())
    }

    /// `component.restore-notice-body-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn restore_notice_body_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.restore-notice-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn restore_notice_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.restore-notice-glyph` → `{semantic.notify-error}`
    #[inline]
    pub fn restore_notice_glyph(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.restore-notice-label-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn restore_notice_label_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.restore-notice-padding` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn restore_notice_padding(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.restore-notice-path-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn restore_notice_path_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.restore-notice-path-border` → `{semantic.border-default}`
    #[inline]
    pub fn restore_notice_path_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.restore-notice-path-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn restore_notice_path_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.restore-notice-width` → `{component.notice-column-width}` = 360px
    #[inline]
    pub fn restore_notice_width(&self) -> LogicalPx {
        self.notice_column_width()
    }

    /// `component.search-bar-border` → `{component.menu-border}`
    #[inline]
    pub fn search_bar_border(&self) -> HexColor {
        self.menu_border()
    }

    /// `component.segtoggle-on-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn segtoggle_on_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.segtoggle-on-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn segtoggle_on_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.select-bg` → `{semantic.surface-field}`
    #[inline]
    pub fn select_bg(&self) -> HexColor {
        self.surface_field()
    }

    /// `component.select-border` → `{semantic.border-control}`
    #[inline]
    pub fn select_border(&self) -> HexColor {
        self.border_control()
    }

    /// `component.select-border-focus` → `{semantic.border-focus}`
    #[inline]
    pub fn select_border_focus(&self) -> HexColor {
        self.border_focus()
    }

    /// `component.select-chevron-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn select_chevron_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.select-chevron-offset` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn select_chevron_offset(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.select-chevron-room` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn select_chevron_room(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.select-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn select_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.select-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn select_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.select-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn select_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.select-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn select_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.select-placeholder` → `{component.input-placeholder}`
    #[inline]
    pub fn select_placeholder(&self) -> HexColor {
        self.input_placeholder()
    }

    /// `component.select-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn select_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.select-raised-bg` → `{semantic.bg-panel}`
    #[inline]
    pub fn select_raised_bg(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.select-raised-border` → `{semantic.border-control-raised}`
    #[inline]
    pub fn select_raised_border(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.select-raised-disabled-bg` → `{semantic.state-disabled-fill-raised}`
    #[inline]
    pub fn select_raised_disabled_bg(&self) -> HexColor {
        self.state_disabled_fill_raised()
    }

    /// `component.select-raised-disabled-border` → `{semantic.state-disabled-border-raised}`
    #[inline]
    pub fn select_raised_disabled_border(&self) -> HexColor {
        self.state_disabled_border_raised()
    }

    /// `component.select-search-field-bg` → `{component.input-raised-bg}`
    #[inline]
    pub fn select_search_field_bg(&self) -> HexColor {
        self.input_raised_bg()
    }

    /// `component.select-search-field-border` → `{component.input-raised-border}`
    #[inline]
    pub fn select_search_field_border(&self) -> HexColor {
        self.input_raised_border()
    }

    /// `component.select-search-field-border-focus` → `{component.input-border-focus}`
    #[inline]
    pub fn select_search_field_border_focus(&self) -> HexColor {
        self.input_border_focus()
    }

    /// `component.select-search-field-fg` → `{component.input-fg}`
    #[inline]
    pub fn select_search_field_fg(&self) -> HexColor {
        self.input_fg()
    }

    /// `component.select-search-field-gap` → `{component.input-gap}` = 8px
    #[inline]
    pub fn select_search_field_gap(&self) -> LogicalPx {
        self.input_gap()
    }

    /// `component.select-search-field-height` → `{component.input-height}` = 28px
    #[inline]
    pub fn select_search_field_height(&self) -> LogicalPx {
        self.input_height()
    }

    /// `component.select-search-field-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn select_search_field_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.select-search-icon-fg` → `{component.input-icon-fg}`
    #[inline]
    pub fn select_search_icon_fg(&self) -> HexColor {
        self.input_icon_fg()
    }

    /// `component.select-search-inset` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn select_search_inset(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.select-search-list-max-height` → `{primitive.size-300}` = 300px
    #[inline]
    pub fn select_search_list_max_height(&self) -> LogicalPx {
        LogicalPx((300.0 * self.ui_zoom).round())
    }

    /// `component.select-search-menu-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn select_search_menu_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.select-search-menu-max-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn select_search_menu_max_width(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.select-search-placeholder` → `{component.input-placeholder}`
    #[inline]
    pub fn select_search_placeholder(&self) -> HexColor {
        self.input_placeholder()
    }

    /// `component.select-search-status-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn select_search_status_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.select-search-trigger-border-hover` → `{semantic.border-control-raised}`
    #[inline]
    pub fn select_search_trigger_border_hover(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.select-search-value-placeholder` → `{semantic.text-placeholder}`
    #[inline]
    pub fn select_search_value_placeholder(&self) -> HexColor {
        self.text_placeholder()
    }

    /// `component.settings-apply-note-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn settings_apply_note_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.settings-apply-note-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn settings_apply_note_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.settings-apply-note-glyph` → `{semantic.text-muted}`
    #[inline]
    pub fn settings_apply_note_glyph(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.settings-content-max-width` → `{primitive.size-620}` = 620px
    #[inline]
    pub fn settings_content_max_width(&self) -> LogicalPx {
        LogicalPx((620.0 * self.ui_zoom).round())
    }

    /// `component.settings-label-gap` → `{semantic.space-lg}` = 16px
    #[inline]
    pub fn settings_label_gap(&self) -> LogicalPx {
        self.spacing_lg
    }

    /// `component.settings-label-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn settings_label_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.settings-label-width` → `{primitive.size-150}` = 150px
    #[inline]
    pub fn settings_label_width(&self) -> LogicalPx {
        LogicalPx((150.0 * self.ui_zoom).round())
    }

    /// `component.settings-row-caption-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn settings_row_caption_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.settings-row-gap` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn settings_row_gap(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.settings-row-min-height` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn settings_row_min_height(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.settings-row-stack-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn settings_row_stack_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.settings-row-stack-hysteresis` → `{semantic.space-lg}` = 16px
    #[inline]
    pub fn settings_row_stack_hysteresis(&self) -> LogicalPx {
        self.spacing_lg
    }

    /// `component.settings-sidebar-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn settings_sidebar_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }

    /// `component.settings-window-height` → `{primitive.size-700}` = 700px
    #[inline]
    pub fn settings_window_height(&self) -> LogicalPx {
        LogicalPx((700.0 * self.ui_zoom).round())
    }

    /// `component.settings-window-width` → `{primitive.size-1100}` = 1100px
    #[inline]
    pub fn settings_window_width(&self) -> LogicalPx {
        LogicalPx((1100.0 * self.ui_zoom).round())
    }

    /// `component.sidebar-button-label-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn sidebar_button_label_font_size(&self) -> LogicalPx {
        self.sidebar_button_label_font_size
    }

    /// `component.sidebar-category-header-bg` → `{semantic.bg-app}`
    #[inline]
    pub fn sidebar_category_header_bg(&self) -> HexColor {
        self.bg_app()
    }

    /// `component.sidebar-category-header-border` → `{semantic.separator}`
    #[inline]
    pub fn sidebar_category_header_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.sidebar-category-header-count-fg` → `{semantic.text-quiet}`
    #[inline]
    pub fn sidebar_category_header_count_fg(&self) -> HexColor {
        self.text_quiet()
    }

    /// `component.sidebar-category-header-count-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn sidebar_category_header_count_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.sidebar-category-header-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn sidebar_category_header_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.sidebar-category-header-pad-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn sidebar_category_header_pad_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.sidebar-category-header-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn sidebar_category_header_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.sidebar-collapsed-icon-height` → `{primitive.size-22}` = 22px
    #[inline]
    pub fn sidebar_collapsed_icon_height(&self) -> LogicalPx {
        self.sidebar_collapsed_icon_height
    }

    /// `component.sidebar-collapsed-slot-width` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn sidebar_collapsed_slot_width(&self) -> LogicalPx {
        self.sidebar_collapsed_slot_width
    }

    /// `component.sidebar-collapsed-workspace-height` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn sidebar_collapsed_workspace_height(&self) -> LogicalPx {
        self.sidebar_collapsed_workspace_height
    }

    /// `component.sidebar-logo-collapsed-size` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn sidebar_logo_collapsed_size(&self) -> LogicalPx {
        self.sidebar_logo_collapsed_size
    }

    /// `component.sidebar-logo-size` → `{primitive.size-22}` = 22px
    #[inline]
    pub fn sidebar_logo_size(&self) -> LogicalPx {
        self.sidebar_logo_size
    }

    /// `component.sidebar-section-heading-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn sidebar_section_heading_font_size(&self) -> LogicalPx {
        self.sidebar_section_heading_font_size
    }

    /// `component.sidebar-wordmark-font-size` → `{semantic.font-size-brand-wordmark}` = 17px
    #[inline]
    pub fn sidebar_wordmark_font_size(&self) -> LogicalPx {
        self.sidebar_wordmark_font_size
    }

    /// `component.spinner-duration` → `{primitive.duration-900}` = 900ms
    #[inline]
    pub fn spinner_duration(&self) -> Millis {
        Millis(900.0)
    }

    /// `component.spinner-indicator` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn spinner_indicator(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.spinner-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn spinner_size(&self) -> LogicalPx {
        self.spinner_size
    }

    /// `component.spinner-track` → `{semantic.surface-active}`
    #[inline]
    pub fn spinner_track(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.split-sibling-min-height` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn split_sibling_min_height(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
    }

    /// `component.status-dot-agent` → `{semantic.accent-agent}`
    #[inline]
    pub fn status_dot_agent(&self) -> HexColor {
        self.accent_agent()
    }

    /// `component.status-dot-agent-active` → `{semantic.accent-agent-on-active}`
    #[inline]
    pub fn status_dot_agent_active(&self) -> HexColor {
        self.accent_agent_on_active()
    }

    /// `component.status-dot-attached-ring` → `{semantic.accent-attached-raised}`
    #[inline]
    pub fn status_dot_attached_ring(&self) -> HexColor {
        self.accent_attached_raised()
    }

    /// `component.status-dot-attached-ring-active` → `{semantic.accent-attached-on-active}`
    #[inline]
    pub fn status_dot_attached_ring_active(&self) -> HexColor {
        self.accent_attached_on_active()
    }

    /// `component.status-dot-attached-ring-offset` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn status_dot_attached_ring_offset(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.status-dot-attached-ring-width` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn status_dot_attached_ring_width(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.status-dot-completion` → `{semantic.attention-completion}`
    #[inline]
    pub fn status_dot_completion(&self) -> HexColor {
        self.attention_completion()
    }

    /// `component.status-dot-completion-active` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn status_dot_completion_active(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.status-dot-danger` → `{semantic.accent-danger}`
    #[inline]
    pub fn status_dot_danger(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.status-dot-danger-active` → `{semantic.accent-danger-on-active}`
    #[inline]
    pub fn status_dot_danger_active(&self) -> HexColor {
        self.accent_danger_on_active()
    }

    /// `component.status-dot-idle` → `{semantic.status-idle}`
    #[inline]
    pub fn status_dot_idle(&self) -> HexColor {
        self.status_idle()
    }

    /// `component.status-dot-idle-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn status_dot_idle_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.status-dot-needs-input` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn status_dot_needs_input(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.status-dot-needs-input-active` → `{semantic.accent-warning-on-active}`
    #[inline]
    pub fn status_dot_needs_input_active(&self) -> HexColor {
        self.accent_warning_on_active()
    }

    /// `component.status-dot-pulse-duration` → `{primitive.duration-1600}` = 1600ms
    #[inline]
    pub fn status_dot_pulse_duration(&self) -> Millis {
        Millis(1600.0)
    }

    /// `component.status-dot-ring` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn status_dot_ring(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.status-dot-ring-width` → `{primitive.size-1-5}` = 1.5px
    #[inline]
    pub fn status_dot_ring_width(&self) -> LogicalPx {
        self.status_dot_ring_width
    }

    /// `component.status-dot-size` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn status_dot_size(&self) -> LogicalPx {
        self.status_dot_size
    }

    /// `component.status-dot-size-compact` → `{primitive.size-6}` = 6px
    #[inline]
    pub fn status_dot_size_compact(&self) -> LogicalPx {
        LogicalPx((6.0 * self.ui_zoom).round())
    }

    /// `component.status-dot-success` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn status_dot_success(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.status-dot-success-active` → `{semantic.accent-success-on-active}`
    #[inline]
    pub fn status_dot_success_active(&self) -> HexColor {
        self.accent_success_on_active()
    }

    /// `component.status-dot-warning` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn status_dot_warning(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.status-dot-warning-active` → `{semantic.accent-warning-on-active}`
    #[inline]
    pub fn status_dot_warning_active(&self) -> HexColor {
        self.accent_warning_on_active()
    }

    /// `component.statusbar-dot-size` → `{component.status-dot-size-compact}` = 6px
    #[inline]
    pub fn statusbar_dot_size(&self) -> LogicalPx {
        self.status_dot_size_compact()
    }

    /// `component.statusbar-glyph` → `{semantic.glyph-dim}`
    #[inline]
    pub fn statusbar_glyph(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.statusbar-glyph-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn statusbar_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.statusbar-theme-glyph` → `{semantic.glyph-dim}`
    #[inline]
    pub fn statusbar_theme_glyph(&self) -> HexColor {
        self.glyph_dim()
    }

    /// `component.surface-focus-edge-focused` → `{semantic.text-secondary}`
    #[inline]
    pub fn surface_focus_edge_focused(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.surface-focus-edge-unfocused` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn surface_focus_edge_unfocused(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.surface-focus-edge-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn surface_focus_edge_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.surface-highlight-done-border` → `{semantic.attention-completion}`
    #[inline]
    pub fn surface_highlight_done_border(&self) -> HexColor {
        self.attention_completion()
    }

    /// `component.surface-highlight-done-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn surface_highlight_done_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.surface-highlight-input-border` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn surface_highlight_input_border(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.surface-highlight-input-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn surface_highlight_input_width(&self) -> LogicalPx {
        self.focus_ring_width
    }

    /// `component.surface-occupied-border-width` → `{semantic.border-width}` = 1px
    #[inline]
    pub fn surface_occupied_border_width(&self) -> LogicalPx {
        self.border_width
    }

    /// `component.surface-occupied-hard-border` → `{semantic.accent-attention-raised}`
    #[inline]
    pub fn surface_occupied_hard_border(&self) -> HexColor {
        self.accent_attention_raised()
    }

    /// `component.surface-occupied-soft-border` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn surface_occupied_soft_border(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.swatch-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn swatch_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.swatch-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn swatch_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.switch-overlay-active-bg` → `{semantic.accent-primary}`
    #[inline]
    pub fn switch_overlay_active_bg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.switch-overlay-active-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn switch_overlay_active_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.switch-overlay-bg` → `{component.kbd-bg}`
    #[inline]
    pub fn switch_overlay_bg(&self) -> HexColor {
        self.kbd_bg()
    }

    /// `component.switch-overlay-border` → `{component.kbd-border}`
    #[inline]
    pub fn switch_overlay_border(&self) -> HexColor {
        self.kbd_border()
    }

    /// `component.switch-overlay-fade` → `{semantic.motion-ui-fast}` = 90ms
    #[inline]
    pub fn switch_overlay_fade(&self) -> Millis {
        Millis(90.0)
    }

    /// `component.switch-overlay-fg` → `{component.kbd-fg}`
    #[inline]
    pub fn switch_overlay_fg(&self) -> HexColor {
        self.kbd_fg()
    }

    /// `component.switch-overlay-shadow-depth` → `{component.kbd-shadow-depth}` = 2px
    #[inline]
    pub fn switch_overlay_shadow_depth(&self) -> LogicalPx {
        self.kbd_shadow_depth()
    }

    /// `component.switch-overlay-size` → `{component.kbd-size}` = 16px
    #[inline]
    pub fn switch_overlay_size(&self) -> LogicalPx {
        self.kbd_size()
    }

    /// `component.switch-radius` → `{semantic.radius-pill}` = 9999px (sentinel — 완전 원형용 상한값)
    #[inline]
    pub fn switch_radius(&self) -> LogicalPx {
        LogicalPx((9999.0 * self.ui_zoom).round())
    }

    /// `component.switch-thumb-bg` → `{semantic.control-thumb}`
    #[inline]
    pub fn switch_thumb_bg(&self) -> HexColor {
        self.control_thumb()
    }

    /// `component.switch-thumb-bg-on` → `{semantic.text-on-accent}`
    #[inline]
    pub fn switch_thumb_bg_on(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.switch-thumb-inset` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn switch_thumb_inset(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.switch-thumb-raised-bg` → `{semantic.control-thumb-raised}`
    #[inline]
    pub fn switch_thumb_raised_bg(&self) -> HexColor {
        self.control_thumb_raised()
    }

    /// `component.switch-thumb-size` → `{primitive.size-12}` = 12px
    #[inline]
    pub fn switch_thumb_size(&self) -> LogicalPx {
        LogicalPx((12.0 * self.ui_zoom).round())
    }

    /// `component.switch-thumb-travel` → `{primitive.size-12}` = 12px
    #[inline]
    pub fn switch_thumb_travel(&self) -> LogicalPx {
        LogicalPx((12.0 * self.ui_zoom).round())
    }

    /// `component.switch-track-bg` → `{semantic.border-control}`
    #[inline]
    pub fn switch_track_bg(&self) -> HexColor {
        self.border_control()
    }

    /// `component.switch-track-bg-on` → `{semantic.accent-primary}`
    #[inline]
    pub fn switch_track_bg_on(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.switch-track-height` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn switch_track_height(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.switch-track-raised-bg` → `{semantic.border-control-raised}`
    #[inline]
    pub fn switch_track_raised_bg(&self) -> HexColor {
        self.border_control_raised()
    }

    /// `component.switch-track-width` → `{primitive.size-28}` = 28px
    #[inline]
    pub fn switch_track_width(&self) -> LogicalPx {
        LogicalPx((28.0 * self.ui_zoom).round())
    }

    /// `component.tab-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn tab_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.tab-bg-active` → `{semantic.bg-panel}`
    #[inline]
    pub fn tab_bg_active(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.tab-close-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn tab_close_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.tab-close-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn tab_close_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.tab-dot-size` → `{component.status-dot-size-compact}` = 6px
    #[inline]
    pub fn tab_dot_size(&self) -> LogicalPx {
        self.status_dot_size_compact()
    }

    /// `component.tab-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn tab_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.tab-fg-active` → `{semantic.text-primary}`
    #[inline]
    pub fn tab_fg_active(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.tab-fg-completion` → `{semantic.accent-primary-raised}`
    #[inline]
    pub fn tab_fg_completion(&self) -> HexColor {
        self.accent_primary_raised()
    }

    /// `component.tab-fg-hover` → `{semantic.text-secondary}`
    #[inline]
    pub fn tab_fg_hover(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.tab-fg-needs-input` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn tab_fg_needs_input(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.tab-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn tab_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tab-height` → `{semantic.control-height-tab}` = 24px
    #[inline]
    pub fn tab_height(&self) -> LogicalPx {
        self.item_height_tab
    }

    /// `component.tab-icon-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn tab_icon_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.tab-indicator` → `{semantic.accent-primary}`
    #[inline]
    pub fn tab_indicator(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.tab-indicator-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn tab_indicator_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.tab-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn tab_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tab-scroll-arrow-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn tab_scroll_arrow_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.tab-scroll-arrow-fg-disabled` → `{semantic.text-disabled}`
    #[inline]
    pub fn tab_scroll_arrow_fg_disabled(&self) -> HexColor {
        self.text_disabled()
    }

    /// `component.tab-scroll-arrow-glyph-size` → `{semantic.icon-size-sm}` = 14px
    #[inline]
    pub fn tab_scroll_arrow_glyph_size(&self) -> LogicalPx {
        self.icon_glyph_size_sm
    }

    /// `component.tab-scroll-arrow-hover-bg` → `{semantic.overlay-hover}`
    #[inline]
    pub fn tab_scroll_arrow_hover_bg(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.tab-scroll-arrow-move-fg` → `{component.move-source-glyph}`
    #[inline]
    pub fn tab_scroll_arrow_move_fg(&self) -> HexColor {
        self.move_source_glyph()
    }

    /// `component.tab-scroll-arrow-width` → `{semantic.control-height-tab}` = 24px
    #[inline]
    pub fn tab_scroll_arrow_width(&self) -> LogicalPx {
        self.item_height_tab
    }

    /// `component.tab-separator` → `{semantic.separator}`
    #[inline]
    pub fn tab_separator(&self) -> PremulColor {
        self.separator
    }

    /// `component.tab-status-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn tab_status_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.tab-strip-width` → `{semantic.tab-width}` = 150px
    #[inline]
    pub fn tab_strip_width(&self) -> LogicalPx {
        self.tab_width
    }

    /// `component.table-border` → `{semantic.separator}`
    #[inline]
    pub fn table_border(&self) -> PremulColor {
        self.separator
    }

    /// `component.table-cell-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn table_cell_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.table-cell-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn table_cell_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.table-cell-padding-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn table_cell_padding_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.table-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn table_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.table-header-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn table_header_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.table-header-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn table_header_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.table-header-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn table_header_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.table-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn table_row_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.table-row-bg-selected` → `{semantic.surface-active}`
    #[inline]
    pub fn table_row_bg_selected(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.table-row-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn table_row_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.table-row-fg-selected` → `{semantic.text-on-active}`
    #[inline]
    pub fn table_row_fg_selected(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.tag-attached-fg` → `{semantic.accent-attached-raised}`
    #[inline]
    pub fn tag_attached_fg(&self) -> HexColor {
        self.accent_attached_raised()
    }

    /// `component.tag-attention-fg` → `{semantic.accent-attention-raised}`
    #[inline]
    pub fn tag_attention_fg(&self) -> HexColor {
        self.accent_attention_raised()
    }

    /// `component.tag-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn tag_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.tag-border` → `{semantic.border-default}`
    #[inline]
    pub fn tag_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.tag-danger-fg` → `{semantic.accent-danger-raised}`
    #[inline]
    pub fn tag_danger_fg(&self) -> HexColor {
        self.accent_danger_raised()
    }

    /// `component.tag-disabled-bg` → `{semantic.state-disabled-fill}`
    #[inline]
    pub fn tag_disabled_bg(&self) -> HexColor {
        self.state_disabled_fill()
    }

    /// `component.tag-disabled-border` → `{semantic.state-disabled-border}`
    #[inline]
    pub fn tag_disabled_border(&self) -> HexColor {
        self.state_disabled_border()
    }

    /// `component.tag-disabled-fg` → `{semantic.state-disabled-fg}`
    #[inline]
    pub fn tag_disabled_fg(&self) -> HexColor {
        self.state_disabled_fg()
    }

    /// `component.tag-dot-size` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn tag_dot_size(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.tag-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn tag_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.tag-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn tag_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.tag-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn tag_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.tag-info-fg` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn tag_info_fg(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.tag-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn tag_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tag-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn tag_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.tag-size` → `{primitive.size-16}` = 16px
    #[inline]
    pub fn tag_size(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.tag-success-fg` → `{semantic.accent-success-ink}`
    #[inline]
    pub fn tag_success_fg(&self) -> HexColor {
        self.accent_success_ink()
    }

    /// `component.tag-warning-fg` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn tag_warning_fg(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.titlebar-button-active-bg` → `{semantic.overlay-active}`
    #[inline]
    pub fn titlebar_button_active_bg(&self) -> PremulColor {
        self.overlay_active()
    }

    /// `component.titlebar-button-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn titlebar_button_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.titlebar-button-fg-hover` → `{semantic.text-primary}`
    #[inline]
    pub fn titlebar_button_fg_hover(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.titlebar-button-hover-bg` → `{semantic.overlay-hover}`
    #[inline]
    pub fn titlebar_button_hover_bg(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.titlebar-caption-width` → `{primitive.size-46}` = 46px
    #[inline]
    pub fn titlebar_caption_width(&self) -> LogicalPx {
        self.caption_width
    }

    /// `component.titlebar-close-hover-bg` → `{semantic.accent-window-close}`
    #[inline]
    pub fn titlebar_close_hover_bg(&self) -> HexColor {
        self.accent_window_close()
    }

    /// `component.titlebar-close-hover-fg` → `{semantic.text-on-window-close}`
    #[inline]
    pub fn titlebar_close_hover_fg(&self) -> HexColor {
        self.text_on_window_close()
    }

    /// `component.titlebar-csd-border` → `{semantic.border-strong}`
    #[inline]
    pub fn titlebar_csd_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.titlebar-csd-radius` → `{primitive.radius-8}` = 8px
    #[inline]
    pub fn titlebar_csd_radius(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.titlebar-csd-shadow-margin` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn titlebar_csd_shadow_margin(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.titlebar-traffic-close` → `{semantic.accent-macos-close}`
    #[inline]
    pub fn titlebar_traffic_close(&self) -> HexColor {
        self.accent_macos_close()
    }

    /// `component.titlebar-traffic-inactive` → `{semantic.surface-active}`
    #[inline]
    pub fn titlebar_traffic_inactive(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.titlebar-traffic-min` → `{semantic.accent-macos-min}`
    #[inline]
    pub fn titlebar_traffic_min(&self) -> HexColor {
        self.accent_macos_min()
    }

    /// `component.titlebar-traffic-size` → `{primitive.size-12}` = 12px
    #[inline]
    pub fn titlebar_traffic_size(&self) -> LogicalPx {
        self.traffic_size
    }

    /// `component.titlebar-traffic-zoom` → `{semantic.accent-macos-zoom}`
    #[inline]
    pub fn titlebar_traffic_zoom(&self) -> HexColor {
        self.accent_macos_zoom()
    }

    /// `component.titlebar-window-button-size` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn titlebar_window_button_size(&self) -> LogicalPx {
        self.window_button_size
    }

    /// `component.toast-accent-agent` → `{semantic.accent-agent}`
    #[inline]
    pub fn toast_accent_agent(&self) -> HexColor {
        self.accent_agent()
    }

    /// `component.toast-accent-danger` → `{semantic.accent-danger}`
    #[inline]
    pub fn toast_accent_danger(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.toast-accent-info` → `{semantic.accent-info-raised}`
    #[inline]
    pub fn toast_accent_info(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `component.toast-accent-success` → `{semantic.accent-success-raised}`
    #[inline]
    pub fn toast_accent_success(&self) -> HexColor {
        self.accent_success_raised()
    }

    /// `component.toast-accent-warning` → `{semantic.accent-warning-raised}`
    #[inline]
    pub fn toast_accent_warning(&self) -> HexColor {
        self.accent_warning_raised()
    }

    /// `component.toast-accent-width` → `{semantic.tint-edge-width}` = 2px
    #[inline]
    pub fn toast_accent_width(&self) -> LogicalPx {
        self.toast_accent_width
    }

    /// `component.toast-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn toast_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.toast-border` → `{semantic.border-strong}`
    #[inline]
    pub fn toast_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.toast-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn toast_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.toast-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn toast_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.toast-hint-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn toast_hint_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.toast-max-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn toast_max_width(&self) -> LogicalPx {
        self.toast_max_width
    }

    /// `component.toast-min-height` → `{semantic.control-height}` = 28px
    #[inline]
    pub fn toast_min_height(&self) -> LogicalPx {
        self.item_height_interactive
    }

    /// `component.toast-padding-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn toast_padding_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.toast-padding-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn toast_padding_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.toast-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn toast_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.toast-stack-offset-bottom` → `{primitive.size-36}` = 36px
    #[inline]
    pub fn toast_stack_offset_bottom(&self) -> LogicalPx {
        LogicalPx((36.0 * self.ui_zoom).round())
    }

    /// `component.toast-stack-offset-bottom-settings` → `{primitive.size-64}` = 64px
    #[inline]
    pub fn toast_stack_offset_bottom_settings(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }

    /// `component.tools-menu-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn tools_menu_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.tools-menu-min-width` → `{primitive.size-160}` = 160px
    #[inline]
    pub fn tools_menu_min_width(&self) -> LogicalPx {
        LogicalPx((160.0 * self.ui_zoom).round())
    }

    /// `component.tooltip-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn tooltip_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.tooltip-border` → `{semantic.border-strong}`
    #[inline]
    pub fn tooltip_border(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.tooltip-delay` → `{semantic.motion-ui-med}` = 150ms
    #[inline]
    pub fn tooltip_delay(&self) -> Millis {
        Millis(150.0)
    }

    /// `component.tooltip-fg` → `{semantic.text-secondary-raised}`
    #[inline]
    pub fn tooltip_fg(&self) -> HexColor {
        self.text_secondary_raised()
    }

    /// `component.tooltip-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn tooltip_font_size(&self) -> LogicalPx {
        self.font_size_caption
    }

    /// `component.tooltip-max-width` → `{primitive.size-240}` = 240px
    #[inline]
    pub fn tooltip_max_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }

    /// `component.tooltip-offset` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn tooltip_offset(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.tooltip-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn tooltip_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tooltip-padding-y` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn tooltip_padding_y(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.tooltip-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn tooltip_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.transfer-body-gap` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn transfer_body_gap(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.transfer-footer-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn transfer_footer_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.transfer-header-pad-y` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn transfer_header_pad_y(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.transfer-pad-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn transfer_pad_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.transfer-popup-width` → `{primitive.size-400}` = 400px
    #[inline]
    pub fn transfer_popup_width(&self) -> LogicalPx {
        LogicalPx((400.0 * self.ui_zoom).round())
    }

    /// `component.transfer-well-pad-x` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn transfer_well_pad_x(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.transfer-well-pad-y` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn transfer_well_pad_y(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tree-row-bg-active` → `{semantic.surface-active}`
    #[inline]
    pub fn tree_row_bg_active(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.tree-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn tree_row_bg_hover(&self) -> PremulColor {
        self.overlay_hover()
    }

    /// `component.tree-row-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn tree_row_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.tree-row-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn tree_row_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.tree-row-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn tree_row_font_size(&self) -> LogicalPx {
        self.font_size_body
    }

    /// `component.tree-row-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn tree_row_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.tree-row-height` → `{semantic.control-height-tree}` = 22px
    #[inline]
    pub fn tree_row_height(&self) -> LogicalPx {
        self.item_height_tree
    }

    /// `component.tree-row-icon-fg-active` → `{semantic.accent-primary-on-active}`
    #[inline]
    pub fn tree_row_icon_fg_active(&self) -> HexColor {
        self.accent_primary_on_active()
    }

    /// `component.tree-row-indent` → `{semantic.space-md}` = 12px
    #[inline]
    pub fn tree_row_indent(&self) -> LogicalPx {
        self.spacing_md
    }

    /// `component.tree-row-meta-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn tree_row_meta_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.tree-row-part-fg-active` → `{semantic.text-on-active}`
    #[inline]
    pub fn tree_row_part_fg_active(&self) -> HexColor {
        self.text_on_active()
    }

    /// `component.trigger-menu-max-height` → `{component.autocomplete-max-height}` = 220px
    #[inline]
    pub fn trigger_menu_max_height(&self) -> LogicalPx {
        self.autocomplete_max_height()
    }

    /// `component.trigger-menu-min-width` → `{semantic.field-width-lg}` = 200px
    #[inline]
    pub fn trigger_menu_min_width(&self) -> LogicalPx {
        self.field_width_lg
    }

    /// `component.ui-code-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn ui_code_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.ui-code-bg-on-raised` → `{semantic.bg-panel}`
    #[inline]
    pub fn ui_code_bg_on_raised(&self) -> HexColor {
        self.bg_panel()
    }

    /// `component.ui-code-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn ui_code_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.ui-code-padding-x` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn ui_code_padding_x(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.ui-code-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn ui_code_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.webview-opaque-radius` → `{semantic.radius-none}` = 0px
    #[inline]
    pub fn webview_opaque_radius(&self) -> LogicalPx {
        LogicalPx((0.0 * self.ui_zoom).round())
    }

    /// `component.workspace-dot-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn workspace_dot_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.workspace-mirror-chip-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn workspace_mirror_chip_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.workspace-mirror-chip-border` → `{component.workspace-mirror-fg}`
    #[inline]
    pub fn workspace_mirror_chip_border(&self) -> HexColor {
        self.workspace_mirror_fg()
    }

    /// `component.workspace-mirror-chip-size` → `{primitive.size-12}` = 12px
    #[inline]
    pub fn workspace_mirror_chip_size(&self) -> LogicalPx {
        LogicalPx((12.0 * self.ui_zoom).round())
    }

    /// `component.workspace-mirror-fg` → `{semantic.accent-remote-raised}`
    #[inline]
    pub fn workspace_mirror_fg(&self) -> HexColor {
        self.accent_remote_raised()
    }

    /// `component.workspace-mirror-fg-active` → `{semantic.accent-remote-on-active}`
    #[inline]
    pub fn workspace_mirror_fg_active(&self) -> HexColor {
        self.accent_remote_on_active()
    }

    /// `component.workspace-mirror-gap` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn workspace_mirror_gap(&self) -> LogicalPx {
        self.spacing_xs
    }

    /// `component.workspace-mirror-icon-size` → `{semantic.icon-size-xs}` = 12px
    #[inline]
    pub fn workspace_mirror_icon_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }

    /// `component.workspace-row-active-bar-width` → `{semantic.selection-edge-width}` = 2px
    #[inline]
    pub fn workspace_row_active_bar_width(&self) -> LogicalPx {
        self.selection_edge_width
    }

    /// `component.workspace-row-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn workspace_row_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }
}
