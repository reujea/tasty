# ADR-0470: IPC 핸들러는 창 상태를 읽을 때만 `AppState` 를 받는다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: ipc, handler, app-state, ownership, headless, signature, adr-0355, adr-0440
- **Group**: architecture

## Context

[ADR-0355](0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md) 는
`AppState` 를 둘째 struct 로 가르지 않고 gui 경계로 갈랐다. 그 결정이 남긴 잔여 ① 은 "핸들러는
여전히 `AppState` 전체를 받는다 — '이 핸들러는 도메인 상태만 만진다' 를 타입이 말해 주지
않는다" 였다. 도메인 실행 쪽은 [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md)
의 포트 `CascadeWindow` 로 끝났고, 남은 것은 IPC 핸들러와 `pump_ipc` 였다.

실측(2026-09-22, 착수 트리 `81fd484b8`). 주석·문자열을 덮은 사본에서 `fn` 인자
`이름: &(mut )?AppState` 를 찾고 본문에서 그 이름이 어떻게 쓰이는지 셌다. 범위는
`src/adapters/ipc/**` · `src/boot/headless_dispatch.rs` · `src/app/ipc*` 다. 이 수는 옛 패턴(경로 한정
표기 `&crate::state::AppState` 미포함)으로 셌다 — 아래 재검토 조건의 재는 법 패턴은 그 뒤 넓혔다.

- 253 자리. 그중 **145 는 인자를 한 번도 안 읽었다**(이름이 `_state` 이거나 본문에 없다).
  55 는 통째로 다른 fn 에 넘기기만 했고, 53 은 필드·메서드를 직접 만졌다.
- 안 읽는 인자가 많은 이유는 라우터(`route_engine_handler`)가 모든 핸들러를 같은 모양
  `(core, state, engine, id, params)` 으로 부르던 관례다. 이미 일부는 그 모양을 벗어나
  있었다 — `workspace_category::handle_create(engine, …)`, `terminal::handle_children(engine, …)`.
- 직접 만지는 53 자리가 읽는 것: 로컬 사용자의 `active_workspace`(대상 생략 시 기본값) ·
  intent 큐(`dispatch_intent`) · GUI 전용 필드(`popups`·`banners`·`dialogs`·`modifier_hint`·
  `tool_registry` — debug·gui 모듈) · `ui.state` 덤프의 view 판정 · `recent_files` ·
  호스트 이벤트 큐. 그리고 `self` 를 안 읽는 `AppState` 메서드 둘(`read_output` ·
  `take_since_output_scan_mark`)과 `AppState` 가 든 Core memory 핸들 사본(`with_memory`).

## Decision

**핸들러의 인자는 그 핸들러가 닿는 상태를 말한다. 창 상태(`AppState`)를 읽지 않는 핸들러는
`AppState` 를 인자로 받지 않는다.** 둘째 struct 를 만들지 않고(ADR-0355 의 기각 그대로)
인자를 빼는 것으로 좁힌다. 그러면 "이 핸들러는 창 상태를 안 만진다" 를 컴파일러가 보증한다
— 본문이 창 상태를 읽으려면 시그니처부터 바꿔야 한다.

걸음은 셋이고 모두 동작을 바꾸지 않는다.

1. 안 읽는 인자를 뺀다. 넘기기만 하던 자리는 받는 쪽이 빠지면 안 읽는 자리가 되므로 고정점까지
   되풀이한다. 한 조합(gui 또는 headless)에서만 읽히는 인자는 남긴다.
2. `AppState` 가 든 Core memory 핸들 사본을 읽던 자리(`surface.meta.*` 넷과 `target_surface`
   의 nickname 해석, 그것을 쓰는 hard 점유 구조 가드)는 `Core` 로 읽는다. 두 운영 조립(GUI 창 ·
   headless 부팅)이 `AppState` 에 `Core::memory_arc` 의 같은 Arc 를 넘긴다.
3. `self` 를 안 읽는 `AppState` 메서드 둘을 `CoreState` 로 옮긴다(`core/state/output_read.rs`).
   포커스로 떨어지는 `read_since_mark` 는 로컬 사용자의 포커스를 읽으므로 `AppState` 에 남긴다.

**현재 상태 — 라우터 · `check_request` · `pump_ipc` 는 아직 `AppState` 를 받는다.** 이것은 결정이
아니라 선행 조건이 안 찬 상태다. 남은 핸들러가 실제로 창 상태를 읽어 라우터가 그것을 넘겨야 하고,
`check_request` 자신도 발화 주체의 기본 워크스페이스를 `active_workspace` 에서 읽는다. ADR-0355
잔여 ① 은 그래서 **열려 있다.** 그것을 닫는 남은 걸음은 이 순서다.

1. intent 큐에 넣기만 하는 핸들러의 큐를 `AppState` 밖으로 뺀다(동작 경계가 바뀌므로 이동 커밋과
   섞지 않는다).
2. 대상 생략 시 `active_workspace` 로 떨어지는 핸들러의 기본값을 어떻게 할지 정한다(포커스
   독립성 쪽 물음 — 새 ADR 이 필요하다).
3. GUI·debug 전용 모듈은 창 상태를 실제로 조작하므로 좁힐 대상이 아니다.
4. 구조 op 의 창 연산을 `CascadeWindow` 처럼 포트로 뺀다.
5. 1 · 2 · 4 가 끝나면 라우터 · `check_request` · `pump_ipc` 의 인자를 `&Core` 로 내린다.

### 남은 걸음의 착지 (후속 — [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md), 2026-09-22)

위 "현재 상태" 문단은 이 결정 시점의 사실이다. 남은 걸음은 ADR-0471 에서 이렇게 착지했다 — 1 · 2 · 4 는
끝났고 5 는 부분이다.

- 1(intent 큐) — 핸들러는 창 큐가 아니라 요청 하나의 출구 `IntentOutbox` 에 넣고, 진입점이 요청 끝에
  창 큐로 옮긴다. 적재 순서는 그대로다.
- 2(`active_workspace` 기본값) — 동작은 안 바꿨다. 값은 좁은 포트 `IpcWindow::active_workspace_index` 로
  읽는다. 기본값 자체의 재결정은 열려 있다.
- 4(창 연산) — `IpcWindow` 가 `CascadeWindow` 를 물려받아 엔진 핸들러가 쓰는 창 연산을 선언한다.
- 5 — 라우터 표(`route_engine_handler`)와 `check_request` 는 `&Core`·`&mut CoreState` 와 그 포트만 받는다.
  **`&Core` 만으로 내려가지는 않았다** — 창을 쥔 진입점 `handle_checked_request` 와 헤드리스 `pump_ipc` 는
  `AppState` 를 받는다. 요청의 출구를 옮길 창 큐와, 창 상태 자체가 대상인 창·debug 핸들러가 거기 있기
  때문이다(ADR-0471 Decision 5).
- 5 의 뒤 — 진입점 아래 입구 본문(`route_checked_request` · `dispatch_routed`)은 `AppState` 대신
  `EntryWindow` 를 받는다(ADR-0471 "입구 본문 봉인과 창 라우터의 호출자", 2026-09-23). 진입점과
  헤드리스 `pump_ipc` 는 여전히 `AppState` 를 받는다.
- Decision 3 이 `AppState` 에 남긴 `read_since_mark` 의 포커스 폴백은 부르는 자리가 없어 지웠다. 대상을
  준 읽기는 `CoreState::read_since_mark_of` 다.

## Consequences

- **얻은 것**: IPC 범위에서 `AppState` 를 받는 자리가 253 → 91 이다(같은 판정기, 2026-09-22).
  안 읽는 자리 145 → 0. 남은 91 은 직접 읽는 46 과, 그것을 부르려고 넘기는 45 다. memory ·
  agent 협업 · telemetry 조회 · approval 조회 · surface 조회·전송 · hook · message ·
  terminal tell · 구조 list 류 핸들러는 이제 `Core`/`CoreState` 만 받는다.
- **얻은 것**: IPC 응답 형태·CLI 출력·plugin wire 가 하나도 안 바뀐다. 모든 걸음이 인자 제거
  또는 이동이다.
- **남은 것 — 창 상태를 읽는 핸들러** (91 자리의 내용). 인자를 빼는 이 결정의 걸음으로는 안 좁혀지고,
  Decision 의 남은 걸음 1 · 2 · 4 가 먼저다.
  - intent 큐에 넣기만 하는 핸들러(`surface.set_mark` · `surface.completion` ·
    `surface.attention.clear` · `notification.create` · `file_handler.dispatch` · `markdown.navigate` ·
    `settings.set_remote_transfer` 등). 큐가 창마다 있다.
  - 대상을 생략하면 로컬 사용자의 `active_workspace` 로 떨어지는 핸들러(`workspace.list` 의
    `focused` 칸, `system.info`, `tree`, approval 요청·telemetry 기록의 기본 워크스페이스, image ·
    webview 의 대상 해석). 포커스 독립성 정책상 "조회는 허용, 의존은 금지" 의 경계에 있는 자리들이다
    — 이 결정은 그 동작을 안 바꾼다.
  - GUI·debug 전용 모듈(`debug*`, `tool`, `file_picker`)과 `ui.state` 덤프.
  - 구조 op 중 창 연산을 부르는 것(`workspace.close` 의 `close_workspace_at` ·
    `workspace.create` 의 cwd 상속 · 구조 intent 를 거는 close·split·tab 경로).
- **잃은 것**: 라우터의 호출 모양이 한 가지가 아니다. 핸들러마다 인자 목록이 다르므로 새
  핸들러를 등록하는 사람은 그 핸들러가 무엇을 받는지 보고 부른다.
- **운영 비용**: 새 핸들러는 필요한 상태만 받는다. 창 상태가 필요 없는 핸들러에 `_state` 를
  달지 않는다 — 그렇게 달면 컴파일러가 아무 말도 안 하고 이 결정이 준 보증이 그 자리에서 사라진다.
  이것을 막는 가드는 두지 않았다(아래 재검토 조건).

## Alternatives Considered

- **`AppState` 를 도메인 struct 와 창 struct 로 가른다**: ADR-0355 가 기각한 안이다. 이번
  실측으로 그 필요가 더 줄었다 — 안 읽는 인자를 빼는 것만으로 IPC 범위의 절반 넘게가 창
  상태에서 떨어진다.
- **intent 큐만 쓰는 핸들러에 큐 하나(`&mut Vec<DispatchedIntent>`)만 넘긴다**: 시그니처가 "큐에
  넣기만 한다" 를 말하게 된다. 안 고른 이유: 큐는 창 소유이고 요청이 어느 창의 큐에 들어가는지가
  라우팅의 일이라, 큐를 따로 떼어 넘기는 모양은 창 상태를 받는 것과 소유 관계상 다르지 않다.
  **이번 회차 범위 밖(다음 걸음)이다** — 기각이 아니다. 큐를 창 밖으로 옮기는 것은 인자 제거가
  아니라 동작 경계의 변경이라 이 결정의 "동작을 바꾸지 않는 걸음" 에 안 들어간다. Decision 의
  남은 걸음 1 이 이것이다.
- **`active_workspace` 를 읽는 핸들러의 대상 생략 갈래를 없앤다**: 포커스 독립성 쪽 물음이고
  외부 동작(대상 생략 시 응답)이 바뀐다. 이 결정은 인자의 모양만 다룬다.
- **`_state` 로 된 안 읽는 인자를 막는 가드를 둔다**: 한 번 없앤 형태가 다시 들어오는 입구를
  막는다. 이번에 안 둔 것은 판정기가 fn 인자의 사용 여부를 세야 해서 가드가 무거워지는 데 비해,
  입구가 새 핸들러 등록 한 자리뿐이기 때문이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 핸들러가 창 상태를 새로 읽어야 한다 — 그 핸들러의 시그니처에 `AppState` 가 없으면 컴파일이
  깨진다. 그때 인자를 더하는 것이 맞는지(창 상태인가) 도메인에 둘 것인지 정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 안 읽는 `AppState` 인자가 다시 늘어난다. 재는 법: 주석·문자열을 덮은 사본(`mask-source`)에서
  인자 `[A-Za-z_][A-Za-z0-9_]*: &(mut )?([A-Za-z_][A-Za-z0-9_]*::)*AppState\b` 중 이름이 `_` 로 시작하거나 본문에 안 나오는 것을 센다 — 경로 한정 표기
  (`&crate::state::AppState`)도 세야 한다(이 결정 직후 IPC 범위 0). 2026-09-22 실측: 이 패턴은
  IPC 범위에서 40 자리를 잡는다. 옛 패턴 `이름: &(mut )?AppState` 가 놓치던 경로 한정 표기 6 자리는
  전부 시험이고 전부 인자를 읽는다(나머지 34 자리의 읽힘은 이 실측에서 다시 재지 않았다). 그 수가 다시 쌓이면 가드를 둔다.
- Decision 의 남은 걸음(큐 분리 · `active_workspace` 기본값 · 창 연산 포트화)이 착지한다 — 그때
  라우터 · `check_request` · `pump_ipc` 를 `&Core` 로 내리고 위 "현재 상태" 문단을 고친다.
  재는 법: 같은 판정기로 IPC 범위의 access 자리를 모아 읽는 필드·메서드를 대조한다.

## References

- [ADR-0355](0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md) — 이 결정이 IPC 쪽을 좁힌 잔여 ①(부분 — 남은 걸음의 엔진 핸들러 층은 ADR-0471 이 닫았고 진입점은 열려 있다)
- [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) — Decision 의 남은 걸음 1 · 2 · 4 · 5 의 착지
- [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) — 도메인 쪽 포트
- [AppState 필드 소유권](../dev-guide/app-state-ownership.md) — 필드 분류표
- [포커스 정책](../design/policies/focus.md)
- 결정이 실현된 현재 위치: `route_engine_handler`(`src/adapters/ipc/handler.rs`) 의 핸들러 호출 모양 ·
  `CoreState::read_output` · `CoreState::take_since_output_scan_mark` · `pane::resolve_surface_target`
