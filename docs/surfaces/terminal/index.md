# 터미널 (Terminal)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent(입력 주입은 [terminal-output](../../features/terminal-output/index.md)/`surface.send*`) · 원격(mirror)
- **설계**: 이 문서 — 파서 스레드와 PTY 수명·절전 복구. 이 문서 — 지원 범위와 인라인 그래픽 보류.
- **코드**: `crates/tasty-terminal/` (PTY·VTE·grid·scrollback), 렌더 `src/gfx/`
- **화면**: GPU 렌더링 셀 그리드 (egui 아님)

## 목적

`terminal` surface kind 의 본체 — PTY 셸 세션을 VTE 파싱해 셀 그리드로 에뮬레이트하고 GPU 로 그린다. host 내장 surface([work-area](../../features/work-area/index.md)의 Surface 종류).

## 내부 동작

### PTY 셸

ConPTY(Windows) / Unix PTY 로 네이티브 셸 실행(`TERM=xterm-256color`).

**실행할 셸**: 설정 셸의 판정은 `tasty_settings::ShellPathStatus` 하나다. 런타임 대체(`GeneralSettings::runtime_shell`), 부팅 판정, 설정 창 터미널 탭 경고, 첫 실행 셸 설정 화면이 모두 이 판정을 쓰므로 같은 경로에 같은 결론을 낸다.

- macOS·Linux 는 셸 이름을 보지 않는다. 실행할 수 있는 파일이면 fish·dash 처럼 bash·zsh 가 아닌 셸도 쓴다. 실행 가능 여부는 portable-pty 가 실행 전에 쓰는 것과 같은 `access(X_OK)` 로 본다.
- Windows 는 Git Bash 만 쓴다. 파일 이름이 `bash.exe` 이고 `<루트>\bin` 또는 `<루트>\usr\bin` 에 있으며 그 루트에 `cmd\git.exe` 와 `usr\bin\msys-2.0.dll` 이 있어야 한다(`tasty_utils::shell_family::is_git_for_windows_bash`). 루트의 위치는 보지 않으므로 다른 곳에 설치했거나 Portable 이어도 쓴다. zsh·cmd·PowerShell·pwsh·nu, `git-bash.exe`, MSYS2 bash, `System32\bash.exe`(WSL), 복사한 파일은 쓰지 않는다. `git-bash.exe` 는 셸 연동을 받지 못한다. `usr\bin\bash.exe` 는 쓸 수 있지만 `MSYSTEM` 이 `MSYS` 여서 Windows PATH 에 Git 이 없으면 그 셸에서 `git` 을 찾지 못한다.
- 값이 비었거나 macOS·Linux 에서 경로에 아무것도 없으면, 새 터미널을 만들 때마다 탐지한 셸(`GeneralSettings::detect_bash`)로 대신 실행한다. 빈 값은 자동 탐지에 맡긴다는 뜻이고, 없는 경로는 설치를 지웠거나 옮긴 경우라 기존처럼 대신한다. 이 대체는 실행 시점에만 적용하며 설정값과 `config.toml` 은 바꾸지 않는다. macOS·Linux 의 설정 창은 경고 색 줄로 대체 중임을 알린다. Windows 의 탐지는 기본 설치 위치 세 곳만 본다. 다른 위치의 Git Bash 는 셸 설정 화면이나 설정 창에 경로를 입력해 쓴다.
- Windows 에서 비어 있지 않은 값이 Git Bash 가 아니거나 경로에 아무것도 없으면 대신하지 않는다. 탐지한 Git Bash 가 있어도 마찬가지다. 설정 창은 원인별 오류 색 줄을 보이고, 새 터미널은 `tasty-terminal` 의 spawn 전 확인에서 child 를 만들기 전에 원인과 함께 실패한다. 값이 비었는데 Git Bash 도 없으면 `COMSPEC`(cmd.exe)으로 빠지지 않고 같은 자리에서 실패한다. 모든 셸 실행이 이 확인을 지나므로 다른 셸이 뜨는 경로는 없다.
- macOS·Linux 에서 경로가 디렉터리이거나 실행 권한 없는 파일이면 대신하지 않는다. 사용자가 고른 경로가 왜 실패하는지 보여 주고 고치게 하려는 것이다. 설정 창은 원인별 오류 색 줄을 보이고, 새 터미널은 원인을 담은 생성 실패가 된다. 실행 권한이 없으면 portable-pty 의 실행 파일 확인이, 디렉터리이면 `tasty-terminal` 의 spawn 전 확인이 child 를 만들기 전에 거절한다([자원 회수](../../architecture/resource-recovery.md)).

모든 셸 실행 경로가 `ShellConfig::from_settings` 에서 `GeneralSettings::runtime_shell` 을 읽는다. GUI 부팅은 설정 셸을 쓸 수 있으면 그대로, 대체 셸로 열 수 있으면 `warn` 로그(설정값·대체 셸)를 남기고 부팅한다. 대체할 셸이 없거나 대체하지 않는 판정(디렉터리·실행 권한 없음, Windows 의 비어 있지 않은 무효 값)이면 [첫 실행 셸 설정 화면](../../architecture/boot-sequence.md)을 열어 판정 줄로 원인을 보인다. 설정 창 터미널 탭의 셸 입력란은 쓸 수 없는 값을 그대로 보여 준다. 저장은 사용자가 셸 설정 화면이나 설정 창에서 확정할 때만 일어난다.

- Given macOS·Linux 에서 `general.shell` 이 존재하지 않는 경로이고 탐지한 셸이 있다 When GUI 를 시작한다 Then 설정 화면 없이 부팅하고 새 터미널은 탐지한 셸로 실행되며 `config.toml` 의 값은 그대로다.
- Given 같은 상태 When 설정 창을 열고 다른 항목만 바꿔 저장한다 Then 셸 값은 사용자가 적은 경로로 남는다.
- Given macOS·Linux 에서 `general.shell` 이 실행할 수 있는 fish(또는 dash) 경로다 When GUI 를 시작하거나 새 터미널을 연다 Then 그 셸로 실행되고 경고가 없다.
- Given macOS·Linux 에서 `general.shell` 이 디렉터리나 실행 권한 없는 파일이다 When GUI 를 시작한다 Then 첫 실행 셸 설정 화면이 그 원인을 판정 줄에 보이고 확정 버튼은 꺼져 있다. 실행 중에 설정을 그렇게 바꾸면 설정 창에 원인별 오류 줄이 뜨고 새 터미널은 원인과 함께 생성에 실패한다.
- Given Windows 에서 `general.shell` 이 설치본 또는 다른 위치 Portable 의 `bin\bash.exe` 다 When GUI 를 시작한다 Then 설정 화면 없이 그 Git Bash 가 셸 연동과 함께 실행되고 값은 그대로다.
- Given Windows 에서 `general.shell` 이 zsh, cmd, PowerShell, pwsh, nu, `git-bash.exe`, MSYS2 bash, `System32\bash.exe`, 임의 폴더의 `bash.exe`, `my-bash.exe`, `bash.exe` 라는 폴더, 또는 없는 경로다 When GUI 를 시작한다 Then Git Bash 가 설치돼 있어도 첫 실행 셸 설정 화면이 뜨고 입력란에 그 값이 있으며 확정 버튼은 꺼져 있다. Quit 하면 `config.toml` 의 값은 그대로다.
- Given 같은 화면 When Git Bash 경로를 입력해 확정한다 Then 그 값이 저장되고 부팅이 이어진다.
- Given Windows 에서 실행 중에 설정을 허용하지 않는 값으로 바꿨다 When 새 터미널을 연다 Then 그 셸도 다른 셸도 뜨지 않고 원인과 함께 생성에 실패한다.
- Given Windows 에서 값이 비었다 When GUI 를 시작한다 Then 기본 위치에서 찾은 Git Bash 로 부팅하고, 찾지 못하면 첫 실행 셸 설정 화면이 뜬다.

**자식 셸 환경변수**: Tasty 가 `TERM`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME` 을 넣고 `PATH` 앞에 자기 실행 파일 디렉터리를 붙인다. Tasty 프로세스에서 상속한 환경변수 중 다음은 지운다.

- `CMUX_*`: 터미널 안에서 cmux CLI 가 동작하지 않게 한다.
- 설치되고 활성인 플러그인이 매니페스트 `contributes.strip_inherited_env` 로 선언한 변수(이름·이름 접두사·이름+값 접두사). 특정 제품의 세션 표지·비밀은 그 제품 플러그인이 선언한다. 번들 플러그인의 목록은 [Claude](../../plugins/claude/index.md#tasty-자식에서-지우는-세션-변수)·[Codex](../../plugins/codex/index.md#tasty-자식에서-지우는-세션-변수) 문서에 있다. 선언 형식과 적용 조건은 [플러그인 제작 가이드](../../dev-guide/plugin-development.md#상속-환경변수-제거-contributesstrip_inherited_env)에 있다.
- 바깥 Tasty 인스턴스의 신원 `TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`·`TASTY_AGENT_ID`: 이 Tasty 를 다른 Tasty 의 터미널에서 띄웠으면 바깥 인스턴스의 세션 토큰·surface·완료 알림 경로·에이전트 ID 가 남아 있다. 셸에서 부른 `tasty` 가 그 토큰을 실으면 이 인스턴스는 모르는 토큰이라 요청을 거절한다. 네 값을 먼저 지우고 `TASTY_SURFACE_ID`·`TASTY_PARENT_HOME` 은 이 셸의 값으로 다시 넣는다. 세션 토큰·에이전트 ID 는 일반 셸에 이 인스턴스가 발급한 값이 없으므로 넣지 않는다. 셸 설정의 환경변수에 같은 이름을 넣으면 그 값은 남는다. Claude 플러그인이 띄우는 자식은 실행 명령 앞에 이 인스턴스가 발급한 값을 붙인다. 목록은 `crates/tasty-utils/src/process.rs` 의 `OUTER_IDENTITY_ENV` 한 곳에 있고, surface 훅·전역 훅·hook_handler 실행과 작업 자식(Run·후처리·reduce custom 셸, [agent runner](../../dev-guide/agent-runner.md))도 같은 목록을 지운다. 플러그인 선언은 `TASTY_*` 를 지울 수 없다.

판정은 `crates/tasty-utils/src/process.rs` 의 `is_stripped_inherited_env` 한 곳에 있고, 터미널 셸·surface 훅·전역 훅·hook_handler 실행·작업 자식이 같은 판정을 쓴다([훅](../../features/hooks/index.md#셸-핸들러-환경변수-tasty_hook_)). Windows·macOS·Linux 가 같은 코드 경로를 쓴다. 플러그인 규칙은 설치 목록과 사용자의 비활성 설정만 따른다. 부팅은 엔진이 첫 셸을 만들기 전에 규칙을 반영하므로([부팅 순서](../../architecture/boot-sequence.md)) 복원 셸도 첫 셸부터 같은 규칙을 받는다.

- Given 활성 Claude·Codex 플러그인이 설치돼 있고 Tasty 를 띄운 환경에 `CLAUDECODE`·`CODEX_THREAD_ID`·`CLAUDE_CODE_GIT_BASH_PATH`·`CODEX_HOME`·`CMUX_SOCKET_PATH` 가 있다 When cold start 로 복원 셸이나 새 셸을 연다 Then 셸에 `CLAUDECODE`·`CODEX_THREAD_ID`·`CMUX_SOCKET_PATH` 가 없고 `CLAUDE_CODE_GIT_BASH_PATH`·`CODEX_HOME` 은 남는다.
- Given 같은 환경 When 두 플러그인을 비활성화하고 새 셸을 연다 Then `CLAUDECODE`·`CODEX_THREAD_ID` 는 남고 `CMUX_SOCKET_PATH` 와 바깥 `TASTY_*` 신원 처리는 그대로다.
- Given 플러그인이 설치·활성이지만 프로세스가 아직 연결되지 않았다 When 새 셸을 연다 Then 그 플러그인의 규칙이 적용된다.

윈도우 리사이즈 시 자식에 새 크기 전파 — rows 축소 시 커서 아래 빈 행 먼저 제거 후 부족분은 위쪽 행을 scrollback 으로 캡처(커서-콘텐츠 관계 보존), 확대 시 scrollback 에서 복원.

**작업 디렉토리 상속**: 새 surface 생성 시 소스의 현재 cwd 를 상속(`general.inherit_cwd`, 기본 on). macOS/Linux 는 셸 PID 로 OS 직접 조회(`proc_pidinfo` / `/proc/<pid>/cwd`, OSC 7 캐시 우선), Windows 는 타 프로세스 cwd API 부재로 셸이 내보내는 OSC 7 캐시에만 의존(합성 rcfile 로 OSC 7 emit 강제). 합성 rcfile 은 OSC 0(cwd 기반 탭 제목, `__tasty_title`)도 함께 보내며, 빌트인 블록에 버전 스탬프(`# tasty-bashrc-v<N>`)를 심어 스탬프가 다른 기존 `~/.tasty/bashrc`·`bashrc.default` 를 셸 spawn 시 자동 재생성한다(`ensure_compiled_bashrc_in` — 사용자 편집 영역 `bashrc.user` 는 보존). carry 규칙은 [surface-cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant).

### 프로세스 종료 / 절전 복귀

자식 종료를 확인하면 ProcessExited를 한 번 발생시키고 훅 통지와 surface 정리를 수행한다.
`try_wait`는 보통 Terminal을 깨워 처리할 때 실행하며 확인 간격은 500ms다. PTY EOF 뒤에는 즉시 확인한다.

EOF와 자식 종료는 같은 사건이 아니다. EOF 직후 자식이 아직 실행 중이면 PTY reader가 10ms부터
간격을 두 배씩 늘려 최대 500ms마다 다시 깨운다. 종료가 확인되거나 해당 연결이 무효화되거나
Terminal이 사라지면 멈춘다. 이 처리가 없으면 출력이 끝난 뒤 자식 종료를 확인할 기회가 사라질 수 있다.
macOS 27.0.1 실 기기에서는 자식이 PTY fd를 모두 닫아도 master EOF가 자식 종료와 함께 왔다. 그래서 EOF 뒤 자식이 살아 있는 구간을 시험으로 만들 수 없어, 이 구간의 시험은 macOS를 제외한 Unix에서만 돈다.

자식은 PTY를 소유한 host와 수명을 함께한다. Windows는 KILL_ON_JOB_CLOSE Job Object를 사용한다.
Unix는 비정상 host 종료 때 PTY hangup과 SIGHUP에 의존하므로 HUP을 무시하는 자식이 남을 수 있다.
정상 surface 닫기는 명시적인 종료와 회수를 수행한다. attach client만 끊는 것은 server의 자식을 종료하지 않는다.

Windows 절전 복귀는 WM_POWERBROADCAST로 감지한다.
종료한 자식을 정리하고, 살아 있는 ConPTY에는 현재 크기의 resize를 보내 처리를 재개하도록 유도한다.
복구되지 않을 수 있는 surface는 사용자에게 알린다. 멈춤과 정상 대기를 구분할 수 없으므로
자동 강제 종료·재생성은 하지 않으며 최종 재시작은 사용자가 결정한다.
macOS·Linux에는 이 Windows 전용 처리를 적용하지 않는다.

### 터미널 상태와 PTY 연결

`tasty-terminal` 안에서 터미널 내용과 OS 연결을 따로 소유한다. `Terminal`은 내용만 소유하며, host collection이 선택적인 `Pty`와 연결한다.

| 구성 | 소유하는 것 | 코드 |
|---|---|---|
| TerminalState | VT parser, grid, 프로그램 커서·모드, scrollback, 출력 버퍼, output·resize tap, 출력 sink | `lib.rs`, `vte_handler/`, `resize.rs` |
| Pty / PtyState | 자식 프로세스, PTY master, writer·reader worker, OS resize 예약·적용, 종료 관측, Drop의 종료·회수 | `pty.rs` |
| OutputSink | 사용자 입력과 VT 응답을 내보내는 현재 연결. local Pty writer 또는 mirror의 attach 입력 채널 | `sink.rs` |

- TerminalState는 Pty를 모르고 OutputSink로만 byte를 내보낸다. DSR/DA/OSC 조회 응답도 ingest 중에 같은 sink로 나간다. mirror에는 Pty가 없으며 sink는 attach 입력 채널이다.
- Pty는 VT를 모르고 읽은 raw 청크를 받는 쪽 계약(`PtyOutput`)으로 넘긴다. 같은 reader worker가 청크를 바로 ingest하지만 grid와 lock은 Terminal 쪽 소유다.
- Remote attach의 `Terminal::snapshot_and_stream(&mut self)`은 VT snapshot·cols/rows·구독 등록을 같은 parser lock 안에서 캡처한다. snapshot 뒤의 `Output`과 `Resize`는 하나의 `AttachEventReceiver`에 발생 순서대로 들어가므로 출력 청크와 크기 변경의 경계가 뒤섞이지 않는다.
- 이 구독은 최대 256개 event와 1 MiB 큐 바이트 예산을 함께 적용하며 producer를 기다리게 하지 않는다. 한도를 넘으면 이미 수락한 prefix 뒤에 `Loss`를 한 번 전달하고 `Disconnected`로 끝난다. Remote는 손실된 구독을 계속 표시하지 않고 원 registration을 끊어 새 snapshot으로 재연결한다. 구독 해제는 원본 PTY 종료가 아니다.
- 기존 `snapshot_and_tap`은 VT snapshot과 별도의 output·resize receiver를 사용하는 소비자를 위해 남아 있다. 한 lock에서 snapshot과 tap을 등록하지만 두 receiver 사이의 통합 순서·`Loss` 종료 계약은 새 ordered stream API의 계약이다.
- resize는 grid 변경과 resize tap 통지가 먼저다. OS resize는 Pty에 예약만 하고, collection의 다음 process 처리에서 강제 flush 또는 호스트가 대기 중인 resize를 순회하는 `Pty::flush_resize`(100ms throttle)에서 적용한다. 그래서 tap은 OS 적용 확인이 아니다.
- child의 kill·wait 소유자는 Pty 하나다. standalone에서 surface로 adopt해도 바뀌지 않으며 정상 Drop의 Unix 신호 전달 뒤 유예·회수는 메인 루프 밖에서 수행한다. reader/writer 준비가 실패해도 이미 생성한 child는 같은 owner가 정리한다.
- EngineSession의 기존 TerminalStore는 한 항목에 Terminal과 Option<Pty>를 함께 보관한다. Terminal 자체에는 child/master가 없다. 내용 조회는 Terminal만 빌리고 PID·OS cwd·resize는 Pty를 명시적으로 조회한다.
- resource generation은 현재 프로세스의 연결 신원으로 ContentEpoch와 독립이다. adopt는 세대를 유지하고 respawn은 교체한다. 로컬 Pty의 늦은 reader output·프로토콜 응답·resize 및 종료/제목/cwd 후속 요청은 현재 binding을 확인한다. durable journal activation과 연결된 세대는 아직 구현하지 않았다. mirror 응답은 기존 attach sink로 전달하며, 재연결 전 forwarder 큐의 바이트를 새 연결에서 배제하는 보장은 Remote connection epoch 이행에 남아 있다.

### 표시 위치의 소유와 읽기

`MainViewState.terminal_views`는 surface별 사용자 `TerminalViewport`를 소유한다.
local/client mirror 표시와 서버 readonly 표시는 별도 문맥이며, terminal resource를
복제하거나 보관하지 않는다. 화면 상태는 저장하지 않고 surface가 사라지면 회수한다.
headless의 `CommandContext`에는 viewport를 만들지 않는다. IPC 화면 읽기는 기존 live
내용 계약을 유지하며 본문·scrollback 수·alternate 진단을 같은 내용 cut에서 읽는다.
에이전트 입력·출력은 로컬 사용자 앵커를 변경하지 않는다.

renderer·선택·vi-copy·검색·링크·마우스 보고·surface screenshot은 같은 표시면의
viewport를 사용한다. `TerminalReadView`는 parser mutex 안에서 앵커와 content epoch,
행 좌표·치수·모드를 함께 읽는다. ED3 뒤 같은 청크에서 새 출력이 쌓여도 옛 앵커는
되살아나지 않는다. alternate는 live를 표시하고 primary 앵커를 보존한다. 1049 clear와
새 alternate buffer는 새 epoch를 사용하고 47/1047의 기존 buffer 복귀는 유지한다.

### VTE 에뮬레이션

termwiz가 VT 시퀀스를 파싱하고 셀 grid를 갱신한다. 주요 지원 범위는 다음과 같다.

| 종류 | 동작 |
|---|---|
| 텍스트와 제어코드 | LF, CR, BS, Bell. HT는 기본 8칸 간격의 탭 정지점을 사용한다. HTS/CTC로 설정하고 TBC로 해제한다. RIS와 리사이즈 때 기본값을 재구성한다. |
| 문자 속성 | 굵게, dim, 기울임, 반전, 전경·배경색을 그린다. dim은 배경과 50:50으로 섞어 그린다. 밑줄(단일·이중·물결·점선·파선), 밑줄 색(SGR 58), 깜빡임, 숨김, 취소선, 윗줄, SGR 73·74는 파서가 받아 셀에 저장하고 `debug.cell_info`로 조회할 수 있지만 그리지 않는다. 깜빡임 글자는 정적으로 보이고 숨김 글자는 그대로 보인다. |
| 커서 | CUP, CHA, VPA, CNL, CPL, CHT, CBT, 저장·복원. DECSCUSR의 블록·바·밑줄은 그리되 blink 변형도 정적으로 표시한다. |
| 화면 편집 | ED, EL, SU, SD, DCH, ICH, DL, IL, ECH, REP. DCH/ICH는 전각 두 셀을 함께 처리한다. DCH·ICH·삽입 모드는 밀리는 칸의 속성을 그대로 두고, 새로 생긴 칸은 [소거 규칙](#스크롤-영역과-소거)을 따른다. |
| ESC | DECSC, DECRC, IND, RI, NEL, HTS, DECALN, RIS. DECALN은 화면을 E로 채운다. |
| 문자 집합 | G0/G1 DEC line drawing과 SO/SI 전환. 활성 상태의 ASCII 0x60–0x7e를 박스 문자로 바꾼다. UK는 ASCII로 처리하고 RIS로 초기화한다. |
| 소프트 리셋 | DECSTR은 마진·저장 커서·문자 속성·앱 커서 키·삽입 모드·커서 가시성을 초기화한다. 화면과 대체 화면은 유지한다. |
| 표준 모드 | IRM(4) 삽입/덮어쓰기와 ShowCursor(25). KAM, SRM, LNM 등은 무시한다. |

지원하는 DEC 모드는 DECCKM(1), DECTCEM(25), 대체 화면(47/1047/1049),
마우스(1000/1002/1003/1006), 포커스 추적(1004), bracketed paste(2004), 동기화 출력(2026)이다.
DECSCNM(5)은 기본 전경·배경을 바꿔 그린다. `general.reverse_screen_enabled`를 끄면
모드 상태와 질의 응답은 유지하면서 화면 반전만 표시하지 않는다. readline의 visible bell도 이 설정으로 억제할 수 있다.
DECOM(6)은 절대 커서 위치를 스크롤 영역 기준으로 해석하고, 설정·해제 시 home으로 이동한다.
상대 커서 이동을 영역 안에 가두는 동작은 아직 구현하지 않았다.

### 터미널 질의와 OSC

| 시퀀스 | 응답·동작 |
|---|---|
| DSR/CPR | 상태·커서 위치 응답 |
| DA1 / DA2 / DA3 | `CSI ?1;2c` / `CSI >0;10;0c` / `DCS !\|54415354 ST` |
| XTVERSION | `DCS >\|tasty(<ver>) ST`; 버전은 tasty-terminal 크레이트 기준 |
| XtGetTcap | 요청별로 `DCS 0+r <hexcap> ST`. 능력 DB가 없어 현재 미지원임을 알리고 요청 hex를 그대로 돌려준다. |
| XTWINOPS | 셀 크기 `18 t`→`CSI 8;rows;cols t`, `19 t`→`CSI 9;rows;cols t`. 제목 push/pop `22/23 t`는 단일 제목과 최대 64개 항목을 사용한다. |
| OSC 8 | 이후 출력 셀에 하이퍼링크 URI를 붙이고 빈 URI로 해제한다. DECSTR/RIS도 속성을 초기화한다. |
| OSC 52 읽기 | host가 `general.allow_clipboard_read`를 확인한 뒤 응답한다. 기본 off이면 응답하지 않는다. [클립보드](../../features/clipboard/index.md) 참조. |
| OSC 10/11/12, OSC 4 질의 | 현재 테마의 전경·배경·커서·ANSI 색을 `rgb:RRRR/GGGG/BBBB`와 ST로 응답한다. host가 생성·테마 변경 시 팔레트를 전달한다. 커서는 전경색이며, 팔레트가 없으면 응답하지 않는다. 색 설정 시퀀스는 무시한다. |

창 이동·크기 변경·최대화·최소화·전체화면·앞뒤 순서 변경은 지원하지 않는다.
창 위치·상태·제목 조회와 픽셀 크기 조회(`14/16 t`)도 응답하지 않는다.
지원 범위와 보류 이유는 터미널 호환성 결정을 참고한다.

### 마우스 입력

트래킹 중 클릭·드래그·휠은 앱으로 보낸다. 1000/1002/1003은 각각 독립적으로 켜고 끄며,
동시에 켜져 있으면 가장 넓은 모드(1003, 1002, 1000 순서)를 적용한다.
RIS는 모두 끄고 DECSTR은 유지한다. SGR(1006)을 협상하지 않으면 X10으로 인코딩한다.
SGR 버튼 코드는 왼쪽 0, 가운데 1, 오른쪽 2이며 motion은 32를 더한다.
modifier는 Shift 4, Alt 8, Ctrl 16이다. 휠·버튼·debug 주입은 같은 인코더를 쓴다.

- 1002와 1003은 눌린 버튼의 드래그를 보고한다. 여러 버튼을 누르면 최근에 누른 버튼이 기준이다.
  그 버튼을 놓으면 아직 눌린 버튼을 사용한다. 앱에 press를 보낸 버튼만 motion을 보낸다.
- 드래그 대상은 press 시점에 고정하고 좌표를 그 surface 안으로 제한한다.
- 1003의 버튼 없는 hover는 `ESC[<35;col;rowM`이다. OS 창과 surface가 포커스된 경우에만 보내며
  셀이 달라졌을 때 전송한다. 중복 제거 키에는 surface ID도 포함한다.
  분할선·창 리사이즈 가장자리·분할선 드래그 중에는 보내지 않는다. 1002는 hover를 보내지 않는다.
- Shift+우클릭은 로컬 메뉴를 열고 Shift+좌클릭 드래그는 로컬 텍스트를 선택한다.
  Shift 여부는 press 때 결정해 release까지 유지하므로 도중에 Shift를 놓아도 선택이 이어진다.
  더블클릭은 단어, 트리플클릭은 줄을 선택하며 release도 앱에 보내지 않는다.
- 트래킹 중 Shift+좌클릭은 새 선택을 시작한다. 트래킹이 꺼졌을 때의 Shift+클릭은 기존 선택을 확장한다.
- macOS 미들클릭 보고는 구현되어 있으나 대상 앱의 paste 동작은 확인되지 않았다.

#### 클릭 커서 이동

`[general] click_to_move_cursor`(기본 `true`)가 켜져 있으면, 드래그 없이 Shift 없이 한 번 좌클릭해
놓았을 때 셸 커서를 클릭한 위치로 옮긴다. 이동은 방향키 입력을 PTY에 보내서 한다.

- 포커스 surface의 전면 프로세스가 셸일 때만 옮긴다. 셸이 아닌 프로그램은 자기 커서 규칙이 있어
  방향키 수로 위치를 맞출 수 없기 때문이다. 셸 판정 목록은 소스(`click_cursor::is_shell_process`)가 정한다.
- 스크롤백을 올려 본 상태, 대체 화면, 마우스 트래킹 중에는 옮기지 않는다.
- 편집 영역은 커서가 있는 줄과, 터미널 폭을 꽉 채워 이어지는 앞뒤 줄로 추정한다. 영역 위아래 한 줄까지의 클릭은
  영역 경계로 맞추고, 그보다 먼 클릭은 무시한다. 마지막 줄은 내용이 끝나는 칸까지만 옮긴다.
- 조합 중인 IME 문자열은 이동 전에 확정한다.
- 설정 창에는 이 항목이 없고 config.toml로만 끈다.

### 마우스 캡처 안내와 앱별 설정

`general.mouse_capture_hint`는 안내 배너를 켜고 끈다(기본 on).
옛 키 `right_click_capture_hint`도 설정 별칭으로 읽는다.
좌·우 일반 클릭 중 처음 발생한 캡처 입력에서 Shift 선택·메뉴 방법을 안내한다.
트래킹을 켤 때 표시 가능 상태가 되고, 한 번 표시하거나 트래킹을 끄거나 RIS를 받으면 해제한다.

| 설정 | 효과 |
|---|---|
| `general.mouse_capture_blacklist` | 해당 앱의 클릭·버튼·드래그를 로컬에서 처리한다. 휠은 계속 앱에 보내고 안내 배너는 표시하지 않는다. |
| `general.mouse_capture_banner_blacklist` | 앱의 마우스 입력은 유지하고 안내 배너만 숨긴다. |

두 목록의 기본값은 비어 있다. 프로세스 이름에서 `.exe`를 제거하고 소문자로 바꾼 뒤
substring 또는 `*` glob으로 비교한다. 1Hz foreground 조회 결과를 재사용한다.
배너만 숨긴 경우에는 표시 가능 상태를 소비하지 않으므로, 같은 세션에서 다른 앱으로 바뀌면 안내가 나올 수 있다.
원격 mirror는 로컬 foreground 프로세스가 없어 이 목록을 적용하지 않는다.

배너의 더보기 메뉴는 Settings와 같은 목록을 수정한다. "{app}에서 다시 보지 않기"(배너 목록)는 배너를 즉시 닫고,
"{app}에서 클릭은 Tasty가 처리"(캡처 목록)는 배너를 남겨 사용자가 안내를 읽고 닫을 수 있게 한다.
Settings의 두 목록 라벨도 같은 동사를 쓴다("캡처 안내를 다시 보지 않을 프로그램", "클릭을 Tasty가 처리할 프로그램").
문구와 행 구성은 [배너 디자인](../../design/systems/banner.md)의 더보기 메뉴 절에 있다.
메뉴가 보내는 이름은 trim 해 저장하며, trim 뒤 빈 이름이나 이미 있는 패턴(trim·ASCII 대소문자 무시 비교)과
같은 이름은 추가하지 않는다. 매칭이 빈 패턴을 건너뛰고 같은 패턴은 결과가 같기 때문이다.
Settings 화면의 직접 추가는 trim 뒤 빈 값만 거부하고 중복은 허용한다. 이미 저장된 빈 항목과 중복은 자동 정리하지 않는다.
foreground 이름이 바뀌거나 사라져 배너가 닫히면 그 배너에서 연 더보기 메뉴도 함께 닫는다. 메뉴가 클릭 시점의 이름을 저장하므로, 남겨 두면 다른 프로그램에 설정이 저장되기 때문이다.
더보기 버튼은 hover 중 또는 메뉴가 열린 동안 표시한다. 메뉴는 `PopupDef`의 headless 컨텍스트 메뉴로
트리거 아래에 우측 정렬하고, 공간이 없으면 위쪽에 연다. 두 행은 간격 없이 붙어 셸 높이가
위아래 여백과 두 행 높이의 합이다. 프로그램 이름은 별도 텍스트 구간(mono, 행과 같은 글자색)으로 그려
긴 번역문 때문에 이름부터 잘리지 않게 한다. 고정 문구가 메뉴 상한 폭(288)에 들어가지 않는 언어(ja)에서는
그 행만 줄을 바꾸고 셸이 그만큼 높아진다. 되돌릴 수 있는 설정이므로 위험 동작 색을 쓰지 않는다.
foreground 프로그램 이름을 모르면(원격 mirror, 첫 조회 전, 조회 실패) 더보기 버튼은 그대로 두고 메뉴를 이름 미상 형태로 연다.
저장할 이름을 알 수 없다는 사유 줄 아래에 "이 프로그램"으로 쓴 같은 두 행을 비활성으로 보이며, 눌러도 저장·닫기가 없다.
이 형태는 열 때 정해지며 열린 메뉴가 정상 행으로 바뀌지는 않는다. 로컬 surface 에서 메뉴가 열린 동안 이름이 확인되면 다음 1Hz foreground 확인에서 세대가 바뀌어 배너와 메뉴가 함께 닫히고, 원격 mirror 는 이름이 계속 없어 비활성 형태가 유지된다. 메뉴가 늘 있으므로 배너는 TTL 없이 남는다([배너 디자인](../../design/systems/banner.md#프로그램-이름을-모를-때)).

배너가 열린 때의 foreground generation을 저장하고, 프로그램 이름이 바뀌면 1Hz 조회에서 닫는다.
다른 종류의 배너를 닫지 않도록 ID를 확인한다. 같은 배너 종류라도 generation이 다르면 교체한다.
같은 이름의 프로그램이 연속 실행된 경우와 원격 mirror에서는 이 방법으로 종료를 구별할 수 없다.

### 스크롤 영역과 소거

DECSTBM 영역은 명시 개행과 오른쪽 끝의 자동 줄바꿈 모두에 적용한다.
영역 하단에서 줄바꿈하면 그 영역만 위로 밀고 커서는 하단 열 0에 남는다.
영역 밖에 둔 TUI 입력창은 덮지 않는다.

- 영역 상단이 화면 0행이면 밀려난 행을 scrollback에 순서대로 보관한다.
  내부 영역은 화면 밖으로 나가는 행이 없으므로 scrollback에 넣지 않는다.
- 커서가 영역 아래의 마지막 화면 행에 있으면 화면과 영역을 스크롤하지 않고 같은 행에서 줄바꿈한다.
- 대체 화면도 같은 영역 규칙을 쓰지만 primary scrollback에 기록하지 않는다.
- 마진은 저장할 때 grid 안으로 정규화한다. 하단은 마지막 행까지 제한하고,
  상단이 하단보다 크거나 화면 밖이면 한 줄 영역으로 정리한다.
- DECAWM으로 자동 줄바꿈을 끄는 기능은 현재 미지원이다. 꺼 달라는 요청에도 위 줄바꿈은 일어난다.

ED/EL은 커서 위치를 유지하며 현재 셀을 포함해 지운다.
마지막 셀까지 출력한 뒤 줄바꿈을 기다리는 커서는 `cx == cols`로 표현되지만,
지울 범위는 실제 마지막 셀에서 계산한다. 소거 뒤에도 줄바꿈 대기는 유지한다.

| 명령 | 줄바꿈 대기 커서에서 지울 범위 | 0열의 경계 사례 |
|---|---|---|
| EL0 | 마지막 한 셀 | 커서부터 행 끝 |
| EL1 | 현재 행 전체 | 한 셀 |
| EL2 | 현재 행 전체 | 행 전체 |
| ED0 | 마지막 셀과 아래 행 전체 | 커서부터 화면 끝 |
| ED1 | 현재 행과 위 행 전체 | 커서 행 한 셀과 위 행 전체 |
| ED2 | 화면 전체 | 화면 전체 |

지운 셀은 기본 문자 속성과 소거 당시 배경색만 갖는다. 밑줄·역상·굵기 등은 복사하지 않는다.
소거 뒤 pen은 이전 값으로 복원해 다음 문자가 원래 속성으로 출력되게 한다.
이는 `TERM=xterm-256color`의 BCE 선언에 맞춘 동작이다.

소거 구현은 `vte_handler/edit.rs`의 erase_attrs·erase_color·restore_pen을 사용한다.
검증은 여섯 명령, 커서 행, 줄바꿈 대기 유무를 조합하고 지운 셀과 소거 직후 문자의 속성을 각각 확인한다.
기본 배경과 밑줄을 함께 시험해야 배경만 복사한다는 조건을 확인할 수 있다.
줄바꿈 대기의 EL0/ED0은 과거 tmux 3.4 비교와 다르다. Tasty는 마지막 셀을 지운다.

파싱은 터미널별 reader 스레드에서 수행하고 main 루프는 렌더링과 이벤트만 처리한다.
공유 grid는 8KB 청크 처리 후 락을 놓는다. 렌더링은 필요한 값을 한 번의 잠금에서 읽고, 잠금 밖으로 grid 참조를 반환하지 않는다.
새 접근자도 상태 잠금을 거치며 mutex가 poison 상태이면 내부 값을 꺼내 복구한다. 백그라운드 터미널의 잠금은 사용자가 보는 터미널의 잠금과 독립이다.
EOF 뒤 계속 실행하는 자식이 있으면 해당 터미널만 처리하는 targeted polling으로 깨우지만, GUI에서 보이는 surface는 그 창을 다시 그릴 수 있다. targeted polling을 끄면 전체 터미널을 처리하고 모든 창을 다시 그릴 수 있다. Terminal을 버릴 때 parser 종료를 기다리지 않는다(최대 한 대기 간격 뒤 종료).
Windows에서는 호스트 종료 시 `nohup`·백그라운드 작업도 종료되고 Job 등록 실패는 경고를 남긴 뒤 기존 정리 방법을 쓴다. 플러그인과 터미널은 종료 구현을 공유하되 Job Object는 각 소유자가 따로 보관한다.
선택 이유는 PTY 처리 결정을 참고한다.

### 스크롤백

화면 위로 밀린 줄을 `VecDeque` 에 보관(`scrollback_lines`, 기본 10,000, 0~100,000 설정). 마우스 휠/PageUp·Down 탐색, 타이핑 시 자동 라이브 뷰 복귀. 대체 화면(vim/less/htop)에선 스크롤백 비활성(모든 입력 PTY 로). 스크롤백 중 새 출력이 도착해도 창의 표시면별 앵커로 읽던 행을 유지한다. 앞쪽 기록이 삭제되면 남은 첫 행으로 보정한다. 텍스트 wrap 에 의한 implicit 스크롤도 기록한다 — 출력 텍스트를 스크롤이 일어날 바이트 위치에서 잘라 적용하며 밀려나기 직전의 상단 행을 회수하므로, 사라진 행이 빠짐없이 순서대로 남는다(선택 영역 `absolute_row` 가 콘텐츠를 정확히 추적). 부분 스크롤 영역에서의 적재 범위는 위 "스크롤 영역과 소거" 절 참조. 세션 간 보존은 disk scrollback. **ED3(`CSI 3J`)** 는 스크롤백 히스토리(메모리+디스크)를 비우고 뷰포트를 라이브로 되돌린다 — 화면 내용은 보존(`clear` 가 보내는 `\x1b[3J\x1b[2J` 에서 ED2 가 화면을, ED3 가 스크롤백을 담당).

**디스크 스왑**(`[performance] scrollback_disk_swap`)이 켜져 있으면 메모리 한도(`scrollback_lines`)를 넘은 줄을 터미널마다 따로 만든 임시 파일로 옮긴다. 파일을 만들 때 연 쓰기 핸들을 터미널이 사라질 때까지 유지하므로 줄을 옮길 때 파일을 다시 열지 않는다. 여러 줄이 한꺼번에 들어오는 경우(함께 밀려난 줄, 닫은 항목·레이아웃 복원의 history 주입, 리사이즈, 한도 축소)에는 넘친 줄 전체를 한 묶음으로 기록한다. 쓰기가 실패하면 그 묶음의 줄을 모두 버린다. 디스크의 줄 수·오프셋·파일 길이는 묶음 이전 값으로 되돌리고 일부만 기록된 꼬리는 잘라 낸다. 버린 줄 때문에 행 좌표가 다른 줄을 가리키지 않도록 콘텐츠 epoch 을 새로 만든다. 터미널이 사라질 때는 쓰기 핸들을 먼저 닫고 파일을 지운다(Windows 는 열린 파일을 지우지 못한다). 프로세스가 이 정리 없이 끝나면(SIGKILL·SIGTERM·크래시·전원 차단) 파일이 남는다. 파일 이름은 `surface-<pid>-<sid>-<nonce>.scrollback`이고, 다음에 GUI나 headless Tasty가 시작할 때 백그라운드 스레드(`scrollback-reclaim`)가 이름의 PID로 남은 파일을 회수한다(`tasty_terminal::disk_scrollback::reclaim_orphans`). 회수 범위는 다음과 같다.

- `scrollback_disk_swap`을 끈 실행에서도 회수한다. 대상은 임시 폴더의 `tasty-scrollback-debug`와 `tasty-scrollback` 두 폴더로, 빌드 프로필과 관계없다.
- 소유 프로세스 생존은 "끝남 / 살아 있음 / 알 수 없음"으로 판정하고 끝남만 지운다. Unix는 `kill(pid, 0)`의 ESRCH가 끝남, 성공·EPERM이 살아 있음이다. Windows는 `OpenProcess`가 ERROR_INVALID_PARAMETER로 실패하거나 종료 코드가 STILL_ACTIVE가 아니면 끝남이고, 접근 거부 등 그 밖의 실패는 알 수 없음이다. 그 밖의 OS는 모두 알 수 없음이다. 자기 PID의 파일은 지우지 않는다.
- 이름 형식이 다른 파일, 같은 이름의 폴더·심볼릭 링크는 건드리지 않는다.
- 끝난 프로세스의 PID를 다른 프로세스가 이미 다시 쓰고 있으면 살아 있음으로 판정돼 그 파일은 그 PID가 끝난 뒤의 실행까지 남는다. 다음 실행 전까지는 남은 파일이 디스크를 차지한다.

**디스크 보관 상한**(`[performance] scrollback_disk_lines`, 기본 10,000)은 디스크에 남기는 줄 수다. 메모리 한도와 별도이므로 터미널 하나가 보관하는 history 는 최대 `scrollback_lines + scrollback_disk_lines` 줄이다. 값의 규칙은 다음과 같다.

- `-1` 은 무제한이다. 디스크 줄 수·파일 크기·오프셋 표가 ED3 전까지 계속 늘어난다.
- `0` 이상은 그 줄 수까지 남긴다. `0` 이면 디스크에 줄을 남기지 않아 디스크 스왑을 끈 것과 같은 history 가 된다.
- `-1` 보다 작은 값은 설정을 읽을 때 경고를 남기고 기본값 10,000 으로 바꾼다. 설정 화면의 입력 범위는 `-1`~100,000,000 이다.
- 값은 새로 만드는 터미널에 적용한다. 실행 중인 터미널은 만들 때의 값을 유지한다(디스크 스왑 설정과 같다).

상한을 넘은 줄은 메모리 한도의 앞쪽 버리기와 같은 의미로 가장 오래된 줄부터 버린다. 남은 줄의 행 좌표는 바뀌지 않고 `first_row` 가 버린 줄 수만큼 늘어나며 콘텐츠 epoch 은 유지한다. 따라서 앵커·선택·검색 결과의 행 번호는 메모리 경로와 같은 규칙으로 남은 첫 행에 보정되고, 행 읽기·구간 읽기·검색·선택 복사·캡처·`scrollback_len` 은 남은 줄만 다룬다. 한 묶음이 상한보다 크면 버릴 줄은 파일에 쓰지 않는다. 버린 줄의 바이트는 파일 앞쪽에 남다가 `max(상한, 4096)` 줄만큼 쌓이면 남은 줄을 같은 폴더의 새 인스턴스 파일로 복사하고 옛 파일을 지운다(compaction). 이때 오프셋 표의 용량도 돌려준다. 그래서 파일과 오프셋 표는 최대 `상한 + max(상한, 4096)` 줄이다. compaction 은 터미널 상태 잠금 안에서 남은 줄(최대 상한만큼)을 동기로 복사하므로, 상한을 크게 둘수록 한 번의 복사가 길어진다(별도 스레드나 분할 복사는 없다).

compaction 이 실패하면(파일이 외부에서 지워짐, 디스크 가득 참 등) 디스크에 남은 줄을 모두 가장 오래된 줄부터 버린 것으로 처리해 상한을 지킨다. 실패한 compaction 을 그대로 두고 다시 시도하면 버린 줄과 오프셋이 상한 없이 쌓이기 때문이다. 버린 줄은 앞쪽 버리기와 같은 규칙을 따른다(`first_row` 가 그 수만큼 늘고 콘텐츠 epoch·메모리 줄·남은 행 좌표는 그대로). 파일은 다음처럼 다시 시작한다.

- 경로의 파일이 없으면 같은 폴더(없으면 다시 만든다)에 빈 인스턴스 파일을 새로 만들어 이어 쓴다. 이후 줄은 다시 디스크에서 읽힌다.
- 파일이 있거나 새 파일을 만들지 못하면 쓰기 핸들로 파일을 헤더 길이로 자른다. 지워진 파일(inode)이 차지하던 공간도 이때 돌려준다.
- 둘 다 실패해도 오프셋과 길이는 헤더부터 다시 시작하고 다음 쓰기가 헤더 뒤부터 덮어쓴다.

기록을 잃은 일은 처음 한 번만 경고로 남기고, 같은 실패가 이어지는 동안에는 debug 로그만 남긴다. compaction 이 다시 성공하면 다음 실패를 다시 한 번 경고한다. 다시 시도는 버린 줄이 다시 `max(상한, 4096)` 줄만큼 쌓였을 때만 하므로 줄마다 임시 파일을 만들지 않는다. 디스크 쓰기(append) 자체가 실패하는 경우는 위 디스크 스왑 절의 묶음 단위 처리를 따른다. 직렬화 크기 하한(`scrollback_serialized_len`)의 디스크 기여분은 남은 첫 레코드부터 파일 끝까지의 바이트다. ED3 는 상한과 관계없이 디스크를 헤더만 남기고 비운다.

터미널이 살아 있는 동안 임시 파일이 외부에서 지워지면(예: 임시 폴더 정리) 쓰기는 오류 없이 지워진 파일(inode)에 계속된다. 그 사용량은 경로로 보이지 않는다. 디스크 보관 상한이 있으면 다음 compaction 이 실패할 때 위 절대로 새 파일로 옮겨 가므로 지워진 파일에 쌓이는 양도 상한 + compaction 기준 이내다. 무제한(`-1`)이면 compaction 이 없어 터미널이 닫힐 때까지 지워진 파일에 계속 쓴다. 경로로 파일을 여는 디스크 구간 읽기는 실패하므로 디스크 쪽 행은 복사·렌더에서 빈 행이 되고 검색·캡처에서는 빠진다. 메모리에 남은 최근 행은 그대로 읽힌다. 디스크 구간 읽기는 [레이아웃 보존](../../features/layout-persistence/index.md)의 순차 읽기를 따른다.

### 키보드 입력

**앱별 Shift+Enter**: 설정 → 터미널 → 입력에서 앱을 추가하고 LF 전송을 켜거나 끄며 규칙을 삭제한다. 저장 모델은 `Settings.terminal_input.rules`의 `app`(실행 파일명)과 `shift_enter_newline`이다. 값이 true인 앱에서 Shift만 누른 Enter는 LF 한 바이트를 보낸다. false 또는 미일치는 아래 플랫폼 기본 인코딩을 쓴다. basename의 대소문자와 `.exe`를 정규화한 정확한 이름으로 매칭하며 부분 문자열이나 `node` 런처를 Claude로 추정하지 않는다. foreground는 기존 약 1초 주기 프로세스 캐시를 재사용하므로 앱 전환 직후에는 다음 갱신까지 지연될 수 있다. 붙여넣기·IME·IPC 텍스트와 호스트에서 소비한 키는 이 규칙을 거치지 않는다.

CLI는 `tasty settings get-input-rules`, `set-input-rule --app claude --shift-enter-newline true`, `remove-input-rule --app claude`이다. 해당 IPC는 `settings.get_input_rules`, `settings.set_input_rule`, `settings.remove_input_rule`이며 전역 설정 저장·전 윈도우 반영 경로를 공유한다. 응답은 실행 시점의 설정에 적용하고 저장한 뒤에 오며, 저장 실패는 오류 응답이다. `settings.initialize_input_rule`은 호출자와 앱별 최초 기본값만 등록한다. 기존 사용자 규칙은 우선하며 최초 등록 이력은 삭제 후에도 남아 재시작으로 규칙을 복구하지 않는다. Claude 플러그인은 최초 활성화에서 `claude`의 LF 규칙을 등록한다(이미 설치된 플러그인도 새 버전의 최초 활성화에서 등록).

**Windows 줄바꿈 키**: Shift+Enter와 Ctrl+J는 ConPTY의 win32-input 시퀀스(`CSI Vk;Sc;Uc;Kd;Cs;Rc _`)로 key-down/up 한 쌍을 보낸다. Shift+Enter는 VK_RETURN·CR·SHIFT_PRESSED, Ctrl+J는 VK_J·LF·LEFT_CTRL_PRESSED를 보존한다. CSI-u는 ConPTY에서 소실되고, bare LF는 VK_RETURN으로 변환되어 Windows 네이티브 콘솔 앱이 Ctrl+J와 구분하지 못하기 때문이다. 보조키 좌우는 구분하지 않는다. 일반 Enter와 다른 Ctrl+문자는 기존 제어문자를 보내고, Unix PTY에는 Shift+Enter의 CSI-u와 Ctrl+J의 LF를 유지한다. 별도 설정은 없다. 이 선택은 Tasty를 빌드한 OS가 아니라 입력을 읽는 PTY를 따른다(`Terminal::win32_console_input`). 로컬 터미널은 실행 중인 OS를 따르고, attach mirror는 서버가 알린 PTY 형식을 따른다. 그래서 Windows에서 Linux 서버에 붙인 mirror는 CSI-u를, Linux·macOS에서 Windows 서버에 붙인 mirror는 win32-input 시퀀스를 보낸다([Attach 메커니즘](../../dev-guide/attach-behavior.md#키-인코딩은-서버-pty를-따른다)). 프로토콜 필드는 [Microsoft Terminal 구현](https://github.com/microsoft/terminal/blob/main/src/terminal/input/terminalInput.cpp)의 `_makeWin32Output`을 따른다. 인코딩 회귀는 `src/view/main/keyboard.rs`의 `newline_keys_*` 시험이 두 PTY 형식을 모두 검사하며 실제 콘솔 수신은 Windows에서 별도로 검증한다.

중앙 키보드 디스패처가 focused surface 타입에 따라 정확히 한 대상에만 전달 — Terminal 은 PTY 로 바이트. 특수 키(Enter/Backspace/Tab/Escape/방향키/Home·End/PageUp·Down/Insert·Delete/F1~F12) 매핑, DECCKM 모드에 따라 방향키 시퀀스 전환(`\x1b[{A..D}` ↔ `\x1bO{A..D}`). 복사/붙여넣기/선택/IME 는 [clipboard](../../features/clipboard/index.md).

방향키와 함께 누른 Shift·물리 Alt(macOS Option)·Ctrl은 xterm 방식 `CSI 1 ; m A/B/C/D`로 전달한다. `m`은 1 + Shift(1) + Alt(2) + Ctrl(4)이며, 보조키가 있으면 DECCKM on/off 모두 CSI를 쓴다. 예: Option/Alt+↑는 `\x1b[1;3A`, Ctrl+Shift+←는 `\x1b[1;6D`. 보조키 없는 방향키는 위 DECCKM 규칙을 유지한다. 근거: [xterm cursor-key 및 modifier 표](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys)와 [modifyCursorKeys](https://invisible-island.net/xterm/manpage/xterm.html#VT100-Widget-Resources:modifyCursorKeys).

이 인코딩은 `option_as_meta`와 독립이다(그 설정은 문자 입력만 제어). 호스트 바인딩의 `alt` 토큰(macOS Command)으로 물리 Option을 치환하지 않으며, Super/Command 자체는 방향키 modifier 값에 넣지 않는다. 호스트 단축키·오버레이·vi 복사 모드가 소비한 키는 PTY로 중복 전달하지 않는다. 인코딩 결정은 `src/view/main/keyboard.rs`의 `decide_key_to_terminal`이 담당하고, 문자 Meta/compose와 IME Commit 경로는 별도로 처리한다.

**Option as Meta (macOS 전용)**: 설정 `general.option_as_meta`(기본 off, macOS 빌드에만 존재). on 이면 Option(Alt)+문자 입력이 특수문자(compose, 예 `å`) 대신 `ESC` + base 문자 Meta 시퀀스(예 `Option+a` → `\x1b a`)로 PTY 에 전달돼 readline/Emacs/vim 의 Meta 바인딩(`Alt+f`/`Alt+b` 등)이 동작한다. 물리 Option 키만 대상이며(Ctrl/Cmd 동시 누름 시 제외), 좌/우 Option 을 구분하지 않는다. base 문자는 Option 합성 이전의 US 레이아웃 문자(`physical_key_to_logical`)라 비-US 레이아웃에서는 US 기준으로 인코딩되는 한계가 있다. off 면 기존 Option=특수문자 동작을 보존한다. 키바인딩이 아니라 입력 인코딩 동작이라 `GeneralSettings` 에 둔다(`KeybindingSettings` 무관). 다른 OS 에는 Option 키가 없어 설정·UI 모두 노출하지 않는다.

### 색상 / 폰트

xterm-256color(ANSI 16 + 216 큐브 + 24 그레이) + TrueColor. 색은 Theme 의 ansi 팔레트([theme](../../design/systems/theme.md)). 폰트는 번들 D2Coding ligature(OFL 1.1, 임베드 — OS 미설치에도 동작), CJK fallback, 블록/박스 드로잉 글리프는 픽셀 퍼펙트 커스텀 렌더. 번들 폰트 파일은 합자 글리프를 포함하지만(폰트 자원), tasty 는 셀-격자 cross-cell 합자(프로그래밍 ligature) 적용도 설정 토글도 **미지원** — 보류 결정은 이 문서. 렌더 파이프라인(누적→flush→단일 패스, atlas LRU)은 [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md).

바탕은 포커스 바탕 쌍을 쓰고 값은 [포커스 표현](../../design/policies/focus.md#surface-포커스-표현)의 판정을 따른다. 찾기의 현재 매치 글자도 같은 판정을 따라 비포커스 때는 `Theme::search_match_active_unfocused_fg()`를 쓴다. 일반 매치 안의 기본 글자 셀은 `text-primary`로 읽는다. 기본 글자 셀은 프로그램이 전경색을 고르지 않았고 반전 표시가 아닌 셀이다. 값이 기본색과 같아도 SGR로 고른 색은 셀의 색을 유지한다. bold와 dim은 기본 글자 셀이고 두 바탕에서 같다. DECSCNM 반전 화면과 블록 커서·vi 커서·링크가 글자색을 정한 칸은 셀 글자를 유지한다.

### 이벤트 드리븐 렌더

파서 스레드가 ingest 직후 `AppEvent::TerminalOutput` 으로 메인 루프를 깨운다(`Waker`). 메인은 파싱이 아니라 변경된 grid 의 렌더·이벤트 수집만. 무조건 redraw 제거 — 실제 변경 시에만. **가시성 게이트**: 안 보이는 surface(비활성 워크스페이스/탭)는 출력을 항상 drain·파싱하되 `request_redraw` 는 생략(데이터 무손실, 재렌더만 절약). 출력·입력이 없을 때 불필요한 렌더링을 줄인다.

Windows 에서는 focused terminal cursor 를 프로그램 주도 화면 갱신 뒤 짧게 숨긴다. VTE 파서가 carriage return, CSI cursor/edit, cursor save/restore action 을 실제로 본 뒤 120ms 동안 적용한다. Codex 같은 redraw-heavy CLI 가 커서 이동/줄 지우기를 빠르게 내보낼 때 중간 cursor hop 을 화면에 드러내지 않기 위한 렌더 정책이다. 단순 printable echo 는 화면 제어 action 이 아니므로 사용자가 직접 타이핑하는 동안 커서를 숨기지 않으며, 입력 직후라도 프로그램이 화면 제어 action 을 출력하면 억제를 적용한다. 커서를 숨긴 프레임은 후속 redraw 를 예약해 화면 갱신이 조용해진 뒤 커서가 다시 나타나게 한다. 다른 OS 에서는 이 억제 정책을 적용하지 않는다.

## 인터페이스

- **사용자**: 키보드/마우스 직접 입력.
- **AI Agent**: 입력 주입·출력 읽기는 [terminal-output](../../features/terminal-output/index.md) + `surface.send*` ([reference/api](../../reference/api.md#surface-상호작용)). 터미널 surface 생성/닫기는 [work-area](../../features/work-area/index.md).
- **원격**: PTY 없는 detached mirror 로 grid 재구성 ([remote-attach](../../features/remote-attach/index.md)).

## 비-목표

- **인라인 그래픽**(Sixel/Kitty/iTerm 이미지) — 보류(이 문서). 이미지는 [image surface](../image/index.md).
- **XTWINOPS 창 조작·창 탐침·픽셀 크기 리포트** — 미지원(이 문서, 사용자/에이전트 분리).
- **tmux control mode(DCS)·DECRQSS** — 미지원(이 문서, 범위 밖/드묾).
- **일부 사설 입력 모드** — Utf8Mouse(1005), SGRPixels(1016), Win32InputMode(9001), DECCOLM(3), ReverseWraparound(45), Meta/AltSendsEscape(1036/1039), GraphemeClustering(2027)은 지원하지 않는다. 표준 입력 방식으로 대신할 수 있어 구현을 보류했다(이 문서).
- 렌더/폰트 atlas 내부 구현 — [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md).

## 관련

- [terminal-search](../../features/terminal-search/index.md) · [terminal-link](../../features/terminal-link/index.md) · [clipboard](../../features/clipboard/index.md)
- 이 문서 · [dev-guide/gpu-rendering](../../dev-guide/gpu-rendering.md)

## 휠 스크롤 거리

`general.wheel_line_scroll`은 휠 한 칸을 논리 포인트로 바꾸는 값이며 기본값은 50이다.
설정은 egui `Options::line_scroll_speed`에 적용한다. host 위젯, modifier hint,
plugin surface·popup·banner와 attach mesh가 모두 이 값을 읽는다.
plugin에 보내는 Scroll도 논리 포인트이므로 재현 로그에서 50을 상수로 가정하지 않는다.
터미널 앱으로 전달하는 휠 보고처럼 단위를 그대로 전달하는 경로는 이 포인트 환산과 구분한다.

검증은 기본값이 아닌 값을 egui 컨텍스트에 넣고 host·와이어 변환·overlay에 동일하게 적용되는지 본다.
새 LineDelta 처리 경로가 생기면 자체 상수를 추가하지 않았는지도 확인한다.
실행 상태 표시는 [busy 정책](../../design/policies/busy-indicator.md)을 따른다.

## 사용자 입력 기록

키보드·IME·붙여넣기는 `CoreState::record_typing`으로 기록한다.
`surface.is_typing`은 최근 5초의 입력 여부와 마지막 입력 후 `idle_seconds`를 반환하며,
기록이 없으면 `idle_seconds`는 -1이다. 에이전트 send/tell은 기록하지 않는다.

붙여넣기는 `MainView::run_paste` 시작에서 기록한다. 단축키·명령 팔레트·이미지·plugin 붙여넣기가
같은 함수를 사용한다. 빈 클립보드나 붙여넣기 미지원 surface에서도 시도는 기록한다.
원격 이미지 업로드 완료 시점에 다시 기록하지 않으므로 긴 업로드 뒤의 typing은 이미 만료될 수 있다.
마우스 보고·휠·클릭 커서 이동과 새 탭을 여는 파일 드롭은 이 기록의 대상이 아니다.

`--wait-idle` 전송과 자동 재개는 이 기록으로 사용자의 입력을 보호한다.
새 붙여넣기 경로는 `run_paste`를 사용하며, 명령 팔레트 회귀 검증은
`tests/gui_tests.rs`의 `test_palette_paste_records_user_typing`으로 수행한다.

publication 동안 surface별 출력 깨움을 하나의 전체 drain으로 합칠 때, GUI·headless는 전체 drain 직전에 default와 모든 surface dedup 표지를 함께 푼다. 원 PTY reader가 보관한 콜백은 이후 출력과 EOF를 다시 알릴 수 있다. 전체 drain은 대상 자원이나 실행 세대를 바꾸지 않는다.
