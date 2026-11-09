# child-terminal (호스트 내재화 자식 터미널 관리 · `tasty terminal`)

- **Status**: Implemented
- **주체**: AI Agent
- **설계**: [Attach 메커니즘](../../dev-guide/attach-behavior.md) (soft 점유 소비자)
- **코드**: `src/runtime/child_terminal.rs` (registry) · `src/adapters/ipc/handler/terminal.rs` (IPC) · `crates/tasty-cli/src/commands/terminal.rs` (CLI)
- **화면**: 없음 — headless 전용 (자식은 일반 terminal surface 로 렌더; soft 점유 테두리는 [surface-highlight](../surface-highlight/index.md)/점유 계약)

## 목적

에이전트가 자식 터미널을 만들고 메시지를 보내고 종료할 수 있도록 호스트가 공통 기능을
제공한다. 자식 목록과 점유 관리는 호스트가 맡고, Codex·Claude 명령 구성은 각 플러그인이 맡는다.

## 내부 동작 (headless-valid)

### 자식 목록과 상태 기록

`ChildTerminalRegistry`는 부모별 자식 목록, 자식의 부모, 부모별 다음 index, 자식의
idle/needs_input 상태를 보관한다. 마지막 상태 보고 시각 `last_state_report_at`도 저장해
훅이 얼마나 오래 오지 않았는지 판단한다. 등록할 때 초기화하고 `terminal.set_state`마다
갱신하며, 재시작 후에도 해석하도록 Unix epoch 밀리초를 쓴다.

목록은 `~/.tasty/child-terminals.json`에 영속한다. 등록·제거 시 즉시 저장하며, 이전 파일에
보고 시각이 없으면 `serde(default)`로 빈 맵을 사용한다. 이 목록은 session token 추적
(`session.rs`)이나 셸 서브프로세스 추적(`runner_host`의 `shell_children`)과 별개다.
호스트는 접근할 때마다 실제 surface 목록과 대조해 사라진 자식을 제거한다. 재시작 후 첫
접근에서도 이전 세션의 잔재를 정리한다.

윈도우마다 engine 이 이 파일을 따로 읽는다. 항목마다 그 surface 가 속한 레이아웃 슬롯을 함께
저장한다(`slot_of`). surface 는 한 engine 에만 있으므로 engine 은 자기 슬롯의 항목과, 자기가
등록·변경·해제했거나 살아 있는 것을 본 surface 의 항목만 소유한다. 슬롯이 없는 headless engine 은
하나뿐이므로 읽은 항목을 모두 소유한다.

- **저장**: 파일을 다시 읽어 소유한 surface 의 항목은 메모리 값으로, 나머지는 파일 값으로 합친 뒤
  쓴다. 메모리도 합친 결과로 바꾼다. 그래서 한 윈도우의 저장이 다른 윈도우가 등록한 관계를 지우지 않는다.
- **정리**: 이 engine 이 소유했는데 지금 이 engine 에 없는 surface 만 지운다. 다른 윈도우의 surface 나
  재시작 뒤 아직 열지 않은 윈도우의 surface 는 이 engine 에 없어도 지우지 않는다. 그런 항목은 그 윈도우가
  다시 열려 정리하거나 surface 가 닫힐 때 지운다.
- **시작할 때**: `restore_layout = false` 이면 첫 engine 을 만들기 전에 파일의 관계를 모두 버린다(이전
  실행의 surface 는 어느 윈도우에도 돌아오지 않는다). 켜져 있으면 시작 복원(journal bootstrap)이 슬롯을
  정한 뒤, 복원한 engine 을 그 슬롯에 다시 묶고 다시 열 수 있는 슬롯(journal 이 아는 은퇴하지 않은
  슬롯)과 열린 슬롯 밖의 항목을 지운다. 슬롯 표시가 없는 이전 형식의 항목은 첫 복원 윈도우의 슬롯에
  속한 것으로 본다.
- **윈도우를 닫을 때**: `restore_layout = false` 라 슬롯을 다시 열지 않으면 그 윈도우가 소유한 항목(빈
  부모 목록·다음 번호 포함)을 지운다. 켜져 있으면 윈도우를 다시 열 수 있으므로 그대로 둔다.

### External 부모

부모가 이 Tasty의 surface가 아닐 때(예: 다른 프로세스의 에이전트 대화) 플러그인은 `parent_external`
문자열로 부모를 지정한다. 호스트는 이 키를 해석하지 않고 같은 문자열끼리 같은 부모로 본다. 키는
1~256 byte이며 공백·제어문자를 넣을 수 없다. 어떤 provider의 식별자로 키를 만들지는 플러그인이 정한다.

- **저장**: Surface 부모와 같은 registry의 별도 필드(`external_children`·`external_parent_of`·
  `external_next_index`)에 둔다. 비어 있으면 파일에 쓰지 않으므로 Surface 부모만 쓰는 파일은 이전 형식과
  같고, 이전 버전은 이 필드를 무시한다. 가짜 부모 surface를 만들지 않는다.
- **소유**: 항목은 자식 surface 의 소유·슬롯을 따른다. 저장·정리·창 닫기·시작 시 정리는 Surface 부모와
  같은 규칙으로 자식을 지운다.
- **번호**: 부모별로 0부터 준다. 여러 윈도우가 같은 키에 자식을 달 수 있으므로 번호를 예약할 때 파일을
  먼저 합치고, 저장할 때 다음 번호는 큰 값을 남긴다. 마지막 자식이 release·kill·닫기·정리로 빠지면 그
  부모의 목록과 다음 번호를 지우므로 다음 spawn은 0부터 받는다. 다른 윈도우에 자식이 남아 있거나, 파일의
  다음 번호가 알려진 자식 번호보다 커서 다른 윈도우가 예약만 하고 아직 등록하지 않은 번호가 있을 수 있으면
  번호를 유지한다. 중복 번호를 막기 위해 재시작을 미루는 쪽을 택한다.
- **점유**: soft 점유의 소유자로 External 키를 기록한다. 포커스 시점 정리는 사라진 부모 surface만 보므로
  External 소유 점유는 release·kill·surface 닫기로만 풀린다.
- **부모 생략 폴백**: `single_parent`는 surface 부모만 센다. External 부모의 자식에는 항상
  `parent_external`을 명시한다.

### 상태 보고와 화면 알림

`terminal.set_state --state needs_input`는 자식 상태만 갱신한다. 화면 표시를 담당하는
[AttentionStore](../surface-highlight/index.md)의 `AttentionKind::NeedsInput`와는 별개다.
Claude 플러그인은 같은 훅에서 `terminal.set_state`와
`surface.completion { kind: "needs_input" }`를 함께 호출한다
(`crates/tasty-plugin-claude/src/hook.rs::apply_hook`). 사용자가 탭을 보면 해제되는 것은
화면 알림이며, `tasty terminal state`의 상태 보고는 바뀌지 않는다.

### spawn과 점유

spawn은 지정한 workspace의 pane에 `terminal` 탭을 만들고 registry에 등록한다.
새 자식에는 부모(surface 또는 External 키)를 소유자로 하는 soft 점유를 설정한다. soft 점유는 표시만 하고
입력을 막지 않는다(`attached=false`). 부모가 사라지면 포커스 시점의 지연 정리로 해제한다.

`command`는 선택 사항이다. 생략하면 탭 생성·등록·soft 점유·`child_surface_id` 반환까지만
진행한다. Codex·Claude 플러그인은 이 ID를 받은 뒤 session token과 surface ID를 포함한
명령을 구성해 `surface.send`로 보낸다. 호스트는 지정된 명령 문자열을 그대로 전송한다.

점유 처리는 내부 함수 `occupy_soft(child, parent)`, `release_occupancy(child)`,
`release_soft_occupancy(child, parent)`를 사용한다. 외부 `occupancy.*` IPC는 없다
([Attach 메커니즘](../../dev-guide/attach-behavior.md)).

### adopt와 release

adopt는 새 PTY를 만들지 않고 기존 surface를 자식으로 등록한다. 대상 존재, 자기 자신인지,
이미 등록됐는지, hard 점유 중인지를 순서대로 검사하고 soft 점유를 시도한다. 다른 부모의
soft 점유 때문에 실패하면 registry는 변경하지 않는다. spawn과 달리 점유를 먼저 확보해
자식 목록과 점유 목록이 어긋나지 않게 한다.

release는 자식 관계와 soft 점유만 제거하고 탭은 남긴다. registry에서 제거한 뒤 저장하며,
점유는 부모까지 확인하는 `release_soft_occupancy(child, parent)`로 해제한다. 따라서 hard
점유를 해제하지 않는다. 점유가 이미 풀려 있어 해제에 실패하더라도 경고만 기록하고
관계 제거는 성공한다. kill은 같은 방식으로 관계와 soft 점유를 제거한 뒤 surface도 닫는다.
kill도 hard 점유는 해제하지 않는다. child가 원격 attach로 hard 점유되어 있으면 `surface.close`와
같은 사유로 거절하고 관계·점유·탭을 그대로 둔다. 점유를 강제로 풀면 holder가 workspace 전체에서
분리되기 때문이다.

Claude의 PTY 오류 스캐너는 `terminal.parent`로 관계가 남아 있는지 확인한다. release 후에도
surface는 살아 있으므로 `surface.locate`로 대신 판단하면 감시를 끝낼 수 없다.
release는 해당 자식의 `claude-error` 알림도 중단한다
([Claude 플러그인](../../plugins/claude/index.md)의 PTY 오류 스캔 범위 참고).

## 훅과 출력 관측을 함께 사용하는 상태 판정


`terminal.children`과 `terminal.state`는 훅으로 보고받은 상태와 호스트 관측을 함께 사용한다.
두 경로는 `EngineRef::child_liveness{,_with_live}`를 공유한다.
registry의 `active`는 idle이나 입력 대기 보고가 없다는 뜻이므로 실제 실행의 증거가 부족할 수 있다.
상태를 보완하기 위해 busy 캐시·최근 조회한 전경 이름·출력 시각을 함께 사용한다.
이 값들은 프로세스의 생존이나 종료를 직접 검사한 결과가 아니다.
`confirmed`도 특정 관측 분기에 붙이는 응답 분류이며, OS의 종료·회수 확인을 뜻하지 않는다.
마지막 훅 보고 시각은 등록 시 초기화하고 각 보고 때 갱신하며, 재시작 후에도 해석할 수 있도록
Unix epoch 밀리초로 저장한다. 관측 조합은 registry 밖의 순수 함수에서 판단한다.

### 판정 우선순위

위에서 아래로 먼저 맞는 규칙이 이긴다.

| # | 조건 | `state` | `confidence` | `evidence` |
|---|---|---|---|---|
| 1 | surface 가 라이브 트리에 없음 | `exited` | `confirmed` | `surface_gone` |
| 2 | hook 이 `needs_input` 보고 | `needs_input` | `reported` | `hook_needs_input` |
| 3 | hook 이 `idle` 보고 | `idle` | `reported` | `hook_idle` |
| 4 | busy 캐시가 true | `active` | `confirmed` | `pty_busy` |
| 5 | 로컬 TerminalStore에 없음(deferred·mirror 등) | `active` | `unobserved` | `pty_not_started` |
| 6 | 캐시된 전경 이름이 알려진 셸 이름 | `stale` | `confirmed` | `foreground_is_shell` |
| 7 | 로컬 출력 후 경과시간을 알 수 없음 | `active` | `unobserved` | `observation_unavailable` |
| 8 | 무출력 < 임계값 | `active` | `heuristic` | `recent_output` |
| 9 | 무출력 ≥ 임계값 && hook 침묵 < 임계값 | `active` | `heuristic` | `recent_hook_report` |
| 10 | 무출력 ≥ 임계값 && hook 침묵 ≥ 임계값 | `stale` | `heuristic` | `output_and_hook_silent` |

- 2·3은 명시적으로 받은 보고를 출력 관측보다 우선한다. 보고의 진위나 최신성을
  검증했다는 뜻은 아니며, 보고가 오래됐어도 이 두 분기가 우선한다.
- 4의 busy 캐시는 최근 출력뿐 아니라 터미널 상태 락 경합 때문에 true가 될 수도 있다.
  mirror는 원격에서 전달받은 busy 캐시를 사용한다.
- 5는 로컬 Terminal이 없을 때 무출력 시간만으로 stale을 만들지 않도록 한다.
  이 조건만으로 deferred와 mirror를 구별하지 않는다.
- 6은 마지막 전경 조회의 이름을 사용한다. 현재 프로세스를 다시 조회하거나
  에이전트의 종료 상태를 OS에서 기다린 결과가 아니다.
- 임계값은 무출력 `CHILD_OUTPUT_SILENCE` = 120s, hook 침묵 `CHILD_HOOK_SILENCE` = 300s다.
  busy의 짧은 출력 구간인 `BUSY_OUTPUT_WINDOW`(2s)와 용도가 다르다.
- hook 보고 기준 시각이 없으면 침묵으로 간주한다. 무출력 시간도 이미 임계값을 넘겼고
  최근 보고를 확인할 수 없을 때 stale로 분류한다.

### 미등록 surface

`terminal.state`는 registry에 등록되지 않은 surface도 조회할 수 있다.
응답은 registry 원값에 관측을 합친 판정이다. 예를 들어 로컬 Terminal이 있고
busy가 아니며 캐시된 전경 이름이 셸이면 `stale`/`foreground_is_shell`을 반환한다.
이름 캐시만으로 해당 surface에서 실행 중인 프로그램이 없다고 단정할 수는 없다.
라이브 트리에는 있지만 로컬 Terminal이 없으면 앞선 훅·busy 분기를 거친 뒤 `active`/`pty_not_started`로 남는다.

### `stale` 의 의미와 한계

`stale`은 surface가 있지만 전경 이름이 셸이거나 출력·훅 보고가 오래 없다는 분류다.
`exited`는 surface가 라이브 트리에 없다는 분류다. 어느 쪽도 OS 프로세스의 종료·회수를
직접 확인한 값은 아니다. `terminal.adopt`로 등록한 자식은 처음부터 일반 셸일 수도 있다.

무출력 시간만으로는 SIGSTOP, 긴 추론, 출력 없는 긴 명령을 구별할 수 없다.
`reported`는 훅으로 받은 상태, `unobserved`는 필요한 관측이 없어 active로 남긴 상태다.
`confirmed`를 포함한 이 분류만으로 작업 성공이나 재시작 가능 여부를 결정하지 않는다.

### 출력 전용

`stale`/`exited` 는 호스트가 관측으로만 만들어내는 값이다. `terminal.set_state` 는
여전히 `idle`/`needs_input`/`active` 세 값만 받는다 — hook 이 파생 상태를 registry 에
밀어넣을 수 있으면 호스트 관측과 훅 보고를 구분할 수 없게 된다.

### 조회만이 소비처가 아니다 — push 축

Claude 플러그인의 `error_scan`은 출력 정지가 일정 시간 이어지면 `terminal.state`를 조회한다.
상태가 `active` 또는 `stale`이면 `claude-error-stalled` → `notify-error` 경로로 부모의
[완료 알림 로그](../../dev-guide/external-interaction.md#child-완료-알림--completion-log)에 남긴다.
이 이벤트 이름은 기존 구독을 유지하기 위한 것이며 오류가 검출되지 않은 정지도 포함한다.
알림 문구는 오류 뒤 정지, 오류 없이 정지한 경우, 백그라운드 작업을 기다리는 경우를 구분한다.

오류 직후 정지는 `STALL_QUIET`(30초), 오류 없는 정지는 플러그인 설정 `no_output_notice_secs`(기본 600초)로 확인한다. 이 설정은 `stale` 분류의 `CHILD_OUTPUT_SILENCE`와 별개다.
정지 시간·중복 여부·쿨다운을 먼저 확인하므로 매 tick마다 상태 조회를 보내지는 않는다.
한 정지 구간에서는 한 번만 알리고 출력이 다시 시작되면 재알림을 허용한다.
백그라운드 작업을 기다리는 자식(대기 Stop 뒤)은 기다리는 작업의 출력 파일을 찾아 모두 `no_output_notice_secs` 동안 바뀌지 않으면 작업 이름을 적어 알린다. 파일을 찾지 못하면 화면 출력을 10분과 설정값 중 큰 값으로 본다. 어느 쪽이든 대기 한 번에 한 번만 알린다. Monitor 작업은 이벤트가 있을 때만 출력하므로 이 판정에서 뺀다. 기다리는 작업이 모두 Monitor면 알리지 않고, 그런 자식은 Monitor가 끝날 때까지 `active`로 남는다.
세부 규칙은 [Claude 통합의 정지 알림](../../plugins/claude/index.md#정지-알림-claude-error-stalled--부모-completion-log)에 있다.
나중에 실제 `needs_input` 훅이 와도 그 알림은 별도로 전달한다.

추정 `stale`도 알림 대상이다. 긴 추론과 실제 정지는 구별되지 않을 수 있으므로 부모가
상태를 확인해야 하며 이 알림만으로 작업을 재시작하지 않는다. `stale` 자체도 훅 유실을
증명하지 않는다. 재사용 후보를 세는 `spawn_census`는 `confidence: confirmed`인 `stale`만 포함한다.
이는 캐시된 셸 이름을 근거로 한 목록이며 종료나 훅 유실이 확인됐다는 뜻은 아니다.
이 감시는 `terminal.set_state`를 호출하지 않고 관측 결과를 읽기만 한다.
Codex에는 이 출력 스캐너가 없으므로 같은 감시가 있다고 설명하지 않는다.

### 능동 프로빙 배제

대상 surface 에 입력을 주입해 반응을 보는 능동 프로빙은 사용자 입력 재현이라 release
금지 대상이고([사용자 통제와 데이터 원칙](../../principles/control-and-data.md)), 자식 에이전트의 프롬프트
상태도 오염시킨다. 판정은 **수동 관측만** 쓴다.

### 비용

추가 프로세스 스냅샷은 없다. 전경 프로그램 이름은 1Hz 일괄 폴링이 이미 채우는
`foreground_names` 캐시에서만 읽는다 — 자식마다 `Terminal::foreground_process_info()`
를 개별 호출하면 O(surfaces × processes) 를 되살리는 회귀다
(`src/runtime/terminal_activity.rs` 폴링 주석).

## 인터페이스

- **AI Agent (IPC/CLI)** — 모든 대상은 ID 로 직접 지정(포커스 독립, [포커스 정책](../../design/policies/focus.md)):
  - `tasty terminal spawn --workspace <ws> --command "<cmd>" [--surface <parent>] [--pane] [--cwd] [--role] [--nickname]` ↔ `terminal.spawn`
  - `tasty terminal tell "<text>" [--surface]` ↔ `terminal.tell`
  - `tasty terminal children [--surface] [--format table|json]` ↔ `terminal.children` — 기본 출력은 [부모 ID 표](../../dev-guide/cli-structure.md#부모-id-표-formatparent_tablers)다. 아래 "children 출력" 참조
  - `tasty terminal parent --surface <child>` ↔ `terminal.parent`
  - `tasty terminal state --surface <child>` ↔ `terminal.state` — 자식 단건 상태(`idle`/`needs_input`/`active`/`stale`/`exited`) 조회. `terminal.children` 의 항목별 `state` 와 **같은 판정 헬퍼**(`EngineRef::child_liveness`)를 쓰므로 목록과 단건의 답이 갈리지 않는다. 이미 registry 에서 정리된(reconcile 로 사라진) surface 도 라이브 트리와 직접 대조해 `"exited"` 로 판별한다 — `ChildTerminalRegistry::state_of` 자체의 미등록 surface `"active"` fallback 계약은 그대로 둔 채, 상위 판정 계층이 그 위에서 죽은 surface 를 걸러낸다
  - `tasty terminal kill [--surface] --child <n>` ↔ `terminal.kill` — child surface 를 먼저 닫고, 실제로 닫혔을 때만 관계를 지우고 `killed_surface_id` 로 답한다. mirror workspace 의 child 처럼 닫기가 원격 실행 큐로 넘어가면 `{"forwarded": true, "surface_id", "child_index"}` 로 답하고 관계와 soft 점유를 남긴다. 원격이 surface 를 닫으면 구조 delta 반영이 soft 점유를 지우고, 관계는 그 뒤 reconcile 이 정리한다. 닫기가 실패하면 그 오류를 돌려주고 관계·soft 점유는 그대로다
  - `tasty terminal respawn [--surface] --child <n> [--cwd] [--command] [--role] [--nickname]` ↔ `terminal.respawn`
  - `tasty terminal broadcast "<text>" [--surface] [--role]` ↔ `terminal.broadcast`
  - `tasty terminal set-state --surface <child> --state <idle|needs_input|active>` ↔ `terminal.set_state` (에이전트 hook 진입점). **파생 상태(`stale`/`exited`)는 입력으로 받지 않는다** — 출력 전용이다(아래 "상태 판정")
  - `tasty terminal adopt --target <surface> [--surface <parent>] [--cwd] [--role] [--nickname]` ↔ `terminal.adopt` — 새 탭을 만들지 않고, 이미 존재하는 임의의 surface 를 지금 시점에 명시적으로 child 로 등록(soft 점유)한다
  - `tasty terminal release [--surface <parent>] --child <n>` ↔ `terminal.release` — child 관계와 soft 점유만 해제한다. surface(탭) 자체는 닫지 않는다(`terminal.kill`과 달리)
- **External 부모** — 플러그인은 IPC로 직접 쓰고, CLI는 `--parent-external <key>`로 노출한다(`--surface`와 함께 쓸 수 없다). Claude·Codex 플러그인 명령의 일반 spawn·tell은 부모를 스스로 정하므로 사용자가 키를 입력하지 않는다. 이 옵션은 키를 직접 관리하는 스크립트·에이전트용이다:
  - CLI: `tasty terminal spawn --workspace <ws> --parent-external <key> --command "<cmd>"`, `tasty terminal children|kill|respawn|broadcast|release --parent-external <key> [--workspace <ws>]`. `--workspace`는 `--parent-external`과 함께만 받으며 다중 창 라우팅에 쓴다. `--parent-external`을 주면 caller의 `TASTY_SURFACE_ID`를 숫자 부모로 채우지 않는다.
  - `terminal.spawn`은 `parent`(별칭 `surface`) 대신 `parent_external`을 받는다. `workspace`는 그대로 필요하다.
  - `terminal.children`·`release`·`kill`·`respawn`·`broadcast`는 `surface` 대신 `parent_external`을 받는다. `tell`은 자식 surface를 직접 지정하므로 바뀌지 않는다.
  - 숫자 부모와 `parent_external`을 함께 주면 `-32602`(`pass either '<key>' or 'parent_external', not both`)로 거절한다.
  - `parent_external`이 문자열이 아니거나 비었거나 256 byte를 넘거나 공백·제어문자를 담으면 `-32602`로 거절한다. 생략으로 읽어 부모 생략 폴백으로 넘기지 않는다. mirror workspace 자식을 닫는 `terminal.kill`도 원격으로 넘기기 전에 같은 규칙으로 판정하므로, 잘못된 키는 다른 부모의 자식을 닫지 않고 같은 오류로 끝난다.
  - `terminal.parent`는 `parent_surface_id`와 함께 `parent_external`(없으면 `null`)을 돌려준다.
  - `terminal.adopt`는 `parent_external`을 받지 않는다.
  - 창이 둘 이상인 GUI에서는 `parent_external`만으로 창을 고를 수 없으므로 `workspace`(이름 또는 ID)를 함께 준다. 없으면 라우팅 단계에서 거절한다.

### children 출력

`tasty terminal children`, `tasty claude children`, `tasty codex children`은 옵션 없이 같은 표를 낸다. 지정한 부모의 직계 자식만 내므로 표는 한 단계이고 `parents`는 모든 행에서 같다. 행은 surface id 숫자 순이다.

```text
| id | parents | nickname | state | cwd | last_report_at | index | confidence | evidence |
|---|---|---|---|---|---|---|---|---|
| 12 | 7 | lane-a | idle | /work/a | 2026-01-05T11:40:00Z | 1 | reported | hook_idle |
```

| 열 | 값 |
|---|---|
| `id` | 자식 surface id |
| `parents` | 조회한 부모. surface id이거나 외부 부모 키(`codex:<thread>`)다 |
| `last_report_at` | 등록 또는 마지막 상태 보고 시각. 초 단위로 내린 UTC다 |
| `index` | `kill`, `respawn`, `release`의 `--child`가 받는 번호 |
| `confidence`, `evidence` | 아래 판정 응답 필드. `state`와 함께 읽는다 |

`role`, `remote`, `liveness`, `foreground_process`, `foreground_pid`는 `--format json`으로 본다. `--format json`은 IPC 응답을 그대로 낸다. 모양은 명령마다 다르다([plugin 쌍의 차이](../../dev-guide/paired-agent-handlers.md)). `terminal.children`의 각 항목에는 판정 필드 외에 `parent`(부모 surface id 또는 외부 키)와 `last_report_at`(밀리초, 없으면 `null`)이 있다.

### 판정 응답 필드

`terminal.children` 의 각 항목과 `terminal.state` 단건 응답은 판정에 사용한 세 필드를 모두
포함한다. 두 경로가 같은 직렬화 지점(`liveness_fields`,
`src/adapters/ipc/handler/terminal.rs`)을 거치므로 키 집합과 값이 구조적으로 일치한다.

| 필드 | 값 |
|---|---|
| `state` | `exited` \| `needs_input` \| `idle` \| `active` \| `stale` |
| `evidence` | `surface_gone` \| `hook_needs_input` \| `hook_idle` \| `pty_busy` \| `pty_not_started` \| `foreground_is_shell` \| `observation_unavailable` \| `recent_output` \| `recent_hook_report` \| `output_and_hook_silent` |
| `confidence` | `confirmed` \| `reported` \| `heuristic` \| `unobserved` |

나오는 조합은 위 "판정 우선순위" 표의 행 그대로다 — 임의 조합은 생기지 않는다.

`state`와 `confidence`, `evidence`를 함께 읽어야 한다. 예를 들어 `active`는 busy 캐시가
true인 경우(`pty_busy`/`confirmed`)와 필요한 관측이 없는 경우
(`observation_unavailable`/`unobserved`)에 모두 나온다. busy에는 락 경합도 포함되므로
현재 출력 중이라는 뜻으로만 읽어서는 안 된다.

`confirmed`는 종료 확정 표시가 아니다. 재사용·종료 여부를 정할 때는 해당 자식의
현재 상태와 작업 결과를 따로 확인한다. 특히 `heuristic`인 `stale`은 SIGSTOP·긴 추론·
무출력 명령과 구별되지 않는다.

원시 관측값(`busy`, 무출력 경과시간 등)은 싣지 않는다 — `evidence` 가 "어떤 관측으로
판정했는가" 를 이미 알려주므로 목적이 달성되고, 원시값을 계약으로 굳히면
임계값 조정이 소비자 계약 변경이 된다.

**소비자 정합**: claude plugin 의 `claude.children` remap 은 화이트리스트라 세 필드를
명시적으로 옮긴다(`crates/tasty-plugin-claude/src/handlers.rs`). codex plugin 의
`codex.children` 은 passthrough 라 자동 반영된다.

### `--child <n>` 은 index 지 surface_id 가 아니다

`--child` 는 부모별로 0 부터 발급되는 **child index**(`ChildTerminalRegistry::next_index_for`)를
받는다. `terminal.children` 이 반환하는 `surface_id` 와는 다른 번호 공간이며, 둘 다 정수라
혼동하기 쉽다. 두 공간은 **실제로 겹칠 수 있다** — 새 인스턴스는 surface id 도 1, 2, 3… 이라
`--child 2` 가 index 2 인지 surface 2 인지 구조적으로 구분되지 않는다. 그래서 넘어온 값을
surface_id 로 자동 해석하지 않고, **인자 의미는 index 로 고정한 채 에러 메시지가 안내**한다:

| 넘긴 값 | 응답 |
|---|---|
| 같은 부모의 `child_surface_id` | `… 4 is a child_surface_id, not a child index — use \`--child 2\`` |
| 다른 부모의 `child_surface_id` | `… under a different parent — use \`--surface 9000 --child 4\`` (External 부모면 `--parent-external <key> --child 4`) |
| 그 외(오타·범위 밖·이미 정리됨) | `… (valid child indices: 0, 2; 2 children)` |
| 그 외 + 등록된 child 가 0 | `… (no children registered under surface 9000)` |

kill/release/respawn 세 경로가 같은 메시지를 쓴다. 실패는 `exit=1` + stderr 이므로, 일괄
처리 스크립트는 **호출당 종료코드를 확인해야 한다** — 버리면 전건 실패를 성공으로 오인한다.

### `--surface` 생략과 다중 윈도우

`kill`/`release`/`respawn`/`broadcast` 는 `--surface`(parent) 를 생략할 수 있다 — host 가
현재 engine 의 `runtime.child_terminals.single_parent()` 로 폴백한다(parent 가 정확히 1개일 때만
성공, 0 개·2 개 이상이면 에러). 셈에 넣는 parent 는 그 engine 이 소유했거나 그 engine 에 살아 있는
surface 뿐이다. 저장 병합으로 받아 온 다른 윈도우의 항목은 세지 않는다. 이 폴백은 **그 engine(= 하나의 main window) 안에서만**
유일성을 본다 — main window 가 2 개 이상 열린 세션에서는 애초에 어느 window 를 봐야
하는지가 정해지지 않는다. 그래서 이 4 개 메서드가 `--surface` 없이(그리고 라우팅 가능한
다른 리소스 id 도 없이) 호출됐는데 main window 가 2 개 이상이면, focused window 로 조용히
새지 않고 라우팅 단계(`App::find_request_owner`)에서 명시적 에러로 거부한다(단일 윈도우
세션은 기존처럼 생략 가능 — 하위 호환). 구현: `src/app/request_owner.rs`
`ambiguous_parent_fallback_requires_surface`.

## 비-목표 (Out of scope)

- **에이전트 특화 로직** — codex/claude 바이너리 command 빌더, hook/trust, telemetry 는 플러그인에 잔류한다. 호스트는 임의 command 만 붙인다.
- **`terminal launch`(새 workspace 생성)** — 에이전트 편의 명령이라 범위 밖.
- **soft 입력 독립성(잔여 입력 비오염)** — 현재는 단순 `surface.send` 로 붙인다(ADR 방향성만).

## Acceptance Criteria

- Given workspace `<ws>` When `terminal.spawn{parent=P, command}` Then 자식 터미널이 생성·registry 등록되고 `occupancy_of(child)==Soft`·`holder.parent==P`·`attached=false`.
- Given soft 점유된 child C When `terminal.kill` Then `occupancy_of(C)==None` + surface 닫힘.
- Given 원격 attach로 hard 점유된 workspace의 child C When `terminal.kill` Then "hard-occupied" 에러 반환 + child 관계·holder 점유·surface 모두 그대로.
- Given mirror workspace의 surface를 adopt한 child C When `terminal.kill` Then GUI는 `forwarded:true`(`killed_surface_id` 없음), headless는 mirror 사유 오류를 반환하고 child 관계·soft 점유·surface 모두 그대로.
- Given 죽은 자식이 남은 registry When `terminal.children` Then reconcile 로 목록에서 제거.
- Given 윈도우 두 개 When 각 윈도우에서 `terminal.adopt` 로 자식을 등록한다 Then 파일에 두 관계가 모두 남는다(`child_terminal.rs` 의 `children_registered_in_two_windows_both_survive_a_restart`).
- Given 재시작 뒤 윈도우 하나만 열렸다 When 그 윈도우에서 `terminal.children` 이 정리를 돌린다 Then 아직 열지 않은 윈도우의 관계는 지우지 않는다(`reconcile_keeps_children_of_windows_it_does_not_own`).
- Given 두 윈도우에 각각 부모가 하나씩 있다 When 한 윈도우가 저장해 다른 윈도우의 항목을 받은 뒤 `--surface` 없이 `terminal.children` 을 부른다 Then 그 윈도우의 부모 하나로 폴백한다(`single_parent_counts_only_this_windows_parents_after_a_merge`).
- Given 파일에 자식 관계가 있다 When 설정을 받아 시작 시 정리를 부른다 Then `restore_layout = true` 면 관계가 남고 `false` 면 모두 버린다(`boot_clear_follows_the_restore_layout_setting`).
- Given `restore_layout = false` When 재시작을 세 번 하며 매번 자식을 하나 등록한다 Then 파일에는 마지막 실행의 관계만 남고 `--surface` 생략 폴백이 동작한다(`relations_do_not_pile_up_across_restarts_without_layout_restore`, 시작 시 정리는 설정을 받아 부른다).
- Given `restore_layout = true` 이고 슬롯 1·2만 다시 열 수 있다 When 시작 복원이 끝난다 Then 슬롯 3 의 관계를 지우고 슬롯 1·2 와 슬롯 표시 없는 항목은 남긴다(`boot_prune_keeps_relations_of_slots_that_can_reopen`). journal 경로에서는 스트림이 있어도 은퇴한 슬롯의 관계를 지운다(`resumed_bootstrap_forgets_child_relations_of_slots_that_cannot_reopen`).
- Given `restore_layout = false` When 윈도우를 닫는다 Then 그 윈도우의 관계·빈 부모 목록·다음 번호를 지우고 다른 윈도우의 것은 둔다(`a_released_window_removes_its_parent_keys`).
- Given 이미 존재하는 임의의 surface(spawn 으로 만들지 않은 일반 터미널 탭 포함) When `terminal.adopt{surface=P, target}` Then `occupancy_of(target)==Soft`·`holder.parent==P`·`terminal.children` 목록에 나타남.
- Given 이미 등록된 child 또는 hard 점유 중인 대상 When `terminal.adopt` Then 에러 반환 + registry 불변.
- Given 점유된 child C When `terminal.release` Then `occupancy_of(C)==None` + `terminal.children` 목록에서 사라짐 + surface(탭)는 여전히 열려있음(닫히지 않음).
- Given 등록되지 않은 child index When `terminal.release` Then 에러 반환.
- Given C 와 무관한 다른 surface 가 hard 점유 중 When `terminal.release{child=C}` Then 그 hard 점유는 영향받지 않음.
- Given External 부모 A·B에 자식이 하나씩 있다 When `terminal.children{parent_external=A}`·`terminal.release{parent_external=A, child=0}` Then A의 자식만 나오고 해제되며 A의 다음 번호는 0으로 돌아가고 B는 그대로다(`external_parents_list_report_and_release_only_their_own_children`).
- Given 두 윈도우가 같은 External 부모에 자식을 단다 When 각자 번호를 예약하고 저장한다 Then 번호가 겹치지 않고, 마지막 자식이 빠진 저장이 파일의 다음 번호를 지운다(`two_engines_share_an_external_parent_without_duplicate_indices`).
- Given 다른 윈도우가 같은 External 부모에 번호를 예약만 했다 When 이 윈도우가 마지막 자식을 빼고 저장한다 Then 파일의 다음 번호가 남아 예약 번호를 다시 주지 않는다(`a_pending_reservation_in_another_engine_survives_the_last_release`).
- Given External 부모가 소유한 soft 점유 When 부모 surface가 없는 상태에서 포커스 정리를 한다 Then 점유가 남는다(`focus_cleanup_keeps_an_external_parent_that_has_no_surface`).
- Given 숫자 부모로 기록된 이전 journal 레코드 When 읽는다 Then Surface 부모로 해석한다(`a_child_recipe_with_a_numeric_parent_still_reads`).
- Given mirror workspace 자식과 Surface 부모 하나 When `terminal.kill`에 숫자·배열·빈 문자열·공백 포함 같은 잘못된 `parent_external`을 준다 Then 원격으로 넘기지 않고 일반 해석이 `-32602`로 거절하며 어떤 자식도 닫지 않는다(`an_invalid_external_parent_does_not_fall_back_to_another_parent`).
- Given 실행 중인 child C When `terminal.state{surface=C}` Then 응답이 `state`·`evidence`·`confidence`·`surface_id:C` 를 싣고 `state` 는 `exited` 가 아니다(값은 위 판정 우선순위 표).
- Given `terminal.kill`로 종료된 child C When `terminal.state{surface=C}` Then `state` 가 `"exited"` 다 (`"active"`가 아님).

## spawn 준비와 실제 PTY 종료

spawn은 registry 등록·soft 점유 준비 후 command를 전송한다. 준비나 동기 전송 단계에서 오류가 나면 이번 호출이 만든 surface만 표준 agent close 경로로 정리하고 자신의 registry/점유도 회수한다. adopt의 실패는 기존 surface나 같은 부모가 이미 갖고 있던 soft 점유를 변경하지 않는다.

headless의 실제 PTY 종료도 GUI와 같은 host process-exit 경로를 사용한다. SessionEnd 없이 종료해도 process-exit 훅을 발생시키고 soft 점유를 정리한다. headless도 HookFired의 task waiter를 처리하며 view 전용 ProcessExited broadcast는 GUI에 남는다. 종료 원인만으로 작업 성공을 추론하지 않는다.
