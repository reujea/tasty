# ADR-0457: 단건 plugin 종료는 메인 스레드 밖에서 회수하고, 새 프로세스는 옛 것이 빠진 뒤에 뜬다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: plugin, host-plugin, lifecycle, shutdown, main-thread, healthcheck, restart, concurrency
- **Group**: plugin-system

## Context

plugin 을 하나만 내리는 경로 — `plugin disable` 과 헬스체크의 무응답 재시작 — 는
`PluginProcess::shutdown(PLUGIN_SHUTDOWN_TIMEOUT)` 으로 자식이 빠질 때까지 **메인 스레드에서**
기다렸다(최대 2 s, 넘으면 kill). `docs/architecture/shutdown-sequence.md` 는 이것을 "단건 경로는
그대로 블로킹 — 대상이 하나뿐이라 겹칠 것이 없다" 로 적어 두었다. 겹칠 대상이 없다는 말은 맞지만,
그 2 s 동안 메인 스레드가 선다는 비용은 적혀 있지 않았다.

그 비용이 재어졌다(격리 debug GUI 인스턴스, 소켓 읽기를 멈춘 probe plugin). shutdown 요청을 못 읽는
plugin 을 disable 하자 CLI 가 2065 ms 걸렸고 같은 구간의 `list info` 한 번이 2101 ms 였다(평시 중앙값
97 ms). 헬스체크 재시작 순간의 `list info` 는 2161 ms 였다. 메인 스레드가 IPC 응답과 프레임을 함께
처리하므로 **모든 IPC 와 화면이 그만큼 선다.** 영구히 멈춘 plugin 은 약 62 초(무응답 60 s + 2 s)마다
재시작되므로 그 정지가 주기적으로 되풀이된다. 느린 plugin 하나가 다른 plugin 과 호스트 응답을 막지
않아야 한다는 요구와 정면으로 부딪힌다.

기다림을 없애는 것만으로는 안 된다. 재시작은 옛 프로세스를 내린 **직후** 새 프로세스를 띄운다. 기다리지
않으면 두 프로세스가 최대 2 s 겹치고, 옛 것이 쥔 자원을 새 것이 못 잡는다. 번들 `agent-stream` 이 그
형태다 — 재시작 뒤 기동하며 사용자가 켜 둔 SSE 엔드포인트를 **같은 포트**로 다시 연다.

## Decision

**단건 종료는 shutdown 요청을 보낸 뒤 회수 대기를 전용 스레드에 맡기고, 메인 스레드는 기다리지 않는다.
새 프로세스는 옛 프로세스가 회수된 뒤에 띄운다.**

- **대기는 스레드가 한다.** `PluginProcess::begin_shutdown` 이 요청을 보내고 돌려준 `PendingShutdown`
  을 `plugin-retire-<id>` 스레드가 `wait()` 한다 — graceful 기회(2 s)와 deadline 뒤 kill 은 예전과
  같다. 스레드를 못 띄우면 그 자리에서 기다린다(예전 동작).
- **메인 스레드는 거두기만 한다.** 회수 중인 것이 있는 동안만 50 ms 주기 타이머(`PluginRetire`)가
  등록되고, 그 tick 에서 끝난 스레드만 join 한다 — 끝난 스레드의 join 은 즉시 돌아온다. 마지막 회수가
  끝나면 타이머를 내린다. 한 건마다 `plugin process retired`(`ms` · `reason`) 한 줄을 남긴다.
- **기동은 한 창구에서 미룬다.** `start_plugin_internal` 이 회수 중인 id 를 받으면 띄우지 않고 "회수
  뒤에 띄운다" 고 적는다. 무응답 재시작의 재기동과 전체 기동(`discover_and_start`)이 그 창구를
  지나므로 어느 쪽에서 와도 겹치지 않는다. 회수 중에 온 `disable` 은 그 예약을 거둔다. 명시적 `enable` 은
  이 창구에 닿기 전에 회수를 끝내므로 미뤄지지 않는다(아래).
- **기다리는 자리는 넷이다 — `plugin remove` · swap · `upgrade-builtins` 의 쓰기 갈래 · 명시적
  `enable`.** 헬스체크 재시작과 `disable` 은 기다리지 않는다. 앞의 셋은 옛 프로세스가 **사라져 있어야**
  하는 자리이고, `enable` 은 그 자리에서 띄우려는 자리다.
- **옛 프로세스가 사라져 있어야 하는 자리는 기다린다 — `plugin remove` · swap · `upgrade-builtins` 의
  쓰기 갈래.** `plugin remove` 는 디렉토리를 지우고, swap(`upgrade-builtins --restart-running` ·
  auto-reload)과 `upgrade-builtins` 의 쓰기 갈래는 디렉토리를 덮어쓴다 — 실행 중인 파일은 Windows 에서
  지워지지도 덮어써지지도 않고 Linux 에서는 `ETXTBSY` 가 난다. 이 자리들은 `wait_retired` 로 그 id 의
  회수를 끝까지 기다린 뒤 쓴다. 사용자가 명시적으로 부른 수명주기 조작이고 주기적으로 오지 않는다.
  `upgrade-builtins` 의 **쓰기 갈래**는 버전이 달라 덮어쓸 때와, 같은 버전인데 바뀐 내용이 있을 때다 —
  같은 버전 갈래는 회수 중일 때만 쓰지 않고 먼저 물어본다. 설치본이 더 높아 건너뛰거나 바뀐 것이
  없으면 기다리지 않는다(기다리면 쓸 것도 없이 메인 스레드가 최대 2 s 선다).
- **명시적 `enable` 은 회수를 기다려 그 자리에서 띄운다.** 사용자·에이전트가 부른 `enable`(IPC · CLI ·
  설치 직후의 기동 · 재승인)이 회수 중인 id 를 만나면 `wait_retired` 로 그 id 의 회수만 끝까지 기다린 뒤
  바로 띄운다. 그래서 `enable` 이 돌아온 순간 plugin 이 떠 있고, "disable → enable → 곧바로 호출" 이
  예전처럼 성공한다. 그 대가로 **이 조작에 한해** 메인 스레드가 최대 2 s 선다 — remove · swap 과 같은
  논리다: 사용자가 명시적으로 부른 수명주기 조작이고 주기적으로 오지 않는다. 주기적으로 오는 헬스체크
  재시작과, 끝을 기다릴 이유가 없는 `disable` 은 계속 기다리지 않는다.
- **기다린 자리는 재기동 예약을 잇는다.** 회수 뒤에 다시 띄우기로 한 예약(무응답 재시작 · 회수 중에
  기동 창구를 지난 전체 기동)은 회수 기록과 함께 `wait_retired` 가 가져가 돌려준다. 호출자는 쓰기를 마친 뒤 그 값이
  참이면 다시 띄운다 — 안 이으면 enabled 인 plugin 이 꺼진 채 남는다. swap 과 `enable` 은 어차피 다시
  띄우므로 따로 잇지 않는다.
- **호스트 종료는 회수 중인 것도 본다.** `poll_shutdown_all` 은 회수 중인 것까지 끝나야 `true` 이고,
  그것들은 다시 띄우지 않는다. 끝난 것마다 S4a 를 `retiring before exit` 문구로 남긴다. 각 회수는 자기
  deadline 을 들고 있으므로 종료 대기의 상한은 그대로 2 s 로 수렴한다.
- **plugin 이 보는 순서는 그대로다** — shutdown 요청 → (2 s 안에 안 빠지면) kill. wire 도 그대로다.

**이전 결정과 바뀐 이유.** `shutdown-sequence.md` 의 "단건 경로는 그대로 블로킹" 을 이 결정이 대체한다.
그 결정의 근거("대상이 하나라 겹칠 것이 없다")는 **여러 plugin 의 대기를 겹칠 필요가 없다**는 말로는
맞았다. 바뀐 것은 비용의 측정이다 — 대상이 하나여도 그 대기가 메인 스레드 위에 있으면 호스트 전체가
선다는 것이 재어졌다.

## Consequences

- **얻은 것**: 정지한 plugin 의 disable 이 CLI 69 ms 로 끝났고 같은 구간의 `list info` 최댓값이
  98 ms 였다(전 2065 ms · 2101 ms). 옛 프로세스는 뒤에서 2050 ms 에 kill 로 회수됐다. 헬스체크 재시작
  구간의 `list info` 최댓값은 194 ms 였다(전 2161 ms). 격리 debug GUI 인스턴스, 2026-09-21.
- **얻은 것**: 새 프로세스가 옛 것과 겹치지 않는다 — 같은 재현에서 새 slowsub 는 옛 것이 회수되고
  44 ms 뒤에 떴다.
- **얻은 것**: "disable → enable → 곧바로 호출" 이 예전처럼 성공한다 — 옛 프로세스를 멈춰 둔 probe
  plugin 에서 disable 53 ms, enable 2044 ms(회수 대기), 이어진 namespace 호출이 72 ms 에 성공했고 새
  프로세스는 회수 27 ms 뒤에 떴다. 격리 debug GUI 인스턴스, 2026-09-22.
- **얻은 것**: 재시작 중에 온 `upgrade-builtins` 가 재기동을 잃지 않는다 — 멈춘 번들 plugin 의 재시작
  구간에 설치본 파일 하나를 어긋나게 두고 부르자 회수를 기다려(2135 ms) 그 파일을 고쳐 썼고, plugin 은
  회수 67 ms 뒤에 다시 떴다. 어긋난 파일이 없으면 같은 호출이 478 ms 에 끝났고(기다리지 않음) plugin 은
  뒤의 회수 tick 에서 떴다. 같은 인스턴스.
- **잃은 것**: 헬스체크 재시작과 `disable` 뒤에는 회수가 끝날 때까지(최대 2 s) 그 plugin 이 **떠 있지
  않다.** 예전에는 그 2 s 동안 메인
  스레드가 서 있어 아무도 그 틈을 못 봤다. 지금은 그 사이의 namespace 호출이 `-32002 plugin '…' is not
  running` 을 받고, `plugin list` 가 `running: false` 를 보인다.
- **잃은 것**: disable 의 응답이 "프로세스가 끝났다" 를 뜻하지 않는다. 끝났음을 알아야 하는 호출자는
  `remove` · `upgrade-builtins` 처럼 `wait_retired` 를 거쳐야 한다. 그 호출은 회수 뒤의 재기동 예약을
  함께 가져오므로, 돌려받은 값이 참이면 쓰기를 마친 뒤 호출자가 다시 띄운다.
- **잃은 것**: 회수 중인 plugin 에 온 명시적 `enable` 은 메인 스레드를 최대 2 s 세운다 — 예전 disable 이
  서던 만큼이 disable 에서 enable 로 옮겨 간 것이고, 회수가 이미 끝났으면 서지 않는다.
- **운영 비용**: 회수마다 스레드 하나(최대 약 2 s 산다). 회수 중에만 50 ms 타이머가 호스트를 깨운다.

## Alternatives Considered

- **A: 스레드 없이 메인 스레드가 논블로킹으로 폴링한다** — 호스트 종료의 `ShutdownBatch` 가 그 모양이다.
  안 고른 이유는 deadline 뒤의 `kill()` + `wait()` 다. 보통은 즉시 돌아오지만 커널 안에서 멈춘 자식이면
  `wait()` 가 서고, 그 자리가 메인 스레드다. 스레드에 두면 그 경우도 메인을 안 세운다.
- **B: 기다리지 않고 새 프로세스를 바로 띄운다** — 가장 단순하다. 옛 프로세스가 쥔 포트·파일과 충돌한다
  (Context 의 `agent-stream`). plugin 쪽에 "앞 인스턴스가 살아 있을 수 있다" 는 새 계약을 지우는 일이다.
- **C: 재시작은 그대로 블로킹하고 disable 만 비동기로** — 주기적으로 되풀이되는 쪽이 재시작이라 문제의
  절반이 남는다.
- **D: graceful 기한(2 s)을 줄인다** — 정지 시간이 줄 뿐 없어지지 않고, 정상 plugin 의 graceful 기회가
  같이 줄어든다.
- **E: `remove` · `upgrade-builtins` 도 기다리지 않는다** — 실행 중인 파일을 지우거나 덮어쓰게 된다.
  Windows 에서 실패하고, 그 실패가 사용자에게는 "제거가 안 된다" 로 보인다.
- **F: 명시적 `enable` 도 미룬다** — 메인 스레드가 한 번도 안 선다. 안 고른 이유는 호환이다. 예전에는
  `enable` 이 돌아오면 plugin 이 떠 있었고, "disable → enable → 곧바로 호출" 하는 스크립트가 그것에
  기대었다. 미루면 그 호출이 최대 2 s 동안 `-32002 not running` 을 받는다 — 부른 쪽은 성공 응답을 받고도
  실패를 본다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- disable 이 다시 회수를 기다리면 `manager::tests_retire::disable_returns_before_a_stalled_child_exits`
  가, 기동 창구가 회수 중인 id 를 그대로 띄우면 `the_start_gate_defers_a_plugin_that_is_still_retiring`
  이 잡는다(두 변이를 실제로 붙여 죽는 것을 확인했다). 회수 중의 disable 이 예약을 안 거두면
  `a_disable_during_a_restart_cancels_the_restart` 가 잡는다.
- 명시적 enable 이 회수를 안 기다리고 미루면
  `an_explicit_enable_during_retirement_waits_and_starts_in_place` 가 잡는다(기다림을 빼는 변이로
  확인했다).
- 재시작이 옛 프로세스를 기다리거나 새 것을 그 자리에서 띄우면
  `an_unresponsive_restart_waits_for_the_old_process_before_starting` 이, 종료가 회수 중인 것을 두고
  끝나거나 다시 띄우면 `exit_waits_for_a_retiring_plugin_and_does_not_respawn_it` 이 잡는다.
- 무응답 재시작 중에 `upgrade-builtins` 가 와서 재기동 예약을 버리면
  `builtin::upgrade_retire_tests::an_upgrade_during_a_restart_keeps_the_restart` 가 잡는다. 그 시험은
  쓰기 갈래 셋 — 같은 버전에 바뀐 내용 · 버전이 오름 · `--force` — 을 모두 태우고 보고의 종류로 그
  갈래를 탔는지 단정한다. 확인한 변이: 끝의 예약을 잇는 한 줄을 끄기, 버전이 오르는 갈래와 `--force`
  갈래에서 각각 예약을 받는 대입만 빼기 — 셋 다 죽었다. 쓰기 갈래가 회수를 안 기다리면 같은 시험이
  잡는다(같은 버전 갈래와 버전이 오르는 갈래에서 변이로 확인했다. `--force` 갈래는 같은 단정이 있지만
  변이로 재지 않았다). 쓸 것이 없는 갈래가 기다리면
  `an_upgrade_that_writes_nothing_does_not_wait_for_a_retirement` 가 잡는다(변이로 확인했다).
- plugin 프로토콜에 "앞 인스턴스가 살아 있어도 된다" 는 계약이 생기면 — 겹침을 막는 이유가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 메인 스레드가 정말 안 서는가. 재는 법: shutdown 요청을 못 읽는 plugin 을 띄우고(소켓 읽기를 멈춘
  probe) disable 하는 동안 `tasty list info` 왕복을 연속으로 재서 최댓값을 평시와 견준다. 재시작 경로는
  같은 plugin 을 enable 한 채 75 초 둔다.
- 회수 중의 `-32002` 가 사용자에게 문제가 되는가. 재는 법: 재시작 직후 2 s 안에 그 plugin 의
  namespace 호출이 몇 번 오는지 호스트 로그에서 센다.

## References

- 관련 문서: [`shutdown-sequence.md`](../architecture/shutdown-sequence.md) "단건 경로" 항 — 이 결정의 현재 운영 상태
- 관련 문서: [`plugin-development.md`](../dev-guide/plugin-development.md) §9.1 — disable → upgrade-builtins → enable 절차
- 관련 ADR: [ADR-0360](0360-plugin-channels-are-bounded-in-bytes-per-queue-and-in-total.md) — 채널 포화가 shutdown 요청을 거절할 때의 판정(이 결정은 그 뒤의 대기만 옮긴다)
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-host-plugin` 의 `manager::retire` 모듈 —
  `PluginManager::retire_process` · `defer_start_until_retired` · `poll_retiring` · `wait_retired` ·
  `start_if_still_wanted` · `is_retiring` · `poll_retiring_for_exit`, 그리고 `PluginTick::Retire`.
  `upgrade-builtins` 쪽은 `builtin` 모듈의 `apply_builtin_upgrade_decision` 과 `builtin::sync_probe`.
