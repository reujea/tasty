# ADR-0413: gui 에서 IPC wake 는 루프의 나머지에 차례를 넘기고, 예산에서 멈춘 회차는 루프를 다시 깨운다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, dispatch, fairness, gui, winit, timers, measurement, adr-0410, adr-0313
- **Group**: ipc-transport

## Context

[ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
은 회차에 시간 예산(16 ms)을 두면서 "회차가 끝나면 gui 는 그 iteration 의 렌더·타이머로 넘어간다" 를
전제로 삼았다. gui 에서 그 전제는 성립하지 않는다.

gui 에서 회차는 두 자리에서 돈다 — `about_to_wait`(iteration 마다 한 번)와 `IpcReady` 사용자 이벤트
처리기. winit(x11, 0.30)은 한 iteration 안에서 사용자 이벤트를 **큐가 빌 때까지** 꺼내 처리한 뒤에야
`AboutToWait` 을 보낸다(`single_iteration` 의 `while let Ok(event) = self.user_receiver.try_recv()`).
IPC 생산자는 명령마다 wake 를 한 번 부르고, 회차가 답을 주면 호출자는 곧바로 다음 명령과 wake 를
보낸다. 그래서 회차가 도는 동안 wake 가 다시 쌓이고, 지속 부하에서는 그 큐가 비는 순간이 없다 —
타이머(`about_to_wait` 앞머리의 `drain_due`)도 렌더도 영영 차례가 안 온다. 회차의 시간 예산은 한
회차를 자를 뿐, 다음 회차가 곧바로 이어지는 것은 막지 못한다.

실측(2026-09-21, debug gui 빌드 · Xvfb · 격리 홈). 부하는 연결 16 개가 탭 만들기·목록·닫기를 20 초
보내고, 드문 호출자가 250 ms 마다 `system.info` 를 부르고, 전역 훅 `interval:1` 이 초마다 시각을 적는
것이다. 표의 "타이머 최대 간격" 은 부하 구간에서 훅 기록의 이웃 간격 최댓값이다(부하가 없을 때도
2.0 s 다 — 훅은 1 Hz tick 에서 "1 초가 지났는가" 를 보므로 간격이 둘로 번갈아 선다).

| 바이너리 | 타이머 최대 간격 | 부하 명령 수 / 20 s | 드문 호출 p50 |
|---|---|---|---|
| 변경 전 main | 21.3 s · 21.4 s · 21.4 s (세 번) | 4,032 · 4,677 · 4,629 | 154 · 136 · 150 ms |
| 시간 예산만(ADR-0410) | 21.0 s | 4,974 | 132 ms |
| 시간 예산 + 이 결정 | 2.02 s | 3,831 | 155 ms |

- 같은 부하에서 변경 전 바이너리의 렌더 통계는 26.6 초 창에 present 1 회였고, 이 결정을 넣은
  바이너리는 부하 내내 초당 30~38 present 였다.
- 이 결정을 넣고 이벤트 수를 센 계측 빌드에서, 부하 중 `about_to_wait` 은 초당 14~34 회 돌았고
  사용자 이벤트가 회차를 연 것은 초당 0 회였다. 회차는 전부 `about_to_wait` 에서 돌았다.
- 원인이 IPC wake 인지 가르기 위해 PTY 를 안 만드는 조회 부하(조회 4 종, 연결 16, 15 초)도 쟀다 —
  변경 전 바이너리도 타이머 최대 간격이 2.03 s 였다. 조회는 한 회차가 수십 µs 라 wake 가 쌓이기
  전에 큐가 빈다. 변경 전 바이너리에서 탭 부하의 연결 수를 줄이면 간격도 준다(연결 1: 1.99 s,
  연결 4: 3.56 s, 연결 16: 21.3 s).
- headless 는 이 형태가 없다. 메인 루프가 이벤트 **하나마다** due 한 타이머를 돌린다
  (`src/boot.rs` 의 루프). 같은 부하에서 headless 의 타이머 최대 간격은 2.01 s 였다.

## Decision

**gui 의 `IpcReady` 처리기는 직전 회차가 끝난 뒤 한 회차 예산(`ROUND_TIME_BUDGET`)이 지났을 때만
회차를 연다.** 그 전에 온 wake 는 루프를 깨우는 일만 하고, 명령은 곧 올 `about_to_wait` 의 회차가
집는다. 회차가 안 돌면 답이 안 나가 새 명령도 안 오므로 사용자 이벤트 큐는 곧 비고, 루프는
`about_to_wait`(타이머 · 회차 · 렌더)으로 넘어간다. 규칙은 `src/app/ipc.rs` 의 `IpcPacer` 가 들고,
회차가 끝날 때마다(어느 자리에서 돌았든) 끝난 시각을 적는다.

**회차가 예산에서 멈췄으면 루프를 한 번 더 깨운다**(`IpcReady` 를 스스로 보낸다). 남은 명령의 wake
는 이미 건너뛴 이벤트로 소비됐을 수 있어, 안 깨우면 다른 입력이나 다음 타이머가 올 때까지 남는다.
큐가 빈 회차는 깨우지 않으므로 busy-spin 은 없다.

**그 재깨움은 양보 규칙을 건너뛴다 — 단 `about_to_wait` 한 번 사이에 한 번.** 재깨움은 곧바로
처리되므로 16 ms 규칙을 따르면 회차를 못 열고, 그러면 `AboutToWait` 이 안 오는 구간에서 재깨움이
아무 일도 못 한다. 그래서 잘린 회차가 표지를 세우고, 그 표지가 선 동안 온 사용자 이벤트는 규칙과
무관하게 회차를 연다. 면제는 `about_to_wait` 에 닿을 때까지 한 번만 쓴다 — 한도를 두지 않으면
재깨움이 연 회차가 또 잘리고 또 재깨워, 사용자 이벤트 루프 안에서 사슬이 끝나지 않는다. 실측
(같은 부하, 두 번씩): 한도 있음 타이머 최대 간격 2.04 · 2.02 s, **한도 없음 20.06 · 20.96 s** —
기아가 되살아났다. 한도 있음의 부하 명령 수는 3,789 · 3,240 / 20 s, 64 건 버스트는 993 ms 에
끝났다.

**사용자 이벤트 쪽 회차를 없애지는 않는다.** 플랫폼 모달 루프(창 크기 조절 · 메뉴 추적 등)가
`AboutToWait` 없이 사용자 이벤트만 전하는 구간이 있다면, 그 구간에서 IPC 를 살리는 것이 이 경로다.
그런 구간이 macOS · Windows 에 실제로 있는지는 이 머신(x11)에서 잴 수 없다 — 그래서 호환을 지키는
쪽을 골랐다. 한 회차 예산이 지난 wake 는 여전히 회차를 연다.

**headless 는 바꾸지 않는다.** 이벤트마다 타이머를 돌리므로 부하가 타이머를 밀어내지 않고, 남은
명령은 그 명령들이 부른 wake 가 다음 회차를 부른다(ADR-0313 의 근거 그대로).

## Consequences

- **얻은 것**: gui 에서 지속 IPC 부하 중에도 타이머(전역 훅 · busy · idle-timeout · plugin pump
  안전망)와 렌더가 돈다. ADR-0410 의 "회차가 한 프레임보다 오래 루프를 쥐지 않는다" 가 gui 에서
  비로소 참이 된다.
- **잃은 것**: 무거운 부하의 처리량이 준다 — 실측 20 초 명령 수 4,974(시간 예산만) → 3,831(이 결정).
  그 몫이 렌더·타이머로 간 시간이다.
- **잃은 것**: 한 iteration 에 회차가 여럿 돈다 — `about_to_wait` 하나, 재깨움의 면제로 여는 사용자
  이벤트 쪽 하나, 그리고 예산 하나가 지난 뒤 온 사용자 이벤트가 여는 것. 면제는 iteration 당 한 번이고
  나머지는 16 ms 간격이 필요하므로 회차가 쉼 없이 연달아 붙지는 않는다.
- **잃은 것 / 미측정**: `AboutToWait` 이 안 오는 구간이 실제로 있다면, 그 구간에서 잘린 회차의 나머지는
  면제 한 번(회차 하나)만 더 진척하고 그 뒤로는 16 ms 뒤에 오는 다른 사용자 이벤트를 기다린다.
  변경 전에는 wake 마다 회차가 돌아 그런 정체가 없었다. 한도를 풀면 x11 기아가 돌아오므로(위 실측)
  이 둘은 이 설계 안에서 맞바꾸는 관계다. 그 구간이 있는지, 있다면 얼마나 긴지는 macOS · Windows 에서만
  잴 수 있어 미측정이다.
- **미측정**: 재깨움이 **유일한** wake 인 구성을 gui 실행으로 만들지 못했다. 탭 만들기로 쌓은 64 건
  버스트는 재깨움을 끈 빌드에서도 추가 입력 없이 끝났다(1.23 · 1.40 s) — 새 셸의 PTY 출력과 렌더가
  다음 iteration 을 부르기 때문이다. 재깨움 자체(잘린 회차마다 한 번 깨우는가, 그 깨움이 규칙 안에서
  회차를 여는가)는 페이서의 순수 로직이라 단위 시험이 잰다(아래 트리거).

## Alternatives Considered

- **사용자 이벤트 처리기에서 회차를 없애고 `about_to_wait` 에서만 돈다** — 가장 단순하다. 안 고른
  이유: 모달 루프 구간에서 `AboutToWait` 가 안 오는 플랫폼이 있으면 그동안 IPC 가 멈춘다. 이 머신에서
  그 구간을 잴 수 없어, 경로를 남기고 양보만 시키는 쪽이 동작을 덜 바꾼다.
- **wake 를 합친다(대기 중인 `IpcReady` 가 있으면 새로 안 보낸다)** — 이벤트 수가 준다. 안 고른 이유:
  회차가 도는 동안 새 명령 하나가 새 wake 하나를 보내는 것은 그대로라, 사용자 이벤트 루프가 끝나지
  않는 형태가 남는다. 그리고 "대기 중" 표시를 언제 내리는가에 따라 wake 를 잃는 경합이 생긴다.
- **사용자 이벤트 처리기에서 due 한 타이머도 함께 돌린다** — 타이머만은 산다. 안 고른 이유: 창
  입력(x11 이벤트는 같은 iteration 의 앞에서 비운다)과 렌더는 여전히 못 돈다.
- **재깨움의 면제에 한도를 두지 않는다** — 모달 구간에서 잘린 회차의 나머지가 끝까지 이어서
  진척한다. 안 고른 이유: x11 에서 잘림과 재깨움이 사슬을 이뤄 기아가 그대로 돌아온다(실측 20.06 ·
  20.96 s).
- **재깨움이 양보 규칙을 따르게 둔다(면제 없음)** — 가장 단순하다. 안 고른 이유: 재깨움은 곧바로
  처리되므로 거의 늘 16 ms 안이라 회차를 못 열고, `AboutToWait` 이 안 오는 구간에서 재깨움이 뜻을
  잃는다.
- **회차 사이의 양보 간격을 예산과 다른 값으로 둔다** — 파생할 값이 없다. 회차 하나가 쥘 수 있는
  시간과 같은 값을 쓰면 "IPC 가 루프를 쥐는 시간 ≤ 나머지에 돌려주는 기회" 로 읽힌다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 양보 규칙이 빠지거나 방향이 뒤집히면 `a_wake_right_after_a_round_yields_to_the_rest_of_the_loop` ·
  `a_wake_a_budget_after_the_last_round_runs_one` 이 빨개진다.
- 회차가 멈춘 이유를 호출자에게 안 돌려주게 되면 `only_rounds_that_took_something_are_counted` 가
  빨개진다.
- 잘린 회차가 루프를 안 깨우면 `a_cut_round_wakes_the_loop_once_and_a_drained_round_does_not` 이,
  재깨움의 면제가 빠지면 `the_wake_after_a_cut_round_opens_a_round_inside_the_budget` 이, 면제의
  한도가 빠지면 `a_chain_of_cut_rounds_yields_until_the_loop_reaches_about_to_wait` 가 빨개진다(세 변이
  모두 걸어 확인했다). 셋 다 페이서 안의 논리이고, 페이서를 부르는 gui 루프 쪽 배선은 아래 "안 붙는
  것" 에 있다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **gui 에서 지속 부하 중 타이머가 도는가.** 재는 법: 격리 홈 gui 에 `set global-hook --condition
  interval:1 --command "date +%s.%N >> <파일>"` 을 걸고, 연결 16 개로 탭 만들기·목록·닫기를 20 초
  보내는 동안 기록의 최대 간격을 본다. 2 초 남짓이면 돈다.
- **`about_to_wait` 의 `loop_reached_about_to_wait` 호출이 제자리에 있는가.** 어느 시험도 이것을 안
  잰다 — gui 루프를 도는 시험 채널이 없다. 호출을 **지우면** 시험은 통과하지만(`app::ipc::tests` 6 개,
  시험이 그 메서드를 부르므로) 제품 빌드가 `dead_code` 거부로 실패한다(`cargo check -p tasty` rc=101,
  변이로 확인). 채널이 없는 것은 호출을 **잘못된 자리로 옮기는** 경우다 — 조건 갈래 안에 넣거나
  회차 뒤로 옮기면 면제가 드물게만 풀리거나 한 번 쓰이고 말아, 재깨움도 양보하는 동작으로 조용히
  돌아간다. **위 gui 지속 부하 절차로는 안 드러난다** — x11 에서는 면제가 없어도 `about_to_wait` 이
  남은 명령을 집으므로 타이머 간격이 같다(면제 도입 전 바이너리가 2.02 s 였다). 드러나는 자리는
  `AboutToWait` 이 안 오는 구간뿐이라, 재는 법은 아래 모달 루프 절차와 같다 — 부하를 건 채 두 번 이상
  연달아 잘리는 회차 뒤에도 `tasty list info` 가 곧바로 답하는지 본다.
- **macOS · Windows 에서 모달 루프 중 IPC 가 도는가.** 재는 법: 창 크기를 조절하거나 메뉴를 연 채
  `tasty list info` 가 답하는지 본다. 이 경로가 불필요하다고 확인되면 첫째 대안(사용자 이벤트 쪽
  회차 제거)을 다시 본다.
- **winit 이 사용자 이벤트 루프를 한 iteration 안에서 자르게 바뀌는가.** 재는 법: 판올림 때
  `platform_impl/linux/x11/mod.rs` 의 `single_iteration` 을 읽는다. 자르게 되면 이 결정의 양보는
  불필요해진다.

## References

- 전제를 고친 대상: [ADR-0410](0410-a-dispatch-round-also-stops-at-a-time-budget-and-callers-are-served-in-arrival-order.md)
  (회차가 끝나면 gui 는 렌더·타이머로 넘어간다는 문장)
- 관련: [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) (명령마다 wake 한 번이
  이월을 보장한다는 근거 — headless 에서는 그대로다)
- 개정 대상: [ADR-0313](0313-the-dispatch-round-budget-is-the-connection-bound.md) (gui 의 이월 재개 —
  위 근거가 gui 에서는 이 결정으로 바뀐다. 0313 이 이 ADR 을 `부분 개정` 으로 적는다)
- 결정이 실현된 현재 위치: `src/app/ipc.rs` 의 `IpcPacer` · `App::process_ipc`,
  `src/app/event_handler.rs` 의 `IpcReady` 처리와 `about_to_wait` 의 `loop_reached_about_to_wait`, `src/app/ipc_round.rs` 의 `IpcRound::finish`
- 정본 문서: [data-flows](../architecture/data-flows.md) §3 · [timer-hub](../dev-guide/timer-hub.md)
