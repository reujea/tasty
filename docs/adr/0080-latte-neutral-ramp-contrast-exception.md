# ADR-0080: latte 중성 램프의 AA 미달을 알려진 예외로 수용한다

- **Status**: Accepted
- **Date**: 2026-08-24
- **Tags**: theme, accessibility, contrast, latte, palette
- **Group**: ui-theme-gallery

## Context

[테마 시스템](../design/systems/theme.md) 의 "텍스트 대비" 규칙은 최소 4.5:1(WCAG
AA)이다. latte 의 `subtext0`(text-muted)은 upstream catppuccin 값 `#6c6f85` 로는
`base` 위에서도 4.37:1 이라 이미 미달이었고, `#63667c` 로 내려 `base`(4.99)·
`mantle`(4.64)·`#ffffff`(5.64)를 통과시켰다.

남은 미달 조합이 두 배경단에 있다 — `crust`(4.26) 와 `surface0`(3.65). 이 둘은
장식용 배경이 아니라 **상시 노출 화면의 배경**이다.

- 상태바는 `bg_app`(=crust) 배경에 `text_muted` 캡션을 그린다(4.26:1).
- 탭바는 포커스된 pane 의 탭 스트립이 `surface_raised`(=surface0)이고 비활성 탭
  제목이 `text_muted` 다(3.65:1).

"한 단 위 `subtext1` 도 미달이라 램프 전반의 문제" 라는 기존 근거는 `surface0`
에만 맞다 — `subtext1` 은 `crust` 위에서 4.73:1 로 **통과**한다. 즉 crust 미달은
`subtext0` 단독의 문제이므로 별도 판단이 필요하다.

## Decision

**팔레트를 더 손대지 않고, latte 중성 램프의 잔여 AA 미달을 알려진 예외로
수용한다.** 근거 수치(전 조합 대비 표)와 미달을 그리는 화면, 그리고 왜 팔레트로
못 고치는지를 [테마 시스템](../design/systems/theme.md) "latte 중성 램프 대비 —
알려진 예외" 에 못 박아, 다음 사람이 같은 계산을 반복하거나 같은 막다른 길을 다시
걷지 않게 한다.

팔레트 경로가 막힌 이유는 두 가지이고 둘 다 산술적으로 확정된다.

1. `subtext0` 을 crust 통과선(`#5f6279`, 4.52:1)까지 더 내리면 `subtext1`
   (`#5c5f77`)과의 차가 `(3,3,2)` 로 줄어 text-muted 와 text-secondary 가 사실상
   같은 색이 된다. 3단 텍스트 위계가 latte 에서만 2단으로 붕괴하고, 그러고도
   `surface0` 는 3.88 로 여전히 미달이다.
2. `surface0` 를 통과시키려면 `subtext0` 이 `#555870` 근처여야 하는데 이는
   `subtext1` 보다 **어둡다** — 램프 순서가 뒤집힌다.

`surface0` 위에서 AA 를 넘는 중성 전경은 `text` 하나뿐이고 `surface1`/`surface2`
는 `text` 조차 미달이다. 이는 catppuccin latte 의 raised/hover 배경단이 라이트
테마치고 어둡다는 팔레트 자체의 성질이라, 제대로 고치려면 중성 램프 전체를 다시
뜨는 디자인 결정이 필요하다.

## Consequences

- **얻은 것**: 3단 텍스트 위계와 vendored 팔레트 정체성이 그대로 유지된다. 미달이
  "모르고 지나간 것" 이 아니라 수치·화면·차단 사유가 적힌 **기록된 예외**가 되어,
  새 UI 를 그릴 때 배경 선택 기준(muted 캡션은 `base`/`mantle`/`#ffffff` 한정)으로
  쓸 수 있다.
- **잃은 것**: latte 에서 상태바 캡션과 비활성 탭 제목이 AA 미달로 남는다. 저시력
  사용자에게는 mocha 대비 열위다(mocha 는 같은 조합이 7.37:1 / 5.65:1 로 통과).
- **운영 비용 / 유지 부담**: 없음에 가깝다 — 대비 표가 문서에 있어 재계산이 필요
  없다. 다만 새 화면이 `surface0` 이상 어두운 배경에 muted 를 얹으면 미달 화면이
  조용히 늘어난다. 표를 규칙으로 쓰는 것이 유일한 방어선이다(자동 가드 없음).

## Alternatives Considered

- **A: `subtext0` 을 `#5f6279` 로 더 내린다** — crust 는 4.52 로 통과하지만
  `surface0` 는 3.88 로 여전히 미달이고, text-muted 와 text-secondary 가 구분되지
  않는다. 접근성 한 칸을 위해 위계 한 단을 버리는 거래라 순손실이다.
- **B: 상태바/탭바만 `text_secondary` 로 승격한다** — crust(4.73)는 통과하지만
  `surface0`(4.05)는 여전히 미달이라 절반만 고쳐진다. 게다가 두 화면 모두 확정
  시안이 muted 를 지정한 것이라, 디자인 요청 없이 바꿀 대상이 아니다.
- **C: `surface0`/`crust` 배경을 밝힌다** — 대비는 풀리지만 raised/hover 단 전체가
  이동해 모든 화면의 elevation 대비가 바뀐다. 팔레트 재설계와 같은 크기의 작업이라
  비차단 폴리시 트랙에서 할 일이 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- latte 중성 램프를 다시 뜨는 디자인 작업이 발생하면 — 그때 `surface0`/`surface1`/
  `surface2` 를 밝혀 예외 자체를 없앨 수 있다.
- 상태바/탭바 시안이 muted 가 아닌 전경으로 갱신되면 — Alternative B 가 디자인
  승인된 경로가 되므로 그대로 반영한다.
- 접근성 요구가 AA 준수 강제로 올라가면(예: 특정 배포처 요건) — 예외 수용 자체가
  선택지에서 빠진다.

## References

- [테마 시스템](../design/systems/theme.md) "latte 중성 램프 대비 — 알려진 예외" — 대비 표와 배경 선택 기준(현재 운영 상태)
- [visual-verification](../ai-verification/visual-verification.md) — 대비 위반 판정 체크리스트
