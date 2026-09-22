# ADR-0548: 진입 게이트의 거절은 압력 응답에 한 덩어리로 더하고, 게이트마다 한 칸으로 가른다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: ipc, cli, telemetry, pressure, permissions, rate-limit, cap, observability, compatibility, adr-0277, adr-0333, adr-0435

## Context

모든 진입점은 공통 게이트(`check_request`, [ADR-0277](0277-ipc-admission-and-observation-run-once.md))에서
권한 → 텔레메트리 cap → rate limit 을 차례로 본다. 세 게이트가 돌려보낸 수는 **어디에서도 수로
읽히지 않았다.**

- 압력 응답(`system.pressure` · `tasty list pressure`, [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md))
  은 게이트 앞(`queue_before_gate`)과 뒤(`handler_after_gate`)를 나란히 두지만 그 차이를 빼지
  않는다 — 게이트를 지나고도 handler 덩어리에 안 잡히는 갈래가 있어 차이는 거절 수가 아니다.
- 거절은 audit log 에 Deny 로 남는다. 그러나 보존 기간이 있고([ADR-0085](0085-ipc-log-retention-bounded.md)),
  세 게이트는 사유 **문자열**로만 갈려 집계 칸이 없다.
- `RateLimit.throttled_count`(`agent.rate_limit_list` · `agent.rate_limit_status`)는 스로틀만 센다.
  그것도 버킷 하나의 수명 동안이고(재설정하면 0), 영속이라 재시작을 넘으며, 게이트 밖의 직접
  소비 거절도 섞인다. 권한 거절과 cap 차단은 그런 칸조차 없다.

그래서 "요청이 왜 안 먹었나" 를 수로 물을 수단이 없었다. 정할 것은 셋이었다 — 어디에 싣는가,
거절을 몇 칸으로 가르는가, 무엇을 모수로 두는가.

## Decision

**압력 응답에 `gate_refusals` 덩어리 하나를 더한다. 칸은 `judged`(게이트에 든 요청) · `permission_denied`
· `cap_blocked` · `throttled` 넷이다. 전부 이 프로세스가 뜬 뒤의 누계이고, 기존 덩어리의 이름 · 칸 ·
뜻은 하나도 바꾸지 않는다.**

- **압력 응답 옆에 붙인다, 따로 내지 않는다.** 이 수의 자리는 이미 있는 두 덩어리 사이다 — 게이트
  앞과 뒤를 나란히 둔 응답에서 게이트 **자신**의 판정이 빠져 있던 것이다. 같은 응답에 있어야 "큐에
  앉았던 수 · 게이트가 돌려보낸 수 · handler 가 돈 수" 가 한 번의 조회로 읽힌다. 권한(local-only
  Read) · 리셋 시점(프로세스 수명 누계) · 저장 위치(메모리 원자값)도 압력 응답과 같다. IPC 는
  `system.pressure`, CLI 는 `tasty list pressure` 로 두 면이 그대로 선다.
- **거절을 게이트마다 한 칸으로 가른다.** 처방이 셋 다 다르다 — 권한 거절은 권한을 청해야 하고(격상
  승인), cap 차단은 운영자가 cap 을 풀어야 하고, 스로틀은 기다리면 풀린다. 합치면 어느 처방이
  필요한지 다시 안 보인다. 게이트는 차례로 보고 앞에서 돌려보낸 요청은 뒤를 안 지나므로 한 요청은
  많아야 한 칸에 세진다. 칸은 게이트의 거절과 1:1 이지 처방과도, wire 코드와도 1:1 이 아니다 — 권한 게이트는
  권한 셋을 가진 호출자(plugin · agent)가 부른 없는 메서드 이름(`UnknownMethod`)과 그 호출자에게 열리지
  않은 메서드(`NotPluginCallable`)도 `-32001` 로 돌려보내므로 `permission_denied` 에는 권한을 청해도 안
  풀리는 거절이 섞인다. 두 오류는 권한 셋을 대조하는 자리에서만 나므로 Local 호출은 없는 메서드여도
  이 칸에 안 든다. 그 셋을
  가르는 것은 응답 메시지이고, 칸을 더 가르지 않은 것은 게이트의 거절과의 1:1 을 지키려는 것이다.
  게이트의 거절은 모두 `-32001` 이지만 역은 아니다 — 게이트 앞의 봉투 토큰 거절(`resolve_caller_from_envelope`
  의 형식 오류 · unknown/expired/revoked `session_token`)은 `judged` 에도 안 들고, 게이트를 지난 뒤 핸들러가
  내는 `-32001`(입력 시뮬레이션 미허용 · 손쉬운 사용 미승인 · 세션 발급 격상 거절 · plugin manager 쪽 거절)은
  통과 쪽에 든다.
- **모수 `judged` 를 함께 싣는다.** 거절 수만 있으면 "셋" 이 판정 열 건 중인지 만 건 중인지 모른다.
  모수는 `check_request` 에 든 요청 전부다 — 호출자 종류를 안 가리고(Local 도 센다), 외부 IPC 와
  plugin host-call 을 다 센다. 그래서 `queue_before_gate` 와 모수가 다르다(host-call 은 큐를 안
  지난다). `judged − (세 거절의 합)` 은 세 게이트를 모두 지난 수와 정확히 같다.
- **게이트에 맞붙은 판정 중 세지 않는 것을 이름으로 적는다.** 게이트 뒤의 봉투 검사(멱등 키 길이, [ADR-0420](0420-the-idempotency-key-envelope-is-judged-at-the-admission-gate.md))
  가 돌려보낸 요청은 거절 칸에 안 든다 — 정책이 아니라 모양이 틀린 인자다. 엔진이 없는 GUI
  부팅·종료 구간에서 Local 이 아닌 호출자를 돌려보내는 `check_without_engine` 은 `judged` 에도 안
  든다 — 그 판정은 `Core` 를 안 거친다.
- **호출자별로 가르지 않는다.** 압력 응답은 프로세스 게이지이고 호출자 축은 audit log 의 몫이다
  ([ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)).
- **원천은 `..` 없이 분해한다** — [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md)
  와 같은 이유로, 원천에 칸이 더해지면 응답 쪽이 빌드에서 멈춘다.

원칙 1 판정: 읽기 전용 관측이고 사용자 상태(포커스 · 히스토리 · 선택)에 닿지 않으며, 에이전트가 자기
요청이 왜 안 먹었는지 아는 데 쓰는 값이다 — release 에 둔다.

## Consequences

- **얻은 것**: "응답이 없다 / 안 먹었다" 가 권한 · cap · 스로틀 중 어디였는지 한 응답에서 수로 갈린다.
  압력 머리말이 "차이는 거절 수가 아니다" 로만 막아 두던 자리에 센 값이 생겼다.
- **잃은 것**: 응답이 덩어리 열둘로 커진다. 덩어리 수를 적은 자리(모듈 머리말 · CLI 도움말과 번역 ·
  API 참조 · 기능 문서 · 사용자 가이드 두 언어)를 함께 옮긴다. 호출자별 · 메서드별 거절은 여전히
  audit log 에만 있고, agent caller 는 local-only 인 이 응답을 못 읽는다(에이전트는 거절마다 받은
  오류 코드로 자기 몫을 안다).
- **운영 비용**: 요청마다 원자 증가 한 번(거절이면 두 번). 고정 크기라 자라지 않는다.

## Alternatives Considered

- **별도 메서드(`system.gate_refusals` 등)** — 두 면(IPC · CLI)을 새로 세워야 하고, 같은 모수 사슬의
  값이 두 응답으로 갈라져 한 번의 조회로 앞뒤를 못 맞춘다. 권한 · 리셋 시점이 같아 가를 이유가 없다.
- **거절 하나로 합친 칸(`refused`)** — 처방이 다른 세 사건을 한 수로 접어 "왜" 가 다시 안 보인다.
- **audit log 집계(`plugin.audit_summary`)에 게이트 축을 더하기** — 보존 기간 밖이 사라지고, 사유
  문자열을 파싱해 갈라야 한다. 저장이 목적인 층에 계수기를 얹는 것이다.
- **`RateLimit.throttled_count` 를 재사용** — 스로틀만 있고, 버킷 수명 · 영속 · 게이트 밖 소비가 섞여
  모수가 다르다.
- **호출자별 칸** — ADR-0305 의 프로세스 게이지 결정과 어긋나고 칸 수가 호출자 수만큼 자란다.

## Reconsideration Triggers

**채널이 붙는 것**

- `check_request` 에 네 번째 게이트가 더해지면 — `GateRefusal` 에 변형이 늘고 그 칸을 이 덩어리에
  더할지 정한다. `gate_refusals_json` 의 분해가 원천 칸 추가를 빌드에서 멈춘다.
- `system.pressure` 가 local-only 에서 풀리면 — 이 덩어리도 같이 풀리는지 다시 본다.

**원리적으로 안 붙는 것**

- 에이전트가 **자기** 거절 수를 물어야 하는 쓰임이 생기면(호출자별 축). 재는 법: agent caller 가 이
  응답을 못 읽어 audit log 나 자체 계수로 우회하는 사례를 이슈·세션에서 찾는다.

## References

- 선행 결정: [ADR-0277](0277-ipc-admission-and-observation-run-once.md) (게이트 순서와 단일 진입점 — 이 계수의 기록 자리)
- 선행 결정: [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) · [ADR-0435](0435-the-queue-and-retry-counts-join-the-pressure-answer-as-three-blocks.md) (덩어리를 더하는 규칙 — 같은 규칙을 따른다)
- 탐색: `git grep -l 'throttled_count\|check_rate_limit_gate\|check_permission_gate\|check_cap_gate\|check_request' -- docs/adr/`
- 코드 근거(현재 위치): `tasty_telemetry::GateStats` · `handler/checked.rs` 의 `check_request` · `handler/pressure.rs` 의 `gate_refusals_json`
