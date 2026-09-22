# ADR-0311: namespace 호출의 만료는 fail-open 이 아니라 caller 에 대한 오류다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: plugin, ipc, timeout, host-plugin, error-handling, adr-0078
- **Group**: plugin-system

## Context

호스트가 plugin 에 보낸 요청은 `PendingRequestKind` 로 보관되다 응답이 오면
소진된다. 그 변종 가운데 `deadline` 을 든 것은 extension hook 4 종뿐이었고,
plugin namespace 로 forward 한 호출 3 종 — local caller 용(`NamespaceInvoke`),
plugin→plugin 용(`PluginToPluginNamespace`), post-hook 이 걸린
것(`NamespaceInvokeWithPostHook`) — 은 deadline 이 없었다.

그래서 그 셋은 **target 이 답하거나 target 이 치워질 때만** 끝났다. 치우는 경로는
이미 있다 — healthcheck 가 `HEALTHCHECK_TIMEOUT`(60s) 무응답 plugin 을 재시작하고
그 길에 `cancel_pending_namespace_calls` 가 caller 에 `-32004` 를 돌려준다. 판정을
ping tick 에서 하므로 상한은 `HEALTHCHECK_TIMEOUT + PING_INTERVAL` = 75s 다.

남는 것은 그 경로가 **원리적으로 볼 수 없는** 경우다: ping 에는 제때 답하면서
이 호출 하나만 끝내 안 돌려주는 plugin. 프로세스는 건강하므로 재시작이 안 걸리고,
pending 항목은 영영 남으며, caller 는 — `IpcConnection` 이 읽기 타임아웃을 걸지
않으므로([ADR-0078](0078-shutdown-rejects-pending-ipc.md)) — 영영 기다린다.

hook 의 deadline 은 그대로 복제할 수 없다. hook 은 매니페스트가 `timeout_ms` 를
선언하고 그 값은 `HOOK_TIMEOUT_MS_MAX` 로 1 초에서 잘린다. namespace 호출에는 그런
선언이 없다 — 실제 작업을 하는 호출이라 1 초 상한이 맞지도 않는다.

## Decision

**namespace 호출 3 종에 `deadline` 을 붙이고, 만료를 fail-open 이 아니라 caller 에
대한 오류로 끝낸다.**

값은 새로 고르지 않고 **기존 회수 상한에서 유도한다** —
`NAMESPACE_CALL_TIMEOUT = 2 × (HEALTHCHECK_TIMEOUT + PING_INTERVAL)`. 유도의 요지는
"길이" 가 아니라 **순서**다: 이 deadline 이 회수 상한보다 짧으면 healthcheck 가 이미
처리하는 경우를 앞질러 회신 시점을 바꾼다. 두 배로 두면 여기에 걸리는 것은
healthcheck 가 볼 수 없는 경우뿐이다.

만료 처리는 hook 과 갈린다. hook 만료는 fail-open 이다 — hook 이 없었던 것으로 치고
원래 흐름(target invoke / event fan-out)을 그대로 진행시킬 수 있기 때문이다.
namespace 호출에는 진행시킬 원래 흐름이 없다. 기다리던 target 응답 자체가 목적이었고,
그것이 안 온 것이 사건이다. 그래서 **plugin 이 사라졌을 때와 같은 모양**으로
끝낸다 — `cancel_pending_namespace_calls` 가 쓰는 세 회신 경로를 그대로 쓴다: local
caller 는 `response_tx`, plugin caller 는 `ipc.result`, post-hook 이 걸린 것은
`send_final_error`. caller 입장에서 두 경우는 같은 일이다 — 기다리던 plugin 응답이
끝내 오지 않았다.

**그 셋이 같은 코드를 싣는다.** `-32004` 는 local/CLI caller 에게도 plugin caller
에게도 그대로 간다. 이 문단은 원래 그 반대를 적고 있었다 — plugin caller 로 가는 둘이
메시지만 싣고 코드를 버려서, plugin A 가 plugin B 의 메서드를 부르고 B 가 삼키면 A 가
보는 코드는 `-32004` 가 아니라 SDK 기본값 `-32000` 이었다. 그 비대칭은 이 결정이 만든
것이 아니라 `send_final_error` 가 원래 가진 것이었고, 그래서 이 ADR 은 그것을 고치지
않은 채 사실로만 적고 아래 재검토 조건에 "없어지면 이 문단을 지워라" 를 달아 두었다.
2026-09-20 에 그 조건이 충족됐다 — 네 자리가 한 번에 코드를 싣게 됐고, 근거는
[ADR-0171](0171-a-host-error-code-survives-the-plugin-boundary.md) 의 2026-09-20 보강이다.
그 ADR 이 센 "버리는 자리 일곱" 에 이것이 없었던 이유(호스트가 *되받은* 코드가 아니라
*스스로 내는* 코드라 축이 한 칸 다르다)도 거기 적혀 있다.

sweep 은 하나로 둔다. 같은 `pending_requests` 를 한 번 훑어 deadline 을 든 변종을
모두 본다(`sweep_expired_requests`).

### 2026-09-20 보강 — 반복되는 만료는 caller 가 아니라 plugin 에 대한 판정이다

위 결정은 만료 **한 건**을 다룬다. 그 결정만으로 끝나지 않는 상태가 하나 남았다:
같은 plugin 이 같은 호출을 **계속** 삼키면 호출마다 `NAMESPACE_CALL_TIMEOUT` 을 태우고
남는 것은 경고 로그뿐이고, 그 상태는 스스로 끝나지 않는다. 프로세스가 ping 에 답하므로
healthcheck 가 원리적으로 못 보는 것이 애초에 이 ADR 의 전제였다.

**그래서 연속 만료를 plugin 단위로 세고, `NAMESPACE_EXPIRY_RESTART_LIMIT`(3) 에 닿으면
그 plugin 을 healthcheck 무응답과 같은 경로로 재시작한다.** 계수는 그 plugin 의 namespace
응답이 하나라도 도착하면 지운다 — **만료 뒤에 도착한 늦은 응답도 포함한다.** 답하고 있는
plugin 은 아무리 느려도 여기 안 쌓인다.

**늦은 응답을 세는 것이 이 결정의 일부다.** 만료된 호출의 응답이 올 때 pending 은 이미
sweep 이 거둬 없다. 그러니 그 자리에서 계수를 안 지우면, `NAMESPACE_CALL_TIMEOUT` 을
조금씩 넘겨 답하는 plugin 이 — **매 호출마다 실제로 응답을 보내는데도** — 세 번마다
재시작된다. 재시작은 느린 것을 빠르게 만들지 못하므로 그 반복은 그 plugin 의 surface ·
popup · banner 를 주기적으로 없앨 뿐이다. 계수가 가리려는 것은 **아무것도 안 답하는**
plugin 이고, 늦어도 답한 것은 답한 것이다. 반대 방향(만료 전 응답만 센다)을 고르면 위
"느린 plugin 은 안 걸린다" 는 성질을 포기해야 한다.

**그런데 "id 가 안 맞는 응답" 전체로 지우면 안 된다.** 그 갈래에는 늦은 hook 응답과 잡음도
들어오고, 그러면 namespace 호출만 삼키면서 hook 에만 답하는 plugin 이 판정을 빠져나간다.
그래서 만료로 거둔 namespace 호출의 request id 를 plugin 당 계수 상한만큼만 기억하고
(`expired_namespace_calls`), 늦은 응답의 id 가 거기 있을 때만 지운다. 상한에 닿으면
재시작이 일어나고 그 경로가 두 자료를 함께 비우므로, 이 자름이 판정을 무르게 하지 않는다.

pong 은 계수를 지우지 않는다: 계수가 가리려는 것이 바로 "ping 에만 답하는 plugin" 이라,
pong 이 지우면 계수가 영영 안 찬다. pong 은 애초에 pending 을 안 만들고, 위 id 목록에도
없으므로 두 갈래 어디서도 계수에 닿지 않는다.

**hook 의 backoff 를 이식하지 않는다.** hook 은 선택적이라 우회가 곧 정상 동작이고,
그래서 연속 실패에 "잠시 안 부른다"(`HOOK_FAIL_BACKOFF`)가 답이 된다. namespace 호출에는
우회할 대상이 없다 — 같은 처방을 옮기면 "시도조차 않고 즉시 실패" 가 되어 **회복한
plugin 이 backoff 창 동안 도달 불가**가 된다. 재시작은 그 반대 방향이다: 그 자리에서
pending 을 전부 거두고(`cancel_pending_namespace_calls`, 이 ADR 의 회신 경로 그대로)
plugin 을 다시 띄우므로 다음 호출은 기다림 없이 건강한 프로세스에 닿는다.

판정 시점은 만료 시점이 아니라 **다음 ping tick** 이다. 만료는 sweep 루프 한가운데서
일어나는데 재시작은 같은 `pending_requests` 를 다시 훑어 거두므로, 계수만 올리고 판정은
`restart_unresponsive_plugins` 가 healthcheck 와 같은 자리에서 한다. 그래서 재시작까지의
추가 지연 상한은 `PING_INTERVAL`(15 s) 이다.

### 2026-09-21 보강 — 이미 끝난 요청의 늦은 응답은 아무것도 다시 진행시키지 않는다

위 두 절은 만료 **시점**을 정했다. 그 뒤에 target 이나 extension 의 응답이 도착하는 경우는
값으로 안 적혀 있었다 — 코드는 `handle_plugin_response` 의 `None` 갈래에서 버리고 있었지만,
그것이 결정인지 우연인지를 말하는 문장이 없었고 post-hook 이 걸린 경로에서 특히 그랬다.
그 경로에는 응답이 올 수 있는 자리가 셋(target 응답 · pre-hook 응답 · post-hook 응답)이고,
각각 만료되면 다른 방식으로 이미 끝이 났다.

**늦은 응답을 처리하는 자리를 하나로 둔다(`settle_late_response`). 그 자리는 연속 만료 계수를
지우는 것(위 보강) 말고는 아무것도 진행시키지 않는다.** 요청이 pending 에서 빠지는 길은 셋 —
응답 매칭 · deadline 만료 · plugin 이 치워짐 — 이고, 뒤 둘은 **그 자리에서 caller 에 대한 끝을
이미 냈다.** 끝이 난 요청에 두 번째 끝을 내면 그것이 결함이다. 같은 자리에는 **pending 에 애초에
안 들어가는 요청**(ping · `event.dispatch` 같은 알림성 요청)의 답도 온다 — id 만으로는 늦은 응답과
갈리지 않고 정상 운용에서 늘 나므로, 한 건마다의 기록은 trace 에 둔다. 변종별로:

- **target 응답이 늦으면 post-hook 을 부르지 않는다.** caller 는 이미 `-32004` 를 받았다.
  post-hook 은 caller 에게 갈 결과를 바꾸는 단계라 결과가 나간 뒤에는 바꿀 대상이 없고, 부르면
  extension 에 **효과만** 남는다 — caller 가 실패로 본 호출에 대해 기록·전송 같은 일이 난다.
- **pre-hook 응답이 늦으면 target 을 다시 부르지 않는다.** fail-open 이 원본 payload 로 이미
  불렀다. 다시 부르면 같은 호출이 두 번 실행된다(변형된 payload 로 한 번 더).
- **post-hook 응답이 늦으면 결과를 다시 보내지 않는다.** fail-open 이 target 결과를 이미 보냈다.
- **hook 실패 계수를 되돌리지 않는다.** 만료가 실패로 셌고, backoff 가 묻는 것은 "제때 답하는가"
  라 늦은 답은 여전히 실패다. namespace 계수와 방향이 반대인 이유는 처방이 다르기 때문이다 —
  namespace 계수의 처방(재시작)은 느린 plugin 을 벌하므로 늦은 답이 구해야 하고, hook 계수의
  처방(우회)은 느린 hook 을 건너뛰는 것이 곧 정상 동작이다.

이 결정은 **실행 취소를 약속하지 않는다.** 늦은 응답이 도착했다는 것은 plugin 이 그 호출을
실제로 실행했다는 뜻이고, 호스트는 그 효과를 되돌리지 못한다. caller 가 받은 `-32004` 는 "결과를
모른다" 이지 "실행되지 않았다" 가 아니다 — 같은 성질을 IPC 쪽 대기 만료가
[ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md)
에서 `-32061` 로 적었다. 이 경로의 코드는 이미 `-32004` 로 정해져 있어 바꾸지 않는다(대안 C 의
이유 그대로).

## Consequences

- **얻은 것**: 건강한 plugin 이 한 호출만 삼켜도 caller 가 끝난다. pending 항목이
  무기한 쌓이는 갈래가 닫힌다. [ADR-0078](0078-shutdown-rejects-pending-ipc.md) 이
  종료 경로에 세운 계약 — "무응답은 hang 과 구분되지 않으므로 답한다" — 이 plugin
  forward 경로에도 선다.
- **잃은 것**: 응답에 `NAMESPACE_CALL_TIMEOUT` 이상 걸리는 namespace 호출은 이제
  실패한다. 그런 호출은 지금 없다 — 번들 plugin 에서 긴 작업은 전부 백그라운드
  스레드로 내보내고 핸들러는 즉시 답한다. 만약 생긴다면 그 호출은 즉시 답하고
  결과를 따로 알리는 모양으로 바꿔야 하며, 이 상수를 올리는 것은 처방이 아니다.
- **운영 비용 / 유지 부담**: 상수 하나와 sweep 의 match 팔 3 개. sweep 자체는 이미
  매 pump 마다 돌고 있었다.
- **2026-09-20 보강이 더한 것**: plugin 당 `u32` 하나(`namespace_expiries`)와 최근 만료
  id 를 상한만큼 담는 큐 하나(`expired_namespace_calls`), 그리고 재시작 판정의 두 번째
  사유. **동작 변경**이다 — 종전에는 namespace 호출만 삼키는 plugin 이 무한히 그 상태로
  남았고, 이제는 연속 3 회 만에 재시작된다. 재시작은 그 plugin 의
  surface·popup·banner·mesh 프레임을 그 경로가 원래 정리하는 대로 정리하므로, 그
  plugin 의 화면 상태가 사라졌다 다시 생긴다.

## Alternatives Considered

- **A: caller 쪽(`IpcConnection`)에 읽기 타임아웃을 건다** — 그쪽에는 의도적으로
  무한히 기다리는 호출(`agent task-await --timeout-ms 0`, 사용자 응답 대기 approval)이
  있어 한 값으로 자르면 정상 동작을 끊는다. [ADR-0078](0078-shutdown-rejects-pending-ipc.md)
  이 이미 그 이유로 기각했다. 여기서 막는 것은 호스트가 **답할 책임이 있는데 안 답하는**
  좁은 경우다.
- **B: 만료도 hook 처럼 fail-open 한다** — 진행시킬 원래 흐름이 없다. 조용히 pending
  에서만 지우면 caller 는 여전히 영영 기다리고, 늦게 온 응답이 갈 곳도 사라져 오히려
  나빠진다.
- **C: 만료에 새 오류 코드를 준다** — 진단에는 유리하지만 caller 계약이 둘로 갈린다.
  "plugin 이 네 호출을 끝내지 못했다" 는 local caller 에게 이미 `-32004` 로 나가고
  있었고, 사유는 메시지가 나른다.
- **D: 값을 매니페스트 선언으로 받는다** — plugin 이 자기 호출의 상한을 스스로 정하게
  되어 이 deadline 이 막으려는 바로 그 경우(안 답하는 plugin)를 plugin 이 무력화할 수
  있다. 선언을 받으려면 protocol 이 바뀌고 번들 plugin 전부가 따라 움직인다.
- **E (2026-09-20 보강): 반복 만료에 hook 처럼 backoff 를 건다** — 기각. 위 보강 절의
  이유 그대로다. hook 의 우회는 정상 동작이지만 namespace 호출의 우회는 도달 불가다.
- **F (2026-09-20 보강): 세기만 하고 로그만 올린다** — 기각. 그러면 이 ADR 이 막으려던
  것("caller 가 영영 기다린다")은 한 건마다 막히지만, **호스트 쪽 비용**(매 호출 150 s 의
  pending 과 그만큼의 caller 대기)은 그대로 무한히 반복된다. 관측만으로는 그 반복이
  끝나지 않는다.
- **G (2026-09-20 보강): 만료 전에 도착한 응답만 계수를 지운다** — 기각. **위 E 와 같은
  반례가 선다** — 처방이 잘못된 plugin 이 아니라 *느릴 뿐인* plugin 을 벌한다. 구현은 가장
  작지만(`None` 갈래를 그냥 `return` 한다) `NAMESPACE_CALL_TIMEOUT` 을 조금씩 넘겨
  **답하는** plugin 이 세 번마다 재시작되고, 재시작이 그 느림을 못 고치므로 그 상태가
  주기적으로 반복된다. 그러면 "느린 plugin 은 안 걸린다" 를 문서에서 **지워야** 하는데,
  그 문장이야말로 이 계수가 healthcheck 와 구별되는 이유다.

- **H (2026-09-21 보강): 늦은 target 응답에도 post-hook 을 부른다** — 기각. post-hook 의
  존재 이유는 caller 에게 갈 결과를 바꾸는 것인데 그 결과는 이미 나갔다. 부르면 extension 이
  "결과를 바꿀 수 없는 호출" 을 받고, 그 hook 이 부수효과를 가진 것이면 caller 가 실패로 본
  호출에 대해 효과가 난다.
- **I (2026-09-21 보강): 늦은 pre-hook 응답의 변형 payload 로 target 을 다시 부른다** — 기각.
  fail-open 이 이미 한 번 불렀으므로 이것은 **같은 호출의 두 번째 실행**이다. extension 의 변형을
  살리는 것보다 한 번 실행이 우선한다.
- **J (2026-09-21 보강): 늦은 hook 응답이 hook 실패 계수를 지운다** — 기각. 위 보강 절의 이유 —
  hook 의 처방은 우회라 느린 hook 을 건너뛰는 것이 곧 정상 동작이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `NAMESPACE_CALL_TIMEOUT` 이 `HEALTHCHECK_TIMEOUT` · `PING_INTERVAL` 에서 유도되지
  않는 형태로 바뀌었을 때. 유도가 이 결정의 내용이므로, 리터럴로 바뀌면 근거가 사라진다.
  (이 좌변에 붙은 판정기는 지금 없다.)
- healthcheck 회수 경로(`cancel_pending_namespace_calls`)가 사라지거나 namespace
  pending 을 더 이상 거두지 않게 됐을 때. 그러면 "앞지르지 않는다" 는 유도의 전제가 없어지고
  값은 회수 상한이 아니라 정상 호출 길이에서 나와야 한다. (좌변이 두 갈래라 판정기도 갈래마다
  다르다. **회수 경로가 사라지는** 갈래 — `forget_plugin_runtime` 이 그 함수를 더 이상
  부르지 않는 것 — 는 `tasty-host-plugin` 의
  `a_plugin_restarted_by_the_expiry_streak_is_restarted_once` ·
  `requests_sent_to_a_restarted_plugin_are_reclaimed` 둘이 잰다. 호출을 조건으로 감싸
  경로에서 빼면 그 둘만 죽는다. **pending 을 더 이상 거두지 않는** 갈래 — 함수 안의
  회수 루프 — 는 `a_forward_cancelled_by_plugin_removal_lands_as_cancelled` 하나가 잰다.
  그 시험은 함수를 직접 부르므로 앞 갈래는 못 보고, 루프를 끄면 그것만 죽는다. 그 시험은 회수가
  남기는 origin hop 기록(`Cancelled`)으로 좌변을 재며, pending 이 비었는지·caller 가 오류를
  받았는지는 단언하지 않는다. 재시작이 두 자료(연속 만료 계수와 거둔 id 목록)를 함께 비우는지는
  앞 갈래의 첫 시험이 잰다 —
  [ADR-0567](0567-a-scanned-id-is-consumed-once-by-structure-or-by-a-replay-test.md).)
- `handle_plugin_response` 의 `None` 갈래가 `expired_namespace_calls` 를 더 이상 안 볼 때.
  그러면 늦은 응답이 계수를 못 지워 위 대안 G 의 상태로 되돌아간다. (좌변에 붙은 판정기는
  `tasty-host-plugin` 의 `a_late_namespace_answer_also_clears_the_expiry_streak` ·
  `only_a_namespace_answer_clears_the_expiry_streak` 둘이다. 늦은 응답이 기억된 id 를 **한 번만**
  소비하는지는 `a_reaped_namespace_id_clears_the_streak_exactly_once` 가 잰다 —
  [ADR-0567](0567-a-scanned-id-is-consumed-once-by-structure-or-by-a-replay-test.md).)
- `restart_unresponsive_plugins` 가 `namespace_expiries` 를 더 이상 안 볼 때. 2026-09-20
  보강의 처방이 그 한 자리에 있으므로, 그것이 빠지면 반복 만료가 다시 로그뿐이 된다.
  (이 좌변에 붙은 판정기는 지금 없다 — 위 보강을 재는 것은
  `tasty-host-plugin` 의 `a_plugin_that_only_expires_namespace_calls_is_restarted` 다.)

- 늦은 응답이 무언가를 다시 진행시키게 됐을 때(2026-09-21 보강). 좌변에 붙은 판정기는
  `tasty-host-plugin` 의 `a_late_target_answer_does_not_run_the_post_hook_or_answer_twice` ·
  `a_late_pre_hook_answer_does_not_invoke_the_target_a_second_time` ·
  `a_late_post_hook_answer_does_not_answer_the_caller_a_second_time` 셋이다(위 보강의 세 갈래에
  하나씩). 세 시험은 만료가
  한 번 끝을 낸 것을 **먼저** 관측한 뒤 늦은 응답을 넣는다 — 그 관측이 없으면 "아무 일도 안
  했다" 와 "할 일이 없었다" 가 안 갈린다. 이 세 시험을 죽이는 변이는 현재 코드에 자연스러운 형태가
  없다(`None` 갈래가 진행시킬 재료를 안 들고 있다). 그래서 이것들은 앞으로 그 재료를 들고 오는
  변경 — 예컨대 만료된 요청의 kind 를 보관해 두는 변경 — 에 대한 회귀 시험이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 정상인데 `NAMESPACE_CALL_TIMEOUT` 을 넘기는 namespace 호출이 나타났을 때.
  재는 법: `~/.tasty/debug.log` 에서 `did not answer within` 경고를 찾고, 그 호출이
  실제로 응답을 만들어내고 있었는지(= 느린 것인가, 삼킨 것인가) target plugin 쪽
  로그와 대조한다.

## References

- [ADR-0078](0078-shutdown-rejects-pending-ipc.md) — 무응답 대신 즉시 거절. caller 에
  읽기 타임아웃을 걸지 않기로 한 근거가 거기 있다.
- [plugin-development](../dev-guide/plugin-development.md) "생명주기" — healthcheck
  상수와 75s 회수 상한.
- 코드 근거(결정이 실현된 현재 위치): `tasty-host-plugin` 의
  `manager::NAMESPACE_CALL_TIMEOUT` · `manager::PendingRequestKind` ·
  `PluginManager::dispatch_target_invoke` · `PluginManager::sweep_expired_requests` ·
  `PluginManager::expire_pending_request` · `PluginManager::settle_late_response`.
