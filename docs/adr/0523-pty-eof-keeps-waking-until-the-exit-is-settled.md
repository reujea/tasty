# ADR-0523: PTY EOF 뒤에는 자식 종료가 판정될 때까지 parser 스레드가 계속 깨운다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: pty, terminal, process-exit, waker, headless, attach, structural-delta, flaky, ci, adr-0002, adr-0211, adr-0481
- **Group**: terminal-input

## Context

CI 의 `check-headless` 잡에서 `tests/attach_structure_sync_loopback.rs` 의
`a_shell_exit_on_the_server_reaches_the_holder_as_a_delta` 가 "no structural_delta within 20s"
로 빨개졌다. 시험은 서버 셸에 `exit` 를 보내고 그 surface 가 닫힌 트리가
`StructuralDelta` 로 holder 에게 오기를 기다린다([ADR-0481](0481-a-server-side-structure-change-reaches-the-holder-as-a-delta.md)).

로컬에서 같은 조합(`--no-default-features`)으로 그 시험 하나만 반복하면 부하를 따로 걸지
않아도 35 회 중 2 회 같은 모양으로 죽었다(2026-09-23, 이 머신의 load average 는 25 안팎).
PTY 종료 → surface 닫기 → delta 발행 → holder 수신의 네 자리에 임시 계측을 넣고 실패한
회차를 보면, 끊긴 자리는 첫째였다.

- parser 스레드가 PTY EOF 를 보고 호스트를 **한 번** 깨웠다.
- 그 wake 가 이끈 `Terminal::process` 의 alive 판정(`try_wait`)이 EOF 와 같은 ms 에
  **살아 있다**고 답했다.
- 그 뒤 20 s 동안 호스트에 온 PTY wake 가 0 건이라 `process()` 가 다시 불리지 않았고,
  `ProcessExited` 가 끝까지 안 나왔다. 그래서 surface 닫기가 안 불렸고, 그 사이 스트림
  활동마다 불린 delta 발행은 보낼 표시가 비어 있었다.

EOF 는 종료가 아니다. 커널은 종료하는 자식의 파일 기술자를 먼저 닫고(이것이 master 쪽 EOF
다) 그다음에야 그 자식을 회수 가능한 상태로 만든다. 그 사이가 보통 짧아 한 번의 판정으로
대개 잡히지만, 못 잡으면 **다시 볼 기회가 없다.** PTY 가 조용해졌으므로 parser 스레드가 더
깨울 일이 없고, 호스트는 wake 없이 그 terminal 을 `process()` 하지 않는다 — headless 는
그 surface 를 부를 주기 경로가 아예 없다. `process()` 안의 `ALIVE_CHECK_INTERVAL` 주기
`try_wait` 는 **`process()` 가 불릴 때** 의 레이트 리밋일 뿐 스스로 도는 폴이 아니다.

그래서 이것은 시험의 시한 문제가 아니라 제품 결함이다 — 셸이 끝났는데 그 surface 가 서버에
남고, 점유 중인 holder 의 mirror 도 그것을 계속 보인다. 시한을 늘려도 한 번 놓친 종료는
그 뒤 어느 시점에도 안 온다(실패 회차는 시한 20 s 동안 PTY wake 가 하나도 없었다).

## Decision

parser 스레드는 PTY EOF 를 본 뒤 한 번 깨우고 끝나지 않는다. 핸들이 종료를 **판정했다고
알릴 때까지** 간격을 두 배씩 늘려(첫 간격 `EOF_REWAKE_FIRST` 10 ms, 상한
`ALIVE_CHECK_INTERVAL` 500 ms) 계속 깨운다. 핸들은 `ProcessExited` 를 낼 때와, 자식을
다른 소유자에게 넘길 때(`take_child`) 공유 플래그 `exit_settled` 를 세운다. 핸들이
drop 됐으면(parser 스레드가 쥔 약한 참조의 강한 수가 0) 역시 멈춘다. 고친 자리는
`crates/tasty-terminal` 하나이고 GUI·headless 두 호스트가 함께 받는다.

## Consequences

- **얻은 것**: EOF 뒤의 종료 판정이 한 번의 경주에 걸리지 않는다. 같은 부하
  조건(CPU 소모 프로세스 16 개 + 그 시험 6 개 동시 × 15 회)에서 옛 동작은 90 회 중 4 회
  delta 를 못 받았고, 이 결정 뒤에는 90 회 중 0 회였다. 창을 결정적으로 넓힌 단위 시험
  (`process_exit_after_pty_eof_is_seen_by_wakes_alone` — 자식이 PTY 를 닫고 1 초 뒤에
  죽는다, 대기는 wake 에만 반응)은 옛 동작에서 wake 1 회 뒤 15 s 를 다 쓰고 죽는다.
- **잃은 것**: EOF 뒤에도 살아 있는 자식(표준 fd 를 PTY 에서 떼고 계속 도는 프로그램)이면
  parser 스레드가 500 ms 마다 깨우기를 그 자식이 죽거나 surface 가 닫힐 때까지 계속한다.
  wake 한 번의 값은 호스트가 그 wake 를 어느 갈래로 받느냐에 따라 다르다.
  - headless 의 targeted wake 는 그 terminal 하나의 `process()` 와 `try_wait` 한 번이다
    (`src/boot.rs` `handle_terminal_output`).
  - GUI 의 targeted wake(`targeted_pty_polling` 켜짐, 기본값)는 그 terminal 하나를
    `process_pty_output` 한 뒤, 그 surface 가 **보이면** 출력이 실제로 바뀌었는지와 무관하게
    `mark_dirty_from(RepaintSource::TerminalOutput)` 를 부른다(`src/app/event_handler.rs`
    `handle_terminal_output`). 그래서 보이는 surface 에 그런 자식이 있으면 **그 창 전체를
    500 ms 마다 다시 그린다.** 안 보이는 surface 면 redraw 는 없고 drain 만 한다.
  - `targeted_pty_polling = false` 면 wake 가 surface 를 싣지 않는 default 갈래로 가서, 모든
    view 와 파킹된 engine 의 `process_all_pty_output` 를 돌고 **모든 창을 dirty 로 만든다**
    (GUI · headless 모두 전체 drain, redraw 는 GUI 만).
- **운영 비용 / 유지 부담**: parser 스레드의 수명이 "EOF 까지" 에서 "종료 판정 또는 drop
  까지" 로 늘었다. drop 뒤 멈추기까지 최대 한 간격(500 ms) 동안 스레드가 남는다 —
  `PtyBackend` 의 drop 은 parser 스레드를 join 하지 않으므로 drop 이 그만큼 막히지는 않는다.

## Alternatives Considered

- **시험의 시한을 늘린다** — 원인이 시한이 아니다. 놓친 종료는 시한 안에 안 오는 것이 아니라
  영영 안 온다(실패 회차의 계측: EOF wake 뒤 20 s 동안 PTY wake 0 회).
- **headless 루프의 1 Hz `Tick::Busy` 에서 모든 terminal 을 `process()` 한다** — 고치는
  자리가 호스트마다 갈리고(GUI 도 전체 `process_all` 을 default wake 에서만 돌려 같은
  구멍을 가진다 — 두 호스트를 따로 고쳐야 한다), 원인이 없는 terminal 까지 초당 한 번
  돈다. 원인은 "EOF 뒤 한 번만 깨운다" 는 terminal 크레이트의 약속에 있으므로 그 자리에서
  고쳤다.
- **parser 스레드가 EOF 뒤 자식을 직접 `wait` 한다** — 자식의 소유자는 핸들(`PtyBackend`)
  이고 `take_child` 로 다른 소유자에게 넘어가기도 한다. 스레드가 소유권을 나누면 drop 의
  kill/reap 계약([ADR-0050](0050-headless-pty-primitive.md))과 겹친다.
- **SIGCHLD 로 깨운다** — unix 전용이고 Windows 에는 대응이 없다(원칙 4). 프로세스 전역
  신호 처리를 한 크레이트가 소유하게 되는 부담도 크다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 호스트가 PTY wake 와 무관하게 모든 terminal 을 주기적으로 `process()` 하는 경로가 생기면
  이 재-wake 는 중복이다. 재는 법: `process_all_pty_output` 의 호출처(`src/boot.rs` ·
  `src/app/event_handler.rs`)가 wake 처리 밖에도 있는지 센다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- EOF 뒤 살아 있는 자식이 흔해 500 ms 재-wake 가 눈에 띄는 비용이 되면 상한을 다시 본다.
  재는 법: 그런 surface 를 띄운 채 wake 수(headless 는 `AppEvent::TerminalOutput` 수신 수)를
  센다. GUI 는 비용이 wake 가 아니라 redraw 로 나오므로 두 갈래를 따로 잰다 — ⑴
  `targeted_pty_polling` 켜짐에서 그 surface 를 **보이게** 둔 창의 redraw 수(보이지 않게
  두면 0 이어야 한다), ⑵ `targeted_pty_polling = false` 에서 **모든 창**의 redraw 수와
  terminal 수에 비례하는 `process_all_pty_output` 비용.

## References

- 코드 근거(현재 위치): `crates/tasty-terminal/src/lib.rs` 의 `Terminal::new` parser
  스레드 EOF 꼬리 · `Terminal::process` · `EOF_REWAKE_FIRST`,
  `crates/tasty-terminal/src/accessors.rs` 의 `Terminal::take_child`
- 시험: `crates/tasty-terminal/src/tests.rs` 의 `process_exit_after_pty_eof_is_seen_by_wakes_alone`
  (재-wake 가 도는가) · `eof_rewakes_stop_once_the_exit_is_settled`(판정 뒤 멈추는가),
  `tests/attach_structure_sync_loopback.rs` 의 `a_shell_exit_on_the_server_reaches_the_holder_as_a_delta`
- [ADR-0002](0002-vte-parsing-off-input-thread.md) — parser 스레드
- [ADR-0211](0211-the-pty-exit-budget-is-a-safety-net-not-a-race-budget.md) — 같은 종료 판정의 다른 시험(폴링 폴백을 가진 쪽)
- [ADR-0481](0481-a-server-side-structure-change-reaches-the-holder-as-a-delta.md) — 셸 종료를 holder 에 보내는 경로
- [`docs/features/terminal/index.md`](../features/terminal/index.md) "프로세스 종료 / 절전 복귀"
