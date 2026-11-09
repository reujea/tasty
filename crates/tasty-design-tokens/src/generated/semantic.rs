//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! 테마에 따라 바뀌지 않는 semantic 치수·글꼴·시간 상수.
//! 치수 상수는 SIZING 초깃값과 대조 시험에 쓴다. 위젯은 Theme 필드나 접근자로
//! 치수를 읽어 UI 배율과 반올림 정책을 따른다. 색은 런타임 Theme에서 읽는다.

use tasty_type_geometry::length::LogicalPx;

/// `semantic.attention-rank-completion` = 10
pub const ATTENTION_RANK_COMPLETION: f32 = 10.0;

/// `semantic.attention-rank-needs-input` = 30
pub const ATTENTION_RANK_NEEDS_INPUT: f32 = 30.0;

/// `semantic.border-dash` → `{primitive.size-4}` = 4px
pub const BORDER_DASH: LogicalPx = super::primitive::SIZE_4;

/// `semantic.border-dash-gap` → `{primitive.size-4}` = 4px
pub const BORDER_DASH_GAP: LogicalPx = super::primitive::SIZE_4;

/// `semantic.border-width` → `{primitive.size-1}` = 1px
pub const BORDER_WIDTH: LogicalPx = super::primitive::SIZE_1;

/// `semantic.control-height` → `{primitive.size-28}` = 28px
pub const CONTROL_HEIGHT: LogicalPx = super::primitive::SIZE_28;

/// `semantic.control-height-tab` → `{primitive.size-24}` = 24px
pub const CONTROL_HEIGHT_TAB: LogicalPx = super::primitive::SIZE_24;

/// `semantic.control-height-tree` → `{primitive.size-22}` = 22px
pub const CONTROL_HEIGHT_TREE: LogicalPx = super::primitive::SIZE_22;

/// `semantic.field-width-color` → `{primitive.size-110}` = 110px
pub const FIELD_WIDTH_COLOR: LogicalPx = super::primitive::SIZE_110;

/// `semantic.field-width-lg` → `{primitive.size-200}` = 200px
pub const FIELD_WIDTH_LG: LogicalPx = super::primitive::SIZE_200;

/// `semantic.field-width-md` → `{primitive.size-160}` = 160px
pub const FIELD_WIDTH_MD: LogicalPx = super::primitive::SIZE_160;

/// `semantic.field-width-range` → `{primitive.size-180}` = 180px
pub const FIELD_WIDTH_RANGE: LogicalPx = super::primitive::SIZE_180;

/// `semantic.field-width-xs` → `{primitive.size-90}` = 90px
pub const FIELD_WIDTH_XS: LogicalPx = super::primitive::SIZE_90;

/// `semantic.focus-ring-width` → `{primitive.size-2}` = 2px
pub const FOCUS_RING_WIDTH: LogicalPx = super::primitive::SIZE_2;

/// `semantic.font-size-body` → `{primitive.font-size-13}` = 13px
pub const FONT_SIZE_BODY: LogicalPx = super::primitive::FONT_SIZE_13;

/// `semantic.font-size-brand-display` → `{primitive.font-size-30}` = 30px
pub const FONT_SIZE_BRAND_DISPLAY: LogicalPx = super::primitive::FONT_SIZE_30;

/// `semantic.font-size-brand-lockup` → `{primitive.font-size-38}` = 38px
pub const FONT_SIZE_BRAND_LOCKUP: LogicalPx = super::primitive::FONT_SIZE_38;

/// `semantic.font-size-brand-wordmark` → `{primitive.font-size-17}` = 17px
pub const FONT_SIZE_BRAND_WORDMARK: LogicalPx = super::primitive::FONT_SIZE_17;

/// `semantic.font-size-caption` → `{primitive.font-size-11}` = 11px
pub const FONT_SIZE_CAPTION: LogicalPx = super::primitive::FONT_SIZE_11;

/// `semantic.font-size-heading` → `{primitive.font-size-13}` = 13px
pub const FONT_SIZE_HEADING: LogicalPx = super::primitive::FONT_SIZE_13;

/// `semantic.font-size-max` → `{primitive.font-size-14}` = 14px
pub const FONT_SIZE_MAX: LogicalPx = super::primitive::FONT_SIZE_14;

/// `semantic.font-size-micro` → `{primitive.font-size-10}` = 10px
pub const FONT_SIZE_MICRO: LogicalPx = super::primitive::FONT_SIZE_10;

/// `semantic.font-size-prose-h1` → `{primitive.font-size-20}` = 20px
pub const FONT_SIZE_PROSE_H1: LogicalPx = super::primitive::FONT_SIZE_20;

/// `semantic.font-size-term` → `{primitive.font-size-14}` = 14px
pub const FONT_SIZE_TERM: LogicalPx = super::primitive::FONT_SIZE_14;

/// `semantic.font-size-term-lg` → `{primitive.font-size-16}` = 16px
pub const FONT_SIZE_TERM_LG: LogicalPx = super::primitive::FONT_SIZE_16;

/// `semantic.font-size-term-sm` → `{primitive.font-size-12}` = 12px
pub const FONT_SIZE_TERM_SM: LogicalPx = super::primitive::FONT_SIZE_12;

/// `semantic.font-weight-bold` → `{primitive.font-weight-700}` = 700
pub const FONT_WEIGHT_BOLD: u16 = super::primitive::FONT_WEIGHT_700;

/// `semantic.font-weight-medium` → `{primitive.font-weight-500}` = 500
pub const FONT_WEIGHT_MEDIUM: u16 = super::primitive::FONT_WEIGHT_500;

/// `semantic.font-weight-normal` → `{primitive.font-weight-400}` = 400
pub const FONT_WEIGHT_NORMAL: u16 = super::primitive::FONT_WEIGHT_400;

/// `semantic.font-weight-semibold` → `{primitive.font-weight-600}` = 600
pub const FONT_WEIGHT_SEMIBOLD: u16 = super::primitive::FONT_WEIGHT_600;

/// `semantic.icon-size-lg` → `{primitive.size-24}` = 24px
pub const ICON_SIZE_LG: LogicalPx = super::primitive::SIZE_24;

/// `semantic.icon-size-md` → `{primitive.size-16}` = 16px
pub const ICON_SIZE_MD: LogicalPx = super::primitive::SIZE_16;

/// `semantic.icon-size-sm` → `{primitive.size-14}` = 14px
pub const ICON_SIZE_SM: LogicalPx = super::primitive::SIZE_14;

/// `semantic.icon-size-xs` → `{primitive.size-12}` = 12px
pub const ICON_SIZE_XS: LogicalPx = super::primitive::SIZE_12;

/// `semantic.label-detail-gap` → `{primitive.size-2}` = 2px
pub const LABEL_DETAIL_GAP: LogicalPx = super::primitive::SIZE_2;

/// `semantic.letter-spacing-ui` → `{primitive.letter-spacing-0}` = 0
pub const LETTER_SPACING_UI: LogicalPx = super::primitive::LETTER_SPACING_0;

/// `semantic.line-height-term` → `{primitive.line-height-120}` = 1.2
pub const LINE_HEIGHT_TERM: f32 = super::primitive::LINE_HEIGHT_120;

/// `semantic.line-height-tight` → `{primitive.line-height-100}` = 1.0
pub const LINE_HEIGHT_TIGHT: f32 = super::primitive::LINE_HEIGHT_100;

/// `semantic.line-height-ui` → `{primitive.line-height-140}` = 1.4
pub const LINE_HEIGHT_UI: f32 = super::primitive::LINE_HEIGHT_140;

/// `semantic.measure-lg` → `{primitive.size-460}` = 460px
pub const MEASURE_LG: LogicalPx = super::primitive::SIZE_460;

/// `semantic.measure-md` → `{primitive.size-400}` = 400px
pub const MEASURE_MD: LogicalPx = super::primitive::SIZE_400;

/// `semantic.measure-sm` → `{primitive.size-300}` = 300px
pub const MEASURE_SM: LogicalPx = super::primitive::SIZE_300;

/// `semantic.measure-xl` → `{primitive.size-560}` = 560px
pub const MEASURE_XL: LogicalPx = super::primitive::SIZE_560;

/// `semantic.motion-hold-reveal` → `{primitive.duration-500}` = 500ms (ms)
pub const MOTION_HOLD_REVEAL: f32 = super::primitive::DURATION_500;

/// `semantic.motion-term` → `{primitive.duration-0}` = 0ms (ms)
pub const MOTION_TERM: f32 = super::primitive::DURATION_0;

/// `semantic.motion-ui` → `{primitive.duration-120}` = 120ms (ms)
pub const MOTION_UI: f32 = super::primitive::DURATION_120;

/// `semantic.motion-ui-fade` → `{primitive.duration-200}` = 200ms (ms)
pub const MOTION_UI_FADE: f32 = super::primitive::DURATION_200;

/// `semantic.motion-ui-fast` → `{primitive.duration-90}` = 90ms (ms)
pub const MOTION_UI_FAST: f32 = super::primitive::DURATION_90;

/// `semantic.motion-ui-med` → `{primitive.duration-150}` = 150ms (ms)
pub const MOTION_UI_MED: f32 = super::primitive::DURATION_150;

/// `semantic.node-dim-opacity` → `{primitive.opacity-dimmed}` = 0.75
pub const NODE_DIM_OPACITY: f32 = super::primitive::OPACITY_DIMMED;

/// `semantic.overlay-top-offset` → `{primitive.size-88}` = 88px
pub const OVERLAY_TOP_OFFSET: LogicalPx = super::primitive::SIZE_88;

/// `semantic.radius` → `{primitive.radius-4}` = 4px
pub const RADIUS: LogicalPx = super::primitive::RADIUS_4;

/// `semantic.radius-none` → `{primitive.radius-0}` = 0px
pub const RADIUS_NONE: LogicalPx = super::primitive::RADIUS_0;

/// `semantic.radius-pill` → `{primitive.radius-full}` = 9999px (sentinel — 완전 원형용 상한값)
pub const RADIUS_PILL: LogicalPx = super::primitive::RADIUS_FULL;

/// `semantic.radius-sm` → `{primitive.radius-2}` = 2px
pub const RADIUS_SM: LogicalPx = super::primitive::RADIUS_2;

/// `semantic.selection-edge-width` → `{primitive.size-2}` = 2px
pub const SELECTION_EDGE_WIDTH: LogicalPx = super::primitive::SIZE_2;

/// `semantic.space-lg` → `{primitive.size-16}` = 16px
pub const SPACE_LG: LogicalPx = super::primitive::SIZE_16;

/// `semantic.space-md` → `{primitive.size-12}` = 12px
pub const SPACE_MD: LogicalPx = super::primitive::SIZE_12;

/// `semantic.space-sm` → `{primitive.size-8}` = 8px
pub const SPACE_SM: LogicalPx = super::primitive::SIZE_8;

/// `semantic.space-xl` → `{primitive.size-24}` = 24px
pub const SPACE_XL: LogicalPx = super::primitive::SIZE_24;

/// `semantic.space-xs` → `{primitive.size-4}` = 4px
pub const SPACE_XS: LogicalPx = super::primitive::SIZE_4;

/// `semantic.state-dim-opacity` → `{primitive.opacity-disabled}` = 0.5
pub const STATE_DIM_OPACITY: f32 = super::primitive::OPACITY_DISABLED;

/// `semantic.state-disabled-opacity` → `{primitive.opacity-disabled}` = 0.5
pub const STATE_DISABLED_OPACITY: f32 = super::primitive::OPACITY_DISABLED;

/// `semantic.status-bar-height` → `{primitive.size-24}` = 24px
pub const STATUS_BAR_HEIGHT: LogicalPx = super::primitive::SIZE_24;

/// `semantic.tab-width` → `{primitive.size-150}` = 150px
pub const TAB_WIDTH: LogicalPx = super::primitive::SIZE_150;

/// `semantic.tint-border-alpha` → `{primitive.opacity-tint-border}` = 0.36
pub const TINT_BORDER_ALPHA: f32 = super::primitive::OPACITY_TINT_BORDER;

/// `semantic.tint-edge-width` → `{primitive.size-2}` = 2px
pub const TINT_EDGE_WIDTH: LogicalPx = super::primitive::SIZE_2;

/// `semantic.tint-fill-alpha` → `{primitive.opacity-tint-fill}` = 0.12
pub const TINT_FILL_ALPHA: f32 = super::primitive::OPACITY_TINT_FILL;

/// `semantic.titlebar-height` → `{primitive.size-36}` = 36px
pub const TITLEBAR_HEIGHT: LogicalPx = super::primitive::SIZE_36;

/// `semantic.toolbar-height` → `{primitive.size-32}` = 32px
pub const TOOLBAR_HEIGHT: LogicalPx = super::primitive::SIZE_32;

/// `semantic.ui-scale` → `{semantic.ui-scale-md}` = 1
pub const UI_SCALE: f32 = UI_SCALE_MD;

/// `semantic.ui-scale-lg` → `{primitive.scale-120}` = 1.2
pub const UI_SCALE_LG: f32 = super::primitive::SCALE_120;

/// `semantic.ui-scale-md` → `{primitive.scale-100}` = 1
pub const UI_SCALE_MD: f32 = super::primitive::SCALE_100;

/// `semantic.ui-scale-sm` → `{primitive.scale-80}` = 0.8
pub const UI_SCALE_SM: f32 = super::primitive::SCALE_80;
