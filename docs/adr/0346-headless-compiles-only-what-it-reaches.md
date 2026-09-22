# ADR-0346: headless 는 자기가 닿는 정의만 컴파일한다 — ADR-0111 의 non-Domain 보존 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: headless, intent, feature, dead-code, adr-0111

## Context

`src/lib.rs` 에 `#![cfg_attr(not(feature = "gui"), allow(dead_code))]` 가 있었다. headless
빌드 전체에서 `dead_code` 를 침묵시키는 crate 단위 예외다. 이유로 적힌 것은 "gui 사용처가
사라져 코어 타입 다수가 dead 로 판정된다 — 컴파일 가드 빌드일 뿐" 이었다.

그 예외를 끄고 재면 출하 라이브러리에서 86 건, 테스트 구성에서 65 건이 나왔다(실측
2026-09-21, 고유 좌표 90 · 34 파일). 그 수 자체보다 중요한 것은 **그 안에 두 종류가 섞여
있었다는 것**이다. 하나는 GUI 소비자만 있는 정의고, 다른 하나는 정말로 마지막 호출자를
잃은 정의다. 예외가 있는 동안 둘은 똑같이 보였고 둘 다 보고되지 않았다.

[ADR-0111](0111-headless-drains-the-intent-queue.md) 은 headless 의 Intent 큐를 응답 전에
적용하면서, 당시 headless 생산자가 없던 non-Domain 요청도 GUI 와 같은 핸들러로 라우팅하도록
남겼다. 그 대비 경로가 위 86 건의 큰 갈래를 이룬다.

## Decision

crate 단위 예외를 제거하고, 그것이 덮던 자리마다 **경계 아니면 근거** 둘 중 하나를 둔다.

**경계** — headless 에 생산자나 호출자가 없는 정의는 `cfg(feature = "gui")` 로 제외한다.
GUI 타입을 담는 variant·필드·분기도 같은 경계를 갖는다. 모듈 통째가 그 안쪽이면 모듈
선언에 붙이고, 일부 항목만이면 그 항목에 붙인다. 순수 단위 테스트가 **실제로 그 정의를
호출하면** `cfg(any(feature = "gui", test))` 로 한다 — 호출하지 않는데 `test` 를 포함시키면
그 정의는 라이브러리 대신 테스트 구성에서 dead 로 남는다.

**근거** — 정의가 headless 에서도 생산되지만 읽는 자리가 GUI 뿐이면 제외하지 않고
`cfg_attr(not(feature = "gui"), expect(dead_code, reason = ...))` 를 그 자리에 둔다. 빼면
동작이 바뀌기 때문이다. 이 형태에 해당하는 것은 셋이다 — 양쪽 빌드가 채우지만 GUI 의
이벤트 루프만 비우는 forward 큐, plugin 매니페스트에서 복사되는 surface kind 플래그,
그리고 headless drain 이 cascade 를 배선하지 않은 CoreEvent 의 페이로드. 모듈이나 crate
전체를 덮는 dead_code 예외는 쓰지 않는다.

**판정 순서** — 바깥에서 안으로 한다. 진단 목록은 평면이지만 사실은 그래프다: 어떤 정의가
dead 로 보이는 이유가 **그 호출자가 같은 실행에서 함께 dead 로 잡혔기 때문**일 수 있다.
안쪽을 먼저 자르면 컴파일이 깨진다.

**재는 법** — `gui` × `headless` 의 두 feature 조합을, 각각 라이브러리와 `--all-targets`
로, 각각 debug 와 `--release` 프로필에서 잰다. 여덟 칸이다. 한 칸만 보면 나머지에서 회귀가
조용히 나간다. 프로필이 축인 이유는 `debug_assertions` 가 feature 와 독립인 두 번째 경계라서다
— headless 쪽 호출자가 debug 전용 핸들러뿐인 정의는 debug 네 칸에서 살아 있고 release
headless 에서만 dead 가 된다. 그런 정의의 경계는 호출자 조건의 합집합
(`cfg(any(feature = "gui", debug_assertions))`, 테스트가 부르면 `test` 추가)이다.

ADR-0111 의 다음 조항은 개정하지 않는다.

- IPC 응답 송신 전·plugin 회신 전·blocking 대기 전의 drain 시점.
- 공유 `Core::apply` 를 통한 Domain 적용과 엔진에서 완결되는 cascade.
- 제한된 drain 라운드와 남은 요청을 다음 drain 으로 넘기는 큐 계약.
- GUI 에서도 headless drain 모듈을 컴파일해 기존 회귀 테스트를 실행하는 구조.
- GUI redraw·popup·알림음과 엔진 상태 변경을 구분하는 경계.

개정하는 것은 **생산자 없는 non-Domain 요청을 미리 라우팅해 두는 조항** 하나다.

## Consequences

- **얻은 것**: headless 빌드가 라이브러리와 테스트 구성 모두에서 dead 정의 0 을 보고한다.
  이 0 은 **debug 프로필의 두 칸**에서 잰 값이다(실측 2026-09-21). 같은 날 release headless
  는 dead 16 건을 냈다 — debug 핸들러만 부르던 정의들이다. 그 칸들을 이후 경계로 닫았고, 지금
  여덟 칸이 모두 0 이라는 것은 위 "재는 법" 으로 그 자리에서 재는 값이다. 그래서 앞으로 나오는
  진단은 전부 새 사실이다. 이 회차 자체가 그 값을 보여줬다 — 예외를
  끈 상태에서만 보이는 `fire_terminal_hooks` 호출 형태 불일치가 push 를 막아서야 드러났다.
- **얻은 것**: 무엇이 GUI 전용인지가 정의 옆에 적힌다. 지금까지는 그 답이 아무 데도 없었다.
- **잃은 것**: GUI 정의에 headless 호출자를 더할 때 `cfg` 와 처리 경로를 함께 고쳐야 한다.
  미리 라우팅해 두던 비용 대신, 새 기능을 들이는 변경에 그 결정을 둔다.
- **운영 비용**: 위 "재는 법" 의 여덟 칸을 커밋 전에 돌려야 한다. 자동 채널은 그중 하나도
  커밋 전에 보지 않는다 — headless debug 컴파일은 pre-push 와 `check-headless` 가, release gui
  라이브러리는 pre-push 와 `check-release` 가 본다([ci-gates](../dev-guide/ci-gates.md)).
  release headless 두 칸과 release gui `--all-targets` 칸은 **어떤 자동 채널도 안 본다**.
- **드러난 것**: headless 는 레이아웃을 저장하지도 복원하지도 않는다. 그 경로 전체가 GUI
  경계 안으로 들어간 것은 기능을 뺀 것이 아니라 이미 그랬던 사실이 보이게 된 것이다.
  마찬가지로 headless 의 OSC 7 cwd 변경은 탭 이름을 갱신하지 않았다 — 터미널 이벤트에서
  그 intent 로 가는 배선이 그 빌드에 없었다.
  (보강 2026-09-23: 둘 다 판정됐다. OSC 7 은 headless PTY drain 에 배선했다 —
  [ADR-0111 "보강 — OSC 7 cwd 의 탭 이름"](0111-headless-drains-the-intent-queue.md).
  레이아웃은 저장·복원하지 않는 것으로 정했다 — [ADR-0539](0539-headless-does-not-persist-layouts.md).)

## Alternatives Considered

- **crate 단위 예외를 유지하고 자리마다 좁은 `allow` 를 더한다**: 예외가 이미 전부를 덮고
  있어 더하는 `allow` 가 아무것도 바꾸지 않는다. 좁은 `allow` 가 의미를 가지려면 넓은
  것이 먼저 없어져야 한다.
- **공유 enum·CoreState·registry 전체를 GUI 로 제한한다**: headless IPC 와 attach 서버가
  쓰는 도메인·자원까지 사라진다.
- **생산자 없는 variant 마다 lint 예외를 준다**: 실제 headless 생산자가 없으므로 오진으로
  분류할 근거가 없다. 대비 라우팅을 위해 출하 코드에 죽은 정의를 남기는 선택이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 위 "근거" 로 남긴 자리의 조건이 달라지면 `expect` 가 충족되지 않아
  `unfulfilled_lint_expectations` 가 그 자리를 이름으로 경고한다(빌드·CI 를 막지는 않는다 — warn 이고
  `Cargo.toml` deny 목록 밖이며 CI 는 `-D warnings` 를 안 쓴다). headless drain 에 cascade
  를 배선하거나 forward 큐를 비우는 쪽이 생기는 순간이 그 시점이다.

**원리적으로 안 붙는 것** — 사람이 요구사항으로 판단한다.

- headless 에 non-Domain 생산자가 필요해지면 그 variant 의 `cfg` 와 실제 drain 결과를 함께
  판정한다. 재는 법: 두 feature 조합의 컴파일과, 그 요청의 응답 전 상태 단언.
- headless 가 레이아웃을 저장·복원해야 하는지, OSC 7 이 그 빌드에서 탭 이름을 갱신해야
  하는지는 제품 결정이다. 재는 법: 그 동작을 요구하는 헤드리스 통합 테스트를 먼저 쓰고,
  그것이 빨간 것을 확인한다.
  **판정됨(2026-09-23)**: OSC 7 → 갱신한다([ADR-0111](0111-headless-drains-the-intent-queue.md) 보강 절),
  레이아웃 → 저장·복원하지 않는다([ADR-0539](0539-headless-does-not-persist-layouts.md)).

## References

- 개정 대상: [ADR-0111](0111-headless-drains-the-intent-queue.md) (생산자 없는 non-Domain 라우팅 보존 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [헤드리스 정의 경계](../dev-guide/headless-build-boundaries.md)
- [액션 발화·dispatch](../design/flows/action-dispatch.md)
- 결정이 실현된 현재 위치: `intent::headless::handle_core_event`, `Core::apply`, `src/lib.rs` 의 crate 속성 목록
