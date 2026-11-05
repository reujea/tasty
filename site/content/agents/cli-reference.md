# CLI 명령 참조

읽기 위치를 지정하는 `read since-mark`, 서피스 상태, 메시지 큐, 공유 메모리 등 입문 밖의 명령을 목적별로 설명합니다. 전체 인자와 옵션은 `tasty help`에서 확인할 수 있습니다.

## 명령 찾기

```sh
tasty help                    # 목적별로 묶은 명령 목록과 첫 작업 순서
tasty -a                      # 모든 명령 경로와 짧은 설명
tasty help read since-mark    # 한 명령의 긴 설명 (= tasty read since-mark --help)
```

`-h` 는 짧은 설명, `--help` 와 `tasty help <명령>` 은 긴 설명을 보여 줍니다. 대상을 고르는 옵션은 `--surface` · `--pane` · `--workspace` 처럼 대상 종류의 이름을 씁니다. 예전 표기(`--workspace-id`, `--target-surface` 등)도 별칭으로 받으며 도움말에 함께 표시됩니다.

## 여러 에이전트가 같은 터미널을 읽을 때: 위치를 들고 읽기

마크는 서피스마다 하나뿐이라, 여러 에이전트가 같은 터미널을 보면 한 에이전트의 `set mark` 가 나머지가 읽는 자리를 옮깁니다. 서로 간섭하지 않으려면 **다음에 읽을 위치를 각자 저장해** 사용합니다.

```sh
tasty read since-mark --surface 42 --strip-ansi --max-bytes 65536
# 답의 next_cursor 와 stream 을 기억해 두었다가 다음 호출에 넘깁니다
tasty read since-mark --surface 42 --strip-ansi --cursor 81920 --stream 1a2b-7
```

- 답(JSON)에 `next_cursor`(다음에 읽을 위치) · `stream`(그 위치를 받은 터미널의 표지) · `skipped`(그 사이 버퍼에서 밀려나 사라진 바이트 수)가 함께 옵니다. **이어 읽기는 항상 `next_cursor` 로 합니다** — `text` 의 길이로 계산하면 색 코드를 걷어낸 만큼 어긋납니다.
- Tasty는 수신자별 읽기 위치를 저장하지 않습니다. 각자 보낸 위치를 사용하므로 다른 수신자의 위치나 공용 마크를 바꾸지 않습니다.
- `--cursor` 는 `--stream` 과 함께만 씁니다. 서피스가 닫혔다 같은 번호로 다시 열렸거나 터미널이 다시 떴으면 옛 위치는 적용되지 않고 오류로 거절됩니다 — 그때는 위치 없이 한 번 읽어 새로 시작합니다.
- `skipped` 가 0 이 아니면 그만큼이 읽기 전에 사라진 것입니다. 출력 버퍼는 최근 1 MiB 만 남깁니다.
- `--max-bytes` 로 한 번에 받을 양을 줄일 수 있습니다. 나머지는 `next_cursor` 부터 이어 읽습니다. `0` 은 "제한 없음" 이 아니라 1 바이트로 처리됩니다 — 제한 없이 받으려면 `--max-bytes` 를 빼세요.
- 이 인자를 모르는 이전 버전의 Tasty 에는 **요청을 보내지 않습니다.** 그때 명령은 stderr 에 `{"error":{"kind":"unsupported_capability",…,"sent":false}}` 한 줄을 쓰고 종료 코드 1 로 끝납니다. 아무것도 보내지 않았으므로 Tasty 를 업데이트한 뒤 그대로 다시 부르면 됩니다.

<a id="서피스-들여다보기"></a>

## 서피스 상태 확인

```sh
tasty surface cursor-position --surface 42     # 커서가 몇 행 몇 열에 있나
tasty surface foreground-process --surface 42  # 지금 앞에서 도는 프로그램 (is_shell 이 true 면 셸이라 유휴)
tasty surface mouse-tracking --surface 42      # 안의 프로그램이 마우스를 잡았나, 그리고 tasty 가 그걸 존중하나
tasty surface locate --surface 42              # 이 서피스가 속한 페인, 그리고 아직 살아 있는지
tasty surface respawn-terminal --surface 42    # 자리를 유지한 채 셸만 다시 띄우기
tasty surface fire-hook --surface 42 --event process-exit    # 훅을 직접 발화
tasty surface fire-hook --surface 42 --event idle-timeout:300 # 초 단위가 붙는 이벤트도 있음
```

`respawn-terminal` 은 새 셸이 화면에 나타난 뒤에도 이전 셸의 종료를 확인하고 나서 응답합니다.
5초 안에 종료를 확인하지 못하면 새 셸은 그대로 쓸 수 있지만 응답은 복구 필요 오류(`-32068`)입니다.
이 오류를 받은 뒤 같은 멱등 키(IPC 요청의 `idempotency_key`)로 다시 보내면 셸 교체는 이미 끝났으므로 성공 결과를 받습니다.
터미널·workspace·탭을 만드는 명령이 셸을 시작하다 실패해 실제로 무엇이 실행됐는지 알 수 없으면 기다리지 않고 복구 필요 오류(`-32068`)로 응답합니다. 같은 멱등 키(IPC 요청의 `idempotency_key`)로 다시 보내도 다시 실행하지 않고 같은 오류를 돌려줍니다.
설정한 셸 파일에 실행 권한이 없거나 경로에 파일이 없고 대신할 셸도 찾지 못한 경우처럼 셸 프로세스가 만들어지기 전에 실패하면 일반 오류로 응답합니다. 같은 멱등 키로 다시 보내면 같은 실패를 돌려주고, 셸 설정을 고친 뒤 새 멱등 키로 보내면 다시 실행합니다.

## 자식 에이전트에게 권한 주기

```sh
tasty session issue --agent-id build-bot --permission surface.read --permission terminal.write
tasty session list
tasty session revoke --token <토큰>
```

발급된 토큰을 자식이 `TASTY_SESSION_TOKEN`으로 사용하면 Tasty에 보내는 요청은 그 토큰의 권한으로 검사됩니다.
세션 토큰은 요청이 어느 에이전트의 것인지 구분하고 에이전트의 실수를 막는 장치이며, 보안 경계가 아닙니다. 토큰 없이 부른 명령은 사용자의 명령으로 처리됩니다.

플러그인이 더한 명령(`tasty markdown recent` · `tasty codex spawn` 등)도 권한이 필요합니다.
`tasty <명령> …` 이면 `--permission ipc.invoke:<명령>` 을 적어 주세요 — `tasty markdown …` 은
`ipc.invoke:markdown` 이고, 명령 이름에 `-` 가 있으면 `_` 로 바꿉니다. 적지 않은 채 부르면 거부되고 사람에게 권한 승인 요청이 갑니다.
`tasty claude spawn` 으로 띄운 Claude 는 `tasty claude …` · `tasty codex …` 권한을 이미 받아 둡니다.

## 알림 보내기

긴 작업이 끝났을 때 사람에게 알립니다. 알림은 알림 패널에 쌓이고 서피스 테두리·사이드바 배지로 표시됩니다. OS 알림 센터로는 보내지 않습니다.

```sh
tasty notify "빌드 완료" --title "cargo"
tasty list notifications
```

자세한 알림 동작과 자동 실행(훅)은 [훅 · 알림 · 웹훅](hooks-notifications.md).

## 서피스에 메모 남기기 (메타데이터)

서피스마다 키-값을 붙여 둘 수 있습니다. 여러 에이전트가 역할을 표시하거나 상태를 주고받을 때 씁니다.

```sh
tasty surface-meta set --key role --value builder --surface 42
tasty surface-meta get --key role --surface 42
tasty surface-meta list --surface 42
tasty surface-meta unset --key role --surface 42
```

메모는 열려 있는 서피스에만 붙일 수 있습니다. 닫은 서피스 ID를 주면 오류로 거절하고, 서피스를 닫으면 그 서피스의 메모도 함께 지워집니다.

## 서피스끼리 메시지 주고받기 (큐)

터미널 입력을 건드리지 않고 서피스 간에 메시지를 전달하는 큐입니다.

```sh
tasty send queue --to 42 "테스트 끝났음, 결과 확인 바람"
tasty list queue --surface 42            # 대기 건수·미리보기
tasty read queue --surface 42            # 쌓인 메시지를 전부 꺼냄
tasty read queue --surface 42 --peek     # 꺼내지 않고 보기
tasty read queue --surface 42 --clear    # 전부 비움
```

<a id="자식-터미널을-에이전트처럼-굴리기"></a>

## 여러 터미널에 작업 맡기기

워크스페이스에 터미널을 더 열어 명령을 실행하고, 입력을 보내고, 진행 중인 작업을 확인하세요. Claude와 Codex를 포함해 **일반 프로그램도** 같은 방식으로 실행할 수 있습니다. Claude와 Codex의 [전용 명령](claude-codex.md)은 세션 관리 기능도 제공합니다.

```sh
tasty terminal spawn --workspace build --command "cargo watch -x test" --cwd ~/proj --role worker
tasty terminal children                        # 내 밑의 자식 목록
tasty terminal tell "y" --surface 57           # 자식에게 입력 보내기 (줄바꿈 보존, 자동 제출)
tasty terminal broadcast $'git pull\r' --role worker # 역할이 같은 자식 전부에게 (끝에 실제 CR 이 있어야 제출)
tasty terminal kill --child 1                   # 자식을 인덱스로 종료
```

`spawn`은 터미널 준비가 끝나면 응답하며 에이전트 작업 종료는 기다리지 않습니다. 에이전트의 상태 전달과 부모의 수신 준비는 [Claude·Codex 연동](claude-codex.md)을 따릅니다. 일반 프로그램의 생성만으로 에이전트 결과 전달을 보장하지 않습니다.
기다리는 명령을 따로 돌릴 필요가 없습니다. `--role` 로 역할을 지정하면 `broadcast` 로 묶어 보냅니다.

자식 번호 예약을 저장한 뒤 새 터미널을 준비합니다. 예약 저장에 실패하면 터미널을 만들거나 command를 보내지 않고 오류로 답합니다. 손상됐거나 읽을 수 없는 관계 정보는 덮어쓰지 않습니다.

`adopt`는 번호 할당과 관계 등록을 함께 저장합니다. `adopt`·`release`·`set-state`는 저장 결과를 확인한 뒤 응답합니다. 저장 결과를 확정하지 못하면 성공으로 답하거나 같은 예약을 자동 재실행하지 않습니다. 저장 중 대상이 닫히거나 바뀌면 live 적용 실패를 알립니다. `spawn --command`의 입력도 자식 관계가 저장된 뒤에 보냅니다.

`terminal spawn`이 준비 단계에서 실패하면 command를 보내지 않고 이번 호출이 만든 터미널을 정리합니다. 기존 터미널은 유지됩니다.

`--command`를 준 `spawn`은 새 터미널이 화면에 나타난 뒤 command를 입력하고 Enter를 보낸 다음 응답합니다. 그 사이 대상 터미널이 다른 셸로 바뀌거나 원격 attach에 점유되면 Enter를 보내지 않고 복구 필요 오류(`-32068`)로 응답하며, 만든 터미널은 그대로 남습니다. command 입력과 Enter 사이에 그 터미널에 직접 친 키는 command 뒤에 붙어 함께 실행될 수 있습니다(`terminal tell`과 같습니다).

자식의 부모는 보통 명령을 실행한 터미널입니다. Claude·Codex 전용 명령은 부모를 알아서 정하므로 따로 지정할 필요가 없습니다. Tasty 밖의 대상(예: 다른 프로세스에서 진행 중인 에이전트 대화)을 부모로 삼는 스크립트라면 `--surface` 대신 `--parent-external <키>`를 주세요. 키는 직접 정하는 문자열(공백 없이 256바이트 이하)이며 같은 키를 주면 같은 부모로 묶입니다. `spawn`·`children`·`kill`·`respawn`·`broadcast`·`release`가 이 옵션을 받습니다. 윈도우가 여러 개 열려 있으면 `--workspace`도 함께 주세요.

```sh
tasty terminal spawn --workspace build --parent-external review-bot --command "make lint"
tasty terminal children --parent-external review-bot --workspace build
tasty terminal release --parent-external review-bot --workspace build --child 0
```

## 화면 없는 PTY

탭도 화면도 없이 프로그램을 진짜 PTY(가상 터미널) 위에서 돌립니다. 화면에 탭을 만들지 않고
TTY가 필요한 명령을 스크립트에서 실행할 때 씁니다. `spawn` 이 돌려주는 id 로 입력을 넣고 화면을 읽습니다.

```sh
tasty pty spawn --cwd ~/proj -- python3         # 명령을 PTY 로 띄우고 id 를 받음
tasty pty write --id 2147483648 $'print(1+1)\n' # 표준 입력으로 보내기 (줄바꿈이 곧 제출)
tasty pty read --id 2147483648 --lines 20      # 지금 화면의 마지막 20줄
tasty pty state --id 2147483648                 # 종료 여부를 바로 확인 (기다리지 않음)
tasty pty list                                  # 떠 있는 PTY 목록
tasty pty kill --id 2147483648                   # 종료
```

실행 상태 확인과 실행 중인 작업을 탭으로 옮기는 방법은 [백그라운드 작업을 탭으로 가져오기](background-tasks.md)에 있습니다.

## 에이전트가 함께 쓰는 메모리

같은 Tasty 안의 여러 에이전트가 값을 주고받는 키-값 저장소입니다. 범위(전역 · 서피스 · 워크스페이스 ·
윈도우 · 계정)를 골라 저장하고, 시간이 지나면 사라지게 하거나(TTL) 겹쳐쓰기를 막을(CAS) 수 있습니다.

```sh
tasty memory put --workspace 7 --key build.status --value running --ttl 600
tasty memory get --workspace 7 --key build.status
tasty memory list --workspace 7 --prefix build.
tasty memory delete --workspace 7 --key build.status
```

`--global` · `--surface 3` · `--window 42` · `--account me` 로 범위를 바꿉니다. 값이 JSON 이면
JSON 으로, 아니면 문자열로 저장합니다.

`--ttl` 이 지난 키는 없는 키와 같습니다. 다른 에이전트도 그 키에 새로 저장할 수 있고 version 은 1 부터
다시 시작합니다. 만료된 값은 용량 한도에도 들어가지 않으며, Tasty 가 10분마다 디스크에서 지웁니다.

`--surface` 범위에 저장하는 것은 열려 있는 서피스에만 됩니다. 닫은 서피스 ID를 주면 오류로 거절합니다.
`tasty memory import` 도 같습니다. 파일에 닫은 서피스 범위의 항목이 하나라도 있으면 아무것도 넣지 않고 오류로 거절합니다.
서피스를 닫으면 그 범위의 값도 함께 지워집니다. 예전에 닫은 서피스 범위에 남아 있는 값은
`tasty memory list --surface <ID>` 로 확인하고 `tasty memory delete` 로 지울 수 있습니다.

Tasty 가 시작할 때 메모리 파일(`~/.tasty/memory.db`)을 열지 못하면 — 파일이 깨졌거나 권한이
없는 경우 — Tasty 는 멈추지 않고 **임시 메모리**로 계속 동작합니다. 그동안 저장한 값은 Tasty 를
다시 시작하면 사라집니다. 메모리뿐 아니라 같은 파일에 저장하는 에이전트 작업 · 세마포어 같은
조율 장치, 승인, surface 메타데이터, 텔레메트리, 세션도 마찬가지입니다. 이 상태에서는 그런 저장
결과에 `"durable": false` 가 함께 나오고,
`tasty list pressure` 의 `db_pragmas.memory_db` 가 `degraded: true` 와 원인(`init_failure`)을
보여 줍니다. 알림 목록에도 원인과 이 영향을 적은 경고 알림이 하나 남습니다.

<a id="출력에서-신호-뽑기-관찰자"></a>

## 출력에서 필요한 정보 모으기 (관찰자)

터미널 출력에서 경로 · URL · 종료 코드 · 프롬프트 경계 같은 항목을 찾아 기록합니다. 사람이 화면을 계속 보지 않아도 스크립트에서 이 정보를 사용할 수 있습니다.

```sh
tasty output observe start --surface 42 --parsers exit_code,url --sink file
tasty output observe list                        # 지금 도는 관찰자 목록
tasty output observe info --observer 1           # 하나의 상태·수집 수
tasty output observe stop --observer 1
```

`--sink memory` 는 메모리 링버퍼에, `--sink file` 은 파일에 모읍니다. `--parsers` 를 비우면 기본
파서(경로 · URL · 프롬프트 경계 · 종료 코드)가 다 켜집니다.

<a id="에이전트-활동-계측"></a>

## 에이전트 사용량 확인

여러 에이전트가 자기 활동을 숫자로 기록하고(토큰 수 · 호출 수 등), 그것을 합계 · 시계열 · 상위
순위로 들여다봅니다. 여러 에이전트의 사용량과 활동을 함께 확인할 때 유용합니다.

```sh
tasty telemetry record --metric tokens --value 1200 --tags '{"model":"opus"}'
tasty telemetry summary --metric tokens           # 합계·건수
tasty telemetry top --by agent --metric tokens    # 에이전트별 상위
tasty telemetry timeseries --metric tokens --window 1h
```

`record`의 `--agent`를 생략하면 Tasty가 확인한 호출자 ID로 기록합니다. 세션 토큰을 쓰는 에이전트는 그 세션의 ID를 사용합니다. 토큰 없는 로컬 호출은 그 셸의 `TASTY_AGENT_ID` 로 기록하고, 값이 없거나 영문자·숫자·`_`·`-` 64자 이내가 아니면 `_host`로 기록합니다. 이 값은 호출자가 스스로 밝힌 이름일 뿐이라 권한에는 영향을 주지 않습니다. 여러 값을 순서까지 지켜
한 번에 넣으려면 `tasty telemetry record-batch` 를 씁니다.

## 그 밖의 조회·설정

에이전트가 가끔 쓰는 것들입니다. 전체 목록은 `tasty <명령> --help` 로 봅니다.

```sh
tasty list pressure                    # 요청에 답하는 동안 시간이 어디서 갔나 (읽는 법: 응답 지연과 연결 진단)
tasty list theme                       # 지금 적용된 테마 스냅샷(색·글자 크기·UI 배율)
tasty list recent --kind markdown      # 그 종류로 최근 연 파일 목록
tasty set cwd --surface 42 --path /tmp # 원격 서피스가 보고하는 작업 디렉터리 변경
tasty set url --surface 42 --url URL   # 웹뷰 서피스의 주소 변경(--label 이름: 페이지가 가려졌을 때 보일 이름)
tasty surface html-script --surface 42 # HTML 서피스의 스크립트·원격 콘텐츠 감지·허용·안내 상태(조회만)
tasty surface html-script-allow --surface 42 --url <url> --fingerprint <hex>  # 조회한 문서의 스크립트를 허용하고 다시 불러옴. 문서가 바뀌었으면 거절. 포커스는 그대로
tasty surface html-remote-allow --surface 42 --url <url> --fingerprint <hex>  # 조회한 문서에만 원격 콘텐츠(네트워크의 스타일시트·이미지·글꼴·프레임·스크립트)를 허용하고 다시 불러옴. 스크립트 허용과 별개. 포커스는 그대로
tasty surface html-nav --surface 42    # HTML 서피스가 보여 주는 문서 주소와 뒤로·앞으로 이동 가능 여부(조회만)
tasty surface html-back --surface 42   # HTML 서피스를 뒤로 이동(html-forward: 앞으로, html-reload: 새로고침). 포커스는 그대로이고, 결과는 html-nav로 확인
tasty hook-handler dispatch --id user/my-handler  # 훅 핸들러를 직접 실행하고 최종 결과까지 기다리기
tasty hook-handler result --run 7       # dispatch가 알려 준 run 번호로 결과 다시 보기
tasty file-handler dispatch 파일경로     # 탐색기에서 더블클릭한 것과 같은 경로로 파일 열기(결과까지 기다림)
tasty file-handler result --run 7       # dispatch가 알려 준 run 번호로 결과 다시 보기
tasty file-handler reload               # 파일 핸들러 설정 파일을 다시 읽기
tasty file-handler detectors            # 파일 형식 판별 규칙이 지금 어떻게 합쳐져 있는지 보기
```

`set cwd` 와 `set url` 은 대상이 각각 원격 서피스·웹뷰 서피스일 때만 동작합니다. 일반 터미널 서피스에 쓰면 지원하지 않는 대상이라는 오류를 반환합니다.

`hook-handler dispatch` 는 두 줄을 출력합니다. 첫 줄은 요청을 받아들였다는 응답이고 `run` 번호가 들어 있습니다. 둘째 줄은 기다린 뒤의 최종 결과입니다. 결과의 `state` 가 `succeeded` 이면 종료 코드 0으로 끝나고, `partial`·`failed`·`unknown` 이거나 기다리는 동안 끝나지 않았으면(`running`) 종료 코드 1로 끝납니다. Tasty가 결과 조회에 답하지 않아도 기다리는 상한에서 멈추고, 이때 둘째 줄에는 `"timed_out": true` 가 붙습니다. 실패한 단계와 이유는 `result` 와 `steps` 에 나옵니다. 받아들이기 전에 거절된 요청(없는 핸들러, 꺼진 핸들러, 대기열이 가득 참 등)은 첫 줄 없이 오류와 종료 코드 1로 끝납니다. `--no-wait` 를 붙이면 첫 줄만 출력하고 종료 코드 0으로 끝나므로, 나중에 `hook-handler result --run <번호>` 로 결과를 봅니다. 결과를 기다릴 때는 요청을 여러 번 보내므로 루트 옵션 `--response-timeout-ms` 를 받지 않습니다. 붙이면 아무것도 보내지 않고 사용 오류(종료 코드 2)로 끝납니다. `--no-wait` 를 붙이면 요청이 하나뿐이라 이 옵션이 그 요청에 적용됩니다. 셸에 설정된 세션 토큰(`TASTY_SESSION_TOKEN`)과 에이전트 ID(`TASTY_AGENT_ID`)는 결과를 묻는 요청에도 그대로 실립니다.

`file-handler dispatch` 는 파일 경로만 받습니다. `https://…` 같은 웹 주소를 넘기면 오류를 반환합니다. 이 명령으로 연 파일은 새 탭으로 뒤에 붙기만 하고, 사용자가 보고 있던 탭은 바뀌지 않습니다. 이 형식을 열 핸들러가 없으면 사용자 화면에 **다음으로 열기…** <!-- en: Open file with… --> 를 띄우지 않고 파일을 열지 않습니다. 명령은 `hook-handler dispatch` 처럼 두 줄을 출력합니다. 첫 줄은 요청을 받아들였다는 응답(`run` 번호 포함)이고, 둘째 줄은 최종 결과입니다. 열었으면 `state` 가 `succeeded` 이고 새 탭은 `surface_id` 로 알려 주며 종료 코드 0으로 끝납니다. 열 핸들러가 없거나 핸들러가 실패하면 `state` 가 `failed` 이고 `result` 에 이유가 나오며 종료 코드 1로 끝납니다. 원격 워크스페이스의 탭을 기준으로 열면 원격 Tasty가 실제로 열었는지까지 기다립니다. 원격이 열지 못하면 `failed` 와 원격이 알린 이유가 나옵니다. 원격의 답을 받기 전에 연결이 끊기면 열렸는지 알 수 없으므로 `state` 가 `unknown` 이고 종료 코드 1입니다. 결과를 30초 안에 받지 못해도 종료 코드 1입니다. `--no-wait` 를 붙이면 첫 줄만 출력하고, 나중에 `file-handler result --run <번호>` 로 결과를 봅니다. 결과를 기다릴 때는 루트 옵션 `--response-timeout-ms` 를 받지 않습니다(종료 코드 2). GUI 없이 서버에서 실행하는 headless 빌드는 파일을 열 수 없으므로, 이 명령에 요청을 받아들였다고 답하지 않고 이 빌드에서는 지원하지 않는다는 오류를 반환합니다.

`file-handler reload` 의 응답에는 `rejected` 목록이 있습니다. 설정 파일에서 지금 적용되지 않은 항목의 `id` 와 사유(`reason`)가 들어 있고, 모두 적용됐으면 빈 목록입니다. 사유는 셋입니다.

- `missing_owner_prefix` — `id` 앞에 `user/` 같은 소유자 부분을 붙이지 않았습니다(`user/이름` 으로 적어야 합니다). 이 항목은 버려집니다.
- `missing_detector_or_action` — 직접 만든 `user/…` 항목에 어떤 파일을 다룰지(`detector`)나 무엇을 할지(`action`)가 비어 있습니다. 이 항목은 버려집니다.
- `target_not_contributed` — 기본 핸들러나 플러그인 핸들러를 고치는 항목인데 그 대상이 지금 없습니다. 플러그인이 꺼져 있거나 `id` 가 틀린 경우입니다. 항목은 남아 있어서, 플러그인이 켜지면 그대로 적용됩니다. 켠 뒤에도 남아 있다면 `id` 를 확인하세요.

`file-handler detectors` 는 파일 형식 판별 규칙(detector)마다 지금 적용되는 값(표시 이름·아이콘·켜짐 여부·규칙)과, 그 값을 만든 출처별 원본(`contributions`)을 함께 보여 줍니다. 출처는 `host`(기본 제공) · `plugin:<id>` · `user`(설정 파일)입니다. 설정 파일에 적은 값이 실제로 적용됐는지 확인할 때 씁니다 — `contributions` 는 설치된 순서로 나열되므로, 어느 값이 적용됐는지는 위쪽의 적용 값을 보세요.

`list info`의 워크스페이스 수와 활성 위치는 조회한 윈도우의 값이며, 함께 반환된 워크스페이스 ID로 소속을 확인할 수 있습니다. 전체 워크스페이스는 `list workspaces`, 각 윈도우의 상태는 `list windows`로 확인하세요.

## 함께 살펴보기

- [tasty CLI로 터미널 조작하기](cli.md) — 처음 쓰는 순서와 기본 패턴.
- [CLI 응답 지연과 연결 진단](../help/cli-diagnostics.md) — `tasty list pressure` 읽는 법과 오류 코드별 조치.
- [백그라운드 작업을 탭으로 가져오기](background-tasks.md) · [Claude·Codex와 함께 쓰기](claude-codex.md) · [작업 순서 관리](tasks.md) · [훅 · 알림 · 웹훅](hooks-notifications.md)
