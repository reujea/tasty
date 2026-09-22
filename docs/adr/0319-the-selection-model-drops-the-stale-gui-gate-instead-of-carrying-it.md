# ADR-0319: 선택 모델은 낡은 gui 게이트를 크레이트 feature 로 옮기지 않고 버린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, crates, layering, selection, cell-width, headless, feature-gate, adr-0308
- **Group**: architecture

## Context

렌더러(`src/gfx/renderer.rs`)는 선택·링크를 **이미 인자로** 받는다. 남아 있던 결합은
전달 방식이 아니라 **타입의 소속**이었다 — `NormalizedSelection`·`SelectionPoint` 가
앱 상태 모듈(`state::selection`)에 살아서, 렌더러가 상태를 거꾸로 보는 모양이었다.
그 모듈은 `crate::` 를 한 줄만 쓰고(`PhysicalRect`) 나머지는 전부 워크스페이스
크레이트라, 소속만 내리면 끝나는 자리로 보였다.

**보이지 않던 것이 하나 있었다.** 그 모듈 안에 `#[cfg(feature = "gui")]` 갈래가 두 쌍
있었고, 문자 폭을 두 가지로 계산했다 — gui 면 `tasty_cell_width::unicode_width(ch)`,
아니면 `if ch.is_ascii() { 1 } else { 2 }`. 붙은 주석은 `headless fallback (no
cosmic-text)` 였다.

그 사유는 **더는 성립하지 않는다.** 폭 계산은 코드포인트 구간표뿐인 의존 0 크레이트
`tasty-cell-width` 로 이미 떨어져 나왔고(그 크레이트의 존재 이유가 "렌더러·선택 모델·
링크 스캐너가 같은 답을 써야 한다" 이다), 본 바이너리는 그것을 `Cargo.toml` 에서
**`gui` 와 무관하게 무조건** 잡는다. 즉 헤드리스 빌드에서도 정확한 폭이 손에 있는데
선택 모델만 ASCII 근사를 쓰고 있었다 — CJK·조합문자·이모지에서 갈린다.

그리고 이 잔재는 **이 이동이 조용히 깨지는 지점**이기도 하다. 크레이트에는 `gui`
feature 가 없으므로, 게이트를 그대로 옮기면 `cfg(feature = "gui")` 가 항상 거짓이 되어
**gui 빌드까지** ASCII 근사로 넘어간다. 컴파일러는 경고만 낸다(`unexpected cfg
condition value`).

## Decision

선택 모델을 `crates/tasty-selection` 으로 내리고, 옮기면서 **두 게이트를 버린다** —
폭은 언제나 `tasty_cell_width::unicode_width` 로 계산한다. 새 크레이트에 `gui`
feature 를 만들지 않는다. 본체는 `state.rs` 에서 `pub use tasty_selection as
selection;` 로 이름만 이어, `state::selection::…` 호출부 열한 자리를 건드리지 않는다.

## Consequences

- **얻은 것**: 렌더러가 앱 상태 모듈이 아니라 크레이트를 본다. 헤드리스와 gui 가
  선택 텍스트에 대해 **같은 답**을 낸다 — `tasty-cell-width` 가 애초에 존재하는 이유가
  그것이다. 그리고 게이트를 옮겼다면 gui 빌드가 조용히 틀려졌을 자리가 닫혔다.
- **잃은 것**: 헤드리스 빌드의 선택 텍스트 폭 계산이 **바뀐다**. 종전에는 비-ASCII 를
  전부 2 칸으로 셌고 이제는 구간표를 따른다. 이 값에 의존하던 헤드리스 소비자가 있다면
  그쪽이 달라진다 — 레포 안에서는 그런 소비자를 찾지 못했다(아래 재는 법).
- **운영 비용 / 유지 부담**: 크레이트 하나가 늘어 크레이트 수 lockstep 아홉 자리와
  아키텍처 문서 절 열거가 함께 움직인다.

## Alternatives Considered

- **A: 새 크레이트에 `gui` feature 를 만들어 게이트를 그대로 옮긴다** — 동작은
  보존되지만 보존되는 것이 **알려진 오답**이다. 그리고 의존 0 인 잎 크레이트에 gui 라는
  이름의 feature 가 생기는데, 그 feature 는 gui 와 아무 관계가 없다(켜면 폭 표를 쓰고
  끄면 근사한다는 뜻일 뿐이다). 이름이 뜻을 잃는 자리를 새로 만드는 값이 없다.
- **B: 게이트를 남긴 채 크레이트로 옮긴다** — 컴파일은 통과하고 경고만 난다. gui
  빌드가 조용히 근사값으로 넘어간다. 기각.
- **C: 선택 모듈을 본체에 두고 렌더러 쪽에서 타입만 재정의한다** — 같은 타입이 두 벌이
  되어 렌더러와 view 가 서로 변환해야 한다. 티켓이 경고한 "새 전달 방식 도입" 으로
  번지는 경로이기도 하다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `tasty-cell-width` 가 의존을 갖게 되거나 본 바이너리에서 `gui` 뒤로 들어가면, 헤드리스에
  폭 표가 없다는 전제가 되살아난다. 그때는 이 결정을 다시 본다.
- 같은 형태의 게이트가 다른 자리에 남아 있다 — `src/view/main/vi_copy.rs` 의 폭 계산이
  같은 쌍을 쓴다. 그쪽은 gui 전용 view 라 이 이동으로 깨지지 않아 손대지 않았다.
  그 파일이 헤드리스 경로로 들어오면 함께 봐야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 헤드리스 선택 텍스트의 폭이 달라진 것이 실제 소비자에게 보이는가. 재는 법:
  `cargo test --workspace --no-default-features --locked --no-fail-fast` 의
  `test result:` 줄을 보고, 비-ASCII 를 담은 선택 텍스트를 단언하는 항목이 있는지
  `extract_selected_text` 호출부를 헤드리스 조합에서 세어 확인한다. 컴파일이 통과하는
  것은 이 물음에 답하지 않는다.

## References

- 적용한 layer 예외의 선례: [ADR-0308](0308-the-format-registry-port-impl-stays-with-the-type.md)
- 크레이트 분할이 의존 방향을 따른다는 결정: [ADR-0089](0089-crate-split-follows-dependency-direction.md)
- 코드 근거(결정이 실현된 **현재 위치**): `tasty-selection` 크레이트의
  `extract_selected_text` · `pixel_to_grid` · `is_selected`, 본체 쪽 이름 잇기는
  `src/state.rs` 의 `selection` 재수출
- 아키텍처 절 소속과 크레이트 수: [architecture](../architecture/index.md)
