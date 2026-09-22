# ADR-0052: 강한 점유는 attach heartbeat TTL 만료도 EOF 와 동등한 해제 사유로 인정한다

- **Status**: Accepted
- **Date**: 2026-07-20
- **Tags**: occupation, hard-occupy, attach, heartbeat, ttl, disconnect, occupancy-registry, adr-0040
- **Group**: remote-attach

## Context

강한(hard) 점유(attach)는 [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) "점유 해제·수명" 절에서 "능동 해제는 명시적 해제뿐(self-release | force-detach), 시간 만료·유휴 자동 해제는 없다" 고 규정했고, 강한 점유의 실제 수명은 "연결 EOF 또는 force-detach" 로만 서술돼 있었다.

attach 스트림에 application-level heartbeat(주기적 `Ping` 프레임 + 서버측 read timeout)가 도입되면서, TCP FIN/RST 없이 조용히 끊긴 연결(무선 신호 소실·NAT 세션 타임아웃 등)도 read timeout 만료로 감지할 수 있게 됐다. 이 감지가 기존 EOF 해제 경로(`release_all_for_client`)를 그대로 태우게 하면 silent disconnect 후에도 점유 lock 이 영구히 남아있던 문제([`features/remote-attach/index.md`](../features/remote-attach/index.md) Acceptance Criteria)가 해결된다.

문제는 이 동작이 ADR-0040 의 "시간 만료·유휴 자동 해제는 없다" 는 문장과 표면적으로 상충한다는 것이다. 실제로는 다른 종류의 "시간 만료" 다 — ADR-0040 이 배제한 것은 "일정 시간 조작이 없으면 끊는다" 는 **유휴(idle) 정책**이고, heartbeat TTL 만료는 "연결이 살아있는지 확인할 방법이 응답 없음으로 소진됐다" 는 **연결 생존 판정**이다. EOF 도 이미 일종의 "연결 종료 확인"이므로, TTL 만료는 그 판정 수단이 하나 늘어난 것으로 볼 수 있다.

## Decision

강한 점유의 해제 사유를 "연결 EOF" 단독에서 **"연결 생존 판정 실패(EOF 또는 attach heartbeat TTL 만료)"** 로 확장한다. 서버는 attach 스트림의 read 가 heartbeat TTL(4 회 heartbeat interval) 동안 아무 프레임도 받지 못하면 그 연결을 EOF 와 동일하게 처리해 `release_all_for_client` 를 호출한다. 메커니즘 상세는 [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md#점유-레지스트리-occupancyregistry).

이 확장은 **강한 점유에만 적용된다** — 약한(soft) 점유의 "죽음을 항상 인지할 수 없다" 는 전제와 지연 해제(포커스 시 청소) 정책은 이 결정과 무관하게 그대로 유지된다. 또한 "임의 유휴 시간 경과에 의한 자동 해제는 없다" 는 ADR-0040 의 원칙도 유지된다 — 이번 확장은 유휴 여부와 무관하게, 연결 생존이 확인 불가능해진 시점에만 발동한다.

## Consequences

- **얻은 것**: silent disconnect 후에도 원격(피점유측) 인스턴스가 점유 lock 을 영구히 들고 있던 문제가 사라진다. 재접속·재attach 시도가 TTL 경과 후 정상적으로 성공한다.
- **잃은 것**: 강한 점유의 해제 조건이 "명시적 신호(EOF/force-detach)" 에서 "타임아웃 기반 추정(TTL 만료)" 으로 한 종류 늘었다 — 매우 느린 네트워크(4×heartbeat interval 이상 지연)에서 살아있는 연결이 오탐으로 끊길 이론적 가능성이 있다. heartbeat interval/TTL 값은 이 트레이드오프를 감안해 결정된다([`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) 참조).
- **운영 비용 / 유지 부담**: 없음 — 기존 EOF 해제 경로(`release_all_for_client`, `StreamInbound::Disconnected`)를 그대로 재사용하고 트리거 조건만 넓어졌다. 신규 상태(예: client 별 `last_seen`)를 추가하지 않았다.

## Alternatives Considered

- **A: TTL 만료를 EOF 와 별도 경로로 처리(예: `last_seen` 타임스탬프 + 별도 만료 스캔)** — read timeout 이 이미 `Err(_) => break` 로 기존 EOF 분기를 그대로 태우므로 별도 상태·스캔 로직이 불필요하다. 기각.
- **B: ADR-0040 을 새 ADR 로 완전히 Supersede** — 이번 변경은 "점유 해제·수명" 절의 강한 점유 해제 사유 한 문장만 좁게 확장하는 것이고, soft 점유 정책·시각 표현 등 ADR-0040 의 나머지 결정은 그대로 유효하다. 전체 Supersede 는 과하다고 판단해 부분 Supersede(Status 필드에 "(부분)" 명시)로 좁혔다.
- **C: ADR-0040 본문을 직접 수정** — [`template.md`](template.md) 의 "Accepted 후에는 본문을 수정하지 않는다" 규칙(References-only errata 예외 제외)에 위배된다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- heartbeat interval/TTL 값 변경이 오탐(살아있는 연결의 잘못된 해제) 빈도를 유의미하게 바꿀 때.
- 강한 점유에도 soft 점유처럼 "죽음을 항상 인지할 수 없는" 연결 방식(비-TCP transport 등)이 추가될 때 — 이 ADR 의 전제(연결 생존을 read timeout 으로 판정 가능)가 깨진다.
- TTL 기반 해제와 다른 규칙(예: 더 짧은/긴 유예, 재연결 유예 창구)이 필요해질 때.

## References

- 강한/약한 점유 2계층 모델(대상 조항: "점유 해제·수명"): [`ADR-0040`](0040-occupancy-soft-hard-tiers-agent-occupant.md)
- attach heartbeat/read-timeout 메커니즘: [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md#점유-레지스트리-occupancyregistry)
- 자동 해제 Acceptance Criteria: [`features/remote-attach/index.md`](../features/remote-attach/index.md)
