# tasty CLI 로 터미널 조작하기

`tasty` CLI로 터미널을 만들고, 명령을 보내고, 결과를 확인해 보세요. 실행 중인 Tasty를 스크립트에서 조작하거나 AI 에이전트가 작업할 터미널을 직접 준비하게 할 수 있습니다.

처음에는 목록 조회부터 시작해 명령 전송과 출력 읽기를 차례로 따라가세요. Claude Code와 Codex 같은 에이전트도 같은 명령을 사용합니다.

## 준비

- Tasty 가 실행 중이어야 합니다. CLI 는 `~/.tasty/tasty.port` 에 적힌 포트로 실행 중인 인스턴스에 접속합니다.
- Tasty 가 띄운 터미널 안에서는 `tasty` 가 이미 PATH 에 있습니다. 밖(다른 터미널 앱)에서 쓰려면 Tasty 실행 파일이 있는 경로를 PATH 에 넣습니다. 설치 방식별 경로는 [설치](../getting-started/install.md#설치-위치) 에 있습니다.
- Tasty 가 띄운 셸에는 `TASTY_SURFACE_ID` 환경변수가 들어 있습니다. `--surface` 를 생략한 명령은 이 값을 쓰므로, 자기 터미널을 조작할 때는 ID 를 적지 않아도 됩니다.

```sh
echo $TASTY_SURFACE_ID     # 예: 42
tasty list info            # 버전·조회한 윈도우의 워크스페이스 수 — 접속 확인용
```

## 용어와 ID

Tasty 의 화면은 **워크스페이스 > 페인 > 탭 > 서피스** 순으로 겹쳐 있습니다. 서피스(surface)가 터미널 하나입니다. CLI 의 모든 대상은 이 ID 로 직접 지정합니다 — 어느 윈도우가 포커스돼 있든 결과가 같습니다.

```sh
tasty list tree            # 전체 계층을 트리로
tasty list workspaces      # 워크스페이스 목록
tasty list surfaces        # 서피스(터미널) 목록 — 전 워크스페이스
tasty list panes           # 페인 목록
tasty list tabs --pane 3   # 특정 페인의 탭
tasty list surface-kinds   # 이 인스턴스가 실제로 등록한 서피스 종류
```

`list tree` 는 분할 구조까지 보여줍니다. 포커스된 서피스에는 `*focus` 가 붙습니다.

```
└─ vertical (L|R) 60:40
   ├─ surface:396 (terminal)
   └─ horizontal (T|B) 50:50
      ├─ surface:417 (terminal) *focus
      └─ surface:418 (markdown)
```

`list surface-kinds`는 현재 `--type <종류>`로 만들 수 있는 서피스 종류를 보여줍니다. 플러그인이 실행되지 않았거나 현재 빌드가 지원하지 않는 종류는 목록에 나타나지 않습니다. 플러그인을 끄거나 제거하면 그 플러그인의 종류는 곧바로 빠지고, 그 종류로 만들려는 요청은 제공하던 플러그인이 꺼져 있거나 아직 다시 연결되지 않았다는 오류로 끝납니다. 각 행에서 렌더링 방식과 기능을 제공하는 플러그인 또는 내장 기능을 확인할 수 있습니다.

`list workspaces` 의 행은 `이름 (id:N) (페인 수)` 형식입니다. 활성 워크스페이스에는 `*`, 원격 mirror 에는 `[mirror]` 가 붙습니다 ([원격 작업 이어가기](../remote/attach.md)).

## 기본 패턴: 마크 → 보내기 → 마크 이후 읽기

명령 하나의 결과만 깔끔히 꺼내는 표준 절차입니다.

1. `tasty set mark` — 지금 출력 위치에 표시를 남깁니다.
2. `tasty send text "명령\r"` — 텍스트를 보냅니다. `\r` 이 Enter 입니다.
3. 잠시 기다린 뒤 `tasty read since-mark --strip-ansi` — 표시 이후에 나온 출력만 읽습니다.

```sh
tasty set mark --surface 42
tasty send text "cargo test 2>&1 | tail -20\r" --surface 42
sleep 5
tasty read since-mark --surface 42 --strip-ansi
```

- `send text` 는 `\r` `\n` `\t` `\\` `\0` 이스케이프를 해석합니다. 셸 따옴표 안에 그대로 적으면 됩니다.
- `--strip-ansi` 를 붙이면 색상 등 제어 시퀀스를 걷어낸 순수 텍스트가 옵니다. 파싱할 때는 항상 붙입니다.
- 마크는 `set mark` 를 다시 부를 때까지 유지됩니다. `read since-mark` 는 마크를 옮기지 않으므로 여러 번 읽어도 같은 구간이 나옵니다.

명령이 끝났는지 모를 때는 화면을 읽어 프롬프트가 돌아왔는지 봅니다.

```sh
tasty read screen --surface 42 --lines 5     # 화면 하단 5줄 (부족하면 스크롤백까지)
tasty is-typing --surface 42                  # 최근 5초 내 사람이 키를 눌렀는지
```

### 여러 에이전트가 같은 터미널을 읽을 때: 위치를 들고 읽기

마크는 서피스마다 하나뿐이라, 여러 에이전트가 같은 터미널을 보면 한 에이전트의 `set mark` 가 나머지가 읽는 자리를 옮깁니다. 서로 간섭하지 않으려면 **읽을 위치를 각자 들고** 읽습니다.

```sh
tasty read since-mark --surface 42 --strip-ansi --max-bytes 65536
# 답의 next_cursor 와 stream 을 기억해 두었다가 다음 호출에 넘깁니다
tasty read since-mark --surface 42 --strip-ansi --cursor 81920 --stream 1a2b-7
```

- 답(JSON)에 `next_cursor`(다음에 읽을 위치) · `stream`(그 위치를 받은 터미널의 표지) · `skipped`(그 사이 버퍼에서 밀려나 사라진 바이트 수)가 함께 옵니다. **이어 읽기는 항상 `next_cursor` 로 합니다** — `text` 의 길이로 계산하면 색 코드를 걷어낸 만큼 어긋납니다.
- 위치를 든 읽기에 대해 Tasty 는 아무것도 기억하지 않으므로, 읽는 쪽이 몇이든 서로를 밀지 않습니다. 마크도 움직이지 않습니다.
- `--cursor` 는 `--stream` 과 함께만 씁니다. 서피스가 닫혔다 같은 번호로 다시 열렸거나 터미널이 다시 떴으면 옛 위치는 적용되지 않고 오류로 거절됩니다 — 그때는 위치 없이 한 번 읽어 새로 시작합니다.
- `skipped` 가 0 이 아니면 그만큼이 읽기 전에 사라진 것입니다. 출력 버퍼는 최근 1 MiB 만 남깁니다.
- `--max-bytes` 로 한 번에 받을 양을 줄일 수 있습니다. 나머지는 `next_cursor` 부터 이어 읽습니다. `0` 은 "제한 없음" 이 아니라 1 바이트로 처리됩니다 — 제한 없이 받으려면 `--max-bytes` 를 빼세요.
- 이 인자를 모르는 이전 버전의 Tasty 에는 **요청을 보내지 않습니다.** 그때 명령은 stderr 에 `{"error":{"kind":"unsupported_capability",…,"sent":false}}` 한 줄을 쓰고 종료 코드 1 로 끝납니다. 아무것도 보내지 않았으므로 Tasty 를 업데이트한 뒤 그대로 다시 부르면 됩니다.

### 응답 대기에 상한 걸기

```sh
tasty --response-timeout-ms 5000 read screen --surface 42
```

`--response-timeout-ms` 는 명령 **앞**에 적습니다. 그 시간 안에 답이 안 오면 명령이 `Error (-32061): …` 로 끝납니다. 이 오류는 **결과를 모른다**는 뜻입니다 — 요청은 Tasty 안에서 계속 실행될 수 있으므로, 무언가를 바꾸는 명령이었다면 다시 보내기 전에 상태를 먼저 확인합니다. 요청이 Tasty 안에서 차례를 기다리는 동안 시간이 다 됐다면 대신 `Error (-32067): …` 로 끝납니다. 이때는 **아무것도 실행되지 않았으므로** 그대로 다시 보내면 됩니다. 플래그를 안 주거나 `0` 을 주면 상한 없이 기다립니다. 요청 하나로 끝나는 명령에만 쓸 수 있고, `events follow` 처럼 반복해서 묻거나 원격 attach 처럼 연결을 여는 명령은 이 플래그를 받으면 아무것도 보내지 않고 종료 코드 2 로 거절합니다. 상한을 이해하지 못하는 이전 버전의 Tasty 에는 위와 같은 `sent:false` 거절로 요청을 보내지 않습니다. 그 확인도 같은 시간 안에 들어갑니다 — Tasty 가 멈춰 있어 확인조차 제시간에 안 끝나면 요청을 보내지 않고 `Error (-32067): …` 로 끝나므로, 그때도 그대로 다시 보내면 됩니다.

`read screen` 은 기본적으로 흐리게 표시되는 자동완성 제안(예: Claude Code 의 회색 제안 텍스트)을 제외합니다. 포함하려면 `--show-dim`.

`--lines N`보다 적은 줄이 반환되면 응답의 `scrollback_len`을 확인하세요. 값이 `0`이면 더 읽을 스크롤백이 없다는 뜻입니다. 전체 화면 앱(TUI)을 처음 실행했을 때 이런 경우가 생길 수 있습니다. 스크롤백이 남아 있는데도 요청한 줄 수보다 적게 반환된다면 출력 조회를 확인해 보세요. `alt_screen`은 현재 전체 화면 앱을 사용 중인지 알려줍니다.

## 키 보내기

Enter 외의 키는 `send key` 로 보냅니다.

```sh
tasty send key enter --surface 42
tasty send key ctrl+c --surface 42
tasty send key escape --surface 42
tasty send key up --surface 42
```

키 이름: `enter` `tab` `escape`(또는 `esc`) `backspace` `delete` `insert` `up` `down` `left` `right` `home` `end` `pageup` `pagedown` `f1`~`f12`. 조합은 `ctrl+c` `alt+x` `ctrl+shift+c` 처럼 `+` 로 잇습니다.

## 셸 통합이 있으면: 명령 단위로 읽기

bash / zsh 는 Tasty 가 셸 통합을 자동으로 넣어 주므로, 실행한 명령과 종료 코드를 명령 단위로 조회할 수 있습니다.

```sh
tasty read commands --surface 42       # 기록된 명령 목록
tasty read last-command --surface 42   # 마지막 명령 (명령 문자열·종료 코드)
tasty read command-at --surface 42 --index -1   # 뒤에서 첫 번째 (0부터, 음수는 끝에서)
```

fish 등 다른 셸은 직접 셸 통합을 설치하지 않으면 빈 목록이 나옵니다.

## 새 터미널 만들기·닫기

```sh
tasty new workspace --name build --cwd ~/proj          # 새 워크스페이스
tasty new workspace --surface 42                       # 서피스 42 가 있는 윈도우에 새 워크스페이스 (작업 디렉토리도 서피스 42 에서)
tasty new window                                        # 새 윈도우 (응답에 window_id)
tasty split --level surface --target-surface this --direction vertical   # 내 서피스를 좌우 분할
tasty split --level pane --target-pane 3 --direction horizontal          # 페인 분할
tasty new tab --pane 3 --cwd ~/proj                     # 페인에 새 탭
tasty close surface --surface 99                        # 서피스 닫기
tasty close tab --tab 12
tasty close workspace --id 3                            # 워크스페이스를 통째로 (안의 탭·서피스까지)
tasty close window --id 1                               # 윈도우 닫기
tasty close self                                        # 지금 이 서피스 닫기
```

`--target-surface this` 는 자기 자신(`TASTY_SURFACE_ID`)입니다. `--type markdown --file README.md` 처럼 터미널이 아닌 표면도 만들 수 있습니다 ([파일 열기](../using/files.md)).

`tasty new tab` 으로 연 탭은 종류와 상관없이 사용자가 보던 탭을 바꾸지 않습니다. 새 탭은 페인의 맨 뒤에 붙고, 사용자가 고를 때까지 뒤에 있습니다. 응답의 `active_tab` 은 새 탭이 아니라 그 페인에서 지금 선택된 탭이므로, 새 탭을 다룰 때는 응답의 `surface_id` 를 씁니다. 단축키나 메뉴로 사용자가 직접 연 탭은 바로 선택됩니다.

`tasty new window` 로 연 윈도우는 사용자가 보던 윈도우의 포커스를 가져가지 않습니다. 그래서 뒤이어 대상을 안 적은 명령(`tasty new workspace` 등)은 새 윈도우가 아니라 사용자가 보던 윈도우에 만들어집니다. 새 윈도우에 워크스페이스를 만들려면 그 윈도우의 서피스 ID 를 `tasty new workspace --surface <ID>` 로 적습니다 — `tasty list windows` 가 윈도우마다 `workspace_ids` 를, `tasty list surfaces` 가 서피스마다 `workspace_id` 를 보여 줍니다. 적은 서피스가 어느 윈도우에도 없으면 명령은 다른 윈도우로 새지 않고 오류로 끝납니다. `--cwd` 를 생략하면 새 워크스페이스의 작업 디렉토리는 적은 서피스의 것을 이어받습니다(그 윈도우에서 사용자가 보고 있는 서피스가 아닙니다). 새 윈도우는 사용자가 보던 윈도우 뒤에 생기고 키 입력도 가져가지 않습니다(macOS · Windows). Linux 의 X11 에서는 윈도우 관리자에게 그렇게 해 달라고 요청하지만 따를지는 윈도우 관리자가 정하고, Wayland 에서는 컴포지터가 정합니다. 어느 쪽이든 대상 없는 명령이 가는 윈도우는 바뀌지 않습니다.

워크스페이스와 윈도우는 마지막 하나를 닫지 못합니다. 워크스페이스를 닫으면 윈도우까지 사라지는 것이 아니라
거절되므로, 윈도우를 없앨 생각이면 `tasty close window` 를 따로 씁니다(윈도우 없이 `tasty --headless` 로 실행한
인스턴스에는 닫을 윈도우가 없어 마지막 워크스페이스는 그냥 닫을 수 없습니다). 자기 터미널이 들어 있는 대상은
닫히지 않습니다 — 그때는 `tasty close self` 입니다. 원격에 접속해 미러로 띄운 워크스페이스도 닫지 못합니다 —
그건 접속을 끊어서 정리합니다. 반대로 **누군가 원격에서 접속해 지금 쓰고 있는 터미널**이 들어 있는
워크스페이스도 닫지 못합니다 — 그 사람이 접속을 놓은 뒤에야 닫힙니다. 보고 있지 않은 워크스페이스를 닫아도 화면에 떠 있는 워크스페이스는
그대로 남습니다.

**워크스페이스 닫기는 되돌릴 수 없습니다.** 안에서 돌던 터미널이 전부 종료되고, "닫은 항목" 으로
되살릴 수 없으며, 스크롤백도 남지 않습니다. 닫은 항목 복원은 사용자가 화면에서 직접 닫은 항목에만 적용됩니다.
닫으려는 대상이 맞는지 `tasty list workspaces`로 먼저 확인하세요.

<a id="서피스-들여다보기"></a>

## 서피스 상태 확인

```sh
tasty surface cursor-position --surface 42     # 커서가 몇 행 몇 열에 있나
tasty surface foreground-process --surface 42  # 지금 앞에서 도는 프로그램 (셸이면 유휴)
tasty surface mouse-tracking --surface 42      # 안의 프로그램이 마우스를 잡았나, 그리고 tasty 가 그걸 존중하나
tasty surface locate --surface 42              # 이 서피스가 속한 페인, 그리고 아직 살아 있는지
tasty surface respawn-terminal --surface 42    # 자리를 유지한 채 셸만 다시 띄우기
tasty surface fire-hook --surface 42 --event process-exit    # 훅을 직접 발화
tasty surface fire-hook --surface 42 --event idle-timeout:300 # 초 단위가 붙는 이벤트도 있음
```

## 사람이 타이핑 중이면 보내지 않기

```sh
tasty send text "make test\r" --surface 42 --wait-idle
```

`--wait-idle` 은 판정과 전송을 한 번에 합니다. `tasty is-typing` 으로 먼저 확인하고 보내면 그 사이에
사람이 타이핑을 시작할 수 있는데, 이 플래그는 그 틈을 없앱니다. 타이핑 중이면 보내지 않고
`"sent": false` 와 이유를 돌려줍니다.

## 자식 에이전트에게 권한 주기

```sh
tasty session issue --agent-id build-bot --permission surface.read --permission terminal.write
tasty session list
tasty session revoke --token <토큰>
```

발급한 토큰을 자식이 `TASTY_SESSION_TOKEN` 으로 들고 있으면 거기 적힌 권한만 쓸 수 있습니다.

플러그인이 더한 명령(`tasty markdown recent` · `tasty codex spawn` 등)도 권한이 필요합니다.
`tasty <명령> …` 이면 `--permission ipc.invoke:<명령>` 을 적어 주세요 — `tasty markdown …` 은
`ipc.invoke:markdown` 이고, 명령 이름에 `-` 가 있으면 `_` 로 바꿉니다. 적지 않은 채 부르면 거부되고 사람에게 권한 승인 요청이 갑니다.
`tasty claude spawn` 으로 띄운 Claude 는 `tasty claude …` · `tasty codex …` 권한을 이미 받아 둡니다.

## 알림 보내기

긴 작업이 끝났을 때 사람에게 알립니다. 알림 패널과 OS 알림으로 나갑니다.

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

## 서피스끼리 메시지 주고받기 (큐)

터미널 입력을 건드리지 않고 서피스 간에 메시지를 전달하는 큐입니다.

```sh
tasty send queue --to 42 "테스트 끝났음, 결과 확인 바람"
tasty list queue --surface 42            # 대기 건수·미리보기
tasty read queue --surface 42            # 가장 오래된 메시지 하나 꺼냄
tasty read queue --surface 42 --peek     # 꺼내지 않고 보기
tasty read queue --surface 42 --clear    # 전부 비움
```

<a id="자식-터미널을-에이전트처럼-굴리기"></a>

## 여러 터미널에 작업 맡기기

워크스페이스에 터미널을 더 열어 명령을 실행하고, 입력을 보내고, 진행 중인 작업을 확인하세요. Claude와 Codex를 포함해 **일반 프로그램도** 같은 방식으로 실행할 수 있습니다. Claude와 Codex의 [전용 명령](claude-codex.md)은 세션 관리 기능도 제공합니다.

```sh
tasty terminal spawn --workspace build --command "cargo watch -x test\r" --cwd ~/proj --role worker
tasty terminal children                        # 내 밑의 자식 목록
tasty terminal tell "y\r" --surface 57         # 자식에게 입력 보내기 (줄바꿈 보존, 자동 제출)
tasty terminal broadcast "git pull\r" --role worker   # 역할이 같은 자식 전부에게
tasty terminal kill --child 1                   # 자식을 인덱스로 종료
```

`spawn`은 새 터미널을 만들고 즉시 반환합니다. 에이전트의 상태 전달과 부모의 수신 준비는 [Claude·Codex 연동](claude-codex.md)을 따릅니다. 일반 프로그램의 생성만으로 에이전트 결과 전달을 보장하지 않습니다.
기다리는 명령을 따로 돌릴 필요가 없습니다. `--role` 로 역할을 지정하면 `broadcast` 로 묶어 보냅니다.

`terminal spawn`이 준비 단계에서 실패하면 command를 보내지 않고 이번 호출이 만든 터미널을 정리합니다. 기존 터미널은 유지됩니다.

## 화면 없는 PTY

탭도 화면도 없이 프로그램을 진짜 PTY(가상 터미널) 위에서 돌립니다. 화면에 자리를 차지하지 않고
TTY 가 필요한 명령을 스크립트로 굴릴 때 씁니다. `spawn` 이 돌려주는 id 로 입력을 넣고 화면을 읽습니다.

```sh
tasty pty spawn --cwd ~/proj -- python3         # 명령을 PTY 로 띄우고 id 를 받음
tasty pty write --id 3 $'print(1+1)\n'           # 표준 입력으로 보내기 (줄바꿈이 곧 제출)
tasty pty read --id 3 --lines 20                # 지금 화면의 마지막 20줄
tasty pty list                                  # 떠 있는 PTY 목록
tasty pty kill --id 3                            # 종료
```

## 에이전트가 함께 쓰는 메모리

같은 Tasty 안의 여러 에이전트가 값을 주고받는 키-값 저장소입니다. 범위(전역 · 서피스 · 워크스페이스 ·
윈도우 · 계정)를 골라 저장하고, 시간이 지나면 사라지게 하거나(TTL) 겹쳐쓰기를 막을(CAS) 수 있습니다.

```sh
tasty memory put --workspace 7 --key build/status --value running --ttl 600
tasty memory get --workspace 7 --key build/status
tasty memory list --workspace 7 --prefix build/
tasty memory delete --workspace 7 --key build/status
```

`--global` · `--surface 3` · `--window 42` · `--account me` 로 범위를 바꿉니다. 값이 JSON 이면
JSON 으로, 아니면 문자열로 저장합니다.

Tasty 가 시작할 때 메모리 파일(`~/.tasty/memory.db`)을 열지 못하면 — 파일이 깨졌거나 권한이
없는 경우 — Tasty 는 멈추지 않고 **임시 메모리**로 계속 동작합니다. 그동안 저장한 값은 Tasty 를
다시 시작하면 사라집니다. 메모리뿐 아니라 같은 파일에 저장하는 에이전트 작업 · 세마포어 같은
조율 장치, 승인, surface 메타데이터, 텔레메트리, 세션도 마찬가지입니다. 이 상태에서는 그런 저장
결과에 `"durable": false` 가 함께 나오고,
`tasty list pressure` 의 `db_pragmas.memory_db` 가 `degraded: true` 와 원인(`init_failure`)을
보여 줍니다. 화면에는 따로 안내가 뜨지 않습니다.

<a id="출력에서-신호-뽑기-관찰자"></a>

## 출력에서 필요한 정보 모으기 (관찰자)

터미널 출력이 흘러가는 것을 지켜보다가 경로 · URL · 종료 코드 · 프롬프트 경계 같은 **구조화된 신호**만
골라 모읍니다. 사람이 화면을 지켜보지 않아도 스크립트가 그 신호에 반응하게 만들 때 씁니다.

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

`record` 는 부르는 쪽을 에이전트로 자동 귀속합니다(`TASTY_AGENT_ID`). 여러 값을 순서까지 지켜
한 번에 넣으려면 `tasty telemetry record-batch` 를 씁니다.

## 그 밖의 조회·설정

에이전트가 가끔 쓰는 것들입니다. 전체 목록은 `tasty <명령> --help` 로 봅니다.

```sh
tasty list pressure                    # 요청에 답하는 동안 시간이 어디서 갔나
tasty list theme                       # 지금 적용된 테마 스냅샷(색·글자 크기·UI 배율)
tasty list recent --kind markdown      # 그 종류로 최근 연 파일 목록
tasty set cwd --surface 42 --path /tmp # 원격 서피스가 보고하는 작업 디렉터리 변경
tasty set url --surface 42 --url URL   # 웹뷰 서피스의 주소 변경
tasty file-handler dispatch 파일경로     # 탐색기에서 더블클릭한 것과 같은 경로로 파일 열기
tasty file-handler reload               # 파일 핸들러 설정 파일을 다시 읽기
tasty file-handler detectors            # 파일 형식 판별 규칙이 지금 어떻게 합쳐져 있는지 보기
```

`set cwd` 와 `set url` 은 대상이 각각 원격 서피스·웹뷰 서피스일 때만 동작합니다. 일반 터미널 서피스에 쓰면 지원하지 않는 대상이라는 오류를 반환합니다.

`file-handler dispatch` 는 파일 경로만 받습니다. `https://…` 같은 웹 주소를 넘기면 오류를 반환합니다. 이 명령으로 연 파일은 새 탭으로 뒤에 붙기만 하고, 사용자가 보고 있던 탭은 바뀌지 않습니다. GUI 없이 서버에서 실행하는 headless 빌드는 파일을 열 수 없으므로, 이 명령에 요청을 받아들였다고 답하지 않고 이 빌드에서는 지원하지 않는다는 오류를 반환합니다.

`file-handler reload` 의 응답에는 `rejected` 목록이 있습니다. 설정 파일에서 지금 적용되지 않은 항목의 `id` 와 사유(`reason`)가 들어 있고, 모두 적용됐으면 빈 목록입니다. 사유는 셋입니다.

- `missing_owner_prefix` — `id` 앞에 `user/` 같은 소유자 부분을 붙이지 않았습니다(`user/이름` 으로 적어야 합니다). 이 항목은 버려집니다.
- `missing_detector_or_action` — 직접 만든 `user/…` 항목에 어떤 파일을 다룰지(`detector`)나 무엇을 할지(`action`)가 비어 있습니다. 이 항목은 버려집니다.
- `target_not_contributed` — 기본 핸들러나 플러그인 핸들러를 고치는 항목인데 그 대상이 지금 없습니다. 플러그인이 꺼져 있거나 `id` 가 틀린 경우입니다. 항목은 남아 있어서, 플러그인이 켜지면 그대로 적용됩니다. 켠 뒤에도 남아 있다면 `id` 를 확인하세요.

`file-handler detectors` 는 파일 형식 판별 규칙(detector)마다 지금 적용되는 값(표시 이름·아이콘·켜짐 여부·규칙)과, 그 값을 만든 출처별 원본(`contributions`)을 함께 보여 줍니다. 출처는 `host`(기본 제공) · `plugin:<id>` · `user`(설정 파일)입니다. 설정 파일에 적은 값이 실제로 이겼는지 확인할 때 씁니다 — `contributions` 는 설치된 순서로 나열되므로, 어느 값이 적용됐는지는 위쪽의 적용 값을 보세요.

`list info`의 워크스페이스 수와 활성 위치는 조회한 윈도우의 값이며, 함께 반환된 워크스페이스 ID로 소속을 확인할 수 있습니다. 전체 워크스페이스는 `list workspaces`, 각 윈도우의 상태는 `list windows`로 확인하세요.

`list pressure`는 응답이 느릴 때 원인을 고르는 데 씁니다. 답이 열한 덩어리로 나뉘어 있고 **세는 대상이 서로 다릅니다** — `queue_before_gate`는 명령이 큐에서 기다린 시간이라 나중에 거절될 요청까지 세고, `handler_after_gate`는 실제로 처리된 것만 세며, `plugin_round_trip`은 Tasty가 플러그인의 답을 기다린 시간이고(답이 실제로 온 것만 셉니다), `db`는 저장소에 쓴 내용이 디스크에 안착하는 데 걸린 시간입니다. 기다린 시간이 크면 밀린 것이고, 처리 시간이 크면 명령 자체가 무거운 것이며, 셋째가 크면 그 시간은 플러그인 안에 있었던 것이고, 넷째가 크면 디스크가 느린 것입니다. 두 숫자를 빼서 "거절된 수"로 읽지 마세요 — 그렇게 안 됩니다. 값은 이 인스턴스가 켜진 뒤의 누계이고, 잰 적이 없는 평균은 `null`로 옵니다.

다섯째 `connections`만 **시간이 아니라 자리**입니다. 앞의 넷이 전부 "얼마나 걸렸나"인 반면 이것은 "붙을 자리가 남았나"입니다 — 아무것도 느리지 않아도 동시에 붙을 수 있는 수가 차면 새로 여는 연결은 **답을 받기도 전에 거절됩니다.** `live`는 지금 붙어 있는 수로 이 덩어리의 누계·순간값 중 유일하게 내려가는 값이고(뒤의 accept 대기 평균은 파생값이라 내려갈 수 있습니다), `live_max`는 켜진 뒤의 최댓값, `limit`은 Tasty가 집행하는 상한, `accepted`와 `refused_saturated`는 각각 받아들인 수와 상한에 걸려 거절한 수의 누계입니다. `live`가 `limit`에 붙어 있거나 `refused_saturated`가 오르고 있으면 그것은 느린 것이 아니라 **자리가 없는 것**이고, 그때는 안 쓰는 연결(오래 붙어 있는 attach 등)을 줄여야 합니다. 여기 세는 것은 요청이 아니라 연결이라 요청을 하나도 안 보내는 연결도 자리를 차지합니다. 이 덩어리에는 시간도 하나 있습니다 — `accept_wait_bound_us_max` · `accept_wait_bound_us_mean`(기록 수 `accept_waits`)은 새 연결이 Tasty에 받아들여지기까지 기다렸을 수 있는 **최대** 시간입니다. Tasty는 새 연결을 최대 100 ms 간격으로 확인하므로, 명령마다 새로 연결하는 도구(`tasty` CLI 등)는 요청이 읽히기 전에 그만큼 기다릴 수 있고, 이 시간은 다른 덩어리에는 나오지 않습니다. 정확한 대기가 아니라 넘을 수 없는 상한이라, 실제 대기는 대개 그보다 짧습니다.

시간을 재는 세 덩어리에는 평균·최댓값 옆에 **분포**(`wait_us_hist` · `us_hist`)가 함께 옵니다. "전부 조금씩 느리다"와 "대부분 빠른데 몇 건이 튄다"는 평균도 최댓값도 같을 수 있는데 할 일이 반대이기 때문입니다 — 앞은 용량 문제이고, 뒤는 그 몇 건이 무엇이었는지 찾는 일입니다. 분포는 칸의 상한 목록(`bounds_us`, 10 µs부터 1초까지 열한 칸)과 칸마다의 건수(`counts`)로 옵니다. `counts`가 한 칸 더 긴 것은 **마지막 칸이 1초를 넘은 것들**이기 때문이고, 그 칸에는 상한이 없어 얼마나 넘었는지는 같은 덩어리의 `us_max`가 답합니다. 칸끼리 겹치지 않으므로 전부 더하면 관측 수가 됩니다. `queue_before_gate`에서는 그 관측 수가 `waits`(대기를 잰 명령 수)로 함께 오고, 평균 대기는 그것으로 나눈 값입니다. 같은 덩어리의 `commands`는 한 번에 꺼낸 묶음이 끝나야 오르므로, 조회한 순간에는 지금 처리 중인 묶음만큼 `waits`보다 작을 수 있습니다. p99 같은 분위수는 주지 않습니다 — 칸 해상도 안에서만 말할 수 있는 값이라 한 숫자로 내놓으면 없는 정밀도를 말하게 됩니다. 시간을 재는 세 덩어리 말고는 분포가 없습니다.

여섯째 `db_pragmas`는 **시간도 자리도 아니고, Tasty가 쓰는 두 데이터베이스의 설정**입니다. 데이터베이스를 열 때 요청한 설정(`requested`)과 실제로 걸린 값(`effective`)을 나란히 보여 줍니다 — 요청이 조용히 거절될 수 있어서, 요청했다는 사실만으로는 그 설정이 걸렸는지 알 수 없기 때문입니다. `memory_db`와 `state_db`가 각각 한 칸이고, `degraded`가 `true`면 하나 이상의 설정이 요청대로 안 걸린 것입니다. 그래도 데이터베이스는 계속 쓰이며, 느리거나 장애 시 덜 안전할 수 있다는 뜻입니다. `state_db`가 `null`이면 이 인스턴스에서 그 데이터베이스가 열려 있지 않은 것이고, 윈도우 없이 도는 인스턴스는 늘 그렇습니다. 이 덩어리는 켜진 뒤 쌓이는 값이 아니라 열 때 한 번 정해진 값입니다. `memory_db`의 `init_failure`는 시작할 때 메모리 파일을 못 열어 임시 메모리로 대신 뜬 경우 그 원인(`cause`)과 오류 문구(`error`)를 싣고, 그렇지 않으면 `null`입니다. 이때는 설정이 다 걸렸어도 `degraded`가 `true`입니다.

일곱째 `stream_push`는 **Tasty가 답한 요청이 아니라 밀어 보낸 화면 데이터**를 셉니다. 원격 attach처럼 오래 붙어 있는 연결에 Tasty가 보낸 조각들입니다. `frames_dropped`는 연결은 살아 있는데 받는 쪽이 따라오지 못해 버린 조각 수이고, `clients_lagged_out`은 너무 뒤처져 끊은 연결 수입니다 — 둘 다 켜진 뒤의 누계라 올라가기만 합니다. `backlog`은 지금 보내려고 쌓여 있는 조각 수로 내려가기도 하며, `sink_capacity`는 연결 하나가 쌓아 둘 수 있는 상한입니다. `frames_dropped`가 오르고 있으면 어느 attach가 화면 일부를 놓쳤다는 뜻이고, Tasty의 attach는 그 사실을 받으면 스스로 다시 붙어 화면을 새로 받습니다.

여덟째 `queue_admission`과 아홉째 `queue_dispatch`는 **명령이 줄 서는 큐의 양 끝**입니다. `queue_admission`은 지금 큐에 든 양(`queued_bytes` 바이트 · `queued_commands` 명령 수 · 그중 Tasty가 스스로 넣은 `queued_injected`)과 켜진 뒤의 최고 바이트(`peak_bytes`), 그리고 큐가 차서 **들여보내지도 못한** 요청 수입니다 — `refused_bytes`는 바이트 상한에, `refused_depth`는 Tasty 내부 명령의 개수 상한에 걸린 것입니다. 견줄 상한도 함께 옵니다(`limit_bytes` · `limit_injected_depth`). 거절된 요청은 큐에 한 번도 들어가지 않아 다른 어느 덩어리에도 남지 않으므로, 요청이 "큐가 가득 찼다"는 오류로 돌아온다면 여기서 확인합니다. `queue_dispatch`는 큐에서 **꺼낸 쪽**입니다 — 꺼낸 횟수(`rounds`)와 그중 한 번에 처리할 양이나 시간을 다 써서 멈춘 횟수(`rounds_stopped_by_count` · `rounds_stopped_by_time`), 기다리는 동안 요청자가 정한 대기 시간이 지나 실행하지 않은 명령 수(`expired_before_run`), 시작한 명령 수(`started`), 그리고 지금 실행 중이면서 요청자가 아직 기다리는 요청 수(`in_flight`, 내려가기도 합니다)와 그 최댓값(`in_flight_max`)입니다. `rounds_stopped_by_time`이나 `expired_before_run`이 오르고 있으면 명령을 꺼내는 쪽이 밀리고 있는 것입니다.

열째 `keyed_requests`는 **멱등 키(`idempotency_key`)를 붙여 보낸 요청만** 셉니다. 같은 키로 다시 보낸 요청을 Tasty가 어떻게 다뤘는지가 칸마다 갈립니다 — `executed`는 처음 본 키라 실행한 것, `replayed`는 같은 요청이 다시 와서 실행하지 않고 먼저 낸 답을 돌려준 것, `conflicted`는 같은 키에 다른 내용이 와서 아무것도 하지 않은 것, `discarded`는 실행은 됐지만 답이 버려진 것, `in_flight`는 같은 요청이 아직 실행 중이라 그 결과를 함께 기다린 것입니다. 요청 하나는 한 번만 셉니다. `replayed`가 `executed`에 비해 크면 재시도가 몰리고 있다는 뜻입니다. 이 `in_flight`는 `queue_dispatch`의 것과 다른 값입니다.

열한째 `slow_requests`는 **누계가 아니라 느렸던 요청을 한 건씩** 보여 줍니다. 큐에서 기다린 시간 · Tasty가 처리한 시간 · 플러그인을 기다린 시간을 더해 `threshold_us`(100 ms) 이상인 요청마다 한 줄이고, 오래된 것부터 최대 `capacity`(32) 줄까지 남습니다 — 넘치면 가장 오래된 줄부터 밀려나고, `admitted`는 켜진 뒤 남긴 줄의 누계라 보이는 줄 수와의 차가 밀려난 수입니다. 한 줄에는 `request_seq`(Tasty가 요청마다 매기는 번호 — 요청의 `id`와 다른 값입니다), `host`(명령 이름 `method` · 보낸 쪽 `caller` · 큐 대기 `queue_wait_us` · 처리 시간 `host_us` · 호출자가 받은 답 `outcome` = `ok`/`error` 와 오류 코드 `error_code` — 아직 답이 안 나갔으면 둘 다 `null`), 플러그인으로 넘긴 요청이면 `plugin_hops`(플러그인 · 그 플러그인이 받은 요청 번호 `host_request_id` · 기다린 시간 · 결과 `ok`/`error`/`expired`/`cancelled`), 그리고 합계 `total_us`가 옵니다. 앞의 분포가 "100 ms를 넘은 것이 몇 건"까지 말한다면 이 덩어리는 **그 한 건의 시간이 어디에 있었는지**를 말합니다 — `plugin_hops`의 대기가 크면 그 요청은 플러그인 안에 있었던 것이고, 플러그인 로그에서 `host_request_id`와 같은 요청 id를 찾으면 됩니다. Tasty의 로그도 플러그인이 오류로 답하거나 끝내 답하지 않았을 때 그 줄에 `id=`와 `request_seq=`를 함께 적습니다. 요청 내용·토큰은 싣지 않고, 이 답을 묻는 조회 자신은 남기지 않습니다. 켜진 뒤 메모리에만 있어 다시 켜면 비고 번호도 1부터 다시 셉니다.

`list info`는 이 Tasty가 무엇을 할 줄 아는지도 `capabilities`로 함께 답합니다. 이름과 버전이 짝으로 오며, 버전 문자열만으로는 알 수 없는 것을 여기서 확인합니다 — 같은 버전이라도 빌드 구성에 따라 할 수 있는 일이 다릅니다. 모르는 이름은 그냥 무시하면 됩니다.

## 자주 쓰는 명령 표

| 하고 싶은 것 | 명령 |
|---|---|
| 계층 구조 보기 | `tasty list tree` |
| 서피스 목록 | `tasty list surfaces` |
| 만들 수 있는 서피스 종류 | `tasty list surface-kinds` |
| 텍스트 보내기 (Enter 포함) | `tasty send text "ls\r" --surface ID` |
| 키 보내기 | `tasty send key enter --surface ID` |
| 마크 찍기 | `tasty set mark --surface ID` |
| 마크 이후 읽기 | `tasty read since-mark --surface ID --strip-ansi` |
| 위치를 들고 이어 읽기 | `tasty read since-mark --surface ID --cursor N --stream S` |
| 응답 대기 상한 | `tasty --response-timeout-ms MS <명령>` |
| 화면 읽기 | `tasty read screen --surface ID --lines N` |
| 알림 | `tasty notify "본문" --title "제목"` |
| 스크린샷 | `tasty screenshot --path out.png [--surface ID] [--window ID]` |
| 도움말 | `tasty --help`, `tasty <명령> --help`, `tasty -a -h` (전체 트리) |

## 문제 해결

- **연결이 안 됩니다** — Tasty 가 실행 중인지, `~/.tasty/tasty.port` 파일이 있는지 확인합니다. 파일이 남아 있는데 접속이 안 되면 이전 인스턴스가 비정상 종료된 것입니다 ([문제 해결](../help/troubleshooting.md)).
- **`--surface` 없이 부르면 거부됩니다** — `TASTY_SURFACE_ID` 가 없는 셸(Tasty 밖)에서는 대상 서피스를 알 수 없어 명령이 오류로 끝납니다. Tasty 는 포커스된 서피스로 추측하지 않습니다 — 어느 윈도우가 앞에 나와 있든 같은 명령은 같은 결과를 냅니다. 스크립트에서는 항상 `--surface` 를 적습니다.
- **`read since-mark` 가 비어 있습니다** — 마크를 찍기 전에 출력이 끝났거나, 명령이 아직 안 끝난 것입니다. `read screen` 으로 현재 상태를 봅니다.
- **`"sent":false` 가 든 오류 한 줄이 나옵니다** — 연결된 Tasty 가 그 기능(위치를 든 읽기, 응답 대기 상한 등)을 모르는 이전 버전입니다. 요청은 보내지 않았습니다. `capability` 에 적힌 이름이 무엇이 없는지를 말합니다.
- **`Error (…)` 줄 아래에 `data: {…}` 줄이 붙습니다** — Tasty 가 실패의 분류를 함께 보냈습니다(예: 메모리 저장 실패의 `storage_failure`, 거절 사유의 `reason`). `data: ` 뒤는 한 줄 JSON 이라 스크립트는 첫 줄 대신 이 값으로 분기할 수 있습니다. 첫 줄 `Error (코드): 메시지` 는 이 줄이 있든 없든 같습니다. `tasty events follow` 와 `tasty plugin audit-follow` 도 이 줄을 붙이며, 두 명령의 첫 줄은 `Error: Error (코드): 메시지` 로 앞에 `Error: ` 가 하나 더 붙은 모양 그대로입니다.
- **`Error (-32065): …` 로 끝납니다** — Tasty 가 처리할 요청이 밀려 있어 이 요청을 받지 않았습니다. 요청은 실행되지 않았으므로 잠시 뒤 그대로 다시 부르면 됩니다.
- **`Error (-32066): …` 로 끝납니다** — 연결한 뒤 20 초 안에 요청을 보내지 않아 Tasty 가 연결을 닫았습니다. 아무것도 실행되지 않았습니다. 직접 소켓을 여는 도구라면 연결 직후 바로 요청을 보냅니다. 한 번 요청을 보낸 연결은 요청 사이에 오래 쉬어도 닫히지 않습니다.
- **`screenshot` 이 어느 윈도우를 찍는지 모르겠습니다** — 자동 선택은 **메인 윈도우(터미널 윈도우)만** 셉니다. 메인 윈도우가 하나면 `--window` 없이 그 윈도우를 찍고, 메인 윈도우가 여럿이면 `--window` 가 필수입니다(포커스된 윈도우를 임의로 고르지 않습니다). 설정 윈도우처럼 `list windows` 에 안 나오는 윈도우는 이 계산에 들어가지 않습니다 — 설정 윈도우가 떠 있어도 `--window` 없이 메인 윈도우가 찍히며, 설정 윈도우 자체를 찍으려면 `--window` 로 그 ID 를 직접 적습니다.

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [Claude · Codex 와 함께 쓰기](claude-codex.md) — 자식 에이전트를 띄우고 완료를 통지받기.
- [작업 순서 관리](tasks.md) — 여러 작업을 의존 관계로 묶어 순서대로 실행하기.
- [훅 · 알림 · 웹훅](hooks-notifications.md) — 이벤트로 명령을 자동 실행하기.

세션 토큰을 사용하는 에이전트의 호출 한도는 일반 명령뿐 아니라 플러그인 명령과 전체 목록 조회에도 적용됩니다. 허용된 요청은 호출량에 한 번 기록되며, 한도를 넘으면 요청을 실행하지 않고 오류로 답합니다. 토큰 없는 로컬 CLI의 기존 예외는 유지됩니다.

플러그인 namespace 호출은 활성화된 소유 플러그인과 해당 IPC 훅에 필요한 활성 확장만 시작합니다. 비활성 플러그인을 켜거나 무관한 플러그인을 시작하지 않습니다. 없는 namespace는 아무것도 시작하지 않으며, 알려진 namespace 안의 메서드 오타는 소유 플러그인을 시작한 뒤 오류를 반환할 수 있습니다.
