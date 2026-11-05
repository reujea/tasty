# Claude·Codex와 함께 쓰기

Claude Code와 Codex CLI를 Tasty의 터미널에서 실행하고 상태를 확인합니다. 두 CLI는 별도로 설치해야 합니다. 여러 에이전트에게 작업을 나누려면 훅과 부모의 결과 수신 경로를 함께 설정하세요.

자식의 응답 종료와 작업 성공은 다릅니다. 부모가 Claude Code인지 Codex인지에 따라 결과를 받는 경로가 달라집니다. 훅은 상태를 전달하고, 부모의 수신 경로는 그 상태나 결과 참조를 대화에 전달합니다. 결과를 읽고 실제 변경과 실행 결과를 대조하세요.

## 빠른 시작

1. 훅을 설치합니다: `tasty claude install`, `tasty codex install` ([훅 설치](#1-훅-설치-처음-한-번)).
2. 에이전트를 실행합니다: `tasty claude launch --workspace myproj --directory ~/proj` ([실행하기](#2-실행하기)).
3. 그 Claude Code 세션 안에서 다른 에이전트에게 작업을 맡깁니다: `tasty claude spawn --workspace workers --cwd ~/proj --prompt "cargo test 를 돌리고 실패 원인을 보고해"` ([작업 맡기기](#3-다른-에이전트에게-작업-맡기기-spawn--tell)).
4. 상태와 결과를 확인합니다: `tasty claude children` 으로 자식 목록과 상태를 보고, 완료는 [완료 알림 받기](#4-완료-알림-받기)의 방법으로 받습니다.

`tasty claude help` · `tasty codex help` 의 마지막 "내부 연동" 그룹은 훅이 부르는 명령이라 직접 입력하지 않습니다. 승인 정책, 권한 모드, 세션 재시작, 세션 프로필은 뒤쪽 절에서 필요할 때 보세요.

## 1. 훅 설치 (처음 한 번)

에이전트의 상태(작업 중 / 대기 / 입력 필요 / 종료)를 Tasty 가 알려면 각 CLI 의 훅 설정에 Tasty 항목이 들어가야 합니다.

```sh
tasty claude install    # Tasty 훅과 완료 알림 mod 설치
tasty codex install     # ~/.codex/config.toml 의 [hooks] 에 Tasty 항목 추가
```

- 이미 직접 넣어 둔 훅은 그대로 보존됩니다. 여러 번 실행해도 중복되지 않습니다.
- Claude 설정 폴더를 다른 위치로 연결해 사용해도 설치와 제거는 같은 관리 폴더를 가리킵니다. Tasty가 관리하는 mod 내부의 파일이나 폴더가 다른 곳에 연결되어 있으면 설치와 제거를 거절합니다.
- **Tasty 를 업데이트한 뒤에는 다시 실행합니다.** 훅 명령 문자열은 설정 파일에 저장되므로 새 형식을 반영하려면 재설치가 필요합니다.
- Claude Code 가 API 에러(서버 과부하 · 요청 한도 · 인증 실패 등)로 턴을 끝낸 경우를 대기로 알리는 훅도 재설치해야 들어갑니다. 그 전에는 그런 자식이 계속 "작업 중" 으로 보입니다.
- 자식이 백그라운드 작업(백그라운드 셸 · 백그라운드 서브에이전트)을 남긴 채 API 에러로 턴을 끝내면, 작업이 끝나 이어지는 턴이 마칠 때까지 "작업 중" 으로 둡니다. 이 동작도 재설치해야 들어갑니다.
- Codex에서 `hook returned invalid ... JSON output` 오류가 뜨면 업데이트 후 `tasty codex install`을 다시 실행하세요. Tasty의 상태 전달 결과가 Codex의 훅 응답에 섞이지 않도록 설정됩니다.
- Tasty 밖에서 Claude Code를 실행하면 이 훅은 동작하지 않습니다.
- Codex는 새로 설치하거나 바뀐 훅을 검토하기 전에는 실행하지 않습니다. 설치 후 Codex의 `/hooks`에서 Tasty 훅을 확인하고 승인하세요. Tasty는 이 검토를 건너뛰지 않습니다.
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
- 자식의 상태·결과를 부모가 받으려면 [수신 설정](#4-완료-알림-받기)도 필요합니다. 완료는 완료 로그에 한 줄로 쌓이고, 지원되는 부모 Claude Code는 완료 알림 mod로 그것을 받습니다.

`tasty codex install` 후 Tasty의 Bash·zsh 터미널에서는 평소처럼 `codex`나 `codex resume`을
입력해도 복원 정보가 저장됩니다. 세션 선택기를 그대로 사용할 수 있고, 선택한 대화는 새 메시지를
보내기 전에도 복원 대상으로 기록됩니다. 승인·샌드박스·모델 옵션도 유지됩니다.
실행할 때 지정한 승인·샌드박스 옵션은 같은 Codex 안에서 시작한 새 대화(`/new`)에도 적용됩니다.
Codex 안에서 `/permissions`로 권한을 바꾸면 그 뒤의 새 대화는 바꾼 값을 따르고, 복원은 실행할 때 지정한 옵션으로 다시 엽니다.
업데이트 후에는 설치 명령을 다시 실행하고 새 터미널에서 시작하세요. 사용자 전역 셸 설정은 바꾸지 않습니다.

Tasty는 터미널마다 전용 Codex 서버를 띄우며, Codex를 끝내거나 터미널을 닫으면 함께 정리합니다.
이미 실행 중인 대화와 공유 서버는 재연결하지 않습니다. Codex CLI 0.154.0 기준입니다.
플러그인 카탈로그나 대화 이력이 크더라도 응답 크기만으로 연결을 끊지 않으며, 목록을 잘라내지 않고 전달합니다.
시작 화면의 자동 복원은 Tasty가 준비될 때까지 최대 30초 기다린 뒤 진행합니다. 부팅 후 따로 터미널을 깨울 필요는 없습니다.

자동 연결은 승인·샌드박스·모델, 작업 디렉터리(`-C`), 세션 선택 옵션을 지원합니다.
`-c`, 프로필, `fork` 등 지원하지 않는 옵션은 안내 후 원래 Codex로 실행되어 자동 복원에 등록되지 않습니다.
`command codex`, 명시적인 `--remote`, 비대화형 명령도 원래대로 실행됩니다.
Bash·zsh 외의 셸에서는 아래의 `tasty codex launch`를 사용하세요.

## 2. 실행하기

```sh
tasty claude launch --workspace myproj --directory ~/proj --task "테스트 고치기"
tasty codex launch --workspace review --directory ~/proj
```

새 워크스페이스를 만들고 그 터미널에서 CLI 를 실행합니다. `--workspace` 를 생략하면 이름은 `claude` / `codex`.

Codex 는 `--approval untrusted|on-request|never`, `--sandbox read-only|workspace-write|danger-full-access`, `--full-auto` 로 승인·샌드박스 정책을 붙일 수 있습니다 (아래 "Codex 승인 정책").

Claude Code 는 `--permission-mode` 로 권한 모드를 지정할 수 있습니다 (아래 "Claude 권한 모드").

Claude Code 플러그인이 켜져 있으면, Claude Code 세션 안에서 Tasty 를 실행해도 Tasty 터미널의 셸과 훅 명령에는 그 세션을 가리키는 환경변수(`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_MESSAGING_TOKEN` 등)가 넘어가지 않습니다. 그 터미널에서 실행한 Claude Code 는 독립된 세션으로 동작하고 대화 기록도 저장합니다. `CLAUDE_CODE_GIT_BASH_PATH` 처럼 직접 설정한 Claude Code 환경변수는 그대로 전달됩니다. 마찬가지로 Codex 플러그인이 켜져 있으면 Codex 대화에서 Tasty 를 실행했을 때 그 대화를 가리키는 `CODEX_THREAD_ID` 가 넘어가지 않으므로, 그 터미널의 셸이나 Claude 가 부른 `tasty` 를 바깥 Codex 의 호출로 보지 않습니다. `CODEX_HOME` 은 그대로 전달됩니다. 플러그인을 끄면 그 플러그인의 변수는 지우지 않고 그대로 넘깁니다.

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

`spawn` 은 **즉시 반환**합니다. 별도의 대기 명령은 없습니다. 자식의 훅과 [수신 설정](#4-완료-알림-받기)을 마치면 자식이 보고한 대기 상태를 부모에게 전달합니다. 대기는 작업 성공을 뜻하지 않습니다. Claude 자식의 서브에이전트가 끝났거나 백그라운드 작업이 남은 채 응답이 끝난 것은 대기로 보지 않습니다. 백그라운드 작업이 끝나고 이어진 응답까지 마쳐야 알립니다. [Stop 게이트](#8-claude-세션-프로필과-stop-게이트)가 체크리스트를 보내 응답을 이어 가게 한 경우도 대기로 보지 않고, 게이트를 통과해 응답이 끝나면 알립니다. 끝나지 않는 백그라운드 명령(개발 서버 등)을 남기면 대기 상태가 되지 않습니다. 백그라운드 작업을 기다리면서 10분 동안 출력이 없으면 부모 완료 로그에 기다리는 작업의 종류와 경과 시간을 적은 줄이 한 번 추가됩니다.

부모는 자기 워크스페이스가 아닌 **다른 워크스페이스**에 자식을 두는 편이 안전합니다. 연결한 원격 워크스페이스에서 자식을 실행하는 방법과 결과를 받는 조건은 아래 ‘[연결한 원격 워크스페이스에 자식 만들기](#연결한-원격-워크스페이스에-자식-만들기)’ 절에서 설명합니다. 자식은 원격 컴퓨터의 도구와 설정으로 실행됩니다.

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

프롬프트 없이 띄운 Claude 자식은 입력 대기 화면이 뜨면 `idle` 입니다. 이때는 완료 알림이 오지 않습니다. 완료 알림은 응답을 마쳤을 때만 옵니다. `--prompt` 로 띄운 자식은 그 프롬프트를 처리하는 동안 `active` 입니다.

`tasty codex …` 도 같은 서브커맨드(`tell` / `children` / `state` / `broadcast` / `kill` / `respawn` / `parent`)를 가집니다. Codex 대화에서 실행하면 결과를 받는 방법이 다릅니다([Codex 부모에게 자동으로 전달하기](#codex-부모에게-자동으로-전달하기)).

자식이 너무 많아지면 spawn 응답에 경고가 붙습니다. 임계치는 **설정** <!-- en: Settings --> › **플러그인** <!-- en: Plugins --> › **Claude Code** / **Codex** 의 **Spawn child 경고 임계치** <!-- en: Spawn child warning threshold --> 에서 바꿉니다 (Claude Code·Codex 모두 기본 6).

### Claude 자식의 모델과 effort

이번 자식 세션에만 모델과 effort를 지정할 수 있습니다.

```sh
tasty claude spawn --workspace workers --cwd ~/proj --model opus --effort high \
  --prompt "테스트 실패 원인을 조사해"
```

`--model`은 alias, 전체 모델 ID, provider ARN 같은 식별자를 받습니다. `--effort`는 `low`, `medium`, `high`, `xhigh`, `max`, `ultracode`를 받으며 지원 여부는 Claude CLI와 모델에 따라 다릅니다. `ultracode`는 xhigh와 Ultracode를 요청하며 Claude Code 2.1.203 이상이 필요합니다.

생략한 옵션은 기존 Claude 설정을 따릅니다. 부모·다른 자식·전역 설정은 바꾸지 않습니다. `--profile-file`이나 `--profile`과 함께 쓰면 명시한 모델·effort가 우선하고 프로필 파일은 그대로 둡니다. 훅이 정상 실행되면 같은 세션을 복원하거나 reboot할 때도 유지합니다. 새 respawn은 이 값을 승계하지 않습니다.

명령이 반환됐다는 것은 모델이 응답했다는 뜻이 아닙니다. 모델을 provider가 거절하면 Tasty가 다른 모델로 바꾸어 재시도하지 않습니다. 실제 모델과 effort는 Claude 화면에서 확인하세요.

## 4. 완료 알림 받기

완료는 부모 종류와 관계없이 같은 로그 파일에 한 줄씩 쌓입니다. 자식이 Claude인지 Codex인지도 관계없습니다.
같은 자식에 tell을 여러 번 보내도 상태가 한 번 바뀌면 한 줄만 남습니다. 다른 부모가 같은 자식에 보낸 tell의 알림은 그 부모의 로그에 따로 남습니다.
입력 요청·중단·오류·프로세스 종료도 상태 알림이므로, 알림을 작업 성공으로 단정하지 마세요.
Claude 자식이 API 에러로 턴을 끝냈으면 완료 알림 줄 끝에 그 사실과 에러 종류(예: `overloaded`)가 붙습니다. 백그라운드 작업이 남아 자식이 계속 기다리는 경우에는 완료 줄 대신 그 사실을 적은 줄이 한 번 남습니다.
자식이 백그라운드 셸이나 서브에이전트를 기다리는 동안 그 작업의 출력이 2분 동안 늘지 않으면, 작업 이름을 적은 줄(`background work <이름> has written no output for <분> min`)이 한 번 남습니다. Tasty가 그 작업의 출력을 찾지 못하면 10분 동안 화면 출력이 없을 때 한 번 남깁니다.

지원되는 Codex 부모 대화에는 자식의 응답이 끝났음을 알리는 결과 참조가 전달됩니다. 참조에 나온 이벤트 ID로 `tasty codex result`를 실행해 답변을 읽습니다. 연결 조건과 지원 경로는 아래 [Codex 부모에게 자동으로 전달하기](#codex-부모에게-자동으로-전달하기) 절을 따르세요.

### Claude Code에서 자동으로 받기

업데이트 후 `tasty claude install`을 다시 실행하고 새 Claude Code 세션을 시작하세요. 설치 명령은 기존 훅과 다른 플러그인 경로를 보존하면서 완료 알림 mod와 중복 방지 플러그인을 함께 설치합니다. Claude Code 2.1.307의 mod API를 기준으로 작성됐습니다. 실제 모델 턴 수신 검증은 별도이며, 사용량 제한이나 인증 오류를 성공으로 보지 않습니다.

mod가 활성화되면 세션 시작부터 자기 완료 로그를 읽고, Claude가 작업 중이면 그 턴이 끝난 뒤 플러그인 메시지로 알립니다. 새 세션은 이전 로그를 다시 전달하지 않고, mod 새로고침은 저장된 위치에서 이어 읽습니다. 알림이 대기 중일 때 mod가 새로고침되어도 보통 한 번만 전달되며, 두 번째 사본을 건너뛰었다는 짧은 안내가 보일 수 있습니다. 드물게 전달 도중 중복 방지 플러그인 자체가 다시 시작되면 같은 알림이 한 번 더 올 수 있지만 유실되지는 않습니다. 이 동작은 설치 뒤 새로 시작한 Claude 세션부터 적용됩니다. 읽지 않은 로그가 정리되면 복구할 수 없습니다. 상태 알림은 작업 성공의 증거가 아닙니다.

그 Claude의 Bash에서 `TASTY_CLAUDE_NOTIFY_MOD` 값이 자기 서피스 번호와 일치하는지 확인할 수 있습니다. mod와 Monitor를 함께 켜면 중복 알림이 됩니다. 기존 개인 지침이나 스킬의 Monitor 절차도 이 표식이 있는 세션에서는 생략하세요. Tasty가 그 파일들을 자동으로 바꾸지는 않습니다. 제거는 `tasty claude uninstall`입니다.

### Claude Code의 Monitor로 받기

mod를 지원하지 않거나 활성화하지 못한 부모 Claude Code에서만 Monitor를 사용합니다. 첫 spawn/tell 전에 구독하고, 구독 직후 자식 목록도 확인하세요.

```text
Monitor({ command: "tail -n0 -F \"$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log\"", timeout_ms: 1800000 })
```

Monitor는 최대 30분 뒤 만료됩니다. 만료 알림을 받으면 같은 명령으로 다시 구독하고, 바로 `tasty claude children`으로 자식 상태를 한 번 확인하세요. 이 목록에는 Codex 자식도 함께 나옵니다(`tasty codex children`도 같은 자식을 보여 줍니다). 구독이 끊긴 사이에 끝난 자식을 놓치지 않습니다.

로그 문구는 앱 언어를 따릅니다. Claude와 Codex 자식이 같은 파일에 알림을 한 줄씩 기록합니다. **Tasty를 다시 시작하면 이전 실행의 완료 로그는 지워집니다.** Monitor 없이 파일을 직접 읽는 것만으로 자동 재개나 영구 보관이 되지는 않습니다.

새 줄을 쓰기 전에 파일 크기가 **256 KiB** 이상이면 파일 전체를 비운 뒤 씁니다. 마지막 부분만 남기지 않으므로 아직 읽지 않은 알림도 함께 사라집니다. 한 줄을 추가하거나 여러 자식이 동시에 쓰는 동안에는 이 크기를 넘을 수 있습니다. 비울 때는 보통 아래 `retention_start` 누계를 비운 바이트 수만큼 올립니다. 다른 자식이 동시에 쓰고 있어 정리 차례를 제때 얻지 못하면 누계를 올리지 않은 채 비우므로, 그때 사라진 알림은 누계로도 알 수 없습니다. 기간이나 파일 개수 제한은 없고, 닫은 탭의 로그도 다음 재시작까지 남습니다.

완료 로그를 직접 이어 읽는다면 옆의 `<번호>.log.meta` 파일에 있는 `retention_start`도 확인하세요. 지금까지 비워진 바이트의 합계입니다. 저장한 읽기 위치와 함께 비교해 중간에 기록이 사라졌는지 확인할 수 있습니다. 다만 로그 정리나 메타데이터 기록에 실패하면 누계에 반영되지 않은 손실이 생길 수 있어 모든 누락을 확인하는 방법은 아닙니다. Monitor 구독에서는 이 파일을 직접 읽을 필요가 없습니다.

같은 데이터 폴더로 tasty 를 **두 번 띄우지 마세요.** 나중에 뜬 쪽이 시작하면서 완료 로그
폴더를 통째로 지우는데, 그때 먼저 떠 있던 쪽이 쓰고 있던 완료 로그까지 사라집니다. 두 벌을
같이 쓰려면 `TASTY_HOME` 으로 데이터 폴더를 따로 주세요.

### Codex 부모에게 자동으로 전달하기

Tasty를 데스크톱에서 실행했더라도, Codex 대화가 사용하는 실행 경로를 자동으로 전달해 결과를 받습니다. 실행 경로나 수신처를 `spawn`·`tell`마다 지정할 필요는 없습니다.

Codex 대화 안에서 `tasty codex spawn`(또는 `tell`·`respawn`)을 실행하면, 자식의 응답이 끝날 때마다
결과가 그 대화로 자동으로 돌아갑니다. 별도 준비 명령은 없습니다.

```sh
tasty codex spawn --workspace workers --cwd ~/proj --sandbox read-only \
  --prompt "방금 커밋된 diff 를 리뷰해"
```

부모 대화에는 다음과 같은 짧은 참조 메시지가 들어갑니다. 자식의 답변 본문은 들어가지 않습니다.

```text
[tasty] A child agent reported a turn boundary. provider=codex status=turn_ended event=<이벤트 ID>. ...
```

부모는 메시지에 적힌 명령으로 결과를 읽습니다.

```sh
tasty codex result <이벤트 ID>
```

- 결과를 받은 대화에서만 읽을 수 있습니다. 여러 번 읽어도 결과는 지워지지 않습니다.
- 결과는 자식 Codex가 그 턴을 실제로 끝낸 뒤 그 턴의 마지막 답으로 기록합니다. 다른 Stop 훅이 응답을 이어 가게 해도 중간 답이 아니라 마지막 답이 돌아옵니다.
- `status`는 응답이 어떻게 끝났는지를 알려 줄 뿐 작업이 잘 됐는지는 판정하지 않습니다. `turn_ended`(응답 끝), `interrupted`(사용자가 중단), `exited`(세션 종료. 자식이 같은 화면에서 `/new`로 대화만 바꾼 경우는 종료가 아닙니다), `turn_failed`(오류로 끝남)가 있습니다.
- 답변이 1 MiB 를 넘으면 잘라서 보관하고 `body_truncated`로 표시합니다.
- 같은 응답이 두 번 보고돼도 한 번만 전달합니다.
- 공유 Codex 서버에 잠시 연결하지 못해 보내지 못한 결과는 간격을 늘려 가며(최대 1분) 다시 보냅니다. 열두 번 넘게 실패하면 그 대화의 다음 결과가 오거나 Tasty를 다시 시작할 때 다시 보냅니다.
- 결과는 전달이 끝난 뒤 30일 동안 보관하고 그 뒤에는 지웁니다.

부모가 결과를 받을 수 있는 경우는 둘입니다.

- 부모가 Tasty의 실행 명령으로 띄운 Codex입니다. 부모 Codex가 잠시 꺼져 있으면 결과를 보관했다가 그 대화가 Tasty에서 다시 열릴 때 보냅니다. 같은 화면에서 `/new`로 새 대화를 시작하거나 다른 대화로 돌아가면, 그 뒤로는 지금 실행 중인 대화가 결과를 받습니다.
- 부모가 Tasty 밖의 Codex이고, 사용자의 공유 Codex 서버에 그 대화가 열려 있습니다.

둘 다 아니면 자식을 만들지 않고 결과를 받을 수 없는 부모라는 오류를 보여 줍니다. Claude Code 부모는 [완료 로그](#4-완료-알림-받기)로 받습니다.

Codex 대화는 자기가 만든 자식만 관리합니다. 같은 화면에서 `/new`로 시작한 새 대화도 이전 대화의 자식을 목록에 보거나 닫을 수 없습니다. 다른 부모의 자식에게 `tell`하거나 다른 부모를 `--surface`로 지정하면 거절합니다. Claude Code나 일반 터미널에서 같은 자식에게 새 지시를 보내면, 그 뒤 결과는 원래 Codex 대화로 돌아가지 않습니다. 그때 아직 끝나지 않았거나 확인하지 못한 응답의 결과도 돌아가지 않습니다. 이미 돌아온 결과는 그대로 읽을 수 있습니다.

Codex 대화의 자식이 여러 workspace에 나뉘어 있으면 `children`·`broadcast`·`kill`·`respawn`에 `--workspace`로 그중 하나를 지정하세요. 요청은 지정한 workspace가 있는 윈도우의 자식에게만 닿으므로, 자식이 여러 윈도우에 있으면 윈도우마다 따로 요청하세요. 자식이 한 workspace에만 있거나 대상 자식이 정해져 있으면 지정하지 않아도 됩니다.

### Codex 대화에서 Claude 자식 부리기

Codex 대화 안에서 `tasty claude spawn`·`tell`·`children`·`broadcast`·`kill`·`respawn` 을 실행하면 부모가 자동으로 정해집니다. `--surface` 를 줄 필요가 없고, 다른 대화의 서피스를 주면 거절됩니다. Codex 플러그인이 설치되고 켜져 있어야 하며, 없으면 다른 부모로 대신하지 않고 오류로 끝납니다.

```sh
tasty claude spawn --workspace workers --cwd ~/proj --prompt "테스트를 돌려 줘"
tasty claude tell "이번엔 clippy 도 돌려" --surface 57   # 대상 자식은 지정합니다
```

자식 Claude 의 응답이 끝나면 그 답이 원래 Codex 대화로 돌아갑니다. Stop 게이트가 응답을 이어 가게 했거나 백그라운드 작업을 기다리는 중이면 보내지 않고, 응답이 실제로 끝났을 때 한 번만 보냅니다. 같은 응답에 합쳐진 tell 은 결과 하나로 돌아옵니다. 돌아온 결과는 응답이 끝났다는 뜻이며 작업이 맞았다는 판정은 아닙니다. API 오류로 끝난 응답과 응답 도중 종료된 자식도 그 상태로 알립니다.

자식에서 `/clear` 나 `/resume` 을 해도 이후 응답은 같은 Codex 대화로 돌아갑니다. Claude Code 가 이전 세션의 종료 알림을 기다리지 않고 새 세션을 시작한 경우에는 새 세션이 다음 응답을 시작할 때 이어집니다.

자식이 끝난 뒤 같은 서피스에서 Claude 를 새로 실행했다면, Codex 대화에서 그 자식에게 다시 `tell` 하면 새 실행의 응답이 돌아옵니다. Codex 가 아닌 곳(일반 셸이나 Claude 부모)에서 같은 자식에게 `tell` 하거나 `respawn` 하면, 그 자식의 결과는 더 이상 Codex 대화로 가지 않습니다. 이미 끝난 응답의 결과는 그대로 보냅니다.

결과를 Codex 플러그인에 넘기지 못했으면 버리지 않고 남겨 둡니다. 점점 긴 간격(최대 5분)을 두고 자동으로 다시 보내며, 하루가 지나도 넘기지 못하면 자동 재시도를 멈춥니다. 같은 결과를 다시 보내도 Codex 대화에는 한 번만 들어갑니다.

```sh
tasty claude codex-returns           # 남은 결과와 본문 보기
tasty claude codex-returns --retry   # 남은 결과를 모두 지금 보낼 차례로 돌리기
```

`--retry` 는 자동 재시도를 멈춘 결과도 다시 보낼 차례로 돌리고, 보내기를 기다리지 않고 바로 끝납니다. 그래서 함께 출력되는 목록에는 방금 돌린 결과도 남아 있을 수 있습니다. 잠시 뒤 다시 `codex-returns` 로 확인하세요.

한 Codex 대화의 자식들이 여러 워크스페이스에 있으면 `children`·`kill`·`broadcast`·`respawn` 에 `--workspace` 로 어느 워크스페이스의 자식인지 지정하세요. 지정하지 않으면 후보를 알려 주며 거절합니다. 자식이 지금 있는 워크스페이스를 기준으로 하므로, 복원되어 다른 워크스페이스로 옮겨 간 자식도 지금 위치로 찾습니다.

### 연결한 원격 워크스페이스에 자식 만들기

다른 컴퓨터의 Tasty 워크스페이스를 연결(attach)해 두었다면, 그 워크스페이스 번호를 `--workspace` 로 주어 `tasty claude spawn`·`tasty codex spawn` 할 수 있습니다. 자식은 원격 컴퓨터에서 그 컴퓨터의 Claude Code·Codex 설치, 설정, 로그인으로 실행되고, 내 쪽에서는 연결한 워크스페이스 안의 자식으로 보입니다. `tell`·`children`·`kill` 도 내 쪽 자식처럼 씁니다.

```sh
tasty claude spawn --workspace 7 --prompt "테스트를 돌려 줘"    # 7 은 연결한 원격 워크스페이스
```

- 원격 컴퓨터에도 `tasty claude install`·`tasty codex install` 로 훅을 설치해 두어야 완료가 돌아옵니다. Claude 훅이 없으면 spawn 결과에 경고가 나옵니다.
- 완료 알림은 내 쪽 부모의 완료 로그로 오고, Codex 대화가 부모면 응답 결과도 그 대화로 돌아옵니다. 결과가 늦게 와도 그 응답을 지시한 대화로 갑니다.
- Claude 세션이나 셸이 부모면 응답 본문이 내 쪽에 저장되고, 완료 로그에 결과 ID 와 읽는 명령이 한 줄로 옵니다. 상태 변경 줄과 따로 오므로 상태 줄을 받은 뒤 결과 줄을 기다리세요. SSH 로 원격 컴퓨터에 들어가지 않아도 됩니다.

  ```sh
  tasty claude remote-results --id 0123456789abcdef   # Claude 자식의 결과 본문 (Codex 자식은 tasty codex remote-results)
  tasty claude remote-results                         # 저장된 결과 목록 (본문 제외)
  ```

  결과 줄의 문구는 Tasty 의 언어 설정에 따라 번역됩니다. 예를 들어 영어로는 `surface 12: turn result 0123456789abcdef (turn_ended) is stored. Read it with: tasty claude remote-results --id 0123456789abcdef`, 한국어로는 `surface 12: 턴 결과 0123456789abcdef (turn_ended)을(를) 저장했습니다. 읽기: tasty claude remote-results --id 0123456789abcdef` 처럼 옵니다. 스크립트로 결과 줄을 찾을 때는 `turn result` 같은 문구나 줄 안의 아무 16진수가 아니라 줄 끝의 `tasty claude remote-results --id <ID>`·`tasty codex remote-results --id <ID>` 명령 부분에서 claude/codex 와 ID(16자리 소문자 16진수)를 함께 꺼내세요. 예: `grep -E '(^| )tasty (claude|codex) remote-results --id ([0-9a-f]{16})$'`

  결과는 자식을 닫은 뒤에도 7일 동안 남습니다. 지시한 뒤 부모 세션이 바뀌었거나 부모가 닫혔으면 결과는 저장만 하고 알리지 않으니 목록에서 찾으세요. 연결 장애 중에는 같은 결과 줄이 한 번 더 올 수 있지만 같은 ID 이므로 다시 읽어도 됩니다.
- 원격 자식에게는 그 자식을 만든 부모만 `tell` 할 수 있습니다. 다른 부모(같은 화면에서 새로 시작한 Claude 대화 포함)가 보내면 응답이 엉뚱한 부모에게 가지 않도록 거절합니다. 그 부모용 자식을 새로 만드세요. 이 규칙이 생기기 전에 만든 원격 자식처럼 만든 부모를 알 수 없는 자식에게는 아무도 `tell` 할 수 없으니 새로 만드세요.
- 연결이 끊기면 지시가 실행됐는지 알 수 없다고 알릴 수 있습니다. 다시 시도하기 전에 `children` 으로 확인하세요. 연결을 직접 끊었다가 다시 연결하면 새 연결로 보며, 이전 연결에서 보낸 지시의 결과는 이어서 받지 못합니다.
- `--cwd`·`--profile-file` 은 원격 컴퓨터 기준의 절대 경로로 줍니다. 상대 경로나 원격 컴퓨터에 없는 경로면 자식을 만들지 않고 거절합니다.
- 원격 쪽 사용자는 연결된 동안 그 자식에게 `tell`·`kill`·`respawn`·`reboot` 를 할 수 없습니다. 연결한 쪽만 다룰 수 있습니다.
- 원격 자식에는 `respawn`·`broadcast` 를 쓸 수 없습니다.

### API 에러로 멈춘 Claude 자동 재개

서버 과부하 같은 일시적인 API 에러로 Claude 의 턴이 끝나면, Tasty 가 잠시 뒤 "이어서 진행해" 문구를 대신 보내 작업을 잇게 할 수 있습니다. **기본은 꺼져 있습니다.** **설정** <!-- en: Settings --> › **플러그인** <!-- en: Plugins --> › **Claude Code** 에서 켭니다.

- **일시적인 API 에러(서버 과부하) 뒤 자동으로 이어서 진행** <!-- en: Resume automatically after a temporary API error (server overload) --> — 켜기/끄기.
- **재개 전 대기 시간(초)** <!-- en: Seconds to wait before resuming --> — 기본 10초. 1초부터 하루(86400초)까지 정할 수 있고, 범위를 벗어난 값은 가장 가까운 끝으로 맞춥니다.
- **포기하기 전 연속 자동 재개 횟수** <!-- en: Consecutive automatic resumes before giving up --> — 기본 5번. 연달아 실패해 이 수에 닿으면 더 보내지 않고 알림을 한 번 띄웁니다.
- 요청 한도 초과·인증 실패·결제 문제처럼 다시 보내도 같은 에러는 재개하지 않습니다.
- 기다리는 동안 직접 입력하면 보내지 않습니다. 그 턴이 시작된 뒤 그 서피스에 키를 한 번이라도 누르거나 무언가를 붙여넣었다면(입력 칸에 쓰다 만 글이 있을 수 있으므로) 재개하지 않습니다.
- 자동으로 보낸 횟수는 서피스 메타 `claude-auto-resume-count` 에서 볼 수 있습니다.
- 이 기능은 `tasty claude install` 로 설치한 훅이 필요합니다. Tasty 를 업데이트한 뒤에는 다시 설치하세요.

## 5. Codex 승인 정책

`tasty codex spawn/launch/respawn/reboot` 는 Codex 의 승인·샌드박스 정책을 플래그로 받습니다.

`launch`/`spawn`/`respawn`/`reboot` 는 셸의 `codex` alias나 function을 거치지 않고 `PATH`에 설치된 Codex를 실행합니다. alias에 넣어 둔 승인·샌드박스 옵션은 아래 플래그나 전역 설정으로 지정하세요. 실행 명령은 sh/bash/zsh와 Windows Git Bash 문법을 씁니다.

- **승인**: `--approval untrusted|on-request|never`. 호출 옵션이 없으면 플러그인의 전역 기본값을 사용합니다. 그 설정도 없거나 **물려받음**이면 **`never`**로 실행됩니다. 자동화가 승인 응답을 기다리며 멈추지 않도록 정한 기본값입니다. 사람이 옆에서 승인해 줄 때만 `untrusted` / `on-request` 를 명시합니다.
- **샌드박스**: `--sandbox read-only|workspace-write|danger-full-access`. 호출 옵션이 없으면 플러그인의 전역 기본값을 사용하고, 그 설정도 없거나 물려받음이면 Codex 기본값을 사용합니다. 리뷰·교차검증용 자식은 `read-only` 가 적당합니다.
- `--full-auto`: 승인과 샌드박스를 모두 우회. `--approval`/`--sandbox` 와 함께 쓸 수 없습니다.
- 전역 기본값은 **설정** › **플러그인** › **Codex** 의 **기본 승인 정책** <!-- en: Default approval policy --> / **기본 샌드박스 모드** <!-- en: Default sandbox mode -->. 호출별 플래그가 우선합니다.
- 기존 대화를 다시 여는 `reboot`와 탭 복원에도 같은 정책이 적용됩니다. 정책을 적용하지 못하면 Codex를 띄우지 않고 터미널에 이유를 표시합니다.

컨테이너 등 중첩 샌드박스가 안 되는 환경에서 `--sandbox` 지정이 `RTM_NEWADDR: Operation not permitted` 류로 실패하면 `--full-auto` 를 씁니다. 완료 알림에도 이 힌트가 붙습니다.

### 자식별 모델과 추론 수준

`tasty codex spawn --workspace workers --model gpt-6.1-sol --effort medium --prompt "테스트를 실행해 줘"`처럼 자식마다 모델과 추론 수준을 지정할 수 있습니다. 두 옵션은 선택 사항이며 생략하면 기존 Codex 설정을 사용합니다. 전역 설정이나 부모·다른 자식의 모델은 바뀌지 않습니다. 같은 대화를 탭 복원하거나 `reboot`해도 명시한 값이 유지됩니다.

`--effort`는 `none|minimal|low|medium|high|xhigh|max|ultra`를 받습니다. 모델 식별자는 Codex가 제공하는 값을 사용하세요. 모델과 추론 수준의 지원 여부는 Codex가 판단하며, 거절되면 오류를 표시하고 다른 모델로 자동 전환하지 않습니다.


## 6. Claude 권한 모드

`tasty claude launch/spawn/respawn/reboot/child-profile` 는 자식 Claude 의 권한 모드를 플래그로 받습니다.

- `--permission-mode acceptEdits|auto|bypassPermissions|manual|dontAsk|plan` — Claude Code 에 그대로 전달됩니다.
- **호출 옵션이 없으면 플러그인의 전역 기본값을 사용합니다.** 그 설정도 없거나 **물려받음**이면 권한 모드 플래그를 붙이지 않고 기존 Claude Code 설정을 따릅니다. Tasty가 자동으로 승인 생략 모드로 바꾸지는 않습니다.
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

`tasty codex reboot` 의 성공 응답은 재시작을 예약했다는 뜻입니다. 재시작을 마쳤는지는 플러그인 로그(`tasty plugin logs com.tasty.codex`)에서 확인합니다. 기다리는 동안 같은 터미널에서 Codex를 새로 띄우면 재시작은 그 Codex에 입력하지 않고 멈춥니다.

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
tasty claude checklist-status                               # 지금 켜져 있는지 확인
tasty claude spawn --workspace w --profile continue-checklist
```

`checklist-enable`·`checklist-disable`·`checklist-status` 는 `--gate <이름>` 으로 다른 등록 게이트를 지정할 수 있습니다(기본 `continue-checklist`). 모든 게이트의 켜짐 여부는 `tasty claude gate-list` 로 한 번에 봅니다.

- 에이전트가 응답 끝에 `[[TASTY-CHECKLIST-DONE]]` 을 넣으면 통과, 아니면 체크리스트를 다시 받습니다. 라운드 상한(기본 3)에 닿으면 자동 통과합니다.
- 상한은 **설정** › **플러그인** › **Claude Code** 의 **게이트 기본 라운드 상한** <!-- en: Default gate round limit -->.
- 자기 게이트를 만들려면 `tasty claude gate-register <이름> --body-file <파일> [--sentinel <문자열>] [--rounds N]`. 본문에 센티넬 문자열이 들어 있어야 합니다. `gate-list` / `gate-show` 로 확인.

## 문제 해결

Windows와 macOS에서 Tasty 터미널의 Codex 세션이 연결 직후 끊기던 문제를 수정했습니다.

- **완료 알림이 안 옵니다** — 자식 CLI의 훅 설치·실행과 [수신 설정](#4-완료-알림-받기)을 확인하세요. 부모가 Claude Code이면 완료 알림 mod가 활성화됐는지 확인하세요. mod가 없을 때는 Monitor 구독을 확인하세요. 훅 전달 실패는 `~/.tasty/hook-failures.log` 에 남습니다. 플러그인 로그는 `tasty plugin logs com.tasty.claude --follow`.
- **`reboot` 가 "claude-session-id meta not set" 으로 실패합니다** — 세션 시작 훅이 세션 ID 를 못 남긴 것입니다. `tasty surface-meta set --key claude-session-id --value <세션ID>` 로 직접 넣습니다.
- **자식이 spawn 되지 않고 "occupied" 오류** — 대상 워크스페이스가 원격에서 attach 중이거나 mirror 입니다. 다른 워크스페이스를 씁니다.
- **`kill` 이 "hard-occupied" 오류로 거부됩니다** — 그 자식이 있는 워크스페이스를 원격 사용자가 attach 해 쓰는 중입니다. 자식과의 관계와 탭은 그대로 남습니다. 원격 쪽이 연결을 끊거나 강제 끊기로 점유를 푼 뒤 다시 `kill` 합니다.
- **Codex가 "app-server could not start"로 시작하지 않습니다** — Tasty가 그 터미널의 전용 Codex 서버를 띄우지 못한 것입니다. 같은 터미널에서 `codex app-server --help`가 실행되는지 확인하세요. 이 기능은 Codex CLI 0.154.0 기준으로 확인했습니다.
- **Codex 부모에게 결과가 오지 않습니다** — Codex의 `/hooks`에서 Tasty 훅이 승인됐는지, 자식을 Codex 대화 안에서 spawn했는지 확인하세요. 부모 Codex를 직접 입력한 `codex`로 띄웠다면 공유 Codex 서버에 그 대화가 열려 있어야 합니다.
- **macOS 에서 앱 아이콘으로 실행하면 알림이 안 옵니다** — Tasty 가 알림을 쓸 때 `tasty` 를 다시 호출하는데, Tasty 는 자기 실행 파일 경로를 자동으로 PATH 에 넣으므로 보통은 문제없습니다. 그래도 안 되면 `hook-failures.log` 를 봅니다.

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [작업 순서 관리](tasks.md) — spawn 과 tell 을 의존 관계로 묶어 한 그래프로 돌리기.
- [훅 · 알림 · 웹훅](hooks-notifications.md) — 완료 통지와 승인 게이트.

Codex 연동을 다시 설치하면 SessionEnd 훅도 등록됩니다. 현재 실행 세션이 종료되면 그 작업의 완료 대기도 끝납니다.
