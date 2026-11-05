<a id="작업-dag"></a>

# 작업 순서 관리 (DAG)

작업을 등록하고 의존 관계를 연결하면 먼저 끝나야 할 단계에 맞춰 실행할 수 있습니다. `task-run`, `task-create`, `task-await`로 작업 하나를 실행한 뒤 순서와 실패 처리를 연결하세요.

작업은 `tasty` CLI로 만들고 관리합니다. 처음이라면 [러너 켜기](#러너-켜기) → [작업 만들기](#작업-만들기) → [결과 확인하기](#결과-확인하기) 세 절을 따라 작업 하나를 `task-run`·`task-create`·`task-await` 로 끝까지 실행해 보세요. 그 뒤의 순서 연결·결과 전달·분기·동기화는 필요할 때 읽으면 됩니다. 실행 상황을 화면에서 확인하려면 [진행 보기](#진행-보기)를 참고하세요.

각 명령에는 작업이 속한 워크스페이스를 `--workspace`로 지정합니다(예전 표기 `--workspace-id`도 받습니다). 다른 워크스페이스를 보고 있어도 지정한 곳에서 작업합니다.

## 러너 켜기

작업을 만들어도 러너가 꺼져 있으면 아무것도 실행되지 않습니다. 워크스페이스마다 한 번 켭니다.

```sh
tasty agent task-run --workspace 2 --action start
tasty agent task-run --workspace 2 --action status
tasty agent task-run --workspace 2 --action stop
```

tasty 를 재시작하면 러너는 자동으로 켜지지 않습니다. 작업 자체는 남아 있으니 다시 `start` 하면 이어서 진행합니다. 재시작 전에 외부 완료 신호를 기다리던 작업은 재시작 뒤 그 신호로는 끝나지 않고, 정해 둔 대기 시간이 지나면 실행 중인 러너가 시간 초과 실패로 끝냅니다. 러너가 멈춰 있으면 DAG 화면의 러너 표시에도 그 사실이 나오고, 표시 위에 마우스를 올리면 다시 켜는 명령이 그대로 보입니다. DAG 탭에서는 그 명령이 머리말에 한 줄로도 적힙니다.

## 작업 만들기

```sh
BUILD=$(tasty agent task-create --workspace 2 --name build \
  --command '{"kind":"run","command":["cargo","build"]}' | jq -r .id)
```

만들면 작업 ID 가 돌아옵니다. 이 ID 로 의존 관계를 걸고 상태를 조회합니다.

| 명령 종류 | 하는 일 |
|---|---|
| `run` | 지정한 명령을 실행합니다. 터미널을 차지하지 않는 백그라운드 프로세스이고, 표준 출력과 표준 에러를 각각 끝에서 64KiB 까지 담아 결과에 싣습니다. 대화형 프로그램은 여기에 맞지 않습니다. 환경변수는 Tasty 를 실행한 환경을 받되, 아래 표 다음의 예외가 있습니다 |
| `custom` | tasty 자신의 동작을 작업으로 만듭니다. 자식 에이전트 띄우기처럼 터미널을 만드는 일이 여기 해당합니다. 끝나기를 따로 기다리는 동작이면, 그래프로 보낸 작업은 처음 받은 접수 응답을 결과의 `raw.accepted` 에 최종 응답과 따로 남깁니다 |
| `reduce` | 여러 작업의 결과를 하나로 합칩니다 |
| `wait_barrier` | 배리어에 신호가 다 모일 때까지 기다립니다 |
| `agent` | Claude 나 Codex 같은 에이전트 세션에 지시 하나를 보내고 그 답을 결과로 받습니다. 아래 [에이전트에게 일을 시키고 답을 결과로 받기](#에이전트에게-일을-시키고-답을-결과로-받기)에서 설명합니다 |

명령 JSON 은 파일로 빼서 `--command @build.json` 처럼 넘겨도 됩니다.

`custom` 작업의 완료를 주기적으로 조회한다면 `poll.interval_ms`로 조회 간격을 지정합니다(기본 500ms). 최초 조회는 바로 하고, 이후에는 이전 조회가 끝난 뒤 지정한 시간만큼 기다립니다. 같은 대상을 기다리는 작업도 각각 조회합니다. 러너가 바쁘면 더 늦게 확인할 수 있지만 밀린 조회를 한꺼번에 보내지는 않습니다. `poll.timeout_ms`를 정했다면 다음 조회 시간이 아직 오지 않아도 기한을 확인합니다. 재시작 뒤 러너를 다시 켜면 저장된 간격과 기한으로 조회를 재개하며, 원래 실행 요청은 다시 보내지 않습니다. 0ms는 러너가 진행할 때마다 한 번 조회한다는 뜻입니다.

작업 하나의 정의(명령, 메타데이터, 결과 타입 등)는 JSON 으로 바꿨을 때 768 KiB 까지입니다(메모리 항목 크기 상한을 1 MiB 보다 작게 설정하면 그에 맞춰 줄어듭니다). 넘으면 작업을 만들지 않고 오류로 알립니다. 큰 데이터는 파일로 두고 그 경로를 넘기세요. 실행 직전에 앞 작업의 결과를 넣어 이 크기를 넘게 되면 그 작업은 실행하지 않고 실패로 끝납니다.

`run` 명령, 아래 후처리 명령, 셸 명령으로 결과를 합치는 리듀서는 Tasty 를 실행한 환경을 그대로 받습니다. 다만 Tasty 를 Claude Code 안에서 띄웠다면 그 세션을 가리키는 변수는 넘기지 않고(터미널과 같습니다), 다른 Tasty 의 터미널에서 띄웠다면 그 Tasty 를 가리키는 `TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`·`TASTY_AGENT_ID` 도 넘기지 않습니다. 그래서 명령 안에서 `tasty` 를 부르면 이 Tasty 에 닿습니다.

## 결과 확인하기

```sh
tasty agent task-await --workspace 2 --id "$BUILD"   # 끝날 때까지 기다림 (기본 최대 10분)
tasty agent task-get --workspace 2 --id "$BUILD"     # 상태와 결과 다시 보기
```

`task-await` 응답의 `outcome` 이 `terminal` 이면 작업이 끝난 것이고, `state` 가 `succeeded` 인지 `failed` 인지로 성공 여부를 판단합니다. `timed_out`(기다리는 시간 초과)과 `not_found`(없는 작업)도 종료 코드 0 으로 돌아오므로 종료 코드만으로 판단하지 마세요. `run` 작업의 종료 코드와 출력은 응답의 `result` 에 있습니다. 대기 시간 옵션은 [작업 기다리기와 관리](#작업-기다리기와-관리)에 있습니다.

여기까지가 작업 하나를 실행하고 결과를 확인하는 기본 흐름입니다. 아래 절들은 여러 작업을 순서대로 잇고, 결과를 넘기고, 분기하는 고급 단계입니다.

## 순서와 실패 처리

```sh
TEST=$(tasty agent task-create --workspace 2 --name test \
  --command '{"kind":"run","command":["cargo","test"]}' \
  --depends-on "$BUILD" --on-failure abort | jq -r .id)
```

`--depends-on` 에 적은 작업이 모두 끝나야 이 작업이 준비 상태가 됩니다. 사이클이 생기는 그래프는 만들 때 거부됩니다.

| 실패 정책 | 뜻 | 어디에 붙이나 |
|---|---|---|
| `abort` (기본) | 의존하던 작업이 실패하면 이 작업은 건너뜁니다. 그 아래로도 이어서 건너뜁니다 | 의존하는 쪽 |
| `continue_downstream` | 의존하던 작업이 실패해도 이 작업은 진행합니다 | 의존하는 쪽 |
| `fallback:<작업 ID>` | 이 작업이 실패하면 대신 그 작업을 깨웁니다 | 실패할 수 있는 쪽 |

붙이는 위치를 헷갈리기 쉽습니다. `abort` 와 `continue_downstream` 은 **뒤따르는** 작업에 붙여야 그 작업의 준비 판정에 반영되고, `fallback` 은 반대로 **실패할 수 있는** 작업 자신에 붙여야 합니다. 다른 위치에 지정하면 원하는 실패 처리가 적용되지 않습니다.

Tasty 가 바빠 `custom` 작업의 요청에 제때 답하지 못해도 작업을 바로 실패로 끝내지 않습니다. 상태 확인이 늦으면 다음 확인에서 다시 묻고, 실행되지 않은 채 만료된 요청은 다시 보냅니다. 이미 실행이 시작된 요청(자식 에이전트 띄우기 등)은 두 번 실행되지 않도록 다시 보내지 않고 늦은 응답을 기다립니다. 그 응답을 끝내 받지 못하면(그사이 앱을 다시 시작한 경우 등) 작업은 실패가 아니라 `unknown` 이 됩니다. 요청이 실제로 반영됐는지(자식이 떴는지 등) 확인한 뒤 다시 실행하거나 취소하세요.

폴백으로 쓸 작업은 본 작업보다 먼저 만들어야 합니다. 그 사이에 러너가 먼저 실행해버리는 것을 막으려면 폴백 쪽을 `--reserved-for-fallback` 으로 만듭니다. 예약하면 본 작업을 만들기 전에도 실행되지 않습니다. 본 작업을 연결한 뒤에는 그 작업이 실패해 폴백을 활성화할 때까지 기다립니다.

## 앞 작업의 결과 넘기기

`--depends-on` 은 순서만 묶습니다. 앞 작업의 결과를 뒤 작업의 **입력**으로 넘기려면 자리표시자를 씁니다.

```
${task.<작업 ID>.output}          결과 전체
${task.<작업 ID>.output/stdout/text}   결과 안의 한 값
```

작업을 보낼 때 그 자리에 실제 값이 들어갑니다. 자식 에이전트를 띄우고 그 자식에게 말을 거는 흐름처럼, 만들 때는 알 수 없고 실행해봐야 정해지는 값을 넘길 때 씁니다. 참조하는 작업은 반드시 `--depends-on` 에 적혀 있어야 하고, 아니면 만들 때 거부됩니다. 값의 형태는 유지됩니다. 자리표시자 하나만 있는 문자열은 숫자면 숫자로 바뀝니다.

## 타입을 정한 작업 묶음 한 번에 보내기

결과의 타입까지 정해 두고 싶으면 작업들을 그래프 하나로 묶어 보냅니다. 그래프 전체를 먼저 검사하므로, 하나라도 잘못되면 아무 작업도 만들어지지 않고 오류에 어느 작업의 어느 자리가 틀렸는지 나옵니다. 모든 작업이 만들어지기 전에는 어떤 작업도 실행되지 않습니다. 그래프 하나에는 작업을 1000 개까지 담을 수 있고, 더 많으면 여러 그래프로 나눠 보냅니다.

```sh
tasty agent task-graph-submit --workspace 2 --graph @graph.json --dry-run   # 검사만
tasty agent task-graph-submit --workspace 2 --graph @graph.json
```

```json
{"contract_version": 2,
 "tasks": [
   {"id": "build", "command": {"kind": "run", "command": ["cargo", "build"]}},
   {"id": "report", "command": {"kind": "run", "command": ["notify"]},
    "input_schema": {"type": "object", "fields": {"code": {"type": "int64"}}},
    "bindings": {"code": {"from_task": "build"}},
    "input_mapping": {"args": ["/code"]}}]}
```

각 작업의 `id` 는 직접 정합니다. `bindings` 는 입력을 어디서 받을지 정합니다. 고정 값(`literal`), 앞 작업 결과의 한 값(`from_task` 와 `pointer`), 본 작업과 폴백 중 실행된 쪽(`one_of`)을 쓸 수 있고, 타입이 맞지 않으면 보낼 때 거부됩니다. 받은 값은 `input_mapping` 에 적은 자리에만 들어갑니다. `args` 는 명령 뒤에 인자로 하나씩 붙이고, `stdin: true` 는 입력 전체를 JSON 으로 표준 입력에 씁니다. `wait_barrier` 작업은 `"barrier": "/<필드>"` 로 기다릴 배리어 이름을 입력에서 받을 수 있고, 이때는 명령에 `name` 을 적지 않습니다. 값 안의 `$(...)` 나 공백은 다시 해석되지 않습니다. 실제로 넘긴 값은 작업의 `input_snapshot` 에서 볼 수 있습니다. 이 그래프의 작업에는 위 자리표시자를 쓰지 않습니다.

`run` 작업에 이미 있는 파일을 넘기려면 `args` 에 `{"file": "/<필드>"}` 를 씁니다. 그 필드의 문자열이 파일 경로이고, 명령 뒤에 경로가 인자로 붙습니다. 작업이 실행되기 직전에 그 파일을 읽을 수 있는지 확인하며, 없거나 읽을 수 없으면 명령을 실행하지 않고 입력 단계에서 실패합니다. 상대 경로는 명령의 `cwd` 가 있을 때만 그 기준으로 받습니다. Tasty 는 파일을 만들거나 지우지 않으므로, 파일을 준비하고 치우는 일은 앞뒤 작업이나 스크립트가 맡습니다. 아래 스크립트는 diff 를 파일로 만들어 검사 명령에 넘기고, 검사가 실패해도 마지막에 지웁니다.

```sh
#!/bin/sh
# lint-diff.sh <workspace id>
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

작업 결과를 다른 명령(판정·요약 도구 등)으로 한 번 더 처리해 결과로 삼으려면 `postprocess` 를 붙입니다. 본 작업이 성공하면 그 명령을 실행하고, 표준 출력을 작업의 결과로 받습니다.

```json
{"id": "judge",
 "command": {"kind": "run", "command": ["make-draft"]},
 "output_schema": {"type": "enum", "values": ["pass", "revise"]},
 "postprocess": {"command": ["judge-cli", "--json"],
                 "stdin": {"draft": {"from": "raw", "pointer": "/execution/stdout/text"}},
                 "stdout": {"pointer": "/verdict"},
                 "timeout_ms": 120000,
                 "retry": {"max_retries": 1}}}
```

- 명령은 셸 없이 그대로 실행됩니다. 셸이 필요하면 `["sh", "-c", "..."]` 처럼 직접 적습니다. 로그인 정보는 Tasty 를 실행한 환경을 그대로 씁니다. 다만 Tasty 를 Claude Code 안에서 띄웠다면 그 세션을 가리키는 변수는 넘기지 않습니다(터미널과 같습니다). Tasty 를 다른 Tasty 의 터미널에서 띄웠다면 그 Tasty 의 터미널을 가리키는 `TASTY_SURFACE_ID` 같은 변수도 넘기지 않습니다.
- `stdin` 은 표준 입력에 쓸 JSON 의 필드를 정합니다. `from` 은 `input`(작업 입력), `raw`(본 작업 결과) 이고 `pointer` 로 그 안의 값을 고릅니다.
- 표준 출력은 기본적으로 JSON 값 하나여야 합니다. `"stdout": {"format": "text"}` 를 쓰면 글자 그대로 받습니다. 진행 로그는 표준 오류로 내보내면 결과에 섞이지 않습니다.
- `cwd` 로 후처리 명령의 작업 폴더를 정합니다. 생략하면 본 작업의 `cwd`, 그것도 없으면 Tasty 를 실행한 폴더입니다.
- `timeout_ms` 는 반드시 적습니다. 시간이 지나거나 작업을 취소하면 그 명령과 그 명령이 띄운 프로세스를 끝냅니다.
- `retry` 를 적으면 실패했을 때 본 작업은 다시 하지 않고 후처리만 그 횟수만큼 다시 실행합니다. `max_retries` 는 1~10 이고, `delay_ms` 로 다시 실행하기 전에 기다릴 시간을 정합니다(기본 0, 최대 3600000). 횟수를 다 쓰면 작업은 실패로 끝나고, 그 뒤 작업은 일반 실패와 같이 실패 정책을 따릅니다. 후처리만 따로 다시 실행하는 방법은 없습니다. 다시 하려면 `task-retry` 로 본 작업부터 다시 실행합니다.
- 타입을 정한 작업이 실행 중이면 `task-list` 줄 끝과 `task-get` 의 `phase:` 줄에 단계가 나옵니다. 본 작업 중이면 `executing` 입니다.
- 후처리는 `run`·`custom` 작업에 붙입니다. `reduce`·`wait_barrier` 작업에는 붙일 수 없습니다.
- 단계가 바뀌어도 이벤트는 나가지 않습니다. 어느 지점에서 알림이 필요하면 그 작업 뒤에 `["tasty", "notify", "build finished"]` 나 webhook 을 부르는 `curl` 같은 `run` 작업을 끼워 넣습니다.
- 후처리가 끝날 때까지 작업은 실행 중으로 보이고 다음 작업은 기다립니다. `task-get` 은 `state: running` 다음 줄에 `phase: postprocessing (run 1)`(재시도를 기다리는 중이면 `phase: retry_wait (run 2)`)을 보여 줍니다.
- 후처리가 실패하면 `task-get` 의 `state` 줄에 이유가 나오고, 그 아래에 `postprocess: run 2 failed (nonzero_exit), exit_code 3` 처럼 마지막 실행의 원인과 종료 코드, 재시도로 넘어간 실행의 원인(`postprocess retried after: ...`)이 나옵니다. 후처리 결과가 너무 커서 저장하지 못하면, 성공한 후처리는 `task-get` 의 원본 결과에서 본 작업 응답을 빼고 성공으로 남깁니다. 그래도 넘치거나 실패한 실행이면 원인은 `result_too_large` 이고, 재시도가 남아 있어도 다시 실행하지 않습니다.
- 후처리가 도는 중에 Tasty 를 끄면 그 후처리 명령과 그 명령이 띄운 프로세스를 끝냅니다. 다시 시작한 뒤 그 후처리는 다시 실행되지 않고 작업은 실패로 끝납니다. 러너를 멈출 때(`task-run --action stop`)도 후처리를 같은 방식으로 끝내며, 러너를 다시 켤 때까지 작업은 실행 중으로 보입니다. 다시 하려면 `task-retry` 로 작업을 다시 실행합니다.
- Tasty 가 강제로 끝나면(`kill` 등) 운영체제마다 다릅니다. Linux 에서는 후처리 명령 자체에만 종료 신호가 갑니다. 보통 그 명령이 끝나므로 다시 시작한 뒤 작업은 결과를 받을 수 없어 실패하고, 그 명령이 띄운 다른 프로세스는 남을 수 있습니다. 예를 들어 `["sh", "-c", "sleep 300; echo x"]` 이면 `sh` 는 끝나고 `sleep` 이 남습니다. Windows 에서는 함께 끝나며, macOS 에서는 모두 남습니다.
- 다시 시작한 뒤에도 후처리 명령이 아직 실행 중이면(macOS 에서 강제로 끝났거나 Linux 에서 명령이 종료 신호를 무시한 경우), 그 명령이 끝날 때까지 작업은 실행 중으로 남고 세마포어·lease 도 계속 쥡니다. 명령이 끝나면 결과를 받을 수 없어 실패로 끝내고 그때 자원을 내놓습니다. 명령이 띄운 다른 프로세스만 남은 경우는 기다리지 않습니다. 후처리를 기다리던 작업(다시 시도 대기 포함)은 자원을 쥔 채로 예정대로 후처리를 실행합니다.

Tasty 가 메모리 파일을 열지 못해 [임시 메모리](cli-reference.md#에이전트가-함께-쓰는-메모리)로 동작하는 동안에는 그래프가 재시작 뒤에 남지 않으므로 그냥 보내면 거부됩니다. 재시작하면 사라져도 괜찮은 그래프는 `"durability": "best_effort"` 를 넣어 보냅니다.

### 결과에 따라 다음 작업 고르기

작업에 `transitions` 를 적으면 그 작업의 결과를 보고 다음에 실행할 작업을 고릅니다. 아래 예에서 `review` 의 판정이 `pass` 면 `ship`, `revise` 면 `fix`, 그 밖이면 `human` 만 실행됩니다.

```json
{"id": "review", "command": {"kind": "run", "command": ["review-tool"]},
 "output_schema": {"type": "object", "fields": {
   "verdict": {"type": "enum", "values": ["pass", "revise", "review"]}}},
 "transitions": {
   "cases": [
     {"when": {"compare": {"path": "/verdict", "op": "eq", "value": "pass"}}, "to": ["ship"]},
     {"when": {"in": {"path": "/verdict", "values": ["revise"]}}, "to": ["fix"]}],
   "otherwise": ["human"]}}
```

- 조건은 결과의 한 자리(`path`)를 값과 비교하거나(`compare`: `eq`·`ne`·`lt`·`le`·`gt`·`ge`), 값 목록에 드는지 봅니다(`in`). `all`·`any`·`not` 으로 묶을 수 있습니다. 결과의 타입에 맞지 않는 조건은 보낼 때 거부됩니다. 소수는 범위로만 비교합니다.
- 결과를 모으는 `reduce` 작업(전략 `all`)에서는 조건에 `"input": "<입력 작업 id>"` 를 더해, 모은 결과 대신 그 입력 작업의 결과를 그 작업의 결과 타입으로 읽을 수 있습니다.
- 기본으로는 맞는 조건이 하나여야 합니다. 둘 이상 맞으면 그 작업이 실패로 끝납니다. 맞는 것을 모두 실행하려면 `"mode": "all_matches"` 를 넣습니다.
- 아무 조건도 맞지 않을 때 실행할 작업(`otherwise`)이나 아무것도 실행하지 않음(`"no_match": "finish"`)을 반드시 적습니다.
- 고르지 않은 작업과 그 뒤로만 이어진 작업은 실패가 아니라 "선택되지 않음"으로 끝납니다. 갈래가 다시 만나는 작업은 실제로 실행된 갈래만 기다립니다. 실행된 갈래가 실패하면 그 실패는 그대로 전해집니다.
- 실행되지 않을 수 있는 갈래의 결과를 꼭 필요한 입력으로 받으면 보낼 때 거부됩니다. 갈래마다 다른 작업이 값을 내면 `one_of` 로 받고, 없어도 되는 값이면 입력 필드를 optional 로 하거나 기본값을 줍니다.
- 작업의 `route` 에 고른 작업이, 선택되지 않은 작업의 `skip` 에 그 이유가 남습니다. 모든 작업이 성공했거나 선택되지 않았으면 DAG 는 성공으로 표시됩니다. 실패한 작업도 폴백이 대신 성공했으면 DAG 를 실패로 만들지 않습니다.
- 한 갈래가 실패해도 다른 갈래가 진행할 수 있는 동안 DAG 는 진행 중 상태(실행 중·준비·대기)로 보입니다. 결과를 알 수 없게 된(`unknown`) 작업은 다시 실행하거나 취소해야 진행되므로 진행 중으로 치지 않습니다. 모두 끝났을 때 끝까지 성공한 갈래가 있으면 DAG 상태는 `partially_failed`(일부 실패), 없으면 `failed` 입니다. 앞 작업만 성공하고 마지막 작업이 실패하거나 건너뛰어진 한 줄짜리 흐름은 `failed` 입니다. 실패했지만 폴백이 대신 성공한 마지막 작업은 성공으로 칩니다. `agent dag-list` 의 `rollup_state` 에 나오고, 작업 DAG 목록과 DAG 탭에는 `◒` 표지와 **일부 실패** <!-- en: Partially failed --> 라벨로 보입니다.

### 에이전트에게 일을 시키고 답을 결과로 받기

`agent` 작업은 Claude 나 Codex 같은 에이전트 세션에 지시 하나를 보내고, 그 답을 작업의 결과로 받습니다. 결과는 다음 작업의 입력이나 위의 갈래 조건에 그대로 쓸 수 있습니다.

```json
{"id": "review",
 "command": {"kind": "agent", "provider": "claude", "workspace_id": 2,
             "session": {"kind": "existing", "surface_id": 12},
             "instruction": "이 브랜치의 변경을 검토해 주세요",
             "timeout_ms": 1800000},
 "output_schema": {"type": "enum", "values": ["approve", "revise"]}}
```

- `provider` 는 세션을 맡을 플러그인의 이름입니다. Claude 플러그인은 `claude`, Codex 플러그인은 `codex` 이고, 에이전트 작업을 지원하는 다른 플러그인도 자기 이름으로 쓸 수 있습니다. 그 플러그인이 설치돼 있지 않거나, 꺼져 있거나, 턴 보고 권한이 없으면 작업을 만들 때 이유와 함께 거절됩니다. 만든 뒤에 플러그인을 끄면 그 작업은 실행할 때 실패합니다.
- `command` 의 `workspace_id` 는 새 세션을 띄울 워크스페이스입니다. 값을 내는 `task-submit` 은 작업이 있는 워크스페이스로 보내며, 지시문 끝에 붙는 안내에 그 번호가 들어 있습니다.
- `session` 이 `{"kind": "new", "parent_surface": <서피스>}` 면 그 서피스 아래에 새 세션을 띄워 지시합니다. 여기에 `"cwd": "<경로>"` 를 더하면 새 세션의 작업 폴더가 됩니다. `{"kind": "existing", "surface_id": <서피스>}` 면 이미 열린 세션에 보냅니다.
- 이미 열린 세션은 비어 있을 때만 지시를 받습니다. 직접 대화하는 중이면 끝날 때까지 기다리므로 대화가 섞이지 않습니다. 지시 끝에는 그 실행을 알아보는 표지 한 줄이 붙고, 그 표지로 시작한 턴만 작업의 턴으로 봅니다. 같은 세션을 쓰는 작업은 차례로 실행됩니다.
- 결과 타입을 적지 않으면 그 턴의 마지막 답변이 문자열 결과가 됩니다.
- Claude 세션에 Stop 게이트를 붙였다면 게이트가 응답을 이어 가게 하는 동안은 턴이 끝나지 않습니다. 게이트가 응답을 마치게 한 뒤의 마지막 답변으로 작업이 끝납니다. 지시에 대한 첫 답이 필요하면 결과 타입을 정해 `task-submit` 으로 내게 하세요.
- 위 예처럼 결과 타입을 정하면 에이전트가 `tasty agent task-submit` 으로 값을 내야 합니다. 지시문 끝에 내는 방법과 타입, 그 실행에만 쓰는 토큰이 자동으로 붙습니다. 토큰이 다른 값은 받지 않으므로 다른 작업이나 지난 실행에 실수로 낸 값이 섞이지 않습니다. 낸 값은 바로 검사하고, 턴이 끝날 때 결과가 됩니다. `revise` 같은 판정도 성공한 결과입니다.
- 입력을 받는 작업은 `"input_mapping": {"input_block": true}` 를 적습니다. 입력이 지시문 뒤에 JSON 으로 붙습니다.
- 답을 내지 않고 턴이 끝나거나, 턴이 오류로 끝나거나, 세션이 끝나거나, `timeout_ms` 가 지나면 작업은 실패합니다. 실패 이유는 `task-get` 의 결과에 구분돼 나옵니다. 값을 내지 않고 턴이 끝났으면 그 턴의 마지막 답이 앞 2000자까지 실패 기록에 남습니다.
- 에이전트가 권한 승인 같은 입력을 기다리면 작업은 실행 중으로 남고, `task-get` 이 `phase: awaiting_input` 줄과 `agent session: claude surface 12, awaiting input since …` 줄을 보여 줍니다.
- Tasty 가 바빠 이미 열린 세션에 지시를 보냈다는 응답이 늦어도 지시를 다시 보내지 않습니다. 같은 지시가 두 번 가지 않도록 늦은 응답을 기다리고, 아직 보내지 않은 것이 확실할 때만 다시 보냅니다. 보냈는지 알 수 없게 되면 작업은 실패가 아니라 `unknown` 이 됩니다. 세션에서 지시를 받았는지 확인한 뒤 다시 실행하거나 취소하세요.
- 작업이 끝나거나 취소돼도 세션은 닫히지 않습니다. Tasty 를 다시 시작하면 실행 중이던 agent 작업은 실패로 끝납니다. 지시를 보냈다는 응답을 기다리던 작업은 `unknown` 이 됩니다.
- 새 세션이 뜨기를 기다리는 동안에도 다른 작업은 진행됩니다. 새 세션이 30초 안에 뜨지 않으면 작업은 실패합니다. 그 뒤에 늦게 뜬 세션은 작업이 닫습니다. 기다리는 동안 작업을 취소해도 늦게 뜬 세션을 닫습니다. 이미 열려 있던 세션은 닫지 않습니다. 기다리는 동안 Tasty 를 다시 시작하면 작업은 `unknown` 이 됩니다.
- Codex 를 네트워크가 막힌 샌드박스에서 실행하면 `task-submit` 이 Tasty 에 닿지 못해, 결과 타입을 정한 작업은 실패합니다. 결과 타입을 정하지 않은 작업(마지막 답변이 결과)은 영향이 없습니다.
- 이미 있는 작업의 결과를 Codex 부모에게 자동으로 전달하려면 [Codex 부모에게 자동으로 전달하기](claude-codex.md#codex-부모에게-자동으로-전달하기)를 참고하세요.

### 예: 구현하고 검토한 뒤 판정대로 갈라졌다 다시 모이기

위의 기능을 하나로 엮은 그래프입니다. `implement` 가 바꾼 내용을 문장으로 내면 `review` 가 그 문장을 입력으로 받아 판정과 확신도를 냅니다. 확신도가 0.9 이상인 `pass` 면 `ship`, 확신도가 0.9 이상인 `revise` 면 `fix`, 그 밖이면 `human_review` 로 가고, 세 갈래는 `report` 에서 다시 만납니다. `make-change`·`review-tool` 은 각자의 도구로 바꿉니다. 에이전트에게 맡기려면 위의 `agent` 작업으로 바꿉니다.

```json
{"contract_version": 2,
 "types": {"ReviewResult": {"type": "object", "fields": {
   "verdict": {"type": "enum", "values": ["pass", "revise", "review"]},
   "confidence": {"type": "float64", "min": 0, "max": 1}}}},
 "tasks": [
  {"id": "implement", "command": {"kind": "run", "command": ["make-change"]},
   "output_schema": {"type": "string"},
   "postprocess": {"command": ["summarize-tool"], "timeout_ms": 10000,
                   "stdin": {"text": {"from": "raw", "pointer": "/execution/stdout/text"}}}},
  {"id": "review", "command": {"kind": "run", "command": ["review-tool"]},
   "input_schema": {"type": "object", "fields": {"summary": {"type": "string"}}},
   "bindings": {"summary": {"from_task": "implement"}},
   "input_mapping": {"args": ["/summary"]},
   "output_schema": {"ref": "ReviewResult"},
   "transitions": {"cases": [
     {"when": {"all": [{"compare": {"path": "/verdict", "op": "eq", "value": "pass"}},
                       {"compare": {"path": "/confidence", "op": "ge", "value": 0.9}}]}, "to": ["ship"]},
     {"when": {"all": [{"compare": {"path": "/verdict", "op": "eq", "value": "revise"}},
                       {"compare": {"path": "/confidence", "op": "ge", "value": 0.9}}]}, "to": ["fix"]}],
     "otherwise": ["human_review"]}},
  {"id": "ship", "command": {"kind": "run", "command": ["ship-tool"]}},
  {"id": "fix", "command": {"kind": "run", "command": ["fix-tool"]}},
  {"id": "human_review", "command": {"kind": "run", "command": ["notify-reviewer"]}},
  {"id": "report", "command": {"kind": "run", "command": ["report-tool"]},
   "depends_on": ["ship", "fix", "human_review"]}]}
```

`review` 가 확신도 0.95 로 `pass` 를 내면 `task-get` 은 이렇게 보여 줍니다.

```text
state: succeeded
route: ship
attempt: review#1
revision: 7
input: summary <- implement (attempt implement#1)
output: {"confidence":0.95,"verdict":"pass"} (from postprocess.stdout.json)
```

- `input:` 줄은 어느 작업의 몇 번째 실행 결과를 받았는지, `output:` 줄은 결과가 어디서 왔는지 알려 줍니다. 결과 없이 끝난 작업은 `output: none` 과 함께 실패 단계와 이유가 `error:` 줄에 나옵니다.
- `fix`·`human_review` 는 `skip: branch_not_selected` 로 끝나고, `report` 는 실행된 `ship` 만 기다렸다가 성공합니다.
- `review` 가 실패하면 세 갈래와 `report` 는 모두 건너뜀이 되고, `skip: upstream_unavailable (review failed)` 처럼 원인 작업이 나옵니다.
- `revision` 은 작업 기록이 저장될 때마다 커집니다. 같은 작업의 두 조회 결과 중 어느 쪽이 나중 것인지 가릴 때 씁니다.
- `confidence` 는 작업이 결과 타입에 선언한 값이라 갈래 조건에 쓸 수 있습니다. 터미널 상태 조회가 돌려주는 `confidence` 는 세션 상태 판단의 확실성이므로 이 값과 다릅니다.

타입을 정하지 않은 작업도 같은 워크스페이스에서 함께 쓸 수 있습니다. `task-create --depends-on report` 로 만든 작업은 `report` 가 끝나면 실행됩니다. 다만 `${task.review.output}` 같은 자리표시자로 타입을 정한 작업의 결과를 읽으려 하면 만들 때 거부됩니다. 결과를 넘기려면 받는 작업도 그래프에 넣고 `bindings` 로 받습니다.

## 실행 기록 남기고 보기

타입을 정한 작업 묶음은 사람이 나중에 읽을 실행 기록(report)을 남깁니다. 작업마다 Tasty 가 자동으로 채우는 부분과, 작업이 실행 중에 직접 적는 메모가 있습니다.

- 자동 부분: 끝난 상태, 시작·끝 시각, 실제로 받은 입력과 낸 결과, 실패했으면 단계와 이유, 건너뛰었으면 이유. 셸 명령은 종료 코드, 에이전트 작업은 어느 세션이었는지, 결과 모으기는 방법과 입력 수가 더 나옵니다.
- 메모: 셸 명령 작업, 그 결과를 다듬는 후처리 명령, 직접 짠 결과 모으기 셸은 환경 변수 `TASTY_TASK_REPORT` 를 받습니다. 그 안에서 `tasty agent report append '테스트 3개 건너뜀'` 처럼 적습니다. 이 명령은 성공하면 아무것도 출력하지 않아 작업 결과를 건드리지 않고, 실패할 때만 표준 오류에 사유를 씁니다. 셸 명령은 표준 오류에 `::tasty-report::<메모>` 로 시작하는 줄을 써도 됩니다(그 줄은 표준 오류에도 그대로 남습니다). 에이전트 작업은 지시문 끝에 메모 쓰는 방법을 읽는 명령(`tasty agent report usage`)과 주소가 붙습니다.

```sh
#!/bin/sh
# 셸 명령 작업 안에서
tasty agent report append "검사한 파일 $(ls src | wc -l)개"
echo "::tasty-report::캐시를 다시 만들었음" >&2
```

메모는 작업이 실행 중일 때만 받습니다. 끝난 뒤에 쓰면 거절됩니다. 메모 한 건이 상한(기본 1 KiB)을 넘으면 잘리고 잘렸다는 표시가 붙습니다. 실행 한 번의 메모 합이 상한(기본 16 KiB)을 넘으면 그 뒤 메모는 저장하지 않고 개수만 셉니다. 어느 경우든 명령은 실패하지 않습니다. 두 상한은 **설정** › **기타** › **작업 파이프라인** <!-- en: Task pipeline --> 에서 바꿉니다. 다시 실행(`task-retry`)하면 메모는 실행마다 따로 모입니다. 메모는 기록일 뿐이라 뒤 작업의 입력(`bindings`)으로 받을 수 없습니다.

```sh
tasty agent dag-report d:review-loop                       # 묶음 전체
tasty agent dag-report d:review-loop --task build --attempt 2   # 작업 하나의 두 번째 실행
tasty agent dag-report d:review-loop --include-raw         # 표준 출력·오류도 함께
```

DAG id 는 `tasty agent dag-list` 에서 봅니다. 다시 실행(retry)한 작업의 이전 실행도 끝난 상태·메모와 함께, 다시 실행을 시작할 때 저장한 자동 부분이 나옵니다.

## 진행 보기

작업 그래프는 진행 상황을 확인하는 화면입니다. 작업 생성과 변경은 CLI에서 합니다. 노드의 실행 상태와 작업이 남긴 결과를 함께 확인하세요.

작업이 어떻게 흘러가는지 보는 화면이 둘입니다. 둘 다 같은 데이터를 봅니다.

- **작업 DAG** <!-- en: Task DAGs --> 창 — `Ctrl+Shift+G`(Mac 프리셋에서는 `Cmd+Shift+G`), 또는 사이드바 **도구** <!-- en: Tools --> 메뉴. 목록에서 하나를 골라 잠깐 보고 닫는 용도입니다. 검색과 상태 필터가 있습니다. 하나를 고르면 같은 자리가 그래프로 바뀌고, 맨 윗줄의 돌아가기 화살표 옆에 확대·축소·전체 맞춤·방향 전환과 러너 표시가 놓입니다.
- **DAG 탭** — 탭 하나를 차지하고 계속 띄워 두는 그래프입니다. `tasty new tab --pane <ID> --type dag_graph` 로 열거나, 이미 있는 서피스에서 `Alt+'` 를 눌러 **DAG** 로 바꿉니다. 확대와 축소, 전체 맞춤, 방향 전환이 있고, 노드를 누르면 명령 · 의존성 · 소요 시간 · 종료 코드 · 출력이 보입니다.

그래프의 선은 관계마다 모양이 다릅니다. 순서만 묶은 의존은 실선, 입력을 받는 연결(`bindings`)은 청록 점·파선, 결과에 따른 갈래(`transitions`)는 보라 파선입니다. 고른 갈래는 굵게 그립니다. 고르지 않았거나 실행되지 않는 경로는 불투명도를 낮추지 않고 중립색으로 그리며, 관계에 따른 실선·점파선·파선 모양은 유지합니다. 두 작업 사이에 순서 의존과 입력 연결이 함께 있으면 입력 연결 하나만 그립니다. 고르지 않아 건너뛴 작업에는 **선택 안 됨** <!-- en: Not selected --> 이 붙습니다. 노드에 마우스를 올리면 작업 이름과 상태가 나오고, 건너뛰었거나 결과를 알 수 없거나 입력을 기다리는 작업은 그 이유가 한 줄 더 나옵니다. `task-graph --format dot` 도 같은 구분으로 그립니다.

노드 왼쪽 위의 작은 그림은 작업 종류입니다. 셸 명령은 터미널, 내부 호출은 플러그, 결과 모으기는 겹친 층, 기다림은 자물쇠, Claude 나 Codex 에게 맡긴 한 턴은 말풍선입니다. Claude 와 Codex 는 같은 말풍선을 쓰고, 어느 쪽인지는 노드를 눌러 상세에서 봅니다.

실행 중인 작업은 하는 일에 따라 다르게 보입니다. Claude 나 Codex 세션이 사람의 답을 기다리면 노드 전체가 노랗게 바뀌고 `!` 표지와 **입력 대기** <!-- en: Needs input --> 라벨, 기다린 시간이 나옵니다. 그 노드를 누르면 상세에 **세션 열기** <!-- en: Open session --> 버튼이 있어, 누르면 그 세션의 탭으로 옮겨 가 바로 답할 수 있습니다. 작업 DAG 목록에서 누르면 목록이 닫힙니다. 목록에서도 입력을 기다리는 작업이 있는 DAG 줄에 `! 1 needs input` 이 붙습니다. 결과를 다듬는 후처리 중이거나 다시 시도하려고 기다리는 작업은 실행 중 색 그대로 **후처리 · 1회차** <!-- en: Postprocess · run 1 --> · **재시도 대기 · 2회차** <!-- en: Retry wait · run 2 --> 처럼 몇 번째 실행인지를 붙입니다.

목록의 각 줄과 DAG 탭 맨 윗줄에는 끝난 작업 수/전체 수가 나옵니다. 건너뛴 작업이 있으면 그 뒤에 `· 2 skipped (1 not selected)` 처럼 건너뛴 수가, 그중 갈래가 골라지지 않아 건너뛴 작업이 있으면 괄호 안에 그 수가 붙습니다.

한 워크스페이스에서 서로 무관한 그래프를 여럿 돌려도 됩니다. 목록은 의존 관계로 이어진 덩어리를 하나의 DAG 로 묶어 보여줍니다. 작업에 `--metadata '{"dag":"이름"}'` 을 붙이면 연결 여부와 무관하게 같은 이름끼리 묶입니다.

| 상태 | 뜻 |
|---|---|
| **대기** <!-- en: Waiting --> | 의존하는 작업이 아직 안 끝났습니다 |
| **준비** <!-- en: Ready --> | 실행 조건을 갖췄고 러너를 기다립니다 |
| **실행** <!-- en: Running --> | 실행 중 |
| **성공** <!-- en: Succeeded --> · **실패** <!-- en: Failed --> | 끝났습니다 |
| **취소** <!-- en: Cancelled --> · **건너뜀** <!-- en: Skipped --> | 사람이 취소했거나, 앞이 실패해 건너뛰었습니다 |
| **선택 안 됨** <!-- en: Not selected --> | 갈래에서 다른 쪽이 골라져 실행하지 않았습니다. 실패가 아닙니다 |
| **입력 대기** <!-- en: Needs input --> | 실행 중인 Claude·Codex 세션이 사람의 답을 기다립니다. DAG 상태는 실행 중 그대로입니다 |
| **알수없음** <!-- en: Unknown --> | 결과를 알 수 없습니다. 노드를 누르면 상세에 그 이유가 나옵니다. Tasty 가 다시 시작되는 사이 명령이 끝났거나, 다시 시작한 뒤 이어서 지켜보던 명령이 나중에 끝나 종료 코드를 받지 못한 경우입니다(다시 시작한 Tasty 는 그 명령의 종료 코드를 받을 수 없습니다). 뒤 작업은 기다리고, 다시 실행(`task-retry`)하거나 취소해야 진행됩니다. 세마포어·lease 를 쥔 작업도 같습니다. 명령이 아직 실행 중이면 끝날 때까지 자원을 쥐고 있다가 끝난 뒤 내놓습니다 |

터미널에서 보려면:

```sh
tasty agent dag-list                                   # 모든 워크스페이스의 DAG
tasty agent task-list --workspace 2 --state waiting,ready,running
tasty agent task-get --workspace 2 --id "$BUILD"
tasty agent task-graph --workspace 2 --format dot   # Graphviz 로 그리기
```

`--format dot` 은 Graphviz 에 바로 넘길 수 있는 그래프만 표준 출력에 씁니다(예: `... --format dot | dot -Tsvg > graph.svg`). 순환이 있다는 안내와 러너 상태는 표준 오류에 나옵니다. `dag-get --format dot` 도 같습니다.

`task-list` 와 `task-get` 은 사람이 읽는 줄을 냅니다. 스크립트에서 쓰거나 작업이 받은 입력(`input_snapshot`)과 결과 원본(`typed_result.raw`)까지 보려면 `--json` 을 붙입니다. Tasty 가 돌려준 응답이 그대로 JSON 으로 나옵니다.

<a id="기다리기와-손보기"></a>

## 작업 기다리기와 관리

```sh
tasty agent task-await --workspace 2 --id "$TEST"             # 끝날 때까지 대기
tasty agent task-retry --workspace 2 --id "$TEST"             # 실패·취소·건너뜀·알수없음 작업 재시도
tasty agent task-cancel --workspace 2 --id "$TEST"
tasty agent task-set-result --workspace 2 --id "$MANUAL" --state succeeded
tasty agent task-purge --workspace 2 --states succeeded
```

저장 응답을 잃어도 완료 기록을 다시 확인하면 대기 중인 완료 알림을 복구합니다. 무기한 대기도 확인된 완료를 받습니다. 저장 결과를 모르는 요청은 성공으로 바꾸지 않으며, 확인하려고 명령을 다시 실행하지 않습니다. 재시작 사이 같은 완료 사건이 재전달될 수 있으므로 사건을 처리하는 도구는 `terminal_id`로 중복을 구분하세요. 완료 기록을 읽지 못하면 보존한 뒤 다음 작업 변경이나 재시작 때 다시 확인합니다.

러너가 꺼진 동안 취소한 작업은 완료 알림을 이미 받았어도 실행 정리를 계속합니다. 다른 작업의 응답이 먼저 처리돼도 취소한 명령과 하류 작업의 정리를 빠뜨리지 않습니다. 점유는 종료를 확인한 뒤에만 반환합니다.

- `task-await` 는 기본 10분까지 기다리고 그 안에 안 끝나면 시간 초과로 돌아옵니다. `--timeout-ms 0` 이면 무한정 기다립니다.
- `task-await` 응답의 `outcome` 은 `terminal`(끝남, `state` 에 succeeded·failed 등), `timed_out`(시간 초과), `not_found`(없는 작업) 중 하나입니다. 셋 다 종료 코드 0 이므로 종료 코드로 성공을 판단하지 말고 `outcome` 과 `state` 를 읽으세요. 루트 옵션 `--response-timeout-ms` 는 응답을 기다리는 시간일 뿐 작업 대기 시간이 아닙니다.
- 배리어 상태는 `tasty agent barrier-state` 로 봅니다. 즉시 돌아오며 닫힐 때까지 기다리지 않습니다. 예전 이름 `barrier-await` 도 같은 결과를 줍니다.
- 타입을 정한 작업은 폴백이 이미 실행됐으면 `task-retry` 가 거부됩니다. 본 작업이 다시 성공하면 둘 중 하나를 받는 다음 작업이 값을 둘 보게 되기 때문입니다. 다시 돌리려면 새 작업으로 보냅니다.
- 경로가 선택되지 않아 건너뛴 작업(**선택 안 됨**)은 `task-retry` 가 거부됩니다. 다시 돌려도 같은 이유로 곧장 건너뛰기 때문입니다.
- 타입을 정한 작업에는 `--reset-downstream` 을 쓸 수 없습니다. 다음 작업들은 이미 이전 실행의 실패나 고른 경로대로 끝났기 때문입니다. `task-retry` 는 그 작업만 다시 실행합니다. 뒤따르는 작업까지 다시 돌리려면 새 그래프로 보냅니다.
- `task-set-result` 는 러너가 실행하지 않은 일, 예컨대 사람이 손으로 하는 확인 절차를 끝났다고 알릴 때 씁니다.
  - 타입을 정한 작업은 실행할 때마다 회차 ID(`<작업 ID>#<번호>`)를 받습니다. `--attempt-id` 로 어느 회차의 결과인지 밝히면, 그 사이 작업이 다시 실행됐을 때 옛 보고가 새 실행을 끝내지 않고 거부됩니다.
  - 같은 보고를 다시 보내면 처음과 같은 답이 돌아옵니다. 이미 끝난 회차에 다른 결과를 보내면 거부됩니다.
- `task-cancel` 로 실행 중인 명령 작업을 취소하면 그 명령과 명령이 띄운 프로세스를 끝냅니다. 명령은 끝났지만 명령이 띄운 프로세스가 아직 출력을 쥐고 있어 작업이 실행 중으로 보일 때도 그 프로세스들을 끝냅니다. 출력을 닫고 남은 데몬은 작업이 끝나면 작업의 일부로 보지 않습니다. 끝난 것을 확인한 뒤에 세마포어·lease 를 내놓으므로, 같은 자원을 기다리던 작업은 앞 명령이 정말 끝난 다음에 시작합니다. 러너가 꺼져 있어도 취소하면 끝내기 시작합니다. 이때 취소 명령은 끝나기를 기다리지 않고 바로 돌아오고, 세마포어·lease 는 끝난 것을 확인한 뒤에 내놓습니다. 그 전에 `task-retry` 로 다시 실행하면, 앞 회차의 프로세스가 모두 끝난 것을 러너가 확인할 때까지 작업은 준비 상태로 기다립니다. 이때 `task-list` 는 줄 끝에 `(waiting_previous_attempt)` 를, `task-get` 은 `phase: waiting_previous_attempt` 줄을 보입니다. 이전 취소의 정리가 늦게 도착해도 다시 실행 중인 새 회차와 그 자원 점유는 건드리지 않습니다. Windows 에서는 Tasty 를 다시 시작하기 전에 띄운 명령의 경우 그 명령 하나만 끝내므로, 그 명령이 띄운 프로세스는 남을 수 있습니다.
- `task-delete` 는 다른 작업이 참조하고 있으면 거부하고 참조하는 쪽 ID 를 알려줍니다. 실행 중인 작업은 먼저 취소해야 합니다. 취소한 작업의 프로세스가 끝난 것을 아직 확인하지 못했을 때도 거부합니다. 끝난 것을 확인해 세마포어·lease 를 내놓은 뒤에 지울 수 있습니다. `task-purge` 는 이런 작업을 건너뛰고 응답의 `skipped`·`skipped_count` 로 알려줍니다.

## 끝난 것을 사건으로 받기

작업이 끝났는지 되풀이해 물어보는 대신, 끝나는 대로 받아볼 수 있습니다.

```sh
tasty events follow --filter 'agent.*'
```

한 줄에 사건 하나가 JSON 으로 나오므로 셸에서 그대로 받아 씁니다.

```sh
tasty events follow --filter 'agent.*' | while read -r line; do
  echo "사건: $line"
done
```

- **읽던 위치는 수신자가 저장합니다.** 답에 함께 오는 `next_offset` 을 다음 번 `--offset` 으로 주면 끊겼던 자리부터 이어집니다. 한 번만 읽고 말 때는 `tasty events fetch --offset <번호>` 를 씁니다.
- 사건은 메모리에만 남고 최근 것만 보관합니다. 너무 오래 끊겨 있어 그 사이가 밀려났으면 **조용히 처음부터 주지 않고** 몇 건을 못 줬는지 알려 줍니다. 그 알림은 사건 목록과 섞이지 않게 따로 나오므로 위의 `while read` 가 방해받지 않습니다.
- Tasty 를 다시 켜면 사건은 사라지고 위치도 처음부터 다시 매겨집니다. 답에 함께 오는 `epoch` 이 지난번과 다르면 저장한 위치는 재시작 전 실행의 값입니다.
- 연결이 끊기면 `follow` 는 다시 붙을 때 줄 `--offset` 과 `--epoch` 을 알려 주고 끝납니다. 그대로 붙여 다시 실행하면, 그 사이 Tasty 가 다시 켜졌을 때 그렇다고 알리고 새로 시작된 사건의 처음부터 받습니다. `--reconnect` 를 주면 끝나지 않고 1초마다 다시 붙어 이어 갑니다.
- 지금 나오는 것은 **작업이 끝났을 때**와 **배리어가 닫혔을 때** 둘입니다. 작업 사건에는 상태와 함께 실행 회차(`attempt_id`), 기록 버전(`revision`), 건너뛴 이유(`skip`, 앞 작업 때문이면 그 작업과 상태)가 실립니다. 왜 실패했는지는 사건에 실리지 않으니 그때는 `tasty agent task-get` 으로 봅니다.
- 읽기가 느린 수신자를 위해 별도 대기열을 계속 늘리지는 않습니다. 최근 이벤트의 공용 보관 범위를 벗어나면 누락을 확인해야 합니다.

## 동시 실행 제한과 신호

작업을 여럿 돌릴 때 쓰는 조율 장치가 함께 있습니다.

| 장치 | 쓰임 |
|---|---|
| 세마포어 | 같은 이름을 단 작업이 동시에 몇 개까지 돌지 정합니다. 작업을 만들 때 `--concurrency-limit <이름>` 이 짧은 표기입니다. 작업에 단 세마포어의 만료 시간은 리스처럼 러너가 작업을 지켜보는 동안 계속 늦춰 줍니다 |
| 배리어 | 정해진 수의 신호가 모일 때까지 막습니다. `wait_barrier` 작업으로 그래프에 끼워 넣습니다 |
| 리스 | 파일 같은 자원을 한 번에 하나만 잡게 합니다. 만료 시간이 있고, 충돌하면 실패하거나 못 잡았다는 응답을 바로 돌려줍니다 — 기다리지 않습니다. 작업에 단 리스(`metadata.lease`)의 만료 시간은 러너가 작업을 지켜보는 동안 계속 늦춰 주므로, 오래 걸리는 작업도 도중에 리스를 빼앗기지 않습니다. 러너가 꺼져 있는 동안에는 늦추지 않습니다. 작업에 다는 만료 시간은 1초 이상이어야 합니다. 그래도 늦추기 전에 만료돼 다른 쪽이 리스를 가져가면 `task-get` 에 `warning:` 줄이 나옵니다. 이 줄은 지금 실행 회차의 일만 보이므로, 다시 실행하면 앞 회차의 경고는 나오지 않습니다 |
| 리듀서 | 여러 작업의 결과를 하나로 합칩니다. 첫 성공만, 전부, JSON 병합, 텍스트 이어붙이기 중에 고르거나, `custom:<셸 명령>` 으로 직접 합칩니다. 이 명령은 입력 결과들을 JSON 배열로 표준 입력에서 받고, 표준 출력이 JSON 이면 그 값을, 아니면 글자 그대로를 결과로 씁니다 |
| 요청량 제한 | 에이전트별 · 지표별로 정해진 시간에 몇 번까지 허용할지 정합니다 |

```sh
tasty agent semaphore-create --workspace 2 --name build --permits 2
tasty agent barrier-create --workspace 2 --name ready --count-required 3
tasty agent lease-acquire --workspace 2 --resource file:/tmp/db --holder agent-a --ttl-ms 60000
tasty agent task-reduce --workspace 2 --inputs "$A,$B" --strategy all --extract-path /stdout/text
```

세마포어 · 배리어 이름에는 영문 소문자 · 숫자 · `.` · `_` · `-` 만 씁니다. 다른 문자가 섞이면 명령이 어느 글자가 문제인지 알려 주며 거절합니다. 리스의 자원 이름에는 이 제한이 없습니다.

전체 목록은 `tasty agent --help` 에 있습니다.

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [tasty CLI 사용하기](cli.md) — 에이전트가 쓰는 명령 전반
- [Claude · Codex](claude-codex.md) — 자식 에이전트를 띄우고 통지받기
- [훅 · 알림 · 웹훅](hooks-notifications.md) — 명령이 끝났을 때 알림 받기
