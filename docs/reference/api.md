# IPC / CLI API 레퍼런스

주요 IPC·CLI를 네임스페이스별로 정리한 목록이다. 모든 메서드를 나열하지는 않는다.
정확한 메서드·권한은 `crates/tasty-ipc/src/method_meta.rs`의 `METHOD_TABLE`/`DEBUG_METHODS`와
`src/adapters/ipc/handler.rs`를 기준으로 한다. 상세 동작은 연결된 기능 문서를 참고한다.

## 접속

```python
import json
import os
import socket
from pathlib import Path

# 기본은 release 데이터 루트. debug/격리 인스턴스는 TASTY_HOME을 지정한다.
data_root = Path(os.environ.get("TASTY_HOME") or "~/.tasty").expanduser()
port = int((data_root / "tasty.port").read_text().strip())

def call(method, params=None):
    request = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=5) as connection:
            connection.sendall((json.dumps(request) + "\n").encode())
            with connection.makefile("rb") as responses:
                line = responses.readline()
                if not line.endswith(b"\n"):
                    raise EOFError("JSON-RPC 응답이 끝나기 전에 연결이 닫혔습니다.")
                return json.loads(line.decode("utf-8"))
    except TimeoutError as error:
        raise TimeoutError("IPC 연결 또는 응답 대기 시간이 지났습니다.") from error
```

- 전송: loopback TCP, 동적 포트(`~/.tasty/tasty.port`), 줄단위 JSON-RPC 2.0. 근거 [IPC 서버](../architecture/ipc-server.md).
- 대부분의 CLI 서브커맨드가 동명 IPC 를 감싼다. 대상은 가능한 한 ID로 직접 지정한다. 생략 시의 기본 대상은 메서드별 규칙을 따른다([focus 정책](../design/policies/focus.md)).
- **포커스 독립**: release 표면엔 포커스 변경 API 가 없다. 사용자 입력 재현(키/마우스 주입, popup 강제 open, `window.focus`)은 debug 전용 → [debug-ipc](../dev-guide/debug-ipc.md).

응답은 TCP 수신 횟수가 아니라 개행으로 구분한다. 위 예제의 5초는 각 소켓 대기의 상한이며,
조금씩 계속 도착하는 응답까지 포함한 전체 호출 기한은 아니다. EOF·연결 오류·잘못된 JSON은
호출자에게 예외로 전달된다.

- 응답을 받을 때까지 연결을 연 채 둔다. 요청을 보낸 뒤 쓰기 쪽만 닫으면(half-close, 예: `nc -N`) 서버는 클라이언트가 떠난 것으로 보고, 1초보다 오래 걸리는 요청의 응답을 보내지 않는다([IPC 서버](../architecture/ipc-server.md#입장-상한)).
- 연결을 닫아도 보낸 요청은 취소되지 않는다. 호스트가 바쁘면 나중에 실행된다.
- 응답이 없다고 짧은 제한 시간으로 같은 요청을 반복하지 않는다. 호스트가 멈춘 동안 반복한 요청은 큐에 쌓였다가 회복 뒤 몰아 실행되고, 쓰기 명령이면 같은 입력이 반복 횟수만큼 재생된다. 호스트 회복은 `system.info` 같은 읽기 요청 하나를 응답이 올 때까지 기다려 확인하고, 쓰기 명령은 재시도하지 않는다([IPC 서버](../architecture/ipc-server.md#기한)).

## 권한

plugin caller 는 메서드별 권한 토큰이 필요하다(`method_meta`). Local(CLI/사용자)은 이 메서드 권한 검사에서 면제된다. 토큰 목록·게이트는 [concepts/plugins](../concepts/plugins.md) · [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md). 런타임 권한 상승(capability elevation)·audit 은 [features/capability-elevation](../features/capability-elevation/index.md).

## 네임스페이스

### 구조 — workspace / pane / tab / surface / split / tree
`workspace.{list,create,update,move}` · `pane.{list,close}` · `split` · `tab.{list,create,close,move}` · `tree`. 도메인은 [work-area](../features/work-area/index.md).

### Surface 상호작용
- 입력: `surface.{send,send_key,send_combo,send_to,send_wait_idle,wake,respawn_terminal}`
- 읽기/마크: `surface.{set_mark,read_since_mark,parse_since_mark,read_since_scan_mark,screen_text,cursor_position,foreground_process,is_typing,locate}`. `foreground_process`는 전경의 `name`·`pid`와 함께 그것이 터미널의 셸인지(`is_shell`)를 같은 조회 결과로 돌려준다. 전경 PID가 터미널의 루트 셸 PID이거나 이름이 알려진 셸이면 `true`이고(busy 판정이 유휴로 보는 조건과 같다), 터미널이 없거나 조회할 수 없으면 세 값이 모두 `null`이다. 원격 attach mirror는 로컬 PTY가 없어 서버가 보낸 원격 전경 이름을 `name`으로 답하고, `pid`는 이 머신의 프로세스가 아니므로 `null`, `is_shell`은 이름으로만 판정한다. 첫 값을 받기 전이나 구버전 서버에서는 지금처럼 세 값이 `null`이다. `screen_text`(및 `pty.read`)는 dim(ghost-suggestion, 예: Claude Code 자동완성 제안) 셀을 기본 제외 — `show_dim:true`(CLI `--show-dim`)로 포함.
  - `read_since_mark` 은 **위치를 받을 수 있다.** `cursor`(절대 바이트 위치) + `stream`(스트림 표지)을 주면 마크 대신 그 위치부터 읽고, **소비자가 읽기 위치를 보관한다** — 여러 소비자가 서로의 조회 범위를 바꾸지 않는다. 안 주면 예전 그대로 `set_mark` 이 세운 마크에서 읽는다. `max_bytes` 로 한 번에 읽을 **원문** 바이트를 줄일 수 있고(상한은 보존 크기 1 MiB), 생략하면 보존 전체다. **하한은 1 이다 — `0` 은 한도 없음이 아니라 1 바이트로 올린다**(0 이면 기다리는 바이트가 있어도 매번 빈 답이라 소비자가 못 나아간다). 음수·정수 아닌 값은 인자 오류로 거절한다.
    - 응답은 두 형태 모두 `stream` · `cursor`(실제로 읽기 시작한 위치) · `next_cursor` · `raw_bytes` · `retention_start` · `retention_end` · `skipped` 를 함께 싣는다. **`skipped` 가 0 이 아니면 그만큼이 보존 범위를 벗어나 더 이상 조회할 수 없다** — 마크가 잘려 나가도 이제는 조용히 처음부터 주지 않는다.
    - **전진은 `next_cursor` 가 정한다.** `text` 의 길이는 읽은 원문 구간의 길이가 아니다(손실 디코딩이 U+FFFD 를 넣고 `strip_ansi` 가 바이트를 뺀다). 그 길이로 이어 읽으면 스트림과 어긋난다.
    - `cursor` 에는 `stream` 이 **반드시** 따라야 한다 — surface id 는 재사용되고 `respawn_terminal` 은 id 를 그대로 둔 채 터미널을 바꾸므로, 표지가 없으면 옛 위치가 남의 출력에 조용히 적용된다. 표지가 안 맞는 위치 · 스트림 끝을 넘은 위치 · 터미널이 없는 surface 는 **빈 답이 아니라 거절**이고, `error.data.reason` 이 `stream_mismatch` · `cursor_ahead_of_stream` · `cursor_without_stream` · `no_terminal` 로 갈린다 — [터미널 출력 구조화 (Terminal output)](../features/terminal-output/index.md).
    - CLI 는 `tasty read since-mark --cursor N --stream S [--max-bytes N]` 이다. 구 서버는 이 인자를 **조용히 버리고** 공유 마크에서 읽으므로, 서버는 capability `ipc.output-cursor` 로 이 계약을 선언하고 CLI 는 위치 인자를 실은 요청을 보내기 전에 그 이름을 묻는다 — 없으면 보내지 않고 구조화된 거절(`sent:false`)로 끝난다([터미널 출력 구조화 (Terminal output)](../features/terminal-output/index.md)).
  - `read_since_scan_mark` 는 **다른 커서**를 쓴다 — 주기적으로 출력을 훑는 소비자(번들 claude plugin 의 에러 스캐너)용이고, 읽으면 커서가 그만큼 전진하므로 같은 구간이 두 번 오지 않는다. `set_mark` 이 그 커서를 밀지 않고 그 반대도 아니다. 소비자가 하나라는 전제 위에 있어 CLI 명령은 제공하지 않는다 — [터미널 (Terminal)](../surfaces/terminal/index.md).
  - `lines:N`(CLI `--lines N`) 생략 시 보이는 화면 전체. 지정하면 **내용 기준 마지막 N 줄** — 내용 아래의 공백 행은 건너뛰고, 화면 내용이 N 에 모자라면 스크롤백에서 채운다(합쳐도 모자라면 있는 만큼). 내용 *중간* 의 빈 줄은 출력의 일부이므로 보존하고 줄 수에도 포함한다. 빈 행 판정은 `show_dim` 과 같은 값으로 하므로 `--show-dim` 유무가 반환 줄 수를 바꾸지 않는다. alternate screen(TUI)이 떠 있을 때도 부족분은 primary 스크롤백으로 채운다 — alt screen 은 자체 스크롤백이 없기 때문이다.
  - **N 보다 적게 왔을 때 왜인지 물을 수 있다.** 응답에 `is_terminal`(그 surface 뒤에 터미널이 있는가) · `scrollback_len`(현재 스크롤백 줄 수) · `alt_screen`(대체 화면인가)이 함께 실린다. `scrollback_len: 0` 이면 **받은 것이 가진 전부**이고(정상 포화), 0 이 아닌데 N 보다 적게 왔다면 그건 결함이다. 터미널이 아닌 surface(markdown/html/explorer/image)나 없는 surface 는 `is_terminal: false` 와 두 필드 `null` 로 나온다 — **`0` 이 아니다**(0 은 "스크롤백이 비었다" 라는 다른 사실이다). 같은 필드가 `pty.read` 응답에도 실린다.
  - **dim 제외는 화면 전체에 적용된다.** 기본 `show_dim:false` 는 커서 행의 제안만이 아니라 SGR 2(흐림)로 그린 모든 셀을 뺀다. 앱이 본문을 흐리게 그린 상태면 기본 읽기는 거의 빈 화면처럼 보인다. 응답의 `dim_omitted_rows` 가 dim 셀을 빼서 내용이 달라진 보이는 화면 행 수를 알린다(스크롤백 행은 세지 않는다. `show_dim:true` 면 0, 터미널이 아니면 `null`). 0 보다 크면 `show_dim:true` 로 다시 읽어 비교한다.
  - **attach mirror 터미널은 서버 원본을 읽는다.** `screen_text`·`read_since_mark`·`set_mark` 의 대상이 mirror 면 holder 연결로 서버 터미널을 읽어 답하고 `remote: true`·`remote_surface_id` 를 붙인다. stream·cursor·`skipped` 는 서버 값이다. 한 번에 받는 원문이 더 작을 수 있으므로 `next_cursor` 로 이어 읽는다. 서버가 지원하지 않으면 `remote_read_unsupported`, 연결이 없으면 `remote_disconnected`, 10초 안에 답이 없으면 `remote_timeout`, 화면이 한 프레임을 넘으면 `too_large` 로 거절한다. mirror 의 `set_mark` 는 서버의 공유 마크를 바꾸지 않는다([Attach 메커니즘](../dev-guide/attach-behavior.md#mirror-터미널-읽기-ipcstreamterminal-read)).
- 명령(OSC 133): `surface.{commands,last_command,command_at}`
- 메타: `surface.meta.{set,get,unset,list}` · `surface.set_cwd`
  - 지금 열린 surface 에만 쓰고 읽는다. 닫혔거나 없는 `surface_id` 는 다른 surface 대상 요청과 같은 `-32602`(`no live surface <id> (named by 'surface.meta.set')`)로 거절하고 저장소에 항목을 만들지 않는다. surface 를 닫으면 그 surface 의 metadata 도 함께 지운다.
- 주의 환기(attention): `surface.completion`(발동) · `surface.attention.{get,clear}`(조회·해제). 해제는 `kind` 선택 필터를 받고, 하드 점유 중인 surface 와 mirror surface 는 거절한다(그 상태의 소유자가 다른 인스턴스다) — [surface-highlight](../features/surface-highlight/index.md)
- 출력 옵저버: `output.observe_{start,stop,list,info}`
- 동작·파서는 [terminal-output](../features/terminal-output/index.md), 파서 카탈로그 [output-parsers](output-parsers.md). (IME `surface.ime_*` 는 debug 빌드 전용 local-only — release 표면에 없다.)

### 메모리 (`memory.*` / `memory.secret.*`)
regular(`put/get/delete/list/exists/count/scopes/stats/query/export/import`) · secret(`put/get/delete/list/exists/count/scopes/stats`) · `gc` · blackboard(`bb_*`) · plan(`plan_*`) · cache(`cache_*`) · goal(`goal_*` — surface 스코프 단일 목표 문장, `surface_id` 명시 필수).
모델·권한은 [design/systems/memory](../design/systems/memory.md).
`surface:<id>` scope 에 새 항목을 만드는 `memory.put` · `memory.secret.put` · `memory.import` 는 열린 surface 에만 쓰고, 닫혔거나 없는 surface 는 `-32602` 로 거절한다. `memory.import` 는 항목 하나라도 걸리면 요청 전체를 거절하고 아무 항목도 넣지 않는다. 읽기·삭제는 그대로 받는다(남은 옛 항목 정리용) — [design/systems/memory](../design/systems/memory.md).
저장소 자체가 실패하면 `-32603 memory db error: …` 이고 `error.data.storage_failure` 가 원인을 싣는다(`busy` · `disk_full` · `io` · `corrupt` · `permission_denied` · `other`) — 실패한 쓰기는 quota 도 변경 알림도 남기지 않는다([storage](../design/systems/storage.md) "저장 실패의 의미").
부팅이 `memory.db` 를 못 열어 in-memory 대체로 떴으면 쓰기 계열의 성공 응답에 `durable: false` 가 더해진다(`ok` 는 그대로, 정상 저장소에서는 칸 없음 — [저장 계약과 writer 명부](../design/systems/storage-contract.md)).
같은 저장소에 쓰는 `agent.*` · `approval.*` · `surface.meta.*` · `telemetry.*` · `session.*` 의 쓰기 응답도 같다([저장 계약과 writer 명부](../design/systems/storage-contract.md)).
`memory.*` · `memory.secret.*` · `surface.meta.*` · `approval.*`(기록을 쓰거나 읽는 메서드)와 에이전트 협업·task 의 `agent.barrier_*` · `agent.semaphore_*` · `agent.lease_*` · `agent.rate_limit_*` · `agent.task_*` · `agent.task_graph_*` · `agent.dag_*` · `agent.report_append` · `agent.task_submit`, telemetry 조회·cap 관리(`telemetry.summary` · `telemetry.timeseries` · `telemetry.top` · `telemetry.session_summary` · `telemetry.anomaly.list` · `telemetry.cap.*`), 감사 조회(`plugin.audit_query` · `plugin.audit_summary`), 세션(`session.*` · `plugin.grant_agent_permission` · `plugin.revoke_agent_permission` · `plugin.list_agent_permissions`), 명령 기록 조회(`surface.commands` · `surface.last_command` · `surface.command_at`) 는 memory writer 가 commit 한 뒤 응답한다. 응답 시한이 지나거나 연결이 끊겨도 이미 접수한 요청은 취소되지 않는다. 같은 `idempotency_key` 의 Mutate 요청을 결과 전에 다시 보내면 `-32061` 로 답한다. writer 를 거치며 생기는 경우의 응답은 다음과 같다([storage-contract](../design/systems/storage-contract.md#ipc-응답)).

| 경우 | 코드 | 메시지 | `error.data` |
|---|---|---|---|
| 큐 포화(접수 거절, 다시 시도할 수 있다) | `-32603` | `memory writer queue is full (<요청 수> requests, <바이트> bytes held)` | `{"storage_failure":"busy","writer":"saturated"}` |
| 종료 중 접수 거절 | `-32603` | `memory writer is not accepting requests` | `{"storage_failure":"other","writer":"closed"}` |
| commit 결과 불명(원인 오류 있음) | 원인 오류의 코드 | 원인 오류의 기존 문구(`memory db error: …`) | 원인 오류의 `data`에 `"outcome":"unknown"`을 더한다 |
| worker 소실로 결과 불명 | `-32603` | `memory writer ended before the request finished; it may have been stored` | `{"storage_failure":"other","outcome":"unknown"}` |
| 요청 본문 panic | `-32603` | `memory request panicked` | `{"storage_failure":"other"}` |
| 저장 실패·busy 재시도 시한 초과·요청 거절 | 기존 매핑 그대로 | 기존 매핑 그대로 | 저장 실패만 `storage_failure` |

### 에이전트 협업 (`agent.*`)

일반 메서드는 `agent`(AgentManage) 권한이 필요하다.

| 메서드 | 역할과 제한 |
|---|---|
| `task_{create,list,get,cancel,retry,graph,reduce,run,delete,purge}` | 작업 생성·조회·실행·정리 |
| `task_create` | `command.kind=custom`의 인라인 `poll.interval_ms`는 task별 조회 완료 뒤 최소 대기(ms, 기본500). `tasty agent task-create --command`도 같은 JSON을 받는다. 최초·복원 조회와 기한은 [조회 간격과 기한](../dev-guide/agent-runner.md#조회-간격과-기한)을 따른다. |
| `task_run` | workspace 러너의 시작·중지·상태 조회. 호스트 재시작 후 자동으로 켜지지 않으므로 플러그인이 자기 workspace의 러너를 다시 시작할 수 있다. |
| `barrier_*`, `semaphore_*`, `lease_*`, `rate_limit_*` | 작업 간 대기와 자원 사용 조정 |
| `dag_{list,get}` | 작업을 DAG별로 묶어 조회 |
| `dag_report` | DAG 하나의 report(`id`·`workspace_id?`·`task?`·`attempt?`·`include_raw?`). task 마다 레코드에서 만든 자동 항목 `auto` 와 회차별 custom 기록 `attempts[].custom` 을 싣는다. retry 로 닫힌 회차는 그때 굳힌 자동 항목을 `attempts[].auto` 로 싣는다. 실패한 Run 회차도 `exit_code` 와 출력 꼬리(`raw`)를 싣는다. 표준 출력·오류는 `include_raw` 일 때만 싣는다. `attempt` 는 `task` 가 있어야 한다([agent-runner](../dev-guide/agent-runner.md#dag-report)) |
| `task_graph_{validate,submit}` | v2 task 그래프를 한 번에 검증·제출. 검증 실패(task 1000 개 초과 포함)는 `-32602` 이고 `error.data.location` 에 그래프 안의 위치가 실리며 아무것도 저장하지 않는다. `submit` 은 모두 저장한 뒤 활성화한다. memory 대체 모드에서는 `durability: "best_effort"` 를 밝힌 그래프만 받는다. 활성화 뒤 readiness 반영 실패는 `-32603` 이고 `error.data` 에 `graph_id`·`possibly_active: true` 가 실린다([agent-runner](../dev-guide/agent-runner.md#그래프-제출)). task 의 `transitions` 로 성공한 출력에 따라 후속 task 를 고르며, 고른 경로는 task 의 `route` 에, 고르지 않아 끝난 이유는 `skip` 에 실린다([agent-runner](../dev-guide/agent-runner.md#전이-조건과-경로-선택)) |
| `task_submit_result` | v2 `agent` task 의 지금 회차에 구조화 결과를 제출한다(`workspace_id`·`id`·`attempt_id`·`token`·`output`. `token` 은 지시에 실린 회차 토큰). 도착할 때 출력 타입으로 검사하고 턴이 끝날 때 결과로 확정하며, 응답 `final: false` 는 task 가 아직 끝나지 않았다는 뜻이다. 거절은 `-32018` 과 `error.data.reason`(`not_running`·`stale_attempt`·`conflict`·`turn_ended`·`not_the_session`·`wrong_token`). 세션 토큰으로 부른 agent 는 자기 세션의 task 에만 낼 수 있다([agent-runner](../dev-guide/agent-runner.md#agent-task)) |
| `report_append` | 실행 중인 v2 작업 회차의 report 에 텍스트 기록 하나를 더한다(`address`·`text`). `address` 는 러너가 자식에게 준 `TASTY_TASK_REPORT` 값이나 agent 지시에 실린 주소이며 토큰을 담는다. 크기 한도를 넘어 잘리거나 저장하지 않아도 성공이고 응답 `result` 가 `stored`·`omitted` 로 알린다. 토큰이 맞지 않거나 회차가 끝났으면 `-32018` 과 `error.data.reason`(`token_mismatch`·`closed`). agent task 의 세션이 세션 토큰으로 부르므로 `agent` 권한으로 연다. 세션 호출은 `task_submit_result` 처럼 자기 세션에 지시를 보낸 회차에만 쓰고, 아니면 `not_the_session` 이다. 세션이 아닌 agent id 는 `-32001`([agent-runner](../dev-guide/agent-runner.md#dag-report)) |
| `task_turn_report` | provider 플러그인이 훅에서 턴 시작·끝(`final_answer` 또는 `error`)을 알린다. `agent.turn_report` 권한(이 메서드만 연다)을 가진 플러그인 중 그 provider namespace 를 소유한 플러그인만 부를 수 있고(그 밖은 `-32001`), surface 에 묶인 agent task 회차가 없으면 아무것도 하지 않는다 |

다음 두 메서드는 **로컬 호출만 허용**하며 플러그인은 호출할 수 없다. 둘 다 [METHOD_TABLE](../../crates/tasty-ipc/src/method_meta.rs)에 `local_only()`로 등록되어 있다. 라우터가 처리하는 메서드는 모두 이 표에 등록한다. 미등록 메서드의 `UnknownMethod` 거부와 의도한 접근 제한을 구분하기 위해 `tests/ipc_router_table_parity.rs`로 누락을 검사한다.

| 메서드 | 로컬로 제한하는 이유 |
|---|---|
| `task_await` | 완료까지 호출을 대기시킨다. `approval.await`와 마찬가지로 플러그인 SDK의 단일 워커를 막지 않도록 제한한다. 기본 timeout은 10분이고 `timeout_ms:0`은 무한 대기다. |
| `task_set_result` | 외부에서 작업 완료를 알린다. 결과와 종결 상태를 함께 기록하며, 타입을 정한 작업은 `attempt_id` 로 보고할 실행 회차를 지정한다. 플러그인의 Custom task는 러너가 상태를 관리하므로 플러그인이 직접 완료 상태를 바꾸지 않고 완료 판정 전략을 선언한다. |

`task_delete`와 `task_purge`는 `depends_on`, `Fallback.task`, `Reduce.inputs` 참조를 검사한다. 참조가 남으면 기본적으로 거절하고 참조자 목록을 반환한다. `--cascade`는 연쇄 삭제, `--force`는 참조 검사만 우회한다. **`running` 상태의 제약은 `--force`로 우회할 수 없다.** 이전 회차의 프로세스 종료를 확인하는 중인 task(러너가 꺼진 동안 취소한 뒤 아직 확인하지 못한 task 등)도 지우지 않는다. `task_delete` 는 대상이나 cascade 로 함께 지울 task 중 하나라도 그렇다면 `-32602` 와 이유(`exit confirmation`)를 돌려주고, `task_purge` 는 그런 task 를 건너뛰고 응답의 `skipped`(id 목록)·`skipped_count` 로 알린다. 확인이 끝나 semaphore·lease 가 반환되면 지울 수 있다.

`task_command.kind = "run"`은 surface 없이 자식 프로세스를 실행한다. `task_get`과 `task_await`의 `result.output`에 stdout/stderr 각각 마지막 64KiB와 `truncated`/`dropped_bytes`를 반환한다. 0이 아닌 종료 코드로 실패해도 `result.exit_code`·`result.output` 에 종료 코드와 같은 출력을 싣고, `result.error` 문자열에는 줄기마다 마지막 2KiB 꼬리가 들어간다. 계약이 있는 v2 task 의 `result` 는 확정 결과의 투영이라 실패하면 `result.output` 이 비고, 출력은 `typed_result.raw.execution` 에 있다. 실행이 끝나는 때는 명령이 끝나고 stdout·stderr 가 모두 닫힌 때다. 실행 중에 `task_cancel` 하면 명령이 이미 끝났어도 그 명령이 띄운 프로세스(Unix 프로세스 그룹 전체, Windows job)를 끝내고, 모두 끝난 것을 확인한 뒤 semaphore·lease 를 반환한다. 러너가 꺼져 있으면 `task_cancel` 은 종료를 기다리지 않고 응답하며, 종료 확인과 반환은 백그라운드에서 한다. 반환 전까지 점유는 그대로다. 그 사이 `task_retry` 한 task 는 이전 회차의 프로세스가 모두 끝난 것을 확인할 때까지 `ready` 로 남고 시작하지 않는다.

작업 metadata 의 `lease.ttl_ms`·`semaphore.ttl_ms` 는 러너가 갱신하며, 하한(1000ms)보다 짧으면 `task_create`·`task_graph_submit` 이 `-32602` 로 거절한다. 갱신하기 전에 TTL 이 지나 다른 holder 가 자원을 가져가면 `task_get` 응답에 `holding_warnings` 배열(`kind`·`name`·`holder`·`at_ms`·`message`·`attempt_id`)이 실린다. 지금 회차의 기록만 싣고, 없으면 키가 없다. 하한이 생기기 전 버전에서 저장된 task 는 하한 검사를 받지 않는다.

`task_get` 은 레코드 `revision`(쓸 때마다 커지는 값)을 싣는다. revision 은 같은 레코드의 수명 안에서만 비교한다. task 를 지우고 같은 id 로 다시 제출하면 1부터 다시 센다. `task_await` 의 종결 응답은 `state`·`result` 와 함께 v2 task 의 `attempt_id`, `revision`, 건너뛴 이유 `skip` 을 싣는다. 이 셋은 종결 사건 `agent.task_finished` 의 같은 이름 필드와 같은 값이다([사건 목록](event-catalog.md#agent-scopesystem-experimental)).

`task_get` 과 `task_list` 의 각 task 는 실행 중인 타입 작업(`contract_version: 2`)에 실행 단계 `phase` 를 싣는다. 값은 `executing`(본 작업 실행), `awaiting_input`(agent 세션이 사람 입력을 기다림), `postprocessing`(후처리 실행 중), `retry_wait`(후처리 재시도 대기)다. v1 task 와 실행 중이 아닌 task 에는 없다. 예외로 `ready` task 가 이전 회차의 프로세스 종료 확인을 기다리느라 시작하지 못하면(v1 포함) `waiting_previous_attempt` 를 싣는다. `state.kind` 가 `unknown` 인 task 는 결과를 알 수 없게 된 이유를 `state.reason` 에 싣는다(예: 호스트 재시작 사이 run 의 종료 코드를 받지 못함, `run result lost: …`). 이유가 없는 옛 레코드는 `reason` 이 없다. unknown 은 종결이 아니며 `task_retry`·`task_cancel` 로 정한다.

`semaphore_set_permits`는 세마포어를 삭제하지 않고 한도를 바꾼다. 한도를 줄여도 기존 사용 권한을 강제로 회수하지 않고 새 acquire를 거절한다. `semaphore_acquire`의 `ttl_ms`는 선택 사항이다. 지정한 경우에만 사용 권한이 만료되어 회수되며, 기본값은 만료 없음이다([다중 에이전트 협업 (Agent collaboration)](../features/agent-collaboration/index.md)).

DAG는 별도로 저장하지 않고 `metadata.dag`의 명시적 지정 또는 작업 그래프의 연결 관계에서 도출한다. `dag_list`에서 `workspace_id`를 생략하면 현재 살아 있는 모든 workspace를 조회하고 `scope: "live_workspaces"`를 반환한다. `dag_get`은 해당 DAG의 작업만 골라 `task_graph`와 같은 `nodes`/`edges` 또는 dot 형식으로 반환한다. 전이는 `kind: "transition"` 간선이며 `selection`(`pending`·`selected`·`not_selected`·`unavailable`)을 싣는다. DAG 요약의 `state_counts.not_selected` 는 경로가 선택되지 않아 끝난 작업 수이고, `recovered` 는 fallback 이 대신 성공한 실패 수다. 성공·선택되지 않음·fallback 이 대신한 실패만 있으면 `rollup_state` 는 `succeeded` 다. `blocked` 는 저절로는 더 실행되지 않는 대기 수(선행 결과를 쓸 수 없어 대기로 남은 작업과, 그런 작업이나 `unknown` 작업을 전이적으로 기다리는 대기)이고, `succeeded_ends` 는 그룹의 끝 작업(그룹 안에 선택되지 않은 작업을 뺀 하류가 없는 작업, fallback 작업 제외) 중 성공했거나 fallback 이 대신 성공한 수다. `awaiting_input` 은 `running` 중 agent 세션이 사람의 입력을 기다리는(`phase: "awaiting_input"`) 작업 수이며 `rollup_state` 를 바꾸지 않는다. 저절로 진행할 수 있는 작업(`unknown` 제외)이 남아 있으면 실패가 섞여 있어도 `rollup_state` 는 `running`·`ready`·`waiting` 중 하나이고, 더 진행할 수 없을 때 복구되지 않은 실패가 있으면 성공한 끝 작업이 있을 때 `partially_failed`, 없을 때 `failed` 다. 자세한 흐름은 [agent-collaboration](../features/agent-collaboration/index.md)을 따른다.

### 사건 피드 (`events.*`)

`events.fetch {offset, max, filter, wait_ms}`는 Event Bus를 지난 사건을 위치로 조회하며 로컬 호출만 허용한다. 서버는 소비자별 커서를 저장하지 않는다. 소비자가 위치를 보관해 매 요청에 보낸다.

| 항목 | 동작 |
|---|---|
| `filter` | 구독과 같은 문법: 정확 일치 또는 `<ns>.*` |
| `wait_ms` | 새 사건을 기다리는 시간. 기본 0은 즉시 응답이며 상한은 60초다. |
| 응답 | `{events, next_offset, epoch, truncated, skipped, ahead_of_stream, stream_end}`. 각 사건에도 자신의 `offset`이 포함된다. |
| 보관 범위 | 메모리에만 최대 1024건 또는 16 MiB까지 보관한다. 둘 중 먼저 닿은 한도를 넘으면 오래된 사건을 내보낸다. |

보관 범위보다 오래된 위치로 요청하면 `truncated: true`와 건너뛴 수 `skipped`를 반환한다. 서버가 재시작하면 `epoch`이 바뀌므로 이전 인스턴스에서 받은 위치와 구별할 수 있다.

요청 위치가 현재 끝인 `stream_end`보다 뒤에 있으면 `ahead_of_stream: true`를 반환한다. 이때 `next_offset`은 요청 위치 그대로이며, `wait_ms`를 지정했다면 기다린 뒤 응답한다. 나머지 응답 필드는 이 표시가 추가되기 전과 같다([Event Bus 1.0 카탈로그](event-catalog.md)).

CLI는 `tasty events fetch`와 `tasty events follow`다. `follow`는 다시 연결할 때 `--epoch`으로 인스턴스를 구별한다. 연결이 끊기면 재연결에 쓸 인자를 출력하며, `--reconnect`를 지정하면 자동으로 다시 연결한다. 키 목록과 등급은 [event-catalog](event-catalog.md)를 참고한다.

### 휴먼 핸드오프 (`approval.*`)
`request,respond,await,cancel,get,list,history,summary.{set,get}`. `request` 의 metadata `kind` 에 host 전용 값 `capability_elevation` 을 주면 `-32602`(`invalid_request: …`)로 거절한다. host 가 발행한 권한 상승 요청에 agent·plugin 이 `respond`·`cancel` 하면 `-32019`(`user_response_required`)로 거절한다. [human-handoff](../features/human-handoff/index.md).

### 텔레메트리 (`telemetry.*`)
`record,record_batch,summary,timeseries,top` · `cap.{set,list,remove,status,reset}` · `anomaly.list` · `session_summary`. [telemetry](../features/telemetry/index.md).

### 세션 / attach
`session.{issue,revoke,list}` · `attach.{acquire,release,into_gui,list}`. `attach.into_gui` 는 GUI 전용이며 헤드리스는 접수하지 않고 `-32017` 로 거절한다. 연결되지 않은 저장된 원격 행을 쓰는 요청(입력·원본 읽기·구조 변경·`terminal.spawn` 등)은 그 행을 연결한 뒤 실행하며, 연결하지 못하면 `-32000` 과 `data.reason`(`window_parked`·`connect_timeout`·`user_disconnected`·`busy`(연결을 기다리는 요청이 상한에 찼다)·`remote_workspace_missing`·연결 실패 이유)으로 답한다([저장된 원격 행을 쓰는 요청](../features/remote-attach/index.md#저장된-원격-행을-쓰는-요청)). 사용자가 끊은 행(`user_disconnected`)은 아래 `remote.connect` 로만 다시 붙는다. attach 는 [remote-attach](../features/remote-attach/index.md), 신원 토큰은 [capability-elevation](../features/capability-elevation/index.md).

### 기타 호스트
- 알림: `notification.{list,create}` — [notifications](../features/notifications/index.md)
- 훅: `hook.{set,list,unset}` · `global_hook.{set,list,unset}` · `surface.fire_hook`
- 웹훅(인바운드 HTTP): `webhook.{register,list,info,unregister,sweep,config}` (`register`만 Network 권한을 가진 plugin도 허용, 나머지는 local-only) — [webhook](../features/webhook/index.md)
- 메시지 패싱: `message.{send,read,count,clear}`
- 파일 핸들러: `file_handler.{reload,detectors,dispatch}` — `detectors` 는 finalize 된 detector 와 출처별 contribution 조회(local-only, 읽기 전용). `dispatch` 응답은 접수(`accepted`, `run`)이고 최종 결과는 `run.result {run}` 으로 조회한다 — [file-handler](../features/file-handler/index.md#접수-뒤-최종-결과)
- 요청 결과: `run.result {run}` — `file_handler.dispatch`·`markdown.navigate`·`attach.into_gui`·기존 워크스페이스 `remote.attach`·`remote.connect` 접수의 진행·최종 결과(`state`: `running`/`succeeded`/`failed`/`cancelled`/`unknown`, `result` 영어 한 줄. `unknown`은 mirror 대상 요청을 보낸 뒤 원격 결과 전에 연결이 끝난 경우). plugin 도 권한 토큰 없이 부를 수 있고 자기가 시작한 run 만 읽는다 — [file-handler](../features/file-handler/index.md#접수-뒤-최종-결과)
- 훅 핸들러: `hook_handler.{list,get,upsert,remove,reload,dispatch,result}` (local-only) — 훅/웹훅 공유 핸들러 레지스트리 조회(비활성 포함)·단건 조회(action 본문 포함)·user 핸들러 제자리 수정/제거·user config(`~/.tasty/hook-handlers.toml`) 재로드·id로 수동 실행(IpcSequence/ShellCommand). `get` 의 `action` 과 `upsert` 의 `action` 은 같은 모양이라 왕복한다. upsert/remove 는 성공 시 user config 를 즉시 atomic write 하고, 쓰기에 실패하면 성공으로 보고하지 않는다. dispatch 응답은 접수(`accepted`, `run`)이고 최종 결과는 `result {run}` 으로 조회한다(`state`: `succeeded`/`partial`/`failed`/`unknown`) — [hooks](../features/hooks/index.md#수동-dispatch의-최종-결과)
- 완료 판정 전략: `completion_strategy.list` (local-only) — `agent.task_create` 의 `Custom.poll` 이름 참조가 가리키는 완료 판정 전략 레지스트리 조회(비활성 포함). reload/dispatch 대응물 없음(판정 함수이지 실행할 작업이 아님) — [agent-runner](../dev-guide/agent-runner.md)
- 이미지: `image.{open,save,export_png,next,prev,paste,list}` — [image plugin](../surfaces/image/index.md)
- 저장된 원격 행 연결: `remote.connect {workspace_id}` → `{attaching, workspace_id, run}` — 저장된 원격 행(로컬 workspace id)을 명시적으로 연결한다. 사용자가 끊은 행도 그 표지를 지우고 붙는다. 이미 연결된 행이면 `attaching: false` 와 성공으로 끝난 `run`, 연결 중인 행이면 그 시도에 합류한 `run` 을 준다. 결과는 `run.result` 로 읽는다. 바로 거절하는 경우는 `-32000` 과 `data.reason`(`not_a_saved_row`(로컬 workspace·에이전트 mirror·없는 id)·`remote_workspace_missing`·`window_parked`)이다. 목록 조회(`remote.workspaces`·`remote.refusals`)는 행을 연결하지 않는다. 포커스는 옮기지 않는다. GUI 전용, 멱등 분류 Idempotent — [저장된 원격 행을 쓰는 요청](../features/remote-attach/index.md#저장된-원격-행을-쓰는-요청)
- 원격 연결 프로필: `remote.profile.{list,get,add,detect,remove,list_local,import}`(`list_local`=로컬 `~/.ssh/config` alias 열거·읽기 전용, `import`=그 alias 를 ssh 프로필로 등록·셸 감지 없음)(구 `tool.ssh.*`/`ssh.profile.*`는 alias로 한시 호환) — [remote-profiles](../features/remote-profiles/index.md)
- webview: `webview.set_url {surface_id, url, label?}` — `url`이 `http(s)://`·`file://`이면 그 주소를, 아니면 raw HTML로 싣는다. `label`은 WebView가 숨겨졌을 때 host chrome이 보일 이름이며, 빼면 지운다. label이 없으면 chrome은 탐색 가능한 URL만 보이고 raw HTML은 보이지 않는다 — [html plugin](../surfaces/html/index.md#상태별-시각)
- html 스크립트·원격 콘텐츠 상태: `surface.html_script {surface_id}` — html surface의 현재 문서 감지 결과·지문·허용 기록·배너 단계와 표지를 읽는다. `remote` 객체는 원격 콘텐츠 차단 상태다: `global`(전역 설정)·`supported`(문서 단위 허용을 받는 view인지, mirror·HTTP view는 `false`)·`view`(`local`·`mirror`·`http`)·`refs`·`remote_scripts`(정적 감지)·`observed`(OS가 하위 리소스 차단을 알림)·`allowance`·`allowed`·`effective`(원격이 실제로 열려 있는지. 로컬 문서는 전역 또는 이 문서 허용, mirror는 늘 `false`, HTTP view는 전역 설정)·`marker`(`blocked`·`allowed`·`null`)·`banner`(단계와 표지). 조회는 배너를 띄우거나 사용자 열람으로 기록하지 않는다. html이 아닌 surface는 거절하며 헤드리스 빌드에서는 `-32017`이다 — [html plugin](../surfaces/html/index.md)
- html 스크립트 허용: `surface.html_script_allow {surface_id, url?, fingerprint?}` → `{surface_id, result, reload, document}` — 에이전트가 현재 문서의 스크립트를 배너 버튼과 같은 기록·재로드로 허용한다. `result`는 `allowed`·`already_allowed`이고 포커스·선택·활성 탭은 바꾸지 않는다. `url`·`fingerprint`(조회 응답 값)가 현재 문서와 다르면 거절한다. 거절은 `-32022`와 `error.data.reason`(`document_changed`·`loading`·`no_document`·`no_fingerprint`)이다. html이 아닌 surface는 invalid params, 헤드리스 빌드에서는 `-32017`이다. 권한 `surface.write`, 멱등 분류 Mutate — [html plugin](../surfaces/html/index.md)
- html 원격 콘텐츠 허용: `surface.html_remote_allow {surface_id, url?, fingerprint?}` → `{surface_id, result, reload, document}` — 에이전트가 현재 문서에 원격 콘텐츠를 원격 안내 버튼과 같은 기록·재로드로 허용한다. 허용은 그 문서(URL·지문)에만 묶이고 스크립트 허용과 따로 기록한다. `result`·`url`·`fingerprint`의 의미와 포커스·선택·활성 탭·안내 표지를 바꾸지 않는 점은 스크립트 허용과 같다. 거절은 `-32022`와 `error.data.reason`(`unsupported_view`·`globally_allowed`·`document_changed`·`loading`·`no_document`·`no_fingerprint`)이다. `unsupported_view`는 mirror·HTTP view라 이 허용이 차단에 영향을 주지 않는 경우, `globally_allowed`는 전역 설정이 이미 원격을 허용하는 경우다. html이 아닌 surface는 invalid params, 헤드리스 빌드에서는 `-32017`이다. 권한 `surface.write`, 멱등 분류 Mutate — [html plugin](../surfaces/html/index.md)
- html 문서 주소: `surface.html_nav {surface_id}` — html surface가 보여 주는 문서의 commit 주소(`url`, 리다이렉트 뒤 최종 주소와 `#fragment` 포함, 로드 중·실패 뒤에는 이전 주소), 호스트가 넣은 주소(`requested_url`), 탐색 상태(`nav_state`), 뒤로·앞으로 이동 가능 여부(`can_go_back`·`can_go_forward`)를 읽는다. html이 아닌 surface는 거절하며 헤드리스 빌드에서는 `-32017`이다 — [html plugin](../surfaces/html/index.md).
- html 히스토리 이동: `surface.html_back {surface_id}` · `surface.html_forward {surface_id}` · `surface.html_reload {surface_id}` → `{surface_id, queued}` — 에이전트용 뒤로·앞으로·새로고침. 대상 창의 큐에 넣고 다음 redraw에서 호스트 출처 로드로 실행하므로 스크립트 배너는 사용자가 그 surface를 선택할 때까지 보류된다. 포커스·선택은 바꾸지 않는다. 대상 확인을 통과하면 항상 `queued: true`다. 갈 곳이 없으면(히스토리 처음·끝, 문서 없는 새로고침) 적용할 때 판정해 아무것도 하지 않으므로 결과는 `surface.html_nav`로 확인한다. html이 아닌 surface·없는 surface·mirror workspace의 html leaf는 거절하고 원격으로 전달하지 않으며, 헤드리스 빌드에서는 `-32017`이다. 권한 `surface.write`, 멱등 분류 Mutate — [html plugin](../surfaces/html/index.md).
- 스크린샷: `ui.screenshot {path, surface_id?, window_id?}` (local-only, focus 독립 — 대상을 ID 로 지정) — [screenshot-methods](../ai-verification/screenshot-methods.md)

### 시스템 상태와 요청 압력

`system.info`는 시스템 정보를, `system.pressure`는 요청 압력을 반환한다. `system.pressure`는 local-only다.

응답은 `queue_before_gate` · `handler_after_gate` · `plugin_round_trip` · `db` · `connections` · `db_pragmas` · `stream_push` · `queue_admission` · `queue_dispatch` · `keyed_requests` · `slow_requests` · `gate_refusals` · `memory_writer` · `state_writer` · `state_db` · `file_writer` · `file_writes` · `file_snapshots` · `file_commands` · `file_operations` · `journal` 항목이며 각각 세는 대상이 다르다.

- **큐 대기(`queue_before_gate`)**: 권한 검사 전에 기록하므로 뒤에 거부될 요청도 센다. `waits`는 대기를 기록한 명령 수로 `wait_us_mean` 의 분모이며 `wait_us_hist.counts` 의 합과 같다. `commands` 는 회차가 끝날 때 오르므로 조회 순간에는 아직 도는 회차만큼 `waits` 보다 작다 — [IPC 서버](../architecture/ipc-server.md).

- **실행한 핸들러(`handler_after_gate`)**: 게이트를 통과해 실행된 요청만 센다.

- **plugin 응답(`plugin_round_trip`)**: host→plugin 요청 중 **응답이 실제로 매칭된 것만** 센다(취소·만료된 것은 끝점이 없어 못 잰다).

- **저장소(`db`)**: `MemoryStore` 의 트랜잭션 commit 과 WAL checkpoint 지연이고, commit 은 **성공한 것만** 센다(거부는 롤백이라 남긴 것이 없다) — checkpoint 는 다른 커넥션과 경합해 못 끝낸 횟수를 `checkpoints_busy` 로 따로 센다. `writes_per_commit`(`le` 구간 상한 · `counts`)은 commit 하나에 확정된 쓰기 수의 분포이고, VACUUM 은 `vacuums` · `vacuum_us_sum` 으로 commit 과 따로 센다([텔레메트리](../features/telemetry/index.md#db-지연)).

- **TCP 연결(`connections`)**: 이 포트에 붙은 TCP 연결 전부를 세고(요청을 안 보내는 attach·mesh 스트림 포함), 누계·순간값 중 `live` 만 내려가는 순간값이며(평균 `accept_wait_bound_us_mean` 은 파생값이라 내려갈 수 있다) `limit` 은 서버가 집행하는 동시 연결 상한, `refused_saturated` 는 그 상한에 걸려 **요청이 되기도 전에** 거절된 연결의 누계다(그래서 `ipc_calls` 에 안 남는다).
  이 항목의 유일한 시간 `accept_wait_bound_us_sum` · `_max` · `_mean`(기록 수 `accept_waits` = `accepted + refused_saturated`)은 연결이 OS 의 accept 큐에서 기다렸을 수 있는 시간의 **상한**이다 — accept 루프가 큐를 마지막으로 비어 있다고 본 뒤 지난 시간이고, 루프가 빈 큐에서 100 ms 자므로 호출마다 연결을 여는 client 에게는 그만큼까지가 되며 다른 어느 항목에도 안 잡힌다([IPC 서버](../architecture/ipc-server.md)).

- **저장 실행기(`memory_writer`)**: memory.db 전담 writer 의 누계다 — `admitted` · `rejected_saturated` · `rejected_closed`, 묶음 수 `batches` 와 요청 합 `batch_requests` · `batch_size_max` · 분포 `batch_sizes`(`le` · `counts`), `superseded` · `retries` · `flushes`, 지금 잡힌 `held_items` · `held_bytes` 와 최댓값 `peak_items` · `peak_bytes`. writer 가 없는 구성이면 `null` 이다([저장 실행기](../design/systems/storage-contract.md#저장-실행기)).
- **공통 writer 추가 관측**: `memory_writer`·`state_writer`·`file_writer`는 `admitted_bytes`·`batch_bytes`·`batch_bytes_max`, `batch_wait`·`batch_execution`·`executor_execution`(`samples`·`sum_us`·`max_us`·`le_us`·`counts`), `completed_ok`·`completed_rejected`·`completed_failed`·`completed_unknown`·`completed_lost`, `queued_items`·`queued_bytes`, `pending_items`·`pending_bytes`·`oldest_pending_age_us`를 같은 형식으로 싣는다. 마지막 나이는 미완료 요청이 없으면 null이고, 완료했지만 읽지 않은 결과는 pending에서 제외하되 held 예산에 남긴다. 시간 구간과 표본·재시도·대체·읽기·물리 commit의 차이는 [저장 실행기 관측](../design/systems/storage-contract.md#저장-실행기-관측)을 따른다.

- **Journal(`journal`)**: GUI·헤드리스가 공유하는 journal worker의 비소비 캐시다. worker 없는 시험 구성은 `null`, Store가 아직 열리지 않았으면 `store: null`이다. 접수·대기·실행 중 Work 단계 수, 입력 바이트 추정치, 고정 시간 분포, 실제 SQL physical/structural commit·논리 요청 수를 구분한다. `dirty_age_us`와 SQLite busy 재시도 수는 관측 불가라 `null`이며 `unpublished_age_us`는 durable 이후 ACK 대기 구간이다. 조회는 DB I/O·executor 잠금 없이 동작한다([단위·실패·분포 계약](../architecture/journal-metrics.md)).

- **state.db 저장 실행기(`state_writer` · `state_db`)**: `state_writer` 는 GUI 의 state.db writer(최근 파일·튜토리얼 진행) 누계로 `memory_writer` 와 같은 필드를 싣는다. `state_db` 는 그 writer 가 성공한 commit 의 `commits` · `commit_us_sum` · `commit_us_max` · `commit_us_mean` 과 `writes_per_commit`(`le` · `counts`)이다. 부팅의 첫 읽기는 세지 않는다. 헤드리스와 state.db 를 열지 못한 실행은 둘 다 `null` 이다([state.db 실행기](../design/systems/storage-contract.md#statedb-실행기)).

- **최근 목록 snapshot(`state_snapshots.recent_files`)**: state writer가 구성된 GUI에서 캐시 전체의 `dirty`·`dirty_age_us`를 읽는다. 첫 표시 변경부터 모든 종류의 최신 표시 revision이 확정될 때까지 연속된 마이크로초이며 새 변경·실패·결과 불명·접수 거절·오래된 성공으로 초기화하지 않는다. 대기 요청이 없어도 미저장 변경의 나이는 유지된다. 조회는 receipt를 수거하지 않는다. headless·state writer 미구성은 null이다([state.db 실행기](../design/systems/storage-contract.md#statedb-실행기)).

- **파일 실행기(`file_writer` · `file_writes` · `file_commands`)**: `file_writer`는 snapshot과 ordered command의 공용 writer 통계다. `file_writes`는 설정·snapshot backend의 replaced/failed/unknown이며 복합 명령을 한 번의 파일 교체로 세지 않는다. `file_commands.presets`는 accepted/rejected/completed/unknown/pending이고 completed에는 알려진 저장소 오류도 포함된다. 원 파일 명령의 완료와 물리 교체 건수는 다르다([파일 실행기](../design/systems/storage-contract.md#파일-실행기)).

- **물리 파일 작업(`file_operations`)**: 실제 file worker 범위에서 공용 helper를 호출한 replace/delete의 attempts/succeeded/failed/unknown, duration, 고정 오류 분류다. replace의 payload_bytes/published_bytes는 제출/성공 게시 바이트 합이며 삭제 바이트는 null이다. helper 전에 거절된 읽기·파싱 실패는 교체 시도가 아니다. executed_commands의 알려진 성공/실패/불명/partial은 backend 분류이며, 불투명한 반환값은 unclassified다. writer가 없으면 전체가 null이다. 부팅 load·CLI나 공용 helper 밖 작업은 포함하지 않는다([계측 경계](../design/systems/storage-contract.md#file-worker의-물리-파일-작업-계측)).

- **파일 snapshot(`file_snapshots`)**: GUI·headless의 settings/file_handlers/hook_handlers 및 GUI의 explorer_favorites/port_favorites마다 visible/committed/pending/dirty/dirty_age_us를 제공한다. settings는 SettingsSaves가 소유한 메모리 적용·저장 확정 상태다. 구성되지 않은 registry는 null이다. dirty는 visible > committed이며 age는 첫 미확정 변경부터 최신 표시 revision이 확정될 때까지 연속된 마이크로초다. 실패·결과 불명·새 변경·오래된 성공은 시간을 초기화하지 않고, 최신 성공 수거 뒤 null이 된다. pending=0인 저장 실패도 dirty가 유지된다. 조회는 receipt를 소비하지 않는다([저장 계약](../design/systems/storage-contract.md)).

- **저장소 초기 설정(`db_pragmas`)**: **누계가 아니라 열 때 한 번 되읽은 설정**이다 — `memory_db` · `state_db` 마다 `in_memory` · `degraded` 와 pragma 별 `requested` · `effective` · `took` · `error` 를 싣고(열리지 않은 DB 는 `null`, 헤드리스의 `state_db` 는 늘 `null`), `degraded` 는 오류가 아니라 열린 채로 쓰이는 상태다([저장 계약과 writer 명부](../design/systems/storage-contract.md)). `memory_db` 에는 `init_failure`(파일을 못 열어 in-memory 대체로 떴으면 `{cause, error}`, 아니면 `null` — 대체면 `degraded` 도 `true`, [저장 계약과 writer 명부](../design/systems/storage-contract.md))가 더 있다.

- **스트림 전송(`stream_push`)**: 서버가 attach·mesh 스트림으로 보낸 프레임을 센다. `frames_dropped`(연결이 살아 있는데 큐가 차서 버린 프레임 누계) · `clients_lagged_out`(연속 drop 한도로 끊은 연결 누계)는 안 내려가며 `backlog`(지금 살아 있는 연결들의 큐에 쌓여 아직 안 쓰인 프레임 수의 합)만 내려가고, `sink_capacity` 는 연결 하나의 큐 상한이다([Attach 메커니즘](../dev-guide/attach-behavior.md)).

- **명령 큐 입장(`queue_admission`)**: 지금 큐에 든 `queued_bytes` · `queued_commands` · `queued_injected`(내려간다) · `peak_bytes` · 상한에 걸려 **큐에 들어가지도 못한** 요청의 누계 `refused_bytes` · `refused_depth` · 집행하는 상한 `limit_bytes` · `limit_injected_depth` 를 싣고, IPC 서버가 없는 조립이면 `null` 이다([IPC 서버](../architecture/ipc-server.md)).

- **명령 실행(`queue_dispatch`)**: 큐에서 **꺼낸** 쪽이다 — `rounds` · `rounds_stopped_by_count` · `rounds_stopped_by_time` · `expired_before_run`(큐에서 기한이 지나 실행하지 않은 명령 — 소켓 요청은 `-32067` 로 답한 것이고, 호스트 주입 명령도 센다. 주입 명령은 `-32067` 로 답해지지 않고 호출자 스레드에서 `InjectError::Expired` 로 끝난다 — [IPC 서버](../architecture/ipc-server.md)) · `started` · `in_flight`(실행을 시작했고 명령 처리 또는 응답 대기가 끝나지 않은 요청, 내려간다) · `in_flight_max`([IPC 서버](../architecture/ipc-server.md)).

- **멱등 키 요청(`keyed_requests`)**: **멱등 키를 실은 요청만** 판정마다 센다 — `executed` · `replayed` · `conflicted` · `discarded` · `in_flight`(진행 중인 같은 요청에 합류, `queue_dispatch.in_flight` 와 다르다)([IPC 서버](../architecture/ipc-server.md)). 세 항목의 분할 근거는 [IPC 서버](../architecture/ipc-server.md).

- **느린 요청(`slow_requests`)**: 느린 요청을 개별 기록한다.
  큐 대기 · 호스트 처리 · plugin 대기의 합이 `threshold_us`(100 ms) 이상인 요청만 메모리 안 고정 용량 링(`capacity` 32)에 한 줄씩 싣는다.
  줄은 `request_seq`(호스트가 요청마다 매기는 프로세스 전역 번호 — JSON-RPC `id` 도 Event Bus `trace_id` 도 아니다) · `host`(`method` · `caller` · `queue_wait_us` · `host_us` — 게이트 포함이라 `handler_after_gate` 와 집계 대상이 다르다 · `outcome` = `ok`/`error` 와 `error_code` — 호출자가 실제로 받은 답이라 `-32067` · `-32001` · 정상이 갈리고, 아직 답이 안 나갔으면 둘 다 `null` 이다, [IPC 서버](../architecture/ipc-server.md)) · `plugin_hops`(hop 마다 `plugin_id` · `host_request_id`(= plugin 이 받은 JSON-RPC id) · `wait_us` · `outcome` = `ok`/`error`/`expired`/`cancelled`) · `total_us` 이고, 항목에 `admitted`(켜진 뒤 링에 든 누계) 가 함께 나간다.
  집계 대상은 호스트 IPC 큐를 지난 요청이고 `system.pressure` 자신은 안 넣는다.
  params · 토큰 · 멱등 키 · RPC `id` 는 싣지 않는다([IPC 서버](../architecture/ipc-server.md)).

- **요청 거부(`gate_refusals`)**: 진입 게이트의 판정 누계다 — `judged`(게이트에 든 요청 전부, Local 과 큐를 안 지나는 plugin host-call 도 센다) · `permission_denied`(`-32001` — 권한 부족만이 아니라 plugin · agent 가 부른 없는 메서드와 그들에게 열리지 않은 메서드도 든다.
  Local 호출은 없는 메서드여도 안 든다) · `cap_blocked`(`-32007`) · `throttled`(`-32010`).
  한 요청은 많아야 한 칸이고 재시작하면 0 이며, 봉투 검사(멱등 키 길이) 거절은 안 센다([IPC 서버](../architecture/ipc-server.md)).
  `queue_before_gate`와 `handler_after_gate`의 차이를 거부 수로 읽으면 안 된다 — 게이트를 통과하고도 app 층에서 답하고 돌아가는 갈래가 있다.
  거부된 수는 `gate_refusals` 에 센 값으로 있다.

관측이 없는 평균은 `null`이다. 시간 항목 셋에는 `*_hist` 가 붙어 같은 관측의 분포를 싣는다 — `bounds_us`(10 µs~1 s 의 반-십진 열한 칸, `le`)와 `counts`(한 칸 더 길고 **누적이 아니다** — 합이 관측 수, 마지막 칸은 상한을 넘은 것들). 분위수는 호스트가 계산하지 않는다. `db` · `connections` · `stream_push` 와 마지막 다섯 항목에는 분포가 없다.

CLI는 `tasty list pressure`다. 자세한 기준은 [telemetry](../features/telemetry/index.md)를 따른다.

`system.gpu_stats`는 local-only이며, GPU 리소스 수의 스냅샷을 반환한다. wgpu 전역 리포트, 창별 렌더러 카운트, 창별 explorer view 수(`explorer_views`)가 메모리 누수 검증에 쓰인다. CLI는 `tasty list gpu-stats`다. 검증 절차: [memory-leak-soak](../dev-guide/memory-leak-soak.md)
- 타이머 관측: `timer.list` (local-only, 조회 전용 — 등록된 주기 작업의 키/주기/다음 데드라인/precision 스냅샷 + 지금 인스턴스를 깨우고 있는 hard deadline 요약. 본체 허브 + plugin manager 허브 합산. CLI `tasty list timers`) — [timer-hub](../dev-guide/timer-hub.md)
- Plugin 설정 read-back: `settings.get_plugin_setting {storage_key}` — 자기 자신의 `plugin_settings` 값만 조회(caller 로 스코프 강제)
- 터미널 입력 규칙: `settings.get_input_rules {}` → `{rules}`; `settings.set_input_rule {app, shift_enter_newline}` / `settings.remove_input_rule {app}` → `{changed}`. 응답은 실행 시점의 설정에 적용하고 저장한 뒤에 오며, 적용·저장 실패는 오류다([완료 의미](../architecture/data-flows.md#에이전트plugin-설정-변경-요청의-완료-의미)). 세 메서드는 local-only이며 전역 설정이다. `settings.initialize_input_rule {app, shift_enter_newline}`는 `ui.settings_page` 권한으로 호출 가능하고 호출자/앱별 한 번만 빈 규칙을 채운다. 이미 있는 규칙과 이후 사용자 삭제를 보존한다. CLI는 `tasty settings get-input-rules`, `set-input-rule`, `remove-input-rule`, `initialize-input-rule` — [terminal](../surfaces/terminal/index.md).
- 원격 전송 저장 정책: `settings.get_remote_transfer` / `settings.set_remote_transfer {dir?, max_mb?}` (local-only, focus 독립 — 전역 설정) — 원격 bulk 파일 전송 수신측 저장 폴더(`dir`, 빈 값=기본 `~/.tasty/transfers/`)와 폴더 최대 용량(`max_mb`, MiB). set 은 지정 필드만 실행 시점의 설정에 적용하고 메모리 적용과 `config.toml` 저장이 끝난 뒤 응답한다. 응답 `remote_transfer`는 적용 뒤의 값이며, 저장 실패는 `-32603`(`data.saved: false`)이다([완료 의미](../architecture/data-flows.md#에이전트plugin-설정-변경-요청의-완료-의미)). CLI `tasty settings {get-remote-transfer,set-remote-transfer}`. — [remote-attach](../features/remote-attach/index.md)

### Plugin 관리 (`plugin.*`, local-only)
`list,show,install,remove,enable,disable,upgrade_builtins,permissions,grant,revoke` · `grant_agent_permission`/`revoke_agent_permission`/`list_agent_permissions` · `request_permission`(Agent caller 는 자기 자신만 대상, 다른 `agent_id` 는 `-32602`) · `audit_{query,summary,follow,clear}`(**deny 만 기록된다** — allow 는 저장하지 않으므로 평시 조회 결과는 비어 있는 것이 정상이다, [저장소 시스템 (Storage System)](../design/systems/storage.md)) · `extension.list`. [plugin-system](../features/plugin-system/index.md) · [capability-elevation](../features/capability-elevation/index.md).

### Lua 스크립트
release IPC 없음 — 등록한 스크립트를 단축키 또는 이벤트 트리거로 실행한다(docs/features/hooks/index.md). 임의 Lua 주입은 debug 빌드 전용 `debug.lua.eval`. [lua-hooks](../features/lua-hooks/index.md).

### Plugin 확장 네임스페이스
plugin 이 `[[contributes.ipc_namespace]]` 로 prefix 를 선언하면 `<prefix>.<method>` 가 그 plugin 으로 forward 된다(예: `claude.*`, `codex.*`). [plugins/](../plugins/index.md).

### Debug 전용 (debug 빌드만, `DEBUG_METHODS`)
`ui.state` · `debug.{info,cell_info,screen_attrs,glyph_color,feed_bytes,inject_mouse,inject_key,tool.*,popup.*,event_bus.*,extension.invoke_hook}` · `window.focus`/`view.focus` · `system.shutdown`. release 미노출 → [debug-ipc](../dev-guide/debug-ipc.md).

(`ui.screenshot` 은 focus-독립 정식 기능으로 승격 — 위 "기타 호스트" 참조. [screenshot-methods](../ai-verification/screenshot-methods.md).)

## CLI 매핑

CLI 는 위 IPC 를 감싼다(`tasty list workspaces`, `tasty send text`, `tasty memory put`, `tasty agent task-create`, `tasty approval request`, `tasty plugin list`, `tasty events follow`, `tasty screenshot --path …` …). debug 서브커맨드는 debug 빌드만. 환경별 접속/실행/종료 패턴은 [environments](environments.md).

## 관련

- [event-catalog](event-catalog.md) — IPC 와 별개 채널인 Event Bus
- [사용자 통제와 데이터](../principles/control-and-data.md#사용자에이전트원격-사용자) · [포커스 정책](../design/policies/focus.md) — 사용자/에이전트 행동 구분(이 표면의 설계 축)

<a id="system-and-window-observations"></a>

## 시스템과 창의 조회 범위

`system.info`의 `workspace_count`와 0부터 시작하는 `active_workspace`는 선택된 engine의
기존 필드다. `scope="engine"`, `workspace_ids`, `active_workspace_id`(없으면 null),
`layout_slot`으로 어떤 engine인지 구분한다. `version`은 프로세스 전체의 버전이다.
`instance_id`는 프로세스가 시작될 때 만드는 무작위 hex 32자로, 같은 프로세스에서는 같고 다른 프로세스와는 다르다.
SSH 너머의 attach 상대가 자기 자신인지 판정하는 데 쓴다. `system.info`에만 포함한다.
`workspace_id` 같은 기존 대상 키로 engine을 고르면 포커스를 바꾸지 않고 조회할 수 있다.
대상을 생략하면 기존 라우팅 기본값을 따른다.

`capabilities`는 이 서버가 협상할 수 있는 기능을 `{name, version}` 목록으로 제공한다.
같은 패키지 버전도 빌드 기능 조합이 다를 수 있어 버전 번호만으로 지원 기능을 판단하지 않는다.
이 필드를 모르는 client는 무시할 수 있다. 서버 기능이므로 `system.info`에만 포함하고
`window.list`에는 반복하지 않는다([CLI / IPC API 규약 — 명명 + 안정성](../dev-guide/api-conventions.md)).

`window.list`에는 각 main 창의 `id`, `focused`, `title`과 함께 같은 engine 조회 필드가
들어간다. parked engine은 OS 창 ID가 없어 목록에 포함하지 않는다. `workspace.list`는
parked engine까지 포함한 전체 workspace 목록이며 그 길이가 전체 개수다.
이 조회를 위해 포커스를 바꿀 필요는 없다.

헤드리스 빌드에는 창이 없어 `window.list`·`view.list`·`window.create`를 제공하지 않는다.
호출하면 빈 목록이 아니라 `-32017`(이 빌드 조합에 없는 메서드) 오류로 답하고 `result`는 싣지 않는다.
빈 목록은 "창이 0개인 GUI"로 읽혀 호출자가 창을 만들려 하기 때문이다. 헤드리스에서 engine을
조회할 때는 `system.info`를 쓴다(`scope="engine"`). 메서드별 분류는
[헤드리스 IPC 표면](../dev-guide/headless-ipc-surface.md)에 있다.
