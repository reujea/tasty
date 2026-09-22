# ADR-0111: headless 는 Intent 큐를 스스로 drain 해 engine 에 적용한다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: headless, intent, dispatch, cascade, ipc, queue, agent-surface

## Context

호스트 내부 액션은 `AppState::dispatch_intent` 로 `pending_intents` 큐에 push 되고,
gui 는 프레임 끝마다 `App::dispatch_pending_intents`(`src/app/dispatch/intents.rs`)가
모든 window / parked state 를 순회 drain 해 적용한다
([action-dispatch](../design/flows/action-dispatch.md)). 그런데 그 drain 경로는
window·view 의존이 커서 통째로 `#[cfg(feature = "gui")]` 이고, headless 메인 루프
(`src/boot.rs` 의 `run_headless`)와 IPC 펌프(`src/boot/headless_dispatch.rs`)는 그 큐를
읽지 않았다. 즉 **headless 에서는 큐에 넣는 쪽만 있고 꺼내는 쪽이 없었다.**

결과는 두 가지다.

1. **무한 적재** — 큐가 프로세스 수명 동안 요청 수에 비례해 자란다. 상시 구동하는
   headless 인스턴스(원격 attach 서버가 주 시나리오)에서 상한 없는 메모리 증가다.
2. **에이전트 표면의 조용한 무동작** — 핸들러는 `ok` 로 응답하는데 상태는 바뀌지
   않는다. `surface.set_mark` 가 성공을 회신하고도 mark 를 세우지 않으므로 이어지는
   `surface.read_since_mark` 가 스크롤백 전체를 돌려주는 식이다. 불가침 원칙 2
   ([identity](../identity.md), 에이전트 기능은 IPC + CLI 양면 동작)의 구멍이다.

headless 빌드에 실제로 컴파일되는 발화 지점을 컴파일러로 전수 확인하면 9 곳이고
(release 는 `debug.settings.apply` 가 빠져 8 곳), 거기서 나오는 것은 전부
`Intent::Domain` 이다 — kind 로는 `UpdateSettings` · `SurfaceCompletion` ·
`PushNotification` · `SetTerminalMark` · `DispatchFile` · `SurfaceAttentionClear`
6 종. gui 전용 발화 지점(view /
단축키 / popup 계층)은 애초에 headless 에 없고, 구조 변경 IPC(`tab.create`,
`surface.close`, `workspace.*` 등)는 큐를 거치지 않고 `Core::apply` + 자유함수 cascade
(headless 는 `src/app/dispatch_domain_stubs.rs`)로 **이미 직접 적용**한다.

[ADR-0107](0107-attention-clear-ipc-symmetry.md) 은 같은 결함을 attention 축에서 마주쳐
"headless dispatch 가 큐를 drain 한다" 를 대안으로 검토했으나, **모든 도메인 cascade 가
headless 에서 처음으로 돌기 시작한다** 는 변경 폭을 이유로 미루고 "별도 트랙에서 도메인별로
검증하며 여는 편이 맞다" 고 적었다. 본 트랙이 그 별도 트랙이며, 위 전수 조사가 그 우려의
전제를 좁힌다 — headless 에서 큐로 들어오는 도메인은 전 도메인이 아니라 5 종이고, 나머지
도메인은 애초에 큐를 쓰지 않는다.

## Decision

**headless 는 자기 Intent 큐를 스스로 drain 해 적용한다.** 진입점은
`crate::intent::headless::drain_pending_intents(core, state, engine)` 하나이고,
IPC 요청 처리 직후(**응답 송신 전**) · plugin 호출 결과 회신 전 · 메인 루프가 블로킹
대기에 들어가기 전 세 지점에서 호출한다. 응답 전 호출은 gui `dispatch_with_caller` 가
응답 반환 전에 drain 하는 것과 같은 계약으로, `set_mark` 응답을 받은 호출자가 곧바로
`read_since_mark` 를 물어도 mark 가 서 있게 만든다.

적용 규칙은 gui 와 같은 경계를 쓴다 — `Intent::Domain` 은 `Core::apply` 후 이벤트
cascade, 나머지는 gui `dispatch_one_intent` 와 같은 표로 도메인 핸들러 직결. cascade 는
gui `App::handle_core_event` 중 **engine 상태로 완결되는 부분만** 가져온다(attention
발동, notification 적재, terminal mark, cwd 표시 갱신, settings 적용·저장). view
redraw · toast · theme 재설치 · NSMenu 재구성처럼 소비처가 GUI 인 것과, 알림음
(headless 빌드는 `src/boot/wiring.rs` 가 NoopPlayer 를 명시 주입한다)은 제외한다.

**host event 발화는 하지 않는다.** headless 에는 `pending_host_events` 를 drain 하는
주체가 없어(`src/app/dispatch/host_events.rs` 가 gui 전용) enqueue 하면 지금 없애는
것과 똑같은 무한 적재를 다른 큐에 새로 만들게 된다. 이 제약은 기존
`dispatch_domain_stubs.rs` 가 이미 같은 이유로 지키고 있던 규칙이다.

drain 은 한 번 호출에 최대 8 라운드를 돌아 적용 중 새로 발화된 intent 까지 이어서
처리하고, 상한에 걸린 나머지는 버리지 않고 다음 drain 이 처리한다. 큐 길이가 "처리 중인
작업량" 에만 비례하고 요청 누적수에 비례하지 않는다는 것이 이 결정의 불변식이며,
`src/intent/headless.rs` 의 회귀 테스트가 그것을 고정한다.

drain 모듈은 **gui 빌드의 시험 구성에서도 컴파일한다**(모듈 선언의
`cfg(any(not(feature = "gui"), test))` — 호출부가 headless 전용이라 gui 라이브러리에는 안 넣는다). 그래야 기본 빌드의
`cargo test --workspace` 가 이 경로를 회귀 검증한다 — headless 전용으로 가리면 큐 누적
회귀는 `--no-default-features` 를 따로 돌린 사람만 보게 된다.

## Consequences

- **얻은 것**: headless 의 Intent 큐가 유계가 됐다. `surface.set_mark` ·
  `surface.completion` · `notification.create` · `settings.set_remote_transfer` 가
  headless 에서 실제로 동작한다 — 지금까지는 `ok` 를 회신하고 아무 일도 하지 않았다.
  발화 지점이 앞으로 늘어도 drain 이 한 곳이라 같은 결함이 재발하지 않는다.
- **잃은 것**: headless cascade 가 gui cascade 의 부분집합이라, 두 실행 형태의 동작
  차이가 코드 두 곳(`src/app/dispatch_domain.rs` 와 `src/intent/headless.rs`)에 나뉘어
  존재한다. gui cascade 에 engine 상태 변경이 추가되면 headless 쪽에 반영이 필요한지
  판단해야 한다. 반영이 빠져도 큐 유계성은 유지되므로 증상이 조용하다.
- **운영 비용 / 유지 부담**: headless 에서 host event 를 필요로 하는 소비처(plugin
  event bus)가 생기면 `pending_host_events` 의 drain 주체를 먼저 만들어야 한다 —
  그 전에는 enqueue 를 늘리지 않는다. `Core::apply` 가 새 `CoreEvent` 를 내면
  headless 쪽은 debug 로그만 남기고 지나가므로, 새 도메인을 headless 발화 경로에
  얹을 때 cascade 반영 여부를 함께 본다.

### 보강 — OSC 7 cwd 의 탭 이름 (2026-09-23)

Decision 이 headless cascade 에 넣은 "cwd 표시 갱신" 은 이 결정 시점에 **닿지 않았다.** gui 에서
그 갱신은 `TerminalCwdChanged`(PTY drain 이 내는 `CoreEvent`)가 `SurfaceCwdChanged` intent 를 큐에
넣고 그 intent 의 cascade 가 탭 이름을 다시 매기는 두 단인데, headless PTY drain(`src/boot.rs`)은
`TerminalCwdChanged` 를 훅 발화 함수로 흘려 버렸고 `SurfaceCwdChanged` 는 gui 전용 variant 다. 그래서
Intent 큐 drain 에는 올 것이 없었다. 이제 headless PTY drain 이 `TerminalCwdChanged` 를 받아
`intent::headless::apply_terminal_cwd_changed` 로 engine 갱신(탭 이름 · 레이아웃 dirty)을 직접 한다 —
같은 "engine 에 완결되는 부분만" 규칙을 intent 가 아니라 이벤트 자리에 적용한 것이다. 시험:
`tests/e2e_tests.rs` 의 `an_osc7_cwd_becomes_the_tab_name`(두 조합).

## Alternatives Considered

- **A. 핸들러가 headless 에서는 push 대신 직접 적용한다** — 형태로는
  [ADR-0107](0107-attention-clear-ipc-symmetry.md) 이 `surface.attention.*` 하나에
  적용한 방식의 일반화다. 채택하지 않은 이유는 발화 지점 8~9 곳에 `#[cfg]` 분기를
  각각 심어야 하고(핸들러마다 gui 경로와 headless 경로가 갈린다), 그렇게 해도 **결함의
  원인이 남는다** — 앞으로 큐에 push 하는 핸들러가 하나 추가되면 그 핸들러만 조용히
  같은 상태로 되돌아간다. 원인이 "큐에 꺼내는 주체가 없다" 인 이상 고칠 곳은 발화
  지점이 아니라 drain 지점이다.
- **B. intent kind 별로 나눈다(일부는 직접 적용, 일부는 drain)** — 두 모델을 동시에
  유지하는 비용만 늘고, 어느 kind 가 어느 쪽인지가 새 코드 작성자에게 보이지 않는다.
  분기 기준을 문서로만 유지해야 한다는 점에서 A 의 단점을 그대로 물려받는다.
- **C. 큐를 비우기만 하고 적용하지 않는다(누적만 제거)** — 메모리 문제는 사라지지만
  "성공을 회신하고 아무 일도 하지 않는다" 는 쪽은 그대로다. 두 증상은 원인이 같아
  한쪽만 고칠 이유가 없다.
- **D. gui 의 `App::dispatch_pending_intents` 를 headless 에서 재사용한다** — window /
  parked state / view 순회가 본체라 headless 에는 대응물이 없다. engine 하나짜리로
  좁힌 별도 진입점이 실제로 공유 가능한 최대치다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- headless 가 `pending_host_events` 를 drain 하는 주체를 갖게 되면 — 그때는 notification
  cascade 의 host event 발화 생략을 되돌리고, plugin event bus 를 headless 에서도 연다.
- gui cascade 와 headless cascade 의 차이가 "engine 완결 여부" 로 설명되지 않을 만큼
  벌어지면, `handle_core_event` 의 engine 부분을 자유함수로 추출해 양쪽이 같은 본문을
  부르는 형태로 접는다.
- headless 발화 지점에서 non-Domain intent 가 실제로 발생하기 시작하면(지금은 라우팅만
  대비해 두고 도달하는 경로가 없다) 그 경로의 headless 동작을 개별 검증한다.
- drain 라운드 상한(8)에 실제로 걸리는 조합이 관측되면 상한값이 아니라 재발화 구조를
  본다 — 상한은 무한 루프 방지용이지 처리량 조절 수단이 아니다.

## References

- [design/flows/action-dispatch](../design/flows/action-dispatch.md) — Intent 큐의 발화·처리 규칙
- [ADR-0107](0107-attention-clear-ipc-symmetry.md) — attention 축을 핸들러 직접 적용으로 푼 선행 결정. 그 ADR 의 재검토 조건 1 번("headless dispatch 가 Intent 큐를 drain 하게 되면 핸들러의 직접 적용은 중복")은 본 ADR 로 **부분 충족**이다 — `SurfaceCompletion` 축에서는 중복이 되었으나, `SurfaceAttentionClear` 는 headless cascade arm 이 없어 핸들러 직접 적용이 여전히 유일 경로다
- [ADR-0039](0039-surface-highlight-shared-primitive.md) — attention 이 producer 중립 공유 상태라는 근거
- [ADR-0050](0050-headless-pty-primitive.md) — headless 실행 형태의 자원 회수 모델
- [identity](../identity.md) — 불가침 원칙 2(에이전트 기능의 IPC + CLI 양면 동작)
- 부분 개정: [0346](0346-headless-compiles-only-what-it-reaches.md) (생산자 없는 non-Domain 라우팅 보존 조항 개정)
