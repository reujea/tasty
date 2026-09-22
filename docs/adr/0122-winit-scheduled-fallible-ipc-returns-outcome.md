# ADR-0122: winit 루프에 스케줄되는 실패 가능한 release IPC op 은 완료 채널로 결과를 돌려준다 — fire-and-forget 금지

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: ipc, window, event-loop, agent, error-handling, identity-principle-1, fire-and-forget, completion-channel, adr-0117
- **Group**: ipc-transport

## Context

`window.create` / `view.create` 는 `AppEvent::CreateWindow` 를 winit 이벤트 루프 프록시에 밀어넣고 곧바로 `{"scheduled": true}` 를 돌려줬다(fire-and-forget). 창 생성은 winit `ActiveEventLoop` 가 있어야 하므로 IPC 처리 중에는 만들 수 없어 다음 프레임으로 미뤄지는데, 그 결과가 응답에 반영되지 않았다.

`create_new_window` 은 5 개의 실패 지점을 갖는다(창 생성 · GPU 초기화 · 엔진 생성 · DB 초기화 · theme fallback). 앞 셋은 창 생성을 취소하는 실패다. 그런데 그 실패가 **요청자(에이전트)에게는 전혀 가지 않았다** — ADR-0117 이 에이전트 발 실패를 사용자 toast 로 냈기 때문에, 정작 요청한 에이전트는 창이 열렸다고 믿고 사용자는 자기가 하지도 않은 일의 실패 통지를 받았다.

이건 한 메서드의 문제가 아니다. winit 루프에 op 를 스케줄하고 `{"scheduled": true}` 를 즉시 돌려주는 형태가 늘어나면 **IPC 응답 계약이 메서드마다 갈린다**. 현재 표면을 전수 조사한 결과:

| 메서드 | release? | 스케줄 방식 | 실패 가능? | 종전 |
|--------|----------|-------------|-----------|------|
| `window.create` / `view.create` | release | `AppEvent::CreateWindow` | 예(창/GPU/엔진) | `{"scheduled": true}` |
| `system.shutdown` | debug | `AppEvent::Shutdown` | 아니오(무결점) | `{"shutdown": true}` |
| `debug.settings.open` | debug | `AppEvent::OpenSettings` | 예 | `{"scheduled": true}` |
| `ui.screenshot` | release | 렌더 파이프라인 pending 플래그 | 예(렌더 시점) | `{"scheduled": true}` |
| `debug.lua.eval` | debug | Lua 워커 스레드 | 예 | `{"scheduled": true}` |

`window.close` / `remote.attach` / `remote.workspaces` 는 이미 실제 결과(동기 또는 워커 왕복)를 돌려주므로 대상이 아니다.

## Decision

**release IPC 메서드가 winit 이벤트 루프에 op 를 스케줄하고 그 op 이 실패할 수 있으면, `{"scheduled": true}` 를 즉시 돌려주는 대신 완료 채널로 성공/실패를 요청자에게 돌려준다.** 판정 기준 두 축을 **둘 다** 충족할 때만 대상이다:

1. **release 표면** — 에이전트가 의존하는 안정 계약인가. debug 전용 메서드(`#[cfg(debug_assertions)]`)는 테스트용 격리 표면이라(원칙 1 의 debug 격리) 이 계약에 매이지 않는다.
2. **실패 가능** — op 이 실패할 수 있는가. 무결점 op(`system.shutdown`)는 돌려줄 실패가 없으므로 즉시 ack 로 충분하다.

**메커니즘**: IPC 핸들러가 `IpcCompletion`(요청 id + `response_tx` 를 담은 완료 채널, `src/app/event.rs`)을 `AppEvent` 에 실어 보내고 응답을 **defer**(`IpcStep::Handled`, 즉시 응답하지 않음)한다. winit 핸들러가 op 를 실제로 수행한 뒤 `reply_ok(result)` / `reply_err(code, msg)` 로 결과를 그 채널에 보낸다. 이는 이미 있는 `approval.await` / `agent.task_await` 의 "응답을 나중에 보내는" 패턴과 같은 결이며, IPC 처리 스레드를 **블록하지 않는다**(블록하면 같은 스레드에서 도는 winit op 와 교착한다 — 아래 대안 참조).

**적용(이 결정에서 구현)**: `window.create` / `view.create`.
- `create_new_window` 이 `Result<WindowId, String>` 을 반환한다. 성공 응답은 `{"created": true, "window_id": <u64>}`, 실패 응답은 JSON-RPC 에러(원인 문자열 포함, code `-32000`).
- **ADR-0117 갱신**: 에이전트 발 창 생성 실패는 이제 요청자에게 응답 에러로 가고 **사용자 toast 는 내지 않는다**. 사용자가 요청하지도 않은 일의 실패 통지가 화면에 뜨는 것 자체가 원칙 1 위반이었고, 동기 응답이 생긴 지금은 불필요하다. 사용자 발(menu/tray) 실패는 종전대로 InfoModal.

**범위 밖**:
- `debug.settings.open` — 축 1(release) 미충족. 같은 `IpcCompletion` 메커니즘을 그대로 쓸 수 있으나, debug 격리 표면이라 안정 계약 대상이 아니다. debug 메서드에 같은 보증이 필요해지는 사례가 생기면 재검토 트리거로 다룬다.
- `ui.screenshot` · `debug.lua.eval` — AppEvent 스케줄이 아니라 렌더 파이프라인 / 워커 스레드 경로다. 완료 지점이 winit 핸들러가 아니라 다른 곳이라 이 메커니즘(`AppEvent` 에 채널 싣기)이 그대로 맞지 않는다. 필요해지면 각 경로의 완료 지점에 맞는 채널로 별도로 다룬다.

## Consequences

- **얻은 것**: 에이전트가 `window.create` 의 실제 성공/실패와 새 창 id 를 받는다 — "열렸다고 믿었는데 아니었다" 가 사라진다. 실패가 요청자에게 가므로 사용자 화면을 오염시키던 toast 를 제거해 원칙 1 을 회복한다. `IpcCompletion` 은 한 벌로 공유되어 이후 다른 release+실패가능 스케줄 op 도 같은 계약으로 붙는다.
- **잃은 것**: `AppEvent::CreateWindow` 가 `Option<IpcCompletion>` 을 싣게 되어 모든 producer(menu/단축키/tray/macOS delegate)가 `None` 을 명시한다. IPC 응답이 defer 되므로, 완료 채널이 응답 없이 drop 되면(예: 이벤트 루프 종료) 요청자는 무한 대기가 아니라 disconnect 를 본다(`SyncSender` drop → receiver `Err`) — 이 성질을 단위 테스트로 고정했다.
- **회귀 방어**: 이 경로는 winit `ActiveEventLoop` 가 있어야 돌아가 행동 테스트로 감쌀 수 없다(ADR-0117 과 같은 제약). 세 겹으로 고정한다 — ⑴ `IpcCompletion` 의 완료 동작은 단위 테스트(`src/app/event.rs` — reply_ok/reply_err/drop-disconnect), ⑵ `create_new_window` 이 `Result` 를 반환한다는 사실은 **타입 시스템**(void 로 되돌리면 전 호출자가 컴파일 실패)과 소스 가드, ⑶ window.create 가 완료 채널로 배선된 채 유지되는지는 소스 형태 가드(`crates/tasty-doc-guards/tests/ipc_window_create_returns_outcome.rs`) + dead-code lint(fire-and-forget 로 되돌리면 `IpcCompletion` 이 미사용이 되어 `-D dead-code` 로 빌드 실패).

## Alternatives Considered

- **fire-and-forget 유지 + toast(현행)** — 가장 단순하나 요청자가 실패를 못 보고(응답에 결과 없음), 실패가 사용자 화면으로 새어 원칙 1 을 어긴다. 이 ADR 이 고치려는 결함 그 자체.
- **IPC 스레드를 블록해 op 완료를 기다린다** — 응답을 동기로 만드는 가장 직관적 방법이나, IPC 는 메인 스레드에서 처리되고 창 생성도 같은 메인 스레드의 winit 콜백에서 일어난다. IPC 핸들러가 그 자리에서 블록하면 자기가 스케줄한 op 가 도는 스레드를 막아 **교착**한다. defer(응답을 나중에 winit 핸들러에서 보냄)가 유일하게 맞는 형태다 — `approval.await`/`task_await` 도 같은 이유로 그렇게 한다.
- **무결점 op 까지 전부 왕복으로 바꾼다** — 계약이 가장 균일해지나, 돌려줄 실패가 없는 `system.shutdown` 에 완료 채널을 다는 것은 순비용이다. "실패 가능" 축으로 걸러내는 편이 낫다.
- **debug 메서드도 함께 왕복으로 바꾼다** — 계약이 release/debug 를 가로질러 완전히 균일해진다. 그러나 debug 표면은 격리된 테스트용이라(원칙 1) 안정 계약 대상이 아니고, `AppEvent::OpenSettings` 는 사용자 menu 경로와 공유되어 변경 반경이 커진다. release 축으로 경계를 그어 지금은 범위 밖으로 둔다(비-breaking, 필요 시 같은 메커니즘으로 확장).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- winit 루프에 스케줄되는 **release + 실패 가능** IPC 메서드가 새로 생긴다 — 같은 완료 채널 계약을 적용한다(fire-and-forget 로 시작하지 않는다).
- debug 메서드(`debug.settings.open` 등)에 실제 완료 보증이 필요한 사례가 생긴다 — 그때 같은 `IpcCompletion` 으로 debug 표면까지 넓힌다.
- `ui.screenshot` / `debug.lua.eval` 처럼 AppEvent 가 아닌 경로(렌더 파이프라인 · 워커 스레드)의 결과를 요청자에게 돌려줘야 하는 요구가 생긴다 — 그 완료 지점에 맞는 채널을 설계한다(이 ADR 의 AppEvent 메커니즘을 그대로 쓰지 않는다).
- 스케줄된 op 이 완료까지 오래 걸려(예: 사용자 상호작용을 기다리는 창) IPC 요청자의 timeout 을 넘기기 시작한다 — `agent.task_await` 처럼 명시적 timeout/취소 의미를 얹는다.

## References

- [`docs/adr/0117-window-and-modal-creation-failure-policy.md`](0117-window-and-modal-creation-failure-policy.md) — 창·모달 생성 실패 정책. 이 ADR 이 그 §3(에이전트 발 실패 안내 채널)을 toast 에서 IPC 응답으로 갱신한다
- [`docs/identity.md`](../identity.md) — 핵심 원칙 1(사용자 행동 ↔ 에이전트 행동 분리) · debug 격리
- `src/app/event.rs` — `IpcCompletion` 완료 채널
- `src/app/window_lifecycle.rs` — `create_new_window`(Result 반환) · `notify_window_creation_failed`
- `src/app/ipc/app_methods.rs` — `window.create` / `view.create` IPC 핸들러(응답 defer)
- `crates/tasty-doc-guards/tests/ipc_window_create_returns_outcome.rs` — 완료 채널 배선 소스 가드
