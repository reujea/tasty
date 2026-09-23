# Gallery-first — 새 UI 컴포넌트 추가 순서

**새 modal · popup · 공용 위젯은 본체에 넣기 전에 갤러리(`crates/tasty-gallery`)에 먼저 만든다.** 갤러리는 본체의 모든 UI 컴포넌트를 노출하는 단일 출처이고([gallery-completeness](../design/policies/gallery-completeness.md), [ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md)), 컴포넌트는 그 출처를 *거쳐서* 본체로 들어온다.

## 순서 (필수)

### 0. 디자인 확보 (acquisition)
디자인에 없는 새 요소(디자인에 없는 컴포넌트를 추가하거나, 디자인과 다른 형태로 바꾸는 것)라면 **소스부터 고치지 않는다.** [디자인 변경 워크플로](design-change-workflow.md)대로 디자인 요청문서를 작성해 claude design 에게서 디자인을 먼저 받는다. (이 단계는 디자인을 *확보*하는 것이고, 받은 디자인을 소스 구조로 옮기는 *구조 전사*는 1 단계다 — 아래 참고.)

> 디자인에 *이미 있는데* 소스만 못 따라간 경우(구현 누락/불일치)는 디자인 변경 불필요 — 1 단계로 바로 간다.

### 1. 갤러리 specimen 먼저
받은 디자인으로 갤러리에 specimen 을 만든다: `catalog/{components,widgets}/<name>.rs` 의 `draw(ui, &Theme)` + `catalog.rs::pages()` 의 해당 페이지에 `section(...)`/`spec(...)` 등록. 정합은 **두 개의 독립 축**으로 맞춘다:

- **토큰 정합**: 색·간격·치수·보더는 모두 Theme 토큰에서 가져온다([theme UI 규칙](../design/systems/theme.md#ui-디자인-규칙-필수)). 보편 이름이 붙는 부품(버튼/입력/표 등)은 [공용 위젯](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로)을 호출한다.
- **구조 전사(structural transcription)**: 디자인의 **레이아웃 구조**(grid·컬럼·패딩·정렬·요소 경계)를 egui 소스에 **1:1 전사**한다 — egui flow 로 눈대중 흉내 내지 않는다. 토큰만 맞고 구조가 어긋나면 specimen 이 드리프트한다(전사 절차·함정은 [`design-parity-notes`](../design/systems/design-parity-notes.md) 의 "구조 전사" 원칙, 매핑은 [`design-gallery-mapping`](../design/systems/design-gallery-mapping.md)).

### 2. 본체 반영
그 후 본체 앱에 넣는다(팝업이면 [popup-implementation](popup-implementation.md) 의 `PopupDef` 3단계). demo=main 이므로 본체와 갤러리가 **같은 view-only 함수**를 호출하도록 props 를 분리한다([model-view-split](model-view-split.md)). 새로 그리지 말고 1 단계에서 만든 함수를 본체에서 호출한다.

## 이미 본체에만 있는 view 를 갤러리로 끌어올릴 때

gallery-first 이전에 만들어져 본체 binary 에만 있는 view 는 갤러리가 복제할 수밖에 없다. 복제를 없애려면 **view 함수를 `crates/tasty-ui-widgets` 로 옮겨** 본체 wrapper 와 갤러리 specimen 이 같은 함수를 호출하게 한다. 이때 crate 로 **넘기지 않는 것**이 정해져 있다:

- **`egui::Area` / `LayerId`** — 부유 배치와 z-order 는 본체 정책이다. crate view 는 넘겨받은 `egui::Ui` 안에 크기를 할당하고 **그 rect 기준**으로만 그린다(화면 절대 좌표 사용 금지 — 갤러리 카드는 임의 위치에 있다).
- **i18n** — 위젯 crate 는 `tasty-i18n` 을 의존하지 않는다. 라벨·tooltip 은 props 필드로 올려 본체 wrapper 가 주입한다. 그리기와 폭 계산이 **같은 필드**를 읽게 해서 문자열이 두 곳에서 독립 조립되지 않게 한다.
- **글로벌 `theme()` 접근** — crate 함수는 항상 `&Theme` 을 명시적으로 받는다.

선례: StatusBar(`crates/tasty-ui-widgets/src/status_bar.rs` ↔ `src/adapters/ui/status_bar.rs` ↔ `catalog/components/status_bar.rs`).

## 무엇이 이 순서를 강제하는가

절차만 적어 두면 지켜지지 않는다. 아래 셋이 `src/source_guards/` 에 있고 본체 bin 의 유닛
테스트로 돈다.

- **등록처 ↔ specimen**(`gallery_specimen_parity`) — 본체 popup 등록처(`all_defs`)와 전체화면
  무대 표(`all_metas`)의 각 항목에 대응하는 갤러리 specimen 이 명부에 있는지 본다. 새 popup 이
  본체에만 들어오면 그 자리에서 실패한다. popup 쪽은 `gui` 조합에서만 돈다.
- **재수출 위젯 ↔ 갤러리**(`gallery_widget_coverage`) — `tasty-ui-widgets` 가 재수출한 모듈의
  항목 중 적어도 하나를 갤러리 소스가 이름으로 부르는지 본다. `pub use` 를 우회해 `pub mod` 로
  내보내는 길은 `pub mod` 명부가 막는다.
- **되풀이한 것이 아직 같은가**(`gallery_copied_dimensions`) — 아래
  "복제를 없앨 수 없을 때" 참조.

강제되지 않는 것도 함께 적는다. 이 셋은 **specimen 이 있는가**와 **되풀이한 값이 같은가**를
묻지(매핑은 지금 아무 가드도 안 본다 — 아래 "매핑 축" 참조), specimen 이 디자인과 맞는지도, 어느 페이지에 얹혔는지도, 갤러리가 그 위젯을 실제로
*그리는지*(이름만 부르고 안 그려도 참이다)도 묻지 않는다.

## 복제를 없앨 수 없을 때 — 갈라짐을 시끄럽게 만든다

위 "본체에만 있는 view 를 끌어올릴 때" 대로 공용 함수로 합치는 것이 1 순위다. 크레이트 경계가
막아 합칠 수 없으면(정본이 무거운 의존을 끌고 오는 경우 등) 복제는 남는다. 그때 필요한 것은
통일이 아니라 **갈라졌을 때 조용하지 않은 것**이고, 두 축이 있다:

- **값 축**(`gallery_copied_dimensions`) — specimen 이 되풀이한 본체 치수가 아직 같은가.
  한쪽이 리터럴이고 다른 쪽이 테마 파생이어도 잡는다. 그리고 갤러리 상수의 doc 이 본체를
  지목하면 그 주장이 명부에서 해소되는지를 함께 묻는다 — 자백해 놓고 아무도 확인하지 않는
  자리가 그 판별자에 걸린다. **명부에서 해소된다는 것과 그 사유가 참이라는 것은 다르다** —
  면제 명부(공유 항목 · 의도된 차이)의 사유 중 사실 진술인 것은 따로 다시 묻는다: 공유라고
  적은 자리가 아직 그 항목을 가리키는 경로식인가, 본체가 아직 그 항목을 **코드에서** 부르는가,
  의도된 차이라고 적은 자리에 대응하는 이름이 본체에 생기지 않았는가. 의미 판단인 사유(무엇의
  폭을 상속하는가 등)는 묻지 않으며, **묻지 않는다는 것을 그 자리에 적어 둔다.**
  그리고 자백한 자리의 **모집단 자체를 이름으로 못 박는다** — 모집단이 doc 의 낱말 하나로
  정해지므로, 그 낱말만 지우면 자리가 모집단에서 사라져 판정이 조용히 초록이 된다. 명부와
  집합 동등을 요구하면 그 삭제가 이름을 대며 빨개진다.

  **산문을 구조 신호로 대체할 수 있는지는 재 보고 접었다.** 갤러리 길이 상수 전체를 모수로
  후보 넷을 셌을 때, 타입은 신호가 아니고(전부 걸린다), 이름 일치는 태반이 무관한 짝이며
  (흔한 이름 하나가 여러 specimen 에서 같은 본체 상수와 짝지어진다), **값 일치는 자기 목적을
  지운다** — 갈라지는 순간 짝이 아니게 되므로 정확히 말해야 할 때 침묵한다. 지금 명부에 오른
  자리를 이름 신호가 절반도 못 집는 것도 함께 쟀다: 못 집는 쪽이 하필 이름이 다른 사본,
  본체가 테마 파생이라 상수가 없는 자리, 공용 토큰을 읽어 값이 소스에 없는 자리다.
  산문에 안 기대는 신호가 **하나** 있고(초기화식이 공용 토큰 크레이트를 가리킨다) 그
  부분집합은 완전히 강제한다. **자백한 적 없는 새 사본은 여전히 어느 신호에도 안 걸린다.**
- **매핑 축에는 지금 가드가 없다.** 한때 있었다 — specimen 이 되풀이한 **매핑**(어떤 갈래가
  무엇으로 가는가)이 본체와 같은지를 `match` 팔로 대조했고, 수신자 변수 이름은 비교에서 뺐다.
  그것이 보던 사본(토스트 kind → accent 색)이 **한 벌이 되면서 함께 내려갔다**(정본 열거를
  갤러리가 직접 받으므로 `match` 비-소진이 컴파일에서 잡는다, [ADR-0329](../adr/0329-a-mirror-is-removed-before-it-is-compared.md)).
  **새로 매핑 사본이 생기면 대조가 다시 필요하다** — 그때는 위 "복제를 없앨 수 없을 때" 의
  순서를 먼저 밟는다(없앨 수 있으면 없애고, 없앨 수 없을 때만 대조를 붙인다).

값도 매핑도 아닌 복제(배치 산술 등)는 이 축들에 안 걸린다. 그때는 가드를 늘리지 말고 공식을
공용 함수로 합쳐라 — 선례가 `tasty_ui_widgets::top_right_inset_square` 다.

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

- **검증을 먼저 세운다**: 갤러리는 본체 앱을 다 띄우지 않고 컴포넌트 하나만 격리 렌더한다. 디자인 정합을 빠르게 반복할 검증대를 본체 배선보다 먼저 갖는다.
- **완전성이 절차로 보장된다**: 컴포넌트가 갤러리를 거쳐야만 본체로 들어오므로, "본체엔 있는데 갤러리엔 없는" 누락이 구조적으로 안 생긴다([gallery-completeness](../design/policies/gallery-completeness.md)).
- **demo=main 이 공짜로 유지된다**: 갤러리용으로 분리한 view-only 함수를 본체가 그대로 호출하므로 거울이 자동으로 맞는다.

## 관련

- [design-change-workflow](design-change-workflow.md) — 0단계 "디자인 확보"의 전체 절차(요청문서→시안→정합 루프).
- [ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — cut 금지 + gallery-first 결정 근거.
- [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) — 갤러리 완전성 운영 상태.
- [popup-implementation](popup-implementation.md) — 본체 팝업 추가(`PopupDef`).
- [architecture/ui-widgets-crate › 무엇을 공용 위젯으로](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로) · [model-view-split](model-view-split.md) — 부품 단위 공용화 + view 분리.
- [design/systems/design-gallery-mapping §공용 crate view specimen](../design/systems/design-gallery-mapping.md) — 이미 이 경로로 올라간 항목 목록.
