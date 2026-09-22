# IPC / CLI API 레퍼런스

tasty 를 조작하는 전체 IPC/CLI 표면의 네임스페이스별 목록. **메서드·권한의 정답은 코드** — `crates/tasty-ipc/src/method_meta.rs`(`METHOD_TABLE`/`DEBUG_METHODS`)와 `src/adapters/ipc/handler.rs` 라우터다. 이 문서는 사람이 읽는 지도이며, 각 메서드의 *동작* 은 해당 feature 문서로 위임한다.

## 접속

```python
import socket, json, os
port = int(open(os.path.expanduser("~/.tasty/tasty.port")).read().strip())  # 동적 포트
s = socket.socket(); s.connect(("127.0.0.1", port))
def call(method, params=None):
    s.sendall((json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params or {}}) + "\n").encode())
    return json.loads(s.recv(1<<16).decode())
```

- 전송: loopback TCP, 동적 포트(`~/.tasty/tasty.port`), 줄단위 JSON-RPC 2.0. 근거 [ADR-0004](../adr/0004-ipc-transport-tcp.md).
- 대부분의 CLI 서브커맨드가 동명 IPC 를 감싼다. **모든 명령은 포커스 비의존** — 대상을 ID 로 직접 지정([focus 정책](../design/policies/focus.md)).
- **포커스 독립**: release 표면엔 포커스 변경 API 가 없다. 사용자 입력 재현(키/마우스 주입, popup 강제 open, `window.focus`)은 debug 전용 → [debug-ipc](../dev-guide/debug-ipc.md).

## 권한

plugin caller 는 메서드별 권한 토큰이 필요하다(`method_meta`). Local(CLI/사용자)은 무제한. 토큰 목록·게이트는 [concepts/plugins](../concepts/plugins.md) · [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md). 런타임 권한 상승(capability elevation)·audit 은 [features/capability-elevation](../features/capability-elevation/index.md).

## 네임스페이스

### 구조 — workspace / pane / tab / surface / split / tree
`workspace.{list,create,update,move}` · `pane.{list,close}` · `split` · `tab.{list,create,close,move}` · `tree`. 도메인은 [work-area](../features/work-area/index.md).

### Surface 상호작용
- 입력: `surface.{send,send_key,send_combo,send_to,send_wait_idle,wake,respawn_terminal}`
- 읽기/마크: `surface.{set_mark,read_since_mark,parse_since_mark,read_since_scan_mark,screen_text,cursor_position,foreground_process,is_typing,locate}`. `screen_text`(및 `pty.read`)는 dim(ghost-suggestion, 예: Claude Code 자동완성 제안) 셀을 기본 제외 — `show_dim:true`(CLI `--show-dim`)로 포함.
  - `read_since_mark` 은 **위치를 받을 수 있다.** `cursor`(절대 바이트 위치) + `stream`(스트림 표지)을 주면 마크 대신 그 위치부터 읽고, **서버는 그 소비자를 위해 아무것도 들지 않는다** — 그래서 소비자가 몇이든 서로의 창을 안 민다. 안 주면 예전 그대로 `set_mark` 이 세운 마크에서 읽는다. `max_bytes` 로 한 번에 읽을 **원문** 바이트를 줄일 수 있고(상한은 보존 크기 1 MiB), 생략하면 보존 전체다. **하한은 1 이다 — `0` 은 한도 없음이 아니라 1 바이트로 올린다**(0 이면 기다리는 바이트가 있어도 매번 빈 답이라 소비자가 못 나아간다). 음수·정수 아닌 값은 인자 오류로 거절한다.
    - 응답은 두 형태 모두 `stream` · `cursor`(실제로 읽기 시작한 위치) · `next_cursor` · `raw_bytes` · `retention_start` · `retention_end` · `skipped` 를 함께 싣는다. **`skipped` 가 0 이 아니면 그만큼이 보존 밖으로 밀려나 영영 사라졌다** — 마크가 잘려 나가도 이제는 조용히 처음부터 주지 않는다.
    - **전진은 `next_cursor` 가 정한다.** `text` 의 길이는 읽은 원문 구간의 길이가 아니다(손실 디코딩이 U+FFFD 를 넣고 `strip_ansi` 가 바이트를 뺀다). 그 길이로 이어 읽으면 스트림과 어긋난다.
    - `cursor` 에는 `stream` 이 **반드시** 따라야 한다 — surface id 는 재사용되고 `respawn_terminal` 은 id 를 그대로 둔 채 터미널을 바꾸므로, 표지가 없으면 옛 위치가 남의 출력에 조용히 적용된다. 표지가 안 맞는 위치 · 스트림 끝을 넘은 위치 · 터미널이 없는 surface 는 **빈 답이 아니라 거절**이고, `error.data.reason` 이 `stream_mismatch` · `cursor_ahead_of_stream` · `cursor_without_stream` · `no_terminal` 로 갈린다 — [ADR-0341](../adr/0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md).
    - CLI 는 `tasty read since-mark --cursor N --stream S [--max-bytes N]` 이다. 구 서버는 이 인자를 **조용히 버리고** 공유 마크에서 읽으므로, 서버는 capability `ipc.output-cursor` 로 이 계약을 선언하고 CLI 는 위치 인자를 실은 요청을 보내기 전에 그 이름을 묻는다 — 없으면 보내지 않고 구조화된 거절(`sent:false`)로 끝난다([ADR-0365](../adr/0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md)).
  - `read_since_scan_mark` 는 **다른 커서**를 쓴다 — 주기적으로 출력을 훑는 소비자(번들 claude plugin 의 에러 스캐너)용이고, 읽으면 커서가 그만큼 전진하므로 같은 구간이 두 번 오지 않는다. `set_mark` 이 그 커서를 밀지 않고 그 반대도 아니다. 소비자가 하나라는 전제 위에 있어 CLI 동사가 없다 — [ADR-0307](../adr/0307-the-output-scanner-reads-its-own-cursor.md).
  - `lines:N`(CLI `--lines N`) 생략 시 보이는 화면 전체. 지정하면 **내용 기준 마지막 N 줄** — 내용 아래의 공백 행은 건너뛰고, 화면 내용이 N 에 모자라면 스크롤백에서 채운다(합쳐도 모자라면 있는 만큼). 내용 *중간* 의 빈 줄은 출력의 일부이므로 보존하고 줄 수에도 포함한다. 빈 행 판정은 `show_dim` 과 같은 값으로 하므로 `--show-dim` 유무가 반환 줄 수를 바꾸지 않는다. alternate screen(TUI)이 떠 있을 때도 부족분은 primary 스크롤백으로 채운다 — alt screen 은 자체 스크롤백이 없기 때문이다.
  - **N 보다 적게 왔을 때 왜인지 물을 수 있다.** 응답에 `is_terminal`(그 surface 뒤에 터미널이 있는가) · `scrollback_len`(현재 스크롤백 줄 수) · `alt_screen`(대체 화면인가)이 함께 실린다. `scrollback_len: 0` 이면 **받은 것이 가진 전부**이고(정상 포화), 0 이 아닌데 N 보다 적게 왔다면 그건 결함이다. 터미널이 아닌 surface(markdown/html/explorer/image)나 없는 surface 는 `is_terminal: false` 와 두 필드 `null` 로 나온다 — **`0` 이 아니다**(0 은 "스크롤백이 비었다" 라는 다른 사실이다). 같은 필드가 `pty.read` 응답에도 실린다.
- 명령(OSC 133): `surface.{commands,last_command,command_at}`
- 메타: `surface.meta.{set,get,unset,list}` · `surface.set_cwd`
- 주의 환기(attention): `surface.completion`(발동) · `surface.attention.{get,clear}`(조회·해제). 해제는 `kind` 선택 필터를 받고, 하드 점유 중인 surface 와 mirror surface 는 거절한다(그 상태의 소유자가 다른 인스턴스다) — [surface-highlight](../features/surface-highlight/index.md)
- 출력 옵저버: `output.observe_{start,stop,list,info}`
- 동작·파서는 [terminal-output](../features/terminal-output/index.md), 파서 카탈로그 [output-parsers](output-parsers.md). (IME `surface.ime_*` 는 debug 빌드 전용 local-only — release 표면에 없다.)

### 메모리 (`memory.*` / `memory.secret.*`)
regular(`put/get/delete/list/exists/count/scopes/stats/query/export/import`) · secret(동일 verb) · `gc` · blackboard(`bb_*`) · plan(`plan_*`) · cache(`cache_*`) · goal(`goal_*` — surface 스코프 단일 목표 문장, `surface_id` 명시 필수). 모델·권한은 [design/systems/memory](../design/systems/memory.md). 저장소 자체가 실패하면 `-32603 memory db error: …` 이고 `error.data.storage_failure` 가 원인을 싣는다(`busy` · `disk_full` · `io` · `corrupt` · `permission_denied` · `other`) — 실패한 쓰기는 quota 도 변경 알림도 남기지 않는다([storage](../design/systems/storage.md) "저장 실패의 의미"). 부팅이 `memory.db` 를 못 열어 in-memory 대체로 떴으면 쓰기 계열의 성공 응답에 `durable: false` 가 더해진다(`ok` 는 그대로, 정상 저장소에서는 칸 없음 — [ADR-0485](../adr/0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md)). 같은 저장소에 쓰는 `agent.*` · `approval.*` · `surface.meta.*` · `telemetry.*` · `session.*` 의 쓰기 응답도 같다([ADR-0518](../adr/0518-every-ipc-write-to-memory-db-says-when-it-is-not-durable.md)).

### 에이전트 협업 (`agent.*`)
`task_{create,list,get,cancel,retry,graph,reduce,run,delete,purge}` · `dag_{list,get}` · `barrier_*` · `semaphore_*` · `lease_*` · `rate_limit_*`. 전부 `agent`(AgentManage) 권한 — `task_run`(workspace runner thread start/stop/status)도 포함(호스트가 재시작 시 runner 를 자동으로 다시 켜지 않으므로, plugin 이 자기 workspace 의 runner 를 스스로 되살릴 수 있어야 한다). **local caller 전용**(plugin 호출 거부)은 `task_await`(진짜 blocking — `approval.await` 와 대칭, plugin SDK 단일 워커 스레드가 막히는 걸 막기 위함. 기본 timeout 10분, `timeout_ms:0` 은 무한 대기)와 `task_set_result`(외부 task 완료 신호 — 러너가 Custom task 생명주기를 단독 소유하므로 plugin 이 별도로 전이시키면 쓰기 주체가 이중화된다. plugin 은 완료 판정 전략 선언으로 우회). 둘 다 [method_meta.rs](../../crates/tasty-ipc/src/method_meta.rs)의 `METHOD_TABLE`에 `local_only()` 로 **명시 등재**돼 있다 — 미등재(`UnknownMethod` 거부)는 정책과 누락이 구분되지 않으므로, 라우터 분기가 있는 모든 메서드는 표에 등재한다(`tests/ipc_router_table_parity.rs` 가 강제). `task_delete`/`task_purge` 는 참조(`depends_on`/`Fallback.task`/`Reduce.inputs`) 안전 검사를 거친다 — 기본 거부+참조자 목록, `--cascade`(연쇄 삭제)/`--force`(참조 검사만 우회, `running` 상태 제약은 못 뚫음). `task_command.kind = "run"`(Surface 없는 bare subprocess)의 결과는 `task_get`/`task_await` 의 `result.output` 에 stdout/stderr 캡처(각 마지막 64KiB tail + `truncated`/`dropped_bytes`)를 싣는다 — 실패(비0 exit)는 `result.error` 문자열에 같은 내용이 포함된다. `semaphore_set_permits` 는 세마포어 한도를 제자리에서 바꾼다(delete→create 우회의 "세마포어가 없는 순간" 을 없앤다) — 축소는 drain 이라 기존 홀더를 강제 회수하지 않고 새 acquire 만 거절한다. `semaphore_acquire` 의 `ttl_ms` 는 선택이며, 준 경우에만 그 홀더가 만료돼 회수된다(기본은 만료 없음, lease 와 같은 메커니즘 — [ADR-0119](../adr/0119-agent-semaphore-resize-and-holder-expiry.md)). `dag_{list,get}` 은 workspace 안의 flat 한 task 를 **DAG 단위로 쪼갠 조회 표면**이다 — DAG 는 영속 레코드가 아니라 `metadata.dag`(explicit) 또는 그래프 연결성(derived)에서 도출된다. `dag_list` 는 `workspace_id` 를 생략하면 살아있는 전 workspace 를 순회하고(응답 `scope: "live_workspaces"`), `dag_get` 은 그 DAG 부분집합만으로 `task_graph` 와 동일한 `nodes`/`edges`(또는 dot)를 낸다. [agent-collaboration](../features/agent-collaboration/index.md).

### 사건 피드 (`events.*`)
`events.fetch {offset, max, filter, wait_ms}` (local-only) — Event Bus 에 지나간 사건을 **위치로** 읽는다. **서버는 소비자별 상태를 들지 않는다** — 커서는 소비자가 들고 매 호출에 가져온다(같은 모양의 선례가 `plugin.audit_follow`). 응답은 `{events, next_offset, epoch, truncated, skipped, ahead_of_stream, stream_end}` 이고 각 사건 봉투에 자기 `offset` 이 실려 있다. `filter` 는 구독과 같은 문법이다(정확 일치 또는 `<ns>.*`) — 새 문법을 만들지 않았다. `wait_ms` 를 주면 그 시간까지 새 사건을 기다렸다 답한다(상한 60초, 기본 0 = 즉답). 링은 메모리에만 있고 용량(1024 건 · 16 MiB 중 먼저 닿는 쪽, [ADR-0456](../adr/0456-the-event-ring-evicts-by-count-or-bytes-whichever-comes-first.md))을 넘긴 사건은 밀려난다 — 보존 밖 위치로 물으면 조용히 처음부터 주지 않고 `truncated: true` 와 건너뛴 수 `skipped` 를 함께 준다. `epoch` 은 host 인스턴스 세대라 재시작 뒤 옛 위치를 들고 오면 값이 달라져 있다. 링의 끝(`stream_end`)보다 **뒤**인 위치로 물으면 — 재시작 전 세대의 위치가 흔한 원인이다 — `ahead_of_stream: true` 를 싣는다. 이때 나머지 필드는 표지가 없던 때와 같고(`next_offset` 은 요청 위치 그대로), `wait_ms` 를 줬으면 예전처럼 기다린 뒤 답한다([ADR-0405](../adr/0405-a-position-past-the-end-of-the-feed-is-marked-not-waited-on-silently.md)). CLI 는 `tasty events fetch` / `tasty events follow` — `follow` 는 재부착 때 `--epoch` 을 받아 세대를 가르고, 끊기면 다시 붙을 인자를 찍거나 `--reconnect` 로 다시 붙는다([ADR-0407](../adr/0407-events-follow-carries-the-generation-across-a-reattach.md)). 키 목록과 등급은 [event-catalog](event-catalog.md).

### 휴먼 핸드오프 (`approval.*`)
`request,respond,await,cancel,get,list,history,summary.{set,get}`. [human-handoff](../features/human-handoff/index.md).

### 텔레메트리 (`telemetry.*`)
`record,record_batch,summary,timeseries,top` · `cap.{set,list,remove,status,reset}` · `anomaly.list` · `session_summary`. [telemetry](../features/telemetry/index.md).

### 세션 / attach
`session.{issue,revoke,list}` · `attach.{acquire,release,force_detach,force_detach_workspace,into_gui,list}`. attach 는 [remote-attach](../features/remote-attach/index.md), 신원 토큰은 [capability-elevation](../features/capability-elevation/index.md).

### 기타 호스트
- 알림: `notification.{list,create}` — [notifications](../features/notifications/index.md)
- 훅: `hook.{set,list,unset}` · `global_hook.{set,list,unset}` · `surface.fire_hook`
- 웹훅(인바운드 HTTP): `webhook.{register,list,info,unregister,sweep,config}` (local-only) — [webhook](../features/webhook/index.md)
- 메시지 패싱: `message.{send,read,count,clear}`
- 파일 핸들러: `file_handler.{reload,detectors,dispatch}` — `detectors` 는 finalize 된 detector 와 출처별 contribution 조회(local-only, 읽기 전용) — [file-handler](../features/file-handler/index.md)
- 훅 핸들러: `hook_handler.{list,get,upsert,remove,reload,dispatch}` (local-only) — 훅/웹훅 공유 핸들러 레지스트리 조회(비활성 포함)·단건 조회(action 본문 포함)·user 핸들러 제자리 수정/제거·user config(`~/.tasty/hook-handlers.toml`) 재로드·id 로 수동 발화(IpcSequence/ShellCommand). `get` 의 `action` 과 `upsert` 의 `action` 은 같은 모양이라 왕복한다. upsert/remove 는 성공 시 user config 를 즉시 atomic write 하고, 쓰기에 실패하면 성공으로 보고하지 않는다. dispatch 는 fire-and-forget(응답은 accepted ACK 만) — [hooks](../features/hooks/index.md)
- 완료 판정 전략: `completion_strategy.list` (local-only) — `agent.task_create` 의 `Custom.poll` 이름 참조가 가리키는 완료 판정 전략 레지스트리 조회(비활성 포함). reload/dispatch 대응물 없음(판정 함수이지 발화 대상 아님) — [agent-runner](../dev-guide/agent-runner.md)
- 이미지: `image.{open,save,export_png,next,prev,paste,list}` — [image plugin](../plugins/image/index.md)
- 원격 연결 프로필: `remote.profile.{list,get,add,detect,remove,list_local,import}`(`list_local`=로컬 `~/.ssh/config` alias 열거·읽기 전용, `import`=그 alias 를 ssh 프로필로 등록·셸 감지 없음)(구 `tool.ssh.*`/`ssh.profile.*`는 alias로 한시 호환) — [remote-profiles](../features/remote-profiles/index.md)
- webview: `webview.set_url`
- 스크린샷: `ui.screenshot {path, surface_id?, window_id?}` (local-only, focus 독립 — 대상을 ID 로 지정) — [screenshot-methods](../ai-verification/screenshot-methods.md)
- 시스템: `system.info` · `system.pressure` (local-only, 요청 압력 게이지 — 프로세스 수명 누계. 응답이 `queue_before_gate` · `handler_after_gate` · `plugin_round_trip` · `db` · `connections` · `db_pragmas` · `stream_push` · `queue_admission` · `queue_dispatch` · `keyed_requests` · `slow_requests` 열한 덩어리이고 **모수가 열하나 다 다르다**: 첫째는 권한 게이트보다 먼저 재므로 뒤에 거부될 요청도 세고(그 안의 `waits` 는 대기를 기록한 명령 수로 `wait_us_mean` 의 분모이며 `wait_us_hist.counts` 의 합과 같다. `commands` 는 회차가 끝날 때 오르므로 조회 순간에는 아직 도는 회차만큼 `waits` 보다 작다 — [ADR-0466](../adr/0466-the-queue-wait-mean-divides-by-the-waits-it-summed.md)), 둘째는 게이트를 통과해 실행된 것만 세며, 셋째는 host→plugin 요청 중 **응답이 실제로 매칭된 것만** 센다(취소·만료된 것은 끝점이 없어 못 잰다). 넷째는 `MemoryStore` 의 트랜잭션 commit 과 WAL checkpoint 지연이고, commit 은 **성공한 것만** 센다(거부는 롤백이라 남긴 것이 없다) — checkpoint 는 다른 커넥션과 경합해 못 끝낸 횟수를 `checkpoints_busy` 로 따로 센다. 다섯째는 **시간이 아니라 자리**다 — 이 포트에 붙은 TCP 연결 전부를 세고(요청을 안 보내는 attach·mesh 스트림 포함), 누계·순간값 중 `live` 만 내려가는 순간값이며(평균 `accept_wait_bound_us_mean` 은 파생값이라 내려갈 수 있다) `limit` 은 서버가 집행하는 동시 연결 상한, `refused_saturated` 는 그 상한에 걸려 **요청이 되기도 전에** 거절된 연결의 누계다(그래서 `ipc_calls` 에 안 남는다). 이 덩어리의 유일한 시간 `accept_wait_bound_us_sum` · `_max` · `_mean`(기록 수 `accept_waits` = `accepted + refused_saturated`)은 연결이 OS 의 accept 큐에서 기다렸을 수 있는 시간의 **상한**이다 — accept 루프가 큐를 마지막으로 비어 있다고 본 뒤 지난 시간이고, 루프가 빈 큐에서 100 ms 자므로 호출마다 연결을 여는 client 에게는 그만큼까지가 되며 다른 어느 덩어리에도 안 잡힌다([ADR-0467](../adr/0467-the-accept-wait-is-reported-as-a-bound-in-the-connection-block.md)). 여섯째 `db_pragmas` 는 **누계가 아니라 열 때 한 번 되읽은 설정**이다 — `memory_db` · `state_db` 마다 `in_memory` · `degraded` 와 pragma 별 `requested` · `effective` · `took` · `error` 를 싣고(열리지 않은 DB 는 `null`, 헤드리스의 `state_db` 는 늘 `null`), `degraded` 는 오류가 아니라 열린 채로 쓰이는 상태다([ADR-0376](../adr/0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md)). `memory_db` 에는 `init_failure`(파일을 못 열어 in-memory 대체로 떴으면 `{cause, error}`, 아니면 `null` — 대체면 `degraded` 도 `true`, [ADR-0485](../adr/0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md))가 더 있다. 일곱째 `stream_push` 는 **요청이 아니라 밀어내기**다 — 서버가 attach·mesh 스트림 연결로 민 프레임을 세고, `frames_dropped`(연결이 살아 있는데 큐가 차서 버린 프레임 누계) · `clients_lagged_out`(연속 drop 한도로 끊은 연결 누계)는 안 내려가며 `backlog`(지금 살아 있는 연결들의 큐에 쌓여 아직 안 쓰인 프레임 수의 합)만 내려가고, `sink_capacity` 는 연결 하나의 큐 상한이다([ADR-0400](../adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)). 여덟째 `queue_admission` 은 명령 큐 **입장 장부**다 — 지금 큐에 든 `queued_bytes` · `queued_commands` · `queued_injected`(내려간다) · `peak_bytes` · 상한에 걸려 **큐에 들어가지도 못한** 요청의 누계 `refused_bytes` · `refused_depth` · 장부가 집행하는 상한 `limit_bytes` · `limit_injected_depth` 를 싣고, IPC 서버가 없는 조립이면 `null` 이다([ADR-0391](../adr/0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md)). 아홉째 `queue_dispatch` 는 큐에서 **꺼낸** 쪽이다 — `rounds` · `rounds_stopped_by_count` · `rounds_stopped_by_time` · `expired_before_run`(큐에서 기한이 지나 실행하지 않은 명령 — 소켓 요청은 `-32067` 로 답한 것이고, 호스트 주입 명령도 센다. 주입 명령은 `-32067` 로 답해지지 않고 호출자 스레드에서 `InjectError::Expired` 로 끝난다 — [ADR-0451](../adr/0451-a-host-injection-carries-its-wait-as-a-deadline.md)) · `started` · `in_flight`(실행 중이고 호출자가 아직 기다리는 요청, 내려간다) · `in_flight_max`([ADR-0412](../adr/0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md)). 열째 `keyed_requests` 는 **멱등 키를 실은 요청만** 판정마다 센다 — `executed` · `replayed` · `conflicted` · `discarded` · `in_flight`(진행 중인 같은 요청에 합류, `queue_dispatch.in_flight` 와 다르다)([ADR-0422](../adr/0422-the-retry-counts-are-kept-by-the-store-that-decides.md)). 세 덩어리의 분할 근거는 [ADR-0435](../adr/0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md). 열한째 `slow_requests` 는 **집계가 아니라 느린 요청 한 건씩**이다 — 큐 대기 · 호스트 처리 · plugin 대기의 합이 `threshold_us`(100 ms) 이상인 요청만 메모리 안 고정 용량 링(`capacity` 32)에 한 줄씩 싣는다. 줄은 `request_seq`(호스트가 요청마다 매기는 프로세스 전역 번호 — JSON-RPC `id` 도 Event Bus `trace_id` 도 아니다) · `host`(`method` · `caller` · `queue_wait_us` · `host_us` — 게이트 포함이라 `handler_after_gate` 와 모수가 다르다 · `outcome` = `ok`/`error` 와 `error_code` — 호출자가 실제로 받은 답이라 `-32067` · `-32001` · 정상이 갈리고, 아직 답이 안 나갔으면 둘 다 `null` 이다, [ADR-0468](../adr/0468-the-slow-request-host-part-carries-the-answer-the-caller-got.md)) · `plugin_hops`(hop 마다 `plugin_id` · `host_request_id`(= plugin 이 받은 JSON-RPC id) · `wait_us` · `outcome` = `ok`/`error`/`expired`/`cancelled`) · `total_us` 이고, 덩어리에 `admitted`(켜진 뒤 링에 든 누계) 가 함께 나간다. 모수는 호스트 IPC 큐를 지난 요청이고 `system.pressure` 자신은 안 넣는다. params · 토큰 · 멱등 키 · RPC `id` 는 싣지 않는다([ADR-0436](../adr/0436-an-ipc-request-is-numbered-by-the-host-and-slow-ones-are-kept-in-a-ring.md)). 앞의 둘을 빼서 "거부된 수" 로 읽으면 안 된다 — 게이트를 통과하고도 app 층에서 답하고 돌아가는 갈래가 있다. 관측이 없는 평균은 `null` 이다. 시간 덩어리 셋에는 `*_hist` 가 붙어 같은 관측의 분포를 싣는다 — `bounds_us`(10 µs~1 s 의 반-십진 열한 칸, `le`)와 `counts`(한 칸 더 길고 **누적이 아니다** — 합이 관측 수, 마지막 칸은 상한을 넘은 것들). 분위수는 호스트가 계산하지 않는다. `db` · `connections` · `stream_push` 와 마지막 네 덩어리에는 분포가 없다. CLI `tasty list pressure`) — [telemetry](../features/telemetry/index.md) · `system.gpu_stats` (local-only, GPU 리소스 카운트 스냅샷 — wgpu 전역 리포트 + 창별 렌더러 카운트 + 창별 explorer view 수(`explorer_views`), 메모리 누수 soak 검증용. CLI `tasty list gpu-stats`) — [memory-leak-soak](../dev-guide/memory-leak-soak.md)
- 타이머 관측: `timer.list` (local-only, 조회 전용 — 등록된 주기 작업의 키/주기/다음 데드라인/precision 스냅샷 + 지금 인스턴스를 깨우고 있는 hard deadline 요약. 본체 허브 + plugin manager 허브 합산. CLI `tasty list timers`) — [timer-hub](../dev-guide/timer-hub.md)
- Plugin 설정 read-back: `settings.get_plugin_setting {storage_key}` — 자기 자신의 `plugin_settings` 값만 조회(caller 로 스코프 강제)
- 원격 전송 저장 정책: `settings.get_remote_transfer` / `settings.set_remote_transfer {dir?, max_mb?}` (local-only, focus 독립 — 전역 설정) — 원격 bulk 파일 전송 수신측 저장 폴더(`dir`, 빈 값=기본 `~/.tasty/transfers/`)와 폴더 최대 용량(`max_mb`, MiB). set 은 지정 필드만 현재 설정 위에 덮어써 저장한다. CLI `tasty settings {get-remote-transfer,set-remote-transfer}`. — [remote-attach](../features/remote-attach/index.md)

### Plugin 관리 (`plugin.*`, local-only)
`list,show,install,remove,enable,disable,upgrade_builtins,permissions,grant,revoke` · `grant_agent_permission`/`revoke_agent_permission`/`list_agent_permissions` · `request_permission` · `audit_{query,summary,follow,clear}`(**deny 만 기록된다** — allow 는 저장하지 않으므로 평시 조회 결과는 비어 있는 것이 정상이다, [ADR-0085](../adr/0085-ipc-log-retention-bounded.md)) · `extension.list`. [plugin-system](../features/plugin-system/index.md) · [capability-elevation](../features/capability-elevation/index.md).

### Lua 스크립트
release IPC 없음 — 스크립트는 등록 목록 + 단축키 트리거로만 실행된다(ADR-0031). 임의 Lua 주입은 debug 빌드 전용 `debug.lua.eval`. [lua-hooks](../features/lua-hooks/index.md).

### Plugin 확장 네임스페이스
plugin 이 `[[contributes.ipc_namespace]]` 로 prefix 를 선언하면 `<prefix>.<method>` 가 그 plugin 으로 forward 된다(예: `claude.*`, `codex.*`). [plugins/](../plugins/index.md).

### Debug 전용 (debug 빌드만, `DEBUG_METHODS`)
`ui.state` · `debug.{info,cell_info,screen_attrs,glyph_color,feed_bytes,inject_mouse,inject_key,tool.*,popup.*,event_bus.*,extension.invoke_hook}` · `window.focus`/`view.focus` · `system.shutdown`. release 미노출 → [debug-ipc](../dev-guide/debug-ipc.md).

(`ui.screenshot` 은 focus-독립 정식 기능으로 승격 — 위 "기타 호스트" 참조. [screenshot-methods](../ai-verification/screenshot-methods.md).)

## CLI 매핑

CLI 는 위 IPC 를 감싼다(`tasty list workspaces`, `tasty send text`, `tasty memory put`, `tasty agent task-create`, `tasty approval request`, `tasty plugin list`, `tasty events follow`, `tasty screenshot --path …` …). debug 서브커맨드는 debug 빌드만. 환경별 접속/실행/종료 패턴은 [environments](environments.md).

## 관련

- [event-catalog](event-catalog.md) — IPC 와 별개 채널인 Event Bus
- [identity](../identity.md) — 사용자/에이전트 행동 분리(이 표면의 설계 축)

## System and window observations

`system.info` keeps `workspace_count` and the zero-based `active_workspace` as the
selected engine's legacy fields. `scope="engine"`, `workspace_ids`,
`active_workspace_id` (nullable), and `layout_slot` identify that engine. `version`
is process-wide. Existing resource target keys such as `workspace_id` select an
engine without changing focus; omitted targets keep the existing routing fallback.

`capabilities` lists what this server can negotiate, as `{name, version}` entries.
It answers a different question than `version` does: the package version is not a
feature list, because two builds of the same version differ by feature combination.
A client that does not know the key ignores it. The list is attached to the
`system.info` reply only — `window.list` does not repeat it, since capabilities
describe the server rather than a window. Rationale:
[ADR-0312](../adr/0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md).

`window.list` includes the same observation fields beside each live main window's
`id`, `focused`, and `title`. Parked engines have no OS window ID and remain outside
that list. `workspace.list` is the existing global workspace inventory, including
parked engines; its length is the global count. No focus-changing API is needed.
