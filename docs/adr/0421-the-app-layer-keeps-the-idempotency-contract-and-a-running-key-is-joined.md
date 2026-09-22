# ADR-0421: App 층도 멱등 키 계약을 지킨다 — 진행 중인 키에 온 재시도는 첫 실행에 합류한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, idempotency, retry, app-layer, concurrency, deferred-response, headless, adr-0338, adr-0361
- **Group**: ipc-contract

## Context

[ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
의 보존소는 engine 라우터(`route_checked_request`) 한 자리에서만 키를 봤다. 그 앞의 **App 층** —
`App` 이 직접 끝내는 메서드 — 에는 `Mutate` 가 여섯 있었고(`window.create` · `view.create` ·
`ui.screenshot` · `remote.attach` · `plugin.install` · `plugin.request_permission`), 거기 키를 실은
재시도는 그대로 두 번째 효과를 남겼다. 창이 둘 생기고, 스크린샷이 두 번 찍혔다.
`plugin.request_permission` 은 결이 다르다 — 핸들러(`approval::publish_capability_elevation_at`)가
같은 agent+permission 의 **Pending** 격상이 있으면 그것을 재사용하므로, 승인 전의 재시도는 키 없이도
같은 레코드를 가리킨다. 그 메서드에서 두 번째 효과가 나는 것은 격상이 이미 **결정된**(승인 · 거절)
뒤에 재시도가 도착해 새 승인 요청을 여는 갈래다. ADR-0338 은 그 배선을 후속 조각으로 남겼다.
plugin namespace forward 는
[ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md) 이 계약
밖으로 선언했다.

App 층이 engine 라우터와 다른 점은 둘이다.

- **답을 반환하지 않고 통로에 보낸다.** 핸들러가 `IpcCommand::response_tx` 로 직접 보내고, 그중
  일부는 **나중에** 보낸다 — `window.create` 는 winit 핸들러가 창을 만든 뒤 완료 채널로
  ([ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md)), `remote.attach` 의 새 워크스페이스
  갈래는 워커가 SSH 수립 뒤에 보낸다. 그래서 `begin → handler → finish` 의 "handler 가 돌아온 뒤
  기록한다" 가 성립하지 않는다.
- **진행 중 상태가 실재한다.** engine 라우터에서는 실행이 직렬이라 둘째 요청이 볼 때 첫째가 이미
  끝나 있었다 — 보존소가 진행 중을 뜻하는 값을 두지 않은 근거가 그것이었다. App 층의 지연 응답
  사이에는 같은 키가 올 수 있다(응답 대기 상한 `response_timeout_ms` 가 만료된 호출자는 결과를 모른 채
  다시 보낸다).

App 층 가로채기는 두 조합에 따로 있다 — GUI 의 `ipc_step_app_methods` 와 헤드리스의
`intercept_app_layer`. 헤드리스에는 여섯 중 `plugin.request_permission` 하나만 있고 나머지는 engine
라우터로 떨어진다(거기서 기존 보존소가 받는다).

## Decision

**App 층의 두 가로채기가 첫 줄에서 `idempotency::run_app_layer` 를 부른다. 보존소에 진행 중 상태를
더하고, 진행 중인 키에 같은 요청이 오면 두 번째 실행 없이 첫 실행에 합류시킨다.**

- **판정은 engine 라우터와 같다.** 키가 있고 `MethodEffect::Mutate` 인 이름만 개입한다(이름은 alias
  정규화 뒤). 재생 · 충돌(`-32063`) · 버려짐(`-32064`)의 답과 보존 상한도 그대로다.
- **통로를 relay 로 바꾼다.** 처음 보는 키면 보존소에 진행 중 항목을 열고, 칸 하나짜리 새 통로와 그
  통로를 기다릴 relay 스레드를 세운 뒤 **키를 뗀** 요청 사본으로 원래 본문을 다시 부른다. 키를 떼는 것이 재귀를 끊는다 — 떼지 않으면
  본문이 같은 함수로 돌아와 자기에게 합류한다. 본문이 그 이름을 맡았으면(GUI `IpcStep` 이
  `NotHandled` 가 아님, 헤드리스 `Some`) relay 스레드가 그 통로의 답을 기다렸다가 **먼저 기록하고,
  합류자에게 나눠 준 뒤, 원래 통로로 넘긴다.** 기록이 먼저라 답을 받은 호출자의 재시도는 반드시
  기록을 본다.
- **이 층이 이름을 안 맡으면 자국을 남기지 않는다.** 본문이 `NotHandled` 면 연 항목을 그 자리에서
  닫고 그 판정의 셈도 되돌린다([ADR-0422](0422-the-retry-counts-are-kept-by-the-store-that-decides.md)).
  relay 스레드는 본문이 relay 통로를 놓는 즉시(통로 끊김) 끝난다. 요청은 다음 층으로 가고 거기서
  처음 보는 키로 다시 판정된다. 그래서 engine 라우터로 가는 `Mutate` 는 App 층을 지나며 항목을 한 번
  열고 닫을 뿐 응답이 안 바뀐다.
- **relay 스레드를 못 세우면 보장만 잃는다.** 스레드는 `std::thread::Builder` 로 세우고(실패하면
  `std::thread::spawn` 은 패닉하고, 부르는 자리는 GUI 이벤트 루프 · 헤드리스 펌프다), 본문을 부르기
  **전에** 세운다. 실패하면 연 항목을 닫고 키를 뗀 요청을 **원래 통로로** 부르며 `warn` 을 남긴다 —
  동작은 키가 없을 때와 같고, 같은 키의 재시도는 다시 실행된다. 본문을 부른 뒤에 세우면 이 갈래가
  없다: 답이 이미 relay 통로에 있거나 나중에 그리로 오는데, 원래 통로로 넘겨 줄 쪽이 없다.
- **합류.** 같은 `(주체, 키, 요청)` 이 진행 중이면 합류자의 `id` 와 통로를 그 항목에 붙이고 끝난다. 첫
  결말이 보관되면 합류자는 그 답을 `idempotent_replay: true` 와 **자기** `id` 로 받는다. 답이 상한을
  넘어 버려졌으면 `-32064` 를 받는다. 진행 중인데 요청이 다르면 합류가 아니라 충돌(`-32063`)이다.
- **결말 없이 통로가 닫히면 잊는다.** 본문이 답을 안 보내고 통로를 버린 경로에서는 연 항목을 닫는다.
  합류자의 통로도 함께 버려져 첫 요청과 같은 결말(응답 없이 연결 종료)을 받는다. 다음 재시도는
  처음 보는 키로 실행된다 — 결말을 모르는 키를 "실행됐다" 로 기억하면 한 번도 성공 못 한 요청이 영원히
  막힌다.
- **늦은 결말은 남의 자리를 안 덮는다.** 진행 중 항목은 여는 쪽이 받은 표(`ticket`)를 든다. 상한에
  밀려난 뒤 같은 키를 다른 실행이 쥐었으면, 늦게 온 첫 결말은 기록하지 않는다.
- **engine 라우터에는 진행 중이 도달하지 않는다.** 같은 `(주체, 키, 요청)` 은 같은 메서드라 같은 층으로
  가고, 진행 중은 App 층만 연다. 닿는다면 라우터는 기다릴 수 없으므로 `-32061`(결과 불명 — 재전송하지
  말고 조회하라)로 답한다. 새 오류 코드를 만들지 않는다.
- **시각은 `Instant::now()` 다.** relay 스레드는 `Core` 를 들고 있지 않고, 여는 쪽과 닫는 쪽이 같은
  원천을 봐야 보존 시간이 한 시계로 잰다.

**남는 구멍 하나.** GUI 의 debug step(`debug_methods.rs` · `window_required.rs`)은 app_methods step
**뒤**라, 거기 있는 `Mutate`(입력 주입 · `debug.lua.eval` 등)는 연 항목이 닫힌 뒤 보존소 없이 실행된다.
사용자 입력 재현이라 release 에 없는 표면이고, 이 결정은 그 층을 넓히지 않는다.

## Consequences

- **얻은 것**: App 층 `Mutate` 여섯의 재시도가 두 번째 효과를 안 남긴다. 창 생성처럼 답이 늦는 메서드에서
  동시에 온 같은 키가 한 실행으로 수렴한다.
- **얻은 것**: 두 조합이 같은 함수를 부른다. 헤드리스의 `plugin.request_permission` 도 같은 보장을
  받는다 — 격상이 결정된 뒤에 온 재시도가 새 승인 요청을 여는 대신 앞선 답을 재생으로 받고, 같은 키로
  다른 권한을 청하면 둘째 격상을 여는 대신 `-32063` 을 받는다.
- **잃은 것 — 키를 실은 App 층 호출 하나가 스레드 하나를 쓴다.** 답이 올 때까지 relay 스레드가 기다린다.
  키가 없는 호출, `Mutate` 가 아닌 호출은 스레드를 안 세운다. 키를 실은 `Mutate` 가운데 이 층이 안 맡는
  호출(engine 라우터로 가는 것)도 스레드를 **잠깐** 세운다 — 스레드가 본문보다 먼저 서야 하기 때문이고,
  본문이 통로를 놓는 즉시 끝난다. 스레드 수의 상한은 보존소 항목 수 상한과 같지 않다 — 진행 중 항목이
  밀려나도 그 스레드는 답이나 통로 끊김까지 산다.
- **잃은 것 — 요청 사본 하나.** relay 는 키를 뗀 요청을 복제한다. params 크기만큼이다.
- **운영 비용**: 보존소가 진행 중 상태를 다룬다(항목 · 합류자 목록 · 표). 기존 engine 라우터 경로는
  그 갈래에 닿지 않는다.

## Alternatives Considered

- **여섯 핸들러 자리마다 `begin`/`finish` 를 둔다** — 지연 응답 메서드는 `finish` 자리가 핸들러 밖(winit
  핸들러 · 워커)이라 그 경로마다 보존소를 들고 가야 한다. 다섯 파일에 흩어지고, 새 App 층 메서드가
  빠뜨린다. 가로채기 첫 줄 한 자리에서 통로를 바꾸면 핸들러는 모른다.
- **진행 중인 키의 재시도를 거절한다(새 오류 코드)** — 호출자가 다시 기다려야 하고, 새 코드는 구 client
  가 모르는 값이다. 합류는 새 코드 없이 호출자가 원래 원하던 답(첫 실행의 결말)을 준다.
- **진행 중인 키의 재시도를 `-32061` 로 답한다** — 코드는 있지만 호출자는 결과를 조회해야 하고, 조회할
  수 없는 결말(새 창 id)도 있다. 합류가 더 많은 것을 준다. engine 라우터에서 도달 불가한 갈래에만
  이 답을 쓴다.
- **relay 없이 App 층 메서드를 동기로 바꾼다** — 창 생성을 기다리는 동안 이벤트 루프가 막힌다
  (ADR-0122 가 지연 응답을 고른 이유). 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 합류가 사라지면(진행 중인 키가 두 번째 실행을 내면)
  `idempotency::tests::a_retry_that_arrives_while_the_first_is_running_joins_it` 가 빨개진다(변이 확인:
  합류를 항상 실패시키면 그 시험과 `a_run_that_drops_its_reply_is_forgotten_with_its_joiners` 가
  실패했다, 2026-09-21).
- relay 스레드를 못 세운 갈래가 연 항목을 안 닫거나 답을 원래 통로로 안 보내면
  `idempotency::tests::a_relay_that_cannot_start_runs_the_request_without_the_guarantee` 가 빨개진다
  (시험은 실패하는 생성 함수를 넣는다. 변이 확인: 그 갈래의 닫는 줄을 지우면 실패했다, 2026-09-21).
  실제 OS 의 스레드 생성 실패는 재지 않았다.
- 이 층이 안 맡은 이름의 항목을 안 닫으면
  `idempotency::tests::a_layer_that_does_not_handle_the_name_leaves_no_trace` 가 빨개진다(변이 확인: 닫는
  줄을 지우면 실패했다, 2026-09-21).
- App 층 가로채기가 debug step 까지 한 함수로 합쳐지면 — 위 "남는 구멍" 이 닫히는 날이다. 그때 이름 표의
  debug 이름 선언을 함께 고친다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **두 조합의 가로채기가 실제로 이 함수를 지나는가.** 단위 시험은 함수만 잰다. 재는 법: 격리
  `TASTY_HOME` 의 debug 인스턴스에 같은 키로 `window.create` 를 두 번 보내 창 수와 둘째 답의
  `idempotent_replay` 를 본다(GUI), 헤드리스에 같은 키로 `plugin.request_permission` 을 두 번 보내
  둘째 답의 `idempotent_replay` 를 본다. 그 메서드에서 `approval.list` 의 레코드 수는 판별력이 없다 —
  핸들러가 같은 (agent, permission) 의 pending 격상을 스스로 합쳐, 키 없이 다시 불러도 레코드가 하나다
  (실측 2026-09-21: 키 없는 재호출이 같은 `approval_id` 를 받았다).
- **relay 스레드가 쌓이는가.** 재는 법: 키를 실은 App 층 호출을 반복한 뒤 프로세스 스레드 수
  (`/proc/<pid>/status` 의 `Threads`)가 답이 끝난 호출만큼 줄어드는지 본다.

## References

- 관련 ADR: [ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
  — 멱등 키 계약. 이 ADR 이 그 결정이 후속으로 남긴 App 층 배선을 확정한다.
  [ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md) — forward 는
  계약 밖. [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md) — 창 생성의 지연 응답.
  [ADR-0420](0420-the-idempotency-key-envelope-is-judged-at-the-admission-gate.md) — 봉투 검사 위치.
- 관련 dev-guide: [api-conventions](../dev-guide/api-conventions.md) 의 멱등 키 절.
- **코드 근거 (결정이 실현된 현재 위치)**: `idempotency::run_app_layer` · `Relay::run` · `spawn_relay` · `Store::open` ·
  `Store::join` · `Store::abandon` · `Store::settle`, `App::ipc_step_app_methods` 첫 줄,
  `headless_dispatch::intercept_app_layer` 첫 줄.
