# ADR-0530: 모듈·crate 단위 dead_code 억제는 판정이 사용을 못 보는 두 부류에만 남긴다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: dead-code, lint, headless, cfg, tests, generated-code, adr-0346

## Context

[ADR-0346](0346-headless-compiles-only-what-it-reaches.md) 은 조건이 `not(feature = "gui")` 하나뿐인
모듈 단위 `allow(dead_code)` 를 걷어 냈다. 그 패턴 밖의 모듈·crate 단위 억제는 남아 있었다 —
`git grep -nE '#!\[(cfg_attr\([^]]*)?allow\([^)]*dead_code' -- '*.rs'` 로 17 자리(실측 2026-09-23,
베이스 `d5e1a9632`). 조건 없는 것 넷(`src/adapters/test/mod.rs` · 단축키 modifier-hint 모델 ·
headless dispatch stub · `RemoteSurface`), 다른 cfg 조합의 것 둘(`src/intent/headless.rs` 의
`cfg_attr(feature = "gui", ..)` · `macos_permissions.rs` 의 `not(all(macos, gui))`), crate 전체 셋
(`tasty-file-format` · `tasty-file-handler` · `tasty-model`), 생성 파일과 그 생성기 둘, 통합 시험
공용 모듈 여섯이다.

모듈 억제는 그 자리만 가리지 않는다. 억제된 모듈이 부르는 정의는 호출자가 살아 있는 것으로
보이므로, 억제를 지우면 다른 모듈의 정의가 새로 드러날 수 있다(ADR-0346 회차의 실측).
남겨 두면 무엇이 정말로 마지막 호출자를 잃었는지가 안 보인다.

## Decision

모듈·crate 단위 `dead_code` 억제는 **판정이 사용을 원리적으로 못 보는 두 부류**에만 남기고,
나머지는 지운 뒤 드러난 정의를 항목마다 가른다.

남기는 두 부류:

- **통합 시험 공용 모듈**(`tests/*/mod.rs` 여섯). 각 test binary 가 `mod` 로 따로 컴파일하고
  binary 마다 쓰는 부분집합이 달라, 어느 한 binary 에서는 반드시 dead 가 생긴다. 판정 단위가
  binary 라 사용 전체를 볼 자리가 없다. 자리마다 사유 주석을 단다.
- **생성 파일**(`tasty-design-tokens` 의 `generated/primitive.rs` 와 그 줄을 쓰는 생성기 문자열).
  `pub(crate)` primitive 스케일을 미참조 엔트리까지 보존하는 것이 생성 계약이다. 사유는 생성되는
  줄에 같이 붙어 있다.

지운 자리에서 드러난 정의의 처분 — [헤드리스 정의 경계](../dev-guide/headless-build-boundaries.md) 의
세 갈래와 같은 기준이고, 조건이 feature 가 아닐 때도 같은 형태를 쓴다:

- 호출자가 없으면 **지운다** — `PluginBindingInput::resolve` · headless `DispatchSource` ·
  `FakeClock::advance` · 아무도 안 읽는 `SpawnRecord` 필드.
- 호출자가 한 조건에만 있으면 **그 조건으로 가린다** — `RemoteSurface` 의 webview URL · 핸들 공유
  (`cfg(feature = "gui")`)와 navigation mirror(`cfg(any(feature = "gui", test))`), 시험만 쓰는 이름
  setter(`cfg(test)`), headless drain 모듈(`cfg(any(not(feature = "gui"), test))`), macOS 권한의
  순수 결정부(`cfg(any(target_os = "macos", test))`).
- 만드는 자는 있고 읽는 자가 한 조건에만 있으면 **사유 붙은 `expect`** — headless
  `WorkspaceCreatedCascade` 의 필드.

crate 전체 셋은 억제를 지워도 dead 가 **하나도** 안 드러났다 — 공개 항목은 라이브러리의
`dead_code` 판정 대상이 아니다. `tasty-file-format` 의 억제가 실제로 가리던 것은
`unused_imports` 로, registry 하위 모듈마다 복사된 import 머리였다. 그것을 걷었다.

같은 회차에 item 단위 `cfg_attr(not(feature = "gui"), allow(dead_code))` 넷도 세 갈래로 맞췄다 —
`layout_persistence::list_slots` 는 ①(`cfg(feature = "gui")`), `App` 과 `AppEvent` 의 두 variant 는
③(`expect`).

## Consequences

- **얻은 것**: 남는 모듈 억제가 두 부류로 닫혔고 둘 다 판정이 못 보는 이유가 구조에 있다. 지운
  자리의 정의는 이제 조건이 틀리면 컴파일러가 이름으로 알린다(`dead_code` 는 deny 다).
  `expect` 자리는 조건이 사라지면 `unfulfilled_lint_expectations` 가 경고한다.
- **잃은 것**: 배선 전인 plugin 단축키 해석(`resolve`)과 시각을 움직이는 fake clock 이 사라졌다.
  배선하거나 시험이 필요해지면 그때 다시 쓴다.
- **운영 비용 / 유지 부담**: macOS 조합은 이 머신에서 컴파일되지 않는다 — macOS 순수부의 cfg 는
  논리로만 확인했다. `scripts/check-allow-reason.sh` 의 상한이 지운 억제만큼 내려갔다.

## Alternatives Considered

- **통합 시험 공용 모듈을 binary 마다 `#[allow]` 로 include** — `mod common;` 선언마다 억제를
  옮기면 자리가 여섯에서 include 수만큼 늘고, 사유는 같다. 안 골랐다.
- **공용 모듈을 dev-only 라이브러리 crate 로 분리** — 공개 항목이 되어 판정이 사라지는 것은 같고,
  워크스페이스 멤버 하나와 그 빌드 비용이 는다. 안 골랐다.
- **드러난 dead 를 전부 `expect` 로 덮기** — 가장 적게 바뀌지만, 호출자가 전혀 없는 정의까지
  "조건부로 쓰인다" 로 적게 된다. 삭제가 기본이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 위 세는 법이 두 부류 밖의 자리를 낸다. 그때는 이 결정을 따르거나 새 부류를 이 ADR 에 더한다.
- `tests/` 공용 모듈이 하나의 test binary 로 합쳐진다 — 판정 단위가 하나가 되므로 억제가 필요
  없어진다.
- `tasty-design-tokens` 의 primitive 가 `pub` 이 된다 — 공개 항목이 되어 억제가 필요 없어진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- macOS 에서 순수부 cfg 가 틀렸다. 재는 법: macOS 에서 `cargo check -p tasty-platform --features gui`
  와 `--all-targets` 를 돌려 `dead_code` 오류가 없는지 본다.

## References

- 운영 문서: [headless-build-boundaries](../dev-guide/headless-build-boundaries.md)
- 선행 결정: [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) (not(gui) 모듈 억제를 걷은 결정 — 이 ADR 은 그 밖의 조건을 다룬다)
- 선행 결정: [ADR-0111](0111-headless-drains-the-intent-queue.md) (headless drain 모듈을 gui 시험에서도 컴파일한다는 조항 — 조건을 `cfg(any(not(feature = "gui"), test))` 로 좁혔다)
- 탐색: `git grep -l 'allow(dead_code)' -- docs/adr/` · `git grep -l 'dead_code' -- docs/adr/`
