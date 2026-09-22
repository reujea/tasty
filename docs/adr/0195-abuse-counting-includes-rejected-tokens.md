# ADR-0195: 남용차단은 인증 실패(401)도 센다 — ADR-0046 의 남용차단 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: webhook, security, rate-limit, abuse, auth, buckets, adr-0046, adr-0112
- **Group**: webhook-hooks

## Context

웹훅에는 **통이 둘** 있고, 같은 사건이 두 통에서 반대 답을 갖는다.

- **통 A — 등록별 호출 예산**(`Lifetime` 의 `CountLimit`). 세는 단위가 "시퀀스를 돌린 횟수" 이고, 통의 키가 토큰이 아니라 **등록**이다. 여기서 401 을 세면 토큰을 모르는 발신자가 owner 의 예산을 태운다 — 2026-09-07 에 그 차감을 걷어냈다(`registry::match_request` 가 인증을 lock 안에서 차감보다 먼저 본다, 시험은 `listener::tests::unauthorized_does_not_consume_count`).
- **통 B — 출처별 실패 카운터**(`AbuseTracker`). 이쪽은 제한이라 **안 세는 것이 우회**다.

통 A 를 고치면서 통 B 의 답은 열린 채 남았다. 실측(2026-09-08):

- 통 B 의 프로덕션 진입점은 쓰기 `abuse::record_failure` 하나, 읽기 `abuse::is_source_blocked` 하나이고, 그 호출자는 각각 리스너의 `handle_request` 안 한 자리씩뿐이다(마스킹한 사본에서 전수: 나머지 히트는 전부 `#[cfg(test)]` 안).
- 그 쓰기 자리의 게이트가 6 개 ACK 상태 중 `NotFound`·`MethodNotAllowed` 만 통과시켰다. 즉 **401 이 통 B 에 도달하는 경로가 없었다.**
- 실 HTTP e2e 로 재면: 인증이 걸린 웹훅에 틀린 토큰으로 65 회(임계치 40 + 25) 요청해 65 회 전부 `401`, `429` 는 0 회.

**그 부재는 어디에도 결정으로 적혀 있지 않았다.** 적힌 곳 셋은 전부 관측이거나 빚이다 — [ADR-0112](0112-agent-stream-turn-correlation.md) 의 Consequences "남은 상류 상한"(그 자리가 스스로 "이 결정 범위 밖 … 그 다음은 리스너 자체를 고치는 몫" 이라고 말한다), [`plugins/agent-stream/index.md`](../plugins/agent-stream/index.md) 의 같은 관측, 통 A 수정의 CHANGELOG 항목. [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) Decision 의 4중 방어선 4 번은 "404/405 반복 출처를 센다" 는 **포함**의 결정이지 401 을 빼는 결정이 아니다. 시간 순서도 그것을 뒷받침한다 — 인증(401)은 남용차단보다 **6 분 먼저** 커밋됐으므로(`629453938` 22:10 → `e091c334d` 22:16) "그때 401 이 없어서 못 적었다" 도 성립하지 않는다.

## Decision

**무엇이 출처 실패인가는 `abuse::counts_as_failure` 하나가 정하고, 401 을 센다.**

`handle_request` 의 인라인 조건을 그 술어 호출로 바꾼다 — 무엇이 남용인가는 남용차단의 정책이지 리스너의 사정이 아니고, 인라인으로 두면 같은 물음의 답이 두 곳에 생긴다. 술어는 와일드카드 없는 전수 `match` 라, ACK 상태가 하나 늘면 그 자리에서 컴파일이 멈춘다.

세는 것은 `NotFound`(404) · `MethodNotAllowed`(405) · `Unauthorized`(401). 안 세는 것과 그 이유:

- `Received`(200) — 정상 트래픽. 세면 남용차단이 정상 발신자를 막는다.
- `Gone`(410) — path 를 맞춰야 나오지만 **추측할 공간이 없다**. 같은 URL 을 몇 번 두드려도 얻는 정보가 0 이므로 레이트 제한의 대상이 아니다. 401 이 대상인 이유가 정확히 그 반대다 — 시도가 비밀에 대한 정보를 준다.
- `TooManyRequests`(429) — 이미 쿨다운 중이라 매칭 전에 거부돼 이 자리에 닿지 않고, 센다면 쿨다운이 스스로 연장된다.

**이 ADR 이 개정하는 것은 [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) Decision 의 4중 방어선 4 번(남용차단)의 집계 대상 하나뿐이다.** 다음은 그대로 유효하다 — owner 신뢰 모델, 불변식 1(데이터/흐름 분리), 불변식 2(단방향 ACK), 방어선 1~3(데이터/흐름 분리 · 선택적 인증 · opaque URL), HMAC 을 안 쓰기로 한 판단, 그리고 임계치·윈도우·쿨다운 값과 그 env 오버라이드.

## Consequences

- **얻은 것**: opaque path 를 맞힌 뒤의 토큰 무차별 대입에 **처음으로** 비용이 붙는다. 통 A 가 401 을 안 태우기로 한 결정과 짝이 맞는다 — 그 결정만 있고 이쪽이 없으면 401 요청은 어느 통도 태우지 않는, 값이 0 인 요청이 된다. [ADR-0112](0112-agent-stream-turn-correlation.md) 가 Consequences 에 남겨 둔 빚 한 줄이 갚아진다(body 크기 상한은 여전히 남는다 — 그쪽은 이 결정 범위 밖이다).
- **잃은 것**: 토큰을 잘못 설정한 **정상** 발신자도 임계치를 넘기면 쿨다운(429)에 걸린다. 그 성질은 새 것이 아니다 — 경로를 잘못 쓴 정상 발신자가 404 로 걸리는 것과 같은 형태이고, 출처 키가 IP 라 NAT 뒤에서는 같은 IP 의 다른 발신자까지 함께 막히는 것도 그대로다. 임계치가 윈도우당 20 회(기본)라 정상 재시도 폭보다 충분히 위다.
- **운영 비용 / 유지 부담**: 늘어난 코드는 술어 하나다. 전수 `match` 라 새 ACK 상태를 더할 때 답을 **반드시** 적게 되고, 그 답은 `abuse::tests::every_ack_status_has_a_stated_answer` 가 전수로 고정한다.

## Alternatives Considered

- **401 을 계속 빼둔 채, 그 부재를 결정으로 승격한다**: 통 B 의 목적을 "짧은해시 keyspace 스캔 방어" 로만 좁히면 성립한다. 그러나 통 A 가 401 을 안 태우게 된 뒤로 401 요청에 붙는 비용이 **어디에도** 없어졌다 — 목적을 좁히는 순간 그 구멍이 설계가 된다. 거부.
- **401 전용 카운터를 등록 단위로 따로 둔다**: 통이 셋이 된다. 키가 등록이면 익명 발신자가 owner 의 웹훅을 잠글 수 있어, 통 A 에서 방금 걷어낸 것과 같은 형태의 결함을 다시 만든다. 거부.
- **410 도 함께 센다**: 시도가 정보를 주지 않는 자리라 레이트 제한의 대상이 아니다. 만료 URL 을 계속 두드리는 것은 낡은 발신자의 흔한 모습이기도 해서, 세면 정상 발신자를 막는 쪽에 가깝다. 범위 밖.
- **리스너의 인라인 조건에 `Unauthorized` 만 더한다**: 가장 작은 변경이지만, 다음에 상태가 늘 때 같은 빠뜨림이 그대로 재현된다 — 이번 결함의 형태가 바로 "게이트가 열거형인데 열거를 안 한 것" 이다. 거부.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **출처 키가 IP 가 아니게 되면**(리버스 프록시 뒤의 `X-Forwarded-For` 채택 등) — 공유 IP 부작용의 계산과 위조 가능성이 함께 바뀐다.
- **인증이 고정 공유 토큰이 아니라 서명(HMAC)이 되면** — 401 의 의미가 "비밀 추측" 에서 "검증 실패" 로 바뀌므로 집계 대상 판단을 다시 한다.
- **통 A 가 다시 401 을 태우게 되면** — 두 통의 답이 반대라는 이 결정의 전제가 무너진다.

## References

- 개정 대상: [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) (Decision 4중 방어선 4 — 남용차단)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [ADR-0112](0112-agent-stream-turn-correlation.md) — Consequences 에 이 부재를 빚으로 적어 둔 자리
- [`features/webhook/index.md`](../features/webhook/index.md) — 웹훅 동작 전체(요청 처리 흐름 · 남용차단)
- 코드 근거(결정이 실현된 현재 위치): `src/webhook/abuse.rs` 의 `counts_as_failure`·`record_failure`·`is_source_blocked`, `src/webhook/listener.rs` 의 `handle_request`
