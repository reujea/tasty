# ADR-0017: Windows 절전(suspend/resume) 후 PTY 헬스 복구는 Windows 전용으로 구현한다

- **Status**: Accepted
- **Date**: 2026-06-21
- **Tags**: pty, conpty, suspend, resume, power-management, windows, platform, lifecycle, terminal, cross-platform
- **Group**: terminal-input

## Context

surface 안에서 TUI(예: claude code = node)를 실행한 채 OS 가 절전(suspend/sleep)에 들어갔다 깨어나면(특히 여러 번 반복), 해당 surface 의 입력이 완전히 멈추는 증상이 보고됐다.

조사 결과:

- tasty 는 입력을 PTY 에 정상적으로 write 한다 (IPC `send` 가 `sent:true`). 즉 입력 라우팅·IME·포커스·렌더 좌표 계산은 무관하다.
- PTY 직접 자식(셸/node)이 `try_wait()` 상 **살아있지만 stdin 을 읽지 않는 hang 상태**가 된다. 자식이 *죽으면* `ProcessExited` cascade(`Terminal::process` → `cascade_terminal_process_exited` → `close_surface_by_id_no_snapshot`)가 surface 를 정리하므로 정상이지만, **hang 은 idle 과 구분할 수 없어 기존 exit 감지로는 잡히지 않는다.**
- 이 증상은 **macOS 에서는 재현되지 않는다.** Unix PTY 는 커널 객체로, sleep 이 프로세스를 freeze→thaw 하면서 master/slave fd 와 파이프를 보존한다. 깨어난 프로세스가 절전 직전 상태 그대로 stdin read 를 재개한다.
- 반면 Windows 의 ConPTY 는 `conhost.exe` 헬퍼 + named pipe 기반이라, 절전(특히 modern standby / hibernate)에서 파이프 연결·입력 루프가 불안정해질 수 있다. 즉 이 문제는 **ConPTY 구조에 기인한 Windows 특유의 취약성**이다.

또한 winit 의 `ApplicationHandler::suspended()` / `resumed()` 는 데스크톱에서 OS 절전(S3/S0/hibernate)과 매핑되지 않는다 (주로 모바일 앱 라이프사이클용이며, 데스크톱에서 `resumed` 는 앱 시작 시 1 회 호출될 뿐이다). 따라서 winit 라이프사이클 콜백만으로는 절전 복귀를 감지할 수 없다.

## Decision

Windows 절전 복귀 후 PTY 헬스 복구를 **`#[cfg(windows)]` 전용 경로**로 구현한다. 구성:

1. **절전/복귀 감지** — Windows `WM_POWERBROADCAST`(`PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND` 등)를 윈도우 메시지 레벨에서 후킹해 resume 신호를 이벤트 루프(`AppEvent`)로 주입한다.
2. **resume 헬스 패스** — resume 시 모든 terminal 을 순회하며 (a) `check_process_alive()` 재확인 → 죽은 PTY 는 기존 `ProcessExited` cascade 로 즉시 정리, (b) 살아있는 PTY 에는 현재 크기로 resize 를 한 번 강제(ConPTY 의 SIGWINCH 상당)해 TUI 재draw·입력 루프 재개를 유도하는 **wake nudge**.
3. **가시화** — wake nudge 로도 응답이 없는(절전 복귀 후 hang 으로 판단되는) surface 를 사용자에게 표시해, 사용자가 원인을 인지하고 재시작할 수 있게 한다.

macOS / Linux 에서는 이 경로 전체가 **no-op**(`#[cfg(not(windows))]`)이다.

## Consequences

- **얻은 것**: 절전 복귀 후 죽은 PTY 가 즉시 정리되고, 깨울 수 있는 TUI 는 wake nudge 로 복구되며, 복구 불가한 surface 는 사용자에게 가시화된다. 그동안 "입력이 그냥 안 먹는다"로만 보이던 증상의 원인이 드러난다.
- **잃은 것**: hang 의 자동 *완전* 복구는 보장하지 못한다. hang 과 idle 은 본질적으로 구분 불가능하므로, wake nudge 가 실패하면 최종 수단은 사용자 재시작이다.
- **운영 비용 / 유지 부담**: Windows 메시지 후킹(`WM_POWERBROADCAST`)이라는 OS 종속 코드가 추가된다. winit 의 HWND 접근 방식이나 power 이벤트 지원이 바뀌면 이 후킹을 따라 유지해야 한다.

## Alternatives Considered

- **winit `suspended()` / `resumed()` 사용** — 데스크톱에서 OS 절전과 매핑되지 않아 절전 복귀를 감지할 수 없다. 기각.
- **크로스플랫폼 공통 구현** — mac/Linux 는 Unix PTY 라 절전에 강건해 대응이 불필요하고, 멀쩡한 PTY 에 resize/개입을 가하면 부작용(불필요한 재draw, TUI 깜빡임) 위험이 있다. 기각.
- **hang 자동 완전 복구(프로세스 강제 kill·재spawn 등)** — hang 과 idle 을 구분할 수 없어 정상 idle 프로세스를 오판해 죽일 위험이 있다. tasty 의 "에이전트 행동이 사용자 상태를 훼손하지 않는다" 원칙에도 어긋난다. 기각 (가시화 + 사용자 재시작으로 대체).
- **주기적 PTY 헬스 폴링(절전과 무관하게 상시)** — 상시 비용을 들여 드물게 발생하는 절전 케이스를 잡는 것은 비효율적이고, hang 오판 위험은 그대로다. resume 트리거 기반으로 한정. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- macOS / Linux 에서도 절전 복귀 후 동일/유사한 PTY hang 증상이 보고된다 (Unix PTY 가정이 깨짐).
- winit(또는 채택 중인 창 라이브러리)이 데스크톱 OS power 이벤트를 1 급으로 지원하게 된다 (OS 종속 후킹을 제거할 수 있음).
- Windows ConPTY 가 절전에 강건해져 wake nudge 없이도 입력 루프가 복구된다.
- hang 과 idle 을 신뢰성 있게 구분할 수단이 생긴다 (자동 복구의 안전성 확보).

## References

- [ADR-0002: VTE 파싱을 입력 스레드 밖 파서 스레드로 분리](0002-vte-parsing-off-input-thread.md) — PTY 파서/리더 스레드 구조, `parser_eof` 신호
- `crates/tasty-terminal/src/lib.rs` — `Terminal::process`(child exit 감지), writer/parser 스레드
- `crates/tasty-terminal/src/accessors.rs` — `check_process_alive`, `is_alive`
- `src/app/dispatch_domain.rs` — `cascade_terminal_process_exited`(자식 사망 시 surface 정리)
- `src/app/event_handler.rs` — `ApplicationHandler`, `AppEvent`
- 관련 design 문서: PTY 라이프사이클 / Windows 절전 대응 (현재 운영 상태)
