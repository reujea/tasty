# ADR-0329: 갤러리 미러는 대조하기 전에 없앨 수 있는지 먼저 본다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: gallery, widgets, duplication, guards, toast, type-appearance, compiler-enforced

## Context

갤러리는 본체 컴포넌트를 전부 전시해야 하고(ADR-0510), specimen 이 본체와 **같은 것을**
보여야 한다. 그런데 정본 타입이 갤러리에 넣기 무거운 크레이트에 있으면 같은 함수를 부를
수 없어 갤러리가 같은 모양을 손으로 다시 만든다.

토스트가 그 자리였다. `ToastKind` 의 정본이 `tasty-model` 에 있었고 그 크레이트는
`termwiz` 를 끌고 오므로, 갤러리는 같은 변종을 가진 enum 을 하나 더 두고 accent 매핑과
카드 chrome 까지 되풀이했다. 그 사본이 갈라진 적이 실제로 있다 — 갤러리에만 있던
`Agent` 변종이 본체가 만들 수 없는 토스트를 카탈로그에 전시했다.

처방은 **사본을 두되 갈라지면 시끄럽게 하는 것**이었고, 가드가 둘 붙었다. 하나는 두
`ALL` 배열을 런타임에 열거해 변종 집합을 양방향 대조했고, 하나는 두 `accent_color` 의
`match` 팔을 (갈래, 부르는 이름) 짝으로 텍스트 대조했다.

그 처방의 전제는 **"정본을 import 할 수 없다"** 하나였다. 그리고 그 전제는 정본이
움직이면 같이 무효가 되는데, `ToastKind` 는 색을 고르는 값이라는 이유로
`tasty-type-appearance` 로 옮겨 갔다 — **갤러리가 이미 의존하고 터미널 모델을 안 끌고
오는 크레이트다.** 전제가 사라진 뒤에도 사본과 가드 둘은 그대로 남아 있었고, 두 가드의
doc 은 없어진 이유를 현재형으로 계속 말했다.

## Decision

**미러를 만들거나 유지하기 전에 그것을 없앨 수 있는지 먼저 잰다.** 없앨 수 있으면
없애고, 없앨 수 없을 때만 기계 대조를 붙인다. 그리고 미러의 존재 근거는 **정본의 위치에
딸린 값**이므로, 정본이 옮겨 가면 그 근거를 다시 잰다.

토스트에 적용한 결과: 갤러리가 정본 `ToastKind` 를 직접 쓰고, 그리기(카드 chrome · 스택
배치 · accent 매핑 · 페이드 곡선)는 본체와 갤러리가 함께 의존하는 `tasty-ui-widgets` 로
옮겨 **한 벌**이 됐다. 사본을 보던 가드 둘은 볼 대상이 없어져 함께 내렸다.

**사본이 한 벌이 된 자리에서 대조를 빼는 것은 이행이고, 사본이 남아 있는데 빼는 것은
보는 눈만 없애는 것이다.** 두 경우를 가르는 것은 "정본을 직접 부르는가" 하나다.

## Consequences

- **얻은 것**: 판정이 런타임에서 컴파일로 올라갔다. 갤러리가 정본 열거를 직접 받으므로
  변종을 더하면 `match` 가 비-소진이 되어 **빌드가 깨진다**(실측 `E0004: non-exhaustive
  patterns`). 종전 채널은 그 시험이 **실행돼야** 봤다 — 그래서 그 가드 자신이 "통합
  테스트로 내리지 마라, 실행 채널을 잃는다" 를 doc 에 적어야 했다. 컴파일 판정에는 그
  걱정이 없다.
- **얻은 것**: 그리기 본문이 하나라 카드 모양이 갈릴 자리가 없어졌다. 실제로 갈려 있던
  것 둘을 이 과정에서 찾았다 — specimen 이 보더를 `border-default` 로 칠했고(본체는
  `component.toast-border` = `border-strong`), 본문 글자를 카드와 함께 페이드했다(본체는
  안 한다).
- **잃은 것**: 시험 8 개(본체 6 · 갤러리 2). 전부 내린 두 가드의 것이고 대체 채널이
  더 세다.
- **잃은 것**: 갤러리 산출물이 `tasty-type-appearance` 를 `dev-dependencies` 가 아니라
  일반 의존으로 든다. 이미 들고 있던 크레이트라 의존 집합은 안 늘었다.
- **운영 비용**: `tasty-ui-widgets` 는 번들 plugin 셋(`clipboard-viewer` · `git-viewer` ·
  `markdown`)의 의존 폐포 안이다. **그 크레이트를 고치는 모든 변경에 그 셋의 버전 bump 가
  붙는다.** 나머지 여섯은 폐포 밖이라 올리지 않는다 — 안 바뀐 산출물에 새 버전 문자열을
  발행하면 안 된다.

## Alternatives Considered

- **A: 사본을 두고 가드를 고친다** — 두 가드의 doc 만 "정본이 `tasty-type-appearance` 로
  옮겨 갔다" 로 갱신하고 사본은 유지. 안 고른 이유: 가드가 지키던 위험(사본이 갈린다)이
  사본을 없애면 **존재하지 않게** 된다. 위험을 남기고 감시를 유지하는 것보다 위험을
  없애는 쪽이 싸다. 그리고 그 가드 중 하나가 자기 실패문에 "빼는 것이 이행인 경우는
  하나뿐이다: 갤러리가 정본을 직접 부르게 되어 매핑이 한 벌이 됐을 때" 라고 적어 두었다.
- **B: 그리기는 그대로 두고 열거만 공유한다** — `ToastKind` 만 정본으로 바꾸고 카드
  chrome 사본은 유지. 안 고른 이유: 갈라져 있던 둘(보더 토큰 · 텍스트 alpha)이 전부
  **열거가 아니라 그리기** 쪽이었다. 열거만 합치면 실제로 갈린 축은 그대로 남는다.
- **C: 갤러리를 본체 binary 에 의존시킨다** — 그러면 이동 없이 같은 함수를 부를 수 있다.
  안 고른 이유: 방향이 거꾸로다. 갤러리는 본체를 모르는 카탈로그여야 하고, 본체 binary 는
  애초에 라이브러리로 링크할 대상이 아니다.
- **D: 그리기를 새 크레이트로 뺀다** — `tasty-toast-view` 같은 것. 안 고른 이유: 새
  크레이트는 lockstep 4 자리(아키텍처 문서 · README 배지 둘 · 크레이트 목록 가드)를
  끌고 오는데, 목적지로 쓸 수 있는 크레이트가 이미 있고 양쪽이 이미 의존한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 갤러리가 `tasty-ui-widgets` 의 토스트 함수를 안 부르고 자기 그리기를 다시 갖게 되는
  것. 그때는 "한 벌" 전제가 깨지므로 대조를 되살려야 한다.
- 번들 plugin 중 `tasty-ui-widgets` 를 폐포에 든 것의 수가 3 이 아니게 되는 것. 위
  운영 비용 문단의 수가 그 값이다. 재는 법은 `cargo tree -p <plugin> -e normal` 이고
  `scripts/check-plugin-version-bump.sh` 가 같은 답을 "판정 대상 N 건" 으로 낸다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **specimen 과 본체가 같은 픽셀을 그리는가.** 함수가 하나라는 것은 *부르는 쪽이 같은
  입력을 준다*를 보장하지 않는다 — 무대 rect·테마·배율이 다르면 그림도 다르다. 재는 법:
  갤러리 specimen 캡처와 본체 토스트 캡처를 같은 테마·같은 배율로 떠서 겹친다
  (`docs/ai-verification/visual-verification.md` 의 절차, Xvfb 넓은 화면 + GPU
  `screenshot --window`). 자동 채널은 없다.
- **토스트 본문 글자가 페이드해야 하는가.** 지금은 카드 chrome 만 페이드한다 — `Fonts::layout`
  이 색을 galley 에 박아 `Painter::galley` 의 fallback 이 죽기 때문이고, 의도한 설계가
  아니라 구현의 결과다. 재는 법: 디자인이 정한다. 값이 정해지면 `draw_toast_scopes` 한
  자리만 고치면 양쪽이 함께 따라온다 — 그것이 이 결정으로 생긴 여유다.

## References

- [gallery-completeness](../design/policies/gallery-completeness.md) — 미러 정책 본문
- [toast](../design/systems/toast.md) — 그리기/상태 경계의 현재 상태
- [design-gallery-mapping](../design/systems/design-gallery-mapping.md) — jsx ↔ 함수 매핑
- [ADR-0510](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — 갤러리 완전성
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ui-widgets/src/toast.rs` 의
  `draw_toast_scopes` · `toast_accent_color` · `fade_alpha`
