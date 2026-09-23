# Codex (`com.tasty.codex`)

- **Status**: Implemented (bundled plugin)
- **주체**: AI Agent / 로컬 사용자 (`tasty codex` CLI · IPC)
- **배포/통합**: bundled · cli + ipc_namespace + 멀티에이전트 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-codex/`
- **권한**: `terminal.spawn` 등 (매니페스트 `permissions`)
- **화면**: 없음 — CLI/IPC 오케스트레이션 플러그인 (headless).

> **예제로서**: **cli + ipc namespace 멀티에이전트** 예제(claude 의 경량판) — state/handlers 모듈 분리 → [plugin-development](../../dev-guide/plugin-development.md#cli--ipc-namespace).

## 부모의 완료 수신 채널

완료는 부모 종류와 무관하게 caller surface 의 `<parent_home>/notify/<caller_surface>.log`
한 줄로 나간다. child CLI 종류로 채널을 선택하지 않는다. 훅 설치·관측 상태와 부모의 수신
준비는 별개다. idle/needs_input/interrupt/exit는 작업 성공과 같은 뜻이 아니다.
[완료 알림 로그](../../dev-guide/external-interaction.md#child-완료-알림--completion-log)를 따른다.


## 목적

**Codex CLI 를 tasty 안에서 실행·오케스트레이션**하는 통합. [claude](../claude/index.md) 플러그인과 동형이며, 주로 작성한 코드/판단을 Codex 에게 교차 검증시키는 용도.

## 내부 동작

짝 핸들러의 서로 다른 공개 응답과 번역 형식은 기존 호출자 호환을 위해 유지한다.
`children`/`kill` 형상 및 완료 hook의 대칭·의도된 차이는
[짝 핸들러 호환 경계](../../dev-guide/paired-agent-handlers.md)를 따른다. 완료 알림은
[completion-log](../../dev-guide/external-interaction.md#child-완료-알림--completion-log)에 기록하며 caller PTY에 발화를 주입하지 않는다.

- **cli `codex`** (`tasty codex …`) — 서브커맨드: `launch` · `spawn`(자식, 페인 분할) · `children`/`parent` · `tell`(메시지 전송, 줄바꿈 보존·자동 제출) · `notify-caller`(내부용, 아래) · `broadcast` · `kill`/`respawn` · `reboot`(같은 세션 resume 재시작, 아래) · `hook`(stop/prompt-submit/session-start/permission-request/post-tool-use/interrupt/session-end). `install`/`uninstall`(Tasty 훅을 Codex CLI 설정에 설치).
- **ipc_namespace `codex`** — 위 동작의 IPC 표면.
- **hook 명령은 OS 별 셸 구문으로 설치된다** — Codex 는 hook 명령을 Windows 에선 PowerShell, 그 외에선 POSIX sh 로 실행하므로, `install` 이 Windows 에는 `if ($env:TASTY_SURFACE_ID) { $input | tasty codex hook … }` 형태(PS), 그 외에는 `if [ -n "$TASTY_SURFACE_ID" ]; then tasty codex hook … --surface $TASTY_SURFACE_ID || true; fi` 형태(POSIX)를 발행한다. POSIX 쪽은 가드와 실패 처리가 분리돼 있다 — 바깥 `if` 는 tasty 밖 환경(`$TASTY_SURFACE_ID` 미설정)을 무소음 exit 0 으로 처리하고, 안쪽 `|| true` 는 hook 명령 실패만 담당한다(codex 턴을 막지 않기 위해 exit 0 유지). 실패 자체는 `<tasty_home>/hook-failures.log` 에 기록된다([ADR-0075](../../adr/0075-agent-hook-delivery-failure-record.md)). **명령 문자열이 바뀌었으므로 기존 사용자는 `tasty codex install` 재실행이 필요하다** — 재실행은 marker(`tasty codex hook`) 로 옛 entry 를 걷어내고 새 entry 를 넣으므로 중복되지 않는다. 같은 matcher group 안에 사용자 handler가 섞여 있어도 Tasty handler만 교체·제거하며 나머지 handler와 matcher/설정은 보존한다. codex 는 hook command 해시가 바뀌면 trust 를 무효화하지만, tasty 는 모든 codex 인스턴스를 `--dangerously-bypass-hook-trust` 로 띄우므로 이를 runtime 신뢰의 증거로 삼으면 안 된다. 0.154.0 remote resume는 플래그가 있어도 훅 검토 화면을 표시할 수 있다. hook 은 Codex 가 stdin 으로 주는 JSON payload 의 `session_id` 를 읽어 surface meta(`codex-session-id`, `restore.command`)에 기록한다 — `reboot` 와 세션 복원이 이 meta 를 소비한다. 설치 대상은 아래 7개다. matcher 는 어느 항목에도 걸지 않는다 — 승인은 어느 tool 에서도 요청될 수 있어 tool 이름으로 좁힐 근거가 없다([claude](../claude/index.md) 는 `PreToolUse`/`PostToolUse` 를 `AskUserQuestion` 하나로 좁힌다). 그 대가로 `PostToolUse` 는 tool 호출마다 hook 프로세스를 하나 띄운다.

  | Codex config table 키 | tasty hook token | trust state 키 | 산출 상태 | 추가로 쏘는 것 |
  |---|---|---|---|---|
  | `Stop` | `stop` | `stop` | `idle` | `codex-idle` |
  | `UserPromptSubmit` | `prompt-submit` | `user_prompt_submit` | `active` | — |
  | `SessionStart` | `session-start` | `session_start` | `active` | — |
  | `SessionEnd` | `session-end` | `session_end` | 실행 종료 | 현재 세션 meta 정리, 해당 실행 구독 종료 |
  | `PermissionRequest` | `permission-request` | `permission_request` | `needs_input` | `needs-input` · `surface.completion`(kind=needs_input) |
  | `PostToolUse` | `post-tool-use` | `post_tool_use` | `active` | — |
  | `Interrupt` | `interrupt` | `interrupt` | `idle` | `codex-idle` |

  `~/.codex/config.toml`의 `[hooks]` 섹션에 심는다(`settings.json`이 아니다 — Codex 의 hook dispatch 경로가 아니다). Codex 는 새 hook entry를 *trust*하기 전엔 fire하지 않는다 — `--dangerously-bypass-hook-trust`(아래)를 전달하지만 remote resume의 검토 화면이 사라진다고 보장하지 않는다.
- **승인 대기(`needs_input`) 감지** — Codex 가 `Would you like to run the following command?` 승인 화면을 띄우기 직전에 `PermissionRequest` 훅이 발화한다. tasty 는 그것을 대상 surface 의 실행 상태 `needs_input` 으로 주입하고, 같은 훅에서 `needs-input` surface hook(완료 알림 경로)과 공용 attention(`surface.completion` kind=needs_input)까지 쏜다 — 그래서 비활성 탭·워크스페이스에 기존 노란 표시가 나온다(표시·해제 정책은 [surface-highlight](../../features/surface-highlight/index.md) 그대로). 다음은 codex-cli 0.154.0 실측이다.
  - 승인이 **필요 없는** 실행(정책이 자동 허용)에는 `PermissionRequest` 가 아예 발화하지 않는다 — 자동 승인·장기 실행·단순 무출력이 대기로 오판되지 않는다.
  - 해제 신호는 `PostToolUse`(승인 후 그 도구가 **끝났을 때**)와 `Interrupt`(거절·Esc·Ctrl-C)다. Codex 에는 "승인이 났다" 자체를 알리는 이벤트가 없어, **승인 직후 장기 실행 중에는 `needs_input` 이 그 도구의 실행 시간만큼 남는다**(측정: 45 s sleep 명령에서 45.4 s). 해제가 늦을 뿐 잔류하지는 않는다.
  - 거절(`No, and tell Codex what to do differently`)·Esc·Ctrl-C 는 `Interrupt` **하나만** 쏘고 `Stop` 도 `PostToolUse` 도 뒤따르지 않는다. 그래서 `Interrupt` 를 idle 로 읽는다 — 이 매핑이 없으면 중단된 자식이 영원히 `active` 로 남아 기다리는 부모가 풀리지 않는다.
  - `PermissionRequest` payload 에는 `tool_use_id` 가 없다(`PreToolUse`/`PostToolUse` 에는 있다). 그래서 대기와 해제를 tool 단위로 짝지을 수 없고, 한 surface 안에서 승인이 연달아 나면 해제는 tool 단위가 아니라 surface 단위로 뭉뚱그려진다. surface 사이에는 섞이지 않는다(`TASTY_SURFACE_ID` 가 대상을 고정한다).
  - 훅 stdout 은 어느 이벤트에서도 승인 결정을 바꾸지 않는다 — 래퍼가 `{}`(결정 없음)만 보내고, tasty 밖(`$TASTY_SURFACE_ID` 미설정)에서는 아무것도 출력하지 않는데 그 경우에도 승인 프롬프트가 정상 동작한다(실측).
  - 일반 질문 입력(`request_user_input` 등)은 이 훅의 coverage 가 아니다 — 지금 지원하는 것은 **도구 실행 승인**뿐이다.
- **`spawn`/`tell` 은 동기 대기하지 않는다** — 호출 즉시 반환하고, 대상이 idle 이 될 때마다, 그리고 최종적으로 exited 가 되면 caller의 완료 알림 로그로 알림을 보낸다. **그 경로는** `codex-idle`(`stop`/`interrupt` hook 이 `surface.fire_hook` 으로 쏨) · `needs-input`(`permission-request` hook) · `process-exit`(host 내장) 세 이벤트에 once(1회성) hook 을 등록하고, 먼저 fire 되는 쪽이 `notify-caller` 를 실행해 알림을 보낸 뒤 형제 hook 을 정리한다(등록 순서 무관 — fire 시점에 `hook.list`(대상 surface 필터) 로 자기와 **동일 command** 를 가진 형제를 찾아 `hook.unset`. 상태를 공유하지 않아 같은 surface 에 spawn/tell 이 겹쳐 등록돼도 서로의 형제를 덮어써 좀비로 남기지 않는다). 정리 후 `notify-caller` 는 `surface.locate` 로 target 이 아직 살아있는지(=이번 fire 가 process-exit 가 아니었는지) 확인해, 살아있으면 세 hook 을 다시 등록한다(자기재무장) — codex-idle 이 여러 번 반복돼도(예: 대기 후 재개) exit 할 때까지 계속 알림이 온다. `needs_input` 도 같은 경로로 알린다 — `needs-input`(`permission-request` hook 이 쏨)이 세 번째 형제로 함께 등록된다. **완료 알림에 샌드박스 초기화 실패 힌트가 자동으로 덧붙는다** — `notify-caller`가 알림을 조립하기 직전 대상 surface 의 최근 화면 출력(`surface.screen_text`, 최근 800줄)을 조회해 `RTM_NEWADDR`(아래 샌드박스 정책 플래그 항목의 실패 시그니처) 이 보이면 "sandbox 초기화 실패로 보임 — `--full-auto`로 재시도해보세요" 류 문구를 알림 본문에 추가한다(best-effort — 조회 실패나 미탐지 시 알림은 기존과 동일).
- **훅 응답은 조용히 실패한 host 호출 수를 싣는다** — `host_call_failures`(항상 있고 항상 수). 이 핸들러는 `terminal.set_state` 만 전파하고(그 실패는 오류 응답이라 이미 보인다) `surface.meta.set`·`surface.fire_hook` 은 최선노력이라 실패해도 응답이 `ok` 다 — 세는 것은 그 최선노력 쪽이다. 0 이 아니면 `<tasty_home>/hook-failures.log` 에도 남는다. 규약은 [error-handling](../../dev-guide/error-handling.md) "최선노력의 대가는 치르되 값으로 노출한다".
- **훅 출력 계약** — `hook`의 IPC/직접 CLI 응답은 진단용 `host_call_failures`를 포함한다. 설치된 일곱 셸 래퍼는 CLI stdout을 버리고 Codex에는 빈 JSON 객체 `{}`만 반환한다 — `PermissionRequest` 에서 그 값은 "결정 없음"이라 승인 흐름을 바꾸지 않는다. 실패 기록(`hook-failures.log`)과 stderr는 유지한다. [Codex 훅 출력 규약](https://learn.chatgpt.com/docs/hooks)에 없는 내부 필드를 전달하지 않으며, 기존 설치에는 `tasty codex install`을 다시 실행해 래퍼를 갱신한다.
- **모든 codex 기동 명령에 `--dangerously-bypass-hook-trust`** — `spawn`/`launch`/`reboot`이 이 옵션을 전달한다. 그러나 0.154.0 remote resume의 훅 검토 화면은 별도로 나타날 수 있다. `install`의 trust 표시는 선택한 파일의 metadata이며 실제 훅 발화나 원격 daemon 신뢰의 증명이 아니다. 검토가 필요한 훅은 사용자가 Codex에서 확인하며 Tasty가 대신 승인하지 않는다.
- **POSIX 셸의 외부 Codex 실행** — `launch`/`spawn`/`respawn` 의 공통 빌더는 모든 OS 에서 기존 POSIX 환경변수·프롬프트 구문과 함께 `command codex` 를 사용해 셸의 `codex` alias/function 을 우회하고 현재 `PATH` 의 실행 파일을 찾는다. Windows 에서 탐지하는 Git Bash 도 이 경로를 사용한다. `reboot` 는 Linux/macOS 에서 `command codex`, Windows 에서는 기존 `codex` 실행어를 유지한다. Windows 의 수신 셸 종류를 모르는 상태에서 cmd 를 중첩하지 않으므로 Git Bash 의 MSYS 인자 변환을 새로 유발하지 않는다. **Windows reboot 의 alias/function 우회와 native cmd/PowerShell 전용 새 세션 전달 개선은 후속 범위**다. 기존 정책 우선순위·기본값·명시 옵션과 Windows 지원 범위는 바꾸지 않는다. alias 에 넣어 둔 옵션을 적용하려면 tasty 의 정책 플래그나 전역 설정에 명시한다.
- **승인/샌드박스 정책 플래그** (`launch`/`spawn`/`respawn`/`reboot` 공통) — `--approval <untrusted|on-request|never>`(codex `-a`), `--sandbox <read-only|workspace-write|danger-full-access>`(codex `-s`), `--full-auto`(codex `--dangerously-bypass-approvals-and-sandbox`, `--approval`/`--sandbox` 와 동시 지정 시 invalid_params 로 거부)를 그대로 기동 명령에 전달한다. 우선순위는 **호출별 플래그 > 전역 설정(Settings › Plugin › Codex 의 "Default approval policy"/"Default sandbox mode", storage key `default_approval_policy`/`default_sandbox_mode`) > 하드코드 기본값**. **`approval` 은 호출별 플래그도 전역 설정도 없으면(또는 전역 설정이 `inherit`) 무조건 `never` 로 해석된다** — 무인 자동화 흐름에서 승인 프롬프트가 뜨면 아무도 응답하지 않아 멈추므로, "아무것도 안 정하면 codex 자체 인터랙티브 기본값" 이라는 옛 동작은 더 이상 없다. 위 `PermissionRequest` 훅으로 그 정지는 **관측 가능**해졌지만(상태가 `needs_input` 이 되고 caller 에게 알림이 간다) 스스로 풀리지는 않는다. 인터랙티브 승인이 정말 필요하면 `--approval untrusted`/`on-request` 를 명시적으로 넘긴다. **`sandbox` 는 승인과 달리 그 자체로 정지를 유발하지 않으므로 기존대로 미설정 시 플래그를 아예 안 붙여 codex 자체 기본값을 쓴다** — 단 nested sandbox(bubblewrap) 를 지원하지 않는 실행 환경에서는 `--sandbox read-only`/`workspace-write` 지정이 `RTM_NEWADDR: Operation not permitted` 류로 실패할 수 있어, 그런 환경에선 `--full-auto`(샌드박스까지 완전 우회)를 명시적으로 골라야 한다.
- **`reboot`** (`tasty codex reboot [--surface <id>] [--delay <초>] [--prompt <추가문구>] [--approval <값>] [--sandbox <값>] [--full-auto]`) — surface 안의 Codex 를 종료하고 **같은 세션으로 재시작**한다([claude reboot](../claude/index.md) 와 동형). 동작: 즉시 응답 → delay(기본 5s) 후 Ctrl+C ×4 → 일반 종료 안내 또는 remote TUI의 `Disconnected from this task.` 증가 확인 후 `<플랫폼별 Codex 실행어> resume --dangerously-bypass-hook-trust [정책 플래그] -c check_for_update_on_startup=false <session_id>` 전송(업데이트 프롬프트가 기동을 가로채는 것을 방지) → 복귀 확인 후 재시작 안내 프롬프트를 화면 검증·재시도 + 별도 Enter 로 제출. 정책 플래그는 위 승인/샌드박스 규칙과 동일하게 해석된다. 안전 가드는 claude 와 동일(전경 불일치 시 미전송·중단, 중복 reboot 거부). **턴의 마지막 행동으로 호출할 것.** SessionEnd는 현재 세션 meta를 정리한다. reboot는 종료 전에 세션과 검증된 remote 연결 문맥을 캡처한다.
- **spawn child 개수 경고** — `spawn` 이 성공한 뒤 parent 의 현재 child 수를 재조회해, Settings › Plugin › Codex 의 "Spawn child warning threshold"(기본 6) 를 넘으면 응답에 `warning` 필드를 실어 돌려준다(soft 경고, spawn 자체는 막지 않음). 재사용 후보가 있으면 그 index 목록과 함께 새로 spawn 하는 대신 `respawn` 사용을 권하는데, 근거가 다른 두 목록으로 나뉜다: **`idle`** 은 자식이 hook 으로 완료를 직접 보고한 것이고, **확정 `stale`**(`confidence: confirmed` = 전경이 셸로 복귀)은 보고가 오지 않은 채 호스트 관측이 에이전트 프로세스 종료를 잡아낸 것이다(hook 유실 — [ADR-0072](../../adr/0072-child-state-hook-observation-fusion.md) 가 겨냥한 시나리오). 후자에 "이미 작업을 끝냈다" 는 문구를 쓰면 자식이 그렇게 보고한 적 없는데 보고한 것처럼 읽히므로 문구를 분리한다. 세 문구는 plugin 의 `lang/{en,ko,ja}.toml` 의 `codex.spawn_warning.{total,idle,stale}` 에 있고, 활성 언어(`general.language`)를 따라간다 — plugin process 는 호스트 i18n 카탈로그에 접근할 수 없으므로 SDK `Translator` 로 자기 `lang/` 를 직접 로드한다([i18n](../../dev-guide/i18n.md) "Plugin 네임스페이스").

  `confidence: heuristic` 인 `stale` 은 **세지 않는다** — SIGSTOP·긴 추론·무출력 명령과 관측상 구별되지 않아, 그것까지 respawn 후보로 부르면 일하는 자식을 재시작하라고 권하게 된다([api-conventions](../../dev-guide/api-conventions.md) 가 같은 이유로 `stale` 을 기본 terminal state 집합에서 뺀 것과 동일한 판단). 판정 축 자체는 [child-terminal](../../features/child-terminal/index.md) "판정 응답 필드" 참조.

## 훅 설치 대상 선택

`tasty codex install --codex-home <절대 디렉터리>` 는 그 Codex home 의 hook 을 설치한다.
`--config-file <절대 파일>` 은 설정 파일을 직접 고르며 `--codex-home` 과 함께 쓸 수 없다.
기존 설정을 읽거나 파싱할 수 없으면 덮어쓰지 않고 실패한다. 둘 다 생략하면 호스트 환경의
`CODEX_HOME` 을, 그것도 없으면 `HOME`/`USERPROFILE` 아래 `.codex` 를 쓴다. 원격 서버의
설정은 로컬 설치가 성공했다는 것만으로 갱신되지 않는다.

## 인터페이스

- **AI Agent / 사용자**: `tasty codex launch|spawn|tell|broadcast|kill|respawn|children|parent|hook|install …`.
- 일반 흐름: `spawn --prompt "…"` → (선택) `tell` → 완료 알림 대기(caller의 completion-log 조회) → 출력 확인.

## 비-목표

- Codex 자체 기능 — 외부 CLI. 이 플러그인은 *실행·배치·관계 관리*.
- 터미널/PTY 내부 — host.

## Acceptance Criteria

- Given Linux/macOS 의 sh/bash/zsh 에 `codex` alias 또는 function 이 우회 옵션을 추가한다 When `launch`/`spawn`/`respawn`/`reboot` 명령을 생성해 실행한다 Then 외부 Codex 에는 tasty 가 결정한 정책 인자만 전달되고 환경변수와 프롬프트 전달 방식이 유지된다.

- Given 플러그인 활성 When `tasty codex spawn --prompt "…"` Then 자식 Codex 가 페인 분할로 생성되고 CLI 는 즉시 반환된다.
- Given 자식 When `tasty codex tell <msg>` Then 줄바꿈 보존하며 메시지가 전송·제출되고 CLI 는 즉시 반환된다.
- Given `spawn`/`tell` 로 등록된 완료 대기 When 대상이 idle · needs_input · exited 에 도달 Then caller 의 완료 알림 로그에 상태를 전달하고 형제 hook 이 정리된다. exited 가 아니었다면 형제 hook 이 재등록돼 이후 상태 전환에도 계속 알림이 온다.
- Given 훅이 설치된 Codex When 도구 실행 승인 프롬프트가 뜬다 Then 그 surface 의 상태가 `needs_input` 으로 조회되고 비포커스 대상의 탭·워크스페이스에 기존 노란 표시가 난다.
- Given 승인 대기 When 사용자가 승인하고 그 도구가 끝난다(`PostToolUse`) 또는 거절·Esc·Ctrl-C 로 중단한다(`Interrupt`) Then 상태가 각각 `active` · `idle` 로 돌아오고 `needs_input` 이 잔류하지 않는다.
- Given 승인이 필요 없는 실행 When 도구가 오래 걸리거나 출력이 없다 Then `needs_input` 으로 오판하지 않는다.
</content>
