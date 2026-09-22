# ADR-0077: 종료를 프레임 구동 상태 머신으로 전개하고 부팅과 같은 로딩 화면을 씌운다

- **Status**: Accepted
- **Date**: 2026-08-23
- **Tags**: shutdown, boot, ui, state-machine, plugin
- **Group**: window-workspace-lifecycle

## Context

종료 cascade 는 한 콜스택 안에서 동기로 돌았다. 그 사이 이벤트 루프로 제어가 한
번도 돌아오지 않으므로 창은 **마지막 프레임 그대로 얼어붙는다**.

얼어 있는 시간은 짧지 않다. [종료 시퀀스](../architecture/shutdown-sequence.md) 의
실측에 따르면 번들 plugin 이 붙은 일반 케이스에서 S4(plugin 종료 대기)가 ≈2.0~2.1s
평탄하고 — 대기를 겹쳐도 개별 plugin 이 2s graceful deadline 을 다 채우고 force
kill 되기 때문이다 — 여기에 Drop tail 100~520ms 가 붙어 체감 종료가 2.1~2.3s 다.
2 초 넘게 정지 화면을 보여 주면서 아무 신호도 주지 않는 것은 크래시와 구분되지
않는다.

부팅에는 이미 대응물이 있다. `BootPhase` 상태 머신이 단계를 프레임 단위로 전개하고
`render_loading` 이 워드마크 + 스피너 + 단계 문구를 매 프레임 그린다. 종료에는 그
대칭이 없었다.

여기서 화면보다 먼저 걸리는 제약이 있다 — **화면만 얹으면 스피너가 멈춘 정지
프레임이 된다.** 종료가 동기 콜스택인 한 프레임이 돌지 않기 때문이다. 즉 이
문제는 렌더 문제가 아니라 제어 흐름 문제다.

## Decision

종료를 `ShutdownPhase` 상태 머신(`src/app/shutdown_machine.rs`)으로 쪼개
`about_to_wait` 워치독이 프레임 단위로 전개하게 하고, 대기가 남아 있는 프레임마다
부팅과 **같은** `render_loading` 을 phase 문구만 바꿔 present 한다. 블로킹이던 두
대기(부팅 워커 회수, plugin 자식 회수)는 각각 `try_recv` + deadline 폴링과
`begin_shutdown_all()` / `poll_shutdown_all()` 조합으로 논블로킹화한다.

세부 결정 넷:

1. **락업은 부팅과 동일하다.** `render_loading` 은 phase 타입이 아니라 i18n 키를
   받고, 두 상태 머신이 각자의 phase → 키 매핑을 소유한다. 종료 전용 시안을 새로
   받지 않는다 — 같은 브랜드 락업에 문구만 다른 화면이라 구조 전사 대상이 이미
   있다.
2. **그릴 창이 없으면 상태 머신을 설치하지 않는다.** macOS 최소화(창 파괴 + park)나
   트레이 hide 상태의 종료는 그 자리에서 끝까지 블로킹으로 돈다(= 이 결정 이전과
   동일한 동작). 창 없이도 이벤트 루프가 계속 깨어난다는 보장이 플랫폼마다 달라,
   보여 줄 화면이 없는 경로에서 구동 방식까지 바꿀 이유가 없다.
3. **창이 여럿이면 전부 종료 화면으로 바꾼다.** 하나만 그리고 나머지를 먼저 닫으면
   사용자에겐 창이 하나씩 사라지는 것으로 보여 크래시와 구분되지 않는다.
4. **한 프레임에 여러 스텝을 돈다.** `drive_shutdown_frame` 은 더 진행할 수 없을
   때까지 스텝을 반복하므로, 대기가 없는 종료(plugin 0 개 — 실측 0.63ms)는 첫
   구동에서 완료에 도달해 **화면이 한 프레임도 그려지지 않는다.** 최소 표시 시간
   타이머나 지연 표시 타이머가 필요 없다.

종료 진행 중에는 steady-state 파이프라인(IPC 처리 / intent drain / plugin pump)을
태우지 않고 `AppEvent` 는 **폐기**한다(부팅 가드는 지연 후 재생하지만, 종료에는
재생할 미래가 없다). IPC 연결은 계속 accept 되지만 응답이 없고, Drop tail 초반
S5d 에서 서버가 drop 되며 끊긴다.

## Consequences

- **얻은 것**: 종료 대기 2 초가 정지 프레임이 아니라 회전하는 스피너 + 단계 문구가
  된다. 부팅 중 종료 경로에서 회수한 `PluginManager` 가 그 자리에서 블로킹
  `shutdown_all` 되지 않고 S4 폴링으로 넘어가, 그 구간에도 화면이 돈다.
- **잃은 것**: `event_loop.exit()` **이후**의 Drop tail(실측 36~154ms, 환경에 따라
  최대 520ms)은 덮이지 않는다. 창이 이미 사라진 뒤라 어떤 화면으로도 덮을 수 없고,
  체감 종료 시간의 하한으로 남는다.
- **운영 비용 / 유지 부담**: 종료 단계 본문(`shutdown_cascade.rs`)과 순서·대기
  (`shutdown_machine.rs`)가 분리되어, 새 단계를 넣을 때 **논블로킹 계약**을 지켜야
  한다. 블로킹 호출을 스텝 안에 넣으면 그 구간만 다시 얼어붙는다(컴파일러가 잡아
  주지 않는다). 계측 마커는 동기 시절과 같은 지점·같은 이름을 유지하므로 기존
  실측 기준치와 계속 비교 가능하다 — 다만 부팅 중 종료에서 **S4/S4a 가 더 이상 S2
  안에 중첩되지 않고 S2 뒤에 나란히** 나온다.

## Alternatives Considered

- **A: 종료 cascade 를 별도 스레드로 옮기고 메인 스레드는 계속 렌더**  — plugin
  manager / CoreState / view 가 전부 `!Send` 이거나 메인 스레드 소유라 옮길 수
  없다. 옮기려면 소유 구조를 갈아엎어야 하는데, 얻는 것은 같다.
- **B: 화면만 얹고 대기는 그대로 동기 유지** — 스피너가 멈춘 정지 프레임이 된다.
  "지금 종료 중" 신호로는 얼어붙은 마지막 프레임보다 낫지만, 멈춘 스피너는 오히려
  hang 으로 읽힌다.
- **C: 최소 표시 시간(예: 300ms)을 두어 화면 깜빡임 방지** — 빠른 종료를 일부러
  느리게 만드는 거래다. 결정 4(스텝 루프)로 깜빡임 자체가 구조적으로 사라지므로
  거래할 이유가 없다.
- **D: Drop tail 까지 덮도록 종료 화면 창을 별도로 띄운다** — 종료 직전에 새 창을
  만들어 destructor 구간을 덮는 방법. 창 생성/파괴 비용이 Drop tail 과 같은
  자릿수라 순이득이 불확실하고, "종료 중에 새 창이 뜬다" 는 동작 자체가 사용자
  상태(포커스/창 목록)를 건드린다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin 종료가 실제로 graceful 로 빠지게 되어 S4 가 수백 ms 대로 내려가면 — 종료
  화면이 보이는 구간이 사라져, 화면 자체가 순수 비용이 된다.
  **충족(2026-09-23 실측, 재검토 미결):** SDK 가 shutdown 요청에 스스로 빠지게 된 뒤 Linux
  Xvfb debug GUI 에서 번들 9 개 전부 running 상태로 `system.shutdown` 3 회 — 9/9 graceful,
  S4 3.44 / 3.78 / 3.70 ms, `shutdown_total` 4.9~5.0 ms([종료 시퀀스 실측 기준치](../architecture/shutdown-sequence.md#실측-기준치)).
  응답하지 않는 plugin 이 있을 때의 S4 상한은 여전히 2s deadline 이다.
- Drop tail 이 S4 를 넘어 체감 종료의 주된 구간이 되면 — 덮이지 않는 쪽이 커지므로
  Alternative D 를 다시 저울질해야 한다.
  **충족(같은 실측):** S5(Drop tail) 60~64 ms 가 S4 3.4~3.8 ms 를 넘어, 체감 종료
  (with_drop) 66~70 ms 의 대부분이다.
- 종료 단계에 논블로킹으로 만들 수 없는 대기(외부 동기 API 등)가 추가되면.

## References

- [종료 시퀀스](../architecture/shutdown-sequence.md) — 단계·마커·실측 기준치
- [부팅 시퀀스](../architecture/boot-sequence.md) — 대칭 대상인 `BootPhase` 상태 머신
- [ADR-0076](0076-close-path-per-surface-blocking-removal.md) — close 경로에서 per-surface 블로킹을 걷어낸 선행 결정
- [ADR-0078](0078-shutdown-rejects-pending-ipc.md) — 위 Decision 의 "종료 중 IPC 는 응답이 없다" 절을 개정(무시 대신 즉시 거절 + exit 이후까지 가드 유지)
