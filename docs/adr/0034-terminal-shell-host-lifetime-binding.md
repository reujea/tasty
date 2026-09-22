# ADR-0034: 터미널 PTY 셸을 호스트(tasty) 수명에 결박한다

- **Status**: Accepted
- **Date**: 2026-07-04
- **Tags**: process-lifetime, reaper, job-object, pty, terminal, windows, conpty, orphan, cross-platform, adr-0009
- **Group**: terminal-input

## Context

tasty 가 PTY pane 에서 띄운 사용자 셸(bash/zsh 등)과 그 자식 트리(AI 에이전트·빌드·MCP
서버 등)의 정리는 그동안 **PTY hangup 에만 의존**했다. 기존 스탠스(구 `dev-guide/plugin-development.md`)
는 "플러그인 프로세스만 호스트 수명에 결박하고, PTY 사용자 셸은 *사용자 프로세스* 이므로
결박하지 않는다 — PTY hangup 으로 정상 정리된다" 였다.

그 전제("PTY hangup 으로 정상 정리")는 **Unix 에서만 참**이다:

- **Unix**: tasty 종료 시 커널이 PTY master fd 를 닫으며 셸 foreground 프로세스 그룹에
  SIGHUP 이 전달되어 셸 트리가 정리된다. 전제 성립.
- **Windows(ConPTY)**: "pseudoconsole 종료 ⇒ 붙은 프로세스 트리 종료" 보장이 **없다.**
  게다가 Windows 는 Job Object 없이는 부모 사망이 자식을 죽이지 않는다. 따라서 tasty 가
  비정상 종료(하드 크래시·`taskkill /f`·디버거 강제 stop — `Drop` 이 돌지 않는 경로)하면
  셸 트리가 **화면 없는 좀비로 잔존**한다. 개발 중 디버거 stop 마다 수십 개씩 누적되어
  실측으로 bash 155개 + 딸린 conhost 90여 개(약 1.7GB)가 쌓인 상태가 관측됐다.

즉 기존 스탠스는 Unix 기준으로는 옳으나 Windows 에서는 근거가 무너지며, 그 결과가
좀비 셸 누수 버그다. tasty 는 크로스 플랫폼 1급 지원이 정체성(identity.md 2.4)이므로
Windows 에서만 누수가 나는 상태는 허용되지 않는다.

## Decision

**PTY 터미널 셸과 그 자식 트리도 호스트(tasty) 프로세스 수명에 결박한다.** tasty 가 어떤
경로로 죽든(정상 종료·크래시·`taskkill /f`·디버거 강제 stop) **tasty 안에서 돌던 프로세스는
함께 종료된다.**

- 공용 primitive `tasty-reaper` 크레이트를 신설한다. `Terminal::new` 이 셸 spawn 직후
  `tasty_reaper::adopt_pid(child.process_id())` 로 자식을 결박한다.
- **Windows**: 전역 호스트 Job Object(`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). job 핸들을
  프로세스 수명 동안 소유하며 호스트 사망 시 OS 가 job 내 전 프로세스를 강제 종료한다
  (job 멤버십은 자식에 상속되므로 중첩 셸·손자 트리 커버). 부팅 시 `boot.rs` 의 호스트
  진입에서 `init_host_reaper()` 를 1회 호출한다.
- **Unix**: 별도 결박을 추가하지 않고 기존 SIGHUP 자동정리에 의존한다(같은 결과). portable-pty
  `CommandBuilder` 가 `pre_exec` 를 노출하지 않아 PDEATHSIG 설치가 불가하기도 하다.
- 정상 종료 경로 보강: `PtyBackend` 에 `Drop` 을 구현해 surface 닫기/quit 시 셸을 명시적으로
  kill 한다(PTY master HUP 에만 의존하지 않음).
- 플러그인 결박([ADR-0009] 범위의 `PluginReaper`)은 이 primitive 를 공유하되 job 인스턴스는
  `PluginManager` 가 자기 소유로 유지한다(플러그인 job 과 터미널 job 은 별개 인스턴스, 둘 다
  `KILL_ON_JOB_CLOSE` 라 프로세스 사망 시 동일하게 정리).

정체성 정합: identity.md 의 4대 불가침 원칙은 "*에이전트 행동(IPC/CLI)* 이 사용자 상태를
침범하지 않는다"(2.1) 등이며, "호스트 프로세스 자신이 죽을 때 자식 정리" 는 에이전트 행동이
아니라 프로세스 수명 관리라 원칙에 걸리지 않는다. Windows 에서 터미널이 죽은 셸은 재접속
수단이 없어 사용자가 닿을 수 없는 좀비이므로, 결박으로 함께 종료하는 것이 사용자 이익을
해치지 않는다(대부분의 터미널 에뮬레이터가 종료 시 셸을 정리한다).

## Consequences

- **얻은 것**: Windows 좀비 셸/conhost 누수 제거. tasty 종료(모든 경로) 시 tasty 안에서 돌던
  프로세스가 결정적으로 정리된다. 플러그인/터미널이 Job Object 생성 로직을 공유(중복 제거).
- **잃은 것**: tasty 가 죽으면 사용자가 터미널에서 돌리던 `nohup`/백그라운드 작업도 Windows
  에서는 무조건 함께 죽는다(`KILL_ON_JOB_CLOSE` 는 catch/ignore 불가). Unix 는 종전대로
  SIGHUP 이라 이론상 HUP 무시 프로세스는 생존 가능 — OS 간 미묘한 동작 차이가 남는다.
- **운영 비용 / 유지 부담**: 낮음. `tasty-reaper` 는 leaf 크레이트(windows-sys + tracing)이며
  결박 실패는 전부 `tracing::warn!` 로 흡수하고 기존 정리로 degrade 한다.

## Alternatives Considered

- **A. 비결박 유지(구 스탠스) + Unix SIGHUP 의존**: Windows 좀비 누수(= 본 버그)를 방치한다.
  크로스 플랫폼 1급 원칙 위반이라 기각.
- **B. 정상 종료 경로만 정리(`PtyBackend::Drop` kill)하고 결박은 안 함**: 크래시·`taskkill /f`·
  디버거 stop 등 `Drop` 이 안 도는 비정상 경로에서 Windows 셸이 여전히 고아가 된다 — 보고된
  버그의 핵심 경로를 못 막음. 기각(단, Drop kill 자체는 정상 경로 보강으로 함께 채택).
- **C. 부팅 시 이전 인스턴스 좀비 셸 sweep**: 결박 없이 다음 tasty 부팅 때 고아 셸을 청소.
  "어느 셸이 진짜 고아인가"(다른 살아있는 tasty 의 셸과 구분)가 근본적으로 racy 하고, 좀비가
  다음 부팅까지 잔존한다. 결박이 더 결정적이라 기각.
- **D. 단일 Job 으로 플러그인+터미널 통합**: 정리 정확성 이득이 없고(둘 다 KILL_ON_JOB_CLOSE)
  플러그인 job 소유가 `PluginManager` 수명에서 프로세스 전역으로 바뀌어 커플링만 증가. 기각 —
  primitive 만 공유하고 job 인스턴스는 각자 소유.

## Reconsideration Triggers

- **display 프로세스와 PTY 소유 데몬의 분리**: 현재는 호스트(standalone GUI 또는 headless 데몬)가
  PTY 를 직접 소유하고 결박도 그 호스트에 건다. 만약 tmux 서버처럼 *PTY 를 소유하는 영속 데몬* 과
  *thin display 클라이언트* 를 분리하는 구조로 가면, 결박 대상은 반드시 **PTY 를 실제 소유하는
  데몬** 이어야 하고(디스플레이 클라이언트 종료가 셸을 죽여선 안 됨) — host job 을 어느 프로세스가
  소유하는지 재검토한다. (단 이는 client attach/detach 와 무관하다: 현행 headless 데몬 + attach
  모델에서도 셸은 데몬에 결박되며 GUI/SSH 클라이언트의 attach/detach 는 셸 수명에 영향을 주지
  않는다. "tasty 가 소유한 PTY 의 셸을 tasty 사후 재연결" 은 ConPTY/pty 구조상 불가능하므로 —
  고아 셸은 재연결 수단이 없는 좀비 — 재연결을 위해 결박을 빼는 시나리오는 성립하지 않는다.)
- portable-pty(또는 대체 PTY 백엔드)가 `pre_exec`/자식 결박 훅을 제공해 Unix 도 명시적 결박이
  가능해지면 OS 간 동작 차이를 없애는 방향으로 재검토.
- ConPTY 가 "pseudoconsole 종료 ⇒ 자식 트리 종료" 를 보장하는 API 를 제공하면 Windows Job
  결박의 필요성 재평가.

## References

- [`dev-guide/plugin-development.md`](../dev-guide/plugin-development.md) — 프로세스 수명 결박(플러그인 + 터미널)
- [`identity.md`](../identity.md) — 크로스 플랫폼 1급(2.4), 사용자/에이전트 분리(2.1)
- [ADR-0009](0009-plugin-sandbox-deferred.md) — 플러그인 spawn/sandbox 스탠스
- `crates/tasty-reaper` — 공용 결박 primitive + 전역 호스트 job
- commit `feat(reaper)` / `fix(terminal)` / `refactor(host-plugin)`
