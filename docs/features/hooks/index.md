# 훅 (Surface / Global hooks)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`hook.*` / `global_hook.*`)
- **ADR**: 없음
- **코드**: `tasty-hooks` 크레이트(`HookManager`/`HookEvent`/`HookBinding`), `hook.*`·`global_hook.*` 핸들러, 실행 배선 `src/hook_handler/trigger.rs`. IdleTimeout 폴링·발화 배선: `src/core/state/idle_hooks.rs`(엔진 쿼리) + `src/app/idle_hooks.rs`(GUI 실행)/`src/boot.rs`(headless 실행). OutputMatch 라인 버퍼 공유: `src/core/output_observer.rs::ObserverRouter::dispatch_text`
- **화면**: 없음

## 목적

특정 이벤트 발생 시 동작을 자동 실행하는 훅. surface 별 이벤트 훅(`hook.*`)과 surface 무관 글로벌 훅(`global_hook.*`)이 있다. "에이전트가 에이전트를 제어하는 자동화" 의 토대(conductor 가 polling 없이 자식 완료를 감지).

surface hook 은 더 이상 셸 명령 문자열을 직접 들지 않고, **공유 훅 핸들러 레지스트리**(`src/hook_handler/`)의 핸들러를 참조한다 — 웹훅과 훅이 같은 핸들러 정의를 공유하는 구조다. 기존 `--command` 인라인 셸은 하위호환을 위해 **익명 hook 핸들러**로 감싸 그대로 실행된다.

## 내부 동작

### Surface hook (`HookEvent`)

`HookManager` 가 등록/삭제/조회/실행 관리. 이벤트 타입:

| 이벤트 | 발화 |
|--------|------|
| `ProcessExit` | 셸 프로세스 종료 |
| `OutputMatch(pattern)` | PTY 출력이 정규식 매칭(등록 시 사전 컴파일) — **완성된 라인 단위로만** 매칭 |
| `Bell` | BEL 수신 |
| `Notification` | OSC 알림 수신 |
| `IdleTimeout(secs)` | N초간 PTY 출력 없음 — **1Hz 해상도**(`Tick::Busy`) |
| `CommandCompleted(Option<i32>)` | OSC 133 D phase — 셸 통합이 개별 명령(`docker build`, `just run` 등)의 종료 + exit code 를 보고 |
| `Custom(string)` | 코어가 모르는 임의 이벤트 식별자. 정확 문자열 일치로 매칭. 플러그인 소유 이벤트(예: claude plugin 이 fire 하는 `claude-idle` / `needs-input` / `claude-error`)는 모두 이 변형으로 처리된다 — 코어에 에이전트 고유 이벤트명을 박지 않는다. |

#### OutputMatch — 완성된 라인 단위 매칭

`OutputMatch` 는 PTY 출력을 별도로 누적하지 않고 `ObserverRouter::dispatch_text`(`output.observe` 옵저버가 쓰는 것과 **동일한** per-surface `LineBuffer`)를 공유한다. `dispatch_text` 가 그 청크에서 완성된(`\n` 로 끝난) 줄만 반환하고, 그 줄들에 대해서만 정규식 매칭을 시도한다.

- 줄바꿈 없이 끝나는 청크(예: 프롬프트 대기 중 부분 출력)는 매칭 대상이 아니다 — 다음 청크가 도착해 줄이 완성돼야 매칭된다. 패턴이 청크 경계에 걸쳐 있어도(`"partial ERR"` + `"OR\n"` → `"partial ERROR"`) 완성 시점에 합쳐진 전체 줄로 매칭된다.
- 옵저버(`output.observe`)가 하나도 등록되지 않은 surface 도 OutputMatch 훅만으로 라인 버퍼 게이트가 열린다(`has_output_match_hook`) — 옵저버 등록이 OutputMatch 동작의 전제조건이 아니다.
- PTY emit 게이트(`sync_output_event_gates`)는 `hook.set`/`hook.unset` 처리 시점에 **즉시(eager)** 동기화된다(`Core::register_surface_hook`/`unregister_surface_hook`) — `observer_register`/`observer_unregister` 와 동일 패턴. VTE 파싱은 전용 parser thread(ADR-0002)가 PTY 바이트 도착 즉시 처리하므로, 게이트를 다음 `process_surface` 호출까지 지연시키면 등록 직후 도착하는 매칭 출력이 게이트 OFF 상태로 파싱되어 이벤트가 유실된다.

#### IdleTimeout — 1Hz 폴링 + epoch 기반 anti-spam

`IdleTimeout` 은 별도 타이머/watcher 가 아니라 기존 `Tick::Busy`(1Hz) tick 에 얹혀 동작한다. tick 마다 `Terminal::last_output_at()` 로 마지막 출력 시각과의 경과초를 계산해 임계값과 비교한다.

- 최대 1초 지연: PTY 출력 정지 시점과 훅 발화 시점 사이에 최대 1초의 오차가 있다(Global hook 의 `file:` 조건과 동일한 해상도).
- **epoch 기반 anti-spam**: 한 번 발화하면 그 시점의 `last_output_at` 값(epoch)을 기록해, 같은 epoch 동안은 재발화하지 않는다(persistent 훅이 매 tick 마다 스팸처럼 재발화하는 것을 막음). 새 출력이 들어와 `last_output_at` 이 갱신되면 자동으로 재무장된다.
- `once` 훅은 발화 후 즉시 제거된다(Global hook 의 `once:SECS` 와 동일한 시맨틱).

#### CommandCompleted — OSC 133 명령 완료(exit code)

셸 프로세스 자체의 종료(`ProcessExit`)와 달리, 셸 *안에서* 실행되는 개별 명령(`docker build`, `just run build` 등)의 완료를 감지한다 — 서브프로세스 종료는 `process-exit` 으로 원리적으로 감지 불가능하다(portable-pty 의 `Child` 추상화가 단일 pid 만 wait). OSC 133 셸 통합(zsh/bash preexec 등)이 D phase(`\e]133;D;<exit_code>\a`)를 보내면 [`command_index`](../terminal-output/index.md)가 이미 인덱싱하는 것과 별개로, 이 훅이 항상(exit code 필터링 없이) 발화한다.

- **등록**: `command-completed` = 임의 exit code 매치. `command-completed:<N>` = 그 exit code 만 매치(예: `command-completed:1` 로 실패한 명령만 구독). 실제 발생 이벤트는 항상 특정 exit code 를 담으므로, `None` 등록만 모든 발생과 매치되고 `Some(n)` 등록은 그 값과 일치할 때만 매치된다.
- **전제 조건**: 셸이 OSC 133 셸 통합 스크립트를 로드해야 한다. 미설치 셸은 D phase 자체가 안 와 이 훅이 절대 발화하지 않는다 — surface 가 출력을 내는데도 일정 시간(10 초) 지나도록 OSC 133 boundary 를 한 번도 못 받으면 "셸 통합 미설치" 안내 배너(`shell-integration-missing`, 마우스 캡처 안내 배너와 동일한 형태 — 자동 조치 없이 설명만)를 surface 스코프로 1 회 띄운다.
- **surface attention 자동 연결**: 이 훅 발화와 별개로, cascade(`cascade_terminal_command_completed`)가 exit code 무관하게 항상 `raise_attention`(kind=Completion) 도 함께 호출한다(설정 없이 즉시 동작하는 자동 경로) — [surface-highlight](../surface-highlight/index.md) 참고. 이 훅(커스터마이즈 경로)은 그와 독립적으로 동작한다 — 예를 들어 실패한 명령만 알림음을 울리고 싶으면 `command-completed:1` 로 별도 바인딩을 추가로 걸 수 있다(자동 attention 을 대체하는 게 아니라 그 위에 얹는 것).
- **구현 메모**: termwiz 는 OSC 133("133")을 미리 알려진 코드로 인식해 `Unspecified` 가 아니라 전용 `FinalTermSemanticPrompt` variant 로 구조화해 반환한다(A/C/D 는 항상 이 경로 — B 만 셸이 `cmd=` 등 부가 토큰을 붙이면 termwiz 의 엄격 파서가 실패해 `Unspecified` 로 폴백). `crates/tasty-terminal/src/vte_handler/osc.rs`가 이 variant 를 tasty 공통 `PromptBoundary{phase, payload}` 로 평평하게 변환해 이후 로직(command_index/이 훅)이 phase 문자만 보고 동작한다.

#### 이벤트 키 검증 (내장 + 플러그인 선언)

`HookEvent::parse` 는 미인식 문자열을 `Custom(String)` 으로 무조건 수용하므로(파싱·검증 책임 분리), `hook.set` / `surface.fire_hook` 핸들러 단계에서 키를 **(내장 ∪ 활성 플러그인 선언)** 집합으로 검증한다.

- **내장 이벤트**(`process-exit` / `bell` / `notification` / `output-match:` / `idle-timeout:` / `command-completed` / `command-completed:<N>`)는 플러그인 무관하게 항상 허용.
- **플러그인 선언 이벤트**는 플러그인이 manifest `[[contributes.hook_events]]` 로 자기가 발사하는 키를 선언해야 한다. 코어는 이름을 하드코딩하지 않고 이 카탈로그를 활성 플러그인 hello 시 집계한다(언로드/제거 시 제거). `disable`→`enable`(또는 `upgrade-builtins`)로 재기동된 새 프로세스의 hello 도, 무응답으로 **자동 재시작**된 새 프로세스의 hello 도 다시 집계된다 — 세 경로(disable · graceful swap · 재시작)가 모두 `PluginManager::forget_plugin_runtime`(`crates/tasty-host-plugin/src/manager/lifecycle.rs`)을 거치고, 그 함수가 `registered_plugins` gate 를 함께 지워야 재기동 후 hello 가 `finalize_plugin_hello`(→`hook_event_registry.register`)까지 재도달한다. 이 gate 를 안 지우면 재기동 후 hello 가 host 에 "이미 등록된 plugin" 으로 오판되어 조용히 무시되고, 그 plugin 이 선언한 hook 이벤트 전부가 완료 알림 없이 사라진다.
- 내장도 아니고 활성 플러그인이 선언하지도 않은 키(오타·미존재 이벤트)는 **등록 거부**(`invalid_params`, 에러 메시지에 내장 + 활성 선언 목록 안내). 죽은 hook 등록을 막는다.
- 따라서 **플러그인이 비활성이면 그 플러그인의 이벤트 hook 등록도 거부**된다(예: claude plugin 비활성 시 `claude-idle` hook 등록 불가 — 의도된 dead-setting 방지). claude plugin 은 위 3개 키를 manifest 로 선언한다.

- **once** 옵션: true 면 한 번 실행 후 자동 삭제. 기본은 persistent.
- **비동기 실행**: 훅 동작은 백그라운드에서(메인 루프 블로킹 없음 — 셸은 자식 프로세스 스레드, `IpcSequence` 는 아래 "바인딩" 절의 실행기 스레드). 각 이벤트의 발생 surface ID 를 추적해 올바른 surface 에서 실행.
- ProcessExit은 GUI/headless 모두에서 surface 자동 닫기까지 수행한다(surface→tab→pane→workspace 계층 정리, 마지막이면 새 셸 spawn). headless는 종료 hook의 binding을 먼저 모으고 surface를 닫은 뒤 실행한다.
- surface가 닫히면 그 surface의 once·persistent hook 등록도 제거한다. 이미 발화해 복사한 binding은 실행을 마치며, 다른 surface의 hook은 유지한다.

#### 바인딩 (핸들러 참조 vs 인라인 셸)

surface hook 은 `HookBinding` 으로 무엇을 실행할지 표현한다:

- **`Handler(id)`** — 공유 훅 핸들러 레지스트리 핸들러 id 참조(`tasty set hook --handler <id>`). 등록 시 핸들러가 존재하고 `source` 가 hook 트리거를 수용(`hook` 또는 `any`)하는지 검증한다 — `webhook` 전용 핸들러는 거부된다.
- **`InlineShell(cmd)`** — 하위호환 익명 셸(`tasty set hook --command "..."`). 레지스트리에 등록되지 않는 인라인 핸들러라 export/영속화 대상이 아니다.

`tasty-hooks` 는 leaf 크레이트라 레지스트리를 볼 수 없어 `(surface, event)` 매칭만 하고 바인딩을 돌려준다(`FiredHook` 에 매칭된 등록 이벤트 포함). 실제 실행(레지스트리 조회 + `source` 게이트 + `ShellCommand`→셸 / `IpcSequence`→IPC 순차 실행)은 본체 `hook_handler::trigger::execute_binding` 이 담당한다. `IpcSequence` 실행에는 IPC injector 가 필요하다(없으면 건너뛰고 warn). `IpcSequence` 는 발화한 스레드에서 실행하지 않는다 — surface 훅은 호스트 명령 큐를 비우는 스레드(GUI 메인 스레드 · headless 루프)에서 발화하므로, 거기서 스텝의 답을 기다리면 스텝마다 대기 상한(10 s)까지 화면과 IPC 가 선다. 그래서 `hook_handler::exec::enqueue_sequence` 가 전용 `hook-sequence` 스레드 하나에 넘기고 바로 돌아온다. 그 스레드는 받은 순서대로 시퀀스를 하나씩, 스텝은 앞 스텝의 답을 받은 뒤 다음 스텝을 넣는다. 대기 시퀀스가 호스트 주입 큐 상한(`tasty_ipc::admission::INJECTED_DEPTH_LIMIT`, 현재 256)과 같은 수에 이르면 새 시퀀스는 실행하지 않고 `error!` 로 남긴다(다시 걸지 않는다). 근거 [ADR-0498](../../adr/0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md).

#### 셸 핸들러 환경변수 (`TASTY_HOOK_*`)

`ShellCommand` 핸들러(인라인 셸 포함)가 spawn 하는 자식 프로세스에는 트리거 컨텍스트가 환경변수로 노출된다 — IpcSequence 가 `${body.*}` 값슬롯 치환으로 받는 값을 셸은 env 로 받는다(의미 대칭). 조립은 `src/hook_handler/env.rs` (순수 함수).

| 변수 | 값 |
|------|-----|
| `TASTY_HOOK_EVENT` | 훅 트리거: 등록 이벤트 표시 문자열(`bell` / `process-exit` / `output-match:<pattern>` / 플러그인 커스텀 키 등). `hook_handler.dispatch` 수동 발화: 핸들러 id |
| `TASTY_HOOK_SOURCE` | `hook`(내부 이벤트 트리거) 또는 `dispatch`(`hook_handler.dispatch` 수동 발화). 셸은 webhook 바인딩이 구조적으로 불가하므로 `webhook` 값은 존재하지 않는다 |
| `TASTY_HOOK_SURFACE_ID` | 훅 트리거의 발생 surface id. 수동 발화(surface 무관)에는 설정되지 않음 |
| `TASTY_HOOK_<UPPER_SNAKE_KEY>` | payload 가 object 면 최상위 key 각각. 훅 트리거의 payload 는 아래 [트리거 payload](#트리거-payload-이벤트별-key) 가 이벤트별로 채우고, `hook_handler.dispatch` 수동 발화는 params 의 `body` 를 그대로 쓴다 |

- **key 정규화**: ASCII 영숫자는 대문자로, 그 외 문자는 `_` 로. 정규화 결과가 겹치거나 위 예약 변수와 겹치면 **먼저 온 값이 이기고** 나머지는 무시한다. payload 안에서 서로 다른 원본 key 가 정규화 후 충돌하는 경우(예: `pr-id` vs `pr_id`)만 warn 하고, 예약 변수와의 충돌은 조용히 무시한다(`surface_id` 처럼 매 발화마다 생기는 정상 경로). 영숫자가 없는 key 는 건너뜀.
- **값**: 문자열은 그대로, 그 외 JSON 은 compact 표현. NUL 문자는 제거(플랫폼 env 제약), 값당 4096 바이트 초과분은 절단(Windows env 블록 상한 보호).
- **데이터/흐름 분리**: env 는 값 전달 전용 — 실행할 명령(command/args)은 레지스트리 owner 가 고정하므로 payload 가 실행 대상을 바꿀 수 없다.

참조 대상 핸들러 레지스트리는 [Settings › Handler › Hook Handlers](../settings/screens/settings.md) 서브탭에서도 조회·편집할 수 있다(user 매핑은 `~/.tasty/hook-handlers.toml` 영속). 시퀀스(`IpcSequence`) 본문은 그 서브탭에 편집 자리가 없고 아래 [핸들러 레지스트리 CLI](#핸들러-레지스트리-hook_handler)가 그 자리다.

##### 트리거 payload (이벤트별 key)

훅 트리거의 payload 는 `src/hook_handler/trigger.rs` 의 `trigger_payload` 가 조립한다 — 셸 env(`TASTY_HOOK_*`)와 IpcSequence 값슬롯(`${body.*}`)이 같은 소스에서 파생되는 단일 지점이다. 값은 **등록 패턴이 아니라 실제 관측된 이벤트**에서 채운다(예: `output-match:ERR.*` 로 등록해도 `matched_text` 는 실제로 매칭된 줄).

| 이벤트 | payload key | 셸 env | 값 |
|--------|-------------|--------|-----|
| (전 이벤트 공통) | `surface_id` | `TASTY_HOOK_SURFACE_ID` | 발화 surface id |
| `OutputMatch` | `matched_text` | `TASTY_HOOK_MATCHED_TEXT` | 매칭된 **완성 라인 전문**(정규식이 소비한 부분만이 아니다) |
| `CommandCompleted` | `exit_code` | `TASTY_HOOK_EXIT_CODE` | OSC 133 D phase 가 보고한 관측 exit code. D phase 가 코드를 안 실었거나 정수로 파싱되지 않으면 payload 는 JSON `null` 이고 셸은 문자열 `null` 을 받는다(`command-completed` 와일드카드 등록이 이 경우와도 매치된다) |
| `IdleTimeout` | `idle_elapsed_secs` | `TASTY_HOOK_IDLE_ELAPSED_SECS` | 마지막 PTY 출력 이후 경과초(1Hz 해상도라 등록 임계값 이상) |
| `Custom` | `custom_event` | `TASTY_HOOK_CUSTOM_EVENT` | 발화된 커스텀 이벤트 식별자 |
| `ProcessExit` / `Bell` / `Notification` | 공통 key 뿐 | — | 이벤트 고유 값 없음 |

- `surface_id` 는 예약 변수 `TASTY_HOOK_SURFACE_ID` 와 이름·값이 그대로 겹친다 — 같은 출처의 중복이라 첫 값이 유지되고 변수가 늘지 않는다(warn 도 내지 않는다). payload 에 담는 이유는 IpcSequence 가 `${body.surface_id}` 로 같은 값을 읽게 하기 위해서다.
- 이벤트별 key 는 위 표가 전부다 — 새 key 를 늘리는 지점도 `trigger_payload` 한 곳이다.
- 위 payload 는 `surface.fire_hook`(IPC 로 이벤트를 직접 발화 — 플러그인이 커스텀 이벤트를 쏘는 경로) 로 발화해도 그대로 적용된다. 같은 `trigger_payload` 를 타므로 `command-completed:1` 발화는 `TASTY_HOOK_EXIT_CODE=1` 을 그대로 내보내고, `TASTY_HOOK_SOURCE` 도 `hook` 이다(`dispatch` 는 `hook_handler.dispatch` 경로 전용).

### Global hook (조건)

surface 무관 — `condition` 으로 트리거:

- `interval:SECS` — 매 N초 반복
- `once:SECS` — N초 후 1회 실행 후 자동 삭제
- `file:/path` — 파일 mtime 변경 감지 시(다른 조건과 동일한 1Hz 폴링 — 별도 watcher
  없음, 파일 저장 즉시가 아니라 최대 1초 지연 후 감지). 등록 시점의 mtime을
  기준선으로 기록하므로 등록 직후엔 발화하지 않는다. 파일이 없는 상태로 등록했다가
  나중에 생기면 그 시점에 발화. 파일이 삭제되면 "변경 없음"으로 취급해 훅이
  자동 삭제되지 않고, 다시 생기면 재감지한다. 파일 하나만 지원 — 디렉토리 경로도
  `metadata`가 mtime을 반환하므로 동작은 하지만 공식 지원 범위 밖이다.

## 인터페이스

- **사용자/AI Agent/CLI**:
  - `hook.set`/`hook.list`/`hook.unset` — `tasty set hook --event bell --command "..." [--once]` 또는 핸들러 참조 `tasty set hook --event bell --handler <id>` (`--command`/`--handler` 택1)
  - `global_hook.set`/`list`/`unset` — `tasty set global-hook --condition interval:60 --command "..." [--label ...]`
  - 표 → [reference/api](../../reference/api.md#기타-호스트)

### 핸들러 레지스트리 (`hook_handler.*`)

전부 **`local_only`** — plugin 은 호출할 수 없다. `IpcSequence` 는 Local 권한으로 실행되므로, plugin 이 시퀀스를 읽거나 고칠 수 있으면 자기 권한 집합을 넘어선 IPC escalation 이 된다(웹훅이 plugin 의 인라인 시퀀스를 거부하는 것과 같은 자리).

| IPC | CLI | 동작 |
|-----|-----|------|
| `hook_handler.list` | `tasty hook-handler list` | 전 출처(host/plugin/user, 비활성 포함) 요약 — action 은 kind 와 `steps` 수만 |
| `hook_handler.get` | `tasty hook-handler get --id <id>` | 단건 상세 — **action 본문까지**. 병합된 유효 핸들러를 준다 |
| `hook_handler.upsert` | `tasty hook-handler upsert --id <id> [--source ...] [--priority N] [--display-name-key K] [--disabled <bool>] (--action <json> \| --calls <json>)` | user 출처 핸들러를 **제자리 수정**하거나 신규 생성 |
| `hook_handler.remove` | `tasty hook-handler remove --id <id>` | user 기여분만 제거(host/plugin 기본값 보존) |
| `hook_handler.reload` | `tasty hook-handler reload` | user config 재로드(host/plugin 영향 없음) |
| `hook_handler.dispatch` | `tasty hook-handler dispatch --id <id> [--body/--header/--query <json>]` | id 로 수동 발화(fire-and-forget) |

- **`upsert` 는 patch 다** — 안 준 필드는 지우는 것이 아니라 그대로 둔다. id·우선순위·나머지가 유지되므로 그 id 를 참조하는 훅 바인딩(`HookBinding::Handler(id)`)은 계속 같은 것을 가리킨다. 지우고 다시 만드는 경로(`remove` 후 재등록)와 **관측 가능하게 다르다**: 후자는 사이에 들어온 트리거가 갈 곳이 없고, host/plugin 기본값이 잠시 드러나며, 안 적은 필드가 기본값으로 되돌아간다.
- **최소 한 필드**는 있어야 한다. 아무 필드도 없는 upsert 는 아무것도 안 고친 채 성공으로 보고되므로 거부한다. 형식이 틀린 `action` 도 같은 이유로 조용히 무시하지 않는다.
- **이미 등록된 웹훅은 안 따라온다.** 웹훅 엔트리는 등록 시점의 `calls` 스냅샷을 직접 소유하고 발화 시 그것을 실행한다 — `--handler <id>` 로 바인딩한 것도 마찬가지다. 바뀐 시퀀스를 외부 URL 에도 적용하려면 그 웹훅을 다시 등록한다. owner 가 등록 시 흐름을 고정한다는 [ADR-0046](../../adr/0046-webhook-owner-trust-one-way-ack.md) 의 모양이다.
- **`remove` 는 user 기여분만** 지운다. host/plugin 이 같은 id 에 기본값을 심어 뒀으면 그것이 다시 드러나므로, 응답의 `still_present` 가 그 사실을 값으로 말한다.
- **병합 순서는 출처 순서다** — 한 id 에 모인 contribution 은 설치 순서와 무관하게 Host → Plugin → User 로 접는다. 원 출처(host 또는 plugin)가 base 가 되고, user 설정은 적은 필드만 그 위에 덮는다. 그래서 `hook-handlers.toml` 로 plugin 핸들러를 patch 하면, 부팅이 user 설정을 plugin 보다 먼저 읽든(headless 는 plugin 을 필요할 때 띄운다) plugin 을 껐다 켜든 reload 없이 user 값이 이긴다. host 와 plugin 은 id 가 `host/<short>` 와 `<plugin_id>/<short>` 로 갈려 한 id 에 함께 오지 않는다([ADR-0430](../../adr/0430-hook-handler-merge-applies-user-patches-last.md)).
- 영속은 `~/.tasty/hook-handlers.toml` atomic write. 쓰기에 실패하면 메모리 레지스트리는 이미 바뀐 상태이며, 그 사실을 오류문에 적고 **성공으로 보고하지 않는다**(다음 부팅에 사라질 변경을 초록으로 덮지 않는다).

## 관련

- **트리거 출처 대칭**: 훅(내부 이벤트)은 웹훅([webhook](../webhook/index.md), 외부 HTTP 트리거)과 대칭인 trigger 출처다. 두 출처는 [공유 훅 핸들러 레지스트리(ADR-0047)](../../adr/0047-shared-hook-handler-registry-source-gate.md)를 공유한다 — `source: hook|webhook|any` 게이트로 셸 action 은 `hook` 출처 전용이다. 훅은 위 "바인딩" 절대로 `HookBinding::Handler(id)` 로 레지스트리 핸들러를 참조해 소비하며, 인라인 셸(`--command`)은 하위호환 익명 경로다.
- [agent-collaboration](../agent-collaboration/index.md) · [notifications](../notifications/index.md) · [claude plugin](../../plugins/claude/index.md)(Claude hook 발화)
