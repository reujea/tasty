//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.
//! 재생성: `cargo run -p tasty-design-tokens --bin generate`.
//!
//! Tier 1 — primitive 치수 스케일. **`pub(crate)`**: "UI 는 primitive 를 직접
//! 읽지 않는다"(3-tier 계약)를 visibility 로 컴파일 타임 강제한다.
//! 외부 crate 는 `semantic` / `component` 를 경유할 것.
#![allow(dead_code)] // 스케일 전체를 보존한다 — 미참조 엔트리 포함.

use tasty_type_geometry::length::LogicalPx;

/// `primitive.duration-0` = 0ms (ms)
pub(crate) const DURATION_0: f32 = 0.0;

/// `primitive.duration-120` = 120ms (ms)
pub(crate) const DURATION_120: f32 = 120.0;

/// `primitive.duration-150` = 150ms (ms)
pub(crate) const DURATION_150: f32 = 150.0;

/// `primitive.duration-1600` = 1600ms (ms)
pub(crate) const DURATION_1600: f32 = 1600.0;

/// `primitive.duration-200` = 200ms (ms)
pub(crate) const DURATION_200: f32 = 200.0;

/// `primitive.duration-500` = 500ms (ms)
pub(crate) const DURATION_500: f32 = 500.0;

/// `primitive.duration-90` = 90ms (ms)
pub(crate) const DURATION_90: f32 = 90.0;

/// `primitive.duration-900` = 900ms (ms)
pub(crate) const DURATION_900: f32 = 900.0;

/// `primitive.font-size-10` = 10px
pub(crate) const FONT_SIZE_10: LogicalPx = LogicalPx(10.0);

/// `primitive.font-size-11` = 11px
pub(crate) const FONT_SIZE_11: LogicalPx = LogicalPx(11.0);

/// `primitive.font-size-12` = 12px
pub(crate) const FONT_SIZE_12: LogicalPx = LogicalPx(12.0);

/// `primitive.font-size-13` = 13px
pub(crate) const FONT_SIZE_13: LogicalPx = LogicalPx(13.0);

/// `primitive.font-size-14` = 14px
pub(crate) const FONT_SIZE_14: LogicalPx = LogicalPx(14.0);

/// `primitive.font-size-16` = 16px
pub(crate) const FONT_SIZE_16: LogicalPx = LogicalPx(16.0);

/// `primitive.font-size-17` = 17px
pub(crate) const FONT_SIZE_17: LogicalPx = LogicalPx(17.0);

/// `primitive.font-size-20` = 20px
pub(crate) const FONT_SIZE_20: LogicalPx = LogicalPx(20.0);

/// `primitive.font-size-30` = 30px
pub(crate) const FONT_SIZE_30: LogicalPx = LogicalPx(30.0);

/// `primitive.font-weight-400` = 400
pub(crate) const FONT_WEIGHT_400: u16 = 400;

/// `primitive.font-weight-500` = 500
pub(crate) const FONT_WEIGHT_500: u16 = 500;

/// `primitive.font-weight-600` = 600
pub(crate) const FONT_WEIGHT_600: u16 = 600;

/// `primitive.font-weight-700` = 700
pub(crate) const FONT_WEIGHT_700: u16 = 700;

/// `primitive.letter-spacing-0` = 0
pub(crate) const LETTER_SPACING_0: LogicalPx = LogicalPx(0.0);

/// `primitive.line-height-100` = 1.0
pub(crate) const LINE_HEIGHT_100: f32 = 1.0;

/// `primitive.line-height-120` = 1.2
pub(crate) const LINE_HEIGHT_120: f32 = 1.2;

/// `primitive.line-height-140` = 1.4
pub(crate) const LINE_HEIGHT_140: f32 = 1.4;

/// `primitive.line-height-160` = 1.6
pub(crate) const LINE_HEIGHT_160: f32 = 1.6;

/// `primitive.opacity-dimmed` = 0.75
pub(crate) const OPACITY_DIMMED: f32 = 0.75;

/// `primitive.opacity-disabled` = 0.5
pub(crate) const OPACITY_DISABLED: f32 = 0.5;

/// `primitive.opacity-recessed` = 0.4
pub(crate) const OPACITY_RECESSED: f32 = 0.4;

/// `primitive.opacity-tint-border` = 0.36
pub(crate) const OPACITY_TINT_BORDER: f32 = 0.36;

/// `primitive.opacity-tint-fill` = 0.12
pub(crate) const OPACITY_TINT_FILL: f32 = 0.12;

/// `primitive.radius-2` = 2px
pub(crate) const RADIUS_2: LogicalPx = LogicalPx(2.0);

/// `primitive.radius-4` = 4px
pub(crate) const RADIUS_4: LogicalPx = LogicalPx(4.0);

/// `primitive.radius-8` = 8px
pub(crate) const RADIUS_8: LogicalPx = LogicalPx(8.0);

/// `primitive.radius-full` = 9999px (sentinel — 완전 원형용 상한값)
pub(crate) const RADIUS_FULL: LogicalPx = LogicalPx(9999.0);

/// `primitive.scale-100` = 1
pub(crate) const SCALE_100: f32 = 1.0;

/// `primitive.scale-120` = 1.2
pub(crate) const SCALE_120: f32 = 1.2;

/// `primitive.scale-80` = 0.8
pub(crate) const SCALE_80: f32 = 0.8;

/// `primitive.size-0` = 0
pub(crate) const SIZE_0: LogicalPx = LogicalPx(0.0);

/// `primitive.size-1` = 1px
pub(crate) const SIZE_1: LogicalPx = LogicalPx(1.0);

/// `primitive.size-110` = 110px
pub(crate) const SIZE_110: LogicalPx = LogicalPx(110.0);

/// `primitive.size-112` = 112px
pub(crate) const SIZE_112: LogicalPx = LogicalPx(112.0);

/// `primitive.size-12` = 12px
pub(crate) const SIZE_12: LogicalPx = LogicalPx(12.0);

/// `primitive.size-120` = 120px
pub(crate) const SIZE_120: LogicalPx = LogicalPx(120.0);

/// `primitive.size-14` = 14px
pub(crate) const SIZE_14: LogicalPx = LogicalPx(14.0);

/// `primitive.size-150` = 150px
pub(crate) const SIZE_150: LogicalPx = LogicalPx(150.0);

/// `primitive.size-16` = 16px
pub(crate) const SIZE_16: LogicalPx = LogicalPx(16.0);

/// `primitive.size-160` = 160px
pub(crate) const SIZE_160: LogicalPx = LogicalPx(160.0);

/// `primitive.size-168` = 168px
pub(crate) const SIZE_168: LogicalPx = LogicalPx(168.0);

/// `primitive.size-180` = 180px
pub(crate) const SIZE_180: LogicalPx = LogicalPx(180.0);

/// `primitive.size-196` = 196px
pub(crate) const SIZE_196: LogicalPx = LogicalPx(196.0);

/// `primitive.size-2` = 2px
pub(crate) const SIZE_2: LogicalPx = LogicalPx(2.0);

/// `primitive.size-200` = 200px
pub(crate) const SIZE_200: LogicalPx = LogicalPx(200.0);

/// `primitive.size-22` = 22px
pub(crate) const SIZE_22: LogicalPx = LogicalPx(22.0);

/// `primitive.size-220` = 220px
pub(crate) const SIZE_220: LogicalPx = LogicalPx(220.0);

/// `primitive.size-24` = 24px
pub(crate) const SIZE_24: LogicalPx = LogicalPx(24.0);

/// `primitive.size-240` = 240px
pub(crate) const SIZE_240: LogicalPx = LogicalPx(240.0);

/// `primitive.size-28` = 28px
pub(crate) const SIZE_28: LogicalPx = LogicalPx(28.0);

/// `primitive.size-288` = 288px
pub(crate) const SIZE_288: LogicalPx = LogicalPx(288.0);

/// `primitive.size-3` = 3px
pub(crate) const SIZE_3: LogicalPx = LogicalPx(3.0);

/// `primitive.size-300` = 300px
pub(crate) const SIZE_300: LogicalPx = LogicalPx(300.0);

/// `primitive.size-32` = 32px
pub(crate) const SIZE_32: LogicalPx = LogicalPx(32.0);

/// `primitive.size-320` = 320px
pub(crate) const SIZE_320: LogicalPx = LogicalPx(320.0);

/// `primitive.size-36` = 36px
pub(crate) const SIZE_36: LogicalPx = LogicalPx(36.0);

/// `primitive.size-4` = 4px
pub(crate) const SIZE_4: LogicalPx = LogicalPx(4.0);

/// `primitive.size-400` = 400px
pub(crate) const SIZE_400: LogicalPx = LogicalPx(400.0);

/// `primitive.size-44` = 44px
pub(crate) const SIZE_44: LogicalPx = LogicalPx(44.0);

/// `primitive.size-46` = 46px
pub(crate) const SIZE_46: LogicalPx = LogicalPx(46.0);

/// `primitive.size-460` = 460px
pub(crate) const SIZE_460: LogicalPx = LogicalPx(460.0);

/// `primitive.size-48` = 48px
pub(crate) const SIZE_48: LogicalPx = LogicalPx(48.0);

/// `primitive.size-52` = 52px
pub(crate) const SIZE_52: LogicalPx = LogicalPx(52.0);

/// `primitive.size-56` = 56px
pub(crate) const SIZE_56: LogicalPx = LogicalPx(56.0);

/// `primitive.size-560` = 560px
pub(crate) const SIZE_560: LogicalPx = LogicalPx(560.0);

/// `primitive.size-6` = 6px
pub(crate) const SIZE_6: LogicalPx = LogicalPx(6.0);

/// `primitive.size-600` = 600px
pub(crate) const SIZE_600: LogicalPx = LogicalPx(600.0);

/// `primitive.size-64` = 64px
pub(crate) const SIZE_64: LogicalPx = LogicalPx(64.0);

/// `primitive.size-8` = 8px
pub(crate) const SIZE_8: LogicalPx = LogicalPx(8.0);

/// `primitive.size-88` = 88px
pub(crate) const SIZE_88: LogicalPx = LogicalPx(88.0);

/// `primitive.size-90` = 90px
pub(crate) const SIZE_90: LogicalPx = LogicalPx(90.0);

/// `primitive.size-96` = 96px
pub(crate) const SIZE_96: LogicalPx = LogicalPx(96.0);
