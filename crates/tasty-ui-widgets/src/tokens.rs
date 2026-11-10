//! 공용 위젯의 구조 치수. 같은 의미의 값은 SIZING을 참조한다.
//! 색과 글꼴은 Theme에서 읽고, 대응 토큰이 없는 치수는 아래에 이유와 함께 둔다.

use tasty_type_appearance::theme::SIZING;
use tasty_type_geometry::length::LogicalPx;

/// 좌측 sub-menu 패널의 고정 폭 (logical px). = `SIZING.tab_width`.
pub const SUB_TAB_PANEL_WIDTH: f32 = SIZING.tab_width.0;

/// 좌측 패널 Frame 의 inner margin (px, symmetric). = `SIZING.spacing_sm`.
pub const PANEL_INNER_MARGIN: i8 = SIZING.spacing_sm.0 as i8;

/// 좌측 패널 Frame 의 corner radius. = `SIZING.corner_radius`.
pub const PANEL_CORNER_RADIUS: f32 = SIZING.corner_radius.0;

/// 좌측 패널 Frame 의 stroke 굵기. = `SIZING.border_width`.
pub const PANEL_STROKE_WIDTH: f32 = SIZING.border_width.0;

/// 좌·우 패널 사이 horizontal spacing. = `SIZING.spacing_sm`.
pub const PANEL_SPACING: f32 = SIZING.spacing_sm.0;

/// 탭 컨텐츠 영역의 inner padding (px, 4 면 동일). = `SIZING.spacing_lg`.
/// settings 모달 본체와 갤러리의 layout idiom 공통 표준.
pub const TAB_CONTENT_PADDING: i8 = SIZING.spacing_lg.0 as i8;

// 구조 보정 간격은 UI 배율을 적용하지 않는다.
// 1px·2px는 지원 배율에서 반올림 결과가 같고, 3px·4px도 고정 보정값으로 유지한다.
// 요소 크기처럼 배율을 따라야 하는 값은 이 상수 대신 Theme 접근자로 관리한다.

/// 구조 간격 1px = DTCG `primitive.size-1`.
/// 예: 사이드바 WorkspaceRow subtitle 의 margin-top (디자인 chrome.jsx
/// `marginTop: var(--tasty-size-1)`).
pub const STRUCT_GAP_1: LogicalPx = LogicalPx(1.0);

/// 구조 간격 2px = DTCG `primitive.size-2`.
/// 예: 사이드바 도구 리스트/collapsed rail 의 gap (디자인 chrome.jsx
/// `gap: var(--tasty-size-2)`).
pub const STRUCT_GAP_2: LogicalPx = LogicalPx(2.0);

/// 구조 간격 3px = DTCG `primitive.size-3`.
/// 예: 사이드바 WorkspaceRow description 의 margin-top (디자인 chrome.jsx
/// `marginTop: var(--tasty-size-3)`).
pub const STRUCT_GAP_3: LogicalPx = LogicalPx(3.0);

/// 컨트롤 내부 위치를 맞추는 고정 간격. 반복되는 요소 간 간격과 구분한다.
pub const STRUCT_GAP_4: LogicalPx = LogicalPx(4.0);

// 본체와 갤러리가 공유하는 토스트 구조 치수.

/// 스코프 가장자리에서의 안쪽 여백. = `SIZING.spacing_md`.
pub const TOAST_SCOPE_MARGIN: f32 = SIZING.spacing_md.0;
/// 본문 텍스트의 좌우 여백. = `SIZING.spacing_md`.
pub const TOAST_PADDING_X: f32 = SIZING.spacing_md.0;
/// 본문 텍스트의 상하 여백. = `SIZING.spacing_sm`.
pub const TOAST_PADDING_Y: f32 = SIZING.spacing_sm.0;
/// 토스트 사이 세로 간격. = `component.toast-gap` → `{semantic.space-sm}` = 8.
pub const TOAST_GAP: f32 = SIZING.spacing_sm.0;
/// 메인 창 범위 스택의 하단 거리. = `component.toast-stack-offset-bottom` → `{primitive.size-36}`
/// (상태바 24 + 12). 상태바와 토스트 구조 치수처럼 UI 배율을 따르지 않는다(OFF-SCALE).
/// 생성 접근자는 배율을 곱하므로 쓰지 않는다. 토큰 값과의 일치는 시험이 확인한다.
pub const TOAST_STACK_OFFSET_BOTTOM: f32 = 36.0;
/// 본문과 hint 키캡 사이 가로 간격. 시안 `.tasty-toast`의 `gap: var(--tasty-space-sm)`.
pub const TOAST_HINT_GAP: f32 = SIZING.spacing_sm.0;

/// 좁은 스코프에서 카드 폭을 제한할 때 사용하는 하한. 본문 줄바꿈 폭은 별도로 1 이상으로 제한한다.
pub const TOAST_MIN_INNER_WIDTH: f32 = 48.0;

/// 클립보드 뷰어 가운데 아이콘 크기 — 콘텐츠 글리프라 공용 `CenterState`(icon-size-lg)
/// 밖에 남는 화면 전용 고정값. plugin 본체와 갤러리 specimen 이 같은 상수를 읽는다.
pub const CLIPBOARD_CENTER_ICON_SIZE: f32 = 28.0;

// ── 본체 ↔ 갤러리 specimen 공용 — 4px 그리드 밖 구조값 ─────────────────────────

/// 튜토리얼 스텝 행의 인덱스 캡 ↔ 본문 가로 간격. 디자인 전사값 10 으로 그리드
/// 밖이고, 같은 프레임의 `inner_margin` 이 쓰는 10(= `spacing_xs * 2.5`)과 짝이다.
/// 본체 `adapters/ui/tutorial/topic_popup.rs` 와 갤러리 specimen 이 같이 읽는다.
pub const TUTORIAL_STEP_GAP_X: f32 = 10.0;

// 대응하는 디자인 반경 토큰이 없는 값이다. 가까운 토큰으로 바꾸면 화면이 달라지므로 그대로 유지한다.
// 이 상수는 UI 배율을 적용하지 않아 Theme의 모서리 반경과 배율 동작이 다르다.

/// accent tag pill 의 코너 반경. **스케일 밖 3px.** 디자인의
/// `component.badge-radius` 는 `semantic.radius-sm`(2)다.
pub const TAG_PILL_CORNER_RADIUS: f32 = 3.0;

// 플러그인 아바타는 목록과 상세에서 크기가 다르다. 같은 값의 다른 역할 토큰으로 대체하지 않는다.

/// plugin 목록 행 왼쪽 아바타 한 변. 디자인 `<PluginAvatar size={32}>`
/// (Installed 목록 · Attention 목록 공통).
pub const PLUGIN_AVATAR_ROW_SIZE: LogicalPx = LogicalPx(32.0);

/// 플러그인 상세 아바타의 크기. 디자인의 size-46에 대응한다.
pub const PLUGIN_AVATAR_DETAIL_SIZE: LogicalPx = LogicalPx(46.0);

/// 목록 행은 아바타 높이와 위아래 여백을 합산한다.
pub const PLUGIN_LIST_ROW_HEIGHT: LogicalPx =
    LogicalPx(PLUGIN_AVATAR_ROW_SIZE.0 + SIZING.spacing_sm.0 * 2.0);

// 파일 핸들러 선택기에서 본체와 갤러리가 공유하는 화면별 치수.

/// 프레임 고정 폭. 디자인 `width: 420`.
pub const FH_FRAME_WIDTH: LogicalPx = LogicalPx(420.0);

/// 파일 핸들러 프레임의 좌우 여백. 같은 수의 아이콘·글꼴 토큰과 역할이 달라 공유하지 않는다.
pub const FH_EDGE_PAD_X: LogicalPx = LogicalPx(14.0);

/// 헤더 위 여백. 아래 여백과 값이 다르다.
pub const FH_HEADER_PAD_TOP: LogicalPx = LogicalPx(14.0);

/// 헤더 아래 여백. 같은 선언의 셋째 값 — 위아래가 다르다(10).
pub const FH_HEADER_PAD_BOTTOM: LogicalPx = LogicalPx(10.0);

/// 디자인 inline gap 6 — 그리드 밖. 두 자리가 같은 값을 쓴다: 헤더의 제목행↔경로 세로
/// gap(`flexDirection: column, gap: 6`)과 그룹 라벨↔count 가로 gap.
pub const FH_GAP_SM: LogicalPx = LogicalPx(6.0);

/// 목록 영역의 안쪽 여백. 디자인 `listStyle = { padding: 6 }`.
pub const FH_LIST_PAD: LogicalPx = LogicalPx(6.0);

/// 행 · 그룹 헤딩 · 그룹 구분선의 좌우 안쪽 여백. 디자인 `padding: "8px 10px"` 의 둘째 값.
pub const FH_ROW_PAD_X: LogicalPx = LogicalPx(10.0);

/// 행 안의 글리프 ↔ 텍스트 블록 가로 gap. 디자인 `gap: 10`.
pub const FH_ROW_GAP: LogicalPx = LogicalPx(10.0);

/// 행 둘째 줄(origin · id · when) 조각 사이 gap. 디자인 `gap: 5` — 그리드 밖.
pub const FH_ID_LINE_GAP: LogicalPx = LogicalPx(5.0);

/// 긴 목록의 최대 높이. 보이는 행 수는 글꼴과 행 여백에 따라 달라진다.
pub const FH_LIST_MAX_HEIGHT: LogicalPx = LogicalPx(264.0);

/// long 상태 목록 하단 페이드 띠의 높이. 디자인 `height: 20` — 그리드 밖.
pub const FH_LIST_FADE_HEIGHT: LogicalPx = LogicalPx(20.0);

/// 빈 상태 블록의 위아래 여백. 같은 값의 아바타 크기와 구분한다.
pub const FH_EMPTY_PAD_Y: LogicalPx = LogicalPx(32.0);

/// 행 둘째 줄의 id 를 **앞에서** 자르기 시작하는 길이(문자). 디자인
/// `h.id.length > 34 ? "…" + h.id.slice(-33) : h.id`.
pub const FH_ID_ELIDE_MAX: usize = 34;

/// [`FH_ID_ELIDE_MAX`] 초과 시 남기는 뒤쪽 문자 수 — 앞에 붙는 `…` 한 글자를 뺀 값.
pub const FH_ID_ELIDE_TAIL: usize = FH_ID_ELIDE_MAX - 1;

/// 경로 한 줄의 가용 폭. 프레임에서 좌우 테두리와 여백을 뺀다.
pub const FH_TARGET_LINE_BOX: LogicalPx =
    LogicalPx(FH_FRAME_WIDTH.0 - SIZING.border_width.0 * 2.0 - FH_EDGE_PAD_X.0 * 2.0);

/// D2Coding 11px, 배율 1, 픽셀 반올림을 사용하는 배치에서 한 글자 추가 시 늘어나는 폭.
/// 공칭 글리프 폭과 다르므로 실제 측정 시에도 문자열의 증가분을 사용해야 한다.
pub const FH_TARGET_MONO_ADVANCE: LogicalPx = LogicalPx(6.0);

/// 글자 폭을 측정할 수 없을 때 사용하는 상한. 가용 폭을 위 증가분으로 나눈 값이다.
pub const FH_TARGET_ELIDE_FALLBACK: usize = 65;

/// 원격 표지 옆 경로의 글자 수를 잴 수 없을 때의 상한. 호스트 글자가 상한(160)일 때
/// 남는 상자 206(390 − 12 − 4 − 160 − 8)을 6 으로 나눈 값이다.
pub const FH_TARGET_ELIDE_FALLBACK_REMOTE: usize = 34;

/// Recent 그룹 행 글리프의 흐리기. 디자인 `opacity: dim && !plugin ? 0.8 : 1` — plugin
/// 행은 mauve 가 출처 표시라 흐리지 않는다.
pub const FH_RECENT_DIM_OPACITY: f32 = 0.8;

/// File Detectors·Handlers 머리 설명 열이 Add 버튼 옆에 남는 최소 폭 = DTCG `primitive.size-240`
/// (시안 `flex: 1 1 var(--tasty-size-240)`). 이보다 좁으면 버튼이 설명 아래로 내려간다.
/// 역할 토큰이 없는 배치 경계라 여기 두고, 그리는 쪽이 UI 배율을 곱한다.
pub const FH_INTRO_TEXT_MIN: LogicalPx = LogicalPx(240.0);
