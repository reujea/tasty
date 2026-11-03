# 텔레메트리 (Telemetry)

- **Status**: Implemented
- **주체**: AI Agent · 로컬 사용자. 에이전트별 활동을 측정하고 한도(cap)를 설정·해제한다.
- **설계**: [플러그인 권한 모델](../../dev-guide/plugin-permissions.md) — 검사를 선택적으로 끄지 않는다, 권한 토큰은 선언이다
- **코드**: `telemetry.*` 핸들러, `tasty-telemetry`, 영속 `tasty-memory`
- **화면**: 없음 (cap 도달 시 알림)
- **메서드 목록**: [reference/api](../../reference/api.md#텔레메트리-telemetry)

## 목적

AI 에이전트 활동을 도메인 메트릭으로 **기록·집계·차단**하는 관측 계층. 비용(토큰)·호출량을 추적하고 임계 초과 시 설정한 동작을 실행한다.

## 내부 동작

### 모델

Metric(`input_tokens`/`ipc_calls`/…) × Agent(plugin 은 id 의 비허용 문자를 `_` 로 바꾼 값 — 예 `com_tasty_claude`, Local 은 `_host`, 미명시 시 CallerContext 자동) × Workspace(없으면 `global`) × Op(`Set`/`Inc`/`Dec`, 집계에서 `Inc`·`Dec` 는 sum 누적·`Set` 은 덮어쓰기) × Window(`1m/1h/1d`) × Tags. 이벤트는 `tasty.telemetry.event.{ts}.{seq}` 로 영속, 조회는 prefix scan + 순수 집계(재시작 후 누적 보존).

**집계본은 영속되지 않는다.** bucket 은 조회할 때마다 raw event 로부터 새로 만들어지고 버려진다 — 주기 rollup task 도, `tasty.telemetry.bucket.*` 키도 없다. 따라서 **raw event 보존량이 곧 조회 가능 범위**이며, 그 상한은 관측 로그 3종 공통 정책(`store::log_retention`)이 정하는 **최근 20,000 이벤트**다. 조용한 인스턴스에서는 수일치, 폴링이 도는 인스턴스에서는 수십 분치가 되므로 조회 범위가 데이터 양에 종속된다. 롤업을 신설하지 않기로 한 근거와 재검토 조건은 [저장소 시스템 (Storage System)](../../design/systems/storage.md).

`ipc_calls` 는 dispatcher 가 plugin IPC 호출마다 자동 1회 기록(`tags.method`). `_host` 또는 `telemetry.*` 는 자기측정/재귀 방지로 skip.

### AgentId — agent 식별

`AgentId`(`crates/tasty-telemetry/src/agent_id.rs`)는 메트릭/cap/anomaly/rate-limit 이 *agent 차원* 으로 집계되기 위한 식별자다. session token 을 동반한 호출은 호스트가 발급 때 기록한 `agent_id` 로 검증되고, 토큰 없는 Local 호출의 env 값은 위조 가능하다(아래 한계).

#### 도출 규칙

| Caller | agent_id |
|--------|----------|
| Agent (session token 동반) | 호스트가 `session.issue` 때 기록한 `agent_id`(토큰 검증 통과) |
| Plugin process | 매니페스트 `plugin_id`(매니페스트 등록으로 인증됨) |
| Local CLI/사용자 | 요청 봉투의 `caller_agent_id`(CLI 가 자기 환경의 `TASTY_AGENT_ID` 를 싣는다, 자기 신고), 없거나 형식이 틀리면 sentinel `_host` |

```rust
caller.agent_id()                  // CallerContext (crates/tasty-ipc/src/caller.rs)
tasty_telemetry::AgentId::from_env()  // 같은 프로세스의 env TASTY_AGENT_ID, 없으면 _host
```

- `AgentId::HOST` = `"_host"` (빈 문자열도 HOST 로 대체), env key = `AgentId::ENV_KEY` (`"TASTY_AGENT_ID"`).

#### child agent env 주입

`claude.spawn`/`launch` 로 띄운 child 는 PTY shell 위에서 inline env prefix 로 실행된다:

```
$ TASTY_SURFACE_ID=<surface_id> TASTY_AGENT_ID=claude_s<surface_id> TASTY_SESSION_TOKEN=<hex> claude
```

- `claude_s<surface_id>` 는 surface 와 1:1 — 호스트가 surface_id 로 역추적 가능.
- inline prefix 라 export 와 달리 셸 환경이나 profile 에 남지 않고 그 명령의 자식 프로세스에만 전달된다. 다만 셸에 입력한 명령 줄이므로 토큰이 화면·scrollback 에 보이고, 앞 공백이 없어 `HISTCONTROL=ignorespace`·zsh `HIST_IGNORE_SPACE` 를 켠 셸에서도 history 에 남을 수 있다. 터미널에 표시된 데이터의 취급은 [사용자 통제와 데이터 원칙](../../principles/control-and-data.md#터미널-기록과-개인정보)을 따른다.

#### 호환 표

| 위치 | sentinel / 형식 |
|------|-----------------|
| `AgentId::HOST` | `"_host"` |
| `tasty_memory::HOST_OWNER` | `"_host"` (memory.db `owner` 컬럼) |
| `CallerContext::owner()` | Local → `_host`, Plugin → `plugin_id`, Agent → `agent_id` |
| `CallerContext::agent_id()` | Local → 봉투의 `caller_agent_id` 또는 `_host`, Plugin → `plugin_id`, Agent → `agent_id` |

Local 분기에서 `owner()` 는 항상 `_host` 이고 `agent_id()` 는 호출자가 봉투로 밝힌 `caller_agent_id` 다. 이 값은 검증할 수 없는 자기 신고라 텔레메트리 기록(`telemetry record` 의 기본 agent, `ipc_calls` 집계)·감사 `caller_id`·헤드리스 PTY `owner_agent_id` 표시에만 쓰고, 권한·memory owner·rate limit·cap 차단 판단에는 쓰지 않는다([플러그인 권한 모델](../../dev-guide/plugin-permissions.md)). 형식(`[a-zA-Z0-9_-]`, 64자 이하)이 틀리면 버리고 `_host` 로 기록하며 요청은 거절하지 않는다. 세션 토큰이 있으면 세션의 ID 가 우선한다. 호스트 프로세스 자신의 `TASTY_AGENT_ID` 는 이 Tasty 를 띄운 바깥 인스턴스에서 상속한 값이라 읽지 않는다. 구 서버는 봉투 필드를 무시하고 `_host` 로 기록하며, 지원 여부는 capability `ipc.caller-agent-id` 로 확인한다.

#### 보안 한계

토큰 없는 Local 호출의 agent ID 는 호출자가 적어 보낸 값이다 — 다른 agent·plugin 의 ID 를 적으면 그 이름의 텔레메트리가 늘고 그 이름에 걸린 cap 이 발동할 수 있다. `telemetry record --agent` 로도 같은 일을 할 수 있다. 이 모델은 **악의보다 버그** 영역으로 처리한다 — *정직한 agent 의 폭주를 막는 안전망*. 적대적 agent 방어는 OS 권한·plugin 매니페스트가 우선.

검증 가능한 신원: claude plugin 은 child 기동 때 `issue_session_token`(`crates/tasty-plugin-claude/src/handlers.rs`)으로 `session.issue` 토큰을 받아 `TASTY_SESSION_TOKEN` 으로 싣고, CLI 가 그 env 를 envelope 에 실으면 요청 게이트의 세션 판정(`src/adapters/ipc/handler/gate.rs` 의 `resolve_caller`, 판정 결과 해석은 `crates/tasty-ipc/src/caller.rs` 의 `caller_from_session`)이 `CallerContext::Agent` 로 해석한다. 토큰이 잘못됐거나 만료면 Local 로 떨어지지 않고 거부된다.

#### 테스트

`tasty_telemetry::agent_id::tests` 의 `from_env_all_cases` 하나가 세 경우(미설정 → host / 값 있음 → 그 값 / 빈 값 → host)를 함께 단정한다.

### IPC 진입 관측

GUI·headless의 외부 소켓과 plugin host-call은 라우팅 전에 권한·cap·rate-limit을
검사한다. 통과한 호출만 `ipc_calls`로 한 번 기록하며 App 인터셉트, namespace forward,
전 창 합산, 일반 handler 사이에 예외를 두지 않는다. 일반 handler로 내려가도 다시
소비하거나 기록하지 않는다. 별칭은 canonical 메서드 태그로 남는다.
권한/한도 거부는 Allow 관측을 남기지 않고 기존 Deny audit·권한 격상 흐름을 유지한다.
`telemetry.*`와 호스트 caller의 관측 제외, Local의 rate 면제는 그대로다.
[플러그인 권한 모델](../../dev-guide/plugin-permissions.md).


### Cost Cap

(agent, metric, window) raw sum 이 `threshold` 이상이면 `triggered` + 액션:

| 액션 | 후속 IPC |
|------|----------|
| `notify` | 변화 없음 (알림만) |
| `pause` | 그 plugin agent 의 모든 IPC `-32007 cap_blocked`(과거 `stop` 값으로 저장된 cap 은 `pause` 로 자동 마이그레이션됨) |
| `require_approval` | `approval.request`(severity=warn) 자동 + IPC 차단 → 사용자 응답 후 `cap.reset` 으로 재개 |

차단된 plugin 본인은 `cap.reset` 도 막힌다 — 다른 plugin(`Telemetry` 권한)·Local(CLI)은 reset 할 수 있다. 누적 임계인 cap 과 *시간당 비율*인 [agent rate-limit](../agent-collaboration/index.md) 은 다른 시스템.

### 이상 탐지

`AnomalyDetector` 가 세 휴리스틱을 유지한다 — **진짜 정체/누수 탐지가 아니라 값싼 신호 기반 후보 알림**이라는 전제로 소비해야 한다.

| 휴리스틱 | 판정 | 기본값 |
|---|---|---|
| `CallBurst` | (agent, method) sliding window 호출 카운트 | 60초 1000회↑, 알림 켬 |
| `SlowLoop` | (agent, method, params-hash) sliding window 반복 카운트(동일 파라미터 반복). 응답 시간은 보지 않는다 — 15초보다 짧은 주기 폴링은 정상이어도 걸린다. 이 params-hash 는 **dedup 단위이기도 하다**(아래) | 300초 20회↑, 기록만 |
| `RssSurge` | agent 당 최근 5개 RSS 샘플의 **엄격한 단조 증가**(한 번의 급증에는 반응하지 않음, 추세만) | 5 샘플, 마지막 RSS 256MiB 이상, 알림 켬 |

RSS 값 소스는 caller 타입별로 다르다: **Plugin** 은 host(`tasty-host-plugin::PluginManager`)가 `PluginProcess.child` 의 PID 를 sysinfo 로 30초 간격 직접 sampling(agent 자가 보고는 신뢰 불가 — 정확한 자기 RSS 를 보고할 유인이 없음). **Agent** 는 PID 기반이 구조적으로 불가능(원격/별도 프로세스)해 `telemetry.record` 자가 보고(`metric == "rss_bytes"`)로 받는다. 세 경로 모두 동일하게 `tasty.telemetry.anomaly.*`에 영속하고 설정에 따라 알리며, 1분 쿨다운 dedup 도 공유한다. 다만 **dedup 키가 셋 다 `subject` 인 것은 아니다**:

| 휴리스틱 | dedup 키 | subject |
|---|---|---|
| `CallBurst` | `(agent, kind, method)` | `method` — 키와 같다 |
| `SlowLoop` | `(agent, kind, "{method}#{params_hash:016x}")` | `method` — **키와 다르다** |
| `RssSurge` | `(agent, kind, "rss_bytes")` | `rss_bytes` — 키와 같다 |

`SlowLoop` 만 dedup 키에 `params_hash` 를 덧붙여, 같은 method 라도 파라미터 조합이 다르면 **독립된 loop 로 취급해 각자 쿨다운을 갖는다**(`params_hash` 는 detail 에도 실린다). 여러 surface 를 주기적으로 조회하면 surface 마다 한 건씩 알릴 수 있다.

`SlowLoop`는 쿨다운에 더해 루프 하나를 한 번만 기록한다. 창 안 호출 수가
`anomaly_detection.slow_loop_threshold` 이상인 동안 다시 발화하지 않는다. 창의 호출 수가
그 아래로 줄어든 뒤 다시 넘으면 새 루프다. 응답 시간을 측정하지 않고 정상 폴링도 잡으므로
기본값은 기록만 하고 알림을 띄우지 않는다.

`RssSurge`도 엄격한 증가 추세 하나를 한 번만 기록한다. 평탄하거나 감소한 샘플에서 보고
표시를 풀며, 새 추세가 샘플 수·최소 RSS·기존 60초 쿨다운을 만족하면 다시 발화한다.
호출 창을 정리해도 이어지는 RSS 추세의 표시는 유지한다. 크기는 마지막 샘플의 절대 크기이며
증가량이나 증가율이 아니다. 최소 크기 0은 크기 조건만 없앤다.

`[anomaly_detection]`은 설정 › 기타 › 이상 탐지에서 편집하고 저장하면 다음 판정부터 적용된다. config.toml과 debug 설정 IPC에서도 바꿀 수 있다. 알림 표시를 꺼도 탐지와 기록은 계속되며 숫자 값도 편집할 수 있다.

| 설정 키 | 기본값 | 허용 범위 |
|---|---|---|
| `rss_surge_notify` | true | bool |
| `rss_surge_samples` | 5 | 3~20 |
| `rss_surge_min_mib` | 256 | 0~65536 MiB |
| `slow_loop_notify` | false | bool |
| `slow_loop_window_secs` | 300 | 30~3600초 |
| `slow_loop_threshold` | 20 | 2~10000 |
| `call_burst_notify` | true | bool |
| `call_burst_window_secs` | 60 | 1~600초 |
| `call_burst_threshold` | 1000 | 10~100000 |

`*_notify=false`는 알림 출력만 막는다. 탐지·영속 기록·anomaly list·세션 요약은 유지하며,
그동안 발화한 추세나 루프를 다시 켰다는 이유로 소급 알리지 않는다. 기준을 바꾸어도
관측 창·이미 보고한 표시는 버리지 않는다. 새 기준은 다음 판정부터 적용한다. 창/샘플 수를
줄이면 다음 판정에서 잘라내며, 늘려도 이미 버린 이력은 돌아오지 않는다. 늘린 범위가 온전히
채워지려면 이후 호출·샘플이 쌓여야 한다. RSS 증가가 끊기거나 루프가 새 기준 아래로 내려간
뒤 다시 넘을 때 새 episode를 기록한다.

파일의 범위 밖 값은 경고와 함께 해당 절 전체를 기본값으로 정규화해 저장한다. 실행 중
변경의 범위 밖 값은 거절하며 이전 설정을 유지한다. GUI/headless의 부팅과 설정 적용은
같은 변환으로 공유 탐지기에 반영한다. 탐지기는 설정 크레이트에 의존하지 않는다.

탐지 창과 emit 표시는 메모리 전용이며 앱 재시작 때 비운다. 현재 SlowLoop 창 주기로
빈 호출 창과 쿨다운이 지난 emit 기록을 정리한다. 기록 보존은
[저장소 시스템](../../design/systems/storage.md)의 공통 정책(50시간·5000건)을 따른다.

### 세션 요약

결정론적 순수 집계(LLM 없음): tokens(ipc_calls 제외 metric sum) / ipc_calls(method 별 top-N) / approvals 분포 / anomalies. `workspace_id` 미지정 시 전 워크스페이스 합산(포커스 독립).

### 요청 압력 게이지 (프로세스 축)

`system.pressure`는 caller별 사용량과 별도로 프로세스 전체의 요청 처리 상태를 보여준다.
큐 대기·실행 시간·연결 수·거절·스트림 손실을 고정 크기 메모리에 기록하며 저장소에는 쓰지 않는다.
시간은 count·sum·max를 기록하고 평균은 조회할 때 계산한다. 일부 지연에는 구간별 분포도 제공한다.
DB 설정과 느린 요청 기록은 아래 표처럼 별도 항목이다.
[설계 근거](../../architecture/ipc-server.md)를 참고한다.

| 응답 항목 | 측정 위치 | 포함 대상 |
|---|---|---|
| `queue_before_gate` | 명령이 큐에서 나온 직후 (`App::process_ipc` · `pump_ipc`) | 큐에 앉았던 **전부** — 뒤에 거부될 요청도 센다 |
| `handler_after_gate` | 게이트 통과 뒤 (`handle_checked_request`) | 실제로 **실행된 것만** |
| `plugin_round_trip` | plugin 응답 매칭부 (`PluginManager::handle_plugin_response`) | 응답이 **매칭된 요청만** — 취소·만료된 것은 끝점이 없다 |
| `db` | `MemoryStore` 의 commit 뒤 · `vacuum_if_fragmented` · `checkpoint_truncate` | **성공한** commit 과 commit 당 쓰기 수, VACUUM, 시도된 checkpoint — 거부된 쓰기는 롤백이라 안 센다 |
| `connections` | accept 루프의 자리 획득·반납 (`TcpIpcServer`) | 이 포트에 붙은 **TCP 연결 전부** — 요청을 하나도 안 보내는 attach·mesh 스트림도 센다 |
| `db_pragmas` | DB 를 열 때 한 번 (`tasty_memory::pragma::apply_connection_pragmas`) | 누계가 아니다 — DB(`memory_db` · `state_db`)마다 요청값과 되읽은 실제값 |
| `stream_push` | 스트림 허브의 push 와 write 스레드의 수신 (`tasty_ipc::stream_hub::StreamHub`) | 서버가 attach·mesh 스트림 연결로 **민 프레임** — 요청 하나 없이도 자란다 |
| `queue_admission` | 명령 큐 입장 판정 (`tasty_ipc::admission::CommandAdmission`) | 큐에 **들어오려던** 요청 전부 — 상한에 걸려 돌려보낸 것도 센다 |
| `queue_dispatch` | 큐에서 꺼내는 회차와 실행 직전 (`tasty_ipc::dispatch::DispatchStats`) | 큐에서 **꺼낸** 명령과 그 회차 — 실행을 시작한 뒤 명령 또는 응답 대기자가 lifecycle을 보유한 요청 |
| `keyed_requests` | 멱등 보존소의 판정 (`idempotency::Store::decide`) | **멱등 키를 실은** 요청만 — 한 요청은 자기를 맡은 층의 판정으로 한 번 |
| `slow_requests` | 명령을 꺼낸 직후·다 다룬 직후 (`app::ipc_round::CommandObservation`, gui · headless 같은 자리) + plugin 대기 표의 응답·만료·취소 (`PluginManager::record_origin_hop`) | 호스트 IPC 큐를 지난 요청 중 **합이 문턱(100 ms) 이상인 것만** — `system.pressure` 자신은 안 넣는다 |
| `memory_writer` | memory.db 전담 writer의 접수와 묶음 실행 (`tasty_utils::batch_writer`) | writer에 **접수된** 요청과 거절, 실행한 묶음 크기 분포(`batch_sizes`), 입력 바이트, 묶음 대기·실행·executor 호출 분포, 종결 결과, 미완료 요청 나이·수·바이트, busy 재실행, flush, 지금·최대 잡힌 항목 수와 바이트([관측 계약](../../design/systems/storage-contract.md#저장-실행기-관측)). writer가 없는 시험 구성이면 `null` |
| `state_writer` | GUI state.db writer의 접수와 묶음 실행 (`crate::db::writer`, `tasty_utils::batch_writer`) | 최근 파일·튜토리얼 저장 요청. 필드는 `memory_writer`와 같다. 헤드리스·state.db 미개방이면 `null` |
| `state_db` | state.db writer의 commit 성공 직후 (`StateExecutor`) | **성공한** writer 묶음 commit의 수·시간과 commit당 실행한 요청 수. 부팅 읽기·혼자 실행한 튜토리얼 읽기는 세지 않는다. 헤드리스·state.db 미개방이면 `null` |
| `gate_refusals` | 공통 진입 게이트 (`handler/gate.rs` 의 판정 단계) | 게이트에 **든** 요청 전부(`judged`) — Local 도, 큐를 안 지나는 plugin host-call 도 센다. 거절은 게이트마다 한 칸 |

`queue_before_gate` 안에서 명령을 세는 수가 둘이다. `waits` 는 대기를 기록한 명령 수로, 명령을 **꺼내는 순간** 대기 합·최댓값·분포와 함께 오른다 — `wait_us_mean` 의 분모이고 `wait_us_hist.counts` 의 합과 같다. `commands` 는 회차가 **끝날 때** 그 회차가 꺼낸 수만큼 오르고 `depth_mean`(= `commands / drains`)의 분자다. 조회는 늘 자기 회차 안에서 답하므로 한 답 안에서 `commands` 는 아직 도는 회차(조회 자신 포함)만큼 `waits` 보다 작다 — 평균을 `commands` 로 나누면 최댓값을 넘는 값이 나왔다([IPC 서버](../../architecture/ipc-server.md)).

#### plugin 왕복 시간

`plugin_round_trip`은 호스트 큐나 handler 시간이 아니라 plugin 응답을 기다린 시간이다.
`PluginWaitStats`는 프로세스가 소유하며 새 plugin manager에도 같은 핸들을 전달한다.
게이지를 주입하지 않은 시험 구성에서는 `matched`가 0이다.

#### DB 지연

`DbLatencyStats`의 commit 시간은 handler 실행 시간에도 포함된다. 둘을 비교하면 handler
자체가 느린지 내부 DB 쓰기가 느린지 구분할 수 있다. `MemoryStore`에서 만든 게이지 핸들을
`Core`가 공유하므로 진단 조회가 저장소 뮤텍스를 기다리지 않는다.
checkpoint가 다른 연결의 읽기 때문에 끝나지 못하면 `checkpoints_busy`로 별도 집계한다.
저장소를 열지 않은 시험 구성에서는 `commits`가 0이다.

`writes_per_commit`은 commit 하나에 확정된 쓰기 수의 분포다. `le`가 구간 상한(1·4·16·64·256)이고
`counts`는 그 구간과 마지막의 256 초과 구간의 commit 수다. 단독 쓰기는 1 구간에, 실행 단위·쓰기
묶음은 확정된 쓰기 수의 구간에 들어간다. 행을 지운 정리 연산(`purge_expired`·`purge_scope`·`prune_*`)도
자기 transaction이나 속한 실행 단위의 commit으로 센다. `vacuums`·`vacuum_us_sum`은 부팅 정비의
VACUUM이며 commit 수에 들어가지 않는다.

#### TCP 연결 수와 대기

동시 연결 상한 `MAX_CONCURRENT_CONNECTIONS`는 256이다. 상한에 걸린 연결은 요청을 읽기
전에 거절되므로 요청 시간이나 `ipc_calls`에 포함되지 않는다.

| 필드 | 의미 |
|---|---|
| `live` | 현재 연결 수. 연결 종료 시 감소 |
| `live_max` | 관측한 최대 연결 수 |
| `limit` | 서버의 연결 상한 |
| `accepted` | 수락한 연결 누계 |
| `refused_saturated` | 포화로 거절한 연결 누계 |

포화 진입은 `warn`, 이후 거절은 `debug`로 기록하지만 거절 수는 모두 집계한다.
게이지는 `Core`가 만들고 뒤에 시작되는 IPC 서버에 전달한다. 서버가 없는 시험 구성에서는
`accepted`가 0이다.

연결 수와 함께 **accept 대기 시간의 상한**도 제공한다(`accept_waits` · `accept_wait_bound_us_sum` ·
`accept_wait_bound_us_max` · `accept_wait_bound_us_mean`). accept 루프는 논블로킹이라 큐가 비면 100 ms
자고, 그 사이 도착한 연결은 요청 줄을 읽히기 전에 기다린다 — 큐 대기 계측(`queue_before_gate`)은 줄을
읽은 뒤에 시작하므로 그 시간이 어디에도 안 잡혔다. OS 가 연결을 큐에 넣은 시각은 사용자 공간에서 안
보이므로, 재는 것은 **루프가 큐를 마지막으로 비어 있다고 본 뒤 지난 시간**이다. 그 뒤에 꺼낸 연결은 그
순간 뒤에 도착했으므로 실제 대기 시간은 이 값을 넘지 않으므로 **상한**으로 사용하며, 잠든 동안 고르게 도착하면 실제
대기는 평균적으로 그 절반이다. 집계 대상은 루프가 꺼낸 TCP 연결 전부라 `accept_waits = accepted +
refused_saturated` 다([IPC 서버](../../architecture/ipc-server.md)).

#### DB에 적용된 설정

`db_pragmas`는 누계가 아니라 설정값이다. `memory_db` · `state_db` 두 칸이 각각 `in_memory` ·
`degraded` 와 pragma 넷(`journal_mode` · `synchronous` · `foreign_keys` · `journal_size_limit`)의
`requested` · `effective` · `took` · `error` 를 싣는다. 요청값을 함께 싣는 것은 소스의 `WAL` 이
runtime 보장이 아니어서다 — SQLite 는 요청을 조용히 거절할 수 있다. `degraded` 는 하나라도 그
DB 모드의 허용 결과와 다르다는 뜻이고 **오류가 아니라 열린 채로 쓰이는 상태**다. `db` 항목과
같은 DB 를 말하므로 한 응답에서 "commit 이 느리다" 와 "WAL이 적용되지 않았다" 가 함께 읽힌다. 열리지
않은 DB 는 `null` 이다 — 헤드리스의 `state_db` 는 늘 `null` 이다(허용 결과표·근거는
[storage](../../design/systems/storage.md) 와
[저장 계약과 writer 명부](../../design/systems/storage-contract.md)).
`memory_db` 에만 `init_failure` 가 하나 더 있다 — 부팅이 `memory.db` 를 못 열어 in-memory 대체로
떴으면 `{cause, error}`(원인 이름과 오류 문구), 아니면 `null` 이다. 대체면 pragma가 모두 적용됐어도
`degraded` 가 `true` 다 — `in_memory: true` 만으로는 "원래 in-memory" 와 "파일을 못 열어
in-memory" 가 안 갈린다([저장 계약과 writer 명부](../../design/systems/storage-contract.md)).

#### 스트림 전송

`stream_push`는 서버가 스트림 연결에 전송한 프레임을 집계한다. 필드는 네 개다:
`frames_dropped`(연결이 살아 있는데 그 연결의 큐가 차서 버린 프레임 누계 — **조용한 손실**) ·
`clients_lagged_out`(연속 drop 한도로 끊은 연결 누계 — 소비자가 이미 아는 손실) · `backlog`(지금
살아 있는 연결들의 큐에 쌓여 아직 write 스레드가 안 가져간 프레임 수의 합 — **이 항목에서 유일하게
내려가는 값**) · `sink_capacity`(연결 하나의 큐 상한 — `connections.limit` 과 같은 이유로 값과 함께
나간다). 연결별 **연속** drop 수는 없다 — 성공 한 번에 0 이 되는 강제 연결 해제 조건이라 읽는 시점에 따라
같은 사건이 0 으로 보인다. 세 값의 정의와 손실을 받은 client 가 하는 일은
[Attach 메커니즘](../../dev-guide/attach-behavior.md)
과 [attach-behavior](../../dev-guide/attach-behavior.md) 에 있다. 허브가 엔진에 주입되지 않은
조립(단위 시험)에서는 항목이 `null` 이다.

#### 명령 큐 접수와 실행

`queue_admission`은 큐 접수를, `queue_dispatch`는 큐에서 꺼낸 뒤의 처리를 집계한다. `queue_before_gate` 가
큐에서 나온 명령이 얼마나 기다렸는가를 잰다면, 이 둘은 무엇이 들어오려 했고 무엇이 나갔는가를 잰다.

`queue_admission` 은 접수 통계다(`tasty_ipc::admission::CommandAdmission`). 큐에 든 요청 바이트 합과
호스트 주입 명령 수에 상한이 걸려 있고, 넘으면 요청은 큐에 들어가지 않는다(소켓 요청은 `-32065`,
호스트 주입은 `InjectError::Refused`). 값은 여덟이다 — 지금 값 셋(`queued_bytes` · `queued_commands` ·
`queued_injected`, **내려간다**) · `peak_bytes`(관측된 최고 바이트) · 거절 누계 둘(`refused_bytes` 바이트
상한 · `refused_depth` 주입 깊이 상한) · 그 통계가 집행하는 상한 둘(`limit_bytes` ·
`limit_injected_depth` — `connections.limit` 과 같은 이유로 값과 함께 나간다). 돌려보낸 요청은 큐에 한
번도 안 들어가므로 **다른 어느 항목에도 안 남는다** — 이 항목이 없으면 그 거절은 요청자가 받은
응답과 `debug` 로그로만 드러난다. 접수 통계는 IPC 서버가 만들고 호스트 주입기가 같은 것을 든다. 서버가
안 뜬 조립(단위 시험)에서는 항목이 `null` 이다. 1 바이트의 정의와 상한 값은
[IPC 서버](../../architecture/ipc-server.md).

`queue_dispatch` 는 큐에서 **꺼낸** 쪽의 누계다(`tasty_ipc::dispatch::DispatchStats`). 값은 일곱이다 —
`rounds`(명령을 하나 이상 꺼낸 회차) · `rounds_stopped_by_count` · `rounds_stopped_by_time`(그중 명령 수 ·
시간 예산에 닿아 멈춘 회차) · `expired_before_run`(호출자의 응답 대기 상한이 큐에서 지나 실행하지 않은
명령 — 소켓 요청은 `-32067` 로 답한 것이고, 호스트 주입 명령([IPC 서버](../../architecture/ipc-server.md))도
센다. 주입 명령은 `-32067` 로 답해지지 않고 주입한 호출자 스레드에서 `InjectError::Expired` 로 끝난다) · `started`(실행을 시작한 명령) · `in_flight`(실행을 시작한 뒤 명령 또는 응답 대기자가 lifecycle을 보유한 요청 — 이 항목에서 유일하게 내려가는 값) · `in_flight_max`. 호출자 대기가 끝나도 실행 중인 명령이 lifecycle을 보유하면 계속 센다. 메인
스레드 handler 는 한 번에 하나라 `in_flight` 가 1 을 넘는 것은 응답을 워커로 넘긴 요청이 기다리는
동안이다. 정의는 [IPC 서버](../../architecture/ipc-server.md).
두 항목을 합치지 않는 이유는 집계 대상이 달라서다 — `queued_commands` 와 `started` 의 차는 "아직 큐에
있다" 가 아니다(큐 안에서 만료된 것과 기다리던 쪽이 물러난 것이 섞인다).

#### 멱등 요청

`keyed_requests`는 **멱등 키가 있는 요청만** 센다. 보존소가 내린 판정마다 한 칸이다 —
`executed`(처음 보는 키, 실행했다) · `replayed`(같은 요청, 보관된 답을 냈다) · `conflicted`(같은 키에
다른 요청, 아무것도 실행하지 않았다) · `discarded`(실행은 됐고 답은 버려졌다) · `in_flight`(같은 요청이
진행 중이어서 거기에 합류했다). 전부 누계다. `executed` 는 재시도가 아니지만 **집계 대상**이다 — 재생 수만으로는
그것이 키 실은 실행 열 건 중 하나인지 만 건 중 하나인지 모른다. 한 요청은 자기를 맡은 층의 판정으로
**한 번만** 세진다. 이 `in_flight` 는 `queue_dispatch.in_flight` 와 이름만 같다. 정의는
[IPC 서버](../../architecture/ipc-server.md).

#### 느린 요청 기록

`slow_requests`는 느린 요청을 개별 기록한다(`tasty_telemetry::SlowRequestLog`).
분포는 "100 ms 를 넘은 것이 몇 건" 까지 말하고, 그 한 건의 시간이 큐 · 호스트 · plugin 중 어디에
있었는지와 그 plugin 대기가 **어느 요청의 것**이었는지는 못 말한다 — 이 항목이 그것을 말한다.

- **요청 번호** — `request_seq` 는 호스트가 `IpcCommand` 를 만들 때 매기는 프로세스 전역 단조 번호다
  (`tasty_ipc::server::RequestSeq`). JSON-RPC `id` 가 아니다(호출자 값이라 거의 늘 `1` 이다). Event Bus
  envelope 의 `trace_id` 도 아니다(사건 사슬의 값이고 plugin 이 보낸 값을 그대로 싣는다). 재시작하면 1
  부터 다시 센다.
- **한 줄** — `request_seq` · `host`(`method` canonical 이름 — 모르는 이름은 받은 그대로이고 128 바이트에서 자른다 · `caller` 봉투가 말한 `local`/`agent` ·
  `queue_wait_us` · `host_us`) · `plugin_hops`(hop 마다 `plugin_id` · `host_request_id` · `wait_us` ·
  `outcome` = `ok`/`error`/`expired`/`cancelled`, 최대 셋 — pre-hook · target · post-hook) · `total_us`.
  `host_us` 는 꺼낸 뒤 호스트가 명령을 다 다루기까지(게이트 포함)라 `handler_after_gate` 와 집계 대상이
  다르다. plugin 으로 넘긴 요청이면 넘기는 데까지이고, plugin 을 기다린 시간은 hop 쪽에 있다.
- **호스트 몫의 결과** — `host` 에는 `outcome`(`ok`/`error` — hop 의 `outcome` 과 같은 낱말)과
  `error_code`(오류면 호출자가 받은 JSON-RPC 코드, 아니면 `null`)가 함께 실린다. 값은 **호출자에게 실제로
  나간 답**이다 — 소켓 연결 스레드와 호스트 주입기가 답을 받거나(dispatch 가 보낸 답 · plugin 의 뒤늦은
  답) 상한에서 스스로 만든 순간(`-32067` 실행 전 만료 · `-32061` 결과 불명) 명령의 결과 칸에 한 번 적고,
  링의 줄이 같은 칸을 들고 있어 **읽는 순간의 값**이 나간다. 그래서 큐에서 만료된 요청(`-32067`) ·
  게이트가 거절한 요청(`-32001`) · 정상 답이 링만으로 갈린다. 아직 답이 안 나갔으면(plugin 으로 넘긴
  요청이 기다리는 중) 둘 다 `null` 이다. 기한 없는 주입이 상한에서 물러난 경우도 `null` 이다 — 그 명령은
  큐에 남아 뒤에 실행될 수 있어 호출자에게 나간 답이 없다([IPC 서버](../../architecture/ipc-server.md)).
- **plugin 로그로 되짚기** — `host_request_id` 는 plugin 이 받은 JSON-RPC id 와 같은 값이다. 호스트의
  plugin 오류 응답 경고와 namespace 만료 경고도 같은 줄에 `id=<host_request_id>` 와
  `request_seq=<번호>` 를 싣는다(번호를 모르는 plugin 요청이면 `none` — IPC 요청에서 오지 않은 것과 아래 "집계 대상 밖" 의 파일 핸들러 경로). 번호는 plugin 에게
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
- **집계 대상 밖** — plugin 이 부른 host-call 은 호스트 IPC 큐를 안 지나 번호가 없고 여기 안 든다. plugin 이
  부른 namespace forward 도 번호가 없다. IPC `file_handler.dispatch` 는 요청 자신은 링의 집계 대상에 들지만,
  그 명령이 파일 핸들러 큐를 거쳐 plugin 으로 넘기는 forward 는 번호를 잃는다 — 그 plugin 대기는 원 요청 줄에
  안 붙는다(그 forward 는 호출자에게 답하지 않는다). `host` 가 `null` 인 줄은 열린 자리가 밀려난 뒤 hop 만 온 것이다.

값과 상수의 근거는 [IPC 서버](../../architecture/ipc-server.md).

#### 진입 검사 거절

`gate_refusals`는 공통 진입 검사가 거절한 요청을 센다(`tasty_telemetry::GateStats`). 칸은 넷이다 —
`judged`(게이트에 든 요청, 집계 대상) · `permission_denied`(권한 게이트, `-32001`) · `cap_blocked`(텔레메트리 cap
게이트, `-32007`) · `throttled`(rate limit 게이트, `-32010`). 게이트는 권한 → cap → rate limit 차례로 보고
앞에서 돌려보낸 요청은 뒤를 안 지나므로 한 요청은 많아야 한 칸에 세진다 — 그래서 `judged` 에서 세 거절을
뺀 값이 세 게이트를 모두 지난 수와 같다. 셋을 한 칸으로 합치지 않는 것은 처방이 달라서다(권한을 청한다 ·
cap 을 푼다 · 기다린다). 단 `permission_denied` 는 **권한 게이트가 돌려보낸 `-32001` 만** 센다 — 그
거절은 모두 `-32001` 이지만 역은 아니다. 그리고 권한이 모자란 거절만 담지 않는다 — 권한 게이트는 권한 셋을 가진 호출자(plugin · agent)가 부른 없는 메서드 이름과 그 호출자에게
열리지 않은 메서드도 같은 코드로 돌려보내고, 그 둘은 권한을 청해도 안 풀리고 부르는 메서드를 바꿔야 한다.
Local 호출자는 이 게이트에서 돌려보내지지 않으므로 없는 메서드를 불러도 이 칸에 안 든다. 응답
메시지(`permission_denied: unknown ipc method …` 등)가 셋을 가른다. `-32001` 인데 이 칸에 안 드는 것이 두
부류다 — ⒜ **봉투 토큰 거절**: `session_token` 의 형식 오류 · unknown/expired/revoked 는
세션 판정 단계(`handler/gate.rs` 의 `resolve_caller`)가 판정 단계 **앞**에서 답하므로 `judged` 에도 안
든다(낡은 `TASTY_SESSION_TOKEN` 을 든 CLI 가 이 경로다). ⒝ **게이트를 지난 뒤 핸들러가 내는 `-32001`**: 입력
시뮬레이션 미허용(debug) · macOS 손쉬운 사용 미승인 `surface.raw_key`(debug, macOS) · 세션 발급의 격상 거절 · plugin manager
쪽 거절(자기 namespace 호출 · `ipc.invoke` 부족 · extension filter 차단)은 게이트를 지났으므로 통과 쪽에 든다.
앞 둘은 Local 호출자도 받는다.

- **리셋** — 전부 이 프로세스가 뜬 뒤의 누계다. 창 단위로 비워지지 않고 재시작하면 0 이다. 영속되는
  `agent.rate_limit_status` 의 `throttled_count` 와 집계 대상이 다르다 — 그쪽은 버킷 하나의 수명 동안 재시작을
  넘어 쌓이고, 게이트 밖의 직접 소비 거절도 센다.
- **집계 대상 밖** — 게이트 뒤의 봉투 검사(멱등 키 길이)에서 돌려보낸 요청은 거절 칸에 안 든다(정책이 아니라
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
`LATENCY_BUCKET_BOUNDS_US`가 정본이다. 당시 측정값(큐 대기 최대 25.4 ms, handler 최대
0.48 ms)과 plugin 왕복이 초 단위까지 걸린다는 점을 고려해 정한 경계다.
응답은 항목마다 `bounds_us` 와 `counts` 를 **함께** 싣는다 — 값과 경계가 떨어지면
소비자가 경계를 복제하고 그 복제본이 갈린다. `counts` 는 `bounds_us` 보다 한 칸 길고
**누적이 아니다**: 칸끼리 겹치지 않아 합이 관측 수이고(Prometheus 의 `le` 누적 버킷과
다르다), 마지막 칸은 상한이 없어 "그 상한을 넘었다" 까지만 말한다 — 얼마나 넘었는지는
같은 항목의 `us_max` 가 답한다. **분위수는 호스트가 계산하지 않는다**: 버킷 해상도
안에서만 답할 수 있는 값이라 한 수로 내놓으면 없는 정밀도를 말하게 된다.

`db` 에는 분포가 없다 — 그 게이지는 `tasty-memory` 에 살고 histogram 타입은
`tasty-telemetry` 에 있어 의존 방향이 반대다. `connections` 에도 없다 — 시간이 아니라
자리라 분포를 잴 축이 아니다. `stream_push` 와 그 뒤 세 항목, 그리고 `gate_refusals` 도 시간이 아니라 수라 없다. `slow_requests` 는 시간을 싣지만 분포가 아니라 문턱을 넘은 요청 한 건씩이다.

<a id="덩어리를-가리지-않고-걸리는-것"></a>

#### 모든 항목에 공통인 해석 규칙

아래 셋은 위 `분포` 소절에만 해당하는 서술이 **아니다** — 응답 전체의 성질이다.

두 수의 차는 "거부된 수" 가 아니다 — 게이트를 통과하고도 `handle_checked_request` 를 안
지나는 갈래가 있다(gui 의 app 층 메서드는 그 자리에서 답하고 돌아간다). 그래서 응답은
두 집계 대상을 나란히 두고 뺄셈을 하지 않는다.

관측이 없는 평균은 `null` 이다 — 0 이면 "기다림이 없었다" 와 "잰 적이 없다" 가 같은 값이 된다.

측정 시간 구간을 나누지 않는 한계는 그대로다: 프로세스 수명 누계라 "지금 밀리는 중" 과 "부팅 직후 한 번
밀렸다" 가 같은 max 로 보인다. 분포는 그 한계를 **반만** 푼다 — 부팅 직후의 한 건은 꼬리
칸의 `1` 로 남아 그것이 소수임은 보이지만, 그 한 건이 언제였는지는 여전히 안 보인다.

## 인터페이스

- **AI Agent / CLI**: `telemetry.record(_batch)`(`telemetry` 권한) · `summary/timeseries/top` · `cap.{set,list,remove,status,reset}` · `anomaly.list` · `session_summary`. [reference/api](../../reference/api.md#텔레메트리-telemetry).
- **로컬 운영자 / CLI**: `system.pressure` (local-only) · `tasty list pressure` — 위 "요청 압력 게이지" 절.
- **Claude Code 통합**: `tasty claude install` hook 이 `session-start`→`stop` 의 `wall_time_ms`(대화 압축 뒤의 `session-start` 는 시작으로 치지 않는다), notification 의 `input_tokens`(`tokens: N` 패턴)를 `com_tasty_claude` agent 로 자동 적재. [claude plugin](../../plugins/claude/index.md).

## 관련

- [agent-collaboration](../agent-collaboration/index.md) — rate-limit 과의 구분 · [human-handoff](../human-handoff/index.md) — require_approval
