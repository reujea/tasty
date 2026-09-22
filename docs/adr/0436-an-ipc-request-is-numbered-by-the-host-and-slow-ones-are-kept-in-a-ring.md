# ADR-0436: IPC 요청은 호스트가 번호를 매기고, 느린 요청은 그 번호로 plugin 대기까지 한 줄에 남긴다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, plugin, telemetry, pressure, correlation, diagnostics, compatibility, adr-0305, adr-0435

## Context

`system.pressure` 는 큐 대기 · handler · plugin 왕복을 **분포**로 준다([ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) · [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)). 분포는 "100 ms 를 넘은 것이 몇 건" 까지 말하고, **그 한 건의 시간이 어느 단계에 있었나** · **그 plugin 대기가 어느 요청의 것이었나** 는 말하지 못한다. 세 분포가 서로 다른 모수로 따로 접히기 때문이다.

그 둘을 잇는 값이 없었다. 결정 시점(`e33bc1758`)에 한 IPC 요청을 가리킬 수 있어 보이는 값은 일곱이었고, 어느 것도 요청의 수명 전체를 한 값으로 잇지 못했다.

- **JSON-RPC `id`** — 호출자가 고르는 값이다. 정적 CLI 단발 · plugin CLI 단발 · 원격 조회 · 호스트 주입(`HostIpcInjector::dispatch`)이 전부 `1` 을 싣는다. 동시에 떠 있는 요청 대부분이 같은 `id` 를 가지므로 진단의 열쇠로 쓰면 서로 다른 요청이 한 키로 섞인다. 응답이 잘못 배달되지 않는 것은 서버가 `id` 가 아니라 연결마다의 응답 통로로 답하기 때문이고, 그 설계는 옳다.
- **Event Bus envelope 의 `trace_id`** — 사건의 사슬을 잇는 값이다. IPC 요청과 무관하고, plugin 이 보낸 값을 그대로 싣는다(위조 가능).
- **`DispatchedIntent.trace_id`** — 모든 생성자가 `None` 이고 값을 넣는 두 helper 는 비-테스트 호출처가 없다. doc 은 "`None` 이면 bridge 가 새로 발급" 이라 했으나 그런 bridge 는 없었다.
- **호스트 req_id**(`next_request_id`) — host→plugin hop 마다 새로 받는다. plugin 이 받는 JSON-RPC id 가 이것이다. 원 요청과의 대응은 대기 표(`pending_requests`) 안에만 있고 응답이 매칭되면 버려졌다.
- 그 밖에 연결의 peer 주소 · telemetry seq(한 호출이 여러 개를 뽑는다) · 멱등 키(호출자 값, forward 에는 안 실린다).

plugin 쪽에서 되짚을 길도 없었다. 호스트가 req_id 를 남기는 자리는 plugin 이 오류로 답한 경고 하나였고, namespace 만료 경고는 plugin id 만 찍었다.

## Decision

**호스트가 요청마다 번호를 매기고, 그 번호로 plugin 대기까지 한 줄에 남긴다.** 부분은 다섯이다.

1. **번호** — `tasty_ipc::server::RequestSeq`. 프로세스 전역 단조 카운터에서 1 부터 받는다. `IpcCommand` 의 생성자가 받으므로(필드가 비공개라 크레이트 밖에서 리터럴로 못 만든다) 소켓 경로와 호스트 주입이 둘 다 번호를 갖는다. **이름에 `trace` 를 쓰지 않는다** — 이미 셋이 그 이름을 쓰고, 넷째가 되면 "RPC id · event trace 와 구분" 이 이름부터 깨진다. JSON-RPC `id` 에서 파생하지 않는다. 멱등 relay 가 키를 뗀 사본을 다시 쌀 때는 `IpcCommand::continuing` 으로 원 번호를 그대로 든다 — 새 번호를 받으면 요청 하나가 번호 둘로 갈린다.
2. **plugin forward 로 전달** — `forward_namespace_call` 이 원 번호를 받는다(GUI routing · headless forward 는 `Some(cmd.request_seq())`, 파일 핸들러 큐의 forward 는 `None` — 그 큐는 IPC `file_handler.dispatch` 로도 차지만, 번호가 intent → `pending_handler_ipc` 로 옮겨지는 사이에 떨어진다. 아래 "잃은 것"). 번호는 `FinalCaller::Local` 안에 실려 회신처와 **한 몸으로** pre-hook → target → post-hook 사슬을 따라가고, 각 hop 을 대기 표에 넣는 자리가 거기서 읽어 `PendingRequest.origin`(변종이 아니라 구조체 칸 하나)에 복사한다. IPC 큐를 안 지난 plugin 요청(event.dispatch · surface · plugin 이 부른 namespace)도 `None` 이다.
3. **관측** — `system.pressure` 의 열한째 덩어리 `slow_requests`. 메모리 전용 고정 용량 링이고, 넣는 기준은 **큐 대기 · 호스트 처리 · plugin 대기의 합이 문턱 이상인 요청**이다. 한 줄은 `request_seq` · `host`(`method`(canonical 이름. alias 해석은 모르는 이름을 받은 그대로 통과시키므로 plugin namespace 메서드나 없는 이름은 호출자 문자열 그대로이고, 그래서 `MAX_METHOD_BYTES` 에서 자른다) · `caller`(봉투가 말한 local/agent) · `queue_wait_us` · `host_us`) · `plugin_hops`(hop 마다 `plugin_id` · `host_request_id` · `wait_us` · `outcome` = ok/error/expired/cancelled) · `total_us` 다. 덩어리에는 `threshold_us` · `capacity` · `admitted`(켜진 뒤 링에 든 누계) · `rows` 가 함께 나간다. 기존 열 덩어리는 한 글자도 안 바뀐다.
   - 호스트 몫은 GUI `process_ipc` 와 headless `pump_ipc` 가 **같은 자리**에서 넘긴다(`app::ipc_round::CommandObservation` — 명령을 꺼낸 직후 `begin`, 다 다룬 직후 `finish`). 큐 대기 계측도 그 `begin` 으로 옮겨, 두 루프에 한 자리씩만 남는다.
   - plugin 몫은 매니저가 넘긴다. 번호를 든 요청을 대기 표에 넣을 때 링에 **열린 자리**를 먼저 만들고(forward 는 dispatch 안에서 일어나므로 호스트 몫보다 먼저다), 응답 · 만료 · 취소 때 그 hop 을 같은 줄에 붙인다. 열린 줄은 합이 문턱을 넘는 순간 링으로 옮겨지고, 사슬이 끝났는데 문턱 아래면 치워진다.
   - `system.pressure` 자신은 넣지 않는다 — 조회가 링을 밀어내면 조회할 때마다 원인 요청이 한 칸씩 사라진다.
4. **plugin 쪽 되짚기** — 이미 있던 `warn` 두 줄(plugin 오류 응답 · namespace 만료)에 호스트 req_id 와 원 요청 번호를 **같은 줄에** 더한다(`(id=…, request_seq=…)`, 번호가 없으면 `none`). 새 줄은 만들지 않고, caller 에 가는 오류 문구는 그대로다.
5. **이름 구분 정리** — `DispatchedIntent.trace_id` 의 틀린 doc 을 사실대로 고친다(발급하는 쪽이 없다, Event Bus `trace_id` 와 무관하다, IPC 요청의 번호는 `RequestSeq` 다). **칸은 지우지 않는다.**

**하지 않는 것**:

- plugin wire(`tasty-plugin-protocol` 의 `IpcInvokeParams` 등)에 번호를 싣지 않는다. 그 크레이트는 번들 plugin 9 개 전부의 의존 폐포 안이라 9 개 bump + 매니페스트 + `Cargo.lock` 이 따라오고, RF25 의 plugin 축 capability 자리와 겹친다. 호스트 쪽 대응표(대기 표의 `origin` + 경고 줄의 `id=`)로 되짚는다.
- plugin → host host-call 을 그 plugin 이 처리하던 host→plugin 요청과 **추측으로 잇지 않는다.** 같은 plugin 이 동시에 여러 요청을 처리하면 어느 것이 부모인지 호스트는 모른다 — 추측하면 틀린 연결을 만든다.
- 번호를 메트릭 레이블로 쓰지 않는다. 분포는 지금처럼 레이블 없는 전역 집계다.
- params 원문 · session token · 멱등 키 · JSON-RPC `id` 값을 싣지 않는다.

**상수**(코드 상수이고 디자인 값이 아니다 — `tasty_telemetry::slow_requests`):

- `SLOW_REQUEST_THRESHOLD` = 100 ms. 파생값이 아니다. ① [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) 이 격리 인스턴스에서 잰 정상 부하의 최댓값이 큐 대기 25.4 ms · handler 0.48 ms 였다 — 그보다 네 배 위라 정상 요청은 안 든다. ② 분포 버킷 경계의 한 칸(100 ms)과 같아, 분포에서 "100 ms 를 넘은 것이 몇 건" 을 읽은 자리에서 그 건들의 줄을 찾을 수 있다.
  - **①은 plugin 왕복을 안 쟀다 — 그 모수에서는 이 문턱이 plugin forward 전부를 넣는다.** 실측 2026-09-21(격리 홈 · debug · 번들 plugin 재빌드 후): 빈 결과를 내는 `markdown.recent` · `image.list` 의 plugin 대기가 gui 에서 130.8–135.3 ms(8 건), headless 에서 123.3–124.1 ms(3 건)였고, 한 건도 100 ms 아래가 없었다. 그래서 지금 이 문턱에서는 **plugin 으로 넘긴 요청이 전부** 링에 들고, plugin 호출이 32 번 이어지면 그 앞의 느린 호스트 요청이 밀려난다.
  - 그래도 100 ms 로 둔다. 할 일이 없는 조회의 왕복이 130 ms 인 것은 그 자체로 조사할 지연이고, 문턱을 그 위(예: 다음 버킷 316 ms)로 올리면 **그 기준선이 링에서 안 보이게 된다** — 링이 "무엇이 느린가" 에 답하려고 있는 것인데 가장 흔한 느림을 치우는 셈이다. 외부 계약도 안 바뀐다(`threshold_us` 가 응답에 함께 나가므로 읽는 쪽이 값을 가정하지 않는다). 기준선의 원인이 잡혀 plugin 왕복이 문턱 아래로 내려오면 이 항은 ①과 같은 근거로 선다.
  - **보강(2026-09-23) — 기준선의 원인이 잡혀 고쳐졌다.** 호스트 ↔ plugin 메인 채널 양 끝에 `TCP_NODELAY` 가 없었고, 메시지를 쓰는 `writeln!` 이 본문과 개행을 두 번에 나눠 써서 개행 조각이 받는 쪽의 지연 ACK(실측 40.0–40.2 ms)를 기다렸다. 빈 결과 namespace 호출 하나가 그런 hop 셋을 지나 ~120 ms 가 됐다. 양 끝에 Nagle 을 끈 뒤 같은 호출의 plugin 대기는 9 건 최대 1.5 ms · 평균 1.24 ms(Linux 헤드리스 debug, `plugin_round_trip`)이고 링에 한 건도 안 들었다. 그래서 이 항은 이제 ①과 같은 근거로 선다 — 문턱을 그대로 둔 결정은 바뀌지 않는다. gui 조합의 재측은 미측정이다(같은 소켓 코드를 쓴다). 상세 [`plugin-development.md` "전송 지연"](../dev-guide/plugin-development.md#전송-지연-nagle-금지).
- `SLOW_REQUEST_CAPACITY` = 32. 파생값이 아니다. 한 번의 조회로 최근의 느린 요청을 훑기에 충분하고 상한에서 수십 KB 다.
- `OPEN_FORWARD_CAPACITY` = 256. 동시 IPC 연결 상한([ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) 의 256)과 같다 — 소켓 연결 하나는 요청 하나를 기다리므로 동시에 plugin 을 기다리는 IPC 요청이 대개 이 안에 든다.
- `MAX_PLUGIN_HOPS` = 3(pre-hook · target · post-hook).
- `MAX_METHOD_BYTES` = 128. 파생값이 아니다. 메서드 칸은 호출자 문자열이라, 자르지 않으면 한 호출자가 긴 이름으로 링 32 줄을 각각 임의 길이로 채울 수 있다. 실측 2026-09-21: 등록 메서드 표(`METHOD_TABLE`, 286 개)의 가장 긴 이름이 31 바이트(`markdown_mirror.content_request`)였고, debug 표(`DEBUG_METHODS`, 52 개)의 가장 긴 이름이 32 바이트(`debug.event_bus.list_subscribers`)였다. 세는 법: 두 표 본문에서 주석을 지운 뒤 `(` 다음의 첫 문자열 리터럴을 여러 줄 튜플까지 포함해 센다(줄 단위로 세면 여러 줄 튜플이 빠진다). 최장 32 의 네 배라 정상 이름은 잘리지 않는다. 상한을 넘으면 그 안의 마지막 char 경계에서 자른다(잘렸다는 표지는 따로 없다 — 길이가 상한 근처면 잘린 것이다).

## Consequences

- **얻은 것**:
  - 느린 요청 한 건의 시간이 큐 · 호스트 · plugin 중 어디에 있었는지가 한 줄로 보인다.
  - plugin 로그의 req_id 에서 호스트 쪽 원 요청으로, 원 요청에서 plugin 로그로 양방향으로 되짚을 수 있다.
  - 만료된 forward 는 분포(`plugin_round_trip`, 응답이 매칭된 것만 센다)에 끝내 안 남지만, 링에는 `expired` 로 남는다.
- **잃은 것**:
  - 요청마다 메서드 이름 하나를 복사하고(링에 넣지 않을 요청도), 뮤텍스를 한 번 잡는다. 열린 표 탐색은 선형(상한 256)이다.
  - 모수가 **호스트 IPC 큐를 지난 요청**뿐이다. plugin 이 부른 host-call 은 `IpcCommand` 를 안 지나 번호가 없고 링에 안 든다(`handle_checked_request` 로 곧장 간다). plugin 이 부른 namespace forward 도 번호가 없다.
  - IPC `file_handler.dispatch` 가 plugin 핸들러로 가는 forward 는 **IPC 큐를 지났는데도 번호가 없다.** 명령이 intent → `enqueue_handler_ipc` → `pending_handler_ipc` → `dispatch_pending_handler_ipc` 를 거치는데, 그 큐의 튜플 `(method, target)` 에 번호 칸이 없다. 잇는 방법은 intent 와 큐 튜플에 번호 칸을 두는 것이다. 다만 그 forward 는 caller 에게 답하지 않는 fire-and-forget 이라(`file_handler.dispatch` 의 답은 큐에 넣는 순간 나간다) 이어도 "이 요청이 왜 느렸나" 에 답하는 줄이 되지 않는다. 그래서 미뤘다.
  - `host_us` 는 꺼낸 뒤 호스트가 명령을 다 다루기까지(게이트 포함)라 `handler_after_gate` 와 모수가 다르다. 이름을 다르게 둔 것이 그 표시다.
  - 링은 재시작하면 비고, 번호도 1 부터 다시 센다.
- **운영 비용 / 유지 부담**:
  - 새 hop 종류(대기 표에 원 번호를 싣는 변종)가 생기면 그 hop 이 끝나는 자리 셋(응답 · 만료 · 취소)이 `record_origin_hop` 을 지나는지 봐야 한다. 대기 표에서 빠지는 길이 그 셋뿐이라는 전제가 깨지면 열린 줄이 상한까지 남는다(자라지는 않는다).
  - 사슬이 끝났는지(`last`)는 hop 의 종류로 정한다. pre-hook 은 늘 이어지고, post-hook 이 걸린 target 은 응답이면 이어진다. filter 차단 · post-hook 송신 실패처럼 예외적으로 끊기는 경우 그 열린 줄은 문턱 아래면 상한에서 밀려날 때까지 남는다.

### 칸을 지우지 않은 이유 — `DispatchedIntent.trace_id`

호환이 가장 많이 보존되는 쪽을 골랐다. 그 칸은 debug 빌드의 intent watch 로그에 `trace_id` 필드로 찍히고, 지우면 그 로그의 모양이 바뀐다. 또 값을 넣는 두 helper 는 `#[allow(dead_code)]` 두 자리이고, 억제 수는 상한 래칫(`scripts/check-allow-reason.sh`)이라 줄어도 상한을 함께 옮겨야 한다 — 이 결정의 물음(요청을 가리키는 값을 구분하는가)과 무관한 움직임이다. 틀린 것은 칸이 아니라 doc 이었으므로 doc 을 고쳤다.

## Alternatives Considered

- **JSON-RPC `id` 를 열쇠로 쓴다** — 호출자 값이라 거의 늘 `1` 이고, 충돌하고, 문자열이면 원문이 샐 수 있다. 기각.
- **Event Bus `trace_id` 를 IPC 로 넓힌다** — plugin 이 위조할 수 있고 사건 사슬이라는 다른 물음에 답하는 값이다. 두 물음을 한 값에 실으면 "구분" 이 깨진다. 기각.
- **번호를 plugin wire 에 싣는다**(`IpcInvokeParams` 에 칸 추가) — plugin 이 원 요청을 직접 안다는 이점이 있다. 번들 plugin 9 개 bump 폐포이고 RF25 plugin 축과 겹친다. 이 결정에서는 빼고 재검토 조건으로 남긴다.
- **전 요청을 링에 넣는다** — 정상 요청이 링을 곧바로 밀어내 원인 요청이 안 남는다. 기각.
- **"가장 느린 N" 을 프로세스 수명 동안 든다** — 오래 전의 한 건이 영구히 자리를 차지해 지금의 원인을 못 보여 준다. 문턱 + 오래된 것부터 밀어내기를 골랐다.
- **호스트 몫과 plugin 몫을 따로 두 링에 남긴다** — 운영자가 두 목록을 번호로 손수 맞춰야 하고, 한쪽이 먼저 밀려나면 맞출 짝이 없다. 기각.
- **원 번호를 사슬 함수의 인자로 따로 넘긴다** — hop 을 부르는 함수 둘이 인자 여덟 개가 되어 `clippy::too_many_arguments` 억제 두 자리를 새로 들여야 했다(억제 수는 상한 래칫이다). 회신처와 함께 사슬을 따라가야 하는 값이므로 `FinalCaller::Local` 에 실었다 — 대기 표의 `origin` 칸은 그대로 두고, 넣는 자리가 거기서 복사한다.
- **plugin host-call 에도 번호를 준다** — 번호를 받는 것 자체는 쉽지만, 그 요청은 큐를 안 지나 큐 대기가 없고 호출 사슬을 잇지 못하면(위 "하지 않는 것") 번호가 가리키는 것이 plugin 의 `call_id` 와 같아진다. 이 결정에서는 빼고 한계로 적는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- `tasty-plugin-protocol` 의 `IpcInvokeParams` 에 원 요청을 가리키는 칸이 생긴다(RF25 plugin 축). 그러면 대기 표 대응표는 plugin 쪽 직접 값과 중복이 된다.
- plugin → host host-call 이 `IpcCommand` 를 지나게 바뀐다. 그러면 그 호출도 번호를 받아 이 링의 모수에 들어온다 — "모수는 호스트 IPC 큐를 지난 요청" 문장이 바뀐다.
- 원 번호를 든 `PendingRequest` 가 대기 표에서 빠지는 길이 응답(`handle_plugin_response`) · 만료(`sweep_expired_requests`) · 취소(`cancel_pending_namespace_calls`) 말고 하나 더 생긴다. 넷째 제거 자리 `reclaim_requests_sent_to` 는 같은 plugin 의 취소가 원 번호를 든 변종(namespace 셋 · IPC hook 둘)을 먼저 거둔 뒤에 돌아 지금은 그런 항목을 안 만난다.

**원리적으로 안 붙는 것**

- 링이 상시 넘친다(`admitted` 가 `capacity` 보다 훨씬 빨리 는다). 문턱이 낮거나 용량이 작다는 뜻이다. 재는 법: 부하 중 `tasty list pressure` 를 두 번 떠 `slow_requests.admitted` 의 증가와 `rows` 길이를 견준다.
- 정상 부하에서 문턱을 넘는 요청이 흔해진다(예: 느린 plugin 이 기본 번들이 된다). 재는 법: 같은 응답의 `plugin_round_trip.us_hist` 에서 100 ms 칸 위의 비율을 본다. **plugin forward 에 대해서는 결정 시점에 이미 충족돼 있다**(위 "상수" 의 실측 — 왕복 전부가 100 ms 칸 위). 그 기준선의 원인이 밝혀져 **고칠 수 없는 것**으로 판정되면 문턱을 plugin 몫과 호스트 몫으로 가르거나 올리는 것을 다시 본다. (2026-09-23 보강: 원인이 밝혀져 고쳐졌다 — 위 "상수" 의 보강 항. 이 조건은 plugin forward 에 대해 더 이상 충족돼 있지 않다.)

## References

- 조사 근거: RF16 correlation 조사(식별자 일곱 · host→plugin 경로별 끊김 · RPC id 가 거의 늘 1 이라는 측정)는 결정 시점의 소스를 직접 읽은 것이고, 그 결론은 위 Context 에 옮겨 적었다.
- 인접 결정: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)(압력은 프로세스 게이지) · [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)(열 덩어리) · [ADR-0085](0085-ipc-log-retention-bounded.md)(호출당 영구 기록 억제) · [ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md)(forward 에 멱등 키를 안 싣는다) · [ADR-0311](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md)(namespace 만료).
- 코드 근거(현재 위치, 심볼 이름): `tasty_ipc::server::RequestSeq` · `IpcCommand::request_seq` · `IpcCommand::continuing` · `tasty_telemetry::slow_requests::SlowRequestLog` · `app::ipc_round::CommandObservation` · `PluginManager::insert_pending` · `PluginManager::record_origin_hop` · `FinalCaller::origin` · `handler::pressure::slow_requests_json`.
- 후속: [ADR-0468](0468-the-slow-request-host-part-carries-the-answer-the-caller-got.md) — 호스트 몫에 호출자가 받은 답(`outcome` · `error_code`)을 더했다.
- 문서: [`docs/features/telemetry/index.md`](../features/telemetry/index.md) · [`docs/reference/api.md`](../reference/api.md).
