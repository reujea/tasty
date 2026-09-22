# ADR-0355: AppState 의 소유권은 둘째 struct 가 아니라 gui 경계로 가른다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: headless, app-state, ownership, popup, feature, adr-0346
- **Group**: architecture

## Context

`AppState` 는 창 하나의 상태이고, 그 안에 도메인 사실·사용자 view 상태·실행 자원이 함께 산다.
headless 빌드(`--no-default-features`)도 같은 struct 를 쓴다. 그래서 headless 가 popup 입력
버퍼 같은 view 상태를 들고 다니는지, 즉 "headless 는 자기가 필요한 상태만으로 구성되는가"
가 물음이 된다.

그 물음에 답하는 방법은 둘이다. `AppState` 를 도메인 struct 와 GUI struct 둘로 가르거나,
한 struct 를 두고 GUI 소유 필드를 `cfg(feature = "gui")` 로 빼는 것이다.
필드 21 개는 이미 뒤쪽 방법으로 빠져 있었고, 그 판정 규칙이
[ADR-0346](0346-headless-compiles-only-what-it-reaches.md) 이다. 그러나 popup·메뉴·드래그 상태를 모은 `dialogs: DialogState` 는 gate 없이 남아 있었다.

실측(2026-09-21, 이 결정 직전 트리):

- `&mut AppState` 또는 `&AppState` 를 인자로 받는 자리가 `src/adapters/ipc`·`src/intent`·
  `src/core` 에 287 곳, 73 파일이다. headless 의 `pump_ipc` 는 이 인자를 그대로 핸들러
  (`check_request` · `handle_checked_request`)와 headless drain 에 넘긴다.
- `dialogs` 를 gate 하면 headless 라이브러리에서 깨지는 자리가 7 곳이었다. picker 를 여는 둘
  (`open_picker` · `open_remote_placeholder_picker`)과 overlay 판정 셋, 그리고 사용자 origin 에서만
  세워지는 요청(preset 저장 뒤 PresetView 열기) 두 줄이다. overlay 판정 하나
  (`has_input_dialog_open`)만 headless 에 독자가 있었다 — `ui.state` debug 덤프다.
- `DialogState.pending_approval_ids` 는 도메인 큐처럼 보이지만 아니다. 넣는 자리
  (`enqueue_approval`)가 gui 전용이고 비우는 자리는 approval popup 뿐이다. approval 레코드
  자체는 `Core` 의 approval 저장소에 있다.

## Decision

`AppState` 를 둘로 가르지 않는다. **GUI 가 소유하는 상태는 모듈 경계와 `cfg(feature = "gui")`
경계를 함께 준다.** `DialogState` 와 그 필드만 쓰는 자료형은 `src/state/dialogs.rs` 한 모듈에
모으고, 그 모듈 선언과 `dialogs` 필드에 gui 경계를 둔다. headless 빌드에는 그 상태가 아예
없다.

`DialogState` 를 가르지도 않는다. 안에 도메인 사실이 하나도 없기 때문이다.
`pending_approval_ids` 를 포함해 모든 필드의 세우는 쪽과 비우는 쪽이 GUI 다.

`pump_ipc` 의 인자 타입은 바꾸지 않는다. headless 에서 그 `AppState` 는 gui 필드가 빠진
형태이고, 좁은 타입을 따로 만들려면 위 287 자리의 핸들러 시그니처를 함께 바꿔야 한다.

필드마다 무엇이고 얼마나 사는지, headless 에 있는지는
[AppState 필드 소유권](../dev-guide/app-state-ownership.md) 표가 적는다.

## Consequences

- **얻은 것**: headless 빌드가 popup 입력 상태를 들고 다니지 않는다. 그 사실은 소스를 읽어서가
  아니라 컴파일로 확인된다 — headless 코드가 `dialogs` 를 읽으면 그 빌드가 깨진다.
- **얻은 것**: IPC 응답 형태·CLI 출력·plugin wire 가 하나도 안 바뀐다. `ui.state` debug
  덤프가 찍는 `gate_input_dialog_open` 과 `keyboard_shortcuts_gated` 는 headless 에서 전과
  같은 값(`false`)을 답한다.
- **잃은 것**: 핸들러는 여전히 `AppState` 전체를 받는다. "이 핸들러는 도메인 상태만 만진다"
  를 타입이 말해 주지 않는다 — gui 조합에서는 핸들러가 view 상태에도 닿는다.
- **잃은 것**: headless 테스트 구성에서 테스트 두 개가 빠진다. picker 행을 만드는
  `picker_lists` 와 원격 placeholder picker 를 보는 테스트다. 둘은 gui 조합에서 그대로 돈다.
- **드러난 것**: headless 가 부팅할 때 picker 선택 기록 파일을 더는 읽지 않는다. 그 값을 읽는
  picker 와 기록하는 `record_file_handler_pick` 이 이미 gui 에만 있었다. 기록이 깨진 파일이면
  나던 경고 로그 한 줄이 headless 에서 사라진다.
- **잔여 — 이 결정은 부분 착지다.** "headless 는 자기가 필요한 상태만으로 구성된다" 는
  `dialogs` 덩어리에서만 성립한다. 남은 셋은 도메인 실행을 `AppState` 없이 돌리는 headless
  core 라이브러리를 세우는 작업이 선행 조건으로 흡수한다.
  1. `pump_ipc` 와 핸들러가 `AppState` 전체 대신 도메인 상태만 받게 인자를 좁힌다.
  2. 소유권 문서의 `독자 없음` 필드 31 개를 ①(gui 전용)과 ③(headless 도 쓰지만 읽는 자가
     GUI 뿐 — `expect`)으로 가른다.
  3. `src/state.rs` 첫머리의 모듈 단위 `allow(dead_code, unused_imports)` 를 지운다. 2 가 끝나야
     지울 수 있다.

  **잔여의 현재 상태 (후속 — 도메인 경계 작업, [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md), 2026-09-21)**:
  - ②·③ 은 끝났다. `독자 없음` 필드 31 개를 ① 29 · ② 1(`tab_bar_height` — headless 테스트가
    읽는다) · ③ 1(`preset_store`)로 갈랐고, `src/state.rs` 의 모듈 단위 `allow` 를 지웠다. 필드가
    아닌 dead 정의(하위 모듈 · 메서드 · 자료형 · `ModalKind` 의 variant)도 같은 세 갈래로 항목마다
    가렸다. 그 `allow` 는 진단만 덮은 것이 아니라 dead 판정의 **뿌리** 노릇도 했다 — 지우자
    `core` 쪽 다섯 자리(`set_category_collapsed` · `reify_plugin_surface` · `SurfaceCwd` 와 그
    `as_str` · `SurfaceKindDef::convert_input_popup`)가 새로 dead 로 드러났고, 같은 규칙으로 갈랐다.
    headless lib 테스트 수는 그대로다(②). 분류표는 [AppState 필드 소유권](../dev-guide/app-state-ownership.md).
  - ① 은 도메인 쪽만 끝났다 — 구조 실행·cascade·forward runner(`core::structural_exec` ·
    `core::structural_cascade` · `core::attach_runtime`)가 `AppState` 대신 도메인이 선언한 포트
    `CascadeWindow` 를 받는다. `pump_ipc` 와 IPC 핸들러의 인자(위 287 자리)는 그대로 `AppState` 다.
    ADR-0440 이 도메인 크레이트를 떼지 않기로 해서 이것은 **크레이트의 선행 조건은 아니게 됐다.**
    그러나 이 ADR 의 목표 — "이 핸들러는 도메인 상태만 만진다" 를 타입이 말하는 것(Consequences
    "잃은 것" 첫 항) — 로는 그대로 남는다. 다음 주인은 IPC 핸들러와 `pump_ipc` 의 인자를 좁히는
    후속 단위다. 아래 재검토 조건 "`독자 없음` 칸이 비면" 이 이 착지로 충족됐고, 그 조건이
    가리키는 물음이 바로 이것이다.
  - **① 의 IPC 쪽 (후속 — [ADR-0470](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md), 2026-09-22)**:
    창 상태를 읽지 않는 IPC 핸들러에서 `AppState` 인자를 뺐다. IPC 범위에서 `AppState` 를 받는
    자리가 253 → 91 이고, 안 읽는 자리는 0 이다. 이제 "이 핸들러는 창 상태를 안 만진다" 를
    시그니처가 말한다 — "잃은 것" 첫 항은 **창 상태를 실제로 읽는 핸들러에만** 남는다. 라우터 ·
    `check_request` · `pump_ipc` 는 그 핸들러들에게 넘겨야 하므로 아직 `AppState` 를 받는다. 남은 91
    자리의 내용(intent 큐 · 로컬 사용자의 `active_workspace` 기본값 · GUI·debug 모듈 · 구조 op 의 창
    연산)은 ADR-0470 이 적는다. ADR-0470 시점의 남은 걸음은 intent 큐를 `AppState` 밖으로 빼고,
    `active_workspace` 기본값을 정하고, 구조 op 의 창 연산을 포트로 뺀 뒤 라우터 · `check_request` ·
    `pump_ipc` 를 `&Core` 로 내리는 것이었다(ADR-0470 Decision 의 남은 걸음). 그 걸음이 어디까지
    착지했는지는 바로 아래 항목이 적는다.
  - **① 의 부분 닫힘 (후속 — [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md), 2026-09-22)**:
    **엔진 핸들러 층은 닫혔다. `pump_ipc` · `handle_checked_request` 등 진입점은 헤드리스 intent 적용
    (`drain_pending_intents` · `drain_pending_host_events`)이 창을 받는 동안 열려 있다** — ADR-0471 의
    재검토 조건 "헤드리스 intent 적용이 창 상태 없이 돌게 되면" 이 풀리면 닫는다.
    IPC 엔진 핸들러와 공통 게이트는 `AppState` 를 받지 않는다. intent 는 요청 하나의 출구에 모였다가
    진입점이 창 큐로 옮기고, 창 연산(대상 생략 시의 `active_workspace` 포함)은 좁은 포트 `IpcWindow` 로
    읽는다. 같은 범위에서 `AppState` 를 받는 자리가 91 → 35 이고, 남은 것은 창을 쥔 진입점
    (`handle_checked_request` · 헤드리스 `pump_ipc`) · 창 상태 자체가 대상인 GUI·debug 핸들러와 그 라우터 ·
    시험 도우미다. **"이 핸들러는 도메인 상태만 만진다" 를 타입이 말한다** — 엔진 핸들러의 시그니처에
    `AppState` 가 없고, 창에서 쓰는 것은 포트 메서드 목록이 답한다. 그래서 "잃은 것" 첫 항은 엔진 핸들러
    층에서 닫혔다. `pump_ipc` 는 `&Core` 가 아니라 창의 소유자로 `AppState` 를 받는다 — 그 이유와 재검토
    조건은 ADR-0471.
  - **① 의 입구 본문 (후속 — ADR-0471 "입구 본문 봉인과 창 라우터의 호출자", 2026-09-23)**:
    `handle_checked_request` 아래 입구 본문은 `EntryWindow` 만 받아 창 상태를 이름으로 못 부른다.
    `pump_ipc` 는 같은 이유(intent 적용이 창을 받는다)로 열려 있다. 그리고 이 잔여가 겨냥한 것은
    **창 상태(`AppState`) 축**이다 — 원칙 2.1 ① 이 지키는 사용자 상태 중 포커스 pane · 활성 탭 ·
    포커스 surface · 닫은 항목 히스토리 · 스크롤·선택은 `CoreState` 에 있어, 이 잔여가 닫혀도
    그쪽은 막히지 않는다. 그쪽의 방어선은 intent 의 origin 판정이다.
- **운영 비용**: 새 dialog 상태를 넣는 사람은 그것이 headless 에서 읽혀야 하는지를 정해야 한다.
  읽혀야 하면 `dialogs` 가 아니라 `AppState` 나 `CoreState` 에 둔다.

## Alternatives Considered

- **`AppState` 를 도메인 struct 와 GUI struct 로 가른다**: 티켓 문장에 가장 가깝다. 그러나 287
  자리의 핸들러 인자를 바꾸는 변경이고, 같은 회차에 IPC 핸들러 시그니처를 고치는 다른 단위와
  파일이 겹친다. `dialogs` 덩어리에 대해서는 cfg gate 만으로도 같은 결과다 — headless 에서 그
  상태가 사라진다. 소유권 문서의 `독자 없음` 필드까지 빼는 것은 두 방법 어느 쪽으로도 필드마다
  가르는 일이 따로 필요하다.
- **`DialogState` 를 가르고 `pending_approval_ids` 만 gate 밖에 남긴다**: 그 큐를 headless
  에서 채우는 자리도 비우는 자리도 없다. 남기면 headless 에 독자 없는 필드를 하나 더 만든다.
- **`src/state.rs` 의 모듈 단위 `allow(dead_code)` 를 이 회차에 지운다**: 지우고 재면 headless
  라이브러리에서 dead_code 진단 27 건이 나온다(이 결정을 적용한 트리, 2026-09-21). 그중 하나가
  `AppState` 의 필드 31 개를 한데 묶은 것이다. 경계를 제대로 긋는 길이지만 이
  단위의 범위(`dialogs`)보다 넓어 다음 단계로 둔다. 그 목록은 소유권 문서의 `독자 없음` 칸이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- headless 코드가 `dialogs` 나 그 자료형을 읽으면 headless 컴파일이 깨진다. 그 시점에 그
  상태를 `dialogs` 밖으로 옮길지, 그 자리를 gui 로 가릴지 정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 도메인 실행이 `AppState` 없이 도는 headless core 라이브러리를 세우게 되면 핸들러 인자가
  갈려야 한다. 그때는 struct 분리가 필요하다. 재는 법: 핸들러가 `AppState` 에서 읽는 필드를
  모아 그것이 전부 소유권 문서의 `읽힘` 칸 안에 드는지 대조한다.
- 소유권 문서의 `독자 없음` 칸이 비면 이 ADR 의 "잃은 것" 첫 항을 다시 본다. 재는 법: 그
  문서의 "재는 법".

## References

- [AppState 필드 소유권](../dev-guide/app-state-ownership.md) — 필드 분류표
- [헤드리스 정의 경계](../dev-guide/headless-build-boundaries.md) — gui 전용 판정 규칙
- [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) — 이 결정이 따르는 경계 규칙
- [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) — 잔여 ① 의 도메인 쪽을 맡은 결정
- [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) — 잔여 ① 의 엔진 핸들러 층을 닫은 결정(진입점은 열려 있다)
- 결정이 실현된 현재 위치: `src/state/dialogs.rs`, `AppState::dialogs`, `AppState::has_input_dialog_open`
