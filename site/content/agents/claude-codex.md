# Claude · Codex 와 함께 쓰기

Claude Code와 Codex CLI를 연결해 여러 에이전트에게 일을 나눠 맡겨보세요. 한 에이전트가 다른 에이전트를 실행하고 결과를 받는 방식으로 구현, 테스트, 검토를 함께 진행할 수 있습니다.

Claude Code와 Codex CLI는 별도로 설치하세요. Tasty는 에이전트 실행과 배치, 작업을 맡긴 에이전트와의 연결을 관리합니다. 훅 설치와 함께 부모의 수신 설정도 마치세요. 완료는 부모 종류와 관계없이 같은 완료 로그에 쌓이고, 부모가 Claude Code이면 그 로그를 Monitor로 구독합니다. 자세한 절차는 [완료 알림 받기](#4-완료-알림-받기)를 따르세요.

## 1. 훅 설치 (처음 한 번)

에이전트의 상태(작업 중 / 대기 / 입력 필요 / 종료)를 Tasty 가 알려면 각 CLI 의 훅 설정에 Tasty 항목이 들어가야 합니다.

```sh
tasty claude install    # ~/.claude/settings.json 의 hooks 에 Tasty 항목 추가
tasty codex install     # ~/.codex/config.toml 의 [hooks] 에 Tasty 항목 추가
```

- 이미 직접 넣어 둔 훅은 그대로 보존됩니다. 여러 번 실행해도 중복되지 않습니다.
- **Tasty 를 업데이트한 뒤에는 다시 실행합니다.** 훅 명령 문자열은 설정 파일에 저장되므로 새 형식을 반영하려면 재설치가 필요합니다.
- Claude Code 가 API 에러(서버 과부하 · 요청 한도 · 인증 실패 등)로 턴을 끝낸 경우를 대기로 알리는 훅도 재설치해야 들어갑니다. 그 전에는 그런 자식이 계속 "작업 중" 으로 보입니다.
- Codex에서 `hook returned invalid ... JSON output` 오류가 뜨면 업데이트 후 `tasty codex install`을 다시 실행하세요. Tasty의 상태 전달 결과가 Codex의 훅 응답에 섞이지 않도록 설정됩니다.
- Tasty 밖에서 Claude Code를 실행하면 이 훅은 동작하지 않습니다.
- 제거는 `tasty claude uninstall` / `tasty codex uninstall`.

다른 Codex home이나 profile을 쓰면 설치 위치를 지정하세요. 두 위치 옵션은 함께 사용하지 않습니다.
기존의 다른 훅과 모델 설정은 보존합니다.

```sh
tasty codex install --codex-home /absolute/codex-home
# profile 파일을 직접 선택하는 경우:
tasty codex install --config-file /absolute/codex-home/work.config.toml
```

설치한 훅이 현재 세션에서 정상 실행되면 Tasty가 상태와 세션 정보를 받습니다.

- 에이전트가 응답을 마치거나 질문을 던지면 그 서피스에 **주의 환기 테두리**가 켜지고 사이드바 워크스페이스에 배지가 붙습니다 (질문 대기는 노란색 우선).
- 탭을 닫았다가 복원하거나 Tasty 를 재시작하면 같은 세션으로 다시 이어집니다 (`claude -r` / `codex resume`).
- 자식의 상태·결과를 부모가 받으려면 [수신 설정](#4-완료-알림-받기)도 필요합니다. 완료는 완료 로그에 한 줄로 쌓이고, 부모 Claude Code는 그것을 Monitor로 구독합니다.

## 2. 실행하기

```sh
tasty claude launch --workspace myproj --directory ~/proj --task "테스트 고치기"
tasty codex launch --workspace review --directory ~/proj
```

새 워크스페이스를 만들고 그 터미널에서 CLI 를 실행합니다. `--workspace` 를 생략하면 이름은 `claude` / `codex`.

Codex 는 `--approval untrusted|on-request|never`, `--sandbox read-only|workspace-write|danger-full-access`, `--full-auto` 로 승인·샌드박스 정책을 붙일 수 있습니다 (아래 "Codex 승인 정책").

Claude Code 는 `--permission-mode` 로 권한 모드를 지정할 수 있습니다 (아래 "Claude 권한 모드").

<a id="3-자식-에이전트-부리기-spawn--tell"></a>

## 3. 다른 에이전트에게 작업 맡기기 (spawn / tell)

Claude Code 세션에서 다음 명령으로 다른 에이전트를 실행해 보세요. 작업을 맡긴 쪽을 부모, 새로 실행한 쪽을 자식 에이전트라고 부릅니다. 새 에이전트는 지정한 워크스페이스의 페인에 탭으로 열리고, 이 관계가 기록됩니다.

```sh
tasty claude spawn --workspace workers --cwd ~/proj --role tester --nickname t1 \
  --prompt "cargo test 를 돌리고 실패 원인을 보고해"
tasty codex spawn --workspace workers --cwd ~/proj --sandbox read-only \
  --prompt "방금 커밋된 diff 를 리뷰해"
```

| 옵션 | 뜻 |
|---|---|
| `--workspace <ID 또는 이름>` | 필수. 자식이 들어갈 워크스페이스 |
| `--pane <ID>` | 워크스페이스의 특정 페인 (기본: 첫 페인) |
| `--cwd <경로>` | 자식의 작업 디렉토리 |
| `--role <라벨>` | 역할 라벨. `broadcast --role` 로 골라 보낼 때 씀 |
| `--nickname <이름>` | 탭에 표시할 이름 |
| `--prompt <텍스트>` | 띄운 직후 보낼 첫 지시 |
| `--surface <ID>` | 부모 서피스 (기본: 자기 자신) |

`spawn` 은 **즉시 반환**합니다. 별도의 대기 명령은 없습니다. 자식의 훅과 [수신 설정](#4-완료-알림-받기)을 마치면 자식이 보고한 대기 상태를 부모에게 전달합니다. 대기는 작업 성공을 뜻하지 않습니다.

부모는 자기 워크스페이스가 아닌 **다른 워크스페이스**에 자식을 두는 편이 안전합니다. 원격 mirror 워크스페이스에는 spawn 할 수 없습니다.

이후 자식에게 추가 지시를 보내거나 상태를 봅니다.

```sh
tasty claude tell "이번엔 clippy 도 돌려" --surface 57     # 여러 줄 가능, 자동 제출
tasty claude children                                       # 자식 목록 (index · surface · 상태)
tasty claude state --surface 57                             # idle / needs_input / active / exited
tasty claude broadcast "진행 상황 보고해\r" --role tester   # 역할별로 일괄 전송 (\r 로 제출)
tasty claude kill --child 0                                 # index 로 종료
tasty claude respawn --child 0 --prompt "다시 시작"          # 같은 자리에서 재시작
tasty claude parent --surface 57                            # 이 자식의 부모
```

`tasty codex …` 도 같은 서브커맨드(`tell` / `children` / `state` / `broadcast` / `kill` / `respawn` / `parent`)를 가집니다.

자식이 너무 많아지면 spawn 응답에 경고가 붙습니다. 임계치는 **설정** <!-- en: Settings --> › **플러그인** <!-- en: Plugin --> › **Claude Code** / **Codex** 의 **Spawn child 경고 임계치** <!-- en: Spawn child warning threshold --> 에서 바꿉니다 (Codex 기본 6).

## 4. 완료 알림 받기

완료는 부모 종류와 관계없이 같은 로그 파일에 한 줄씩 쌓입니다. 자식이 Claude인지 Codex인지도 관계없습니다.
입력 요청·중단·오류·프로세스 종료도 상태 알림이므로, 알림을 작업 성공으로 단정하지 마세요.
Claude 자식이 API 에러로 턴을 끝냈으면 완료 알림 줄 끝에 그 사실과 에러 종류(예: `overloaded`)가 붙습니다.

### Claude Code의 Monitor로 받기

부모가 Claude Code이면 완료 로그를 Monitor로 구독합니다.

```text
Monitor({ command: "tail -n0 -F \"$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log\"", persistent: true })
```

로그 문구는 앱 언어를 따르고, 256 KiB 이상이면 비워집니다. 같은 로그에 claude child 와
codex child 가 함께 쓰더라도 **한 줄은 통째로 남거나 통째로 없습니다** — 두 줄이 섞이거나
줄 중간이 잘리지 않습니다. **tasty 를 다시 시작하면 이전
실행이 남긴 완료 로그는 지워집니다** — 재시작을 사이에 두고 과거 줄을 되읽을 수는 없습니다.
Monitor 없이 직접 읽을 수는 있지만 자동 재개나 영구 보관을 보장하지 않습니다.

이 로그가 보관하는 범위는 **지금 실행 중인 tasty 하나**이고, 크기 기준은 **바이트 하나**
입니다. 기간 제한도, 파일 개수 제한도 없습니다 — 닫은 탭의 완료 로그도 다음 재시작 때까지
그 자리에 남습니다. 256 KiB 에 닿으면 **마지막 256 KiB 를 남기는 것이 아니라 그 파일을
통째로 비웁니다**(아직 안 읽은 줄이 있었다면 함께 사라집니다). 이때도 사라지는 단위는
줄이라 줄 중간이 잘리지는 않습니다. 얼마를 버렸는지는 그때 로그에 남으므로, 알림이 안 온
이유를 나중에 되짚을 수 있습니다. 완료 로그를 직접 이어 읽는다면 옆의 `<번호>.log.meta`
파일도 보세요. 거기 적힌 `retention_start` 는 지금까지 비워진 바이트의 합계입니다. 읽던 위치를
이 값과 견주면, 멈췄다 다시 읽을 때 그 사이 못 읽고 사라진 양을 알 수 있습니다. Monitor 구독은
이 파일을 몰라도 됩니다.

같은 데이터 폴더로 tasty 를 **두 번 띄우지 마세요.** 나중에 뜬 쪽이 시작하면서 완료 로그
폴더를 통째로 지우는데, 그때 먼저 떠 있던 쪽이 쓰고 있던 완료 로그까지 사라집니다. 두 벌을
같이 쓰려면 `TASTY_HOME` 으로 데이터 폴더를 따로 주세요.

### API 에러로 멈춘 Claude 자동 재개

서버 과부하 같은 일시적인 API 에러로 Claude 의 턴이 끝나면, Tasty 가 잠시 뒤 "이어서 진행해" 문구를 대신 보내 작업을 잇게 할 수 있습니다. **기본은 꺼져 있습니다.** **설정** <!-- en: Settings --> › **플러그인** <!-- en: Plugin --> › **Claude Code** 에서 켭니다.

- **일시적인 API 에러(서버 과부하) 뒤 자동으로 이어서 진행** <!-- en: Resume automatically after a temporary API error (server overload) --> — 켜기/끄기.
- **재개 전 대기 시간(초)** <!-- en: Seconds to wait before resuming --> — 기본 10초. 1초부터 하루(86400초)까지 정할 수 있고, 범위를 벗어난 값은 가장 가까운 끝으로 맞춥니다.
- **포기하기 전 연속 자동 재개 횟수** <!-- en: Consecutive automatic resumes before giving up --> — 기본 5번. 연달아 실패해 이 수에 닿으면 더 보내지 않고 알림을 한 번 띄웁니다.
- 요청 한도 초과·인증 실패·결제 문제처럼 다시 보내도 같은 에러는 재개하지 않습니다.
- 기다리는 동안 직접 입력하면 보내지 않습니다. 그 턴이 시작된 뒤 그 서피스에 키를 한 번이라도 누르거나 무언가를 붙여넣었다면(입력 칸에 쓰다 만 글이 있을 수 있으므로) 재개하지 않습니다.
- 자동으로 보낸 횟수는 서피스 메타 `claude-auto-resume-count` 에서 볼 수 있습니다.
- 이 기능은 `tasty claude install` 로 설치한 훅이 필요합니다. Tasty 를 업데이트한 뒤에는 다시 설치하세요.

## 5. Codex 승인 정책

`tasty codex spawn/launch/respawn/reboot` 는 Codex 의 승인·샌드박스 정책을 플래그로 받습니다.

`launch`/`spawn`/`respawn` 는 sh/bash/zsh 및 Windows Git Bash에서 셸의 `codex` alias나 function을 건너뛰고 `PATH`에 설치된 Codex를 실행합니다. alias에 넣어 둔 승인·샌드박스 옵션은 아래 플래그나 전역 설정으로 지정하세요. `reboot`의 alias/function 우회는 Linux/macOS에 적용됩니다. Windows의 `reboot`는 기존 실행 방식을 유지하며 alias/function 우회는 아직 지원하지 않습니다.

- **승인**: `--approval untrusted|on-request|never`. 아무것도 안 주면 **`never`** 로 실행됩니다 — 자동화 중 승인 프롬프트에 걸려 영원히 멈추는 것을 막기 위해서입니다. 사람이 옆에서 승인해 줄 때만 `untrusted` / `on-request` 를 명시합니다.
- **샌드박스**: `--sandbox read-only|workspace-write|danger-full-access`. 안 주면 Codex 기본값. 리뷰·교차검증용 자식은 `read-only` 가 적당합니다.
- `--full-auto`: 승인과 샌드박스를 모두 우회. `--approval`/`--sandbox` 와 함께 쓸 수 없습니다.
- 전역 기본값은 **설정** › **플러그인** › **Codex** 의 **기본 승인 정책** <!-- en: Default approval policy --> / **기본 샌드박스 모드** <!-- en: Default sandbox mode -->. 호출별 플래그가 우선합니다.

컨테이너 등 중첩 샌드박스가 안 되는 환경에서 `--sandbox` 지정이 `RTM_NEWADDR: Operation not permitted` 류로 실패하면 `--full-auto` 를 씁니다. 완료 알림에도 이 힌트가 붙습니다.

## 6. Claude 권한 모드

`tasty claude launch/spawn/respawn/reboot/child-profile` 는 자식 Claude 의 권한 모드를 플래그로 받습니다.

- `--permission-mode acceptEdits|auto|bypassPermissions|manual|dontAsk|plan` — Claude Code 에 그대로 전달됩니다.
- **아무것도 안 주면 플래그가 붙지 않습니다.** 자식은 여러분이 쓰던 Claude Code 설정 그대로 뜹니다. Codex 와 달리 자동으로 "묻지 않음" 이 되지 않습니다 — Claude Code 에는 샌드박스 축이 따로 없어서, 안 묻게 만드는 순간 그것이 곧 제한 없는 실행이 되기 때문입니다.
- 자동화 중에 자식이 승인 대기로 멈추는 것이 곤란하면 그 호출에만 원하는 모드를 명시하세요. 멈춘 자식의 상태도 [수신 설정](#4-완료-알림-받기)에 따라 전달됩니다.
- 전역 기본값은 **설정** › **플러그인** › **Claude Code** 의 **자식 세션 기본 권한 모드** <!-- en: Default permission mode for child sessions -->. 기본값은 **물려받음**(플래그 미부착)이고, 호출별 플래그가 우선합니다.
- `--profile` / `--profile-file` 로 붙이는 설정 JSON 이 `permissions.defaultMode` 를 정하고 있으면 `--permission-mode` 와 함께 쓸 수 없습니다 — 둘이 같은 것을 정하므로 하나만 고르라는 에러가 납니다.
- `reboot` / `child-profile` 에서 지정한 모드는 **그 재시작에만** 적용됩니다. 탭 복원으로 다시 뜰 때는 따라가지 않습니다.

<a id="6-세션-재시작-reboot"></a>

## 7. 세션 재시작 (reboot)

훅이나 설정을 바꾼 뒤 에이전트를 같은 세션으로 다시 띄웁니다.

```sh
tasty claude reboot --surface 57 --delay 5
tasty codex reboot --surface 58
```

지정한 시간 뒤 프로세스를 끊고 같은 세션을 이어서 시작합니다. 에이전트가 **자기 자신**에게 호출할 때는 턴의 마지막 행동으로 부릅니다 — 이후 응답은 프로세스 종료로 중단됩니다. 자식 에이전트만 재시작할 때는 작업을 요청한 에이전트의 응답이 중단되지 않습니다.

<a id="7-claude-세션-프로필과-stop-게이트"></a>

## 8. Claude 세션 프로필과 Stop 게이트

Claude Code 는 훅을 시작할 때 한 번만 읽습니다. 특정 세션에만 추가 훅·권한을 붙이려면 프로필을 등록하고 실행 시 `--profile` 로 지정합니다.

```sh
tasty claude profile-register strict --file ./strict.json   # settings JSON 을 이름으로 등록
tasty claude profile-list
tasty claude spawn --workspace w --profile strict           # 이 자식에게만 적용
tasty claude reboot --profile strict                        # 이후 재시작에도 승계
tasty claude child-profile --child 0 --profile strict       # 자식에게 지속 부착
```

**Stop 게이트**를 사용하면 에이전트가 응답을 마치기 전에 체크리스트로 작업을 다시 확인하도록 할 수 있습니다. 기본 제공 게이트인 `continue-checklist`를 켜고 세션에 연결하세요.

```sh
tasty claude checklist-enable                               # 게이트 켜기 (checklist-disable 로 끔)
tasty claude spawn --workspace w --profile continue-checklist
```

- 에이전트가 응답 끝에 `[[TASTY-CHECKLIST-DONE]]` 을 넣으면 통과, 아니면 체크리스트를 다시 받습니다. 라운드 상한(기본 3)에 닿으면 자동 통과합니다.
- 상한은 **설정** › **플러그인** › **Claude Code** 의 **게이트 기본 라운드 상한** <!-- en: Default gate round limit -->.
- 자기 게이트를 만들려면 `tasty claude gate-register <이름> --body-file <파일> [--sentinel <문자열>] [--rounds N]`. 본문에 센티넬 문자열이 들어 있어야 합니다. `gate-list` / `gate-show` 로 확인.

## 문제 해결

- **완료 알림이 안 옵니다** — 자식 CLI의 훅 설치·실행과 [수신 설정](#4-완료-알림-받기)을 확인하세요. 부모가 Claude Code이면 Monitor가 완료 로그를 구독하는지 확인하세요. 훅 전달 실패는 `~/.tasty/hook-failures.log` 에 남습니다. 플러그인 로그는 `tasty plugin logs com.tasty.claude --follow`.
- **`reboot` 가 "claude-session-id meta not set" 으로 실패합니다** — 세션 시작 훅이 세션 ID 를 못 남긴 것입니다. `tasty surface-meta set --key claude-session-id --value <세션ID>` 로 직접 넣습니다.
- **자식이 spawn 되지 않고 "occupied" 오류** — 대상 워크스페이스가 원격에서 attach 중이거나 mirror 입니다. 다른 워크스페이스를 씁니다.
- **macOS 에서 앱 아이콘으로 실행하면 알림이 안 옵니다** — Tasty 가 알림을 쓸 때 `tasty` 를 다시 호출하는데, Tasty 는 자기 실행 파일 경로를 자동으로 PATH 에 넣으므로 보통은 문제없습니다. 그래도 안 되면 `hook-failures.log` 를 봅니다.

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [작업 순서 관리](tasks.md) — spawn 과 tell 을 의존 관계로 묶어 한 그래프로 돌리기.
- [훅 · 알림 · 웹훅](hooks-notifications.md) — 완료 통지와 승인 게이트.

Codex 연동을 다시 설치하면 SessionEnd 훅도 등록됩니다. 현재 실행 세션이 종료되면 그 작업의 완료 대기도 끝납니다.
