# 훅 (Surface / Global hooks)

## 원칙과 현재 동작

[피드백·실패 원칙](../../principles/feedback-and-recovery.md)에 따라 결과와 실패가 후속 진행에 미치는 영향을 전달해야 한다. 아래 훅 접수/HookFired와 완료는 다르며, 자동 실행(surface 훅·전역 훅)이 실패하면 알림으로 남긴다. 수동 dispatch는 접수 응답의 `run` 번호로 최종 결과(`hook_handler.result`)를 돌려준다. 의존 단계도 무조건 계속하는 경로는 현행 구현의 미충족이다.

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`hook.*` / `global_hook.*`)
- **설계**: 없음
- **코드**: `tasty-hooks` 크레이트(`HookManager`/`HookEvent`/`HookBinding`), `hook.*`·`global_hook.*` 핸들러, 발화 판정·바인딩 실행·`HookFired` 생성 `HookRuntimeState::fire`(`src/hook_runtime/mod.rs`), 바인딩 실행 연결 `src/hook_runtime/trigger.rs`, 전역 훅 감시·셸 실행 `src/hook_runtime/global.rs`. IdleTimeout 판정: `HookRuntimeState::fire_idle_timeouts` + `src/hook_runtime/engine.rs`(터미널 출력 시각 제공), 전달: `src/app/idle_hooks.rs`(GUI 창과 parked engine)/`src/boot.rs`(headless). OutputMatch 라인 버퍼 공유: `src/runtime/output_observer.rs::ObserverRouter::dispatch_text`
- **화면**: 없음

## 목적

특정 이벤트 발생 시 동작을 자동 실행하는 훅. surface 별 이벤트 훅(`hook.*`)과 surface 무관 글로벌 훅(`global_hook.*`)이 있다. "에이전트가 에이전트를 제어하는 자동화" 의 토대(conductor 가 polling 없이 자식 완료를 감지).

surface hook 은 셸 명령 문자열 대신 **공유 훅 핸들러 레지스트리**(`src/hook_handler/`)의 핸들러를 참조한다 — 웹훅과 훅이 같은 핸들러 정의를 공유하는 구조다. 기존 `--command` 인라인 셸은 하위호환을 위해 **익명 hook 핸들러**로 감싸 그대로 실행된다.

## 내부 동작

### Surface hook (`HookEvent`)

`HookManager` 가 등록/삭제/조회/실행 관리. 이벤트 타입:

| 이벤트 | 발생 조건 |
|--------|------|
| `ProcessExit` | 셸 프로세스 종료 |
| `OutputMatch(pattern)` | PTY 출력이 정규식 매칭(등록 시 사전 컴파일) — **완성된 라인 단위로만** 매칭 |
| `Bell` | BEL 수신 |
| `Notification` | OSC 알림 수신 |
| `IdleTimeout(secs)` | N초간 PTY 출력 없음 — **1Hz 해상도**(`Tick::Busy`) |
| `CommandCompleted(Option<i32>)` | OSC 133 D phase — 셸 통합이 개별 명령(`docker build`, `just run` 등)의 종료 + exit code 를 보고 |
| `Custom(string)` | 코어가 모르는 임의 이벤트 식별자. 정확 문자열 일치로 매칭. 플러그인 소유 이벤트(예: claude plugin 이 fire 하는 `claude-idle` / `needs-input` / `claude-stop-failure` / `claude-error`)는 모두 이 변형으로 처리된다 — 코어에 에이전트 고유 이벤트명을 박지 않는다. |

#### OutputMatch — 완성된 라인 단위 매칭

`OutputMatch` 는 PTY 출력을 별도로 누적하지 않고 `ObserverRouter::dispatch_text`(`output.observe` 옵저버가 쓰는 것과 **동일한** per-surface `LineBuffer`)를 공유한다. `dispatch_text` 가 그 청크에서 완성된(`\n` 로 끝난) 줄만 반환하고, 그 줄들에 대해서만 정규식 매칭을 시도한다.

- 줄바꿈 없이 끝나는 청크(예: 프롬프트 대기 중 부분 출력)는 매칭 대상이 아니다 — 다음 청크가 도착해 줄이 완성돼야 매칭된다. 패턴이 청크 경계에 걸쳐 있어도(`"partial ERR"` + `"OR\n"` → `"partial ERROR"`) 완성 시점에 합쳐진 전체 줄로 매칭된다.
- 옵저버(`output.observe`)가 하나도 등록되지 않은 surface 도 OutputMatch 훅만으로 라인 버퍼 게이트가 열린다(`has_output_match_hook`) — 옵저버 등록이 OutputMatch 동작의 전제조건이 아니다.
- PTY emit 게이트(`sync_output_event_gates`)는 `hook.set`/`hook.unset` 처리 시점에 **즉시(eager)** 동기화된다(`Core::register_surface_hook`/`unregister_surface_hook`) — `observer_register`/`observer_unregister` 와 동일 패턴. VTE 파싱은 전용 parser thread(docs/surfaces/terminal/index.md)가 PTY 바이트 도착 즉시 처리하므로, 게이트를 다음 `process_surface` 호출까지 지연시키면 등록 직후 도착하는 매칭 출력이 게이트 OFF 상태로 파싱되어 이벤트가 유실된다.

#### IdleTimeout — 1Hz 폴링 + epoch 기반 anti-spam

`IdleTimeout` 은 별도 타이머/watcher 가 아니라 기존 `Tick::Busy`(1Hz) tick 에 얹혀 동작한다. tick 마다 `Terminal::last_output_at()` 로 마지막 출력 시각과의 경과초를 계산해 임계값과 비교한다.

- 최대 1 초 지연: PTY 출력 정지 시점과 훅 발생 시점 사이에 최대 1 초의 오차가 있다(Global hook 의 `file:` 조건과 동일한 해상도).
- **epoch 기반 anti-spam**: 한 번 발생하면 그 시점의 `last_output_at` 값(epoch)을 기록해, 같은 epoch 동안은 재발생하지 않는다(persistent 훅이 매 tick 마다 스팸처럼 재발생하는 것을 막음). 새 출력이 들어와 `last_output_at` 이 갱신되면 다시 알릴 수 있다.
- `once` 훅은 발생 후 즉시 제거된다(Global hook 의 `once:SECS` 와 동일한 동작).

#### CommandCompleted — OSC 133 명령 완료(exit code)

셸 프로세스 자체의 종료(`ProcessExit`)와 달리, 셸 *안에서* 실행되는 개별 명령(`docker build`, `just run build` 등)의 완료를 감지한다 — 서브프로세스 종료는 `process-exit` 으로 원리적으로 감지 불가능하다(portable-pty 의 `Child` 추상화가 단일 pid 만 wait). OSC 133 셸 통합(zsh/bash preexec 등)이 D phase(`\e]133;D;<exit_code>\a`)를 보내면 [`command_index`](../terminal-output/index.md)가 이미 인덱싱하는 것과 별개로, 이 훅이 항상(exit code 필터링 없이) 발생한다. 단, 같은 PTY 연결에서 프롬프트(A phase)를 그리기 전의 D 는 발생시키지 않는다 — bash·zsh 통합의 precmd 는 첫 프롬프트 전에도 `D;0` 을 보내므로(`D;0` → `A`) 그 D 는 셸 시작 보고다. respawn 은 새 연결이라 새 셸의 시작 D 도 같다.

- **등록**: `command-completed` = 임의 exit code 매치. `command-completed:<N>` = 그 exit code 만 매치(예: `command-completed:1` 로 실패한 명령만 구독). 실제 발생 이벤트는 항상 특정 exit code 를 담으므로, `None` 등록만 모든 발생과 매치되고 `Some(n)` 등록은 그 값과 일치할 때만 매치된다.
- **전제 조건**: 셸이 OSC 133 셸 통합 스크립트를 로드해야 한다. 미설치 셸은 D phase 자체가 안 와 이 훅이 절대 발생하지 않는다 — surface 가 출력을 내는데도 일정 시간(10 초) 지나도록 OSC 133 boundary 를 한 번도 못 받으면 "셸 통합 미설치" 안내 배너(`shell-integration-missing`, 마우스 캡처 안내 배너와 동일한 형태 — 자동 조치 없이 설명만)를 surface 스코프로 1 회 띄운다. 판정 대상은 로컬 PTY 가 있는 셸 surface 이며, 훅·observer 등록과 무관하게 판정한다. 원격 attach mirror 는 첫 내용인 스냅샷에 OSC 133 이 없고 안내가 권하는 로컬 셸 통합 설치도 해당하지 않으므로 띄우지 않는다. 이 배너는 동작이 없어 10 초 뒤 스스로 닫힌다(hover 중이거나 surface 가 화면에 없으면 멈춘다).
- **surface attention 자동 연결**: 이 훅 발생과 별개로, cascade(`cascade_terminal_command_completed`)가 exit code 무관하게 항상 `raise_attention`(kind=Completion) 도 함께 호출한다(셸 시작 D 는 위 규칙대로 이 cascade 에 오지 않는다)(설정 없이 즉시 동작하는 자동 경로) — [surface-highlight](../surface-highlight/index.md) 참고. 이 훅(커스터마이즈 경로)은 그와 독립적으로 동작한다 — 예를 들어 실패한 명령만 알림음을 울리고 싶으면 `command-completed:1` 로 별도 바인딩을 추가로 걸 수 있다(자동 attention 을 대체하는 게 아니라 그 위에 얹는 것).
- **구현 메모**: termwiz 는 OSC 133("133")을 미리 알려진 코드로 인식해 `Unspecified` 가 아니라 전용 `FinalTermSemanticPrompt` variant 로 구조화해 반환한다(A/C/D 는 항상 이 경로 — B 만 셸이 `cmd=` 등 부가 토큰을 붙이면 termwiz 의 엄격 파서가 실패해 `Unspecified` 로 폴백). `crates/tasty-terminal/src/vte_handler/osc.rs`가 이 variant 를 tasty 공통 `PromptBoundary{phase, payload}` 로 평평하게 변환해 이후 로직(command_index/이 훅)이 phase 문자만 보고 동작한다.

#### 이벤트 키 검증 (내장 + 플러그인 선언)

`HookEvent::parse` 는 미인식 문자열을 `Custom(String)` 으로 무조건 수용하므로(파싱·검증 책임 분리), `hook.set` / `surface.fire_hook` 핸들러 단계에서 키를 **(내장 ∪ 활성 플러그인 선언)** 집합으로 검증한다.

- **내장 이벤트**(`process-exit` / `bell` / `notification` / `output-match:` / `idle-timeout:` / `command-completed` / `command-completed:<N>`)는 플러그인 무관하게 항상 허용.
- **플러그인 선언 이벤트**는 manifest의 `[[contributes.hook_events]]`에서 선언한다.
  호스트는 활성 plugin의 hello에서 목록을 모으고 언로드·제거 시 지운다. disable,
  graceful swap, 자동 재시작, 연결 실패 정리는 모두 `PluginManager::forget_plugin_runtime`
  (`crates/tasty-host-plugin/src/manager/lifecycle.rs`)을 거쳐야 한다. 이 함수가
  `registered_plugins` 기록까지 지워야 다음 hello가 `finalize_plugin_hello`를 거쳐
  이벤트 목록을 다시 등록한다. 기록이 남으면 재시작한 plugin의 hello가 무시될 수 있다.
- 내장도 아니고 활성 플러그인이 선언하지도 않은 키(오타·미존재 이벤트)는 **등록 거부**(`invalid_params`, 에러 메시지에 내장 + 활성 선언 목록 안내). 실행될 수 없는 훅 등록을 막는다.
- 따라서 **플러그인이 비활성이면 그 플러그인의 이벤트 hook 등록도 거부**된다(예: claude plugin 비활성 시 `claude-idle` hook 등록 불가 — 동작하지 않는 설정 방지). claude plugin 이 선언하는 키는 `crates/tasty-plugin-claude/tasty-plugin.toml` 의 `[[contributes.hook_events]]` 가 정본이다.

- **once**는 첫 매칭 이벤트에서 한 번 실행한 뒤 등록을 제거한다. 한 번의 판정에 이벤트가 여러 개 들어와도 같은 once 등록을 다시 실행하지 않는다. 기본값인 persistent는 맞는 이벤트마다 실행한다. 검증 방법은 [guard-verification](../../dev-guide/guard-verification.md)을 따른다.
- **비동기 실행**: 훅 동작은 백그라운드에서(메인 루프 블로킹 없음 — 셸은 자식 프로세스 스레드, `IpcSequence` 는 아래 "바인딩" 절의 실행기 스레드). 각 이벤트의 발생 surface ID 를 추적해 올바른 surface 에서 실행.
- ProcessExit은 GUI/headless 모두에서 surface 자동 닫기까지 수행한다(surface→tab→pane→workspace 계층 정리, 마지막이면 새 셸 spawn). 두 빌드 모두 공용 PTY 종료 처리(`src/app/process_exit.rs`)에서 종료 hook의 binding 실행을 시작하고 `HookFired`를 쌓은 뒤 surface를 닫는다.
- 자연 발생 이벤트는 모두 `HookRuntimeState::fire` 한 경로로 발화해 `HookFired`를 쌓는다. GUI는 process-exit · bell · notification · output-match · idle-timeout · command-completed를, headless는 그중 process-exit · output-match · idle-timeout을 관측한다. `HookFired`는 훅이 발화했다는 뜻이며 바인딩의 셸 작업이 끝났다는 뜻이 아니다.
- surface가 닫히면 그 surface의 once·persistent hook 등록도 제거한다. 이미 발생해 복사한 binding은 실행을 마치며, 다른 surface의 hook은 유지한다.

#### 바인딩 (핸들러 참조 vs 인라인 셸)

surface hook 은 `HookBinding` 으로 무엇을 실행할지 표현한다:

- **`Handler(id)`** — 공유 훅 핸들러 레지스트리 핸들러 id 참조(`tasty set hook --handler <id>`). 등록 시 핸들러가 존재하고 `source` 가 hook 트리거를 수용(`hook` 또는 `any`)하는지 검증한다 — `webhook` 전용 핸들러는 거부된다.
- **`InlineShell(cmd)`** — 하위호환 익명 셸(`tasty set hook --command "..."`). 레지스트리에 등록되지 않는 인라인 핸들러라 export/영속화 대상이 아니다.

`tasty-hooks`는 surface와 이벤트를 매칭해 `FiredHook`을 반환한다. 레지스트리 조회와 실제 실행은
`hook_runtime::trigger::execute_binding`이 담당한다. 핸들러 정의 레지스트리는 엔진이 공유하고(`src/hook_handler/`),
훅 등록·감시 상태는 `EngineSession.hooks`의 엔진별 `HookRuntimeState`가, 실행은 `src/hook_runtime/`이 맡는다. CoreState에는 훅 상태 원본을 두지 않는다.

IpcSequence는 호스트 명령 큐를 처리하는 스레드에서 기다리지 않는다.
지연 생성한 `hook-sequence` 워커 하나(`src/hook_runtime/worker.rs`)가 surface hook과 수동 `hook_handler.dispatch`를 접수 순서대로 실행한다.
한 시퀀스의 step이 응답하거나 대기가 끝나면 다음 step으로 넘어가며 오류를 기록하고 계속한다.
webhook은 요청별 스레드에서 실행하므로 이 순서에 포함되지 않는다.

대기 시퀀스 수는 `PENDING_SEQUENCE_LIMIT`로 제한하며 현재 `INJECTED_DEPTH_LIMIT`의 256 건을 사용한다.
포화·워커 시작 실패·중단이면 실행하지 않고 로그를 남긴다. 메인 스레드에서 대신 실행하거나 자동 재시도하지 않는다.
수동 dispatch는 이 경우 `-32603`으로 거절하고, 접수되면 accepted 응답을 즉시 보낸다.
`HookFired`와 accepted는 step 완료를 뜻하지 않는다. 수동 dispatch의 완료는 아래 [수동 dispatch의 최종 결과](#수동-dispatch의-최종-결과)로 확인한다. 스텝의 10 초 대기 종료 뒤에도 이미 주입된 요청은 실행될 수 있다.
느린 시퀀스가 뒤 작업을 지연시키며 수동 요청과 surface hook은 같은 상한을 사용한다.

로그의 출처는 `SequenceOrigin`에 따라 `webhook`, `surface hook`, `hook_handler.dispatch`로 구분한다.
로컬 실행에 필요한 injector가 없으면 warning을 남기고 실행을 건너뛴다.

#### 수동 dispatch의 최종 결과

`hook_handler.dispatch` 응답은 접수 확인이다. `accepted: true`의 의미는 그대로이며 `run`(실행 번호)이 추가됐다. 호출자는 `hook_handler.result {run}`으로 진행·최종 결과를 조회한다(`src/hook_runtime/runs.rs`).

- 응답: `{run, id, action, done, state, result, steps?, exit_code?}`. `done`이 false면 `state: "running"`이다.
- `state`: `IpcSequence`는 `SequenceOutcome::verdict`로 정한다. 모든 step이 `ok`면 `succeeded`, 성공한 step이 없으면 `failed`, 일부만 성공하면 `partial`, 실패는 없지만 결과 불명 step이 있으면 `unknown`이다. 결과 불명은 성공으로 보고하지 않는다.
- `result`: 표시 언어와 관계없는 영어 한 줄. 예: `1 of 2 steps succeeded; step 2 (nosuch.method) failed: rpc error -32601: ...`. `steps`는 step별 `{step, method, status(ok|failed|not_run|unknown), error}`다.
- `ShellCommand`는 프로세스를 띄웠으면 `succeeded`("process started"), 띄우지 못했으면 `failed`("process could not start: <원인>")다. 종료 코드는 끝난 뒤 `exit_code`와 `result`에 정보로 실으며 판정에 쓰지 않는다.
- 실행 기록은 `file_handler.dispatch`·`markdown.navigate`와 같은 저장소(`src/request_runs.rs`)에 있으며, 합쳐서 최근 128건(`request_runs::KEEP`)만 보관한다. 그보다 오래되었거나 알 수 없는 `run`은 `-32602`다. 큐 포화·실행기 부재로 동기 거절된 dispatch는 기록을 남기지 않는다.
- CLI `tasty hook-handler dispatch`는 접수 줄을 출력한 뒤 200ms 간격으로 조회해 최종 결과 줄을 출력한다. 기다리는 상한은 10초 × (step 수 + 1)이다. 최종 상태가 `succeeded`가 아니거나 상한 안에 끝나지 않으면 종료 코드 1이다. 결과 조회 응답도 남은 시간까지만 읽는다(소켓 읽기 timeout). 상한 안에 응답이 오지 않으면 `{run, done: false, state: "running", timed_out: true, result: "no answer to hook_handler.result within <N> ms; the final result is unknown"}`를 마지막 줄로 출력하고 종료 코드 1로 끝난다. `--no-wait`는 접수 줄만 출력하고, 결과는 `tasty hook-handler result --run <N>`으로 조회한다.
- 기다리는 dispatch는 요청을 여러 번 보내는 client-driven 명령(`crates/tasty-cli/src/dispatch.rs`의 `ClientDriven`)이다. 요청 하나에 싣는 루트 옵션 `--response-timeout-ms`를 주면 `Envelope::refuse_if_set`이 아무것도 보내지 않고 종료 코드 2로 끝낸다. `--no-wait`는 요청 하나라 일반 경로로 보내고 이 옵션을 그 요청에 싣는다. CLI에는 멱등 키 옵션이 없으며, 결과 조회 요청은 항상 멱등 키 없이 보낸다(`hook_handler.result`는 `Read`라 키를 보존하지 않는다).
- 결과 조회 요청은 접수 요청을 복제해 만들므로 셸 환경의 `TASTY_SESSION_TOKEN`(세션 토큰)과 `TASTY_AGENT_ID`(호출자 agent ID)를 그대로 싣는다.
- 수동 dispatch의 결과는 호출자에게만 돌려주며 알림을 남기지 않는다. 자동 실행의 실패만 아래 알림으로 남긴다.

#### 자동 실행 실패 알림

`execute_sequence`는 스텝마다 `ok`·`failed`(실행되어 오류)·`not_run`(실행 안 됨)·`unknown`(응답 대기 종료로 결과 불명)을 담은 `SequenceOutcome`을 돌려준다(`src/hook_handler/outcome.rs`). 자동 실행 경로는 성공하지 못한 결과를 `src/hook_runtime/notice.rs`로 알림 채널에 남긴다. 수동 dispatch는 알림을 남기지 않는다.

| 실패 지점 | 레벨 | 대상 | source |
|---|---|---|---|
| surface 훅: 핸들러 없음·훅 트리거 거절·injector 없음·실행기 대기열 거절·셸을 띄우지 못함 | 오류(`hook_notice.hook_failed`) | 발생 surface와 그 workspace | `hook` |
| surface 훅 IpcSequence: 성공한 스텝 없음 | 오류(`hook_notice.hook_failed`) | 발생 surface | `hook` |
| surface 훅 IpcSequence: 일부 스텝 실패 / 결과 불명 스텝 | 경고(`hook_notice.hook_partial` / `hook_unknown`) | 발생 surface | `hook` |
| 전역 훅 명령을 띄우지 못함 | 오류(`hook_notice.global_failed`) | 활성 workspace, `surface_id: 0` | `global_hook` |

- 레벨 기준은 [실패 뒤의 처리](../../principles/feedback-and-recovery.md#실패-뒤의-처리)를 따른다. 자동화가 전혀 실행되지 않았으면 오류, 일부만 적용됐거나 결과를 모르면 경고다.
- 발생 surface가 알림 전에 닫혔으면 활성 workspace가 `surface_id: 0`으로 받는다.
- 셸 명령의 0이 아닌 종료 코드는 실패로 판정하지 않는다. surface 훅의 `ShellCommand`·inline 명령과 전역 훅은 `sh -c`/`cmd /C`를 거치므로 없는 명령은 셸 종료 코드로 끝나며 알림 대상이 아니다.
- 반복 실패는 같은 source·레벨·제목·본문을 10초에 한 번만 남긴다. 서로 다른 실패는 저장소의 500ms 합치기를 거치지 않고 각각 남는다([알림](../notifications/index.md#작업-스레드의-시스템-알림)).
- 웹훅 실행 실패는 [웹훅](../webhook/index.md#실행-실패-알림)에 있다.

#### 셸 핸들러 환경변수 (`TASTY_HOOK_*`)

`ShellCommand` 핸들러(인라인 셸 포함)가 spawn 하는 자식 프로세스에는 트리거 컨텍스트가 환경변수로 노출된다 — IpcSequence 가 `${body.*}` 값슬롯 치환으로 받는 값을 셸은 env 로 받는다(의미 대칭). 조립은 `src/hook_handler/env.rs` (순수 함수).

자식 프로세스는 호스트의 stdin·stdout·stderr를 물려받지 않는다. 수동 dispatch의 직접 실행(`hook_handler::exec::spawn_shell`)은 세 통로를 모두 `null`로 두고, surface 훅의 `sh -c` 실행은 출력을 모아 버린다. 결과가 필요하면 셸 안에서 파일로 돌리거나 `tasty notify` 등으로 보낸다.

| 변수 | 값 |
|------|-----|
| `TASTY_HOOK_EVENT` | 훅 트리거: 등록 이벤트 표시 문자열(`bell` / `process-exit` / `output-match:<pattern>` / 플러그인 커스텀 키 등). `hook_handler.dispatch` 수동 실행: 핸들러 id |
| `TASTY_HOOK_SOURCE` | `hook`(내부 이벤트 트리거) 또는 `dispatch`(`hook_handler.dispatch` 수동 실행). 셸은 webhook 바인딩이 구조적으로 불가하므로 `webhook` 값은 존재하지 않는다 |
| `TASTY_HOOK_SURFACE_ID` | 훅 이벤트가 발생한 surface ID. 수동 실행(surface 무관)에는 설정되지 않음 |
| `TASTY_HOOK_<UPPER_SNAKE_KEY>` | payload 가 object 면 최상위 key 각각. 훅 트리거의 payload 는 아래 [트리거 payload](#트리거-payload-이벤트별-key) 가 이벤트별로 채우고, `hook_handler.dispatch` 수동 실행은 params 의 `body` 를 그대로 쓴다 |

- **key 정규화**: ASCII 영숫자는 대문자로, 그 외 문자는 `_` 로. 정규화 결과가 겹치거나 위 예약 변수와 겹치면 **먼저 온 값을 사용하고** 나머지는 무시한다. payload 안에서 서로 다른 원본 key 가 정규화 후 충돌하는 경우(예: `pr-id` vs `pr_id`)만 warn 하고, 예약 변수와의 충돌은 조용히 무시한다(`surface_id` 처럼 이벤트가 발생할 때마다 생기는 정상 경로). 영숫자가 없는 key 는 건너뜀.
- **값**: 문자열은 그대로, 그 외 JSON 은 compact 표현. NUL 문자는 제거(플랫폼 env 제약), 값당 4096 바이트 초과분은 절단(Windows env 블록 상한 보호).
- **상속 env 제거**: 셸 명령(`ShellCommand`)·직접 실행 명령·surface 훅 명령 모두 Tasty 프로세스에서 상속한 `CMUX_*` 와 설치·활성 플러그인이 `contributes.strip_inherited_env` 로 선언한 제품 세션 변수(예: Claude 플러그인의 `CLAUDECODE`·`CLAUDE_CODE_MESSAGING_TOKEN`, Codex 플러그인의 `CODEX_THREAD_ID`)를 지운다. 바깥 Tasty 인스턴스에서 상속한 신원(`TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`·`TASTY_AGENT_ID`)도 지워, 훅 명령의 `tasty` 가 이 인스턴스가 모르는 토큰이나 바깥 surface 를 싣지 않는다. 훅은 자기 surface 를 `TASTY_HOOK_SURFACE_ID` 로 받는다. 플러그인이 선언하지 않은 사용자 설정(`CLAUDE_CODE_GIT_BASH_PATH` 등)은 남는다. 터미널 셸과 같은 판정이다([터미널 자식 셸 환경변수](../../surfaces/terminal/index.md#pty-셸)).
- **데이터/흐름 분리**: env 는 값 전달 전용 — 실행할 명령(command/args)은 레지스트리 owner 가 고정하므로 payload 가 실행 대상을 바꿀 수 없다.

참조 대상 핸들러 레지스트리는 [Settings › Handler › Hook Handlers](../settings/screens/settings.md) 서브탭에서도 조회·편집할 수 있다(user 매핑은 `~/.tasty/hook-handlers.toml` 영속). 시퀀스(`IpcSequence`) 본문은 그 서브탭의 인라인 문자열 편집기(한 줄에 호출 하나 — `hook_handler::sequence_text`)로 고친다. host·plugin 핸들러를 고치면 같은 id 의 user 패치가 되어 이후 기본값 변경을 따라가지 않으며, 되돌리려면 `tasty hook-handler remove --id <id>` 로 그 패치를 지운다. 그 형식으로 쓸 수 없는 method 가 든 시퀀스는 아래 [핸들러 레지스트리 CLI](#핸들러-레지스트리-hook_handler)로 고친다.

##### 트리거 payload (이벤트별 key)

훅 트리거의 payload 는 `src/hook_runtime/trigger.rs` 의 `trigger_payload` 가 조립한다 — 셸 env(`TASTY_HOOK_*`)와 IpcSequence 값슬롯(`${body.*}`)이 같은 소스에서 파생되는 단일 지점이다. 값은 **등록 패턴이 아니라 실제 관측된 이벤트**에서 채운다(예: `output-match:ERR.*` 로 등록해도 `matched_text` 는 실제로 매칭된 줄).

| 이벤트 | payload key | 셸 env | 값 |
|--------|-------------|--------|-----|
| (전 이벤트 공통) | `surface_id` | `TASTY_HOOK_SURFACE_ID` | 발생한 surface id |
| `OutputMatch` | `matched_text` | `TASTY_HOOK_MATCHED_TEXT` | 매칭된 **완성 라인 전문**(정규식이 소비한 부분만이 아니다) |
| `CommandCompleted` | `exit_code` | `TASTY_HOOK_EXIT_CODE` | OSC 133 D phase 가 보고한 관측 exit code. D phase 가 코드를 안 실었거나 정수로 파싱되지 않으면 payload 는 JSON `null` 이고 셸은 문자열 `null` 을 받는다(`command-completed` 와일드카드 등록이 이 경우와도 매치된다) |
| `IdleTimeout` | `idle_elapsed_secs` | `TASTY_HOOK_IDLE_ELAPSED_SECS` | 마지막 PTY 출력 이후 경과초(1Hz 해상도라 등록 임계값 이상) |
| `Custom` | `custom_event` | `TASTY_HOOK_CUSTOM_EVENT` | 발생한 커스텀 이벤트 식별자 |
| `ProcessExit` / `Bell` / `Notification` | 공통 key 뿐 | — | 이벤트 고유 값 없음 |

- `surface_id` 는 예약 변수 `TASTY_HOOK_SURFACE_ID` 와 이름·값이 그대로 겹친다 — 같은 출처의 중복이라 첫 값이 유지되고 변수가 늘지 않는다(warn 도 내지 않는다). payload 에 담는 이유는 IpcSequence 가 `${body.surface_id}` 로 같은 값을 읽게 하기 위해서다.
- 이벤트별 key 는 위 표가 전부다 — 새 key 를 늘리는 지점도 `trigger_payload` 한 곳이다.
- 위 payload 는 `surface.fire_hook`(IPC 로 이벤트를 직접 발생시킴 — 플러그인이 커스텀 이벤트를 발생시키는 경로) 로 발생시켜도 그대로 적용된다. 같은 `trigger_payload` 를 타므로 `command-completed:1` 발생은 `TASTY_HOOK_EXIT_CODE=1` 을 그대로 내보내고, `TASTY_HOOK_SOURCE` 도 `hook` 이다(`dispatch` 는 `hook_handler.dispatch` 경로 전용).

### Global hook (조건)

surface 무관 — `condition` 으로 트리거:

- `interval:SECS` — 매 N초 반복
- `once:SECS` — N초 후 1 회 실행 후 자동 삭제
- `file:/path` — 파일 mtime 변경 감지 시(다른 조건과 동일한 1Hz 폴링 — 별도 watcher
  없음, 파일 저장 즉시가 아니라 최대 1 초 지연 후 감지). 등록 시점의 mtime을
  기준선으로 기록하므로 등록 직후엔 발생하지 않는다. 파일이 없는 상태로 등록했다가
  나중에 생기면 그 시점에 발생. 파일이 삭제되면 "변경 없음"으로 취급해 훅이
  자동 삭제되지 않고, 다시 생기면 재감지한다. 파일 하나만 지원 — 디렉토리 경로도
  `metadata`가 mtime을 반환하므로 동작은 하지만 공식 지원 범위 밖이다.

전역 훅 명령(`sh -c` / `cmd /C`)도 surface 훅과 같은 판정으로 상속한 `CMUX_*` 와 활성 플러그인이 선언한 제품 세션 변수, 바깥 인스턴스의 신원 변수(`TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`·`TASTY_AGENT_ID`)를 지운다([터미널 자식 셸 환경변수](../../surfaces/terminal/index.md#pty-셸)).

## 인터페이스

- **사용자/AI Agent/CLI**:
  - `hook.set`/`hook.list`/`hook.unset` — `tasty set hook --event bell --command "..." [--once]` 또는 핸들러 참조 `tasty set hook --event bell --handler <id>` (`--command`/`--handler` 택 1)
  - `global_hook.set`/`list`/`unset` — `tasty set global-hook --condition interval:60 --command "..." [--label ...]`
  - 표 → [reference/api](../../reference/api.md#기타-호스트)

### 핸들러 레지스트리 (`hook_handler.*`)

등록할 때 disabled 상태와 `source: hook|webhook|any`를 확인한다. source가 맞지 않으면 거절한다.
웹훅에는 IpcSequence만 바인딩할 수 있다. Host·User의 ShellCommand도 source=hook만 허용하며,
Plugin 선언에는 ShellCommand variant가 없어 매니페스트 파싱 단계에서 거절된다.
레지스트리는 현재 process singleton으로 IPC와 listener가 공유한다.
같은 ID의 기여는 Host→Plugin→User 순서로 병합하고 제공된 필드만 덮는다.
조회 목록은 priority 오름차순, 같은 priority에서는 User·Plugin·Host 순서, 마지막으로 ID로 정렬한다.

전부 **`local_only`** — plugin 은 호출할 수 없다. `IpcSequence` 는 Local 권한으로 실행되므로, plugin 이 시퀀스를 읽거나 고칠 수 있으면 자기 권한 집합을 넘어선 IPC escalation 이 된다(웹훅이 plugin 의 인라인 시퀀스를 거부하는 것과 같은 자리).

| IPC | CLI | 동작 |
|-----|-----|------|
| `hook_handler.list` | `tasty hook-handler list` | 전 출처(host/plugin/user, 비활성 포함) 요약 — action 은 kind 와 `steps` 수만 |
| `hook_handler.get` | `tasty hook-handler get --id <id>` | 단건 상세 — **action 본문까지**. 병합된 유효 핸들러를 준다 |
| `hook_handler.upsert` | `tasty hook-handler upsert --id <id> [--source ...] [--priority N] [--display-name-key K] [--disabled <bool>] (--action <json> \| --calls <json>)` | user 출처 핸들러를 **제자리 수정**하거나 신규 생성 |
| `hook_handler.remove` | `tasty hook-handler remove --id <id>` | user 기여분만 제거(host/plugin 기본값 보존) |
| `hook_handler.reload` | `tasty hook-handler reload` | user config 재로드(host/plugin 영향 없음) |
| `hook_handler.dispatch` | `tasty hook-handler dispatch --id <id> [--body/--header/--query <json>] [--no-wait]` | id 로 수동 실행. 응답은 접수(`accepted`, `run`)이고 CLI 는 최종 결과까지 기다린다. `IpcSequence` 는 surface 훅과 같은 실행기에 발생 순서대로 줄 선다 |
| `hook_handler.result` | `tasty hook-handler result --run <N>` | dispatch 의 진행·최종 결과 조회. [수동 dispatch의 최종 결과](#수동-dispatch의-최종-결과) |

- **`upsert` 는 patch 다** — 안 준 필드는 지우는 것이 아니라 그대로 둔다. id·우선순위·나머지가 유지되므로 그 id 를 참조하는 훅 바인딩(`HookBinding::Handler(id)`)은 계속 같은 것을 가리킨다. 지우고 다시 만드는 경로(`remove` 후 재등록)와 **관측 가능하게 다르다**: 후자는 사이에 들어온 트리거가 갈 곳이 없고, host/plugin 기본값이 잠시 드러나며, 안 적은 필드가 기본값으로 되돌아간다.
- **최소 한 필드**는 있어야 한다. 아무 필드도 없는 upsert 는 아무것도 안 고친 채 성공으로 보고되므로 거부한다. 형식이 틀린 `action` 도 같은 이유로 조용히 무시하지 않는다.
- **이미 등록된 웹훅은 안 따라온다.** 웹훅 엔트리는 등록 시점의 `calls` 스냅샷을 직접 소유하고 발생 시 그것을 실행한다 — `--handler <id>` 로 바인딩한 것도 마찬가지다. 바뀐 시퀀스를 외부 URL 에도 적용하려면 그 웹훅을 다시 등록한다. owner 가 등록 시 흐름을 고정한다는 [웹훅 (Inbound webhook listener)](../webhook/index.md) 의 규칙이다.
- **`remove` 는 user 기여분만** 지운다. host/plugin 이 같은 id 에 기본값을 심어 뒀으면 그것이 다시 드러나므로, 응답의 `still_present` 가 기본값이 남아 있는지를 알려준다.
- **병합 순서는 출처 순서다** — 한 id 에 모인 contribution 은 설치 순서와 무관하게 Host → Plugin → User 순서로 병합한다. 원 출처(host 또는 plugin)가 base 가 되고, user 설정은 적은 필드만 그 위에 덮는다. 그래서 `hook-handlers.toml` 로 plugin 핸들러를 patch 하면, 부팅이 user 설정을 plugin 보다 먼저 읽든(headless 는 plugin 을 필요할 때 띄운다) plugin 을 껐다 켜든 reload 없이 user 값을 우선한다. host 와 plugin 은 id 가 `host/<short>` 와 `<plugin_id>/<short>` 로 갈려 한 id 에 함께 오지 않는다(이 문서).
- **TOML에는 null이 없다.** `IpcSequence` 호출의 params 자체가 null이면 `params` 키를 쓰지 않고, 읽을 때 null로 채운다. params 안쪽에 null이 있으면(예: `{"a":null}`) 그 호출만 params 전체를 JSON 문자열 키 `params_json`으로 쓰고 읽을 때 되돌린다. null이 없는 params는 TOML 표 `params`로 쓰므로 이 규칙 이전의 파일도 그대로 읽힌다. 한 호출에 `params`와 `params_json`이 함께 있거나 `params_json`이 JSON이 아니면 파일 전체를 파싱 실패로 보고 이전 사용자 설정을 유지한다. null 키를 지우지 않는 것은 받는 IPC 메서드가 키의 존재 여부로 동작을 가를 수 있어서다. IPC 계약은 바뀌지 않는다(IPC 요청은 JSON이라 null을 그대로 받는다).
  - 한 파일 안에 params 표기가 두 가지가 되고, `params_json`은 JSON 문자열이라 TOML 편집기의 구조 표시와 검증을 받지 못한다. `params_json` 오류의 위치는 그 호출을 담은 action 표다(호출이 인라인 배열 안에 있다).
  - 파일 전체를 JSON으로 바꾸는 안은 기존 TOML 파일의 이관과 두 형식의 공존 기간이 필요하고 다른 사용자 설정(config.toml·preset)과 표기가 갈라져 택하지 않았다. 모든 params를 JSON 문자열로 쓰는 안은 손으로 쓴 파일의 모양이 바뀌고 TOML 편집의 이점을 모든 호출에서 잃는다.
- 영속은 `~/.tasty/hook-handlers.toml`의 불변 snapshot을 file writer에 제출한 뒤 receipt로 확정한다. upsert/remove 응답은 저장 뒤 한 번 돌아오며 성공은 `applied:true, saved:true`, 저장 실패는 `applied:true, saved:false`인 오류다. 실패해도 메모리 적용은 유지하며 다음 부팅에 유지되지 않을 수 있다. 설정 창 Save는 저장을 기다리지 않고 진행하며 최종 실패를 토스트와 알림에 표시한다.
- `reload`는 파일 저장 pending 중이면 오류다. 읽기·파싱 실패도 오류로 답하고 이전 메모리와 원본 관측을 유지한다. 성공한 reload는 아직 저장하지 못한 변경을 파일 내용으로 바꾸므로 외부 수정과 저장 충돌을 해소할 때 쓴다. 자세한 revision·원본 보호·종료 계약은 [파일 저장 계약](../../design/systems/storage-contract.md#훅-registry-snapshot)에 있다.

## 관련

- **트리거 출처 대칭**: 훅(내부 이벤트)은 웹훅([webhook](../webhook/index.md), 외부 HTTP 트리거)과 대칭인 trigger 출처다. 두 출처는 이 문서를 공유한다 — `source: hook|webhook|any` 게이트로 셸 action 은 `hook` 출처 전용이다. 훅은 위 "바인딩" 절대로 `HookBinding::Handler(id)` 로 레지스트리 핸들러를 참조해 소비하며, 인라인 셸(`--command`)은 하위호환 익명 경로다.
- [agent-collaboration](../agent-collaboration/index.md) · [notifications](../notifications/index.md) · [claude plugin](../../plugins/claude/index.md)(Claude hook 발생)
