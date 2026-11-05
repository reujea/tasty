# tasty CLI로 터미널 조작하기

`tasty` CLI로 실행 중인 Tasty의 터미널을 만들고 조작합니다. 연결과 대상 ID를 확인한 뒤 명령을 보내고, `read since-mark`로 표시해 둔 위치 이후의 출력을 읽어 보세요. 스크립트와 AI 에이전트가 같은 명령을 사용할 수 있습니다.

이 페이지는 처음 쓰는 순서대로 따라가면 됩니다. 연결을 확인하고, 대상 ID를 찾고, 작업용 터미널을 하나 만들어 명령을 보내고 출력을 읽은 뒤 정리합니다. 읽기 위치 지정·메타데이터·큐·메모리 같은 나머지 명령은 [CLI 명령 참조](cli-reference.md)에, 응답 지연·연결·오류 코드별 조치는 [CLI 응답 지연과 연결 진단](../help/cli-diagnostics.md)에 있습니다.

## 준비

- Tasty 가 실행 중이어야 합니다. CLI 는 `~/.tasty/tasty.port` 에 적힌 포트로 실행 중인 인스턴스에 접속합니다.
- Tasty 가 띄운 터미널 안에서는 `tasty` 가 이미 PATH 에 있습니다. 밖(다른 터미널 앱)에서 쓰려면 Tasty 실행 파일이 있는 경로를 PATH 에 넣습니다. 설치 방식별 경로는 [설치](../getting-started/install.md#설치-위치) 에 있습니다.
- Tasty 가 띄운 셸에는 `TASTY_SURFACE_ID` 환경변수가 들어 있습니다. `--surface` 를 생략한 명령은 대부분 이 값을 쓰므로(`tasty new workspace` · `tasty output observe start` 는 예외), 자기 터미널을 조작할 때는 ID 를 적지 않아도 됩니다.
- Codex 플러그인이 설치돼 있으면, Codex 의 명령 실행 안(`CODEX_THREAD_ID` 가 있는 환경)에서는 그 셸의 `TASTY_SURFACE_ID` 가 다른 터미널의 값일 수 있어 plugin 이 추가한 명령(예: `tasty codex …`, `tasty claude …`)은 이 값으로 `--surface` 를 채우지 않습니다. 대상이 필요하면 `--surface` 로 지정합니다. `CODEX_THREAD_ID` 값이 올바르지 않으면 `tasty codex …`·`tasty claude …` 는 실행되지 않고, 다른 plugin 명령은 평소처럼 실행됩니다.
- 다른 Tasty 의 터미널에서 Tasty 를 실행해도, 새 Tasty 가 띄운 셸과 훅 명령은 바깥 Tasty 의 세션 토큰(`TASTY_SESSION_TOKEN`)과 에이전트 ID(`TASTY_AGENT_ID`)를 물려받지 않습니다. 그 안에서 부른 `tasty` 명령은 새 Tasty 에 일반 사용자로 접속합니다.

```sh
echo $TASTY_SURFACE_ID     # 예: 42
tasty list info            # 버전·조회한 윈도우의 워크스페이스 수 — 접속 확인용
```

## 용어와 ID

Tasty 의 화면은 **워크스페이스 > 페인 > 탭 > 서피스** 순으로 겹쳐 있습니다. 서피스(surface)는 터미널이나 마크다운 문서처럼 실제 내용을 표시하는 영역입니다. 대상 ID를 직접 지정한 명령은 어느 윈도우에 포커스가 있든 그 대상을 사용합니다.

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

## 첫 실습: 작업용 터미널에서 명령 실행하기

사용자가 쓰고 있는 터미널에 명령을 보내지 말고, 작업용 탭을 따로 만들어 그 ID로 작업하세요. 아래 예시의 `3`과 `57`은 자리 표시입니다. 실제로는 앞 명령의 응답에 나온 값을 씁니다.

```sh
tasty list panes                               # 탭을 만들 페인 ID 확인 (예: 3)
tasty new tab --pane 3 --cwd ~/proj            # 응답의 surface_id 가 새 터미널 (예: 57)
tasty set mark --surface 57                    # 지금 출력 위치에 표시
tasty read last-command --surface 57           # 보내기 전 ended_at 을 기억
tasty send text "ls\r" --surface 57            # 명령 보내기 (\r 이 Enter)
tasty read last-command --surface 57           # ended_at 이 바뀌면 명령이 끝난 것
tasty read since-mark --surface 57 --strip-ansi # 표시 이후의 출력 읽기
tasty close surface --surface 57               # 다 쓴 터미널 정리
```

- `new tab`으로 만든 탭은 사용자가 보던 탭을 바꾸지 않습니다. 응답의 `active_tab`이 아니라 `surface_id`를 사용하세요.
- 명령이 끝났는지는 정해진 시간만큼 기다리거나 출력이 비었는지만 보고 판단하지 마세요. bash·zsh에서는 셸 통합이 명령마다 종료 기록을 남기므로, 보내기 전과 후의 `read last-command` 응답에서 `ended_at`이 바뀌었는지와 `exit_code`를 확인합니다. 셸 통합이 없는 셸에서는 `read screen`으로 프롬프트가 돌아왔는지 확인합니다.
- 명령 하나의 인자를 모두 보려면 `tasty help <명령>`을 실행합니다. 예: `tasty help read since-mark`.

## 자주 쓰는 명령 표

| 하고 싶은 것 | 명령 |
|---|---|
| 계층 구조 보기 | `tasty list tree` |
| 서피스 목록 | `tasty list surfaces` |
| 작업용 탭 만들기 | `tasty new tab --pane ID` |
| 텍스트 보내기 (Enter 포함) | `tasty send text "ls\r" --surface ID` |
| 키 보내기 | `tasty send key enter --surface ID` |
| 마크 찍기 | `tasty set mark --surface ID` |
| 마크 이후 읽기 | `tasty read since-mark --surface ID --strip-ansi` |
| 마지막 명령의 종료 기록 | `tasty read last-command --surface ID` |
| 화면 읽기 | `tasty read screen --surface ID --lines N` |
| 서피스 닫기 | `tasty close surface --surface ID` |
| 알림 | `tasty notify "본문" --title "제목"` |
| 스크린샷 | `tasty screenshot --path out.png [--surface ID] [--window ID]` |
| 도움말 | `tasty help`, `tasty help <명령>`(= `tasty <명령> --help`), `tasty -a` (모든 명령 경로와 짧은 설명. 인자는 각 명령의 `--help`) |

`tasty help`는 명령을 기본 조작·에이전트 협업·백그라운드·원격처럼 목적별로 묶어 보여 주고, 맨 앞에 첫 작업 순서를 안내합니다. `-h`는 짧은 설명, `--help`와 `tasty help <명령>`은 긴 설명을 보여 줍니다. Claude·Codex 도움말의 마지막 "내부 연동" 그룹은 훅이 부르는 명령이라 직접 입력하지 않습니다.

## 기본 패턴: 마크 → 보내기 → 마크 이후 읽기

명령을 보내기 전에 표시를 남겨 이후 출력을 읽는 절차입니다. 같은 터미널에서 나온 다른 출력도 함께 포함될 수 있습니다.

1. `tasty set mark` — 지금 출력 위치에 표시를 남깁니다.
2. `tasty send text "명령\r"` — 텍스트를 보냅니다. `\r` 이 Enter 입니다.
3. 명령이 끝난 뒤 `tasty read since-mark --strip-ansi` — 표시 이후에 나온 출력만 읽습니다.

```sh
tasty set mark --surface 42
tasty send text "cargo test 2>&1 | tail -20\r" --surface 42
# 끝났는지는 아래 첫 실습처럼 read last-command 나 read screen 으로 확인합니다
tasty read since-mark --surface 42 --strip-ansi
```

- `send text` 는 `\r` `\n` `\t` `\\` `\0` 이스케이프를 해석합니다. 셸 따옴표 안에 그대로 적으면 됩니다.
- `--strip-ansi` 를 붙이면 색상 등 제어 시퀀스를 걷어낸 순수 텍스트가 옵니다. 파싱할 때는 항상 붙입니다.
- 마크는 `set mark` 를 다시 부를 때까지 유지됩니다. `read since-mark` 는 마크를 옮기지 않으므로 여러 번 읽어도 같은 구간이 나옵니다.

명령이 끝났는지 모를 때는 화면을 읽어 프롬프트가 돌아왔는지 봅니다.

```sh
tasty read screen --surface 42 --lines 5     # 화면 하단 5줄 (부족하면 스크롤백까지)
tasty is-typing --surface 42                  # 최근 5초 내 사람이 키를 누르거나 붙여넣었는지
```

`read screen` 은 기본적으로 흐리게 표시되는 자동완성 제안(예: Claude Code 의 회색 제안 텍스트)을 제외합니다. 포함하려면 `--show-dim`. 이 제외는 입력 제안뿐 아니라 흐리게 그린 모든 글자에 적용됩니다. 응답의 `dim_omitted_rows`가 0보다 크면 흐린 글자 때문에 내용이 달라진 행이 있으니 `--show-dim`으로 다시 읽어 비교하세요.

원격 연결(attach)로 띄운 mirror 터미널에서 `read screen`·`read since-mark`·`set mark`를 쓰면 원격 컴퓨터의 원래 터미널을 읽습니다. 한 번에 받는 출력이 더 작을 수 있으니 응답의 `next_cursor`로 이어 읽으세요. 원격 Tasty가 이 기능을 지원하지 않거나 연결이 끊긴 동안에는 오류로 답하므로, 원격 Tasty를 같은 버전으로 갱신하거나 다시 연결된 뒤 읽으세요.

`--lines N`보다 적은 줄이 반환되면 응답의 `scrollback_len`을 확인하세요. 값이 `0`이면 더 읽을 스크롤백이 없다는 뜻입니다. 전체 화면 앱(TUI)을 처음 실행했을 때 이런 경우가 생길 수 있습니다. 스크롤백이 남아 있는데도 요청한 줄 수보다 적게 반환된다면 출력 조회를 확인해 보세요. `alt_screen`은 현재 전체 화면 앱을 사용 중인지 알려줍니다.

## 키 보내기

Enter 외의 키는 `send key` 로 보냅니다.

```sh
tasty send key enter --surface 42
tasty send key ctrl+c --surface 42
tasty send key escape --surface 42
tasty send key up --surface 42
```

키 이름: `enter` `tab` `escape`(또는 `esc`) `backspace` `delete` `insert` `up` `down` `left` `right` `home` `end` `pageup` `pagedown` `f1`~`f12`. 조합은 `ctrl+c` `alt+x` 처럼 `+` 로 잇습니다. `shift+` 는 받아들이지만 무시됩니다.

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
tasty split --level surface --surface this --direction vertical  # 내 서피스를 좌우 분할
tasty split --level pane --pane 3 --direction horizontal          # 페인 분할
tasty new tab --pane 3 --cwd ~/proj                     # 페인에 새 탭
tasty close surface --surface 99                        # 서피스 닫기
tasty close tab --tab 12
tasty close workspace --workspace 3                     # 워크스페이스를 통째로 (안의 탭·서피스까지)
tasty close window --window 1                           # 윈도우 닫기
tasty close self                                        # 지금 이 서피스 닫기
```

예전 표기 `split --target-surface`·`--target-pane`, `close workspace|window --id`도 그대로 받습니다. 두 표기를 함께 주면 같은 옵션을 두 번 준 것으로 거절합니다.

`--surface this` 는 자기 자신(`TASTY_SURFACE_ID`)입니다. `--type markdown --file README.md` 처럼 터미널이 아닌 표면도 만들 수 있습니다 ([파일 관리와 보기](../using/files.md)). `new workspace` · `new tab` · `split` 의 `--cwd` 는 탐색기(`--type explorer`)에서도 통해, `--path` 가 없으면 그 폴더를 루트로 엽니다. 둘 다 적으면 `--path` 가 이깁니다. 탐색기는 `--cwd` 를 생략해도 다른 서피스의 작업 디렉토리를 이어받지 않고 홈을 엽니다.

`tasty new tab` 으로 연 탭은 종류와 상관없이 사용자가 보던 탭을 바꾸지 않습니다. 새 탭은 페인의 맨 뒤에 붙고, 사용자가 고를 때까지 뒤에 있습니다. 응답의 `active_tab` 은 새 탭이 아니라 그 페인에서 지금 선택된 탭이므로, 새 탭을 다룰 때는 응답의 `surface_id` 를 씁니다. 단축키나 메뉴로 사용자가 직접 연 탭은 바로 선택됩니다.

`tasty new window` 로 연 윈도우는 사용자가 보던 윈도우의 포커스를 가져가지 않습니다. 그래서 뒤이어 대상을 안 적은 명령(`tasty new workspace` 등)은 새 윈도우가 아니라 사용자가 보던 윈도우에 만들어집니다.

새 윈도우에 워크스페이스를 만들려면 그 윈도우의 서피스 ID 를 `tasty new workspace --surface <ID>` 로 적습니다 — `tasty list windows` 가 윈도우마다 `workspace_ids` 를, `tasty list surfaces` 가 서피스마다 `workspace_id` 를 보여 줍니다. 적은 서피스가 어느 윈도우에도 없으면 명령은 다른 윈도우로 새지 않고 오류로 끝납니다. `--cwd` 를 생략하면 새 워크스페이스의 작업 디렉토리는 적은 서피스의 것을 이어받습니다(그 윈도우에서 사용자가 보고 있는 서피스가 아닙니다).

새 윈도우는 사용자가 보던 윈도우 뒤에 생기고 키 입력도 가져가지 않습니다(macOS · Windows). Linux 의 X11 에서는 윈도우 관리자에게 그렇게 해 달라고 요청하지만 따를지는 윈도우 관리자가 정하고, Wayland 에서는 컴포지터가 정합니다. 어느 쪽이든 대상 없는 명령이 가는 윈도우는 바뀌지 않습니다.

워크스페이스와 윈도우는 마지막 하나를 닫지 못합니다. 워크스페이스를 닫으면 윈도우까지 사라지는 것이 아니라
거절되므로, 윈도우를 없앨 생각이면 `tasty close window` 를 따로 씁니다(윈도우 없이 도는
[headless 빌드](../getting-started/install.md#headless-빌드)에는 닫을 윈도우가 없어 마지막 워크스페이스는 그냥 닫을 수 없습니다). 자기 터미널이 들어 있는 대상은
닫히지 않습니다 — 그때는 `tasty close self` 입니다. 원격에 접속해 미러로 띄운 워크스페이스도 닫지 못합니다 —
그건 접속을 끊어서 정리합니다. 반대로 **누군가 원격에서 접속해 지금 쓰고 있는 터미널**이 들어 있는
워크스페이스도 닫지 못합니다 — 그 사람이 접속을 놓은 뒤에야 닫힙니다. 보고 있지 않은 워크스페이스를 닫아도 화면에 떠 있는 워크스페이스는
그대로 남습니다.

**워크스페이스 닫기는 되돌릴 수 없습니다.** 안에서 돌던 터미널이 전부 종료되고, "닫은 항목" 으로
되살릴 수 없으며, 스크롤백도 남지 않습니다. 닫은 항목 복원은 사용자가 화면에서 직접 닫은 항목에만 적용됩니다.
닫으려는 대상이 맞는지 `tasty list workspaces`로 먼저 확인하세요.

## 사람이 타이핑 중이면 보내지 않기

```sh
tasty send text "make test\r" --surface 42 --wait-idle
```

`--wait-idle` 은 판정과 전송을 한 번에 합니다. `tasty is-typing` 으로 먼저 확인하고 보내면 그 사이에
사람이 타이핑을 시작할 수 있는데, 이 플래그는 그 틈을 없앱니다. 타이핑 중이면 보내지 않고
`"sent": false` 와 이유를 돌려줍니다.

## 문제 해결

- **연결이 안 됩니다** — Tasty 가 실행 중인지, `~/.tasty/tasty.port` 파일이 있는지 확인합니다. 파일이 남아 있어도 이전 실행의 파일이거나 다른 인스턴스의 경로일 수 있습니다 ([문제 해결](../help/troubleshooting.md)).
- **`--surface` 없이 부르면 거부됩니다** — `TASTY_SURFACE_ID` 가 없는 셸(Tasty 밖)에서는 대상 서피스를 알 수 없어 명령이 오류로 끝납니다. Tasty 는 포커스된 서피스로 추측하지 않습니다 — 어느 윈도우가 앞에 나와 있든 같은 명령은 같은 결과를 냅니다. 스크립트에서는 항상 `--surface` 를 적습니다.
- **`read since-mark` 가 비어 있습니다** — 마크를 찍기 전에 출력이 끝났거나, 명령이 아직 안 끝난 것입니다. `read screen` 으로 현재 상태를 봅니다.
- **`screenshot` 이 어느 윈도우를 찍는지 모르겠습니다** — 자동 선택은 **메인 윈도우(터미널 윈도우)만** 셉니다. 메인 윈도우가 하나면 `--window` 없이 그 윈도우를 찍고, 메인 윈도우가 여럿이면 `--window` 가 필수입니다(포커스된 윈도우를 임의로 고르지 않습니다). 설정 윈도우처럼 `list windows` 에 안 나오는 윈도우는 이 계산에 들어가지 않습니다 — 설정 윈도우가 떠 있어도 `--window` 없이 메인 윈도우가 찍히며, 설정 윈도우 자체를 찍으려면 `--window` 로 그 ID 를 직접 적습니다.

응답이 늦거나 `Error (…)` 줄로 끝나면 [CLI 응답 지연과 연결 진단](../help/cli-diagnostics.md)에서 오류 코드별 조치를 확인하세요.

## 다른 명령 찾기

입문에서 다루지 않은 명령은 목적별 페이지에 있습니다.

- <a id="여러-에이전트가-같은-터미널을-읽을-때-위치를-들고-읽기"></a>[여러 에이전트가 같은 터미널을 읽을 때: 위치를 들고 읽기](cli-reference.md#여러-에이전트가-같은-터미널을-읽을-때-위치를-들고-읽기)
- <a id="서피스-들여다보기"></a><a id="서피스-상태-확인"></a>[서피스 상태 확인](cli-reference.md#서피스-상태-확인)
- <a id="자식-에이전트에게-권한-주기"></a>[자식 에이전트에게 권한 주기](cli-reference.md#자식-에이전트에게-권한-주기)
- <a id="알림-보내기"></a>[알림 보내기](cli-reference.md#알림-보내기)
- <a id="서피스에-메모-남기기-메타데이터"></a>[서피스에 메모 남기기 (메타데이터)](cli-reference.md#서피스에-메모-남기기-메타데이터)
- <a id="서피스끼리-메시지-주고받기-큐"></a>[서피스끼리 메시지 주고받기 (큐)](cli-reference.md#서피스끼리-메시지-주고받기-큐)
- <a id="자식-터미널을-에이전트처럼-굴리기"></a><a id="여러-터미널에-작업-맡기기"></a>[여러 터미널에 작업 맡기기](cli-reference.md#여러-터미널에-작업-맡기기)
- <a id="화면-없는-pty"></a>[화면 없는 PTY](cli-reference.md#화면-없는-pty)
- <a id="에이전트가-함께-쓰는-메모리"></a>[에이전트가 함께 쓰는 메모리](cli-reference.md#에이전트가-함께-쓰는-메모리)
- <a id="출력에서-신호-뽑기-관찰자"></a><a id="출력에서-필요한-정보-모으기-관찰자"></a>[출력에서 필요한 정보 모으기 (관찰자)](cli-reference.md#출력에서-필요한-정보-모으기-관찰자)
- <a id="에이전트-활동-계측"></a><a id="에이전트-사용량-확인"></a>[에이전트 사용량 확인](cli-reference.md#에이전트-사용량-확인)
- <a id="그-밖의-조회설정"></a>[그 밖의 조회·설정](cli-reference.md#그-밖의-조회설정)
- <a id="응답-대기에-상한-걸기"></a>[응답 대기에 상한 걸기](../help/cli-diagnostics.md#응답-대기에-상한-걸기)
- <a id="응답-지연과-연결-상태-확인"></a><a id="연결-수와-한도-connections"></a><a id="시간-분포"></a><a id="데이터베이스-설정-db_pragmas"></a><a id="화면-전송-stream_push"></a><a id="요청-큐-queue_admission-queue_dispatch"></a><a id="중복-요청-처리-keyed_requests"></a><a id="느린-요청-상세-slow_requests"></a><a id="요청-거절-사유-gate_refusals"></a>[응답 지연과 연결 상태 확인](../help/cli-diagnostics.md#응답-지연과-연결-상태-확인)

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [Claude·Codex와 함께 쓰기](claude-codex.md) — 자식 에이전트를 띄우고 완료를 통지받기.
- [작업 순서 관리](tasks.md) — 여러 작업을 의존 관계로 묶어 순서대로 실행하기.
- [훅 · 알림 · 웹훅](hooks-notifications.md) — 이벤트로 명령을 자동 실행하기.
- [CLI 명령 참조](cli-reference.md) — 입문 밖의 명령을 목적별로 찾기.
- [CLI 응답 지연과 연결 진단](../help/cli-diagnostics.md) — 응답 지연, 연결 수, 오류 코드별 조치.
