//! 값이 `size-*` 스케일 **안에 있는데** 숫자로 쓴 길이 자리를 세는 가드.
//!
//! [`length_constant_frontier`](super::length_constant_frontier) 와 축이 다르다. 그쪽은
//! **선언의 타입**(`const W: f32 = 96.0;`)을 묻고, 여기는 **값의 출처**를 묻는다 —
//! `egui::vec2(400.0, 320.0)` 은 타입 위반이 아니지만 `size-400`·`size-320` 토큰의 값을
//! 손으로 다시 적은 것이다. 토큰이 움직여도 이 자리는 안 따라간다.
//!
//! # 판정: 자리가 가족을 정하고, 값이 그 가족 안인지를 본다
//!
//! [ADR-0126] 의 "판정은 가족별로 한다" 를 그대로 집행한다. 여기서 보는 자리는
//! **`size-*` 가족을 재는 자리** 하나뿐이다 — 길이 타입 생성자([`LEN_CTOR`])와 egui
//! 기하 인자([`EGUI_LENGTH_HEADS`]). `.corner_radius(`(→`radius-*`)나 `FontId`
//! (→`font-size-*`)는 **여기서 안 본다.** 여러 가족을 합집합으로 재면 부류 하나가
//! 통째로 숨는다 — 그 착시가 이 가드가 생긴 계기다.
//!
//! 스케일은 손으로 안 적는다. [`SIZE_SCALE_SOURCE`] 의 생성 파일에서 읽으므로 토큰이
//! 늘거나 줄면 판정도 같이 움직인다. 그 파일이 사라지거나 이름이 바뀌면 스케일이 비고
//! 이 가드의 0 은 "없다" 가 아니라 "안 봤다" 가 된다 — [`the_size_scale_is_not_empty`]
//! 가 그 공허를 막는다.
//!
//! # 이 가드가 못 보는 것
//!
//! 아래는 설계상 사각이다. **여기 0 이 나온다고 "없다" 가 아니다.** 여섯 중 다섯은
//! [`the_blind_spots_are_still_the_size_they_say`] 가 건수를 **실측으로** 들고 있어
//! 사각이 조용히 자라지 않는다. 사각을 좁히려면 술어를 고치고 그 수를 함께 옮겨라.
//!
//! - **`0.0`** — 실측 건수는 위 테스트가 든다. `size-0` 이 실재하지만 기하에서 0 은
//!   디자인 결정이 아니라 덧셈의 항등원이다. `vec2(SIZE_0.value(), ..)` 는 정합이
//!   아니라 소음이다.
//! - **clamp·문턱 자리의 `1.0`**([`is_the_degenerate_floor`]) — 자리(clamp·문턱)와 값(1)의
//!   **연언**일 때만 빠진다. `.max(LogicalPx(200.0))` 같은 진짜 치수 하한은 같은 자리라도
//!   그대로 세어진다.
//! - **정규화 좌표 자리**([`UNIT_SPACE_SITES`]) — 부류가 아니라 **자리 명부**다. 등록된
//!   (파일, 호출 머리)만 빠지고, 그 자리의 수는 상한이자 하한이다.
//!
//! 이 둘이 왜 이 축의 값이 아닌지, 그리고 왜 하나는 부류이고 하나는 자리 명부인지는
//! [ADR-0252] 에 있다 — 여기서는 되풀이하지 않는다.
//! - **테스트 게이트 안** — 화면에 안 나가는 코드다. 판정은 [`super::test_gate`].
//! - **값을 선언하는 자리**([`DECLARATION_SITES`]) — 다른 자리가 참조해야 할 이름이
//!   사는 곳이라, 여기를 판정하면 선언에게 자기 자신을 참조하라고 요구하게 된다.
//! - **지역 변수 선언**(`let x = 20.0;`) — 이 가드가 **재지 않는** 유일한 사각이다.
//!   선언 자리에는 길이 문맥이 없어 같은 술어로는 길이인지 배율인지 안 갈린다. 축을
//!   열 때 쓴 임시 스캐너로 80 건을 셌고, 그 도구는 커밋되지 않았으므로 이 수는
//!   여기서 다시 재지지 않는다. 재려면 술어가 먼저 있어야 한다.
//!
//! 한때 여기 둘이 더 있었다 — 둘 다 **사각이 아니라 오탐**이라 닫는 것이 처방이었다.
//!
//! - **문자열·주석 안의 수.** `"window-button-size(24) …"` 의 24 가 `.size(` 인자로
//!   계상돼 축의 수를 4 만큼 부풀렸다. 손수 만든 마스커를 한 벌 더 두는 대신
//!   [`mask_non_code`] 를 쓰면서 닫혔다.
//! - **첨자.** `vec2(p[0] * s, p[1] * s)` 의 `p[1]` 이 `vec2(` 의 인자로 계상됐다.
//!   [`head_span_of`] 의 doc 은 "짝 없는 여는 기호가 `(` 가 아니면 인자 자리가 아니다"
//!   라고 이미 말하고 있었고 구현에서 `[` 만 빠져 있었다 — 정책 변경이 아니라 계약
//!   이행이다.
//!
//! # 상한은 여유가 아니다 — 양방향 래칫
//!
//! [`AREAS`] 의 수는 상한인 동시에 **하한**이다. 늘면 새 위반이고, 줄어도 실패한다 —
//! `scripts/check-allow-reason.sh` 가 같은 형태이고 이유도 같다: **남는 여유가 곧 안
//! 보는 구간이다.** 슬라이스로 고칠 때마다 그 줄의 수를 같이 내려야 한다. 형태만
//! 베끼면 다음 사람이 여유를 남기고, 그 여유 안에서는 이 가드가 아무것도 안 막는다.
//!
//! **총합은 어디에도 안 적는다.** [`AREAS`] 둘째 항의 합이 그것이다. 적는 순간 그 수는
//! 두 곳에 살게 되고, 값이 두 곳에 있는데 하나만 움직이는 사고가 이 레포에서 반복된
//! 형태다. 보고에 총합이 필요하면 그 자리에서 더해라.
//!
//! [ADR-0126]: ../../docs/adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md
//! [ADR-0252]: ../../docs/adr/0252-a-degenerate-floor-is-a-class-and-unit-space-is-a-roster.md

use super::test_gate::blank_test_modules;
use super::{mask_non_code, repo_root, rust_sources};

/// `size-*` 스케일의 정본. 생성 파일이라 DTCG 원본이 바뀌면 여기도 같이 바뀐다.
const SIZE_SCALE_SOURCE: &str = "crates/tasty-design-tokens/src/generated/primitive.rs";

/// 길이 타입 생성자. 인자가 곧 픽셀이다.
const LEN_CTOR: &[&str] = &["LogicalPx", "PhysicalPx"];

/// 인자를 길이로 먹는 egui/emath 기하 머리. 여기 없는 머리는 0 이 아니라 미측정이다.
const EGUI_LENGTH_HEADS: &[&str] = &[
    "vec2",
    "pos2",
    "splat",
    "shrink",
    "expand",
    "expand2",
    "from_min_size",
    "from_min_max",
    "add_space",
    "max_height",
    "min_height",
    "max_width",
    "min_width",
    "desired_width",
    "exact_width",
    "fixed_size",
    "at_least",
    "at_most",
    "set_width",
    "set_height",
    "set_min_width",
    "set_min_height",
    "set_max_width",
    "set_max_height",
    "set_min_size",
    "size",
    "inner_margin",
    "outer_margin",
    "symmetric",
    "same",
    "circle",
    "circle_filled",
    "circle_stroke",
];

/// **값을 선언하는 자리** — 여기는 판정하지 않는다. 건수는 모듈 문서의 사각 목록에
/// 대응하며 [`the_blind_spots_are_still_the_size_they_say`] 가 실측으로 든다.
const DECLARATION_SITES: &[(&str, usize, &str)] = &[
    (
        "crates/tasty-design-tokens/src/generated/",
        // 44 -> 47 은 `size-*` 스케일이 얻은 `SIZE_6` · `SIZE_64` · `SIZE_96` 셋이다 —
        // 선언 자신이라 처방이 없고, 스케일이 자라면 이 수도 그만큼 자란다.
        // 47 -> 49 는 같은 방식으로 `SIZE_44` · `SIZE_52` 둘이다(프리셋 설정 화면의 헤더 ·
        // footer 높이 토큰이 원시 스케일에 44 · 52 를 열었다).
        49,
        "스케일 자신 — 이 파일이 곧 size-* 의 정본이다",
    ),
    (
        "crates/tasty-type-appearance/src/theme.rs",
        47,
        "Theme 의 값표 — 다른 자리가 참조해야 할 이름(border_width 등)이 여기 산다",
    ),
];

/// **전시 대상 명부** — 자리 단위다. 부류로 면제하면 다음 사람이 아무 자리나 "전시" 라고
/// 부른다. 항목은 (파일, 호출 머리, 자리 수, 사유)이고 수는 상한이자 하한이다.
///
/// 줄 번호로 적지 않는 이유는 그 수가 위쪽 편집마다 낡기 때문이다 — (파일, 머리)는
/// 그 자리가 무엇인지로 식별한다.
const DISPLAY_SPECIMENS: &[(&str, &str, usize, &str)] = &[(
    "crates/tasty-gallery/src/catalog/components/prim_spinner.rs",
    "size",
    5,
    "스피너를 여러 크기로 보여주는 것이 이 카드의 목적이다 — 토큰으로 바꾸면 전시가 사라진다",
)];

/// **정규화 좌표 자리 명부** — `DISPLAY_SPECIMENS` 와 같이 자리 단위다. 부류가 아닌
/// 이유는 [ADR-0252].
///
/// 항목은 (파일, 호출 머리, 자리 수, 사유)이고 수는 상한이자 하한이다 —
/// [`the_blind_spots_are_still_the_size_they_say`] 가 실측으로 든다.
///
/// [ADR-0252]: ../../docs/adr/0252-a-degenerate-floor-is-a-class-and-unit-space-is-a-roster.md
const UNIT_SPACE_SITES: &[(&str, &str, usize, &str)] = &[(
    "crates/tasty-plugin-image/src/render.rs",
    "pos2",
    4,
    "텍스처 UV — 0..1 정규화 좌표라 픽셀이 아니다. 전체 텍스처를 가리키는 \
     `pos2(1.0, 1.0)` 의 1 은 1px 가 아니라 100% 다",
)];

/// 그 자리가 [`UNIT_SPACE_SITES`] 에 **등록돼 있는가.**
fn is_in_unit_space(hit: &Hit) -> bool {
    UNIT_SPACE_SITES
        .iter()
        .any(|(path, head, _, _)| hit.rel == *path && hit.head == *head)
}

/// 영역별 잔여와 그 사유. 수는 상한이자 **하한**이다 — 위 "양방향 래칫" 참조.
const AREAS: &[(&str, usize, &str)] = &[
    (
        "src/adapters/ui/popup/",
        48,
        // 49 -> 48 은 path bar 가 `component.fp-*` 토큰을 직접 읽던 자리를 뷰 밖
        // (`src/file/picker_caps.rs`)으로 옮기면서 그 자리의 리터럴이 함께 나간 것이다.
        // 안 보게 된 것이 아니라 이 영역에 사본이 하나 줄었다.
        // 48 -> 50 은 `port_scanner.rs` 의 컬럼 최소폭 표(`column_layout`)가 타입을
        // 얻으면서 **바늘 안으로 들어온 것**이다. 그 일곱은 전에도 그 자리에 있었고
        // `(84.0, false, ..)` 처럼 맨 f32 라 이 가드의 술어가 안 닿았다. `LogicalPx` 로
        // 넓히자 그중 `size-*` 스케일과 값이 겹치는 둘(120 · 200)이 보이게 됐다.
        // 위반이 새로 생긴 것이 아니라 **안 보던 것이 보이는 것**이다 — 아래
        // `src/` 항목이 `LogicalSize::new(400, 200)` 에서 겪은 것과 같은 방향이다.
        // 없앨 수 없는 이유는 그 자리 주석에 적혀 있다: 컬럼 최소폭에는 이름이 없다
        // (`component.port-*` 에 있는 것은 별 컬럼 폭·즐겨찾기 높이 셋뿐이다).
        // 같은 회차에 하나는 **없앴다** — 별 컬럼 폭 28 은 그 치수의 이름이 실재해서
        // (`component.port-star-col-width`) 상수를 지우고 토큰을 부른다. 그래서 +3 이
        // 아니라 +2 다.
        //
        // 49 -> 48 은 `file_handler_picker` 등록 한 줄이다. 그 자리의 처방이 여기
        // 적혀 있던 것과 같았다 — **placeholder 를 안 적는 형태**. 함수로 빼는 것만으로는
        // 자리가 옮겨질 뿐이지만, 그 함수가 값을 `sizer` 와 **같은 식에서 도출**하면
        // 리터럴 자체가 사라진다(높이는 계산이고 폭은 `size-*` 스케일 밖이다).
        // 그 popup 은 sizer 가 view 스택을 덜 세어 목록 마지막 행이 잘리고 있었고,
        // 그것을 고치면서 등록 값도 같은 식에 붙였다 — 두 수가 어긋날 자리가 없어진다.
        //
        // 50 -> 48 이 그 두 형제다 — destructive confirm 둘
        // (`confirm_delete_category` · `confirm_force_detach_workspace`)의 **높이 150**
        // (380 은 스케일 밖이라 여기 안 걸린다 —
        // `const WIDTH: LogicalPx = LogicalPx(380.0)` 도 마찬가지다). 둘 다 `sizer` 가
        // 첫 프레임에 덮어쓰는 placeholder 였고, 각 모듈의 크기 식을 `size_for(body_len)`
        // 하나로 모아 등록 값도 거기서 뽑게 했다. 리터럴이 옮겨진 것이 아니라 **없어진
        // 것**이다 — 그 식 안의 값은 폰트 metric 과 여백 계산이라 이 축의 바늘 밖이다.
        //
        // 48 -> 47 은 **이사**다 — 없어진 것도 고쳐진 것도 아니다. remote_tool 의 필터
        // 드롭다운 치수가 공용 view(`tasty_ui_widgets::remote_tool`)로 내려가면서 같은
        // 리터럴이 아래 `crates/tasty-ui-widgets/` 칸으로 옮겨갔다(12 -> 13).
        // **두 수가 같은 커밋에서 반대로 움직이는 것이 이사의 표지다** — 한쪽만 고치면
        // 총수가 맞는데도 두 칸 다 빨개진다.
        //
        // 47 -> 49 는 command palette 가 카드 높이를 콘텐츠에 맞추면서 생겼다. 위
        // `port_scanner.rs` 와 **같은 방향**이다 — 그 수들은 전에도 그 자리에 있었고
        // (`let row_height = 28.0;` · `- 6.0`) 맨 f32 라 이 가드의 술어가 안 닿았다.
        // sizer 가 매 프레임 같은 식을 다시 세어야 해서 draw 와 sizer 가 **한 이름**을
        // 봐야 했고, 이름을 주는 순간 `LogicalPx` 가 되면서 바늘 안으로 들어왔다.
        // 위반이 새로 생긴 것이 아니라 안 보던 것이 보이는 것이다.
        //
        // 셋 중 하나는 같은 회차에 **없앴다** — 행 높이 28 은 그 치수의 이름이 실재해서
        // (`semantic.control-height` → `Theme.item_height_interactive`) 상수를 짓지
        // 않고 토큰을 읽는다. 그래서 +3 이 아니라 +2 다. 남은 둘의 사유는 그 자리
        // 주석에 있다: 목록 세로 상한 320 은 `size-320` 을 쓰는 토큰 넷이 **전부 폭**
        // 이라 옮길 이름이 없고, 목록·footer 사이 6 은 spacing semantic 에 6 이 없다
        // (4 와 8 사이). 둘 다 이름이 생기면 옮긴다.
        "popup 기본 크기표 — vec2(400.0, 320.0) 처럼 정의 옆에 값이 그대로 박혀 있다",
    ),
    (
        "src/adapters/ui/",
        // 44 -> 42 는 `egui_panels.rs` 의 `.max(PhysicalPx(1.0))` 둘이다. 위반이
        // 고쳐진 것이 아니라 **애초에 이 축의 값이 아니었다** — 퇴화 방지 하한이라
        // [`is_the_degenerate_floor`] 가 걷어냈다. 그 몫의 크기는
        // [`the_blind_spots_are_still_the_size_they_say`] 가 센다.
        // 42 -> 51 은 토큰 export 가 `size-*` 스케일에 6 · 64 · 96 을 더하면서
        // (`crates/tasty-design-tokens/src/generated/primitive.rs` 의 `SIZE_6` ·
        // `SIZE_64` · `SIZE_96` — 이 세 이름이 그 사실의 정본이다)
        // **바늘 밖에 있던 리터럴이 안으로 들어온 것**이다. 그 자리들은 전에도 그대로
        // 있었고 값이 스케일에 없어 안 보였을 뿐이다 — 위반이 새로 생긴 것이 아니라
        // 바늘이 넓어졌다. 위 `src/adapters/ui/popup/` 과 아래 `src/` 항목이 겪은 것과
        // 같은 방향이다. 실측으로 갈랐다 — HEAD 트리에 이 회차의 생성 파일 넷만
        // 얹으면(primitive · theme · generated_component · semantic_color_generated)
        // 이 수가 53 이 되고, 회차의 소스 수정이 그중 둘을 `Theme` 접근자로 없애 51 이
        // 된다. 나머지는 이 회차의 범위 밖 파일이라 손대지 않았다.
        // 51 -> 50 은 두 방향이 겹친 것이다. 프리셋 편집기의 인라인 leaf 폼이 지워지면서
        // 그 폼의 치수 둘(`demo_layout.rs` 의 폼 최대폭 240 · 폼 패딩 6)이 **없어졌고**,
        // 토큰 export 가 `size-*` 에 44 · 52 를 더해 전부터 있던 `preset.rs` 의
        // `TOOLBAR_HEIGHT`(44)가 바늘 안으로 들어왔다(+1). 그 자리는 못 고친다 — 툴바
        // 높이라는 역할의 이름이 토큰에 없고, 값이 같은 `preset_cfg_header_height` 는 설정
        // 화면 헤더의 치수라 가져다 쓰면 틀린 결합이 된다(사유는 그 자리 주석에 있다).
        50,
        "나머지 host chrome(사이드바·타이틀바·서피스 장식)",
    ),
    (
        "src/view/",
        // 31 -> 29 는 plugins 창 목록 행 둘(`list.rs` · `attention.rs`)의
        // `vec2(8.0, 6.0)` 이 `spacing_sm` 짝으로 바뀐 것이다. **자리가 안 보이게 된
        // 것이 아니라 없어졌다** — 그 커밋의 `src/view/` diff 에서 길이 자리를 담은
        // 삭제 줄은 정확히 그 둘이고, 추가 줄에는 `0.0`(이 가드의 사각) 말고 숫자
        // 리터럴이 없다. 8 은 이름이 있는 값이라(`spacing_sm`) 그대로 갔고, 6 은
        // 애초에 이 가드의 바늘 밖이었다 — `size-*` 에 6 이 없다.
        // 29 -> 27 도 퇴화 방지 하한 둘이다(`main/redraw.rs` 의 `.max(..)` ·
        // `settings/ui.rs` 의 `(w - margin*2).max(LogicalPx(1.0))`).
        // 27 -> 31 은 단축키 가져오기/내보내기 화면(`keybindings_tab/import_export.rs`)의 명명
        // 상수 넷(선택 열 32 · 원래 조합 열 120 · 녹화 슬롯 높이 24 · 카드 패딩 14)이다. jsx 가
        // `--tasty-size-*` 원시 스케일을 직접 부르는데 그 스케일이 크레이트 밖에 안 열려 있고
        // 같은 뜻의 semantic 이름도 없다 — 못 고치는 이유는 그 자리 주석에 있다.
        // 31 -> 29 는 그중 둘(슬롯 높이 24 · 카드 패딩 14)이 **없어진 것**이다. 디자인이 그
        // 치수들에 토큰을 열었고, semantic 별칭인 둘은 Theme 이름(`item_height_tab` ·
        // `spacing_md`)이 이미 있어 그것을 읽는다(14 는 12 로 스냅). 남은 둘(32 · 120)은
        // 원시 스케일을 바로 가리키는 컴포넌트 토큰이라 vendor 한 DTCG export 에 그 이름이
        // 들어오면 없어진다 — 디자인이 기대한 끝값은 27 이다.
        // 29 -> 33 중 셋은 위 `src/adapters/ui/` 와 같은 원인이다 — `size-*` 에 6 · 64 · 96
        // 이 들어오면서 전부터 있던 자리가 보이게 됐다. 남은 하나는 이 회차가 만든 것이다:
        // `plugins/ui.rs` 의 세그먼트 탭 라벨이 off-scale 12.5 를 버리고 12 를 이름 붙여
        // 받는다(ADR-0126 의 off-scale 집합이 닫힌 결과라 되돌릴 값이 아니다).
        // 33 -> 34 도 `size-*` 가 44 · 52 를 얻어 바늘이 넓어진 몫이다 — 전부터 있던
        // `settings/ui.rs` 의 `SETTINGS_HEADER_HEIGHT`(44)가 보이게 됐다. 설정 L1 헤더
        // 높이라는 역할의 이름이 토큰에 없어 못 고친다(사유는 그 자리 주석에 있다).
        34,
        "설정 화면의 폼 레이아웃",
    ),
    (
        "src/",
        // 16 -> 9 는 퇴화 방지 하한 일곱이다 — 전부 `(pane_rect.height -
        // tab_bar_h).max(PhysicalPx(1.0))` 계열(`state/` 넷 · `core/impl_pty.rs` ·
        // `gfx/gpu.rs` · `app/window_lifecycle.rs`)이다. 같은 식이 여섯 자리에 복제돼
        // 있는 것은 별개 물음이고, 이 축의 물음은 아니다.
        // 9 -> 10 은 아래 사유 그대로다 — 토큰 export 가 `size-*` 에 6 · 64 · 96 을
        // 더했고, 전부터 있던 자리 하나가 그 바늘 안으로 들어왔다.
        10,
        "그 밖의 본체(gfx·state·app) — GPU/상태 쪽이라 자리마다 사정이 다르다. \
         이 수가 마지막으로 오른 것은 위반이 늘어서가 아니라 `LogicalSize::new(400, 200)` \
         처럼 **세지 않는 형태**로 숨어 있던 값이 이름을 얻어 보이게 됐기 때문이다 — \
         바늘 밖에 있던 것이 안으로 들어오면 이 수는 오른다",
    ),
    (
        "crates/tasty-gallery/",
        // 85 -> 90 은 같은 화면의 specimen(`kb_import_export.rs`) — 본체 넷에 마이그레이션
        // 라벨 열 288 을 더한 다섯이다. 사유는 본체 쪽과 같고 그 자리 주석에 있다.
        // 90 -> 88 은 본체와 같은 둘(24 · 14)이 Theme 을 읽게 된 것이다. 남은 셋(32 · 288 · 120)은
        // export 에 컴포넌트 토큰이 들어오면 없어진다 — 디자인이 기대한 끝값은 85 다.
        // 거기서 +1 은 `file_picker.rs` specimen 의 `CRUMB_MAX_W`(180) — 디자인 `FpCrumbs` 의
        // 성분 maxWidth 이고, 대응 `Theme` 토큰이 없으며 디자인 회신도 새 토큰을 두지 않았다.
        // switch_overlay의 이름/설명 gap 두 사용처가 한 명명 상수를 공유한다.
        // 나머지 역할 명명은 계상 자리를 선언으로 옮기므로 총수는 그대로다.
        // 88 -> 95 도 `size-*` 가 6 · 64 · 96 을 얻어 바늘이 넓어진 몫이다. 생성 파일만
        // 얹은 실측에서는 98 이고, 이 회차가 셋을 `Theme` 접근자로 없애 95 가 된다.
        // 95 -> 97 은 본체↔specimen 정합으로 **새로 생긴 둘**이다 — Plugins 창의
        // `ATTN_PRIMITIVE_12` 와 `SEGMENT_TAB_LABEL_PRIMITIVE_12`. 본체가 off-scale
        // 12.5 를 버리고 primitive 12 를 이름 붙여 읽게 됐는데 specimen 이 13 으로
        // 근사하고 있어 픽셀이 갈려 있었다. 값을 맞추는 방법이 같은 primitive 를 같은
        // 이유로 부르는 것뿐이라(12 에는 semantic role 이 없다) 이 수가 오른다.
        // 97 -> 100 은 프리셋 surface 설정 화면 specimen(`preset_surface_settings.rs`)의
        // 데모 프레임 치수 셋(좁은 폭 220 · 높이 300 · 스크롤 위치 120)이다. 맨 `f32` 였던
        // 것을 `LogicalPx` 로 선언하자 바늘 안으로 들어왔다(폭 280 은 스케일 밖). 본체
        // 치수가 아니라 디자인 `SettingsDemo` 의 배치값이라 옮길 `Theme` 이름이 없다 —
        // 사유는 그 자리 주석에 있다.
        100,
        "갤러리 specimen — 배율에는 면제지만(ADR-0135) 스케일에는 아니다. \
         한 항목이 아니다 — 모양은 `the_gallery_share_is_one_question_or_it_is_not` 이, \
         갈래는 `the_gallery_share_splits_into_four_kinds` 가 든다",
    ),
    (
        "crates/tasty-ui-widgets/",
        // 4 -> 6 은 `PluginAvatar` 의 두 변(32 · 46)이다. **없앨 수 있는지 먼저 쟀다.**
        // 두 값 다 primitive 스케일에 정확히 있지만(`SIZE_32` · `SIZE_46`) 그쪽은
        // `pub(crate)` 라 밖에서 부를 이름이 아니고, `Theme` 에서 값이 같은 필드는
        // 각각 하나씩뿐인데 둘 다 뜻이 다른 축이다 — 32 는 `sidebar_collapsed_slot_width`
        // (접힌 사이드바 슬롯 폭), 46 은 `caption_width`(창 캡션 버튼 띠 폭). 값이
        // 같다고 그 이름이 이 자리에 맞는 이름은 아니다([`theme_named_values`] 의
        // 한 방향 결론과 같은 이야기다). 계산으로 없앨 수도 없다 — 행 높이
        // (`PLUGIN_LIST_ROW_HEIGHT`)는 아바타에서 **도출**돼 리터럴이 아니지만, 아바타
        // 한 변 자체는 도출되는 값이 아니라 디자인 입력이다. 이 항목이 이미 세고 있는
        // 넷(`STRUCT_GAP_1..4`)이 같은 이유로 여기 있다 — 처방이 "토큰으로 바꿔라" 가
        // 아니라 "이 치수에 semantic 이름을 줄 것인가" 이고, 그것은 디자인 결정이다.
        //
        // 6 -> 7 은 위와 같다 — `size-*` 에 6 이 들어오면서 전부터 있던 자리 하나가
        // 바늘 안으로 들어왔다.
        //
        // 7 -> 12 는 file handler picker 의 전사 치수 **다섯**(14 · 14 · 32 · 6 · 6)이다.
        // 같은 형태다 — 값이 스케일의 어떤 수와 겹치지만 **그 축이 아니다**: 14 둘은
        // 프레임 가장자리 여백인데 값이 같은 토큰은 글리프 한 변(`icon_glyph_size_sm`)과
        // 폰트 크기(`font_size_max`)이고, 32 는 블록 여백인데 값이 같은 것은 정사각
        // 요소의 한 변이다(`PLUGIN_AVATAR_ROW_SIZE` · `sidebar_collapsed_slot_width`).
        // 6 둘(헤더·그룹의 줄 간격과 목록 안쪽 여백)은 `size-6` 에 값이 있지만 그것을
        // 쓰는 component 토큰은 `status-dot-size-compact` 하나 — 점의 지름이다. 겹치는
        // 토큰을 부르면 "점이 커지면 목록 여백도 커진다" 는 없는 관계가 생긴다. spacing
        // 스텝에는 6 도 14 도 32 도 없어(4·8·12·16·24) 부를 이름이 실제로 없다. 사유는
        // 각 상수의 doc 주석에도 값 옆에 적혀 있다.
        //
        // **이 수는 두 증가가 겹친 자리라 한쪽을 고르면 틀린다** — main 의 +1 과 이
        // lane 의 다섯은 서로 다른 자리에서 왔다. 게다가 6 이 스케일에 들어온 것이
        // main 쪽 변경이라, 이 lane 이 자기 base 에서 세면 6 둘이 **안 보인다**(그때는
        // 셋만 세어 9 였다). 그래서 이 수는 병합 뒤 좌변을 다시 세서 정했다.
        //
        // 12 -> 13 은 **이사**다 — remote_tool 의 필터 드롭다운 치수가 본체 popup 에서
        // 이 crate 의 공용 view 로 내려왔다. 위 `src/adapters/ui/popup/` 칸이 같은
        // 커밋에서 48 -> 47 로 함께 움직인다. 새 리터럴이 아니라 자리가 바뀐 것이라
        // 처방도 다르다 — 사유는 옮기기 전과 같다(드롭다운 최소폭·스크롤 상한에
        // 대응하는 semantic 이 없다).
        //
        // 13 -> 14 는 로컬 ssh config 섹션의 `SSH_GAP`(6) 하나다 — 이사가 아니라 **새로
        // 생긴 자리**다(전에는 그 섹션이 Theme 의 `spacing_xs`/`spacing_sm` 만 썼고,
        // 확정 시안이 `gap: 6` · `padding: "2px 4px 6px"` · `padding: "6px 4px"` 로 6 을
        // 세 자리에서 쓴다). 위 file handler picker 의 6 둘과 **같은 사유로 못 없앤다** —
        // `size-6` 에 값은 있지만 그것을 쓰는 component 토큰은 점의 지름
        // (`status-dot-size-compact`) 하나뿐이고, spacing 스텝(4·8·12·16·24)에 6 이 없다.
        // 같은 섹션의 `SSH_SECTION_MARGIN_TOP`(10)은 이 수에 안 들어간다 — 10 은 스케일에
        // 없어 바늘 밖이다. **이 가드에는 그 몫을 세는 칸이 없다**(옆의
        // [`the_blind_spots_are_still_the_size_they_say`] 는 0 · 테스트 전용 · 하한 ·
        // 정규화 좌표를 세고, off-scale 값은 안 센다). 변이로 확인했다 — 같은 상수를
        // 12 로 바꾸면 이 수가 15 로 움직이고, 10 으로 두면 아무 칸도 안 움직인다.
        // 14 -> 15 는 `FH_TARGET_MONO_ADVANCE`(6)다. **크기 선택이 아니라 잰 값이다** —
        // 한 글자 늘 때의 advance 증분이고, 그 자리 주석이 왜 단일 글리프 폭이 아니라
        // 증분이어야 하는지를 적고 있다. `size-6` 과 값이 겹쳐 이 가드에 들어왔을 뿐,
        // 부를 치수 토큰이 없다 — 폰트가 바뀌면 이 수도 다시 재야 한다.
        15,
        "공용 위젯",
    ),
    (
        "crates/",
        // 26 -> 14 는 셋이 겹친 결과다. **어느 것도 자리를 고친 것이 아니다** —
        // 가드가 안 봐야 할 것을 보고 있었다.
        //  · 퇴화 방지 하한 3 (`tasty-model` 의 `.max(PhysicalPx(1.0))` · `.min(..)`)
        //  · 서브픽셀 동일성 문턱 4 (`tasty-type-geometry` 의 `.abs() < PhysicalPx(1.0)`)
        //  · 정규화 좌표 4 (`tasty-plugin-image` 의 UV `pos2(1.0, 1.0)` — 명부에 등록)
        //  · 첨자 1 (`tasty-plugin-sdk` 의 `vec2(p[0] * s, p[1] * s)` — `p[1]` 의 1 이
        //    `vec2(` 인자로 계상됐다. `head_span_of` 가 `[` 를 세게 되면서 닫혔다)
        // 14 -> 15 도 같은 원인이다 — 토큰 export 가 더한 `size-*` 한 값에
        // 전부터 있던 자리 하나가 겹쳤다.
        15,
        "나머지 크레이트(dag-layout·model·plugin 뷰어·settings·geometry)",
    ),
];

/// 한 자리의 관측.
struct Hit {
    rel: String,
    line: usize,
    head: String,
    value: f32,
    /// 그 수가 [`is_the_degenerate_floor`] 인가. 스캔이 지우지 않고 표시만 하는 것은
    /// 빠지는 몫의 크기를 [`the_blind_spots_are_still_the_size_they_say`] 가 세기
    /// 때문이다 — 지워 버리면 그 부류는 존재하지 않는 것이 된다.
    floor: bool,
}

/// 생성된 primitive 에서 `size-*` 값 집합을 읽는다.
fn size_scale() -> Vec<f32> {
    let text = std::fs::read_to_string(repo_root().join(SIZE_SCALE_SOURCE)).unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((_, rest)) = line.trim_start().split_once("const SIZE_") else {
            continue;
        };
        let Some((_, rest)) = rest.split_once("LogicalPx(") else {
            continue;
        };
        let Some((value, _)) = rest.split_once(')') else {
            continue;
        };
        if let Ok(v) = value.trim().parse::<f32>() {
            out.push(v);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup();
    out
}

/// `&Theme` 를 거쳐 나오는 길이 값들. **이름이 있는 값의 집합**이다.
///
/// 두 자리에서 읽는다 — `Theme` 의 DEFAULT 값표(손으로 쓰는 semantic 이름)와 생성된
/// component 접근자(디자인 토큰에서 나온 이름). 어느 쪽도 손으로 안 적는다.
///
/// 이 집합에 **없는** 값은 이름을 붙일 수 없다 — 그 자리의 처방은 "토큰으로 바꿔라" 가
/// 아니라 "이 치수에 이름을 줄 것인가" 이고 그건 디자인 결정이다. 반대로 있다고 해서
/// 그 이름이 **그 자리에 맞는** 이름이라는 뜻은 아니다(값이 같을 뿐일 수 있다).
/// 그래서 이 술어는 한 방향으로만 결론을 낸다: **없으면 못 고친다.**
fn theme_named_values() -> Vec<f32> {
    let mut out = Vec::new();
    for (path, prefix) in [
        ("crates/tasty-type-appearance/src/theme.rs", ": LogicalPx("),
        (
            "crates/tasty-type-appearance/src/generated_component.rs",
            "LogicalPx((",
        ),
    ] {
        let text = std::fs::read_to_string(repo_root().join(path)).unwrap_or_default();
        for line in text.lines() {
            let trimmed = line.trim();
            // 필드 **선언**(`pub name: LogicalPx,`)이 아니라 **값**이 붙은 줄만 본다.
            let Some(rest) = trimmed.split_once(prefix).map(|(_, r)| r) else {
                continue;
            };
            let head: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(v) = head.parse::<f32>() {
                out.push(v);
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup();
    out
}

/// 리터럴을 감싸는 **가장 안쪽 호출**의 (이름 시작 위치, 여는 괄호 위치, 머리 이름).
/// 짝 없는 여는 기호가 `(` 가 아니면(블록·구조체 리터럴·**첨자** 안이면) 인자 자리가
/// 아니므로 `None`.
///
/// 여는 괄호 위치를 함께 돌려주는 것은 **한 겹 밖**을 다시 물을 수 있게 하기 위해서다 —
/// `.max(PhysicalPx(1.0))` 의 안쪽 머리는 `PhysicalPx` 지만 그 수의 성격을 정하는 것은
/// 바깥의 `max` 다([`is_the_degenerate_floor`]). 이름 시작 위치는 그 앞의 비교 연산자를
/// 보기 위한 것이다.
fn head_span_of(text: &[char], at: usize) -> Option<(usize, usize, String)> {
    let (mut paren, mut brace, mut bracket) = (0usize, 0usize, 0usize);
    let mut j = at;
    let open = loop {
        if j == 0 {
            return None;
        }
        j -= 1;
        match text[j] {
            ')' => paren += 1,
            '}' => brace += 1,
            ']' => bracket += 1,
            '(' => {
                if paren == 0 {
                    break j;
                }
                paren -= 1;
            }
            '{' => {
                if brace == 0 {
                    return None;
                }
                brace -= 1;
            }
            '[' => {
                if bracket == 0 {
                    return None;
                }
                bracket -= 1;
            }
            _ => {}
        }
    };
    let mut k = open;
    while k > 0 && (text[k - 1].is_ascii_alphanumeric() || matches!(text[k - 1], '_' | ':')) {
        k -= 1;
    }
    let name: String = text[k..open].iter().collect();
    name.rsplit("::")
        .next()
        .filter(|s| !s.is_empty())
        .map(|n| (k, open, n.to_owned()))
}

/// 리터럴을 감싸는 **가장 안쪽 호출 머리**.
fn head_of(text: &[char], at: usize) -> Option<String> {
    head_span_of(text, at).map(|(_, _, name)| name)
}

/// 그 `1` 이 **치수가 아니라 퇴화 방지 하한**인가.
///
/// 참이 되는 자리는 둘이다 — clamp 계열(`.max` · `.min` · `.clamp`)의 인자와 비교 문턱
/// (`.abs() < PhysicalPx(1.0)`)의 오른쪽. **어느 쪽이든 값이 1 일 때만** 참이므로
/// `.max(LogicalPx(200.0))` 의 200 은 그대로 세어진다. 왜 이 둘이 이 축의 값이 아닌지와
/// 왜 자리 명부가 아니라 부류인지는 [ADR-0252].
///
/// # 이 술어가 일부러 안 잡는 것
///
/// 이름을 얻은 1(`const EPS: LogicalPx = LogicalPx(1.0);` · `GLYPH_STROKE` ·
/// `SURFACE_BORDER_WIDTH`)은 여기서 안 빠진다. 그 줄은 값을 쓰는 자리가 아니라 이름을
/// 짓는 자리라 처방이 다르고([`declares_a_named_dimension`]), `=` 하나로 그것까지
/// 걷어내면 이 축에서 제일 고칠 만한 자리(헤어라인)가 통째로 사라진다.
///
/// [ADR-0252]: ../../docs/adr/0252-a-degenerate-floor-is-a-class-and-unit-space-is-a-roster.md
fn is_the_degenerate_floor(text: &[char], at: usize, value: f32) -> bool {
    if value != 1.0 {
        return false;
    }
    let Some((name_start, open, _)) = head_span_of(text, at) else {
        return false;
    };
    // 문턱 비교의 오른쪽 — `.abs() < PhysicalPx(1.0)`.
    //
    // `>` 하나만 보면 안 된다. 홑 `=`(대입)는 물론이고 **`=>`(매치 팔)** 와
    // **`->`(반환 타입)** 도 마지막 글자가 `>` 다 — 실측으로 `MouseWheelUnit::Point =>
    // LogicalPx(1.0)`(포인트→논리픽셀 단위 환산)이 이 술어에 걸려 하한으로 잘못 세어졌다.
    // 그 자리의 1 은 하한이 아니라 **환산 계수**라 성격이 다르다.
    let mut i = name_start;
    while i > 0 && matches!(text[i - 1], ' ' | '\t' | '\n') {
        i -= 1;
    }
    if i > 0 {
        let c = text[i - 1];
        let prev = if i >= 2 { text[i - 2] } else { ' ' };
        let comparison = match c {
            '<' => true,
            '>' => !matches!(prev, '=' | '-'),
            '=' => matches!(prev, '<' | '>' | '!' | '='),
            _ => false,
        };
        if comparison {
            return true;
        }
    }
    // 한 겹 밖이 clamp 계열 — `.max(PhysicalPx(1.0))`.
    matches!(
        head_span_of(text, open)
            .as_ref()
            .map(|(_, _, n)| n.as_str()),
        Some("max" | "min" | "clamp")
    )
}

/// 곱셈·나눗셈의 피연산자면 길이가 아니라 **배율**이다.
///
/// 실측으로 걸렸다: `d * 0.5`(지름→반지름) · `s * 0.6`(형상 비례) · `5.0 / 18.0`. 셋 다
/// `circle_filled(` · `pos2(` 안이라 **자리로는 길이인데 값은 길이가 아니다.** 이 필터가
/// 없으면 `0.5` 하나가 137 건으로 최다값이 되어 축 전체가 비율에 파묻힌다. 그래서 이
/// 술어가 이 가드의 판별력 자체이고, 문서가 아니라
/// [`a_factor_is_not_a_length`](detector::a_factor_is_not_a_length) 가 못박는다.
fn is_length_operand(text: &[char], start: usize, end: usize) -> bool {
    let mut i = start;
    while i > 0 && matches!(text[i - 1], ' ' | '\t' | '\n') {
        i -= 1;
    }
    if i > 0 && matches!(text[i - 1], '*' | '/') {
        return false;
    }
    let mut j = end;
    while j < text.len() && matches!(text[j], ' ' | '\t' | '\n') {
        j += 1;
    }
    !(j < text.len() && matches!(text[j], '*' | '/'))
}

/// 마스킹된 소스에서 `size-*` 값과 같은 수가 길이 자리에 박힌
/// (줄번호, 머리, 값, [`is_the_degenerate_floor`] 인가).
/// `0.0` 도, 퇴화 방지 하한도 포함해 돌려준다 — 그 사각의 크기를 재는 쪽이 따로 있다.
///
/// 순수 함수다 — 합성 스니펫을 그대로 먹일 수 있다.
fn on_scale_literals(masked: &str, scale: &[f32]) -> Vec<(usize, String, f32, bool)> {
    let text: Vec<char> = masked.chars().collect();
    let mut out = Vec::new();
    let mut line = 1usize;
    let mut i = 0usize;
    while i < text.len() {
        if text[i] == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if !text[i].is_ascii_digit()
            || (i > 0 && (text[i - 1].is_ascii_alphanumeric() || matches!(text[i - 1], '_' | '.')))
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < text.len() && text[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < text.len() && text[i] == '.' && text[i + 1].is_ascii_digit() {
            i += 1;
            while i < text.len() && text[i].is_ascii_digit() {
                i += 1;
            }
        }
        let end = i;
        if end < text.len() && (text[end].is_ascii_alphanumeric() || matches!(text[end], '_' | '.'))
        {
            continue;
        }
        let Ok(value) = text[start..end].iter().collect::<String>().parse::<f32>() else {
            continue;
        };
        if !scale.contains(&value) || !is_length_operand(&text, start, end) {
            continue;
        }
        let Some(head) = head_of(&text, start) else {
            continue;
        };
        if LEN_CTOR.contains(&head.as_str()) || EGUI_LENGTH_HEADS.contains(&head.as_str()) {
            let floor = is_the_degenerate_floor(&text, start, value);
            out.push((line, head, value, floor));
        }
    }
    out
}

/// 레포 전수 스캔. `blank_tests` 가 참이면 테스트 게이트 안을 지운다 — 그 차이가 곧
/// "테스트 안" 사각의 크기다.
fn scan(blank_tests: bool) -> Vec<Hit> {
    let scale = size_scale();
    let files = rust_sources();
    // 파일 단위 판정("이 파일이 출하되나")은 `shipping_scope` 의 일이다 — 선언 간선의
    // 전이 폐쇄를 따르고 `#[path]` 도 푼다. 여기서 다시 세지 않는다.
    let gated = tasty_doc_guards::shipping_scope::test_only_files(&super::repo_root(), &files);
    let mut out = Vec::new();
    for (path, text) in &files {
        if blank_tests && gated.contains(path) {
            continue;
        }
        let rel = path.to_string_lossy().replace('\\', "/");
        let masked = mask_non_code(text);
        let masked = if blank_tests {
            blank_test_modules(&masked)
        } else {
            masked
        };
        for (line, head, value, floor) in on_scale_literals(&masked, &scale) {
            out.push(Hit {
                rel: rel.clone(),
                line,
                head,
                value,
                floor,
            });
        }
    }
    out
}

/// 이 가드가 실제로 판정하는 집합 — 출하물이고, 선언 자리가 아니고, 0 이 아니고,
/// **전시 대상이 아닌** 것.
///
/// 전시 대상은 [`DISPLAY_SPECIMENS`] 에 **자리로** 적혀 있다 — 부류가 아니다. 그 수를
/// 토큰으로 바꾸면 specimen 이 보여주려던 것이 사라지고, 래칫에 남겨 두면 다음 사람이
/// 고칠 수 **없는** 자리를 향해 달린다. 빠진 몫은 [`considered`] 와
/// [`the_gallery_share_splits_into_four_kinds`] 가 계속 세므로 조용히 사라지지 않는다.
fn judged() -> Vec<Hit> {
    considered()
        .into_iter()
        .filter(|h| !is_a_registered_display_specimen(h))
        .collect()
}

/// 그 자리가 [`DISPLAY_SPECIMENS`] 에 **등록돼 있는가.** 등록되지 않은 전시 후보는
/// 빠지지 않는다 — 면제는 부류가 아니라 자리다.
fn is_a_registered_display_specimen(hit: &Hit) -> bool {
    DISPLAY_SPECIMENS
        .iter()
        .any(|(path, head, _, _)| hit.rel == *path && hit.head == *head)
}

/// 전시 대상을 빼기 **전**의 집합. 빼는 몫의 크기를 재는 쪽이 이것을 쓴다 — 빼 놓고
/// 그 수를 안 재면 그 부류는 존재하지 않는 것이 된다.
fn considered() -> Vec<Hit> {
    scan(true)
        .into_iter()
        .filter(|h| h.value != 0.0)
        .filter(|h| !h.floor)
        .filter(|h| !is_in_unit_space(h))
        .filter(|h| {
            !DECLARATION_SITES
                .iter()
                .any(|(p, _, _)| h.rel.starts_with(p))
        })
        .collect()
}

/// 경로 → 마스킹된 원문. 여러 판정이 같은 사본을 쓴다.
fn masked_sources() -> std::collections::BTreeMap<String, String> {
    rust_sources()
        .iter()
        .map(|(p, t)| (p.to_string_lossy().replace('\\', "/"), mask_non_code(t)))
        .collect()
}

/// 래칫 줄이 맡는 영역. 접두사가 긴 것부터 본다.
fn area_of(rel: &str) -> Option<&'static str> {
    AREAS
        .iter()
        .filter(|(p, _, _)| rel.starts_with(p))
        .max_by_key(|(p, _, _)| p.len())
        .map(|(p, _, _)| *p)
}

/// 한 자리에 **붙어 있는 주석 블록**. 선언 줄 바로 위의 연속된 `//`·`///` 줄이다.
///
/// 입력은 [`mask_literals`] 를 거친 사본이어야 한다 — 문자열만 덮고 **주석은 남긴다.**
/// [`mask_non_code`] 와 판정기가 다른 이유는 물음이 다르기 때문이다: 저쪽은 "코드에 X 가
/// 있나", 이쪽은 "**주석이 달려 있나**". 저 판정기로 물으면 주석이 통째로 사라져 답이
/// 항상 "없다" 가 된다.
fn attached_comment(masked_literals: &str, line: usize) -> String {
    let lines: Vec<&str> = masked_literals.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = line.saturating_sub(1);
    while i > 0 {
        let t = lines.get(i - 1).map_or("", |l| l.trim());
        if t.starts_with("//") {
            out.push(t);
            i -= 1;
        } else {
            break;
        }
    }
    out.reverse();
    out.join("\n")
}

/// 그 자리가 **디자인 산출물을 출처로 인용하는가.** 갤러리 specimen 은 디자인의 구조를
/// 1:1 로 전사하는 곳이라(`docs/design/systems/design-parity-notes.md` 의 구조 전사),
/// 거기서 나온 고정 치수는 스케일에서 뽑은 값이 아니라 **디자인이 그 자리에 정한 수**다.
fn cites_the_design(comment: &str) -> bool {
    const MARKS: &[&str] = &["jsx", "디자인", "시안", "design"];
    let lower = comment.to_lowercase();
    MARKS.iter().any(|m| lower.contains(m))
}

/// 그 줄이 **이름 붙은 치수를 선언**하는가(`const W: LogicalPx = LogicalPx(150.0);`).
///
/// 이름이 붙은 치수와 호출 인자에 그대로 박힌 수는 성질이 다르다. 앞쪽은 그 파일이 그
/// 수를 **하나의 치수로 인정**하고 이름과 사유를 붙인 것이고, 뒤쪽은 자리에 스며든
/// 여백이다. 처방도 다르다 — 뒤쪽은 테마 이름으로 옮기면 끝이지만, 앞쪽은 그 치수에
/// 대응하는 이름이 있어야 옮길 수 있다.
///
/// # 이 갈래의 이름 — **사본 축**
///
/// 갤러리 몫에서 이쪽이 제일 크다. 이 가드가 왜 판정하지 못하는지는 한 줄이다:
///
/// > **그 줄은 값을 쓰는 자리가 아니라 이름을 짓는 자리라, "리터럴을 토큰으로
/// > 바꿔라" 가 애초에 적용되지 않는다. 물어야 할 것은 "이 이름이 이미 있는 이름의
/// > 사본인가" 이고, 그 답은 값이 아니라 출처가 준다.**
///
/// 표본 하나가 그 물음을 그대로 보여준다. `catalog/popup_frame.rs` 는
/// `TITLE_BAR_HEIGHT` 를 28 로 **직접 선언**하는데, host 의 같은 치수는
/// `Theme.item_height_interactive` 에서 **파생**된다(`adapters/ui/popup.rs` 의 주석이
/// 그 유래를 적고 있다). 값이 같아서 지금은 아무 데도 안 걸리지만, 테마가 움직이면
/// host 만 따라 움직이고 갤러리는 그 자리에 남는다 — 갤러리가 존재하는 이유가 host
/// 와의 정합인데 그 정합이 **조용히** 깨지는 형태다. 값 비교로는 영영 안 보인다.
///
/// 그러나 이 갈래가 한 처방으로 쓸리지는 않는다. 같은 갈래 안에 있던 dag 캔버스의
/// 줌 상한은 길이 타입을 입은 **비율**이라 애초에 이 축의 대상이 아니었다(그래서
/// 타입을 벗겼다). 자리마다 출처를 물어야 하고, 그 물음은 이 가드 밖에 있다.
fn declares_a_named_dimension(masked: &str, line: usize) -> bool {
    let Some(text) = masked.lines().nth(line.saturating_sub(1)) else {
        return false;
    };
    text.contains("const ") && (text.contains(": LogicalPx =") || text.contains(": PhysicalPx ="))
}

#[test]
fn every_on_scale_literal_lives_inside_a_recorded_area() {
    let hits = judged();
    let stray: Vec<String> = hits
        .iter()
        .filter(|h| area_of(&h.rel).is_none())
        .map(|h| format!("  {}:{}  {}({})", h.rel, h.line, h.head, h.value))
        .collect();
    assert!(
        stray.is_empty(),
        "스케일 위의 값을 숫자로 쓴 자리가 래칫 밖에서 나왔다 — 토큰으로 바꾸거나, \
         영역을 AREAS 에 사유와 함께 등록하라:\n{}",
        stray.join("\n")
    );
}

#[test]
fn every_area_holds_exactly_the_count_it_records() {
    let hits = judged();
    let mut lines: Vec<String> = Vec::new();
    for (area, budget, why) in AREAS {
        let n = hits
            .iter()
            .filter(|h| area_of(&h.rel) == Some(*area))
            .count();
        if n != *budget {
            lines.push(format!("  {area}  기록 {budget} → 실측 {n}  ({why})"));
            // 남은 자리를 함께 낸다. 슬라이스는 "무엇이 남았나" 를 보고 고르는 작업이라,
            // 그 목록을 다른 도구로 다시 내면 두 도구의 수가 갈린다 — 이 축에서 이미
            // 한 번 겪었다(임시 스캐너의 수가 재현되지 않았다).
            //
            // 한 영역당 상한을 둔다. 테스트 하네스가 긴 실패 메시지를 자르기 때문이다 —
            // 실측으로 280 줄짜리 목록이 231 줄에서 잘렸고 **잘린 뒤쪽 영역은 통째로 안
            // 보였다.** 목록이 조용히 잘리면 "그 영역엔 자리가 없다" 로 읽힌다.
            const PER_AREA: usize = 40;
            let mine: Vec<&Hit> = hits
                .iter()
                .filter(|h| area_of(&h.rel) == Some(*area))
                .collect();
            for h in mine.iter().take(PER_AREA) {
                lines.push(format!(
                    "      {}:{}  {}({})",
                    h.rel, h.line, h.head, h.value
                ));
            }
            if mine.len() > PER_AREA {
                lines.push(format!(
                    "      … 외 {} 자리(메시지 길이 상한)",
                    mine.len() - PER_AREA
                ));
            }
        }
    }
    assert!(
        lines.is_empty(),
        "래칫이 실측과 어긋났다. 늘었으면 새 위반이고, **줄었으면 그 수를 같이 내려라** \
         — 남는 여유가 곧 안 보는 구간이다.\n\
         ★ 늘어서 빨간 경우 **이 수를 올리는 것은 이행이 아니다.** 아래 자리를 고치거나, \
         정말 못 고치면 왜 못 고치는지를 그 자리 주석에 적고 올려라 — 수만 올리면 그 자리는 \
         영영 안 보인다:\n{}",
        lines.join("\n")
    );
}

/// 그 값이 **specimen 이 전시하려고 바꾸는 값**인가. 같은 호출 머리가 그 파일에서 셋
/// 이상 나오고 서로 다른 값을 둘 이상 취하면, 그 수는 자리의 치수가 아니라 **보여주는
/// 대상**이다(`Spinner::new().size(12.0)` · `.size(24.0)` · `.size(14.0)`).
///
/// 토큰으로 바꾸면 전시가 사라진다 — 그래서 이 몫은 이 축의 위반이 아니다.
fn is_a_varied_specimen_value(masked: &str, hit: &Hit) -> bool {
    // 기하 생성자(`vec2`·`LogicalPx` …)는 어느 파일에나 여럿 나오고 값도 제각각이라
    // 변주 조건이 거의 항상 참이 된다. 전시는 **위젯을 설정하는 자리**에서 일어난다.
    const SETTERS: &[&str] = &[
        "size",
        "desired_width",
        "exact_width",
        "fixed_size",
        "max_height",
        "min_height",
        "max_width",
        "min_width",
    ];
    if !SETTERS.contains(&hit.head.as_str()) {
        return false;
    }
    let scale = size_scale();
    let same_head: Vec<f32> = on_scale_literals(masked, &scale)
        .into_iter()
        .filter(|(_, head, ..)| *head == hit.head)
        .map(|(_, _, v, _)| v)
        .collect();
    if same_head.len() < 3 {
        return false;
    }
    let mut distinct = same_head.clone();
    distinct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    distinct.dedup();
    distinct.len() >= 2
}

/// 갤러리 몫이 **한 물음인가**를 잰다. 래칫의 3 분의 1 이 갤러리 한 자리에 있어서,
/// 그 몫이 단일 항목이 아니면 분모가 틀린 것이다.
///
/// 두 성질로 가른다 — 그 자리가 **디자인을 출처로 인용하는가**와, 그 파일이 **토큰
/// 이름을 이미 쓰는가**. 뒤쪽이 참인데 앞쪽도 참이면 "짝 중 한쪽만 테마" 이고, 뒤쪽이
/// 거짓이면 그 파일은 애초에 토큰을 소비하는 자리가 아니다.
#[test]
fn the_gallery_share_is_one_question_or_it_is_not() {
    let files = rust_sources();
    let hits = considered();
    let (mut named_cited, mut named_plain) = (0usize, 0usize);
    let (mut inline_cited, mut inline_plain) = (0usize, 0usize);
    let mut with_comment = 0usize;
    for h in hits
        .iter()
        .filter(|h| h.rel.starts_with("crates/tasty-gallery/"))
    {
        let raw = files
            .iter()
            .find(|(p, _)| p.to_string_lossy().replace('\\', "/") == h.rel)
            .map_or("", |(_, t)| t.as_str());
        let comment = attached_comment(&tasty_doc_guards::source_text::mask_literals(raw), h.line);
        if !comment.is_empty() {
            with_comment += 1;
        }
        let named = declares_a_named_dimension(&mask_non_code(raw), h.line);
        match (named, cites_the_design(&comment)) {
            (true, true) => named_cited += 1,
            (true, false) => named_plain += 1,
            (false, true) => inline_cited += 1,
            (false, false) => inline_plain += 1,
        }
    }
    // 하한 — 주석 추출이 죽으면 `cites_the_design` 이 전부 거짓이 되고 이 단정은
    // 언제나 참이 된다. 그 상태의 0 은 "인용이 없다" 가 아니라 "안 읽었다" 다.
    assert!(
        with_comment >= 40,
        "주석이 붙은 자리가 {with_comment} 개뿐이다 — 주석 추출이 죽었다"
    );
    assert_eq!(
        (named_cited, named_plain, inline_cited, inline_plain),
        // 53 -> 52 · 14 -> 15 는 갤러리 port_scanner specimen 한 자리가 갈래를 옮긴
        // 것이다. 이름 붙은 치수였던 `FAV_COL_WIDTH`(28) 상수를 지우고
        // `theme.port_star_col_width()` 로 바꿨고(앞 갈래 -1), 컬럼 폭 리터럴이
        // 타입을 얻으면서 그중 200 이 인라인 자리로 보이게 됐다(뒤 갈래 +1).
        // 22 -> 27 은 `kb_import_export.rs` 의 명명 상수 다섯이다(jsx 인용 · 이름 붙은 치수).
        // 27 -> 25 는 그중 둘(`RECORD_SLOT_H` · `CARD_PAD_X`)이 지워지고 Theme 을 읽게 된 것이다.
        // 거기서 +1 은 `file_picker.rs` 의 `CRUMB_MAX_W`(180, jsx `FpCrumbs` maxWidth 인용)다.
        // 역할이 확정된 인라인 9자리를 명명 상수 8개로 옮겼다(같은 line gap은 공유).
        // 60 -> 66 · 6 -> 7 은 토큰 export 가 `size-*` 에 6 · 64 · 96 을 더해 바늘이
        // 넓어진 몫이다 — 갈래가 옮겨간 것이 아니라 전부터 있던 자리가 보이게 됐다.
        // 앞 둘(디자인 인용)이 그대로인 것이 그 증거다: 새로 보인 자리에는 인용이 없다.
        // 66 -> 68 은 위 둘(specimen 의 `*_PRIMITIVE_12`)이다 — 이름 붙은 치수이되
        // 디자인 인용이 아니라 본체 전사라 앞 갈래가 아니라 둘째로 센다.
        // 26 -> 27 · 68 -> 70 은 위 `crates/tasty-gallery/` 97 -> 100 의 셋이다. 스크롤
        // 위치 120 은 디자인 `scrollTo` 를 인용하고, 좁은 폭 220 · 높이 300 은 인용이 없다.
        (27, 70, 1, 7),
        "갤러리 몫의 갈래가 바뀌었다 — 이름 붙은 치수(앞 둘)와 인라인 여백(뒤 둘)은 \
         처방이 다르다. 인라인을 줄였으면 뒤의 수를, 치수에 이름을 줬으면 앞의 수를 내려라"
    );
}

/// 갤러리 몫을 네 갈래로 가른다. 앞의 갈래(모양)가 "무엇인가" 라면 이쪽은 "그 자리에
/// 이름이 있는가" 다.
///
/// # ★ 값이 같다는 것은 이름이 맞다는 뜻이 아니다
///
/// 둘째 갈래는 **"같은 값을 가진 `Theme` 이름이 존재한다"** 까지만 말한다. 그것을 한때
/// "고칠 수 있는 것" 이라고 불렀는데, 그 이름표가 틀렸다는 것이 열 자리를 손으로 읽어
/// 드러났다 — 13 중 자리에 맞는 이름을 가진 것은 3 이었다.
///
/// 나머지 열은 값만 겹친다: 툴바 높이 32 옆에 있는 이름은 `button_height_lg` 이고,
/// 스크롤 최대 높이 200 옆에 있는 이름은 전부 **너비**(`settings_sidebar_width` 등)다.
/// 그 이름을 갖다 쓰면 픽셀은 그대로인 채 **틀린 결합**이 생긴다 — 다음에 그 토큰이
/// 움직일 때 아무 상관 없는 자리가 따라 움직인다.
///
/// 그래서 이 갈래는 처방이 아니라 **후보**다. 자리에 맞는 이름인지는 술어가 아니라
/// 사람이 정한다. 술어로 정하려 하면 값 일치가 곧 처방이 되어, 위의 결합을 자동으로
/// 만들어 낸다.
#[test]
fn the_gallery_share_splits_into_four_kinds() {
    let files = rust_sources();
    let named_values = theme_named_values();
    // 양성 대조 — 이름 집합이 비면 아래 판정이 전부 "이름 없음" 으로 쏠린다.
    assert!(
        named_values.len() > 30 && named_values.contains(&28.0),
        "Theme 이름 값을 못 읽었다({} 개) — 그 상태의 판정은 전부 거짓이다",
        named_values.len()
    );

    let hits = considered();
    let (mut displayed, mut named_value, mut nameless, mut undecided) =
        (0usize, 0usize, 0usize, 0usize);
    for h in hits
        .iter()
        .filter(|h| h.rel.starts_with("crates/tasty-gallery/"))
    {
        let raw = files
            .iter()
            .find(|(p, _)| p.to_string_lossy().replace('\\', "/") == h.rel)
            .map_or("", |(_, t)| t.as_str());
        let masked = mask_non_code(raw);
        if is_a_varied_specimen_value(&masked, h) {
            displayed += 1;
        } else if !named_values.contains(&h.value) {
            nameless += 1;
        } else if declares_a_named_dimension(&masked, h.line) {
            undecided += 1;
        } else {
            named_value += 1;
        }
    }
    // 같은 값을 두 곳에 적지 않는다 — 전시 몫은 명부가, 나머지의 합은 `AREAS` 가 이미
    // 들고 있다. 여기 손으로 적는 것은 **갈래별 크기**뿐이고, 그 둘과의 정합은 단정이
    // 확인한다. 짝을 만들면 한쪽만 움직인다.
    let roster: usize = DISPLAY_SPECIMENS.iter().map(|(.., n, _)| n).sum();
    let ratcheted = AREAS
        .iter()
        .find(|(a, ..)| *a == "crates/tasty-gallery/")
        .map_or(0, |(_, n, _)| *n);
    assert_eq!(
        (displayed, named_value, nameless, undecided),
        // 10 -> 11 은 위와 같은 자리다 — 컬럼 최소폭 200 이 타입을 얻어 보이게 됐고,
        // 같은 값을 가진 `Theme` 이름이 있다(그 이름이 이 자리에 맞는다는 뜻은 아니다 —
        // 이 갈래의 doc 참조).
        // 1 -> 2 는 `file_picker.rs` 의 `CRUMB_MAX_W`(180)다 — 같은 값의 `Theme` 이름이 없다.
        // nudge·줄 간격·스크롤 상한·무대·정렬선의 역할 명명으로 인라인 후보 9개가 이동했다.
        // 2 -> 3 · 2 -> 0 은 토큰 export 가 `Theme` 이름 집합을 넓힌 결과다 —
        // 이름이 없던 둘(180 · 28)이 이름을 얻어 `nameless` 가 비었고, 그중 하나만
        // 선언 줄이 아니라 `named_value` 로 갔다. 자리가 고쳐진 것이 아니라 **처방이
        // 생긴 것**이다 — 값이 같다고 그 이름이 이 자리에 맞는다는 뜻은 아니라는
        // 이 갈래의 doc 이 그대로 적용된다.
        // 위 둘은 그 줄이 스스로 치수를 이름 짓는 갈래라 넷째가 그만큼 는다
        // (`ratcheted` 가 함께 올라 식은 그대로다).
        (roster, 3, 0, ratcheted - 3),
        "갤러리 몫의 갈래가 바뀌었다 — 전시(래칫 밖) · 같은 값의 이름이 있다 · 이름이 \
         없다 · 그 줄이 스스로 치수를 이름 짓는다"
    );
}

/// 전시 명부가 **실재를 가리키고, 그 크기가 기록과 같은가.** 그리고 명부 **밖**에
/// 전시로 보이는 자리가 없는가.
///
/// 앞쪽이 없으면 명부는 오타 하나로 조용히 비고, 빈 명부는 아무것도 안 뺀다(그러면
/// 이 축의 수가 소리 없이 늘어난다). 뒤쪽이 없으면 새 전시 specimen 이 등록 없이
/// 래칫에 쌓여 다음 사람이 고칠 수 없는 자리를 향해 달린다.
#[test]
fn the_display_roster_points_at_real_sites_and_covers_the_ones_that_look_like_it() {
    assert!(
        !DISPLAY_SPECIMENS.is_empty(),
        "전시 명부가 비었다 — 빈 명부는 아무것도 안 빼면서 언제나 통과한다"
    );
    let all = considered();
    for (path, head, budget, why) in DISPLAY_SPECIMENS {
        let n = all
            .iter()
            .filter(|h| h.rel == *path && h.head == *head)
            .count();
        assert_eq!(
            n, *budget,
            "전시 명부 `{path}` 의 `{head}`(사유: {why}) 자리가 {n} 개다. 늘었으면 전시가 \
             아닌 것이 섞였을 수 있고, 줄었으면 그 수를 같이 내려라"
        );
    }

    let masked = masked_sources();
    let unregistered: Vec<String> = all
        .iter()
        .filter(|h| !is_a_registered_display_specimen(h))
        .filter(|h| {
            masked
                .get(&h.rel)
                .is_some_and(|m| is_a_varied_specimen_value(m, h))
        })
        .map(|h| format!("  {}:{}  {}({})", h.rel, h.line, h.head, h.value))
        .collect();
    assert!(
        unregistered.is_empty(),
        "전시로 보이는데 명부에 없는 자리가 있다 — 전시가 맞으면 사유와 함께 등록하고, \
         아니면 토큰으로 바꿔라:\n{}",
        unregistered.join("\n")
    );
}

/// 스케일이 비면 `contains` 가 항상 거짓이 되어 이 가드 전체가 조용히 통과한다.
#[test]
fn the_size_scale_is_not_empty() {
    let scale = size_scale();
    assert!(
        scale.len() > 10 && scale.contains(&24.0),
        "size-* 스케일을 못 읽었다({} 개). `{SIZE_SCALE_SOURCE}` 가 옮겨졌거나 형태가 \
         바뀌었다 — 그 상태의 0 은 통과가 아니라 측정 실패다",
        scale.len()
    );
}

/// 사각의 크기를 실측으로 든다. 사각을 적어 두기만 하면 그 수는 낡고, 낡은 수는
/// "여기엔 없다" 로 읽힌다.
#[test]
fn the_blind_spots_are_still_the_size_they_say() {
    let all = scan(false);
    let shipped = scan(true);
    let zeros = shipped.iter().filter(|h| h.value == 0.0).count();
    let in_tests = all.len() - shipped.len();
    let floors = shipped.iter().filter(|h| h.value != 0.0 && h.floor).count();
    let unit_space = shipped
        .iter()
        .filter(|h| h.value != 0.0 && !h.floor && is_in_unit_space(h))
        .count();
    assert_eq!(
        (zeros, in_tests),
        // Zero spacings and nonnegative bounds are counted, but are not token choices.
        // file_picker::entry_row clamps the remaining filename-column width to zero
        // when fixed columns consume it. This adds one shipped zero, not a size token.
        // The test-only census also includes the narrow-width file-picker viewport:
        // vec2(400, 360) contributes 400 (on size-*), while 360 is off that scale.
        // Both sites remain visible to this census; neither needs a new exemption.
        // 178 -> 176: the file handler picker's canonical transcription dropped its two
        // shipped zeros. Its zero item_spacing now says egui::Vec2::ZERO, and the
        // fallback-strip height folds to zero by scaling instead of by a LogicalPx(0.0)
        // literal. Nothing became invisible — the sites are gone, not exempted.
        // 220 -> 221: the file picker's scope census opens a second narrow viewport,
        // vec2(400, 360), to place a surface-scoped parent. Its 400 is on size-*, its
        // 360 is not, so the test-only side grows by one while the shipped side does
        // not move. Measured by shifting that literal off the scale: the pair returned
        // to 220 and came back to 221 when it was put back.
        // 176 -> 175: 갤러리의 remote 탭 스트립 미러(`tab_btn`)가 지워지고 그 자리가 공용
        // view 호출로 바뀌면서, 그 미러가 들고 있던 `rect_filled(bar, 0.0, ..)` 의 0 이
        // 함께 없어졌다. 공용 view 에도 같은 모양의 0 이 있지만 그것은 전부터 세고 있던
        // 자리다 — 사본이 하나 줄어든 것이지 안 보게 된 것이 아니다.
        // 175 -> 177: clipboard viewer 의 compact 세그먼트 specimen 이다. 둘 다 치수
        // 선택이 아니다 — 하나는 type-bar 배경의 corner radius 0 이고(옆의 2 종 세그먼트
        // 행이 이미 같은 자리를 들고 있다), 하나는 아이콘 전용 세그먼트에서 라벨 앞 gap 이
        // **없다**는 뜻의 0 이다(plugin `view.rs` 의 `row_gap` 과 같은 갈래).
        // 221 -> 225: command palette 의 높이 테스트 둘이 원점을 네 번 적는다 —
        // view 하네스가 카드 rect 를 `pos2(0.0, 0.0)` 에서 띄우고, sizer 배선 테스트가
        // 창 rect 를 `PhysicalPx(0.0)` 둘로 시작한다. 넷 다 0 이라 출하 쪽 `zeros` 는
        // 안 움직인다. 치수 선택이 아니라 좌표 원점이다.
        // 177 -> 178: scrim scope specimen 이 surface radius 를 결정표가 주는 값으로 적고,
        // 오늘 그 값이 0 이다. 출하되는 `LogicalPx(0.0)` 이 여기서는 명시된 디자인 사실이지
        // 크기 토큰이 아니다. lane 이 그 상수를 0 에서 옮겨 재니 출하 쪽이 하나 줄었다.
        // 225 -> 239: scrim scope 시험들이 egui 좌표로 셸을 배치하고 그 픽스처 수 열넷이
        // size-* 위에 있다. 열셋은 순수 판정 시험의 2-surface 셸이고(그 블록을 지워 재니
        // 시험 쪽이 돌아왔다), 열넷째는 칠하는 scrim 픽스처의 400 point surface 폭이다.
        // 239 -> 246: 파일 핸들러 lane 의 시험 일곱이 프레임 폭·advance 를 픽스처로
        // 적는다. 출하 쪽 178 은 안 움직였다 — 시험 쪽만 자란 것이라 이 가드가 안 보는
        // 구간이 넓어진 것이 아니다.
        // 178 -> 182: 프리셋 surface 설정 화면(본체 `surface_settings.rs` 와 갤러리 specimen)이
        // 헤더·footer 를 가로로만 줄이는 `shrink2(vec2(x, 0.0))` 를 둘씩 쓰고(+4), 선택
        // leaf 핸들을 가로로만 옮기는 `translate(vec2(-.., 0.0))` 가 본체·갤러리에 하나씩
        // 생겼다(+2). 지운 인라인 폼의 `.max(LogicalPx(0.0))` 둘이 빠져 순증은 넷이다.
        // 넷 다 "그 축으로는 움직이지 않는다" 는 0 이지 크기 토큰 선택이 아니다.
        (182, 246),
        "0.0 사각과 테스트 사각의 크기가 바뀌었다. 늘었으면 이 가드가 안 보는 구간이 \
         자란 것이고, 줄었으면 그 수를 같이 내려라"
    );
    // 정규화 좌표 몫은 명부가 이미 들고 있다 — 같은 값을 두 곳에 적지 않는다.
    let roster: usize = UNIT_SPACE_SITES.iter().map(|(.., n, _)| n).sum();
    assert_eq!(
        (floors, unit_space),
        // 하한 몫은 여기서만 센다. 전부 `(a - b).max(PhysicalPx(1.0))`(퇴화한 rect 방지)
        // 와 `(a - b).abs() < PhysicalPx(1.0)`(서브픽셀 동일성 문턱)이다.
        (18, roster),
        "1 의 두 사각(퇴화 방지 하한 · 정규화 좌표)의 크기가 바뀌었다. 하한이 늘었으면 \
         새 clamp 이고, 줄었으면 그 수를 같이 내려라 — 이 둘은 래칫 밖이라 여기서만 보인다"
    );
    for (path, head, budget, why) in UNIT_SPACE_SITES {
        let n = shipped
            .iter()
            .filter(|h| h.value != 0.0 && h.rel == *path && h.head == *head)
            .count();
        assert_eq!(
            n, *budget,
            "정규화 좌표 명부 `{path}` 의 `{head}`(사유: {why}) 자리가 {n} 개다. \
             늘었으면 픽셀인 것이 섞였을 수 있고, 줄었으면 그 수를 같이 내려라"
        );
    }
    for (site, budget, why) in DECLARATION_SITES {
        let n = shipped.iter().filter(|h| h.rel.starts_with(site)).count();
        assert_eq!(
            n, *budget,
            "선언 자리 `{site}`(사유: {why})의 건수가 바뀌었다"
        );
    }
}

#[cfg(test)]
mod detector {
    use super::*;

    const SCALE: &[f32] = &[0.0, 1.0, 4.0, 24.0, 400.0];

    fn hits(src: &str) -> Vec<(usize, String, f32)> {
        on_scale_literals(&mask_non_code(src), SCALE)
            .into_iter()
            .map(|(line, head, value, _)| (line, head, value))
            .collect()
    }

    #[test]
    fn it_reads_a_literal_sitting_in_a_length_position() {
        assert_eq!(
            hits("let s = egui::vec2(400.0, 24.0);"),
            vec![(1, "vec2".to_owned(), 400.0), (1, "vec2".to_owned(), 24.0)]
        );
        assert_eq!(
            hits("LogicalPx(24.0)"),
            vec![(1, "LogicalPx".to_owned(), 24.0)]
        );
    }

    #[test]
    fn a_zero_width_bound_is_counted_and_a_nonzero_bound_is_not_a_floor_exemption() {
        let zero = "(name_right - name_left).max(LogicalPx(0.0))";
        assert_eq!(hits(zero), vec![(1, "LogicalPx".to_owned(), 0.0)]);
        // A real size at the same location must stay visible; do not exempt max().
        let dimension = "(name_right - name_left).max(LogicalPx(24.0))";
        assert_eq!(hits(dimension), vec![(1, "LogicalPx".to_owned(), 24.0)]);
        assert!(floors(dimension).is_empty());
    }

    #[test]
    fn a_test_viewport_is_counted_before_gating_but_not_as_shipped_geometry() {
        let test =
            "#[cfg(test)]\nmod layout_tests { fn narrow_width() { egui::vec2(400.0, 360.0); } }";
        assert_eq!(hits(test), vec![(2, "vec2".to_owned(), 400.0)]);
        assert!(hits(&blank_test_modules(&mask_non_code(test))).is_empty());
        // Removing the test gate makes the identical viewport a shipping candidate.
        let shipped = test.replace("#[cfg(test)]\n", "");
        assert_eq!(
            hits(&blank_test_modules(&mask_non_code(&shipped))),
            vec![(1, "vec2".to_owned(), 400.0)]
        );
    }

    /// 곱셈·나눗셈의 피연산자는 길이가 아니라 배율이다. 이 술어가 없으면 `0.5` 하나가
    /// 최다값이 되어 축이 비율에 파묻힌다 — 가드의 판별력이 여기 걸려 있다.
    #[test]
    fn a_factor_is_not_a_length() {
        assert!(hits("egui::vec2(d * 4.0, 1.0 / n)").is_empty());
        assert!(hits("egui::pos2(w / 4.0, 24.0 * k)").is_empty());
        // 같은 값이 곱셈 밖에 있으면 다시 길이다 — 값이 아니라 자리가 판정한다.
        assert_eq!(
            hits("egui::vec2(4.0, h)"),
            vec![(1, "vec2".to_owned(), 4.0)]
        );
    }

    /// 스케일 밖의 값은 이 가드의 대상이 아니다. 그쪽은 ADR-0126 이 다루는 축이고,
    /// 처방이 반대다(토큰으로 스냅하지 **않는다**).
    #[test]
    fn a_value_off_the_scale_is_not_this_guards_business() {
        assert!(hits("egui::vec2(17.0, 13.0)").is_empty());
    }

    /// 자리가 가족을 정한다. `radius-*`·`font-size-*` 를 재는 자리는 여기서 안 본다 —
    /// 합집합으로 재면 부류 하나가 통째로 숨는다.
    #[test]
    fn another_familys_position_is_not_measured_here() {
        assert!(hits(".corner_radius(4.0)").is_empty());
        assert!(hits("FontId::proportional(24.0)").is_empty());
    }

    /// 코드가 아닌 자리의 수는 코드가 아니다. 실제로 `\"window-button-size(24) …\"` 의
    /// 24 가 `.size(` 인자로 계상돼 축의 수를 4 만큼 부풀린 적이 있다.
    #[test]
    fn a_number_inside_prose_is_not_code() {
        assert!(hits("// vec2(24.0) 이라고 쓰면 된다").is_empty());
        assert!(hits("let doc = \"size(24.0) 원형\";").is_empty());
    }

    /// 인자 자리가 아니면 머리가 없다 — 블록이나 구조체 리터럴 안의 값은 이 술어가
    /// 판정하지 않는다(모듈 문서의 "지역 변수 선언" 사각과 같은 이유).
    #[test]
    fn a_value_outside_an_argument_list_has_no_head() {
        assert!(hits("ui.horizontal(|ui| { let x = 24.0; });").is_empty());
    }

    /// **첨자는 길이가 아니다.** `head_span_of` 의 doc 이 "짝 없는 여는 기호가 `(` 가
    /// 아니면 인자 자리가 아니다" 라고 말하는데 `[` 만 구현에서 빠져 있었고, 그 틈으로
    /// `p[1]` 의 1 이 바깥 `vec2(` 의 인자로 계상됐다.
    #[test]
    fn a_subscript_is_not_a_length() {
        assert!(hits("egui::vec2(p[1], q)").is_empty());
        assert!(hits("egui::vec2(pts[0][1], q)").is_empty());
        // 같은 자리의 진짜 길이는 그대로 잡힌다 — 대괄호가 머리를 가리는 것이지
        // 그 호출 전체를 면제하는 것이 아니다.
        assert_eq!(
            hits("egui::vec2(p[1], 24.0)"),
            vec![(1, "vec2".to_owned(), 24.0)]
        );
    }

    fn floors(src: &str) -> Vec<f32> {
        on_scale_literals(&mask_non_code(src), SCALE)
            .into_iter()
            .filter(|(.., floor)| *floor)
            .map(|(_, _, v, _)| v)
            .collect()
    }

    /// clamp·문턱 자리의 `1` 은 치수가 아니라 퇴화 방지 하한이다. 그리고 그 판정은
    /// **자리와 값 둘 다**를 본다 — 자리만 보면 진짜 치수 하한까지 숨는다.
    #[test]
    fn the_degenerate_floor_is_not_a_dimension() {
        assert_eq!(floors("(h - t).max(PhysicalPx(1.0))"), vec![1.0]);
        assert_eq!(floors("w.min(LogicalPx(1.0))"), vec![1.0]);
        assert_eq!(floors("(a - b).abs() < PhysicalPx(1.0)"), vec![1.0]);
        assert_eq!(floors("d <= LogicalPx(1.0)"), vec![1.0]);
        // 값이 1 이 아니면 그 자리는 진짜 치수 하한이다.
        assert!(floors("w.max(LogicalPx(24.0))").is_empty());
        // 대입·매치 팔·반환 화살표는 비교가 아니다.
        assert!(floors("let x = LogicalPx(1.0);").is_empty());
        assert!(floors("MouseWheelUnit::Point => LogicalPx(1.0),").is_empty());
        assert!(floors("fn f() -> LogicalPx(1.0)").is_empty());
        // 이름을 짓는 줄은 처방이 달라 일부러 안 잡는다.
        assert!(floors("const GLYPH_STROKE: LogicalPx = LogicalPx(1.0);").is_empty());
    }
}
