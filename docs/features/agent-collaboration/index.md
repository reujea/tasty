# 다중 에이전트 협업 (Agent collaboration)

- **Status**: Implemented
- **주체**: AI Agent (여럿이 한 인스턴스 공유)
- **설계**: 에이전트 작업 조율과 DAG 화면
- **코드**: `agent.*` 핸들러(`src/adapters/ipc/handler/agent.rs`), 영속 `tasty-memory`
- **화면**: 둘 다 호스트의 작업 조회 데이터를 사용한다 — [DAG 그래프 surface](../../surfaces/dag-graph/index.md)(`tasty new tab --type dag_graph`)는 탭 하나를 점유하는 상주 관찰용, [DAG 목록 popup](screens/dag-list-popup.md)(도구 메뉴 · `KeybindingSettings.toggle_dag_list`)은 목록에서 하나를 골라 잠깐 확인하고 닫는 용도의 workspace 스코프 창이다. IPC/CLI 관측 수단(`agent.task_list`/`task_graph`/`task_get`/`dag_list`/`dag_get`)은 그대로 유효하다.
- **메서드 목록**: [reference/api](../../reference/api.md#에이전트-협업-agent)

부모 Claude의 자식 상태 수신은 [완료 알림 mod](../../plugins/claude/notify-mod.md)가 담당하며, mod가 없는 세션에서는 Monitor로 구독한다. 상태 알림은 task 결과 검증과 별개다.

## 목적

여러 AI 에이전트가 같은 인스턴스에서 작업을 조율하는 6가지 기능이다. 모두 `agent`(AgentManage) 권한으로 사용한다 — `agent.*` 단일 네임스페이스 + `<verb>_<modifier>` 패턴. 영속은 `tasty.agent.*` memory 키(task/barrier/semaphore/lease 는 `workspace:<id>` scope, rate-limit 은 `global`).

## 내부 동작

### `*_await` — 메서드마다 다르다

`barrier_await` 는 blocking 이 아니라 **현재 상태 즉시 응답**(poll, `barrier_state` 의 alias) — 호출자가 terminal 상태가 아니면 반복 호출한다. CLI 의 추천 표기는 `tasty agent barrier-state` 이고 `barrier-await` 는 호환용 예전 이름이다.

`task_await` 는 응답을 기다린다(`TaskWakerHub` 기반, 워커 스레드에서 처리). `timeout_ms` 생략 시 기본 10분(600,000ms, 잠정값 — 실사용 경험이 쌓이면 재조정)까지 대기하고, 그 안에 terminal 에 도달하지 못하면 `{"outcome":"timed_out"}` 으로 반환한다. `timeout_ms: 0` 을 명시하면 이 기본값을 우회해 무한 대기한다. 응답의 `outcome` 은 `terminal`(`state` 에 succeeded·failed 등 종결 상태), `timed_out`, `not_found` 중 하나이며 셋 다 성공 응답이라 CLI 종료 코드는 0 이다. 종료 코드 0 은 작업 성공이 아니므로 `outcome` 다음 `state` 를 읽는다. 루트 `--response-timeout-ms` 는 CLI 가 서버 응답을 기다리는 상한일 뿐 작업 대기 시간(`timeout_ms`)이 아니다. **local caller 전용**(`local_only`) — `approval.await` 와 대칭으로, plugin SDK 는 단일 워커 스레드가 요청을 직렬 처리하므로 plugin 이 `task_await` 로 블록되면 자기 자신의 다른 host→plugin 요청을 전혀 처리하지 못한다(자기 자신이 호출한 응답 수신은 별도 경로라 자기-교착까지는 아니지만, 그 task 가 자신을 다시 호출하는 구성이면 dispatch 타임아웃(5s)으로 정상 작업이 실패한다). plugin 은 대신 완료 판정 전략(`[[contributes.completion_strategy]]`)을 선언해 러너가 대신 기다리게 하거나, `task_get` 을 폴링한다.

대기는 요청한 `workspace_id` 를 가진 engine 의 `TaskWakerHub` 에서 한다. 허브는 engine 마다 따로 있고 완료 통지는 task 가 속한 engine 의 허브로만 가므로, 창이 여러 개이거나 parked engine 이 있어도 포커스나 창 순서로 허브를 고르지 않는다. 어느 engine 도 그 workspace 를 갖지 않으면 다른 요청의 라우팅과 같은 대상 없음 오류(`-32602`)로 답한다. workspace 는 있지만 task 가 없으면 `{"outcome":"not_found"}` 다. 허브는 대기자를 먼저 등록한 뒤 저장소 상태를 읽는다. 그래서 상태를 읽은 직후에 끝난 task 의 완료도 놓치지 않는다.

<a id="6-primitive"></a>

### 6가지 협업 기능

#### Task DAG

Task DAG는 작업의 의존 관계와 상태를 관리한다. 생성 시 의존성 사이클을 거절하며 상태가 바뀌면 후속 작업을 다시 평가한다.

- 상태 8종: `waiting`, `ready`, `running`, `succeeded`, `failed`, `cancelled`, `skipped`, `unknown`
- command 4종: `run`, `custom`(선택적 `poll`), `reduce`, `wait_barrier`
- `run`은 Surface 없는 자식 프로세스다. stdout·stderr 각각 마지막 64KiB를 보관한다. `TaskResult.output`의 `{"pid","stdout":{"text","truncated","dropped_bytes"},"stderr":{...}}`와 종료 코드로 반환한다. 실패하면 종료 코드와 줄기마다 마지막 2KiB 꼬리를 담은 오류 메시지도 함께 반환한다. TTY는 지원하지 않으며 필요하면 `pty.*`를 사용한다.
- `custom`은 폴링이나 외부 훅으로 완료를 판단한다. 예를 들어 `host/command-completed`는 OSC 133 셸 통합의 완료 훅과 종료 코드를 사용한다.

실패 정책은 종류에 따라 설정할 작업이 다르다.

| OnFailure | 설정 위치와 동작 |
|---|---|
| `abort`(기본) | 후속 작업에 설정. 의존 작업이 실패하면 후속 작업을 skipped로 처리 |
| `continue_downstream` | 후속 작업에 설정. 의존 작업이 실패해도 진행 가능 여부를 평가 |
| `fallback{task}` 또는 `fallback{inline}` | 실패할 수 있는 선행 작업에 설정. 그 작업이 Failed가 되면 대체 작업 실행 |

fallback을 후속 작업에 설정해도 의존성 실패로 인한 Skipped에는 적용되지 않는다. 이 경우 해당 작업이 경고 없이 waiting에 남을 수 있다.

기존 작업을 지정한 `fallback{task}`는 원래 작업이 Failed가 되기 전까지 실행하지 않는다. 자신의 `depends_on`이 비어도 바로 ready가 되지 않는다. 원래 작업이 Succeeded·Cancelled·Skipped로 끝나면 대체 작업도 skipped로 마감한다. `fallback{inline}`은 실패 시 작업 자체를 생성하므로 이 대기 상태를 거치지 않는다.

기존 fallback 작업은 원래 작업보다 먼저 만들어야 한다. 두 생성 호출 사이에 러너가 대체 작업을 먼저 실행하지 않도록 `reserved_for_fallback: true`(CLI `--reserved-for-fallback`)로 예약한다. 그러면 의존 작업이 끝나도 waiting을 유지하고, 이를 참조하는 원래 작업의 생성 호출이 예약을 해제한다. 참조 작업을 끝내 만들지 않으면 계속 waiting에 남으므로 `task_delete`로 정리하고 다시 만든다. 예약 없이 생성하면 이 경합을 막지 못한다.

자세한 완료 전략과 출력 캡처는 [작업 러너](../../dev-guide/agent-runner.md#완료-판정-전략-레지스트리-srccompletion_strategy)와 [run 출력](../../dev-guide/agent-runner.md#run-출력-캡처)을 따른다.

Custom 완료 조회의 `poll.interval_ms`는 task별로 이전 조회 완료 뒤의 최소 간격이다. 최초·복원 조회는 즉시 하고, 다음 조회 전에 기한이 오면 호출 없이 timeout으로 끝낸다. 느린 조회 뒤 밀린 호출을 몰아서 보내지 않는다. [조회 간격과 기한](../../dev-guide/agent-runner.md#조회-간격과-기한)에 저장·재개와 응답 우선순위가 있다.

#### 참조 검증과 데이터 전달

`task_create`는 `depends_on`, `OnFailure::Fallback{task}`, `TaskCommand::Reduce.inputs`가 가리키는 ID의 존재를 검사하고 없으면 `-32602`로 거절한다(`crates/tasty-agent/src/task/store.rs`). 실패할 때 생성하는 `Fallback{inline}`은 제외한다. 이미 저장된 끊어진 참조를 자동 복구하지는 않는다. 해당 작업은 waiting에 남을 수 있고, 존재하지 않는 fallback은 실패 전이 때 경고도 남긴다.

선행 작업 결과를 입력으로 사용할 때는 `${task.<id>.output<pointer>}`를 쓴다. 실행 직전에 `result.output`에서 RFC 6901 JSON Pointer로 값을 읽어 `Custom.params`의 JSON 트리, `Run.command` 인자, `Run.cwd`에 넣는다. 포인터를 생략한 `${task.t-a.output}`은 출력 전체를 뜻한다.

문자열이 placeholder 하나뿐이면 JSON 타입을 유지해 교체한다. 숫자 결과는 숫자로 남아 `as_u64` 검사도 통과한다. 다른 텍스트와 섞였을 때만 문자열로 보간한다. 대상은 `depends_on` 또는 `Reduce.inputs`에 선언돼 있어야 하며 미선언·잘못된 문법은 생성 시 `-32602`로 거절한다. 작업·결과·포인터를 찾지 못하면 null로 대신하지 않고 실패를 알린다([결과를 파라미터로 전달](../../dev-guide/agent-runner.md#선행-task-출력을-파라미터로-넘기기--taskidoutputpointer)).

`Reduce.inputs`도 그래프 엣지와 사이클 검사에 포함한다. 다만 reducer는 실패 결과도 모으므로 입력 하나가 실패했다고 skipped로 처리하지 않는다. 입력이 모두 종결될 때까지 waiting을 유지한 뒤 ready가 된다.

#### 타입 계약 (v2)

task 는 선택적으로 `contract_version: 2` 계약을 가진다. 계약은 입력·출력 타입(boolean·int64·float64·string·enum·object·list·unit·json·이름 붙은 타입)을 선언하고, 저장소가 보고된 결과를 그 타입으로 확정한다. 출력이 타입에 맞지 않거나 없으면 성공이 아니라 실패로 끝난다. 결과는 최종 출력(`has_output`·`output`)과 원시 응답·실패 단계·출처를 나눠 담는다. 기본 출력은 `run` 이 종료 코드(int64), `custom` 이 json, `wait_barrier` 가 unit, `reduce` 가 전략별 타입이다.

v2 의 int64 값은 내부에서는 정수로 들고, 직렬화할 때(저장·IPC·CLI 등 경로와 무관) JavaScript 소비자를 지나도 바뀌지 않도록 10진 문자열로 쓴다(입력은 정수도 받는다). `json` 타입 값은 JSON 숫자 그대로이므로 2^53 을 넘는 정수는 JavaScript 도구에서 정밀도를 잃을 수 있다. 정밀도가 필요하면 `int64` 로 선언한다. v1 출력 placeholder 나 v1 reduce(단발 `agent.task_reduce` 포함)로 v2 task 의 결과를 읽는 요청은 거절한다. 계약이 없는 task(v1)의 결과와 reducer 동작은 그대로다. v2 task 는 별도 저장 namespace 에 envelope 로 저장해 구버전이 실행하지 않는다. v2 task 는 그래프 단위로 제출한다(`agent.task_graph_submit`, CLI `tasty agent task-graph-submit`, 검증만 하는 `agent.task_graph_validate`·`--dry-run`). 그래프 전체를 검증한 뒤 한꺼번에 활성화하므로, 검증에 실패하면 아무것도 저장되지 않고 오류에 task 와 그래프 안의 위치(JSON Pointer)가 실린다. 활성화 전에는 어떤 task 도 실행되지 않는다. memory 가 대체 모드라 재시작 뒤 남지 않으면 `durability: "best_effort"` 를 밝힌 그래프만 받는다. 실행 회차마다 결과와 종결을 한 번에 기록하며 다른 회차의 보고는 적용하지 않는다. 입력은 `bindings` 로 고정 값이나 다른 task 출력의 한 위치(`from_task`), 실행된 원본 하나(`one_of`)에서 받으며, 생성할 때 타입을 검사하고 실행 직전에 해석해 `input_snapshot` 으로 고정한다. 값은 `input_mapping` 이 정한 run argv 요소·stdin JSON·custom params·wait_barrier 의 barrier 이름 자리에만 들어가고 다시 해석하지 않는다. wait_barrier 는 이름을 command `name` 이나 입력(`input_mapping.barrier`) 중 하나로 받는다. run 은 이미 있는 파일의 경로를 argv 로 받을 수 있다(`args` 원소 `{"file": <포인터>}`). 실행 직전에 읽을 수 있는 일반 파일인지만 확인하고, 파일을 만들거나 지우지 않는다. 완료를 따로 기다리는 custom 의 접수 응답(dispatch 응답)은 최종 응답과 구별해 결과의 `raw.accepted` 에 남는다(64 KiB 를 넘으면 앞부분과 잘림 표시). 형식과 규칙은 [작업 러너 §v2 타입 계약](../../dev-guide/agent-runner.md#v2-타입-계약-contract_version-2).

그래프 안의 v2 task 는 `transitions` 로 성공한 출력에 따라 후속 task 를 고른다. 조건은 출력의 한 위치와 상수를 비교하거나(`compare`), 값 목록에 드는지 보거나(`in`), 이를 `all`·`any`·`not` 으로 묶는다. 제출할 때 출력 타입으로 검사한다. 기본은 참인 case 가 하나여야 하는 `exclusive` 이고 `all_matches` 는 참인 case 를 모두 고른다. 맞는 case 가 없을 때의 처리(`otherwise` 또는 `no_match: "finish"`)를 반드시 적는다. 고른 경로는 결과와 함께 task 의 `route` 에 저장한다. 고르지 않은 대상과 그 아래로만 이어진 task 는 실패가 아니라 `skip.reason: branch_not_selected` 로 끝나고, 합류 task 는 선택된 경로만 기다린다. 선택되지 않을 수 있는 task 의 출력을 필수 입력으로 읽는 정의는 거절하며 `one_of`·optional 로 적게 한다. DAG 요약은 이를 `not_selected` 로 세고 성공으로 집계한다. 형식과 판정 규칙은 [작업 러너 §전이 조건과 경로 선택](../../dev-guide/agent-runner.md#전이-조건과-경로-선택).

#### 작업·DAG 조회

#### agent task

v2 `agent` task 는 provider 세션(새로 띄우거나 이미 열린 세션. 번들은 Claude·Codex)에 지시 하나를 보내고 그 턴의 끝을 결과로 받는다. provider 는 고정 목록이 아니라 그 이름의 IPC namespace 를 소유한 설치 플러그인이 활성이고 `agent.turn_report` 를 허가받았는지로 판정하며, 자격이 없으면 생성과 실행 직전에 원인(소유자 없음·비활성·권한 없음)을 구분해 거절한다. 기본 출력은 최종 답변 string 이고, 다른 출력 타입은 에이전트가 같은 회차에 `agent.task_submit_result`(CLI `tasty agent task-submit`)로 낸 값이다. 제출은 도착할 때 타입을 검사하고 턴이 끝날 때 확정하며, 같은 회차의 다른 값은 거절한다. 턴 경계는 provider 플러그인이 훅에서 `agent.task_turn_report` 로 알린다. 이미 열린 세션은 idle 이고 다른 task 가 쥐지 않았을 때만 지시를 받고, 같은 세션의 task 는 차례로 실행된다. idle 만으로는 성공하지 않으며 결과 없음·턴 오류·세션 종료·시간 초과·provider 사용 불가를 `typed_result.error.code` 로 구별한다. 이미 열린 세션에 보낸 지시(`<provider>.tell`)가 5초 안에 답하지 않으면 실행되지 않았음이 확실할 때만 다시 보내고, 시작된 지시는 다시 보내지 않고 늦은 응답을 기다린다. 전달 여부를 알 수 없게 되면 `failed` 가 아니라 `unknown`(`dispatch outcome unknown: …`)이다. 새 세션의 spawn 응답(최대 30초)은 러너 스레드 밖에서 기다리므로 그동안 다른 task 가 진행된다. 입력 대기는 Running 의 `awaiting_input` phase 다. task 는 세션을 닫지 않는다. 형식과 규칙은 [작업 러너 §agent task](../../dev-guide/agent-runner.md#agent-task).

#### 후처리 CLI

v2 run·custom task 는 계약의 `postprocess` 로 본 작업 뒤 CLI 하나를 실행해 그 stdout 을 최종 출력으로 받을 수 있다. 명령은 셸 없이 직접 실행하고, 입력 snapshot·본 작업 원본에서 고른 값을 JSON 문서 하나로 stdin 에 쓴다. stdout 은 JSON 값 하나(선택 pointer) 또는 UTF-8 text 이고 출력 타입으로 검증한다. 후처리가 끝날 때까지 task 는 Running 이고 permit 을 쥐며 하류는 기다린다. 실패 원인(종료 코드·시작 실패·시간 초과·취소·형식 오류 등)을 구분해 기록하고, 명시한 횟수만큼 저장한 본 작업 결과로 후처리만 다시 실행한다. 시작했지만 결과를 받지 못한 실행은 재시작 뒤 다시 실행하지 않는다. `task_get` 은 이 단계의 task 에 `phase`(`postprocessing`·`retry_wait`)를 싣고, CLI `task-get` 은 단계와 실행 번호, 끝난 뒤에는 마지막 실행의 원인·종료 코드와 재시도로 넘어간 실행의 원인을 줄로 보인다. 후처리 자식은 활성 플러그인이 선언한 바깥 제품 세션 변수와 바깥 Tasty 인스턴스의 신원 변수를 받지 않는다. 앱을 정상 종료하면 진행 중인 후처리 그룹을 끝내고 보고를 저장할 때까지 짧게 기다린다. 형식과 규칙은 [작업 러너 §후처리 CLI](../../dev-guide/agent-runner.md#후처리-cli-postprocess).

#### report

v2 DAG 는 사람이 읽는 실행 기록인 report 를 가진다(`agent.dag_report`, CLI `tasty agent dag-report <dag> [--task <id> [--attempt <n>]] [--include-raw]`). task 마다 레코드가 남아 있는 동안 조회 때 만들고 retry 로 닫힌 회차는 그때 한 번 굳혀 두는 자동 항목(종결 상태·시작·끝 시각·실제 입력·출력·실패 단계와 사유·skip 사유, 종류별로 run 종료 코드·agent provider 와 세션·custom 의 IPC 메서드·reduce 전략과 입력 수·barrier 이름)과 회차별 custom 기록을 싣는다. 표준 출력·오류는 `--include-raw` 일 때만 싣는다. custom 기록은 실행 중인 회차만 쓸 수 있다. run·후처리·custom reduce 셸은 환경의 `TASTY_TASK_REPORT` 로 `tasty agent report append '<text>'` 를 부르고, run 은 stderr 에 `::tasty-report::<text>` 줄을 써도 된다. agent 는 지시문 끝의 사용법 명령(`tasty agent report usage`)과 주소를 받는다. 기록 한 건과 회차 블록에는 크기 상한(설정 › 기타 › 작업 파이프라인)이 있고, 넘으면 자르거나 세기만 하며 호출은 실패하지 않는다. 회차가 끝나면 블록이 닫혀 이후 기록은 거절된다. retry 는 새 회차 블록을 따로 만들고, task 를 지우면 블록도 지운다. report 는 binding 입력으로 쓸 수 없다. 형식과 규칙은 [작업 러너 §DAG report](../../dev-guide/agent-runner.md#dag-report).

`task_get`의 CLI 출력에는 command 종류, `depends_on`, `on_failure`, `metadata`가 포함된다. v2 task 는 회차(`attempt:`), 레코드 revision(`revision:`), 입력 출처(`input: <필드> <- <원본 task><포인터> (attempt <원본 회차>)`), 최종 출력(`output: <값> (from <출력 자리>)`)을 더한다. 출력이 없으면 `output: none` 과 실패 단계·코드를 적어(`error: execution/result_missing: ...`) 결과를 내지 않고 끝난 task 가 성공처럼 보이지 않는다. `task_graph`의 노드는 `command_kind`·`on_failure_kind`·`skip` 을, 엣지는 `depends_on`·`fallback`·`reduce`·`binding`(v2 입력 binding 의 원본 → 받는 task)·`transition`(전이, `selection` 포함) 종류를 제공한다. 같은 원본에서 `depends_on` 과 `binding` 이 함께 오면 JSON 엣지는 둘 다 내고, dot 과 DAG 화면은 `binding` 한 줄만 그린다. dot 에서는 depends_on 실선, fallback 주황 파선, reduce 파랑 점선, binding 청록 파선, transition 남보라 파선이고 전이는 선택 상태를 라벨로 붙여 선택된 간선은 굵게, 선택되지 않았거나 쓸 수 없는 간선은 반투명으로 그린다. dot 노드는 상태를 라벨로 적고, 갈래에서 선택되지 않아 건너뛴 노드(`skip.reason: branch_not_selected`)는 `not selected` 라벨과 파선 테두리로 앞 작업 때문에 건너뛴 노드(`skipped`)와 구분한다. JSON·dot·DAG 화면은 같은 수집기에서 간선을 얻고, dot 과 DAG 화면만 같은 쌍의 depends_on 을 binding 한 줄로 줄여 그린다. DAG 화면의 표시는 [DAG 그래프 surface](../../surfaces/dag-graph/index.md)에 있다.

**fallback 참조는 사이클 검사 대상이 아니다.** `detect_cycles()`·`TaskGraph::dfs_cycle`은 `depends_on`과 `Reduce.inputs`만 순회한다. A와 F가 서로를 fallback으로 참조해도 존재 검사만 통과하면 저장된다. 그래프에는 보이지만 `-32602`로 차단되지 않는다.

DAG는 별도 영속 레코드가 아니라 workspace의 task에서 도출한다.

1. `task.metadata.dag`가 문자열이면 같은 값끼리 explicit 그룹으로 묶는다. 연결 여부와 무관하다.
2. 나머지는 `depends_on`·`Fallback.task`·`Reduce.inputs`·`metadata.fallback_of`·v2 `bindings`(입력을 주는 task)·v2 `transitions`(전이 대상) 참조를 무방향으로 본 약연결 컴포넌트로 묶는다(derived).

`metadata.dag_name`을 표시 이름으로 쓰고 없으면 explicit 키, root task 이름 순서로 선택한다. metadata와 관계로 도출하므로 새 DAG 필드를 추가하는 마이그레이션 없이 기존 작업도 포함한다.

ID는 explicit의 `d:<metadata.dag 값>` 또는 derived의 `c:<root task id>`다. root는 그룹에서 `(created_at, id)`가 가장 작은 작업이다. 같은 작업 집합은 같은 ID를 가지며 전체 식별에는 `(workspace_id, id)`를 쓴다.

`dag_list`의 각 항목에는 `id`, `workspace_id`, `name`, `source`(`explicit` 또는 `derived`), `task_count`, `state_counts`(상태 8종과, 그중 경로가 선택되지 않은 수 `not_selected`·fallback 이 대신 성공한 실패 수 `recovered`·저절로는 더 실행되지 않는 대기 수 `blocked`·성공한 끝 작업 수 `succeeded_ends`·실행 중 agent 세션이 사람의 입력을 기다리는 수 `awaiting_input`), `rollup_state`, `created_at`, `updated_at`, `root_task_ids`, `has_cycle`이 있다. `root_task_ids`는 흐름의 시작점이다. 그룹 안의 선행을 기다리지 않고, 그룹 안 다른 작업의 fallback(`Fallback.task`·`metadata.fallback_of`)이나 전이 대상도 아닌 작업이다. `include_tasks:true`일 때만 `task_ids`를 포함한다. rollup 판단 순서는 running → ready → waiting → 실패(partially_failed/failed) → 전부 종결(succeeded/skipped)이다. 저절로 진행할 수 있는 작업이 남아 있으면 실패가 섞여 있어도 running·ready·waiting 중 하나다. 이때 waiting 은 막히지 않은 대기(`waiting` 에서 `blocked` 를 뺀 수)가 있을 때다. `blocked` 는 저절로는 더 실행되지 않는 대기다. 선행 결과를 쓸 수 없는데 실패 정책이 건너뛰지 않고 대기로 남긴 작업(자기 실패에 쓸 fallback 을 둔 v1 작업이 선행 실패 뒤 남은 경우)에서 시작해, 그런 작업이나 unknown 작업을 기다리는 대기를 전이적으로 모두 센다. 막힌 작업의 하류, 막힌 입력을 기다리는 v1 Reduce, main 이 끝나지 않아 깨어날 수 없는 fallback 작업(`Fallback.task` 대상이나 `metadata.fallback_of` 로 main 을 가리키는 작업)이 여기에 든다. 판정은 그룹 밖 선행도 보도록 workspace 전체로 한다. unknown 은 사람이 retry·cancel 해야 진행되므로 진행할 수 있는 작업으로 보지 않는다. 더 진행할 수 없을 때 fallback 이 대신하지 못한 실패가 있으면, 끝까지 성공한 갈래가 하나라도 있으면 partially_failed, 없으면 failed 다. 갈래의 성공은 끝 작업으로 센다(`state_counts.succeeded_ends`). 끝 작업은 그룹 안 다른 작업이 `depends_on`·binding·`Reduce.inputs` 로 기다리지 않고 전이로 이어지는 작업도 없는 작업이다. fallback 간선과 경로가 선택되지 않아 건너뛴 작업은 하류로 보지 않는다. 그래서 경로를 고르지 않고 끝난 성공 작업이나 하류가 모두 선택되지 않은 성공 작업은 성공한 끝이다. 또 fallback 작업 자신(`Fallback.task` 대상, `metadata.fallback_of` 가 있는 작업)은 main 을 대신하므로 끝 작업이 아니다. 끝 작업이 성공했거나, 실패했지만 fallback 이 대신 성공했으면(`recovered`) 성공한 끝으로 센다. 복구된 실패를 실패로 세지 않는 것과 맞춘 것이다. 그래서 앞단만 성공하고 마지막이 실패·건너뜀인 단일 사슬은 failed 다. 실패 없이 막힌 대기나 unknown 이 남으면 waiting 이다. 끝난 것으로 보이면 사람이 개입해야 하는 작업이 가려지기 때문이다. 같은 그룹의 fallback(`Fallback.task` 또는 `metadata.fallback_of` 로 main 을 가리키는 inline fallback)이 성공한 실패는 `recovered` 로 세고 실패로 보지 않는다. fallback 도 실패했으면 그 fallback 의 fallback 을 따라간다. 그룹 밖의 fallback 은 보지 않는다. 전부 종결이면 cancelled 가 없고 skipped 가 모두 선택되지 않은 것일 때 succeeded, 아니면 skipped 다.

workspace_id를 생략하면 살아 있는 모든 workspace를 조회하고 `scope: "live_workspaces"`로 범위를 알린다. 삭제된 workspace의 고아 작업은 제외하며 부팅 GC가 정리한다. `dag_get`은 선택한 DAG의 부분집합으로 `task_graph`와 같은 nodes·edges 또는 dot을 만든다. CLI `task-graph`·`dag-get`의 기본 출력은 이 응답의 `nodes`·`edges`로 CLI가 만든 [부모 ID 표](../../dev-guide/cli-structure.md#부모-id-표-formatparent_tablers)이고, IPC 응답과 `--format json|dot`은 그대로다.

#### 작업 삭제

`task_delete`는 다른 작업이 `depends_on`·`Fallback.task`·`Reduce.inputs`로 참조하면 `-32010`과 `error.data.referenced_by` 목록을 반환한다. `cascade:true`는 이를 참조하는 작업들을 재귀적으로 함께 지우고, `force:true`는 참조 검사만 생략한다. force 사용 뒤 끊어진 참조는 호출자가 처리해야 한다.

running 작업은 옵션과 관계없이 삭제하지 않는다(`-32011`). 먼저 취소해야 한다. waiting·ready·종결 상태는 참조 조건을 충족하면 삭제할 수 있다. 종결 작업만 허용하면 방치된 waiting reducer와 그 입력을 정리할 수 없기 때문이다.

`task_purge`는 `states`와 `older_than_ms`로 후보를 고른다. 후보 밖의 작업이 참조하는 대상은 `retained`로 남기며 `dry_run:true`는 삭제 계획만 반환한다. 실제 삭제 시 `tasty.agent.handle.<id>`·`tasty.agent.run_result.<id>`도 정리한다.

#### Barrier

N회 signal을 받으면 닫힌다. `timeout_ms`가 지났는지는 signal·state·list 호출 때 확인해 `timed_out`으로 바꾸며 별도 스레드를 만들지 않는다.

#### Semaphore

N개 permit으로 동시 점유를 제한한다. 같은 holder의 재획득은 중복 점유를 만들지 않고 `acquired_at`·`expires_at`을 갱신한다. permit은 holder가 반납하거나 명시한 `--ttl-ms`가 만료될 때 회수한다. 기본은 만료 없음이다. 작업의 `metadata.semaphore.ttl_ms` 는 러너가 permit 을 쥔 동안 TTL 의 절반마다 갱신한다(lease 와 같다). 정상 작업 도중 permit을 회수하면 다른 작업이 같은 자원에 들어갈 수 있기 때문이다.

`semaphore-set-permits`로 한도를 줄여도 기존 holder는 강제로 회수하지 않는다. 반납될 때까지 새 획득을 거절한다(이 문서).

`task.metadata.semaphore = { name }`인 작업은 permit 수만큼 Running이 되고 나머지는 Ready로 기다린다. `task-create --concurrency-limit <name>`으로 지정할 수 있다([동시성 제한](../../dev-guide/agent-runner.md#동시성-제한-concurrency-limit)).

#### Lease

TTL이 있는 협조적 자원 점유 표시이며 OS 잠금은 아니다. 충돌 시 mode `fail`은 `-32009`, `block`은 `acquired:false`를 반환한다. 만료된 항목은 list·acquire 때 정리한다. 작업의 `metadata.lease.ttl_ms` 는 러너가 그 lease 를 쥔 동안 TTL 의 절반마다 갱신해, 실행 중인 작업의 lease 가 TTL 뒤 다른 holder 에게 넘어가지 않는다. 재시작 뒤 이어서 지켜보는 작업도 같다. TTL 은 1초 이상이어야 하며 더 짧으면 작업 생성·제출을 거절한다. 갱신하기 전에 TTL 이 지나 다른 holder 가 가져가면 작업은 계속 실행되고 `task-get` 에 `warning:` 줄(IPC `holding_warnings`)로 남는다. 러너가 꺼진 동안에는 갱신하지 않는다([agent runner 가이드](../../dev-guide/agent-runner.md#dispatch-게이트-lease--semaphore)).

풀 모드는 `task.metadata.lease.candidates`에서 하나를 배정한다. 실제 자원은 `${lease.resource}`를 치환해 Run.cwd·Custom.params에 전달한다. 기본 fixed 모드는 후보 안에서만 선택한다. 명시적으로 elastic을 켜면 소진 시 `overflow_prefix+N` 후보를 원자적으로 만들며, `fail` 모드에서 풀을 사용할 수 없으면 `-32012`를 반환한다([자원 풀](../../dev-guide/agent-runner.md#자원-풀-배정-lease-pool--candidateselastic)).

#### Reducer

여러 작업의 결과를 `first_success`·`all`·`merge_json`·`concat_text`·`custom`의 5가지 방식으로 합친다. custom은 호스트 셸에 결과 배열 JSON을 stdin으로 보낸다. 단발 `agent.task_reduce` 또는 DAG의 `TaskCommand::Reduce`로 사용하며 후자의 inputs는 의존성에도 포함한다. v2 계약을 가진 `Reduce`는 전략별 타입 규칙을 따른다(`all`은 상태·출력 유무 레코드, `merge_json` 충돌은 기본 오류, `concat_text`는 문자열만, `custom` stdout은 JSON 값 하나).

#### Rate-limit

`(agent, metric)`별 token bucket이며 보충률은 `limit/per_ms`, 상한은 `burst`다. global scope에 저장한다. 누적 비용을 제한하는 [telemetry cap](../telemetry/index.md)과 달리 단위 시간당 사용량을 제한한다. CRUD·`try_consume`을 제공하며 IPC의 `should_rate_limit`이 호출마다 확인한다.

## 자식 에이전트를 DAG 노드로

Claude / Codex 자식을 **띄우고 그 완료를 기다리는 일**을 Task DAG 노드 하나로 표현할 수 있다. 전용 command kind 는 없다 — `custom` 노드로 plugin 의 spawn 메서드를 호출하면 된다. 코어는 어떤 에이전트인지 모른 채 IPC dispatch → 폴링만 한다.

```sh
tasty agent task-create --workspace 1 --name spawn-worker \
  --command '{"kind":"custom","ipc_method":"claude.spawn",
              "params":{"surface_id":560,"workspace":"wt-11",
                        "cwd":"/home/me/work/wt-11","prompt":"이 디렉토리의 테스트를 고쳐라"}}'
```

**`poll` 을 주지 않는 것이 요점이다.** 두 plugin 이 자기 매니페스트에 `[[contributes.completion_strategy]]` 를 선언하면서 `default_for_methods` 로 자기 `spawn`/`tell` 메서드를 지목해 두었으므로, 러너가 dispatch 시점에 그 전략을 자동으로 집어 폴링 모드로 전이한다. 전략이 없으면(= 해당 plugin 이 비활성이면) `custom` 노드는 dispatch 성공 즉시 `Succeeded` 가 되므로, 자식이 도는 동안 기다려주지 않는다.

Claude 자식은 입력 대기 화면으로 시작하면 `idle` 이므로, 프롬프트 없이 띄운 `claude.spawn` 노드는 세션이 준비되면 끝난다. `prompt` 를 주면 그 턴이 끝날 때 끝난다([Claude 플러그인](../../plugins/claude/index.md)의 `SessionStart` 행).

호스트가 바빠 5초 안에 답하지 못해도 노드를 바로 실패로 끝내지 않는다. `claude.state` 같은 상태 조회가 늦으면 다음 poll 에서 다시 묻는다(전략에 `timeout_ms` 가 있으면 그 기한까지). spawn 요청이 큐에서 기다리다 만료됐으면 실행되지 않았으므로 다음 tick 에 다시 보낸다. 시작된 spawn 이 늦으면 자식이 둘 생기지 않도록 다시 보내지 않고 늦은 응답을 받아 같은 전략으로 이어 간다. 답 없이 끝나거나 재시작으로 응답 통로가 사라지면 노드는 `failed` 가 아니라 `unknown`(사유 `dispatch outcome unknown: …`)이 되고, 자식이 떴는지 확인한 뒤 `retry`·`cancel` 로 정한다. 규칙은 [작업 러너 §호스트 응답 지연](../../dev-guide/agent-runner.md#호스트-응답-지연).

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

`tell` 에도 기본 전략(`tell-wait`)이 있어 **자식이 그 지시를 마칠 때까지** 노드가 `running` 에 머문다. Claude 자식의 서브에이전트 종료나 백그라운드 작업을 남긴 채 끝난 응답은 지시를 마친 것으로 보지 않는다. 백그라운드 작업이 끝난 뒤 이어지는 턴의 응답이 끝나야 노드가 끝난다. 끝나지 않는 백그라운드 명령(개발 서버 등)을 남기면 노드도 끝나지 않으므로 제한 시간을 둔다([Claude 통합](../../plugins/claude/index.md)). spawn 노드와 달리 `surface_id` 는 **자식** surface 다:

```sh
tasty agent task-create --workspace 1 --name tell-worker \
  --command '{"kind":"custom","ipc_method":"claude.tell",
              "params":{"surface_id":561,"message":"방금 고친 테스트를 다시 돌려라"}}'
```

호스트 `terminal.tell` 은 본문 주입에 성공한 직후 대상 자식의 `idle`(과 `needs_input`) 플래그를 내린다 — 그러지 않으면 첫 폴링 tick 이 직전 턴의 상태를 읽고 자식이 답을 시작하기도 전에 노드를 완료 처리한다. `terminal.broadcast` 도 같다.

### 실패 판정

자식이 종료되면(`exited` — 프로세스 종료 또는 surface 소멸) 그 노드는 **`failed`** 다. 두 plugin 모두 `exited` 를 `failure_states` 로 분류하므로, 자식 에이전트 노드에서도 `on_failure`(`abort` / `continue_downstream` / `fallback`)가 그대로 동작한다 — 기본값 `abort` 면 downstream 이 `skipped` 로 cascade 된다. 실패 메시지에는 상태값과 poll 응답 요약이 함께 실린다.

claude 의 `needs_input`(사람 승인 대기)은 **성공** 쪽에 남는다. 사람이 승인해주면 이어서 끝나는 상태라 영구 실패가 아니고, 두 plugin 의 spawn/tell 완료 알림이 이미 `idle` 과 동일 취급하는 계약을 깨지 않기 위함이다. 승인 대기를 실패로 보고 싶으면 그 노드에 인라인 `poll` 을 주어 `failure_states` 를 직접 지정한다.

앞 노드의 산출물을 뒤 노드 파라미터로 넘기는 것(예: spawn 이 만든 자식의 surface id 를 뒤따르는 tell 노드에 넘기기)은 위 "노드 간 데이터 흐름(`${task.<id>.output<pointer>}`)" 이 다룬다.

전략 레지스트리·dispatch 배선 상세는 [dev-guide/agent-runner](../../dev-guide/agent-runner.md#hostexecutor-매핑).

### 사건으로 받기 — 폴링하지 않고

task 조회의 어떤 필드도 세션 생존 판단의 신뢰도를 싣지 않는다. 터미널 상태 조회(`terminal.state`)의 `confidence` 는 상태 관측(idle·needs_input 등)을 얼마나 믿을지에 대한 값이고, v1 custom task 가 Claude·Codex 완료 전략으로 끝나면 그 상태 응답이 `result.output` 에 그대로 남을 뿐이다. 모델이 내린 판단의 확신도는 사용자가 출력 타입에 선언한 필드(예: `{"verdict": ..., "confidence": float64}`)로만 오며, 전이 조건은 그 필드를 비교한다. 두 값은 이름이 같아도 서로 대신하지 않는다.

종결 사실은 Event Bus 로도 나간다. plugin 이 `agent.task_finished` · `agent.barrier_closed` 를 구독하면 `task_get` 을 되풀이해 묻지 않아도 된다. 두 키의 payload·등급·구독 조건은 [reference/event-catalog](../../reference/event-catalog.md#agent-scopesystem-experimental) 이 정본이고, 무엇을 싣고 무엇을 안 싣는지의 근거는 [Event Bus 1.0 카탈로그](../../reference/event-catalog.md) 이다.

경계 셋만 여기 적는다.

- **종결만 나간다.** `waiting`/`ready`/`running` 으로 들어가는 전이는 이벤트를 보내지 않는다 — 종결에는 모든 진입 경로가 지나는 공통 처리 지점(`task_await` 를 깨우는 그 자리)이 있고 비종결에는 없다. 결과 불명 뒤에는 영속 알림 의무의 확인 읽기를 통해 전달한다.
- **lease 만료는 사건이 아니다.** 만료는 전이가 아니라 조회 시점에 확인하는 조건이라 "언제 일어났다" 가 없다.
- **왜 실패했는지는 payload 에 없다.** 실패 원인은 `task_id`로 `task_get` 을 부른다 — 구독 권한과 호출 권한이 다른 것이 그 이유다.

발화는 `task_await` 의 blocking 동작과 간섭하지 않는다. 같은 처리가 대기자에게 알리고 피드에도 기록하며, **대기자가 없어도 피드에는 적힌다.** 종결 기록과 고유 알림 의무를 원자적으로 저장한다. v1·v2 보고, IPC 전이, 훅 완료와 재시작 복구에서 응답을 잃어도 확인 읽기가 성공하면 종결된 하류까지 전달하며 timeout 0 대기자도 깨어난다. 결과 불명 응답은 성공으로 바뀌지 않고 외부 명령을 다시 실행하지 않는다. 같은 허브는 같은 `terminal_id`를 중복 발행하지 않는다. 전달 뒤 확인 처리 전에 프로세스가 종료되면 재시작 후 같은 ID가 재발행될 수 있다. 소비자는 이 ID로 재전달을 구분한다. 피드는 휘발성 bounded 큐라 외부 수신까지 보장하지 않는다. 확인 읽기가 실패하면 원본을 보존하고 다음 task 변경 또는 재시작 때 다시 확인한다. 자세한 범위는 [agent-runner §실행 회차와 완료](../../dev-guide/agent-runner.md#실행-회차와-완료).

plugin 이 아닌 쪽은 `events.fetch` 로 같은 사건을 **위치로** 읽는다. 이 메서드는 로컬 호출자만 사용할 수 있다 — 세션 토큰을 들고 붙은 외부 에이전트는 이 메서드를 직접 못 부르고, 로컬 소켓의 `tasty` CLI 를 경유한다. `tasty events follow --filter 'agent.*'` 가 그 루프이고, 한 줄에 한 사건씩 JSON 으로 찍으므로 셸에서 `while read` 로 받는다. 끊겼다 다시 붙을 때는 마지막 위치를 그대로 주면 그 사이 사건부터 이어 받고, 기다린 사이 위치가 링 밖으로 밀렸으면 설명 없이 처음부터 반환하지 않고 건너뛴 수를 알린다. 재시작을 넘는 재부착은 세대를 함께 준다 — 연결이 끊기면 `follow` 가 다시 붙을 `--offset` · `--epoch` 을 stderr 에 찍고 끝나고(`--reconnect` 면 1 초마다 다시 붙는다), 그 세대가 새 세대와 다르거나 위치가 새 피드의 끝보다 뒤면 stderr 로 알리고 새 세대의 처음부터 잇는다([Event Bus 1.0 카탈로그](../../reference/event-catalog.md)). 커서를 소비자가 드는 이유와 재시작이 위치를 리셋하는 이유는 [Event Bus 1.0 카탈로그](../../reference/event-catalog.md).

## 인터페이스

- **AI Agent / CLI**: `tasty agent {task-create,task-list,...,dag-list,dag-get,dag-report,report append,barrier-*,semaphore-*,lease-*,task-reduce,rate-limit-*}`. `--command`/`--metadata` 는 인라인 JSON 또는 `@path`. 전체 표 → [reference/api](../../reference/api.md#에이전트-협업-agent).
- **state 필터는 콤마 다중값** — `task-list --state`(단수) 와 `task-purge --states`(복수)는 플래그 이름만 다를 뿐 같은 파싱을 쓴다: `--state waiting,ready,running` 처럼 여러 state 를 OR 로 매칭하고, 단일값도 그대로 동작한다. "아직 안 끝난 task 가 남았는가" 를 한 번의 조회로 판정하는 완료 감지의 기본 패턴. 예시 → [dev-guide/agent-runner §CLI 예](../../dev-guide/agent-runner.md#cli-예).

## 재시작 동작

task 는 영속되지만(`Scope::Workspace`) runner thread 는 in-memory 다 — 호스트 재시작 후 **runner 는 자동으로 켜지지 않는다.** 대신:

- **취소는 실행 중인 프로세스를 끝낸다** — 실행 중인 `run` 작업(과 후처리)을 취소하면 그 명령과 명령이 띄운 프로세스(Unix 프로세스 그룹, Windows job)를 끝내고, 끝난 것을 확인한 뒤 semaphore·lease 를 반환한다. 명령이 먼저 끝났어도 그 명령이 띄운 프로세스가 출력을 쥐고 있어 작업이 아직 실행 중이면 그 프로세스들을 끝낸다. 출력을 닫고 남은 데몬은 작업이 끝나면 작업의 일부가 아니다. 러너가 꺼져 있으면 취소 요청은 프로세스 종료를 기다리지 않고 응답하고, 종료 확인과 반환은 백그라운드에서 한다. 반환 전까지 점유는 남는다. 끝낼 프로세스는 저장한 PID 와 시작 시각으로 같은 프로세스임을 확인하며, Tasty 가 다시 시작한 뒤 Windows 는 직접 자식만 끝낸다(손자는 남을 수 있다). 러너 정지는 취소가 아니라 프로세스를 끝내지 않는다.
- **부팅 시 상태 정리 1회** — 라이브 workspace 전부에 대해 stale semaphore/lease holder 회수(그 holder 인 `Running` task 는 `Failed("host restart")` 로 마감. 단 저장된 Run handle 로 다시 감시할 task 는 프로세스가 살아 있는 동안 점유를 유지하고 끝난 뒤 반환한다. 후처리를 기다리던 task 는 점유를 쥔 채 예약대로 후처리를 실행한다) + persisted `DispatchHandle` reload(살아있는 `ShellProcess`/`PolledDispatch`/`BarrierPoll`/미만료 `AwaitExternal` 는 복원, 죽은 건 마감, 이미 Running 이 아닌 task 는 남은 프로세스를 끝내고 종료를 확인한 뒤 handle 을 지우고 그 task 가 쥔 점유를 반환). Run 의 종료 결과를 회수할 수 없으면(죽은 pid 에 저장된 결과가 없거나, 복원한 pid 가 나중에 끝남) 그 task 는 `unknown` 이 되고 `retry`·`cancel` 을 기다린다. runner thread 는 여전히 안 켜져 있으므로 복원된 handle 을 실제로 poll 하려면 수동(또는 plugin) `agent.task_run --action start` 가 필요하다.
- **같은 부팅 정리 경로에서 자동 GC 도 함께 돈다** — 상태 무관 + 잠정 임계값(7일) 이상 방치된 task 를 `task_purge` 와 동일한 참조 안전 로직(`plan_sweep`/`apply_sweep_plan`)으로 쓸어낸다. memory 자체 TTL(`PutOpts.expires_at`)은 쓰지 않는다 — TTL 만료는 참조 무결성·상태 검사를 우회해 dangling 참조를 재도입하기 때문. 상세: [dev-guide/agent-runner](../../dev-guide/agent-runner.md#자동-gc).
- **정지 상태는 조회로 드러난다** — `task_list`/`task_graph` 응답에 `runner: { running, crashed, state, ready_count, running_count, store_error, list_failures }` 가 동반된다. `state` 는 `running`·`stopping`·`stopped`·`crashed` 중 하나다. runner 가 꺼져 있어도 `ready_count`/`running_count` 는 store 의 실제 값이라, "할 일은 있는데 아무도 안 돌리고 있다"가 이 응답만으로 드러난다. store 를 못 읽으면 두 카운트는 `null` + `store_error` 이고(0 으로 흡수하지 않는다), 러너가 살아 있는데 계속 못 읽는 상태는 `list_failures` 로 드러난다. `task_get` 은 `AwaitExternal` 로 외부 신호를 기다리는 task 에 `awaiting_external: { wait_key, deadline_ms }` 를 실어 "그냥 running" 과 구분한다.
- **후처리 단계의 v2 task 는 회차의 진행으로 복원한다** — 예약만 된 실행은 이어서 실행하고, 시작했는데 저장된 보고가 없으면 `outcome_unknown` 으로 끝내며 다시 실행하지 않는다. 그 후처리 프로세스가 재시작 뒤에도 살아 있으면(저장한 PID·시작 시각으로 확인) 끝날 때까지 Running 으로 두고 semaphore·lease 를 쥔 뒤 `outcome_unknown` 으로 끝내며 반환한다.
- **`hook_task_waits`(push 완료 전략의 hook_id → task_id 매핑)는 비영속** — 재시작하면 그 task 는 훅으로는 깨어날 수 없다. 대신 `AwaitExternal` handle 자체가 `deadline_ms` 를 들고 다니므로(핸들은 영속), 다음 재시작의 reload 가 만료 여부를 독자적으로 판정해 마감한다.
- **stop 은 기다리지 않는다** — `agent.task_run` stop 은 정지를 요청하고 바로 응답한다. 정지 요청 뒤에는 아직 보내지 않은 Ready task 를 dispatch 하지 않는다. 이미 진행 중인 dispatch, Running task 의 poll, 후처리 정리 대기가 끝날 때까지 runner 가 살아 있을 수 있어 종료까지 걸리는 시간의 상한은 없다. runner 가 끝나기 전까지 응답과 이후 status 의 `state` 는 `stopping` 이고, 그동안 start 는 새 runner 를 띄우지 않는다(오류는 아니다). 이미 끝난 runner(crash 포함)에 보낸 stop 은 `stopped` 로 답한다. 정지 완료는 status 의 `state: stopped` 로 확인한다.
- `agent.task_run` 은 plugin 도 호출 가능(`AgentManage`) — 자동 시작이 없으므로 plugin 이 자기 workspace 의 runner를 직접 시작할 수 있어야 하기 때문이다.

상세: [dev-guide/agent-runner](../../dev-guide/agent-runner.md#재시작-계약).

## 에러 코드

`-32004`(not found) · `-32008`(already terminal) · `-32009`(lease conflict) · `-32010`(task 참조 중 — `task_delete` 기본 거부, `error.data.referenced_by` 에 참조자 목록) · `-32011`(task 가 `running` — 삭제 불가, `cancel` 선행 필요) · `-32012`(lease pool 소진 — fixed 전부 점유 중이거나 elastic `max_candidates` 상한 도달, mode `fail`) · `-32018`(회차 결과 거절 — 완료 보고는 다른 실행 회차의 보고이거나 이미 끝난 회차에 다른 보고, agent task 결과 제출은 `not_running`·`stale_attempt`·`conflict`·`turn_ended`·`not_the_session`. `error.data` 에 `reason`·`attempt_id`·`current_attempt_id`. report append 는 `token_mismatch`·`closed`·`not_the_session` 이고 `error.data` 에 `reason`·`task_id`·`attempt`·`state`) · `-32602`(사이클/미존재 dep(`depends_on`/`Fallback.task`/`Reduce.inputs`)/잘못된 strategy/`depends_on` 밖을 가리키거나 문법이 깨진 `${task.<id>.output…}` 참조, 결과 전 레코드 상한(기본 768 KiB, memory 값 상한에 맞춘다)을 넘는 task 정의 등) · `-32603`(internal).

**이름·id 의 문자 규칙.** semaphore·barrier 의 `name`, task 의 `id`, rate-limit 의 `id` 는 그대로 memory 키의 한 조각이 되므로 memory 키 규칙([design/systems/memory](../../design/systems/memory.md) — 소문자 `a-z`·`0-9`·`.`·`_`·`-`, 접두사 포함 256 바이트)을 따른다. 어기면 `-32602` 이고, 메시지는 **호출자가 준 값 기준** 문자 좌표(0 부터)와 그 문자로 무엇이 틀렸는지 말한다(`semaphore name "v6S": invalid char at 2: 'S' (allowed: …; at most 234 bytes)` — 다바이트 문자도 입력한 그대로, `aé` 면 `invalid char at 1: 'é'`) — 상한 바이트 수는 종류마다 접두사 길이만큼 다르다. 판정은 생성뿐 아니라 그 값으로 키를 만드는 모든 호출(acquire·release·delete·get 등)에서 같다. lease 의 `resource` 는 키에 넣기 전에 인코딩하므로 이 규칙이 없다.

## 관련

- [telemetry](../telemetry/index.md) — rate-limit vs cap 구분 · [human-handoff](../human-handoff/index.md) — approval
- [design/systems/memory](../../design/systems/memory.md) — 영속 backing store
- [dev-guide/agent-runner](../../dev-guide/agent-runner.md) — task runner 내부 동작(dispatch/poll, 완료 판정 전략 레지스트리)
- 이 문서 — task graph를 화면 두 곳에서 제공하고 host 내장 기능으로 구현한 이유

DAG 화면의 비동기 조회·대상 세대 검증·취소 규약은 [그래프 surface](../../surfaces/dag-graph/index.md#조회-소유와-취소)에 있다. 목록 popup도 같은 App 조회 서비스를 쓴다.
