# ADR-0172: 뒤에 로컬 정리가 있는 훅 핸들러는 host 호출 실패를 전파하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: plugin, error-handling, hook, agent-integration, adr-0075, adr-0092
- **Group**: plugin-system

## Context

같은 훅 이벤트를 죽은 surface 에 보내면 번들 plugin 둘이 다르게 답한다.

| 호출 | 살아있는 surface | 죽은 surface |
|---|---|---|
| `claude.hook session-end` | `{"ok":true,…}` | `{"ok":true,…}` — **바이트 동일** |
| `codex.hook stop` | `{}` | `-32602 no live surface N (named by 'terminal.set_state')` |

짝 핸들러 8 쌍(`parent`·`state`·`children`·`kill`·`tell`·`broadcast`·`respawn`·`hook`)
중 갈라지는 것은 `hook` 1 쌍뿐이라, 처음에는 표류로 보였다.

재보니 **검증에서 갈리는 것이 아니다** — 어느 쪽도 대상 surface 를 검증하지 않는다.
같은 host 호출들 중 **무엇을 전파하느냐**가 다르다.

- `tasty-plugin-codex` 의 `handle_hook`: `surface.meta.set` → `warn!`,
  `terminal.set_state` → `?` **전파**, `surface.fire_hook` → `warn!`.
- `tasty-plugin-claude` 의 `handle_claude_hook`: 전부 `deliver()` 를 지나고 `deliver`
  는 `warn!` 만 한다. 하나도 전파하지 않는다.

그리고 그 차이에는 이유가 있다. `handle_claude_hook` 은 host 호출 루프 **뒤에** 로컬
정리를 한다 — session-end 의 `checklist::remove_state_for_session` ·
`profile_attach::mark_ended`, session-start 의 `profile_attach::store`/`sweep`. 이 넷은
`HostHandle` 을 받지 않는 순수 파일시스템 작업이라 **surface 가 없어도 정의된다.**
codex 의 `?` 뒤에는 지킬 로컬 상태가 없다(이어지는 `fire_hook` 자체가 최선노력).

둘 중 어느 자리에도 왜인지가 소스에 없었다. 그것이 이 ADR 이 다루는 결함이다.

## Decision

**`claude.hook` 은 지금대로 전파하지 않는다. 동작은 바꾸지 않고, 판별식을 기록한다.**

> **그 host 호출 뒤에 이 핸들러가 지켜야 할 로컬 상태가 있는가. 있으면 최선노력,
> 없으면 전파.**

`?` 로 끊으면 뒤따르는 로컬 작업이 **호출이 실패한 그 경우에만** 건너뛰어진다. 그
로컬 작업이 "대상이 사라졌을 때의 정리" 라면, 정리가 필요한 유일한 경우에만 정리가
안 돈다. 그래서 두 핸들러의 차이는 표류가 아니라 **같은 규칙의 양끝**이다.

판별식 본문과 두 실례 표는
[error-handling](../dev-guide/error-handling.md) "plugin 핸들러의 host 호출 —
전파와 최선노력" 에 두고, 두 호출 자리에는 그 절을 가리키는 주석을 박았다.

**빈도는 이 판정의 근거가 아니다.** 실측(2026-09-05)에서 dead-surface 실패는 오히려
드물었다 — 아래 Consequences 의 수를 보라. 근거는 대가의 비대칭이다: 전파해도 훅
명령이 `|| true` 로 감싸여 있어(`crates/tasty-plugin-claude/src/install.rs` 의
`hook_command`) 호출자에게 닿지 않는데, 대신 orphan 정리가 통째로 안 돈다. **얻는 것이
0 에 가깝고 잃는 것이 무한정인 교환**이다.

## Consequences

- **얻은 것**: 죽은 surface 에 훅이 도착해도 로컬 정리가 돈다. 그 시나리오가 가능한
  이유는 close 순서다 — 호스트가 레이아웃에서 surface 를 먼저 지우고
  (`close_surface_by_id_inner`) 그 다음 `cleanup_surface` → `drop_terminal` 로 PTY 를
  떨구므로, PTY 사망 뒤에 도착하는 훅은 이미 없는 surface 를 가리킨다.
- **얻은 것**: 다음 핸들러가 같은 갈림길에서 취향으로 정하지 않는다. 판별식이 두 자리
  밖에 있다.
- **잃은 것**: **claude 훅 응답은 host 호출이 전부 실패해도 살아 있을 때와 바이트가
  같다.** 호출자는 반영 여부를 응답으로 알 수 없다. 유일한 흔적은 plugin 로그의
  `warn!` 다(`<tasty_home>/plugins-logs/com.tasty.claude.log`).
- **운영 비용**: 없음. 동작 변경이 아니다.

### 실측 (2026-09-05)

**채널이 살아 있다(양성 대조).** 격리 인스턴스에서 한 번 살았던 surface 를 닫고 같은
훅을 보내면 그 로그에 `claude hook host call '<method>' failed: …` 8 줄이 나온다
(`terminal.set_state` ×2 · `surface.meta.set` ×2 · `surface.meta.unset` ×2 ·
`surface.fire_hook` · `surface.completion`). 즉 아래 0 은 "안 재진 0" 이 아니다.

**실사용 빈도는 0 이다.** 이 `warn!` 은 2026-05-13 부터 있었고, 실사용 인스턴스의
plugin 로그는 한 plugin 수명(`connected to host` 1 줄) 동안 2026-08-27 ~ 09-04 의
8 일을 담고 있다. 그 안에 session-end 70 건, host 호출 실패 **0 건**.

## Alternatives Considered

- **A: codex 처럼 죽은 surface 를 거절한다** — 로컬 정리가 통째로 안 돈다. 그것도
  정리가 필요한 유일한 경우에만. 게다가 훅 명령의 `|| true` 가 에러를 삼키므로
  호출자는 그 거절을 보지도 못한다. 얻는 것 없이 잃기만 한다.
- **B: 응답에 가산 필드를 실어 실패를 노출한다**(`host_calls: {delivered, failed}` 류)
  — 기존 소비자에 무해한 순수 가산이라 기술적으로는 싸다. 하지만 **사는 것이 지금
  0 이다**: 최선노력 실패가 조용히 사라져 누가 헤맸다는 관측이 없고, 위 실측이
  8 일 · 70 건 · 0 실패다. 와이어 스키마는 한 번 늘리면 못 줄이므로, 필요가 셀 수
  있게 나타난 뒤에 한다. 재검토 조건에 명령으로 박아 둔다.
- **C: codex 쪽을 claude 처럼 전부 최선노력으로 바꾼다** — codex 의 `handle_hook` 은
  state 주입이 하는 일의 전부라, 실패를 삼키면 그 핸들러는 아무것도 보고하지 않는
  함수가 된다. 판별식이 정확히 반대 답을 내는 자리다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **최선노력 실패가 실제로 발생한다.** 재는 명령 —
  `grep -c "hook host call" ~/.tasty/plugins-logs/com.tasty.claude.log`
  (로그는 plugin 수명 단위다. 모수는 같은 파일의
  `grep -c "session-end" ~/.tasty/plugins-logs/com.tasty.claude.log`).
  0 이 아니면 대안 B 를 다시 본다.
- `handle_claude_hook` 의 host 호출 **뒤**에서 로컬 정리가 사라진다 — 판별식이 반대
  답을 내므로 전파로 바꾼다.
- 훅 명령의 `|| true` 가 없어져 비-0 exit 가 호출자에게 닿게 된다 — 전파의 값이
  0 이 아니게 된다.

## References

- [error-handling](../dev-guide/error-handling.md) — "plugin 핸들러의 host 호출 —
  전파와 최선노력" (판별식 본문과 두 실례)
- [ADR-0075](0075-agent-hook-delivery-failure-record.md) — 훅 **전달** 실패의 기록
  채널(`hook-failures.log`). 이 ADR 이 다루는 것은 전달이 성공한 뒤의 host 호출 실패라
  그 채널에 안 남는다.
- [ADR-0092](0092-file-log-host-process-only.md) — plugin/CLI 로그가 어디로 가는지
