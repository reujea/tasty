# Claude Code (`com.tasty.claude`)

- **Status**: Implemented (bundled plugin)
- **주체**: AI Agent / 로컬 사용자 (`tasty claude` CLI · IPC)
- **배포/통합**: bundled · cli + ipc_namespace + 멀티에이전트 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-claude/`
- **권한**: `terminal.spawn` 등 (매니페스트 `permissions`) · `memory.read`(Stop-훅 게이트가 발화 surface 의 goal 을 읽는다 — 읽기 전용)
- **화면**: 없음 — CLI/IPC 로 터미널 surface 를 조작하는 오케스트레이션 플러그인 (headless).

> **예제로서**: **최대 통합 레퍼런스**(~3.5k줄) — cli + ipc namespace + 멀티에이전트 + **훅** + event_subscribe + 외부 설치. state/handlers/install/hook/error_scan 모듈 분리의 본보기 → [plugin-development](../../dev-guide/plugin-development.md#cli--ipc-namespace).

## 부모의 완료 수신 채널

완료는 부모 종류와 무관하게 caller surface 의 `<parent_home>/notify/<caller_surface>.log`
한 줄로 나간다. child CLI 종류로 채널을 선택하지 않는다. 훅 설치·관측 상태와 부모의 수신
준비는 별개다. idle/needs_input/interrupt/exit는 작업 성공과 같은 뜻이 아니다.
[완료 알림 로그](../../dev-guide/external-interaction/child-completion-notify-log.md)를 따른다.


## 목적

**Claude Code CLI 를 tasty 안에서 실행·오케스트레이션**하는 통합. 새 워크스페이스/페인에 Claude 인스턴스를 띄우고, 부모-자식 관계로 여러 인스턴스를 spawn·제어한다 (멀티에이전트).

## 내부 동작

짝 핸들러의 서로 다른 공개 응답과 번역 형식은 기존 호출자 호환을 위해 유지한다.
`children`/`kill` 형상 및 완료 hook의 대칭·의도된 차이는
[짝 핸들러 호환 경계](../../dev-guide/paired-agent-handlers.md)를 따른다. 완료 알림은
[completion-log](../../dev-guide/external-interaction/child-completion-notify-log.md)에 기록하며 caller PTY에 발화를 주입하지 않는다.

- **cli `claude`** (`tasty claude …`) — 서브커맨드: `launch`(새 워크스페이스에서 실행) · `spawn`(자식 인스턴스, 페인 분할) · `children`/`parent`(관계 조회) · `tell`/`broadcast`(메시지 전송) · `kill`/`respawn` · `reboot`(같은 세션 resume 재시작, 아래) · `child-profile`(자식에게 지속 프로필 부착, 아래) · `hook`(Claude Code 훅 통합, 아래 "Claude Code 훅 통합" 절) · `checklist-hook`(`continue-checklist` 세션 프로필 전용 `Stop` 훅, 아래 "continue-checklist 세션 프로필" 절) · `checklist-enable`/`checklist-disable`/`checklist-status`(게이트별 마커 파일을 켜고 끄고 조회 — `--gate` 생략 시 `continue-checklist`, 같은 절) · `notify-done`(내부용: spawn/tell 상태 전환 시 caller 에게 알림 전달 + 형제 hook 정리·재무장, 아래) · `profile-register`/`profile-unregister`/`profile-list`/`profile-show`/`profile-current`(Claude 세션 프로필 레지스트리, 아래 "Claude 세션 프로필 레지스트리" 절).
- `spawn`/`tell`은 **동기 블록 없이 즉시 반환**한다. 대상(child 또는 tell 대상 surface)이 idle/needs_input 에 도달할 때마다, 그리고 최종적으로 exited 에 도달했을 때 caller surface(spawn/tell을 호출한 surface)의 완료 알림 로그로 상태를 전달한다. `claude-idle`/`needs-input`/`process-exit` 3개의 once(1회성) surface hook을 등록해 구현하며, 그중 하나가 fire되면 `notify-done`이 알림 전송 + 나머지 형제 hook 정리 후, target surface 가 아직 살아있으면(=이번 fire 가 process-exit 가 아니었으면) `surface.locate` 로 확인해 3개 hook 을 다시 등록한다(자기재무장). 이 덕분에 needs-input(되묻기) 같은 일시적 상태 전환을 거쳐도 그 뒤 진짜 완료 시 알림을 놓치지 않는다 — "spawn/tell 당 알림 1회"가 아니라 "child 가 살아있는 동안 상태 전환마다 알림"이다.
- **ipc_namespace `claude`** — 위 동작의 IPC 표면.
- **event_subscribe** `surface.closed` — surface 종료를 받아 인스턴스 상태 정리.
- 실제 Claude 프로세스는 터미널 surface 안에서 돌고(`terminal.spawn`), 플러그인은 그 생명주기·관계를 관리한다.
- **`reboot`** (`tasty claude reboot [--surface <id>] [--delay <초>] [--prompt <추가문구>] [--profile-file <경로> | --profile <이름[,이름2,...]>] [--clear-profile] [--permission-mode <모드>]`) — surface 안의 Claude 를 종료하고 **같은 세션으로 재시작**한다. Claude 는 스스로 자기 TUI 를 껐다 켤 수 없으므로 에이전트가 이 명령을 자기 surface 에 호출한다(설정/훅/버전 변경 반영용). 동작: 즉시 응답 반환 → `--delay`(기본 5s) 후 Ctrl+C ×4(0.5s 간격) → 전경 프로세스가 Claude 에서 이탈했는지 확인 후 셸에 `claude -r <session_id>`(프로필이 해석되면 뒤에 `--settings "<경로>"` 추가) 전송(session id 는 요청 시점에 surface meta `claude-session-id` 에서 캡처) → Claude 복귀 확인 후 재시작 안내 프롬프트를 `terminal.tell` 로 제출(화면 검증·재시도 + 별도 Enter 로 결정적 제출). 안전 가드: 전경이 여전히 Claude 면 텍스트 미전송·중단, resume 후 미복귀면 안내 미전송(셸 오염 방지), 같은 surface 중복 reboot 거부. **턴의 마지막 행동으로 호출할 것** — delay 이후 진행 중이던 턴은 잘린다.
  - **`claude-session-id` meta 가 비어 reboot 가 실패하는 경우**: `no active claude session on surface {id} (claude-session-id meta not set …)` 에러는 hook 미설치가 아니어도 발생할 수 있다 — session-start hook 이 이 meta 를 못 심은 것이 원인. 조용히 실패할 수 있는 지점이 최소 3곳: ① `install.rs`의 등록 커맨드가 `[ -n "$TASTY_SURFACE_ID" ] && tasty claude hook … || true` 라 `TASTY_SURFACE_ID` 미설정 시 tasty 바이너리 자체가 실행되지 않음(로그 불가), ② `hook.rs`의 `apply_hook` session-start 분기가 stdin JSON 에 `session_id` 가 없으면 meta 기록을 건너뜀(`tracing::warn!`으로 로그, `tasty plugin logs com.tasty.claude --follow` 또는 `~/.tasty/plugins-logs/com.tasty.claude.log` 에서 확인), ③ `dynamic.rs`의 `read_stdin_json` 이 TTY/파싱 실패로 `None` 을 반환(hook 은 CLI 프로세스라 tracing 이 **stderr 로만** 나간다 — 공유 로그 파일에는 남지 않는다, [ADR-0092](../../adr/0092-file-log-host-process-only.md). 전달 실패 자체는 `$TASTY_HOME/hook-failures.log` 에 기록된다). 수동 복구: `tasty surface-meta set --key claude-session-id --value <세션ID>`.
- **`child-profile`** (`tasty claude child-profile [--surface <부모>] --child <index> [--delay <초>] [--prompt <추가문구>] [--profile-file <경로> | --profile <이름[,이름2,...]>] [--clear-profile] [--permission-mode <모드>]`) — **부모가 자식에게 지속 세션 프로필을 부착**한다. `--child <index>`(`claude children` 이 보여주는 index)를 `terminal.children` 으로 자식 surface id 로 해석한 뒤, 그 surface 에 대해 위 `reboot` 과 **완전히 같은 경로**를 태운다(프로필 검증 → surface meta 부착 → Ctrl+C 시퀀스 → `claude -r <sid> --settings "<경로>"` → 안내 프롬프트). 별도 부착 메커니즘이 아니라 reboot 진입점의 재사용이므로, 부착 상태는 자식의 이후 **무인자 `reboot` 에 그대로 승계**된다. 중복 가드도 reboot 과 같은 set 을 쓴다 — 같은 자식에 `reboot` 과 이 명령이 겹치면 뒤엣것이 "이미 진행 중" 으로 거부된다.
  - **`reboot` 과 다른 점 1 — 턴이 잘리지 않는다.** `reboot` 의 "턴의 마지막 행동으로 호출할 것" 경고는 **호출자 자신이 재기동될 때**의 제약이다. 이 명령은 자식만 재기동시키므로 **부모의 턴은 잘리지 않는다** — 호출 후 계속 작업해도 된다.
  - **`reboot` 과 다른 점 2 — 완료 알림이 걸린다.** `spawn`/`tell` 과 동일하게 caller surface 로 `claude-idle`/`needs-input`/`process-exit` 알림 hook 이 자동 등록된다(위 spawn/tell 항목의 자기재무장 사이클과 같음). 자식이 재기동을 마치고 idle 에 도달하면 부모가 그 사실을 통지받는다. `reboot` 은 알림을 걸지 않는다(자기 자신이 대상이라 받을 주체가 없다).
  - **`--child` 는 필수다.** 자기 자신에게 붙이는 것은 `reboot --profile` 의 몫이라 창구를 겹치지 않게 한다. 없는 index 를 주면 사용 가능한 index 목록과 함께 즉시 에러이며, **아무 자식도 죽지 않는다** — 프로필 인자 검증(상호배타 · 미등록 이름 · JSON 파싱)도 전부 Ctrl+C 시퀀스 시작 **이전**에 끝난다.
  - **`spawn`/`respawn`/`launch` 의 `--profile` 과의 차이**: 그쪽은 그 기동 명령 **1회에만** `--settings` 를 싣고 meta 를 건드리지 않는다 — 자식이 한 번이라도 `reboot` 하면 프로필이 빠진다. 지속 부착이 필요하면 이 명령을 쓴다.

- **Claude 세션 프로필**(용어 정의: [ubiquitous-language.md](../../concepts/ubiquitous-language.md)) — Claude Code 는 훅을 프로세스 기동 시 한 번만 읽으므로, 살아있는 세션에 훅을 추가하는 유일한 창구가 `reboot`/`spawn`/`respawn`/`launch` 4개 기동 경로다. 프로필을 붙이는 방법은 두 가지고 **상호 배타적**이다(둘 다 주면 즉시 에러):
  - `--profile-file <경로>`(`path_kind = "file"`, CLI 가 호출자 cwd 기준 절대경로로 정규화, **반복 지정 거부** — 아래 "왜 반복 지정을 CLI 가 거부하는가") — 파일 경로를 그대로 쓴다.
  - `--profile <이름[,이름2,...]>` — 아래 "Claude 세션 프로필 레지스트리"에 등록해 둔 프로필 이름, 또는 "Stop-훅 게이트 레지스트리"에 등록해 둔 **게이트 이름**으로 부착한다(두 레지스트리는 이름 공간을 공유한다). 이름을 둘 이상 쉼표로 주면 레지스트리가 머지해 만든 파일 하나를 쓴다.

  어느 쪽이든 최종적으로 기동 명령에 `--settings "<경로>"` 가 붙는다 — Claude Code 의 `--settings` 는 `~/.claude/settings.json` 의 기존 훅을 **대체가 아니라 병합**하므로 tasty 내장 훅(`claude hook` 경유)도 그대로 발화한다. `reboot`(및 자식을 대상으로 같은 경로를 타는 `child-profile`) 만 부착 상태를 surface meta 에 기록해 **다음 무인자 reboot 가 기본값으로 승계**한다 — 경로로 부착하면 `claude-session-profile`(경로 그대로), 이름으로 부착하면 `claude-session-profile-names`(이름 문자열)에 기록되고 두 meta 는 상호 배타적으로 관리된다(한쪽을 새로 쓰면 다른 쪽은 지운다). 이름-meta 는 **승계 시점마다 레지스트리에서 다시 해석**한다 — 경로를 캐시하지 않으므로 그 사이 `profile-register` 로 내용이 갱신됐다면 다음 reboot 에 최신 내용이 반영된다. 두 meta 모두 파일 존재 + JSON 파싱을 매 reboot 마다 동기 재검증한다(승계된 프로필이 깨져 있으면 kill 시퀀스를 시작하지 않고 에러 반환). `--clear-profile` 로 둘 다 뗀다. `spawn`/`respawn`/`launch` 는 그 호출 1회의 기동 명령에만 반영하고 meta 를 건드리지 않는다(반복 재기동은 `reboot` 계열만의 개념) — 그렇게 띄운 자식에 지속 부착이 필요하면 `child-profile` 을 쓴다.
  - **복원을 건너 프로필이 유지된다** — 아래 "복원을 건너 프로필이 유지되는 방식".
  - **왜 반복 지정을 CLI 가 거부하는가**: Claude Code 의 `--settings` 는 반복 지정 시 **마지막 값만 남고 앞선 값이 조용히 사라진다**(실측). tasty CLI 인자 자체(`--profile-file`)를 실수로 두 번 주는 경우도 같은 함정에 빠질 수 있어, 매니페스트 `CliArg.reject_repeat = true`(`crates/tasty-cli/src/dynamic/build.rs`)로 clap 을 `ArgAction::Append` 로 등록해 두 번째 값이 들어오면 조용히 버리지 않고 에러로 거부한다.

### 복원을 건너 프로필이 유지되는 방식

앱 재시작(레이아웃 복원)과 닫은 탭 복원(Ctrl+Shift+T)은 **surface meta 를 넘기지 못한다** — 복원은 stale id 와 겹치지 않는 새 surface id 를 발급하고 곧바로 live 아닌 surface meta 를 purge 하기 때문이다. 그래서 부착 상태를 meta 에만 두면 복원된 Claude 는 `claude -r <id>` 로만 떠서 프로필 훅이 발화하지 않는다.

plugin 은 이를 **session id 로 키잉한 부착 기록**으로 해결한다 (host 는 관여하지 않는다 — [layout-persistence](../../features/layout-persistence/index.md) 의 `restore.command` 계약은 그대로 agent-agnostic).

- **기록 위치**: `TASTY_PLUGIN_DATA_DIR/profiles/attachments/<session_id>.json`. 내용은 `{"kind": "names"|"path", "value": …}` — **이름으로 부착한 것은 이름을** 남긴다(복원 시 재해석 대상). data dir 은 설치 디렉터리와 분리돼 있어 `upgrade-builtins`/재설치를 건너 보존된다([plugin-development](../../dev-guide/plugin-development.md) §6 "data dir 수명 계약").
- **쓰기**: `reboot` 이 프로필을 부착/해제할 때 surface meta 갱신과 같은 지점에서 함께 갱신한다(`--clear-profile` 은 meta 2키와 기록을 함께 지운다).
- **복구**: `session-start` 훅이 프로필 meta 를 먼저 보고, 비어 있으면(=복원을 건너온 세션) 기록에서 meta 를 되살린다. 그다음 이름을 **매번 다시 해석**해(레지스트리 최신 내용 반영) `restore.command` 를 `claude -r <id> --settings "<경로>"` 로 쓴다. 세션 id 는 `claude -r <id>` 를 건너 보존되므로 기록의 키로 쓸 수 있다 — 이 전제가 깨지면(Claude Code 가 resume 시 새 id 를 발급하면) 프로필만 조용히 빠지므로, 재시작 후 세션 생존 확인을 릴리스 점검에 유지한다.
- **재기록(re-stamp)**: 프로필이 확정되면 session-start 마다 기록을 다시 쓴다. `reboot` 의 Ctrl+C 는 `SessionEnd` 를 발화시켜 방금 쓴 기록을 지우는데(정리 자체는 정상 동작), 이어지는 session-start 가 프로필 meta 를 근거로 즉시 복구한다 — 이 재기록이 없으면 모든 reboot 이 기록을 영구 소실시킨다.
- **실패는 조용한 강등**: 부착된 이름이 그 사이 `profile-unregister`/`gate-unregister` 됐거나 경로가 깨졌으면 warn 로그만 남기고 **프로필 없이** 복원한다. 같은 상황에서 `reboot` 은 에러로 시퀀스를 시작조차 하지 않지만(깨진 프로필로 기동이 실패하면 전경이 방치된다), session-start 에는 에러를 돌려줄 상대가 없고 여기서 실패시키면 세션 복원 자체가 깨진다.
- **수명**: 전역 `session-end` 는 기록을 즉시 지우지 않고 **종료 표시**(`ended_at`)만 하고, 24시간 유예 뒤 sweep 이 회수한다. 즉시 삭제하지 않는 이유는 실측된 닫은 탭 복원 경로 때문이다 — 탭을 닫으면 PTY 가 죽으면서 `SessionEnd` 가 발화하는데 호스트는 아직 살아 있어 훅이 정상 도달한다. 여기서 기록을 지우면 곧바로 이어지는 Ctrl+Shift+T 복원이 프로필 meta 를 되살릴 근거를 잃는다(프로세스 자체는 `restore.command` 덕에 `--settings` 를 달고 뜨지만 `profile-current` 와 무인자 reboot 승계가 깨진다). 기록은 session id 로 키잉되므로 유예 동안 살아 있어도 다른 세션이 읽을 수 없다 — 같은 id 가 다시 나타나는 유일한 경로가 `claude -r`(=복원)이다. 훅이 아예 못 뛴 잔재(강제 종료 등)는 90일 TTL 이 담당하며, 살아있는 세션의 기록은 re-stamp 로 계속 젊어지므로 오탐 여지가 사실상 없다. `--clear-profile` 만은 사용자가 명시적으로 뗀 것이라 유예 없이 즉시 삭제한다.
- **게이트도 같은 경로**: 프로필과 게이트는 이름 평면을 공유하므로 게이트 이름으로 부착한 것도 그대로 복원된다. 같은 이름이 프로필↔게이트로 재등록됐으면 다음 복원은 **새 정의**로 해석한다(경로를 캐시하지 않는 것의 귀결).
- **범위 밖**: 레이아웃 프리셋은 세션 복원이 아니라 구조 템플릿이라 `restore_command` 를 저장하지 않는다 — 프리셋 적용으로는 프로필이 붙지 않는다. `spawn`/`launch`/`respawn --profile` 은 부착 기록을 만들지 않는다(반복 재기동은 `reboot` 만의 개념).

### Claude 세션 프로필 레지스트리

프로필 파일을 매번 손으로 만들고 경로를 외우는 대신, **이름으로 등록해 두고** 위 `--profile <이름>` 으로 부착하는 계층. `src/hook_handler/registry.rs` 의 형태(patch semantics · `<owner>/<short>` id)를 미러링하되 타입은 공유하지 않는다 — 소비자가 이 플러그인 하나뿐이라 호스트 레지스트리를 신설하지 않고 plugin 내부(`crates/tasty-plugin-claude/src/profile.rs`)에 둔다.

- **등록**: `tasty claude profile-register <이름> --file <경로>` — `<경로>`(JSON object) 를 읽어 `TASTY_PLUGIN_DATA_DIR/profiles/registered/<이름>.json` 에 **복사본**으로 저장한다(원본이 나중에 옮겨지거나 지워져도 레지스트리는 영향받지 않는다). 이미 등록된 이름이면 내용을 덮어쓴다. 이름은 소문자/숫자/`-`, 최대 32자.
- **해제**: `tasty claude profile-unregister <이름>`.
- **목록**: `tasty claude profile-list` — **이름으로 부착 가능한 것 전부**를 보여준다: 등록 프로필(`user/<이름>`, `description` 없음) · 등록 게이트(`user/<이름>`, 게이트임을 알리는 `description`) · host 기본 게이트(`host/continue-checklist`). 여기에 항상 전역 설치돼 있는 내장 훅 9종(`host/<token>`, attachable 아님 — 위 "Claude Code 훅 통합" 절의 `install.rs::MANAGED_HOOKS` 를 그대로 나열, 정의를 복제하지 않는다)이 더해진다. `profile-list` 와 `gate-list` 가 둘 다 게이트를 보여주는 것은 의도된 중복이다 — 전자는 "부착 가능한 것들" 관점, 후자는 "게이트 정의"(본문·센티넬·상한·on/off) 관점.
- **조회**: `tasty claude profile-show <이름>` — 등록 프로필이면 원본 JSON 그대로, 게이트면 그 게이트를 발동시키는 **생성된 Stop 훅 조각**. `owner` 는 실제 출처를 그대로 반영한다(`user` 등록 프로필/등록 게이트, `host` 기본 게이트). `tasty claude profile-current [--surface <id>]` — 그 surface 에 지금 부착된 것(이름 또는 경로)과 내장 훅 목록을 함께 보여준다("지금 이 세션에 무슨 프로필/게이트가 걸려 있나").
- **이름 해석 순서(부착 시)**: `--profile <이름>` 으로 **부착할 때**의 순서다 — ① `profiles/registered/<이름>.json` → ② 등록 게이트 → ③ host 기본 게이트 → ④ 내장 훅 토큰이면 "attach 불가" 에러 → ⑤ 그 외 미등록 에러. ①과 ②는 등록 시점에 상호 배제되지만(아래 "Stop-훅 게이트 레지스트리" 의 이름 충돌 거부) 순서는 방어적으로 고정돼 있다. 위 `profile-show` 는 이 경로를 쓰지 않으므로 ④가 적용되지 않는다 — 내장 훅 토큰을 주면 "attach 불가" 가 아니라 미등록 에러(`no registered profile named 'user/stop'`)가 난다.
- **조합 머지**(`crates/tasty-plugin-claude/src/profile_merge.rs`) — `--settings` 는 슬롯이 하나뿐이라(위 실측) 이름을 둘 이상 주면 등록된 각 파일을 순서대로 접어 하나의 JSON 으로 만들고 `TASTY_PLUGIN_DATA_DIR/profiles/generated/<정렬된-이름들>.json` 에 실체화한다(등록 원본과 별도 하위 디렉토리 — 재생성되는 산출물이 원본을 덮어쓰지 않도록). 매 attach 시점마다 다시 만들어 항상 최신 등록 내용을 반영한다. 키 유형별 규칙:

  | 키 유형 | 예 | 규칙 |
  |---|---|---|
  | 훅 이벤트 배열 | `hooks.Stop` | union(중복 command 문자열 제거) — 사실상 concat, 둘 다 발화 |
  | 객체 맵 | `env`, `enabledPlugins` | 키 단위 재귀 병합. 리프 값 충돌은 스칼라 규칙과 동일 |
  | 허용/거부 리스트 | `permissions.allow`/`deny` | union 후 **불변식 강제**: `deny` 에 있는 항목은 `allow` 에서 제거한다 — 프로필 조합 순서와 무관하게 deny 가 항상 이긴다(테스트: `profile_merge::tests::deny_beats_allow_*`) |
  | 스칼라(대부분) | `theme`, `effortLevel` | 값이 다르면 경고 로그 남기고 나중 프로필 값으로 last-wins |
  | 스칼라(보안 민감) | `permissions.defaultMode` | 값이 다르면 **거부**(에러) — 권한 모드가 조합으로 조용히 약해지는 것을 last-wins 보다 우선 차단 |

- **저장 위치** — 전부 `TASTY_PLUGIN_DATA_DIR`(`~/.tasty/plugin-data/com.tasty.claude/`) 하위. 호스트가 이 디렉토리를 미리 만들어 주므로 `fs.write` 권한 없이도 쓸 수 있다. 호스트가 이 env 를 주입하지 않은 비정상 기동(`data_dir = None`)이면 등록/부착 모두 명시적 에러로 거부한다 — `~/.claude/` 나 새 경로를 조용히 쓰지 않는다.
- IPC: `claude.profile_register`/`claude.profile_unregister`/`claude.profile_list`/`claude.profile_show`/`claude.profile_current` — CLI 서브커맨드와 1:1 대응(원칙 2, 에이전트 조작 가능성).
- spawn 시 parent 의 살아있는 child 수가 설정 임계치를 넘으면 응답에 `warning` 필드가 실린다 — Settings › Plugin › Claude Code 에서 임계치 조정. 재사용 후보는 근거가 다른 두 목록으로 나뉜다: **`idle`**(자식이 hook 으로 완료를 직접 보고) 과 **확정 `stale`**(`confidence: confirmed` — 보고는 없었지만 전경이 셸로 복귀해 에이전트 프로세스 종료가 관측됨, 즉 hook 유실). `confidence: heuristic` 인 `stale` 은 SIGSTOP·긴 추론과 구별되지 않아 세지 않는다 — 판정 축은 [child-terminal](../../features/child-terminal/index.md) "판정 응답 필드" 참조.
- **승인 정책 (`--permission-mode`)** — `launch`/`spawn`/`respawn`/`reboot`/`child-profile` 다섯 경로가 `--permission-mode <acceptEdits|auto|bypassPermissions|manual|dontAsk|plan>` 를 받아 기동 명령에 그대로 실어 보낸다(IPC 쪽 params 이름은 `permission_mode`). **아무것도 안 주면 플래그가 아예 안 붙어** 자식은 사용자 자신의 Claude Code 설정(`~/.claude/settings.json` 의 `permissions.defaultMode` 등)대로 뜬다 — [codex](../codex/index.md) 가 미지정을 `never` 로 떨어뜨리는 것과 갈리며, 근거는 축의 개수다: codex 는 승인을 안 물어도 샌드박스 축이 남지만 Claude Code 에는 그 두 번째 축이 없어 "안 묻는다" 가 곧 봉쇄 없는 전권이 된다([ADR-0265](../../adr/0265-child-approval-policy-is-the-callers-choice.md)). 전역 기본값은 Settings › Plugin › Claude Code 의 **자식 세션 기본 권한 모드**(기본 `inherit` = 미부착)이고 호출별 플래그가 그 호출에 한해 이긴다. 모르는 값은 조용히 전달하지 않고 거부한다. `--profile`/`--profile-file` 이 가리키는 settings JSON 이 `permissions.defaultMode` 를 정하고 있으면 `--permission-mode` 와 **동시 지정이 거부**된다(둘이 같은 것을 정한다 — `permissions.allow`/`deny` 는 축이 달라 공존한다). `reboot`/`child-profile` 에서는 그 값이 **그 호출의 resume 명령에만** 실리고 `restore.command` meta 에는 안 들어간다 — 복원은 호출자가 없는 자리에서 일어나므로 한 번 고른 정책이 이후 모든 재부팅에 눌러앉지 않는다.

### Stop-훅 게이트 레지스트리

세션 종료를 막고 체크리스트를 주입하는 **게이트를 이름으로 등록**하는 계층. 게이트는 3요소로 이뤄진다 — **본문**(block 될 때 `reason` 으로 주입되는 지시), **센티넬**(모델이 종료를 선언하는 문자열), **라운드 상한**(백스톱). 위 세션 프로필 레지스트리(`profile.rs`)의 형태를 미러링하되 타입은 공유하지 않는다(`crates/tasty-plugin-claude/src/gate.rs`). 결정 배경은 [ADR-0083](../../adr/0083-stop-gate-named-registry.md).

- **등록**: `tasty claude gate-register <이름> --body-file <경로> [--sentinel <문자열>] [--rounds <n>]` — 본문 파일을 **복사본**으로 저장한다(원본이 옮겨지거나 지워져도 게이트는 살아 있다). 이미 등록된 이름이면 정의와 본문을 둘 다 덮어쓴다. 이름 규칙은 프로필과 동일(소문자/숫자/`-`, 최대 32자).
- **해제**: `tasty claude gate-unregister <이름>` — 정의와 본문 복사본을 **둘 다** 지운다(본문만 남으면 다음 등록이 옛 본문을 조용히 덮어쓰는 것처럼 보이는 orphan 이 된다). 그 게이트의 **런타임 상태**(`checklist/gates/<이름>/` 의 마커 + 라운드)도 함께 지운다 — 남겨 두면 같은 이름으로 재등록했을 때 과거의 켜짐 상태와 라운드 카운터가 부활해, "지웠다 새로 만든 게이트" 가 이전 상태를 물려받는다. 이 정리는 실패해도 해제 자체를 실패시키지 않는다(경고 로그만 — 레지스트리에서 사라지는 것이 주 목적이고, 남은 상태는 어차피 해석되지 않는다).
- **목록**: `tasty claude gate-list` — 등록 게이트(`user/<이름>`)와 host 기본 게이트(`host/continue-checklist`)를 함께, 각각의 실효 `sentinel`/`round_limit`, 상한의 출처(`round_limit_source`: `gate` = 정의가 직접 지정 / `settings` = 미지정이라 plugin 설정으로 폴백), 그리고 **마커 on/off**(`enabled`)를 함께 보여준다 — 게이트별 on/off 를 한 번에 보는 경로는 여기다(`checklist-status` 는 게이트 하나씩 답한다). 사용자가 host 기본 게이트와 같은 이름으로 등록했으면 그 이름은 **user 항목으로만** 나온다 — 같은 이름이 두 줄로 보이면 어느 쪽이 실효인지 목록만 봐서는 알 수 없다.
- **부착**: `tasty claude launch|spawn|respawn|reboot --profile <게이트이름>` — 게이트 이름은 프로필 이름과 같은 평면이라 `--profile` 로 그대로 부착된다. 부착 시 만들어지는 것은 그 게이트를 지목하는 `Stop` 훅 하나뿐이다:

  ```
  if [ -n "$TASTY_SURFACE_ID" ]; then tasty claude checklist-hook --gate <이름> || true; fi
  ```

  게이트의 3요소(본문·센티넬·상한)는 이 명령에 담기지 않는다 — 훅이 발화할 때 `--gate` 로 레지스트리를 다시 읽으므로, 게이트를 재등록해 본문을 고치면 **재부착 없이** 다음 발화에 반영된다. 명령 문자열은 `install.rs::tasty_guarded_command` 한 곳에서만 만들어진다(형태가 두 경로로 갈리는 것을 구조로 막는다). 게이트 이름이 셸 명령에 그대로 들어가는데 안전한 이유는 short-name 규칙(소문자/숫자/`-`)이 셸 메타문자를 원천 배제하기 때문이다 — 이름 규칙을 느슨하게 바꾸려면 인용/이스케이프를 함께 손봐야 한다.
- **조합**: `--profile gate-a,gate-b` 처럼 게이트 둘, 또는 `--profile mygate,myprofile` 처럼 게이트와 프로필을 섞어 부착하면 `profile_merge` 의 `hooks` concat 규칙에 따라 **Stop 훅이 각각 등록**된다(그래서 라운드 상태·마커가 게이트별이다).
- **조회**: `tasty claude gate-show <이름>` — 정의(센티넬·상한)와 본문 텍스트를 함께. 사용자 등록이 없으면 host 기본 게이트로 폴백하고, 그때 `owner` 는 실제 출처를 그대로 반영한다(`host`).

**등록 시점 검증**

- **본문은 그 게이트의 실효 센티넬을 포함해야 한다.** 없으면 거부 — 센티넬이 본문에 없으면 모델이 종료를 선언할 방법을 안내받지 못해 라운드 상한까지 무조건 도달하고, 게이트가 "N턴 강제 연장" 장치로 변질된다. host 기본 본문에 대해서는 같은 불변식을 로케일별 컴파일 타임 테스트가 강제한다(사용자 본문에는 컴파일 타임 테스트를 걸 수 없어 등록 시점 런타임 검증으로 옮긴 것).
- **빈 센티넬 거부** — 빈 문자열은 모든 메시지에 매칭되어(`str::contains("")` 는 항상 참) 게이트가 첫 라운드에 통과한다.
- **`--rounds` 는 1 이상.**
- **동명 세션 프로필과 충돌하면 거부** — 게이트를 등록하면 동명 프로필로 그대로 부착 가능해지므로 두 레지스트리는 **이름 공간을 공유**한다. 같은 이름이 양쪽에 생기면 조용히 한쪽이 가려지므로 등록 시점에 **양방향으로** 거부한다(`gate-register` 는 동명 registered 프로필을, `profile-register` 는 동명 게이트를).
- `data_dir` 이 없는 비정상 기동이면 등록/해제는 명시적 에러. **조회(list/show)는 host 기본 게이트만 반환**한다 — 조회는 저장소를 요구하지 않는다.

**저장 위치** — `TASTY_PLUGIN_DATA_DIR` 하위, 프로필 레지스트리와 같은 "사용자 원본 vs tasty 생성물" 분리 방침:

| 경로 | 내용 |
|---|---|
| `gates/registered/<이름>.json` | 게이트 정의 — `sentinel`(등록 시 미지정이면 기본 센티넬이 실체화된다: 정의만 보고도 실효값을 알 수 있어야 한다) · `round_limit`(미지정이면 키 자체가 없다) |
| `gates/bodies/<이름>.md` | 본문 원본의 복사본 |

**host 기본 게이트는 파일이 아니라 코드다** — `continue-checklist` 는 데이터 디렉토리에 실체화되지 않고 조회 함수로만 존재한다(본문 = `claude.checklist.body` 번역 키, 센티넬 = 기본 센티넬, 상한 = 미지정 → 설정 폴백). 실체화하면 사용자가 지웠을 때 되살릴 경로가 없어진다(`install.rs::MANAGED_HOOKS` 와 같은 형태). 사용자가 같은 이름으로 등록하면 그쪽이 이긴다.

- IPC: `claude.gate_register`/`claude.gate_unregister`/`claude.gate_list`/`claude.gate_show` — CLI 서브커맨드와 1:1 대응(원칙 2). GUI 노출은 없다.

### Stop-훅 게이트 판정

등록된 게이트를 실제 `Stop` 훅 발화에서 집행하는 층(`crates/tasty-plugin-claude/src/checklist.rs`). 판정 자체는 4분기 그대로고(아래 continue-checklist 절), 3요소(본문·센티넬·상한)를 **게이트별로** 가져온다.

- **어느 게이트인지는 명령 인자로 온다** — `tasty claude checklist-hook --gate <이름>`. Stop payload(`session_id`/`prompt_id`/`stop_hook_active`/`last_assistant_message`)에는 게이트를 식별할 정보가 없어서, 부착 시점에 훅 명령 문자열에 박는 것이 유일한 경로다. `--gate` 는 optional 이고 기본값이 `continue-checklist` 라, `--gate` 없이 이미 설치돼 있는 훅 명령도 그대로 host 기본 게이트로 동작한다.
- **라운드 상태 경로**: `TASTY_PLUGIN_DATA_DIR/checklist/gates/<게이트>/rounds/<session_id>.json`. 키가 (게이트 × 세션)인 이유는 **게이트를 둘 이상 동시에 부착할 수 있기 때문**이다 — `--profile a,b` 의 머지 규칙상 `hooks` 배열은 concat 이라 두 게이트의 Stop 훅이 각각 등록되고 각각 발화한다. 게이트를 구분하지 않으면 둘이 한 카운터를 읽고 써서 서로의 라운드를 깎는다(세션 축을 도입한 것과 같은 이유가 한 축 위로 올라온 것).
- **라운드 상한 우선순위**: 게이트 정의(`--rounds`) **>** Settings(게이트 기본 라운드 상한) **>** 3. 명시 지정이 전역 기본값을 이기는 일반 원칙이다 — `--rounds 5` 로 등록한 게이트가 Settings 값에 조용히 덮이면 등록 인자가 무의미해진다. **이 폴백은 게이트 출처(host 기본 / 사용자 등록)를 구분하지 않는다** — 상한을 지정하지 않은 게이트는 어느 쪽이든 Settings 값으로 내려가고, `gate-list` 의 `round_limit_source` 가 그때 `settings` 로 나온다. Settings 항목의 storage key 이름(`continue_checklist_round_limit`)은 host 기본 게이트 전용처럼 보이지만 **의미는 전 게이트 공용 기본값으로 재정의됐다**(사용자에게 보이는 라벨이 "게이트가 자체 값을 지정하지 않았을 때의 기본값" 이라고 안내한다; 키 자체는 조정해 둔 값 유실 방지를 위해 그대로 뒀다). 키 이름대로 좁게 해석해 사용자 게이트를 곧장 3 으로 떨어뜨리면 매 게이트마다 `--rounds` 를 명시하지 않는 한 **사용자 게이트의 기본 상한을 조절할 수단이 없어지므로** 그렇게 하지 않는다. host 기본 게이트도 상한을 지정하지 않아 같은 폴백을 타므로 **기존 동작이 그대로 보존**된다.
- **본문의 `{{goal}}` placeholder (opt-in)**: 본문에 `{{goal}}` 토큰이 있으면, 훅이 그 자리를 **발화 surface 의 goal 절**로 치환한 결과를 `reason` 으로 낸다. goal 은 훅이 직접 `memory.goal_get` 으로 읽는다 — 본문에 "goal 을 조회해라" 라고 적어 모델에게 맡기면 게이트의 핵심 판정 근거 획득이 모델의 성실성에 의존하게 되고, 게이트의 존재 이유(모델의 자기판단을 믿지 않는다)와 어긋난다. 3분기다: goal 있음 → 토큰 자리에 goal 절(`claude.checklist.goal_clause`, goal 텍스트 삽입) · goal 없음 → 토큰이 있던 **줄째** 제거(나머지 본문은 그대로 = 토큰 도입 전과 바이트 단위로 동일) · **토큰 미포함 본문 → 완전 무변화**(등록 게이트 하위호환 — 저자 본문에 예고 없이 남의 문장이 붙지 않는다). goal 조회 IPC 는 **block 이 확정되고 그 본문에 토큰이 있을 때만** 하므로, 통과하는 발화와 토큰 없는 게이트는 IPC 를 한 번도 하지 않는다. 조회가 실패하면(호스트 IPC 오류, surface id 부재 등) goal 없음과 동일하게 취급한다 — 이 모듈의 "불확실하면 통과/무시" 방침 그대로다. surface id 는 `checklist_hook_args` 의 `surface`(u32, optional) 인자로 오는데, CLI 층이 미지정 시 `TASTY_SURFACE_ID` env 로 채우므로 **`--surface` 없이 이미 설치돼 있는 훅 명령 문자열도 그대로 동작한다**(`--gate` 가 지킨 하위호환과 같은 성질). 결정 근거는 [ADR-0088](../../adr/0088-stop-gate-goal-aware-continuation.md).
- **본문 해석**: 등록 게이트 본문은 **매 발화마다 파일에서 읽는다** — 재등록으로 갱신한 본문이 세션 재기동 없이 반영되어야 한다(마커를 매 발화마다 확인하는 것과 같은 취지). host 기본 게이트 본문만 기동 시 1회 해석해 둔 lang 문자열 캐시를 계속 쓴다.
- **미등록 게이트는 조용히 통과** — 등록이 지워졌는데 훅 명령이 남아 있는 세션은 정상적인 상태다. 여기서 에러를 내면 그 세션이 종료 불가가 되므로, 이 모듈의 기존 "불확실하면 통과" 방침을 그대로 따른다. 상태 파일도 만들지 않고, Settings 조회 IPC 도 하지 않는다.
- **SessionEnd 정리는 게이트 전체 순회** — 전역 `session-end` 훅은 `MANAGED_HOOKS` 로 항상 설치되고 게이트와 무관하게 발화해서 호출부가 게이트를 알 수 없다. 그래서 `checklist/gates/*/rounds/<session_id>.json` 을 전부 지운다(다른 세션 파일은 건드리지 않는다).
- **legacy 경로** — 게이트 축 이전의 `checklist/rounds/<session_id>.json` 은 읽지도 쓰지도 않는다. 라운드 상태는 세션 수명과 함께 사라지는 휘발성 데이터라 마이그레이션하지 않지만, 구버전이 남긴 파일이 orphan 으로 남지 않도록 **session-end 정리는 이 경로도 함께 지운다**.

**Stop 훅 여러 개가 동시에 block 할 때 (실측)** — 게이트 둘을 한 세션에 부착하는 시나리오의 체감이 여기 달려 있어 Claude Code 로 직접 재현했다(`Stop` 에 독립 훅 2개를 등록하고 각각 다른 `reason` 으로 block):

- 두 훅은 **매 `Stop` 발화마다 둘 다 발화한다**(발화 횟수가 lockstep 으로 일치).
- **두 `reason` 이 모두 모델에 전달된다** — 각 훅이 서로 다른 토큰을 최종 답변에 넣으라고 지시했을 때 두 토큰이 모두 답변에 나타났다. 한 훅만 채택되는 방식이 아니다.
- **하나라도 block 이면 턴이 이어진다** — 상한이 다른 두 훅(1회 / 3회)을 걸면, 먼저 상한에 도달한 쪽이 통과(`{}`)로 돌아선 뒤에도 아직 block 하는 쪽 때문에 세션이 계속됐고, **둘 다 통과한 발화**에서 끝났다.
- 두 번째 발화부터 두 훅 모두 `stop_hook_active=true` 를 받는다(플러그인은 이 값을 sanity check 로만 쓰고 판정에 쓰지 않는다).

### continue-checklist 세션 프로필

**게이트 프리미티브 위의 host 기본 인스턴스 하나**다 — 고유명으로 특별 취급되는 기능이 아니라, 위 게이트 레지스트리가 host 출처로 내장한 게이트(`host/continue-checklist`) 이고 부착 경로도 등록 게이트와 완전히 같다. `--profile continue-checklist` 로 부착하면(사용자가 같은 이름으로 프로필을 직접 등록했으면 그쪽이 우선한다) `Stop` 훅으로 `tasty claude checklist-hook --gate continue-checklist` 를 심는다 — 전역 `install`(위 "Claude Code 훅 통합" 절의 9종)에는 포함되지 않으며, 부착된 세션에서만 발화한다. 아래 설명은 이 기본 인스턴스의 값(본문·센티넬·상한 폴백)을 기준으로 하며, 등록 게이트는 같은 자리에 자기 값을 쓴다.

- **동작**: 매 `Stop` 훅 발화마다 stdin JSON(`session_id`/`prompt_id`/`stop_hook_active`/`last_assistant_message`)을 읽어 4분기로 판단한다: ① 저장된 `prompt_id` 와 다르면(또는 저장 상태 없음) 라운드 0 으로 취급 ② `last_assistant_message` 에 **이 게이트의** 센티넬(host 기본값 `[[TASTY-CHECKLIST-DONE]]`)이 포함되면 통과 ③ 라운드 수가 상한에 도달했으면 통과(백스톱) ④ 그 외엔 `{"decision":"block","reason":"<체크리스트 본문>"}` 을 반환하고 라운드 +1. `reason` 본문은 `t("claude.checklist.body")`(lang 파일, 3개 언어)로 활성 locale 로 해석된 문자열이며 3개 범용 항목(결과가 요청을 충족했는지 재검토 / 코드·설정 변경을 실제로 검증했는지 / 후속 작업 유무 명시) · `{{goal}}` placeholder 단락 · 센티넬 포함 지시로 구성된다. 3번 항목은 **공시 요구**지 완료 요구가 아니라서 "남은 작업 A, B 가 있습니다" 라고 밝히기만 해도 충족된다 — goal 절이 그 뒤에서 "밝힌 남은 작업을 goal 에 비추어 판정하고, 사용자 판단이 필요 없고 스스로 진행 가능한 것은 여기서 끝내지 말고 진행하라" 는 후속 지시를 준다(3번 항목 문구 자체는 바꾸지 않는다). **goal 이 설정돼 있지 않으면 그 단락이 통째로 사라져 기존 동작 그대로다** — goal 부재는 사용자가 자율 진행 범위를 선언하지 않았다는 뜻이므로 계속-진행을 지시할 근거가 없다. goal 은 `tasty memory goal set "<문장>" --surface <id>` 로 설정한다([memory](../../design/systems/memory.md)).
- **라운드 상한 백스톱이 필요한 이유**: Claude Code 자체엔 `Stop` 훅의 block 을 무한 반복해도 막아주는 host 측 안전장치가 없다(실측 확인 — 모델이 루프에 갇혔음을 스스로 인지해도 탈출하지 못했다). 상한은 Settings › Plugin › Claude Code 의 게이트 기본 라운드 상한 항목(기본 3)으로 노출된다 — 게이트가 자체 `--rounds` 를 지정하면 그쪽이 이긴다.
- **라운드 상태 저장**: `TASTY_PLUGIN_DATA_DIR/checklist/gates/continue-checklist/rounds/<session_id>.json` — (게이트 × 세션)으로 키잉한다(위 "Stop-훅 게이트 판정" 절). 해당 세션의 `SessionEnd` 훅(아래 "Claude Code 훅 통합" 절의 9종 중 하나, 이 프로필과 무관하게 항상 발화)이 오면 상태 파일을 정리한다.
- **마커 파일 게이트 (게이트별)**: `TASTY_PLUGIN_DATA_DIR/checklist/gates/<게이트>/enabled.marker` — 존재 여부로 그 게이트의 발동을 켜고 끈다. 프로필 attach(=훅 등록)는 Claude Code 프로세스 기동 시점에 고정되지만, 마커는 매 훅 발화마다 파일 존재를 새로 확인하므로 재기동 없이 즉시 토글된다. 마커가 게이트별인 이유도 이 지점이다 — 게이트를 여럿 붙여 두고 마커가 하나면 즉시 토글이 전부-아니면-전무가 되어 게이트를 나눈 의미가 토글 축에서만 사라진다. 라운드 상태와 같은 `gates/<게이트>/` 아래 두어 게이트 하나의 런타임 상태가 한 디렉토리에 모인다.
  - `checklist-enable [--gate <이름>]` / `checklist-disable [--gate <이름>]` / `checklist-status [--gate <이름>]` CLI(및 대응 IPC)가 마커를 만들고 지우고 조회하는 제어된 진입점이다 — raw `touch`/`rm` 로 직접 조작할 필요가 없다. **`--gate` 를 생략하면 `continue-checklist`** 라서 게이트 축이 생기기 전의 무인자 호출이 그대로 동작하고, `checklist-status` 응답도 기존과 같은 `{ "enabled": bool }` 이다.
  - **enable/disable 은 미등록 게이트 이름을 거부한다**(오타로 만든 마커 디렉토리가 `gate-list` 에도 안 보이는 유령 상태로 쌓이는 것을 막는다). 반대로 **조회(`checklist-status`)와 발동(훅) 경로는 관대하다** — status 는 `enabled: false` + `registered: false` 로 답하고, 훅은 조용히 통과한다(등록이 지워졌는데 훅 명령이 남은 세션에서 에러를 내면 그 세션이 종료 불가가 된다).
  - **legacy 마커 1회 이관**: 게이트 축 이전 경로(`checklist/enabled.marker`)에 마커가 있으면 `gates/continue-checklist/enabled.marker` 로 옮기고 원본을 지운다. enable/disable/status/훅 어느 진입점을 타도 수행되므로 명령을 한 번도 부르지 않고 훅만 도는 인스턴스에서도 이관된다. 라운드 상태와 달리 마커는 사용자가 명시적으로 켜 둔 설정이라, 업그레이드하면서 조용히 꺼지면 회귀로 보인다.
- **안전한 통과 원칙**: 마커 부재, `session_id`/`prompt_id` 누락, stdin 파싱 실패 등 불확실한 조건은 전부 block 하지 않고 조용히 통과시킨다 — 판단 불가 상태에서 세션을 가두지 않는 것을 우선한다.
- IPC: `claude.checklist_hook` — `hook_args` 와 별개 파라미터 스키마(`checklist_hook_args`). Stop payload 필드는 전부 stdin 자동 채움이고, `gate` 만 stdin 에 없어 명령 인자로 받는다(기본값 `continue-checklist`). `stop_hook_active` 는 `bool` 이 아니라 `string` 타입으로 선언돼 있다 — `CliArgType::Bool` 은 부재를 표현하지 못해(`extract_value` 가 항상 `Some(false)` 를 반환) `stdin_field` 매핑과 함께 쓰면 stdin 값이 절대 반영되지 않는다(핸들러가 `"true"`/`"false"` 문자열을 직접 비교). `claude.checklist_enable`/`claude.checklist_disable`/`claude.checklist_status` — `checklist_gate_args`(`--gate`, optional, 기본값 `continue-checklist`), 그 게이트의 마커 파일 생성/삭제/조회. status 응답은 `{ "enabled": bool, "registered": bool }` — `enabled` 는 기존 그대로이고, `registered`(그 이름의 게이트가 실재하는가 — host 기본 게이트도 `true`)는 조회가 오타를 거부하지 않는 대신 알아볼 수 있게 하는 필드다. `data_dir` 이 없는 비정상 기동이면 enable/disable 은 명시적 에러로 거부하고(profile.rs 결정 3과 동일 방침), status 는 `marker_present` 와 동일하게 `enabled: false` 로 안전 폴백한다(에러 아님 — 조회는 항상 응답 가능해야 한다).

### Claude Code 훅 통합

**훅 응답은 조용히 실패한 host 호출 수를 싣는다** — `host_call_failures`(항상 있고 항상 수). 이 핸들러의 host 호출은 전부 최선노력이라(`?` 로 끊지 않는다 — 뒤따르는 로컬 정리를 지키기 위해서다, [ADR-0172](../../adr/0172-a-hook-handler-that-cleans-up-locally-does-not-propagate.md)) 실패해도 응답은 `ok` 다. 그 수 없이는 전부 실패한 훅과 전부 성공한 훅이 바이트까지 같다. 0 이 아니면 `<tasty_home>/hook-failures.log` 에도 한 줄 남는다 — 실사용에서 이 CLI 는 훅 명령 안에서 돌고 그 명령은 출력을 버리기 때문이다. 규약은 [error-handling](../../dev-guide/error-handling.md) "최선노력의 대가는 치르되 값으로 노출한다".

`tasty claude install`이 `~/.claude/settings.json`의 `hooks`에 아래 9개 이벤트를 심는다. 모든 이벤트가 같은 형태의 명령 문자열을 쓴다:

```
if [ -n "$TASTY_SURFACE_ID" ]; then tasty claude hook <token> || true; fi
```

**가드와 실패 처리는 분리돼 있다.** 바깥 `if` 는 "tasty 밖에서 Claude Code 를 쓰는 환경"(`$TASTY_SURFACE_ID` 미설정)을 **명시적 성공 종료**로 처리해 아무 소음도 내지 않는다. 안쪽 `|| true` 는 오직 `tasty claude hook` 자체의 실패만 담당한다 — 에이전트 턴을 막지 않기 위해 exit 0 을 유지하되, **실패 사실은 버리지 않고** `<tasty_home>/hook-failures.log` 에 한 줄 기록한다([ADR-0075](../../adr/0075-agent-hook-delivery-failure-record.md)). 옛 형태(`[ -n … ] && … || true`)는 두 경우를 한 연산자로 함께 삼켜 상태 push 유실이 무흔적으로 사라졌다.

명령 문자열 생성은 `install.rs::tasty_guarded_command` 한 곳뿐이다 — 세션 프로필(`continue-checklist`)의 hook 명령도 같은 함수를 쓴다(예전엔 `profile.rs` 에 같은 형태가 따로 하드코딩돼 있어 한쪽만 고치는 사고가 났다).

> **기존 사용자는 `tasty claude install` 재실행이 필요하다.** 명령 문자열은 사용자의 `settings.json` 에 이미 기록돼 있어, plugin 을 업데이트해도 옛 문자열 그대로다. 재실행하면 marker(`tasty claude hook <token>`) 가 일치하는 기존 entry 를 찾아 **제자리 갱신**하므로 entry 가 중복되지 않는다.

`session_id`/`message`/`notification_type`/`error`/`agent_id` 같은 이벤트별 가변 데이터는 명령 인자가 아니라 **stdin JSON**으로 들어온다 — 매니페스트 `hook` cli 항목이 `stdin_json = true`를 선언하고, `--session`/`--message`/`--notification-type`/`--error`/`--agent-id` 플래그가 각각 `stdin_field`로 stdin JSON에서 자동 채워진다(Claude Code가 hook 실행 시 stdin으로 JSON payload를 준다). POSIX 셸 구문 1종만 발행한다 — [codex](../codex/index.md)처럼 Windows PowerShell 분기는 없다.

| Claude Code 이벤트 | matcher | tasty hook token | `terminal.set_state` | `surface.fire_hook` | surface meta | `surface.completion` kind |
|---|---|---|---|---|---|---|
| `Stop` / `SubagentStop` | `""`(전체) | `stop` / `subagent-stop` | `idle` | `claude-idle` | — | `completion` |
| `StopFailure` | `""`(전체) | `stop-failure` | `idle` | `claude-idle` + `claude-stop-failure` | `claude-last-stop-failure` = stdin `error`(없으면 `unknown`) **set** | `completion` |
| `SessionEnd` | `""`(전체) | `session-end` | `idle` | `claude-idle` | `claude-session-id`·`restore.command`·`claude-last-stop-failure` **unset** (프로필 meta 2키는 건드리지 않는다. 프로필 **부착 기록**에는 종료 표시만 하고 유예 뒤 회수 — 아래 "복원을 건너 프로필이 유지되는 방식") | `completion` |
| `Notification` | `""`(전체) | `notification` | `needs_input`(단 `notification_type`이 `idle_prompt`면 건너뜀 — 무입력 대기 오탐이라 실제 질문 없음) | `needs-input`(동일 조건) | — | `needs_input`(동일 조건) |
| `UserPromptSubmit` | `""`(전체) | `prompt-submit` | `active` | — | `claude-last-stop-failure` **unset** | — |
| `SessionStart` | `""`(전체) | `session-start` | `active` | — | `claude-last-stop-failure` **unset**. `claude-session-id` = 세션 ID, `restore.command` = `claude -r <id>` **set**(stdin JSON에 `session_id`가 없으면 건너뜀). 프로필이 부착돼 있으면 `claude -r <id> --settings "<경로>"` 로 쓰고, 복원으로 프로필 meta 가 사라졌으면 부착 기록에서 **복구**한다(아래 "복원을 건너 프로필이 유지되는 방식") | — |
| `PreToolUse` | `AskUserQuestion` | `pre-tool-use` | `needs_input` | `needs-input` | — | `needs_input` |
| `PostToolUse` | `AskUserQuestion` | `post-tool-use` | `active` | — | — | — |

`surface.completion` 은 `{ surface_id, kind }` 로 호출되며(`HostCall::SurfaceCompletion`,
`hook.rs`), `kind` 는 위 표의 값을 그대로 싣는다 — 호스트의 `AttentionStore` 가
`NeedsInput` 을 `Completion` 보다 높은 우선순위로 표시한다(surface 테두리 노랑, 탭
제목·워크스페이스 배지도 동일 우선순위, [surface-highlight](../../features/surface-highlight/index.md)
참고).

`UserPromptSubmit`은 child가 2번째 이후 prompt를 받을 때 직전 `Stop` hook이 남긴 `idle=true` 잔재를 지우는 데 필수다 — 미등록 시 실제로는 active인 child를 idle로 오보고하는 상태 버그가 생긴다. `PreToolUse`/`PostToolUse`만 matcher `AskUserQuestion`으로 좁혀 등록돼 그 툴 호출에만 발화한다(나머지 7개는 matcher `""`로 이벤트 전체를 받는다) — 실측(실제 Claude Code를 띄워 hook stdin payload를 덤프해 확인) 결과 `AskUserQuestion` 답변은 `UserPromptSubmit`을 발생시키지 않으므로(질문/답변이 같은 prompt turn 안의 tool 상호작용이라 새 프롬프트로 집계되지 않음), 기존 `UserPromptSubmit`(→active)만으로는 이 케이스의 needs_input 해제 시점을 잡을 수 없다. `PreToolUse`가 질문 UI가 뜨기 **전에** 발화해(`tool_input.questions` 포함) needs_input을 켜고, `PostToolUse`가 답변 즉시(관찰상 `duration_ms: 0`) 그 짝으로 active로 되돌린다 — `needs_input`은 이제 `Notification`과 `PreToolUse` 두 경로에서 나온다.

`StopFailure`는 API 에러(재시도를 다 쓴 `529 Overloaded` · rate limit · 인증 실패 …)로 턴이 끝날 때 Claude Code가 `Stop` **대신** 발화한다 — 그 턴에는 `Stop`이 오지 않는다. 이것이 없으면 실패로 끝난 턴에 턴 종료 신호가 하나도 오지 않아 상태가 직전 `UserPromptSubmit`의 `active`에 머문다(`UserPromptSubmit` 미등록 때와 같은 부류의 오보고). 턴은 끝났고 Claude는 입력을 기다리므로 상태는 `idle`이고, **`claude-idle`도 함께 쏜다** — 부모 완료 알림 3형제가 그 키를 구독하므로 빠뜨리면 부모가 아무 알림도 못 받는다. 그와 별도로 `claude-stop-failure`를 쏘고, 에러 종류(stdin JSON `error` — Claude Code가 matcher 값으로 선언한 `rate_limit`/`overloaded`/`authentication_failed`/`billing_error`/`invalid_request`/`server_error`/`max_output_tokens`/`unknown` 등)를 surface meta `claude-last-stop-failure`에 남긴다. surface hook의 `Custom` payload는 이벤트 키만 싣고 값은 못 싣기 때문이다([hooks](../../features/hooks/index.md)). meta는 fire보다 먼저 쓰고, 새 턴(`prompt-submit`/`session-start`/`active`)과 `session-end`에서 지운다 — 있는지 묻지 않고 매번 지우는 것은 plugin 재시작이 "지난 턴이 실패했다"는 메모리를 잃어도 지난 에러가 다음 알림에 붙지 않게 하려는 것이다. 완료 알림(`notify-done`)은 이 meta가 있으면 문구 **뒤에** 에러 종류를 덧붙인다(`… task complete (via spawn) — the turn ended on an API error (overloaded) and is waiting for input`) — 앞부분은 그대로라 기존 문구로 거르던 부모는 그대로 동작한다. **서브에이전트의 실패는 무시한다.** `Stop`은 서브에이전트용 `SubagentStop`이 따로 있지만 `StopFailure`는 하나뿐이고, 그것을 조립하는 경로는 질의 루프 공용이라 서브에이전트(Agent 툴 호출)의 API 실패에도 불린다. 그때 메인 턴은 그 실패를 tool 결과로 받고 계속 돈다. 구분은 훅 payload 공통부의 `agent_id`로 한다 — 서브에이전트 문맥에서만 값이 있고 메인 스레드에는 필드가 없다(Claude Code 2.1.280 바이너리의 payload 조립부로 확인). `agent_id`가 실린 `stop-failure`는 상태·알림·meta를 전부 건드리지 않고 로그 한 줄만 남긴다.

install이 심는 훅은 이 9개뿐이다 — matcher가 지정되지 않은 `PreToolUse`/`PostToolUse` 호출 전체나 `PreCompact` 등 다른 Claude Code 이벤트는 걸지 않는다. `install.rs`의 `install_preserves_other_hooks` 테스트가 사용자가 직접 추가한(matcher가 다른) `PreToolUse` entry를 tasty의 `AskUserQuestion`-matcher entry와 분리해 그대로 보존함을 검증한다.

install은 marker substring(`tasty claude hook <token>`)으로 자기 entry를 식별해 멱등하게 동작한다 — marker가 일치하는 기존 entry는 명령 문자열만 최신 형태로 덮어쓰고(옛 버전이 심은 잘못된 명령이 남는 회귀 방지), 사용자가 직접 추가한 다른 entry는 건드리지 않는다. `PreToolUse`/`PostToolUse`처럼 matcher가 있는 이벤트는 marker 일치만으로는 matcher 값까지 보증되지 않으므로, install이 matcher도 canonical 값(`AskUserQuestion`)으로 함께 갱신한다.

이 플러그인이 fire하는 surface hook 이벤트는 `claude-idle`/`needs-input`/`claude-stop-failure`/`claude-error`/`claude-error-stalled` 5개이며, 매니페스트 `contributes.hook_events`로 선언한다 — host가 (내장 ∪ 활성 plugin 선언) 집합으로 `hook.set` 등록을 검증하므로([hooks](../../features/hooks/index.md)), 이 플러그인이 비활성이면 저 5개 키로의 hook 등록도 거부된다. **이 5개가 전부 위 9개 설치 훅에서 나오는 건 아니다** — `claude-idle`은 위 `apply_hook`(Stop/SubagentStop/StopFailure/SessionEnd)에서, `claude-stop-failure`는 `StopFailure`에서, `needs-input`은 `Notification`(idle_prompt 제외)과 `PreToolUse`(matcher `AskUserQuestion`) 두 경로에서 나오지만, `claude-error`/`claude-error-stalled`는 이 훅 메커니즘과 무관한 별도 producer다: `error_scan.rs`가 surface 출력 텍스트를 패턴 매칭해 매치 시 직접 `surface.fire_hook`으로 발사한다(정지 판정은 아래 절). API 에러로 턴이 **끝나면** `StopFailure`가 구조적 신호를 주지만, 요청이 응답 없이 매달리면 턴이 끝나지 않아 `Stop`도 `StopFailure`도 **발화하지 않는다** — 그때는 PTY에 찍히는 에러 문자열과 출력 정적이 얻을 수 있는 유일한 신호다. `claude-idle`/`needs-input`은 [surface-highlight](../../features/surface-highlight/index.md)(Stop hook → highlight)와 [telemetry](../../features/telemetry/index.md)(`session-start`→`stop`의 `wall_time_ms`, `notification`의 `input_tokens`)가 소비하고, `SessionStart`/`SessionEnd`의 meta set/unset은 [layout-persistence](../../features/layout-persistence/index.md)의 `restore.command` 복원이 소비한다.

### API 에러 뒤 자동 재개 (`auto_resume.rs`)

API 에러로 끝난 턴(위 `StopFailure`)을 설정된 지연 뒤에 재개 문구 하나로 이어 준다. **기본 꺼짐**이다 — 사용자 세션에 텍스트를 넣는 기능이라 opt-in 이다. 근거·대안·재검토 조건은 [ADR-0521](../../adr/0521-claude-auto-resume-after-an-api-error-is-opt-in-and-judged-at-send-time.md).

| 설정(`storage_key`) | 종류 | 기본 | 뜻 |
|---|---|---|---|
| `auto_resume_enabled` | toggle | `false` | 켜야 동작한다 |
| `auto_resume_delay_secs` | number(최소 1, 최대 86400) | 10 | 턴이 끝난 뒤 재개까지 기다리는 초. 범위 밖 값은 코드가 다시 자른다(설정 파일을 손으로 고친 경우) |
| `auto_resume_max_attempts` | number(최소 1) | 5 | 연속으로 보내는 재개 문구 수의 상한 |

- **대상 에러**: `overloaded` · `server_error` 뿐이다. `rate_limit` 은 제외한다(해제가 분~시간 단위라 몇 초 뒤 재개는 다시 실패한다). 인증·과금·요청 형식·출력 한도 등은 다시 보내도 같아 제외한다. 서브에이전트(`agent_id` 가 실린)의 실패는 메인 턴이 계속되므로 예약하지 않는다.
- **예약**: `stop-failure` 가 오면 설정을 읽어 켜짐 · 대상 에러일 때만 만기 시각을 적는다(같은 surface 의 예약은 덮어쓴다). 새 턴(`prompt-submit` · `session-start` · `active`) · 성공 `stop` · `session-end` 가 예약을 지운다 — 사람이 먼저 입력하면 그 입력의 `prompt-submit` 이 지운다. hook 핸들러는 잠들지 않고, 만기는 전용 스레드 `claude-auto-resume`(500 ms 주기)가 처리한다.
- **보내기 직전 확인**(전부 참이어야 보낸다): 설정이 여전히 켜짐(예약 뒤 끄면 안 나간다) · `terminal.state` 가 `idle` · 전경이 `claude` 이고(대소문자 무시, 끝의 `.exe` 는 떼고 비교 — Windows 의 `claude.exe`) pid 가 예약 때와 같음 · **실패한 턴이 시작된 뒤 사용자 입력이 없음** · 연속 상한 미만. 마지막으로 예약 이후 턴 경계가 없었는지 한 번 더 본다.
- **사용자 입력 가드**: `surface.is_typing` 의 `idle_seconds`(마지막 사용자 입력 뒤 경과)를 그 턴의 `prompt-submit` 이후 경과와 견준다 — 작으면 입력창에 초안이 있을 수 있으므로 취소한다. `typing`(최근 5 초)만 보면 초안을 쓰다 멈춘 사용자를 놓친다. 이 조건을 통과했는데 `typing` 이 참이면 5 초 미루고 시도 수를 쓰지 않는다. 사용자 입력은 GUI 의 키보드·IME·붙여넣기(단축키·명령 팔레트)만 기록하므로 에이전트의 `send`/`tell` 은 이 가드에 걸리지 않고, 붙여넣은 초안은 걸린다([ADR-0560](../../adr/0560-paste-is-user-input-and-is-recorded-where-both-paste-paths-meet.md)). 조회가 실패하면 입력이 있었던 것으로 본다.
- **제출**: `terminal.tell`(본문 write 확인 → 정착 지연 → Enter, 원격 attach 점유 surface 거절). 거절되면 다시 걸지 않는다. 문구는 i18n 키 `claude.auto_resume.message` 고정이다.
- **상한**: 연속으로 보낸 수가 상한에 닿으면 보내지 않고 `notification.create` 로 한 번 알린다(i18n `claude.auto_resume.limit_*`). 계수는 성공 `stop` 과 새 세션에서 0 이 되고, 재개 문구 자신의 `prompt-submit` 은 계수를 지우지 않는다. 가짜 서버처럼 매번 실패하면 요청은 처음 1 + 재개 상한만큼이다.
- **흔적**: 보낼 때마다 `tracing::info!` 한 줄과 surface meta `claude-auto-resume-count`(연속 재개 수 — 성공 턴 · 새 세션 · 세션 종료가 지움)를 남긴다. 부모 완료 로그에는 실패한 턴마다 에러 종류가 붙은 완료 줄이 이미 가므로 따로 쓰지 않는다.

### PTY 에러 스캔 (`claude-error`) 범위

`error_scan.rs`는 800ms 주기 폴링으로 추적 대상 surface 마다 `surface.read_since_scan_mark`(strip-ansi)를 읽어 알려진 네트워크/API 에러 패턴(`API Error` / `Output blocked by content filtering policy` / `overloaded_error` / `rate_limit_error` / `Internal Server Error` / `network error` / `Bad Request`)을 매칭하고, 매치 시 그 surface 에 `claude-error` 를 fire 한다. 같은 텍스트가 연속 폴링에서 다시 잡히면 발화하지 않으며(dedupe), 새 턴 시작 신호(`prompt-submit`/`session-start`/`active`)에 dedupe 가 풀린다.

추적 대상은 **`claude launch` 로 만든 top-level surface 와 `claude spawn`/`claude respawn` 으로 만든 자식 surface 전부**다. 사람이 화면을 보고 있지 않은 자식이야말로 감지가 가장 필요한 대상이므로 자식을 제외하지 않는다.

정리(추적 해제)는 별도 구독 없이 같은 폴링 주기에 편승하되, **등록 경로에 따라 생존 판정 기준이 다르다**:

| 대상 | 등록 | 생존 판정 |
|------|------|-----------|
| top-level (`launch`) | child registry 에 없음 | `surface.locate` 로 surface 존재 확인 |
| 자식 (`spawn`/`respawn`) | 호스트 child registry | `terminal.parent` 로 **부모-자식 관계** 존재 확인 |

자식을 관계로 판정하는 이유는 [`terminal.release`](../../features/child-terminal/index.md)가 surface 를 닫지 않고 관계·soft 점유만 해제하기 때문이다 — surface 존재만 봤다면 release 후에도 영원히 폴링되며, 더 이상 자식이 아닌 사용자 터미널에 `claude-error` 를 계속 발화한다. 호스트가 관계 조회 전 `reconcile_child_terminals()` 를 돌리므로 이 한 번의 조회가 kill/close 실패로 surface 가 살아남은 케이스까지 함께 걷어낸다. `claude kill` 은 성공 응답의 `killed_surface_id` 로 즉시 `disable` 해 최대 800ms 의 잔여 발화 창까지 없앤다. 조회 자체가 실패(IPC 오류)하면 "죽었다"로 단정하지 않고 추적을 유지한다 — 재활성화 경로가 없어 오탐 정리가 오탐 유지보다 위험하다.

**스캐너는 에이전트와 다른 커서를 쓴다.** `surface.read_since_scan_mark` 는 에이전트의 mark(`tasty set mark` · `tasty read since-mark` · `parse-since-mark` 가 쓰는 것)와 **별개 커서**를 읽고, 읽을 때마다 읽은 자리 끝으로 전진한다. 그래서 ① 에이전트가 `tasty set mark` 를 걸어도 스캐너의 관측 창이 안 움직이고 ② 폴링 1 회가 나르는 것은 지난 800ms 에 새로 온 바이트뿐이다(한때는 아무도 mark 를 안 세운 surface 에서 폴링마다 버퍼 전체가 실렸다). 반대 방향도 닫혀 있다 — 스캐너의 읽기는 에이전트의 mark 를 안 움직인다.

커서가 전진하므로 한 호출이 주는 것은 화면 전체가 아니라 **델타**다. 패턴 매칭이 볼 창은 plugin 이 누적해 두고, 그 상한은 호스트 출력 버퍼의 상한과 같은 값을 따로 적은 사본이다. **이 커서의 소비자는 하나라는 전제 위에 있다** — 둘이 같은 이름을 부르면 서로의 바이트를 먹고 그 손실은 조용하다. 그래서 CLI 동사가 없다. 근거·대안·재검토 조건은 [ADR-0307](../../adr/0307-the-output-scanner-reads-its-own-cursor.md).

### 정지 알림 (`claude-error-stalled` → 부모 completion-log)

`claude-error` 자체는 **부모에게 알리지 않는다.** 패턴에 `overloaded_error`/`rate_limit_error`처럼 Claude Code가 자동 재시도하는 일시적 에러가 포함돼 있어, 그대로 알리면 재시도가 잦은 세션에서 알림이 쏟아진다. 대신 스캐너가 "재시도 중"과 "멈춤"을 가른 뒤 **`claude-error-stalled`** 를 따로 발사하고, 부모 알림은 이쪽만 구독한다.

**이름에 `error`가 남아 있지만 범위는 에러 뒤 정지만이 아니다** — 에러 문자열이 한 번도 안 나온 정지도, 호스트가 이미 `stale`로 본 자식도 같은 키로 알린다([ADR-0266](../../adr/0266-derived-stale-must-reach-the-push-channel.md)). 키는 부모가 `hook.set`으로 이미 등록해 둔 배선 식별자라 개명하면 등록된 훅이 전부 깨지고 기능적으로 얻는 것이 없어서 그대로 두었고, 원인은 **알림 문구가** 가른다(에러 줄이 화면에 있으면 그 줄을 힌트로 붙인 문구, 없으면 "출력도 완료 신호도 없다" 문구).

판정 기준은 두 조건의 **동시** 충족이다(`error_scan.rs`):

| 조건 | 왜 |
|---|---|
| PTY 출력이 **문턱 이상 전혀 변하지 않음** — 화면에 에러 줄이 있으면 **30초**, 없으면 **120초** | 재시도 중에는 시도 횟수·백오프 카운트다운이 계속 그려져 출력이 흐른다. 응답 없이 매달리면 출력이 완전히 멈춘다. 에러가 없는 정적은 보강 증거가 없어(긴 추론과 관측상 구별되지 않는다) 더 긴 문턱을 요구하며, 그 값은 호스트가 자식을 조용하다고 부르기 시작하는 문턱(`CHILD_OUTPUT_SILENCE`, `src/core/state/child_liveness.rs`)에 맞췄다. 비교는 dedupe 스니펫(앞 200자)이 아니라 **텍스트 전체 지문**으로 한다 — 뒤에 출력이 붙어도 앞 200자는 그대로라, 스니펫으로 보면 재시도를 정지로 오판한다 |
| `terminal.state`가 **`active` 또는 `stale`** | `idle`/`needs_input`/`exited`면 턴이 이미 끝났고 그 사건은 완료 알림 3형제(`claude-idle`/`needs-input`/`process-exit`)가 이미 부모에게 알렸다 — 같은 사건에 알림이 두 번 가지 않게 막는다. 반면 **`stale`에는 그런 완료 알림 경로가 없다**(그 값이 나온다는 것 자체가 훅이 유실됐다는 뜻이다) — `confidence`가 `confirmed`든 `heuristic`든 알린다: 승인 대기는 전경이 여전히 `claude`라 휴리스틱 쪽으로 판정되므로 확정만 알리면 정작 이 경로가 존재하는 이유인 사고를 못 잡는다 |

노이즈 상한: 한 정적 구간당 1회(출력이 재개되면 해제), 그리고 surface당 최소 5분 간격. 오탐(긴 추론 중인 자식)은 이 문턱·상한으로 누르고 받아들인다 — 미탐은 부모가 영원히 기다리는 비용이라 대가가 비대칭이다. 새 턴 신호(`prompt-submit`/`session-start`/`active`)는 dedupe와 함께 정적 구간 측정도 리셋하지만 쿨다운은 유지한다(턴을 넘나드는 반복 에러의 빈도 상한이라 턴 경계에서 풀리면 무의미).

**API 에러로 턴이 끝난 경우는 이 경로가 아니라 완료 알림이 받는다.** `StopFailure`가 상태를 `idle`로 닫으므로 위 둘째 조건이 거짓이 되어 `claude-error-stalled`는 나가지 않고, 대신 `claude-idle`로 깨어난 `notify-done`이 에러 종류를 덧붙인 한 줄을 곧바로 보낸다(30초를 기다리지 않는다). 이 경로가 남아 잡는 것은 턴이 끝나지 않은 정지 — 응답 없이 매달린 요청, 훅이 오지 않은 프롬프트 — 다.

**상태 축은 건드리지 않는다.** 이 경로는 `terminal.set_state`를 호출하지 않으므로 `claude children`의 `state`는 변하지 않는다 — 에러는 재시도로 복구될 수 있어 상태로 승격하면 오탐이고, 파생 상태는 관측 융합의 출력 전용 계약이다([ADR-0072](../../adr/0072-child-state-hook-observation-fusion.md)).

배선(`handlers.rs`)은 완료 알림 3형제와 **분리된 수명**을 갖는다:

- `register_notify_hooks`가 3형제(once)와 함께 `claude-error-stalled` 하나를 **상시 hook**(`once: false`)으로 등록한다. command 문자열이 `tasty claude notify-error --caller-surface … --target-surface … --observer …` 로 달라서, 형제 그룹의 `cleanup_sibling_hooks`(command 완전 일치) 정리 대상에 걸리지 않는다.
- 상시라서 **재무장이 필요 없다** — 3형제의 fire→정리→재무장 사이클과 얽히지 않는다. 발사 빈도 상한은 발신 측(위 쿨다운)이 갖는다.
- 같은 observer의 재등록은 멱등하다. spawn/tell 구독의 observer는 각각 독립이며, 같은 부모의 동일 epoch는 host에서 한 번으로 합쳐 중복 로그를 막는다. 다른 부모의 유효 tell 수신은 별도로 유지한다.
- `notify-error`는 구독과 child 실행 세대에 묶인 observer를 host에 전달해 현재 소유권을 확인한다. 종료·교체된 실행 또는 observer 없는 구형 callback은 상태와 로그를 변경하지 않는다.
- `notify-error` 핸들러는 알림 조립 직전 `surface.screen_text`를 읽어 **원인을 가른다** — 에러 줄이 있으면 그 줄을 힌트로 덧붙이고, 없으면 에러 없는 정지용 문구를 쓴다(codex `notify-caller`와 같은 방식). 완료 알림과 같은 `<parent_home>/notify/<caller_surface>.log` 한 줄로 나간다([child-completion-notify-log](../../dev-guide/external-interaction/child-completion-notify-log.md)).

## 인터페이스

- **AI Agent / 사용자**: `tasty claude launch|spawn|tell|broadcast|kill|respawn|children|parent|hook|checklist-hook|checklist-enable|checklist-disable|checklist-status|profile-register|profile-unregister|profile-list|profile-show|profile-current|gate-register|gate-unregister|gate-list|gate-show …`.
- surface/페인 생성 자체는 [work-area](../../features/work-area/index.md) 도메인을 사용.

## 비-목표

- Claude Code 자체 기능 — 외부 CLI. 이 플러그인은 *실행·배치·관계 관리*.
- 터미널/PTY 내부 — host.

## Acceptance Criteria

- Given 플러그인 활성 When `tasty claude launch` Then 새 워크스페이스에서 Claude 가 실행된다.
- Given 부모 인스턴스 When `tasty claude spawn` Then 자식 인스턴스가 페인 분할로 생성되고 `children` 에 보인다.
- Given 자식 When `tasty claude spawn`(또는 `tell`) 후 자식이 idle/needs_input/exited 에 도달 Then caller의 completion-log에 완료 알림이 기록되고 형제 hook 이 함께 정리된다. 자식이 exited 가 아닌 상태(idle/needs_input)로 도달한 경우엔 형제 hook 이 재등록돼 그 뒤 상태 전환에도 계속 알림이 온다.
- Given `~/.claude/settings.json`에 사용자가 직접 추가한 hook entry가 있음 When `tasty claude install` 실행 Then 6개 tasty hook entry가 추가/갱신되고 사용자 entry는 그대로 보존된다.
- Given 유효한 프로필 JSON When `tasty claude reboot --profile-file <경로>` Then 재시작된 Claude 에서 프로필 훅과 tasty 내장 훅이 함께 발화하고, 무인자로 다시 reboot 해도 프로필이 승계된다. `--clear-profile` 후 reboot 하면 프로필 훅이 더 이상 발화하지 않는다. 존재하지 않는 경로/깨진 JSON 은 kill 시퀀스를 시작하지 않고 즉시 에러를 반환한다.
- Given 등록된 프로필 둘(각각 다른 마커를 남기는 `SessionStart` 훅) When 이름 둘을 쉼표로 `--profile` 에 함께 부착해 spawn Then **둘 다** 발화한다(머지가 last-wins 로 떨어지지 않는다). `permissions.deny`를 담은 프로필을 부착하면 그 자식에게서 해당 도구가 사라지고(거부 프롬프트가 아니라 툴셋에서 빠짐), `deny` 프로필과 그 도구를 `allow` 하는 프로필을 함께 부착해도 도구는 여전히 없다(deny 가 allow 를 이긴다). `--profile-file` 과 `--profile` 을 함께 주면 즉시 에러.
- Given `--profile continue-checklist` 로 부착한 세션 + 마커 파일 존재 When Claude 가 센티넬 없이 응답을 끝내려 함 Then block 되고 체크리스트 본문이 주입되며, 센티넬을 포함해 응답하거나 라운드 상한에 도달하면 정상 종료된다. 프로필을 부착하지 않은 세션은 이 동작에 전혀 영향받지 않는다. 같은 프로필을 부착한 세션 둘을 동시에 진행해도 라운드 카운터가 서로 섞이지 않는다.
- Given 마커 파일 부재 When `tasty claude checklist-enable` Then `continue-checklist` 게이트의 마커 파일이 생성되고 `checklist-status` 가 `enabled: true` 를 보고한다. When `tasty claude checklist-disable` Then 마커 파일이 삭제되고 `checklist-status` 가 `enabled: false` 를 보고한다 — 마커가 이미 없는 상태에서 다시 `checklist-disable` 을 호출해도 에러 없이 `enabled: false` 를 반환한다(멱등).
- Given 등록 게이트 둘(`gate-a`/`gate-b`) When `checklist-enable --gate gate-a` Then `checklist-status --gate gate-a` 는 `enabled: true`, `--gate gate-b` 는 `enabled: false` 이고 `gate-list` 가 두 상태를 함께 보여준다. 그 세션에서 `gate-a` 훅만 block 을 걸고 `gate-b` 훅은 조용히 통과한다. 미등록 이름으로 `checklist-enable --gate nope` 를 호출하면 에러이고 마커 디렉토리도 생기지 않으며, `checklist-status --gate nope` 는 에러 없이 `enabled: false, registered: false` 를 돌려준다.
- Given 켜 두고 라운드가 쌓인 게이트 When `gate-unregister <이름>` 후 같은 이름으로 다시 `gate-register` Then `checklist-status` 가 `enabled: false` 이고 라운드 카운터도 1 부터 시작한다(이전 인스턴스의 상태를 물려받지 않는다).
- Given 게이트 축 이전 경로(`checklist/enabled.marker`)에만 마커가 있는 인스턴스 When 아무 진입점(`checklist-status` 또는 훅 발화) Then 마커가 `gates/continue-checklist/enabled.marker` 로 옮겨지고 legacy 파일은 사라지며, 켜져 있던 상태가 그대로 보존된다.
- Given `gate-register mygate --sentinel '[[MY-DONE]]' --rounds 2` 로 등록하고 `checklist-enable --gate mygate` 로 켜 둔 게이트 When `spawn --profile mygate` Then 그 자식의 `Stop` 훅으로 `checklist-hook --gate mygate` 가 걸리고, 센티넬 없이 턴을 끝내려 하면 **mygate 의 본문**(host 기본 체크리스트 본문이 아니라)이 주입되며 block 된다. `[[MY-DONE]]` 을 포함해 답하면 통과하고, 포함하지 않은 채 계속하면 **mygate 의 상한 2** 에서 백스톱으로 통과한다. 라운드는 `checklist/gates/mygate/rounds/<session>.json` 에 쌓이고 그 세션이 끝나면 정리된다.
- Given 등록 게이트 `mygate` 와 그것을 `reboot --profile mygate` 로 부착해 둔 자식(이름 meta 를 기록하는 기동 경로는 `reboot` 뿐이라 `spawn` 으로 띄운 자식은 훅이 걸려 있어도 `attached_names` 가 `null` 이다) When `profile-list` / `profile-show mygate` / `profile-current --surface <자식>` Then 목록에 `user/mygate`(attachable, 게이트 설명)와 `host/continue-checklist` 가 함께 보이고, show 는 owner `user` 와 생성된 Stop 훅 JSON 을 돌려주며, current 의 `attached_names` 에 `mygate` 가 있다.
- Given 게이트 둘 When `--profile mygate,continue-checklist` 로 부착 Then `generated/<정렬된이름>.json` 의 `hooks.Stop` 항목이 2개이고 각각 `--gate mygate` / `--gate continue-checklist` 를 지목한다.
</content>
