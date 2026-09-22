# 훅 · 알림 · 웹훅

빌드가 끝났을 때 알림을 받거나, 로그에 특정 문구가 나오면 명령을 실행하도록 설정해 보세요. **훅**은 이벤트에 맞춰 명령을 실행하고, **알림**은 확인할 일이 생겼음을 알려줍니다. 외부 서비스에서 Tasty에 요청을 보내려면 **웹훅**을 사용할 수 있습니다.

## 서피스 훅

headless 인스턴스에서도 셸 종료 훅이 실행됩니다. 서피스를 닫으면 `--once` 없는 훅을 포함해 그 서피스에 남은 훅 등록도 정리됩니다.

특정 서피스(터미널)에서 이벤트가 일어나면 셸 명령을 실행합니다.

```sh
tasty set hook --surface 42 --event process-exit --command "tasty notify '셸 종료됨'"
tasty set hook --surface 42 --event 'output-match:error\[E\d+\]' --command "tasty notify \"$TASTY_HOOK_MATCHED_TEXT\""
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

- `--once` 를 붙이면 한 번 실행 후 자동으로 지워집니다. 기본은 계속 유지.
- 훅 명령은 백그라운드에서 실행되며 터미널 입력을 막지 않습니다.
- `command-completed` 는 셸 통합 없이는 절대 발생하지 않습니다. 셸 통합이 없는 서피스에는 안내 배너가 한 번 뜹니다.

명령에는 이벤트 정보가 환경변수로 들어옵니다.

| 변수 | 값 |
|---|---|
| `TASTY_HOOK_EVENT` | 등록한 이벤트 문자열 (`bell`, `output-match:…` 등) |
| `TASTY_HOOK_SURFACE_ID` | 이벤트가 난 서피스 ID |
| `TASTY_HOOK_MATCHED_TEXT` | `output-match` 에서 실제로 맞은 줄 전체 |
| `TASTY_HOOK_EXIT_CODE` | `command-completed` 의 종료 코드 |
| `TASTY_HOOK_IDLE_ELAPSED_SECS` | `idle-timeout` 에서 마지막 출력 후 지난 초 |

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
tasty hook-handler dispatch --id user/my-handler            # 손으로 발화해 테스트
tasty hook-handler reload                                   # ~/.tasty/hook-handlers.toml 다시 읽기
```

핸들러를 **고치거나 새로 만드는** 것도 명령으로 됩니다. 적지 않은 항목은 그대로 두므로,
이름을 유지한 채 하는 일만 바꿀 수 있습니다 (그 핸들러를 가리키는 훅들은 그대로 따라옵니다).

```sh
# 하는 일만 바꾸기 — 먼저 get 으로 지금 무엇을 하는지 보고, 그 모양 그대로 돌려줍니다
tasty hook-handler upsert --id user/my-handler \
  --calls '[{"method":"notification.create","params":{"message":"빌드 끝"}}]'

# 잠시 꺼두기 / 다시 켜기
tasty hook-handler upsert --id user/my-handler --disabled true

# 내가 만든 항목 지우기
tasty hook-handler remove --id user/my-handler
```

바꾼 내용은 바로 `~/.tasty/hook-handlers.toml` 에 저장됩니다. 다만 **이미 만들어 둔 웹훅 주소는 따라오지 않습니다** — 웹훅은 등록하던 순간의 동작을 그대로 들고 있어서, 새 동작을 쓰려면 그 웹훅을 다시 등록하세요.

사용자 핸들러는 **설정** <!-- en: Settings --> › **핸들러** <!-- en: Handlers --> › **훅 핸들러** <!-- en: Hook Handlers --> 탭에서 추가·편집합니다. 저장하면 `~/.tasty/hook-handlers.toml` 에 기록되며, 파일을 직접 써도 됩니다 (`tasty hook-handler reload` 로 반영).

목록의 각 줄에는 그것을 심은 쪽이 표시됩니다 — Tasty 자신은 `host`, 플러그인은 그 플러그인 이름, 직접 만든 것은 `you` 입니다. 지울 수 있는 줄에만 휴지통이 붙고, 나머지 줄에는 자물쇠가 놓입니다 (Tasty 와 플러그인이 시작할 때마다 자기 핸들러를 다시 심기 때문입니다). 여러 내부 동작을 잇는 핸들러는 그 순서가 한 줄로 보이며, 탭 안에서는 고칠 수 없습니다 — 위의 `tasty hook-handler upsert` 로 바꾸거나, 파일을 고치고 `tasty hook-handler reload` 하면 됩니다.

```toml
[[handler]]
id = "user/notify-fail"
source = "hook"          # hook | webhook | any — 어느 트리거가 이 핸들러를 쓸 수 있나
[handler.action]
kind = "shell_command"
command = "tasty"
args = ["notify", "명령 실패", "--title", "hook"]
```

`kind = "ipc_sequence"` 로 두면 셸 대신 Tasty 내부 동작 목록(`calls = [{ method = "...", params = {} }]`)을 실행합니다.

## 알림

### 보내기

```sh
tasty notify "빌드 완료"                    # 제목은 기본값 "알림"
tasty notify "테스트 3개 실패" --title "cargo test"
tasty list notifications
```

`list notifications`는 모든 윈도우의 알림 중 새로 생성된 순서로 최근 50개를 반환합니다. 알림 ID는 같은 실행 중 윈도우 사이에서도 중복되지 않습니다. 짧은 간격의 알림이 기존 항목에 합쳐지면 그 항목의 순서는 유지됩니다. 화면의 알림 패널과 읽음 처리는 각 윈도우에 남습니다.


터미널 프로그램이 보내는 알림 시퀀스(OSC 9 / 99 / 777)와 벨도 같은 알림으로 모입니다.

### 어디에 나타나나

- **알림 패널** — `Ctrl+Shift+I` (macOS `Cmd+Shift+I`) 로 엽니다. 최신순 목록에 워크스페이스·제목·본문·경과 시간이 보이고, **이동** <!-- en: Jump --> 으로 그 워크스페이스로 갑니다. 열면 전부 읽음 처리되며 **모두 읽음** <!-- en: Mark all read --> 버튼도 있습니다.
- **서피스 테두리** — 알림이 난 서피스에 파란 테두리. 그 서피스로 포커스하면 사라집니다.
- **사이드바 배지** — 주의가 필요한 서피스가 있는 워크스페이스 행에 개수 배지.
- **OS 알림** — Tasty 윈도우가 비활성일 때 시스템 알림 (초당 1회 제한).
- **소리** — 설정에서 켜면 알림마다 시스템 비프 1회.

### 설정

**설정** › **알림** <!-- en: Notifications --> 탭, 또는 `~/.tasty/config.toml`:

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
      --choices "approve:실행,deny:중단:1" --timeout-ms 600000)
tasty approval await --id "$ID"            # 응답이 올 때까지 대기, 결과를 JSON 으로 출력
```

- 팝업이 뜨고 알림도 함께 갑니다. 사용자는 팝업 버튼 클릭, 팝업에서 숫자 키 `1`~`9`(선택지 순서), 또는 `tasty approval respond --id <ID> --choice approve` 로 답합니다.
- `--severity info` 는 팝업 없이 알림만, `warn`/`danger` 는 팝업 + 알림. `danger` 는 사용자가 직접 답해야 합니다.
- 팝업은 Esc 로 닫히지 않습니다 (우회 방지). `tasty approval list` / `get` / `history` 로 조회.

## 웹훅 (외부 → Tasty)

CI 나 다른 서비스가 HTTP 요청을 보내 Tasty 안의 동작을 일으키게 합니다. Tasty 는 지정 포트 하나를 열고, 웹훅마다 추측할 수 없는 URL 을 발급합니다.

웹훅은 추측하기 어려운 URL과 선택적으로 설정하는 고정 토큰(`--auth-*`)을 사용합니다. **HMAC 서명 검증은 지원하지 않으므로**, 외부 서비스가 보낸 서명 헤더는 검사하지 않습니다. **인증을 설정하지 않으면 URL에 접근할 수 있는 누구나 동작을 요청할 수 있습니다.** 필요한 인증 옵션을 등록할 때 함께 지정하세요.

### 포트 설정

```sh
tasty webhook config                # 현재 포트와 bind 여부
tasty webhook config --port 28429   # 포트 변경 — 재시작 후 반영
```

- 설정 파일은 `~/.tasty/webhooks.toml`. 처음 실행 시 `28429` 가 기본으로 기록됩니다.
- 포트가 비어 있거나 bind 에 실패하면 Tasty 는 다른 포트로 바꾸지 않으며 화면에 경고를 표시합니다. 포트 설정을 확인하고 Tasty를 다시 시작하세요.
- **리스너는 모든 네트워크 인터페이스에 bind 합니다.** 포워딩을 열지 않아도 같은 네트워크(사내망·공용 Wi-Fi 등)에서는 이미 닿습니다 — 공유기 포워딩·방화벽은 *인터넷에서* 들어오게 할 때 여는 것이지, 그 전까지 닫혀 있다는 뜻이 아닙니다. HTTPS 는 앞단 리버스 프록시에 맡깁니다.

### 등록

```sh
# 등록된 핸들러에 연결
tasty webhook register --method POST --handler host/notify --persistent

# 인라인 동작 정의 — 바디의 값을 ${body.x} 로 끌어다 쓰기
tasty webhook register --method POST \
  --sequence '[{"method":"notification.create","params":{"title":"CI","body":"${body.status}"}}]' \
  --ttl-secs 3600 \
  --auth-location header --auth-key X-Token --auth-token s3cret
```

등록하면 `http://127.0.0.1:28429/<16자리 id>` 형태의 URL 이 출력됩니다. 바깥에서 부를 때는 호스트 부분을 실제 주소로 바꿉니다 — 그 자리에서 바로 `curl` 하기 편하라고 넣은 표기일 뿐, 실제 bind 주소가 아닙니다. 반환된 경로를 그대로 사용하세요. `/webhook/` 접두사는 붙이지 않습니다.

| 옵션 | 뜻 |
|---|---|
| `--method <M>` | 허용 HTTP 메서드 (반복 가능, 기본 POST) |
| `--handler <id>` 또는 `--sequence <json>` | 둘 중 하나 필수 |
| `--persistent` | 재시작 후에도 유지 (기본은 재시작 시 사라짐). `--auth-token` 을 함께 걸면 그 토큰도 `~/.tasty/webhooks.toml` 에 평문으로 저장됩니다 |
| `--ttl-secs <초>` / `--count <N>` | 시간 제한 / **접수된** 호출 횟수 제한 (둘 중 하나). 인증에 걸린 요청(`401`)과 허용되지 않은 메서드(`405`)는 횟수를 깎지 않습니다 |
| `--auth-location query\|bearer\|body\|header` + `--auth-token` (+ `--auth-key`) | 선택 인증. 안 걸면 무인증 |

`--ttl-secs`와 `--count`를 모두 생략하면 호출 제한이 없습니다. `--persistent`는 제한 종류와 함께 사용할 수 있으며, 재시작할 때 기한이나 횟수가 남은 등록을 복원합니다.

쓸 수 있는 핸들러 id 는 `tasty hook-handler list` 로 확인합니다. `--sequence` 는 Tasty 내부 동작(알림 보내기, 텍스트 전송 등)을 순서대로 적은 JSON 목록입니다. 올바른 JSON 이 아니면 아무것도 등록하지 않고 원인을 적은 오류로 끝납니다(종료 코드 1).

### 응답과 관리

호출한 서비스에는 상태 코드와 고정 응답 문구가 전달됩니다. 실제 동작의 결과는 응답에 포함되지 않습니다.

**`200` 은 "실행됐다" 가 아니라 "접수했다" 입니다.** 응답은 동작이 시작되기 전에 정해지므로 동작이 통째로 실패해도 호출 측은 `200` 을 받습니다. 여러 단계짜리 `--sequence` 는 한 단계가 실패해도 다음 단계가 계속 실행돼 절반만 적용된 채로 끝날 수 있고, 무엇이 실패했는지는 Tasty 쪽 로그에만 남습니다. 상태 코드로 재시도를 판단하는 CI 라면 `200` 을 "성공" 으로 세지 않도록 주의합니다.

| 코드 | 뜻 |
|---|---|
| 200 `received` | 접수됨 (동작은 백그라운드 실행) |
| 401 `unauthorized` | 인증 실패 (반복하면 429 로 차단됩니다) |
| 404 | 없는 URL |
| 405 | 허용되지 않은 메서드 |
| 410 | 기한·횟수 만료 |
| 413 | 본문이 요청당 크기 상한(기본 1 MiB)을 넘음 |
| 429 | 같은 출처가 10초에 20회 이상 실패(`401`·`404`·`405`·`413`)해 60초 차단 중 |

본문 크기 제한은 인증보다 먼저 적용됩니다. 청크로 전송한 본문에 잘못된 UTF-8 바이트가 있어도 같은 크기 제한이 적용됩니다. 토큰이 없어도 본문이 상한을 넘으면 `413 payload too large`를 받으며 동작은 실행되지 않습니다. 이 상한은 처리할 본문의 크기를 제한합니다. 413 응답 뒤에도 서버가 남은 본문을 받아 버릴 수 있으므로, 응답을 받았다고 연결의 수신 처리가 즉시 끝난 것은 아닙니다. 연결 전체의 전송량·정리 시간·총 메모리·동시 연결 수를 제한하는 기능은 아닙니다.

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
