# ADR-0465: headless 는 이벤트 채널에 IPC wake 를 하나만 두고, 예산에서 멈춘 회차가 루프를 다시 깨운다 — ADR-0313 의 headless 이월 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: ipc, dispatch, fairness, headless, timers, plugin, wake, measurement, adr-0313, adr-0410
- **Group**: ipc-transport

## Context

headless 의 메인 루프는 `mpsc::Receiver<AppEvent>` 하나를 `recv_timeout` 으로 기다린다
(`src/boot.rs` 의 `run_headless`). 그 채널에 네 종류가 섞여 들어온다 — IPC 명령의 `IpcReady`,
PTY 출력의 `TerminalOutput(Some)`, **plugin 수신 스레드가 공유하는 default wake**
`TerminalOutput(None)`, 스트림의 `StreamReady`. 채널은 FIFO 다.

[ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) 은 headless 의 이월(예산에서
멈춘 회차가 남긴 명령)을 별도 배선 없이 두었다. 근거는 "생산자는 명령마다 waker 를 한 번 부르고 회차는
적어도 하나를 처리하므로 남은 wake 수가 남은 명령 수보다 적어지지 않는다" 였다. 명시적 재깨움은 "그
wake 가 이미 큐에 있으니 빈 회차만 는다" 는 이유로 기각했다.
[ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
도 같은 근거를 그대로 옮겼다.

그 불변식이 지속 부하에서 **채널 적체**를 만든다. 회차 하나는 큐의 명령을 예산까지 전부 집지만
`IpcReady` 는 하나만 소비한다. 나머지 `IpcReady` 는 이미 처리된 명령의 몫인데도 채널에 남고, 뒤에 붙은
다른 wake 는 그것을 전부 기다린다.

실측(2026-09-22, debug headless 빌드 · 격리 홈 · 번들 plugin 스테이징). 부하는 연결 16 개가 탭
만들기·목록·닫기를 20 초 보내고, `markdown.recent`(plugin namespace)를 2 초마다 부른 것이다.
계측 빌드로 `IpcReady` 송신·수신 수와 default wake 의 송신→처리 지연을 1 초마다 찍었다.

| 바이너리 | `markdown.recent` 왕복 | 채널의 `IpcReady` 적체 | default wake 송신→처리 최댓값 |
|---|---|---|---|
| 변경 전 | 749–2,010 ms (6 회) | 20 초 동안 742 → 18,570 으로 선형 증가 | 16,611 ms |
| 이 결정 | 197–304 ms (9 회) | 부하 내내 1 이하 | 29 ms |

- 변경 전 부하 중 초당 `IpcReady` 송신은 약 1,000–1,150 건이었고 수신(회차)은 약 58–63 건이었다.
- 변경 전 plugin 답은 default wake 로는 한 번도 안 거둬졌다. 1 Hz `Tick::Busy` 안전망 pump 만
  거뒀고, `markdown.recent` 는 pump 가 두 번 필요해(호출 송신 뒤 plugin 의 host-call, 그리고 답) 약
  2 s 에서 묶였다. 같은 부하에서 gui 는 212–435 ms 였다. gui 는 [ADR-0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md)
  으로 wake 를 양보시키고 `about_to_wait` 가 깨어날 때마다 `mgr.pump` 를 부르므로 이 형태가 없다.
- PTY 출력 wake 도 같은 꼬리에 섰다. 변경 전에는 부하 중 초당 1–7 건만 처리됐고 부하가 끝난 뒤
  802 건이 한꺼번에 처리됐다. 이 결정 뒤에는 부하 중 초당 16–66 건이 처리됐다.
- IPC 처리량은 줄지 않았다. 부하 명령 수는 20,499 에서 21,261 이 됐고, 드문 호출자의 `system.info`
  p50 은 51 ms 로 같았다.
- ADR-0413 이 "headless 는 이 형태가 없다" 고 적은 것은 **타이머**에 대해서는 맞다. 루프가 이벤트
  하나마다 due 한 타이머를 돌리기 때문이다. 채널의 다른 wake 가 굶는 것은 그 관측 밖이었다.

## Decision

**headless 의 IPC waker 는 게이트를 둔다 — 채널에 `IpcReady` 가 이미 하나 있으면 보내지 않는다.**
모든 사본이 게이트 하나를 나눠 든다(`HeadlessWaker::ipc_gate`, accept 스레드와 호스트 주입기가 같은
사본을 쓴다). PTY waker 의 dual gate 와 같은 형태다.

**메인 루프는 `IpcReady` 를 꺼내면 회차를 열기 전에 게이트를 푼다**(`note_ipc_drained`). 그래서
회차가 도는 동안 든 명령은 다음 `IpcReady` 를 세운다. 해제는 `swap(false, AcqRel)` 이다 — 건너뛴
wake 의 `swap(true)` 를 획득해야 그 wake 앞에서 큐에 든 명령이 뒤따르는 회차에 보인다.

**회차가 끝났을 때 큐에 명령이 남았으면 루프를 한 번 더 깨운다**(`wake_ipc`). 남은 명령의 wake 는
게이트에 접혀 사라졌으므로, 안 깨우면 다음 입력까지 남는다. 남았는지는 입장 장부의 지금 값
(`queued_commands > 0`, [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md))
으로 본다 — 명령을 꺼낼 때 장부에서 빠지므로 "아직 안 꺼낸 명령" 과 정확히 같은 모수다. 재깨움은
채널 **꼬리**에 붙으므로 그 사이에 온 PTY·plugin wake 가 먼저 처리된다. 큐가 빈 회차는 깨우지
않는다. 데몬을 멈추는 회차(`Break`)도 깨우지 않는다.

`pump_ipc` 의 시그니처와 회차 규칙(`IpcRound` 의 두 예산)은 바꾸지 않는다. 게이트 해제와 재깨움은
그 호출부(`dispatch_headless_event`)가 한다.

**개정하지 않는 것**:

- ADR-0313 의 수 예산(`DRAIN_BUDGET_PER_ROUND`)과 그 파생, 종료 drain 이 예산을 안 쓰는 것.
- ADR-0410 의 시간 예산(16 ms), 꺼내면서 실행하는 순서, 도착 순(FIFO) 공정성, 호출자별 스케줄링을
  두지 않는 것.
- gui 의 이월(ADR-0413 의 `IpcPacer`). gui 는 winit 사용자 이벤트 큐를 쓰며 이 결정과 무관하다.

## Consequences

- **얻은 것**: 지속 IPC 부하에서도 headless 의 plugin 답과 PTY 출력이 한 회차(최대 약 16 ms) 안에
  차례를 받는다. plugin namespace 왕복이 gui 와 같은 대역이 됐다. 채널이 부하에 비례해 자라지 않는다
  — 변경 전에는 `IpcReady` 한 건마다 채널 노드가 하나 남아 부하 시간에 비례해 메모리를 쥐었다.
- **잃은 것**: "명령마다 wake 한 번" 이라는, 배선 없는 이월 불변식. 이제 이월은 게이트 해제·재깨움
  두 호출에 달려 있다. 둘 중 하나가 빠지면 명령이 다음 입력까지 선다.
- **운영 비용 / 유지 부담**: 재깨움 판정이 입장 장부를 한 번 잠근다(회차마다 한 번, 정수 읽기).
  게이트와 재깨움 판정은 단위 시험이 잰다(`headless_waker.rs` 의 `ipc_wakes_coalesce_until_the_loop_takes_one` ·
  `a_rewake_obeys_the_same_gate`, `src/boot.rs` 의 `rewake_if_left` 시험 둘). 재깨움 호출을 지우면
  `rewake_if_left` 가 안 쓰여 headless 조합 컴파일이 `dead_code` 로 멈춘다. `dispatch_headless_event`
  의 **배선**(게이트 해제 · 판정 인자 `ipc_commands_left` · `wake_ipc`)은 실행 파일째로
  `tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut` 가 잰다 —
  debug 전용 `TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS=0` 으로 띄워 회차마다 한 명령만 꺼내게 하고,
  동시 요청 16 개가 전부 답을 받는지 본다. 기본 예산에서는 그 요청들이 회차를 안 자르므로 이 주입
  없이는 재깨움 갈래를 지나는 시험이 없다. 실측(headless): 재깨움을 `black_box(false)` 로 무력화 ·
  판정 인자를 부정 · 판정 인자에 `&& black_box(false)` 를 붙이는 변이 셋 모두 그 시험이 빨갛다
  (16 s — 응답 상한 10 s 에서 끊긴다). 판정 인자를 `|| false` 로 바꾸면 `ipc_commands_left` 가
  안 쓰여 컴파일이 먼저 멈춘다.

## Alternatives Considered

- **현행 유지**(명령마다 wake, 배선 없는 이월) — 위 실측의 적체가 그대로 남는다. plugin 답은 1 Hz
  안전망에 묶이고 PTY 출력은 부하 동안 거의 안 흐른다.
- **소비 쪽에서 접는다** — `IpcReady` 를 받으면 `try_recv` 로 채널을 비우며 여분의 `IpcReady` 를
  버린다. 생산자는 그대로 둘 수 있다. 안 고른 이유는 버린 wake 가 이월의 몫이었는지 알 수 없어
  결국 같은 재깨움이 필요하고, 그때는 생산자 게이트보다 채널 노드를 더 만들고 더 버리기 때문이다.
- **`IpcReady` 를 채널 대신 원자 플래그로 두고 루프가 매 바퀴 확인한다** — 채널 순서 밖으로 빠져
  "재깨움은 꼬리에 선다" 는 공정성 규칙을 따로 만들어야 한다. 대기(`recv_timeout`)를 깨우는 수단도
  여전히 채널 송신이 필요하다.
- **plugin 수신 스레드에 전용 이벤트를 준다** — plugin 만 풀리고 PTY 출력은 같은 꼬리에 남는다.
  원인은 wake 종류가 아니라 적체다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- headless 에서 `IpcReady` 를 소비하는 자리가 `dispatch_headless_event` 말고 하나 더 생긴다 — 게이트
  해제가 그 자리에 빠지면 게이트가 닫힌 채 남아 IPC 가 다음 재깨움까지 멈춘다. 지금 이 조건을 재는
  가드는 없다.
- 입장 장부를 거치지 않고 명령 큐에 넣는 생산자가 생긴다 — 재깨움 판정이 그 명령을 못 본다. 지금
  이 조건을 재는 가드는 없다.
- 회차 시간 예산의 debug 주입(`src/app/ipc_round.rs` 의 `round_time_budget`)이 사라지거나, 회차가
  `IpcRound::begin` 을 안 거치게 바뀐다 — 그러면 `concurrent_requests_are_all_answered_when_every_round_is_cut`
  가 재깨움 갈래를 안 지난다. 그 시험은 끝에 `queue_dispatch.rounds_stopped_by_time > 0` 을 단언해
  이 경우 통과가 아니라 빨강으로 끝난다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- `StreamReady` 가 같은 적체를 만든다 — 스트림 read 스레드는 프레임마다 wake 를 보내고 게이트가
  없다. 재는 법: attach 스트림에 지속 입력을 넣으면서 headless 의 plugin namespace 왕복과 PTY 출력
  지연을 이 ADR 의 표와 같은 계측으로 잰다.

## References

- 개정 대상: [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) (headless 이월을
  "명령마다 wake 한 번" 불변식에 맡기고 명시적 재깨움을 기각한 조항)
- 같은 근거를 옮긴 자리: [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
  ("이월은 headless 에서 ADR-0313 의 근거 그대로다")
- 개정 대상: [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
  (위 자리의 headless 이월 bullet — 0410 이 이 ADR 을 `부분 개정` 으로 적는다)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- gui 쪽 대응: [ADR-0413](0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md)
- 같은 루프의 짝 수정: headless 루프 한 바퀴가 plugin 허브 데드라인도 거둔다
  (`src/boot.rs` 의 `run_due_timers` → `pump_plugins_if_due`, 소스 가드
  `src/source_guards/headless_loop_reaps_both_hubs.rs`). 그것이 없으면 plugin 데드라인이 지난 뒤
  다음 `Tick::Busy` 까지 루프가 헛돈다(실측: `PluginTick::Ping` 15 s 마다 약 1 s 동안 160 만 회 →
  이 수정 뒤 1 초 창 최대 4 회).
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/production/headless_waker.rs` 의
  `HeadlessWaker::ipc_waker` · `note_ipc_drained` · `wake_ipc`, `src/boot.rs` 의
  `dispatch_headless_event` · `rewake_if_left` · `ipc_commands_left`
- 설계 문서: [`docs/dev-guide/timer-hub.md`](../dev-guide/timer-hub.md),
  [`docs/architecture/data-flows.md`](../architecture/data-flows.md)
