//! Theme schema — UI 시각 표현의 데이터 모델.
//!
//! ```text
//! tasty-themes ──▶ resolve(settings)/mutate ──▶ Theme 인스턴스
//!                                                 │
//!                                                 ▼
//!                                          UI: theme().X
//! ```
//!
//! 이 모듈은 **데이터 구조** 만 정의한다. partial 누적·TOML 로딩·전역 RwLock·
//! 빌트인 mocha fallback 같은 mutation/IO 로직은 `tasty-themes` 가 담당하고,
//! 그 결과를 `Theme` 인스턴스로 빌드하여 themes 의 전역 슬롯에 박아 넣는다.
//!
//! `Theme` 은 평평한 단일 구조체로, UI 코드가 `theme().crust` /
//! `theme().spacing_sm` / `theme().is_light` 처럼 한 단계로 접근한다.
//! 색상 직렬화·partial 표현은 `ThemeColors` / `PartialColors` 에서 분리.

use crate::color::{GpuRgb, HexColor};
use crate::motion::Millis;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tasty_type_geometry::length::LogicalPx;

// ============================================================================
//  SurfaceTheme — surface kind 별 focused/unfocused 색 묶음
// ============================================================================

/// 한 surface 종류의 색 묶음. `Theme.surface_themes` 안에 `id -> SurfaceTheme` 으로 보관.
///
/// terminal 특유의 selection / search_match 색은 여기 들지 않는다 — Theme 의 top-level
/// 필드에 남아있고, 다른 surface 가 그 기능을 가질 때 sub-struct 로 흡수한다.
///
/// plugin 이 자기 surface kind 를 등록하면 그 id 로 SurfaceTheme 을 추가할 수 있다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceTheme {
    pub focused_bg: HexColor,
    pub focused_fg: HexColor,
    pub unfocused_bg: HexColor,
    pub unfocused_fg: HexColor,
}

impl Default for SurfaceTheme {
    fn default() -> Self {
        FALLBACK_SURFACE.clone()
    }
}

/// `SurfaceTheme` 의 모든 필드를 `Option` 으로 감싼 partial. theme TOML 의 `[surfaces.<id>]`
/// sub-table 과 `theme_overrides` 양쪽에 쓰인다.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialSurfaceTheme {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_fg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfocused_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfocused_fg: Option<HexColor>,
}

impl PartialSurfaceTheme {
    pub fn is_empty(&self) -> bool {
        self.focused_bg.is_none()
            && self.focused_fg.is_none()
            && self.unfocused_bg.is_none()
            && self.unfocused_fg.is_none()
    }
}

impl SurfaceTheme {
    /// `Some(v)` 인 필드만 자기 자신에 덮어쓴다. (None 은 보존)
    pub fn apply_partial(&mut self, p: &PartialSurfaceTheme) {
        if let Some(v) = p.focused_bg {
            self.focused_bg = v;
        }
        if let Some(v) = p.focused_fg {
            self.focused_fg = v;
        }
        if let Some(v) = p.unfocused_bg {
            self.unfocused_bg = v;
        }
        if let Some(v) = p.unfocused_fg {
            self.unfocused_fg = v;
        }
    }
}

/// surface_themes 에 해당 id 가 없을 때 호출자가 쓸 수 있는 안전한 fallback.
/// 모든 surface 가 검은 배경 + 흰 글자로 동작한다. theme 이 정상 적용된 상태에서는
/// 절대 도달하지 않으며, 부팅 직후 / 잘못된 plugin 등록 케이스의 마지막 보루.
#[allow(clippy::disallowed_methods)] // reason: 부팅/오류 최후 보루용 고정 색 리터럴 정의
pub const FALLBACK_SURFACE: SurfaceTheme = SurfaceTheme {
    focused_bg: HexColor::from_rgb(0, 0, 0),
    focused_fg: HexColor::from_rgb(0xcd, 0xd6, 0xf4),
    unfocused_bg: HexColor::from_rgb(0x1e, 0x1e, 0x2e),
    unfocused_fg: HexColor::from_rgb(0xa6, 0xad, 0xc8),
};

// ============================================================================
//  Titlebar OS 리터럴 — 테마 불변 (mocha/latte 공통)
// ============================================================================

/// Windows 캡션 close 버튼 hover 시 시스템 red. OS 가 박아둔 리터럴이라 테마와
/// 무관하게 고정 (`--tasty-color-os-windows-close` = `#c42b1c`). primitive 1곳에만.
#[allow(clippy::disallowed_methods)] // reason: OS 고정 리터럴 색 — 테마 무관
pub const ACCENT_WINDOW_CLOSE: HexColor = HexColor::from_rgb(0xc4, 0x2b, 0x1c);

/// close 버튼 글리프 — 어두운 red 위 흰 글자라 두 테마 모두 white 고정
/// (`--tasty-text-on-window-close`).
#[allow(clippy::disallowed_methods)] // reason: OS 고정 리터럴 색 — 테마 무관
pub const TEXT_ON_WINDOW_CLOSE: HexColor = HexColor::from_rgb(0xff, 0xff, 0xff);

/// light 테마(Latte)에서 accent 위 텍스트색 — DTCG `text-on-accent` 의 Latte remap
/// 은 절대색 white(`--tasty-color-white`). vivid accent(blue 등) 위 white 대비
/// ≈4.9:1 로 4.5:1 충족. Mocha 는 `crust` 를 쓰므로 이 리터럴은 light 전용.
#[allow(clippy::disallowed_methods)] // reason: DTCG 고정 리터럴 색 — 테마 무관
pub const TEXT_ON_ACCENT_LIGHT: HexColor = HexColor::from_rgb(0xff, 0xff, 0xff);

/// macOS 신호등(traffic light) 색. OS 가 인식하는 affordance 라 사용자가 정확한
/// 시스템 red/amber/green 을 기대한다 — Catppuccin accent 가 아니다. Windows close
/// 처럼 테마 불변 OS-system 리터럴 (`--tasty-color-os-macos-*`). mocha/latte 동일값.
#[allow(clippy::disallowed_methods)] // reason: OS 고정 리터럴 색 — 테마 무관
pub const OS_MACOS_CLOSE: HexColor = HexColor::from_rgb(0xec, 0x6a, 0x5e);
/// macOS 신호등 — minimize (amber).
#[allow(clippy::disallowed_methods)] // reason: OS 고정 리터럴 색 — 테마 무관
pub const OS_MACOS_MIN: HexColor = HexColor::from_rgb(0xf4, 0xbf, 0x4f);
/// macOS 신호등 — zoom (green).
#[allow(clippy::disallowed_methods)] // reason: OS 고정 리터럴 색 — 테마 무관
pub const OS_MACOS_ZOOM: HexColor = HexColor::from_rgb(0x61, 0xc5, 0x54);

/// 워터멜론 브랜드(수박) 마크 색. OS 신호등처럼 테마 불변 브랜드 고정 리터럴
/// (`--tasty-color-melon-flesh` = `#f25d6b`, primitives.css). mocha/latte 동일값.
#[allow(clippy::disallowed_methods)] // reason: 브랜드 고정 리터럴 색 — 테마 무관
pub const BRAND_MELON_FLESH: HexColor = HexColor::from_rgb(0xf2, 0x5d, 0x6b);

/// disabled 컨트롤 공통 톤 (`--tasty-opacity-disabled` = 0.5). 모든 위젯이 이 값으로
/// 통일한다. LogicalPx 가 아닌 순수 비율이므로 별도 f32 상수.
pub const OPACITY_DISABLED: f32 = 0.5;

/// 뒤로 물러난(recessed) 요소 공통 톤 (`--tasty-opacity-recessed` = 0.4). 상위 스코프
/// 배너 뒤로 디밍되는 하위 스코프 배너가 이 값(≈60% 투명)을 쓴다. OPACITY_DISABLED
/// 와 같은 이유로 순수 비율 f32 상수.
pub const OPACITY_RECESSED: f32 = 0.4;

/// modifier-hint **Shift 단독** 홀드 표시 지연 (1200ms). Shift 는 대문자·기호 입력에 상시
/// 쓰여 스침이 잦으므로, Shift 만 눌린 경우에 한해 기본 500ms(`component.modhint-hold-delay`)
/// 대신 1.2초를 기다려 타이핑 중 오버레이가 튀는 것을 억제한다(Ctrl+Shift 등 다른 modifier 를
/// 동반한 조합은 의도적 단축키라 기본값 유지). **지연이며 모션이 아니라** reduced_motion 무관.
///
/// **이 값에는 대응 디자인 토큰이 없다** — `primitive.duration-1200` 자체가 없어서
/// 생성 경로를 탈 수 없고, 코드가 값을 발명한 자리다. 나머지 모션 값은 전부
/// `generated_component.rs` 의 생성 접근자로 옮겼고 여기만 손으로 남았다.
pub const MOTION_HOLD_REVEAL_SHIFT_MS: Millis = Millis(1200.0);

/// 떠 있는 표면의 lift 그림자 토큰. egui 비의존 순수 표현 — egui 변환은
/// `egui-compat` feature 의 [`ShadowToken::to_egui`] 가 담당한다.
///
/// 값은 둘이다([`SHADOW_POPOVER`] / [`SHADOW_MODAL`]) — 어느 표면이 어느 쪽을 쓰는지는
/// `docs/adr/0254-floating-surface-shadow-scope-rule.md` 의 SCOPE RULE 이 정한다.
/// `alpha` 는 0~255 straight 검정 알파.
///
/// # `spread` 는 음수를 담는다 (CSS `box-shadow` 네 번째 길이와 같은 의미)
/// 디자인 `--tasty-titlebar-csd-shadow`(`0 18px 50px -8px`)가 음수 spread 를 쓴다 —
/// 그 `-8px` 는 장식이 아니라 falloff 를 8px 리사이즈 엣지 밴드 안으로 묶는 기능이다.
/// 그래서 이 타입은 음수를 **표현**한다. 다만 egui 0.31 의 `epaint::Shadow::spread` 는
/// `u8` 이라 음수를 담지 못하고, [`to_egui`](ShadowToken::to_egui) 는 그 사실을 숨기지
/// 않는다(그 함수 문서 참고). 근사값을 임의로 만들지 않는 것이 디자인 지시라, 음수
/// spread 를 쓰는 토큰은 **아직 만들지 않는다**(CSD 그림자 미구현).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowToken {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    /// 그림자 확장(px). 음수는 축소 — CSS `box-shadow` 의 spread 와 같은 의미다.
    pub spread: f32,
    pub alpha: u8,
}

#[cfg(feature = "egui-compat")]
impl ShadowToken {
    /// egui epaint Shadow 로 변환. offset/blur/spread 는 px 정수로 반올림.
    ///
    /// # 음수 `spread` 는 이 경로로 갈 수 없다
    /// egui 0.31 의 `epaint::Shadow::spread` 는 `u8` 이다. `as u8` 캐스트는 음수를
    /// **조용히 0 으로 saturate** 해 디자인이 지정한 것과 다른 그림자를 그린다 —
    /// falloff 를 좁히라고 쓴 `-8px` 가 "확장 없음" 이 되어버린다. 그 조용한 왜곡을
    /// 막기 위해 음수를 캐스트에 맡기지 않고 여기서 `max(0)` 로 **명시적으로** 잘라내고
    /// debug 빌드에서는 단언으로 터뜨린다. 음수 spread 가 필요한 표면(CSD 타이틀바)은
    /// 근사값을 만드는 대신 **미구현으로 둔다** — 지원하려면 egui 의 `Shadow` 가 아니라
    /// 자체 렌더 경로가 필요하고, 그것은 이 타입의 몫이 아니다.
    pub fn to_egui(self) -> egui::epaint::Shadow {
        debug_assert!(
            self.spread >= 0.0,
            "음수 spread({})는 egui Shadow(u8)로 표현할 수 없다 — 근사하지 말고 미구현으로 두어라",
            self.spread
        );
        egui::epaint::Shadow {
            offset: [self.offset_x.round() as i8, self.offset_y.round() as i8],
            blur: self.blur.round() as u8,
            spread: self.spread.round().max(0.0) as u8,
            color: egui::Color32::from_black_alpha(self.alpha),
        }
    }
}

/// `--tasty-shadow-popover` 값(`0 6px 18px rgba(0,0,0,0.4)`). 트리거 옆에 붙어 살아
/// 있는 콘텐츠 위에 뜨는 표면(anchored + scrim-less)의 단차 — 배너·tooltip·드롭다운·
/// anchored popup 이 쓴다. 어느 표면이 어느 쪽을 쓰는지는
/// `docs/adr/0254-floating-surface-shadow-scope-rule.md` 의 SCOPE RULE.
///
/// `alpha` 는 디자인 `rgba(0,0,0,0.4)` 의 0.4 를 0~255 로 옮긴 값이다:
/// 0.4 × 255 = 102.0 → **102**. ([`SHADOW_MODAL`] 의 140 이 같은 규칙의 결과다.)
pub const SHADOW_POPOVER: ShadowToken = ShadowToken {
    offset_x: 0.0,
    offset_y: 6.0,
    blur: 18.0,
    spread: 0.0,
    alpha: 102,
};

/// `--tasty-shadow-modal` 값. **뷰포트를 점유하는** centered 표면의 단차 — popover 보다
/// **크다**. `scrim` 유무는 갈래를 가르는 술어가 아니다(이 값을 받는 표면 중 실제로
/// scrim 이 깔리는 것은 일부다). scrim 은 값을 더 크게 잡은 **근거**로만 등장한다 —
/// 바닥을 어둡게 하지만 엣지를 그리지 않아, 어두운 테마에서 어두운 모달이 어두워진
/// 바닥 위에 놓이면 1px 보더만으로는 실루엣이 사라진다.
/// 근거·대안·재검토 조건은 `docs/adr/0254-floating-surface-shadow-scope-rule.md`.
///
/// `alpha` 는 디자인 `rgba(0,0,0,0.55)` 의 0.55 를 0~255 로 옮긴 값이다:
/// 0.55 × 255 = 140.25 → 최근접 정수 **140**. ([`SHADOW_POPOVER`] 의 102 가 같은 규칙의
/// 결과다.)
pub const SHADOW_MODAL: ShadowToken = ShadowToken {
    offset_x: 0.0,
    offset_y: 20.0,
    blur: 60.0,
    spread: 0.0,
    alpha: 140,
};

/// design `--tasty-scrim-bg` — 모달/팝업 뒤 무대를 어둡게 덮는 scrim 알파. black 50%
/// straight (테마 무관 고정 검정). `Theme::scrim()` 이 이 값으로 색을 만든다.
pub const SCRIM_ALPHA: u8 = 128;

/// design `--tasty-preset-split-zone-bg` = accent-primary 22%. 프리셋 편집기 경계
/// hover-split 존 밴드 채움 알파(22%×255≈56). accent 색은 테마 가변 → 알파만 파생.
pub const PRESET_SPLIT_ZONE_BG_ALPHA: u8 = 56;

/// design `--tasty-preset-split-zone-border` = accent-primary 55%. split 존 안쪽 변의
/// 2px 분할선 알파(55%×255≈140).
pub const PRESET_SPLIT_ZONE_BORDER_ALPHA: u8 = 140;

/// `color-mix(in srgb, <accent> 45%, transparent)` 의 알파(45%×255≈115). DAG surface
/// 의 경고/위험 pill 테두리 3 종이 공유한다 — 두 번째 항이 `transparent` 인 합성은
/// 색을 섞지 않고 알파만 낮추는 것과 같다.
pub const DAG_MIX_45_ALPHA: u8 = 115;

/// design `plugins_window.jsx` `PluginAvatar` 보더 `color-mix(in srgb, C 38%, transparent)`
/// 의 알파(38%×255≈97). 두 번째 항이 `transparent` 라 색은 그대로 두고 알파만 낮춘다
/// ([`DAG_MIX_45_ALPHA`] 와 같은 형태).
pub const PLUGIN_AVATAR_BORDER_ALPHA: u8 = 97;

/// design `--tasty-tint-fill-alpha` → `--tasty-opacity-tint-fill`. "accent 로 옅게
/// 채우고 같은 accent 로 테두리를 두르는" 관용구의 **채움** 계수. ADR-0290 이
/// 흩어져 있던 네 짝(0.14/0.45 · 0.12/0.35 · 0.11/0.36 · 0.12/—)을 이 한 짝으로
/// 모았다. 알파가 아니라 계수라 `u8` 이 아닌 `f32` 다 — 소비처가
/// `gamma_multiply()` / `mix_srgb()` 의 비율 자리에 그대로 넣는다.
pub const TINT_FILL_ALPHA: f32 = 0.12;

/// design `--tasty-tint-border-alpha` → `--tasty-opacity-tint-border`. 위 짝의
/// **테두리** 계수([`TINT_FILL_ALPHA`]).
pub const TINT_BORDER_ALPHA: f32 = 0.36;

/// `color-mix(in srgb, <a> <ratio>, <b>)` 의 srgb 채널 보간.
///
/// CSS 의 `color-mix` 는 두 색이 모두 불투명할 때 채널을 선형 보간한다. 디자인 토큰이
/// 쓰는 형태가 정확히 그 케이스(두 항 모두 테마 색)라 같은 계산을 옮긴다. 결과 알파는
/// 배경(`b`)의 알파를 따른다 — wash 는 그 위에 무엇도 비치지 않는 채움색이다.
#[allow(clippy::disallowed_methods)] // reason: 디자인 토큰 color-mix 식의 유일한 구현부
fn mix_srgb(a: HexColor, ratio: f32, b: HexColor) -> HexColor {
    let t = ratio.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
    HexColor::from_rgba(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()), b.a())
}

// ============================================================================
//  ThemeSizing — 모든 테마 공통
// ============================================================================

/// UI 크기/간격. 모든 테마에서 공통. Theme 인스턴스에도 동일 값이 복사된다.
#[derive(Debug, Clone, Copy)]
pub struct ThemeSizing {
    /// 서브-caption micro-label (kbd / badge / tag / tree·menu meta) — 10px.
    pub font_size_micro: LogicalPx,
    pub font_size_caption: LogicalPx,
    pub font_size_body: LogicalPx,
    pub font_size_heading: LogicalPx,
    pub font_size_max: LogicalPx,
    /// 브랜딩 전용 디스플레이 크기 (30px, design `--tasty-font-size-brand-display`).
    /// UI 14px 상한의 **두 번째이자 마지막 예외** — 첫 실행 셸 설정 카드의 "Tasty"
    /// 브랜드 타이틀 하나만 쓴다(wordmark 17 이 첫 예외).
    pub font_size_brand_display: LogicalPx,
    /// markdown surface heading 앵커 — egui_commonmark 이 `Heading`↔`Body` 사이를 보간하는
    /// 헤딩 사다리의 최상단(H1). 렌더 CONTENT 라 UI 14px 상한 예외 (20px). per-H2 픽셀 토큰
    /// (`prose-h2`)·본문 leading 배수(`line-height-prose`)는 라이브러리가 소유해 은퇴됨.
    pub font_size_prose_h1: LogicalPx,
    /// UI 텍스트(툴팁 등 여러 줄 chrome 문단) 줄간격 배수 (1.4, design
    /// `--tasty-line-height-ui`). prose(1.6)보다 촘촘한 UI 전용 배수 — 무차원 비율이라
    /// `f32`. 폰트 크기에 곱해 줄 높이를 만든다.
    pub line_height_ui: f32,
    /// terminal cell 스케일 — small (12px).
    pub font_size_term_sm: LogicalPx,
    /// terminal cell 스케일 — 기본 (14px).
    pub font_size_term: LogicalPx,
    /// terminal cell 스케일 — large (16px).
    pub font_size_term_lg: LogicalPx,
    pub border_width: LogicalPx,
    /// 대상을 **감싸 지목하는 링**의 두께 (2px). 키보드 포커스(egui
    /// `selection.stroke`)가 원래 용도지만, 우클릭/드롭 대상 표시·튜토리얼 마커·
    /// 선택 카드 테두리처럼 "이것" 을 가리키는 링 전반이 같은 굵기를 쓴다. 색은
    /// 별개 축이라 `accent_success` 등과 조합해도 이 토큰이다.
    ///
    /// **한쪽 변에 붙는 띠(활성 행 좌측 바·탭 밑줄)는 이 토큰이 아니다** —
    /// `tab_indicator_width` 다. 값은 같은 2 지만 이쪽만 `zoomed()` 를 탄다.
    pub focus_ring_width: LogicalPx,
    /// painter 로 직접 전사한 chrome 글리프(popup 타이틀바의 close X · 전체화면
    /// 브래킷)의 선 굵기. SVG 아이콘은 `Icon::image` 가 24 viewBox·2px stroke 를
    /// 스케일해 주지만, `Ui` 가 없어 `Painter::line_segment` 로 같은 형상을 그려야
    /// 하는 구간은 굵기를 직접 정해야 한다. `border_width`(1) 와
    /// `focus_ring_width`(2) 사이의 hairline 이고 DTCG dim 토큰에 대응이 없다
    /// (`icon_glyph_size_row_action` 과 같은 부류).
    pub icon_stroke_width: LogicalPx,
    pub corner_radius: LogicalPx,
    /// 작은 inner element(키캡 등)용 코너 반경 (2px, design `--tasty-radius-sm`).
    pub corner_radius_sm: LogicalPx,
    /// 떠 있는 패널(배너)용 큰 코너 반경 (8px, design `--tasty-radius-8`). 시스템
    /// 기본 4px 의 의도적 2배 — CSD 윈도우 코너 / floating 패널 느낌. 배너 셸이 사용.
    pub corner_radius_lg: LogicalPx,
    pub item_height_tree: LogicalPx,
    pub item_height_interactive: LogicalPx,
    pub item_height_tab: LogicalPx,
    pub tab_width: LogicalPx,
    pub spacing_xs: LogicalPx,
    pub spacing_sm: LogicalPx,
    pub spacing_md: LogicalPx,
    pub spacing_lg: LogicalPx,
    pub spacing_xl: LogicalPx,
    // ── IconButton 글리프 (정사각 프레임 = `item_height_tab`, 코너 = `corner_radius`) ──
    /// 인라인 글리프 (chevron / close 등) 최소 글리프 크기 (12px, design `--tasty-icon-size-xs`).
    pub icon_glyph_size_xs: LogicalPx,
    /// IconButton `sm` 안의 SVG 글리프 크기 (search bar nav/toggle).
    pub icon_glyph_size_sm: LogicalPx,
    /// IconButton `md` 안의 SVG 글리프 크기 (sidebar tools/plugins/settings 등).
    pub icon_glyph_size_md: LogicalPx,
    /// 목록 행 우측 액션 아이콘(가져오기 / 편집 / 삭제 / 재감지 / reveal) 글리프 크기.
    /// `sm`(14) 과 `md`(16) 사이라 DTCG dim 토큰에 대응이 없다 — `corner_radius_lg` /
    /// `line_height_ui` 처럼 토큰 없이 `Theme` 에만 사는 치수다. 평범한 `const` 가
    /// 아니라 여기 두는 이유는 zoom 이다: `const` 는 `with_colors_and_zoom` 의 배율을
    /// 타지 못해 같은 팝업 안에서 헤더 아이콘만 커지고 행 아이콘은 고정된다.
    pub icon_glyph_size_row_action: LogicalPx,
    // ── Sidebar 전용 (host UI zoom 영향 받음) ──
    /// Full sidebar 헤더의 수박 로고 크기.
    pub sidebar_logo_size: LogicalPx,
    /// Collapsed sidebar 헤더의 수박 로고 크기.
    pub sidebar_logo_collapsed_size: LogicalPx,
    /// `tasty.` 워드마크 mono 폰트 크기.
    pub sidebar_wordmark_font_size: LogicalPx,
    /// "WORKSPACES" 섹션 헤딩 mono 폰트 크기.
    pub sidebar_section_heading_font_size: LogicalPx,
    /// Tools / Plugins / Settings / New workspace 등 사이드바 ghost block 버튼 라벨 폰트 크기.
    pub sidebar_button_label_font_size: LogicalPx,
    /// Collapsed sidebar icon / workspace 슬롯의 공통 너비.
    pub sidebar_collapsed_slot_width: LogicalPx,
    /// Collapsed sidebar Tools / Plugins / Settings / Expand 아이콘 슬롯 높이.
    pub sidebar_collapsed_icon_height: LogicalPx,
    /// Collapsed sidebar workspace 슬롯 높이.
    pub sidebar_collapsed_workspace_height: LogicalPx,
    // ── Tab bar 전용 (host UI zoom 영향 받지 않음) ──
    // 사용자 제약 — 탭바는 host UI zoom 제외. with_colors_and_zoom 에서 SIZING 그대로
    // 복사 (border_width / tab_width 와 동일 처리).
    /// 탭바 자체 높이.
    pub tab_bar_height: LogicalPx,
    /// "+" 새 탭 버튼 폰트 크기.
    pub tab_bar_label_font_size: LogicalPx,
    /// 좌/우 스크롤 화살표 폰트 크기.
    pub tab_bar_arrow_font_size: LogicalPx,
    // ── 작업영역 하단 StatusBar 전용 (host UI zoom 영향 받지 않음) ──
    // tab_bar 와 동일하게 px 고정 — with_colors_and_zoom 에서 SIZING 그대로 복사.
    /// 작업영역 하단 StatusBar 높이.
    pub status_bar_height: LogicalPx,
    // ── Titlebar (CSD) 전용 (host UI zoom 영향 받지 않음) ──
    // 디자인 jsx 가 px 고정이고, OS 데코 관습상 고정 px 가 맞다. tab_bar 와 동일하게
    // with_colors_and_zoom 에서 SIZING 그대로 복사 (zoom 미적용).
    /// CSD 타이틀바 높이.
    pub titlebar_height: LogicalPx,
    /// macOS 신호등(traffic light) 점 지름.
    pub traffic_size: LogicalPx,
    /// Windows 캡션 버튼(min·max·close) 폭.
    pub caption_width: LogicalPx,
    /// Linux DE 버튼(min·max·close) 원형 지름.
    pub window_button_size: LogicalPx,
    // ── 가독 폭 (Note / content / 모달 컬럼용) ──
    /// 좁은 가독 폭 (300px).
    pub measure_sm: LogicalPx,
    /// 중간 가독 폭 (400px).
    pub measure_md: LogicalPx,
    /// 넓은 가독 폭 (460px).
    pub measure_lg: LogicalPx,
    /// 최대 가독 폭 (560px).
    pub measure_xl: LogicalPx,
    // ── form-control 폭 ──
    /// 극소 입력 폭 (90px).
    pub field_width_xs: LogicalPx,
    /// 색상 입력 폭 (110px).
    pub field_width_color: LogicalPx,
    /// 중간 입력 폭 (160px).
    pub field_width_md: LogicalPx,
    /// 넓은 입력 폭 (200px).
    pub field_width_lg: LogicalPx,
    // ── 세부 치수 ──
    /// 토스트 최대 폭 (320px).
    pub toast_max_width: LogicalPx,
    /// 툴팁 버블 최대 폭 (240px, design `--tasty-tooltip-max-width` = `--tasty-size-240`).
    /// 초과 시 텍스트 줄바꿈. host UI content 라 zoom 적용.
    pub tooltip_max_width: LogicalPx,
    /// 상태 점(status dot) 지름 (8px).
    pub status_dot_size: LogicalPx,
    /// 스피너 지름 (16px).
    pub spinner_size: LogicalPx,
    /// 토스트 좌측 accent 바 두께 (3px).
    pub toast_accent_width: LogicalPx,
    /// 탭 active indicator 두께 (2px). 대상을 감싸지 않고 **한쪽 변에 붙는 띠**
    /// 전반 — 탭 밑줄, 활성 행의 좌측 accent 바. 감싸는 링은
    /// `focus_ring_width`(같은 2 지만 그쪽만 `zoomed()` 를 탄다).
    pub tab_indicator_width: LogicalPx,
    /// 상단 정렬 모달(command palette) 상단 gap (88px).
    pub overlay_top_offset: LogicalPx,
}

pub const SIZING: ThemeSizing = ThemeSizing {
    font_size_micro: LogicalPx(10.0),
    font_size_caption: LogicalPx(11.0),
    font_size_body: LogicalPx(13.0),
    font_size_heading: LogicalPx(13.0), // semibold 로 구분, 크기는 같음
    font_size_max: LogicalPx(14.0),
    font_size_brand_display: LogicalPx(30.0),
    font_size_prose_h1: LogicalPx(20.0),
    line_height_ui: 1.4,
    font_size_term_sm: LogicalPx(12.0),
    font_size_term: LogicalPx(14.0),
    font_size_term_lg: LogicalPx(16.0),
    border_width: LogicalPx(1.0),
    focus_ring_width: LogicalPx(2.0),
    icon_stroke_width: LogicalPx(1.5),
    corner_radius: LogicalPx(4.0),
    corner_radius_sm: LogicalPx(2.0),
    corner_radius_lg: LogicalPx(8.0),
    item_height_tree: LogicalPx(22.0),
    item_height_interactive: LogicalPx(28.0),
    item_height_tab: LogicalPx(24.0),
    tab_width: LogicalPx(150.0),
    spacing_xs: LogicalPx(4.0),
    spacing_sm: LogicalPx(8.0),
    spacing_md: LogicalPx(12.0),
    spacing_lg: LogicalPx(16.0),
    spacing_xl: LogicalPx(24.0),
    icon_glyph_size_xs: LogicalPx(12.0),
    icon_glyph_size_sm: LogicalPx(14.0),
    icon_glyph_size_md: LogicalPx(16.0),
    icon_glyph_size_row_action: LogicalPx(15.0),
    sidebar_logo_size: LogicalPx(22.0),
    sidebar_logo_collapsed_size: LogicalPx(24.0),
    sidebar_wordmark_font_size: LogicalPx(17.0),
    sidebar_section_heading_font_size: LogicalPx(10.0),
    // 디자인 판정 (2026-07-02 token-coverage): UI 타입 스케일은 10/11/13/14 고정,
    // 12 는 터미널 전용 — `font-size-caption`(11) 으로 스냅.
    sidebar_button_label_font_size: LogicalPx(11.0),
    sidebar_collapsed_slot_width: LogicalPx(32.0),
    sidebar_collapsed_icon_height: LogicalPx(22.0),
    sidebar_collapsed_workspace_height: LogicalPx(28.0),
    tab_bar_height: LogicalPx(24.0),
    tab_bar_label_font_size: LogicalPx(13.0),
    tab_bar_arrow_font_size: LogicalPx(11.0),
    status_bar_height: LogicalPx(24.0),
    titlebar_height: LogicalPx(36.0),
    traffic_size: LogicalPx(12.0),
    caption_width: LogicalPx(46.0),
    window_button_size: LogicalPx(24.0),
    measure_sm: LogicalPx(300.0),
    measure_md: LogicalPx(400.0),
    measure_lg: LogicalPx(460.0),
    measure_xl: LogicalPx(560.0),
    field_width_xs: LogicalPx(90.0),
    field_width_color: LogicalPx(110.0),
    field_width_md: LogicalPx(160.0),
    field_width_lg: LogicalPx(200.0),
    toast_max_width: LogicalPx(320.0),
    tooltip_max_width: LogicalPx(240.0),
    status_dot_size: LogicalPx(8.0),
    spinner_size: LogicalPx(16.0),
    toast_accent_width: LogicalPx(3.0),
    tab_indicator_width: LogicalPx(2.0),
    overlay_top_offset: LogicalPx(88.0),
};

// ============================================================================
//  ThemeColors / PartialColors — 직렬화 표현
// ============================================================================

/// 테마 색상 풀 세트. 직렬화 가능 — `AppearanceSettings.theme_base` 가 이걸 저장.
///
/// `hover_overlay` / `active_overlay` / `separator` 같은 반투명 의미 색은 여기 없다.
/// 그건 `is_light` 에서 자동 도출되므로 `Theme` 인스턴스에서만 보유한다.
///
/// 모든 색 필드가 `HexColor`. ansi 는 hex 로 통일했고, GPU 셰이더에 넘길 때만
/// `.to_float()` 한다. surface 종류별(focused/unfocused × bg/fg) 색은
/// `surface_themes` map 안에 `SurfaceTheme` 으로 담는다 — plugin 이 자기 id 로
/// 추가 가능.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeColors {
    // ── Surfaces (low → high elevation) ──
    pub crust: HexColor,
    pub mantle: HexColor,
    pub base: HexColor,
    pub surface0: HexColor,
    pub surface1: HexColor,
    pub surface2: HexColor,

    // ── Overlays ──
    pub overlay0: HexColor,
    pub overlay1: HexColor,
    pub overlay2: HexColor,

    // ── Text ──
    pub text: HexColor,
    pub subtext1: HexColor,
    pub subtext0: HexColor,
    pub placeholder: HexColor,

    // ── Accent ──
    pub blue: HexColor,
    pub green: HexColor,
    pub red: HexColor,
    pub yellow: HexColor,
    pub peach: HexColor,
    pub mauve: HexColor,
    pub teal: HexColor,
    pub sky: HexColor,
    pub lavender: HexColor,
    pub flamingo: HexColor,
    pub pink: HexColor,
    pub maroon: HexColor,
    pub rosewater: HexColor,

    // ── Terminal-specific 색 (다른 surface 가 selection 기능을 가지면 SurfaceTheme 로 흡수) ──
    pub selection_bg: HexColor,
    /// vi copy mode 의 cursor cell highlight. selection_bg 와 시각적으로 구분되어야 한다.
    pub vi_cursor_bg: HexColor,
    pub search_match_bg: HexColor,
    pub search_match_active_bg: HexColor,

    // ── ANSI 16 ──
    pub ansi_black: HexColor,
    pub ansi_red: HexColor,
    pub ansi_green: HexColor,
    pub ansi_yellow: HexColor,
    pub ansi_blue: HexColor,
    pub ansi_magenta: HexColor,
    pub ansi_cyan: HexColor,
    pub ansi_white: HexColor,
    pub ansi_bright_black: HexColor,
    pub ansi_bright_red: HexColor,
    pub ansi_bright_green: HexColor,
    pub ansi_bright_yellow: HexColor,
    pub ansi_bright_blue: HexColor,
    pub ansi_bright_magenta: HexColor,
    pub ansi_bright_cyan: HexColor,
    pub ansi_bright_white: HexColor,

    // ── surface kind 별 색 묶음 ──
    /// surface kind id ("terminal", "markdown", plugin id 등) → `SurfaceTheme`.
    /// theme TOML 의 `[surfaces.<id>]` sub-table 에서 정의. apply_partial 시
    /// entry 단위 merge.
    #[serde(default)]
    pub surface_themes: BTreeMap<String, SurfaceTheme>,
}

/// `ThemeColors` 의 모든 필드를 `Option<HexColor>` 로 감싼 표현.
///
/// - 사용자가 settings UI 픽커로 손댄 흔적(`AppearanceSettings.theme_overrides`)
/// - 외부 TOML 의 partial 테마 정의 (`ThemeFile` 에서 변환)
///
/// `ThemeColors::apply_partial()` 로 `Some` 필드만 base 에 덮어쓴다.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crust: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mantle: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface0: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface1: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface2: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay0: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay1: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay2: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtext1: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtext0: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blue: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yellow: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peach: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mauve: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teal: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sky: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lavender: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flamingo: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pink: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maroon: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rosewater: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vi_cursor_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match_active_bg: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_black: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_red: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_green: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_yellow: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_blue: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_magenta: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_cyan: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_white: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_black: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_red: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_green: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_yellow: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_blue: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_magenta: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_cyan: Option<HexColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_bright_white: Option<HexColor>,

    /// surface kind 별 partial 색. entry 단위로 base 의 `SurfaceTheme` 위에 덮어쓴다.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub surface_themes: BTreeMap<String, PartialSurfaceTheme>,
}

impl PartialColors {
    /// 모든 필드를 None 으로 리셋.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// 단일 None 인지 (= 사용자 흔적 없음).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl ThemeColors {
    /// `Some(v)` 인 필드만 자기 자신에 덮어쓴다. (None 은 보존)
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 평면 per-필드 Option 병합 — 색 필드 수만큼 동일 if-let 반복, 중첩 없음(clippy 과대계상)
    pub fn apply_partial(&mut self, p: &PartialColors) {
        if let Some(v) = p.crust {
            self.crust = v;
        }
        if let Some(v) = p.mantle {
            self.mantle = v;
        }
        if let Some(v) = p.base {
            self.base = v;
        }
        if let Some(v) = p.surface0 {
            self.surface0 = v;
        }
        if let Some(v) = p.surface1 {
            self.surface1 = v;
        }
        if let Some(v) = p.surface2 {
            self.surface2 = v;
        }
        if let Some(v) = p.overlay0 {
            self.overlay0 = v;
        }
        if let Some(v) = p.overlay1 {
            self.overlay1 = v;
        }
        if let Some(v) = p.overlay2 {
            self.overlay2 = v;
        }
        if let Some(v) = p.text {
            self.text = v;
        }
        if let Some(v) = p.subtext1 {
            self.subtext1 = v;
        }
        if let Some(v) = p.subtext0 {
            self.subtext0 = v;
        }
        if let Some(v) = p.placeholder {
            self.placeholder = v;
        }
        if let Some(v) = p.blue {
            self.blue = v;
        }
        if let Some(v) = p.green {
            self.green = v;
        }
        if let Some(v) = p.red {
            self.red = v;
        }
        if let Some(v) = p.yellow {
            self.yellow = v;
        }
        if let Some(v) = p.peach {
            self.peach = v;
        }
        if let Some(v) = p.mauve {
            self.mauve = v;
        }
        if let Some(v) = p.teal {
            self.teal = v;
        }
        if let Some(v) = p.sky {
            self.sky = v;
        }
        if let Some(v) = p.lavender {
            self.lavender = v;
        }
        if let Some(v) = p.flamingo {
            self.flamingo = v;
        }
        if let Some(v) = p.pink {
            self.pink = v;
        }
        if let Some(v) = p.maroon {
            self.maroon = v;
        }
        if let Some(v) = p.rosewater {
            self.rosewater = v;
        }
        if let Some(v) = p.selection_bg {
            self.selection_bg = v;
        }
        if let Some(v) = p.vi_cursor_bg {
            self.vi_cursor_bg = v;
        }
        if let Some(v) = p.search_match_bg {
            self.search_match_bg = v;
        }
        if let Some(v) = p.search_match_active_bg {
            self.search_match_active_bg = v;
        }
        if let Some(v) = p.ansi_black {
            self.ansi_black = v;
        }
        if let Some(v) = p.ansi_red {
            self.ansi_red = v;
        }
        if let Some(v) = p.ansi_green {
            self.ansi_green = v;
        }
        if let Some(v) = p.ansi_yellow {
            self.ansi_yellow = v;
        }
        if let Some(v) = p.ansi_blue {
            self.ansi_blue = v;
        }
        if let Some(v) = p.ansi_magenta {
            self.ansi_magenta = v;
        }
        if let Some(v) = p.ansi_cyan {
            self.ansi_cyan = v;
        }
        if let Some(v) = p.ansi_white {
            self.ansi_white = v;
        }
        if let Some(v) = p.ansi_bright_black {
            self.ansi_bright_black = v;
        }
        if let Some(v) = p.ansi_bright_red {
            self.ansi_bright_red = v;
        }
        if let Some(v) = p.ansi_bright_green {
            self.ansi_bright_green = v;
        }
        if let Some(v) = p.ansi_bright_yellow {
            self.ansi_bright_yellow = v;
        }
        if let Some(v) = p.ansi_bright_blue {
            self.ansi_bright_blue = v;
        }
        if let Some(v) = p.ansi_bright_magenta {
            self.ansi_bright_magenta = v;
        }
        if let Some(v) = p.ansi_bright_cyan {
            self.ansi_bright_cyan = v;
        }
        if let Some(v) = p.ansi_bright_white {
            self.ansi_bright_white = v;
        }
        // surface_themes: entry 단위 merge. base 에 없는 id 면 default 위에 partial 적용.
        for (id, partial_st) in &p.surface_themes {
            self.surface_themes
                .entry(id.clone())
                .or_default()
                .apply_partial(partial_st);
        }
    }
}

// ============================================================================
//  Theme — 실제 적용된 인스턴스 (평평한 구조)
// ============================================================================

/// 현재 적용된 테마 인스턴스. **UI 코드는 `theme()` 으로 받아 평평하게 접근**한다
/// (예: `theme().crust`, `theme().spacing_sm`, `theme().is_light`).
///
/// `ThemeColors` 의 모든 필드를 펼쳐 담고, sizing/플래그/도출 색상을 함께 보유.
/// surface kind 별 색은 `surface_themes` map — `surface(id)` 헬퍼로 접근 권장.
#[derive(Debug, Clone)]
pub struct Theme {
    // ── ThemeColors 와 동일 필드 (펼친 형태) ──
    pub crust: HexColor,
    pub mantle: HexColor,
    pub base: HexColor,
    pub surface0: HexColor,
    pub surface1: HexColor,
    pub surface2: HexColor,
    pub overlay0: HexColor,
    pub overlay1: HexColor,
    pub overlay2: HexColor,
    pub text: HexColor,
    pub subtext1: HexColor,
    pub subtext0: HexColor,
    pub placeholder: HexColor,
    pub blue: HexColor,
    pub green: HexColor,
    pub red: HexColor,
    pub yellow: HexColor,
    pub peach: HexColor,
    pub mauve: HexColor,
    pub teal: HexColor,
    pub sky: HexColor,
    pub lavender: HexColor,
    pub flamingo: HexColor,
    pub pink: HexColor,
    pub maroon: HexColor,
    pub rosewater: HexColor,
    pub selection_bg: HexColor,
    pub vi_cursor_bg: HexColor,
    pub search_match_bg: HexColor,
    pub search_match_active_bg: HexColor,
    pub ansi_black: HexColor,
    pub ansi_red: HexColor,
    pub ansi_green: HexColor,
    pub ansi_yellow: HexColor,
    pub ansi_blue: HexColor,
    pub ansi_magenta: HexColor,
    pub ansi_cyan: HexColor,
    pub ansi_white: HexColor,
    pub ansi_bright_black: HexColor,
    pub ansi_bright_red: HexColor,
    pub ansi_bright_green: HexColor,
    pub ansi_bright_yellow: HexColor,
    pub ansi_bright_blue: HexColor,
    pub ansi_bright_magenta: HexColor,
    pub ansi_bright_cyan: HexColor,
    pub ansi_bright_white: HexColor,

    // ── `is_light` 에서 자동 도출 (premultiplied 바이트) ──
    /// 호버 시 배경 오버레이 (~8%). `to_egui_premultiplied()` 로 변환할 것.
    pub hover_overlay: HexColor,
    /// 눌림 시 배경 오버레이 (~12%). `to_egui_premultiplied()` 로 변환할 것.
    pub active_overlay: HexColor,
    /// 구분선 (~8%). `to_egui_premultiplied()` 로 변환할 것.
    pub separator: HexColor,

    // ── 모든 테마 공통 sizing (SIZING 에서 복사) ──
    /// 서브-caption micro-label (kbd / badge / tag / tree·menu meta) — 10px.
    pub font_size_micro: LogicalPx,
    pub font_size_caption: LogicalPx,
    pub font_size_body: LogicalPx,
    pub font_size_heading: LogicalPx,
    pub font_size_max: LogicalPx,
    /// 브랜딩 전용 디스플레이 크기 (30px, design `--tasty-font-size-brand-display`).
    /// UI 14px 상한의 **두 번째이자 마지막 예외** — 첫 실행 셸 설정 카드의 "Tasty"
    /// 브랜드 타이틀 하나만 쓴다(wordmark 17 이 첫 예외).
    pub font_size_brand_display: LogicalPx,
    /// markdown surface heading 앵커 — egui_commonmark 헤딩 사다리 최상단(H1). 렌더 CONTENT 라
    /// UI 14px 상한 예외 (20px). per-H2·본문 leading 은 라이브러리 소유로 은퇴됨.
    /// **zoom 제외 — 렌더 콘텐츠라 UI 배율 축 밖이다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub font_size_prose_h1: LogicalPx,
    /// UI 텍스트(툴팁 등) 줄간격 배수 (1.4, design `--tasty-line-height-ui`). 무차원 비율.
    pub line_height_ui: f32,
    /// terminal cell 스케일 — small (12px).
    /// **zoom 제외 — 터미널 콘텐츠. `effective_terminal_font` 로 GPU 셰이더에 따로 간다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub font_size_term_sm: LogicalPx,
    /// terminal cell 스케일 — 기본 (14px).
    /// **zoom 제외 — 터미널 콘텐츠. `effective_terminal_font` 로 GPU 셰이더에 따로 간다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub font_size_term: LogicalPx,
    /// terminal cell 스케일 — large (16px).
    /// **zoom 제외 — 터미널 콘텐츠. `effective_terminal_font` 로 GPU 셰이더에 따로 간다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub font_size_term_lg: LogicalPx,
    /// 기본 보더 굵기 (1px).
    /// **zoom 제외 — 1px 보더 정책. 배율을 태우면 hairline 이 아니게 된다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub border_width: LogicalPx,
    /// 대상을 **감싸 지목하는 링**의 두께 (2px). 키보드 포커스(egui
    /// `selection.stroke`)가 원래 용도지만, 우클릭/드롭 대상 표시·튜토리얼 마커·
    /// 선택 카드 테두리처럼 "이것" 을 가리키는 링 전반이 같은 굵기를 쓴다. 색은
    /// 별개 축이라 `accent_success` 등과 조합해도 이 토큰이다.
    ///
    /// **한쪽 변에 붙는 띠(활성 행 좌측 바·탭 밑줄)는 이 토큰이 아니다** —
    /// `tab_indicator_width` 다. 값은 같은 2 지만 이쪽만 `zoomed()` 를 탄다.
    pub focus_ring_width: LogicalPx,
    /// painter 로 직접 전사한 chrome 글리프(popup 타이틀바의 close X · 전체화면
    /// 브래킷)의 선 굵기. SVG 아이콘은 `Icon::image` 가 24 viewBox·2px stroke 를
    /// 스케일해 주지만, `Ui` 가 없어 `Painter::line_segment` 로 같은 형상을 그려야
    /// 하는 구간은 굵기를 직접 정해야 한다. `border_width`(1) 와
    /// `focus_ring_width`(2) 사이의 hairline 이고 DTCG dim 토큰에 대응이 없다
    /// (`icon_glyph_size_row_action` 과 같은 부류).
    /// **zoom 제외 — hairline. 이 굵기를 쓰는 타이틀바 버튼 기하가 고정 px 라 선만 굵어지면 글리프가 뭉갠다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub icon_stroke_width: LogicalPx,
    pub corner_radius: LogicalPx,
    /// 작은 inner element(키캡 등)용 코너 반경 (2px, design `--tasty-radius-sm`).
    pub corner_radius_sm: LogicalPx,
    /// 떠 있는 패널(배너)용 큰 코너 반경 (8px, design `--tasty-radius-8`).
    pub corner_radius_lg: LogicalPx,
    pub item_height_tree: LogicalPx,
    pub item_height_interactive: LogicalPx,
    pub item_height_tab: LogicalPx,
    /// 탭 하나의 기본 폭. 본체 탭바는 `AppearanceSettings.tab_width` 를 읽고, 이 필드의 소비자는 갤러리와 sub-menu 패널 폭이다.
    /// **zoom 제외 — 탭바 크롬. 컨테이너와 그 안의 폰트가 함께 고정이라 클리핑이 안 난다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub tab_width: LogicalPx,
    pub spacing_xs: LogicalPx,
    pub spacing_sm: LogicalPx,
    pub spacing_md: LogicalPx,
    pub spacing_lg: LogicalPx,
    pub spacing_xl: LogicalPx,
    // ── IconButton 글리프 (정사각 프레임 = `item_height_tab`, 코너 = `corner_radius`) ──
    /// 인라인 글리프 (chevron / close 등) 최소 글리프 크기 (12px, design `--tasty-icon-size-xs`).
    pub icon_glyph_size_xs: LogicalPx,
    /// IconButton `sm` 안의 SVG 글리프 크기 (search bar nav/toggle).
    pub icon_glyph_size_sm: LogicalPx,
    /// IconButton `md` 안의 SVG 글리프 크기 (sidebar tools/plugins/settings 등).
    pub icon_glyph_size_md: LogicalPx,
    /// 목록 행 우측 액션 아이콘(가져오기 / 편집 / 삭제 / 재감지 / reveal) 글리프 크기.
    /// `sm`(14) 과 `md`(16) 사이라 DTCG dim 토큰에 대응이 없다 — `corner_radius_lg` /
    /// `line_height_ui` 처럼 토큰 없이 `Theme` 에만 사는 치수다. 평범한 `const` 가
    /// 아니라 여기 두는 이유는 zoom 이다: `const` 는 `with_colors_and_zoom` 의 배율을
    /// 타지 못해 같은 팝업 안에서 헤더 아이콘만 커지고 행 아이콘은 고정된다.
    pub icon_glyph_size_row_action: LogicalPx,
    // ── Sidebar 전용 (host UI zoom 영향 받음) ──
    pub sidebar_logo_size: LogicalPx,
    pub sidebar_logo_collapsed_size: LogicalPx,
    pub sidebar_wordmark_font_size: LogicalPx,
    pub sidebar_section_heading_font_size: LogicalPx,
    pub sidebar_button_label_font_size: LogicalPx,
    pub sidebar_collapsed_slot_width: LogicalPx,
    pub sidebar_collapsed_icon_height: LogicalPx,
    pub sidebar_collapsed_workspace_height: LogicalPx,
    // ── Tab bar 전용 (host UI zoom 영향 받지 않음) ──
    /// 탭바 자체 높이.
    /// **zoom 제외 — 탭바 크롬(`tab_width` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub tab_bar_height: LogicalPx,
    /// 탭 라벨 폰트 크기.
    /// **zoom 제외 — 탭바 크롬(`tab_width` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub tab_bar_label_font_size: LogicalPx,
    /// 좌/우 스크롤 화살표 폰트 크기.
    /// **zoom 제외 — 탭바 크롬(`tab_width` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub tab_bar_arrow_font_size: LogicalPx,
    // ── 작업영역 하단 StatusBar 전용 (host UI zoom 영향 받지 않음) ──
    /// 작업영역 하단 StatusBar 높이.
    /// **zoom 제외 — 상태바 크롬. 컨테이너와 내용이 함께 고정이다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub status_bar_height: LogicalPx,
    // ── Titlebar (CSD) 전용 (host UI zoom 영향 받지 않음) ──
    /// CSD 타이틀바 높이.
    /// **zoom 제외 — CSD 타이틀바 크롬. OS 창 장식 기하라 배율과 독립이다.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub titlebar_height: LogicalPx,
    /// macOS 신호등(traffic light) 점 지름.
    /// **zoom 제외 — CSD 타이틀바 크롬(`titlebar_height` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub traffic_size: LogicalPx,
    /// Windows 캡션 버튼(min·max·close) 폭.
    /// **zoom 제외 — CSD 타이틀바 크롬(`titlebar_height` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub caption_width: LogicalPx,
    /// Linux DE 버튼(min·max·close) 원형 지름.
    /// **zoom 제외 — CSD 타이틀바 크롬(`titlebar_height` 와 같은 이유).** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub window_button_size: LogicalPx,
    // ── 가독 폭 (Note / content / 모달 컬럼용) ──
    pub measure_sm: LogicalPx,
    pub measure_md: LogicalPx,
    pub measure_lg: LogicalPx,
    pub measure_xl: LogicalPx,
    // ── form-control 폭 ──
    pub field_width_xs: LogicalPx,
    pub field_width_color: LogicalPx,
    pub field_width_md: LogicalPx,
    pub field_width_lg: LogicalPx,
    // ── 세부 치수 ──
    pub toast_max_width: LogicalPx,
    /// 툴팁 버블 최대 폭 (240px, design `--tasty-tooltip-max-width`).
    pub tooltip_max_width: LogicalPx,
    pub status_dot_size: LogicalPx,
    pub spinner_size: LogicalPx,
    pub toast_accent_width: LogicalPx,
    /// 한쪽 변에 붙는 띠(탭 밑줄·활성 행 좌측 accent 바). 감싸는 링은
    /// `focus_ring_width` — 값은 같은 2 지만 그쪽만 zoom 을 탄다.
    /// **zoom 제외 — hairline 띠.** 면제 집합 자체는 이 크레이트의 zoom 면제 가드가 이름 단위로 고정한다.
    pub tab_indicator_width: LogicalPx,
    pub overlay_top_offset: LogicalPx,

    /// host UI zoom 배율 (`with_colors_and_zoom` 에 전달된 값 그대로, 기본 1.0).
    /// component 접근자가 primitive 직접 alias 치수에 곱하는 용도 — 이미 `zoomed()`
    /// 로 resolve 된 필드에는 재적용하지 않는다.
    pub ui_zoom: f32,

    /// 접근성 "모션 감소" 설정(`accessibility.reduced_motion`)이 켜져 있는가.
    ///
    /// **색·치수가 아닌데 `Theme` 이 드는 이유는 위젯이 잊을 수 없게 하기 위해서다.**
    /// 종전에는 위젯이 이 값을 호출부 인자로 받았고, 그 인자를 실제로 넘기는 자리가
    /// 레포 전체에 0 이었다 — 설정을 켜도 스피너가 계속 돌았다. 그리는 코드는 `Theme`
    /// 없이는 그릴 수 없으므로, 여기 실으면 새 자리가 생겨도 빠뜨릴 수 없다.
    /// 결정과 대안은 ADR-0174.
    ///
    /// 기본은 `false` — `with_colors*` 로 직접 만든 `Theme`(테스트·갤러리·plugin 프로세스)
    /// 은 이 설정을 모른다. host 는 전역 설치 경로가 값을 실어 나른다.
    pub reduced_motion: bool,

    // ── 라이트/다크 플래그 ──
    pub is_light: bool,

    // ── surface kind 별 색 묶음 ──
    /// `"terminal"`, `"markdown"`, 또는 plugin 등록 id → `SurfaceTheme`.
    /// 호출자는 보통 [`Theme::surface`] 헬퍼를 통해 접근 (없는 id 는 `FALLBACK_SURFACE`).
    pub surface_themes: BTreeMap<String, SurfaceTheme>,
}

/// `is_light` 에 따른 hover/active/separator 도출.
/// premultiplied sRGB 바이트로 저장 — 변환 시 `to_egui_premultiplied()` 사용.
#[allow(clippy::disallowed_methods)] // reason: 도출 overlay 색 정의 본거지
const fn derive_overlays(is_light: bool) -> (HexColor, HexColor, HexColor) {
    if is_light {
        (
            HexColor::from_rgba(0, 0, 0, 20), // black ~8%
            HexColor::from_rgba(0, 0, 0, 31), // black ~12%
            HexColor::from_rgba(0, 0, 0, 20), // black ~8%
        )
    } else {
        (
            HexColor::from_rgba(20, 20, 20, 20), // white-ish ~8%
            HexColor::from_rgba(31, 31, 31, 31), // white-ish ~12%
            HexColor::from_rgba(20, 20, 20, 20), // white-ish ~8%
        )
    }
}

impl Theme {
    /// `ThemeColors` + `is_light` 로 풀 인스턴스 빌드. SIZING 은 const 에서 복사.
    /// `surface_themes` 가 `BTreeMap` 이라 더 이상 `const fn` 일 수 없다.
    pub fn with_colors(c: ThemeColors, is_light: bool) -> Self {
        Self::with_colors_and_zoom(c, is_light, 1.0)
    }

    /// `with_colors` 의 일반화 — host UI zoom 배율을 sizing token 자체에 곱해서
    /// 박는다. `ui_zoom == 1.0` 이면 `with_colors` 와 결과 동일.
    ///
    /// 미적용 토큰 (디자인 정책상 zoom 영향 받지 않음):
    /// - `border_width` (1px 보더는 zoom 무관)
    /// - `tab_width` (탭바는 host UI zoom 제외)
    pub fn with_colors_and_zoom(c: ThemeColors, is_light: bool, ui_zoom: f32) -> Self {
        let (hover_overlay, active_overlay, separator) = derive_overlays(is_light);
        let zoomed = |px: LogicalPx| LogicalPx((px.value() * ui_zoom).round());
        Self {
            crust: c.crust,
            mantle: c.mantle,
            base: c.base,
            surface0: c.surface0,
            surface1: c.surface1,
            surface2: c.surface2,
            overlay0: c.overlay0,
            overlay1: c.overlay1,
            overlay2: c.overlay2,
            text: c.text,
            subtext1: c.subtext1,
            subtext0: c.subtext0,
            placeholder: c.placeholder,
            blue: c.blue,
            green: c.green,
            red: c.red,
            yellow: c.yellow,
            peach: c.peach,
            mauve: c.mauve,
            teal: c.teal,
            sky: c.sky,
            lavender: c.lavender,
            flamingo: c.flamingo,
            pink: c.pink,
            maroon: c.maroon,
            rosewater: c.rosewater,
            selection_bg: c.selection_bg,
            vi_cursor_bg: c.vi_cursor_bg,
            search_match_bg: c.search_match_bg,
            search_match_active_bg: c.search_match_active_bg,
            ansi_black: c.ansi_black,
            ansi_red: c.ansi_red,
            ansi_green: c.ansi_green,
            ansi_yellow: c.ansi_yellow,
            ansi_blue: c.ansi_blue,
            ansi_magenta: c.ansi_magenta,
            ansi_cyan: c.ansi_cyan,
            ansi_white: c.ansi_white,
            ansi_bright_black: c.ansi_bright_black,
            ansi_bright_red: c.ansi_bright_red,
            ansi_bright_green: c.ansi_bright_green,
            ansi_bright_yellow: c.ansi_bright_yellow,
            ansi_bright_blue: c.ansi_bright_blue,
            ansi_bright_magenta: c.ansi_bright_magenta,
            ansi_bright_cyan: c.ansi_bright_cyan,
            ansi_bright_white: c.ansi_bright_white,
            hover_overlay,
            active_overlay,
            separator,
            font_size_micro: zoomed(SIZING.font_size_micro),
            font_size_caption: zoomed(SIZING.font_size_caption),
            font_size_body: zoomed(SIZING.font_size_body),
            font_size_heading: zoomed(SIZING.font_size_heading),
            font_size_max: zoomed(SIZING.font_size_max),
            font_size_brand_display: zoomed(SIZING.font_size_brand_display),
            // prose / term 스케일 = surface CONTENT 폰트 (markdown/terminal). UI zoom
            // 영향 받지 않는다 — 터미널/마크다운 셀 폰트는 자체 설정 경로를 따른다.
            font_size_prose_h1: SIZING.font_size_prose_h1,
            // 줄간격 배수 — 무차원 비율, zoom 무관 (폰트 크기 자체가 스케일을 담당).
            line_height_ui: SIZING.line_height_ui,
            font_size_term_sm: SIZING.font_size_term_sm,
            font_size_term: SIZING.font_size_term,
            font_size_term_lg: SIZING.font_size_term_lg,
            border_width: SIZING.border_width,
            focus_ring_width: zoomed(SIZING.focus_ring_width),
            // `border_width` 와 같이 zoom 을 타지 않는다 — 이 굵기를 쓰는 타이틀바
            // 버튼 기하가 고정 px 라 선만 굵어지면 글리프가 뭉갠다.
            icon_stroke_width: SIZING.icon_stroke_width,
            corner_radius: zoomed(SIZING.corner_radius),
            corner_radius_sm: zoomed(SIZING.corner_radius_sm),
            corner_radius_lg: zoomed(SIZING.corner_radius_lg),
            item_height_tree: zoomed(SIZING.item_height_tree),
            item_height_interactive: zoomed(SIZING.item_height_interactive),
            item_height_tab: zoomed(SIZING.item_height_tab),
            tab_width: SIZING.tab_width,
            spacing_xs: zoomed(SIZING.spacing_xs),
            spacing_sm: zoomed(SIZING.spacing_sm),
            spacing_md: zoomed(SIZING.spacing_md),
            spacing_lg: zoomed(SIZING.spacing_lg),
            spacing_xl: zoomed(SIZING.spacing_xl),
            icon_glyph_size_xs: zoomed(SIZING.icon_glyph_size_xs),
            icon_glyph_size_sm: zoomed(SIZING.icon_glyph_size_sm),
            icon_glyph_size_md: zoomed(SIZING.icon_glyph_size_md),
            icon_glyph_size_row_action: zoomed(SIZING.icon_glyph_size_row_action),
            sidebar_logo_size: zoomed(SIZING.sidebar_logo_size),
            sidebar_logo_collapsed_size: zoomed(SIZING.sidebar_logo_collapsed_size),
            sidebar_wordmark_font_size: zoomed(SIZING.sidebar_wordmark_font_size),
            sidebar_section_heading_font_size: zoomed(SIZING.sidebar_section_heading_font_size),
            sidebar_button_label_font_size: zoomed(SIZING.sidebar_button_label_font_size),
            sidebar_collapsed_slot_width: zoomed(SIZING.sidebar_collapsed_slot_width),
            sidebar_collapsed_icon_height: zoomed(SIZING.sidebar_collapsed_icon_height),
            sidebar_collapsed_workspace_height: zoomed(SIZING.sidebar_collapsed_workspace_height),
            // Tab bar 전용 — 사용자 제약 "탭바 zoom 제외" 에 따라 SIZING 그대로 (zoom 미적용).
            tab_bar_height: SIZING.tab_bar_height,
            tab_bar_label_font_size: SIZING.tab_bar_label_font_size,
            tab_bar_arrow_font_size: SIZING.tab_bar_arrow_font_size,
            // 작업영역 하단 StatusBar — tab_bar 와 동일하게 zoom 미적용.
            status_bar_height: SIZING.status_bar_height,
            // Titlebar (CSD) 전용 — px 고정 디자인, tab_bar 와 동일하게 zoom 미적용.
            titlebar_height: SIZING.titlebar_height,
            traffic_size: SIZING.traffic_size,
            caption_width: SIZING.caption_width,
            window_button_size: SIZING.window_button_size,
            // 가독 폭 / form-control 폭 / 세부 치수 — host UI content, zoom 적용.
            measure_sm: zoomed(SIZING.measure_sm),
            measure_md: zoomed(SIZING.measure_md),
            measure_lg: zoomed(SIZING.measure_lg),
            measure_xl: zoomed(SIZING.measure_xl),
            field_width_xs: zoomed(SIZING.field_width_xs),
            field_width_color: zoomed(SIZING.field_width_color),
            field_width_md: zoomed(SIZING.field_width_md),
            field_width_lg: zoomed(SIZING.field_width_lg),
            toast_max_width: zoomed(SIZING.toast_max_width),
            tooltip_max_width: zoomed(SIZING.tooltip_max_width),
            status_dot_size: zoomed(SIZING.status_dot_size),
            spinner_size: zoomed(SIZING.spinner_size),
            toast_accent_width: zoomed(SIZING.toast_accent_width),
            tab_indicator_width: SIZING.tab_indicator_width,
            overlay_top_offset: zoomed(SIZING.overlay_top_offset),
            ui_zoom,
            reduced_motion: false,
            is_light,
            surface_themes: c.surface_themes,
        }
    }

    /// surface kind id 로 SurfaceTheme 조회. 없으면 [`FALLBACK_SURFACE`] 를 가리킨다.
    /// theme 이 정상 적용된 상태에서는 항상 빌트인 entry (`"terminal"`, `"markdown"`) 가
    /// 존재한다.
    pub fn surface(&self, id: &str) -> &SurfaceTheme {
        self.surface_themes.get(id).unwrap_or(&FALLBACK_SURFACE)
    }

    /// 현재 색상 스냅샷을 `ThemeColors` 로 추출.
    pub fn extract_colors(&self) -> ThemeColors {
        ThemeColors {
            crust: self.crust,
            mantle: self.mantle,
            base: self.base,
            surface0: self.surface0,
            surface1: self.surface1,
            surface2: self.surface2,
            overlay0: self.overlay0,
            overlay1: self.overlay1,
            overlay2: self.overlay2,
            text: self.text,
            subtext1: self.subtext1,
            subtext0: self.subtext0,
            placeholder: self.placeholder,
            blue: self.blue,
            green: self.green,
            red: self.red,
            yellow: self.yellow,
            peach: self.peach,
            mauve: self.mauve,
            teal: self.teal,
            sky: self.sky,
            lavender: self.lavender,
            flamingo: self.flamingo,
            pink: self.pink,
            maroon: self.maroon,
            rosewater: self.rosewater,
            selection_bg: self.selection_bg,
            vi_cursor_bg: self.vi_cursor_bg,
            search_match_bg: self.search_match_bg,
            search_match_active_bg: self.search_match_active_bg,
            ansi_black: self.ansi_black,
            ansi_red: self.ansi_red,
            ansi_green: self.ansi_green,
            ansi_yellow: self.ansi_yellow,
            ansi_blue: self.ansi_blue,
            ansi_magenta: self.ansi_magenta,
            ansi_cyan: self.ansi_cyan,
            ansi_white: self.ansi_white,
            ansi_bright_black: self.ansi_bright_black,
            ansi_bright_red: self.ansi_bright_red,
            ansi_bright_green: self.ansi_bright_green,
            ansi_bright_yellow: self.ansi_bright_yellow,
            ansi_bright_blue: self.ansi_bright_blue,
            ansi_bright_magenta: self.ansi_bright_magenta,
            ansi_bright_cyan: self.ansi_bright_cyan,
            ansi_bright_white: self.ansi_bright_white,
            surface_themes: self.surface_themes.clone(),
        }
    }

    /// `ThemeColors` 의 색상 필드만 자신에게 덮어쓴다 (sizing / is_light / 도출 색상은 보존).
    /// is_light 변경이 필요하면 `set_is_light()` 도 호출할 것.
    pub fn apply_colors(&mut self, c: &ThemeColors) {
        let next = Self::with_colors(c.clone(), self.is_light);
        // 색상 + 도출 overlay 만 갱신 (is_light/sizing 은 보존되지만 next 와 동일).
        *self = next;
    }

    /// is_light 플래그를 바꾸고 hover/active/separator 를 재도출.
    pub fn set_is_light(&mut self, is_light: bool) {
        let (h, a, s) = derive_overlays(is_light);
        self.is_light = is_light;
        self.hover_overlay = h;
        self.active_overlay = a;
        self.separator = s;
    }

    /// 평면 색 필드를 다시 [`ThemeColors`] 로 모은다 (`apply_colors`/`with_colors`
    /// 의 역방향). resolved Theme 을 (zoom 독립적인) 색 집합으로 직렬화해 프로세스
    /// 경계 너머로 보내고 [`Theme::with_colors_and_zoom`] 으로 재구성할 때 쓴다
    /// (egui-mesh plugin 의 Theme parity — ADR-0028).
    pub fn to_colors(&self) -> ThemeColors {
        ThemeColors {
            crust: self.crust,
            mantle: self.mantle,
            base: self.base,
            surface0: self.surface0,
            surface1: self.surface1,
            surface2: self.surface2,
            overlay0: self.overlay0,
            overlay1: self.overlay1,
            overlay2: self.overlay2,
            text: self.text,
            subtext1: self.subtext1,
            subtext0: self.subtext0,
            placeholder: self.placeholder,
            blue: self.blue,
            green: self.green,
            red: self.red,
            yellow: self.yellow,
            peach: self.peach,
            mauve: self.mauve,
            teal: self.teal,
            sky: self.sky,
            lavender: self.lavender,
            flamingo: self.flamingo,
            pink: self.pink,
            maroon: self.maroon,
            rosewater: self.rosewater,
            selection_bg: self.selection_bg,
            vi_cursor_bg: self.vi_cursor_bg,
            search_match_bg: self.search_match_bg,
            search_match_active_bg: self.search_match_active_bg,
            ansi_black: self.ansi_black,
            ansi_red: self.ansi_red,
            ansi_green: self.ansi_green,
            ansi_yellow: self.ansi_yellow,
            ansi_blue: self.ansi_blue,
            ansi_magenta: self.ansi_magenta,
            ansi_cyan: self.ansi_cyan,
            ansi_white: self.ansi_white,
            ansi_bright_black: self.ansi_bright_black,
            ansi_bright_red: self.ansi_bright_red,
            ansi_bright_green: self.ansi_bright_green,
            ansi_bright_yellow: self.ansi_bright_yellow,
            ansi_bright_blue: self.ansi_bright_blue,
            ansi_bright_magenta: self.ansi_bright_magenta,
            ansi_bright_cyan: self.ansi_bright_cyan,
            ansi_bright_white: self.ansi_bright_white,
            surface_themes: self.surface_themes.clone(),
        }
    }

    /// GPU 렌더러용 ANSI 16색 팔레트.
    /// 인덱스 순서: black, red, green, yellow, blue, magenta, cyan, white,
    /// bright_black, bright_red, bright_green, bright_yellow, bright_blue,
    /// bright_magenta, bright_cyan, bright_white.
    pub fn ansi_palette(&self) -> [GpuRgb; 16] {
        [
            self.ansi_black.to_gpu_rgb(),
            self.ansi_red.to_gpu_rgb(),
            self.ansi_green.to_gpu_rgb(),
            self.ansi_yellow.to_gpu_rgb(),
            self.ansi_blue.to_gpu_rgb(),
            self.ansi_magenta.to_gpu_rgb(),
            self.ansi_cyan.to_gpu_rgb(),
            self.ansi_white.to_gpu_rgb(),
            self.ansi_bright_black.to_gpu_rgb(),
            self.ansi_bright_red.to_gpu_rgb(),
            self.ansi_bright_green.to_gpu_rgb(),
            self.ansi_bright_yellow.to_gpu_rgb(),
            self.ansi_bright_blue.to_gpu_rgb(),
            self.ansi_bright_magenta.to_gpu_rgb(),
            self.ansi_bright_cyan.to_gpu_rgb(),
            self.ansi_bright_white.to_gpu_rgb(),
        ]
    }
}

// ============================================================================
//  Semantic 접근자 — A1 token-crosswalk 의 의미 라벨 기준
// ============================================================================
//
// `Theme` 은 평면 primitive 필드를 노출하고, 의미(accent-primary 등) 매핑을 semantic
// 접근자로 끌어올린다. bg-*/surface-*/text-*(placeholder 까지)/accent-*/border-* 의
// **단순 primitive 필드 alias** 접근자는 `semantic_color_generated.rs` 로 이관됐다
// (DTCG semantic 색 토큰에서 생성 — SSoT 는 디자인 파일). 아래 `impl Theme` 에는
// codegen 불가라 수기로 남는 접근자만 둔다: is_light 분기(text-on-accent), derive_overlays
// 도출(overlay-*), 합성색(scrim), OS/brand 리터럴, component tier 조합(titlebar/banner) 등.
//
// 매핑 근거: `docs/design/systems/design-token-mapping.md#rust-필드--호출처-토큰-크로스워크` (semantic ↔ primitive ↔ 필드).
// 같은 primitive 가 여러 role 로 갈리는 다의성(crosswalk §4)은 호출처가 어느 접근자를
// 쓰는지로 표현된다 (예: blue → `accent_primary` / `border_focus` / ansi-blue).
impl Theme {
    /// accent 위 텍스트색 (DTCG `text-on-accent`). 테마별 role-remap: Mocha(dark)
    /// 는 neutral-0(=`crust`), Latte(light)는 절대색 white. vivid accent 위 대비
    /// (4.5:1) 를 양 테마에서 충족시키기 위한 분기 — `is_light` 로 식별.
    #[inline]
    pub fn text_on_accent(&self) -> HexColor {
        if self.is_light {
            TEXT_ON_ACCENT_LIGHT
        } else {
            self.crust
        }
    }

    // ── 오버레이 (overlay-*) — is_light 에서 도출된 필드를 semantic 이름으로 ──
    #[inline]
    pub fn overlay_hover(&self) -> HexColor {
        self.hover_overlay
    }
    #[inline]
    pub fn overlay_active(&self) -> HexColor {
        self.active_overlay
    }

    /// tinted 채움/테두리 관용구의 채움 계수. design `--tasty-tint-fill-alpha`
    /// ([`TINT_FILL_ALPHA`]). zoom 과 무관한 비율이라 배율을 타지 않는다.
    #[inline]
    pub fn tint_fill_alpha(&self) -> f32 {
        TINT_FILL_ALPHA
    }

    /// 같은 관용구의 테두리 계수. design `--tasty-tint-border-alpha`
    /// ([`TINT_BORDER_ALPHA`]).
    #[inline]
    pub fn tint_border_alpha(&self) -> f32 {
        TINT_BORDER_ALPHA
    }

    /// 모달/팝업 뒤 무대를 덮는 scrim 색. design `--tasty-scrim-bg`(black 50%) — 테마
    /// 무관 고정 검정 + [`SCRIM_ALPHA`]. 갤러리 dialog 레시피와 동일 토큰.
    #[inline]
    #[allow(clippy::disallowed_methods)] // reason: 테마 무관 고정 scrim 색 정의
    pub fn scrim(&self) -> HexColor {
        HexColor::from_rgba(0, 0, 0, SCRIM_ALPHA)
    }

    /// 프리셋 편집기 경계 hover-split 존의 밴드 채움색. design
    /// `--tasty-preset-split-zone-bg` = accent-primary 22% — accent 색(테마 가변)은
    /// 유지하고 알파만 파생한다([`PRESET_SPLIT_ZONE_BG_ALPHA`]). split-zone overlay 는
    /// 향후 drag-drop drop-zone 과 토큰을 공유할 의도로 명명(계획 §rationale).
    #[inline]
    pub fn preset_split_zone_bg(&self) -> HexColor {
        self.accent_primary().with_alpha(PRESET_SPLIT_ZONE_BG_ALPHA)
    }

    /// split 존의 안쪽 변에 그리는 2px 분할선 색. design
    /// `--tasty-preset-split-zone-border` = accent-primary 55%
    /// ([`PRESET_SPLIT_ZONE_BORDER_ALPHA`]).
    #[inline]
    pub fn preset_split_zone_border(&self) -> HexColor {
        self.accent_primary()
            .with_alpha(PRESET_SPLIT_ZONE_BORDER_ALPHA)
    }

    // ── DAG surface — color-mix 합성색 9종 ────────────────────────────────────
    //
    // `component.dag-*` 토큰 89 종은 생성기가 `Theme` 접근자로 뽑아 두었지만
    // (`generated_component.rs`), 값이 `color-mix(in srgb, …)` 인 9 종은 참조가
    // 아니라 *식* 이라 생성기가 건너뛴다. 여기서 같은 식을 [`mix_srgb`] 로 그대로
    // 옮긴다 — 원본 식은 `crates/tasty-design-tokens/dtcg/tasty.tokens.json` 의
    // 대응 `component.dag-*` 항목이다(vendor 재동기화 시 이 9 개도 함께 확인).
    //
    // `…-border` 3 종은 두 번째 항이 `transparent` 다. srgb 합성에서 그건 "그 비율
    // 만큼의 알파" 와 같으므로 알파만 파생한다(색 자체는 accent 유지 — 테마 가변).

    /// `component.dag-status-running-bg` = accent-primary 16% + surface-raised.
    #[inline]
    pub fn dag_status_running_bg(&self) -> HexColor {
        mix_srgb(self.accent_primary(), 0.16, self.surface_raised())
    }

    /// `component.dag-status-failed-bg` = accent-danger 12% + surface-raised.
    #[inline]
    pub fn dag_status_failed_bg(&self) -> HexColor {
        mix_srgb(self.accent_danger(), 0.12, self.surface_raised())
    }

    /// `component.dag-status-unknown-bg` = accent-warning 10% + surface-raised.
    #[inline]
    pub fn dag_status_unknown_bg(&self) -> HexColor {
        mix_srgb(self.accent_warning(), 0.10, self.surface_raised())
    }

    /// `component.dag-cycle-bg` = accent-warning 14% + bg-panel.
    #[inline]
    pub fn dag_cycle_bg(&self) -> HexColor {
        mix_srgb(self.accent_warning(), 0.14, self.bg_panel())
    }

    /// `component.dag-cycle-border` = accent-warning 45% + transparent.
    #[inline]
    pub fn dag_cycle_border(&self) -> HexColor {
        self.accent_warning().with_alpha(DAG_MIX_45_ALPHA)
    }

    /// `component.dag-runner-crashed-bg` = accent-danger 12% + surface-raised.
    #[inline]
    pub fn dag_runner_crashed_bg(&self) -> HexColor {
        mix_srgb(self.accent_danger(), 0.12, self.surface_raised())
    }

    /// `component.dag-runner-crashed-border` = accent-danger 45% + transparent.
    #[inline]
    pub fn dag_runner_crashed_border(&self) -> HexColor {
        self.accent_danger().with_alpha(DAG_MIX_45_ALPHA)
    }

    /// `component.dag-runner-stalled-bg` = accent-warning 10% + surface-raised.
    #[inline]
    pub fn dag_runner_stalled_bg(&self) -> HexColor {
        mix_srgb(self.accent_warning(), 0.10, self.surface_raised())
    }

    /// `component.dag-runner-stalled-border` = accent-warning 45% + transparent.
    #[inline]
    pub fn dag_runner_stalled_border(&self) -> HexColor {
        self.accent_warning().with_alpha(DAG_MIX_45_ALPHA)
    }

    // ── PluginAvatar — 디자인의 카테고리색 합성 2 종 ──────────────────────────
    //
    // 디자인(`plugins_window.jsx`)은 두 항을 `CAT_COLOR[plugin.cat]` 로 섞고, 그 표에
    // 없으면 `var(--tasty-accent-primary)` 로 떨어진다. **tasty 매니페스트에는 카테고리
    // 필드가 없다**(`crates/tasty-plugin-manifest/src/types.rs` 의 `Manifest`) — 그래서
    // 도달 가능한 갈래가 그 fallback 하나뿐이고, 여기서는 `accent_primary` 를 고정으로
    // 적는다. 카테고리가 생기면 두 함수에 색 인자를 받게 하는 것이 바뀌는 전부다.

    /// design `PluginAvatar` 배경 = 카테고리색 18% + surface-raised.
    #[inline]
    pub fn plugin_avatar_bg(&self) -> HexColor {
        mix_srgb(self.accent_primary(), 0.18, self.surface_raised())
    }

    /// design `PluginAvatar` 보더 = 카테고리색 38% + transparent
    /// ([`PLUGIN_AVATAR_BORDER_ALPHA`]).
    #[inline]
    pub fn plugin_avatar_border(&self) -> HexColor {
        self.accent_primary().with_alpha(PLUGIN_AVATAR_BORDER_ALPHA)
    }

    /// 프리셋 편집기 leaf 미리보기 값 요약의 라벨(소문자 필드 키) 색. design
    /// `--tasty-preset-leaf-label-fg` → `text-muted`.
    #[inline]
    pub fn preset_leaf_label_fg(&self) -> HexColor {
        self.text_muted()
    }

    /// 프리셋 편집기 leaf 미리보기 값 요약의 값 색. design
    /// `--tasty-preset-leaf-value-fg` → `text-secondary`.
    #[inline]
    pub fn preset_leaf_value_fg(&self) -> HexColor {
        self.text_secondary()
    }

    /// 프리셋 편집기 surface 설정 화면이 열려 있는 동안 왼쪽 preset 리스트와 L1 scope
    /// 탭을 흐리게 하는 opacity. design `--tasty-preset-cfg-dim-opacity` →
    /// `state-disabled-opacity`. number 토큰이라 생성기가 접근자를 만들지 않는다.
    #[inline]
    pub fn preset_cfg_dim_opacity(&self) -> f32 {
        self.opacity_disabled()
    }

    // ── Titlebar (CSD) 컴포넌트 색 — 신규 primitive 없이 기존 semantic 접근자 조합으로 구성 ──
    /// 타이틀바 배경 (active/focused). `--tasty-titlebar-bg` → `bg-app`.
    #[inline]
    pub fn titlebar_bg(&self) -> HexColor {
        self.bg_app()
    }
    /// 타이틀바 배경 (inactive/unfocused). `--tasty-titlebar-bg-inactive` → `bg-sidebar`.
    #[inline]
    pub fn titlebar_bg_inactive(&self) -> HexColor {
        self.bg_sidebar()
    }
    /// 타이틀바 하단 1px 보더. `--tasty-titlebar-border` → `separator`.
    #[inline]
    pub fn titlebar_border(&self) -> HexColor {
        self.separator
    }
    /// 타이틀바 전경 (active/focused). `--tasty-titlebar-fg` → `text-secondary`.
    #[inline]
    pub fn titlebar_fg(&self) -> HexColor {
        self.text_secondary()
    }
    /// 타이틀바 전경 (inactive/unfocused). `--tasty-titlebar-fg-inactive` → `text-muted`.
    #[inline]
    pub fn titlebar_fg_inactive(&self) -> HexColor {
        self.text_muted()
    }
    /// Windows close 버튼 hover 의 시스템 red. 테마 불변 OS 리터럴.
    #[inline]
    pub fn accent_window_close(&self) -> HexColor {
        ACCENT_WINDOW_CLOSE
    }
    /// close 버튼 글리프 — 두 테마 모두 white 고정.
    #[inline]
    pub fn text_on_window_close(&self) -> HexColor {
        TEXT_ON_WINDOW_CLOSE
    }

    /// macOS 신호등 close 색 (테마 불변 OS 리터럴). macOS 네이티브 신호등이 OS 렌더되는
    /// 경로에서는 미사용 — tasty 가 자체 신호등을 그리게 되는 경우의 색 소스.
    #[inline]
    pub fn accent_macos_close(&self) -> HexColor {
        OS_MACOS_CLOSE
    }
    /// macOS 신호등 minimize 색 (테마 불변 OS 리터럴).
    #[inline]
    pub fn accent_macos_min(&self) -> HexColor {
        OS_MACOS_MIN
    }
    /// macOS 신호등 zoom 색 (테마 불변 OS 리터럴).
    #[inline]
    pub fn accent_macos_zoom(&self) -> HexColor {
        OS_MACOS_ZOOM
    }

    /// 워터멜론 브랜드(수박) 마크 색 (테마 불변 브랜드 리터럴).
    #[inline]
    pub fn brand_melon_flesh(&self) -> HexColor {
        BRAND_MELON_FLESH
    }

    /// disabled 컨트롤 공통 opacity (0.5). 모든 위젯이 disabled 디밍에 이 값을 쓴다.
    #[inline]
    pub fn opacity_disabled(&self) -> f32 {
        OPACITY_DISABLED
    }

    /// recessed(뒤로 물러난) 요소 공통 opacity (0.4). 상위 스코프 배너 뒤로 디밍되는
    /// 하위 스코프 배너가 이 값(≈60% 투명)을 쓴다. `--tasty-opacity-recessed`.
    #[inline]
    pub fn opacity_recessed(&self) -> f32 {
        OPACITY_RECESSED
    }

    /// cut-pending(잘라내기 대기) explorer 셀 전경(아이콘+라벨) 디밍 opacity (0.5).
    /// 디자인 explorer cell-state matrix "cut (50% opacity) until paste". 디자인 토큰에
    /// cut 전용 primitive 가 없고 값이 disabled 와 동일한 0.5 이므로 같은 primitive 를
    /// semantic 으로 재사용한다(새 primitive 값 미도입 — 디자인 토큰과 drift 없음).
    #[inline]
    pub fn opacity_cut(&self) -> f32 {
        OPACITY_DISABLED
    }

    // ── 컴포넌트 토큰 (banner) — 기존 semantic 접근자 / 신규 primitive 조합 ──
    /// 배너 셸 배경. `--tasty-banner-bg` → `surface-raised` (surface0).
    #[inline]
    pub fn banner_bg(&self) -> HexColor {
        self.surface_raised()
    }
    /// 배너 셸 보더. `--tasty-banner-border` → `border-strong`.
    #[inline]
    pub fn banner_border(&self) -> HexColor {
        self.border_strong()
    }
    /// 배너 전경(본문). `--tasty-banner-fg` → `text-primary`.
    #[inline]
    pub fn banner_fg(&self) -> HexColor {
        self.text_primary()
    }
    /// 배너 leading 글리프 기본색. `--tasty-banner-icon-fg` → `text-muted`
    /// (배너별 심각도 표현이 override 가능).
    #[inline]
    pub fn banner_icon_fg(&self) -> HexColor {
        self.text_muted()
    }
    /// 배너 우상단 TTL 카운트다운 색. `--tasty-banner-countdown-fg` → `text-muted`.
    #[inline]
    pub fn banner_countdown_fg(&self) -> HexColor {
        self.text_muted()
    }
    /// 떠 있는 패널 그림자. `--tasty-banner-shadow` → `--tasty-shadow-popover`.
    #[inline]
    pub fn shadow_popover(&self) -> ShadowToken {
        SHADOW_POPOVER
    }
    /// **뷰포트를 점유하는** centered 표면(모달) 그림자. `--tasty-shadow-modal`.
    /// scrim 유무는 갈래를 가르는 술어가 아니다 — 이 값을 받는 표면 중 scrim 이
    /// 깔리는 것은 일부다.
    /// 어느 표면이 이 값을 쓰는지는 `docs/adr/0254-floating-surface-shadow-scope-rule.md`.
    #[inline]
    pub fn shadow_modal(&self) -> ShadowToken {
        SHADOW_MODAL
    }

    // ── 모션 (modifier-hint) ──
    /// modifier-hint **Shift 단독** 홀드 표시 지연 (1200ms). 타이핑 중 Shift 스침으로
    /// 오버레이가 튀는 것을 억제한다. **지연이며 모션이 아니라** reduced_motion 무관.
    ///
    /// 대응 토큰이 없어 손으로 남은 유일한 모션 값이다([`MOTION_HOLD_REVEAL_SHIFT_MS`]).
    #[inline]
    pub fn motion_hold_reveal_shift(&self) -> Millis {
        MOTION_HOLD_REVEAL_SHIFT_MS
    }

    /// 설정 창 콘텐츠 컬럼의 상한 (620px). `--tasty-settings-content-max-width`.
    ///
    /// 스크롤하는 콘텐츠 컬럼 **한 곳**에 건다 — 그러면 full-bleed 가 아닌 L2 서브탭이
    /// 전부 이것을 물려받고 블록이 저마다 폭을 들 필요가 없다. full-bleed 서브탭은
    /// 컬럼 자체를 자기 레이아웃으로 대체하므로 예외다. 본문 산문은 이것과 **다른 축**인
    /// `measure_md`(읽는 줄 길이, 이것보다 좁다)를 그대로 쓴다.
    ///
    /// 디자인 export 에 이 이름이 아직 없어 ADR-0135 대로 접근자로 든다.
    #[inline]
    pub fn settings_content_max_width(&self) -> LogicalPx {
        LogicalPx((620.0 * self.ui_zoom).round())
    }

    // ── 컴포넌트 토큰 (modifier-hint 오버레이) — `--tasty-modhint-*` ──
    // 4분류(Popup/Toast/Banner/Modal) 밖의 신규 요소: 키보드 포커스 없음 + 마우스
    // 인터랙티브 + 홀드 수명. 치수는 LogicalPx(DPI 자연대응), 색은 semantic 재사용.
    // raw px 하드코딩 금지 — 본체 draw 는 전부 이 접근자를 경유한다.
    /// 기본 너비 (180px). `--tasty-modhint-width` → `--tasty-size-180`.
    /// 열린 사이드바 폭(`AppearanceSettings.sidebar_width` 기본 180)과 정렬.
    #[inline]
    pub fn modhint_width(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }
    /// 기본 높이 (400px). `--tasty-modhint-height`. 펼친 사이드바 하단을 채우는 세로 패널.
    #[inline]
    pub fn modhint_height(&self) -> LogicalPx {
        LogicalPx((400.0 * self.ui_zoom).round())
    }
    /// 리사이즈 최소 너비 (180px = 기본 너비). `--tasty-modhint-min-width`.
    #[inline]
    pub fn modhint_min_width(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }
    /// 리사이즈 최소 높이 (240px). `--tasty-modhint-min-height`.
    #[inline]
    pub fn modhint_min_height(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }
    /// 드래그 스트립 높이 (28px). `--tasty-modhint-strip-height` → `--tasty-size-28`
    /// (= `item-height-interactive`).
    #[inline]
    pub fn modhint_strip_height(&self) -> LogicalPx {
        self.item_height_interactive
    }
    /// 스크롤 리스트 안쪽 패딩 (10px). `--tasty-modhint-pad`.
    #[inline]
    pub fn modhint_pad(&self) -> LogicalPx {
        LogicalPx((10.0 * self.ui_zoom).round())
    }
    /// 섹션 사이 간격 (12px). `--tasty-modhint-section-gap` → `--tasty-space-md`.
    #[inline]
    pub fn modhint_section_gap(&self) -> LogicalPx {
        self.spacing_md
    }
    /// 섹션 내부 행 사이 간격 (6px). `--tasty-modhint-row-gap`.
    #[inline]
    pub fn modhint_row_gap(&self) -> LogicalPx {
        LogicalPx((6.0 * self.ui_zoom).round())
    }
    /// 빈 조합 섹션의 내부 간격 (3px) — 채워진 섹션(6px)보다 좁게 잡아, 항상 표시되는
    /// 빈 섹션이 리스트를 과하게 늘어뜨리지 않게 한다. `--tasty-modhint-empty-row-gap`
    /// (디자인 `explorations/modifier-hint-empty-section.html` §6-5, `.mh-section--empty { gap: 3px }`).
    #[inline]
    pub fn modhint_empty_row_gap(&self) -> LogicalPx {
        LogicalPx((3.0 * self.ui_zoom).round())
    }
    /// 빈 조합 플레이스홀더 행의 최소 높이 (20px) — 키캡 행(24px)보다 타이트.
    /// `--tasty-modhint-empty-row-min-height` (디자인 시안 §6-5, `.mh-empty { min-height: 20px }`).
    #[inline]
    pub fn modhint_empty_row_min_height(&self) -> LogicalPx {
        LogicalPx((20.0 * self.ui_zoom).round())
    }
    /// 코너 리사이즈 그립 크기 (12px). `--tasty-modhint-grip-size` → `--tasty-icon-size-xs`.
    #[inline]
    pub fn modhint_grip_size(&self) -> LogicalPx {
        self.icon_glyph_size_xs
    }
    /// 패널 배경 (불투명 — 라이브 출력 위). `--tasty-modhint-bg` → `bg-panel`.
    #[inline]
    pub fn modhint_bg(&self) -> HexColor {
        self.bg_panel()
    }
    /// 패널 보더. `--tasty-modhint-border` → `border-strong`.
    #[inline]
    pub fn modhint_border(&self) -> HexColor {
        self.border_strong()
    }
    /// 드래그 스트립 배경. `--tasty-modhint-strip-bg` → `bg-sidebar`.
    #[inline]
    pub fn modhint_strip_bg(&self) -> HexColor {
        self.bg_sidebar()
    }
    /// 스트립 하단 / 조합 헤더 하단 구분선. `--tasty-modhint-separator` → `separator`.
    #[inline]
    pub fn modhint_separator(&self) -> HexColor {
        self.separator
    }
    /// 드래그 스트립 "held" 라벨 색. `--tasty-modhint-held-fg` → `text-muted`.
    #[inline]
    pub fn modhint_held_fg(&self) -> HexColor {
        self.text_muted()
    }
    /// 특수 역할 행 배경 (washed). `--tasty-modhint-role-bg` → `surface-active`.
    #[inline]
    pub fn modhint_role_bg(&self) -> HexColor {
        self.surface_active()
    }
    /// 특수 역할 행 leading 글리프 색. `--tasty-modhint-role-fg` → `accent-primary`.
    #[inline]
    pub fn modhint_role_fg(&self) -> HexColor {
        self.accent_primary()
    }
    /// 액션/역할 행 텍스트 색. `--tasty-modhint-row-fg` → `text-secondary`.
    #[inline]
    pub fn modhint_row_fg(&self) -> HexColor {
        self.text_secondary()
    }
    /// 빈 조합 플레이스홀더("바인딩 없음") 텍스트 색. `--tasty-modhint-empty-fg` → `text-muted`.
    /// 키캡 행의 `text-secondary` 보다 한 단계 절제된 톤 — 실제 항목이 아니라 부재 신호라
    /// 리스트에서 가장 조용하다(wash·글리프·키캡 없음).
    #[inline]
    pub fn modhint_empty_fg(&self) -> HexColor {
        self.text_muted()
    }
    /// plugin 행 leading agent dot 색. `--tasty-modhint-agent-dot` → `accent-agent`.
    #[inline]
    pub fn modhint_agent_dot(&self) -> HexColor {
        self.accent_agent()
    }

    // ── 컴포넌트 치수 (디자인 export 에 아직 토큰이 없는 자리) ──
    // 아래 열셋은 대응 디자인 토큰이 **없다.** 그래도 리터럴로 두면 안 되는 이유는
    // 토큰 부재가 아니라 **배율**이다: 본체는 egui `zoom_factor` 를 1.0 으로 고정하고
    // (`gfx/gpu.rs` `update_scale_factor`) UI 배율을 `with_colors_and_zoom` 의
    // `zoomed()` 로만 적용하므로, 호출부 리터럴은 `ui_scale` 을 따라가지 않는다.
    // 상자만 고정이고 안의 폰트·간격·글리프는 커지므로 0.85 에서 여백이 뜨고 1.2 에서
    // 내용이 잘린다 — 이 축의 값은 16~340 이라 폰트 축(13~17)보다 대가가 크다.
    // 값은 이식 전 리터럴 그대로다(zoom 1 픽셀 불변, `component_accessors_invariant_at_zoom_one`).
    // `modhint_*` 와 같은 사정 — export 가 갱신되면 생성물로 넘어간다.
    /// 포트 스캐너 컬럼 메뉴 최소 폭 (180px).
    #[inline]
    pub fn port_columns_menu_min_width(&self) -> LogicalPx {
        LogicalPx((180.0 * self.ui_zoom).round())
    }
    /// 포트 스캐너 상태 필터 메뉴 최소 폭 (216px). 컬럼 메뉴보다 넓은 것은 라벨이
    /// 길어서다 — 두 값을 하나로 합치면 컬럼 메뉴가 불필요하게 넓어진다.
    #[inline]
    pub fn port_state_menu_min_width(&self) -> LogicalPx {
        LogicalPx((216.0 * self.ui_zoom).round())
    }
    /// 포트 스캐너 상태 필터 체크박스 리스트 최대 높이 (168px). 초과분은 내부 스크롤.
    #[inline]
    pub fn port_state_menu_max_height(&self) -> LogicalPx {
        LogicalPx((168.0 * self.ui_zoom).round())
    }
    /// remote tool 헤더 최소 높이 (26px). 디자인 헤더 콘텐츠 높이 24 에, popup border 가
    /// stroke Outside 라 콘텐츠가 1px 아래에서 시작하는 것을 +2 로 보정한 값이다.
    /// 스케일 위의 값이 아니므로 토큰으로 스냅하지 않는다(ADR-0126).
    #[inline]
    pub fn remote_tool_header_min_height(&self) -> LogicalPx {
        LogicalPx((26.0 * self.ui_zoom).round())
    }
    /// 튜토리얼 토픽 팝업 본문 스크롤 최대 높이 (200px).
    #[inline]
    pub fn tutorial_topic_body_max_height(&self) -> LogicalPx {
        LogicalPx((200.0 * self.ui_zoom).round())
    }
    /// plugins 화면 헤더 높이 (48px).
    #[inline]
    pub fn plugins_header_height(&self) -> LogicalPx {
        LogicalPx((48.0 * self.ui_zoom).round())
    }
    /// plugins 화면 좌측 리스트 패널 폭 (240px). 목록 탭과 알림 탭이 같은 폭을
    /// 공유한다 — 탭을 바꿀 때 패널 경계가 움직이지 않아야 한다.
    #[inline]
    pub fn plugins_side_panel_width(&self) -> LogicalPx {
        LogicalPx((240.0 * self.ui_zoom).round())
    }
    /// 설정 > 모양의 폰트 패밀리 리스트 최대 높이 (250px).
    #[inline]
    pub fn font_family_menu_max_height(&self) -> LogicalPx {
        LogicalPx((250.0 * self.ui_zoom).round())
    }
    /// 파일 선택 팝업의 설명문 최대 폭 (340px) — 이 폭에서 줄바꿈한다.
    #[inline]
    pub fn file_picker_note_max_width(&self) -> LogicalPx {
        LogicalPx((340.0 * self.ui_zoom).round())
    }
    // 부팅·종료 로딩 화면의 브랜드 락업 스택(`src/gfx/gpu/loading.rs` + 갤러리
    // `chrome_loading` specimen). 값은 브랜드 락업 확정값(`guidelines/brand-logo.html`)
    // 이라 **바꾸지 않는다** — 바뀌는 것은 배율 추종뿐이다. 같은 스택의 간격
    // (`spacing_xl`·`spacing_lg`)과 phase 문구(`font_size_body`)는 이미 배율을 타므로,
    // 이 넷만 리터럴로 두면 배율에서 스택이 어긋난다. 특히 phase 슬롯은 높이가 고정인데
    // 안의 글자만 커져 **문구가 슬롯을 넘는다** — 그 슬롯의 존재 이유가 레이아웃 고정이다.
    /// 로딩 화면 워드마크 마크(수박 아이콘) 크기 (64px). 14px UI 폰트 상한의
    /// sanctioned 예외(브랜드 락업 — `docs/design/systems/theme.md` "명명 구조 상수").
    #[inline]
    pub fn loading_screen_wordmark_icon_size(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }
    /// 로딩 화면 워드마크 `tasty.` 폰트 크기 (38px). 위와 동일 근거의 브랜드 락업 값.
    /// 사이드바 헤더의 워드마크는 다른 값(`sidebar_wordmark_font_size`)이다 — 같은
    /// 락업의 두 크기이므로 하나로 합치지 않는다.
    #[inline]
    pub fn loading_screen_wordmark_font_size(&self) -> LogicalPx {
        LogicalPx((38.0 * self.ui_zoom).round())
    }
    /// 로딩 화면 스피너 크기 (32px). 디자인 확정: 기본 16 → boot hero 32.
    #[inline]
    pub fn loading_screen_spinner_size(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }
    /// 로딩 화면 phase 문구의 고정 높이 슬롯 (16px). 문구 유무와 무관하게 레이아웃이
    /// 흔들리지 않도록 항상 이 높이를 예약한다 — 그래서 안의 글자와 **같은 배율**을
    /// 타야 한다.
    #[inline]
    pub fn loading_screen_phase_slot_height(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }
    // ── 생성물로 넘어간 컴포넌트 토큰 그룹 ──
    // sidebar-category-header · autocomplete · md-table · drilldown · listctrl 의
    // 접근자는 전부 `generated_component.rs` 에서 생성된다. 디자인 export 에 해당
    // component 토큰이 들어오기 전까지 여기 수기로 두었던 것들인데, 생성물과 본문이
    // 완전히 같아 중복 정의가 되므로 제거했다. 값을 고치려면 디자인 CSS → vendor json.
    //
    // 토큰은 있으나 접근자가 없는 두 건은 egui 폰트 한계 때문이다:
    // `--tasty-sidebar-category-header-weight`(bold) · `--tasty-drilldown-title-font-weight`
    // (semibold) — 합성 bold 가 egui font_registry 에 등록돼 있지 않다(D2Coding Bold 는
    // 터미널 GPU 글리프 전용). 굵기 대신 색 승격으로 위계를 준다(`button.rs` 참조).
}

// ============================================================================
//  Tests
// ============================================================================

#[cfg(test)]
mod tests {
    // 테스트는 Theme/ThemeColors 병합·도출 로직을 검증하려고 원시 색상값을 직접
    // 만든다 (UI 색 "디자인" 이 아니라 스키마 동작 자체의 테스트). clippy 의 정상 예외 경로.
    #![allow(clippy::disallowed_methods)]

    use super::*;

    /// 모든 필드가 같은 색인 schema-level dummy. 빌트인 테마 색 의존 없이
    /// apply_partial / with_colors / extract_colors 동작만 검증.
    /// `surface_themes` 가 BTreeMap 이라 `const fn` 일 수 없다.
    fn dummy_colors() -> ThemeColors {
        let c = HexColor::from_rgb(0x12, 0x34, 0x56);
        ThemeColors {
            crust: c,
            mantle: c,
            base: c,
            surface0: c,
            surface1: c,
            surface2: c,
            overlay0: c,
            overlay1: c,
            overlay2: c,
            text: c,
            subtext1: c,
            subtext0: c,
            placeholder: c,
            blue: c,
            green: c,
            red: c,
            yellow: c,
            peach: c,
            mauve: c,
            teal: c,
            sky: c,
            lavender: c,
            flamingo: c,
            pink: c,
            maroon: c,
            rosewater: c,
            selection_bg: c,
            vi_cursor_bg: c,
            search_match_bg: c,
            search_match_active_bg: c,
            ansi_black: c,
            ansi_red: c,
            ansi_green: c,
            ansi_yellow: c,
            ansi_blue: c,
            ansi_magenta: c,
            ansi_cyan: c,
            ansi_white: c,
            ansi_bright_black: c,
            ansi_bright_red: c,
            ansi_bright_green: c,
            ansi_bright_yellow: c,
            ansi_bright_blue: c,
            ansi_bright_magenta: c,
            ansi_bright_cyan: c,
            ansi_bright_white: c,
            surface_themes: BTreeMap::new(),
        }
    }

    /// dummy 와 달리 semantic 접근자 매핑 검증용 — 비교 대상 필드마다 **고유 색**을
    /// 줘서 `accent_primary()==blue` 가 `==green` 으로 잘못 매핑돼도 잡히게 한다.
    fn distinct_colors() -> ThemeColors {
        let mut c = dummy_colors();
        c.crust = HexColor::from_rgb(1, 0, 0);
        c.mantle = HexColor::from_rgb(2, 0, 0);
        c.base = HexColor::from_rgb(3, 0, 0);
        c.surface0 = HexColor::from_rgb(4, 0, 0);
        c.surface1 = HexColor::from_rgb(5, 0, 0);
        c.surface2 = HexColor::from_rgb(6, 0, 0);
        c.overlay1 = HexColor::from_rgb(7, 0, 0);
        c.subtext0 = HexColor::from_rgb(8, 0, 0);
        c.subtext1 = HexColor::from_rgb(9, 0, 0);
        c.text = HexColor::from_rgb(10, 0, 0);
        c.placeholder = HexColor::from_rgb(11, 0, 0);
        c.blue = HexColor::from_rgb(12, 0, 0);
        c.green = HexColor::from_rgb(13, 0, 0);
        c.red = HexColor::from_rgb(14, 0, 0);
        c.yellow = HexColor::from_rgb(15, 0, 0);
        c.sky = HexColor::from_rgb(16, 0, 0);
        c.mauve = HexColor::from_rgb(17, 0, 0);
        c
    }

    /// A2: semantic 접근자가 A1 크로스워크대로 primitive 필드에 매핑되는지 고정.
    #[test]
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 반복 assert_eq 테스트 — clippy 과대계상, rca cognitive 0
    fn semantic_accessors_map_to_primitives() {
        let th = Theme::with_colors(distinct_colors(), false);

        // accent (의미색)
        assert_eq!(th.accent_primary(), th.blue);
        assert_eq!(th.accent_info(), th.sky);
        assert_eq!(th.accent_success(), th.green);
        assert_eq!(th.accent_warning(), th.yellow);
        assert_eq!(th.accent_danger(), th.red);
        assert_eq!(th.accent_agent(), th.mauve);

        // 배경 / 표면
        assert_eq!(th.bg_app(), th.crust);
        assert_eq!(th.bg_sidebar(), th.mantle);
        assert_eq!(th.bg_panel(), th.base);
        assert_eq!(th.surface_raised(), th.surface0);
        assert_eq!(th.surface_hover(), th.surface1);
        assert_eq!(th.surface_active(), th.surface2);

        // 텍스트
        assert_eq!(th.text_primary(), th.text);
        assert_eq!(th.text_secondary(), th.subtext1);
        assert_eq!(th.text_muted(), th.subtext0);
        assert_eq!(th.text_disabled(), th.overlay1);
        assert_eq!(th.text_placeholder(), th.placeholder);

        // 상태 표시
        assert_eq!(th.status_idle(), th.overlay0);
        assert_eq!(th.text_on_accent(), th.crust); // dark(Mocha): neutral-0=crust
        // light(Latte): accent 위 텍스트는 절대색 white 로 role-remap.
        let light = Theme::with_colors(distinct_colors(), true);
        assert_eq!(light.text_on_accent(), TEXT_ON_ACCENT_LIGHT);
        assert_ne!(light.text_on_accent(), light.crust);

        // 보더
        assert_eq!(th.border_default(), th.surface0);
        assert_eq!(th.border_strong(), th.surface1);
        assert_eq!(th.border_focus(), th.blue);
        assert_eq!(th.border_attached(), th.lavender); // accent-attached(이름≠토큰)

        // 오버레이 (도출 필드)
        assert_eq!(th.overlay_hover(), th.hover_overlay);
        assert_eq!(th.overlay_active(), th.active_overlay);

        // 다의성: 같은 primitive 로 수렴하는 role 들이 동일값인지 확인
        assert_eq!(th.accent_primary(), th.border_focus()); // 둘 다 blue
        assert_eq!(th.surface_raised(), th.border_default()); // 둘 다 surface0
    }

    /// markdown surface 인라인 표 토큰이 semantic 접근자 포인터로 매핑되고,
    /// 채움 값 사다리(mantle < base < surface0 < surface1)가 유지되는지 고정.
    #[test]
    fn md_table_tokens_map_to_semantics_and_keep_ladder() {
        let th = Theme::with_colors(distinct_colors(), false);

        assert_eq!(th.md_table_border(), th.border_strong()); // surface1
        assert_eq!(th.md_table_header_bg(), th.surface_raised()); // surface0
        assert_eq!(th.md_table_header_fg(), th.text_primary());
        assert_eq!(th.md_table_row_bg(), th.bg_panel()); // base
        assert_eq!(th.md_table_row_bg_zebra(), th.bg_sidebar()); // mantle
        assert_eq!(th.md_table_cell_fg(), th.text_secondary());
        assert_eq!(th.md_table_cell_padding_x(), th.spacing_sm);
        assert_eq!(th.md_table_cell_padding_y(), th.spacing_xs);

        // 값 사다리: zebra(mantle) < 행(base) < 헤더(surface0) < 격자선(surface1).
        assert_eq!(th.md_table_row_bg_zebra(), th.mantle);
        assert_eq!(th.md_table_row_bg(), th.base);
        assert_eq!(th.md_table_header_bg(), th.surface0);
        assert_eq!(th.md_table_border(), th.surface1);
    }

    #[test]
    fn apply_partial_overwrites_only_some_fields() {
        let mut base = dummy_colors();
        let original_red = base.red;
        let partial = PartialColors {
            blue: Some(HexColor::from_rgb(0x00, 0xff, 0x00)),
            ..Default::default()
        };
        base.apply_partial(&partial);
        assert_eq!(base.blue, HexColor::from_rgb(0x00, 0xff, 0x00));
        // red 는 건드리지 않음
        assert_eq!(base.red, original_red);
    }

    #[test]
    fn partial_default_is_empty() {
        let p = PartialColors::default();
        assert!(p.is_empty());
    }

    #[test]
    fn theme_with_colors_propagates_sizing() {
        let t = Theme::with_colors(dummy_colors(), false);
        assert_eq!(t.spacing_sm, SIZING.spacing_sm);
        assert_eq!(t.tab_width, SIZING.tab_width);
        assert!(!t.is_light);
    }

    /// 명명 const 로 값을 빼는 **대가가 축마다 다르다**는 것을 배율 ≠ 1 에서 고정한다.
    ///
    /// [`zoom_one_preserves_sizing`] 은 배율 1.0 에서만 재므로 `zoomed()` 를 타는 필드와
    /// 안 타는 필드를 **원리적으로 가르지 못한다**. 이 테스트는 그것이 못 보는 조건에서 잰다.
    ///
    /// **처음에 "반경은 zoom 을 타고 굵기는 안 탄다" 로 적었다가 변이로 고쳤다.**
    /// `border_width` 를 `zoomed()` 에 태우는 변이가 **살아남았고, 살아남는 것이 옳았다**:
    /// `zoomed()` 는 `(px * z).round()` 이고 지원 배율은 0.85 · 1.0 · 1.2 뿐이라
    /// (`AppearanceSettings::ui_scale_factor_for`), **1.0 과 2.0 은 셋 다 자기 자신으로
    /// 되돌아온다.** 즉 그 값들에 대해서는 `zoomed()` 경유 여부가 **값에서 관측되지 않는다.**
    ///
    /// ```text
    /// border_width      1.0 → 1 / 1 / 1     반올림 아래 배율 불변  → 경유 여부 관측 불가
    /// focus_ring_width  2.0 → 2 / 2 / 2     (경유하는데도 값은 그대로)
    /// corner_radius_sm  2.0 → 2 / 2 / 2
    /// icon_stroke_width 1.5 → 1 / 2 / 2     경유하면 값이 변한다   → 관측 가능
    /// corner_radius     4.0 → 3 / 4 / 5     경유하므로 값이 변한다
    /// corner_radius_lg  8.0 → 7 / 8 / 10
    /// ```
    ///
    /// 그래서 `docs/adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md` 의
    /// "축 확장" 절이 드는 근거는 **경유 여부가 아니라 값의 배율 가변성**이다 — 명명
    /// const 로 빼는 대가는 값이 배율에 따라 변하는 자리에서만 실재한다. 반경 기본·lg 는
    /// 변하고, 1px 보더는 어차피 안 변한다.
    ///
    /// 이 테스트가 잡는 것은 셋이다.
    /// 1. 반경 기본·lg 가 배율 가변성을 잃는다(스케일 값이 바뀌거나 반올림이 바뀐다).
    /// 2. `icon_stroke_width` 가 `zoomed()` 를 타게 된다 — 이건 값이 변하므로 관측된다.
    /// 3. **불변 셋의 전제 위에서 값이 어긋난다** — 아래 세 배율에서 `border_width` ·
    ///    `focus_ring_width` · `corner_radius_sm` 이 반올림 불변성을 잃는 경우(예:
    ///    `border_width` 가 1.0 → 1.5 로 바뀌면 1.2 배에서 2 가 된다).
    ///
    /// **3번이 잡지 *못하는* 것을 분명히 적는다 — 여기서 한 번 틀렸다.**
    /// 지원 배율 **집합 자체**가 바뀌는 것(예: 1.5 가 추가되는 것)은 이 테스트가
    /// 감지하지 못한다. 아래 `SUPPORTED_ZOOMS` 는 **하드코딩 사본**이라 원본
    /// (`AppearanceSettings::ui_scale_factor_for`)에 배율이 늘어도 그대로 있고,
    /// 이 테스트는 초록으로 남는다. 의존 방향이 반대라(그 크레이트가 이 크레이트에
    /// 의존한다) 여기서 원본을 읽을 방법이 없다.
    ///
    /// 그래서 **집합의 핀은 원본 쪽에 있다**:
    /// `tasty-settings` 의 `the_supported_ui_scale_set_is_pinned`. 배율이 늘면
    /// 그 핀이 울고, 그 메시지가 이 사본을 좌표로 지목한다.
    ///
    /// (원래 이 자리에는 "값이 어긋나면 3번이 운다" 고 적혀 있었는데, **토큰 '값'과
    /// 배율 '집합' 이 한 문장에서 섞여** 집합 변화까지 잡는 것처럼 읽혔다. 값 쪽은
    /// 참이고 집합 쪽은 거짓이었다.)
    #[test]
    fn zoom_cost_differs_by_axis() {
        /// `AppearanceSettings::ui_scale_factor_for` 의 값. 의존 방향이 반대라 복사한다.
        ///
        /// **사본이라 원본을 따라가지 않는다.** 원본에 배율이 추가돼도 여기는 그대로다 —
        /// 그 어긋남을 잡는 것은 이 파일이 아니라 원본 크레이트의
        /// `the_supported_ui_scale_set_is_pinned` 다.
        const SUPPORTED_ZOOMS: [f32; 3] = [0.85, 1.0, 1.2];

        let base = Theme::with_colors_and_zoom(dummy_colors(), false, 1.0);

        for z in SUPPORTED_ZOOMS {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);

            // ① 대가가 실재하는 축 — 배율에서 값이 `zoomed()` 결과와 같아야 한다.
            for (name, b, v) in [
                ("corner_radius", base.corner_radius, t.corner_radius),
                (
                    "corner_radius_lg",
                    base.corner_radius_lg,
                    t.corner_radius_lg,
                ),
            ] {
                let want = LogicalPx((b.value() * z).round());
                assert_eq!(v, want, "{name} 이 배율 {z} 에서 `zoomed()` 결과와 다르다");
            }

            // ② 굵기 hairline 은 `zoomed()` 밖이다. 1.5 라 경유하면 값이 변하므로
            //    이 등식이 실제로 판별력을 갖는다(변이로 확인했다).
            assert_eq!(
                t.icon_stroke_width, base.icon_stroke_width,
                "icon_stroke_width 가 배율 {z} 에서 변했다 — `zoomed()` 를 타게 됐다"
            );

            // ③ 불변 셋의 전제 — 이 값들은 경유하든 안 하든 배율에서 그대로다.
            //    **토큰 값이 바뀌면** 여기가 운다(예: border_width 1.0 → 1.5).
            //    지원 배율 집합이 바뀌는 것은 여기가 아니라 `tasty-settings` 의
            //    `the_supported_ui_scale_set_is_pinned` 가 잡는다 — 위 사본 참조.
            for (name, v) in [
                ("border_width", base.border_width),
                ("focus_ring_width", base.focus_ring_width),
                ("corner_radius_sm", base.corner_radius_sm),
            ] {
                assert_eq!(
                    LogicalPx((v.value() * z).round()),
                    v,
                    "{name}({v:?}) 이 배율 {z} 에서 더 이상 반올림 불변이 아니다 — \
                     굵기 축에 zoom 대가가 없다는 전제가 깨졌다"
                );
            }
        }

        // ①의 실물: 기본 반경은 지원 배율 양끝에서 실제로 다른 값이 된다.
        let small = Theme::with_colors_and_zoom(dummy_colors(), false, 0.85);
        let large = Theme::with_colors_and_zoom(dummy_colors(), false, 1.2);
        assert_ne!(small.corner_radius, base.corner_radius);
        assert_ne!(large.corner_radius, base.corner_radius);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 반복 assert_eq 테스트 — clippy 과대계상, rca cognitive 0
    fn zoom_one_preserves_sizing() {
        let base = Theme::with_colors(dummy_colors(), false);
        let zoomed = Theme::with_colors_and_zoom(dummy_colors(), false, 1.0);
        assert_eq!(base.spacing_xs, zoomed.spacing_xs);
        assert_eq!(base.spacing_sm, zoomed.spacing_sm);
        assert_eq!(base.spacing_md, zoomed.spacing_md);
        assert_eq!(base.spacing_lg, zoomed.spacing_lg);
        assert_eq!(base.spacing_xl, zoomed.spacing_xl);
        assert_eq!(base.font_size_caption, zoomed.font_size_caption);
        assert_eq!(base.font_size_body, zoomed.font_size_body);
        assert_eq!(base.font_size_heading, zoomed.font_size_heading);
        assert_eq!(base.font_size_max, zoomed.font_size_max);
        assert_eq!(base.border_width, zoomed.border_width);
        assert_eq!(base.focus_ring_width, zoomed.focus_ring_width);
        assert_eq!(base.corner_radius, zoomed.corner_radius);
        assert_eq!(base.item_height_tree, zoomed.item_height_tree);
        assert_eq!(base.item_height_interactive, zoomed.item_height_interactive);
        assert_eq!(base.item_height_tab, zoomed.item_height_tab);
        assert_eq!(base.tab_width, zoomed.tab_width);
        assert_eq!(base.sidebar_logo_size, zoomed.sidebar_logo_size);
        assert_eq!(
            base.sidebar_logo_collapsed_size,
            zoomed.sidebar_logo_collapsed_size
        );
        assert_eq!(
            base.sidebar_wordmark_font_size,
            zoomed.sidebar_wordmark_font_size
        );
        assert_eq!(
            base.sidebar_section_heading_font_size,
            zoomed.sidebar_section_heading_font_size
        );
        assert_eq!(
            base.sidebar_button_label_font_size,
            zoomed.sidebar_button_label_font_size
        );
        assert_eq!(
            base.sidebar_collapsed_slot_width,
            zoomed.sidebar_collapsed_slot_width
        );
        assert_eq!(
            base.sidebar_collapsed_icon_height,
            zoomed.sidebar_collapsed_icon_height
        );
        assert_eq!(
            base.sidebar_collapsed_workspace_height,
            zoomed.sidebar_collapsed_workspace_height
        );
        assert_eq!(base.tab_bar_height, zoomed.tab_bar_height);
        assert_eq!(base.tab_bar_label_font_size, zoomed.tab_bar_label_font_size);
        assert_eq!(base.tab_bar_arrow_font_size, zoomed.tab_bar_arrow_font_size);
        assert_eq!(base.titlebar_height, zoomed.titlebar_height);
        assert_eq!(base.traffic_size, zoomed.traffic_size);
        assert_eq!(base.caption_width, zoomed.caption_width);
        assert_eq!(base.window_button_size, zoomed.window_button_size);
    }

    #[test]
    fn ui_zoom_field_stores_value() {
        let default = Theme::with_colors(dummy_colors(), false);
        assert_eq!(default.ui_zoom, 1.0);
        let custom = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        assert_eq!(custom.ui_zoom, 1.5);
    }

    #[test]
    fn sidebar_tokens_scale_with_zoom() {
        // 22 * 1.5 = 33.0, 17 * 1.5 = 25.5 → round → 26.
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        assert_eq!(t.sidebar_logo_size.value(), 33.0);
        assert_eq!(t.sidebar_wordmark_font_size.value(), 26.0);
        // 32 * 1.5 = 48.0
        assert_eq!(t.sidebar_collapsed_slot_width.value(), 48.0);
    }

    #[test]
    fn zoom_one_point_five_doubles_almost_spacing() {
        // spacing_md=12 → 12 * 1.5 = 18.0 (정수). spacing_sm=8 → 12.
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        assert_eq!(t.spacing_md.value(), 18.0);
        assert_eq!(t.spacing_sm.value(), 12.0);
        assert_eq!(t.spacing_lg.value(), 24.0);
        // font_size_body=13 → 13 * 1.5 = 19.5 → round → 20.0
        assert_eq!(t.font_size_body.value(), 20.0);
    }

    #[test]
    fn border_width_unaffected_by_zoom() {
        let t_small = Theme::with_colors_and_zoom(dummy_colors(), false, 0.85);
        let t_large = Theme::with_colors_and_zoom(dummy_colors(), false, 1.2);
        assert_eq!(t_small.border_width.value(), 1.0);
        assert_eq!(t_large.border_width.value(), 1.0);
        // tab_width 도 zoom 무관 (탭바 제외 정책).
        assert_eq!(t_small.tab_width, SIZING.tab_width);
        assert_eq!(t_large.tab_width, SIZING.tab_width);
        // tab_bar_* 도 zoom 무관 (탭바 제외 정책).
        assert_eq!(t_small.tab_bar_height, SIZING.tab_bar_height);
        assert_eq!(t_large.tab_bar_height, SIZING.tab_bar_height);
        assert_eq!(
            t_small.tab_bar_label_font_size,
            SIZING.tab_bar_label_font_size
        );
        assert_eq!(
            t_large.tab_bar_label_font_size,
            SIZING.tab_bar_label_font_size
        );
        assert_eq!(
            t_small.tab_bar_arrow_font_size,
            SIZING.tab_bar_arrow_font_size
        );
        assert_eq!(
            t_large.tab_bar_arrow_font_size,
            SIZING.tab_bar_arrow_font_size
        );
        // titlebar(CSD) 토큰도 zoom 무관 (px 고정 정책).
        assert_eq!(t_small.titlebar_height, SIZING.titlebar_height);
        assert_eq!(t_large.titlebar_height, SIZING.titlebar_height);
        assert_eq!(t_small.traffic_size, SIZING.traffic_size);
        assert_eq!(t_large.traffic_size, SIZING.traffic_size);
        assert_eq!(t_small.caption_width, SIZING.caption_width);
        assert_eq!(t_large.caption_width, SIZING.caption_width);
        assert_eq!(t_small.window_button_size, SIZING.window_button_size);
        assert_eq!(t_large.window_button_size, SIZING.window_button_size);
    }

    /// P1/P6: CSD 타이틀바 길이 토큰 값 고정 (디자인 jsx px).
    #[test]
    fn titlebar_sizing_tokens_fixed() {
        let t = Theme::with_colors(dummy_colors(), false);
        assert_eq!(t.titlebar_height.value(), 36.0);
        assert_eq!(t.traffic_size.value(), 12.0);
        assert_eq!(t.caption_width.value(), 46.0);
        assert_eq!(t.window_button_size.value(), 24.0);
    }

    /// P1: titlebar 컴포넌트 색 접근자가 각각의 semantic 접근자(bg_app/bg_sidebar/separator/
    /// text_secondary/text_muted)에 그대로 위임하는지 고정 — 신규 토큰 없이 조합만으로
    /// 구성됨을 회귀 방지.
    #[test]
    fn titlebar_color_accessors_map_to_semantics() {
        let th = Theme::with_colors(distinct_colors(), false);
        assert_eq!(th.titlebar_bg(), th.bg_app());
        assert_eq!(th.titlebar_bg_inactive(), th.bg_sidebar());
        assert_eq!(th.titlebar_border(), th.separator);
        assert_eq!(th.titlebar_fg(), th.text_secondary());
        assert_eq!(th.titlebar_fg_inactive(), th.text_muted());
    }

    /// P1: OS 리터럴(테마 불변) close red / white 글리프 값 고정.
    #[test]
    fn window_close_literals_are_theme_invariant() {
        let dark = Theme::with_colors(distinct_colors(), false);
        let light = Theme::with_colors(distinct_colors(), true);
        assert_eq!(
            dark.accent_window_close(),
            HexColor::from_rgb(0xc4, 0x2b, 0x1c)
        );
        assert_eq!(dark.accent_window_close(), light.accent_window_close());
        assert_eq!(
            dark.text_on_window_close(),
            HexColor::from_rgb(0xff, 0xff, 0xff)
        );
        assert_eq!(dark.text_on_window_close(), light.text_on_window_close());
    }

    /// token-policy 신설 토큰 값 고정 (semantic.css 의 role 값).
    #[test]
    fn token_policy_new_sizing_values() {
        let t = Theme::with_colors(dummy_colors(), false);
        assert_eq!(t.font_size_micro.value(), 10.0);
        assert_eq!(t.font_size_prose_h1.value(), 20.0);
        assert_eq!(t.line_height_ui, 1.4);
        assert_eq!(t.tooltip_max_width.value(), 240.0);
        assert_eq!(t.font_size_term_sm.value(), 12.0);
        assert_eq!(t.font_size_term.value(), 14.0);
        assert_eq!(t.font_size_term_lg.value(), 16.0);
        assert_eq!(t.icon_glyph_size_xs.value(), 12.0);
        // icon sm/md 정합 확인
        assert_eq!(t.icon_glyph_size_sm.value(), 14.0);
        assert_eq!(t.icon_glyph_size_md.value(), 16.0);
        assert_eq!(t.icon_glyph_size_row_action.value(), 15.0);
    }

    /// 상태바 인라인 글리프의 **크기 role**(2026-09-20 결정 G1). 배율은 바의 다른 글자와
    /// 같이 타고 — `round(12 × s)` = 10 / 12 / 14 — **바 높이 24 는 배율 밖**이라 이
    /// 결정으로 안 바뀐다.
    ///
    /// 두 테마를 나란히 잰다. *치수가 어느 테마가 켜졌는지에 의존하면 안 된다* 는 것이
    /// 결정의 명시 제약이라, `is_light` 분기를 실수로 들이면 여기서 죽는다 — 값 하나만
    /// 보는 시험은 그 실수를 못 본다.
    #[test]
    fn statusbar_glyph_size_scales_with_the_bar_and_never_with_the_theme() {
        for (zoom, want) in [(0.85_f32, 10.0_f32), (1.0, 12.0), (1.2, 14.0)] {
            let dark = Theme::with_colors_and_zoom(dummy_colors(), false, zoom);
            let light = Theme::with_colors_and_zoom(dummy_colors(), true, zoom);
            assert_eq!(
                dark.statusbar_glyph_size().value(),
                want,
                "zoom {zoom}: round(12 × s) 가 아니다"
            );
            assert_eq!(
                dark.statusbar_glyph_size(),
                light.statusbar_glyph_size(),
                "zoom {zoom}: 글리프 크기가 테마에 따라 갈렸다"
            );
            // 크기 role 은 `icon-size-xs` 를 거쳐 온다 — 위젯이 직접 읽던 그 값이다.
            assert_eq!(
                dark.statusbar_glyph_size(),
                dark.icon_glyph_size_xs,
                "zoom {zoom}: 크기 role 이 semantic 사슬을 벗어났다"
            );
            for (name, t) in [("dark", &dark), ("light", &light)] {
                assert_eq!(
                    t.status_bar_height.value(),
                    24.0,
                    "zoom {zoom} ({name}): 바 높이는 배율 밖이다"
                );
            }
        }
    }

    /// 이 lane 이 리터럴에서 옮겨 온 컴포넌트 치수 아홉이 **`ui_scale` 을 탄다**는 것을
    /// 고정한다. 위 `component_accessors_invariant_at_zoom_one` 은 zoom 1 값만 보므로,
    /// 접근자를 다시 상수로 되돌리는 변경을 못 잡는다 — 그게 이 축의 원래 결함이었다.
    /// 본체는 egui `zoom_factor` 를 1.0 으로 고정하고 배율을 `zoomed()` 로만 넣으므로,
    /// 이 값들이 배율을 놓치면 상자만 고정되고 내용은 커져 1.2 에서 잘린다.
    #[test]
    fn component_dimensions_without_design_tokens_follow_ui_zoom() {
        let at = |z: f32| Theme::with_colors_and_zoom(dummy_colors(), false, z);
        // (접근자, zoom 1 값) — 값은 이식 전 호출부 리터럴이다.
        /// (접근자, zoom 1 값, 이름). clippy `type_complexity` 회피용 별칭.
        type ZoomProbe = (fn(&Theme) -> LogicalPx, f32, &'static str);
        let probes: &[ZoomProbe] = &[
            (
                Theme::port_columns_menu_min_width,
                180.0,
                "port_columns_menu_min_width",
            ),
            (
                Theme::port_state_menu_min_width,
                216.0,
                "port_state_menu_min_width",
            ),
            (
                Theme::port_state_menu_max_height,
                168.0,
                "port_state_menu_max_height",
            ),
            (
                Theme::remote_tool_header_min_height,
                26.0,
                "remote_tool_header_min_height",
            ),
            (
                Theme::tutorial_topic_body_max_height,
                200.0,
                "tutorial_topic_body_max_height",
            ),
            (Theme::plugins_header_height, 48.0, "plugins_header_height"),
            (
                Theme::plugins_side_panel_width,
                240.0,
                "plugins_side_panel_width",
            ),
            (
                Theme::font_family_menu_max_height,
                250.0,
                "font_family_menu_max_height",
            ),
            (
                Theme::file_picker_note_max_width,
                340.0,
                "file_picker_note_max_width",
            ),
            (
                Theme::loading_screen_wordmark_icon_size,
                64.0,
                "loading_screen_wordmark_icon_size",
            ),
            (
                Theme::loading_screen_wordmark_font_size,
                38.0,
                "loading_screen_wordmark_font_size",
            ),
            (
                Theme::loading_screen_spinner_size,
                32.0,
                "loading_screen_spinner_size",
            ),
            (
                Theme::loading_screen_phase_slot_height,
                16.0,
                "loading_screen_phase_slot_height",
            ),
        ];
        let (small, base, large) = (at(0.85), at(1.0), at(1.2));
        for (f, expect_at_one, name) in probes {
            assert_eq!(f(&base).value(), *expect_at_one, "{name} zoom 1");
            assert_eq!(
                f(&small).value(),
                (expect_at_one * 0.85f32).round(),
                "{name} zoom 0.85"
            );
            assert_eq!(
                f(&large).value(),
                (expect_at_one * 1.2f32).round(),
                "{name} zoom 1.2"
            );
            // 배율 셋이 실제로 갈라지는지 — 이 축의 값은 전부 16 이상이라 반올림이
            // 셋을 뭉개지 않는다(폰트·굵기 축과 다른 점이다, ADR-0126 의 표).
            // 하한이 16 인 것은 로딩 화면 phase 슬롯이다: 0.85→14 · 1.0→16 · 1.2→19.
            assert!(
                f(&large).value() > f(&base).value() && f(&base).value() > f(&small).value(),
                "{name} 이 배율을 안 탄다"
            );
        }
    }

    /// 행 액션 글리프가 zoom 경로에 실제로 들어가 있는지. 평범한 `const` 로 두면
    /// 같은 팝업의 헤더 아이콘만 커지고 이 아이콘만 고정되므로, 배율이 적용되는지
    /// 자체를 고정한다(`zoomed` 는 반올림한다).
    #[test]
    fn row_action_glyph_follows_ui_zoom() {
        let at = |z: f32| {
            Theme::with_colors_and_zoom(dummy_colors(), false, z)
                .icon_glyph_size_row_action
                .value()
        };
        assert_eq!(at(1.0), 15.0);
        assert_eq!(at(0.85), (15.0f32 * 0.85).round());
        assert_eq!(at(1.2), (15.0f32 * 1.2).round());
        assert!(at(1.2) > at(1.0) && at(1.0) > at(0.85));
    }

    /// macOS 신호등 색 + disabled opacity 는 테마 불변 OS/정책 리터럴.
    #[test]
    fn macos_traffic_and_disabled_opacity_are_invariant() {
        let dark = Theme::with_colors(distinct_colors(), false);
        let light = Theme::with_colors(distinct_colors(), true);
        assert_eq!(
            dark.accent_macos_close(),
            HexColor::from_rgb(0xec, 0x6a, 0x5e)
        );
        assert_eq!(
            dark.accent_macos_min(),
            HexColor::from_rgb(0xf4, 0xbf, 0x4f)
        );
        assert_eq!(
            dark.accent_macos_zoom(),
            HexColor::from_rgb(0x61, 0xc5, 0x54)
        );
        assert_eq!(dark.accent_macos_close(), light.accent_macos_close());
        assert_eq!(dark.accent_macos_min(), light.accent_macos_min());
        assert_eq!(dark.accent_macos_zoom(), light.accent_macos_zoom());
        assert_eq!(
            dark.brand_melon_flesh(),
            HexColor::from_rgb(0xf2, 0x5d, 0x6b)
        );
        assert_eq!(dark.brand_melon_flesh(), light.brand_melon_flesh());
        assert_eq!(dark.opacity_disabled(), 0.5);
    }

    /// prose / term 폰트는 surface CONTENT 라 UI zoom 미적용 (micro 는 적용).
    #[test]
    fn prose_term_fonts_unaffected_by_zoom() {
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        assert_eq!(t.font_size_prose_h1, SIZING.font_size_prose_h1);
        assert_eq!(t.line_height_ui, SIZING.line_height_ui);
        assert_eq!(t.font_size_term, SIZING.font_size_term);
        // micro 는 caption 처럼 zoom 적용: 10 * 1.5 = 15.
        assert_eq!(t.font_size_micro.value(), 15.0);
    }

    #[test]
    fn set_is_light_swaps_overlays() {
        let mut t = Theme::with_colors(dummy_colors(), false);
        let dark_hover = t.hover_overlay;
        t.set_is_light(true);
        assert!(t.is_light);
        assert_ne!(t.hover_overlay, dark_hover);
        // 라이트 오버레이는 RGB 가 0
        assert_eq!(t.hover_overlay.r, 0);
    }

    #[test]
    fn extract_apply_round_trip() {
        // dummy 와 두 필드만 다른 변형으로 extract/apply round-trip 검증.
        let mut variant = dummy_colors();
        variant.blue = HexColor::from_rgb(0x00, 0xff, 0x00);
        variant.red = HexColor::from_rgb(0xff, 0x00, 0xff);

        let t = Theme::with_colors(variant.clone(), true);
        let c = t.extract_colors();
        assert_eq!(c, variant);

        let mut t2 = Theme::with_colors(dummy_colors(), false);
        t2.apply_colors(&c);
        // apply_colors 는 with_colors(clone, self.is_light) 이므로 is_light 보존.
        assert!(!t2.is_light);
        assert_eq!(t2.extract_colors(), variant);
    }

    #[test]
    fn apply_partial_merges_surface_themes_entry_wise() {
        let mut base = dummy_colors();
        base.surface_themes
            .insert("terminal".to_string(), FALLBACK_SURFACE.clone());

        let mut partial = PartialColors::default();
        let p_st = PartialSurfaceTheme {
            focused_bg: Some(HexColor::from_rgb(0x11, 0x22, 0x33)),
            ..Default::default()
        };
        partial.surface_themes.insert("terminal".to_string(), p_st);
        // 또 base 에 없는 id 도 partial 만으로 등장 가능 — default 위에 입혀짐
        let p_md = PartialSurfaceTheme {
            focused_fg: Some(HexColor::from_rgb(0xaa, 0xbb, 0xcc)),
            ..Default::default()
        };
        partial.surface_themes.insert("markdown".to_string(), p_md);

        base.apply_partial(&partial);

        let term = base
            .surface_themes
            .get("terminal")
            .expect("terminal exists");
        assert_eq!(term.focused_bg, HexColor::from_rgb(0x11, 0x22, 0x33));
        // partial 이 안 건드린 focused_fg 는 base (FALLBACK_SURFACE) 값
        assert_eq!(term.focused_fg, FALLBACK_SURFACE.focused_fg);

        let md = base
            .surface_themes
            .get("markdown")
            .expect("markdown created");
        assert_eq!(md.focused_fg, HexColor::from_rgb(0xaa, 0xbb, 0xcc));
        // base 에 없던 id 라 default(FALLBACK_SURFACE) 위에 입혔으니 그 외 필드는 fallback
        assert_eq!(md.focused_bg, FALLBACK_SURFACE.focused_bg);
    }

    #[test]
    fn surface_theme_apply_partial_overwrites_only_some_fields() {
        let mut base = FALLBACK_SURFACE.clone();
        let original_fg = base.focused_fg;
        let partial = PartialSurfaceTheme {
            focused_bg: Some(HexColor::from_rgb(0xff, 0x00, 0x00)),
            ..Default::default()
        };
        base.apply_partial(&partial);
        assert_eq!(base.focused_bg, HexColor::from_rgb(0xff, 0x00, 0x00));
        // focused_fg 는 건드리지 않음
        assert_eq!(base.focused_fg, original_fg);
    }

    #[test]
    fn partial_surface_theme_default_is_empty() {
        let p = PartialSurfaceTheme::default();
        assert!(p.is_empty());
    }

    #[test]
    fn fallback_surface_is_dark_friendly() {
        // 모든 surface 가 안전하게 동작하려면 fallback 이 검은 배경 + 가독성 있는 fg 여야 한다.
        assert_eq!(FALLBACK_SURFACE.focused_bg.r, 0);
        assert_eq!(FALLBACK_SURFACE.focused_bg.g, 0);
        assert_eq!(FALLBACK_SURFACE.focused_bg.b, 0);
    }

    // ── design-tokens 04 (F2b): component tier 접근자 zoom 회귀 + 값 불변 ──

    /// zoom 1.0 에서 component 접근자가 tasty-ui-widgets 이식 전 위젯이 쓰던 값과
    /// 정확히 일치함을 대표 속성으로 실측 대조 (갤러리 픽셀 diff 0 의 근거).
    #[test]
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 반복 assert_eq 테스트 — clippy 과대계상, rca cognitive 0
    fn component_accessors_invariant_at_zoom_one() {
        let t = Theme::with_colors(dummy_colors(), false);
        // primitive-직접 종착 — 구 매직넘버/파일 const 값이 zoom 1.0 에서 고정.
        assert_eq!(t.button_height_lg().value(), 32.0); // control.rs CONTROL_HEIGHT_LG
        assert_eq!(t.checkbox_size().value(), 16.0); // toggle.rs BOX
        assert_eq!(t.switch_track_width().value(), 28.0); // toggle.rs SWITCH_W
        assert_eq!(t.switch_track_height().value(), 16.0); // toggle.rs SWITCH_H
        assert_eq!(t.switch_thumb_size().value(), 12.0); // toggle.rs SWITCH_THUMB
        assert_eq!(t.switch_thumb_inset().value(), 2.0); // toggle.rs SWITCH_INSET
        assert_eq!(t.tag_size().value(), 16.0); // chip.rs TAG_HEIGHT
        assert_eq!(t.tag_dot_size().value(), 8.0); // chip.rs TAG_DOT
        assert_eq!(t.kbd_gap().value(), 3.0); // chip.rs KBD_GAP
        assert_eq!(t.kbd_shadow_depth().value(), 2.0); // chip.rs KBD_BOTTOM_BORDER
        assert_eq!(t.select_chevron_room().value(), 28.0); // select.rs CHEVRON_PAD
        // 디자인 export 에 토큰이 없는 컴포넌트 치수 — 이식 전 호출부 리터럴과 동일.
        assert_eq!(t.port_columns_menu_min_width().value(), 180.0);
        assert_eq!(t.port_state_menu_min_width().value(), 216.0);
        assert_eq!(t.port_state_menu_max_height().value(), 168.0);
        assert_eq!(t.remote_tool_header_min_height().value(), 26.0);
        assert_eq!(t.tutorial_topic_body_max_height().value(), 200.0);
        assert_eq!(t.plugins_header_height().value(), 48.0);
        assert_eq!(t.plugins_side_panel_width().value(), 240.0);
        assert_eq!(t.font_family_menu_max_height().value(), 250.0);
        assert_eq!(t.file_picker_note_max_width().value(), 340.0);
        // semantic-종착 — 이식 전 위젯이 읽던 바로 그 zoomed 필드와 동일 값.
        assert_eq!(t.button_gap().value(), t.spacing_sm.value()); // button gap
        assert_eq!(t.button_radius().value(), t.corner_radius.value());
        assert_eq!(t.input_height().value(), t.item_height_interactive.value());
        assert_eq!(t.tree_row_height().value(), t.item_height_tree.value()); // =22
        assert_eq!(
            t.menu_item_height().value(),
            t.item_height_interactive.value()
        );
        assert_eq!(t.status_dot_size().value(), 8.0);
        // 색: component 접근자 == 이식 전 semantic 접근자.
        assert_eq!(t.button_primary_bg(), t.accent_primary());
        assert_eq!(t.input_border_focus(), t.border_focus());
        assert_eq!(t.status_dot_success(), t.accent_success());
        assert_eq!(t.tree_row_fg_active(), t.text_primary());
        assert_eq!(t.table_header_fg(), t.text_muted());
    }

    /// zoom≠1.0 에서 이식으로 스케일이 생긴 치수 접근자가 with_colors_and_zoom
    /// resolve(= value*zoom 반올림)를 따라감을 검증 (완료조건 3).
    #[test]
    fn component_dim_accessors_scale_with_zoom() {
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        // primitive-직접: 구 고정 매직넘버가 이제 zoom 스케일한다.
        assert_eq!(t.button_height_lg().value(), 48.0); // 32 * 1.5
        assert_eq!(t.checkbox_size().value(), 24.0); // 16 * 1.5
        assert_eq!(t.switch_track_width().value(), 42.0); // 28 * 1.5
        assert_eq!(t.tag_size().value(), 24.0); // 16 * 1.5
        assert_eq!(t.kbd_gap().value(), 5.0); // 3 * 1.5 = 4.5 → round 5
        assert_eq!(t.select_chevron_room().value(), 42.0); // 28 * 1.5
        // semantic-종착: 대응 zoomed 필드를 그대로 반영한다.
        assert_eq!(t.button_gap().value(), t.spacing_sm.value()); // 12
        assert_eq!(t.tree_row_height().value(), t.item_height_tree.value()); // 33
        assert_eq!(t.input_height().value(), 42.0); // control-height 28 * 1.5
    }

    /// UI 폰트 토큰은 **어떤 zoom 에서도 정수**다 — `zoomed()` 가 `.round()` 하기
    /// 때문이다. 이건 편의 성질이 아니라 `docs/design/systems/theme.md` "스케일 밖
    /// 폰트 값" 규칙("`.5` 로 끝나는 값은 토큰이 될 수 없다")이 서 있는 전제다.
    /// 그 전제가 깨지는 길은 둘이다 — `zoomed()` 가 반올림을 그만두거나, 새 UI 폰트
    /// 필드가 `zoomed()` 를 우회해 `SIZING` 값을 그대로 받거나. 어느 쪽이든 규칙
    /// 문장이 먼저 거짓이 되므로 여기서 잡는다. (`SIZING` 리터럴 자체가 `.5` 가
    /// 되는 것은 여기 걸리지 않는다 — 그래도 `zoomed()` 가 정수로 만들기 때문이고,
    /// 규칙이 말하는 "토큰 값" 은 zoom 을 거친 뒤의 값이다.)
    #[test]
    fn ui_font_size_tokens_are_integers_at_every_zoom() {
        for zoom in [0.5, 0.85, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 3.0] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, zoom);
            for (name, px) in [
                ("font_size_micro", t.font_size_micro),
                ("font_size_caption", t.font_size_caption),
                ("font_size_body", t.font_size_body),
                ("font_size_heading", t.font_size_heading),
                ("font_size_max", t.font_size_max),
            ] {
                let v = px.value();
                assert_eq!(
                    v,
                    v.round(),
                    "{name} 이 zoom {zoom} 에서 정수가 아니다({v}) — theme.md 의 \
                     \"`.5` 값은 토큰이 될 수 없다\" 규칙이 이 성질 위에 서 있다"
                );
            }
        }
    }

    /// 출하되는 그림자 토큰 명부 + 각자의 디자인 소수 알파. 아래 두 시험(알파 산술 ·
    /// 음수 spread 금지)이 **이 하나**를 공유한다 — 명부를 시험마다 손으로 적으면 새
    /// 토큰이 한쪽에만 들어가 다른 쪽이 조용히 그것을 안 본다.
    ///
    /// 이 명부가 `theme.rs` 의 실제 `pub const …: ShadowToken` 전부인지는
    /// [`shipped_shadow_token_roster_is_complete`] 가 소스에서 다시 읽어 대조한다.
    const SHIPPED_SHADOW_TOKENS: &[(&str, ShadowToken, f32)] = &[
        ("SHADOW_POPOVER", SHADOW_POPOVER, 0.4),
        ("SHADOW_MODAL", SHADOW_MODAL, 0.55),
    ];

    /// [`SHIPPED_SHADOW_TOKENS`] 가 `theme.rs` 가 출하하는 `ShadowToken` 상수 전부인가.
    /// 명부를 안 늘린 채 세 번째 토큰이 들어오면 알파 산술도 음수 spread 잠금도 그
    /// 토큰을 **안 보고** 초록이 된다 — 그 조용한 절반 실행을 막으려고 명부를
    /// 정본(이 파일의 소스)에서 다시 읽는다.
    ///
    /// ADR-0254 의 "출하되는 그림자 토큰 중 하나가 음수 `spread` 를 갖게 된다" 재검토
    /// 트리거가 채널로 성립하는 근거가 이 대조다.
    #[test]
    fn shipped_shadow_token_roster_is_complete() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/theme.rs"),
        )
        .expect("theme.rs");
        let mut found: Vec<String> = Vec::new();
        for line in src.lines() {
            let t = line.trim_end();
            // `pub const SHADOW_XXX: ShadowToken = ShadowToken {`
            let Some(rest) = t.strip_prefix("pub const ") else {
                continue;
            };
            let Some((name, tail)) = rest.split_once(':') else {
                continue;
            };
            if tail.trim_start().starts_with("ShadowToken") {
                found.push(name.trim().to_string());
            }
        }
        found.sort();
        assert!(
            !found.is_empty(),
            "theme.rs 에서 ShadowToken 상수를 하나도 못 찾았다 — 스캔이 비면 이 대조는 \
             거짓 초록이 된다(경로/선언 형태를 먼저 확인해라)"
        );
        let mut listed: Vec<String> = SHIPPED_SHADOW_TOKENS
            .iter()
            .map(|(n, _, _)| (*n).to_string())
            .collect();
        listed.sort();
        assert_eq!(
            found, listed,
            "theme.rs 의 ShadowToken 상수와 SHIPPED_SHADOW_TOKENS 가 어긋났다 — 빠진 \
             토큰은 알파 산술도 음수 spread 잠금도 통과하지 않고 **안 보인다**"
        );
    }

    /// 그림자 토큰의 알파는 디자인 rgba 의 소수 알파를 0~255 로 옮긴 값이다 —
    /// 그 산술을 상수 옆 주석이 아니라 여기서 고정한다(주석은 값을 안 지킨다).
    #[test]
    fn shadow_token_alphas_match_their_design_fractions() {
        for (name, token, fraction) in SHIPPED_SHADOW_TOKENS {
            let expected = (fraction * 255.0).round() as u8;
            assert_eq!(
                token.alpha, expected,
                "{name} 의 alpha 가 디자인 알파({fraction})의 반올림({expected})과 다르다"
            );
        }
    }

    /// `ShadowToken.spread` 는 음수를 **표현**하지만(CSD 의 `-8px`), 그 값을 쓰는 토큰은
    /// 아직 없다. egui 변환이 음수를 담지 못해 조용히 0 이 되므로, 음수 spread 토큰을
    /// 새로 들이는 순간 그 사실이 여기서 드러나야 한다 — 근사값을 만들지 않기 위한
    /// 잠금이다(`ShadowToken::to_egui` 문서). 명부의 완전성은
    /// [`shipped_shadow_token_roster_is_complete`] 가 따로 잰다.
    #[test]
    fn no_shipped_shadow_token_uses_negative_spread() {
        for (name, token, _) in SHIPPED_SHADOW_TOKENS {
            assert!(
                token.spread >= 0.0,
                "{name} 이 음수 spread({})를 쓴다 — egui Shadow(u8)로는 표현할 수 없으니 \
                 근사하지 말고 자체 렌더 경로를 먼저 만들어라",
                token.spread
            );
        }
    }

    /// 음수 spread 는 `as u8` 캐스트에 맡기면 **조용히** 0 으로 saturate 된다. 그
    /// 왜곡이 조용하지 않다는 것을 고정한다 — debug 빌드에서 `to_egui` 가 단언으로
    /// 터진다. 이 성질이 깨지면 위 `no_shipped_shadow_token_uses_negative_spread`
    /// 잠금이 남아도 실수로 들어온 음수가 그림자를 말없이 바꾼다.
    #[cfg(all(feature = "egui-compat", debug_assertions))]
    #[test]
    #[should_panic(expected = "음수 spread")]
    fn to_egui_refuses_negative_spread_in_debug() {
        // 디자인 `--tasty-titlebar-csd-shadow` 의 형상(`0 18px 50px -8px`).
        let t = ShadowToken {
            offset_x: 0.0,
            offset_y: 18.0,
            blur: 50.0,
            spread: -8.0,
            alpha: 140,
        };
        // 반환값은 볼 것이 없다 — 이 호출이 `debug_assert!` 로 터지는 것 자체가 단언이다.
        let _ = t.to_egui();
    }
}
