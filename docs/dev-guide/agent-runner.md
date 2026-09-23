# Agent task runner

workspace 단위 thread 1개가 `Ready` task 를 자동 dispatch 하고 `Running` task 완료를 polling 으로 감지해 state 머신을 진행시키는 *task DAG executor*. 상태 머신·영속과 custom reducer 의 기본 셸 runner(`run_custom_shell`)는 [`tasty-agent`](../../crates/tasty-agent/) 가, task *실행* 은 host 가 위임받는다. IPC/CLI 표면 명세는 [reference/api](../reference/api.md)("agent" namespace).

## 구성

| 파일 | 역할 |
|------|------|
| `crates/tasty-agent/src/runner.rs` | `TaskExecutor` trait + `RunnerLoop::tick`(순수 로직) |
| `crates/tasty-agent/src/platform/` | cross-platform pid liveness probe(`process_alive`) |
| `src/core/agent/runner_host.rs` | `HostExecutor` — `TaskExecutor` host 구현 + `RunnerContext`(memory + agent_seq + host_ipc injector) |
| `src/core/agent/runner_thread.rs` | `RunnerRegistry` — workspace 별 thread start/stop/status + 재시작 정화 |
| `crates/tasty-ipc/src/host_call.rs` | `HostIpcInjector` — runner thread 가 plugin IPC 를 동기 호출하는 통로 |
| `src/adapters/ipc/handler/agent/` | `task`/`barrier`/`semaphore`/`lease`/`ratelimit` IPC 핸들러 |

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

2단계는 `Ready` 인지만 보고 그 task 가 "어떤 main 의 아직 만들어지지 않은 fallback 참조 대상" 인지는 알 방법이 없다 — `fallback{task}` 는 참조 대상(fallback)이 참조자(main)보다 먼저 존재해야 하는 2단계 `task_create` 흐름이라, 그 사이에 tick 이 끼면 아직 아무도 참조하지 않는 fallback 후보가 정상적으로 `Ready` 로 dispatch 돼버릴 수 있다(TOCTOU). `task_create(..., reserved_for_fallback: true)`(`TaskStore::create_reserved_for_fallback`)로 만든 task 는 `TaskGraph::dormant_as_pending_fallback` 이 이 필드를 최우선으로 존중해 `Ready` 로 노출하지 않으므로, 애초에 이 2단계 dispatch 루프의 대상이 되지 않는다 — 상세는 [features/agent-collaboration §Task DAG](../features/agent-collaboration/index.md).

### DispatchHandle

`PolledDispatch { workspace_id, poll_method, poll_params, state_field, terminal_states, failure_states, interval_ms, deadline_ms }`(범용 폴링 — dispatch 시점에 완성된 `poll_params` 로 terminal 상태 도달까지 `poll_method` 반복 호출; `failure_states` 적중은 성공이 아니라 실패로 종결) · `ShellProcess { pid }`(`Run` task 자식; `Child` 객체는 Clone 불가라 executor 의 `shell_children` map 에 별도 보관) · `BarrierPoll { workspace_id, name }` · `ReduceImmediate`/`CustomImmediate`/`ImmediateFail`(dispatch 시점 즉시 결정) · `AwaitExternal { wait_key, deadline_ms }`(push-kind 완료 전략, 아래 참조 — `poll` 은 계약대로 **항상 Active**, 종결은 외부에서 store 를 직접 전이시킨다). `deadline_ms` 는 dispatch 시점 `now + timeout_ms` — handle 자체에 실려 영속되므로, 이 handle 을 만든 `hook_task_waits` 매핑(비영속)이 재시작으로 사라져도 재시작 후 reload 가 독자적으로 만료 판정을 할 수 있다(아래 "호스트 재시작 정화 + 핸들 영속" 참조). 이 필드 도입 이전에 영속된 구 포맷은 `#[serde(default)]` 로 `0`(=즉시 만료)이 된다.

### HostExecutor 매핑

| TaskCommand | dispatch | poll |
|-------------|----------|------|
| `Run { command, cwd }` | `Command::spawn`(stdout/stderr `Stdio::piped()`) → pid → `ShellProcess`. 빈 command Err. stdout/stderr 를 각각 별도 드레인 스레드로 즉시 읽기 시작(파이프 교착 방지 — 아래 "출력 캡처" 참조) | watcher thread 의 `child.wait()` 결과 cell 조회 → exit 0 이면 두 드레인 스레드를 join 해 캡처 결과를 실은 Done / 아니면 Failed(캡처 결과를 에러 메시지에 포함) |
| `Custom { ipc_method, params, poll: None }` | host IPC dispatch(timeout 5s) → 등록된 완료 판정 전략 중 `ipc_method` 를 `default_for_methods` 로 지목한 전략이 있으면 그 kind 를 채택(아래 "완료 판정 전략 레지스트리" 결정 6 — poll 이면 `PolledDispatch`, push 면 아래 push 행과 동일), 없으면 `CustomImmediate` | 매칭된 kind 에 따라 poll/push 행과 동일 / 아니면 즉시 Done |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Inline(spec)) }` | host IPC dispatch → `map_from_request`/`map_from_response` 로 `poll_params` 완성 → `PolledDispatch` | `poll_method` 호출 → `state_field` 가 `failure_states` 중 하나면 **Failed**(상태값 + 응답 요약을 에러 메시지에) / `terminal_states` 중 하나면 Done(응답 전체가 산출물) / 아니면 Active(`deadline_ms` 초과 시 Failed) |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Named{strategy}) }`, poll-kind | 완료 판정 전략 레지스트리에서 `strategy` 를 이름 해석(`resolve_strategy`) → 얻은 `PollSpec` 으로 위 Inline 행과 동일 처리. 미등록/비활성이면 해석 실패 → dispatch 자체가 `PermanentFail`(Running 진입 전에 드러남) | 위와 동일 |
| `Custom { ipc_method, params, poll: Some(PollSpecRef::Named{strategy}) }`, push-kind | `dispatch_push_strategy` — 원 dispatch `params.surface_id` 대상 surface 에 `notify_via` 훅 핸들러를 `hook.set(..., once: true)` 로 1 회성 등록해 `hook_id` 획득 → `RunnerContext.hook_task_waits` 에 `(workspace_id, task_id, deadline)` 등록 → `AwaitExternal`. `surface_id` param 이 없으면 `PermanentFail` | 항상 Active(계약) — 종결은 `PendingHostEvent::HookFired` 소비부(`Core::resolve_hook_task_wait`, exit code 로 성공/실패 분기)와 timeout 안전망(`runner_thread::expire_overdue_hook_waits`)이 담당 |
| `Reduce { inputs, strategy }` | input 결과 collect → `reduce_with_custom` → `ReduceImmediate`. `inputs` 는 Task DAG 의 암묵적 의존성(`TaskGraph`, `crates/tasty-agent/src/task/graph.rs`)이라 dispatch 시점엔 이미 전부 종결(terminal) 상태다 — `Ready` 로 올라오기 전에 readiness 평가가 그 종결을 강제한다 | 즉시 Done |
| `WaitBarrier { name }` | `BarrierPoll` | `Open`→Active / `Closed`→Done / `TimedOut`→Failed |

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
> 이어서 지시를 주는 노드도 같은 모양이다 — `tell` 에도 기본 전략(`tell-wait`)이 있어 자식이 그 지시를 마칠 때까지 러너가 기다린다. 여기서는 대상이 **자식** surface 다(spawn 노드의 `surface_id` 는 부모였다):
> ```json
> {"kind":"custom","ipc_method":"claude.tell",
>  "params":{"surface_id":561,"message":"방금 고친 테스트를 다시 돌려라"}}
> ```
>
> **현재 되는 것.** 자식을 띄우고(spawn) 이어서 지시를 주고(tell) 각각의 완료를 기다리는 것까지 위 그대로 동작하고, 자식이 죽으면 그 노드는 실패한다. spawn→tell 을 한 DAG 로 이을 때 tell 노드의 `surface_id` 는 아래 `${task.<id>.output<pointer>}` 로 넘긴다.

### `Run` 출력 캡처

`Run` 은 Surface(Tab)를 만들지 않는 bare subprocess다 — argv 를 그대로 `Command::spawn` 에 넘기고(셸 word-splitting 없음), exit code 도 명령 자신의 것이다. tty 가 필요한 명령은 지원 대상이 아니다(그건 `pty.*` primitive 의 몫).

- **드레인 스레드 필수**: `Stdio::piped()` 만 붙이고 파이프를 읽지 않은 채 `child.wait()` 하면 자식이 OS 파이프 버퍼(플랫폼별 16~64KB)를 채우고 block, 부모는 그 자식의 종료를 기다리므로 교착한다. dispatch 가 stdout/stderr 를 각각 별도 스레드(`agent-shell-{stdout,stderr}-pid<N>`)로 즉시 드레인 시작하고, watcher(`agent-shell-watcher-pid<N>`)는 `child.wait()` 후 두 드레인 스레드를 join 한다 — task 당 스레드 3개(watcher + drain 2개).
- **스트림당 64KiB tail**: 각 드레인 스레드는 EOF 까지 계속 읽되 마지막 64KiB 만 보관한다(head 는 버림). 상한을 넘기면 `truncated: true` + `dropped_bytes` 를 함께 기록. 64KiB × 2스트림인 이유는 캡처 결과가 `run_result` 로 memory store 값 하나(1MiB 상한)에 JSON 직렬화되기 때문 — ANSI escape 팽창까지 고려한 최악의 경우도 1MiB 아래.
- **성공/실패 모두에 담김**: 성공(exit 0)은 `TaskResult.output = {"pid", "stdout": {"text","truncated","dropped_bytes"}, "stderr": {...}}`. 실패(비0 exit)는 `PollOutcome::Failed` 가 문자열 하나만 나르는 계약이라, 캡처한 stdout/stderr tail 을 에러 메시지 본문에 그대로 이어붙인다 — `cargo build` 같은 명령의 컴파일 에러 본문도 이 경로로 드러난다.
- **알려진 한계**: tail 전용이라 긴 빌드의 *첫* 에러는 놓칠 수 있다(요약/실패 지점은 보통 출력 뒤쪽). ANSI 는 벗기지 않고 보존한다.

### `Run` 결과를 reduce 하기 — `--extract-path`

`agent.task_reduce`(CLI `tasty agent task-reduce`)의 in-process 전략(`concat_text`/`merge_json`/`all`/`first_success`)은 각 input 의 `output` 을 구조 무관하게 통째로 다룬다 — `Run` task 의 `output` 이 위 "출력 캡처" 의 `{pid, stdout:{text,...}, stderr:{...}}` 구조라는 걸 모른 채 `concat_text` 로 이어 붙이면 직렬화된 JSON 조각이 그대로 이어붙어 유효한 JSON 도, 사람이 읽을 텍스트도 아닌 결과가 나온다. `merge_json` 도 같은 구조를 가진 두 `Run` 결과를 merge 하면 `pid`/`stdout`/`stderr` 키가 전부 겹쳐 뒤 input 이 앞 input 을 통째로 덮어쓴다.

`--extract-path <JSON Pointer>`(예: `/stdout/text`)를 주면, reducer 전략을 적용하기 전에 각 input 의 `output` 에서 그 경로(RFC 6901 JSON Pointer)만 뽑아낸다 — `crates/tasty-agent/src/reducer.rs::extract_paths`. 예:

```sh
tasty agent task-reduce --workspace-id 1 --inputs t-a,t-b \
  --strategy concat_text --extract-path /stdout/text
# → {"value": "out1\nout2\n", "warnings": []}
```

- 생략 시 기존 동작(전체 `output` 그대로) 유지 — 하위 호환.
- 지정된 경로가 없는 input(예: `Run` 이 아닌 다른 kind 의 결과)은 reduce 전체를 실패시키지 않는다 — 그 input 만 `output: null` 로 취급하고 나머지는 정상 진행하되, 응답의 `warnings` 배열에 `input #<i>(task <id>)에 경로 '<path>'가 없어 null로 처리했습니다` 를 남긴다.
- DAG 안의 `TaskCommand::Reduce`(위 `HostExecutor 매핑` 표) 는 이 옵션이 없다 — `--extract-path` 는 독립 호출 `agent.task_reduce`/`task-reduce` CLI 전용이다.

### 선행 task 출력을 파라미터로 넘기기 — `${task.<id>.output<pointer>}`

`depends_on` 은 실행 **순서**만 묶는다. 선행 task 의 결과를 후행 task 의 **입력**으로
넘기려면 placeholder 를 쓴다 — dispatch 직전에 upstream 의 `result.output` 에서 값을
뽑아 `task.command` 에 주입한다(`src/core/agent/task_output_ref.rs` 가 문법 소유자,
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
tasty agent task-create --workspace-id 1 --name spawn-child \
  --command '{"kind":"custom","ipc_method":"claude.spawn",
              "params":{"surface_id":1,"workspace":"ws","cwd":"/tmp","prompt":"..."}}'

# B: 그 자식에게 tell — A 의 응답을 참조. depends_on 은 필수다(아래).
tasty agent task-create --workspace-id 1 --name tell-child --depends-on "$T_A" \
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

`Custom.poll` 이름 참조(`PollSpecRef::Named`)와 결정 6(`default_for_methods`) 이 가리키는 대상 — "임의 IPC dispatch 가 끝났는지"를 이름으로 등록해두는 독립 레지스트리다. `src/hook_handler/`(공유 훅 핸들러 레지스트리) 를 정본 템플릿으로 **형태만** 미러링한다 — 3출처 병합(host 내장 TOML + plugin manifest + user config `~/.tasty/completion-strategies.toml`), patch semantics(Host→Plugin→User, `Some` 필드만 override), id 규약(`<owner>/<short>`, `host`/`<plugin_id>`/`user`), 전역 싱글턴 `global()`. `HookHandlerId`(push 형이 참조) 외에는 훅 핸들러의 타입을 import 하지 않는다.

전략 종류(`CompletionStrategyKind`):
- **poll**: `tasty-agent::PollSpec` 그대로 재사용.
- **push**: `notify_via: HookHandlerId` + 필수 `timeout_ms`(보고 유실 시 task 가 영구 Running 에 남지 않도록 하는 안전망). `notify_via` 는 등록 시점에 존재·owner(자기 자신 또는 `host`) 를 검증한다.

`resolve_poll_spec(id)` 은 poll-kind 만 반환(push 는 `NotPollKind` 에러) — poll spec 만 필요한 소비자용. `resolve_strategy(id)` 는 kind-agnostic — `Custom` dispatch(runner_host.rs)처럼 poll/push 를 모두 다뤄야 하는 호출부가 쓴다.

**push 완료 신호 소비 배선**: push-kind dispatch 는 `notify_via` 가 가리키는 훅 핸들러를 원 dispatch `params.surface_id` 대상 surface 에 `hook.set(event: "command-completed", once: true)` 로 1 회성 등록해 `hook_id` 를 얻고, `RunnerContext.hook_task_waits`(`Arc<HookTaskWaits>` — runner thread 가 `Core` 를 거치지 않고 `task_waker_hub` 와 동형으로 직접 공유)에 `(workspace_id, task_id, deadline)` 로 등록한 뒤 `AwaitExternal` 로 전이한다. 실제 종결은 두 경로:
- **정상 보고**: 그 훅이 실제로 발화하면 `PendingHostEvent::HookFired` 소비부(`app/dispatch/host_events.rs::resolve_hook_fired_task_waits` → `Core::resolve_hook_task_wait`)가 hook_id 로 대기 task 를 찾아 마감한다 — 실제 관측된 exit code(`CommandCompleted` 만 보유)가 있으면 `0`/없음 → Succeeded, 비-0 → Failed(그 코드를 실은 error 메시지). exit code 개념이 없는 push 신호는 Succeeded.
- **timeout 안전망**: `runner_thread::expire_overdue_hook_waits` 가 매 tick `HookTaskWaits::sweep_expired` 로 deadline 지난 항목을 강제 Failed 마감한다(워크스페이스 무관 전역 sweep — 어느 runner thread 든 편승 가능).

이 기전으로 등록되는 오늘 유일한 push 전략이 `host/command-completed`(아래 참조)다. `notify_via` 훅 핸들러의 실제 action(현재: `notification.create` 알림)은 task 완료 판정과 무관한 부가 효과일 뿐 — 판정은 hook_id 매칭만으로 이뤄진다.

네임스페이스 제한(결정 2): plugin 소유 전략의 `poll_method`/`default_for_methods` 는 자기 IPC namespace(`<plugin_id>.*`) 만 가리킬 수 있고, host/user 소유는 어떤 plugin namespace 도 가리킬 수 없다(`_host` 권한 우회 방지) — `tasty_ipc::method_meta::is_registered_plugin_prefix` 로 검증.

결정 6(`default_for_methods`, 역방향 소유): 매니페스트에 메서드 단위 선언 축이 없어 전략이 자기가 기본이 될 IPC 메서드 목록을 든다. 여러 활성 전략이 같은 메서드를 지목하면 정렬 승자(priority↑ → owner tie-break user>plugin>host → id)가 채택되고 패자는 warn.

실패 축(`failure_states`): poll 형 전략은 `terminal_states` 와 별개로 **실패로 종결할** 상태값 목록을 든다. 이 목록에 적중하면 task 가 `Succeeded` 가 아니라 `Failed` 로 전이하므로 그 노드의 `OnFailure`(abort / continue_downstream / fallback)가 비로소 동작한다 — 목록이 비면(생략 시 기본) 종전대로 terminal 도달을 전부 성공으로 읽는다. 두 목록에 같은 값이 들어간 선언은 거부하지 않고 **실패를 우선**한다(실패를 성공으로 읽는 쪽이 그 반대보다 위험하다). 번들 claude/codex 전략은 `exited`(자식 프로세스 사망 / surface 소멸)를 여기에 둔다. claude 의 `needs_input` 은 성공 쪽에 남는다 — 사람이 승인하면 이어서 끝나는 상태라 영구 실패가 아니고, spawn/tell 완료 알림이 이미 `idle` 과 동일 취급한다. 이 축은 **DAG 러너 전용**이다: CLI auto_wait(`PollingDecl`)로는 넘기지 않는다(`CompletionStrategyDecl::to_polling_decl` 주석 참조) — CLI 폴링은 terminal 도달 시 block 만 풀고 exit code 를 가르지 않으므로, 실패 축을 흘리면 `tasty claude spawn` 같은 기존 명령이 자식 사망 시 비-0 으로 죽는 계약 변경이 된다.

IPC/CLI: `completion_strategy.list`(전 범위 조회, 비활성 포함) / `tasty completion-strategy list`. reload/dispatch 대응물은 없다(user config 재로드 미노출, "발화" 개념 없음). 내장 host 기본값은 `src/completion_strategy/defaults/default-completion-strategies.toml`:

- `host/command-completed`(결정 7) — OSC 133 셸 통합 기반 push 전략. `notify_via = "host/command-completed"` 는 `src/hook_handler/defaults/default-hook-handlers.toml` 에 등록된 훅 핸들러를 가리킨다(둘 다 host defaults 로 함께 설치되므로 항상 존재). `timeout_ms = 300000`(5분). 전제: 대상 surface 가 OSC 133(셸 통합 스크립트)을 로드하고 있어야 발화한다 — 미로드 surface 를 대상으로 `hook.set --event command-completed`(이 전략의 내부 dispatch 경로 포함)를 걸면 거부는 아니고 warn 로그만 남는다(`shell_integration_boundary_seen`, 시간 기반 추정이라 오탐 가능). `Custom` task 의 dispatch `params` 는 `surface_id` 를 포함해야 한다(예: `surface.send`).

## RunnerRegistry

`Core::agent_runner_registry()` 로 접근. workspace 1개당 thread 1개:

- `start(ctx, ws) -> bool` — 이미 실행 중이면 false(idempotent). crashed 면 정리 후 재시작 허용.
- `stop(ws) -> bool` — stop_tx + join.
- `status(ctx, ws)` — `running`/`crashed`/`ready_count`/`running_count`/`store_error`/`list_failures`. 두 카운트는 `Option` 이다 — store 를 못 읽으면 `None`(응답에선 `null`)이고 `store_error` 가 이유를 싣는다. `list_failures` 는 러너 스레드의 연속 조회 실패 횟수(러너가 없으면 0).

thread 본문은 `RunnerLoop::tick` + 500ms `recv_timeout`. tick 안 memory lock 은 *짧은 구간* 만(list → release → dispatch/poll(lock 밖) → re-lock for set_state) — 사용자 CLI 동시 호출과 락 경합 최소화.

tick 머리의 `TaskStore::list` 가 실패하면 **빈 목록으로 흡수하지 않고 로그를 남긴다.** `tick` 은 넘겨받은 task 슬라이스만 순회하므로(terminal 흡수 · Running poll · Ready dispatch 전부) 빈 목록으로 진행하는 것과 이번 tick 을 건너뛰는 것의 동작은 같다 — permit 회수도 snapshot 에서 terminal 로 바뀐 task 를 봤을 때만 일어난다. 그래서 흐름은 종전대로 빈 snapshot 으로 돌리고(다음 tick 에 회복되면 밀린 전이를 한꺼번에 흡수한다) 관측 가능성만 더한다: 첫 실패는 `warn`, 연속 6회(약 3초)째부터는 `error` 로 올리고 그 뒤로는 약 60초 주기로만 반복한다. 조회가 회복되면 `info` 로 연속 실패 횟수와 함께 남긴다. `error` 인 이유는 이 상태가 지속되면 task DAG 는 정지해 있는데 `task_list` 응답의 `ready_count`/`running_count` 는 여전히 진행 중으로 보이기 때문이다.

`agent.task_run`(start/stop/status)은 `METHOD_TABLE` 에 `plugin(&[AgentManage])` 로 등록돼 있다 — 호스트가 재시작 시 runner 를 자동으로 켜지 않으므로(아래 "재시작 계약"), plugin 이 자기 workspace 의 runner 를 스스로 되살릴 수단이 필요하기 때문이다. `task_set_result` 는 `local_only()` 로 등재돼 있다 — 러너가 Custom task 의 생명주기를 단독 소유하므로 plugin 이 같은 task 를 별도로 전이시키면 쓰기 주체가 둘이 되어 러너의 완료 판정과 경합한다. plugin 은 완료 판정 전략 선언(아래 "완료 판정 전략 레지스트리")으로 우회한다.

## 재시작 계약

**자동 시작은 하지 않는다.** 호스트 재시작 후 어떤 workspace 의 runner thread 도 자동으로 켜지지 않는다 — `agent.task_run --action start` 로 수동(또는 plugin) 재개해야 한다. 대신 다음 두 가지를 보장한다:

1. **재시작 정화는 부팅 시 1회, runner 없이도 수행한다.** `purge_stale_agent_state_on_boot`(`Core`, `src/core/mod.rs`)가 headless(`src/boot.rs`, host IPC injector 등록 + `CoreState` 확보 직후)와 GUI(`src/app/boot_machine.rs::finish_boot`, 첫 윈도우 등록 직전) 양쪽 부팅 경로에서 호출된다. 라이브 `CoreState.workspaces` 전부에 대해 아래 "호스트 재시작 정화 + 핸들 영속" 절의 3종 세트(`purge_stale_semaphore_holders`/`purge_stale_lease_holders`/`reload_persistent_handles`)를 수행하고, `reload_persistent_handles` 가 되살린 handle 목록은 버린다(이 시점엔 그걸 넘겨받아 poll 할 runner 가 없다 — 다음 수동 start 가 다시 reload 한다). task 가 없는 workspace 는 각 정화 함수가 candidates 없음으로 조기 반환하므로 실질적으로 no-op — "라이브 workspace ∩ task 보유 workspace" 교집합과 동치. 여러 번 호출해도 안전(idempotent): `alive` 분류는 부수효과가 없고, `dead`/`stale`/`precise` 분류는 이미 정리된 뒤엔 대상이 남지 않는다.
2. **정지 상태는 조회로 드러난다.** `task_run --action status` 뿐 아니라 `task_list`/`task_graph` 응답에도 `runner: { running, crashed, ready_count, running_count, store_error, list_failures }` 를 동반한다 — runner 가 꺼져 있어도(`running: false`) `ready_count`/`running_count` 는 store 를 직접 조회한 실제 값이라, "비-terminal task 는 있는데 아무도 안 돌리고 있다"가 이 응답만으로 드러난다. **그 조회 자체가 실패하면 두 카운트는 `null`** 이고 `store_error` 가 이유를 싣는다 — 0 을 돌려주면 "task 가 없다" 와 값이 같아져 이 계약이 거짓이 된다. 러너는 살아 있는데 계속 못 읽는 상태는 `list_failures`(연속 실패 횟수)로 드러난다: `running: true` 이면서 이 값이 크면 DAG 는 정지 상태다. `task_get` 응답은 task 가 `AwaitExternal` handle 로 외부 신호를 기다리는 중이면 `awaiting_external: { wait_key, deadline_ms }` 를 함께 실어 "그냥 running" 과 구분한다(`AwaitExternal` 의 poll 은 계약상 항상 Active 라 state 만으로는 대기 이유를 알 수 없다). CLI(`tasty agent task-{list,get,run}`)는 이 값들을 사람이 바로 읽는 텍스트로 렌더한다(`crates/tasty-cli/src/format.rs`) — runner 가 멈춰 있고 대기 중인 task 가 있으면 재개 커맨드까지 안내 문구로 보여준다.

`hook_task_waits`(hook_id → task_id 매핑)는 여전히 **비영속**(프로세스 메모리 전용)이다 — 재시작하면 사라진다. 그래서 재시작 후 `AwaitExternal` task 는 **훅으로는 깨어날 수 없고**, 그 handle 에 실린 `deadline_ms`(위 참조)로만 마감된다: reload 시점에 이미 만료된 handle 은 즉시 `Failed`, 아직이면 그대로 복원되지만 이후 그 프로세스가 계속 살아있는 동안은(`AwaitExternal` poll 이 항상 Active 라 tick 이 deadline 을 검사하지 않음) 다음 재시작의 reload 가 다시 판정할 때까지 마감되지 않는다 — "재시작을 한 번 더 거쳐야 완전히 청소된다"는 절충이다.

### 자동 GC

`purge_stale_agent_state_on_boot`(`src/core/agent/runner_thread.rs`)의 같은 루프 안에서, 위 3종 세트 정화 직후 `gc_stale_tasks(ctx, workspace_id)` 가 한 번 더 돈다 — task 삭제 경로(`agent.task_delete`/`agent.task_purge`, `crates/tasty-agent/src/task/store.rs::{delete_checked,plan_sweep,apply_sweep_plan}`)와 정확히 같은 참조 안전 로직을 태우는 자동 스윕이다.

- **임계값**: `AGENT_TASK_GC_MIN_AGE_MS`(`runner_thread.rs`, 잠정 7일) — 상태와 무관하게 `now - 기준시각 >= 임계값` 인 task 가 후보. 기준시각은 terminal task 는 `finished_at`, 그 외(`waiting`/`ready`)는 `created_at`. 값 자체는 provisional — 실사용 데이터가 쌓이면 재검토 대상.
- **상태를 terminal 로 제한하지 않는 이유**: 방치된 `waiting` task(예: 입력이 끝나지 않는 `Reduce`)를 terminal-only 로 제약하면 영원히 못 지우고, 그게 참조로 자기 입력들을 붙잡아 그 입력들도 영영 GC 대상에서 빠진다. `running` 은 `plan_sweep` 이 항상 후보에서 제외하므로 별도 처리가 필요 없다.
- **`PutOpts.expires_at` 류의 memory 자체 TTL 은 쓰지 않는다** — TTL 만료는 참조 무결성·상태 검사를 완전히 우회한 채 그냥 지워버려, dangling 참조·자원 누수를 다시 끌어들이기 때문이다. 항상 `plan_sweep`(순수 함수, 후보 선정) → `apply_sweep_plan`(실제 삭제) 을 거치고, 실제로 지워진 task 마다 `tasty.agent.handle.<id>`/`tasty.agent.run_result.<id>` side-key 도 `evict_task_side_keys` 로 정리한다.
- `plan_sweep` 은 fixed-point 로 "후보 집합 밖에서 참조되는 task"만 반복 제외한다 — 즉 서로를 참조하는 방치 task 끼리는(예: `waiting` `Reduce`(X)가 그 input(Y)을 참조) 후보 집합 안에서 함께 드레인되고, 후보 밖의 살아있는 task 가 참조하는 대상만 보존(`retained`)된다.

## host→plugin 동기 IPC (`HostIpcInjector`)

runner thread 는 off-main 이라 `PluginManager`(App main thread 단독 소유)를 직접 못 부른다. injector 경유: `IpcCommand`+`sync_channel(1)` 을 App IPC 큐에 push → waker 로 App 깨움 → tick 의 routing 이 plugin 에 forward → 응답이 sync_channel 회신 → runner 의 `recv_timeout(5s)`. `Core::set_host_ipc_injector` 가 IPC 시작 직후 1회 등록(boot.rs headless + app/boot_machine.rs gui 양쪽).

주입은 IPC 서버와 **같은 큐 입장 장부**를 거친다. 큐에 든 호스트 주입 명령이 이미 상한만큼이거나(메인 루프가 서 있는 동안 시간 초과로 돌아간 호출이 남긴 명령이 쌓인 경우) 큐의 바이트 합이 넘치면, 명령은 큐에 들어가지 않고 `InjectError::Refused` 로 즉시 돌아온다 — `InjectError::nothing_ran()` 이 참이라 "안 됐다" 가 확실하고, 시간 초과(결과 불명)와 갈린다. runner 의 `dispatch_plugin` 은 그 오류를 문자열(문구에 "nothing ran")로 task 결과에 올리고 다시 걸지 않는다. 상한 값과 근거는 [ADR-0391](../adr/0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md). 주입은 제 대기 상한(5 s)을 명령의 기한으로도 싣는다 — 상한까지 큐에서 못 나간 명령은 나중에도 실행되지 않고 `InjectError::Expired`(문구 `… while still queued (nothing ran)`)로 돌아오며, 상한 전에 시작된 명령만 `InjectError::Timeout`(결과 불명)이다. 근거는 [ADR-0451](../adr/0451-a-host-injection-carries-its-wait-as-a-deadline.md).

## 동기화 primitive 통합

| primitive | 통합 위치 | 결합 |
|-----------|----------|------|
| `Semaphore` | RunnerLoop dispatch | `task.metadata.semaphore = { name, holder?, ttl_ms? }` — `ttl_ms` 는 task 최대 소요보다 길게(아래 dispatch 게이트) |
| `Lease` | RunnerLoop dispatch | `task.metadata.lease = { resource, holder?, ttl_ms?, mode? }` 또는 pool 모드(`candidates`/`elastic` — 아래 "자원 풀 배정") |
| `Barrier` | dispatch/poll | `WaitBarrier { name }` task(DAG 안 명시 gate) |
| `RateLimit` | IPC dispatcher 미들웨어 | `(agent, "ipc_calls")` 호출당 1 차감 |

### dispatch 게이트 (lease → semaphore)

`lease → semaphore` 순서로 점유. 한쪽 점유 후 다음이 Deferred/Err 면 점유 자원 즉시 release(idempotent). dead-lock 회피 — 두 자원 모두 가용일 때만 통과. permit/lease 는 task 가 Succeeded/Failed/Cancelled 로 종결되면 자동 release.

**`ttl_ms` 는 그 task 의 최대 소요보다 길어야 한다(호출자 책임).** runner 는 dispatch 시점에만 permit 을 잡고, 이미 `Running` 인 task 의 permit 을 **renew 하지 않는다** — 재dispatch 되지 않으므로 heartbeat 할 지점이 없다. 그래서 `metadata.semaphore.ttl_ms` 를 실제 소요보다 짧게 걸면 permit 이 실행 도중 만료되고, 다음 tick 에 대기하던 task 가 **같은 슬롯을 취득해 둘이 동시에 임계구역에 들어간다**(이중 dispatch). `ttl_ms` 를 아예 주지 않으면(기본) 만료가 없어 이 상태가 생기지 않는다 — TTL 은 "홀더가 죽어도 permit 이 돌아오게" 하는 opt-in 이지 소요시간 추정 장치가 아니다. 근거와 기각한 대안(runner 가 poll 마다 renew)은 [ADR-0119](../adr/0119-agent-semaphore-resize-and-holder-expiry.md).

**`holder == task.id` 컨벤션(강제)**: holder 가 task.id 와 다르면 *외부 도구가 직접 acquire 한 것* 으로 간주, 호스트 재시작 정화 대상에서 제외. 외부 점유 회수는 외부 도구 책임 — 그 회수 수단이 아래 "죽은 홀더와 한도 조정" 이다.

### 죽은 홀더와 한도 조정

세마포어 홀더는 `{ id, acquired_at?, expires_at? }` 객체다. `semaphore-list` 로 **누가 언제부터 잡고 있는지** 알 수 있고, 그게 "이 홀더가 죽었는가" 를 사람이 판정하는 근거다. `acquired_at` 이 없는 홀더는 시각 기록이 도입되기 전에 잡힌 것이다(0 이 아니라 부재로 표현한다).

**만료는 opt-in 이다.** `semaphore-acquire --ttl-ms <N>` 으로 잡은 permit 은 `now + N` 에 만료되고, 다음 `acquire` 또는 `semaphore-list` 시점에 lazy 하게 회수된다. 같은 holder 로 다시 acquire 하면 갱신된다(heartbeat) — lease 의 `ttl_ms` 와 같은 메커니즘·같은 의미다. `--ttl-ms` 를 주지 않은 permit 은 **회수되지 않는다**: 오래 걸리는 정당한 작업의 permit 이 도중에 만료돼 두 홀더가 동시에 임계구역에 들어가는 것이 교착보다 나쁘기 때문이다([ADR-0119](../adr/0119-agent-semaphore-resize-and-holder-expiry.md)).

**한도는 `semaphore-set-permits` 로 제자리에서 바꾼다.** `semaphore-create` 는 이미 있는 이름을 거절하므로, delete → create 로 우회하면 세마포어가 존재하지 않는 틈이 생기고 그 사이 아무나 임계구역에 들어간다.

```sh
tasty agent semaphore-set-permits --workspace-id 1 --name cap2 --permits 3   # 확대: 즉시 반영
tasty agent semaphore-set-permits --workspace-id 1 --name cap2 --permits 1   # 축소: drain
```

축소는 **drain** 이다 — 이미 점유 중인 홀더를 강제로 끌어내지 않는다. `holders.len() > permits_total` 인 초과 상태를 그대로 두고 새 acquire 만 거절하므로, 홀더가 하나씩 반납하며 새 한도로 수렴한다. 그동안 `permits_available` 은 0 이다(음수로 내려가지 않는다).

교착이 이미 났다면(홀더가 반납 없이 사라졌고 그 permit 에 TTL 이 없다면) 회수 수단은 두 가지다 — 제3자가 `semaphore-release --holder <그 id>` 로 대신 반납하거나, `semaphore-set-permits` 로 한도를 늘려 대기자를 통과시킨다. 어느 쪽이든 `semaphore-list` 의 `acquired_at` 을 보고 판단한다.

### 동시성 제한 (concurrency limit)

Semaphore 를 이 용도로 쓴다. 로컬 오케스트레이터(예: conductor 처럼 여러 task 를 동시에 굴리는 상위 도구)가 리소스가 약한 환경에서 "동시 실행 개수"에 상한을 두고 싶을 때 쓰는 패턴이다. `agent.rate_limit_*`(IPC dispatcher 미들웨어, 위 "rate_limit 미들웨어" 참조)는 *호출 빈도* 제한이고 `Local` caller 를 면제 대상에서 빼므로 이 목적에 맞지 않는다 — 여기서 쓰는 건 위 표의 `Semaphore` 통합이다.

절차:

1. 세마포어를 원하는 permit 수로 만든다: `agent semaphore-create --workspace-id 1 --name cap2 --permits 2`.
2. 동시성을 묶고 싶은 task 들을 만들 때 각각 `--metadata '{"semaphore":{"name":"cap2"}}'` 를 붙인다(편의 플래그가 있으면 `--concurrency-limit cap2` 로 대체 가능 — 아래 "CLI 예" 참조).
3. `metadata.semaphore` 를 안 붙인 task 는 이 제한과 무관하게 즉시 병렬 실행된다 — 세마포어는 **태그가 붙은 task 끼리만** 경쟁한다. 서로 다른 이름의 세마포어를 쓰면 그룹별로 독립된 상한을 걸 수 있다.

동작(dispatch 게이트, 위 참조): permit 이 남아있는 동안은 태그된 task 가 즉시 `Running` 으로 전이하고, permit 이 바닥나면 dispatch 가 `Deferred` 를 반환해 `Ready` 상태를 유지한 채 다음 tick 에 재평가한다 — 큐잉이 자동이라 오케스트레이터가 직접 대기열을 관리할 필요가 없다. task 가 종결(Succeeded/Failed/Cancelled)되면 permit 이 자동 반환되고, 대기 중이던 다음 task 가 같은 tick 이후 자동으로 이어받는다.

라이브 검증 예(2-permit 세마포어에 독립 task 4개, 각 4초 sleep):

```sh
tasty agent semaphore-create --workspace-id 1 --name cap2 --permits 2
for i in 1 2 3 4; do
  tasty agent task-create --workspace-id 1 --name "t$i" \
    --command '{"kind":"run","command":["sleep","4"]}' \
    --concurrency-limit cap2   # 또는 --metadata '{"semaphore":{"name":"cap2"}}'
done
tasty agent task-run --workspace-id 1 --action start
tasty agent task-list --workspace-id 1   # 즉시 조회 시 2개만 running, 2개는 ready
# … 4초 후
tasty agent task-list --workspace-id 1   # 앞 2개가 끝나며 나머지 2개가 자동으로 running 전환
tasty agent semaphore-list --workspace-id 1
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

`resource: "x"` 는 `candidates: ["x"]` 의 sugar 다 — 둘 다 store 안에서 같은 `lease_key`(`crates/tasty-agent/src/lease.rs`) 위치에 쓰기 때문에 관측적으로 동일하다(같은 자원을 가리키면 서로 충돌 판정된다). 별도 코드 경로 통합은 하지 않는다 — `resource` 단일 경로(`LeaseStore::acquire`)는 `agent.lease_acquire` IPC 등 기존 호출자의 `LeaseConflict` 에러 계약을 그대로 유지하려고 독립 구현을 보존한다.

**두 서브모드 — `elastic` 은 반드시 명시적 opt-in, 기본은 fixed다:**

- **fixed**(`elastic` 생략, 기본): `candidates` 안에서만 순회한다. 전부 점유 중이면 `mode` 에 따라 실패(`fail`) 또는 대기(`block` → Deferred, 다음 tick 재시도).
- **elastic**(`elastic: {...}` — 빈 객체 `{}` 도 opt-in으로 인정): candidates 가 전부 소진되면 `overflow_prefix + N` 형태의 새 후보 이름을 store 가 원자적으로 합성해 즉시 배정한다. `max_candidates` 를 주면 그 상한(고정 candidates 개수 + 합성된 개수)까지만 증설하고, 넘으면 fixed 와 동일하게 대기한다.

elastic 이 기본이 아닌 이유: fixed 의 자원 배정은 순수 마킹(store 안 카운터/키 하나)이지만, elastic 의 증설은 **워크트리 같은 실물 자원을 자동으로 만들어내는 부수효과**를 동반한다. 실행 환경이 강한 서버인지 약한 노트북인지 tasty 는 알 방법이 없으므로, 부수효과를 동반하는 자동 증설을 기본값으로 깔지 않는다 — pool 을 선언하는 쪽(오케스트레이터/사용자)이 매번 명시적으로 켠다.

**원자성**: 새 candidate 이름 합성은 `LeaseStore::acquire_any` 한 호출 안에서(카운터 읽기 → 스캔 → 필요시 카운터 +1 → 새 이름으로 acquire) 순차 수행된다. 이 호출 전체가 `RunnerContext::with_memory` 클로저 하나 안에서 실행되므로(기존 `try_acquire_lease` 관례와 동일), 그 클로저가 프로세스 전역 `Mutex`(`Core::memory`)를 처음부터 끝까지 쥔 채 진행된다 — 워크스페이스마다 runner thread 가 정확히 하나뿐이라 워크스페이스 내부 경쟁이 없고, 워크스페이스 간 경쟁은 그 전역 락 하나로 직렬화된다. 별도 CAS/락 primitive 를 새로 만들지 않는다.

**합성된 candidate 의 재사용**: 카운터는 "지금까지 합성된 개수의 상한"일 뿐 현재 점유 개수가 아니다. 매 `acquire_any` 호출이 `candidates ++ (합성된 이름 전체)`를 다시 스캔하므로, 합성됐다가 release 된 이름은 다음 배정에서 빈 자리로 재발견돼 재사용된다 — 카운터가 오르는 건 그 스캔에서도 빈 자리가 전혀 없었을 때뿐이다.

**dispatch 시점 치환**: 배정된 resource 식별자는 `dispatch_command` 호출 직전 `task.command` 에 주입된다(`substitute_lease_resource`, `src/core/agent/runner_host.rs`).

- `TaskCommand::Run`: `cwd` 가 `None` 이면 곧장 그 resource 경로로 채운다(가장 흔한 용법 — "이 후보에서 실행해라"). `cwd`/`command` 인자 안에 `${lease.resource}` placeholder 가 있으면 그 부분만 실제 resource 로 치환한다(원래 값을 통째로 덮지 않음).
- `TaskCommand::Custom.params`: JSON 트리 전체를 재귀적으로 훑어 문자열 값 안의 `${lease.resource}` 를 치환한다(예: `claude.spawn` 의 `cwd` 파라미터).
- `${lease.resource}` 는 두 placeholder 중 하나다 — 선행 task 의 출력을 파라미터로 넘기는 `${task.<id>.output<pointer>}` 는 위 "선행 task 출력을 파라미터로 넘기기" 참조. 둘은 같은 dispatch 지점에서 lease → 출력 순으로 적용된다.
- elastic 으로 새로 합성된 이름이 가리키는 실제 워크트리를 만드는 건(예: `git worktree add`) task 의 Run 커맨드 쪽 자가 프로비저닝 책임이다 — lease primitive 는 포트/임시디렉토리/GPU 슬롯 등에도 쓰이는 범용 도구라 워크트리 특화 side-effect 를 store 안에 넣지 않는다.

**주의 — `cwd` 를 배정된 resource 로 직접 채우는 방식은 elastic 자가 프로비저닝과 함께 쓸 수 없다.** `cwd` 는 프로세스 spawn(`chdir`)이 그 경로로 실제로 이동을 시도하는 시점에 적용되는데, 이 chdir 은 커맨드가 실행되기 **이전에** OS 레벨에서 일어난다 — 아직 디스크에 없는 합성 경로를 `cwd` 로 주면 `Run spawn 'sh': No such file or directory` 로 spawn 자체가 즉시 실패하고, `command` 안에 `mkdir -p` 를 아무리 앞세워도 그 스크립트조차 실행되지 못한다(라이브로 재현 확인). 자가 프로비저닝이 필요하면 `cwd` 는 항상 존재하는 고정 경로(예: `/tmp`, 부모 워크트리 루트)로 지정해 두고, `${lease.resource}` 는 **`command` 인자 쪽**에서 받아 그 안에서 `mkdir -p`/`cd` 를 수행한다(아래 elastic 라이브 검증 예 참조).

**release**: pool 모드로 얻은 자원도 일반 lease 와 동일하게 `release_lease`(task 종결 시 자동 호출)로 반환된다 — `held_leases` 가 이미 "실제로 받은 resource 문자열"만 저장하므로 pool 여부와 무관하게 그대로 동작한다(재설계 불필요).

CLI 편의 플래그(`--concurrency-limit` 대구)는 만들지 않았다 — `--metadata '{"lease":{"candidates":[...],"elastic":{...}}}'` 를 직접 쓰는 것으로 충분하다고 판단(부차적 스코프).

라이브 검증 예(3개 candidates 에 5개 task, fixed):

```sh
mkdir -p /tmp/wt-1 /tmp/wt-2 /tmp/wt-3
for i in 1 2 3 4 5; do
  tasty agent task-create --workspace-id 1 --name "t$i" \
    --command '{"kind":"run","command":["pwd"]}' \
    --metadata '{"lease":{"candidates":["/tmp/wt-1","/tmp/wt-2","/tmp/wt-3"]}}'
done
tasty agent task-run --workspace-id 1 --action start
tasty agent task-list --workspace-id 1   # 3개만 running(서로 다른 cwd), 2개는 ready(대기)
tasty agent task-get --workspace-id 1 --id t-...   # command.cwd 로 실제 배정된 candidate 확인
```

elastic 을 켜면 5개 모두 대기 없이 동시 실행되고, 그중 2개는 `/tmp/wt-3-overflow-1`/`-2` 처럼 아직 디스크에 없는 합성 경로를 받는다 — 위 "주의" 대로 `cwd` 를 그 합성 경로로 직접 채우면(fill-when-absent) spawn 이 즉시 실패하므로, `cwd` 는 존재하는 고정 경로에 두고 `${lease.resource}` 를 command 인자로 받아 스스로 `mkdir -p`/`cd` 하게 한다:

```sh
mkdir -p /tmp/wt-1 /tmp/wt-2 /tmp/wt-3
for i in 1 2 3 4 5; do
  tasty agent task-create --workspace-id 1 --name "e$i" \
    --command '{"kind":"run","command":["sh","-c","mkdir -p \"$1\" && cd \"$1\" && pwd","_","${lease.resource}"],"cwd":"/tmp"}' \
    --metadata '{"lease":{"candidates":["/tmp/wt-1","/tmp/wt-2","/tmp/wt-3"],"elastic":{}}}'
done
tasty agent task-run --workspace-id 1 --action start
tasty agent task-list --workspace-id 1   # 5개 모두 즉시 running, 대기 없음
tasty agent lease-list --workspace-id 1  # 3개 원본 + wt-3-overflow-1/-2 총 5개 holder
```

`cwd:"/tmp"` 는 항상 존재하므로 spawn 은 무조건 성공하고, `sh -c '...' _ ${lease.resource}` 에서 `$1` 로 실제 배정된(원본 또는 합성) 경로를 받아 그 안에서 직접 생성·이동한다 — `${lease.resource}` 를 `cwd` 자체에 두는 방식(위 "주의")과 달리 이 패턴은 elastic 로 새로 합성된, 아직 존재하지 않는 경로에도 그대로 쓸 수 있다.

### 호스트 재시작 정화 + 핸들 영속

`held_permits`/`held_handles` 는 in-memory only이라 재시작 시 비지만, store 의 holders/handle 은 영속이라 leak 가능. `purge_and_reload_on_restart`(`src/core/agent/runner_thread.rs`)로 묶여 있고, **runner thread 없이도** 호출 가능하다 — 부팅 경로(위 "재시작 계약")와 `run_loop` 진입부(수동/plugin start) 양쪽이 이 함수 하나를 공유한다:

- `purge_stale_{semaphore,lease}_holders` — Running task 중 `metadata.*.holder == task.id` 만 release + task=Failed("host restart").
- `reload_persistent_handles`(key `tasty.agent.handle.<task_id>`, workspace scope) — `ShellProcess` 는 `process_alive::is_alive(pid)` 검사(alive 복원 / dead 는 영속 `run_result` 로 정확한 exit_code 마감 또는 Failed). `PolledDispatch`/`BarrierPoll` 은 insert-only 복원(다음 tick poll). PolledDispatch 첫 poll 이 injector 미준비면 `INJECTOR_GRACE_MS=30s` 안에서 Active 유지. `AwaitExternal { deadline_ms, .. }` 은 `deadline_ms` 가 이미 지났으면 즉시 `Failed`(구 포맷도 `deadline_ms` 기본값 0 이라 이 분기), 아직이면 insert-only 복원 — 단 poll 이 절대 관여하지 않는 계약이라 다음 재시작 전까지는 deadline 이 재판정되지 않는다(위 "재시작 계약" 참조).

`ReduceImmediate`/`CustomImmediate`/`ImmediateFail` 은 영속 안 함(다음 tick 즉시 흡수 + reload 시 재dispatch side-effect 위험).

### rate_limit 미들웨어

`src/adapters/ipc/handler.rs` 의 미들웨어 체인:

```text
ensure_allowed → check_cap_block → rate_limit_try_consume → record_ipc_call → audit Allow → route
```

차단 시 `-32010 throttled: tokens_left=N` + audit Deny. **면제**(`should_rate_limit`): `Local`(사용자 직접) · host 자기 호출(`_host`) · `telemetry.*`(재귀 폭주 방지) · `agent.rate_limit_*`(자가 회복 경로 — 막히면 영구 차단) · `system.info`. 미등록 `(agent, metric)` 은 면제(opt-in 모델). store 접근 실패는 fail-open(warn 후 통과 — 인프라 고장으로 전 IPC 차단은 과도).

## CLI 예

```sh
tasty agent task-run --workspace-id 1 --action start    # 시작(실행 중이면 no-op)
tasty agent task-run --workspace-id 1                    # 상태 조회
tasty agent task-run --workspace-id 1 --action stop      # 중단(자식 프로세스는 생존)
```

`--action` 은 `clap::ValueEnum { Start, Stop, Status }` — 오타는 CLI 시점 거부.

```sh
tasty agent task-list --workspace-id 1                                   # 전체
tasty agent task-list --workspace-id 1 --state running                   # 단일 state
tasty agent task-list --workspace-id 1 --state waiting,ready,running     # 콤마 다중값 = "아직 안 끝난 task"
```

`task-list --state` 는 `task-purge --states` 와 **동일한 콤마 다중값 파싱**을 쓴다(플래그 이름만 단수/복수로 다르다 — 하위호환 때문에 유지). 여러 state 는 OR 매칭이고, 단일값은 예전과 같이 동작한다. IPC(`agent.task_list` / `agent.task_purge`)도 배열 `["waiting","ready"]` · 콤마 문자열 `"waiting,ready"` · 단일 문자열을 모두 받고, 단수/복수 키(`state` ↔ `states`)를 서로 폴백한다 — 콤마 목록을 단일값 필터로 넘겨 매칭이 0 건이 되고 "아직 안 끝난 task 가 있는가" 판정이 조용히 무력화되던 함정을 없앤 것. 매칭이 없으면 빈 목록, 필터 미지정이면 전체다.

**단, "빈 값" 의 의미는 두 커맨드가 다르다.** `task_list` 는 빈 값(`[]` / `""` / 콤마만)을 "필터 없음"(= 전체)으로 접지만, `task_purge` 는 **키가 있는데 이름이 하나도 없으면 "매칭 없음"** 으로 본다 — 즉 `{"states": [], "older_than_ms": ...}` 는 아무것도 지우지 않는다. 상태 목록을 동적으로 조립하는 호출자가 상태를 하나도 안 골랐을 때, 파괴적 명령에서 그걸 "상태 무관 전체 삭제" 로 승격시키지 않기 위해서다. `states` 키가 **아예 없거나 `null`** 이면 그건 그대로 "상태 필터 없음" 이므로 `older_than_ms` 만으로 전체가 후보가 된다(둘 다 미지정이면 `Core::task_purge` 가 거부).

```sh
tasty agent task-create --workspace-id 1 --name build --command '{"kind":"run","command":["cargo","build"]}' \
  --concurrency-limit cap2   # metadata.semaphore.name=cap2 를 자동 조립(위 "동시성 제한" 참조)
```

`--concurrency-limit` 는 `--metadata` 에 `metadata.semaphore = { name: <값> }` 하나를 채워 넣는 단축일 뿐이다 — `--metadata` 를 이미 쓰고 있으면 그 JSON 객체에 `semaphore` 키를 병합하고(다른 키는 보존), `--metadata` 가 이미 `semaphore` 를 담고 있으면(어느 쪽을 취할지 모호) 에러로 거부한다. `holder` 지정 등 더 세밀한 제어가 필요하면 이 플래그 대신 `--metadata '{"semaphore":{"name":"...","holder":"..."}}'` 를 직접 쓴다.

```sh
tasty agent dag-list                                   # 살아있는 전 workspace 의 DAG
tasty agent dag-list --workspace-id 1                  # 한 workspace 로 한정
tasty agent dag-list --include-tasks                   # 각 DAG 에 task id 목록 동봉
tasty agent dag-get --id c:t-1716800000123-1           # 그 DAG 만의 nodes/edges
tasty agent dag-get --id d:build --workspace-id 1 --format dot
```

DAG 는 영속 레코드가 아니라 `metadata.dag`(explicit, id `d:<값>`) 또는 그래프 연결성
(derived, id `c:<root task id>`)에서 도출된다 — 같은 task 집합이면 id 가 항상 같다.
`dag-list` 는 `--workspace-id` 를 생략하면 *살아있는* workspace 전부를 순회하며
(응답 `scope: "live_workspaces"`), 삭제된 workspace 에 남은 고아 task 는 열거하지
않는다(그건 아래 "자동 GC" 의 몫). 도출 규칙과 응답 필드는
[features/agent-collaboration](../features/agent-collaboration/index.md) 의 Task DAG 항목.

한 workspace 안에서 무관한 그래프를 여럿 돌릴 때 그룹을 명시하려면 생성 시
`metadata` 로 태깅한다:

```sh
tasty agent task-create --workspace-id 1 --name build --command '{"kind":"run","command":["true"]}' \
  --metadata '{"dag":"release","dag_name":"Release pipeline"}'
```

```sh
tasty agent task-delete --workspace-id 1 --id t-...              # 참조 있으면 거부 + 참조자 목록
tasty agent task-delete --workspace-id 1 --id t-... --cascade    # 참조자까지 함께 삭제
tasty agent task-purge --workspace-id 1 --states succeeded,failed --older-than-ms 604800000 --dry-run
```

## 한계

호스트가 ShellProcess spawn 과 watcher 완료 영속 사이에 죽으면 자식이 init(1) reparent 되어 exit_code 손실 → reload 시 `Failed("exit_code unknown")`. (cross-platform 으로 회피 불가.)

같은 이유로 **캡처한 stdout/stderr 도 유실된다** — 자식은 호스트 재시작 후에도 살아남지만(수명 계약은 그대로 유지), 파이프를 들고 있던 드레인 스레드는 호스트와 함께 사라지므로 그 사이의 출력은 다시 읽을 방법이 없다. 장시간 작업(빌드/배포)의 결과 보존이 중요해지면 `pty.*` 기반 별도 경로가 더 맞다.

## 관련

- [telemetry › AgentId](../features/telemetry/index.md#agentid--agent-식별) — `AgentId` 도출 · [reference/api](../reference/api.md) — agent namespace
