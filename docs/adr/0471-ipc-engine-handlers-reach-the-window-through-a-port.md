# ADR-0471: IPC 엔진 핸들러는 창에 포트와 intent 출구로만 닿는다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: ipc, handler, app-state, ownership, port, intent, focus, headless, adr-0355, adr-0470

## Context

[ADR-0470](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md) 은 창 상태를 안
읽는 IPC 핸들러에서 `AppState` 인자를 뺐다. 남은 자리는 창 상태를 **실제로 읽는** 핸들러와,
그것에 넘기려고 `AppState` 를 받는 라우터 · `check_request` · `pump_ipc` 였다. 그 ADR 은 이
자리를 좁히는 남은 걸음을 적었다 — intent 큐 · 대상 생략 시의 `active_workspace` · 구조 op 의
창 연산 · 그 뒤 라우터·게이트·pump.

실측(2026-09-22, 착수 트리 `153566489`). ADR-0470 과 같은 범위(`src/adapters/ipc/**` ·
`src/boot/headless_dispatch.rs` · `src/app/ipc*`)에서 `fn` 인자 `&(mut )?AppState` 는 91 자리였다
(시험 도우미 7 포함). 남은 핸들러가 창에서 읽는 것은 여섯 갈래였다.

- **intent 큐** — 출하 코드의 `state.dispatch_intent(...)` 12 출현이 창 큐에 넣기만 했다
  (`surface.set_mark` · `surface.completion` · `surface.attention.clear` · `notification.create` ·
  `file_handler.dispatch` · `markdown.navigate` · `settings.set_remote_transfer`, 그리고 게이트의
  telemetry 상한·이상 탐지 알림, debug popup 둘과 `debug.settings.apply`).
- **로컬 사용자의 `active_workspace`** — 출하 코드의 `state.active_workspace` 읽기 13 출현(debug
  파일 둘 포함). 응답의 활성 표시(`workspace.list` ·
  `system.info` · `tree`), 권한·상한 게이트의 정책 스코프, telemetry·승인 기록의 기본 귀속.
- **구조 op 의 창 연산** — `workspace.close` 의 `close_workspace_at`, `workspace.create` 의 cwd
  상속과 생성 cascade, `workspace.update` · `workspace.move` 의 cascade, preset 적용, 그리고
  split · close · tab 이 이미 도메인 포트 `CascadeWindow` 로 넘기던 것.
- **호스트 이벤트 큐 · 최근 파일 · 승인 팝업** — `surface.fire_hook` · `recent.query` · 승인
  요청 발행(gui).
- **GUI·debug 전용 필드** — popup · 배너 · 도구 메뉴 · 파일 선택기 · modifier hint · `ui.state`.
- **`surface.parse_since_mark` 의 `read_since_mark`** — 대상 surface 를 반드시 받아 포커스로
  떨어지는 갈래는 부를 수 없었다.

## Decision

**IPC 엔진 핸들러는 `AppState` 를 받지 않는다. 창에 닿아야 하는 일은 좁은 포트
`IpcWindow` 와 요청 하나의 intent 출구 `IntentOutbox` 로만 한다.** 창 상태 자체가 대상인
GUI·debug 핸들러만 `AppState` 를 받고, 그것은 엔진 핸들러 표와 갈린 라우터가 넘긴다.

1. **intent 는 출구로** — 핸들러는 `IntentOutbox` 에 intent 를 넣는다. 진입점이 요청이 끝날 때
   출구를 창 큐 끝으로 옮긴다: 게이트(`record_telemetry_and_audit`) · 엔진 라우터
   (`dispatch_routed`) · `record_plugin_rss_samples`. 한 요청 안의 순서는 넣은 순서 그대로이고
   게이트가 낸 것이 핸들러가 낸 것보다 먼저다. 그래서 intent 만 내는 핸들러 일곱은 창을 아예
   안 받는다.
2. **창 연산은 포트로** — `IpcWindow`(`adapters/ipc/window_port.rs`)가 `CascadeWindow` 를 물려받아
   엔진 핸들러가 쓰는 창 연산을 선언하고, `AppState` 가 `state/ipc_window.rs` 에서 한 줄 위임으로
   구현한다: 활성 워크스페이스 index · cwd 상속 원본 · 워크스페이스 생성·메타·닫기·이동 cascade ·
   호스트 이벤트 · 최근 파일 · preset 적용 · 승인 팝업(gui) · 출구를 창 큐로 옮기기.
3. **`active_workspace` 기본값은 동작을 바꾸지 않는다** — ID 를 안 준 요청은 지금처럼 그 창의
   활성 워크스페이스로 간다. 달라진 것은 그 값을 `AppState` 전체가 아니라
   `IpcWindow::active_workspace_index` 로 읽는다는 것뿐이다.
4. **라우터가 둘로 갈린다** — `route_engine_handler` 와 `check_request` 는 `&mut dyn IpcWindow` 를
   받는다. 창 상태 자체가 대상인 `file_picker.trigger` 는 엔진 표에서 gui 전용
   `route_window_handler` 로 옮겼고, debug 핸들러는 원래대로 `route_debug_handler` 에 있다. 두
   창 라우터는 `AppState` 를 받는다.
5. **창을 쥔 자리는 `AppState` 를 받는다** — 라우터 진입점 `handle_checked_request` 와 헤드리스
   `pump_ipc`(와 plugin pump)는 창의 **소유자** 자리다: 요청의 출구를 그 창 큐로 옮기고, 창·debug
   라우터에 그 창을 넘기고, 헤드리스에서는 응답 전에 그 큐를 비워 적용한다(intent 적용이 창
   상태를 받는다 — [ADR-0111](0111-headless-drains-the-intent-queue.md)). 그 아래로는 포트만 내려간다.

## Consequences

- **얻은 것**: 같은 범위에서 `AppState` 를 받는 자리가 91 → 35 다(같은 판정기, 시험 도우미 포함).
  남은 35 는 창을 쥔 진입점 4(`handle_with_caller` 는 시험 전용) · 창·debug 라우터 2 · 창 상태 자체가
  대상인 GUI·debug 핸들러 19 · 헤드리스 pump 2 · 시험 도우미 8 이다. 엔진 핸들러 표와 공통 게이트
  아래에는 `AppState` 가 없다 — 핸들러가 창에서 무엇을 쓰는지는 `IpcWindow` 의 메서드 목록이
  답한다.
- **얻은 것**: 외부 동작이 안 바뀐다. 응답 · intent 의 종류와 적재 순서 · 대상 생략 시의 기본
  워크스페이스가 같다. 적재 순서는 `handler/intent_order_tests.rs` 가 고정하고, 그 시험은 이 결정
  이전 트리에서도 통과한다.
- **얻은 것**: 포트가 드러낸 죽은 갈래를 지웠다. `AppState::read_since_mark` 의 포커스 폴백은 부르는
  자리가 없었다(`surface.parse_since_mark` 는 surface 를 반드시 받는다). 대상을 준 읽기는
  `CoreState::read_since_mark_of` 로 옮기고 메서드와 `state/mark.rs` 를 지웠다.
- **잃은 것 — 원칙 3 과의 긴장이 타입 안에 그대로 있다.** `active_workspace_index` 는 로컬 사용자의
  포커스를 읽는다. 에이전트 대면 경로에서 그 읽기가 **보고**(활성 표시)인지 **귀속**(기록·게이트의
  기본 워크스페이스)인지는 `crates/tasty-doc-guards/tests/agent_facing_reads_of_active_state_are_classified.rs`
  가 파일마다 명부로 분류하고, 포트 메서드 이름이 바늘 `active_workspace` 를 담으므로 그 분류는
  그대로 세어진다(대상을 활성 상태로 **고르는** `OpenDefect` 는 0). 대상을 생략한 요청의 기본값을
  어떻게 할지 — 포커스 독립 원칙에 맞춰 지목을 요구할지 — 는 이 결정이 **정하지 않았다.** 그 재결정은
  별도 작업이다.
- **잃은 것**: `pump_ipc` 와 `handle_checked_request` 는 `&Core` 로 내려가지 않았다. ADR-0470 이 적은
  "라우터 · `check_request` · `pump_ipc` 를 `&Core` 로" 중 라우터 표와 게이트만 그렇게 됐다. 창을 쥔
  자리는 창을 받아야 한다 — 출구를 옮길 창 큐와, 창 상태를 조작하는 debug 핸들러가 거기 있다.
  그래서 ADR-0355 잔여 ① 은 엔진 핸들러 층만 닫혔고, 진입점은 헤드리스 intent 적용
  (`drain_pending_intents` · `drain_pending_host_events`)이 창을 받는 동안 열려 있다 — 아래 재검토 조건
  "헤드리스 intent 적용이 창 상태 없이 돌게 되면" 이 풀리면 닫는다.
- **운영 비용**: 새 엔진 핸들러가 창에서 새 것을 읽어야 하면 `IpcWindow` 에 메서드를 더하고
  `state/ipc_window.rs` 에 한 줄 위임을 쓴다. 창 상태 자체를 조작하는 새 핸들러는 엔진 표가 아니라
  창·debug 라우터에 등록한다. intent 를 내는 새 핸들러는 `&mut IntentOutbox` 를 받는다.

## Alternatives Considered

- **라우터가 intent 를 반환값으로 돌려주고 호출자가 적재한다**: 브리핑이 든 방향이다. 안 고른
  이유는 둘이다. `handle_checked_request` 의 호출자가 GUI 라우팅 · 헤드리스 pump · plugin pump ·
  intent cascade · 원격 forward 시험까지 여럿이라 적재를 잊는 자리가 곧 intent 유실이 되고 그것은
  조용하다. 그리고 [action-dispatch](../design/flows/action-dispatch.md) 가 "처리 핸들러가 intent 를
  리턴 경로로 흘려보내면 금지" 라 적는다. 출구를 요청 안에서 만들고 진입점이 창 큐로 옮기면 반환
  경로가 없고 적재 자리가 셋으로 고정된다.
- **`IpcWindow` 에 `dispatch_intent` 를 둔다**: 가장 작은 변경이다. 안 고른 이유: intent 만 내는
  핸들러가 창 포트 전체를 받게 되어 "이 핸들러는 창을 안 읽는다" 를 시그니처가 말하지 못한다.
- **GUI·debug 핸들러도 포트 뒤로 보낸다(포트에 `AppState` 접근자를 둔다)**: 라우터 진입점까지
  `AppState` 가 없어진다. 안 고른 이유: 그 접근자는 포트를 우회하는 문이라 gui 빌드에서는 어느
  핸들러든 창 상태를 다시 받을 수 있고, 이 결정이 준 보증이 이름만 남는다. 창 상태 자체가 대상인
  핸들러는 창을 받는 것이 참이다.
- **`pump_ipc` 도 포트로 받는다(드레인을 포트 메서드로 감싼다)**: 시그니처에서 `AppState` 가 사라진다.
  안 고른 이유: 드레인은 intent 를 창 상태에 적용하는 일이라 그 메서드는 포트가 아니라 창 전체를
  부른다. 시그니처가 사실과 다른 것을 말하게 된다.
- **`active_workspace` 대신 지목을 요구한다**: 원칙 3 에 가장 맞다. 안 고른 이유: 대상 생략 시 응답이
  바뀌는 외부 동작 변경이고, 이 결정의 범위(시그니처)가 아니다. 호환이 가장 많이 보존되는 쪽을 골랐다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 엔진 핸들러가 창에서 새 것을 읽어야 한다 — 그 핸들러는 `AppState` 가 없어 컴파일이 깨진다.
  `IpcWindow` 메서드를 더할지, 도메인(`Core`/`CoreState`)에 둘지, 창 라우터로 옮길지 그때 정한다.
- 에이전트 대면 경로가 활성 포인터로 **대상을 고르면** 명부 가드가 `OpenDefect` 로 잡는다.
- intent 적재 순서가 바뀌면 `handler/intent_order_tests.rs` 가 깨진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 대상 생략 시 기본 워크스페이스를 재결정한다 — 그때 `active_workspace_index` 의 에이전트 대면
  호출자와 이 ADR 의 "잃은 것" 첫 항을 다시 본다. 재는 법: `git grep -n active_workspace_index -- src/adapters/ipc`.
- 헤드리스 intent 적용이 창 상태 없이 돌게 되면(intent 핸들러가 `AppState` 대신 포트를 받게 되면)
  `pump_ipc` 가 포트로 내려갈 수 있다. 재는 법: `src/intent/headless.rs` 의 `route_non_domain` 이
  부르는 도메인 핸들러의 인자 타입.
- `AppState` 를 받는 자리가 다시 는다. 재는 법: 주석·문자열을 덮은 사본(`mask-source`)의 위 범위
  (`src/adapters/ipc/**` · `src/boot/headless_dispatch.rs` · `src/app/ipc*`)에서 인자 `[A-Za-z_][A-Za-z0-9_]*: &(mut )?([A-Za-z_][A-Za-z0-9_]*::)*AppState\b` 를
  센다 — 경로 한정 표기(`&crate::state::AppState`)도 세야 한다. 늘어난 자리가 창을 쥔 진입점 ·
  창·debug 라우터 · 창 상태 자체가 대상인 핸들러 중 하나인지 본다. 기록값: 이 결정 직후 35(출하 27 ·
  시험 8 — 그 "출하" 는 시험 전용 `handle_with_caller` 를 담고 있어, 갈라 세면 출하 26). 2026-09-22
  위 패턴 실측 40 = **출하 26 · 시험 14**(클로저 인자 2 포함, 둘 다 시험). 출하는 그대로이고 는 것은
  전부 시험이다. 경로 한정 표기만 잡히는 자리는 6 이고 전부 시험이다. 옛 패턴
  `이름: &(mut )?AppState` 로는 34 가 나와 그 6 을 놓친다.

## References

- [ADR-0355](0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md) — 이 결정이 엔진 핸들러 층을 닫는 잔여 ①(진입점은 열려 있다)
- [ADR-0470](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md) — 인자를 빼는 앞 걸음과 이 결정이 이은 남은 걸음
- [ADR-0533](0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md) — 이 결정이 정하지 않은 대상 생략 기본값의 자리별 재결정
- [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) — 이 포트가 물려받는 도메인 포트 `CascadeWindow`
- [ADR-0111](0111-headless-drains-the-intent-queue.md) — 헤드리스 pump 가 창 큐를 비우는 이유
- [AppState 필드 소유권](../dev-guide/app-state-ownership.md) · [포커스 정책](../design/policies/focus.md) · [Action Dispatch](../design/flows/action-dispatch.md)
- 결정이 실현된 현재 위치: `IpcWindow` · `IntentOutbox`(`src/adapters/ipc/window_port.rs`) ·
  `impl IpcWindow for AppState`(`src/state/ipc_window.rs`) · `route_engine_handler` · `route_window_handler` ·
  `dispatch_routed` · `check_request` · `CoreState::read_since_mark_of`
