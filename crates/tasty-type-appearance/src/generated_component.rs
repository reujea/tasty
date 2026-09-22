//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! Tier 3 (component) 치수·색·시간 접근자. `generated::component` 의
//! raw const 와 달리 **`&Theme` 경유** — 치수는 zoom-resolve 된 필드를
//! 반환하거나(semantic 종착) `ui_zoom` 을 직접 곱하고(primitive 직접
//! 종착), 색은 semantic 접근자 체인 또는 component→component 접근자
//! 상호 호출로 이어붙인다. 시간은 `Millis` 로 나가며 **zoom 을 곱하지
//! 않는다** — 배율은 길이 축이다.

use crate::color::HexColor;
use crate::motion::Millis;
use tasty_type_geometry::length::LogicalPx;

impl crate::theme::Theme {
    /// `component.autocomplete-empty-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn autocomplete_empty_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.autocomplete-match-fg` → `{semantic.accent-primary}`
    #[inline]
    pub fn autocomplete_match_fg(&self) -> HexColor {
        self.accent_primary()
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

    /// `component.autocomplete-row-bg-active` → `{semantic.surface-active}`
    #[inline]
    pub fn autocomplete_row_bg_active(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.autocomplete-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn autocomplete_row_bg_hover(&self) -> HexColor {
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

    /// `component.badge-dot-size` → `{component.status-dot-size}` = 8px
    #[inline]
    pub fn badge_dot_size(&self) -> LogicalPx {
        self.status_dot_size
    }

    /// `component.badge-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn badge_fg(&self) -> HexColor {
        self.text_secondary()
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

    /// `component.badge-neutral-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn badge_neutral_fg(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.badge-padding-x` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn badge_padding_x(&self) -> LogicalPx {
        self.spacing_xs
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

    /// `component.badge-success-bg` → `{semantic.accent-success}`
    #[inline]
    pub fn badge_success_bg(&self) -> HexColor {
        self.accent_success()
    }

    /// `component.badge-success-fg` → `{semantic.text-on-accent}`
    #[inline]
    pub fn badge_success_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// `component.banner-body-font-size` → `{semantic.font-size-caption}` = 11px
    #[inline]
    pub fn banner_body_font_size(&self) -> LogicalPx {
        self.font_size_caption
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

    /// `component.banner-margin` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn banner_margin(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.banner-more-app-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn banner_more_app_fg(&self) -> HexColor {
        self.text_primary()
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

    /// `component.banner-more-reserve` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn banner_more_reserve(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
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

    /// `component.banner-radius` → `{primitive.radius-8}` = 8px
    #[inline]
    pub fn banner_radius(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.banner-title-font-size` → `{semantic.font-size-body}` = 13px
    #[inline]
    pub fn banner_title_font_size(&self) -> LogicalPx {
        self.font_size_body
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

    /// `component.button-ghost-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn button_ghost_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.button-ghost-fg-hover` → `{semantic.text-primary}`
    #[inline]
    pub fn button_ghost_fg_hover(&self) -> HexColor {
        self.text_primary()
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
    pub fn button_overlay_active(&self) -> HexColor {
        self.overlay_active()
    }

    /// `component.button-overlay-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn button_overlay_hover(&self) -> HexColor {
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

    /// `component.checkbox-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn checkbox_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.checkbox-bg-checked` → `{semantic.accent-primary}`
    #[inline]
    pub fn checkbox_bg_checked(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.checkbox-border` → `{semantic.border-strong}`
    #[inline]
    pub fn checkbox_border(&self) -> HexColor {
        self.border_strong()
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

    /// `component.dag-cycle-fg` → `{semantic.accent-warning}`
    #[inline]
    pub fn dag_cycle_fg(&self) -> HexColor {
        self.accent_warning()
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

    /// `component.dag-edge-corner-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn dag_edge_corner_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.dag-edge-depends` → `{semantic.border-strong}`
    #[inline]
    pub fn dag_edge_depends(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.dag-edge-fallback` → `{semantic.accent-attention}`
    #[inline]
    pub fn dag_edge_fallback(&self) -> HexColor {
        self.accent_attention()
    }

    /// `component.dag-edge-highlight` → `{semantic.accent-primary}`
    #[inline]
    pub fn dag_edge_highlight(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.dag-edge-reduce` → `{semantic.accent-info}`
    #[inline]
    pub fn dag_edge_reduce(&self) -> HexColor {
        self.accent_info()
    }

    /// `component.dag-edge-width` → `{primitive.size-1}` = 1px
    #[inline]
    pub fn dag_edge_width(&self) -> LogicalPx {
        LogicalPx((1.0 * self.ui_zoom).round())
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

    /// `component.dag-minimap-node` → `{semantic.border-strong}`
    #[inline]
    pub fn dag_minimap_node(&self) -> HexColor {
        self.border_strong()
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
    pub fn dag_node_hover_bg(&self) -> HexColor {
        self.overlay_hover()
    }

    /// `component.dag-node-meta-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn dag_node_meta_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.dag-node-meta-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn dag_node_meta_font_size(&self) -> LogicalPx {
        self.font_size_micro
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

    /// `component.dag-runner-crashed-fg` → `{semantic.accent-danger}`
    #[inline]
    pub fn dag_runner_crashed_fg(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.dag-runner-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn dag_runner_fg(&self) -> HexColor {
        self.text_secondary()
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

    /// `component.dag-runner-idle-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn dag_runner_idle_fg(&self) -> HexColor {
        self.text_muted()
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

    /// `component.dag-runner-stalled-fg` → `{semantic.accent-warning}`
    #[inline]
    pub fn dag_runner_stalled_fg(&self) -> HexColor {
        self.accent_warning()
    }

    /// `component.dag-sibling-gap` → `{primitive.size-24}` = 24px
    #[inline]
    pub fn dag_sibling_gap(&self) -> LogicalPx {
        LogicalPx((24.0 * self.ui_zoom).round())
    }

    /// `component.dag-status-cancelled` → `{semantic.text-disabled}`
    #[inline]
    pub fn dag_status_cancelled(&self) -> HexColor {
        self.text_disabled()
    }

    /// `component.dag-status-cancelled-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_cancelled_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-cancelled-label` → `{semantic.text-secondary}`
    #[inline]
    pub fn dag_status_cancelled_label(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.dag-status-failed` → `{semantic.accent-danger}`
    #[inline]
    pub fn dag_status_failed(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.dag-status-failed-label` → `{component.dag-status-failed}`
    #[inline]
    pub fn dag_status_failed_label(&self) -> HexColor {
        self.dag_status_failed()
    }

    /// `component.dag-status-ready` → `{semantic.accent-info}`
    #[inline]
    pub fn dag_status_ready(&self) -> HexColor {
        self.accent_info()
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

    /// `component.dag-status-running` → `{semantic.accent-primary}`
    #[inline]
    pub fn dag_status_running(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.dag-status-running-label` → `{component.dag-status-running}`
    #[inline]
    pub fn dag_status_running_label(&self) -> HexColor {
        self.dag_status_running()
    }

    /// `component.dag-status-skipped` → `{semantic.text-disabled}`
    #[inline]
    pub fn dag_status_skipped(&self) -> HexColor {
        self.text_disabled()
    }

    /// `component.dag-status-skipped-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_skipped_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-skipped-label` → `{semantic.text-secondary}`
    #[inline]
    pub fn dag_status_skipped_label(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.dag-status-succeeded` → `{semantic.accent-success}`
    #[inline]
    pub fn dag_status_succeeded(&self) -> HexColor {
        self.accent_success()
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

    /// `component.dag-status-unknown` → `{semantic.accent-warning}`
    #[inline]
    pub fn dag_status_unknown(&self) -> HexColor {
        self.accent_warning()
    }

    /// `component.dag-status-unknown-label` → `{component.dag-status-unknown}`
    #[inline]
    pub fn dag_status_unknown_label(&self) -> HexColor {
        self.dag_status_unknown()
    }

    /// `component.dag-status-waiting` → `{semantic.status-idle}`
    #[inline]
    pub fn dag_status_waiting(&self) -> HexColor {
        self.status_idle()
    }

    /// `component.dag-status-waiting-bg` → `{component.dag-node-bg}`
    #[inline]
    pub fn dag_status_waiting_bg(&self) -> HexColor {
        self.dag_node_bg()
    }

    /// `component.dag-status-waiting-label` → `{semantic.text-muted}`
    #[inline]
    pub fn dag_status_waiting_label(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.drilldown-backbar-border` → `{semantic.separator}`
    #[inline]
    pub fn drilldown_backbar_border(&self) -> HexColor {
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

    /// `component.explorer-sidebar-width` → `{primitive.size-196}` = 196px
    #[inline]
    pub fn explorer_sidebar_width(&self) -> LogicalPx {
        LogicalPx((196.0 * self.ui_zoom).round())
    }

    /// `component.explorer-split-border` → `{semantic.separator}`
    #[inline]
    pub fn explorer_split_border(&self) -> HexColor {
        self.separator
    }

    /// `component.fh-when-width` → `{primitive.size-56}` = 56px
    #[inline]
    pub fn fh_when_width(&self) -> LogicalPx {
        LogicalPx((56.0 * self.ui_zoom).round())
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

    /// `component.fp-popup-min-width` → `{primitive.size-320}` = 320px
    #[inline]
    pub fn fp_popup_min_width(&self) -> LogicalPx {
        LogicalPx((320.0 * self.ui_zoom).round())
    }

    /// `component.git-toolbar-height` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn git_toolbar_height(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
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

    /// `component.icon-button-bg-active` → `{semantic.overlay-active}`
    #[inline]
    pub fn icon_button_bg_active(&self) -> HexColor {
        self.overlay_active()
    }

    /// `component.icon-button-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn icon_button_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.icon-button-fg-hover` → `{semantic.text-primary}`
    #[inline]
    pub fn icon_button_fg_hover(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.icon-button-overlay-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn icon_button_overlay_hover(&self) -> HexColor {
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

    /// `component.input-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn input_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.input-border` → `{semantic.border-default}`
    #[inline]
    pub fn input_border(&self) -> HexColor {
        self.border_default()
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

    /// `component.kbd-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn kbd_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.kbd-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn kbd_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.kbd-gap` → `{primitive.size-3}` = 3px
    #[inline]
    pub fn kbd_gap(&self) -> LogicalPx {
        LogicalPx((3.0 * self.ui_zoom).round())
    }

    /// `component.kbd-padding-x` → `{semantic.space-xs}` = 4px
    #[inline]
    pub fn kbd_padding_x(&self) -> LogicalPx {
        self.spacing_xs
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
    pub fn listctrl_divider(&self) -> HexColor {
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

    /// `component.listctrl-label-fg-active` → `{semantic.text-primary}`
    #[inline]
    pub fn listctrl_label_fg_active(&self) -> HexColor {
        self.text_primary()
    }

    /// `component.listctrl-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn listctrl_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.listctrl-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn listctrl_row_bg_hover(&self) -> HexColor {
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

    /// `component.listctrl-selected-bar` → `{semantic.accent-primary}`
    #[inline]
    pub fn listctrl_selected_bar(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.listctrl-selected-bar-width` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn listctrl_selected_bar_width(&self) -> LogicalPx {
        LogicalPx((2.0 * self.ui_zoom).round())
    }

    /// `component.md-code-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn md_code_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.md-code-border` → `{semantic.separator}`
    #[inline]
    pub fn md_code_border(&self) -> HexColor {
        self.separator
    }

    /// `component.md-doc-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn md_doc_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.md-quote-bar` → `{semantic.border-strong}`
    #[inline]
    pub fn md_quote_bar(&self) -> HexColor {
        self.border_strong()
    }

    /// `component.md-quote-fg` → `{semantic.text-muted}`
    #[inline]
    pub fn md_quote_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// `component.md-rule` → `{semantic.separator}`
    #[inline]
    pub fn md_rule(&self) -> HexColor {
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

    /// `component.md-table-row-bg-zebra` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn md_table_row_bg_zebra(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.menu-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn menu_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.menu-border` → `{semantic.border-default}`
    #[inline]
    pub fn menu_border(&self) -> HexColor {
        self.border_default()
    }

    /// `component.menu-item-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn menu_item_bg_hover(&self) -> HexColor {
        self.overlay_hover()
    }

    /// `component.menu-item-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn menu_item_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.menu-item-fg-hover` → `{semantic.text-primary}`
    #[inline]
    pub fn menu_item_fg_hover(&self) -> HexColor {
        self.text_primary()
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

    /// `component.menu-item-radius` → `{semantic.radius-sm}` = 2px
    #[inline]
    pub fn menu_item_radius(&self) -> LogicalPx {
        self.corner_radius_sm
    }

    /// `component.menu-item-shortcut-font-size` → `{semantic.font-size-micro}` = 10px
    #[inline]
    pub fn menu_item_shortcut_font_size(&self) -> LogicalPx {
        self.font_size_micro
    }

    /// `component.menu-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn menu_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.modhint-fade` → `{semantic.motion-ui-fade}` = 200ms
    #[inline]
    pub fn modhint_fade(&self) -> Millis {
        Millis(200.0)
    }

    /// `component.modhint-grip-fg` → `{semantic.border-strong}`
    #[inline]
    pub fn modhint_grip_fg(&self) -> HexColor {
        self.border_strong()
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

    /// `component.multiselect-all-fg` → `{semantic.accent-primary}`
    #[inline]
    pub fn multiselect_all_fg(&self) -> HexColor {
        self.accent_primary()
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

    /// `component.multiselect-border-hover` → `{semantic.border-strong}`
    #[inline]
    pub fn multiselect_border_hover(&self) -> HexColor {
        self.border_strong()
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
    pub fn multiselect_row_bg_hover(&self) -> HexColor {
        self.overlay_hover()
    }

    /// `component.multiselect-row-fg` → `{semantic.text-primary}`
    #[inline]
    pub fn multiselect_row_fg(&self) -> HexColor {
        self.text_primary()
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
    pub fn multiselect_separator(&self) -> HexColor {
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

    /// `component.plugin-avatar-border-width` → `{semantic.border-width}` = 1px
    #[inline]
    pub fn plugin_avatar_border_width(&self) -> LogicalPx {
        self.border_width
    }

    /// `component.plugin-avatar-fg` → `{semantic.accent-primary}`
    #[inline]
    pub fn plugin_avatar_fg(&self) -> HexColor {
        self.accent_primary()
    }

    /// `component.plugin-avatar-initial-font-size-lg` → `{primitive.font-size-16}` = 16px
    #[inline]
    pub fn plugin_avatar_initial_font_size_lg(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }

    /// `component.plugin-avatar-initial-font-size-sm` → `{semantic.font-size-max}` = 14px
    #[inline]
    pub fn plugin_avatar_initial_font_size_sm(&self) -> LogicalPx {
        self.font_size_max
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

    /// `component.port-favorites-bg` → `{semantic.bg-sidebar}`
    #[inline]
    pub fn port_favorites_bg(&self) -> HexColor {
        self.bg_sidebar()
    }

    /// `component.port-favorites-border` → `{semantic.separator}`
    #[inline]
    pub fn port_favorites_border(&self) -> HexColor {
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

    /// `component.port-star-on` → `{semantic.accent-warning}`
    #[inline]
    pub fn port_star_on(&self) -> HexColor {
        self.accent_warning()
    }

    /// `component.port-state-none-dot` → `{component.status-dot-idle}`
    #[inline]
    pub fn port_state_none_dot(&self) -> HexColor {
        self.status_dot_idle()
    }

    /// `component.preset-cfg-draft-fg` → `{semantic.accent-warning}`
    #[inline]
    pub fn preset_cfg_draft_fg(&self) -> HexColor {
        self.accent_warning()
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

    /// `component.preset-cfg-footer-padding-x` → `{primitive.size-14}` = 14px
    #[inline]
    pub fn preset_cfg_footer_padding_x(&self) -> LogicalPx {
        LogicalPx((14.0 * self.ui_zoom).round())
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

    /// `component.select-bg` → `{semantic.surface-raised}`
    #[inline]
    pub fn select_bg(&self) -> HexColor {
        self.surface_raised()
    }

    /// `component.select-border` → `{semantic.border-default}`
    #[inline]
    pub fn select_border(&self) -> HexColor {
        self.border_default()
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

    /// `component.select-radius` → `{semantic.radius}` = 4px
    #[inline]
    pub fn select_radius(&self) -> LogicalPx {
        self.corner_radius
    }

    /// `component.settings-row-min-height` → `{primitive.size-32}` = 32px
    #[inline]
    pub fn settings_row_min_height(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }

    /// `component.settings-sidebar-width` → `{primitive.size-200}` = 200px
    #[inline]
    pub fn settings_sidebar_width(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
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
    pub fn sidebar_category_header_border(&self) -> HexColor {
        self.separator
    }

    /// `component.sidebar-category-header-count-fg` → `{semantic.text-disabled}`
    #[inline]
    pub fn sidebar_category_header_count_fg(&self) -> HexColor {
        self.text_disabled()
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

    /// `component.spinner-indicator` → `{semantic.accent-primary}`
    #[inline]
    pub fn spinner_indicator(&self) -> HexColor {
        self.accent_primary()
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

    /// `component.status-dot-agent` → `{semantic.accent-agent}`
    #[inline]
    pub fn status_dot_agent(&self) -> HexColor {
        self.accent_agent()
    }

    /// `component.status-dot-attached-ring` → `{semantic.accent-attached}`
    #[inline]
    pub fn status_dot_attached_ring(&self) -> HexColor {
        self.border_attached()
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

    /// `component.status-dot-danger` → `{semantic.accent-danger}`
    #[inline]
    pub fn status_dot_danger(&self) -> HexColor {
        self.accent_danger()
    }

    /// `component.status-dot-idle` → `{semantic.status-idle}`
    #[inline]
    pub fn status_dot_idle(&self) -> HexColor {
        self.status_idle()
    }

    /// `component.status-dot-pulse-duration` → `{primitive.duration-1600}` = 1600ms
    #[inline]
    pub fn status_dot_pulse_duration(&self) -> Millis {
        Millis(1600.0)
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

    /// `component.status-dot-success` → `{semantic.accent-success}`
    #[inline]
    pub fn status_dot_success(&self) -> HexColor {
        self.accent_success()
    }

    /// `component.status-dot-warning` → `{semantic.accent-warning}`
    #[inline]
    pub fn status_dot_warning(&self) -> HexColor {
        self.accent_warning()
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

    /// `component.surface-highlight-done-width` → `{semantic.focus-ring-width}` = 2px
    #[inline]
    pub fn surface_highlight_done_width(&self) -> LogicalPx {
        self.focus_ring_width
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

    /// `component.surface-occupied-hard-border` → `{semantic.accent-occupied-hard}`
    #[inline]
    pub fn surface_occupied_hard_border(&self) -> HexColor {
        self.accent_occupied_hard()
    }

    /// `component.surface-occupied-soft-border` → `{semantic.accent-occupied-soft}`
    #[inline]
    pub fn surface_occupied_soft_border(&self) -> HexColor {
        self.accent_occupied_soft()
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

    /// `component.switch-thumb-bg` → `{semantic.text-muted}`
    #[inline]
    pub fn switch_thumb_bg(&self) -> HexColor {
        self.text_muted()
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

    /// `component.switch-track-bg` → `{semantic.surface-active}`
    #[inline]
    pub fn switch_track_bg(&self) -> HexColor {
        self.surface_active()
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

    /// `component.tab-fg-hover` → `{semantic.text-secondary}`
    #[inline]
    pub fn tab_fg_hover(&self) -> HexColor {
        self.text_secondary()
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

    /// `component.tab-indicator-width` → `{primitive.size-2}` = 2px
    #[inline]
    pub fn tab_indicator_width(&self) -> LogicalPx {
        self.tab_indicator_width
    }

    /// `component.tab-padding-x` → `{semantic.space-sm}` = 8px
    #[inline]
    pub fn tab_padding_x(&self) -> LogicalPx {
        self.spacing_sm
    }

    /// `component.tab-separator` → `{semantic.separator}`
    #[inline]
    pub fn tab_separator(&self) -> HexColor {
        self.separator
    }

    /// `component.tab-strip-width` → `{semantic.tab-width}` = 150px
    #[inline]
    pub fn tab_strip_width(&self) -> LogicalPx {
        self.tab_width
    }

    /// `component.table-border` → `{semantic.separator}`
    #[inline]
    pub fn table_border(&self) -> HexColor {
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
    pub fn table_row_bg_hover(&self) -> HexColor {
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

    /// `component.tag-dot-size` → `{primitive.size-8}` = 8px
    #[inline]
    pub fn tag_dot_size(&self) -> LogicalPx {
        LogicalPx((8.0 * self.ui_zoom).round())
    }

    /// `component.tag-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn tag_fg(&self) -> HexColor {
        self.text_secondary()
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

    /// `component.titlebar-button-active-bg` → `{semantic.overlay-active}`
    #[inline]
    pub fn titlebar_button_active_bg(&self) -> HexColor {
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
    pub fn titlebar_button_hover_bg(&self) -> HexColor {
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

    /// `component.toast-accent-info` → `{semantic.accent-info}`
    #[inline]
    pub fn toast_accent_info(&self) -> HexColor {
        self.accent_info()
    }

    /// `component.toast-accent-success` → `{semantic.accent-success}`
    #[inline]
    pub fn toast_accent_success(&self) -> HexColor {
        self.accent_success()
    }

    /// `component.toast-accent-warning` → `{semantic.accent-warning}`
    #[inline]
    pub fn toast_accent_warning(&self) -> HexColor {
        self.accent_warning()
    }

    /// `component.toast-accent-width` → `{primitive.size-3}` = 3px
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

    /// `component.tooltip-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn tooltip_fg(&self) -> HexColor {
        self.text_secondary()
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

    /// `component.transfer-popup-width` → `{primitive.size-400}` = 400px
    #[inline]
    pub fn transfer_popup_width(&self) -> LogicalPx {
        LogicalPx((400.0 * self.ui_zoom).round())
    }

    /// `component.tree-row-bg-active` → `{semantic.surface-active}`
    #[inline]
    pub fn tree_row_bg_active(&self) -> HexColor {
        self.surface_active()
    }

    /// `component.tree-row-bg-hover` → `{semantic.overlay-hover}`
    #[inline]
    pub fn tree_row_bg_hover(&self) -> HexColor {
        self.overlay_hover()
    }

    /// `component.tree-row-fg` → `{semantic.text-secondary}`
    #[inline]
    pub fn tree_row_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// `component.tree-row-fg-active` → `{semantic.text-primary}`
    #[inline]
    pub fn tree_row_fg_active(&self) -> HexColor {
        self.text_primary()
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

    /// `component.workspace-mirror-fg` → `{semantic.accent-remote}`
    #[inline]
    pub fn workspace_mirror_fg(&self) -> HexColor {
        self.accent_remote()
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
}
