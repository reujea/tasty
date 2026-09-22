# ADR-0211: PTY 종료 대기의 상한은 경주 예산이 아니라 안전망이다 — 실패문이 두 사건을 스스로 가른다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: testing, flaky, pty, ci, macos, diagnostics, measurement, adr-0129, adr-0181, adr-0217, adr-0206
- **Group**: testing

## Context

자체 호스팅 macOS 러너의 `check-macos` 잡에서 `tasty-terminal` 의
`process_exited_eventually_emitted` 가 상한을 다 쓰고 빨개졌다. 같은 모양이 하루 전
같은 러너에서 다른 시험 가족(`src/adapters/ipc/handler/pty.rs` 의 `wait_for_exit` 를
쓰는 두 시험)에도 났다 — 그쪽도 상한을 다 썼다.

우리에게 macOS 러너가 없어 그 자리에 붙을 수 없다. 그래서 이 회차에 잰 것은 셋이다.

**① 이력 — 모수는 "그 시험이 실제로 실행된 run" 이다.** `check-macos` 가 빨간 run 은
그 잡이 배선한 커버리지가 실패가 아니라 **미측정**이다([ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md) 의 층 구분과 같은 이유).
받은 macOS 잡 로그 76 건 중 이 시험이 실제로 실행된 것은 27 건이고(나머지 49 건은 앞
스텝이 죽어 시험 스텝에 도달하지 않았다), 그중 실패는 1 건이다.

**② 그 시험 바이너리의 소요시간 분포에 중간값이 없다.** 실행된 27 건에서 26 건이
0.78~1.42 초 구간에 모여 있고, 나머지 1 건이 상한을 다 썼다. 그 사이에 표본이 하나도
없다. "러너가 굶어서 느려졌다" 면 그 구간에 표본이 쌓여야 하는데 쌓이지 않았다.

**③ 실패의 모양은 리눅스에서 그대로 재현된다 — 자식이 안 죽게 두면 된다.** 자식 셸을
종료하지 않는 것으로 바꿔 같은 시험을 돌리면 같은 상한 소진으로 죽고, 그동안
`Terminal::process` 의 `try_wait` 폴백은 `ALIVE_CHECK_INTERVAL` 주기대로 61 회 돌았다
(`strace -e trace=wait4` 계수와 시험이 센 `process()` 횟수가 같은 값을 냈다).
**즉 이벤트(waker) 축도 폴 축도 죽지 않았다.** 시험의 머리 주석이 설명하던 두 축은
"자식이 죽은 것을 언제 보나" 만 답하고, **자식이 죽는다는 것 자체**는 답하지 않는다.
그것은 주석에 이름이 없는 기계에 걸려 있다 — 로그인+대화형 셸이 그 러너에서 뜨는 것,
그리고 spawn 뒤에 쓰는 `initial_input` 이 그 셸의 첫 입력으로 들어가는 것.

한편 그 뒤쪽(입력 유실)은 리눅스에서 **찾지 못했다.** zsh·bash 를 `-li` 로 띄우고
spawn 과 write 사이 지연을 0 ms 부터 150 ms 까지 여덟 단계로 두 번씩(32 시행) 재는 동안
쓴 글자가 유실된 경우는 없었다. 배제한 것이지 macOS 에서 배제한 것은 아니다.

그리고 유일하게 남아 있던 macOS 쪽 관측 — 실패 시 화면 꼬리 — 은 **잘못 읽히고 있었다.**
그 진단문은 화면에 글자가 있으면 "셸은 떴다" 고 적었는데, PTY 의 에코는 자식이 아니라
라인 디시플린이 낸다(리눅스 실측: 자식을 tty 를 읽지도 쓰지도 않는 프로그램으로 두고
master 에 쓴 글자가 그대로 되읽힌다). 그 화면이 실제로 말하던 것은 반대였다 —
자식이 프롬프트를 한 글자도 안 뱉었다는 것.

## Decision

**상한은 그대로 둔다. 상한 인상은 이 사건의 처방이 아니다.** 근거는 ② 다 — 정상 구간의
최댓값(1.42 초)과 상한 사이에 표본이 없어서, 상한을 올려도 아무것도 더 잡히지 않는다.
올리려면 먼저 그 구간에 표본이 생겨야 하고, 그때 재는 것은 상한이 아니라 그 표본이다.

**대신 실패문이 두 사건을 스스로 가르게 한다.** 붙어서 못 보는 러너에서 나는 빨강은
그 자리에서 답을 남겨야 한다. 실패 갈래에서만 세 값을 싣는다 — `process()` 를 몇 회
돌렸는가(폴백이 돌 자리가 있었는가), 마지막에 **자식이 살아 있었는가**, 화면 꼬리.
가르는 것은 둘째 값이다: `alive=true` 면 자식이 안 죽은 것이라 감지가 아니라 자식을
봐야 하고, `alive=false` 면 죽었는데 감지가 못 본 것이다.

**화면 내용은 자식이 떴다는 증거로 쓰지 않는다.** 우리가 보낸 것의 에코를 뺀 나머지가
남을 때만 자식이 무엇인가 뱉었다고 적는다. 갈래는 셋이다 — 에코조차 없다 / 에코뿐이다 /
에코 밖이 있다.

## Consequences

- **얻은 것**: 다음 빨강이 원인 자리를 스스로 지목한다. 그리고 틀린 처방 둘이 문서로
  막힌다 — 상한 인상과, 에코를 자식의 출력으로 세는 것.
- **잃은 것**: 이 회차에 macOS 쪽 원인 자체는 못 좁혔다. 좁히려면 다음 실패 한 건이
  더 필요하다 — 그 한 건이 위 세 값을 들고 온다.
- **운영 비용 / 유지 부담**: 관측은 실패 갈래에서만 꺼내므로 정상 경로에 비용이 없다.
  대신 시험이 보내는 글자와 실패문이 짝을 이뤄야 한다 — 보내는 글자를 바꾸면 에코 판정의
  입력도 함께 바꿔야 한다.

## Alternatives Considered

- **상한을 올린다**: 30 초를 60 초로. — 분포에 근거가 없다(②). 상한을 올려서 통과시키는
  형태이기도 하다. 기각.
- **자식을 셸이 아닌 것(스스로 종료하는 프로그램)으로 바꿔 결합을 끊는다**: 로그인 셸
  기동과 `initial_input` 전달을 시험에서 들어낸다. — 이 크레이트에서 `initial_input` 이
  자식의 첫 입력으로 들어가는지를 재는 자리가 여기 하나뿐이라, 그 커버리지가 사라진다.
  더 나쁜 것은 관측 자리가 사라지는 것이다 — 지금 유일하게 이 현상을 보여 주는 시험을
  현상이 안 보이게 고치는 셈이다. 기각.
- **재시도(retry)나 `#[ignore]`**: 빨강을 없앤다. — 위와 같은 이유로 기각. 관측을 지우는
  쪽이고, ②·③ 을 모으는 데 쓴 값도 그때는 안 남는다.
- **에코 판정을 시험 크레이트에도 한 벌 더 둔다**: 같은 물음에 사본을 둘로 만든다.
  종료 대기 쪽은 `alive` 한 값이 답을 정하므로 판정이 필요 없다 — 화면 꼬리는 원문 그대로
  싣고 "에코일 수 있다" 는 사실만 문장으로 붙인다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다. **셋이 한 갈래가 아니다.**

**원리적으로 안 붙는 것** — CI 실행 중에만 나타나는 관측치라 사람이 재야 한다. 그 대신
재는 법을 함께 적는다.

- **정상 구간과 상한 사이에 표본이 생기면** — "느려서 못 잡았다" 가 처음으로 사실이 되고,
  그때는 상한을 다시 정한다. 재는 법: `check-macos` 잡 로그에서 그 시험 바이너리의
  `test result` 줄에 붙는 `finished in` 값을 실행된 run 전수로 모은다.
  - **같은 조건을 보는 둘째 관측(후속 트랙에서 확정)**: `pty.rs` 쪽 실패문의 watcher 위상이
    `Reaped` 로 나오는 것. 그 조합(대기는 `None`, cell 은 참)이 나는 경로는 하나뿐이다 —
    대기가 상한을 다 쓴 **직후**에 자식이 끝나는 것. 대기가 놓치는 경로는 없다 — 깨우기가
    통째로 없어도 만료 후 재검사가 잡는다(park 중 채움 100 회에 유실 0, 그리고 `notify_all`
    을 지운 변이에서도 판정이 안 바뀌고 벽시계만 늘었다). 즉 이 위상은 "자식이 끝나긴 했는데 상한을 막 넘겼다" 를
    직접 말하므로, 위 `finished in` 수집을 기다리지 않고 그 자리에서 트리거가 선다.
- **실패문이 `alive=false` 로 나오면** — 자식이 아니라 감지가 문제다. 그때 여는 것은
  상한이 아니라 `Terminal::process` 의 종료 감지 경로다. 재는 법: 그 시험이 빨간 run 의
  실패문에서 `자식 alive=` 뒤의 값을 읽는다(`false` 면 이 트리거, `true` 면 자식이 안
  죽은 것이라 위 첫 트리거 쪽이다). 실패문이 두 사건을 스스로 가르므로 로그 한 줄로 끝난다.
**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- **`initial_input` 을 쓰지 않는 형태로 그 시험이 바뀌면** — 이 결정이
  지키던 커버리지가 사라지므로 위 "기각" 의 전제가 무너진다. 이 조건의 주어는 CI 관측치가
  아니라 **소스의 선언**이다 — `crates/tasty-terminal/src/tests.rs` 의
  `process_exited_eventually_emitted` 가 `TerminalConfig` 에 `initial_input: Some(..)` 을
  주는가. 판정 시점에 레포가 읽을 수 있으므로 `template.md` 기준으로 채널이 붙는다.
  아직 안 지었다.

## References

- **일반 진술은 [ADR-0217](0217-a-precondition-lives-in-the-failure-text-not-in-prose.md) 이다** — 판별에 필요한 정보는 실패 문구 자신이 져야 한다. 이 결정은 그 원리를 **상한(예산)** 축에 적용한 것이다: 상한을 다 쓴 것은 경주에서 진 것과 죽은 것을 못 가르므로, 실패문이 두 사건을 스스로 갈라야 한다.
- 형제: [ADR-0206](0206-a-refusal-is-told-apart-by-its-own-words.md) — 같은 원리를 **종료 코드** 축에 적용한다. 포함 관계가 아니라 형제다 — 축이 다르다.
- [ADR-0129](0129-flaky-test-classes-and-standard-fixes.md) — 간헐 실패의 분류와 표준 처방
- [ADR-0181](0181-a-latency-assertion-must-carry-a-control-that-load-moves-and-code-does-not.md) — 벽시계 예산을 단정에 쓰는 것의 조건
- [ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md) — 잡이 빨간 동안 그 커버리지는 실패가 아니라 미측정이다
- [ADR-0194](0194-code-citations-name-symbols-not-line-numbers.md) — 코드 인용은 심볼 이름으로
- 코드(결정이 실현된 현재 위치): `crates/tasty-terminal/src/tests.rs` 의
  `process_exited_eventually_emitted`, `crates/tasty-terminal/src/lib.rs` 의
  `Terminal::process`·`ALIVE_CHECK_INTERVAL`, `crates/tasty-terminal/src/accessors.rs` 의
  `Terminal::check_process_alive`, `src/adapters/ipc/handler/pty.rs` 의
  `screen_holds_more_than_our_echo`·`watch_phase_note`·`observed_pty_state`·`exit_wait_failure`,
  `src/core/pty_registry.rs` 의 `WatchPhase`·`PtyEntry::watch_phase`·`PtyRegistry::attach_exit_watcher`
