# ADR-0326: egui 는 링크하는 크레이트가 켜는 것이지 기본값으로 따라오는 것이 아니다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: build, cargo, features, headless, type-appearance, dependency-graph
- **Group**: architecture

## Context

`--no-default-features`(headless) 로 빌드해도 `egui`·`ecolor`·`emath`·`epaint`·`egui_extras`
가 의존 그래프에 그대로 있었다. **컴파일은 통과한다** — 그래서 이 사실은 어느 게이트에도
안 잡혔다. "컴파일된다" 와 "그래프에 안 들어온다" 는 다른 좌변이고, 레포에 뒤엣것을 묻는
자리가 없었다.

들어오는 문은 셋이었고 **전부 루트의 비-optional path 의존**이었다.

- `tasty-type-appearance` 의 `default = ["egui-compat"]`. 이 크레이트는 type 레이어라
  소비자가 열둘이고, 그중 egui 를 안 링크하는 것(`tasty-ipc`·`tasty-model`·
  `tasty-plugin-manifest`·`tasty-host-plugin`)까지 기본값으로 egui 를 들였다.
  **끄는 기구는 있었다** — 그 파일의 주석이 `default-features = false` 를 쓰라고 적어
  뒀다. 실제로 끈 소비자는 넷뿐이었고, 기본값이 켜짐이라 새 소비자는 자동으로 켜졌다.
- 루트의 `tasty-icons = { …, features = ["egui"] }` — 그 크레이트의 `default` 는 이미
  비어 있는데 루트가 조건 없이 켜고 있었다.
- 루트의 `tasty-egui-theme`·`tasty-ui-widgets` — 크레이트 자체가 egui 전용인데 비-optional.

그리고 이 셋을 **헤드리스에서 컴파일되는 `src/` 파일이 하나도 안 쓴다**(실측 2026-09-20:
컴파일되는 263 개 중 `tasty-ui-widgets` 0 · `tasty-egui-theme` 0 · `tasty-icons` 0).
그 263 의 모수는 `cargo build -p tasty --no-default-features` 뒤 `target/debug/libtasty.d`
가 드는 `src/*.rs` 다 — 정의는 [ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md)
에 있다. 참고로 게이트 뒤를 포함한 `src/` 전체에서는 그 셋을 참조하는 파일이 **84** 개라,
"헤드리스에서는 0" 은 84 대 0 으로 갈린 사실이다. 즉 코드가 막고 있던 것이 아니라
매니페스트 다섯 줄이 막고 있었다.

## Decision

**egui 를 링크하는 크레이트가 명시적으로 켠다.** 세 가지를 함께 바꾼다.

1. `tasty-type-appearance` 의 `default` 를 **비운다.** `egui-compat` 은 남고, egui 를 실제로
   링크하는 소비자(`tasty-egui-theme` · `tasty-ui-widgets` · `tasty-gallery` · 번들 plugin
   넷)가 `features = ["egui-compat"]` 로 켠다. 루트는 `gui` feature 가 켠다.
2. 루트의 `tasty-icons` 에서 `features = ["egui"]` 를 떼고 `gui` 로 옮긴다.
3. 루트의 `tasty-egui-theme`·`tasty-ui-widgets` 를 `optional = true` 로 바꾸고 `gui` 뒤에
   둔다(`tasty-key-match` 가 이미 같은 형태다).

**`src/` 코드는 한 줄도 안 고친다.**

**기본값을 뒤집은 것이 요지다.** 기구를 더한 것이 아니다 — 기구는 있었고 아무도 안 썼다.
기본값이 켜짐인 한, 새 소비자는 자기가 egui 를 링크하는지 묻지 않아도 되고 그래서 안 묻는다.

## Consequences

- **얻은 것**: 헤드리스 그래프 **344 → 330**(name 기준, `--edges normal`). 빠진 14 개는
  `ecolor` `egui` `egui_extras` `emath` `enum-map` `enum-map-derive` `epaint`
  `epaint_default_fonts` `mime` `mime_guess2` `nohash-hasher` `tasty-egui-theme`
  `tasty-ui-widgets` `unicase`. 그리고 새 소비자가 egui 를 링크하는지 **매니페스트에서
  선언하게** 된다.
- **★ 잃지 않은 것 — wgpu 스택은 그대로 남는다.** `wgpu`·`wgpu-core`·`wgpu-hal`·`naga`·
  `glow`·`ash`·`cosmic-text`·`fontdb`·`swash`·`skrifa` 는 **여전히 헤드리스 그래프에
  있다.** 들어오는 문이 `tasty-font` 하나이고(실측: `cargo tree -i wgpu` 의 역경로가
  `tasty-font → tasty` 단 하나), 그 크레이트는 헤드리스 코드가 **실제로 쓰면서**
  `wgpu`·`cosmic-text` 를 비-optional 로 든다. 그래서 이 결정으로는 안 빠지고, 빼려면 그
  크레이트를 설정/해석 타입과 GPU 아틀라스로 갈라야 한다 — 별개 작업이다.
- **운영 비용 / 유지 부담**: `tasty-type-appearance` 를 새로 의존하는 크레이트가 egui 색
  변환 shim(`HexColor` ↔ `Color32`, `ShadowToken::to_egui`)을 쓰려면 **컴파일 에러로**
  알게 된다. 방향이 안전한 쪽이다(조용한 통과가 아니라 시끄러운 실패).
- **번들 plugin 의 patch bump 를 요구한다** — 이 크레이트가 아홉 plugin 전부의 워크스페이스
  의존 폐포 안이다(ADR-0166).

## Alternatives Considered

- **A: 소비자마다 `default-features = false` 를 적는다** — 오늘 이미 가능한 형태이고,
  그래서 **이미 실패한 형태**다. 넷만 껐고 나머지 여덟은 안 껐다. 기본값이 켜짐인 한
  같은 일이 다음 소비자에게 되풀이된다.
- **B: `tasty-type-appearance` 를 둘로 가른다(색 primitive ↔ egui shim)** — 더 깨끗하지만
  크레이트가 하나 늘고 모든 소비자의 import 가 바뀐다. `default` 한 줄로 같은 결과를 얻는데
  그 비용을 낼 이유가 지금은 없다. shim 이 커지면 그때 다시 본다.
- **C: `tasty-font` 까지 이번에 가른다** — 그러면 wgpu 스택도 함께 빠진다. 그러나 그것은
  매니페스트 변경이 아니라 **크레이트 내부를 가르는 작업**이라 성격도 크기도 다르다.
  한 결정에 묶으면 되돌릴 때도 함께 되돌아간다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `tasty-type-appearance` 의 `default` 가 다시 비어 있지 않게 된다. **채널 없음** — 이
  회차는 새 가드를 만들지 않는 회차였다. 재는 법은 아래 첫 줄과 같다.
- `tasty-font` 가 `wgpu`·`cosmic-text` 를 비-optional 로 들지 않게 된다 — 그러면 위
  "잃지 않은 것" 이 해소되고 헤드리스 그래프에서 GPU 스택 전체가 빠진다. 채널 없음.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 헤드리스 그래프에 egui 가 **다시 들어온다.** 재는 법:
  `cargo tree --no-default-features --edges normal -i egui` — **안 들어왔으면 빈 출력이
  아니라 `did not match any packages` 로 죽는다**(실측 2026-09-20, rc=101). 즉 신호는
  출력이 아니라 종료 코드다: rc≠0 이면 그래프에 그 이름이 없는 것이고, rc=0 이면 그
  역경로가 누가 끌고 오는지를 그대로 찍는다. 스크립트에 넣을 때 그 둘을 "명령이
  고장났다" 와 섞지 마라. 크기는
  `cargo tree --no-default-features --edges normal --prefix none | awk '{print $1}' | sort -u | wc -l`.
  이 두 줄을 [`../dev-guide/build.md`](../dev-guide/build.md) 의 headless 절에 값 자리로
  남겼다.

## References

- 이 결정이 실현된 현재 위치: `crates/tasty-type-appearance/Cargo.toml` 의 `[features]`,
  루트 `Cargo.toml` 의 `gui` feature 와 `tasty-egui-theme`·`tasty-ui-widgets`·`tasty-icons`
  선언, 그리고 `features = ["egui-compat"]` 를 켜는 여섯 크레이트의 매니페스트.
- [`../dev-guide/build.md`](../dev-guide/build.md) — headless 절에 재는 법과 **채널이 없다는
  사실**을 적었다.
- [ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md) — 공용
  크레이트 변경이 번들 plugin 의 bump 를 요구하는 근거.
