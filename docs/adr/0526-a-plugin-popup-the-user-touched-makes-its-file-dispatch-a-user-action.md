# ADR-0526: 사용자가 만진 plugin popup 에서 온 파일 열기는 사용자 행동이다 — ADR-0302 의 `file_handler.dispatch` 분류 조항 · ADR-0502 의 `Intent::NewTab` 선택 조항 · ADR-0503 의 오분류 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: file-handler, focus, tab, plugin, popup, user-agent-separation, identity, user-activation, adr-0302, adr-0502, adr-0503
- **Group**: file-handler

## Context

[ADR-0302](0302-a-user-file-open-selects-its-result-tab.md) 는 파일 열기의 발화 주체를
`FileDispatchOrigin`(`User`/`Agent`)으로 싣게 하면서 IPC `file_handler.dispatch` 를 `Agent` 로
고정했다 — 요청에 "이것은 사용자 조작을 중계한 것" 이라고 가를 값이 없었기 때문이다.
[ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md) 는 새 탭의 선택을
`DomainIntent::CreateTab.activate` 로 옮기면서 `Intent::NewTab` 만은 origin 과 무관하게 `true` 로
두었다. 에이전트 라벨로 그 인텐트에 오는 발화점은 origin 없는 `file_handler.dispatch` 하나인데,
markdown plugin 의 파일열기 팝업 — 사용자의 `open_markdown` 단축키가 여는 것 — 의 [열기] 가 바로
그 호출로 새 탭을 열어서다. origin 으로 가르면 사용자가 연 markdown 탭이 선택되지 않는다.

그 결과 두 결함이 한 뿌리에서 났다.

- 에이전트가 `file_handler.dispatch`(origin 없이)로 파일을 열면 사용자가 보던 탭이 새 탭으로
  바뀌었다 — [정체성 원칙](../identity.md) 1·3 위반이 ADR-0502 의 "잃은 것" 에 남아 있었다.
- 반대로 사용자가 팝업으로 연 파일은 host 에 에이전트 요청으로 도착해, 적용 실패를 에이전트
  기준(로그)으로 보고하는 경로가 생기면 사용자에게 안 보인다.

두 ADR 모두 재검토 조건으로 "plugin 이 사용자 조작임을 실어 보낼 채널이 생기면" 을 적어 두었다.

## Decision

**plugin 은 자기 popup 안의 사용자 조작으로 `file_handler.dispatch` 를 부를 때 그 popup 의
instance id 를 `owner_popup_instance` 로 싣고, host 는 세 조건이 모두 맞을 때만 그 호출을
사용자 행동(`FileDispatchOrigin::User`)으로 친다.**

1. 호출자가 plugin 이다(`CallerContext::Plugin`). 외부 IPC 호출자(CLI · 에이전트)는 같은 키를
   실어도 사용자가 될 수 없다.
2. 그 instance 가 호출 plugin 소유로 이 창에 열려 있다.
3. 그 instance 가 사용자의 **확정형 입력**(포인터 버튼 누름 · 키 누름)을 받았다. host 는
   popup 에 입력을 forward 하는 자리(`plugin_bridge::popup_render`)에서 이를 기록한다
   (`AppState::plugin_popup_user_activated`). 포인터 이동 · 휠 · 떼기는 세지 않는다. 닫힌
   instance 의 기록은 forward 추적과 같은 자리에서 걷힌다.

release 에는 입력 주입이 없으므로(원칙 1 ②) 3 은 사람이 그 popup 을 만졌다는 뜻이다. 어느 하나라도
어긋나면 조용히 `Agent` 로 떨어진다(debug 로그 한 줄) — 거절하지 않는다. 요청은 종전처럼 처리되고
선택만 옮기지 않는 쪽이라, 호출자가 틀린 값을 실어도 잃는 것은 사용자 상태가 아니다.

발화 intent 의 `IntentOrigin` 도 같은 값에서 나온다 — `User` 면 `from_user_menu("plugin_popup")`,
`Agent` 면 종전대로 `from_agent_ipc()`. 이 값이 identify 왕복 뒤 `Intent::NewTab` 의 origin 과
적용 실패 보고의 갈래를 정한다.

**`Intent::NewTab` 의 `activate` 는 이제 `origin.is_user()` 다.** 사용자 발화점(탐색기 단축키 ·
도구 메뉴 · 사용자 파일 열기의 origin 없는 갈래)은 종전대로 선택하고, 에이전트 라벨로 온 탭은
사용자가 보던 탭 뒤에 붙기만 한다.

markdown plugin 의 파일열기 팝업 [열기] 는 `owner_popup_instance` 를 싣는다. popup 을 닫는
`popup.close` 는 `host.call` 이 동기라 dispatch 가 처리된 **뒤에** 나가므로 조건 2 가 성립한다.

그래서 [ADR-0503](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) 의
"잃은 것(오분류)" 조항 — markdown 파일열기 팝업으로 연 탭이 mirror 에서 원격에 거절되거나 forward
전에 차단돼도 에이전트 발화로 분류돼 toast 없이 로그로 간다 — 도 개정한다. 그 탭의 발화 intent 는
이제 사용자라, 원격 거절과 차단은 종전 사용자 조작처럼 toast 가 된다. 그 조항이 가리키던
`dispatch_origin` 주석은 이 결정이 `dispatch_origin_of` 로 바꿨다.

**개정하지 않는 것**

- ADR-0302 의 나머지: `FileDispatchOrigin` 이라는 별도 값 · 그것이 identify 왕복과 picker 를
  건너는 방식 · 명시 origin 갈래(`open_surface_tab`)가 `selects_result()` 로 선택을 정하는 것 ·
  그 밖의 발화점 분류(explorer · 드롭 · picker 확정 · 터미널 링크 메뉴는 `User`).
- ADR-0502 의 나머지: `CreateTab.activate` 필드 · `tab.create` 의 `false` · 파일 디스패치 origin
  갈래 · attach forward `NewTab` 의 `ForwardOrigin` 규칙 · terminal kind 의 background 고정 ·
  mirror pane 에서 서버가 선택을 정하는 것 · 응답 `active_tab` 의 의미.
- ADR-0503 의 나머지: 에이전트 발화의 적용 실패는 로그로 간다는 결정 자체 · forward op 의
  `silent_failure` 표시와 attach 세션의 op_id 집합 · origin 을 모르는 자리의 기본값(종전 toast) ·
  markdown 원문 요청의 `agent_origin` · ADR-0401 에 대한 개정. popup 근거가 없는 origin 없는
  `file_handler.dispatch` 는 여전히 에이전트이고 그 실패는 로그로 간다.
- ADR-0279 의 라우팅(어느 pane 에 붙는가).
- `file_handler.dispatch` 의 응답 모양(`accepted` · `depth` · `ignore_size_limit`)과 오류 코드.

## Consequences

- **얻은 것**:
  - 에이전트가 origin 없이 `file_handler.dispatch` 를 불러도 사용자가 보던 탭과 포커스가 그대로다.
  - 사용자가 markdown 파일열기 팝업으로 연 파일은 종전대로 새 탭이 선택되고, host 에 사용자
    행동으로 도착한다 — 적용 실패를 origin 으로 가르는 보고 경로에서도 사용자 쪽으로 간다.
  - plugin 이 사용자 조작을 중계하는 일반 채널이 하나 생겼다. 근거는 요청 값이 아니라 host 가
    직접 관측한 입력이라 외부 호출자가 흉내낼 수 없다.
- **잃은 것**:
  - origin 없는 `file_handler.dispatch` 를 부르면서 새 탭이 선택되기를 기대하던 호출자는 그
    선택을 잃는다. 레포 안에서 그렇게 부르는 사용자 경로는 markdown 파일열기 팝업 하나였고 이
    결정이 옮겼다. 다른 plugin(서드파티 포함)이 자기 popup 에서 같은 호출을 하면 이 키를 실어야
    한다.
  - plugin 이 사용자가 한 번 누른 자기 popup 이 열려 있는 동안에는 그 popup 을 근거로 몇 번이든
    사용자 행동을 주장할 수 있다. plugin 코드는 이미 사용자 입력을 받는 쪽이라 그 이상은 막지
    않았다 — 막을 대상은 외부 호출자와 사람이 안 만진 popup 이다.
  - markdown 문서 안의 파일 링크 클릭(명시 origin 갈래)은 여전히 `Agent` 로 도착한다 — webview
    클릭은 popup 입력이 아니라 이 채널이 안 덮는다. ADR-0302 의 재검토 조건 "재는 법" 이 그대로
    유효하다. (ADR-0568 이후 macOS 에서만 그렇다.)
- **운영 비용 / 유지 부담**: popup 입력 forward 자리가 이 기록을 세우는 유일한 곳이다. 입력
  경로를 새로 만들면(예: popup 에 입력을 다른 채널로 보내기) 그 자리도 기록을 세워야 사용자
  조작이 에이전트로 떨어지지 않는다.

## Alternatives Considered

- **요청에 `origin: "user"` 같은 선언을 받는다**: 외부 IPC 호출자도 실을 수 있어 에이전트가
  사용자 포커스를 옮기는 release API 가 된다(원칙 3).
- **popup 소유만 본다(입력은 안 본다)**: plugin 은 이벤트로 자기 popup 을 스스로 열 수 있어,
  사람이 안 만진 popup 으로도 사용자 행동이 된다.
- **입력 뒤 일정 시간 창을 둔다(브라우저 user activation 식)**: 시간 창은 부하에 따라 흔들리고
  값을 정할 근거가 없다. popup 수명이 이미 자연스러운 경계다.
- **plugin 이 보내는 모든 `file_handler.dispatch` 를 사용자로 친다**: plugin 의 백그라운드 작업이
  사용자 포커스를 옮긴다.
- **팝업 [열기] 를 host 쪽 별도 사용자 메서드로 옮긴다**: popup 이 plugin 소유라 host 가 [열기]
  의 의미를 알아야 한다 — generic 계약(원칙 2)이 깨지고, 같은 문제가 다음 plugin 에서 반복된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- release 에 popup 입력을 주입하는 경로가 생기면 조건 3 이 사람의 조작을 뜻하지 않게 된다.
  `debug.inject_*` 가 debug 격리 밖으로 나오는지로 잰다(`docs/dev-guide/debug-ipc.md`).
- `IntentOrigin` 이 identify 왕복을 건너게 되면(ADR-0302 의 같은 조건) `FileDispatchOrigin` 과
  이 채널을 그 값으로 합친다.
- 파일 열기 말고 다른 host 메서드도 plugin 이 사용자 조작을 중계하게 되면, 조건 1~3 을 그
  메서드마다 되풀이하지 말고 공용 판정으로 올린다. 지금 이 판정을 부르는 자리는
  `adapters::ipc::handler::file_handler::dispatch_origin_of` 하나다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- markdown 링크 클릭처럼 popup 밖에서 오는 사용자 조작의 중계가 흔해지면 webview 입력에도 같은
  활성화 기록이 필요하다. 재는 법: markdown 문서 안의 파일 링크를 클릭하고 `tab.list` 로 새 탭이
  활성인지 본다 — 지금은 아니다. (ADR-0568 로 발화했다 — macOS 에서만 아직 아니다.)

## References

- 개정 대상: [ADR-0302](0302-a-user-file-open-selects-its-result-tab.md) (`file_handler.dispatch` 를 `Agent` 로 고정한 조항)
- 개정 대상: [ADR-0502](0502-an-agent-created-tab-does-not-take-the-users-tab.md) (`Intent::NewTab` 의 `activate: true` 조항)
- 개정 대상: [ADR-0503](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) (잃은 것(오분류) 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 부분 개정: [0568](0568-a-user-gesture-on-a-page-the-owning-plugin-wrote-makes-its-webview-file-dispatch-a-user-action.md) ("세 조건이 모두 맞을 때만 사용자로 친다" 조항 개정 — 엔진이 사용자 제스처로 보고하고 자기가 쓴 페이지 위에서 난 자기 webview 의 navigation 도 근거가 된다)
- 선행 결정: [ADR-0084](0084-plugin-triggered-host-popup-ownership.md) (plugin 이 자기 popup instance 를 `owner_popup_instance` 로 신고하는 선례 — 같은 키 이름을 쓴다)
- 탐색: `git grep -l 'owner_popup_instance\|FileDispatchOrigin\|Intent::NewTab' -- docs/adr/`
- [Focus policy](../design/policies/focus.md) · [File handler](../features/file-handler/index.md) · [markdown plugin](../plugins/markdown/index.md)
- 현재 구현(심볼): `adapters::ipc::handler::file_handler::dispatch_origin_of` ·
  `plugin_bridge::popup_render::is_user_activation` · `AppState::plugin_popup_user_activated` ·
  `IpcWindow::plugin_popup_user_activated` · `intent::tab::new_tab`. 입구 시험은
  `adapters/ipc/handler/file_handler_origin_tests.rs` 이고, 조건 3 의 기록 자리(누름에서 세우고
  호버로는 안 세우며 닫힌 popup 의 기록을 걷는 것)는 `plugin_bridge/popup_render_activation_tests.rs`
  가 `draw_plugin_popups` 를 실제로 돌려 잰다. 그 시험이 plugin 프로세스 없이 popup 인스턴스를
  세우는 훅은 `PluginManager::insert_popup_instance_for_test` 이고 `tasty-host-plugin` 의
  `test-support` feature 뒤에 있다 — 루트가 dev-dependency 로만 켜므로 제품 빌드에는 없다.
