//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! Tier 3 — component 치수 (테마 불변), 컴포넌트별 하위 모듈. semantic (일부는
//! primitive 직접 — 디자인 실물의 tier-skip alias) 참조로 정의된다.
//! 색 component 접근자는 시리즈 04 에서 결정.
//!
//! **zoom 주의**: 런타임 소비는 반드시 `&Theme` 경유 — `semantic.rs` 참조.

pub mod autocomplete {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.autocomplete-max-height` → `{primitive.size-220}` = 220px
    pub const MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_220;

    /// `component.autocomplete-menu-radius` → `{component.menu-radius}` = 4px
    pub const MENU_RADIUS: LogicalPx = super::menu::RADIUS;

    /// `component.autocomplete-row-height` → `{component.menu-item-height}` = 28px
    pub const ROW_HEIGHT: LogicalPx = super::menu::ITEM_HEIGHT;

    /// `component.autocomplete-row-padding-x` → `{component.menu-item-padding-x}` = 12px
    pub const ROW_PADDING_X: LogicalPx = super::menu::ITEM_PADDING_X;
}

pub mod badge {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.badge-dot-size` → `{component.status-dot-size}` = 8px
    pub const DOT_SIZE: LogicalPx = super::status_dot::SIZE;

    /// `component.badge-font-size` → `{semantic.font-size-micro}` = 10px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.badge-font-weight` → `{semantic.font-weight-bold}` = 700
    pub const FONT_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_BOLD;

    /// `component.badge-group-gap` → `{semantic.space-xs}` = 4px
    pub const GROUP_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.badge-padding-x` → `{semantic.space-xs}` = 4px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.badge-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.badge-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod banner {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.banner-body-font-size` → `{semantic.font-size-caption}` = 11px
    pub const BODY_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.banner-countdown-font-size` → `{semantic.font-size-micro}` = 10px
    pub const COUNTDOWN_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.banner-fade` → `{semantic.motion-ui}` = 120ms (ms)
    pub const FADE: f32 = crate::generated::semantic::MOTION_UI;

    /// `component.banner-gap` → `{semantic.space-md}` = 12px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.banner-margin` → `{semantic.space-sm}` = 8px
    pub const MARGIN: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.banner-more-column-gap` → `{semantic.space-xs}` = 4px
    pub const MORE_COLUMN_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.banner-more-menu-max-width` → `{primitive.size-288}` = 288px
    pub const MORE_MENU_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_288;

    /// `component.banner-more-menu-min-width` → `{primitive.size-200}` = 200px
    pub const MORE_MENU_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.banner-more-menu-offset` → `{semantic.space-xs}` = 4px
    pub const MORE_MENU_OFFSET: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.banner-more-menu-padding` → `{semantic.space-xs}` = 4px
    pub const MORE_MENU_PADDING: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.banner-more-menu-radius` → `{component.menu-radius}` = 4px
    pub const MORE_MENU_RADIUS: LogicalPx = super::menu::RADIUS;

    /// `component.banner-more-reserve` → `{primitive.size-56}` = 56px
    pub const MORE_RESERVE: LogicalPx = crate::generated::primitive::SIZE_56;

    /// `component.banner-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.banner-padding-y` → `{semantic.space-sm}` = 8px
    pub const PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.banner-radius` → `{primitive.radius-8}` = 8px
    pub const RADIUS: LogicalPx = crate::generated::primitive::RADIUS_8;

    /// `component.banner-recessed-opacity` → `{primitive.opacity-recessed}` = 0.4
    pub const RECESSED_OPACITY: f32 = crate::generated::primitive::OPACITY_RECESSED;

    /// `component.banner-title-font-size` → `{semantic.font-size-body}` = 13px
    pub const TITLE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;
}

pub mod button {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.button-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.button-font-weight` → `{semantic.font-weight-medium}` = 500
    pub const FONT_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_MEDIUM;

    /// `component.button-gap` → `{semantic.space-sm}` = 8px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.button-height` → `{semantic.control-height}` = 28px
    pub const HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.button-height-lg` → `{primitive.size-32}` = 32px
    pub const HEIGHT_LG: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.button-height-sm` → `{semantic.control-height-tab}` = 24px
    pub const HEIGHT_SM: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TAB;

    /// `component.button-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.button-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod checkbox {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.checkbox-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.checkbox-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod dag {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.dag-canvas-dot-gap` → `{primitive.size-16}` = 16px
    pub const CANVAS_DOT_GAP: LogicalPx = crate::generated::primitive::SIZE_16;

    /// `component.dag-canvas-dot-size` → `{primitive.size-1}` = 1px
    pub const CANVAS_DOT_SIZE: LogicalPx = crate::generated::primitive::SIZE_1;

    /// `component.dag-canvas-padding` → `{semantic.space-lg}` = 16px
    pub const CANVAS_PADDING: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.dag-chrome-height` → `{semantic.control-height}` = 28px
    pub const CHROME_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.dag-chrome-inset` → `{semantic.space-sm}` = 8px
    pub const CHROME_INSET: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-cycle-height` → `{semantic.control-height}` = 28px
    pub const CYCLE_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.dag-detail-log-max-height` → `{primitive.size-160}` = 160px
    pub const DETAIL_LOG_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_160;

    /// `component.dag-detail-out-max-height` → `{primitive.size-112}` = 112px
    pub const DETAIL_OUT_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_112;

    /// `component.dag-detail-padding` → `{semantic.space-md}` = 12px
    pub const DETAIL_PADDING: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.dag-detail-sheet-height` → `{primitive.size-220}` = 220px
    pub const DETAIL_SHEET_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_220;

    /// `component.dag-detail-width` → `{primitive.size-288}` = 288px
    pub const DETAIL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_288;

    /// `component.dag-edge-arrow-size` → `{primitive.size-8}` = 8px
    pub const EDGE_ARROW_SIZE: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.dag-edge-corner-radius` → `{semantic.radius}` = 4px
    pub const EDGE_CORNER_RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.dag-edge-dim-opacity` → `{primitive.opacity-recessed}` = 0.4
    pub const EDGE_DIM_OPACITY: f32 = crate::generated::primitive::OPACITY_RECESSED;

    /// `component.dag-edge-width` → `{primitive.size-1}` = 1px
    pub const EDGE_WIDTH: LogicalPx = crate::generated::primitive::SIZE_1;

    /// `component.dag-layer-gap` → `{primitive.size-32}` = 32px
    pub const LAYER_GAP: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.dag-minimap-height` → `{primitive.size-112}` = 112px
    pub const MINIMAP_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_112;

    /// `component.dag-minimap-min-surface` → `{primitive.size-560}` = 560px
    pub const MINIMAP_MIN_SURFACE: LogicalPx = crate::generated::primitive::SIZE_560;

    /// `component.dag-minimap-width` → `{primitive.size-160}` = 160px
    pub const MINIMAP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_160;

    /// `component.dag-node-bar-width` → `{primitive.size-3}` = 3px
    pub const NODE_BAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_3;

    /// `component.dag-node-dim-opacity` → `{primitive.opacity-dimmed}` = 0.75
    pub const NODE_DIM_OPACITY: f32 = crate::generated::primitive::OPACITY_DIMMED;

    /// `component.dag-node-gap` → `{semantic.space-sm}` = 8px
    pub const NODE_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-node-height` → `{primitive.size-48}` = 48px
    pub const NODE_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_48;

    /// `component.dag-node-meta-font-size` → `{semantic.font-size-micro}` = 10px
    pub const NODE_META_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.dag-node-name-font-size` → `{semantic.font-size-body}` = 13px
    pub const NODE_NAME_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.dag-node-padding-x` → `{semantic.space-sm}` = 8px
    pub const NODE_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-node-padding-y` → `{semantic.space-xs}` = 4px
    pub const NODE_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.dag-node-radius` → `{semantic.radius}` = 4px
    pub const NODE_RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.dag-node-row-gap` → `{semantic.space-xs}` = 4px
    pub const NODE_ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.dag-node-selected-ring-width` → `{semantic.focus-ring-width}` = 2px
    pub const NODE_SELECTED_RING_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

    /// `component.dag-node-width` → `{primitive.size-168}` = 168px
    pub const NODE_WIDTH: LogicalPx = crate::generated::primitive::SIZE_168;

    /// `component.dag-popup-height` → `{primitive.size-460}` = 460px
    pub const POPUP_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_460;

    /// `component.dag-popup-width` → `{primitive.size-560}` = 560px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_560;

    /// `component.dag-row-count-font-size` → `{semantic.font-size-caption}` = 11px
    pub const ROW_COUNT_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.dag-row-height` → `{component.listctrl-row-min-height}` = 36px
    pub const ROW_HEIGHT: LogicalPx = super::listctrl::ROW_MIN_HEIGHT;

    /// `component.dag-row-summary-gap` → `{semantic.space-sm}` = 8px
    pub const ROW_SUMMARY_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-runner-gap` → `{semantic.space-sm}` = 8px
    pub const RUNNER_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-runner-height` → `{semantic.control-height-tree}` = 22px
    pub const RUNNER_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TREE;

    /// `component.dag-runner-padding-x` → `{semantic.space-sm}` = 8px
    pub const RUNNER_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.dag-runner-radius` → `{semantic.radius-sm}` = 2px
    pub const RUNNER_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.dag-sibling-gap` → `{primitive.size-24}` = 24px
    pub const SIBLING_GAP: LogicalPx = crate::generated::primitive::SIZE_24;
}

pub mod drilldown {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.drilldown-backbar-gap` → `{semantic.space-sm}` = 8px
    pub const BACKBAR_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.drilldown-backbar-height` → `{primitive.size-36}` = 36px
    pub const BACKBAR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_36;

    /// `component.drilldown-backbar-padding-x` → `{semantic.space-sm}` = 8px
    pub const BACKBAR_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.drilldown-backbar-padding-y` → `{semantic.space-xs}` = 4px
    pub const BACKBAR_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.drilldown-title-font-size` → `{semantic.font-size-body}` = 13px
    pub const TITLE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.drilldown-title-font-weight` → `{semantic.font-weight-semibold}` = 600
    pub const TITLE_FONT_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_SEMIBOLD;
}

pub mod explorer {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.explorer-favorites-pin-height` → `{primitive.size-240}` = 240px
    pub const FAVORITES_PIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-favorites-pin-min-height` → `{primitive.size-120}` = 120px
    pub const FAVORITES_PIN_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_120;

    /// `component.explorer-favorites-pin-ratio` = 0.4
    pub const FAVORITES_PIN_RATIO: f32 = 0.4;

    /// `component.explorer-favorites-pin-threshold` → `{primitive.size-600}` = 600px
    pub const FAVORITES_PIN_THRESHOLD: LogicalPx = crate::generated::primitive::SIZE_600;

    /// `component.explorer-sidebar-width` → `{primitive.size-196}` = 196px
    pub const SIDEBAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_196;
}

pub mod fh {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.fh-when-width` → `{primitive.size-56}` = 56px
    pub const WHEN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_56;
}

pub mod fp {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.fp-bar-hysteresis` → `{primitive.size-8}` = 8px
    pub const BAR_HYSTERESIS: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.fp-crumb-current-min-width` → `{primitive.size-96}` = 96px
    pub const CRUMB_CURRENT_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_96;

    /// `component.fp-crumb-max-width` → `{primitive.size-180}` = 180px
    pub const CRUMB_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_180;

    /// `component.fp-crumb-menu-max-width` → `{primitive.size-320}` = 320px
    pub const CRUMB_MENU_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;

    /// `component.fp-crumb-menu-min-width` → `{primitive.size-180}` = 180px
    pub const CRUMB_MENU_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_180;

    /// `component.fp-crumb-min-width` → `{primitive.size-64}` = 64px
    pub const CRUMB_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_64;

    /// `component.fp-popup-min-width` → `{primitive.size-320}` = 320px
    pub const POPUP_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;
}

pub mod git {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.git-toolbar-height` → `{primitive.size-32}` = 32px
    pub const TOOLBAR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_32;
}

pub mod help_hint {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.help-hint-gap` → `{semantic.space-xs}` = 4px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.help-hint-size` → `{semantic.icon-size-sm}` = 14px
    pub const SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;
}

pub mod icon_button {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.icon-button-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.icon-button-size` → `{semantic.control-height}` = 28px
    pub const SIZE: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.icon-button-size-sm` → `{primitive.size-24}` = 24px
    pub const SIZE_SM: LogicalPx = crate::generated::primitive::SIZE_24;
}

pub mod input {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.input-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.input-gap` → `{semantic.space-sm}` = 8px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.input-height` → `{semantic.control-height}` = 28px
    pub const HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.input-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.input-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod kbd {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.kbd-font-size` → `{semantic.font-size-micro}` = 10px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.kbd-gap` → `{primitive.size-3}` = 3px
    pub const GAP: LogicalPx = crate::generated::primitive::SIZE_3;

    /// `component.kbd-padding-x` → `{semantic.space-xs}` = 4px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.kbd-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.kbd-shadow-depth` → `{primitive.size-2}` = 2px
    pub const SHADOW_DEPTH: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.kbd-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod listctrl {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.listctrl-desc-font-size` → `{semantic.font-size-caption}` = 11px
    pub const DESC_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.listctrl-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.listctrl-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.listctrl-row-gap` → `{semantic.space-sm}` = 8px
    pub const ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.listctrl-row-min-height` → `{primitive.size-36}` = 36px
    pub const ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_36;

    /// `component.listctrl-row-padding-x` → `{semantic.space-md}` = 12px
    pub const ROW_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.listctrl-row-padding-y` → `{semantic.space-sm}` = 8px
    pub const ROW_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.listctrl-selected-bar-width` → `{primitive.size-2}` = 2px
    pub const SELECTED_BAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_2;
}

pub mod md {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.md-table-cell-padding-x` → `{semantic.space-sm}` = 8px
    pub const TABLE_CELL_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.md-table-cell-padding-y` → `{semantic.space-xs}` = 4px
    pub const TABLE_CELL_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;
}

pub mod menu {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.menu-item-height` → `{semantic.control-height}` = 28px
    pub const ITEM_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.menu-item-padding-x` → `{semantic.space-md}` = 12px
    pub const ITEM_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.menu-item-radius` → `{semantic.radius-sm}` = 2px
    pub const ITEM_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.menu-item-shortcut-font-size` → `{semantic.font-size-micro}` = 10px
    pub const ITEM_SHORTCUT_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.menu-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod modhint {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.modhint-fade` → `{semantic.motion-ui-fade}` = 200ms (ms)
    pub const FADE: f32 = crate::generated::semantic::MOTION_UI_FADE;

    /// `component.modhint-header-height` → `{primitive.size-28}` = 28px
    pub const HEADER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_28;

    /// `component.modhint-height` → `{primitive.size-400}` = 400px
    pub const HEIGHT: LogicalPx = crate::generated::primitive::SIZE_400;

    /// `component.modhint-hold-delay` → `{semantic.motion-hold-reveal}` = 500ms (ms)
    pub const HOLD_DELAY: f32 = crate::generated::semantic::MOTION_HOLD_REVEAL;

    /// `component.modhint-min-height` → `{primitive.size-240}` = 240px
    pub const MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.modhint-min-width` → `{primitive.size-180}` = 180px
    pub const MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_180;

    /// `component.modhint-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.modhint-section-gap` → `{semantic.space-md}` = 12px
    pub const SECTION_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.modhint-width` → `{primitive.size-180}` = 180px
    pub const WIDTH: LogicalPx = crate::generated::primitive::SIZE_180;
}

pub mod multiselect {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.multiselect-chevron-offset` → `{component.select-chevron-offset}` = 8px
    pub const CHEVRON_OFFSET: LogicalPx = super::select::CHEVRON_OFFSET;

    /// `component.multiselect-chevron-room` → `{component.select-chevron-room}` = 28px
    pub const CHEVRON_ROOM: LogicalPx = super::select::CHEVRON_ROOM;

    /// `component.multiselect-font-size` → `{component.select-font-size}` = 13px
    pub const FONT_SIZE: LogicalPx = super::select::FONT_SIZE;

    /// `component.multiselect-height` → `{component.select-height}` = 28px
    pub const HEIGHT: LogicalPx = super::select::HEIGHT;

    /// `component.multiselect-menu-gap` → `{semantic.space-xs}` = 4px
    pub const MENU_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.multiselect-menu-max-height` → `{component.autocomplete-max-height}` = 220px
    pub const MENU_MAX_HEIGHT: LogicalPx = super::autocomplete::MAX_HEIGHT;

    /// `component.multiselect-menu-max-width` → `{primitive.size-320}` = 320px
    pub const MENU_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;

    /// `component.multiselect-menu-padding` → `{semantic.space-xs}` = 4px
    pub const MENU_PADDING: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.multiselect-menu-radius` → `{component.menu-radius}` = 4px
    pub const MENU_RADIUS: LogicalPx = super::menu::RADIUS;

    /// `component.multiselect-padding-x` → `{component.select-padding-x}` = 12px
    pub const PADDING_X: LogicalPx = super::select::PADDING_X;

    /// `component.multiselect-radius` → `{component.select-radius}` = 4px
    pub const RADIUS: LogicalPx = super::select::RADIUS;

    /// `component.multiselect-row-gap` → `{semantic.space-sm}` = 8px
    pub const ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.multiselect-row-height` → `{component.menu-item-height}` = 28px
    pub const ROW_HEIGHT: LogicalPx = super::menu::ITEM_HEIGHT;

    /// `component.multiselect-row-padding-x` → `{component.menu-item-padding-x}` = 12px
    pub const ROW_PADDING_X: LogicalPx = super::menu::ITEM_PADDING_X;
}

pub mod plugin {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.plugin-avatar-border-width` → `{semantic.border-width}` = 1px
    pub const AVATAR_BORDER_WIDTH: LogicalPx = crate::generated::semantic::BORDER_WIDTH;

    /// `component.plugin-avatar-initial-font-size-lg` → `{primitive.font-size-16}` = 16px
    pub const AVATAR_INITIAL_FONT_SIZE_LG: LogicalPx = crate::generated::primitive::FONT_SIZE_16;

    /// `component.plugin-avatar-initial-font-size-sm` → `{semantic.font-size-max}` = 14px
    pub const AVATAR_INITIAL_FONT_SIZE_SM: LogicalPx = crate::generated::semantic::FONT_SIZE_MAX;

    /// `component.plugin-avatar-initial-weight` → `{semantic.font-weight-bold}` = 700
    pub const AVATAR_INITIAL_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_BOLD;

    /// `component.plugin-avatar-radius` → `{semantic.radius}` = 4px
    pub const AVATAR_RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.plugin-avatar-size-lg` → `{primitive.size-46}` = 46px
    pub const AVATAR_SIZE_LG: LogicalPx = crate::generated::primitive::SIZE_46;

    /// `component.plugin-avatar-size-sm` → `{primitive.size-32}` = 32px
    pub const AVATAR_SIZE_SM: LogicalPx = crate::generated::primitive::SIZE_32;
}

pub mod plugins_list {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.plugins-list-width` → `{primitive.size-288}` = 288px
    pub const WIDTH: LogicalPx = crate::generated::primitive::SIZE_288;
}

pub mod port {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.port-favorites-max-height` → `{primitive.size-112}` = 112px
    pub const FAVORITES_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_112;

    /// `component.port-favorites-row-height` → `{semantic.control-height-tree}` = 22px
    pub const FAVORITES_ROW_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TREE;

    /// `component.port-process-col-min-width` → `{primitive.size-200}` = 200px
    pub const PROCESS_COL_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.port-star-col-width` → `{primitive.size-28}` = 28px
    pub const STAR_COL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_28;
}

pub mod preset {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.preset-cfg-dim-opacity` → `{semantic.state-disabled-opacity}` = 0.5
    pub const CFG_DIM_OPACITY: f32 = crate::generated::semantic::STATE_DISABLED_OPACITY;

    /// `component.preset-cfg-field-gap` → `{semantic.space-md}` = 12px
    pub const CFG_FIELD_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.preset-cfg-footer-height` → `{primitive.size-52}` = 52px
    pub const CFG_FOOTER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_52;

    /// `component.preset-cfg-footer-padding-x` → `{primitive.size-14}` = 14px
    pub const CFG_FOOTER_PADDING_X: LogicalPx = crate::generated::primitive::SIZE_14;

    /// `component.preset-cfg-form-max-width` → `{primitive.size-460}` = 460px
    pub const CFG_FORM_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_460;

    /// `component.preset-cfg-form-padding` → `{semantic.space-lg}` = 16px
    pub const CFG_FORM_PADDING: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.preset-cfg-header-height` → `{primitive.size-44}` = 44px
    pub const CFG_HEADER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_44;

    /// `component.preset-leaf-label-font-size` → `{semantic.font-size-micro}` = 10px
    pub const LEAF_LABEL_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.preset-leaf-summary-gap` → `{semantic.space-xs}` = 4px
    pub const LEAF_SUMMARY_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.preset-leaf-value-font-size` → `{semantic.font-size-caption}` = 11px
    pub const LEAF_VALUE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;
}

pub mod progress {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.progress-height` → `{primitive.size-4}` = 4px
    pub const HEIGHT: LogicalPx = crate::generated::primitive::SIZE_4;

    /// `component.progress-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;
}

pub mod remote {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.remote-filter-menu-width` → `{primitive.size-240}` = 240px
    pub const FILTER_MENU_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.remote-label-col` → `{primitive.size-112}` = 112px
    pub const LABEL_COL: LogicalPx = crate::generated::primitive::SIZE_112;
}

pub mod select {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.select-chevron-offset` → `{semantic.space-sm}` = 8px
    pub const CHEVRON_OFFSET: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.select-chevron-room` → `{primitive.size-28}` = 28px
    pub const CHEVRON_ROOM: LogicalPx = crate::generated::primitive::SIZE_28;

    /// `component.select-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.select-height` → `{semantic.control-height}` = 28px
    pub const HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.select-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.select-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod settings {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.settings-row-min-height` → `{primitive.size-32}` = 32px
    pub const ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.settings-sidebar-width` → `{primitive.size-200}` = 200px
    pub const SIDEBAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;
}

pub mod sidebar {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.sidebar-button-label-font-size` → `{semantic.font-size-caption}` = 11px
    pub const BUTTON_LABEL_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.sidebar-category-header-count-font-size` → `{semantic.font-size-micro}` = 10px
    pub const CATEGORY_HEADER_COUNT_FONT_SIZE: LogicalPx =
        crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.sidebar-category-header-pad-x` → `{semantic.space-sm}` = 8px
    pub const CATEGORY_HEADER_PAD_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.sidebar-category-header-pad-y` → `{semantic.space-sm}` = 8px
    pub const CATEGORY_HEADER_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.sidebar-category-header-weight` → `{semantic.font-weight-bold}` = 700
    pub const CATEGORY_HEADER_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_BOLD;

    /// `component.sidebar-collapsed-icon-height` → `{primitive.size-22}` = 22px
    pub const COLLAPSED_ICON_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_22;

    /// `component.sidebar-collapsed-slot-width` → `{primitive.size-32}` = 32px
    pub const COLLAPSED_SLOT_WIDTH: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.sidebar-collapsed-workspace-height` → `{primitive.size-28}` = 28px
    pub const COLLAPSED_WORKSPACE_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_28;

    /// `component.sidebar-logo-collapsed-size` → `{primitive.size-24}` = 24px
    pub const LOGO_COLLAPSED_SIZE: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.sidebar-logo-size` → `{primitive.size-22}` = 22px
    pub const LOGO_SIZE: LogicalPx = crate::generated::primitive::SIZE_22;

    /// `component.sidebar-section-heading-font-size` → `{semantic.font-size-micro}` = 10px
    pub const SECTION_HEADING_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.sidebar-wordmark-font-size` → `{semantic.font-size-brand-wordmark}` = 17px
    pub const WORDMARK_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BRAND_WORDMARK;
}

pub mod spinner {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.spinner-duration` → `{primitive.duration-900}` = 900ms (ms)
    pub const DURATION: f32 = crate::generated::primitive::DURATION_900;

    /// `component.spinner-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod status_dot {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.status-dot-attached-ring-offset` → `{primitive.size-2}` = 2px
    pub const ATTACHED_RING_OFFSET: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.status-dot-attached-ring-width` → `{primitive.size-2}` = 2px
    pub const ATTACHED_RING_WIDTH: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.status-dot-pulse-duration` → `{primitive.duration-1600}` = 1600ms (ms)
    pub const PULSE_DURATION: f32 = crate::generated::primitive::DURATION_1600;

    /// `component.status-dot-size` → `{primitive.size-8}` = 8px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.status-dot-size-compact` → `{primitive.size-6}` = 6px
    pub const SIZE_COMPACT: LogicalPx = crate::generated::primitive::SIZE_6;
}

pub mod statusbar {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.statusbar-dot-size` → `{component.status-dot-size-compact}` = 6px
    pub const DOT_SIZE: LogicalPx = super::status_dot::SIZE_COMPACT;

    /// `component.statusbar-glyph-size` → `{semantic.icon-size-xs}` = 12px
    pub const GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;
}

pub mod surface {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.surface-highlight-done-width` → `{semantic.focus-ring-width}` = 2px
    pub const HIGHLIGHT_DONE_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

    /// `component.surface-highlight-input-width` → `{semantic.focus-ring-width}` = 2px
    pub const HIGHLIGHT_INPUT_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

    /// `component.surface-occupied-border-width` → `{semantic.border-width}` = 1px
    pub const OCCUPIED_BORDER_WIDTH: LogicalPx = crate::generated::semantic::BORDER_WIDTH;
}

pub mod swatch {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.swatch-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.swatch-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod switch {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.switch-radius` → `{semantic.radius-pill}` = 9999px (sentinel — 완전 원형용 상한값)
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_PILL;

    /// `component.switch-thumb-inset` → `{primitive.size-2}` = 2px
    pub const THUMB_INSET: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.switch-thumb-size` → `{primitive.size-12}` = 12px
    pub const THUMB_SIZE: LogicalPx = crate::generated::primitive::SIZE_12;

    /// `component.switch-thumb-travel` → `{primitive.size-12}` = 12px
    pub const THUMB_TRAVEL: LogicalPx = crate::generated::primitive::SIZE_12;

    /// `component.switch-track-height` → `{primitive.size-16}` = 16px
    pub const TRACK_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_16;

    /// `component.switch-track-width` → `{primitive.size-28}` = 28px
    pub const TRACK_WIDTH: LogicalPx = crate::generated::primitive::SIZE_28;
}

pub mod switch_overlay {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.switch-overlay-fade` → `{semantic.motion-ui-fast}` = 90ms (ms)
    pub const FADE: f32 = crate::generated::semantic::MOTION_UI_FAST;

    /// `component.switch-overlay-shadow-depth` → `{component.kbd-shadow-depth}` = 2px
    pub const SHADOW_DEPTH: LogicalPx = super::kbd::SHADOW_DEPTH;

    /// `component.switch-overlay-size` → `{component.kbd-size}` = 16px
    pub const SIZE: LogicalPx = super::kbd::SIZE;
}

pub mod tab {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.tab-close-radius` → `{semantic.radius-sm}` = 2px
    pub const CLOSE_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.tab-close-size` → `{primitive.size-16}` = 16px
    pub const CLOSE_SIZE: LogicalPx = crate::generated::primitive::SIZE_16;

    /// `component.tab-dot-size` → `{component.status-dot-size-compact}` = 6px
    pub const DOT_SIZE: LogicalPx = super::status_dot::SIZE_COMPACT;

    /// `component.tab-gap` → `{semantic.space-sm}` = 8px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.tab-height` → `{semantic.control-height-tab}` = 24px
    pub const HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TAB;

    /// `component.tab-icon-size` → `{semantic.icon-size-sm}` = 14px
    pub const ICON_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.tab-indicator-width` → `{primitive.size-2}` = 2px
    pub const INDICATOR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.tab-padding-x` → `{semantic.space-sm}` = 8px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.tab-strip-width` → `{semantic.tab-width}` = 150px
    pub const STRIP_WIDTH: LogicalPx = crate::generated::semantic::TAB_WIDTH;
}

pub mod table {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.table-cell-height` → `{semantic.control-height}` = 28px
    pub const CELL_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.table-cell-padding-x` → `{semantic.space-md}` = 12px
    pub const CELL_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.table-cell-padding-y` → `{semantic.space-sm}` = 8px
    pub const CELL_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.table-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.table-header-font-size` → `{semantic.font-size-caption}` = 11px
    pub const HEADER_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.table-header-font-weight` → `{semantic.font-weight-medium}` = 500
    pub const HEADER_FONT_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_MEDIUM;
}

pub mod tag {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.tag-dot-size` → `{primitive.size-8}` = 8px
    pub const DOT_SIZE: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.tag-font-size` → `{semantic.font-size-micro}` = 10px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.tag-gap` → `{semantic.space-xs}` = 4px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.tag-padding-x` → `{semantic.space-sm}` = 8px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.tag-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.tag-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod titlebar {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.titlebar-caption-width` → `{primitive.size-46}` = 46px
    pub const CAPTION_WIDTH: LogicalPx = crate::generated::primitive::SIZE_46;

    /// `component.titlebar-csd-radius` → `{primitive.radius-8}` = 8px
    pub const CSD_RADIUS: LogicalPx = crate::generated::primitive::RADIUS_8;

    /// `component.titlebar-csd-shadow-margin` → `{primitive.size-8}` = 8px
    pub const CSD_SHADOW_MARGIN: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.titlebar-traffic-size` → `{primitive.size-12}` = 12px
    pub const TRAFFIC_SIZE: LogicalPx = crate::generated::primitive::SIZE_12;

    /// `component.titlebar-window-button-size` → `{primitive.size-24}` = 24px
    pub const WINDOW_BUTTON_SIZE: LogicalPx = crate::generated::primitive::SIZE_24;
}

pub mod toast {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.toast-accent-width` → `{primitive.size-3}` = 3px
    pub const ACCENT_WIDTH: LogicalPx = crate::generated::primitive::SIZE_3;

    /// `component.toast-gap` → `{semantic.space-sm}` = 8px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.toast-hint-font-size` → `{semantic.font-size-micro}` = 10px
    pub const HINT_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.toast-max-width` → `{primitive.size-320}` = 320px
    pub const MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;

    /// `component.toast-min-height` → `{semantic.control-height}` = 28px
    pub const MIN_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.toast-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.toast-padding-y` → `{semantic.space-sm}` = 8px
    pub const PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.toast-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod tooltip {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.tooltip-delay` → `{semantic.motion-ui-med}` = 150ms (ms)
    pub const DELAY: f32 = crate::generated::semantic::MOTION_UI_MED;

    /// `component.tooltip-font-size` → `{semantic.font-size-caption}` = 11px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.tooltip-line-height` → `{semantic.line-height-ui}` = 1.4
    pub const LINE_HEIGHT: f32 = crate::generated::semantic::LINE_HEIGHT_UI;

    /// `component.tooltip-max-width` → `{primitive.size-240}` = 240px
    pub const MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.tooltip-offset` → `{semantic.space-xs}` = 4px
    pub const OFFSET: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.tooltip-padding-x` → `{semantic.space-sm}` = 8px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.tooltip-padding-y` → `{semantic.space-xs}` = 4px
    pub const PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.tooltip-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod transfer {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.transfer-popup-width` → `{primitive.size-400}` = 400px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_400;
}

pub mod tree_row {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.tree-row-font-size` → `{semantic.font-size-body}` = 13px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.tree-row-gap` → `{semantic.space-xs}` = 4px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.tree-row-height` → `{semantic.control-height-tree}` = 22px
    pub const HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TREE;

    /// `component.tree-row-indent` → `{semantic.space-md}` = 12px
    pub const INDENT: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.tree-row-meta-font-size` → `{semantic.font-size-micro}` = 10px
    pub const META_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;
}

pub mod workspace {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.workspace-mirror-gap` → `{semantic.space-xs}` = 4px
    pub const MIRROR_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.workspace-mirror-icon-size` → `{semantic.icon-size-xs}` = 12px
    pub const MIRROR_ICON_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;
}
