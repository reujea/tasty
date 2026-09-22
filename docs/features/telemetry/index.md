# 텔레메트리 (Telemetry)

- **Status**: Implemented
- **주체**: AI Agent — 측정 대상이자 **통제 주체**다. 에이전트가 다른 에이전트에게 cap 을 걸고 지우고 푼다(`agent` 는 파라미터로 지목한다). 로컬 사용자도 CLI 로 같은 것을 할 수 있으나 1급 대상이 아니다 — headless 인스턴스에는 로컬 사용자가 아예 없다([identity](../../identity.md) §2.2).
- **ADR**: [0246](../../adr/0246-telemetry-has-no-opt-out-and-its-token-is-a-declaration.md) — 옵트아웃 축을 두지 않는다, 권한 토큰은 선언이다
- **코드**: `telemetry.*` 핸들러, `tasty-telemetry`, 영속 `tasty-memory`
- **화면**: 없음 (cap 발화 시 알림)
- **메서드 목록**: [reference/api](../../reference/api.md#텔레메트리-telemetry)

## 목적

AI 에이전트 활동을 도메인 메트릭으로 **기록·집계·차단**하는 관측 계층. 비용(토큰)·호출량을 추적하고 임계 초과 시 액션을 발화한다.

## 내부 동작

### 모델

Metric(`input_tokens`/`ipc_calls`/…) × Agent(`tasty.<plugin_id>`/`cli.<exe>`/`_host`, 미명시 시 CallerContext 자동) × Workspace(없으면 `global`) × Op(`Set`/`Inc`, 시계열은 둘 다 sum) × Window(`1m/5m/1h/1d`) × Tags. 이벤트는 `tasty.telemetry.event.{ts}.{seq}` 로 영속, 조회는 prefix scan + 순수 집계(재시작 후 누적 보존).

**집계본은 영속되지 않는다.** bucket 은 조회할 때마다 raw event 로부터 새로 만들어지고 버려진다 — 주기 rollup task 도, `tasty.telemetry.bucket.*` 키도 없다. 따라서 **raw event 보존량이 곧 조회 가능 범위**이며, 그 상한은 관측 로그 3종 공통 정책(`store::log_retention`)이 정하는 **최근 20,000 이벤트**다. 조용한 인스턴스에서는 수일치, 폴링이 도는 인스턴스에서는 수십 분치가 되므로 조회 범위가 데이터 양에 종속된다. 롤업을 신설하지 않기로 한 근거와 재검토 조건은 [ADR-0085](../../adr/0085-ipc-log-retention-bounded.md).

`ipc_calls` 는 dispatcher 가 plugin IPC 호출마다 자동 1회 기록(`tags.method`). `_host` 또는 `telemetry.*` 는 자기측정/재귀 방지로 skip.

### IPC 진입 관측

GUI·headless의 외부 소켓과 plugin host-call은 라우팅 전에 권한·cap·rate-limit을
검사한다. 통과한 호출만 `ipc_calls`로 한 번 기록하며 App 인터셉트, namespace forward,
전 창 합산, 일반 handler 사이에 예외를 두지 않는다. 일반 handler로 내려가도 다시
소비하거나 기록하지 않는다. 별칭은 canonical 메서드 태그로 남는다.
권한/한도 거부는 Allow 관측을 남기지 않고 기존 Deny audit·권한 격상 흐름을 유지한다.
`telemetry.*`와 호스트 caller의 관측 제외, Local의 rate 면제는 그대로다.
[ADR-0277](../../adr/0277-ipc-admission-and-observation-run-once.md).


### Cost Cap

(agent, metric, window) raw sum 이 `threshold` 이상이면 `triggered` + 액션:

| 액션 | 후속 IPC |
|------|----------|
| `notify` | 변화 없음 (알림만) |
| `pause` | 그 plugin agent 의 모든 IPC `-32007 cap_blocked`(과거 `stop` 값으로 저장된 cap 은 `pause` 로 자동 마이그레이션됨) |
| `require_approval` | `approval.request`(severity=warn) 자동 + IPC 차단 → 사용자 응답 후 `cap.reset` 으로 재개 |

차단된 plugin 본인은 `cap.reset` 도 막힌다 — **Local(CLI)만 reset**. 누적 임계인 cap 과 *시간당 비율*인 [agent rate-limit](../agent-collaboration/index.md) 은 다른 시스템.

### 이상 탐지

`AnomalyDetector` 가 세 휴리스틱을 유지한다 — **진짜 정체/누수 탐지가 아니라 값싼 신호 기반 후보 알림**이라는 전제로 소비해야 한다.

| 휴리스틱 | 판정 | 기본값 |
|---|---|---|
| `CallBurst` | (agent, method) sliding window 호출 카운트 | 1분 1000회↑ |
| `SlowLoop` | (agent, method, params-hash) sliding window 반복 카운트(동일 파라미터 반복) — 이 params-hash 는 **dedup 단위이기도 하다**(아래) | 5분 20회↑ |
| `RssSurge` | agent 당 최근 5개 RSS 샘플의 **엄격한 단조 증가**(스파이크 1회는 발화 안 함, 추세만) | 5 샘플 |

RSS 값 소스는 caller 타입별로 다르다: **Plugin** 은 host(`tasty-host-plugin::PluginManager`)가 `PluginProcess.child` 의 PID 를 sysinfo 로 30초 간격 직접 sampling(agent 자가 보고는 신뢰 불가 — 정확한 자기 RSS 를 보고할 유인이 없음). **Agent** 는 PID 기반이 구조적으로 불가능(원격/별도 프로세스)해 `telemetry.record` 자가 보고(`metric == "rss_bytes"`)로 받는다. 발화는 셋 다 동일하게 `tasty.telemetry.anomaly.*` 영속 + 알림이고, 1분 쿨다운 dedup 도 공유한다. 다만 **dedup 키가 셋 다 `subject` 인 것은 아니다**:

| 휴리스틱 | dedup 키 | subject |
|---|---|---|
| `CallBurst` | `(agent, kind, method)` | `method` — 키와 같다 |
| `SlowLoop` | `(agent, kind, "{method}#{params_hash:016x}")` | `method` — **키와 다르다** |
| `RssSurge` | `(agent, kind, "rss_bytes")` | `rss_bytes` — 키와 같다 |

`SlowLoop` 만 dedup 키에 `params_hash` 를 덧붙여, 같은 method 라도 파라미터 조합이 다르면 **독립된 loop 로 취급해 각자 쿨다운을 갖는다**(`params_hash` 는 detail 에도 실린다). 그래서 같은 method 의 발화가 수백 ms 간격으로 연달아 보이는 것은 정상이며 — 조합 수만큼 각자 분당 1건씩 나온다 — 쿨다운 버그가 아니다. surface 마다 파라미터가 다른 폴링에서는 이 배증이 커서, 18시간에 21,102건(≈ 조합 20종 × 1,080분)이 쌓인 실측이 있다. 보존 상한은 [ADR-0085](../../adr/0085-ipc-log-retention-bounded.md) 의 공통 정책(50시간 · 5,000건)을 따른다.

### 세션 요약

결정론적 순수 집계(LLM 없음): tokens(ipc_calls 제외 metric sum) / ipc_calls(method 별 top-N) / approvals 분포 / anomalies. `workspace_id` 미지정 시 전 워크스페이스 합산(포커스 독립).

### 요청 압력 게이지 (프로세스 축)

위 `ipc_calls` 와 **다른 축**이다. caller 로 나누지 않고, 저장소를 거치지 않으며, 프로세스
수명 동안 자라지 않는 고정 크기 원자값이다(근거·대안은
[ADR-0305](../../adr/0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md),
노출 표면은 [ADR-0333](../../adr/0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md)).
재는 것은 큐 깊이 · 큐 대기 · handler 실행 시간 · plugin 왕복 · DB 지연의 count·sum·max 이고,
평균은 파생이라 메서드로 낸다. 시간 축 셋(`db` 제외)에는 **고정 경계 분포**가 하나씩 더
붙는다([ADR-0340](../../adr/0340-the-pressure-answer-counts-seats-and-carries-a-fixed-bound-distribution.md)).
여기에 **시간이 아닌 축**이 하나 더 있다 — 동시 IPC 연결 자리다. 그리고 누계가 아닌 덩어리가
하나 있다 — 두 SQLite DB 가 열 때 되읽은 pragma 다. 또 하나는 **요청이 아니라 밀어내기**를
잰다 — 스트림 연결로 민 프레임의 손실과 적체다. 다음 셋은 명령 큐의 **양 끝**(들어온 쪽의 입장
장부 · 꺼낸 쪽의 회차와 실행 중인 요청)과 멱등 키를 실은 요청이 받은 **판정**이다
([ADR-0435](../../adr/0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)).
그다음 하나는 집계가 아니라 **느린 요청 한 건씩**이다 — 호스트가 발급한 요청 번호로 큐 대기 · 호스트
처리 · plugin 대기를 한 줄에 잇는다([ADR-0436](../../adr/0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md)).
그리고 진입 게이트 **자신의 판정** — 게이트가 요청을 몇 건 보았고 그중 몇 건을 권한 · cap · 스로틀
중 어느 게이트가 돌려보냈는가 — 가 덩어리 하나다([ADR-0548](../../adr/0548-gate-refusals-join-the-pressure-answer-as-one-block-split-by-gate.md)).

`system.pressure`(local-only) 가 그 누계를 읽는다. 응답은 **모수마다 한 덩어리**다.

| 덩어리 | 재는 자리 | 모수 |
|---|---|---|
| `queue_before_gate` | 명령이 큐에서 나온 직후 (`App::process_ipc` · `pump_ipc`) | 큐에 앉았던 **전부** — 뒤에 거부될 요청도 센다 |
| `handler_after_gate` | 게이트 통과 뒤 (`handle_checked_request`) | 실제로 **실행된 것만** |
| `plugin_round_trip` | plugin 응답 매칭부 (`PluginManager::handle_plugin_response`) | 응답이 **매칭된 요청만** — 취소·만료된 것은 끝점이 없다 |
| `db` | `MemoryStore` 의 `tx.commit()` 뒤 · `checkpoint_truncate` | **성공한** commit 과 시도된 checkpoint — 거부된 쓰기는 롤백이라 안 센다 |
| `connections` | accept 루프의 자리 획득·반납 (`TcpIpcServer`) | 이 포트에 붙은 **TCP 연결 전부** — 요청을 하나도 안 보내는 attach·mesh 스트림도 센다 |
| `db_pragmas` | DB 를 열 때 한 번 (`tasty_memory::pragma::apply_connection_pragmas`) | 누계가 아니다 — DB(`memory_db` · `state_db`)마다 요청값과 되읽은 실제값 |
| `stream_push` | 스트림 허브의 push 와 write 스레드의 수신 (`tasty_ipc::stream_hub::StreamHub`) | 서버가 attach·mesh 스트림 연결로 **민 프레임** — 요청 하나 없이도 자란다 |
| `queue_admission` | 명령 큐 입장 판정 (`tasty_ipc::admission::CommandAdmission`) | 큐에 **들어오려던** 요청 전부 — 상한에 걸려 돌려보낸 것도 센다 |
| `queue_dispatch` | 큐에서 꺼내는 회차와 실행 직전 (`tasty_ipc::dispatch::DispatchStats`) | 큐에서 **꺼낸** 명령과 그 회차 — 실행 중인 요청은 호출자가 아직 기다리는 것만 |
| `keyed_requests` | 멱등 보존소의 판정 (`idempotency::Store::decide`) | **멱등 키를 실은** 요청만 — 한 요청은 자기를 맡은 층의 판정으로 한 번 |
| `slow_requests` | 명령을 꺼낸 직후·다 다룬 직후 (`app::ipc_round::CommandObservation`, gui · headless 같은 자리) + plugin 대기 표의 응답·만료·취소 (`PluginManager::record_origin_hop`) | 호스트 IPC 큐를 지난 요청 중 **합이 문턱(100 ms) 이상인 것만** — `system.pressure` 자신은 안 넣는다 |
| `gate_refusals` | 공통 진입 게이트 (`handler/checked.rs` 의 `check_request`) | 게이트에 **든** 요청 전부(`judged`) — Local 도, 큐를 안 지나는 plugin host-call 도 센다. 거절은 게이트마다 한 칸 |

`queue_before_gate` 안에서 명령을 세는 수가 둘이다. `waits` 는 대기를 기록한 명령 수로, 명령을 **꺼내는 순간** 대기 합·최댓값·분포와 함께 오른다 — `wait_us_mean` 의 분모이고 `wait_us_hist.counts` 의 합과 같다. `commands` 는 회차가 **끝날 때** 그 회차가 꺼낸 수만큼 오르고 `depth_mean`(= `commands / drains`)의 분자다. 조회는 늘 자기 회차 안에서 답하므로 한 답 안에서 `commands` 는 아직 도는 회차(조회 자신 포함)만큼 `waits` 보다 작다 — 평균을 `commands` 로 나누면 최댓값을 넘는 값이 나왔다([ADR-0466](../../adr/0466-the-queue-wait-mean-divides-by-the-waits-it-summed.md)).

셋째는 앞의 둘과 축이 다르다. 앞의 둘은 호스트가 **자기 큐와 자기 handler** 에서 보낸
시간이고, 셋째는 호스트가 **남의 프로세스를 기다린** 시간이다(`PluginWaitStats`). 큐도
handler 도 빠른데 응답이 느리면 그 시간은 plugin 안에 있었던 것이다. 게이지 하나를
프로세스가 소유하고 plugin manager 에 **핸들만** 넘긴다 — 창마다 매니저를 다시 만들어도
같은 핸들을 받으므로 축이 프로세스로 유지된다. 주입이 없는 구성(단위 시험)에서는
아무것도 세지 않아 `matched` 가 0 으로 남는다.

넷째는 **디스크가 받아준** 시간이다(`DbLatencyStats`). commit 은 handler 실행 시간 *안에*
들어 있으므로, 그 둘을 나란히 두면 "handler 가 느리다" 와 "handler 안의 쓰기가 느리다" 가
갈린다. 게이지는 `MemoryStore` 가 열릴 때 그 안에서 태어나고 부팅 wiring 이 핸들을 `Core`
에 복제해 둔다 — 진단이 값을 읽을 때 **메모리 뮤텍스를 안 잡는다**(적체를 재려고 적체하는
자물쇠를 잡으면 진단이 같이 막힌다). checkpoint 는 부팅 때의 WAL 되감기이고, 다른 커넥션이
읽는 중이면 못 끝내고 돌아오므로 그 횟수를 `checkpoints_busy` 로 따로 센다 — 시간만 봐서는
느린 것과 경합한 것이 안 갈린다. 스토어를 못 여는 조립(단위 시험)에서는 아무도 안 올려
`commits` 가 0 으로 남는다.

다섯째는 **시간이 아니라 자리**다(`ConnectionStats`). 앞의 넷이 전부 "얼마나 걸렸나" 인
반면 이것은 "자리가 남았나" 이고, 그 구분이 없으면 자원 포화가 밖에서 안 보인다 — 요청이
하나도 안 느려도 동시 연결 상한(`MAX_CONCURRENT_CONNECTIONS`, 256)이 차면 새 client 는
**붙지도 못한다.** 그 거절은 요청이 되기 전에 일어나므로 앞의 네 덩어리 어디에도 안 남고
`ipc_calls` 에도 안 남는다(JSON-RPC 요청이 아니라 TCP 연결이다). 자리를 세는 값은 다섯이다(시간 칸 넷은 아래 accept 대기 문단) —
`live`(지금 붙어 있는 수, **이 덩어리의 누계·순간값 중 유일하게 내려가는 값** — 평균은 파생값이라 내려갈 수 있다) · `live_max`(관측된 최댓값) ·
`limit`(서버가 집행하는 상한) · `accepted`(자리를 받아 간 누계) ·
`refused_saturated`(상한에 걸려 거절된 누계). `limit` 이 같이 나가야 `live` 가 상한에 얼마나
가까운지가 한 응답 안에서 읽힌다. 거절 로그는 포화로 들어가는 순간만 `warn` 이고 그 뒤로는
`debug` 로 접히지만, **접히는 것은 로그뿐이고 `refused_saturated` 는 전부 센다.** 게이지는
`Core` 가 낳아 부팅이 IPC 서버에 핸들을 건넨다 — `db` 와 방향이 반대인 이유는 서버가 `Core`
보다 **뒤에** 뜨기 때문이다. 서버가 안 뜬 조립(단위 시험)에서는 `accepted` 가 0 으로 남아
"연결을 받은 적이 없다" 로 읽힌다.

이 덩어리에 시간이 하나 있다 — **accept 대기의 상한**(`accept_waits` · `accept_wait_bound_us_sum` ·
`accept_wait_bound_us_max` · `accept_wait_bound_us_mean`). accept 루프는 논블로킹이라 큐가 비면 100 ms
자고, 그 사이 도착한 연결은 요청 줄을 읽히기 전에 기다린다 — 큐 대기 계측(`queue_before_gate`)은 줄을
읽은 뒤에 시작하므로 그 시간이 어디에도 안 잡혔다. OS 가 연결을 큐에 넣은 시각은 사용자 공간에서 안
보이므로, 재는 것은 **루프가 큐를 마지막으로 비어 있다고 본 뒤 지난 시간**이다. 그 뒤에 꺼낸 연결은 그
순간 뒤에 도착했으므로 이 값은 실제 대기를 넘을 수 없는 **상한**이고, 잠든 동안 고르게 도착하면 실제
대기는 평균적으로 그 절반이다. 모수는 루프가 꺼낸 TCP 연결 전부라 `accept_waits = accepted +
refused_saturated` 다([ADR-0467](../../adr/0467-the-accept-wait-is-reported-as-a-bound-in-the-connection-block.md)).

여섯째는 **누계가 아니라 설정**이다. `memory_db` · `state_db` 두 칸이 각각 `in_memory` ·
`degraded` 와 pragma 넷(`journal_mode` · `synchronous` · `foreign_keys` · `journal_size_limit`)의
`requested` · `effective` · `took` · `error` 를 싣는다. 요청값을 함께 싣는 것은 소스의 `WAL` 이
runtime 보장이 아니어서다 — SQLite 는 요청을 조용히 거절할 수 있다. `degraded` 는 하나라도 그
DB 모드의 허용 결과로 안 섰다는 뜻이고 **오류가 아니라 열린 채로 쓰이는 상태**다. `db` 덩어리와
같은 DB 를 말하므로 한 응답에서 "commit 이 느리다" 와 "WAL 이 안 섰다" 가 함께 읽힌다. 열리지
않은 DB 는 `null` 이다 — 헤드리스의 `state_db` 는 늘 `null` 이다(허용 결과표·근거는
[storage](../../design/systems/storage.md) 와
[ADR-0376](../../adr/0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md)).
`memory_db` 에만 `init_failure` 가 하나 더 있다 — 부팅이 `memory.db` 를 못 열어 in-memory 대체로
떴으면 `{cause, error}`(원인 이름과 오류 문구), 아니면 `null` 이다. 대체면 pragma 가 다 섰어도
`degraded` 가 `true` 다 — `in_memory: true` 만으로는 "원래 in-memory" 와 "파일을 못 열어
in-memory" 가 안 갈린다([ADR-0485](../../adr/0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md)).

일곱째는 **요청이 아니라 밀어내기**다. 앞의 여섯은 전부 client 가 물어본 것(요청 · 연결 · 쓰기)을
재고, 이것은 서버가 스트림 연결로 **민** 프레임을 잰다. 값은 넷이다 —
`frames_dropped`(연결이 살아 있는데 그 연결의 큐가 차서 버린 프레임 누계 — **조용한 손실**) ·
`clients_lagged_out`(연속 drop 한도로 끊은 연결 누계 — 소비자가 이미 아는 손실) · `backlog`(지금
살아 있는 연결들의 큐에 쌓여 아직 write 스레드가 안 가져간 프레임 수의 합 — **이 덩어리에서 유일하게
내려가는 값**) · `sink_capacity`(연결 하나의 큐 상한 — `connections.limit` 과 같은 이유로 값과 함께
나간다). 연결별 **연속** drop 수는 없다 — 성공 한 번에 0 이 되는 강제분리의 좌변이라 읽는 시점에 따라
같은 사건이 0 으로 보인다. 세 값의 정의와 손실을 받은 client 가 하는 일은
[ADR-0400](../../adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)
과 [attach-behavior](../../dev-guide/attach-behavior.md) 에 있다. 허브가 엔진에 주입되지 않은
조립(단위 시험)에서는 덩어리가 `null` 이다.

여덟째 `queue_admission` 과 아홉째 `queue_dispatch` 는 **명령 큐의 양 끝**이다. `queue_before_gate` 가
큐에서 나온 명령이 얼마나 기다렸는가를 잰다면, 이 둘은 무엇이 들어오려 했고 무엇이 나갔는가를 잰다.

`queue_admission` 은 입장 장부다(`tasty_ipc::admission::CommandAdmission`). 큐에 든 요청 바이트 합과
호스트 주입 명령 수에 상한이 걸려 있고, 넘으면 요청은 큐에 들어가지 않는다(소켓 요청은 `-32065`,
호스트 주입은 `InjectError::Refused`). 값은 여덟이다 — 지금 값 셋(`queued_bytes` · `queued_commands` ·
`queued_injected`, **내려간다**) · `peak_bytes`(관측된 최고 바이트) · 거절 누계 둘(`refused_bytes` 바이트
상한 · `refused_depth` 주입 깊이 상한) · 그 장부가 집행하는 상한 둘(`limit_bytes` ·
`limit_injected_depth` — `connections.limit` 과 같은 이유로 값과 함께 나간다). 돌려보낸 요청은 큐에 한
번도 안 들어가므로 **다른 어느 덩어리에도 안 남는다** — 이 덩어리가 없으면 그 거절은 요청자가 받은
응답과 `debug` 로그로만 드러난다. 장부는 IPC 서버가 만들고 호스트 주입기가 같은 것을 든다. 서버가
안 뜬 조립(단위 시험)에서는 덩어리가 `null` 이다. 1 바이트의 정의와 상한 값은
[ADR-0391](../../adr/0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md).

`queue_dispatch` 는 큐에서 **꺼낸** 쪽의 누계다(`tasty_ipc::dispatch::DispatchStats`). 값은 일곱이다 —
`rounds`(명령을 하나 이상 꺼낸 회차) · `rounds_stopped_by_count` · `rounds_stopped_by_time`(그중 명령 수 ·
시간 예산에 닿아 멈춘 회차) · `expired_before_run`(호출자의 응답 대기 상한이 큐에서 지나 실행하지 않은
명령 — 소켓 요청은 `-32067` 로 답한 것이고, 호스트 주입 명령([ADR-0451](../../adr/0451-a-host-injection-carries-its-wait-as-a-deadline.md))도
센다. 주입 명령은 `-32067` 로 답해지지 않고 주입한 호출자 스레드에서 `InjectError::Expired` 로 끝난다) · `started`(실행을 시작한 명령) · `in_flight`(지금 실행 중이고 **응답을
기다리는 쪽이 아직 기다리는** 요청 — **이 덩어리에서 유일하게 내려가는 값**) · `in_flight_max`. 메인
스레드 handler 는 한 번에 하나라 `in_flight` 가 1 을 넘는 것은 응답을 워커로 넘긴 요청이 기다리는
동안이다. 정의는 [ADR-0412](../../adr/0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md).
두 덩어리를 합치지 않는 이유는 모수가 달라서다 — `queued_commands` 와 `started` 의 차는 "아직 큐에
있다" 가 아니다(큐 안에서 만료된 것과 기다리던 쪽이 물러난 것이 섞인다).

열째 `keyed_requests` 는 **멱등 키를 실은 요청만** 센다. 보존소가 내린 판정마다 한 칸이다 —
`executed`(처음 보는 키, 실행했다) · `replayed`(같은 요청, 보관된 답을 냈다) · `conflicted`(같은 키에
다른 요청, 아무것도 실행하지 않았다) · `discarded`(실행은 됐고 답은 버려졌다) · `in_flight`(같은 요청이
진행 중이어서 거기에 합류했다). 전부 누계다. `executed` 는 재시도가 아니지만 **모수**다 — 재생 수만으로는
그것이 키 실은 실행 열 건 중 하나인지 만 건 중 하나인지 모른다. 한 요청은 자기를 맡은 층의 판정으로
**한 번만** 세진다. 이 `in_flight` 는 `queue_dispatch.in_flight` 와 이름만 같다. 정의는
[ADR-0422](../../adr/0422-the-retry-counts-are-kept-by-the-store-that-decides.md).

열한째 `slow_requests` 는 **집계가 아니라 느린 요청 한 건씩**이다(`tasty_telemetry::SlowRequestLog`).
분포는 "100 ms 를 넘은 것이 몇 건" 까지 말하고, 그 한 건의 시간이 큐 · 호스트 · plugin 중 어디에
있었는지와 그 plugin 대기가 **어느 요청의 것**이었는지는 못 말한다 — 이 덩어리가 그것을 말한다.

- **요청 번호** — `request_seq` 는 호스트가 `IpcCommand` 를 만들 때 매기는 프로세스 전역 단조 번호다
  (`tasty_ipc::server::RequestSeq`). JSON-RPC `id` 가 아니다(호출자 값이라 거의 늘 `1` 이다). Event Bus
  envelope 의 `trace_id` 도 아니다(사건 사슬의 값이고 plugin 이 보낸 값을 그대로 싣는다). 재시작하면 1
  부터 다시 센다.
- **한 줄** — `request_seq` · `host`(`method` canonical 이름 — 모르는 이름은 받은 그대로이고 128 바이트에서 자른다 · `caller` 봉투가 말한 `local`/`agent` ·
  `queue_wait_us` · `host_us`) · `plugin_hops`(hop 마다 `plugin_id` · `host_request_id` · `wait_us` ·
  `outcome` = `ok`/`error`/`expired`/`cancelled`, 최대 셋 — pre-hook · target · post-hook) · `total_us`.
  `host_us` 는 꺼낸 뒤 호스트가 명령을 다 다루기까지(게이트 포함)라 `handler_after_gate` 와 모수가
  다르다. plugin 으로 넘긴 요청이면 넘기는 데까지이고, plugin 을 기다린 시간은 hop 쪽에 있다.
- **호스트 몫의 결과** — `host` 에는 `outcome`(`ok`/`error` — hop 의 `outcome` 과 같은 낱말)과
  `error_code`(오류면 호출자가 받은 JSON-RPC 코드, 아니면 `null`)가 함께 실린다. 값은 **호출자에게 실제로
  나간 답**이다 — 소켓 연결 스레드와 호스트 주입기가 답을 받거나(dispatch 가 보낸 답 · plugin 의 뒤늦은
  답) 상한에서 스스로 만든 순간(`-32067` 실행 전 만료 · `-32061` 결과 불명) 명령의 결과 칸에 한 번 적고,
  링의 줄이 같은 칸을 들고 있어 **읽는 순간의 값**이 나간다. 그래서 큐에서 만료된 요청(`-32067`) ·
  게이트가 거절한 요청(`-32001`) · 정상 답이 링만으로 갈린다. 아직 답이 안 나갔으면(plugin 으로 넘긴
  요청이 기다리는 중) 둘 다 `null` 이다. 기한 없는 주입이 상한에서 물러난 경우도 `null` 이다 — 그 명령은
  큐에 남아 뒤에 실행될 수 있어 호출자에게 나간 답이 없다([ADR-0468](../../adr/0468-the-slow-request-host-part-carries-the-answer-the-caller-got.md)).
- **plugin 로그로 되짚기** — `host_request_id` 는 plugin 이 받은 JSON-RPC id 와 같은 값이다. 호스트의
  plugin 오류 응답 경고와 namespace 만료 경고도 같은 줄에 `id=<host_request_id>` 와
  `request_seq=<번호>` 를 싣는다(번호를 모르는 plugin 요청이면 `none` — IPC 요청에서 오지 않은 것과 아래 "모수 밖" 의 파일 핸들러 경로). 번호는 plugin 에게
  안 간다 — plugin wire 는 그대로다.
- **넣는 기준** — 큐 대기 · 호스트 처리 · plugin 대기의 합이 `threshold_us`(100 ms) 이상인 요청만.
  plugin 으로 넘긴 요청은 넘기는 순간 열린 자리를 잡고, 합이 문턱을 넘는 순간 링에 든다 — 그 뒤 hop
  도 같은 줄에 붙는다. 만료된 forward 는 `plugin_round_trip`(매칭된 것만 센다)에는 끝내 안 남지만 여기에는
  `expired` 로 남는다.
- **상한** — 링은 메모리 안의 `capacity`(32) 줄이고 넘치면 오래된 줄부터 밀려난다. `admitted` 는 켜진
  뒤 링에 든 누계라 줄 수와의 차가 밀려난 수다. 끝나지 않은 forward 를 드는 열린 자리는 256 이 상한이다.
  호출당 저장소 기록은 0 이다.
- **싣지 않는 것** — params 원문 · session token · 멱등 키 · JSON-RPC `id` 값. 요청 번호는 레이블로
  쓰지 않는다.
- **모수 밖** — plugin 이 부른 host-call 은 호스트 IPC 큐를 안 지나 번호가 없고 여기 안 든다. plugin 이
  부른 namespace forward 도 번호가 없다. IPC `file_handler.dispatch` 는 요청 자신은 링의 모수에 들지만,
  그 명령이 파일 핸들러 큐를 거쳐 plugin 으로 넘기는 forward 는 번호를 잃는다 — 그 plugin 대기는 원 요청 줄에
  안 붙는다(그 forward 는 호출자에게 답하지 않는다). `host` 가 `null` 인 줄은 열린 자리가 밀려난 뒤 hop 만 온 것이다.

값과 상수의 근거는 [ADR-0436](../../adr/0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md).

열둘째 `gate_refusals` 는 게이트가 **돌려보낸** 요청이다(`tasty_telemetry::GateStats`). 칸은 넷이다 —
`judged`(게이트에 든 요청, 모수) · `permission_denied`(권한 게이트, `-32001`) · `cap_blocked`(텔레메트리 cap
게이트, `-32007`) · `throttled`(rate limit 게이트, `-32010`). 게이트는 권한 → cap → rate limit 차례로 보고
앞에서 돌려보낸 요청은 뒤를 안 지나므로 한 요청은 많아야 한 칸에 세진다 — 그래서 `judged` 에서 세 거절을
뺀 값이 세 게이트를 모두 지난 수와 같다. 셋을 한 칸으로 합치지 않는 것은 처방이 달라서다(권한을 청한다 ·
cap 을 푼다 · 기다린다). 단 `permission_denied` 는 **권한 게이트가 돌려보낸 `-32001` 만** 센다 — 그
거절은 모두 `-32001` 이지만 역은 아니다. 그리고 권한이 모자란 거절만 담지 않는다 — 권한 게이트는 권한 셋을 가진 호출자(plugin · agent)가 부른 없는 메서드 이름과 그 호출자에게
열리지 않은 메서드도 같은 코드로 돌려보내고, 그 둘은 권한을 청해도 안 풀리고 부르는 메서드를 바꿔야 한다.
Local 호출자는 이 게이트에서 돌려보내지지 않으므로 없는 메서드를 불러도 이 칸에 안 든다. 응답
메시지(`permission_denied: unknown ipc method …` 등)가 셋을 가른다. `-32001` 인데 이 칸에 안 드는 것이 두
부류다 — ⒜ **봉투 토큰 거절**: `session_token` 의 형식 오류 · unknown/expired/revoked 는
`resolve_caller_from_envelope`(`crates/tasty-ipc/src/caller.rs`)가 게이트 **앞**에서 답하므로 `judged` 에도 안
든다(낡은 `TASTY_SESSION_TOKEN` 을 든 CLI 가 이 경로다). ⒝ **게이트를 지난 뒤 핸들러가 내는 `-32001`**: 입력
시뮬레이션 미허용(debug) · macOS 손쉬운 사용 미승인 `surface.raw_key`(debug, macOS) · 세션 발급의 격상 거절 · plugin manager
쪽 거절(자기 namespace 호출 · `ipc.invoke` 부족 · extension filter 차단)은 게이트를 지났으므로 통과 쪽에 든다.
앞 둘은 Local 호출자도 받는다.

- **리셋** — 전부 이 프로세스가 뜬 뒤의 누계다. 창 단위로 비워지지 않고 재시작하면 0 이다. 영속되는
  `agent.rate_limit_status` 의 `throttled_count` 와 모수가 다르다 — 그쪽은 버킷 하나의 수명 동안 재시작을
  넘어 쌓이고, 게이트 밖의 직접 소비 거절도 센다.
- **모수 밖** — 게이트 뒤의 봉투 검사(멱등 키 길이)에서 돌려보낸 요청은 거절 칸에 안 든다(정책이 아니라
  틀린 인자다). 엔진이 없는 GUI 부팅·종료 구간에서 Local 이 아닌 호출자를 돌려보내는 판정
  (`check_without_engine`)은 `judged` 에도 안 든다.
- **호출자별이 아니다** — 누가 거절당했는지는 audit log(`plugin.audit_query`, Deny 만 남는다)의 몫이다.

#### 분포 — 평균·최대가 못 답하는 것

`queue_before_gate.wait_us_hist` · `handler_after_gate.us_hist` ·
`plugin_round_trip.us_hist` 가 같은 관측의 **분포**를 든다. 평균과 최대만으로는 "전부
조금씩 느린가, 대부분 빠른데 꼬리가 몇 건인가" 가 안 갈린다 — 넷 중 한 건이 1 s 이고 셋이
0 인 분포와 넷이 전부 250 ms 인 분포는 count·sum·평균이 **전부 같다.** 처방은 반대다
(뒤는 용량, 앞은 그 한 건의 원인).

경계는 10 µs 부터 1 s 까지 반-십진(√10 ≈ 3.16 배) **열한 칸 + 넘침 한 칸**이고
`LATENCY_BUCKET_BOUNDS_US` 가 정본이다. 파생되지 않은 고른 값이며, 근거는 ADR-0333 의
실측(큐 대기 max 25.4 ms · handler max 0.48 ms)과 plugin 왕복이 초 단위까지 간다는 것이다.
응답은 덩어리마다 `bounds_us` 와 `counts` 를 **함께** 싣는다 — 값과 경계가 떨어지면
소비자가 경계를 복제하고 그 복제본이 갈린다. `counts` 는 `bounds_us` 보다 한 칸 길고
**누적이 아니다**: 칸끼리 겹치지 않아 합이 관측 수이고(Prometheus 의 `le` 누적 버킷과
다르다), 마지막 칸은 상한이 없어 "그 상한을 넘었다" 까지만 말한다 — 얼마나 넘었는지는
같은 덩어리의 `us_max` 가 답한다. **분위수는 호스트가 계산하지 않는다**: 버킷 해상도
안에서만 답할 수 있는 값이라 한 수로 내놓으면 없는 정밀도를 말하게 된다.

`db` 에는 분포가 없다 — 그 게이지는 `tasty-memory` 에 살고 histogram 타입은
`tasty-telemetry` 에 있어 의존 방향이 반대다. `connections` 에도 없다 — 시간이 아니라
자리라 분포를 잴 축이 아니다. `stream_push` 와 그 뒤 세 덩어리, 그리고 `gate_refusals` 도 시간이 아니라 수라 없다. `slow_requests` 는 시간을 싣지만 분포가 아니라 문턱을 넘은 요청 한 건씩이다.

#### 덩어리를 가리지 않고 걸리는 것

아래 셋은 위 `분포` 소절에만 해당하는 서술이 **아니다** — 응답 전체의 성질이다.

두 수의 차는 "거부된 수" 가 아니다 — 게이트를 통과하고도 `handle_checked_request` 를 안
지나는 갈래가 있다(gui 의 app 층 메서드는 그 자리에서 답하고 돌아간다). 그래서 응답은
두 모수를 나란히 두고 뺄셈을 하지 않는다.

관측이 없는 평균은 `null` 이다 — 0 이면 "기다림이 없었다" 와 "잰 적이 없다" 가 같은 값이 된다.

창이 없다는 한계는 그대로다: 프로세스 수명 누계라 "지금 밀리는 중" 과 "부팅 직후 한 번
밀렸다" 가 같은 max 로 보인다. 분포는 그 한계를 **반만** 푼다 — 부팅 직후의 한 건은 꼬리
칸의 `1` 로 남아 그것이 소수임은 보이지만, 그 한 건이 언제였는지는 여전히 안 보인다.

## 인터페이스

- **AI Agent / CLI**: `telemetry.record(_batch)`(`telemetry` 권한) · `summary/timeseries/top` · `cap.{set,list,remove,status,reset}` · `anomaly.list` · `session_summary`. [reference/api](../../reference/api.md#텔레메트리-telemetry).
- **로컬 운영자 / CLI**: `system.pressure` (local-only) · `tasty list pressure` — 위 "요청 압력 게이지" 절.
- **Claude Code 통합**: `tasty claude install` hook 이 `session-start`→`stop` 의 `wall_time_ms`, notification 의 `input_tokens`(`tokens: N` 패턴)를 `tasty.com.tasty.claude` agent 로 자동 적재. [claude plugin](../../plugins/claude/index.md).

## 관련

- [agent-collaboration](../agent-collaboration/index.md) — rate-limit 과의 구분 · [human-handoff](../human-handoff/index.md) — require_approval
