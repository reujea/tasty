# ADR-0502: 에이전트가 만든 탭은 사용자가 보던 탭을 바꾸지 않는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: focus, tab, ipc, cli, user-agent-separation, identity, attach, file-handler, adr-0302, adr-0497

## Context

새 탭을 pane 에 붙이는 도메인 인텐트는 `DomainIntent::CreateTab` 하나이고, 본문은
`Core::apply_create_tab` 이다. 이 결정 전에는 그 본문이 kind 로 선택을 갈랐다.

- terminal 은 `Pane::add_terminal_marker_tab_background` 로 붙였다 — 활성 탭 불변.
- 그 밖의 kind(markdown · html · explorer · empty · plugin kind)는 `Pane::add_surface_tab` 으로
  붙였고, 그 함수가 `active_tab` 을 새 탭으로 옮겼다.

그 인텐트로 들어오는 진입점은 넷이다.

- IPC `tab.create`(CLI `tasty new tab`) — 에이전트.
- `Intent::NewTab` 핸들러 — 사용자(탐색기 단축키 · 도구 메뉴 · 파일 열기의 origin 없는 갈래)와
  에이전트(`file_handler.dispatch` 의 origin 없는 갈래)가 함께 쓴다.
- 파일 디스패치의 origin 갈래(`open_surface_tab`) — 이미 `FileDispatchOrigin` 으로 갈라
  에이전트면 인텐트를 적용한 **뒤에** 옛 `active_tab` 을 되돌려 놓고 있었다
  ([ADR-0302](0302-a-user-file-open-selects-its-result-tab.md)).
- attach forward 의 `StructuralOp::NewTab` — 원격 사용자 또는 원격 에이전트
  (`ForwardOrigin`).

그래서 에이전트가 `tasty new tab --type html` 을 부르면 그 pane 의 활성 탭이 새 탭으로 바뀌었고,
그 pane 이 포커스를 쥔 pane 이면 focused surface 도 따라 옮겨 갔다(focused surface 는 활성 탭에서
파생된다). 응답의 `active_tab` 도 새 탭을 가리켰다. 사용자가 보던 탭이 에이전트 행동으로 바뀐
것이고, [정체성 원칙](../identity.md) 1(에이전트 행동의 부수효과가 사용자 포커스에 닿지 않는다)과
3(포커스 독립성)을 어긴다. terminal 만 background 였던 것은 원칙이 아니라 우연이다 — 사용자의 새
터미널 탭은 이 인텐트가 아니라 `AppState::add_tab` 이 열고, 그 함수는 스스로 선택한다.

창 단위의 같은 문제를 [ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) 이
"그 창은 누가 원했나" 로 갈랐다. 탭도 같은 물음이다.

## Decision

**탭을 만든 주체가 선택을 정한다. 그 값은 `DomainIntent::CreateTab` 의 `activate` 필드 하나다.**

- `activate: true` 면 새 탭이 그 pane 의 활성 탭이 된다(`Pane::add_surface_tab`).
  `false` 면 뒤에 붙기만 한다(`Pane::add_surface_tab_background`).
- terminal kind 는 `activate` 와 무관하게 종전대로 background 다.
- 값은 진입점마다 정한다.
  - IPC `tab.create`: 언제나 `false`.
  - `Intent::NewTab`: 언제나 `true`. 이 인텐트는 focused pane 에 붙는 사용자 동작의 것이고
    (정의의 doc 이 그렇게 말한다), 에이전트 라벨로 들어오는 발화점은 origin 없는
    `file_handler.dispatch` 하나다. 그런데 markdown plugin 의 파일열기 팝업 — 사용자의
    `open_markdown` 단축키가 여는 것 — 이 바로 그 호출(`path`·`depth` 만, origin 없음)로 새 탭을
    연다. host 는 그 호출이 에이전트인지 plugin 을 거친 사용자 조작인지 가를 값이 없다(ADR-0302
    와 같은 한계). origin 으로 가르면 사용자가 연 markdown 탭이 선택되지 않는다.
  - 파일 디스패치 origin 갈래: `FileDispatchOrigin::selects_result()`. 적용 뒤 되돌리던 코드는
    지운다 — 같은 결정이 인텐트 안으로 들어왔다.
  - attach forward `NewTab`: `ForwardOrigin::User` 일 때만 `true`. 복원 스택을 가르는
    `restorable` 과 같은 축이다.

**mirror(원격 attach) pane 위에서는 선택을 서버가 forward origin 으로 정한다.** 클라이언트가
mirror pane 에 낸 `CreateTab` 은 `StructuralOp::NewTab` 으로 바뀌어 서버로 가는데, 그때
`activate` 는 실리지 않는다 — 서버는 `ForwardOrigin`(클라이언트의 `user_triggered`, 곧 발화한
인텐트의 origin 이 사용자인가)만 보고 위 attach forward 규칙대로 정한다. 그래서 origin 없는
`file_handler.dispatch` 가 mirror pane 에 연 탭은 로컬 pane 에서와 달리 서버에서 선택되지
않는다. 원칙 1 쪽으로 기우는 차이라 그대로 두고, wire(`StructuralOp::NewTab`)에 `activate` 를
싣지 않는다.

**응답의 `active_tab` 의미는 바꾸지 않는다.** 종전에도 그 값은 "생성 뒤 그 pane 의 활성 탭
인덱스" 였고, 지금도 그렇다. 달라진 것은 값이다 — 에이전트가 만든 탭이면 사용자가 보던 탭의
인덱스가 나온다. 새 탭의 인덱스가 필요한 호출자는 `tab_count - 1`(탭은 언제나 뒤에 붙는다)이나
`surface_id` 를 쓴다.

**개정하지 않는 것**: ADR-0302 의 사용자/에이전트 분류(어느 발화점이 `User` 인가)와 ADR-0279
의 라우팅(어느 pane 에 붙는가)은 그대로다. 이 결정은 선택 한 축만 인텐트 안으로 옮긴다.

## Consequences

- **얻은 것**:
  - 에이전트가 markdown · html · explorer · plugin 탭을 열어도 사용자가 보던 탭과 포커스가 그대로다.
  - 사용자 경로(탐색기 단축키 · 도구 메뉴 · 사용자 파일 열기 · 원격 사용자의 새 탭)는 종전대로
    새 탭을 선택한다.
- **잃은 것**:
  - origin 없이 온 `file_handler.dispatch` 는 에이전트가 불러도 종전대로 새 탭을 선택한다.
    ADR-0302 가 origin 갈래에서 세운 "an agent request does not select" 가 이 갈래에서는 아직
    성립하지 않는다. 막으려면 plugin 이 사용자 조작임을 실어 보낼 채널이 먼저 있어야 한다.
  - `tab.create` 응답의 `active_tab` 을 "새 탭의 인덱스" 로 읽던 호출자는 다른 값을 받는다.
    레포 안의 소비자는 그렇게 읽지 않았다(통합 테스트는 `surface_id` 를 쓴다).
  - 에이전트가 만든 비터미널 탭은 사용자가 고를 때까지 렌더되지 않는다. 렌더 경로를 재야 하는
    시험(explorer 우클릭 GUI 시험)은 debug `debug.switch_tab` 으로 사용자 전환을 재현한다.
  - 메모리 soak 의 S6(`tests/soak_memory.rs` `cycle_plugin_view_churn`)은 `tab.create` 로 연
    탭을 닫기만 하므로, 그 탭이 선택·렌더되지 않는 지금은 view store `drop_view` · egui-mesh
    retain 경로를 타지 않는다 — 그 누수를 더는 재지 못한다. release soak 에는 탭을 고르는 API 가
    없어(원칙 3) GUI 시험처럼 `debug.switch_tab` 으로 메울 수 없다. 시나리오 재설계가 필요하다
    (별도 작업).
- **운영 비용 / 유지 부담**: `CreateTab` 을 새로 만드는 자리는 `activate` 를 정해야 한다 —
  필드가 필수라 빠뜨리면 컴파일이 멈춘다.

## Alternatives Considered

- **파일 디스패치처럼 호출자가 적용 뒤 `active_tab` 을 되돌린다**: 진입점마다 같은 보정을
  반복해야 하고, 빠뜨린 진입점은 조용히 선택을 옮긴다. 이 결함이 정확히 그 형태였다.
- **비터미널도 언제나 background 로 붙이고 사용자 경로가 따로 선택한다**: 사용자 진입점이
  넷으로 흩어져 있어 같은 반복이 반대편에 생긴다.
- **`Intent::NewTab` 도 origin 으로 가른다**: 원칙에는 더 맞지만, 그 에이전트 라벨이 실제로
  싣는 것이 사용자의 markdown 파일열기라 사용자 경로가 회귀한다. 티켓의 조건("사용자 경로는
  종전대로")을 깬다.
- **terminal 도 `activate` 를 따르게 한다**: 원격 사용자가 mirror 에서 연 터미널 탭이 서버 쪽에서
  선택되는 등 이 티켓 밖의 동작이 바뀐다. 사용자 터미널 탭은 이 인텐트를 안 거치므로 얻는 것이
  없다.
- **`active_tab` 을 새 탭의 인덱스로 바꿔 싣는다**: 필드 이름이 "그 pane 의 활성 탭" 이라
  그 값을 싣는 것이 거짓이 된다.

## Reconsideration Triggers

**채널이 붙는 것**

- `file_handler.dispatch` 가 사용자 조작임을 실을 값(또는 markdown 파일열기 팝업이 그 호출
  대신 쓸 사용자 경로)이 생기면 `Intent::NewTab` 의 `activate` 를 origin 으로 가른다. 이 조건을
  재는 가드는 없다 — `file_handler.dispatch` 의 요청 구조체 필드와 markdown plugin 의 파일열기
  호출을 읽는다. 지금의 선택 값은
  `intent::headless::tests::an_agent_labelled_new_tab_still_selects_it` 가 고정한다.
- 사용자의 새 터미널 탭이 `AppState::add_tab` 대신 `DomainIntent::CreateTab` 을 거치게 되면
  terminal 의 background 고정을 다시 본다 — 그때는 사용자 경로가 선택을 잃는다.
  `apply_create_tab` 의 terminal 갈래와 `add_tab` 호출자를 함께 읽는다.

**원리적으로 안 붙는 것**

- 에이전트가 "탭을 열고 사용자에게 보여 달라" 는 요구가 반복되면 선택을 옮기는 명시 옵션을
  검토한다(release 에 포커스 변경 API 를 두지 않는다는 [focus 정책](../design/policies/focus.md)
  과 충돌하는지부터). 재는 법: 그 요구를 담은 이슈·요청이 들어오는지 본다.

## References

- [ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) — 창 단위의 같은 결정
- [ADR-0302](0302-a-user-file-open-selects-its-result-tab.md) · [ADR-0279](0279-file-dispatch-retains-origin-through-completion.md)
- [focus 정책](../design/policies/focus.md)
- 코드 근거(결정이 실현된 현재 위치): `Core::apply_create_tab` · `structural_exec::create_tab` ·
  `crate::intent::tab::new_tab` · `open_surface_tab` · `execute_forwarded_structural_op` ·
  `Pane::add_surface_tab_background`
