# ADR-0266: 관측으로 파생된 정지(`stale`)는 조회뿐 아니라 push 알림에도 도달한다

- **Status**: Accepted
- **Date**: 2026-09-12
- **Tags**: plugin, claude, child-terminal, agent-state, hooks, notification, stall, adr-0072
- **Group**: agent-integration

## Context

부모 에이전트는 "자식이 끝나면 알림이 온다" 는 전제로 동작한다 — 완료 알림은
`<TASTY_PARENT_HOME>/notify/<caller_surface>.log` 한 줄로 오고, 부모는 그것을
`Monitor` 로 기다린다([`docs/dev-guide/external-interaction.md`](../dev-guide/external-interaction.md#child-완료-알림--completion-log)).
그 전제가 성립하지 않는 갈래가 하나 있다. **자식이 승인 프롬프트 같은 자리에서 멈췄고
훅이 오지 않으면 부모에게 아무것도 가지 않는다.** 호스트는 그 자식을 `stale` 로
판정하지만([ADR-0072](0072-child-state-hook-observation-fusion.md)), 그 값은 부모가
**물었을 때만** 보인다. 부모가 묻지 않는 이유는 알림을 기다리고 있기 때문이고, 그
알림이 안 오는 상황이 바로 `stale` 이다.

**훅이 오는 갈래는 정상이다.** `crates/tasty-plugin-claude/src/hook.rs` 는
`Notification`(`idle_prompt` 제외)과 `PreToolUse`/`AskUserQuestion` 에서
`SetState{needs_input}` + `FireHook{needs-input}` + `SurfaceCompletion{needs_input}`
세 벌을 한꺼번에 내고, 부모는 그것으로 깨어난다. 전제는 `tasty claude install` 로
`~/.claude/settings.json` 에 훅이 등록돼 있는 것이다
(`crates/tasty-plugin-claude/src/install.rs` 의 `MANAGED_HOOKS`).

**훅이 오지 않는 갈래(미설치·유실·세션 시작 이전 단계)에서는 차단이 두 층이다.**
`crates/tasty-plugin-claude/src/error_scan.rs` 의 `scan_one_at` 이
`self.track_output(...)` 직후 `if !detect_claude_error(text) { return None; }` 로
빠져나가고, `maybe_notify_stall` 호출은 그 **뒤**에 있다 — 승인 프롬프트는 에러
문자열이 아니므로 정지 판정이 **한 번도 돌지 않는다**(층 1). 도달하더라도 같은 파일의
`should_notify_stall` 이 `child_state == "active"` 를 요구해 `stale` 을 배제한다(층 2).
그 게이트의 주석은 배제 이유를 "`idle`/`needs_input`/`exited` 는 턴이 이미 끝나
완료 알림 경로가 부모에게 이미 알렸다" 로 적는데, **`stale` 은 그 목록에 없고 그런
알림 경로도 없다** — `stale` 이 나온다는 것 자체가 훅이 유실됐다는 뜻이기 때문이다.

이 모듈의 원래 목적이 좁았던 것이 원인이다. 도입 당시 제목이 "notify parent when a
child stalls **after an API error**" 였고, 매니페스트의 이벤트 설명도 같은 범위를
적는다. 그리고 `stale` 문자열을 소비하는 자리는 전부 **조회 응답**이다 —
`src/adapters/ipc/handler/terminal.rs` 의 `liveness_fields`(`terminal.state` /
`terminal.children` 공통 직렬화)와 `crates/tasty-plugin-agent-common/src/children.rs`
의 `spawn_census`(그나마 `confirmed` 만 센다).

**이것은 의도적 기각이 아니다.** ADR-0072 는 파생 상태를 만들고 두 **조회** 경로가
그것을 공유하게 한 결정이고, "파생 상태는 출력 전용" 이라는 제약은
`terminal.set_state` 의 **입력**으로 받지 않는다는 뜻이지 push 알림을 금지한 것이
아니다. 본문 어디에도 push 채널에 대한 결정이 없다. 시간 순서도 그렇다 — stall
watchdog 이 ADR-0072 보다 **뒤인데도** `stale` 을 언급하지 않는다. 서로 다른 목적의
기능이 독립적으로 자라며 생긴 사각지대다. 이 ADR 은 ADR-0072 를 **잇는다** — 그
판정을 그대로 쓰고, 그 값이 닿는 곳을 하나 늘린다.

## Decision

`stale` 을 포함한 정지 판정이 **부모의 push 채널까지 도달**하게 한다. 관측·발사는
지금 있는 plugin 폴링 배관이 그대로 하고, 호스트에는 새 경로를 만들지 않는다. 일곱
가지를 정한다.

1. **알리는 주체는 plugin 폴링 루프다.** `crates/tasty-plugin-claude/src/error_scan.rs` 가 매 tick 하던 일
   (출력 흐름 관측 → 값싼 게이트 → `terminal.state` 조회)을 그대로 쓰고, 호스트는
   판정 결과를 **조회로 답하는 역할만** 유지한다. 호스트가 상태 전이를 스스로
   push 하는 형태는 고르지 않는다(대안 B) — 새 intent · 직렬화 · 구독 배관이 필요한데,
   그 셋이 사는 자리는 이 사각지대와 무관하게 이미 붐빈다.
2. **이벤트 키는 기존 `claude-error-stalled` 를 그대로 쓴다.** 키는 부모가 `hook.set`
   으로 이미 등록해 둔 **배선 식별자**라, 개명은 등록된 모든 훅과 문서의 이벤트
   카탈로그를 동시에 깨뜨리면서 기능적으로 얻는 것이 없다. 대신 매니페스트의
   `description` 에서 "after an error" 를 걷어내 범위를 사실대로 적고, **원인은
   알림 문구로 가른다** — 에러 뒤 정지와 에러 없는 정지는 부모가 다르게 대응하므로
   한 줄 안에서 구별돼야 한다.
3. **휴리스틱 판정도 알린다.** `stale` 이면 `confidence` 가 `confirmed` 든
   `heuristic` 이든 알린다. 승인 대기는 전경 프로세스가 여전히 `claude` 라
   **휴리스틱 쪽으로 판정되고**, 확정만 알리면 이 ADR 이 고치려는 사고를 정확히
   못 잡는다. 조회 축(`spawn_census` 의 respawn 후보)은 지금대로 **확정만** 센다 —
   두 축은 묻는 것이 다르다: 조회는 "이 자식을 재사용해도 되나", push 는 "부모가
   더 기다려도 소용없나".
4. **중복 알림은 정적 구간 단위로 막고, 채널을 가로지르는 dedupe 는 만들지 않는다.**
   기존 규칙을 그대로 쓴다 — 한 정적 구간에 한 번(`stall_notified`), 출력이 재개되면
   해제, 그리고 surface 당 쿨다운. 정지 알림을 보낸 뒤 훅이 뒤늦게 도착해
   `needs_input` 완료 알림이 또 가는 경우는 **허용한다**: 두 줄은 같은 사건의 중복이
   아니라 서로 다른 정보이고(하나는 "멈췄고 이유를 모른다", 다른 하나는 "입력을
   기다린다"), 두 생산자가 상태를 공유하게 만드는 비용이 그 한 줄보다 크다.
5. **시계는 둘로 가른다.** 에러가 매치된 뒤의 정지는 지금대로 짧은 정적
   (`STALL_QUIET`)으로 판정하고, **에러 없는 정지는 더 긴 정적**을 요구한다 — 보강
   증거(에러 문자열)가 없기 때문이다. 긴 쪽 값은 호스트가 자식을 조용하다고 부르기
   시작하는 값(`src/core/state/child_liveness.rs` 의 `CHILD_OUTPUT_SILENCE`)에
   맞춘다: 같은 현상을 두 모듈이 서로 다른 문턱으로 부르면 "호스트는 조용하다는데
   plugin 은 아직 아니다" 같은 상태가 생긴다.
6. **상태 축은 건드리지 않는다.** 이 경로는 `terminal.set_state` 를 부르지 않는다 —
   파생 상태는 관측 융합의 출력 전용 계약(ADR-0072)이고, 알림은 그 값을 **읽어서**
   나가는 것이지 그 값을 만들지 않는다.
7. **codex plugin 은 이 ADR 의 범위 밖이다.** codex 는 ADR-0262 이후
   `PermissionRequest` 훅으로 승인 대기를 관측하고 `needs-input` 이벤트를 선언한다 —
   이 ADR 이 고치는 사각지대(훅이 있어도 못 가는 갈래)와 다른 상태다. codex 에는
   에러 스캐너 자체가 없으므로 대응 작업은 "게이트를 넓히는 일" 이 아니라 "없는 것을
   새로 만드는 일" 이고, plugin 버전 bump 가 독립적으로 적용돼야 하므로 별도 결정으로
   다룬다.

## Consequences

- **얻은 것**: 부모가 묻지 않는 동안 멈춘 자식이 알림 한 줄로 부모에게 도달한다.
  에러 없이 멈춘 갈래(승인 프롬프트·훅 미설치·훅 유실)와 호스트가 `stale` 로 본
  갈래 둘 다 덮인다. 새 이벤트 키·새 호스트 배관 없이 기존 배선 위에서 끝난다.
- **잃은 것 (오탐)**: 에러 문자열이라는 보강 증거 없이 정적만으로 알리므로, **긴
  추론 중이라 화면이 멎은 자식**이 정지로 보고될 수 있다. 관측상 그 둘은 구별되지
  않는다(`src/core/state/child_liveness.rs` 의 "확실성의 한계" 가 이미 적은 대로). 이 대가를
  받아들이는 이유는 비대칭이다 — 오탐은 부모가 한 번 확인하고 넘어가는 비용이지만,
  미탐은 부모가 영원히 기다리는 비용이다. 크기는 결정 5 의 긴 시계와 기존 쿨다운이
  누른다.
- **운영 비용 / 유지 부담**: `terminal.state` 조회가 에러 없는 tick 에서도 일어날 수
  있다. 다만 값싼 게이트(정적 지속시간 · 중복 · 쿨다운)를 모두 통과한 tick 에서만
  부르는 규율은 그대로라, 조회 빈도는 "정적이 임계를 넘은 자식당 쿨다운마다 한 번"
  수준으로 남는다.

## Alternatives Considered

- **A: 그대로 두고 부모가 주기적으로 조회하게 한다** — 코드 변경이 0 이다. 그러나
  부모가 조회하려면 깨어 있어야 하고, 부모가 자는 이유가 바로 알림을 기다리기
  때문이다. 부모마다 폴링 루프를 심는 것은 같은 일을 자식 수만큼 반복하는 것이고,
  그 규율은 문서로만 강제된다 — 한 명이 안 하면 그 자식은 그대로 사각지대다. 기각.
- **B: 호스트가 `stale` 전이를 이벤트로 push 한다** — 판정 주체와 발신 주체가 같아져
  가장 곧다. 그러나 새 intent · 직렬화 · plugin 구독 배관이 필요하고, 파생 상태는
  매 조회마다 즉석에서 계산되는 값이라 "전이" 라는 개념 자체를 호스트가 새로 들고
  있어야 한다(지금은 아무도 이전 값을 기억하지 않는다). 사각지대 하나를 닫는 대가로
  상태 기계를 하나 새로 만드는 셈이라 기각 — 결정 1 이 그 배관 없이 같은 결과를 낸다.
- **C: plugin 폴링이 조회로 확인해 알린다** — 채택(결정 1).
- **D: 새 이벤트 키(`claude-stalled`)를 만든다** — 이름이 사실과 맞는다. 그러나
  부모는 기존 키로 훅을 걸어 두었고, 새 키를 만들면 두 키를 모두 구독하도록 등록
  배선을 이중화하거나 기존 구독자를 조용히 버려야 한다. 이름의 정확성보다 배선의
  연속성이 크다고 보아 기각 — 대신 설명과 알림 문구를 고친다(결정 2).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 호스트가 자식 상태 **전이**를 실제로 들고 있게 된다(파생 판정이 즉석 계산이 아니라
  저장된 값이 된다). 대안 B 를 기각한 근거가 "전이라는 개념이 호스트에 없다" 이므로,
  `src/core/state/child_liveness.rs` 가 이전 판정을 보관하기 시작하면 근거가 사라진다.
- codex plugin 에 출력 스캐너(정적 관측)가 생긴다. 결정 7 이 codex 를 범위 밖으로
  둔 근거가 "없는 것을 새로 만드는 일" 이므로, 그 배관이 생기면 같은 게이트 결정을
  거기에도 적용할지 다시 판단한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 오탐(긴 추론 중인 자식을 정지로 보고)이 실사용에서 성가신 수준이 된다.
  재는 법: 부모의 알림 로그(`<TASTY_PARENT_HOME>/notify/<caller_surface>.log`)에서
  정지 줄을 센 뒤, 그 시각 이후 같은 자식이 스스로 완료 알림을 냈는지 대조한다 —
  냈다면 그 정지 줄은 오탐이다. 오탐 비율이 높으면 결정 5 의 긴 시계를 더 늘리거나
  결정 3(휴리스틱 포함)을 확정 전용으로 되돌린다.
- Claude Code 가 승인 대기 중에도 화면을 계속 갱신하게 바뀐다(정적이 더 이상 정지의
  신호가 아니게 된다). 재는 법: 승인 프롬프트를 띄운 자식의 화면을 임계 시간 동안
  `tasty read screen` 으로 두 번 떠서 달라지는지 본다.

## References

- 부분 개정 후 철회: [0288](0288-codex-parent-tool-output-completion.md) (Codex 부모의 push 주체 개정) — [ADR-0291](0291-remove-the-codex-app-server-completion-channel.md) 이 그 경로를 제거해 결정 1 이 원상 복귀했다

- [ADR-0072](0072-child-state-hook-observation-fusion.md) — `stale` 의 정의와 판정
  우선순위. 이 ADR 은 그 결정을 잇고 뒤집지 않는다.
- [ADR-0262](0262-codex-approval-wait-is-observed-via-permission-request.md) —
  codex 쪽 승인 대기 관측. 결정 7 의 근거.
- [ADR-0265](0265-child-approval-policy-is-the-callers-choice.md) — 승인 정책 축.
  그 ADR 이 기본값을 "사용자 설정 그대로" 로 고른 근거가 "정지는 관측된다" 이고,
  이 ADR 이 그 전제의 마지막 구멍을 닫는다.
- [`docs/features/child-terminal/index.md`](../features/child-terminal/index.md) —
  판정 우선순위표 SoT. push 축이 그 판정을 소비한다.
- [`docs/plugins/claude/index.md`](../plugins/claude/index.md) — 이 plugin 이 발사하는
  hook 이벤트와 정지 알림 절차.
- [`docs/dev-guide/external-interaction.md`](../dev-guide/external-interaction.md#child-완료-알림--completion-log)
  — 부모가 기다리는 push 채널의 계약.
- 코드 근거(결정 시점의 현재 위치): `crates/tasty-plugin-claude/src/error_scan.rs`
  (`scan_one_at`·`should_notify_stall`·`maybe_notify_stall`·`STALLED_EVENT`),
  `crates/tasty-plugin-claude/src/hook.rs`(`Notification`/`PreToolUse` 경로),
  `src/adapters/ipc/handler/terminal.rs`(`liveness_fields`),
  `crates/tasty-plugin-agent-common/src/children.rs`(`spawn_census`).
