# Gallery-first — 새 UI 컴포넌트 추가 순서

**새 modal · popup · 공용 위젯은 본체에 넣기 전에 갤러리(`crates/tasty-gallery`)에 먼저 만든다.** 갤러리는 본체 UI 컴포넌트를 한곳에서 확인하는 카탈로그이며([gallery-completeness](../design/policies/gallery-completeness.md), [테마 시스템 (운영 상세)](../design/systems/theme.md)), 새 컴포넌트는 여기서 확인한 뒤 본체에 연결한다.

## 순서 (필수)

### 0. 디자인 확보 (acquisition)
디자인에 없는 새 요소(디자인에 없는 컴포넌트를 추가하거나, 디자인과 다른 형태로 바꾸는 것)라면 **소스부터 고치지 않는다.** [디자인 변경 워크플로](design-change-workflow.md)대로 디자인 요청문서를 작성하고, 디자이너 세션이 tasty-page 갤러리(`vendor/gallery/`)에 디자인을 먼저 반영하게 한다. tasty-page 갤러리에 디자인이 있어야 1단계로 간다. (이 단계는 디자인을 *확보*하는 것이고, 받은 디자인을 소스 구조로 옮기는 *구조 전사*는 1 단계다 — 아래 참고.)

> 디자인에 *이미 있는데* 소스만 못 따라간 경우(구현 누락/불일치)는 디자인 변경 불필요 — 1 단계로 바로 간다.

### 1. 갤러리 specimen 먼저
받은 디자인으로 갤러리에 specimen 을 만든다: `catalog/{components,widgets}/<name>.rs` 의 `draw(ui, &Theme)` + `catalog.rs::pages()` 의 해당 페이지에 `section(...)`/`spec(...)` 등록. 값과 구조를 각각 확인한다:

- **토큰 정합**: 색·간격·치수·보더는 모두 Theme 토큰에서 가져온다([theme UI 규칙](../design/systems/theme.md#ui-디자인-규칙-필수)). 보편 이름이 붙는 부품(버튼/입력/표 등)은 [공용 위젯](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로)을 호출한다.
- **구조 전사(structural transcription)**: 디자인의 **레이아웃 구조**(grid·컬럼·패딩·정렬·요소 경계)를 egui 소스에 **1:1 전사**한다 — egui flow 로 눈대중 흉내 내지 않는다. 토큰만 맞고 구조가 어긋나면 specimen이 디자인과 달라진다(전사 절차·함정은 [`design-parity-notes`](../design/systems/design-parity-notes.md) 의 "구조 전사" 원칙, 매핑은 [`design-gallery-mapping`](../design/systems/design-gallery-mapping.md)).
- **캡처 대조**: specimen 은 `TASTY_GALLERY_SHOT` 으로 찍고, 시작 테마는 `TASTY_GALLERY_THEME=mocha|latte` 로 골라 두 테마를 각각 찍는다. 형식과 예시는 [갤러리 캡처](../ai-verification/screenshot-methods.md#tasty-gallery-캡처-tasty_gallery_shot).

### 2. 본체 반영
그 후 본체 앱에 넣는다(팝업이면 [popup-implementation](popup-implementation.md) 의 `PopupDef` 3단계).  본체와 갤러리가 **같은 view-only 함수**를 호출하도록 props 를 분리한다([model-view-split](model-view-split.md)). 새로 그리지 말고 1 단계에서 만든 함수를 본체에서 호출한다.

<a id="이미-본체에만-있는-view-를-갤러리로-끌어올릴-때"></a>

## 이미 본체에만 있는 view 를 갤러리로 옮길 때

gallery-first 이전에 만들어져 본체 binary 에만 있는 view 는 갤러리가 복제할 수밖에 없다. 복제를 없애려면 **view 함수를 `crates/tasty-ui-widgets` 로 옮겨** 본체 wrapper 와 갤러리 specimen 이 같은 함수를 호출하게 한다. 이때 crate 로 **넘기지 않는 것**이 정해져 있다:

- **`egui::Area` / `LayerId`** — 부유 배치와 z-order 는 본체 정책이다. crate view 는 넘겨받은 `egui::Ui` 안에 크기를 할당하고 **그 rect 기준**으로만 그린다(화면 절대 좌표 사용 금지 — 갤러리 카드는 임의 위치에 있다).
- **i18n** — 위젯 crate 는 `tasty-i18n` 을 의존하지 않는다. 라벨·tooltip 은 props 필드로 올려 본체 wrapper 가 주입한다. 그리기와 폭 계산이 **같은 필드**를 읽게 해서 문자열이 두 곳에서 독립 조립되지 않게 한다.
- **글로벌 `theme()` 접근** — crate 함수는 항상 `&Theme` 을 명시적으로 받는다.

선례: StatusBar(`crates/tasty-ui-widgets/src/status_bar.rs` ↔ `src/adapters/ui/status_bar.rs` ↔ `catalog/components/status_bar.rs`), 공통 그래프 뷰(`crates/tasty-ui-widgets/src/graph_view/` ↔ `src/adapters/ui/surface/dag_graph/` ↔ `catalog/components/dag/`, [graph-view](../design/systems/graph-view.md)).

## 무엇이 이 순서를 강제하는가

절차만 적어 두면 지켜지지 않는다. 아래 셋이 `src/source_guards/` 에 있고 본체 lib의 단위
테스트로 돈다.

- **등록처 ↔ specimen**(`gallery_specimen_parity`) — 본체 popup 등록처(`all_defs`)와 전체화면
  무대 표(`all_metas`)의 각 항목에 대응하는 갤러리 specimen 이 명부에 있는지 본다. 새 popup 이
  본체에만 들어오면 그 자리에서 실패한다. popup 쪽은 `gui` 조합에서만 돈다.
- **재수출 위젯 ↔ 갤러리**(`gallery_widget_coverage`) — `tasty-ui-widgets` 가 재수출한 모듈의
  항목 중 적어도 하나를 갤러리 소스가 이름으로 부르는지 본다. `pub use` 를 우회해 `pub mod` 로
  내보내는 길은 `pub mod` 명부가 막는다.
- **되풀이한 것이 아직 같은가**(`gallery_copied_dimensions`) — 아래
  "복제를 없앨 수 없을 때" 참조.

이 검사들은 specimen의 등록 여부와 복제한 치수의 일치 여부를 확인한다.
디자인과의 일치, 페이지 배치, 실제 그리기 호출은 별도로 확인해야 한다.
위젯 이름만 참조하고 그리지 않는 경우도 위젯 사용 검사에는 통과할 수 있다.

## 복제를 없앨 수 없을 때

먼저 본체와 갤러리가 같은 view 함수나 상수를 사용하도록 합친다.
크레이트 의존성 때문에 합칠 수 없다면 원본과 복제본의 관계를 기록하고 변경 시 비교한다.

- **치수 값**: `gallery_copied_dimensions`에 원본과 복제본을 등록한다.
  리터럴뿐 아니라 테마에서 계산한 값도 비교한다. 원본을 가리키는 주석이 있는 상수는
  비교 목록과 이름이 일치해야 하므로, 주석만 지워 검사에서 빠져나갈 수 없다.
- **공유 값과 의도된 차이**: 비교에서 제외한 이유를 남긴다.
  검사는 공용 상수를 실제로 참조하는지, 본체에서도 사용하는지, 대응 상수가 새로 생겼는지까지 확인한다.
  어떤 폭을 따라야 하는지 같은 디자인 판단은 사람이 검토한다.
- **검사 범위의 한계**: 복제 사실을 주석에 적지 않은 새 상수는 빠질 수 있다.
  타입·이름·값이 같다는 이유만으로 복제 관계를 추정하지 않는다.
  특히 값이 같은 항목만 묶으면 값이 달라졌을 때 비교 대상에서도 빠진다.
  초기화식이 공용 토큰 크레이트를 직접 참조하는 경우는 별도로 검사한다.
- **종류와 값의 대응 관계**: 현재 별도의 비교 검사는 없다.
  토스트 종류별 색상처럼 공용 정의를 직접 사용하면 복제 자체가 필요 없다.
  새로운 대응표를 복제해야 한다면 먼저 공유할 수 있는지 확인하고, 불가능할 때만 비교 검사를 추가한다.

배치 계산식은 별도 검사를 늘리기보다 공용 함수로 합친다.
`tasty_ui_widgets::top_right_inset_square`가 그 예다.

## specimen 기하 이름

갤러리의 nudge·열 정렬선·데모 anchor·겹침 영역은 해당 역할의 `LogicalPx` 상수와
바로 옆 사유로 표현한다. 예를 들어 `switch_overlay`의 이름/설명 줄 간격은 행 높이와
설명 중심 계산이 같은 상수를 읽고, `tutorial`의 topic 목록 상한과 topic 무대 높이는
서로 다른 역할로 둔다. 타일 nudge를 선 굵기 토큰에, 줄번호 정렬선을 행 높이 토큰에
연결하지 않는다. 수가 같다는 것은 역할이 같다는 근거가 아니다.

[무대 치수 예외](../design/policies/gallery-completeness.md#무대-치수는-액자다--그리드배율-축의-모수-밖)는
기존 숫자와 배치식을 유지하는 명명에도 적용한다. 갤러리 배율은 egui 전역 zoom을
거치므로 이름을 붙일 때 별도의 Theme 배율을 곱하지 않는다. 이름의 적절성은 소스의
소비식을 읽어 판단하며, on-scale 가드의 계수 감소가 완료 조건은 아니다.

## 왜 이 순서인가

- **검증을 먼저 세운다**: 갤러리는 본체 앱을 다 띄우지 않고 컴포넌트 하나만 격리 렌더한다. 디자인 정합을 빠르게 반복할 검증대를 본체 연결보다 먼저 갖는다.
- **누락을 줄인다**: 갤러리에 먼저 등록해 본체에만 있는 컴포넌트를 줄인다([gallery-completeness](../design/policies/gallery-completeness.md)).
- **공용 구현을 유지한다**: 갤러리용으로 분리한 view-only 함수를 본체가 그대로 호출하므로 두 환경에서 같은 구현을 검증할 수 있다.

## 관련

- [design-change-workflow](design-change-workflow.md) — 0단계 "디자인 확보"의 전체 절차(요청문서→시안→정합 루프).
- [테마 시스템 (운영 상세)](../design/systems/theme.md) — cut 금지 + gallery-first 결정 근거.
- [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) — 갤러리 완전성 운영 상태.
- [popup-implementation](popup-implementation.md) — 본체 팝업 추가(`PopupDef`).
- [architecture/ui-widgets-crate › 무엇을 공용 위젯으로](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로) · [model-view-split](model-view-split.md) — 부품 단위 공용화 + view 분리.
- [design/systems/design-gallery-mapping §공용 crate view specimen](../design/systems/design-gallery-mapping.md) — 이미 이 경로로 올라간 항목 목록.
