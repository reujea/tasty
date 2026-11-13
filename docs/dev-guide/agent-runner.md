# Agent task runner

## 원칙과 현재 동작

[플러그인 경계](../principles/plugins.md)에 따라 특정 제품에 대한 실행 규칙은 해당 플러그인이 소유한다. 공용 child_env가 지우는 제품 세션 변수는 설치·활성 플러그인의 매니페스트 선언에서 온다. 일반 task·후처리·재시도 계약 자체를 특정 제품 기능으로 보지는 않는다.

workspace마다 스레드 하나가 준비된 task를 실행하고 완료 여부를 확인해 DAG를 진행한다.
상태 전이·영속 저장·custom reducer의 기본 셸 runner(`run_custom_shell`)는
[`tasty-agent`](../../crates/tasty-agent/)가 맡고 실제 task 실행은 host가 맡는다.
IPC/CLI 명세는 [API의 agent namespace](../reference/api.md)를 따른다.

Codex 부모가 `tasty codex spawn`으로 만든 Codex 자식의 턴 결과는
[Codex 부모에게 결과 반환](../plugins/codex/index.md#codex-부모에게-결과-반환)으로 자동으로 돌아간다.
runner의 성공 판정과 부모 대기열 수락은 서로 다른 증거다.

## 구성

| 파일 | 역할 |
|------|------|
| `crates/tasty-agent/src/runner.rs` | `TaskExecutor` trait + `RunnerLoop::tick`(순수 로직) |
| `crates/tasty-agent/src/platform/` | cross-platform pid liveness probe(`process_alive`) |
| `crates/tasty-task-runtime/src/runner_host.rs` | `HostExecutor` — `TaskExecutor` host 구현 + `RunnerContext`(memory + agent_seq + host_ipc injector). 점유 획득·반환은 `runner_host/acquire.rs`, command 종류별 dispatch 는 `runner_host/dispatch.rs`, handle 완료 확인은 `runner_host/poll.rs` |
| `crates/tasty-task-runtime/src/runner_thread.rs` | `RunnerRegistry` — workspace 별 thread start/stop/status + 재시작 후 정리 |
| `crates/tasty-task-runtime/src/service.rs` | `TaskService` — `AppServices.tasks` 로 조립되는 작업 실행 서비스. `RunnerRegistry`·`HookTaskWaits` 를 소유하고 `RunnerContext` 를 만든다. engine별 자원(task ID 순번·완료 대기 허브·사건 큐)은 `EngineSession.task_scope` 의 `TaskScope` 로 받는다. `TaskAwaiter` 는 대기자 등록 → 저장소 조회 → 대기 순서의 완료 대기 계약이다 |
| `crates/tasty-task-runtime/src/task.rs` | `TaskService` 의 작업 API(생성·조회·취소·재시도·상태/결과 기록·reducer 입력 수집·삭제·정리, DAG 목록·조회, 훅 완료 반영 `resolve_hook_task_wait`). 서비스를 받지 않는 DAG 화면용 목록 함수 `task_list_in`·`dag_list_in` 은 writer 요청 본문 안에서 저장소를 받아 서비스와 같은 구현으로 읽는다. 연산 본문은 `task_tx.rs` 의 `TaskTx` 가 writer 복합 연산 하나 안에서 실행하고, 대기자 깨우기·정리 시작 같은 효과는 `TaskEffects` 로 모으며, `terminal_recovery.rs` 가 commit 후 또는 결과 불명 뒤의 확인 읽기로 종결 의무를 전달한다 |
| `crates/tasty-ipc/src/host_call.rs` | `HostIpcInjector` — runner thread 가 plugin IPC 를 동기 호출하는 통로 |
| `src/adapters/ipc/handler/agent/` | `task`/`barrier`/`semaphore`/`lease`/`ratelimit` IPC 핸들러 |

App의 `task_completion` adapter는 현재 등록된 완료 전략을 실행 크레이트의 `CompletionResolver`에 제공한다. runtime은 전역 App registry나 Core/View를 조회하지 않는다. DAG 대상 workspace 목록도 App이 명시 값으로 전달한다.

## 모델

`TaskExecutor` 는 `dispatch`(비차단 실행 시작 → 핸들) + `poll`(1 tick 현재 상태) 두 메서드. 호스트는 polling interval 마다 `RunnerLoop::tick` 호출:

```text
1. Running task → executor.poll(handle)
   Active → 유지 / Done(result) → Succeeded / Failed(err) → Failed
2. Ready task → executor.dispatch(task)  → DispatchOutcome 3-way:
   Started(h)    → handle 보관 + Ready→Running
   Deferred      → 이번 tick 불가(permit 부족 등). state 전이 X, 다음 tick 재평가
   PermanentFail(e) → ImmediateFail handle wrap → 다음 tick poll 에서 Failed 흡수
```

state 전이는 `tasty-agent` 의 `is_valid_transition` 표를 따른다. `Ready→Failed` 직접 전이는 불허라, dispatch 실패도 *먼저 Running 으로* 보낸 뒤 다음 tick 에서 Failed 로 흡수한다.

`Started` 뒤 Ready→Running 전이가 확정되지 않으면(저장 실패·결과 불명) 실행은 이미 시작됐으므로 같은 task 를 다시 dispatch 하지 않는다. 러너는 그 handle 과 시작할 회차 id 를 `RunnerLoop::spawned` 에 두고 다음 tick 의 snapshot 으로 이어 간다.

| 다음 snapshot | 처리 |
|---|---|
| Running, 같은 회차(결과를 모르던 전이가 확정됨) | 그 handle 을 지켜본다(poll·완료 보고·permit 반환은 일반 Running 과 같다) |
| Ready, 같은 회차 | Running 전이를 다시 시도한다. 실패하면 그대로 두고 다음 tick 에 다시 본다 |
| 종결(취소 등) | `release_permit` 으로 실행을 멈추고 점유를 돌려준다 |
| 회차가 다르거나 Waiting·Unknown | 이 실행은 저장소에서 시작한 적 없는 회차라 멈추고 점유를 돌려준다. 그 뒤 Ready 가 되면 새로 시작한다 |
| snapshot 에 없음 | 목록을 읽지 못했을 수 있어 그대로 둔다 |

v1 task 는 회차 id 가 없어 상태만 본다. 시작 뒤 handle 저장이 확정되지 않아도 executor 는 그 task 를 지켜보는 것으로 기록한다. 결과를 모르던 저장이 실제로 확정된 경우 남은 handle 정리(`settle_unwatched_handles`)가 실행 중인 프로세스를 끝내지 않게 하기 위해서다.

2단계는 `Ready` 인지만 보고 그 task 가 "어떤 main 의 아직 만들어지지 않은 fallback 참조 대상" 인지는 알 방법이 없다 — `fallback{task}` 는 참조 대상(fallback)이 참조자(main)보다 먼저 존재해야 하는 2단계 `task_create` 흐름이라, 그 사이에 tick 이 끼면 아직 아무도 참조하지 않는 fallback 후보가 정상적으로 `Ready` 로 dispatch 돼버릴 수 있다(TOCTOU). `task_create(..., reserved_for_fallback: true)`(`TaskStore::create_reserved_for_fallback`)로 만든 task 는 `TaskGraph::dormant_as_pending_fallback` 이 이 필드를 최우선으로 존중해 `Ready` 로 노출하지 않으므로, 애초에 이 2단계 dispatch 루프의 대상이 되지 않는다 — 상세는 [features/agent-collaboration §Task DAG](../features/agent-collaboration/index.md).

### DispatchHandle

`PolledDispatch { workspace_id, poll_method, poll_params, state_field, terminal_states, failure_states, interval_ms, deadline_ms }`(범용 폴링 — dispatch 시점에 완성된 `poll_params` 로 terminal 상태 도달까지 `poll_method` 반복 호출; `failure_states` 적중은 성공이 아니라 실패로 종결) · `ShellProcess { pid }`(`Run` task 자식; `Child` 객체는 Clone 불가라 executor 의 `shell_children` map 에 별도 보관) · `BarrierPoll { workspace_id, name }` · `ReduceImmediate`/`CustomImmediate`/`ImmediateFail`(dispatch 시점 즉시 결정) · `AwaitDispatchAnswer { workspace_id, task_id, ipc_method }`(응답을 기다리는 실행 요청 — 시작한 Custom 요청이 응답 대기 상한을 넘겨 늦은 응답을 기다리는 중, agent task 새 세션의 `<provider>.spawn` 응답을 기다리는 동안 러너가 드는 handle, agent task 의 `<provider>.tell` 이 늦은 응답을 기다리는 동안의 저장 handle(러너는 `AgentTurn` 을 든다). 아래 "호스트 응답 지연") · `AwaitExternal { wait_key, deadline_ms }`(push-kind 완료 전략, 아래 참조 — `poll` 은 계약대로 **항상 Active**, 종결은 외부에서 store 를 직접 전이시킨다). `deadline_ms` 는 dispatch 시점 `now + timeout_ms` — handle 자체에 실려 영속되므로, 이 handle 을 만든 `hook_task_waits` 매핑(비영속)이 재시작으로 사라져도 재시작 후 reload 가 독자적으로 만료 판정을 할 수 있다(아래 "호스트 재시작 후 정리 + 핸들 영속" 참조). 이 필드 도입 이전에 영속된 구 포맷은 `#[serde(default)]` 로 `0`(=즉시 만료)이 된다.

### 조회 간격과 기한

`Custom.poll.interval_ms`(생략 시 500ms)는 task별 상태 조회가 끝난 뒤 다음 조회까지의 최소 대기다.
최초 조회는 dispatch 뒤 첫 runner tick에서 즉시 한다. 느린 요청이 끝나거나 tick을 놓쳐도
한 번만 조회하며 밀린 횟수를 따라잡지 않는다. 0은 매 tick 한 번의 조회를 허용한다.
러너는 500ms 대기와 깨움 신호로 진행하므로 짧은 간격의 정확한 실행 시각을 보장하지 않는다.
같은 method·인자를 가진 task도 스케줄·응답·결과를 각각 가진다.

`RunnerLoop`는 task ID와 handle을 `TaskExecutor::poll_task`에 넘기고, `HostExecutor`가
`HoldingClock`으로 다음 시각을 검사한다. 기본 executor 구현은 기존 `poll`에 위임한다.
즉시 dispatch 응답과 늦게 도착한 응답 뒤의 `PolledDispatch` 모두 같은 경로를 쓴다.
조회 시각은 executor 메모리에만 두고, 취소·종결의 자원 해제 때 버린다. 재시작·runner 재개는
저장된 handle의 간격·기한·인자를 그대로 읽어 최초 조회를 즉시 하고 이후 간격을 지킨다.
원 실행 명령은 다시 보내지 않으며 이전 task의 성공 응답도 재사용하지 않는다.

다음 조회가 아직 이르더라도 `deadline_ms` 이상인 tick은 endpoint 호출 없이 timeout으로 끝낸다.
조회가 due이면 기존 응답 우선 계약을 따른다: 실패 상태가 성공 상태보다 우선하고, 성공 응답은
기한을 지난 뒤 받아도 성공이다. 비종결 응답과 호스트 응답 지연은 기한을 검사한다. 진행 중인
동기 IPC 대기를 task 기한으로 끊지는 않는다. 호스트 지연을 다시 조회할 때도 완료 뒤 간격을
지킨다. `stale`·`needs_input` 등은 지정한 성공/실패 목록에 없으면 계속 Active다.

### HostExecutor 매핑

| TaskCommand | dispatch | poll |
|-------------|----------|------|
| `Run { command, cwd }` | `Command::spawn`(stdout/stderr `Stdio::piped()`) → pid → `ShellProcess`. 빈 command Err. stdout/stderr 를 각각 별도 드레인 스레드로 즉시 읽기 시작(파이프 교착 방지 — 아래 "출력 캡처" 참조) | watcher thread 의 `child.wait()` 결과 cell 조회 → exit 0 이면 두 드레인 스레드를 join 해 캡처 결과를 실은 Done / 아니면 Failed(캡처 결과를 에러 메시지에 포함) |
| `Custom { ipc_method, params, poll: None }` | host IPC dispatch(timeout 5s) → 등록된 완료 판정 전략 중 `ipc_method` 를 `default_for_methods` 로 지목한 전략이 있으면 그 kind 를 채택(아래 "완료 판정 전략 레지스트리" 기본 전략 선택 — poll 이면 `PolledDispatch`, push 면 아래 push 행과 동일), 없으면 `CustomImmediate` | 매칭된 kind 에 따라 poll/push 행과 동일 / 아니면 즉시 Done |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Inline(spec)) }` | host IPC dispatch → `map_from_request`/`map_from_response` 로 `poll_params` 완성 → `PolledDispatch` | `poll_method` 호출 → `state_field` 가 `failure_states` 중 하나면 **Failed**(상태값 + 응답 요약을 에러 메시지에) / `terminal_states` 중 하나면 Done(응답 전체가 산출물) / 아니면 Active(`deadline_ms` 초과 시 Failed) |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Named{strategy}) }`, poll-kind | 완료 판정 전략 레지스트리에서 `strategy` 를 이름 해석(`resolve_strategy`) → 얻은 `PollSpec` 으로 위 Inline 행과 동일 처리. 미등록/비활성이면 해석 실패 → dispatch 자체가 `PermanentFail`(Running 진입 전에 드러남) | 위와 동일 |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Named{strategy}) }`, push-kind | `dispatch_push_strategy` — 원 dispatch `params.surface_id` 대상 surface 에 `notify_via` 훅 핸들러를 `hook.set(..., once: true)` 로 1 회성 등록해 `hook_id` 획득 → `RunnerContext.hook_task_waits` 에 `(workspace_id, task_id, deadline)` 등록 → `AwaitExternal`. `surface_id` param 이 없으면 `PermanentFail` | 항상 Active(계약) — 종결은 `PendingHostEvent::HookFired` 소비부(`TaskService::resolve_hook_task_wait`, exit code 로 성공/실패 분기)와 timeout 안전망(`runner_thread::expire_overdue_hook_waits`)이 담당 |
| `Reduce { inputs, strategy }` | input 결과 collect → `reduce_with_custom` → `ReduceImmediate`. `inputs` 는 Task DAG 의 암묵적 의존성(`TaskGraph`, `crates/tasty-agent/src/task/graph.rs`)이라 dispatch 시점엔 이미 전부 종결(terminal) 상태다 — `Ready` 로 올라오기 전에 readiness 평가가 그 종결을 강제한다 | 즉시 Done |
| `WaitBarrier { name }` | `BarrierPoll`. 이름은 `name`, 없으면 입력 snapshot 의 `execution.barrier`(아래 §입력 binding) | `Open`→Active / `Closed`→Done / `TimedOut`→Failed |
| `Agent { provider, session, instruction }` | 새 세션: `<provider>.spawn` 을 따로 띄운 스레드가 보내고 기다린다 → `AwaitDispatchAnswer`(`ipc_method` = `<provider>.spawn`). 응답이 오면 그 스레드가 턴 표에 묶고 poll 이 `AgentTurn` 으로 이어 간다. 기존 세션: 지시를 handle 의 `pending_instruction` 에 담은 `AgentTurn` | 지시를 아직 안 보냈으면 세션이 idle 이고 묶을 수 있을 때 `<provider>.tell`. 그 뒤 턴 표의 종료 보고·`<provider>.state` 로 판정(아래 §agent task) |

### 호스트 응답 지연

러너의 host IPC 호출은 5초(`HOST_DISPATCH_TIMEOUT`)까지 응답을 기다린다. 메인 루프가 바빠 그 안에 답이 없을 때 task 를 실패로 확정하지 않도록, 실패를 `InjectError` 갈래로 나눠 다룬다.

| 단계 | 큐 대기 중 만료(`Expired`, 미실행) | 시작 후 상한 경과(`Timeout`, 결과 불명) | 그 밖(`Refused`·RPC 오류·`Send`·`Disconnected`) |
|---|---|---|---|
| `PolledDispatch` 의 poll(`claude.state` 등, 다시 물어도 되는 조회) | Active — 다음 poll 에서 다시 묻는다 | Active — 다음 poll 에서 다시 묻는다 | Failed |
| Custom 실행(`claude.spawn` 등, 다시 보내면 두 번 실행될 수 있는 요청) | `DispatchOutcome::Deferred` — 점유를 반환하고 Ready 로 둔 채 다음 tick 에 다시 보낸다 | `AwaitDispatchAnswer` — 다시 보내지 않는다 | `PermanentFail` |
| agent task 의 `<provider>.tell`(기존 세션에 지시, 다시 보내면 지시가 두 번 간다) | 턴 묶음을 풀고 Active — 다음 poll 에서 세션이 idle 이면 다시 보낸다(`timeout_ms` 가 지났으면 `timed_out`) | 늦은 응답 대기 — 다시 보내지 않는다 | Failed `agent_unavailable` |

- poll 의 재시도에 따로 횟수 상한은 없다. poll 기한(`PollSpec.timeout_ms` → `deadline_ms`)이 있으면 그 기한이 지난 뒤의 지연은 `<poll_method>: poll timeout (last poll: <오류>)` 로 실패한다. 기한이 없으면 응답 상태가 비종결일 때와 같이 계속 기다린다.
- `AwaitDispatchAnswer` 는 `HostIpcInjector::dispatch_keeping_late_answer` 가 넘긴 응답 통로(`LateAnswer`)를 executor 가 task 별로 보관하고 poll 마다 확인한다. 응답이 오면 즉시 응답을 받은 경우와 같은 경로(`custom_after_answer`)로 완료 전략 handle(`PolledDispatch`·`AwaitExternal`, 전략이 없으면 Done)을 만들고, 저장 handle 을 그것으로 바꿔 재시작 뒤에도 그 handle 로 복원한다. poll 기한은 요청을 처음 보낸 시각부터 잰다. 응답이 RPC 오류면 Failed 다. 늦은 응답을 기다리는 동안에는 따로 기한을 검사하지 않는다(전략은 응답을 받은 뒤에 해석한다). 응답이 오거나 통로가 닫히거나 task 를 `cancel` 할 때까지 Running 이다.
- 통로가 답 없이 닫혔거나, 재시작 등으로 이 executor 가 통로를 갖고 있지 않으면 결과를 알 수 없다. `PollOutcome::Lost` → task `unknown`(`reason` 머리말 `dispatch outcome unknown:`)이다. 요청이 반영됐는지 확인한 뒤 `retry`·`cancel` 로 정한다(아래 "한계"의 `unknown` 과 같다).
- agent task 의 `<provider>.tell` 이 상한을 넘기면 executor 가 그 응답 통로를 task 별로 보관하고, 턴 묶음은 남겨 그 사이 시작한 이 회차의 턴을 받는다. 러너는 `AgentTurn` 을 그대로 들고 있고 저장 handle 만 `AwaitDispatchAnswer`(`ipc_method` = `<provider>.tell`)로 바꾼다. 그래서 응답을 기다리는 동안 재시작하면 위와 같이 `unknown` 이다. 응답이 성공이면 저장 handle 을 `AgentTurn` 으로 되돌리고 턴 판단을 잇는다. RPC 오류면 지시가 전달되지 않았으므로 묶음을 풀고 `agent_unavailable` 로 끝난다. 통로가 답 없이 닫히면 `unknown`(`dispatch outcome unknown: '<provider>.tell' …`)이다. 기다리는 동안에도 `timeout_ms` 가 지나면 `timed_out` 이다.
- agent task 의 `<provider>.spawn` 은 이 표가 아니라 아래 §agent task 의 별도 대기(30초 + 정리 대기)를 쓴다. 대기는 러너 스레드가 아니라 spawn 마다 띄운 스레드가 하므로, 그동안 같은 러너의 다른 task 의 dispatch·poll 이 진행된다. 러너가 드는 handle 은 `AwaitDispatchAnswer` 이고 executor 가 응답 자리를 task 별로 보관한다. 응답이 오면 위 Custom 처럼 `AgentTurn` 으로 이어 가고 저장 handle 도 바꾼다. 응답 전에 재시작하면 위와 같이 `unknown` 이다.

> 자식 에이전트 완료 판정(`claude.spawn`/`codex.spawn` · `claude.tell`/`codex.tell`)과 셸 명령 완료(`host/command-completed`)가 이 범용 `Custom { poll }` 메커니즘의 실사용자다 — 코어는 특정 에이전트를 모른 채 임의 IPC dispatch→폴링/훅-보고를 표현한다. CLI auto_wait(`AutoWaitDecl`/`PollingDecl`)와 동형 스펙으로 폴링 semantics 를 통일한다.
>
> 훅 보고(push) 예 — 전략을 이름으로 지목한다:
> ```json
> {"kind":"custom","ipc_method":"surface.send","params":{"surface_id":7,"text":"npm test"},
>  "poll":{"strategy":"host/command-completed"}}
> ```
>
> 자식 에이전트 예 — **`poll` 을 아예 생략한다.** claude plugin 매니페스트가 `claude.spawn` 을 `default_for_methods` 로 지목한 전략을 이미 선언해 두었으므로, dispatch 시 `resolve_default_for_method` 가 그것을 집어 `PolledDispatch` 로 전이한다(위 표의 `poll: None` 행). 자식이 terminal 상태에 도달할 때까지 러너가 대신 기다린다:
> ```json
> {"kind":"custom","ipc_method":"claude.spawn",
>  "params":{"surface_id":560,"workspace":"wt-11","cwd":"/home/me/work/wt-11",
>            "prompt":"이 디렉토리의 테스트를 고쳐라"}}
> ```
> `params.surface_id` 는 **부모** surface(자식을 매달 대상)이고, spawn 응답의 `child_surface_id` 가 `map_from_response` 로 poll 호출의 파라미터가 된다. `codex.spawn` 도 같은 모양이며, 두 plugin 의 전략 값 차이(poll 파라미터 키 이름 · 상태 목록)는 [features/agent-collaboration §자식 에이전트를 DAG 노드로](../features/agent-collaboration/index.md#자식-에이전트를-dag-노드로) 참조. 해당 plugin 이 비활성이면 전략이 레지스트리에 없으므로 `poll` 생략은 `CustomImmediate`(= dispatch 성공 즉시 Succeeded)로 떨어진다.
>
> 이어서 지시를 주는 노드도 같은 모양이다 — `tell` 에도 기본 전략(`tell-wait`)이 있어 자식이 그 지시를 마칠 때까지 러너가 기다린다. Claude 자식이 백그라운드 작업을 남기고 응답을 끝낸 경우는 마친 것으로 보지 않는다([features/agent-collaboration](../features/agent-collaboration/index.md#이어서-지시-주기-tell)). 여기서는 대상이 **자식** surface 다(spawn 노드의 `surface_id` 는 부모였다):
> ```json
> {"kind":"custom","ipc_method":"claude.tell",
>  "params":{"surface_id":561,"message":"방금 고친 테스트를 다시 돌려라"}}
> ```
>
> **현재 되는 것.** 자식을 띄우고(spawn) 이어서 지시를 주고(tell) 각각의 완료를 기다리는 것까지 위 그대로 동작하고, 자식이 죽으면 그 노드는 실패한다. spawn→tell 을 한 DAG 로 이을 때 tell 노드의 `surface_id` 는 아래 `${task.<id>.output<pointer>}` 로 넘긴다.

### `Run` 출력 캡처

`Run` 은 Surface(Tab)를 만들지 않는 bare subprocess다 — argv 를 그대로 `Command::spawn` 에 넘기고(셸 word-splitting 없음), exit code 도 명령 자신의 것이다. tty 가 필요한 명령은 지원 대상이 아니다(그건 `pty.*` primitive 의 몫). 환경은 아래 "runner 자식의 환경" 규칙을 따른다.

- **드레인 스레드 필수**: `Stdio::piped()` 만 붙이고 파이프를 읽지 않은 채 `child.wait()` 하면 자식이 OS 파이프 버퍼(플랫폼별 16~64KB)를 채우고 block, 부모는 그 자식의 종료를 기다리므로 교착한다. dispatch 가 stdout/stderr 를 각각 별도 스레드(`agent-shell-{stdout,stderr}-pid<N>`)로 즉시 드레인 시작하고, watcher(`agent-shell-watcher-pid<N>`)는 `child.wait()` 후 두 드레인 스레드를 join 한다 — task 당 스레드 3개(watcher + drain 2개).
- **스트림당 64KiB tail**: 각 드레인 스레드는 EOF 까지 계속 읽되 마지막 64KiB 만 보관한다(head 는 버림). 상한을 넘기면 `truncated: true` + `dropped_bytes` 를 함께 기록. 64KiB × 2스트림인 이유는 캡처 결과가 `run_result` 로 memory store 값 하나(1MiB 상한)에 JSON 직렬화되기 때문 — ANSI escape 팽창까지 고려한 최악의 경우도 1MiB 아래.
- **성공/실패 모두에 담김**: 성공(exit 0)은 `TaskResult.output = {"pid", "stdout": {"text","truncated","dropped_bytes"}, "stderr": {...}}`. 실패(비0 exit)는 `PollOutcome::Failed` 가 문자열 하나만 나르는 계약이라, 캡처한 stdout/stderr tail 을 에러 메시지 본문에 그대로 이어붙인다 — `cargo build` 같은 명령의 컴파일 에러 본문도 이 경로로 드러난다.
- **알려진 한계**: tail 전용이라 긴 빌드의 *첫* 에러는 놓칠 수 있다(요약/실패 지점은 보통 출력 뒤쪽). ANSI 는 벗기지 않고 보존한다.

### runner 자식의 환경

`Run` task, 후처리 CLI, reduce 의 custom 전략 셸(`run_custom_shell` — runner 의 reduce task 와 `agent.task_reduce` 모두)의 자식은 같은 규칙(`crates/tasty-agent/src/child_env.rs`)으로 Tasty 프로세스의 환경을 받는다. 규칙이 한 곳에 있어 세 경로가 같이 바뀐다.

- 지우는 것 1 — 바깥 제품 세션의 표지·비밀. 남기면 자식이 부르는 에이전트가 자신을 바깥 세션의 자식으로 오인하고 세션 비밀이 무관한 프로세스로 샌다. 터미널 셸에서도 지우는 판정(`tasty_utils::process::is_stripped_inherited_env`)이다: 본체가 정하는 `CMUX_*`, 설치·활성 플러그인이 매니페스트 `contributes.strip_inherited_env` 로 선언한 변수(번들 Claude 플러그인의 `CLAUDECODE`·`CLAUDE_CODE_SESSION_ID`·`CLAUDE_PLUGIN_OPTION_*`·Claude 값의 `AI_AGENT` 등, 번들 Codex 플러그인의 `CODEX_THREAD_ID`). 플러그인을 비활성화·제거하면 그 선언은 적용되지 않는다([플러그인 제작 가이드](plugin-development.md#상속-환경변수-제거-contributesstrip_inherited_env)).
- 지우는 것 2 — 바깥 Tasty 인스턴스의 신원: `TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`·`TASTY_AGENT_ID`. 이 Tasty 를 다른 Tasty 의 터미널에서 띄웠으면 이 값들은 그 인스턴스의 surface·세션 토큰·완료 알림 경로를 가리킨다. 터미널 셸은 `TASTY_SURFACE_ID`·`TASTY_PARENT_HOME` 을 자기 값으로 덮어쓰지만 runner 의 자식에는 덮어쓸 surface 가 없어 지운다. 그래서 자식이 부른 `tasty` 는 다른 인스턴스의 신원으로 요청하지 않는다. 목록은 터미널 셸·훅과 같은 `tasty_utils::process::OUTER_IDENTITY_ENV` 이고, 판정은 훅 실행과 같은 `is_child_stripped_env` 다.
- 그대로 넘기는 것: 그 밖의 `TASTY_*`(예: `TASTY_HOME`, 부팅 때 정한 `TASTY_LOCALE`)와 `CLAUDE_CODE_OAUTH_TOKEN`·`ANTHROPIC_API_KEY` 같은 사용자 설정·인증. `TASTY_HOME` 이 남으므로 자식이 부른 `tasty` 는 이 인스턴스에 닿는다.
- Tasty 가 더하는 task 별 변수는 v2 task 의 report 주소 `TASTY_TASK_REPORT` 하나다(아래 §DAG report). v1 task 와 `agent.task_reduce` 의 custom 셸에는 넣지 않는다. task 에는 자기 surface 가 없다. 터미널 셸과 달리 `TASTY_PARENT_HOME` 도 이 인스턴스의 값으로 넣지 않는다: 작업의 자식은 터미널 surface 가 아니고, 자식이 띄운 에이전트의 완료 알림은 그 알림을 기다리는 호출자의 것이다.

### `Run` 결과를 reduce 하기 — `--extract-path`

`agent.task_reduce`(CLI `tasty agent task-reduce`)의 in-process 전략(`concat_text`/`merge_json`/`all`/`first_success`)은 각 input 의 `output` 을 구조 무관하게 통째로 다룬다 — `Run` task 의 `output` 이 위 "출력 캡처" 의 `{pid, stdout:{text,...}, stderr:{...}}` 구조라는 걸 모른 채 `concat_text` 로 이어 붙이면 직렬화된 JSON 조각이 그대로 이어붙어 유효한 JSON 도, 사람이 읽을 텍스트도 아닌 결과가 나온다. `merge_json` 도 같은 구조를 가진 두 `Run` 결과를 merge 하면 `pid`/`stdout`/`stderr` 키가 전부 겹쳐 뒤 input 이 앞 input 을 통째로 덮어쓴다.

`--extract-path <JSON Pointer>`(예: `/stdout/text`)를 주면, reducer 전략을 적용하기 전에 각 input 의 `output` 에서 그 경로(RFC 6901 JSON Pointer)만 뽑아낸다 — `crates/tasty-agent/src/reducer.rs::extract_paths`. 예:

```sh
tasty agent task-reduce --workspace 1 --inputs t-a,t-b \
  --strategy concat_text --extract-path /stdout/text
# → {"value": "out1\nout2\n", "warnings": []}
```

- 생략 시 기존 동작(전체 `output` 그대로) 유지 — 하위 호환.
- 지정된 경로가 없는 input(예: `Run` 이 아닌 다른 kind 의 결과)은 reduce 전체를 실패시키지 않는다 — 그 input 만 `output: null` 로 취급하고 나머지는 정상 진행하되, 응답의 `warnings` 배열에 `input #<i>(task <id>)에 경로 '<path>'가 없어 null로 처리했습니다` 를 남긴다.
- DAG 안의 `TaskCommand::Reduce`(위 `HostExecutor 매핑` 표) 는 이 옵션이 없다 — `--extract-path` 는 독립 호출 `agent.task_reduce`/`task-reduce` CLI 전용이다.

### 선행 task 출력을 파라미터로 넘기기 — `${task.<id>.output<pointer>}`

`depends_on` 은 실행 **순서**만 묶는다. 선행 task 의 결과를 후행 task 의 **입력**으로
넘기려면 placeholder 를 쓴다 — dispatch 직전에 upstream 의 `result.output` 에서 값을
뽑아 `task.command` 에 주입한다(`crates/tasty-task-runtime/src/task_output_ref.rs` 가 문법 소유자,
치환은 `runner_host.rs`).

```text
${task.<task_id>.output<JSON Pointer>}
```

포인터는 위 `--extract-path` 와 **같은 RFC 6901**(`serde_json::Value::pointer`)이라
경로 문법을 두 개 외울 필요가 없다. 포인터를 생략하면(`${task.t-a.output}`) 출력 전체다.

`claude.spawn` 의 child surface id 처럼 **런타임에야 정해지는 값**이 주 용도다 —
`task_create` 시점엔 알 수 없어 정적 params 로는 표현할 수 없다.

```sh
# A: 자식 spawn → output {"child_surface_id": 3, ...}
tasty agent task-create --workspace 1 --name spawn-child \
  --command '{"kind":"custom","ipc_method":"claude.spawn",
              "params":{"surface_id":1,"workspace":"ws","cwd":"/tmp","prompt":"..."}}'

# B: 그 자식에게 tell — A 의 응답을 참조. depends_on 은 필수다(아래).
tasty agent task-create --workspace 1 --name tell-child --depends-on "$T_A" \
  --command '{"kind":"custom","ipc_method":"claude.tell",
              "params":{"surface_id":"${task.'"$T_A"'.output/child_surface_id}",
                        "message":"..."}}'
```

**타입이 보존된다.** 문자열 값이 *정확히* placeholder 하나뿐이면 뽑아낸 JSON 값으로
통째 교체된다 — 숫자는 숫자로 남는다. `"3"` 처럼 문자열로 떨어지면 `claude.spawn` 의
`require_surface_id`(`as_u64`) 같은 타입 검사가 거부한다. 다른 텍스트에 섞여 있으면
그때만 문자열 보간이다(`"review PR ${task.t-a.output/pr_number}"` → `"review PR 42"`).
객체/배열/불리언도 통째 교체 대상이다.

적용 자리는 `Custom.params`(JSON 트리 전체) 와 `Run.command` 인자 / `Run.cwd` 다.

- **참조하려면 `depends_on` 에 넣어야 한다.** 없으면 `task_create` 가 `invalid_params`
  로 거부한다 — 안 그러면 upstream 이 끝나기 전에 dispatch 돼 값이 없는 채로 실패하는
  race 가 된다. 인정 출처는 `depends_on` ∪ `Reduce.inputs`. 자동으로 엣지를 추가하는
  대안은 택하지 않았다(선언하지 않은 의존성이 `task_graph` 렌더와 DAG 그룹핑에 조용히
  나타난다).
- **해석 실패는 dispatch 실패**(`PermanentFail`)다 — 참조 task 없음 / `result` 없음 /
  포인터 미스 전부. 조용한 `null` 은 downstream 에서 원인과 먼 자리에서 터진다.
- **문법이 깨진 참조도 거부**한다(`${task.` 로 시작하는데 형식 불일치). 리터럴로
  흘리면 프롬프트에 `${task.t-x.ouput/id}` 가 그대로 박혀 나간다. 생성 시점 검증과
  dispatch 치환이 같은 파서를 쓰므로 둘의 판정이 갈릴 수 없다.
- **치환된 command 는 store 에 되쓴다** — `task-get`/`task-list` 가 원본 placeholder
  대신 실제로 호출된 값을 보여준다(lease 치환과 같은 이유).
- **`${lease.resource}` 와 함께 쓸 수 있다.** 치환은 lease 먼저, 출력 나중이고 각각
  1-pass 다 — 주입된 값 안의 placeholder 처럼 보이는 텍스트는 다시 해석되지 않는다.
  순서가 이렇게 고정된 이유는 반대면 upstream 이 만든 문자열 안의 `${lease.resource}`
  가 lease 치환의 입력이 되어, 자식 에이전트가 만든 데이터가 lease 의미론에 끼어들기
  때문이다.

## 완료 판정 전략 레지스트리 (`src/completion_strategy/`)

이 레지스트리는 `Custom` 작업의 IPC 완료 조건을 관리한다. `Custom.poll`의 이름 참조
(`PollSpecRef::Named`)와 `default_for_methods`의 기본 전략을 여기서 찾는다.
호스트 내장 TOML, 플러그인 매니페스트, 사용자 설정 `~/.tasty/completion-strategies.toml`이
전략을 등록하며 레지스트리는 실제 등록 순서대로 `Some` 필드를 덮어쓴다.
출처 순서를 레지스트리 자체가 강제하지 않으며 owner도 마지막 기여자로 바뀐다.

ID는 `<owner>/<short>`다. 호스트와 사용자는 `host`·`user`를 사용한다.
플러그인은 `completion_strategy_owner_id`가 고른 첫 IPC namespace를 우선 사용하고,
namespace가 없으면 매니페스트 ID를 쓴다. 전역 인스턴스는 `global()`로 얻는다.

설정 병합과 ID 규칙은 공유 훅 핸들러 레지스트리(`src/hook_handler/`)와 같다.
다만 레지스트리는 독립적이며, push 전략의 `HookHandlerId` 외에는 훅 핸들러 타입을
가져오지 않는다.

| 전략 종류 | 완료 조건 | 조회 함수 |
|---|---|---|
| poll | `tasty-agent::PollSpec`에 따라 상태를 조회한다. | `resolve_poll_spec(id)`는 poll만 반환하고 push에는 `NotPollKind` 오류를 낸다. |
| push | `notify_via: HookHandlerId`로 지정한 훅 신호를 기다린다. 필수 `timeout_ms`로 신호 유실에 대비한다. | `resolve_strategy(id)`는 poll과 push 모두 반환한다. `Custom` dispatch가 이 함수를 쓴다. |

push 전략은 등록할 때 `notify_via` 훅의 존재와 소유자를 검사한다. 자기 소유 또는
`host` 소유 훅만 쓸 수 있다. 플러그인 전략의 `poll_method`와 `default_for_methods`도
자기 소유자로 등록한 IPC namespace 안에 있어야 한다. host/user 전략은 플러그인
namespace를 가리킬 수 없다. `_host` 권한 우회를 막기 위한 제한이며,
`tasty_ipc::method_meta::is_registered_plugin_prefix`로 확인한다.

**기본 전략 선택**: 전략의 `default_for_methods`에 자신을 기본값으로 쓸 IPC 메서드를
적는다. 같은 메서드를 여러 활성 전략이 지정하면 priority가 작은 순, 소유자
user → plugin → host 순, ID 순으로 고른다. 선택되지 않은 전략은 warn 로그에 남긴다.

**push 완료 신호 처리**: dispatch는 원래 `params.surface_id`의 surface에
`hook.set(event: "command-completed", once: true)`를 호출해 훅을 한 번 등록한다.
받은 `hook_id`에 `(workspace_id, task_id, deadline)`을 연결해
`RunnerContext.hook_task_waits`에 넣고 작업을 `AwaitExternal`로 전환한다.
이 저장소는 `Arc<HookTaskWaits>`로 runner thread와 직접 공유하므로 `Core`를 거치지
않고 접근할 수 있다. `RunnerContext.task_waker_hub`도 같은 공유 방식을 쓴다. 이 허브는 engine의 `TaskScope`(`crates/tasty-task-runtime/src/service.rs`)가 만든 것이다.

- 훅이 발생하면 `resolve_hook_fired_task_waits` → `TaskService::resolve_hook_task_wait`가
  `hook_id`로 작업을 찾는다. `CommandCompleted`의 종료 코드가 0이거나 없으면
  Succeeded, 0이 아닌 값이면 그 코드를 담은 오류와 함께 Failed로 끝난다.
  종료 코드가 없는 push 신호도 Succeeded로 처리한다.
- 각 runner thread는 매 tick `expire_overdue_hook_waits`에서
  `HookTaskWaits::take_expired`를 호출한다. 전역 sweep은 워크스페이스 구분 없이 기한이 지난
  항목을 Failed로 끝내되, 등록 당시의 TaskWakerHub와 agent_seq를 사용한다. 다른 workspace의
  runner가 만료를 처리해도 원 engine 허브로 종결 사건을 보내고 원 순번으로 inline fallback ID를 발급한다.
  tick·만료·startup cleanup/reload의 성공한 상태 전이가 반환한 task와 downstream 중 종결 항목은
  저장소 락을 놓은 뒤 통지한다.

현재 push 전략은 `host/command-completed` 하나다. 훅의 action인 `notification.create`는
알림을 추가하는 동작이며 작업의 완료 여부를 정하지 않는다. 작업 연결은 `hook_id`로 찾는다.

**poll의 성공·실패 판정**: `failure_states`에 도달하면 작업을 Failed로 끝내고 노드의
`OnFailure` 설정(abort / continue_downstream / fallback)을 적용한다.
`terminal_states`와 `failure_states`에 같은 값이 있으면 실패를 우선한다.
`failure_states`를 생략하면 빈 목록이므로 `terminal_states`의 상태를 모두 성공으로 본다.

번들 Claude/Codex 전략은 자식 프로세스가 끝나거나 surface가 사라진 상태인 `exited`를
실패로 본다. Claude의 `needs_input`은 성공으로 남긴다. 사용자 승인을 받아 계속할 수
있는 상태이며, spawn/tell 완료 알림도 이 상태를 `idle`처럼 처리한다.

이 성공·실패 구분은 **DAG 러너 전용**이다. CLI auto_wait로 변환하는
`CompletionStrategyDecl::to_polling_decl`은 `failure_states`를 넘기지 않는다.
CLI 폴링은 `terminal_states`에 도달하면 대기를 끝내며 자식의 상태로 CLI 종료 코드를
정하지 않는다. 따라서 자식이 끝났다는 이유만으로 `tasty claude spawn`을 0이 아닌
종료 코드로 끝내도록 바꾸지 않는다.

IPC/CLI: `completion_strategy.list`(전 범위 조회, 비활성 포함) / `tasty completion-strategy list`. reload/dispatch 대응물은 없다(user config 재로드 미노출, 수동 실행 기능 없음). 내장 host 기본값은 `src/completion_strategy/defaults/default-completion-strategies.toml`:

- `host/command-completed` — OSC 133 셸 통합 기반 push 전략. `notify_via = "host/command-completed"` 는 `src/hook_handler/defaults/default-hook-handlers.toml` 에 등록된 훅 핸들러를 가리킨다(둘 다 host defaults 로 함께 설치되므로 항상 존재). `timeout_ms = 300000`(5분). 전제: 대상 surface 가 OSC 133(셸 통합 스크립트)을 로드하고 있어야 발화한다 — 미로드 surface 를 대상으로 `hook.set --event command-completed`(이 전략의 내부 dispatch 경로 포함)를 걸면 거부는 아니고 warn 로그만 남는다(`shell_integration_boundary_seen`, 시간 기반 추정이라 오탐 가능). `Custom` task 의 dispatch `params` 는 `surface_id` 를 포함해야 한다(예: `surface.send`).

## 호출 경계

`RunnerContext` 는 `TaskService` 안에서만 다룬다. `RunnerRegistry` 는 `TaskService` 가 소유하고 engine 생성 때 `TaskScope` 에 같은 `Arc` 를 넘기는 것 외에는 서비스 밖으로 나가지 않는다. 바깥 코드는 서비스(`AppServices.tasks`)와 engine 의 `TaskScope` 를 통해 다음 API 만 부른다.

- IPC 핸들러(`src/adapters/ipc/handler/agent/task.rs`): 구성 표의 작업 API, runner 제어 `runner_start`/`request_stop_workspace`/`runner_status`, `AwaitExternal` 대기 정보를 읽는 `dispatch_handle`.
- `agent.task_await` dispatch(GUI `src/app/ipc/app_methods/task_await.rs`, headless `src/boot/headless_dispatch.rs`): 소유 engine 범위의 `awaiter` 를 받아 워커로 넘긴다.
- 부팅(`src/boot.rs`, `src/app/boot_machine.rs`): `purge_stale_agent_state_on_boot`.
- engine 생성(`src/boot.rs`, `src/app/window_lifecycle.rs`): `TaskScope` 생성자는 registry `Arc` 를 필수 인자로 받는다. 첫 창·추가 창·headless engine 모두 `TaskService::runner_registry` 의 같은 `Arc` 를 넘긴다.
- workspace 삭제와 Engine 해제: `TaskScope::request_stop_workspace` 또는 `TaskService::request_stop_scope`로 원 범위의 runner에 중지만 요청한다. `RunnerStopReceipt`와 `poll_runner_stops`는 끝난 worker만 join하며 UI에서 blocking stop을 호출하지 않는다.
- 호스트 이벤트 소비(`src/app/dispatch/host_events.rs`, `src/intent/headless.rs`): `resolve_hook_task_wait`.
- DAG 화면: 서비스를 받지 않으므로 runner 상태는 `TaskScope::runner_liveness`, 목록은 `dag-read` worker 가 writer 요청 하나 안에서 `task_list_in`·`dag_list_in` 으로 읽는다.

## RunnerRegistry

`TaskService` 의 비공개 필드다. 시작·정지·조회는 위 `runner_*` API 로만 하고, 화면은 `TaskScope::runner_liveness` 로 실행·crash 여부만 읽는다. workspace 1개당 thread 1개:

- `start(ctx, ws) -> bool` — 이미 실행 중이면 false(idempotent). crashed 면 정리 후 재시작 허용.
- 명시 IPC stop(`agent.task_run` stop)도 `request_stop_workspace(scope, ws)`로 원 범위의 runner에 정지만 요청하고 바로 응답한다. 메인 스레드에서 join하지 않는다. runner tick의 Custom dispatch는 메인 스레드의 host 응답을 기다리므로, 메인이 join하면 둘이 서로를 기다리고 dispatch마다 5초 상한까지 메인 루프가 멈춘다. 정지 요청 뒤 runner는 아직 보내지 않은 Ready dispatch를 보내지 않는다. 이미 진행 중인 dispatch, Running task의 poll, 후처리 정리 대기가 끝날 때까지는 worker가 살아 있을 수 있어 종료 시간의 상한을 약속하지 않는다. worker가 끝나기 전까지 응답과 `status`의 `state`는 `stopping`이며, 이 동안 `start`는 새 runner를 띄우지 않는다. 기준은 회수가 아니라 worker 종료다. 핸들러는 receipt를 비차단으로 한 번 확인해(끝난 worker만 join) 이미 끝난 worker(crash 포함)를 그 자리에서 회수하고 `stopped`로 답한다. 나머지는 메인 루프의 `poll_runner_stops`(`App::runner_stop_poll_deadline`, headless 루프)가 회수한다. 정지 완료는 `status`로 확인한다. 동기 `stop`·`join_blocking`은 시험 전용(`cfg(test)`)이다.
- 수명 종료의 stop 요청은 원 hub와 실행 control을 고정한 receipt를 반환한다. `Waiting`은 미회수, `Joined`는 정상 join, `WorkerFailed`는 실패한 worker의 실제 join이다. 옛 receipt가 같은 숫자 workspace ID의 새 owner를 지우지 않는다.
- `status(ctx, ws)` — `running`/`crashed`/`stopping`/`ready_count`/`running_count`/`store_error`/`list_failures`. `stopping`은 정지를 요청했지만 worker가 아직 끝나지 않은 상태다. IPC 응답은 이를 `state`(`running`·`stopping`·`stopped`·`crashed`, crash가 가장 앞선다)로 함께 싣는다. 두 카운트는 `Option` 이다 — store 를 못 읽으면 `None`(응답에선 `null`)이고 `store_error` 가 이유를 싣는다. `list_failures` 는 러너 스레드의 연속 조회 실패 횟수(러너가 없으면 0).

TaskScope의 Drop은 task 취소가 아니다. runner stop도 task 상태를 바꾸지 않고 Run의 OS 자식을 끝내지 않는다. 예외로 진행 중인 후처리 실행은 그룹을 끝내고 `cancelled` 보고를 저장한다(아래 §후처리 CLI, 이 문서). workspace가 확정 구조에서 사라졌을 때만 해당 runner를 중지하며 같은 Engine 안의 순서 변경은 유지한다. Engine 해제는 scope의 모든 runner와 물리 자원 receipt를 기다린다. 앱 종료 대기 상한을 넘기면 남은 worker를 미회수로 기록하며 join 완료로 간주하지 않는다.

scope 전체 stop 뒤에는 같은 scope의 늦은 runner 시작을 거절한다. 이미 등록된 hook wait와 awaiter는 지우지 않으며, 늦은 hook 완료도 등록 당시의 `TaskWakerHub`와 `agent_seq`로 전달한다. 현재 선택된 다른 Engine의 허브로 재해소하지 않는다.

thread 본문은 `RunnerLoop::tick` + 500ms `recv_timeout`. tick 안 memory lock 은 *짧은 구간* 만(list → release → dispatch/poll(lock 밖) → re-lock for set_state) — 사용자 CLI 동시 호출과 락 경합 최소화.

정지 신호는 tick 앞과 tick 뒤의 `recv_timeout` 에서 받고, tick 안에서는 Ready task 를 하나 dispatch 하기 전마다 `TaskExecutor::stop_requested` 로 다시 확인한다. 참이면 남은 Ready task 를 보내지 않고 tick 을 끝낸다. Custom dispatch 하나는 메인 스레드의 응답을 최대 5초(`HOST_DISPATCH_TIMEOUT`) 기다리므로, tick 끝에서만 정지를 보면 정지가 Ready task 수 × 5초만큼 늦어진다. `HostExecutor` 는 `RunnerControl` 의 정지 표시와 scope 정지 표시를 읽는다. 이미 시작한 dispatch 하나는 그 상한 안에 끝난다.

tick 머리의 `TaskStore::list` 가 실패하면 **빈 목록으로 흡수하지 않고 로그를 남긴다.** `tick` 은 넘겨받은 task 슬라이스만 순회하므로(terminal 흡수 · Running poll · Ready dispatch 전부) 빈 목록으로 진행하는 것과 이번 tick 을 건너뛰는 것의 동작은 같다 — permit 회수도 snapshot 에서 terminal 로 바뀐 task 를 봤을 때만 일어난다. 그래서 흐름은 종전대로 빈 snapshot 으로 돌리고(다음 tick 에 회복되면 밀린 전이를 한꺼번에 흡수한다) 관측 가능성만 더한다: 첫 실패는 `warn`, 연속 6회(약 3초)째부터는 `error` 로 올리고 그 뒤로는 약 60초 주기로만 반복한다. 조회가 회복되면 `info` 로 연속 실패 횟수와 함께 남긴다. `error` 인 이유는 이 상태가 지속되면 task DAG 는 정지해 있는데 `task_list` 응답의 `ready_count`/`running_count` 는 여전히 진행 중으로 보이기 때문이다.

`agent.task_run`(start/stop/status)은 `METHOD_TABLE` 에 `plugin(&[AgentManage])` 로 등록돼 있다 — 호스트가 재시작 시 runner 를 자동으로 켜지 않으므로(아래 "재시작 계약"), plugin 이 자기 workspace 의 runner 를 스스로 되살릴 수단이 필요하기 때문이다. `task_set_result` 는 `local_only()` 로 등재돼 있다 — 러너가 Custom task 의 생명주기를 단독 소유하므로 plugin 이 같은 task 를 별도로 전이시키면 쓰기 주체가 둘이 되어 러너의 완료 판정과 경합한다. plugin 은 완료 판정 전략 선언(아래 "완료 판정 전략 레지스트리")으로 우회한다.

## 재시작 계약

**자동 시작은 하지 않는다.** 호스트 재시작 후 어떤 workspace 의 runner thread 도 자동으로 켜지지 않는다 — `agent.task_run --action start` 로 수동(또는 plugin) 재개해야 한다. 대신 다음 두 가지를 보장한다:

1. **재시작 후 정리는 부팅 시 1회, runner 없이도 수행한다.** `purge_stale_agent_state_on_boot`(`TaskService`, `crates/tasty-task-runtime/src/service.rs`)가 headless(`src/boot.rs`, host IPC injector 등록 + `CoreState` 확보 직후)와 GUI(`src/app/boot_machine.rs::finish_boot`, 첫 윈도우 등록 직전) 양쪽 부팅 경로에서 호출된다. 라이브 `CoreState::workspaces()` 합성 조회의 모든 workspace에 대해 아래 "호스트 재시작 후 정리 + 핸들 영속" 절의 3종 세트(`purge_stale_semaphore_holders`/`purge_stale_lease_holders`/`reload_persistent_handles`)를 수행한 뒤 Waiting task 전체의 readiness 를 다시 평가하고(`TaskStore::resettle_waiting` — 완료 쓰기 뒤 하류 반영 전에 멈췄거나 그래프 레코드를 쓴 뒤 readiness 반영 전에 멈춘 task 를 마무리한다. 이미 맞는 상태면 바꾸지 않는다), `reload_persistent_handles` 가 되살린 handle 목록은 버린다(이 시점엔 그걸 넘겨받아 poll 할 runner 가 없다 — 다음 수동 start 가 다시 reload 한다). task 가 없는 workspace 는 각 정화 함수가 candidates 없음으로 조기 반환하므로 실질적으로 no-op — "라이브 workspace ∩ task 보유 workspace" 교집합과 동치. 여러 번 호출해도 안전(idempotent): `alive` 분류는 부수효과가 없고, `dead`/`stale`/`precise` 분류는 이미 정리된 뒤엔 대상이 남지 않는다.
2. **정지 상태는 조회로 드러난다.** `task_run --action status` 뿐 아니라 `task_list`/`task_graph` 응답에도 `runner: { running, crashed, ready_count, running_count, store_error, list_failures }` 를 동반한다 — runner 가 꺼져 있어도(`running: false`) `ready_count`/`running_count` 는 store 를 직접 조회한 실제 값이라, "비-terminal task 는 있는데 아무도 안 돌리고 있다"가 이 응답만으로 드러난다. **그 조회 자체가 실패하면 두 카운트는 `null`** 이고 `store_error` 가 이유를 싣는다 — 0 을 돌려주면 "task 가 없다" 와 값이 같아져 이 계약이 거짓이 된다. 러너는 살아 있는데 계속 못 읽는 상태는 `list_failures`(연속 실패 횟수)로 드러난다: `running: true` 이면서 이 값이 크면 DAG 는 정지 상태다. `task_get` 응답은 task 가 `AwaitExternal` handle 로 외부 신호를 기다리는 중이면 `awaiting_external: { wait_key, deadline_ms }` 를 함께 실어 "그냥 running" 과 구분한다(`AwaitExternal` 의 poll 은 계약상 항상 Active 라 state 만으로는 대기 이유를 알 수 없다). CLI(`tasty agent task-{list,get,run}`)는 이 값들을 사람이 바로 읽는 텍스트로 렌더한다(`crates/tasty-cli/src/format.rs`) — runner 가 멈춰 있고 대기 중인 task 가 있으면 재개 커맨드까지 안내 문구로 보여준다.

`hook_task_waits`(hook_id → task_id 매핑)는 **비영속**(프로세스 메모리 전용)이다 — 재시작하면 사라진다. hook ID 는 재시작 뒤 다시 발급될 수 있으므로 재시작 후 `AwaitExternal` task 는 **훅으로는 깨어나지 않고**, 그 handle 에 실린 `deadline_ms`(위 참조)로 마감된다.

- reload 시점에 이미 만료된 handle 은 즉시 `Failed`(`host restart: push completion strategy deadline already expired`)다.
- 아직 만료되지 않은 handle 은 복원하면서 저장된 기한을 `HookTaskWaits::register_restored` 로 같은 공유 대기에 다시 건다. 이 항목은 hook ID 로 찾을 수 없고 기한으로만 회수된다. 기한이 지나면 재시작 없는 경로와 같은 `expire_overdue_hook_waits` 가 `push completion strategy timed out waiting for external report` 실패를 `complete_task` 로 기록한다. 그래서 `task-await` 대기자를 깨우고 하류 readiness 를 다시 평가하는 경로도 같다.
- 회수는 공유 대기의 락 안에서 항목을 제거하므로 여러 workspace 러너가 같은 항목을 두 번 종결하지 않는다. 부팅 정리와 수동 start 가 둘 다 reload 해도 같은 task 의 기한은 하나로 남는다. 같은 task 를 기다리는 hook 매핑이 아직 있으면(프로세스 재시작 없이 러너만 다시 시작한 경우) 그 매핑이 만료를 맡으므로 따로 걸지 않고, 새 회차가 hook 을 걸면 이전 회차의 복원 기한은 지운다.
- 기한 전에 외부 상태 변경으로 task 가 끝났거나 다른 회차로 넘어갔으면, 회수 시 task 가 그 회차로 `Running` 인지 확인해 시간 초과를 기록하지 않는다.
- 기한 검사는 hook 대기와 마찬가지로 실행 중인 러너(어느 workspace 든)의 tick 이 한다. 부팅 정리 뒤 러너가 하나도 시작되지 않았으면 기한이 지나도 task 는 `running` 으로 남고, 첫 러너가 시작되면 그 첫 tick 에서 시간 초과 실패로 끝난다. 러너가 꺼져 있다는 사실은 위 2번의 `runner` 조회로 드러난다.

### 자동 GC

`purge_stale_agent_state_on_boot`(`crates/tasty-task-runtime/src/runner_thread.rs`)의 같은 루프 안에서, 위 3종 세트 정화 직후 `gc_stale_tasks(ctx, workspace_id)` 가 한 번 더 돈다 — task 삭제 경로(`agent.task_delete`/`agent.task_purge`, `crates/tasty-agent/src/task/store/sweep.rs::{delete_checked,plan_sweep,apply_sweep_plan}`)와 정확히 같은 참조 안전 로직을 태우는 자동 스윕이다.

- **임계값**: `AGENT_TASK_GC_MIN_AGE_MS`(`runner_thread.rs`, 잠정 7일) — 상태와 무관하게 `now - 기준시각 >= 임계값` 인 task 가 후보. 기준시각은 terminal task 는 `finished_at`, 그 외(`waiting`/`ready`)는 `created_at`. 값 자체는 provisional — 실사용 데이터가 쌓이면 재검토 대상.
- **상태를 terminal 로 제한하지 않는 이유**: 방치된 `waiting` task(예: 입력이 끝나지 않는 `Reduce`)를 terminal-only 로 제약하면 영원히 못 지우고, 그게 참조로 자기 입력들을 붙잡아 그 입력들도 영영 GC 대상에서 빠진다. `running` 은 `plan_sweep` 이 항상 후보에서 제외하므로 별도 처리가 필요 없다. handle 이 남은 task(종료 확인 중)도 `keep` 으로 넘겨 제외한다.
- **`PutOpts.expires_at` 류의 memory 자체 TTL 은 쓰지 않는다** — TTL 만료는 참조 무결성·상태 검사를 완전히 우회한 채 그냥 지워버려, dangling 참조·자원 누수를 다시 끌어들이기 때문이다. 항상 `plan_sweep`(순수 함수, 후보 선정) → `apply_sweep_plan`(실제 삭제) 을 거치고, 실제로 지워진 task 마다 `tasty.agent.handle.<id>`/`tasty.agent.run_result.<id>` side-key 도 `evict_task_side_keys` 로 정리한다.
- `plan_sweep` 은 fixed-point 로 "후보 집합 밖에서 참조되는 task"만 반복 제외한다 — 즉 서로를 참조하는 방치 task 끼리는(예: `waiting` `Reduce`(X)가 그 input(Y)을 참조) 후보 집합 안에서 함께 드레인되고, 후보 밖의 살아있는 task 가 참조하는 대상만 보존(`retained`)된다.

## host→plugin 동기 IPC (`HostIpcInjector`)

runner thread 는 off-main 이라 `PluginManager`(App main thread 단독 소유)를 직접 못 부른다. injector 경유: `IpcCommand`+`sync_channel(1)` 을 App IPC 큐에 push → waker 로 App 깨움 → tick 의 routing 이 plugin 에 forward → 응답이 sync_channel 회신 → runner 의 `recv_timeout(5s)`. `Core::set_host_ipc_injector` 가 IPC 시작 직후 1회 등록(boot.rs headless + app/boot_machine.rs gui 양쪽).

주입은 IPC 서버와 **같은 큐 개수·바이트 제한**을 거친다. 큐에 든 호스트 주입 명령이 이미 상한만큼이거나(메인 루프가 서 있는 동안 시간 초과로 돌아간 호출이 남긴 명령이 쌓인 경우) 큐의 바이트 합이 넘치면, 명령은 큐에 들어가지 않고 `InjectError::Refused` 로 즉시 돌아온다 — `InjectError::nothing_ran()` 이 참이라 실행되지 않았음을 알 수 있고, 시간 초과(결과 불명)와 갈린다. runner 의 `dispatch_plugin` 은 그 오류를 문자열(문구에 "nothing ran")로 task 결과에 올리고 다시 걸지 않는다(만료·시간 초과의 처리는 위 "호스트 응답 지연"). 상한 값과 근거는 [IPC 서버](../architecture/ipc-server.md). 주입은 제 대기 상한(5 s)을 명령의 기한으로도 싣는다 — 상한까지 큐에서 못 나간 명령은 나중에도 실행되지 않고 `InjectError::Expired`(문구 `… while still queued (nothing ran)`)로 돌아오며, 상한 전에 시작된 명령만 `InjectError::Timeout`(결과 불명)이다. 근거는 [IPC 서버](../architecture/ipc-server.md).

## 동기화 primitive 통합

| primitive | 통합 위치 | 결합 |
|-----------|----------|------|
| `Semaphore` | RunnerLoop dispatch | `task.metadata.semaphore = { name, holder?, ttl_ms? }`. `ttl_ms` 는 러너가 갱신(아래 dispatch 게이트) |
| `Lease` | RunnerLoop dispatch | `task.metadata.lease = { resource, holder?, ttl_ms?, mode? }` 또는 pool 모드(`candidates`/`elastic` — 아래 "자원 풀 배정"). `ttl_ms` 는 러너가 갱신(아래 dispatch 게이트) |
| `Barrier` | dispatch/poll | `WaitBarrier { name }` task(DAG 안 명시 gate) |
| `RateLimit` | IPC dispatcher 미들웨어 | `(agent, "ipc_calls")` 호출당 1 차감 |

### dispatch 게이트 (lease → semaphore)

러너는 lease를 먼저 획득하고 semaphore를 획득한다. 다음 자원을 얻지 못하면 먼저 얻은
자원을 즉시 반납한다. task가 성공·실패·취소로 끝나면 점유한 자원도 반납한다.

실행 중인 Run·후처리가 밖에서 종결되면(취소 등) 그 프로세스 묶음을 끝내고, 끝난 것을 확인한 다음
tick 에 자원을 반납한다(`release_permit` → `stop_run`, `maintain`). 생사를 모르는 프로세스가 쥔 자원을
다른 task 에 넘기지 않기 위해서다. Run 묶음은 후처리와 같은 규칙이다. Unix 는 자식을 새 프로세스
그룹의 리더로 띄워 그룹 전체에 SIGKILL 을 보내고, Windows 는 자식을 job 에 넣어 job 을 끝낸다
(`runner_host/run_group.rs`). 후처리와 달리 Run 은 호스트가 다시 시작해도 계속 실행되므로 호스트
수명에 묶지 않고, Windows job 도 닫혀도 프로세스를 끝내지 않는다.

Run 의 끝과 취소 때 끝내는 범위는 다르다.

- **Run 의 끝**: 같은 호스트에서 시작한 Run 은 리더가 끝나고 stdout·stderr 가 모두 EOF 일 때 끝난다
  (watcher). 리더가 끝나도 그룹 구성원이 출력을 쥐고 있으면 Run 은 Running 이고 점유도 유지한다.
  출력을 닫고 남은 구성원(`cmd >/dev/null 2>&1 &` 같은 데몬)은 Run 이 아니어서, Run 이 끝나면
  남아 있어도 점유를 반환한다. 데몬을 띄우는 명령이 점유를 끝없이 쥐지 않게 하는 계약이다.
- **취소 때 끝내는 범위**: Run 이 끝나기 전에 취소하면 리더가 이미 끝났어도 그룹 전체에 SIGKILL 을
  보내고, 리더가 끝나고 그룹이 빈 것(`kill(-pgid, 0)` 이 ESRCH)을 확인한 뒤에 반환한다. 리더가 끝난
  뒤의 그룹 신호는 이 호스트의 watcher 가 아직 출력을 기다리는 Run 에만 보낸다(`run_group::adopt`
  부터 `forget` 까지, 러너가 멈춰도 유지). 그룹에 구성원이 남아 있는 동안 그 id 는 새 프로세스에
  쓰이지 않는다.
- **재시작 뒤 넘겨받은 Run**: 출력을 볼 수 없어 리더의 끝을 Run 의 끝으로 본다. 리더가 살아 있을 때
  취소하면 같은 규칙으로 그룹 전체를 끝내고 그룹이 빈 뒤에 반환한다.

한계:

- 스스로 새 세션·그룹으로 옮긴 프로세스는 끝내는 대상에 들지 않는다. 그런 프로세스가 출력을 쥐고
  그룹이 비면 그룹 id 가 다시 쓰일 수 있는데(PID 가 한 바퀴 돈 뒤), 그 경우는 막지 않는다.
- Windows 는 Tasty 가 다시 시작하면 job 을 다시 열 수 없어 직접 자식만 끝내고 리더의 종료만
  확인한다. 손자 프로세스는 남을 수 있다. job 에 넣기 전에 만든 프로세스도 대상에 들지 않는다.

끝낼 프로세스는 저장한 PID 와 프로세스 시작 시각으로 같은 프로세스임을 확인한 뒤에만 정한다
(`tasty_agent::platform::process_start`). Run·후처리 handle 레코드는 시작 시각을 `started_at` 키에
함께 저장한다. 시작 시각이 없는 레코드(이 키를 남기기 전에 만든 handle)는 같은 프로세스인지 알 수
없어 신호를 보내지 않고 이전처럼 점유만 반환한다. 이 키가 생기기 전 버전에서 시작해 업그레이드 뒤에도
살아 있는 Run 에만 생기는 한계다. 확인과 신호 사이에 리더가 끝나고 PID 가 다시
쓰이는 짧은 틈은 막지 않는다.

시작 시각은 Linux 가 `/proc/<pid>/stat` 의 22번째 필드(부팅 이후 clock tick), macOS 가
`sysctl(KERN_PROC_PID)` 의 `kp_proc.p_starttime`(마이크로초), Windows 가 `GetProcessTimes` 의 생성
시각에서 읽는다. macOS 의 `proc_pidinfo(PROC_PIDTBSDINFO)` 는 같은 값을 주지만 회수 전 종료
상태(좀비)와 다른 사용자의 프로세스에서 실패해 쓰지 않는다. 그러면 좀비 리더를 끝난 프로세스로
보게 되어 Linux 와 판정이 달라진다.

macOS 실 기기 측정(Darwin 27.0 arm64, 격리 debug GUI 인스턴스):

- 시작 시각은 살아 있는 프로세스에서 `ps -o lstart` 의 초와 일치했고 같은 PID 를 다시 읽어도 같았다.
  좀비와 root 프로세스도 읽었고, 회수한 PID 는 읽지 못했다.
- `sh -c 'sleep 300 & …; exit 0'` Run 에 permit 1 인 semaphore 를 걸고 취소했다. 리더가 끝나고 sleep 이
  출력을 쥔 상태에서 그룹 SIGKILL 을 보냈다(01:15:01.202). kqueue 로 잡은 sleep 종료는 .203 이었다. permit 은
  다음 tick 인 .707 에 반환됐고 기다리던 task 가 그 permit 을 얻었다.
- 리더가 살아 있는 Run(`sh -c 'sleep 300 & …; wait'`)은 호스트를 종료해도 남았다(부모가 launchd 로
  바뀐다). 다시 띄우고 러너를 시작한 뒤에도 permit 을 쥐고 있었다. 이 Run 을 취소하면 리더와 sleep 이 같은
  ms 에 끝났고, 그 뒤 tick 에 permit 이 반환됐다.
- 호스트가 꺼진 동안 저장한 `started_at` 을 1µs 바꿔 PID 재사용을 흉내 냈다. 부팅 정리는 그 handle 을 다른
  프로세스로 보았다. task 를 `unknown` 으로 끝내고 permit 을 반환했으며, 살아 있는 리더와 sleep 에는 신호를
  보내지 않았다.
- 좀비만 남은 그룹에 `kill(-pgid, 0)` 을 보내면 macOS 는 EPERM 을 돌려준다. 구현은 EPERM 을 구성원이 남은
  것으로 보므로, 회수되어 ESRCH 가 될 때까지 점유를 쥔다.

러너가 없을 때(러너를 멈췄거나 Tasty 재시작 뒤 아직 켜지 않음) 취소하면 `TaskService::task_cancel`
이 정리를 시작하고 기다리지 않고 응답한다(`settle_ended_tasks_in_background`). 별도 스레드가 취소한
task 와 함께 취소된 하류 중 handle 이 남은 task 의 프로세스 묶음을 끝내고, 종료를 확인하면 handle 을
지우고 그 task 가 자기 id 로 쥔 semaphore·lease 를 반환한다. 확인하기 전까지 점유는 그대로 남아 다른
task 에 배정되지 않는다. IPC 는 앱의 처리 경로에서 차례로 처리되므로, SIGKILL 이 늦게 듣는
프로세스(D 상태 등)를 기다리느라 다른 IPC 를 막지 않게 하려는 것이다. 이 스레드는
`BACKGROUND_CONFIRM_WAIT`(10분)까지 기다리고, 넘으면 둘 다 남겨 러너에 맡긴다. 스레드가
끝없이 남지 않게 하는 상한이다.

취소 후 정리는 종결 알림의 최초 전달·UUID 확인 처리와 별개다. 다른 요청의 지연된 후속 처리가
알림을 먼저 전달하고 확인 처리했어도, 취소 후 확인 읽기는 현재 종결 task와 남은 handle에서 정리
의무를 확인한다. 알림 행이 없어도 정리를 시작하며 같은 사건을 다시 발행하지 않는다. 새 회차가
Running이면 정리 대상으로 삼지 않고, handle 삭제와 점유 반환은 기존 회차·프로세스 대조를 따른다.
알림 토큰은 강한 읽기에서 공유하고 정확 UUID의 확인 쓰기 성공 또는 순서가 보장된 부재 확인
뒤에 registry에서 회수한다. 지연된 읽기 결과는 이미 전달한 공유 토큰을 계속 보유해 같은 사건을
다시 발행하지 않는다. 실패·결과 불명 확인 쓰기는 재확인 전까지 토큰을 남기며, 이 수명 관리가
취소 후 정리의 실행 여부를 결정하지 않는다.
백그라운드 정리는 확인 읽기에서 함께 읽은 task revision·회차와 handle 전체를 보존한다.
프로세스에 신호를 보내기 전에 현재 task가 같은 종결 기록이고 handle도 그대로인지 확인하며,
신호는 캡처한 PID·시작 시각에만 보낸다. 종료 뒤에도 같은 task·handle인지 다시 확인한 단위 안에서
handle을 CAS 삭제하고 자기 점유를 반환한다. 그 사이 retry나 다른 정리가 기록을 바꿨으면 지연된
정리는 새 handle·프로세스·점유를 건드리지 않고, 남은 이전 handle은 기존 러너 정리가 확인한다.

reload(러너 시작·부팅 정리)의 NotRunning 정리는 같은 정리를 그 자리에서 하며
`KILL_CONFIRM_WAIT`(2초)까지 기다린다(`settle_ended_task`). 2초는 SIGKILL·job 종료와 회수에
충분하면서 부팅이 오래 멈추지 않을 만큼 짧게 정한 값이다. 러너가 있으면 다음 tick 이 종결을
흡수해 위의 러너 경로로 정리한다.

두 상한 안에 확인하지 못한 handle 은 러너가 tick 마다 다시 본다(`settle_unwatched_handles`). 대상은
러너가 넘겨받지 않았고 task 가 없거나 Running 이 아닌 handle 이다. 신호를 한 번 보내고 기다리지 않고
종료를 확인해, 확인한 tick 에 위와 같이 정리한다. 러너가 켜져 있는 동안은 점유가 다음 재시작까지
묶이지 않는다. 시작 시각이 없는 옛 레코드는 같은 프로세스인지 알 수 없어 기다리지 않고 정리한다.

이 정리들은 확인한 handle 의 회차 id·프로세스(PID·시작 시각)가 지울 때도 그대로일 때만 handle 을
지우고 점유를 반환한다(`settle::finalize`, 한 잠금 안). 그 사이 다른 정리가 지웠거나 다른 회차의
handle 이 들어왔으면 손대지 않는다. 그리고 task 에 이전 회차의 handle 이 남은 동안 러너는 그 task 의
새 회차(retry 로 다시 Ready 가 된 task)를 시작하지 않고 Ready 로 둔다(`HostExecutor::dispatch` 가
미룬다). 점유의 holder 는 task id 라서 새 회차가 같은 holder 로 다시 얻으면 멱등으로 통과해, 끝나지
않은 옛 프로세스 묶음과 새 회차가 같은 permit 을 함께 쓰게 되기 때문이다. 이렇게 기다리는 task 는
`task_get`·`task_list` 의 `phase` 가 `waiting_previous_attempt` 다.

handle 이 남은 Running 아닌 task(종료 확인 중)는 지우지 않는다. `task_delete` 는 그 task 나 cascade 로
함께 지울 참조자 중 하나라도 그렇다면 `-32602` 로 거절하고 이유(exit confirmation)를 싣는다.
`task_purge` 와 자동 GC 는 그런 task 를 Running 처럼 후보에서 빼고, purge 는 `skipped`·`skipped_count`
로 알린다(`TaskStore::plan_sweep` 의 `keep`). 지우면 handle 이 함께 지워져 확인 뒤 점유를 반환할
정리가 없어지고, 반환은 task metadata 에서 자원 이름을 읽으므로 task 가 없으면 permit 이 남는다.

TTL을 생략하면 자동 만료하지 않는다.

task 의 lease·semaphore TTL(`metadata.lease.ttl_ms`·`metadata.semaphore.ttl_ms`)은 러너가 갱신한다
(`runner_host/ttl_renewal.rs`). 러너가 그 점유를 쥐고 있는 동안(task 가 종결돼 프로세스 종료를
기다리는 동안 포함, 반환 전까지) TTL 의 절반이 지날 때마다 만료 시각을 지금 + TTL 로 늦춘다. 획득
시각은 바꾸지 않는다(`LeaseStore::renew`·`SemaphoreStore::renew`). 절반 주기는 다음 갱신이 tick
지연·저장소 대기로 늦어져도 남은 절반 안에 들면 만료되지 않게 하는 여유다. 재시작 뒤 넘겨받은
Run·후처리의 lease·permit 도 넘겨받은 다음 tick 에 바로 늦추고 이어서 갱신한다.

TTL 의 하한은 `MIN_HOLDING_TTL_MS`(1초 = tick 500ms 의 두 배)다. 갱신은 TTL 의 절반이 지난 뒤
처음 오는 tick 시작(`maintain`)에 하고, 그 tick 은 늦어도 절반 + tick 간격에 오므로 TTL 이 tick 의
두 배 이상이어야 만료 전에 갱신된다. `task_create`·`task_graph_submit` 은 metadata(inline fallback
포함)의 `lease.ttl_ms`·`semaphore.ttl_ms` 가 하한보다 짧으면 `-32602` 로 거절한다(`check_holding_ttls`).
하한 검사가 생기기 전 버전에서 저장된 task 는 다시 검사하지 않으므로, 하한보다 짧은 TTL 을 가진 채
남아 있으면 갱신 전에 만료될 수 있다.

만료 시각이 지났어도 다른 holder 가 가져가지 않았으면 다시 늦춘다. 이미 다른 holder 가 쥐었으면
되찾지 않고 갱신을 멈추며, task 는 계속 실행된다(프로세스를 멈출 근거가 없다). 이 일은 task 조회의
`holding_warnings`(`kind`·`name`·`holder`·`at_ms`·`message`·`attempt_id`)에 남는다. 기록은 그때의
회차 id 를 담고, 조회는 지금 회차의 것만 보인다(retry 뒤에는 이전 회차의 경고가 보이지 않는다,
v1 task 는 회차 id 가 없다)(`runner_host/holding_warning.rs`,
memory 키 `tasty.agent.holding_warning.<task_id>`, task 당 최근 16건, task 를 지울 때 함께 지운다).
러너가 tick 을 오래 건너뛴 경우(앱 멈춤·저장소 대기)에만 생긴다. 러너가 꺼진 동안에는 아무도
갱신하지 않으므로, 그 사이 TTL 이 지나면 다른 holder 가 얻을 수 있다. TTL 은 이제 "task 를
지켜보는 러너가 없어진 뒤 자원을 돌려받을 시간" 으로 정한다. 근거는 docs/features/agent-collaboration/index.md 의 "task
lease·semaphore TTL 갱신" 절.

호스트 재시작 시에는 `holder == task.id`인 러너 소유 점유만 정리한다.
다른 holder로 잡은 외부 도구의 점유는 그 도구나 운영자가 정리해야 한다.

### 죽은 홀더와 한도 조정

`semaphore-list`는 홀더의 `id`, `acquired_at`, `expires_at`을 보여준다.
옛 형식으로 저장돼 획득 시각을 모르면 `acquired_at`을 생략하며 0으로 대신하지 않는다.
저장된 옛 문자열 형식의 홀더도 계속 읽을 수 있다.

`semaphore-acquire --ttl-ms <N>`은 `now + N`에 만료된다. 만료된 점유는 다음 acquire나
list에서 회수한다. 같은 holder의 재획득은 중복 permit을 쓰지 않고 시각을 갱신한다.
TTL 없이 잡은 점유는 자동 만료하지 않으므로 운영자가 실제 작업 상태를 확인해야 한다.

한도를 바꿀 때는 삭제·재생성하지 않고 다음 명령을 쓴다. create는 기존 이름을 거절한다.

```sh
tasty agent semaphore-set-permits --workspace 1 --name cap2 --permits 3
tasty agent semaphore-set-permits --workspace 1 --name cap2 --permits 1
```

한도를 줄여 현재 홀더 수보다 작아져도 기존 작업은 그대로 둔다.
새 획득만 제한하고 반납을 기다리며, 그동안 `permits_available`은 0이다.
`is_over_subscribed()`로 한도 초과 여부를 구분할 수 있다.
홀더가 종료돼 반납할 수 없다면 `semaphore-release --holder <id>`로 대신 반납하거나
한도를 늘릴 수 있다. 획득 시각만으로 종료를 단정하지 말고 작업의 실제 상태도 확인한다.

### 동시성 제한 (concurrency limit)

Semaphore 를 이 용도로 쓴다. 로컬 오케스트레이터(예: conductor 처럼 여러 task 를 동시에 굴리는 상위 도구)가 리소스가 약한 환경에서 "동시 실행 개수"에 상한을 두고 싶을 때 쓰는 패턴이다. `agent.rate_limit_*`(IPC dispatcher 미들웨어, 위 "rate_limit 미들웨어" 참조)는 *호출 빈도* 제한이고 `Local` caller를 면제하므로 이 목적에 맞지 않는다 — 여기서 쓰는 건 위 표의 `Semaphore` 통합이다.

절차:

1. 세마포어를 원하는 permit 수로 만든다: `agent semaphore-create --workspace 1 --name cap2 --permits 2`.
2. 동시성을 묶고 싶은 task 들을 만들 때 각각 `--metadata '{"semaphore":{"name":"cap2"}}'` 를 붙인다(편의 플래그가 있으면 `--concurrency-limit cap2` 로 대체 가능 — 아래 "CLI 예" 참조).
3. `metadata.semaphore` 를 안 붙인 task 는 이 제한과 무관하게 즉시 병렬 실행된다 — 세마포어는 **태그가 붙은 task 끼리만** 경쟁한다. 서로 다른 이름의 세마포어를 쓰면 그룹별로 독립된 상한을 걸 수 있다.

동작(dispatch 게이트, 위 참조): permit 이 남아있는 동안은 태그된 task 가 즉시 `Running` 으로 전이하고, permit 이 바닥나면 dispatch 가 `Deferred` 를 반환해 `Ready` 상태를 유지한 채 다음 tick 에 재평가한다 — 큐잉이 자동이라 오케스트레이터가 직접 대기열을 관리할 필요가 없다. task 가 종결(Succeeded/Failed/Cancelled)되면 permit 이 자동 반환되고, 대기 중이던 다음 task 가 같은 tick 이후 자동으로 이어받는다.

라이브 검증 예(2-permit 세마포어에 독립 task 4개, 각 4초 sleep):

```sh
tasty agent semaphore-create --workspace 1 --name cap2 --permits 2
for i in 1 2 3 4; do
  tasty agent task-create --workspace 1 --name "t$i" \
    --command '{"kind":"run","command":["sleep","4"]}' \
    --concurrency-limit cap2   # 또는 --metadata '{"semaphore":{"name":"cap2"}}'
done
tasty agent task-run --workspace 1 --action start
tasty agent task-list --workspace 1   # 즉시 조회 시 2개만 running, 2개는 ready
# … 4초 후
tasty agent task-list --workspace 1   # 앞 2개가 끝나며 나머지 2개가 자동으로 running 전환
tasty agent semaphore-list --workspace 1
# 전부 종결 후 cap2 항목: permits_available == 2, holders: []
# holders 원소는 { id, acquired_at?, expires_at? } — 누가 언제부터 잡고 있는지가 보인다
```

### 자원 풀 배정 (lease pool — `candidates`/`elastic`)

`Semaphore`(동시성 제한)는 "몇 개까지"만 표현한다 — 어느 task 가 어느 슬롯을 받았는지는 알 수 없다. `Lease` pool 모드는 "N개 후보(예: `wt-1..wt-N` 워크트리) 중 하나를 배정받고, dispatch된 task 가 실제로 어느 걸 받았는지 알아야" 하는 시나리오(conductor 의 worktree pool 등)를 표현한다. `task.metadata.lease` 를 아래처럼 확장한다:

```jsonc
{
  "lease": {
    "candidates": ["wt-1", "wt-2", "wt-3"],   // resource(단일 문자열) 대신 후보 배열
    "holder": "...", "ttl_ms": 0, "mode": "block",  // 기존 필드와 동일 의미
    "elastic": { "max_candidates": 4, "overflow_prefix": "wt-overflow-" }  // 생략하면 fixed
  }
}
```

`resource: "x"` 는 `candidates: ["x"]` 의 단일 후보 표현이다 — 둘 다 store 안에서 같은 `lease_key`(`crates/tasty-agent/src/lease.rs`) 위치에 쓰기 때문에 같은 자원을 점유한 것으로 판정한다(같은 자원을 가리키면 서로 충돌 판정된다). 별도 코드 경로 통합은 하지 않는다 — `resource` 단일 경로(`LeaseStore::acquire`)는 `agent.lease_acquire` IPC 등 기존 호출자의 `LeaseConflict` 에러 계약을 그대로 유지하려고 독립 구현을 보존한다.

**두 서브모드 — `elastic` 은 반드시 명시적 opt-in, 기본은 fixed다:**

- **fixed**(`elastic` 생략, 기본): `candidates` 안에서만 순회한다. 전부 점유 중이면 `mode` 에 따라 실패(`fail`) 또는 대기(`block` → Deferred, 다음 tick 재시도).
- **elastic**(`elastic: {...}` — 빈 객체 `{}` 도 opt-in으로 인정): candidates 가 전부 소진되면 `overflow_prefix + N` 형태의 새 후보 이름을 store 가 원자적으로 합성해 즉시 배정한다. `max_candidates` 를 주면 그 상한(고정 candidates 개수 + 합성된 개수)까지만 증설하고, 넘으면 fixed 와 동일하게 대기한다.

elastic은 store에서 새 자원 이름을 배정한다. 실제 워크트리나 디렉터리는 만들지 않는다.
다만 호출자가 그 이름을 받아 자원을 생성하면 추가 비용이 생기므로 자동 증설은 명시적으로
선택해야 한다. 기본 fixed 모드는 선언한 후보 안에서만 배정한다.

**원자성**: 새 candidate 이름 합성은 `LeaseStore::acquire_any` 한 호출 안에서(카운터 읽기 → 스캔 → 필요시 카운터 +1 → 새 이름으로 acquire) 순차 수행된다. 이 호출 전체가 `RunnerContext::store` 의 writer 복합 연산 하나 안에서 실행되므로(기존 `try_acquire_lease` 관례와 동일), 판단과 쓰기가 같은 transaction 안에서 끝난다 — 워크스페이스마다 runner thread 가 정확히 하나뿐이라 워크스페이스 내부 경쟁이 없고, 워크스페이스 간 경쟁은 memory writer 의 직렬 실행으로 정리된다. 결과를 모르면(`Unknown`) 같은 holder 의 점유를 돌려주고 다음 tick 으로 미룬다. 별도 CAS/락 primitive 를 새로 만들지 않는다.

**합성된 candidate 의 재사용**: 카운터는 "지금까지 합성된 개수의 상한"일 뿐 현재 점유 개수가 아니다. 매 `acquire_any` 호출이 `candidates ++ (합성된 이름 전체)`를 다시 스캔하므로, 합성됐다가 release 된 이름은 다음 배정에서 빈 자리로 재발견돼 재사용된다 — 카운터가 오르는 건 그 스캔에서도 빈 자리가 전혀 없었을 때뿐이다.

**dispatch 시점 치환**: 배정된 resource 식별자는 `dispatch_command` 호출 직전 `task.command` 에 주입된다(`substitute_lease_resource`, `crates/tasty-task-runtime/src/runner_host.rs`).

- `TaskCommand::Run`: `cwd` 가 `None` 이면 곧장 그 resource 경로로 채운다(가장 흔한 용법 — "이 후보에서 실행해라"). `cwd`/`command` 인자 안에 `${lease.resource}` placeholder 가 있으면 그 부분만 실제 resource 로 치환한다(원래 값을 통째로 덮지 않음).
- `TaskCommand::Custom.params`: JSON 트리 전체를 재귀적으로 훑어 문자열 값 안의 `${lease.resource}` 를 치환한다(예: `claude.spawn` 의 `cwd` 파라미터).
- `${lease.resource}` 는 두 placeholder 중 하나다 — 선행 task 의 출력을 파라미터로 넘기는 `${task.<id>.output<pointer>}` 는 위 "선행 task 출력을 파라미터로 넘기기" 참조. 둘은 같은 dispatch 지점에서 lease → 출력 순으로 적용된다.
- elastic 으로 새로 합성된 이름이 가리키는 실제 워크트리를 만드는 건(예: `git worktree add`) task 의 Run 커맨드 쪽 자원 생성 책임이다 — lease primitive 는 포트/임시디렉토리/GPU 슬롯 등에도 쓰이는 범용 도구라 워크트리 특화 side-effect 를 store 안에 넣지 않는다.

**주의 — `cwd` 를 배정된 resource 로 직접 채우는 방식은 elastic 자원 생성과 함께 쓸 수 없다.** `cwd` 는 프로세스 spawn(`chdir`)이 그 경로로 실제로 이동을 시도하는 시점에 적용되는데, 이 chdir 은 커맨드가 실행되기 **이전에** OS 레벨에서 일어난다 — 아직 디스크에 없는 합성 경로를 `cwd` 로 주면 `Run spawn 'sh': No such file or directory` 로 spawn 자체가 즉시 실패하고, `command` 안에 `mkdir -p` 를 아무리 앞세워도 그 스크립트도 실행되지 못한다. 자원 생성이 필요하면 `cwd` 는 항상 존재하는 고정 경로(예: `/tmp`, 부모 워크트리 루트)로 지정해 두고, `${lease.resource}` 는 **`command` 인자 쪽**에서 받아 그 안에서 `mkdir -p`/`cd` 를 수행한다(아래 elastic 라이브 검증 예 참조).

**release**: pool 모드로 얻은 자원도 일반 lease 와 동일하게 `release_lease`(task 종결 시 자동 호출)로 반환된다 — `held_leases` 가 이미 "실제로 받은 resource 문자열"만 저장하므로 pool 여부와 무관하게 그대로 동작한다(재설계 불필요).

lease pool 전용 CLI 플래그는 없다. `--metadata '{"lease":{"candidates":[...],"elastic":{...}}}'`로 지정한다.

라이브 검증 예(3개 candidates 에 5개 task, fixed):

```sh
mkdir -p /tmp/wt-1 /tmp/wt-2 /tmp/wt-3
for i in 1 2 3 4 5; do
  tasty agent task-create --workspace 1 --name "t$i" \
    --command '{"kind":"run","command":["pwd"]}' \
    --metadata '{"lease":{"candidates":["/tmp/wt-1","/tmp/wt-2","/tmp/wt-3"]}}'
done
tasty agent task-run --workspace 1 --action start
tasty agent task-list --workspace 1   # 3개만 running(서로 다른 cwd), 2개는 ready(대기)
tasty agent task-get --workspace 1 --id t-...   # command.cwd 로 실제 배정된 candidate 확인
```

elastic 을 켜면 5개 모두 대기 없이 동시 실행되고, 그중 2개는 `/tmp/wt-3-overflow-1`/`-2` 처럼 아직 디스크에 없는 합성 경로를 받는다 — 위 "주의" 대로 `cwd` 를 그 합성 경로로 직접 채우면(fill-when-absent) spawn 이 즉시 실패하므로, `cwd` 는 존재하는 고정 경로에 두고 `${lease.resource}` 를 command 인자로 받아 스스로 `mkdir -p`/`cd` 하게 한다:

```sh
mkdir -p /tmp/wt-1 /tmp/wt-2 /tmp/wt-3
for i in 1 2 3 4 5; do
  tasty agent task-create --workspace 1 --name "e$i" \
    --command '{"kind":"run","command":["sh","-c","mkdir -p \"$1\" && cd \"$1\" && pwd","_","${lease.resource}"],"cwd":"/tmp"}' \
    --metadata '{"lease":{"candidates":["/tmp/wt-1","/tmp/wt-2","/tmp/wt-3"],"elastic":{}}}'
done
tasty agent task-run --workspace 1 --action start
tasty agent task-list --workspace 1   # 5개 모두 즉시 running, 대기 없음
tasty agent lease-list --workspace 1  # 3개 원본 + wt-3-overflow-1/-2 총 5개 holder
```

`cwd:"/tmp"`는 이미 존재하는 경로이므로 미생성 lease 경로 때문에 spawn이 실패하는 문제를 피한다. `sh -c '...' _ ${lease.resource}` 에서 `$1` 로 실제 배정된(원본 또는 합성) 경로를 받아 그 안에서 직접 생성·이동한다 — `${lease.resource}` 를 `cwd` 자체에 두는 방식(위 "주의")과 달리 이 패턴은 elastic 로 새로 합성된, 아직 존재하지 않는 경로에도 그대로 쓸 수 있다.

<a id="호스트-재시작-정화--핸들-영속"></a>

### 호스트 재시작 후 정리 + 핸들 영속

`held_permits`/`held_handles` 는 in-memory only이라 재시작 시 비지만, store 의 holders/handle 은 영속이라 leak 가능. `purge_and_reload_on_restart`(`crates/tasty-task-runtime/src/runner_thread.rs`)로 묶여 있고, **runner thread 없이도** 호출 가능하다 — 부팅 경로(위 "재시작 계약")와 `run_loop` 진입부(수동/plugin start) 양쪽이 이 함수 하나를 공유한다:

- `purge_stale_{semaphore,lease}_holders` — Running task 중 `metadata.*.holder == task.id` 만 release + task=Failed("host restart"). 단 저장된 handle(지금 회차의 것)로 다시 감시할 task 는 건드리지 않는다(`runner_host/holdings.rs::resumes_after_restart`). 본 작업 중이면 `ShellProcess`, 후처리 실행을 시작한(`started`) 회차면 같은 실행 번호의 `PostprocessProcess` 다. 그 점유는 아래 reload 가 정한다. 후처리를 기다리는(`pending`, 재시도 대기 포함) 회차도 뺀다. 실행 중인 프로세스는 없지만 reload 가 예약대로 후처리를 실행하므로 점유를 쥔 채 이어 간다(그때 저장된 handle 은 직전 본 작업의 `ShellProcess` 나 직전 실행의 `PostprocessProcess` 다). 점유가 없는 같은 단계의 task 와 결과가 같다.
- `reload_persistent_handles`(key `tasty.agent.handle.<task_id>`, workspace scope) — `ShellProcess` 는 저장한 PID·시작 시각으로 같은 프로세스가 살아 있는지 검사(`process_start::is_same_process`, 시작 시각이 없는 옛 레코드는 PID 생존만 본다. alive 복원 / dead 는 영속 `run_result`로 저장된 결과를 반영, 없으면 `unknown`). 복원한 `ShellProcess` 는 이 executor 의 watcher 가 없어(Tasty 는 그 프로세스의 부모가 아니다) 종료 코드와 출력을 받을 수 없다. 프로세스가 살아 있는 동안 task 는 Running 이고 semaphore·lease 를 쥔다. 러너 시작 때 executor 가 그 점유와 handle 을 넘겨받아(`adopt_restored_run`) 끝날 때 반환한다. 끝나면 `unknown`(`reason`: `run result lost: pid N ended after a host restart …`)이 된다. 저장된 결과가 없는 죽은 pid 도 `unknown`(`… ended before the runner resumed watching it …`)이다. 호스트가 내려간 동안 끝난 경우와, 부팅 정리 뒤 러너를 켜기 전에 끝난 경우(부팅 정리는 살아 있는 handle 을 복원만 하고 감시하지 않는다)가 여기에 든다. 이렇게 확정(저장 결과 또는 `unknown`)하면 그 task 가 쥔 점유를 반환한다(`release_own_holdings`). 그 사이 TTL 이 지나 다른 holder 가 얻은 자원은 holder 가 달라 건드리지 않는다. 러너를 켜지 않으면 감시할 주체가 없어 끝난 뒤에도 다음 러너 시작까지 점유가 남는다. handle 이 남았는데 task 가 이미 Running 이 아니면(러너가 꺼진 동안 `cancel` 했는데 그때 정리를 마치지 못한 경우 등) 위 "dispatch 게이트" 의 러너 없는 취소와 같은 정리(`settle_ended_task`)를 한다. 남은 프로세스 묶음을 끝내고 종료를 확인한 뒤 handle 을 지우고 그 task 가 자기 id 로 쥔 semaphore·lease 를 반환한다. 부팅 정리와 러너 시작(러너를 멈췄다 다시 켤 때) 모두 이 경로를 지난다. `PolledDispatch`/`BarrierPoll` 은 insert-only 복원(다음 tick poll). `AwaitDispatchAnswer` 는 응답 통로가 재시작을 넘지 못하므로 바로 `unknown`(`dispatch outcome unknown: …`)으로 확정한다. PolledDispatch 첫 poll 이 injector 미준비면 `INJECTOR_GRACE_MS=30s` 안에서 Active 유지. `AwaitExternal { deadline_ms, .. }` 은 `deadline_ms` 가 이미 지났으면 즉시 `Failed`(구 포맷도 `deadline_ms` 기본값 0 이라 이 분기), 아직이면 insert-only 복원 — 단 poll 이 절대 관여하지 않는 계약이라 다음 재시작 전까지는 deadline 이 재판정되지 않는다(위 "재시작 계약" 참조).

`ReduceImmediate`/`CustomImmediate`/`ImmediateFail` 은 영속 안 함(다음 tick 즉시 흡수 + reload 시 재dispatch side-effect 위험).

### rate_limit 미들웨어

`src/adapters/ipc/handler.rs` 의 미들웨어 체인:

```text
세션 판정 → ensure_allowed → check_cap_block → rate limit 소비·ipc_calls 기록(writer 실행 단위 하나) → audit Allow → route
```

차단 시 `-32010 throttled: tokens_left=N` + audit Deny. **면제**(`should_rate_limit`): `Local`(사용자 직접) · host 자기 호출(`_host`) · `telemetry.*`(재귀 폭주 방지) · `agent.rate_limit_*`(자가 회복 경로 — 막히면 영구 차단) · `system.info`. 미등록 `(agent, metric)` 은 면제(opt-in 모델). store 접근 실패는 fail-open(warn 후 통과 — 인프라 고장으로 전 IPC 차단은 과도).

## CLI 예

```sh
tasty agent task-run --workspace 1 --action start    # 시작(실행 중이면 no-op)
tasty agent task-run --workspace 1                    # 상태 조회
tasty agent task-run --workspace 1 --action stop      # 중단 요청(바로 응답, 자식 프로세스는 생존)
tasty agent task-run --workspace 1                    # state 가 stopped 인지 확인
```

`--action` 은 `clap::ValueEnum { Start, Stop, Status }` — 오타는 CLI 시점 거부.

```sh
tasty agent task-list --workspace 1                                   # 전체
tasty agent task-list --workspace 1 --state running                   # 단일 state
tasty agent task-list --workspace 1 --state waiting,ready,running     # 콤마 다중값 = "아직 안 끝난 task"
```

`task-list --state` 는 `task-purge --states` 와 **동일한 콤마 다중값 파싱**을 쓴다(플래그 이름만 단수/복수로 다르다 — 하위호환 때문에 유지). 여러 state 는 OR 매칭이고, 단일값은 예전과 같이 동작한다. IPC(`agent.task_list` / `agent.task_purge`)도 배열 `["waiting","ready"]` · 콤마 문자열 `"waiting,ready"` · 단일 문자열을 모두 받고, 단수/복수 키(`state` ↔ `states`)를 서로 폴백한다 — 콤마 목록을 단일값 필터로 넘겨 매칭이 0 건이 되고 "아직 안 끝난 task 가 있는가" 판정이 조용히 무력화되던 함정을 없앤 것. 매칭이 없으면 빈 목록, 필터 미지정이면 전체다.

**단, "빈 값" 의 의미는 두 커맨드가 다르다.** `task_list` 는 빈 값(`[]` / `""` / 콤마만)을 "필터 없음"(= 전체)으로 접지만, `task_purge` 는 **키가 있는데 이름이 하나도 없으면 "매칭 없음"** 으로 본다 — 즉 `{"states": [], "older_than_ms": ...}` 는 아무것도 지우지 않는다. 상태 목록을 동적으로 조립하는 호출자가 상태를 하나도 안 골랐을 때, 파괴적 명령에서 그걸 "상태 무관 전체 삭제" 로 승격시키지 않기 위해서다. `states` 키가 **아예 없거나 `null`** 이면 그건 그대로 "상태 필터 없음" 이므로 `older_than_ms` 만으로 전체가 후보가 된다(둘 다 미지정이면 `TaskService::task_purge` 가 거부).

```sh
tasty agent task-create --workspace 1 --name build --command '{"kind":"run","command":["cargo","build"]}' \
  --concurrency-limit cap2   # metadata.semaphore.name=cap2 를 자동 조립(위 "동시성 제한" 참조)
```

`--concurrency-limit` 는 `--metadata` 에 `metadata.semaphore = { name: <값> }` 하나를 채워 넣는 단축일 뿐이다 — `--metadata` 를 이미 쓰고 있으면 그 JSON 객체에 `semaphore` 키를 병합하고(다른 키는 보존), `--metadata` 가 이미 `semaphore` 를 담고 있으면(어느 쪽을 취할지 모호) 에러로 거부한다. `holder` 지정 등 더 세밀한 제어가 필요하면 이 플래그 대신 `--metadata '{"semaphore":{"name":"...","holder":"..."}}'` 를 직접 쓴다.

```sh
tasty agent dag-list                                   # 살아있는 전 workspace 의 DAG
tasty agent dag-list --workspace 1                  # 한 workspace 로 한정
tasty agent dag-list --include-tasks                   # 각 DAG 에 task id 목록 동봉
tasty agent dag-get --id c:t-1716800000123-1           # 그 DAG 의 부모 ID 표
tasty agent dag-get --id c:t-1716800000123-1 --format json  # 그 DAG 만의 nodes/edges
tasty agent dag-get --id d:build --workspace 1 --format dot
```

`task-graph`·`dag-get` 의 기본 출력은 [부모 ID 표](cli-structure.md#부모-id-표-formatparent_tablers)이고 열은 `id`, `parents`, `name`, `state` 다. `parents` 에는 DOT 과 같은 간선의 원본을 모두 넣으므로 fallback·전이 원본도 부모로 보인다. 간선 종류는 `--format json` 이나 `--format dot` 으로 본다. 갈래에서 선택되지 않은 task 의 `state` 는 DOT 라벨과 같은 `not selected` 다. 표 본문은 stdout 에, `cycle`·`runner` 는 stderr 에 한 줄씩 쓴다. 순환이 있어도 모든 행을 낸다. `dag-get` 의 `dag` 요약은 표에 싣지 않는다.

`task-graph`·`dag-get` 의 `--format dot` 은 응답의 `dot` 본문만 stdout 에 쓰고 `cycle`·`runner` 는 stderr 에 한 줄씩 쓴다(`crates/tasty-cli/src/format/graph_dot.rs`). stdout 을 그대로 Graphviz 에 넘기기 위해서다. 응답 전체(JSON)가 필요하면 `--format json` 이나 IPC `agent.task_graph`·`agent.dag_get` 을 쓴다.

DAG 는 영속 레코드가 아니라 `metadata.dag`(explicit, id `d:<값>`) 또는 그래프 연결성
(derived, id `c:<root task id>`)에서 도출된다 — 같은 task 집합이면 id 가 항상 같다.
`dag-list` 는 `--workspace` 를 생략하면 *살아있는* workspace 전부를 순회하며
(응답 `scope: "live_workspaces"`), 삭제된 workspace 에 남은 고아 task 는 열거하지
않는다(그건 아래 "자동 GC" 의 몫). 도출 규칙과 응답 필드는
[features/agent-collaboration](../features/agent-collaboration/index.md) 의 Task DAG 항목.

한 workspace 안에서 무관한 그래프를 여럿 돌릴 때 그룹을 명시하려면 생성 시
`metadata` 로 태깅한다:

```sh
tasty agent task-create --workspace 1 --name build --command '{"kind":"run","command":["true"]}' \
  --metadata '{"dag":"release","dag_name":"Release pipeline"}'
```

```sh
tasty agent task-delete --workspace 1 --id t-...              # 참조 있으면 거부 + 참조자 목록
tasty agent task-delete --workspace 1 --id t-... --cascade    # 참조자까지 함께 삭제
tasty agent task-purge --workspace 1 --states succeeded,failed --older-than-ms 604800000 --dry-run
```

## v2 타입 계약 (`contract_version: 2`)

task 는 선택적으로 타입 계약(`TaskContract`)을 가진다. 계약이 없는 task 가 v1 이며, 결과 형식·reducer 동작·저장 형식이 바뀌지 않는다. v2 task 는 IPC `agent.task_graph_submit`(CLI `tasty agent task-graph-submit`)으로 그래프 단위로 만들거나 Rust API `TaskStore::create_typed` 로 하나씩 만든다. `task_create` 는 계약을 받지 않는다.

코드: 타입 `crates/tasty-agent/src/task/types.rs`, 계약·결과 `crates/tasty-agent/src/task/contract.rs`, 입력 binding `crates/tasty-agent/src/task/binding.rs`(입력 매핑 `binding/mapping.rs`), 그래프 제출 `crates/tasty-agent/src/task/store/graph_submit.rs`(형식 읽기 `store/graph_parse.rs`), 실행 시 입력 해석 `crates/tasty-task-runtime/src/runner_host/typed_inputs.rs`, 회차 완료 기록과 handle 의 회차 `crates/tasty-task-runtime/src/runner_host/attempt_record.rs`, v2 reduce `crates/tasty-agent/src/reducer.rs::reduce_typed`, 후처리 계약·결과 확정 `crates/tasty-agent/src/task/postprocess.rs`, 후처리 단계의 완료 기록 `crates/tasty-agent/src/task/store/postprocess.rs`, 후처리 프로세스 실행 `crates/tasty-task-runtime/src/runner_host/postprocess.rs`, 전이와 경로 선택 `crates/tasty-agent/src/task/route.rs`, 자식 환경 `crates/tasty-agent/src/child_env.rs`, agent task 계약 `crates/tasty-agent/src/task/agent.rs`, 턴 표 `crates/tasty-task-runtime/src/agent_turns.rs`, agent task 실행 `crates/tasty-task-runtime/src/runner_host/agent.rs`.

### 계약 형식

```json
{"contract_version": 2,
 "types": {"Verdict": {"type": "enum", "values": ["pass", "revise", "review"]}},
 "input_schema": {"type": "object", "fields": {"retries": {"type": "int64", "default": 2}}},
 "output_schema": {"type": "object", "fields": {
   "verdict": {"ref": "Verdict"},
   "reviewer": {"type": "string", "nullable": true},
   "note": {"type": "string", "optional": true}}},
 "allowed_exit_codes": [0],
 "merge_conflict": "error"}
```

- 모르는 키는 계약·스키마 어디서든 거절한다. `contract_version` 은 2 만 받는다.
- 타입: `boolean`, `int64`, `float64`(`min`·`max`, 양끝 포함), `string`(`max_len`, 문자 수), `enum`(`values`, 비어 있지 않고 중복 없음), `object`(`fields`), `list`(`items`, `max_len`), `unit`, `json`, `{"ref": "<이름>"}`.
- 이름 붙은 타입은 `types` 에 선언하며 재귀를 허용하지 않는다.
- `nullable` 은 값 자리에 null 을 허용한다. `unit`·`json` 에는 쓸 수 없다. 필드 전용 키는 `optional`(생략 가능)과 `default`(생략 시 채울 값)다. 기본값은 선언할 때 그 필드 타입으로 검사한다.
- 선언하지 않은 필드는 오류다. int64 의 10진 문자열(아래) 외에는 암묵 변환이 없다. `42.0` 은 int64 가 아니고 정수는 float64 로 바꾸지 않는다.
- 한도: 스키마 깊이 32, 값 깊이 64, 값의 직렬화 크기 256KiB(`types.rs` 의 `MAX_SCHEMA_DEPTH`·`MAX_VALUE_DEPTH`·`MAX_VALUE_BYTES`). 근거는 docs/dev-guide/agent-runner.md.
  - 크기: memory 값 하나의 한도가 1MiB이고 v2 레코드에는 출력 외에 계약·raw 응답(run은 stdout·stderr 각 64KiB tail)·v1 투영이 함께 실리므로, 출력 하나를 그 4분의 1로 두었다.
  - 깊이: 검증·직렬화가 재귀하는 깊이를 막는 값이며 사람이 쓰는 스키마·결과에 충분한 여유를 둔 초기값이다.
- 오류(`TypeError`)는 종류, JSON Pointer 경로, 기대 타입, 실제 값 요약을 가진다. 결과 검증 실패는 task id 도 싣는다. IPC 는 `AgentError::TypeContract` 를 `-32602` 로 돌려주고 `error.data` 에 실패 단계와 타입 오류를 싣는다.

### int64 와 JSON 숫자

확정된 출력은 선언 타입을 아는 값(`TypedValue`, `task/types/value.rs`)으로 든다. 메모리 안에서 int64 는 i64 이고, reducer·결과 확정·이후 조건 평가는 이 값을 그대로 쓴다. 문자열은 wire 표현일 뿐이다. 변환은 `TypedValue` 의 `Serialize` 에 있어서 저장·IPC 응답·CLI 출력, 그리고 `typed_result` 만 따로 내보내는 경로까지 어떤 경로로 직렬화해도 int64 는 10진 문자열이 된다(proto3 JSON 과 같은 관례). 숫자를 f64 로 읽는 소비자(JavaScript 의 `JSON.parse` 등)를 지나도 i64 최솟값·최댓값과 9007199254740993 이 같은 정수로 돌아온다. wire 형식만으로는 int64 와 string 을 구별할 수 없으므로 역직렬화에는 스키마가 필요하다. `TypedValueSeed` 가 스키마를 받는 역직렬화이고, `Task` 의 역직렬화(`task/record.rs`)는 같은 레코드의 계약에서 출력 스키마를 얻어 읽는다. int64 자리에 정수가 아닌 값이 있으면 읽기 오류다.

- 입력으로는 JSON 정수 토큰과 10진 문자열을 모두 받는다. 문자열은 `-?(0|[1-9][0-9]*)` 꼴만 받고 `-0`·앞자리 0·`+`·공백·지수 표기는 `type_mismatch` 다.
- 범위를 넘는 값(토큰·문자열)과 |값| ≥ 2^63 인 정수형 실수는 `out_of_range`, 소수(`1.5`, `"1.5"`, `42.0`)는 `not_integer` 다.
- 기본값도 같은 경로로 내부 표현이 된다.
- `json` 타입 값은 무타입 JSON 이라 안의 숫자를 JSON 숫자 그대로 쓴다. 그래서 2^53 을 넘는 정수는 JavaScript 도구에서 정밀도를 잃을 수 있다. 정밀도가 필요하면 그 값을 `int64` 로 선언한다.
- v1 투영 `result.output` 은 무타입 JSON 이라 메모리에서도 wire 형식(int64 는 문자열)으로 둔다.
- float64 는 f64 로 정확히 표현되는 정수 토큰만 받는다(9007199254740993 은 거절). JSON 에는 NaN·Infinity 를 쓸 수 없어 IPC 요청은 파싱 단계에서 `-32700` 이 되고(`1e400` 처럼 f64 범위를 넘는 수도 같다) 결과가 기록되지 않는다. Rust API 에서도 serde_json 이 비유한 f64 를 null 로 바꾸므로 `null_not_allowed`(nullable 이면 null)가 된다. `not_finite` 는 이런 값이 `Value` 숫자로 들어오는 경우를 막는 방어 검사로, 현재 경로에서는 도달하지 않는다.

### 종류별 기본 출력과 결과

| command | 기본 출력 타입 | 출력 값 |
|---|---|---|
| `run` | `int64`(후처리가 없으면 다시 선언 불가) | 종료 코드(wire 에서는 `"0"` 같은 문자열). `allowed_exit_codes`(기본 `[0]`)에 든 코드면 성공. stdout·stderr 는 `raw.execution`, 숫자 종료 코드는 `raw.exit_code` |
| `custom` | `json` | IPC 응답(최종 응답). 완료를 따로 기다리면 dispatch 응답은 `raw.accepted` 에 따로 둔다(아래) |
| `wait_barrier` | `unit`(다시 선언 불가) | 확정된 null |
| `reduce` | 전략별(아래) | reducer 값 |
| `agent` | `string` | 턴의 최종 답변. string 이 아닌 출력은 명시 제출 값(아래 §agent task) |

결과는 `typed_result` 에 저장한다: `has_output`·`output`(최종 출력), `raw`(`exit_code`, `execution`, 후처리가 있으면 `postprocess`, 완료를 따로 기다린 custom 이면 `accepted`), `error`(`stage`: `input`·`execution`·`postprocess`·`output_validation`·`persistence`·`route`, agent task 는 `code` 도 싣는다), `provenance`(`contract_version`, `kind`, `output_source`). `has_output: true` 이고 `output: null` 이면 unit 또는 nullable 출력이 확정된 것이고, `has_output: false` 는 출력이 없다는 뜻이다. v1 호환을 위해 `result` 에는 최종 출력이 `output` 으로 투영된다.

완료를 따로 기다리는 custom(`poll` 이 있거나 메서드에 기본 완료 전략이 있는 경우)은 dispatch 응답을 접수 응답으로 보고 저장한다. 레코드에는 시점마다 한 곳에만 있다.

| 시점 | 위치 |
|---|---|
| dispatch 뒤 결과 확정 전(실행 중·후처리 중, `task_get`·`task_list` 의 Running task) | task 의 `accepted` |
| 결과 확정 뒤(성공·실패·후처리 확정, 결과 없이 실패로 끝난 경우 포함) | `typed_result.raw.accepted`. task 의 `accepted` 는 비운다 |
| 결과 불명(`unknown`) | 결과를 확정하지 않으므로 task 의 `accepted` 에 남는다 |
| `retry` 뒤 | 어디에도 없다 |

`raw.execution` 은 완료를 알린 응답(poll 이면 마지막 poll 응답)이다. 응답으로 바로 끝나는 custom 은 그 응답이 출력이라 `accepted` 가 없다.

- 형식은 `{"response": <응답>}` 이다. 직렬화한 JSON 이 64 KiB(`ACCEPTED_RESPONSE_CAP`, Run 출력 한 줄기의 상한과 같다)를 넘으면 `{"text": <JSON 앞부분>, "truncated": true, "dropped_bytes": <버린 바이트 수>}` 로 둔다. 자르는 자리는 UTF-8 문자 경계다. 잘린 `text` 는 JSON 으로 읽을 수 없고 출력 검증에 쓰지 않는다.
- v1 task 에는 저장하지 않는다.
- 저장에 실패하면 경고 로그를 남기고 실행은 계속한다. 요청은 이미 실행됐기 때문이다.

custom 의 최종 응답과 agent 의 실행 보고(`raw.execution`)에도 같은 상한을 둔다. 출력은 `typed_result.output` 에 따로 저장하므로 raw 를 잘라도 출력은 바뀌지 않는다. 레코드에는 출력이 두 벌(`typed_result.output` 과 v1 투영 `result.output`) 있고 각각 출력 값 상한 안이라, raw 를 상한으로 막으면 레코드가 memory 값 상한(1 MiB) 안에 든다. agent 의 최종 답변은 출력과 별개로 길이 제한이 없어, 막지 않으면 유효한 제출이 있어도 레코드를 저장하지 못했다.

- 직렬화한 응답이 64 KiB(`EXECUTION_RESPONSE_CAP`, 접수 응답과 같다)를 넘으면 `raw.execution` 을 비우고 `raw.execution_truncated` 에 `{"text": <JSON 앞부분>, "truncated": true, "dropped_bytes": <버린 바이트 수>}` 를 둔다. 자르는 자리는 UTF-8 문자 경계다. 상한 안이면 `execution_truncated` 는 없다.
- 응답이 출력이 되는 custom 과 최종 답변·제출이 출력이 되는 agent 는 출력 값 상한(256 KiB, `MAX_VALUE_BYTES`)을 넘으면 `output_validation` 실패로 끝난다. raw 는 잘린 앞부분만 남으므로 응답이 memory 값 상한(1 MiB)보다 커도 결과를 저장한다. agent 의 출력이 제출이면 최종 답변이 아무리 길어도 출력은 그대로다.
- 후처리가 있는 custom·agent 는 응답(agent 는 실행 보고)을 회차에 저장해 후처리 입력(`raw.execution`)으로 쓴다. 입력은 자를 수 없으므로 응답이 출력 값 상한을 넘으면 후처리를 실행하지 않고 같은 `output_validation` 실패로 끝낸다.
- 다른 종류의 `raw.execution` 은 자르지 않는다. run 은 실행기가 stdout·stderr 를 각 64 KiB 꼬리로 모은다.

보고된 결과는 계약에 맞춰 확정하고, 유효한 출력 없이 성공으로 가려는 v2 task 는 Failed 로 끝낸다. 러너·재시작 복구·훅 완료·훅 만료·IPC `task_set_result` 가 모두 같은 완료 경로(아래 §실행 회차와 완료)를 지나므로 완료 경로마다 따로 검사하지 않는다. 비즈니스 값(`"revise"`, `false`)은 정상 출력이다. 출력 타입에 맞지 않는 값만 `output_validation` 실패가 된다. `retry` 는 `typed_result` 를 지운다.

### 실행 회차와 완료

v2 task 는 Running 이 될 때마다 새 회차(`attempt`: `id` 는 `<task id>#<번호>`, `number`, `started_at`)를 받는다. `retry` 는 회차를 지우지 않으므로 다음 실행은 번호를 이어 간다. v1 task 에는 회차가 없다.

v2 task 의 fallback 이 이미 실행됐으면(Ready·Running·Succeeded) 그 task 의 `retry` 는 `-32602` 로 거절한다. 본 작업이 다시 성공하면 `one_of` 소비자가 성공한 원본 둘을 보게 되고(input 단계 실패), 이미 끝난 fallback 의 결과와 전파를 되돌릴 수 없기 때문이다. 다시 실행하려면 새 task 로 제출한다. fallback 이 실패했거나 실행 전에 끝났으면(Failed·Skipped·Cancelled) 재시도할 수 있다. v2 task 의 `retry` 는 `reset_downstream: true` 도 `-32602` 로 거절한다. 하류는 이전 회차의 실패 전파나 경로 선택으로 이미 판정됐고, 되감으면 두 회차의 판단이 섞이기 때문이다(근거 docs/dev-guide/agent-runner.md). 재시도는 그 task 만 새 회차로 다시 실행하며 저장된 경로(`route`)와 skip 이유를 지운다. 경로가 선택되지 않아 건너뛴(`branch_not_selected`) v2 task 는 다시 판정해도 선택되지 않으면 `retry` 를 `-32602` 로 거절한다(메시지에 `not selected`). 받아들이면 곧장 같은 이유로 건너뛰어 아무것도 실행하지 않은 채 성공 응답만 남기 때문이다. v1 task 는 이 제한이 없다.

완료 보고(`Completion`: 회차 id·결과·성공/실패)는 저장소의 `complete` 하나로 기록한다(근거 docs/dev-guide/agent-runner.md).

- v2 는 결과 확정(출력 검증 포함)과 종결 상태, 성공했을 때 고른 경로(`route`, 아래 §전이 조건과 경로 선택)를 레코드 한 번의 쓰기로 저장한다. 쓰기가 실패하면 상태·결과가 그대로이고 하류 readiness·fallback 도 움직이지 않는다. 그 쓰기가 끝난 뒤에야 하류를 평가하고 대기자에게 종결을 알린다.
- 보고의 회차가 지금 회차와 다르면 적용하지 않는다(`stale_attempt`). 회차 id 를 생략하면 지금 회차로 본다.
- 이미 끝난 회차에 같은 내용(결과·종결 종류)의 보고가 다시 오면 같은 레코드를 `duplicate: true` 로 돌려주고 하류 반영만 다시 시도한다. 다른 내용이면 거절한다(`different_report`). 회차를 끝낸 보고의 지문(결과와 종결 종류의 FNV-1a 64 해시)은 `attempt.completion.digest` 에 남는다. 보고 없이 끝난 task(취소·건너뜀)에 온 보고는 `already_terminal` 로 거절한다.
- 거절은 IPC 에서 `-32018` 이고 `error.data` 에 `reason`·`attempt_id`(보고한 회차)·`current_attempt_id` 를 싣는다.
- 러너는 기록하지 못한 보고를 보관하고 다음 tick 에 같은 보고를 다시 낸다. 그동안 그 task 를 다시 poll 하지 않고 handle 과 permit(세마포어·lease)을 유지한다. 거절된 보고는 다시 내지 않는다.
- 결과 불명 응답은 성공으로 바꾸지 않는다. v1·v2 완료, IPC 전이, 일회성 훅 완료는 뒤따르는 강한 읽기로 영속 종결 의무를 확인한 다음 timeout 0 대기자와 사건 피드에 전달한다. 재시작 복구는 남은 의무와 복구 중 새로 생긴 의무를 같은 방식으로 처리한다. 이미 종결된 하류도 자체 의무가 있어 재보고가 반환한 전이 목록에 의존하지 않는다. 러너의 v2 동일 보고 재전송과 `after_unanswered` 판정은 유지하며, 알림 전달은 고유 ID로 중복을 막는다. 명령·외부 부수효과를 이 확인 때문에 다시 실행하지 않는다.
- 종결 의무는 [저장 계약](../design/systems/storage-contract.md#task-종결-알림의-영속-의무)의 additive schema를 따른다. 전달 뒤 정확한 의무 ID를 확인 처리한다. 같은 허브는 같은 ID를 한 번만 발행하며 별도 회차는 v1도 새 ID를 쓴다. 전달과 확인 사이에 프로세스가 종료되면 재시작 후 같은 `terminal_id`가 재발행될 수 있다. 외부 소비자는 이 ID로 재전달을 구분한다. 피드는 휘발성 bounded 큐이며 외부 수신 확인은 보장하지 않는다.
- 확인 읽기가 실패하거나 알 수 없는 schema를 만나면 알리지 않고 원본을 보존한다. 다음 task 변경의 확인 읽기 또는 부팅·러너 시작 때 다시 읽는다. 주기적으로 무조건 발행하지 않는다. 과거 회차의 의무는 과거 사건으로 전달하되 현재 task가 실행 중이면 그 대기자를 깨우지 않는다.
- 레코드가 memory 값 상한을 넘어 기록하지 못한 보고(`ValueTooLarge`)는 다시 내도 같은 크기라 보관하지 않는다. 출력을 싣거나 사유가 4 KiB(`SHRUNK_ERROR_LIMIT`)를 넘는 보고면 출력을 뺀 같은 회차의 실패(`<원래 사유의 첫 줄> (the full result could not be stored: <이유>)`, 종료 코드는 유지)로 바꿔 한 번 기록한다. 바꾼 사유는 4 KiB 안이다. 첫 줄이 길면 UTF-8 문자 경계에서 자르고 `...(truncated)` 를 붙인다. 그래서 바꾼 보고는 다시 바꿀 대상이 아니고, 그것마저 기록하지 못하면 다른 저장 오류처럼 보관해 다음 tick 에 다시 낸다. 재시작 복구의 보고도 같다. 후처리 실행 보고는 회차 진행과 맞도록 같은 실행 번호·종료 코드를 유지한 채 두 단계로 줄인다. 성공 보고는 먼저 진단용 본 작업 결과 사본(회차 진행의 `execution`, 그로 만드는 `raw.execution`·`raw.execution_truncated`)만 비운 성공으로 다시 기록한다. 그래도 넘치거나 실패 보고면 `result_too_large` 실패(`postprocess result_too_large: the postprocess result could not be stored: <이유>`, stdout·stderr 는 버리고 같은 사본도 비운다)로 바꾼다. 이 실패는 재시도가 남아도 다시 실행하지 않고 그 보고로 회차를 확정한다. 이미 바꾼 실패는 다시 바꾸지 않으므로 재기록은 많아야 두 번이다. 사유가 없는 성공 보고는 `the result could not be stored: <이유>` 가 된다. 계약 없는(v1) custom 은 응답이 곧 출력이라 자르지 않고 레코드에 한 벌 두므로, 직렬화가 약 1 MiB(레코드의 나머지 필드 몫을 뺀 크기)를 넘는 응답은 이 경로로 출력 없는 실패가 된다.

| 후처리 보고 | 처음 기록이 넘치면 | 그래도 넘치면 |
|---|---|---|
| 성공(`Collected`) | 본 작업 사본만 비운 성공. 정의 760 KiB·본 작업 custom 응답 240 KiB·출력 `"ok"` 는 1,107,029 B 로 넘쳤다가 795,606 B 의 Succeeded 로 기록됐다(시험 `a_collected_report_drops_the_main_result_copy_and_still_succeeds`) | `result_too_large` 실패. 정의 300 KiB·stdout 250 KiB(pointer 사용)는 1,113,107 B → 1,092,601 B → 308,563 B 의 Failed 였다 |
| 실패 | `result_too_large` 실패 | — |

- 저장소가 계속 실패하면 permit 을 쥐는 시간에 상한이 없다. 재시도 횟수나 시간으로 포기하지 않는다. 포기하면 결과가 기록되지 않은 채 Running 인 task 의 permit 을 풀어 같은 자원을 다른 task 에 넘기게 되기 때문이다. 묶이는 permit 은 보고가 보류된 task 마다 하나다. 줄인 보고마저 기록하지 못하는 경우(레코드의 다른 필드가 커서 출력을 빼도 memory 값 상한을 넘는 경우)도 같다. 그 보고를 보관해 다시 내는 동안 task 는 Running 이고 permit 을 쥔다. 새로 만드는 task 는 아래 §task 레코드 크기의 상한이 이 경우를 실행 전에 막는다. 남는 경우는 상한이 생기기 전에 저장돼 이미 Running 인 task, 결과 몫을 남기지 못할 만큼 memory 값 상한이 작은 경우(아래 표)다. 풀리는 시점은 셋이다.
  1. 저장이 회복돼 같은 보고가 기록되거나 거절될 때.
  2. 러너를 멈춘 뒤 다음 러너 시작·부팅의 정리가 Running task 를 마무리할 때. 보류됐던 보고는 메모리에만 있어 사라진다. Run 은 저장된 실행 결과로 확정하거나(없으면 `unknown`) 그때 점유를 반환한다. 그 밖의 task 는 `purge_stale_semaphore_holders`·`purge_stale_lease_holders` 가 점유를 회수하고 Failed 로 끝낸다.
  3. task 가 밖에서 종결됐을 때(취소 등). 다음 tick 의 종결 흡수가 보류 보고를 한 번 더 내고, 그 보고가 이미 끝난 task 라 거절되면 permit 을 푼다. 저장소가 여전히 실패하면 이 경우에도 계속 쥔다.
- 재시작 복구(죽은 pid, 저장된 실행 결과, 기한이 지난 외부 완료 대기)는 handle 레코드에 함께 저장한 dispatch 회차 id(`attempt_id` 키)로 보고한다. 재시도 뒤 남은 옛 회차의 handle 은 `stale_attempt` 로 거절돼 지워지고 새 회차를 끝내지 않는다. 회차 id 가 없는 레코드(v1 task, 회차 저장 전에 만든 handle)는 지금 회차로 보고한다. 기록에 실패하면 handle 을 남겨 다음 reload 가 다시 보고한다.
- 완료 쓰기는 끝났지만 하류 반영 중에 실패하면 오류를 돌려준다. 러너는 같은 보고를 다시 내 하류 반영을 마치고, 다시 낼 보고가 없는 재시작 뒤에는 부팅·러너 시작의 readiness 재평가가 마친다.
- push 완료 전략의 훅 대기는 dispatch 한 회차 id 를 함께 저장한다. 늦게 온 훅이나 만료가 다음 회차를 끝내지 않는다.
- v1 task 는 결과를 쓴 뒤 상태를 전이한다. 상태 전이가 맞지 않는 보고는 결과를 쓰기 전에 거절한다.

| | v1 task | v2 task |
|---|---|---|
| 기록 | 결과 쓰기와 상태 전이, 두 번의 쓰기 | 결과 확정·종결 상태·완료 지문을 한 번의 쓰기 |
| 두 쓰기 사이 실패 | 결과만 남은 Running task 가 될 수 있다. 같은 보고를 다시 내면 결과를 덮어쓰고 전이한다 | 해당 없음. 쓰기가 실패하면 아무것도 바뀌지 않는다 |
| 회차 | 없음 | Running 전이마다 `<id>#<번호>` |
| 같은 보고 재전송 | 이미 종결이면 전이 오류(`-32602`) | `duplicate: true` 로 같은 응답 |
| 다른 회차·다른 내용 보고 | 구별하지 않는다 | `-32018` 거절 |

Running 인 v2 task 는 `agent.task_get` 과 `agent.task_list` 의 각 task 에 세부 단계 `phase` 를 싣는다(`Task::phase`). `postprocessing`·`retry_wait` 는 후처리(아래 §후처리 CLI), `awaiting_input` 은 입력을 기다리는 agent task(아래 §agent task), 그 밖은 `executing` 이다. Running 이 아니거나 v1 task 면 없다. 예외로 handle 이 남은 Ready task(v1 포함)는 `waiting_previous_attempt` 다(`tasty_task_runtime::task::task_phase`). 이전 회차의 프로세스가 끝난 것을 확인할 때까지 러너가 시작하지 않고 있다는 뜻이다(위 §dispatch 게이트 아래 취소·정리 설명). `task_get` 은 그 task 의 handle 키 하나만, `task_list` 는 handle 목록을 한 번 읽어 판정한다. 목록 읽기는 key 의 `LIKE` 접두사 검색이라 워크스페이스의 레코드 수에 비례한다. 완료는 한 번의 쓰기라 출력 검증·저장 중인 단계(`validating`)는 바깥에서 관측되지 않아 두지 않는다(docs/dev-guide/agent-runner.md). CLI `task-list` 는 줄 끝에 `(phase)` 를, `task-get` 은 `phase:` 줄을 보인다.

단계가 바뀔 때 사건은 발행하지 않는다. task 사건은 종결(`agent.task_finished`)과 barrier 닫힘뿐이다(이 문서). 진행 신호가 필요하면 DAG 에 신호를 보내는 task 를 끼운다. 신호 task 는 앞 task 가 끝난 뒤 실행되므로 그 경계의 신호가 된다. 실행 중 단계는 `task_get` 으로 조회한다.

```json
{"id": "built", "command": {"kind": "run", "command": ["tasty", "notify", "build finished", "--title", "release DAG"]},
 "depends_on": ["build"]}
```

- run 자식은 바깥 Tasty 의 신원 변수를 받지 않으므로 `tasty` 는 이 Tasty 에 닿는다([runner 자식의 환경](#runner-자식의-환경)).
- 외부로 알리려면 같은 자리에서 webhook 을 부른다(예: `["curl", "-fsS", "-X", "POST", "https://…"]`).
- 앞 task 가 실패해도 신호를 보내려면 앞 task 에 `on_failure: continue_downstream` 을 둔다.

v1 이 v2 결과를 읽는 경로는 `-32602`(`error.data.task_id` 에 참조 대상)로 거절한다. 대상은 v1 출력 placeholder(`${task.<id>.output…}`)로 v2 task 를 참조하는 생성, `inputs` 에 v2 task 가 든 v1 `Reduce` 생성, v2 task 를 입력으로 준 단발 `agent.task_reduce` 다. 이미 저장된 v1 task 가 v2 를 가리키면 실행 직전 치환·reduce 수집이 실패로 끝낸다. 허용 범위는 입력 binding 이 정한다.

inline fallback 은 v2 에서 거절한다.

### 후처리 CLI (`postprocess`)

run·custom task 는 본 작업 뒤 CLI 하나를 실행해 그 stdout 을 최종 출력으로 삼을 수 있다. 모델 접속·인증·질문 작성은 CLI 의 일이고, Tasty 는 명령 실행·입출력·타입 검증·실패 처리만 한다. 인증은 기존 환경을 쓴다.

```json
{"id": "judge",
 "command": {"kind": "run", "workspace_id": 1, "command": ["make-draft"]},
 "output_schema": {"type": "enum", "values": ["pass", "revise"]},
 "postprocess": {
   "command": ["judge-cli", "--json"],
   "cwd": "/work",
   "stdin": {"draft": {"from": "raw", "pointer": "/execution/stdout/text"},
             "request": {"from": "input"}},
   "stdout": {"format": "json", "pointer": "/verdict"},
   "timeout_ms": 120000,
   "retry": {"max_retries": 1, "delay_ms": 5000}}}
```

- `command`: 실행 파일과 인자. 셸을 거치지 않고 직접 실행한다. 셸이 필요하면 `["sh", "-c", ...]` 처럼 셸을 명시한다. 입력 값은 명령 문자열에 끼워 넣지 않고 stdin 으로만 간다. TTY 가 없는 CLI 만 지원한다.
- 환경변수: `Run` 과 같다(위 "runner 자식의 환경"). 바깥 Claude Code 세션의 표지·비밀과 바깥 Tasty 인스턴스의 신원 변수 네 개를 지우고 나머지는 넘긴다.
- `cwd`: 생략하면 run 의 `cwd`, 그것도 없으면 호스트 프로세스의 디렉터리.
- `stdin`: stdin 에 쓸 JSON object 의 필드별 출처. `from` 은 `input`(이 회차의 입력 snapshot, wire 형식), `raw`(본 작업 원본 `{exit_code?, execution?, accepted?}`) 이고 `pointer` 로 그 안의 위치를 고른다. 위치에 값이 없으면 실행하지 않고 `stdin_mapping` 실패다. 예전 출처 `artifacts` 는 없어졌다. 제출하면 `/postprocess/stdin/<필드>/from` 위치로 거절하고, 이미 저장된 task 에서는 빈 배열로 읽는다. 생략하면 `{}` 를 쓴다. 문서 하나를 쓰고 stdin 을 닫는다.
- `stdout.format`: `json`(기본)은 JSON 값 정확히 하나, `text` 는 UTF-8 문자열 그대로. json 형식이 실패해도 text 로 바꾸지 않는다. `stdout.pointer` 는 json 형식에서만 쓰며 그 위치의 값을 출력 후보로 고른다. 생략하면 값 전체다.
- `timeout_ms`: 필수, 1 ~ 86400000. 기본값을 두지 않는다(무기한 대기를 받지 않는 이유는 아래 상한 근거). 프로세스 종료와 stdin 쓰기, 상속된 stdout·stderr 파이프의 EOF 까지 포함한다.
- `retry`: 생략하면 재시도하지 않는다. `max_retries` 1 ~ 10, `delay_ms` 0 ~ 3600000.
- 출력 스키마를 생략하면 json 형식은 `json`, text 형식은 `string` 이다. run 의 int64 출력 제한은 후처리가 있으면 적용하지 않는다. reduce·wait_barrier 에는 둘 수 없다.

실행 순서:

1. 본 작업이 성공하면 task 는 Running 으로 남고 회차의 `attempt.postprocess` 에 본 작업 결과(`execution`)와 진행(`phase`)이 기록된다. 본 작업이 실패하면 후처리 없이 끝난다.
2. 호스트는 `phase` 를 `started` 로 기록한 뒤 프로세스를 띄운다. 러너는 본 작업이 끝난 tick 에 첫 실행을 시작한다.
3. 실행 보고가 재시도 대상 실패이고 횟수가 남았으면 `phase` 는 `pending`(다음 `run`, `not_before_ms`)이 되고 task 는 Running 이다. 아니면 그 보고로 결과를 확정하고 종결한다. 종결·`on_failure`·하류 반영은 이때 한 번 일어난다.
4. 세마포어·lease 는 본 작업부터 마지막 종결까지 쥔다. 다음 task 는 후처리가 끝난 뒤에야 Ready 가 된다.

재시도 예산을 다 쓴 후처리 실패는 task 의 실패로 끝난다. 후처리만 다시 실행하는 수동 재시도는 없다. 그 task 의 하류는 일반 실패와 같이 실패 정책을 따른다(`depends_on` 하류는 `upstream_unavailable` 로 건너뛰고, `on_failure` fallback 이 있으면 실행한다). 다른 갈래는 계속 진행하고, DAG 요약은 진행할 작업이 남아 있는 동안 진행 상태다. 끝까지 진행한 뒤 끝까지 성공한 다른 갈래가 있으면 `partially_failed`, 없으면 `failed` 다. 다시 실행하려면 `retry` 로 본 작업부터 새 회차를 연다.

`agent.task_get` 은 후처리 단계의 Running task 에 `phase`(`postprocessing`, 재시도 대기 중이면 `retry_wait`)를 싣는다. task 가 끝나면 `attempt.postprocess.phase` 는 `finished` 다. 보고로 확정했으면 그 실행 번호, 취소 등으로 먼저 끝났으면 마지막으로 시작한 실행 번호(없으면 0)를 `run` 에 둔다.

CLI `tasty agent task-get` 은 같은 정보를 줄로 보인다(`--json` 을 주면 `agent.task_get` 응답을 그대로 JSON 으로 낸다. `task-list --json` 도 같다). 진행 중에는 `state` 다음 줄에 단계와 실행 번호를, 끝난 뒤에는 마지막 실행의 결과와 재시도로 넘어간 실행의 원인을 보인다.

```text
state: running
phase: retry_wait (run 2)
```

```text
state: failed (postprocess nonzero_exit: exited with code 3)
postprocess: run 2 failed (nonzero_exit), exit_code 3
postprocess retried after: run 1 nonzero_exit, exit_code 3
```

stdout 해석과 성공 판정:

- 성공은 종료 코드 0 이고 stdout 이 형식에 맞으며 출력 후보가 출력 타입에 맞을 때다. `false`·`0`·`null`·`"revise"` 같은 값은 비즈니스 결과이며 실패가 아니다. `null` 은 출력 타입이 unit 이거나 nullable 일 때만 유효하다.
- stderr 는 진단 로그로만 쓰고 출력에 섞지 않는다. 마지막 16 KiB 만 남긴다.
- stdout 은 256 KiB 까지 모은다. 넘으면 앞부분이 온전한 JSON 이어도 실패한다(`stdout_too_large`). 상한은 task 레코드가 memory 값 상한(1 MiB) 안에 들도록 정했다.

실패 원인(`raw.postprocess.cause`, 오류 메시지는 `postprocess <원인>: ...`):

| 원인 | 뜻 | 재시도 |
|---|---|---|
| `spawn` | 실행 파일을 시작하지 못함(없는 실행 파일 등) | 예 |
| `stdin_write` | stdin 을 쓰지 못함. 읽지 않고 닫은 경우는 실패가 아니다 | 예 |
| `nonzero_exit` | 0 이 아닌 종료 코드 | 예 |
| `signal` | 숫자 종료 코드 없이 끝남 | 예 |
| `timeout` | 제한 시간 안에 종료·출력 EOF 가 오지 않음 | 예 |
| `stdout_too_large` · `stdout_read` · `invalid_utf8` | stdout 수집 실패 | 예 |
| `invalid_json` · `multiple_documents` · `empty_output` · `pointer_missing` | json 형식 해석 실패 | 예 |
| `stdin_mapping` | stdin 출처의 위치에 값이 없음 | 아니요 |
| `cancelled` | task 취소 또는 러너 정지로 중단 | 아니요 |
| `outcome_unknown` | 시작했지만 결과를 받기 전에 호스트가 재시작함 | 아니요 |
| `result_too_large` | 실행 보고를 실은 레코드가 memory 값 상한을 넘어 기록하지 못함(성공 보고는 본 작업 사본을 비워도 넘칠 때) | 아니요 |

출력 후보가 출력 타입에 맞지 않으면 `output_validation` 실패이고 재시도하지 않는다.

재시도하지 않는 이유: `output_validation` 은 CLI 와 계약이 어긋난 것이라 같은 입력으로 다시 실행해도 고쳐질 근거가 없다. `stdin_mapping` 은 재시도도 같은 본 작업 결과를 쓰므로 바뀌지 않는다. `cancelled` 는 사용자·러너의 중단 의도이고, `outcome_unknown` 은 다시 실행하면 같은 후처리가 두 번 실행될 수 있다. `result_too_large` 는 같은 본 작업 결과로 다시 실행해도 보고 크기가 줄어들 근거가 없다. 상한의 근거:

- stdout 256 KiB(`MAX_POSTPROCESS_STDOUT_BYTES`): 출력은 task 레코드의 `typed_result.output` 과 v1 투영 `result.output` 에 두 번 들어가고, pointer 를 쓰면 `raw.postprocess.stdout` 에도 들어간다. 레코드 하나가 memory 값 상한 1 MiB 안에 남도록 그 3분의 1 아래로 잡았다.
- stderr 16 KiB tail(`POSTPROCESS_STDERR_TAIL_BYTES`): 진단용이며 같은 레코드에 들어간다. 마지막 실행의 것만 남긴다.
- `timeout_ms` 24시간(`MAX_POSTPROCESS_TIMEOUT_MS`): 무기한 대기는 받지 않는다. 모델 호출 같은 긴 작업을 담되 permit 을 하루 넘게 쥐지 않게 한다.
- 재시도 10번(`MAX_POSTPROCESS_RETRIES`)·대기 1시간(`MAX_POSTPROCESS_RETRY_DELAY_MS`): 재시도 동안 permit 을 쥐므로 횟수와 대기를 유한하게 묶는다. 재시도로 넘어간 실행의 요약도 레코드에 쌓인다.
- 정상 종료 대기(executor 3초 = 그룹 종료 뒤 파이프 EOF 를 기다리는 2초 + 저장 시간, runner registry 4초): 멈춘 runner 가 앱 종료를 막지 않게 한다.

결과:

- `raw.exit_code`·`raw.execution` 은 본 작업 원본 그대로다(custom 응답의 raw 상한은 위 §결과 확정과 같다). 예외로, 레코드가 넘쳐 줄인 보고(본 작업 사본을 비운 성공, `result_too_large` 실패)면 `raw.execution`·`raw.execution_truncated` 가 비어 있다. `raw.postprocess` 에 `command`, 회차 안의 실행 번호 `run`, `exit_code`, `stderr`(tail)·`stderr_truncated`, 실패면 `cause`, pointer 를 썼으면 stdout 전체(`stdout`), 재시도로 넘어간 앞선 실행(`failed_runs`: 번호·원인·종료 코드·메시지)이 있다.
- `provenance.output_source` 는 `postprocess.stdout.json` 또는 `postprocess.stdout.text` 다. 모델 이름 같은 메타데이터는 CLI 가 stdout 에 담았을 때만 남는다.

프로세스 소유와 재시작:

- 실행·출력 수집은 작업 스레드에서 하며 저장소 잠금은 시작 기록과 결과 저장에만 잡는다. stdin 쓰기와 stdout·stderr 읽기는 서로 다른 스레드라 stdin 을 읽지 않는 CLI 나 출력이 큰 CLI 도 막히지 않는다.
- 시간 초과·취소는 그 실행이 만든 프로세스 그룹(Unix, 자식을 새 그룹 리더로 띄운다)이나 job(Windows)만 종료한다. 직접 자식이 끝난 뒤에도 자손이 파이프를 쥐고 있으면 EOF 를 기다리다 시간 초과로 그룹을 종료한다. Linux 는 끝난 리더를 회수하지 않고 관찰해 그룹을 종료할 때까지 그룹 id 가 재사용되지 않는다. 다른 Unix 는 리더를 회수한 뒤에는 그룹을 종료하지 않는다. 스스로 새 세션·그룹으로 옮긴 프로세스나 Windows 에서 job 에 넣기 전에 만든 프로세스는 종료 대상에 들지 않는다.
- 취소된 task 의 permit 은 실행이 끝난 것을 확인한 다음 tick 에 놓는다.
- 각 실행 보고는 `tasty.agent.postprocess_result.<task id>` 에 회차 id 와 함께 저장한다. 재시작하면 `phase` 로 복원한다. `pending` 은 점유를 쥔 채 예약대로 실행하고, `started` 는 같은 회차·번호의 저장된 보고가 있으면 그것으로 확정한다. 없으면 저장한 handle 의 PID·시작 시각(`started_at`)이 가리키는 같은 프로세스가 아직 살아 있는지 본다. 살아 있으면 그 handle 로 기다리며 task 는 Running, semaphore·lease 는 그대로 쥔다(재시작한 호스트는 출력 파이프를 읽을 수 없으니 기다리는 것은 끝뿐이다). 끝나면 `outcome_unknown`(`… ended after a host restart and its result could not be collected …`)으로 끝내고 점유를 반환한다. 살아 있지 않거나 시작 시각이 없는 handle 이면 reload 가 바로 `outcome_unknown` 으로 확정하고 점유를 반환한다(Run 의 결과 불명과 같다). 점유를 쥐었다고 `host restart` 실패가 되지 않는다. 결과 불명인 실행은 재시도 예산이 남아도 다시 실행하지 않는다. 다시 실행하려면 `retry` 로 새 회차를 연다(본 작업부터 실행한다).
- 러너가 멈추면(명시 stop·workspace 정리·앱 종료) 진행 중인 후처리를 `cancelled` 로 중단하고 보고를 저장한다. task 상태는 바꾸지 않으므로 다음 러너 시작이나 재시작까지 Running(`postprocessing`)으로 보인다. 그때 이 보고로 실패하며 재시도 예산이 남아도 다시 실행하지 않는다. 보고를 저장하기 전에 호스트가 끝났으면 `outcome_unknown` 이다.
- 정상 종료 때는 그룹 종료와 보고 저장을 기다린다. executor 는 작업 스레드를 최대 3초, runner registry 는 runner 스레드를 최대 4초 기다린 뒤 경고하고 돌아간다. 보통은 수십 ms 안에 끝난다.
- 비정상 종료(SIGTERM 등, Tasty 에는 SIGTERM 처리기가 없다): Linux 는 후처리를 호스트 수명에 묶어(`tasty_reaper::spawn_bound_to_host`, PDEATHSIG) 그룹 리더가 SIGTERM 을 받는다. 그룹의 다른 프로세스는 신호를 받지 않아, 리더가 전달하지 않으면 남는다. Windows 는 실행별 KILL_ON_JOB_CLOSE job 이 호스트 종료와 함께 닫혀 job 안의 프로세스가 끝난다. macOS 는 묶지 않아 그룹 전체가 남는다. 재시작 뒤 리더가 살아 있으면 위처럼 끝날 때까지 점유를 쥐고 기다리며, 그동안 `cancel` 하면 그 그룹을 끝낸다. 리더가 끝난 뒤 남은 그룹 구성원은 정리하지 않는다. 재시작 판정은 리더만 보므로 구성원이 살아 있어도 task 는 `outcome_unknown` 으로 끝난다. 예를 들어 `["sh", "-c", "sleep 300; echo x"]` 는 Linux 에서 `sh` 가 SIGTERM 으로 끝나고 `sleep` 이 남는다.

### 입력 binding

입력은 계약의 `bindings`(입력 필드 이름 → binding)로 채운다. binding 이 있으면 `input_schema` 는 object 여야 한다. 필수 필드는 binding 이나 `default` 가 있어야 하고, 둘 다 없으면 생성할 때 거절한다.

| 형식 | 뜻 |
|---|---|
| `{"literal": <값>}` | 고정 값. 생성할 때 필드 타입으로 검사한다 |
| `{"from_task": "<id>", "pointer": "/a/b", "convert": ...}` | 다른 task 의 최종 출력에서 JSON Pointer 위치의 값. `pointer` 를 생략하면 출력 전체 |
| `{"one_of": [{"from_task": ..., "pointer": ...}, ...], "convert": ...}` | 성공한 원본 하나의 값. 본 작업과 fallback 중 실행된 쪽을 받을 때 쓴다 |

- 원본 task 는 같은 그래프나 같은 workspace 의 v2 task 여야 한다. binding 은 의존성이기도 해서 원본이 끝나기 전에는 실행되지 않고, 삭제 보호·DAG 묶음·순환 검사·`task_graph` 의 `binding` 간선에 포함된다.
- 생성할 때 원본의 출력 타입에서 포인터 위치의 타입을 구해 필드 타입에 대입 가능한지 검사한다. 암묵 변환은 없다. `convert` 로만 바꾼다: `to_string`(int64·boolean·enum → 10진·`true`/`false`·값 문자열), `int64_to_float64`(정확히 표현되는 값만, 아니면 `out_of_range`), `assert`(값을 바꾸지 않고 실행 시 입력 스키마로 검사한다. `json` 출력 안을 가리킬 때 쓴다).
- 포인터가 optional 필드(기본값 없음)를 지나면 값이 없을 수 있다. 그러면 대상 필드도 optional 이거나 기본값이 있어야 한다. list 와 json 안의 위치는 실행 시 확인한다.
- `from_task` 는 그 원본 자신이 성공해야 한다. 원본이 실패하면 받는 task 는 건너뛴다(fallback 이 대신 성공해도 마찬가지). 원본이 경로 선택에서 빠졌으면(`branch_not_selected`) optional·기본값 필드는 비워 두고 필수 필드는 받는 task 를 건너뛴다. `one_of` 는 모든 원본이 끝날 때까지 기다리고 성공한 원본의 값을 쓴다. 성공한 원본이 없으면 필수 필드는 task 를 건너뛰고 optional 필드는 비워 둔다. 성공한 원본이 둘 이상이면 실행 시 input 단계에서 실패한다.
- `on_failure: continue_downstream` 과 `from_task` binding 은 함께 쓸 수 없다(값이 없는데 실행하게 된다). fallback 을 가진 task 를 `depends_on` 으로 기다리면서 그 task 만 `from_task` 로 받으면 거절하고 `one_of` 를 쓰라고 안내한다. v2 task 의 fallback 은 v2 task 여야 하고, fallback 이 실행된 v2 task 의 하위 작업은 대기하지 않고 건너뛴다.

#### 실행할 때

러너는 dispatch 직전에(lease 와 v1 placeholder 치환 뒤) binding 을 해석해 입력 값을 만들고 입력 스키마로 검사한다. 결과는 task 의 `input_snapshot` 에 저장한다: `resolved_at`, `value`(선언 타입대로 직렬화), `sources`(읽은 원본마다 필드·task·포인터와 값을 낸 원본 회차 `producer_attempt`), `execution`(실제로 넘긴 argv 요소·stdin 여부·custom params·agent 지시문·wait_barrier 의 barrier 이름), 실패 시 `failure`. 원본을 나중에 다시 실행해도 snapshot 은 바뀌지 않는다. `retry` 는 snapshot 을 지운다. 해석이나 검사에 실패하면 실행하지 않고 실패 단계 `input` 으로 끝난다(`error.location` 은 `/bindings/<필드>`).

입력은 `input_mapping` 이 정한 자리에만 값으로 들어간다. 원본 command 는 바꾸지 않으며 값 안의 `${...}`·`$(...)`·공백을 다시 해석하지 않는다.

| 키 | 대상 | 규칙 |
|---|---|---|
| `args` | `run` | 입력 포인터 목록. 각 값을 argv 끝에 요소 하나로 붙인다. string·enum·int64·boolean 만 받는다(int64 는 10진, boolean 은 `true`/`false`). 원소가 `{"file": "<포인터>"}` 면 그 string 은 이미 있는 파일의 경로다(아래 입력 파일) |
| `stdin` | `run` | `true` 면 입력 전체를 wire 형식 JSON 한 문서로 stdin 에 쓴다(int64 는 10진 문자열). 별도 스레드에서 쓰고 닫으며, 명령이 읽지 않고 끝나도 결과는 종료 코드로 정한다 |
| `params` | `custom` | params 포인터 → 입력 포인터. params 의 그 자리에 값을 넣는다. 부모 object 는 원래 params 에 있어야 한다. 값은 내부 표현이라 int64 가 JSON 정수다. snapshot 의 `execution.params` 도 같은 JSON 정수라, 2^53 을 넘는 값은 JavaScript 같은 f64 소비자가 읽으면 바뀐다(정확한 값은 `input_snapshot.value` 의 10진 문자열) |
| `input_block` | `agent` | `true` 면 입력 전체를 wire 형식 JSON 블록(`Task input (JSON):` 머리말)으로 지시문 끝에 붙인다. snapshot 의 `execution.instruction` 이 실제로 보낸 지시문이다 |
| `barrier` | `wait_barrier` | 기다릴 barrier 이름을 담은 입력 포인터. 그 자리는 string 이어야 한다. snapshot 의 `execution.barrier` 가 받은 이름이다 |

매핑하는 입력 위치는 항상 값이 있어야 한다. `reduce` 는 입력을 받지 않으므로 입력 스키마가 unit 이어야 한다. `wait_barrier` 도 `barrier` 매핑이 없으면 unit 이어야 한다. unit 이 아닌 입력을 받는 `agent` 는 `input_block` 이 필요하다.

#### run 입력 파일

run 은 이미 있는 파일의 경로를 argv 요소로 받을 수 있다. `input_mapping.args` 의 원소를 `{"file": "<입력 포인터>"}` 로 쓴다. 경로는 binding 이 주는 string(`literal` 이나 앞 task 의 출력)이다.

- Tasty 는 파일을 만들거나 지우거나 수명을 관리하지 않는다. 파일을 만드는 일과 정리하는 일은 DAG 를 짜는 쪽이 앞뒤 task 로 넣는다.
- 실행 직전(입력 해석 때) 그 경로가 지금 열리는 일반 파일인지만 본다. 내용은 읽지 않고, argv 에는 받은 경로를 그대로 넣는다. 파일을 여는 것은 프로그램이다.
- 상대 경로는 run 의 `cwd` 기준이다. `cwd` 가 없으면 상대 경로는 받지 않는다. Tasty 의 작업 디렉터리는 DAG 를 짜는 쪽이 알 수 없기 때문이다.
- 없거나, 디렉터리 같은 일반 파일이 아니거나, 열 수 없으면(권한 등) 실행하지 않고 실패 단계 `input`, `error.location` `/input_mapping/args/<i>/file` 로 끝난다. 메시지에 경로와 이유가 있다.
- 심볼릭 링크는 따라가 대상으로 판정하고, argv 에는 링크 경로를 그대로 넣는다. 디렉터리를 가리키는 링크와 끊긴 링크는 위와 같이 input 단계에서 실패한다.
- 확인한 뒤 프로그램이 열기 전에 파일이 사라지면 프로그램의 실패(실행 단계)다.
- 제출할 때 그 입력 위치가 string 이 아니거나 optional 이면 `/tasks/<i>/input_mapping/args/<j>/file` 로 거절한다. 모르는 키는 거절한다.

stdin 으로 파일 내용을 흘리는 방식 대신 경로를 argv 에 넣는다. 프로그램이 경로를 받으면 여러 파일을 받거나 파일 안을 오갈 수 있고(seek), 크기에 상한이 없으며, Tasty 가 내용을 복사하지 않는다. 내용이 stdin 으로 필요하면 프로그램 쪽에서 `sh -c 'my-tool < "$1"' sh` 처럼 바꾼다.

미리 만든 스크립트에서 쓰는 예. 파일은 스크립트가 만들고, 뒤 task 가 지운다.

```sh
#!/bin/sh
# lint-diff.sh <workspace id> — 지금의 diff 를 파일로 두고 검사한 뒤 지운다.
set -eu
diff_file=$(mktemp)
git diff > "$diff_file"
tasty agent task-graph-submit --workspace "$1" --graph "$(cat <<JSON
{"contract_version": 2, "tasks": [
  {"id": "lint", "command": {"kind": "run", "command": ["my-lint", "--diff"]},
   "input_schema": {"type": "object", "fields": {"diff": {"type": "string"}}},
   "bindings": {"diff": {"literal": "$diff_file"}},
   "input_mapping": {"args": [{"file": "/diff"}]},
   "on_failure": {"kind": "continue_downstream"}},
  {"id": "cleanup", "command": {"kind": "run", "command": ["rm", "-f", "$diff_file"]},
   "depends_on": ["lint"]}]}
JSON
)"
```

- `mktemp` 은 절대 경로를 돌려주므로 `cwd` 가 없어도 된다. 경로에 `"`·`\` 가 들어갈 수 있으면 JSON 문자열로 이스케이프해 넣는다.
- `lint` 가 실패해도 `cleanup` 이 돌도록 `continue_downstream` 을 쓴다.

wait_barrier 의 barrier 이름은 command 의 `name`(정적) 이나 `input_mapping.barrier`(입력) 중 정확히 하나로 정한다. 같은 그래프에서 두 방식을 섞어 써도 된다.

- 제출할 때 거절하는 경우(`-32602`, `error.data.location`):
  - 둘 다 있다: `/tasks/<i>/input_mapping/barrier`
  - 둘 다 없다: `/tasks/<i>/command`
  - `barrier` 가 가리키는 입력이 string 이 아니거나 optional 이다: `/tasks/<i>/input_mapping/barrier`
  - 정적 이름이 barrier 이름 규칙(memory 키 문자와 길이)에 맞지 않는다: `/tasks/<i>/command/name`
  - `barrier` 를 wait_barrier 가 아닌 task 에 썼다: `/tasks/<i>/input_mapping/barrier`
- 입력에서 받은 이름이 규칙에 맞지 않으면 실행하지 않고 실패 단계 `input`, `error.location` `/input_mapping/barrier` 로 끝난다.
- v1 `task_create` 의 wait_barrier 는 입력이 없으므로 `name` 이 필요하다(없으면 `-32602`).
- 이름의 barrier 가 없거나 시간이 지나면 지금처럼 실행 단계에서 실패한다. barrier 를 만드는 일은 그래프를 짜는 쪽이 정한다.

### task 레코드 크기

task 레코드는 memory 값 하나라 직렬화가 memory 값 상한(기본 1 MiB)을 넘으면 저장하지 못한다. 결과가 너무 크면 위 §실행 회차와 완료처럼 출력을 뺀 짧은 실패로 바꿔 기록하는데, 그 짧은 실패마저 들어가지 않으면 같은 보고를 계속 다시 내며 task 가 Running 에 머물고 permit 을 쥔다. 그래서 결과 없이도 레코드를 키우는 몫(정의, 실행 직전에 붙는 입력 snapshot, 치환한 command)을 결과 전 상한으로 막는다(`task/record_limit.rs`).

상한은 memory 값 상한(`config.toml` `[memory]` 의 `entry_max_mb`, 바이트로 `entry_max_bytes`)에 맞춘다. 결과 몫 256 KiB(`RESULT_ROOM_BYTES`)를 남긴 값과 768 KiB(`MAX_RECORD_BEFORE_RESULT_BYTES`) 중 작은 쪽이다. 그 값이 64 KiB(`MIN_RECORD_BEFORE_RESULT_BYTES`)보다 작으면 64 KiB 를 쓴다.

| memory 값 상한 | 결과 전 상한 | 결과 몫 보장 |
|---|---|---|
| 1 MiB(기본) 이상 | 768 KiB | 있다 |
| 320 KiB 초과 1 MiB 미만(설정은 MiB 단위라 쓰지 않는다) | 값 상한 − 256 KiB | 있다 |
| 320 KiB 이하(설정 `0`) | 64 KiB(최솟값) | 없다. 부팅 때 `memory entry limit leaves no room for a task result` 경고를 남긴다 |

- 정의에 드는 것: `name`, `command`(run 의 argv·`cwd`, custom 의 `params`, agent 의 지시 등), `metadata`(그래프면 넣어 준 `dag` 포함), `on_failure`(inline fallback 정의), 계약(스키마, literal binding, 매핑, 후처리, 전이).
- 남은 256 KiB 는 짧은 실패 결과의 몫이다. 줄인 사유(4 KiB)의 사본 셋(상태·`typed_result.error`·v1 `result.error`), 접수 응답 머리, 회차 기록이 든다. 결과 전 레코드가 상한에 닿은 v2 custom 에 이스케이프가 가장 큰 사유(제어 문자, 바이트당 6배)로 줄인 실패와 다시 이스케이프하면 2배가 되는 64 KiB 접수 응답 머리를 함께 실은 레코드는 990,031 B 였다(시험 `a_shrunk_failure_always_fits_beside_a_definition_at_the_limit`).

| 시점 | 상한을 넘으면 |
|---|---|
| `agent.task_create`(CLI `task-create`) | 만들지 않는다. `-32602` 와 `invalid argument: task record too large: task <id>: the definition makes the task record <n> bytes, over the <상한> byte limit for a task before its result; …` |
| `agent.task_graph_submit`·`agent.task_graph_validate`(CLI `task-graph-submit`, `--dry-run`) | 아무것도 저장하지 않는다. `-32602`, `error.data` 의 단계는 `input`, `location` 은 그 task(`/tasks/<i>`) |
| 실행 직전 v1 출력·lease 치환 | 실행하지 않고 `task output substitution: task record too large: task <id>: the command with substituted values makes the task record …` 실패로 끝낸다. 치환한 command 는 저장하지 않는다 |
| 실행 직전 v2 입력 해석 | 실행하지 않고 입력 단계 실패(`location` 은 `/bindings`)로 끝낸다. snapshot 의 `value`·`execution` 은 비운다 |
| 상한이 생기기 전에 저장된 레코드의 실행 | Running 으로 바꾸기 전에 재서 시작하지 않고 Ready 에서 바로 실패로 끝낸다. 사유는 고정된 `task record too large: the definition leaves no room for a result` 이고 상태에만 싣는다. 회차 기록·`started_at`·결과(v1 `result`, v2 `typed_result`)는 붙이지 않는다. 크기를 담은 자세한 사유는 warn 로그에 남긴다 |

- 사유는 모두 `task record too large: ` 로 시작해 다른 입력 오류와 구별된다.
- 상한은 저장할 그대로(초기 상태 포함) 잰다. 그래프는 활성화 전 상태(`waiting`)로 잰다.
- 이미 Running 인 옛 레코드는 바꾸지 않는다. 그 결과를 기록하지 못하면 위 보류 규칙을 따른다.
- 시작하지 않은 실패도 상태가 `ready` 에서 `failed` 와 고정 사유로 바뀌고 `finished_at` 이 붙는 만큼 커진다. memory 값 상한까지 남은 몫이 v1 은 94 B, v2 는 135 B(봉투 포함 전 직렬화 기준) 이상이어야 기록된다(시험 `an_old_record_just_under_the_memory_entry_limit_still_fails`). 그보다 작으면 실패도 기록하지 못해 Ready 로 남는다. 시작하지 않았으므로 회차 기록·permit·lease·semaphore 는 없고, 러너는 tick 마다 다시 시작을 시도하며 warn 로그를 남긴다(시험 `an_old_record_without_room_for_the_failure_stays_ready`). 레코드를 다시 쓰지 않는 `task-delete` 로 정리한다.
- 후처리 회차 진행(`attempt.postprocess` 에 저장하는 본 작업 응답, 256 KiB 까지)은 이 몫에 넣지 않았다.

### 그래프 제출

앞뒤 task 를 서로 참조하는 정의를 하나씩 만들면 중간에 일부가 실행될 수 있다. `agent.task_graph_submit` 은 그래프 전체를 받아 검증한 뒤 한꺼번에 활성화한다.

```json
{"workspace_id": 1,
 "graph": {"contract_version": 2,
   "types": {"Verdict": {"type": "enum", "values": ["pass", "revise"]}},
   "tasks": [
     {"id": "build", "command": {"kind": "run", "workspace_id": 1, "command": ["cargo", "build"]}},
     {"id": "report", "command": {"kind": "run", "workspace_id": 1, "command": ["notify"]},
      "input_schema": {"type": "object", "fields": {"code": {"type": "int64"}}},
      "bindings": {"code": {"from_task": "build"}},
      "input_mapping": {"args": ["/code"]}}]}}
```

- task 키: `id`(필수, 호출자가 정하는 task id), `name`, `command`, `depends_on`, `on_failure`, `metadata`, `input_schema`, `output_schema`, `bindings`, `input_mapping`, `allowed_exit_codes`, `merge_conflict`, `postprocess`, `transitions`. 모르는 키는 거절한다. `command`·`on_failure` 안(`poll`·inline fallback 포함)의 모르는 키도 거절한다. 이 둘의 타입은 v1 `task_create` 와 저장된 기록도 읽으므로 타입 자체는 모르는 키를 무시한다. 그래프 제출만 값을 표지 값으로 바꿔 다시 읽어 보고 결과가 같은 키를 모르는 키로 본다(`params`·`metadata` 같은 JSON 값 필드 안의 키는 결과가 달라지므로 받는다). `types` 는 모든 task 가 함께 쓴다.
- 검증: id 형식과 중복(그래프 안·workspace), `depends_on`·fallback·reduce 입력·binding 원본의 존재, 계약과 binding 의 타입, 매핑, 전이(아래 절), 위 조합 규칙, 순환(전이 간선 포함). 그래프 task 의 command 에 v1 출력 placeholder(`${task.…}`)가 있으면 거절하고 binding 을 쓰라고 안내한다.
- 그래프 하나의 현재 상한은 1000개(MAX_GRAPH_TASKS)이며 초과하면 location: /tasks로 거절한다. 제출은 reply_task가 memory writer에 실행 단위 하나로 접수하고 완료 후 응답한다. 메인 루프는 writer 결과를 기다리지 않는다([저장 계약](../design/systems/storage-contract.md)).
- 실패하면 아무것도 저장하지 않고 `-32602` 로 답한다. `error.data` 는 실패 단계·task id·타입 오류와 함께 `location`(제출한 그래프 안의 JSON Pointer, 예: `/tasks/1/bindings/label`, 순환은 `/tasks`)을 싣는다. JSON 형식 오류(모르는 키, 필수 필드 누락, 타입이 다른 값)도 같다(`TaskGraphSpec::from_json`, `store/graph_parse.rs`). 형식 오류의 위치는 다음과 같다.
  - 모르는 키는 그 키다(`/tasks/0/bogus`, `/tasks/0/command/session/bogus`). 한 object 에 둘 이상이면 메시지가 말하는 키다.
  - 타입이나 값이 틀린 필드는 그 필드다(`/tasks/0/command/workspace_id`, `/tasks/0/on_failure/task`). 태그 enum(`kind`) 안의 오류도 `kind` 가 아니라 틀린 필드를 가리킨다. 틀린 것이 `kind` 값이면 `kind` 다.
  - 필수 필드가 빠졌으면 그 필드가 있어야 할 object 다(`/tasks/0/postprocess` 와 메시지 `missing field \`timeout_ms\``, `/tasks/0/command` 와 `missing field \`workspace_id\``).
  - 위치를 더 좁힐 수 없으면 좁힌 데까지의 object 다(그래프 수준이면 `""`).
  - 판정은 정의를 조금씩 바꿔 다시 읽어 보는 방식이다. 키를 빼서 오류가 사라지는 자리로 내려가되, 빼서 그 키만 없다는 오류로 바뀌는 필수 키는 오류 메시지가 가리키는 값(모르는 키 이름, 틀린 값)을 그 키 안에서 바꿨을 때 오류가 달라질 때만 탓한다. 누락은 그 필드를 `null` 로 넣어 오류가 바뀌는 가장 깊은 object 다.
- 통과하면 task 를 활성화 전 상태로 모두 저장한 뒤 그래프 레코드(`tasty.agent.task_graph.<그래프 id>`, `tasty.task_graph/v1`) 하나를 쓰고 readiness 를 평가한다. 그래프 레코드가 없는 task 는 Ready 가 되지 않으므로 저장 도중 러너가 돌아도 실행되지 않는다. task 나 그래프 레코드를 쓰다 실패하면 저장한 task 를 지운다. 레코드를 쓴 뒤 readiness 반영이 실패하면 지우지 않고 `-32603` 으로 답하며 `error.data` 에 `graph_id`·`possibly_active: true`·`cause` 를 싣는다(복구는 아래 §한계). 응답은 `{valid, activated, graph_id, durability, tasks}` 다.
- 그래프 id 는 `g-<ms>-<순번>` 이며 task 의 `graph_id` 에 기록한다. `metadata.dag` 가 없으면 그래프 id 를 넣어 DAG 로 묶는다. 그래프의 task 가 모두 삭제되면 그래프 레코드도 지운다.
- `agent.task_graph_validate` 는 같은 검증만 하고 저장하지 않는다(`{valid, activated: false, durability, tasks}`). CLI 는 `--dry-run` 이다.
- 그래프의 `durability` 는 `required`(기본) 또는 `best_effort` 다. memory 저장소가 대체 모드(`memory_init_fallback`, 재시작하면 사라진다)일 때 `required` 그래프는 검증·제출 모두 `-32602`(`error.data`: `location: /durability`, `store_durable: false`, `cause`)로 거절하고 아무것도 저장하지 않는다. `best_effort` 는 그대로 실행하되 재시작 복구를 약속하지 않는다. 판정은 `TaskService::task_graph_submit` 이 하므로(`AgentError::StoreNotDurable`) IPC 를 거치지 않는 호출자도 같다. 그래프 레코드에 `durability` 를 남기고, 제출 응답은 `durability` 와(대체 모드면) `durable: false` 를 싣는다(근거 docs/dev-guide/agent-runner.md).
- 러너는 켜지 않는다. 정지한 러너에서는 활성화된 task 가 Ready 로 남는다.

그래프 한도의 측정:

- 지금은 3단계와 하류 전파(`TaskStore::settle_waiting`)가 저장소 목록을 한 번 읽고 작업본을 갱신한다. 메모리 안의 readiness 그래프 재구성은 task 마다 남아 있어 비용은 여전히 제곱이지만 상수가 작다. `TaskStore::submit_graph` 를 같은 머신에서 번갈아 잰 값(dev 프로필, 3회 최소~최대)은 이전 구현이 200 개 178~539ms · 1000 개 3781~5441ms, 지금 구현이 200 개 38~85ms · 1000 개 181~488ms 다. 당시 구현의 1000개가 이전 200개와 같은 범위여서 상한을 정했다. 이 비교는 writer 이행 전의 알고리즘 측정이다. 초과는 `-32602`(`location: /tasks`)로 거절한다.
- 호스트 IPC 실측(격리 GUI 인스턴스, dev 빌드, 러너 정지, 제출 동안 다른 연결이 10ms 간격으로 `system.ping`, 측정 전 ping 기준선 0.09~0.10s):

  | 형태 | 제출 시간 | 제출 중 ping 최대 |
  |---|---|---|
  | 독립 1000(기존 task 1000 개가 있는 workspace, 1회) | 0.356s | 0.258s |
  | 독립 1000(새 workspace, 2회) | 1.515s / 0.390s | 1.404s / 0.285s |
  | 사슬 1000(새 workspace, 2회) | 0.354s / 0.391s | 0.247s / 0.285s |

  위 수치는 memory writer 이행 전 동기 IPC 경로의 과거 측정이며 현재 지연 보장이 아니다. 현재 경로의 상한은 1000이며 실행 위치는 위 writer 계약을 따른다. 이 문서에 현재 경로를 다시 실측한 값은 없다.

### 전이 조건과 경로 선택

생산자 task 의 `transitions` 는 성공한 출력으로 후속 task 를 고른다(근거 docs/dev-guide/agent-runner.md). 대상이 함께 제출돼야 하므로 그래프 제출로만 정한다. 저장소의 단건 생성(`TaskStore::create_typed`)은 전이가 든 계약을 거절한다.

```json
{"id": "review", "output_schema": {"ref": "ReviewResult"},
 "transitions": {
   "cases": [
     {"when": {"compare": {"path": "/verdict", "op": "eq", "value": "pass"}}, "to": ["ship"]},
     {"when": {"in": {"path": "/verdict", "values": ["revise"]}}, "to": ["fix"]}],
   "otherwise": ["human"]}}
```

| 키 | 뜻 |
|---|---|
| `mode` | `exclusive`(기본): 참인 case 가 정확히 하나여야 한다. `all_matches`: 참인 case 를 모두 고른다 |
| `cases` | `{when, to}` 목록. `to` 는 같은 그래프의 task id 목록 |
| `otherwise` | 맞는 case 가 없을 때 고를 대상 |
| `no_match` | `"finish"`: 맞는 case 가 없으면 아무것도 고르지 않는다. `otherwise` 와 둘 중 하나를 반드시 적는다 |

조건(`when`)은 다음을 조합한다.

| 형식 | 뜻 |
|---|---|
| `{"compare": {"path", "op", "value", "input"?}}` | 출력의 JSON Pointer 위치와 상수 비교. `op` 는 `eq`·`ne`·`lt`·`le`·`gt`·`ge` |
| `{"in": {"path", "values", "input"?}}` | 그 위치의 값이 목록 중 하나 |
| `{"all": [...]}`·`{"any": [...]}`·`{"not": {...}}` | 논리 조합. 목록은 비어 있으면 안 된다 |

- 조건은 그 task 의 확정된 최종 출력만 읽는다. reduce `all` 은 `input` 에 입력 task id 를 주면 그 입력의 레코드 출력을 입력의 선언 타입으로 읽는다(`contract::reduce_all_record_output`). 다른 task 에서는 `input` 을 거절한다.
- 제출할 때 위치의 타입을 출력 스키마에서 구해 검사한다. 비교 대상은 boolean·int64·float64·string·enum 이다. float64 는 `lt`·`le`·`gt`·`ge` 만, boolean·string·enum 은 `eq`·`ne` 만, `in` 은 int64·string·enum 만 받는다. 상수는 그 위치의 타입으로 검사한다(예: enum 에 없는 값은 거절). 출력 타입이 json 인 위치는 읽지 않는다.
- exclusive 에서 같은 조건이거나 같은 위치의 `eq`·`in` 값이 겹치는 case 는 제출할 때 거절한다. 그 밖의 다중 참은 실행할 때 경로 오류다.
- 대상은 같은 그래프의 다른 task 여야 한다. `on_failure: continue_downstream` 인 task 와 다른 task 의 fallback 은 대상이 될 수 없다.
- 오류의 `location` 은 `/tasks/<i>/transitions/...` 이다.

성공으로 끝나는 회차에서 경로를 고르고 결과와 같은 쓰기로 `route` 에 저장한다: `attempt_id`, `matched`(참인 case 순번), `otherwise`, `selected`(고른 대상, 선언 순서·중복 없음). 고르지 못하면(값이 없거나 null, exclusive 다중 참, reduce 입력 레코드 없음) 그 task 는 실패 단계 `route` 로 Failed 가 되고 출력은 남긴다. 실패한 task 는 경로를 고르지 않으며 대상은 실패 전파를 받는다.

v2 task 의 readiness:

1. 전이로 들어오는 제어 엣지가 있으면, 그 원본이 모두 끝날 때까지 기다린다. 하나라도 이 task 를 골랐으면 다음으로 간다. 원본이 성공 결과를 내지 못했으면 실패 전파, 아무도 고르지 않았으면 선택되지 않음이다.
2. depends_on·`from_task`·`one_of`·reduce 입력의 원본이 모두 끝날 때까지 기다린다. 선택되지 않은 원본은 기다릴 필요가 없는 경로로 본다(합류).
3. 제어 엣지가 없고 위 원본이 모두 선택되지 않았으면 이 task 도 선택되지 않는다.
4. 원본이 실패·취소·실패 전파로 끝났으면 실패 전파다(depends_on 은 fallback 이 대신 성공하면 통과). 선택되지 않은 원본을 필수 `from_task` 로 읽거나 `one_of` 원본이 하나도 성공하지 않았으면 역시 실패 전파다.

선택되지 않은 task 는 실행 없이 Skipped 가 되고 `skip: {"reason": "branch_not_selected"}` 를 남긴다. 실패가 아니므로 실패 정책(fallback·continue_downstream)을 적용하지 않는다. main 이 실패 없이 끝나 실행할 일이 없는 v2 fallback 도 같은 이유로 끝난다. 실패 전파로 건너뛴 v2 task 는 `skip: {"reason": "upstream_unavailable", "source", "source_state"}` 를 남긴다. v1 task 의 판정과 레코드는 그대로다.

선택되지 않을 수 있는 task(전이 대상, 또는 들어오는 경로가 모두 그런 task 에서만 오는 task)의 출력을 필수 `from_task` 로 읽는 task 는, 그 원본 말고 다른 경로로도 실행될 수 있으면 제출할 때 거절한다. 대안 경로의 값은 `one_of` 로, 없어도 되는 값은 optional·`default` 로 적는다. 같은 갈래 안의 사슬처럼 들어오는 경로가 그 원본뿐이면 받는다.

DAG 요약(`agent.dag_list`·`agent.dag_get`)의 `state_counts.not_selected` 는 `skipped` 중 선택되지 않은 수다. `recovered` 는 `failed` 중 같은 그룹의 fallback 이 대신 성공한 수다(fallback 의 fallback 을 따라간다). `awaiting_input` 은 `running` 중 `phase` 가 `awaiting_input` 인 수로, rollup 에는 쓰지 않고 화면의 DAG 목록 행이 입력 대기 표시에 쓴다. 성공·선택되지 않음·fallback 이 대신한 실패만 있으면 `rollup_state` 는 `succeeded` 다. v1 task 의 fallback 에도 같다. 저절로 진행할 수 있는 작업(running·ready, `blocked` 가 아닌 waiting. unknown 은 넣지 않는다)이 남아 있으면 실패가 섞여 있어도 `rollup_state` 는 진행 상태이고, 더 진행할 수 없을 때 복구되지 않은 실패가 있으면 성공한 끝 작업(`succeeded_ends`)이 있을 때 `partially_failed`, 없을 때 `failed` 다. 판단 순서는 [agent-collaboration](../features/agent-collaboration/index.md)에 있다. `agent.task_graph` 는 전이를 `kind: "transition"` 간선으로 내고 `selection`(`pending`·`selected`·`not_selected`·`unavailable`)을 싣는다. 노드는 `skip` 을 싣고, DOT 형식은 전이 간선에 선택 상태를 라벨로 붙이고(선택된 간선은 굵게, 선택되지 않았거나 쓸 수 없는 간선은 반투명), 선택되지 않은 노드를 `not selected` 라벨과 파선 테두리로 그린다. DAG 화면은 전이 간선을 긴 파선으로 그리고 선택 상태를 굵기와 불투명도로, 선택되지 않은 노드를 `NOT SELECTED` 라벨로 보인다.

### agent task

`{"kind": "agent", "provider": "<namespace>", "workspace_id": N, "session": ..., "instruction": "...", "timeout_ms"?: N}` 는 provider 세션에 지시 하나를 보내고 그 턴의 끝을 task 결과로 만든다. v2 계약이 필요하다(`task_create` 는 `-32602`).  명령의 `workspace_id` 는 새 세션이 뜨는 workspace 다. 회차 기록, 제출 안내의 `--workspace`, 턴 표의 키는 task 자신의 workspace 를 쓴다.

`provider` 는 IPC namespace 이름이다(번들 예: `claude`·`codex`). 고정 목록은 없다. 그 namespace 를 소유한 설치 플러그인이 활성이고 `agent.turn_report` 를 허가받았으면 결과 수집을 지원하는 provider 다(`tasty_agent::task::agent::check_provider`). 플러그인 관리자가 설치 목록과 설정으로 계산해 발행한 namespace 소유 표(`tasty_utils::plugin_namespaces`)를 읽으므로 플러그인 프로세스가 아직 준비되지 않았어도 판정은 같다. 첫 `<provider>.spawn`·`tell` 호출이 프로세스를 띄운다. 부팅은 엔진이 복원 task 의 러너를 만들기 전에 표를 발행한다([부팅 순서](../architecture/boot-sequence.md)). provider 플러그인은 `<provider>.spawn`·`<provider>.tell`·`<provider>.state` 를 구현하고 아래 턴 보고를 보내야 한다.

- Given 자격이 없는 provider When 생성하면 Then 계약 오류(입력 단계)로 거절하고 문구가 원인을 구분한다: `no installed plugin owns the '<p>' namespace`, `plugin '<id>' that owns the '<p>' namespace is disabled`, `... does not have the agent.turn_report permission`, 표 발행 전 `plugin information is not loaded yet`. 모두 `agent provider '<p>' does not support result collection:` 으로 시작한다.
- Given 생성 뒤 플러그인이 비활성화·제거되거나 권한이 회수된 task When 실행하면 Then 실행 직전에 다시 판정해 `agent_unavailable` 로 실패하고 같은 원인 문구를 남긴다. 성공으로 끝나지 않는다.

| `session` | 동작 |
|---|---|
| `{"kind": "new", "parent_surface": N, "cwd"?: "..."}` | `<provider>.spawn` 으로 자식을 띄우고 지시를 첫 프롬프트로 준다. 그 자식의 첫 턴이 이 회차의 턴이다. spawn 은 다시 보내면 세션이 하나 더 생기므로 일반 호출(5초)보다 긴 30초까지 응답을 기다린다. 기다리는 일은 spawn 마다 띄운 스레드가 맡고, 러너는 poll 마다 응답이 왔는지만 본다(그동안 다른 task 가 진행된다). 응답이 오면 그 스레드가 바로 턴 표에 묶는다. 30초까지 답이 없으면 `agent_unavailable` 로 끝난다. 그 뒤에도 spawn 응답을 5분까지 기다려, 그 사이 뜬 세션은 task 가 `surface.close` 로 닫는다(이 task 가 만든 세션이라 회차에 묶이지 않은 채 남기지 않는다). 응답을 기다리는 중이나 응답을 넘겨받기 전에 task 를 취소하거나 러너를 멈춰도 같게 닫는다. 5분이 지나도 답이 없으면 결과를 알 수 없어 닫지 못한다. 응답을 기다리는 동안 호스트가 재시작하거나 러너를 다시 켜면 `unknown` 이다 |
| `{"kind": "existing", "surface_id": N}` | 세션이 idle 이고 다른 task 가 쥐지 않았을 때만 `<provider>.tell` 로 보낸다. 사용자의 턴이 진행 중이거나 입력을 기다리면 끼어들지 않고 기다린다(`timeout_ms` 가 지나면 `timed_out`). tell 이 5초 안에 답하지 않으면 실행되지 않았음이 확실할 때(큐 대기 중 만료)만 다시 보내고, 시작된 tell 은 다시 보내지 않고 늦은 응답을 기다린다(위 §호스트 응답 지연) |

턴 귀속은 메모리의 턴 표(`AgentTurns`)가 한다. surface 하나에 회차 하나만 묶이므로 같은 세션의 agent task 는 차례로 실행된다. task id 는 workspace 마다 정해지므로 표는 회차를 (workspace, task) 로 찾고 풀며, 다른 workspace 의 같은 이름 task 와 섞이지 않는다. provider 플러그인은 훅에서 `agent.task_turn_report` 로 턴 시작(`turn_started`)과 끝(`turn_ended`: `final_answer` 또는 `error`)을 알린다. 이 메서드는 `agent.turn_report` 권한(이 메서드만 연다)을 가진 플러그인 중 그 provider namespace 를 소유한 플러그인만 부를 수 있다(그 밖의 호출은 `-32001`). 기존 세션은 시작 보고를 받은 뒤의 종료만 이 회차의 것으로 본다(앞선 사용자 턴의 늦은 종료를 섞지 않는다). 지시 끝에는 회차 표지 줄(`[tasty-task-attempt:<회차 토큰>]`)을 붙이고(앞부분의 계약 값은 `tasty-plugin-protocol`의 `task_turn::ATTEMPT_MARKER_PREFIX`다. `tasty-agent`는 계층 규칙상 protocol에 의존하지 않아 같은 값을 두고, 루트 패키지 시험이 두 값을 대조한다), provider 플러그인은 시작 보고에 프롬프트에서 찾은 표지 토큰을 싣는다(`prompt_seen: true`·`attempt_marker`). 기존 세션의 시작 보고는 표지가 그 회차 토큰일 때만 받으므로, 묶은 뒤 지시가 전달되기 전에 사용자가 시작한 턴은 이 회차의 턴이 되지 않는다. 훅이 프롬프트를 넘기지 않은 시작 보고(`prompt_seen` 없음)는 가릴 수 없어 그대로 받는다. 이때는 묶음과 지시 전달 사이의 짧은 창에 시작한 사용자 턴이 섞일 수 있다. 다른 provider 의 보고와 종결 뒤 보고는 무시한다. Claude 세션에 Stop 게이트가 붙어 있으면 게이트가 응답을 이어 가게 한 Stop 은 턴 끝이 아니다. 플러그인은 게이트 판정이 턴 종료로 확정된 Stop 에서 그 Stop 의 최종 답변으로 끝을 한 번 보고한다([Claude 플러그인](../plugins/claude/index.md)의 턴 보고 표). 그래서 게이트 세션의 최종 답변은 게이트에 대한 마지막 응답이다. 게이트 세션에서 지시의 첫 답이 필요하면 명시 제출(`agent.task_submit_result`)을 쓴다. 회차에 적용된 턴 보고는 호스트 로그에 debug 레벨 한 줄(provider·surface·보고 종류·task id·회차 id)로 남는다. 실제 실행에서 보고 횟수와 귀속을 확인할 때 `TASTY_LOG="info,tasty::adapters::ipc::handler::agent::agent_turn=debug"` 로 켠다.

결과:

- 출력 타입이 `string`(기본)이면 최종 답변이 출력이다(`provenance.output_source`: `agent.final_answer`).
- 그 밖의 타입은 같은 회차에 명시 제출한 값이 출력이다(`agent.submitted`). 지시문 끝에 제출 방법(`tasty agent task-submit --workspace … --id … --attempt-id … --token … --output '<JSON>'`)과 출력 스키마를 붙인다. 후처리가 있으면 제출이 필요 없고 후처리가 받는 실행 결과에 최종 답변이 든다.
- `agent.task_submit_result` 는 값이 도착할 때 출력 타입으로 검사하고(실패하면 `-32602` 와 `output_validation`) 턴이 끝날 때 결과로 확정한다. 응답 `final: false` 는 task 가 아직 끝나지 않았다는 뜻이다. 같은 값을 다시 내면 `duplicate: true`, 거절은 `-32018` 과 `error.data.reason`: `not_running`·`stale_attempt`·`conflict`(같은 회차의 다른 값)·`turn_ended`·`not_the_session`(세션 토큰의 agent 가 그 task 의 세션이 아님)·`wrong_token`(회차 토큰이 다름).
- 회차마다 예측할 수 없는 회차 토큰(16바이트 난수, 16진수 32자)을 새로 만들어 지시의 제출 안내에 싣고(`--token`), 제출은 `token` 이 그 회차의 것과 같아야 받는다. 회차 id(`<task id>#<n>`)는 짐작할 수 있어 다른 회차나 다른 호출자가 실수로 낸 값을 막지 못하기 때문이다. 로컬 IPC 가 신뢰 경계라는 원칙은 그대로이고, 토큰은 보안 경계가 아니다(같은 컴퓨터의 호출자는 세션 화면이나 handle 기록에서 토큰을 읽을 수 있다).
- 원본 보고(`provider`·`surface_id`·`final_answer`·`submitted`)는 `raw.execution` 에 남는다. 직렬화가 64 KiB 를 넘으면 앞부분만 `raw.execution_truncated` 에 남는다(위 §결과 확정의 응답 상한).

| 상황 | 결과 |
|---|---|
| 턴이 끝났는데 필요한 결과가 없다(제출 없음, 최종 답변 없음) | Failed, `code: result_missing`, stage `output_validation`. 제출이 없었으면 그 턴의 마지막 답을 앞에서 2000자(`ANSWER_EXCERPT_CHARS`)까지 `raw.execution.final_answer` 에 남기고, 잘랐으면 `final_answer_truncated: true` 를 붙인다. 답이 출력 타입에 맞아도 제출하지 않았으면 결과가 아니다 |
| 턴이 오류로 끝났다(Claude StopFailure, Codex interrupt) | Failed, `agent_turn_error` |
| 세션이 끝났다(`exited`) | Failed, `agent_exited` |
| `timeout_ms` 초과 | Failed, `timed_out` |
| provider 를 호출할 수 없다, 재시작으로 턴 귀속을 잃었다 | Failed, `agent_unavailable` |
| 시작한 tell 의 응답 통로가 답 없이 닫혔다, tell·spawn 응답을 기다리는 동안 재시작했다 | Unknown, `reason` 머리말 `dispatch outcome unknown:`. 지시가 전달됐는지(spawn 이면 세션이 떴는지) 확인한 뒤 `retry`·`cancel` 로 정한다 |
| 입력 대기(`needs_input`) | Running 유지. 회차에 `agent: {provider, surface_id, awaiting_input_since}` 를 남기고 `task_get` 의 `phase` 는 `awaiting_input`(그 밖의 Running agent task 는 `executing`) |
| idle 인데 종료 보고가 없다 | Running 유지. 성공으로 보지 않는다 |

실패 코드는 `typed_result.error.code` 다. 업무 값 `"revise"` 같은 출력은 성공이다. 취소·실패·종결 때 턴 묶음만 풀고 세션은 닫지 않는다(사용자가 결과를 확인할 수 있게 둔다). 예외는 위의 늦게 뜬 spawn 세션 하나다. 기존 세션(`existing`)은 어떤 경우에도 닫지 않는다. 턴 표는 영속하지 않으므로 호스트가 재시작되면 실행 중이던 agent task 는 `agent_unavailable` 로 끝난다. tell 의 늦은 응답이나 spawn 응답을 기다리던 task 는 지시가 전달됐는지 알 수 없어 `unknown` 이다.

세션은 provider 의 spawn 이 만드는 터미널 surface 이고, 그 셸 환경은 터미널 규칙을 따른다. 위 §runner 자식의 환경의 규칙은 러너가 직접 띄우는 프로세스에만 적용된다.

한계:

- Codex 세션이 샌드박스(네트워크·loopback 차단) 안에서 돌면 `tasty agent task-submit` 이 호스트에 닿지 못한다. 구조화 출력은 제출이 없어 `result_missing` 으로 끝난다. 기본 `string` 출력은 턴 보고로 받으므로 영향이 없다. 구조화 출력이 필요한 Codex agent task 는 loopback 을 허용하는 실행 모드로 띄운다.
- 훅이 프롬프트를 넘기지 않는 provider 의 기존 세션은 묶음과 지시 전달 사이에 사용자가 시작한 턴을 가리지 못한다(위 턴 귀속 문단).

### v2 reduce

| 전략 | 출력 타입 | 규칙 |
|---|---|---|
| `all` | `list<{task_id, state, has_output, output?, attempt_id?}>` 고정 | 실패한 입력의 출력을 만들어 넣지 않는다. `attempt_id` 는 값을 낸 입력 회차(v1 입력은 없음)다. `output` 은 입력마다 타입이 달라 json 으로 선언되며, 각 입력의 선언 타입대로 직렬화한 값을 넣는다(int64 입력은 10진 문자열, json 입력은 그대로). 구체 타입으로 쓰려면 입력 task 를 지목해 그 출력 스키마로 다시 읽는다(`contract::reduce_all_record_output`) |
| `first_success` | 선언한 공통 타입 T(기본 json) | 모든 입력이 T 에 대입 가능해야 생성된다. v1 입력은 json 으로 본다. 성공 출력이 하나도 없으면 실패 |
| `merge_json` | object 또는 json | 모든 입력이 object 출력을 가져야 한다. 같은 경로의 다른 값은 기본 오류, `merge_conflict: "overwrite"` 면 뒤 입력이 이긴다. 같은 값은 충돌이 아니다. 숫자끼리는 표기가 아니라 수치로 비교한다(`1` 과 `1.0` 은 같고, 정수는 f64 로 바꾸지 않고 정확히 비교한다) |
| `concat_text` | string | 모든 입력이 string 출력을 가져야 한다. 다른 타입은 명시적으로 변환하라는 오류 |
| `custom` | 선언 타입(기본 json) | stdin 은 `all` 과 같은 레코드 배열(같은 직렬화 규칙). stdout 은 JSON 값 하나여야 하며 문자열로 대신하지 않는다 |

v1 reduce 는 기존 동작(`reduce_with_custom`) 그대로다.

custom 셸(v1·v2·`agent.task_reduce` 공통, `run_custom_shell`)의 입력은 별도 스레드에서 stdin 에 쓰고 stdout·stderr 는 종료를 기다리며 모은다. 그래서 입력을 다 읽기 전에 파이프 버퍼보다 많이 출력하는 셸도 막히지 않는다. 셸이 stdin 을 읽지 않고 끝나 쓰기가 파이프 끊김(`EPIPE`)으로 끝나도 실패가 아니며, 성패는 셸의 종료 상태로 정한다(0 이 아니면 `exit_code=<n>, stderr=<…>` 실패). 그 밖의 쓰기 오류는 실패다.

### DAG report

report 는 사람이 읽는 실행 기록이다.  task 마다 자동 항목(`auto`)과 회차별 custom 기록을 둔다. 뒤 task 가 읽는 데이터가 아니다. binding 은 출력 문서의 위치만 가리키므로 report 를 입력으로 받을 수 없다. 코드: 모델 `crates/tasty-agent/src/task/report.rs`, 저장 `crates/tasty-agent/src/task/store/report.rs`, 주소 발급·stderr 표지 `crates/tasty-task-runtime/src/runner_host/report.rs`, 조회 `crates/tasty-task-runtime/src/report.rs`.

자동 항목은 원본인 task 레코드가 살아 있는 동안 복사하지 않고 조회할 때 레코드에서 만든다. retry 로 닫히는 회차만 그 원본이 지워지기 직전에 한 번 굳혀 저장한다(아래).

| 필드 | 내용 |
|---|---|
| 공통 | `kind`, `state`(종결 상태), `started_at`·`finished_at`, `input`(실제 입력 snapshot, wire 형식), `output`(실제 출력, wire 형식), `failure: {stage, location, message}`, `skip`(skip 사유) |
| run | `exit_code` |
| agent | `provider`, `surface_id` |
| custom | `ipc_method` |
| reduce | `strategy`(전략 이름), `input_count` |
| wait_barrier | `barrier` |
| `include_raw` 일 때 | `raw: {stdout, stderr}` — 저장된 실행 결과의 출력. 기본으로는 싣지 않는다. 실패한 Run 도 성공과 같은 출력 꼬리를 싣는다 |

실패한 Run 도 결과에 종료 코드(`raw.exit_code`)와 실행 출력(`raw.execution` 의 stdout·stderr 꼬리, 성공과 같은 64 KiB 상한)을 저장한다. 그래서 그 회차의 자동 항목에도 `exit_code` 와 `raw` 가 있고, retry 때 굳힌 자동 항목도 같다. `failure.message` 는 `Run exited with code <n>` 과 뒤따르는 stdout·stderr 꼬리인데, 꼬리는 줄기마다 마지막 2 KiB(`FAILURE_MESSAGE_TAIL_CAP`)만 싣고 머리에 버린 바이트 수를 적는다. 메시지는 레코드의 상태·`typed_result.error`·v1 `result.error` 세 곳에 복사되므로, 64 KiB 꼬리를 그대로 두면 이스케이프가 많은 출력(빈 줄·ANSI·NUL)에서 레코드가 memory 값 상한(1 MiB)을 넘는다. 전체 꼬리는 raw 에 한 벌만 있다. 종료 코드 없이(신호로) 끝났으면 `exit_code` 는 `null` 이다. 종료 코드·출력을 저장하기 전에 기록한 레코드와 재시작용 실행 결과 셀(`run_result`)은 둘 다 없이 읽는다.

`retry` 는 다음 회차를 열면서 레코드의 결과·입력 snapshot 을 지운다. 그래서 retry 는 지우기 전에 닫히는 회차의 자동 항목(`raw` 포함)을 `tasty.agent.task_report_auto.<task id>.<n>` 에 한 번 저장하고, 그 회차 블록에 끝난 상태(`settled`)를 남긴다. 원본이 사라진 뒤의 유일한 사본이라 같은 값을 두 곳에 두지 않는다. 자동 항목은 task 레코드의 일부만 담으므로 레코드처럼 memory 값 상한 안에 들어간다. 굳힌 키를 만들 수 있도록 task 제출은 가장 긴 report 키(`tasty.agent.task_report_auto.<task id>.4294967295`)가 memory 키 길이 256 바이트 안에 드는 id 만 받는다. 이 검사 전에 저장된 더 긴 id 의 task 는 retry 때 경고만 남기고 굳히지 않는다.

custom 기록은 회차마다 memory 키 하나(`tasty.agent.task_report.<task id>.<n>`)에 둔다. task 레코드와 키를 나눠 report 크기가 상태 전이 쓰기를 막지 않게 한다.

- 블록: `{entries: [{seq, at, source, text, omit_by_limit?}], omitted_appends}`. `source` 는 `run`·`postprocess`·`agent`·`reduce_custom`·`stderr_marker` 중 하나이며 호출자가 밝힌 값이다(인증하지 않는다). 텍스트만 받는다. JSON 을 넣어도 문자열로 저장한다.
- 회차 토큰: 러너가 v2 task 를 dispatch 할 때 그 회차의 report 토큰(16바이트 난수, 16진수)을 만들어 task 레코드에 둔다. 주소는 `<workspace id>/<회차>/<source>/<토큰>/<task id>` 다.
- 받는 조건: 주소의 토큰·회차가 task 의 마지막 dispatch 회차와 같고 task 가 Ready 또는 Running 이며, 그 회차가 retry 로 닫히지 않았어야 한다(retry 뒤 다음 dispatch 전에는 레코드가 Ready 이고 토큰도 이전 회차 것이므로 블록의 `settled` 로 판정한다). 다른 task·이전 회차의 토큰은 `-32018` `reason: token_mismatch`, 끝난 회차는 `reason: closed`(`state` 에 지금 상태)다. Unknown 도 닫힌 회차다. 세션 토큰으로 부른 agent 세션은 `agent.task_submit_result` 처럼 자기 세션에 지시를 보낸 회차에만 쓴다. 턴 표에서 그 task 의 회차가 이 세션 surface 와 주소의 회차에 묶여 있지 않으면 토큰이 맞아도 `reason: not_the_session` 이다. 이 검사는 토큰·닫힘 검사 뒤에 하므로 끝난 회차에 대한 늦은 쓰기는 세션이어도 `closed` 다. 사용자 CLI 와 플러그인(`agent` 권한) 호출은 주소의 토큰만 본다. 종결 전이는 저장소 잠금 안에서 task 레코드를 쓰므로, 그 쓰기 뒤의 append 는 모두 거절된다. 그래서 블록은 회차가 끝난 순간의 내용으로 확정된다.
- 상한: append 하나가 append 상한(기본 1 KiB)을 넘으면 UTF-8 경계에서 자르고 항목 끝에 `omit_by_limit`(잘린 바이트 수)를 붙인다. 블록의 저장 텍스트 합이 블록 상한(기본 16 KiB)을 넘게 되면 저장하지 않고 `omitted_appends` 만 늘린다. 이 판정은 append 마다 하므로, 큰 기록이 한 번 넘쳐 세어진 뒤에도 남은 자리에 들어가는 짧은 기록은 저장된다(`seq` 는 세어진 것까지 이어 매긴다). 어느 경우든 호출은 성공한다. 상한은 설정의 `[task_pipeline]` 이고 `report_append_bytes < report_block_bytes` 여야 한다(범위 64~65536, 128~131072). 설정 적용 때 어긋난 쌍은 거절하고, 설정 파일에 어긋난 쌍이 있으면 기본값으로 읽는다. 블록 상한의 위쪽은 JSON 이스케이프로 텍스트가 몇 배 커져도 memory 값 하나의 상한(1 MiB) 안에 들도록 정했다.
- 실패·취소·Unknown 회차의 블록도 그 상태로 남는다. Skipped 회차는 실행하지 않아 블록이 없다. retry 는 새 회차 블록(#2, #3 …)을 따로 만든다. task 를 지우거나 GC 하면 블록과 굳힌 자동 항목도 지운다.

경로별 append:

| 경로 | 방법 |
|---|---|
| run · 후처리 CLI · reduce custom 셸 | 자식 환경의 `TASTY_TASK_REPORT` 로 `tasty agent report append '<text>'`(source 는 각각 `run`·`postprocess`·`reduce_custom`) |
| run 의 stderr | `::tasty-report::<text>` 로 시작하는 줄 하나가 항목 하나다(source `stderr_marker`). 러너가 stderr 를 읽으면서 저장하고, 결과 기록 전에 stderr 읽기가 끝나므로 표지는 블록이 닫히기 전에 들어간다. 표지 줄은 저장된 stderr 에도 그대로 남는다. 줄 끝의 `\r` 은 뺀다. 한 줄은 64 KiB 까지만 읽고, 넘어서 버린 바이트 수(줄 끝 `\r` 제외)는 append 상한이 자른 수와 합쳐 `omit_by_limit` 에 남긴다. 블록에 남은 자리가 0 이 되면 그 뒤 표지 줄은 저장소 잠금과 쓰기 없이 세기만 하고, stderr 를 다 읽은 뒤(블록이 닫히기 전) `omitted_appends` 에 한 번에 더한다. 그 사이 설정에서 블록 상한을 올려도 그 회차의 나머지 표지 줄은 세기만 한다. stdout 은 보지 않는다 |
| agent | 지시문 끝에 사용법을 읽는 명령(`tasty agent report usage`) 한 줄과 그 회차의 주소를 붙인다. 에이전트는 `tasty agent report append --address '<주소>' '<text>'` 로 쓴다 |
| custom(IPC 메서드) · reduce 기본 전략 · wait_barrier | custom 기록이 없다. 자동 항목만 있다 |

조회는 `agent.dag_report`(`{id, workspace_id?, task?, attempt?, include_raw?}`, CLI `tasty agent dag-report <dag> [--workspace N] [--task <id> [--attempt <n>]] [--include-raw]`)다. 응답은 `{dag, workspace_id, name, tasks: [{task_id, name, auto, attempts: [{attempt, state, custom, auto?}]}]}` 이다. 맨 위 `auto` 는 고른 회차(주지 않으면 지금 레코드)의 자동 항목이다. retry 로 닫힌 회차는 `attempts[]` 항목에도 굳힌 `auto` 가 실리고, 굳힌 값이 없는 회차(이 저장이 생기기 전에 닫힌 회차)는 `null` 이다. 지금 회차의 항목에는 `auto` 가 없다(맨 위와 같다). 굳힌 `raw` 도 `include_raw` 일 때만 싣는다. `attempt` 는 `task` 가 있어야 한다. append 는 `agent.report_append`(`{address, text}`, 응답 `{result: "stored", seq, omit_by_limit?}` 또는 `{result: "omitted", seq}`)다. agent task 의 세션은 세션 토큰으로 부르므로 `agent` 권한으로 열려 있다. 세션 호출은 자기 세션의 회차로 묶이고, 그 밖의 호출은 주소의 토큰이 쓸 블록을 정한다. CLI `tasty agent report append` 는 성공하면 아무것도 출력하지 않고 종료 코드 0 으로 끝난다(잘리거나 세기만 한 경우도 같다). 거절·연결 실패만 표준 오류에 쓰고 0 이 아닌 코드로 끝난다. 후처리와 reduce 셸에서는 표준 출력이 결과 자리이기 때문이다.

### 저장 형식

v2 task 는 `tasty.agent.typed_task.<id>` 키에 `{"record_format": "tasty.task/v2", "task": {...}}` envelope 로 저장한다. v1 키(`tasty.agent.task.<id>`)와 접두사가 겹치지 않아 v1 목록 조회(구버전 앱 포함)는 v2 레코드를 보지 않는다. 구버전 `Task` 모델은 envelope 를 task 로 읽지 못하므로 v2 레코드를 실행하지 않는다. 현재 저장소는 v1 namespace 에서 계약을 가진 레코드와 모르는 `record_format` 을 오류로 보고한다.


## 한계

호스트가 Run 의 자식이 끝나기 전에 죽으면 자식은 init(1) 등으로 넘어가 Tasty 가 종료 코드를 받을 수 없다. 재시작 뒤 그 task 는 결과를 꾸미지 않고 `unknown`(`state.reason` 에 사유)이 된다. 프로세스가 아직 살아 있으면 끝날 때까지 Running 으로 기다린 뒤 `unknown` 이 된다. `unknown` 은 종결이 아니다. 하류는 실패 전파 없이 기다리고(DAG 집계에서는 `blocked`), fallback 도 실행되지 않는다. 사람이 결과를 확인한 뒤 `retry`(새 회차로 다시 실행)나 `cancel` 로 정한다. 근거는 docs/dev-guide/agent-runner.md "결과를 회수할 수 없는 회차" 절. 감시 프로세스를 두어 종료 코드를 남기는 방식은 쓰지 않는다(같은 절의 대안). semaphore·lease 를 쥔 Run 도 같다. 프로세스가 살아 있는 동안 점유를 유지하고, 끝난 것을 확인한 때 반환한다. `unknown` task 는 점유를 쥐지 않으며 `retry` 가 새 회차로 다시 얻는다. 감시할 Run handle 이 없는 Running task 의 점유는 부팅 정리(`purge_stale_{semaphore,lease}_holders`)가 반환하고 `Failed("host restart")` 로 끝낸다.

같은 이유로 **캡처한 stdout/stderr 도 유실된다** — 자식은 호스트 재시작 후에도 살아남지만(수명 계약은 그대로 유지), 파이프를 들고 있던 드레인 스레드는 호스트와 함께 사라지므로 그 사이의 출력은 다시 읽을 방법이 없다. 장시간 작업(빌드/배포)의 결과 보존이 중요해지면 `pty.*` 기반 별도 경로가 더 맞다.

그래프 제출에서 그래프 레코드를 쓴 뒤 readiness 반영을 마치기 전에 호스트가 죽거나 저장이 실패하면(응답 `-32603`, `error.data.possibly_active: true`), 그래프는 활성이고 일부 task 는 이미 실행됐을 수 있다. 반영하지 못한 의존 없는 task 는 Waiting 으로 남는다. 다음 부팅이나 러너 시작이 readiness 를 다시 평가해 남은 task 를 마저 활성화하므로, 그래프를 그대로 실행하려면 러너를 다시 시작하면 된다. 실행하지 않고 다시 제출하려면 다음 순서로 한다.

1. `task_list` 로 그 그래프(`graph_id`)의 task 상태를 확인한다.
2. 남은 Waiting task 를 `task_purge`(`states: ["waiting"]`)나 `task_delete` 로 지운다.
3. 지운 task 를 담은 그래프를 다시 제출한다. 새 그래프 id 가 붙는다. 이미 성공해 남아 있는 task 와 id 가 겹치면 거절되므로, 그런 task 를 다시 실행하려면 다른 id 를 쓴다. 남아 있는 task 를 원본으로 읽는 binding 은 그 id 를 그대로 참조할 수 있다.

그래프 레코드를 쓰기 전에 죽으면 활성화 전 task 가 남지만 실행되지 않으며 `task_delete`·`task_purge` 로 지운다.

## 관련

- [telemetry › AgentId](../features/telemetry/index.md#agentid--agent-식별) — `AgentId` 도출 · [reference/api](../reference/api.md) — agent namespace

## 저장 키에 사용하는 이름

barrier·semaphore·rate-limit 이름과 task ID는 소문자 `a-z`, 숫자, `.`, `_`, `-`를 사용한다.
저장 키 전체는 256바이트 이하이며, 각 종류의 내부 접두사 길이만큼 이름의 한도가 줄어든다.
빈 값은 기존 규칙대로 허용한다. 생성뿐 아니라 조회·삭제·획득 등 이름을 받는 경로에 같은
검사를 적용하고 위반은 `-32602` 입력 오류로 반환한다.

공용 `tasty_agent::component_key`는 허용 문자와 전체 길이를 `tasty_memory`의 규칙으로
확인한다. 오류에는 입력한 값 기준의 위치·문자·남은 바이트 한도를 싣는다.
비ASCII 문자를 바이트로 잘라 잘못 표시하지 않는다. 새 저장소도 이 함수를 사용한다.
lease의 resource는 키 구성요소를 인코딩하므로 임의 문자열을 받는 별도 규칙이다.
