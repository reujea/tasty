//! UI 색상·치수·그림자의 타입과 계산을 정의한다.
//! 파일 읽기·전역 상태·테마 선택은 tasty-themes가 담당한다.
//! ThemeColors/PartialColors는 저장·병합용이고 Theme는 그리기에 사용할 값을 담는다.

use crate::color::{GpuRgb, HexColor, PremulColor};
use crate::motion::Millis;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tasty_type_geometry::length::LogicalPx;

// ============================================================================
//  SurfaceTheme — surface kind 별 focused/unfocused 색 묶음
// ============================================================================

/// 서피스 종류별 포커스·비포커스 색상. 종류 ID를 키로 저장하며 플러그인도 추가할 수 있다.
/// 터미널의 선택·검색 색상은 Theme의 별도 필드다.
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

/// 등록되지 않은 서피스 종류를 조회했을 때 사용할 기본 색상.
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

/// 밝은 테마에서 강조색 위에 쓰는 흰색 텍스트. 어두운 테마는 crust를 사용한다.
#[allow(clippy::disallowed_methods)] // reason: DTCG 고정 리터럴 색 — 테마 무관
pub const TEXT_ON_ACCENT_LIGHT: HexColor = HexColor::from_rgb(0xff, 0xff, 0xff);

/// macOS 창 버튼에 대응하는 고정색. Catppuccin 팔레트와 구분한다.
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

/// 흐린 항목 공통 톤 (`--tasty-opacity-disabled` = 0.5). 이름과 달리 disabled 컨트롤 신호가
/// 아니다. 꺼진 행·잘라내기 대기·비활성 영역처럼 상호작용할 수 있는 항목을 흐리며,
/// disabled 컨트롤은 opacity 없이 disabled ink를 쓴다. LogicalPx 가 아닌 순수 비율이므로 별도 f32 상수.
pub const OPACITY_DISABLED: f32 = 0.5;

/// 뒤로 물러난(recessed) 요소 공통 톤 (`--tasty-opacity-recessed` = 0.4). 상위 스코프
/// 배너 뒤로 디밍되는 하위 스코프 배너가 이 값(≈60% 투명)을 쓴다. OPACITY_DISABLED
/// 와 같은 이유로 순수 비율 f32 상수.
pub const OPACITY_RECESSED: f32 = 0.4;

/// 진행 중이라 한 단계 물러난 내용의 톤 (`--tasty-opacity-dimmed` = 0.75). 흐려져도
/// 4.5:1 대비를 유지하는 값이며 재로드 중인 배너 본문이 쓴다.
pub const OPACITY_DIMMED: f32 = 0.75;

/// Shift만 누를 때 보조 키 안내를 표시하기까지의 지연. 타이핑 중 불필요한 표시를 줄인다.
/// 다른 보조 키 조합은 기본 지연을 사용한다. 애니메이션 시간이 아니므로 모션 감소 설정과 무관하다.
/// 대응하는 디자인 토큰이 없어 수동으로 정의한다.
pub const MOTION_HOLD_REVEAL_SHIFT_MS: Millis = Millis(1200.0);

/// egui에 의존하지 않는 그림자 표현. alpha는 검정색의 0..=255 불투명도다.
/// 표면별 선택은 docs/design/systems/theme.md#떠-있는-표면의-그림자를 따른다.
/// 음수 spread를 표현할 수 있지만 egui 변환은 지원하지 않는다.
/// 음수 spread가 필요한 CSD 그림자는 임의의 근사값을 쓰지 않고 미구현으로 둔다.
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
    /// 길이를 정수 픽셀로 반올림해 egui 그림자로 변환한다.
    /// 음수 spread는 debug에서 실패하며 release에서는 0으로 제한한다.
    /// 이는 음수 spread 지원이 아니므로 해당 그림자는 별도 렌더 경로가 필요하다.
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

/// 트리거 옆에 뜨는 팝오버·배너·툴팁의 그림자.
/// 디자인 알파 0.4에 255를 곱한 값은 102다.
pub const SHADOW_POPOVER: ShadowToken = ShadowToken {
    offset_x: 0.0,
    offset_y: 6.0,
    blur: 18.0,
    spread: 0.0,
    alpha: 102,
};

/// 창 중앙을 차지하는 모달의 그림자. scrim 유무만으로 선택하지 않는다.
/// 어두운 배경에서도 모달 외곽을 구분하도록 팝오버보다 크게 설정한다.
/// 디자인 알파 0.55에 255를 곱해 반올림한 값은 140이다.
/// 선택 기준은 docs/design/systems/theme.md#떠-있는-표면의-그림자를 따른다.
pub const SHADOW_MODAL: ShadowToken = ShadowToken {
    offset_x: 0.0,
    offset_y: 20.0,
    blur: 60.0,
    spread: 0.0,
    alpha: 140,
};

/// 그림자 없음. design `--tasty-shadow-none`(`none`). 불투명 모드 WebView 페이지와 겹친 상자는 자기
/// 사각형 밖을 그릴 수 없어 그림자를 반만 보이게 되므로 `--tasty-webview-opaque-shadow`가 이 값을 가리킨다.
pub const SHADOW_NONE: ShadowToken = ShadowToken {
    offset_x: 0.0,
    offset_y: 0.0,
    blur: 0.0,
    spread: 0.0,
    alpha: 0,
};

/// design `--tasty-scrim-bg` — 모달/팝업 뒤 무대를 어둡게 덮는 scrim 알파. black 50%
/// straight (테마 무관 고정 검정). `Theme::scrim()` 이 이 값으로 색을 만든다.
pub const SCRIM_ALPHA: u8 = 128;

/// design `--tasty-preset-split-zone-bg` = accent-primary 22%. 프리셋 편집기 경계
/// hover-split 존 밴드 채움 알파(22%×255≈56). accent 색은 테마 가변 → 알파만 파생.
pub const PRESET_SPLIT_ZONE_BG_ALPHA: u8 = 56;

/// `color-mix(in srgb, <accent> 45%, transparent)` 의 알파(45%×255≈115). DAG surface
/// 의 경고/위험 pill 테두리 3 종이 공유한다 — 두 번째 항이 `transparent` 인 합성은
/// 색을 섞지 않고 알파만 낮추는 것과 같다.
pub const DAG_MIX_45_ALPHA: u8 = 115;

/// design `plugins_window.jsx` `PluginAvatar` 보더 `color-mix(in srgb, C 38%, transparent)`
/// 의 알파(38%×255≈97). 두 번째 항이 `transparent` 라 색은 그대로 두고 알파만 낮춘다
/// ([`DAG_MIX_45_ALPHA`] 와 같은 형태).
pub const PLUGIN_AVATAR_BORDER_ALPHA: u8 = 97;

/// design `--tasty-explorer-drop-target-bg` = accent-primary × tint-fill-alpha(12%) 와
/// `transparent` 의 합성. 색은 그대로 두고 알파만 낮춘다(12%×255≈31).
pub const EXPLORER_DROP_TARGET_BG_ALPHA: u8 = 31;

/// design `--tasty-file-drop-overlay-bg` = accent-primary × tint-fill-alpha(12%) 와
/// `transparent` 의 합성. explorer drop target 채움과 같은 식이다(12%×255≈31).
pub const FILE_DROP_OVERLAY_BG_ALPHA: u8 = 31;

/// design `--tasty-file-drop-overlay-border` = accent-primary × tint-border-alpha(36%) 와
/// `transparent` 의 합성. 색은 그대로 두고 알파만 낮춘다(36%×255≈92).
pub const FILE_DROP_OVERLAY_BORDER_ALPHA: u8 = 92;

/// 강조색으로 옅게 채우는 공통 비율. 알파 바이트가 아닌 계산용 f32 계수다.
pub const TINT_FILL_ALPHA: f32 = 0.12;

/// 같은 강조색을 사용하는 테두리의 공통 비율.
pub const TINT_BORDER_ALPHA: f32 = 0.36;

/// 불투명한 두 색의 CSS color-mix(in srgb)에 맞춰 RGB 채널을 보간한다.
/// 반환값의 alpha는 b를 그대로 사용한다.
#[allow(clippy::disallowed_methods)] // reason: 디자인 토큰 color-mix 식의 유일한 구현부
pub(crate) fn mix_srgb(a: HexColor, ratio: f32, b: HexColor) -> HexColor {
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
    /// 첫 실행 화면의 브랜드 제목 크기. 일반 UI 글꼴 상한과 별도로 정한다.
    pub font_size_brand_display: LogicalPx,
    /// Markdown H1의 기본 크기. 본문 렌더러가 하위 제목 크기를 계산한다.
    pub font_size_prose_h1: LogicalPx,
    /// UI 문단의 줄간격 비율. 글꼴 크기에 곱해 줄 높이를 구한다.
    pub line_height_ui: f32,
    /// terminal cell 스케일 — small (12px).
    pub font_size_term_sm: LogicalPx,
    /// terminal cell 스케일 — 기본 (14px).
    pub font_size_term: LogicalPx,
    /// terminal cell 스케일 — large (16px).
    pub font_size_term_lg: LogicalPx,
    pub border_width: LogicalPx,
    /// 대상을 둘러싸는 포커스·선택 링의 굵기. 색상은 용도별로 고른다.
    /// 한쪽 변에만 붙는 선택 표시에는 selection_edge_width를 사용한다.
    pub focus_ring_width: LogicalPx,
    /// painter로 직접 그리는 창 버튼 글리프의 선 굵기. 대응하는 DTCG 치수 토큰은 없다.
    pub icon_stroke_width: LogicalPx,
    pub corner_radius: LogicalPx,
    /// 작은 inner element(키캡 등)용 코너 반경 (2px, design `--tasty-radius-sm`).
    pub corner_radius_sm: LogicalPx,
    /// 배너처럼 떠 있는 패널의 모서리 반경.
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
    /// 빈 상태·오류 안내처럼 화면 가운데에 놓이는 큰 글리프 (design `--tasty-icon-size-lg`).
    pub icon_glyph_size_lg: LogicalPx,
    /// 뷰 상단 도구 막대 높이 (design `--tasty-toolbar-height`). UI 배율을 적용한다.
    pub toolbar_height: LogicalPx,
    /// 목록 행 액션 아이콘의 크기. 다른 아이콘과 함께 UI 배율을 적용한다.
    /// 대응하는 DTCG 치수 토큰은 없다.
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
    // 탭바 치수는 UI 배율을 적용하지 않는다.
    /// 탭바 자체 높이.
    pub tab_bar_height: LogicalPx,
    /// "+" 새 탭 버튼 폰트 크기.
    pub tab_bar_label_font_size: LogicalPx,
    // ── 작업영역 하단 StatusBar 전용 (host UI zoom 영향 받지 않음) ──
    // tab_bar 와 동일하게 px 고정 — with_colors_and_zoom 에서 SIZING 그대로 복사.
    /// 작업영역 하단 StatusBar 높이.
    pub status_bar_height: LogicalPx,
    // ── Titlebar (CSD) 전용 (host UI zoom 영향 받지 않음) ──
    // 창 장식의 치수는 UI 배율과 독립적으로 유지한다.
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
    /// 선택 항목의 한쪽 변에 붙는 띠 두께 (2px).
    pub selection_edge_width: LogicalPx,
    /// tinted 강조 상자(Do / Don't 콜아웃 등)의 앞쪽 한 변 두께 (2px).
    pub tint_edge_width: LogicalPx,
    /// 1px 점선 테두리의 대시 길이 (4px).
    pub border_dash: LogicalPx,
    /// 1px 점선 테두리의 대시 사이 간격 (4px).
    pub border_dash_gap: LogicalPx,
    /// 접힌 레일 상태 점의 배경 고리 두께 (1.5px).
    pub status_dot_ring_width: LogicalPx,
    /// 상단 정렬 모달(command palette) 상단 gap (88px).
    pub overlay_top_offset: LogicalPx,
    /// 한 글 묶음 안에서 라벨 줄과 그 부연 줄 사이 간격 (2px).
    pub label_detail_gap: LogicalPx,
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
    icon_glyph_size_lg: LogicalPx(24.0),
    icon_glyph_size_row_action: LogicalPx(15.0),
    sidebar_logo_size: LogicalPx(22.0),
    sidebar_logo_collapsed_size: LogicalPx(24.0),
    sidebar_wordmark_font_size: LogicalPx(17.0),
    sidebar_section_heading_font_size: LogicalPx(10.0),
    sidebar_button_label_font_size: LogicalPx(11.0),
    sidebar_collapsed_slot_width: LogicalPx(32.0),
    sidebar_collapsed_icon_height: LogicalPx(22.0),
    sidebar_collapsed_workspace_height: LogicalPx(28.0),
    tab_bar_height: LogicalPx(24.0),
    tab_bar_label_font_size: LogicalPx(13.0),
    status_bar_height: LogicalPx(24.0),
    toolbar_height: LogicalPx(32.0),
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
    toast_accent_width: LogicalPx(2.0),
    selection_edge_width: LogicalPx(2.0),
    tint_edge_width: LogicalPx(2.0),
    border_dash: LogicalPx(4.0),
    border_dash_gap: LogicalPx(4.0),
    status_dot_ring_width: LogicalPx(1.5),
    overlay_top_offset: LogicalPx(88.0),
    label_detail_gap: LogicalPx(2.0),
};

// ============================================================================
//  ThemeColors / PartialColors — 직렬화 표현
// ============================================================================

/// 저장 가능한 전체 색상 집합. 파생 오버레이 색은 Theme에 따로 둔다.
/// GPU 변환에는 to_gpu_rgba/to_gpu_rgb를 사용하고 종류별 색상은 surface_themes에 담는다.
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
    /// 현재(active) 검색 매치 안 셀의 글리프 색. 그 칸에서만 SGR 글자색을 덮어쓴다.
    /// 값이 없는 직렬화(이전 host)는 Theme 을 만들 때 도출한다.
    #[serde(default = "crate::search_ink::unset_search_match_active_fg")]
    pub search_match_active_fg: HexColor,

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

/// 사용자 변경분이나 부분 TOML 테마. Some인 색상과 서피스 종류별 변경분만 기본 색에 덮어쓴다.
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
    pub search_match_active_fg: Option<HexColor>,
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
    /// 색상 변경분과 서피스별 변경분을 모두 비운다.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// 색상과 서피스에 적용할 변경분이 없는지 확인한다.
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
        if let Some(v) = p.search_match_active_fg {
            self.search_match_active_fg = v;
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

/// 그리기에 필요한 색·치수·배율·모션 설정. 종류별 색은 surface로 조회한다.
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
    /// 현재(active) 검색 매치 안 셀의 글리프 색. 그 칸에서만 SGR 글자색을 덮어쓴다.
    pub search_match_active_fg: HexColor,
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
    // 타입이 PremulColor라 알파를 한 번 더 곱하는 일반 to_egui()는 컴파일되지 않는다.
    /// 호버 시 배경 오버레이 (~8%).
    pub hover_overlay: PremulColor,
    /// 눌림 시 배경 오버레이 (~12%).
    pub active_overlay: PremulColor,
    /// 구분선 (~8%).
    pub separator: PremulColor,

    // ── 모든 테마 공통 sizing (SIZING 에서 복사) ──
    /// 서브-caption micro-label (kbd / badge / tag / tree·menu meta) — 10px.
    pub font_size_micro: LogicalPx,
    pub font_size_caption: LogicalPx,
    pub font_size_body: LogicalPx,
    pub font_size_heading: LogicalPx,
    pub font_size_max: LogicalPx,
    /// 첫 실행 화면의 브랜드 제목 크기. 일반 UI 글꼴 상한과 별도로 정한다.
    pub font_size_brand_display: LogicalPx,
    /// Markdown H1 크기. 콘텐츠 글꼴이므로 UI zoom 제외.
    pub font_size_prose_h1: LogicalPx,
    /// UI 텍스트(툴팁 등) 줄간격 배수 (1.4, design `--tasty-line-height-ui`). 무차원 비율.
    pub line_height_ui: f32,
    /// terminal cell 스케일 — small (12px).
    /// 콘텐츠 글꼴이므로 UI zoom 제외. 터미널 배율은 별도로 적용한다.
    pub font_size_term_sm: LogicalPx,
    /// terminal cell 스케일 — 기본 (14px).
    /// 콘텐츠 글꼴이므로 UI zoom 제외. 터미널 배율은 별도로 적용한다.
    pub font_size_term: LogicalPx,
    /// terminal cell 스케일 — large (16px).
    /// 콘텐츠 글꼴이므로 UI zoom 제외. 터미널 배율은 별도로 적용한다.
    pub font_size_term_lg: LogicalPx,
    /// 기본 보더 굵기 (1px).
    /// hairline 보더는 UI zoom 제외.
    pub border_width: LogicalPx,
    /// 대상을 둘러싸는 포커스·선택 링의 굵기. 한쪽 변의 선택 표시는 selection_edge_width를 사용한다.
    pub focus_ring_width: LogicalPx,
    /// 창 버튼 글리프의 hairline 굵기. 버튼의 고정 치수에 맞춰 UI zoom 제외.
    pub icon_stroke_width: LogicalPx,
    pub corner_radius: LogicalPx,
    /// 작은 inner element(키캡 등)용 코너 반경 (2px, design `--tasty-radius-sm`).
    pub corner_radius_sm: LogicalPx,
    /// 떠 있는 패널(배너)용 큰 코너 반경 (8px, design `--tasty-radius-8`).
    pub corner_radius_lg: LogicalPx,
    pub item_height_tree: LogicalPx,
    pub item_height_interactive: LogicalPx,
    pub item_height_tab: LogicalPx,
    /// 갤러리·하위 메뉴에서 쓰는 기본 탭 폭. 본체 탭 폭은 설정에서 읽는다.
    /// 탭바 크롬은 UI zoom 제외.
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
    /// 빈 상태·오류 안내처럼 화면 가운데에 놓이는 큰 글리프 (design `--tasty-icon-size-lg`).
    pub icon_glyph_size_lg: LogicalPx,
    /// 뷰 상단 도구 막대 높이 (design `--tasty-toolbar-height`). UI 배율을 적용한다.
    pub toolbar_height: LogicalPx,
    /// 목록 행 액션 아이콘 크기. 대응 DTCG 토큰은 없지만 다른 아이콘처럼 배율을 적용한다.
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
    /// 탭바 크롬은 UI zoom 제외.
    pub tab_bar_height: LogicalPx,
    /// 탭 라벨 폰트 크기.
    /// 탭바 크롬은 UI zoom 제외.
    pub tab_bar_label_font_size: LogicalPx,
    // ── 작업영역 하단 StatusBar 전용 (host UI zoom 영향 받지 않음) ──
    /// 작업영역 하단 StatusBar 높이.
    /// 상태바 크롬은 UI zoom 제외.
    pub status_bar_height: LogicalPx,
    // ── Titlebar (CSD) 전용 (host UI zoom 영향 받지 않음) ──
    /// CSD 타이틀바 높이.
    /// CSD 타이틀바 크롬은 UI zoom 제외.
    pub titlebar_height: LogicalPx,
    /// macOS 신호등(traffic light) 점 지름.
    /// CSD 타이틀바 크롬은 UI zoom 제외.
    pub traffic_size: LogicalPx,
    /// Windows 캡션 버튼(min·max·close) 폭.
    /// CSD 타이틀바 크롬은 UI zoom 제외.
    pub caption_width: LogicalPx,
    /// Linux DE 버튼(min·max·close) 원형 지름.
    /// CSD 타이틀바 크롬은 UI zoom 제외.
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
    /// 선택 항목의 한쪽 변 띠(탭 밑줄·활성 워크스페이스 행·remote attach·preset 목록 행).
    /// hairline 띠이므로 UI zoom 제외.
    pub selection_edge_width: LogicalPx,
    /// tinted 강조 상자의 앞쪽 한 변(Do / Don't 콜아웃). design `--tasty-tint-edge-width`.
    /// hairline 띠이므로 UI zoom 제외.
    pub tint_edge_width: LogicalPx,
    /// 1px 점선 테두리(Add plugin 빈 안내 상자·Scripts Add trigger…)의 대시 길이.
    /// hairline 테두리의 무늬라 border_width와 같이 UI zoom 제외.
    pub border_dash: LogicalPx,
    /// 1px 점선 테두리의 대시 사이 간격. hairline 테두리의 무늬라 UI zoom 제외.
    pub border_dash_gap: LogicalPx,
    /// 접힌 레일 상태 점을 아바타에서 떼어 내는 배경 고리 두께. design
    /// `--tasty-status-dot-ring-width`. hairline이라 UI zoom 제외.
    pub status_dot_ring_width: LogicalPx,
    pub overlay_top_offset: LogicalPx,
    /// 라벨 줄 ↔ 부연 줄 간격. 글자와 함께 커지므로 UI zoom 적용.
    pub label_detail_gap: LogicalPx,

    /// 컴포넌트 접근자가 직접 보관한 치수에 적용할 UI 배율.
    /// 생성자에서 이미 확대한 필드에는 다시 곱하지 않는다.
    pub ui_zoom: f32,

    /// 모션 감소 설정을 위젯에 전달한다. 생성자의 기본값은 false이며 호스트가 설정값을 반영한다.
    pub reduced_motion: bool,

    // ── 라이트/다크 플래그 ──
    pub is_light: bool,

    // ── surface kind 별 색 묶음 ──
    /// `"terminal"`, `"markdown"`, 또는 plugin 등록 id → `SurfaceTheme`.
    /// 호출자는 보통 [`Theme::surface`] 헬퍼를 통해 접근 (없는 id 는 `FALLBACK_SURFACE`).
    pub surface_themes: BTreeMap<String, SurfaceTheme>,
}

/// `is_light` 에 따른 hover/active/separator 도출. premultiplied sRGB 바이트다.
const fn derive_overlays(is_light: bool) -> (PremulColor, PremulColor, PremulColor) {
    if is_light {
        (
            PremulColor::from_premultiplied(0, 0, 0, 20), // black ~8%
            PremulColor::from_premultiplied(0, 0, 0, 31), // black ~12%
            PremulColor::from_premultiplied(0, 0, 0, 20), // black ~8%
        )
    } else {
        (
            PremulColor::from_premultiplied(20, 20, 20, 20), // white ~8%
            PremulColor::from_premultiplied(31, 31, 31, 31), // white ~12%
            PremulColor::from_premultiplied(20, 20, 20, 20), // white ~8%
        )
    }
}

impl Theme {
    /// 색상과 밝기 모드로 Theme를 만든다. 배율은 1, 모션 감소는 false다.
    pub fn with_colors(c: ThemeColors, is_light: bool) -> Self {
        Self::with_colors_and_zoom(c, is_light, 1.0)
    }

    /// 색상과 밝기 모드에 UI 배율을 적용한다. 배율 적용 치수는 반올림한다.
    /// 제외 필드는 각 필드 설명과 zoom_exempt_fields_guard의 EXEMPT 목록을 따른다.
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
            search_match_active_fg: c.resolved_search_match_active_fg(),
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
            // 창 버튼의 고정 기하와 선 굵기를 함께 유지한다.
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
            icon_glyph_size_lg: zoomed(SIZING.icon_glyph_size_lg),
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
            // 작업영역 하단 StatusBar — tab_bar 와 동일하게 zoom 미적용.
            status_bar_height: SIZING.status_bar_height,
            toolbar_height: zoomed(SIZING.toolbar_height),
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
            selection_edge_width: SIZING.selection_edge_width,
            tint_edge_width: SIZING.tint_edge_width,
            border_dash: SIZING.border_dash,
            border_dash_gap: SIZING.border_dash_gap,
            status_dot_ring_width: SIZING.status_dot_ring_width,
            overlay_top_offset: zoomed(SIZING.overlay_top_offset),
            label_detail_gap: zoomed(SIZING.label_detail_gap),
            ui_zoom,
            reduced_motion: false,
            is_light,
            surface_themes: c.surface_themes,
        }
    }

    /// 서피스 종류의 색을 조회한다. 등록되지 않은 ID는 FALLBACK_SURFACE를 사용한다.
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
            search_match_active_fg: self.search_match_active_fg,
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

    /// 밝기 모드를 유지한 채 새 색상으로 Theme 전체를 다시 만든다.
    /// 배율은 1, 모션 감소는 false로 초기화된다. 밝기 변경은 set_is_light로 따로 처리한다.
    pub fn apply_colors(&mut self, c: &ThemeColors) {
        let next = Self::with_colors(c.clone(), self.is_light);
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

    /// 색상만 ThemeColors로 모은다. 프로세스 간 전달 후 배율과 설정을 별도로 적용할 수 있다.
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
            search_match_active_fg: self.search_match_active_fg,
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

// 단순 색상 별칭은 semantic_color_generated.rs에서 생성한다.
// 아래에는 밝기 모드 분기·오버레이·합성색·OS/브랜드 고정색처럼 별도 계산이 필요한 접근자를 둔다.
impl Theme {
    /// 강조색 위 텍스트. 어두운 테마는 crust, 밝은 테마는 흰색을 사용한다.
    #[inline]
    pub fn text_on_accent(&self) -> HexColor {
        if self.is_light {
            TEXT_ON_ACCENT_LIGHT
        } else {
            self.crust
        }
    }

    /// NeedsInput 알림 배지·점 위의 글자. `semantic.attention-needs-input-fg`는
    /// notify-warning-fg의 별칭이므로 같은 밝기 분기를 따른다.
    #[inline]
    pub fn attention_needs_input_fg(&self) -> HexColor {
        self.notify_warning_fg()
    }

    /// Completion 알림 배지·점 위의 글자. `semantic.attention-completion-fg`는
    /// text-on-accent의 별칭이다.
    #[inline]
    pub fn attention_completion_fg(&self) -> HexColor {
        self.text_on_accent()
    }

    /// 비활성 글자. 밝은 테마는 한 단계 진한 neutral-800(overlay2)을 사용한다.
    /// `semantic.text-disabled`의 Latte remap이다.
    #[inline]
    pub fn text_disabled(&self) -> HexColor {
        if self.is_light {
            self.overlay2
        } else {
            self.overlay1
        }
    }

    /// surface-raised(메뉴·드롭다운 목록) 위의 보조 글자. 밝은 테마는 neutral-1000이
    /// surface0 위에서 4.5:1에 못 미쳐 neutral-1100(text)을 사용한다.
    /// `semantic.text-secondary-raised`의 Latte remap이다.
    #[inline]
    pub fn text_secondary_raised(&self) -> HexColor {
        if self.is_light {
            self.text
        } else {
            self.subtext1
        }
    }
    /// surface-raised(배너 본문) 위의 약한 글자. 밝은 테마는 neutral-900이 surface0 위에서
    /// 4.5:1에 못 미쳐 neutral-1100(text)을 사용한다. `semantic.text-muted-raised`의 Latte remap이다.
    #[inline]
    pub fn text_muted_raised(&self) -> HexColor {
        if self.is_light {
            self.text
        } else {
            self.subtext0
        }
    }

    // ── disabled 컨트롤 (state-disabled-*) — opacity 없이 중립 상자와 disabled ink ──

    /// disabled 컨트롤의 라벨·글리프 ink. `semantic.state-disabled-fg` → text-disabled.
    #[inline]
    pub fn state_disabled_fg(&self) -> HexColor {
        self.text_disabled()
    }

    /// disabled 컨트롤의 채움. `semantic.state-disabled-fill` → surface-raised.
    #[inline]
    pub fn state_disabled_fill(&self) -> HexColor {
        self.surface_raised()
    }

    /// disabled 컨트롤의 테두리. `semantic.state-disabled-border` → border-default.
    #[inline]
    pub fn state_disabled_border(&self) -> HexColor {
        self.border_default()
    }

    /// raised 바탕 위 disabled 컨트롤의 채움. `semantic.state-disabled-fill-raised` → bg-panel.
    #[inline]
    pub fn state_disabled_fill_raised(&self) -> HexColor {
        self.bg_panel()
    }

    /// raised 바탕 위 disabled 컨트롤의 테두리. `semantic.state-disabled-border-raised` → border-strong.
    #[inline]
    pub fn state_disabled_border_raised(&self) -> HexColor {
        self.border_strong()
    }

    // ── 오버레이 (overlay-*) — is_light 에서 도출된 필드를 semantic 이름으로 ──
    #[inline]
    pub fn overlay_hover(&self) -> PremulColor {
        self.hover_overlay
    }
    #[inline]
    pub fn overlay_active(&self) -> PremulColor {
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
    /// `scrim-bg`. 무관 고정 검정 + [`SCRIM_ALPHA`]. 갤러리 dialog 레시피와 동일 토큰.
    #[inline]
    #[allow(clippy::disallowed_methods)] // reason: 테마 무관 고정 scrim 색 정의
    pub fn scrim(&self) -> HexColor {
        HexColor::from_rgba(0, 0, 0, SCRIM_ALPHA)
    }

    /// 프리셋 경계의 분할 영역을 accent-primary 22%로 채운다.
    #[inline]
    pub fn preset_split_zone_bg(&self) -> HexColor {
        self.accent_primary().with_alpha(PRESET_SPLIT_ZONE_BG_ALPHA)
    }

    /// 드래그 중 놓을 수 있는 폴더 행·셀·트리 노드·목록 본문의 채움. design
    /// `--tasty-explorer-drop-target-bg` ([`EXPLORER_DROP_TARGET_BG_ALPHA`]).
    #[inline]
    pub fn explorer_drop_target_bg(&self) -> HexColor {
        self.accent_primary()
            .with_alpha(EXPLORER_DROP_TARGET_BG_ALPHA)
    }

    /// 창 파일 드롭 오버레이의 채움. design `--tasty-file-drop-overlay-bg`
    /// ([`FILE_DROP_OVERLAY_BG_ALPHA`]).
    #[inline]
    pub fn file_drop_overlay_bg(&self) -> HexColor {
        self.accent_primary().with_alpha(FILE_DROP_OVERLAY_BG_ALPHA)
    }

    /// 창 파일 드롭 오버레이의 1px 테두리. design `--tasty-file-drop-overlay-border`
    /// ([`FILE_DROP_OVERLAY_BORDER_ALPHA`]).
    #[inline]
    pub fn file_drop_overlay_border(&self) -> HexColor {
        self.accent_primary()
            .with_alpha(FILE_DROP_OVERLAY_BORDER_ALPHA)
    }

    // 매니페스트에 카테고리가 없으므로 디자인의 기본 카테고리색인 accent_primary를 사용한다.

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
    /// `state-dim-opacity`. number 토큰이라 생성기가 접근자를 만들지 않는다.
    #[inline]
    pub fn preset_cfg_dim_opacity(&self) -> f32 {
        self.state_dim_opacity()
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
    pub fn titlebar_border(&self) -> PremulColor {
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

    /// 흐린 항목의 opacity (0.5). `semantic.state-dim-opacity`. 꺼진 행·잘라내기 대기·
    /// 편집 중 비활성 영역처럼 상호작용할 수 있는 항목이 자기 색을 유지한 채 흐려진다.
    /// disabled 컨트롤에는 쓰지 않는다(`state_disabled_*`).
    #[inline]
    pub fn state_dim_opacity(&self) -> f32 {
        OPACITY_DISABLED
    }

    /// recessed(뒤로 물러난) 요소 공통 opacity (0.4). 상위 스코프 배너 뒤로 디밍되는
    /// 하위 스코프 배너가 이 값(≈60% 투명)을 쓴다. `--tasty-opacity-recessed`.
    #[inline]
    pub fn opacity_recessed(&self) -> f32 {
        OPACITY_RECESSED
    }

    /// 진행 중이라 물러난 내용의 opacity (0.75). `--tasty-opacity-dimmed`.
    #[inline]
    pub fn opacity_dimmed(&self) -> f32 {
        OPACITY_DIMMED
    }

    /// 흐려진 DAG 노드의 opacity. `semantic.node-dim-opacity`.
    /// 노드는 선택할 수 있고 글자를 읽어야 해서 Latte에서는 흐리지 않는다.
    #[inline]
    pub fn node_dim_opacity(&self) -> f32 {
        if self.is_light { 1.0 } else { OPACITY_DIMMED }
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
    /// 배너 우상단 TTL 카운트다운 색. `--tasty-banner-countdown-fg` → `text-muted-raised`.
    #[inline]
    pub fn banner_countdown_fg(&self) -> HexColor {
        self.text_muted_raised()
    }
    /// 떠 있는 패널 그림자. `--tasty-banner-shadow` → `--tasty-shadow-popover`.
    #[inline]
    pub fn shadow_popover(&self) -> ShadowToken {
        SHADOW_POPOVER
    }
    /// 창 중앙을 차지하는 모달의 그림자. scrim 유무만으로 선택하지 않는다.
    /// 표면별 기준은 docs/design/systems/theme.md#떠-있는-표면의-그림자를 따른다.
    #[inline]
    pub fn shadow_modal(&self) -> ShadowToken {
        SHADOW_MODAL
    }
    /// 그림자 없음. `--tasty-webview-opaque-shadow` → `--tasty-shadow-none`. 불투명 모드 WebView
    /// 페이지와 겹친 팝업·드롭다운 상자가 쓴다.
    #[inline]
    pub fn shadow_none(&self) -> ShadowToken {
        SHADOW_NONE
    }

    // ── 모션 (modifier-hint) ──
    /// Shift 단독 입력의 안내 표시 지연. 모션 감소 설정과 무관하다.
    #[inline]
    pub fn motion_hold_reveal_shift(&self) -> Millis {
        MOTION_HOLD_REVEAL_SHIFT_MS
    }

    /// 워크스페이스 행의 상태 점 슬롯 폭. attached ring 전체를 담도록 반올림한 점 지름과
    /// ring 폭·offset을 더해 만든다. 슬롯만 따로 반올림하면 0.85에서 ring(15)이 슬롯(14)을
    /// 넘으므로 `--tasty-workspace-dot-slot`(size-16)의 배율 값을 쓰지 않는다.
    /// 0.85 → 15 · 1 → 16 · 1.2 → 18.
    #[inline]
    pub fn workspace_dot_slot(&self) -> LogicalPx {
        LogicalPx(
            self.status_dot_size.value()
                + 2.0
                    * (self.status_dot_attached_ring_width().value()
                        + self.status_dot_attached_ring_offset().value()),
        )
    }

    // 보조 키 안내 패널의 치수와 색상.
    /// 기본 너비 (180px). `--tasty-modhint-width` → `--tasty-size-180`.
    /// 열린 사이드바 폭(`AppearanceSettings::scaled_sidebar_width`의 medium 값 180)과 정렬.
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
    /// 빈 조합 섹션은 채워진 섹션보다 간격을 좁게 해 불필요한 높이를 줄인다.
    #[inline]
    pub fn modhint_empty_row_gap(&self) -> LogicalPx {
        LogicalPx((3.0 * self.ui_zoom).round())
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
    /// 스트립 하단 / 조합 헤더 하단 구분선. `--tasty-modhint-separator` → `separator`.
    #[inline]
    pub fn modhint_separator(&self) -> PremulColor {
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
    /// 특수 역할 행 leading 글리프 색. `--tasty-modhint-role-fg` → `accent-primary-on-active`.
    #[inline]
    pub fn modhint_role_fg(&self) -> HexColor {
        self.accent_primary_on_active()
    }
    /// 액션/역할 행 텍스트 색. `--tasty-modhint-row-fg` → `text-secondary`.
    #[inline]
    pub fn modhint_row_fg(&self) -> HexColor {
        self.text_secondary()
    }
    /// 바인딩이 없는 행은 아이콘·키캡 없이 text-muted로 표시한다.
    #[inline]
    pub fn modhint_empty_fg(&self) -> HexColor {
        self.text_muted()
    }
    /// plugin 행 leading agent dot 색. `--tasty-modhint-agent-dot` → `accent-agent`.
    #[inline]
    pub fn modhint_agent_dot(&self) -> HexColor {
        self.accent_agent()
    }

    // 대응 디자인 토큰이 없는 치수도 UI 배율을 함께 적용하도록 접근자로 관리한다.
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
    /// 스케일 위의 값이 아니므로 토큰으로 스냅하지 않는다(docs/design/systems/theme.md#토큰에-없는-값과-배율).
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
    /// 파일 선택 팝업의 설명문 최대 폭 (340px) — 이 폭에서 줄바꿈한다.
    #[inline]
    pub fn file_picker_note_max_width(&self) -> LogicalPx {
        LogicalPx((340.0 * self.ui_zoom).round())
    }
    /// 로딩 화면 브랜드 아이콘 크기. 같은 스택의 글꼴·간격과 함께 배율을 적용한다.
    #[inline]
    pub fn loading_screen_wordmark_icon_size(&self) -> LogicalPx {
        LogicalPx((64.0 * self.ui_zoom).round())
    }
    /// 로딩 화면의 브랜드 글꼴 크기. 사이드바 워드마크와 용도가 달라 값을 공유하지 않는다.
    /// 값은 `component.loading-lockup-font-size` 토큰에서 온다.
    #[inline]
    pub fn loading_screen_wordmark_font_size(&self) -> LogicalPx {
        self.loading_lockup_font_size()
    }
    /// 로딩 화면 스피너 크기.
    #[inline]
    pub fn loading_screen_spinner_size(&self) -> LogicalPx {
        LogicalPx((32.0 * self.ui_zoom).round())
    }
    /// 진행 문구가 없어도 같은 높이를 확보한다. 안쪽 글자와 함께 배율을 적용한다.
    #[inline]
    pub fn loading_screen_phase_slot_height(&self) -> LogicalPx {
        LogicalPx((16.0 * self.ui_zoom).round())
    }
    // sidebar-category-header·autocomplete·md-table·drilldown·listctrl 접근자는 생성 파일에 있다.
    // 글꼴 굵기 토큰은 생성하지 않는다. egui 글꼴 등록에 합성 굵기가 없어 색으로 강조를 구분한다.
}

// ============================================================================
//  Tests
// ============================================================================

#[cfg(test)]
mod tests {
    // 색상 병합·변환 자체를 검사하므로 합성 색상 사용을 허용한다.
    #![allow(clippy::disallowed_methods)]

    use super::*;

    /// 모든 필드에 같은 색을 넣어 팔레트와 독립적으로 병합·변환을 검사한다.
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
            search_match_active_fg: c,
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

    /// 서로 다른 색을 넣어 접근자가 잘못된 필드에 연결된 경우를 검출한다.
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

    /// 의미별 색상 접근자가 지정한 필드에 연결되는지 확인한다.
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

        // 알림 종류
        assert_eq!(th.attention_needs_input(), th.yellow);
        assert_eq!(th.attention_completion(), th.blue);
        assert_eq!(th.attention_needs_input_fg(), th.text_on_accent());
        assert_eq!(th.attention_completion_fg(), th.text_on_accent());

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
        assert_eq!(th.text_secondary_raised(), th.subtext1);
        assert_eq!(th.text_muted_raised(), th.subtext0);
        assert_eq!(th.text_placeholder(), th.placeholder);

        // 상태 표시
        assert_eq!(th.status_idle(), th.overlay0);
        assert_eq!(th.text_on_accent(), th.crust); // dark(Mocha): neutral-0=crust
        // light(Latte): accent 위 텍스트는 절대색 white 로 role-remap.
        let light = Theme::with_colors(distinct_colors(), true);
        assert_eq!(light.text_on_accent(), TEXT_ON_ACCENT_LIGHT);
        assert_ne!(light.text_on_accent(), light.crust);
        // light(Latte): 비활성 글자는 neutral-800(overlay2)으로 role-remap.
        assert_eq!(light.text_disabled(), light.overlay2);
        assert_ne!(light.text_disabled(), light.overlay1);
        // light(Latte): surface-raised 위 보조 글자는 neutral-1100(text)으로 role-remap.
        assert_eq!(light.text_secondary_raised(), light.text);
        assert_ne!(light.text_secondary_raised(), light.subtext1);
        assert_eq!(light.text_muted(), light.subtext1);
        assert_ne!(light.text_muted(), light.subtext0);
        assert_eq!(light.text_muted_raised(), light.text);
        assert_ne!(light.text_muted_raised(), light.subtext0);

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
    /// 각 배경이 지정한 색상 필드에 연결되는지 확인한다.
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

        // 줄무늬·행·헤더·격자선이 서로 다른 색상 필드를 사용하는지 확인한다.
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

    /// 지원 배율에서 반경은 변하고 고정 선 굵기는 유지되는지 확인한다.
    /// 1px와 2px는 반올림 결과가 같아 값만으로 zoomed 호출 여부를 구분할 수 없다.
    /// 여기 배율 목록은 사본이며 지원 집합 변경은 tasty-settings의 별도 검사가 알린다.
    #[test]
    fn zoom_cost_differs_by_axis() {
        /// 의존 방향 때문에 복사한 배율 목록. tasty-settings의 the_supported_ui_scale_set_is_pinned와 맞춘다.
        const SUPPORTED_ZOOMS: [f32; 3] = [0.85, 1.0, 1.2];

        let base = Theme::with_colors_and_zoom(dummy_colors(), false, 1.0);

        for z in SUPPORTED_ZOOMS {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);

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

            // 1.5px 선은 확대하면 값이 달라지므로 고정 여부를 확인할 수 있다.
            assert_eq!(
                t.icon_stroke_width, base.icon_stroke_width,
                "icon_stroke_width는 배율 {z}에서도 고정되어야 한다"
            );

            // 지원 배율에서 반올림 결과가 변하지 않는 작은 값을 확인한다.
            for (name, v) in [
                ("border_width", base.border_width),
                ("focus_ring_width", base.focus_ring_width),
                ("corner_radius_sm", base.corner_radius_sm),
            ] {
                assert_eq!(
                    LogicalPx((v.value() * z).round()),
                    v,
                    "{name}({v:?})의 반올림 결과가 배율 {z}에서 달라졌다"
                );
            }
        }

        let small = Theme::with_colors_and_zoom(dummy_colors(), false, 0.85);
        let large = Theme::with_colors_and_zoom(dummy_colors(), false, 1.2);
        assert_ne!(small.corner_radius, base.corner_radius);
        assert_ne!(large.corner_radius, base.corner_radius);
    }

    /// 슬롯은 점·ring을 각각 반올림한 뒤 더한다. 16에 배율을 곱해 반올림하면 0.85에서 14가 되어
    /// ring 전체(15)를 담지 못한다.
    #[test]
    fn workspace_dot_slot_is_derived_after_rounding() {
        for (z, want) in [(0.85, 15.0), (1.0, 16.0), (1.2, 18.0)] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);
            assert_eq!(t.workspace_dot_slot(), LogicalPx(want), "배율 {z}");
            let ring = t.status_dot_size.value()
                + 2.0
                    * (t.status_dot_attached_ring_width().value()
                        + t.status_dot_attached_ring_offset().value());
            assert!(
                ring <= t.workspace_dot_slot().value(),
                "배율 {z}에서 ring {ring}이 슬롯을 넘는다"
            );
        }
    }

    /// 권한 상자 배치 경계는 디자인 `calc()`처럼 반올림된 칸·여백·테두리를 더한 값이다.
    /// 배율 1에서 디자인의 242·186이고, 다른 배율에서도 그 부분들로 그린 상자 폭과 같다.
    #[test]
    fn perm_box_layout_bounds_are_the_sum_of_their_rounded_parts() {
        for z in [0.85, 1.0, 1.25, 1.5] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);
            let frame = 2.0 * (t.spacing_sm.value() + t.border_width.value());
            let cell = t.explorer_props_perm_cell_width().value();
            assert_eq!(
                t.explorer_props_perm_grid_min_width(),
                LogicalPx(4.0 * cell + frame),
                "배율 {z}"
            );
            assert_eq!(
                t.explorer_props_perm_stack_min_width(),
                LogicalPx(3.0 * cell + frame),
                "배율 {z}"
            );
        }
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.0);
        assert_eq!(t.explorer_props_perm_grid_min_width(), LogicalPx(242.0));
        assert_eq!(t.explorer_props_perm_stack_min_width(), LogicalPx(186.0));
    }

    /// 레일 점 고리는 hairline이라 배율과 무관하게 1.5다. 배율 경로로 반올림하면 2가 된다.
    #[test]
    fn rail_dot_ring_stays_one_and_a_half_at_every_zoom() {
        for z in [0.85, 1.0, 1.2, 1.5, 2.0] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);
            assert_eq!(t.status_dot_ring_width(), LogicalPx(1.5), "배율 {z}");
        }
    }

    /// 한쪽 변 선택 띠는 hairline이라 배율과 무관하게 2다.
    #[test]
    fn selection_edge_bars_stay_two_at_every_zoom() {
        for z in [0.85, 1.0, 1.2, 1.5, 2.0] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);
            assert_eq!(t.selection_edge_width, LogicalPx(2.0), "배율 {z}");
            assert_eq!(t.tint_edge_width, LogicalPx(2.0), "배율 {z}");
            assert_eq!(
                t.workspace_row_active_bar_width(),
                LogicalPx(2.0),
                "배율 {z}"
            );
            assert_eq!(t.tab_indicator_width(), LogicalPx(2.0), "배율 {z}");
            assert_eq!(t.listctrl_selected_bar_width(), LogicalPx(2.0), "배율 {z}");
        }
    }

    /// 점선 무늬는 1px 테두리와 같이 배율과 무관하게 4 / 4다.
    #[test]
    fn border_dash_pattern_stays_four_at_every_zoom() {
        for z in [0.85, 1.0, 1.2, 1.5, 2.0] {
            let t = Theme::with_colors_and_zoom(dummy_colors(), false, z);
            assert_eq!(t.border_dash, LogicalPx(4.0), "배율 {z}");
            assert_eq!(t.border_dash_gap, LogicalPx(4.0), "배율 {z}");
        }
    }

    /// 탭 밑줄·목록 선택 막대·워크스페이스 활성 막대는 selection-edge-width의 alias다.
    /// 별도 필드를 읽으면 selection-edge 값이 바뀔 때 혼자 남는다.
    #[test]
    fn selection_edge_aliases_read_the_selection_edge_field() {
        let mut t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.0);
        t.selection_edge_width = LogicalPx(5.0);
        assert_eq!(t.tab_indicator_width(), LogicalPx(5.0));
        assert_eq!(t.listctrl_selected_bar_width(), LogicalPx(5.0));
        assert_eq!(t.workspace_row_active_bar_width(), LogicalPx(5.0));
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

    /// CSD 타이틀바의 고정 치수를 확인한다.
    #[test]
    fn titlebar_sizing_tokens_fixed() {
        let t = Theme::with_colors(dummy_colors(), false);
        assert_eq!(t.titlebar_height.value(), 36.0);
        assert_eq!(t.traffic_size.value(), 12.0);
        assert_eq!(t.caption_width.value(), 46.0);
        assert_eq!(t.window_button_size.value(), 24.0);
    }

    /// 타이틀바 색상 접근자가 지정된 의미별 색상에 연결되는지 확인한다.
    #[test]
    fn titlebar_color_accessors_map_to_semantics() {
        let th = Theme::with_colors(distinct_colors(), false);
        assert_eq!(th.titlebar_bg(), th.bg_app());
        assert_eq!(th.titlebar_bg_inactive(), th.bg_sidebar());
        assert_eq!(th.titlebar_border(), th.separator);
        assert_eq!(th.titlebar_fg(), th.text_secondary());
        assert_eq!(th.titlebar_fg_inactive(), th.text_muted());
    }

    /// 창 닫기 버튼의 고정 색을 확인한다.
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
        assert_eq!(t.icon_glyph_size_lg.value(), 24.0);
        assert_eq!(t.toolbar_height.value(), 32.0);
        assert_eq!(t.icon_glyph_size_row_action.value(), 15.0);
    }

    /// 두 테마에서 상태바 아이콘은 같은 배율을 적용하고 바 높이는 고정되는지 확인한다.
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

    /// 컴포넌트 치수가 배율 1 외에서도 확대되는지 확인한다. 고정값으로 바뀌는 오류를 검출한다.
    #[test]
    fn component_dimensions_without_design_tokens_follow_ui_zoom() {
        let at = |z: f32| Theme::with_colors_and_zoom(dummy_colors(), false, z);
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
            // 이 목록의 값은 지원 배율을 달리하면 반올림 결과도 달라져야 한다.
            assert!(
                f(&large).value() > f(&base).value() && f(&base).value() > f(&small).value(),
                "{name} 이 배율을 안 탄다"
            );
        }
    }

    /// 행 액션 아이콘의 크기도 UI 배율을 적용해 반올림하는지 확인한다.
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
        assert_eq!(dark.state_dim_opacity(), 0.5);
        assert_eq!(dark.opacity_dimmed(), 0.75);
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

    /// em 자간 접근자는 넘겨받은 (이미 배율이 적용된) 글자 크기에 비례하고 배율을 다시 곱하지 않는다.
    #[test]
    fn tracking_scales_with_the_given_font_size_only() {
        let base = Theme::with_colors(dummy_colors(), false);
        let zoomed = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        let at = |t: &Theme| t.letter_spacing_caps(t.font_size_micro).value();
        assert!((at(&base) - 0.4).abs() < 1e-6);
        assert!((at(&zoomed) - 0.6).abs() < 1e-6);
        assert_eq!(
            zoomed.sidebar_section_heading_tracking(zoomed.font_size_micro),
            zoomed.letter_spacing_caps_wide(zoomed.font_size_micro)
        );
        let wide = |t: &Theme| t.letter_spacing_caps_wide(t.font_size_micro).value();
        assert!((wide(&base) - 0.7).abs() < 1e-6);
        assert!((wide(&zoomed) - 1.05).abs() < 1e-6);
        assert_eq!(
            zoomed.table_header_tracking(zoomed.font_size_caption),
            zoomed.letter_spacing_caps(zoomed.font_size_caption)
        );
    }

    #[test]
    fn set_is_light_swaps_overlays() {
        let mut t = Theme::with_colors(dummy_colors(), false);
        let dark_hover = t.hover_overlay;
        t.set_is_light(true);
        assert!(t.is_light);
        assert_ne!(t.hover_overlay, dark_hover);
        // 라이트 오버레이는 RGB 가 0
        assert_eq!(t.hover_overlay.premultiplied_bytes()[0], 0);
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

    /// 배율 1에서 대표 컴포넌트 치수와 색상 연결을 확인한다. 픽셀 비교를 대신하지는 않는다.
    #[test]
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 반복 assert_eq 테스트 — clippy 과대계상, rca cognitive 0
    fn component_accessors_invariant_at_zoom_one() {
        let t = Theme::with_colors(dummy_colors(), false);
        assert_eq!(t.button_height_lg().value(), 32.0); // control.rs CONTROL_HEIGHT_LG
        assert_eq!(t.checkbox_size().value(), 16.0); // toggle.rs BOX
        assert_eq!(t.switch_track_width().value(), 28.0); // toggle.rs SWITCH_W
        assert_eq!(t.switch_track_height().value(), 16.0); // toggle.rs SWITCH_H
        assert_eq!(t.switch_thumb_size().value(), 12.0); // toggle.rs SWITCH_THUMB
        assert_eq!(t.switch_thumb_inset().value(), 2.0); // toggle.rs SWITCH_INSET
        assert_eq!(t.tag_size().value(), 16.0); // chip.rs TAG_HEIGHT
        assert_eq!(t.tag_dot_size().value(), 8.0); // chip.rs TAG_DOT
        assert_eq!(t.kbd_gap().value(), 4.0); // chip.rs KBD_GAP
        assert_eq!(t.kbd_shadow_depth().value(), 2.0); // chip.rs KBD_BOTTOM_BORDER
        assert_eq!(t.select_chevron_room().value(), 28.0); // select.rs CHEVRON_PAD
        assert_eq!(t.port_columns_menu_min_width().value(), 180.0);
        assert_eq!(t.port_state_menu_min_width().value(), 216.0);
        assert_eq!(t.port_state_menu_max_height().value(), 168.0);
        assert_eq!(t.remote_tool_header_min_height().value(), 26.0);
        assert_eq!(t.tutorial_topic_body_max_height().value(), 200.0);
        assert_eq!(t.plugins_header_height().value(), 48.0);
        assert_eq!(t.plugins_side_panel_width().value(), 240.0);
        assert_eq!(t.file_picker_note_max_width().value(), 340.0);
        assert_eq!(t.button_gap().value(), t.spacing_sm.value()); // button gap
        assert_eq!(t.button_radius().value(), t.corner_radius.value());
        assert_eq!(t.input_height().value(), t.item_height_interactive.value());
        assert_eq!(t.tree_row_height().value(), t.item_height_tree.value()); // =22
        assert_eq!(
            t.menu_item_height().value(),
            t.item_height_interactive.value()
        );
        assert_eq!(t.status_dot_size().value(), 8.0);
        assert_eq!(t.button_primary_bg(), t.accent_primary());
        assert_eq!(t.input_border_focus(), t.border_focus());
        assert_eq!(t.status_dot_success(), t.accent_success());
        assert_eq!(t.tree_row_fg_active(), t.text_primary());
        assert_eq!(t.table_header_fg(), t.text_muted());
    }

    /// 배율 1.5에서 직접 치수와 위임한 필드가 올바르게 확대되는지 확인한다.
    #[test]
    fn component_dim_accessors_scale_with_zoom() {
        let t = Theme::with_colors_and_zoom(dummy_colors(), false, 1.5);
        assert_eq!(t.button_height_lg().value(), 48.0); // 32 * 1.5
        assert_eq!(t.checkbox_size().value(), 24.0); // 16 * 1.5
        assert_eq!(t.switch_track_width().value(), 42.0); // 28 * 1.5
        assert_eq!(t.tag_size().value(), 24.0); // 16 * 1.5
        assert_eq!(t.kbd_gap().value(), 6.0); // 4 * 1.5
        assert_eq!(t.select_chevron_room().value(), 42.0); // 28 * 1.5
        assert_eq!(t.button_gap().value(), t.spacing_sm.value()); // 12
        assert_eq!(t.tree_row_height().value(), t.item_height_tree.value()); // 33
        assert_eq!(t.input_height().value(), 42.0); // control-height 28 * 1.5
    }

    /// 아래 글꼴 필드가 검사한 배율에서 정수 픽셀로 반올림되는지 확인한다.
    /// 다른 글꼴 필드나 검사하지 않은 배율까지 보장하지 않는다.
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
                    "{name}의 배율 {zoom} 적용 결과가 정수가 아니다: {v}"
                );
            }
        }
    }

    /// 알파와 음수 spread 검사가 공유하는 그림자 토큰 목록.
    /// 실제 ShadowToken 상수와 같은 목록인지 별도 소스 검사로 확인한다.
    const SHIPPED_SHADOW_TOKENS: &[(&str, ShadowToken, f32)] = &[
        ("SHADOW_POPOVER", SHADOW_POPOVER, 0.4),
        ("SHADOW_MODAL", SHADOW_MODAL, 0.55),
        ("SHADOW_NONE", SHADOW_NONE, 0.0),
    ];

    /// ShadowToken 상수와 검사 목록을 이름으로 대조해 빠진 토큰을 찾는다.
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
            "theme.rs에서 ShadowToken 상수를 찾지 못했다. 파일 경로와 선언 형태를 확인한다."
        );
        let mut listed: Vec<String> = SHIPPED_SHADOW_TOKENS
            .iter()
            .map(|(n, _, _)| (*n).to_string())
            .collect();
        listed.sort();
        assert_eq!(
            found, listed,
            "theme.rs의 ShadowToken 상수와 SHIPPED_SHADOW_TOKENS가 다르다. 빠진 토큰은 알파·spread 검사 대상에 포함되지 않는다."
        );
    }

    /// 디자인 알파에 255를 곱해 반올림한 값과 비교한다.
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

    /// drop target 채움 알파는 tint-fill-alpha 계수를 바이트로 반올림한 값이다.
    #[test]
    fn explorer_drop_target_bg_alpha_follows_the_tint_fill_fraction() {
        let expected = (TINT_FILL_ALPHA * 255.0).round() as u8;
        assert_eq!(EXPLORER_DROP_TARGET_BG_ALPHA, expected);
        let th = Theme::with_colors(distinct_colors(), false);
        assert_eq!(
            th.explorer_drop_target_bg(),
            th.accent_primary().with_alpha(expected)
        );
    }

    /// 파일 드롭 오버레이 채움·테두리 알파는 tint-fill·tint-border 계수를 바이트로 반올림한 값이다.
    #[test]
    fn file_drop_overlay_alphas_follow_the_tint_fractions() {
        let fill = (TINT_FILL_ALPHA * 255.0).round() as u8;
        let border = (TINT_BORDER_ALPHA * 255.0).round() as u8;
        assert_eq!(FILE_DROP_OVERLAY_BG_ALPHA, fill);
        assert_eq!(FILE_DROP_OVERLAY_BORDER_ALPHA, border);
        let th = Theme::with_colors(distinct_colors(), false);
        assert_eq!(
            th.file_drop_overlay_bg(),
            th.accent_primary().with_alpha(fill)
        );
        assert_eq!(
            th.file_drop_overlay_border(),
            th.accent_primary().with_alpha(border)
        );
    }

    /// egui 변환이 음수 spread를 지원하지 않으므로 현재 토큰에서 사용하지 않는지 확인한다.
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

    /// debug 빌드에서 음수 spread의 egui 변환이 패닉하는지 확인한다.
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
        // 반환값이 아니라 debug_assert의 패닉을 확인한다.
        let _ = t.to_egui();
    }
}
