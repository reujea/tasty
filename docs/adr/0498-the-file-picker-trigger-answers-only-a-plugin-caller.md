# ADR-0498: `file_picker.trigger` 는 plugin 호출자에게만 답한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: file-picker, popup, ipc, cli, caller, focus, user-agent-separation, identity

## Context

`file_picker.trigger`([ADR-0058](0058-plugin-triggered-host-popup-async-ack-push.md))는 plugin 이
host 소유 `file_picker` popup 을 열게 하는 메서드다. popup 확정을 기다리지 않고 `request_id` 만
즉답하고, 사용자가 고른 경로는 `"file_picker.result"` 이벤트로 **그 요청을 낸 plugin 에게만**
push 된다.

그런데 권한 표는 이 메서드를 `plugin(Mutate, &[FsRead])` 로 적는다. 이 생성자는 `plugin_only:
false` 라 CLI(`CallerContext::Local`)와 agent 토큰(`CallerContext::Agent`)도 게이트를 통과한다.
핸들러는 그 둘에 대해 requester 를 `None` 으로 두고 popup 을 열었고, popup 쪽은 requester 가
없으면 그 `OpenPopup` intent 를 Tools 메뉴 발화(`from_user_menu("tools_menu")`)로 기록했다.

실측(2026-09-22, debug gui 격리 인스턴스, Xvfb): CLI 로 `file_picker.trigger {}` 를 한 번 부르자
`ui.state` 의 `gate_host_popup_focused` 와 `keyboard_shortcuts_gated` 가 둘 다 false 에서 true 로
바뀌었다. intent 감시 로그에는 `origin=User { source: Menu("tools_menu") }` 가 찍혔다. 즉:

- 에이전트 호출이 사용자의 입력 포커스를 popup 으로 가져가고 단축키를 막았다. 원칙 2.1 ①
  (에이전트 행동의 부수효과가 사용자 상태에 닿지 않는다)과 2.3(release 에는 포커스를 바꾸는 API 가
  없다)의 위반이다. [ADR-0162](0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md) 는 "에이전트
  호출이 모달을 연다는 것 자체가 ① 위반" 이라 적고 이 메서드를 대안으로 들었는데, 즉답+푸시는
  **응답 규약**을 고쳤을 뿐 **포커스 효과**는 그대로였다.
- 그 호출자에게는 결과가 돌아가지 않는다(받을 plugin 이 없다). 호출이 남기는 효과가 포커스 탈취와
  틀린 발화 기록뿐이었다.
- 원칙 1 의 방어선은 `origin.is_user()` 분기다. 라벨이 사용자 메뉴로 틀리면 그 분기가 전부 통과된다.

plugin 발화 — 사용자가 plugin UI(markdown 파일 열기의 Browse)를 눌러 시작한 흐름 — 는 ADR-0058 이
설계한 대로이고 이 결정의 대상이 아니다.

## Decision

**`file_picker.trigger` 는 `CallerContext::Plugin` 에게만 popup 을 연다. CLI·agent 호출은 popup 을
열지 않고 `-32016` 으로 끝난다.** 거부는 핸들러(`handle_trigger`)의 첫 판정이고, popup 이 이미
열려 있는지(`-32000`)나 인자 오류(`-32602`)보다 먼저 온다 — 호출자가 다음에 볼 것이 주체이기
때문이다.

코드는 `-32016`(부를 수 있는 주체가 다르다, [ADR-0163](0163-a-registered-name-answers-who-not-whether.md))을
그대로 쓴다. 뜻이 같다. 메시지는 이 메서드의 사유를 적는다:

    -32016  method 'file_picker.trigger' answers only a plugin caller: the picked path is
            pushed to the calling plugin, so a CLI or agent caller would only take the
            user's input focus

권한 표의 `plugin_only` 표식은 **달지 않는다.** 그 표식의 뜻은 "외부 dispatch arm 이 없고 plugin
host-call 진입부가 직접 인터셉트한다" 이고(`src/source_guards/plugin_only_dispatch_parity.rs` 가
인터셉트 집합과 양방향으로 맞춘다), 이 메서드는 외부 arm(gui 창 라우터 `route_window_handler`)으로
라우팅된다.

개정하지 않는 것: ADR-0058 의 즉답+푸시 규약, 동시성 정책(이미 열려 있으면 `-32000`), `FsRead`
권한 요구, Tools 메뉴 경로의 발화 기록(`from_user_menu` — 그것은 실제로 사용자 메뉴 발화다).

## Consequences

- **얻은 것**: 에이전트 호출로 사용자 입력 포커스가 옮겨 가는 release 경로가 하나 사라진다. IPC 로
  연 popup 은 이제 항상 requester 를 갖고, 그 `OpenPopup` 은 `from_agent_plugin` 으로 기록된다 —
  사용자 메뉴 발화로 잘못 기록되는 IPC 경로가 없다.
- **잃은 것**: CLI 나 agent 가 이 메서드로 사용자 화면에 파일 선택 popup 을 띄우던 것이 에러가 된다
  (외부 동작 변화). 그 호출은 결과를 받을 방법이 없었으므로 잃는 기능은 popup 표시 자체뿐이다.
  레포 안 CLI 명령 중 이 메서드를 부르는 것은 없다. 에이전트는 경로를 직접 지정한다.
- **잃은 것**: 권한 표만 보고는 이 거부가 안 보인다 — 표는 여전히 `plugin_callable` 이고
  `plugin_only` 가 아니다. 이 사실은 이 ADR 과 `docs/dev-guide/api-conventions.md` 의 `-32016` 절이
  적는다.
- **운영 비용**: 창 라우터에 새 메서드를 더하는 사람은 그 메서드가 사용자 상태(popup·포커스)에
  닿는지, 그렇다면 누가 불러도 되는지를 정해야 한다.

## Alternatives Considered

- **권한 표에 `plugin_only` 를 단다**: 표만으로 거부가 보인다. 안 고른 이유: 그 표식은 "외부 arm
  이 없다" 는 라우팅 사실을 말하고, 인터셉트 집합과 짝 맞춤 가드가 그것을 강제한다. 이 메서드는
  외부 arm 으로 라우팅되므로 표식이 사실과 다른 것을 말하게 된다.
- **popup 을 포커스 없이 연다**: 사용자 입력은 안 막는다. 안 고른 이유: 에이전트가 사용자 화면에
  popup 을 띄우는 것 자체가 원칙 2.1 ② 의 "popup 강제 open" 이고, 결과를 받을 곳도 여전히 없다.
- **발화 라벨만 바로 세운다(`from_agent_…`)**: 기록은 맞아진다. 안 고른 이유: 포커스 효과가 그대로다.
- **debug 로 옮긴다**: 자기검증용으로는 debug popup 강제 open(`debug.popup.*`)이 이미 있다. 이 메서드의
  정상 사용자는 plugin 이라 release 에 남아야 한다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 비-plugin 호출이 다시 popup 을 연다 — `src/adapters/ipc/handler/file_picker.rs` 의
  `trigger_from_a_non_plugin_caller_is_refused_without_opening_the_popup` 이 깨진다.
- 창 라우터(`route_window_handler`)에 팔이 새로 생기거나 이 메서드가 호출자를 다시 가리지 않는다 —
  `src/adapters/ipc/handler/window_router_caller_tests.rs` 가 깨진다. 앞쪽은 팔과 호출자 명부의 집합
  대조이고, 뒤쪽은 `PluginOnly` 팔을 CLI·agent 로 불러 `-32016` 과 창 무변화를 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 에이전트가 사람에게 파일을 고르게 하고 그 결과를 받아야 하는 요구가 나온다. 그때는 결과를 agent
  에게 돌려줄 채널(`approval.await` 같은 지연 응답)과 포커스 규약을 함께 정하는 새 메서드가 필요하다.
  재는 법: 그런 요구를 담은 issue·TODO 가 있는지, 에이전트 쪽에서 이 메서드를 부른 흔적(`-32016`
  응답)이 사용자 보고에 나오는지.

## References

- [ADR-0058](0058-plugin-triggered-host-popup-async-ack-push.md) — 이 메서드의 즉답+푸시 규약
- [ADR-0162](0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md) — 에이전트 호출이 모달을 여는 것이 원칙 ① 위반이라는 판정
- [ADR-0163](0163-a-registered-name-answers-who-not-whether.md) — `-32016` 의 뜻(주체 축)
- [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) — 이 메서드를 gui 창 라우터로 옮긴 결정
- 코드 근거(현재 위치): `src/adapters/ipc/handler/file_picker.rs` 의 `handle_trigger`
- 기능 문서: [네이티브 파일 피커](../features/native-file-picker/index.md)
