//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! 컴포넌트별 상수. 원본 토큰에 따라 semantic 또는 primitive를 참조한다.
//! 위젯의 치수는 Theme 필드나 접근자로 읽어 UI 배율 정책을 따른다.
//! 색 접근자는 tasty-type-appearance에 생성한다.

pub mod attach {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.attach-refusal-chip-glyph-size` → `{component.move-source-chip-glyph-size}` = 8px
    pub const REFUSAL_CHIP_GLYPH_SIZE: LogicalPx = super::move_source::CHIP_GLYPH_SIZE;

    /// `component.attach-refusal-chip-size` → `{component.move-source-chip-size}` = 12px
    pub const REFUSAL_CHIP_SIZE: LogicalPx = super::move_source::CHIP_SIZE;

    /// `component.attach-sync-name-max-width` → `{primitive.size-160}` = 160px
    pub const SYNC_NAME_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_160;

    /// `component.attach-sync-name-min-width` → `{primitive.size-40}` = 40px
    pub const SYNC_NAME_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_40;
}

pub mod autocomplete {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.autocomplete-match-underline-offset` → `{primitive.size-2}` = 2px
    pub const MATCH_UNDERLINE_OFFSET: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.autocomplete-match-underline-width` → `{semantic.border-width}` = 1px
    pub const MATCH_UNDERLINE_WIDTH: LogicalPx = crate::generated::semantic::BORDER_WIDTH;

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

    /// `component.banner-glyph-offset` → `{primitive.size-1}` = 1px
    pub const GLYPH_OFFSET: LogicalPx = crate::generated::primitive::SIZE_1;

    /// `component.banner-inset-gap` → `{semantic.space-sm}` = 8px
    pub const INSET_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

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

    /// `component.banner-more-reason-glyph-offset` → `{primitive.size-1}` = 1px
    pub const MORE_REASON_GLYPH_OFFSET: LogicalPx = crate::generated::primitive::SIZE_1;

    /// `component.banner-more-reserve` → `{primitive.size-56}` = 56px
    pub const MORE_RESERVE: LogicalPx = crate::generated::primitive::SIZE_56;

    /// `component.banner-narrow-below` → `{primitive.size-440}` = 440px
    pub const NARROW_BELOW: LogicalPx = crate::generated::primitive::SIZE_440;

    /// `component.banner-padding-x` → `{semantic.space-md}` = 12px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.banner-padding-y` → `{semantic.space-sm}` = 8px
    pub const PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.banner-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.banner-recessed-opacity` → `{primitive.opacity-recessed}` = 0.4
    pub const RECESSED_OPACITY: f32 = crate::generated::primitive::OPACITY_RECESSED;

    /// `component.banner-text-gap` → `{semantic.label-detail-gap}` = 2px
    pub const TEXT_GAP: LogicalPx = crate::generated::semantic::LABEL_DETAIL_GAP;

    /// `component.banner-title-font-size` → `{semantic.font-size-body}` = 13px
    pub const TITLE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;
}

pub mod boot {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.boot-form-width` → `{primitive.size-360}` = 360px
    pub const FORM_WIDTH: LogicalPx = crate::generated::primitive::SIZE_360;
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

pub mod center {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.center-state-action-gap` → `{semantic.space-md}` = 12px
    pub const STATE_ACTION_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.center-state-gap` → `{semantic.space-sm}` = 8px
    pub const STATE_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.center-state-glyph-size` → `{semantic.icon-size-lg}` = 24px
    pub const STATE_GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_LG;

    /// `component.center-state-line-gap` → `{semantic.space-xs}` = 4px
    pub const STATE_LINE_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.center-state-max-width` → `{semantic.measure-sm}` = 300px
    pub const STATE_MAX_WIDTH: LogicalPx = crate::generated::semantic::MEASURE_SM;
}

pub mod checkbox {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.checkbox-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.checkbox-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod codearea {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.codearea-font-size` → `{semantic.font-size-caption}` = 11px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.codearea-gutter-width` → `{semantic.space-xl}` = 24px
    pub const GUTTER_WIDTH: LogicalPx = crate::generated::semantic::SPACE_XL;

    /// `component.codearea-line-height` → `{semantic.line-height-ui}` = 1.4
    pub const LINE_HEIGHT: f32 = crate::generated::semantic::LINE_HEIGHT_UI;

    /// `component.codearea-max-height` → `{primitive.size-200}` = 200px
    pub const MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.codearea-padding-x` → `{semantic.space-sm}` = 8px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.codearea-padding-y` → `{semantic.space-xs}` = 4px
    pub const PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;
}

pub mod convert {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.convert-popup-width` → `{primitive.size-240}` = 240px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;
}

pub mod cut {

    /// `component.cut-pending-opacity` → `{semantic.state-dim-opacity}` = 0.5
    pub const PENDING_OPACITY: f32 = crate::generated::semantic::STATE_DIM_OPACITY;
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

    /// `component.dag-edge-selected-width` → `{semantic.focus-ring-width}` = 2px
    pub const EDGE_SELECTED_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

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

    /// `component.dag-node-dim-opacity` → `{semantic.node-dim-opacity}` = 0.75
    pub const NODE_DIM_OPACITY: f32 = crate::generated::semantic::NODE_DIM_OPACITY;

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

    /// `component.explorer-address-pending-delay` → `{semantic.motion-ui-fade}` = 200ms (ms)
    pub const ADDRESS_PENDING_DELAY: f32 = crate::generated::semantic::MOTION_UI_FADE;

    /// `component.explorer-address-pending-size` → `{semantic.icon-size-sm}` = 14px
    pub const ADDRESS_PENDING_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.explorer-autoscroll-zone` → `{primitive.size-24}` = 24px
    pub const AUTOSCROLL_ZONE: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.explorer-conflict-width` → `{primitive.size-400}` = 400px
    pub const CONFLICT_WIDTH: LogicalPx = crate::generated::primitive::SIZE_400;

    /// `component.explorer-create-field-max-width` → `{primitive.size-240}` = 240px
    pub const CREATE_FIELD_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-create-indent` → `{semantic.space-lg}` = 16px
    pub const CREATE_INDENT: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.explorer-cursor-ring-width` → `{semantic.border-width}` = 1px
    pub const CURSOR_RING_WIDTH: LogicalPx = crate::generated::semantic::BORDER_WIDTH;

    /// `component.explorer-delete-list-max-height` → `{primitive.size-160}` = 160px
    pub const DELETE_LIST_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_160;

    /// `component.explorer-drag-chip-max-width` → `{primitive.size-240}` = 240px
    pub const DRAG_CHIP_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-drag-expand-delay` → `{primitive.duration-800}` = 800ms (ms)
    pub const DRAG_EXPAND_DELAY: f32 = crate::generated::primitive::DURATION_800;

    /// `component.explorer-favorites-hide-below` → `{primitive.size-240}` = 240px
    pub const FAVORITES_HIDE_BELOW: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-favorites-pin-height` → `{primitive.size-240}` = 240px
    pub const FAVORITES_PIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-favorites-pin-min-height` → `{primitive.size-120}` = 120px
    pub const FAVORITES_PIN_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_120;

    /// `component.explorer-favorites-pin-ratio` = 0.4
    pub const FAVORITES_PIN_RATIO: f32 = 0.4;

    /// `component.explorer-favorites-pin-threshold` → `{primitive.size-600}` = 600px
    pub const FAVORITES_PIN_THRESHOLD: LogicalPx = crate::generated::primitive::SIZE_600;

    /// `component.explorer-grid-thumb-size` → `{primitive.size-40}` = 40px
    pub const GRID_THUMB_SIZE: LogicalPx = crate::generated::primitive::SIZE_40;

    /// `component.explorer-link-glyph-size` → `{semantic.icon-size-xs}` = 12px
    pub const LINK_GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;

    /// `component.explorer-list-min-width` → `{primitive.size-200}` = 200px
    pub const LIST_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.explorer-min-height` → `{primitive.size-180}` = 180px
    pub const MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_180;

    /// `component.explorer-name-error-max-width` → `{primitive.size-240}` = 240px
    pub const NAME_ERROR_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.explorer-preview-body-min-height` → `{primitive.size-120}` = 120px
    pub const PREVIEW_BODY_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_120;

    /// `component.explorer-preview-header-height` → `{primitive.size-40}` = 40px
    pub const PREVIEW_HEADER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_40;

    /// `component.explorer-preview-max-width` → `{primitive.size-460}` = 460px
    pub const PREVIEW_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_460;

    /// `component.explorer-preview-min-width` → `{primitive.size-200}` = 200px
    pub const PREVIEW_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.explorer-preview-width` → `{primitive.size-288}` = 288px
    pub const PREVIEW_WIDTH: LogicalPx = crate::generated::primitive::SIZE_288;

    /// `component.explorer-progress-height` → `{primitive.size-2}` = 2px
    pub const PROGRESS_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.explorer-props-label-width` → `{primitive.size-96}` = 96px
    pub const PROPS_LABEL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_96;

    /// `component.explorer-props-padding-x` → `{semantic.space-md}` = 12px
    pub const PROPS_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.explorer-props-perm-cell-width` → `{primitive.size-56}` = 56px
    pub const PROPS_PERM_CELL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_56;

    /// `component.explorer-props-perm-role-gap` → `{semantic.space-xs}` = 4px
    pub const PROPS_PERM_ROLE_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.explorer-props-row-line` → `{primitive.size-20}` = 20px
    pub const PROPS_ROW_LINE: LogicalPx = crate::generated::primitive::SIZE_20;

    /// `component.explorer-props-row-min-height` → `{primitive.size-24}` = 24px
    pub const PROPS_ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.explorer-props-row-pad-top` → `{primitive.size-2}` = 2px
    pub const PROPS_ROW_PAD_TOP: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.explorer-props-width` → `{primitive.size-360}` = 360px
    pub const PROPS_WIDTH: LogicalPx = crate::generated::primitive::SIZE_360;

    /// `component.explorer-remote-queue-head-gap` → `{semantic.space-xs}` = 4px
    pub const REMOTE_QUEUE_HEAD_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.explorer-search-bar-height` → `{primitive.size-36}` = 36px
    pub const SEARCH_BAR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_36;

    /// `component.explorer-search-folder-col-width` → `{primitive.size-160}` = 160px
    pub const SEARCH_FOLDER_COL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_160;

    /// `component.explorer-sidebar-width` → `{primitive.size-196}` = 196px
    pub const SIDEBAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_196;

    /// `component.explorer-state-compact-below` → `{primitive.size-120}` = 120px
    pub const STATE_COMPACT_BELOW: LogicalPx = crate::generated::primitive::SIZE_120;

    /// `component.explorer-toolbar-compact-below` → `{primitive.size-440}` = 440px
    pub const TOOLBAR_COMPACT_BELOW: LogicalPx = crate::generated::primitive::SIZE_440;
}

pub mod fh {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.fh-when-width` → `{primitive.size-56}` = 56px
    pub const WHEN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_56;
}

pub mod file {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.file-handler-form-padding` → `{semantic.space-md}` = 12px
    pub const HANDLER_FORM_PADDING: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.file-handler-form-row-gap` → `{semantic.space-sm}` = 8px
    pub const HANDLER_FORM_ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.file-handler-id-min-width` → `{primitive.size-96}` = 96px
    pub const HANDLER_ID_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_96;

    /// `component.file-handler-kind-width` → `{semantic.field-width-md}` = 160px
    pub const HANDLER_KIND_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_MD;

    /// `component.file-handler-priority-width` → `{semantic.field-width-xs}` = 90px
    pub const HANDLER_PRIORITY_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_XS;

    /// `component.file-handler-select-width` → `{semantic.field-width-lg}` = 200px
    pub const HANDLER_SELECT_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_LG;
}

pub mod font {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.font-combo-list-max-height` → `{component.select-search-list-max-height}` = 300px
    pub const COMBO_LIST_MAX_HEIGHT: LogicalPx = super::select::SEARCH_LIST_MAX_HEIGHT;

    /// `component.font-combo-search-inset` → `{component.select-search-inset}` = 4px
    pub const COMBO_SEARCH_INSET: LogicalPx = super::select::SEARCH_INSET;

    /// `component.font-preview-line-height` → `{semantic.line-height-ui}` = 1.4
    pub const PREVIEW_LINE_HEIGHT: f32 = crate::generated::semantic::LINE_HEIGHT_UI;

    /// `component.font-preview-min-width` → `{semantic.field-width-lg}` = 200px
    pub const PREVIEW_MIN_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_LG;

    /// `component.font-preview-padding-x` → `{semantic.space-md}` = 12px
    pub const PREVIEW_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.font-preview-padding-y` → `{semantic.space-sm}` = 8px
    pub const PREVIEW_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_SM;
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

    /// `component.fp-filter-height` → `{semantic.control-height}` = 28px
    pub const FILTER_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.fp-filter-max-width` → `{semantic.field-width-md}` = 160px
    pub const FILTER_MAX_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_MD;

    /// `component.fp-footer-label-width` → `{primitive.size-64}` = 64px
    pub const FOOTER_LABEL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_64;

    /// `component.fp-footer-pad-y` → `{semantic.space-sm}` = 8px
    pub const FOOTER_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.fp-header-pad-y` → `{semantic.space-sm}` = 8px
    pub const HEADER_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.fp-inset-end` → `{semantic.space-sm}` = 8px
    pub const INSET_END: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.fp-inset-start` → `{semantic.space-md}` = 12px
    pub const INSET_START: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.fp-list-head-pad-y` → `{semantic.space-xs}` = 4px
    pub const LIST_HEAD_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.fp-path-pad-y` → `{semantic.space-xs}` = 4px
    pub const PATH_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.fp-popup-min-width` → `{primitive.size-320}` = 320px
    pub const POPUP_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;

    /// `component.fp-row-pad-y` → `{semantic.space-xs}` = 4px
    pub const ROW_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.fp-section-gap` → `{semantic.space-sm}` = 8px
    pub const SECTION_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;
}

pub mod git {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.git-toolbar-height` → `{semantic.toolbar-height}` = 32px
    pub const TOOLBAR_HEIGHT: LogicalPx = crate::generated::semantic::TOOLBAR_HEIGHT;
}

pub mod help_hint {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.help-hint-gap` → `{semantic.space-xs}` = 4px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.help-hint-size` → `{semantic.icon-size-sm}` = 14px
    pub const SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;
}

pub mod html {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.html-address-bar-gap` → `{semantic.space-sm}` = 8px
    pub const ADDRESS_BAR_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.html-address-bar-height` → `{primitive.size-40}` = 40px
    pub const ADDRESS_BAR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_40;

    /// `component.html-address-bar-nav-gap` → `{semantic.space-xs}` = 4px
    pub const ADDRESS_BAR_NAV_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.html-address-bar-padding-x` → `{semantic.space-sm}` = 8px
    pub const ADDRESS_BAR_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.html-address-error-height` → `{primitive.size-24}` = 24px
    pub const ADDRESS_ERROR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.html-address-field-min-width` → `{primitive.size-96}` = 96px
    pub const ADDRESS_FIELD_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_96;

    /// `component.html-remote-marker-hit` → `{primitive.size-20}` = 20px
    pub const REMOTE_MARKER_HIT: LogicalPx = crate::generated::primitive::SIZE_20;

    /// `component.html-remote-marker-size` → `{semantic.icon-size-sm}` = 14px
    pub const REMOTE_MARKER_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.html-script-marker-hit` → `{primitive.size-16}` = 16px
    pub const SCRIPT_MARKER_HIT: LogicalPx = crate::generated::primitive::SIZE_16;

    /// `component.html-script-marker-size` → `{semantic.icon-size-xs}` = 12px
    pub const SCRIPT_MARKER_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;
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

pub mod image {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.image-handle-size` → `{primitive.size-6}` = 6px
    pub const HANDLE_SIZE: LogicalPx = crate::generated::primitive::SIZE_6;

    /// `component.image-path-row-gap` → `{semantic.space-sm}` = 8px
    pub const PATH_ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.image-popup-btn-gap` → `{semantic.space-sm}` = 8px
    pub const POPUP_BTN_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.image-popup-gap` → `{semantic.space-sm}` = 8px
    pub const POPUP_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.image-popup-pad-top` → `{semantic.space-md}` = 12px
    pub const POPUP_PAD_TOP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.image-popup-pad-x` → `{semantic.space-md}` = 12px
    pub const POPUP_PAD_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.image-popup-title-font-size` → `{semantic.font-size-max}` = 14px
    pub const POPUP_TITLE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MAX;

    /// `component.image-popup-title-weight` → `{semantic.font-weight-semibold}` = 600
    pub const POPUP_TITLE_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_SEMIBOLD;

    /// `component.image-popup-width` → `{primitive.size-300}` = 300px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_300;

    /// `component.image-size-input-width` → `{primitive.size-64}` = 64px
    pub const SIZE_INPUT_WIDTH: LogicalPx = crate::generated::primitive::SIZE_64;

    /// `component.image-zoom-font-size` → `{semantic.font-size-caption}` = 11px
    pub const ZOOM_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.image-zoom-min-width` → `{primitive.size-40}` = 40px
    pub const ZOOM_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_40;
}

pub mod info {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.info-modal-max-height` → `{primitive.size-360}` = 360px
    pub const MODAL_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_360;

    /// `component.info-modal-min-height` → `{primitive.size-140}` = 140px
    pub const MODAL_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_140;

    /// `component.info-modal-para-gap` → `{semantic.space-md}` = 12px
    pub const MODAL_PARA_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.info-modal-width` → `{primitive.size-440}` = 440px
    pub const MODAL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_440;
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

pub mod kb {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.kb-ie-action-column-width` → `{primitive.size-288}` = 288px
    pub const IE_ACTION_COLUMN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_288;

    /// `component.kb-ie-from-column-width` → `{primitive.size-120}` = 120px
    pub const IE_FROM_COLUMN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_120;

    /// `component.kb-ie-notice-inset` → `{semantic.space-md}` = 12px
    pub const IE_NOTICE_INSET: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.kb-ie-select-column-width` → `{primitive.size-32}` = 32px
    pub const IE_SELECT_COLUMN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.kb-ie-slot-height` → `{semantic.control-height-tab}` = 24px
    pub const IE_SLOT_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TAB;

    /// `component.kb-ie-slot-min-width` → `{primitive.size-140}` = 140px
    pub const IE_SLOT_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_140;

    /// `component.kb-plugin-caption-gap` → `{semantic.space-xs}` = 4px
    pub const PLUGIN_CAPTION_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.kb-plugin-control-gap` → `{semantic.space-sm}` = 8px
    pub const PLUGIN_CONTROL_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.kb-plugin-control-height` → `{semantic.control-height}` = 28px
    pub const PLUGIN_CONTROL_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.kb-plugin-draft-dot-size` → `{component.status-dot-size-compact}` = 6px
    pub const PLUGIN_DRAFT_DOT_SIZE: LogicalPx = super::status_dot::SIZE_COMPACT;

    /// `component.kb-plugin-list-gap` → `{semantic.space-md}` = 12px
    pub const PLUGIN_LIST_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.kb-plugin-mode-width` → `{semantic.field-width-md}` = 160px
    pub const PLUGIN_MODE_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_MD;

    /// `component.kb-plugin-picker-width` → `{semantic.field-width-lg}` = 200px
    pub const PLUGIN_PICKER_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_LG;

    /// `component.kb-plugin-record-height` → `{component.kb-plugin-control-height}` = 28px
    pub const PLUGIN_RECORD_HEIGHT: LogicalPx = super::kb::PLUGIN_CONTROL_HEIGHT;

    /// `component.kb-plugin-row-min-height` → `{component.settings-row-min-height}` = 32px
    pub const PLUGIN_ROW_MIN_HEIGHT: LogicalPx = super::settings::ROW_MIN_HEIGHT;

    /// `component.kb-plugin-row-padding-y` → `{semantic.space-xs}` = 4px
    pub const PLUGIN_ROW_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.kb-plugin-slot-width` → `{semantic.field-width-lg}` = 200px
    pub const PLUGIN_SLOT_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_LG;

    /// `component.kb-plugin-title-gap` → `{component.settings-label-gap}` = 16px
    pub const PLUGIN_TITLE_GAP: LogicalPx = super::settings::LABEL_GAP;

    /// `component.kb-plugin-title-width` → `{component.settings-label-width}` = 150px
    pub const PLUGIN_TITLE_WIDTH: LogicalPx = super::settings::LABEL_WIDTH;

    /// `component.kb-record-add-width` → `{primitive.size-32}` = 32px
    pub const RECORD_ADD_WIDTH: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.kb-record-height` → `{component.kb-ie-slot-height}` = 24px
    pub const RECORD_HEIGHT: LogicalPx = super::kb::IE_SLOT_HEIGHT;

    /// `component.kb-record-width` → `{component.kb-ie-slot-min-width}` = 140px
    pub const RECORD_WIDTH: LogicalPx = super::kb::IE_SLOT_MIN_WIDTH;

    /// `component.kb-row-gap` → `{semantic.space-sm}` = 8px
    pub const ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;
}

pub mod kbd {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.kbd-font-size` → `{semantic.font-size-micro}` = 10px
    pub const FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.kbd-gap` → `{semantic.space-xs}` = 4px
    pub const GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

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

    /// `component.listctrl-selected-bar-width` → `{semantic.selection-edge-width}` = 2px
    pub const SELECTED_BAR_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;
}

pub mod loading {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.loading-lockup-font-size` → `{semantic.font-size-brand-lockup}` = 38px
    pub const LOCKUP_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_BRAND_LOCKUP;

    /// `component.loading-lockup-tracking` → `{primitive.letter-spacing-n1px}` = -1px
    pub const LOCKUP_TRACKING: LogicalPx = crate::generated::primitive::LETTER_SPACING_N1PX;

    /// `component.loading-overlay-delay` → `{semantic.motion-ui-fade}` = 200ms (ms)
    pub const OVERLAY_DELAY: f32 = crate::generated::semantic::MOTION_UI_FADE;

    /// `component.loading-overlay-lnb-size` → `{semantic.icon-size-sm}` = 14px
    pub const OVERLAY_LNB_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.loading-overlay-rail-size` → `{primitive.size-28}` = 28px
    pub const OVERLAY_RAIL_SIZE: LogicalPx = crate::generated::primitive::SIZE_28;

    /// `component.loading-overlay-surface-bed-size` → `{primitive.size-24}` = 24px
    pub const OVERLAY_SURFACE_BED_SIZE: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.loading-overlay-surface-inset` → `{semantic.space-sm}` = 8px
    pub const OVERLAY_SURFACE_INSET: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.loading-overlay-surface-size` → `{component.spinner-size}` = 16px
    pub const OVERLAY_SURFACE_SIZE: LogicalPx = super::spinner::SIZE;

    /// `component.loading-overlay-tab-size` → `{component.tab-icon-size}` = 14px
    pub const OVERLAY_TAB_SIZE: LogicalPx = super::tab::ICON_SIZE;
}

pub mod md {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.md-address-bar-gap` → `{component.html-address-bar-gap}` = 8px
    pub const ADDRESS_BAR_GAP: LogicalPx = super::html::ADDRESS_BAR_GAP;

    /// `component.md-address-bar-height` → `{component.html-address-bar-height}` = 40px
    pub const ADDRESS_BAR_HEIGHT: LogicalPx = super::html::ADDRESS_BAR_HEIGHT;

    /// `component.md-address-bar-nav-gap` → `{component.html-address-bar-nav-gap}` = 4px
    pub const ADDRESS_BAR_NAV_GAP: LogicalPx = super::html::ADDRESS_BAR_NAV_GAP;

    /// `component.md-address-bar-padding-x` → `{component.html-address-bar-padding-x}` = 8px
    pub const ADDRESS_BAR_PADDING_X: LogicalPx = super::html::ADDRESS_BAR_PADDING_X;

    /// `component.md-address-error-height` → `{component.html-address-error-height}` = 24px
    pub const ADDRESS_ERROR_HEIGHT: LogicalPx = super::html::ADDRESS_ERROR_HEIGHT;

    /// `component.md-address-field-min-width` → `{component.html-address-field-min-width}` = 96px
    pub const ADDRESS_FIELD_MIN_WIDTH: LogicalPx = super::html::ADDRESS_FIELD_MIN_WIDTH;

    /// `component.md-address-go-hide-below` → `{semantic.field-width-md}` = 160px
    pub const ADDRESS_GO_HIDE_BELOW: LogicalPx = crate::generated::semantic::FIELD_WIDTH_MD;

    /// `component.md-callout-icon-gap` → `{semantic.space-xs}` = 4px
    pub const CALLOUT_ICON_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.md-callout-marker-size` → `{semantic.icon-size-sm}` = 14px
    pub const CALLOUT_MARKER_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.md-quote-bar-width` → `{semantic.selection-edge-width}` = 2px
    pub const QUOTE_BAR_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;

    /// `component.md-table-cell-padding-x` → `{semantic.space-sm}` = 8px
    pub const TABLE_CELL_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.md-table-cell-padding-y` → `{semantic.space-xs}` = 4px
    pub const TABLE_CELL_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;
}

pub mod menu {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.menu-item-check-size` → `{semantic.icon-size-sm}` = 14px
    pub const ITEM_CHECK_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.menu-item-height` → `{semantic.control-height}` = 28px
    pub const ITEM_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;

    /// `component.menu-item-padding-x` → `{semantic.space-md}` = 12px
    pub const ITEM_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.menu-item-radius` → `{semantic.radius-sm}` = 2px
    pub const ITEM_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;

    /// `component.menu-item-shortcut-font-size` → `{semantic.font-size-micro}` = 10px
    pub const ITEM_SHORTCUT_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.menu-item-wrap-padding-y` → `{semantic.space-xs}` = 4px
    pub const ITEM_WRAP_PADDING_Y: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.menu-radius` → `{semantic.radius}` = 4px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS;
}

pub mod modhint {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.modhint-empty-row-min-height` → `{primitive.size-20}` = 20px
    pub const EMPTY_ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_20;

    /// `component.modhint-fade` → `{semantic.motion-ui-fade}` = 200ms (ms)
    pub const FADE: f32 = crate::generated::semantic::MOTION_UI_FADE;

    /// `component.modhint-grip-inset` → `{primitive.size-2}` = 2px
    pub const GRIP_INSET: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.modhint-grip-size` → `{semantic.icon-size-xs}` = 12px
    pub const GRIP_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;

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

    /// `component.modhint-row-font-size` → `{semantic.font-size-caption}` = 11px
    pub const ROW_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.modhint-row-gap` → `{semantic.space-sm}` = 8px
    pub const ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.modhint-row-min-height` → `{primitive.size-24}` = 24px
    pub const ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.modhint-section-gap` → `{semantic.space-md}` = 12px
    pub const SECTION_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.modhint-width` → `{primitive.size-180}` = 180px
    pub const WIDTH: LogicalPx = crate::generated::primitive::SIZE_180;
}

pub mod move_source {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.move-source-chip-glyph-size` → `{primitive.size-8}` = 8px
    pub const CHIP_GLYPH_SIZE: LogicalPx = crate::generated::primitive::SIZE_8;

    /// `component.move-source-chip-size` → `{primitive.size-12}` = 12px
    pub const CHIP_SIZE: LogicalPx = crate::generated::primitive::SIZE_12;

    /// `component.move-source-dash` → `{primitive.size-4}` = 4px
    pub const DASH: LogicalPx = crate::generated::primitive::SIZE_4;

    /// `component.move-source-dash-gap` → `{primitive.size-4}` = 4px
    pub const DASH_GAP: LogicalPx = crate::generated::primitive::SIZE_4;

    /// `component.move-source-glyph-size` → `{semantic.icon-size-xs}` = 12px
    pub const GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;

    /// `component.move-source-ring-width` → `{semantic.focus-ring-width}` = 2px
    pub const RING_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;
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

pub mod notice {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.notice-column-gap` → `{semantic.space-sm}` = 8px
    pub const COLUMN_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notice-column-inset` → `{semantic.space-sm}` = 8px
    pub const COLUMN_INSET: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notice-column-width` → `{primitive.size-360}` = 360px
    pub const COLUMN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_360;
}

pub mod notification {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.notification-badge-glyph-gap` → `{primitive.size-2}` = 2px
    pub const BADGE_GLYPH_GAP: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.notification-badge-glyph-size` → `{primitive.size-10}` = 10px
    pub const BADGE_GLYPH_SIZE: LogicalPx = crate::generated::primitive::SIZE_10;

    /// `component.notification-badge-group-gap` → `{semantic.space-xs}` = 4px
    pub const BADGE_GROUP_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.notification-badge-max` = 99
    pub const BADGE_MAX: f32 = 99.0;

    /// `component.notification-badge-stack-gap` → `{primitive.size-2}` = 2px
    pub const BADGE_STACK_GAP: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.notification-item-body-lines` = 2
    pub const ITEM_BODY_LINES: f32 = 2.0;

    /// `component.notification-item-body-lines-wide` = 3
    pub const ITEM_BODY_LINES_WIDE: f32 = 3.0;

    /// `component.notification-item-glyph-gap` → `{semantic.space-sm}` = 8px
    pub const ITEM_GLYPH_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notification-item-glyph-size` → `{semantic.icon-size-sm}` = 14px
    pub const ITEM_GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.notification-item-line-gap` → `{primitive.size-2}` = 2px
    pub const ITEM_LINE_GAP: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.notification-item-pad-x` → `{semantic.space-md}` = 12px
    pub const ITEM_PAD_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.notification-item-pad-y` → `{semantic.space-sm}` = 8px
    pub const ITEM_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notification-item-title-weight` → `{semantic.font-weight-semibold}` = 600
    pub const ITEM_TITLE_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_SEMIBOLD;

    /// `component.notification-item-unread-dot-size` → `{component.badge-dot-size}` = 8px
    pub const ITEM_UNREAD_DOT_SIZE: LogicalPx = super::badge::DOT_SIZE;

    /// `component.notification-rail-width` → `{primitive.size-44}` = 44px
    pub const RAIL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_44;

    /// `component.notification-summary-icon-gap` → `{semantic.space-sm}` = 8px
    pub const SUMMARY_ICON_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notification-summary-pad` → `{semantic.space-sm}` = 8px
    pub const SUMMARY_PAD: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notification-summary-pad-x` → `{semantic.space-sm}` = 8px
    pub const SUMMARY_PAD_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.notification-summary-preview-font-size` → `{semantic.font-size-caption}` = 11px
    pub const SUMMARY_PREVIEW_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.notification-summary-row-height` → `{primitive.size-24}` = 24px
    pub const SUMMARY_ROW_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_24;
}

pub mod notifications {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.notifications-popup-height` → `{primitive.size-400}` = 400px
    pub const POPUP_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_400;

    /// `component.notifications-popup-width` → `{primitive.size-352}` = 352px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_352;

    /// `component.notifications-toolbar-height` → `{semantic.control-height}` = 28px
    pub const TOOLBAR_HEIGHT: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT;
}

pub mod number {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.number-field-min-width` → `{semantic.field-width-xs}` = 90px
    pub const FIELD_MIN_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_XS;

    /// `component.number-field-unit-gap` → `{semantic.space-sm}` = 8px
    pub const FIELD_UNIT_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;
}

pub mod palette {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.palette-list-max-height` → `{primitive.size-320}` = 320px
    pub const LIST_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_320;

    /// `component.palette-width` → `{primitive.size-540}` = 540px
    pub const WIDTH: LogicalPx = crate::generated::primitive::SIZE_540;
}

pub mod pending {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.pending-indicator-compact-below` → `{primitive.size-200}` = 200px
    pub const INDICATOR_COMPACT_BELOW: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.pending-indicator-delay` → `{component.loading-overlay-delay}` = 200ms (ms)
    pub const INDICATOR_DELAY: f32 = super::loading::OVERLAY_DELAY;

    /// `component.pending-indicator-gap` → `{semantic.space-sm}` = 8px
    pub const INDICATOR_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.pending-indicator-height` → `{primitive.size-24}` = 24px
    pub const INDICATOR_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.pending-indicator-padding-x` → `{semantic.space-sm}` = 8px
    pub const INDICATOR_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.pending-indicator-radius` → `{semantic.radius}` = 4px
    pub const INDICATOR_RADIUS: LogicalPx = crate::generated::semantic::RADIUS;

    /// `component.pending-indicator-spinner-size` → `{semantic.icon-size-sm}` = 14px
    pub const INDICATOR_SPINNER_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;
}

pub mod perm {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.perm-row-height` → `{component.settings-row-min-height}` = 32px
    pub const ROW_HEIGHT: LogicalPx = super::settings::ROW_MIN_HEIGHT;

    /// `component.perm-status-gap` → `{semantic.space-xs}` = 4px
    pub const STATUS_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;
}

pub mod plugin {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.plugin-avatar-border-width` → `{semantic.border-width}` = 1px
    pub const AVATAR_BORDER_WIDTH: LogicalPx = crate::generated::semantic::BORDER_WIDTH;

    /// `component.plugin-avatar-initial-font-size-lg` → `{semantic.font-size-max}` = 14px
    pub const AVATAR_INITIAL_FONT_SIZE_LG: LogicalPx = crate::generated::semantic::FONT_SIZE_MAX;

    /// `component.plugin-avatar-initial-font-size-sm` → `{semantic.font-size-body}` = 13px
    pub const AVATAR_INITIAL_FONT_SIZE_SM: LogicalPx = crate::generated::semantic::FONT_SIZE_BODY;

    /// `component.plugin-avatar-initial-weight` → `{semantic.font-weight-normal}` = 400
    pub const AVATAR_INITIAL_WEIGHT: u16 = crate::generated::semantic::FONT_WEIGHT_NORMAL;

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

pub mod popup {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.popup-content-margin` → `{semantic.space-xs}` = 4px
    pub const CONTENT_MARGIN: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.popup-title-btn-gap` → `{semantic.space-xs}` = 4px
    pub const TITLE_BTN_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.popup-title-btn-size` → `{component.icon-button-size-sm}` = 24px
    pub const TITLE_BTN_SIZE: LogicalPx = super::icon_button::SIZE_SM;

    /// `component.popup-title-edge-inset` → `{semantic.space-xs}` = 4px
    pub const TITLE_EDGE_INSET: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.popup-title-text-gap` → `{semantic.space-xs}` = 4px
    pub const TITLE_TEXT_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;
}

pub mod port {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.port-addr-col-min-width` → `{primitive.size-140}` = 140px
    pub const ADDR_COL_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_140;

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

    /// `component.preset-cfg-dim-opacity` → `{semantic.state-dim-opacity}` = 0.5
    pub const CFG_DIM_OPACITY: f32 = crate::generated::semantic::STATE_DIM_OPACITY;

    /// `component.preset-cfg-field-gap` → `{semantic.space-md}` = 12px
    pub const CFG_FIELD_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.preset-cfg-footer-height` → `{primitive.size-52}` = 52px
    pub const CFG_FOOTER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_52;

    /// `component.preset-cfg-footer-padding-x` → `{semantic.space-md}` = 12px
    pub const CFG_FOOTER_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.preset-cfg-form-max-width` → `{primitive.size-460}` = 460px
    pub const CFG_FORM_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_460;

    /// `component.preset-cfg-form-padding` → `{semantic.space-lg}` = 16px
    pub const CFG_FORM_PADDING: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.preset-cfg-header-height` → `{primitive.size-44}` = 44px
    pub const CFG_HEADER_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_44;

    /// `component.preset-leaf-label-font-size` → `{semantic.font-size-micro}` = 10px
    pub const LEAF_LABEL_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_MICRO;

    /// `component.preset-leaf-selected-ring-width` → `{semantic.focus-ring-width}` = 2px
    pub const LEAF_SELECTED_RING_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

    /// `component.preset-leaf-summary-gap` → `{semantic.space-xs}` = 4px
    pub const LEAF_SUMMARY_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.preset-leaf-value-font-size` → `{semantic.font-size-caption}` = 11px
    pub const LEAF_VALUE_FONT_SIZE: LogicalPx = crate::generated::semantic::FONT_SIZE_CAPTION;

    /// `component.preset-split-divider-width` → `{semantic.selection-edge-width}` = 2px
    pub const SPLIT_DIVIDER_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;
}

pub mod progress {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.progress-height` → `{primitive.size-4}` = 4px
    pub const HEIGHT: LogicalPx = crate::generated::primitive::SIZE_4;

    /// `component.progress-radius` → `{semantic.radius-sm}` = 2px
    pub const RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;
}

pub mod rail {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.rail-category-line-length` → `{primitive.size-24}` = 24px
    pub const CATEGORY_LINE_LENGTH: LogicalPx = crate::generated::primitive::SIZE_24;

    /// `component.rail-category-line-width` → `{semantic.focus-ring-width}` = 2px
    pub const CATEGORY_LINE_WIDTH: LogicalPx = crate::generated::semantic::FOCUS_RING_WIDTH;

    /// `component.rail-flat-divider-length` → `{primitive.size-28}` = 28px
    pub const FLAT_DIVIDER_LENGTH: LogicalPx = crate::generated::primitive::SIZE_28;
}

pub mod remote {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.remote-filter-dropdown-width` → `{primitive.size-240}` = 240px
    pub const FILTER_DROPDOWN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.remote-filter-menu-width` → `{primitive.size-240}` = 240px
    pub const FILTER_MENU_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.remote-label-col` → `{primitive.size-112}` = 112px
    pub const LABEL_COL: LogicalPx = crate::generated::primitive::SIZE_112;
}

pub mod restore {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.restore-notice-gap` → `{semantic.space-sm}` = 8px
    pub const NOTICE_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.restore-notice-padding` → `{semantic.space-md}` = 12px
    pub const NOTICE_PADDING: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.restore-notice-width` → `{component.notice-column-width}` = 360px
    pub const NOTICE_WIDTH: LogicalPx = super::notice::COLUMN_WIDTH;
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

    /// `component.select-search-field-gap` → `{component.input-gap}` = 8px
    pub const SEARCH_FIELD_GAP: LogicalPx = super::input::GAP;

    /// `component.select-search-field-height` → `{component.input-height}` = 28px
    pub const SEARCH_FIELD_HEIGHT: LogicalPx = super::input::HEIGHT;

    /// `component.select-search-field-padding-x` → `{semantic.space-sm}` = 8px
    pub const SEARCH_FIELD_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.select-search-inset` → `{semantic.space-xs}` = 4px
    pub const SEARCH_INSET: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.select-search-list-max-height` → `{primitive.size-300}` = 300px
    pub const SEARCH_LIST_MAX_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_300;

    /// `component.select-search-menu-gap` → `{semantic.space-xs}` = 4px
    pub const SEARCH_MENU_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.select-search-menu-max-width` → `{primitive.size-320}` = 320px
    pub const SEARCH_MENU_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_320;
}

pub mod settings {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.settings-apply-note-gap` → `{semantic.space-sm}` = 8px
    pub const APPLY_NOTE_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.settings-content-max-width` → `{primitive.size-620}` = 620px
    pub const CONTENT_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_620;

    /// `component.settings-label-gap` → `{semantic.space-lg}` = 16px
    pub const LABEL_GAP: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.settings-label-max-width` → `{primitive.size-240}` = 240px
    pub const LABEL_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.settings-label-width` → `{primitive.size-150}` = 150px
    pub const LABEL_WIDTH: LogicalPx = crate::generated::primitive::SIZE_150;

    /// `component.settings-row-caption-gap` → `{semantic.space-xs}` = 4px
    pub const ROW_CAPTION_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.settings-row-gap` → `{semantic.space-md}` = 12px
    pub const ROW_GAP: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.settings-row-min-height` → `{primitive.size-32}` = 32px
    pub const ROW_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_32;

    /// `component.settings-row-stack-gap` → `{semantic.space-xs}` = 4px
    pub const ROW_STACK_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.settings-row-stack-hysteresis` → `{semantic.space-lg}` = 16px
    pub const ROW_STACK_HYSTERESIS: LogicalPx = crate::generated::semantic::SPACE_LG;

    /// `component.settings-sidebar-width` → `{primitive.size-200}` = 200px
    pub const SIDEBAR_WIDTH: LogicalPx = crate::generated::primitive::SIZE_200;

    /// `component.settings-window-height` → `{primitive.size-700}` = 700px
    pub const WINDOW_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_700;

    /// `component.settings-window-width` → `{primitive.size-1100}` = 1100px
    pub const WINDOW_WIDTH: LogicalPx = crate::generated::primitive::SIZE_1100;
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

    /// `component.sidebar-wordmark-tracking` → `{semantic.letter-spacing-ui}` = 0
    pub const WORDMARK_TRACKING: LogicalPx = crate::generated::semantic::LETTER_SPACING_UI;
}

pub mod spinner {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.spinner-duration` → `{primitive.duration-900}` = 900ms (ms)
    pub const DURATION: f32 = crate::generated::primitive::DURATION_900;

    /// `component.spinner-size` → `{primitive.size-16}` = 16px
    pub const SIZE: LogicalPx = crate::generated::primitive::SIZE_16;
}

pub mod split {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.split-sibling-min-height` → `{primitive.size-56}` = 56px
    pub const SIBLING_MIN_HEIGHT: LogicalPx = crate::generated::primitive::SIZE_56;
}

pub mod status_dot {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.status-dot-attached-ring-offset` → `{primitive.size-2}` = 2px
    pub const ATTACHED_RING_OFFSET: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.status-dot-attached-ring-width` → `{primitive.size-2}` = 2px
    pub const ATTACHED_RING_WIDTH: LogicalPx = crate::generated::primitive::SIZE_2;

    /// `component.status-dot-pulse-duration` → `{primitive.duration-1600}` = 1600ms (ms)
    pub const PULSE_DURATION: f32 = crate::generated::primitive::DURATION_1600;

    /// `component.status-dot-ring-width` → `{primitive.size-1-5}` = 1.5px
    pub const RING_WIDTH: LogicalPx = crate::generated::primitive::SIZE_1_5;

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

    /// `component.surface-focus-edge-width` → `{semantic.selection-edge-width}` = 2px
    pub const FOCUS_EDGE_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;

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

    /// `component.tab-indicator-width` → `{semantic.selection-edge-width}` = 2px
    pub const INDICATOR_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;

    /// `component.tab-padding-x` → `{semantic.space-sm}` = 8px
    pub const PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.tab-scroll-arrow-glyph-size` → `{semantic.icon-size-sm}` = 14px
    pub const SCROLL_ARROW_GLYPH_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_SM;

    /// `component.tab-scroll-arrow-width` → `{semantic.control-height-tab}` = 24px
    pub const SCROLL_ARROW_WIDTH: LogicalPx = crate::generated::semantic::CONTROL_HEIGHT_TAB;

    /// `component.tab-status-gap` → `{semantic.space-xs}` = 4px
    pub const STATUS_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

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

    /// `component.toast-accent-width` → `{semantic.tint-edge-width}` = 2px
    pub const ACCENT_WIDTH: LogicalPx = crate::generated::semantic::TINT_EDGE_WIDTH;

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

    /// `component.toast-stack-offset-bottom` → `{primitive.size-36}` = 36px
    pub const STACK_OFFSET_BOTTOM: LogicalPx = crate::generated::primitive::SIZE_36;

    /// `component.toast-stack-offset-bottom-settings` → `{primitive.size-64}` = 64px
    pub const STACK_OFFSET_BOTTOM_SETTINGS: LogicalPx = crate::generated::primitive::SIZE_64;
}

pub mod tools {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.tools-menu-max-width` → `{primitive.size-240}` = 240px
    pub const MENU_MAX_WIDTH: LogicalPx = crate::generated::primitive::SIZE_240;

    /// `component.tools-menu-min-width` → `{primitive.size-160}` = 160px
    pub const MENU_MIN_WIDTH: LogicalPx = crate::generated::primitive::SIZE_160;
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

    /// `component.transfer-body-gap` → `{semantic.space-sm}` = 8px
    pub const BODY_GAP: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.transfer-footer-pad-y` → `{semantic.space-sm}` = 8px
    pub const FOOTER_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;

    /// `component.transfer-header-pad-y` → `{semantic.space-md}` = 12px
    pub const HEADER_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.transfer-pad-x` → `{semantic.space-md}` = 12px
    pub const PAD_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.transfer-popup-width` → `{primitive.size-400}` = 400px
    pub const POPUP_WIDTH: LogicalPx = crate::generated::primitive::SIZE_400;

    /// `component.transfer-well-pad-x` → `{semantic.space-md}` = 12px
    pub const WELL_PAD_X: LogicalPx = crate::generated::semantic::SPACE_MD;

    /// `component.transfer-well-pad-y` → `{semantic.space-sm}` = 8px
    pub const WELL_PAD_Y: LogicalPx = crate::generated::semantic::SPACE_SM;
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

pub mod trigger {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.trigger-menu-max-height` → `{component.autocomplete-max-height}` = 220px
    pub const MENU_MAX_HEIGHT: LogicalPx = super::autocomplete::MAX_HEIGHT;

    /// `component.trigger-menu-min-width` → `{semantic.field-width-lg}` = 200px
    pub const MENU_MIN_WIDTH: LogicalPx = crate::generated::semantic::FIELD_WIDTH_LG;
}

pub mod ui {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.ui-code-padding-x` → `{semantic.space-xs}` = 4px
    pub const CODE_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.ui-code-radius` → `{semantic.radius-sm}` = 2px
    pub const CODE_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_SM;
}

pub mod webview {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.webview-opaque-radius` → `{semantic.radius-none}` = 0px
    pub const OPAQUE_RADIUS: LogicalPx = crate::generated::semantic::RADIUS_NONE;
}

pub mod workspace {
    use tasty_type_geometry::length::LogicalPx;

    /// `component.workspace-dot-gap` → `{semantic.space-xs}` = 4px
    pub const DOT_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.workspace-dot-slot` → `{primitive.size-16}` = 16px
    pub const DOT_SLOT: LogicalPx = crate::generated::primitive::SIZE_16;

    /// `component.workspace-mirror-chip-size` → `{primitive.size-12}` = 12px
    pub const MIRROR_CHIP_SIZE: LogicalPx = crate::generated::primitive::SIZE_12;

    /// `component.workspace-mirror-gap` → `{semantic.space-xs}` = 4px
    pub const MIRROR_GAP: LogicalPx = crate::generated::semantic::SPACE_XS;

    /// `component.workspace-mirror-icon-size` → `{semantic.icon-size-xs}` = 12px
    pub const MIRROR_ICON_SIZE: LogicalPx = crate::generated::semantic::ICON_SIZE_XS;

    /// `component.workspace-row-active-bar-width` → `{semantic.selection-edge-width}` = 2px
    pub const ROW_ACTIVE_BAR_WIDTH: LogicalPx = crate::generated::semantic::SELECTION_EDGE_WIDTH;

    /// `component.workspace-row-padding-x` → `{semantic.space-sm}` = 8px
    pub const ROW_PADDING_X: LogicalPx = crate::generated::semantic::SPACE_SM;
}
