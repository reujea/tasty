# ADR-0556: 생성 길이 상수 가드의 처방은 그 토큰 자신의 `&Theme` 경로다 — 값이 같은 이름이 아니다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: design-tokens, guards, theme, zoom, prescription, file-picker, adr-0135

## Context

`src/design_token_guard.rs` 의 `ui_does_not_consume_generated_length_consts_directly` 는 UI 계층이
`tasty_design_tokens::generated` 의 `LogicalPx` 상수를 직접 읽는 것을 막는다(생성 상수는 `zoomed()`
밖이라 `ui_scale` 을 안 탄다). 그 실패문은 "같은 값의 `Theme` 필드/접근자를 경유해라" 였다.

그 문구는 두 방향으로 틀렸다.

- **경로가 있는 토큰에서 경로를 가렸다.** component tier 의 `LogicalPx` 상수는 전부 제 이름의
  `&Theme` 접근자를 갖는다(2026-09-23 실측 253 중 253 — 248 은 `generated_component.rs` 생성물,
  5 는 생성기가 이름 충돌로 건너뛰고 `theme.rs` 에 수기로 둔 `modhint_*`). 그런데 문구가 "값이 같은
  것" 을 찾으라고 해서, 파일 피커 path bar 를 쓴 작업은 component tier 에 `Theme` 경로가 없다고
  판단했고 토큰 읽기를 가드 스캔 루트 밖 모듈(`picker_caps`)로 옮겨 배율을 손으로 걸었다. 그 모듈의
  식은 생성 접근자의 식과 같았다 — 처방이 가리키지 못한 경로가 그대로 거기 있었다.
- **경로가 없는 토큰에서 틀린 결합을 처방했다.** semantic tier 에는 `Theme` 경로가 없는 길이 토큰이
  있다(같은 날 실측 4). 그 자리에 "같은 값의 이름" 을 따르면 픽셀은 같고 다른 토큰에 묶인다 — 그
  토큰이 움직이면 무관한 자리가 따라 움직인다. `source_guards::on_scale_length_literal` 이 값 일치
  후보 13 을 손으로 읽어 10 이 그런 자리였다고 이미 적어 둔 형태다.

## Decision

실패문은 **그 토큰 자신의 `&Theme` 경로를 이름으로 댄다.** 가드가 경로를 소스에서 찾는다 — 먼저
생성기의 `SEMANTIC_DIM_TO_THEME_FIELD`(먼저 나오는 항목이 이김, 필드가 `Theme` 에 실재해야 함), 없으면
component 토큰 이름의 접근자(`generated_component.rs` 또는 `theme.rs`). 둘 다 없는 토큰은 "경로가
없다" 고 말하고 경로를 만드는 절차를 댄다. 값이 같은 다른 이름은 어느 갈래에서도 처방하지 않는다.

경로가 없는 토큰은 사유와 함께 `PATHLESS_LENGTH_TOKENS` 명부에 오르고, 가드가 그 명부를 실측과
**집합 동등**으로 대조한다. 같은 대조가 "component tier 는 전부 경로가 있다" 를 단정한다.

`picker_caps` 모듈은 지운다. path bar 가 `th.fp_*()` 접근자를 읽는다 — 식이 같아 픽셀은 안 바뀐다.

## Consequences

- **얻은 것**: 실패문을 그대로 따르면 통과한다(2026-09-23 에 path bar 에 생성 상수 읽기를 되돌려
  넣어 가드가 네 자리에 `th.fp_*()` 를 처방하는 것을 보고, 그대로 고쳐 통과를 확인했다). 스캔 루트
  밖으로 옮겨 가드를 피하는 우회의 동기가 사라진다. 경로 없는 토큰이 이름으로 보인다.
- **잃은 것**: 명부 하나가 늘었다 — 경로가 생기거나 새 토큰이 들어오면 사람이 한 줄을 고쳐야 한다.
- **운영 비용 / 유지 부담**: 가드가 생성 파일의 doc 주석(`/// \`component.<이름>\``)과 상수의 짝을
  읽는다. 생성기가 그 주석 형태를 바꾸면 표의 줄 수 대조가 먼저 빨개진다.
- **표 해석의 두 규칙은 따로 잰다.** "먼저 나오는 항목이 이긴다" 는 필드가 둘인 토큰
  (`semantic.control-height-tab` → `item_height_tab` · 배율을 안 타는 `tab_bar_height`)의 양성 대조가
  고정한다. "필드가 `Theme` 에 실재해야 한다" 는 오늘 실제 표로는 아무것도 안 거른다(표가 가리키는 필드가
  전부 `Theme` 에 있다) — 그래서 `ThemeSizing` 에만 있는 필드를 가리키는 가짜 표 줄로
  `a_table_field_missing_from_theme_is_not_a_path` 가 잰다.

## Alternatives Considered

- **문구만 "같은 토큰의 접근자" 로 바꾸기** — 이름을 안 대면 읽는 사람이 다시 값으로 찾는다. 이번
  결함이 정확히 그 경로로 났다.
- **경로 없는 토큰을 가드 모수에서 빼기** — 그 토큰을 UI 가 쓰는 자리가 조용히 통과한다. 모수는
  그대로 두고 처방만 "경로를 먼저 만든다" 로 가른다.
- **경로 없는 토큰 넷에 지금 `Theme` 필드를 만들기** — 소비처가 없다. 필드는 쓰는 자리가 생길 때
  만든다(값은 `sizing_parity` 가 토큰에 묶으므로 그때도 디자인 값 결정이 아니다).
- **`picker_caps` 를 두고 `&Theme` 을 받게 고치기** — 그 모듈이 뷰 밖에 있던 이유(가드가 막는 형태를
  뷰에서 떼기)가 사라지므로 남길 이유가 없다. 남기면 가드 스캔 루트 밖에 토큰 소비 자리가 계속 산다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 생성기가 component 길이 토큰의 접근자를 건너뛴다 — `every_generated_length_token_has_a_theme_path_or_is_listed`
  가 component 쪽 고아를 이름으로 뱉는다.
- `PATHLESS_LENGTH_TOKENS` 가 비거나 새 줄이 는다 — 같은 테스트가 집합 동등으로 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 가드 스캔 루트(`SCAN_ROOTS`) 밖에서 생성 길이 상수를 읽는 자리가 다시 생긴다. 이 가드는 루트
  밖을 원리적으로 못 본다. 재는 법: `git grep -n 'tasty_design_tokens::generated::' -- src crates ':!crates/tasty-design-tokens' ':!crates/tasty-gallery'`
  의 결과 중 무차원 상수가 아닌 줄.

## References

- [`docs/design/systems/theme.md`](../design/systems/theme.md) — "생성 토큰 상수 직접 소비" 행 · "Component tier 접근자" 절
- `src/design_token_guard.rs` — `theme_path_table` · `PATHLESS_LENGTH_TOKENS` · `length_const_prescription`
- 선행 결정: [ADR-0135](0135-ui-length-literals-do-not-follow-ui-scale-in-the-app.md) (배율 축 — 이 가드가 지키는 사실의 근거, 결정은 다른 조항)
- 선행 결정 없음(처방 조항) — 탐색: `git grep -l 'ui_does_not_consume_generated_length_consts_directly\|picker_caps\|SEMANTIC_DIM_TO_THEME_FIELD\|design_token_guard' -- docs/adr/`
