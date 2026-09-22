# ADR-0503: 에이전트 intent 의 적용 실패는 사용자 toast 가 아니라 로그로 간다 — ADR-0401 의 구조 전달 실패 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: toast, attach, mirror, intent, origin, identity-principle-1, user-agent-separation, adr-0401
- **Group**: ui-theme-gallery

## Context

IPC 핸들러 일부는 일을 직접 하지 않고 intent 를 agent origin 으로 만들어 요청의 intent 출구에
넣는다(`from_agent_ipc()`). 메인 루프가 그것을 `src/intent/*` 의 도메인 핸들러로 처리한다. 그
핸들러들이 적용에 실패하면 사용자 toast 를 낼 수 있는 자리가 둘 있었고, 둘 다 origin 을 보지
않았다.

- **동기 차단** — `report_apply_error`(`src/intent.rs`). mirror 워크스페이스에서 forward 할 수 없는
  구조 op(`MirrorStructuralBlocked { forwarded: false }`)면 `attach.toast.mirror_structural_blocked`.
  preset 적용·저장 실패(`src/intent/preset.rs`)도 origin 없이 실패 toast 를 냈다.
- **비동기 실패** — forward 한 op 가 원격에서 실패하면 attach client 가
  `attach.toast.mirror_structural_forward_failed` 를 낸다(`src/app/attach_client.rs`
  `apply_one_mirror_event`). 이 toast 는 [ADR-0401](0401-remote-connection-events-may-raise-a-toast-without-a-user-action.md)
  이 "원격 연결 상태 사건" 부류의 구성원으로 허용했다.

교차 조사(결정 시점)에서 에이전트 origin intent 를 만드는 자리는 IPC `markdown.navigate`
(`Intent::ConvertSurface`) 와 `file_handler.dispatch`(`origin_surface_id` 없이)가 새 탭으로 떨어지는
갈래(`Intent::NewTab`)였다. 둘 다
mirror 워크스페이스에서 forward 된다. 그래서 동기 차단 toast 로 이어진 실례는 확인하지 못했지만,
**비동기 실패 toast 는 실례가 있다** — 원격이 그 op 를 적용하지 못하면(예: 원격에 등록되지 않은
surface kind) 에이전트의 IPC 한 번이 사용자 화면에 경고 toast 를 띄운다.

같은 원칙 위반이 intent 밖에 하나 더 있었다. ADR-0401 이 알려진 예외로 적은
`attach.toast.mirror_markdown_truncated` 는 원격 markdown 원문이 잘려 오면 원문 요청마다 난다.
요청하는 자리 넷 중 `markdown.reload` IPC(`tasty markdown reload --surface`)는 mirror 문서에 대해
바깥 호출자만 부른다 — mirror 문서는 idle 감시에 등록되지 않아 plugin 자신의 self_invoke 가 없다.
ADR-0401 은 그것을 "별도 결함 후보(원칙 1)" 로 남겼다.

ADR-0401 은 같은 문서 안에서 "에이전트 IPC 호출이 직접 일으킨 결과는 이 부류가 아니다" 라고도
적었다. 구조 전달 실패를 부류에 넣을 때 그 실패를 누가 일으켰는지는 가르지 않았다 — Context 가
"조작은 사용자가 했지만" 이라고 사용자 조작만 상정했다.

## Decision

`src/intent/*` 의 적용 실패 신호는 **사용자 origin 에서만 toast 가 된다.** 에이전트 origin 의
실패는 `warn` 로그로 끝난다.

- `report_apply_error` 는 origin 을 받는다. forward 할 수 없는 차단은 사용자 origin 에서만 차단
  toast 를 낸다. preset 적용·저장 **실패** toast 도 사용자 origin 에서만 낸다.
- 에이전트 origin 의 op 가 forward 되면 `report_apply_error` 가 그 큐 원소에 `silent_failure` 를
  표시한다(`mark_last_forward_agent_origin`). attach client 는 전송할 때 그 op_id 를 세션에 기억하고,
  그 op_id 의 실패 회신은 toast 대신 로그로 보낸다.
- **ADR-0401 의 개정 조항**: 허용 부류의 구성원 `mirror_structural_forward_failed` 는 이제
  **에이전트 origin intent 가 forward 한 op 의 실패를 포함하지 않는다.** 그것은 에이전트 IPC 호출이
  직접 일으킨 결과이고, ADR-0401 이 부류 밖이라고 적은 것에 해당한다.
- markdown plugin 은 `markdown.reload` 로 건 원문 요청에만 `agent_origin: true` 를 싣는다
  (`markdown_mirror.content_request`). host 는 그 request_id 를 세션에 기억하고, 그 회신이 잘려 와도
  toast 대신 로그로 남긴다. **ADR-0401 의 개정 조항 둘째**: 알려진 예외 `mirror_markdown_truncated`
  에서 `markdown.reload` 경로가 빠진다 — 최초 열기 · 변경 신호 재조회 · 새로고침 버튼의 잘림 toast 는
  그대로다.
- **개정하지 않는 것**: ADR-0401 의 나머지 — 허용 부류 자체, 다른 다섯 구성원(`mirror_reconnecting` ·
  `mirror_reconnected` · `mirror_reconnect_giveup` · `mirror_disconnected` · `mirror_desynced`),
  parked engine 게이트, 사용자 조작이 forward 한 op 의 실패 toast, 알려진 예외
  `mirror_markdown_truncated` 의 나머지 세 경로와 그것을 문서 안 표지로 옮길지의 결정 — 는 그대로다.
- 기본값은 종전 동작이다. origin 을 모르는 자리(`AppState::forward_mirror_structural` ·
  `src/file/dispatch.rs` `open_surface_tab` 의 `Some(pane)` 갈래 — `origin_surface_id` 를 실은
  `file_handler.dispatch` 가 `core.apply` 를 직접 부른다)가 쌓은 forward op 는 표시가 없고, 그
  실패는 종전대로 toast 가 된다. `Core::apply` 를 intent 없이 직접 부르는 IPC 구조 핸들러는 이
  자리가 아니다 — 아래 "보강" 절.

IPC 응답은 바뀌지 않는다. 이 intent 들은 적용 전에 요청이 응답을 돌려주는 형태라(`accepted`), 적용
실패의 사유를 응답에 실을 자리가 없다. 사유는 로그에 남는다.

## Consequences

- **얻은 것**: 에이전트가 mirror 워크스페이스에 `markdown.navigate` 나 `origin_surface_id` 없는
  `file_handler.dispatch` 를 보내 원격이 거절해도 사용자 화면에 경고가 뜨지 않는다. `origin_surface_id`
  를 실은 `file_handler.dispatch` 의 원격 거절은 위 "기본값은 종전 동작" 대로 여전히 toast 가 된다.
  같은 조작을 사용자가 하면 종전대로 뜬다. 에이전트가 mirror markdown 문서를 `markdown.reload` 로
  몇 번 불러도 잘림 toast 가 그 수만큼 쌓이지 않는다.
- **잃은 것**: 에이전트는 그 실패를 IPC 응답으로 알 수 없다 — 전에도 알 수 없었고, 전에는 사용자가
  대신 봤다. 이제는 로그에만 있다.
- **잃은 것(오분류)**: 사용자 조작 한 갈래가 실패 toast 를 잃는다 — markdown plugin 의 **"파일 열기"
  팝업**이다. 그 [열기] 는 `crates/tasty-plugin-markdown/src/popup.rs` 의 `open_markdown_file` 이
  host `file_handler.dispatch` 를 surface 없이 부르고, host 는 그 호출이 에이전트인지 plugin 을 거친
  사용자 클릭인지 가를 수단이 없어 agent origin 으로 싣는다(`src/adapters/ipc/handler/file_handler.rs`
  의 `dispatch_origin` 주석). 그래서 활성 워크스페이스가 mirror 이고 원격이 그 새 탭(focused pane 의
  `Intent::NewTab`)을 거절하면 — 또는 forward 전에 차단되면(`forwarded: false`) — 사용자가 연
  것인데도 실패가 toast 없이 로그로만 간다. 같은 plugin 의
  다른 사용자 조작은 이 갈래가 아니다 — 문서 안 링크 클릭은 대상 pane 이 정해진(`Some(pane)`)
  경로로 `Core::apply` 를 직접 거치고(`src/file/dispatch.rs`) 이 결정의 표시를 안 붙이므로 원격 거절이
  종전대로 toast 로 뜬다. mirror 문서의 주소창·링크는 host 를 부르지 않는다.
  이 한 갈래는 이 결정에서 고치지 않고, origin 없이 불리는 `file_handler.dispatch`
  를 다루는 후속 작업이 맡는다(아래 재검토 조건의 두 번째 항목이 그 변화를 잰다).
  **팝업이 `origin_surface_id` 를 싣는 것만으로는 못 고친다**(2026-09-23 후속 조사, 소스로 확인).
  그러면 host 가 그 요청을 `Some(pane)` 갈래(`src/file/dispatch.rs` 의 `open_surface_tab`)로
  보내 원격 거절 toast 는 돌아오지만, IPC 핸들러가 `FileDispatchOrigin::Agent` 로 고정하므로
  그 갈래의 `activate`(`selects_result`)가 거짓이 되어 **사용자가 연 파일의 새 탭이 선택되지
  않는다.** 지금의 origin 없는 갈래는 `Intent::NewTab` 이 origin 과 무관하게 선택하므로
  (`src/intent/tab.rs`, ADR-0502 가 바로 이 팝업 때문에 그렇게 뒀다) 매번 일어나는 선택을 드문
  원격 거절의 toast 와 맞바꾸게 된다. 그래서 이 갈래는 plugin 이 사용자 클릭임을 싣는 수단
  (재검토 조건 둘째)이 생길 때 **선택과 toast 를 함께** 사용자 쪽으로 옮기며 고친다. origin 을 가를 수
  없는 자리에서는 사용자 상태를 건드리지 않는 쪽(포커스를 안 옮기는 쪽과 같은 방향)을 골랐다.
  [ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md) 는 같은 자리를 사용자 쪽으로
  두었다(새 탭을 선택한다) — 두 ADR 은 `file_handler.dispatch` 가 사용자 조작임을 싣는 채널이 생기면
  함께 풀린다.
- **운영 비용 / 유지 부담**: forward 큐 원소에 칸이 하나 늘었고 attach 세션에 op_id 집합이 하나
  늘었다. 그 집합은 회신(성공·실패)마다 지워지고 재연결 때 비워진다.

### 보강 (2026-09-23) — intent 를 안 거치는 IPC 구조 핸들러

결정 시점의 "origin 을 모르는 자리" 목록에 `Core::apply` 를 intent 없이 직접 부르는 IPC 핸들러가
들어 있었다. 그 핸들러는 origin 을 모르는 것이 아니다 — IPC 요청은 언제나 에이전트 행동이고,
그 실행을 맡는 `core::structural_exec` 는 이미 에이전트 origin(`agent_origin()`)으로 cascade 를
돈다. 그래서 같은 규칙을 그대로 적용한다. 실례는 `tasty new tab --type <원격에 없는 kind>` 를
mirror 워크스페이스의 pane 에 보낸 것이다 — 응답은 `{forwarded:true}` 성공인데 원격이 거절하면
사용자 화면에 `mirror_structural_forward_failed` toast 가 떴다.

- `structural_exec` 의 `Core::apply` 호출은 전부 `apply_as_agent` 를 지난다. 그 함수가 실패를
  `StructuralFailure::Apply` 로 바꾸기 전에 `mark_last_forward_agent_origin` 을 부른다. 덮는
  요청: `split`(pane · surface) · `tab.create` · `tab.close` · `tab.move` · `pane.close` ·
  `surface.close`.
- `image.open`(`src/adapters/ipc/handler/image.rs` `handle_open`)은 `structural_exec` 밖에서
  convert 를 직접 적용하므로 그 자리에서 같은 표시를 붙인다.
- `structural_exec` 는 원격이 forward 한 op 의 실행(`core::attach_runtime`)도 부른다. 그 op 가 이
  기계에서 다시 mirror 로 forward 되면 역시 표시가 붙는다 — 이 기계 앞 사용자의 행동이 아니므로
  같은 결론이다.
- 사용자 조작은 영향이 없다 — 사용자 GUI 경로는 intent(`report_apply_error`)나
  `AppState::forward_mirror_structural` 을 거치고 이 함수들을 부르지 않는다. 번들 plugin 중 이
  메서드들을 사용자 클릭에서 부르는 것은 없다(`image.open` 은 image plugin 이 바깥 호출을 host 로
  넘기는 trampoline 뿐이다).
- 시험: `src/intent/apply_error_tests.rs` 의
  `an_ipc_direct_structural_forward_is_marked_for_a_silent_failure` — 여덟 요청 각각이 forward 되고
  표시되는지 본다. `apply_as_agent` 의 표시 호출을 지우는 변이와 `handle_open` 의 표시 호출을
  지우는 변이에서 각각 실패한다(실측 2026-09-23).

## Alternatives Considered

- **기본값을 "조용히" 로 뒤집는다(표시가 있으면 toast)** — 안 골랐다. origin 을 모르는 `Core::apply`
  호출에는 사용자 GUI 경로도 섞일 수 있고, 표시를 빠뜨린 사용자 경로가 실패 신호를 조용히 잃는다.
  기존 외부 동작을 가장 많이 보존하는 쪽은 에이전트라고 **아는** 자리만 조용하게 하는 것이다.
- **기존 `user_triggered` 로 가른다** — 안 골랐다. 그 칸은 focus 보정과 wire 의 `origin` 을
  정하고, 사용자 경로 중 일부(`intent::surface` 의 convert)는 그것을 세우지 않는다. 그것으로 가르면
  사용자 convert 의 원격 실패 toast 가 사라진다.
- **실패 사유를 IPC 응답에 싣는다** — 안 골랐다. 응답이 적용보다 먼저 나가는 형태를 바꿔야 하고,
  그것은 intent 출구 계약의 변경이다.

## Reconsideration Triggers

**채널이 붙는 것**

- `structural_apply_error` 를 부르는 IPC 핸들러가 새로 생기거나 `Core::apply` 를
  `structural_exec::apply_as_agent` 밖에서 부르게 되면, 그 자리도 표시를 붙이는지 본다(아래
  "보강" 절). 재는 법: `grep -rn 'structural_apply_error(' src/adapters/ipc` 의 호출 자리와
  `grep -n 'core.apply(' src/core/structural_exec.rs`(`apply_as_agent` 안의 한 줄뿐이어야 한다).

- plugin 이 host 를 부를 때 그 호출이 사용자 클릭에서 왔는지 싣게 되면 — 위 오분류 자리를 사용자
  origin 으로 옮긴다 — toast 와 결과 탭 선택을 함께(위 "잃은 것(오분류)" 의 origin 만 싣는 처방이
  안 되는 이유). 재는 법: `FileDispatchOrigin::Agent` 로 고정된 자리(`file_handler.rs`)가
  요청 값으로 바뀌었는지.

**원리적으로 안 붙는 것**

- 에이전트가 실패를 몰라 같은 op 를 되풀이한다는 보고가 오면 — 사유를 응답이나 완료 채널로 돌려주는
  길을 다시 연다. 재는 법: mirror 워크스페이스에 원격에 없는 kind 로 `markdown.navigate` 를 보내고,
  에이전트 쪽에서 실패를 관측할 수 있는 신호가 로그 말고 있는지 본다.

## References

- 개정 대상: [ADR-0401](0401-remote-connection-events-may-raise-a-toast-without-a-user-action.md) (허용 부류 구성원 `mirror_structural_forward_failed` 의 범위 · 알려진 예외 `mirror_markdown_truncated` 의 `markdown.reload` 경로)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 같은 발화점의 선택 축: [ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md)(반대 기본값 — 사용자 경로 보존)
- 부분 개정: [0526](0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md) (잃은 것(오분류) 조항 개정 — markdown 파일열기 팝업으로 연 탭은 이제 사용자 발화라 원격 거절이 toast 가 된다)
- [design/systems/toast.md](../design/systems/toast.md) "트리거 정책" 경로 ④
- [identity.md](../identity.md) 원칙 1
- 코드 근거(결정이 실현된 현재 위치, 심볼 이름): `crate::intent::report_apply_error` ·
  `crate::core::mark_last_forward_agent_origin` · `PendingStructuralForward::silent_failure` ·
  `AttachClientSession::agent_requests`(`src/app/attach_client/agent_origin.rs` 의 `AgentRequests`) ·
  markdown plugin 의 `RemoteRequester`. 시험: `src/intent/apply_error_tests.rs` ·
  `src/app/attach_client/agent_origin.rs` 의 `an_agent_forward_failure_does_not_toast` ·
  `an_agent_markdown_reload_truncation_does_not_toast` · `src/adapters/ipc/handler/markdown_mirror.rs`
  의 `content_request_carries_the_agent_origin` · markdown plugin 의
  `only_an_agent_content_request_carries_the_agent_origin`. `markdown_reload` 가 `RemoteRequester::Agent`
  를 고르는 배선 자체는 plugin 에 host 테스트 더블이 없어 시험 채널이 없다.
