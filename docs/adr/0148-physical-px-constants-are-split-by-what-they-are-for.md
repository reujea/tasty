# ADR-0148: 물리 px 상수는 "무엇을 위한 값인가" 로 갈라 다룬다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: dpi, typed-length, layout, design-tokens, hidpi, adr-0145
- **Group**: typed-length-dpi

## Context

레포에는 길이를 **물리(device) 픽셀**로 고정한 상수족이 있다.

```
crates/tasty-model/src/lib.rs   PANE_BORDER_WIDTH:    PhysicalPx = PhysicalPx(2.0)
crates/tasty-model/src/lib.rs   SURFACE_BORDER_WIDTH: PhysicalPx = PhysicalPx(1.0)
src/state/mouse.rs              DIVIDER_HIT_THRESHOLD   (이전: 타입 없는 f32 = 4.0)
```

주석은 "Gap in physical pixels" 라고 적혀 있다. **그 주석만으로는 이 값들이 물리인 것이
설계인지 잊힌 것인지 갈리지 않는다.** 두 경우 모두 같은 주석이 붙는다.

### DPI 배율 2 실측이 그것을 갈랐다

배율 1 과 2 에서 pane 을 분할해 화면을 재니, **붙어 있는 두 선 중 하나만 배율을 탔다**.

| 요소 | 배율 1 | 배율 2 | 논리 크기 |
|---|---|---|---|
| 분할 보더 (`PANE_BORDER_WIDTH`) | 2 물리px | **2 물리px** | 2 → **1** |
| 바로 옆 포커스 강조선 (egui 가 논리로 그림) | 2 물리px | **4 물리px** | 2 → 2 |

배율 1 에서는 둘 다 2px 라 **구분되지 않는다.** 배율 2 에서만 갈라진다. 관측이 있어야
"설계된 물리" 와 "잊힌 물리" 가 갈린다는 것을 이 표가 보여준다 — 단일 값은 그럴듯한
수라 통과하지만, 짝지어 놓으면 갈린다.

### 같은 수를 두 문서가 다르게 읽고 있다

[`work-area.md`](../features/work-area/screens/work-area.md) 는 이 값들을 디자인
시스템의 치수와 **일치한다**고 적는다("보더 폭은 코드 상수(`PANE_BORDER_WIDTH`=2px,
`SURFACE_BORDER_WIDTH`=1px)와 일치"). 디자인 시스템의 px 는 **논리**다. 코드는
**물리**로 구현했다. 두 해석은 **배율 1 에서만 같은 값을 낸다.**

이것은 두 자리가 서로 어긋난 것이 아니라 **같은 수를 두 자리가 다르게 읽는** 형태라,
어느 쪽을 고쳐도 다른 쪽이 조용히 틀린 채 남는다.

### 이미 갈라진 하나 — 조작 표적

`DIVIDER_HIT_THRESHOLD` 는 보더가 아니라 **드래그 히트 밴드**다. 물리로 고정하면
고배율일수록 표적의 실제 크기가 작아져 **집기 어려워진다**(배율 2 에서 절반). 이는
미관이 아니라 조작성 회귀이고, 값 자체가 타입 없는 `f32` 라
[`typed-length`](../concepts/typed-length.md) 정책에도 걸렸다. 이 ADR 과 같은 회차에서
논리(`LogicalPx(4.0)`)로 옮기고 비교 직전에만 `to_physical` 하도록 고쳤다.

## Decision

**물리 px 상수를 한 덩어리로 다루지 않는다. "무엇을 위한 값인가" 로 갈라 판정한다.**

- **조작 표적(히트 밴드·드래그 영역)은 논리다.** 사람 손가락·커서가 겨누는 크기라
  실제 크기가 배율에 따라 변하면 안 된다. — `DIVIDER_HIT_THRESHOLD` (적용 완료)
- **디자인 시스템이 치수를 정하는 시각 요소는 논리다.** 디자인의 px 가 논리이므로
  코드도 논리여야 두 해석이 모든 배율에서 일치한다. — `PANE_BORDER_WIDTH` 를
  `LogicalPx(2.0)` 로 옮긴다.
- **1 물리 px 는 hairline 으로 명시하고 물리를 유지한다.** 논리로 옮기면 배율 2 에서
  2 물리 px 가 되어 굵어진다. 고해상도에서 실선을 최소 굵기로 유지하는 것은 의도된
  기법이고, 이 값은 옆에 나란히 놓이는 논리 상대가 없어 불일치가 관측되지 않는다.
  — `SURFACE_BORDER_WIDTH` 는 물리 1px 로 남기되, 그것이 **hairline 정책**이라는 것을
  상수 doc 에 적는다(지금은 "physical pixels" 라고만 적혀 있어 의도가 안 읽힌다).

세 갈래 모두 적용됐다. `BinaryTree::BORDER_WIDTH` 는 연관 상수라 배율을 못 받으므로
`border_width(scale_factor)` 메서드로 바뀌었다 — pane 보더는 논리라 배율을 받아야 물리가
나오고 surface 보더는 hairline 이라 배율을 **무시하는 것이 정답**이라, 두 경우를 한
상수로 표현할 수 없다. 그 인자는 레이아웃 계산 경로를 따라 흐른다(배율을 캐시하지 않는
것이 [`typed-length`](../concepts/typed-length.md) 의 "변환에 scale factor 가 강제 인자"
와 같은 방향이다).

## Consequences

- **얻은 것**: 물리로 남길 값과 논리로 옮길 값의 판정 기준이 생겼다. 새 상수를 더할 때
  "물리인가 논리인가" 를 취향이 아니라 **용도**로 답한다. `SURFACE_BORDER_WIDTH` 의
  물리성이 잊힌 것이 아니라 결정임이 기록됐다.
- **얻은 것**: 배율 1 에서만 일치하던 코드·디자인 두 해석이 `PANE_BORDER_WIDTH` 이관
  후 모든 배율에서 일치한다.
- **잃은 것**: 상수족이 더 이상 한 규칙을 따르지 않는다. `PANE_BORDER_WIDTH` 는 논리고
  `SURFACE_BORDER_WIDTH` 는 물리라, 둘을 나란히 보는 사람은 이 ADR 을 읽어야 이유를
  안다. 그래서 두 상수 doc 에 각각 근거를 적는 것이 결정의 일부다.
- **운영 비용 / 유지 부담**: 비용은 상수가 몇 번 쓰였나가 아니라 **배율이 흘러야 하는
  경로의 길이**였다. 상수 이름의 등장은 여덟 자리인데, 논리로 바꾸는 순간 그것을 쓰는
  트리 순회 메서드 넷(`compute_rects` · `collect_dividers` · `find_divider_at` ·
  `update_ratio_for_rect`)과 그 호출 경로 전체가 `scale_factor` 를 받아야 해서 실제로는
  26 개 파일이 바뀌었다. 이 상수족을 더 옮길 때 같은 형태의 과소평가를 하지 않도록
  두 수를 함께 적어 둔다.
- **일반 검사는 없고, 이 결정만 지키는 검사는 있다**: "논리여야 할 값이 물리로 적혔다"
  를 소스 패턴으로 잡는 검사는 만들지 않는다 — 그 판정의 근거가 *수가 겹치는가* 가
  아니라 *용도가 무엇인가* 라 구문에 안 드러난다. 다만 **이 두 상수의 갈림**은
  `crates/tasty-model` 의 단위 테스트가 배율 2 에서 고정한다(pane 4 물리px ·
  surface 1 물리px). 배율 1 에서는 두 좌표계가 같은 관측(2·1)을 내므로 그 테스트는
  **반드시 배율 2 를 함께 단언해야** 의미가 있다. 화면 쪽 관측 절차는
  [`dpi-scale-verification`](../ai-verification/dpi-scale-verification.md) 에 있다.

## Alternatives Considered

- **A: 상수족 전부를 논리로 옮긴다** — 규칙이 하나가 되어 단순하다. 안 고른 이유는
  `SURFACE_BORDER_WIDTH` 가 배율 2 에서 2 물리 px 로 굵어지기 때문이다. 1 물리 px
  실선을 유지할 방법이 사라지고, 그 손실이 규칙 단순화의 이득보다 크다. 그리고 이
  선택은 "물리 1px 은 hairline 이다" 라는 판단을 **하지 않고 뭉개는** 것이다.
- **B: 물리를 유지하고 디자인 문서를 고친다** — 코드를 안 건드리므로 가장 싸고,
  "우리 보더는 device px 단위다" 라는 일관된 입장도 가능하다. 안 고른 이유는 두 가지다.
  ① 실측에서 `PANE_BORDER_WIDTH` 는 **논리로 그려지는 포커스 강조선과 화면에서
  맞닿아 있고**, 배율 2 에서 굵기가 갈린다 — 문서를 고쳐도 그 시각적 불일치는 남는다.
  ② 디자인 시스템 전체가 논리 px 로 돼 있어, 이 두 값만 물리로 두면 디자인 산출물을
  코드로 옮길 때마다 예외를 기억해야 한다.
- **C: 상수마다 개별 판단, 기준 없이** — 지금 상태다. 안 고른 이유는 이번 축이 보여준
  것이 정확히 그 비용이기 때문이다: 기준이 없으니 `DIVIDER_HIT_THRESHOLD` 는 타입도
  없이 `f32` 로 남았고, 네 번째 리터럴 사본이 webview 인셋에 복사돼 있었다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 디자인 시스템이 hairline(1 device px)을 **명시**한다 — 그러면 `SURFACE_BORDER_WIDTH`
  의 물리성이 이 ADR 의 추론이 아니라 디자인의 요구가 되고, 서술 근거가 바뀐다.
- 배율 3 이상(또는 비정수 배율 1.5·2.5)에서 두 상수가 다르게 보인다 — 이 결정은 배율
  1·2 실측만을 근거로 한다.
- 갤러리가 두 배율을 함께 렌더하게 된다 — 그러면 불일치가 실측 절차가 아니라 상시
  관측면에서 드러나고, "자동 채널 없음" 이 더 이상 참이 아니다.
- `PANE_BORDER_WIDTH` 이관이 렌더 좌표에서 1px 어긋남을 만든다 — 이관 자체가 비용을
  넘어서면 B 를 다시 본다.

## References

- [`docs/concepts/typed-length.md`](../concepts/typed-length.md) — 이 축이 지키려는 정책
- [`docs/ai-verification/dpi-scale-verification.md`](../ai-verification/dpi-scale-verification.md) — 배율 2 환경 재현 절차(위 실측이 따른 절차)
- [`docs/features/work-area/screens/work-area.md`](../features/work-area/screens/work-area.md) — 같은 수를 논리 px 로 읽는 문서
- `crates/tasty-model/src/lib.rs` — `PANE_BORDER_WIDTH` · `SURFACE_BORDER_WIDTH`
- `src/state/mouse.rs` — `DIVIDER_HIT_THRESHOLD` (본 결정의 첫 적용)
- [ADR-0145](0145-typed-length-constructors-stay-open-for-now.md) — 길이 newtype 의 생성자를
  열어 둔다는 결정. 이 ADR 의 처방은 상수를 `LogicalPx(2.0)` 처럼 **생성자 직접 호출**로
  옮기는 것이라, 그 생성자가 열려 있다는 전제 위에 선다. 0145 가 봉인 쪽으로 뒤집히면
  여기 적은 이관 형태도 `from_raw()` 류로 다시 써야 한다.
