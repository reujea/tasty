# 다중 에이전트 협업 (Agent collaboration)

- **Status**: Implemented
- **주체**: AI Agent (여럿이 한 인스턴스 공유)
- **ADR**: [ADR-0073](../../adr/0073-task-graph-view-unblock.md)(task-graph 화면 착수 — host builtin surface + workspace popup 두 표면). 선행 보류 결정 [ADR-0066](../../adr/0066-task-graph-view-deferred.md) 은 ADR-0073 로 supersede 됐다.
- **코드**: `agent.*` 핸들러(`src/adapters/ipc/handler/agent.rs`), 영속 `tasty-memory`
- **화면**: 둘 다 같은 데이터를 본다([ADR-0073](../../adr/0073-task-graph-view-unblock.md)) — [DAG 그래프 surface](screens/dag-graph-surface.md)(`tasty new tab --type dag_graph`)는 탭 하나를 점유하는 상주 관찰용, [DAG 목록 popup](screens/dag-list-popup.md)(도구 메뉴 · `KeybindingSettings.toggle_dag_list`)은 목록에서 하나를 골라 잠깐 확인하고 닫는 용도의 workspace 스코프 창이다. IPC/CLI 관측 수단(`agent.task_list`/`task_graph`/`task_get`/`dag_list`/`dag_get`)은 그대로 유효하다.
- **메서드 목록**: [reference/api](../../reference/api.md#에이전트-협업-agent)

## 목적

여러 AI 에이전트가 같은 tasty 인스턴스를 공유할 때 쓰는 협업 primitive 6종. 모두 `agent`(AgentManage) 권한 하나로 grant 된다 — `agent.*` 단일 네임스페이스 + `<verb>_<modifier>` 패턴. 영속은 `tasty.agent.*` memory 키(task/barrier/semaphore/lease 는 `workspace:<id>` scope, rate-limit 은 `global`).

## 내부 동작

### `*_await` — 메서드마다 다르다

`barrier_await` 는 blocking 이 아니라 **현재 상태 즉시 응답**(poll, `barrier_state` 의 alias) — 호출자가 terminal 상태가 아니면 반복 호출한다.

`task_await` 는 **진짜 blocking** 이다(`TaskWakerHub` 기반, 워커 스레드에서 처리). `timeout_ms` 생략 시 기본 10분(600,000ms, 잠정값 — 실사용 경험이 쌓이면 재조정)까지 대기하고, 그 안에 terminal 에 도달하지 못하면 `{"outcome":"timed_out"}` 으로 반환한다. `timeout_ms: 0` 을 명시하면 이 기본값을 우회해 무한 대기한다. **local caller 전용**(`local_only`) — `approval.await` 와 대칭으로, plugin SDK 는 단일 워커 스레드가 요청을 직렬 처리하므로 plugin 이 `task_await` 로 블록되면 자기 자신의 다른 host→plugin 요청을 전혀 처리하지 못한다(자기 자신이 호출한 응답 수신은 별도 경로라 자기-교착까지는 아니지만, 그 task 가 자신을 다시 호출하는 구성이면 dispatch 타임아웃(5s)으로 정상 작업이 실패한다). plugin 은 대신 완료 판정 전략(`[[contributes.completion_strategy]]`)을 선언해 러너가 대신 기다리게 하거나, `task_get` 을 폴링한다.

### 6 primitive

- **Task DAG** — 의존성 그래프 + state 머신. 사이클은 create 시 거부, 상태 변화 시 transitive downstream 자동 재평가. 상태 8종(`waiting/ready/running/succeeded/failed/cancelled/skipped/unknown`). command 4종(`run/custom`(옵션 폴링 `poll` 포함)/`reduce/wait_barrier`). `run` 은 Surface 없는 bare subprocess 로, stdout/stderr 를 각각 마지막 64KiB(tail)까지 캡처해 성공 시 `TaskResult.output`(`{"pid","stdout":{"text","truncated","dropped_bytes"},"stderr":{...}}`)에, 실패 시 에러 메시지에 싣는다(tty 는 미지원 — 필요하면 `pty.*` 사용). OnFailure 3종(`abort`(기본, downstream skip cascade)/`continue_downstream`/`fallback{task}`) — **어느 task에 설정하느냐가 종류마다 다르다**: `abort`/`continue_downstream`은 의존하는 쪽(downstream) task 자신에 설정해야 그 downstream의 readiness 평가에 반영된다. `fallback`은 반대로 실패할 수 있는 쪽(upstream) task 자신에 설정해야 한다 — 그 task가 `Failed`로 전이하는 순간 자기 자신의 `on_failure`를 보고 fallback을 승격시키는 구조이기 때문. `fallback`을 downstream 쪽에 설정하면 의존성 실패로 인한 `Skipped` 전이에는 아무 효과가 없고 해당 task가 `waiting`에 영구히 멈춘다(경고 없이). `fallback{task}`(기존 task 참조)로 지정된 대상은 그 main 이 `Failed`로 전이하기 전까지 **dormant** 상태다 — `depends_on`이 비어 있어도 곧장 `ready`로 뜨지 않고, main이 `Failed`가 되는 순간에야 승격된다. main이 `Succeeded`/`Cancelled`/`Skipped`로 끝나 다시는 `Failed`가 될 일이 없어지면, 그 fallback은 `waiting`에 영구 잔류하지 않고 자동으로 `skipped`로 마감된다. `fallback{inline}`은 애초에 main이 `Failed`로 전이하는 순간에야 task 자체가 동적 생성되므로 이 dormant 상태를 거치지 않는다. **생성 순서 TOCTOU 방지(`reserved_for_fallback`)**: `fallback{task}`는 참조 대상이 참조자보다 먼저 존재해야 하는 생성 순서 제약이 있다 — "fallback task 생성" → "main task 생성(그 fallback을 참조)" 두 호출 사이에 러너가 tick해 그 fallback을 dispatch해버리면(아직 아무도 참조하지 않는 시점이라 정상적으로 `ready`였다면) dormant 정정이 무력화되어 main의 성공/실패와 무관하게 그대로 실행되는 레이스가 있었다. `task_create` 에 `reserved_for_fallback: true`(CLI `--reserved-for-fallback`)를 주면 그 task 는 자신의 `depends_on`이 모두 끝나도 `ready` 를 결코 거치지 않고 `waiting` 에 묶인다 — 나중에 그 task 를 `fallback{task}` 로 참조하는 main 을 만드는 `task_create` 호출이 예약을 해제하고 정상 dormant 판정으로 넘긴다. 두 호출 사이의 지연이 얼마나 길든(러너가 몇 번을 tick하든) 안전하다. 예약만 하고 끝내 참조하는 main 을 만들지 않으면 그 task 는 `waiting` 에 영구 잔류한다(opt-in 계약 — `task_delete` 로 정리 후 재생성). `reserved_for_fallback` 없이(기존처럼) 생성하면 이 보호가 없으므로 두 호출을 촘촘히 이어붙여야 한다. `custom` 의 완료 판정은 poll(반복 호출) 뿐 아니라 push(외부 훅 보고)도 지원한다 — 예: `host/command-completed`(OSC 133 셸 통합 기반, 셸 명령 완료를 훅으로 통지받아 exit code 로 성공/실패를 가름). 전략 레지스트리·배선 상세는 [dev-guide/agent-runner](../../dev-guide/agent-runner.md#완료-판정-전략-레지스트리-srccompletion_strategy), `run` 캡처 상세는 [dev-guide/agent-runner](../../dev-guide/agent-runner.md#run-출력-캡처).
  - **참조 검증 범위**: `task_create` 는 `depends_on` 뿐 아니라 `OnFailure::Fallback{task}` 와 `TaskCommand::Reduce.inputs` 가 가리키는 task id 의 존재도 생성 시점에 검증한다(`crates/tasty-agent/src/task/store.rs`) — 미존재면 `-32602` 로 거부. `Fallback{inline}` 은 생성 시점엔 대상이 존재하지 않는 게 정상(실패 전이 시 동적 생성)이라 검증 대상이 아니다. 이 검증은 **신규 생성만** 막는다 — 검증 도입 이전에 이미 저장된 dangling 참조는 마이그레이션하지 않고 그대로 남는다(그 참조를 쓰는 task 는 관측 가능하게 영구 `waiting` 에 머문다; dangling fallback 은 실패 전이 시 `tracing::warn!` 도 남긴다).
  - **노드 간 데이터 흐름(`${task.<id>.output<pointer>}`)**: `depends_on` 은 실행 순서만 묶으므로, 선행 task 의 결과를 후행 task 의 **입력**으로 넘기려면 이 placeholder 를 쓴다 — dispatch 직전에 upstream 의 `result.output` 에서 RFC 6901 JSON Pointer 로 값을 뽑아 `Custom.params`(JSON 트리 전체) / `Run.command` 인자 / `Run.cwd` 에 주입한다. 포인터를 생략하면(`${task.t-a.output}`) 출력 전체. `claude.spawn` 의 child surface id 처럼 `task_create` 시점엔 알 수 없고 런타임에야 정해지는 값을 넘기는 게 주 용도이며, 이게 있어야 `spawn` → 그 자식에게 `tell` 이 한 DAG 로 표현된다. **타입이 보존된다** — 문자열 값이 정확히 placeholder 하나뿐이면 뽑아낸 JSON 값으로 통째 교체되므로 숫자는 숫자로 남는다(`require_surface_id` 류의 `as_u64` 검사를 통과). 다른 텍스트에 섞여 있을 때만 문자열 보간이다. 참조 대상은 **`depends_on`(∪ `Reduce.inputs`)에 선언돼 있어야 하고**, 아니면 `task_create` 가 `-32602` 로 거부한다 — 선언하지 않으면 upstream 이 끝나기 전에 dispatch 되는 race 가 되기 때문. 해석 실패(참조 task 없음/`result` 없음/포인터 미스)와 문법 오류는 조용한 `null` 이 아니라 실패로 드러난다. 상세: [dev-guide/agent-runner §선행 task 출력을 파라미터로 넘기기](../../dev-guide/agent-runner.md#선행-task-출력을-파라미터로-넘기기--taskidoutputpointer).
  - **`Reduce.inputs` 는 암묵적 의존성**이다 — `depends_on` 과 합쳐 그래프 엣지·사이클 검출 대상이 된다. 다만 readiness 의미는 `depends_on` 과 다르다: reducer(특히 `all`)는 실패한 입력도 의도적으로 수집하므로, 입력 하나가 실패했다고 이 task 를 `skipped` 로 몰지 않는다 — 입력 전부가 *종결*(성공이든 실패든)될 때까지만 `waiting` 을 유지하고, 종결되면 `ready` 로 진행한다.
  - **관측(`task_get`/`task_graph`)**: `task_get` 의 CLI 텍스트 렌더는 `command`(kind 요약)/`depends_on`/`on_failure`/`metadata` 를 함께 보여준다 — "이 task 가 정말 저 fallback/의존 task 에 게이트돼 있나" 같은 감사 질문을 raw JSON 파싱이나 memory store 우회 조회 없이 CLI 만으로 답할 수 있다. `task_graph` 의 `nodes` 는 `command_kind`/`on_failure_kind` 를 함께 싣고, `edges` 는 `depends_on`/`fallback`(`OnFailure::Fallback.task`)/`reduce`(`Reduce.inputs`) 세 `kind` 로 태그된다 — 위 "참조 검증 범위"가 보는 3종 참조를 그대로 시각화한 것이다. **단, `fallback` 은 참조 관계일 뿐 사이클 검출 대상이 아니다**: 사이클 검출(`detect_cycles()`/`TaskGraph::dfs_cycle`)은 `depends_on`/`Reduce.inputs` 만 순회하고 `OnFailure::Fallback.task` 는 보지 않는다 — `A`→(`fallback:F`), `F`→(`fallback:A`) 처럼 서로를 fallback 으로 참조하는 순환은 생성 시점 존재 검증만 통과하면 그대로 저장되고, `task_graph` 로 관측은 되지만 `-32602` 로 자동 차단되지 않는다. `dot` 포맷은 `depends_on` 을 실선, `fallback` 을 주황 점선, `reduce` 를 파랑 점선으로 구분해 그린다.
  - **DAG 그룹(`dag_list`/`dag_get`)**: Tasty 의 영속 모델에 DAG 레코드는 없다 — task 는 workspace 에만 속하고 `task_list`/`task_graph` 는 그 workspace 를 통째로 다룬다. 한 workspace 에서 서로 무관한 그래프를 여럿 돌리는 사용(conductor 류)이 정상이므로, "workspace = DAG" 로 간주하지 않고 **DAG 를 도출**한다: ① `task.metadata.dag` 가 문자열이면 그 값이 그룹 키(**explicit**) — 연결성과 무관하게 같은 키끼리 한 DAG 로 묶인다. ② 나머지는 **약연결 컴포넌트**(**derived**) — 엣지는 `task_graph` 가 그리는 것과 같은 4종(`depends_on` ∪ `Fallback.task` ∪ `Reduce.inputs` ∪ `metadata.fallback_of` 역참조)을 무방향으로 본다. `metadata.dag_name` 을 붙이면 표시 이름이 된다(없으면 explicit 키 > root task 이름 순). `metadata` 를 쓰는 이유는 `Task` 에 필드를 새로 박으면 이미 저장된 task 가 전부 "DAG 없음" 으로 떨어져 기존 그래프가 목록에서 사라지기 때문 — 도출은 마이그레이션 없이 기존 task 를 자동 편입시킨다(`semaphore`/`lease`/`fallback_of` 가 이미 쓰는 확장 지점과 같은 관례).
    - DAG id 는 explicit `d:<metadata.dag 값>` / derived `c:<root task id>`(그룹 내 `(created_at, id)` 최소). 같은 task 집합이면 호출 때마다 같은 id 가 나온다(화면이 선택 상태를 id 로 들고 폴링마다 재계산하므로 결정론이 계약이다). 완전한 신원은 `(workspace_id, id)` — explicit 키는 사용자가 정하므로 두 workspace 가 같은 값을 쓸 수 있다.
    - `dag_list` 응답의 각 원소: `id`/`workspace_id`/`name`/`source`(`explicit`|`derived`)/`task_count`/`state_counts`(8종)/`rollup_state`/`created_at`/`updated_at`/`root_task_ids`/`has_cycle`, 그리고 `include_tasks:true` 일 때만 `task_ids`. `rollup_state` 판정 순서는 `running` > `failed` > 전부 terminal(`succeeded`/`skipped`) > `ready` > `waiting`.
    - **열거 범위**: `workspace_id` 를 생략하면 *지금 살아있는* workspace 전부를 순회한다(포커스 독립 — 활성 workspace 에 의존하지 않는다). 삭제된 workspace 에 남은 고아 task 는 뜨지 않으며, 응답이 `scope: "live_workspaces"` 로 그 사실을 명시한다 — 고아 정리는 부팅 시 자동 GC 의 책임이다.
    - `dag_get` 은 그 DAG 부분집합만으로 `task_graph` 와 **동일한** `nodes`/`edges`(또는 `--format dot`)를 낸다 — 렌더 규칙은 한 벌을 공유하므로 두 표면이 갈라지지 않는다.
  - **삭제(`task_delete`)/일괄 삭제(`task_purge`)**: 참조(`depends_on` ∪ `Fallback.task` ∪ `Reduce.inputs`)가 있는 task 삭제는 기본 거부하고 참조자 목록을 `error.data.referenced_by` 에 실어 반환(`-32010`) — dangling 참조로 인한 `create()` 실패(`UnknownDependency`)·downstream 영구 `waiting` 을 막기 위함. `cascade:true` 는 전이적 참조자 전부를 함께 지우고, `force:true` 는 참조 검사만 우회한다(dangling 참조는 호출자 책임). 삭제 금지 상태는 `running` 하나뿐이다(`-32011`) — `waiting`/`ready`/종결 상태는 `cascade`/`force` 여부와 무관하게 항상 허용된다. 이 제약이 terminal 로 좁지 않고 `running` 하나뿐인 이유: 방치된 `waiting` task(예: 입력이 끝나지 않는 `Reduce`)를 terminal-only 제약으로는 영원히 못 지우고, 그게 참조로 자기 입력들을 붙잡아 그 입력들도 영영 GC 대상에서 빠지기 때문. `task_purge` 는 상태 이름 목록(`states`)·경과시간(`older_than_ms`) 필터로 후보를 고르되, 후보 집합 밖에서 참조되는 task 는 자동으로 보존(`retained`)한다 — `dry_run:true` 로 실제 삭제 없이 계획만 확인할 수 있다. 삭제가 실제로 이뤄지면 `tasty.agent.handle.<id>`/`tasty.agent.run_result.<id>` side-key 도 함께 정리된다.
- **Barrier** — N회 signal 모이면 닫히는 게이트. `timeout_ms` 경과 시 `timed_out` 으로 **lazy 전이**(별도 스레드 없음 — signal/state/list 호출 시 도장).
- **Semaphore** — N permit 동시 점유. 같은 holder 재acquire 는 idempotent(retry-safe)이며 그 홀더의 `acquired_at`/`expires_at` 를 갱신한다. permit 회복은 그 holder 의 release, 또는 `--ttl-ms` 로 잡은 permit 의 만료뿐이다 — **만료는 opt-in 이고 기본은 만료 없음**(오래 걸리는 정당한 작업의 permit 이 회수돼 두 홀더가 동시에 들어가는 것이 교착보다 나쁘다). 한도는 `semaphore-set-permits` 로 제자리에서 바꾸며, **축소는 drain** — 기존 홀더를 강제 회수하지 않고 새 acquire 만 거절해 수렴시킨다([ADR-0119](../../adr/0119-agent-semaphore-resize-and-holder-expiry.md)). **동시성 제한(concurrency limit) 용도로도 쓴다**: `task.metadata.semaphore = { name }` 를 태그한 task 들은 그 세마포어 permit 수만큼만 동시 `Running`, 초과분은 자동 `Ready` 대기 — `task-create --concurrency-limit <name>` 이 이 태깅을 대신해 준다. 절차·라이브 예시는 [dev-guide/agent-runner §동시성 제한](../../dev-guide/agent-runner.md#동시성-제한-concurrency-limit).
- **Lease** — 협조적(advisory) 자원 점유 마커 + TTL. OS 락 아님(위반 감지 수준). mode `fail`(충돌 시 `-32009`) / `block`(`acquired:false`). 만료는 list/acquire 시 lazy evict. **pool 모드**(`task.metadata.lease.candidates`): 단일 `resource` 대신 후보 배열을 선언하면 N개 중 하나를 배정받고, dispatch된 task 는 실제 배정 자원을 `command`(Run.cwd/Custom.params — `${lease.resource}` placeholder 치환)로 전달받는다. `elastic`(명시적 opt-in, 기본은 candidates 안에서만 도는 fixed) 이면 소진 시 `overflow_prefix+N` 새 후보를 원자적으로 합성한다(pool 소진 시 `-32012`). 상세·라이브 예시는 [dev-guide/agent-runner §자원 풀 배정](../../dev-guide/agent-runner.md#자원-풀-배정-lease-pool--candidateselastic).
- **Reducer** — N task 결과를 단일 값으로 합성. 5전략: `first_success`/`all`/`merge_json`/`concat_text`/`custom`(호스트 shell, stdin 에 결과 배열 JSON). 단발 `agent.task_reduce` 또는 DAG 노드(`TaskCommand::Reduce`, 이 경우 `inputs` 는 암묵적 의존성 — 위 Task DAG 항목 참조).
- **Rate-limit** — (agent, metric) token bucket(보충률 `limit/per_ms`, 상한 `burst`). `global` scope. 누적 임계인 [telemetry cap](../telemetry/index.md) 과 구분(이쪽은 *시간당 비율*). CRUD + `try_consume` 제공 + IPC dispatcher 미들웨어(`should_rate_limit`)가 매 호출 자동 평가.

## 자식 에이전트를 DAG 노드로

Claude / Codex 자식을 **띄우고 그 완료를 기다리는 일**을 Task DAG 노드 하나로 표현할 수 있다. 전용 command kind 는 없다 — `custom` 노드로 plugin 의 spawn 메서드를 호출하면 된다. 코어는 어떤 에이전트인지 모른 채 IPC dispatch → 폴링만 한다.

```sh
tasty agent task-create --workspace-id 1 --name spawn-worker \
  --command '{"kind":"custom","ipc_method":"claude.spawn",
              "params":{"surface_id":560,"workspace":"wt-11",
                        "cwd":"/home/me/work/wt-11","prompt":"이 디렉토리의 테스트를 고쳐라"}}'
```

**`poll` 을 주지 않는 것이 요점이다.** 두 plugin 이 자기 매니페스트에 `[[contributes.completion_strategy]]` 를 선언하면서 `default_for_methods` 로 자기 `spawn`/`tell` 메서드를 지목해 두었으므로, 러너가 dispatch 시점에 그 전략을 자동으로 집어 폴링 모드로 전이한다. 전략이 없으면(= 해당 plugin 이 비활성이면) `custom` 노드는 dispatch 성공 즉시 `Succeeded` 가 되므로, 자식이 도는 동안 기다려주지 않는다.

`params.surface_id` 는 자식을 매달 **부모** surface 다. spawn 응답의 `child_surface_id` 가 `map_from_response` 를 타고 poll 호출의 파라미터로 옮겨간다.

두 plugin 의 전략 값이 다르므로 그대로 옮겨 쓰면 안 된다:

| | claude | codex |
|---|---|---|
| dispatch 메서드 | `claude.spawn` · `claude.tell` | `codex.spawn` · `codex.tell` |
| poll 메서드 | `claude.state` | `codex.state` |
| `map_from_response` (spawn) | `child_surface_id` → `surface_id` | `child_surface_id` → `surface` |
| `map_from_response` (tell) | `surface_id` → `surface_id` | `surface_id` → `surface` |
| `terminal_states`(성공) | `idle`, `needs_input` | `idle` |
| `failure_states`(실패) | `exited` | `exited` |

poll 파라미터 키가 다른 것은 각 plugin 의 state 핸들러가 요구하는 이름이 다르기 때문이고(`surface_id` vs `surface`), codex 의 목록에 `needs_input` 이 없는 것은 관측이 안 돼서가 아니다 — codex `PermissionRequest` hook 이 그 상태를 실제로 세운다([plugins/codex](../../plugins/codex/index.md)). 두 plugin 이 갈리는 것은 **그 상태를 노드 종결로 볼지**다: claude 는 되묻기를 종결로 보고(사람이 답할 때까지 기다리는 것이 그 노드가 할 수 있는 전부), codex 는 도구 실행 승인이라 답이 오면 같은 턴이 이어지므로 종결로 보지 않는다. 지금은 그 판단을 바꾸지 않는다 — 바꾸면 `codex.spawn` 노드의 의미가 달라진다. spawn 과 tell 은 소스 키도 다르다: spawn 응답은 `child_surface_id`(새로 만든 자식), tell 응답은 `surface_id`(이미 존재하는 대상)를 싣는다.

### 이어서 지시 주기 (`tell`)

`tell` 에도 기본 전략(`tell-wait`)이 있어 **자식이 그 지시를 마칠 때까지** 노드가 `running` 에 머문다. spawn 노드와 달리 `surface_id` 는 **자식** surface 다:

```sh
tasty agent task-create --workspace-id 1 --name tell-worker \
  --command '{"kind":"custom","ipc_method":"claude.tell",
              "params":{"surface_id":561,"message":"방금 고친 테스트를 다시 돌려라"}}'
```

호스트 `terminal.tell` 은 본문 주입에 성공한 직후 대상 자식의 `idle`(과 `needs_input`) 플래그를 내린다 — 그러지 않으면 첫 폴링 tick 이 직전 턴의 상태를 읽고 자식이 답을 시작하기도 전에 노드를 완료 처리한다. `terminal.broadcast` 도 같다.

### 실패 판정

자식이 죽으면(`exited` — 프로세스 사망 또는 surface 소멸) 그 노드는 **`failed`** 다. 두 plugin 모두 `exited` 를 `failure_states` 로 분류하므로, 자식 에이전트 노드에서도 `on_failure`(`abort` / `continue_downstream` / `fallback`)가 그대로 동작한다 — 기본값 `abort` 면 downstream 이 `skipped` 로 cascade 된다. 실패 메시지에는 상태값과 poll 응답 요약이 함께 실린다.

claude 의 `needs_input`(사람 승인 대기)은 **성공** 쪽에 남는다. 사람이 승인해주면 이어서 끝나는 상태라 영구 실패가 아니고, 두 plugin 의 spawn/tell 완료 알림이 이미 `idle` 과 동일 취급하는 계약을 깨지 않기 위함이다. 승인 대기를 실패로 보고 싶으면 그 노드에 인라인 `poll` 을 주어 `failure_states` 를 직접 지정한다.

앞 노드의 산출물을 뒤 노드 파라미터로 넘기는 것(예: spawn 이 만든 자식의 surface id 를 뒤따르는 tell 노드에 넘기기)은 위 "노드 간 데이터 흐름(`${task.<id>.output<pointer>}`)" 이 다룬다.

전략 레지스트리·dispatch 배선 상세는 [dev-guide/agent-runner](../../dev-guide/agent-runner.md#hostexecutor-매핑).

### 사건으로 받기 — 폴링하지 않고

종결 사실은 Event Bus 로도 나간다. plugin 이 `agent.task_finished` · `agent.barrier_closed` 를 구독하면 `task_get` 을 되풀이해 묻지 않아도 된다. 두 키의 payload·등급·구독 조건은 [reference/event-catalog](../../reference/event-catalog.md#agent-scopesystem-experimental) 이 정본이고, 무엇을 싣고 무엇을 안 싣는지의 근거는 [ADR-0321](../../adr/0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md) 이다.

경계 셋만 여기 적는다.

- **종결만 나간다.** `waiting`/`ready`/`running` 으로 들어가는 전이는 발화 대상이 아니다 — 종결에는 모든 진입 경로가 지나는 단일 깔때기(`task_await` 를 깨우는 그 자리)가 있고 비종결에는 없다.
- **lease 만료는 사건이 아니다.** 만료는 전이가 아니라 조회 시점에 평가되는 술어라 "언제 일어났다" 가 없다.
- **왜 실패했는지는 payload 에 없다.** 그 자리는 `task_id` 로 `task_get` 을 부른다 — 구독 권한과 호출 권한이 다른 것이 그 이유다.

발화는 `task_await` 의 blocking 동작과 간섭하지 않는다. 같은 호출이 대기자에게 보내고 피드에 적을 뿐이고, **대기자가 없어도 피드에는 적힌다.**

plugin 이 아닌 쪽은 `events.fetch` 로 같은 사건을 **위치로** 읽는다. 부를 수 있는 것은 **로컬 호출자뿐**이다 — 세션 토큰을 들고 붙은 외부 에이전트는 이 메서드를 직접 못 부르고, 로컬 소켓의 `tasty` CLI 를 경유한다. `tasty events follow --filter 'agent.*'` 가 그 루프이고, 한 줄에 한 사건씩 JSON 으로 찍으므로 셸에서 `while read` 로 받는다. 끊겼다 다시 붙을 때는 마지막 위치를 그대로 주면 그 사이 사건부터 이어 받고, 기다린 사이 위치가 링 밖으로 밀렸으면 조용히 처음부터 주는 대신 건너뛴 수를 알린다. 재시작을 넘는 재부착은 세대를 함께 준다 — 연결이 끊기면 `follow` 가 다시 붙을 `--offset` · `--epoch` 을 stderr 에 찍고 끝나고(`--reconnect` 면 1 초마다 다시 붙는다), 그 세대가 새 세대와 다르거나 위치가 새 피드의 끝보다 뒤면 stderr 로 알리고 새 세대의 처음부터 잇는다([ADR-0407](../../adr/0407-events-follow-carries-the-generation-across-a-reattach.md)). 커서를 소비자가 드는 이유와 재시작이 위치를 리셋하는 이유는 [ADR-0322](../../adr/0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) · [ADR-0323](../../adr/0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md).

## 인터페이스

- **AI Agent / CLI**: `tasty agent {task-create,task-list,...,dag-list,dag-get,barrier-*,semaphore-*,lease-*,task-reduce,rate-limit-*}`. `--command`/`--metadata` 는 인라인 JSON 또는 `@path`. 전체 표 → [reference/api](../../reference/api.md#에이전트-협업-agent).
- **state 필터는 콤마 다중값** — `task-list --state`(단수) 와 `task-purge --states`(복수)는 플래그 이름만 다를 뿐 같은 파싱을 쓴다: `--state waiting,ready,running` 처럼 여러 state 를 OR 로 매칭하고, 단일값도 그대로 동작한다. "아직 안 끝난 task 가 남았는가" 를 한 번의 조회로 판정하는 완료 감지의 기본 패턴. 예시 → [dev-guide/agent-runner §CLI 예](../../dev-guide/agent-runner.md#cli-예).

## 재시작 동작

task 는 영속되지만(`Scope::Workspace`) runner thread 는 in-memory 다 — 호스트 재시작 후 **runner 는 자동으로 켜지지 않는다.** 대신:

- **부팅 시 정화만 1회** — 라이브 workspace 전부에 대해 stale semaphore/lease holder 회수 + 직전 `Running` task 를 `Failed("host restart")` 로 마감 + persisted `DispatchHandle` reload(살아있는 `ShellProcess`/`PolledDispatch`/`BarrierPoll`/미만료 `AwaitExternal` 는 복원, 죽은 건 마감). runner thread 는 여전히 안 켜져 있으므로 복원된 handle 을 실제로 poll 하려면 수동(또는 plugin) `agent.task_run --action start` 가 필요하다.
- **같은 부팅 정화 경로에서 자동 GC 도 함께 돈다** — 상태 무관 + 잠정 임계값(7일) 이상 방치된 task 를 `task_purge` 와 동일한 참조 안전 로직(`plan_sweep`/`apply_sweep_plan`)으로 쓸어낸다. memory 자체 TTL(`PutOpts.expires_at`)은 쓰지 않는다 — TTL 만료는 참조 무결성·상태 검사를 우회해 dangling 참조를 재도입하기 때문. 상세: [dev-guide/agent-runner](../../dev-guide/agent-runner.md#자동-gc).
- **정지 상태는 조회로 드러난다** — `task_list`/`task_graph` 응답에 `runner: { running, crashed, ready_count, running_count, store_error, list_failures }` 가 동반된다. runner 가 꺼져 있어도 `ready_count`/`running_count` 는 store 의 실제 값이라, "할 일은 있는데 아무도 안 돌리고 있다"가 이 응답만으로 드러난다. store 를 못 읽으면 두 카운트는 `null` + `store_error` 이고(0 으로 흡수하지 않는다), 러너가 살아 있는데 계속 못 읽는 상태는 `list_failures` 로 드러난다. `task_get` 은 `AwaitExternal` 로 외부 신호를 기다리는 task 에 `awaiting_external: { wait_key, deadline_ms }` 를 실어 "그냥 running" 과 구분한다.
- **`hook_task_waits`(push 완료 전략의 hook_id → task_id 매핑)는 비영속** — 재시작하면 그 task 는 훅으로는 깨어날 수 없다. 대신 `AwaitExternal` handle 자체가 `deadline_ms` 를 들고 다니므로(핸들은 영속), 다음 재시작의 reload 가 만료 여부를 독자적으로 판정해 마감한다.
- `agent.task_run` 은 plugin 도 호출 가능(`AgentManage`) — 자동 시작이 없으므로 plugin 이 자기 workspace 의 runner 를 스스로 되살릴 수 있어야 하기 때문이다.

상세: [dev-guide/agent-runner](../../dev-guide/agent-runner.md#재시작-계약).

## 에러 코드

`-32004`(not found) · `-32008`(already terminal) · `-32009`(lease conflict) · `-32010`(task 참조 중 — `task_delete` 기본 거부, `error.data.referenced_by` 에 참조자 목록) · `-32011`(task 가 `running` — 삭제 불가, `cancel` 선행 필요) · `-32012`(lease pool 소진 — fixed 전부 점유 중이거나 elastic `max_candidates` 상한 도달, mode `fail`) · `-32602`(사이클/미존재 dep(`depends_on`/`Fallback.task`/`Reduce.inputs`)/잘못된 strategy/`depends_on` 밖을 가리키거나 문법이 깨진 `${task.<id>.output…}` 참조 등) · `-32603`(internal).

**이름·id 의 문자 규칙.** semaphore·barrier 의 `name`, task 의 `id`, rate-limit 의 `id` 는 그대로 memory 키의 한 조각이 되므로 memory 키 규칙([design/systems/memory](../../design/systems/memory.md) — 소문자 `a-z`·`0-9`·`.`·`_`·`-`, 접두사 포함 256 바이트)을 따른다. 어기면 `-32602` 이고, 메시지는 **호출자가 준 값 기준** 좌표로 무엇이 틀렸는지 말한다(`semaphore name "v6S": invalid char at 2: 'S' (allowed: …; at most 234 bytes)`) — 상한 바이트 수는 종류마다 접두사 길이만큼 다르다. 판정은 생성뿐 아니라 그 값으로 키를 만드는 모든 호출(acquire·release·delete·get 등)에서 같다. lease 의 `resource` 는 키에 넣기 전에 인코딩하므로 이 규칙이 없다. 인코딩 대신 판정을 고른 근거는 [ADR-0517](../../adr/0517-an-agent-primitive-name-is-judged-before-it-becomes-a-memory-key.md).

## 관련

- [telemetry](../telemetry/index.md) — rate-limit vs cap 구분 · [human-handoff](../human-handoff/index.md) — approval
- [design/systems/memory](../../design/systems/memory.md) — 영속 backing store
- [dev-guide/agent-runner](../../dev-guide/agent-runner.md) — task runner 내부 동작(dispatch/poll, 완료 판정 전략 레지스트리)
- [ADR-0073](../../adr/0073-task-graph-view-unblock.md) — task-graph 화면 착수 결정(두 표면·host builtin 근거) · [ADR-0066](../../adr/0066-task-graph-view-deferred.md) — 그 이전 보류 결정(superseded)
