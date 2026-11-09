# Busy Indicator (실행 중 표시)

탭·워크스페이스에서 작업이 실행 중인지 표시한다. [사용자 포커스](focus.md)와 무관하게 판단한다.

## 판정 — 해제 두 조건 · 진입 조건 하나

busy는 이전에 확인한 상태를 기억한다. 입력 직후 에코만 있으면 idle을 유지하지만, 이미 작업 중인 프로그램은 입력만으로 idle이 되지 않는다([터미널 (Terminal)](../../surfaces/terminal/index.md)).

터미널 surface를 다음 순서로 판단한다.

1. PTY 전경 프로세스가 셸 자체이거나 알려진 셸 이름(`is_known_shell_name`)이면 idle.
2. 마지막 활동 출력이 `BUSY_OUTPUT_WINDOW`(2초)보다 오래됐으면 idle.
3. 같은 전경 프로세스의 직전 판정이 busy가 아니고, 마지막 활동 출력이 입력 에코 구간(`last_input_at <= last_activity_at <= last_input_at + INPUT_ECHO_WINDOW`, 200ms)에 있으면 idle.
4. 나머지는 busy.

**활동 출력**은 터미널 제목만 바꾸는 시퀀스 말고 다른 시퀀스나 글자가 하나라도 든 PTY 출력이다. 제목 시퀀스는 OSC 0·1·2, Sun 형식 OSC `l`·`L`, XTWINOPS 제목 스택 push·pop(`CSI 22;Ps t`·`CSI 23;Ps t`)이다. 문자열 종결자(ST, `ESC \`)는 어떤 문자열을 끝내든 항상 뺀다. ST 로 끝나는 제목이 아닌 문자열(OSC 8·52·133, DCS 등)은 그 문자열의 action 이 따로 활동으로 세어진다. termwiz 가 action 으로 내지 않는 APC·PM·SOS 문자열(kitty 그래픽 APC 는 제외)은 ST 만 남으므로 활동이 아니다. 판정은 바이트 수나 읽기 단위가 아니라 파싱된 action 종류로 하므로, 제목 OSC 가 두 번에 나뉘어 와도 결과가 같다. 제목 시퀀스와 다른 시퀀스(동기화 출력 `CSI ?2026 h/l`, 커서 이동, 질의 등)가 함께 오면 활동이다. 제목 변경 자체(탭 제목 갱신)는 그대로 처리한다.

활동 출력 시각(`last_activity_at`)은 busy 판정만 쓴다. idle-timeout 훅과 자식 Claude·Codex 출력 침묵 판정이 읽는 `last_output_at` 은 제목만 있는 출력을 포함한 모든 출력에 갱신한다. 그래서 hook 보고 없이 입력을 기다리며 제목만 바꾸는 자식의 상태는 `active` 그대로지만, 근거가 `pty_busy`(확신도 `confirmed`)에서 `recent_output`(확신도 `heuristic`)으로 바뀐다.

busy를 해제하는 조건은 (1)과 (2)다. 입력은 해제 조건이 아니다. (3)은 입력 이후의 출력만 에코로 보므로 마지막 입력보다 앞선 출력에는 적용하지 않는다.

`Terminal`은 마지막으로 busy로 판단한 전경 PID를 기억한다.

- (1)·(2)로 idle이 되면 기록을 지운다. 같은 프로그램을 `fg`로 다시 실행해도 이전 busy 상태를 물려받지 않는다.
- 전경 PID가 바뀌면 새 프로그램에 이전 기록을 적용하지 않는다.
- 파서의 입력 처리로 상태 락이 경합하면 해당 조회에서는 busy로 추정한다([터미널 (Terminal)](../../surfaces/terminal/index.md)). 이 추정으로 기록을 만들거나 지우지는 않는다.
- 기록은 busy 판정 때만 갱신하므로 같은 상태를 연달아 조회해도 답이 같다.

| 상황 | 판정 |
|---|---|
| 셸이 프롬프트에서 대기 | (1)에 따라 idle |
| `vim` 화면이 멈춰 있거나 `claude`가 입력 대기 | (2)에 따라 idle |
| 입력을 기다리며 제목만 약 1초마다 바꾸는 프로그램(Codex의 `Action Required`) | 제목만 있는 출력은 활동이 아니므로 (2)에 따라 idle |
| idle인 `vim`·`claude`에 타이핑 시작 | 에코만 발생하면 (3)에 따라 idle |
| `claude` 응답 중 다음 질문 입력 | 직전이 busy여서 (3)을 적용하지 않음. 출력이 멈춘 뒤 2초가 지나면 idle |
| `cargo build` 또는 `claude`가 계속 출력 | busy |
| idle인 마우스 트래킹 TUI에서 마우스 이동 | 마우스 보고도 입력이므로 (3)에 따라 idle. 직전이 busy라면 출력이 이어지는 동안 busy 유지 |
| DSR(`ESC[6n`)·DA·OSC 색상 질의를 반복하면서 출력하는 TUI | 터미널의 질의 응답은 입력이 아니므로 busy |

200ms보다 짧은 간격으로 계속 타이핑하면 그동안의 출력도 에코 구간에 들어가 busy 전환이 늦어질 수 있다. 타이핑이 멈춘 뒤 다음 폴링에서 다시 판단한다. 비터미널 surface(Markdown/Explorer/Image)는 항상 idle이다. 단, plugin surface는 소유 plugin이 `surface.set_busy`로 알린 값을 쓴다 — 아래 (P). 터미널도 같은 입구로 알린 값을 위 판정과 합친다 — 아래 (R).

### (3) 의 억제 창을 갱신하는 write / 갱신하지 않는 write

PTY로 보내는 데이터는 같은 채널을 사용하지만, `last_input_at`은 터미널 바깥에서 들어온 입력만 갱신한다.

| 전송 종류 | 입력 시각 갱신 | 경로 |
|-------|-------------|------|
| 키 입력 / `send_key` / `send_bytes` (에이전트 IPC 포함) | 갱신 | `TerminalState::write_input` |
| 마우스 보고(`encode_mouse_report`) | 갱신 | 위와 동일 |
| 터미널 자체 응답 — DSR(커서 위치/상태) · DA/DA2 · DECRPTUI · XTVERSION · OSC 4/10/11 색상 · XTWINOPS 18/19 | 갱신하지 않음 | `TerminalState::send_terminal_response` |

터미널 자체 응답까지 입력으로 기록하면 200ms보다 짧게 질의를 반복하는 TUI가 계속 에코 구간에 머무른다. 출력이 있어도 `busy == false`가 되는 오류를 피하려고 두 경우를 구분한다. 실제 전송 경로와 `enqueued_count`·`WriteAck` 규칙은 같다.

### (P) plugin surface

plugin이 그리는 surface(webview 등)에는 전경 프로세스가 없어 호스트가 판단하지 못한다. 소유 plugin이 `surface.set_busy { surface_id, busy }`로 알린 값을 쓴다.

- 호출자는 그 surface를 소유한 plugin이어야 한다. 다른 호출자는 `-32000`으로 거절한다. 터미널도 plugin도 아닌 surface(Markdown 등)도 거절한다.
- 값은 폴링이 다시 계산하는 터미널 busy와 따로 보관하고, 판정은 둘의 합집합이다. 폴링이 plugin 값을 지우지 않는다.
- surface가 닫히면 값도 지운다. 턴 도중 닫힌 탭이 워크스페이스를 busy로 남기지 않는다.
- plugin이 내려가며 끄지 못한 값은 surface가 닫힐 때까지 남는다.

### (R) 터미널에 알린 busy

터미널의 판정은 PTY 출력으로 한다. 그래서 출력 없이 일하는 프로그램은 2초 뒤 idle로 보인다. 응답을 기다리는 에이전트가 그렇다. 그 프로그램이나 그 훅이 같은 입구 `surface.set_busy`(CLI `tasty surface set-busy [--surface <id>] --busy true|false`, `--surface`를 생략하면 그 터미널의 `TASTY_SURFACE_ID`)로 알린다.

- 터미널에는 소유자가 없다. 셸(로컬 CLI)과 plugin 누구나 알린다.
- 판정은 위 (1)~(4)의 결과와 알린 값의 합집합이다. 알린 값이 출력 판정을 끄지는 않는다.
- 알린 뒤 첫 폴링에서 본 전경 pid를 기억한다. 전경이 셸 자신이 되거나 비거나 다른 pid로 바뀌면 지운다. 알린 프로그램이 idle을 알리지 못하고 끝나도 점이 남지 않는다. 그래서 셸 프롬프트에서 직접 부른 표시는 다음 폴링에서 지워진다. 스크립트 안에서 부르면 스크립트가 끝날 때까지 남는다.
- surface가 닫히면 값도 지운다.
- 번들 Claude·Codex plugin의 훅은 자식 상태를 `active`로 알릴 때 켜고 idle·입력 대기로 알릴 때 끈다. 세션 시작은 끄기만 한다. 사용자가 아직 아무것도 시키지 않았으면 Stop이 오지 않아 점이 계속 켜져 있게 되기 때문이다.

| 상황 | 판정 |
|---|---|
| `claude`·`codex`가 API 응답을 기다리며 출력이 없음 | 프롬프트 제출 훅이 켠 값으로 busy |
| `claude` 턴이 끝남(Stop 훅) | 훅이 끈다. 출력 판정도 idle이면 idle |
| `claude`가 훅 없이 죽어 셸이 앞으로 옴 | 다음 폴링이 지워 idle |

## 집계 — OR

| 단위 | busy 판정 |
|------|-----------|
| Surface(terminal) | 위 판정 |
| Surface(terminal) — 알린 값 | 위 판정과 합집합 (R) |
| Surface(plugin) | 소유 plugin이 알린 값 (P) |
| Tab | 포함 surface 중 하나라도 busy |
| Workspace | 포함 surface 중 하나라도 busy. 개수도 제공 |

## 시각 표시

- **Tab**: 라벨 옆에 작은 점 1개. 포커스된 탭의 텍스트는 흐리게 표시하고 개수는 표시하지 않는다.
- **Workspace(사이드바)**: 펼친 행에 점과 개수(예: `● 3`). 다른 워크스페이스의 실행 상태를 알린다. 점과 숫자는 같은 `busy_count`를 읽어 어긋나지 않는다. 값이 0이면 숫자와 툴팁이 없고 칸만 남아 이름이 움직이지 않는다. 100 이상은 `99+`이고 툴팁은 정확한 값이다. 접힌 레일은 점만 있고 개수가 없다.
- **Surface 자체**: 포커스 테두리와 분할 영역이 있으므로 별도 표시는 두지 않는다.

## 폴링 / 플랫폼

PTY 전경 프로세스는 매 프레임이 아닌 약 1초마다 조회한다(`crates/tasty-terminal/src/foreground_process.rs`).

| 플랫폼 | 조회 방법 | 정확도 |
|--------|---------|--------|
| Linux | `/proc/<pid>/stat`의 tpgid → `/proc/<tpgid>/comm` | 정확 |
| macOS | `proc_pidinfo(PROC_PIDTBSDINFO)`의 `e_tpgid` → 같은 시스템 호출로 leader의 `pbi_name` | 정확 |
| Windows | `CreateToolhelp32Snapshot`으로 셸의 가장 얕은 비셸 자손 선택. 같은 깊이는 PID 오름차순. 없으면 가장 깊은 리프/셸로 대체 | 근사. ConPTY는 전경 PGID를 제공하지 않음 |

프로세스 조회는 fork 없이 시스템 호출이나 파일 읽기로 처리한다. Windows는 한 번의 폴링에서 시스템 프로세스 스냅샷을 만들고 모든 surface가 공유한다. 중첩 셸은 건너뛰고 가장 얕은 비셸 자손을 골라 짧게 실행되는 보조 프로세스 때문에 이름이 자주 바뀌는 것을 줄인다. Windows는 PID를 재사용하므로 부모 PID 연결이 순환할 수 있다(셸 자신이 순환에 들어가는 경우 포함). 탐색은 PID마다 한 번만 펼쳐 스냅샷 크기 안에서 끝나고, 이미 지난 PID로 돌아오는 연결은 무시한 채 나머지 자손에서 같은 규칙으로 고른다. Tasty 자신의 PID는 탐색에서 뺀다. Tasty가 모든 셸의 부모이므로 셸 아래에 Tasty가 보이는 것은 종료된 Tasty 부모의 PID를 셸 자손이 물려받은 경우뿐이다.

StatusBar의 프로세스명도 이 1Hz 캐시(`CoreState::foreground_name`)를 읽으므로 표시가 최대 1초 늦을 수 있다.

## CLI/IPC

`surface.list`는 `busy: bool`, `tree`·`workspace.list`·`tab.list`는 `busy_count: number`를 제공한다(`annotate_tree_busy`). 포커스와 관계없이 해당 surface의 상태를 반환한다.

## 비-목표

- 실행 중인 명령줄 조회. [OSC 133 셸 통합](../../features/terminal-output/index.md)에서 다룬다.
- busy 지속시간과 CPU·메모리 사용량 추적.

## 관련

- [focus](focus.md) · [notifications](../../features/notifications/index.md) · [terminal](../../surfaces/terminal/index.md)
