# ADR-0256: 바인딩 문자열 파서는 그 문자열을 저장하는 크레이트에 둔다 — 매칭 레이어는 결과만 소비한다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: keybindings, parser, crate-boundary, feature-gate, headless, single-source
- **Group**: keybindings

## Context

`KeybindingSettings`(`tasty-settings`)가 저장하는 값은 두 종류의 문자열이다 — 콤보
(`"ctrl+shift+n"`)와 축 modifier 조합(`"ctrl+shift"`). 그 문자열을 **해석하는** 코드는
본체 `src/adapters/ui/input/shortcuts/` 에만 있었다.

- `parse_binding` / `ParsedBinding` 은 `pub(super)` 라 `shortcuts` 모듈 밖에서도 안 보였다.
- `Combo::parse_modifiers` / `all_modifier_combos` 는 `modifier_hint` 안에 있었고 그 모듈은
  `pub(crate)` 다.
- 그 위의 `src/adapters/ui` 트리 전체가 `#[cfg(feature = "gui")]` 다. 즉 headless 빌드에는
  파서가 **아예 존재하지 않는다**.

새로 필요해진 소비자는 매칭 레이어가 아니다. 단축키 이식 번들(`tasty-host-plugin` 의
`keybinding_bundle`)이 "이 바인딩이 `option` 축을 요구하는가" 를 판정해야 하는데,
그 판정은 문자열 검색으로 하면 틀린다(`"option"` 이라는 이름의 키·대소문자·토큰 순서).
실제 매칭이 쓰는 파서와 **같은 것**을 써야 한다. 그런데 그 크레이트는 본체를 볼 수 없고,
본체의 파서는 gui feature 뒤에 있어 headless 시험에서도 닿지 않는다.

## Decision

**바인딩 문자열의 해석 규칙은 그 문자열을 저장하는 크레이트가 소유한다.**
`parse_binding`·`ParsedBinding`·`Combo`(+`parse_modifiers`·`name`·`all_modifier_combos`·
`combos_containing_all`·`OPTION_AXIS`)를 `tasty_settings::keybindings::parse` 로 내리고,
`src/adapters/ui/input/shortcuts/{binding,modifier_hint}.rs` 는 그것을 **재수출해 소비**한다.
호출 사이트 이름(`parse_binding`, `Combo`, `all_modifier_combos`)은 그대로 두어 매칭 레이어
코드는 바뀌지 않는다.

경계는 "순수한가" 가 아니라 **"무엇의 규칙인가"** 로 그었다. 내려간 것은 저장된 값의 문법
전부이고, 남은 것은 그 결과를 실제 키 이벤트와 맞추는 플랫폼 규칙(winit `Key`/egui
`Modifiers` 대조, macOS `alt`→⌘ 매핑, 컨트롤 문자 역변환)이다. 뒤엣것은 winit·egui 타입을
직접 다루므로 gui feature 뒤가 제자리다.

내려가면서 `parse_binding` 의 더블탭 조기 반환(`is_double_tap_binding`)은 없앴다.
`"shift+shift"`·`"ctrl+ctrl"`·`"alt+alt"` 는 프리픽스를 떼면 모디파이어 키워드 단독만
남아 그 아래의 거부 규칙에 이미 걸린다 — 목록을 함께 내리면 같은 사실이 두 자리에 남는다.
동치는 `double_tap_spellings_are_rejected` 테스트가 고정한다.

## Consequences

- **얻은 것**: `option` 판정·번들 코덱·향후 CLI 가 매칭과 **같은 파서**를 쓴다. 파서 시험이
  headless 조합(`check-headless`)에서 돈다 — 전에는 gui feature 를 켠 빌드에서만 돌았다.
  `Combo` 의 메서드가 `pub` 이 되면서 `modifier_hint.rs` 의 blanket `#![allow(dead_code)]` 가
  덮던 범위도 그만큼 줄었다.
- **잃은 것**: `tasty-settings` 의 공개 표면이 늘었다(`keybindings` 모듈이 `pub` 이 됐고
  `KeybindingSettings` 로 가는 경로가 둘이 됐다 — 크레이트 루트 재수출과 모듈 경로).
- **운영 비용 / 유지 부담**: 새 modifier 축을 추가하면 두 자리를 함께 고쳐야 한다 —
  `parse` 의 토큰 목록과 매칭 레이어의 플랫폼 대조. 축을 늘리는 일은 OS 키보드가
  바뀌는 일이라 회차 단위로 오지 않는다.

## Alternatives Considered

- **번들 코덱을 본체 `src/` 에 둔다** — 파서가 그 자리에서 보이므로 이동이 필요 없다.
  안 고른 이유: 그 파서가 있는 트리가 `#[cfg(feature = "gui")]` 라 코덱도 함께 gui 뒤로
  들어간다. 본체는 `lib.rs` 가 없는 bin 크레이트여서 통합 테스트에서 부를 수도 없고,
  headless 조합에서는 코덱이 컴파일조차 안 된다. 티켓이 기본값으로 적어둔 선택지였으나
  전제(`gui` 게이트에 안 묶을 수 있다)가 트리에서 성립하지 않았다.
- **`option` 판정만 문자열 검색으로 한다** — 이동이 전혀 필요 없다. 안 고른 이유:
  `"option"` 이라는 이름의 키 토큰, 대소문자 변형, modifier 순서 변형에서 틀린다.
  틀리는 방향이 **거짓 음성**(죽은 바인딩을 못 찾음)이라 조용하다.
- **파서를 별도 leaf 크레이트로 뺀다** — `tasty-settings` 의 공개 표면이 안 늘어난다.
  안 고른 이유: 그 크레이트의 유일한 존재 이유가 `KeybindingSettings` 의 값 문법이고,
  소비자 전부가 이미 `tasty-settings` 를 의존한다. 크레이트 하나가 늘면 빌드 그래프에
  노드만 늘고 경계는 아무것도 안 나뉜다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crates/tasty-key-match/src/lib.rs` 또는 `modifier_hint.rs` 에 `tasty_settings`
  재수출을 거치지 않는 자체 `parse_binding`/`Combo` 정의가 다시 생긴다 — 파서가 둘로
  갈렸다는 뜻이고, 그때는 어느 쪽이 정본인지를 다시 정해야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 파싱 규칙이 플랫폼별로 갈려야 하는 요구가 생긴다(현재는 파싱이 플랫폼 무관이고
  플랫폼은 매칭에서만 갈린다). 그러면 파서가 `tasty-settings` 에 있는 것이 부담이 된다.
  재는 법: `parse` 모듈에 `#[cfg(target_os = ...)]` 가 `OPTION_AXIS` 말고 더 필요해지는지
  본다 — `OPTION_AXIS` 는 조합 **열거**용이라 파싱 자체는 아직 플랫폼 무관이다.

## References

- `docs/design/policies/key-mapping.md` — 토큰 ↔ 실제 키 표, 저장↔표시 분리
- `docs/features/keybindings/index.md` — 필드 3부류와 quick-switch 축 modifier
- 코드 근거(결정이 실현된 현재 위치): `tasty_settings::keybindings::parse` 의
  `parse_binding`·`Combo`, 그것을 재수출하는 `crates/tasty-key-match/src/lib.rs` ·
  `modifier_hint.rs`
