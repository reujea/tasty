# 훅 · 알림 · 웹훅

훅은 Tasty의 이벤트에 맞춰 동작을 실행하고, 알림은 확인할 일을 앱 안에 표시합니다. 웹훅은 외부 서비스의 HTTP 요청을 받습니다. 필요한 경로를 골라 설정하세요.

- 이벤트에 반응하려면 훅을 설정하세요.
- 알림은 알림 패널과 서피스 표시에서 확인합니다.
- 외부 요청은 웹훅으로 받습니다. 접수 응답은 실행 완료를 뜻하지 않습니다.
- 결정과 응답을 기다려야 하면 승인 요청을 사용합니다.

## 서피스 훅

headless 인스턴스에서도 셸 종료 훅이 실행됩니다. 서피스를 닫으면 `--once` 없는 훅을 포함해 그 서피스에 남은 훅 등록도 정리됩니다.

특정 서피스(터미널)에서 이벤트가 일어나면 셸 명령을 실행합니다.

```sh
tasty set hook --surface 42 --event process-exit --command "tasty notify '셸 종료됨'"
tasty set hook --surface 42 --event 'output-match:error\[E\d+\]' --command 'tasty notify "$TASTY_HOOK_MATCHED_TEXT"'
tasty set hook --surface 42 --event idle-timeout:30 --command "tasty notify '30초간 출력 없음'" --once
tasty list hooks [--surface 42]
tasty unset hook --hook <HOOK_ID>
```

| 이벤트 | 언제 |
|---|---|
| `process-exit` | 셸 프로세스가 종료됐을 때 |
| `bell` | 벨(`\a`)을 받았을 때 |
| `notification` | 터미널 알림 시퀀스(OSC 9/99/777)를 받았을 때 |
| `output-match:<정규식>` | 출력의 **완성된 한 줄**이 정규식에 맞을 때 |
| `idle-timeout:<초>` | N초 동안 출력이 없을 때 (1초 단위, 새 출력이 오면 다시 대기) |
| `command-completed` | 셸 안에서 명령 하나가 끝났을 때 (bash/zsh 셸 통합 필요) |
| `command-completed:<코드>` | 그 종료 코드로 끝난 명령만 (예: `command-completed:1` = 실패한 명령) |
| `claude-idle` / `needs-input` / `codex-idle` | Claude Code / Codex 플러그인이 보내는 이벤트 ([Claude · Codex](claude-codex.md)) |
| `claude-stop-failure` | Claude Code 의 턴이 API 에러(서버 과부하·요청 한도·인증 실패 등)로 끝났을 때. 보통 `claude-idle` 과 함께 오고, 백그라운드 작업이 남아 있으면 이것만 옵니다 |
| `claude-error` | Claude Code 화면 출력에 알려진 에러 문구가 나타났을 때 |
| `claude-error-stalled` | 턴이 끝나지 않았는데 Claude Code 출력이 한동안 바뀌지 않아 멈춘 것으로 보일 때(앞선 에러가 없어도 옵니다) |

- `--once` 를 붙이면 한 번 실행 후 자동으로 지워집니다. 기본은 계속 유지.
- 훅 명령은 백그라운드에서 실행되며 터미널 입력을 막지 않습니다.
- `command-completed` 는 셸 통합 없이는 절대 발생하지 않습니다. 셸 통합이 없는 로컬 서피스에는 안내 배너가 한 번 뜨고 10초 뒤 저절로 닫힙니다.

명령에는 이벤트 정보가 환경변수로 들어옵니다.

| 변수 | 값 |
|---|---|
| `TASTY_HOOK_EVENT` | 등록한 이벤트 문자열 (`bell`, `output-match:…` 등) |
| `TASTY_HOOK_SURFACE_ID` | 이벤트가 난 서피스 ID |
| `TASTY_HOOK_MATCHED_TEXT` | `output-match` 에서 실제로 맞은 줄 전체 |
| `TASTY_HOOK_EXIT_CODE` | `command-completed` 의 종료 코드 |
| `TASTY_HOOK_IDLE_ELAPSED_SECS` | `idle-timeout` 에서 마지막 출력 후 지난 초 |
| `TASTY_HOOK_CUSTOM_EVENT` | 플러그인 이벤트(`claude-idle` 등)의 이름 |
| `TASTY_HOOK_SOURCE` | 훅이 실행했으면 `hook`, `tasty hook-handler dispatch` 로 직접 실행했으면 `dispatch` |

`TASTY_HOOK_EVENT`·`TASTY_HOOK_SOURCE`·`TASTY_HOOK_SURFACE_ID` 를 뺀 변수는 이벤트 정보의 키 이름에서 만들어집니다. 키의 영문자·숫자는 대문자로, 나머지 문자는 `_` 로 바꿔 `TASTY_HOOK_` 뒤에 붙입니다(`matched_text` → `TASTY_HOOK_MATCHED_TEXT`). 셸 명령으로 동작하는 훅 핸들러를 `tasty hook-handler dispatch --body '{"build-id":7}'` 로 실행하면 `--body` 객체의 최상위 키도 같은 규칙으로 들어옵니다(`TASTY_HOOK_BUILD_ID=7`). 문자열은 그대로, 다른 값은 JSON 글자로 들어가고 값마다 4096 바이트에서 잘립니다. 같은 이름이 생기면 먼저 만든 값을 씁니다.

## 글로벌 훅

서피스와 무관하게 시간 조건으로 실행합니다.

```sh
tasty set global-hook --condition interval:60 --command "df -h > ~/disk.log" --label "디스크 기록"
tasty set global-hook --condition once:300 --command "tasty notify '5분 지남'"
tasty list global-hooks
tasty unset global-hook --hook <HOOK_ID>
```

| 조건 | 뜻 |
|---|---|
| `interval:<초>` | N초마다 반복 |
| `once:<초>` | N초 뒤 한 번 실행하고 삭제 |

## 훅 핸들러 (재사용 가능한 동작)

`--command` 대신 미리 등록한 **훅 핸들러**를 이름으로 연결할 수 있습니다. 같은 핸들러를 여러 훅과 웹훅이 공유합니다.

```sh
tasty hook-handler list                                     # 등록된 핸들러 (host / plugin / user)
tasty set hook --surface 42 --event bell --handler user/my-handler
tasty hook-handler get --id user/my-handler                 # 한 건을 자세히 (하는 일까지)
tasty hook-handler dispatch --id user/my-handler            # 직접 실행해 테스트
tasty hook-handler reload                                   # ~/.tasty/hook-handlers.toml 다시 읽기
```

`dispatch` 는 받아들였다는 줄(`run` 번호 포함)을 먼저 출력하고, 끝날 때까지 기다린 뒤 결과 줄을 출력합니다. 결과의 `state` 는 모두 성공하면 `succeeded`, 일부만 성공하면 `partial`, 모두 실패하면 `failed`, 응답을 받지 못해 결과를 알 수 없으면 `unknown` 이고, `result` 와 `steps` 에 어느 단계가 왜 실패했는지 나옵니다. 셸 명령 핸들러는 프로세스가 시작되면 성공으로 보며, 끝났을 때의 종료 코드는 참고로만 보여 줍니다. 결과가 `succeeded` 가 아니면 명령은 종료 코드 1로 끝납니다. 기다리지 않으려면 `--no-wait` 를 붙이고, 나중에 `tasty hook-handler result --run <번호>` 로 결과를 확인하세요.

핸들러를 **고치거나 새로 만드는** 것도 명령으로 됩니다. 적지 않은 항목은 그대로 두므로,
이름을 유지한 채 하는 일만 바꿀 수 있습니다 (그 핸들러를 가리키는 훅들은 그대로 따라옵니다).

```sh
# 하는 일만 바꾸기 — 먼저 get 으로 지금 무엇을 하는지 보고, 그 모양 그대로 돌려줍니다
tasty hook-handler upsert --id user/my-handler \
  --calls '[{"method":"notification.create","params":{"body":"빌드 끝"}}]'

# 잠시 꺼두기 / 다시 켜기
tasty hook-handler upsert --id user/my-handler --disabled true

# 내가 만든 항목 지우기
tasty hook-handler remove --id user/my-handler
```

바꾼 내용은 먼저 현재 실행에 적용되고, 명령은 `~/.tasty/hook-handlers.toml` 저장 결과를 받은 뒤 끝납니다. 저장 실패는 오류로 표시되며 현재 실행에 적용된 변경은 유지됩니다. 다만 **이미 만들어 둔 웹훅 주소는 따라오지 않습니다** — 웹훅은 등록하던 순간의 동작을 그대로 들고 있어서, 새 동작을 쓰려면 그 웹훅을 다시 등록하세요.

사용자 핸들러는 **설정** <!-- en: Settings --> › **핸들러** <!-- en: Handler --> › **훅 핸들러** <!-- en: Hook Handlers --> 탭에서 추가·편집합니다. 저장하면 현재 실행에 먼저 적용하고 `~/.tasty/hook-handlers.toml` 기록은 뒤에서 진행합니다. 저장하지 못하면 토스트와 알림에 표시되며 다음 시작에 변경이 남지 않을 수 있습니다. 파일을 직접 써도 됩니다 (`tasty hook-handler reload` 로 반영). 저장 중에는 reload를 거절하므로 완료 뒤 다시 실행하세요. 파일 읽기나 형식 오류가 있으면 이전 설정을 유지합니다. 시작할 때 설정을 읽지 못하면 로그에 파일명과 문제 종류를 남기며, 파일 내용은 기록하지 않습니다.

목록의 각 줄에는 핸들러를 등록한 쪽이 표시됩니다 — Tasty 자신은 `host`, 플러그인은 그 플러그인 이름, 직접 만든 것은 `you` 입니다. 지울 수 있는 줄에만 휴지통이 붙고, 나머지 줄에는 자물쇠가 놓입니다 (Tasty 와 플러그인이 시작할 때마다 자기 핸들러를 다시 등록하기 때문입니다). 여러 내부 동작을 잇는 핸들러는 그 순서가 한 줄로 보이고, 줄 끝의 **편집** <!-- en: Edit --> 을 누르면 그 자리에서 글로 고칠 수 있습니다. 한 줄에 동작 하나씩, 메서드 이름 뒤에 필요하면 JSON 매개변수를 씁니다 (`#` 으로 시작하는 줄은 건너뛰며 저장되지 않습니다). 잘못 쓴 줄이 있으면 첫 번째 문제와 그 줄 번호가 입력칸 아래에 나오고, 고칠 때까지 **적용** <!-- en: Apply --> 이 눌리지 않습니다. 적용한 내용은 설정 윈도우 아래의 **저장** <!-- en: Save --> 을 눌러야 파일에 기록됩니다. 입력칸 안에서 Ctrl+Enter(macOS 는 Cmd+Enter)는 적용, Esc 는 취소입니다 — 이 두 키는 [단축키](../customize/keybindings.md) 설정의 **코드 입력칸 적용** · **코드 입력칸 취소** 에서 바꿀 수 있습니다. Tasty 나 플러그인이 등록한 핸들러를 고치면 같은 이름의 내 설정으로 저장됩니다. 이렇게 저장한 핸들러는 나중에 Tasty 나 플러그인이 기본 동작을 바꿔도 **따라가지 않습니다**. 이런 줄에는 출처 옆에 **수정됨** <!-- en: edited --> 표시가 붙고, 둘째 줄의 **되돌리기** <!-- en: Revert --> 를 누르면 기본값으로 돌아갑니다. 되돌리기는 **저장** 을 눌러야 적용되며, 그전까지는 **저장 시 되돌림** <!-- en: reverts on save --> 표시와 함께 같은 자리의 **실행 취소** <!-- en: Undo --> 로 취소할 수 있습니다. 기다리는 동안에는 켜고 끄는 스위치가 기본값으로 잠기고 **편집** 도 눌리지 않습니다 — 바꾸려면 먼저 **실행 취소** 를 누르세요. 터미널에서는 `tasty hook-handler remove --id <id>` 가 같은 일을 합니다 (내 설정만 지워지고 원래 핸들러가 다시 적용됩니다). 글로 옮길 수 없는 이름의 동작이 들어 있으면 편집 대신 **CLI로 편집** <!-- en: Edit with CLI --> 과 복사 버튼이 나옵니다 — 복사되는 `tasty hook-handler get --id <id>` 로 내용을 보고 위의 `tasty hook-handler upsert` 로 바꾸세요.

```toml
[[handler]]
id = "user/notify-fail"
source = "hook"          # hook | webhook | any — 어느 트리거가 이 핸들러를 쓸 수 있나
[handler.action]
kind = "shell_command"
command = "tasty"
args = ["notify", "명령 실패", "--title", "hook"]
```

`kind = "ipc_sequence"` 로 두면 셸 대신 Tasty 내부 동작 목록(`calls = [{ method = "...", params = {} }]`)을 실행합니다. TOML에는 null이 없으므로, params 안에 null 값이 필요하면 `params` 대신 JSON 문자열로 `params_json = '{"a":null}'` 처럼 적습니다. `tasty hook-handler upsert`로 저장하면 이 형식으로 자동 기록됩니다.

훅이 스스로 실행될 때 실패하면 알림 목록에 남습니다. 핸들러가 없어졌거나 명령을 띄우지 못해 아예 실행되지 않았으면 **오류**, 동작 목록 중 일부 단계만 실패했거나 제때 응답이 없어 결과를 모르면 **경고**입니다. 서피스 훅의 알림은 그 훅이 걸린 서피스에 붙습니다. 셸 명령이 0이 아닌 종료 코드로 끝난 것은 실패로 보지 않습니다. 같은 실패가 짧은 간격으로 되풀이되면 10초에 한 번만 남습니다.

## 알림

### 보내기

```sh
tasty notify "빌드 완료"                    # 제목은 기본값 "알림"
tasty notify "테스트 3개 실패" --title "cargo test" --level error --surface 42
tasty list notifications
```

`list notifications`는 모든 윈도우의 알림 중 새로 생성된 순서로 최근 50개를 반환합니다. 알림 ID는 같은 실행 중 윈도우 사이에서도 중복되지 않습니다. 짧은 간격의 알림이 기존 항목에 합쳐지면 그 항목의 순서는 유지됩니다. 화면의 알림 패널은 해당 윈도우의 알림을 보여줍니다. 집계와 명시적 읽음 명령은 모든 윈도우를 대상으로 할 수 있습니다.


터미널 프로그램이 보내는 알림 시퀀스(OSC 9 / 99 / 777)와 벨도 같은 알림으로 모입니다.

크리티컬한 실패는 `--level error`, 비치명적인 문제는 `--level warning`, 정보는 `--level info`로 보냅니다. 생략하면 `info`입니다. 서로 다른 레벨은 합쳐지지 않으며 읽은 항목에 새 내용이 합쳐지면 다시 미확인 상태가 됩니다.

```sh
tasty notification counts
tasty notification counts --workspace 12
tasty notification read 7
tasty notification read --all --workspace 12
```

집계는 목록의 최근 50개 제한과 무관하게 보관된 모든 미확인 알림을 셉니다. 개별 읽음에는 `list notifications`에 표시된 알림 ID를 사용합니다. 워크스페이스 필터 없이 `read --all`을 실행하면 모든 윈도우가 대상입니다. 같은 서피스의 마지막 미확인 알림을 읽으면 해당 주의 표시도 해제되지만 하드 점유 중에는 유지됩니다. 알림 개수와 서피스 개수는 별도로 집계합니다.

서피스와 무관한 알림은 `notify --workspace 12`로 보냅니다. 이 호출은 서피스 주의 표시를 추가하지 않습니다. 워크스페이스를 명시하면 환경의 서피스 ID를 자동으로 추가하지 않습니다.

### 어디에 나타나나

- **사이드바 알림 요약** — 도구 버튼 바로 위에 미확인 오류(빨강)·경고(노랑)·정보(파랑) 개수가 표시됩니다. 0인 레벨은 숨기고 99개를 넘으면 `99+`로 표시합니다. 사이드바를 펼치면 최신 미확인 제목도 보입니다. 요약을 누르면 알림 패널이 열립니다. 라이트 테마의 경고 배지는 밝은 노란 배경과 어두운 숫자로 표시됩니다. 목록의 경고 아이콘도 배경과 구분되는 어두운 색이며 읽음 처리 후에도 같은 색을 유지합니다.
- **알림 패널** — 요약 클릭이나 `Ctrl+Shift+I`로 엽니다. 레벨·제목·본문·출처·경과 시간이 최신순으로 보이며, 열거나 닫기만 해서는 읽음 처리되지 않습니다. 행이나 **더 보기** <!-- en: More --> 를 누르면 전문을 펼치고 해당 알림을 읽음 처리합니다. 체크 버튼으로 한 건을, **모두 읽음** <!-- en: Mark all read --> 으로 해당 윈도우의 알림을 처리할 수 있습니다. **이동** <!-- en: Jump --> 은 알림을 보낸 서피스 또는 워크스페이스로 갑니다. 출처가 닫혀도 기록은 남고 이동 버튼은 비활성화됩니다. 제목 표시줄에서 전체화면으로 전환할 수 있습니다. 새 알림이 와도 패널이 자동으로 열리거나 보던 목록이 맨 위로 이동하지 않습니다. 기존 알림에 내용이 합쳐져 본문이 길어져도 읽던 위치를 유지합니다.
- **서피스 테두리** — 알림이 난 서피스에 파란 테두리. Tasty 윈도우가 활성인 상태에서 그 서피스로 포커스하면 사라집니다. 다른 앱을 보는 동안에는 남아 있다가 윈도우로 돌아오면 사라집니다.
- **사이드바 배지** — 주의가 필요한 서피스가 있는 워크스페이스 행에 개수 배지.
- **소리** — 설정에서 켜면 알림마다 시스템 비프 1회.

OS 알림 센터(시스템 알림)로는 보내지 않습니다. 다른 앱을 쓰는 동안에도 알아차리려면 소리를 켜세요.

### 설정

**설정** › **일반** › **알림** <!-- en: Notifications --> 탭, 또는 `~/.tasty/config.toml`:

```toml
[notification]
enabled = true        # 알림 활성화
sound = false         # 소리
coalesce_ms = 500     # 알림 병합 간격 (ms) — 같은 출처의 연속 알림을 하나로 합침

[general]
bell_notification = true   # 벨 알림 표시 (끄면 벨 토스트만 억제, bell 훅은 그대로 발화)
```

### 사람의 결정 기다리기 (승인)

작업을 진행하기 전에 사용자의 판단이 필요하다면 승인 요청을 보내세요. 에이전트가 요청을 보낸 뒤 응답을 기다릴 수 있습니다.

```sh
ID=$(tasty approval request --title "prod DB 마이그레이션 실행?" --severity danger \
      --choices "approve:실행,deny:중단:1" --timeout-ms 600000 | jq -r .id)
tasty approval await --id "$ID"            # 응답이 올 때까지 대기, 결과를 JSON 으로 출력
```

- 팝업이 뜨고 알림도 함께 갑니다. 사용자는 팝업 버튼 클릭, 팝업에서 숫자 키 `1`~`9`(선택지 순서), 또는 `tasty approval respond --id <ID> --choice approve` 로 답합니다.
- 팝업은 키보드 포커스를 가져가지 않습니다. 터미널이나 문서에 치던 글자는 그대로 그곳에 들어갑니다. 숫자 키로 고르려면 먼저 팝업의 제목이나 빈 곳을 누르세요. 버튼은 바로 누를 수 있습니다. 마크다운·HTML 문서에 입력하는 중이면 그 문서가 팝업과 겹치는 부분을 가릴 수 있습니다. 함께 가는 알림으로도 요청을 확인할 수 있습니다.
- 코멘트 칸에 입력하는 동안 친 숫자는 코멘트 글자로 들어가고 선택지를 고르지 않습니다. 코멘트를 쓴 뒤 숫자 키로 고르려면 Tab으로 코멘트 칸을 벗어나거나 팝업의 빈 곳을 누르세요.
- `--severity info` 는 팝업 없이 알림만, `warn`/`danger` 는 팝업 + 알림. `danger` 는 사용자가 직접 답해야 합니다.
- 팝업은 Esc 로 닫히지 않습니다 (우회 방지). `tasty approval list` / `get` / `history` 로 조회.
- `approval await` 는 사용자 전용입니다. 세션 토큰(`TASTY_SESSION_TOKEN`)으로 접속한 에이전트는 부를 수 없으니 `tasty approval get --id "$ID"` 로 상태를 확인하세요(`approval` 권한이 필요합니다).
- `--metadata` 의 `kind` 에 `capability_elevation` 은 쓸 수 없습니다. 에이전트 권한 요청 전용 값이라 요청이 거절됩니다. 에이전트 권한은 `tasty plugin request-permission` 으로 요청하세요.
- 요청은 워크스페이스에 묶입니다. `--workspace` 를 주면 그곳, `--surface` 만 주면 그 터미널이 있는 워크스페이스, 둘 다 없으면 지금 보고 있는 워크스페이스입니다(예전 표기 `--workspace-id`·`--surface-id` 도 받습니다). 에이전트가 요청하면 자기 터미널을 `--surface "$TASTY_SURFACE_ID"` 로 주는 것이 안전합니다 — 사용자가 다른 워크스페이스를 보고 있어도 같은 곳에 묶입니다.

## 웹훅 (외부 → Tasty)

CI 나 다른 서비스가 HTTP 요청을 보내 Tasty 안의 동작을 일으키게 합니다. Tasty 는 웹훅용 포트 하나를 열고, 웹훅마다 추측하기 어려운 URL을 발급합니다.

웹훅은 추측하기 어려운 URL과 선택적으로 설정하는 고정 토큰(`--auth-*`)을 사용합니다. **HMAC 서명 검증은 지원하지 않으므로**, 외부 서비스가 보낸 서명 헤더는 검사하지 않습니다. **인증을 설정하지 않으면 URL에 접근할 수 있는 누구나 동작을 요청할 수 있습니다.** 필요한 인증 옵션을 등록할 때 함께 지정하세요.

### 포트와 외부 수신

```sh
tasty webhook port                  # 실행 중인 리스너의 주소·포트와 정한 방법
tasty webhook port 28500            # 다음 시작부터 이 포트를 사용
tasty webhook port --unset          # 저장한 포트를 지워 다음 시작부터 빈 포트를 자동 선택
tasty webhook allow-external on     # 다음 시작부터 다른 컴퓨터의 호출도 받음 (off 로 되돌림)
tasty webhook allow-external        # 저장한 값과 실행 중인 리스너가 실제로 연 주소 보기
tasty --webhook-port 28500          # 이번 실행만 이 포트로 시작
```

- **포트를 정하지 않으면** `28429` 부터 하나씩 올리며 비어 있는 포트를 씁니다(최대 64개, `28492` 까지). 다른 Tasty가 이미 `28429` 를 쓰고 있어도 웹훅이 동작합니다. 모두 쓰이고 있으면 알림 목록에 경고를 남기고 웹훅 없이 실행합니다.
- **포트를 정했다면**(`--webhook-port` 또는 `tasty webhook port <N>`, 둘 다 있으면 `--webhook-port` 가 우선) 그 포트만 씁니다. 다른 프로그램이 쓰고 있으면 다른 포트로 옮기지 않고 **웹훅 없이 시작합니다**. 터미널은 평소처럼 쓸 수 있고, 알림 목록에 포트와 원인, 고치는 방법을 담은 경고가 하나 남습니다. 이때 웹훅 주소(저장한 `--persistent` 웹훅 포함)는 응답하지 않지만 등록은 그대로 남아, 포트를 고쳐 다시 시작하면 같은 등록으로 다시 받습니다. 주소의 포트는 그 실행에서 연 포트로 바뀌므로 실제 주소는 `tasty webhook list` 에서 확인하세요. `tasty webhook port` 는 `bound: false` 와 `unavailable` 에 열지 못한 포트와 원인을 보여 줍니다. 다른 포트로 시작하거나, 저장한 포트라면 `tasty webhook port --unset`(지우기) 또는 `tasty webhook port <빈 포트>`(바꾸기)를 실행한 뒤 다시 시작하세요. 이 두 명령은 Tasty가 실행 중이 아니어도 `~/.tasty/webhooks.toml` 을 직접 고치고 그렇게 했다고 알려 줍니다. Tasty가 실행 중이면 다음 시작부터 적용됩니다.
- `tasty webhook config --port <N>` 은 이전 이름이며 더 이상 권장하지 않습니다(deprecated). 지금도 `tasty webhook port <N>` 과 똑같이 동작합니다.
- 포트가 `28429` 에서 밀렸는데 `--persistent` 로 저장한 웹훅이 있으면 알림 목록에 경고를 남깁니다. 저장한 웹훅의 주소 포트도 바뀌므로 `tasty webhook list` 로 새 URL을 확인하세요. 주소가 바뀌면 안 되는 경우에는 포트를 정해 두세요.
- Tasty가 이미 실행 중인데 다른 `--webhook-port` 로 다시 실행하면, 실행 중인 Tasty로 넘기지 않고 안내 후 끝납니다. 같은 포트면 평소처럼 실행 중인 Tasty에 새 윈도우를 엽니다.
- 이전 버전이 자동으로 써 넣은 `port = 28429` 는 처음 실행할 때 지워져 자동 선택으로 바뀝니다. 직접 정한 다른 포트는 그대로 유지됩니다.
- **기본으로 웹훅 서버는 이 컴퓨터(`127.0.0.1`)에서 오는 호출만 받습니다.** 다른 컴퓨터나 CI가 직접 불러야 하면 **설정 → 일반 → 다른 컴퓨터의 웹훅 호출 받기**를 켜거나 `tasty webhook allow-external on` 을 실행한 뒤 Tasty를 다시 시작합니다. `~/.tasty/config.toml` 의 `[webhook]` 아래 `allow_external = true` 로 적어도 같습니다. 켜면 모든 IPv4 네트워크 인터페이스(`0.0.0.0`)에서 받으므로, 호스트 방화벽이 허용하면 공유기 포워딩 없이도 같은 네트워크(사내망·공용 Wi-Fi 등)에서 접근할 수 있습니다. 방화벽과 인증 설정을 확인하세요. HTTPS는 앞단 리버스 프록시에서 처리합니다.

### 등록

```sh
# 등록된 핸들러에 연결
tasty webhook register --method POST --handler host/webhook-notify --persistent

# 인라인 동작 정의 — 바디의 값을 ${body.x} 로 끌어다 쓰기
tasty webhook register --method POST \
  --sequence '[{"method":"notification.create","params":{"title":"CI","body":"${body.status}"}}]' \
  --ttl-secs 3600 \
  --auth-location header --auth-key X-Token --auth-token s3cret
```

등록하면 `http://127.0.0.1:<포트>/<16자리 id>` 형태의 URL 이 출력됩니다. 포트는 이번 실행에서 실제로 연 포트입니다. 외부 수신을 켰다면 바깥에서 부를 때 호스트 부분을 실제 주소로 바꿉니다 — 그 자리에서 바로 `curl` 하기 편하라고 넣은 표기입니다. 반환된 경로를 그대로 사용하세요. `/webhook/` 접두사는 붙이지 않습니다.

| 옵션 | 뜻 |
|---|---|
| `--method <M>` | 허용 HTTP 메서드 (반복 가능, 기본 POST) |
| `--handler <id>` 또는 `--sequence <json>` | 둘 중 하나 필수 |
| `--persistent` | 재시작 후에도 유지 (기본은 재시작 시 사라짐). `--auth-token` 을 함께 걸면 그 토큰도 `~/.tasty/webhooks.toml` 에 평문으로 저장됩니다 |
| `--ttl-secs <초>` / `--count <N>` | 시간 제한 / **접수된** 호출 횟수 제한 (둘 중 하나). 인증에 걸린 요청(`401`)과 허용되지 않은 메서드(`405`)는 횟수를 깎지 않습니다 |
| `--auth-location query\|bearer\|body\|header` + `--auth-token` (+ `--auth-key`) | 선택 인증. 안 걸면 무인증 |

실행 중 포트를 조회하거나 바꾸면 파일 처리가 끝난 뒤 결과를 받습니다. 저장할 수 없으면 원본을 유지하고 오류를 돌려주며, 실행 중인 리스너 주소는 바뀌지 않습니다.

영속 웹훅의 등록·해제는 파일 저장이 끝난 뒤 결과를 받습니다. 저장하지 못하면 이전 등록을 유지합니다. 저장을 기다리는 동안 다른 Tasty 작업은 계속할 수 있습니다.

포트 설정과 영속 등록은 같은 파일에 저장하지만 서로의 내용을 지우지 않습니다. 파일을 읽거나 해석하지 못하면 원본을 그대로 두고 저장을 거절합니다. 직접 편집할 때 주석과 공백은 다음 저장에서 달라질 수 있습니다.

`--ttl-secs`와 `--count`를 모두 생략하면 호출 제한이 없습니다. `--persistent`는 제한 종류와 함께 사용할 수 있으며, 재시작할 때 기한이나 횟수가 남은 등록을 복원합니다.

쓸 수 있는 핸들러 id 는 `tasty hook-handler list` 로 확인합니다. `--sequence` 는 Tasty 내부 동작(알림 보내기, 텍스트 전송 등)을 순서대로 적은 JSON 목록입니다. 올바른 JSON 이 아니면 아무것도 등록하지 않고 원인을 적은 오류로 끝납니다(종료 코드 1).

### 응답과 관리

호출한 서비스에는 상태 코드와 고정 응답 문구가 전달됩니다. 실제 동작의 결과는 응답에 포함되지 않습니다.

**`200` 은 "실행됐다" 가 아니라 "접수했다" 입니다.** 응답은 동작이 시작되기 전에 정해지므로 동작이 통째로 실패해도 호출 측은 `200` 을 받습니다. 여러 단계짜리 `--sequence` 는 한 단계가 실패해도 다음 단계가 계속 실행돼 절반만 적용된 채로 끝날 수 있습니다. 무엇이 실패했는지는 Tasty의 알림 목록에 남습니다(아무 단계도 성공하지 못하면 오류, 일부만 실패하면 경고). 상태 코드로 재시도를 판단하는 CI 라면 `200` 을 "성공" 으로 세지 않도록 주의합니다.

| 코드 | 뜻 |
|---|---|
| 200 `received` | 접수됨 (동작은 백그라운드 실행) |
| 401 `unauthorized` | 인증 실패 (반복하면 429 로 차단됩니다) |
| 404 | 없는 URL |
| 405 | 허용되지 않은 메서드 |
| 410 | 기한·횟수 만료 |
| 413 | 본문이 요청당 크기 상한(기본 1 MiB)을 넘음 |
| 429 | 같은 출처가 10초에 20회 이상 실패(`401`·`404`·`405`·`413`)해 60초 차단 중 |
| 503 `unavailable` | 요청을 접수하지 못했거나 저장 결과를 확정하지 못함. 동작은 실행하지 않음 |

본문 크기 제한은 인증보다 먼저 적용됩니다. 청크로 전송한 본문에 잘못된 UTF-8 바이트가 있어도 같은 크기 제한이 적용됩니다. 토큰이 없어도 본문이 상한을 넘으면 `413 payload too large`를 받으며 동작은 실행되지 않습니다. 이 상한은 처리할 본문의 크기를 제한합니다. 413 응답에는 `Connection: close`가 붙고, 서버는 남은 본문을 받지 않고 연결을 닫습니다. 차단 중(`429`)인 요청도 같습니다. 같은 연결로 다음 요청을 보낼 수 없으니 새 연결을 여세요. 본문을 보내는 도중이었다면 연결이 재설정(reset)으로 끊길 수 있습니다. 총 메모리나 동시 연결 수를 제한하는 기능은 아닙니다.

횟수·만료 변경의 저장 실패가 확인됐을 때는 경고를 남기고 기존처럼 접수와 동작을 계속합니다. 이 경우 재시작 후 저장하지 못한 변경이 되돌아올 수 있습니다. 접수 거절이나 저장 결과를 알 수 없는 경우에는 `503`을 보내고 동작을 실행하지 않습니다. 결과가 불명확해진 뒤에는 Tasty를 다시 시작해 저장된 등록을 복원하기 전까지 새 변경을 거절합니다.

차단 상태는 메모리에만 있습니다 — Tasty 를 재시작하면 남은 차단 시간이 사라지고 그 출처는 다시 처음부터 시작합니다.

```sh
tasty webhook list
tasty webhook info --id <ID>
tasty webhook unregister --id <ID>
tasty webhook sweep                 # 만료된 웹훅 일괄 정리
```

웹훅은 셸 명령을 직접 실행할 수 없습니다 (Tasty 내부 동작만). 셸이 필요하면 웹훅 → 알림 → 서피스 훅처럼 이어 붙입니다.

## 조합 예: 긴 빌드가 끝나면 알리기

```sh
tasty set hook --surface 42 --event command-completed --once \
  --command 'if [ "$TASTY_HOOK_EXIT_CODE" = 0 ]; then tasty notify "빌드 성공" --title build; else tasty notify "빌드 실패 ($TASTY_HOOK_EXIT_CODE)" --title build; fi'
tasty send text "cargo build --release\r" --surface 42
```

`command-completed` 는 모든 종료 코드에 걸리고, `command-completed:0` 처럼 정수 하나를 붙이면 그 코드일 때만 걸립니다. 실제 종료 코드는 훅 명령에 `TASTY_HOOK_EXIT_CODE` 환경변수로 들어옵니다.
